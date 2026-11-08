//! The surface town: each service building as its own pixel-art structure with a sign and a
//! door, built from art-pixel rectangles on a 128x64 art-pixel lot (8 tiles wide, standing on
//! the ground line). A building's door lights up while the local rig stands at it.

use raylib::prelude::*;

use drillgame_simulation::{
    economy::SurfaceZone,
    surface::{SURFACE_BUILDINGS, SURFACE_SERVICE_MAX_DEPTH_TILES, SurfaceBuilding},
    terrain::TILE_SIZE,
};

use crate::render_snapshot::PlayerPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{
    pixel_art::sprites::{rig, world as props},
    terrain::tile_art::hash,
};

use super::{ART, GROUND_Y, Pen, text_art_width};

/// Lot height in art pixels (the ground line is at the bottom).
const LOT_HEIGHT: i32 = 64;
/// Lot width in art pixels.
const LOT_WIDTH: i32 = 128;

/// Per-building drawing state.
#[derive(Clone, Copy)]
struct Lot<'a> {
    pen: Pen,
    label: &'a str,
    /// The local rig is at this building's door.
    open: bool,
    seconds: f32,
}

/// Draws every building that overlaps `visible`.
pub fn draw_town(
    draw: &mut impl RaylibDraw,
    visible: Rectangle,
    local: Option<&PlayerPresentation>,
    seconds: f32,
) {
    let top = GROUND_Y - LOT_HEIGHT as f32 * ART;
    if visible.y > GROUND_Y || visible.y + visible.height < top {
        return;
    }
    for building in SURFACE_BUILDINGS {
        let x = building.tile_x as f32 * TILE_SIZE;
        let width = building.tile_width as f32 * TILE_SIZE;
        if x + width < visible.x || x > visible.x + visible.width {
            continue;
        }
        let lot = Lot {
            pen: Pen::at(x + (width - LOT_WIDTH as f32 * ART) * 0.5, top),
            label: building.label,
            open: local.is_some_and(|player| at_door(&building, player)),
            seconds,
        };
        draw_building(draw, building.zone, lot);
    }
}

fn at_door(building: &SurfaceBuilding, player: &PlayerPresentation) -> bool {
    let tile_x = (player.x / TILE_SIZE).floor() as i32;
    player.y <= SURFACE_SERVICE_MAX_DEPTH_TILES * TILE_SIZE
        && tile_x >= building.tile_x
        && tile_x < building.tile_x + building.tile_width
}

fn draw_building(draw: &mut impl RaylibDraw, zone: SurfaceZone, lot: Lot<'_>) {
    match zone {
        SurfaceZone::Fuel => fuel_station(draw, lot),
        SurfaceZone::Repair => repair_garage(draw, lot),
        SurfaceZone::Depot => ore_depot(draw, lot),
        SurfaceZone::Headquarters => headquarters(draw, lot),
        SurfaceZone::Shop => upgrade_shop(draw, lot),
        SurfaceZone::Bank => bank(draw, lot),
        SurfaceZone::Explosives => explosives_bunker(draw, lot),
        SurfaceZone::Salvage => salvage_yard(draw, lot),
    }
}

// --- Shared parts -------------------------------------------------------------------------------

/// A glass window with a frame, a mullion and a glint.
fn window(draw: &mut impl RaylibDraw, pen: Pen, x: i32, y: i32, w: i32, h: i32, lit: bool) {
    pen.boxed(draw, x, y, w, h, if lit { GOLD } else { SKY });
    if lit {
        pen.rect(draw, x + 1, y + 1, w - 2, 1, CREAM);
    } else {
        pen.rect(draw, x + 1, y + h - 3, w - 2, 2, OCEAN);
        for step in 0..(w.min(h) - 3) {
            pen.px(draw, x + 2 + step, y + h - 3 - step, ICE);
        }
    }
    pen.rect(draw, x + w / 2, y + 1, 1, h - 2, INK);
}

