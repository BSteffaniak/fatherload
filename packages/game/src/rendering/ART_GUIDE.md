# Drillgame art guide

Drillgame's look is **crisp pixel art**: every tile, sprite, glyph and UI frame is authored in
code in *art pixels*, drawn from one named palette, and shown at whole-number scales with point
(nearest) filtering. Nothing is downloaded or loaded from image files.

The quickest way to see the whole kit is the `art_gallery` capture scene (see
[Screenshot workflow](#screenshot-workflow)); `gallery.rs` draws it.

## Where things live

| Module | Owns |
| --- | --- |
| `crate::pixel_scale` | The scaling policy (pure math, unit tested). |
| `rendering/pixel_art/palette.rs` | The palette constants and `PALETTE` list. |
| `rendering/pixel_art/sprite.rs` | `SpriteDef`, `NineSlice`, atlas packing/rasterizing, draw helpers. |
| `rendering/pixel_art/shade.rs` | Palette indices, the one-step-darker ramp walk (`darker`) and the 4x4 Bayer dither. |
| `rendering/pixel_art/sprites/*.rs` | All sprite data: `ui` (frames), `icons` (rig stats, minerals), `menu_icons` (menu rows, counters, roster), `prompts` (keycap + controller glyphs), `rig`, `interior`. |
| `rendering/prompts.rs` | Button prompts: glyph drawing, prompt rows, the menu device. |
| `rendering/pixel_art/font.rs` | The bitmap UI font (regular + derived bold). |
| `rendering/ui_kit.rs` | UI canvas + widgets (panels, buttons, bars, rows, tabs, tooltips...). |
| `rendering/layout.rs` | Menu/modal content model drawn with the UI kit. |
| `rendering/screen.rs` | HUD (stats, wallet + tool chips, warning badges), toasts, minimap, depth ruler, inventory, map, help. |
| `rendering/terrain.rs` + `terrain/{tile_art,fog}.rs` | Terrain chunks, the procedural tile art and the exploration fog. |
| `rendering/world.rs` + `world/{sky,buildings,rig,fx}.rs` | Everything in world space: sky and parallax, the town, rigs, props and effects. |
| `rendering/interior.rs` | The walk-in building rooms (shell, door, lamps, sign, counter, keeper, shop dressing). |
| `rendering/counters.rs` | Every building's counter panel and the receipt-style purchase confirmations. |
| `rendering/menus.rs` | Pause, options, save/load slots, exit confirmations, rescue prompt, online screens and lobby. |
| `rendering/title.rs` | The title screen: claim cross-section backdrop (shared by online screens), logo, menu. |
| `rendering/interior/props.rs` | Each non-shop room's furniture and counter-top details. |
| `rendering/gallery.rs` | The art reference sheet. |
| `rendering/ui_sheet.rs` | The UI reference sheet: menu icons and props at 1x and 3x. |

## Palette

44 colours in ramps (dark to light). Always pick a palette constant by role; never write raw
RGB in rendering code. `palette::with_alpha(color, a)` is the only allowed variation (overlays,
fades, light cones). A unit test rejects off-palette colours in sprite legends.

| Ramp | Colours | Roles |
| --- | --- | --- |
| Neutrals | `INK` #0e0b14, `NIGHT` #1a1626, `SLATE_DARK` #28243a, `SLATE` #3c3854, `SLATE_LIGHT` #5a5677, `FOG` #8987a6, `MIST` #c1c2d6, `BONE` #f3efe4 | `INK` is every outline and text shadow. UI panels are `SLATE_*`/`NIGHT`. Body text `BONE`, labels `MIST`, hints `FOG`. |
| Earth | `SOIL_DARK` #3a2418, `SOIL` #5c3a22, `LOAM` #80542f, `CLAY` #a8653f, `SAND` #d49b5c | Dirt and clay strata, wood, crates, cargo bars. |
| Stone | `STONE_DARK` #34302f, `STONE` #575250, `STONE_LIGHT` #857e76 | Rock strata, concrete floors, smoke. |
| Cool | `DEEP_BLUE` #1b2a4e, `OCEAN` #294d89, `SKY` #4d8ed5, `SKY_LIGHT` #8ac5ee, `ICE` #cbedf8 | Sky, glass, water, diamond, depth readouts, section headings. |
| Greens | `PINE` #1d3b2b, `MOSS` #2e6a39, `LEAF` #4ea348, `LIME` #9bd85a | Hull bar, emerald, uranium, "good" states. |
| Warm | `MAROON` #591a25, `RED` #b8302f, `ORANGE` #e8702a, `AMBER` #f2a93b, `GOLD` #ffd75e, `CREAM` #fff2b2 | Fuel, credits, gold, lava, warnings. `GOLD` = titles and money; `AMBER` = focus/selection; `RED` = danger. |
| Purples | `PLUM` #3d1f4b, `VIOLET` #6e3fa1, `LAVENDER` #a97de1, `ROSE` #df5a8b, `PINK` #f6a4c1 | Mythril, ruby, artifacts, the shop's walls. |
| Teals | `TEAL_DARK` #15494e, `TEAL` #2a8a82, `MINT` #6fe0c0 | Screens, scanner, bank. |
| Metals | `RUST` #8a3d1f, `COPPER` #c56b35, `STEEL_DARK` #495163, `STEEL` #798497, `STEEL_LIGHT` #b4becb | Rig parts, machinery, drills, copper/iron. |

Shading convention: a lit object uses three steps of one ramp (light top-left, body, shade
bottom-right) plus `INK` outline; one extra highlight pixel (`CREAM`/`BONE`/`ICE`) for glints.

## Sprite authoring format

Sprites are `const SpriteDef` values: a name, a legend and one or more frames of char rows.

```rust
pub const GEAR: SpriteDef = SpriteDef {
    name: "prop_gear",                       // unique atlas key
    legend: &[('o', INK), ('m', STEEL_LIGHT), ('d', STEEL)],
    frames: &[&[
        "..o.o..", //
        ".ommmo.", //
        "omdodmo", //
        ".mo.om.", //
        "omdodmo", //
        ".ommmo.", //
        "..o.o..", //
    ]],
    frame_seconds: 0.0,                      // > 0 animates the frames in a loop
};
```

- `.` and space are transparent; every other character needs a legend entry.
- All frames share one width/height; rows must be equal length (the tests check all of this).
- Conventional characters: `o` outline (`INK`), `l`/`m`/`d` light/mid/dark of the main ramp,
  `w` glint. Reuse one grid with several legends for recolours (see `icons::mineral_icon!`,
  `ui::PANEL_GRID`, `interior::interior_set!`).
- **Register every sprite in `sprites::ALL`** or it is not packed into the atlas (drawing an
  unregistered sprite silently draws nothing).
- Animation: `sprite.frame_at(seconds)` picks the frame; pass a deterministic clock
  (`ui_kit::seconds()` is driven by shell ticks, so captures are reproducible). Hold a frame by
  repeating it (the shopkeeper blinks once every 15 frames).
- `NineSlice { sprite, border }` stretches a frame: corners fixed, edges/centre stretched. Make the
  edge rows/columns constant along the stretch axis.

Drawing (`pixel_art::sprite`): `draw_sprite`, `draw_sprite_flipped` (mirror for facing),
`draw_nine_slice`, `draw_tiled`. Positions and scales must be whole numbers in the current
coordinate space.

The atlas (`sprite::load_atlas`) is rasterized once at startup into a point-filtered texture;
`GameRenderer` owns it and `sprite::set_current_atlas` publishes it for the frame.

## Scale policy

One art pixel always covers a whole number of physical framebuffer pixels (`pixel_scale`):

- **World**: one art pixel = `WORLD_PIXELS_PER_ART_PIXEL` (2) world pixels, so a 32-world-pixel
  tile is 16 art pixels and the rig is 16x13. The zoom is
  `round(framebuffer height / 360)` physical pixels per art pixel, which keeps
  `WORLD_VIEW_HEIGHT` (720 world pixels) exactly at 720/1080/1440/2160 px and within one rounding
  step elsewhere. High-DPI density is included (the framebuffer is what counts). The camera
  snaps to the physical pixel grid; world sprites snap to the art-pixel grid
  (`world::draw_world_sprite`). Draw world sprites with scale `WORLD_PIXELS_PER_ART_PIXEL`.
- **UI**: screen UI is laid out on a canvas in art pixels (640x360 at 1280x720 and 1920x1080,
  480x270 at the 960x540 minimum window) inside a `ui_kit::UiCanvasScope`, a camera that scales
  by the integer `ui_pixel_size`. Everything under that scope uses canvas coordinates; convert
  a world viewport with `ui_kit::canvas().viewport_rect(view.viewport)`. Clip with
  `ui_kit::begin_clip`/`end_clip` (canvas rects).
- **Interiors**: a room is a fixed 320x180 art-pixel canvas drawn at
  `fit_pixel_size(framebuffer, (320, 180))` (4 px at 720p, 6 px at 1080p) and centred; walls and
  floor extend past it on wide screens. Room art is therefore chunkier than UI art (2x), which
  reads as a close-up. UI drawn over a room uses the normal UI canvas.

Text: the pixel font is 9 art pixels tall (cap height 7). `TextStyle::Body` (regular, 1x),
`Bold` (1x) and `Title` (bold, 2x) are the only sizes. Text always gets a 1-pixel `INK` drop
shadow (`ui_kit::text`), or none on light surfaces (`text_shadowed(.., None)`).

## UI kit usage

All functions take canvas art-pixel coordinates (inside `UiCanvasScope`):

```rust
let _canvas = UiCanvasScope::begin(draw);
ui_kit::panel(draw, rect, PanelStyle::Standard);          // Standard / Accent / Warning
ui_kit::well(draw, inner);                                // recessed list/map area
ui_kit::button(draw, rect, "Resume", ButtonState::Selected); // + animated cursor
ui_kit::stat_bar(draw, x, y, width, &icons::FUEL, "FUEL", fuel, max, BarColors::FUEL, 0.2);
ui_kit::bar(draw, rect, ratio, BarColors::HULL, 0.35);   // pulses red below the threshold
ui_kit::item_row(draw, rect, &ItemRow { icon, label, detail, price, price_color, coin, state });
ui_kit::tabs(draw, rect, &["Cargo", "Market"], selected);
ui_kit::tooltip(draw, "text", anchor_x, anchor_y, max_width, bounds); // speech bubble
ui_kit::badge(draw, x, y, &icons::WARNING, "HULL");        // blinking warning chip
ui_kit::text(draw, "label", x, y, TextStyle::Bold, GOLD);
```

Menus and dialogs should go through `layout::UiLayout::modal_with_render_plan` with a
`ModalContent` of `Section`s: `SectionItem::stat` (label/value row; selectable when the label is
prefixed `"> "`/`"  "`), `icon_stat`, `item` (icon + label + detail + price rows, the shop
style; prefer it for every selectable row so each choice carries an icon), `meter`, `text` and
`field` (text entry well with caret). `ModalContent::compact()` narrows short menus,
`.warning()` gives the red frame, `.footer(text)` pins a description (the selected row's
details) under the scrolling content. Dialogs shrink to their content; inside a building they
dock on the left (`ModalPlacement::Counter`) so the counter, keeper and rig stay visible.
Content taller than the panel scrolls, a thin amber scroll bar shows that more rows exist, and
a newly selected row is scrolled into view (a different dialog title starts at the top).

Counter rows (`counters.rs`) must mirror, index for index, the request the shell submits for
that menu item, so a label always says what Enter does. Show prices with the coin, payouts as
`+N` in `LIME`, tags (`FREE`, `READY`, `MAX`) for non-price values, and grey out (disabled)
rows that cannot be used, keeping the price `RED` when credits are the reason. Selected rows
draw every value in `INK` on the amber face; disabled rows grey every tag except red prices.

## Button prompts

Never write key names into UI text. Name the *action* and let the prompt system draw it for the
device the player is using:

```rust
use super::prompts::{hint, Hint};
use crate::input::prompts::Prompt;

// Dialogs: prompts sit under the title, drawn for seat 0's device (the menu device).
ModalContent::new(sections).prompts(&[
    hint(Prompt::Navigate, "select"),
    hint(Prompt::Confirm, "buy"),
    hint(Prompt::Back, "close"),
]);
// Anywhere else: one prompt or a row, for an explicit device (per-view HUD) or the menu device.
prompts::draw_prompt(draw, x, y, hint(Prompt::Use, "Leave"), device, BONE);
prompts::draw_bar(draw, x, y, &HINTS, prompts::menu_device(), MIST);
// Sentences and toasts: the text form ("E", "X", "LB/RB"); None when the device has no binding.
crate::input::prompts::inline_text(Prompt::Use, device);
```

- `input::prompts::glyphs(prompt, device)` picks the glyphs: keyboard keys per layout (solo,
  WASD seat, arrow-keys seat) or controller buttons read from the binding table
  (`input::gamepad::PAD_BINDINGS`), so prompts follow any remapping. A prompt the device has no
  binding for (F11 on a controller) is simply left out of the row.
- A hint with an empty label groups with the next one (`[Tab] [Esc] close`).
- Every glyph is 11 art pixels tall (`PROMPT_HEIGHT`); labels are body text offset 2 pixels so
  the cap height centres on the glyph. Rows: 1 px between glyphs, 3 px before the label, 9 px
  between prompts; `wrap_bar` wraps rows to a width.
- Keyboard keys are the `prompts::KEYCAP` nine-slice (`INK` outline, `BONE` top lip, `MIST`
  face, `FOG` bottom edge) stretched to the label in `INK`.
- Controller glyphs (`sprites/prompts.rs`): one 11x11 circle grid per printed letter/symbol,
  recoloured per family through the legend. Xbox: `A` `LEAF`, `B` `RED`, `X` `SKY`, `Y` `GOLD`
  faces with `BONE` letters (`INK` on yellow), lit top-left and shaded bottom-right in the same
  ramp. PlayStation: dark `SLATE_DARK` buttons with coloured symbols (cross `SKY_LIGHT`, circle
  `ROSE`, square `PINK`, triangle `MINT`). Nintendo: dark `SLATE` buttons with `BONE` letters,
  printed at their own positions (B bottom, A right, Y left, X top). Bumpers (15 wide) and
  triggers (13 wide) are blank `STEEL_DARK` shoulder shapes labelled with the font (LB/L1/L,
  LT/L2/ZL). `MENU` (three lines), `VIEW` (two windows), the d-pad (one cross grid with
  vertical / horizontal / down arms lit in `BONE`) and the left stick complete the set. The
  family comes from the controller name (`PadFamily::from_name`); unknown pads use Xbox glyphs.
- Check changes with the `prompt_sheet` scene (every prompt on every device, glyphs at 3x) and
  the `pad_*` scenes.

Shell messages appear as a toast (`screen::draw_toast`) for four seconds after they change
(`ShellState::message_tick`), beside the counter dialog inside buildings; refusals get the red
frame.

Screen colour roles: titles `GOLD` (warnings `ORANGE`), section headings `SKY_LIGHT` (or a role
colour), labels `MIST`, values `BONE`/`GOLD`, hints `FOG`, selection `AMBER` face with `INK`
text, unaffordable prices `RED` on a disabled row.

## Style reference

- **HUD** (`screen::draw_view_hud`): stats panel with hull/fuel/cargo stat bars top-left,
  credits + depth panel top-right, blinking warning badges below the stats, framed minimap
  bottom-right, depth gauge down the right edge.
- **Interiors** (`interior.rs` + `interior/props.rs`): every building is a walk-in room with the
  same structure (ceiling, wall, wainscot, floor, exit door, sign, lamps, counter + keeper) in
  its own wall pattern and colours via `interior_set!` (brick, glazed tiles, dressed stone,
  wood panelling, marble, boards, corrugated sheet), dressed with its own furniture between
  the door and the counter and right of it, and counter-top details (register, scale,
  monitor, teller bars, goods). Room props are drawn from rectangles and sprites in room art
  pixels only (no raylib circles: use `props::disc`). The rig walks in the shell's room units
  (`interior_x / 2` = art x), so door and counter positions must keep following
  `shell::interior_service_x`.
- **Counters** (`counters.rs`): the Upgrade Shop and Salvage Yard panels show the full row
  vocabulary (prices, payouts, tags, locked rows, details footer).
- **Title** (`title.rs`): the logo uses `ui_kit::display_text` at 4x (3x below 340 canvas rows)
  with an ink outline, maroon drop shadow and gold/amber/cream bands.

- **World** (`world.rs`): see [World art](#world-art).

Still placeholder (next steps): the map modal's drawing.

## World art

Draw order (`world::draw_world_scene`): sky, sun, clouds and hills; the town; bedrock beyond the
world's edges; terrain; lava glow; infrastructure; drill cracks; lost cargo; bombs; boulders;
scanner pings; rigs; drill debris; gas clouds; sparks; explosions; remote name tags. Everything
snaps to the art grid (`world::Pen` draws art-pixel rectangles, `draw_world_sprite` sprites).
Animation is driven by shell ticks (`WorldScene::ticks`), never wall-clock time.

**Terrain** (`terrain/tile_art.rs`). A tile is 16x16 art pixels painted into palette indices:

- Materials are patterns sampled in *global* art coordinates so they flow across tiles: dirt
  (noise clumps, grains, pebbles), clay (dark body with broken wavy strata), stone (Worley
  cobbles lit top-left), hard rock (Manhattan-Worley angular slabs), foundation (offset blocks).
- Each depth band recolours them through 4-step ramps (`Strata`): topsoil 5-13, clay 14-27, grey
  stone 28-45, slate 46-64, basalt 65-89, then one palette per deep stratum (Crystal Faults
  blues, Fossil Oceans sand, Pressure Cathedrals steel, Radioactive Hollow greens, Ancient
  Machine steel/copper, Void Geode purples, Mantle Storm maroon), each with rare accent specks.
- Ores are 3-4 cluster stamps (`sprites::terrain::ore_stamp`: nugget/gem/chunk/flake shapes in
  the mineral's ramp) on the host rock of that depth; artifacts are one centred stamp each;
  gas/pressure pockets are bubbled cavities, explosive pockets volatile red crystals, magma vents
  a glowing seam, lava molten noise (plus an animated glow and glints in `world/fx.rs`, only on
  explored tiles).
- Faces meeting dug air get an `INK` outline, a lit lip (top/left) or shade (bottom/right) and
  rounded outer corners; ground under open sky gets grass. Dug air shows the band's back wall
  (`Strata::back`) with contact shadows under ceilings and along walls.
- `material_color(kind, y)` is a tile's representative colour (debris, dust; usable by the minimap
  if it wants to match the terrain).

**Fog** (`terrain/fog.rs`). Per-tile darkness (deeper and farther from explored tiles is darker;
the top three ground rows are capped so the surface stays readable) is interpolated between tile
centres and applied per art pixel as a Bayer-dithered walk down each colour's ramp
(`shade::darker`) ending in `INK`. Chunks cache their art; exploration only recomposites.

**Rig** (`sprites/rig.rs`, `world/rig.rs`). 16x13 body facing right (flipped for left, and a
stopped rig keeps its last facing); tracks roll while driving; a nose drill at the front, the
spinning bit below while drilling down, a side bit while drilling sideways, one-pixel drilling
shake; a long climb flame or short hover flame; exhaust puffs, thick smoke and sparks under 35%
hull, scorched paint under 30%, a fast red beacon under 15%. Other players' rigs use another hull
paint by player id (`REMOTE_BODIES`) and carry a name tag with fuel/hull pips whose border turns
amber (warning) or blinks red (critical).

**Surface** (`world/sky.rs`, `world/buildings.rs`). The sky is a startup texture of flat bands
with 5-row dithered steps; far snowy mountains (parallax 0.18), near pine hills (0.42) and
drifting clouds (0.3) are tiled textures; the sun is nearly screen-fixed. Each building is drawn
on a 128x64 art-pixel lot with its own silhouette, a sign carrying its simulation label and a
door that lights up while the local rig stands at it.

**Effects** (`world/fx.rs`). Sparks are square embers cooling cream to red; drilling throws chips
of the target's material (plus sparks on rock); `effects::Explosion`s (spawned where a detonated
bomb was) flash, burn as a stepped fireball and roll smoke; bombs blink faster as the fuse runs
down; boulders tremble under a blinking "!" before falling; gas clouds are drifting dithered
puffs; scanner pings are pulsing corner brackets (gold ore, pink artifact, red hazard).

## Screenshot workflow

```sh
cargo build --release -p drillgame_game
./target/release/drillgame --screenshot-scenes target/shots/latest            # all scenes, 1280x720
./target/release/drillgame --screenshot-scenes target/shots/1080 --size 1920x1080
./target/release/drillgame --screenshot-scenes target/shots/shop --only interior_shop,art_gallery
./target/release/drillgame --screenshot-scenes x --list                        # scene names
```

A window opens briefly, each scene is built deterministically (fixed seed, fixed frame time,
scripted input), rendered for a few frames and saved as `<dir>/<scene>.png` at framebuffer
resolution, then the app exits. Use the release build (terrain rebuilds are slow in debug).
Keep captures under `target/` (never commit them). macOS `sips` can crop and enlarge a PNG for a
close look at individual pixels.

Scenes (see `--list`): `art_gallery`, `ui_sheet`, `title`, world scenes (`surface_town`, `town_west`, `town_market`, `town_east`,
`surface_flying`, `shallow_dig`, `mid_depth_ores`, `deep_lava`, `lost_cargo`, `world_props`,
`drill_side`, `remote_rigs`, `bomb_blast`, `gas_leak`, `cave_in`, `deep_crystal_faults`,
`deep_void_geodes`), every
`interior_<building>` with its counter open, `interior_shop_walk`, every `room_<building>` with
no panel open, `toast_receipt`, `hud_low_fuel`, `hud_low_hull`, `pause_menu`,
`modal_exit_confirm`, `modal_inventory`, `modal_options`, `modal_save_slots`, `map`,
`online_lobby_host`, `online_lobby_client`, `online_join_address`, `online_lan_browser`,
`net_overlay`, `split_screen`, `rescue_prompt`, `prompt_sheet`, controller prompts
(`pad_title`, `pad_interior_shop`, `pad_room`, `pad_pause`, `pad_hud`, `pad_help`,
`pad_split_screen`, `pad_split_join`, `pad_playstation_counter`, `pad_nintendo_options`) and
`keyboard_hud`. Scenes choose prompt devices with `SceneStage::use_devices`. Check UI changes at 1280x720, 1920x1080 and the
960x540 minimum.

Adding a scene: append a `Scene { name, summary, setup }` to `scene_capture::SCENES`. `setup`
gets a `SceneStage`: `play(seats, |world, seats| ..)` enters a staged world (helpers
`park_rig`, `sink_rig`, `carve`, `reveal`, `find_tile`, `load_cargo`), `hold(frames, input)` /
`settle(frames)` run frames, and `shell()` opens menus directly. A unit test builds every scene
headlessly.
