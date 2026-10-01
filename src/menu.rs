use std::f32::consts::TAU;

use bevy::app::AppExit;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::class::{LocalLoadout, Loadout, PirateClass};
use crate::guns::GunKind;
use crate::net::{DEFAULT_PORT, EndSession, NetMode, Notice, Connection, StartSession, local_ip, parse_port, resolve};
use crate::loot::LootKind;
use crate::perks::PerkKind;
use crate::player::Player;
use crate::settings::Settings;
use crate::{GameState, Hud};

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
            .add_systems(OnEnter(MenuScreen::Main), main_screen)
            .add_systems(OnEnter(MenuScreen::Host), host_screen)
            .add_systems(OnEnter(MenuScreen::Join), join_screen)
            .add_systems(OnEnter(MenuScreen::Loadout), loadout_screen)
            .add_systems(OnEnter(MenuScreen::Perks), perks_screen)
            .add_systems(OnEnter(MenuScreen::Settings), settings_screen)
            .add_systems(
                Update,
                (
                    button_styles,
                    button_actions,
                    edit_fields,
                    draw_fields,
                    update_host_hint,
                    update_status,
                    animate_backdrop,
                    back_on_escape,
                    (choice_actions, style_choices, update_slot_count).chain(),
                    dress_backdrop_pirate.run_if(resource_changed::<LocalLoadout>),
                    hide_backdrop.run_if(state_changed::<MenuScreen>),
                    update_setting_values.run_if(resource_changed::<Settings>),
                )
                    .run_if(in_state(GameState::Menu)),
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
    Host,
    Join,
    Loadout,
    Perks,
    Settings,
}

#[derive(Component, Event, Clone, Copy, PartialEq, Eq, Debug)]
enum MenuCommand {
    Solo,
    OpenHost,
    OpenJoin,
    OpenLoadout,
    OpenPerks,
    OpenSettings,
    Toggle(Setting),
    Quit,
    StartHost,
    Connect,
    Back,
}

/// What the player typed last time, so rejoining after a disconnect is one click.
#[derive(Resource)]
struct MenuMemory {
    port: String,
    address: String,
}

