use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::boss::{KeeperKind, spawn_keeper};
use crate::breach::spawn_breach;
use crate::challenge::Challenge;
use crate::combat::{Health, Noise};
use crate::deckgen::{self, Layout};
use crate::enemies::{Enemy, EnemyKind, spawn_enemy};
use crate::loot::{LootKind, spawn_loot};
use crate::mission::MissionStatus;
use crate::net::{NetMode, authority};
use crate::perks::{PerkKind, spawn_pickup};
use crate::player::{OFF_DECK, PirateStatus, Player, crew_spot, spawn_crew};
use crate::vault::spawn_vault;
use crate::{GameState, Level};

pub const TILE: f32 = 16.0;
/// Decks per heist; each holds a vault, and only the last has no lift down.
pub const DECKS: u8 = 3;
/// Room a hacked guard riding down with the crew needs: the biggest guard's.
const CARRIED_HALF: f32 = 6.0;

// Layouts come from `deckgen`. Legend:
// '#' wall   'D' door   '.' floor   ',' grate   '=' hazard (escape pad)   'v' vent
// 'L' lift down   'u' lift shaft the crew arrived by   'V' vault (2x2, one per deck)
// 'G' vault gate (2 tiles, the only way into the vault room; open once its keeper is down)
// loot: c chip, i ingot, x crystal, r relic, p plasma, w crate
// guards: d sentry drone, s stalker, h warden, b brute, g herald
// perks: 1 overdrive, 2 nano patch, 3 shock field, 4 cloak, 5 recharge

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
                    sync_deck.run_if(in_state(GameState::Playing).or_else(in_state(GameState::Over))),
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
    /// Centre of the deck's vault.
    pub vault: Option<Vec2>,
    /// The vault room's gate.
    pub gate: GateSpot,
    /// The vault room's floor, in world space.
    pub vault_room: Rect,
    /// Gate tiles let things through (see `breach.rs`, which keeps this in step).
    gate_open: bool,
}

/// Where the vault room's gate is, and what's around it, in world space.
#[derive(Clone, Copy, Debug)]
pub struct GateSpot {
    /// Centre of the two gate tiles.
    pub centre: Vec2,
    /// The gate runs up and down, in the room's left or right wall.
    pub vertical: bool,
    /// Corridor floor just outside, level with `centre`.
    pub outside: Vec2,
    /// Vault room floor just inside, level with `centre`.
    pub inside: Vec2,
    /// Where the keeper waits, across the vault from the gate.
    pub keeper: Vec2,
}

impl RoomGrid {
    /// Whether `p` is on the vault room's floor.
    pub fn in_vault_room(&self, p: Vec2) -> bool {
        self.vault_room.contains(p)
    }

    fn cols(&self) -> usize {
        self.tiles[0].len()
    }

    fn rows(&self) -> usize {
        self.tiles.len()
    }

