//! Options picked in the menu's settings screen, remembered between launches.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{env, fs};

use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};

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
        let Some(text) = config_file("settings.txt").and_then(|path| fs::read_to_string(path).ok()) else {
            return settings;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
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
        let Some(path) = config_file("settings.txt") else { return };
        let on = |b: bool| if b { "on" } else { "off" };
        let rate = self.refresh_rate.map_or("unlimited".into(), |hz| hz.to_string());
        let text = format!(
            "reinforcements={}\nvsync={}\nrefresh_rate={rate}\n",
            on(self.reinforcements),
            on(self.vsync)
        );
        if let Err(error) = path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&path, text)) {
            warn!("couldn't save the settings to {}: {error}", path.display());
        }
    }
}

/// Loads the saved settings and applies the display ones to the window.
pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load())
            .add_systems(Update, apply_vsync.run_if(resource_changed::<Settings>))
            .add_systems(Last, limit_frame_rate);
    }
}

fn apply_vsync(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let mode = if settings.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if window.present_mode != mode {
        window.present_mode = mode;
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
