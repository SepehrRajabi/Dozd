use std::collections::HashSet;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

use crate::abilities::{Active, Passive};
use crate::challenge::Rewards;
use crate::class::{Ability, Boons, CrewNames, LASER_SLOW, Laser, LocalLoadout, Loadout, Loadouts, PirateClass, crew_label};
use crate::crew::JoinOrder;
use crate::combat::{Fade, Health, Hurtbox, Shake, Team};
use crate::guns::Arsenal;
use crate::net::{ChooseLoadout, EndSession, HOST_ID, LocalId, LocalPlayer, NetMode, PlayerInput, StartSession, TakePart, authority};
use crate::mission::MissionStatus;
use crate::perks::{PerkKind, Perks};
use crate::room::{Heist, RoomGrid, TILE};
use crate::stats::RunStats;
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
            .init_resource::<WantsIn>()
            .add_observer(refuse_impostors)
            .add_observer(on_client_joined)
            .add_observer(on_client_left)
            // A new crew, or the same one back from a run: nobody's asked yet.
            .add_observer(|_: On<StartSession>, mut wants_in: ResMut<WantsIn>| wants_in.0.clear())
            .add_observer(|_: On<EndSession>, mut wants_in: ResMut<WantsIn>| wants_in.0.clear())
            .add_systems(OnEnter(GameState::Lobby), |mut wants_in: ResMut<WantsIn>| wants_in.0.clear())
            .add_systems(
                PreUpdate,
                ((receive_loadouts, receive_take_part).chain(), receive_input)
                    .after(ServerSystems::Receive)
                    .run_if(authority),
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
            .init_resource::<PadAim>()
            .add_observer(dress_player)
            // The click that started the run is still held: it mustn't fire.
            .add_systems(OnEnter(GameState::Playing), |mut armed: ResMut<TriggerArmed>| armed.0 = false)
            .add_systems(
                Update,
                (
                    gather_input.run_if(in_state(GameState::Playing).or_else(in_state(GameState::Over))),
                    (
                        animate_players,
                        dash_trails,
                        hide_other_decks,
                        // The menus and lobby hold the camera on their backdrop.
                        follow_camera.run_if(in_state(GameState::Playing).or_else(in_state(GameState::Over))),
                    )
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
#[require(RunStats)]
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
    /// The display name they chose, already clean; empty for none.
    pub name: String,
}

impl Player {
    /// What to call this pirate on screen.
    pub fn label(&self) -> String {
        crew_label(&self.name, self.slot as usize)
    }
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

/// How far ahead of their pirate a gamepad player's crosshair sits, the way
/// the right stick points.
const PAD_AIM_REACH: f32 = 110.0;
/// How far a stick must be pushed before it counts: less is drift.
const PAD_DEADZONE: f32 = 0.2;

/// Whether the crosshair follows the mouse or a gamepad's right stick:
/// whichever moved last. `dir` is where the stick last pointed, while it has
/// the crosshair; `cursor` where the mouse was, to notice it moving.
#[derive(Resource, Default)]
struct PadAim {
    dir: Option<Vec2>,
    cursor: Option<Vec2>,
}

/// What this machine's gamepads ask for this frame, any of them. The same
/// controls as the keyboard and mouse, laid out as twin-stick shooters do:
///
/// | Pad | Does | Keys |
/// | --- | --- | --- |
/// | left stick | move | WASD |
/// | right stick | aim | mouse |
/// | right trigger | fire (and lets a hack go) | left click |
/// | left trigger | class ability | Q |
/// | B / East | call a hack off | right click |
/// | A / South | dash | Space |
/// | X / West | reload | R |
/// | Y / North | interact, held to pull bars | E |
/// | bumpers | previous / next gun | wheel |
/// | d-pad up / down | join / turn down a breach | Y / N |
/// | Start | on from the results | Enter |
#[derive(Default, Debug, PartialEq)]
struct PadInput {
    movement: Vec2,
    /// Which way the right stick points, if it's pushed.
    aim: Option<Vec2>,
    fire: bool,
    fire_pressed: bool,
    ability: bool,
    cancel: bool,
    dash: bool,
    reload: bool,
    interact: bool,
    interact_held: bool,
    cycle: i8,
    accept: bool,
    decline: bool,
    restart: bool,
}

fn read_pads<'a>(pads: impl IntoIterator<Item = &'a Gamepad>) -> PadInput {
    use GamepadButton::*;
    let stick = |v: Vec2| if v.length() > PAD_DEADZONE { v.clamp_length_max(1.0) } else { Vec2::ZERO };
    let mut input = PadInput::default();
    for pad in pads {
        input.movement += stick(pad.left_stick());
        input.aim = input.aim.or(stick(pad.right_stick()).try_normalize());
        input.fire |= pad.pressed(RightTrigger2);
        input.fire_pressed |= pad.just_pressed(RightTrigger2);
        input.ability |= pad.just_pressed(LeftTrigger2);
        input.cancel |= pad.just_pressed(East);
        input.dash |= pad.just_pressed(South);
        input.reload |= pad.just_pressed(West);
        input.interact |= pad.just_pressed(North);
        input.interact_held |= pad.pressed(North);
        input.cycle += pad.just_pressed(RightTrigger) as i8 - pad.just_pressed(LeftTrigger) as i8;
        input.accept |= pad.just_pressed(DPadUp);
        input.decline |= pad.just_pressed(DPadDown);
        input.restart |= pad.just_pressed(Start);
    }
    input.movement = input.movement.clamp_length_max(1.0);
    input.cycle = input.cycle.clamp(-1, 1);
    input
}

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
            name: loadout.name.clone(),
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

/// Host-side: clients who asked (`TakePart(true)`) to be dealt into the run
/// under way. Only they get a pirate mid-run; everyone connected gets one when
/// a run starts from the lobby, so it's emptied there.
#[derive(Resource, Default)]
pub struct WantsIn(pub HashSet<u64>);

/// Who has a pirate, by owner and slot. Kept up to date by hand as pirates come
/// and go within a frame, as commands only land after the system.
fn aboard<'a>(players: impl IntoIterator<Item = &'a Player>) -> Vec<(u64, u8)> {
    players.into_iter().map(|p| (p.owner, p.slot)).collect()
}

/// Gives a client who asked into the run a pirate, once they're authorized and
/// their loadout has arrived (whichever comes last). Those who ask on the
/// results screen get one when the next run starts.
fn spawn_joiner(
    commands: &mut Commands,
    owner: u64,
    loadout: &Loadout,
    aboard: &mut Vec<(u64, u8)>,
    grid: Option<&RoomGrid>,
    heist: &Heist,
    state: &State<GameState>,
) {
    let Some(grid) = grid else { return };
    if *state.get() != GameState::Playing || aboard.iter().any(|&(o, _)| o == owner) {
        return;
    }
    let slot = aboard.iter().map(|&(_, slot)| slot + 1).max().unwrap_or(0);
    spawn_player(commands, owner, loadout, slot, crew_spot(grid.crew_spawn, slot as usize), heist.deck);
    aboard.push((owner, slot));
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
    wants_in: Res<WantsIn>,
    mut order: ResMut<JoinOrder>,
) {
    let Ok(id) = ids.get(add.entity) else { return };
    info!("client {} joined", id.get());
    order.0.retain(|o| *o != id.get());
    order.0.push(id.get());
    // Someone connecting mid-run waits in the lobby until they ask in.
    if wants_in.0.contains(&id.get())
        && let Some(loadout) = loadouts.0.get(&id.get())
    {
        let mut aboard = aboard(players);
        spawn_joiner(&mut commands, id.get(), loadout, &mut aboard, grid.as_deref(), &heist, &state);
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
    mut names: ResMut<CrewNames>,
    players: Query<&Player>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    state: Res<State<GameState>>,
    wants_in: Res<WantsIn>,
) {
    let mut aboard = aboard(players);
    for FromClient { client_id, message } in messages.read() {
        let Some((id, authorized)) = client_id.entity().and_then(|e| clients.get(e).ok()) else { continue };
        let id = id.get();
        // Once aboard, a pirate keeps their class, guns and name until the lobby.
        if *state.get() != GameState::Lobby && aboard.iter().any(|&(o, _)| o == id) {
            continue;
        }
        let loadout = message.0.clone().sanitized();
        // Their name holds whatever becomes of the class they asked for.
        names.0.insert(id, loadout.name.clone());
        if let Some(held) = loadouts.0.get_mut(&id) {
            held.name.clone_from(&loadout.name);
        }
        // One of each class per crew. A joiner whose class is taken waits,
        // and picks another from what the roster shows is free.
        let taken = local.0.class == loadout.class
            || loadouts.0.iter().any(|(&owner, l)| owner != id && l.class == loadout.class);
        if taken {
            info!("client {id} asked for {}, which is taken", loadout.class.name());
            continue;
        }
        if authorized && wants_in.0.contains(&id) {
            spawn_joiner(&mut commands, id, &loadout, &mut aboard, grid.as_deref(), &heist, &state);
        }
        loadouts.0.insert(id, loadout);
    }
}

/// A joiner asks into the run under way, or sits the rest of it out. Sitting
/// out takes their pirate out of the run as leaving would, but keeps their
/// seat: their loadout, name and place in the join order.
#[allow(clippy::too_many_arguments)]
fn receive_take_part(
    mut commands: Commands,
    mut messages: MessageReader<FromClient<TakePart>>,
    clients: Query<(&NetworkId, Has<AuthorizedClient>), Without<Refused>>,
    loadouts: Res<Loadouts>,
    mut wants_in: ResMut<WantsIn>,
    players: Query<(Entity, &Player)>,
    grid: Option<Res<RoomGrid>>,
    heist: Res<Heist>,
    state: Res<State<GameState>>,
) {
    let mut aboard = aboard(players.iter().map(|(_, p)| p));
    for FromClient { client_id, message } in messages.read() {
        let Some((id, authorized)) = client_id.entity().and_then(|e| clients.get(e).ok()) else { continue };
        let id = id.get();
        if message.0 {
            info!("client {id} asked into the run");
            wants_in.0.insert(id);
            // The loadout may still be on its way: `receive_loadouts` deals
            // them in when it lands, `on_client_joined` once authorized.
            if authorized && let Some(loadout) = loadouts.0.get(&id) {
                spawn_joiner(&mut commands, id, loadout, &mut aboard, grid.as_deref(), &heist, &state);
            }
        } else {
            info!("client {id} is sitting the run out");
            wants_in.0.remove(&id);
            aboard.retain(|&(owner, _)| owner != id);
            for (entity, player) in &players {
                if player.owner == id {
                    commands.entity(entity).try_despawn();
                }
            }
        }
    }
}

fn on_client_left(
    remove: On<Remove, ConnectedClient>,
    mut commands: Commands,
    ids: Query<&NetworkId, Without<Refused>>,
    mut loadouts: ResMut<Loadouts>,
    mut names: ResMut<CrewNames>,
    mut order: ResMut<JoinOrder>,
    mut wants_in: ResMut<WantsIn>,
    players: Query<(Entity, &Player)>,
) {
    let Ok(id) = ids.get(remove.entity) else { return };
    info!("client {} left", id.get());
    wants_in.0.remove(&id.get());
    loadouts.0.remove(&id.get());
    names.0.remove(&id.get());
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
            Text2d::new(player.label()),
            TextFont::from_font_size(6.0 * PIXEL_SCALE),
            TextColor(slot_color(player.slot)),
            Transform::from_xyz(0.0, 13.0, 1.0).with_scale(Vec3::splat(1.0 / PIXEL_SCALE)),
        ));
    }
}

