//! Fluids (Part III F9): water, acid, brine and liquid metal drawn live over the terrain.
//!
//! The terrain chunks paint a fluid tile as the open ground it fills (so a flood never repaints
//! a chunk); this pass draws the fluid on top each frame, after the rigs and pilots, so a
//! submerged rig shows through it. Everything is a handful of rectangles per tile (cheap on the
//! browser build):
//!
//! * the body: a translucent fill up to the cell's level, its colour and opacity deepening with
//!   the fluid stacked above it (a depth tint),
//! * the surface: a bright, slowly rolling waveline on cells with open air over them,
//! * flow: falling streaks where fluid pours into the cell below, and drifting glints where it
//!   runs sideways,
//! * bubbles rising through acid, brine and molten metal (and the odd one through deep water).
//!
//! Animation is a pure function of the clock and the tile position, so it never needs state.

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    fluid::{FLUID_FULL, FluidCell, FluidKind},
    terrain::{TILE_SIZE, Terrain, TilePosition},
};

use crate::rendering::{
    pixel_art::palette::{
        AMBER, CREAM, ICE, LEAF, MINT, OCEAN, ORANGE, SKY_LIGHT, TEAL_DARK, with_alpha,
    },
    terrain::tile_art::{fluid_color, hash},
};

use super::{ART, snap};

/// Fluid cells stacked above that the depth tint counts (deeper looks the same).
const DEPTH_TINT_ROWS: i32 = 6;
/// Columns a surface waveline is drawn in, per tile.
const WAVE_COLUMNS: i32 = 8;

/// Colours of a fluid: its body near the surface, deep down, the surface highlight and the
/// bubbles and glints in it.
struct FluidLook {
    shallow: Color,
    deep: Color,
    highlight: Color,
    fleck: Color,
    /// Body opacity at the surface and fully deep.
    alpha: (u8, u8),
    /// Bubbles per tile per second (0 for none), in one tile out of `bubble_tiles`.
    bubbles: f32,
    bubble_tiles: u32,
    /// How far the surface rolls (art pixels) and how fast.
    wave: (f32, f32),
}

const fn look(kind: FluidKind) -> FluidLook {
    match kind {
        FluidKind::Water => FluidLook {
            shallow: fluid_color(FluidKind::Water),
            deep: OCEAN,
            highlight: ICE,
            fleck: SKY_LIGHT,
            alpha: (115, 190),
            bubbles: 0.5,
            bubble_tiles: 6,
            wave: (1.0, 2.2),
        },
        FluidKind::Acid => FluidLook {
            shallow: fluid_color(FluidKind::Acid),
            deep: LEAF,
            highlight: CREAM,
            fleck: CREAM,
            alpha: (140, 205),
            bubbles: 1.2,
            bubble_tiles: 2,
            wave: (1.0, 3.0),
        },
        FluidKind::Brine => FluidLook {
            shallow: fluid_color(FluidKind::Brine),
            deep: TEAL_DARK,
            highlight: MINT,
            fleck: ICE,
            alpha: (145, 210),
            bubbles: 0.5,
            bubble_tiles: 5,
            wave: (0.6, 1.6),
        },
        FluidKind::LiquidMetal => FluidLook {
            shallow: fluid_color(FluidKind::LiquidMetal),
            deep: ORANGE,
            highlight: CREAM,
            fleck: AMBER,
            alpha: (215, 245),
            bubbles: 0.7,
            bubble_tiles: 3,
            wave: (0.5, 0.9),
        },
    }
}

fn mix(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color::new(
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
        255,
    )
}

/// The tiles overlapping `visible`.
fn visible_tiles(visible: Rectangle) -> impl Iterator<Item = TilePosition> {
    // `visible` is in the world's local pixels (F7): tiles go through the frame.
    let frame = crate::view_frame::frame();
    let min_x = frame.col(visible.x);
    let min_y = frame.row(visible.y);
    let max_x = frame.col_ceil(visible.x + visible.width);
    let max_y = frame.row_ceil(visible.y + visible.height);
    (min_y..max_y).flat_map(move |y| (min_x..max_x).map(move |x| TilePosition { x, y }))
}

/// A 0..1 phase from a tile hash (so neighbouring tiles never animate in lockstep).
fn phase(position: TilePosition, salt: u32) -> f32 {
    (hash(position.x, position.y, salt) % 1_000) as f32 / 1_000.0
}

/// Draws every explored fluid cell in `visible`.
pub(super) fn draw_fluids(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    explored: Option<&DiscoveredTiles>,
    visible: Rectangle,
    seconds: f32,
) {
    for position in visible_tiles(visible) {
        let Some(cell) = terrain.fluid_at(position) else {
            continue;
        };
        if !explored.is_some_and(|tiles| tiles.contains(position)) {
            continue;
        }
        draw_cell(draw, terrain, position, cell, seconds);
    }
}

