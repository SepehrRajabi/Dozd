use std::f32::consts::TAU;
use std::net::{IpAddr, SocketAddr};

use bevy::app::AppExit;
use bevy::clipboard::Clipboard;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::abilities::{Active, Passive};
use crate::boss::{KEEPER_COLOR, KeeperKind, PRISM_COLOR};
use crate::class::{LocalLoadout, Loadout, NAME_LEN, PirateClass, clean_name, name_allows};
use crate::guns::GunKind;
use crate::hud::LastDevice;
use crate::crew::Roster;
use crate::enemies::EnemyKind;
use crate::mission::MissionStatus;
use crate::net::{
    CODE_LEN, ChooseLoadout, Connection, DEFAULT_PORT, DropOut, EndSession, HOST_ID, JoinRun, LocalId, NetMode,
    Notice, StartSession, VERSION, local_ip, parse_code, parse_port, resolve, room_code, show_code, tailscale_ip,
};
use crate::loot::LootKind;
use crate::perks::PerkKind;
use crate::player::Player;
use crate::settings::{Settings, read_config, write_config};
use crate::vault::Vault;
use crate::hud::{self, HudSlot};
use crate::room;
use crate::{GameState, PIXEL_SCALE};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.35);
const GOLD_FAINT: Color = Color::srgba(1.0, 0.82, 0.35, 0.3);
const BUTTON: Color = Color::srgb(0.07, 0.08, 0.14);
const BUTTON_HOVER: Color = Color::srgb(0.14, 0.15, 0.25);
const BUTTON_PRESSED: Color = Color::srgb(0.3, 0.24, 0.1);
const TEXT: Color = Color::srgb(0.9, 0.92, 1.0);
const DIM: Color = Color::srgba(0.8, 0.85, 1.0, 0.55);
const ERROR: Color = Color::srgb(1.0, 0.45, 0.4);
/// How long the "press ESC again" prompt stays armed.
const LEAVE_WINDOW: f32 = 2.5;
/// Seconds between looking up this machine's addresses, so Tailscale going up
/// or down shows while a menu is open. Each lookup is only a UDP connect, which
/// sends nothing.
const ADDRESS_REFRESH: f32 = 2.0;
/// How long COPY reads COPIED after a click.
const COPIED_TIME: f32 = 1.5;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // Last launch's pick, ready before the first screen is drawn.
        app.insert_resource(LocalLoadout(Loadout::load().unwrap_or_default()))
            .add_sub_state::<MenuScreen>()
            .init_resource::<MenuMemory>()
            .init_resource::<LeavePrompt>()
            .init_resource::<HostAddresses>()
            .init_resource::<CopyFeedback>()
            .add_observer(run_menu_command)
            .add_systems(Startup, spawn_session_hud)
            .add_systems(OnEnter(GameState::Menu), (spawn_backdrop, show_cursor))
            .add_systems(OnExit(GameState::Menu), hide_cursor)
            .add_systems(OnEnter(GameState::Lobby), (lobby_screen, spawn_backdrop, show_cursor))
            .add_systems(OnExit(GameState::Lobby), hide_cursor)
            .add_systems(OnEnter(GameState::Playing), unzoom)
            .add_systems(OnEnter(MenuScreen::Main), main_screen)
            .add_systems(OnEnter(MenuScreen::Multiplayer), multiplayer_screen)
            .add_systems(OnEnter(MenuScreen::Extras), extras_screen)
            .add_systems(OnEnter(MenuScreen::Enemies), enemies_screen)
            .add_systems(OnEnter(MenuScreen::Host), host_screen)
            .add_systems(OnEnter(MenuScreen::Join), join_screen)
            .add_systems(OnEnter(MenuScreen::Loadout), loadout_screen)
            .add_systems(OnEnter(MenuScreen::Perks), perks_screen)
            .add_systems(OnEnter(MenuScreen::Settings), settings_screen)
            .add_systems(
                Update,
                (
                    edit_fields,
                    draw_fields,
                    update_host_hint,
                    back_on_escape,
                    update_setting_values.run_if(resource_changed::<Settings>),
                )
                    .run_if(in_state(GameState::Menu)),
            )
            .add_systems(
                Update,
                (
                    button_styles,
                    button_actions,
                    update_status,
                    fit_menus,
                    hold_camera,
                    animate_backdrop,
                    fade_backdrop,
                    update_copy_buttons,
                    dress_backdrop_pirate.run_if(resource_changed::<LocalLoadout>),
                    (choice_actions, style_choices, show_ability_choices, update_slot_count).chain(),
                )
                    .run_if(in_state(GameState::Menu).or_else(in_state(GameState::Lobby))),
            )
            .add_systems(
                Update,
                (update_lobby, send_lobby_loadout.run_if(resource_changed::<LocalLoadout>))
                    .run_if(in_state(GameState::Lobby)),
            )
            .add_systems(
                Update,
                (leave_on_escape, update_session_hud).run_if(not(in_state(GameState::Menu))),
            )
            .add_systems(PreUpdate, refresh_addresses);
    }
}

#[derive(SubStates, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[source(GameState = GameState::Menu)]
enum MenuScreen {
    #[default]
    Main,
    /// Host or join.
    Multiplayer,
    Host,
    Join,
    Loadout,
    /// Perks and enemies.
    Extras,
    Perks,
    Enemies,
    Settings,
}

impl MenuScreen {
    /// Where BACK leads from here.
    fn parent(self) -> Self {
        match self {
            Self::Host | Self::Join => Self::Multiplayer,
            Self::Perks | Self::Enemies => Self::Extras,
            _ => Self::Main,
        }
    }
}

#[derive(Component, Event, Clone, Copy, PartialEq, Eq, Debug)]
enum MenuCommand {
    Solo,
    OpenMultiplayer,
    OpenHost,
    OpenJoin,
    OpenLoadout,
    OpenExtras,
    OpenPerks,
    OpenEnemies,
    OpenSettings,
    Toggle(Setting),
    Quit,
    StartHost,
    Connect,
    Back,
    /// The host sets off from the lobby.
    StartRun,
    /// Back to the main menu from the lobby.
    Leave,
    /// Puts the room code on the clipboard, from the host screen or the lobby.
    CopyCode,
    /// A joiner waiting in the lobby drops into the crew's run.
    JoinRun,
}

/// What the player typed last time, so hosting or rejoining is one click,
/// even after restarting the game.
#[derive(Resource)]
struct MenuMemory {
    port: String,
    address: String,
    /// The last room code typed in. Only kept until the game closes: a host
    /// gets a new one each time, so an old one would only mislead.
    code: String,
}

impl Default for MenuMemory {
    fn default() -> Self {
        let mut memory = Self { port: DEFAULT_PORT.to_string(), address: String::new(), code: String::new() };
        for (key, value) in read_config(Self::FILE) {
            match key.as_str() {
                "port" if parse_port(&value).is_ok() => memory.port = value,
                "address" => memory.address = value,
                _ => {}
            }
        }
        memory
    }
}

impl MenuMemory {
    const FILE: &str = "multiplayer.txt";

    fn save(&self) {
        write_config(Self::FILE, &[("port", self.port.clone()), ("address", self.address.clone())]);
    }
}

/// What a text field holds, which decides what can be typed into it. Tab
/// steps through a screen's fields in this order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum FieldKind {
    Port,
    Address,
    /// Typed in capitals, however it's keyed, with the dash filled in (see
    /// `dash_code`).
    Code,
    /// Last, so Tab still goes from the address straight to the code.
    Name,
}

impl FieldKind {
    fn allows(self, c: char) -> bool {
        match self {
            Self::Port => c.is_ascii_digit(),
            Self::Address => c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-'),
            // The dash comes by itself, so a typed one would only be in the way.
            Self::Code => c.is_ascii_alphanumeric(),
            Self::Name => name_allows(c),
        }
    }
}

/// A room code's letters with the dash where `show_code` puts it, put in as
/// soon as the first half is typed so it reads like the host's: `K7QF-`, then
/// `K7QF-29XA`.
fn dash_code(letters: &[char]) -> String {
    let half = CODE_LEN / 2;
    let mut code: String = letters.iter().take(half).collect();
    if letters.len() >= half {
        code.push('-');
        code.extend(&letters[half..]);
    }
    code
}

#[derive(Component)]
struct TextField {
    value: String,
    /// The most it holds: characters, or a code's letters.
    max_len: usize,
    kind: FieldKind,
    /// Pressing Enter in the field runs this.
    submit: MenuCommand,
    /// Takes the typing. One field on a screen has it; Tab or a click moves it.
    focused: bool,
    /// Where typing goes, as a byte offset into `value`.
    caret: usize,
}

impl TextField {
    fn new(kind: FieldKind, value: &str, max_len: usize, submit: MenuCommand, focused: bool) -> Self {
        let mut field = Self { value: String::new(), max_len, kind, submit, focused, caret: 0 };
        field.insert(value);
        field
    }

    /// What's edited, one unit at a time, and how many units come before the
    /// caret: a code's letters without the dash, so the dash is never in the
    /// way of the caret or Backspace, or else every character.
    fn units(&self) -> (Vec<char>, usize) {
        let unit = |c: &char| self.kind != FieldKind::Code || *c != '-';
        // A caret off a character boundary (never set so) counts from the end.
        let before = self.value.get(..self.caret).unwrap_or(&self.value);
        (self.value.chars().filter(unit).collect(), before.chars().filter(unit).count())
    }

    /// Writes the units back, with the caret after the first `at` of them.
    fn set_units(&mut self, units: &[char], at: usize) {
        let at = at.min(units.len());
        if self.kind == FieldKind::Code {
            self.value = dash_code(units);
            self.caret = dash_code(&units[..at]).len();
        } else {
            self.value = units.iter().collect();
            self.caret = units[..at].iter().map(|c| c.len_utf8()).sum();
        }
    }

    /// Adds what was typed or pasted at the caret, as far as this field takes it.
    fn insert(&mut self, text: &str) {
        let (mut units, mut at) = self.units();
        for c in text.chars() {
            let c = if self.kind == FieldKind::Code { c.to_ascii_uppercase() } else { c };
            if self.kind.allows(c) && units.len() < self.max_len {
                units.insert(at, c);
                at += 1;
            }
        }
        self.set_units(&units, at);
    }

    /// Backspace: the character before the caret. Right after a code's dash,
    /// the letter before it, and the dash goes with it.
    fn backspace(&mut self) {
        let (mut units, at) = self.units();
        if at > 0 {
            units.remove(at - 1);
            self.set_units(&units, at - 1);
        }
    }

