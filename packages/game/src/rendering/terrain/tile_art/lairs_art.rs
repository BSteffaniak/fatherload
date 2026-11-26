//! The Wave 12 worlds' lair tiles (Wave 14 finish, `fatherload_simulation::planet::lairs`): each
//! boss's relic set in a block of dark builder stone, and the builder lamps lining the halls
//! (a bronze sconce round a bright flame). The halls' seals, vault walls and doors are the
//! outland ones (`outlands_art`).

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "presentation code: art math reads top to bottom"
)]

use fatherload_simulation::planet::lairs::LairTile;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, index_of};

use super::{Edges, Strata, TILE_ART, TileCanvas, hash, paint_edges};

type Color = raylib::prelude::Color;

/// A relic's colours: (shadow, body, light, spark).
const fn relic_colours(tile: LairTile) -> [Color; 4] {
    match tile {
        LairTile::BrambleCrown => [MOSS, LEAF, LIME, ROSE],
        LairTile::TidePearl => [SKY, SKY_LIGHT, ICE, BONE],
        LairTile::FrostHeart => [OCEAN, SKY, ICE, BONE],
        LairTile::SentinelEye => [AMBER, GOLD, CREAM, RED],
        LairTile::DrakeHeart => [RED, ORANGE, AMBER, GOLD],
        LairTile::ChoirCore => [VIOLET, LAVENDER, PINK, BONE],
        LairTile::Lamp => [COPPER, AMBER, GOLD, CREAM],
    }
}

/// The colour a lair tile reads as (dust, map previews, the minimap).
#[must_use]
pub const fn lair_color(tile: LairTile) -> Color {
    relic_colours(tile)[1]
}

/// The light a lair tile gives (the light overlay's tint).
#[must_use]
#[allow(dead_code, reason = "the round ground's halos")]
pub const fn lair_glow(tile: LairTile) -> Color {
    match tile {
        LairTile::Lamp => CREAM,
        _ => relic_colours(tile)[2],
    }
}

/// The dark builder stone the relics and lamps are set in.
const fn stone(x: i32, y: i32, px: i32, py: i32) -> PaletteIndex {
    let seam = px == 0 || py == 0 || (py == 8 && px > 0);
    if seam {
        index_of(INK)
    } else if hash(x, y, 7_101).is_multiple_of(17) {
        index_of(SLATE)
    } else if px + py < 8 {
        index_of(SLATE_LIGHT)
    } else {
        index_of(SLATE_DARK)
    }
}

/// One art pixel of `tile` (tile pixel `(px, py)`, absolute `(x, y)`).
fn body(tile: LairTile, x: i32, y: i32, px: i32, py: i32) -> PaletteIndex {
    let [dark, mid, light, spark] = relic_colours(tile);
    let (cx, cy) = (px - 8, py - 8);
    let d2 = cx * cx + cy * cy;
    match tile {
        LairTile::Lamp => {
            // A bronze sconce with a flame over it.
            let flame = (cx * cx * 4 + (cy + 2) * (cy + 2)) <= 18 && cy <= 0;
            let core = (cx * cx * 6 + (cy + 1) * (cy + 1)) <= 6 && cy <= 0;
            let cup = (2..=4).contains(&cy) && cx.abs() <= 4 - (cy - 2);
            if core {
                index_of(spark)
            } else if flame {
                index_of(if cy < -3 { light } else { mid })
            } else if cup {
                index_of(if cx < 0 { COPPER } else { RUST })
            } else {
                stone(x, y, px, py)
            }
        }
        LairTile::SentinelEye => {
            // A golden eye: almond white, a red iris, a gold rim.
            let almond = cx * cx + cy * cy * 4 <= 36;
            let rim = cx * cx + cy * cy * 4 <= 52;
            if d2 <= 3 {
                index_of(INK)
            } else if d2 <= 8 && almond {
                index_of(spark)
            } else if almond {
                index_of(CREAM)
            } else if rim {
                index_of(if cy < 0 { light } else { dark })
            } else {
                stone(x, y, px, py)
            }
        }
        LairTile::BrambleCrown => {
            // A ring of thorned vine with points and a rose on top.
            let ring = (2..=4).contains(&cy) && cx.abs() <= 6;
            let points = (-3..2).contains(&cy) && cx.rem_euclid(3) == 0 && cx.abs() <= 6;
            let rose = (cx * cx + (cy + 4) * (cy + 4)) <= 3;
            if rose {
                index_of(spark)
            } else if ring {
                index_of(if cy == 2 { light } else { mid })
            } else if points {
                index_of(dark)
            } else {
                stone(x, y, px, py)
            }
        }
        _ => {
            // A round gem (a pearl, a heart, a core) with a highlight up-left.
            if d2 <= 2 && cx <= 0 && cy <= 0 || (cx == -2 && cy == -2) {
                index_of(spark)
            } else if d2 <= 14 {
                index_of(if cx + cy < 0 { light } else { mid })
            } else if d2 <= 24 {
                index_of(dark)
            } else {
                stone(x, y, px, py)
            }
        }
    }
}

/// Paints lair tile `tile` on `canvas`.
pub(super) fn paint(canvas: &mut TileCanvas, tile: LairTile, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body(tile, gx, gy, px, py));
        }
    }
    paint_edges(
        canvas,
        strata,
        edges,
        index_of(SLATE_LIGHT),
        index_of(INK),
        true,
    );
}

/// A lair tile's base pixel at absolute art pixel `(x, y)` (a host for ore stamps).
pub(super) fn base_pixel(tile: LairTile, x: i32, y: i32) -> PaletteIndex {
    body(tile, x, y, x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART))
}
