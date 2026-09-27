use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::enemies::{EnemyLook, roll_reinforcement, spawn_enemy};
use crate::loot::LootKind;
use crate::net::{LocalPlayer, NetMode, authority};
use crate::player::{Controls, DASH_COOLDOWN, Dash, PirateStatus, Player};
use crate::room::{DECKS, Heist, RoomGrid};
use crate::{GameState, Hud, Rng};

pub const MAX_ALARM: f32 = 5.0;
/// Reinforcement waves. Off for now while extraction is being tested; set back to true.
const REINFORCEMENTS: bool = false;
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
                (update_player_bars, update_alarm_text, update_banner, update_extraction_hud)
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
struct HullFill;

#[derive(Component)]
struct ShieldFill;

#[derive(Component)]
struct DashFill;

#[derive(Component)]
struct AlarmText;

#[derive(Component)]
struct BannerText;

#[derive(Component)]
struct ExtractionPanel;

#[derive(Component)]
struct ExtractionFill;

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
    mut status: Single<&mut MissionStatus>,
) {
    status.set_if_neq(MissionStatus {
        phase: *state.get(),
        seed: heist.seed,
        deck: heist.deck,
        alarm: alarm.level,
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
    players: Query<(&Transform, &Health, &Player)>,
    enemies: Query<(), With<EnemyLook>>,
) {
    if alarm.level < 1.0 {
        return;
    }
    let dt = time.delta_secs();
    alarm.level = (alarm.level + ALARM_CREEP * dt).min(MAX_ALARM);
    if !REINFORCEMENTS {
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

/// A labelled bar whose fill node carries `marker`.
fn stat_row(panel: &mut ChildSpawnerCommands, label: &str, width: f32, marker: impl Component, color: Color) {
    panel
        .spawn(Node {
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont::from_font_size(14.0),
                TextColor(Color::srgb(0.8, 0.85, 0.95)),
                Node {
                    width: Val::Px(44.0),
                    ..default()
                },
            ));
            row.spawn((bar(width, 8.0), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))))
                .with_child((
                    marker,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(color),
                ));
        });
}

fn bar(width: f32, height: f32) -> Node {
    Node {
        width: Val::Px(width),
        height: Val::Px(height),
        ..default()
    }
}

fn spawn_mission_hud(mut commands: Commands) {
    // Shield, hull and dash readiness, under the haul counter.
    commands
        .spawn((Hud, Node {
            position_type: PositionType::Absolute,
            top: Val::Px(46.0),
            left: Val::Px(18.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        }))
        .with_children(|panel| {
            stat_row(panel, "SHLD", 180.0, ShieldFill, Color::srgb(0.4, 0.72, 1.0));
            stat_row(panel, "HULL", 180.0, HullFill, Color::srgb(0.35, 0.9, 0.45));
            stat_row(panel, "DASH", 60.0, DashFill, Color::srgb(0.75, 0.92, 1.0));
        });

    // Alarm meter and banner, top centre.
    commands.spawn((
        Hud,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(8.0),
            ..default()
        },
        children![
            (
                AlarmText,
                Text::new(""),
                TextFont::from_font_size(20.0),
                TextColor(Color::WHITE),
            ),
            (
                BannerText,
                Text::new(""),
                TextFont::from_font_size(22.0),
                TextColor(Color::WHITE),
                // Clear of the stat bars in the top-left.
                Node {
                    margin: UiRect::top(Val::Px(56.0)),
                    ..default()
                },
            ),
        ],
    ));

    // Extraction progress, lower centre.
    commands.spawn((
        Hud,
        ExtractionPanel,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(110.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            display: Display::None,
            ..default()
        },
        children![
            (
                ExtractionText,
                Text::new(""),
                TextFont::from_font_size(20.0),
                TextColor(Color::srgb(1.0, 0.85, 0.3)),
            ),
            (
                bar(320.0, 10.0),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                children![(
                    ExtractionFill,
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(1.0, 0.8, 0.25)),
                )],
            ),
        ],
    ));
}

