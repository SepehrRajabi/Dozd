use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::abilities::{RICOCHET_DAMAGE, RICOCHET_REACH, RICOCHET_TARGETS};
use crate::class::{Boons, RIOT_POINTS, RIOT_THICKNESS, RiotShield};
use crate::guns::GunKind;
use crate::net::{LocalPlayer, authority};
use crate::perks::PerkKind;
use crate::room::RoomGrid;
use crate::{GameState, Level, PIXEL_SCALE, Rng};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Noise>()
            .add_message::<CrewHit>()
            .add_systems(OnEnter(GameState::Playing), reset_noise)
            .add_systems(
                Update,
                (tick_health, move_projectiles, move_grenades, move_novas)
                    .run_if(in_state(GameState::Playing))
                    .run_if(authority),
            );
    }
}

pub struct CombatViewPlugin;

impl Plugin for CombatViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .add_observer(spawn_fx)
            .add_observer(dress_projectile)
            .add_systems(OnEnter(GameState::Playing), reset_shake)
            .add_systems(Update, (fade_fx, float_popups, shake_on_local_damage, spin_sprites.run_if(in_state(GameState::Playing))));
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
    /// Seconds since the last hit, for shield regen. Host-only bookkeeping:
    /// ticking it doesn't mark the component changed (see `tick_health`), so
    /// clients only see it when something else changes.
    pub(crate) since_hit: f32,
    /// Seconds left on the hit flash.
    pub flash: f32,
    /// Whether the last hit was fully absorbed by the shield.
    pub shield_hit: bool,
    /// Seconds left before this can be hurt again.
    pub invulnerable: f32,
    /// Seconds left on a Doctor's ward: unhurtable, like `invulnerable`, but
    /// drawn as a bubble rather than the post-hit blink.
    pub warded: f32,
    /// Shrugs off every hit while set, of every sort: a Vault Prism's aegis,
    /// kept in step with its phase.
    pub negating: bool,
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
            warded: 0.0,
            negating: false,
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

    /// Restores hull, up to the max. The dead stay dead.
    pub fn heal(&mut self, amount: f32) {
        if !self.is_dead() {
            self.current = (self.current + amount).min(self.max);
        }
    }

    /// Returns false if the hit was ignored (dead, invulnerable, warded or negating).
    pub fn hurt(&mut self, damage: f32) -> bool {
        self.hurt_for(damage).is_some()
    }

    /// Like `hurt`, but says how much shield and hull the hit actually took:
    /// never more than was left, so overkill doesn't count. `None` if ignored.
    pub fn hurt_for(&mut self, damage: f32) -> Option<f32> {
        if self.invulnerable > 0.0 || self.warded > 0.0 || self.is_dead() {
            return None;
        }
        if self.negating {
            // Shrugged off, but still flashes, so the view can show it landed.
            self.flash = 0.12;
            return None;
        }
        let dealt = damage.min(self.shield + self.current);
        let absorbed = damage.min(self.shield);
        self.shield -= absorbed;
        self.current -= damage - absorbed;
        self.shield_hit = absorbed >= damage;
        self.since_hit = 0.0;
        self.flash = 0.12;
        self.invulnerable = self.grace;
        Some(dealt)
    }
}

/// Circular hit area.
#[derive(Component)]
pub struct Hurtbox(pub f32);

/// Host only: while `active`, the other side's shots, grenades and nova orbs
/// that reach this bounce back at them as this side's own, weaker, in `color`
/// (a Vault Prism's mirror). Anything else still hurts it.
#[derive(Component, Clone, Copy)]
pub struct Reflector {
    pub active: bool,
    pub color: Color,
}

/// Shots, grenades and orbs a `Reflector` sends back hit this much as hard.
pub const REFLECT_DAMAGE: f32 = 0.6;
/// How far a reflected shot or orb flies.
const REFLECT_RANGE: f32 = 260.0;
/// A reflected shot or orb heads straight back the way it came, unless
/// someone on the other side is within this many radians of that line: then
/// it's nudged onto the nearest of them.
const REFLECT_AIM: f32 = 25.0 * std::f32::consts::PI / 180.0;

/// Knockback velocity applied by the owner's movement code, decays over time.
#[derive(Component, Default)]
pub struct Knockback(pub Vec2);

