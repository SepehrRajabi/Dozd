use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

use crate::abilities::{Active, Passive};
use crate::challenge::Rewards;
use crate::class::{Ability, Boons, LASER_SLOW, Laser, LocalLoadout, Loadout, Loadouts, PirateClass};
use crate::crew::JoinOrder;
use crate::combat::{Fade, Health, Hurtbox, Shake, Team};
use crate::guns::Arsenal;
use crate::net::{ChooseLoadout, HOST_ID, LocalId, LocalPlayer, NetMode, PlayerInput, authority};
use crate::mission::MissionStatus;
use crate::perks::{PerkKind, Perks};
use crate::room::{Heist, RoomGrid, TILE};
use crate::vault::Looting;
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
/// Where pirates who aren't on the current deck, or have extracted, are parked,
/// far off the map.
pub const OFF_DECK: Vec2 = Vec2::new(-100_000.0, 0.0);
/// Name-tag colours, by join order.
const SLOT_COLORS: [Color; 4] = [
    Color::srgb(1.0, 0.6, 0.25),
    Color::srgb(0.4, 0.85, 1.0),
    Color::srgb(0.5, 1.0, 0.5),
    Color::srgb(1.0, 0.5, 0.85),
];

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrewSize>()
            .add_observer(refuse_impostors)
            .add_observer(on_client_joined)
            .add_observer(on_client_left)
            .add_systems(
                PreUpdate,
                (receive_loadouts, receive_input).after(ServerSystems::Receive).run_if(authority),
            )
            .add_systems(OnEnter(ClientState::Connected), send_loadout)
            .add_systems(
                Update,
                move_players.run_if(in_state(GameState::Playing)).run_if(authority),
            )
            .add_systems(PostUpdate, clear_input_edges.run_if(authority));
    }
}

pub struct PlayerViewPlugin;

impl Plugin for PlayerViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalAim>()
            .init_resource::<TriggerArmed>()
            .init_resource::<HackAiming>()
            .add_observer(dress_player)
            // The click that started the run is still held: it mustn't fire.
            .add_systems(OnEnter(GameState::Playing), |mut armed: ResMut<TriggerArmed>| armed.0 = false)
            .add_systems(
                Update,
                (
                    gather_input.run_if(in_state(GameState::Playing).or_else(in_state(GameState::Over))),
                    (animate_players, dash_trails, hide_other_decks, follow_camera)
                        .chain()
                        .after(move_players),
                ),
            );
    }
}

/// How many pirates this machine plays: 1 normally, more when headless.
/// They get `Player::owner` ids `HOST_ID`, `HOST_ID + 1`, ...
#[derive(Resource)]
pub struct CrewSize(pub u8);

