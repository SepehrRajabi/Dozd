//! How a vault's guardian looks: the keeper's body, hands, beams and charging
//! orbs, the stalker's crouch, routes and lunge, its rampage lanes, the
//! brute's stomp, slam and roar, the prism's phases, blinks and arc charges,
//! and the health bar while the crew fights any of them. None of it shows
//! unless the camera is in the vault room: what guards a vault is meant to be
//! a surprise.

use bevy::prelude::*;

use crate::boss::{
    self, BRUTE_SLAM_RADIUS, BruteLook, BruteState, HandAction, HandSide, Keeper, KeeperBeam, KeeperHand, KeeperKind, PRISM_ARC_RANGE,
    PRISM_COLOR, PrismLook, PrismPhase, SLAMS, LUNGE_SWEEP, StalkerLook, StalkerState, slam_wind_up, stalker_wind_up,
};
use crate::combat::{Fade, Health, Shake};
use crate::hud::{self, HudSlot, Meter};
use crate::ring_view::{AuraRing, RING_AT};
use crate::room::RoomGrid;
use crate::vault::VaultTier;
use crate::{Level, Rng};

/// The keeper's beams: a hostile violet, well clear of the Gunner's red laser.
const BEAM_COLOR: Color = Color::srgb(0.85, 0.3, 1.0);
const ENRAGED_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);
const HIT_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);
/// The stalker's marks and its wind-up blink: a hot, hostile orange. Every
/// spoke it might lunge down shares it, so its fan reads as one even star
/// with none favoured, until it settles on one (`CHOSEN_COLOR`).
const MARK_COLOR: Color = Color::srgb(1.0, 0.4, 0.15);
/// Spent after a lunge, like a guard stalker recovering: dull and bluish.
const RECOVER_COLOR: Color = Color::srgb(0.6, 0.6, 0.75);
/// How far apart the chevrons along the stalker's routes sit, and how many
/// there can be across all of them.
const CHEVRON_GAP: f32 = 9.0;
const CHEVRONS: usize = 240;
/// How many spoke ends can show reticles at once.
const RETICLES: usize = 12;
/// How long the route it takes flashes as it settles on it, and again as it
/// goes, and the white-hot it burns from then on.
const CHOSEN_FLASH: f32 = 0.25;
const CHOSEN_COLOR: Color = Color::srgb(1.0, 0.97, 0.9);
/// How many legs, across all its routes, can show a danger corridor at once.
const ROUTE_STRIPS: usize = 16;
/// The ring round the stalker tying its fan of spokes together, just clear
/// of its body, where they start: how far out it starts, and how far it has
/// drawn in by the end of the wind-up.
const HUB_RADIUS: (f32, f32) = (30.0, 23.0);
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
/// Roughly how long the brute roars, for its summoning sigils to fill over,
/// and how long a slam's flash lingers once it lands. Its slam discs fill over
/// `slam_wind_up`.
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
/// One frame of the brute's sheet (`brute_sheet.png` and `brute_glow.png`),
/// and how far up it sits so its feet stand where the other guardians' do.
const BRUTE_FRAME: UVec2 = UVec2::new(56, 52);
const BRUTE_LIFT: f32 = 5.0;
/// The sheet's frames, in order (see `BRUTE_POSES` in `tools/gen_assets.py`).
const IDLE: [usize; 2] = [0, 1];
const WALK: [usize; 4] = [2, 3, 4, 5];
const RAISE: usize = 6;
const OVERHEAD: usize = 7;
const SLAM: usize = 8;
const SPENT: [usize; 2] = [9, 10];
const ROAR: [usize; 2] = [11, 12];
const BRUTE_FRAMES: u32 = 13;
/// Walk frames a second at full plod, and the speed below which it stands.
const WALK_RATE: f32 = 7.0;
const STANDING: f32 = 4.0;
/// How long it holds its fists at its shoulders before locking them overhead,
/// how long the slam frame holds, and how long it throws its head back
/// before bellowing.
const RAISE_HOLD: f32 = 0.22;
const SLAM_HOLD: f32 = 0.22;
const ROAR_HOLD: f32 = 0.2;
/// What its eyes and furnace burn: a hot orange, at rest.
const BRUTE_GLOW: Color = Color::srgb(1.0, 0.5, 0.18);
/// The brute hardened: cold steel armour, and the green of it knitting back together.
const ARMOUR_COLOR: Color = Color::srgb(0.62, 0.78, 1.0);
const HEAL_COLOR: Color = Color::srgb(0.45, 1.0, 0.55);
/// The armour plates set round a hardened brute's shell like a clock face,
/// one going dark at a time as its hardening runs out; the ellipse they sit
/// on, round its body.
const PLATES: usize = 12;
const PLATE_ORBIT: Vec2 = Vec2::new(36.0, 33.0);
/// Where its shell and plates centre, and where its heal motes start rising.
const SHELL_Y: f32 = BRUTE_LIFT - 2.0;
const MOTE_Y: f32 = -12.0;
/// Green motes rising off a hardened brute as it heals.
const MOTES: usize = 6;
/// Seconds the shell takes to clamp shut round it.
const HARDEN_SHUT: f32 = 0.2;
/// The last part of its hardening, as a fraction, over which what's left flickers.
const HARDEN_WARN: f32 = 0.25;
const DUST_COLOR: Color = Color::srgba(0.75, 0.7, 0.65, 0.8);
/// The prism's aegis, while nothing hurts it: a warm amber, so "immune" reads
/// well apart from its cold crystal.
const AEGIS_COLOR: Color = Color::srgb(1.0, 0.72, 0.22);
/// Its mirror panes, while it sends shots back: polished silver, a touch violet.
const MIRROR_COLOR: Color = Color::srgb(0.86, 0.86, 1.0);
/// The bar's word for its open window: a bright "go" green.
const EXPOSED_COLOR: Color = Color::srgb(0.45, 1.0, 0.55);
/// How far each half of the prism slides out while it's exposed, baring its heart.
const PRISM_OPEN: f32 = 3.0;
/// How many mirror panes orbit it, the ellipse they orbit on, and how fast
/// they go round, in radians a second, calm and frantic.
const SHARDS: usize = 6;
const SHARD_ORBIT: Vec2 = Vec2::new(27.0, 10.0);
const SHARD_SPIN: f32 = 1.6;
const FRANTIC_SPIN: f32 = 3.4;
/// Roughly how long its warning before a shift, its blink and its arc charge
/// last, for their looks to run over; how long it takes to swell back in after
/// a blink, and how long its reach ring lingers once the arcs fly.
const SHIFT_LOOK: f32 = 0.8;
const WARP_LOOK: f32 = 0.3;
const CHARGE_LOOK: f32 = 0.3;
const ARRIVE_LOOK: f32 = 0.18;
const REACH_LINGER: f32 = 0.3;

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
            .add_observer(dress_brute_body)
            .add_observer(dress_prism)
            .add_systems(Startup, spawn_boss_bar)
            .add_systems(
                Update,
                (
                    watch_vault_room,
                    (
                        hide_boss,
                        draw_keeper,
                        animate_brute,
                        draw_harden,
                        draw_hands,
                        draw_beams,
                        draw_marks,
                        draw_routes,
                        draw_lane,
                        draw_brute,
                        draw_prism,
                        draw_warp,
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
/// The brute's is a `BruteSprite` instead.
#[derive(Component)]
struct KeeperSprite;

/// The brute's body: a frame of its sheet, picked by what it's doing.
#[derive(Component, Default)]
struct BruteSprite {
    /// Where its body was last frame, to tell how fast it's plodding.
    last: Option<Vec2>,
    /// How far through its walk cycle, in frames.
    stride: f32,
    /// The frame showing, to catch each foot coming down.
    frame: usize,
    /// Which way it faces: 1 right, -1 left.
    facing: f32,
    clock: StateClock<BruteState>,
}

/// What burns on the brute (eyes, furnace, maw): the frame matching its
/// body's, on top, coloured by its mood.
#[derive(Component)]
struct BruteGlow;

/// The brute's hardened shell round its body, a child of it, shown only while
/// it's `BruteState::Harden`.
#[derive(Component, Default)]
struct HardenRig {
    clock: StateClock<BruteState>,
}

/// The honeycomb shell over the hardened brute, the `0`th, `1`st... armour
/// plate round its feet counting its hardening down, or a heal mote rising.
#[derive(Component, Clone, Copy, PartialEq)]
enum HardenPart {
    Shell,
    Plate(usize),
    Mote(usize),
}

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

/// Draws a stalker's `StalkerLook` routes: the spokes it might lunge down (or
/// the one it took), a reticle at each one's end, and the ring at their hub.
/// Its children sit in world space.
#[derive(Component)]
struct MarksLook(Entity);

/// This frame's danger corridors, one per leg still ahead, for `draw_routes`.
#[derive(Component, Default)]
struct RoutePlan(Vec<StripPlan>);

/// The `index`th danger corridor: one leg of a route, as wide as the lunge
/// reaches, so a pirate can see how far to step off it. Laid along the leg,
/// so its `StripPart` children sit in leg space.
#[derive(Component)]
struct RouteStrip(usize);

/// A corridor's faint floor, the fuse filling it as the wind-up runs out, the
/// bright line where the fuse has burnt to, or one of its two edges (`-1` or `1`).
#[derive(Component, Clone, Copy, PartialEq)]
enum StripPart {
    Floor,
    Fuse,
    Head,
    Edge(f32),
}

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

/// The `index`th reticle, at the end of the `spoke`th route; bigger on the
/// one it has `chosen`.
#[derive(Component)]
struct MarkReticle {
    index: usize,
    spoke: usize,
    color: Color,
    chosen: bool,
}

/// A reticle's ring, a child so it can spin and throb.
#[derive(Component)]
struct MarkRing;

/// The ring round the winding-up stalker, at the hub of its spokes.
#[derive(Component)]
struct MarkHub;

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

/// The prism's floating body, a child so it can bob, tremble and blink out
/// without moving the keeper; its parts are its children.
#[derive(Component, Default)]
struct PrismRig {
    /// How far its halves have parted, 0 shut to 1 open.
    open: f32,
    /// How far round its mirror panes have gone, in radians.
    orbit: f32,
    shift: StateClock<(PrismPhase, PrismPhase)>,
    warp: StateClock<bool>,
    charge: StateClock<bool>,
    /// It has blinked, or gathered an arc, at least once: so the swell back
    /// in, or the reach ring's fade, isn't played when it first shows up.
    warped: bool,
    charged: bool,
}

/// One of the prism's parts, in its rig.
#[derive(Component, Clone, Copy, PartialEq)]
enum PrismPart {
    /// Its left (`-1`) or right (`1`) half, which part to bare its heart.
    Half(f32),
    /// The glow at its heart, behind its halves.
    Heart,
    /// The amber bubble round it while nothing hurts it.
    Aegis,
    /// The `0`th, `1`st... mirror pane orbiting it while it reflects.
    Shard(usize),
}

/// The ring on the floor round the prism showing how far its arcs reach,
/// while it gathers one.
#[derive(Component)]
struct PrismReach;

/// Where a blinking prism will land: drawn on the spot, in world space.
#[derive(Component)]
struct PrismMarks(Entity);

/// The rings closing in on a blinking prism's landing spot (the later one
/// starts `delay` into the blink), or the faint copy of it growing there.
#[derive(Component, Clone, Copy, PartialEq)]
enum WarpPart {
    Ring { delay: f32 },
    Ghost,
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
    // The prism floats higher, its shadow well below it.
    let shadow = if keeper.kind == KeeperKind::Prism { -11.0 } else { -9.0 };
    commands.entity(add.entity).insert(Visibility::Hidden).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, shadow, -0.1).with_scale(Vec3::new(3.2, 2.4, 1.0)),
        ));
        if !matches!(keeper.kind, KeeperKind::Brute | KeeperKind::Prism) {
            parent.spawn((KeeperSprite, Sprite::from_image(assets.load(keeper.kind.sprite()))));
        }
    });
}

