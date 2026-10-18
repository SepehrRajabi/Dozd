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


def noise(x, y, seed):
    n = (x * 374761393 + y * 668265263 + seed * 144269504) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((n >> 16) & 0xFF) / 255.0


def shade(c, amount):
    return tuple(max(0, min(255, int(v + amount))) for v in c[:3]) + (255,)


def blank(color):
    return [[color for _ in range(SIZE)] for _ in range(SIZE)]


# --------------------------------------------------------------------------
# Station tiles. Seen from above and a little south: walls show a front face
# where the floor below meets them and a dark cap elsewhere. Floors come in a
# few plate variants the game mixes so the deck doesn't look stamped out.
# --------------------------------------------------------------------------

STEEL = rgb(58, 64, 82)
STEEL_HI = rgb(74, 81, 102)
STEEL_LO = rgb(40, 44, 59)
SEAM = rgb(28, 31, 42)
RIVET = rgb(120, 130, 156)
CAP = rgb(30, 34, 47)
CAP_RIM = rgb(104, 114, 142)
FACE = rgb(52, 58, 76)
FACE_HI = rgb(74, 82, 104)
FACE_LO = rgb(34, 38, 51)
CYAN = rgb(80, 230, 255)
AMBER = rgb(255, 176, 50)


def mottled(base, seed, amount=7):
    return [[shade(base, (noise(x, y, seed) - 0.5) * amount) for x in range(SIZE)] for y in range(SIZE)]


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


def floor_plate(seed):
    px = mottled(STEEL, seed)
    bevel(px, 0, 0, SIZE - 1, SIZE - 1, STEEL_HI, STEEL_LO)
    return px