/// Host-side: a pirate's own weapon hurt a guard, by `dealt` (see
/// `Health::hurt_for`). Sent wherever pirates deal damage, for passives and
/// challenges that feed on it.
#[derive(Message, Clone, Copy, Debug)]
pub struct CrewHit {
    /// `Player::owner` of the pirate whose weapon it was.
    pub owner: u64,
    pub dealt: f32,
    /// The hit took the guard from alive to dead: a kill to their name.
    pub killed: bool,
    /// Lifesteal may heal the pirate for it. Off for leech mines, which heal
    /// the crew themselves.
    pub drains: bool,
}

/// Server-side projectile state; clients only see its [`ProjectileLook`] and transform.
#[derive(Component)]
pub struct Projectile {
    pub team: Team,
    /// `Player::owner` of the pirate who fired it; `None` for guards and sentries.
    pub owner: Option<u64>,
    pub velocity: Vec2,
    pub remaining: f32,
    pub damage: f32,
    pub knockback: f32,
    /// Passes through targets instead of stopping at the first one.
    pub pierce: bool,
    pub color: Color,
    pub hits: Vec<Entity>,
    /// Odds that hitting a guard sends it bouncing on into others nearby.
    pub ricochet: f32,
}

#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectileLook {
    CyanBolt,
    OrangePellet,
    GreenBolt,
    RailBeam,
    EnemyBolt,
    EnemyOrb,
    Grenade,
    NovaOrb,
}