    /// Delete: the character after the caret.
    fn delete(&mut self) {
        let (mut units, at) = self.units();
        if at < units.len() {
            units.remove(at);
            self.set_units(&units, at);
        }
    }

    /// Moves the caret `by` characters, stopping at either end.
    fn step(&mut self, by: isize) {
        let (units, at) = self.units();
        self.set_units(&units, at.saturating_add_signed(by));
    }

    fn home(&mut self) {
        let (units, _) = self.units();
        self.set_units(&units, 0);
    }

    fn end(&mut self) {
        let (units, _) = self.units();
        self.set_units(&units, units.len());
    }

    /// The text either side of the caret, to draw it between them.
    fn split(&self) -> (&str, &str) {
        self.value.split_at_checked(self.caret).unwrap_or((&self.value, ""))
    }
}

/// The text before or after a field's caret.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum FieldText {
    BeforeCaret,
    AfterCaret,
}

/// Blinks on its own, so the text beside it never changes with the blink.
#[derive(Component)]
struct Caret;

/// Shown steadily while the field is empty.
#[derive(Component)]
struct Placeholder;

#[derive(Component)]
struct HostHint;

/// The room code the crew will need, shown on the host screen before hosting
/// starts so it can be passed round early; START HOSTING uses it.
#[derive(Component)]
struct HostCode(String);

/// Under the host's addresses: how a crew far away gets in.
#[derive(Component)]
struct TailscaleNote;

/// The text on a COPY button, which reads COPIED for a moment after a click.
#[derive(Component)]
struct CopyLabel;

/// This machine's addresses for the crew to join on, looked up afresh every
/// `ADDRESS_REFRESH` seconds while they're on screen.
#[derive(Resource, Default)]
struct HostAddresses {
    lan: Option<IpAddr>,
    /// While Tailscale is up.
    tailscale: Option<IpAddr>,
    /// Seconds to the next lookup.
    next: f32,
}

impl HostAddresses {
    fn line(&self, port: u16) -> String {
        join_addresses(self.lan, self.tailscale, port, "  or over Tailscale  ")
    }
}

/// How the crew reaches this host, each address with `:port` unless the port
/// is the default: the LAN address, and while Tailscale is up the tailnet one
/// after `or`. Just the one when they're the same.
fn join_addresses(lan: Option<IpAddr>, tailscale: Option<IpAddr>, port: u16, or: &str) -> String {
    let at = |ip: IpAddr| if port == DEFAULT_PORT { ip.to_string() } else { SocketAddr::new(ip, port).to_string() };
    match (lan, tailscale) {
        (Some(lan), Some(tailscale)) if lan != tailscale => format!("{}{or}{}", at(lan), at(tailscale)),
        (_, Some(tailscale)) => format!("{}  (Tailscale)", at(tailscale)),
        (Some(lan), None) => at(lan),
        (None, None) => "<your IP>".into(),
    }
}

/// When the last COPY happened, and whether the clipboard took it.
#[derive(Resource, Default)]
struct CopyFeedback {
    timer: f32,
    failed: bool,
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct Backdrop {
    base: Vec3,
    phase: f32,
    /// Loot circles the pirate; everything else just bobs.
    orbit: Option<(Vec2, f32)>,
}

/// A pickable card on the loadout screen; styled by `style_choices`, not `button_styles`.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Class(PirateClass),
    Passive(Passive),
    Active(Active),
    Gun(GunKind),
}

/// Under a class card in the lobby: who has the class, if anyone.
#[derive(Component)]
struct ClassNote(PirateClass);

/// Shows which slot a gun card's gun sits in.
#[derive(Component)]
struct SlotBadge(GunKind);

#[derive(Component)]
struct SlotCount;

/// The backdrop pirate and the gun in their hands, dressed as the chosen loadout.
#[derive(Component)]
enum BackdropPirate {
    Body,
    Gun,
}

/// A row on the settings screen; clicking it flips or cycles the value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Setting {
    Reinforcements,
    Vsync,
    RefreshRate,
}

impl Setting {
    fn value(self, settings: &Settings) -> String {
        let on = |b: bool| if b { "ON" } else { "OFF" }.to_string();
        match self {
            Self::Reinforcements => on(settings.reinforcements),
            Self::Vsync => on(settings.vsync),
            Self::RefreshRate => settings.refresh_label(),
        }
    }
}

#[derive(Component)]
struct SettingValue(Setting);

#[derive(Component)]
struct SessionText;

#[derive(Component)]
struct LootLeftText;

#[derive(Component)]
struct LeaveText;

#[derive(Component)]
struct LeavePanel;

#[derive(Resource, Default)]
struct LeavePrompt {
    timer: f32,
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

/// Full-screen column with the logo on top; each screen fills in the rest.
fn screen_root(commands: &mut Commands, assets: &AssetServer, screen: MenuScreen) -> Entity {
    commands
        .spawn((
            DespawnOnExit(screen),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(56.0)),
                row_gap: Val::Px(14.0),
                ..default()
            },
            children![
                (
                    ImageNode::new(assets.load("sprites/ui/logo.png")),
                    Node {
                        width: Val::Px(38.0 * 10.0),
                        height: Val::Px(14.0 * 10.0),
                        ..default()
                    },
                ),
                (
                    Text::new("SPACE PIRATE SALVAGE"),
                    TextFont::from_font_size(18.0),
                    TextColor(GOLD),
                ),
                (
                    Text::new("Loot together. Leave together. Or don't."),
                    TextFont::from_font_size(15.0),
                    TextColor(DIM),
                    Node {
                        margin: UiRect::bottom(Val::Px(18.0)),
                        ..default()
                    },
                ),
            ],
        ))
        .id()
}

fn button(label: &str, command: MenuCommand) -> impl Bundle {
    (
        Button,
        command,
        Node {
            width: Val::Px(320.0),
            height: Val::Px(46.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(GOLD_FAINT),
        children![(Text::new(label), TextFont::from_font_size(20.0), TextColor(TEXT))],
    )
}

fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), TextFont::from_font_size(size), TextColor(color))
}

/// A big room code to read out, which copies itself when clicked, and the
/// small COPY button beside it doing the same.
fn copyable_code(row: &mut ChildSpawnerCommands, code: impl Bundle) {
    row.spawn((code, Button, MenuCommand::CopyCode));
    row.spawn((
        Button,
        MenuCommand::CopyCode,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(2.0)),
            align_self: AlignSelf::Center,
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(GOLD_FAINT),
        children![(CopyLabel, label("COPY", 13.0, TEXT), TextLayout::no_wrap())],
    ));
}

/// A box to type into, `width` wide; clicking it gives it the typing.
fn text_field(
    kind: FieldKind,
    value: &str,
    placeholder: &'static str,
    max_len: usize,
    width: f32,
    focused: bool,
    submit: MenuCommand,
) -> impl Bundle {
    let field = TextField::new(kind, value, max_len, submit, focused);
    let (before, after) = field.split();
    let part = |part: FieldText, text: &str| {
        (part, Text::new(text), TextFont::from_font_size(20.0), TextColor(TEXT), TextLayout::no_wrap())
    };
    let children = children![
        part(FieldText::BeforeCaret, before),
        (
            Caret,
            Node {
                width: Val::Px(2.0),
                height: Val::Px(22.0),
                margin: UiRect::horizontal(Val::Px(1.0)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(GOLD),
        ),
        part(FieldText::AfterCaret, after),
        // Out of the flow: hidden, it would still crowd what's typed.
        (
            Placeholder,
            Text::new(placeholder),
            TextFont::from_font_size(20.0),
            TextColor(DIM),
            TextLayout::no_wrap(),
            Node { position_type: PositionType::Absolute, left: Val::Px(15.0), ..default() },
        ),
    ];
    (
        field,
        Interaction::default(),
        Node {
            width: Val::Px(width),
            height: Val::Px(44.0),
            padding: UiRect::horizontal(Val::Px(12.0)),
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.02, 0.02, 0.05)),
        BorderColor::all(GOLD),
        children,
    )
}

/// A box and the label over it, for laying boxes side by side: both screens
/// with a name box only just fit a 720 px tall window.
fn field_column() -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }
}

/// The box for this player's display name, and the label over it.
fn name_field(parent: &mut ChildSpawnerCommands, loadout: &LocalLoadout, submit: MenuCommand) {
    parent.spawn(field_column()).with_children(|name| {
        name.spawn(label("Your name  (optional)", 14.0, DIM));
        name.spawn(text_field(FieldKind::Name, &loadout.0.name, "Mira", NAME_LEN, 200.0, false, submit));
    });
}

/// Where notices show. Wide enough that a long one, like being refused for
/// running another build, wraps to two lines: a third wouldn't leave the main
/// menu any room at 720 px tall.
fn status_line() -> impl Bundle {
    (
        StatusText,
        Text::new(""),
        TextFont::from_font_size(16.0),
        TextColor(ERROR),
        TextLayout::justify(Justify::Center),
        Node {
            max_width: Val::Px(640.0),
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        },
    )
}

/// Small print in the bottom-left corner, clear of the centred column. Its last
/// line is always the build, so players can compare theirs with a host's.
fn corner_note(text: impl Into<String>) -> impl Bundle {
    (
        label(text, 13.0, DIM),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(14.0),
            left: Val::Px(18.0),
            ..default()
        },
    )
}

fn main_screen(mut commands: Commands, assets: Res<AssetServer>, loadout: Res<LocalLoadout>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Main);
    let loadout_label = format!("LOADOUT: {}", loadout.0.class.name());
    commands.entity(root).with_children(|parent| {
        parent.spawn(button("SOLO RAID", MenuCommand::Solo));
        parent.spawn(button("MULTIPLAYER", MenuCommand::OpenMultiplayer));
        parent.spawn(button(&loadout_label, MenuCommand::OpenLoadout));
        parent.spawn(button("EXTRAS", MenuCommand::OpenExtras));
        parent.spawn(button("SETTINGS", MenuCommand::OpenSettings));
        parent.spawn(button("QUIT", MenuCommand::Quit));
        parent.spawn(status_line());
        // On two lines: as one, it would reach under a long notice.
        parent.spawn(corner_note(format!("LAN co-op for up to {} pirates\n{VERSION}", PirateClass::ALL.len())));
    });
}

fn multiplayer_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Multiplayer);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("MULTIPLAYER", 24.0, GOLD));
        parent.spawn(button("HOST A CREW", MenuCommand::OpenHost));
        parent.spawn(button("JOIN A CREW", MenuCommand::OpenJoin));
        parent.spawn(button("BACK", MenuCommand::Back));
    });
}

