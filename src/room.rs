use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::combat::{Health, Noise};
use crate::deckgen::{self, Layout};
use crate::enemies::{EnemyKind, spawn_enemy};
use crate::loot::{LootKind, spawn_loot};
use crate::mission::MissionStatus;
use crate::net::{NetMode, authority};
use crate::perks::{PerkKind, spawn_pickup};
use crate::player::{OFF_DECK, PirateStatus, Player, crew_spot, spawn_crew};
use crate::{GameState, Level};

pub const TILE: f32 = 16.0;
/// Decks per heist; the last one holds the vault.
pub const DECKS: u8 = 3;
const HULL: Color = Color::srgb(0.055, 0.06, 0.085);

// Layouts come from `deckgen`. Legend:
// '#' wall   'D' door   '.' floor   ',' grate   '=' hazard (escape pad)   'v' vent
// 'L' lift down   'u' lift shaft the crew arrived by
// loot: c chip, i ingot, x crystal, r relic, p plasma, w crate
// guards: d sentry drone, s stalker, h warden
// perks: 1 overdrive, 2 nano patch, 3 shock field, 4 cloak

pub struct RoomPlugin;

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlowField>()
            .init_resource::<Heist>()
            .init_resource::<BuiltDeck>()
            .add_systems(
                OnEnter(GameState::Playing),
                (clear_level, new_heist.run_if(authority), sync_deck, spawn_crew.run_if(authority)).chain(),
            )
            .add_systems(
                Update,
                (
                    sync_deck.run_if(not(in_state(GameState::Menu))),
                    update_flow_field
                        .run_if(in_state(GameState::Playing))
                        .run_if(authority),
                )
                    .chain(),
            );
    }
}

pub struct RoomViewPlugin;

impl Plugin for RoomViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_starfield).add_observer(draw_deck);
    }
}

/// Host only: the heist being played and how deep the crew is. Clients read the
/// same numbers from `MissionStatus`.
#[derive(Resource, Default)]
pub struct Heist {
    pub seed: u32,
    pub deck: u8,
}

/// Host only: if present, the next run plays this layout instead of a random one.
#[derive(Resource)]
pub struct NextSeed(pub u32);

/// A deck's collision grid is in place; the view draws its tiles.
#[derive(Event)]
struct DeckBuilt;

/// The `(seed, deck)` whose tiles this machine has built.
#[derive(Resource, Default)]
struct BuiltDeck(Option<(u32, u8)>);

#[derive(Resource)]
pub struct RoomGrid {
    tiles: Vec<Vec<char>>,
    /// World position of the centre of tile (0, 0).
    origin: Vec2,
    /// Where reinforcements enter: vents and the tile inside each door.
    pub spawn_points: Vec<Vec2>,
    /// Where the crew starts.
    pub crew_spawn: Vec2,
}

impl RoomGrid {
    fn cols(&self) -> usize {
        self.tiles[0].len()
    }

    fn rows(&self) -> usize {
        self.tiles.len()
    }

    /// Whether any of the eight surrounding tiles is walkable.
    fn touches_floor(&self, col: usize, row: usize) -> bool {
        (row.saturating_sub(1)..=(row + 1).min(self.rows() - 1)).any(|r| {
            (col.saturating_sub(1)..=(col + 1).min(self.cols() - 1)).any(|c| self.is_open(c, r))
        })
    }

    pub fn tile_center(&self, col: usize, row: usize) -> Vec2 {
        self.origin + Vec2::new(col as f32 * TILE, -(row as f32) * TILE)
    }

    pub fn tile_of(&self, p: Vec2) -> Option<(usize, usize)> {
        let col = ((p.x - self.origin.x) / TILE + 0.5).floor();
        let row = ((self.origin.y - p.y) / TILE + 0.5).floor();
        if col < 0.0 || row < 0.0 || col as usize >= self.cols() || row as usize >= self.rows() {
            return None;
        }
        Some((col as usize, row as usize))
    }

    fn is_open(&self, col: usize, row: usize) -> bool {
        deckgen::is_open(self.tiles[row][col])
    }

    pub fn tile_at(&self, p: Vec2) -> Option<char> {
        self.tile_of(p).map(|(c, r)| self.tiles[r][c])
    }

    pub fn is_solid_at(&self, p: Vec2) -> bool {
        self.tile_of(p).is_none_or(|(c, r)| !self.is_open(c, r))
    }

    /// True if an axis-aligned box centred at `center` overlaps any solid tile.
    pub fn collides(&self, center: Vec2, half: Vec2) -> bool {
        [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(-half.x, half.y),
            Vec2::new(half.x, half.y),
        ]
        .iter()
        .any(|&o| self.is_solid_at(center + o))
    }

    pub fn line_of_sight(&self, from: Vec2, to: Vec2) -> bool {
        let steps = (from.distance(to) / 4.0).ceil().max(1.0) as u32;
        (1..steps).all(|i| !self.is_solid_at(from.lerp(to, i as f32 / steps as f32)))
    }

