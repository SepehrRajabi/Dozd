//! `dozd headless`: the game with no window, renderer, input or assets. Time
//! advances a fixed step per frame and frames run back to back, so heists play
//! out far faster than real time. Each run is reproducible from its seed.
//!
//! The pirates are driven by simple built-in bots; this is where a scripted
//! player or a trained policy would plug in.

use std::time::{Duration, Instant, SystemTime};

use bevy::app::AppExit;
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use crate::abilities::{Active, Passive};
use crate::challenge::{ChallengeLog, Rewards};
use crate::class::{Loadout, Loadouts, PirateClass};
use crate::combat::{CrewHit, Health, Team};
use crate::deckgen::{COLS, ROWS};
use crate::enemies::{AiState, Enemy, EnemyLook};
use crate::mission::{Alarm, Fate, Settlement};
use crate::net::{HOST_ID, NetMode, PlayerInput, StartSession};
use crate::player::{Controls, CrewSize, PirateStatus, Player};
use crate::room::{DECKS, FlowField, Heist, NextSeed, RoomGrid, TILE};
use crate::vault::{VAULT_REACH, Vault};
use crate::{GamePlugins, GameState, Rng};

const USAGE: &str = "usage: dozd headless [--seed N] [--runs N] [--crew 1-8] [--class gunner|engineer|bulwark|hacker|doctor|mixed] \
                      [--passive NAME] [--active NAME] [--bot random|idle] [--time SECS] [--hz N] [--csv] [--quiet] [--trace]";
// Column docs: tools/README.md. Keep these headers in sync with that table.
const CSV_HEADER: &str = "class,run,seed,ending,extracted,left_early,dead,still_in,deck,decks,credits,payout,\
                          pool,bonus,bars_silver,bars_gold,bars_platinum,kills,alarm,seconds,pirates,\
                          ch_offered,ch_completed,ch_failed,reward_secs,pirate_kills";
/// Per-tick rows: one per pirate and one per enemy, every tick. `kind` tells
/// you which columns apply — `ai_state` is enemy-only, `move_x..dash` are
/// pirate-only, blank on the other kind.
const TRACE_HEADER: &str =
    "tick,run,seed,kind,entity,x,y,team,health,max_health,ai_state,move_x,move_y,aim_x,aim_y,fire,dash";
/// Bots only fight enemies this close.
const ENGAGE: f32 = 140.0;
/// Bots work a deck's vault for at most this many seconds, so a crew pinned
/// down beside it still moves on.
const LOOT_TIME: f32 = 80.0;
/// After this long on one deck, bots give up on the lift and make for the pad.
const DECK_TIME: f32 = 200.0;
/// Below this share of health a bot makes for the pad for good.
const HURT: f32 = 0.3;
/// The crew taking the last pad out gather within this distance of it before
/// stepping on, so nobody still on the way gets counted as staying behind.
const PAD_GATHER: f32 = 72.0;
/// Chance per bot, in a crew of two or more, to leave early by the pad of one
/// of the upper decks once its vault is done.
const LEAVE_EARLY: f32 = 0.15;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BotKind {
    /// Stands still. Guards stay idle, so runs only end on the time limit.
    Idle,
    /// Works each deck's vault (until it's empty or `LOOT_TIME` runs out), then
    /// rides the lift down together, or takes the pad on the last deck or when
    /// badly hurt; a few leave early by a pad. Shoots whatever is close and
    /// wanders a little on the way. For smoke tests and benchmarks, not a real
    /// player.
    Random,
}

#[derive(Resource, Clone, Debug)]
pub struct Config {
    /// Seed of the first run; run `i` plays `seed + i`, so any run can be replayed alone.
    pub seed: u32,
    pub runs: u32,
    pub crew: u8,
    /// Every bot's class; `None` alternates through them.
    pub class: Option<PirateClass>,
    /// Abilities for the bots whose class has them; the rest keep their defaults.
    pub passive: Option<Passive>,
    pub active: Option<Active>,
    pub bot: BotKind,
    /// Game seconds before a run is called off.
    pub time_limit: f32,
    /// Simulation steps per game second.
    pub hz: f32,
    /// Print one CSV row per run to stdout instead of the human-readable line.
    pub csv: bool,
    /// Suppress the status banner, per-run line (unless --csv) and summary.
    pub quiet: bool,
    /// Print one row per pirate/enemy per tick to stdout (see TRACE_HEADER).
    pub trace: bool,
}

