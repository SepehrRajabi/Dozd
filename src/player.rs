use std::iter;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

use crate::combat::{Fade, Health, Hurtbox, Shake, Team};
use crate::guns::{Arsenal, GunKind};
use crate::net::{HOST_ID, LocalId, LocalPlayer, PlayerInput, authority};
use crate::mission::MissionStatus;
use crate::perks::{PerkKind, Perks};
use crate::room::{Heist, RoomGrid, TILE};
use crate::{GameState, Level, PIXEL_SCALE, Rng};

const SPEED: f32 = 75.0;
pub const MAX_HEALTH: f32 = 100.0;
pub const MAX_SHIELD: f32 = 50.0;
const DASH_SPEED: f32 = 280.0;
const DASH_TIME: f32 = 0.14;
pub const DASH_COOLDOWN: f32 = 1.1;
/// Seconds between afterimages while dashing.
const TRAIL_INTERVAL: f32 = 0.025;
const HITBOX_HALF: Vec2 = Vec2::new(5.0, 3.0);
/// The hitbox sits at the pirate's feet, not the sprite centre.
const FEET_OFFSET: Vec2 = Vec2::new(0.0, -5.0);
/// Where pirates who aren't on the current deck are parked, far off the map.
pub const OFF_DECK: Vec2 = Vec2::new(-100_000.0, 0.0);
const SLOT_COLORS: [Color; 4] = [
    Color::srgb(1.0, 0.6, 0.25),
    Color::srgb(0.4, 0.85, 1.0),
    Color::srgb(0.5, 1.0, 0.5),
    Color::srgb(1.0, 0.5, 0.85),
];

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalAim>()
            .add_observer(dress_player)
            .add_observer(on_client_joined)
            .add_observer(on_client_left)
            .add_systems(PreUpdate, receive_input.after(ServerSystems::Receive).run_if(authority))
            .add_systems(
                Update,
                (
                    gather_input.run_if(not(in_state(GameState::Menu))),
                    move_players.run_if(in_state(GameState::Playing)).run_if(authority),
                    animate_players,
                    dash_trails,
                    hide_other_decks,
                    follow_camera,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, clear_input_edges.run_if(authority));
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PirateStatus {
    Active,
    Extracted,
    /// Alive when the extraction left without them.
    LeftBehind,
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Player {
    /// `LocalId` of the machine controlling this pirate.
    pub owner: u64,
    /// Join order, used for the name tag and spawn spot.
    pub slot: u8,
    /// This pirate's own haul.
    pub credits: u32,
    pub items: u32,
    pub status: PirateStatus,
    /// Deck this pirate is on (0 = top). The crew only moves down together.
    pub deck: u8,
}

#[derive(Component)]
struct PlayerSprite {
    walk_time: f32,
}

/// Where the pirate is aiming, in world space.
#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Aim {
    pub dir: Vec2,
    pub target: Vec2,
}

#[derive(Component, Serialize, Deserialize, Clone, Default)]
pub struct Walking {
    moving: bool,
}

/// Short invulnerable burst of speed.
#[derive(Component, Serialize, Deserialize, Clone, Default)]
pub struct Dash {
    pub cooldown: f32,
    active: f32,
    dir: Vec2,
}

#[derive(Component, Default)]
struct DashTrail {
    timer: f32,
}

/// Latest input for a pirate, applied by the host. Edge flags (`fire_pressed`,
/// `dash`, ...) stay set until the end of the frame so no press is lost when
/// several input messages arrive between frames.
#[derive(Component, Default)]
pub struct Controls {
    movement: Vec2,
    aim: Vec2,
    pub fire: bool,
    pub fire_pressed: bool,
    dash: bool,
    pub reload: bool,
    pub slot: Option<u8>,
    pub cycle: i8,
    pub interact: bool,
}

/// This machine's aim, used for the crosshair and the local pirate's gun so they
/// respond instantly instead of waiting for the host.
#[derive(Resource)]
pub struct LocalAim {
    pub target: Vec2,
    pub dir: Vec2,
}

impl Default for LocalAim {
    fn default() -> Self {
        Self { target: Vec2::ZERO, dir: Vec2::X }
    }
}

fn spawn_player(commands: &mut Commands, owner: u64, slot: u8, pos: Vec2, deck: u8) {
    commands.spawn((
        Level,
        Replicated,
        Player {
            owner,
            slot,
            credits: 0,
            items: 0,
            status: PirateStatus::Active,
            deck,
        },
        Team::Player,
        Health::new(MAX_HEALTH, 0.5).with_shield(MAX_SHIELD),
        Hurtbox(6.0),
        Walking::default(),
        Dash::default(),
        Aim { dir: Vec2::X, target: pos + Vec2::X * 32.0 },
        Arsenal::new(&GunKind::ALL),
        Perks::default(),
        Controls::default(),
        Transform::from_translation(pos.extend(10.0)),
    ));
}

/// Packs the crew around the arrival tile: first the 2x2 lift, then around it.
pub fn crew_spot(spawn: Vec2, index: usize) -> Vec2 {
    const SPOTS: [(f32, f32); 9] = [
        (0.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (1.0, -1.0),
        (-1.0, 0.0),
        (-1.0, -1.0),
        (0.0, 1.0),
        (1.0, 1.0),
        (0.5, -2.0),
    ];
    let (x, y) = SPOTS[index % SPOTS.len()];
    spawn + Vec2::new(x, y) * TILE
}

/// Spawns the host's pirate and one for every connected client at the start of a run.
pub fn spawn_crew(
    mut commands: Commands,
    grid: Res<RoomGrid>,
    heist: Res<Heist>,
    clients: Query<&NetworkId, With<AuthorizedClient>>,
) {
    let owners = iter::once(HOST_ID).chain(clients.iter().map(NetworkId::get));
    for (slot, owner) in owners.enumerate() {
        let pos = crew_spot(grid.crew_spawn, slot);
        spawn_player(&mut commands, owner, slot as u8, pos, heist.deck);
    }
}

fn on_client_joined(
    add: On<Add, AuthorizedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId>,
    players: Query<&Player>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    state: Res<State<GameState>>,
) {
    let (Ok(id), Some(grid)) = (ids.get(add.entity), grid) else { return };
    info!("client {} joined", id.get());
    // Mid-results joiners get a pirate when the next run starts.
    if *state.get() == GameState::Playing {
        let slot = players.iter().map(|p| p.slot + 1).max().unwrap_or(0);
        spawn_player(&mut commands, id.get(), slot, crew_spot(grid.crew_spawn, slot as usize), heist.deck);
    }
}

fn on_client_left(
    remove: On<Remove, ConnectedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId>,
    players: Query<(Entity, &Player)>,
) {
    let Ok(id) = ids.get(remove.entity) else { return };
    info!("client {} left", id.get());
    for (entity, player) in &players {
        if player.owner == id.get() {
            commands.entity(entity).despawn();
        }
    }
}

/// Adds sprites and local-only bits to a pirate, whether spawned here or replicated.
fn dress_player(
    add: On<Add, Player>,
    mut commands: Commands,
    assets: Res<AssetServer>,
    local: Res<LocalId>,
    players: Query<&Player>,
) {
    let Ok(player) = players.get(add.entity) else { return };
    let mut entity = commands.entity(add.entity);
    entity.insert((Visibility::default(), DashTrail::default())).with_children(|parent| {
        parent.spawn((
            Sprite::from_image(assets.load("sprites/shadow.png")),
            Transform::from_xyz(0.0, 0.0, -0.1),
        ));
        parent.spawn((PlayerSprite { walk_time: 0.0 }, Sprite::from_image(assets.load("sprites/player.png"))));
    });

    if player.owner == local.0 {
        entity.insert(LocalPlayer);
    } else {
        entity.with_child((
            Text2d::new(format!("P{}", player.slot + 1)),
            TextFont::from_font_size(6.0 * PIXEL_SCALE),
            TextColor(SLOT_COLORS[player.slot as usize % SLOT_COLORS.len()]),
            Transform::from_xyz(0.0, 13.0, 1.0).with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
        ));
    }
}

/// Reads this machine's keyboard and mouse and sends them to the host.
fn gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    player: Option<Single<&Transform, With<LocalPlayer>>>,
    mut local_aim: ResMut<LocalAim>,
    mut inputs: MessageWriter<PlayerInput>,
) {
    let (camera, camera_transform) = *camera;
    if let Some(target) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
    {
        local_aim.target = target;
    }
    if let Some(player) = player
        && let Some(dir) = (local_aim.target - player.translation.truncate()).try_normalize()
    {
        local_aim.dir = dir;
    }

    let axis = |neg: [KeyCode; 2], pos: [KeyCode; 2]| {
        keys.any_pressed(pos) as i8 as f32 - keys.any_pressed(neg) as i8 as f32
    };
    const SLOTS: [KeyCode; 6] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ];

    inputs.write(PlayerInput {
        movement: Vec2::new(
            axis([KeyCode::KeyA, KeyCode::ArrowLeft], [KeyCode::KeyD, KeyCode::ArrowRight]),
            axis([KeyCode::KeyS, KeyCode::ArrowDown], [KeyCode::KeyW, KeyCode::ArrowUp]),
        ),
        aim: local_aim.target,
        fire: mouse.pressed(MouseButton::Left),
        fire_pressed: mouse.just_pressed(MouseButton::Left),
        dash: keys.just_pressed(KeyCode::Space),
        reload: keys.just_pressed(KeyCode::KeyR),
        slot: SLOTS.iter().position(|&k| keys.just_pressed(k)).map(|i| i as u8),
        // Wheel down cycles forward.
        cycle: if scroll.delta.y < 0.0 {
            1
        } else if scroll.delta.y > 0.0 {
            -1
        } else {
            0
        },
        interact: keys.just_pressed(KeyCode::KeyE),
        restart: keys.just_pressed(KeyCode::Enter),
    });
}

