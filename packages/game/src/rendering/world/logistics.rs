//! Depth logistics in the world (Part III F10): lift rails, stations and cabs, deep outposts
//! and depth beacons, drawn over the terrain from the replicated [`Logistics`] section, plus
//! their lamps after the darkness pass.
//!
//! One tile is 16 art pixels; everything is art-pixel rectangles animated from the shell clock,
//! except a moving cab, which follows its ride's ticks (interpolated by the snapshot's alpha) so
//! it stays glued to its rider.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "art pixels are small integers drawn at float scales"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    SimulationTick,
    logistics::{DepthBeacon, LiftShaft, Logistics, OUTPOST_HALF_WIDTH, OUTPOST_HEIGHT, Outpost},
    terrain::TILE_SIZE,
};

use crate::render_snapshot::PlayerPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, Pen, fx::disc, player_accent, text_art_width};

/// Art pixels per tile.
const T: i32 = 16;

/// What the logistics layer needs from the view.
pub(super) struct LogisticsScene<'a> {
    pub logistics: &'a Logistics,
    pub visible: Rectangle,
    pub world_tick: u64,
    pub alpha: f32,
    pub seconds: f32,
    pub players: &'a [PlayerPresentation],
}

impl LogisticsScene<'_> {
    /// The visible tile rows and columns, with a margin.
    fn tile_bounds(&self) -> (i32, i32, i32, i32) {
        // `visible` is in the world's local pixels (F7): tiles go through the frame.
        let frame = crate::view_frame::frame();
        let min_x = frame.col(self.visible.x) - 2;
        let max_x = frame.col_ceil(self.visible.x + self.visible.width) + 2;
        let min_y = frame.row(self.visible.y) - 2;
        let max_y = frame.row_ceil(self.visible.y + self.visible.height) + 2;
        (min_x, max_x, min_y, max_y)
    }

    /// The row lift `lift`'s cab is drawn at, relative to the frame's origin row (F7): its
    /// ride's (interpolated) or where it waits.
    fn cab_row(&self, lift: &LiftShaft) -> f32 {
        let origin = crate::view_frame::frame().origin_row() as f32;
        let row = self
            .logistics
            .rides
            .values()
            .find(|ride| ride.shaft == lift.id)
            .map_or(lift.cab_row as f32, |ride| {
                let now = ride.row_at(SimulationTick::new(self.world_tick)) as f32;
                let next = ride.row_at(SimulationTick::new(self.world_tick + 1)) as f32;
                (next - now).mul_add(self.alpha, now)
            });
        row - origin
    }

    fn accent_of(&self, player: fatherload_simulation::PlayerId) -> Color {
        let cosmetics = self
            .players
            .iter()
            .find(|presentation| presentation.player_id == player)
            .map(|presentation| presentation.cosmetics);
        player_accent(player, cosmetics)
    }
}

/// Draws every visible lift, outpost and beacon (before the rigs, so a rider sits in its cab).
/// The local pixel `x` of absolute column `column`'s left edge (the view's frame, F7).
fn col_px(column: i32) -> f32 {
    crate::view_frame::frame().rel_col(column) * TILE_SIZE
}

/// The local pixel `y` of absolute row `row`'s top edge.
fn row_px(row: i32) -> f32 {
    crate::view_frame::frame().rel_row(row) * TILE_SIZE
}

pub(super) fn draw_logistics(draw: &mut impl RaylibDraw, scene: &LogisticsScene<'_>) {
    let (min_x, max_x, min_y, max_y) = scene.tile_bounds();
    for outpost in scene.logistics.outposts.values() {
        if outpost.column + OUTPOST_HALF_WIDTH + 2 < min_x
            || outpost.column - OUTPOST_HALF_WIDTH - 2 > max_x
            || outpost.floor_row < min_y
            || outpost.floor_row - OUTPOST_HEIGHT - 2 > max_y
        {
            continue;
        }
        draw_outpost(draw, scene, outpost);
    }
    for lift in scene.logistics.lifts.values() {
        if lift.column < min_x || lift.column > max_x {
            continue;
        }
        draw_lift(draw, scene, lift, (min_y, max_y));
    }
    for beacon in scene.logistics.beacons.values() {
        if (min_x..=max_x).contains(&beacon.at.x) && (min_y..=max_y).contains(&beacon.at.y) {
            draw_beacon(draw, scene, beacon);
        }
    }
}