/// A door: shut in `body` colour with a glass panel, or open onto a warm lit interior.
fn door(draw: &mut impl RaylibDraw, lot: Lot<'_>, x: i32, y: i32, w: i32, h: i32, body: Color) {
    let pen = lot.pen;
    pen.boxed(draw, x, y, w, h, body);
    if lot.open {
        pen.rect(draw, x + 1, y + 1, w - 2, h - 1, AMBER);
        pen.rect(draw, x + 1, y + 1, w - 2, 2, ORANGE);
        pen.rect(draw, x + 2, y + h - 3, w - 4, 2, GOLD);
        // Light spilling onto the ground.
        pen.rect(draw, x - 1, LOT_HEIGHT - 1, w + 2, 1, with_alpha(GOLD, 150));
    } else {
        pen.rect(draw, x + 2, y + 2, w - 4, (h / 3).max(3), SKY);
        pen.px(draw, x + 2, y + 2, ICE);
        pen.px(draw, x + w - 3, y + h / 2 + 1, GOLD);
    }
}

/// A sign board centred on art `cx` with its top at `y`, sized to `text`.
fn sign(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    cx: i32,
    y: i32,
    text: &str,
    colors: (Color, Color),
) -> (i32, i32) {
    let (board, ink) = colors;
    let width = text_art_width(text) + 6;
    let x = cx - width / 2;
    pen.boxed(draw, x, y, width, 13, board);
    pen.rect(draw, x + 1, y + 1, width - 2, 1, with_alpha(BONE, 60));
    pen.text(draw, text, x + 3, y + 2, ink);
    (x, width)
}

/// Brick wall: rows of offset bricks with mortar lines.
fn bricks(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    area: (i32, i32, i32, i32),
    colors: (Color, Color, Color),
) {
    let (x, y, w, h) = area;
    let (mortar, brick, light) = colors;
    pen.rect(draw, x, y, w, h, brick);
    for row in 0..=h / 4 {
        let ry = y + row * 4;
        pen.rect(draw, x, ry + 3, w, 1.min(y + h - ry - 3), mortar);
        pen.rect(draw, x, ry, w, 1.min(y + h - ry), light);
        let offset = if row % 2 == 0 { 0 } else { 4 };
        let mut bx = x + offset;
        while bx < x + w {
            pen.rect(draw, bx, ry, 1, 3.min(y + h - ry), mortar);
            bx += 8;
        }
    }
}

/// An `INK`-edged slanted roof over `x..x + w`, rising `h` pixels.
fn pitched_roof(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    colors: (Color, Color),
) {
    let (body, light) = colors;
    for row in 0..h {
        let inset = h - 1 - row;
        pen.rect(draw, x + inset, y + row, w - inset * 2, 1, INK);
        pen.rect(
            draw,
            x + inset + 1,
            y + row,
            w - inset * 2 - 2,
            1,
            if row == 1 { light } else { body },
        );
    }
    pen.rect(draw, x + h - 1, y, w - (h - 1) * 2, 1, INK);
}

// --- Buildings ----------------------------------------------------------------------------------

