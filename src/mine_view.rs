//! How a Doctor's leech mine looks: a steel puck that pops down when planted,
//! sits dark while it arms, then blinks pink over a faint ring showing how
//! close a guard can come. The host draws the blast itself.

use bevy::prelude::*;

use crate::class::{LeechMine, MEDIC_COLOR, MINE_ARM, MINE_TRIGGER};

/// The shock ring sprite's width, to scale it to a radius.
const RING_SIZE: f32 = 113.0;
/// Seconds the plant pop takes to settle, and how big it starts.
const POP_LIFE: f32 = 0.2;
const POP_SCALE: f32 = 1.6;
/// Seconds between blinks once armed, and how long each blink stays lit.
const BLINK_GAP: f32 = 0.7;
const BLINK_ON: f32 = 0.18;
/// Seconds the trigger ring takes to swell out once the mine arms.
const RING_GROW: f32 = 0.25;
/// Where the glow sits over the cross on the puck.
const LIGHT_AT: Vec2 = Vec2::new(0.0, 1.0);

pub struct MineViewPlugin;

impl Plugin for MineViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_mine).add_systems(Update, draw_mines);
    }
}

/// How long a mine has been down, and been live, as this client saw it: for
/// the pop, the blink and the ring.
#[derive(Component, Default)]
struct MineClock {
    age: f32,
    armed_for: f32,
}

/// The puck itself.
#[derive(Component)]
struct MineBody;

/// The pink glow over the cross, lit once armed.
#[derive(Component)]
struct MineLight;

/// The faint ring on the floor showing `MINE_TRIGGER`.
#[derive(Component)]
struct MineRing;

/// Dresses a planted mine. Everything hangs off children, so the replicated
/// `Transform` is never touched.
fn dress_mine(add: On<Add, LeechMine>, mut commands: Commands, assets: Res<AssetServer>) {
    commands.entity(add.entity).insert((MineClock::default(), Visibility::default())).with_children(|parent| {
        parent.spawn((
            MineRing,
            Visibility::Hidden,
            Sprite { image: assets.load("sprites/fx/shock_ring.png"), color: MEDIC_COLOR.with_alpha(0.0), ..default() },
            Transform::from_xyz(0.0, 0.0, -0.4).with_scale(Vec3::splat(2.0 * MINE_TRIGGER / RING_SIZE)),
        ));
        parent.spawn((Sprite::from_image(assets.load("sprites/shadow.png")), Transform::from_xyz(0.0, 1.0, -0.2)));
        parent.spawn((
            MineBody,
            Sprite::from_image(assets.load("sprites/mine.png")),
            Transform::from_scale(Vec3::splat(POP_SCALE)),
        ));
        parent.spawn((
            MineLight,
            Visibility::Hidden,
            Sprite::from_image(assets.load("sprites/mine_light.png")),
            Transform::from_translation(LIGHT_AT.extend(0.1)),
        ));
    });
}

/// Pops a mine down as it lands, keeps it dark while it arms, then blinks its
/// light and breathes its trigger ring once it's live.
fn draw_mines(
    time: Res<Time>,
    mut mines: Query<(&LeechMine, &mut MineClock)>,
    mut bodies: Query<(&ChildOf, &mut Transform, &mut Sprite), (With<MineBody>, Without<MineLight>, Without<MineRing>)>,
    mut lights: Query<(&ChildOf, &mut Visibility, &mut Sprite), (With<MineLight>, Without<MineBody>, Without<MineRing>)>,
    mut rings: Query<(&ChildOf, &mut Visibility, &mut Transform, &mut Sprite), (With<MineRing>, Without<MineBody>, Without<MineLight>)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mine, mut clock) in &mut mines {
        clock.age += dt;
        if mine.arm <= 0.0 {
            clock.armed_for += dt;
        }
    }
    for (parent, mut transform, mut sprite) in &mut bodies {
        let Ok((mine, clock)) = mines.get(parent.parent()) else { continue };
        // Slams down big, then settles to size.
        let pop = (clock.age / POP_LIFE).min(1.0);
        transform.scale = Vec3::splat(1.0 + (POP_SCALE - 1.0) * (1.0 - pop) * (1.0 - pop));
        // Dark while arming, brightening as it nears live.
        let arming = (mine.arm / MINE_ARM).clamp(0.0, 1.0);
        sprite.color = Color::WHITE.mix(&Color::srgb(0.45, 0.45, 0.5), arming);
    }
    for (parent, mut visibility, mut sprite) in &mut lights {
        let Ok((mine, clock)) = mines.get(parent.parent()) else { continue };
        let lit = mine.arm <= 0.0 && clock.armed_for % BLINK_GAP < BLINK_ON;
        visibility.set_if_neq(if lit { Visibility::Inherited } else { Visibility::Hidden });
        // Flares white the moment it arms, then blinks pink.
        let flare = (1.0 - clock.armed_for / BLINK_ON).max(0.0);
        sprite.color = MEDIC_COLOR.mix(&Color::WHITE, 0.3 + 0.5 * flare);
    }
    for (parent, mut visibility, mut transform, mut sprite) in &mut rings {
        let Ok((mine, clock)) = mines.get(parent.parent()) else { continue };
        let armed = mine.arm <= 0.0;
        visibility.set_if_neq(if armed { Visibility::Inherited } else { Visibility::Hidden });
        if !armed {
            continue;
        }
        // Swells out to its reach as the mine goes live, then breathes faintly.
        let grow = (clock.armed_for / RING_GROW).min(1.0);
        transform.scale = Vec3::splat((0.5 + 0.5 * grow) * 2.0 * MINE_TRIGGER / RING_SIZE);
        sprite.color = MEDIC_COLOR.with_alpha((0.2 + 0.05 * (t * 3.0 + clock.age).sin()) * grow);
    }
}