impl ProjectileLook {
    pub fn sprite(self) -> &'static str {
        match self {
            Self::CyanBolt => "sprites/fx/bolt_cyan.png",
            Self::OrangePellet => "sprites/fx/pellet_orange.png",
            Self::GreenBolt => "sprites/fx/bolt_green.png",
            Self::RailBeam => "sprites/fx/rail_purple.png",
            Self::EnemyBolt => "sprites/fx/enemy_bolt.png",
            Self::EnemyOrb => "sprites/fx/enemy_orb.png",
            Self::Grenade => "sprites/fx/grenade.png",
            Self::NovaOrb => "sprites/fx/nova_orb.png",
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

/// Host-side grenade in flight: follows an arc from `from` and explodes at `to`.
#[derive(Component)]
pub struct Grenade {
    /// The thrower's side: its blast hurts the other.
    pub team: Team,
    /// `Player::owner` of the pirate who threw it.
    pub owner: Option<u64>,
    pub from: Vec2,
    pub to: Vec2,
    pub age: f32,
    /// Seconds from launch to landing.
    pub flight: f32,
    /// Damage at the centre of the blast, halving toward the edge.
    pub damage: f32,
    pub radius: f32,
    pub knockback: f32,
    pub color: Color,
}

/// Peak height of a grenade's arc for every second of flight.
const LOB_HEIGHT: f32 = 40.0;

pub fn spawn_grenade(commands: &mut Commands, grenade: Grenade) {
    let pos = grenade.from;
    commands.spawn((
        Level,
        Replicated,
        grenade,
        ProjectileLook::Grenade,
        Transform::from_translation(pos.extend(15.0)),
    ));
}

/// Host-side Nova Cannon orb: drifts straight on, lashing out with arcs at
/// guards it passes, and bursts on the first wall, guard or riot shield it
/// meets. Sent back by a `Reflector`, the same goes for the crew.
#[derive(Component)]
pub struct NovaOrb {
    /// Whose side loosed it: its arcs and burst hurt the other.
    pub team: Team,
    /// `Player::owner` of the pirate who loosed it.
    pub owner: Option<u64>,
    pub velocity: Vec2,
    pub remaining: f32,
    /// Damage at the centre of the burst, halving toward the edge.
    pub damage: f32,
    pub radius: f32,
    pub knockback: f32,
    /// Damage of each arc to a guard it passes, and how far the arcs reach.
    pub arc_damage: f32,
    pub arc_reach: f32,
    /// Seconds until it lashes out again.
    pub arc_timer: f32,
    pub color: Color,
}

/// Seconds between a nova orb's arcs, and how many guards each lashes.
const NOVA_ARC_GAP: f32 = 0.2;
const NOVA_ARC_TARGETS: usize = 2;

/// Launches a nova orb, drawn at `scale` to show how charged it was.
pub fn spawn_nova(commands: &mut Commands, orb: NovaOrb, pos: Vec2, scale: f32) {
    commands.spawn((
        Level,
        Replicated,
        orb,
        ProjectileLook::NovaOrb,
        Transform::from_translation(pos.extend(15.0)).with_scale(Vec3::splat(scale)),
    ));
}

/// A sprite turning on the spot at this many radians a second, all on this
/// machine: a grenade tumbling or an orb churning. It sits on a child of the
/// replicated entity, as the host's updates replace that one's whole
/// transform, and the host never turns the projectile itself, so spinning
/// costs no bandwidth.
#[derive(Component)]
struct Spin(f32);

fn dress_projectile(add: On<Add, ProjectileLook>, mut commands: Commands, assets: Res<AssetServer>, looks: Query<&ProjectileLook>) {
    let Ok(look) = looks.get(add.entity) else { return };
    let sprite = Sprite::from_image(assets.load(look.sprite()));
    let spin = match look {
        ProjectileLook::Grenade => Some(12.0),
        ProjectileLook::NovaOrb => Some(6.0),
        _ => None,
    };
    let mut entity = commands.entity(add.entity);
    match spin {
        // The parent's scale (a charged nova's size) still carries to the child.
        Some(rate) => entity.insert(Visibility::default()).with_child((Spin(rate), sprite, Transform::default())),
        // Bolts are pointed along their flight by the host's rotation.
        None => entity.insert(sprite),
    };
}

fn spin_sprites(time: Res<Time>, mut sprites: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut sprites {
        transform.rotate_z(spin.0 * time.delta_secs());
    }
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
    /// Perk name and a burst of sparks when one is picked up.
    Perk(PerkKind),
    /// "+12 RAIL" after grabbing an ammo box: the rounds added and their gun.
    Ammo(GunKind, u32),
    /// An electric arc from `pos` to this point.
    Zap(Vec2),
    /// A grenade going off.
    Explosion,
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

fn spawn_fx(fx: On<Fx>, mut commands: Commands, assets: Res<AssetServer>, mut rng: Local<Rng>, mut shake: ResMut<Shake>) {
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
        FxKind::Zap(to) => {
            // A jagged bolt: a few kinked segments, each a stretched 1px sprite.
            let segments = (fx.pos.distance(to) / 7.0).ceil().max(2.0) as usize;
            let normal = (to - fx.pos).normalize_or(Vec2::X).perp();
            let mut points: Vec<Vec2> = (0..=segments)
                .map(|i| fx.pos.lerp(to, i as f32 / segments as f32))
                .collect();
            for p in &mut points[1..segments] {
                *p += normal * rng.signed() * 3.5;
            }
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                commands.spawn((
                    Level,
                    Fade { age: 0.0, life: 0.14 },
                    Sprite::from_color(fx.color, Vec2::new(a.distance(b) + 1.0, 1.0)),
                    Transform::from_translation(((a + b) / 2.0).extend(17.0))
                        .with_rotation(Quat::from_rotation_z((b - a).to_angle())),
                ));
            }
            spark(&mut commands, to, 0.15);
        }
        FxKind::Explosion => {
            commands.spawn((
                Level,
                Fade { age: 0.0, life: 0.3 },
                Sprite {
                    image: assets.load("sprites/fx/explosion.png"),
                    color: fx.color,
                    ..default()
                },
                Transform::from_translation(fx.pos.extend(16.5)),
            ));
            for _ in 0..8 {
                let offset = Vec2::from_angle(rng.unit() * std::f32::consts::TAU) * (4.0 + rng.unit() * 18.0);
                spark(&mut commands, fx.pos + offset, 0.2 + rng.unit() * 0.3);
            }
            shake.add(0.25);
        }
        FxKind::Popup(_) | FxKind::Perk(_) | FxKind::Ammo(..) => {
            let text = match fx.kind {
                FxKind::Perk(kind) => {
                    for i in 0..10 {
                        let dir = Vec2::from_angle(i as f32 / 10.0 * std::f32::consts::TAU);
                        spark(&mut commands, fx.pos + dir * (6.0 + rng.unit() * 6.0), 0.25 + rng.unit() * 0.2);
                    }
                    kind.name().to_string()
                }
                FxKind::Popup(credits) => format!("+{credits} cr"),
                FxKind::Ammo(kind, rounds) => format!("+{rounds} {}", kind.short_name()),
                _ => unreachable!(),
            };
            commands.spawn((
                Level,
                Popup { age: 0.0 },
                Text2d::new(text),
                TextFont::from_font_size(8.0 * PIXEL_SCALE),
                TextColor(fx.color),
                Transform::from_translation((fx.pos + Vec2::new(0.0, 10.0)).extend(20.0))
                    .with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
            ));
        }
    }
}

fn reset_noise(mut noise: ResMut<Noise>) {
    noise.0.clear();
}

fn reset_shake(mut shake: ResMut<Shake>) {
    shake.0 = 0.0;
}

fn tick_health(time: Res<Time>, mut health: Query<(&mut Health, Option<&Boons>)>) {
    let dt = time.delta_secs();
    // Replication resends a `Health` whenever it's marked changed, so it's
    // only marked when something the crew can see moves: timers that have run
    // out are left alone, and the host-only `since_hit` doesn't count.
    for (mut h, boons) in &mut health {
        if h.flash > 0.0 {
            h.flash = (h.flash - dt).max(0.0);
        }
        if h.invulnerable > 0.0 {
            h.invulnerable = (h.invulnerable - dt).max(0.0);
        }
        if h.warded > 0.0 {
            h.warded = (h.warded - dt).max(0.0);
        }
        h.bypass_change_detection().since_hit += dt;
        // Overclocked, the shield starts back sooner and fills faster.
        let rate = boons.map_or(1.0, |b| b.cooldown_rate());
        if h.since_hit * rate >= SHIELD_DELAY && !h.is_dead() && h.shield < h.max_shield {
            h.shield = (h.shield + SHIELD_REGEN * rate * dt).min(h.max_shield);
        }
    }
}

/// What a shot can hit: anyone with a hurtbox, some of them mirrors.
type ShotTargets<'w, 's> =
    Query<'w, 's, (Entity, &'static Team, &'static Hurtbox, &'static Transform, Option<&'static Reflector>), Without<Projectile>>;

#[allow(clippy::too_many_arguments)]
fn move_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut projectiles: Query<(Entity, &mut Projectile, &ProjectileLook, &mut Transform)>,
    targets: ShotTargets,
    mut victims: Query<(&mut Health, Option<&mut Knockback>)>,
    riot_shields: Query<(&RiotShield, &Transform), Without<Projectile>>,
    mut crew_hits: MessageWriter<CrewHit>,
) {
    // Sub-step so fast shots can't tunnel through walls or targets in one frame.
    const MAX_STEP: f32 = 4.0;
    let shields = shield_outlines(&riot_shields);
    let blocked = |pos: Vec2| hits_shield(&shields, pos);

    'projectiles: for (entity, mut projectile, look, mut transform) in &mut projectiles {
        let travel = projectile.velocity * time.delta_secs();
        let steps = (travel.length() / MAX_STEP).ceil().max(1.0);
        for _ in 0..steps as u32 {
            transform.translation += (travel / steps).extend(0.0);
            let pos = transform.translation.truncate();

            // A Bulwark's riot shield stops everything, whoever fired it.
            if grid.is_solid_at(pos) || blocked(pos) {
                fx(&mut commands, FxKind::Spark, pos, 0.0, projectile.color);
                commands.entity(entity).despawn();
                continue 'projectiles;
            }

            for (target, team, hurtbox, target_transform, reflector) in &targets {
                if *team == projectile.team
                    || projectile.hits.contains(&target)
                    || target_transform.translation.truncate().distance(pos) > hurtbox.0
                {
                    continue;
                }
                let Ok((health, _)) = victims.get(target) else { continue };
                if health.is_dead() {
                    continue;
                }
                // A mirror sends it back at whoever fired it.
                if let Some(mirror) = reflector.filter(|r| r.active) {
                    reflect_shot(&mut commands, &projectile, *team, mirror.color, pos, &targets, &victims, &grid);
                    fx(&mut commands, FxKind::Spark, pos, 0.0, mirror.color);
                    commands.entity(entity).despawn();
                    continue 'projectiles;
                }
                // Negated: it stops dead against it, doing nothing.
                if health.negating {
                    fx(&mut commands, FxKind::Spark, pos, 0.0, projectile.color);
                    commands.entity(entity).despawn();
                    continue 'projectiles;
                }
                let Ok((mut health, knockback)) = victims.get_mut(target) else { continue };
                let Some(dealt) = health.hurt_for(projectile.damage) else { continue };
                if let Some(owner) = projectile.owner
                    && *team == Team::Enemy
                {
                    crew_hits.write(CrewHit { owner, dealt, killed: health.is_dead(), drains: true });
                }
                if let Some(mut knockback) = knockback {
                    knockback.0 += projectile.velocity.normalize_or_zero() * projectile.knockback;
                }
                fx(&mut commands, FxKind::Spark, pos, 0.0, projectile.color);
                projectile.hits.push(target);
                if *team == Team::Enemy && projectile.ricochet > 0.0 && rng.unit() < projectile.ricochet {
                    ricochet(&mut commands, &projectile, *look, target, pos, &targets, &victims, &grid);
                }
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

/// The nearest of `spots` to `pos`.
fn nearest(pos: Vec2, spots: impl Iterator<Item = Vec2>) -> Option<Vec2> {
    spots.min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)))
}

