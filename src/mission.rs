use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::enemies::{EnemyLook, roll_reinforcement, spawn_enemy};
use crate::hud::{self, HudSlot, Meter};
use crate::loot::LootKind;
use crate::net::{LocalPlayer, NetMode, authority};
use crate::player::{DASH_COOLDOWN, Dash, OFF_DECK, PirateStatus, Player};
use crate::room::{DECKS, Heist, RoomGrid};
use crate::settings::Settings;
use crate::vault::VaultTier;
use crate::{GameState, Rng};

pub const MAX_ALARM: f32 = 5.0;
/// Once raised, the alarm keeps climbing on its own: lingering is dangerous.
const ALARM_CREEP: f32 = 0.015;
/// Seconds of holding the pad needed to extract.
const EXTRACT_TIME: f32 = 15.0;
/// Seconds aboard before the lift goes down.
const LIFT_TIME: f32 = 6.0;
/// Alarm levels shed on reaching a new deck: its security hasn't caught up yet.
const DECK_ALARM_RELIEF: f32 = 1.0;
/// Guards alive at once on the top deck, for a solo pirate.
const MAX_ALIVE: usize = 14;
/// Each deck down allows this many more guards alive at once, in step with the
/// guards each deck starts with, so deep waves aren't swallowed by the cap.
const DECK_ALIVE: usize = 10;
/// Each deck down brings the waves this many seconds closer together.
const DECK_WAVE_HASTE: f32 = 1.5;
/// How much deeper decks weigh on what reinforcements are sent, in alarm levels.
const DECK_THREAT: f32 = 0.75;
/// Alarm raised by each gold bar pulled from the vault.
const BAR_ALARM: f32 = 0.04;
/// Reinforcements won't pop out of a spawn point closer than this to any pirate.
const SPAWN_CLEARANCE: f32 = 70.0;
/// Each pirate who leaves early grows the final group's pool by this share of it.
pub const CREW_BONUS_PER_LEAVER: f64 = 0.25;

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Alarm>()
            .init_resource::<Extraction>()
            .init_resource::<Lift>()
            .init_resource::<Banner>()
            .init_resource::<Settlement>()
            .add_systems(
                OnEnter(GameState::Playing),
                (spawn_status, reset_mission).run_if(authority),
            )
            .add_systems(OnEnter(GameState::Over), settle_run.run_if(authority))
            .add_systems(OnEnter(GameState::Lobby), spawn_status.run_if(authority))
            .add_systems(
                Update,
                (announce_alarm, run_waves, ride_lift, extraction, check_crew_wiped)
                    .run_if(in_state(GameState::Playing))
                    .run_if(authority),
            )
            .add_systems(
                Update,
                (
                    tick_banner.run_if(authority),
                    publish_status.run_if(authority),
                    follow_host_state.run_if(not(authority)),
                )
                    .chain(),
            );
    }
}

pub struct MissionViewPlugin;

impl Plugin for MissionViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_mission_hud)
            .add_systems(OnEnter(GameState::Over), spawn_results)
            .add_systems(
                Update,
                (
                    update_player_bars,
                    update_alarm_hud,
                    update_banner,
                    update_extraction_hud,
                    update_got_out,
                    update_results.run_if(in_state(GameState::Over)),
                )
                    .chain()
                    .after(follow_host_state),
            );
    }
}

#[derive(Resource, Default)]
pub struct Alarm {
    pub level: f32,
    wave_timer: f32,
    /// Extra enemies added to the next wave.
    surge: usize,
    announced: u32,
    /// The deck the crew is on; deeper decks answer harder.
    deck: u8,
}

impl Alarm {
    fn raise(&mut self, amount: f32) {
        if self.level < 1.0 && self.level + amount >= 1.0 {
            // Grace period before the first wave arrives.
            self.wave_timer = 8.0;
        }
        self.level = (self.level + amount).min(MAX_ALARM);
    }

    pub fn loot_taken(&mut self, kind: LootKind) {
        self.raise(kind.value() as f32 / 700.0);
    }

    /// Gunfire or being spotted puts the station on alert.
    pub fn alert(&mut self) {
        if self.level < 1.0 {
            self.raise(1.0 - self.level);
        }
    }

    /// Every guard on the station is hunting you.
    pub fn lockdown(&self) -> bool {
        self.level >= 2.0
    }

    /// The first bar out of the vault: lockdown, and an immediate, bigger wave.
    pub(crate) fn vault_breached(&mut self) {
        self.raise((2.0 - self.level).max(0.5));
        // The breach banner says it all; skip the alarm level's own.
        self.announced = self.level.floor() as u32;
        self.surge += 2;
        self.wave_timer = 0.0;
    }

    pub(crate) fn bar_taken(&mut self) {
        self.raise(BAR_ALARM);
    }

    fn wave_interval(&self) -> f32 {
        (18.0 - 2.5 * self.level - DECK_WAVE_HASTE * self.deck as f32).max(5.0)
    }

    /// The alarm level reinforcements are picked for: deeper decks send worse.
    fn threat(&self) -> f32 {
        self.level + DECK_THREAT * self.deck as f32
    }

    /// Guards alive at once before waves hold back.
    fn max_alive(&self, crew: usize) -> usize {
        MAX_ALIVE + DECK_ALIVE * self.deck as usize + 4 * crew.saturating_sub(1)
    }

    /// Arriving on a fresh deck: the alarm carries over, minus a little relief.
    fn new_deck(&mut self, deck: u8) {
        self.level = (self.level - DECK_ALARM_RELIEF).max(0.0);
        self.announced = self.level.floor() as u32;
        self.wave_timer = 8.0;
        self.surge = 0;
        self.deck = deck;
    }