fn update_player_bars(
    player: Option<Single<(&Health, &Dash), With<LocalPlayer>>>,
    mut hull: Single<(&mut Node, &mut BackgroundColor), With<HullFill>>,
    mut shield: Single<&mut Node, (With<ShieldFill>, Without<HullFill>)>,
    mut dash_fill: Single<(&mut Node, &mut BackgroundColor), (With<DashFill>, Without<HullFill>, Without<ShieldFill>)>,
) {
    let Some(player) = player else { return };
    let (health, dash) = *player;

    let frac = (health.current / health.max).clamp(0.0, 1.0);
    let (hull_node, hull_color) = &mut *hull;
    hull_node.width = Val::Percent(frac * 100.0);
    hull_color.0 = if frac > 0.5 {
        Color::srgb(0.35, 0.9, 0.45)
    } else if frac > 0.25 {
        Color::srgb(1.0, 0.75, 0.25)
    } else {
        Color::srgb(1.0, 0.3, 0.3)
    };

    shield.width = Val::Percent((health.shield / health.max_shield).clamp(0.0, 1.0) * 100.0);

    let ready = 1.0 - (dash.cooldown / DASH_COOLDOWN).clamp(0.0, 1.0);
    let (dash_node, dash_color) = &mut *dash_fill;
    dash_node.width = Val::Percent(ready * 100.0);
    dash_color.0 = if ready >= 1.0 {
        Color::srgb(0.75, 0.92, 1.0)
    } else {
        Color::srgb(0.4, 0.45, 0.55)
    };
}

fn update_alarm_text(
    status: Option<Single<&MissionStatus>>,
    local: Option<Single<&Player, With<LocalPlayer>>>,
    text: Single<(&mut Text, &mut TextColor), With<AlarmText>>,
) {
    let Some(status) = status else { return };
    let (mut text, mut color) = text.into_inner();
    let deck = format!("DECK {}/{DECKS}", status.deck + 1);
    let left_behind = local.is_some_and(|p| p.status == PirateStatus::LeftBehind);
    let (content, tint) = if left_behind && status.phase == GameState::Playing {
        (
            format!("LEFT BEHIND  -  watching the crew on deck {}", status.deck + 1),
            Color::srgb(1.0, 0.7, 0.3),
        )
    } else if status.alarm <= 0.0 {
        (format!("{deck}    UNDETECTED"), Color::srgba(0.7, 0.9, 1.0, 0.7))
    } else {
        let filled = (status.alarm.floor() as usize).min(MAX_ALARM as usize);
        let meter: String = (0..MAX_ALARM as usize).map(|i| if i < filled { '#' } else { '-' }).collect();
        let waves = if REINFORCEMENTS {
            format!("next wave {:.0}s", status.next_wave)
        } else {
            "reinforcements off".into()
        };
        (format!("{deck}    ALARM [{meter}]   {waves}"), alarm_color(status.alarm))
    };
    if text.0 != content {
        text.0 = content;
    }
    color.0 = tint;
}

fn update_banner(status: Option<Single<&MissionStatus>>, text: Single<(&mut Text, &mut TextColor), With<BannerText>>) {
    let Some(status) = status else { return };
    let (mut text, mut color) = text.into_inner();
    if text.0 != status.banner {
        text.0.clone_from(&status.banner);
    }
    color.0 = status.banner_color.with_alpha(status.banner_alpha);
}

fn update_extraction_hud(
    status: Option<Single<&MissionStatus>>,
    mut panel: Single<&mut Node, (With<ExtractionPanel>, Without<ExtractionFill>)>,
    mut fill: Single<&mut Node, With<ExtractionFill>>,
    mut text: Single<&mut Text, With<ExtractionText>>,
) {
    let Some(status) = status else { return };
    // Extraction matters more, so it wins if both are running.
    let content = if status.extraction > 0.0 {
        let frac = (status.extraction / EXTRACT_TIME).clamp(0.0, 1.0);
        let pct = frac * 100.0;
        Some((frac, if status.on_pad {
            format!("EXTRACTING  {pct:.0}%  -  be on the pad when it hits 100")
        } else {
            format!("EXTRACTION PAUSED  {pct:.0}%  -  get back on the pad")
        }))
    } else if status.lift > 0.0 {
        let frac = (status.lift / LIFT_TIME).clamp(0.0, 1.0);
        let pct = frac * 100.0;
        let next = status.deck + 2;
        Some((frac, if status.on_lift {
            format!("LIFT TO DECK {next}  {pct:.0}%  -  anyone not aboard stays behind")
        } else {
            format!("LIFT PAUSED  {pct:.0}%  -  get back aboard")
        }))
    } else {
        None
    };
    panel.display = if content.is_some() { Display::Flex } else { Display::None };
    let Some((frac, content)) = content else { return };
    fill.width = Val::Percent(frac * 100.0);
    if text.0 != content {
        text.0 = content;
    }
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
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(18.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.02, 0.75)),
        children![
            (Text::new(title), TextFont::from_font_size(64.0), TextColor(title_color)),
            (Text::new(detail), TextFont::from_font_size(24.0), TextColor(Color::WHITE)),
            (
                Text::new("ENTER  run it again        ESC  main menu"),
                TextFont::from_font_size(18.0),
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
            ),
        ],
    ));
}
