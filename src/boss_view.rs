//! How a vault's guardian looks: the keeper's body, hands, beams and charging
//! orbs, the stalker's crouch, marks and lunge, and the health bar while the
//! crew fights either. None of it shows unless the camera is in the vault room:
//! what guards a vault is meant to be a surprise.

use bevy::prelude::*;

use crate::boss::{HandAction, HandSide, Keeper, KeeperBeam, KeeperHand, StalkerLook, StalkerState};
use crate::combat::{Fade, Health, Shake};
use crate::hud::{self, HudSlot, Meter};
use crate::room::RoomGrid;
use crate::vault::VaultTier;
use crate::{Level, PIXEL_SCALE, Rng};

/// The keeper's beams: a hostile violet, well clear of the Gunner's red laser.
const BEAM_COLOR: Color = Color::srgb(0.85, 0.3, 1.0);
const ENRAGED_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);
const HIT_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);
/// The stalker's marks and its wind-up blink: a hot, hostile orange.
const MARK_COLOR: Color = Color::srgb(1.0, 0.4, 0.15);
/// Spent after a lunge, like a guard stalker recovering: dull and bluish.
const RECOVER_COLOR: Color = Color::srgb(0.6, 0.6, 0.75);
/// How far apart the chevrons along the stalker's path sit, and how many
/// there can be.
const CHEVRON_GAP: f32 = 9.0;
const CHEVRONS: usize = 72;
/// Seconds between the afterimages a lunge leaves, and how long each lasts.
const GHOST_GAP: f32 = 0.025;
const GHOST_LIFE: f32 = 0.22;
/// How far from a hand's centre its palm emitter sits, along its aim.
const PALM: f32 = 4.0;
const BAR_WIDTH: f32 = 420.0;
/// Half the height of a guardian's body sprite.
const BODY_HALF: f32 = 20.0;

pub struct BossViewPlugin;

impl Plugin for BossViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BossSeen>()
            .add_observer(dress_keeper)
            .add_observer(dress_hand)
            .add_observer(dress_beam)
            .add_observer(dress_marks)
            .add_systems(Startup, spawn_boss_bar)
            .add_systems(
                Update,
                (
                    watch_vault_room,
                    (hide_boss, draw_keeper, draw_hands, draw_beams, draw_marks, lunge_ghosts, fade_ghosts, update_boss_bar),
                )
                    .chain(),
            );
    }
}

/// Whether the camera is in the vault room, and so whether its guardian, and
/// anything it's doing, may be drawn.
#[derive(Resource, Default)]
struct BossSeen(bool);

/// The keeper's body sprite, a child so it can bob without moving the keeper.
#[derive(Component)]
struct KeeperSprite;

/// A hand's sprite, a child so it can turn to its aim and kick back.
#[derive(Component)]
struct HandSprite;

/// The glow in a hand's palm while it aims or fires its laser.
#[derive(Component)]
struct PalmGlow;

/// The nova orb gathering in a hand's palm.
#[derive(Component)]
struct PalmOrb;

/// Draws a `KeeperBeam`. A separate entity rather than the beam's child, so it
/// doesn't depend on how the beam is placed; it follows the beam and goes with it.
#[derive(Component)]
struct BeamLook(Entity);

/// The thin warning line, the soft glow, or the bright core of a beam.
#[derive(Component, Clone, Copy, PartialEq)]
enum BeamPart {
    Warning,
    Glow,
    Core,
}

/// The flare where a burning beam hits.
#[derive(Component)]
struct BeamImpact;

/// Draws a stalker's `StalkerLook` marks: the path it'll lunge along and a
/// numbered reticle on each pirate it marked. Its children sit in world space.
#[derive(Component)]
struct MarksLook(Entity);

/// One chevron along the stalker's path, pointing the way it'll go.
#[derive(Component)]
struct MarkChevron(usize);

/// One of a chevron's two strokes.
#[derive(Component)]
struct MarkStroke;

/// The reticle on the stalker's `n`th mark, numbered in the order it'll strike.
#[derive(Component)]
struct MarkReticle(usize);

/// A reticle's ring, a child so it can spin and throb under its number.
#[derive(Component)]
struct MarkRing;

