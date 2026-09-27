//! Procedural deck layouts: rooms joined by corridors, with loot and guards that
//! get richer and nastier the deeper the crew goes. Deterministic integer maths
//! only, so every machine builds the same deck from `(seed, deck)`.

use std::cmp::Reverse;
use std::collections::VecDeque;

pub const COLS: usize = 60;
pub const ROWS: usize = 42;
/// Guards never start closer than this many tiles to the crew's arrival point.
const GUARD_CLEARANCE: usize = 12;

/// A generated deck in the room legend (see `room.rs`).
pub struct Layout {
    pub tiles: Vec<Vec<char>>,
    /// Tile where the crew arrives.
    pub spawn: (usize, usize),
}

/// xorshift; `Rng` in main uses floats, which we avoid here.
struct Gen(u32);

impl Gen {
    fn new(seed: u32, deck: u8) -> Self {
        let mut rng = Self((seed ^ (deck as u32 + 1).wrapping_mul(0x9E37_79B9)).max(1));
        for _ in 0..8 {
            rng.next();
        }
        rng
    }

    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// Uniform in `lo..=hi`.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.next() as usize % (hi - lo + 1)
    }

    fn chance(&mut self, percent: u32) -> bool {
        self.next() % 100 < percent
    }

    fn pick(&mut self, weighted: &[(char, u32)]) -> char {
        let total: u32 = weighted.iter().map(|&(_, w)| w).sum();
        let mut roll = self.next() % total;
        for &(item, weight) in weighted {
            if roll < weight {
                return item;
            }
            roll -= weight;
        }
        weighted[0].0
    }
}

/// A room's floor, not counting its walls.
#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn center(&self) -> (usize, usize) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    fn near(&self, other: &Rect, margin: usize) -> bool {
        self.x < other.x + other.w + margin
            && other.x < self.x + self.w + margin
            && self.y < other.y + other.h + margin
            && other.y < self.y + self.h + margin
    }
}

fn loot_table(deck: u8) -> &'static [(char, u32)] {
    match deck {
        0 => &[('c', 4), ('p', 3), ('i', 2), ('x', 1)],
        1 => &[('p', 2), ('i', 3), ('x', 3), ('w', 2)],
        _ => &[('i', 1), ('x', 3), ('w', 3), ('r', 1)],
    }
}

fn guard_table(deck: u8) -> &'static [(char, u32)] {
    match deck {
        0 => &[('d', 5), ('s', 3), ('h', 1)],
        1 => &[('d', 4), ('s', 4), ('h', 2)],
        _ => &[('d', 3), ('s', 4), ('h', 3)],
    }
}

pub fn generate(seed: u32, deck: u8, decks: u8) -> Layout {
    let mut rng = Gen::new(seed, deck);
    let last = deck + 1 >= decks;
    let mut tiles = vec![vec!['#'; COLS]; ROWS];

    let rooms = place_rooms(&mut rng, 7 + deck as usize);
    for room in &rooms {
        fill(&mut tiles, room.x, room.y, room.w, room.h, '.');
    }
    connect(&mut rng, &mut tiles, &rooms);

    // Roles: arrive in one room; the lift (or vault) is the farthest walk away,
    // and the escape pad is somewhere in the far half.
    let start = rng.range(0, rooms.len() - 1);
    let spawn = rooms[start].center();
    let dist = walk_distances(&tiles, spawn);
    let mut far: Vec<usize> = (0..rooms.len()).filter(|&i| i != start).collect();
    far.sort_by_key(|&i| {
        let (c, r) = rooms[i].center();
        Reverse(dist[r][c])
    });
    let goal = far[0];
    let pad = far[1 + rng.range(0, (far.len() - 1) / 2).min(far.len() - 2)];

    for (i, room) in rooms.iter().enumerate() {
        if i != start && i != goal && i != pad {
            decorate(&mut rng, &mut tiles, room);
        }
    }

    let (sx, sy) = spawn;
    if deck > 0 {
        // The lift shaft the crew rode down.
        fill(&mut tiles, sx, sy, 2, 2, 'u');
    }
    let (px, py) = rooms[pad].center();
    fill(&mut tiles, px - 2, py - 1, 4, 3, '=');
    let (gx, gy) = rooms[goal].center();
    if last {
        let vault = rooms[goal];
        fill(&mut tiles, vault.x, vault.y, vault.w, vault.h, ',');
    } else {
        fill(&mut tiles, gx, gy, 2, 2, 'L');
    }

    for (i, room) in rooms.iter().enumerate() {
        if i == start {
            continue;
        }
        place_door(&mut rng, &mut tiles, room);
        place(&mut rng, &mut tiles, room, 'v', None);

        let (loot, guards): (Vec<char>, Vec<char>) = if last && i == goal {
            (vec!['r', 'r', 'w'], vec!['h', 'h', 's'])
        } else {
            let loot_count = if i == goal || i == pad {
                rng.range(0, 1)
            } else {
                rng.range(1, 2 + deck.min(1) as usize)
            };
            let mut guard_count = match deck {
                0 => rng.range(0, 2),
                1 => rng.range(1, 2),
                _ => rng.range(1, 3),
            };
            if i == pad {
                // Nobody gets a free extraction.
                guard_count = guard_count.max(1) + deck.min(1) as usize;
            }
            (
                (0..loot_count).map(|_| rng.pick(loot_table(deck))).collect(),
                (0..guard_count).map(|_| rng.pick(guard_table(deck))).collect(),
            )
        };
        for item in loot {
            place(&mut rng, &mut tiles, room, item, None);
        }
        for guard in guards {
            place(&mut rng, &mut tiles, room, guard, Some(spawn));
        }
    }

    Layout { tiles, spawn }
}