/// Reads this machine's keyboard, mouse and gamepads and sends them to the host.
fn gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    pads: Query<&Gamepad>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    player: Option<Single<(&Transform, &Player, &Health, &Ability), With<LocalPlayer>>>,
    mut local_aim: ResMut<LocalAim>,
    mut pad_aim: ResMut<PadAim>,
    mut armed: ResMut<TriggerArmed>,
    mut hack_aiming: ResMut<HackAiming>,
    mut inputs: MessageWriter<PlayerInput>,
) {
    let pad = read_pads(pads.iter());
    if !mouse.pressed(MouseButton::Left) && !pad.fire {
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
    let mut ability = keys.just_pressed(KeyCode::KeyQ) || pad.ability;
    let mut hack_released = false;
    if !hack_ready {
        hack_aiming.0 = false;
    } else if hack_aiming.0 {
        if ability || mouse.just_pressed(MouseButton::Right) || pad.cancel {
            hack_aiming.0 = false;
        } else if armed.0 && (mouse.just_pressed(MouseButton::Left) || pad.fire_pressed) {
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
    // The mouse takes the crosshair back by moving; the stick, by being pushed.
    let cursor = window.cursor_position();
    if cursor != pad_aim.cursor {
        pad_aim.cursor = cursor;
        if cursor.is_some() {
            pad_aim.dir = None;
        }
    }
    if pad.aim.is_some() {
        pad_aim.dir = pad.aim;
    }
    let (camera, camera_transform) = *camera;
    if let Some(dir) = pad_aim.dir {
        if let Some(player) = &player {
            local_aim.target = player.0.translation.truncate() + dir * PAD_AIM_REACH;
        }
    } else if let Some(target) = cursor.and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok()) {
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

    let keyboard = Vec2::new(
        axis([KeyCode::KeyA, KeyCode::ArrowLeft], [KeyCode::KeyD, KeyCode::ArrowRight]),
        axis([KeyCode::KeyS, KeyCode::ArrowDown], [KeyCode::KeyW, KeyCode::ArrowUp]),
    );
    inputs.write(PlayerInput {
        movement: (keyboard + pad.movement).clamp_length_max(1.0),
        aim: local_aim.target,
        fire: trigger && (mouse.pressed(MouseButton::Left) || pad.fire),
        fire_pressed: trigger && (mouse.just_pressed(MouseButton::Left) || pad.fire_pressed),
        dash: keys.just_pressed(KeyCode::Space) || pad.dash,
        reload: keys.just_pressed(KeyCode::KeyR) || pad.reload,
        slot: SLOTS.iter().position(|&k| keys.just_pressed(k)).map(|i| i as u8),
        // Wheel down cycles forward.
        cycle: if scroll.delta.y < 0.0 {
            1
        } else if scroll.delta.y > 0.0 {
            -1
        } else {
            pad.cycle
        },
        interact: keys.just_pressed(KeyCode::KeyE) || pad.interact,
        interact_held: keys.pressed(KeyCode::KeyE) || pad.interact_held,
        ability,
        accept: keys.just_pressed(KeyCode::KeyY) || pad.accept,
        decline: keys.just_pressed(KeyCode::KeyN) || pad.decline,
        restart: keys.just_pressed(KeyCode::Enter) || pad.restart,
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
    use crate::crew::{Crewmate, Roster};
    use crate::net::{DropOut, StartSession};
    use crate::room::NextSeed;
    use crate::GamePlugins;

    /// A gamepad with `pressed` held down (pressed this frame) and its sticks at `left` and `right`.
    fn pad(pressed: &[GamepadButton], left: Vec2, right: Vec2) -> Gamepad {
        let mut pad = Gamepad::default();
        for &button in pressed {
            pad.digital_mut().press(button);
        }
        for (axis, value) in [
            (GamepadAxis::LeftStickX, left.x),
            (GamepadAxis::LeftStickY, left.y),
            (GamepadAxis::RightStickX, right.x),
            (GamepadAxis::RightStickY, right.y),
        ] {
            pad.analog_mut().set(axis, value);
        }
        pad
    }

    #[test]
    fn a_gamepad_works_every_control() {
        use GamepadButton::*;
        let idle = read_pads([&pad(&[], Vec2::ZERO, Vec2::ZERO)]);
        assert_eq!(idle, PadInput::default(), "nothing pressed, nothing asked");

        let busy = read_pads([&pad(&[RightTrigger2, LeftTrigger2, South, West, North, RightTrigger, DPadUp, Start], Vec2::new(0.0, 1.0), Vec2::new(-0.8, 0.0))]);
        assert_eq!(busy.movement, Vec2::Y);
        assert_eq!(busy.aim, Some(Vec2::NEG_X));
        assert!(busy.fire && busy.fire_pressed && busy.ability && busy.dash && busy.reload);
        assert!(busy.interact && busy.interact_held && busy.accept && busy.restart);
        assert_eq!(busy.cycle, 1);
        assert!(!busy.cancel && !busy.decline);
        let back = read_pads([&pad(&[LeftTrigger, East, DPadDown], Vec2::ZERO, Vec2::ZERO)]);
        assert!(back.cancel && back.decline && back.cycle == -1);
    }

    #[test]
    fn stick_drift_counts_for_nothing() {
        let drifting = read_pads([&pad(&[], Vec2::new(0.1, -0.12), Vec2::new(0.15, 0.0))]);
        assert_eq!((drifting.movement, drifting.aim), (Vec2::ZERO, None));
        // A full push at an angle is still only full speed.
        assert!(read_pads([&pad(&[], Vec2::ONE, Vec2::ZERO)]).movement.length() <= 1.0 + 1e-6);
    }

    #[test]
    fn two_gamepads_share_one_pirate() {
        use GamepadButton::*;
        let both = read_pads([&pad(&[South], Vec2::X, Vec2::ZERO), &pad(&[RightTrigger2], Vec2::X, Vec2::Y)]);
        assert!(both.dash && both.fire);
        assert_eq!(both.movement, Vec2::X, "never faster than one");
        assert_eq!(both.aim, Some(Vec2::Y));
    }

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

    const JOINER: u64 = 1 << 63 | 9;

    /// `host`, opened to joiners, with one aboard the lobby or run it's in.
    fn host_with_joiner(app: &mut App) -> Entity {
        app.insert_resource(NetMode::Host { port: 0, code: String::new() });
        app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(JOINER), AuthorizedClient)).id()
    }

    /// The joiner asks for `class`, going by `name`.
    fn choose(app: &mut App, client: Entity, class: PirateClass, name: &str) {
        let loadout = Loadout { name: name.into(), ..Loadout::new(class) };
        app.world_mut().write_message(FromClient { client_id: ClientId::Client(client), message: ChooseLoadout(loadout) });
        app.update();
        app.update();
    }

    /// The joiner asks into the run (`true`) or sits it out.
    fn take_part(app: &mut App, client: Entity, wants: bool) {
        app.world_mut().write_message(FromClient { client_id: ClientId::Client(client), message: TakePart(wants) });
        app.update();
        app.update();
    }

    fn roster_entry(app: &mut App) -> Option<Crewmate> {
        let mut rosters = app.world_mut().query::<&Roster>();
        let roster = rosters.single(app.world()).expect("the host keeps a roster");
        roster.0.iter().find(|c| c.owner == JOINER).cloned()
    }

    fn pirate_name(app: &mut App) -> Option<String> {
        let mut pirates = app.world_mut().query::<&Player>();
        pirates.iter(app.world()).find(|p| p.owner == JOINER).map(|p| p.name.clone())
    }

    #[test]
    fn a_joiner_whose_class_is_taken_shows_their_name() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        // The host plays the Gunner.
        choose(&mut app, client, PirateClass::Gunner, "Mira");
        let entry = roster_entry(&mut app).expect("on the roster");
        assert_eq!((entry.class, entry.name.as_str()), (None, "Mira"), "still choosing, but named");
        assert!(pirate_name(&mut app).is_none(), "no pirate until they pick a free class");
        let mut rosters = app.world_mut().query::<&Roster>();
        assert_eq!(rosters.single(app.world()).unwrap().label(1).as_deref(), Some("Mira"));
        assert!(!app.world().resource::<Loadouts>().0.contains_key(&JOINER));

        // A name from a modified client is cleaned like any other.
        choose(&mut app, client, PirateClass::Gunner, " \nZed\u{7}!!  ");
        assert_eq!(roster_entry(&mut app).unwrap().name, "Zed");

        // Picking a free class brings the name aboard.
        choose(&mut app, client, PirateClass::Engineer, "Zed");
        let entry = roster_entry(&mut app).unwrap();
        assert_eq!((entry.class, entry.name.as_str()), (Some(PirateClass::Engineer), "Zed"));
        assert_eq!(pirate_name(&mut app).as_deref(), Some("Zed"));
    }

    #[test]
    fn a_pirate_aboard_keeps_their_name_until_the_lobby() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        choose(&mut app, client, PirateClass::Engineer, "Zed");
        assert_eq!(roster_entry(&mut app).unwrap().name, "Mira");
        assert_eq!(pirate_name(&mut app).as_deref(), Some("Mira"));
    }

    #[test]
    fn a_blank_name_stays_the_slot_label() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        choose(&mut app, client, PirateClass::Engineer, "   ");
        let mut pirates = app.world_mut().query::<&Player>();
        let pirate = pirates.iter(app.world()).find(|p| p.owner == JOINER).expect("aboard");
        assert_eq!(pirate.label(), "P2");
        let mut rosters = app.world_mut().query::<&Roster>();
        assert_eq!(rosters.single(app.world()).unwrap().label(1).as_deref(), Some("P2"));
    }

    #[test]
    fn a_rename_reaches_the_kept_loadout_even_if_its_class_is_refused() {
        let mut app = host();
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Lobby);
        app.update();
        app.update();
        let client = host_with_joiner(&mut app);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        choose(&mut app, client, PirateClass::Gunner, "Zed");
        let kept = &app.world().resource::<Loadouts>().0[&JOINER];
        assert_eq!((kept.class, kept.name.as_str()), (PirateClass::Engineer, "Zed"), "the name the next run spawns with");
        assert_eq!(roster_entry(&mut app).unwrap().name, "Zed");
    }

    #[test]
    fn leaving_forgets_the_name() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        choose(&mut app, client, PirateClass::Gunner, "Mira");
        assert!(app.world().resource::<CrewNames>().0.contains_key(&JOINER));
        app.world_mut().despawn(client);
        app.update();
        assert!(app.world().resource::<CrewNames>().0.is_empty());
        assert!(roster_entry(&mut app).is_none());
    }

    fn has_pirate(app: &mut App) -> bool {
        pirate_name(app).is_some()
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    #[test]
    fn a_mid_run_joiner_gets_a_pirate_only_once_they_ask() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        assert!(!has_pirate(&mut app), "connecting mid-run lands them in the lobby");
        assert_eq!(roster_entry(&mut app).unwrap().class, Some(PirateClass::Engineer), "but their class is held");
        take_part(&mut app, client, true);
        assert!(has_pirate(&mut app));
        assert!(app.world().resource::<WantsIn>().0.contains(&JOINER));
    }

    #[test]
    fn asking_in_before_the_loadout_lands_still_deals_them_in() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        assert!(!has_pirate(&mut app), "no loadout yet");
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        assert!(has_pirate(&mut app));
    }

    #[test]
    fn asking_in_before_being_authorized_waits_for_it() {
        let mut app = host();
        app.insert_resource(NetMode::Host { port: 0, code: String::new() });
        let client = app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(JOINER))).id();
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        take_part(&mut app, client, true);
        assert!(!has_pirate(&mut app));
        app.world_mut().entity_mut(client).insert(AuthorizedClient);
        app.update();
        assert!(has_pirate(&mut app));
    }

    #[test]
    fn sitting_out_takes_the_pirate_out_but_keeps_the_seat() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        let mut pirates = app.world_mut().query::<&Player>();
        let first_slot = pirates.iter(app.world()).find(|p| p.owner == JOINER).unwrap().slot;

        take_part(&mut app, client, false);
        assert!(!has_pirate(&mut app), "out of the run");
        assert!(app.world().get_entity(client).is_ok(), "still connected");
        assert!(!app.world().resource::<WantsIn>().0.contains(&JOINER));
        assert_eq!(app.world().resource::<Loadouts>().0[&JOINER].class, PirateClass::Engineer);
        assert_eq!(app.world().resource::<CrewNames>().0[&JOINER], "Mira");
        assert!(app.world().resource::<JoinOrder>().0.contains(&JOINER));
        assert_eq!(roster_entry(&mut app).unwrap().class, Some(PirateClass::Engineer));
        let mut pirates = app.world_mut().query::<&Player>();
        assert!(pirates.iter(app.world()).any(|p| p.owner == HOST_ID), "the host plays on");

        // Their loadout no longer counts as a pirate aboard: they can change it.
        choose(&mut app, client, PirateClass::Doctor, "Mira");
        assert_eq!(app.world().resource::<Loadouts>().0[&JOINER].class, PirateClass::Doctor);
        assert!(!has_pirate(&mut app), "changing class doesn't deal them back in");

        take_part(&mut app, client, true);
        let mut pirates = app.world_mut().query::<&Player>();
        let back = pirates.iter(app.world()).find(|p| p.owner == JOINER).expect("a fresh pirate");
        assert_eq!(back.class, PirateClass::Doctor);
        assert_eq!(back.slot, first_slot, "the slot they left is free again");
        assert_eq!(state(&app), GameState::Playing);
    }

    #[test]
    fn sitting_out_and_back_in_the_same_frame_deals_a_fresh_pirate() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        take_part(&mut app, client, true);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        let mut pirates = app.world_mut().query::<(Entity, &Player)>();
        let old = pirates.iter(app.world()).find(|(_, p)| p.owner == JOINER).unwrap().0;
        for wants in [false, true] {
            app.world_mut().write_message(FromClient { client_id: ClientId::Client(client), message: TakePart(wants) });
        }
        app.update();
        app.update();
        assert!(app.world().get_entity(old).is_err());
        let mut pirates = app.world_mut().query::<&Player>();
        assert_eq!(pirates.iter(app.world()).filter(|p| p.owner == JOINER).count(), 1);
    }

    #[test]
    fn asking_in_on_the_results_waits_for_the_next_run() {
        let mut app = host();
        let client = host_with_joiner(&mut app);
        choose(&mut app, client, PirateClass::Engineer, "Mira");
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Over);
        app.update();
        take_part(&mut app, client, true);
        assert!(!has_pirate(&mut app));
        // Back in the lobby nobody's asked: the next run deals in everyone.
        app.world_mut().trigger(DropOut);
        app.update();
        app.update();
        assert_eq!(state(&app), GameState::Lobby);
        assert!(app.world().resource::<WantsIn>().0.is_empty());
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        app.update();
        assert!(has_pirate(&mut app));
    }

    #[test]
    fn the_host_dropping_out_takes_the_crew_to_the_lobby() {
        for from in [GameState::Playing, GameState::Over] {
            let mut app = host();
            let client = host_with_joiner(&mut app);
            take_part(&mut app, client, true);
            choose(&mut app, client, PirateClass::Engineer, "Mira");
            assert!(has_pirate(&mut app));
            app.world_mut().resource_mut::<NextState<GameState>>().set(from);
            app.update();
            assert_eq!(state(&app), from);
            app.world_mut().trigger(DropOut);
            app.update();
            app.update();
            assert_eq!(state(&app), GameState::Lobby, "from {from:?}");
            assert!(app.world().get_entity(client).is_ok(), "the crew stays together");
            assert_eq!(*app.world().resource::<NetMode>(), NetMode::Host { port: 0, code: String::new() });
            let mut status = app.world_mut().query::<&MissionStatus>();
            assert_eq!(status.single(app.world()).unwrap().phase(), GameState::Lobby, "joiners follow");

            // And a new run from there deals everyone in.
            app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
            app.update();
            app.update();
            assert_eq!(state(&app), GameState::Playing);
            assert!(has_pirate(&mut app));
        }
    }

    #[test]
    fn dropping_out_solo_ends_the_session() {
        let mut app = host();
        app.world_mut().trigger(DropOut);
        app.update();
        app.update();
        assert_eq!(state(&app), GameState::Menu);
        let mut pirates = app.world_mut().query::<&Player>();
        assert_eq!(pirates.iter(app.world()).count(), 0);
    }
}
