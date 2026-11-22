//! The Living Dark's back walls (Wave 8, Descent III): the open ground of each living stratum
//! shows its own wall, so a tunnel or a set piece reads as the place it is.
//!
//! * Fungal Forest: plum murk hung with pale mycelium threads and drifting spore motes.
//! * Root Labyrinth: dark soil braided with the silhouettes of roots behind roots.
//! * The Hive: dim honeycomb, wax cells in shadow.
//! * Flesh Caverns: dark meat netted with veins, a wet glint here and there.
//! * Glow Sea: the abyss, near black, with plankton specks that only a light ever shows.

use fatherload_simulation::{
    descent::Stratum,
    terrain::{Terrain, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{Cell, TILE_ART, TileCanvas, hash, noise, worley};

/// The living stratum at `position` (`None` outside the Living Dark or on older worlds).
#[must_use]
pub(super) fn living_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    (stratum.descent() == fatherload_simulation::descent::Descent::LivingDark).then_some(stratum)
}

/// One art pixel of `stratum`'s back wall at absolute art pixel `(x, y)`.
#[must_use]
pub(super) fn back_pixel(stratum: Stratum, x: i32, y: i32) -> PaletteIndex {
    match stratum {
        Stratum::FungalForest => {
            // A few threads hang, swaying; motes drift in a soft plum murk.
            let sway = (noise(x, y, 14, 601) * 4.0) as i32;
            let thread = (x + sway).rem_euclid(23) == 0 && noise(x, y, 8, 603) > 0.5;
            if hash(x, y, 605).is_multiple_of(331) {
                index_of(LIME)
            } else if hash(x, y, 607).is_multiple_of(257) {
                index_of(MINT)
            } else if thread {
                index_of(PLUM)
            } else if noise(x, y, 10, 609) + f32::from(bayer4(x, y)) / 40.0 > 0.62 {
                index_of(PLUM)
            } else {
                index_of(NIGHT)
            }
        }
        Stratum::RootLabyrinth => {
            // Braids of roots far behind, in soil shadow.
            let wave = (noise(x, y, 10, 611) * 8.0) as i32;
            let braid = (x * 2 + y + wave).rem_euclid(13);
            let cross = (x - y * 2 + wave).rem_euclid(17);
            if braid == 0 || cross == 0 {
                index_of(INK)
            } else if braid <= 2 || cross <= 1 {
                index_of(SOIL)
            } else if noise(x, y, 7, 613) > 0.7 {
                index_of(SOIL_DARK)
            } else {
                index_of(NIGHT)
            }
        }
        Stratum::TheHive => {
            // Honeycomb in shadow: cell walls dim amber, hollows dark.
            let row = y.div_euclid(5);
            let shift = if row.rem_euclid(2) == 0 { 0 } else { 3 };
            let cx = (x + shift).rem_euclid(6);
            let cy = y.rem_euclid(5);
            let wall = cy == 0 || cx == 0;
            if wall {
                if noise(x, y, 8, 621) > 0.55 {
                    index_of(SOIL)
                } else {
                    index_of(SOIL_DARK)
                }
            } else if cy == 1 && hash(x, y, 623).is_multiple_of(9) {
                index_of(RUST)
            } else {
                index_of(NIGHT)
            }
        }
        Stratum::FleshCaverns => {
            // A net of veins over dark meat, wet glints catching.
            let (near, second, _, _) = worley(x, y, 9, 631, false);
            if second - near < 0.7 {
                index_of(RED)
            } else if second - near < 1.4 {
                index_of(MAROON)
            } else if hash(x, y, 633).is_multiple_of(97) {
                index_of(ROSE)
            } else if noise(x, y, 6, 635) > 0.62 {
                index_of(PLUM)
            } else {
                index_of(NIGHT)
            }
        }
        _ => {
            // The Glow Sea's abyss: near black, plankton specks, faint currents.
            if hash(x, y, 641).is_multiple_of(89) {
                index_of(TEAL)
            } else if hash(x, y, 643).is_multiple_of(53) {
                index_of(TEAL_DARK)
            } else if noise(x, y, 14, 645) + f32::from(bayer4(x, y)) / 50.0 > 0.7 {
                index_of(DEEP_BLUE)
            } else {
                index_of(INK)
            }
        }
    }
}

/// Paints an open tile of `stratum`: its back wall, shadowed under rock overhead and beside
/// rock walls (like every tunnel).
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    stratum: Stratum,
    up: Cell,
    left: Cell,
    right: Cell,
) {
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
                back_pixel(stratum, gx, gy)
            };
            canvas.set(px, py, index);
        }
    }
}
