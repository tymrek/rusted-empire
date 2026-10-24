# Changelog

All notable changes to rusted-empire. Versions follow the repository's tags.

## 0.9.1-beta (2026-10-24)

Bug fixes found in play, the new interface tests, and simpler handling
of units aboard ships.

### Fixed
- **Fighters low on fuel stalled in the air for good.** A fighter on an automatic order
  (explore, attack, destination, vector, land) that reached bingo fuel was given a
  course home but no moves that turn; the next turn the fuel check, seeing it still
  low, cancelled the course and sent it "home" again. It never moved, so it never
  landed (and, burning no fuel, never crashed). Now it flies home at once with the
  moves it has left, a fighter already headed for a base is never turned back, and one
  that can't reach any base wakes up and asks for orders, as in the original. Fighters
  already stuck in a saved game recover on the next turn.
- **The edit-mode cursor vanished in unexplored territory** (and on the enemy's map):
  it was only drawn on squares you had explored. It now always shows, and the
  edit-mode panel gives its position.
- The prompt line was cut off on narrow terminals: at 80 columns a unit's prompt lost
  its last keys (for an army, `[V]ctr`, `[W]ake`, `[T]rnsWait`). Long prompts now wrap
  onto the second prompt line.
- A save given to `--load` that can't be read now exits with an error status, with the
  reason printed after the terminal is restored.

### Changed
- **No more eject.** The separate `X` (eject/launch) key duplicated `W` and is gone,
  from both the unit prompt and edit mode. `W` on a transport or carrier brings up
  everything aboard.
- **Armies aboard come up when their transport arrives.** When a transport carrying
  sleeping armies finishes a standing order (or gets stuck) next to land, its armies
  come up on deck for orders. Sailing along a coast, or moving the transport by hand,
  leaves them asleep. Fighters on carriers still wait for `W`.
- All of the front end's input and output goes through a `Ui` trait (`ui.rs`), which
  is what makes the interface testable, and is the seam a network client will use.
- The repository now has a `.gitignore` and tracks `Cargo.lock`, so release builds use
  exactly the dependency versions that were tested.

### Added
- **Automated interface tests**: screen tests on an in-memory terminal, scripted tests
  that drive every prompt of the real front end, random-input fuzz tests that fail on
  any crash, hang or unsavable game, and an end-to-end test of the binary in a
  pseudo-terminal (`tools/tui_smoke.py`).

## 0.9.0-beta (2026-10-23)

First public beta. The rules and the computer player are complete and match
VMS-Empire 1.13, apart from the deliberate differences listed in the README. The
terminal interface still needs wider testing before 1.0.

### Added
- GPL v2 `LICENSE` and a `NOTICE` file with the game's full lineage and credits.
- `README.md`, a player manual (`docs/MANUAL.md`) and a developer guide
  (`docs/DEVELOPING.md`).
- Headless rig: mirror matches, where the same AI code plays both sides on full-size
  worlds. The default difficulty is the fairest level that puts the two sides on
  different continents. New options: `--small`, `--level`, `--turns`, `--stalemate`.
- Headless rig rule experiments: `--original-sight` and `--adjacent-subs` restore the
  original rules for sight and for submarine detection, to measure what this port's
  changes do.

### Fixed
- Saves from the very first builds of the port, which predate scoring, failed to load
  ("not a valid save file"). Every save format the project has had now loads; this
  was checked with real saves written by each old build.
- Headless rig fairness: when the AI-run player side captured a city, the city kept
  its old production instead of being queued for an AI production decision, and the
  player's capital started with a retooling penalty the computer's did not pay.
  Together these gave the computer side a 66–34 edge in 100 mirror games.
- Headless rig: a stalemate is now judged by progress (a side reaching a new high in
  cities). Cities changing hands back and forth no longer resets the count, so an
  even war of attrition can't run to the turn limit while unit counts grow past 1500.

## 0.6.0-beta

### Added
- **The original computer player**, a function-by-function port of VMS-Empire's
  `compmove.c` and the path-finding in `map.c`, replacing the earlier home-grown AI.
  The computer has its own view of the world and explores as the original does.
- **Original difficulty levels and start selection**: start continents are paired by
  value as in the original, and the difficulty level chooses the pairing.
- **The computer concedes** when it is outclassed (fewer than a third of your cities
  and armies), as in the original. You can fight on and view its map with `V`.
- **Fog of war** in the original's style: the map shows what you have seen, and
  cities and enemy units as you last saw them.
- **Satellites** fly themselves on random diagonals, bounce off the edges of the world
  and burn out, as in the original.
- **Retooling penalty**: switching production loses progress and adds a fifth of the
  new unit's cost, as in the original.
- Starting continents must have an open-ocean port (quality-of-life change).
- `-t` / `--torus` for a wrap-around map.
- Headless rig fighter statistics (built, landed, low-fuel landings, out of fuel,
  shot down, lost over cities, lost with carriers).

### Fixed
- Satellites could fly over land incorrectly.
- Transport deck capacity: armies above capacity could stay aboard, and armies
  stranded at sea when their transport moved on were not lost.
- Fighters on attack orders targeted cities they could not attack.
- A fighter landing now ends its move, as in the original.
- Armies in your own cities were prompted for orders when they shouldn't be.

### Changed
- Code cleanup in preparation for a client/server split: the game state (`Game`) no
  longer contains any screen state, which moved to `View`. Keys moved to `keys.rs`.
  `main.rs` only parses the command line.
- Saves no longer contain screen state. Old saves that do still load.

## 0.5.0-beta – 0.5.1-beta
- Versioned save format (0.5.1), with migration of older saves and validation that
  rejects damaged files.
- `TerminalGuard` always restores the terminal, including after a panic.
- Terrain as a proper type, saved in the original's characters.
- Damaged ships move at reduced speed and are repaired in port, as in the original.
- Sentries wake at the correct distance.

## 0.4.0-beta – 0.4.2-beta
- Seeded random number generator and `--seed` for reproducible worlds (0.4.2).
- Scoring.

## 0.1.0a – 0.3.1-alpha
- The initial Rust port: world generation, units, movement, combat, production, the
  ratatui interface, edit mode, standing orders (fill, upgrade, transport wait, land)
  and saving.

## Known issues
- In AI-vs-AI games, the player side always moves first within a turn.
- Very long, even games can build up more than 1500 units, and the computer's move
  then takes 100–200 ms (the original capped the board at 5000 pieces).
