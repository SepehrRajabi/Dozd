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
use crate::{GameState, Level};

pub const DEFAULT_PORT: u16 = 5000;
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
    generate_random_bytes::<CODE_LEN>()
        .iter()
        .map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char)
        .collect()
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

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RepliconPlugins, RepliconRenetPlugins))
            .insert_resource(NetMode::Solo)
            .insert_resource(LocalId(HOST_ID))
            .init_resource::<Notice>()
            .init_resource::<Connection>()
            // Registration order must match on every machine; keep it all here.
            .replicate::<Transform>()
            .replicate::<Player>()
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
            .add_server_event::<Fx>(Channel::Unordered)
            .add_observer(start_session)
            .add_observer(end_session)
            .add_systems(Update, watch_connection);
    }
}

fn start_session(
    start: On<StartSession>,
    mut commands: Commands,
    channels: Res<RepliconChannels>,
    mut notice: ResMut<Notice>,
    mut connection: ResMut<Connection>,
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
    if let Some(reason) = &end.0 {
        info!("session ended: {reason}");
    }
    notice.0.clone_from(&end.0);
    // `if_neq`: a failed join ends the session while already in the menu.
    NextState::set_if_neq(&mut *next_state, GameState::Menu);
}

/// Sends a joiner back to the menu if the host can't be reached, turns them
/// away or goes away.
fn watch_connection(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<NetMode>,
    state: Res<State<ClientState>>,
    transport: Option<Res<NetcodeClientTransport>>,
    mut connection: ResMut<Connection>,
) {
    let NetMode::Join { addr, .. } = *mode else { return };
    let reason = transport.and_then(|t| t.disconnect_reason());
    match state.get() {
        ClientState::Connected => connection.connected = true,
        _ if connection.connected => {
            connection.connected = false;
            let message = if reason == Some(NetcodeDisconnectReason::DisconnectedByServer) {
                "The host closed the connection"
            } else {
                "Lost connection to the host"
            };
            commands.trigger(EndSession(Some(message.into())));
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
    use super::*;

    #[test]
    fn room_codes_round_trip_and_key_sessions() {
        let code = room_code();
        assert_eq!(parse_code(&show_code(&code).to_lowercase()), Ok(code.clone()));
        assert!(parse_code("").is_err());
        assert!(parse_code("ABCD-EFG0").is_err(), "0 isn't in a code");
        assert_eq!(room_key(&code), room_key(&code), "every machine derives the same key");
        assert_ne!(room_key("ABCDEFGH"), room_key("ABCDEFGJ"), "one letter off is another key");
    }
}
