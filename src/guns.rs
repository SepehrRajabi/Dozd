use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::abilities::{Passive, RICOCHET_CHANCE};
use crate::class::{Boons, RIOT_COLOR, RiotShield, SUPPLY_CHANCE};
use crate::combat::{
    FxKind, Grenade, Health, Hurtbox, Knockback, Noise, NovaOrb, Projectile, ProjectileLook, Team, fx, spawn_grenade,
    spawn_nova, spawn_projectile,
};
use crate::enemies::Enemy;
use crate::hud::{self, HudSlot, Meter};
use crate::mission::Alarm;
use crate::net::{LocalPlayer, authority};
use crate::perks::{PerkKind, Perks};
use crate::player::{Aim, Controls, LocalAim, Player, PirateStatus};
use crate::room::RoomGrid;
use crate::{GameState, Hud, Rng};

/// Where the gun is held relative to the player's origin.
const HAND: Vec2 = Vec2::new(0.0, -3.0);
const HOLD_DISTANCE: f32 = 5.0;
/// Distance from the gun's centre to its muzzle.
const BARREL: f32 = 8.0;

pub struct GunsPlugin;

impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (switch_gun, reload, fire)
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(authority),
        );
    }
}

pub struct GunsViewPlugin;

impl Plugin for GunsViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_crosshair, spawn_weapon_hud))
            .add_systems(Update, ((equip_held_gun, aim_held_guns, draw_charge_glows).chain(), update_weapon_hud));
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum GunKind {
    BlasterPistol,
    ScatterCannon,
    PulseRifle,
    RailLance,
    ArcCoil,
    GrenadeLauncher,
    NovaCannon,
}

/// How a gun turns a trigger pull into damage.
#[derive(Clone, Copy)]
enum FireMode {
    /// `pellets` projectiles flying straight.
    Shots,
    /// Instant arcs to the closest enemies within `range`. The gun's `damage` is
    /// shared between everything it hits, so a lone target takes all of it.
    Arc { targets: usize },
    /// A burst of grenades lobbed toward the aim point (at most `range` away),
    /// each landing within `scatter` of it and exploding in `radius`.
    Lob { burst: u32, gap: f32, radius: f32, scatter: f32 },
    /// Held to charge, one cell of the magazine every `cell_time`, and loosed
    /// on release as a slow orb. The orb's burst (`damage` at full charge,
    /// within `radius`) and the arcs it lashes passing guards with, up to
    /// `arc_reach` away, scale with the cells it burned.
    Charge { cell_time: f32, radius: f32, arc_damage: f32, arc_reach: f32 },
}

pub struct GunStats {
    pub name: &'static str,
    /// What it's good for, in a few words, for the loadout screen.
    pub role: &'static str,
    pub sprite: &'static str,
    mode: FireMode,
    look: ProjectileLook,
    color: Color,
    /// Seconds between shots.
    cooldown: f32,
    automatic: bool,
    pellets: u32,
    /// Full width of the firing cone, in radians.
    spread: f32,
    speed: f32,
    range: f32,
    damage: f32,
    knockback: f32,
    pierce: bool,
    magazine: u32,
    reload_time: f32,
    /// Spare rounds carried at the start of a run; `None` means unlimited.
    /// Pirates can carry up to twice this.
    reserve: Option<u32>,
}

const BLASTER_PISTOL: GunStats = GunStats {
    name: "Blaster Pistol",
    role: "never runs dry",
    mode: FireMode::Shots,
    sprite: "sprites/guns/blaster_pistol.png",
    look: ProjectileLook::CyanBolt,
    color: Color::srgb(0.4, 0.88, 1.0),
    cooldown: 0.2,
    automatic: false,
    pellets: 1,
    spread: 0.04,
    speed: 320.0,
    range: 240.0,
    damage: 14.0,
    knockback: 60.0,
    pierce: false,
    magazine: 12,
    reserve: None,
    reload_time: 0.9,
};