    /// Moves a box by `delta`, resolving each axis separately so it slides along walls.
    pub fn slide(&self, pos: Vec2, delta: Vec2, half: Vec2) -> Vec2 {
        let mut pos = pos;
        for step in [Vec2::new(delta.x, 0.0), Vec2::new(0.0, delta.y)] {
            if !self.collides(pos + step, half) {
                pos += step;
            }
        }
        pos
    }

    /// Open 8-way neighbours; diagonals are only allowed when both sides are open,
    /// so paths never clip wall corners.
    fn neighbors(&self, col: usize, row: usize) -> impl Iterator<Item = (usize, usize, u16)> + '_ {
        const DIRS: [(i32, i32, u16); 8] = [
            (1, 0, 10),
            (-1, 0, 10),
            (0, 1, 10),
            (0, -1, 10),
            (1, 1, 14),
            (1, -1, 14),
            (-1, 1, 14),
            (-1, -1, 14),
        ];
        DIRS.into_iter().filter_map(move |(dc, dr, cost)| {
            let c = col as i32 + dc;
            let r = row as i32 + dr;
            if c < 0 || r < 0 || c as usize >= self.cols() || r as usize >= self.rows() {
                return None;
            }
            let (c, r) = (c as usize, r as usize);
            let diagonal_ok = dc == 0
                || dr == 0
                || (self.is_open((col as i32 + dc) as usize, row) && self.is_open(col, (row as i32 + dr) as usize));
            (self.is_open(c, r) && diagonal_ok).then_some((c, r, cost))
        })
    }
}

/// Path distance from every tile to the nearest living pirate, so any number of
/// enemies can navigate around walls with one Dijkstra pass per crew move.
#[derive(Resource, Default)]
pub struct FlowField {
    dist: Vec<Vec<u16>>,
    targets: Vec<(usize, usize)>,
}

impl FlowField {
    /// Direction toward the next tile on the shortest path, or `None` if already
    /// in the target tile or unreachable.
    pub fn step(&self, grid: &RoomGrid, from: Vec2) -> Option<Vec2> {
        let (col, row) = grid.tile_of(from)?;
        let here = *self.dist.get(row)?.get(col)?;
        if here == 0 || here == u16::MAX {
            return None;
        }
        let (c, r, _) = grid
            .neighbors(col, row)
            .min_by_key(|&(c, r, _)| self.dist[r][c])?;
        (grid.tile_center(c, r) - from).try_normalize()
    }
}

fn update_flow_field(
    grid: Res<RoomGrid>,
    mut field: ResMut<FlowField>,
    players: Query<(&Transform, &Health), With<Player>>,
) {
    let mut targets: Vec<(usize, usize)> = players
        .iter()
        .filter(|(_, health)| !health.is_dead())
        .filter_map(|(t, _)| grid.tile_of(t.translation.truncate()))
        .collect();
    targets.sort_unstable();
    targets.dedup();
    if field.targets == targets {
        return;
    }

    let mut dist = vec![vec![u16::MAX; grid.cols()]; grid.rows()];
    let mut queue = BinaryHeap::new();
    for &(col, row) in &targets {
        dist[row][col] = 0;
        queue.push(Reverse((0u16, col, row)));
    }
    while let Some(Reverse((d, col, row))) = queue.pop() {
        if d > dist[row][col] {
            continue;
        }
        for (c, r, cost) in grid.neighbors(col, row) {
            let nd = d + cost;
            if nd < dist[r][c] {
                dist[r][c] = nd;
                queue.push(Reverse((nd, c, r)));
            }
        }
    }

    field.dist = dist;
    field.targets = targets;
}

fn clear_level(mut commands: Commands, level: Query<Entity, With<Level>>, mut built: ResMut<BuiltDeck>) {
    built.0 = None;
    for entity in &level {
        commands.entity(entity).despawn();
    }
}

fn new_heist(mut heist: ResMut<Heist>, next: Option<Res<NextSeed>>) {
    let seed = next.map_or_else(
        || {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
                | 1
        },
        |next| next.0,
    );
    *heist = Heist { seed, deck: 0 };
}