/// A fading copy of the stalker left behind as it lunges.
#[derive(Component)]
struct Ghost {
    age: f32,
}

/// The keeper's colour for its vault's tier: the tier's own, softened so the
/// steel still reads.
fn tier_tint(tier: VaultTier) -> Color {
    Srgba::from(tier.color()).mix(&Srgba::WHITE, 0.45).into()
}

/// `a` filtered through `b`, channel by channel.
fn times(a: Srgba, b: Srgba) -> Srgba {
    Srgba::new(a.red * b.red, a.green * b.green, a.blue * b.blue, a.alpha * b.alpha)
}

/// `color` darkened to `amount` of its brightness, keeping its alpha.
fn dim(color: Srgba, amount: f32) -> Srgba {
    Srgba::new(color.red * amount, color.green * amount, color.blue * amount, color.alpha)
}

/// Dressed hidden: `hide_boss` shows it once the camera is in its room.
fn dress_keeper(add: On<Add, Keeper>, mut commands: Commands, assets: Res<AssetServer>, keepers: Query<&Keeper>) {
    let Ok(keeper) = keepers.get(add.entity) else { return };
    commands.entity(add.entity).insert(Visibility::Hidden).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, -9.0, -0.1).with_scale(Vec3::new(3.2, 2.4, 1.0)),
        ));
        parent.spawn((KeeperSprite, Sprite::from_image(assets.load(keeper.kind.sprite()))));
    });
}

fn dress_hand(add: On<Add, KeeperHand>, mut commands: Commands, assets: Res<AssetServer>) {
    commands.entity(add.entity).insert(Visibility::Hidden).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, -6.0, -0.1).with_scale(Vec3::new(0.9, 0.7, 1.0)),
        ));
        parent.spawn((HandSprite, Sprite::from_image(assets.load("sprites/boss/hand.png"))));
        parent.spawn((
            PalmGlow,
            Sprite { image: assets.load("sprites/fx/spark.png"), color: BEAM_COLOR, ..default() },
            Transform::from_xyz(0.0, 0.0, 0.2),
            Visibility::Hidden,
        ));
        parent.spawn((
            PalmOrb,
            Sprite::from_image(assets.load("sprites/fx/nova_orb.png")),
            Transform::from_xyz(0.0, 0.0, 0.3),
            Visibility::Hidden,
        ));
    });
}

fn dress_beam(add: On<Add, KeeperBeam>, mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((Level, BeamLook(add.entity), Transform::default(), Visibility::Hidden))
        .with_children(|parent| {
            for (part, z) in [(BeamPart::Warning, 0.0), (BeamPart::Glow, 0.1), (BeamPart::Core, 0.2)] {
                parent.spawn((part, Sprite::from_color(BEAM_COLOR, Vec2::ONE), Transform::from_xyz(0.0, 0.0, z)));
            }
            parent.spawn((
                BeamImpact,
                Sprite { image: assets.load("sprites/fx/spark.png"), color: BEAM_COLOR, ..default() },
                Transform::from_xyz(0.0, 0.0, 0.3),
            ));
        });
}

/// Where the camera is decides whether the guardian may be seen: from the
/// corridor, even through the gate, there's nothing to see.
fn watch_vault_room(
    grid: Option<Res<RoomGrid>>,
    camera: Option<Single<&Transform, With<Camera2d>>>,
    mut seen: ResMut<BossSeen>,
) {
    let inside = match (grid, camera) {
        (Some(grid), Some(camera)) => grid.in_vault_room(camera.translation.truncate()),
        _ => false,
    };
    if seen.0 != inside {
        seen.0 = inside;
    }
}

/// Shows the guardian's body and hands only while the camera is in its room.
fn hide_boss(seen: Res<BossSeen>, mut bodies: Query<&mut Visibility, Or<(With<Keeper>, With<KeeperHand>)>>) {
    let shown = if seen.0 { Visibility::Inherited } else { Visibility::Hidden };
    for mut visibility in &mut bodies {
        visibility.set_if_neq(shown);
    }
}

/// `tint` seething toward red at time `t`, for a guardian badly hurt.
fn seethe(tint: Srgba, t: f32) -> Srgba {
    let heat = 0.5 + 0.5 * (t * 8.0).sin();
    tint.mix(&Srgba::from(ENRAGED_COLOR), 0.35 + 0.35 * heat)
}

