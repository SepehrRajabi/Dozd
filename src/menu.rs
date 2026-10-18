use std::f32::consts::TAU;

use bevy::app::AppExit;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::abilities::{Active, Passive};
use crate::boss::{KEEPER_COLOR, KeeperKind};
use crate::class::{LocalLoadout, Loadout, PirateClass};
use crate::guns::GunKind;
use crate::crew::Roster;
use crate::enemies::EnemyKind;
use crate::net::{
    ChooseLoadout, Connection, DEFAULT_PORT, EndSession, LocalId, NetMode, Notice, StartSession, local_ip, parse_port,
    resolve,
};
use crate::loot::LootKind;
use crate::perks::PerkKind;
use crate::player::Player;
use crate::settings::{Settings, read_config, write_config};
use crate::vault::Vault;
use crate::hud::{self, HudSlot};
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

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // Last launch's pick, ready before the first screen is drawn.
        app.insert_resource(LocalLoadout(Loadout::load().unwrap_or_default()))
            .add_sub_state::<MenuScreen>()
            .init_resource::<MenuMemory>()
            .init_resource::<LeavePrompt>()
            .add_observer(run_menu_command)
            .add_systems(Startup, spawn_session_hud)
            .add_systems(OnEnter(GameState::Menu), (spawn_backdrop, reset_camera, show_cursor))
            .add_systems(OnExit(GameState::Menu), hide_cursor)
            .add_systems(OnEnter(GameState::Lobby), (lobby_screen, show_cursor))
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
                    animate_backdrop,
                    back_on_escape,
                    dress_backdrop_pirate.run_if(resource_changed::<LocalLoadout>),
                    hide_backdrop.run_if(state_changed::<MenuScreen>),
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
            );
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
}

/// What the player typed last time, so hosting or rejoining is one click,
/// even after restarting the game.
#[derive(Resource)]
struct MenuMemory {
    port: String,
    address: String,
}

impl Default for MenuMemory {
    fn default() -> Self {
        let mut memory = Self { port: DEFAULT_PORT.to_string(), address: String::new() };
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

#[derive(Component)]
struct TextField {
    value: String,
    max_len: usize,
    digits_only: bool,
    /// Pressing Enter in the field runs this.
    submit: MenuCommand,
}

#[derive(Component)]
struct FieldText;

/// Blinks on its own, so the text beside it never changes with the blink.
#[derive(Component)]
struct Caret;

/// Shown steadily while the field is empty.
#[derive(Component)]
struct Placeholder;

#[derive(Component)]
struct HostHint;

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

fn text_field(value: &str, placeholder: &'static str, max_len: usize, digits_only: bool, submit: MenuCommand) -> impl Bundle {
    (
        TextField {
            value: value.to_string(),
            max_len,
            digits_only,
            submit,
        },
        Node {
            width: Val::Px(320.0),
            height: Val::Px(44.0),
            padding: UiRect::horizontal(Val::Px(12.0)),
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.02, 0.02, 0.05)),
        BorderColor::all(GOLD),
        children![
            (FieldText, Text::new(value), TextFont::from_font_size(20.0), TextColor(TEXT)),
            (
                Caret,
                Node {
                    width: Val::Px(2.0),
                    height: Val::Px(22.0),
                    margin: UiRect::horizontal(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(GOLD),
            ),
            (Placeholder, Text::new(placeholder), TextFont::from_font_size(20.0), TextColor(DIM)),
        ],
    )
}

fn status_line() -> impl Bundle {
    (
        StatusText,
        Text::new(""),
        TextFont::from_font_size(16.0),
        TextColor(ERROR),
        Node {
            max_width: Val::Px(560.0),
            margin: UiRect::top(Val::Px(6.0)),
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
    });
    commands.spawn((
        DespawnOnExit(MenuScreen::Main),
        label(
            format!("v{}   -   LAN co-op for up to {} pirates", env!("CARGO_PKG_VERSION"), PirateClass::ALL.len()),
            13.0,
            DIM,
        ),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(14.0),
            left: Val::Px(18.0),
            ..default()
        },
    ));
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
        (Setting::Reinforcements, "REINFORCEMENTS", "Guard waves arrive while the alarm is up. The host's setting counts."),
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

fn host_screen(mut commands: Commands, assets: Res<AssetServer>, memory: Res<MenuMemory>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Host);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("HOST A CREW", 24.0, GOLD));
        parent.spawn(label("Port", 14.0, DIM));
        parent.spawn(text_field(&memory.port, "5000", 5, true, MenuCommand::StartHost));
        parent.spawn((HostHint, label("", 16.0, TEXT)));
        parent.spawn(label(
            "Over the internet? Use Tailscale and share its IP.",
            13.0,
            DIM,
        ));
        parent.spawn(button("START HOSTING", MenuCommand::StartHost));
        parent.spawn(button("BACK", MenuCommand::Back));
        parent.spawn(status_line());
    });
}

