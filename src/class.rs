//! Pirate classes and loadouts. Each pirate picks a class and their guns in the
//! menu; the host spawns them with it. The Gunner carries an extra gun; the
//! others have an ability on Q: the Engineer drops a sentry gun, the Bulwark
//! carries a bulletproof riot shield and the Hacker turns a guard against the
//! station.

use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fs};

use bevy::color::Mix;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, Hurtbox, Noise, Projectile, ProjectileLook, Team, fx, spawn_projectile};
use crate::enemies::Enemy;
use crate::guns::GunKind;
use crate::mission::Alarm;
use crate::net::{LocalId, LocalPlayer, authority};
use crate::player::{Aim, Controls, PirateStatus, Player, move_players};
use crate::room::RoomGrid;
use crate::{GameState, Hud, Level, Rng};

/// Guns every pirate can carry; the Gunner gets one more.
pub const BASE_SLOTS: usize = 3;
/// Seconds a sentry keeps fighting.
pub const SENTRY_LIFE: f32 = 15.0;
/// Seconds from dropping a sentry until the next one is ready.
pub const SENTRY_COOLDOWN: f32 = 25.0;
const SENTRY_HEALTH: f32 = 80.0;
const SENTRY_RANGE: f32 = 120.0;
const SENTRY_FIRE_GAP: f32 = 0.22;
const SENTRY_DAMAGE: f32 = 6.0;
const SENTRY_SPREAD: f32 = 0.08;
const SENTRY_SHOT_SPEED: f32 = 340.0;
const SENTRY_COLOR: Color = Color::srgb(1.0, 0.62, 0.25);
/// How close the Engineer must be to pick their sentry back up.
const SENTRY_PICKUP: f32 = 16.0;
/// Picking a sentry up takes this share of its remaining life, as a share of
/// the full cooldown, off the cooldown: a fresh one comes back 75% ready.
const SENTRY_REFUND: f32 = 0.75;
/// Where the gun head turns, relative to the sentry's origin.
const SENTRY_PIVOT: Vec2 = Vec2::new(0.0, 0.5);

/// Seconds of riot shield a full charge holds. Q raises and lowers it; it
/// drains while up, recharges while down, and can't be shot down.
pub const RIOT_LIFE: f32 = 10.0;
/// Seconds to recharge from empty to full.
const RIOT_RECHARGE: f32 = 22.0;
/// Charge needed to raise it, so it can't be flickered on and off.
const RIOT_MIN_CHARGE: f32 = 2.0;
/// Half the shield's width, across the Bulwark's aim: room for a pirate or two behind.
const RIOT_HALF: f32 = 13.0;
/// How far in front of the Bulwark it's held.
const RIOT_REACH: f32 = 11.0;
/// How far the middle of the shield bows out past its edges.
const RIOT_BULGE: f32 = 3.5;
/// Where the Bulwark holds it from, relative to his origin.
const RIOT_HAND: Vec2 = Vec2::new(0.0, -3.0);
/// Points along the curve, edge to edge.
const RIOT_POINTS: usize = 7;
/// Shots this close to the shield are stopped.
pub const RIOT_THICKNESS: f32 = 3.0;
pub const RIOT_COLOR: Color = Color::srgb(0.45, 0.85, 1.0);
/// What it burns down to as its time runs out.
const RIOT_SPENT: Color = Color::srgb(1.0, 0.3, 0.2);

/// Seconds a hacked guard fights for the crew.
pub const HACK_LIFE: f32 = 20.0;
const HACK_COOLDOWN: f32 = 30.0;
/// How far away a Hacker can reach a guard.
const HACK_RANGE: f32 = 140.0;
/// How close to the crosshair the guard must be.
const HACK_AIM_RADIUS: f32 = 36.0;
const HACK_COLOR: Color = Color::srgb(0.45, 1.0, 0.55);

pub struct ClassPlugin;