impl Config {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut config = Self {
            seed: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos(),
            runs: 1,
            crew: 1,
            class: None,
            passive: None,
            active: None,
            bot: BotKind::Random,
            time_limit: 600.0,
            hz: 60.0,
            csv: false,
            quiet: false,
            trace: false,
        };
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
            let bad = |raw: &str| format!("bad value `{raw}` for {flag}\n{USAGE}");
            match flag.as_str() {
                "--seed" => {
                    let raw = value()?;
                    config.seed = raw.parse().map_err(|_| bad(raw))?;
                }
                "--runs" => {
                    let raw = value()?;
                    config.runs = raw.parse().ok().filter(|&n| n > 0).ok_or_else(|| bad(raw))?;
                }
                "--crew" => {
                    let raw = value()?;
                    config.crew = raw.parse().ok().filter(|n| (1..=8).contains(n)).ok_or_else(|| bad(raw))?;
                }
                "--class" => {
                    let raw = value()?;
                    config.class = match raw.as_str() {
                        "mixed" => None,
                        _ => Some(PirateClass::parse(raw).ok_or_else(|| bad(raw))?),
                    };
                }
                "--passive" => {
                    let raw = value()?;
                    config.passive = Some(Passive::parse(raw).ok_or_else(|| bad(raw))?);
                }
                "--active" => {
                    let raw = value()?;
                    config.active = Some(Active::parse(raw).ok_or_else(|| bad(raw))?);
                }
                "--bot" => {
                    let raw = value()?;
                    config.bot = match raw.as_str() {
                        "random" => BotKind::Random,
                        "idle" => BotKind::Idle,
                        _ => return Err(bad(raw)),
                    };
                }
                "--time" => {
                    let raw = value()?;
                    config.time_limit = raw.parse().ok().filter(|&t: &f32| t > 0.0).ok_or_else(|| bad(raw))?;
                }
                "--hz" => {
                    let raw = value()?;
                    config.hz = raw.parse().ok().filter(|&h: &f32| h >= 10.0).ok_or_else(|| bad(raw))?;
                }
                "--csv" => config.csv = true,
                "--quiet" => config.quiet = true,
                "--trace" => config.trace = true,
                _ => return Err(format!("unknown option `{flag}`\n{USAGE}")),
            }
        }
        Ok(config)
    }

    fn run_seed(&self, run: u32) -> u32 {
        self.seed.wrapping_add(run)
    }

    /// Bot `i`'s class and guns (its class's default loadout), with the
    /// abilities asked for where its class has them.
    fn loadouts(&self) -> Loadouts {
        let class = |i: usize| self.class.unwrap_or(PirateClass::ALL[i % PirateClass::ALL.len()]);
        let loadout = |i: usize| {
            let mut loadout = Loadout::new(class(i));
            if let Some(passive) = self.passive.filter(|p| p.class() == loadout.class) {
                loadout.passive = passive;
            }
            if let Some(active) = self.active.filter(|a| a.class() == loadout.class) {
                loadout.set_active(active);
            }
            loadout
        };
        Loadouts((0..self.crew as usize).map(|i| (HOST_ID + i as u64, loadout(i))).collect())
    }
}

/// Progress through the batch of runs.
#[derive(Resource, Default)]
struct Batch {
    run: u32,
    ticks: u32,
    kills: u32,
    timed_out: bool,
    started: Option<Instant>,
    total_ticks: u64,
    extracted: u32,
    /// Bars pulled this run from each tier's vault (silver, gold, platinum).
    bars: [u32; 3],
    /// Guards each pirate killed with their own weapons this run (`CrewHit`s
    /// with `killed`), by `Player::owner`.
    pirate_kills: Vec<(u64, u32)>,
    /// Pirate-seconds this run with a challenge's damage reward running.
    reward_secs: f32,
}

/// Drives the bots; reseeded every run so a seed replays exactly.
#[derive(Resource, Default)]
struct BotRng(Rng);

#[derive(Component)]
struct Bot {
    heading: Vec2,
    /// Seconds until picking a new heading.
    timer: f32,
    /// Ignoring the goal for this heading, which also shakes loose a stuck bot.
    wander: bool,
    /// The upper deck this bot leaves early on, by the pad, once its vault is done.
    leave_on: Option<u8>,
    /// Badly hurt: making for the pad for good.
    bail: bool,
}

/// Paths across the deck being played, rebuilt with each new deck.
#[derive(Default)]
struct Nav {
    vault: Option<(Vec2, FlowField)>,
    lift: Option<Spot>,
    pad: Option<Spot>,
    /// Game seconds on this deck.
    since: f32,
}