/// Asleep, a guardian sits dim and breathes slowly; enraged, it seethes red;
/// either flashes red when hit, like any guard. Awake, the keeper hovers. The
/// stalker prowls, crouches trembling and blinking as it winds up, stretches
/// as it lunges and slumps, dull, when spent.
fn draw_keeper(
    time: Res<Time>,
    keepers: Query<(&Keeper, &Health, Option<&StalkerLook>, &Transform), Without<KeeperSprite>>,
    mut sprites: Query<(&ChildOf, &mut Sprite, &mut Transform), With<KeeperSprite>>,
) {
    let t = time.elapsed_secs();
    let blink = (t * 30.0).sin() > 0.0;
    for (parent, mut sprite, mut transform) in &mut sprites {
        let Ok((keeper, health, look, body)) = keepers.get(parent.parent()) else { continue };
        let tint = Srgba::from(tier_tint(keeper.tier));
        // Squashing keeps its feet where they were.
        let squash = |scale: Vec2| Vec2::new(0.0, BODY_HALF * (scale.y - 1.0));
        let (color, offset, scale) = if health.is_dead() {
            (dim(tint, 0.4), Vec2::ZERO, Vec2::ONE)
        } else if !keeper.awake {
            (dim(tint, 0.5 + 0.06 * (t * 1.3).sin()), Vec2::ZERO, Vec2::new(1.0, 1.0 + 0.015 * (t * 1.3).sin()))
        } else if let Some(look) = look {
            match look.state {
                StalkerState::WindUp => {
                    let scale = Vec2::new(1.08, 0.86);
                    let tremble = Vec2::new((t * 57.0).sin(), (t * 43.0).cos() * 0.5);
                    let color = if blink { Srgba::from(MARK_COLOR) } else { tint };
                    (color, squash(scale) + tremble, scale)
                }
                StalkerState::Lunge { hit } => {
                    // Stretched along the way it's going.
                    let next = look.marks.get(hit as usize).copied().unwrap_or(body.translation.truncate());
                    let dir = (next - body.translation.truncate()).normalize_or_zero().abs();
                    let scale = Vec2::ONE + 0.3 * dir - 0.12 * dir.yx();
                    let color = if keeper.enraged { seethe(tint, t) } else { tint.mix(&Srgba::WHITE, 0.3) };
                    (color, Vec2::ZERO, scale)
                }
                StalkerState::Recover => {
                    let scale = Vec2::new(1.04, 0.92 + 0.02 * (t * 2.0).sin());
                    (times(tint, Srgba::from(RECOVER_COLOR)), squash(scale), scale)
                }
                StalkerState::Prowl => {
                    // A heavy, swaying gait.
                    let step = Vec2::new(0.0, (t * 4.5).sin().abs());
                    let color = if keeper.enraged { seethe(tint, t) } else { tint };
                    (color, step, Vec2::ONE)
                }
            }
        } else if keeper.enraged {
            (seethe(tint, t), Vec2::new(0.0, 2.5 * (t * 3.4).sin()), Vec2::ONE)
        } else {
            (tint, Vec2::new(0.0, 2.0 * (t * 2.2).sin()), Vec2::ONE)
        };
        sprite.color = if health.flash > 0.0 { HIT_COLOR } else { color.into() };
        transform.translation.x = offset.x.round();
        transform.translation.y = offset.y.round();
        transform.scale = scale.extend(1.0);
    }
}

