use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{
    FxKind, Grenade, Health, Hurtbox, Knockback, Noise, Projectile, ProjectileLook, Team, fx, spawn_grenade,
    spawn_projectile,
};
use crate::enemies::Enemy;
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
            .add_systems(Update, ((equip_held_gun, aim_held_guns).chain(), update_weapon_hud));
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
}

pub struct GunStats {
    pub name: &'static str,
    sprite: &'static str,
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

impl GunKind {
    pub const ALL: [GunKind; 6] = [
        Self::BlasterPistol,
        Self::ScatterCannon,
        Self::PulseRifle,
        Self::RailLance,
        Self::ArcCoil,
        Self::GrenadeLauncher,
    ];

    pub fn stats(self) -> &'static GunStats {
        match self {
            Self::BlasterPistol => &BLASTER_PISTOL,
            Self::ScatterCannon => &SCATTER_CANNON,
            Self::PulseRifle => &PULSE_RIFLE,
            Self::RailLance => &RAIL_LANCE,
            Self::ArcCoil => &ARC_COIL,
            Self::GrenadeLauncher => &GRENADE_LAUNCHER,
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
            self.cooldown = 0.15;
        }
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

#[derive(Component)]
struct Crosshair;

#[derive(Component)]
struct WeaponIcon;

#[derive(Component)]
struct WeaponName;

#[derive(Component)]
struct AmmoText;

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
    mut players: Query<(&Player, &Health, &Controls, &Transform, &Aim, &mut Arsenal, &mut Perks)>,
    mut enemies: Query<(Entity, &Transform, &mut Health, &Hurtbox, &mut Knockback), (With<Enemy>, Without<Player>)>,
) {
    let dt = time.delta_secs();
    for (player, health, controls, transform, aim, mut arsenal, mut perks) in &mut players {
        if arsenal.cooldown > 0.0 {
            arsenal.cooldown = (arsenal.cooldown - dt).max(0.0);
        }
        if !can_act(player, health) {
            arsenal.burst_left = 0;
            continue;
        }

        let stats = arsenal.gun().kind.stats();
        if arsenal.burst_left > 0 {
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
        let infinite = perks.has(PerkKind::Overdrive);
        if arsenal.gun().ammo == 0 && !infinite {
            arsenal.burst_left = 0;
            arsenal.start_reload();
            continue;
        }

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
                            damage: stats.damage,
                            knockback: stats.knockback,
                            pierce: stats.pierce,
                            color: stats.color,
                            hits: Vec::new(),
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
                    .filter(|(_, t, h, hurtbox, _)| {
                        let pos = t.translation.truncate();
                        !h.is_dead() && pos.distance(origin) <= stats.range + hurtbox.0 && grid.line_of_sight(origin, pos)
                    })
                    .map(|(e, t, ..)| (t.translation.truncate().distance(origin), e))
                    .collect();
                if in_reach.is_empty() {
                    // Nothing to jump to: a harmless crackle that costs nothing.
                    let end = muzzle + Vec2::from_angle(aim_angle + rng.signed() * 0.4) * stats.range * 0.3;
                    fx(&mut commands, FxKind::Zap(end), muzzle, 0.0, stats.color.with_alpha(0.5));
                    arsenal.cooldown = 0.25;
                    continue;
                }
                in_reach.sort_by(|a, b| a.0.total_cmp(&b.0));
                in_reach.truncate(targets);
                let share = stats.damage / in_reach.len() as f32;
                for &(_, target) in &in_reach {
                    let Ok((_, t, mut h, _, mut knockback)) = enemies.get_mut(target) else { continue };
                    let pos = t.translation.truncate();
                    if h.hurt(share) {
                        knockback.0 += (pos - origin).normalize_or_zero() * stats.knockback;
                        fx(&mut commands, FxKind::Zap(pos), muzzle, 0.0, stats.color);
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
                        damage: stats.damage,
                        radius,
                        knockback: stats.knockback,
                        color: stats.color,
                    },
                );
                fx(&mut commands, FxKind::MuzzleFlash, muzzle, aim_angle, stats.color);
            }
        }

        let current = arsenal.current;
        if !infinite {
            arsenal.guns[current].ammo -= 1;
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

fn equip_held_gun(mut commands: Commands, new: Query<Entity, Added<Arsenal>>) {
    for entity in &new {
        commands.entity(entity).with_child((
            HeldGun { kind: None, kick: 0.0, last_shots: 0 },
            Sprite::default(),
            Transform::default(),
        ));
    }
}

fn aim_held_guns(
    time: Res<Time>,
    assets: Res<AssetServer>,
    local_aim: Res<LocalAim>,
    players: Query<(&Aim, &Arsenal, Option<&Perks>, Has<LocalPlayer>)>,
    mut held: Query<(&ChildOf, &mut HeldGun, &mut Sprite, &mut Transform)>,
    mut crosshair: Single<&mut Transform, (With<Crosshair>, Without<HeldGun>)>,
) {
    crosshair.translation = local_aim.target.extend(50.0);

    for (parent, mut gun, mut sprite, mut transform) in &mut held {
        let Ok((aim, arsenal, perks, local)) = players.get(parent.parent()) else { continue };
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
        sprite.color = Color::WHITE.with_alpha(if cloaked { 0.3 } else { 1.0 });
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

fn spawn_weapon_hud(mut commands: Commands) {
    commands.spawn((
        Hud,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(14.0),
            right: Val::Px(18.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 0.75)),
        children![
            (
                WeaponIcon,
                ImageNode::default(),
                Node {
                    width: Val::Px(96.0),
                    height: Val::Px(48.0),
                    ..default()
                },
            ),
            (
                WeaponName,
                Text::new(""),
                TextFont::from_font_size(20.0),
                TextColor(Color::WHITE),
            ),
            (
                AmmoText,
                Text::new(""),
                TextFont::from_font_size(18.0),
                TextColor(Color::WHITE),
            ),
        ],
    ));
}

fn update_weapon_hud(
    assets: Res<AssetServer>,
    arsenal: Option<Single<(&Arsenal, &Perks), With<LocalPlayer>>>,
    mut icon: Single<&mut ImageNode, With<WeaponIcon>>,
    mut name: Single<(&mut Text, &mut TextColor), (With<WeaponName>, Without<AmmoText>)>,
    mut ammo: Single<(&mut Text, &mut TextColor), With<AmmoText>>,
) {
    let Some(arsenal) = arsenal else { return };
    let (arsenal, perks) = *arsenal;
    let gun = arsenal.gun();
    let stats = gun.kind.stats();

    let image = assets.load(stats.sprite);
    if icon.image != image {
        icon.image = image;
    }

    let (name_text, name_color) = &mut *name;
    name_text.0 = format!("[{}] {}", arsenal.current + 1, stats.name);
    name_color.0 = stats.color;

    let (ammo_text, ammo_color) = &mut *ammo;
    match arsenal.reloading {
        _ if perks.has(PerkKind::Overdrive) => {
            ammo_text.0 = "INFINITE".into();
            ammo_color.0 = PerkKind::Overdrive.color();
        }
        Some(left) => {
            ammo_text.0 = format!("RELOADING  {left:.1}s");
            ammo_color.0 = Color::srgb(1.0, 0.8, 0.3);
        }
        None if gun.ammo == 0 && gun.spare == Some(0) => {
            ammo_text.0 = "OUT OF AMMO".into();
            ammo_color.0 = Color::srgb(1.0, 0.35, 0.35);
        }
        None => {
            let spare = gun.spare.map_or("INF".to_string(), |s| s.to_string());
            ammo_text.0 = format!("{} / {spare}", gun.ammo);
            ammo_color.0 = if gun.ammo == 0 {
                Color::srgb(1.0, 0.35, 0.35)
            } else {
                Color::WHITE
            };
        }
    }
}