/// Gives the brute its animated body and glow, from its sheets.
fn dress_brute_body(
    add: On<Add, BruteLook>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(BRUTE_FRAME, BRUTE_FRAMES, 1, None, None));
    let atlas = |image: &str| Sprite::from_atlas_image(assets.load(image.to_string()), TextureAtlas { layout: layout.clone(), index: 0 });
    commands.entity(add.entity).with_children(|parent| {
        parent
            .spawn((
                BruteSprite { facing: 1.0, ..default() },
                atlas("sprites/boss/brute_sheet.png"),
                Transform::from_xyz(0.0, BRUTE_LIFT, 0.0),
            ))
            .with_child((BruteGlow, atlas("sprites/boss/brute_glow.png"), Transform::from_xyz(0.0, 0.0, 0.1)));
        parent.spawn((HardenRig::default(), Transform::default(), Visibility::Hidden)).with_children(|rig| {
            rig.spawn((HardenPart::Shell, Sprite::from_image(assets.load("sprites/boss/prism_aegis.png"))));
            for i in 0..PLATES {
                rig.spawn((HardenPart::Plate(i), Sprite::from_color(Color::WHITE, Vec2::new(7.0, 3.0))));
            }
            for i in 0..MOTES {
                rig.spawn((HardenPart::Mote(i), Sprite::from_color(Color::WHITE, Vec2::splat(2.0))));
            }
        });
    });
}

