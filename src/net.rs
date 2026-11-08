use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::netcode::{
    ClientAuthentication, ConnectToken, NETCODE_KEY_BYTES, NetcodeClientTransport, NetcodeDisconnectReason,
    NetcodeServerTransport, ServerAuthentication, ServerConfig, generate_random_bytes,
};
use bevy_replicon_renet::renet::ConnectionConfig;
use bevy_replicon_renet::{RenetChannelsExt, RenetClient, RenetServer, RepliconRenetPlugins};
use serde::{Deserialize, Serialize};

use crate::boss::{BruteLook, Keeper, KeeperBeam, KeeperHand, PrismLook, StalkerLook};
use crate::breach::Breach;
use crate::challenge::{Challenge, Rewards};
use crate::class::{Ability, Boons, Laser, LeechMine, Loadout, PirateClass, RiotShield, Sentry};
use crate::combat::{Fx, Health, ProjectileLook};
use crate::crew::{Link, Roster};
use crate::drops::EnemyDrop;
use crate::enemies::EnemyLook;
use crate::guns::Arsenal;
use crate::loot::LootKind;
use crate::mission::MissionStatus;
use crate::perks::{PerkPickup, Perks};
use crate::player::{Aim, Dash, Player, Walking};
use crate::vault::{Looting, Vault};
use crate::stats::RunStats;
use crate::{GameState, Level};

pub const DEFAULT_PORT: u16 = 5000;
/// This build, as shown to players. Only builds with the same fingerprint
/// (`DOZD_BUILD`, see `build.rs`) can play together: `Cargo.toml`'s version
/// alone wouldn't tell two different builds apart.
pub const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"), " (", env!("DOZD_BUILD"), ")");
const PROTOCOL_ID: u64 = 0xD02D;
/// One pirate per class: the host plus a joiner for each class left.
const MAX_CLIENTS: usize = PirateClass::ALL.len() - 1;
/// Give up on reaching a host after this long.
const CONNECT_TIMEOUT: f32 = 10.0;
/// Player id of the host (or solo player); joining clients get random non-zero ids.
pub const HOST_ID: u64 = 0;
/// What every connect token lists after the address the joiner really dials,
/// and all a host claims as its public address. Netcode only lets in a token
/// naming one of the host's public addresses, but behind NAT a host can't know
/// the address its crew dials, so both sides name this stand-in instead. It's
/// in TEST-NET-1 (RFC 5737), where nothing answers: a joiner that ever falls
/// back to it (netcode tries a token's addresses in turn) just times out.
const SENTINEL: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 45)), 0xD02D);
/// How long a joiner's connect token is good for. Generous, as the host checks
/// it against its own clock, which may be off from the joiner's.
const TOKEN_LIFE: u64 = 7 * 24 * 60 * 60;
/// Seconds of silence before netcode drops a connection.
const TOKEN_TIMEOUT: i32 = 15;
/// Rounds of mixing that turn a room code into a key (see `room_key`).
const KEY_ROUNDS: u32 = 1 << 20;

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub enum NetMode {
    Solo,
    /// Hosting on `port`; joiners need the room `code` (see `room_code`).
    Host { port: u16, code: String },
    /// Joining the host at `addr` with its room `code`, as `parse_code` leaves it.
    Join { addr: SocketAddr, code: String },
}

/// The letters a room code is made of: no 0/O, 1/I/L to mix up.
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
/// How many letters a room code has.
pub const CODE_LEN: usize = 8;

/// A fresh random room code for a host to hand its crew, e.g. `K7QF29XA`.
pub fn room_code() -> String {
    // Bytes past the last whole run of the alphabet are thrown away, so every
    // letter is equally likely (a plain `%` would favour the first few).
    let fair = u8::MAX as usize + 1 - (u8::MAX as usize + 1) % CODE_ALPHABET.len();
    let mut code = String::with_capacity(CODE_LEN);
    while code.len() < CODE_LEN {
        for b in generate_random_bytes::<CODE_LEN>() {
            if (b as usize) < fair && code.len() < CODE_LEN {
                code.push(CODE_ALPHABET[b as usize % CODE_ALPHABET.len()] as char);
            }
        }
    }
    code
}

/// A room code as typed: any case, with or without dashes and spaces.
pub fn parse_code(raw: &str) -> Result<String, String> {
    let code: String = raw.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
    if code.is_empty() {
        Err("enter the host's room code".into())
    } else if code.len() == CODE_LEN && code.bytes().all(|b| CODE_ALPHABET.contains(&b)) {
        Ok(code)
    } else {
        Err(format!("`{raw}` isn't a room code: ask the host for theirs ({CODE_LEN} letters and digits)"))
    }
}