/// Fuel station: a striped canopy over two pumps, and a kiosk.
fn fuel_station(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Kiosk.
    pen.boxed(draw, 78, 26, 46, 38, BONE);
    pen.rect(draw, 79, 54, 44, 9, MIST);
    pen.rect(draw, 79, 30, 44, 3, ORANGE);
    pen.boxed(draw, 76, 22, 50, 5, RED);
    window(draw, pen, 108, 36, 12, 12, lot.open);
    door(draw, lot, 90, 40, 13, 24, STEEL);
    // Canopy posts.
    for x in [10, 64] {
        pen.boxed(draw, x, 21, 4, 43, STEEL_LIGHT);
        pen.rect(draw, x + 2, 22, 1, 41, STEEL);
    }
    // Canopy.
    pen.boxed(draw, 2, 9, 76, 13, RED);
    pen.rect(draw, 3, 10, 74, 1, ROSE);
    pen.rect(draw, 3, 19, 74, 2, BONE);
    let width = text_art_width(lot.label);
    pen.text(draw, lot.label, 40 - width / 2, 10, BONE);
    // Pumps.
    for x in [22, 44] {
        pen.boxed(draw, x, 44, 11, 20, RED);
        pen.rect(draw, x + 1, 45, 9, 1, ROSE);
        pen.boxed(draw, x + 2, 47, 7, 5, NIGHT);
        let blink = ((lot.seconds * 2.0) as i32 + x) % 2 == 0;
        pen.rect(draw, x + 3, 49, 5, 1, if blink { LIME } else { MOSS });
        pen.rect(draw, x + 2, 55, 7, 2, MAROON);
        pen.rect(draw, x + 11, 50, 2, 1, INK);
        pen.rect(draw, x + 12, 50, 1, 8, INK);
        pen.boxed(draw, x + 1, 41, 9, 4, GOLD);
    }
    // Price board.
    pen.boxed(draw, 112, 2, 12, 20, NIGHT);
    pen.rect(draw, 117, 22, 2, 4, STEEL);
    for row in 0..3 {
        pen.rect(
            draw,
            114,
            4 + row * 6,
            8,
            3,
            if row == 0 { GOLD } else { AMBER },
        );
    }
}

/// Repair garage: a brick workshop with a big roll-up door.
fn repair_garage(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    pen.rect(draw, 4, 20, 120, 44, INK);
    bricks(draw, pen, (5, 21, 118, 43), (SOIL_DARK, RUST, COPPER));
    pitched_roof(draw, pen, 1, 10, 126, 11, (STEEL_DARK, STEEL));
    // Roll-up door.
    pen.boxed(draw, 38, 32, 52, 32, STEEL);
    let raised = if lot.open { 16 } else { 0 };
    for row in 0..(30 - raised) / 3 {
        let y = 33 + row * 3;
        pen.rect(draw, 39, y, 50, 1, STEEL_LIGHT);
        pen.rect(draw, 39, y + 2, 50, 1, STEEL_DARK);
    }
    if lot.open {
        pen.rect(draw, 39, 33 + 30 - raised, 50, raised, AMBER);
        pen.rect(draw, 39, 33 + 30 - raised, 50, 2, ORANGE);
        pen.rect(draw, 40, 62, 48, 1, GOLD);
    } else {
        pen.rect(draw, 60, 58, 8, 2, INK);
    }
    // Sign above the door.
    sign(draw, pen, 64, 18, lot.label, (BONE, RED));
    // Windows and a wrench plaque.
    window(draw, pen, 12, 32, 18, 14, false);
    window(draw, pen, 100, 32, 18, 14, false);
    pen.boxed(draw, 14, 50, 14, 10, BONE);
    for step in 0..8 {
        pen.px(draw, 17 + step, 57 - step / 2, STEEL_DARK);
    }
    pen.rect(draw, 23, 52, 3, 2, STEEL_DARK);
    // Tyre stack.
    for (index, y) in [56, 50].into_iter().enumerate() {
        let x = 104 + index as i32;
        pen.boxed(draw, x, y, 14, 7, SLATE_DARK);
        pen.rect(draw, x + 4, y + 2, 6, 3, NIGHT);
        pen.rect(draw, x + 1, y + 1, 12, 1, SLATE);
    }
}