    /// Guards per wave, in proportion to the crew that set out: each pirate
    /// brings a solo run's worth, plus one more per deck down.
    fn wave_size(&self, crew: usize) -> usize {
        let base = 1 + (self.level * 0.8).floor() as usize;
        base * crew.max(1) + self.deck as usize
    }
}

#[derive(Resource, Default)]
struct Extraction {
    progress: f32,
    started: bool,
    on_pad: bool,
}

#[derive(Resource, Default)]
struct Lift {
    progress: f32,
    /// The whole crew still in the heist is aboard, so it's filling.
    aboard: bool,
    /// Living pirates in the heist standing on the lift, and how many there are.
    count: u8,
    crew: u8,
}

/// How a pirate's run ended, as far as the money goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fate {
    /// Took a pad out while the rest of the crew stayed in.
    LeftEarly,
    /// Got out by the pad that ended the run.
    Finished,
    /// Dead, or still in the heist when it ended.
    Lost,
}

impl Fate {
    pub fn of(player: &Player) -> Self {
        match (player.status, player.left_early) {
            (PirateStatus::Extracted, true) => Self::LeftEarly,
            (PirateStatus::Extracted, false) => Self::Finished,
            _ => Self::Lost,
        }
    }
}

/// Host only: how the finished run's loot was split, for `MissionStatus`.
#[derive(Resource, Default, Clone, Copy, PartialEq, Debug)]
pub struct Settlement {
    pub crew_out: bool,
    pub pool: u32,
    pub bonus: u32,
}

/// Splits a finished run's loot. `crew` is each pirate's own credits and fate;
/// returns the split and each pirate's payout, in the same order. Early leavers
/// keep their own haul. If the final group got out, they pool theirs, grow it by
/// `CREW_BONUS_PER_LEAVER` of itself per early leaver, and share it evenly.
/// Everyone else gets nothing.
pub fn settle(crew: &[(u32, Fate)]) -> (Settlement, Vec<u32>) {
    let finishers = crew.iter().filter(|(_, f)| *f == Fate::Finished).count();
    let leavers = crew.iter().filter(|(_, f)| *f == Fate::LeftEarly).count();
    let pool: u32 = crew.iter().filter(|(_, f)| *f == Fate::Finished).map(|(c, _)| c).sum();
    let bonus = (pool as f64 * CREW_BONUS_PER_LEAVER * leavers as f64).round() as u32;
    let share = if finishers > 0 { ((pool + bonus) as f64 / finishers as f64).round() as u32 } else { 0 };
    let payouts = crew
        .iter()
        .map(|&(credits, fate)| match fate {
            Fate::LeftEarly => credits,
            Fate::Finished => share,
            Fate::Lost => 0,
        })
        .collect();
    let crew_out = finishers > 0;
    let settlement = Settlement {
        crew_out,
        pool: if crew_out { pool } else { 0 },
        bonus: if crew_out { bonus } else { 0 },
    };
    (settlement, payouts)
}

#[derive(Resource, Default)]
pub(crate) struct Banner {
    text: String,
    color: Color,
    timer: f32,
}

impl Banner {
    pub(crate) fn show(&mut self, text: impl Into<String>, color: Color, secs: f32) {
        self.text = text.into();
        self.color = color;
        self.timer = secs;
    }
}

/// The mission state every machine needs to draw the HUD, replicated from the host.
#[derive(Component, Serialize, Deserialize, Clone, PartialEq)]
pub struct MissionStatus {
    phase: GameState,
    /// Which heist layout, so clients build the same decks.
    pub seed: u32,
    pub deck: u8,
    alarm: f32,
    /// The host's reinforcements setting, so every HUD tells the same story.
    reinforcements: bool,
    next_wave: f32,
    extraction: f32,
    on_pad: bool,
    lift: f32,
    on_lift: bool,
    /// Living pirates still in the heist standing on the lift.
    pub aboard: u8,
    /// Living pirates still in the heist; the lift only fills with all of them aboard.
    pub crew_in: u8,
    /// Pirates who have left early by a pad so far this run.
    pub leavers: u8,
    /// Once the run is over: the final group got out by a pad.
    pub crew_out: bool,
    /// Once the run is over: the final group's pooled haul (0 unless `crew_out`).
    pub pool: u32,
    /// Once the run is over: what the early leavers added to the pool (0 unless `crew_out`).
    pub bonus: u32,
    banner: String,
    banner_color: Color,
    banner_alpha: f32,
}

#[derive(Component)]
struct HullBar;

#[derive(Component)]
struct HullValue;

#[derive(Component)]
struct ShieldBar;

#[derive(Component)]
struct ShieldValue;

#[derive(Component)]
struct DashBar;

#[derive(Component)]
struct DeckText;

/// One step of the alarm meter, filling as the alarm passes through its level.
#[derive(Component)]
struct AlarmPip(usize);

#[derive(Component)]
struct AlarmText;

#[derive(Component)]
struct WaveText;

#[derive(Component)]
struct BannerPanel;

#[derive(Component)]
struct BannerText;

#[derive(Component)]
struct ExtractionPanel;

#[derive(Component)]
struct ExtractionBar;

#[derive(Component)]
struct ExtractionText;

/// Shown to a pirate who left early while the crew plays on.
#[derive(Component)]
struct GotOutPanel;

#[derive(Component)]
struct GotOutText;

/// The parts of the results screen that follow the settlement as it arrives.
#[derive(Component)]
struct ResultsPanel;

#[derive(Component)]
struct ResultsTitle;

#[derive(Component)]
struct ResultsDetail;

#[derive(Component)]
struct ResultsBreakdown;