impl Default for CrewSize {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PirateStatus {
    Active,
    /// Got out by a pad; out of play for the rest of the run.
    Extracted,
}

#[derive(Component, Serialize, Deserialize, Clone)]
pub struct Player {
    /// `LocalId` of the machine controlling this pirate.
    pub owner: u64,
    pub class: PirateClass,
    /// The abilities picked in their loadout.
    pub passive: Passive,
    pub active: Option<Active>,
    /// Join order, used for the name tag and spawn spot.
    pub slot: u8,
    /// This pirate's own haul.
    pub credits: u32,
    pub items: u32,
    pub status: PirateStatus,
    /// Took a pad out while the rest of the crew stayed in: keeps only their own haul.
    pub left_early: bool,
    /// What they take home, settled by the host when the run ends (0 until then).
    pub payout: u32,
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
    pub interact_held: bool,
    pub ability: bool,
    pub accept: bool,
    pub decline: bool,
}

impl Controls {
    /// Folds in one input message. Held inputs take the latest value; presses are
    /// kept until `clear_input_edges` so none get lost.
    pub fn apply(&mut self, input: &PlayerInput) {
        self.movement = input.movement.clamp_length_max(1.0);
        self.aim = input.aim;
        self.fire = input.fire;
        self.fire_pressed |= input.fire_pressed;
        self.dash |= input.dash;
        self.reload |= input.reload;
        self.slot = input.slot.or(self.slot);
        self.cycle = (self.cycle + input.cycle).clamp(-1, 1);
        self.interact |= input.interact;
        self.interact_held = input.interact_held;
        self.ability |= input.ability;
        self.accept |= input.accept;
        self.decline |= input.decline;
    }
}

/// This machine's aim, used for the crosshair and the local pirate's gun so they
/// respond instantly instead of waiting for the host.
#[derive(Resource)]
pub struct LocalAim {
    pub target: Vec2,
    pub dir: Vec2,
}

/// Whether the trigger answers the mouse yet. Off when a run starts, until the
/// button is let go, so the menu click that started it doesn't fire a shot.
#[derive(Resource, Default)]
struct TriggerArmed(bool);

/// A Hacker with a hack ready pressed Q: the hack circle follows the
/// crosshair until a left click lets it go, or Q or a right click calls it off.
/// The click doesn't fire the gun.
#[derive(Resource, Default)]
pub struct HackAiming(pub bool);

impl Default for LocalAim {
    fn default() -> Self {
        Self { target: Vec2::ZERO, dir: Vec2::X }
    }
}

fn spawn_player(commands: &mut Commands, owner: u64, loadout: &Loadout, slot: u8, pos: Vec2, deck: u8) {
    commands.spawn((
        Level,
        Replicated,
        Player {
            owner,
            class: loadout.class,
            passive: loadout.passive,
            active: loadout.active,
            slot,
            credits: 0,
            items: 0,
            status: PirateStatus::Active,
            left_early: false,
            payout: 0,
            deck,
        },
        Team::Player,
        Health::new(MAX_HEALTH, 0.5).with_shield(loadout.passive.shield()),
        Hurtbox(6.0),
        Walking::default(),
        Dash::default(),
        Aim { dir: Vec2::X, target: pos + Vec2::X * 32.0 },
        Arsenal::new(&loadout.guns),
        Ability::default(),
        Boons::default(),
        Perks::default(),
        (Controls::default(), Looting::default(), Rewards::default()),
        Transform::from_translation(pos.extend(10.0)),
    ));
}

/// The colour of a pirate's name tag, by join order.
pub fn slot_color(slot: u8) -> Color {
    SLOT_COLORS[slot as usize % SLOT_COLORS.len()]
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
    crew: Res<CrewSize>,
    local: Res<LocalLoadout>,
    loadouts: Res<Loadouts>,
    order: Res<JoinOrder>,
    clients: Query<&NetworkId, With<AuthorizedClient>>,
) {
    let local_owners = (0..crew.0 as u64).map(|i| HOST_ID + i);
    // In join order, so slots match the lobby's. A client whose loadout hasn't
    // arrived yet joins as soon as it does.
    let mut client_owners: Vec<u64> =
        clients.iter().map(NetworkId::get).filter(|id| loadouts.0.contains_key(id)).collect();
    client_owners.sort_by_key(|id| order.0.iter().position(|o| o == id));
    for (slot, owner) in local_owners.chain(client_owners).enumerate() {
        let pos = crew_spot(grid.crew_spawn, slot);
        spawn_player(&mut commands, owner, loadouts.get(owner, &local), slot as u8, pos, heist.deck);
    }
}

/// Gives a client who joined mid-run a pirate, once they're authorized and their
/// loadout has arrived (whichever comes last). Mid-results joiners get one when
/// the next run starts.
fn spawn_joiner(
    commands: &mut Commands,
    owner: u64,
    loadout: &Loadout,
    players: &Query<&Player>,
    grid: Option<&RoomGrid>,
    heist: &Heist,
    state: &State<GameState>,
) {
    let Some(grid) = grid else { return };
    if *state.get() != GameState::Playing || players.iter().any(|p| p.owner == owner) {
        return;
    }
    let slot = players.iter().map(|p| p.slot + 1).max().unwrap_or(0);
    spawn_player(commands, owner, loadout, slot, crew_spot(grid.crew_spawn, slot as usize), heist.deck);
}

/// Host-side: a client turned away by `refuse_impostors`. Its messages are
/// ignored until it's gone, and its leaving touches nobody's pirate.
#[derive(Component)]
pub struct Refused;

/// Whether a client claiming to be `id` would pass for a pirate already
/// aboard: the host's own (`HOST_ID` and the rest of this machine's crew) or
/// anyone's who has one. Joiners choose their own ids, so a modified client
/// could otherwise take over someone else's pirate.
pub fn impostor(id: u64, crew: &CrewSize, players: impl IntoIterator<Item = u64>) -> bool {
    (HOST_ID..HOST_ID + crew.0 as u64).contains(&id) || players.into_iter().any(|owner| owner == id)
}

/// Turns away, as soon as they connect, clients whose id is already spoken for
/// (see `impostor`).
fn refuse_impostors(
    add: On<Add, ConnectedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId>,
    crew: Res<CrewSize>,
    players: Query<&Player>,
) {
    let Ok(id) = ids.get(add.entity) else { return };
    if impostor(id.get(), &crew, players.iter().map(|p| p.owner)) {
        warn!("refused client {}: that id is already aboard", id.get());
        commands.entity(add.entity).insert(Refused);
        commands.write_message(DisconnectRequest { client: add.entity });
    }
}

/// The `Player::owner` id a message came from, `None` for a refused client.
fn sender(client_id: &ClientId, ids: &Query<(&NetworkId, Has<Refused>)>) -> Option<u64> {
    match client_id {
        ClientId::Server => Some(HOST_ID),
        ClientId::Client(entity) => ids.get(*entity).ok().filter(|(_, refused)| !refused).map(|(id, _)| id.get()),
    }
}

fn on_client_joined(
    add: On<Add, AuthorizedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId, Without<Refused>>,
    loadouts: Res<Loadouts>,
    players: Query<&Player>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    state: Res<State<GameState>>,
    mut order: ResMut<JoinOrder>,
) {
    let Ok(id) = ids.get(add.entity) else { return };
    info!("client {} joined", id.get());
    order.0.retain(|o| *o != id.get());
    order.0.push(id.get());
    if let Some(loadout) = loadouts.0.get(&id.get()) {
        spawn_joiner(&mut commands, id.get(), loadout, &players, grid.as_deref(), &heist, &state);
    }
}

/// Tells the host which class and guns this machine's pirate brings.
fn send_loadout(local: Res<LocalLoadout>, mut messages: MessageWriter<ChooseLoadout>) {
    messages.write(ChooseLoadout(local.0.clone()));
}

fn receive_loadouts(
    mut commands: Commands,
    mut messages: MessageReader<FromClient<ChooseLoadout>>,
    clients: Query<(&NetworkId, Has<AuthorizedClient>), Without<Refused>>,
    local: Res<LocalLoadout>,
    mut loadouts: ResMut<Loadouts>,
    players: Query<&Player>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    state: Res<State<GameState>>,
) {
    for FromClient { client_id, message } in messages.read() {
        let Some((id, authorized)) = client_id.entity().and_then(|e| clients.get(e).ok()) else { continue };
        let id = id.get();
        // Once aboard, a pirate keeps their class and guns until the lobby.
        if *state.get() != GameState::Lobby && players.iter().any(|p| p.owner == id) {
            continue;
        }
        // One of each class per crew. A joiner whose class is taken waits,
        // and picks another from what the roster shows is free.
        let loadout = message.0.clone().sanitized();
        let taken = local.0.class == loadout.class
            || loadouts.0.iter().any(|(&owner, l)| owner != id && l.class == loadout.class);
        if taken {
            info!("client {id} asked for {}, which is taken", loadout.class.name());
            continue;
        }
        if authorized {
            spawn_joiner(&mut commands, id, &loadout, &players, grid.as_deref(), &heist, &state);
        }
        loadouts.0.insert(id, loadout);
    }
}

fn on_client_left(
    remove: On<Remove, ConnectedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId, Without<Refused>>,
    mut loadouts: ResMut<Loadouts>,
    mut order: ResMut<JoinOrder>,
    players: Query<(Entity, &Player)>,
) {
    let Ok(id) = ids.get(remove.entity) else { return };
    info!("client {} left", id.get());
    loadouts.0.remove(&id.get());
    order.0.retain(|o| *o != id.get());
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
        parent.spawn((PlayerSprite { walk_time: 0.0 }, Sprite::from_image(assets.load(player.class.sprite()))));
    });

    if player.owner == local.0 {
        entity.insert(LocalPlayer);
    } else {
        entity.with_child((
            Text2d::new(format!("P{}", player.slot + 1)),
            TextFont::from_font_size(6.0 * PIXEL_SCALE),
            TextColor(slot_color(player.slot)),
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
    player: Option<Single<(&Transform, &Player, &Health, &Ability), With<LocalPlayer>>>,
    mut local_aim: ResMut<LocalAim>,
    mut armed: ResMut<TriggerArmed>,
    mut hack_aiming: ResMut<HackAiming>,
    mut inputs: MessageWriter<PlayerInput>,
) {
    if !mouse.pressed(MouseButton::Left) {
        armed.0 = true;
    }
    // A hack goes off on the click after Q, not on Q itself.
    let hack_ready = player.as_ref().is_some_and(|p| {
        let (_, player, health, ability) = **p;
        player.active == Some(Active::Hack)
            && player.status == PirateStatus::Active
            && !health.is_dead()
            && ability.cooldown <= 0.0
    });
    let mut ability = keys.just_pressed(KeyCode::KeyQ);
    let mut hack_released = false;
    if !hack_ready {
        hack_aiming.0 = false;
    } else if hack_aiming.0 {
        if keys.just_pressed(KeyCode::KeyQ) || mouse.just_pressed(MouseButton::Right) {
            hack_aiming.0 = false;
        } else if armed.0 && mouse.just_pressed(MouseButton::Left) {
            hack_aiming.0 = false;
            hack_released = true;
            // Holding the button on doesn't fire the gun after the hack.
            armed.0 = false;
        }
        ability = hack_released;
    } else if ability {
        hack_aiming.0 = true;
        ability = false;
    }
    let trigger = armed.0 && !hack_aiming.0 && !hack_released;
    let (camera, camera_transform) = *camera;
    if let Some(target) = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
    {
        local_aim.target = target;
    }
    if let Some(player) = player
        && let Some(dir) = (local_aim.target - player.0.translation.truncate()).try_normalize()
    {
        local_aim.dir = dir;
    }

    let axis = |neg: [KeyCode; 2], pos: [KeyCode; 2]| {
        keys.any_pressed(pos) as i8 as f32 - keys.any_pressed(neg) as i8 as f32
    };
    const SLOTS: [KeyCode; 4] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];

    inputs.write(PlayerInput {
        movement: Vec2::new(
            axis([KeyCode::KeyA, KeyCode::ArrowLeft], [KeyCode::KeyD, KeyCode::ArrowRight]),
            axis([KeyCode::KeyS, KeyCode::ArrowDown], [KeyCode::KeyW, KeyCode::ArrowUp]),
        ),
        aim: local_aim.target,
        fire: trigger && mouse.pressed(MouseButton::Left),
        fire_pressed: trigger && mouse.just_pressed(MouseButton::Left),
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
        interact_held: keys.pressed(KeyCode::KeyE),
        ability,
        accept: keys.just_pressed(KeyCode::KeyY),
        decline: keys.just_pressed(KeyCode::KeyN),
        restart: keys.just_pressed(KeyCode::Enter),
    });
}

fn receive_input(
    mut inputs: MessageReader<FromClient<PlayerInput>>,
    ids: Query<(&NetworkId, Has<Refused>)>,
    mut players: Query<(&Player, &mut Controls)>,
    state: Res<State<GameState>>,
    mode: Res<NetMode>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for FromClient { client_id, message } in inputs.read() {
        let Some(owner) = sender(client_id, &ids) else { continue };
        // Only the host moves everyone on from the results.
        if message.restart && *client_id == ClientId::Server && *state.get() == GameState::Over {
            next_state.set(if *mode == NetMode::Solo { GameState::Playing } else { GameState::Lobby });
        }
        if let Some((_, mut controls)) = players.iter_mut().find(|(p, _)| p.owner == owner) {
            controls.apply(message);
        }
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
        c.ability = false;
        c.accept = false;
        c.decline = false;
    }
}

pub fn move_players(
    time: Res<Time>,
    grid: Res<RoomGrid>,
    mut players: Query<(&Player, &Controls, &mut Transform, &mut Walking, &mut Dash, &mut Health, &mut Aim, Option<&Boons>)>,
    lasers: Query<&Laser>,
) {
    let dt = time.delta_secs();
    for (player, controls, mut transform, mut walking, mut dash, mut health, mut aim, boons) in &mut players {
        if health.is_dead() || player.status != PirateStatus::Active {
            if walking.moving {
                walking.moving = false;
            }
            continue;
        }

        let pos = transform.translation.truncate();
        // Only touched when it moves, so an idle pirate's aim isn't resent.
        if aim.target != controls.aim {
            aim.target = controls.aim;
        }
        if let Some(dir) = (controls.aim - pos).try_normalize()
            && aim.dir != dir
        {
            aim.dir = dir;
        }

        let moving = controls.movement != Vec2::ZERO;
        if walking.moving != moving {
            walking.moving = moving;
        }

        // Overclocked, the dash comes back sooner.
        let rate = boons.map_or(1.0, |b| b.cooldown_rate());
        if dash.cooldown > 0.0 {
            dash.cooldown = (dash.cooldown - dt * rate).max(0.0);
        }
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
            // Bracing against the laser's kick slows a Gunner down.
            let lasing = lasers.iter().any(|l| l.owner == player.owner);
            controls.movement.normalize() * SPEED * if lasing { LASER_SLOW } else { 1.0 } * dt
        } else {
            continue;
        };

        let new = grid.slide(pos + FEET_OFFSET, step, HITBOX_HALF) - FEET_OFFSET;
        // Pushing into a wall goes nowhere: leave the transform be.
        if new != pos {
            transform.translation.x = new.x;
            transform.translation.y = new.y;
        }
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
    mut players: Query<(&Player, &Transform, &Dash, &Aim, &mut DashTrail, Has<LocalPlayer>)>,
) {
    for (player, transform, dash, aim, mut trail, local) in &mut players {
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
                image: assets.load(player.class.sprite()),
                color: Color::srgb(0.5, 0.85, 1.0),
                flip_x: facing.x < 0.0,
                ..default()
            },
            Transform::from_translation(transform.translation.with_z(9.9)),
        ));
    }
}