fn draw_lift(
    draw: &mut impl RaylibDraw,
    scene: &LogisticsScene<'_>,
    lift: &LiftShaft,
    rows: (i32, i32),
) {
    let (Some(&top), Some(&bottom)) = (
        lift.stations.iter().next(),
        lift.stations.iter().next_back(),
    ) else {
        return;
    };
    let live = lift.restored;
    let rail = if lift.is_ancient() {
        if live { TEAL } else { STEEL_DARK }
    } else {
        STEEL_LIGHT
    };
    let rail_shadow = if lift.is_ancient() {
        SLATE_DARK
    } else {
        STEEL_DARK
    };
    let first = top.max(rows.0);
    let last = (bottom + 1).min(rows.1);
    let x0 = col_px(lift.column);
    // The rails run the whole shaft, with a sleeper every tile and, on an ancient lift, the
    // builders' glyphs every few rows (lit once it runs).
    for row in first..=last {
        let pen = Pen::at(x0, row_px(row));
        pen.rect(draw, 1, 0, 2, T, rail_shadow);
        pen.rect(draw, 1, 0, 1, T, rail);
        pen.rect(draw, T - 3, 0, 2, T, rail_shadow);
        pen.rect(draw, T - 3, 0, 1, T, rail);
        pen.rect(draw, 1, T - 2, 3, 1, rail_shadow);
        pen.rect(draw, T - 4, T - 2, 3, 1, rail_shadow);
        if lift.is_ancient() && row.rem_euclid(6) == 0 {
            let glyph = if live { MINT } else { SLATE };
            pen.rect(draw, 6, 5, 4, 1, glyph);
            pen.rect(draw, 7, 4, 2, 4, glyph);
            pen.rect(draw, 6, 8, 4, 1, glyph);
        }
    }
    // Stations: a hazard-striped landing on the floor, a call lamp and the sign.
    for &station in &lift.stations {
        if station < rows.0 - 1 || station > rows.1 + 1 {
            continue;
        }
        let pen = Pen::at(x0, row_px(station));
        for stripe in 0..6 {
            let color = if stripe % 2 == 0 { GOLD } else { INK };
            pen.rect(draw, -T + stripe * 8, T - 1, 8, 2, color);
            pen.rect(draw, stripe * 8, T - 1, 8, 2, color);
        }
        let lamp = if live {
            if (scene.seconds * 2.0).sin() > -0.6 {
                LIME
            } else {
                MOSS
            }
        } else {
            RED
        };
        pen.boxed(draw, -6, 2, 5, 7, STEEL_DARK);
        pen.rect(draw, -5, 3, 3, 2, lamp);
        pen.rect(draw, -5, 6, 3, 2, if live { LIME } else { MAROON });
        let label = if lift.is_ancient() {
            "ANCIENT LIFT"
        } else {
            "LIFT"
        };
        let width = text_art_width(label);
        pen.text(
            draw,
            label,
            T / 2 - width / 2,
            -9,
            if live { CREAM } else { FOG },
        );
    }
    // The cable from the top station to the cab, then the cab: a cage around the rider.
    let cab = scene.cab_row(lift);
    let cab_y = cab * TILE_SIZE;
    let cable_top = row_px(top).max(scene.visible.y - TILE_SIZE);
    if cab_y > cable_top {
        draw.draw_rectangle_rec(
            Rectangle::new(
                x0 + TILE_SIZE * 0.5 - ART * 0.5,
                cable_top,
                ART,
                cab_y - cable_top,
            ),
            with_alpha(STEEL_LIGHT, 200),
        );
    }
    let pen = Pen::at(x0, cab_y);
    let frame = if lift.is_ancient() { TEAL_DARK } else { RUST };
    let trim = if lift.is_ancient() { MINT } else { AMBER };
    pen.rect(draw, -1, -2, T + 2, 3, INK);
    pen.rect(draw, 0, -1, T, 1, frame);
    pen.rect(draw, 0, -2, T, 1, trim);
    pen.rect(draw, -1, 0, 2, T, INK);
    pen.rect(draw, T - 1, 0, 2, T, INK);
    pen.rect(draw, 0, 0, 1, T, frame);
    pen.rect(draw, T - 1, 0, 1, T, frame);
    pen.rect(draw, -1, T - 1, T + 2, 2, INK);
    pen.rect(draw, 0, T - 1, T, 1, trim);
    pen.rect(draw, T / 2 - 1, -4, 2, 2, STEEL_LIGHT);
}