    /// Whether any of the eight surrounding tiles is floor, the gate included.
    fn touches_floor(&self, col: usize, row: usize) -> bool {
        (row.saturating_sub(1)..=(row + 1).min(self.rows() - 1)).any(|r| {
            (col.saturating_sub(1)..=(col + 1).min(self.cols() - 1)).any(|c| self.is_floor(c, r))
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
        match self.tiles[row][col] {
            'G' => self.gate_open,
            ch => deckgen::is_open(ch),
        }
    }

    /// Floor as drawn: like `is_open`, but the gate always counts.
    fn is_floor(&self, col: usize, row: usize) -> bool {
        deckgen::is_open(self.tiles[row][col])
    }

    pub fn gate_open(&self) -> bool {
        self.gate_open
    }

    /// Opens or shuts the vault gate's tiles.
    pub fn set_gate_open(&mut self, open: bool) {
        self.gate_open = open;
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
    /// Whether the vault gate was open when this was worked out.
    gate_open: bool,
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

    /// Paths to the nearest of `targets` (tiles), over the whole deck.
    pub fn toward(grid: &RoomGrid, targets: Vec<(usize, usize)>) -> Self {
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
        Self { dist, targets, gate_open: grid.gate_open }
    }
}

fn update_flow_field(
    grid: Res<RoomGrid>,
    mut field: ResMut<FlowField>,
    players: Query<(&Transform, &Health, &Player)>,
) {
    let mut targets: Vec<(usize, usize)> = players
        .iter()
        .filter(|(_, health, player)| !health.is_dead() && player.status == PirateStatus::Active)
        .filter_map(|(t, ..)| grid.tile_of(t.translation.truncate()))
        .collect();
    targets.sort_unstable();
    targets.dedup();
    if field.targets != targets || field.gate_open != grid.gate_open {
        *field = FlowField::toward(&grid, targets);
    }
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
    stale: Query<Entity, (With<Level>, Without<Player>, Without<Remote>, Without<Challenge>)>,
    mut crew: Query<(&Player, &Health, &mut Transform)>,
    mut hacked: Query<(&mut Enemy, &Health, &mut Transform), Without<Player>>,
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

    // The old deck's tiles, guards, loot and effects. Challenges outlive the
    // deck: the host resolves deck-span ones itself. Guards a Hacker turned
    // ride down with the crew.
    let rides = |entity| hacked.get(entity).is_ok_and(|(enemy, health, _)| enemy.is_hacked() && !health.is_dead());
    for entity in stale.iter().filter(|e| !rides(*e)) {
        commands.entity(entity).despawn();
    }
    *field = FlowField::default();
    noise.0.clear();

    let grid = build_grid(deckgen::generate(target.0, target.1, DECKS));
    // `breach.rs` opens the gate once the deck's `Breach` says so.
    if mode.is_authority() {
        populate(&mut commands, &grid, target.0, target.1);
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
        land_hacked(&grid, &mut hacked);
    }
    commands.insert_resource(grid);
    commands.trigger(DeckBuilt);
}

/// Sets the guards fighting for the crew down on open floor around where the
/// crew lands, in rings past them, each in sight of the crew's spot.
fn land_hacked(grid: &RoomGrid, guards: &mut Query<(&mut Enemy, &Health, &mut Transform), Without<Player>>) {
    let centre = grid.crew_spawn;
    let mut spots = (0..4).flat_map(|ring| {
        let radius = TILE * (1.5 + ring as f32);
        let count = 8 + 4 * ring;
        (0..count).map(move |i| centre + Vec2::from_angle(std::f32::consts::TAU * (i as f32 + 0.5) / count as f32) * radius)
    });
    for (mut enemy, health, mut transform) in guards.iter_mut() {
        if !enemy.is_hacked() || health.is_dead() {
            continue;
        }
        let pos = spots
            .by_ref()
            .find(|p| !grid.collides(*p, Vec2::splat(CARRIED_HALF)) && grid.line_of_sight(centre, *p))
            .unwrap_or(centre);
        enemy.arrive(pos);
        transform.translation = pos.extend(transform.translation.z);
    }
}

/// The collision grid for a layout.
fn build_grid(layout: Layout) -> RoomGrid {
    let rows = layout.tiles.len();
    let cols = layout.tiles[0].len();
    let origin = Vec2::new(-(cols as f32 - 1.0) * TILE / 2.0, (rows as f32 - 1.0) * TILE / 2.0);
    let at = |(col, row): (usize, usize)| origin + Vec2::new(col as f32 * TILE, -(row as f32) * TILE);
    // The gate's spots sit beside its first tile; shift them half a tile along it.
    let along = (at(layout.gate[1]) - at(layout.gate[0])) / 2.0;
    let gate = GateSpot {
        centre: at(layout.gate[0]) + along,
        vertical: layout.gate[0].0 == layout.gate[1].0,
        outside: at(layout.gate_outside) + along,
        inside: at(layout.gate_inside) + along,
        keeper: at(layout.keeper) + along,
    };
    let mut grid = RoomGrid {
        tiles: layout.tiles,
        origin,
        spawn_points: Vec::new(),
        crew_spawn: Vec2::ZERO,
        vault: None,
        gate,
        vault_room: {
            let (x, y, w, h) = layout.vault_room;
            // Out to the tile edges, not just their centres.
            let half = Vec2::splat(TILE / 2.0);
            Rect::from_corners(at((x, y + h - 1)) - half, at((x + w - 1, y)) + half)
        },
        gate_open: false,
    };
    grid.crew_spawn = grid.tile_center(layout.spawn.0, layout.spawn.1);
    // The vault is 2x2 tiles from its top-left one.
    grid.vault = layout
        .vault
        .map(|(col, row)| grid.tile_center(col, row) + Vec2::new(TILE, -TILE) / 2.0);

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

/// Each deck has its own set of station tiles (`sprites/tiles/deck_N`).
const DECK_LOOKS: u8 = 3;
// How often each look turns up; keep these in step with tools/gen_assets.py.
const FLOOR_WEIGHTS: [u32; 6] = [34, 14, 14, 10, 22, 6];
/// The plate with loading-bay paint: a floor's accent.
const FLOOR_ACCENT: usize = 5;
const GRATE_WEIGHTS: [u32; 4] = [35, 30, 10, 25];
const FACE_WEIGHTS: [u32; 9] = [26, 18, 14, 8, 6, 10, 8, 6, 4];
/// Wall faces that light the floor below them: the lamp and the terminal.
const LIT_FACES: [(usize, &str); 2] = [(3, "spill_lamp"), (4, "spill_screen")];
const DECALS: [&str; 10] = ["oil", "scorch", "debris", "scuffs", "cable", "coolant", "arrow", "drain", "rust", "dent"];
/// Decal odds per deck: the engine works are the rusty, scorched one.
const DECAL_WEIGHTS: [[u32; 10]; 3] = [
    [14, 6, 14, 22, 8, 6, 8, 10, 2, 10],
    [16, 12, 14, 18, 8, 4, 4, 8, 14, 10],
    [10, 4, 10, 22, 6, 10, 10, 12, 2, 8],
];
/// Percent of plain floor tiles with a decal on them.
const DECAL_CHANCE: u32 = 11;

/// Spawns the tile sprites for the deck just built. They carry `Level`, so the
/// next deck's `sync_deck` clears them.
fn draw_deck(_: On<DeckBuilt>, mut commands: Commands, assets: Res<AssetServer>, grid: Res<RoomGrid>, built: Res<BuiltDeck>) {
    let deck = built.0.map_or(0, |(_, deck)| deck).min(DECK_LOOKS - 1) as usize;
    let tile = |name: &str| assets.load(format!("sprites/tiles/deck_{deck}/{name}.png"));
    let shared = |name: &str| assets.load(format!("sprites/tiles/{name}.png"));
    // A 2x2 panel's four quarters, in the order `panel` picks them.
    let panels = |kind: &str, looks: usize| -> Vec<[Handle<Image>; 4]> {
        (0..looks).map(|i| std::array::from_fn(|q| tile(&format!("{kind}_{i}_{q}")))).collect()
    };
    let floors = panels("floor", FLOOR_WEIGHTS.len());
    let grates = panels("grate", GRATE_WEIGHTS.len());
    let hull: [Handle<Image>; 4] = std::array::from_fn(|q| tile(&format!("hull_{q}")));
    let faces: Vec<Handle<Image>> = (0..FACE_WEIGHTS.len()).map(|i| tile(&format!("wall_face_{i}"))).collect();
    let caps: Vec<Handle<Image>> = (0..16).map(|i| tile(&format!("wall_cap_{i}"))).collect();
    let lift: Vec<Handle<Image>> = (0..4).map(|i| tile(&format!("lift_{i}"))).collect();
    let (hazard, door, vent) = (tile("hazard"), tile("door"), tile("vent"));
    let door_glow = tile("spill_door");
    let lit_faces = LIT_FACES.map(|(face, glow)| (face, tile(glow)));
    let shades = [shared("shade_n"), shared("shade_w"), shared("shade_e")];
    let lips = ["n", "e", "s", "w"].map(|side| shared(&format!("lip_{side}")));
    let decals = DECALS.map(|name| shared(&format!("decal_{name}")));

    // Off the edge of the deck counts as solid; the gate, open or shut, as floor
    // (its doors are drawn from the `Breach`).
    let open = |col: isize, row: isize| {
        col >= 0 && row >= 0 && (col as usize) < grid.cols() && (row as usize) < grid.rows() && grid.is_floor(col as usize, row as usize)
    };
    // Grating and the escape pad's paint are set into the plate floor around them.
    let inset = |ch: char| match ch {
        ',' | 'G' => 1,
        '=' => 2,
        _ => 0,
    };
    let overlay = |commands: &mut Commands, image: &Handle<Image>, pos: Vec2, z: f32| {
        commands.spawn((Level, Sprite::from_image(image.clone()), Transform::from_translation(pos.extend(z))));
    };
    for row in 0..grid.rows() {
        for col in 0..grid.cols() {
            let (c, r) = (col as isize, row as isize);
            let ch = grid.tiles[row][col];
            let pos = grid.tile_center(col, row);
            let quarter = 2 * (row % 2) + col % 2;
            let image = match ch {
                // Solid rock with no floor next to it is the hull between rooms.
                '#' if !grid.touches_floor(col, row) => hull[quarter].clone(),
                // Floor below: the wall's front face shows, and a lamp in it
                // lights the floor.
                '#' if open(c, r + 1) => {
                    let face = variant(col, row, 3, &FACE_WEIGHTS);
                    if let Some((_, glow)) = lit_faces.iter().find(|(lit, _)| *lit == face) {
                        overlay(&mut commands, glow, pos - Vec2::new(0.0, TILE * 1.5), 0.06);
                    }
                    faces[face].clone()
                }
                '#' => {
                    let mask = open(c, r - 1) as usize
                        | (open(c + 1, r) as usize) << 1
                        | (open(c, r + 1) as usize) << 2
                        | (open(c - 1, r) as usize) << 3;
                    caps[mask].clone()
                }
                'D' => {
                    overlay(&mut commands, &door_glow, pos - Vec2::new(0.0, TILE * 1.5), 0.06);
                    door.clone()
                }
                ',' | 'G' => grates[panel(col, row, 2, &GRATE_WEIGHTS, None)][quarter].clone(),
                '=' => hazard.clone(),
                'v' => vent.clone(),
                // A lift is one 2x2 platform; each tile shows its quarter.
                'L' | 'u' => {
                    let same = |c: isize, r: isize| open(c, r) && grid.tiles[r as usize][c as usize] == ch;
                    lift[2 * same(c, r - 1) as usize + same(c - 1, r) as usize].clone()
                }
                _ => floors[panel(col, row, 1, &FLOOR_WEIGHTS, Some(FLOOR_ACCENT))][quarter].clone(),
            };
            let sprite = match ch {
                // The shaft the crew rode in on is dimmed: it doesn't go anywhere.
                'u' => Sprite { image, color: Color::srgb(0.45, 0.45, 0.5), ..default() },
                _ => Sprite::from_image(image),
            };
            commands.spawn((Level, sprite, Transform::from_translation(pos.extend(0.0))));

            if ch == '.' && hash(col, row, 4) % 100 < DECAL_CHANCE {
                overlay(&mut commands, &decals[variant(col, row, 5, &DECAL_WEIGHTS[deck])], pos, 0.02);
            }
            if inset(ch) > 0 {
                for (lip, (dc, dr)) in lips.iter().zip([(0, -1), (1, 0), (0, 1), (-1, 0)]) {
                    if open(c + dc, r + dr) && inset(grid.tiles[(r + dr) as usize][(c + dc) as usize]) != inset(ch) {
                        overlay(&mut commands, lip, pos, 0.03);
                    }
                }
            }
            // Walls shade the floor beside them.
            if grid.is_floor(col, row) {
                for (shade, (dc, dr)) in shades.iter().zip([(0, -1), (-1, 0), (1, 0)]) {
                    if !open(c + dc, r + dr) {
                        overlay(&mut commands, shade, pos, 0.05);
                    }
                }
            }
        }
    }
}

/// A tile's position, scrambled with Murmur3's finaliser so neighbours don't
/// fall into patterns. `salt` keeps different choices about one tile apart.
fn hash(col: usize, row: usize, salt: u32) -> u32 {
    let mut h = (col as u32) | (row as u32) << 16;
    h ^= salt.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    h
}

/// Picks one of a tile's looks by weight, from its position alone, so every
/// machine draws a deck the same way.
fn variant(col: usize, row: usize, salt: u32, weights: &[u32]) -> usize {
    let mut roll = hash(col, row, salt) % weights.iter().sum::<u32>();
    for (i, &weight) in weights.iter().enumerate() {
        if roll < weight {
            return i;
        }
        roll -= weight;
    }
    0
}

/// Which look the 2x2 panel holding a tile has. Panels near each other mostly
/// match, so an area reads as fitted out alike rather than as patchwork; an
/// `accent` look only turns up here and there, never across a whole area.
fn panel(col: usize, row: usize, salt: u32, weights: &[u32], accent: Option<usize>) -> usize {
    let (pc, pr) = (col / 2, row / 2);
    let area = variant(pc / 3, pr / 3, salt, weights);
    if Some(area) != accent && hash(pc, pr, salt + 1) % 100 < 60 {
        area
    } else {
        variant(pc, pr, salt + 2, weights)
    }
}

/// Places loot, guards, the vault, its gate and its keeper. Host only; everyone
/// else receives them by replication.
fn populate(commands: &mut Commands, grid: &RoomGrid, seed: u32, deck: u8) {
    if let Some(pos) = grid.vault {
        spawn_vault(commands, pos, deck);
    }
    spawn_breach(commands, &grid.gate, deck);
    spawn_keeper(commands, grid.gate.keeper, deck, KeeperKind::pick(seed, deck));
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