impl Default for MenuMemory {
    fn default() -> Self {
        Self { port: DEFAULT_PORT.to_string(), address: String::new() }
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
    Gun(GunKind),
}

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
        parent.spawn(button("HOST A CREW", MenuCommand::OpenHost));
        parent.spawn(button("JOIN A CREW", MenuCommand::OpenJoin));
        parent.spawn(button(&loadout_label, MenuCommand::OpenLoadout));
        parent.spawn(button("PERKS", MenuCommand::OpenPerks));
        parent.spawn(button("SETTINGS", MenuCommand::OpenSettings));
        parent.spawn(button("QUIT", MenuCommand::Quit));
        parent.spawn(status_line());
    });
    commands.spawn((
        DespawnOnExit(MenuScreen::Main),
        label(
            format!("v{}   -   LAN co-op for up to 8 pirates", env!("CARGO_PKG_VERSION")),
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
            parent.spawn(row(16.0)).with_children(|classes| {
                for class in PirateClass::ALL {
                    classes
                        .spawn((
                            Button,
                            Choice::Class(class),
                            card(200.0, 130.0, FlexDirection::Column),
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
                        });
                }
            });

            parent.spawn((SlotCount, label("", 15.0, DIM)));
            parent.spawn(row(10.0)).with_children(|guns| {
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

            parent.spawn(button("DONE", MenuCommand::Back));
            parent.spawn(status_line());
        });
}

fn perks_screen(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            DespawnOnExit(MenuScreen::Perks),
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
            parent.spawn(label("PERKS", 28.0, GOLD));
            parent.spawn(label(
                "Found lying around each deck. Walk over one to use it.",
                15.0,
                DIM,
            ));
            parent
                .spawn(Node {
                    column_gap: Val::Px(16.0),
                    row_gap: Val::Px(16.0),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    max_width: Val::Px(900.0),
                    ..default()
                })
                .with_children(|grid| {
                    for perk in PerkKind::ALL {
                        let duration = perk.duration().map_or("INSTANT".into(), |secs| format!("{secs:.0}s"));
                        grid.spawn((
                            Node {
                                width: Val::Px(420.0),
                                height: Val::Px(128.0),
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(16.0),
                                padding: UiRect::all(Val::Px(14.0)),
                                border: UiRect::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(BUTTON),
                            BorderColor::all(perk.color().with_alpha(0.5)),
                        ))
                        .with_children(|card| {
                            card.spawn((
                                ImageNode::new(assets.load(perk.sprite())),
                                Node {
                                    width: Val::Px(64.0),
                                    height: Val::Px(64.0),
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
                                info.spawn(Node {
                                    column_gap: Val::Px(10.0),
                                    align_items: AlignItems::Baseline,
                                    ..default()
                                })
                                .with_children(|title| {
                                    title.spawn(label(perk.name(), 20.0, perk.color()));
                                    title.spawn(label(perk.category(), 12.0, DIM));
                                    title.spawn(label(duration, 14.0, TEXT));
                                });
                                info.spawn(label(perk.details(), 14.0, TEXT));
                            });
                        });
                    }
                });
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
    mut screen: ResMut<NextState<MenuScreen>>,
    mut settings: ResMut<Settings>,
    mut exit: MessageWriter<AppExit>,
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
        MenuCommand::OpenPerks => screen.set(MenuScreen::Perks),
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
                Ok(port) => commands.trigger(StartSession(NetMode::Host { port })),
                Err(message) => notice.0 = Some(message),
            }
        }
        MenuCommand::Connect if joining => {}
        MenuCommand::Connect => {
            memory.address = field.clone();
            match resolve(&field) {
                Ok(addr) => commands.trigger(StartSession(NetMode::Join { addr })),
                Err(message) => notice.0 = Some(message),
            }
        }
        MenuCommand::Back => {
            if joining {
                // Cancels the pending connection; stays on this screen.
                commands.trigger(EndSession(None));
            } else {
                notice.0 = None;
                screen.set(MenuScreen::Main);
            }
        }
    }
}

fn choice_actions(
    choices: Query<(&Interaction, &Choice), Changed<Interaction>>,
    mut loadout: ResMut<LocalLoadout>,
    mut notice: ResMut<Notice>,
) {
    for (interaction, choice) in &choices {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let loadout = &mut loadout.0;
        notice.0 = match *choice {
            Choice::Class(class) => {
                loadout.set_class(class);
                None
            }
            Choice::Gun(gun) if loadout.guns == [gun] => Some("Take at least one gun".into()),
            Choice::Gun(gun) if !loadout.toggle(gun) => Some(format!(
                "All {} slots are full. Click an equipped gun to drop it first.",
                loadout.class.slots()
            )),
            Choice::Gun(_) => None,
        };
        loadout.save();
    }
}

/// Highlights the chosen class and equipped guns, and numbers the guns by slot.
fn style_choices(
    loadout: Res<LocalLoadout>,
    mut cards: Query<(&Interaction, &Choice, &mut BackgroundColor, &mut BorderColor)>,
    mut badges: Query<(&SlotBadge, &mut Text)>,
) {
    let loadout = &loadout.0;
    for (interaction, choice, mut background, mut border) in &mut cards {
        let chosen = match *choice {
            Choice::Class(class) => loadout.class == class,
            Choice::Gun(gun) => loadout.guns.contains(&gun),
        };
        let (bg, edge) = match (interaction, chosen) {
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

fn update_slot_count(loadout: Res<LocalLoadout>, mut text: Query<&mut Text, With<SlotCount>>) {
    let Ok(mut text) = text.single_mut() else { return };
    let loadout = &loadout.0;
    let content = format!(
        "Guns  {}/{}   -   click to equip in the next slot, again to drop",
        loadout.guns.len(),
        loadout.class.slots()
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
    notice: Res<Notice>,
    connection: Res<Connection>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusText>>,
) {
    let Ok((mut text, mut color)) = status.single_mut() else { return };
    let (content, tint) = match (&*mode, &notice.0) {
        (NetMode::Join { addr }, _) => (
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

/// The loadout and perks screens fill the window, so the backdrop would only clutter them.
fn hide_backdrop(screen: Res<State<MenuScreen>>, mut sprites: Query<&mut Visibility, With<Backdrop>>) {
    let full = matches!(screen.get(), MenuScreen::Loadout | MenuScreen::Perks);
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

fn animate_backdrop(time: Res<Time>, mut sprites: Query<(&Backdrop, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (backdrop, mut transform) in &mut sprites {
        transform.translation = match backdrop.orbit {
            Some((center, radius)) => {
                let angle = t * 0.35 + backdrop.phase;
                // Squashed ellipse reads as a ring around the pirate; items behind him sit lower in z.
                let offset = Vec2::new(angle.cos() * radius, angle.sin() * radius * 0.45);
                let z = if angle.sin() > 0.0 { 9.5 } else { 12.0 };
                (center + offset + Vec2::new(0.0, (t * 2.0 + backdrop.phase).sin() * 2.0)).extend(z)
            }
            None => backdrop.base + Vec3::new(0.0, ((t * 1.6 + backdrop.phase).sin() * 2.0).round(), 0.0),
        };
    }
}

// ---------------------------------------------------------------------------
// In-game: session info and leaving
// ---------------------------------------------------------------------------

fn spawn_session_hud(mut commands: Commands) {
    commands.spawn((
        Hud,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            right: Val::Px(18.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(6.0),
            ..default()
        },
        children![
            (SessionText, label("", 14.0, DIM)),
            (LootLeftText, label("", 14.0, DIM)),
            (LeaveText, label("", 15.0, GOLD)),
        ],
    ));
}

fn update_session_hud(
    mode: Res<NetMode>,
    prompt: Res<LeavePrompt>,
    players: Query<(), With<Player>>,
    loot: Query<(), With<LootKind>>,
    mut session: Single<&mut Text, (With<SessionText>, Without<LeaveText>, Without<LootLeftText>)>,
    mut loot_left: Single<&mut Text, (With<LootLeftText>, Without<LeaveText>)>,
    mut leave: Single<&mut Text, With<LeaveText>>,
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
    let remaining = format!("LOOT LEFT {}", loot.iter().count());
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
    if *state.get() == GameState::Over || prompt.timer > 0.0 {
        prompt.timer = 0.0;
        commands.trigger(EndSession(None));
    } else {
        prompt.timer = LEAVE_WINDOW;
    }
}
