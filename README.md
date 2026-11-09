# Drillgame

A Rust-native 2D mining game mechanically inspired by classic drill/mining games, with original names, procedural programmer art, and a desktop-first initial target.

## Development

```bash
cargo fmt
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
cargo run --bin drillgame
```

With Nix:

```bash
nix develop -c cargo run --bin drillgame
```

### Windows

Builds natively with the MSVC toolchain (`x86_64-pc-windows-msvc`). raylib is compiled from C
source, so it needs:

- Visual Studio 2022 Build Tools with the C++ workload (provides `cl` and a bundled CMake under
  `Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin`; put that or any CMake on `PATH`).
- `libclang.dll` for raylib's bindgen step: install LLVM, or unpack the `libclang` wheel from
  PyPI, and set `LIBCLANG_PATH` to the folder containing the DLL.

```powershell
$env:LIBCLANG_PATH = "C:\path\to\folder\with\libclang"
cargo build --release -p drillgame_game   # target\release\drillgame.exe
```

Saves and settings go to `%LOCALAPPDATA%\drillgame`.

## Current Gameplay

Drill below the surface, gather minerals and artifacts, manage fuel/hull/cargo/debt, complete HQ contracts, buy upgrades, use scanners and bombs, and push deeper toward the Star Core. Deeper layers add harder rock, richer ore, artifacts, gas, lava, magma vents, pressure pockets, explosive pockets, falling boulders, cave-ins, and stronger heat pressure.

### Worlds

Every new game is a different world. "New Game", "Local Split-Screen" and "Host Online Game"
open a **New World** dialog: a random seed (re-roll it, type hex digits or any words, or paste
with `Ctrl+V`), world size (standard 240 tiles wide or large 360), resources (lean / standard /
rich), hazards (calm / standard / brutal) and **test mode**. The same seed and settings make
the same world on every machine; the pause menu shows the seed and copies it to share. Online,
the host's settings define the session.

**Test mode** is for exploring the layers quickly (a god mode): every rig starts with every
upgrade maxed (drill mount included), every rig part in the locker and 1,000,000 credits. The
pause menu adds **Warp to Layer** (drops your rig into a safe carved pocket at the top of any
layer), **Invulnerable Hull** and **Infinite Fuel** (for the whole crew). Depth records and
deep-reward milestones do not count. Test-mode worlds are marked on the HUD, the pause menu,
save slots and the title's last-save line; it works in split-screen and online (the host
chooses it).

Worlds keep a familiar town and a fixed order of layers (topsoil, clay beds, grey stone, slate,
basalt, then seven deep strata), but each seed picks its own layer thicknesses (so a layer can
start a dozen rows higher or lower than in another world), waves the layer boundaries, shapes
soft channels and hard shelves, and places its own caverns, worm tunnels, deep chambers, ore
veins and pods, rare motherlodes, artifact beds and clustered hazard pockets. Rock next to a
hazard pocket shows tell-tale stains, so careful drillers can read danger before breaking in.
Every world guarantees a starter ore field near spawn, an ore budget per layer, and a path down
through each layer with that layer's drill.

### Points of interest

Hand-built places are stamped into every world (generator v2 and later; worlds and saves from
older generators keep generating exactly as before; v3, the current generator, adds per-seed
layer thicknesses and fewer, chunkier deep ore deposits on rich worlds):

- **Supply caches** (every layer below the topsoil): hollows of crates. Drill a crate open for a
  fuel canister, a bomb or two, or some credits.
- **Derelict rigs** (mid to deep): crashed prospector rigs. Drill the glowing core for salvage, a
  layer material, sometimes a part, and a log entry for your collection. Loot (crates, cores,
  lockers, vault prizes) shrugs off bombs, so a blast never wastes it.
- **Vaults** (basalt and the deep strata): sealed rooms with walls no drill or bomb gets through.
  Open the lock to reach the prize: drill the floor switches in the order their pips show (a
  wrong one resets them), bomb every pressure seal in the walls, or drill both relays within 15
  seconds of each other (a timer bar over the relays shows what is left).
- **Formations**: fossil beds packed with artifacts and geodes lined with rich ore, guarded by
  their layer's hazard.
- **Rival claims**: abandoned outposts whose lockers hold infrastructure kits.

