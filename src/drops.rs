//! What enemies may leave behind: a few credits, an ammo box. Drops pop out of
//! the body, then drift toward any pirate who comes close and are picked up
//! on touch.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::guns::Arsenal;
use crate::net::authority;
use crate::player::{PirateStatus, Player};
use crate::room::RoomGrid;
use crate::{GameState, Level, Rng};

const PICKUP_RADIUS: f32 = 8.0;
/// Drops start drifting toward a pirate this close.
const MAGNET_RADIUS: f32 = 30.0;
const MAGNET_SPEED: f32 = 140.0;
const CREDIT_COLOR: Color = Color::srgb(1.0, 0.84, 0.3);
const AMMO_COLOR: Color = Color::srgb(0.85, 0.9, 0.55);

pub struct DropsPlugin;

impl Plugin for DropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (scatter_drops, collect_drops)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(authority),
        );
    }
}

pub struct DropsViewPlugin;

impl Plugin for DropsViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_drop).add_systems(Update, bob_drops);
    }
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyDrop {
    Credits(u32),
    /// A magazine for every gun with limited ammo.
    Ammo,
}

impl EnemyDrop {
    fn sprite(self) -> &'static str {
        match self {
            Self::Credits(_) => "sprites/drops/credits.png",
            Self::Ammo => "sprites/drops/ammo.png",
        }
    }
}

/// Host-only: the pop out of the body when the drop spawns.
#[derive(Component)]
struct Scatter(Vec2);

#[derive(Component)]
struct DropSprite {
    phase: f32,
}

/// Throws `drops` out of a body at `pos` in random directions.
pub fn spawn_drops(commands: &mut Commands, rng: &mut Rng, pos: Vec2, drops: &[EnemyDrop]) {
    for &drop in drops {
        let dir = Vec2::from_angle(rng.unit() * std::f32::consts::TAU);
        commands.spawn((
            Level,
            Replicated,
            drop,
            Scatter(dir * (50.0 + rng.unit() * 40.0)),
            Transform::from_translation(pos.extend(4.0)),
        ));
    }
}

fn dress_drop(add: On<Add, EnemyDrop>, mut commands: Commands, assets: Res<AssetServer>, drops: Query<(&EnemyDrop, &Transform)>) {
    let Ok((drop, transform)) = drops.get(add.entity) else { return };
    let pos = transform.translation.truncate();
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, -3.0, -0.1).with_scale(Vec3::splat(0.6)),
        ));
        parent.spawn((
            DropSprite { phase: pos.x * 0.41 + pos.y * 0.23 },
            Sprite::from_image(assets.load(drop.sprite())),
        ));
    });
}

fn bob_drops(time: Res<Time>, mut sprites: Query<(&DropSprite, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (drop, mut transform) in &mut sprites {
        transform.translation.y = ((t * 3.5 + drop.phase).sin() + 1.0).round();
    }
}

fn scatter_drops(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut drops: Query<(Entity, &mut Scatter, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut scatter, mut transform) in &mut drops {
        let new = grid.slide(transform.translation.truncate(), scatter.0 * dt, Vec2::splat(3.0));
        transform.translation.x = new.x;
        transform.translation.y = new.y;
        scatter.0 *= (-7.0 * dt).exp();
        if scatter.0.length() < 2.0 {
            commands.entity(entity).remove::<Scatter>();
        }
    }
}

fn collect_drops(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut players: Query<(&mut Player, &Health, &Transform, &mut Arsenal)>,
    mut drops: Query<(Entity, &EnemyDrop, &mut Transform), (Without<Player>, Without<Scatter>)>,
) {
    for (entity, drop, mut transform) in &mut drops {
        let pos = transform.translation.truncate();
        // Ammo only goes to pirates who have room for it.
        let nearest = players
            .iter_mut()
            .filter(|(player, health, _, arsenal)| {
                player.status == PirateStatus::Active
                    && !health.is_dead()
                    && (*drop != EnemyDrop::Ammo || arsenal.needs_ammo())
            })
            .map(|(player, _, t, arsenal)| (t.translation.truncate().distance(pos), t.translation.truncate(), player, arsenal))
            .filter(|(dist, ..)| *dist <= MAGNET_RADIUS)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((dist, target, mut player, mut arsenal)) = nearest else { continue };

        if dist > PICKUP_RADIUS {
            let step = (target - pos).normalize_or_zero() * MAGNET_SPEED * time.delta_secs();
            let new = grid.slide(pos, step, Vec2::splat(3.0));
            transform.translation.x = new.x;
            transform.translation.y = new.y;
            continue;
        }

        commands.entity(entity).despawn();
        match *drop {
            EnemyDrop::Credits(amount) => {
                player.credits += amount;
                fx(&mut commands, FxKind::Popup(amount), pos, 0.0, CREDIT_COLOR);
            }
            EnemyDrop::Ammo => {
                arsenal.add_ammo();
                fx(&mut commands, FxKind::Ammo, pos, 0.0, AMMO_COLOR);
            }
        }
    }
}