/// Ore depot: a corrugated warehouse with a loading bay and an ore cart.
fn ore_depot(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Barrel roof: outline every row (and the row above) first, then fill.
    let half_width = |row: i32| {
        let t = (14 - row) as f32 / 14.0;
        (62.0 * (1.0 - t * t).sqrt()).round() as i32
    };
    for row in 0..14 {
        let half = half_width(row);
        pen.rect(draw, 64 - half - 1, 7 + row, half * 2 + 2, 2, INK);
    }
    for row in 0..14 {
        let half = half_width(row);
        let shade = if row < 2 { STEEL_LIGHT } else { STEEL };
        pen.rect(draw, 64 - half, 8 + row, half * 2, 1, shade);
    }
    for x in (8..120).step_by(8) {
        pen.rect(draw, x, 12, 1, 10, STEEL_DARK);
    }
    // Corrugated walls.
    pen.rect(draw, 3, 21, 122, 43, INK);
    for x in 4..124 {
        let color = match x % 4 {
            0 => STEEL_DARK,
            3 => STEEL_LIGHT,
            _ => STEEL,
        };
        pen.rect(draw, x, 22, 1, 42, color);
    }
    pen.rect(draw, 4, 22, 120, 1, STEEL_LIGHT);
    // Loading bay.
    pen.boxed(draw, 42, 36, 44, 28, NIGHT);
    pen.rect(draw, 43, 37, 42, 2, INK);
    let lamp = if lot.open { GOLD } else { AMBER };
    pen.rect(draw, 63, 39, 2, 2, lamp);
    if lot.open {
        pen.rect(draw, 43, 41, 42, 23, with_alpha(AMBER, 90));
    }
    for (x, y) in [(46, 52), (56, 52), (51, 44), (72, 54)] {
        pen.boxed(draw, x, y, 10, 10, LOAM);
        pen.rect(draw, x + 1, y + 1, 8, 1, CLAY);
        pen.rect(draw, x + 1, y + 4, 8, 1, SOIL);
    }
    // Stripes either side of the bay.
    for row in 0..14 {
        let color = if row % 2 == 0 { AMBER } else { INK };
        pen.rect(draw, 38, 36 + row * 2, 3, 2, color);
        pen.rect(draw, 87, 36 + row * 2, 3, 2, color);
    }
    sign(draw, pen, 64, 22, lot.label, (SLATE_DARK, GOLD));
    // Ore cart.
    pen.rect(draw, 96, 50, 24, 1, INK);
    pen.boxed(draw, 97, 50, 22, 10, RUST);
    pen.rect(draw, 98, 51, 20, 1, COPPER);
    for (index, color) in [GOLD, COPPER, STEEL_LIGHT, GOLD, LIME, COPPER, BONE]
        .into_iter()
        .enumerate()
    {
        let i = index as i32;
        pen.rect(draw, 99 + i * 3, 47 + (i % 2), 3, 3, INK);
        pen.rect(draw, 99 + i * 3, 47 + (i % 2), 2, 2, color);
    }
    for x in [100, 112] {
        pen.boxed(draw, x, 59, 5, 5, SLATE_DARK);
        pen.px(draw, x + 2, 61, STEEL_LIGHT);
    }
    // Small window.
    window(draw, pen, 12, 30, 14, 12, false);
}

