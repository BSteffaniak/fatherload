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

## Current Gameplay

Drill below the surface, gather minerals and artifacts, manage fuel/hull/cargo/debt, complete HQ contracts, buy upgrades, use scanners and bombs, and push deeper toward the Star Core. Deeper layers add harder rock, richer ore, artifacts, gas, lava, magma vents, pressure pockets, explosive pockets, falling boulders, cave-ins, and stronger heat pressure.

Successful return trips award streak bonuses. Emergency rescue returns the rig to the surface for a fee, drops part of the cargo at the rescue site, and records a depot invoice; lost cargo can be recovered later if there is cargo space.

## Controls

- `A` / `D` or left/right arrows: move horizontally and drill sideways while grounded
- `W` / up arrow / space: thrust upward
- `S` / down arrow: drill downward
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
- `F11`: fullscreen toggle
- `+` / `-`: volume hotkeys

## Surface Buildings

Press `E` at a surface building to walk inside its room. Move with `A`/`D`, use the service counter with `E`, and leave through the door or `Backspace`/`Esc`.

- Fuel Station: refills fuel in partial or full service increments
- Repair Garage: repairs hull in partial or full service increments
- Ore Depot: sells mineral/artifact cargo and shows receipt history
- HQ: completes contracts, provides named story/radio briefings, and offers/collects debt advances
- Upgrade Shop: sells drill, fuel tank, cargo bay, engine, hull, radiator, scanner, and bomb-pack upgrades

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
`~/.local/state/drillgame`.

- Quick save: `save.json`; save slots: `save-slot-1.json` through `save-slot-3.json`
- A save (format v4) is the host's `WorldState` plus play time; v1-v3 saves are migrated on load,
  and loading starts a new session epoch.
- Slot UI displays depth and credits; the title shows the latest save's depth, credits and playtime.
- Settings are stored in `settings.json` and persist volume/fullscreen preferences.