fn fill(tiles: &mut [Vec<char>], x: usize, y: usize, w: usize, h: usize, ch: char) {
    for row in &mut tiles[y..y + h] {
        row[x..x + w].fill(ch);
    }
}

fn place_rooms(rng: &mut Gen, want: usize) -> Vec<Rect> {
    let mut rooms: Vec<Rect> = Vec::new();
    for _ in 0..600 {
        if rooms.len() >= want {
            break;
        }
        let w = rng.range(7, 13);
        let h = rng.range(6, 10);
        let room = Rect {
            x: rng.range(2, COLS - w - 3),
            y: rng.range(2, ROWS - h - 3),
            w,
            h,
        };
        // Thick walls between rooms, so corridors read as corridors.
        if rooms.iter().all(|other| !room.near(other, 3)) {
            rooms.push(room);
        }
    }
    rooms
}

/// Joins every room (minimum spanning tree over room centres), plus a couple of
/// loops so there's more than one way around.
fn connect(rng: &mut Gen, tiles: &mut [Vec<char>], rooms: &[Rect]) {
    let gap = |a: usize, b: usize| {
        let (ax, ay) = rooms[a].center();
        let (bx, by) = rooms[b].center();
        ax.abs_diff(bx) + ay.abs_diff(by)
    };

    let mut joined = vec![false; rooms.len()];
    joined[0] = true;
    let mut edges = Vec::new();
    for _ in 1..rooms.len() {
        let (a, b) = (0..rooms.len())
            .filter(|&a| joined[a])
            .flat_map(|a| (0..rooms.len()).filter(|&b| !joined[b]).map(move |b| (a, b)))
            .min_by_key(|&(a, b)| gap(a, b))
            .expect("rooms left to join");
        joined[b] = true;
        edges.push((a, b));
    }
    for _ in 0..2 {
        let a = rng.range(0, rooms.len() - 1);
        let b = rng.range(0, rooms.len() - 1);
        if a != b && gap(a, b) < 36 && !edges.contains(&(a, b)) && !edges.contains(&(b, a)) {
            edges.push((a, b));
        }
    }

    for (a, b) in edges {
        let (ax, ay) = rooms[a].center();
        let (bx, by) = rooms[b].center();
        let corner = if rng.chance(50) { (bx, ay) } else { (ax, by) };
        carve(tiles, (ax, ay), corner);
        carve(tiles, corner, (bx, by));
    }
}

/// A straight, two-tile-wide corridor; only digs through solid rock.
fn carve(tiles: &mut [Vec<char>], from: (usize, usize), to: (usize, usize)) {
    let (x0, x1) = (from.0.min(to.0), from.0.max(to.0));
    let (y0, y1) = (from.1.min(to.1), from.1.max(to.1));
    for row in &mut tiles[y0..=y1 + 1] {
        for tile in &mut row[x0..=x1 + 1] {
            if *tile == '#' {
                *tile = ',';
            }
        }
    }
}