fn receive_input(
    mut inputs: MessageReader<FromClient<PlayerInput>>,
    ids: Query<&NetworkId>,
    mut players: Query<(&Player, &mut Controls)>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for FromClient { client_id, message } in inputs.read() {
        let owner = match client_id {
            ClientId::Server => HOST_ID,
            ClientId::Client(entity) => match ids.get(*entity) {
                Ok(id) => id.get(),
                Err(_) => continue,
            },
        };
        if message.restart && *state.get() == GameState::Over {
            next_state.set(GameState::Playing);
        }
        let Some((_, mut controls)) = players.iter_mut().find(|(p, _)| p.owner == owner) else { continue };
        controls.movement = message.movement.clamp_length_max(1.0);
        controls.aim = message.aim;
        controls.fire = message.fire;
        controls.fire_pressed |= message.fire_pressed;
        controls.dash |= message.dash;
        controls.reload |= message.reload;
        controls.slot = message.slot.or(controls.slot);
        controls.cycle = (controls.cycle + message.cycle).clamp(-1, 1);
        controls.interact |= message.interact;
    }
}

fn clear_input_edges(mut controls: Query<&mut Controls>) {
    for mut c in &mut controls {
        c.fire_pressed = false;
        c.dash = false;
        c.reload = false;
        c.slot = None;
        c.cycle = 0;
        c.interact = false;
    }
}

