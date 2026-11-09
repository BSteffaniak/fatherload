//! Room dressing for the walk-in buildings: each building's signature furniture, wall pieces and
//! counter-top details, drawn in room art pixels (see the parent module) from palette colours.
//!
//! Props sit in two zones that never overlap the walking rules: the "showroom" between the
//! exit door and the counter, and the strip right of the counter. Everything here is
//! decoration; the door and counter hotspots stay those of the shell.

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::{CEILING_Y, COUNTER_HEIGHT, COUNTER_WIDTH, FLOOR_Y, room_sprite, ui_icon};
use crate::render_snapshot::MarketQuote;
use crate::rendering::{
    pixel_art::{
        palette,
        sprites::{icons, interior as art, rig},
    },
    ui_kit::{self, TextStyle},
};
use fatherload_simulation::{economy::SurfaceZone, terrain::MineralKind};

/// What a room's dressing may react to (live prices on the depot board, the clock).
pub(super) struct RoomContext<'a> {
    pub zone: SurfaceZone,
    pub counter_x: i32,
    pub seconds: f32,
    pub market: &'a [MarketQuote],
}

/// Solid rectangle.
fn fill(draw: &mut impl RaylibDraw, x: i32, y: i32, width: i32, height: i32, color: Color) {
    draw.draw_rectangle(x, y, width, height, color);
}

/// An ink-outlined box with a light top-left edge and a shaded bottom-right edge.
#[allow(clippy::too_many_arguments, reason = "one call draws one shaded box")]
fn bevel(
    draw: &mut impl RaylibDraw,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    light: Color,
    body: Color,
    shade: Color,
) {
    fill(draw, x, y, width, height, INK);
    fill(draw, x + 1, y + 1, width - 2, height - 2, body);
    fill(draw, x + 1, y + 1, width - 2, 1, light);
    fill(draw, x + 1, y + 1, 1, height - 2, light);
    fill(draw, x + 1, y + height - 2, width - 2, 1, shade);
    fill(draw, x + width - 2, y + 1, 1, height - 2, shade);
}

/// A filled disc of radius `radius` centred on pixel `(cx, cy)`, built from whole-pixel spans.
fn disc(draw: &mut impl RaylibDraw, cx: i32, cy: i32, radius: i32, color: Color) {
    for dy in -radius..=radius {
        let span = (((radius * radius - dy * dy) as f32) + radius as f32 * 0.6).sqrt() as i32;
        fill(draw, cx - span, cy + dy, span * 2 + 1, 1, color);
    }
}

/// A soft shadow on the floor under a prop `width` wide.
fn floor_shadow(draw: &mut impl RaylibDraw, x: i32, width: i32) {
    fill(
        draw,
        x + 1,
        FLOOR_Y,
        width - 2,
        2,
        palette::with_alpha(INK, 110),
    );
    fill(
        draw,
        x + 3,
        FLOOR_Y + 2,
        width - 6,
        1,
        palette::with_alpha(INK, 60),
    );
}

/// A framed window with a slice of daytime sky, a far ridge and the town's dust.
fn window(draw: &mut impl RaylibDraw, x: i32, y: i32, width: i32, height: i32, seconds: f32) {
    bevel(draw, x - 3, y - 3, width + 6, height + 6, SAND, LOAM, SOIL);
    fill(draw, x, y, width, height, SKY);
    fill(draw, x, y, width, height / 3, SKY_LIGHT);
    // A slow cloud.
    let cloud_x = x + ((seconds * 2.0) as i32).rem_euclid(width + 16) - 12;
    for (dx, dy, w) in [(0, 0, 10), (3, -2, 5), (2, 1, 8)] {
        let left = (cloud_x + dx).max(x);
        let right = (cloud_x + dx + w).min(x + width);
        if right > left {
            fill(draw, left, y + 5 + dy, right - left, 2, BONE);
        }
    }
    // Ridge and ground.
    for column in 0..width {
        let phase = column as f32 / width as f32 * std::f32::consts::TAU;
        let ridge = (5.0 + 3.0 * (phase * 1.3).sin() + 1.5 * (phase * 3.1).cos()).round() as i32;
        fill(
            draw,
            x + column,
            y + height - 6 - ridge,
            1,
            ridge,
            SLATE_LIGHT,
        );
        fill(draw, x + column, y + height - 6 - ridge, 1, 1, MIST);
    }
    fill(draw, x, y + height - 6, width, 6, LOAM);
    fill(draw, x, y + height - 6, width, 1, SAND);
    // Mullions.
    fill(draw, x + width / 2, y, 1, height, LOAM);
    fill(draw, x, y + height / 2, width, 1, LOAM);
    // Glass glint.
    fill(draw, x + 2, y + 2, 1, 4, ICE);
    fill(draw, x + 3, y + 2, 2, 1, ICE);
    // Sill.
    fill(draw, x - 5, y + height + 3, width + 10, 3, INK);
    fill(draw, x - 4, y + height + 3, width + 8, 1, SAND);
    fill(draw, x - 4, y + height + 4, width + 8, 1, CLAY);
}