/// Created once per session; it persists across restarts so clients keep following it.
fn spawn_status(mut commands: Commands, existing: Query<(), With<MissionStatus>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        Replicated,
        MissionStatus {
            phase: GameState::Playing,
            seed: 0,
            deck: 0,
            alarm: 0.0,
            reinforcements: false,
            next_wave: 0.0,
            extraction: 0.0,
            on_pad: false,
            lift: 0.0,
            on_lift: false,
            aboard: 0,
            crew_in: 0,
            leavers: 0,
            crew_out: false,
            pool: 0,
            bonus: 0,
            banner: String::new(),
            banner_color: Color::WHITE,
            banner_alpha: 0.0,
        },
    ));
}

fn reset_mission(
    mut alarm: ResMut<Alarm>,
    mut extraction: ResMut<Extraction>,
    mut lift: ResMut<Lift>,
    mut banner: ResMut<Banner>,
    mut settlement: ResMut<Settlement>,
    mode: Res<NetMode>,
) {
    *alarm = Alarm::default();
    *extraction = Extraction::default();
    *lift = Lift::default();
    *settlement = Settlement::default();
    let text = match *mode {
        NetMode::Solo => "Crack each deck's vault, ride the lift down to richer ones, or hold a pad to get out",
        _ => "The lift waits for the whole crew. Leave by a pad early and keep only your own haul; \
              the crew who finish split theirs, +25% per leaver",
    };
    banner.show(text, Color::srgb(0.85, 0.9, 1.0), 6.0);
}

fn tick_banner(time: Res<Time>, mut banner: ResMut<Banner>) {
    banner.timer = (banner.timer - time.delta_secs()).max(0.0);
}

/// Copies host-only mission state into the replicated status, only when it changes.
fn publish_status(
    state: Res<State<GameState>>,
    heist: Res<Heist>,
    alarm: Res<Alarm>,
    extraction: Res<Extraction>,
    lift: Res<Lift>,
    banner: Res<Banner>,
    settings: Res<Settings>,
    settlement: Res<Settlement>,
    players: Query<&Player>,
    mut status: Single<&mut MissionStatus>,
) {
    status.set_if_neq(MissionStatus {
        phase: *state.get(),
        seed: heist.seed,
        deck: heist.deck,
        alarm: alarm.level,
        reinforcements: settings.reinforcements,
        // Whole seconds are all the HUD shows; avoids replicating every frame.
        next_wave: alarm.wave_timer.max(0.0).ceil(),
        extraction: (extraction.progress * 10.0).round() / 10.0,
        on_pad: extraction.on_pad,
        lift: (lift.progress * 10.0).round() / 10.0,
        on_lift: lift.aboard,
        aboard: lift.count,
        crew_in: lift.crew,
        leavers: players.iter().filter(|p| p.left_early).count() as u8,
        crew_out: settlement.crew_out,
        pool: settlement.pool,
        bonus: settlement.bonus,
        banner: banner.text.clone(),
        banner_color: banner.color,
        banner_alpha: (banner.timer.min(1.0) * 20.0).round() / 20.0,
    });
}

/// Clients mirror the host's run state (playing vs. results screen).
fn follow_host_state(
    status: Option<Single<&MissionStatus>>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if let Some(status) = status
        && status.phase != *state.get()
    {
        next_state.set(status.phase);
    }
}

fn announce_alarm(mut alarm: ResMut<Alarm>, mut banner: ResMut<Banner>) {
    let level = alarm.level.floor() as u32;
    if level <= alarm.announced {
        return;
    }
    alarm.announced = level;
    let text = match level {
        1 => "ALARM 1  -  security alerted",
        2 => "ALARM 2  -  LOCKDOWN: every guard is hunting you",
        3 => "ALARM 3  -  reinforcements inbound",
        4 => "ALARM 4  -  heavy response deployed",
        _ => "ALARM 5  -  STATION PURGE",
    };
    banner.show(text, alarm_color(alarm.level), 3.5);
}

fn run_waves(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut alarm: ResMut<Alarm>,
    settings: Res<Settings>,
    players: Query<(&Transform, &Health, &Player)>,
    enemies: Query<(), With<EnemyLook>>,
) {
    if alarm.level < 1.0 {
        return;
    }
    let dt = time.delta_secs();
    alarm.level = (alarm.level + ALARM_CREEP * dt).min(MAX_ALARM);
    if !settings.reinforcements {
        return;
    }
    alarm.wave_timer -= dt;
    if alarm.wave_timer > 0.0 {
        return;
    }
    alarm.wave_timer = alarm.wave_interval();

    let crew: Vec<Vec2> = players
        .iter()
        .filter(|(_, h, p)| !h.is_dead() && p.status == PirateStatus::Active)
        .map(|(t, _, _)| t.translation.truncate())
        .collect();
    let mut points: Vec<Vec2> = grid
        .spawn_points
        .iter()
        .copied()
        .filter(|p| crew.iter().all(|c| p.distance(*c) > SPAWN_CLEARANCE))
        .collect();
    if points.is_empty() {
        points = grid.spawn_points.clone();
    }

    // Everyone who set out from the lobby, fallen or gone: the station sizes
    // its response to the crew it saw come aboard.
    let aboard = players.iter().count();
    let room = alarm.max_alive(aboard).saturating_sub(enemies.iter().count());
    let wave = alarm.wave_size(aboard);
    let count = (wave + alarm.surge).min(room);
    // A surge with no room yet (a breach on a full deck) waits for the next wave.
    alarm.surge -= alarm.surge.min(count.saturating_sub(wave));
    for i in 0..count {
        let point = points[(i + (rng.unit() * points.len() as f32) as usize) % points.len()];
        let pos = point + Vec2::new(rng.signed(), rng.signed()) * 3.0;
        let kind = roll_reinforcement(&mut rng, alarm.threat());
        spawn_enemy(&mut commands, kind, pos, true);
        fx(&mut commands, FxKind::Burst, pos, 0.0, Color::srgb(1.0, 0.3, 0.2));
    }
}