impl Plugin for ClassPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalLoadout>()
            .init_resource::<Loadouts>()
            .add_systems(
                Update,
                (use_abilities, run_sentries, carry_riot_shields)
                    .chain()
                    .after(move_players)
                    .run_if(in_state(GameState::Playing))
                    .run_if(authority),
            );
    }
}

pub struct ClassViewPlugin;

impl Plugin for ClassViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(dress_sentry)
            .add_observer(dress_riot_shield)
            .add_systems(Startup, spawn_ability_hud)
            .add_systems(Update, (draw_sentries, draw_riot_shields, update_ability_hud));
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum PirateClass {
    /// Carries a fourth gun.
    #[default]
    Gunner,
    /// Drops a sentry gun.
    Engineer,
    /// Carries a bulletproof riot shield.
    Bulwark,
    /// Turns a guard against the station.
    Hacker,
}

/// A class ability's timings and look.
pub struct AbilityStats {
    /// Short name for the HUD.
    pub label: &'static str,
    pub cooldown: f32,
    /// Seconds the effect lasts.
    pub life: f32,
    pub color: Color,
}

impl PirateClass {
    pub const ALL: [PirateClass; 4] = [Self::Gunner, Self::Engineer, Self::Bulwark, Self::Hacker];

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name().eq_ignore_ascii_case(raw))
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Gunner => "GUNNER",
            Self::Engineer => "ENGINEER",
            Self::Bulwark => "BULWARK",
            Self::Hacker => "HACKER",
        }
    }

    pub fn perk(self) -> &'static str {
        match self {
            Self::Gunner => "Carries a 4th gun",
            Self::Engineer => "Q: drop a sentry gun",
            Self::Bulwark => "Q: riot shield",
            Self::Hacker => "Q: turn a guard",
        }
    }

    pub fn slots(self) -> usize {
        match self {
            Self::Gunner => BASE_SLOTS + 1,
            Self::Engineer | Self::Bulwark | Self::Hacker => BASE_SLOTS,
        }
    }

    /// The Q ability, if the class has one.
    pub fn ability(self) -> Option<AbilityStats> {
        Some(match self {
            Self::Gunner => return None,
            Self::Engineer => AbilityStats {
                label: "SNTRY",
                cooldown: SENTRY_COOLDOWN,
                life: SENTRY_LIFE,
                color: SENTRY_COLOR,
            },
            Self::Bulwark => AbilityStats {
                label: "SHIELD",
                // Unused: the shield runs on `Ability::charge` instead.
                cooldown: RIOT_RECHARGE,
                life: RIOT_LIFE,
                color: RIOT_COLOR,
            },
            Self::Hacker => AbilityStats {
                label: "HACK",
                cooldown: HACK_COOLDOWN,
                life: HACK_LIFE,
                color: HACK_COLOR,
            },
        })
    }

    pub fn sprite(self) -> &'static str {
        match self {
            Self::Gunner => "sprites/player.png",
            Self::Engineer => "sprites/player_engineer.png",
            Self::Bulwark => "sprites/player_bulwark.png",
            Self::Hacker => "sprites/player_hacker.png",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Gunner => Color::srgb(1.0, 0.82, 0.35),
            Self::Engineer => SENTRY_COLOR,
            Self::Bulwark => RIOT_COLOR,
            Self::Hacker => HACK_COLOR,
        }
    }
}

/// A class and the guns it carries, in slot order.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Loadout {
    pub class: PirateClass,
    pub guns: Vec<GunKind>,
}

impl Loadout {
    /// The class with its slots filled from a sensible default list.
    pub fn new(class: PirateClass) -> Self {
        const DEFAULT: [GunKind; 4] = [
            GunKind::BlasterPistol,
            GunKind::PulseRifle,
            GunKind::ScatterCannon,
            GunKind::RailLance,
        ];
        Self { class, guns: DEFAULT[..class.slots()].to_vec() }
    }

    pub fn is_full(&self) -> bool {
        self.guns.len() >= self.class.slots()
    }

    /// Switches class, dropping guns from the end that no longer fit.
    pub fn set_class(&mut self, class: PirateClass) {
        self.class = class;
        self.guns.truncate(class.slots());
    }

