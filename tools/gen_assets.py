#!/usr/bin/env python3
"""Generates DOZD's pixel-art sprites as PNGs (stdlib only).

Run from the repo root:  python3 tools/gen_assets.py
"""

import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(__file__), "..", "assets", "sprites")
SIZE = 16

OUTLINE = (20, 18, 30, 255)
WHITE = (255, 255, 255, 255)


def write_png(path, pixels):
    h = len(pixels)
    w = len(pixels[0])
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in pixels)

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)


def from_ascii(name, rows, palette, width=SIZE, height=SIZE):
    pal = {".": (0, 0, 0, 0), "k": OUTLINE, "w": WHITE, **palette}
    rows = rows + ["." * width] * (height - len(rows))
    for i, r in enumerate(rows):
        if len(r) != width:
            raise ValueError(f"{name}: row {i} has width {len(r)}: {r!r}")
    return [[pal[ch] for ch in r] for r in rows]


def rgb(r, g, b, a=255):
    return (r, g, b, a)


# --------------------------------------------------------------------------
# Loot
# --------------------------------------------------------------------------

LOOT = {
    "credit_chip": (
        [
            "................",
            "................",
            "................",
            "................",
            "..kkkkkkkkkkkk..",
            "..kwBBBBBBBBBk..",
            "..kBYYBBBBBBbk..",
            "..kBYYBBBBBBbk..",
            "..kBBBBBBBBBbk..",
            "..kBsssBssBBbk..",
            "..kBBBBBBBBBbk..",
            "..kbbbbbbbbbbk..",
            "..kkkkkkkkkkkk..",
        ],
        {
            "B": rgb(70, 140, 255),
            "b": rgb(35, 75, 170),
            "Y": rgb(255, 210, 70),
            "s": rgb(170, 200, 255),
        },
    ),
    "gold_ingot": (
        [
            "................",
            "................",
            "................",
            "..........w.....",
            ".........www....",
            "....kkkkkkwkk...",
            "...kwwYYYYYYk...",
            "..kYYYYYYYYYYk..",
            ".kkkkkkkkkkkkkk.",
            ".kyyyyyyyyyyyyk.",
            ".kyyyyyyyyyyyyk.",
            ".kooooooooooook.",
            ".kkkkkkkkkkkkkk.",
        ],
        {
            "Y": rgb(255, 214, 80),
            "y": rgb(215, 155, 40),
            "o": rgb(150, 95, 30),
        },
    ),
    "data_crystal": (
        [
            "................",
            "......kkkk......",
            ".....kcwcck.....",
            "....kcwcccck....",
            "...kcwcCCccck...",
            "...kccCCCCcck...",
            "..kccCCWWCCcck..",
            "..kcCCWWWWCCck..",
            "..kcCCWWWWCCck..",
            "..kccCCWWCCcck..",
            "...kccCCCCcck...",
            "...kdccCCccdk...",
            "....kddccddk....",
            ".....kddddk.....",
            "......kkkk......",
        ],
        {
            "c": rgb(50, 180, 220),
            "C": rgb(130, 235, 255),
            "W": rgb(230, 255, 255),
            "d": rgb(25, 95, 140),
        },
    ),
    "alien_relic": (
        [
            "................",
            "......kkkk......",
            "....kkppppkk....",
            "...kpwwppppPk...",
            "..kpwwpppppPPk..",
            "..kppppgpppPPk..",
            ".kpppggggppPPPk.",
            ".kppppgpgpPPPPk.",
            ".kpppggggpPPPPk.",
            ".kppppgppPPPPPk.",
            "..kpppppPPPPPk..",
            "..kPPPPPPPPPdk..",
            "...kkPPPPPdkk...",
            "....kmmmmmmk....",
            "...kmmMMMMmmk...",
            "...kkkkkkkkkk...",
        ],
        {
            "p": rgb(170, 90, 230),
            "P": rgb(110, 50, 170),
            "d": rgb(70, 30, 110),
            "g": rgb(120, 255, 140),
            "m": rgb(90, 95, 115),
            "M": rgb(140, 145, 165),
        },
    ),
    "plasma_cell": (
        [
            "................",
            "......kkkk......",
            ".....kmMMmk.....",
            "....kkkkkkkk....",
            "....kmgGGgmk....",
            "....kmgWGgmk....",
            "....kmgGGgmk....",
            "....kmgGWgmk....",
            "....kmgGGgmk....",
            "....kmgWGgmk....",
            "....kmgGGgmk....",
            "....kmgGGgmk....",
            "....kkkkkkkk....",
            ".....kmMMmk.....",
            "......kkkk......",
        ],
        {
            "m": rgb(90, 95, 115),
            "M": rgb(150, 155, 175),
            "g": rgb(40, 170, 70),
            "G": rgb(110, 255, 120),
            "W": rgb(225, 255, 225),
        },
    ),
    "weapon_crate": (
        [
            "................",
            "................",
            ".kkkkkkkkkkkkkk.",
            ".kLLLLLLLLLLLLk.",
            ".kkkkkkkkkkkkkk.",
            ".kAAkAAAAAAkAAk.",
            ".kAAkAAAAAAkAAk.",
            ".kAAkYkYkYkkAAk.",
            ".kAAkkYkYkYkAAk.",
            ".kAAkAAAAAAkAAk.",
            ".kAAkAAAAAAkAAk.",
            ".kkkkkkkkkkkkkk.",
            ".kddddddddddddk.",
            ".kkkkkkkkkkkkkk.",
        ],
        {
            "L": rgb(125, 150, 90),
            "A": rgb(90, 112, 62),
            "d": rgb(55, 70, 40),
            "Y": rgb(240, 200, 50),
        },
    ),
}