fn move_players(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut players: Query<(&Player, &Controls, &mut Transform, &mut Walking, &mut Dash, &mut Health, &mut Aim)>,
) {
    let dt = time.delta_secs();
    for (player, controls, mut transform, mut walking, mut dash, mut health, mut aim) in &mut players {
        if health.is_dead() || player.status != PirateStatus::Active {
            if walking.moving {
                walking.moving = false;
            }
            continue;
        }

        let pos = transform.translation.truncate();
        aim.target = controls.aim;
        if let Some(dir) = (controls.aim - pos).try_normalize() {
            aim.dir = dir;
        }

        let moving = controls.movement != Vec2::ZERO;
        if walking.moving != moving {
            walking.moving = moving;
        }

        dash.cooldown = (dash.cooldown - dt).max(0.0);
        if controls.dash && dash.cooldown <= 0.0 {
            // Dash where you're moving; standing still, dash toward the aim.
            dash.dir = controls.movement.try_normalize().unwrap_or(aim.dir);
            dash.active = DASH_TIME;
            dash.cooldown = DASH_COOLDOWN;
            health.invulnerable = health.invulnerable.max(DASH_TIME + 0.04);
        }

        let step = if dash.active > 0.0 {
            dash.active = (dash.active - dt).max(0.0);
            dash.dir * DASH_SPEED * dt
        } else if moving {
            controls.movement.normalize() * SPEED * dt
        } else {
            continue;
        };

        let new = grid.slide(pos + FEET_OFFSET, step, HITBOX_HALF) - FEET_OFFSET;
        transform.translation.x = new.x;
        transform.translation.y = new.y;
    }
}