    /// Equips `gun` in the next free slot, or unequips it. Returns false if
    /// there was no room.
    pub fn toggle(&mut self, gun: GunKind) -> bool {
        if let Some(i) = self.guns.iter().position(|&g| g == gun) {
            self.guns.remove(i);
        } else if self.is_full() {
            return false;
        } else {
            self.guns.push(gun);
        }
        true
    }

    /// Makes a loadout from another machine legal: no repeats, no more guns
    /// than slots, and at least one gun.
    pub fn sanitized(mut self) -> Self {
        let mut seen = Vec::new();
        self.guns.retain(|g| {
            let fresh = !seen.contains(g);
            seen.push(*g);
            fresh
        });
        self.guns.truncate(self.class.slots());
        if self.guns.is_empty() {
            self.guns.push(GunKind::BlasterPistol);
        }
        self
    }
}

impl Loadout {
    /// Where this machine's pick is remembered between launches.
    fn path() -> Option<PathBuf> {
        let base = env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("dozd"));
        base.or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".dozd")))
            .map(|dir| dir.join("loadout.txt"))
    }

    /// The pick saved by `save`, if there is a readable one.
    pub fn load() -> Option<Self> {
        let text = fs::read_to_string(Self::path()?).ok()?;
        let mut lines = text.lines();
        let class = PirateClass::parse(lines.next()?.trim())?;
        let guns = lines
            .next()?
            .split(',')
            .map(|name| GunKind::ALL.into_iter().find(|g| format!("{g:?}") == name.trim()))
            .collect::<Option<Vec<_>>>()?;
        Some(Self { class, guns }.sanitized())
    }

    /// Remembers this pick for the next launch. Failing to is harmless.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let guns: Vec<String> = self.guns.iter().map(|g| format!("{g:?}")).collect();
        let text = format!("{}\n{}\n", self.class.name(), guns.join(","));
        if let Err(error) = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&path, text)) {
            warn!("couldn't save the loadout to {}: {error}", path.display());
        }
    }
}

impl Default for Loadout {
    fn default() -> Self {
        Self::new(PirateClass::default())
    }
}

/// This machine's pick from the menu; sent to the host on joining.
#[derive(Resource, Default)]
pub struct LocalLoadout(pub Loadout);

/// Host-side: what each pirate chose, by `Player::owner`. Pirates not listed
/// (the host's own) use `LocalLoadout`.
#[derive(Resource, Default)]
pub struct Loadouts(pub HashMap<u64, Loadout>);

impl Loadouts {
    pub fn get<'a>(&'a self, owner: u64, local: &'a LocalLoadout) -> &'a Loadout {
        self.0.get(&owner).unwrap_or(&local.0)
    }
}

/// A pirate's class ability, ready when `cooldown` reaches zero.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Ability {
    pub cooldown: f32,
    /// Seconds left on the effect last used.
    pub active: f32,
    /// A Bulwark's riot shield charge, in seconds of shield.
    pub charge: f32,
}

impl Default for Ability {
    fn default() -> Self {
        Self { cooldown: 0.0, active: 0.0, charge: RIOT_LIFE }
    }
}

/// A Bulwark's riot shield, held out in front of them wherever they aim. Stops
/// every shot, from either side, until it times out.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct RiotShield {
    pub owner: u64,
    pub left: f32,
    /// Which way it faces: the Bulwark's aim.
    pub facing: Vec2,
}

impl RiotShield {
    /// The shield's curve in the world, edge to edge, given where it's held.
    pub fn outline(&self, center: Vec2) -> [Vec2; RIOT_POINTS] {
        riot_curve().map(|p| center + self.facing * p.x + self.facing.perp() * p.y)
    }
}

/// The shield's curve relative to where it's held: `x` outward, `y` across.
fn riot_curve() -> [Vec2; RIOT_POINTS] {
    std::array::from_fn(|i| {
        let t = i as f32 / (RIOT_POINTS - 1) as f32 * 2.0 - 1.0;
        Vec2::new(RIOT_BULGE * (1.0 - t * t), t * RIOT_HALF)
    })
}

