#!/usr/bin/env python3
"""Generates DOZD's pixel-art sprites as PNGs (stdlib only).

Run from the repo root:  python3 tools/gen_assets.py
"""

import math
import os
import struct
import zlib
from types import SimpleNamespace

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


# The Engineer: the same pirate in a hi-vis coat and a blue bandana.
ENGINEER_PALETTE = {
    **PLAYER[1],
    "G": rgb(80, 160, 255),
    "T": rgb(220, 130, 45),
    "t": rgb(150, 82, 30),
}

# The Bulwark: steel plating and a cyan bandana.
BULWARK_PALETTE = {
    **PLAYER[1],
    "G": rgb(90, 210, 255),
    "T": rgb(100, 115, 145),
    "t": rgb(62, 72, 98),
}

# The Hacker: a dark coat and a green bandana.
HACKER_PALETTE = {
    **PLAYER[1],
    "G": rgb(90, 240, 110),
    "T": rgb(78, 56, 105),
    "t": rgb(48, 34, 68),
}

# The Doctor: a white coat and a red bandana.
DOCTOR_PALETTE = {
    **PLAYER[1],
    "G": rgb(235, 70, 85),
    "T": rgb(218, 226, 232),
    "t": rgb(140, 152, 170),
}

# The Engineer's sentry: a tripod base, and a gun head that turns to aim.
SENTRY_METAL = {
    "O": rgb(220, 130, 45),
    "o": rgb(150, 82, 30),
    "Y": rgb(255, 220, 90),
    "M": rgb(150, 155, 172),
    "m": rgb(88, 92, 110),
    "S": rgb(175, 182, 200),
}

SENTRY = [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    ".....kkkkkk.....",
    "....kOOOOOOk....",
    "....kOYooYOk....",
    "....kkkkkkkk....",
    ".....kMmmMk.....",
    "....kMkmmkMk....",
    "...kMk.kk.kMk...",
    "..kMk..kk..kMk..",
    "..kk...kk...kk..",
    "................",
]

SENTRY_GUN = [
    "............",
    ".kkkkkk.....",
    "kOOOOOOkkkkk",
    "kooYoooSSSSk",
    "kkkkkkkkkkkk",
    "............",
]

# The Doctor's leech mine: a squat steel puck, its pink cross dim until armed,
# and the glow the game blinks over the cross once it is.
MINE_METAL = {
    "S": rgb(132, 138, 158),
    "M": rgb(82, 86, 104),
    "m": rgb(46, 46, 62),
    "d": rgb(34, 34, 46),
    "P": rgb(214, 84, 112),
    "L": rgb(255, 176, 192),
    "r": rgb(150, 56, 82),
}

MINE = [
    "............",
    "...kkkkkk...",
    ".kkSSSSSSkk.",
    "kSMmmLPmmMSk",
    "kMmLPPPPPmMk",
    "kMmPPPPPPmMk",
    "kMMmmPPmmMMk",
    "kdMMMMMMMMdk",
    "kddddddddddk",
    ".kdrddddrdk.",
    "..kkkkkkkk..",
    "............",
]

