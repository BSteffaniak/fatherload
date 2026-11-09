# drillgame_game

Desktop raylib frontend for Drillgame.

Single-player and local split-screen run on `drillgame_netcode::HostSession<WorldState>` with
one local seat per player and no remote clients; that host is the only gameplay authority.

Frame loop (`local_game::LocalGame::frame`, driven by `app::run`):

1. Sample keyboards and controllers and route them to seats (`app` + `input::router`); each
   seat's active device goes to the shell for its button prompts.
2. Every seat's input goes to `shell::ShellState::update`. The shell holds the global screens
   (title, the shared pause menu, options, save/load slots, exit prompts, online lobby),
   settings and global messages, plus one `shell::SeatUi` per local seat: outdoors or inside a
   building (zone, position, facing), its counter panel or personal dialog (inventory, map,
   help, research log, crafting...), selection, pending services, service animation, messages
   and receipts. While playing, each seat's input drives only its own `SeatUi`; any seat's
   pause opens the shared pause menu and that seat (`ShellState::menu_seat`, with
   `ShellState::paused_by` naming who paused) drives it; seat 0 takes over when that seat's
   controller is unplugged. Every seat's press of a frame lands even when another seat pauses
   on it, and actions picked on a frame that loads or starts a world are dropped. Menu
   choices become `ShellRequest`s (`Action { seat, command }` for service/rescue actions,
   saves, loads, new game, extra seats). Nothing in a `SeatUi` is saved; loads put every seat
   outdoors.
3. Each seat's held controls (only while it is outdoors with nothing open) and one-shot actions
   go to `runtime::Runtime` as that seat's player's, which advances 0..n fixed ticks from real
   time. The local host ticks while the game runs and at least one seat is outdoors
   (`ShellState::world_should_tick`): alone, a building pauses the world as always; in
   split-screen it keeps running until every seat is inside or someone pauses. Title and pause
   stop it; online hosts never pause.
4. `render_snapshot::RenderSnapshot` is built from the world: one view per seat (camera follows
   that seat's rig, map fog from that player's `DiscoveredTiles`), terrain, hazards, bombs,
   infrastructure, HUD values and this frame's de-duplicated gameplay events. Terrain redraws
   follow the host's `WorldOp::Tile` ops (or the client's terrain diff).
5. `rendering::GameRenderer` draws the snapshot plus shell state; audio plays the frame's cues.
   Each view draws its seat's world and HUD, or the building the seat is in, with the seat's
   dialog, rescue prompt and toasts on a UI canvas covering just that view
   (`ui_kit::UiCanvas::for_viewport`) and prompts for the seat's device; dialog focus and
   scroll are kept per seat. Only global screens cover the whole window.

Modules:

- `runtime` — `Runtime { Host, Client }`, `LocalHost`, `MAX_LOCAL_SEATS` (the only seat cap).
- `shell` — shell state and menu logic; reads the world, never mutates it.
- `render_snapshot` — immutable per-frame view, the N-view split-screen layout for the current
  window size, and the world zoom (see `pixel_scale`).
- `pixel_scale` — the pixel-perfect scaling policy: world zoom snapped to whole physical pixels
  per art pixel (`WORLD_VIEW_HEIGHT` is kept within one rounding step at any window size) and the
  integer UI canvas scale.
- `window` — the resizable, maximizable high-DPI window, borderless fullscreen and the
  remembered window size.
- `effects` — exactly-once event delivery, sound cues and presentation particles (sparks, bomb
  blasts).
- `rendering` — `GameRenderer`: `terrain` (chunked art-resolution textures of procedural pixel
  tiles + dithered fog), `world` (sky and parallax, the town, rigs, props and effects),
  `screen` (HUD, menus, map), `interior` (walk-in pixel-art rooms), `layout` (menu/modal
  layout), `layout::widgets` (widget tree), `ui_kit` (pixel UI canvas and widgets),
  `pixel_art` (palette, char-grid sprites + atlas, pixel font) and `gallery` (art reference
  sheet). Art conventions: `src/rendering/ART_GUIDE.md`.
- `scene_capture` — `drillgame --screenshot-scenes <dir> [--size WxH] [--only a,b] [--list]`
  renders scripted scenes to PNGs (see the art guide). Scenes run in the default world (fixed
  seed `D111-6A4E`); scenes that need a particular place (`hazard_telltales`,
  `strata_boundary`) find it through the world's feature index rather than fixed coordinates,
  and the `poi_*` scenes swap in the first world from the default seed on that holds the POI
  they show (`stage_poi`).
- `worldgen_tools` — `drillgame --worldgen-report [--seeds N] [--first-seed S] [--size X]
  [--richness X] [--hazards X] [--legacy] [--generator v1|v2|v3]` prints generator distributions over many seeds, and
  `drillgame --worldgen-preview <seed> <out.png> [--size X] [--richness X] [--hazards X]
  [--scale N] [--no-features]` writes a whole-world map PNG through raylib's encoder without a
  window (points of interest get their kind's outline and map icon).
- `shell::quests` — quests in the shell: each keeper's counter (fuel, repair, depot, HQ,
  explosives, salvage) has a Quests tab (left/right or LB/RB) listing jobs to turn in, jobs on
  offer, jobs in progress (confirm twice to drop) and the next locked ones; while other rigs
  play, a co-op job has two rows: take it as a crew job, or take its solo variant. The quest log flips with the mine map and the collection
  log (map -> log -> quests -> map) and its selection is the seat's tracked quest
  (`SeatUi::tracked_quest`); `SeatUi::fanfare` is the banner raised for a finished, ready,
  failed job or a completed blueprint (`LocalGame::announce_quests`). `rendering::quests` draws
  the desk, the log, the HUD tracker, the fanfare and map/minimap markers;
  `rendering/world/quest_marks.rs` draws them in the world.