/// An area to stand on (the lift or the pad): paths to its tiles, and its middle.
struct Spot {
    tile: char,
    center: Vec2,
    field: FlowField,
}

impl Spot {
    fn find(grid: &RoomGrid, tile: char) -> Option<Self> {
        let tiles: Vec<_> = (0..ROWS)
            .flat_map(|r| (0..COLS).map(move |c| (c, r)))
            .filter(|&(c, r)| grid.tile_at(grid.tile_center(c, r)) == Some(tile))
            .collect();
        if tiles.is_empty() {
            return None;
        }
        let sum: Vec2 = tiles.iter().map(|&(c, r)| grid.tile_center(c, r)).sum();
        let center = sum / tiles.len() as f32;
        Some(Self { tile, center, field: FlowField::toward(grid, tiles) })
    }

    /// Movement toward the middle of the area; zero once there.
    fn steer(&self, grid: &RoomGrid, pos: Vec2) -> Vec2 {
        // Where the game checks whether a pirate is standing on a tile.
        let feet = pos + Vec2::new(0.0, -4.0);
        let off = self.center - pos;
        if grid.tile_at(feet) != Some(self.tile) {
            self.field.step(grid, pos).unwrap_or(off.normalize_or_zero())
        } else if off.length() > 6.0 {
            off.normalize()
        } else {
            Vec2::ZERO
        }
    }
}