/// A framed paper poster with pseudo-text lines and a coloured header band.
fn poster(draw: &mut impl RaylibDraw, x: i32, y: i32, width: i32, height: i32, header: Color) {
    fill(draw, x, y, width, height, INK);
    fill(draw, x + 1, y + 1, width - 2, height - 2, CREAM);
    fill(draw, x + 1, y + 1, width - 2, 4, header);
    let mut line_y = y + 8;
    let mut row = 0;
    while line_y < y + height - 3 {
        let line_width = (width - 6) - (row * 5) % 7;
        fill(draw, x + 3, line_y, line_width, 1, SAND);
        line_y += 3;
        row += 1;
    }
    // Pin.
    fill(draw, x + width / 2, y - 1, 1, 2, RED);
}

/// A potted plant standing on the floor at `x`.
fn plant(draw: &mut impl RaylibDraw, x: i32) {
    let base = FLOOR_Y;
    // Pot.
    bevel(draw, x + 2, base - 10, 12, 10, CLAY, LOAM, SOIL);
    fill(draw, x + 1, base - 11, 14, 3, INK);
    fill(draw, x + 2, base - 10, 12, 1, SAND);
    // Leaves.
    for (dx, dy, w, h, color) in [
        (6, -26, 4, 15, MOSS),
        (2, -22, 4, 10, LEAF),
        (10, -23, 4, 11, LEAF),
        (0, -17, 4, 6, MOSS),
        (12, -18, 4, 7, MOSS),
        (7, -28, 2, 4, LIME),
        (3, -21, 1, 3, LIME),
        (11, -22, 1, 3, LIME),
    ] {
        fill(draw, x + dx - 1, base + dy - 1, w + 2, h + 2, INK);
        fill(draw, x + dx, base + dy, w, h, color);
    }
    // Re-fill so the leaf outlines do not cut each other.
    for (dx, dy, w, h, color) in [
        (6, -26, 4, 15, MOSS),
        (2, -22, 4, 10, LEAF),
        (10, -23, 4, 11, LEAF),
        (7, -28, 2, 4, LIME),
    ] {
        fill(draw, x + dx, base + dy, w, h, color);
    }
    floor_shadow(draw, x + 1, 14);
}

/// A wall clock (round face, ticking hand) centred on `(cx, cy)`.
fn wall_clock(draw: &mut impl RaylibDraw, cx: i32, cy: i32, seconds: f32) {
    disc(draw, cx, cy, 7, INK);
    disc(draw, cx, cy, 6, LOAM);
    disc(draw, cx, cy, 5, CREAM);
    fill(draw, cx, cy - 4, 1, 4, INK);
    let tick = (seconds as i32).rem_euclid(4);
    let (hx, hy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][usize::try_from(tick).unwrap_or(0)];
    for step in 1..=3 {
        fill(draw, cx + hx * step, cy + hy * step, 1, 1, RED);
    }
    fill(draw, cx, cy, 1, 1, INK);
}

/// A stack of tyres lying flat, `count` high, standing on the floor at `x`.
fn tyres(draw: &mut impl RaylibDraw, x: i32, count: i32) {
    const SPANS: [(i32, i32); 7] = [
        (3, 12),
        (1, 16),
        (0, 18),
        (0, 18),
        (0, 18),
        (1, 16),
        (3, 12),
    ];
    for index in 0..count {
        let y = FLOOR_Y - 7 * (index + 1);
        for (row, (dx, width)) in SPANS.into_iter().enumerate() {
            let row = i32::try_from(row).unwrap_or(0);
            fill(draw, x + dx, y + row, width, 1, INK);
            if (1..6).contains(&row) {
                let color = if row == 1 { SLATE } else { SLATE_DARK };
                fill(draw, x + dx + 1, y + row, width - 2, 1, color);
            }
        }
        // Tread notches and the rim seen through the hole.
        for notch in (x + 3..x + 16).step_by(3) {
            fill(draw, notch, y + 3, 1, 2, NIGHT);
        }
        fill(draw, x + 6, y + 2, 6, 1, NIGHT);
        fill(draw, x + 7, y + 2, 4, 1, STEEL_DARK);
    }
    floor_shadow(draw, x, 18);
}

/// Draws the dressing behind the counter for `ctx.zone` (everything except the counter top).
pub(super) fn draw_room_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    match ctx.zone {
        SurfaceZone::Fuel => fuel_props(draw, ctx),
        SurfaceZone::Repair => repair_props(draw, ctx),
        SurfaceZone::Depot => depot_props(draw, ctx),
        SurfaceZone::Headquarters => hq_props(draw, ctx),
        SurfaceZone::Bank => bank_props(draw, ctx),
        SurfaceZone::Explosives => explosives_props(draw, ctx),
        SurfaceZone::Salvage => salvage_props(draw, ctx),
        SurfaceZone::Shop => {}
    }
}

