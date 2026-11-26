# fatherload_game

Desktop raylib frontend for Fatherload.

Single-player and local split-screen run on `fatherload_netcode::HostSession<WorldState>` with
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
5. `rendering::GameRenderer` draws the snapshot plus shell state; `audio::AudioBus` hears the
   frame's placed cues plus the snapshot (thrust, drilling, impacts, alarms, nearby hazards).
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
- `effects` — exactly-once event delivery, placed sound cues (event -> cue table in
  `audio::cues`) and presentation particles (sparks, bomb blasts).
- `audio` — procedural sound (no audio files): a synth engine renders 41 designed patches with
  seeded variations at startup, a software mixer plays them on raylib's audio thread (voices,
  loops, pan, priorities, ducking, reverb, limiter) and a director turns each frame into mixer
  commands, panning each seat's sounds toward its split-screen view and attenuating world
  sounds by distance. `fatherload --render-audio <dir> [--seed N]` writes every patch to WAV with
  a level report and a showcase. Design table and details: `src/audio/README.md`.
- `rendering` — `GameRenderer`: `terrain` (chunked art-resolution textures of procedural pixel
  tiles + dithered fog), `world` (sky and parallax, the town, rigs, props and effects),
  `screen` (HUD, menus, map), `interior` (walk-in pixel-art rooms), `layout` (menu/modal
  layout), `layout::widgets` (widget tree), `ui_kit` (pixel UI canvas and widgets),
  `pixel_art` (palette, char-grid sprites + atlas, pixel font) and `gallery` (art reference
  sheet). Art conventions: `src/rendering/ART_GUIDE.md`. The upgrade shop (`counters`) lists
  all fourteen lines in one scrolling list (core upgrades, then the hazard counters with their
  own icons), each row showing `Tier n/max` and the footer the next tier's effect
  (`economy::upgrade_effect`); the inventory lists every line's tier; HUD bars whose maximum
  passes 999 show just the reading (`ui_kit::compact_amount`) so large tanks and hulls fit.
