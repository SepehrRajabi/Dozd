use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::enemies::{EnemyLook, roll_reinforcement, spawn_enemy};
use crate::hud::{self, HudSlot, Meter};
use crate::loot::LootKind;
use crate::net::{LocalPlayer, NetMode, authority};
use crate::player::{Controls, DASH_COOLDOWN, Dash, PirateStatus, Player};
use crate::room::{DECKS, Heist, RoomGrid};
use crate::settings::Settings;
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
const MAX_ALIVE: usize = 14;
/// Reinforcements won't pop out of a spawn point closer than this to any pirate.
const SPAWN_CLEARANCE: f32 = 70.0;

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Alarm>()
            .init_resource::<Extraction>()
            .init_resource::<Lift>()
            .init_resource::<Banner>()
            .add_systems(
                OnEnter(GameState::Playing),
                (spawn_status, reset_mission).run_if(authority),
            )
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
                (update_player_bars, update_alarm_hud, update_banner, update_extraction_hud)
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

    fn wave_interval(&self) -> f32 {
        (18.0 - 2.5 * self.level).max(5.0)
    }

    /// Arriving on a fresh deck: the alarm carries over, minus a little relief.
    fn new_deck(&mut self) {
        self.level = (self.level - DECK_ALARM_RELIEF).max(0.0);
        self.announced = self.level.floor() as u32;
        self.wave_timer = 8.0;
        self.surge = 0;
    }

    fn wave_size(&self, crew: usize) -> usize {
        let base = 1.0 + (self.level * 0.8).floor();
        // Each extra pirate adds half a wave: a crew is stronger, but not twice as safe.
        (base * (1.0 + 0.5 * crew.saturating_sub(1) as f32)).round() as usize
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
    aboard: bool,
}

#[derive(Resource, Default)]
struct Banner {
    text: String,
    color: Color,
    timer: f32,
}