fn draw_outpost(draw: &mut impl RaylibDraw, scene: &LogisticsScene<'_>, outpost: &Outpost) {
    let left = col_px(outpost.column - OUTPOST_HALF_WIDTH);
    let top = row_px(outpost.floor_row - OUTPOST_HEIGHT);
    let width = (OUTPOST_HALF_WIDTH * 2 + 1) * T;
    let height = OUTPOST_HEIGHT * T;
    let pen = Pen::at(left, top);
    // Back wall: riveted panels, a darker band at the bottom.
    pen.rect(draw, 0, 0, width, height, with_alpha(SLATE_DARK, 235));
    for panel in 0..(width / 24) {
        let x = panel * 24;
        pen.rect(draw, x, 0, 1, height, INK);
        pen.px(draw, x + 3, 3, SLATE_LIGHT);
        pen.px(draw, x + 20, 3, SLATE_LIGHT);
        pen.px(draw, x + 3, height - 14, SLATE_LIGHT);
        pen.px(draw, x + 20, height - 14, SLATE_LIGHT);
    }
    pen.rect(draw, 0, height - 10, width, 10, with_alpha(NIGHT, 220));
    pen.rect(draw, 0, 0, width, 2, STEEL);
    // The sign over the counter, with the crew's flag once claimed.
    let name = outpost.name.to_uppercase();
    let text_width = text_art_width(&name);
    let sign_x = width / 2 - text_width / 2 - 4;
    pen.boxed(draw, sign_x, 5, text_width + 8, 13, NIGHT);
    pen.rect(draw, sign_x + 1, 6, text_width + 6, 1, AMBER);
    pen.text(
        draw,
        &name,
        sign_x + 4,
        8,
        if outpost.claimed() { MINT } else { GOLD },
    );
    if let Some(owner) = outpost.claimed_by {
        let accent = scene.accent_of(owner);
        let flag_x = sign_x + text_width + 12;
        pen.rect(draw, flag_x, 4, 1, 18, STEEL_LIGHT);
        let wave = ((scene.seconds * 3.0).sin() * 1.5) as i32;
        pen.rect(draw, flag_x + 1, 4 + wave.max(0), 9, 6, accent);
        pen.rect(
            draw,
            flag_x + 1,
            4 + wave.max(0),
            9,
            1,
            with_alpha(CREAM, 120),
        );
    }
    // The counter and its terminal on the left.
    let floor = height;
    pen.boxed(draw, 10, floor - 18, 30, 18, RUST);
    pen.rect(draw, 11, floor - 17, 28, 2, AMBER);
    pen.boxed(draw, 18, floor - 30, 14, 12, INK);
    let blink = (scene.seconds * 1.5).fract() < 0.7;
    pen.rect(
        draw,
        20,
        floor - 28,
        10,
        7,
        if blink { TEAL } else { TEAL_DARK },
    );
    pen.rect(draw, 21, floor - 26, 6, 1, MINT);
    pen.rect(draw, 21, floor - 24, 4, 1, MINT);
    // A fuel pump on the right.
    let pump_x = width - 30;
    pen.boxed(draw, pump_x, floor - 26, 14, 26, MAROON);
    pen.rect(draw, pump_x + 2, floor - 23, 10, 6, CREAM);
    pen.rect(draw, pump_x + 3, floor - 21, 5, 2, ORANGE);
    pen.rect(draw, pump_x + 14, floor - 20, 3, 2, INK);
    pen.rect(draw, pump_x + 16, floor - 20, 2, 12, INK);
    // Crates in the corner, and the lamps.
    pen.boxed(draw, width / 2 + 12, floor - 10, 10, 10, SOIL);
    pen.rect(draw, width / 2 + 13, floor - 6, 8, 1, SOIL_DARK);
    pen.boxed(draw, width / 2 + 20, floor - 8, 8, 8, LOAM);
    for lamp_x in [width / 4, width * 3 / 4] {
        pen.rect(draw, lamp_x - 1, 2, 2, 4, STEEL);
        pen.rect(draw, lamp_x - 3, 6, 6, 2, GOLD);
        pen.rect(draw, lamp_x - 2, 8, 4, 1, CREAM);
    }
}

