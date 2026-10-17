//! What enemies may leave behind: a few credits, an ammo box. Drops pop out of
//! the body, then drift toward any pirate who comes close and are picked up
//! on touch. An ammo box holds a magazine for one gun the crew carries, leaning
//! toward guns running low, and only goes to a pirate with room for it.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, fx};
use crate::guns::{Arsenal, GunKind};
use crate::net::authority;
use crate::player::{PirateStatus, Player};
use crate::room::RoomGrid;
use crate::{GameState, Level, Rng};

const PICKUP_RADIUS: f32 = 8.0;
/// Drops start drifting toward a pirate this close.
const MAGNET_RADIUS: f32 = 30.0;
const MAGNET_SPEED: f32 = 140.0;
const CREDIT_COLOR: Color = Color::srgb(1.0, 0.84, 0.3);
/// Weight of a carried gun in the ammo pick even when no carrier needs rounds,
/// next to 1.0 for each carrier below their spare cap.
const AMMO_BASE_WEIGHT: f32 = 0.25;

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
    /// A magazine of spare rounds for this gun only.
    Ammo(GunKind),
}

impl EnemyDrop {
    fn sprite(self) -> String {
        match self {
            Self::Credits(_) => "sprites/drops/credits.png".to_string(),
            Self::Ammo(kind) => format!("sprites/drops/ammo_{}.png", kind.stem()),
        }
    }
}

/// Which gun an ammo box is for, from the arsenals of the living crew: each
/// carried limited-ammo gun weighs `AMMO_BASE_WEIGHT` plus one per carrier
/// with room for more. `None` if nobody carries one.
pub fn pick_ammo<'a>(rng: &mut Rng, arsenals: impl IntoIterator<Item = &'a Arsenal>) -> Option<GunKind> {
    let mut weights: Vec<(GunKind, f32)> = Vec::new();
    for arsenal in arsenals {
        for kind in arsenal.limited_guns() {
            let need = if arsenal.needs_ammo_for(kind) { 1.0 } else { 0.0 };
            match weights.iter_mut().find(|(k, _)| *k == kind) {
                Some((_, w)) => *w += need,
                None => weights.push((kind, AMMO_BASE_WEIGHT + need)),
            }
        }
    }
    // Fixed order, so the same roll picks the same gun on every run.
    weights.sort_by_key(|(kind, _)| GunKind::ALL.iter().position(|k| k == kind));
    let total: f32 = weights.iter().map(|(_, w)| w).sum();
    let mut roll = rng.unit() * total;
    for &(kind, weight) in &weights {
        if roll < weight {
            return Some(kind);
        }
        roll -= weight;
    }
    weights.last().map(|&(kind, _)| kind)
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
        // Ammo only goes to pirates who carry its gun and have room for it.
        let nearest = players
            .iter_mut()
            .filter(|(player, health, _, arsenal)| {
                player.status == PirateStatus::Active
                    && !health.is_dead()
                    && match *drop {
                        EnemyDrop::Ammo(kind) => arsenal.needs_ammo_for(kind),
                        EnemyDrop::Credits(_) => true,
                    }
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
            EnemyDrop::Ammo(kind) => {
                let rounds = arsenal.add_ammo_for(kind);
                fx(&mut commands, FxKind::Ammo(kind, rounds), pos, 0.0, kind.stats().color);
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// An arsenal of `kinds` with every limited gun's spare rounds full.
    fn full(kinds: &[GunKind]) -> Arsenal {
        let mut arsenal = Arsenal::new(kinds);
        for &kind in kinds {
            while arsenal.needs_ammo_for(kind) {
                arsenal.add_ammo_for(kind);
            }
        }
        arsenal
    }

    fn picks(arsenals: &[Arsenal], rolls: usize) -> Vec<GunKind> {
        let mut rng = Rng::new(7);
        (0..rolls).filter_map(|_| pick_ammo(&mut rng, arsenals)).collect()
    }

    #[test]
    fn only_carried_limited_guns() {
        let crew = [
            Arsenal::new(&[GunKind::BlasterPistol, GunKind::RailLance]),
            Arsenal::new(&[GunKind::BlasterPistol, GunKind::ArcCoil]),
        ];
        let got = picks(&crew, 500);
        assert_eq!(got.len(), 500);
        assert!(got.iter().all(|k| matches!(k, GunKind::RailLance | GunKind::ArcCoil)));
        assert!(got.contains(&GunKind::RailLance) && got.contains(&GunKind::ArcCoil));
    }

    #[test]
    fn none_without_limited_guns() {
        let crew = [Arsenal::new(&[GunKind::BlasterPistol])];
        assert!(picks(&crew, 50).is_empty());
        assert!(picks(&[], 50).is_empty());
    }

    #[test]
    fn full_guns_still_drop() {
        let crew = [full(&[GunKind::BlasterPistol, GunKind::PulseRifle])];
        assert!(picks(&crew, 20).iter().all(|&k| k == GunKind::PulseRifle));
    }

    #[test]
    fn leans_toward_needed_guns() {
        // Two carriers short on the Scatter Cannon, one full Rail Lance:
        // weights 2.25 to 0.25, so about 90% scatter.
        let crew = [
            Arsenal::new(&[GunKind::BlasterPistol, GunKind::ScatterCannon]),
            Arsenal::new(&[GunKind::BlasterPistol, GunKind::ScatterCannon]),
            full(&[GunKind::BlasterPistol, GunKind::RailLance]),
        ];
        let got = picks(&crew, 2000);
        let scatter = got.iter().filter(|&&k| k == GunKind::ScatterCannon).count() as f32 / got.len() as f32;
        assert!((0.85..0.95).contains(&scatter), "scatter share {scatter}");
    }

    #[test]
    fn ammo_goes_to_its_gun_only() {
        let mut arsenal = Arsenal::new(&[GunKind::BlasterPistol, GunKind::RailLance]);
        assert!(arsenal.needs_ammo_for(GunKind::RailLance));
        assert!(!arsenal.needs_ammo_for(GunKind::ArcCoil));
        assert!(!arsenal.needs_ammo_for(GunKind::BlasterPistol));
        assert_eq!(arsenal.add_ammo_for(GunKind::ArcCoil), 0);
        assert!(arsenal.add_ammo_for(GunKind::RailLance) > 0);
    }

    #[test]
    fn ammo_sprite_paths() {
        assert_eq!(EnemyDrop::Ammo(GunKind::RailLance).sprite(), "sprites/drops/ammo_rail_lance.png");
        assert_eq!(EnemyDrop::Ammo(GunKind::GrenadeLauncher).sprite(), "sprites/drops/ammo_grenade_launcher.png");
    }
}