/// Pirates on another deck, or who left early by a pad, aren't drawn, nor is
/// anything hung on them (name tag, shadow, pull bar).
fn hide_other_decks(status: Option<Single<&MissionStatus>>, mut players: Query<(&Player, &mut Visibility)>) {
    let Some(status) = status else { return };
    for (player, mut visibility) in &mut players {
        visibility.set_if_neq(if player.deck == status.deck && !player.left_early {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Follows this machine's pirate, or, once they've got out, whoever is still in
/// the heist.
fn follow_camera(
    time: Res<Time>,
    mut shake: ResMut<Shake>,
    // Its own stream, so looks never change how the game plays out.
    mut rng: Local<Rng>,
    players: Query<(&Transform, &Player, &Health, Has<LocalPlayer>), Without<Camera2d>>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
    mut smoothed: Local<Option<Vec2>>,
) {
    let local = players.iter().find(|(_, _, _, local)| *local);
    let in_heist = |p: &Player| p.status == PirateStatus::Active;
    let followed = match local {
        // Those who end the run on a pad stay where they got out.
        Some(local) if !local.1.left_early => Some(local),
        // Watch a living crewmate, else where the crew fell; an early leaver
        // sits off the map, so their own pirate is the last resort.
        _ => players
            .iter()
            .find(|(_, p, h, _)| in_heist(p) && !h.is_dead())
            .or_else(|| players.iter().find(|(_, p, ..)| in_heist(p)))
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::net::StartSession;
    use crate::room::NextSeed;
    use crate::GamePlugins;

    #[test]
    fn ids_already_aboard_are_impostors() {
        let crew = CrewSize(3);
        assert!(impostor(HOST_ID, &crew, []), "the host's own");
        assert!(impostor(HOST_ID + 2, &crew, []), "another local pirate");
        assert!(!impostor(HOST_ID + 3, &crew, []), "past the local crew");
        assert!(impostor(77, &crew, [5, 77]), "a pirate already aboard");
        assert!(!impostor(1 << 63 | 12345, &CrewSize(1), [42]));
    }

    /// A host in a run, with the network left out: clients are faked by
    /// spawning their entities.
    fn host() -> App {
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
        app
    }

    fn host_controls(app: &mut App) -> Vec2 {
        let mut pirates = app.world_mut().query::<(&Player, &Controls)>();
        let (_, controls) = pirates.iter(app.world()).find(|(p, _)| p.owner == HOST_ID).expect("the host's pirate");
        controls.movement
    }

    #[test]
    fn a_client_posing_as_the_host_is_refused() {
        let mut app = host();
        let impostor = app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(HOST_ID))).id();
        let honest = app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(1 << 63 | 9))).id();
        assert!(app.world().entity(impostor).contains::<Refused>(), "refused on connecting");
        assert!(!app.world().entity(honest).contains::<Refused>());

        // Whatever it sends before it's gone doesn't steer the host's pirate.
        let input = PlayerInput { movement: Vec2::X, ..default() };
        app.world_mut().write_message(FromClient { client_id: ClientId::Client(impostor), message: input });
        app.update();
        assert_eq!(host_controls(&mut app), Vec2::ZERO);

        // It's disconnected, and its leaving takes nobody's pirate with it.
        assert!(app.world().get_entity(impostor).is_err(), "disconnected");
        assert!(app.world().get_entity(honest).is_ok());
        let mut pirates = app.world_mut().query::<&Player>();
        assert!(pirates.iter(app.world()).any(|p| p.owner == HOST_ID), "the host keeps their pirate");
    }

    #[test]
    fn standing_still_changes_nothing() {
        let mut app = host();
        let ticks = |app: &mut App| {
            let mut pirates = app.world_mut().query::<(Ref<Aim>, Ref<Dash>, Ref<Walking>, Ref<Transform>)>();
            let (aim, dash, walking, transform) = pirates.single(app.world()).expect("one pirate");
            [aim.last_changed(), dash.last_changed(), walking.last_changed(), transform.last_changed()]
        };
        let before = ticks(&mut app);
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(ticks(&mut app), before, "an idle pirate's state was marked changed");
    }

    #[test]
    fn only_the_host_restarts() {
        let mut app = host();
        let client = app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(1 << 63 | 9))).id();
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Over);
        app.update();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Over);

        let restart = PlayerInput { restart: true, ..default() };
        app.world_mut().write_message(FromClient { client_id: ClientId::Client(client), message: restart.clone() });
        app.update();
        app.update();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Over, "a joiner can't");

        app.world_mut().write_message(FromClient { client_id: ClientId::Server, message: restart });
        app.update();
        app.update();
        assert_ne!(*app.world().resource::<State<GameState>>().get(), GameState::Over, "the host can");
    }
}
