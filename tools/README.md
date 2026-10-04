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

## Per-tick trace (`--trace`)

`--trace` prints one row per pirate and one row per enemy, every tick, to
stdout — for building (state, action) training data rather than summarizing
a finished run. Like `--csv`, it takes over stdout as a pure data stream
(status/per-run/summary text moves to stderr); `--quiet` additionally drops
that stderr chatter. `--trace` and `--csv` are independent — if both are
given, both write their own rows to stdout (CSV rows only at each run's end,
trace rows every tick), which is rarely what you want; pick one per
invocation. Because it's a row per entity per tick, prefer `--runs 1` — the
binary warns on stderr if `--runs` > 1 with `--trace` set.

Header: `tick,run,seed,kind,entity,x,y,team,health,max_health,ai_state,move_x,move_y,aim_x,aim_y,fire,dash`

| column | type | meaning | possible values |
|---|---|---|---|
| `tick` | u32 | 0-based simulation tick within this run | `0..` |
| `run` | u32 | 1-based run number, same meaning as in the per-run CSV | `1..=runs` |
| `seed` | u32 | this run's world seed | any u32 |
| `kind` | string | which entity this row describes | `enemy`, `player` |
| `entity` | u32 | Bevy entity index — stable within this process, not across runs/processes | any u32 |
| `x`, `y` | f32 | world position this tick | any f32 |
| `team` | string | `Team` component value | `Player`, `Enemy` |
| `health` | f32 | current health this tick | `0.0..=max_health` |
| `max_health` | f32 | max health | `0.0..` |
| `ai_state` | string | enemy FSM state (see `AiState` in `src/enemies.rs`); blank on `player` rows | `Idle`, `Hunt`, `Telegraph`, `Lunge`, `Recover` |
| `move_x`, `move_y` | f32 | bot's movement input this tick; blank on `enemy` rows | `-1.0..=1.0` each axis |
| `aim_x`, `aim_y` | f32 | bot's aim point in world space; blank on `enemy` rows | any f32 |
| `fire` | bool | bot fired this tick; blank on `enemy` rows | `true`, `false` |
| `dash` | bool | bot dashed this tick; blank on `enemy` rows | `true`, `false` |

This is state/action data, not (state, action, **reward**, next_state) —
derive a reward signal by diffing consecutive rows (e.g. health deltas,
distance closed to a target) once you've picked what you're optimizing for;
the trace deliberately doesn't bake in a reward definition.