fn standing_on(grid: &RoomGrid, transform: &Transform, tile: char) -> bool {
    let feet = transform.translation.truncate() + Vec2::new(0.0, -4.0);
    grid.tile_at(feet) == Some(tile)
}

fn on_pad(grid: &RoomGrid, transform: &Transform) -> bool {
    standing_on(grid, transform, '=')
}

/// The lift fills only while every living pirate still in the heist is aboard,
/// so nobody gets left behind; the fallen don't hold it up. When it goes, the
/// whole crew rides down to the next deck.
fn ride_lift(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut heist: ResMut<Heist>,
    mut alarm: ResMut<Alarm>,
    mut lift: ResMut<Lift>,
    mut extraction: ResMut<Extraction>,
    mut banner: ResMut<Banner>,
    mut players: Query<(&mut Player, &Health, &Transform)>,
) {
    let active = |p: &Player, h: &Health| p.status == PirateStatus::Active && !h.is_dead();
    let was_aboard = lift.aboard;
    let crew = players.iter().filter(|(p, h, _)| active(p, h)).count() as u8;
    let count = players
        .iter()
        .filter(|(p, h, t)| active(p, h) && standing_on(&grid, t, 'L'))
        .count() as u8;
    if lift.crew != crew || lift.count != count {
        lift.crew = crew;
        lift.count = count;
    }
    lift.aboard = crew > 0 && count == crew;
    if lift.aboard && !was_aboard && lift.progress == 0.0 {
        banner.show("LIFT ARMED  -  the whole crew rides down together", Color::srgb(0.5, 0.9, 1.0), 3.0);
    }

    let dt = time.delta_secs();
    lift.progress = if lift.aboard {
        lift.progress + dt
    } else {
        (lift.progress - 0.5 * dt).max(0.0)
    };
    if lift.progress < LIFT_TIME {
        return;
    }

    heist.deck += 1;
    for (mut player, health, _) in &mut players {
        if active(&player, health) {
            player.deck = heist.deck;
        }
    }
    *lift = Lift::default();
    *extraction = Extraction::default();
    alarm.new_deck(heist.deck);
    let tier = VaultTier::for_deck(heist.deck).name();
    let text = if heist.deck + 1 == DECKS {
        format!("DECK {DECKS}/{DECKS}  -  {tier} VAULT. No lift down from here: crack it, then find a pad")
    } else {
        format!("DECK {}/{DECKS}  -  a {tier} vault, tougher guards", heist.deck + 1)
    };
    banner.show(text, Color::srgb(0.5, 0.9, 1.0), 4.0);
}

/// Extraction fills while any living pirate holds the pad. When it completes,
/// the pirates on it get out. If anyone in the heist is still off the pad, those
/// who went left early and the run goes on; otherwise the crew is out and it ends.
fn extraction(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut alarm: ResMut<Alarm>,
    mut extraction: ResMut<Extraction>,
    mut banner: ResMut<Banner>,
    mut next_state: ResMut<NextState<GameState>>,
    mut players: Query<(&mut Player, &Health, &mut Transform)>,
) {
    let active = |p: &Player, h: &Health| p.status == PirateStatus::Active && !h.is_dead();
    extraction.on_pad = players.iter().any(|(p, h, t)| active(p, h) && on_pad(&grid, t));

    if extraction.on_pad && !extraction.started {
        extraction.started = true;
        alarm.raise(1.5);
        alarm.surge += 2;
        // Start the defense with an immediate wave.
        alarm.wave_timer = 0.0;
        banner.show("EXTRACTION STARTED  -  HOLD THE PAD", Color::srgb(1.0, 0.85, 0.3), 3.5);
    }

    let dt = time.delta_secs();
    extraction.progress = if extraction.on_pad {
        extraction.progress + dt
    } else {
        (extraction.progress - 0.25 * dt).max(0.0)
    };
    if extraction.progress < EXTRACT_TIME {
        return;
    }

    let staying = players.iter().filter(|(p, h, t)| active(p, h) && !on_pad(&grid, t)).count();
    let mut leaving = 0;
    for (mut player, health, mut transform) in &mut players {
        if !active(&player, health) || !on_pad(&grid, &transform) {
            continue;
        }
        player.status = PirateStatus::Extracted;
        leaving += 1;
        if staying > 0 {
            player.left_early = true;
            // Out of play: off the map, out of reach of stray shots and blasts.
            transform.translation = OFF_DECK.extend(transform.translation.z);
        }
    }
    if staying == 0 {
        next_state.set(GameState::Over);
        return;
    }
    // The next pad anyone starts alerts the station all over again.
    *extraction = Extraction::default();
    let who = if leaving == 1 { "A PIRATE LEFT EARLY".to_string() } else { format!("{leaving} PIRATES LEFT EARLY") };
    banner.show(
        format!("{who}  -  they keep their own haul; the crew's split grows"),
        Color::srgb(1.0, 0.85, 0.3),
        4.0,
    );
}

fn check_crew_wiped(mut next_state: ResMut<NextState<GameState>>, players: Query<(&Health, &Player)>) {
    let anyone_left = players
        .iter()
        .any(|(h, p)| !h.is_dead() && p.status == PirateStatus::Active);
    if !players.is_empty() && !anyone_left {
        next_state.set(GameState::Over);
    }
}