/// An Engineer's sentry gun. Shoots the closest enemy it can see until its time
/// runs out or it's destroyed.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Sentry {
    /// `Player::owner` of the Engineer who dropped it.
    pub owner: u64,
    /// Seconds left before it shuts down.
    pub left: f32,
    pub aim: Vec2,
    reload: f32,
}

#[derive(Component)]
struct SentryHead;

/// A piece of a riot shield's curve: the bright core, or its soft glow.
#[derive(Component)]
struct RiotPart {
    glow: bool,
}

/// Time-left bar over a sentry: the backing, or the fill when `fill` is set.
#[derive(Component)]
struct SentryBar {
    fill: bool,
}

#[derive(Component)]
struct AbilityPanel;

#[derive(Component)]
struct AbilityLabel;

#[derive(Component)]
struct AbilityFill;

#[derive(Component)]
struct AbilityText;

fn can_act(player: &Player, health: &Health) -> bool {
    player.status == PirateStatus::Active && !health.is_dead()
}

fn use_abilities(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut players: Query<(&Player, &Health, &Controls, &Transform, &Aim, &mut Ability)>,
    sentries: Query<(Entity, &Sentry, &Transform), Without<Player>>,
    riot_shields: Query<(Entity, &RiotShield)>,
    mut enemies: Query<(&mut Enemy, &Transform, &Health), Without<Player>>,
) {
    let dt = time.delta_secs();
    for (player, health, controls, transform, aim, mut ability) in &mut players {
        ability.cooldown = (ability.cooldown - dt).max(0.0);
        ability.active = (ability.active - dt).max(0.0);
        let Some(stats) = player.class.ability() else { continue };
        let pos = transform.translation.truncate();

        // The riot shield toggles, running on its charge rather than a cooldown.
        if player.class == PirateClass::Bulwark {
            let raised = riot_shields.iter().find(|(_, s)| s.owner == player.owner).map(|(e, _)| e);
            if raised.is_none() {
                ability.charge = (ability.charge + dt * RIOT_LIFE / RIOT_RECHARGE).min(RIOT_LIFE);
            }
            if !controls.ability || !can_act(player, health) {
                continue;
            }
            match raised {
                // Lowered: whatever charge is left stays for next time.
                Some(entity) => {
                    commands.entity(entity).despawn();
                }
                None if ability.charge >= RIOT_MIN_CHARGE => {
                    let shield = RiotShield { owner: player.owner, left: ability.charge, facing: aim.dir };
                    let transform = riot_transform(pos, aim.dir);
                    fx(&mut commands, FxKind::Burst, transform.translation.truncate(), 0.0, RIOT_COLOR);
                    commands.spawn((Level, Replicated, shield, transform));
                }
                None => fx(&mut commands, FxKind::Spark, pos + aim.dir * RIOT_REACH, 0.0, RIOT_SPENT),
            }
            continue;
        }

        // Standing by their own sentry, the Engineer packs it up instead, and
        // the time it had left comes back as a shorter cooldown.
        if player.class == PirateClass::Engineer && controls.ability && can_act(player, health) {
            let nearby = sentries.iter().find(|(_, s, t)| {
                s.owner == player.owner && t.translation.truncate().distance(pos) <= SENTRY_PICKUP
            });
            if let Some((entity, sentry, t)) = nearby {
                let refund = SENTRY_REFUND * (sentry.left / SENTRY_LIFE).clamp(0.0, 1.0) * SENTRY_COOLDOWN;
                ability.cooldown = (ability.cooldown - refund).max(0.0);
                ability.active = 0.0;
                fx(&mut commands, FxKind::Burst, t.translation.truncate(), 0.0, SENTRY_COLOR);
                commands.entity(entity).despawn();
                continue;
            }
        }

        if !controls.ability || ability.cooldown > 0.0 || !can_act(player, health) {
            continue;
        }
        match player.class {
            PirateClass::Gunner | PirateClass::Bulwark => {}
            PirateClass::Engineer => {
                // One sentry each: a new one replaces the old.
                for (entity, sentry, _) in &sentries {
                    if sentry.owner == player.owner {
                        commands.entity(entity).despawn();
                    }
                }
                let ahead = pos + aim.dir * 10.0;
                let spot = if grid.is_solid_at(ahead) { pos } else { ahead };
                commands.spawn((
                    Level,
                    Replicated,
                    Sentry {
                        owner: player.owner,
                        left: SENTRY_LIFE,
                        aim: aim.dir,
                        reload: 0.5,
                    },
                    Team::Player,
                    Health::new(SENTRY_HEALTH, 0.0),
                    Hurtbox(6.0),
                    Transform::from_translation(spot.extend(9.0)),
                ));
                fx(&mut commands, FxKind::Burst, spot, 0.0, SENTRY_COLOR);
            }
            PirateClass::Hacker => {
                // The guard nearest the crosshair that the Hacker can see.
                let target = enemies
                    .iter_mut()
                    .map(|(enemy, t, h)| (enemy, t.translation.truncate(), h))
                    .filter(|(enemy, at, h)| {
                        !enemy.is_hacked()
                            && !h.is_dead()
                            && at.distance(pos) <= HACK_RANGE
                            && at.distance(aim.target) <= HACK_AIM_RADIUS
                            && grid.line_of_sight(pos, *at)
                    })
                    .min_by(|a, b| a.1.distance(aim.target).total_cmp(&b.1.distance(aim.target)));
                let Some((mut enemy, at, _)) = target else {
                    // Nothing to hack: a fizzle, and the ability stays ready.
                    fx(&mut commands, FxKind::Spark, aim.target, 0.0, HACK_COLOR.with_alpha(0.5));
                    continue;
                };
                enemy.hack(HACK_LIFE);
                fx(&mut commands, FxKind::Zap(at), pos, 0.0, HACK_COLOR);
                fx(&mut commands, FxKind::Burst, at, 0.0, HACK_COLOR);
            }
        }
        ability.cooldown = stats.cooldown;
        ability.active = stats.life;
    }
}

