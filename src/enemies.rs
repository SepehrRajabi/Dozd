use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::class::Sentry;
use crate::combat::{FxKind, Health, Hurtbox, Knockback, Noise, Projectile, ProjectileLook, Team, fx, spawn_projectile};
use crate::drops::{EnemyDrop, spawn_drops};
use crate::mission::Alarm;
use crate::net::authority;
use crate::perks::{PerkKind, Perks};
use crate::player::{PirateStatus, Player};
use crate::room::{FlowField, RoomGrid};
use crate::{GameState, Level, Rng};

/// How far a gunshot carries.
const HEARING: f32 = 150.0;
/// An alerted enemy wakes idle ones within this radius.
const ALERT_SHARE: f32 = 90.0;
const SEPARATION: f32 = 12.0;
/// A brute starts winding up a punch when its target is this close.
const BRUTE_REACH: f32 = 16.0;
/// The punch still lands if the target hasn't backed off past this.
const BRUTE_SWING: f32 = 22.0;
const BRUTE_DAMAGE: f32 = 20.0;
const STALKER_DAMAGE: f32 = 25.0;
/// Odds a downed enemy leaves its bounty behind.
const CREDIT_DROP_CHANCE: f32 = 0.6;
/// Odds of an ammo box, rolled once per box (Wardens and Brutes carry two).
const AMMO_DROP_CHANCE: f32 = 0.4;

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (think, move_enemies, enemy_deaths)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(authority),
        );
    }
}

pub struct EnemiesViewPlugin;

impl Plugin for EnemiesViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_enemy)
            .add_systems(Update, (enemy_visuals, update_health_bars));
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind {
    /// Hovers at mid range, strafes, fires 3-round bursts.
    SentryDrone,
    /// Fast melee alien: winds up, then lunges.
    Stalker,
    /// Slow armoured tank that fires wide spreads.
    Warden,
    /// Hulking melee bruiser: plods after you and punches when it gets close.
    Brute,
}

struct EnemyStats {
    sprite: &'static str,
    health: f32,
    speed: f32,
    hurtbox: f32,
    /// Collision half-extent against walls.
    half: f32,
    sight: f32,
    /// Fraction of incoming knockback actually applied.
    knockback_taken: f32,
    death_color: Color,
    /// Credits dropped on death, rolled in `lo..=hi`. Always below a Credit Chip.
    bounty: (u32, u32),
}

const SENTRY_DRONE: EnemyStats = EnemyStats {
    sprite: "sprites/enemies/sentry_drone.png",
    health: 35.0,
    speed: 55.0,
    hurtbox: 6.0,
    half: 4.0,
    sight: 120.0,
    knockback_taken: 1.0,
    death_color: Color::srgb(1.0, 0.35, 0.3),
    bounty: (15, 30),
};

const STALKER: EnemyStats = EnemyStats {
    sprite: "sprites/enemies/stalker.png",
    health: 70.0,
    speed: 62.0,
    hurtbox: 6.5,
    half: 4.0,
    sight: 100.0,
    knockback_taken: 0.6,
    death_color: Color::srgb(0.55, 1.0, 0.5),
    bounty: (25, 45),
};

const WARDEN: EnemyStats = EnemyStats {
    sprite: "sprites/enemies/warden.png",
    health: 200.0,
    speed: 26.0,
    hurtbox: 7.5,
    half: 5.0,
    sight: 130.0,
    knockback_taken: 0.1,
    death_color: Color::srgb(1.0, 0.7, 0.3),
    bounty: (50, 80),
};

const BRUTE: EnemyStats = EnemyStats {
    sprite: "sprites/enemies/brute.png",
    health: 300.0,
    speed: 20.0,
    hurtbox: 8.0,
    half: 5.5,
    sight: 100.0,
    knockback_taken: 0.05,
    death_color: Color::srgb(0.85, 0.45, 0.35),
    bounty: (40, 70),
};