/// Builds the current deck whenever it changes: at the start of a run and each
/// time the lift goes down. Runs on every machine; only `(seed, deck)` is shared.
fn sync_deck(
    mut commands: Commands,
    mode: Res<NetMode>,
    heist: Res<Heist>,
    status: Option<Single<&MissionStatus>>,
    mut built: ResMut<BuiltDeck>,
    mut field: ResMut<FlowField>,
    mut noise: ResMut<Noise>,
    stale: Query<Entity, (With<Level>, Without<Player>, Without<Remote>)>,
    mut crew: Query<(&Player, &Health, &mut Transform)>,
) {
    let target = if mode.is_authority() {
        (heist.seed, heist.deck)
    } else {
        let Some(status) = status else { return };
        (status.seed, status.deck)
    };
    if built.0 == Some(target) {
        return;
    }
    built.0 = Some(target);

    // The old deck's tiles, guards, loot and effects.
    for entity in &stale {
        commands.entity(entity).despawn();
    }
    *field = FlowField::default();
    noise.0.clear();

    let grid = build_grid(deckgen::generate(target.0, target.1, DECKS));
    if mode.is_authority() {
        populate(&mut commands, &grid);
        let mut spot = 0;
        for (player, health, mut transform) in &mut crew {
            let pos = if player.deck == target.1 && player.status == PirateStatus::Active && !health.is_dead() {
                spot += 1;
                crew_spot(grid.crew_spawn, spot - 1)
            } else {
                // Left on an upper deck: out of reach of this deck's bullets.
                OFF_DECK
            };
            transform.translation = pos.extend(transform.translation.z);
        }
    }
    commands.insert_resource(grid);
    commands.trigger(DeckBuilt);
}

/// The collision grid for a layout.
fn build_grid(layout: Layout) -> RoomGrid {
    let rows = layout.tiles.len();
    let cols = layout.tiles[0].len();
    let mut grid = RoomGrid {
        tiles: layout.tiles,
        origin: Vec2::new(-(cols as f32 - 1.0) * TILE / 2.0, (rows as f32 - 1.0) * TILE / 2.0),
        spawn_points: Vec::new(),
        crew_spawn: Vec2::ZERO,
    };
    grid.crew_spawn = grid.tile_center(layout.spawn.0, layout.spawn.1);

    for row in 0..rows {
        for col in 0..cols {
            let pos = grid.tile_center(col, row);
            match grid.tiles[row][col] {
                'v' => grid.spawn_points.push(pos),
                'D' => {
                    let inside = grid
                        .neighbors(col, row)
                        .next()
                        .map(|(c, r, _)| grid.tile_center(c, r));
                    grid.spawn_points.extend(inside);
                }
                _ => {}
            }
        }
    }
    grid
}

/// Spawns the tile sprites for the deck just built. They carry `Level`, so the
/// next deck's `sync_deck` clears them.
fn draw_deck(_: On<DeckBuilt>, mut commands: Commands, assets: Res<AssetServer>, grid: Res<RoomGrid>) {
    let floor = assets.load("sprites/tiles/floor.png");
    let grate = assets.load("sprites/tiles/grate.png");
    let wall = assets.load("sprites/tiles/wall.png");
    let hazard = assets.load("sprites/tiles/hazard.png");
    let door = assets.load("sprites/tiles/door.png");
    let vent = assets.load("sprites/tiles/vent.png");
    let lift = assets.load("sprites/tiles/lift.png");

    for row in 0..grid.rows() {
        for col in 0..grid.cols() {
            let sprite = match grid.tiles[row][col] {
                // Solid rock with no floor next to it is just dark hull.
                '#' if !grid.touches_floor(col, row) => Sprite::from_color(HULL, Vec2::splat(TILE)),
                '#' => Sprite::from_image(wall.clone()),
                'D' => Sprite::from_image(door.clone()),
                ',' => Sprite::from_image(grate.clone()),
                '=' => Sprite::from_image(hazard.clone()),
                'v' => Sprite::from_image(vent.clone()),
                'L' => Sprite::from_image(lift.clone()),
                'u' => Sprite {
                    image: lift.clone(),
                    color: Color::srgb(0.45, 0.45, 0.5),
                    ..default()
                },
                _ => Sprite::from_image(floor.clone()),
            };
            let pos = grid.tile_center(col, row);
            commands.spawn((Level, sprite, Transform::from_translation(pos.extend(0.0))));
        }
    }
}

/// Places loot and guards. Host only; everyone else receives them by replication.
fn populate(commands: &mut Commands, grid: &RoomGrid) {
    for (row, line) in grid.tiles.iter().enumerate() {
        for (col, &ch) in line.iter().enumerate() {
            let pos = grid.tile_center(col, row);
            if let Some(kind) = LootKind::from_char(ch) {
                spawn_loot(commands, kind, pos);
            } else if let Some(kind) = EnemyKind::from_char(ch) {
                spawn_enemy(commands, kind, pos, false);
            } else if let Some(kind) = PerkKind::from_char(ch) {
                spawn_pickup(commands, kind, pos);
            }
        }
    }
}

fn spawn_starfield(mut commands: Commands) {
    // Small xorshift so the sky is identical every run without pulling in `rand`.
    let mut seed: u32 = 0x9E37_79B9;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };

    for _ in 0..400 {
        let pos = Vec2::new(next() * 1200.0 - 600.0, next() * 800.0 - 400.0);
        let brightness = 0.3 + next() * 0.7;
        let size = if next() > 0.9 { 2.0 } else { 1.0 };
        commands.spawn((
            Sprite::from_color(
                Color::srgb(brightness * 0.8, brightness * 0.85, brightness),
                Vec2::splat(size),
            ),
            Transform::from_translation(pos.extend(-10.0)),
        ));
    }
}