/// Details drawn on and in front of the counter (teller bars, registers, goods on display).
pub(super) fn draw_counter_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    let top = FLOOR_Y - COUNTER_HEIGHT;
    let x = ctx.counter_x;
    match ctx.zone {
        SurfaceZone::Fuel => register(draw, x + COUNTER_WIDTH - 22, top),
        SurfaceZone::Repair => {
            // A spare drill head and a rag on the counter.
            let drill = &rig::DRILL_DOWN;
            room_sprite(draw, drill, 0, x + 6, top - drill.height(), false);
            fill(draw, x + COUNTER_WIDTH - 18, top - 2, 9, 2, INK);
            fill(draw, x + COUNTER_WIDTH - 17, top - 2, 7, 1, RED);
        }
        SurfaceZone::Depot => scale(draw, x + COUNTER_WIDTH - 22, top),
        SurfaceZone::Headquarters => {
            // Desk monitor and a paper tray.
            bevel(
                draw,
                x + COUNTER_WIDTH - 24,
                top - 13,
                16,
                11,
                STEEL_LIGHT,
                STEEL,
                STEEL_DARK,
            );
            fill(draw, x + COUNTER_WIDTH - 22, top - 11, 12, 7, TEAL_DARK);
            let blink = (ctx.seconds * 2.0) as i32 % 2 == 0;
            fill(draw, x + COUNTER_WIDTH - 21, top - 10, 6, 1, MINT);
            fill(draw, x + COUNTER_WIDTH - 21, top - 8, 9, 1, TEAL);
            if blink {
                fill(draw, x + COUNTER_WIDTH - 21, top - 6, 2, 1, MINT);
            }
            fill(draw, x + COUNTER_WIDTH - 17, top - 2, 2, 2, INK);
            fill(draw, x + 6, top - 3, 14, 3, INK);
            fill(draw, x + 7, top - 3, 12, 1, BONE);
            fill(draw, x + 7, top - 2, 12, 1, MIST);
        }
        SurfaceZone::Bank => teller_bars(draw, x, top),
        SurfaceZone::Explosives => {
            // A display bomb and a bundle of dynamite sticks.
            ui_icon(draw, &icons::BOMB, x + 6, top - 9);
            for stick in 0..3 {
                let sx = x + COUNTER_WIDTH - 20 + stick * 4;
                fill(draw, sx, top - 10, 4, 10, INK);
                fill(draw, sx + 1, top - 9, 2, 9, RED);
                fill(draw, sx + 1, top - 9, 1, 9, ORANGE);
            }
            fill(draw, x + COUNTER_WIDTH - 20, top - 6, 12, 2, INK);
            fill(draw, x + COUNTER_WIDTH - 19, top - 6, 10, 1, SAND);
            fill(draw, x + COUNTER_WIDTH - 15, top - 13, 1, 3, SAND);
        }
        SurfaceZone::Salvage => {
            ui_icon(draw, &art::GEAR, x + 6, top - 7);
            ui_icon(draw, &art::GEAR, x + 12, top - 7);
            register(draw, x + COUNTER_WIDTH - 22, top);
        }
        SurfaceZone::Shop => {}
    }
}

/// A cash register standing on a counter whose top is at `top`.
fn register(draw: &mut impl RaylibDraw, x: i32, top: i32) {
    let y = top - 9;
    bevel(draw, x, y, 16, 10, STEEL_LIGHT, STEEL, STEEL_DARK);
    fill(draw, x + 3, y + 3, 8, 3, INK);
    fill(draw, x + 4, y + 4, 6, 1, LIME);
    fill(draw, x + 12, y + 3, 2, 1, BONE);
    fill(draw, x + 12, y + 5, 2, 1, BONE);
}

/// A weighing scale on the depot counter.
fn scale(draw: &mut impl RaylibDraw, x: i32, top: i32) {
    fill(draw, x, top - 3, 18, 3, INK);
    fill(draw, x + 1, top - 3, 16, 1, STEEL_LIGHT);
    fill(draw, x + 1, top - 2, 16, 1, STEEL);
    fill(draw, x + 8, top - 12, 2, 9, INK);
    disc(draw, x + 9, top - 15, 4, INK);
    disc(draw, x + 9, top - 15, 3, CREAM);
    fill(draw, x + 9, top - 17, 1, 2, RED);
    // A few ore lumps on the pan.
    for (dx, color) in [(3, COPPER), (6, GOLD), (12, STEEL_LIGHT)] {
        fill(draw, x + dx - 1, top - 6, 4, 3, INK);
        fill(draw, x + dx, top - 5, 2, 2, color);
    }
}

/// Brass teller bars across the bank counter, open in the middle for the teller.
fn teller_bars(draw: &mut impl RaylibDraw, x: i32, top: i32) {
    let bar_top = top - 22;
    fill(draw, x + 2, bar_top - 1, COUNTER_WIDTH - 4, 3, INK);
    fill(draw, x + 3, bar_top, COUNTER_WIDTH - 6, 1, GOLD);
    let window_left = x + COUNTER_WIDTH / 2 - 12;
    let window_right = x + COUNTER_WIDTH / 2 + 12;
    let mut bar_x = x + 5;
    while bar_x < x + COUNTER_WIDTH - 4 {
        if bar_x < window_left || bar_x > window_right {
            fill(draw, bar_x - 1, bar_top + 2, 3, 20, INK);
            fill(draw, bar_x, bar_top + 2, 1, 20, AMBER);
        }
        bar_x += 5;
    }
    // Arch over the window.
    fill(
        draw,
        window_left,
        bar_top + 2,
        window_right - window_left + 1,
        2,
        INK,
    );
    fill(
        draw,
        window_left + 1,
        bar_top + 2,
        window_right - window_left - 1,
        1,
        GOLD,
    );
}