impl EnemyKind {
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'd' => Self::SentryDrone,
            's' => Self::Stalker,
            'h' => Self::Warden,
            'b' => Self::Brute,
            _ => return None,
        })
    }

    fn stats(self) -> &'static EnemyStats {
        match self {
            Self::SentryDrone => &SENTRY_DRONE,
            Self::Stalker => &STALKER,
            Self::Warden => &WARDEN,
            Self::Brute => &BRUTE,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiState {
    /// Unaware; loiters near its post.
    Idle,
    Hunt,
    /// Visible wind-up before an attack, so the player can react.
    Telegraph,
    Lunge,
    /// Vulnerable pause after a lunge.
    Recover,
}

/// What clients need to draw an enemy. The host keeps it in sync with the brain.
#[derive(Component, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct EnemyLook {
    pub kind: EnemyKind,
    pub state: AiState,
    pub stunned: bool,
    /// Fighting for the crew.
    pub hacked: bool,
}

/// Host-only AI state.
#[derive(Component)]
pub struct Enemy {
    kind: EnemyKind,
    state: AiState,
    home: Vec2,
    /// Counts down the current state (telegraph, lunge, recover) or burst spacing.
    timer: f32,
    attack_cooldown: f32,
    burst_left: u32,
    lunge_dir: Vec2,
    lunge_hit: bool,
    strafe_sign: f32,
    wander: Option<Vec2>,
    velocity: Vec2,
    /// Seconds left frozen by a shock field.
    stun: f32,
    /// Seconds left fighting for the crew after a Hacker turned it.
    hacked: f32,
}

impl Enemy {
    /// Freezes the enemy for at least `secs`, cancelling whatever it was winding up.
    pub fn stun(&mut self, secs: f32) {
        self.stun = self.stun.max(secs);
        self.state = AiState::Hunt;
        self.burst_left = 0;
        self.velocity = Vec2::ZERO;
    }

    pub fn is_hacked(&self) -> bool {
        self.hacked > 0.0
    }

    /// Turns the enemy against its own side for `secs`.
    pub fn hack(&mut self, secs: f32) {
        self.hacked = secs;
        self.state = AiState::Hunt;
        self.burst_left = 0;
        self.attack_cooldown = self.attack_cooldown.min(0.5);
    }
}

#[derive(Component)]
struct HealthBar {
    fill: bool,
}

pub fn spawn_enemy(commands: &mut Commands, kind: EnemyKind, pos: Vec2, alerted: bool) {
    let stats = kind.stats();
    let state = if alerted { AiState::Hunt } else { AiState::Idle };
    commands.spawn((
        Level,
        Replicated,
        EnemyLook { kind, state, stunned: false, hacked: false },
        Enemy {
            kind,
            state,
            home: pos,
            timer: 0.0,
            attack_cooldown: 1.0,
            burst_left: 0,
            lunge_dir: Vec2::ZERO,
            lunge_hit: false,
            strafe_sign: if (pos.x as i32) % 2 == 0 { 1.0 } else { -1.0 },
            wander: None,
            velocity: Vec2::ZERO,
            stun: 0.0,
            hacked: 0.0,
        },
        Team::Enemy,
        Health::new(stats.health, 0.0),
        Hurtbox(stats.hurtbox),
        Knockback::default(),
        Transform::from_translation(pos.extend(8.0)),
    ));
}

