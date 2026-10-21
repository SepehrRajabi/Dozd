use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::abilities::Passive;
use crate::boss::{Keeper, KeeperJam};
use crate::class::{Boons, HACK_COLOR, JAM_MISS, JAM_RADIUS, Sentry};
use crate::combat::{FxKind, Health, Hurtbox, Knockback, Noise, Projectile, ProjectileLook, Team, fx, spawn_projectile};
use crate::drops::{EnemyDrop, pick_ammo, spawn_drops};
use crate::guns::Arsenal;
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
/// A Herald's rifle shot.
const HERALD_DAMAGE: f32 = 10.0;
/// A hacked guard fights only what comes this close to the pirate nearest it,
/// heads back once it's `LEASH` from them, and with nothing to fight keeps
/// within `FOLLOW` of them.
const GUARD_REACH: f32 = 130.0;
const LEASH: f32 = 110.0;
const FOLLOW: f32 = 36.0;

/// How long a Herald stands rallying before its phase kicks in. Stunning or
/// killing it in this window stops it.
const HERALD_RALLY_TIME: f32 = 1.0;
/// Guards this close hear a Herald's war cry; and it needs this many of them
/// nearby to cry out, or it goes into a frenzy instead.
const HERALD_REACH: f32 = 90.0;
const HERALD_PACK: usize = 2;
/// A war cry's lasting damage multiplier for everyone it reaches: guards, or,
/// from a hacked Herald, the crew.
pub const WAR_CRY_POWER: f32 = 1.6;
pub const EMPOWERED_COLOR: Color = Color::srgb(1.0, 0.45, 0.2);
/// A frenzied Herald's speed and damage multipliers.
const FRENZY_SPEED: f32 = 1.9;
const FRENZY_POWER: f32 = 1.35;
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
            .add_systems(Update, (enemy_visuals, update_health_bars, draw_power_rings));
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
    /// Station officer with a rifle. Meeting the crew, it rallies: a war cry
    /// that empowers the guards around it for good, or, alone, a frenzy.
    Herald,
}

/// Which way a Herald's rally went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HeraldPhase {
    /// Its war cry empowered the guards around it.
    WarCry,
    /// Nobody to rally: it hunts faster and hits harder itself.
    Frenzy,
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

const HERALD: EnemyStats = EnemyStats {
    sprite: "sprites/enemies/herald.png",
    health: 90.0,
    speed: 40.0,
    hurtbox: 6.5,
    half: 4.0,
    sight: 120.0,
    knockback_taken: 0.5,
    death_color: Color::srgb(0.9, 0.3, 0.55),
    bounty: (35, 60),
};

