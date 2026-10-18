//! How a vault's guardian looks: the keeper's body, hands, beams and charging
//! orbs, the stalker's crouch, routes and lunge, its rampage lanes, the
//! brute's stomp, slam and roar, and the health bar while the crew fights any of them. None of it shows unless the camera is in the vault room:
//! what guards a vault is meant to be a surprise.

use bevy::prelude::*;

use crate::boss::{
    BRUTE_SLAM_RADIUS, BruteLook, BruteState, HandAction, HandSide, Keeper, KeeperBeam, KeeperHand, StalkerLook, StalkerState,
};
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
/// How far apart the chevrons along the stalker's routes sit, and how many
/// there can be across all of them.
const CHEVRON_GAP: f32 = 9.0;
const CHEVRONS: usize = 240;
/// How many marked pirates its routes can show reticles on at once.
const RETICLES: usize = 12;
/// Roughly how long a wind-up lasts, for its candidate routes to brighten
/// over, and how long the route it takes flashes as it goes.
const WIND_UP_LOOK: f32 = 0.85;
const CHOSEN_FLASH: f32 = 0.25;
/// How wide a rampage lane's danger strip is, how far apart its chevrons sit
/// and how many there can be, and how long a warning takes to reach full heat.
const LANE_WIDTH: f32 = 36.0;
const LANE_ARROW_GAP: f32 = 24.0;
const LANE_ARROWS: usize = 40;
const LANE_HEAT: f32 = 0.6;
/// Seconds between the afterimages a lunge leaves, and how long each lasts.
const GHOST_GAP: f32 = 0.025;
const GHOST_LIFE: f32 = 0.22;
/// The brute's roar and the spots its guards will appear at: an alarm amber.
const SUMMON_COLOR: Color = Color::srgb(1.0, 0.75, 0.2);
/// Roughly how long the brute's fists stay raised (and, enraged, how long),
/// and how long it roars, for its slam disc and summoning sigils to fill over;
/// and how long the slam's flash lingers once it lands.
const SLAM_LOOK: f32 = 0.8;
const ENRAGED_SLAM_LOOK: f32 = 0.65;
const ROAR_LOOK: f32 = 1.2;
const SLAM_FLASH: f32 = 0.3;
/// How many summoning sigils one roar can show at once.
const SIGILS: usize = 8;
/// How far across a sigil grows by the end of the roar.
const SIGIL_RADIUS: f32 = 10.0;
/// The diameters, in pixels, of `shock_ring.png` and of the disc in `explosion.png`.
const RING_SIZE: f32 = 113.0;
const DISC_SIZE: f32 = 52.0;
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
            .add_observer(dress_lane)
            .add_observer(dress_brute)
            .add_systems(Startup, spawn_boss_bar)
            .add_systems(
                Update,
                (
                    watch_vault_room,
                    (
                        hide_boss,
                        draw_keeper,
                        draw_hands,
                        draw_beams,
                        draw_marks,
                        draw_lane,
                        draw_brute,
                        lunge_ghosts,
                        fade_ghosts,
                        update_boss_bar,
                    ),
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

/// Draws a stalker's `StalkerLook` routes: the ways it might lunge (or the
/// one it took) and a numbered reticle on each pirate they pass through. Its
/// children sit in world space.
#[derive(Component)]
struct MarksLook(Entity);

/// How long a guardian has been doing what it's doing, as far as a look can
/// tell: reset whenever `key` changes.
#[derive(Component)]
struct StateClock<K: Send + Sync + 'static> {
    key: Option<K>,
    since: f32,
}

impl<K: Send + Sync + 'static> Default for StateClock<K> {
    fn default() -> Self {
        Self { key: None, since: 0.0 }
    }
}

impl<K: PartialEq + Copy + Send + Sync + 'static> StateClock<K> {
    /// Seconds spent in `key`, counting this frame's `dt`.
    fn tick(&mut self, key: K, dt: f32) -> f32 {
        if self.key == Some(key) {
            self.since += dt;
        } else {
            self.key = Some(key);
            self.since = 0.0;
        }
        self.since
    }
}