/// Where a riot shield sits for a Bulwark at `pos` aiming along `facing`.
fn riot_transform(pos: Vec2, facing: Vec2) -> Transform {
    let center = pos + RIOT_HAND + facing * RIOT_REACH;
    // In front of the Bulwark, unless held up and away from the camera.
    let z = if facing.y > 0.4 { 9.7 } else { 10.4 };
    Transform::from_translation(center.extend(z)).with_rotation(Quat::from_rotation_z(facing.to_angle()))
}

/// Keeps each riot shield in its Bulwark's hands, draining their charge, until
/// it runs dry or they fall or leave.
fn carry_riot_shields(
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<(&Player, &Health, &Transform, &Aim, &mut Ability)>,
    mut shields: Query<(Entity, &mut RiotShield, &mut Transform), Without<Player>>,
) {
    for (entity, mut shield, mut transform) in &mut shields {
        let holder = players
            .iter_mut()
            .find(|(p, h, ..)| p.owner == shield.owner && can_act(p, h));
        let Some((_, _, holder, aim, mut ability)) = holder else {
            commands.entity(entity).despawn();
            continue;
        };
        ability.charge = (ability.charge - time.delta_secs()).max(0.0);
        shield.left = ability.charge;
        if ability.charge <= 0.0 {
            fx(&mut commands, FxKind::Burst, transform.translation.truncate(), 0.0, RIOT_SPENT);
            commands.entity(entity).despawn();
            continue;
        }
        if shield.facing != aim.dir {
            shield.facing = aim.dir;
        }
        let held = riot_transform(holder.translation.truncate(), aim.dir);
        if *transform != held {
            *transform = held;
        }
    }
}

