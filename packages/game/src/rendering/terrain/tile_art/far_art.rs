//! The far Strange Layers' tiles and back walls (Wave 9 strange-b, Descent VI strata 4-6).
//!
//! * Echo quartz: pale blue hexagonal crystal with white glints and a faint inner glow.
//! * The keyed walls: stained glass panes of one colour in dark lead, a highlight across
//!   each pane (red, green, blue).
//! * Builder alloy: dark steel plates on a grid, teal circuit seams and rivets.
//! * Chronoglass: amber glass with bubbles and a moment caught in it (a drop, a mote).
//! * Back walls: the Time Sludge's sepia murk hung with frozen drips (honey-gold inside a slow
//!   pool, streaked blue-white inside a fast lane, striped in the Stopped Clock), the Echo
//!   Caves' black with ripple rings only a ping would show, the Prism Desert's dark teal
//!   banded with spectrum sand.

use fatherload_simulation::{
    cave::CaveTile,
    descent::Stratum,
    far_strange::TimePocketKind,
    terrain::{Terrain, TilePosition},
    worldgen::descent::far_strange::{self as sites, FarStampKind},
};

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{Cell, Ramp, TILE_ART, TileCanvas, hash, noise, ramp};

/// The far stratum at `position` (`None` elsewhere or on older worlds).
#[must_use]
pub(super) fn far_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    sites::is_far(stratum).then_some(stratum)
}

/// What kind of open ground `position` is in the Time Sludge: a slow pool, a fast lane or a
/// clock hall stripe (`None`: the plain stratum).
fn time_ground(terrain: &Terrain, position: TilePosition) -> Option<TimePocketKind> {
    let settings = terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let stamp = sites::stamp_at(settings, &layout, position.x, position.y)?;
    match stamp.kind {
        FarStampKind::TimePocket(kind) if sites::pocket_open(&stamp, position.x, position.y) => {
            Some(kind)
        }
        FarStampKind::ClockHall => Some(sites::clock_stripe(&stamp.bounds, position.x)),
        _ => None,
    }
}

/// The material ramp of a far tile (edges and lips).
#[must_use]
pub const fn far_ramp(tile: CaveTile) -> Option<Ramp> {
    Some(match tile {
        CaveTile::EchoQuartz => ramp([DEEP_BLUE, SKY, SKY_LIGHT, ICE]),
        CaveTile::PrismRed => ramp([NIGHT, MAROON, RED, PINK]),
        CaveTile::PrismGreen => ramp([NIGHT, PINE, LIME, CREAM]),
        CaveTile::PrismBlue => ramp([NIGHT, DEEP_BLUE, SKY, ICE]),
        CaveTile::BuilderAlloy => ramp([INK, STEEL_DARK, STEEL, STEEL_LIGHT]),
        CaveTile::Chronoglass => ramp([RUST, AMBER, GOLD, CREAM]),
        _ => return None,
    })
}

/// One art pixel of far tile `tile` at absolute art pixel `(x, y)`, if it is one of them.
#[must_use]
pub(super) fn far_pixel(tile: CaveTile, x: i32, y: i32) -> Option<PaletteIndex> {
    Some(match tile {
        CaveTile::EchoQuartz => quartz_pixel(x, y),
        CaveTile::PrismRed => glass_pixel(x, y, [MAROON, RED, PINK]),
        CaveTile::PrismGreen => glass_pixel(x, y, [PINE, LIME, CREAM]),
        CaveTile::PrismBlue => glass_pixel(x, y, [DEEP_BLUE, SKY, ICE]),
        CaveTile::BuilderAlloy => alloy_pixel(x, y),
        CaveTile::Chronoglass => chronoglass_pixel(x, y),
        _ => return None,
    })
}

/// Echo quartz: hexagonal prisms in pale blue, white glints on their faces.
const fn quartz_pixel(x: i32, y: i32) -> PaletteIndex {
    let row = y.div_euclid(6);
    let shift = if row.rem_euclid(2) == 0 { 0 } else { 4 };
    let cx = (x + shift).rem_euclid(8);
    let cy = y.rem_euclid(6);
    let edge = cx == 0 || cy == 0 || (cy == 1 && (cx == 1 || cx == 7));
    if edge {
        return index_of(DEEP_BLUE);
    }
    if hash(x, y, 701).is_multiple_of(29) {
        return index_of(ICE);
    }
    // Lit on the left of each prism, shaded on the right.
    if cx <= 2 {
        index_of(SKY_LIGHT)
    } else if cx >= 6 {
        index_of(OCEAN)
    } else {
        index_of(SKY)
    }
}

/// Stained glass: panes in dark lead came, each pane lit across its upper left, a shimmer that
/// shifts across the glass.
fn glass_pixel(x: i32, y: i32, [dark, mid, light]: [Color; 3]) -> PaletteIndex {
    let pane_w = 8;
    let pane_h = 8;
    let row = y.div_euclid(pane_h);
    let shift = if row.rem_euclid(2) == 0 { 0 } else { 4 };
    let px = (x + shift).rem_euclid(pane_w);
    let py = y.rem_euclid(pane_h);
    if px == 0 || py == 0 {
        return index_of(INK);
    }
    // A diagonal highlight across every pane.
    if px + py == 4 || px + py == 5 {
        return index_of(light);
    }
    if px >= 6 || py >= 6 {
        return index_of(dark);
    }
    if noise(x, y, 6, 703) + f32::from(bayer4(x, y)) / 50.0 > 0.7 {
        index_of(light)
    } else {
        index_of(mid)
    }
}