# --------------------------------------------------------------------------
# Player: pirate captain (tricorn, eye patch, cyber eye, long teal coat,
# metal gun arm). Faces right; the game flips it to face left.
# --------------------------------------------------------------------------

PLAYER = (
    [
        ".....kkkkkk.....",
        "....kHHwwHHk....",
        "...kHHHwwHHHk...",
        ".kkGGGGGGGGGGkk.",
        "....kFFFFFFk....",
        "....kFkFFCFk....",
        "....kbFFFFbk....",
        "....kbbbbbbk....",
        "..kTTtWWWWtTTk..",
        ".kTkTtWWWWtTkMk.",
        ".kTkTtWGWWtTkMk.",
        ".kfkLLLGGLLLkmk.",
        "...kTTtkktTTk...",
        "...kTtDkkDtTk...",
        "....kDDkkDDk....",
        "....kkkkkkkk....",
    ],
    {
        "H": rgb(50, 42, 68),
        "G": rgb(240, 190, 70),
        "F": rgb(232, 182, 140),
        "C": rgb(255, 60, 70),
        "b": rgb(125, 72, 40),
        "T": rgb(40, 140, 140),
        "t": rgb(25, 88, 95),
        "W": rgb(236, 230, 214),
        "L": rgb(95, 60, 35),
        "M": rgb(160, 170, 185),
        "m": rgb(105, 112, 128),
        "f": rgb(232, 182, 140),
        "D": rgb(58, 46, 52),
    },
)


# --------------------------------------------------------------------------
# Guns: 16x8, barrel pointing right, grip toward the left-centre.
# --------------------------------------------------------------------------

GUN_METAL = {
    "S": rgb(175, 182, 200),
    "s": rgb(105, 110, 128),
    "g": rgb(62, 52, 48),
}