/// A room code split in two for reading out, e.g. `K7QF-29XA`.
pub fn show_code(code: &str) -> String {
    let (a, b) = code.split_at(code.len() / 2);
    format!("{a}-{b}")
}

/// The netcode key a room code stands for, the same on every machine: the host
/// encrypts with it and joiners sign their connect tokens with it, so only
/// those who know the code get in or can read the traffic.
///
/// The code is the whole secret; `PROTOCOL_ID` only keeps keys apart between
/// versions. Its 8 letters from 31 carry about 40 bits, plenty against a
/// stranger guessing, and `KEY_ROUNDS` of mixing make each guess cost a few
/// milliseconds for someone trying every code against a captured handshake,
/// but it's no match for a determined attacker: a fresh code every session
/// keeps what they could learn short-lived.
pub fn room_key(code: &str) -> [u8; NETCODE_KEY_BYTES] {
    // SplitMix64's finaliser: every input bit flips about half the output bits.
    fn mix(mut x: u64) -> u64 {
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    }
    let mut lanes = [PROTOCOL_ID, 0x9E37_79B9_7F4A_7C15, 0x6A09_E667_F3BC_C908, 0xBB67_AE85_84CA_A73B];
    for (i, b) in code.bytes().enumerate() {
        let lane = i % lanes.len();
        lanes[lane] = mix(lanes[lane] ^ u64::from(b) ^ ((i as u64) << 8));
    }
    for round in 0..KEY_ROUNDS {
        for i in 0..lanes.len() {
            let next = lanes[(i + 1) % lanes.len()];
            lanes[i] = mix(lanes[i] ^ next.rotate_left(17) ^ u64::from(round));
        }
    }
    let mut key = [0; NETCODE_KEY_BYTES];
    for (chunk, lane) in key.as_chunks_mut::<8>().0.iter_mut().zip(lanes) {
        *chunk = lane.to_le_bytes();
    }
    key
}

impl NetMode {
    /// `dozd` | `dozd host [port]` | `dozd join <address> <room code>`; `None` means show the menu.
    pub fn from_args() -> Result<Option<Self>, String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        match args.first().map(String::as_str) {
            None => Ok(None),
            Some("solo") => Ok(Some(Self::Solo)),
            Some("host") => {
                let port = match args.get(1) {
                    Some(p) => parse_port(p)?,
                    None => DEFAULT_PORT,
                };
                Ok(Some(Self::Host { port, code: room_code() }))
            }
            Some("join") => {
                const USAGE: &str = "usage: dozd join <ip or hostname>[:port] <room code>";
                let raw = args.get(1).ok_or(USAGE)?;
                let code = parse_code(args.get(2).ok_or(USAGE)?)?;
                Ok(Some(Self::Join { addr: resolve(raw)?, code }))
            }
            Some(other) => Err(format!(
                "unknown mode `{other}`\nusage: dozd [solo | host [port] | join <address> <room code>]"
            )),
        }
    }

    pub fn is_authority(&self) -> bool {
        !matches!(self, Self::Join { .. })
    }
}

pub fn parse_port(raw: &str) -> Result<u16, String> {
    raw.trim()
        .parse::<u16>()
        .ok()
        .filter(|&p| p > 0)
        .ok_or_else(|| format!("`{raw}` isn't a valid port (1-65535)"))
}

/// Accepts `ip`, `ip:port`, `hostname` or `hostname:port`; the port defaults to 5000.
pub fn resolve(raw: &str) -> Result<SocketAddr, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("enter the host's address".into());
    }
    if let Ok(addr) = raw.parse::<SocketAddr>() {
        return Ok(addr);
    }
    if let Ok(ip) = raw.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, DEFAULT_PORT));
    }
    let with_port = if raw.contains(':') { raw.to_string() } else { format!("{raw}:{DEFAULT_PORT}") };
    with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.find(SocketAddr::is_ipv4))
        .ok_or_else(|| format!("couldn't find a host called `{raw}`"))
}

/// This machine's LAN address, to tell friends where to connect.
pub fn local_ip() -> Option<IpAddr> {
    // Connecting a UDP socket sends nothing; it just picks the outgoing interface.
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// This machine's Tailscale address, while Tailscale is up. The route to
/// Tailscale's own DNS server, 100.100.100.100, only leaves by the tailnet
/// then, so the address it leaves from is this machine's on the tailnet.
pub fn tailscale_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(100, 100, 100, 100), 53)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if is_tailnet(ip) => Some(IpAddr::V4(ip)),
        _ => None,
    }
}

