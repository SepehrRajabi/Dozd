use std::f32::consts::TAU;

use bevy::app::AppExit;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::net::{DEFAULT_PORT, EndSession, NetMode, Notice, Connection, StartSession, local_ip, parse_port, resolve};
use crate::loot::LootKind;
use crate::player::Player;
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
        app.add_sub_state::<MenuScreen>()
            .init_resource::<MenuMemory>()
            .init_resource::<LeavePrompt>()
            .add_observer(run_menu_command)
            .add_systems(Startup, spawn_session_hud)
            .add_systems(OnEnter(GameState::Menu), (spawn_backdrop, reset_camera, show_cursor))
            .add_systems(OnExit(GameState::Menu), hide_cursor)
            .add_systems(OnEnter(MenuScreen::Main), main_screen)
            .add_systems(OnEnter(MenuScreen::Host), host_screen)
            .add_systems(OnEnter(MenuScreen::Join), join_screen)
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
}

#[derive(Component, Event, Clone, Copy, PartialEq, Eq, Debug)]
enum MenuCommand {
    Solo,
    OpenHost,
    OpenJoin,
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

fn main_screen(mut commands: Commands, assets: Res<AssetServer>) {
    let root = screen_root(&mut commands, &assets, MenuScreen::Main);
    commands.entity(root).with_children(|parent| {
        parent.spawn(button("SOLO RAID", MenuCommand::Solo));
        parent.spawn(button("HOST A CREW", MenuCommand::OpenHost));
        parent.spawn(button("JOIN A CREW", MenuCommand::OpenJoin));
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

// ---------------------------------------------------------------------------
// Interaction
// ---------------------------------------------------------------------------

fn button_styles(
    mut buttons: Query<(&Interaction, &mut BackgroundColor, &mut BorderColor), (Changed<Interaction>, With<Button>)>,
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

fn spawn_backdrop(mut commands: Commands, assets: Res<AssetServer>) {
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
        ));
    };

    let pirate = Vec2::new(-140.0, -30.0);
    sprite("sprites/shadow.png", pirate.extend(9.0), 3.0, false, None, 0.0);
    sprite("sprites/player.png", pirate.extend(10.0), 3.0, false, None, 0.0);
    sprite("sprites/guns/pulse_rifle.png", (pirate + Vec2::new(12.0, -9.0)).extend(11.0), 2.2, false, None, 0.0);

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