fn dress_enemy(add: On<Add, EnemyLook>, mut commands: Commands, assets: Res<AssetServer>, looks: Query<&EnemyLook>) {
    let Ok(look) = looks.get(add.entity) else { return };
    commands
        .entity(add.entity)
        .insert(Sprite::from_image(assets.load(look.kind.stats().sprite)))
        .with_children(|parent| {
            parent.spawn((
                Sprite::from_image(assets.load("sprites/shadow.png")),
                Transform::from_xyz(0.0, 0.0, -0.1),
            ));
            parent.spawn((
                HealthBar { fill: false },
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.75), Vec2::new(14.0, 3.0)),
                Transform::from_xyz(0.0, 11.0, 1.0),
                Visibility::Hidden,
            ));
            parent.spawn((
                HealthBar { fill: true },
                Sprite::from_color(Color::srgb(1.0, 0.3, 0.3), Vec2::new(12.0, 1.0)),
                Transform::from_xyz(0.0, 11.0, 1.1),
                Visibility::Hidden,
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn enemy_shot(
    commands: &mut Commands,
    team: Team,
    pos: Vec2,
    angle: f32,
    speed: f32,
    damage: f32,
    look: ProjectileLook,
    color: Color,
) {
    spawn_projectile(
        commands,
        Projectile {
            team,
            velocity: Vec2::from_angle(angle) * speed,
            remaining: 260.0,
            damage,
            knockback: 0.0,
            pierce: false,
            color,
            hits: Vec::new(),
        },
        look,
        pos,
    );
}

fn think(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    field: Res<FlowField>,
    mut alarm: ResMut<Alarm>,
    mut noise: ResMut<Noise>,
    mut rng: ResMut<Rng>,
    mut victims: Query<
        (Entity, &Transform, &mut Health, Option<&Player>, Option<&Perks>),
        (Without<Enemy>, Or<(With<Player>, With<Sentry>)>),
    >,
    mut enemies: Query<(Entity, &mut Enemy, &Transform, &mut Health, &mut Team)>,
) {
    let dt = time.delta_secs();
    // Hacks wear off, and a turned enemy's shots and wounds follow its side.
    for (_, mut enemy, _, _, mut team) in &mut enemies {
        if enemy.hacked > 0.0 {
            enemy.hacked = (enemy.hacked - dt).max(0.0);
        }
        let side = if enemy.hacked > 0.0 { Team::Player } else { Team::Enemy };
        if *team != side {
            *team = side;
        }
    }

    // The crew: pirates, their sentries and hacked enemies. Cloaked pirates
    // simply don't exist as far as the AI is concerned.
    let mut crew: Vec<(Entity, Vec2)> = victims
        .iter()
        .filter(|(_, _, health, player, perks)| {
            !health.is_dead()
                && player.is_none_or(|p| p.status == PirateStatus::Active)
                && perks.is_none_or(|p| !p.has(PerkKind::Cloak))
        })
        .map(|(e, t, ..)| (e, t.translation.truncate()))
        .collect();
    // Whoever a hacked enemy goes after: the station's own.
    let mut station = Vec::new();
    for (entity, enemy, transform, health, _) in &enemies {
        if !health.is_dead() {
            let side = if enemy.hacked > 0.0 { &mut crew } else { &mut station };
            side.push((entity, transform.translation.truncate()));
        }
    }
    let heard = std::mem::take(&mut noise.0);

    // Waking up: sight, gunfire, getting shot, or a station-wide lockdown.
    let mut woken = Vec::new();
    for (_, mut enemy, transform, health, _) in &mut enemies {
        if enemy.state != AiState::Idle {
            continue;
        }
        let pos = transform.translation.truncate();
        let sight = enemy.kind.stats().sight;
        let sees = crew
            .iter()
            .any(|(_, t)| pos.distance(*t) < sight && grid.line_of_sight(pos, *t));
        let hears = heard.iter().any(|n| n.distance(pos) < HEARING);
        if sees || hears || health.current < health.max || alarm.lockdown() {
            enemy.state = AiState::Hunt;
            woken.push(pos);
            if sees {
                alarm.alert();
            }
        }
    }
    if !woken.is_empty() {
        for (_, mut enemy, transform, ..) in &mut enemies {
            let pos = transform.translation.truncate();
            if enemy.state == AiState::Idle && woken.iter().any(|w| w.distance(pos) < ALERT_SHARE) {
                enemy.state = AiState::Hunt;
            }
        }
    }

    // Lunges landing on other enemies, applied once the loop lets go of them.
    let mut bites = Vec::new();
    for (entity, mut enemy, transform, _, team) in &mut enemies {
        let stats = enemy.kind.stats();
        let pos = transform.translation.truncate();
        if enemy.stun > 0.0 {
            enemy.stun = (enemy.stun - dt).max(0.0);
            enemy.velocity = Vec2::ZERO;
            continue;
        }
        enemy.timer -= dt;
        enemy.attack_cooldown -= dt;

        let hacked = enemy.hacked > 0.0;
        let foes = if hacked { &station } else { &crew };
        let nearest = |list: &[(Entity, Vec2)]| {
            list.iter()
                .filter(|(e, _)| *e != entity)
                .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
                .copied()
        };
        let Some((target_entity, target)) = nearest(foes) else {
            enemy.velocity = match nearest(&crew) {
                // A hacked enemy with nothing to fight tags along with the crew.
                Some((_, leader)) if hacked && leader.distance(pos) > 40.0 => {
                    let dir = (leader - pos).normalize_or_zero();
                    let chase = if grid.line_of_sight(pos, leader) { dir } else { field.step(&grid, pos).unwrap_or(Vec2::ZERO) };
                    chase * stats.speed
                }
                // Nobody left to hunt.
                _ => Vec2::ZERO,
            };
            continue;
        };
        let side = *team;
        let to_target = target - pos;
        let dist = to_target.length();
        let dir = to_target.normalize_or(Vec2::X);
        let los = grid.line_of_sight(pos, target);
        let chase = if los { dir } else { field.step(&grid, pos).unwrap_or(Vec2::ZERO) };

        match (enemy.kind, enemy.state) {
            (_, AiState::Idle) => {
                if enemy.timer <= 0.0 {
                    let spot = enemy.home + Vec2::new(rng.signed(), rng.signed()) * 20.0;
                    enemy.wander = (!grid.is_solid_at(spot)).then_some(spot);
                    enemy.timer = 1.5 + rng.unit() * 2.0;
                }
                enemy.velocity = match enemy.wander {
                    Some(spot) if spot.distance(pos) > 2.0 => (spot - pos).normalize() * stats.speed * 0.3,
                    _ => Vec2::ZERO,
                };
            }

            (EnemyKind::SentryDrone, AiState::Hunt) => {
                if rng.unit() < dt * 0.5 {
                    enemy.strafe_sign = -enemy.strafe_sign;
                }
                enemy.velocity = if !los || dist > 120.0 {
                    chase * stats.speed
                } else if dist < 60.0 {
                    -dir * stats.speed
                } else {
                    let strafe = dir.perp() * enemy.strafe_sign;
                    (strafe + dir * (dist - 90.0) / 60.0).normalize_or_zero() * stats.speed * 0.7
                };

                if enemy.burst_left > 0 && enemy.timer <= 0.0 {
                    let angle = dir.to_angle() + rng.signed() * 0.08;
                    let color = Color::srgb(1.0, 0.3, 0.25);
                    enemy_shot(&mut commands, side, pos + dir * 6.0, angle, 150.0, 8.0, ProjectileLook::EnemyBolt, color);
                    enemy.burst_left -= 1;
                    enemy.timer = 0.1;
                } else if enemy.burst_left == 0 && enemy.attack_cooldown <= 0.0 && los && dist < 150.0 {
                    enemy.state = AiState::Telegraph;
                    enemy.timer = 0.35;
                }
            }
            (EnemyKind::SentryDrone, AiState::Telegraph) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    enemy.state = AiState::Hunt;
                    enemy.burst_left = 3;
                    enemy.attack_cooldown = 1.7 + rng.unit() * 0.6;
                }
            }

            (EnemyKind::Stalker, AiState::Hunt) => {
                enemy.velocity = chase * stats.speed;
                if los && dist < 44.0 && enemy.attack_cooldown <= 0.0 {
                    enemy.state = AiState::Telegraph;
                    enemy.timer = 0.45;
                    // Committed at wind-up start, so sidestepping dodges it.
                    enemy.lunge_dir = dir;
                }
            }
            (EnemyKind::Stalker, AiState::Telegraph) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    enemy.state = AiState::Lunge;
                    enemy.timer = 0.22;
                    enemy.lunge_hit = false;
                }
            }
            (EnemyKind::Stalker, AiState::Lunge) => {
                enemy.velocity = enemy.lunge_dir * 240.0;
                if !enemy.lunge_hit && dist < 11.0 {
                    enemy.lunge_hit = match victims.get_mut(target_entity) {
                        Ok((_, _, mut health, ..)) => health.hurt(STALKER_DAMAGE),
                        Err(_) => {
                            bites.push((target_entity, STALKER_DAMAGE));
                            true
                        }
                    };
                }
                if enemy.timer <= 0.0 {
                    enemy.state = AiState::Recover;
                    enemy.timer = 0.6;
                }
            }
            (EnemyKind::Stalker, AiState::Recover) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    enemy.state = AiState::Hunt;
                    enemy.attack_cooldown = 0.3;
                }
            }

            (EnemyKind::Warden, AiState::Hunt) => {
                enemy.velocity = if los && dist < 80.0 { Vec2::ZERO } else { chase * stats.speed };
                if los && dist < 160.0 && enemy.attack_cooldown <= 0.0 {
                    enemy.state = AiState::Telegraph;
                    enemy.timer = 0.55;
                }
            }
            (EnemyKind::Warden, AiState::Telegraph) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    let color = Color::srgb(1.0, 0.55, 0.2);
                    for i in 0..5 {
                        let angle = dir.to_angle() + (i as f32 / 4.0 - 0.5) * 0.7;
                        enemy_shot(&mut commands, side, pos + dir * 8.0, angle, 115.0, 12.0, ProjectileLook::EnemyOrb, color);
                    }
                    enemy.state = AiState::Hunt;
                    enemy.attack_cooldown = 2.4 + rng.unit() * 0.6;
                }
            }

            (EnemyKind::Brute, AiState::Hunt) => {
                // Plods straight in; stops at arm's length rather than shoving.
                enemy.velocity = if los && dist < BRUTE_REACH * 0.8 { Vec2::ZERO } else { chase * stats.speed };
                if los && dist < BRUTE_REACH && enemy.attack_cooldown <= 0.0 {
                    enemy.state = AiState::Telegraph;
                    enemy.timer = 0.6;
                    enemy.lunge_dir = dir;
                }
            }
            (EnemyKind::Brute, AiState::Telegraph) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    // Lands on whoever is still in reach when the fist comes down.
                    let fist = pos + enemy.lunge_dir * 8.0;
                    if dist < BRUTE_SWING {
                        match victims.get_mut(target_entity) {
                            Ok((_, _, mut health, ..)) => {
                                health.hurt(BRUTE_DAMAGE);
                            }
                            Err(_) => bites.push((target_entity, BRUTE_DAMAGE)),
                        }
                    }
                    fx(&mut commands, FxKind::Spark, fist, enemy.lunge_dir.to_angle(), stats.death_color);
                    enemy.state = AiState::Recover;
                    enemy.timer = 0.7;
                }
            }
            (EnemyKind::Brute, AiState::Recover) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    enemy.state = AiState::Hunt;
                    enemy.attack_cooldown = 0.6 + rng.unit() * 0.4;
                }
            }

            (_, state) => {
                // States a kind never enters (e.g. a drone lunging); recover gracefully.
                debug_assert!(false, "{:?} has no {state:?} behaviour", enemy.kind);
                enemy.state = AiState::Hunt;
            }
        }
    }

    for (victim, damage) in bites {
        if let Ok((.., mut health, _)) = enemies.get_mut(victim) {
            health.hurt(damage);
        }
    }
}