GUNS = {
    "blaster_pistol": (
        [
            "................",
            "...kkkkkkkkk....",
            "..kSSSSSSSSSkk..",
            "..kssAAAssssSSk.",
            "..kkkgggkkkkkk..",
            "....kggk.kk.....",
            "....kggk........",
            "....kkkk........",
        ],
        {**GUN_METAL, "A": rgb(70, 215, 255)},
    ),
    "scatter_cannon": (
        [
            ".kkkkk..........",
            ".kAAAkkkkkkkkkk.",
            "kkAAASSSSSSSSSSk",
            "kssssssssssssssk",
            "kSSSSSSSSSSSSSSk",
            "kkkkggkkkkAAkkkk",
            "...kggk..kAAk...",
            "...kkkk..kkkk...",
        ],
        {**GUN_METAL, "A": rgb(255, 140, 40)},
    ),
    "pulse_rifle": (
        [
            "........kk......",
            "..kkkkkkkkkkk...",
            ".kSSSSSSSSSSSkkk",
            "kkssAAAAsssssSSk",
            "kSkkkkkkkkkkkkk.",
            "kSk.kgk.kAAk....",
            "kkk.kgk.kAAk....",
            "....kkk.kkkk....",
        ],
        {**GUN_METAL, "A": rgb(90, 245, 110)},
    ),
    "rail_lance": (
        [
            "................",
            "......kkkk......",
            "..kkkkkMMkkkkkk.",
            ".kSSSSSSSSSSSSSk",
            "kkssAAAAAAAAAAAk",
            "kSkkkggkkkkkkkk.",
            "kkk.kggk........",
            "....kkkk........",
        ],
        {**GUN_METAL, "A": rgb(190, 100, 255), "M": rgb(120, 220, 255)},
    ),
}


# --------------------------------------------------------------------------
# Enemies
# --------------------------------------------------------------------------

ENEMIES = {
    # Hovering security drone: keeps its distance and fires bursts.
    "sentry_drone": (
        [
            "................",
            "..kkkk....kkkk..",
            "...kk......kk...",
            "....k.kkkk.k....",
            ".....kMMMMk.....",
            "....kMMMMMMk....",
            "...kMMrrrrMMk...",
            "...kMrRRwrrMk...",
            "...kMrRRRRrMk...",
            "...kMMrrrrMMk...",
            "....kmMMMMmk....",
            "....kmmmmmmk....",
            ".....kkkkkk.....",
            "......k..k......",
            ".....kk..kk.....",
        ],
        {
            "M": rgb(150, 155, 172),
            "m": rgb(88, 92, 110),
            "r": rgb(120, 20, 30),
            "R": rgb(255, 60, 60),
        },
    ),
    # Alien stalker: closes in, winds up, then lunges.
    "stalker": (
        [
            "................",
            "................",
            ".....kkkkkk.....",
            "....kCCCCCCk....",
            "...kCCgCCgCCk...",
            "...kCccccccCk...",
            ".k.kCCCCCCCCk.k.",
            "kTk.kCccccCk.kTk",
            "kTTkkCCCCCCkkTTk",
            ".kTTkCccccCkTTk.",
            "..kkkCCCCCCkkk..",
            "....kCkCCkCk....",
            "...kCk.kk.kCk...",
            "..kCk......kCk..",
            "..kk........kk..",
        ],
        {
            "C": rgb(100, 52, 128),
            "c": rgb(62, 30, 84),
            "g": rgb(140, 255, 120),
            "T": rgb(218, 204, 172),
        },
    ),
    # Armoured station warden: slow, tanky, fires spreads.
    "warden": (
        [
            "................",
            "....kkkkkkkk....",
            "...kAAAAAAAAk...",
            "...kAvvvvvvAk...",
            "...kAAAAAAAAk...",
            ".kkkkkkkkkkkkkk.",
            "kAAAkaaaaaakAAAk",
            "kAAAkaYaaYakAAAk",
            "kAAAkaaaaaakAAAk",
            "kkkkkaaaaaakkkkk",
            "kssk.kaaaak.kssk",
            "kssk.kkkkkk.kssk",
            "kkkk.kAkkAk.kkkk",
            ".....kAk.kAk....",
            "....kkkk.kkkk...",
        ],
        {
            "A": rgb(98, 108, 134),
            "a": rgb(64, 71, 92),
            "v": rgb(255, 70, 50),
            "Y": rgb(255, 190, 40),
            "s": rgb(145, 150, 165),
        },
    ),
}


