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
| `class` | string | `--class` this run used | `GUNNER`, `ENGINEER`, `BULWARK`, `HACKER`, `DOCTOR`, or `mixed` (bots rotate through all five by crew index when `--class` isn't given) |
| `run` | u32 | 1-based run number within this process's batch | `1..=runs` |
| `seed` | u32 | this run's world seed (`--seed + (run - 1)`); rerun a single result with `--seed <this> --runs 1` | any u32 |
| `ending` | string | why the run stopped | `OVER` (the mission resolved — the crew got out by a pad or was wiped), `TIMEOUT` (hit `--time` first) |
| `extracted` | u32 | pirates who got out by the pad that ended the run (the final group) | `0..=crew` |
| `left_early` | u32 | pirates who took a pad out while the rest of the crew stayed in | `0..=crew` |
| `dead` | u32 | pirates killed (not extracted/left early) | `0..=crew` |
| `still_in` | u32 | pirates alive and active when the run ended (only >0 on `TIMEOUT`) | `0..=crew` |
| `deck` | u8 | deepest deck reached, 1-based | `1..=decks` |
| `decks` | u8 | total decks in the mission (currently fixed, `DECKS` in `src/room.rs`) | `3` |
| `credits` | u32 | total credits carried by the crew at run end (each pirate's own haul, before the split) | `0..` |
| `payout` | u32 | total credits the crew takes home after settlement (`mission::settle`) | `0..` |
| `pool` | u32 | the final group's pooled credits (0 if nobody finished) | `0..` |
| `bonus` | u32 | added to `pool` for the early leavers: 25% of it per leaver (0 if nobody finished) | `0..` |
| `bars_silver`, `bars_gold`, `bars_platinum` | u32 | bars pulled from each deck's vault (decks 1, 2 and 3; full vaults hold 24, 32 and 40) | `0..=40` |
| `kills` | u32 | enemies killed during the run | `0..` |
| `alarm` | f32 | alarm meter level at run end (see `Alarm` in `src/mission.rs`) | `0.0..` |
| `seconds` | f32 | in-game seconds the run lasted | `0.0..=time_limit` |
| `pirates` | string | each pirate by slot, `;`-separated, as `<fate><deck>:<credits>:<payout>`. Fate: `F` finished (final group), `E` left early, `D` dead, `I` still in. Deck is 1-based. E.g. `F3:5022:8075;E2:662:662;D3:900:0` | |
| `ch_offered` | u32 | mid-heist challenges offered this run (`ChallengeLog` in `src/challenge.rs`); each deck offers the catalog's challenges someone in the crew can complete, `OFFER_DELAY` after arrival. 0 for a crew with no Gunner while the catalog is Gunner-only | `0..` |
| `ch_completed` | u32 | of those, completed | `0..=ch_offered` |
| `ch_failed` | u32 | of those, failed: the crew left the deck, or nobody who could complete it was still in play (dead or out by a pad). `ch_offered - ch_completed - ch_failed` is how many were still open when the run stopped | `0..=ch_offered` |
| `reward_secs` | f32 | pirate-seconds with a challenge's damage reward running, summed over the crew (one 45 s reward that runs out = `45.0`; it stops early if its holder dies or the run ends) | `0.0..` |
| `pirate_kills` | string | guards each pirate killed with their own weapons (guns, laser, grenades, novas, arcs: the `CrewHit`s marked `killed` that challenges count), by slot, `;`-separated, e.g. `12;5;7`. Their sum can fall short of `kills`: sentries, hacked guards and perks don't count | |
| `breaches` | u32 | times the crew was let in through a vault's gate (`Breach` in `src/breach.rs` reaching `Fight`): every living pirate on the deck accepted the call and got locked in with the keeper. At most one per deck | `0..=decks` |
| `keepers` | u32 | vault keepers (`src/boss.rs`) killed: breaches that ended with the gate opening. `breaches - keepers` is 1 when the crew was wiped (or the run timed out) locked in | `0..=breaches` |
| `keeper_secs` | f32 | in-game seconds spent locked in with a keeper (`Fight`), summed over the run's breaches | `0.0..=seconds` |
| `keeper_deaths` | u32 | times a pirate went down while locked in with a keeper (a pirate revived and downed again counts twice) | `0..` |

Settlement rules, for checking rows: early leavers' payout equals their own
credits; the final group each get `round((pool + bonus) / finishers)`; dead
and still-in pirates get 0, and on a wipe `pool` and `bonus` are 0.

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