pub fn run(config: Config) {
    let has_data_output = config.csv || config.trace;
    if config.quiet && !has_data_output {
        eprintln!(
            "warning: --quiet with no other output method (e.g. --csv or --trace) specified; this run will produce no output"
        );
    }
    if config.trace && config.runs > 1 {
        eprintln!(
            "warning: --trace with --runs {} will print a row per pirate/enemy per tick for every run; \
             expect a lot of output (consider --runs 1)",
            config.runs
        );
    }

    let step = Duration::from_secs_f32(1.0 / config.hz);
    // With --csv/--trace, stdout is pure data (for piping/redirecting); status goes to stderr instead.
    let banner = format!(
        "headless: {} run(s) from seed {}, crew {} ({}), {:?} bots, {} Hz, limit {}s",
        config.runs,
        config.seed,
        config.crew,
        config.class.map_or("mixed", PirateClass::name),
        config.bot,
        config.hz,
        config.time_limit
    );
    if !config.quiet {
        if has_data_output {
            eprintln!("{banner}");
        } else {
            println!("{banner}");
        }
    }
    if config.csv {
        println!("{CSV_HEADER}");
    }
    if config.trace {
        println!("{TRACE_HEADER}");
    }

    let mut app = App::new();
    app.add_plugins((
        // The default runner loops as fast as it can, with no frame pacing.
        MinimalPlugins,
        StatesPlugin,
        LogPlugin {
            level: Level::WARN,
            ..default()
        },
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(step))
    .add_plugins(GamePlugins)
    .insert_resource(CrewSize(config.crew))
    .insert_resource(config.loadouts())
    .insert_resource(config)
    .init_resource::<Batch>()
    .init_resource::<BotRng>()
    .add_observer(count_kills)
    .add_systems(Startup, start_batch)
    .add_systems(PreUpdate, drive_bots.run_if(in_state(GameState::Playing)))
    .add_systems(Update, tally_challenges.run_if(in_state(GameState::Playing)))
    .add_systems(Last, next_run);

    // One small world gains nothing from spreading systems over threads; the
    // hand-off costs more than the work. Run several processes to use more cores.
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
    app.run();
}

fn start_batch(mut commands: Commands, config: Res<Config>, mut batch: ResMut<Batch>) {
    batch.started = Some(Instant::now());
    begin_run(&mut commands, &config, 0);
    commands.trigger(StartSession(NetMode::Solo));
}

/// Seeds everything random before `OnEnter(Playing)` builds the heist.
fn begin_run(commands: &mut Commands, config: &Config, run: u32) {
    let seed = config.run_seed(run);
    commands.insert_resource(NextSeed(seed));
    commands.insert_resource(Rng::new(seed));
    commands.insert_resource(BotRng(Rng::new(seed ^ 0xB07B_07B0)));
}

fn count_kills(remove: On<Remove, Enemy>, health: Query<&Health>, mut batch: ResMut<Batch>) {
    // Guards also go when a deck or run is cleared; only count the dead ones.
    if health.get(remove.entity).is_ok_and(Health::is_dead) {
        batch.kills += 1;
    }
}

/// Counts each pirate's own kills and the time damage rewards run, for the
/// challenge columns.
fn tally_challenges(time: Res<Time>, mut hits: MessageReader<CrewHit>, mut batch: ResMut<Batch>, players: Query<&Rewards>) {
    for hit in hits.read().filter(|h| h.killed) {
        match batch.pirate_kills.iter_mut().find(|(owner, _)| *owner == hit.owner) {
            Some((_, kills)) => *kills += 1,
            None => batch.pirate_kills.push((hit.owner, 1)),
        }
    }
    let boosted = players.iter().filter(|r| r.damage() > 1.0).count();
    batch.reward_secs += boosted as f32 * time.delta_secs();
}

fn drive_bots(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<Config>,
    mut batch: ResMut<Batch>,
    mut rng: ResMut<BotRng>,
    mut pirates: Query<(Entity, &Player, &Transform, &Health, &mut Controls, Option<&mut Bot>)>,
    enemies: Query<(Entity, &Transform, &Health, &Team, &EnemyLook), With<Enemy>>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    vaults: Query<&Vault>,
    mut nav: Local<Nav>,
) {
    for vault in &vaults {
        batch.bars[vault.tier as usize] = vault.total - vault.bars;
    }

    if config.trace {
        for (entity, transform, health, team, look) in &enemies {
            trace_row(TraceRow {
                tick: batch.ticks,
                run: batch.run,
                seed: config.run_seed(batch.run),
                kind: "enemy",
                entity: entity.index().index(),
                pos: transform.translation.truncate(),
                team: *team,
                health,
                ai_state: Some(look.state),
                input: None,
            });
        }
    }

    if config.bot == BotKind::Idle {
        return;
    }
    let Some(grid) = grid else { return };
    let dt = time.delta_secs();
    if grid.is_changed() {
        *nav = Nav {
            vault: grid.vault.and_then(|v| grid.tile_of(v).map(|tile| (v, FlowField::toward(&grid, vec![tile])))),
            lift: Spot::find(&grid, 'L'),
            pad: Spot::find(&grid, '='),
            since: 0.0,
        };
    }
    nav.since += dt;
    // The whole crew stops looting at once, so they all make for the lift together.
    let looting = vaults.iter().any(|v| v.bars > 0) && nav.since < LOOT_TIME;
    let give_up_lift = nav.since >= DECK_TIME;
    // Whether everyone meaning to leave together is at the pad (or has given up waiting).
    let gathered = give_up_lift
        || nav.pad.as_ref().is_none_or(|pad| {
            pirates.iter().all(|(_, player, transform, health, _, bot)| {
                let leaving_alone = bot.is_some_and(|b| b.bail || b.leave_on == Some(heist.deck));
                player.status != PirateStatus::Active
                    || health.is_dead()
                    || leaving_alone
                    || transform.translation.truncate().distance(pad.center) <= PAD_GATHER
            })
        });

    let rng = &mut rng.0;
    for (entity, player, transform, health, mut controls, bot) in &mut pirates {
        let Some(mut bot) = bot else {
            // From the run's seed and the pirate's slot alone, so it replays exactly.
            let mut roll = Rng::new(config.run_seed(batch.run) ^ (player.slot as u32 + 1).wrapping_mul(0x5EED_1EAF));
            let leave_on = (config.crew > 1 && roll.unit() < LEAVE_EARLY)
                .then(|| ((roll.unit() * (DECKS - 1) as f32) as u8).min(DECKS - 2));
            let bot = Bot { heading: Vec2::ZERO, timer: 0.0, wander: false, leave_on, bail: false };
            commands.entity(entity).insert(bot);
            continue;
        };
        let pos = transform.translation.truncate();
        let input = 'input: {
            if player.status != PirateStatus::Active || health.is_dead() {
                break 'input PlayerInput { aim: pos, ..default() };
            }
            bot.timer -= dt;
            if bot.timer <= 0.0 {
                bot.timer = 0.4 + rng.unit() * 1.1;
                bot.heading = if rng.unit() < 0.2 {
                    Vec2::ZERO
                } else {
                    Vec2::from_angle(rng.unit() * std::f32::consts::TAU)
                };
                bot.wander = rng.unit() < 0.1;
            }
            bot.bail |= health.current < health.max * HURT;

            let target = enemies
                .iter()
                .filter(|(_, _, h, team, _)| **team == Team::Enemy && !h.is_dead())
                .map(|(_, t, ..)| t.translation.truncate())
                .filter(|e| e.distance(pos) <= ENGAGE)
                .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)));
            let to_vault = |(v, field): &(Vec2, FlowField)| {
                if v.distance(pos) <= VAULT_REACH * 0.6 {
                    Vec2::ZERO
                } else {
                    field.step(&grid, pos).unwrap_or((*v - pos).normalize_or_zero())
                }
            };
            let pad = nav.pad.as_ref();
            let lift = nav.lift.as_ref().filter(|_| !give_up_lift);
            let goal = if bot.bail {
                pad.map(|p| p.steer(&grid, pos))
            } else if looting {
                nav.vault.as_ref().map(to_vault)
            } else if bot.leave_on == Some(heist.deck) {
                pad.map(|p| p.steer(&grid, pos))
            } else if let Some(lift) = lift {
                Some(lift.steer(&grid, pos))
            } else {
                pad.map(|p| {
                    let step = p.steer(&grid, pos);
                    // Waits at the edge for the rest of the crew.
                    let feet = pos + Vec2::new(0.0, -4.0);
                    let stepping_on = grid.tile_at(feet) != Some('=') && grid.tile_at(feet + step * TILE) == Some('=');
                    if stepping_on && !gathered { Vec2::ZERO } else { step }
                })
            };
            // Fights on the move; only wanders while on the way, never off the lift or pad.
            let movement = match goal {
                Some(step) if step == Vec2::ZERO || !bot.wander || target.is_some() => step,
                _ => bot.heading,
            };
            let aim = target.unwrap_or(pos + bot.heading * 40.0);
            PlayerInput {
                movement,
                aim,
                fire: target.is_some(),
                fire_pressed: target.is_some(),
                // Never off the lift or pad: dashing standing still goes toward the aim.
                dash: movement != Vec2::ZERO && rng.unit() < 0.01,
                reload: rng.unit() < 0.002,
                slot: (rng.unit() < 0.004).then(|| (rng.unit() * 4.0) as u8 % 4),
                cycle: 0,
                interact: true,
                interact_held: true,
                // Engineers drop a sentry now and then mid-fight.
                ability: target.is_some() && rng.unit() < 0.01,
                restart: false,
            }
        };
        if config.trace {
            trace_row(TraceRow {
                tick: batch.ticks,
                run: batch.run,
                seed: config.run_seed(batch.run),
                kind: "player",
                entity: entity.index().index(),
                pos,
                team: Team::Player,
                health,
                ai_state: None,
                input: Some(&input),
            });
        }
        controls.apply(&input);
    }
}