fn run_sentries(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut noise: ResMut<Noise>,
    mut alarm: ResMut<Alarm>,
    mut sentries: Query<(Entity, &mut Sentry, &Health, &Transform)>,
    enemies: Query<(&Transform, &Health, &Hurtbox, &Team), With<Enemy>>,
) {
    let dt = time.delta_secs();
    for (entity, mut sentry, health, transform) in &mut sentries {
        let pos = transform.translation.truncate();
        sentry.left -= dt;
        if sentry.left <= 0.0 || health.is_dead() {
            fx(&mut commands, FxKind::Burst, pos, 0.0, SENTRY_COLOR);
            commands.entity(entity).despawn();
            continue;
        }

        sentry.reload = (sentry.reload - dt).max(0.0);
        let pivot = pos + SENTRY_PIVOT;
        let target = enemies
            .iter()
            .filter(|(t, h, hurtbox, team)| {
                let at = t.translation.truncate();
                **team == Team::Enemy && !h.is_dead() && at.distance(pivot) <= SENTRY_RANGE + hurtbox.0 && grid.line_of_sight(pivot, at)
            })
            .map(|(t, ..)| t.translation.truncate())
            .min_by(|a, b| a.distance(pivot).total_cmp(&b.distance(pivot)));
        let Some(target) = target else { continue };

        let dir = (target - pivot).normalize_or(sentry.aim);
        if sentry.aim != dir {
            sentry.aim = dir;
        }
        if sentry.reload > 0.0 {
            continue;
        }
        sentry.reload = SENTRY_FIRE_GAP;
        let muzzle = pivot + dir * 8.0;
        let angle = dir.to_angle() + rng.signed() * SENTRY_SPREAD / 2.0;
        spawn_projectile(
            &mut commands,
            Projectile {
                team: Team::Player,
                velocity: Vec2::from_angle(angle) * SENTRY_SHOT_SPEED,
                remaining: SENTRY_RANGE * 1.5,
                damage: SENTRY_DAMAGE,
                knockback: 20.0,
                pierce: false,
                color: SENTRY_COLOR,
                hits: Vec::new(),
            },
            ProjectileLook::OrangePellet,
            muzzle,
        );
        fx(&mut commands, FxKind::MuzzleFlash, muzzle, dir.to_angle(), SENTRY_COLOR);
        // Gunfire is gunfire: guards hear a sentry like any pirate.
        noise.0.push(pivot);
        alarm.alert();
    }
}

fn dress_sentry(add: On<Add, Sentry>, mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .entity(add.entity)
        .insert((Visibility::default(), Sprite::from_image(assets.load("sprites/sentry.png"))))
        .with_children(|parent| {
            parent.spawn((
                Sprite::from_image(assets.load("sprites/shadow.png")),
                Transform::from_xyz(0.0, -1.0, -0.1),
            ));
            parent.spawn((
                SentryHead,
                Sprite::from_image(assets.load("sprites/sentry_gun.png")),
                Transform::from_translation(SENTRY_PIVOT.extend(0.1)),
            ));
            parent.spawn((
                SentryBar { fill: false },
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.75), Vec2::new(14.0, 3.0)),
                Transform::from_xyz(0.0, 9.0, 1.0),
            ));
            parent.spawn((
                SentryBar { fill: true },
                Sprite::from_color(SENTRY_COLOR, Vec2::new(12.0, 1.0)),
                Transform::from_xyz(0.0, 9.0, 1.1),
            ));
        });
}