impl Banner {
    fn show(&mut self, text: impl Into<String>, color: Color, secs: f32) {
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
    mode: Res<NetMode>,
) {
    *alarm = Alarm::default();
    *extraction = Extraction::default();
    *lift = Lift::default();
    let text = match *mode {
        NetMode::Solo => "Hold a hazard pad to extract, or ride the lift down to richer decks",
        _ => "Only pirates aboard when a lift or pad completes go with it",
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

    let cap = MAX_ALIVE + 4 * crew.len().saturating_sub(1);
    let room = cap.saturating_sub(enemies.iter().count());
    let count = (alarm.wave_size(crew.len()) + alarm.surge).min(room);
    alarm.surge = 0;
    for i in 0..count {
        let point = points[(i + (rng.unit() * points.len() as f32) as usize) % points.len()];
        let pos = point + Vec2::new(rng.signed(), rng.signed()) * 3.0;
        let kind = roll_reinforcement(&mut rng, alarm.level);
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

/// The lift fills while any living pirate is aboard. When it goes, the pirates
/// aboard ride down to the next deck; everyone else is left behind.
fn ride_lift(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut heist: ResMut<Heist>,
    mut alarm: ResMut<Alarm>,
    mut lift: ResMut<Lift>,
    mut extraction: ResMut<Extraction>,
    mut banner: ResMut<Banner>,
    mut players: Query<(&mut Player, &Health, &Transform, &mut Controls)>,
) {
    let active = |p: &Player, h: &Health| p.status == PirateStatus::Active && !h.is_dead();
    let was_aboard = lift.aboard;
    lift.aboard = players
        .iter()
        .any(|(p, h, t, _)| active(p, h) && standing_on(&grid, t, 'L'));
    if lift.aboard && !was_aboard && lift.progress == 0.0 {
        banner.show("LIFT ARMED  -  anyone not aboard stays on this deck", Color::srgb(0.5, 0.9, 1.0), 3.0);
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
    for (mut player, health, transform, mut controls) in &mut players {
        if !active(&player, health) {
            continue;
        }
        if standing_on(&grid, transform, 'L') {
            player.deck = heist.deck;
        } else {
            player.status = PirateStatus::LeftBehind;
            *controls = Controls::default();
        }
    }
    *lift = Lift::default();
    *extraction = Extraction::default();
    alarm.new_deck();
    let text = if heist.deck + 1 == DECKS {
        format!("DECK {DECKS}/{DECKS}  -  THE VAULT. Nothing deeper: grab it and find a pad")
    } else {
        format!("DECK {}/{DECKS}  -  richer loot, tougher guards", heist.deck + 1)
    };
    banner.show(text, Color::srgb(0.5, 0.9, 1.0), 4.0);
}

/// Extraction fills while any living pirate holds the pad. When it completes,
/// only the pirates standing on it get out; everyone else is left behind.
fn extraction(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut alarm: ResMut<Alarm>,
    mut extraction: ResMut<Extraction>,
    mut banner: ResMut<Banner>,
    mut next_state: ResMut<NextState<GameState>>,
    mut players: Query<(&mut Player, &Health, &Transform)>,
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

    if extraction.progress >= EXTRACT_TIME {
        for (mut player, health, transform) in &mut players {
            if active(&player, health) {
                player.status = if on_pad(&grid, transform) {
                    PirateStatus::Extracted
                } else {
                    PirateStatus::LeftBehind
                };
            }
        }
        next_state.set(GameState::Over);
    }
}

fn check_crew_wiped(mut next_state: ResMut<NextState<GameState>>, players: Query<(&Health, &Player)>) {
    let anyone_left = players
        .iter()
        .any(|(h, p)| !h.is_dead() && p.status == PirateStatus::Active);
    if !players.is_empty() && !anyone_left {
        next_state.set(GameState::Over);
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

    let left_behind = local.is_some_and(|p| p.status == PirateStatus::LeftBehind);
    deck.set_if_neq(Text(format!("DECK {}/{DECKS}", status.deck + 1)));
    let (content, tint) = if left_behind && status.phase == GameState::Playing {
        ("LEFT BEHIND", Color::srgb(1.0, 0.7, 0.3))
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

    let waves = if left_behind {
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
        Some((frac, if status.on_pad {
            (format!("EXTRACTING  {pct:.0}%  -  be on the pad when it hits 100"), hud::GOLD)
        } else {
            (format!("EXTRACTION PAUSED  {pct:.0}%  -  get back on the pad"), paused)
        }))
    } else if status.lift > 0.0 {
        let frac = (status.lift / LIFT_TIME).clamp(0.0, 1.0);
        let pct = frac * 100.0;
        let next = status.deck + 2;
        Some((frac, if status.on_lift {
            (format!("LIFT TO DECK {next}  {pct:.0}%  -  anyone not aboard stays behind"), hud::GOLD)
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

fn spawn_results(mut commands: Commands, player: Option<Single<(&Player, &Health), With<LocalPlayer>>>) {
    let (title, title_color, detail) = match player.as_deref() {
        Some((p, _)) if p.status == PirateStatus::Extracted => (
            "EXTRACTED",
            Color::srgb(0.45, 1.0, 0.55),
            format!("You got out from deck {} with {} items worth {} cr", p.deck + 1, p.items, p.credits),
        ),
        Some((p, _)) if p.status == PirateStatus::LeftBehind => (
            "LEFT BEHIND",
            Color::srgb(1.0, 0.7, 0.3),
            format!("The crew left without you. {} cr stays on deck {}", p.credits, p.deck + 1),
        ),
        Some((p, _)) => (
            "LOST IN THE STATION",
            Color::srgb(1.0, 0.35, 0.35),
            format!("{} cr of loot is lost with you", p.credits),
        ),
        None => ("RUN OVER", Color::WHITE, "You joined too late for this run".into()),
    };

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
            hud::panel_accent(
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(18.0),
                    padding: UiRect::axes(Val::Px(48.0), Val::Px(32.0)),
                    ..default()
                },
                title_color,
            ),
            children![
                hud::text(title, 56.0, title_color),
                (
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(1.0),
                        ..default()
                    },
                    BackgroundColor(hud::EDGE),
                ),
                hud::text(detail, 22.0, hud::TEXT),
                (
                    Node {
                        column_gap: Val::Px(24.0),
                        margin: UiRect::top(Val::Px(8.0)),
                        ..default()
                    },
                    children![hud::key_hint("ENTER", "run it again"), hud::key_hint("ESC", "main menu")],
                ),
            ],
        )],
    ));
}