def bolt(w, h, glow):
    """Capsule-shaped energy bolt pointing right with a hot white core."""
    core = tuple(int(c + (255 - c) * 0.7) for c in glow[:3]) + (255,)
    cx, cy = (w - 1) / 2, (h - 1) / 2
    px = [[(0, 0, 0, 0)] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            d = ((x - cx) / (w / 2)) ** 2 + ((y - cy) / (h / 2)) ** 2
            if d <= 0.3:
                px[y][x] = core
            elif d <= 1.0:
                px[y][x] = glow
    return px


PROJECTILES = {
    "bolt_cyan": bolt(7, 3, rgb(70, 215, 255)),
    "pellet_orange": bolt(3, 3, rgb(255, 150, 50)),
    "bolt_green": bolt(6, 3, rgb(90, 245, 110)),
    "rail_purple": bolt(16, 3, rgb(190, 100, 255)),
    "enemy_bolt": bolt(5, 3, rgb(255, 70, 60)),
    "enemy_orb": bolt(4, 4, rgb(255, 130, 40)),
}

# White so the game can tint them per gun.
FX = {
    "muzzle_flash": (
        [
            "...w...",
            ".w.w.w.",
            "..www..",
            "wwwwwww",
            "..www..",
            ".w.w.w.",
            "...w...",
        ],
        7,
    ),
    "spark": (
        [
            "w...w",
            ".w.w.",
            "..w..",
            ".w.w.",
            "w...w",
        ],
        5,
    ),
}

CROSSHAIR = [
    "....kwk....",
    "....kwk....",
    "....kkk....",
    "...........",
    "kkk.....kkk",
    "wwk..w..kww",
    "kkk.....kkk",
    "...........",
    "....kkk....",
    "....kwk....",
    "....kwk....",
]


# --------------------------------------------------------------------------
# Title logo: chunky pixel letters, gold gradient, dark outline, drop shadow.
# --------------------------------------------------------------------------

LOGO_GLYPHS = {
    "D": [
        "######.",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "######.",
    ],
    "O": [
        ".#####.",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        "##...##",
        ".#####.",
    ],
    "Z": [
        "#######",
        ".....##",
        "....##.",
        "....##.",
        "...##..",
        "..##...",
        ".##....",
        "##.....",
        "#######",
    ],
}


def logo(word="DOZD", gap=2):
    glyph_w, glyph_h = 7, 9
    text_w = len(word) * glyph_w + (len(word) - 1) * gap
    # Room for a 1px outline on every side plus a 2px drop shadow.
    w, h = text_w + 4, glyph_h + 5
    ox, oy = 1, 1
    solid = set()
    for i, ch in enumerate(word):
        for y, row in enumerate(LOGO_GLYPHS[ch]):
            for x, c in enumerate(row):
                if c == "#":
                    solid.add((ox + i * (glyph_w + gap) + x, oy + y))

    px = [[(0, 0, 0, 0)] * w for _ in range(h)]

    def put(x, y, c):
        if 0 <= x < w and 0 <= y < h:
            px[y][x] = c

    for x, y in solid:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                put(x + dx + 1, y + dy + 2, rgb(40, 12, 36))
    for x, y in solid:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                put(x + dx, y + dy, OUTLINE)
    for x, y in solid:
        t = (y - oy) / (glyph_h - 1)
        fill = rgb(int(255 - 25 * t), int(224 - 110 * t), int(100 - 60 * t))
        if y == oy or (x, y - 1) not in solid:
            fill = rgb(255, 246, 200)
        put(x, y, fill)
    return px


# --------------------------------------------------------------------------
# Tiles (procedural)
# --------------------------------------------------------------------------


def noise(x, y, seed):
    n = (x * 374761393 + y * 668265263 + seed * 144269504) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n >> 16) & 0xFF) / 255.0


def shade(c, amount):
    return tuple(max(0, min(255, int(v + amount))) for v in c[:3]) + (255,)


def blank(color):
    return [[color for _ in range(SIZE)] for _ in range(SIZE)]