/// Gives the prism its rig: two halves over its heart, the aegis bubble and
/// the mirror panes (hidden until it raises them), and the ring showing its
/// arcs' reach; and, in world space, the marks for where it blinks to.
fn dress_prism(add: On<Add, PrismLook>, mut commands: Commands, assets: Res<AssetServer>) {
    let sprite = |image: &str| Sprite::from_image(assets.load(image.to_string()));
    commands.entity(add.entity).with_children(|parent| {
        parent.spawn((
            PrismReach,
            AuraRing::new(PRISM_ARC_RANGE, 0.08),
            Transform::from_translation(RING_AT),
            Visibility::Hidden,
        ));
        parent.spawn((PrismRig::default(), Transform::default(), Visibility::default())).with_children(|rig| {
            rig.spawn((PrismPart::Heart, sprite("sprites/boss/prism_core.png"), Transform::from_xyz(0.0, 0.0, -0.05)));
            rig.spawn((PrismPart::Half(-1.0), sprite("sprites/boss/prism_left.png")));
            rig.spawn((PrismPart::Half(1.0), sprite("sprites/boss/prism_right.png")));
            rig.spawn((PrismPart::Aegis, sprite("sprites/boss/prism_aegis.png"), Transform::from_xyz(0.0, 0.0, 0.5), Visibility::Hidden));
            for i in 0..SHARDS {
                rig.spawn((PrismPart::Shard(i), sprite("sprites/boss/prism_shard.png"), Visibility::Hidden));
            }
        });
    });
    // Not `Level`: on a client the boss can arrive before its deck is built,
    // and building it clears `Level` entities. It goes when its boss does.
    commands
        .spawn((PrismMarks(add.entity), StateClock::<bool>::default(), Transform::default(), Visibility::Hidden))
        .with_children(|parent| {
            let ring = assets.load("sprites/fx/shock_ring.png");
            for (part, z) in [(WarpPart::Ring { delay: 0.0 }, 0.0), (WarpPart::Ring { delay: 0.35 }, 0.1)] {
                parent.spawn((part, Sprite::from_image(ring.clone()), Transform::from_xyz(0.0, 0.0, z)));
            }
            parent.spawn((WarpPart::Ghost, sprite(KeeperKind::Prism.sprite()), Transform::from_xyz(0.0, 0.0, 0.2)));
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
    // Not `Level`: on a client the boss can arrive before its deck is built,
    // and building it clears `Level` entities. It goes when its boss does.
    commands
        .spawn((BeamLook(add.entity), Transform::default(), Visibility::Hidden))
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
/// brute is animated by `animate_brute`.
#[allow(clippy::type_complexity)]
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

/// Plays the brute's sheet. Asleep it breathes, hunched and dim. It stands
/// breathing, or knuckle-drags along in a heavy stomp that kicks up dust and
/// shakes the floor with each foot; turning to face where it's going. Winding
/// up it rears, fists climbing to its shoulders, then locks them overhead,
/// trembling and flashing; the slam folds it over its fists, leaning into the
/// blow, and it stays slumped over its knuckles, heaving and dull, while spent.
/// Roaring it throws its head back, then bellows, arms flung wide, chest
/// blazing amber. Hardened it hunkers down over its knuckles, cold steel, its
/// eyes and furnace pulsing green as it heals (see `draw_harden`). Its eyes
/// and furnace burn orange, red once enraged.
#[allow(clippy::type_complexity)]
fn animate_brute(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    mut shake: ResMut<Shake>,
    mut rng: Local<Rng>,
    brutes: Query<(&Keeper, &Health, &BruteLook, &Transform), Without<BruteSprite>>,
    mut bodies: Query<(Entity, &ChildOf, &mut BruteSprite, &mut Sprite, &mut Transform), Without<BruteGlow>>,
    mut glows: Query<(&ChildOf, &mut Sprite), (With<BruteGlow>, Without<BruteSprite>)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    let blink = (t * 30.0).sin() > 0.0;
    let mut shown = Vec::new();
    for (entity, parent, mut anim, mut sprite, mut transform) in &mut bodies {
        let Ok((keeper, health, brute, body)) = brutes.get(parent.parent()) else { continue };
        let pos = body.translation.truncate();
        let speed = anim.last.map_or(0.0, |last| last.distance(pos) / dt.max(1e-4));
        let heading = anim.last.map_or(0.0, |last| pos.x - last.x);
        anim.last = Some(pos);
        let since = anim.clock.tick(brute.state, dt);
        let tint = Srgba::from(tier_tint(keeper.tier));
        let mut offset = Vec2::ZERO;
        let mut flip_to = if heading.abs() > 0.05 { heading.signum() } else { anim.facing };

        let (frame, color, glow, burn) = if health.is_dead() {
            (SPENT[1], dim(tint, 0.4), BRUTE_GLOW, 0.0)
        } else if !keeper.awake {
            let breath = (t * 0.8).sin();
            (IDLE[(breath > 0.0) as usize], dim(tint, 0.5 + 0.06 * breath), BRUTE_GLOW, 0.3 + 0.1 * breath)
        } else {
            let mood = if keeper.enraged { seethe(tint, t) } else { tint };
            let fury = if keeper.enraged { Color::from(seethe(Srgba::from(BRUTE_GLOW), t)) } else { BRUTE_GLOW };
            let toward = brute.at.x - pos.x;
            match brute.state {
                BruteState::Plod if speed > STANDING => {
                    anim.stride += dt * WALK_RATE * (speed / 32.0).clamp(0.6, 1.6);
                    (WALK[anim.stride as usize % WALK.len()], mood, fury, 0.9)
                }
                BruteState::Harden => {
                    // Braced low over its knuckles, gone to cold steel, a slow
                    // green pulse in its eyes and furnace as it mends.
                    let pulse = 0.5 + 0.5 * (t * 3.0).sin();
                    let steel = tint.mix(&Srgba::from(ARMOUR_COLOR), 0.55).mix(&Srgba::WHITE, 0.1 * pulse);
                    (SPENT[0], steel, HEAL_COLOR, 0.5 + 0.5 * pulse)
                }
                BruteState::Plod => {
                    let breath = (t * 2.4).sin();
                    (IDLE[(breath > 0.0) as usize], mood, fury, 0.85 + 0.1 * breath)
                }
                BruteState::WindUp { blow } => {
                    if toward.abs() > 2.0 {
                        flip_to = toward.signum();
                    }
                    // Between slams the fists swing straight back up.
                    let raise = if blow == 0 { RAISE_HOLD } else { RAISE_HOLD * 0.5 };
                    if since < raise {
                        (RAISE, mood, fury, 1.0)
                    } else {
                        // Straining: shaking harder the longer it holds.
                        let strain = ((since - raise) * 3.0).min(1.5);
                        offset = strain * Vec2::new((t * 57.0).sin(), (t * 43.0).cos() * 0.5);
                        let color = if blink { tint.mix(&Srgba::WHITE, 0.45) } else { mood };
                        let glow = if blink { Color::WHITE } else { ENRAGED_COLOR };
                        (OVERHEAD, color, glow, 1.0)
                    }
                }
                BruteState::Recover { blow } => {
                    // Mid-flurry it stays down over its fists until the next.
                    if since < SLAM_HOLD || blow + 1 < SLAMS {
                        // Leaning into the blow, toward where it landed.
                        offset = (brute.at - pos).normalize_or_zero() * 2.0;
                        (SLAM, tint.mix(&Srgba::WHITE, 0.2), Color::WHITE, 1.0)
                    } else {
                        let heave = (t * 3.2).sin();
                        (SPENT[(heave > 0.0) as usize], times(tint, Srgba::from(RECOVER_COLOR)), BRUTE_GLOW, 0.35 + 0.1 * heave)
                    }
                }
                BruteState::Summon => {
                    let heat = 0.5 + 0.5 * (t * 14.0).sin();
                    let color = tint.mix(&Srgba::from(SUMMON_COLOR), 0.2 + 0.25 * heat);
                    if since < ROAR_HOLD {
                        (ROAR[0], color, SUMMON_COLOR, 0.9)
                    } else {
                        offset = 1.2 * Vec2::new((t * 53.0).sin(), (t * 41.0).cos() * 0.5);
                        (ROAR[((t * 7.0) as usize) % 2], color, SUMMON_COLOR, 0.8 + 0.2 * heat)
                    }
                }
            }
        };

        // Each foot coming down kicks up dust and thumps the floor.
        let landed = frame != anim.frame && (frame == WALK[1] || frame == WALK[3]);
        if landed && seen.0 {
            shake.0 = shake.0.max(0.12);
            let side = if frame == WALK[1] { -1.0 } else { 1.0 } * anim.facing;
            for _ in 0..4 {
                let at = pos + Vec2::new(side * 8.0 + rng.signed() * 4.0, -19.0 + rng.signed() * 1.5);
                commands.spawn((
                    Level,
                    Fade { age: 0.0, life: 0.3 + rng.unit() * 0.2 },
                    Sprite::from_color(DUST_COLOR, Vec2::splat(1.0 + rng.unit().round())),
                    Transform::from_translation(at.round().extend(7.5)),
                ));
            }
        }
        if frame == SLAM && anim.frame != SLAM && seen.0 {
            shake.0 = shake.0.max(0.55);
        }
        anim.frame = frame;
        anim.facing = flip_to;

        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = frame;
        }
        sprite.flip_x = anim.facing < 0.0;
        sprite.color = match (health.flash > 0.0, brute.state) {
            // Hits glance off its armour: a glint, not a wound.
            (true, BruteState::Harden) => color.mix(&Srgba::WHITE, 0.6).into(),
            (true, _) => HIT_COLOR,
            (false, _) => color.into(),
        };
        transform.translation.x = offset.x.round();
        transform.translation.y = (BRUTE_LIFT + offset.y).round();
        let glow = Srgba::from(glow);
        shown.push((entity, frame, anim.facing < 0.0, glow.with_alpha(glow.alpha * burn)));
    }
    for (parent, mut sprite) in &mut glows {
        let Some(&(_, frame, flip, color)) = shown.iter().find(|(e, ..)| *e == parent.parent()) else { continue };
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = frame;
        }
        sprite.flip_x = flip;
        sprite.color = color.into();
    }
}

/// How a hardened brute's shell looks this frame.
struct HardenPlan {
    /// How much of its hardening is left, `1..=0`.
    left: f32,
    /// How far its shell has clamped shut, `0..=1`.
    shut: f32,
    /// A hit just glanced off it.
    flare: bool,
}

/// While the brute is hardened: a steel honeycomb shell clamps shut over it,
/// shimmering, and flares white when a hit glances off; a ring of armour
/// plates round it goes dark one by one, like a clock, as its hardening runs out,
/// what's left flickering at the end; and green motes rise off it as it heals.
#[allow(clippy::type_complexity)]
fn draw_harden(
    time: Res<Time>,
    brutes: Query<(&Keeper, &Health, &BruteLook)>,
    mut rigs: Query<(Entity, &ChildOf, &mut HardenRig, &mut Visibility)>,
    mut parts: Query<(&ChildOf, &HardenPart, &mut Sprite, &mut Transform)>,
) {
    use std::f32::consts::{PI, TAU};
    let t = time.elapsed_secs();
    let mut plans = Vec::new();
    for (entity, parent, mut rig, mut visibility) in &mut rigs {
        let Ok((keeper, health, brute)) = brutes.get(parent.parent()) else { continue };
        let since = rig.clock.tick(brute.state, time.delta_secs());
        let shown = brute.state == BruteState::Harden && keeper.awake && !health.is_dead();
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if shown {
            let left = (1.0 - since / boss::brute_harden_time(keeper.enraged).max(0.1)).clamp(0.0, 1.0);
            let shut = (since / HARDEN_SHUT).min(1.0);
            plans.push((entity, HardenPlan { left, shut, flare: health.flash > 0.0 }));
        }
    }
    let warn_blink = (t * 16.0).sin() > 0.0;
    let armour = Srgba::from(ARMOUR_COLOR);
    for (parent, part, mut sprite, mut transform) in &mut parts {
        let Some((_, plan)) = plans.iter().find(|(entity, _)| *entity == parent.parent()) else { continue };
        let ending = plan.left < HARDEN_WARN;
        match *part {
            HardenPart::Shell => {
                let shimmer = 0.5 + 0.5 * (t * 4.0).sin();
                let color = if plan.flare {
                    armour.mix(&Srgba::WHITE, 0.85).with_alpha(0.6)
                } else if ending && !warn_blink {
                    armour.with_alpha(0.08)
                } else {
                    armour.mix(&Srgba::WHITE, 0.2 * shimmer).with_alpha(0.16 + 0.1 * shimmer)
                };
                // Clamps shut from wide, then breathes.
                let swell = 1.0 + 0.4 * (1.0 - plan.shut) + 0.02 * (t * 2.0).sin();
                transform.translation = Vec3::new(0.0, SHELL_Y, 0.4);
                transform.scale = Vec3::new(1.2 * swell, swell, 1.0);
                sprite.color = color.into();
            }
            HardenPart::Plate(i) => {
                // Clockwise from the top, so the ring empties like a clock.
                let angle = PI / 2.0 - i as f32 * TAU / PLATES as f32;
                let at = PLATE_ORBIT * Vec2::from_angle(angle) * (1.0 + 0.5 * (1.0 - plan.shut));
                let tangent = Vec2::new(-PLATE_ORBIT.x * angle.sin(), PLATE_ORBIT.y * angle.cos());
                transform.translation = Vec3::new(at.x.round(), (SHELL_Y + at.y).round(), 0.42);
                transform.rotation = Quat::from_rotation_z(tangent.to_angle());
                // How much of this plate is left: whole, going, or gone.
                let lit = (plan.left * PLATES as f32 - (PLATES - 1 - i) as f32).clamp(0.0, 1.0);
                let wave = 0.5 + 0.5 * (t * 6.0 - i as f32 * 0.8).sin();
                let color = if lit <= 0.0 {
                    dim(armour, 0.6).with_alpha(0.18)
                } else if plan.flare {
                    Srgba::WHITE
                } else if ending && !warn_blink {
                    armour.with_alpha(0.35)
                } else {
                    armour.mix(&Srgba::WHITE, 0.25 + 0.35 * wave).with_alpha(0.2 + 0.8 * lit)
                };
                sprite.color = color.into();
            }
            HardenPart::Mote(i) => {
                let rise = (t * 0.7 + i as f32 / MOTES as f32).fract();
                let x = ((i * 7) % MOTES) as f32 * 5.0 - 12.5 + 2.0 * (t * 2.0 + i as f32).sin();
                transform.translation = Vec3::new(x.round(), (MOTE_Y + 36.0 * rise).round(), 0.45);
                let glow = Srgba::from(HEAL_COLOR).mix(&Srgba::WHITE, 0.3);
                sprite.color = glow.with_alpha(0.9 * (PI * rise).sin()).into();
            }
        }
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
    // Not `Level`: on a client the boss can arrive before its deck is built,
    // and building it clears `Level` entities. It goes when its boss does.
    commands
        .spawn((
            MarksLook(add.entity),
            StateClock::<bool>::default(),
            RoutePlan::default(),
            StateClock::<StalkerState>::default(),
            Transform::from_xyz(0.0, 0.0, 13.0),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            for i in 0..ROUTE_STRIPS {
                parent.spawn((RouteStrip(i), Transform::default(), Visibility::Hidden)).with_children(|strip| {
                    for (part, z) in [(StripPart::Floor, -0.3), (StripPart::Fuse, -0.2), (StripPart::Head, -0.15), (StripPart::Edge(-1.0), -0.1), (StripPart::Edge(1.0), -0.1)] {
                        strip.spawn((part, Sprite::from_color(MARK_COLOR, Vec2::ONE), Transform::from_xyz(0.0, 0.0, z)));
                    }
                });
            }
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
                        MarkReticle { index: n, spoke: 0, color: MARK_COLOR, chosen: false },
                        Transform::from_xyz(0.0, 0.0, 0.5),
                        Visibility::Hidden,
                    ))
                    .with_child((
                        MarkRing,
                        Sprite { image: assets.load("sprites/ui/crosshair.png"), color: MARK_COLOR, ..default() },
                    ));
            }
            parent.spawn((
                MarkHub,
                Sprite::from_image(assets.load("sprites/fx/shock_ring.png")),
                Transform::from_xyz(0.0, 0.0, -0.05),
                Visibility::Hidden,
            ));
        });
}