/// Splits the loot once the run is over, however it ended (see `settle`).
fn settle_run(mut settlement: ResMut<Settlement>, mut players: Query<&mut Player>) {
    let crew: Vec<(u32, Fate)> = players.iter().map(|p| (p.credits, Fate::of(p))).collect();
    let (split, payouts) = settle(&crew);
    *settlement = split;
    // Nothing is added or removed between the two passes, so the order holds.
    for (mut player, payout) in players.iter_mut().zip(payouts) {
        player.payout = payout;
    }
}

fn alarm_color(level: f32) -> Color {
    let t = (level / MAX_ALARM).clamp(0.0, 1.0);
    Color::srgb(1.0, 0.85 - 0.6 * t, 0.3 - 0.15 * t)
}

const SHIELD_COLOR: Color = Color::srgb(0.4, 0.72, 1.0);
const HULL_COLOR: Color = Color::srgb(0.35, 0.9, 0.45);
const DASH_COLOR: Color = Color::srgb(0.75, 0.92, 1.0);

/// A labelled meter with a number after it.
fn stat_row(panel: &mut ChildSpawnerCommands, label: &str, color: Color, bar: impl Component, value: impl Bundle) {
    panel
        .spawn(Node {
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((hud::text(label, 12.0, hud::LABEL), Node {
                width: Val::Px(hud::ROW_LABEL),
                ..default()
            }));
            row.spawn((bar, hud::meter(hud::ROW_METER, 10.0, color)));
            row.spawn(value);
        });
}

fn spawn_mission_hud(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Vitals)).with_children(|panel| {
        stat_row(panel, "SHLD", SHIELD_COLOR, ShieldBar, (ShieldValue, hud::text("", 12.0, hud::TEXT)));
        stat_row(panel, "HULL", HULL_COLOR, HullBar, (HullValue, hud::text("", 12.0, hud::TEXT)));
        stat_row(panel, "DASH", DASH_COLOR, DashBar, ());
    });

    // Deck, alarm meter and what security is up to.
    commands.entity(hud::slot(&slots, HudSlot::Status)).with_children(|slot| {
        slot.spawn(hud::panel(Node {
            column_gap: Val::Px(12.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
            ..default()
        }))
        .with_children(|panel| {
            panel.spawn((DeckText, hud::text("", 15.0, hud::TEXT)));
            panel.spawn((
                Node {
                    width: Val::Px(1.0),
                    height: Val::Px(16.0),
                    ..default()
                },
                BackgroundColor(hud::EDGE),
            ));
            panel.spawn(hud::text("ALARM", 12.0, hud::LABEL));
            panel
                .spawn(Node {
                    column_gap: Val::Px(3.0),
                    ..default()
                })
                .with_children(|pips| {
                    for i in 0..MAX_ALARM as usize {
                        pips.spawn((AlarmPip(i), hud::meter(18.0, 9.0, alarm_color(i as f32 + 1.0))));
                    }
                });
            panel.spawn((AlarmText, hud::text("", 15.0, hud::TEXT)));
            panel.spawn((WaveText, hud::text("", 12.0, hud::DIM)));
        });
    });

    commands.entity(hud::slot(&slots, HudSlot::Banner)).with_child((
        BannerPanel,
        Node {
            max_width: Val::Px(560.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(6.0)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::NONE),
        children![(BannerText, hud::text("", 18.0, Color::WHITE), TextLayout::justify(Justify::Center))],
    ));

    // Extraction or lift progress.
    commands.entity(hud::slot(&slots, HudSlot::Progress)).with_child((
        ExtractionPanel,
        hud::panel(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
            display: Display::None,
            ..default()
        }),
        children![
            (ExtractionText, hud::text("", 16.0, hud::GOLD)),
            (ExtractionBar, hud::meter(340.0, 10.0, hud::GOLD)),
        ],
    ));

    commands.entity(hud::slot(&slots, HudSlot::Prompt)).with_child((
        GotOutPanel,
        hud::panel_accent(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                display: Display::None,
                ..default()
            },
            hud::GOLD,
        ),
        children![
            (GotOutText, hud::text("", 20.0, hud::GOLD)),
            hud::text("It's yours whatever happens. Watching the crew...", 14.0, hud::DIM),
        ],
    ));
}

fn update_player_bars(
    player: Option<Single<(&Health, &Dash), With<LocalPlayer>>>,
    mut hull: Single<&mut Meter, With<HullBar>>,
    mut shield: Single<&mut Meter, (With<ShieldBar>, Without<HullBar>)>,
    mut dash_bar: Single<&mut Meter, (With<DashBar>, Without<HullBar>, Without<ShieldBar>)>,
    mut hull_value: Single<&mut Text, (With<HullValue>, Without<ShieldValue>)>,
    mut shield_value: Single<&mut Text, With<ShieldValue>>,
) {
    let Some(player) = player else { return };
    let (health, dash) = *player;

    let frac = (health.current / health.max).clamp(0.0, 1.0);
    hull.frac = frac;
    hull.color = if frac > 0.5 {
        HULL_COLOR
    } else if frac > 0.25 {
        Color::srgb(1.0, 0.75, 0.25)
    } else {
        Color::srgb(1.0, 0.3, 0.3)
    };
    shield.frac = (health.shield / health.max_shield).clamp(0.0, 1.0);
    hull_value.set_if_neq(Text(format!("{:.0}", health.current.max(0.0).ceil())));
    shield_value.set_if_neq(Text(format!("{:.0}", health.shield.max(0.0).ceil())));

    let ready = 1.0 - (dash.cooldown / DASH_COOLDOWN).clamp(0.0, 1.0);
    dash_bar.frac = ready;
    dash_bar.color = if ready >= 1.0 { DASH_COLOR } else { hud::SPENT };
}