/// Turns each hand's palm to its aim, lights it while the laser winds up and
/// burns, grows the orb gathering in it, and kicks it back as the orb flies.
#[allow(clippy::type_complexity)]
fn draw_hands(
    time: Res<Time>,
    keepers: Query<&Keeper>,
    hands: Query<&KeeperHand>,
    mut sprites: Query<(&ChildOf, &mut Sprite, &mut Transform), (With<HandSprite>, Without<PalmGlow>, Without<PalmOrb>)>,
    mut glows: Query<(&ChildOf, &mut Sprite, &mut Transform, &mut Visibility), (With<PalmGlow>, Without<PalmOrb>)>,
    mut orbs: Query<(&ChildOf, &mut Transform, &mut Visibility), (With<PalmOrb>, Without<HandSprite>, Without<PalmGlow>)>,
) {
    let t = time.elapsed_secs();
    let catch_up = 1.0 - (-18.0 * time.delta_secs()).exp();
    // There's only ever one keeper on a deck.
    let keeper = keepers.iter().next();
    let tint = Srgba::from(keeper.map_or(Color::WHITE, |k| tier_tint(k.tier)));
    let enraged = keeper.is_some_and(|k| k.enraged);
    let asleep = keeper.is_some_and(|k| !k.awake);

    for (parent, mut sprite, mut transform) in &mut sprites {
        let Ok(hand) = hands.get(parent.parent()) else { continue };
        let aim = hand.aim.try_normalize().unwrap_or(Vec2::X * hand.side.sign());
        sprite.flip_y = hand.side == HandSide::Left;
        transform.rotation = Quat::from_rotation_z(aim.to_angle());
        let kick = if hand.action == HandAction::NovaRecoil { -aim * 4.0 } else { Vec2::ZERO };
        let bob = if asleep { 0.0 } else { 1.5 * (t * 2.6 + hand.side.sign()).sin() };
        let target = kick + Vec2::new(0.0, bob);
        let now = transform.translation.truncate();
        let next = now + (target - now) * catch_up;
        transform.translation.x = next.x;
        transform.translation.y = next.y;
        let color = if asleep {
            dim(tint, 0.5)
        } else if enraged {
            tint.mix(&Srgba::from(ENRAGED_COLOR), 0.3 + 0.3 * (t * 8.0).sin().abs())
        } else {
            tint
        };
        sprite.color = color.into();
    }

    for (parent, mut sprite, mut transform, mut visibility) in &mut glows {
        let Ok(hand) = hands.get(parent.parent()) else { continue };
        let aim = hand.aim.try_normalize().unwrap_or(Vec2::X);
        let (on, scale, alpha) = match hand.action {
            // Winding up: a quickening throb.
            HandAction::LaserAim => (true, 0.7 + 0.3 * (t * 18.0).sin().abs(), 0.6),
            HandAction::LaserFire => (true, 1.3 + 0.2 * (t * 50.0).sin(), 1.0),
            _ => (false, 1.0, 1.0),
        };
        visibility.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        if on {
            transform.translation = (aim * PALM).extend(0.2);
            transform.scale = Vec3::splat(scale);
            sprite.color = BEAM_COLOR.with_alpha(alpha);
        }
    }

    for (parent, mut transform, mut visibility) in &mut orbs {
        let Ok(hand) = hands.get(parent.parent()) else { continue };
        let aim = hand.aim.try_normalize().unwrap_or(Vec2::X);
        let HandAction::NovaCharge { charge } = hand.action else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        // Grows with the charge, trembling harder the fuller it gets.
        let charge = charge.clamp(0.0, 1.0);
        let tremble = charge * 0.6 * Vec2::new((t * 41.0).sin(), (t * 37.0).cos());
        transform.translation = (aim * (PALM + 2.0 + 2.0 * charge) + tremble).extend(0.3);
        transform.scale = Vec3::splat(0.3 + 1.0 * charge + 0.08 * (t * 20.0).sin());
    }
}