fn fuel_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    window(draw, 56, 32, 40, 30, ctx.seconds);
    poster(draw, 108, 34, 22, 28, ORANGE);
    // Two tall pumps with blinking displays and coiled hoses.
    for (index, x) in [104, 134].into_iter().enumerate() {
        let y = FLOOR_Y - 34;
        bevel(draw, x, y, 20, 34, ORANGE, RED, MAROON);
        fill(draw, x + 2, y - 4, 16, 5, INK);
        fill(draw, x + 3, y - 3, 14, 3, CREAM);
        fill(draw, x + 6, y - 2, 8, 1, RED);
        fill(draw, x + 4, y + 4, 12, 8, INK);
        fill(draw, x + 5, y + 5, 10, 6, NIGHT);
        let lit = ((ctx.seconds * 2.0) as i32 + i32::try_from(index).unwrap_or(0)) % 3;
        for digit in 0..3 {
            let color = if digit == lit { LIME } else { MOSS };
            fill(draw, x + 6 + digit * 3, y + 7, 2, 3, color);
        }
        fill(draw, x + 4, y + 15, 12, 1, MAROON);
        fill(draw, x + 4, y + 18, 5, 4, INK);
        fill(draw, x + 5, y + 19, 3, 2, STEEL_LIGHT);
        // Hose looping down from the nozzle holster.
        fill(draw, x + 19, y + 12, 3, 3, INK);
        fill(draw, x + 20, y + 13, 1, 1, STEEL_LIGHT);
        fill(draw, x + 21, y + 15, 2, 14, INK);
        fill(draw, x + 18, y + 28, 4, 2, INK);
        floor_shadow(draw, x, 22);
    }
    // Oil-can pyramid by the window.
    let can = &art::CAN;
    for (row, count) in [(0, 3), (1, 2), (2, 1)] {
        for index in 0..count {
            let x = 58 + row * 3 + index * 6;
            let y = FLOOR_Y - can.height() * (row + 1);
            ui_icon(draw, can, x, y);
        }
    }
    // Barrels and a tyre stack right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 10;
    ui_icon(draw, &art::BARREL, right, FLOOR_Y - art::BARREL.height());
    ui_icon(
        draw,
        &art::BARREL,
        right + 11,
        FLOOR_Y - art::BARREL.height(),
    );
    ui_icon(
        draw,
        &art::BARREL,
        right + 5,
        FLOOR_Y - art::BARREL.height() * 2,
    );
    tyres(draw, right + 26, 3);
    // An oil stain on the concrete.
    fill(draw, 112, FLOOR_Y + 6, 18, 2, palette::with_alpha(INK, 90));
    fill(draw, 116, FLOOR_Y + 8, 8, 1, palette::with_alpha(INK, 70));
}

fn repair_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    // Chain hoist with an engine block hanging over the lift.
    let hoist_x = 98;
    fill(draw, hoist_x - 10, CEILING_Y, 22, 4, INK);
    fill(draw, hoist_x - 9, CEILING_Y + 1, 20, 2, STEEL_DARK);
    let sway = ((ctx.seconds * 1.5).sin() * 1.0).round() as i32;
    for link in 0..9 {
        let y = CEILING_Y + 4 + link * 4;
        fill(draw, hoist_x + sway, y, 2, 3, STEEL);
        fill(draw, hoist_x + sway, y + 3, 2, 1, INK);
    }
    let block_y = CEILING_Y + 40;
    bevel(
        draw,
        hoist_x - 10 + sway,
        block_y,
        22,
        16,
        STEEL_LIGHT,
        STEEL,
        STEEL_DARK,
    );
    for cylinder in 0..3 {
        fill(
            draw,
            hoist_x - 7 + sway + cylinder * 6,
            block_y + 4,
            4,
            6,
            STEEL_DARK,
        );
    }
    // Hydraulic lift.
    fill(draw, 60, FLOOR_Y - 6, 84, 6, INK);
    fill(draw, 61, FLOOR_Y - 5, 82, 4, RUST);
    fill(draw, 61, FLOOR_Y - 5, 82, 1, COPPER);
    for post in [70, 130] {
        bevel(
            draw,
            post,
            FLOOR_Y - 38,
            6,
            32,
            STEEL_LIGHT,
            STEEL,
            STEEL_DARK,
        );
    }
    fill(draw, 64, FLOOR_Y - 40, 76, 4, INK);
    fill(draw, 65, FLOOR_Y - 39, 74, 2, AMBER);
    for stripe in (66..138).step_by(8) {
        fill(draw, stripe, FLOOR_Y - 39, 3, 2, INK);
    }
    // Pegboard with tools and a safety poster.
    // Between the hoist and the hanging sign so the sign never covers it.
    pegboard(draw, 124, 34);
    poster(draw, 56, 36, 20, 26, LEAF);
    // Red tool chest and tyres right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 10;
    bevel(draw, right, FLOOR_Y - 30, 26, 30, ORANGE, RED, MAROON);
    for drawer in 0..4 {
        let y = FLOOR_Y - 27 + drawer * 7;
        fill(draw, right + 3, y + 5, 20, 1, MAROON);
        fill(draw, right + 10, y + 2, 6, 1, STEEL_LIGHT);
    }
    floor_shadow(draw, right, 26);
    tyres(draw, right + 30, 4);
}