fn move_enemies(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut enemies: Query<(Entity, &Enemy, &mut EnemyLook, &mut Transform, &mut Knockback)>,
) {
    let dt = time.delta_secs();
    let positions: Vec<(Entity, Vec2)> = enemies
        .iter()
        .map(|(e, _, _, t, _)| (e, t.translation.truncate()))
        .collect();

    for (entity, enemy, mut look, mut transform, mut knockback) in &mut enemies {
        look.set_if_neq(EnemyLook {
            kind: enemy.kind,
            state: enemy.state,
            stunned: enemy.stun > 0.0,
            hacked: enemy.hacked > 0.0,
        });

        let stats = enemy.kind.stats();
        let pos = transform.translation.truncate();

        // Keep the pack from stacking into a single sprite.
        let push: Vec2 = positions
            .iter()
            .filter(|(other, _)| *other != entity)
            .map(|(_, p)| pos - *p)
            .filter(|d| d.length() < SEPARATION)
            .map(|d| d.normalize_or(Vec2::X) * (SEPARATION - d.length()) * 6.0)
            .sum();

        let velocity = enemy.velocity + knockback.0 * stats.knockback_taken + push;
        knockback.0 *= (-10.0 * dt).exp();
        if velocity.length_squared() < 0.01 {
            continue;
        }

        let new = grid.slide(pos, velocity * dt, Vec2::splat(stats.half));
        transform.translation.x = new.x;
        transform.translation.y = new.y;
    }
}