/// Lays each beam along its line: a flickering warning thread while the hand
/// aims, then a thick burning core in a soft glow, throwing sparks where it hits.
#[allow(clippy::type_complexity)]
fn draw_beams(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    mut shake: ResMut<Shake>,
    mut rng: Local<Rng>,
    beams: Query<&KeeperBeam>,
    mut looks: Query<(Entity, &BeamLook, &mut Transform, &mut Visibility), Without<BeamPart>>,
    mut parts: Query<(&ChildOf, &BeamPart, &mut Sprite, &mut Transform, &mut Visibility), (Without<BeamLook>, Without<BeamImpact>)>,
    mut impacts: Query<(&ChildOf, &mut Transform, &mut Visibility), (With<BeamImpact>, Without<BeamLook>, Without<BeamPart>)>,
) {
    let t = time.elapsed_secs();
    let flicker = 0.8 + 0.2 * (t * 60.0).sin();
    let blink = 0.5 + 0.5 * (t * 22.0).sin();
    for (entity, look, mut transform, mut visibility) in &mut looks {
        let Ok(beam) = beams.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        *transform = Transform::from_translation(beam.from.extend(14.0))
            .with_rotation(Quat::from_rotation_z(beam.dir.to_angle()));
        visibility.set_if_neq(if seen.0 { Visibility::Inherited } else { Visibility::Hidden });
        if beam.firing && seen.0 {
            shake.0 = shake.0.max(0.2);
            let end = beam.from + beam.dir * beam.length;
            if rng.unit() < 0.6 {
                let spray = Vec2::new(rng.signed(), rng.signed()) * 4.0 - beam.dir * 2.0;
                commands.spawn((
                    Level,
                    Fade { age: 0.0, life: 0.25 },
                    Sprite::from_color(BEAM_COLOR, Vec2::ONE),
                    Transform::from_translation((end + spray).extend(16.0)),
                ));
            }
        }
    }

    for (parent, part, mut sprite, mut transform, mut visibility) in &mut parts {
        let Ok(look) = looks.get(parent.parent()) else { continue };
        let Ok(beam) = beams.get(look.1.0) else { continue };
        let shown = match part {
            BeamPart::Warning => !beam.firing,
            BeamPart::Glow | BeamPart::Core => beam.firing,
        };
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if !shown {
            continue;
        }
        let (width, color) = match part {
            BeamPart::Warning => (1.0, BEAM_COLOR.with_alpha(0.25 + 0.45 * blink)),
            BeamPart::Glow => (10.0 * flicker, BEAM_COLOR.with_alpha(0.35)),
            BeamPart::Core => (3.0, Srgba::from(BEAM_COLOR).mix(&Srgba::WHITE, 0.6).into()),
        };
        sprite.custom_size = Some(Vec2::new(beam.length, width));
        sprite.color = color;
        transform.translation.x = beam.length / 2.0;
    }

    for (parent, mut transform, mut visibility) in &mut impacts {
        let Ok(look) = looks.get(parent.parent()) else { continue };
        let Ok(beam) = beams.get(look.1.0) else { continue };
        visibility.set_if_neq(if beam.firing { Visibility::Inherited } else { Visibility::Hidden });
        transform.translation.x = beam.length;
        transform.scale = Vec3::splat(1.6 + 0.6 * flicker);
    }
}

fn dress_marks(add: On<Add, StalkerLook>, mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((Level, MarksLook(add.entity), Transform::from_xyz(0.0, 0.0, 13.0), Visibility::Hidden))
        .with_children(|parent| {
            for i in 0..CHEVRONS {
                parent.spawn((MarkChevron(i), Transform::default(), Visibility::Hidden)).with_children(|chevron| {
                    // Two short strokes meeting in a point: ">".
                    for side in [-1.0f32, 1.0] {
                        chevron.spawn((
                            MarkStroke,
                            Sprite::from_color(MARK_COLOR, Vec2::new(4.0, 1.0)),
                            Transform::from_xyz(-1.4, side * 1.4, 0.0)
                                .with_rotation(Quat::from_rotation_z(-side * std::f32::consts::FRAC_PI_4)),
                        ));
                    }
                });
            }
            for n in 0..3 {
                parent
                    .spawn((MarkReticle(n), Transform::from_xyz(0.0, 0.0, 0.5), Visibility::Hidden))
                    .with_children(|reticle| {
                        reticle.spawn((
                            MarkRing,
                            Sprite { image: assets.load("sprites/ui/crosshair.png"), color: MARK_COLOR, ..default() },
                        ));
                        reticle.spawn((
                            Text2d::new(format!("{}", n + 1)),
                            TextFont::from_font_size(6.0 * PIXEL_SCALE),
                            TextColor(MARK_COLOR),
                            Transform::from_xyz(0.0, 10.0, 0.1).with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
                        ));
                    });
            }
        });
}