/// Whether `ip` is in 100.64.0.0/10, the range Tailscale hands out.
fn is_tailnet(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && b & 0xC0 == 64
}

/// Run condition: this app simulates the game (solo or host), rather than mirroring a host.
pub fn authority(mode: Res<NetMode>) -> bool {
    mode.is_authority()
}

/// The `Player::owner` id of the pirate controlled from this machine.
#[derive(Resource)]
pub struct LocalId(pub u64);

/// Marks the pirate controlled from this machine.
#[derive(Component)]
pub struct LocalPlayer;

/// A message for the menu, e.g. why the last session ended.
#[derive(Resource, Default)]
pub struct Notice(pub Option<String>);

/// Joining progress, so we can time out and notice a lost host.
#[derive(Resource, Default)]
pub struct Connection {
    pub waited: f32,
    connected: bool,
}

/// Starts playing in the given mode (from the menu or the command line).
#[derive(Event)]
pub struct StartSession(pub NetMode);

/// Leaves the current game and returns to the menu, optionally explaining why.
#[derive(Event)]
pub struct EndSession(pub Option<String>);

/// Leaves the run in progress for the crew lobby without leaving the crew: a
/// joiner sits the rest of the run out, while the host ends the run and takes
/// the whole crew back to the lobby. Solo, it ends the session like
/// `EndSession(None)`.
#[derive(Event)]
pub struct DropOut;

/// A joiner waiting in the lobby while the crew is mid-run asks to drop in.
#[derive(Event)]
pub struct JoinRun;

/// Client-side: this joiner chose to sit out the run in progress, so it waits
/// in the lobby whatever the host's phase, until it joins back in or the crew
/// returns to the lobby.
#[derive(Resource, Default)]
pub struct SittingOut(pub bool);

/// A `Transform` as it goes to joiners. Everything in this 2D game sits flat,
/// turned only about Z and scaled evenly, so that's all that's sent: 20 bytes
/// a move where the whole transform takes 40, and moves are most of the traffic.
#[derive(Serialize, Deserialize)]
struct NetTransform {
    translation: Vec3,
    angle: f32,
    scale: f32,
}

impl From<Transform> for NetTransform {
    fn from(transform: Transform) -> Self {
        Self {
            translation: transform.translation,
            angle: transform.rotation.to_euler(EulerRot::ZYX).0,
            scale: transform.scale.x,
        }
    }
}

impl From<NetTransform> for Transform {
    fn from(net: NetTransform) -> Self {
        Self {
            translation: net.translation,
            rotation: Quat::from_rotation_z(net.angle),
            scale: Vec3::splat(net.scale),
        }
    }
}

/// Everything a pirate can do in one frame, sent from each machine to the host.
#[derive(Message, Serialize, Deserialize, Clone, Default, Debug)]
pub struct PlayerInput {
    pub movement: Vec2,
    pub aim: Vec2,
    pub fire: bool,
    pub fire_pressed: bool,
    pub dash: bool,
    pub reload: bool,
    pub slot: Option<u8>,
    pub cycle: i8,
    pub interact: bool,
    /// E is held down (pulling bars from the vault).
    pub interact_held: bool,
    /// Use the class ability (the Engineer's sentry).
    pub ability: bool,
    /// Y / N: join, or turn down, a crewmate's call to breach the vault.
    pub accept: bool,
    pub decline: bool,
    pub restart: bool,
}

/// A joiner's class and guns, sent to the host as soon as they connect.
#[derive(Message, Serialize, Deserialize, Clone, Debug)]
pub struct ChooseLoadout(pub Loadout);