/// A dark pegboard hung with wrenches and hammers.
fn pegboard(draw: &mut impl RaylibDraw, x: i32, y: i32) {
    bevel(draw, x, y, 30, 30, SOIL, SOIL_DARK, INK);
    for py in (y + 4..y + 28).step_by(4) {
        for px in (x + 4..x + 28).step_by(4) {
            fill(draw, px, py, 1, 1, INK);
        }
    }
    for (index, (dx, dy)) in [(5, 5), (13, 6), (21, 5), (9, 17), (19, 17)]
        .into_iter()
        .enumerate()
    {
        let (tx, ty) = (x + dx, y + dy);
        if index % 2 == 0 {
            fill(draw, tx + 1, ty, 3, 9, STEEL_LIGHT);
            fill(draw, tx, ty, 5, 3, STEEL);
            fill(draw, tx + 2, ty, 1, 2, INK);
        } else {
            fill(draw, tx + 2, ty + 2, 2, 8, LOAM);
            fill(draw, tx, ty, 6, 3, STEEL_DARK);
        }
    }
}

/// A mineral's chalk colour on the depot price board.
const fn chalk_color(mineral: MineralKind) -> Color {
    match mineral {
        MineralKind::Copper => COPPER,
        MineralKind::Iron => MIST,
        MineralKind::Silver => ICE,
        _ => GOLD,
    }
}

/// The depot's chalk board with today's price and trend for the common ores.
fn price_board(draw: &mut impl RaylibDraw, market: &[MarketQuote]) {
    // Chalk price board with today's market.
    let (board_x, board_y, board_w, board_h) = (54, 30, 78, 50);
    bevel(
        draw,
        board_x - 3,
        board_y - 3,
        board_w + 6,
        board_h + 6,
        SAND,
        LOAM,
        SOIL,
    );
    fill(draw, board_x, board_y, board_w, board_h, PINE);
    fill(draw, board_x, board_y, board_w, 1, MOSS);
    for (row, mineral) in [
        MineralKind::Copper,
        MineralKind::Iron,
        MineralKind::Silver,
        MineralKind::Gold,
    ]
    .into_iter()
    .enumerate()
    {
        let y = board_y + 3 + i32::try_from(row).unwrap_or(0) * 11;
        let quote = market.iter().find(|quote| quote.mineral == mineral);
        ui_icon(draw, icons::mineral(mineral), board_x + 3, y);
        let value = quote.map_or_else(|| mineral.value(), |quote| quote.value);
        let trend = quote.map_or(0, |quote| {
            i64::from(quote.factor) - i64::from(quote.previous_factor.unwrap_or(quote.factor))
        });
        let label = format!("{value}");
        ui_kit::text_shadowed(
            draw,
            &label,
            board_x + 15,
            y + 1,
            TextStyle::Body,
            chalk_color(mineral),
            None,
        );
        let (arrow, color) = match trend.signum() {
            1 => ("\u{2191}", LIME),
            -1 => ("\u{2193}", ORANGE),
            _ => ("\u{2192}", MIST),
        };
        ui_kit::text_shadowed(
            draw,
            arrow,
            board_x + board_w - 9,
            y + 1,
            TextStyle::Body,
            color,
            None,
        );
    }
    // Chalk dust on the ledge.
    fill(
        draw,
        board_x - 2,
        board_y + board_h + 3,
        board_w + 4,
        2,
        INK,
    );
    fill(draw, board_x + 6, board_y + board_h + 2, 5, 1, BONE);
}

fn depot_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    price_board(draw, ctx.market);
    // Mine cart on rails, heaped with ore.
    fill(draw, 46, FLOOR_Y - 2, 130, 1, STEEL_DARK);
    fill(draw, 46, FLOOR_Y - 1, 130, 1, INK);
    for sleeper in (48..176).step_by(10) {
        fill(draw, sleeper, FLOOR_Y - 1, 5, 2, SOIL);
    }
    let cart_x = 124;
    let cart_y = FLOOR_Y - 20;
    for (dx, color) in [
        (4, COPPER),
        (9, STEEL_LIGHT),
        (14, GOLD),
        (19, COPPER),
        (24, ICE),
        (8, LEAF),
        (17, STEEL_LIGHT),
    ] {
        let dy = if dx % 2 == 0 { -5 } else { -3 };
        fill(draw, cart_x + dx - 1, cart_y + dy - 1, 6, 5, INK);
        fill(draw, cart_x + dx, cart_y + dy, 4, 3, color);
        fill(draw, cart_x + dx, cart_y + dy, 1, 1, BONE);
    }
    fill(draw, cart_x, cart_y, 34, 14, INK);
    fill(draw, cart_x + 1, cart_y + 1, 32, 12, STEEL);
    fill(draw, cart_x + 1, cart_y + 1, 32, 1, STEEL_LIGHT);
    fill(draw, cart_x + 1, cart_y + 6, 32, 1, STEEL_DARK);
    for rivet in (cart_x + 4..cart_x + 32).step_by(7) {
        fill(draw, rivet, cart_y + 3, 1, 1, STEEL_DARK);
        fill(draw, rivet, cart_y + 9, 1, 1, STEEL_DARK);
    }
    for wheel in [cart_x + 4, cart_x + 24] {
        disc(draw, wheel + 3, FLOOR_Y - 4, 3, INK);
        fill(draw, wheel + 2, FLOOR_Y - 5, 2, 2, STEEL_LIGHT);
    }
    // Ore sacks by the cart.
    for (index, x) in [56, 70, 63].into_iter().enumerate() {
        let y = if index == 2 {
            FLOOR_Y - 24
        } else {
            FLOOR_Y - 13
        };
        sack(draw, x, y);
    }
    // Crates and a barrel right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 10;
    let crate_sprite = &art::CRATE;
    ui_icon(draw, crate_sprite, right, FLOOR_Y - crate_sprite.height());
    ui_icon(
        draw,
        crate_sprite,
        right + 11,
        FLOOR_Y - crate_sprite.height(),
    );
    ui_icon(
        draw,
        crate_sprite,
        right + 5,
        FLOOR_Y - crate_sprite.height() * 2 + 1,
    );
    ui_icon(
        draw,
        &art::BARREL,
        right + 26,
        FLOOR_Y - art::BARREL.height(),
    );
}