/// One row of `--trace` output: a pirate or enemy's state/action on one tick.
struct TraceRow<'a> {
    tick: u32,
    run: u32,
    seed: u32,
    kind: &'static str,
    entity: u32,
    pos: Vec2,
    team: Team,
    health: &'a Health,
    ai_state: Option<AiState>,
    input: Option<&'a PlayerInput>,
}

fn trace_row(row: TraceRow) {
    let ai_state = row.ai_state.map_or(String::new(), |s| format!("{s:?}"));
    let (move_x, move_y, aim_x, aim_y, fire, dash) = row.input.map_or(
        (String::new(), String::new(), String::new(), String::new(), String::new(), String::new()),
        |i| {
            (
                i.movement.x.to_string(),
                i.movement.y.to_string(),
                i.aim.x.to_string(),
                i.aim.y.to_string(),
                i.fire.to_string(),
                i.dash.to_string(),
            )
        },
    );
    println!(
        "{},{},{},{},{},{},{},{:?},{},{},{ai_state},{move_x},{move_y},{aim_x},{aim_y},{fire},{dash}",
        row.tick,
        row.run + 1,
        row.seed,
        row.kind,
        row.entity,
        row.pos.x,
        row.pos.y,
        row.team,
        row.health.current,
        row.health.max,
    );
}

