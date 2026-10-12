//! The HUD's shared look and layout: where each piece sits on screen, and the
//! panels, meters and key caps every module builds its piece from.

use bevy::prelude::*;

use crate::combat::Health;
use crate::net::LocalPlayer;
use crate::{GameState, Hud};

pub const PANEL: Color = Color::srgba(0.03, 0.035, 0.07, 0.82);
pub const EDGE: Color = Color::srgba(0.6, 0.68, 0.9, 0.22);
pub const LABEL: Color = Color::srgb(0.58, 0.64, 0.78);
pub const TEXT: Color = Color::srgb(0.92, 0.94, 1.0);
pub const DIM: Color = Color::srgba(0.85, 0.9, 1.0, 0.5);
pub const GOLD: Color = Color::srgb(1.0, 0.84, 0.3);
/// A meter that's recharging rather than ready.
pub const SPENT: Color = Color::srgb(0.4, 0.45, 0.55);
/// Widths of a stat row's label and meter, so the rows line up.
pub const ROW_LABEL: f32 = 40.0;
pub const ROW_METER: f32 = 150.0;
const TRACK: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const MARGIN: f32 = 16.0;
/// Seconds of play before the controls hint bows out.
const CONTROLS_HINT_TIME: f32 = 30.0;
/// Hull fraction below which the screen edges start to glow red.
const LOW_HULL: f32 = 0.35;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        // Before `Startup`, so every module's HUD piece finds its slot.
        app.add_systems(PreStartup, spawn_layout)
            .add_systems(Update, (draw_meters, draw_vignette, retire_controls_hint));
    }
}

/// Where a piece of the HUD goes. Each module fills its slot at startup.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum HudSlot {
    /// Top left, in one panel: haul, then shield, hull and dash, then the class ability.
    Haul,
    Vitals,
    Ability,
    /// Top centre: deck and alarm, the banner, then the leave prompt.
    Status,
    Banner,
    Notice,
    /// Top right.
    Session,
    /// Bottom left, above the controls hint.
    Perks,
    /// Bottom centre: the loot prompt over the extraction or lift progress.
    Prompt,
    Progress,
    /// Bottom right.
    Weapon,
}

/// The id of a slot's node, to spawn a HUD piece into.
pub fn slot(slots: &Query<(Entity, &HudSlot)>, which: HudSlot) -> Entity {
    slots
        .iter()
        .find_map(|(entity, s)| (*s == which).then_some(entity))
        .expect("HUD slots are spawned in PreStartup")
}

/// A bordered backdrop around `node`, whose padding and layout are kept.
pub fn panel(node: Node) -> (Node, BackgroundColor, BorderColor) {
    panel_accent(node, EDGE)
}

/// A panel whose top edge is drawn in `accent`.
pub fn panel_accent(node: Node, accent: Color) -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            border: UiRect { top: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
            ..node
        },
        BackgroundColor(PANEL),
        BorderColor { top: accent, ..BorderColor::all(EDGE) },
    )
}

pub fn text(content: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(content), TextFont::from_font_size(size), TextColor(color))
}

/// A keyboard key, drawn as a little cap.
pub fn key_cap(key: &str) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
            border: UiRect { bottom: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
        children![text(key, 12.0, TEXT)],
    )
}

/// A key cap followed by what it does.
pub fn key_hint(key: &str, action: &str) -> impl Bundle {
    (
        Node {
            column_gap: Val::Px(5.0),
            align_items: AlignItems::Center,
            ..default()
        },
        children![key_cap(key), text(action, 13.0, DIM)],
    )
}

/// A bar filled to `frac`. Whatever it just lost lingers as a pale trail that
/// drains after it, so a hit reads at a glance.
#[derive(Component)]
pub struct Meter {
    pub frac: f32,
    pub color: Color,
    trail: f32,
}

#[derive(Component)]
struct MeterFill;

#[derive(Component)]
struct MeterTrail;