def floor_tile():
    base = rgb(58, 64, 82)
    px = [[shade(base, (noise(x, y, 1) - 0.5) * 8) for x in range(SIZE)] for y in range(SIZE)]
    for i in range(SIZE):
        px[0][i] = rgb(76, 84, 106)
        px[i][0] = rgb(76, 84, 106)
        px[SIZE - 1][i] = rgb(36, 40, 54)
        px[i][SIZE - 1] = rgb(36, 40, 54)
    for rx, ry in [(3, 3), (12, 3), (3, 12), (12, 12)]:
        px[ry][rx] = rgb(110, 120, 145)
        px[ry + 1][rx + 1] = rgb(36, 40, 54)
    return px


def grate_tile():
    px = floor_tile()
    for y in range(2, SIZE - 2):
        for x in range(2, SIZE - 2):
            px[y][x] = rgb(22, 24, 34) if y % 2 == 0 else rgb(48, 54, 70)
    return px


def wall_tile():
    px = blank(rgb(32, 36, 50))
    for y in range(SIZE):
        for x in range(SIZE):
            px[y][x] = shade(px[y][x], (noise(x, y, 7) - 0.5) * 6)
    for x in range(SIZE):
        px[0][x] = rgb(98, 108, 136)
        px[1][x] = rgb(82, 92, 118)
        px[2][x] = rgb(64, 72, 94)
        px[SIZE - 2][x] = rgb(22, 24, 34)
        px[SIZE - 1][x] = rgb(14, 15, 22)
    for y in range(3, SIZE - 2):
        px[y][7] = rgb(20, 22, 32)
        px[y][8] = rgb(46, 52, 70)
    return px