fn draw_sentries(
    time: Res<Time>,
    sentries: Query<(&Sentry, &Health)>,
    mut bodies: Query<(&Sentry, &Health, &mut Sprite), (Without<SentryHead>, Without<SentryBar>)>,
    mut heads: Query<(&ChildOf, &mut Transform, &mut Sprite), (With<SentryHead>, Without<SentryBar>)>,
    mut bars: Query<(&ChildOf, &SentryBar, &mut Transform, &mut Sprite), Without<SentryHead>>,
) {
    // Flickers as it runs down, so the crew knows it's about to go.
    let blink = (time.elapsed_secs() * 10.0).sin() > 0.0;
    let tint = |sentry: &Sentry, health: &Health| {
        if health.flash > 0.0 {
            Color::srgb(1.0, 0.3, 0.3)
        } else if sentry.left < 3.0 && blink {
            Color::srgba(1.0, 1.0, 1.0, 0.5)
        } else {
            Color::WHITE
        }
    };
    for (sentry, health, mut sprite) in &mut bodies {
        sprite.color = tint(sentry, health);
    }
    for (parent, mut transform, mut sprite) in &mut heads {
        let Ok((sentry, health)) = sentries.get(parent.parent()) else { continue };
        let dir = sentry.aim;
        // Pivot near the back of the gun so it swings like a turret, not a propeller.
        transform.translation = (SENTRY_PIVOT + dir * 3.0).extend(if dir.y > 0.4 { -0.05 } else { 0.1 });
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
        sprite.flip_y = dir.x < 0.0;
        sprite.color = tint(sentry, health);
    }
    for (parent, bar, mut transform, mut sprite) in &mut bars {
        if !bar.fill {
            continue;
        }
        let Ok((sentry, _)) = sentries.get(parent.parent()) else { continue };
        let frac = (sentry.left / SENTRY_LIFE).clamp(0.0, 1.0);
        sprite.custom_size = Some(Vec2::new(12.0 * frac, 1.0));
        transform.translation.x = -6.0 * (1.0 - frac);
    }
}

/// Draws the shield's curve as short strokes: a soft glow under a bright core.
fn dress_riot_shield(add: On<Add, RiotShield>, mut commands: Commands) {
    commands.entity(add.entity).insert(Visibility::default()).with_children(|parent| {
        for (glow, width, z) in [(true, 5.0, 0.0), (false, 2.0, 0.1)] {
            for pair in riot_curve().windows(2) {
                let (a, b) = (pair[0], pair[1]);
                parent.spawn((
                    RiotPart { glow },
                    Sprite::from_color(RIOT_COLOR, Vec2::new(a.distance(b) + 1.0, width)),
                    Transform::from_translation(((a + b) / 2.0).extend(z))
                        .with_rotation(Quat::from_rotation_z((b - a).to_angle())),
                ));
            }
        }
    });
}

/// Burns from cyan down to red and fades as the shield's time runs out,
/// flickering over the last moments.
fn draw_riot_shields(
    time: Res<Time>,
    shields: Query<&RiotShield>,
    mut parts: Query<(&ChildOf, &RiotPart, &mut Sprite)>,
) {
    let t = time.elapsed_secs();
    for (parent, part, mut sprite) in &mut parts {
        let Ok(shield) = shields.get(parent.parent()) else { continue };
        let life = (shield.left / RIOT_LIFE).clamp(0.0, 1.0);
        let color = Srgba::from(RIOT_SPENT).mix(&Srgba::from(RIOT_COLOR), life);
        let mut alpha = if part.glow {
            (0.12 + 0.18 * life) * (1.0 + 0.3 * (t * 5.0).sin())
        } else {
            0.3 + 0.6 * life
        };
        if shield.left < 1.5 && (t * 16.0).sin() < 0.0 {
            alpha *= 0.3;
        }
        sprite.color = color.with_alpha(alpha).into();
    }
}