impl EnemyKind {
    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'd' => Self::SentryDrone,
            's' => Self::Stalker,
            'h' => Self::Warden,
            'b' => Self::Brute,
            'g' => Self::Herald,
            _ => return None,
        })
    }

    fn stats(self) -> &'static EnemyStats {
        match self {
            Self::SentryDrone => &SENTRY_DRONE,
            Self::Stalker => &STALKER,
            Self::Warden => &WARDEN,
            Self::Brute => &BRUTE,
            Self::Herald => &HERALD,
        }
    }

    pub const ALL: [EnemyKind; 5] = [Self::SentryDrone, Self::Stalker, Self::Warden, Self::Brute, Self::Herald];

    pub fn name(self) -> &'static str {
        match self {
            Self::SentryDrone => "SENTRY DRONE",
            Self::Stalker => "STALKER",
            Self::Warden => "WARDEN",
            Self::Brute => "BRUTE",
            Self::Herald => "HERALD",
        }
    }

    /// How it fights, in a word, for the enemies screen.
    pub fn role(self) -> &'static str {
        match self {
            Self::SentryDrone => "RANGED",
            Self::Stalker | Self::Brute => "MELEE",
            Self::Warden => "HEAVY",
            Self::Herald => "SUPPORT",
        }
    }

    /// What to expect from it, for the enemies screen.
    pub fn details(self) -> &'static str {
        match self {
            Self::SentryDrone => "Hovers at mid range, strafing, and fires three-round bursts.",
            Self::Stalker => "Fast alien. Winds up, then lunges: sidestep it.",
            Self::Warden => "Slow armoured tank. Holds its ground and fires wide spreads.",
            Self::Brute => "Hulking bruiser. Plods after you and punches hard up close.",
            Self::Herald => "Rallies on sight: a war cry that empowers guards nearby for good, or alone, a frenzy. Stun it to stop it.",
        }
    }

    pub fn sprite(self) -> &'static str {
        self.stats().sprite
    }

    pub fn health(self) -> f32 {
        self.stats().health
    }

    /// Its colour when it goes down, to tint its card.
    pub fn color(self) -> Color {
        self.stats().death_color
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
    /// Hits harder for good, after a Herald's war cry.
    pub empowered: bool,
    /// A Herald in a frenzy.
    pub frenzied: bool,
    /// A Hacker nearby is jamming it.
    pub jammed: bool,
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
    /// A Hacker turned it: it fights for the crew for good.
    hacked: bool,
    /// Heard a Herald's war cry: its attacks hit `WAR_CRY_POWER` harder for good.
    empowered: bool,
    /// A Herald's rally, once it has rallied.
    phase: Option<HeraldPhase>,
    /// A Hacker nearby is jamming it: its attacks may miss.
    jammed: bool,
}

impl Enemy {
    /// Freezes the enemy for at least `secs`, cancelling whatever it was winding up.
    pub fn stun(&mut self, secs: f32) {
        self.stun = self.stun.max(secs);
        self.state = AiState::Hunt;
        self.burst_left = 0;
        self.velocity = Vec2::ZERO;
    }

    /// What its attacks' damage is multiplied by.
    fn power(&self) -> f32 {
        let cry = if self.empowered { WAR_CRY_POWER } else { 1.0 };
        let frenzy = if self.phase == Some(HeraldPhase::Frenzy) { FRENZY_POWER } else { 1.0 };
        cry * frenzy
    }

    fn speed(&self) -> f32 {
        let frenzy = if self.phase == Some(HeraldPhase::Frenzy) { FRENZY_SPEED } else { 1.0 };
        self.kind.stats().speed * frenzy
    }

    fn look(&self) -> EnemyLook {
        EnemyLook {
            kind: self.kind,
            state: self.state,
            stunned: self.stun > 0.0,
            hacked: self.hacked,
            empowered: self.empowered,
            frenzied: self.phase == Some(HeraldPhase::Frenzy),
            jammed: self.jammed,
        }
    }

    pub fn is_hacked(&self) -> bool {
        self.hacked
    }

    /// Turns the enemy against its own side, for good.
    pub fn hack(&mut self) {
        self.hacked = true;
        self.state = AiState::Hunt;
        self.burst_left = 0;
        self.attack_cooldown = self.attack_cooldown.min(0.5);
    }
}

/// Host only: called in by a boss (see the Vault Brute in `boss.rs`). Fights
/// like any other guard, but drops only ammo, never credits, so the fight
/// can't be farmed for loot, and goes down with the boss.
#[derive(Component)]
pub struct Summoned;

#[derive(Component)]
struct HealthBar {
    fill: bool,
}

/// Glows at an empowered guard's feet.
#[derive(Component)]
struct PowerRing;

pub fn spawn_enemy(commands: &mut Commands, kind: EnemyKind, pos: Vec2, alerted: bool) -> Entity {
    spawn_guard(commands, kind, pos, alerted, false)
}

/// Spawns a guard a boss called in: hunting at once, enraged as if it had
/// heard a Herald's war cry, and `Summoned`.
pub fn spawn_summoned(commands: &mut Commands, kind: EnemyKind, pos: Vec2) -> Entity {
    let guard = spawn_guard(commands, kind, pos, true, true);
    commands.entity(guard).insert(Summoned);
    guard
}

