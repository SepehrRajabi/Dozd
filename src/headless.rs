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

use crate::class::{Loadout, Loadouts, PirateClass};
use crate::combat::{Health, Team};
use crate::enemies::Enemy;
use crate::mission::Alarm;
use crate::net::{HOST_ID, NetMode, PlayerInput, StartSession};
use crate::player::{Controls, CrewSize, PirateStatus, Player};
use crate::room::{DECKS, NextSeed};
use crate::{GamePlugins, GameState, Rng};

const USAGE: &str = "usage: dozd headless [--seed N] [--runs N] [--crew 1-8] [--class gunner|engineer|bulwark|hacker|mixed] \
                      [--bot random|idle] [--time SECS] [--hz N] [--csv] [--quiet]";
// Column docs: tools/README.md. Keep that table in sync with this header.
const CSV_HEADER: &str = "class,run,seed,ending,extracted,left_behind,dead,still_in,deck,decks,credits,kills,alarm,seconds";
/// Bots only fight enemies this close.
const ENGAGE: f32 = 140.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BotKind {
    /// Stands still. Guards stay idle, so runs only end on the time limit.
    Idle,
    /// Wanders, shoots whatever is close, grabs whatever it walks past. A chaos
    /// monkey for smoke tests and benchmarks, not a real player.
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
    pub bot: BotKind,
    /// Game seconds before a run is called off.
    pub time_limit: f32,
    /// Simulation steps per game second.
    pub hz: f32,
    /// Print one CSV row per run to stdout instead of the human-readable line.
    pub csv: bool,
    /// Suppress the status banner, per-run line (unless --csv) and summary.
    pub quiet: bool,
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
            bot: BotKind::Random,
            time_limit: 600.0,
            hz: 60.0,
            csv: false,
            quiet: false,
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
                _ => return Err(format!("unknown option `{flag}`\n{USAGE}")),
            }
        }
        Ok(config)
    }

    fn run_seed(&self, run: u32) -> u32 {
        self.seed.wrapping_add(run)
    }

    /// Bot `i`'s class and guns (its class's default loadout).
    fn loadouts(&self) -> Loadouts {
        let class = |i: usize| self.class.unwrap_or(PirateClass::ALL[i % PirateClass::ALL.len()]);
        Loadouts(
            (0..self.crew as usize)
                .map(|i| (HOST_ID + i as u64, Loadout::new(class(i))))
                .collect(),
        )
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
}

/// Drives the bots; reseeded every run so a seed replays exactly.
#[derive(Resource, Default)]
struct BotRng(Rng);

#[derive(Component)]
struct Bot {
    heading: Vec2,
    /// Seconds until picking a new heading.
    timer: f32,
}

pub fn run(config: Config) {
    if config.quiet && !config.csv {
        eprintln!(
            "warning: --quiet with no other output method (e.g. --csv) specified; this run will produce no output"
        );
    }

    let step = Duration::from_secs_f32(1.0 / config.hz);
    // With --csv, stdout is pure CSV (for piping/redirecting); status goes to stderr instead.
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
        if config.csv {
            eprintln!("{banner}");
        } else {
            println!("{banner}");
        }
    }
    if config.csv {
        println!("{CSV_HEADER}");
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

fn drive_bots(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<Config>,
    mut rng: ResMut<BotRng>,
    mut pirates: Query<(Entity, &Transform, &mut Controls, Option<&mut Bot>), With<Player>>,
    enemies: Query<(&Transform, &Health, &Team), With<Enemy>>,
) {
    if config.bot == BotKind::Idle {
        return;
    }
    let rng = &mut rng.0;
    let dt = time.delta_secs();
    for (entity, transform, mut controls, bot) in &mut pirates {
        let Some(mut bot) = bot else {
            commands.entity(entity).insert(Bot { heading: Vec2::ZERO, timer: 0.0 });
            continue;
        };
        bot.timer -= dt;
        if bot.timer <= 0.0 {
            bot.timer = 0.4 + rng.unit() * 1.1;
            bot.heading = if rng.unit() < 0.2 {
                Vec2::ZERO
            } else {
                Vec2::from_angle(rng.unit() * std::f32::consts::TAU)
            };
        }

        let pos = transform.translation.truncate();
        let target = enemies
            .iter()
            .filter(|(_, h, team)| **team == Team::Enemy && !h.is_dead())
            .map(|(t, ..)| t.translation.truncate())
            .filter(|e| e.distance(pos) <= ENGAGE)
            .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)));
        let aim = target.unwrap_or(pos + bot.heading * 40.0);
        controls.apply(&PlayerInput {
            movement: bot.heading,
            aim,
            fire: target.is_some(),
            fire_pressed: target.is_some(),
            dash: rng.unit() < 0.01,
            reload: rng.unit() < 0.002,
            slot: (rng.unit() < 0.004).then(|| (rng.unit() * 4.0) as u8 % 4),
            cycle: 0,
            interact: true,
            // Engineers drop a sentry now and then mid-fight.
            ability: target.is_some() && rng.unit() < 0.01,
            restart: false,
        });
    }
}

/// Ends each run (by the rules, or on the time limit), reports it and starts the next.
fn next_run(
    mut commands: Commands,
    config: Res<Config>,
    mut batch: ResMut<Batch>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
    alarm: Res<Alarm>,
    players: Query<(&Player, &Health)>,
    mut exit: MessageWriter<AppExit>,
) {
    match state.get() {
        GameState::Menu => return,
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
    let (mut haul, mut deck) = (0, 0);
    for (player, health) in &players {
        match player.status {
            PirateStatus::Extracted => extracted += 1,
            PirateStatus::LeftBehind => left += 1,
            PirateStatus::Active if health.is_dead() => dead += 1,
            PirateStatus::Active => active += 1,
        }
        haul += player.credits;
        deck = deck.max(player.deck);
    }
    let mut outcome = Vec::new();
    for (count, label) in [(extracted, "extracted"), (left, "left behind"), (dead, "dead"), (active, "still in")] {
        if count > 0 {
            outcome.push(format!("{count} {label}"));
        }
    }
    let ending = if batch.timed_out { "TIMEOUT" } else { "OVER" };
    let seconds = batch.ticks as f32 / config.hz;
    if config.csv {
        println!(
            "{},{},{},{ending},{extracted},{left},{dead},{active},{},{DECKS},{haul},{},{:.1},{seconds:.1}",
            config.class.map_or("mixed", PirateClass::name),
            batch.run + 1,
            config.run_seed(batch.run),
            deck + 1,
            batch.kills,
            alarm.level,
        );
    } else if !config.quiet {
        println!(
            "run {:>3}  seed {:>10}  {ending:<7}  {:<24}  deck {}/{DECKS}  carried {:>5} cr  kills {:>2}  alarm {:.1}  {seconds:>6.1}s",
            batch.run + 1,
            config.run_seed(batch.run),
            outcome.join(", "),
            deck + 1,
            haul,
            batch.kills,
            alarm.level,
        );
    }

    batch.total_ticks += batch.ticks as u64;
    batch.extracted += (extracted > 0) as u32;
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
        "done: {} run(s), {} with an extraction; {:.0}s of game in {:.2}s ({:.0}x real time, {:.0} ticks/s)",
        config.runs,
        batch.extracted,
        game,
        wall,
        game / wall.max(1e-9),
        batch.total_ticks as f64 / wall.max(1e-9),
    );
    if !config.quiet {
        if config.csv {
            eprintln!("{summary}");
        } else {
            println!("{summary}");
        }
    }
    exit.write(AppExit::Success);
}