/// While the stalker winds up and lunges, lays a trail of marching chevrons
/// from it through each pirate it marked, in order, and a throbbing numbered
/// reticle on each mark it has yet to reach.
#[allow(clippy::type_complexity)]
fn draw_marks(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    stalkers: Query<(&StalkerLook, &Health, &Transform), (With<Keeper>, Without<MarksLook>)>,
    mut looks: Query<(Entity, &MarksLook, &mut Visibility), (Without<MarkChevron>, Without<MarkReticle>)>,
    mut chevrons: Query<(&ChildOf, &MarkChevron, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkReticle>, Without<MarkRing>)>,
    mut reticles: Query<(&ChildOf, &MarkReticle, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkChevron>, Without<MarkRing>)>,
    mut rings: Query<(&ChildOf, &mut Transform, &mut Sprite), (With<MarkRing>, Without<Keeper>)>,
    mut strokes: Query<&mut Sprite, (With<MarkStroke>, Without<MarkRing>)>,
) {
    let t = time.elapsed_secs();
    let blink = 0.55 + 0.45 * (t * 14.0).sin();

    for (entity, look, mut visibility) in &mut looks {
        let Ok((stalker, health, _)) = stalkers.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let marking = matches!(stalker.state, StalkerState::WindUp | StalkerState::Lunge { .. });
        let shown = seen.0 && marking && !health.is_dead() && !stalker.marks.is_empty();
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }

    // Which marks are still ahead of it, and the path through them.
    let path = |look: &MarksLook| {
        let (stalker, _, body) = stalkers.get(look.0).ok()?;
        let reached = match stalker.state {
            StalkerState::Lunge { hit } => hit as usize,
            _ => 0,
        };
        let from = body.translation.truncate();
        Some((reached, std::iter::once(from).chain(stalker.marks.iter().skip(reached).copied()).collect::<Vec<_>>()))
    };

    for (parent, chevron, mut transform, mut visibility) in &mut chevrons {
        let Ok((_, look, _)) = looks.get(parent.parent()) else { continue };
        let Some((_, points)) = path(look) else { continue };
        // Walk the path from the body, chevrons marching along it.
        let mut along = (t * 24.0) % CHEVRON_GAP + chevron.0 as f32 * CHEVRON_GAP;
        let mut spot = None;
        for leg in points.windows(2) {
            let length = leg[0].distance(leg[1]);
            if along < length {
                let dir = (leg[1] - leg[0]) / length.max(0.01);
                spot = Some((leg[0] + dir * along, dir));
                break;
            }
            along -= length;
        }
        let Some((at, dir)) = spot else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at.extend(0.0);
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
    }

    for (parent, reticle, mut transform, mut visibility) in &mut reticles {
        let Ok((_, look, _)) = looks.get(parent.parent()) else { continue };
        let Some((reached, _)) = path(look) else { continue };
        let Ok((stalker, ..)) = stalkers.get(look.0) else { continue };
        let mark = stalker.marks.get(reticle.0).filter(|_| reticle.0 >= reached);
        visibility.set_if_neq(if mark.is_some() { Visibility::Inherited } else { Visibility::Hidden });
        if let Some(mark) = mark {
            transform.translation = mark.extend(0.5);
        }
    }

    // The chevrons and the rings blink together.
    let color = MARK_COLOR.with_alpha(blink);
    for (parent, mut transform, mut sprite) in &mut rings {
        let Ok((_, reticle, ..)) = reticles.get(parent.parent()) else { continue };
        let beat = (t * 10.0 - reticle.0 as f32 * 1.2).sin();
        transform.rotation = Quat::from_rotation_z(t * 2.5);
        transform.scale = Vec3::splat(1.4 + 0.25 * beat);
        sprite.color = color;
    }
    for mut sprite in &mut strokes {
        sprite.color = color;
    }
}
/// Leaves fading copies of the stalker behind it as it lunges.
fn lunge_ghosts(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    assets: Res<AssetServer>,
    mut wait: Local<f32>,
    stalkers: Query<(&Keeper, &StalkerLook, &Transform)>,
) {
    *wait -= time.delta_secs();
    if !seen.0 || *wait > 0.0 {
        return;
    }
    for (keeper, look, transform) in &stalkers {
        if !matches!(look.state, StalkerState::Lunge { .. }) {
            continue;
        }
        *wait = GHOST_GAP;
        let color = Srgba::from(tier_tint(keeper.tier)).mix(&Srgba::from(MARK_COLOR), 0.5);
        commands.spawn((
            Level,
            Ghost { age: 0.0 },
            Sprite { image: assets.load(keeper.kind.sprite()), color: color.with_alpha(0.5).into(), ..default() },
            Transform::from_translation(transform.translation - Vec3::Z * 0.5),
        ));
    }
}