/// Headquarters: a two-storey office with a rooftop mast, radar dish and flag.
fn headquarters(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Mast, dish and flag above the roof.
    pen.rect(draw, 30, 0, 1, 9, STEEL_LIGHT);
    pen.rect(draw, 28, 4, 5, 1, STEEL);
    let blink = (lot.seconds * 1.5) as i32 % 2 == 0;
    pen.px(draw, 30, 0, if blink { RED } else { MAROON });
    pen.rect(draw, 86, 3, 9, 2, STEEL_LIGHT);
    pen.rect(draw, 85, 2, 1, 2, STEEL);
    pen.rect(draw, 95, 2, 1, 2, STEEL);
    pen.rect(draw, 90, 5, 1, 4, STEEL);
    pen.rect(draw, 102, 0, 1, 9, STEEL_LIGHT);
    let wave = (lot.seconds * 4.0) as i32 % 2;
    pen.rect(draw, 103, 0, 8, 4, SKY);
    pen.rect(draw, 103, 4 - wave, 8, 1 + wave, OCEAN);
    pen.px(draw, 106, 1, GOLD);
    // Tower.
    pen.boxed(draw, 18, 8, 92, 56, MIST);
    pen.rect(draw, 19, 9, 90, 3, SLATE_LIGHT);
    pen.rect(draw, 19, 12, 90, 1, FOG);
    pen.rect(draw, 104, 13, 5, 50, FOG);
    pen.rect(draw, 19, 34, 90, 2, SLATE_LIGHT);
    pen.rect(draw, 19, 60, 90, 3, FOG);
    for (row, y) in [(0, 15), (1, 39)] {
        // Two windows each side, mirrored about the centre plaque and entrance.
        for x in [23, 39, 77, 93] {
            window(draw, pen, x, y, 12, 12, row == 1 && lot.open);
        }
    }
    // Plaque with the sign, and the entrance.
    sign(draw, pen, 64, 16, lot.label, (DEEP_BLUE, SKY_LIGHT));
    pen.boxed(draw, 54, 36, 20, 4, SLATE);
    door(draw, lot, 56, 40, 16, 22, SLATE_DARK);
    pen.rect(draw, 64, 41, 1, 20, INK);
    pen.rect(draw, 50, 62, 28, 2, STONE_LIGHT);
    pen.rect(draw, 50, 62, 28, 1, BONE);
}

/// Upgrade shop: a purple storefront with a striped awning and a drill in the window.
fn upgrade_shop(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    pen.boxed(draw, 8, 18, 112, 46, VIOLET);
    pen.rect(draw, 9, 19, 110, 2, LAVENDER);
    pen.rect(draw, 9, 57, 110, 6, PLUM);
    // Marquee sign with chasing bulbs.
    let (x, width) = sign(draw, pen, 64, 3, lot.label, (PLUM, GOLD));
    let phase = (lot.seconds * 6.0) as i32;
    for (index, bx) in (x + 2..x + width - 2).step_by(3).enumerate() {
        let on = (index as i32 + phase) % 3 == 0;
        pen.px(draw, bx, 3, if on { CREAM } else { AMBER });
        pen.px(draw, bx, 15, if on { AMBER } else { CREAM });
    }
    pen.rect(draw, 40, 16, 2, 2, STEEL_DARK);
    pen.rect(draw, 86, 16, 2, 2, STEEL_DARK);
    // Awning.
    pen.rect(draw, 5, 26, 118, 8, INK);
    for stripe in 0..14 {
        let color = if stripe % 2 == 0 { PINK } else { BONE };
        let sx = 6 + stripe * 8;
        pen.rect(draw, sx, 27, 8, 6, color);
        pen.rect(draw, sx + 1, 33, 6, 1, color);
        pen.rect(draw, sx + 1, 34, 6, 1, INK);
    }
    pen.rect(draw, 6, 27, 116, 1, with_alpha(BONE, 120));
    // Display window with a drill head on show.
    window(draw, pen, 14, 38, 46, 18, false);
    pen.rect(draw, 15, 52, 44, 3, PLUM);
    pen.sprite(draw, &rig::DRILL_DOWN, 0, 32, 44);
    pen.boxed(draw, 46, 46, 9, 6, BONE);
    pen.rect(draw, 48, 48, 5, 1, RED);
    // Door.
    door(draw, lot, 78, 38, 16, 26, PLUM);
    pen.boxed(draw, 100, 40, 14, 10, PLUM);
    pen.rect(draw, 102, 43, 10, 1, GOLD);
    pen.rect(draw, 102, 46, 7, 1, LAVENDER);
}

