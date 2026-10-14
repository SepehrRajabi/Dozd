# DOZD

![DOZD](docs/preview/logo.png)

A co-op space-pirate heist game. Loot a station three decks deep, ride the lift
down to richer decks, and get out on a hazard pad before security overwhelms
you. Only the pirates standing on the pad when it finishes get out.

Built with Rust and [Bevy](https://bevyengine.org/); LAN multiplayer uses
[bevy_replicon](https://github.com/projectharmonia/bevy_replicon).

> [!IMPORTANT]
> This project has been developed entirely by Claude Opus 5.5 and is a
> prototype. I want to figure out the game itself before starting to write it
> myself.

## Screenshots

| | |
| --- | --- |
| ![Main menu](docs/screenshots/menu.png) | ![Loadout](docs/screenshots/loadout.png) |
| The main menu | Picking a class and guns |
| ![Crew lobby](docs/screenshots/lobby.png) | ![Perks](docs/screenshots/perks.png) |
| The crew lobby: one of each class, picked by whoever took it | The perks lying around each deck, under **EXTRAS** |
| ![A raid](docs/screenshots/raid.png) | ![Results](docs/screenshots/results.png) |
| A Bulwark and a Doctor against a Herald's empowered guards, with each other's status in the crew panel | A crew that didn't make it out |
| ![Enemies](docs/screenshots/enemies.png) | |
| The station's guards, under **EXTRAS** in the main menu | |

## Run

```sh
cargo run --release                    # main menu
cargo run --release -- solo            # straight into a solo run
cargo run --release -- host [port]     # host a LAN game (default port 5000)
cargo run --release -- join <address>  # join a host, e.g. 192.168.1.20 or host:5000
```

Release builds embed all assets, so `target/release/dozd` is a single file you
can hand to friends.

## Pirates

Pick a class and your guns under **LOADOUT** in the main menu before a raid.
Every pirate carries 3 guns, in the order you pick them.

Class passives show as chips under your bars and on each crewmate's card,
alongside a Doctor's ward.

Online, a crew holds at most one of each class, so up to 5 pirates. Host or
join one under **MULTIPLAYER**. Hosting opens a lobby where everyone picks a free class and their guns, then the host
starts the run; after each run the crew returns to the lobby. Someone joining
mid-run picks from the classes still free.

| Class | Ability |
| --- | --- |
| Gunner | Every crewmate, wherever they are, has a 30% chance for a shot not to use up ammo. Carries a 4th gun |
| Engineer | Wears heavier armour: 80% more shield. Q drops a sentry gun that shoots guards for 15 s (25 s cooldown). Guards shoot back at it. Q next to it picks it back up, taking time off the cooldown for the time it had left |
| Bulwark | Crewmates within reach deal 25% more damage. Q raises or lowers a riot shield you carry in front of you, turning as you aim. No shot gets through, from either side, and you can't fire while holding it. It drains while up (10 s from full) and recharges while down (22 s from empty) |
| Hacker | Guards within reach are jammed: 35% of their shots go wide and their blows whiff. Q turns the guard nearest your crosshair to the crew's side for 20 s (30 s cooldown) |
| Doctor | Slowly heals crewmates within reach; Q wards them and the Doctor against all harm for 4 s (28 s cooldown) |

## Controls

| Key | Action |
| --- | --- |
| WASD / arrows | Move |
| Mouse | Aim |
| Left click | Fire |
| Space | Dash |
| R | Reload |
| 1-4 / wheel | Switch gun |
| Q | Class ability |
| E | Take loot |
| Enter | Play again (results screen) |
| Esc | Leave / back |

## Headless

Runs the game with no window, rendering or input, many times faster than real
time. Bots play the pirates, and every run is reproducible from its seed.

```sh
cargo run --release -- headless --seed 1 --runs 20
```

Options: `--seed N`, `--runs N`, `--crew 1-8`,
`--class gunner|engineer|bulwark|hacker|doctor|mixed` (bots' class; mixed rotates), `--bot random|idle`,
`--time SECS` (per-run limit), `--hz N` (steps per game second).

## Development

```sh
cargo test
cargo clippy
python3 tools/gen_assets.py   # regenerate the pixel-art sprites in assets/
```

Game rules live in each module's main plugin (`GamePlugins`); sprites, HUD,
menus and input live in its `*ViewPlugin` (`ViewPlugins`), which headless mode
leaves out.
