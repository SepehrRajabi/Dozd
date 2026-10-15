//! Options picked in the menu's settings screen, remembered between launches.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs};

use bevy::prelude::*;
use bevy::window::{Monitor, PresentMode, PrimaryWindow, WindowMoved, WindowResized};

/// Frame-rate caps the refresh rate option cycles through; `None` is uncapped.
pub const REFRESH_RATES: [Option<u32>; 6] = [None, Some(30), Some(60), Some(120), Some(144), Some(240)];

#[derive(Resource, Clone, PartialEq, Debug)]
pub struct Settings {
    /// Reinforcement waves once the alarm is raised. Only the host's choice counts.
    pub reinforcements: bool,
    pub vsync: bool,
    /// Frames per second the game won't exceed; `None` leaves it to vsync or the GPU.
    pub refresh_rate: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        // Reinforcements stay off while extraction is being tested.
        Self { reinforcements: false, vsync: true, refresh_rate: None }
    }
}

/// A file in this machine's DOZD folder: `%APPDATA%\dozd` or `~/.dozd`.
pub fn config_file(name: &str) -> Option<PathBuf> {
    let base = env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("dozd"));
    base.or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".dozd")))
        .map(|dir| dir.join(name))
}

/// The `key=value` lines of a file in the DOZD folder; empty if it's missing or unreadable.
pub fn read_config(name: &str) -> Vec<(String, String)> {
    let text = config_file(name).and_then(|path| fs::read_to_string(path).ok()).unwrap_or_default();
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// Writes `key=value` lines to a file in the DOZD folder. Failing to is harmless.
pub fn write_config(name: &str, pairs: &[(&str, String)]) {
    let Some(path) = config_file(name) else { return };
    let text: String = pairs.iter().map(|(key, value)| format!("{key}={value}\n")).collect();
    if let Err(error) = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&path, text)) {
        warn!("couldn't save {}: {error}", path.display());
    }
}

impl Settings {
    pub fn refresh_label(&self) -> String {
        self.refresh_rate.map_or("UNLIMITED".into(), |hz| format!("{hz} HZ"))
    }

    pub fn next_refresh_rate(&mut self) {
        let i = REFRESH_RATES.iter().position(|&r| r == self.refresh_rate).unwrap_or(0);
        self.refresh_rate = REFRESH_RATES[(i + 1) % REFRESH_RATES.len()];
    }

    /// The saved settings; anything missing or unreadable keeps its default.
    pub fn load() -> Self {
        let mut settings = Self::default();
        for (key, value) in read_config("settings.txt") {
            let value = value.as_str();
            match key.as_str() {
                "reinforcements" => settings.reinforcements = value == "on",
                "vsync" => settings.vsync = value == "on",
                "refresh_rate" => {
                    let rate = value.parse().ok();
                    if REFRESH_RATES.contains(&rate) {
                        settings.refresh_rate = rate;
                    }
                }
                _ => {}
            }
        }
        settings
    }

    /// Remembers these settings for the next launch. Failing to is harmless.
    pub fn save(&self) {
        let on = |b: bool| if b { "on" } else { "off" }.to_string();
        let rate = self.refresh_rate.map_or("unlimited".into(), |hz| hz.to_string());
        write_config(
            "settings.txt",
            &[("reinforcements", on(self.reinforcements)), ("vsync", on(self.vsync)), ("refresh_rate", rate)],
        );
    }
}

/// The window's size and place when the game last ran, so it opens the same way.
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct SavedWindow {
    /// In the desktop's scaled points, as the window was asked for.
    pub size: UVec2,
    /// The top-left corner in screen pixels; `None` lets the OS place it.
    pub position: Option<IVec2>,
}

impl Default for SavedWindow {
    fn default() -> Self {
        Self { size: UVec2::new(1280, 720), position: None }
    }
}

impl SavedWindow {
    /// The smallest size remembered, so a window squashed by accident still opens usable.
    const MIN_SIZE: UVec2 = UVec2::new(640, 360);

