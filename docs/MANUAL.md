# rusted-empire player manual

This manual covers how to play rusted-empire 0.9.0-beta. The rules are those of
VMS-Empire; where this port differs, the text says so. The README lists all the
differences in one place.

## The object of the game

Two empires share a world of continents, oceans and neutral cities: yours and the
computer's. Each starts with one city. Cities build units; armies capture more
cities; ships and aircraft carry the war across the sea. You win when the computer
has no cities and no units left, or when you hold every city on the world. The
computer may also concede first (see [The end of the game](#the-end-of-the-game)).

## Starting a game

```
rs-empire                 # new game
rs-empire --seed 42       # new game on a reproducible world
rs-empire -t              # new game on a wrap-around (torus) map
rs-empire --load my.sav   # resume a saved game
```

A new game asks three questions:

1. **Difficulty level.** The world is generated first, and the number of levels
   depends on it. Level 0 is easiest. As in the original, the level only decides
   which continents the two sides start on: the higher the level, the better the
   computer's start compared with yours. The computer plays the same way at every
   level.
2. **Autosave interval** in turns, or 0 for none. Autosaves overwrite
   `empire_auto.sav`.
3. **What your first city builds.**

## The screen

```
┌ rs Empire | Turn: 12 | Mode: Manual ───────┐┌ Info ─────────────┐
│                 map                         ││ status            │
│                                             ││ active unit       │
│                                             ││ legend            │
└─────────────────────────────────────────────┘└───────────────────┘
┌ Messages (Scroll: 0) ──────────────────────────────────────────────┐
│ the latest events                                                  │
└────────────────────────────────────────────────────────────────────┘
[prompt line: what the game is asking, and the keys that apply]
```

- **Map.** It follows the unit or city being asked about. Drag with the left mouse
  button to look elsewhere.
- **Info panel.** Your score, armies and cities, the enemy cities you know of, and
  the hits and moves of the active unit.
- **Messages.** Combat results, production, sentries waking, and so on. Scroll back
  with the mouse wheel; a new message scrolls back down.
- **Prompt line.** Always shows what is being asked and which keys answer it. A long
  prompt wraps onto a second line.

### Map symbols

| Symbol | Meaning |
|---|---|
| `.` | Sea |
| `#` | Land |
| (blank) | Not yet explored |
| `O` | Your city |
| `X` | Enemy city |
| `*` | Neutral city |
| `A F P D S C B T Z` | Your units (see the unit table) |
| `a f p d s c b t z` | Enemy units |

You see only what your units and cities can see right now (fog of war). Elsewhere
the map shows what you saw last time you looked, so enemy units and city owners may
be out of date.

## Units

| Key | Unit | Cost (turns) | Moves | Hits | Strike | Sight | Notes |
|---|---|---|---|---|---|---|---|
| `A` | Army | 5 | 1 | 1 | 1 | 2 | The only unit that can capture cities. Land only, or aboard a transport. |
| `F` | Fighter | 10 | 8 | 1 | 1 | 6 | Flies over land and sea. Fuel for 32 squares; refuels in your cities and on carriers. |
| `P` | Patrol boat | 15 | 4 | 1 | 1 | 4 | Fast scout. |
| `D` | Destroyer | 20 | 2 | 3 | 1 | 4 | Detects submarines up to 4 squares away. |
| `S` | Submarine | 20 | 2 | 2 | 3 | 3 | Hard to see: only noticed next to an enemy, or by a destroyer. |
| `T` | Transport | 30 | 2 | 1 | 1 | 3 | Carries up to 6 armies. |
| `C` | Carrier | 30 | 2 | 8 | 1 | 5 | Carries up to 8 fighters, and is a fuel base for them. |
| `B` | Battleship | 40 | 2 | 10 | 2 | 5 | The heaviest ship. |
| `Z` | Satellite | 50 | 10 | 1 | — | 10 | Flies itself: see [Satellites](#satellites). |

- **Cost** is how many turns a city takes to build one.
- **Strike** is the damage dealt by each successful blow in combat.
- **Sight** is how many squares the unit can see. In the original, every unit sees
  only the squares next to it and satellites see two; the longer ranges are a change
  in this port.
- A damaged ship moves more slowly, in proportion to its remaining hits, and a damaged
  transport or carrier holds proportionally less.

## Giving orders

When a unit has moves left and no standing order, it blinks and the prompt shows
something like:

```
[Army at (13,78) M:1]: [789/46/123] move,[S]entry,[Y]attack,[G]rope,[D]est,[R]nd,[V]ctr,[W]ake,[T]rnsWait:
```

### Moving

Move one square with the numeric keypad. The arrow keys also work wherever a
direction or a cursor is asked for.

```
7 8 9
4   6
1 2 3
```

Moving onto an enemy unit attacks it. Moving an army onto a city it doesn't own
assaults the city. If a move would destroy your own unit (an army walking into the
sea, a ship running ashore, a fighter flying into an enemy city), you are asked to
confirm first, in the original's words ("Troops can't walk on water, sir..."). A
fighter also warns you if a move would leave it without enough fuel to reach a base.

### Unit keys

| Key | Order | For | What it does |
|---|---|---|---|
| `Space` or `N` | Skip | all | Leave the unit for this turn. |
| `S` | Sentry | all | Rest until an enemy comes within sight. A fighter on sentry scrambles to attack instead of just waking. |
| `Y` | Attack | all | Hunt for the nearest enemy and attack it. |
| `G` | Grope | all | Explore unknown territory. |
| `D` | Destination | all | Pick a square with the cursor and `Enter`; the unit travels there over the following turns. |
| `V` | Vector | all | March in a direction until stopped. |
| `R` | Random | all | Wander at random. |
| `W` | Wake | all | Clear the unit's standing order. On a transport or carrier, also brings everything aboard up for orders (unloading or launching). |
| `T` | Wait for transport | armies | Board the next transport with room that comes alongside. |
| `L` | Land | fighters | Fly to the nearest base and land. |
| `F` | Fill | transports, carriers | Wait in place until full, then wake. |
| `U` | Upgrade (repair) | ships | Sail to the nearest of your cities and stay until fully repaired. |

A unit on a standing order that gets stuck (blocked, out of targets) comes back for
orders, and the prompt says which order it was on, for example `[STUCK: Dest]`.

### Turn and game keys

When no unit needs orders, the prompt reads
`[N]ext, [A]utomove, [M]anual, [S]ave, [L]oad, [Q]uit`. While a unit is being asked
about, `A`, `M`/`O`, `E` and `Q` work too; `N` and `S` mean skip and sentry there.

| Key | Action |
|---|---|
| `N` | End the turn. |
| `A` | Automove: play turns by themselves, carrying out standing orders, until a unit needs you. `O` or `M` stops it. |
| `M` or `O` | Back to manual play. |
| `E` | Edit mode (see below). |
| `S` | Save to `empire.sav` (in the folder you started from). |
| `L` | Load `empire.sav`. |
| `V` | After the computer has conceded, toggle a view of what it knows of the world. |
| `Q` or `Ctrl-C` | Quit (asks to confirm). Save first; there is no prompt to save. |

### Edit mode

Edit mode (`E`) gives you a free cursor on the map, so you can work on cities and
units without waiting for their turn. The prompt reads
`[Arrows/NumPad] Move | [P]rod | [O]rders | [W]ake | [I]nitial Orders | [E]xit Edit`.

| Key | Action |
|---|---|
| arrows / keypad | Move the cursor (the panel on the right shows its position). |
| `P` | Change what the city under the cursor builds. |
| `O` | Give an order to the unit under the cursor. |
| `W` | Wake units under the cursor: one type, or `*` for all. Waking a transport or carrier also brings up everything aboard. |
| `I` | Set a **standing order for new units**, either for the city under the cursor (local) or for every city (global). For example, every fighter built goes on sentry, or every army explores. A city's local order takes precedence over the global one. |
| `E` or `Esc` | Leave edit mode. |

## Cities and production

- A city builds one unit at a time. The unit appears in the city when the number of
  turns in the cost table has passed.
- **Choosing production.** You are asked when you capture a city. Change it any time
  in edit mode with `P`, using the unit keys `A T P D S C B F Z`. Ships can only be
  built in coastal cities.
- **Retooling.** As in the original, switching a city to a different unit throws away
  the work done so far and adds a retooling delay of a fifth of the new unit's cost.
  A newly captured city always pays the retooling delay.
- **Repair.** A ship that stays in one of your cities regains one hit per turn.
- **Fuel.** A fighter that lands in one of your cities or on one of your carriers is
  refuelled (see [Fighters](#fighters)).

## Combat

- **Unit against unit.** The fight goes in rounds. Each round, one side lands a blow
  at even odds, dealing its strike damage. The fight ends when one unit is out of
  hits. Strength comes from hits and strike: a battleship (10 hits, strike 2) beats a
  destroyer almost every time, while a submarine's strike of 3 makes it deadly to
  ships.
- **Stacks.** When several units share a square, ground units are fought first by
  ground attackers and air units by air attackers, then whatever remains.
- **Capturing a city.** Only an army can take a city. The assault succeeds half the
  time. Either way the army is used up: on success the city is yours, along with any
  defending armies destroyed; on failure the army is lost.
- **Fighters and ships cannot enter enemy or neutral cities.** A fighter that tries
  is shot down, and a ship breaks up on shore, as in the original.
- **Cargo.** Armies aboard a transport that sinks are lost, and so are fighters
  aboard a sunk carrier.
- **Score.** Each side scores the build cost of every enemy unit it destroys.

## Transports and carriers

An army boards a transport by moving onto it (or with the `T` order, waiting for one
to come alongside), and a fighter lands on a carrier the same way. Units aboard go to
sleep in the hold: they sail with the ship and are not asked for orders, so you can
send a loaded transport along a coast without being asked about its troops every
turn.

- **On arrival, the armies come up by themselves.** When a transport carrying armies
  finishes a standing order next to land (it reaches its destination, or a vector,
  explore, random or attack order stops), its armies come up on deck and are asked for
  orders. Move them ashore with the keypad.
- **`W` on the transport unloads at any time**, wherever it is. On a carrier, `W`
  launches its fighters. Fighters never come up by themselves.
- Units that come up on deck but stay aboard sail on with the ship, and are asked for
  orders each turn until they leave it or are put on sentry.

In the original, armies aboard a transport are asked for orders whenever it is next to
land; waiting until it arrives is a change in this port.

## Fighters

A fighter carries fuel for 32 squares, which is four turns at its full speed of 8.
Every square it flies uses one. Landing in one of your cities or on one of your
carriers refuels it and ends its move for the turn. If it runs out of fuel in the
air, it crashes. A move that would leave it unable to reach a base triggers a
warning, and the `L` order sends it to the nearest base. Fighters may fly over your
own units; in the original, other units blocked them.

## Satellites

A satellite sets off diagonally in a random direction as soon as it is built. It
moves 10 squares a turn, bounces off the edges of the world, and burns out after 50
turns. It sees 10 squares around it, cannot be attacked, and takes no orders: it
exists to scout.

## The end of the game

- **Victory:** the computer has no cities and no units left, or you hold every city.
- **Defeat:** you have no cities and no units left.
- **The computer concedes** when it has fewer than a third as many cities as you and
  fewer than a third as many armies, as in the original. You can accept the victory
  or fight on. If you fight on, the `V` key shows what the computer had learned of
  the world.

## Saving and loading

- `S` saves to `empire.sav` and `L` loads it, in the folder the game was started from.
- `--load FILE` starts with any save file.
- Autosaves go to `empire_auto.sav` every *N* turns, if you chose an interval.
- A save is written to a temporary file first and then renamed, so a crash while
  saving never damages the previous save.
- Saves from every earlier version of rusted-empire load. A save from a newer version
  is refused with an explanation instead of loading wrongly.
