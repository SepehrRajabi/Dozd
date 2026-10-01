# Headless CSV output

`dozd headless --csv` (see `src/headless.rs`) prints one CSV row per run to
stdout, matching `CSV_HEADER` in that file. `tools/sweep.sh` wraps this to
sweep `--class` across all pirate classes.

Status/progress text (the startup banner and the final `done: ...` summary)
goes to stderr instead of stdout, so stdout stays pure CSV — safe to redirect
straight to a file or pipe into pandas/whatever without stripping anything.
Pair `--csv` with `--quiet` to also drop that stderr chatter.

If this schema changes, update this table and `CSV_HEADER` together — they
are not generated from a single source of truth.

| column | type | meaning | possible values |
| --- | --- | --- | --- |
| `class` | string | `--class` this run used | `GUNNER`, `ENGINEER`, `BULWARK`, `HACKER`, or `mixed` (bots rotate through all four by crew index when `--class` isn't given) |
| `run` | u32 | 1-based run number within this process's batch | `1..=runs` |
| `seed` | u32 | this run's world seed (`--seed + (run - 1)`); rerun a single result with `--seed <this> --runs 1` | any u32 |
| `ending` | string | why the run stopped | `OVER` (the mission resolved — crew wiped or all pirates left the deck one way or another), `TIMEOUT` (hit `--time` first) |
| `extracted` | u32 | pirates who rode the lift out | `0..=crew` |
| `left_behind` | u32 | pirates stranded when the lift left without them | `0..=crew` |
| `dead` | u32 | pirates killed and still on the deck (not extracted/left behind) | `0..=crew` |
| `still_in` | u32 | pirates alive and active when the run ended (only >0 on `TIMEOUT`) | `0..=crew` |
| `deck` | u8 | deepest deck reached, 1-based | `1..=decks` |
| `decks` | u8 | total decks in the mission (currently fixed, `DECKS` in `src/room.rs`) | `3` |
| `credits` | u32 | total credits carried by the crew at run end | `0..` |
| `kills` | u32 | enemies killed during the run | `0..` |
| `alarm` | f32 | alarm meter level at run end (see `Alarm` in `src/mission.rs`) | `0.0..` |
| `seconds` | f32 | in-game seconds the run lasted | `0.0..=time_limit` |

Every row also implicitly reflects whatever else was passed to `headless`
that run (`--crew`, `--bot`, `--hz`, `--time`) — those aren't repeated per
row since a single invocation holds them constant across its batch. If you
sweep more than one of them, carry the value into the CSV yourself (e.g. a
second column from the sweep script) since the binary doesn't know it's
being swept.
