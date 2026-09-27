mod combat;
mod deckgen;
mod drops;
mod enemies;
mod guns;
mod headless;
mod loot;
mod menu;
mod mission;
mod net;
mod perks;
mod player;
mod room;

use bevy::prelude::*;
use bevy::window::CursorOptions;
use serde::{Deserialize, Serialize};

use net::{NetMode, StartSession};

/// World pixels are magnified by this factor on screen.
pub const PIXEL_SCALE: f32 = 3.0;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameState {
    #[default]
    Menu,
    Playing,
    /// The run ended; the world stays frozen behind the results screen.
    Over,
}

/// Everything belonging to a single run; cleared when a new run starts.
#[derive(Component)]
pub struct Level;

/// In-game HUD elements, hidden while the menu is up.
#[derive(Component)]
pub struct Hud;

#[derive(Resource)]
pub struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x2545_F491)
    }
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        // Xorshift is stuck at zero, and nearby seeds should still diverge.
        Self(seed.wrapping_mul(0x9E37_79B9) | 1)
    }

    /// Uniform in [0, 1].
    pub fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }

    /// Uniform in [-1, 1].
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("headless") {
        match headless::Config::from_args(&args[1..]) {
            Ok(config) => headless::run(config),
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
        return;
    }

    let cli_mode = match NetMode::from_args() {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    let mut app = App::new();
    embed_assets(&mut app);
    app.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.05)))
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    // Dev builds read straight from the repo so they work however
                    // they're launched; release builds use the embedded copy.
                    file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "DOZD".into(),
                        resolution: (1280, 720).into(),
                        ..default()
                    }),
                    // The in-world crosshair replaces the OS cursor.
                    primary_cursor_options: Some(CursorOptions {
                        visible: false,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_plugins((GamePlugins, ViewPlugins))
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, toggle_hud.run_if(state_changed::<GameState>));

    // A mode on the command line skips the menu.
    if let Some(mode) = cli_mode {
        app.add_systems(Startup, move |mut commands: Commands| {
            commands.trigger(StartSession(mode.clone()));
        });
    }
    app.run();
}

/// The game itself: rules, AI and networking. Needs no window, renderer, input
/// or assets, so it also runs headless.
pub struct GamePlugins;

impl Plugin for GamePlugins {
    fn build(&self, app: &mut App) {
        app.init_resource::<Rng>().init_state::<GameState>().add_plugins((
            net::NetPlugin,
            room::RoomPlugin,
            player::PlayerPlugin,
            loot::LootPlugin,
            guns::GunsPlugin,
            combat::CombatPlugin,
            enemies::EnemiesPlugin,
            mission::MissionPlugin,
            perks::PerksPlugin,
            drops::DropsPlugin,
        ));
    }
}

/// Everything a player sees and touches: sprites, effects, HUD, menus, input.
struct ViewPlugins;

impl Plugin for ViewPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            room::RoomViewPlugin,
            player::PlayerViewPlugin,
            loot::LootViewPlugin,
            guns::GunsViewPlugin,
            combat::CombatViewPlugin,
            enemies::EnemiesViewPlugin,
            mission::MissionViewPlugin,
            perks::PerksViewPlugin,
            drops::DropsViewPlugin,
            menu::MenuPlugin,
        ));
    }
}

fn toggle_hud(state: Res<State<GameState>>, mut hud: Query<&mut Visibility, With<Hud>>) {
    let visibility = if *state.get() == GameState::Menu {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut v in &mut hud {
        *v = visibility;
    }
}

#[cfg(not(debug_assertions))]
mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));
}

/// Release builds carry every asset inside the executable (see build.rs), so the
/// game is a single file to hand to friends.
#[cfg(not(debug_assertions))]
fn embed_assets(app: &mut App) {
    use bevy::asset::io::memory::{Dir, MemoryAssetReader};
    use bevy::asset::io::{AssetSourceBuilder, AssetSourceId};
    use std::path::Path;

    let dir = Dir::default();
    for (path, bytes) in embedded::ASSETS {
        dir.insert_asset(Path::new(path), *bytes);
    }
    // Must be registered before `AssetPlugin`, which otherwise installs the file reader.
    app.register_asset_source(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || Box::new(MemoryAssetReader { root: dir.clone() })),
    );
}

#[cfg(debug_assertions)]
fn embed_assets(_: &mut App) {}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / PIXEL_SCALE,
            ..OrthographicProjection::default_2d()
        }),
    ));
}