/// A joiner asks the host to deal them into the run under way (`true`), or
/// says they're sitting the rest of it out (`false`). Mid-run, only those who
/// asked get a pirate; one who sits out keeps their seat in the crew.
#[derive(Message, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakePart(pub bool);

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RepliconPlugins, RepliconRenetPlugins));
        // The handshake compares type names and their order, which most changes
        // leave alone: this makes it turn away every build but the host's own.
        app.world_mut().resource_mut::<ProtocolHasher>().add_custom(env!("DOZD_BUILD"));
        app.insert_resource(NetMode::Solo)
            .insert_resource(LocalId(HOST_ID))
            .init_resource::<Notice>()
            .init_resource::<Connection>()
            .init_resource::<SittingOut>()
            // Registration order must match on every machine; keep it all here.
            .replicate_as::<Transform, NetTransform>()
            .replicate::<Player>()
            .replicate::<RunStats>()
            .replicate::<Aim>()
            .replicate::<Walking>()
            .replicate::<Dash>()
            .replicate::<Health>()
            .replicate::<Arsenal>()
            .replicate::<EnemyLook>()
            .replicate::<LootKind>()
            .replicate::<PerkPickup>()
            .replicate::<EnemyDrop>()
            .replicate::<Perks>()
            .replicate::<ProjectileLook>()
            .replicate::<MissionStatus>()
            .replicate::<Ability>()
            .replicate::<Sentry>()
            .replicate::<LeechMine>()
            .replicate::<RiotShield>()
            .replicate::<Laser>()
            .replicate::<Link>()
            .replicate::<Roster>()
            .replicate::<Boons>()
            .replicate::<Vault>()
            .replicate::<Looting>()
            .replicate::<Challenge>()
            .replicate::<Rewards>()
            .replicate::<Breach>()
            .replicate::<Keeper>()
            .replicate::<KeeperHand>()
            .replicate::<KeeperBeam>()
            .replicate::<StalkerLook>()
            .replicate::<BruteLook>()
            .replicate::<PrismLook>()
            .add_client_message::<PlayerInput>(Channel::Ordered)
            .add_client_message::<ChooseLoadout>(Channel::Ordered)
            .add_client_message::<TakePart>(Channel::Ordered)
            .add_server_event::<Fx>(Channel::Unordered)
            .add_observer(start_session)
            .add_observer(end_session)
            .add_observer(drop_out)
            .add_observer(join_run)
            .add_observer(on_protocol_mismatch)
            .add_systems(Update, watch_connection);
    }
}

fn start_session(
    start: On<StartSession>,
    mut commands: Commands,
    channels: Res<RepliconChannels>,
    mut notice: ResMut<Notice>,
    mut connection: ResMut<Connection>,
    mut sitting_out: ResMut<SittingOut>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let mode = start.0.clone();
    let config = || ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..Default::default()
    };
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let result: Result<u64, String> = match &mode {
        NetMode::Solo => Ok(HOST_ID),
        NetMode::Host { port, code } => parse_code(code)
            .and_then(|code| {
                let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, *port))
                    .map_err(|e| format!("couldn't host on port {port}: {e}"))?;
                let config_server = ServerConfig {
                    current_time: now,
                    max_clients: MAX_CLIENTS,
                    protocol_id: PROTOCOL_ID,
                    authentication: ServerAuthentication::Secure { private_key: room_key(&code) },
                    // Joiners' tokens name the sentinel, whatever address they dial.
                    public_addresses: vec![SENTINEL],
                };
                NetcodeServerTransport::new(config_server, socket).map_err(|e| e.to_string())
            })
            .map(|transport| {
                commands.insert_resource(RenetServer::new(config()));
                commands.insert_resource(transport);
                // Printed too, for whoever started the host from a terminal.
                println!("hosting on UDP port {port}, room code {}", show_code(code));
                info!("hosting on UDP port {port}, room code {}", show_code(code));
                HOST_ID
            }),
        NetMode::Join { addr, code } => {
            // Random, with the top bit set: well clear of the host's own ids
            // (`HOST_ID` and up), which the host refuses anyway.
            let id = u64::from_le_bytes(generate_random_bytes()) | 1 << 63;
            parse_code(code)
                .and_then(|code| {
                    let token = ConnectToken::generate(
                        now,
                        PROTOCOL_ID,
                        TOKEN_LIFE,
                        id,
                        TOKEN_TIMEOUT,
                        // The host only accepts tokens naming the sentinel (see `SENTINEL`).
                        vec![*addr, SENTINEL],
                        None,
                        &room_key(&code),
                    )
                    .map_err(|e| e.to_string())?;
                    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
                    let authentication = ClientAuthentication::Secure { connect_token: token };
                    NetcodeClientTransport::new(now, authentication, socket).map_err(|e| e.to_string())
                })
                .map(|transport| {
                    commands.insert_resource(RenetClient::new(config()));
                    commands.insert_resource(transport);
                    info!("connecting to {addr}");
                    id
                })
        }
    };

    match result {
        Ok(id) => {
            notice.0 = None;
            *connection = Connection::default();
            sitting_out.0 = false;
            commands.insert_resource(LocalId(id));
            // A host gathers the crew in the lobby first. Joiners wait in the
            // menu until the host's world arrives, then follow it.
            match &mode {
                NetMode::Solo => next_state.set(GameState::Playing),
                NetMode::Host { .. } => next_state.set(GameState::Lobby),
                NetMode::Join { .. } => {}
            }
            commands.insert_resource(mode);
        }
        Err(message) => {
            warn!("{message}");
            notice.0 = Some(message);
        }
    }
}