fn draw_beacon(draw: &mut impl RaylibDraw, scene: &LogisticsScene<'_>, beacon: &DepthBeacon) {
    let pen = Pen::at(col_px(beacon.at.x), row_px(beacon.at.y));
    let accent = scene.accent_of(beacon.owner);
    // A tripod and mast with the owner's band, and a light that pulses.
    pen.rect(draw, 3, T - 2, 10, 2, INK);
    pen.rect(draw, 4, T - 4, 2, 2, STEEL_DARK);
    pen.rect(draw, 10, T - 4, 2, 2, STEEL_DARK);
    pen.rect(draw, 7, 2, 2, T - 3, STEEL_LIGHT);
    pen.rect(draw, 6, 7, 4, 2, accent);
    let on = (scene.seconds * 2.0).fract() < 0.5;
    pen.boxed(draw, 5, -1, 6, 4, if on { SKY_LIGHT } else { TEAL_DARK });
}

/// The lamps of outposts, beacons and running lifts' stations (after the darkness pass, so
/// they read as light sources).
pub(super) fn draw_logistics_lights(draw: &mut impl RaylibDraw, scene: &LogisticsScene<'_>) {
    let (min_x, max_x, min_y, max_y) = scene.tile_bounds();
    let flicker = 1.0 + (scene.seconds * 9.0).sin() * 0.04;
    for outpost in scene.logistics.outposts.values() {
        if outpost.column + OUTPOST_HALF_WIDTH + 4 < min_x
            || outpost.column - OUTPOST_HALF_WIDTH - 4 > max_x
            || outpost.floor_row + 2 < min_y
            || outpost.floor_row - OUTPOST_HEIGHT - 4 > max_y
        {
            continue;
        }
        let cx = col_px(outpost.column) + 0.5 * TILE_SIZE;
        let cy = row_px(outpost.floor_row - OUTPOST_HEIGHT) + 8.0 * ART;
        for offset in [-2.25_f32, 2.25] {
            let x = offset.mul_add(TILE_SIZE, cx);
            disc(draw, x, cy, (40.0 * flicker) as i32, with_alpha(AMBER, 12));
            disc(draw, x, cy, (20.0 * flicker) as i32, with_alpha(GOLD, 22));
        }
    }
    for beacon in scene.logistics.beacons.values() {
        if !(min_x..=max_x).contains(&beacon.at.x) || !(min_y..=max_y).contains(&beacon.at.y) {
            continue;
        }
        let pulse = ((scene.seconds * 2.0).sin() * 0.5 + 0.5) * 10.0;
        disc(
            draw,
            col_px(beacon.at.x) + 0.5 * TILE_SIZE,
            row_px(beacon.at.y) + ART,
            14 + pulse as i32,
            with_alpha(SKY_LIGHT, 22),
        );
    }
    for lift in scene.logistics.lifts.values() {
        if !lift.restored || lift.column < min_x || lift.column > max_x {
            continue;
        }
        for &station in &lift.stations {
            if (min_y..=max_y).contains(&station) {
                disc(
                    draw,
                    col_px(lift.column) - 4.0 * ART,
                    row_px(station) + 5.0 * ART,
                    10,
                    with_alpha(LIME, 26),
                );
            }
        }
    }
}