fn extras_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Extras);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("EXTRAS", 24.0, GOLD));
        parent.spawn(button("PERKS", MenuCommand::OpenPerks));
        parent.spawn(button("ENEMIES", MenuCommand::OpenEnemies));
        parent.spawn(button("BACK", MenuCommand::Back));
    });
}

fn settings_screen(mut commands: Commands, assets: Res<AssetServer>, settings: Res<Settings>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Settings);
    let rows = [
        (Setting::Reinforcements, "REINFORCEMENTS", "Guard waves arrive while the alarm is up, in solo raids. Always on in LAN games."),
        (Setting::Vsync, "VSYNC", "Wait for the monitor between frames. Stops tearing."),
        (Setting::RefreshRate, "REFRESH RATE", "Frames per second cap. With vsync on, the monitor's rate is the ceiling."),
    ];
    commands.entity(root).with_children(|parent| {
        for (setting, name, hint) in rows {
            parent.spawn(setting_row(setting, name, &setting.value(&settings)));
            parent.spawn((label(hint, 13.0, DIM), Node { margin: UiRect::bottom(Val::Px(6.0)), ..default() }));
        }
        parent.spawn(button("BACK", MenuCommand::Back));
    });
}

fn setting_row(setting: Setting, name: &str, value: &str) -> impl Bundle {
    (
        Button,
        MenuCommand::Toggle(setting),
        Node {
            width: Val::Px(440.0),
            height: Val::Px(46.0),
            padding: UiRect::horizontal(Val::Px(18.0)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(GOLD_FAINT),
        children![
            (Text::new(name), TextFont::from_font_size(20.0), TextColor(TEXT)),
            (SettingValue(setting), Text::new(value), TextFont::from_font_size(20.0), TextColor(GOLD)),
        ],
    )
}

fn host_screen(mut commands: Commands, assets: Res<AssetServer>, memory: Res<MenuMemory>, loadout: Res<LocalLoadout>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Host);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("HOST A CREW", 24.0, GOLD));
        parent
            .spawn(Node { column_gap: Val::Px(16.0), ..default() })
            .with_children(|row| {
                row.spawn(field_column()).with_children(|port| {
                    port.spawn(label("Port", 14.0, DIM));
                    port.spawn(text_field(FieldKind::Port, &memory.port, "5000", 5, 104.0, true, MenuCommand::StartHost));
                });
                name_field(row, &loadout, MenuCommand::StartHost);
            });
        parent.spawn((HostHint, label("", 16.0, TEXT)));
        let code = room_code();
        parent
            .spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Baseline, ..default() })
            .with_children(|row| {
                row.spawn(label("and room code", 16.0, TEXT));
                copyable_code(row, (label(show_code(&code), 30.0, GOLD), HostCode(code)));
            });
        parent.spawn((TailscaleNote, label("", 13.0, DIM)));
        parent.spawn(button("START HOSTING", MenuCommand::StartHost));
        parent.spawn(button("BACK", MenuCommand::Back));
        parent.spawn(status_line());
        parent.spawn(corner_note(VERSION));
    });
}

fn join_screen(mut commands: Commands, assets: Res<AssetServer>, memory: Res<MenuMemory>, loadout: Res<LocalLoadout>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Join);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("JOIN A CREW", 24.0, GOLD));
        // Straight to the code when the address is already filled in from last time.
        let code_first = !memory.address.is_empty() && memory.code.is_empty();
        parent
            .spawn(Node { column_gap: Val::Px(16.0), ..default() })
            .with_children(|row| {
                row.spawn(field_column()).with_children(|address| {
                    address.spawn(label("Host address  (ip, ip:port or hostname)", 14.0, DIM));
                    address.spawn(text_field(
                        FieldKind::Address,
                        &memory.address,
                        "192.168.1.23",
                        40,
                        320.0,
                        !code_first,
                        MenuCommand::Connect,
                    ));
                });
                row.spawn(field_column()).with_children(|code| {
                    code.spawn(label("Room code  (from the host)", 14.0, DIM));
                    code.spawn(text_field(
                        FieldKind::Code,
                        &memory.code,
                        "XXXX-XXXX",
                        CODE_LEN,
                        200.0,
                        code_first,
                        MenuCommand::Connect,
                    ));
                });
                name_field(row, &loadout, MenuCommand::Connect);
            });
        parent.spawn(label("TAB switches between the boxes, and Ctrl/Cmd+V pastes", 13.0, DIM));
        parent.spawn(button("CONNECT", MenuCommand::Connect));
        parent.spawn(button("BACK", MenuCommand::Back));
        parent.spawn(status_line());
        parent.spawn(corner_note(VERSION));
    });
}

fn loadout_screen(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            DespawnOnExit(MenuScreen::Loadout),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(40.0)),
                row_gap: Val::Px(14.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn(label("LOADOUT", 28.0, GOLD));
            parent.spawn(label("Pick your pirate", 15.0, DIM));
            loadout_cards(parent, &assets);
            parent.spawn(button("DONE", MenuCommand::Back));
            parent.spawn(status_line());
        });
}

/// The class and gun cards: on the loadout screen, and in the lobby.
fn loadout_cards(parent: &mut ChildSpawnerCommands, assets: &AssetServer) {
    let card = |width: f32, height: f32, direction: FlexDirection| Node {
        width: Val::Px(width),
        height: Val::Px(height),
        flex_direction: direction,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        column_gap: Val::Px(12.0),
        row_gap: Val::Px(2.0),
        border: UiRect::all(Val::Px(2.0)),
        ..default()
    };
    let row = |gap: f32| Node {
        column_gap: Val::Px(gap),
        row_gap: Val::Px(gap),
        flex_wrap: FlexWrap::Wrap,
        justify_content: JustifyContent::Center,
        max_width: Val::Px(880.0),
        ..default()
    };

    // Wider than the gun rows, so every class fits on one line.
    parent.spawn(Node { max_width: Val::Px(1100.0), ..row(16.0) }).with_children(|classes| {
        for class in PirateClass::ALL {
            classes
                .spawn((
                    Button,
                    Choice::Class(class),
                    card(196.0, 150.0, FlexDirection::Column),
                    BackgroundColor(BUTTON),
                    BorderColor::all(GOLD_FAINT),
                ))
                .with_children(|card| {
                    card.spawn((
                        ImageNode::new(assets.load(class.sprite())),
                        Node {
                            width: Val::Px(64.0),
                            height: Val::Px(64.0),
                            ..default()
                        },
                    ));
                    card.spawn(label(class.name(), 20.0, class.color()));
                    card.spawn(label(class.perk(), 14.0, TEXT));
                    card.spawn((ClassNote(class), label("", 12.0, DIM)));
                });
        }
    });

    // The picks of passive and active ability; only the chosen class's show.
    parent
        .spawn(Node {
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            let ability = |choice: Choice, name: &'static str, details: &'static str, color: Color| {
                (
                    Button,
                    choice,
                    Node {
                        align_items: AlignItems::FlexStart,
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        ..card(236.0, 50.0, FlexDirection::Column)
                    },
                    BackgroundColor(BUTTON),
                    BorderColor::all(GOLD_FAINT),
                    children![label(name, 15.0, color), label(details, 12.0, TEXT)],
                )
            };
            row.spawn(label("PASSIVE", 13.0, DIM));
            for passive in Passive::ALL {
                row.spawn(ability(Choice::Passive(passive), passive.name(), passive.details(), passive.class().color()));
            }
            row.spawn(Node { width: Val::Px(14.0), ..default() });
            row.spawn(label("ACTIVE", 13.0, DIM));
            for active in Active::ALL {
                row.spawn(ability(Choice::Active(active), active.name(), active.details(), active.class().color()));
            }
        });

    parent.spawn((SlotCount, label("", 15.0, DIM)));
    // Four guns to a line.
    parent.spawn(Node { max_width: Val::Px(1160.0), ..row(10.0) }).with_children(|guns| {
        for gun in GunKind::ALL {
            let stats = gun.stats();
            guns.spawn((
                Button,
                Choice::Gun(gun),
                card(280.0, 60.0, FlexDirection::Row),
                BackgroundColor(BUTTON),
                BorderColor::all(GOLD_FAINT),
            ))
            .with_children(|card| {
                card.spawn((
                    ImageNode::new(assets.load(stats.sprite)),
                    Node {
                        width: Val::Px(64.0),
                        height: Val::Px(32.0),
                        ..default()
                    },
                ));
                card.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    width: Val::Px(165.0),
                    ..default()
                })
                .with_children(|info| {
                    info.spawn(label(stats.name, 16.0, TEXT));
                    info.spawn(label(stats.role, 12.0, DIM));
                });
                card.spawn((
                    SlotBadge(gun),
                    label("", 20.0, GOLD),
                    Node {
                        width: Val::Px(14.0),
                        ..default()
                    },
                ));
            });
        }
    });

}

/// One card on the perks or enemies screen.
struct Entry {
    sprite: &'static str,
    /// How big its picture draws: a whole multiple of the sprite's pixels.
    art: f32,
    name: &'static str,
    /// A word on what kind of thing it is, beside the name.
    tag: &'static str,
    /// A figure worth knowing: how long a perk lasts, a guard's health.
    note: String,
    details: &'static str,
    color: Color,
}

/// A codex card's width: three a row, or two a row spanning the same.
const CARD_WIDTH: f32 = 372.0;
const WIDE_CARD_WIDTH: f32 = (3.0 * CARD_WIDTH + 16.0) / 2.0;

/// A run of cards on a codex screen, under an optional heading, each
/// `card_width` wide.
struct Section {
    heading: Option<&'static str>,
    entries: Vec<Entry>,
    card_width: f32,
}

/// A vault guardian's colour on its card.
fn boss_color(kind: KeeperKind) -> Color {
    match kind {
        KeeperKind::Hands => KEEPER_COLOR,
        // The orange of the marks it lays on its prey.
        KeeperKind::Stalker => Color::srgb(1.0, 0.4, 0.15),
        KeeperKind::Brute => Color::srgb(0.95, 0.6, 0.2),
        KeeperKind::Prism => PRISM_COLOR,
    }
}

fn perks_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let entries = PerkKind::ALL.map(|perk| Entry {
        sprite: perk.sprite(),
        art: 64.0,
        name: perk.name(),
        tag: perk.category(),
        note: perk.duration().map_or("INSTANT".into(), |secs| format!("{secs:.0}s")),
        details: perk.details(),
        color: perk.color(),
    });
    codex_screen(
        &mut commands,
        &assets,
        MenuScreen::Perks,
        "PERKS",
        "Found lying around each deck. Walk over one to use it.",
        &[Section { heading: None, entries: entries.into(), card_width: CARD_WIDTH }],
        128.0,
    );
}