/// Which way a shot reflected at `pos` heads: straight `back`, or onto the
/// nearest of `crew` within `REFLECT_AIM` of that line, in sight and in reach.
fn reflect_dir(grid: &RoomGrid, pos: Vec2, back: Vec2, crew: impl Iterator<Item = Vec2>) -> Vec2 {
    let back = back.normalize_or(Vec2::X);
    let near = crew.filter(|at| {
        at.distance(pos) <= REFLECT_RANGE && back.angle_to(*at - pos).abs() <= REFLECT_AIM && grid.line_of_sight(pos, *at)
    });
    nearest(pos, near).map_or(back, |at| (at - pos).normalize_or(back))
}

/// Sends a shot that just reached a `Reflector` on `side` at `pos` back the
/// way it came, as a fresh shot of `side`'s own: weaker, in `color`, nudged
/// onto someone on the other side near its line (see `reflect_dir`).
#[allow(clippy::too_many_arguments)]
fn reflect_shot(
    commands: &mut Commands,
    shot: &Projectile,
    side: Team,
    color: Color,
    pos: Vec2,
    targets: &ShotTargets,
    victims: &Query<(&mut Health, Option<&mut Knockback>)>,
    grid: &RoomGrid,
) {
    let crew = targets
        .iter()
        .filter(|(e, team, ..)| **team != side && victims.get(*e).is_ok_and(|(h, _)| !h.is_dead()))
        .map(|(_, _, _, t, _)| t.translation.truncate());
    let dir = reflect_dir(grid, pos, -shot.velocity, crew);
    spawn_projectile(
        commands,
        Projectile {
            team: side,
            owner: None,
            velocity: dir * shot.velocity.length(),
            remaining: REFLECT_RANGE,
            damage: shot.damage * REFLECT_DAMAGE,
            knockback: shot.knockback * REFLECT_DAMAGE,
            pierce: false,
            color,
            hits: Vec::new(),
            ricochet: 0.0,
        },
        ProjectileLook::EnemyBolt,
        pos,
    );
}