fn animate_players(
    time: Res<Time>,
    local_aim: Res<LocalAim>,
    players: Query<(&Walking, &Aim, &Health, &Dash, Option<&Perks>, Has<LocalPlayer>)>,
    mut sprites: Query<(&ChildOf, &mut PlayerSprite, &mut Transform, &mut Sprite)>,
) {
    for (parent, mut body, mut transform, mut sprite) in &mut sprites {
        let Ok((walking, aim, health, dash, perks, local)) = players.get(parent.parent()) else { continue };
        let facing = if local { local_aim.dir } else { aim.dir };
        sprite.flip_x = facing.x < 0.0;

        sprite.color = if health.is_dead() {
            Color::srgb(0.35, 0.3, 0.3)
        } else if health.flash > 0.0 && health.shield_hit {
            Color::srgb(0.45, 0.75, 1.0)
        } else if health.flash > 0.0 {
            Color::srgb(1.0, 0.3, 0.3)
        } else if dash.active > 0.0 {
            Color::srgb(0.75, 0.92, 1.0)
        } else if health.invulnerable > 0.0 && (time.elapsed_secs() * 20.0).sin() > 0.0 {
            Color::srgba(1.0, 1.0, 1.0, 0.4)
        } else if perks.is_some_and(|p| p.has(PerkKind::Cloak)) {
            // A faint shimmer so the crew can still find you.
            Color::srgba(0.8, 0.65, 1.0, 0.25 + 0.08 * (time.elapsed_secs() * 4.0).sin())
        } else {
            Color::WHITE
        };

        if walking.moving && !health.is_dead() {
            body.walk_time += time.delta_secs();
            transform.translation.y = ((body.walk_time * 14.0).sin().abs() * 1.5).round();
        } else {
            body.walk_time = 0.0;
            transform.translation.y = 0.0;
        }
    }
}

fn dash_trails(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    local_aim: Res<LocalAim>,
    mut players: Query<(&Transform, &Dash, &Aim, &mut DashTrail, Has<LocalPlayer>)>,
) {
    for (transform, dash, aim, mut trail, local) in &mut players {
        if dash.active <= 0.0 {
            trail.timer = 0.0;
            continue;
        }
        trail.timer -= time.delta_secs();
        if trail.timer > 0.0 {
            continue;
        }
        trail.timer = TRAIL_INTERVAL;
        let facing = if local { local_aim.dir } else { aim.dir };
        commands.spawn((
            Level,
            Fade { age: 0.0, life: 0.2 },
            Sprite {
                image: assets.load("sprites/player.png"),
                color: Color::srgb(0.5, 0.85, 1.0),
                flip_x: facing.x < 0.0,
                ..default()
            },
            Transform::from_translation(transform.translation.with_z(9.9)),
        ));
    }
}

/// Pirates left on another deck aren't drawn.
fn hide_other_decks(status: Option<Single<&MissionStatus>>, mut players: Query<(&Player, &mut Visibility)>) {
    let Some(status) = status else { return };
    for (player, mut visibility) in &mut players {
        visibility.set_if_neq(if player.deck == status.deck {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Follows this machine's pirate, or, once they're left behind, whoever is still
/// in the heist.
fn follow_camera(
    time: Res<Time>,
    mut shake: ResMut<Shake>,
    mut rng: ResMut<Rng>,
    players: Query<(&Transform, &Player, &Health, Has<LocalPlayer>), Without<Camera2d>>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
    mut smoothed: Local<Option<Vec2>>,
) {
    let local = players.iter().find(|(_, _, _, local)| *local);
    let in_heist = |p: &Player| p.status != PirateStatus::LeftBehind;
    let followed = match local {
        Some(local) if in_heist(local.1) => Some(local),
        _ => players
            .iter()
            .find(|(_, p, h, _)| p.status == PirateStatus::Active && !h.is_dead())
            .or(local),
    };
    let Some((player, ..)) = followed else { return };
    // Follow a smoothed point and add shake on top, so shake never accumulates.
    let target = player.translation.truncate();
    let t = 1.0 - (-6.0 * time.delta_secs()).exp();
    // Big jumps (riding the lift, switching who we watch) cut instead of panning.
    let follow = smoothed
        .filter(|s| s.distance(target) < 200.0)
        .map_or(target, |s| s.lerp(target, t));
    *smoothed = Some(follow);

    let jolt = Vec2::new(rng.signed(), rng.signed()) * shake.0 * shake.0 * 5.0;
    shake.0 = (shake.0 - 2.5 * time.delta_secs()).max(0.0);
    camera.translation = (follow + jolt).extend(camera.translation.z);
}
