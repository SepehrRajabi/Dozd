//! The vault room's gate as drawn, and the call to breach it: the prompt every
//! pirate answers with Y or N.

use bevy::prelude::*;

use crate::PIXEL_SCALE;
use crate::breach::{Breach, BreachState};
use crate::hud::{self, HudSlot};
use crate::net::LocalId;
use crate::player::{Player, slot_color};

const LOCK_COLOR: Color = Color::srgb(1.0, 0.2, 0.25);
const VOTE_COLOR: Color = Color::srgb(1.0, 0.7, 0.2);
const READY_COLOR: Color = Color::srgb(0.45, 0.95, 0.5);
/// The most pirates a crew can have, and so the most pips the vote shows.
const MAX_VOTERS: usize = 4;
/// Seconds left on the call when its countdown turns red.
const VOTE_LOW: f32 = 5.0;
/// What the sealed gate says: never what waits inside, only that something does.
const GATE_LABEL: &str = "SEALED  -  SOMETHING GUARDS THE VAULT";

pub struct BreachViewPlugin;

impl Plugin for BreachViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_breach)
            .add_systems(Startup, (load_gate_images, spawn_vote_panel))
            .add_systems(Update, (draw_gates, update_vote_panel));
    }
}

/// The gate drawn shut and drawn open.
#[derive(Resource)]
struct GateImages {
    sealed: Handle<Image>,
    open: Handle<Image>,
}

fn load_gate_images(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GateImages {
        sealed: assets.load("sprites/tiles/gate.png"),
        open: assets.load("sprites/tiles/gate_open.png"),
    });
}

/// The gate's bulkhead sprite, turned to run along its wall.
#[derive(Component)]
struct GateSprite;

/// The glow over the gate's lock lights.
#[derive(Component)]
struct GateLock;

/// "SEALED - SOMETHING GUARDS THE VAULT", in the vault's tier colour, floating
/// over the gate until the crew is let in. It never names what waits inside.
#[derive(Component)]
struct GateLabel;

fn dress_breach(add: On<Add, Breach>, mut commands: Commands, assets: Res<AssetServer>, breaches: Query<&Breach>) {
    let Ok(breach) = breaches.get(add.entity) else { return };
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        parent.spawn((
            GateSprite,
            Sprite::from_image(assets.load("sprites/tiles/gate.png")),
            gate_transform(breach.vertical),
        ));
        parent.spawn((
            GateLock,
            Sprite { image: assets.load("sprites/fx/spark.png"), color: LOCK_COLOR, ..default() },
            Transform::from_xyz(0.0, 0.0, 0.2),
        ));
        parent.spawn((
            GateLabel,
            Text2d::new(GATE_LABEL),
            TextFont::from_font_size(5.0 * PIXEL_SCALE),
            TextColor(breach.tier.color()),
            Transform::from_xyz(0.0, label_height(breach.vertical), 20.0).with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
        ));
    });
}

/// The gate sprite is drawn across; a vertical gate turns it a quarter.
fn gate_transform(vertical: bool) -> Transform {
    let turn = if vertical { std::f32::consts::FRAC_PI_2 } else { 0.0 };
    Transform::from_xyz(0.0, 0.0, 0.1).with_rotation(Quat::from_rotation_z(turn))
}

/// How high over the gate's centre its label floats, clear of the gate.
fn label_height(vertical: bool) -> f32 {
    if vertical { 22.0 } else { 14.0 }
}