/// The end of the `spoke`th route, and whether it's the one it has chosen.
struct PlannedMark {
    at: Vec2,
    spoke: usize,
    chosen: bool,
    color: Color,
}

/// One leg's danger corridor: from `from` to `to`, its fuse filled `fuse`
/// pixels from `from`. `floor`, `fill`, `head` and `edge` are its parts'
/// opacities; `top` lays it over the others.
#[derive(Clone, Copy)]
struct StripPlan {
    from: Vec2,
    to: Vec2,
    fuse: f32,
    color: Color,
    floor: f32,
    fill: f32,
    head: f32,
    edge: f32,
    top: bool,
}

/// Where a stalker's corridors, chevrons, reticles and hub ring go this frame.
#[derive(Default)]
struct MarksPlan {
    strips: Vec<StripPlan>,
    spots: Vec<(Vec2, Vec2, Color)>,
    marks: Vec<PlannedMark>,
    /// The hub ring round its body while it winds up: where, how big, and its colour.
    hub: Option<(Vec2, f32, Color)>,
}

/// Lays out every route the stalker shows, from `from`: its spokes, each a
/// straight corridor as wide as the lunge reaches, chevrons marching along its
/// middle and a reticle at its end. Winding up, they're drawn alike in one
/// hostile colour, an even star round it: a light chases round the fan,
/// quickening as the wind-up (`wind_up` seconds) runs out, while a fuse fills
/// every corridor at once from the stalker outward, reaching the ends the
/// moment it goes, and a ring round its body, where the spokes start, draws in. Once it settles on a route
/// (`since_chosen` seconds ago), that one flashes and turns white-hot, its
/// reticle bigger, the hub with it, and the rest fade out; it stays so as it
/// lunges. Routes of more than one leg are walked leg by leg.
fn plan_marks(stalker: &StalkerLook, from: Vec2, since: f32, since_chosen: f32, t: f32, wind_up: f32) -> MarksPlan {
    let mut plan = MarksPlan::default();
    let choosing = stalker.state == StalkerState::WindUp;
    let reached = match stalker.state {
        StalkerState::Lunge { hit } => hit as usize,
        _ => 0,
    };
    let heat = (since / wind_up.max(0.01)).clamp(0.0, 1.0);
    let settled = stalker.chosen.map(usize::from);
    let spokes = stalker.routes.len().max(1) as f32;
    // The chase light's way round the fan, in turns: quicker as it heats up.
    let chase = since * (1.2 + 1.8 * heat);
    let danger = Srgba::from(MARK_COLOR);
    // A flash as it settles on the route, and again as it goes.
    let flash = (1.0 - if choosing { since_chosen } else { since } / CHOSEN_FLASH).max(0.0);
    let blink = (0.6 + 0.4 * (t * 18.0).sin()).max(flash);
    let hub = HUB_RADIUS.0 + (HUB_RADIUS.1 - HUB_RADIUS.0) * heat;
    if choosing {
        let color = if settled.is_some() {
            CHOSEN_COLOR.with_alpha(0.6 + 0.4 * blink)
        } else {
            danger.mix(&Srgba::WHITE, 0.2 * heat).with_alpha(0.6 + 0.4 * heat).into()
        };
        plan.hub = Some((from, hub, color));
    }
    // The route it settled on goes first, so it's drawn over the rest.
    let order = settled.into_iter().chain((0..stalker.routes.len()).filter(|&r| Some(r) != settled));
    for r in order {
        let Some(route) = stalker.routes.get(r) else { continue };
        let picked = !choosing || settled == Some(r);
        let passed_over = !picked && settled.is_some();
        let (color, floor, fill, head, edge): (Color, f32, f32, f32, f32) = if picked {
            let head = if choosing { 1.0 } else { 0.0 };
            (CHOSEN_COLOR.with_alpha(blink), 0.08 + 0.06 * flash, 0.22 + 0.1 * blink, head, blink)
        } else if passed_over {
            (danger.with_alpha(0.2).into(), 0.02, 0.0, 0.0, 0.15)
        } else {
            // How close the chase light is to this spoke: 1 on it, trailing off behind.
            let lag = (chase - r as f32 / spokes).rem_euclid(1.0);
            let glow = 0.7 + 0.3 * (1.0 - lag).powi(3);
            let color = danger.mix(&Srgba::WHITE, 0.25 * (1.0 - lag).powi(6));
            (color.with_alpha((0.45 + 0.45 * heat) * glow).into(), 0.04 + 0.03 * heat, 0.1 + 0.08 * heat, 0.9, (0.35 + 0.5 * heat) * glow)
        };
        let mut points: Vec<Vec2> = std::iter::once(from).chain(route.iter().skip(reached).copied()).collect();
        // Winding up, each spoke starts at the hub ring, leaving its body clear.
        if choosing && let Some(&next) = points.get(1) {
            let gap = hub.min(from.distance(next) - 1.0).max(0.0);
            points[0] = from + (next - from).normalize_or_zero() * gap;
        }
        // Winding up, the fuse burns out along the whole route as the wind-up
        // runs down; once it goes, the whole route is lit.
        let total: f32 = points.windows(2).map(|leg| leg[0].distance(leg[1])).sum();
        let mut lit = if choosing { heat * total } else { total };
        for leg in points.windows(2) {
            let length = leg[0].distance(leg[1]);
            if length < 1.0 {
                continue;
            }
            let fuse = lit.clamp(0.0, length);
            lit -= length;
            // Only the leg the fuse is burning along shows its head.
            let head = if fuse > 0.0 && fuse < length { head } else { 0.0 };
            plan.strips.push(StripPlan { from: leg[0], to: leg[1], fuse, color, floor, fill, head, edge, top: picked });
        }
        if passed_over {
            continue;
        }
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
        if let Some(&end) = route.last().filter(|_| route.len() > reached) {
            plan.marks.push(PlannedMark { at: end, spoke: r, chosen: picked, color });
        }
    }
    plan
}