fn spawn_ability_hud(mut commands: Commands) {
    // Under the shield, hull and dash bars.
    commands.spawn((
        Hud,
        AbilityPanel,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(112.0),
            left: Val::Px(18.0),
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
        children![
            (
                AbilityLabel,
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextColor(Color::srgb(0.8, 0.85, 0.95)),
                Node {
                    min_width: Val::Px(44.0),
                    ..default()
                },
            ),
            (
                Node {
                    width: Val::Px(60.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                children![(
                    AbilityFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(SENTRY_COLOR),
                )],
            ),
            (
                AbilityText,
                Text::new(""),
                TextFont::from_font_size(13.0),
                TextColor(Color::WHITE),
            ),
        ],
    ));
}

fn update_ability_hud(
    local: Res<LocalId>,
    player: Option<Single<(&Player, &Ability, &Transform), With<LocalPlayer>>>,
    sentries: Query<(&Sentry, &Transform), Without<LocalPlayer>>,
    riot_shields: Query<&RiotShield>,
    mut panel: Single<&mut Node, (With<AbilityPanel>, Without<AbilityFill>)>,
    mut name: Single<&mut Text, (With<AbilityLabel>, Without<AbilityText>)>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<AbilityFill>>,
    mut text: Single<(&mut Text, &mut TextColor), With<AbilityText>>,
) {
    let stats = player.as_ref().and_then(|p| p.0.class.ability());
    let display = if stats.is_some() { Display::Flex } else { Display::None };
    if panel.display != display {
        panel.display = display;
    }
    let (Some(player), Some(stats)) = (player, stats) else { return };
    let (player, ability, transform) = *player;
    if name.0 != stats.label {
        name.0 = stats.label.into();
    }

    // A sentry can be destroyed early, so ask it rather than the timer.
    let sentry = sentries.iter().find(|(s, _)| s.owner == local.0);
    let active = match player.class {
        PirateClass::Engineer => sentry.map(|(s, _)| s.left),
        _ => (ability.active > 0.0).then_some(ability.active),
    };
    let in_reach = sentry.is_some_and(|(_, t)| {
        t.translation.truncate().distance(transform.translation.truncate()) <= SENTRY_PICKUP
    });
    let (fill_node, fill_color) = &mut *fill;
    let (text, text_color) = &mut *text;
    let dim = Color::srgba(1.0, 1.0, 1.0, 0.6);
    let (frac, color, label, label_color) = if player.class == PirateClass::Bulwark {
        // The bar is the shield's charge.
        let frac = ability.charge / RIOT_LIFE;
        if riot_shields.iter().any(|s| s.owner == local.0) {
            (frac, stats.color, format!("UP {:.0}s", ability.charge.ceil()), stats.color)
        } else if ability.charge < RIOT_MIN_CHARGE {
            (frac, Color::srgb(0.4, 0.45, 0.55), "CHARGING".into(), dim)
        } else {
            (frac, stats.color, "READY  [Q]".into(), Color::WHITE)
        }
    } else if let Some(left) = active {
        let label = if in_reach { "PICK UP  [Q]".into() } else { format!("ACTIVE {:.0}s", left.ceil()) };
        (left / stats.life, stats.color, label, stats.color)
    } else if ability.cooldown > 0.0 {
        let ready = 1.0 - ability.cooldown / stats.cooldown;
        (ready, Color::srgb(0.4, 0.45, 0.55), format!("{:.0}s", ability.cooldown.ceil()), Color::srgba(1.0, 1.0, 1.0, 0.6))
    } else {
        (1.0, stats.color, "READY  [Q]".into(), Color::WHITE)
    };
    fill_node.width = Val::Percent(frac.clamp(0.0, 1.0) * 100.0);
    fill_color.0 = color;
    if text.0 != label {
        text.0 = label;
    }
    text_color.0 = label_color;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loadouts_respect_slots() {
        let mut loadout = Loadout::new(PirateClass::Gunner);
        assert_eq!(loadout.guns.len(), 4);
        assert!(!loadout.toggle(GunKind::ArcCoil), "a full loadout takes no more");
        loadout.set_class(PirateClass::Engineer);
        assert_eq!(loadout.guns.len(), 3);
        assert!(loadout.toggle(GunKind::PulseRifle));
        assert!(loadout.toggle(GunKind::ArcCoil));
        assert_eq!(loadout.guns.last(), Some(&GunKind::ArcCoil));

        let cheat = Loadout {
            class: PirateClass::Engineer,
            guns: vec![GunKind::RailLance; 3].into_iter().chain(GunKind::ALL).collect(),
        };
        let fixed = cheat.sanitized();
        assert_eq!(fixed.guns, [GunKind::RailLance, GunKind::BlasterPistol, GunKind::ScatterCannon]);
        let empty = Loadout { class: PirateClass::Gunner, guns: Vec::new() }.sanitized();
        assert_eq!(empty.guns, [GunKind::BlasterPistol]);
    }
}