def hazard_tile():
    px = blank(rgb(0, 0, 0))
    for y in range(SIZE):
        for x in range(SIZE):
            stripe = ((x + y) // 4) % 2 == 0
            px[y][x] = rgb(232, 190, 40) if stripe else rgb(30, 30, 34)
    for i in range(SIZE):
        px[0][i] = px[i][0] = rgb(60, 60, 64)
        px[SIZE - 1][i] = px[i][SIZE - 1] = rgb(20, 20, 24)
    return px


def door_tile():
    px = wall_tile()
    for y in range(3, SIZE - 2):
        for x in range(2, SIZE - 2):
            px[y][x] = rgb(70, 76, 92)
        px[y][7] = rgb(18, 20, 28)
        px[y][8] = rgb(18, 20, 28)
    for y, x in [(6, 4), (7, 5), (8, 4), (6, 11), (7, 10), (8, 11)]:
        px[y][x] = rgb(240, 150, 40)
    px[11][4] = px[11][11] = rgb(80, 255, 120)
    return px


def vent_tile():
    """Floor vent that reinforcements crawl out of."""
    px = floor_tile()
    for y in range(3, SIZE - 3):
        for x in range(3, SIZE - 3):
            edge = x in (3, SIZE - 4) or y in (3, SIZE - 4)
            if edge:
                px[y][x] = rgb(20, 20, 28)
            elif y % 2 == 0:
                px[y][x] = rgb(12, 10, 16)
            else:
                glow = 1.0 - abs(x - 7.5) / 5.0
                px[y][x] = rgb(int(90 + 120 * glow), int(25 + 30 * glow), 30)
    return px


def lift_tile():
    """Cargo lift platform, laid 2x2: ride it down to the next deck."""
    px = blank(rgb(44, 50, 66))
    for y in range(SIZE):
        for x in range(SIZE):
            px[y][x] = shade(px[y][x], (noise(x, y, 11) - 0.5) * 6)
    for i in range(SIZE):
        px[0][i] = px[i][0] = rgb(18, 20, 28)
        px[SIZE - 1][i] = px[i][SIZE - 1] = rgb(18, 20, 28)
        px[1][i] = px[i][1] = rgb(96, 106, 130)
    # Lights along the rim.
    for i in (4, 11):
        px[1][i] = px[i][1] = rgb(80, 230, 255)
    # Two down-pointing chevrons.
    for top, color in ((4, rgb(80, 230, 255)), (8, rgb(40, 140, 170))):
        for k in range(4):
            for x in (4 + k, 11 - k):
                px[top + k][x] = color
                if top + k + 1 < SIZE - 1:
                    px[top + k + 1][x] = shade(color, -60)
    return px


def shadow():
    px = blank((0, 0, 0, 0))
    cx, cy, rx, ry = 7.5, 13.5, 6.0, 2.0
    for y in range(SIZE):
        for x in range(SIZE):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                px[y][x] = (0, 0, 0, int(110 * (1.0 - d * 0.6)))
    return px


def contact_sheet(sprites, scale=6, pad=4):
    cell_w = max(len(px[0]) for px in sprites) * scale
    cell_h = max(len(px) for px in sprites) * scale
    w = len(sprites) * (cell_w + pad) + pad
    h = cell_h + pad * 2
    sheet = [[rgb(12, 12, 20) for _ in range(w)] for _ in range(h)]
    for i, px in enumerate(sprites):
        sw, sh = len(px[0]) * scale, len(px) * scale
        ox = pad + i * (cell_w + pad) + (cell_w - sw) // 2
        oy = pad + (cell_h - sh) // 2
        for y in range(sh):
            for x in range(sw):
                c = px[y // scale][x // scale]
                if c[3] == 0:
                    continue
                a = c[3] / 255.0
                bg = sheet[oy + y][ox + x]
                sheet[oy + y][ox + x] = tuple(
                    int(c[j] * a + bg[j] * (1 - a)) for j in range(3)
                ) + (255,)
    return sheet


def main():
    loot = {}
    for name, (rows, pal) in LOOT.items():
        px = from_ascii(name, rows, pal)
        loot[name] = px
        write_png(os.path.join(ROOT, "loot", f"{name}.png"), px)

    player = from_ascii("player", *PLAYER)
    write_png(os.path.join(ROOT, "player.png"), player)

    tiles = {
        "floor": floor_tile(),
        "grate": grate_tile(),
        "wall": wall_tile(),
        "hazard": hazard_tile(),
        "door": door_tile(),
        "vent": vent_tile(),
        "lift": lift_tile(),
    }
    for name, px in tiles.items():
        write_png(os.path.join(ROOT, "tiles", f"{name}.png"), px)

    write_png(os.path.join(ROOT, "shadow.png"), shadow())

    guns = {}
    for name, (rows, pal) in GUNS.items():
        px = from_ascii(name, rows, pal, height=8)
        guns[name] = px
        write_png(os.path.join(ROOT, "guns", f"{name}.png"), px)

    for name, px in PROJECTILES.items():
        write_png(os.path.join(ROOT, "fx", f"{name}.png"), px)
    fx = {}
    for name, (rows, size) in FX.items():
        px = from_ascii(name, rows, {}, width=size, height=size)
        fx[name] = px
        write_png(os.path.join(ROOT, "fx", f"{name}.png"), px)

    enemies = {}
    for name, (rows, pal) in ENEMIES.items():
        px = from_ascii(name, rows, pal)
        enemies[name] = px
        write_png(os.path.join(ROOT, "enemies", f"{name}.png"), px)

    title = logo()
    write_png(os.path.join(ROOT, "ui", "logo.png"), title)

    crosshair = from_ascii("crosshair", CROSSHAIR, {}, width=11, height=11)
    write_png(os.path.join(ROOT, "ui", "crosshair.png"), crosshair)

    preview = os.path.join(os.path.dirname(__file__), "..", "docs", "preview")
    write_png(os.path.join(preview, "loot.png"), contact_sheet(list(loot.values())))
    write_png(
        os.path.join(preview, "player_tiles.png"),
        contact_sheet([player] + list(tiles.values())),
    )
    write_png(os.path.join(preview, "logo.png"), contact_sheet([title], scale=8))
    write_png(os.path.join(preview, "enemies.png"), contact_sheet(list(enemies.values())))
    write_png(os.path.join(preview, "guns.png"), contact_sheet(list(guns.values())))
    write_png(
        os.path.join(preview, "fx.png"),
        contact_sheet(list(PROJECTILES.values()) + list(fx.values()) + [crosshair]),
    )
    print("assets written to", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()
