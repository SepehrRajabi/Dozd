use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::net::{LocalPlayer, authority};
use crate::room::RoomGrid;
use crate::{GameState, Level, PIXEL_SCALE, Rng};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .init_resource::<Noise>()
            .add_observer(spawn_fx)
            .add_observer(dress_projectile)
            .add_systems(OnEnter(GameState::Playing), reset_combat)
            .add_systems(
                Update,
                (
                    (tick_health, move_projectiles)
                        .run_if(in_state(GameState::Playing))
                        .run_if(authority),
                    (fade_fx, float_popups, shake_on_local_damage),
                ),
            );
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Player,
    Enemy,
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    /// Absorbs damage before `current`; regenerates after a quiet period.
    pub shield: f32,
    pub max_shield: f32,
    /// Seconds since the last hit, for shield regen.
    since_hit: f32,
    /// Seconds left on the hit flash.
    pub flash: f32,
    /// Whether the last hit was fully absorbed by the shield.
    pub shield_hit: bool,
    /// Seconds left before this can be hurt again.
    pub invulnerable: f32,
    /// Grace period granted after each hit.
    grace: f32,
}

/// Shield regen waits this long after the last hit.
const SHIELD_DELAY: f32 = 3.0;
const SHIELD_REGEN: f32 = 25.0;

impl Health {
    pub fn new(max: f32, grace: f32) -> Self {
        Self {
            current: max,
            max,
            shield: 0.0,
            max_shield: 0.0,
            since_hit: 0.0,
            flash: 0.0,
            shield_hit: false,
            invulnerable: 0.0,
            grace,
        }
    }

    pub fn with_shield(mut self, max: f32) -> Self {
        self.shield = max;
        self.max_shield = max;
        self
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    /// Returns false if the hit was ignored (dead or invulnerable).
    pub fn hurt(&mut self, damage: f32) -> bool {
        if self.invulnerable > 0.0 || self.is_dead() {
            return false;
        }
        let absorbed = damage.min(self.shield);
        self.shield -= absorbed;
        self.current -= damage - absorbed;
        self.shield_hit = absorbed >= damage;
        self.since_hit = 0.0;
        self.flash = 0.12;
        self.invulnerable = self.grace;
        true
    }
}

/// Circular hit area.
#[derive(Component)]
pub struct Hurtbox(pub f32);

/// Knockback velocity applied by the owner's movement code, decays over time.
#[derive(Component, Default)]
pub struct Knockback(pub Vec2);

/// Server-side projectile state; clients only see its [`ProjectileLook`] and transform.
#[derive(Component)]
pub struct Projectile {
    pub team: Team,
    pub velocity: Vec2,
    pub remaining: f32,
    pub damage: f32,
    pub knockback: f32,
    /// Passes through targets instead of stopping at the first one.
    pub pierce: bool,
    pub color: Color,
    pub hits: Vec<Entity>,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectileLook {
    CyanBolt,
    OrangePellet,
    GreenBolt,
    RailBeam,
    EnemyBolt,
    EnemyOrb,
}

impl ProjectileLook {
    fn sprite(self) -> &'static str {
        match self {
            Self::CyanBolt => "sprites/fx/bolt_cyan.png",
            Self::OrangePellet => "sprites/fx/pellet_orange.png",
            Self::GreenBolt => "sprites/fx/bolt_green.png",
            Self::RailBeam => "sprites/fx/rail_purple.png",
            Self::EnemyBolt => "sprites/fx/enemy_bolt.png",
            Self::EnemyOrb => "sprites/fx/enemy_orb.png",
        }
    }
}

pub fn spawn_projectile(commands: &mut Commands, projectile: Projectile, look: ProjectileLook, pos: Vec2) {
    let angle = projectile.velocity.to_angle();
    commands.spawn((
        Level,
        Replicated,
        projectile,
        look,
        Transform::from_translation(pos.extend(15.0)).with_rotation(Quat::from_rotation_z(angle)),
    ));
}

fn dress_projectile(add: On<Add, ProjectileLook>, mut commands: Commands, assets: Res<AssetServer>, looks: Query<&ProjectileLook>) {
    let Ok(look) = looks.get(add.entity) else { return };
    commands
        .entity(add.entity)
        .insert(Sprite::from_image(assets.load(look.sprite())));
}

/// A cosmetic effect the host broadcasts so every machine draws it locally.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct Fx {
    pub kind: FxKind,
    pub pos: Vec2,
    pub angle: f32,
    pub color: Color,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum FxKind {
    Spark,
    Burst,
    MuzzleFlash,
    /// Floating "+N cr" after picking up loot.
    Popup(u32),
}

/// Broadcasts an effect to every machine, including this one.
pub fn fx(commands: &mut Commands, kind: FxKind, pos: Vec2, angle: f32, color: Color) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Fx { kind, pos, angle, color },
    });
}

#[derive(Component)]
pub struct Fade {
    pub age: f32,
    pub life: f32,
}

#[derive(Component)]
struct Popup {
    age: f32,
}

/// Camera shake intensity in [0, 1]; the camera reads and decays it.
#[derive(Resource, Default)]
pub struct Shake(pub f32);

impl Shake {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount).min(1.0);
    }
}

/// Where pirates fired since the enemies last listened.
#[derive(Resource, Default)]
pub struct Noise(pub Vec<Vec2>);