const SCATTER_CANNON: GunStats = GunStats {
    name: "Scatter Cannon",
    role: "close-range spread",
    mode: FireMode::Shots,
    sprite: "sprites/guns/scatter_cannon.png",
    look: ProjectileLook::OrangePellet,
    color: Color::srgb(1.0, 0.6, 0.2),
    cooldown: 0.65,
    automatic: false,
    pellets: 7,
    spread: 0.5,
    speed: 260.0,
    range: 110.0,
    damage: 9.0,
    knockback: 45.0,
    pierce: false,
    magazine: 4,
    reserve: Some(12),
    reload_time: 1.4,
};

const PULSE_RIFLE: GunStats = GunStats {
    name: "Pulse Rifle",
    role: "automatic, mid range",
    mode: FireMode::Shots,
    sprite: "sprites/guns/pulse_rifle.png",
    look: ProjectileLook::GreenBolt,
    color: Color::srgb(0.4, 1.0, 0.45),
    cooldown: 0.09,
    automatic: true,
    pellets: 1,
    spread: 0.12,
    speed: 360.0,
    range: 200.0,
    damage: 8.0,
    knockback: 25.0,
    pierce: false,
    magazine: 30,
    reserve: Some(90),
    reload_time: 1.6,
};

const RAIL_LANCE: GunStats = GunStats {
    name: "Rail Lance",
    role: "pierces, long range",
    mode: FireMode::Shots,
    sprite: "sprites/guns/rail_lance.png",
    look: ProjectileLook::RailBeam,
    color: Color::srgb(0.78, 0.45, 1.0),
    cooldown: 1.0,
    automatic: false,
    pellets: 1,
    spread: 0.0,
    speed: 900.0,
    range: 420.0,
    damage: 70.0,
    knockback: 140.0,
    pierce: true,
    magazine: 3,
    reserve: Some(6),
    reload_time: 2.0,
};

const ARC_COIL: GunStats = GunStats {
    name: "Arc Coil",
    role: "chains to 4 nearby",
    mode: FireMode::Arc { targets: 4 },
    sprite: "sprites/guns/arc_coil.png",
    look: ProjectileLook::CyanBolt,
    color: Color::srgb(1.0, 0.92, 0.35),
    cooldown: 0.4,
    automatic: true,
    pellets: 1,
    spread: 0.0,
    speed: 0.0,
    range: 80.0,
    damage: 40.0,
    knockback: 30.0,
    pierce: false,
    magazine: 16,
    reserve: Some(48),
    reload_time: 1.5,
};

const GRENADE_LAUNCHER: GunStats = GunStats {
    name: "Grenade Launcher",
    role: "3-grenade burst",
    mode: FireMode::Lob {
        burst: 3,
        gap: 0.14,
        radius: 26.0,
        scatter: 14.0,
    },
    sprite: "sprites/guns/grenade_launcher.png",
    look: ProjectileLook::Grenade,
    color: Color::srgb(1.0, 0.55, 0.2),
    cooldown: 1.2,
    automatic: false,
    pellets: 1,
    spread: 0.0,
    // Flight speed of a grenade, in pixels per second.
    speed: 170.0,
    range: 120.0,
    damage: 45.0,
    knockback: 160.0,
    pierce: false,
    magazine: 6,
    reserve: Some(12),
    reload_time: 2.2,
};

const NOVA_CANNON: GunStats = GunStats {
    name: "Nova Cannon",
    role: "charged, burns cells",
    mode: FireMode::Charge {
        cell_time: 0.4,
        radius: 44.0,
        arc_damage: 14.0,
        arc_reach: 60.0,
    },
    sprite: "sprites/guns/nova_cannon.png",
    look: ProjectileLook::NovaOrb,
    color: Color::srgb(0.45, 1.0, 0.35),
    cooldown: 0.8,
    automatic: false,
    pellets: 1,
    spread: 0.0,
    // The orb's crawl, in pixels per second.
    speed: 120.0,
    range: 260.0,
    damage: 200.0,
    knockback: 220.0,
    pierce: false,
    magazine: 4,
    reserve: Some(8),
    reload_time: 2.6,
};

impl GunKind {
    pub const ALL: [GunKind; 7] = [
        Self::BlasterPistol,
        Self::ScatterCannon,
        Self::PulseRifle,
        Self::RailLance,
        Self::ArcCoil,
        Self::GrenadeLauncher,
        Self::NovaCannon,
    ];