- `view_frame` (F7, wave 7) — presentation draws in the presented world's local pixel frame
  (`fatherload_simulation::position::Frame`, set from `RenderSnapshot::build` and after each
  runtime step): every position the simulation hands out is relative to it, so the camera's
  sector is the frame origin and `f32` drawing stays exact however far out play goes. Every
  tile-to-pixel conversion in rendering, effects, audio and the HUD goes through
  `view_frame::frame()` (`col_px`, `rel_col`, `rel_row`, `tile_center`, `col`, `row`,
  `col_ceil`, ...; the ground line is `rendering::world::ground_y()`), parallax layers use
  `view_frame::parallax_x/y` (absolute place, so a frame move never makes them jump), effects
  kept in pixels follow a frame move (`Effects::reframe`) and an online client's pose overrides
  rebase onto the rendered world's frame (`Pose::rebased`). Split-screen seats share the
  world's frame (one per world until F11). Patterns anchored to the world grid (bedrock, weather
  cells, wind streaks) use `view_frame::pattern_start_x` / `pattern_phase_x/y` /
  `pattern_anchor_x`, and animation phases seeded by place (light flicker, waterfall wobble,
  treads, people's steps, bolt jags) use `view_frame::phase_x/y`, so a frame move never makes
  them jump; art- and physical-pixel snapping round halfway cases up on both sides of zero
  (`pixel_scale::round_half_up`) for the same reason.
- `scene_capture` — `fatherload --screenshot-scenes <dir> [--size WxH] [--only a,b] [--list]
  [--frame-timing N] [--export-saves] [--frame-shift X,Y]` renders scripted scenes to PNGs (see
  the art guide). `--frame-shift X,Y` re-expresses every staged world in a frame X sectors
  right and Y down of its staged one (keeping whether it floats) before its frames run: nothing
  moves in the world, so diffing against an unshifted run finds drawing that still reads
  positions as world pixels (wave 7 review: 381 of 444 scenes pixel-identical, the rest under
  1 %, GPU seam rounding and `f32` physics at a different magnitude). Scene scripts that place
  actors after staging go through `world.frame()`.
  `far_billion_tiles` and `far_frame_hop` show play 10^9 tiles out and a frame hop mid-drive.
  `--frame-timing N` then draws N more frames per scene and prints the CPU time of the frame
  work (snapshot, renderer sync and draw calls; not the swap): mean, median, p95 and max.
  `--export-saves` opens no window and writes each scene's staged world as a save
  (`<dir>/<scene>.json`) for `scripts/perf-web.cjs`, which loads them into the WASM build and
  times every animation-frame callback (see "Browser frame timing" below). Scenes run in the default world (fixed
  seed `D111-6A4E`); scenes that need a particular place (`hazard_telltales`,
  `strata_boundary`) find it through the world's feature index rather than fixed coordinates,
  and the `poi_*` scenes swap in the first world from the default seed on that holds the POI
  they show (`stage_poi`). The `enemies_*` scenes (`scene_capture/enemy_scenes.rs`) dig a
  cavern in each family's habitat band east of the town and show every kind (`enemies_lineup`),
  their tells, a bite, a bomb fight, an EMP and a split-screen fight. The `w6_*` scenes
  (`scene_capture/wave6_scenes.rs`) show the Wave 6 content: a sky town's land office and its
  isle plots with a cloud cottage, the Moon's colonists, the culture furniture and tomb
  trophies inside homes, a longhouse at dusk, and a tomb-bound story at the tomb door and in
  the journal.
- Wave 6 content in the shell and renderer: the land office screen reads
  `WorldState::land_office_listing` / `land_office_catalogue` (a sky town's hall titles itself
  "Sky Land Office" and sells isle plots and the sky pieces), listed sky sites get FOR SALE
  boards (`rendering::world::homes::listed_sites` adds `plot::sky_sites_of` of the sky towns
  over the view), `rendering::interior::home::draw_wave6_piece` draws the 17 new pieces,
  `house_look` and the wall and roof arms draw the four new plans (moon plans as domes),
  `people_art` dresses Lunar colonists (jumpsuits) and the lunar trades, and the journal
  colours the Tombs kind.
- `rendering::world::enemies` — enemies and enemy shots (sprites in
  `pixel_art/sprites/enemies.rs`, one per kind; mode-driven frames, wind-up tells, health and
  shield bars, stun sparks, flames, burrowers churning through rock, hidden in undiscovered
  tiles, interpolated with the bomb alpha) and the red hit flash on rigs inside their
  post-hit i-frames (`RenderSnapshot::rig_guards`).
- `balance_report` (desktop only) — `fatherload --balance-report [--seeds N] [--minutes M]
  [--hz H] [--only economy,progression,combat,exposure,building,climb,trip] [--strategies
  shaft,prospector,charges] [--threads T]` runs `fatherload_simulation::balance` and prints its
  Markdown report (threads default to every core).
- `worldgen_tools` — `fatherload --worldgen-report [--seeds N] [--first-seed S] [--size X]
  [--richness X] [--hazards X] [--legacy] [--generator v1|v2|v3]` prints generator distributions over many seeds, and
  `fatherload --worldgen-preview <seed> <out.png> [--size X] [--richness X] [--hazards X]
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
- `shell::world_setup` — the New World dialog (seed, size, richness, hazards, enemies, test
  mode) opened by every "new world" title entry; the seed comes from OS randomness
  (`random_world_seed`, 48 bits). `local_game::world_from_setup` builds the world (test mode via
  `WorldState::enable_test_mode`, the enemy setting via `WorldState::set_enemy_difficulty`).
  In play, the local or hosting player's pause menu cycles Enemies (peaceful / normal / hard)
  with `PlayerCommand::SetEnemyDifficulty`; joined clients do not get the entry, and the host
  refuses the command from a remote client anyway (`NetSimulation::host_only_action`).
- `shell::test_mode` — the pause menu's test-mode entries (Warp to Layer list, invulnerable hull
  and infinite fuel toggles), each a `PlayerCommand::TestMode` action for the seat driving the
  menu; a paused local world applies toggles on resume, so the menu shows the pending choice
  (`ShellState::pending_test_mode`) until the world catches up.
- `save` — format v8 (`WorldState` + play time; v8 adds enemies and the enemy director, v7
  files load as is with their defaults; the terrain as generation settings plus tile
  diffs, rigs' item containers and the entity store, see the simulation README);
  `save::legacy` migrates v1-v6 (v1-v5 terrain wrote out every tile and is read as
  `FullTerrain`; v3/v4 worlds carried a derived summary whose `won_game` flag is lifted into
  the world; v1-v6 rigs carried separate cargo / artifact / material maps, moved into the cargo
  bay, and v1-v6 worlds kept gas clouds and boulders in lists, moved into entities). A save naming an unknown generator version fails
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
  - `pads` — raylib/GLFW sampling and `fatherload --list-gamepads` (opens a small window for
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
  - `bots` + `headless`: `fatherload --headless-host [addr] [--lan] [--until-empty]
    [--duration S]` and `fatherload --bot-client <addr> [--count N] [--duration S]` (no window,
    periodic stats lines; the host also prints one whenever its client count changes). A bot's
    `--duration` is play time, counted once every bot is in game (joining is bounded by its own
    30 s timeout); `--until-empty` ends the host once a client has come and every client has
    gone. The `tests/headless.rs` smoke test uses both, so a slow start on a loaded machine no
    longer races fixed durations.

Online flow: Title > Host Online Game (lobby with address, descriptor copy, roster, Start) or
Join Online Game (address, pasted descriptor, or LAN list), lobby ready toggle, Leave/End,
reconnect with the token after a dropped connection, and notices when the host ends the session,
rejects the join, or the connection is lost. A running single-player game can be opened from the
pause menu. Online pause is a local overlay (the world keeps running). Only the host saves (manual
and a periodic autosave); joined clients cannot save or load. F3 toggles the network overlay; in
dev builds F4 / Page Up / Page Down tune live `NetTuning` values on the host while the overlay is
open (otherwise Page Up / Page Down scroll dialogs; `ShellState::tuning_keys_active`). The
`FATHERLOAD_NET_SIM` link conditioner applies to every online session.

## Wave 7 effects (F9: post-process pipeline, rolling camera, rotation controls)

- **Pipeline** (`rendering/post_process.rs`): a view whose seat sees screen effects, a darkened
  field or a rolled camera is drawn into its own render texture (world pass), turned around the
  rig into a view-sized texture when the camera rolls (roll pass), then put on screen through
  one shader running the stack in order: lensing, wobble, pixelation, mirror, chromatic
  aberration, bloom, exposure, the field's light, desaturation, colour grade, palette
  reduction, fog, scanlines, vignette. GLSL 330 on the desktop and GLSL ES 100 in the browser
  come from one body (`fragment_source`; a unit test checks both are well formed, scene capture
  prints whether the desktop shader compiled, the browser build compiles the ES variant at
  start). Views with nothing to do draw straight to the screen as before, so every older scene
  is pixel-identical. The HUD is drawn afterwards on the seat canvas (upright, untouched).
  Each split-screen seat has its own textures, levels and roll; the view scissor is lifted
  during the texture passes.
- **Levels** (`screen_fx.rs`, pure and unit tested): the view's `FieldSample` (at the seat's rig
  or pilot, `ViewSnapshot::fields`) times `EffectsIntensity` (Options: full / reduced / off),
  eased (0.3 s) and always rate-limited (`MAX_LEVEL_RATE`, brightness-changing effects and the
  light `MAX_LUMINANCE_RATE`), exposure and bloom capped, the wobble phase under 0.8 Hz: no
  flash or strobe is possible whatever a field does (`a_strobing_field_never_flashes_the_screen`).
- **Rolling camera**: `ViewSnapshot::roll` is the seat's rig orientation while in the rig (0 on
  foot: the pilot stays upright). Rigs are drawn turned about their middle
  (`rendering/world.rs::TurnedAbout`, rlgl matrix stack), so the seat's own craft looks upright
  while the world turns; other rigs show their own lean (`PlayerPresentation::orientation`).
- **Controls**: `PlayerInput::{rotate, dpad_rotates, stick_horizontal}`: keyboard `,` / `.`,
  the arrow-keys seat Numpad `/` `*`, a controller's d-pad left/right in the rig (a deflected
  stick then drives alone, `PlayerInput::rig_horizontal`; with the stick centred the d-pad
  still drives and drills sideways, so grounded rigs stay drivable; on foot and in menus the
  d-pad walks and steps), touch `<ROT` / `ROT>`
  (`TouchControl::{RotateLeft, RotateRight}`, top inner corner of the stick zone, below any
  dropped top-row button). Options gains Screen effects, Gyro stabiliser and Horizon hold
  (`SettingsFile::{effects, assists}`, defaults full / off); the assists ride along in every
  `ControlState`.
- **Aiming through the roll**: `weapon_aim::ViewAnchor` carries the view's `roll` and
  `pivot`, so the mouse's world point and the right stick's direction turn with the rolled
  camera (`a_rolled_camera_turns_the_mouse_and_the_stick_aim_with_the_world`). Not yet: the
  mirror effect does not mirror the mouse's world point.
- **Dev zones**: test-mode pause entries Place Field Zone (cycles `FieldPreset`s around the
  seat's rig) and Clear Field Zones, both `TestModeAction`s (so online too).
- **Scenes** (`scene_capture/field_scenes.rs`): `fx_pixelate`, `fx_palette`, `fx_exposure`,
  `fx_bloom`, `fx_desaturate`, `fx_grade`, `fx_chromatic`, `fx_mirror`, `fx_wobble`,
  `fx_lensing`, `fx_scanlines`, `fx_vignette`, `fx_fog`, `fx_combined`, `fx_dark`,
  `fx_pixel_rot_cavern`, `fx_split_screen` (per-seat effects, seat 2 rolled),
  `rotate_home_flying` (35 degree lean over town), `rotate_shear_wall` (standing on a wall),
  `rotate_upside_down` (on the ceiling), `rotate_spinning_weightless`.

- **Browser** (WebGL1): the ES 100 shader compiles and draws the same picture as the desktop
  (`target/web-perf/fx_combined.png` matches `fx_combined`). Frame budget, `node
  scripts/perf-web.cjs` on saves exported from the scenes above (5 s each, 1280 x 720, Chrome
  on an Apple Silicon Mac under heavy load from parallel builds):

  | Scene | GPU: frames in 5 s / mean JS work (ms) | Software WebGL: frames in 5 s / mean JS work (ms) |
  |---|---|---|
  | town_market (no effects, straight to screen) | 300 / 1.09 | 26 / 2.52 |
  | fx_combined (every effect, all stages on) | 300 / 1.20 | 41 / 1.15 |
  | fx_pixelate | 300 / 1.24 | 49 / 1.08 |
  | fx_wobble | 300 / 1.20 | 35 / 1.22 |
  | fx_dark (light only) | 300 / 1.02 | 66 / 1.16 |
  | rotate_shear_wall (world, roll and effect passes) | 300 / 1.05 | 19 / 1.17 |

  On a GPU every state holds the display's 60 Hz with about 1 ms of JavaScript-side work, the
  same as a plain scene: the extra passes are GPU fill (one view-sized texture, plus a
  roughly 2x square while rolled) and one full-screen fragment pass of at most 14 texture
  reads. Software WebGL is bound by rasterization outside the measured callback; its frame
  counts swing with the machine's load (the plain scene drew fewer frames than the effect
  scenes in this run), and the rolled view, which rasterizes about three times the pixels,
  is the slowest there. Budget: keep the stack one pass; a rolled view is the cost to watch on
  weak devices.

  Review re-run (same scenes and settings, Chrome 1280 x 720, frames in 5 s / mean JS ms):
  GPU: town_market 299 / 1.30, fx_combined 300 / 1.14, fx_pixelate 300 / 1.16, fx_wobble
  300 / 1.15, fx_dark 300 / 0.95, rotate_shear_wall 300 / 1.15. Software WebGL: 47 / 1.12,
  34 / 1.41, 32 / 2.30, 27 / 2.19, 69 / 1.11, 30 / 1.23. The GPU result (every state at
  60 Hz, about 1 ms of JavaScript) holds; under software WebGL the full-screen effect pass
  costs frames this time (pixelate and wobble 27-34 frames against 47 plain), so effect-heavy
  strata are the cost to watch on weak devices without a GPU. The measured callback time
  does not include GPU or rasterizer time.

## Wave 6 QA (presentation, tests and browser timing)

- **Presentation clock** (`ShellState::update_ticks`): toasts, banners and every world
  animation treat it as sixtieths of a second, but it counted frames. The browser build runs
  one frame per display refresh, so on a 120 Hz screen all of it ran at double speed (half
  speed at 30 fps). It now follows real time (`advance_presentation_clock`); a frame of about
  1/60 s still counts exactly one, so 60 Hz displays behave as before. Regression test:
  `shell::tests::the_presentation_clock_follows_real_time_at_any_frame_rate`.
- **The backdrop Moon** (`rendering/world/sky.rs`, `MoonDiscs`): drawn every frame as runs of
  rectangles broken by its speckles and dither (about ten thousand once it looms in the high
  sky), it was the browser's worst frame by far. It is now rasterized into a lit and a dark
  layer texture when its radius or phase changes (four recent discs cached for split-screen
  views) and drawn as two textures with the same alphas. The phase is quantized to 1/4096 of
  the disc, so a few terminator pixels can land one art pixel over (8 pixels of the
  `sky_space` capture); everything else is pixel-identical. Review follow-up: climbing or falling under the Moon changes its radius
  nearly every frame, and rebuilding (and uploading) a disc of a few hundred art pixels each
  frame was the remaining software-WebGL spike (26 ms mean against a 3.3 ms median). Discs
  are now rasterized at most once per `MOON_REBUILD_GAP` (8) draws while the disc keeps
  changing; in between the cached disc nearest in size (same side lit) is stretched over the
  new one, and the exact disc is drawn as soon as the view settles
  (`a_moving_moon_reuses_the_nearest_disc_between_rebuilds`).
- **Robust real-network tests**: `online/tests.rs` and `online/persistence_tests.rs` wait for
  their conditions (every issued action resolved, more than ten authority hashes compared,
  server frames applied) with at least the old windows, under a one-minute bound, instead of
  asserting after fixed windows; the loopback datagram burst in `quic_transport` is paced in
  tens. No check was weakened.
- **Saves across versions**: `save::tests::every_supported_save_format_loads_plays_and_round_trips`
  (formats 1 to 12 load, play two minutes with the mining bot, save and reload exactly, rigs on
  the current fuel and cargo curves) and
  `save::cross_version_tests::every_generator_version_saves_mid_play_and_continues_identically`
  (a two-rig world of every generator, legacy and V1 to V10, played a minute at dusk on the
  short day, saved and loaded through the game's save code, then replayed 40 s into the night
  next to the never-saved world: identical every second; V10 with a crew member saved
  mid-crypt).

### Browser frame timing

```bash
cargo run --release -p fatherload_game -- --screenshot-scenes target/perf-saves --export-saves \
  --only night_town,night_full_moon,cave_dark_lamp,sky_archipelago,sky_city,sky_space,moon_colony,tomb_run_side_view_traps,side_tomb_sand,crypt_traps,crypt_skeleton_pack
nix develop .#web --command bash scripts/build-web.sh
FATHERLOAD_WEB_GPU=1 PLAYWRIGHT_CHROMIUM_EXECUTABLE=/path/to/chrome node scripts/perf-web.cjs target/perf-saves
```

Mean frame work per animation frame (ms, 5 s per scene, 1280 x 720, Chrome on an Apple
Silicon Mac under heavy load from other builds), before / after the Moon fix:

| Scene | GPU before | GPU after | Software WebGL before | Software WebGL after |
|---|---|---|---|---|
| sky_space (the Moon looming) | 4.74 (p95 6.3) | 2.73 (p95 4.2) | 190 (median 139) | 26 (median 3.3; texture rebuilds as the rig drifts) |
| night_full_moon | 2.04 | - | 2.30 | 2.12 |
| sky_city / sky_archipelago | 2.52 / 2.25 | - | 2.32 / 2.41 | 2.51 / 1.80 |
| moon_colony | 1.74 | - | 1.67 | 2.02 |
| night_town / cave_dark_lamp | 1.89 / 1.61 | - | 2.15 / 1.70 | 2.01 / 1.59 |
| tomb_run_side_view_traps / side_tomb_sand | 1.04 / 2.81 | - | 1.93 / 1.54 | 1.23 / 1.27 |
| crypt_traps / crypt_skeleton_pack | 1.22 / 1.00 | - | 1.54 / 1.30 | 1.37 / 1.11 |

Review re-run (software WebGL, system Chrome, load average 110-130, 10 s per run): `sky_space`
with the stand-in discs 2.28 / 24.7 ms mean (medians 2.2 / 3.1) against 43.5 / 57.4 ms
(medians 3.6 / 39.7) for the build before it, alternating runs. The spread between runs of the
same build is as large as the difference, so treat it as "no worse, probably better" until it
is measured on a quiet machine; `night_full_moon` and `sky_moon_from_below` stay at about 2 ms.

Every other state stays near 1-3 ms of frame work on both backends (software WebGL is bound
by rasterizing, which happens outside the measured callback). Desktop `--frame-timing` agrees:
`sky_space` 3.25 ms before and 1.69 ms after, most scenes 1-3 ms, `split_four_mixed` the
heaviest at about 5.5 ms for four views.

### Browser frame timing, Wave 13 (the galaxy)

The dune world drove at about 38 fps in the browser with 70 ms to 2 s hitches. A profiling
build (`--profiling-funcs` added to the link flags of `scripts/build-web.sh`) under Chrome's CPU
profiler, with the rig driving east across the dunes, put 60 % of the frame on the draw side:

* the polar ground's coarse map repainted its whole 2,048 x 1,024 window each time the camera
  crossed a window step, generating every chunk it touched (`PolarGround::sync` 22 %, up to 2 s
  in one frame), and
* `PlaceRenderers::sync` painted the seat's round ground flat as well, for nobody (27 %, plus
  the darkness grid's 11 %); the home ground was also synced for the away seat's camera.

Now the coarse map (`rendering/space/polar/coarse.rs`) keeps what a moved window holds and
paints the rest within 3 ms a frame, the rows nearest the camera first (a tile write's rows at
once; screenshot scenes paint it whole); round places keep no flat cache; the home ground is
synced only for seats at home. Same profile after: the frame is 83 % idle.

Measured with `scripts/perf-web.cjs` (saves exported by `--export-saves`) and
`scripts/perf-web-space.cjs` (the staged space views), Chrome on an Apple Silicon Mac under
heavy load from other builds (load average 30-60), 1280 x 720, 8 s a scene; "driving" holds the
right arrow (the rig drives or flies east all the while). Frames counted in 8 s (60 fps is
about 480), mean and p95 frame work in ms, and the frames that took longer than 25 ms:

| Scene | GPU still: frames, mean, p95 | GPU driving: frames, mean, p95, >25 ms | Software still: frames, mean | Software driving: frames, mean |
|---|---|---|---|---|
| alien_vessi_town | 481, 1.89, 2.00 | 481, 2.44, 9.80, 0 | 36, 15.85 | 222, 8.92 |
| frontier_crystal_surface | 480, 1.72, 1.80 | 480, 3.89, 12.50, 0 | 46, 7.17 | 113, 22.98 |
| frontier_dunes_sandstorm | 481, 1.83, 2.00 | 481, 2.84, 7.00, 0 | 36, 2.48 | 57, 37.72 |
| frontier_dunes_surface | 480, 1.54, 1.70 | 480, 2.87, 7.70, 0 | 31, 22.24 | 52, 41.85 |
| frontier_volcanic_eruption | 481, 1.78, 1.90 | 480, 4.69, 13.60, 0 | 96, 2.55 | 63, 50.24 |
| space_asteroid_belt | 480, 1.14, 1.20 | 480, 1.67, 2.10, 0 | 65, 1.24 | 93, 3.35 |
| space_boss_arbiter | 480, 1.91, 2.00 | 480, 4.15, 5.50, 0 | 36, 2.26 | 46, 164.30 |
| space_boss_pirate_king | 480, 1.89, 2.00 | 480, 4.17, 5.50, 0 | 48, 2.34 | 57, 123.23 |
| space_boss_swarm_queen | 480, 1.96, 2.10 | 480, 4.02, 5.00, 0 | 65, 2.18 | 64, 103.30 |
| space_dogfight | 481, 1.91, 2.00 | 480, 4.10, 5.10, 0 | 101, 2.14 | 64, 117.58 |
| space_flight_cruise | 480, 8.14, 10.80 | 481, 8.58, 11.70, 0 | 35, 2.40 | 60, 138.63 |
| space_ground_raid | 481, 1.71, 1.80 | 480, 2.75, 5.60, 0 | 21, 2.46 | 97, 25.49 |
| space_landed | 480, 1.72, 1.80 | 481, 2.01, 2.50, 0 | 76, 2.08 | 60, 13.15 |
| space_split_places | 480, 2.33, 4.00 | 481, 4.39, 6.00, 0 | 31, 104.52 | 57, 103.64 |
| wild_glacier_surface | 480, 2.14, 2.30 | 480, 3.33, 8.50, 0 | 37, 2.31 | 135, 17.71 |
| wild_jungle_surface | 481, 1.83, 2.00 | 481, 3.08, 8.50, 0 | 101, 2.10 | 128, 18.15 |
| wild_ocean_surface | 481, 1.96, 2.10 | 480, 2.44, 9.20, 0 | 139, 2.12 | 56, 109.21 |

Staged space views (`?space=`, `scripts/perf-web-space.cjs`), frames in 8 s, fps and mean frame work:

| View | GPU | Software |
|---|---|---|
| space_asteroid_belt | 480, 60.0 fps, 1.61 ms | 311, 38.8 fps, 1.52 ms |
| space_planet_gallery | 480, 60.0 fps, 1.36 ms | 78, 9.7 fps, 1.33 ms |
| space_black_hole | 480, 60.0 fps, 1.67 ms | 70, 8.8 fps, 1.68 ms |
| space_moon_flyover | 480, 60.0 fps, 1.80 ms | 97, 12.2 fps, 56.02 ms |
| space_polar_quads | 480, 60.0 fps, 3.73 ms | 82, 10.2 fps, 98.99 ms |
| space_nebula | 480, 60.0 fps, 1.39 ms | 112, 14.1 fps, 1.37 ms |
| space_cruise | 480, 60.0 fps, 1.39 ms | 193, 24.1 fps, 1.34 ms |
| space_warp | 480, 60.0 fps, 1.37 ms | 208, 25.9 fps, 1.35 ms |
| space_core_shaft | 479, 59.9 fps, 1.81 ms | 155, 19.4 fps, 21.51 ms |
| space_planet_surface | 480, 60.0 fps, 1.46 ms | 168, 21.0 fps, 1.37 ms |

Software WebGL (SwiftShader) is bound by rasterizing on the CPU, so on this machine (load
average 30 to 130 from other builds while it ran) its frame counts measure the load more than
the game; the frame work it shows inside the callback includes its texture uploads. Against
the Wave 12 master's bundle, alternating runs under the same load: the dune world driving,
median frame work 5.6-5.9 ms against 30.1-30.2 ms (the mean swings with the load: 53 / 101 ms
against 93 / 95 ms); the ocean world 98-135 ms against 128-134 ms; the planet gallery and the
moon flyover alike (7.1 / 9.3 fps against 7.2 / 10.1 fps; those views are untouched). The Wave
11 software numbers (planet gallery 44 fps) were taken on a quiet machine.

To profile the browser build, add `-C link-arg=--profiling-funcs` to the emscripten rustflags
of `scripts/build-web.sh` (function names survive into the `.wasm`), and record Chrome's CPU
profile through the DevTools protocol while the page runs (`Profiler.start` / `Profiler.stop`
in a Playwright CDP session).

## Wave 11 regions (F11: seats in different places)

Each split-screen seat (and each online client) sees its own place: a seat away from the home
store gets `ViewSnapshot::place_scene` (its store's terrain, entities, frame, rigs and pilots),
a camera in that store's frame and the HUD read over that ground. `rendering::world::place_view`
draws it (one terrain cache per store with the game's tile art, stars fixed to the absolute
sky, drops, pilots, rigs); it is the stub the space-render track replaces. Audio listens in the
seat's place (a body's thin wind, free flight's suit hum; the home world's music and ambience
belong to seats at home). `view_frame::scoped(frame)` draws and listens in another store's
frame. Body tile changes arrive as `FrameReport::changed_store_tiles`. Save format 15 holds
every visited body's ground (see the simulation README, "Wave 11 regions"). Scenes:
`place_body_landed`, `place_free_flight`, `split_two_places`, `split_four_places`.
## Wave 11 space-render (space and round bodies)

`rendering/space.rs` and `rendering/space/` draw space and round bodies from a plain-data
`SpaceRenderView` per seat: the parallax starfield, stars by class, nebulae, belts and rocks,
comets, stations and derelicts, black holes (with a lens pass) and wormholes, planets from space
(the planet shader), the polar ground (tile art bent into the ring by the polar shader, or curved
quads without it), and cruise and warp effects. Shaders: GLSL 330 and GLSL ES 100 from one body
(`rendering/space/shaders.rs`). The camera is locked to the craft; the HUD is drawn after and
stays upright. The full write-up, the seams the integrator wires and the timing are in
`packages/simulation/README.md` ("Wave 11 space-render").

- Look: `--screenshot-scenes <dir> --only space_star_system,space_moon_surface,...` (every
  `space_*` scene), or interactively `fatherload --space-view space_black_hole`.
- Time the browser build: `node scripts/perf-web-space.cjs [names]` (serves `target/web`,
  opens `?space=<name>`; `FATHERLOAD_WEB_GPU=1` for the GPU).


## Wave 11 integration (every seat in its own place)

Each seat draws its own place every frame (`rendering/space_seat.rs`, synced in
`GameRenderer::sync`): a seat under the Star Drive sees the real galaxy round its rig
(`rendering/space/live.rs` gathers the space generator's suns, worlds, belts, comets, nebulae,
black holes, wormholes and derelicts; flight's layer, stations and HUD go over it); a seat on a
generated body sees that body store's round ground through the polar renderer, the galaxy above
it, the camera locked to the craft and the HUD upright; a seat at home sees the home world as
ever. Each seat keeps its own polar ground textures, and the body's tile writes
(`FrameReport::changed_store_tiles`) repaint only their rows. Scenes on real worlds:
`space_landed`, `space_moon_approach`, `space_moon_ground`, `space_moon_far_side`,
`space_moon_shaft`, `space_split_places`, `space_near_black_hole`. Details in the simulation
README ("Wave 11 integration").

## Wave 12 systems (space's things on screen)

The flight HUD and world layer read what stars, black holes, wormholes, nebulae, belt rocks and
comets do (`rendering/space_systems_view.rs`): glare and flare washes (capped, smooth, scaled by
the screen effects setting), a nebula's tint and faded nav markers, a hole's dashed
point-of-no-return ring with its warnings and the `TIME x / CLOCK -` dilation readout, the
wormhole tunnel and edge markers to known routes, drill sparks and the Mining Beam with their
gauges, comet dust, and the parking hold. Belt boulders are the flight model's real rocks.
On a round body the map button opens the body's own map (`rendering/body_map.rs`). Five
`systems_*` sound patches. Scenes `systems_*` (`scene_capture/systems_scenes.rs`). Details in the
simulation README ("Wave 12 systems").

## Wave 14 atlas (the test-mode travel browser)

A test-mode world's seats each get an Atlas (`F2`, the pause menu, the touch ATLAS button):
categories, type-ahead search, a live preview of the chosen place painted with the real tile
art (`rendering/atlas/preview.rs`: 44 x 26 tiles from the world's own terrain or a planet's
ground, a few rows a frame, 24 cached) or drawn by the space renderer from orbit with a galaxy
minimap, an info card, Go / Prev / Next / Random / Bookmark / Back / Map, and map mode (a depth
map of all eight Descents, then the galaxy). The destinations live in `src/atlas/` (pure, no
raylib; a stand-in for the simulation's destination index, TODO-SEAM), the seat's state and
input in `src/shell/atlas.rs`, the drawing and layout in `src/rendering/atlas.rs` and
`src/rendering/atlas/layout.rs` (Wide, Compact and Narrow shapes, tested for overlap at ten
canvas sizes). Scenes `atlas_*` (`scene_capture/atlas_scenes.rs`). Details in the simulation
README ("Wave 14 atlas").