A scanner hears a "strange echo" from sites in range; from tier 3 it outlines them and shows
their kind, in the world, on the minimap and on the map, and a scanner pulse records them. The
map shows every site you have found or detected; confirm on the map opens the **collection
log** (sites visited, scanner contacts and recovered logs). Rewards are a top-up, never a
replacement for selling ore (reward tables: `packages/simulation/src/poi.rs`).

World generation tools (no window opens):

```bash
cargo run --release --bin drillgame -- --worldgen-report --seeds 300 [--size large] [--richness rich] [--hazards brutal] [--generator v2]
cargo run --release --bin drillgame -- --worldgen-preview D111-6A4E world.png [--size large] [--scale 4]
```

The report prints per-layer top row, ore value, ore specks (deposits of 1-3 tiles), cave share, hazard tiles, artifacts and points of
interest plus feature, POI-kind and hazard-cluster counts (min / mean / max over the seeds); the
preview writes a whole-world map (layers, caves, ores, hazards, artifacts, with motherlodes,
artifact beds and chambers outlined, and each point of interest outlined with its map icon).

Successful return trips award streak bonuses. Emergency rescue returns the rig to the surface for a fee, drops part of the cargo at the rescue site, and records a depot invoice; lost cargo can be recovered later if there is cargo space.

## Controls

- `A` / `D` or left/right arrows: move horizontally and drill sideways while resting on the
  ground (in flight too with the gimbal drill mount)
- `W` / up arrow / space: thrust upward (with the crown bore, thrusting into a ceiling drills it)
- `S` / down arrow: drill downward (while resting on the ground)
- `C`: trigger an active scanner pulse when a scanner is installed
- `B`: place a purchased timed bomb underground
- `E` / `Enter`: interact with buildings/interior counters, confirm menus, or start game
- `Backspace` / `Esc`: close menus; `P` pauses/resumes
- `M`: large mine map with ore, hazard, rescue, and lost-cargo markers
- `H`: help screen
- hold `Tab`: detailed cargo/contract/status overlay
- `1`-`6`: quick-select upgrades while the shop is open
- `F5` / `F9`: quick save/load using `save.json` in the state directory
- Pause menu: save/load slots, options, exit confirmation
- `F11`: borderless fullscreen toggle (the window also resizes, maximizes and uses macOS
  native fullscreen from the green button)
- `+` / `-`: volume hotkeys

### Controllers

Xbox, PlayStation and Nintendo controllers (anything GLFW has a standard mapping for) work out
of the box, over USB or Bluetooth, and can be plugged in or out mid-game. Prompts switch to the
last device used: keyboard keys as keycaps, or the controller's own button glyphs.

| Action | Controller (Xbox names) | Keyboard |
| --- | --- | --- |
| Move / drill sideways | Left stick or d-pad left/right | `A` / `D` |
| Thrust | `A`, `RT`, or up on stick / d-pad | `W` / Space |
| Drill down | Down on stick / d-pad | `S` |
| Use building, counter, door | `X` | `E` |
| Confirm in menus / call rescue | `A` | `Enter` |
| Back / close / leave building | `B` (never pauses or quits) | `Esc` / `Backspace` |
| Menu navigation (auto-repeats) | Left stick or d-pad | arrows / `W` `S` |
| Pause | `Menu` (Start) | `Esc` / `P` |
| Inventory | `Y` | `Tab` |
| Map | `View` (Back) | `M` |
| Scanner / bomb | `LB` / `RB` | `C` / `B` |
| Previous / next tab or column | `LB` / `RB` in menus | left / right arrows |

The layout lives in one table (`PAD_BINDINGS` in `packages/game/src/input/gamepad.rs`).
`drillgame --list-gamepads` lists every detected controller (index, name, axis and button
counts, whether GLFW has a standard mapping); without a mapping a pad's buttons cannot be read.
Rumble is not available: raylib's desktop GLFW backend does not implement
`SetGamepadVibration`.

### Local split-screen

Up to four players share the screen, each in their own view. Player 1 uses WASD, player 2 the
arrow keys, and connected controllers fill seats without one (keyboardless seats first, then
the arrow-keys seat, then WASD). A spare controller joins with `A` or `Start`, taking a free
seat or adding one (up to four). A seat with a plugged-in controller is driven by that controller
only; its keyboard keys take over again while the controller is unplugged. Playing alone, the
keyboard and every controller drive the rig.

Every player has their own menus: each walks into any building, uses its counter, reads their
own inventory or map and leaves again inside their own view (with their own button prompts,
selection, scrolling and messages), while the others keep playing. Purchases and sales go to
the player who made them.