fn end_session(
    end: On<EndSession>,
    mut commands: Commands,
    mut server: Option<ResMut<RenetServer>>,
    mut server_transport: Option<ResMut<NetcodeServerTransport>>,
    client_transport: Option<ResMut<NetcodeClientTransport>>,
    world: Query<Entity, Or<(With<Level>, With<Remote>, With<MissionStatus>)>>,
    mut notice: ResMut<Notice>,
    mut sitting_out: ResMut<SittingOut>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // Tell the other side right away instead of letting them time out.
    if let (Some(server), Some(transport)) = (server.as_deref_mut(), server_transport.as_deref_mut()) {
        transport.disconnect_all(server);
    }
    if let Some(mut transport) = client_transport {
        transport.disconnect();
    }
    commands.remove_resource::<RenetServer>();
    commands.remove_resource::<NetcodeServerTransport>();
    commands.remove_resource::<RenetClient>();
    commands.remove_resource::<NetcodeClientTransport>();

    for entity in &world {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(NetMode::Solo);
    commands.insert_resource(LocalId(HOST_ID));
    sitting_out.0 = false;
    if let Some(reason) = &end.0 {
        info!("session ended: {reason}");
    }
    notice.0.clone_from(&end.0);
    // `if_neq`: a failed join ends the session while already in the menu.
    NextState::set_if_neq(&mut *next_state, GameState::Menu);
}

/// Leaves the run for the lobby (see `DropOut`). Only from a run or its
/// results: the host has nothing to end elsewhere, and a joiner still waiting
/// in the menu has no seat to keep yet. A joiner may also drop out from the
/// lobby, calling off a `JoinRun` the host hasn't answered yet.
fn drop_out(
    _: On<DropOut>,
    mut commands: Commands,
    mode: Res<NetMode>,
    state: Res<State<GameState>>,
    mut sitting_out: ResMut<SittingOut>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let in_run = matches!(state.get(), GameState::Playing | GameState::Over);
    match *mode {
        NetMode::Solo => commands.trigger(EndSession(None)),
        // The run isn't settled: an abandoned run pays nobody. Joiners follow
        // the host's phase to the lobby.
        NetMode::Host { .. } if in_run => next_state.set(GameState::Lobby),
        NetMode::Host { .. } => {}
        NetMode::Join { .. } if *state.get() == GameState::Menu => {}
        NetMode::Join { .. } => {
            // Set first, so `follow_host_state` holds this machine in the
            // lobby even while the host hasn't taken its pirate out yet.
            sitting_out.0 = true;
            commands.write_message(TakePart(false));
            NextState::set_if_neq(&mut *next_state, GameState::Lobby);
        }
    }
}

/// A joiner sitting in the lobby asks for a pirate in the run under way. It
/// stays in the lobby until the pirate arrives (see `follow_host_state`).
fn join_run(_: On<JoinRun>, mut commands: Commands, mode: Res<NetMode>, mut sitting_out: ResMut<SittingOut>) {
    if matches!(*mode, NetMode::Join { .. }) {
        sitting_out.0 = false;
        commands.write_message(TakePart(true));
    }
}

/// What a joiner is told when the host says it's running another build.
fn wrong_build_message() -> String {
    format!("The host is running a different build than yours ({VERSION}). You both need the same build to play together")
}

/// What a joiner who had a connection is told on losing it. `waiting` means
/// they're still in the menu: the host's world never arrived, so if it hung up,
/// it most likely turned them away for their build and word of that got lost.
fn lost_host_message(reason: Option<NetcodeDisconnectReason>, waiting: bool) -> String {
    match reason {
        Some(NetcodeDisconnectReason::DisconnectedByServer) if waiting => {
            format!("The host turned you away. You may be on a different build ({VERSION})")
        }
        Some(NetcodeDisconnectReason::DisconnectedByServer) => "The host closed the connection".into(),
        _ => "Lost connection to the host".into(),
    }
}

/// Ends a joiner's session when the host says it's running another build.
///
/// This can't fight `watch_connection`. Word arrives in `PreUpdate`, so the
/// session is over before `watch_connection` looks, in `Update`. And once
/// `watch_connection` sees the link down, nothing more can arrive: it's then
/// the only one to speak, with its best guess.
fn on_protocol_mismatch(_: On<ProtocolMismatch>, mut commands: Commands, mode: Res<NetMode>) {
    if matches!(*mode, NetMode::Join { .. }) {
        commands.trigger(EndSession(Some(wrong_build_message())));
    }
}

/// Sends a joiner back to the menu if the host can't be reached, turns them
/// away or goes away.
fn watch_connection(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<NetMode>,
    state: Res<State<ClientState>>,
    game: Res<State<GameState>>,
    transport: Option<Res<NetcodeClientTransport>>,
    mut connection: ResMut<Connection>,
) {
    let NetMode::Join { addr, .. } = *mode else { return };
    let reason = transport.and_then(|t| t.disconnect_reason());
    match state.get() {
        ClientState::Connected => connection.connected = true,
        _ if connection.connected => {
            connection.connected = false;
            let waiting = *game.get() == GameState::Menu;
            commands.trigger(EndSession(Some(lost_host_message(reason, waiting))));
        }
        // The only refusal netcode sends: every seat is taken.
        _ if reason == Some(NetcodeDisconnectReason::ConnectionDenied) => {
            commands.trigger(EndSession(Some("The host's crew is full".into())));
        }
        _ => {
            // A wrong room code looks just like no host at all: a host can't
            // read the request, so it never answers.
            connection.waited += time.delta_secs();
            if connection.waited > CONNECT_TIMEOUT {
                commands.trigger(EndSession(Some(format!(
                    "No answer from a host at {addr}. Check the room code, that the host is running, and that its port is open"
                ))));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy_replicon::shared::backend::connected_client::NetworkId;

    use super::*;
    use crate::class::{CrewNames, Loadouts, LocalLoadout};
    use crate::crew::JoinOrder;
    use crate::room::NextSeed;
    use crate::{GamePlugins, Rng};

    /// The game with the network left out, as the menu leaves it.
    fn game() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)))
            .add_plugins(GamePlugins)
            .insert_resource(NextSeed(3))
            .insert_resource(Rng::new(3));
        app
    }

    /// A joiner waiting in the menu whose link to the host just went down, so
    /// `watch_connection` speaks up at its first chance.
    fn dropped_joiner() -> App {
        let mut app = game();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT);
        app.insert_resource(NetMode::Join { addr, code: "ABCDEFGH".into() });
        app.world_mut().resource_mut::<Connection>().connected = true;
        app
    }

    fn notice(app: &App) -> Option<&str> {
        app.world().resource::<Notice>().0.as_deref()
    }

    #[test]
    fn the_version_names_the_release_and_the_build() {
        assert!(VERSION.contains(env!("CARGO_PKG_VERSION")));
        assert!(VERSION.contains(env!("DOZD_BUILD")));
        assert!(!env!("DOZD_BUILD").is_empty());
    }

    #[test]
    fn being_turned_away_says_why_and_what_to_do() {
        let message = wrong_build_message();
        assert!(message.contains(VERSION));
        assert!(message.contains("different build"));
        assert!(message.contains("same build"));
    }

    #[test]
    fn losing_the_host_is_only_blamed_on_the_build_before_its_world_arrives() {
        let hung_up = Some(NetcodeDisconnectReason::DisconnectedByServer);
        let guess = lost_host_message(hung_up, true);
        assert!(guess.contains("turned you away") && guess.contains(VERSION));
        assert_eq!(lost_host_message(hung_up, false), "The host closed the connection");
        assert_eq!(lost_host_message(None, true), "Lost connection to the host");
        assert_eq!(lost_host_message(None, false), "Lost connection to the host");
    }

    #[test]
    fn notices_fit_the_menu() {
        // Two lines of 66 letters: 640 px at 16 px Fira Mono. Less, as lines
        // break between words.
        for message in [wrong_build_message(), lost_host_message(Some(NetcodeDisconnectReason::DisconnectedByServer), true)] {
            assert!(message.len() <= 120, "`{message}` won't fit");
        }
    }

    #[test]
    fn word_of_another_build_ends_the_session_and_stands() {
        let mut app = dropped_joiner();
        // Arrives as packets do, ahead of `watch_connection` in the same frame.
        app.add_systems(PreUpdate, |mut commands: Commands| commands.trigger(ProtocolMismatch));
        app.update();
        app.update();
        assert_eq!(notice(&app), Some(wrong_build_message().as_str()));
        assert_eq!(*app.world().resource::<NetMode>(), NetMode::Solo);
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Menu);
    }

    #[test]
    fn word_of_another_build_after_the_session_ended_changes_nothing() {
        let mut app = dropped_joiner();
        app.update();
        let first = notice(&app).expect("the dropped joiner is told").to_string();
        assert_eq!(*app.world().resource::<NetMode>(), NetMode::Solo);
        app.world_mut().trigger(ProtocolMismatch);
        app.update();
        assert_eq!(notice(&app), Some(first.as_str()));
    }

    #[test]
    fn a_joiner_turned_away_leaves_nothing_behind() {
        let mut app = game();
        app.world_mut().trigger(StartSession(NetMode::Solo));
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Playing);
        app.insert_resource(NetMode::Host { port: 0, code: String::new() });

        // Connected but not authorized: replicon delivers what such a client
        // sends, and its loadout goes out alongside the hash that fails.
        const ID: u64 = 1 << 63 | 9;
        let client = app.world_mut().spawn((ConnectedClient { max_size: 1200 }, NetworkId::new(ID))).id();
        let host_class = app.world().resource::<LocalLoadout>().0.class;
        let class = *PirateClass::ALL.iter().find(|c| **c != host_class).unwrap();
        let message = ChooseLoadout(Loadout { name: "Zed".into(), ..Loadout::new(class) });
        app.world_mut().write_message(FromClient { client_id: ClientId::Client(client), message });
        app.update();
        app.update();
        assert!(app.world().resource::<Loadouts>().0.contains_key(&ID), "held while they're connected");

        let mut players = app.world_mut().query::<&Player>();
        assert!(!players.iter(app.world()).any(|p| p.owner == ID), "no pirate");
        let mut rosters = app.world_mut().query::<&Roster>();
        let roster = rosters.single(app.world()).expect("the host keeps a roster");
        assert!(!roster.0.iter().any(|c| c.owner == ID), "not on the roster");

        // The handshake's refusal: gone the same frame it was checked.
        app.world_mut().write_message(DisconnectRequest { client });
        app.update();
        assert!(app.world().get_entity(client).is_err(), "disconnected");
        assert!(!app.world().resource::<Loadouts>().0.contains_key(&ID));
        assert!(!app.world().resource::<CrewNames>().0.contains_key(&ID));
        assert!(!app.world().resource::<JoinOrder>().0.contains(&ID));
    }

    /// What this machine has asked the host about taking part, oldest first.
    fn asked(app: &App) -> Vec<bool> {
        app.world().resource::<Messages<TakePart>>().iter_current_update_messages().map(|m| m.0).collect()
    }

    #[test]
    fn a_joiner_dropping_out_sits_out_in_the_lobby_and_can_ask_back_in() {
        let mut app = dropped_joiner();
        app.world_mut().resource_mut::<Connection>().connected = false;
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Playing);

        app.world_mut().trigger(DropOut);
        app.world_mut().flush();
        assert!(app.world().resource::<SittingOut>().0);
        assert_eq!(asked(&app), [false], "the host is told to take their pirate out");
        app.update();
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Lobby);
        assert!(matches!(*app.world().resource::<NetMode>(), NetMode::Join { .. }), "still in the crew");

        app.world_mut().trigger(JoinRun);
        app.world_mut().flush();
        assert!(!app.world().resource::<SittingOut>().0);
        assert!(asked(&app).ends_with(&[true]));
    }

    #[test]
    fn a_joiner_still_waiting_in_the_menu_has_nothing_to_drop_out_of() {
        let mut app = dropped_joiner();
        app.world_mut().resource_mut::<Connection>().connected = false;
        app.world_mut().trigger(DropOut);
        app.update();
        assert!(!app.world().resource::<SittingOut>().0);
        assert!(asked(&app).is_empty());
        assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::Menu);
    }

    #[test]
    fn only_joiners_ask_into_a_run() {
        let mut app = game();
        app.world_mut().trigger(StartSession(NetMode::Solo));
        app.update();
        app.world_mut().trigger(JoinRun);
        app.world_mut().flush();
        assert!(asked(&app).is_empty());
    }

    #[test]
    fn room_codes_round_trip_and_key_sessions() {
        let code = room_code();
        assert_eq!(parse_code(&show_code(&code).to_lowercase()), Ok(code.clone()));
        assert!(parse_code("").is_err());
        assert!(parse_code("ABCD-EFG0").is_err(), "0 isn't in a code");
        assert_eq!(room_key(&code), room_key(&code), "every machine derives the same key");
        assert_ne!(room_key("ABCDEFGH"), room_key("ABCDEFGJ"), "one letter off is another key");
    }

    /// Whether `transform` comes through `NetTransform` as it went in.
    fn survives_the_wire(transform: Transform) -> bool {
        let back = Transform::from(NetTransform::from(transform));
        back.translation == transform.translation
            && (back.rotation * Vec3::X).abs_diff_eq(transform.rotation * Vec3::X, 1e-5)
            && (back.rotation * Vec3::Z).abs_diff_eq(transform.rotation * Vec3::Z, 1e-5)
            && back.scale.abs_diff_eq(transform.scale, 1e-6)
    }

    #[test]
    fn flat_transforms_survive_the_wire() {
        for angle in [0.0, 0.5, -2.0, std::f32::consts::PI, 3.0] {
            let turned = Transform::from_xyz(-120.5, 33.25, 15.0).with_rotation(Quat::from_rotation_z(angle));
            assert!(survives_the_wire(turned), "{angle}");
            assert!(survives_the_wire(turned.with_scale(Vec3::splat(1.3))));
        }
        assert!(!survives_the_wire(Transform::from_scale(Vec3::new(2.0, 1.0, 1.0))), "uneven scale is lost");
    }

    #[test]
    fn transforms_reach_joiners_through_the_compact_form() {
        use bevy_replicon::test_app::ServerTestAppExt;

        let (mut host, mut joiner) = (App::new(), App::new());
        for app in [&mut host, &mut joiner] {
            app.add_plugins((MinimalPlugins, StatesPlugin, RepliconPlugins.set(ServerPlugin::new(PostUpdate))))
                .replicate_as::<Transform, NetTransform>()
                .finish();
        }
        host.connect_client(&mut joiner);
        let at = Transform::from_xyz(-40.5, 120.25, 15.0)
            .with_rotation(Quat::from_rotation_z(2.5))
            .with_scale(Vec3::splat(1.3));
        let shot = host.world_mut().spawn((Replicated, at)).id();
        let mirrored = |joiner: &mut App| {
            let mut remote = joiner.world_mut().query_filtered::<&Transform, With<Remote>>();
            remote.single(joiner.world()).copied().expect("one entity across")
        };
        host.update();
        host.exchange_with_client(&mut joiner);
        joiner.update();
        let seen = mirrored(&mut joiner);
        assert_eq!(seen.translation, at.translation);
        assert!(survives_the_wire(seen) && seen.scale.abs_diff_eq(at.scale, 1e-6));
        assert!((seen.rotation * Vec3::X).abs_diff_eq(at.rotation * Vec3::X, 1e-5));

        // And as it moves.
        host.world_mut().get_mut::<Transform>(shot).unwrap().translation.x += 9.0;
        host.update();
        host.exchange_with_client(&mut joiner);
        joiner.update();
        assert_eq!(mirrored(&mut joiner).translation.x, at.translation.x + 9.0);
    }

    #[test]
    fn everything_replicated_in_a_run_is_flat() {
        let mut app = game();
        app.world_mut().trigger(StartSession(NetMode::Solo));
        for _ in 0..120 {
            app.update();
        }
        let mut replicated = app.world_mut().query_filtered::<&Transform, With<Replicated>>();
        let transforms: Vec<Transform> = replicated.iter(app.world()).copied().collect();
        assert!(transforms.len() > 10, "a deck's worth");
        assert!(transforms.into_iter().all(survives_the_wire));
    }

    #[test]
    fn every_letter_of_a_room_code_is_as_likely() {
        let mut counts = [0u32; CODE_ALPHABET.len()];
        let codes = 20_000;
        for _ in 0..codes {
            let code = room_code();
            assert_eq!(code.len(), CODE_LEN);
            for b in code.bytes() {
                counts[CODE_ALPHABET.iter().position(|&a| a == b).unwrap()] += 1;
            }
        }
        // About 5161 each, give or take 72: a plain `%` over bytes would put
        // about 5625 on each of the first eight, well past this.
        let each = (codes * CODE_LEN) as f32 / CODE_ALPHABET.len() as f32;
        assert!(counts.iter().all(|&n| (n as f32 - each).abs() < each * 0.07), "{counts:?}");
    }
}