/// While the stalker winds up, lays a trail of marching chevrons along every
/// spoke it might lunge down, a small throbbing reticle where each ends and a
/// ring round its body, none favoured, until moments before it goes it
/// settles on one and shows it (see `plan_marks`). Once it lunges, only the
/// route it took is left.
#[allow(clippy::type_complexity)]
fn draw_marks(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    stalkers: Query<(&Keeper, &StalkerLook, &Health, &Transform), Without<MarksLook>>,
    mut looks: Query<
        (Entity, &MarksLook, &mut StateClock<StalkerState>, &mut StateClock<bool>, &mut RoutePlan, &mut Visibility),
        (Without<MarkChevron>, Without<MarkReticle>),
    >,
    mut chevrons: Query<(&ChildOf, &mut MarkChevron, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkReticle>, Without<MarkRing>)>,
    mut reticles: Query<(&ChildOf, &mut MarkReticle, &mut Transform, &mut Visibility), (Without<Keeper>, Without<MarkChevron>, Without<MarkRing>)>,
    mut rings: Query<(&ChildOf, &mut Transform, &mut Sprite), (With<MarkRing>, Without<Keeper>, Without<MarkHub>)>,
    mut hubs: Query<
        (&ChildOf, &mut Transform, &mut Sprite, &mut Visibility),
        (With<MarkHub>, Without<Keeper>, Without<MarksLook>, Without<MarkChevron>, Without<MarkReticle>, Without<MarkRing>),
    >,
    mut strokes: Query<(&ChildOf, &mut Sprite), (With<MarkStroke>, Without<MarkRing>, Without<MarkHub>)>,
) {
    let t = time.elapsed_secs();
    let mut plans = Vec::new();
    for (entity, look, mut clock, mut settled, mut route_plan, mut visibility) in &mut looks {
        let Ok((keeper, stalker, health, body)) = stalkers.get(look.0) else {
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
        let since_chosen = settled.tick(stalker.chosen.is_some(), time.delta_secs());
        if shown {
            let wind_up = stalker_wind_up(keeper.enraged);
            let plan = plan_marks(stalker, body.translation.truncate(), since, since_chosen, t, wind_up);
            route_plan.0.clone_from(&plan.strips);
            plans.push((entity, plan));
        } else if !route_plan.0.is_empty() {
            route_plan.0.clear();
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
        reticle.spoke = mark.spoke;
        reticle.chosen = mark.chosen;
        reticle.color = mark.color;
    }
    for (parent, mut transform, mut sprite) in &mut rings {
        let Ok((_, reticle, ..)) = reticles.get(parent.parent()) else { continue };
        let beat = (t * 10.0 - reticle.spoke as f32 * 1.2).sin();
        transform.rotation = Quat::from_rotation_z(t * 2.5);
        // Candidates' reticles sit smaller than the chosen route's.
        let size = if reticle.chosen { 1.4 } else { 1.0 };
        transform.scale = Vec3::splat(size + 0.25 * beat);
        sprite.color = reticle.color;
    }
    for (parent, mut transform, mut sprite, mut visibility) in &mut hubs {
        let Some(&(at, radius, color)) = plan(parent.parent()).and_then(|plan| plan.hub.as_ref()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = at.extend(-0.05);
        transform.rotation = Quat::from_rotation_z(-t * 1.5);
        transform.scale = Vec3::splat(2.0 * radius / RING_SIZE);
        sprite.color = color;
    }
}

/// Lays each leg of the stalker's routes as a corridor as wide as its lunge
/// reaches: a faint floor with bright edges, and a fuse filling it from the
/// stalker outward as the wind-up runs out, a bright line at its head.
#[allow(clippy::type_complexity)]
fn draw_routes(
    looks: Query<&RoutePlan>,
    mut strips: Query<(&ChildOf, &RouteStrip, &mut Transform, &mut Visibility), Without<StripPart>>,
    mut parts: Query<(&ChildOf, &StripPart, &mut Sprite, &mut Transform), Without<RouteStrip>>,
) {
    let plan_of = |parent: &ChildOf, index: usize| looks.get(parent.parent()).ok().and_then(|plan| plan.0.get(index)).copied();
    for (parent, strip, mut transform, mut visibility) in &mut strips {
        let Some(leg) = plan_of(parent, strip.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let dir = (leg.to - leg.from).normalize_or(Vec2::X);
        transform.translation = leg.from.extend(if leg.top { -0.4 } else { -0.5 });
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
    }
    for (parent, part, mut sprite, mut transform) in &mut parts {
        let Ok((strip_parent, strip, ..)) = strips.get(parent.parent()) else { continue };
        let Some(leg) = plan_of(strip_parent, strip.0) else { continue };
        let length = leg.from.distance(leg.to);
        // In leg space: x runs from the leg's start (0) to its end.
        let (from, to, y, width, alpha) = match *part {
            StripPart::Floor => (0.0, length, 0.0, LUNGE_SWEEP * 2.0, leg.floor),
            StripPart::Fuse => (0.0, leg.fuse, 0.0, LUNGE_SWEEP * 2.0, if leg.fuse > 0.5 { leg.fill } else { 0.0 }),
            StripPart::Head => (leg.fuse - 2.0, leg.fuse, 0.0, LUNGE_SWEEP * 2.0, leg.head),
            StripPart::Edge(side) => (0.0, length, side * LUNGE_SWEEP, 1.0, leg.edge),
        };
        sprite.custom_size = Some(Vec2::new((to - from).max(0.0), width));
        sprite.color = leg.color.with_alpha(alpha);
        transform.translation.x = (from + to) / 2.0;
        transform.translation.y = y;
    }
}

fn dress_lane(add: On<Add, StalkerLook>, mut commands: Commands) {
    // Not `Level`: on a client the boss can arrive before its deck is built,
    // and building it clears `Level` entities. It goes when its boss does.
    commands
        .spawn((LaneLook(add.entity), StateClock::<StalkerState>::default(), Transform::from_xyz(0.0, 0.0, 12.5), Visibility::Hidden))
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
    // Not `Level`: on a client the boss can arrive before its deck is built,
    // and building it clears `Level` entities. It goes when its boss does.
    commands
        .spawn((
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
            BruteState::WindUp { blow } => slam_wind_up(blow, keeper.enraged),
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
            (BruteState::WindUp { .. }, _) => {
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
            (BruteState::Recover { .. }, SlamPart::Fill | SlamPart::Edge) if plan.since < SLAM_FLASH => {
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

/// How much of one of the prism's overlays shows, given whether its phase has
/// it `now` and whether the one it's shifting to will: all of it while it
/// keeps it, none while it never had it, and over a warning `warn` of the way
/// through, flickering in as it's coming (quicker and stronger toward the
/// end) or out as it's going.
fn presence(now: bool, next: bool, warn: f32, t: f32) -> f32 {
    let on = (t * (14.0 + 22.0 * warn)).sin() > 0.0;
    match (now, next) {
        (true, true) => 1.0,
        (false, false) => 0.0,
        (true, false) if on => 1.0 - 0.6 * warn,
        (true, false) => 0.2 * (1.0 - warn),
        (false, true) if on => 0.25 + 0.6 * warn,
        (false, true) => 0.0,
    }
}

/// How the prism looks this frame.
struct PrismPlan {
    open: f32,
    body: Color,
    heart: Color,
    aegis: f32,
    /// A hit just landed on its aegis: the bubble flares instead of the crystal flashing.
    flare: bool,
    shards: f32,
    orbit: f32,
    /// Enraged, or both guarding at once: everything quicker and shakier.
    frantic: bool,
}

/// The prism hovers. Asleep it rests dim, breathing slowly. Exposed, its
/// halves part, the crystal dims and its heart throbs bright: the crew's
/// window. Guarding, it shuts, its heart banked, behind the amber aegis bubble
/// (negating: hits on it flare the bubble instead of flashing red) or with its
/// mirror panes orbiting and glinting (reflecting), or both, frantic, once
/// enraged. Before a shift the coming phase's overlay flickers in, the going
/// one's out, and it shuts or starts to crack open. Blinking, it draws in to a
/// bright sliver and fades, then swells back in where it lands. Gathering an
/// arc it trembles and crackles white, and a ring shows how far its arcs reach.
#[allow(clippy::type_complexity)]
fn draw_prism(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    mut rng: Local<Rng>,
    prisms: Query<(&Keeper, &Health, &PrismLook, &Transform), Without<PrismRig>>,
    mut rigs: Query<(Entity, &ChildOf, &mut PrismRig, &mut Transform), (Without<Keeper>, Without<PrismPart>)>,
    mut parts: Query<(&ChildOf, &PrismPart, &mut Sprite, &mut Transform, &mut Visibility), (Without<Keeper>, Without<PrismRig>)>,
    mut reaches: Query<(&ChildOf, &mut AuraRing, &mut Visibility), (With<PrismReach>, Without<PrismPart>)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs();
    let catch_up = 1.0 - (-14.0 * dt).exp();
    let crystal = Srgba::from(PRISM_COLOR);
    let mut plans = Vec::new();
    let mut reach = Vec::new();
    for (entity, parent, mut rig, mut transform) in &mut rigs {
        let Ok((keeper, health, look, body)) = prisms.get(parent.parent()) else { continue };
        let since_shift = rig.shift.tick((look.phase, look.next), dt);
        let since_warp = rig.warp.tick(look.warp.is_some(), dt);
        let since_charge = rig.charge.tick(look.charging, dt);
        // Tinted by its tier, but lightly, so the glass stays glass.
        let tint = Srgba::from(tier_tint(keeper.tier)).mix(&Srgba::WHITE, 0.6);

        if !keeper.awake || health.is_dead() {
            rig.open += (0.0 - rig.open) * catch_up;
            let breath = (t * 1.3).sin();
            let level = if health.is_dead() { 0.4 } else { 0.5 + 0.06 * breath };
            transform.translation = Vec3::new(0.0, (-1.0 + breath).round(), 0.0);
            transform.scale = Vec3::ONE;
            let heart = dim(crystal, if health.is_dead() { 0.2 } else { 0.35 + 0.05 * breath });
            plans.push((entity, PrismPlan {
                open: rig.open,
                body: dim(tint, level).into(),
                heart: heart.into(),
                aegis: 0.0,
                flare: false,
                shards: 0.0,
                orbit: rig.orbit,
                frantic: false,
            }));
            reach.push((parent.parent(), 0.0));
            continue;
        }

        let shifting = look.next != look.phase;
        let warn = if shifting { (since_shift / SHIFT_LOOK).clamp(0.0, 1.0) } else { 0.0 };
        let exposed = look.phase == PrismPhase::Exposed;
        let frantic = keeper.enraged || look.phase == PrismPhase::Both;
        let open = match (exposed, look.next == PrismPhase::Exposed) {
            (true, true) => 1.0,
            // Shutting before it guards again.
            (true, false) => 1.0 - warn,
            // A hairline crack before it opens.
            (false, true) => 0.3 * warn,
            (false, false) => 0.0,
        };
        rig.open += (open - rig.open) * catch_up;
        rig.orbit += dt * if frantic { FRANTIC_SPIN } else { SHARD_SPIN };

        let mut color = if keeper.enraged { seethe(tint, t) } else { tint };
        if exposed {
            color = dim(color, 0.72);
        }
        let mut offset = Vec2::new(0.0, if frantic { 2.5 * (t * 3.4).sin() } else { 2.0 * (t * 2.2).sin() });
        if look.phase == PrismPhase::Both {
            offset += 0.4 * Vec2::new((t * 37.0).sin(), (t * 29.0).cos());
        }
        let pulse = 0.5 + 0.5 * (t * 7.0).sin();
        let mut heart = if exposed { dim(crystal, 0.55).mix(&Srgba::WHITE, pulse) } else { dim(crystal, 0.75) };

        // Gathering an arc: trembling, crackling white.
        let mut lit = 0.0;
        if look.charging {
            rig.charged = true;
            let charge = (since_charge / CHARGE_LOOK).clamp(0.0, 1.0);
            let flicker = 0.5 + 0.5 * (t * 47.0).sin();
            lit = 0.3 + 0.5 * charge * flicker;
            heart = crystal.mix(&Srgba::WHITE, 0.6 + 0.4 * flicker);
            offset += (0.5 + charge) * Vec2::new((t * 57.0).sin(), (t * 43.0).cos() * 0.5);
            if seen.0 && rng.unit() < 0.6 {
                let at = body.translation.truncate() + offset + Vec2::new(rng.signed() * 14.0, rng.signed() * 19.0);
                let spark = crystal.mix(&Srgba::WHITE, 0.6);
                commands.spawn((
                    Level,
                    Fade { age: 0.0, life: 0.1 + rng.unit() * 0.1 },
                    Sprite::from_color(spark, Vec2::new(2.0 + rng.unit().round(), 1.0)),
                    Transform::from_translation(at.round().extend(15.0)).with_rotation(Quat::from_rotation_z(rng.signed() * 1.6)),
                ));
            }
        }
        let reach_alpha = if look.charging {
            0.25 + 0.5 * (since_charge / CHARGE_LOOK).clamp(0.0, 1.0)
        } else if rig.charged && since_charge < REACH_LINGER {
            0.75 * (1.0 - since_charge / REACH_LINGER)
        } else {
            0.0
        };
        reach.push((parent.parent(), reach_alpha));

        // Blinking out: drawn in to a bright sliver, fading; then swelling back in.
        let mut scale = Vec2::ONE;
        let mut alpha = 1.0;
        if look.warp.is_some() {
            rig.warped = true;
            let p = (since_warp / WARP_LOOK).clamp(0.0, 1.0);
            scale = Vec2::new(1.0 - 0.85 * p, 1.0 + 0.3 * p);
            alpha = 1.0 - p * p;
            lit = lit.max(p);
        } else if rig.warped && since_warp < ARRIVE_LOOK {
            let p = since_warp / ARRIVE_LOOK;
            let ease = 1.0 - (1.0 - p).powi(2);
            scale = Vec2::new(0.15 + 0.85 * ease, 1.3 - 0.3 * ease);
            lit = lit.max(1.0 - p);
        }
        color = color.mix(&crystal.mix(&Srgba::WHITE, 0.5), lit);

        let hit = health.flash > 0.0 && !look.phase.negates();
        let body_color = if hit { HIT_COLOR } else { color.into() };
        transform.translation = offset.round().extend(0.0);
        transform.scale = scale.extend(1.0);
        plans.push((entity, PrismPlan {
            open: rig.open,
            body: body_color.with_alpha(alpha),
            heart: Color::from(heart).with_alpha(alpha),
            aegis: presence(look.phase.negates(), look.next.negates(), warn, t) * alpha,
            flare: health.flash > 0.0 && look.phase.negates(),
            shards: presence(look.phase.reflects(), look.next.reflects(), warn, t + 0.37) * alpha,
            orbit: rig.orbit,
            frantic,
        }));
    }
    let plan = |rig: Entity| plans.iter().find(|(entity, _)| *entity == rig).map(|(_, plan)| plan);

    for (parent, part, mut sprite, mut transform, mut visibility) in &mut parts {
        let Some(plan) = plan(parent.parent()) else { continue };
        match *part {
            PrismPart::Half(side) => {
                transform.translation.x = (side * plan.open * PRISM_OPEN).round();
                sprite.color = plan.body;
            }
            PrismPart::Heart => sprite.color = plan.heart,
            PrismPart::Aegis => {
                visibility.set_if_neq(if plan.aegis > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
                let beat = 0.5 + 0.5 * (t * if plan.frantic { 9.0 } else { 4.0 }).sin();
                sprite.color = if plan.flare {
                    Srgba::from(AEGIS_COLOR).mix(&Srgba::WHITE, 0.6).with_alpha(plan.aegis).into()
                } else {
                    AEGIS_COLOR.with_alpha(plan.aegis * (0.6 + 0.3 * beat))
                };
            }
            PrismPart::Shard(i) => {
                visibility.set_if_neq(if plan.shards > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
                let angle = plan.orbit + i as f32 * std::f32::consts::TAU / SHARDS as f32;
                // Frantic, they swing in and out as they go.
                let sway = if plan.frantic { 1.0 + 0.12 * (t * 9.0 + i as f32 * 2.3).sin() } else { 1.0 };
                let at = Vec2::new(angle.cos(), angle.sin()) * SHARD_ORBIT * sway + Vec2::Y;
                // In front of the crystal on the near side of their orbit, behind it on the far.
                transform.translation = at.round().extend(if at.y < 1.0 { 0.6 } else { -0.3 });
                // Each pane turns as it goes, glinting as it faces out.
                let turn = (t * 3.0 + i as f32 * 1.3).sin();
                transform.scale = Vec3::new(0.35 + 0.65 * turn.abs(), 1.0, 1.0);
                sprite.flip_x = turn < 0.0;
                let shine = if turn.abs() > 0.93 { Color::WHITE } else { Color::from(dim(Srgba::from(MIRROR_COLOR), 0.8)) };
                sprite.color = shine.with_alpha(plan.shards);
            }
        }
    }

    for (parent, mut ring, mut visibility) in &mut reaches {
        let alpha = reach.iter().find(|(prism, _)| *prism == parent.parent()).map_or(0.0, |(_, alpha)| *alpha);
        visibility.set_if_neq(if alpha > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        ring.color = PRISM_COLOR.with_alpha(alpha);
    }
}

/// While the prism blinks, marks where it'll land: two dashed rings closing
/// in on the spot, one after the other, and a faint copy of it growing there,
/// all brightening until it arrives.
#[allow(clippy::type_complexity)]
fn draw_warp(
    mut commands: Commands,
    time: Res<Time>,
    seen: Res<BossSeen>,
    prisms: Query<(&PrismLook, &Health)>,
    mut marks: Query<(Entity, &PrismMarks, &mut StateClock<bool>, &mut Transform, &mut Visibility), Without<WarpPart>>,
    mut parts: Query<(&ChildOf, &WarpPart, &mut Sprite, &mut Transform), Without<PrismMarks>>,
) {
    let t = time.elapsed_secs();
    let mut shown = Vec::new();
    for (entity, look, mut clock, mut transform, mut visibility) in &mut marks {
        let Ok((prism, health)) = prisms.get(look.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let since = clock.tick(prism.warp.is_some(), time.delta_secs());
        let Some(to) = prism.warp.filter(|_| seen.0 && !health.is_dead()) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        transform.translation = to.round().extend(12.5);
        shown.push((entity, (since / WARP_LOOK).clamp(0.0, 1.0)));
    }

    for (parent, part, mut sprite, mut transform) in &mut parts {
        let Some(&(_, p)) = shown.iter().find(|(entity, _)| *entity == parent.parent()) else { continue };
        let glow = Srgba::from(PRISM_COLOR).mix(&Srgba::WHITE, 0.6 * p);
        match *part {
            WarpPart::Ring { delay } => {
                let q = ((p - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                // From wide to snug round its body.
                transform.scale = Vec3::splat(1.1 - 0.7 * q);
                transform.rotation = Quat::from_rotation_z(if delay > 0.0 { -t * 5.0 } else { t * 4.0 });
                let alpha = if p < delay { 0.0 } else { 0.35 + 0.65 * q };
                sprite.color = glow.with_alpha(alpha).into();
            }
            WarpPart::Ghost => {
                transform.scale = Vec3::splat(0.5 + 0.5 * p);
                sprite.color = glow.with_alpha(0.12 + 0.4 * p).into();
            }
        }
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

/// Under the meter, how it's guarding itself: shown for the prism, and for
/// the brute while it's hardened.
#[derive(Component)]
struct BossPhaseRow;

/// The prism's phase in a word, or, while it's about to shift, the arrow and
/// the word for what it's shifting to.
#[derive(Component, Clone, Copy, PartialEq)]
enum BossPhase {
    Now,
    Arrow,
    Next,
}

/// What the bar calls a prism phase, and in what colour.
fn phase_label(phase: PrismPhase) -> (&'static str, Color) {
    match phase {
        PrismPhase::Exposed => ("EXPOSED", EXPOSED_COLOR),
        PrismPhase::Aegis => ("IMMUNE", AEGIS_COLOR),
        PrismPhase::Mirror => ("REFLECTING", MIRROR_COLOR),
        PrismPhase::Both => ("IMMUNE + REFLECTING", Srgba::from(AEGIS_COLOR).mix(&Srgba::from(MIRROR_COLOR), 0.4).into()),
    }
}

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
            (
                BossPhaseRow,
                Node {
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Baseline,
                    display: Display::None,
                    ..default()
                },
                children![
                    (BossPhase::Now, hud::text("", 12.0, hud::TEXT), TextLayout::no_wrap()),
                    (BossPhase::Arrow, hud::text(">", 12.0, hud::DIM), TextLayout::no_wrap()),
                    (BossPhase::Next, hud::text("", 12.0, hud::TEXT), TextLayout::no_wrap()),
                ],
            ),
        ],
    ));
}

#[allow(clippy::type_complexity)]
fn update_boss_bar(
    time: Res<Time>,
    seen: Res<BossSeen>,
    keepers: Query<(&Keeper, &Health, Option<&PrismLook>, Option<&BruteLook>)>,
    mut bar: Single<(&mut Node, &mut BorderColor), With<BossBar>>,
    mut phase_row: Single<&mut Node, (With<BossPhaseRow>, Without<BossBar>)>,
    mut phases: Query<(&BossPhase, &mut Text, &mut TextColor), (Without<BossName>, Without<BossTier>, Without<BossRage>)>,
    mut name: Single<(&mut Text, &mut TextColor), (With<BossName>, Without<BossTier>, Without<BossRage>)>,
    mut tier: Single<(&mut Text, &mut TextColor), (With<BossTier>, Without<BossName>, Without<BossRage>)>,
    mut rage: Single<&mut TextColor, (With<BossRage>, Without<BossName>, Without<BossTier>)>,
    mut meter: Single<&mut Meter, With<BossMeter>>,
) {
    let fighting = keepers.iter().find(|(k, h, ..)| seen.0 && k.awake && !h.is_dead());
    let (node, border) = &mut *bar;
    let display = if fighting.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some((keeper, health, prism, brute)) = fighting else { return };
    let hardened = brute.is_some_and(|brute| brute.state == BruteState::Harden);

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
    meter.color = if hardened { ARMOUR_COLOR } else { accent };

    // The prism's phase; exposed it throbs, and a coming shift blinks after an arrow.
    let display = if prism.is_some() || hardened { Display::Flex } else { Display::None };
    if phase_row.display != display {
        phase_row.display = display;
    }
    if hardened {
        let mend = 0.65 + 0.35 * (time.elapsed_secs() * 3.0).sin();
        for (part, mut text, mut tint) in &mut phases {
            let (label, color) = match part {
                BossPhase::Now => ("HARDENED", ARMOUR_COLOR),
                BossPhase::Arrow => ("+", HEAL_COLOR.with_alpha(mend)),
                BossPhase::Next => ("HEALING", HEAL_COLOR.with_alpha(mend)),
            };
            if text.0 != label {
                text.0 = label.into();
            }
            tint.set_if_neq(TextColor(color));
        }
        return;
    }
    let Some(prism) = prism else { return };
    let throb = 0.65 + 0.35 * (time.elapsed_secs() * 7.0).sin();
    let shifting = prism.next != prism.phase;
    for (part, mut text, mut tint) in &mut phases {
        let (label, color) = match part {
            BossPhase::Now => {
                let (label, color) = phase_label(prism.phase);
                let alpha = if prism.phase == PrismPhase::Exposed { throb } else { 1.0 };
                (label, color.with_alpha(alpha))
            }
            BossPhase::Arrow => (">", if shifting { hud::DIM } else { Color::NONE }),
            BossPhase::Next => {
                let (label, color) = phase_label(prism.next);
                (if shifting { label } else { "" }, if blink { color } else { color.with_alpha(0.4) })
            }
        };
        if text.0 != label {
            text.0 = label.into();
        }
        tint.set_if_neq(TextColor(color));
    }
}