fn enemies_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let guards = EnemyKind::ALL.map(|kind| Entry {
        sprite: kind.sprite(),
        art: 64.0,
        name: kind.name(),
        tag: kind.role(),
        note: format!("{:.0} HP", kind.health()),
        details: kind.details(),
        color: kind.color(),
    });
    // Their health depends on the vault and the crew, so no figure.
    let bosses = KeeperKind::ALL.map(|kind| Entry {
        sprite: kind.sprite(),
        // Twice its sprite: the prism's is a little bigger than the rest.
        art: if kind == KeeperKind::Prism { 88.0 } else { 80.0 },
        name: kind.name(),
        tag: kind.role(),
        note: String::new(),
        details: kind.details(),
        color: boss_color(kind),
    });
    codex_screen(
        &mut commands,
        &assets,
        MenuScreen::Enemies,
        "ENEMIES",
        "The station's security, and what it throws at you.",
        &[
            Section { heading: None, entries: guards.into(), card_width: CARD_WIDTH },
            // Two a row: their write-ups are long, and wider cards keep them
            // short enough for every boss to fit a 720 px tall window.
            Section { heading: Some("BOSSES"), entries: bosses.into(), card_width: WIDE_CARD_WIDTH },
        ],
        100.0,
    );
}

/// A full-screen grid of cards, each a picture beside its name and what it
/// does, in sections; each card is at least `card_height` tall, more if its
/// text needs it.
fn codex_screen(
    commands: &mut Commands,
    assets: &AssetServer,
    screen: MenuScreen,
    title: &str,
    subtitle: &str,
    sections: &[Section],
    card_height: f32,
) {
    // Every picture gets a column as wide as the biggest, so the text lines up.
    let art = sections.iter().flat_map(|s| &s.entries).map(|e| e.art).fold(0.0, f32::max);
    commands
        .spawn((
            DespawnOnExit(screen),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(24.0)),
                row_gap: Val::Px(10.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn(label(title, 28.0, GOLD));
            parent.spawn(label(subtitle, 15.0, DIM));
            for section in sections {
                if let Some(heading) = section.heading {
                    parent.spawn(label(heading, 15.0, GOLD));
                }
                parent
                    .spawn(Node {
                        column_gap: Val::Px(16.0),
                        row_gap: Val::Px(16.0),
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        // Three cards a row, so it all fits a 720 px tall window.
                        max_width: Val::Px(1180.0),
                        ..default()
                    })
                    .with_children(|grid| {
                        for entry in &section.entries {
                            grid.spawn((
                                Node {
                                    width: Val::Px(section.card_width),
                                    min_height: Val::Px(card_height),
                                    align_items: AlignItems::Center,
                                    column_gap: Val::Px(16.0),
                                    padding: UiRect::axes(Val::Px(14.0), Val::Px(10.0)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BackgroundColor(BUTTON),
                                BorderColor::all(entry.color.with_alpha(0.5)),
                            ))
                            .with_children(|card| {
                                card.spawn(Node {
                                    width: Val::Px(art),
                                    justify_content: JustifyContent::Center,
                                    flex_shrink: 0.0,
                                    ..default()
                                })
                                .with_child((
                                    ImageNode::new(assets.load(entry.sprite)),
                                    Node {
                                        width: Val::Px(entry.art),
                                        height: Val::Px(entry.art),
                                        flex_shrink: 0.0,
                                        ..default()
                                    },
                                ));
                                card.spawn(Node {
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(5.0),
                                    flex_grow: 1.0,
                                    ..default()
                                })
                                .with_children(|info| {
                                    // Names stay whole; the tag drops below one that's too long.
                                    info.spawn(Node {
                                        column_gap: Val::Px(10.0),
                                        row_gap: Val::Px(2.0),
                                        flex_wrap: FlexWrap::Wrap,
                                        align_items: AlignItems::Baseline,
                                        ..default()
                                    })
                                    .with_children(|heading| {
                                        heading.spawn((label(entry.name, 20.0, entry.color), TextLayout::no_wrap()));
                                        heading.spawn((label(entry.tag, 12.0, DIM), TextLayout::no_wrap()));
                                        heading.spawn((label(entry.note.clone(), 14.0, TEXT), TextLayout::no_wrap()));
                                    });
                                    info.spawn(label(entry.details, 14.0, TEXT));
                                });
                            });
                        }
                    });
            }
            parent.spawn(button("BACK", MenuCommand::Back));
        });
}

// ---------------------------------------------------------------------------
// Interaction
// ---------------------------------------------------------------------------

fn button_styles(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<Button>, Without<Choice>),
    >,
) {
    for (interaction, mut background, mut border) in &mut buttons {
        let (bg, edge) = match interaction {
            Interaction::Pressed => (BUTTON_PRESSED, GOLD),
            Interaction::Hovered => (BUTTON_HOVER, GOLD),
            Interaction::None => (BUTTON, GOLD_FAINT),
        };
        background.0 = bg;
        *border = BorderColor::all(edge);
    }
}

fn button_actions(mut commands: Commands, buttons: Query<(&Interaction, &MenuCommand), Changed<Interaction>>) {
    for (interaction, command) in &buttons {
        if *interaction == Interaction::Pressed {
            commands.trigger(*command);
        }
    }
}

/// Makes what's in the name box this player's display name, and remembers it
/// for next launch. A name that's already theirs is left alone.
fn take_name(typed: &str, loadout: &mut ResMut<LocalLoadout>) {
    let name = clean_name(typed);
    if loadout.0.name != name {
        loadout.0.name = name;
        loadout.0.save();
    }
}

fn run_menu_command(
    command: On<MenuCommand>,
    mut commands: Commands,
    mode: Res<NetMode>,
    mut memory: ResMut<MenuMemory>,
    mut notice: ResMut<Notice>,
    mut fields: Query<&mut TextField>,
    host_code: Query<&HostCode>,
    // Absent outside the menu, e.g. in the lobby.
    current: Option<Res<State<MenuScreen>>>,
    mut screen: ResMut<NextState<MenuScreen>>,
    mut settings: ResMut<Settings>,
    mut exit: MessageWriter<AppExit>,
    roster: Option<Single<&Roster>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut loadout: ResMut<LocalLoadout>,
    (clipboard, mut copied, status): (Option<ResMut<Clipboard>>, ResMut<CopyFeedback>, Option<Single<&MissionStatus>>),
) {
    let joining = matches!(*mode, NetMode::Join { .. });
    let field = |kind: FieldKind| fields.iter().find(|f| f.kind == kind).map(|f| f.value.clone()).unwrap_or_default();

    match *command {
        MenuCommand::Solo => commands.trigger(StartSession(NetMode::Solo)),
        MenuCommand::OpenHost => {
            notice.0 = None;
            screen.set(MenuScreen::Host);
        }
        MenuCommand::OpenJoin => {
            notice.0 = None;
            screen.set(MenuScreen::Join);
        }
        MenuCommand::OpenLoadout => {
            notice.0 = None;
            screen.set(MenuScreen::Loadout);
        }
        MenuCommand::OpenMultiplayer => screen.set(MenuScreen::Multiplayer),
        MenuCommand::OpenExtras => screen.set(MenuScreen::Extras),
        MenuCommand::OpenPerks => screen.set(MenuScreen::Perks),
        MenuCommand::OpenEnemies => screen.set(MenuScreen::Enemies),
        MenuCommand::OpenSettings => screen.set(MenuScreen::Settings),
        MenuCommand::Toggle(setting) => {
            match setting {
                Setting::Reinforcements => settings.reinforcements = !settings.reinforcements,
                Setting::Vsync => settings.vsync = !settings.vsync,
                Setting::RefreshRate => settings.next_refresh_rate(),
            }
            settings.save();
        }
        MenuCommand::Quit => {
            exit.write(AppExit::Success);
        }
        MenuCommand::StartHost => {
            let port = field(FieldKind::Port);
            memory.port = port.clone();
            take_name(&field(FieldKind::Name), &mut loadout);
            match parse_port(&port) {
                Ok(port) => {
                    memory.save();
                    let code = host_code.iter().next().map_or_else(room_code, |c| c.0.clone());
                    commands.trigger(StartSession(NetMode::Host { port, code }));
                }
                Err(message) => notice.0 = Some(message),
            }
        }
        MenuCommand::Connect if joining => {}
        MenuCommand::Connect => {
            let (address, code) = (field(FieldKind::Address), field(FieldKind::Code));
            memory.address = address.clone();
            memory.code = code.clone();
            // Before the session starts: the host hears it in the first message.
            take_name(&field(FieldKind::Name), &mut loadout);
            // Checked here too, so a bad code sends the typing back to its box.
            match (resolve(&address), parse_code(&code)) {
                (Ok(addr), Ok(code)) => {
                    memory.save();
                    commands.trigger(StartSession(NetMode::Join { addr, code }));
                }
                (Err(message), _) => notice.0 = Some(message),
                (Ok(_), Err(message)) => {
                    notice.0 = Some(message);
                    // Straight back to the box that needs fixing.
                    for mut field in &mut fields {
                        field.focused = field.kind == FieldKind::Code;
                    }
                }
            }
        }
        // Only from a real lobby: never while the crew is still out on a run.
        MenuCommand::StartRun if status.is_some_and(|s| s.phase() != GameState::Lobby) => {}
        MenuCommand::StartRun => match roster.as_deref().and_then(|r| waiting_on(r)) {
            Some(name) => notice.0 = Some(format!("Waiting for {name} to pick a class")),
            None => {
                notice.0 = None;
                next_state.set(GameState::Playing);
            }
        },
        MenuCommand::Leave => commands.trigger(EndSession(None)),
        MenuCommand::JoinRun => commands.trigger(JoinRun),
        MenuCommand::CopyCode => {
            // The host screen's code before hosting starts, else the session's.
            let code = match (host_code.iter().next(), &*mode) {
                (Some(code), _) => code.0.clone(),
                (None, NetMode::Host { code, .. } | NetMode::Join { code, .. }) => code.clone(),
                (None, NetMode::Solo) => return,
            };
            let done = clipboard.ok_or_else(|| "no clipboard".to_string()).and_then(|mut clipboard| {
                clipboard.set_text(show_code(&code)).map_err(|e| e.to_string())
            });
            if let Err(error) = &done {
                warn!("Couldn't copy the room code: {error}");
            }
            *copied = CopyFeedback { timer: COPIED_TIME, failed: done.is_err() };
        }
        MenuCommand::Back => {
            if joining {
                // Cancels the pending connection; stays on this screen.
                commands.trigger(EndSession(None));
            } else {
                notice.0 = None;
                screen.set(current.map_or(MenuScreen::Main, |c| c.get().parent()));
            }
        }
    }
}

/// What to call the crewmate who took `class`, if anyone other than `me` did.
fn taken_by(roster: &Roster, class: PirateClass, me: u64) -> Option<String> {
    let owner = roster.holder(class).filter(|owner| *owner != me)?;
    roster.label(roster.slot(owner)?)
}

fn choice_actions(
    choices: Query<(&Interaction, &Choice), Changed<Interaction>>,
    state: Res<State<GameState>>,
    local: Res<LocalId>,
    roster: Option<Single<&Roster>>,
    mut loadout: ResMut<LocalLoadout>,
    mut notice: ResMut<Notice>,
) {
    // In the lobby, one of each class per crew.
    let taken = |class: PirateClass| {
        let roster = roster.as_deref().filter(|_| *state.get() == GameState::Lobby)?;
        taken_by(roster, class, local.0)
    };
    for (interaction, choice) in &choices {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let loadout = &mut loadout.0;
        notice.0 = match *choice {
            Choice::Class(class) if let Some(name) = taken(class) => {
                notice.0 = Some(format!("{name} already plays the {}", class.name()));
                continue;
            }
            Choice::Class(class) => {
                loadout.set_class(class);
                None
            }
            Choice::Passive(passive) => {
                loadout.passive = passive;
                None
            }
            Choice::Active(active) => {
                loadout.set_active(active);
                None
            }
            Choice::Gun(gun) if loadout.guns == [gun] => Some("Take at least one gun".into()),
            Choice::Gun(gun) if !loadout.toggle(gun) => Some(format!(
                "All {} slots are full. Click an equipped gun to drop it first.",
                loadout.slots()
            )),
            Choice::Gun(_) => None,
        };
        loadout.save();
    }
}

/// Highlights the chosen class and equipped guns, and numbers the guns by slot.
/// In the lobby, marks each class someone else took with who took it.
fn style_choices(
    loadout: Res<LocalLoadout>,
    state: Res<State<GameState>>,
    local: Res<LocalId>,
    roster: Option<Single<&Roster>>,
    mut cards: Query<(&Interaction, &Choice, &mut BackgroundColor, &mut BorderColor)>,
    mut badges: Query<(&SlotBadge, &mut Text), Without<ClassNote>>,
    mut notes: Query<(&ClassNote, &mut Text, &mut TextColor)>,
) {
    let roster = roster.as_deref().filter(|_| *state.get() == GameState::Lobby);
    let taken = |class: PirateClass| taken_by(roster?, class, local.0);
    for (note, mut text, mut color) in &mut notes {
        let (content, tint) = match (roster, taken(note.0)) {
            (None, _) => (String::new(), DIM),
            (_, Some(name)) => (format!("TAKEN BY {}", name.to_uppercase()), ERROR),
            (Some(roster), None) if roster.class_of(local.0) == Some(note.0) => ("YOURS".into(), GOLD),
            (Some(_), None) => (String::new(), DIM),
        };
        if text.0 != content {
            text.0 = content;
        }
        color.set_if_neq(TextColor(tint));
    }

    let loadout = &loadout.0;
    for (interaction, choice, mut background, mut border) in &mut cards {
        let chosen = match *choice {
            Choice::Class(class) => loadout.class == class && taken(class).is_none(),
            Choice::Passive(passive) => loadout.passive == passive,
            Choice::Active(active) => loadout.active == Some(active),
            Choice::Gun(gun) => loadout.guns.contains(&gun),
        };
        let taken = matches!(*choice, Choice::Class(class) if taken(class).is_some());
        let (bg, edge) = match (interaction, chosen) {
            _ if taken => (Color::srgb(0.04, 0.04, 0.07), Color::NONE),
            (Interaction::Pressed, _) => (BUTTON_PRESSED, GOLD),
            (_, true) => (Color::srgb(0.2, 0.16, 0.07), GOLD),
            (Interaction::Hovered, false) => (BUTTON_HOVER, GOLD_FAINT),
            (Interaction::None, false) => (BUTTON, Color::NONE),
        };
        background.set_if_neq(BackgroundColor(bg));
        border.set_if_neq(BorderColor::all(edge));
    }
    for (badge, mut text) in &mut badges {
        let slot = loadout.guns.iter().position(|&g| g == badge.0);
        let content = slot.map_or(String::new(), |i| (i + 1).to_string());
        if text.0 != content {
            text.0 = content;
        }
    }
}

fn update_setting_values(settings: Res<Settings>, mut values: Query<(&SettingValue, &mut Text)>) {
    for (value, mut text) in &mut values {
        let content = value.0.value(&settings);
        if text.0 != content {
            text.0 = content;
        }
    }
}

/// Shows only the abilities the chosen class can pick from.
fn show_ability_choices(
    loadout: Res<LocalLoadout>,
    mut choices: Query<(&Choice, &mut Node)>,
) {
    let class = loadout.0.class;
    for (choice, mut node) in &mut choices {
        let shown = match *choice {
            Choice::Passive(passive) => passive.class() == class,
            Choice::Active(active) => active.class() == class,
            _ => continue,
        };
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

fn update_slot_count(loadout: Res<LocalLoadout>, mut text: Query<&mut Text, With<SlotCount>>) {
    let Ok(mut text) = text.single_mut() else { return };
    let loadout = &loadout.0;
    let content = format!(
        "Guns  {}/{}   -   click to equip in the next slot, again to drop",
        loadout.guns.len(),
        loadout.slots()
    );
    if text.0 != content {
        text.0 = content;
    }
}

fn edit_fields(
    mut commands: Commands,
    mut keys: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    mut clipboard: Option<ResMut<Clipboard>>,
    mut fields: Query<(&mut TextField, Ref<Interaction>)>,
) {
    // A click gives that field the typing, with the caret at the end.
    let clicked = fields.iter().find(|(_, i)| i.is_changed() && **i == Interaction::Pressed).map(|(f, _)| f.kind);
    if let Some(clicked) = clicked {
        for (mut field, _) in &mut fields {
            field.focused = field.kind == clicked;
            if field.focused {
                field.end();
            }
        }
    }
    // Cmd on a Mac, Ctrl elsewhere; either works everywhere.
    let command = held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    for key in keys.read() {
        if !key.state.is_pressed() {
            continue;
        }
        if key.logical_key == Key::Tab {
            // To the next field, or the one before with Shift.
            let mut kinds: Vec<FieldKind> = fields.iter().map(|(f, _)| f.kind).collect();
            kinds.sort();
            let Some(at) = fields.iter().find(|(f, _)| f.focused).and_then(|(f, _)| kinds.iter().position(|k| *k == f.kind))
            else {
                continue;
            };
            let back = held.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            let next = kinds[if back { at + kinds.len() - 1 } else { at + 1 } % kinds.len()];
            for (mut field, _) in &mut fields {
                field.focused = field.kind == next;
            }
            continue;
        }
        let Some((mut field, _)) = fields.iter_mut().find(|(f, _)| f.focused) else { continue };
        match &key.logical_key {
            // Shortcuts, never letters: Cmd+V would otherwise type a "v".
            Key::Character(text) if command => {
                if text.eq_ignore_ascii_case("v")
                    && let Some(Ok(pasted)) = clipboard.as_mut().and_then(|c| c.fetch_text().poll_result())
                {
                    field.insert(&pasted);
                }
            }
            Key::Character(text) => field.insert(text),
            // A key of its own, not a character.
            Key::Space if !command => field.insert(" "),
            Key::Backspace => field.backspace(),
            Key::Delete => field.delete(),
            Key::ArrowLeft if command => field.home(),
            Key::ArrowRight if command => field.end(),
            Key::ArrowLeft => field.step(-1),
            Key::ArrowRight => field.step(1),
            Key::Home => field.home(),
            Key::End => field.end(),
            Key::Enter => commands.trigger(field.submit),
            _ => {}
        }
    }
}

fn draw_fields(
    time: Res<Time>,
    mut fields: Query<(&TextField, &Children, &mut BorderColor)>,
    mut texts: Query<(&FieldText, &mut Text)>,
    mut carets: Query<&mut Visibility, (With<Caret>, Without<Placeholder>)>,
    mut placeholders: Query<&mut Visibility, With<Placeholder>>,
) {
    let caret_on = (time.elapsed_secs() * 2.0).fract() < 0.5;
    for (field, children, mut border) in &mut fields {
        // The field taking the typing is the one outlined in full gold.
        border.set_if_neq(BorderColor::all(if field.focused { GOLD } else { GOLD_FAINT }));
        let (before, after) = field.split();
        for child in children.iter() {
            if let Ok((part, mut text)) = texts.get_mut(child) {
                let content = if *part == FieldText::BeforeCaret { before } else { after };
                if text.0 != content {
                    text.0 = content.into();
                }
            }
            if let Ok(mut visibility) = carets.get_mut(child) {
                let shown = caret_on && field.focused;
                visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
            if let Ok(mut visibility) = placeholders.get_mut(child) {
                let shown = field.value.is_empty();
                visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
}

/// Looks up this machine's addresses every `ADDRESS_REFRESH` seconds while
/// they're on screen: on the host screen and while hosting.
fn refresh_addresses(
    time: Res<Time>,
    mode: Res<NetMode>,
    screen: Option<Res<State<MenuScreen>>>,
    mut addresses: ResMut<HostAddresses>,
) {
    let shown = matches!(*mode, NetMode::Host { .. }) || screen.is_some_and(|s| *s.get() == MenuScreen::Host);
    if !shown {
        // Fresh as soon as they show again.
        addresses.next = 0.0;
        return;
    }
    addresses.next -= time.delta_secs();
    if addresses.next <= 0.0 {
        let (lan, tailscale) = (local_ip(), tailscale_ip());
        // Only touched on a change, so the screens showing them could tell.
        if addresses.lan != lan || addresses.tailscale != tailscale {
            addresses.lan = lan;
            addresses.tailscale = tailscale;
        }
        addresses.bypass_change_detection().next = ADDRESS_REFRESH;
    }
}

fn update_host_hint(
    fields: Query<&TextField>,
    addresses: Res<HostAddresses>,
    mut hint: Query<&mut Text, (With<HostHint>, Without<TailscaleNote>)>,
    mut note: Query<&mut Text, With<TailscaleNote>>,
) {
    let Ok(mut hint) = hint.single_mut() else { return };
    let port = fields.iter().find(|f| f.kind == FieldKind::Port).map_or("", |f| f.value.as_str());
    let content = match port.parse::<u16>() {
        Ok(port) if port > 0 => format!("Your crew joins with:  {}", addresses.line(port)),
        _ => "Pick a port between 1 and 65535".into(),
    };
    if hint.0 != content {
        hint.0 = content;
    }
    let Ok(mut note) = note.single_mut() else { return };
    let content = if addresses.tailscale.is_some() {
        "Friends on your tailnet use the Tailscale address, from anywhere."
    } else {
        "Over the internet? Turn on Tailscale and its address shows here too."
    };
    if note.0 != content {
        note.0 = content.into();
    }
}

/// COPY reads COPIED for a moment after a click, or says the clipboard
/// wouldn't take it.
fn update_copy_buttons(time: Res<Time>, mut copied: ResMut<CopyFeedback>, mut labels: Query<&mut Text, With<CopyLabel>>) {
    if copied.timer > 0.0 {
        copied.timer = (copied.timer - time.delta_secs()).max(0.0);
    }
    let content = match (copied.timer > 0.0, copied.failed) {
        (false, _) => "COPY",
        (true, false) => "COPIED",
        (true, true) => "NO CLIPBOARD",
    };
    for mut text in &mut labels {
        if text.0 != content {
            text.0 = content.into();
        }
    }
}

fn update_status(
    mode: Res<NetMode>,
    state: Res<State<GameState>>,
    notice: Res<Notice>,
    connection: Res<Connection>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusText>>,
) {
    let Ok((mut text, mut color)) = status.single_mut() else { return };
    let connecting = matches!(*mode, NetMode::Join { .. }) && *state.get() == GameState::Menu;
    let (content, tint) = match (&*mode, &notice.0) {
        (NetMode::Join { addr, .. }, _) if connecting => (
            format!("Connecting to {addr}...  {:.0}s", connection.waited),
            GOLD,
        ),
        (_, Some(message)) => (message.clone(), ERROR),
        _ => (String::new(), ERROR),
    };
    if text.0 != content {
        text.0 = content;
    }
    color.set_if_neq(TextColor(tint));
}

fn back_on_escape(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, screen: Res<State<MenuScreen>>) {
    if keys.just_pressed(KeyCode::Escape) && *screen.get() != MenuScreen::Main {
        commands.trigger(MenuCommand::Back);
    }
}

// ---------------------------------------------------------------------------
// Backdrop: the pirate, orbiting loot, and the station's guards
// ---------------------------------------------------------------------------

/// The window size the menus are laid out for.
const MENU_SIZE: Vec2 = Vec2::new(1280.0, 720.0);

/// How much larger the menus draw so they fill a big screen. It steps so each
/// sprite pixel stays a whole number of screen pixels, keeping the art crisp.
fn menu_zoom(window: &Window) -> f32 {
    let fit = (window.width() / MENU_SIZE.x).min(window.height() / MENU_SIZE.y);
    ((fit * PIXEL_SCALE).floor() / PIXEL_SCALE).max(1.0)
}

/// Grows the menus and their backdrop with the window.
fn fit_menus(
    window: Single<&Window, With<PrimaryWindow>>,
    ui: ResMut<UiScale>,
    projection: Single<&mut Projection, With<Camera2d>>,
) {
    zoom(menu_zoom(&window), ui, projection);
}

/// Raids are always drawn at 1×: a bigger view would let a pirate on a big
/// screen see further than their crew.
fn unzoom(ui: ResMut<UiScale>, projection: Single<&mut Projection, With<Camera2d>>) {
    zoom(1.0, ui, projection);
}

fn zoom(zoom: f32, mut ui: ResMut<UiScale>, mut projection: Single<&mut Projection, With<Camera2d>>) {
    // Only write on a change, so UI layout isn't redone every frame.
    if ui.0 != zoom {
        ui.0 = zoom;
    }
    let scale = 1.0 / (PIXEL_SCALE * zoom);
    if matches!(&**projection, Projection::Orthographic(o) if o.scale != scale)
        && let Projection::Orthographic(ortho) = &mut **projection
    {
        ortho.scale = scale;
    }
}

/// Keeps the camera on the backdrop, away from any deck left over from a run.
/// It checks every frame since the camera may not exist yet on the first menu frame.
fn hold_camera(mut camera: Single<&mut Transform, With<Camera2d>>) {
    let at = MENU_ORIGIN.extend(camera.translation.z);
    if camera.translation != at {
        camera.translation = at;
    }
}

fn show_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = true;
}

fn hide_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = false;
}

/// The menus and the lobby share one backdrop, far from where decks are built,
/// so a deck left over from the last run never shows behind them.
const MENU_ORIGIN: Vec2 = Vec2::new(0.0, 100_000.0);

/// Spawns the backdrop on the first menu or lobby screen; it stays until a run starts.
fn spawn_backdrop(
    mut commands: Commands,
    assets: Res<AssetServer>,
    loadout: Res<LocalLoadout>,
    existing: Query<(), With<Backdrop>>,
) {
    if !existing.is_empty() {
        return;
    }
    let sky = room::starfield(MENU_ORIGIN).into_iter().map(|star| (DespawnOnEnter(GameState::Playing), star));
    commands.spawn_batch(sky.collect::<Vec<_>>());
    let mut sprite = |path: &str, pos: Vec3, scale: f32, flip: bool, orbit: Option<(Vec2, f32)>, phase: f32| {
        let pos = pos + MENU_ORIGIN.extend(0.0);
        let orbit = orbit.map(|(center, radius)| (center + MENU_ORIGIN, radius));
        commands.spawn((
            DespawnOnEnter(GameState::Playing),
            Backdrop { base: pos, phase, orbit },
            Sprite {
                image: assets.load(path.to_string()),
                flip_x: flip,
                ..default()
            },
            Transform::from_translation(pos).with_scale(Vec3::splat(scale)),
        ))
        .id()
    };

    let pirate = Vec2::new(-140.0, -30.0);
    sprite("sprites/shadow.png", pirate.extend(9.0), 3.0, false, None, 0.0);
    let body = sprite(loadout.0.class.sprite(), pirate.extend(10.0), 3.0, false, None, 0.0);
    let gun = sprite(
        loadout.0.guns[0].stats().sprite,
        (pirate + Vec2::new(12.0, -9.0)).extend(11.0),
        2.2,
        false,
        None,
        0.0,
    );

    let loot = [
        "sprites/loot/alien_relic.png",
        "sprites/loot/data_crystal.png",
        "sprites/loot/gold_ingot.png",
        "sprites/loot/credit_chip.png",
        "sprites/loot/plasma_cell.png",
        "sprites/loot/weapon_crate.png",
    ];
    for (i, path) in loot.iter().enumerate() {
        let phase = i as f32 / loot.len() as f32 * TAU;
        sprite(path, pirate.extend(12.0), 1.6, false, Some((pirate, 42.0)), phase);
    }

    sprite("sprites/enemies/warden.png", Vec3::new(150.0, -25.0, 10.0), 3.0, true, None, 0.7);
    sprite("sprites/enemies/stalker.png", Vec3::new(118.0, -55.0, 11.0), 2.5, true, None, 1.9);
    sprite("sprites/enemies/sentry_drone.png", Vec3::new(168.0, 40.0, 11.0), 2.5, true, None, 2.8);

    commands.entity(body).insert(BackdropPirate::Body);
    commands.entity(gun).insert(BackdropPirate::Gun);
}

/// The loadout, perks, enemies and lobby screens fill the window, so there the
/// backdrop fades back to keep the cards in front readable.
fn fade_backdrop(
    state: Res<State<GameState>>,
    screen: Option<Res<State<MenuScreen>>>,
    mut sprites: Query<&mut Sprite, With<Backdrop>>,
) {
    let full = *state.get() == GameState::Lobby
        || screen.is_some_and(|s| matches!(s.get(), MenuScreen::Loadout | MenuScreen::Perks | MenuScreen::Enemies));
    let tint = Color::WHITE.with_alpha(if full { 0.3 } else { 1.0 });
    for mut sprite in &mut sprites {
        if sprite.color != tint {
            sprite.color = tint;
        }
    }
}

fn dress_backdrop_pirate(
    assets: Res<AssetServer>,
    loadout: Res<LocalLoadout>,
    mut sprites: Query<(&BackdropPirate, &mut Sprite)>,
) {
    for (part, mut sprite) in &mut sprites {
        let path = match part {
            BackdropPirate::Body => loadout.0.class.sprite(),
            BackdropPirate::Gun => loadout.0.guns[0].stats().sprite,
        };
        sprite.image = assets.load(path);
    }
}

fn animate_backdrop(
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut sprites: Query<(&Backdrop, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    // The menu hangs from the top of the window, so in a taller one the
    // backdrop rises with it rather than staying in the middle.
    let lift = Vec3::Y * ((window.height() / menu_zoom(&window) - MENU_SIZE.y) / 2.0 / PIXEL_SCALE).round();
    for (backdrop, mut transform) in &mut sprites {
        transform.translation = match backdrop.orbit {
            Some((center, radius)) => {
                let angle = t * 0.35 + backdrop.phase;
                // Squashed ellipse reads as a ring around the pirate; items behind him sit lower in z.
                let offset = Vec2::new(angle.cos() * radius, angle.sin() * radius * 0.45);
                let z = if angle.sin() > 0.0 { 9.5 } else { 12.0 };
                (center + offset + Vec2::new(0.0, (t * 2.0 + backdrop.phase).sin() * 2.0)).extend(z) + lift
            }
            None => backdrop.base + lift + Vec3::new(0.0, ((t * 1.6 + backdrop.phase).sin() * 2.0).round(), 0.0),
        };
    }
}

// ---------------------------------------------------------------------------
// In-game: session info and leaving
// ---------------------------------------------------------------------------

fn spawn_session_hud(mut commands: Commands, slots: Query<(Entity, &HudSlot)>) {
    commands.entity(hud::slot(&slots, HudSlot::Session)).with_children(|slot| {
        // Right-aligned line by line: a host on Tailscale gets a second line.
        slot.spawn((SessionText, label("", 14.0, DIM), TextLayout::justify(Justify::Right), Node {
            align_self: AlignSelf::FlexEnd,
            ..default()
        }));
        slot.spawn((LootLeftText, label("", 14.0, DIM), Node {
            align_self: AlignSelf::FlexEnd,
            ..default()
        }));
    });
    commands.entity(hud::slot(&slots, HudSlot::Notice)).with_child((
        LeavePanel,
        hud::panel_accent(
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(6.0)),
                display: Display::None,
                ..default()
            },
            GOLD,
        ),
        children![(LeaveText, label("", 15.0, GOLD))],
    ));
}

fn update_session_hud(
    mode: Res<NetMode>,
    prompt: Res<LeavePrompt>,
    device: Res<LastDevice>,
    players: Query<(), With<Player>>,
    loot: Query<(), With<LootKind>>,
    vault: Option<Single<&Vault>>,
    mut session: Single<&mut Text, (With<SessionText>, Without<LeaveText>, Without<LootLeftText>)>,
    mut loot_left: Single<&mut Text, (With<LootLeftText>, Without<LeaveText>)>,
    mut leave: Single<&mut Text, With<LeaveText>>,
    mut leave_panel: Single<&mut Node, With<LeavePanel>>,
    addresses: Res<HostAddresses>,
) {
    let crew = players.iter().count();
    let content = match &*mode {
        NetMode::Solo => String::new(),
        NetMode::Host { port, code } => {
            // The code and crew go after the first address: both addresses on
            // one line would reach the deck panel in the middle of the screen.
            let rest = format!("  CODE {}    CREW {crew}", show_code(code));
            let at = join_addresses(addresses.lan, addresses.tailscale, *port, &format!("{rest}\nTailscale "));
            if at.contains('\n') { format!("HOSTING  {at}") } else { format!("HOSTING  {at}{rest}") }
        }
        NetMode::Join { addr, code } => format!("CREW {crew}    host {addr}  CODE {}", show_code(code)),
    };
    if session.0 != content {
        session.0 = content;
    }
    let mut remaining = format!("LOOT LEFT {}", loot.iter().count());
    if let Some(vault) = vault {
        remaining += &format!("    {} VAULT {}/{}", vault.tier.name(), vault.bars, vault.total);
    }
    if loot_left.0 != remaining {
        loot_left.0 = remaining;
    }

    let warning = if prompt.timer > 0.0 { leave_warning(&mode, GameState::Playing, *device) } else { String::new() };
    let display = if warning.is_empty() { Display::None } else { Display::Flex };
    if leave.0 != warning {
        leave.0 = warning;
    }
    if leave_panel.display != display {
        leave_panel.display = display;
    }
}

/// What a second ESC (or Select) would do, from `state`.
fn leave_warning(mode: &NetMode, state: GameState, device: LastDevice) -> String {
    let key = device.glyph("ESC");
    match (mode, state) {
        (NetMode::Host { .. }, GameState::Lobby) => format!("Press {key} again to close the room for everyone"),
        (_, GameState::Lobby) => format!("Press {key} again to leave the crew"),
        (NetMode::Host { .. }, _) => format!("Press {key} again to end the run and take the crew back to the lobby"),
        (NetMode::Join { .. }, _) => format!("Press {key} again to drop out to the crew lobby"),
        (NetMode::Solo, _) => format!("Press {key} again to leave to the main menu"),
    }
}

/// ESC (or a gamepad's Select) drops out of a run to the crew lobby, needing a
/// second press mid-run so it can't end a raid by accident, and one from the
/// results screen. In the lobby, a second press leaves the crew: needing it
/// there too keeps a quick extra press after dropping out from leaving it.
fn leave_on_escape(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    state: Res<State<GameState>>,
    mut prompt: ResMut<LeavePrompt>,
) {
    // A press armed on one screen doesn't carry over to the next.
    prompt.timer = if state.is_changed() { 0.0 } else { (prompt.timer - time.delta_secs()).max(0.0) };
    // A gamepad's Select (Back, View, Share) does what ESC does.
    if !keys.just_pressed(KeyCode::Escape) && !pads.iter().any(|p| p.just_pressed(GamepadButton::Select)) {
        return;
    }
    let confirmed = prompt.timer > 0.0;
    prompt.timer = 0.0;
    match state.get() {
        GameState::Over => commands.trigger(DropOut),
        GameState::Playing if confirmed => commands.trigger(DropOut),
        GameState::Lobby if confirmed => commands.trigger(EndSession(None)),
        _ => prompt.timer = LEAVE_WINDOW,
    }
}

// ---------------------------------------------------------------------------
// Lobby: the crew picks classes and guns, then the host sets off
// ---------------------------------------------------------------------------

/// Under the title: where friends can join, or which host this is.
#[derive(Component)]
struct LobbyInfo;

/// Beside that, big enough to read out: the room code joiners need.
#[derive(Component)]
struct LobbyCode;

/// A crewmate's tag in the lobby, by place in the roster.
#[derive(Component)]
struct CrewChip(usize);

/// What the crew is waiting on before the run can start.
#[derive(Component)]
struct LobbyHint;

/// A lobby button that only shows some of the time: START while the crew is
/// all in the lobby, JOIN RUN while a joiner here could drop into a run.
#[derive(Component, PartialEq, Eq)]
enum LobbyButton {
    Start,
    JoinRun,
}

/// What to call the first pirate in the roster still choosing a class, if any.
fn waiting_on(roster: &Roster) -> Option<String> {
    roster.label(roster.0.iter().position(|c| c.class.is_none())?)
}

fn lobby_screen(mut commands: Commands, assets: Res<AssetServer>, mode: Res<NetMode>) {
    let hosting = matches!(*mode, NetMode::Host { .. });
    commands
        .spawn((
            DespawnOnExit(GameState::Lobby),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(24.0)),
                row_gap: Val::Px(12.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn(label("CREW LOBBY", 28.0, GOLD));
            parent
                .spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Baseline, ..default() })
                .with_children(|row| {
                    row.spawn((LobbyInfo, label("", 15.0, DIM)));
                    copyable_code(row, (LobbyCode, label("", 24.0, GOLD)));
                });
            parent
                .spawn(Node {
                    column_gap: Val::Px(10.0),
                    row_gap: Val::Px(6.0),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    ..default()
                })
                .with_children(|chips| {
                    for i in 0..PirateClass::ALL.len() {
                        chips.spawn((
                            CrewChip(i),
                            hud::panel(Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                display: Display::None,
                                ..default()
                            }),
                            children![(label("", 14.0, TEXT), TextLayout::no_wrap())],
                        ));
                    }
                });
            loadout_cards(parent, &assets);
            parent
                .spawn(Node { column_gap: Val::Px(16.0), align_items: AlignItems::Center, ..default() })
                .with_children(|buttons| {
                    // Shown by `update_lobby`, as the host's phase changes under the screen.
                    if hosting {
                        buttons.spawn((button("START", MenuCommand::StartRun), LobbyButton::Start));
                    } else {
                        buttons.spawn((button("JOIN RUN", MenuCommand::JoinRun), LobbyButton::JoinRun));
                    }
                    buttons.spawn(button(if hosting { "CLOSE ROOM" } else { "LEAVE CREW" }, MenuCommand::Leave));
                });
            parent.spawn((LobbyHint, label("", 15.0, DIM)));
            parent.spawn(status_line());
            parent.spawn(corner_note(VERSION));
        });
}

/// Fills in the lobby, which works whatever the host is doing: a joiner who
/// dropped out of a run, or arrived during one, waits here until they join in
/// or the crew comes back.
fn update_lobby(
    mode: Res<NetMode>,
    local: Res<LocalId>,
    device: Res<LastDevice>,
    prompt: Res<LeavePrompt>,
    addresses: Res<HostAddresses>,
    roster: Option<Single<&Roster>>,
    status: Option<Single<&MissionStatus>>,
    players: Query<&Player>,
    mut info: Single<&mut Text, (With<LobbyInfo>, Without<LobbyHint>, Without<LobbyCode>)>,
    mut code: Single<&mut Text, (With<LobbyCode>, Without<LobbyHint>)>,
    mut hint: Single<&mut Text, (With<LobbyHint>, Without<LobbyInfo>)>,
    mut buttons: Query<(&LobbyButton, &mut Node), Without<CrewChip>>,
    mut chips: Query<(&CrewChip, &mut Node, &mut BorderColor, &Children)>,
    mut chip_texts: Query<(&mut Text, &mut TextColor), (Without<LobbyInfo>, Without<LobbyHint>, Without<LobbyCode>)>,
) {
    let phase = status.map_or(GameState::Lobby, |s| s.phase());
    let host_name = roster.as_deref().and_then(|r| r.label(r.slot(HOST_ID)?));
    let (content, room) = match &*mode {
        NetMode::Host { port, code } => {
            (format!("Friends join at  {}  with room code", addresses.line(*port)), show_code(code))
        }
        // So a joiner can pass it on too.
        NetMode::Join { addr, code } => {
            let host = host_name.unwrap_or_else(|| addr.to_string());
            let under_way = match phase {
                GameState::Playing => "  -  run under way",
                GameState::Over => "  -  run over",
                _ => "",
            };
            (format!("Aboard {host}'s crew{under_way}  -  room code"), show_code(code))
        }
        NetMode::Solo => (String::new(), String::new()),
    };
    if info.0 != content {
        info.0 = content;
    }
    if code.0 != room {
        code.0 = room;
    }

    let joiner = matches!(*mode, NetMode::Join { .. });
    for (button, mut node) in &mut buttons {
        let shown = match button {
            LobbyButton::Start => phase == GameState::Lobby,
            LobbyButton::JoinRun => joiner && phase == GameState::Playing,
        };
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }

    let Some(roster) = roster else { return };
    let running = phase != GameState::Lobby;
    for (chip, mut node, mut border, children) in &mut chips {
        let entry = roster.0.get(chip.0);
        let display = if entry.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let (Some(entry), Some(name)) = (entry, roster.label(chip.0)) else { continue };
        let you = if entry.owner == local.0 { "  (you)" } else { "" };
        // A pirate of their own is what puts a crewmate in the run.
        let in_run = players.iter().any(|p| p.owner == entry.owner);
        let (mut content, mut color) = match entry.class {
            Some(class) => (format!("{name}  {}{you}", class.name()), class.color()),
            None => (format!("{name}  choosing...{you}"), DIM),
        };
        if running {
            content += if in_run { "  -  in the run" } else { "  -  in the lobby" };
            if !in_run {
                color = DIM;
            }
        }
        border.set_if_neq(BorderColor { top: color, ..BorderColor::all(hud::EDGE) });
        if let Some(Ok((mut text, mut tint))) = children.first().map(|c| chip_texts.get_mut(*c)) {
            if text.0 != content {
                text.0 = content;
            }
            tint.set_if_neq(TextColor(color));
        }
    }

    let mine = roster.class_of(local.0);
    let content = match (waiting_on(&roster), &*mode) {
        _ if prompt.timer > 0.0 => leave_warning(&mode, GameState::Lobby, *device),
        _ if mine.is_none() => "Your class is taken: pick a free one".to_string(),
        _ if phase == GameState::Playing => "A run is under way: pick a free class and JOIN RUN to drop in".to_string(),
        _ if phase == GameState::Over => "The crew is on the results screen: you'll be in the next run".to_string(),
        (Some(name), _) => format!("Waiting for {name} to pick a class"),
        (None, NetMode::Host { .. }) => "Everyone's ready. Start when you are".to_string(),
        (None, _) => "Waiting for the host to start".to_string(),
    };
    if hint.0 != content {
        hint.0 = content;
    }
}

/// Tells the host about every change a joiner makes in the lobby.
fn send_lobby_loadout(
    mode: Res<NetMode>,
    loadout: Res<LocalLoadout>,
    mut messages: MessageWriter<ChooseLoadout>,
) {
    if matches!(*mode, NetMode::Join { .. }) {
        messages.write(ChooseLoadout(loadout.0.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::crew::Crewmate;

    fn field(kind: FieldKind, max_len: usize) -> TextField {
        TextField::new(kind, "", max_len, MenuCommand::Back, true)
    }

    fn code_field() -> TextField {
        field(FieldKind::Code, CODE_LEN)
    }

    /// The field's text with `|` at the caret.
    fn shown(field: &TextField) -> String {
        let (before, after) = field.split();
        format!("{before}|{after}")
    }

    fn crewmate(owner: u64, class: Option<PirateClass>, name: &str) -> Crewmate {
        Crewmate { owner, class, name: name.into() }
    }

    #[test]
    fn a_name_box_takes_what_a_name_can_hold() {
        for c in ['a', 'Z', '7', ' ', '-', '_', '.'] {
            assert!(FieldKind::Name.allows(c), "{c:?}");
        }
        for c in ['/', '!', ':', '\n', '\u{e9}'] {
            assert!(!FieldKind::Name.allows(c), "{c:?}");
        }
    }

    #[test]
    fn typing_stops_at_the_field_limit() {
        let mut name = field(FieldKind::Name, NAME_LEN);
        name.insert("Dread");
        name.insert(" ");
        name.insert("Pirate Roberts");
        assert_eq!(name.value, "Dread Pirate");
        assert_eq!(name.value.len(), NAME_LEN);
    }

    #[test]
    fn only_a_name_takes_spaces_and_only_a_code_is_capitalised() {
        let mut address = field(FieldKind::Address, 40);
        address.insert("10.0.0.1 ");
        assert_eq!(address.value, "10.0.0.1");
        let mut code = code_field();
        code.insert("ab-c d");
        assert_eq!(code.value, "ABCD-", "the dash comes by itself; typed ones and spaces don't count");
        let mut name = field(FieldKind::Name, NAME_LEN);
        name.insert("ab");
        assert_eq!(name.value, "ab");
    }

    #[test]
    fn the_caret_moves_and_edits_where_it_is() {
        let mut name = field(FieldKind::Name, NAME_LEN);
        name.insert("Mira");
        assert_eq!(shown(&name), "Mira|");
        name.step(-2);
        assert_eq!(shown(&name), "Mi|ra");
        name.insert("XY");
        assert_eq!(shown(&name), "MiXY|ra");
        name.backspace();
        assert_eq!(shown(&name), "MiX|ra");
        name.delete();
        assert_eq!(shown(&name), "MiX|a");
        name.home();
        assert_eq!(shown(&name), "|MiXa");
        name.step(-1);
        name.backspace();
        assert_eq!(shown(&name), "|MiXa", "nothing before the start");
        name.end();
        name.step(1);
        name.delete();
        assert_eq!(shown(&name), "MiXa|", "nothing after the end");
    }

    #[test]
    fn inserting_mid_field_still_stops_at_the_limit() {
        let mut port = field(FieldKind::Port, 5);
        port.insert("5000");
        port.home();
        port.insert("123");
        assert_eq!(shown(&port), "1|5000");
    }

    #[test]
    fn a_caret_off_a_character_boundary_does_not_panic() {
        let mut name = field(FieldKind::Name, NAME_LEN);
        name.value = "\u{e9}a".into();
        name.caret = 1;
        assert_eq!(name.split(), ("\u{e9}a", ""));
        name.backspace();
        assert_eq!(name.value, "\u{e9}");
        name.caret = 99;
        name.step(-1);
        assert_eq!(shown(&name), "|\u{e9}");
    }

    #[test]
    fn pasting_keeps_only_what_the_field_takes() {
        let mut address = field(FieldKind::Address, 40);
        address.insert(" 192.168.1.23:5000\n");
        assert_eq!(address.value, "192.168.1.23:5000");
        let mut port = field(FieldKind::Port, 5);
        port.insert("port 7777!");
        assert_eq!(port.value, "7777");
        let mut name = field(FieldKind::Name, NAME_LEN);
        name.insert("Mira\tthe \u{e9}Bold\n");
        assert_eq!(name.value, "Mirathe Bold");
    }

    #[test]
    fn a_code_gets_its_dash_however_it_comes() {
        let mut code = code_field();
        for (typed, value) in [("k", "K"), ("7", "K7"), ("q", "K7Q"), ("f", "K7QF-"), ("-", "K7QF-"), ("2", "K7QF-2")] {
            code.insert(typed);
            assert_eq!(code.value, value, "after typing {typed:?}");
        }
        for pasted in ["k7qf29xa", "K7QF-29XA", "k7qf 29xa", " K7QF-29XA\n", "k7qf29xa99"] {
            let mut code = code_field();
            code.insert(pasted);
            assert_eq!(shown(&code), "K7QF-29XA|", "pasting {pasted:?}");
            assert!(parse_code(&code.value).is_ok());
        }
    }

    #[test]
    fn backspace_takes_the_dash_with_the_letter_before_it() {
        let mut code = code_field();
        code.insert("K7QF");
        assert_eq!(shown(&code), "K7QF-|");
        code.backspace();
        assert_eq!(shown(&code), "K7Q|");
        code.insert("F2");
        code.backspace();
        code.backspace();
        assert_eq!(shown(&code), "K7Q|", "never stuck on the dash");
    }

    #[test]
    fn the_caret_skips_a_codes_dash() {
        let mut code = code_field();
        code.insert("K7QF29XA");
        code.step(-4);
        assert_eq!(shown(&code), "K7QF-|29XA", "after the dash, by the letter after it");
        code.step(-1);
        assert_eq!(shown(&code), "K7Q|F-29XA");
        code.step(1);
        assert_eq!(shown(&code), "K7QF-|29XA");
        code.backspace();
        assert_eq!(shown(&code), "K7Q|2-9XA", "the letters close up round the dash");
        code.insert("f");
        assert_eq!(shown(&code), "K7QF-|29XA");
        code.home();
        code.delete();
        assert_eq!(shown(&code), "|7QF2-9XA");
        code.insert("zz");
        assert_eq!(shown(&code), "Z|7QF-29XA", "a full code takes no more");
    }

    #[test]
    fn a_remembered_code_comes_back_dashed() {
        let code = TextField::new(FieldKind::Code, "k7qf29xa", CODE_LEN, MenuCommand::Connect, true);
        assert_eq!(shown(&code), "K7QF-29XA|");
    }

    #[test]
    fn the_crew_joins_on_every_address_shown() {
        let lan: IpAddr = "192.168.1.23".parse().unwrap();
        let tailnet: IpAddr = "100.95.119.29".parse().unwrap();
        let or = "  or over Tailscale  ";
        assert_eq!(join_addresses(Some(lan), None, DEFAULT_PORT, or), "192.168.1.23");
        assert_eq!(
            join_addresses(Some(lan), Some(tailnet), DEFAULT_PORT, or),
            "192.168.1.23  or over Tailscale  100.95.119.29"
        );
        assert_eq!(join_addresses(Some(tailnet), Some(tailnet), DEFAULT_PORT, or), "100.95.119.29  (Tailscale)");
        assert_eq!(join_addresses(None, Some(tailnet), 7777, or), "100.95.119.29:7777  (Tailscale)");
        assert_eq!(
            join_addresses(Some(lan), Some(tailnet), 7777, or),
            "192.168.1.23:7777  or over Tailscale  100.95.119.29:7777"
        );
        assert_eq!(join_addresses(None, None, DEFAULT_PORT, or), "<your IP>");
    }

    #[test]
    fn a_second_escape_warns_what_it_will_do() {
        let host = NetMode::Host { port: DEFAULT_PORT, code: "K7QF29XA".into() };
        let join = NetMode::Join { addr: "192.168.1.23:5000".parse().unwrap(), code: "K7QF29XA".into() };
        let warn = |mode: &NetMode, state| leave_warning(mode, state, LastDevice::Keys);
        assert!(warn(&host, GameState::Playing).contains("take the crew back to the lobby"));
        assert!(warn(&join, GameState::Playing).contains("drop out to the crew lobby"));
        assert!(warn(&NetMode::Solo, GameState::Playing).contains("main menu"));
        assert!(warn(&host, GameState::Lobby).contains("close the room"));
        assert!(warn(&join, GameState::Lobby).contains("leave the crew"));
        assert!(leave_warning(&join, GameState::Lobby, LastDevice::Pad).starts_with("Press SELECT"));
    }

    #[test]
    fn tab_reaches_the_name_last() {
        assert!(FieldKind::Port < FieldKind::Name);
        assert!(FieldKind::Code < FieldKind::Name);
    }

    #[test]
    fn the_lobby_calls_pirates_by_name() {
        use PirateClass::*;
        let roster = Roster(vec![
            crewmate(1, Some(Gunner), "Mira"),
            crewmate(2, Some(Doctor), ""),
            crewmate(3, None, "Sol"),
        ]);
        assert_eq!(taken_by(&roster, Gunner, 2).as_deref(), Some("Mira"));
        assert_eq!(taken_by(&roster, Doctor, 1).as_deref(), Some("P2"), "no name chosen");
        assert_eq!(taken_by(&roster, Gunner, 1), None, "not taken from its own holder");
        assert_eq!(taken_by(&roster, Hacker, 1), None);
        assert_eq!(waiting_on(&roster).as_deref(), Some("Sol"));
    }
}