/// A tied burlap sack, 13x13, top-left at `(x, y)`.
fn sack(draw: &mut impl RaylibDraw, x: i32, y: i32) {
    fill(draw, x + 1, y + 3, 11, 10, INK);
    fill(draw, x, y + 5, 13, 7, INK);
    fill(draw, x + 2, y + 4, 9, 8, SAND);
    fill(draw, x + 1, y + 6, 11, 5, SAND);
    fill(draw, x + 8, y + 6, 3, 5, CLAY);
    fill(draw, x + 2, y + 11, 9, 1, CLAY);
    fill(draw, x + 4, y, 5, 4, INK);
    fill(draw, x + 5, y + 1, 3, 2, CLAY);
    fill(draw, x + 4, y + 7, 4, 1, LOAM);
}

fn hq_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    // Survey screen: a cross-section of the claim with a blinking rig marker.
    let (x, y, w, h) = (54, 30, 80, 48);
    bevel(
        draw,
        x - 3,
        y - 3,
        w + 6,
        h + 6,
        STEEL_LIGHT,
        STEEL,
        STEEL_DARK,
    );
    fill(draw, x, y, w, h, NIGHT);
    let bands = [
        (0, 10, DEEP_BLUE),
        (10, 6, LOAM),
        (16, 8, SOIL),
        (24, 8, STONE),
        (32, 8, STONE_DARK),
        (40, 8, MAROON),
    ];
    for (dy, bh, color) in bands {
        fill(draw, x, y + dy, w, bh, color);
    }
    // Scanlines and the town on the surface.
    for line in (y..y + h).step_by(2) {
        fill(draw, x, line, w, 1, palette::with_alpha(INK, 60));
    }
    for (dx, bw, bh) in [(8, 6, 4), (18, 8, 5), (30, 5, 3), (44, 9, 4), (60, 6, 5)] {
        fill(draw, x + dx, y + 10 - bh, bw, bh, SKY);
    }
    // A shaft with tunnels.
    fill(draw, x + 40, y + 10, 2, 28, INK);
    fill(draw, x + 28, y + 24, 14, 2, INK);
    fill(draw, x + 42, y + 32, 18, 2, INK);
    for (dx, dy, color) in [
        (16, 20, GOLD),
        (62, 28, ICE),
        (24, 36, LEAF),
        (66, 42, ROSE),
    ] {
        fill(draw, x + dx, y + dy, 2, 2, color);
    }
    if (ctx.seconds * 2.0) as i32 % 2 == 0 {
        fill(draw, x + 39, y + 36, 4, 3, AMBER);
    }
    // Filing cabinet and a plant.
    let cabinet_x = 56;
    bevel(
        draw,
        cabinet_x,
        FLOOR_Y - 32,
        18,
        32,
        STEEL_LIGHT,
        STEEL,
        STEEL_DARK,
    );
    for drawer in 0..3 {
        let dy = FLOOR_Y - 30 + drawer * 10;
        fill(draw, cabinet_x + 2, dy + 8, 14, 1, STEEL_DARK);
        fill(draw, cabinet_x + 6, dy + 3, 6, 2, INK);
        fill(draw, cabinet_x + 7, dy + 3, 4, 1, MIST);
    }
    floor_shadow(draw, cabinet_x, 18);
    plant(draw, 132);
    // A flag on a stand.
    fill(draw, 108, FLOOR_Y - 44, 2, 44, INK);
    fill(draw, 108, FLOOR_Y - 44, 1, 44, STEEL_LIGHT);
    fill(draw, 110, FLOOR_Y - 43, 16, 11, INK);
    fill(draw, 110, FLOOR_Y - 42, 15, 9, OCEAN);
    fill(draw, 110, FLOOR_Y - 42, 15, 2, SKY);
    fill(draw, 115, FLOOR_Y - 39, 5, 3, GOLD);
    fill(draw, 104, FLOOR_Y - 2, 10, 2, INK);
    // Bookshelf right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 12;
    bevel(draw, right, FLOOR_Y - 52, 36, 52, CLAY, LOAM, SOIL);
    let colors = [RED, OCEAN, LEAF, GOLD, VIOLET, TEAL, CLAY, ROSE];
    for shelf_row in 0..4 {
        let sy = FLOOR_Y - 50 + shelf_row * 12;
        fill(draw, right + 2, sy + 10, 32, 2, SOIL_DARK);
        let mut bx = right + 3;
        let mut index: i32 = shelf_row * 3;
        while bx < right + 32 {
            let width = 2 + index % 3;
            let height = 7 + index % 2 * 2;
            let color = colors[usize::try_from(index).unwrap_or(0) % colors.len()];
            fill(draw, bx, sy + 10 - height, width, height, color);
            fill(draw, bx, sy + 10 - height, width, 1, BONE);
            bx += width + 1;
            index += 1;
        }
    }
    floor_shadow(draw, right, 36);
    wall_clock(draw, right + 18, CEILING_Y + 22, ctx.seconds);
}