/// Bank: a little temple with columns, a pediment and a gold coin.
fn bank(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Pediment: each row is outlined across its full width and filled only under the row
    // above, so the stepped slope keeps a continuous ink edge.
    for row in 0..12 {
        let half = 10 + row * 5;
        pen.rect(draw, 64 - half, 4 + row, half * 2, 1, INK);
        if row > 0 {
            let inner = half - 5;
            let color = if row < 3 { BONE } else { MIST };
            pen.rect(draw, 64 - inner, 4 + row, inner * 2, 1, color);
        }
    }
    pen.rect(draw, 4, 15, 120, 1, INK);
    pen.rect(draw, 58, 8, 12, 7, INK);
    pen.rect(draw, 59, 7, 10, 9, INK);
    pen.rect(draw, 59, 8, 10, 7, GOLD);
    pen.rect(draw, 60, 9, 4, 2, CREAM);
    pen.rect(draw, 63, 10, 2, 4, AMBER);
    // Frieze with the name.
    pen.boxed(draw, 6, 15, 116, 13, BONE);
    pen.rect(draw, 7, 26, 114, 1, MIST);
    let width = text_art_width(lot.label);
    pen.text(draw, lot.label, 64 - width / 2, 17, SLATE_DARK);
    // Back wall and columns.
    pen.rect(draw, 10, 28, 108, 26, INK);
    pen.rect(draw, 11, 28, 106, 26, STONE_LIGHT);
    for x in [12, 30, 88, 106] {
        pen.boxed(draw, x - 1, 28, 12, 3, BONE);
        pen.boxed(draw, x, 30, 10, 22, BONE);
        pen.rect(draw, x + 6, 31, 3, 20, MIST);
        pen.rect(draw, x + 3, 31, 1, 20, MIST);
        pen.boxed(draw, x - 1, 51, 12, 3, BONE);
    }
    window(draw, pen, 46, 33, 8, 10, false);
    window(draw, pen, 74, 33, 8, 10, false);
    door(draw, lot, 56, 34, 16, 20, ORANGE);
    pen.rect(draw, 64, 35, 1, 18, INK);
    // Steps.
    for (index, inset) in [0, 4, 8].into_iter().enumerate() {
        let y = 63 - index as i32 * 3;
        pen.boxed(draw, 4 + inset, y - 2, 120 - inset * 2, 4, STONE_LIGHT);
        pen.rect(draw, 5 + inset, y - 1, 118 - inset * 2, 1, BONE);
    }
}

/// Explosives bunker: a squat concrete blockhouse with hazard stripes and a warning sign.
fn explosives_bunker(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Warning sign on poles.
    pen.rect(draw, 44, 18, 2, 12, STEEL_DARK);
    pen.rect(draw, 82, 18, 2, 12, STEEL_DARK);
    let (x, _) = sign(draw, pen, 66, 6, lot.label, (AMBER, MAROON));
    pen.sprite(draw, &props::BOMB, 0, x - 11, 6);
    // Bunker.
    pen.boxed(draw, 8, 28, 112, 36, STONE);
    pen.boxed(draw, 6, 25, 116, 5, STONE_LIGHT);
    for row in 0..4 {
        for x in 0..110 {
            if (x + row) % 8 < 4 {
                pen.px(draw, 9 + x, 30 + row, AMBER);
            } else {
                pen.px(draw, 9 + x, 30 + row, INK);
            }
        }
    }
    for x in (24..116).step_by(16) {
        pen.rect(draw, x, 35, 1, 28, STONE_DARK);
    }
    pen.rect(draw, 9, 61, 110, 2, STONE_DARK);
    // Blast door.
    door(draw, lot, 56, 38, 18, 26, STEEL_DARK);
    for (dx, dy) in [(2, 2), (15, 2), (2, 23), (15, 23)] {
        pen.px(draw, 56 + dx, 38 + dy, STEEL_LIGHT);
    }
    // Slit window.
    pen.boxed(draw, 22, 40, 22, 4, NIGHT);
    // Sandbags.
    for (index, (bx, by)) in [(14, 58), (22, 58), (30, 58), (18, 54), (26, 54)]
        .into_iter()
        .enumerate()
    {
        pen.boxed(draw, bx, by, 9, 5, SAND);
        pen.rect(draw, bx + 1, by + 1, 7, 1, CREAM);
        if index % 2 == 0 {
            pen.px(draw, bx + 4, by + 3, CLAY);
        }
    }
    // TNT barrels.
    for bx in [88, 99, 110] {
        pen.boxed(draw, bx, 50, 10, 14, RED);
        pen.rect(draw, bx + 1, 51, 2, 12, ROSE);
        pen.rect(draw, bx + 1, 55, 8, 2, BONE);
    }
}