pub fn meter(width: f32, height: f32, color: Color) -> impl Bundle {
    let part = || Node {
        position_type: PositionType::Absolute,
        top: Val::Px(0.0),
        left: Val::Px(0.0),
        height: Val::Percent(100.0),
        width: Val::Percent(100.0),
        ..default()
    };
    (
        Meter { frac: 1.0, color, trail: 1.0 },
        Node {
            width: Val::Px(width),
            height: Val::Px(height),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(TRACK),
        BorderColor::all(EDGE),
        children![
            (MeterTrail, part(), BackgroundColor(Color::NONE)),
            (MeterFill, part(), BackgroundColor(color)),
        ],
    )
}

fn draw_meters(
    time: Res<Time>,
    mut meters: Query<(&mut Meter, &Children)>,
    mut parts: Query<(&mut Node, &mut BackgroundColor, Has<MeterTrail>), Or<(With<MeterFill>, With<MeterTrail>)>>,
) {
    let catch_up = 1.0 - (-4.0 * time.delta_secs()).exp();
    for (mut meter, children) in &mut meters {
        let frac = meter.frac.clamp(0.0, 1.0);
        let trail = if meter.trail <= frac { frac } else { meter.trail + (frac - meter.trail) * catch_up };
        meter.bypass_change_detection().trail = trail;
        let mut parts = parts.iter_many_mut(children);
        while let Some((mut node, mut color, is_trail)) = parts.fetch_next() {
            let (width, tint) = if is_trail {
                (trail, Color::srgba(1.0, 0.95, 0.85, 0.55))
            } else {
                (frac, meter.color)
            };
            node.width = Val::Percent(width * 100.0);
            color.0 = tint;
        }
    }
}

/// Red glow around the screen edges: a flash when the hull takes a hit, and a
/// slow pulse while it's low.
#[derive(Component)]
struct Vignette;

/// The controls reminder in the bottom-left corner.
#[derive(Component)]
struct ControlsHint;

fn spawn_layout(mut commands: Commands) {
    commands.spawn((
        Hud,
        Vignette,
        Pickable::IGNORE,
        ZIndex(-1),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundGradient::default(),
    ));

    let corner = |top: bool, left: bool, align: AlignItems| Node {
        position_type: PositionType::Absolute,
        top: if top { Val::Px(MARGIN) } else { Val::Auto },
        bottom: if top { Val::Auto } else { Val::Px(MARGIN) },
        left: if left { Val::Px(MARGIN) } else { Val::Auto },
        right: if left { Val::Auto } else { Val::Px(MARGIN) },
        flex_direction: FlexDirection::Column,
        align_items: align,
        row_gap: Val::Px(8.0),
        ..default()
    };
    let centred = |top: bool| Node {
        position_type: PositionType::Absolute,
        top: if top { Val::Px(MARGIN) } else { Val::Auto },
        // Clear of the controls hint along the bottom edge.
        bottom: if top { Val::Auto } else { Val::Px(MARGIN + 64.0) },
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: Val::Px(8.0),
        ..default()
    };
    let slot = |which: HudSlot| {
        (which, Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Stretch,
            row_gap: Val::Px(6.0),
            ..default()
        })
    };
    let centred_slot = |which: HudSlot| {
        (which, Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
    };

    commands.spawn((Hud, corner(true, true, AlignItems::FlexStart), children![(
        panel(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        }),
        children![slot(HudSlot::Haul), slot(HudSlot::Vitals), slot(HudSlot::Ability)],
    )]));
    commands.spawn((Hud, centred(true), children![
        centred_slot(HudSlot::Status),
        centred_slot(HudSlot::Banner),
        centred_slot(HudSlot::Notice),
    ]));
    commands.spawn((Hud, corner(true, false, AlignItems::FlexEnd), children![slot(HudSlot::Session)]));
    commands.spawn((Hud, corner(false, true, AlignItems::FlexStart), children![
        slot(HudSlot::Perks),
        (
            ControlsHint,
            Node {
                column_gap: Val::Px(14.0),
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
            children![
                key_hint("WASD", "move"),
                key_hint("SPACE", "dash"),
                key_hint("LMB", "fire"),
                key_hint("R", "reload"),
                key_hint("1-4", "guns"),
                key_hint("Q", "ability"),
                key_hint("E", "loot"),
                key_hint("ESC", "leave"),
            ],
        ),
    ]));
    commands.spawn((Hud, centred(false), children![
        centred_slot(HudSlot::Prompt),
        centred_slot(HudSlot::Progress),
    ]));
    commands.spawn((Hud, corner(false, false, AlignItems::FlexEnd), children![slot(HudSlot::Weapon)]));
}

fn draw_vignette(
    time: Res<Time>,
    player: Option<Single<&Health, With<LocalPlayer>>>,
    mut vignette: Single<&mut BackgroundGradient, With<Vignette>>,
    mut last_hull: Local<f32>,
    mut flash: Local<f32>,
) {
    let dt = time.delta_secs();
    let hull = player.map_or(1.0, |h| if h.is_dead() { 1.0 } else { (h.current / h.max).clamp(0.0, 1.0) });
    if hull < *last_hull {
        *flash = 1.0;
    }
    *last_hull = hull;
    *flash = (*flash - 2.5 * dt).max(0.0);

    let low = ((LOW_HULL - hull) / LOW_HULL).clamp(0.0, 1.0);
    let pulse = low * (0.2 + 0.08 * (time.elapsed_secs() * 5.0).sin());
    let alpha = (pulse + 0.25 * *flash).min(0.4);
    if alpha <= 0.0 {
        if !vignette.0.is_empty() {
            vignette.0.clear();
        }
        return;
    }
    let red = Color::srgb(0.85, 0.05, 0.05);
    vignette.0 = vec![
        RadialGradient::new(UiPosition::CENTER, RadialGradientShape::FarthestCorner, vec![
            ColorStop::percent(red.with_alpha(0.0), 50.0),
            ColorStop::percent(red.with_alpha(0.2 * alpha), 68.0),
            ColorStop::percent(red.with_alpha(0.55 * alpha), 84.0),
            ColorStop::percent(red.with_alpha(alpha), 100.0),
        ])
        .into(),
    ];
}

fn retire_controls_hint(
    time: Res<Time>,
    state: Res<State<GameState>>,
    mut hint: Single<&mut Node, With<ControlsHint>>,
    mut played: Local<f32>,
) {
    if *state.get() == GameState::Playing {
        *played += time.delta_secs();
    }
    if *played > CONTROLS_HINT_TIME && hint.display != Display::None {
        hint.display = Display::None;
    }
}