/// Shut and dimly lit while sealed; lights blinking amber while the crew
/// votes; locked red and throbbing during the fight; drawn open after.
#[allow(clippy::type_complexity)]
fn draw_gates(
    time: Res<Time>,
    images: Res<GateImages>,
    breaches: Query<&Breach>,
    mut sprites: Query<(&ChildOf, &mut Sprite, &mut Transform), (With<GateSprite>, Without<GateLock>, Without<GateLabel>)>,
    mut locks: Query<(&ChildOf, &mut Sprite, &mut Transform, &mut Visibility), (With<GateLock>, Without<GateLabel>)>,
    mut labels: Query<(&ChildOf, &mut TextColor, &mut Transform, &mut Visibility), (With<GateLabel>, Without<GateSprite>, Without<GateLock>)>,
) {
    let t = time.elapsed_secs();
    for (parent, mut sprite, mut transform) in &mut sprites {
        let Ok(breach) = breaches.get(parent.parent()) else { continue };
        let image = if breach.is_open() { &images.open } else { &images.sealed };
        if sprite.image != *image {
            sprite.image = image.clone();
        }
        transform.set_if_neq(gate_transform(breach.vertical));
    }
    for (parent, mut sprite, mut transform, mut visibility) in &mut locks {
        let Ok(breach) = breaches.get(parent.parent()) else { continue };
        let (color, scale) = match breach.state {
            BreachState::Sealed => (LOCK_COLOR.with_alpha(0.35 + 0.15 * (t * 1.5).sin()), 0.8),
            BreachState::Vote { .. } => {
                let on = (t * 6.0).sin() > 0.0;
                (VOTE_COLOR.with_alpha(if on { 0.9 } else { 0.25 }), 0.9)
            }
            BreachState::Fight => {
                let throb = 0.5 + 0.5 * (t * 7.0).sin();
                (LOCK_COLOR.with_alpha(0.55 + 0.45 * throb), 1.0 + 0.5 * throb)
            }
            BreachState::Open => (Color::NONE, 0.0),
        };
        visibility.set_if_neq(if breach.is_open() { Visibility::Hidden } else { Visibility::Inherited });
        sprite.color = color;
        transform.scale = Vec3::splat(scale);
    }
    for (parent, mut color, mut transform, mut visibility) in &mut labels {
        let Ok(breach) = breaches.get(parent.parent()) else { continue };
        let shown = matches!(breach.state, BreachState::Sealed | BreachState::Vote { .. });
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        let bob = (t * 2.0).sin().round();
        transform.translation.y = label_height(breach.vertical) + bob;
        color.set_if_neq(TextColor(breach.tier.color()));
    }
}

/// The call to breach, for a pirate it asks: who called it, Y or N, and how
/// the crew stands.
#[derive(Component)]
struct VotePanel;

#[derive(Component)]
struct VoteCaller;

#[derive(Component)]
struct VoteCall;

/// The Y / N keys, until this pirate has said yes.
#[derive(Component)]
struct VoteKeys;

/// "WAITING FOR THE CREW", once this pirate has said yes.
#[derive(Component)]
struct VoteWaiting;

#[derive(Component)]
struct VoteCount;

#[derive(Component)]
struct VoteTimer;

/// One crewmate's answer, lit in their colour once they've said yes.
#[derive(Component)]
struct VotePip(usize);

