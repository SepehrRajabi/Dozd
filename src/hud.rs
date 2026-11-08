//! The HUD's shared look and layout: where each piece sits on screen, and the
//! panels, meters and key caps every module builds its piece from.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::combat::{Health, hit_weight};
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
/// Widths of a stat row's label, meter and readout, so the rows line up
/// across the pirate card (`CARD_WIDTH` wide, inside its padding).
pub const ROW_LABEL: f32 = 48.0;
pub const ROW_METER: f32 = 164.0;
pub const ROW_VALUE: f32 = 50.0;
pub const CARD_WIDTH: f32 = 300.0;
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
        app.init_resource::<LastDevice>()
            .add_systems(PreStartup, spawn_layout)
            .add_systems(Update, (draw_meters, draw_vignette, retire_controls_hint, (track_device, show_key_caps).chain()));
    }
}

/// What this machine last played with, so the key hints name its buttons.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LastDevice {
    #[default]
    Keys,
    Pad,
}

impl LastDevice {
    /// How the control bound to `key` (as the keyboard hints name it) reads
    /// on this device: on a gamepad, its button (see `PadInput` in `player.rs`).
    pub fn glyph(self, key: &'static str) -> &'static str {
        if self == Self::Keys {
            return key;
        }
        match key {
            "WASD" => "L-STICK",
            "SPACE" => "A",
            "LMB" => "RT",
            "R" => "X",
            "1-4" => "LB/RB",
            "Q" => "LT",
            "E" => "Y",
            "ESC" => "SELECT",
            "Y" => "D-UP",
            "N" => "D-DOWN",
            "ENTER" => "START",
            other => other,
        }
    }
}

/// A key cap's label, as the keyboard names it; `show_key_caps` swaps in the
/// gamepad's button while one is in use.
#[derive(Component)]
pub struct KeyCapText(pub &'static str);

/// Notices a switch between keyboard and mouse and a gamepad: whichever was
/// pressed, or moved, last.
fn track_device(
    mut last: ResMut<LastDevice>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    pads: Query<&Gamepad>,
) {
    let pad = pads
        .iter()
        .any(|p| p.digital().get_just_pressed().next().is_some() || p.left_stick().length() > 0.5 || p.right_stick().length() > 0.5);
    let keyboard = keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some() || motion.delta.length() > 2.0;
    if pad {
        last.set_if_neq(LastDevice::Pad);
    } else if keyboard {
        last.set_if_neq(LastDevice::Keys);
    }
}

fn show_key_caps(last: Res<LastDevice>, mut caps: Query<(Ref<KeyCapText>, &mut Text)>) {
    for (cap, mut text) in &mut caps {
        if last.is_changed() || cap.is_added() {
            let glyph = last.glyph(cap.0);
            if text.0 != glyph {
                text.0 = glyph.into();
            }
        }
    }
}

/// Where a piece of the HUD goes. Each module fills its slot at startup.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum HudSlot {
    /// Top left, in one card: who you are beside your haul, then shield,
    /// hull and dash, then your ability on Q, your passive, and what the
    /// crew's passives are doing for you.
    Pirate,
    Haul,
    Vitals,
    Ability,
    Passive,
    Effects,
    /// Top left, under that panel: the challenges in play.
    Challenges,
    /// Top centre: deck and alarm, the vault guardian's health while the crew
    /// fights it, the banner, then the leave prompt.
    Status,
    Boss,
    Banner,
    Notice,
    /// Top right: the session, then the rest of the crew.
    Session,
    Crew,
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

/// A pirate card's backdrop: their class's colour, darkened right down and
/// kept translucent, so the card reads as theirs and its text stays legible.
pub fn card_back(class_color: Color) -> Color {
    let c = class_color.to_srgba();
    let navy = PANEL.to_srgba();
    let mix = |class: f32, base: f32| base * 0.45 + class * 0.15;
    Color::srgba(mix(c.red, navy.red), mix(c.green, navy.green), mix(c.blue, navy.blue), 0.86)
}

/// A pirate's card: a panel backed in their class's dark shade, its top
/// edge in the class colour itself.
pub fn panel_class(node: Node, class_color: Color) -> (Node, BackgroundColor, BorderColor) {
    let (node, _, border) = panel_accent(node, class_color);
    (node, BackgroundColor(card_back(class_color)), border)
}

/// A whole number with thousands separators, e.g. `1,240`.
pub fn thousands(amount: u32) -> String {
    let digits = amount.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Credits with thousands separators, e.g. `1,240 cr`.
pub fn credits(amount: u32) -> String {
    thousands(amount) + " cr"
}

pub fn text(content: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(content), TextFont::from_font_size(size), TextColor(color))
}

/// The local pirate's card in the top-left corner, backed in a dark shade of
/// their class's colour with its top edge in the colour itself.
#[derive(Component)]
pub struct PirateCard;

/// The card's header strip, washed in their class's colour.
#[derive(Component)]
pub struct PirateHeader;

/// A hairline between a card's sections.
pub fn divider() -> impl Bundle {
    (
        Node {
            height: Val::Px(1.0),
            width: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(EDGE),
    )
}

/// A pirate's sprite in a frame tinted `color`, `size` pixels across.
pub fn portrait(image: Handle<Image>, color: Color, size: f32) -> impl Bundle {
    (
        Node {
            width: Val::Px(size + 6.0),
            height: Val::Px(size + 6.0),
            padding: UiRect::all(Val::Px(2.0)),
            border: UiRect { bottom: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(color.with_alpha(0.14)),
        BorderColor::all(color.with_alpha(0.7)),
        children![(
            ImageNode::new(image),
            Node {
                width: Val::Px(size),
                height: Val::Px(size),
                ..default()
            },
        )],
    )
}

/// A left-to-right wash of `color`, fading out by the middle: kept faint, as
/// it lies over a card already backed in that colour (`card_back`).
pub fn wash(color: Color) -> BackgroundGradient {
    BackgroundGradient::from(LinearGradient::new(LinearGradient::TO_RIGHT, vec![
        ColorStop::percent(color.with_alpha(0.1), 0.0),
        ColorStop::percent(color.with_alpha(0.0), 70.0),
    ]))
}

/// A keyboard key, drawn as a little cap.
pub fn key_cap(key: &'static str) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
            border: UiRect { bottom: Val::Px(2.0), ..UiRect::all(Val::Px(1.0)) },
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
        BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
        children![(KeyCapText(key), text(key, 12.0, TEXT))],
    )
}

/// A key cap followed by what it does.
pub fn key_hint(key: &'static str, action: &str) -> impl Bundle {
    (
        Node {
            column_gap: Val::Px(5.0),
            align_items: AlignItems::Center,
            ..default()
        },
        children![key_cap(key), text(action, 13.0, DIM)],
    )
}

/// The most effect chips a row shows at once.
const MAX_CHIPS: usize = 4;

/// A row of small labelled chips for the effects on a pirate, hidden while
/// there are none. Fill it with `show_chips`.
#[derive(Component)]
pub struct ChipRow;

#[derive(Component)]
pub struct Chip;

pub fn chip_row() -> impl Bundle {
    let chip = || {
        (
            Chip,
            Node {
                padding: UiRect::axes(Val::Px(5.0), Val::Px(1.0)),
                border: UiRect::all(Val::Px(1.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(EDGE),
            children![text("", 11.0, TEXT)],
        )
    };
    (
        ChipRow,
        Node {
            column_gap: Val::Px(4.0),
            row_gap: Val::Px(4.0),
            flex_wrap: FlexWrap::Wrap,
            display: Display::None,
            ..default()
        },
        children![chip(), chip(), chip(), chip()],
    )
}

/// The parts of the HUD `show_chips` writes to.
pub type ChipParts<'w, 's> = (
    Query<'w, 's, (&'static mut Node, &'static Children), With<ChipRow>>,
    Query<'w, 's, (&'static mut Node, &'static mut BackgroundColor, &'static mut BorderColor, &'static Children), (With<Chip>, Without<ChipRow>)>,
    Query<'w, 's, (&'static mut Text, &'static mut TextColor)>,
);

/// Shows `effects` on the chip row `row`, each label tinted its colour.
pub fn show_chips(parts: &mut ChipParts, row: Entity, effects: &[(String, Color)]) {
    let (rows, chips, texts) = parts;
    let Ok((mut node, children)) = rows.get_mut(row) else { return };
    let display = if effects.is_empty() { Display::None } else { Display::Flex };
    if node.display != display {
        node.display = display;
    }
    for (i, chip) in children.iter().enumerate().take(MAX_CHIPS) {
        let Ok((mut node, mut background, mut border, label)) = chips.get_mut(chip) else { continue };
        let effect = effects.get(i);
        let display = if effect.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let Some((content, color)) = effect else { continue };
        background.set_if_neq(BackgroundColor(color.with_alpha(0.15)));
        border.set_if_neq(BorderColor::all(color.with_alpha(0.6)));
        if let Some(Ok((mut text, mut tint))) = label.first().map(|&l| texts.get_mut(l)) {
            if text.0 != *content {
                text.0.clone_from(content);
            }
            tint.set_if_neq(TextColor(*color));
        }
    }
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
    meter_ticked(Val::Px(width), height, color, 0)
}

/// A meter split by `ticks` - 1 notches into equal parts, so amounts read at
/// a glance (no notches for 0 or 1).
pub fn meter_ticked(width: Val, height: f32, color: Color, ticks: u32) -> impl Bundle {
    let part = || Node {
        position_type: PositionType::Absolute,
        top: Val::Px(0.0),
        left: Val::Px(0.0),
        height: Val::Percent(100.0),
        width: Val::Percent(100.0),
        ..default()
    };
    let notches: Vec<_> = (1..ticks.max(1))
        .map(|i| {
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    left: Val::Percent(100.0 * i as f32 / ticks as f32),
                    width: Val::Px(1.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            )
        })
        .collect();
    (
        Meter { frac: 1.0, color, trail: 1.0 },
        Node {
            width,
            height: Val::Px(height),
            border: UiRect::all(Val::Px(1.0)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(TRACK),
        BorderColor::all(EDGE),
        Children::spawn((
            Spawn((MeterTrail, part(), BackgroundColor(Color::NONE))),
            Spawn((MeterFill, part(), BackgroundColor(color))),
            // A sheen along the top of the fill.
            Spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(0.0),
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(35.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
            )),
            SpawnIter(notches.into_iter()),
        )),
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
        // Settled once it's within a hair, rather than creeping on forever.
        let trail = if trail - frac < 0.001 { frac } else { trail };
        meter.bypass_change_detection().trail = trail;
        let mut parts = parts.iter_many_mut(children);
        while let Some((mut node, mut color, is_trail)) = parts.fetch_next() {
            let (width, tint) = if is_trail {
                (trail, Color::srgba(1.0, 0.95, 0.85, 0.55))
            } else {
                (frac, meter.color)
            };
            // Only on a change, so a still meter doesn't redo the HUD's layout.
            let width = Val::Percent(width * 100.0);
            if node.width != width {
                node.width = width;
            }
            color.set_if_neq(BackgroundColor(tint));
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
        PirateCard,
        panel(Node {
            width: Val::Px(CARD_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        }),
        children![
            (
                PirateHeader,
                Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    // Out to the card's edges, so the wash fills the strip.
                    margin: UiRect { left: Val::Px(-10.0), right: Val::Px(-10.0), top: Val::Px(-10.0), ..default() },
                    padding: UiRect { left: Val::Px(10.0), right: Val::Px(10.0), top: Val::Px(10.0), bottom: Val::Px(2.0) },
                    ..default()
                },
                BackgroundGradient::default(),
                children![
                    (HudSlot::Pirate, Node { flex_grow: 1.0, min_width: Val::Px(0.0), ..default() }),
                    (HudSlot::Haul, Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexEnd,
                        flex_shrink: 0.0,
                        ..default()
                    }),
                ],
            ),
            divider(),
            slot(HudSlot::Vitals),
            divider(),
            slot(HudSlot::Ability),
            slot(HudSlot::Passive),
            slot(HudSlot::Effects),
        ],
    ), slot(HudSlot::Challenges)]));
    commands.spawn((Hud, centred(true), children![
        centred_slot(HudSlot::Status),
        centred_slot(HudSlot::Boss),
        centred_slot(HudSlot::Banner),
        centred_slot(HudSlot::Notice),
    ]));
    commands.spawn((Hud, corner(true, false, AlignItems::FlexEnd), children![
        slot(HudSlot::Session),
        slot(HudSlot::Crew),
    ]));
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
    let max = player.as_ref().map_or(1.0, |h| h.max);
    let hull = player.map_or(1.0, |h| if h.is_dead() { 1.0 } else { (h.current / h.max).clamp(0.0, 1.0) });
    if hull < *last_hull {
        *flash = flash.max(hit_weight((*last_hull - hull) * max));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_hint_has_a_gamepad_button() {
        for key in ["WASD", "SPACE", "LMB", "R", "1-4", "Q", "E", "ESC", "Y", "N", "ENTER"] {
            assert_eq!(LastDevice::Keys.glyph(key), key);
            assert_ne!(LastDevice::Pad.glyph(key), key, "{key} has no gamepad button");
        }
    }

    #[test]
    fn credits_are_grouped_in_thousands() {
        assert_eq!(credits(0), "0 cr");
        assert_eq!(credits(950), "950 cr");
        assert_eq!(credits(1240), "1,240 cr");
        assert_eq!(credits(1_000_000), "1,000,000 cr");
    }
}