# White so the game can tint it, soft at the edges.
MINE_LIGHT = [
    "..ww..",
    ".wWWw.",
    "wWWWWw",
    "wWWWWw",
    ".wWWw.",
    "..ww..",
]
MINE_GLOW = {"w": rgb(255, 255, 255, 110), "W": WHITE}


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
    "arc_coil": (
        [
            "...k.k.k........",
            "..kAkAkAkkkkk...",
            ".kSAkAkAkSSSSkk.",
            "kkssAsAsAsssSSwk",
            "kSkkkkkkkkkkkkk.",
            "kSk.kgk.........",
            "kkk.kgk.........",
            "....kkk.........",
        ],
        {**GUN_METAL, "A": rgb(255, 235, 90)},
    ),
    "grenade_launcher": (
        [
            "....kkkkk.......",
            "...kSSSSSkkkkkkk",
            "kkkkSAAASSSSSSSk",
            "kssksAAAsssssssk",
            "kSkksAAAskkkkkkk",
            "kSk.kSSSk.......",
            "kkk.kkgk........",
            ".....kkk........",
        ],
        {**GUN_METAL, "A": rgb(150, 190, 70)},
    ),
    "nova_cannon": (
        [
            "..kkkkkkkk......",
            ".kSSSSSSSSkkkkk.",
            "kkSAAAAAASSSSSSk",
            "ksSAwwwwAsssssAk",
            "ksSAAAAAASSSSSSk",
            "kSkkkkkkkkkkkkk.",
            "kSk.kggk........",
            "kkk.kkkk........",
        ],
        {**GUN_METAL, "A": rgb(110, 255, 90), "w": rgb(225, 255, 210)},
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
    # Hulking station bruiser: slow, tough, punches whoever gets close.
    "brute": (
        [
            "................",
            "......kkkk......",
            ".....kHHHHk.....",
            ".....kHeeHk.....",
            "..kkkkHHHHkkkk..",
            ".kBBBBkkkkBBBBk.",
            "kBBBbBBBBBBbBBBk",
            "kBBbbBBttBBbbBBk",
            "kBBk.kBBBBk.kBBk",
            "kFFk.kbBBbk.kFFk",
            "kFFFkkbbbbkkFFFk",
            ".kkk.kBkkBk.kkk.",
            ".....kBk.kBk....",
            "....kkBk.kBkk...",
            "....kkkk.kkkk...",
        ],
        {
            "B": rgb(150, 92, 74),
            "b": rgb(104, 60, 50),
            "H": rgb(92, 98, 116),
            "e": rgb(255, 120, 40),
            "t": rgb(210, 190, 60),
            "F": rgb(170, 175, 190),
        },
    ),
    # Station herald: an officer who rallies the guards, or works itself into a frenzy.
    "herald": (
        [
            "................",
            "......kkkk......",
            ".....kGGGGk.....",
            "....kRRRRRRk....",
            "....kkvvvvkk....",
            ".....kMMMMk.....",
            "...kkkGGGGkkk...",
            "..kRRkRRRRkRRk..",
            "..kRrkRGGRkrRk..",
            "..kRrkRRRRkrRk..",
            "..kMk.kRRk.kMk..",
            "...k.krrrrk.k...",
            ".....kRkkRk.....",
            ".....kRk.kRk....",
            "....kkkk.kkkk...",
        ],
        {
            "R": rgb(150, 36, 70),
            "r": rgb(96, 22, 48),
            "G": rgb(240, 190, 70),
            "v": rgb(255, 120, 60),
            "M": rgb(160, 165, 180),
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
    "nova_orb": bolt(9, 9, rgb(110, 255, 90)),
    "grenade": [
        [(0, 0, 0, 0), OUTLINE, OUTLINE, OUTLINE, (0, 0, 0, 0)],
        [OUTLINE, rgb(120, 150, 60), rgb(150, 190, 70), rgb(120, 150, 60), OUTLINE],
        [OUTLINE, rgb(90, 110, 45), rgb(255, 90, 60), rgb(90, 110, 45), OUTLINE],
        [OUTLINE, rgb(70, 85, 35), rgb(90, 110, 45), rgb(70, 85, 35), OUTLINE],
        [(0, 0, 0, 0), OUTLINE, OUTLINE, OUTLINE, (0, 0, 0, 0)],
    ],
    "explosion": None,
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


# --------------------------------------------------------------------------
# Enemy drops: smaller than loot so they read as scraps.
# --------------------------------------------------------------------------

DROPS = {
    "credits": (
        [
            "................",
            "................",
            "................",
            "................",
            "......kkkkk.....",
            ".....kYYwYYk....",
            ".....kyyyyyk....",
            ".....kkkkkkk....",
            ".....kYYYYYk....",
            ".....kyyyyyk....",
            ".....kkkkkkk....",
            ".....kYYYYYk....",
            ".....kyyyyyk....",
            ".....koooook....",
            "......kkkkk.....",
        ],
        {
            "Y": rgb(255, 214, 80),
            "y": rgb(215, 155, 40),
            "o": rgb(150, 95, 30),
        },
    ),
    # Ammo, one per limited-ammo gun, in that gun's colour and with its own
    # silhouette. The Blaster Pistol never runs dry, so it has none.
    # Scatter Cannon: three red shotgun shells on brass bases.
    "ammo_scatter_cannon": (
        [
            "................",
            "................",
            "................",
            "................",
            ".......kk.......",
            "....kkkrrkkk....",
            "...krrkLRkrrk...",
            "...kLRkLRkLRk...",
            "...kLRkLRkLRk...",
            "...kLRkLRkLRk...",
            "...kLRkLRkLRk...",
            "...kBbkBbkBbk...",
            "...kbbkbbkbbk...",
            "...kkkkkkkkkk...",
        ],
        {
            "L": rgb(255, 150, 80),
            "R": rgb(220, 70, 35),
            "r": rgb(150, 40, 25),
            "B": rgb(245, 205, 90),
            "b": rgb(175, 125, 40),
        },
    ),
    # Pulse Rifle: a curved box magazine with a green glow strip.
    "ammo_pulse_rifle": (
        [
            "................",
            "................",
            "................",
            "................",
            ".....kk.kk......",
            "....kGwkGwk.....",
            "....kkkkkkk.....",
            "....kSAAsmk.....",
            "....kSAAsmk.....",
            "....kSAAsmk.....",
            "....kSAAsmk.....",
            "....kSAAsmk.....",
            "....kmmmmmk.....",
            "....kkkkkkk.....",
        ],
        {
            "S": rgb(175, 182, 200),
            "s": rgb(105, 110, 128),
            "m": rgb(70, 74, 90),
            "A": rgb(90, 245, 110),
            "G": rgb(90, 245, 110),
            "w": rgb(220, 255, 220),
        },
    ),
    # Rail Lance: two long violet slugs with cyan tips.
    "ammo_rail_lance": (
        [
            "................",
            "................",
            "................",
            "................",
            "................",
            "...kkkkkkkkkk...",
            "..kMLLLLLLLLbk..",
            "..kMPPPPPPPPbk..",
            "...kkkkkkkkkkk..",
            "...kMLLLLLLLLbk.",
            "...kMPPPPPPPPbk.",
            "....kkkkkkkkkk..",
        ],
        {
            "L": rgb(220, 165, 255),
            "P": rgb(165, 80, 235),
            "b": rgb(80, 40, 120),
            "M": rgb(120, 220, 255),
        },
    ),
    # Arc Coil: a battery cell with a yellow bolt and stray sparks.
    "ammo_arc_coil": (
        [
            "................",
            "................",
            "................",
            "................",
            "......kkkk......",
            "......kYYk...w..",
            "....kkkkkkkkY...",
            "....kmmmYYmk....",
            "....kmmYYmmk....",
            ".Y..kmYYYYmk....",
            "..w.kmmYYmmk....",
            "....kmYYmmmk....",
            "....kSSSSSSk....",
            "....kkkkkkkk....",
        ],
        {
            "S": rgb(175, 182, 200),
            "m": rgb(70, 74, 90),
            "Y": rgb(255, 235, 90),
        },
    ),
    # Grenade Launcher: two round olive grenades with an orange band.
    "ammo_grenade_launcher": (
        [
            "................",
            "................",
            "................",
            "................",
            "................",
            "...kkk..........",
            "..kkSkk...kkk...",
            ".kOwOOOk.kkSkk..",
            ".kOOOOok.kOwOOOk",
            ".kRRRRRk.kOOOOok",
            ".kOOOook.kRRRRRk",
            "..kOook..kOOOook",
            "...kkk....kOook.",
            "...........kkk..",
        ],
        {
            "O": rgb(135, 165, 65),
            "o": rgb(85, 105, 40),
            "w": rgb(205, 225, 150),
            "R": rgb(255, 140, 40),
            "S": rgb(175, 182, 200),
        },
    ),
    # Nova Cannon: a plasma canister with a bright green core.
    "ammo_nova_cannon": (
        [
            "................",
            "................",
            "................",
            "................",
            "................",
            "................",
            ".kkkk......kkkk.",
            ".kSskkkkkkkkSsk.",
            ".kSskgGGGGgkSsk.",
            ".kSskGwwwwGkSsk.",
            ".kSskGwwwwGkSsk.",
            ".kSskgGGGGgkSsk.",
            ".kSskkkkkkkkSsk.",
            ".kkkk......kkkk.",
        ],
        {
            "S": rgb(175, 182, 200),
            "s": rgb(105, 110, 128),
            "G": rgb(110, 255, 90),
            "g": rgb(55, 165, 60),
            "w": rgb(225, 255, 210),
        },
    ),
}

# --------------------------------------------------------------------------
# Perks: a round badge with a symbol, coloured by perk.
# --------------------------------------------------------------------------

PERK_SYMBOLS = {
    # Infinite ammo: a round with an infinity loop under it.
    "overdrive": [
        "...ww...",
        "..wSSw..",
        "..wSSw..",
        "..wSSw..",
        "........",
        ".ww..ww.",
        "w..ww..w",
        ".ww..ww.",
    ],
    # Full restore: a medical cross.
    "nano_patch": [
        "..wwww..",
        "..wSSw..",
        "wwwSSwww",
        "wSSSSSSw",
        "wSSSSSSw",
        "wwwSSwww",
        "..wSSw..",
        "..wwww..",
    ],
    # Stun: a lightning bolt.
    "shock_field": [
        "....www.",
        "...wSw..",
        "..wSw...",
        ".wSSSSw.",
        "...wSw..",
        "..wSw...",
        ".wSw....",
        ".ww.....",
    ],
    # Invisibility: a closed eye.
    "cloak": [
        "........",
        "........",
        "w......w",
        ".wSSSSw.",
        "..wwww..",
        ".w.w.w..",
        "........",
        "........",
    ],
    # Ability ready: a fully charged battery.
    "recharge": [
        "...ww...",
        ".wwwwww.",
        ".wSSSSw.",
        ".w....w.",
        ".wSSSSw.",
        ".w....w.",
        ".wSSSSw.",
        ".wwwwww.",
    ],
}

PERK_COLORS = {
    "overdrive": (rgb(255, 150, 50), rgb(120, 55, 15)),
    "nano_patch": (rgb(90, 235, 120), rgb(20, 90, 45)),
    "shock_field": (rgb(90, 210, 255), rgb(20, 70, 120)),
    "cloak": (rgb(190, 120, 255), rgb(70, 35, 120)),
    "recharge": (rgb(255, 220, 65), rgb(115, 85, 15)),
}


def perk_badge(name):
    rim, fill = PERK_COLORS[name]
    px = [[(0, 0, 0, 0)] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            d = ((x - 7.5) ** 2 + (y - 7.5) ** 2) ** 0.5
            if d <= 5.8:
                px[y][x] = shade(fill, 12) if y < 7 else fill
            elif d <= 6.9:
                px[y][x] = rim
            elif d <= 7.8:
                px[y][x] = OUTLINE
    sym = {"w": WHITE, "S": rim}
    for y, row in enumerate(PERK_SYMBOLS[name]):
        for x, ch in enumerate(row):
            if ch in sym:
                px[y + 4][x + 4] = sym[ch]
    return px


def explosion(radius=26):
    """White blast disc (tinted in game): solid core, ragged dithered edge."""
    size = radius * 2 + 1
    px = [[(0, 0, 0, 0)] * size for _ in range(size)]
    for y in range(size):
        for x in range(size):
            d = ((x - radius) ** 2 + (y - radius) ** 2) ** 0.5 / radius
            n = noise(x, y, 7)
            if d < 0.45:
                px[y][x] = WHITE
            elif d < 0.8 + 0.15 * n:
                px[y][x] = (255, 255, 255, 200)
            elif d < 1.0 and (x + y) % 2 == 0:
                px[y][x] = (255, 255, 255, 120)
    return px


def shock_ring(radius=56):
    """Dashed ring drawn around a pirate while their shock field is up."""
    size = radius * 2 + 1
    px = [[(0, 0, 0, 0)] * size for _ in range(size)]
    import math
    for y in range(size):
        for x in range(size):
            dx, dy = x - radius, y - radius
            d = (dx * dx + dy * dy) ** 0.5
            if abs(d - (radius - 1)) < 0.8:
                dash = int((math.atan2(dy, dx) + math.pi) / (2 * math.pi) * 36) % 2
                px[y][x] = rgb(140, 225, 255) if dash else rgb(140, 225, 255, 90)
    return px


def aura_ring(radius, fill=0.06, edge=1.0):
    """A soft white ring, tinted in game, `radius` pixels across from its centre
    to the middle of its line so it draws 1:1 with no scaling. A thin line with
    feathered sides and a faint glow rising toward it from inside."""
    size = radius * 2 + 2
    c = size / 2
    px = [[(0, 0, 0, 0)] * size for _ in range(size)]
    for y in range(size):
        for x in range(size):
            d = ((x + 0.5 - c) ** 2 + (y + 0.5 - c) ** 2) ** 0.5
            line = max(0.0, 1.0 - abs(d - (radius - 0.5)) / 0.85) * edge
            t = min(1.0, max(0.0, (d - 0.5 * radius) / (0.5 * radius)))
            glow = fill * t * t * (3 - 2 * t) if d < radius else 0.0
            a = int(round(255 * max(line, glow)))
            if a:
                px[y][x] = (255, 255, 255, a)
    return px


def noise(x, y, seed):
    n = (x * 374761393 + y * 668265263 + seed * 144269504) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n >> 16) & 0xFF) / 255.0


def shade(c, amount):
    return tuple(max(0, min(255, int(v + amount))) for v in c[:3]) + (255,)


def blank(color):
    return [[color for _ in range(SIZE)] for _ in range(SIZE)]


# --------------------------------------------------------------------------
# Shared steel: the vault, its gate and the station helpers below.
# --------------------------------------------------------------------------

STEEL = rgb(58, 64, 82)
SEAM = rgb(28, 31, 42)
RIVET = rgb(120, 130, 156)
CYAN = rgb(80, 230, 255)
AMBER = rgb(255, 176, 50)


def bevel(px, x0, y0, x1, y1, hi, lo):
    """Lights the top and left edges of a box and darkens its bottom and right."""
    for x in range(x0, x1 + 1):
        px[y0][x] = hi
        px[y1][x] = lo
    for y in range(y0, y1 + 1):
        px[y][x0] = hi if y < y1 else lo
        px[y][x1] = lo if y > y0 else hi


def rivet(px, x, y):
    px[y][x] = RIVET
    px[y + 1][x + 1] = SEAM
    px[y][x + 1] = shade(RIVET, -40)


def vault_floor():
    """The strongroom floor the vault's gold sits on, 2x2 tiles: a heavy rim
    striped like a hazard line around a dark, sunken bed."""
    n = SIZE * 2
    bed = rgb(30, 32, 42)
    px = [[shade(bed, (noise(x, y, 51) - 0.5) * 6) for x in range(n)] for y in range(n)]
    for y in range(n):
        for x in range(n):
            d = min(x, y, n - 1 - x, n - 1 - y)
            if d < 4:
                stripe = ((x + y) // 3) % 2 == 0
                px[y][x] = rgb(214, 170, 40) if stripe else rgb(34, 32, 30)
            elif (x - 4) % 6 == 0 or (y - 4) % 6 == 0:
                px[y][x] = shade(bed, -8)
    for d, c in [(0, rgb(12, 13, 18)), (1, rgb(96, 104, 126)), (4, rgb(14, 15, 21)), (5, rgb(22, 24, 32))]:
        for i in range(d, n - d):
            px[d][i] = px[i][d] = c
            px[n - 1 - d][i] = px[i][n - 1 - d] = c if d != 1 else shade(c, -40)
    for x, y in [(1, 1), (n - 3, 1), (1, n - 3), (n - 3, n - 3)]:
        rivet(px, x, y)
    return px


# A small bar for the vault's pile; neighbours share their outlines.
VAULT_BAR = (
    [
        "kkkkkk",
        "kwYYYk",
        "kYYYYk",
        "kyyyok",
        "kkkkkk",
    ],
    {
        "Y": rgb(255, 214, 80),
        "y": rgb(226, 168, 40),
        "o": rgb(160, 104, 24),
    },
)

# The same bar in plain steel, tinted in game for the silver and platinum vaults.
VAULT_BAR_PLAIN = (
    VAULT_BAR[0],
    {
        "Y": rgb(226, 230, 238),
        "y": rgb(170, 176, 190),
        "o": rgb(112, 118, 134),
    },
)


def floor_shadows():
    """Soft shade a wall casts across the floor beside it: from the north, west
    and east."""
    north = blank((0, 0, 0, 0))
    west = blank((0, 0, 0, 0))
    east = blank((0, 0, 0, 0))
    for d, alpha in enumerate([120, 80, 48, 24, 10]):
        for i in range(SIZE):
            north[d][i] = (0, 0, 0, alpha)
            if d < 3:
                west[i][d] = (0, 0, 0, alpha // 2)
                east[i][SIZE - 1 - d] = (0, 0, 0, alpha // 2)
    return {"shade_n": north, "shade_w": west, "shade_e": east}


# --------------------------------------------------------------------------
# Station tiles, one set per deck. Seen from above and a little south: walls
# show a front face where the floor below meets them and a dark cap elsewhere.
# Each deck draws the same shapes in its own steel, paint and light, so riding
# a lift down reads as arriving somewhere new. Floors, grating and the hull are
# drawn as 2x2 panels and cut into quarters, so a room isn't a grid of squares.
# --------------------------------------------------------------------------

DECK_THEMES = [
    # Deck 1, the cargo hold: cold blue steel, cyan lamps, yellow hazard paint.
    SimpleNamespace(
        steel=rgb(58, 64, 82), cap=rgb(30, 34, 47), rim=rgb(104, 114, 142), face=rgb(52, 58, 76),
        light=rgb(80, 230, 255), screen=rgb(70, 220, 130), led=rgb(255, 176, 50), paint=rgb(232, 190, 40),
        pipe=rgb(92, 98, 116), grime=rgb(22, 24, 32), under=rgb(13, 14, 21), hull=rgb(15, 17, 25),
    ),
    # Deck 2, the engine works: warm gunmetal, copper pipe, orange lamps, rust.
    SimpleNamespace(
        steel=rgb(68, 63, 58), cap=rgb(35, 32, 30), rim=rgb(140, 122, 100), face=rgb(62, 56, 51),
        light=rgb(255, 156, 64), screen=rgb(255, 190, 80), led=rgb(120, 230, 110), paint=rgb(232, 118, 40),
        pipe=rgb(156, 102, 66), grime=rgb(52, 32, 20), under=rgb(17, 13, 12), hull=rgb(20, 17, 16),
    ),
    # Deck 3, the treasury: dark teal slate, brass trim, mint lamps, gold paint.
    SimpleNamespace(
        steel=rgb(46, 56, 60), cap=rgb(22, 27, 31), rim=rgb(178, 148, 86), face=rgb(40, 49, 54),
        light=rgb(120, 255, 196), screen=rgb(120, 255, 196), led=rgb(255, 210, 90), paint=rgb(214, 172, 60),
        pipe=rgb(156, 134, 86), grime=rgb(16, 22, 24), under=rgb(9, 13, 15), hull=rgb(12, 16, 18),
    ),
]

PANEL = SIZE * 2
# Tiny stencil digits, 3x5, for floor markings.
DIGITS = {
    "0": ["###", "#.#", "#.#", "#.#", "###"],
    "1": [".#.", "##.", ".#.", ".#.", "###"],
    "2": ["###", "..#", "###", "#..", "###"],
    "3": ["###", "..#", ".##", "..#", "###"],
    "4": ["#.#", "#.#", "###", "..#", "..#"],
    "7": ["###", "..#", ".#.", ".#.", ".#."],
}


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3)) + (255,)


def quarters(px):
    """A 2x2 panel cut into its top-left, top-right, bottom-left and
    bottom-right tiles, the order the game picks them in."""
    return [[row[x : x + SIZE] for row in px[y : y + SIZE]] for y in (0, SIZE) for x in (0, SIZE)]


def brushed(base, w, h, seed):
    """Steel with a grain: soft blotches and faint streaks running across."""
    return [
        [
            shade(
                base,
                (noise(x // 3, y // 3, seed) - 0.5) * 6
                + (noise(x // 7, y, seed + 1) - 0.5) * 5
                + (noise(x, y, seed + 2) - 0.5) * 3,
            )
            for x in range(w)
        ]
        for y in range(h)
    ]


def plate_rim(px, x0, y0, x1, y1, T):
    """Frames a plate: a dark seam on its top and left (the plates beside it
    close the other two sides), a lit inner edge and a shaded far edge."""
    seam, hi, lo = shade(T.steel, -28), shade(T.steel, 13), shade(T.steel, -11)
    for x in range(x0, x1 + 1):
        px[y1][x] = lo
        if x > x0:
            px[y0 + 1][x] = hi
    for y in range(y0, y1 + 1):
        px[y][x1] = lo
        if y0 < y < y1:
            px[y][x0 + 1] = hi
    for x in range(x0, x1 + 1):
        px[y0][x] = seam
    for y in range(y0, y1 + 1):
        px[y][x0] = seam


def recess(px, x0, y0, x1, y1, T, depth=-8):
    """Sinks a box into a plate: shaded top and left, lit bottom and right."""
    for y in range(y0 + 1, y1):
        for x in range(x0 + 1, x1):
            px[y][x] = shade(px[y][x], depth)
    bevel(px, x0, y0, x1, y1, shade(T.steel, -26), shade(T.steel, 12))


def bolt(px, x, y, T):
    px[y][x] = shade(T.steel, 44)
    px[y][x + 1] = shade(T.steel, 20)
    px[y + 1][x] = shade(T.steel, 12)
    px[y + 1][x + 1] = shade(T.steel, -26)


def weather(px, T, seed, grime=1.0, scratches=2):
    """Grime gathered in patches, and a few bright scratches."""
    h, w = len(px), len(px[0])
    for y in range(h):
        for x in range(w):
            g = noise(x // 4, y // 4, seed) * 0.6 + noise(x // 2, y // 2, seed + 1) * 0.4
            if g > 0.68:
                px[y][x] = mix(px[y][x], T.grime, (g - 0.68) * 1.5 * grime)
    for k in range(scratches):
        sx, sy = int(noise(k, 1, seed + 7) * w), int(noise(k, 2, seed + 7) * h)
        length, slope = 2 + int(noise(k, 3, seed + 7) * 5), noise(k, 4, seed + 7) > 0.5
        for i in range(length):
            x, y = sx + i, sy + (i // 3 if slope else 0)
            if 2 <= x < w - 1 and 2 <= y < h - 1:
                px[y][x] = shade(px[y][x], 7)


def stencil(px, text, x0, y0, color, seed, keep=0.3):
    """Worn stencil lettering: boots have taken some of the paint."""
    for i, ch in enumerate(text):
        for dy, line in enumerate(DIGITS[ch]):
            for dx, on in enumerate(line):
                x, y = x0 + i * 4 + dx, y0 + dy
                if on == "#" and noise(x, y, seed) > keep:
                    px[y][x] = mix(px[y][x], color, 0.55)


def floor_panels(T, seed):
    """Heavy plate, four small plates, tread plate, a service hatch, staggered
    planks, and a plate with loading-bay paint. Each 32x32."""
    n = PANEL

    heavy = brushed(T.steel, n, n, seed)
    plate_rim(heavy, 0, 0, n - 1, n - 1, T)
    for y in range(3, n - 3):
        for x in range(3, n - 3):
            # Worn brighter down the middle, where boots go.
            d = math.hypot(x - 15.5, y - 15.5)
            heavy[y][x] = shade(heavy[y][x], max(0.0, 5 - d * 0.4))
    for x, y in [(3, 3), (27, 3), (3, 27), (27, 27)]:
        bolt(heavy, x, y, T)

    four = brushed(shade(T.steel, 3), n, n, seed + 10)
    for x0, y0 in [(0, 0), (16, 0), (0, 16), (16, 16)]:
        plate_rim(four, x0, y0, x0 + 15, y0 + 15, T)
        bolt(four, x0 + 3, y0 + 3, T)
        bolt(four, x0 + 11, y0 + 11, T)

    tread = brushed(shade(T.steel, -3), n, n, seed + 20)
    plate_rim(tread, 0, 0, n - 1, n - 1, T)
    for y in range(5, n - 5):
        for x in range(5, n - 5):
            # Raised diamond grip, two pixels long, alternating direction.
            cell = (x // 4 + y // 4) % 2
            if (x + (y if cell else -y)) % 4 == 0:
                tread[y][x] = shade(T.steel, 11)
                if y + 1 < n - 5:
                    tread[y + 1][x] = shade(T.steel, -9)
    bevel(tread, 4, 4, n - 5, n - 5, shade(T.steel, 9), shade(T.steel, -14))

    hatch = brushed(T.steel, n, n, seed + 30)
    plate_rim(hatch, 0, 0, n - 1, n - 1, T)
    # A round access hatch: its rim shaded like a shallow dish, hinged on the
    # left, a pull notch on the right.
    for y in range(n):
        for x in range(n):
            dx, dy = x - 15.5, y - 15.5
            r = math.hypot(dx, dy)
            if r < 8.0:
                hatch[y][x] = shade(hatch[y][x], -3)
            elif r < 9.3:
                lit = (dx + dy) / max(r, 1.0)
                hatch[y][x] = shade(T.steel, 20 * lit - 10)
    for y in (11, 19):
        for x in (5, 6):
            hatch[y][x] = shade(T.steel, 18)
            hatch[y + 1][x] = shade(T.steel, -20)
    for y in range(14, 18):
        hatch[y][22] = shade(T.steel, -34)
    for x, y in [(10, 10), (20, 10), (10, 20), (20, 20), (3, 3), (27, 27)]:
        bolt(hatch, x, y, T)

    planks = brushed(shade(T.steel, -4), n, n, seed + 40)
    plate_rim(planks, 0, 0, n - 1, 15, T)
    plate_rim(planks, 0, 16, 10, n - 1, T)
    plate_rim(planks, 11, 16, n - 1, n - 1, T)
    for x, y in [(3, 7), (27, 7), (6, 23), (27, 23)]:
        bolt(planks, x, y, T)

    marked = brushed(T.steel, n, n, seed + 50)
    plate_rim(marked, 0, 0, n - 1, n - 1, T)
    # Corner brackets of a loading bay, and the bay's number.
    for cx, cy, sx, sy in [(4, 4, 1, 1), (27, 4, -1, 1), (4, 27, 1, -1), (27, 27, -1, -1)]:
        for i in range(6):
            for x, y in [(cx + sx * i, cy), (cx, cy + sy * i), (cx + sx * i, cy + sy), (cx + sx, cy + sy * i)]:
                if noise(x, y, seed + 51) > 0.22:
                    marked[y][x] = mix(marked[y][x], T.paint, 0.6)
    stencil(marked, "07" if seed % 2 else "13", 12, 13, T.paint, seed + 52, keep=0.15)

    panels = [heavy, four, tread, hatch, planks, marked]
    for i, px in enumerate(panels):
        weather(px, T, seed + 100 + i)
    return panels


def pipe_run(px, at, thick, color, vertical, span=None):
    """A round pipe: lit along one side, shaded along the other, with a
    flange every 16 pixels."""
    n = len(px)
    for along in range(n) if span is None else range(*span):
        for i in range(thick):
            t = i / max(1, thick - 1)
            tone = 20 - 44 * t + (14 if along % 16 in (6, 7) else 0)
            x, y = (at + i, along) if vertical else (along, at + i)
            px[y][x] = shade(color, tone)


def grate_panels(T, seed):
    """Catwalk grating over the deck's guts: pipes running across, pipes
    running down past a cable bundle, a lit service trench, and plain dark."""
    n = PANEL

    def under(s):
        return [[shade(T.under, (noise(x // 2, y // 2, seed + s) - 0.5) * 5) for x in range(n)] for y in range(n)]

    beam = shade(T.under, 12)
    across = under(0)
    for y in range(n):
        for x in (14, 15, 16, 17):
            across[y][x] = shade(beam, -4 if x in (14, 17) else 2)
    pipe_run(across, 6, 5, mix(T.pipe, T.under, 0.15), False)
    pipe_run(across, 20, 4, mix(T.pipe, T.under, 0.25), False)

    down = under(1)
    for x in range(n):
        for y in (14, 15, 16):
            down[y][x] = shade(beam, -3)
    pipe_run(down, 5, 5, mix(T.pipe, T.under, 0.15), True)
    pipe_run(down, 23, 3, mix(T.pipe, T.under, 0.25), True)
    for x in range(10, 20):
        # A bundle of cables, each its own colour.
        for i, c in enumerate([rgb(120, 40, 40), rgb(40, 60, 110), rgb(60, 60, 64)]):
            down[24 + i][x] = mix(c, T.under, 0.2)

    lit = under(2)
    for y in range(n):
        for x in range(n):
            glow = max(0.0, 1.0 - abs(y - 15.5) / 9.0) ** 1.5
            lit[y][x] = mix(lit[y][x], T.light, glow * 0.55)
    pipe_run(lit, 26, 3, mix(T.pipe, T.under, 0.3), False)

    dark = under(3)
    for i in range(n):
        for d in (-1, 0, 1):
            dark[i][15 + d] = shade(beam, -6)
            dark[15 + d][i] = shade(beam, -6)

    out = []
    for px in [across, down, lit, dark]:
        top = shade(T.steel, 10)
        for y in range(n):
            for x in range(n):
                if x == 0 or y == 0:
                    px[y][x] = shade(T.steel, -20)
                elif x == 1 or y == 1:
                    px[y][x] = shade(T.steel, 9)
                elif x == n - 1 or y == n - 1:
                    px[y][x] = shade(T.steel, -8)
                elif x == 2 or y == 2 or x == n - 2 or y == n - 2:
                    px[y][x] = T.steel
                elif y % 8 == 7:
                    px[y][x] = top
                elif x % 3 == 0:
                    px[y][x] = shade(T.steel, 4 if y % 8 else 10)
                else:
                    # The bars shade what's below them on their right and underneath.
                    depth = 0.6 if x % 3 == 1 or y % 8 == 0 else 0.9
                    px[y][x] = tuple(int(v * depth) for v in px[y][x][:3]) + (255,)
        weather(px, T, seed + 60 + len(out), grime=0.5, scratches=2)
        out.append(px)
    return out


def hull_panel(T, seed):
    """The hull between rooms: big, faint plates, barely lighter than space."""
    n = PANEL
    px = [[shade(T.hull, (noise(x // 4, y // 4, seed) - 0.5) * 3) for x in range(n)] for y in range(n)]
    for i in range(n):
        px[0][i] = px[i][0] = shade(T.hull, -5)
        px[1][i] = shade(T.hull, 4)
        if i < 16:
            px[16][i] = shade(T.hull, -4)
            px[17][i] = shade(T.hull, 3)
    for x, y in [(4, 4), (27, 9), (9, 24)]:
        px[y][x] = shade(T.hull, 12)
        px[y + 1][x + 1] = shade(T.hull, -6)
    return px


def wall_cap(T, mask):
    """The top of a wall. `mask` bits (N=1, E=2, S=4, W=8) mark open floor
    beside it; those edges get a lit, bolted rim."""
    px = [
        [shade(T.cap, (noise(x, y, 7) - 0.5) * 4 - (4 if (x + y) % 8 == 0 else 0)) for x in range(SIZE)]
        for y in range(SIZE)
    ]
    rim, mid, dark = T.rim, shade(T.rim, -36), shade(T.cap, -10)
    last = SIZE - 1
    edges = [
        (1, lambda d, i: (i, d)),
        (4, lambda d, i: (i, last - d)),
        (8, lambda d, i: (d, i)),
        (2, lambda d, i: (last - d, i)),
    ]
    for bit, at in edges:
        if not mask & bit:
            continue
        for i in range(SIZE):
            for d, c in [(2, dark), (1, mid), (0, rim)]:
                x, y = at(d, i)
                px[y][x] = c
        for i in (4, 12):
            x, y = at(1, i)
            px[y][x] = shade(T.rim, 30)
    return px


def face_base(T, seed):
    """A wall's front face: a lit lip, a body lighter near the top, a painted
    band in the deck's colour, and a dark kickplate."""
    px = [[shade(T.face, (noise(x, y, seed) - 0.5) * 5 + 5 - y * 0.8) for x in range(SIZE)] for y in range(SIZE)]
    for x in range(SIZE):
        px[0][x] = T.rim
        px[1][x] = shade(T.rim, -34)
        px[2][x] = shade(T.face, -22)
        px[12][x] = shade(T.face, 12)
        worn = noise(x, 13, seed) < 0.18
        px[13][x] = mix(T.paint, T.face, 0.75 if worn else 0.4)
        px[14][x] = shade(T.face, -26)
        px[15][x] = shade(T.face, -40)
    return px


def wall_faces(T):
    """Panels, a rib, pipes, a lamp, a terminal, lockers, a grille, a junction
    box and a warning plate. The lamp and terminal light the floor below."""
    hi, lo = shade(T.face, 16), shade(T.face, -18)

    panels = face_base(T, 21)
    bevel(panels, 1, 3, 7, 11, hi, lo)
    bevel(panels, 8, 3, 14, 11, hi, lo)
    for x, y in [(2, 4), (13, 4), (2, 10), (13, 10)]:
        panels[y][x] = shade(T.rim, -20)
        panels[y + 1][x] = lo

    rib = face_base(T, 22)
    for y in range(3, 13):
        rib[y][6] = shade(T.face, 22)
        rib[y][7] = rib[y][8] = shade(T.face, 10)
        rib[y][9] = shade(T.face, -12)
        rib[y][10] = shade(T.face, -22)
    for y in (4, 10):
        rib[y][7] = shade(T.rim, -10)
        rib[y + 1][8] = lo
    for y in (5, 7, 9):
        for x in list(range(2, 5)) + list(range(12, 15)):
            rib[y][x] = shade(T.face, -30)
            rib[y + 1][x] = shade(T.face, 8)

    pipes = face_base(T, 23)
    for x in range(SIZE):
        for y0 in (4, 8):
            for i, tone in enumerate([20, 2, -24]):
                pipes[y0 + i][x] = shade(T.pipe, tone)
    for x in (3, 11):
        for y in range(3, 12):
            pipes[y][x] = shade(T.face, -28)
        pipes[3][x] = shade(T.rim, -10)
    # A valve wheel on the top pipe.
    for x, y in [(13, 4), (12, 5), (13, 5), (14, 5), (13, 6)]:
        pipes[y][x] = rgb(196, 52, 48)
    pipes[5][13] = rgb(250, 120, 100)

    lamp = face_base(T, 24)
    bevel(lamp, 2, 3, 13, 11, lo, hi)
    for y in range(4, 11):
        for x in range(3, 13):
            heat = 1.0 - abs(y - 7) / 4.0
            lamp[y][x] = mix(T.light, WHITE, heat * 0.45) if 4 < y < 10 else shade(T.light, -110)
    for y in range(4, 11):
        for x in (5, 8, 11):
            lamp[y][x] = shade(T.face, -30)

    terminal = face_base(T, 25)
    bevel(terminal, 2, 3, 13, 11, lo, hi)
    glass = mix(T.under, T.screen, 0.12)
    for y in range(4, 11):
        for x in range(3, 13):
            terminal[y][x] = glass if y % 2 else shade(glass, -4)
    for y, length in [(5, 7), (7, 4), (8, 8)]:
        for x in range(4, 4 + length):
            terminal[y][x] = T.screen if x < 3 + length else shade(T.screen, -90)
    terminal[5][12] = WHITE
    terminal[12][6] = T.led
    terminal[12][9] = shade(T.led, -90)

    lockers = face_base(T, 26)
    for x0 in (2, 8):
        bevel(lockers, x0, 3, x0 + 5, 12, hi, lo)
        for y in (4, 6):
            for x in range(x0 + 1, x0 + 5):
                lockers[y][x] = shade(T.face, -30)
                lockers[y + 1][x] = shade(T.face, 6)
    for y in (8, 9):
        lockers[y][6] = lockers[y][9] = shade(T.rim, 20)
    lockers[10][4] = T.paint

    grille = face_base(T, 27)
    bevel(grille, 2, 3, 13, 11, lo, hi)
    for y in range(4, 11):
        for x in range(3, 13):
            grille[y][x] = shade(T.face, 10) if y % 2 else shade(T.face, -34)

    # A breaker panel: a row of switches under their lights.
    junction = face_base(T, 28)
    bevel(junction, 2, 3, 13, 11, shade(T.face, 24), shade(T.face, -24))
    for y in range(4, 11):
        for x in range(3, 13):
            junction[y][x] = shade(T.face, 6)
    for i, x in enumerate((4, 7, 10)):
        junction[5][x] = junction[5][x + 1] = [T.led, T.led, rgb(255, 60, 60)][i]
        for y in (7, 8, 9):
            junction[y][x] = junction[y][x + 1] = shade(T.face, -30)
        up = i != 2
        junction[8 if up else 9][x] = shade(T.rim, 30)
        junction[7 if up else 8][x + 1] = shade(T.rim, 10)

    warning = face_base(T, 29)
    for y in range(3, 12):
        for x in range(2, 14):
            stripe = ((x + y) // 2) % 2 == 0
            c = T.paint if stripe else shade(T.under, 10)
            warning[y][x] = mix(c, T.face, 0.4) if noise(x, y, 291) > 0.8 else c
    bevel(warning, 2, 3, 13, 11, hi, lo)

    faces = [panels, rib, pipes, lamp, terminal, lockers, grille, junction, warning]
    for i, px in enumerate(faces):
        weather(px, T, 300 + i, grime=0.6, scratches=1)
    return faces


def door_tile(T):
    """A blast door set into a wall face: two leaves striped at the foot, a
    seam, view slits and a red lamp over it."""
    px = face_base(T, 31)
    leaf = shade(T.face, 22)
    for y in range(3, SIZE):
        for x in range(2, SIZE - 2):
            px[y][x] = shade(leaf, (noise(x, y, 32) - 0.5) * 6)
    for y in range(11, SIZE - 1):
        for x in range(2, SIZE - 2):
            if ((x + y) // 2) % 2 == 0:
                px[y][x] = mix(T.paint, leaf, 0.2)
    bevel(px, 2, 3, 7, SIZE - 1, shade(leaf, 22), shade(leaf, -26))
    bevel(px, 8, 3, SIZE - 3, SIZE - 1, shade(leaf, 22), shade(leaf, -26))
    for y in range(3, SIZE):
        px[y][7] = shade(T.under, 2)
        px[y][1] = px[y][SIZE - 2] = shade(T.rim, -40)
    for x in (4, 5, 10, 11):
        px[6][x] = shade(T.under, 6)
        px[7][x] = mix(T.light, T.under, 0.6)
    px[2][7] = px[2][8] = rgb(255, 60, 60)
    px[1][7] = px[1][8] = rgb(255, 160, 150)
    return px


def vent_tile(T):
    """Floor vent that reinforcements crawl out of, hot red inside."""
    px = brushed(T.steel, SIZE, SIZE, 61)
    plate_rim(px, 0, 0, SIZE - 1, SIZE - 1, T)
    for y in range(SIZE):
        for x in range(SIZE):
            soot = math.hypot(x - 7.5, y - 7.5)
            if 5.5 < soot < 8.5:
                px[y][x] = mix(px[y][x], rgb(10, 8, 8), 0.3)
    bevel(px, 3, 3, SIZE - 4, SIZE - 4, shade(T.steel, -30), shade(T.steel, 14))
    for y in range(4, SIZE - 4):
        for x in range(4, SIZE - 4):
            glow = max(0.0, 1.0 - abs(x - 7.5) / 4.5) * (0.55 + 0.45 * (y / SIZE))
            if y % 2 == 0:
                px[y][x] = shade(T.steel, 4)
            else:
                px[y][x] = rgb(int(80 + 170 * glow), int(18 + 60 * glow ** 2), int(20 + 20 * glow))
    for x, y in [(1, 1), (13, 1), (1, 13), (13, 13)]:
        bolt(px, x, y, T)
    return px


def hazard_tile(T):
    """The escape pad's paint. Seamless, so a pad reads as one marking."""
    px = blank(CLEAR)
    dark = shade(T.under, 14)
    for y in range(SIZE):
        for x in range(SIZE):
            stripe = ((x + y) // 4) % 2 == 0
            c = T.paint if stripe else dark
            wear = noise(x, y, 31) * 0.7 + noise(x // 3, y // 3, 32) * 0.3
            if stripe and wear > 0.74:
                c = mix(T.paint, dark, 0.45)
            elif not stripe and wear > 0.86:
                c = shade(dark, 14)
            px[y][x] = shade(c, (noise(x, y, 33) - 0.5) * 8)
    return px


def lift_platform(T):
    """Cargo lift platform, 2x2 tiles: ride it down to the next deck. Returned
    as its four quarters: top-left, top-right, bottom-left, bottom-right."""
    n = PANEL
    base = shade(T.steel, -10)
    px = brushed(base, n, n, 11)
    for y in range(5, n - 5):
        for x in range(5, n - 5):
            if (x + y) % 4 == 0 and (x - y) % 8 == 0:
                px[y][x] = shade(base, 20)
                px[y + 1][x] = shade(base, -14)
    # Heavy rim with running lights and hazard corners.
    for i in range(n):
        for d, c in [(0, shade(T.under, 2)), (1, T.rim), (2, shade(T.rim, -34)), (3, shade(base, -22))]:
            px[d][i] = px[i][d] = c
            px[n - 1 - d][i] = px[i][n - 1 - d] = c if d else shade(T.under, -4)
    for i in range(5, n - 5, 4):
        for row, col in [(1, i), (n - 2, i), (i, 1), (i, n - 2)]:
            px[row][col] = CYAN
    for cx, cy in [(4, 4), (n - 8, 4), (4, n - 8), (n - 8, n - 8)]:
        for y in range(4):
            for x in range(4):
                px[cy + y][cx + x] = T.paint if (x + y) % 3 == 0 else shade(T.under, 12)
    # Two glowing chevrons pointing down.
    for top, color in ((10, CYAN), (16, shade(CYAN, -90))):
        for k in range(6):
            for x in (10 + k, n - 11 - k):
                px[top + k][x] = color
                px[top + k + 1][x] = shade(color, -70)
    return quarters(px)


def spill(color, peak=58):
    """Light thrown onto the floor below a lamp, 32x32 and centred under it:
    brightest at the wall, fading down and out."""
    px = [[CLEAR for _ in range(PANEL)] for _ in range(PANEL)]
    for y in range(PANEL):
        for x in range(PANEL):
            across = max(0.0, 1.0 - (abs(x - 15.5) / 16.0) ** 2)
            down = max(0.0, 1.0 - y / PANEL) ** 1.7
            a = int(peak * across * down)
            if a:
                px[y][x] = color[:3] + (a,)
    return px


def deck_tiles(T, seed):
    """Every tile one deck needs, by file name."""
    tiles = {}
    for kind, panels in [("floor", floor_panels(T, seed)), ("grate", grate_panels(T, seed))]:
        for i, px in enumerate(panels):
            for q, quarter in enumerate(quarters(px)):
                tiles[f"{kind}_{i}_{q}"] = quarter
    tiles.update({f"hull_{q}": px for q, px in enumerate(quarters(hull_panel(T, seed)))})
    tiles.update({f"wall_face_{i}": px for i, px in enumerate(wall_faces(T))})
    tiles.update({f"wall_cap_{mask}": wall_cap(T, mask) for mask in range(16)})
    tiles.update({f"lift_{i}": px for i, px in enumerate(lift_platform(T))})
    tiles.update({"door": door_tile(T), "vent": vent_tile(T), "hazard": hazard_tile(T)})
    tiles.update({"spill_lamp": spill(T.light), "spill_screen": spill(T.screen, 40), "spill_door": spill(rgb(255, 60, 60), 36)})
    return tiles


def floor_decals():
    """Wear and mess scattered over plain floor, all translucent so they take
    on each deck's steel: oil, a scorch mark, loose bolts, scuffs, a loose
    cable, a coolant puddle, a painted arrow, a drain, rust and a dent."""
    def canvas16():
        return [[CLEAR for _ in range(SIZE)] for _ in range(SIZE)]

    oil = canvas16()
    for y in range(SIZE):
        for x in range(SIZE):
            d = ((x - 7.5) / 6.0) ** 2 + ((y - 8.5) / 4.2) ** 2 + (noise(x // 2, y // 2, 401) - 0.5) * 0.9
            if d < 1.0:
                oil[y][x] = (6, 6, 10, 120 if d < 0.65 else 64)
    for x, y in [(5, 7), (6, 7), (9, 9)]:
        oil[y][x] = (120, 130, 170, 70)

    scorch = canvas16()
    for y in range(SIZE):
        for x in range(SIZE):
            r = math.hypot(x - 7.5, y - 7.5)
            reach = 4.0 + noise(int((math.atan2(y - 7.5, x - 7.5) + math.pi) * 2.5), 0, 402) * 4.0
            if r < reach:
                scorch[y][x] = (10, 7, 6, int(170 * (1.0 - r / reach) ** 0.7))

    debris = canvas16()
    for x, y in [(3, 4), (11, 3), (8, 11), (12, 12)]:
        debris[y][x] = (150, 156, 172, 255)
        debris[y][x + 1] = (90, 94, 108, 255)
        debris[y + 1][x + 1] = (0, 0, 0, 90)
    for i in range(5):
        debris[7 + i // 3][3 + i] = (110, 116, 130, 255)
        debris[8 + i // 3][3 + i] = (0, 0, 0, 70)

    scuffs = canvas16()
    for k, (x0, y0, length) in enumerate([(2, 4, 7), (6, 7, 8), (3, 11, 6), (9, 13, 5)]):
        for i in range(length):
            scuffs[y0 + i // 4][x0 + i] = (255, 255, 255, 16 + 6 * (k % 2))

    cable = canvas16()
    pts = [(2 + i * 0.5, 9 + 3.5 * math.sin(i / 24 * math.pi * 1.6)) for i in range(25)]
    for x, y in pts:
        xi, yi = int(x), int(y)
        cable[yi + 1][xi] = (0, 0, 0, 80)
        cable[yi][xi] = (34, 34, 40, 255)
        cable[yi - 1][xi] = (70, 72, 84, 255)
    for x, y in (pts[0], pts[-1]):
        cable[int(y) - 1][int(x)] = cable[int(y)][int(x)] = (200, 160, 60, 255)

    coolant = canvas16()
    for y in range(SIZE):
        for x in range(SIZE):
            d = ((x - 8) / 5.5) ** 2 + ((y - 8) / 3.6) ** 2 + (noise(x // 2, y // 2, 403) - 0.5) * 0.8
            if d < 1.0:
                coolant[y][x] = (60, 220, 190, 80 if d > 0.75 else 44)
    coolant[6][6] = coolant[7][7] = (200, 255, 240, 110)

    arrow = canvas16()
    shape = [(x, 7) for x in range(2, 12)] + [(x, 8) for x in range(2, 12)]
    shape += [(11 - k + d, 7 - k) for k in range(1, 5) for d in (0, 1)]
    shape += [(11 - k + d, 8 + k) for k in range(1, 5) for d in (0, 1)]
    for x, y in shape:
        if 0 <= y < SIZE and noise(x, y, 404) > 0.25:
            arrow[y][x] = (230, 230, 220, 54)

    drain = canvas16()
    for y in range(SIZE):
        for x in range(SIZE):
            r = math.hypot(x - 7.5, y - 7.5)
            if r < 5.0:
                drain[y][x] = (0, 0, 0, 150) if (x % 2 and r < 4.0) else (0, 0, 0, 70)
            elif r < 6.0:
                drain[y][x] = (255, 255, 255, 34) if y < 8 else (0, 0, 0, 90)

    rust = canvas16()
    for y in range(SIZE):
        for x in range(SIZE):
            g = noise(x // 2, y // 2, 405) * 0.6 + noise(x, y, 406) * 0.4
            d = math.hypot(x - 7.5, y - 8.0) / 7.0
            if g > 0.35 + d * 0.5:
                rust[y][x] = (150, 70, 28, 70 if g > 0.6 + d * 0.3 else 40)

    dent = canvas16()
    for i in range(10):
        x, y = 3 + i, 6 + (i * 3) // 5
        dent[y][x] = (0, 0, 0, 100)
        dent[y + 1][x] = (255, 255, 255, 34)
    for i in range(4):
        dent[9 + i][8 + i // 2] = (0, 0, 0, 80)

    return {
        "oil": oil, "scorch": scorch, "debris": debris, "scuffs": scuffs, "cable": cable,
        "coolant": coolant, "arrow": arrow, "drain": drain, "rust": rust, "dent": dent,
    }


def floor_lips():
    """The edge of grating or paint against plate floor: a lit lip and a dark
    drop, on each side."""
    lips = {}
    for side in "nesw":
        px = [[CLEAR for _ in range(SIZE)] for _ in range(SIZE)]
        for i in range(SIZE):
            for d, c in [(0, (255, 255, 255, 34)), (1, (0, 0, 0, 120)), (2, (0, 0, 0, 46))]:
                x, y = {"n": (i, d), "s": (i, SIZE - 1 - d), "w": (d, i), "e": (SIZE - 1 - d, i)}[side]
                px[y][x] = c
        lips[f"lip_{side}"] = px
    return lips


# How often each look turns up; keep these in step with `draw_deck` in room.rs.
FLOOR_WEIGHTS = [34, 14, 14, 10, 22, 6]
FLOOR_ACCENT = 5
GRATE_WEIGHTS = [35, 30, 10, 25]
FACE_WEIGHTS = [26, 18, 14, 8, 6, 10, 8, 6, 4]
FACE_SPILLS = {3: "spill_lamp", 4: "spill_screen"}
DECALS = ["oil", "scorch", "debris", "scuffs", "cable", "coolant", "arrow", "drain", "rust", "dent"]
DECAL_WEIGHTS = [
    [14, 6, 14, 22, 8, 6, 8, 10, 2, 10],
    [16, 12, 14, 18, 8, 4, 4, 8, 14, 10],
    [10, 4, 10, 22, 6, 10, 10, 12, 2, 8],
]
DECAL_CHANCE = 11


def tile_hash(col, row, salt):
    """The same hash as `hash` in room.rs, so the preview matches the game."""
    m = 0xFFFFFFFF
    h = (col | row << 16) & m
    h ^= (salt * 0x9E3779B9) & m
    h ^= h >> 16
    h = (h * 0x85EBCA6B) & m
    h ^= h >> 13
    h = (h * 0xC2B2AE35) & m
    h ^= h >> 16
    return h


def tile_pick(col, row, salt, weights):
    roll = tile_hash(col, row, salt) % sum(weights)
    for i, w in enumerate(weights):
        if roll < w:
            return i
        roll -= w
    return 0


def panel_pick(col, row, salt, weights, accent=None):
    """Which 2x2 panel a tile belongs to, and which quarter of it it shows.
    Panels near each other mostly match, so an area reads as fitted out alike;
    an `accent` look only turns up here and there."""
    bc, br = col // 2, row // 2
    area = tile_pick(bc // 3, br // 3, salt, weights)
    keep = area != accent and tile_hash(bc, br, salt + 1) % 100 < 60
    look = area if keep else tile_pick(bc, br, salt + 2, weights)
    return f"{look}_{2 * (row % 2) + col % 2}"


def blend(dst, src, ox, oy):
    for y, row in enumerate(src):
        for x, c in enumerate(row):
            X, Y = ox + x, oy + y
            if c[3] and 0 <= Y < len(dst) and 0 <= X < len(dst[0]):
                a, d = c[3] / 255.0, dst[Y][X]
                dst[Y][X] = tuple(int(d[i] * (1 - a) + c[i] * a) for i in range(3)) + (255,)


PREVIEW_PLAN = [
    "######################",
    "######################",
    "#####D########D#######",
    "##..................##",
    "##....,,,,,,,,......##",
    "##.v..,,,,,,,,...LL.##",
    "##....,,,,,,,,...LL.##",
    "##..................##",
    "##..##........=====.##",
    "##..##........=====.##",
    "##............=====.##",
    "#######,,,,###########",
    "#######,,,,###########",
    "######################",
]


def station_preview(sets, shared):
    """A mock room drawn the way `draw_deck` draws one, in each deck's set."""
    plan = PREVIEW_PLAN
    rows, cols = len(plan), len(plan[0])
    is_floor = lambda r, c: 0 <= r < rows and 0 <= c < cols and plan[r][c] not in "#D"
    family = lambda ch: {",": 1, "=": 2}.get(ch, 0)
    out = []
    for deck, tiles in enumerate(sets):
        img = [[rgb(6, 7, 10) for _ in range(cols * SIZE)] for _ in range(rows * SIZE)]
        later = []
        for r in range(rows):
            for c in range(cols):
                ch = plan[r][c]
                touches = any(is_floor(r + dr, c + dc) for dr in (-1, 0, 1) for dc in (-1, 0, 1))
                if ch == "#" and not touches:
                    tile = tiles[f"hull_{2 * (r % 2) + c % 2}"]
                elif ch == "#" and is_floor(r + 1, c):
                    look = tile_pick(c, r, 3, FACE_WEIGHTS)
                    tile = tiles[f"wall_face_{look}"]
                    if look in FACE_SPILLS:
                        later.append((tiles[FACE_SPILLS[look]], c * SIZE - 8, (r + 1) * SIZE))
                elif ch == "#":
                    mask = is_floor(r - 1, c) | is_floor(r, c + 1) << 1 | is_floor(r + 1, c) << 2 | is_floor(r, c - 1) << 3
                    tile = tiles[f"wall_cap_{mask}"]
                elif ch == "D":
                    tile = tiles["door"]
                    later.append((tiles["spill_door"], c * SIZE - 8, (r + 1) * SIZE))
                elif ch == ",":
                    tile = tiles["grate_" + panel_pick(c, r, 2, GRATE_WEIGHTS)]
                elif ch == "=":
                    tile = tiles["hazard"]
                elif ch == "v":
                    tile = tiles["vent"]
                elif ch == "L":
                    tile = tiles[f"lift_{(plan[r - 1][c] == 'L') * 2 + (plan[r][c - 1] == 'L')}"]
                else:
                    tile = tiles["floor_" + panel_pick(c, r, 1, FLOOR_WEIGHTS, FLOOR_ACCENT)]
                blend(img, tile, c * SIZE, r * SIZE)
                if ch == "." and tile_hash(c, r, 4) % 100 < DECAL_CHANCE:
                    blend(img, shared["decal_" + DECALS[tile_pick(c, r, 5, DECAL_WEIGHTS[deck])]], c * SIZE, r * SIZE)
                if family(ch):
                    for side, (dr, dc) in zip("nesw", [(-1, 0), (0, 1), (1, 0), (0, -1)]):
                        if is_floor(r + dr, c + dc) and family(plan[r + dr][c + dc]) != family(ch):
                            blend(img, shared[f"lip_{side}"], c * SIZE, r * SIZE)
                if is_floor(r, c):
                    for key, (dr, dc) in [("shade_n", (-1, 0)), ("shade_w", (0, -1)), ("shade_e", (0, 1))]:
                        if not is_floor(r + dr, c + dc):
                            blend(img, shared[key], c * SIZE, r * SIZE)
        for spill_px, x, y in later:
            blend(img, spill_px, x, y)
        out.append(img)
    return out

# --------------------------------------------------------------------------
# The vault keeper and its gate. The keeper is drawn in near-neutral steel so
# the game can tint it by the vault's tier; only its eye and core are coloured.
# --------------------------------------------------------------------------

CLEAR = (0, 0, 0, 0)
KEEPER_METAL = rgb(150, 156, 172)
KEEPER_EYE = rgb(235, 70, 255)
KEEPER_CORE = rgb(255, 120, 255)


def canvas(w, h):
    return [[CLEAR for _ in range(w)] for _ in range(h)]


def outlined(px):
    """Rings every opaque pixel's empty neighbours with the outline colour."""
    h, w = len(px), len(px[0])
    out = [row[:] for row in px]
    for y in range(h):
        for x in range(w):
            if px[y][x][3]:
                continue
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and 0 <= ny < h and px[ny][nx][3] and px[ny][nx] != OUTLINE:
                    out[y][x] = OUTLINE
                    break
    return out


def fill_ellipse(px, cx, cy, rx, ry, color_at):
    for y in range(len(px)):
        for x in range(len(px[0])):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                c = color_at(x, y, d)
                if c:
                    px[y][x] = c


def metal_at(base, lx=-0.6, ly=-0.8, cx=0.0, cy=0.0, rx=1.0, ry=1.0, seed=0):
    """Shades a rounded surface lit from the top left, with a little grain."""
    def at(x, y, d):
        nx, ny = (x - cx) / rx, (y - cy) / ry
        light = -(nx * lx + ny * ly)
        rim = -34 if d > 0.82 else 0
        return shade(base, light * 34 + rim + (noise(x, y, seed) - 0.5) * 8)
    return at


def stamp(px, part):
    """Lays an outlined `part` over `px`, so each piece reads on its own."""
    for y, row in enumerate(outlined(part)):
        for x, c in enumerate(row):
            if c[3]:
                px[y][x] = c


def keeper_body():
    """The vault keeper, 40x40: a horned, domed guardian with a single great
    eye, heavy pauldrons and a vault dial in its chest, hovering on a thruster.
    It has no arms: its two hands float free (see `KEEPER_HAND`)."""
    n = 40
    px = canvas(n, n)
    m = KEEPER_METAL

    # Thruster skirt and its glow, under everything.
    part = canvas(n, n)
    for y in range(30, 37):
        half = 8 - (y - 30)
        for x in range(20 - half, 20 + half):
            part[y][x] = shade(m, -50 - (y - 30) * 6 + (16 if x < 20 else 0))
    for x in range(14, 26, 3):
        for y in range(31, 34):
            if part[y][x][3]:
                part[y][x] = shade(m, -95)
    for x in range(16, 24):
        part[36][x] = KEEPER_CORE if 17 < x < 22 else shade(KEEPER_CORE, -90)
    stamp(px, part)
    px[38][19] = px[38][20] = shade(KEEPER_CORE, -60)

    # Torso: a broad barrel, darkening toward its underside.
    part = canvas(n, n)
    lit = metal_at(m, cx=19.5, cy=22.0, rx=11.0, ry=9.5, seed=71)
    fill_ellipse(part, 19.5, 22.0, 11.0, 9.5, lambda x, y, d: shade(lit(x, y, d), -max(0, y - 22) * 4))
    for x in range(9, 31):
        if part[27][x][3]:
            part[27][x] = shade(part[27][x], -34)
    for y in range(13, 31):
        for x in (13, 26):
            if part[y][x][3]:
                part[y][x] = shade(part[y][x], -26)
    for x, y in ((11, 24), (28, 24), (15, 29), (24, 29)):
        rivet(part, x, y)
    # Chest: the vault dial, a ring of spokes around a glowing core.
    dark = shade(m, -80)
    fill_ellipse(part, 19.5, 21.5, 4.6, 4.6, lambda x, y, d: dark if d > 0.6 else shade(m, -30))
    for x, y in ((19, 17), (20, 17), (19, 26), (20, 26), (15, 21), (15, 22), (24, 21), (24, 22)):
        part[y][x] = shade(m, 36)
    fill_ellipse(part, 19.5, 21.5, 2.0, 2.0, lambda x, y, d: KEEPER_CORE if d > 0.3 else WHITE)
    stamp(px, part)

    # Pauldrons: heavy shells over where shoulders would be, amber-pipped.
    for cx in (7.0, 32.0):
        part = canvas(n, n)
        fill_ellipse(part, cx, 15.5, 6.5, 5.5, metal_at(shade(m, 12), cx=cx, cy=15.5, rx=6.5, ry=5.5, seed=72))
        for x in range(int(cx) - 5, int(cx) + 6):
            if part[18][x][3]:
                part[18][x] = shade(m, -56)
            if part[19][x][3]:
                part[19][x] = shade(m, -30)
        for dx in (-3, 0, 3):
            part[14][int(cx) + dx] = AMBER
            part[15][int(cx) + dx] = shade(AMBER, -110)
        stamp(px, part)

    # Horns sweeping up and out from the head.
    part = canvas(n, n)
    for side in (-1, 1):
        for i, (dx, dy) in enumerate(((5, 6), (6, 5), (7, 4), (8, 3), (8, 2), (9, 1))):
            x = int(19.5 + side * dx + (0.5 if side > 0 else -0.5))
            part[dy][x] = shade(m, 30 - i * 12)
            part[dy + 1][x] = shade(m, -30 - i * 6)
    stamp(px, part)

    # Head: a dome sunk between the pauldrons, its visor a scowling V.
    part = canvas(n, n)
    fill_ellipse(part, 19.5, 9.5, 7.0, 6.5, metal_at(shade(m, 6), cx=19.5, cy=9.5, rx=7.0, ry=6.5, seed=73))
    for x in range(13, 27):
        brow = abs(x - 19.5) // 3
        for y in range(int(8 + 2 - brow), int(12 - brow)):
            if part[y][x][3]:
                part[y][x] = rgb(22, 18, 30)
    for x in range(16, 24):
        part[10][x] = KEEPER_EYE if 16 < x < 23 else shade(KEEPER_EYE, -80)
    part[9][18] = part[9][19] = part[9][20] = part[9][21] = shade(KEEPER_EYE, -50)
    part[11][19] = part[11][20] = shade(KEEPER_EYE, -90)
    part[10][19] = part[10][20] = WHITE
    for y in range(3, 8):
        part[y][19] = shade(m, 44)
        part[y][20] = shade(m, -24)
    stamp(px, part)
    return px


# One of the keeper's floating hands, drawn as the right hand with its palm
# facing right (the game turns it to its aim and mirrors it for the left): an
# armoured cuff, the knuckle plate, and two great claws curled round a palm
# emitter.
KEEPER_HAND = (
    [
        "................",
        "......kkkk......",
        ".....kHHHHkk....",
        "....kHMMMMHWk...",
        "...kHMMMMmkkWk..",
        ".kkkMHMMmk..kWk.",
        "kCHkMMMMk.kk.k..",
        "kCMkMMMmkkPPk...",
        "kCMkMMMmkPwPPk..",
        "kCmkMMMmkkPPk...",
        "kCmkMMMMk.kk.k..",
        ".kkkMHMMmk..kWk.",
        "...kMMMMMmkkWk..",
        "....kmMMMMmWk...",
        ".....kmmmmkk....",
        "......kkkk......",
    ],
    {
        "H": shade(KEEPER_METAL, 40),
        "M": KEEPER_METAL,
        "m": shade(KEEPER_METAL, -50),
        "W": rgb(232, 236, 248),
        "C": shade(KEEPER_CORE, -70),
        "P": KEEPER_CORE,
    },
)


# The vault stalker: the deck's stalker grown monstrous, drawn like the keeper
# in near-neutral chitin and bone so the game can tint it by the vault's tier;
# only its eyes and the crack in its gut glow.
STALKER_CHITIN = rgb(150, 144, 164)
STALKER_BONE = rgb(222, 214, 196)
STALKER_EYE = rgb(140, 255, 120)


def thick_curve(px, points, r0, r1, color_at, steps=80):
    """Sweeps a disc shrinking from `r0` to `r1` along a quadratic bezier."""
    (ax, ay), (bx, by), (cx, cy) = points
    for i in range(steps + 1):
        t = i / steps
        x = (1 - t) ** 2 * ax + 2 * (1 - t) * t * bx + t * t * cx
        y = (1 - t) ** 2 * ay + 2 * (1 - t) * t * by + t * t * cy
        r = r0 + (r1 - r0) * t
        for py in range(int(y - r) - 1, int(y + r) + 2):
            for qx in range(int(x - r) - 1, int(x + r) + 2):
                if 0 <= qx < len(px[0]) and 0 <= py < len(px) and (qx - x) ** 2 + (py - y) ** 2 <= r * r:
                    px[py][qx] = color_at(qx, py, t, (qx - x) / max(r, 0.5))


def stalker_boss():
    """The vault stalker, 40x40: the deck's stalker grown monstrous. A hunched
    slab of armoured muscle with plated shoulders, a ridged head, four legs and
    two great bone scythes, its eyes and the crack down its gut glowing."""
    n = 40
    px = canvas(n, n)
    m = STALKER_CHITIN
    bone = STALKER_BONE

    # Four thick legs splayed under the body: hip, knee, foot.
    for side in (-1, 1):
        for hip, knee, foot in (((4, 27), (9, 32), (8, 38)), ((7, 25), (14, 30), (14, 37))):
            part = canvas(n, n)
            pts = [(19.5 + side * hip[0], hip[1]), (19.5 + side * knee[0], knee[1]), (19.5 + side * foot[0], foot[1])]
            thick_curve(part, pts, 2.2, 1.0, lambda x, y, t, s: shade(m, -40 - t * 30 - s * side * 14))
            stamp(px, part)

    # Torso and gut: a heavy barrel, plated in bands like the stalker's belly.
    part = canvas(n, n)
    lit = metal_at(m, cx=19.5, cy=23.0, rx=9.0, ry=9.0, seed=81)
    fill_ellipse(part, 19.5, 23.0, 9.0, 9.0, lambda x, y, d: shade(lit(x, y, d), -max(0, y - 23) * 3))
    for y in (21, 25, 29):
        for x in range(10, 30):
            if part[y][x][3]:
                part[y][x] = shade(part[y][x], -46)
    for x in range(10, 30):
        if part[17][x][3]:
            part[17][x] = shade(part[17][x], -30)
    # A glowing crack down the middle of the gut.
    for y, x in ((19, 20), (20, 19), (21, 19), (22, 20), (23, 20), (24, 19), (26, 19), (27, 20)):
        part[y][x] = STALKER_EYE if y % 3 else shade(STALKER_EYE, -70)
    stamp(px, part)

    # Scythe arms: a thick forearm off each shoulder, sweeping out and down
    # into a long hooked blade of bone.
    for side in (-1, 1):
        part = canvas(n, n)
        pts = [(19.5 + side * 11, 15), (19.5 + side * 19, 18), (19.5 + side * 16, 25)]
        thick_curve(part, pts, 3.2, 2.2, lambda x, y, t, s: shade(m, -10 - t * 30 - s * side * 18))
        stamp(px, part)
        part = canvas(n, n)
        pts = [(19.5 + side * 16, 24), (19.5 + side * 20, 32), (19.5 + side * 11, 38)]
        thick_curve(part, pts, 2.2, 0.4, lambda x, y, t, s: shade(bone, 20 - t * 40 - s * side * 30))
        stamp(px, part)
        # Spikes off the elbow.
        part = canvas(n, n)
        for i, (dx, dy) in enumerate(((17, 15), (18, 14), (18, 13))):
            part[dy][int(19.5 + side * dx + (0.5 if side > 0 else -0.5))] = shade(bone, -i * 20)
        stamp(px, part)

    # Shoulders: two great humps of muscle under armour plates, spiked.
    for side in (-1, 1):
        cx = 19.5 + side * 9.5
        part = canvas(n, n)
        fill_ellipse(part, cx, 14.0, 6.5, 5.5, metal_at(shade(m, 10), cx=cx, cy=14.0, rx=6.5, ry=5.5, seed=82))
        for x in range(int(cx) - 6, int(cx) + 7):
            if 0 <= x < n and part[16][x][3]:
                part[16][x] = shade(m, -50)
        for i, (dx, dy) in enumerate(((2, 9), (3, 8), (4, 7), (5, 6))):
            part[dy][int(cx + side * dx)] = shade(bone, -i * 18)
        # Overlapping plates, their edges lit.
        for x in range(int(cx) - 4, int(cx) + 5):
            y = 12 + (abs(x - cx) > 3)
            if part[y][x][3]:
                part[y][x] = shade(m, -44)
                part[y - 1][x] = shade(m, 36)
        stamp(px, part)

    # Head: low and sunk between the shoulders, ridged on top, with two great
    # eyes and two small ones over its mandibles.
    part = canvas(n, n)
    fill_ellipse(part, 19.5, 9.5, 5.5, 5.0, metal_at(shade(m, 4), cx=19.5, cy=9.5, rx=5.5, ry=5.0, seed=83))
    for y in range(4, 10):
        part[y][19] = shade(m, 40)
        part[y][20] = shade(m, -30)
    # A scowling brow over each great eye.
    for x, y in ((15, 7), (16, 7), (17, 8), (18, 8), (24, 7), (23, 7), (22, 8), (21, 8)):
        part[y][x] = rgb(22, 18, 30)
    for x, y in ((16, 8), (16, 9), (17, 9), (23, 8), (23, 9), (22, 9)):
        part[y][x] = STALKER_EYE
    part[9][17] = part[9][22] = WHITE
    part[10][16] = part[10][17] = part[10][22] = part[10][23] = shade(STALKER_EYE, -90)
    part[11][18] = part[11][21] = shade(STALKER_EYE, -30)
    for x in range(16, 24):
        part[13][x] = rgb(22, 18, 30)
    stamp(px, part)
    # Mandibles.
    part = canvas(n, n)
    for side in (-1, 1):
        for i, (dx, dy) in enumerate(((2, 14), (3, 15), (3, 16), (2, 17))):
            part[dy][int(19.5 + side * dx + (0.5 if side > 0 else -0.5))] = shade(bone, -i * 20)
    stamp(px, part)
    return px


# The vault brute: the deck's brute grown huge and plated, drawn like the
# keeper in near-neutral hide and steel so the game can tint it by the vault's
# tier; only its visor and the studs on its belt glow.
BRUTE_HIDE = rgb(168, 150, 146)
BRUTE_PLATE = rgb(150, 156, 172)
BRUTE_EYE = rgb(255, 120, 40)


BRUTE_DARK = rgb(22, 18, 30)
# Each frame of the vault brute's sheet, and where its feet stand.
BRUTE_W, BRUTE_H = 56, 52
BRUTE_CX = 27.5
BRUTE_GROUND = 49


def brute_disc(px, cx, cy, rx, ry, color_at):
    for y in range(max(0, int(cy - ry) - 1), min(len(px), int(cy + ry) + 2)):
        for x in range(max(0, int(cx - rx) - 1), min(len(px[0]), int(cx + rx) + 2)):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                c = color_at(x, y, d)
                if c:
                    px[y][x] = c


def brute_limb(px, a, b, r0, r1, color_at):
    """A tapered capsule from `a` to `b`."""
    (ax, ay), (bx, by) = a, b
    steps = max(8, int(math.hypot(bx - ax, by - ay) * 3))
    for i in range(steps + 1):
        t = i / steps
        x, y = ax + (bx - ax) * t, ay + (by - ay) * t
        r = r0 + (r1 - r0) * t
        for py in range(int(y - r) - 1, int(y + r) + 2):
            for qx in range(int(x - r) - 1, int(x + r) + 2):
                if 0 <= qx < len(px[0]) and 0 <= py < len(px) and (qx - x) ** 2 + (py - y) ** 2 <= r * r:
                    px[py][qx] = color_at(qx, py, t, (qx - x) / max(r, 0.5), (py - y) / max(r, 0.5))


def brute_hide_at(base, seed):
    def at(x, y, t, sx, sy):
        light = -(sx * -0.6 + sy * -0.8)
        rim = -30 if sx * sx + sy * sy > 0.7 else 0
        return shade(base, light * 30 + rim + (noise(x, y, seed) - 0.5) * 10)
    return at


def brute_pose(name, by=0, sway=0, head=(0, 0), jaw=0, arms=None, feet=((0, 0), (0, 0)), furnace=0.6, eyes=1.0, hunch=0):
    return dict(name=name, by=by, sway=sway, head=head, jaw=jaw, arms=arms, feet=feet, furnace=furnace, eyes=eyes, hunch=hunch)


# Each arm: (elbow, fist) offsets from the frame's centre line, for the left
# (-1) side; the right is mirrored unless given. Fist "flat" ones rest on knuckles.
def brute_arms(le, lf, re=None, rf=None):
    re = re or (-le[0], le[1])
    rf = rf or (-lf[0], lf[1])
    return {-1: (le, lf), 1: (re, rf)}


BRUTE_HANG = brute_arms((-17, 27), (-16, 41))
BRUTE_POSES = [
    brute_pose("idle_a", arms=BRUTE_HANG),
    brute_pose("idle_b", by=1, arms=brute_arms((-17, 28), (-16, 41)), furnace=0.7),
    # A knuckle-dragging stomp: one side lifts, the other bears down.
    brute_pose("walk_a", by=-1, sway=-1, arms=brute_arms((-17, 28), (-15, 43), (17, 25), (17, 36)), feet=((0, 0), (1, 3))),
    brute_pose("walk_b", by=1, arms=brute_arms((-17, 28), (-16, 42)), feet=((0, 0), (0, 0))),
    brute_pose("walk_c", by=-1, sway=1, arms=brute_arms((-17, 25), (-17, 36), (17, 28), (15, 43)), feet=((-1, 3), (0, 0))),
    brute_pose("walk_d", by=1, arms=brute_arms((-17, 28), (-16, 42)), feet=((0, 0), (0, 0))),
    # Rearing up, fists climbing past its head, then locked together overhead.
    brute_pose("raise", by=-2, head=(0, 0), arms=brute_arms((-19, 17), (-14, 11)), furnace=0.6, eyes=1.0),
    brute_pose("overhead", by=-3, head=(0, 1), arms=brute_arms((-16, 14), (-2.5, 9)), furnace=0.75, eyes=1.0),
    # The slam: folded forward over fists driven into the floor.
    brute_pose("slam", by=4, head=(0, 0), arms=brute_arms((-13, 32), (-5, 44)), feet=((-2, 0), (2, 0)), furnace=0.9, hunch=2),
    # Spent: hunched over its knuckles, heaving.
    brute_pose("spent_a", by=3, head=(0, 1), arms=brute_arms((-18, 32), (-15, 44)), feet=((-1, 0), (1, 0)), furnace=0.15, eyes=0.45, hunch=2),
    brute_pose("spent_b", by=4, head=(0, 2), arms=brute_arms((-18, 33), (-15, 44)), feet=((-1, 0), (1, 0)), furnace=0.1, eyes=0.35, hunch=2),
    # Roaring: head thrown back, jaw wide, arms flung out, chest blazing.
    brute_pose("roar_a", by=-1, head=(0, -2), jaw=1, arms=brute_arms((-20, 20), (-23, 28)), furnace=0.9),
    brute_pose("roar_b", by=-2, head=(0, -3), jaw=2, arms=brute_arms((-21, 14), (-23, 9)), furnace=1.0),
]


def brute_frame(p):
    """One frame of the vault brute: a hunched juggernaut, all shoulders and
    forearms, its small head sunk low between them behind a jaw grille, a
    furnace grate glowing in its chest and broken shackles on its wrists.
    Returns the body and its glow (eyes, furnace, maw) as separate layers."""
    px = canvas(BRUTE_W, BRUTE_H)
    glow = canvas(BRUTE_W, BRUTE_H)
    hide, plate = BRUTE_HIDE, BRUTE_PLATE
    by, sx = p["by"], p["sway"]
    cx = BRUTE_CX + sx
    hips_y = 33 + by * 0.6
    sh_y = 19 + by

    def emit(x, y, level):
        if 0 <= x < BRUTE_W and 0 <= y < BRUTE_H and level > 0:
            a = int(255 * max(0.0, min(1.0, level)))
            if a > glow[y][x][3]:
                glow[y][x] = (255, 255, 255, a)

    # Legs: short, bowed pillars in heavy plated boots.
    for i, side in enumerate((-1, 1)):
        fdx, lift = p["feet"][i]
        hip = (BRUTE_CX + side * 6, hips_y)
        foot = (BRUTE_CX + side * 8 + fdx, BRUTE_GROUND - 3 - lift)
        part = canvas(BRUTE_W, BRUTE_H)
        brute_limb(part, hip, (foot[0], foot[1] - 1), 4.2, 3.6, brute_hide_at(shade(hide, -28), 11 + i))
        # Knee plate.
        kx, ky = (hip[0] + foot[0]) / 2, (hip[1] + foot[1]) / 2
        brute_disc(part, kx, ky, 2.6, 2.2, lambda x, y, d: shade(plate, 20 - d * 40))
        brute_disc(part, foot[0], foot[1] + 0.5, 5.0, 2.8, lambda x, y, d: shade(plate, -6 - (y - foot[1]) * 14))
        for x in range(int(foot[0] - 4), int(foot[0] + 5)):
            y = int(foot[1] - 2)
            if 0 <= y < BRUTE_H and part[y][x][3]:
                part[y][x] = shade(plate, 36)
        stamp(px, part)

    # Torso: a huge hump of shoulders tapering to a heavy gut.
    part = canvas(BRUTE_W, BRUTE_H)
    lit = brute_hide_at(hide, 21)
    brute_disc(part, cx, hips_y - 4, 10.0, 7.0, lambda x, y, d: lit(x, y, 0, (x - cx) / 10, (y - hips_y + 4) / 7 - 0.3))
    brute_disc(part, cx, sh_y + 1 + p["hunch"], 14.0, 8.0, lambda x, y, d: lit(x, y, 0, (x - cx) / 14, (y - sh_y) / 8.0))
    # The hump of muscle its head sinks into.
    ty = sh_y - 4 + p["hunch"]
    brute_disc(part, cx, ty, 9.0, 4.5, lambda x, y, d: lit(x, y, 0, (x - cx) / 9, (y - ty) / 4.5 - 0.4))
    # Pecs either side of the furnace.
    for side in (-1, 1):
        for x in range(int(cx + side * 3), int(cx + side * 11), side):
            y = int(sh_y + 8 + p["hunch"] - abs(x - cx) * 0.25)
            if part[y][x][3]:
                part[y][x] = shade(part[y][x], -30)
    # Belt of riveted plate.
    for x in range(BRUTE_W):
        for y in (int(hips_y), int(hips_y) + 1):
            if part[y][x][3]:
                part[y][x] = shade(plate, -18 if y == int(hips_y) else -48)
    for x in range(int(cx - 8), int(cx + 9), 4):
        if part[int(hips_y)][x][3]:
            part[int(hips_y)][x] = shade(plate, 50)
    stamp(px, part)

    # The furnace: a riveted grate in its chest, slits glowing through.
    part = canvas(BRUTE_W, BRUTE_H)
    gy = sh_y + 6 + p["hunch"]
    fur = metal_at(shade(plate, 4), cx=cx, cy=gy, rx=7.0, ry=5.0, seed=31)
    brute_disc(part, cx, gy, 7.0, 5.0, fur)
    for k, dy in enumerate((-2, 0, 2)):
        half = 4 - (k != 1)
        for x in range(int(cx - half), int(cx + half + 1)):
            y = int(gy + dy)
            part[y][x] = BRUTE_DARK
            emit(x, y, p["furnace"] * (0.75 + 0.25 * (abs(x - cx) < 2)))
    for x, y in ((cx - 5.5, gy - 2), (cx + 5.5, gy - 2), (cx - 5.5, gy + 2), (cx + 5.5, gy + 2)):
        part[int(y)][int(x)] = shade(plate, 50)
    stamp(px, part)

    # Head: small and low, pushed forward between the shoulders, a heavy
    # brow over two burning eyes and a grilled jaw that drops to roar.
    hx, hy = cx + p["head"][0], sh_y - 4 + p["head"][1] + p["hunch"]
    part = canvas(BRUTE_W, BRUTE_H)
    helm = metal_at(shade(plate, -4), cx=hx, cy=hy, rx=5.5, ry=5.0, seed=41)
    brute_disc(part, hx, hy, 5.5, 5.0, helm)
    jaw = p["jaw"]
    # Jaw grille below, hanging open by `jaw`.
    jy = hy + 3 + jaw
    brute_disc(part, hx, jy, 4.5, 2.6, lambda x, y, d: shade(plate, -20 - d * 20))
    for x in range(int(hx - 3), int(hx + 4)):
        if (x - int(hx)) % 2 == 0:
            part[int(jy)][x] = shade(plate, -70)
    if jaw:
        for y in range(int(hy + 2), int(jy)):
            for x in range(int(hx - 3), int(hx + 4)):
                part[y][x] = BRUTE_DARK
                if abs(x - hx) < 2.5:
                    emit(x, y, 0.35 + 0.15 * jaw)
        # Teeth along the gap.
        for x in range(int(hx - 3), int(hx + 4), 2):
            part[int(hy + 2)][x] = shade(WHITE, -40)
            part[int(jy) - 1][x + 1] = shade(WHITE, -70)
    # Brow and eyes.
    ey = int(hy)
    for x in range(int(hx - 4), int(hx + 5)):
        part[ey - 1][x] = shade(plate, -64)
    for x in range(int(hx - 4), int(hx + 5)):
        part[ey][x] = BRUTE_DARK
    for side in (-1, 1):
        ex = int(hx + side * 2 - (side < 0))
        for x in (ex, ex + 1):
            part[ey][x] = BRUTE_EYE
            emit(x, ey, p["eyes"])
        emit(ex + (side > 0), ey - 1, p["eyes"] * 0.5)
    # Crest.
    for y in range(int(hy - 5), int(hy - 1)):
        part[y][int(hx)] = shade(plate, 44)
        part[y][int(hx) + 1] = shade(plate, -24)
    stamp(px, part)

    # Arms, then the pauldrons over their tops, then the fists in front of all.
    fists = []
    overhead = False
    for side in (-1, 1):
        (ex, ey2), (fx, fy) = p["arms"][side]
        sh = (cx + side * 12, sh_y + 1 + p["hunch"])
        el = (BRUTE_CX + ex, ey2 + by * 0.5)
        fi = (BRUTE_CX + fx, fy + (by * 0.5 if fy < 40 else 0))
        overhead |= fi[1] < hy
        part = canvas(BRUTE_W, BRUTE_H)
        brute_limb(part, sh, el, 4.0, 3.4, brute_hide_at(shade(hide, -6), 51 + side))
        # Forearms like barrels, swelling toward the wrist.
        brute_limb(part, el, fi, 3.8, 4.8, brute_hide_at(shade(hide, 4), 61 + side))
        stamp(px, part)
        fists.append((side, el, fi))

    for side in (-1, 1):
        pcx = cx + side * 13
        pcy = sh_y + p["hunch"]
        part = canvas(BRUTE_W, BRUTE_H)
        brute_disc(part, pcx, pcy, 6.0, 4.4, metal_at(shade(plate, 10), cx=pcx, cy=pcy, rx=6.0, ry=4.4, seed=71))
        # Banded: two overlapping plates.
        for x in range(int(pcx) - 6, int(pcx) + 7):
            for y, amount in ((int(pcy + 1), -56), (int(pcy + 2), -20)):
                if 0 <= x < BRUTE_W and part[y][x][3]:
                    part[y][x] = shade(plate, amount)
        # A row of rivets and two horn-spikes off the top.
        for k in (-3, 3):
            x, y = int(pcx + k), int(pcy - 1)
            part[y][x] = shade(plate, 60)
        # One stubby horn-spike off the outer edge.
        x = int(pcx + side * 3)
        for j in range(3):
            y = int(pcy - 4 - j)
            if 0 <= y < BRUTE_H and 0 <= x < BRUTE_W:
                part[y][x] = shade(plate, 44 - j * 20)
        stamp(px, part)

    for side, el, fi in fists:
        part = canvas(BRUTE_W, BRUTE_H)
        fx, fy = fi
        # Shackle cuff with a broken chain.
        wx, wy = (el[0] + fx * 2) / 3, (el[1] + fy * 2) / 3
        brute_disc(part, wx, wy, 4.4, 2.2, lambda x, y, d: shade(plate, 40 - (y - wy) * 20))
        fist = metal_at(shade(plate, 10), cx=fx, cy=fy, rx=5.2, ry=4.6, seed=81 + side)
        brute_disc(part, fx, fy, 5.2, 4.6, fist)
        # Knuckle plates across the bottom.
        ky = int(fy + 2)
        for x in range(int(fx - 4), int(fx + 5)):
            if 0 <= ky < BRUTE_H and part[ky][x][3]:
                part[ky][x] = shade(plate, -62)
        for dx in (-3, -1, 1, 3):
            x, y = int(fx + dx), int(fy + 1)
            if 0 <= y < BRUTE_H:
                part[y][x] = shade(plate, 56)
        stamp(px, part)
        # Chain links dangling from the cuff (none when overhead).
        if fy > wy - 1 and not overhead:
            for k in range(2):
                lx, ly = int(wx - side * (3 + k)), int(wy + 2 + k * 2)
                if 0 <= ly < BRUTE_H and 0 <= lx < BRUTE_W and not px[ly][lx][3]:
                    px[ly][lx] = shade(plate, 30 - k * 20)

    return px, glow


def brute_boss():
    """The vault brute's animation sheet, one row of `BRUTE_POSES` frames,
    and its glow: the same frames holding only what burns (eyes, furnace,
    maw), for the game to colour by mood. Also a square portrait, the first
    frame, for the menus."""
    frames = [brute_frame(p) for p in BRUTE_POSES]
    sheet = canvas(BRUTE_W * len(frames), BRUTE_H)
    glow = canvas(BRUTE_W * len(frames), BRUTE_H)
    for i, (body, light) in enumerate(frames):
        for y in range(BRUTE_H):
            for x in range(BRUTE_W):
                sheet[y][i * BRUTE_W + x] = body[y][x]
                glow[y][i * BRUTE_W + x] = light[y][x]
    pad = (BRUTE_W - BRUTE_H) // 2
    portrait = [[CLEAR] * BRUTE_W for _ in range(pad)] + [row[:] for row in frames[0][0]] + [[CLEAR] * BRUTE_W for _ in range(BRUTE_W - BRUTE_H - pad)]
    return portrait, sheet, glow, [f[0] for f in frames]


# The vault prism: a great faceted crystal sentinel, drawn like the keeper in
# near-white glass and steel so the game can tint it by the vault's tier, with
# a floating pylon at each flank. Its body comes in two halves that part to
# bare its heart, a separate glowing layer the game colours by phase; and the
# overlays it raises: the aegis bubble and the mirror panes that orbit it.
PRISM_GLASS = rgb(196, 226, 240)
PRISM_N = 44
# The seam between its halves runs between columns 21 and 22.
PRISM_CX = 21.5
# Its heart, behind the window in its middle.
PRISM_HEART = (21.5, 21.0)


def inside(x, y, points):
    """Whether `(x, y)` lies inside the polygon through `points`."""
    hit = False
    for (ax, ay), (bx, by) in zip(points, points[1:] + points[:1]):
        if (ay > y) != (by > y) and x < ax + (y - ay) * (bx - ax) / (by - ay):
            hit = not hit
    return hit


def facets(px, faces, seed, ids=None):
    """Fills each `(points, light)` face, lit by `light`, brighter toward the
    top, with a little grain; `ids` records which face each pixel took."""
    for i, (points, light) in enumerate(faces):
        for y in range(len(px)):
            for x in range(len(px[0])):
                if inside(x, y, points):
                    px[y][x] = shade(PRISM_GLASS, light - y * 0.8 + 14 + (noise(x, y, seed) - 0.5) * 7)
                    if ids is not None:
                        ids[y][x] = i


def crystal_edges(px, ids):
    """Catches the light along the ridges between faces: lit where a face
    meets the one to its left or above, shadowed on the far side."""
    h, w = len(px), len(px[0])
    for y in range(1, h):
        for x in range(1, w):
            here = ids[y][x]
            if here < 0:
                continue
            if ids[y][x - 1] not in (-1, here) or ids[y - 1][x] not in (-1, here):
                px[y][x] = shade(px[y][x], 34 if x < PRISM_CX else -22)


def prism_boss():
    """The vault prism, 44x44, and its parts: the body's left and right
    halves (each the full canvas, empty past the seam) with the window its
    heart shows through, the heart layer, and the composite for the menus,
    halves closed; also the composite with its halves parted, for the preview."""
    n = PRISM_N
    cx = PRISM_CX
    body = canvas(n, n)

    # The great crystal: a tall six-sided gem, a crown over a girdle band over
    # a long pavilion, each split into four faces lit from the top left.
    T, B = (cx, 1.0), (cx, 42.5)
    ul, ur, ll, lr = (11.0, 13.0), (32.0, 13.0), (12.0, 29.0), (31.0, 29.0)
    il, ir, jl, jr = (16.5, 13.0), (26.5, 13.0), (17.0, 29.0), (26.0, 29.0)
    mt, mb = (cx, 13.0), (cx, 29.0)
    faces = [
        ([T, ul, il], 40), ([T, il, mt], 56), ([T, mt, ir], 6), ([T, ir, ur], -22),
        ([ul, il, jl, ll], 16), ([il, mt, mb, jl], 28), ([mt, ir, jr, mb], -6), ([ir, ur, lr, jr], -36),
        ([ll, jl, B], -8), ([jl, mb, B], 4), ([mb, jr, B], -30), ([jr, lr, B], -58),
    ]
    part = canvas(n, n)
    ids = [[-1] * n for _ in range(n)]
    facets(part, faces, 91, ids)
    crystal_edges(part, ids)
    # A few glints on the lit faces.
    for x, y in ((18, 6), (19, 7), (14, 17), (13, 18), (18, 33), (24, 9)):
        part[y][x] = WHITE
    stamp(body, part)

    # Steel bands strapped round the girdle, top and bottom, capped past its
    # edges and studded with glowing pips: the vault's leash on it.
    steel = KEEPER_METAL
    for (gl, gr), y0 in (((ul, ur), 12), ((ll, lr), 28)):
        part = canvas(n, n)
        for y in (y0, y0 + 1):
            for x in range(int(gl[0]) - 1, int(gr[0]) + 2):
                part[y][x] = shade(steel, (14 if y == y0 else -30) + (noise(x, y, 93) - 0.5) * 8 - (x - cx) * 0.8)
        for x in (int(gl[0]) + 2, int(cx) - 4, int(cx) + 5, int(gr[0]) - 2):
            part[y0][x] = rgb(150, 235, 255)
            part[y0 + 1][x] = shade(rgb(150, 235, 255), -90)
        stamp(body, part)

    # The window its heart shows through: a dark glass socket ringed by a
    # bright bevel, cut after the outline so the heart fills it.
    hx, hy = PRISM_HEART
    for y in range(n):
        for x in range(n):
            d = ((x - hx) / 4.0) ** 2 + ((y - hy) / 6.4) ** 2
            if d <= 1.0:
                body[y][x] = CLEAR
            elif d <= 1.45:
                body[y][x] = rgb(46, 70, 92)
            elif d <= 2.1 and body[y][x][3]:
                body[y][x] = shade(body[y][x], 36 if x < cx else 8)

    # A floating pylon at each flank: a slim gem with a spark of light inside.
    for side in (-1, 1):
        px0 = cx + side * 17.0
        top, bottom, mid = (px0, 9.0), (px0, 35.0), 20.0
        left, right = (px0 - 3.8, mid), (px0 + 3.8, mid)
        centre = (px0, mid)
        part = canvas(n, n)
        ids = [[-1] * n for _ in range(n)]
        facets(part, [([top, left, centre], 34), ([top, centre, right], -8), ([left, bottom, centre], 0), ([centre, bottom, right], -40)], 92 + side, ids)
        crystal_edges(part, ids)
        x = int(px0 - 0.5)
        for y in range(17, 24):
            part[y][x] = rgb(150, 235, 255) if 18 < y < 22 else shade(rgb(150, 235, 255), -50)
        part[19][x] = part[20][x] = WHITE
        stamp(body, part)

    # The ridge down its middle, where the halves meet.
    for y in range(n):
        if body[y][21][3] and body[y][21] != OUTLINE:
            body[y][21] = shade(body[y][21], -16)
        if body[y][22][3] and body[y][22] != OUTLINE:
            body[y][22] = shade(body[y][22], 18)
    left = [[c if x < 22 else CLEAR for x, c in enumerate(row)] for row in body]
    right = [[c if x >= 22 else CLEAR for x, c in enumerate(row)] for row in body]

    # Its heart: a white-hot gem in a soft halo, and a thread of light up the
    # seam that shows when the halves part.
    heart = canvas(n, n)
    for y in range(n):
        for x in range(n):
            d = ((x - hx) / 4.6) ** 2 + ((y - hy) / 7.0) ** 2
            halo = ((x - hx) / 8.0) ** 2 + ((y - hy) / 12.0) ** 2
            if d < 0.35:
                heart[y][x] = WHITE
            elif d < 0.7:
                heart[y][x] = rgb(214, 248, 255)
            elif d <= 1.0:
                heart[y][x] = rgb(140, 222, 255)
            elif halo <= 1.0:
                heart[y][x] = rgb(140, 222, 255, int(110 * (1.0 - halo) + 30))
            elif 3 <= y <= 40 and x in (21, 22):
                heart[y][x] = rgb(200, 240, 255, 150)
            elif 4 <= y <= 39 and x in (20, 23):
                heart[y][x] = rgb(140, 222, 255, 60)

    def compose(gap):
        out = [row[:] for row in heart]
        for half, dx in ((left, -gap), (right, gap)):
            for y in range(n):
                for x in range(n):
                    c = half[y][x]
                    if c[3] and 0 <= x + dx < n:
                        out[y][x + dx] = c
        return out

    return compose(0), left, right, heart, compose(3)


def prism_aegis():
    """The bubble a negating prism raises, 56x64, white for the game to tint:
    a pointy-topped hexagon with a bright rim, faint honeycomb inside,
    thickening toward the rim, and a glint at its top left."""
    w, h = 56, 64
    cx, cy = 27.5, 31.5
    r_in = 30.0 * math.sqrt(3) / 2
    cell = 6.0
    s3 = math.sqrt(3)

    def hex_norm(x, y):
        return max(abs(x), abs(x / 2 + y * s3 / 2), abs(x / 2 - y * s3 / 2))

    px = canvas(w, h)
    for y in range(h):
        for x in range(w):
            dx, dy = x - cx, y - cy
            hn = hex_norm(dx, dy)
            if hn > r_in:
                continue
            edge = hn / r_in
            # Light from the top left.
            lit = (-dx - dy) / (r_in * 2) * 0.5 + 0.5
            grey = int(190 + 65 * lit)
            if hn > r_in - 1.0:
                px[y][x] = (grey, grey, grey, 235)
                continue
            if hn > r_in - 2.0:
                px[y][x] = (grey, grey, grey, 150)
                continue
            # Honeycomb: how near this pixel is to its cell's edge.
            q = (s3 / 3 * dx - dy / 3) / cell
            r = (2 / 3 * dy) / cell
            xx, zz = q, r
            yy = -xx - zz
            rx, ry, rz = round(xx), round(yy), round(zz)
            ex, ey, ez = abs(rx - xx), abs(ry - yy), abs(rz - zz)
            if ex > ey and ex > ez:
                rx = -ry - rz
            elif ey > ez:
                ry = -rx - rz
            else:
                rz = -rx - ry
            ccx = cell * s3 * (rx + rz / 2)
            ccy = cell * 1.5 * rz
            lattice = hex_norm(dx - ccx, dy - ccy) > cell * s3 / 2 - 0.9
            a = 14 + 48 * edge ** 3
            if lattice:
                a += 30 + 50 * edge
            px[y][x] = (grey, grey, grey, int(min(255, a)))
    # A curved glint inside the top left of the rim.
    for t in range(14):
        ang = math.radians(200 + t * 4.5)
        x = int(round(cx + math.cos(ang) * (r_in - 5)))
        y = int(round(cy + math.sin(ang) * (r_in - 3)))
        px[y][x] = (255, 255, 255, 170 if 2 < t < 11 else 90)
    return px


def prism_shard():
    """One of the mirror panes that orbit a reflecting prism, 10x15: a shard
    of silvered glass with a glint across it, bright so the game can dim it
    between glints."""
    w, h = 10, 15
    pts = [(5.0, 0.5), (9.3, 5.0), (5.6, 14.4), (0.6, 9.0)]
    part = canvas(w, h)
    for y in range(h):
        for x in range(w):
            if inside(x, y, pts):
                k = (x * 0.4 + y * 0.6) / 11.0
                c = mix(rgb(240, 246, 255), rgb(140, 160, 196), min(1.0, k))
                if abs((x - y * 0.55) - 1.2) < 0.8:
                    c = WHITE
                elif x >= 6 and y <= 6:
                    c = mix(c, rgb(170, 230, 255), 0.5)
                elif x <= 3:
                    c = shade(c, -24)
                part[y][x] = c
    return outlined(part)


def gate_tiles():
    """The vault's gate, 32x16 across two wall tiles: a sealed bulkhead with
    hazard-striped leaves meeting at a locked seam, and the same frame once
    open, its leaves drawn back into the walls."""
    w, h = SIZE * 2, SIZE
    frame = rgb(46, 50, 64)
    leaf = rgb(78, 84, 102)
    sealed = [[shade(frame, (noise(x, y, 81) - 0.5) * 6) for x in range(w)] for y in range(h)]
    bevel(sealed, 0, 0, w - 1, h - 1, rgb(104, 114, 142), rgb(16, 18, 26))
    for y in range(2, h - 2):
        for x in range(2, w - 2):
            sealed[y][x] = shade(leaf, (noise(x, y, 82) - 0.5) * 6)
    # Hazard stripes along both edges of the leaves.
    for y in list(range(2, 5)) + list(range(h - 5, h - 2)):
        for x in range(2, w - 2):
            stripe = ((x + y) // 2) % 2 == 0
            sealed[y][x] = rgb(214, 170, 40) if stripe else rgb(34, 32, 30)
    bevel(sealed, 2, 2, 15, h - 3, shade(leaf, 26), shade(leaf, -30))
    bevel(sealed, 16, 2, w - 3, h - 3, shade(leaf, 26), shade(leaf, -30))
    # The seam and the lock housing over it.
    for y in range(2, h - 2):
        sealed[y][15] = rgb(14, 15, 20)
        sealed[y][16] = rgb(14, 15, 20)
    for y in range(5, 11):
        for x in range(12, 20):
            edge = x in (12, 19) or y in (5, 10)
            sealed[y][x] = rgb(24, 26, 34) if edge else rgb(56, 60, 76)
    for x, y in ((14, 7), (17, 7), (14, 8), (17, 8)):
        sealed[y][x] = rgb(255, 50, 60)
    sealed[7][15] = sealed[8][16] = rgb(120, 20, 30)
    sealed[7][16] = sealed[8][15] = rgb(120, 20, 30)
    # Heavy bolts on each leaf.
    for x in (5, 9, 22, 26):
        rivet(sealed, x, 7)

    opened = [[shade(rgb(26, 28, 38), (noise(x, y, 83) - 0.5) * 6) for x in range(w)] for y in range(h)]
    bevel(opened, 0, 0, w - 1, h - 1, rgb(104, 114, 142), rgb(16, 18, 26))
    for y in range(1, h - 1):
        for x in list(range(1, 4)) + list(range(w - 4, w - 1)):
            opened[y][x] = shade(leaf, -10 + (16 if x in (1, w - 4) else 0))
        # Floor runners the leaves slide along.
        opened[y][4] = opened[y][w - 5] = rgb(16, 18, 24)
    for x in range(4, w - 4):
        opened[1][x] = rgb(214, 170, 40) if (x // 2) % 2 == 0 else rgb(34, 32, 30)
        opened[h - 2][x] = rgb(214, 170, 40) if (x // 2) % 2 == 0 else rgb(34, 32, 30)
    for x in (2, w - 3):
        opened[7][x] = opened[8][x] = rgb(80, 255, 120)
    return sealed, opened


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
    engineer = from_ascii("player_engineer", PLAYER[0], ENGINEER_PALETTE)
    write_png(os.path.join(ROOT, "player_engineer.png"), engineer)
    bulwark = from_ascii("player_bulwark", PLAYER[0], BULWARK_PALETTE)
    write_png(os.path.join(ROOT, "player_bulwark.png"), bulwark)
    hacker = from_ascii("player_hacker", PLAYER[0], HACKER_PALETTE)
    write_png(os.path.join(ROOT, "player_hacker.png"), hacker)
    doctor = from_ascii("player_doctor", PLAYER[0], DOCTOR_PALETTE)
    write_png(os.path.join(ROOT, "player_doctor.png"), doctor)
    sentry = from_ascii("sentry", SENTRY, SENTRY_METAL)
    write_png(os.path.join(ROOT, "sentry.png"), sentry)
    sentry_gun = from_ascii("sentry_gun", SENTRY_GUN, SENTRY_METAL, width=12, height=6)
    write_png(os.path.join(ROOT, "sentry_gun.png"), sentry_gun)
    mine = from_ascii("mine", MINE, MINE_METAL, width=12, height=12)
    write_png(os.path.join(ROOT, "mine.png"), mine)
    mine_light = from_ascii("mine_light", MINE_LIGHT, MINE_GLOW, width=6, height=6)
    write_png(os.path.join(ROOT, "mine_light.png"), mine_light)

    sets = []
    for deck, theme in enumerate(DECK_THEMES):
        tiles = deck_tiles(theme, 1 + deck)
        sets.append(tiles)
        for name, px in tiles.items():
            write_png(os.path.join(ROOT, "tiles", f"deck_{deck}", f"{name}.png"), px)
    shared = floor_shadows()
    shared.update(floor_lips())
    shared.update({f"decal_{name}": px for name, px in floor_decals().items()})
    for name, px in shared.items():
        write_png(os.path.join(ROOT, "tiles", f"{name}.png"), px)

    write_png(os.path.join(ROOT, "shadow.png"), shadow())
    write_png(os.path.join(ROOT, "vault.png"), vault_floor())
    write_png(os.path.join(ROOT, "vault_bar.png"), from_ascii("vault_bar", *VAULT_BAR, width=6, height=5))
    write_png(os.path.join(ROOT, "vault_bar_plain.png"), from_ascii("vault_bar_plain", *VAULT_BAR_PLAIN, width=6, height=5))

    guns = {}
    for name, (rows, pal) in GUNS.items():
        px = from_ascii(name, rows, pal, height=8)
        guns[name] = px
        write_png(os.path.join(ROOT, "guns", f"{name}.png"), px)

    PROJECTILES["explosion"] = explosion()
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

    drops = {}
    for name, (rows, pal) in DROPS.items():
        px = from_ascii(name, rows, pal)
        drops[name] = px
        write_png(os.path.join(ROOT, "drops", f"{name}.png"), px)

    perks = {}
    for name in PERK_SYMBOLS:
        px = perk_badge(name)
        perks[name] = px
        write_png(os.path.join(ROOT, "perks", f"{name}.png"), px)
    write_png(os.path.join(ROOT, "fx", "shock_ring.png"), shock_ring())
    # Each drawn 1:1 in src/class.rs: at HACK_RADIUS and round the ward bubble.
    write_png(os.path.join(ROOT, "fx", "hack_ring.png"), aura_ring(40, fill=0.1))
    write_png(os.path.join(ROOT, "fx", "ward_ring.png"), aura_ring(11, fill=0.2))

    keeper = keeper_body()
    write_png(os.path.join(ROOT, "boss", "keeper.png"), keeper)
    hand = from_ascii("hand", *KEEPER_HAND)
    write_png(os.path.join(ROOT, "boss", "hand.png"), hand)
    stalker = stalker_boss()
    write_png(os.path.join(ROOT, "boss", "stalker.png"), stalker)
    brute, brute_sheet, brute_glow, brute_frames = brute_boss()
    write_png(os.path.join(ROOT, "boss", "brute.png"), brute)
    write_png(os.path.join(ROOT, "boss", "brute_sheet.png"), brute_sheet)
    write_png(os.path.join(ROOT, "boss", "brute_glow.png"), brute_glow)
    prism, prism_left, prism_right, prism_core, prism_open = prism_boss()
    write_png(os.path.join(ROOT, "boss", "prism.png"), prism)
    write_png(os.path.join(ROOT, "boss", "prism_left.png"), prism_left)
    write_png(os.path.join(ROOT, "boss", "prism_right.png"), prism_right)
    write_png(os.path.join(ROOT, "boss", "prism_core.png"), prism_core)
    aegis = prism_aegis()
    write_png(os.path.join(ROOT, "boss", "prism_aegis.png"), aegis)
    shard = prism_shard()
    write_png(os.path.join(ROOT, "boss", "prism_shard.png"), shard)
    gate, gate_open = gate_tiles()
    write_png(os.path.join(ROOT, "tiles", "gate.png"), gate)
    write_png(os.path.join(ROOT, "tiles", "gate_open.png"), gate_open)

    title = logo()
    write_png(os.path.join(ROOT, "ui", "logo.png"), title)

    crosshair = from_ascii("crosshair", CROSSHAIR, {}, width=11, height=11)
    write_png(os.path.join(ROOT, "ui", "crosshair.png"), crosshair)

    preview = os.path.join(os.path.dirname(__file__), "..", "docs", "preview")
    write_png(os.path.join(preview, "loot.png"), contact_sheet(list(loot.values())))
    write_png(
        os.path.join(preview, "player_tiles.png"),
        contact_sheet([player, engineer, bulwark, hacker, doctor, sentry, mine, mine_light]),
    )
    write_png(os.path.join(preview, "station.png"), contact_sheet(station_preview(sets, shared), scale=2))
    write_png(os.path.join(preview, "logo.png"), contact_sheet([title], scale=8))
    write_png(os.path.join(preview, "enemies.png"), contact_sheet(list(enemies.values())))
    write_png(os.path.join(preview, "boss.png"), contact_sheet([keeper, hand, stalker, gate, gate_open]))
    write_png(os.path.join(preview, "brute.png"), contact_sheet(brute_frames, scale=4))
    write_png(os.path.join(preview, "prism.png"), contact_sheet([prism, prism_open, prism_core, aegis, shard], scale=4))
    write_png(os.path.join(preview, "perks.png"), contact_sheet(list(perks.values()) + list(drops.values())))
    write_png(os.path.join(preview, "guns.png"), contact_sheet(list(guns.values())))
    write_png(
        os.path.join(preview, "fx.png"),
        contact_sheet(list(PROJECTILES.values()) + list(fx.values()) + [crosshair]),
    )
    print("assets written to", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()