/// Every riot shield's curve, to test shots against.
pub(crate) fn shield_outlines<F: bevy::ecs::query::QueryFilter>(riot_shields: &Query<(&RiotShield, &Transform), F>) -> Vec<[Vec2; RIOT_POINTS]> {
    riot_shields.iter().map(|(s, t)| s.outline(t.translation.truncate())).collect()
}

/// Whether a shot at `pos` is touching any of the shields.
pub(crate) fn hits_shield(shields: &[[Vec2; RIOT_POINTS]], pos: Vec2) -> bool {
    shields.iter().flat_map(|outline| outline.windows(2)).any(|pair| {
        let (a, b) = (pair[0], pair[1]);
        let t = ((pos - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        pos.distance(a + (b - a) * t) <= RIOT_THICKNESS
    })
}

/// Whether the straight line from `from` to `to` passes through any of the
/// shields: a blast or arc going off at `from` doesn't reach someone at `to`
/// behind one.
pub(crate) fn shield_between(shields: &[[Vec2; RIOT_POINTS]], from: Vec2, to: Vec2) -> bool {
    shields.iter().flat_map(|outline| outline.windows(2)).any(|pair| segments_cross(from, to, pair[0], pair[1]))
}

/// Whether the segments `a`-`b` and `c`-`d` cross (parallel ones never do).
/// How far round from straight ahead a raised shield covers its holder from
/// blasts, as the cosine of the angle: anything in front of them.
const SHIELD_COVER: f32 = 0.2;

/// Whether a shield raised `facing` covers the pirate holding it, at
/// `holder`, from a blast or blow coming from `from`. A blast landing by the
/// shield's edge can reach round it, so `shield_between` alone isn't enough.
pub(crate) fn shield_covers(facing: Vec2, holder: Vec2, from: Vec2) -> bool {
    (from - holder).normalize_or_zero().dot(facing) > SHIELD_COVER
}

fn segments_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let (ab, cd) = (b - a, d - c);
    let denom = ab.perp_dot(cd);
    if denom.abs() < 1e-6 {
        return false;
    }
    let t = (c - a).perp_dot(cd) / denom;
    let u = (c - a).perp_dot(ab) / denom;
    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
}

/// What a blast or a nova orb's arc can reach: anyone with a hurtbox and
/// health, some of them mirrors.
type Blastable = (
    &'static Team,
    &'static Hurtbox,
    &'static Transform,
    &'static mut Health,
    Option<&'static mut Knockback>,
    Option<&'static Reflector>,
);

/// Hurts everyone not on `side` within `radius` of `center`, `damage` at the
/// middle halving toward the edge, and throws them outward. The crew are safe
/// behind a riot shield. Reports the hits as `owner`'s, if a pirate set it off.
#[allow(clippy::too_many_arguments)]
fn blast(
    targets: &mut Query<Blastable, impl bevy::ecs::query::QueryFilter>,
    shields: &[[Vec2; RIOT_POINTS]],
    center: Vec2,
    damage: f32,
    radius: f32,
    knockback: f32,
    side: Team,
    owner: Option<u64>,
    crew_hits: &mut MessageWriter<CrewHit>,
) {
    for (team, hurtbox, target_transform, mut health, target_knockback, _) in targets {
        let at = target_transform.translation.truncate();
        let offset = at - center;
        let reach = radius + hurtbox.0;
        if *team == side || offset.length() > reach || (*team == Team::Player && shield_between(shields, center, at)) {
            continue;
        }
        let falloff = 1.0 - 0.5 * offset.length() / reach;
        let Some(dealt) = health.hurt_for(damage * falloff) else { continue };
        if let Some(owner) = owner {
            crew_hits.write(CrewHit { owner, dealt, killed: health.is_dead(), drains: true });
        }
        if let Some(mut target_knockback) = target_knockback {
            target_knockback.0 += offset.normalize_or(Vec2::X) * knockback * falloff;
        }
    }
}

/// Sends a shot that just hit `struck` at `pos` bouncing on toward the guards
/// nearest it, weaker, and without bouncing again.
#[allow(clippy::too_many_arguments)]
fn ricochet(
    commands: &mut Commands,
    shot: &Projectile,
    look: ProjectileLook,
    struck: Entity,
    pos: Vec2,
    targets: &ShotTargets,
    victims: &Query<(&mut Health, Option<&mut Knockback>)>,
    grid: &RoomGrid,
) {
    let mut near: Vec<(f32, Vec2)> = targets
        .iter()
        .filter(|(e, team, ..)| *e != struck && **team == Team::Enemy && victims.get(*e).is_ok_and(|(h, _)| !h.is_dead()))
        .map(|(_, _, _, t, _)| t.translation.truncate())
        .filter(|at| at.distance(pos) <= RICOCHET_REACH && grid.line_of_sight(pos, *at))
        .map(|at| (at.distance(pos), at))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    for &(distance, at) in near.iter().take(RICOCHET_TARGETS) {
        let dir = (at - pos).normalize_or(Vec2::X);
        spawn_projectile(
            commands,
            Projectile {
                velocity: dir * shot.velocity.length(),
                remaining: distance + 12.0,
                damage: shot.damage * RICOCHET_DAMAGE,
                knockback: shot.knockback * RICOCHET_DAMAGE,
                pierce: false,
                hits: vec![struck],
                ricochet: 0.0,
                ..*shot
            },
            look,
            pos,
        );
    }
    if !near.is_empty() {
        fx(commands, FxKind::Burst, pos, 0.0, shot.color);
    }
}

fn move_grenades(
    mut commands: Commands,
    time: Res<Time>,
    mut noise: ResMut<Noise>,
    mut grenades: Query<(Entity, &mut Grenade, &mut Transform)>,
    mut targets: Query<Blastable, Without<Grenade>>,
    riot_shields: Query<(&RiotShield, &Transform), (Without<Grenade>, Without<Hurtbox>)>,
    mut crew_hits: MessageWriter<CrewHit>,
) {
    let shields = shield_outlines(&riot_shields);
    for (entity, mut grenade, mut transform) in &mut grenades {
        grenade.age += time.delta_secs();
        let t = (grenade.age / grenade.flight).min(1.0);
        let height = LOB_HEIGHT * grenade.flight * 4.0 * t * (1.0 - t);
        let pos = grenade.from.lerp(grenade.to, t) + Vec2::Y * height;
        // It tumbles in the view (see `Spin`), not here.
        transform.translation = pos.extend(transform.translation.z);
        if t < 1.0 {
            continue;
        }

        let center = grenade.to;
        // Landing where its blast would reach a mirror, it doesn't go off: it's
        // lobbed back at the nearest of whoever threw it, as the mirror's own.
        let thrower = grenade.team;
        let mirror = targets
            .iter()
            .find(|(team, hurtbox, t, health, _, reflector)| {
                reflector.is_some_and(|r| r.active)
                    && **team != thrower
                    && !health.is_dead()
                    && t.translation.truncate().distance(center) <= grenade.radius + hurtbox.0
            })
            .map(|(team, .., reflector)| (*team, reflector.map_or(grenade.color, |r| r.color)));
        if let Some((side, color)) = mirror {
            fx(&mut commands, FxKind::Spark, center, 0.0, color);
            let crew = targets
                .iter()
                .filter(|(team, _, _, health, ..)| **team == thrower && !health.is_dead())
                .map(|(_, _, t, ..)| t.translation.truncate());
            let Some(to) = nearest(center, crew) else {
                commands.entity(entity).despawn();
                continue;
            };
            let speed = (grenade.from.distance(grenade.to) / grenade.flight).max(1.0);
            *grenade = Grenade {
                team: side,
                owner: None,
                from: center,
                to,
                age: 0.0,
                flight: (center.distance(to) / speed).max(0.3),
                damage: grenade.damage * REFLECT_DAMAGE,
                knockback: grenade.knockback * REFLECT_DAMAGE,
                color,
                ..*grenade
            };
            continue;
        }

        blast(&mut targets, &shields, center, grenade.damage, grenade.radius, grenade.knockback, thrower, grenade.owner, &mut crew_hits);
        noise.0.push(center);
        fx(&mut commands, FxKind::Explosion, center, 0.0, grenade.color);
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::type_complexity)]
fn move_novas(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut noise: ResMut<Noise>,
    mut orbs: Query<(Entity, &mut NovaOrb, &mut Transform)>,
    mut targets: Query<Blastable, Without<NovaOrb>>,
    riot_shields: Query<(&RiotShield, &Transform), (Without<NovaOrb>, Without<Hurtbox>)>,
    mut crew_hits: MessageWriter<CrewHit>,
) {
    const MAX_STEP: f32 = 3.0;
    let dt = time.delta_secs();
    let shields = shield_outlines(&riot_shields);
    // They churn in the view (see `Spin`), not here.
    'orbs: for (entity, mut orb, mut transform) in &mut orbs {
        let side = orb.team;

        // Lashes the nearest of the other side in reach that it can see; the
        // crew are safe behind a riot shield.
        orb.arc_timer -= dt;
        if orb.arc_timer <= 0.0 {
            orb.arc_timer = NOVA_ARC_GAP;
            let pos = transform.translation.truncate();
            let mut near: Vec<(f32, Vec2)> = targets
                .iter()
                .filter(|(team, _, t, health, ..)| {
                    let at = t.translation.truncate();
                    **team != side
                        && !health.is_dead()
                        && at.distance(pos) <= orb.arc_reach
                        && grid.line_of_sight(pos, at)
                        && (**team != Team::Player || !shield_between(&shields, pos, at))
                })
                .map(|(_, _, t, ..)| (t.translation.truncate().distance(pos), t.translation.truncate()))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (team, _, t, mut health, ..) in &mut targets {
                let at = t.translation.truncate();
                if *team == side || !near.iter().take(NOVA_ARC_TARGETS).any(|(_, n)| *n == at) {
                    continue;
                }
                let Some(dealt) = health.hurt_for(orb.arc_damage) else { continue };
                if let Some(owner) = orb.owner {
                    crew_hits.write(CrewHit { owner, dealt, killed: health.is_dead(), drains: true });
                }
                fx(&mut commands, FxKind::Zap(at), pos, 0.0, orb.color);
            }
        }

        let travel = orb.velocity * dt;
        let steps = (travel.length() / MAX_STEP).ceil().max(1.0);
        for _ in 0..steps as u32 {
            transform.translation += (travel / steps).extend(0.0);
            let pos = transform.translation.truncate();
            let touched = targets
                .iter()
                .find(|(team, hurtbox, t, health, ..)| {
                    **team != side && !health.is_dead() && t.translation.truncate().distance(pos) <= hurtbox.0 + 3.0
                })
                .map(|(team, .., reflector)| (*team, reflector.filter(|r| r.active).map(|r| r.color)));
            // A mirror sends it back at the nearest of whoever loosed it, as its own.
            if let Some((mirror, Some(color))) = touched {
                let crew = targets
                    .iter()
                    .filter(|(team, _, t, health, ..)| **team == side && !health.is_dead() && grid.line_of_sight(pos, t.translation.truncate()))
                    .map(|(_, _, t, ..)| t.translation.truncate());
                let back = -orb.velocity.normalize_or(Vec2::X);
                let dir = nearest(pos, crew).map_or(back, |at| (at - pos).normalize_or(back));
                orb.velocity = dir * orb.velocity.length();
                orb.team = mirror;
                orb.owner = None;
                orb.remaining = REFLECT_RANGE;
                orb.damage *= REFLECT_DAMAGE;
                orb.arc_damage *= REFLECT_DAMAGE;
                orb.knockback *= REFLECT_DAMAGE;
                orb.color = color;
                fx(&mut commands, FxKind::Spark, pos, 0.0, color);
                continue 'orbs;
            }
            orb.remaining -= travel.length() / steps;
            if grid.is_solid_at(pos) || hits_shield(&shields, pos) || touched.is_some() || orb.remaining <= 0.0 {
                // Bursts just short of a wall, not inside it.
                let center = pos - orb.velocity.normalize_or_zero() * 2.0;
                blast(&mut targets, &shields, center, orb.damage, orb.radius, orb.knockback, side, orb.owner, &mut crew_hits);
                noise.0.push(center);
                fx(&mut commands, FxKind::Explosion, center, 0.0, orb.color);
                commands.entity(entity).despawn();
                continue 'orbs;
            }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::GamePlugins;
    use crate::net::{NetMode, StartSession};
    use crate::player::Player;
    use crate::room::NextSeed;

    #[test]
    fn untouched_health_is_not_resent() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(GamePlugins)
            .insert_resource(NextSeed(3))
            .insert_resource(Rng::new(3));
        app.world_mut().trigger(StartSession(NetMode::Solo));
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Playing);
        // Replication resends whatever was marked changed since it last ran.
        let mut guards = app.world_mut().query_filtered::<(Entity, Ref<Health>), Without<Player>>();
        let before: Vec<_> = guards.iter(app.world()).map(|(e, h)| (e, h.last_changed())).collect();
        assert!(!before.is_empty(), "guards on the deck");
        for _ in 0..30 {
            app.update();
        }
        for (entity, tick) in before {
            let health = app.world().entity(entity).get_ref::<Health>().expect("still there");
            assert_eq!(health.last_changed(), tick, "an unhurt guard's health was marked changed");
        }
    }

    #[test]
    fn shields_block_lines_through_them() {
        // A shield held facing +x, its curve running up and down through x = 10.
        let shield = RiotShield { owner: 0, left: 1.0, facing: Vec2::X };
        let shields = [shield.outline(Vec2::new(10.0, 0.0))];
        assert!(shield_between(&shields, Vec2::new(40.0, 0.0), Vec2::ZERO), "straight through");
        assert!(shield_between(&shields, Vec2::new(40.0, 5.0), Vec2::new(-5.0, -5.0)), "slanting through");
        assert!(!shield_between(&shields, Vec2::new(40.0, 0.0), Vec2::new(20.0, 0.0)), "both in front");
        assert!(!shield_between(&shields, Vec2::new(40.0, 40.0), Vec2::new(0.0, 40.0)), "past its edge");
        assert!(!shield_between(&[], Vec2::new(40.0, 0.0), Vec2::ZERO), "no shields");
        // A blast by the shield's edge reaches round it, but still lands in front of its holder.
        let edge = Vec2::new(4.0, 16.0);
        assert!(!shield_between(&shields, edge, Vec2::ZERO));
        assert!(shield_covers(Vec2::X, Vec2::ZERO, edge), "in front, off to the side");
        assert!(!shield_covers(Vec2::X, Vec2::ZERO, Vec2::new(-10.0, 3.0)), "behind them");
    }
}