/// The `index`th chevron along the stalker's routes, pointing the way it'd go.
#[derive(Component)]
struct MarkChevron {
    index: usize,
    color: Color,
}

/// One of a chevron's two strokes.
#[derive(Component)]
struct MarkStroke;

/// The `index`th reticle, on a pirate a route passes through; `label` is where
/// it falls in each such route, `first` the earliest.
#[derive(Component)]
struct MarkReticle {
    index: usize,
    first: usize,
    color: Color,
    label: String,
}

/// A reticle's ring, a child so it can spin and throb under its number.
#[derive(Component)]
struct MarkRing;

/// A reticle's number.
#[derive(Component)]
struct MarkNumber;

/// Draws a stalker's rampage lane, from wall to wall; laid along it, so its
/// children sit in lane space.
#[derive(Component)]
struct LaneLook(Entity);

/// The wide danger strip down a lane, or one of its two edges (`-1` or `1`).
#[derive(Component, Clone, Copy, PartialEq)]
enum LanePart {
    Strip,
    Edge(f32),
}

/// One of the big chevrons racing down a lane the way the stalker will charge.
#[derive(Component)]
struct LaneArrow(usize);

/// One of a lane chevron's two strokes.
#[derive(Component)]
struct LaneStroke;

/// Draws a brute's `BruteLook`: the disc its slam will land on, and a sigil
/// at each spot a guard it's calling will appear. Its children sit in world space.
#[derive(Component)]
struct BruteMarks(Entity);

/// The faint disc the slam reaches, the brighter one filling it as the fists
/// rise, or the dashed ring round its edge.
#[derive(Component, Clone, Copy, PartialEq)]
enum SlamPart {
    Base,
    Fill,
    Edge,
}

/// The `0`th, `1`st... summoning sigil, on the matching `BruteLook::summons` spot.
#[derive(Component)]
struct Sigil(usize);

