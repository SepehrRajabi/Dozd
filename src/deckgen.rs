//! Procedural deck layouts: rooms joined by corridors, with loot and guards that
//! get richer and nastier the deeper the crew goes. The vault room is sealed off
//! behind a single gate ('G'), the only way in. Deterministic integer maths
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
    /// Top-left tile of the 2x2 vault. Every deck has one.
    pub vault: Option<(usize, usize)>,
    /// The vault room's two gate tiles, side by side in its wall.
    pub gate: [(usize, usize); 2],
    /// The vault room's floor: top-left tile, then width and height in tiles.
    pub vault_room: (usize, usize, usize, usize),
    /// Corridor floor just outside `gate[0]`.
    pub gate_outside: (usize, usize),
    /// Vault room floor just inside `gate[0]`.
    pub gate_inside: (usize, usize),
    /// Where the vault's keeper waits: across the vault from the gate, so the
    /// crew is let in at a distance from it.
    pub keeper: (usize, usize),
}

/// xorshift; `Rng` in main uses floats, which we avoid here.
struct Gen(u32);

impl Gen {
    /// `attempt` > 0 rerolls a deck whose first try came out broken.
    fn new(seed: u32, deck: u8, attempt: u32) -> Self {
        let seed = seed ^ attempt.wrapping_mul(0x85EB_CA6B);
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

    /// The room grown by `by` tiles on every side.
    fn grown(&self, by: usize) -> Rect {
        Rect { x: self.x - by, y: self.y - by, w: self.w + 2 * by, h: self.h + 2 * by }
    }

    fn contains(&self, (x, y): (usize, usize)) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
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

/// Drones thin out and wardens, brutes and heralds take over the deeper you go.
fn guard_table(deck: u8) -> [(char, u32); 5] {
    let deck = deck as u32;
    [
        ('d', 5u32.saturating_sub(deck).max(1)),
        ('s', 3 + deck.min(1)),
        ('h', 1 + 3 * deck / 2),
        ('b', 1 + deck),
        ('g', 1 + deck),
    ]
}

/// Every deck's vault room: an arena to fight the keeper in around the vault.
/// Even sides, so the 2x2 vault sits dead centre.
const VAULT_ROOM: (usize, usize) = (18, 12);

/// Rooms every deck needs: the vault, the arrival, the lift and the pad.
const MIN_ROOMS: usize = 4;

/// Perk pickups: restores and ammo are common, the tactical ones rarer.
const PERK_TABLE: &[(char, u32)] = &[('1', 3), ('2', 3), ('3', 2), ('4', 2), ('5', 2)];

pub fn generate(seed: u32, deck: u8, decks: u8) -> Layout {
    // A corridor between other rooms can clip the sealed vault room; roll again.
    (0..)
        .find_map(|attempt| try_generate(seed, deck, decks, attempt))
        .expect("some attempt builds a deck")
}

/// One go at a deck; `None` if the vault room's seal cut the rest of it apart.
fn try_generate(seed: u32, deck: u8, decks: u8, attempt: u32) -> Option<Layout> {
    let mut rng = Gen::new(seed, deck, attempt);
    let last = deck + 1 >= decks;
    let mut tiles = vec![vec!['#'; COLS]; ROWS];

    // The vault room goes in first, so it always gets its space.
    let rooms = place_rooms(&mut rng, 7 + deck as usize, Some(VAULT_ROOM));
    for room in &rooms[1..] {
        fill(&mut tiles, room.x, room.y, room.w, room.h, '.');
    }
    let goal = 0;
    let room = rooms[goal];
    fill(&mut tiles, room.x, room.y, room.w, room.h, ',');
    // The other rooms join up among themselves, steering clear of the vault
    // room, then one corridor runs from its gate to the nearest of them.
    connect(&mut rng, &mut tiles, &rooms[1..], room.grown(2));
    let (gate, gate_outside, gate_inside, keeper) = open_gate(&mut tiles, &rooms);
    if !all_connected(&tiles, rooms[1].center()) {
        return None;
    }

    // Roles: the crew arrives as far from the vault (room 0) as it gets; the
    // lift down is the farthest walk from there, and the escape pad is somewhere
    // in the far half of what's left. The last deck has no lift.
    let farthest_from = |from: (usize, usize), rooms_left: &mut Vec<usize>| {
        let dist = walk_distances(&tiles, from);
        rooms_left.sort_by_key(|&i| {
            let (c, r) = rooms[i].center();
            Reverse(dist[r][c])
        });
    };
    let mut others: Vec<usize> = (1..rooms.len()).collect();
    farthest_from(rooms[goal].center(), &mut others);
    let start = others[0];
    let mut far: Vec<usize> = (1..rooms.len()).filter(|&i| i != start).collect();
    farthest_from(rooms[start].center(), &mut far);
    let lift = (!last).then(|| far.remove(0));
    let spawn = rooms[start].center();
    let pad = far[rng.range(0, far.len() / 2).min(far.len() - 1)];
    let special = |i: usize| i == start || i == goal || i == pad || Some(i) == lift;

    for (i, room) in rooms.iter().enumerate() {
        if !special(i) {
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
    if let Some(lift) = lift {
        let (lx, ly) = rooms[lift].center();
        fill(&mut tiles, lx, ly, 2, 2, 'L');
    }
    let (gx, gy) = vault_tile(&room);
    let vault = Some((gx, gy));
    fill(&mut tiles, gx, gy, 2, 2, 'V');

    // Nothing but the keeper waits in the vault room: no vents, doors, loot or guards.
    for (i, room) in rooms.iter().enumerate() {
        if i == start || i == goal {
            continue;
        }
        place_door(&mut rng, &mut tiles, room);
        place(&mut rng, &mut tiles, room, 'v', None);

        let loot_count = if Some(i) == lift || i == pad {
            rng.range(0, 1)
        } else {
            rng.range(1, 2 + deck.min(1) as usize)
        };
        let depth = deck as usize;
        let mut guard_count = rng.range(depth, 2 + depth);
        if i == pad {
            // Nobody gets a free extraction.
            guard_count = guard_count.max(1) + depth;
        }
        for _ in 0..loot_count {
            let item = rng.pick(loot_table(deck));
            place(&mut rng, &mut tiles, room, item, None);
        }
        for _ in 0..guard_count {
            let guard = rng.pick(&guard_table(deck));
            place(&mut rng, &mut tiles, room, guard, Some(spawn));
        }
    }

    // A couple of perks per deck, never in the arrival room so they're earned,
    // nor the vault room.
    let perk_count = 2 + deck.min(1) as usize;
    for _ in 0..perk_count {
        let room = loop {
            let i = rng.range(1, rooms.len() - 1);
            if i != start {
                break rooms[i];
            }
        };
        let perk = rng.pick(PERK_TABLE);
        place(&mut rng, &mut tiles, &room, perk, None);
    }

    let Rect { x, y, w, h } = rooms[goal];
    let vault_room = (x, y, w, h);
    Some(Layout { tiles, spawn, vault, gate, vault_room, gate_outside, gate_inside, keeper })
}

/// Top-left tile of the 2x2 vault, dead centre in the vault room.
fn vault_tile(room: &Rect) -> (usize, usize) {
    (room.x + (room.w - 2) / 2, room.y + (room.h - 2) / 2)
}

/// Seals the vault room (`rooms[0]`) behind a two-tile gate in the wall facing
/// the nearest other room, with a corridor from the gate to it. Returns the
/// gate's tiles, the tiles just outside and inside it, and the keeper's tile.
fn open_gate(
    tiles: &mut [Vec<char>],
    rooms: &[Rect],
) -> ([(usize, usize); 2], (usize, usize), (usize, usize), (usize, usize)) {
    let room = rooms[0];
    let (cx, cy) = room.center();
    let (tx, ty) = rooms[1..]
        .iter()
        .map(Rect::center)
        .min_by_key(|&(x, y)| x.abs_diff(cx) + y.abs_diff(cy))
        .expect("rooms besides the vault's");
    // The gate lines up with the vault: rows `gy..=gy+1` or columns `gx..=gx+1`.
    let (gx, gy) = vault_tile(&room);
    let (right, left) = (room.x + room.w, room.x - 1);
    let (bottom, top) = (room.y + room.h, room.y - 1);
    // Corridors are carved from their top-left tile, two wide, so one that has
    // to end next to the gate on its left or top starts a tile further out.
    let (gate, outside, inside, keeper) = if tx.abs_diff(cx) >= ty.abs_diff(cy) {
        if tx > cx {
            carve(tiles, (right + 1, gy), (tx, gy));
            ([(right, gy), (right, gy + 1)], (right + 1, gy), (right - 1, gy), (room.x + 3, gy))
        } else {
            carve(tiles, (tx, gy), (left - 2, gy));
            ([(left, gy), (left, gy + 1)], (left - 1, gy), (left + 1, gy), (right - 4, gy))
        }
    } else if ty > cy {
        carve(tiles, (gx, bottom + 1), (gx, ty));
        ([(gx, bottom), (gx + 1, bottom)], (gx, bottom + 1), (gx, bottom - 1), (gx, room.y + 2))
    } else {
        carve(tiles, (gx, ty), (gx, top - 2));
        ([(gx, top), (gx + 1, top)], (gx, top - 1), (gx, top + 1), (gx, bottom - 3))
    };
    // Then on to the room itself.
    let turn = if gate[0].0 == gate[1].0 { (tx, gy) } else { (gx, ty) };
    carve(tiles, turn, (tx, ty));

    // Re-wall the ring, whatever corridors dug into it, leaving just the gate.
    let ring = room.grown(1);
    for (y, row) in tiles.iter_mut().enumerate().skip(ring.y).take(ring.h) {
        for (x, tile) in row.iter_mut().enumerate().skip(ring.x).take(ring.w) {
            if !room.contains((x, y)) {
                *tile = if gate.contains(&(x, y)) { 'G' } else { '#' };
            }
        }
    }
    (gate, outside, inside, keeper)
}

/// Every open tile can be walked to from `from`.
fn all_connected(tiles: &[Vec<char>], from: (usize, usize)) -> bool {
    let dist = walk_distances(tiles, from);
    tiles.iter().zip(&dist).all(|(row, d)| row.iter().zip(d).all(|(&ch, &d)| !is_open(ch) || d != u32::MAX))
}

fn fill(tiles: &mut [Vec<char>], x: usize, y: usize, w: usize, h: usize, ch: char) {
    for row in &mut tiles[y..y + h] {
        row[x..x + w].fill(ch);
    }
}

/// Scatters `want` rooms; `first`, if given, is the size of the first one.
fn place_rooms(rng: &mut Gen, want: usize, first: Option<(usize, usize)>) -> Vec<Rect> {
    let mut rooms: Vec<Rect> = Vec::new();
    // Past the usual tries, keep going until there's a room for every role.
    for tries in 0.. {
        if rooms.len() >= want || (tries >= 600 && rooms.len() >= MIN_ROOMS) {
            break;
        }
        let (w, h) = match first {
            Some(size) if rooms.is_empty() => size,
            _ => (rng.range(7, 13), rng.range(6, 10)),
        };
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
/// loops so there's more than one way around. Corridors bend whichever way keeps
/// them out of `keep_out`; a loop that can't is left out.
fn connect(rng: &mut Gen, tiles: &mut [Vec<char>], rooms: &[Rect], keep_out: Rect) {
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
    let tree = edges.len();
    for _ in 0..2 {
        let a = rng.range(0, rooms.len() - 1);
        let b = rng.range(0, rooms.len() - 1);
        if a != b && gap(a, b) < 36 && !edges.contains(&(a, b)) && !edges.contains(&(b, a)) {
            edges.push((a, b));
        }
    }

    for (i, (a, b)) in edges.into_iter().enumerate() {
        let (ax, ay) = rooms[a].center();
        let (bx, by) = rooms[b].center();
        let mut corners = [(bx, ay), (ax, by)];
        if rng.chance(50) {
            corners.swap(0, 1);
        }
        let clear = |&corner: &(usize, usize)| {
            !crosses(&keep_out, (ax, ay), corner) && !crosses(&keep_out, corner, (bx, by))
        };
        // The tree has to be dug regardless; if that breaks the deck, it's rerolled.
        let Some(&corner) = corners.iter().find(|c| clear(c)).or((i < tree).then_some(&corners[0])) else {
            continue;
        };
        carve(tiles, (ax, ay), corner);
        carve(tiles, corner, (bx, by));
    }
}

/// Whether `carve(from, to)` would dig into `rect`.
fn crosses(rect: &Rect, from: (usize, usize), to: (usize, usize)) -> bool {
    let (x0, x1) = (from.0.min(to.0), from.0.max(to.0) + 1);
    let (y0, y1) = (from.1.min(to.1), from.1.max(to.1) + 1);
    x0 < rect.x + rect.w && rect.x <= x1 && y0 < rect.y + rect.h && rect.y <= y1
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

/// Whether a tile can be walked on. The vault gate counts as open here; in play
/// `RoomGrid` only lets it be while the gate is.
pub fn is_open(ch: char) -> bool {
    !matches!(ch, '#' | 'D')
}

/// Four-way walking distance from `from` to every tile (`u32::MAX` if unreachable).
fn walk_distances(tiles: &[Vec<char>], from: (usize, usize)) -> Vec<Vec<u32>> {
    walk_distances_by(tiles, from, is_open)
}

/// `walk_distances`, with `open` saying which tiles can be walked.
fn walk_distances_by(tiles: &[Vec<char>], from: (usize, usize), open: impl Fn(char) -> bool) -> Vec<Vec<u32>> {
    let mut dist = vec![vec![u32::MAX; COLS]; ROWS];
    let mut queue = VecDeque::from([from]);
    dist[from.1][from.0] = 0;
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[y][x];
        for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if open(tiles[ny][nx]) && dist[ny][nx] == u32::MAX {
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
                assert_eq!(count('V'), 4, "seed {seed} deck {deck}: vault");
                if let Some((x, y)) = layout.vault {
                    assert_eq!(tiles[y][x], 'V');
                    assert_eq!(tiles[y + 1][x + 1], 'V');
                }
                let perks = ['1', '2', '3', '4', '5'].into_iter().map(count).sum::<usize>();
                assert!((1..=3).contains(&perks), "seed {seed} deck {deck}: {perks} perks");
            }
        }
    }

    #[test]
    fn every_deck_has_a_vault_and_a_pad() {
        for decks in [1, 2, 3] {
            for seed in 1..500 {
                for deck in 0..decks {
                    let layout = generate(seed, deck, decks);
                    let last = deck + 1 == decks;
                    let tiles = &layout.tiles;
                    let count = |c: char| tiles.iter().flatten().filter(|&&t| t == c).count();
                    let at = |c: char| tiles.iter().flatten().position(|&t| t == c).map(|i| (i % COLS, i / COLS));
                    let (vx, vy) = layout.vault.expect("every deck has a vault");
                    assert_eq!(count('V'), 4, "seed {seed} deck {deck}/{decks}: one 2x2 vault");
                    assert_eq!(tiles[vy][vx], 'V');
                    assert_eq!(count('='), 12, "seed {seed} deck {deck}/{decks}: one pad");
                    assert_eq!(count('L'), if last { 0 } else { 4 }, "seed {seed} deck {deck}/{decks}: lift");
                    // The lift and pad stand apart from the vault and the arrival point.
                    let (sx, sy) = layout.spawn;
                    for c in ['L', '='] {
                        if let Some((x, y)) = at(c) {
                            assert!(x.abs_diff(vx) + y.abs_diff(vy) > 3, "seed {seed} deck {deck}: {c} by the vault");
                            assert!(x.abs_diff(sx) + y.abs_diff(sy) > 3, "seed {seed} deck {deck}: {c} at arrival");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_vault_room_is_sealed_behind_its_gate() {
        let shut = |c: char| c != 'G' && is_open(c);
        for seed in 1..300 {
            for deck in 0..3 {
                let layout = generate(seed, deck, 3);
                let tiles = &layout.tiles;
                let at = |(x, y): (usize, usize)| tiles[y][x];
                assert_eq!(tiles.iter().flatten().filter(|&&t| t == 'G').count(), 2, "seed {seed} deck {deck}: gate");
                let [(ax, ay), (bx, by)] = layout.gate;
                assert!(at((ax, ay)) == 'G' && at((bx, by)) == 'G');
                assert_eq!(ax.abs_diff(bx) + ay.abs_diff(by), 1, "seed {seed} deck {deck}: gate tiles side by side");
                for (name, tile) in [("outside", layout.gate_outside), ("inside", layout.gate_inside), ("keeper", layout.keeper)] {
                    assert!(matches!(at(tile), '.' | ','), "seed {seed} deck {deck}: {name} {tile:?} is {:?}", at(tile));
                }

                // With the gate shut, the deck splits in two: the vault room, and
                // everything else, reachable from the arrival point.
                let outside = walk_distances_by(tiles, layout.spawn, shut);
                let inside = walk_distances_by(tiles, layout.gate_inside, shut);
                for (y, row) in tiles.iter().enumerate() {
                    for (x, &ch) in row.iter().enumerate() {
                        if !shut(ch) {
                            continue;
                        }
                        let (out, inn) = (outside[y][x] != u32::MAX, inside[y][x] != u32::MAX);
                        assert!(out != inn, "seed {seed} deck {deck}: ({x},{y}) out {out} in {inn}");
                        // Only the keeper waits in there.
                        assert!(!inn || matches!(ch, ',' | 'V'), "seed {seed} deck {deck}: {ch:?} in the vault room");
                    }
                }
                let (vx, vy) = layout.vault.expect("a vault");
                let (kx, ky) = layout.keeper;
                assert!(inside[vy][vx] != u32::MAX && inside[ky][kx] != u32::MAX);
                let (ox, oy) = layout.gate_outside;
                assert!(outside[oy][ox] != u32::MAX, "seed {seed} deck {deck}: gate cut off");
                // The keeper waits across the vault from the gate.
                assert!(inside[ky][kx] > inside[vy][vx], "seed {seed} deck {deck}: keeper by the gate");
                // The gate is a ring tile around the vault room, so it doesn't touch any
                // doors or vents.
                for (x, y) in layout.gate {
                    for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                        assert!(!matches!(tiles[ny][nx], 'D' | 'v'), "seed {seed} deck {deck}: by the gate");
                    }
                }
            }
        }
    }

    #[test]
    fn same_seed_same_deck() {
        assert_eq!(generate(42, 1, 3).tiles, generate(42, 1, 3).tiles);
        assert_ne!(generate(42, 1, 3).tiles, generate(43, 1, 3).tiles);
        assert_eq!(generate(42, 2, 3).tiles, generate(42, 2, 3).tiles);
        assert_eq!(generate(42, 2, 3).vault, generate(42, 2, 3).vault);
        assert_eq!(generate(42, 2, 3).gate, generate(42, 2, 3).gate);
    }
}
