# rusted-empire developer guide

This guide is for anyone changing the code: how it is organised, how a turn runs,
where the original VMS-Empire C code went, how saves are versioned, and how to test.
It describes 0.9.0-beta.

## Project goals

1. **True to the original.** Rules, world generation and the computer player follow
   VMS-Empire 1.13. When the original's behaviour looks odd but is deliberate (the
   computer ignoring fighter fuel, difficulty only choosing start continents), keep it.
2. **Quality-of-life changes are explicit.** Every intended difference from the
   original is listed in the README under "Differences from the original". A change to
   the rules goes on that list, or it is a bug.
3. **Ready for a client/server split.** A fork is planned that separates the game
   (server) from the terminal front end (client). Keep the game logic free of screen
   and input code; see [The client/server seam](#the-clientserver-seam).

## Building and testing

```
cargo build --release
cargo test --release        # 154 tests, under a second
cargo clippy
```

**Toolchain compatibility.** The maintainer builds with a Rust toolchain older than
1.87. Don't use standard-library APIs newer than about Rust 1.74 without checking.
Known traps: `is_multiple_of` (1.87) and `Option::is_none_or` (1.82). `cargo clippy
--fix` on a newer toolchain can introduce these by itself, so always build and test on
the oldest toolchain you support before sending a change. Dependencies are pinned
(`ratatui = "=0.26.0"`, `crossterm = "=0.27.0"`) for the same reason.

## Source map

| File | Lines | What it holds |
|---|---|---|
| `main.rs` | ~80 | Command-line parsing only, then hands off to `app::run` or `headless::run`. Keep it that small. |
| `models.rs` | ~290 | Core types: `Terrain`, `Owner`, `PieceType` (with all unit statistics), `UnitActivity` (orders), `Piece`, `City`, `Loc`, `FighterStats`, and the rig's rule-experiment flags. |
| `game.rs` | ~2700 | `Game`: the whole game state and every rule. World generation, start selection, production, movement, combat, city assault, sight and fog of war, standing orders for the player's units, the end of the game, save and load. |
| `ai.rs` | ~1500 | The computer player: a translation of `compmove.c` and the path-finding in `map.c`, written for either side (`ai_move(me)`). |
| `headless.rs` | ~310 | The AI-vs-AI test rig (`--headless`). |
| `app.rs` | ~900 | The interactive front end: startup questions, the main input loop, prompts, edit mode. |
| `disp.rs` | ~360 | Draws the screen with ratatui. |
| `view.rs` | ~75 | `View`: what the screen is showing (mode, cursor, scroll, map centre). Never saved. |
| `keys.rs` | ~120 | The keyboard vocabulary: unit keys, direction keys, order keys and menus. |
| `terminal.rs` | ~40 | `TerminalGuard`: raw mode and the alternate screen, restored on every exit path, including panics. |
| `game_tests.rs` | ~2700 | Rule tests, regression tests, save-format tests. |

## Architecture

```
            keys (crossterm events)
                    │
        ┌───────────▼───────────┐         ┌──────────────┐
        │ app.rs  (input loop)  │────────▶│ disp.rs      │──▶ terminal
        │ View (screen state)   │  draw   │ (ratatui)    │
        └───────────┬───────────┘         └──────▲───────┘
          commands  │                            │ reads
        ┌───────────▼────────────────────────────┴───────┐
        │ Game (game.rs + ai.rs)   state + rules          │
        │ never reads View, never touches the terminal    │
        └─────────────────────────────────────────────────┘
```

- **`Game` is the model.** It holds the world, both sides' units and cities, the
  computer's and the player's views of the world, the message log and the RNG. It
  knows nothing about the screen.
- **`View` is the screen state**: mode (manual, auto, edit, picking a destination),
  edit cursor, map centre, message scroll, whether the enemy map is shown. It's
  created fresh on load and never saved.
- **The front end talks to `Game` through command methods**: `attempt_move`,
  `order_unit`, `skip_unit`, `sentry_unit`, `wake_unit`, `wake_units_at`,
  `launch_cargo_at`, `set_production`, `set_standing_order(OrderScope, ...)`,
  `process_turn`, `save_game` and `load_game`. New front-end features should add a
  command method rather than writing to `Game` fields from `app.rs`. A few older
  paths still set fields directly (for example the edit-mode `O` order sets
  `pieces[i].activity`); converting those is good preparation for the split.
- **Messages** go through `Game::log`. The log keeps the last 100 messages
  (`MAX_MESSAGES`) and counts all of them (`messages_logged`, not saved), so the view
  can notice new ones.

### Data model

- `Piece` has an `id` (stable across the game; allocate with `next_free_id`, never by
  hand), `ptype`, `owner`, `loc`, `hits`, `moves_left`, `activity` (its order) and
  `fuel` (fighters: squares left; satellites: turns left). Units in a transport's or
  carrier's **hold** live in that piece's `cargo` vector, not in `Game::pieces`.
  Waking the cargo moves it back to `pieces` ("on deck"). Code that counts or searches
  units must decide whether to look in holds; see `count_armies` in
  `comp_should_resign` for a recursive count.
- **Piece indices are not stable.** Dead pieces are removed with
  `pieces.retain(|p| p.hits > 0)`, so an index taken before a move may point at a
  different piece afterwards. Within a turn, mark pieces dead (`hits = 0`) and clean up
  at the end. Across a change to the list, look pieces up by `id`; `ai_move` does this.
- `City` has `owner`, `prod`, `work` (progress; negative while retooling),
  `default_orders` (the city's standing orders for new units) and `seen_owner` (who
  the player last saw holding it).
- `Game::default_orders` holds the global standing orders.

### The two views of the world (fog of war)

| Field | Whose | Contents |
|---|---|---|
| `player_map` | player | 1 where the player has ever seen the terrain, 0 where not. |
| `player_view` | player | The enemy unit letter last seen on each square, 0 for none. Updated only for squares in sight, so it can be out of date, as in the original. |
| `City::seen_owner` | player | The last owner the player saw for each city. |
| `comp_map` | computer | The computer's view in the original's characters: `' '` unexplored, `'.'` sea, `'+'` land, `'*'` neutral city, `'X'` its own city, `'O'` the player's city, lower case for its own units and upper case for the player's. |

The AI code is written for "my side": `ai_map(me)` and `side_char` translate so that
`X` and lower case always mean *mine*. In headless games the player side uses
`player_ai_map`.

Sight: `player_sight()` builds the player's sight grid once; `player_sees(loc)` and
`is_visible_with(...)` answer per square or per unit. `refresh_player_view()` runs at
the start of each player turn. Unit sight ranges are `PieceType::sight_range()`;
cities see 1 (`Game::CITY_SIGHT`). A submarine is visible only next to an enemy unit,
or within a destroyer's sight range.

## How a turn runs

The player moves units interactively; `N` (or automove) then calls
`Game::process_turn`, which does the rest of the turn:

1. `turn += 1`.
2. **The player's cities produce** (`advance_production`, `produce_unit`). New units
   take the city's standing order, or the global one.
3. **End-of-turn repair:** a damaged ship that sat in its own port without moving
   regains a hit.
4. **Sentries check for enemies** within their sight range and wake. Fighters on
   sentry switch to attack ("CAP intercept").
5. **Satellites move** (they move before everything else, as in the original).
6. **The player's standing orders run**: explore, attack, random, destination,
   direction, fill, upgrade and land, in that order.
7. **If the player side is AI-run** (headless rig): `ai_move(Owner::Player)`.
8. **The computer moves**: `comp_move()` → `ai_move(Owner::Comp)`:
   1. Armies in a transport's hold at sea come up on deck.
   2. `ai_scan_all`: the computer looks around with every unit and city.
   3. `do_cities`: choose production for newly won cities, build what is ready,
      reconsider production, move ship building off lakes.
   4. Move units by type in the original's order: transports, carriers, battleships,
      patrol boats, submarines, destroyers, armies, fighters. Each unit is moved by
      `cpiece_move` until its moves are used up.
9. Satellites age and burn out after `SATELLITE_LIFE` turns.
10. Dead pieces are removed, and `refresh_player_view()` starts the player's turn.

The player always moves before the computer within a turn. This matters for the
headless rig's balance: see [The headless rig](#the-headless-rig).

## Where the original C code went

The computer player is a function-by-function translation, and the original names
are kept in the doc comments (written in backticks) so a C function can be found by
grepping. Main correspondences:

| VMS-Empire (C) | rusted-empire (Rust) |
|---|---|
| `comp_move` | `Game::comp_move` → `Game::ai_move(me)` (`ai.rs`) |
| `do_cities`, `comp_prod`, `comp_set_prod`, `comp_set_needed`, `overproduced`, `need_more` | same names in `ai.rs` |
| `cpiece_move`, `move_objective` | same names |
| `army_move`, `load_army`, `make_army_load_map`, `nearby_load`, `nearby_count`, `make_unload_map` | `army_move`, `load_army`, `make_army_load_map`, `nearby_load`, `nearby_count`, `unload_map` |
| `transport_move`, `fighter_move`, `ship_move` | same names |
| `find_attack`, `vmap_cont`, `vmap_cont_scan`, `vmap_at_sea` | `find_attack`, `vmap_cont`, `cont_scan`, `at_sea` |
| `vmap_prune_explore_locs`, `unmark_explore_locs` | `prune_explore_locs`, `unmark_explore_locs` |
| `expand_perimeter`, `objective_cost`, `terrain_type` | same names |
| `vmap_find_xobj`, `vmap_find_lwobj`, `vmap_find_wlobj`, `vmap_find_dest` | `find_xobj`, `find_lwobj`, `find_wlobj`, `find_dest` |
| `vmap_mark_path`, `vmap_mark_near_path`, `vmap_find_dir` | `mark_path`, `mark_near_path`, `find_dir` |
| `make_map`, `place_cities`, `select_cities`, `find_cont`, `make_pair` (game.c) | `generate_map`, city placement in `generate_world`, `choose_start`, `start_continents`, `start_pairs` |
| `attack_city` / `kill_city` (attack.c) | `assault_city` |
| `check_endgame` (empire.c) | `comp_should_resign`, `check_game_over` |
| `scan`, `scan_sat` (object.c) | `ai_scan_many` / `scan_for_player`, with `sight_range()` |
| `move_sat`, `bounce` (object.c) | `move_satellites` |
| `dist` (math.c) | `geo_dist` / `Game::dist` |
| `piece_attr[]` (data.c) | `PieceType` methods in `models.rs` |
| `LIST_SIZE` (empire.h, 5000 pieces) | no limit; see [Performance](#performance) |

Two things to keep in mind when changing the AI:

- **Iteration order is behaviour.** The original walks its lists and arrays in
  particular orders, and ties are broken by whichever is found first. The port keeps
  those orders (the unit move order, perimeter expansion order, `find_dir` preferring
  diagonals). Changing an iteration order changes how the computer plays.
- **Check the fingerprint.** A seeded headless run is deterministic. Before and after
  a refactor that shouldn't change behaviour, run the same seeds
  (`--headless --seed N --games 20`) and compare the per-game lines. They should
  match exactly.

## Save files

- Saves are `serde_json` of `Game`. Fields that are UI-only or rebuilt at load are
  `#[serde(skip)]` (for example `rng`, `fighter_stats`, `player_ai`,
  `messages_logged`).
- `SAVE_VERSION` (currently **3**) is written into every save.
  - 0 (no field): before versioning. The very first saves also lack the score
    fields, which default to 0.
  - 1: the first versioned format.
  - 2: added the player's remembered view (`player_view`, `City::seen_owner`).
  - 3: gave the computer its own view of the world (`comp_map`).
- `load_game` refuses saves from a newer version, runs `migrate()` to bring older ones
  up to date, then `validate()` to reject damaged files instead of panicking later.
- `save_game` writes `<file>.tmp` and renames it, so a failed save never destroys the
  previous one.

**When you add or change a saved field:**

1. Give a new field `#[serde(default)]` (or a default function), so older saves load.
2. If an older save needs more than the default to be correct, bump `SAVE_VERSION`
   and add a step to `migrate()` guarded by `if self.save_version < N`.
3. If the field has a size that must match the map, add it to `validate()`.
4. Add a test in the "save format" section of `game_tests.rs`. The existing tests
   remove keys from a fresh save's JSON to imitate older formats.
5. For a real check, build the previous release, play and save a game with it, and
   load that save with your build. 0.9.0-beta was checked this way against every
   save format the project has had.

## Testing

### Unit and regression tests

`cargo test` runs about 150 tests: rules (combat, movement, cargo, fuel, satellites,
production and retooling, city assault, the endgame), the AI (fixed scenarios, and
that it can play either side), world generation and start selection, keys, and the
save format. Tests build small hand-made maps with the helpers at the top of
`game_tests.rs`. Prefer a seeded `Game::with_seed` and a tiny map over a full random
world.

When fixing a bug, add a test that fails without the fix.

### The headless rig

`rs-empire --headless` plays the AI against itself on full-size worlds. The player
side is driven by the same `ai_move` code as the computer (`Game::player_ai`), so a
game is a mirror match. Each game prints its outcome, length, final city counts and
fighter statistics, and appends a row to `playtest_metrics.csv`:

```
Game,Seed,Outcome,Turns,ElapsedSec,PCities,CCities,
PFBuilt,PFLanded,PFLowFuelLandings,PFOutOfFuel,PFShotDown,PFLostOverCity,PFLostWithCarrier,
CFBuilt,CFLanded,CFLowFuelLandings,CFOutOfFuel,CFShotDown,CFLostOverCity,CFLostWithCarrier
```

Outcomes: `Wipeout` (one side has nothing left), `Resigned` (the original's
concession rule, applied to both sides), `Stalemate` (neither side reached a new
high in cities for `--stalemate` turns), `TurnLimit`.

Uses:

- **Regression and fingerprint checks**: the same seeds give the same games.
- **Fairness**: in a mirror match the two sides should win about equally over 100
  games. A lopsided split means one side is being treated differently. 0.9.0-beta
  fixed two such asymmetries: captured cities not being queued for an AI production
  decision, and the player's capital paying a retooling penalty the computer's did
  not. Remaining known asymmetry: the player side always moves first within a turn.
- **Measuring a rule change**: run the same seeds with and without it. The rule
  switches `--original-sight` and `--adjacent-subs` set process-wide flags in
  `models.rs` (`ORIGINAL_SIGHT`, `ADJACENT_SUB_DETECTION`) that only the rig sets. To
  add another experiment, add a flag the same way and read it at the one place the
  rule is decided. Don't toggle these flags in unit tests: tests run in parallel and
  the flags are global.
- **Finding a bad game**: every game prints its seed. Re-run it alone with
  `--headless --seed S --games 1`.

Threads: the rig runs games in parallel on half the available cores (1 to 4).

### Interactive checks

Debug builds (`cargo build`, not `--release`) have two extra keys at the turn prompt:

- `` ` `` or `~` reveals the whole map;
- `Z` enters observer mode, where the player's turns are skipped and the computer plays
  on (`O` or `Esc` to stop).

### TUI testing (planned before 1.0)

The terminal interface has no automated tests yet. That's the main reason this is a
beta. The plan:

1. **Screen tests.** Let `disp.rs` draw on any ratatui backend instead of fetching the
   real terminal itself, then render seeded game states to `TestBackend` and check the
   buffer: sidebar values, prompt text, fog of war, and small terminal sizes.
2. **Scripted input.** Put key reading in `app.rs` behind a small event-source trait,
   so tests can feed key sequences (move, order, edit mode, standing orders, save,
   load) and check `Game` and `View` after each step. The same trait is the input side
   of a future network client.
3. **End to end and random input.** Drive the release binary in a pseudo-terminal,
   with scripted sessions and long runs of random keys on seeded games, failing on any
   panic, hang or unloadable save.

## Performance

The computer player runs several full-map path searches per army per move, as the
original does. That is fast at normal sizes: a few milliseconds a turn early, and
tens of milliseconds with several hundred units. The original capped the board at
5000 pieces (`LIST_SIZE`); this port has no cap. In very long, even games, unit
counts can pass 1500 and the computer's move then takes 100–200 ms. Most of that is
in `army_move` (`find_lwobj`, `make_army_load_map` and `nearby_count`). If it becomes
a problem, the safe optimisations are ones that keep results identical (caching per
move, cheaper checks first), verified with the fingerprint check above.

## The client/server seam

When the game is split into a server and clients:

- **Server**: `Game` (`game.rs`, `ai.rs`, `models.rs`). It owns the RNG and all rules.
- **Client**: `app.rs`, `disp.rs`, `view.rs`, `keys.rs`, `terminal.rs`.
- **The protocol** is roughly the command methods listed under
  [Architecture](#architecture) (client to server), plus a per-player view of the game
  (server to client).
- **A client must only see its own side's view.** Today `disp.rs` reads `Game`
  directly and filters by the player's view maps. A server would instead send each
  client its `player_map`, `player_view`, `seen_owner` and own units only.
  `comp_map` already has the same shape, so a second human player fits the same
  model.
- `keys.rs` is deliberately free of terminal and game state so a client can reuse it.

## Working on the project

- Changes arrive as patch series (`git am`) against `main`. Keep one topic per patch,
  with the reason in the commit message.
- A rule change that departs from the original goes in the README's differences list,
  with a test.
- Before sending: `cargo build`, `cargo test` and `cargo clippy` on the oldest
  supported toolchain, plus a short headless run if the AI or the rules were touched.
- Files that translate the original C code (`ai.rs`, and the world generation and
  rules in `game.rs`) are derived from VMS-Empire under the GPL v2. Keep the
  attribution in [NOTICE](../NOTICE) accurate when moving code between files.