/// Builder alloy: dark plates on a 16-pixel grid, teal circuit seams, rivets at the corners.
const fn alloy_pixel(x: i32, y: i32) -> PaletteIndex {
    let gx = x.rem_euclid(16);
    let gy = y.rem_euclid(16);
    if gx == 0 || gy == 0 {
        return index_of(INK);
    }
    if (gx == 2 || gx == 13) && (gy == 2 || gy == 13) {
        return index_of(STEEL_LIGHT);
    }
    // A circuit trace wanders across each plate.
    let plate = hash(x.div_euclid(16), y.div_euclid(16), 705);
    let trace_y = 4 + (plate % 8) as i32;
    let trace_x = 4 + ((plate >> 4) % 8) as i32;
    if (gy == trace_y && gx >= 3 && gx <= trace_x) || (gx == trace_x && gy >= trace_y && gy <= 12) {
        return index_of(if (plate >> 8).is_multiple_of(5) {
            MINT
        } else {
            TEAL
        });
    }
    if gy <= 1 || gx <= 1 {
        index_of(STEEL)
    } else if gy >= 14 || gx >= 14 {
        index_of(NIGHT)
    } else {
        index_of(STEEL_DARK)
    }
}

/// Chronoglass: amber glass, darker at depth, bubbles, now and then a frozen drop.
fn chronoglass_pixel(x: i32, y: i32) -> PaletteIndex {
    if hash(x, y, 707).is_multiple_of(53) {
        return index_of(CREAM);
    }
    let depth = noise(x, y, 7, 709) + f32::from(bayer4(x, y)) / 40.0;
    let drop = hash(x.div_euclid(10), y.div_euclid(10), 711);
    let (dx, dy) = (x.rem_euclid(10), y.rem_euclid(10));
    if drop.is_multiple_of(4) && (dx - 5).abs() + (dy - 5).abs() <= 1 {
        return index_of(if dy < 5 { ICE } else { SKY });
    }
    if depth > 0.75 {
        index_of(GOLD)
    } else if depth > 0.4 {
        index_of(AMBER)
    } else {
        index_of(RUST)
    }
}

/// One art pixel of `stratum`'s back wall at `(x, y)`, with the Time Sludge's pockets.
fn back_pixel(stratum: Stratum, time: Option<TimePocketKind>, x: i32, y: i32) -> PaletteIndex {
    match (stratum, time) {
        (Stratum::TimeSludge, Some(TimePocketKind::Slow)) => {
            // Honey: warm gold murk, motes hanging still in it.
            if hash(x, y, 721).is_multiple_of(97) {
                index_of(CREAM)
            } else if noise(x, y, 9, 723) + f32::from(bayer4(x, y)) / 40.0 > 0.6 {
                index_of(AMBER)
            } else {
                index_of(RUST)
            }
        }
        (Stratum::TimeSludge, Some(TimePocketKind::Fast)) => {
            // Speed: long blue-white streaks racing sideways.
            let streak = hash(0, y, 725);
            let run = (x + (streak % 64) as i32).rem_euclid(37);
            if run < 9 && streak.is_multiple_of(3) {
                index_of(if run < 3 { ICE } else { SKY })
            } else if noise(x, y, 11, 727) > 0.65 {
                index_of(DEEP_BLUE)
            } else {
                index_of(NIGHT)
            }
        }
        (Stratum::TimeSludge, None) => {
            // Sepia murk with frozen drips hanging in it.
            let drip = hash(x.div_euclid(5), 0, 729);
            let length = (drip % 9) as i32;
            if drip.is_multiple_of(7) && x.rem_euclid(5) == 2 && y.rem_euclid(40) < length {
                index_of(AMBER)
            } else if noise(x, y, 10, 731) > 0.62 {
                index_of(SOIL)
            } else {
                index_of(SOIL_DARK)
            }
        }
        (Stratum::EchoCaves, _) => {
            // Black, with faint ripple rings (what a ping leaves behind).
            let (cx, cy) = (x.div_euclid(24) * 24 + 12, y.div_euclid(24) * 24 + 12);
            let ring = (x - cx).abs().max((y - cy).abs()) + (x - cx).abs().min((y - cy).abs()) / 2;
            if ring.rem_euclid(7) == 0 && hash(cx, cy, 733).is_multiple_of(3) {
                index_of(NIGHT)
            } else {
                index_of(INK)
            }
        }
        _ => {
            // Prism Desert: dark teal with bands of spectrum sand.
            let band = (y + (noise(x, y, 18, 735) * 10.0) as i32).rem_euclid(21);
            match band {
                0 => index_of(MAROON),
                1 => index_of(PINE),
                2 => index_of(DEEP_BLUE),
                _ if hash(x, y, 737).is_multiple_of(173) => index_of(ICE),
                _ if noise(x, y, 8, 739) > 0.66 => index_of(TEAL_DARK),
                _ => index_of(NIGHT),
            }
        }
    }
}

/// Paints the open ground at `position` in a far stratum: its back wall, shadowed under rock.
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    stratum: Stratum,
    up: Cell,
    left: Cell,
    right: Cell,
) {
    let time = (stratum == Stratum::TimeSludge)
        .then(|| time_ground(terrain, position))
        .flatten();
    let shade = index_of(INK);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && (px == 0 || (px == 1 && bayer4(gx, gy) < 8)))
                || (right == Cell::Solid
                    && (px == TILE_ART - 1 || (px == TILE_ART - 2 && bayer4(gx, gy) < 8)));
            let index = if shadowed {
                shade
            } else {
                back_pixel(stratum, time, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}