    pub fn stats(self) -> &'static GunStats {
        match self {
            Self::BlasterPistol => &BLASTER_PISTOL,
            Self::ScatterCannon => &SCATTER_CANNON,
            Self::PulseRifle => &PULSE_RIFLE,
            Self::RailLance => &RAIL_LANCE,
            Self::ArcCoil => &ARC_COIL,
            Self::GrenadeLauncher => &GRENADE_LAUNCHER,
            Self::NovaCannon => &NOVA_CANNON,
        }
    }

    /// Recoil distance in pixels.
    fn kick(self) -> f32 {
        match self {
            Self::BlasterPistol => 1.5,
            Self::ScatterCannon => 4.0,
            Self::PulseRifle => 1.0,
            Self::RailLance => 5.0,
            Self::ArcCoil => 1.0,
            Self::GrenadeLauncher => 3.5,
            Self::NovaCannon => 6.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Gun {
    kind: GunKind,
    ammo: u32,
    /// Rounds left to reload from; `None` is unlimited.
    spare: Option<u32>,
}

/// The guns a pirate is carrying and the state of the one in hand.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Arsenal {
    guns: Vec<Gun>,
    current: usize,
    cooldown: f32,
    /// Seconds left until the reload finishes.
    reloading: Option<f32>,
    /// Shots fired so far, so every machine can play recoil even when ammo doesn't drop.
    shots: u32,
    /// Shots still to come from the current burst, and the time until the next.
    burst_left: u32,
    burst_timer: f32,
    /// Seconds a charging gun's trigger has been held.
    charge: f32,
}

impl Arsenal {
    pub fn new(kinds: &[GunKind]) -> Self {
        Self {
            guns: kinds
                .iter()
                .map(|&kind| Gun {
                    kind,
                    ammo: kind.stats().magazine,
                    spare: kind.stats().reserve,
                })
                .collect(),
            current: 0,
            cooldown: 0.0,
            reloading: None,
            shots: 0,
            burst_left: 0,
            burst_timer: 0.0,
            charge: 0.0,
        }
    }

    /// Fills every magazine and cancels any reload.
    pub fn refill(&mut self) {
        for gun in &mut self.guns {
            gun.ammo = gun.kind.stats().magazine;
        }
        self.reloading = None;
    }

    /// Adds a magazine's worth of spare rounds to every gun with limited ammo.
    pub fn add_ammo(&mut self) {
        for gun in &mut self.guns {
            let stats = gun.kind.stats();
            if let (Some(spare), Some(reserve)) = (gun.spare.as_mut(), stats.reserve) {
                *spare = (*spare + stats.magazine).min(reserve * 2);
            }
        }
    }

    /// Whether any gun can carry more spare rounds.
    pub fn needs_ammo(&self) -> bool {
        self.guns
            .iter()
            .any(|gun| matches!((gun.spare, gun.kind.stats().reserve), (Some(spare), Some(reserve)) if spare < reserve * 2))
    }

    fn gun(&self) -> &Gun {
        &self.guns[self.current]
    }

    fn select(&mut self, index: usize) {
        if index != self.current && index < self.guns.len() {
            self.current = index;
            self.reloading = None;
            self.burst_left = 0;
            self.charge = 0.0;
            self.cooldown = 0.15;
        }
    }

    /// For a charging gun: how much of a full charge is built up, and the cells
    /// it would burn if loosed now (at least one).
    pub fn charged(&self) -> Option<(f32, u32)> {
        let FireMode::Charge { cell_time, .. } = self.gun().kind.stats().mode else { return None };
        let magazine = self.gun().kind.stats().magazine;
        (self.charge > 0.0).then(|| {
            let cells = ((self.charge / cell_time).ceil() as u32).clamp(1, magazine);
            (self.charge / (cell_time * magazine as f32), cells)
        })
    }

    fn start_reload(&mut self) {
        let gun = self.gun();
        let has_spare = gun.spare.is_none_or(|s| s > 0);
        if self.reloading.is_none() && gun.ammo < gun.kind.stats().magazine && has_spare {
            self.reloading = Some(gun.kind.stats().reload_time);
        }
    }
}

/// The gun sprite in a pirate's hands. Purely visual; exists on every machine.
#[derive(Component)]
struct HeldGun {
    kind: Option<GunKind>,
    kick: f32,
    /// `Arsenal::shots` last frame; a change triggers recoil for any pirate.
    last_shots: u32,
}

/// The orb swelling at the muzzle of a charging gun.
#[derive(Component)]
struct ChargeGlow;

#[derive(Component)]
struct Crosshair;

#[derive(Component)]
struct WeaponIcon;

#[derive(Component)]
struct WeaponName;

#[derive(Component)]
struct AmmoText;

#[derive(Component)]
struct SpareText;

/// The magazine, or the reload's progress while reloading.
#[derive(Component)]
struct MagazineBar;

/// The panel around the weapon readout; its top edge takes the gun's colour.
#[derive(Component)]
struct WeaponPanel;

/// One of the carried guns in the strip along the top of the weapon panel.
#[derive(Component)]
struct WeaponSlot(usize);

fn can_act(player: &Player, health: &Health) -> bool {
    player.status == PirateStatus::Active && !health.is_dead()
}

fn switch_gun(mut players: Query<(&Player, &Health, &Controls, &mut Arsenal)>) {
    for (player, health, controls, mut arsenal) in &mut players {
        if !can_act(player, health) {
            continue;
        }
        if let Some(slot) = controls.slot {
            arsenal.select(slot as usize);
        }
        let count = arsenal.guns.len();
        match controls.cycle {
            1 => {
                let next = (arsenal.current + 1) % count;
                arsenal.select(next);
            }
            -1 => {
                let prev = (arsenal.current + count - 1) % count;
                arsenal.select(prev);
            }
            _ => {}
        }
    }
}

fn reload(time: Res<Time>, mut players: Query<(&Player, &Health, &Controls, &mut Arsenal)>) {
    for (player, health, controls, mut arsenal) in &mut players {
        if !can_act(player, health) {
            continue;
        }
        if controls.reload {
            arsenal.start_reload();
        }
        if let Some(left) = arsenal.reloading {
            let left = left - time.delta_secs();
            if left <= 0.0 {
                let current = arsenal.current;
                let gun = &mut arsenal.guns[current];
                let wanted = gun.kind.stats().magazine - gun.ammo;
                let loaded = gun.spare.map_or(wanted, |s| s.min(wanted));
                gun.ammo += loaded;
                if let Some(spare) = &mut gun.spare {
                    *spare -= loaded;
                }
                arsenal.reloading = None;
            } else {
                arsenal.reloading = Some(left);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fire(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut rng: ResMut<Rng>,
    mut noise: ResMut<Noise>,
    mut alarm: ResMut<Alarm>,
    mut players: Query<(&Player, &Health, &Controls, &Transform, &Aim, &mut Arsenal, &mut Perks, &Boons)>,
    mut enemies: Query<(Entity, &Transform, &mut Health, &Hurtbox, &mut Knockback, &Team), (With<Enemy>, Without<Player>)>,
    riot_shields: Query<&RiotShield>,
) {
    let dt = time.delta_secs();
    for (player, health, controls, transform, aim, mut arsenal, mut perks, boons) in &mut players {
        if arsenal.cooldown > 0.0 {
            arsenal.cooldown = (arsenal.cooldown - dt).max(0.0);
        }
        // A Bulwark needs both hands for the riot shield.
        let shielding = riot_shields.iter().any(|s| s.owner == player.owner);
        if !can_act(player, health) || shielding {
            arsenal.burst_left = 0;
            arsenal.charge = 0.0;
            continue;
        }

        let stats = arsenal.gun().kind.stats();
        let infinite = perks.has(PerkKind::Overdrive);
        // Rounds the shot burns: one, or a charging gun's cells.
        let mut cells = 1;
        if let FireMode::Charge { cell_time, .. } = stats.mode {
            // Can't charge past what's in the magazine.
            let loaded = if infinite { stats.magazine } else { arsenal.gun().ammo };
            if arsenal.reloading.is_some() || loaded == 0 {
                arsenal.charge = 0.0;
                if loaded == 0 && controls.fire_pressed {
                    arsenal.start_reload();
                }
                continue;
            }
            if controls.fire && arsenal.cooldown <= 0.0 {
                arsenal.charge = (arsenal.charge + dt).min(cell_time * loaded as f32);
                continue;
            }
            let Some((_, charged)) = arsenal.charged() else { continue };
            cells = charged.min(loaded);
            arsenal.charge = 0.0;
        } else if arsenal.burst_left > 0 {
            // The rest of a burst fires on its own.
            arsenal.burst_timer -= dt;
            if arsenal.burst_timer > 0.0 {
                continue;
            }
        } else {
            let trigger = if stats.automatic { controls.fire } else { controls.fire_pressed };
            if !trigger || arsenal.cooldown > 0.0 || arsenal.reloading.is_some() {
                continue;
            }
        }
        if arsenal.gun().ammo == 0 && !infinite {
            arsenal.burst_left = 0;
            arsenal.start_reload();
            continue;
        }

        // A Bulwark nearby makes every shot count for more.
        let damage = stats.damage * boons.damage();
        let ricochet = if player.passive == Passive::Ricochet { RICOCHET_CHANCE } else { 0.0 };
        let origin = transform.translation.truncate();
        let muzzle = origin + HAND + aim.dir * (HOLD_DISTANCE + BARREL);
        let aim_angle = aim.dir.to_angle();

        match stats.mode {
            FireMode::Shots => {
                for i in 0..stats.pellets {
                    let (angle, speed) = if stats.pellets > 1 {
                        // Fan pellets evenly across the cone, with a little jitter.
                        let t = i as f32 / (stats.pellets - 1) as f32 - 0.5;
                        let jitter = rng.signed() * 0.5 / stats.pellets as f32;
                        (
                            aim_angle + (t + jitter) * stats.spread,
                            stats.speed * (1.0 + rng.signed() * 0.08),
                        )
                    } else {
                        (aim_angle + rng.signed() * stats.spread / 2.0, stats.speed)
                    };

                    spawn_projectile(
                        &mut commands,
                        Projectile {
                            team: Team::Player,
                            velocity: Vec2::from_angle(angle) * speed,
                            remaining: stats.range,
                            damage,
                            knockback: stats.knockback,
                            pierce: stats.pierce,
                            color: stats.color,
                            hits: Vec::new(),
                            ricochet,
                        },
                        stats.look,
                        muzzle,
                    );
                }
                fx(&mut commands, FxKind::MuzzleFlash, muzzle, aim_angle, stats.color);
            }

            FireMode::Arc { targets } => {
                let mut in_reach: Vec<(f32, Entity)> = enemies
                    .iter()
                    .filter(|(_, t, h, hurtbox, _, team)| {
                        let pos = t.translation.truncate();
                        **team == Team::Enemy && !h.is_dead() && pos.distance(origin) <= stats.range + hurtbox.0 && grid.line_of_sight(origin, pos)
                    })
                    .map(|(e, t, ..)| (t.translation.truncate().distance(origin), e))
                    .collect();
                if in_reach.is_empty() {
                    // Nothing to jump to: the charge crackles out into the air,
                    // costing a round like any other shot.
                    let end = muzzle + Vec2::from_angle(aim_angle + rng.signed() * 0.4) * stats.range * 0.3;
                    fx(&mut commands, FxKind::Zap(end), muzzle, 0.0, stats.color.with_alpha(0.5));
                } else {
                    in_reach.sort_by(|a, b| a.0.total_cmp(&b.0));
                    in_reach.truncate(targets);
                    let share = damage / in_reach.len() as f32;
                    for &(_, target) in &in_reach {
                        let Ok((_, t, mut h, _, mut knockback, _)) = enemies.get_mut(target) else { continue };
                        let pos = t.translation.truncate();
                        if h.hurt(share) {
                            knockback.0 += (pos - origin).normalize_or_zero() * stats.knockback;
                            fx(&mut commands, FxKind::Zap(pos), muzzle, 0.0, stats.color);
                        }
                    }
                }
            }

            FireMode::Lob { burst, gap, radius, scatter } => {
                if arsenal.burst_left == 0 {
                    arsenal.burst_left = burst;
                }
                arsenal.burst_left -= 1;
                arsenal.burst_timer = gap;

                let reach = (aim.target - origin).clamp_length_max(stats.range);
                let spread = Vec2::from_angle(rng.unit() * std::f32::consts::TAU) * rng.unit().sqrt() * scatter;
                let to = landing_spot(&grid, muzzle, origin + reach + spread);
                let flight = (muzzle.distance(to) / stats.speed).max(0.3);
                spawn_grenade(
                    &mut commands,
                    Grenade {
                        from: muzzle,
                        to,
                        age: 0.0,
                        flight,
                        damage,
                        radius,
                        knockback: stats.knockback,
                        color: stats.color,
                    },
                );
                fx(&mut commands, FxKind::MuzzleFlash, muzzle, aim_angle, stats.color);
            }

            FireMode::Charge { radius, arc_damage, arc_reach, .. } => {
                // Everything scales with the share of the magazine it burns.
                let power = cells as f32 / stats.magazine as f32;
                spawn_nova(
                    &mut commands,
                    NovaOrb {
                        velocity: aim.dir * stats.speed,
                        remaining: stats.range,
                        damage: damage * power,
                        radius: radius * (0.5 + 0.5 * power),
                        knockback: stats.knockback * power,
                        arc_damage: arc_damage * boons.damage(),
                        arc_reach: arc_reach * (0.5 + 0.5 * power),
                        arc_timer: 0.0,
                        color: stats.color,
                    },
                    muzzle,
                    0.5 + 0.5 * power,
                );
                fx(&mut commands, FxKind::MuzzleFlash, muzzle, aim_angle, stats.color);
            }
        }

        let current = arsenal.current;
        // A Gunner in the crew sometimes spares a round.
        let spent = (0..cells)
            .filter(|_| !(boons.supplied && rng.unit() < SUPPLY_CHANCE))
            .count() as u32;
        if !infinite {
            arsenal.guns[current].ammo -= spent;
        }
        arsenal.shots = arsenal.shots.wrapping_add(1);
        arsenal.cooldown = stats.cooldown;
        // Muzzle flash gives the cloak away.
        if perks.has(PerkKind::Cloak) {
            perks.end(PerkKind::Cloak);
        }
        noise.0.push(origin);
        alarm.alert();

        if arsenal.gun().ammo == 0 && arsenal.burst_left == 0 {
            arsenal.start_reload();
        }
    }
}

/// Where a grenade thrown from `from` toward `to` comes down: short of the first wall.
fn landing_spot(grid: &RoomGrid, from: Vec2, to: Vec2) -> Vec2 {
    let steps = (from.distance(to) / 4.0).ceil().max(1.0) as u32;
    let mut last = from;
    for i in 1..=steps {
        let p = from.lerp(to, i as f32 / steps as f32);
        if grid.is_solid_at(p) {
            break;
        }
        last = p;
    }
    last
}

fn equip_held_gun(mut commands: Commands, assets: Res<AssetServer>, new: Query<Entity, Added<Arsenal>>) {
    for entity in &new {
        commands.entity(entity).with_child((
            HeldGun { kind: None, kick: 0.0, last_shots: 0 },
            Sprite::default(),
            Transform::default(),
            children![(
                ChargeGlow,
                Sprite::from_image(assets.load(ProjectileLook::NovaOrb.sprite())),
                Transform::from_xyz(BARREL, 0.0, 0.1),
                Visibility::Hidden,
            )],
        ));
    }
}

/// Grows the orb at a charging gun's muzzle with the charge, throbbing once full.
fn draw_charge_glows(
    time: Res<Time>,
    players: Query<&Arsenal>,
    held: Query<&ChildOf, With<HeldGun>>,
    mut glows: Query<(&ChildOf, &mut Visibility, &mut Transform, &mut Sprite), With<ChargeGlow>>,
) {
    let t = time.elapsed_secs();
    for (parent, mut visibility, mut transform, mut sprite) in &mut glows {
        let charged = held
            .get(parent.parent())
            .ok()
            .and_then(|gun| players.get(gun.parent()).ok())
            .and_then(Arsenal::charged);
        let Some((frac, _)) = charged else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let throb = if frac >= 1.0 { 0.12 * (t * 14.0).sin() } else { 0.05 * (t * 9.0).sin() };
        transform.scale = Vec3::splat(0.25 + 0.6 * frac + throb);
        sprite.color = Color::WHITE.with_alpha(0.6 + 0.4 * frac);
    }
}

fn aim_held_guns(
    time: Res<Time>,
    assets: Res<AssetServer>,
    local_aim: Res<LocalAim>,
    players: Query<(&Player, &Aim, &Arsenal, Option<&Perks>, Has<LocalPlayer>)>,
    riot_shields: Query<&RiotShield>,
    mut held: Query<(&ChildOf, &mut HeldGun, &mut Sprite, &mut Transform)>,
    mut crosshair: Single<&mut Transform, (With<Crosshair>, Without<HeldGun>)>,
) {
    crosshair.translation = local_aim.target.extend(50.0);

    for (parent, mut gun, mut sprite, mut transform) in &mut held {
        let Ok((player, aim, arsenal, perks, local)) = players.get(parent.parent()) else { continue };
        let current = arsenal.gun();
        if gun.kind != Some(current.kind) {
            gun.kind = Some(current.kind);
            gun.last_shots = arsenal.shots;
            sprite.image = assets.load(current.kind.stats().sprite);
        }
        if arsenal.shots != gun.last_shots {
            gun.kick = current.kind.kick();
        }
        gun.last_shots = arsenal.shots;
        let cloaked = perks.is_some_and(|p| p.has(PerkKind::Cloak));
        // Put away while both hands hold a riot shield.
        let shielding = riot_shields.iter().any(|s| s.owner == player.owner);
        let alpha = match (shielding, cloaked) {
            (true, _) => 0.0,
            (false, true) => 0.3,
            (false, false) => 1.0,
        };
        sprite.color = Color::WHITE.with_alpha(alpha);
        gun.kick = (gun.kick - gun.kick * 18.0 * time.delta_secs()).max(0.0);

        let dir = if local { local_aim.dir } else { aim.dir };
        let pos = HAND + dir * (HOLD_DISTANCE - gun.kick);
        // Tuck the gun behind the pirate when aiming upward.
        let z = if dir.y > 0.4 { -0.2 } else { 0.2 };
        transform.translation = pos.extend(z);
        transform.rotation = Quat::from_rotation_z(dir.to_angle());
        sprite.flip_y = dir.x < 0.0;
    }
}

fn spawn_crosshair(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        Hud,
        Crosshair,
        Sprite::from_image(assets.load("sprites/ui/crosshair.png")),
        Transform::from_xyz(0.0, 0.0, 50.0),
    ));
}

fn spawn_weapon_hud(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Weapon)).with_children(|slot| {
        slot.spawn((
            WeaponPanel,
            hud::panel(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                min_width: Val::Px(190.0),
                ..default()
            }),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|strip| {
                    for i in 0..GunKind::ALL.len().min(4) {
                        strip.spawn((
                            WeaponSlot(i),
                            Node {
                                width: Val::Px(36.0),
                                height: Val::Px(20.0),
                                border: UiRect::all(Val::Px(1.0)),
                                padding: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor::all(hud::EDGE),
                            ImageNode::default(),
                        ));
                    }
                });
            panel.spawn((
                WeaponIcon,
                ImageNode::default(),
                Node {
                    width: Val::Px(96.0),
                    height: Val::Px(48.0),
                    ..default()
                },
            ));
            panel.spawn((WeaponName, hud::text("", 16.0, Color::WHITE)));
            panel.spawn(Node {
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Baseline,
                ..default()
            })
            .with_children(|row| {
                row.spawn((AmmoText, hud::text("", 26.0, Color::WHITE)));
                row.spawn((SpareText, hud::text("", 14.0, hud::DIM)));
            });
            panel.spawn((MagazineBar, hud::meter(168.0, 5.0, Color::WHITE)));
        });
    });
}