/// A sigil's outer ring, the smaller ring turning the other way inside it, or
/// the glow at its heart.
#[derive(Component, Clone, Copy, PartialEq)]
enum SigilPart {
    Ring,
    Inner,
    Core,
}

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
/// as it lunges and slumps, dull, when spent. Before a rampage it rears up
/// tall, shaking and flashing; rampaging it stretches along its lane as it
/// dashes and squashes, coiled and blinking, braced against each wall. The
/// brute stomps heavily; raising its fists it stretches up tall, trembling and
/// flashing; after a slam it slumps, squashed and dull; roaring it swells and
/// shakes, glowing amber.
#[allow(clippy::type_complexity)]
fn draw_keeper(
    time: Res<Time>,
    keepers: Query<(&Keeper, &Health, Option<&StalkerLook>, Option<&BruteLook>, &Transform), Without<KeeperSprite>>,
    mut sprites: Query<(&ChildOf, &mut Sprite, &mut Transform), With<KeeperSprite>>,
) {
    let t = time.elapsed_secs();
    let blink = (t * 30.0).sin() > 0.0;
    for (parent, mut sprite, mut transform) in &mut sprites {
        let Ok((keeper, health, look, brute, body)) = keepers.get(parent.parent()) else { continue };
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
                    let next = look.routes.first().and_then(|route| route.get(hit as usize)).copied().unwrap_or(body.translation.truncate());
                    let dir = (next - body.translation.truncate()).normalize_or_zero().abs();
                    let scale = Vec2::ONE + 0.3 * dir - 0.12 * dir.yx();
                    let color = if keeper.enraged { seethe(tint, t) } else { tint.mix(&Srgba::WHITE, 0.3) };
                    (color, Vec2::ZERO, scale)
                }
                StalkerState::Recover => {
                    let scale = Vec2::new(1.04, 0.92 + 0.02 * (t * 2.0).sin());
                    (times(tint, Srgba::from(RECOVER_COLOR)), squash(scale), scale)
                }
                StalkerState::Rear => {
                    let scale = Vec2::new(0.86, 1.22);
                    let tremble = 1.2 * Vec2::new((t * 61.0).sin(), (t * 47.0).cos() * 0.5);
                    let color = if blink {
                        Srgba::from(MARK_COLOR)
                    } else if keeper.enraged {
                        seethe(tint, t)
                    } else {
                        tint.mix(&Srgba::WHITE, 0.5)
                    };
                    (color, squash(scale) + tremble, scale)
                }
                StalkerState::Rampage { moving, .. } => {
                    let dir = look.lane.map_or(Vec2::X, |[from, to]| (to - from).normalize_or_zero()).abs();
                    if moving {
                        // Stretched along its lane, hurtling.
                        let scale = Vec2::ONE + 0.38 * dir - 0.15 * dir.yx();
                        let color = if keeper.enraged { seethe(tint, t) } else { tint.mix(&Srgba::from(MARK_COLOR), 0.35) };
                        (color, Vec2::ZERO, scale)
                    } else {
                        // Squashed against the wall, coiled for the next dash.
                        let scale = Vec2::ONE - 0.2 * dir + 0.14 * dir.yx();
                        let tremble = 0.6 * Vec2::new((t * 57.0).sin(), (t * 43.0).cos() * 0.5);
                        let color = if blink { Srgba::from(MARK_COLOR) } else { tint };
                        (color, squash(scale) + tremble, scale)
                    }
                }
                StalkerState::Prowl => {
                    // A heavy, swaying gait.
                    let step = Vec2::new(0.0, (t * 4.5).sin().abs());
                    let color = if keeper.enraged { seethe(tint, t) } else { tint };
                    (color, step, Vec2::ONE)
                }
            }
        } else if let Some(brute) = brute {
            match brute.state {
                BruteState::Plod => {
                    // Heavy stomps: up off each foot, squashing as it lands.
                    let lift = (t * 3.5).sin().abs();
                    let scale = Vec2::new(1.0 + 0.05 * (1.0 - lift), 1.0 - 0.05 * (1.0 - lift));
                    let sway = Vec2::new(0.6 * (t * 3.5).cos(), 2.0 * lift);
                    let color = if keeper.enraged { seethe(tint, t) } else { tint };
                    (color, squash(scale) + sway, scale)
                }
                BruteState::WindUp => {
                    // Fists raised over its head, straining.
                    let scale = Vec2::new(0.9, 1.18);
                    let tremble = Vec2::new((t * 57.0).sin(), (t * 43.0).cos() * 0.5);
                    let color = if blink {
                        Srgba::from(MARK_COLOR)
                    } else if keeper.enraged {
                        seethe(tint, t)
                    } else {
                        tint.mix(&Srgba::WHITE, 0.4)
                    };
                    (color, squash(scale) + tremble, scale)
                }
                BruteState::Recover => {
                    let scale = Vec2::new(1.14, 0.84 + 0.02 * (t * 2.0).sin());
                    (times(tint, Srgba::from(RECOVER_COLOR)), squash(scale), scale)
                }
                BruteState::Summon => {
                    // Roaring: chest swelling, shaking with it.
                    let swell = 1.1 + 0.04 * (t * 18.0).sin();
                    let scale = Vec2::new(swell, swell);
                    let shake = 1.2 * Vec2::new((t * 53.0).sin(), (t * 41.0).cos() * 0.5);
                    let heat = 0.45 + 0.25 * (t * 12.0).sin();
                    (tint.mix(&Srgba::from(SUMMON_COLOR), heat), squash(scale) + shake, scale)
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
        .spawn((Level, MarksLook(add.entity), StateClock::<StalkerState>::default(), Transform::from_xyz(0.0, 0.0, 13.0), Visibility::Hidden))
        .with_children(|parent| {
            for i in 0..CHEVRONS {
                parent.spawn((MarkChevron { index: i, color: MARK_COLOR }, Transform::default(), Visibility::Hidden)).with_children(
                    |chevron| {
                        // Two short strokes meeting in a point: ">".
                        for side in [-1.0f32, 1.0] {
                            chevron.spawn((
                                MarkStroke,
                                Sprite::from_color(MARK_COLOR, Vec2::new(4.0, 1.0)),
                                Transform::from_xyz(-1.4, side * 1.4, 0.0)
                                    .with_rotation(Quat::from_rotation_z(-side * std::f32::consts::FRAC_PI_4)),
                            ));
                        }
                    },
                );
            }
            for n in 0..RETICLES {
                parent
                    .spawn((
                        MarkReticle { index: n, first: 0, color: MARK_COLOR, label: String::new() },
                        Transform::from_xyz(0.0, 0.0, 0.5),
                        Visibility::Hidden,
                    ))
                    .with_children(|reticle| {
                        reticle.spawn((
                            MarkRing,
                            Sprite { image: assets.load("sprites/ui/crosshair.png"), color: MARK_COLOR, ..default() },
                        ));
                        reticle.spawn((
                            MarkNumber,
                            Text2d::new(""),
                            TextFont::from_font_size(6.0 * PIXEL_SCALE),
                            TextColor(MARK_COLOR),
                            Transform::from_xyz(0.0, 10.0, 0.1).with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
                        ));
                    });
            }
        });
}

/// A point some route turns at (or ends on), and its number, if it has one.
struct PlannedMark {
    at: Vec2,
    label: String,
    first: usize,
    color: Color,
}

/// Where a stalker's chevrons and reticles go this frame.
#[derive(Default)]
struct MarksPlan {
    spots: Vec<(Vec2, Vec2, Color)>,
    marks: Vec<PlannedMark>,
}

/// Lays out the chevrons along every route the stalker shows, from `from`,
/// and a reticle on each point still ahead of it. Winding up, the candidate
/// routes all look alike, dim and flickering out of step, quickening and
/// brightening as the wind-up runs out, their reticles small and unnumbered;
/// lunging, the one it took flashes white the moment it goes, then blinks as
/// before with its points numbered in the order it'll reach them.
fn plan_marks(stalker: &StalkerLook, from: Vec2, since: f32, t: f32) -> MarksPlan {
    let mut plan = MarksPlan::default();
    let choosing = stalker.state == StalkerState::WindUp;
    let reached = match stalker.state {
        StalkerState::Lunge { hit } => hit as usize,
        _ => 0,
    };
    let heat = (since / WIND_UP_LOOK).clamp(0.0, 1.0);
    for (r, route) in stalker.routes.iter().enumerate() {
        let color: Color = if choosing {
            let flicker = 0.5 + 0.5 * (t * (8.0 + 22.0 * heat) + r as f32 * 2.1).sin();
            MARK_COLOR.with_alpha((0.2 + 0.55 * heat) * (0.55 + 0.45 * flicker))
        } else {
            let flash = (1.0 - since / CHOSEN_FLASH).max(0.0);
            let blink = 0.55 + 0.45 * (t * 14.0).sin();
            Srgba::from(MARK_COLOR).mix(&Srgba::WHITE, flash).with_alpha(blink.max(flash)).into()
        };
        // Walk the route from the body, chevrons marching along it.
        let points: Vec<Vec2> = std::iter::once(from).chain(route.iter().skip(reached).copied()).collect();
        let mut along = (t * 24.0) % CHEVRON_GAP;
        for leg in points.windows(2) {
            let length = leg[0].distance(leg[1]);
            let dir = (leg[1] - leg[0]) / length.max(0.01);
            while along < length {
                plan.spots.push((leg[0] + dir * along, dir, color));
                along += CHEVRON_GAP;
            }
            along -= length;
        }
        // Routes through the same point share its reticle.
        for (n, &mark) in route.iter().enumerate().skip(reached) {
            if !plan.marks.iter().any(|m| m.at.distance(mark) < 2.0) {
                let label = if choosing { String::new() } else { (n + 1).to_string() };
                plan.marks.push(PlannedMark { at: mark, label, first: n, color });
            }
        }
    }
    plan
}

/// While the stalker winds up, lays a trail of marching chevrons along every
/// route it might lunge, and a small throbbing reticle where each turns, none
/// favoured, so nobody knows which it'll take. Once it lunges, only the route
/// it took is left, its points numbered in the order it'll reach them.
#[allow(clippy::type_complexity)]
fn draw_marks(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    stalkers: Query<(&StalkerLook, &Health, &Transform), (With<Keeper>, Without<MarksLook>)>,
    mut looks: Query<(Entity, &MarksLook, &mut StateClock<StalkerState>, &mut Visibility), (Without<MarkChevron>, Without<MarkReticle>)>,
    mut chevrons: Query<(&ChildOf, &mut MarkChevron, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkReticle>, Without<MarkRing>)>,
    mut reticles: Query<(&ChildOf, &mut MarkReticle, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkChevron>, Without<MarkRing>)>,
    mut rings: Query<(&ChildOf, &mut Transform, &mut Sprite), (With<MarkRing>, Without<Keeper>)>,
    mut numbers: Query<(&ChildOf, &mut Text2d, &mut TextColor), With<MarkNumber>>,
    mut strokes: Query<(&ChildOf, &mut Sprite), (With<MarkStroke>, Without<MarkRing>)>,
) {
    let t = time.elapsed_secs();
    let mut plans = Vec::new();
    for (entity, look, mut clock, mut visibility) in &mut looks {
        let Ok((stalker, health, body)) = stalkers.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        // Each hit of a lunge is still the same lunge.
        let key = match stalker.state {
            StalkerState::Lunge { .. } => StalkerState::Lunge { hit: 0 },
            state => state,
        };
        let since = clock.tick(key, time.delta_secs());
        let marking = matches!(stalker.state, StalkerState::WindUp | StalkerState::Lunge { .. });
        let shown = seen.0 && marking && !health.is_dead() && stalker.routes.iter().any(|route| !route.is_empty());
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if shown {
            plans.push((entity, plan_marks(stalker, body.translation.truncate(), since, t)));
        }
    }
    let plan = |look: Entity| plans.iter().find(|(entity, _)| *entity == look).map(|(_, plan)| plan);

    for (parent, mut chevron, mut transform, mut visibility) in &mut chevrons {
        let spot = plan(parent.parent()).and_then(|plan| plan.spots.get(chevron.index));
        let Some(&(at, dir, color)) = spot else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at.extend(0.0);
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
        chevron.color = color;
    }
    for (parent, mut sprite) in &mut strokes {
        let Ok((_, chevron, ..)) = chevrons.get(parent.parent()) else { continue };
        sprite.color = chevron.color;
    }

    for (parent, mut reticle, mut transform, mut visibility) in &mut reticles {
        let mark = plan(parent.parent()).and_then(|plan| plan.marks.get(reticle.index));
        let Some(mark) = mark else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = mark.at.extend(0.5);
        if reticle.label != mark.label {
            reticle.label.clone_from(&mark.label);
        }
        reticle.first = mark.first;
        reticle.color = mark.color;
    }
    for (parent, mut transform, mut sprite) in &mut rings {
        let Ok((_, reticle, ..)) = reticles.get(parent.parent()) else { continue };
        let beat = (t * 10.0 - reticle.first as f32 * 1.2).sin();
        transform.rotation = Quat::from_rotation_z(t * 2.5);
        // Candidates' reticles (unnumbered) sit smaller than the chosen route's.
        let size = if reticle.label.is_empty() { 1.0 } else { 1.4 };
        transform.scale = Vec3::splat(size + 0.25 * beat);
        sprite.color = reticle.color;
    }
    for (parent, mut text, mut color) in &mut numbers {
        let Ok((_, reticle, ..)) = reticles.get(parent.parent()) else { continue };
        if text.0 != reticle.label {
            text.0.clone_from(&reticle.label);
        }
        color.set_if_neq(TextColor(reticle.color));
    }
}

fn dress_lane(add: On<Add, StalkerLook>, mut commands: Commands) {
    commands
        .spawn((Level, LaneLook(add.entity), StateClock::<StalkerState>::default(), Transform::from_xyz(0.0, 0.0, 12.5), Visibility::Hidden))
        .with_children(|parent| {
            for (part, z) in [(LanePart::Strip, 0.0), (LanePart::Edge(-1.0), 0.1), (LanePart::Edge(1.0), 0.1)] {
                parent.spawn((part, Sprite::from_color(MARK_COLOR, Vec2::ONE), Transform::from_xyz(0.0, 0.0, z)));
            }
            for i in 0..LANE_ARROWS {
                parent
                    .spawn((LaneArrow(i), Transform::from_xyz(0.0, 0.0, 0.2).with_scale(Vec3::splat(2.0)), Visibility::Hidden))
                    .with_children(|arrow| {
                        for side in [-1.0f32, 1.0] {
                            arrow.spawn((
                                LaneStroke,
                                Sprite::from_color(MARK_COLOR, Vec2::new(4.0, 1.0)),
                                Transform::from_xyz(-1.4, side * 1.4, 0.0)
                                    .with_rotation(Quat::from_rotation_z(-side * std::f32::consts::FRAC_PI_4)),
                            ));
                        }
                    });
            }
        });
}

/// How a rampage lane looks this frame: how long it is, whether it's a
/// warning (the stalker rearing or braced to go) and how hot it burns.
struct LanePlan {
    length: f32,
    warning: bool,
    pulse: f32,
    color: Color,
}

/// While the stalker rears up or rampages, lays its lane wall to wall as a
/// wide danger strip. Before a dash it throbs, quicker and whiter the longer
/// it's held, with big chevrons racing the way it'll charge; while it dashes
/// the strip just smoulders.
#[allow(clippy::type_complexity)]
fn draw_lane(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    stalkers: Query<(&StalkerLook, &Health), With<Keeper>>,
    mut looks: Query<(Entity, &LaneLook, &mut StateClock<StalkerState>, &mut Transform, &mut Visibility), (Without<LanePart>, Without<LaneArrow>)>,
    mut parts: Query<(&ChildOf, &LanePart, &mut Sprite, &mut Transform), (Without<LaneLook>, Without<LaneArrow>, Without<LaneStroke>)>,
    mut arrows: Query<(&ChildOf, &LaneArrow, &mut Transform, &mut Visibility), (Without<LaneLook>, Without<LanePart>)>,
    mut strokes: Query<(&ChildOf, &mut Sprite), (With<LaneStroke>, Without<LanePart>)>,
) {
    let t = time.elapsed_secs();
    let mut plans = Vec::new();
    for (entity, look, mut clock, mut transform, mut visibility) in &mut looks {
        let Ok((stalker, health)) = stalkers.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let since = clock.tick(stalker.state, time.delta_secs());
        let warning = match stalker.state {
            StalkerState::Rear | StalkerState::Rampage { moving: false, .. } => true,
            StalkerState::Rampage { moving: true, .. } => false,
            _ => {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
        };
        let Some([from, to]) = stalker.lane.filter(|_| seen.0 && !health.is_dead()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let length = from.distance(to);
        let dir = (to - from) / length.max(0.01);
        transform.translation = ((from + to) / 2.0).extend(12.5);
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
        let heat = (since / LANE_HEAT).clamp(0.0, 1.0);
        let pulse = if warning { 0.5 + 0.5 * (t * (10.0 + 20.0 * heat)).sin() } else { 0.0 };
        let color = Srgba::from(MARK_COLOR).mix(&Srgba::WHITE, 0.45 * heat * pulse).into();
        plans.push((entity, LanePlan { length, warning, pulse: pulse * (0.5 + 0.5 * heat), color }));
    }
    let plan = |look: Entity| plans.iter().find(|(entity, _)| *entity == look).map(|(_, plan)| plan);

    for (parent, part, mut sprite, mut transform) in &mut parts {
        let Some(lane) = plan(parent.parent()) else { continue };
        let (width, y, alpha) = match (*part, lane.warning) {
            (LanePart::Strip, true) => (LANE_WIDTH, 0.0, 0.12 + 0.2 * lane.pulse),
            (LanePart::Strip, false) => (LANE_WIDTH, 0.0, 0.07),
            (LanePart::Edge(side), true) => (1.0, side * LANE_WIDTH / 2.0, 0.4 + 0.6 * lane.pulse),
            (LanePart::Edge(side), false) => (1.0, side * LANE_WIDTH / 2.0, 0.15),
        };
        sprite.custom_size = Some(Vec2::new(lane.length, width));
        sprite.color = lane.color.with_alpha(alpha);
        transform.translation.y = y;
    }

    for (parent, arrow, mut transform, mut visibility) in &mut arrows {
        let along = (t * 90.0) % LANE_ARROW_GAP + arrow.0 as f32 * LANE_ARROW_GAP;
        // Only while it's about to go, and clear of the wall at the far end.
        let lane = plan(parent.parent()).filter(|lane| lane.warning && along < lane.length - LANE_ARROW_GAP / 2.0);
        let Some(lane) = lane else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation.x = along - lane.length / 2.0;
    }
    for (parent, mut sprite) in &mut strokes {
        let Ok((arrow, ..)) = arrows.get(parent.parent()) else { continue };
        let Some(lane) = plan(arrow.parent()) else { continue };
        sprite.color = lane.color.with_alpha(0.4 + 0.6 * lane.pulse);
    }
}

fn dress_brute(add: On<Add, BruteLook>, mut commands: Commands, assets: Res<AssetServer>) {
    let disc = assets.load("sprites/fx/explosion.png");
    let ring = assets.load("sprites/fx/shock_ring.png");
    commands
        .spawn((
            Level,
            BruteMarks(add.entity),
            StateClock::<BruteState>::default(),
            Transform::from_xyz(0.0, 0.0, 12.5),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            for (part, image) in [(SlamPart::Base, &disc), (SlamPart::Fill, &disc), (SlamPart::Edge, &ring)] {
                parent.spawn((part, Sprite::from_image(image.clone()), Transform::default(), Visibility::Hidden));
            }
            for i in 0..SIGILS {
                parent.spawn((Sigil(i), Transform::default(), Visibility::Hidden)).with_children(|sigil| {
                    for (part, image, z) in [(SigilPart::Core, &disc, 0.0), (SigilPart::Ring, &ring, 0.1), (SigilPart::Inner, &ring, 0.2)] {
                        sigil.spawn((part, Sprite::from_image(image.clone()), Transform::from_xyz(0.0, 0.0, z)));
                    }
                });
            }
        });
}

/// How a brute's marks look this frame.
struct BrutePlan {
    state: BruteState,
    at: Vec2,
    summons: Vec<Vec2>,
    /// How far through its wind-up or roar it is, `0..=1`.
    progress: f32,
    since: f32,
}

/// While the brute raises its fists, lays the disc its slam will land on: a
/// faint disc under a dashed edge, throbbing quicker, with a brighter disc
/// filling it from the middle as the fists rise, near white just before they
/// fall. When they land it flashes and fades. While it roars, a sigil opens on
/// each spot a guard will appear, spinning and growing until they step out.
#[allow(clippy::type_complexity)]
fn draw_brute(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    brutes: Query<(&Keeper, &BruteLook, &Health)>,
    mut looks: Query<(Entity, &BruteMarks, &mut StateClock<BruteState>, &mut Visibility), (Without<SlamPart>, Without<Sigil>)>,
    mut slams: Query<
        (&ChildOf, &SlamPart, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<BruteMarks>, Without<Sigil>, Without<SigilPart>),
    >,
    mut sigils: Query<(&ChildOf, &Sigil, &mut Transform, &mut Visibility), (Without<BruteMarks>, Without<SlamPart>, Without<SigilPart>)>,
    mut parts: Query<(&ChildOf, &SigilPart, &mut Sprite, &mut Transform), (Without<BruteMarks>, Without<SlamPart>, Without<Sigil>)>,
) {
    let t = time.elapsed_secs();
    let mut plans = Vec::new();
    for (entity, look, mut clock, mut visibility) in &mut looks {
        let Ok((keeper, brute, health)) = brutes.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let since = clock.tick(brute.state, time.delta_secs());
        let shown = seen.0 && keeper.awake && !health.is_dead();
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if !shown {
            continue;
        }
        let length = match brute.state {
            BruteState::WindUp if keeper.enraged => ENRAGED_SLAM_LOOK,
            BruteState::WindUp => SLAM_LOOK,
            BruteState::Summon => ROAR_LOOK,
            _ => 1.0,
        };
        let progress = (since / length).clamp(0.0, 1.0);
        plans.push((entity, BrutePlan { state: brute.state, at: brute.at, summons: brute.summons.clone(), progress, since }));
    }
    let plan = |look: Entity| plans.iter().find(|(entity, _)| *entity == look).map(|(_, plan)| plan);

    for (parent, part, mut sprite, mut transform, mut visibility) in &mut slams {
        let Some(plan) = plan(parent.parent()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let full = BRUTE_SLAM_RADIUS;
        let look = match (plan.state, *part) {
            (BruteState::WindUp, _) => {
                let p = plan.progress;
                let pulse = 0.5 + 0.5 * (t * (10.0 + 22.0 * p)).sin();
                let hot = Srgba::from(MARK_COLOR).mix(&Srgba::WHITE, 0.6 * p * p);
                match part {
                    SlamPart::Base => Some((full, hot.with_alpha(0.1 + 0.06 * pulse))),
                    SlamPart::Fill => Some((full * p, hot.with_alpha(0.18 + 0.3 * p))),
                    SlamPart::Edge => Some((full, hot.mix(&Srgba::WHITE, 0.3).with_alpha(0.45 + 0.55 * pulse))),
                }
            }
            // The flash where it landed, fading.
            (BruteState::Recover, SlamPart::Fill | SlamPart::Edge) if plan.since < SLAM_FLASH => {
                let fade = 1.0 - plan.since / SLAM_FLASH;
                let color = Srgba::from(MARK_COLOR).mix(&Srgba::WHITE, fade);
                let radius = if *part == SlamPart::Edge { full * (1.0 + 0.25 * (1.0 - fade)) } else { full };
                Some((radius, color.with_alpha(0.7 * fade)))
            }
            _ => None,
        };
        let Some((radius, color)) = look else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let (size, z) = match part {
            SlamPart::Base => (DISC_SIZE, 0.0),
            SlamPart::Fill => (DISC_SIZE, 0.1),
            SlamPart::Edge => (RING_SIZE, 0.2),
        };
        transform.translation = plan.at.extend(z);
        transform.rotation = Quat::from_rotation_z(if *part == SlamPart::Edge { t * 1.5 } else { 0.0 });
        transform.scale = Vec3::splat(2.0 * radius / size);
        sprite.color = color.into();
    }

    for (parent, sigil, mut transform, mut visibility) in &mut sigils {
        let spot = plan(parent.parent()).filter(|plan| plan.state == BruteState::Summon).and_then(|plan| plan.summons.get(sigil.0));
        let Some(&at) = spot else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at.extend(0.3 + 0.01 * sigil.0 as f32);
    }
    for (parent, part, mut sprite, mut transform) in &mut parts {
        let Ok((sigil_parent, sigil, ..)) = sigils.get(parent.parent()) else { continue };
        let Some(plan) = plan(sigil_parent.parent()) else { continue };
        let p = plan.progress;
        // Opens fast, then keeps swelling; each a little out of step.
        let phase = sigil.0 as f32 * 1.7;
        let radius = SIGIL_RADIUS * (0.25 + 0.75 * (1.0 - (1.0 - p).powi(2))) * (1.0 + 0.08 * (t * (8.0 + 16.0 * p) + phase).sin());
        let glow = Srgba::from(SUMMON_COLOR).mix(&Srgba::WHITE, 0.5 * p * p);
        let (scale, spin, color) = match part {
            SigilPart::Ring => (2.0 * radius / RING_SIZE, t * 3.0 + phase, glow.mix(&Srgba::WHITE, 0.3).with_alpha(0.5 + 0.5 * p)),
            SigilPart::Inner => (1.2 * radius / RING_SIZE, -t * 5.0 - phase, glow.mix(&Srgba::WHITE, 0.3).with_alpha(0.35 + 0.5 * p)),
            SigilPart::Core => (1.6 * radius / DISC_SIZE, 0.0, glow.with_alpha(0.12 + 0.4 * p)),
        };
        transform.scale = Vec3::splat(scale);
        transform.rotation = Quat::from_rotation_z(spin);
        sprite.color = color.into();
    }
}

/// Leaves fading copies of the stalker behind it as it lunges or rampages.
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
        if !matches!(look.state, StalkerState::Lunge { .. } | StalkerState::Rampage { moving: true, .. }) {
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
