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
| `rendering/pixel_art/sprites/*.rs` | All sprite data: `ui` (frames), `icons`, `rig`, `interior`. |
| `rendering/pixel_art/font.rs` | The bitmap UI font (regular + derived bold). |
| `rendering/ui_kit.rs` | UI canvas + widgets (panels, buttons, bars, rows, tabs, tooltips...). |
| `rendering/layout.rs` | Menu/modal content model drawn with the UI kit. |
| `rendering/screen.rs` | HUD, minimap, depth ruler and every menu/modal screen. |
| `rendering/interior.rs` | The walk-in building rooms. |
| `rendering/gallery.rs` | The art reference sheet. |

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
  tile is 16 art pixels and the rig is 14x11. The zoom is
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
style), `meter` and `text`. `ModalContent::compact()` narrows short menus, `.warning()` gives
the red frame. Dialogs shrink to their content; inside a building they dock on the left
(`ModalPlacement::Counter`) so the counter, keeper and rig stay visible.

Screen colour roles: titles `GOLD` (warnings `ORANGE`), section headings `SKY_LIGHT` (or a role
colour), labels `MIST`, values `BONE`/`GOLD`, hints `FOG`, selection `AMBER` face with `INK`
text, unaffordable prices `RED` on a disabled row.

## Style reference

- **HUD** (`screen::draw_view_hud`): stats panel with hull/fuel/cargo stat bars top-left,
  credits + depth panel top-right, blinking warning badges below the stats, framed minimap
  bottom-right, depth gauge down the right edge.
- **Upgrade Shop interior** (`interior.rs`, `draw_shop_props`): the fully dressed room. Every
  building is a walk-in room with the same structure (ceiling, brick wall, wainscot, floor,
  exit door, sign, lamp, counter + keeper) in its own colours via `interior_set!`; dress the
  others like the shop. The rig walks in the shell's room units (`interior_x / 2` = art x), so
  door and counter positions must keep following `shell::interior_service_x`.

Still placeholder (next steps): terrain tiles and fog (`terrain.rs`), surface buildings and sky
(`world.rs`), remote rigs, bombs/infrastructure/particles, the map modal, the title screen
backdrop, and dressing for the seven non-shop interiors.

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

Scenes: `art_gallery`, `title`, `surface_town`, `surface_flying`, `shallow_dig`,
`mid_depth_ores`, `deep_lava`, `interior_fuel`, `interior_repair`, `interior_depot`,
`interior_hq`, `interior_shop`, `interior_bank`, `interior_explosives`, `interior_salvage`,
`interior_shop_walk`, `hud_low_fuel`, `hud_low_hull`, `pause_menu`, `modal_exit_confirm`,
`modal_inventory`, `map`, `online_lobby_host`, `split_screen`, `rescue_prompt`.

Adding a scene: append a `Scene { name, summary, setup }` to `scene_capture::SCENES`. `setup`
gets a `SceneStage`: `play(seats, |world, seats| ..)` enters a staged world (helpers
`park_rig`, `sink_rig`, `carve`, `reveal`, `find_tile`, `load_cargo`), `hold(frames, input)` /
`settle(frames)` run frames, and `shell()` opens menus directly. A unit test builds every scene
headlessly.