fn spawn_vote_panel(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    let pip = |i: usize| {
        (
            VotePip(i),
            Node {
                width: Val::Px(10.0),
                height: Val::Px(10.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(hud::EDGE),
        )
    };
    commands.entity(hud::slot(&slots, HudSlot::Prompt)).with_child((
        VotePanel,
        hud::panel_accent(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                display: Display::None,
                ..default()
            },
            VOTE_COLOR,
        ),
        children![
            (
                Node {
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Baseline,
                    ..default()
                },
                children![
                    (VoteCaller, hud::text("", 16.0, hud::TEXT), TextLayout::no_wrap()),
                    (VoteCall, hud::text("", 16.0, hud::TEXT), TextLayout::no_wrap()),
                ],
            ),
            (
                VoteKeys,
                Node {
                    column_gap: Val::Px(16.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![hud::key_hint("Y", "JOIN"), hud::key_hint("N", "NOT YET")],
            ),
            (VoteWaiting, hud::text("WAITING FOR THE CREW", 13.0, hud::DIM), TextLayout::no_wrap()),
            (
                Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    (
                        Node {
                            column_gap: Val::Px(4.0),
                            ..default()
                        },
                        children![pip(0), pip(1), pip(2), pip(3)],
                    ),
                    (VoteCount, hud::text("", 13.0, hud::TEXT), TextLayout::no_wrap()),
                    (VoteTimer, hud::text("", 13.0, hud::LABEL), TextLayout::no_wrap()),
                ],
            ),
        ],
    ));
}

/// What the call says to the pirate `me` (a `Player::owner`), and the
/// answers so far: e.g. "P1" and "IS CALLING THE CREW INTO THE SILVER VAULT".
fn call_text(breach: &Breach, by: u64, me: u64, players: &[(u64, u8)]) -> (String, String) {
    let vault = format!("THE {} VAULT", breach.tier.name());
    if by == me {
        return ("YOU".into(), format!("CALLED THE CREW INTO {vault}"));
    }
    let caller = players
        .iter()
        .find(|(owner, _)| *owner == by)
        .map_or_else(|| "A CREWMATE".into(), |(_, slot)| format!("P{}", slot + 1));
    (caller, format!("IS CALLING THE CREW INTO {vault}"))
}

/// "2/3 READY".
fn ready_text(accepted: usize, voters: usize) -> String {
    format!("{accepted}/{voters} READY")
}

#[allow(clippy::type_complexity)]
fn update_vote_panel(
    time: Res<Time>,
    local: Option<Res<LocalId>>,
    breaches: Query<&Breach>,
    players: Query<&Player>,
    mut panel: Single<&mut Node, With<VotePanel>>,
    mut nodes: Query<&mut Node, (Or<(With<VoteKeys>, With<VoteWaiting>)>, Without<VotePanel>, Without<VotePip>)>,
    keys: Single<Entity, With<VoteKeys>>,
    waiting: Single<Entity, With<VoteWaiting>>,
    caller: Single<Entity, With<VoteCaller>>,
    call: Single<Entity, With<VoteCall>>,
    count: Single<Entity, With<VoteCount>>,
    timer: Single<Entity, With<VoteTimer>>,
    mut pips: Query<(&VotePip, &mut Node, &mut BackgroundColor, &mut BorderColor), Without<VotePanel>>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let me = local.map(|id| id.0);
    // The call this pirate is asked to answer, if any.
    let vote = me.and_then(|me| {
        breaches.iter().find_map(|breach| match &breach.state {
            BreachState::Vote { by, voters, accepted, left } if voters.contains(&me) => {
                Some((me, breach, *by, voters, accepted, *left))
            }
            _ => None,
        })
    });
    let display = if vote.is_some() { Display::Flex } else { Display::None };
    if panel.display != display {
        panel.display = display;
    }
    let Some((me, breach, by, voters, accepted, left)) = vote else { return };

    let slots: Vec<(u64, u8)> = players.iter().map(|p| (p.owner, p.slot)).collect();
    let slot_of = |owner: u64| slots.iter().find(|(o, _)| *o == owner).map(|(_, slot)| *slot);
    let (who, what) = call_text(breach, by, me, &slots);
    let who_color = if by == me { hud::TEXT } else { slot_of(by).map_or(hud::TEXT, slot_color) };
    set_text(&mut texts, *caller, &who, who_color);
    set_text(&mut texts, *call, &what, hud::TEXT);

    let answered = accepted.contains(&me);
    for (entity, shown) in [(*keys, !answered), (*waiting, answered)] {
        if let Ok(mut node) = nodes.get_mut(entity) {
            let display = if shown { Display::Flex } else { Display::None };
            if node.display != display {
                node.display = display;
            }
        }
    }

    let all_in = accepted.len() >= voters.len();
    let count_color = if all_in { READY_COLOR } else { hud::TEXT };
    set_text(&mut texts, *count, &ready_text(accepted.len(), voters.len()), count_color);
    let blink = (time.elapsed_secs() * 6.0).sin() > 0.0;
    let timer_color = if left <= VOTE_LOW && blink { LOCK_COLOR } else { hud::LABEL };
    set_text(&mut texts, *timer, &format!("{:.0}s", left.max(0.0).ceil()), timer_color);

    for (pip, mut node, mut background, mut border) in &mut pips {
        let voter = voters.get(pip.0).filter(|_| pip.0 < MAX_VOTERS);
        let display = if voter.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let Some(&voter) = voter else { continue };
        let color = slot_of(voter).map_or(hud::TEXT, slot_color);
        let fill = if accepted.contains(&voter) { color } else { Color::NONE };
        background.set_if_neq(BackgroundColor(fill));
        border.set_if_neq(BorderColor::all(color.with_alpha(0.8)));
    }
}

fn set_text(texts: &mut Query<(&mut Text, &mut TextColor)>, entity: Entity, content: &str, color: Color) {
    let Ok((mut text, mut text_color)) = texts.get_mut(entity) else { return };
    if text.0 != content {
        text.0 = content.into();
    }
    text_color.set_if_neq(TextColor(color));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::VaultTier;

    fn breach() -> Breach {
        Breach {
            state: BreachState::Sealed,
            tier: VaultTier::Silver,
            gate: Vec2::ZERO,
            vertical: false,
            outside: Vec2::ZERO,
            inside: Vec2::ZERO,
        }
    }

    #[test]
    fn the_call_names_its_caller_and_vault() {
        let players = [(10, 0), (20, 2)];
        let (who, what) = call_text(&breach(), 10, 20, &players);
        assert_eq!(who, "P1");
        assert_eq!(what, "IS CALLING THE CREW INTO THE SILVER VAULT");
        let (who, what) = call_text(&breach(), 20, 20, &players);
        assert_eq!(who, "YOU");
        assert_eq!(what, "CALLED THE CREW INTO THE SILVER VAULT");
        assert_eq!(call_text(&breach(), 99, 20, &players).0, "A CREWMATE");
        assert_eq!(ready_text(2, 3), "2/3 READY");
    }
}
