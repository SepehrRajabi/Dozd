use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication, ServerConfig,
};
use bevy_replicon_renet::renet::ConnectionConfig;
use bevy_replicon_renet::{RenetChannelsExt, RenetClient, RenetServer, RepliconRenetPlugins};
use serde::{Deserialize, Serialize};

use crate::combat::{Fx, Health, ProjectileLook};
use crate::drops::EnemyDrop;
use crate::enemies::EnemyLook;
use crate::guns::Arsenal;
use crate::loot::LootKind;
use crate::mission::MissionStatus;
use crate::perks::{PerkPickup, Perks};
use crate::player::{Aim, Dash, Player, Walking};
use crate::{GameState, Level};

pub const DEFAULT_PORT: u16 = 5000;
const PROTOCOL_ID: u64 = 0xD02D;
const MAX_CLIENTS: usize = 7;
/// Give up on reaching a host after this long.
const CONNECT_TIMEOUT: f32 = 10.0;
/// Player id of the host (or solo player); joining clients get random non-zero ids.
pub const HOST_ID: u64 = 0;

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub enum NetMode {
    Solo,
    Host { port: u16 },
    Join { addr: SocketAddr },
}

impl NetMode {
    /// `dozd` | `dozd host [port]` | `dozd join <address>`; `None` means show the menu.
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
                Ok(Some(Self::Host { port }))
            }
            Some("join") => {
                let raw = args.get(1).ok_or("usage: dozd join <ip or hostname>[:port]")?;
                Ok(Some(Self::Join { addr: resolve(raw)? }))
            }
            Some(other) => Err(format!(
                "unknown mode `{other}`\nusage: dozd [solo | host [port] | join <address>]"
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
    pub restart: bool,
}

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
            .add_client_message::<PlayerInput>(Channel::Ordered)
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

    let result: Result<u64, String> = match mode {
        NetMode::Solo => Ok(HOST_ID),
        NetMode::Host { port } => UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
            .map_err(|e| format!("couldn't host on port {port}: {e}"))
            .and_then(|socket| {
                let config_server = ServerConfig {
                    current_time: now,
                    max_clients: MAX_CLIENTS,
                    protocol_id: PROTOCOL_ID,
                    authentication: ServerAuthentication::Unsecure,
                    public_addresses: Default::default(),
                };
                NetcodeServerTransport::new(config_server, socket).map_err(|e| e.to_string())
            })
            .map(|transport| {
                commands.insert_resource(RenetServer::new(config()));
                commands.insert_resource(transport);
                info!("hosting on UDP port {port}");
                HOST_ID
            }),
        NetMode::Join { addr } => {
            let id = (now.as_nanos() as u64).max(1);
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
                .map_err(|e| e.to_string())
                .and_then(|socket| {
                    let authentication = ClientAuthentication::Unsecure {
                        client_id: id,
                        protocol_id: PROTOCOL_ID,
                        server_addr: addr,
                        user_data: None,
                    };
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
            // Joiners wait in the menu until the host's world arrives.
            if mode.is_authority() {
                next_state.set(GameState::Playing);
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
    notice.0.clone_from(&end.0);
    // `if_neq`: a failed join ends the session while already in the menu.
    NextState::set_if_neq(&mut *next_state, GameState::Menu);
}

/// Sends a joiner back to the menu if the host can't be reached or goes away.
fn watch_connection(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<NetMode>,
    state: Res<State<ClientState>>,
    mut connection: ResMut<Connection>,
) {
    let NetMode::Join { addr } = *mode else { return };
    match state.get() {
        ClientState::Connected => connection.connected = true,
        _ if connection.connected => {
            connection.connected = false;
            commands.trigger(EndSession(Some("Lost connection to the host".into())));
        }
        _ => {
            connection.waited += time.delta_secs();
            if connection.waited > CONNECT_TIMEOUT {
                commands.trigger(EndSession(Some(format!(
                    "Couldn't reach a host at {addr}. Is it running, and is the port open?"
                ))));
            }
        }
    }
}