fn join_screen(mut commands: Commands, assets: Res<AssetServer>, memory: Res<MenuMemory>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Join);
    commands.entity(root).with_children(|parent| {
        parent.spawn(label("JOIN A CREW", 24.0, GOLD));
        parent.spawn(label("Host address  (ip, ip:port or hostname)", 14.0, DIM));
        parent.spawn(text_field(&memory.address, "192.168.1.23", 40, false, MenuCommand::Connect));
        parent.spawn(button("CONNECT", MenuCommand::Connect));
        parent.spawn(button("BACK", MenuCommand::Back));
        parent.spawn(status_line());
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

/// A run of cards on a codex screen, under an optional heading.
struct Section {
    heading: Option<&'static str>,
    entries: Vec<Entry>,
}

/// A vault guardian's colour on its card.
fn boss_color(kind: KeeperKind) -> Color {
    match kind {
        KeeperKind::Hands => KEEPER_COLOR,
        // The orange of the marks it lays on its prey.
        KeeperKind::Stalker => Color::srgb(1.0, 0.4, 0.15),
        KeeperKind::Brute => Color::srgb(0.95, 0.6, 0.2),
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
        &[Section { heading: None, entries: entries.into() }],
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
        art: 80.0,
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
            Section { heading: None, entries: guards.into() },
            Section { heading: Some("BOSSES"), entries: bosses.into() },
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
                padding: UiRect::top(Val::Px(40.0)),
                row_gap: Val::Px(14.0),
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
                                    width: Val::Px(372.0),
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

fn run_menu_command(
    command: On<MenuCommand>,
    mut commands: Commands,
    mode: Res<NetMode>,
    mut memory: ResMut<MenuMemory>,
    mut notice: ResMut<Notice>,
    fields: Query<&TextField>,
    // Absent outside the menu, e.g. in the lobby.
    current: Option<Res<State<MenuScreen>>>,
    mut screen: ResMut<NextState<MenuScreen>>,
    mut settings: ResMut<Settings>,
    mut exit: MessageWriter<AppExit>,
    roster: Option<Single<&Roster>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let joining = matches!(*mode, NetMode::Join { .. });
    let field = fields.iter().next().map(|f| f.value.clone()).unwrap_or_default();

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
            memory.port = field.clone();
            match parse_port(&field) {
                Ok(port) => {
                    memory.save();
                    commands.trigger(StartSession(NetMode::Host { port }));
                }
                Err(message) => notice.0 = Some(message),
            }
        }
        MenuCommand::Connect if joining => {}
        MenuCommand::Connect => {
            memory.address = field.clone();
            match resolve(&field) {
                Ok(addr) => {
                    memory.save();
                    commands.trigger(StartSession(NetMode::Join { addr }));
                }
                Err(message) => notice.0 = Some(message),
            }
        }
        MenuCommand::StartRun => match roster.as_deref().and_then(|r| waiting_on(r)) {
            Some(slot) => notice.0 = Some(format!("Waiting for P{} to pick a class", slot + 1)),
            None => {
                notice.0 = None;
                next_state.set(GameState::Playing);
            }
        },
        MenuCommand::Leave => commands.trigger(EndSession(None)),
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

fn choice_actions(
    choices: Query<(&Interaction, &Choice), Changed<Interaction>>,
    state: Res<State<GameState>>,
    local: Res<LocalId>,
    roster: Option<Single<&Roster>>,
    mut loadout: ResMut<LocalLoadout>,
    mut notice: ResMut<Notice>,
) {
    // In the lobby, one of each class per crew.
    let taken_by = |class: PirateClass| {
        let roster = roster.as_deref().filter(|_| *state.get() == GameState::Lobby)?;
        let owner = roster.holder(class).filter(|owner| *owner != local.0)?;
        Some(roster.slot(owner).unwrap_or(0))
    };
    for (interaction, choice) in &choices {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let loadout = &mut loadout.0;
        notice.0 = match *choice {
            Choice::Class(class) if let Some(slot) = taken_by(class) => {
                notice.0 = Some(format!("P{} already plays the {}", slot + 1, class.name()));
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
    let taken_by = |class: PirateClass| {
        let owner = roster?.holder(class).filter(|owner| *owner != local.0)?;
        Some(roster?.slot(owner).unwrap_or(0))
    };
    for (note, mut text, mut color) in &mut notes {
        let (content, tint) = match (roster, taken_by(note.0)) {
            (None, _) => (String::new(), DIM),
            (_, Some(slot)) => (format!("TAKEN BY P{}", slot + 1), ERROR),
            (Some(roster), None) if roster.class_of(local.0) == Some(note.0) => ("YOURS".into(), GOLD),
            (Some(_), None) => (String::new(), DIM),
        };
        if text.0 != content {
            text.0 = content;
        }
        color.0 = tint;
    }

    let loadout = &loadout.0;
    for (interaction, choice, mut background, mut border) in &mut cards {
        let chosen = match *choice {
            Choice::Class(class) => loadout.class == class && taken_by(class).is_none(),
            Choice::Passive(passive) => loadout.passive == passive,
            Choice::Active(active) => loadout.active == Some(active),
            Choice::Gun(gun) => loadout.guns.contains(&gun),
        };
        let taken = matches!(*choice, Choice::Class(class) if taken_by(class).is_some());
        let (bg, edge) = match (interaction, chosen) {
            _ if taken => (Color::srgb(0.04, 0.04, 0.07), Color::NONE),
            (Interaction::Pressed, _) => (BUTTON_PRESSED, GOLD),
            (_, true) => (Color::srgb(0.2, 0.16, 0.07), GOLD),
            (Interaction::Hovered, false) => (BUTTON_HOVER, GOLD_FAINT),
            (Interaction::None, false) => (BUTTON, Color::NONE),
        };
        background.0 = bg;
        *border = BorderColor::all(edge);
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

fn edit_fields(mut commands: Commands, mut keys: MessageReader<KeyboardInput>, mut fields: Query<&mut TextField>) {
    let Some(mut field) = fields.iter_mut().next() else {
        keys.clear();
        return;
    };
    for key in keys.read() {
        if !key.state.is_pressed() {
            continue;
        }
        match &key.logical_key {
            Key::Character(text) => {
                for c in text.chars() {
                    let allowed = if field.digits_only {
                        c.is_ascii_digit()
                    } else {
                        c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-')
                    };
                    if allowed && field.value.len() < field.max_len {
                        field.value.push(c);
                    }
                }
            }
            Key::Backspace => {
                field.value.pop();
            }
            Key::Enter => commands.trigger(field.submit),
            _ => {}
        }
    }
}

fn draw_fields(
    time: Res<Time>,
    fields: Query<(&TextField, &Children)>,
    mut texts: Query<&mut Text, With<FieldText>>,
    mut carets: Query<&mut Visibility, (With<Caret>, Without<Placeholder>)>,
    mut placeholders: Query<&mut Visibility, With<Placeholder>>,
) {
    let caret_on = (time.elapsed_secs() * 2.0).fract() < 0.5;
    for (field, children) in &fields {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child)
                && text.0 != field.value
            {
                text.0.clone_from(&field.value);
            }
            if let Ok(mut visibility) = carets.get_mut(child) {
                visibility.set_if_neq(if caret_on { Visibility::Inherited } else { Visibility::Hidden });
            }
            if let Ok(mut visibility) = placeholders.get_mut(child) {
                let shown = field.value.is_empty();
                visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
    }
}

fn update_host_hint(fields: Query<&TextField>, mut hint: Query<&mut Text, With<HostHint>>, mut ip: Local<Option<String>>) {
    let Ok(mut hint) = hint.single_mut() else { return };
    let ip = ip.get_or_insert_with(|| local_ip().map_or("<your IP>".into(), |ip| ip.to_string()));
    let port = fields.iter().next().map(|f| f.value.as_str()).unwrap_or("");
    let content = match port.parse::<u16>() {
        Ok(port) if port == DEFAULT_PORT => format!("Your crew joins with:  {ip}"),
        Ok(port) => format!("Your crew joins with:  {ip}:{port}"),
        Err(_) => "Pick a port between 1 and 65535".into(),
    };
    if hint.0 != content {
        hint.0 = content;
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
        (NetMode::Join { addr }, _) if connecting => (
            format!("Connecting to {addr}...  {:.0}s", connection.waited),
            GOLD,
        ),
        (_, Some(message)) => (message.clone(), ERROR),
        _ => (String::new(), ERROR),
    };
    if text.0 != content {
        text.0 = content;
    }
    color.0 = tint;
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

fn reset_camera(mut camera: Single<&mut Transform, With<Camera2d>>) {
    camera.translation.x = 0.0;
    camera.translation.y = 0.0;
}

fn show_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = true;
}

fn hide_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.visible = false;
}

fn spawn_backdrop(mut commands: Commands, assets: Res<AssetServer>, loadout: Res<LocalLoadout>) {
    let mut sprite = |path: &str, pos: Vec3, scale: f32, flip: bool, orbit: Option<(Vec2, f32)>, phase: f32| {
        commands.spawn((
            DespawnOnExit(GameState::Menu),
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

/// The loadout, perks and enemies screens fill the window, so the backdrop would only clutter them.
fn hide_backdrop(screen: Res<State<MenuScreen>>, mut sprites: Query<&mut Visibility, With<Backdrop>>) {
    let full = matches!(screen.get(), MenuScreen::Loadout | MenuScreen::Perks | MenuScreen::Enemies);
    let visibility = if full { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut sprites {
        v.set_if_neq(visibility);
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
        slot.spawn((SessionText, label("", 14.0, DIM), Node {
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
    players: Query<(), With<Player>>,
    loot: Query<(), With<LootKind>>,
    vault: Option<Single<&Vault>>,
    mut session: Single<&mut Text, (With<SessionText>, Without<LeaveText>, Without<LootLeftText>)>,
    mut loot_left: Single<&mut Text, (With<LootLeftText>, Without<LeaveText>)>,
    mut leave: Single<&mut Text, With<LeaveText>>,
    mut leave_panel: Single<&mut Node, With<LeavePanel>>,
    mut ip: Local<Option<String>>,
) {
    let crew = players.iter().count();
    let content = match &*mode {
        NetMode::Solo => String::new(),
        NetMode::Host { port } => {
            let ip = ip.get_or_insert_with(|| local_ip().map_or("?".into(), |ip| ip.to_string()));
            format!("HOSTING  {ip}:{port}    CREW {crew}")
        }
        NetMode::Join { addr } => format!("CREW {crew}    host {addr}"),
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

    let warning = match (&*mode, prompt.timer > 0.0) {
        (_, false) => "",
        (NetMode::Host { .. }, true) => "Press ESC again to leave  (ends the session for your crew)",
        (_, true) => "Press ESC again to leave to the main menu",
    };
    if leave.0 != warning {
        leave.0 = warning.into();
    }
    let display = if warning.is_empty() { Display::None } else { Display::Flex };
    if leave_panel.display != display {
        leave_panel.display = display;
    }
}

fn leave_on_escape(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GameState>>,
    mut prompt: ResMut<LeavePrompt>,
) {
    prompt.timer = (prompt.timer - time.delta_secs()).max(0.0);
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // Mid-run, ESC needs confirming so it can't end a raid by accident.
    if matches!(state.get(), GameState::Over | GameState::Lobby) || prompt.timer > 0.0 {
        prompt.timer = 0.0;
        commands.trigger(EndSession(None));
    } else {
        prompt.timer = LEAVE_WINDOW;
    }
}

// ---------------------------------------------------------------------------
// Lobby: the crew picks classes and guns, then the host sets off
// ---------------------------------------------------------------------------

/// Under the title: where friends can join, or which host this is.
#[derive(Component)]
struct LobbyInfo;

/// A crewmate's tag in the lobby, by place in the roster.
#[derive(Component)]
struct CrewChip(usize);

/// What the crew is waiting on before the run can start.
#[derive(Component)]
struct LobbyHint;

/// The first pirate in the roster still choosing a class, if any.
fn waiting_on(roster: &Roster) -> Option<usize> {
    roster.0.iter().position(|(_, class)| class.is_none())
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
            // Hides the last run's frozen deck.
            BackgroundColor(Color::srgb(0.02, 0.02, 0.045)),
        ))
        .with_children(|parent| {
            parent.spawn(label("CREW LOBBY", 28.0, GOLD));
            parent.spawn((LobbyInfo, label("", 15.0, DIM)));
            parent
                .spawn(Node { column_gap: Val::Px(10.0), ..default() })
                .with_children(|chips| {
                    for i in 0..PirateClass::ALL.len() {
                        chips.spawn((
                            CrewChip(i),
                            hud::panel(Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                display: Display::None,
                                ..default()
                            }),
                            children![label("", 14.0, TEXT)],
                        ));
                    }
                });
            loadout_cards(parent, &assets);
            parent
                .spawn(Node { column_gap: Val::Px(16.0), align_items: AlignItems::Center, ..default() })
                .with_children(|buttons| {
                    if hosting {
                        buttons.spawn(button("START", MenuCommand::StartRun));
                    }
                    buttons.spawn(button("LEAVE", MenuCommand::Leave));
                });
            parent.spawn((LobbyHint, label("", 15.0, DIM)));
            parent.spawn(status_line());
        });
}

fn update_lobby(
    mode: Res<NetMode>,
    local: Res<LocalId>,
    roster: Option<Single<&Roster>>,
    mut info: Single<&mut Text, (With<LobbyInfo>, Without<LobbyHint>)>,
    mut hint: Single<&mut Text, (With<LobbyHint>, Without<LobbyInfo>)>,
    mut chips: Query<(&CrewChip, &mut Node, &mut BorderColor, &Children)>,
    mut chip_texts: Query<(&mut Text, &mut TextColor), (Without<LobbyInfo>, Without<LobbyHint>)>,
    mut ip: Local<Option<String>>,
) {
    let content = match &*mode {
        NetMode::Host { port } => {
            let ip = ip.get_or_insert_with(|| local_ip().map_or("?".into(), |ip| ip.to_string()));
            format!("Friends join at  {ip}:{port}")
        }
        NetMode::Join { addr } => format!("Aboard {addr}'s crew"),
        NetMode::Solo => String::new(),
    };
    if info.0 != content {
        info.0 = content;
    }

    let Some(roster) = roster else { return };
    for (chip, mut node, mut border, children) in &mut chips {
        let entry = roster.0.get(chip.0);
        let display = if entry.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let Some(&(owner, class)) = entry else { continue };
        let you = if owner == local.0 { "  (you)" } else { "" };
        let (content, color) = match class {
            Some(class) => (format!("P{}  {}{you}", chip.0 + 1, class.name()), class.color()),
            None => (format!("P{}  choosing...{you}", chip.0 + 1), DIM),
        };
        *border = BorderColor { top: color, ..BorderColor::all(hud::EDGE) };
        if let Some(Ok((mut text, mut tint))) = children.first().map(|c| chip_texts.get_mut(*c)) {
            if text.0 != content {
                text.0 = content;
            }
            tint.0 = color;
        }
    }

    let mine = roster.class_of(local.0);
    let content = match (waiting_on(&roster), &*mode) {
        _ if mine.is_none() => "Your class is taken: pick a free one".to_string(),
        (Some(slot), _) => format!("Waiting for P{} to pick a class", slot + 1),
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