    pub fn load() -> Self {
        let mut saved = Self::default();
        let (mut width, mut height, mut x, mut y) = (None, None, None, None);
        for (key, value) in read_config("window.txt") {
            match key.as_str() {
                "width" => width = value.parse().ok(),
                "height" => height = value.parse().ok(),
                "x" => x = value.parse().ok(),
                "y" => y = value.parse().ok(),
                _ => {}
            }
        }
        if let (Some(width), Some(height)) = (width, height) {
            saved.size = UVec2::new(width, height).max(Self::MIN_SIZE);
        }
        if let (Some(x), Some(y)) = (x, y) {
            saved.position = Some(IVec2::new(x, y));
        }
        saved
    }

    fn of(window: &Window) -> Self {
        Self {
            size: UVec2::new(window.width().round() as u32, window.height().round() as u32),
            position: match window.position {
                WindowPosition::At(position) => Some(position),
                _ => None,
            },
        }
    }

    fn save(&self) {
        let mut pairs = vec![("width", self.size.x.to_string()), ("height", self.size.y.to_string())];
        if let Some(position) = self.position {
            pairs.extend([("x", position.x.to_string()), ("y", position.y.to_string())]);
        }
        write_config("window.txt", &pairs);
    }
}

/// Loads the saved settings and applies the display ones to the window.
pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load())
            .insert_resource(SavedWindow::load())
            .add_systems(Update, apply_vsync.run_if(resource_changed::<Settings>))
            .add_systems(Update, (place_window, remember_window))
            .add_systems(Last, limit_frame_rate);
    }
}

fn apply_vsync(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let mode = if settings.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if window.present_mode != mode {
        window.present_mode = mode;
    }
}

/// Moves the window back where it was last left, once the monitors are known,
/// unless that spot is on a monitor that's since been unplugged. Done after the
/// window opens because on macOS a starting position places the inside of the
/// window, not its frame, which would creep it up a title bar each launch.
fn place_window(
    mut done: Local<bool>,
    saved: Res<SavedWindow>,
    monitors: Query<&Monitor>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    if *done || monitors.is_empty() {
        return;
    }
    *done = true;
    let Some(corner) = saved.position else { return };
    // The title bar's left end, which the player needs to reach to drag it.
    let grip = corner + IVec2::new(40, 10);
    let visible = monitors.iter().any(|monitor| {
        let size = UVec2::new(monitor.physical_width, monitor.physical_height).as_ivec2();
        IRect::from_corners(monitor.physical_position, monitor.physical_position + size).contains(grip)
    });
    if visible {
        window.position = WindowPosition::At(corner);
    }
}

/// Saves the window's size and place a moment after the player stops moving or
/// resizing it, rather than on every step of the drag.
fn remember_window(
    time: Res<Time>,
    mut moved: MessageReader<WindowMoved>,
    mut resized: MessageReader<WindowResized>,
    mut wait: Local<Option<f32>>,
    window: Single<&Window, With<PrimaryWindow>>,
) {
    const SETTLE: f32 = 0.5;
    if moved.read().count() + resized.read().count() > 0 {
        *wait = Some(SETTLE);
    }
    let Some(left) = wait.as_mut() else { return };
    *left -= time.delta_secs();
    if *left <= 0.0 {
        *wait = None;
        SavedWindow::of(&window).save();
    }
}

/// Sleeps out the rest of each frame so the game never runs faster than the cap.
fn limit_frame_rate(settings: Res<Settings>, mut next_frame: Local<Option<Instant>>) {
    let now = Instant::now();
    let Some(hz) = settings.refresh_rate else {
        *next_frame = None;
        return;
    };
    let frame = Duration::from_secs_f64(1.0 / f64::from(hz));
    // Aim at fixed deadlines rather than "a frame after the last one", so sleep
    // overshoot doesn't add up; after a slow frame, start counting afresh.
    let deadline = next_frame.filter(|d| *d > now).unwrap_or(now);
    std::thread::sleep(deadline - now);
    *next_frame = Some(deadline + frame);
}