/// Grates and pillars. Pillars sit two tiles in from the walls, so they never cut
/// a room in half.
fn decorate(rng: &mut Gen, tiles: &mut [Vec<char>], room: &Rect) {
    match rng.range(0, 3) {
        0 if room.w >= 10 && room.h >= 9 => {
            for (x, y) in [
                (room.x + 2, room.y + 2),
                (room.x + room.w - 4, room.y + 2),
                (room.x + 2, room.y + room.h - 4),
                (room.x + room.w - 4, room.y + room.h - 4),
            ] {
                fill(tiles, x, y, 2, 2, '#');
            }
        }
        1 => {
            let (_, cy) = room.center();
            fill(tiles, room.x + 2, cy - 1, room.w - 4, 3, ',');
        }
        2 => {
            fill(tiles, room.x, room.y, room.w, room.h, ',');
            fill(tiles, room.x + 1, room.y + 1, room.w - 2, room.h - 2, '.');
        }
        _ => {}
    }
}

/// A sealed bulkhead in the room's top wall that reinforcements can burst from.
fn place_door(rng: &mut Gen, tiles: &mut [Vec<char>], room: &Rect) {
    if !rng.chance(40) {
        return;
    }
    let x = room.x + rng.range(1, room.w - 2);
    let y = room.y - 1;
    let solid = |x: usize, y: usize| tiles[y][x] == '#';
    if solid(x, y) && solid(x - 1, y) && solid(x + 1, y) && solid(x, y - 1) && tiles[y + 1][x] == '.' {
        tiles[y][x] = 'D';
    }
}

/// Puts `item` on a random plain floor tile inside `room`, away from the walls.
/// Guards also keep their distance from `avoid`.
fn place(rng: &mut Gen, tiles: &mut [Vec<char>], room: &Rect, item: char, avoid: Option<(usize, usize)>) {
    for _ in 0..40 {
        let x = rng.range(room.x + 1, room.x + room.w - 2);
        let y = rng.range(room.y + 1, room.y + room.h - 2);
        if !matches!(tiles[y][x], '.' | ',') {
            continue;
        }
        if let Some((ax, ay)) = avoid {
            let (dx, dy) = (x.abs_diff(ax), y.abs_diff(ay));
            if dx * dx + dy * dy < GUARD_CLEARANCE * GUARD_CLEARANCE {
                continue;
            }
        }
        tiles[y][x] = item;
        return;
    }
}

pub fn is_open(ch: char) -> bool {
    !matches!(ch, '#' | 'D')
}

/// Four-way walking distance from `from` to every tile (`u32::MAX` if unreachable).
fn walk_distances(tiles: &[Vec<char>], from: (usize, usize)) -> Vec<Vec<u32>> {
    let mut dist = vec![vec![u32::MAX; COLS]; ROWS];
    let mut queue = VecDeque::from([from]);
    dist[from.1][from.0] = 0;
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[y][x];
        for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if is_open(tiles[ny][nx]) && dist[ny][nx] == u32::MAX {
                dist[ny][nx] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decks_are_connected_and_complete() {
        for seed in 1..300 {
            for deck in 0..3 {
                let layout = generate(seed, deck, 3);
                let tiles = &layout.tiles;
                assert!(tiles.iter().all(|r| r.len() == COLS) && tiles.len() == ROWS);
                // The border stays solid so nothing walks off the map.
                assert!(tiles[0].iter().chain(&tiles[ROWS - 1]).all(|&c| c == '#'));
                assert!(tiles.iter().all(|r| r[0] == '#' && r[COLS - 1] == '#'));

                let dist = walk_distances(tiles, layout.spawn);
                for (y, row) in tiles.iter().enumerate() {
                    for (x, &ch) in row.iter().enumerate() {
                        if is_open(ch) {
                            assert!(dist[y][x] != u32::MAX, "seed {seed} deck {deck}: ({x},{y}) unreachable");
                        }
                    }
                }
                let count = |c: char| tiles.iter().flatten().filter(|&&t| t == c).count();
                assert_eq!(count('='), 12, "seed {seed} deck {deck}: pad");
                assert_eq!(count('L'), if deck < 2 { 4 } else { 0 }, "seed {seed} deck {deck}: lift");
                assert!(count('r') >= if deck == 2 { 2 } else { 0 });
            }
        }
    }

    #[test]
    fn same_seed_same_deck() {
        assert_eq!(generate(42, 1, 3).tiles, generate(42, 1, 3).tiles);
        assert_ne!(generate(42, 1, 3).tiles, generate(43, 1, 3).tiles);
    }
}
