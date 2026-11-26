//! The alien towns' tiles (Wave 12 aliens track, `fatherload_simulation::alien`): each
//! species' street paving and the lexicon stones buried under their towns.
//!
//! * Vessi vault ice: packed blue slabs in long courses with frost-white seams.
//! * Tokkari mound stone: fused, rounded cobbles of warm brown with dark grout and a fired lip.
//! * Siphra starglass: pale violet glass panes with a few points of light inside.
//! * Lexicon stones: a dark tablet in the rock carved with a few of the species' glyphs,
//!   glinting in the species' ink.

use fatherload_simulation::alien::{AlienSpecies, AlienTile};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp};

const ICE_RAMP: Ramp = ramp([OCEAN, SKY, SKY_LIGHT, ICE]);
const MOUND_RAMP: Ramp = ramp([SOIL_DARK, SOIL, CLAY, SAND]);
const GLASS_RAMP: Ramp = ramp([PLUM, VIOLET, LAVENDER, CREAM]);
const TABLET_RAMP: Ramp = ramp([INK, SLATE_DARK, SLATE, STONE]);

/// Representative colour of an alien tile (drill dust, debris, the map).
#[must_use]
pub const fn alien_color(tile: AlienTile) -> Color {
    match tile {
        AlienTile::VessiPaving => SKY_LIGHT,
        AlienTile::TokkariPaving => CLAY,
        AlienTile::SiphraPaving => LAVENDER,
        AlienTile::LexiconStone(_) => GOLD,
    }
}

const fn ink(species: AlienSpecies) -> PaletteIndex {
    match species {
        AlienSpecies::Vessi => index_of(SKY_LIGHT),
        AlienSpecies::Tokkari => index_of(AMBER),
        AlienSpecies::Siphra => index_of(MINT),
    }
}

fn body_pixel(tile: AlienTile, x: i32, y: i32, lx: i32, ly: i32) -> PaletteIndex {
    match tile {
        AlienTile::VessiPaving => {
            // Long slabs, offset every course, frost-white seams.
            let course = y.div_euclid(6);
            let shift = if course.rem_euclid(2) == 0 { 0 } else { 11 };
            let (bx, by) = ((x + shift).rem_euclid(22), y.rem_euclid(6));
            if bx == 0 || by == 0 {
                ICE_RAMP[3]
            } else if by == 1 && !hash(x, y, 1_201).is_multiple_of(5) {
                ICE_RAMP[2]
            } else if noise(x, y, 7, 1_203) + f32::from(bayer4(x, y)) / 40.0 > 0.72 {
                ICE_RAMP[0]
            } else {
                ICE_RAMP[1]
            }
        }
        AlienTile::TokkariPaving => {
            // Rounded fused cobbles with dark grout.
            let (cx, cy) = (x.div_euclid(5), y.div_euclid(5));
            let jitter = hash(cx, cy, 1_211) % 3;
            let (bx, by) = (x.rem_euclid(5), y.rem_euclid(5));
            let edge = bx == 0 || by == 0 || (bx == 4 && by == 4);
            if edge {
                MOUND_RAMP[0]
            } else if bx == 1 && by == 1 {
                MOUND_RAMP[3]
            } else if jitter == 0 {
                MOUND_RAMP[1]
            } else {
                MOUND_RAMP[2]
            }
        }
        AlienTile::SiphraPaving => {
            // Glass panes with points of starlight.
            let (bx, by) = (x.rem_euclid(8), y.rem_euclid(8));
            if bx == 0 || by == 0 {
                GLASS_RAMP[0]
            } else if hash(x, y, 1_221).is_multiple_of(29) {
                GLASS_RAMP[3]
            } else if (bx + by) % 7 == 0 {
                GLASS_RAMP[2]
            } else {
                GLASS_RAMP[1]
            }
        }
        AlienTile::LexiconStone(species) => {
            // A dark tablet with a carved glyph or two glinting in the species' ink.
            let border = lx == 1 || ly == 1 || lx == TILE_ART - 2 || ly == TILE_ART - 2;
            if lx == 0 || ly == 0 || lx == TILE_ART - 1 || ly == TILE_ART - 1 {
                TABLET_RAMP[0]
            } else if border {
                TABLET_RAMP[3]
            } else {
                let glyph = crate::rendering::alien_art::glyph_bits(
                    species,
                    (hash(x.div_euclid(16), y.div_euclid(16), 1_231) % 8) as u8,
                );
                let (gx, gy) = (lx - 5, ly - 4);
                let carved = (0..5).contains(&gx)
                    && (0..6).contains(&gy)
                    && glyph[gy as usize] & (1 << (4 - gx)) != 0;
                if carved {
                    ink(species)
                } else if (lx + ly) % 5 == 0 {
                    TABLET_RAMP[2]
                } else {
                    TABLET_RAMP[1]
                }
            }
        }
    }
}

/// Paints an alien tile.
pub(super) fn paint(canvas: &mut TileCanvas, tile: AlienTile, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy, px, py));
        }
    }
    if tile.lexicon().is_some() {
        return;
    }
    let lip = match tile {
        AlienTile::VessiPaving => ICE_RAMP[3],
        AlienTile::TokkariPaving => MOUND_RAMP[3],
        AlienTile::SiphraPaving | AlienTile::LexiconStone(_) => GLASS_RAMP[3],
    };
    let shade = match tile {
        AlienTile::VessiPaving => ICE_RAMP[0],
        AlienTile::TokkariPaving => MOUND_RAMP[0],
        AlienTile::SiphraPaving | AlienTile::LexiconStone(_) => GLASS_RAMP[0],
    };
    paint_edges(canvas, strata, edges, lip, shade, true);
}