| Action | WASD seat (player 1) | Arrow-keys seat (player 2) |
| --- | --- | --- |
| Drive / walk, thrust, drill | `A` `D`, `W` / Space, `S` | left / right, up, down |
| Use building, counter, door | `E` | `Right Ctrl` or `Numpad Enter` |
| Confirm in menus | `Enter` / `E` | `Right Ctrl` or `Numpad Enter` |
| Back / close / leave building | `Esc` / `Backspace` | `Right Shift` (never pauses) |
| Menu navigation | `W` `S` (adjust `A` `D`) | up / down (adjust left / right) |
| Pause (shared pause menu) | `Esc` / `P` | `Right Alt` (Right Option) |
| Inventory / map | `Tab` / `M` | `Numpad 1` / `Numpad 2` |
| Bomb / scanner | `B` / `C` | `Numpad 0` / `Numpad .` |
| Scroll a dialog | mouse wheel, `Page Up` / `Page Down` | (the selection scrolls it) |

Controllers use their usual buttons (table above) in their own seat, with the right stick
scrolling that seat's dialogs. Global keys (quick save/load, fullscreen, volume, `F2`, `F3`,
help) stay on the main keyboard.

Any player's pause opens one shared pause menu, driven by the player who opened it, which
pauses everyone (save, load, options and exit work as alone). If that player's controller is
unplugged, player 1 (keyboard) takes the menu over; the window's close prompt is always answered
by player 1 and then hands the pause menu back. The world keeps running while at
least one player is outdoors, and rests only once every player is inside a building, just as
playing alone pauses while you are inside. Online sessions never pause. What a player has open
is not saved: after loading, everyone starts outdoors.

## Surface Buildings

Press `E` (controller `X`) at a surface building to walk inside its room. Move with `A`/`D` or the stick, use the service counter with `E`/`X`, and leave through the door or `Backspace`/`Esc` (controller `B`). In split-screen each player enters and uses buildings on their own (see above).

- Fuel Station: refills fuel in partial or full service increments
- Repair Garage: repairs hull in partial or full service increments
- Ore Depot: sells mineral/artifact cargo and shows receipt history
- HQ: completes contracts, provides named story/radio briefings, and offers/collects debt advances
- Upgrade Shop: sells drill, fuel tank, cargo bay, engine, hull, radiator, scanner, and bomb-pack upgrades,
  plus the late-game **Drill Mount**: the Gimbal Drill Mount (6,000 cr, on sale once anyone reaches
  the Crystal Faults) drills sideways in flight, and the Crown Bore Mount (24,000 cr, once anyone
  reaches the Pressure Cathedrals) drills up into a ceiling you thrust against

## Multiplayer Status

Multiplayer is being rebuilt per `multiplayer-architecture-plan.md`:

- `packages/simulation`: the canonical `WorldState` / `simulate_tick` (all gameplay, including
  town services, runs inside fixed ticks).
- `packages/netcode`: sans-IO `HostSession` / `ClientSession` with prediction, interpolation and
  a link simulator, proven end to end by its harness.
- `packages/quic_transport`: the Quinn network thread, framing, datagrams, link conditioner and
  LAN discovery.
- `packages/game`: single-player and local split-screen (up to 4 seats; `F2` adds a seat, the
  title menu's "Local Split-Screen" starts with two) run on `HostSession` with local seats.

Online host/join is hidden in the menus until it is wired onto the same runtime.

## Saves and Settings

Files live in `$DRILLGAME_STATE_DIR`, else `$XDG_STATE_HOME/drillgame`, else
`%LOCALAPPDATA%\drillgame` on Windows, else `~/.local/state/drillgame`.

- Quick save: `save.json`; save slots: `save-slot-1.json` through `save-slot-3.json`
- A save (format v6) is the host's `WorldState` plus play time. Its terrain is stored as the
  world's generation settings (seed, size, richness, hazards, generator version) and only the
  tiles that differ from freshly generated terrain; loading regenerates the rest and checks a
  terrain digest. v1-v5 saves (which wrote out every tile) are migrated on load and re-saved as
  v6; saves from before seeded worlds keep their terrain and load as legacy worlds. A save from
  an unknown generator version is refused with a clear error. Loading starts a new session
  epoch.
- Slot UI displays depth and credits; the title shows the latest save's depth, credits and playtime.
- Settings are stored in `settings.json` and persist volume/fullscreen preferences and
  the window size/maximized state.