fn bank_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    // Round vault door set in a steel frame.
    let (cx, cy) = (88, FLOOR_Y - 30);
    bevel(
        draw,
        cx - 28,
        cy - 28,
        57,
        58,
        STEEL_LIGHT,
        STEEL,
        STEEL_DARK,
    );
    fill(draw, cx - 25, cy - 25, 51, 52, STEEL_DARK);
    disc(draw, cx, cy, 23, INK);
    disc(draw, cx, cy, 22, STEEL_LIGHT);
    disc(draw, cx, cy, 20, STEEL);
    disc(draw, cx + 1, cy + 1, 17, STEEL_DARK);
    disc(draw, cx, cy, 17, STEEL);
    // Bolts around the rim.
    for (dx, dy) in [
        (0, -19),
        (13, -13),
        (19, 0),
        (13, 13),
        (0, 19),
        (-13, 13),
        (-19, 0),
        (-13, -13),
    ] {
        fill(draw, cx + dx - 1, cy + dy - 1, 3, 3, INK);
        fill(draw, cx + dx, cy + dy, 1, 1, MIST);
    }
    // Handle wheel.
    disc(draw, cx, cy, 8, INK);
    disc(draw, cx, cy, 7, GOLD);
    disc(draw, cx, cy, 5, STEEL);
    fill(draw, cx - 9, cy, 19, 1, INK);
    fill(draw, cx, cy - 9, 1, 19, INK);
    disc(draw, cx, cy, 2, GOLD);
    fill(draw, cx - 1, cy - 1, 1, 1, CREAM);
    // Gold bars stacked by the vault.
    for (row, count) in [(0, 3), (1, 2), (2, 1)] {
        for index in 0..count {
            let x = 120 + row * 4 + index * 8;
            let y = FLOOR_Y - 5 * (row + 1);
            fill(draw, x, y, 9, 5, INK);
            fill(draw, x + 1, y + 1, 7, 3, GOLD);
            fill(draw, x + 1, y + 1, 7, 1, CREAM);
            fill(draw, x + 1, y + 3, 7, 1, AMBER);
        }
    }
    floor_shadow(draw, 119, 25);
    // Framed portrait of the founder over the gold.
    bevel(draw, 120, 36, 24, 30, CREAM, GOLD, AMBER);
    fill(draw, 123, 39, 18, 24, TEAL_DARK);
    disc(draw, 132, 47, 4, SAND);
    fill(draw, 126, 53, 12, 10, SLATE);
    fill(draw, 131, 53, 2, 4, BONE);
    fill(draw, 128, 42, 8, 2, SLATE_DARK);
    // Grandfather clock and a plant right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 12;
    bevel(draw, right, FLOOR_Y - 58, 18, 58, CLAY, LOAM, SOIL);
    disc(draw, right + 9, FLOOR_Y - 48, 6, INK);
    disc(draw, right + 9, FLOOR_Y - 48, 5, CREAM);
    fill(draw, right + 9, FLOOR_Y - 52, 1, 4, INK);
    fill(draw, right + 9, FLOOR_Y - 48, 3, 1, INK);
    fill(draw, right + 4, FLOOR_Y - 38, 10, 30, INK);
    fill(draw, right + 5, FLOOR_Y - 37, 8, 28, SOIL_DARK);
    let swing = ((ctx.seconds * 3.0).sin() * 2.0).round() as i32;
    fill(draw, right + 9 + swing / 2, FLOOR_Y - 37, 1, 18, GOLD);
    disc(draw, right + 9 + swing, FLOOR_Y - 17, 2, GOLD);
    floor_shadow(draw, right, 18);
    plant(draw, right + 24);
    // Velvet rope between brass posts in front of the vault.
    for post in [64, 112] {
        fill(draw, post, FLOOR_Y - 18, 3, 18, INK);
        fill(draw, post + 1, FLOOR_Y - 17, 1, 17, GOLD);
        fill(draw, post - 1, FLOOR_Y - 20, 5, 3, INK);
        fill(draw, post, FLOOR_Y - 19, 3, 1, CREAM);
        fill(draw, post - 2, FLOOR_Y - 2, 7, 2, INK);
    }
    for step in 0..46 {
        let sag = (step * (46 - step)) / 120;
        fill(draw, 67 + step, FLOOR_Y - 16 + sag, 1, 2, MAROON);
        fill(draw, 67 + step, FLOOR_Y - 16 + sag, 1, 1, RED);
    }
}