fn enemy_visuals(time: Res<Time>, mut enemies: Query<(&EnemyLook, &Health, &mut Sprite)>) {
    let blink = (time.elapsed_secs() * 30.0).sin() > 0.0;
    for (look, health, mut sprite) in &mut enemies {
        sprite.color = if health.flash > 0.0 {
            Color::srgb(1.0, 0.3, 0.3)
        } else if look.hacked {
            // Glitches between the crew's green and its own colours.
            if blink { Color::srgb(0.45, 1.0, 0.55) } else { Color::srgb(0.75, 1.0, 0.8) }
        } else if look.stunned {
            // Crackles between white and electric blue.
            if blink { Color::srgb(0.55, 0.85, 1.0) } else { Color::srgb(0.85, 0.95, 1.0) }
        } else if look.state == AiState::Telegraph && blink {
            Color::srgb(1.0, 0.85, 0.3)
        } else if look.state == AiState::Recover {
            Color::srgb(0.6, 0.6, 0.75)
        } else {
            Color::WHITE
        };
    }
}

fn update_health_bars(
    health: Query<&Health>,
    mut bars: Query<(&HealthBar, &ChildOf, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    for (bar, parent, mut sprite, mut transform, mut visibility) in &mut bars {
        let Ok(h) = health.get(parent.parent()) else { continue };
        let frac = (h.current / h.max).clamp(0.0, 1.0);
        *visibility = if frac < 1.0 { Visibility::Inherited } else { Visibility::Hidden };
        if bar.fill {
            sprite.custom_size = Some(Vec2::new(12.0 * frac, 1.0));
            transform.translation.x = -6.0 * (1.0 - frac);
        }
    }
}

fn enemy_deaths(mut commands: Commands, mut rng: ResMut<Rng>, enemies: Query<(Entity, &Enemy, &Health, &Transform)>) {
    for (entity, enemy, health, transform) in &enemies {
        if health.is_dead() {
            let stats = enemy.kind.stats();
            let pos = transform.translation.truncate();
            fx(&mut commands, FxKind::Burst, pos, 0.0, stats.death_color);

            let (lo, hi) = stats.bounty;
            let credits = lo + (rng.unit() * (hi - lo) as f32).round() as u32;
            // Each drop is its own roll, so a body may leave nothing at all.
            let ammo_rolls = if matches!(enemy.kind, EnemyKind::Warden | EnemyKind::Brute) { 2 } else { 1 };
            let mut drops = Vec::new();
            if rng.unit() < CREDIT_DROP_CHANCE {
                drops.push(EnemyDrop::Credits(credits));
            }
            for _ in 0..ammo_rolls {
                if rng.unit() < AMMO_DROP_CHANCE {
                    drops.push(EnemyDrop::Ammo);
                }
            }
            spawn_drops(&mut commands, &mut rng, pos, &drops);
            commands.entity(entity).despawn();
        }
    }
}

/// Picks a reinforcement type; heavier units become likelier as the alarm climbs.
pub fn roll_reinforcement(rng: &mut Rng, alarm_level: f32) -> EnemyKind {
    let r = rng.unit();
    let warden = 0.08 * alarm_level;
    let brute = 0.06 * alarm_level;
    if r < warden {
        EnemyKind::Warden
    } else if r < warden + brute {
        EnemyKind::Brute
    } else if r < warden + brute + 0.4 {
        EnemyKind::Stalker
    } else {
        EnemyKind::SentryDrone
    }
}