fn spawn_fx(fx: On<Fx>, mut commands: Commands, assets: Res<AssetServer>, mut rng: ResMut<Rng>, mut shake: ResMut<Shake>) {
    let spark = |commands: &mut Commands, pos: Vec2, life: f32| {
        commands.spawn((
            Level,
            Fade { age: 0.0, life },
            Sprite {
                image: assets.load("sprites/fx/spark.png"),
                color: fx.color,
                ..default()
            },
            Transform::from_translation(pos.extend(16.0)),
        ));
    };

    match fx.kind {
        FxKind::Spark => spark(&mut commands, fx.pos, 0.15),
        FxKind::Burst => {
            for _ in 0..6 {
                let offset = Vec2::new(rng.signed(), rng.signed()) * 7.0;
                spark(&mut commands, fx.pos + offset, 0.2 + rng.unit() * 0.25);
            }
            shake.add(0.1);
        }
        FxKind::MuzzleFlash => {
            commands.spawn((
                Level,
                Fade { age: 0.0, life: 0.06 },
                Sprite {
                    image: assets.load("sprites/fx/muzzle_flash.png"),
                    color: fx.color,
                    ..default()
                },
                Transform::from_translation(fx.pos.extend(16.0)).with_rotation(Quat::from_rotation_z(fx.angle)),
            ));
        }
        FxKind::Popup(credits) => {
            commands.spawn((
                Level,
                Popup { age: 0.0 },
                Text2d::new(format!("+{credits} cr")),
                TextFont::from_font_size(8.0 * PIXEL_SCALE),
                TextColor(fx.color),
                Transform::from_translation((fx.pos + Vec2::new(0.0, 10.0)).extend(20.0))
                    .with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
            ));
        }
    }
}

fn reset_combat(mut shake: ResMut<Shake>, mut noise: ResMut<Noise>) {
    shake.0 = 0.0;
    noise.0.clear();
}

fn tick_health(time: Res<Time>, mut health: Query<&mut Health>) {
    let dt = time.delta_secs();
    for mut h in &mut health {
        h.flash = (h.flash - dt).max(0.0);
        h.invulnerable = (h.invulnerable - dt).max(0.0);
        h.since_hit += dt;
        if h.since_hit >= SHIELD_DELAY && !h.is_dead() && h.shield < h.max_shield {
            h.shield = (h.shield + SHIELD_REGEN * dt).min(h.max_shield);
        }
    }
}

fn move_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
    targets: Query<(Entity, &Team, &Hurtbox, &Transform), Without<Projectile>>,
    mut victims: Query<(&mut Health, Option<&mut Knockback>)>,
) {
    // Sub-step so fast shots can't tunnel through walls or targets in one frame.
    const MAX_STEP: f32 = 4.0;

    'projectiles: for (entity, mut projectile, mut transform) in &mut projectiles {
        let travel = projectile.velocity * time.delta_secs();
        let steps = (travel.length() / MAX_STEP).ceil().max(1.0);
        for _ in 0..steps as u32 {
            transform.translation += (travel / steps).extend(0.0);
            let pos = transform.translation.truncate();

            if grid.is_solid_at(pos) {
                fx(&mut commands, FxKind::Spark, pos, 0.0, projectile.color);
                commands.entity(entity).despawn();
                continue 'projectiles;
            }

            for (target, team, hurtbox, target_transform) in &targets {
                if *team == projectile.team
                    || projectile.hits.contains(&target)
                    || target_transform.translation.truncate().distance(pos) > hurtbox.0
                {
                    continue;
                }
                let Ok((mut health, knockback)) = victims.get_mut(target) else { continue };
                if !health.hurt(projectile.damage) {
                    continue;
                }
                if let Some(mut knockback) = knockback {
                    knockback.0 += projectile.velocity.normalize_or_zero() * projectile.knockback;
                }
                fx(&mut commands, FxKind::Spark, pos, 0.0, projectile.color);
                projectile.hits.push(target);
                if !projectile.pierce {
                    commands.entity(entity).despawn();
                    continue 'projectiles;
                }
            }
        }

        projectile.remaining -= travel.length();
        if projectile.remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn fade_fx(mut commands: Commands, time: Res<Time>, mut fx: Query<(Entity, &mut Fade, &mut Sprite)>) {
    for (entity, mut fade, mut sprite) in &mut fx {
        fade.age += time.delta_secs();
        if fade.age >= fade.life {
            commands.entity(entity).despawn();
        } else {
            sprite.color.set_alpha(1.0 - fade.age / fade.life);
        }
    }
}

fn float_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &mut Popup, &mut Transform, &mut TextColor)>,
) {
    const LIFETIME: f32 = 1.2;
    for (entity, mut popup, mut transform, mut color) in &mut popups {
        popup.age += time.delta_secs();
        transform.translation.y += 18.0 * time.delta_secs();
        color.0.set_alpha(1.0 - (popup.age / LIFETIME).powi(2));
        if popup.age >= LIFETIME {
            commands.entity(entity).despawn();
        }
    }
}

/// Shakes the camera whenever this machine's pirate loses shield or hull,
/// however the damage arrived (locally or via replication).
fn shake_on_local_damage(
    mut shake: ResMut<Shake>,
    player: Option<Single<&Health, With<LocalPlayer>>>,
    mut last: Local<Option<f32>>,
) {
    let Some(health) = player else {
        *last = None;
        return;
    };
    let total = health.current + health.shield;
    if let Some(previous) = *last
        && total < previous - 0.5
    {
        shake.add(0.45);
    }
    *last = Some(total);
}