fn explosives_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    // Danger sign with hazard stripes.
    let (sx, sy) = (56, 34);
    let sign_width = ui_kit::text_width("DANGER", TextStyle::Bold) + 10;
    fill(draw, sx, sy, sign_width, 22, INK);
    for stripe in 0..(sign_width - 2) / 2 {
        let color = if stripe % 2 == 0 { GOLD } else { INK };
        fill(draw, sx + 1 + stripe * 2, sy + 1, 2, 2, color);
        fill(draw, sx + 1 + stripe * 2, sy + 19, 2, 2, color);
    }
    fill(draw, sx + 1, sy + 4, sign_width - 2, 14, CREAM);
    ui_kit::text_shadowed(draw, "DANGER", sx + 5, sy + 7, TextStyle::Bold, RED, None);
    // TNT crates.
    let label_width = ui_kit::text_width("TNT", TextStyle::Body);
    let crate_width = label_width + 8;
    for (index, (x, y)) in [
        (54, FLOOR_Y - 15),
        (55 + crate_width, FLOOR_Y - 15),
        (54 + crate_width / 2, FLOOR_Y - 30),
    ]
    .into_iter()
    .enumerate()
    {
        bevel(draw, x, y, crate_width, 15, SAND, CLAY, LOAM);
        fill(draw, x + 2, y + 3, crate_width - 4, 9, RED);
        fill(draw, x + 2, y + 3, crate_width - 4, 1, ORANGE);
        ui_kit::text_shadowed(draw, "TNT", x + 4, y + 4, TextStyle::Body, CREAM, None);
        if index == 2 {
            fill(draw, x + crate_width / 2, y - 3, 1, 3, SAND);
            fill(draw, x + crate_width / 2, y - 4, 1, 1, GOLD);
        }
    }
    floor_shadow(draw, 54, crate_width * 2 + 1);
    // Red drums with flame markings, set just past the crate stack (whose width follows the
    // font), and the sandbags beyond them.
    let drums_x = (57 + crate_width * 2).max(100);
    let shift = drums_x - 100;
    for x in [drums_x, drums_x + 14] {
        let y = FLOOR_Y - 18;
        bevel(draw, x, y, 13, 18, ORANGE, RED, MAROON);
        fill(draw, x + 1, y + 5, 11, 1, MAROON);
        fill(draw, x + 1, y + 12, 11, 1, MAROON);
        fill(draw, x + 5, y + 7, 3, 4, GOLD);
        fill(draw, x + 6, y + 6, 1, 1, GOLD);
        floor_shadow(draw, x, 13);
    }
    // Sandbags.
    for (dx, dy) in [(132, 0), (146, 0), (139, -6)] {
        let (x, y) = (dx + shift, FLOOR_Y - 7 + dy);
        fill(draw, x, y, 15, 7, INK);
        fill(draw, x + 1, y + 1, 13, 5, SAND);
        fill(draw, x + 1, y + 5, 13, 1, CLAY);
        fill(draw, x + 7, y + 1, 1, 5, CLAY);
    }
    // Fuse spools and a crate right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 10;
    for (index, dx) in [0, 13].into_iter().enumerate() {
        let x = right + dx;
        disc(draw, x + 5, FLOOR_Y - 6, 5, INK);
        disc(draw, x + 5, FLOOR_Y - 6, 4, LOAM);
        disc(
            draw,
            x + 5,
            FLOOR_Y - 6,
            2,
            if index == 0 { RED } else { SAND },
        );
    }
    ui_icon(draw, &art::CRATE, right + 28, FLOOR_Y - art::CRATE.height());
    poster(draw, right + 4, 40, 20, 24, RED);
}

fn salvage_props(draw: &mut impl RaylibDraw, ctx: &RoomContext<'_>) {
    // Crane magnet hanging from a ceiling rail.
    let magnet_x = 120;
    fill(draw, 46, CEILING_Y, 120, 3, INK);
    fill(draw, 46, CEILING_Y + 1, 120, 1, STEEL);
    fill(draw, magnet_x, CEILING_Y + 3, 1, 26, INK);
    bevel(
        draw,
        magnet_x - 9,
        CEILING_Y + 28,
        19,
        8,
        ORANGE,
        RED,
        MAROON,
    );
    fill(draw, magnet_x - 9, CEILING_Y + 35, 19, 2, INK);
    fill(draw, magnet_x - 8, CEILING_Y + 35, 17, 1, STEEL_LIGHT);
    // Scrap pile: plates, a drum and gears.
    for (dx, dy, w, h, light, body, shade) in [
        (0, 0, 18, 12, COPPER, RUST, MAROON),
        (14, 3, 20, 14, STEEL_LIGHT, STEEL, STEEL_DARK),
        (30, 0, 16, 11, SAND, CLAY, LOAM),
        (6, 10, 16, 12, STEEL, STEEL_DARK, SLATE),
        (20, 13, 18, 10, COPPER, RUST, MAROON),
        (12, 20, 12, 8, STEEL_LIGHT, STEEL, STEEL_DARK),
    ] {
        bevel(draw, 54 + dx, FLOOR_Y - h - dy, w, h, light, body, shade);
    }
    ui_icon(draw, &art::GEAR, 62, FLOOR_Y - 34);
    ui_icon(draw, &art::GEAR, 80, FLOOR_Y - 31);
    floor_shadow(draw, 54, 46);
    // A wrecked rig up on blocks.
    let wreck = &rig::RIG_DAMAGED;
    let wreck_x = 106;
    for block in [wreck_x + 1, wreck_x + wreck.width() - 6] {
        bevel(
            draw,
            block,
            FLOOR_Y - 5,
            6,
            5,
            STONE_LIGHT,
            STONE,
            STONE_DARK,
        );
    }
    room_sprite(draw, wreck, 0, wreck_x, FLOOR_Y - 5 - wreck.height(), true);
    // Tyres, a barrel and gears right of the counter.
    let right = ctx.counter_x + COUNTER_WIDTH + 10;
    tyres(draw, right, 3);
    ui_icon(
        draw,
        &art::BARREL,
        right + 22,
        FLOOR_Y - art::BARREL.height(),
    );
    ui_icon(draw, &art::GEAR, right + 6, FLOOR_Y - 21 - 7);
    poster(draw, right + 8, 40, 22, 26, LIME);
}