def floor_tiles():
    """Plain plate, quartered plate, tread plate, and a worn plate with a drain."""
    plain = floor_plate(1)
    for x, y in [(2, 2), (12, 2), (2, 12), (12, 12)]:
        rivet(plain, x, y)

    quartered = floor_plate(2)
    for i in range(1, SIZE - 1):
        quartered[7][i] = shade(STEEL, -12)
        quartered[i][7] = shade(STEEL, -12)

    tread = floor_plate(3)
    for y in range(2, SIZE - 2):
        for x in range(2, SIZE - 2):
            # Raised diamond grip, two pixels long, alternating direction.
            cell = (x // 4 + y // 4) % 2
            if (x + (y if cell else -y)) % 4 == 0:
                tread[y][x] = shade(STEEL, 9)
                tread[y + 1][x] = shade(STEEL, -7) if y + 1 < SIZE - 2 else tread[y + 1][x]

    worn = floor_plate(4)
    for y in range(1, SIZE - 1):
        for x in range(1, SIZE - 1):
            stain = noise(x // 2, y // 2, 40)
            if stain > 0.72:
                worn[y][x] = shade(worn[y][x], -10)
    for y in range(6, 10):
        for x in range(6, 10):
            edge = x in (6, 9) or y in (6, 9)
            worn[y][x] = shade(STEEL, -14) if edge else rgb(30, 33, 44)
    rivet(worn, 2, 2)
    rivet(worn, 12, 12)
    return [plain, quartered, tread, worn]


def wall_cap(mask):
    """The top of a wall. `mask` bits (N=1, E=2, S=4, W=8) mark open floor
    beside it; those edges get a lit rim."""
    px = mottled(CAP, 7, 5)
    for i in range(SIZE):
        px[i][7] = shade(CAP, -6)
        px[7][i] = shade(CAP, -6)
    rim = shade(CAP_RIM, -30)
    if mask & 1:
        for x in range(SIZE):
            px[0][x], px[1][x] = CAP_RIM, rim
    if mask & 4:
        for x in range(SIZE):
            px[SIZE - 1][x], px[SIZE - 2][x] = CAP_RIM, rim
    if mask & 8:
        for y in range(SIZE):
            px[y][0], px[y][1] = CAP_RIM, rim
    if mask & 2:
        for y in range(SIZE):
            px[y][SIZE - 1], px[y][SIZE - 2] = CAP_RIM, rim
    return px


def wall_face_base(seed):
    """A wall's front face: lit cap lip on top, panels, a dark kickplate."""
    px = mottled(FACE, seed, 6)
    for x in range(SIZE):
        px[0][x] = CAP_RIM
        px[1][x] = shade(CAP_RIM, -30)
        px[2][x] = FACE_LO
        px[SIZE - 3][x] = FACE_HI
        px[SIZE - 2][x] = rgb(28, 31, 42)
        px[SIZE - 1][x] = rgb(16, 17, 24)
    return px


def wall_faces():
    """Panelled, piped, lit and with a terminal screen."""
    panels = wall_face_base(21)
    bevel(panels, 1, 3, 7, 12, FACE_HI, FACE_LO)
    bevel(panels, 8, 3, 14, 12, FACE_HI, FACE_LO)
    for x, y in [(2, 4), (12, 4), (2, 10), (12, 10)]:
        panels[y][x] = RIVET

    pipes = wall_face_base(22)
    for x in range(SIZE):
        for y, tone in [(5, 0), (6, 10), (7, -14), (9, 0), (10, 10), (11, -14)]:
            pipes[y][x] = shade(rgb(86, 92, 104), tone)
    for x in (3, 12):
        for y in range(4, 13):
            pipes[y][x] = rgb(40, 44, 58)
        pipes[4][x] = RIVET

    lit = wall_face_base(23)
    bevel(lit, 2, 4, 13, 11, FACE_LO, FACE_HI)
    for x in range(4, 12):
        for y in range(6, 10):
            lit[y][x] = rgb(18, 20, 28) if y % 2 == 0 else rgb(46, 50, 64)
    lit[5][3] = CYAN
    lit[5][12] = shade(CYAN, -80)

    screen = wall_face_base(24)
    bevel(screen, 2, 4, 13, 11, FACE_LO, FACE_HI)
    glass = rgb(14, 30, 26)
    for y in range(5, 11):
        for x in range(3, 13):
            screen[y][x] = glass
    for y, length in [(6, 7), (8, 5), (9, 8)]:
        for x in range(4, 4 + length):
            screen[y][x] = rgb(70, 220, 130) if x < 4 + length - 1 else rgb(40, 130, 80)
    screen[12][7] = AMBER
    return [panels, pipes, lit, screen]


def grate_tile():
    px = floor_plate(5)
    bevel(px, 1, 1, SIZE - 2, SIZE - 2, STEEL_LO, STEEL_HI)
    for y in range(2, SIZE - 2):
        for x in range(2, SIZE - 2):
            bar = x % 3 == 1
            depth = rgb(14, 15, 22)
            # Pipes glimpsed through the grating.
            if y in (6, 7) or y in (11, 12):
                depth = rgb(36, 40, 52) if y in (6, 11) else rgb(26, 28, 38)
            px[y][x] = shade(STEEL, 8) if bar else depth
    for x in range(2, SIZE - 2):
        px[2][x] = shade(STEEL, 14)
    return px


def hazard_tile():
    px = blank(rgb(0, 0, 0))
    for y in range(SIZE):
        for x in range(SIZE):
            stripe = ((x + y) // 4) % 2 == 0
            c = rgb(232, 190, 40) if stripe else rgb(30, 30, 34)
            # Boots have worn the paint thin in places.
            wear = noise(x, y, 31)
            if stripe and wear > 0.82:
                c = rgb(170, 140, 40)
            elif not stripe and wear > 0.9:
                c = rgb(48, 46, 44)
            px[y][x] = c
    bevel(px, 0, 0, SIZE - 1, SIZE - 1, rgb(80, 80, 86), rgb(18, 18, 22))
    return px


def door_tile():
    """A blast door set into a wall face: two leaves, a seam and a status light."""
    px = wall_face_base(25)
    leaf = rgb(76, 82, 98)
    for y in range(3, SIZE - 3):
        for x in range(2, SIZE - 2):
            px[y][x] = shade(leaf, (noise(x, y, 26) - 0.5) * 6)
    bevel(px, 2, 3, 7, SIZE - 4, shade(leaf, 22), shade(leaf, -26))
    bevel(px, 8, 3, SIZE - 3, SIZE - 4, shade(leaf, 22), shade(leaf, -26))
    for y in range(3, SIZE - 3):
        px[y][7] = rgb(16, 18, 24)
    for x in range(3, 7):
        px[5][x] = AMBER if x % 2 else rgb(40, 34, 20)
    for x in range(9, 13):
        px[5][x] = AMBER if x % 2 == 0 else rgb(40, 34, 20)
    px[9][5] = px[9][10] = RIVET
    px[SIZE - 3][7] = px[SIZE - 3][8] = rgb(80, 255, 120)
    return px


def vent_tile():
    """Floor vent that reinforcements crawl out of."""
    px = floor_plate(6)
    bevel(px, 2, 2, SIZE - 3, SIZE - 3, SEAM, STEEL_HI)
    for y in range(3, SIZE - 3):
        for x in range(3, SIZE - 3):
            glow = max(0.0, 1.0 - abs(x - 7.5) / 5.0) * (0.6 + 0.4 * (y / SIZE))
            if y % 3 == 0:
                px[y][x] = shade(STEEL, 6)
            elif y % 3 == 1:
                px[y][x] = rgb(14, 12, 18)
            else:
                px[y][x] = rgb(int(70 + 150 * glow), int(20 + 40 * glow), 26)
    for x, y in [(1, 1), (13, 1), (1, 13), (13, 13)]:
        rivet(px, x, y)
    return px


def lift_platform():
    """Cargo lift platform, 2x2 tiles: ride it down to the next deck. Returned
    as its four quarters: top-left, top-right, bottom-left, bottom-right."""
    n = SIZE * 2
    base = rgb(46, 52, 68)
    px = [[shade(base, (noise(x, y, 11) - 0.5) * 6) for x in range(n)] for y in range(n)]
    # Diamond plate.
    for y in range(4, n - 4):
        for x in range(4, n - 4):
            if (x + y) % 4 == 0 and (x - y) % 8 == 0:
                px[y][x] = shade(base, 20)
                px[y + 1][x] = shade(base, -14)
    # Heavy rim with running lights and hazard corners.
    for i in range(n):
        for d, c in [(0, rgb(16, 18, 26)), (1, rgb(100, 110, 134)), (2, rgb(70, 78, 98)), (3, rgb(30, 34, 46))]:
            px[d][i] = px[i][d] = c
            px[n - 1 - d][i] = px[i][n - 1 - d] = c if d else rgb(10, 11, 16)
    for i in range(5, n - 5, 4):
        for row, col in [(1, i), (n - 2, i), (i, 1), (i, n - 2)]:
            px[row][col] = CYAN
    for cx, cy in [(4, 4), (n - 8, 4), (4, n - 8), (n - 8, n - 8)]:
        for y in range(4):
            for x in range(4):
                px[cy + y][cx + x] = AMBER if (x + y) % 3 == 0 else rgb(30, 30, 34)
    # Two glowing chevrons pointing down.
    for top, color in ((10, CYAN), (16, shade(CYAN, -90))):
        for k in range(6):
            for x in (10 + k, n - 11 - k):
                px[top + k][x] = color
                px[top + k + 1][x] = shade(color, -70)
    return [
        [row[:SIZE] for row in px[:SIZE]],
        [row[SIZE:] for row in px[:SIZE]],
        [row[:SIZE] for row in px[SIZE:]],
        [row[SIZE:] for row in px[SIZE:]],
    ]


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


def station_preview(floors, faces, caps, lift, tiles):
    """A small mock room, to see the tiles together."""
    plan = [
        "##########",
        "#FFFDFFFF#",
        "#.,,....=#",
        "#.,,.LL.=#",
        "#....LL.v#",
        "##########",
    ]
    rows, cols = len(plan), len(plan[0])
    open_ = lambda r, c: 0 <= r < rows and 0 <= c < cols and plan[r][c] not in "#FD"
    out = [[rgb(14, 15, 22) for _ in range(cols * SIZE)] for _ in range(rows * SIZE)]
    lift_seen = {}
    for r in range(rows):
        for c in range(cols):
            ch = plan[r][c]
            if ch == "F":
                img = faces[(r * 7 + c * 3) % len(faces)]
            elif ch == "D":
                img = tiles["door"]
            elif ch == "#":
                mask = (open_(r - 1, c)) | (open_(r, c + 1) << 1) | (open_(r + 1, c) << 2) | (open_(r, c - 1) << 3)
                img = caps[mask]
            elif ch == "L":
                q = (plan[r - 1][c] == "L") * 2 + (plan[r][c - 1] == "L")
                img = lift[q]
            elif ch == ",":
                img = tiles["grate"]
            elif ch == "=":
                img = tiles["hazard"]
            elif ch == "v":
                img = tiles["vent"]
            else:
                img = floors[(r * 5 + c * 11) % len(floors)]
            for y in range(SIZE):
                for x in range(SIZE):
                    out[r * SIZE + y][c * SIZE + x] = img[y][x]
            if ch not in "#FD":
                shades = floor_shadows()
                for key, solid in [("shade_n", not open_(r - 1, c)), ("shade_w", not open_(r, c - 1)), ("shade_e", not open_(r, c + 1))]:
                    if not solid:
                        continue
                    for y in range(SIZE):
                        for x in range(SIZE):
                            a = shades[key][y][x][3] / 255.0
                            o = out[r * SIZE + y][c * SIZE + x]
                            out[r * SIZE + y][c * SIZE + x] = tuple(int(v * (1 - a)) for v in o[:3]) + (255,)
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

    tiles = {
        "grate": grate_tile(),
        "hazard": hazard_tile(),
        "door": door_tile(),
        "vent": vent_tile(),
    }
    floors = floor_tiles()
    faces = wall_faces()
    caps = [wall_cap(mask) for mask in range(16)]
    lift = lift_platform()
    tiles.update({f"floor_{i}": px for i, px in enumerate(floors)})
    tiles.update({f"wall_face_{i}": px for i, px in enumerate(faces)})
    tiles.update({f"wall_cap_{i}": px for i, px in enumerate(caps)})
    tiles.update({f"lift_{i}": px for i, px in enumerate(lift)})
    tiles.update(floor_shadows())
    for name, px in tiles.items():
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

    keeper = keeper_body()
    write_png(os.path.join(ROOT, "boss", "keeper.png"), keeper)
    hand = from_ascii("hand", *KEEPER_HAND)
    write_png(os.path.join(ROOT, "boss", "hand.png"), hand)
    stalker = stalker_boss()
    write_png(os.path.join(ROOT, "boss", "stalker.png"), stalker)
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
        contact_sheet([player, engineer, bulwark, hacker, doctor, sentry] + floors + faces + [caps[0], caps[15]] + [tiles[k] for k in ("grate", "hazard", "door", "vent")]),
    )
    station = station_preview(floors, faces, caps, lift, tiles)
    write_png(os.path.join(preview, "station.png"), contact_sheet([station], scale=4))
    write_png(os.path.join(preview, "logo.png"), contact_sheet([title], scale=8))
    write_png(os.path.join(preview, "enemies.png"), contact_sheet(list(enemies.values())))
    write_png(os.path.join(preview, "boss.png"), contact_sheet([keeper, hand, stalker, gate, gate_open]))
    write_png(os.path.join(preview, "perks.png"), contact_sheet(list(perks.values()) + list(drops.values())))
    write_png(os.path.join(preview, "guns.png"), contact_sheet(list(guns.values())))
    write_png(
        os.path.join(preview, "fx.png"),
        contact_sheet(list(PROJECTILES.values()) + list(fx.values()) + [crosshair]),
    )
    print("assets written to", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()