/// Ends each run (by the rules, or on the time limit), reports it and starts the next.
fn next_run(
    mut commands: Commands,
    config: Res<Config>,
    mut batch: ResMut<Batch>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    alarm: Res<Alarm>,
    settlement: Res<Settlement>,
    challenges: Res<ChallengeLog>,
    players: Query<(&Player, &Health)>,
    mut exit: MessageWriter<AppExit>,
) {
    match state.get() {
        GameState::Menu | GameState::Lobby => return,
        GameState::Playing => {
            batch.ticks += 1;
            if batch.ticks as f32 / config.hz >= config.time_limit {
                batch.timed_out = true;
                next_state.set(GameState::Over);
            }
            return;
        }
        GameState::Over => {}
    }

    let (mut extracted, mut left, mut dead, mut active) = (0, 0, 0, 0);
    let (mut haul, mut paid, mut deck) = (0, 0, 0);
    let mut crew: Vec<_> = players.iter().collect();
    crew.sort_by_key(|(p, _)| p.slot);
    let mut pirates = Vec::new();
    let mut pirate_kills = Vec::new();
    for (player, health) in crew {
        // Settlement (`Fate`) doesn't tell the dead from those still in when time ran out.
        let fate = match Fate::of(player) {
            Fate::Finished => 'F',
            Fate::LeftEarly => 'E',
            Fate::Lost if health.is_dead() => 'D',
            Fate::Lost => 'I',
        };
        match fate {
            'F' => extracted += 1,
            'E' => left += 1,
            'D' => dead += 1,
            _ => active += 1,
        }
        pirates.push(format!("{fate}{}:{}:{}", player.deck + 1, player.credits, player.payout));
        let own = batch.pirate_kills.iter().find(|(o, _)| *o == player.owner).map_or(0, |(_, k)| *k);
        pirate_kills.push(own.to_string());
        haul += player.credits;
        paid += player.payout;
        deck = deck.max(player.deck);
    }
    let pirates = pirates.join(";");
    let pirate_kills = pirate_kills.join(";");
    let (ch_offered, ch_completed, ch_failed) = (challenges.offered, challenges.completed, challenges.failed);
    let reward_secs = batch.reward_secs;
    let mut outcome = Vec::new();
    for (count, label) in [(extracted, "extracted"), (left, "left early"), (dead, "dead"), (active, "still in")] {
        if count > 0 {
            outcome.push(format!("{count} {label}"));
        }
    }
    let ending = if batch.timed_out { "TIMEOUT" } else { "OVER" };
    let seconds = batch.ticks as f32 / config.hz;
    let [silver, gold, platinum] = batch.bars;
    if config.csv {
        println!(
            "{},{},{},{ending},{extracted},{left},{dead},{active},{},{DECKS},{haul},{paid},{},{},\
             {silver},{gold},{platinum},{},{:.1},{seconds:.1},{pirates},\
             {ch_offered},{ch_completed},{ch_failed},{reward_secs:.1},{pirate_kills}",
            config.class.map_or("mixed", PirateClass::name),
            batch.run + 1,
            config.run_seed(batch.run),
            deck + 1,
            settlement.pool,
            settlement.bonus,
            batch.kills,
            alarm.level,
        );
    } else if !config.quiet {
        let line = format!(
            "run {:>3}  seed {:>10}  {ending:<7}  {:<34}  deck {}/{DECKS}  bars {silver}/{gold}/{platinum}  \
             carried {haul:>5} cr  paid {paid:>5} cr  kills {:>2} ({pirate_kills})  alarm {:.1}  \
             challenges {ch_completed}/{ch_offered} done, {ch_failed} failed, boost {reward_secs:.0}s  {seconds:>6.1}s",
            batch.run + 1,
            config.run_seed(batch.run),
            outcome.join(", "),
            deck + 1,
            batch.kills,
            alarm.level,
        );
        // --trace already owns stdout as a data stream (no --csv to carry this instead).
        if config.trace {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }

    batch.total_ticks += batch.ticks as u64;
    batch.extracted += (extracted > 0) as u32;
    batch.bars = [0; 3];
    batch.pirate_kills.clear();
    batch.reward_secs = 0.0;
    batch.run += 1;
    batch.ticks = 0;
    batch.kills = 0;
    batch.timed_out = false;

    if batch.run < config.runs {
        begin_run(&mut commands, &config, batch.run);
        next_state.set(GameState::Playing);
        return;
    }

    let wall = batch.started.map_or(0.0, |s| s.elapsed().as_secs_f64());
    let game = batch.total_ticks as f64 / config.hz as f64;
    let summary = format!(
        "done: {} run(s), {} with the crew out; {:.0}s of game in {:.2}s ({:.0}x real time, {:.0} ticks/s)",
        config.runs,
        batch.extracted,
        game,
        wall,
        game / wall.max(1e-9),
        batch.total_ticks as f64 / wall.max(1e-9),
    );
    if !config.quiet {
        if config.csv || config.trace {
            eprintln!("{summary}");
        } else {
            println!("{summary}");
        }
    }
    exit.write(AppExit::Success);
}