/// Salvage yard: a plank shack with a rusty roof, a scrap heap and a crane.
fn salvage_yard(draw: &mut impl RaylibDraw, lot: Lot<'_>) {
    let pen = lot.pen;
    // Crane.
    pen.boxed(draw, 12, 4, 5, 60, STEEL_DARK);
    for y in (6..62).step_by(4) {
        pen.px(draw, 13, y, STEEL);
        pen.px(draw, 15, y + 2, STEEL);
    }
    pen.boxed(draw, 10, 2, 46, 4, AMBER);
    pen.rect(draw, 11, 3, 44, 1, GOLD);
    let sway = ((lot.seconds * 1.2).sin() * 1.5).round() as i32;
    pen.rect(draw, 48 + sway / 2, 6, 1, 18, STEEL_LIGHT);
    pen.boxed(draw, 44 + sway, 24, 9, 4, SLATE);
    pen.rect(draw, 45 + sway, 25, 7, 1, STEEL_LIGHT);
    // Scrap heap.
    for (index, (x, y, w, h)) in [
        (20, 52, 40, 12),
        (24, 46, 30, 8),
        (30, 40, 18, 7),
        (4, 56, 20, 8),
    ]
    .into_iter()
    .enumerate()
    {
        pen.boxed(draw, x, y, w, h, STEEL_DARK);
        for chunk in 0..w * h / 22 {
            let hsh = hash(index as i32, chunk, 81);
            let cx = x + 1 + (hsh % (w - 3) as u32) as i32;
            let cy = y + 1 + ((hsh >> 8) % (h - 2).max(1) as u32) as i32;
            let color = match hsh >> 20 & 3 {
                0 => RUST,
                1 => STEEL,
                2 => COPPER,
                _ => SLATE,
            };
            pen.rect(draw, cx, cy, 2, 1, color);
        }
    }
    pen.boxed(draw, 34, 54, 10, 10, INK);
    pen.rect(draw, 37, 57, 4, 4, SLATE_DARK);
    // Shack.
    pen.rect(draw, 64, 22, 60, 42, INK);
    for row in 0..14 {
        let y = 23 + row * 3;
        pen.rect(
            draw,
            65,
            y,
            58,
            3.min(64 - y),
            if row % 2 == 0 { LOAM } else { SOIL },
        );
        pen.rect(draw, 65, y + 2, 58, 1.min(63 - y - 1), SOIL_DARK);
    }
    // Rusty sloped roof.
    for row in 0..8 {
        pen.rect(draw, 60, 14 + row, 68, 1, INK);
        let color = if row == 0 {
            COPPER
        } else if (row % 2) == 0 {
            RUST
        } else {
            COPPER
        };
        pen.rect(draw, 61, 14 + row, 66, 1, color);
    }
    for x in (63..126).step_by(5) {
        pen.rect(draw, x, 15, 1, 7, MAROON);
    }
    sign(draw, pen, 94, 24, lot.label, (SOIL_DARK, AMBER));
    door(draw, lot, 76, 40, 14, 24, SOIL);
    pen.rect(draw, 77, 46, 12, 1, LOAM);
    pen.rect(draw, 77, 54, 12, 1, LOAM);
    window(draw, pen, 104, 42, 12, 10, false);
    pen.px(draw, 109, 45, INK);
    pen.px(draw, 110, 46, INK);
}