- `shell::world_setup` — the New World dialog (seed, size, richness, hazards, test mode) opened
  by every "new world" title entry; the seed comes from OS randomness (`random_world_seed`, 48
  bits). `local_game::world_from_setup` builds the world (test mode via
  `WorldState::enable_test_mode`).
- `shell::test_mode` — the pause menu's test-mode entries (Warp to Layer list, invulnerable hull
  and infinite fuel toggles), each a `PlayerCommand::TestMode` action for the seat driving the
  menu; a paused local world applies toggles on resume, so the menu shows the pending choice
  (`ShellState::pending_test_mode`) until the world catches up.
- `save` — format v6 (`WorldState` + play time; the terrain as generation settings plus tile
  diffs, see the simulation README); `save::legacy` migrates v1-v5 (v1-v5 terrain wrote out
  every tile and is read as `FullTerrain`; v3/v4 worlds carried a derived summary whose
  `won_game` flag is lifted into the world). A save naming an unknown generator version fails
  with `SaveError::UnknownGenerator`. A fresh world's save is ~3.4 KB (v5: ~1.8 MB), a heavily
  dug one ~43 KB (v5: ~1.8 MB).
- `input` — devices to per-seat `PlayerInput`:
  - keyboard sampling (`input.rs`; solo reads WASD + arrows, split-screen a WASD and an
    arrow-keys scheme: arrows drive and navigate menus, Right Ctrl / Numpad Enter use and
    confirm, Right Shift backs out, Right Alt pauses, Numpad 1/2 inventory/map, Numpad 0/. bomb
    and scanner; see `read_secondary_keyboard_input`);
  - `gamepad` — the controller layout as one binding table (`PAD_BINDINGS`, Xbox names by
    position) and the pure `PadMapper` (sampled `PadState` -> `PlayerInput`: deadzone, edges,
    stick + d-pad folded into one direction, menu auto-repeat from `repeat`; the right stick
    (`SCROLL_AXIS`) scrolls dialogs in proportion to its deflection, as frame-time scaled
    wheel notches in `PlayerInput::ui_scroll`);
  - `seats` — the seat policy: solo = keyboard + every pad; split-screen = WASD seat,
    arrow-keys seat, pads fill seats without one (keyboardless, then arrows, then WASD) whenever
    the seat count changes, a spare pad claims a free or new seat with A/Start, a pad keeps its
    seat while unplugged, and a seat's keyboard keys are ignored while its pad is plugged in;
  - `router::InputRouter` — per frame: hot-plug notices (toasts), seat routing, claims and
    each seat's active device (`devices::SeatActivity`: the last device with *new* input, so
    stick noise inside the deadzone or a held button never switches it);
  - `prompts` — action prompts (`Prompt::Use`, `Back`, `Confirm`...) to keycaps or pad glyphs
    for a device, plus their text form for toasts;
  - `pads` — raylib/GLFW sampling and `drillgame --list-gamepads` (opens a small window for
    about a second, prints each pad's index, name, axes/buttons, standard mapping, GUID).
- `input_mapping` — seat input to held controls and one-shot actions.

- `online` — online play on the same runtime:
  - `bridge` maps netcode `ClientMessage`/`ServerMessage` to `quic_transport` bytes (client
    inputs as datagrams, everything else on the reliable control stream);
  - `host::HostNet` feeds a `HostSession` from the network thread (non-blocking drain each frame)
    and publishes the session on the LAN;
  - `client::OnlineClient` wraps a `ClientSession` (`Runtime::Client`): predicted world for
    terrain/HUD, predicted + smoothed local rig, interpolated remote rigs. The client only ever
    holds its own player's loadout and discovery (other rigs show their public build), so its
    F3 hash status compares the host's client-visible hash;
  - `lan` runs mDNS browsing and descriptor fetches on helper threads;
  - `bots` + `headless`: `drillgame --headless-host [addr] [--lan] [--duration S]` and
    `drillgame --bot-client <addr> [--count N] [--duration S]` (no window, periodic stats lines).

Online flow: Title > Host Online Game (lobby with address, descriptor copy, roster, Start) or
Join Online Game (address, pasted descriptor, or LAN list), lobby ready toggle, Leave/End,
reconnect with the token after a dropped connection, and notices when the host ends the session,
rejects the join, or the connection is lost. A running single-player game can be opened from the
pause menu. Online pause is a local overlay (the world keeps running). Only the host saves (manual
and a periodic autosave); joined clients cannot save or load. F3 toggles the network overlay; in
dev builds F4 / Page Up / Page Down tune live `NetTuning` values on the host while the overlay is
open (otherwise Page Up / Page Down scroll dialogs; `ShellState::tuning_keys_active`). The
`DRILLGAME_NET_SIM` link conditioner applies to every online session.
