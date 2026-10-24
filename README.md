# rusted-empire

A Rust port of **VMS-Empire**, the classic 1970s–80s strategy game that is an
ancestor of Civilization and every 4X game since. You and the computer each start
with one city on an unexplored world. Cities build armies, fighters, ships and
satellites; you explore, expand, and fight until only one empire is left.

This port aims to play **true to the original**: the same world generation, the same
rules of movement, combat and production, and a faithful translation of the original
computer player. It adds a modern terminal interface (panels, a scrollable message
log, mouse support) and a short list of quality-of-life changes, all listed below.

**Status: 0.9.0-beta.** The game rules, the computer player and the terminal
interface are covered by automated tests (including random-input fuzzing of the
interface), and by thousands of simulated AI-vs-AI games. It's a beta while it gets
real-world play: please report anything that looks wrong.

- Player manual: [docs/MANUAL.md](docs/MANUAL.md)
- Developer guide: [docs/DEVELOPING.md](docs/DEVELOPING.md)
- Changes: [CHANGELOG.md](CHANGELOG.md)
- License and credits: [LICENSE](LICENSE) (GNU GPL v2), [NOTICE](NOTICE)

## Building

You need a Rust toolchain (stable; 1.74 or newer should work). The game runs in an
80 × 24 terminal; a larger one shows more of the 60 × 100 map.

```
git clone https://github.com/tymrek/rusted-empire.git
cd rusted-empire
cargo build --release
```

The program is `target/release/rs-empire`. It runs on Linux, macOS and Windows
terminals that crossterm supports.

## Quick start

```
./target/release/rs-empire
```

1. Pick a difficulty level. 0 is easiest; the highest level offered depends on the
   world that was generated. The level decides which continents the two sides start
   on, as in the original.
2. Pick an autosave interval in turns (0 turns it off). Autosaves go to
   `empire_auto.sav`.
3. Choose what your first city builds. `A` (army) is the usual opening.
4. Units that need orders blink one at a time. Move them with the numeric keypad
   (`7 8 9 / 4 6 / 1 2 3`) or give an order: `S` sentry, `G` explore ("grope"),
   `D` go to a destination, and so on. The prompt line always lists the keys that
   apply.
5. When nothing needs orders, press `N` to end the turn.

See [docs/MANUAL.md](docs/MANUAL.md) for every key, the units, and the rules.

## Command-line options

| Option | Meaning |
|---|---|
| `--load FILE`, `-l FILE` | Resume a saved game. |
| `--seed N` | Make a new game reproducible: the same seed gives the same world. |
| `-t`, `--torus` | New game on a map that wraps around at its edges. |
| `--headless` | Run AI-vs-AI test games instead of playing (see below). |

### Headless test rig

`--headless` plays the computer player against itself on full-size worlds and prints
per-game results, fighter statistics and a summary. It also writes
`playtest_metrics.csv`. It is a tool for checking the AI and measuring the effect of
rule changes. You don't need it to play.

| Option | Default | Meaning |
|---|---|---|
| `--games N` | 100 | Number of games. |
| `--seed N` | random | First seed; game *k* uses seed N + k − 1. |
| `--small` | off | 30 × 30 world for quick runs. |
| `--level N` | fairest | Difficulty level. Default: the level whose two continents are closest in value. |
| `--turns N` | 3000 | Stop a game at this turn. |
| `--stalemate N` | 500 | End a game when neither side has reached a new high in cities for N turns. |
| `--original-sight` | off | Experiment: use the original's sight rules (see below). |
| `--adjacent-subs` | off | Experiment: destroyers find submarines only next to them. |

## Differences from the original

The rules follow VMS-Empire 1.13. These are the deliberate changes:

**Interface**
- A ratatui interface: map, status sidebar, scrollable message log, and a prompt line
  that lists the keys that apply. The unit or city being asked about blinks.
- Arrow keys work wherever directions are asked for. The mouse wheel scrolls the
  message log, and dragging pans the map.
- Fog of war is drawn from what you have actually seen. Cities and enemy units show as
  you last saw them until you look again.
- Warnings before a fighter move that would leave it without fuel to get home.
- Saves are versioned JSON. Saves from every earlier version of this port still load.
- `--seed` for reproducible worlds; `-t` for a wrap-around (torus) map.

**Rules**
- **Fighters can fly over friendly units.**
- **Sight ranges depend on the unit** (armies 2, transports and submarines 3, patrol
  boats and destroyers 4, carriers and battleships 5, fighters 6, satellites 10).
  In the original, everything sees only the squares next to it, and satellites see
  two.
- **Sentries wake when an enemy comes within sight**, not only when one is next to
  them. A fighter on sentry scrambles to attack.
- **Destroyers detect submarines up to 4 squares away.** In the original, submarines
  are seen only when next to something.
- **Carriers count as bases** for fighter fuel and range warnings.
- **Armies aboard a transport sleep until it arrives.** They come up for orders when
  the transport finishes a standing order next to land (or when you wake it with `W`),
  so a loaded transport can sail along a coast without asking about its troops every
  turn. In the original, they are asked whenever the transport is next to land.
- **Starting continents must have an open-ocean port**, so neither side can be stuck
  on a continent whose only coast is a lake.
- Cities are placed at a fixed minimum spacing, close to the original's derived value.

The sight, sentry and submarine changes are under review during the beta. The
headless rig's `--original-sight` and `--adjacent-subs` switches measure what they
change.

**Kept on purpose, because the original does it this way:** the computer player
ignores fighter fuel when it attacks, and the difficulty level only chooses the
starting continents.

## License and credits

rusted-empire is free software under the **GNU General Public License, version 2**
(see [LICENSE](LICENSE)), the license of the VMS-Empire sources it is derived from.

Empire was created by Walter Bright. The C version this port descends from was
written by Chuck Simmons (Copyright © 1987, 1988 Chuck Simmons), building on work by
Ed James and Craig Leres, and was maintained by Eric S. Raymond as VMS-Empire, with
contributions from Michael Self, James T. Jordan, Walt Stoneburner, Joe Wells,
Andrew Morrow and Dennis Pinckard. The Rust port is by James T. Jordan
<kermyt@gmail.com>. The full lineage is in [NOTICE](NOTICE).