fn update_alarm_hud(
    status: Option<Single<&MissionStatus>>,
    local: Option<Single<&Player, With<LocalPlayer>>>,
    mut deck: Single<&mut Text, (With<DeckText>, Without<AlarmText>, Without<WaveText>)>,
    mut pips: Query<(&AlarmPip, &mut Meter)>,
    alarm_text: Single<(&mut Text, &mut TextColor), (With<AlarmText>, Without<WaveText>)>,
    mut wave: Single<&mut Text, With<WaveText>>,
) {
    let Some(status) = status else { return };
    for (pip, mut meter) in &mut pips {
        meter.frac = status.alarm - pip.0 as f32;
    }

    let left_early = local.is_some_and(|p| p.left_early);
    deck.set_if_neq(Text(format!("DECK {}/{DECKS}", status.deck + 1)));
    let (content, tint) = if left_early && status.phase == GameState::Playing {
        ("GOT OUT", hud::GOLD)
    } else if status.alarm <= 0.0 {
        ("UNDETECTED", Color::srgba(0.7, 0.9, 1.0, 0.8))
    } else if status.alarm < 2.0 {
        ("ALERT", alarm_color(status.alarm))
    } else {
        ("LOCKDOWN", alarm_color(status.alarm))
    };
    let (mut text, mut color) = alarm_text.into_inner();
    if text.0 != content {
        text.0 = content.into();
    }
    color.0 = tint;

    let waves = if left_early {
        "watching the crew".into()
    } else if status.alarm <= 0.0 {
        String::new()
    } else if status.reinforcements {
        format!("next wave {:.0}s", status.next_wave)
    } else {
        "no reinforcements".into()
    };
    if wave.0 != waves {
        wave.0 = waves;
    }
}

fn update_banner(
    status: Option<Single<&MissionStatus>>,
    panel: Single<(&mut Node, &mut BackgroundColor), With<BannerPanel>>,
    text: Single<(&mut Text, &mut TextColor), With<BannerText>>,
) {
    let Some(status) = status else { return };
    let (mut node, mut background) = panel.into_inner();
    let display = if status.banner.is_empty() || status.banner_alpha <= 0.0 {
        Display::None
    } else {
        Display::Flex
    };
    if node.display != display {
        node.display = display;
    }
    background.0 = hud::PANEL.with_alpha(0.7 * status.banner_alpha);
    let (mut text, mut color) = text.into_inner();
    if text.0 != status.banner {
        text.0.clone_from(&status.banner);
    }
    color.0 = status.banner_color.with_alpha(status.banner_alpha);
}

fn update_extraction_hud(
    mode: Res<NetMode>,
    status: Option<Single<&MissionStatus>>,
    mut panel: Single<&mut Node, With<ExtractionPanel>>,
    mut bar: Single<&mut Meter, With<ExtractionBar>>,
    text: Single<(&mut Text, &mut TextColor), With<ExtractionText>>,
) {
    let Some(status) = status else { return };
    let paused = Color::srgb(1.0, 0.55, 0.4);
    // Extraction matters more, so it wins if both are running.
    let content = if status.extraction > 0.0 {
        let frac = (status.extraction / EXTRACT_TIME).clamp(0.0, 1.0);
        let pct = frac * 100.0;
        // Online, only those on the pad at 100% go; everyone else plays on.
        let who = if *mode == NetMode::Solo { "be on the pad when it hits 100" } else { "only pirates on the pad leave" };
        Some((frac, if status.on_pad {
            (format!("EXTRACTING  {pct:.0}%  -  {who}"), hud::GOLD)
        } else {
            (format!("EXTRACTION PAUSED  {pct:.0}%  -  get back on the pad"), paused)
        }))
    } else if status.lift > 0.0 || status.aboard > 0 {
        let frac = (status.lift / LIFT_TIME).clamp(0.0, 1.0);
        let pct = frac * 100.0;
        let next = status.deck + 2;
        Some((frac, if status.on_lift {
            (format!("LIFT TO DECK {next}  {pct:.0}%  -  the whole crew is aboard"), hud::GOLD)
        } else if status.crew_in > 1 {
            (format!("LIFT  {}/{} ABOARD  -  waiting for the whole crew", status.aboard, status.crew_in), paused)
        } else {
            (format!("LIFT PAUSED  {pct:.0}%  -  get back aboard"), paused)
        }))
    } else {
        None
    };
    let display = if content.is_some() { Display::Flex } else { Display::None };
    if panel.display != display {
        panel.display = display;
    }
    let Some((frac, (content, color))) = content else { return };
    bar.frac = frac;
    bar.color = color;
    let (mut text, mut text_color) = text.into_inner();
    if text.0 != content {
        text.0 = content;
    }
    text_color.0 = color;
}

/// While this machine's pirate is out early and the crew plays on, says what
/// they got away with.
fn update_got_out(
    status: Option<Single<&MissionStatus>>,
    local: Option<Single<&Player, With<LocalPlayer>>>,
    mut panel: Single<&mut Node, With<GotOutPanel>>,
    mut text: Single<&mut Text, With<GotOutText>>,
) {
    let playing = status.is_some_and(|s| s.phase == GameState::Playing);
    let out = local.filter(|p| playing && p.left_early);
    let display = if out.is_some() { Display::Flex } else { Display::None };
    if panel.display != display {
        panel.display = display;
    }
    let Some(player) = out else { return };
    let line = format!("YOU GOT OUT WITH {}", hud::credits(player.credits).to_uppercase());
    if text.0 != line {
        text.0 = line;
    }
}

/// This machine's pirate, as the results screen sees them.
#[derive(Clone, Copy)]
struct Seat {
    fate: Fate,
    credits: u32,
    payout: u32,
    deck: u8,
}