fn spawn_guard(commands: &mut Commands, kind: EnemyKind, pos: Vec2, alerted: bool, empowered: bool) -> Entity {
    let stats = kind.stats();
    let state = if alerted { AiState::Hunt } else { AiState::Idle };
    let enemy = Enemy {
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
            hacked: false,
            empowered,
            phase: None,
            jammed: false,
        };
    commands.spawn((
        Level,
        Replicated,
        enemy.look(),
        enemy,
        Team::Enemy,
        Health::new(stats.health, 0.0),
        Hurtbox(stats.hurtbox),
        Knockback::default(),
        Transform::from_translation(pos.extend(8.0)),
    ))
    .id()
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
                PowerRing,
                Sprite { image: assets.load("sprites/fx/shock_ring.png"), color: EMPOWERED_COLOR, ..default() },
                Transform::from_xyz(0.0, -4.0, -0.05).with_scale(Vec3::new(24.0 / 113.0, 14.0 / 113.0, 1.0)),
                Visibility::Hidden,
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
            owner: None,
            velocity: Vec2::from_angle(angle) * speed,
            remaining: 260.0,
            damage,
            knockback: 0.0,
            pierce: false,
            color,
            hits: Vec::new(),
                ricochet: 0.0,
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
        (Entity, &Transform, &mut Health, Option<&Player>, Option<&Perks>, Option<&mut Boons>),
        (Without<Enemy>, Or<(With<Player>, With<Sentry>)>),
    >,
    mut enemies: Query<(Entity, &mut Enemy, &Transform, &mut Health, &mut Team)>,
    mut keepers: Query<(Entity, &Keeper, &Transform, &mut KeeperJam, &mut Health), (Without<Enemy>, Without<Player>, Without<Sentry>)>,
) {
    let dt = time.delta_secs();
    // A turned enemy's shots and wounds follow its side.
    for (_, enemy, _, _, mut team) in &mut enemies {
        let side = if enemy.hacked { Team::Player } else { Team::Enemy };
        if *team != side {
            *team = side;
        }
    }

    // The crew: pirates, their sentries and hacked enemies. Cloaked pirates
    // simply don't exist as far as the AI is concerned.
    let mut crew: Vec<(Entity, Vec2)> = victims
        .iter()
        .filter(|(_, _, health, player, perks, _)| {
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
            let side = if enemy.hacked { &mut crew } else { &mut station };
            side.push((entity, transform.translation.truncate()));
        }
    }
    // An awake vault boss is fair game for hacked guards too.
    for (entity, keeper, transform, _, health) in &keepers {
        if keeper.awake && !health.is_dead() {
            station.push((entity, transform.translation.truncate()));
        }
    }
    // The pirates hacked guards stay close to and guard.
    let pirates: Vec<Vec2> = victims
        .iter()
        .filter(|(_, _, health, player, ..)| !health.is_dead() && player.is_some_and(|p| p.status == PirateStatus::Active))
        .map(|(_, t, ..)| t.translation.truncate())
        .collect();
    let heard = std::mem::take(&mut noise.0);

    // Hackers jam the station's guards around them, and count them.
    let jammers: Vec<(Entity, Vec2)> = victims
        .iter()
        .filter(|(_, _, health, player, ..)| {
            !health.is_dead() && player.is_some_and(|p| p.passive == Passive::Jamming && p.status == PirateStatus::Active)
        })
        .map(|(e, t, ..)| (e, t.translation.truncate()))
        .collect();
    let mut jamming = vec![0u8; jammers.len()];
    for (_, mut enemy, transform, health, _) in &mut enemies {
        let pos = transform.translation.truncate();
        let mut jammed = false;
        if !enemy.hacked && !health.is_dead() {
            for (i, (_, at)) in jammers.iter().enumerate() {
                if at.distance(pos) <= JAM_RADIUS {
                    jamming[i] = jamming[i].saturating_add(1);
                    jammed = true;
                }
            }
        }
        if enemy.jammed != jammed {
            enemy.jammed = jammed;
        }
    }
    // Vault bosses too, once they're awake.
    for (_, keeper, transform, mut jam, _) in &mut keepers {
        let pos = transform.translation.truncate();
        let mut jammed = false;
        if keeper.awake {
            for (i, (_, at)) in jammers.iter().enumerate() {
                if at.distance(pos) <= JAM_RADIUS {
                    jamming[i] = jamming[i].saturating_add(1);
                    jammed = true;
                }
            }
        }
        if jam.0 != jammed {
            jam.0 = jammed;
        }
    }
    for ((hacker, _), count) in jammers.iter().zip(jamming) {
        if let Ok((.., Some(mut boons))) = victims.get_mut(*hacker)
            && boons.jamming != count
        {
            boons.jamming = count;
        }
    }

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
    // Where Heralds cried out this frame, and whether for the crew.
    let mut cries: Vec<(Vec2, bool)> = Vec::new();
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
        let power = enemy.power();
        let jammed = enemy.jammed;

        let hacked = enemy.hacked;
        // A hacked guard sticks with the pirate nearest it: it only fights
        // what comes within `GUARD_REACH` of them, and never strays past `LEASH`.
        let leader = pirates.iter().copied().min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)));
        let strayed = hacked && leader.is_some_and(|l| l.distance(pos) > LEASH);
        let nearest = |list: &[(Entity, Vec2)]| {
            list.iter()
                .filter(|(e, at)| *e != entity && (!hacked || leader.is_none_or(|l| l.distance(*at) <= GUARD_REACH)))
                .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
                .copied()
        };
        let foes = if hacked { &station } else { &crew };
        let Some((target_entity, target)) = nearest(foes).filter(|_| !strayed) else {
            enemy.velocity = match leader.filter(|_| hacked) {
                // A hacked enemy with nothing near to fight tags along with the crew.
                Some(leader) if leader.distance(pos) > FOLLOW => {
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
                    let aim = dir.to_angle() + rng.signed() * 0.08;
                    let angle = jam_aim(&mut rng, jammed, aim);
                    let color = Color::srgb(1.0, 0.3, 0.25);
                    enemy_shot(&mut commands, side, pos + dir * 6.0, angle, 150.0, 8.0 * power, ProjectileLook::EnemyBolt, color);
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
                if !enemy.lunge_hit && dist < 11.0 && whiffs(&mut rng, jammed) {
                    enemy.lunge_hit = true;
                    fx(&mut commands, FxKind::Spark, pos, enemy.lunge_dir.to_angle(), HACK_COLOR);
                } else if !enemy.lunge_hit && dist < 11.0 {
                    enemy.lunge_hit = match victims.get_mut(target_entity) {
                        Ok((_, _, mut health, ..)) => health.hurt(STALKER_DAMAGE * power),
                        Err(_) => {
                            bites.push((target_entity, STALKER_DAMAGE * power));
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
                        let angle = jam_aim(&mut rng, jammed, dir.to_angle() + (i as f32 / 4.0 - 0.5) * 0.7);
                        enemy_shot(&mut commands, side, pos + dir * 8.0, angle, 115.0, 12.0 * power, ProjectileLook::EnemyOrb, color);
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
                    let whiffed = dist < BRUTE_SWING && whiffs(&mut rng, jammed);
                    if dist < BRUTE_SWING && !whiffed {
                        match victims.get_mut(target_entity) {
                            Ok((_, _, mut health, ..)) => {
                                health.hurt(BRUTE_DAMAGE * power);
                            }
                            Err(_) => bites.push((target_entity, BRUTE_DAMAGE * power)),
                        }
                    }
                    let color = if whiffed { HACK_COLOR } else { stats.death_color };
                    fx(&mut commands, FxKind::Spark, fist, enemy.lunge_dir.to_angle(), color);
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

            (EnemyKind::Herald, AiState::Hunt) => {
                let frenzied = enemy.phase == Some(HeraldPhase::Frenzy);
                // Keeps a rifle's distance; frenzied, it closes right in.
                let keep = if frenzied { 30.0 } else { 80.0 };
                enemy.velocity = if !los || dist > keep + 20.0 {
                    chase * enemy.speed()
                } else if dist < keep - 20.0 {
                    -dir * enemy.speed() * 0.6
                } else {
                    Vec2::ZERO
                };
                // A hacked Herald rallies too, for whichever side it's on.
                if enemy.phase.is_none() && los && dist < stats.sight {
                    enemy.state = AiState::Telegraph;
                    enemy.timer = HERALD_RALLY_TIME;
                } else if los && dist < 170.0 && enemy.attack_cooldown <= 0.0 {
                    let aim = dir.to_angle() + rng.signed() * 0.06;
                    let angle = jam_aim(&mut rng, jammed, aim);
                    let damage = HERALD_DAMAGE * power;
                    enemy_shot(&mut commands, side, pos + dir * 6.0, angle, 170.0, damage, ProjectileLook::EnemyBolt, stats.death_color);
                    enemy.attack_cooldown = if frenzied { 0.6 } else { 1.1 } + rng.unit() * 0.3;
                }
            }
            (EnemyKind::Herald, AiState::Telegraph) => {
                enemy.velocity = Vec2::ZERO;
                if enemy.timer <= 0.0 {
                    let allies = if hacked { &crew } else { &station };
                    let pack = allies
                        .iter()
                        .filter(|(e, at)| *e != entity && at.distance(pos) <= HERALD_REACH)
                        .count();
                    if pack >= HERALD_PACK {
                        enemy.phase = Some(HeraldPhase::WarCry);
                        cries.push((pos, hacked));
                        fx(&mut commands, FxKind::Explosion, pos, 0.0, EMPOWERED_COLOR);
                    } else {
                        enemy.phase = Some(HeraldPhase::Frenzy);
                        fx(&mut commands, FxKind::Burst, pos, 0.0, Color::srgb(1.0, 0.2, 0.25));
                    }
                    enemy.state = AiState::Hunt;
                    enemy.attack_cooldown = 0.4;
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
        } else if let Ok((.., mut health)) = keepers.get_mut(victim) {
            health.hurt(damage);
        }
    }

    // A war cry empowers everyone on the Herald's side in earshot for good, the
    // Herald too: guards, or for a hacked Herald, the crew and its turned guards.
    for (cry, for_crew) in cries {
        for (_, mut enemy, transform, health, _) in &mut enemies {
            let at = transform.translation.truncate();
            let ally = (enemy.hacked) == for_crew;
            if enemy.empowered || !ally || health.is_dead() || at.distance(cry) > HERALD_REACH {
                continue;
            }
            enemy.empowered = true;
            fx(&mut commands, FxKind::Zap(at), cry, 0.0, EMPOWERED_COLOR);
        }
        if !for_crew {
            continue;
        }
        for (_, transform, health, player, _, boons) in &mut victims {
            let at = transform.translation.truncate();
            let Some(mut boons) = boons else { continue };
            let active = player.is_some_and(|p| p.status == PirateStatus::Active);
            if boons.empowered || !active || health.is_dead() || at.distance(cry) > HERALD_REACH {
                continue;
            }
            boons.empowered = true;
            fx(&mut commands, FxKind::Zap(at), cry, 0.0, EMPOWERED_COLOR);
        }
    }
}

/// Whether a guard's blow misses, if a Hacker is jamming it.
pub(crate) fn whiffs(rng: &mut Rng, jammed: bool) -> bool {
    jammed && rng.unit() < JAM_MISS
}

/// Where a guard's shot goes: if a Hacker is jamming it, now and then well
/// wide of `angle`.
fn jam_aim(rng: &mut Rng, jammed: bool, angle: f32) -> f32 {
    if whiffs(rng, jammed) {
        angle + rng.signed().signum() * (0.4 + 0.4 * rng.unit())
    } else {
        angle
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
        look.set_if_neq(enemy.look());

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
        } else if look.jammed && ((time.elapsed_secs() * 23.0).sin() * (time.elapsed_secs() * 7.0).sin()) > 0.45 {
            // Glitches now and then: its targeting is being jammed.
            Color::srgb(0.55, 0.8, 0.6)
        } else if look.empowered {
            // Smoulders orange: it hits much harder now.
            let heat = 0.5 + 0.5 * (time.elapsed_secs() * 4.0).sin();
            Color::srgb(1.0, 0.7 + 0.15 * heat, 0.45 + 0.2 * heat)
        } else if look.frenzied {
            // Seething red.
            let heat = 0.5 + 0.5 * (time.elapsed_secs() * 9.0).sin();
            Color::srgb(1.0, 0.45 + 0.25 * heat, 0.45 + 0.25 * heat)
        } else if look.state == AiState::Recover {
            Color::srgb(0.6, 0.6, 0.75)
        } else {
            Color::WHITE
        };
    }
}

fn draw_power_rings(
    time: Res<Time>,
    looks: Query<&EnemyLook>,
    mut rings: Query<(&ChildOf, &mut Sprite, &mut Visibility), With<PowerRing>>,
) {
    let pulse = 0.85 + 0.15 * (time.elapsed_secs() * 5.0).sin();
    for (parent, mut sprite, mut visibility) in &mut rings {
        let on = looks.get(parent.parent()).is_ok_and(|l| l.empowered);
        visibility.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
        if on {
            sprite.color = EMPOWERED_COLOR.with_alpha(pulse);
        }
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

fn enemy_deaths(
    mut commands: Commands,
    mut rng: ResMut<Rng>,
    enemies: Query<(Entity, &Enemy, &Health, &Transform, Has<Summoned>)>,
    pirates: Query<(&Player, &Health, &Arsenal), Without<Enemy>>,
) {
    // Ammo boxes are for the guns the living crew carries.
    let crew = || {
        pirates
            .iter()
            .filter(|(player, health, _)| player.status == PirateStatus::Active && !health.is_dead())
            .map(|(.., arsenal)| arsenal)
    };
    for (entity, enemy, health, transform, summoned) in &enemies {
        if health.is_dead() {
            let stats = enemy.kind.stats();
            let pos = transform.translation.truncate();
            fx(&mut commands, FxKind::Burst, pos, 0.0, stats.death_color);
            let (lo, hi) = stats.bounty;
            let credits = lo + (rng.unit() * (hi - lo) as f32).round() as u32;
            // Each drop is its own roll, so a body may leave nothing at all.
            let ammo_rolls = if matches!(enemy.kind, EnemyKind::Warden | EnemyKind::Brute) { 2 } else { 1 };
            let mut drops = Vec::new();
            // A boss's summons leave only ammo behind.
            if !summoned && rng.unit() < CREDIT_DROP_CHANCE {
                drops.push(EnemyDrop::Credits(credits));
            }
            for _ in 0..ammo_rolls {
                if rng.unit() < AMMO_DROP_CHANCE
                    && let Some(kind) = pick_ammo(&mut rng, crew())
                {
                    drops.push(EnemyDrop::Ammo(kind));
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
    // Heralds come with the reinforcements, to rally them.
    let herald = 0.05 + 0.03 * alarm_level;
    if r < warden {
        EnemyKind::Warden
    } else if r < warden + brute {
        EnemyKind::Brute
    } else if r < warden + brute + herald {
        EnemyKind::Herald
    } else if r < warden + brute + herald + 0.4 {
        EnemyKind::Stalker
    } else {
        EnemyKind::SentryDrone
    }
}