/// A surface cell's body: narrow columns whose tops roll with the waveline, each capped with
/// the bright skin of the surface. `(left, surface, bottom)` in world pixels.
fn draw_surface(
    draw: &mut impl RaylibDraw,
    style: &FluidLook,
    (left, surface, bottom): (f32, f32, f32),
    body: Color,
    seconds: f32,
) {
    let column = TILE_SIZE / WAVE_COLUMNS as f32;
    let (roll, speed) = style.wave;
    for index in 0..WAVE_COLUMNS {
        let x = left + index as f32 * column;
        let wave = ((x * 0.09) + seconds * speed).sin() * roll * ART;
        let top = snap(surface + wave).clamp(bottom - TILE_SIZE, bottom - ART);
        draw.draw_rectangle_rec(Rectangle::new(x, top, column, bottom - top), body);
        draw.draw_rectangle_rec(
            Rectangle::new(x, top, column, ART),
            with_alpha(style.highlight, 190),
        );
    }
}

fn draw_cell(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    position: TilePosition,
    cell: FluidCell,
    seconds: f32,
) {
    let style = look(cell.kind);
    let at = |dx: i32, dy: i32| {
        terrain.fluid_at(TilePosition {
            x: position.x + dx,
            y: position.y + dy,
        })
    };
    let above = at(0, -1).filter(|over| over.kind == cell.kind);
    let below_open = terrain
        .tile(TilePosition {
            x: position.x,
            y: position.y + 1,
        })
        .is_some_and(|tile| tile.kind.is_open());
    let pouring = below_open && at(0, 1).is_none_or(|under| under.level < FLUID_FULL);
    // Depth tint: how much of this fluid is stacked over the cell.
    let depth = (1..=DEPTH_TINT_ROWS)
        .take_while(|dy| at(0, -dy).is_some_and(|over| over.kind == cell.kind))
        .count() as f32
        / DEPTH_TINT_ROWS as f32;
    let body = mix(style.shallow, style.deep, depth);
    let alpha =
        f32::from(style.alpha.0) + (f32::from(style.alpha.1) - f32::from(style.alpha.0)) * depth;
    let body = with_alpha(body, alpha as u8);
    let frame = crate::view_frame::frame();
    let left = frame.col_px(position.x);
    let bottom = frame.row_px(position.y + 1);
    // A cell under the same fluid is full to its top whatever its own level says (the column
    // is continuous); a surface cell fills to its level.
    let fill = if above.is_some() { 1.0 } else { cell.fill() };
    let surface = snap(fill.mul_add(-TILE_SIZE, bottom));
    if above.is_some() {
        draw.draw_rectangle_rec(
            Rectangle::new(left, surface, TILE_SIZE, bottom - surface),
            body,
        );
    } else {
        draw_surface(draw, &style, (left, surface, bottom), body, seconds);
    }
    if pouring {
        // Falling streaks running down through the cell.
        for streak in 0..2_u32 {
            let lane = (hash(position.x, position.y, 40 + streak) % 12) as f32 + 2.0;
            let travel = (seconds * 2.6 + phase(position, 50 + streak)).fract();
            let y = snap((bottom - TILE_SIZE) + travel * TILE_SIZE);
            draw.draw_rectangle_rec(
                Rectangle::new(snap(left + lane * ART), y, ART, ART * 4.0),
                with_alpha(style.highlight, 150),
            );
        }
    } else {
        // Running sideways towards a lower neighbour: glints drifting that way.
        let lower = |dx: i32| {
            terrain
                .tile(TilePosition {
                    x: position.x + dx,
                    y: position.y,
                })
                .is_some_and(|tile| tile.kind.is_open())
                && at(dx, 0).is_none_or(|side| side.level + 6 < cell.level)
        };
        let drift = match (lower(-1), lower(1)) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        if drift != 0.0 && above.is_none() {
            let travel = (seconds * 1.4 + phase(position, 60)).fract();
            let x = if drift > 0.0 {
                left + travel * TILE_SIZE
            } else {
                left + TILE_SIZE - travel * TILE_SIZE
            };
            draw.draw_rectangle_rec(
                Rectangle::new(snap(x), snap(surface + ART * 2.0), ART * 3.0, ART),
                with_alpha(style.fleck, 170),
            );
        }
    }
    // Bubbles rising from the floor of the cell to its surface.
    if style.bubbles > 0.0
        && cell.level > FLUID_FULL / 4
        && hash(position.x, position.y, 72).is_multiple_of(style.bubble_tiles)
    {
        let rate = style.bubbles * if depth > 0.3 { 1.0 } else { 0.6 };
        let cycle = (seconds * rate + phase(position, 70)).fract();
        if cycle < 0.6 {
            let rise = cycle / 0.6;
            let lane = (hash(position.x, position.y, 71) % 13) as f32 + 1.5;
            let y = bottom - rise * (bottom - surface);
            let wobble = (seconds * 7.0 + lane).sin() * ART;
            draw.draw_rectangle_rec(
                Rectangle::new(snap(left + lane * ART + wobble), snap(y), ART, ART),
                with_alpha(style.fleck, 200),
            );
        }
    }
}
