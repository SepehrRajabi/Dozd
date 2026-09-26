use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::combat::{FxKind, Health, Noise, Projectile, ProjectileLook, Team, fx, spawn_projectile};
use crate::mission::Alarm;
use crate::net::{LocalPlayer, authority};
use crate::player::{Aim, Controls, LocalAim, Player, PirateStatus};
use crate::{GameState, Hud, Rng};

/// Where the gun is held relative to the player's origin.
const HAND: Vec2 = Vec2::new(0.0, -3.0);
const HOLD_DISTANCE: f32 = 5.0;
/// Distance from the gun's centre to its muzzle.
const BARREL: f32 = 8.0;

pub struct GunsPlugin;

impl Plugin for GunsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_crosshair, spawn_weapon_hud))
            .add_systems(
                Update,
                (
                    (switch_gun, reload, fire)
                        .chain()
                        .run_if(in_state(GameState::Playing))
                        .run_if(authority),
                    (equip_held_gun, aim_held_guns).chain(),
                    update_weapon_hud,
                ),
            );
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum GunKind {
    BlasterPistol,
    ScatterCannon,
    PulseRifle,
    RailLance,
}

pub struct GunStats {
    pub name: &'static str,
    sprite: &'static str,
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
}

const BLASTER_PISTOL: GunStats = GunStats {
    name: "Blaster Pistol",
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
    reload_time: 0.9,
};

const SCATTER_CANNON: GunStats = GunStats {
    name: "Scatter Cannon",
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
    reload_time: 1.4,
};

const PULSE_RIFLE: GunStats = GunStats {
    name: "Pulse Rifle",
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
    reload_time: 1.6,
};

const RAIL_LANCE: GunStats = GunStats {
    name: "Rail Lance",
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
    reload_time: 2.0,
};

impl GunKind {
    pub const ALL: [GunKind; 4] = [
        Self::BlasterPistol,
        Self::ScatterCannon,
        Self::PulseRifle,
        Self::RailLance,
    ];

    pub fn stats(self) -> &'static GunStats {
        match self {
            Self::BlasterPistol => &BLASTER_PISTOL,
            Self::ScatterCannon => &SCATTER_CANNON,
            Self::PulseRifle => &PULSE_RIFLE,
            Self::RailLance => &RAIL_LANCE,
        }
    }

    /// Recoil distance in pixels.
    fn kick(self) -> f32 {
        match self {
            Self::BlasterPistol => 1.5,
            Self::ScatterCannon => 4.0,
            Self::PulseRifle => 1.0,
            Self::RailLance => 5.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Gun {
    kind: GunKind,
    ammo: u32,
}

/// The guns a pirate is carrying and the state of the one in hand.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Arsenal {
    guns: Vec<Gun>,
    current: usize,
    cooldown: f32,
    /// Seconds left until the reload finishes.
    reloading: Option<f32>,
}

impl Arsenal {
    pub fn new(kinds: &[GunKind]) -> Self {
        Self {
            guns: kinds
                .iter()
                .map(|&kind| Gun { kind, ammo: kind.stats().magazine })
                .collect(),
            current: 0,
            cooldown: 0.0,
            reloading: None,
        }
    }

    fn gun(&self) -> &Gun {
        &self.guns[self.current]
    }

    fn select(&mut self, index: usize) {
        if index != self.current && index < self.guns.len() {
            self.current = index;
            self.reloading = None;
            self.cooldown = 0.15;
        }
    }

    fn start_reload(&mut self) {
        let gun = self.gun();
        if self.reloading.is_none() && gun.ammo < gun.kind.stats().magazine {
            self.reloading = Some(gun.kind.stats().reload_time);
        }
    }
}

/// The gun sprite in a pirate's hands. Purely visual; exists on every machine.
#[derive(Component)]
struct HeldGun {
    kind: Option<GunKind>,
    kick: f32,
    /// Ammo last frame, so a drop (a shot) triggers recoil for any pirate.
    last_ammo: u32,
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
                gun.ammo = gun.kind.stats().magazine;
                arsenal.reloading = None;
            } else {
                arsenal.reloading = Some(left);
            }
        }
    }
}