fn update_weapon_hud(
    assets: Res<AssetServer>,
    arsenal: Option<Single<(&Player, &Arsenal, &Perks), With<LocalPlayer>>>,
    riot_shields: Query<&RiotShield>,
    mut panel: Single<&mut BorderColor, (With<WeaponPanel>, Without<WeaponSlot>)>,
    mut strip: Query<(&WeaponSlot, &mut Node, &mut ImageNode, &mut BorderColor), Without<WeaponIcon>>,
    mut icon: Single<&mut ImageNode, With<WeaponIcon>>,
    name: Single<(&mut Text, &mut TextColor), (With<WeaponName>, Without<AmmoText>, Without<SpareText>)>,
    ammo: Single<(&mut Text, &mut TextColor), (With<AmmoText>, Without<SpareText>)>,
    mut spare: Single<&mut Text, With<SpareText>>,
    mut magazine: Single<&mut Meter, With<MagazineBar>>,
) {
    let Some(arsenal) = arsenal else { return };
    let (player, arsenal, perks) = *arsenal;
    let shielding = riot_shields.iter().any(|s| s.owner == player.owner);
    let gun = arsenal.gun();
    let stats = gun.kind.stats();

    let image = assets.load(stats.sprite);
    if icon.image != image {
        icon.image = image;
    }
    panel.top = stats.color;
    for (slot, mut node, mut image, mut border) in &mut strip {
        let Some(carried) = arsenal.guns.get(slot.0) else {
            node.display = Display::None;
            continue;
        };
        node.display = Display::Flex;
        let handle = assets.load(carried.kind.stats().sprite);
        if image.image != handle {
            image.image = handle;
        }
        let current = slot.0 == arsenal.current;
        let empty = carried.ammo == 0 && carried.spare == Some(0);
        image.color = Color::WHITE.with_alpha(if current { 1.0 } else if empty { 0.2 } else { 0.45 });
        *border = BorderColor::all(if current { stats.color } else { hud::EDGE });
    }

    let (mut name_text, mut name_color) = name.into_inner();
    name_text.set_if_neq(Text(format!("{}  {}", arsenal.current + 1, stats.name.to_uppercase())));
    name_color.0 = stats.color;

    let (mut ammo_text, mut ammo_color) = ammo.into_inner();
    let red = Color::srgb(1.0, 0.35, 0.35);
    let (big, small, color, frac, bar) = if let (false, Some((frac, cells))) = (shielding, arsenal.charged()) {
        let label = if frac >= 1.0 { "FULL CHARGE" } else { "CHARGING" };
        (format!("{cells} / {}", stats.magazine), label.into(), stats.color, frac, stats.color)
    } else {
        match arsenal.reloading {
            _ if shielding => ("SHIELD UP".into(), String::new(), RIOT_COLOR, 1.0, RIOT_COLOR),
            _ if perks.has(PerkKind::Overdrive) => {
                let color = PerkKind::Overdrive.color();
                ("INFINITE".into(), String::new(), color, 1.0, color)
            }
            Some(left) => {
                let amber = Color::srgb(1.0, 0.8, 0.3);
                ("RELOADING".into(), format!("{left:.1}s"), amber, 1.0 - left / stats.reload_time, amber)
            }
            None if gun.ammo == 0 && gun.spare == Some(0) => ("EMPTY".into(), "no spare ammo".into(), red, 0.0, red),
            None => {
                let spare = gun.spare.map_or("/ INF".to_string(), |s| format!("/ {s}"));
                let frac = gun.ammo as f32 / stats.magazine as f32;
                let color = if gun.ammo == 0 { red } else { Color::WHITE };
                let bar = if frac <= 0.25 { red } else { Color::srgba(1.0, 1.0, 1.0, 0.85) };
                (gun.ammo.to_string(), spare, color, frac, bar)
            }
        }
    };
    ammo_text.set_if_neq(Text(big));
    ammo_color.0 = color;
    spare.set_if_neq(Text(small));
    magazine.frac = frac;
    magazine.color = bar;
}