/// Fades each afterimage out, and drops them all if the camera leaves the room.
fn fade_ghosts(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    mut ghosts: Query<(Entity, &mut Ghost, &mut Sprite)>,
) {
    for (entity, mut ghost, mut sprite) in &mut ghosts {
        ghost.age += time.delta_secs();
        if ghost.age >= GHOST_LIFE || !seen.0 {
            commands.entity(entity).despawn();
        } else {
            sprite.color.set_alpha(0.5 * (1.0 - ghost.age / GHOST_LIFE));
        }
    }
}

/// The guardian's health bar across the top of the screen while it's awake and
/// the camera is in its room.
#[derive(Component)]
struct BossBar;

#[derive(Component)]
struct BossName;

#[derive(Component)]
struct BossTier;

/// "ENRAGED", blinking, once it's badly hurt.
#[derive(Component)]
struct BossRage;

#[derive(Component)]
struct BossMeter;

fn spawn_boss_bar(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Boss)).with_child((
        BossBar,
        hud::panel_accent(
            Node {
                width: Val::Px(BAR_WIDTH),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                display: Display::None,
                ..default()
            },
            hud::EDGE,
        ),
        children![
            (
                Node {
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Baseline,
                    ..default()
                },
                children![
                    (BossName, hud::text("", 16.0, hud::TEXT), TextLayout::no_wrap()),
                    (BossTier, hud::text("", 11.0, hud::DIM), TextLayout::no_wrap()),
                    Node { flex_grow: 1.0, ..default() },
                    (BossRage, hud::text("ENRAGED", 12.0, ENRAGED_COLOR), TextLayout::no_wrap()),
                ],
            ),
            (BossMeter, hud::meter(BAR_WIDTH - 18.0, 9.0, hud::TEXT)),
        ],
    ));
}

#[allow(clippy::type_complexity)]
fn update_boss_bar(
    time: Res<Time>,
    seen: Res<BossSeen>,
    keepers: Query<(&Keeper, &Health)>,
    mut bar: Single<(&mut Node, &mut BorderColor), With<BossBar>>,
    mut name: Single<(&mut Text, &mut TextColor), (With<BossName>, Without<BossTier>, Without<BossRage>)>,
    mut tier: Single<(&mut Text, &mut TextColor), (With<BossTier>, Without<BossName>, Without<BossRage>)>,
    mut rage: Single<&mut TextColor, (With<BossRage>, Without<BossName>, Without<BossTier>)>,
    mut meter: Single<&mut Meter, With<BossMeter>>,
) {
    let fighting = keepers.iter().find(|(k, h)| seen.0 && k.awake && !h.is_dead());
    let (node, border) = &mut *bar;
    let display = if fighting.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some((keeper, health)) = fighting else { return };

    let color = keeper.tier.color();
    let accent = if keeper.enraged { ENRAGED_COLOR } else { color };
    border.set_if_neq(BorderColor { top: accent, ..BorderColor::all(hud::EDGE) });
    let (text, tint) = &mut *name;
    if text.0 != keeper.kind.name() {
        text.0 = keeper.kind.name().into();
    }
    tint.set_if_neq(TextColor(color));
    let label = format!("{} VAULT", keeper.tier.name());
    let (text, tint) = &mut *tier;
    if text.0 != label {
        text.0 = label;
    }
    tint.set_if_neq(TextColor(hud::DIM));
    let blink = (time.elapsed_secs() * 6.0).sin() > 0.0;
    let rage_color = if !keeper.enraged {
        Color::NONE
    } else if blink {
        ENRAGED_COLOR
    } else {
        ENRAGED_COLOR.with_alpha(0.5)
    };
    rage.set_if_neq(TextColor(rage_color));
    meter.frac = (health.current / health.max.max(1.0)).clamp(0.0, 1.0);
    meter.color = accent;
}