/// How the whole crew's run ended, counted from their pirates.
#[derive(Clone, Copy, Default)]
struct Tally {
    size: usize,
    finishers: usize,
    leavers: usize,
}

/// What the results screen says: title, its colour, the headline, and any
/// breakdown lines under it.
#[derive(PartialEq, Debug)]
struct Outcome {
    title: &'static str,
    color: Color,
    detail: String,
    lines: Vec<String>,
}

fn pirates(n: usize) -> String {
    if n == 1 { "1 pirate".into() } else { format!("{n} pirates") }
}

/// The results for `seat`. The settlement reaches clients by replication, so
/// until it's in (`split.crew_out` and a payout) the crew's share reads as
/// still being counted.
fn outcome(seat: Option<Seat>, tally: Tally, split: Settlement) -> Outcome {
    const OUT: Color = Color::srgb(0.45, 1.0, 0.55);
    const LOST: Color = Color::srgb(1.0, 0.35, 0.35);
    let Some(seat) = seat else {
        return Outcome {
            title: "RUN OVER",
            color: Color::WHITE,
            detail: "You joined too late for this run".into(),
            lines: Vec::new(),
        };
    };
    let settled = split.crew_out && (seat.payout > 0 || split.pool + split.bonus == 0);
    match seat.fate {
        Fate::Finished if tally.size == 1 => Outcome {
            title: "EXTRACTED",
            color: OUT,
            detail: format!("You got out with {} from deck {}", hud::credits(seat.credits), seat.deck + 1),
            lines: Vec::new(),
        },
        Fate::LeftEarly => {
            let crew = match (tally.finishers > 0, split.crew_out) {
                (true, true) => format!("The crew got out too, and split {}", hud::credits(split.pool + split.bonus)),
                (true, false) => "The crew got out too".into(),
                (false, _) => "The crew was lost in the station".into(),
            };
            Outcome {
                title: "GOT OUT EARLY",
                color: hud::GOLD,
                detail: format!("You kept {}", hud::credits(seat.credits)),
                lines: vec![crew],
            }
        }
        Fate::Finished => {
            let mut lines = Vec::new();
            if settled {
                lines.push(format!("Pool: {}", hud::credits(split.pool)));
                if tally.leavers > 0 {
                    let pct = (CREW_BONUS_PER_LEAVER * 100.0 * tally.leavers as f64).round() as u32;
                    lines.push(format!(
                        "Bonus: +{}  (+{pct}% from {} who left early)",
                        hud::credits(split.bonus),
                        pirates(tally.leavers),
                    ));
                }
                lines.push(format!("Split between {}", pirates(tally.finishers)));
            }
            Outcome {
                title: "CREW EXTRACTED",
                color: OUT,
                detail: if settled {
                    format!("Your share: {}", hud::credits(seat.payout))
                } else {
                    "Your share: counting...".into()
                },
                lines,
            }
        }
        Fate::Lost => {
            let mut lines = Vec::new();
            if tally.finishers > 0 {
                lines.push("The crew got out without you".into());
            }
            if tally.leavers > 0 {
                lines.push(format!("{} who left early kept theirs", pirates(tally.leavers)));
            }
            Outcome {
                title: "LOST IN THE STATION",
                color: LOST,
                detail: format!("{} of loot lost", hud::credits(seat.credits)),
                lines,
            }
        }
    }
}

/// The local pirate's results from what's replicated so far.
fn current_outcome(status: Option<&MissionStatus>, players: &Query<(&Player, Has<LocalPlayer>)>) -> Outcome {
    let mut tally = Tally::default();
    let mut seat = None;
    for (player, local) in players {
        let fate = Fate::of(player);
        tally.size += 1;
        tally.finishers += (fate == Fate::Finished) as usize;
        tally.leavers += (fate == Fate::LeftEarly) as usize;
        if local {
            seat = Some(Seat { fate, credits: player.credits, payout: player.payout, deck: player.deck });
        }
    }
    let split = status.map_or_else(Settlement::default, |s| Settlement {
        crew_out: s.crew_out,
        pool: s.pool,
        bonus: s.bonus,
    });
    outcome(seat, tally, split)
}

fn spawn_results(
    mut commands: Commands,
    mode: Res<NetMode>,
    status: Option<Single<&MissionStatus>>,
    players: Query<(&Player, Has<LocalPlayer>)>,
) {
    // Online, the crew regroups in the lobby, where classes can change hands.
    let again = if *mode == NetMode::Solo { "run it again" } else { "back to the lobby" };
    // `update_results` keeps this in step as the settlement arrives.
    let shown = current_outcome(status.map(Single::into_inner), &players);

    commands.spawn((
        DespawnOnExit(GameState::Over),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.7)),
        children![(
            ResultsPanel,
            hud::panel_accent(
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(18.0),
                    padding: UiRect::axes(Val::Px(48.0), Val::Px(32.0)),
                    ..default()
                },
                shown.color,
            ),
            children![
                (ResultsTitle, hud::text(shown.title, 56.0, shown.color)),
                (
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        ..default()
                    },
                    BackgroundColor(hud::EDGE),
                ),
                (ResultsDetail, hud::text(shown.detail, 22.0, hud::TEXT)),
                (
                    ResultsBreakdown,
                    hud::text(shown.lines.join("\n"), 15.0, hud::LABEL),
                    TextLayout::justify(Justify::Center),
                ),
                (
                    Node {
                        column_gap: Val::Px(24.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    },
                    children![hud::key_hint("ENTER", again), hud::key_hint("ESC", "main menu")],
                ),
            ],
        )],
    ));
}