fn fire(
    mut commands: Commands,
    time: Res<Time>,
    mut rng: ResMut<Rng>,
    mut noise: ResMut<Noise>,
    mut alarm: ResMut<Alarm>,
    mut players: Query<(&Player, &Health, &Controls, &Transform, &Aim, &mut Arsenal)>,
) {
    for (player, health, controls, transform, aim, mut arsenal) in &mut players {
        if arsenal.cooldown > 0.0 {
            arsenal.cooldown = (arsenal.cooldown - time.delta_secs()).max(0.0);
        }
        if !can_act(player, health) {
            continue;
        }

        let stats = arsenal.gun().kind.stats();
        let trigger = if stats.automatic { controls.fire } else { controls.fire_pressed };
        if !trigger || arsenal.cooldown > 0.0 || arsenal.reloading.is_some() {
            continue;
        }
        if arsenal.gun().ammo == 0 {
            arsenal.start_reload();
            continue;
        }

        let current = arsenal.current;
        arsenal.guns[current].ammo -= 1;
        arsenal.cooldown = stats.cooldown;

        let origin = transform.translation.truncate();
        noise.0.push(origin);
        alarm.alert();

        let muzzle = origin + HAND + aim.dir * (HOLD_DISTANCE + BARREL);
        let aim_angle = aim.dir.to_angle();

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

        if arsenal.gun().ammo == 0 {
            arsenal.start_reload();
        }
    }
}

fn equip_held_gun(mut commands: Commands, new: Query<Entity, Added<Arsenal>>) {
    for entity in &new {
        commands.entity(entity).with_child((
            HeldGun { kind: None, kick: 0.0, last_ammo: 0 },
            Sprite::default(),
            Transform::default(),
        ));
    }
}

fn aim_held_guns(
    time: Res<Time>,
    assets: Res<AssetServer>,
    local_aim: Res<LocalAim>,
    players: Query<(&Aim, &Arsenal, Has<LocalPlayer>)>,
    mut held: Query<(&ChildOf, &mut HeldGun, &mut Sprite, &mut Transform)>,
    mut crosshair: Single<&mut Transform, (With<Crosshair>, Without<HeldGun>)>,
) {
    crosshair.translation = local_aim.target.extend(50.0);

    for (parent, mut gun, mut sprite, mut transform) in &mut held {
        let Ok((aim, arsenal, local)) = players.get(parent.parent()) else { continue };
        let current = arsenal.gun();
        if gun.kind != Some(current.kind) {
            gun.kind = Some(current.kind);
            gun.last_ammo = current.ammo;
            sprite.image = assets.load(current.kind.stats().sprite);
        }
        if current.ammo < gun.last_ammo {
            gun.kick = current.kind.kick();
        }
        gun.last_ammo = current.ammo;
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
    arsenal: Option<Single<&Arsenal, With<LocalPlayer>>>,
    mut icon: Single<&mut ImageNode, With<WeaponIcon>>,
    mut name: Single<(&mut Text, &mut TextColor), (With<WeaponName>, Without<AmmoText>)>,
    mut ammo: Single<(&mut Text, &mut TextColor), With<AmmoText>>,
) {
    let Some(arsenal) = arsenal else { return };
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
        Some(left) => {
            ammo_text.0 = format!("RELOADING  {left:.1}s");
            ammo_color.0 = Color::srgb(1.0, 0.8, 0.3);
        }
        None => {
            ammo_text.0 = format!("{} / {}", gun.ammo, stats.magazine);
            ammo_color.0 = if gun.ammo == 0 {
                Color::srgb(1.0, 0.35, 0.35)
            } else {
                Color::WHITE
            };
        }
    }
}