/// Rewrites the results as replication catches up: on a client the payouts
/// and split can land a moment after the run ends.
fn update_results(
    status: Option<Single<&MissionStatus>>,
    players: Query<(&Player, Has<LocalPlayer>)>,
    panel: Option<Single<&mut BorderColor, With<ResultsPanel>>>,
    title: Option<Single<(&mut Text, &mut TextColor), With<ResultsTitle>>>,
    detail: Option<Single<&mut Text, (With<ResultsDetail>, Without<ResultsTitle>)>>,
    breakdown: Option<Single<&mut Text, (With<ResultsBreakdown>, Without<ResultsTitle>, Without<ResultsDetail>)>>,
) {
    let (Some(mut panel), Some(title), Some(mut detail), Some(mut breakdown)) = (panel, title, detail, breakdown) else {
        return;
    };
    let shown = current_outcome(status.map(Single::into_inner), &players);
    panel.top = shown.color;
    let (mut title, mut color) = title.into_inner();
    if title.0 != shown.title {
        title.0 = shown.title.into();
    }
    color.set_if_neq(TextColor(shown.color));
    if detail.0 != shown.detail {
        detail.0 = shown.detail;
    }
    let lines = shown.lines.join("\n");
    if breakdown.0 != lines {
        breakdown.0 = lines;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use Fate::*;

    #[test]
    fn solo_extraction_keeps_the_haul() {
        let (split, pay) = settle(&[(1000, Finished)]);
        assert_eq!(pay, [1000]);
        assert_eq!(split, Settlement { crew_out: true, pool: 1000, bonus: 0 });
    }

    #[test]
    fn leavers_keep_theirs_and_grow_the_finishers_split() {
        let (split, pay) = settle(&[(300, LeftEarly), (500, Finished), (200, LeftEarly), (700, Finished)]);
        // Pool 1200, +25% for each of 2 leavers = 600 bonus, split two ways.
        assert_eq!(split, Settlement { crew_out: true, pool: 1200, bonus: 600 });
        assert_eq!(pay, [300, 900, 200, 900]);
    }

    #[test]
    fn a_wipe_pays_only_the_leavers() {
        let (split, pay) = settle(&[(400, LeftEarly), (900, Lost), (50, Lost)]);
        assert_eq!(split, Settlement::default());
        assert_eq!(pay, [400, 0, 0]);
    }

    #[test]
    fn the_dead_get_nothing_and_add_nothing() {
        let (split, pay) = settle(&[(100, Finished), (2000, Lost), (201, Finished)]);
        assert_eq!(split.pool, 301);
        assert_eq!(split.bonus, 0);
        assert_eq!(pay, [151, 0, 151], "shares are rounded");
    }

    fn seat(fate: Fate, credits: u32, payout: u32) -> Option<Seat> {
        Some(Seat { fate, credits, payout, deck: 1 })
    }

    #[test]
    fn results_for_a_finisher_wait_for_the_split() {
        let tally = Tally { size: 4, finishers: 2, leavers: 2 };
        let pending = outcome(seat(Finished, 500, 0), tally, Settlement::default());
        assert_eq!(pending.detail, "Your share: counting...");
        assert!(pending.lines.is_empty());

        let split = Settlement { crew_out: true, pool: 1200, bonus: 600 };
        let done = outcome(seat(Finished, 500, 900), tally, split);
        assert_eq!(done.title, "CREW EXTRACTED");
        assert_eq!(done.detail, "Your share: 900 cr");
        assert_eq!(done.lines, [
            "Pool: 1,200 cr",
            "Bonus: +600 cr  (+50% from 2 pirates who left early)",
            "Split between 2 pirates",
        ]);
    }

    #[test]
    fn results_for_leavers_and_the_lost() {
        let tally = Tally { size: 3, finishers: 0, leavers: 1 };
        let early = outcome(seat(LeftEarly, 1240, 1240), tally, Settlement::default());
        assert_eq!((early.title, early.detail.as_str()), ("GOT OUT EARLY", "You kept 1,240 cr"));
        assert_eq!(early.lines, ["The crew was lost in the station"]);

        let lost = outcome(seat(Lost, 800, 0), tally, Settlement::default());
        assert_eq!((lost.title, lost.detail.as_str()), ("LOST IN THE STATION", "800 cr of loot lost"));
        assert_eq!(lost.lines, ["1 pirate who left early kept theirs"]);
    }

    #[test]
    fn solo_results_stay_simple() {
        let tally = Tally { size: 1, finishers: 1, leavers: 0 };
        let solo = outcome(seat(Finished, 3000, 3000), tally, Settlement::default());
        assert_eq!(solo.title, "EXTRACTED");
        assert_eq!(solo.detail, "You got out with 3,000 cr from deck 2");
    }

    #[test]
    fn waves_grow_with_the_crew() {
        let alarm = Alarm { level: 2.0, ..default() };
        let solo = alarm.wave_size(1);
        assert_eq!(solo, 2);
        assert_eq!(alarm.wave_size(3), 3 * solo);
        assert_eq!(alarm.wave_size(5), 5 * solo);
        assert_eq!(alarm.wave_size(0), solo, "an empty deck still gets a wave");
    }

    #[test]
    fn deeper_decks_answer_harder() {
        let top = Alarm { level: 2.0, ..default() };
        let bottom = Alarm { level: 2.0, deck: 2, ..default() };
        assert_eq!(bottom.wave_size(3), top.wave_size(3) + 2);
        assert!(bottom.wave_interval() < top.wave_interval());
        assert!(bottom.max_alive(1) > top.max_alive(1));
        assert!(bottom.threat() > top.threat());
        let purge = Alarm { level: MAX_ALARM, deck: 2, ..default() };
        assert_eq!(purge.wave_interval(), 5.0, "waves bottom out");
    }
}
