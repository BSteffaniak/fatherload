//! Palette indices and the "one step darker" ramp walk used for shadows and fog.
//!
//! Procedural art (terrain, sky) is rasterized into buffers of palette indices (`u8` positions in
//! [`PALETTE`]) so shading stays on-palette: [`darker`] moves a colour one step down its ramp
//! toward `INK` instead of multiplying RGB.

use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "the darker table names colours from the whole palette"
)]
use super::palette::*;

/// A position in [`PALETTE`].
pub type PaletteIndex = u8;

/// Marks a transparent pixel in an index buffer.
pub const TRANSPARENT: PaletteIndex = u8::MAX;

/// The [`PALETTE`] index of `color` (compile-time; panics for an off-palette colour).
#[must_use]
pub const fn index_of(color: Color) -> PaletteIndex {
    let mut index = 0;
    while index < PALETTE.len() {
        let entry = PALETTE[index].1;
        if entry.r == color.r && entry.g == color.g && entry.b == color.b {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the palette has fewer than 256 colours"
            )]
            return index as PaletteIndex;
        }
        index += 1;
    }
    panic!("colour is not in the palette");
}

/// The colour at palette index `index` (`CLEAR` for [`TRANSPARENT`]).
#[must_use]
pub const fn color_at(index: PaletteIndex) -> Color {
    if (index as usize) < PALETTE.len() {
        PALETTE[index as usize].1
    } else {
        CLEAR
    }
}

/// Each colour's next darker step (by ramp, ending in `INK`).
const DARKER_PAIRS: [(Color, Color); 44] = [
    (INK, INK),
    (NIGHT, INK),
    (SLATE_DARK, NIGHT),
    (SLATE, SLATE_DARK),
    (SLATE_LIGHT, SLATE),
    (FOG, SLATE_LIGHT),
    (MIST, FOG),
    (BONE, MIST),
    (SOIL_DARK, NIGHT),
    (SOIL, SOIL_DARK),
    (LOAM, SOIL),
    (CLAY, LOAM),
    (SAND, CLAY),
    (STONE_DARK, NIGHT),
    (STONE, STONE_DARK),
    (STONE_LIGHT, STONE),
    (DEEP_BLUE, NIGHT),
    (OCEAN, DEEP_BLUE),
    (SKY, OCEAN),
    (SKY_LIGHT, SKY),
    (ICE, SKY_LIGHT),
    (PINE, NIGHT),
    (MOSS, PINE),
    (LEAF, MOSS),
    (LIME, LEAF),
    (MAROON, NIGHT),
    (RED, MAROON),
    (ORANGE, RED),
    (AMBER, ORANGE),
    (GOLD, AMBER),
    (CREAM, GOLD),
    (PLUM, NIGHT),
    (VIOLET, PLUM),
    (LAVENDER, VIOLET),
    (ROSE, MAROON),
    (PINK, ROSE),
    (TEAL_DARK, NIGHT),
    (TEAL, TEAL_DARK),
    (MINT, TEAL),
    (RUST, SOIL_DARK),
    (COPPER, RUST),
    (STEEL_DARK, SLATE_DARK),
    (STEEL, STEEL_DARK),
    (STEEL_LIGHT, STEEL),
];

const fn build_darker() -> [PaletteIndex; 44] {
    let mut table = [0; 44];
    let mut index = 0;
    while index < DARKER_PAIRS.len() {
        let (from, to) = DARKER_PAIRS[index];
        table[index_of(from) as usize] = index_of(to);
        index += 1;
    }
    table
}

static DARKER: [PaletteIndex; 44] = build_darker();

/// `index` one ramp step darker ([`TRANSPARENT`] stays transparent).
#[must_use]
pub fn darker(index: PaletteIndex) -> PaletteIndex {
    DARKER.get(usize::from(index)).copied().unwrap_or(index)
}

/// `index` darkened `steps` times.
#[must_use]
pub fn darker_by(mut index: PaletteIndex, steps: u8) -> PaletteIndex {
    for _ in 0..steps {
        index = darker(index);
    }
    index
}

/// The 4x4 ordered-dither threshold at `(x, y)`, in `0..16`.
#[must_use]
pub const fn bayer4(x: i32, y: i32) -> u8 {
    const MATRIX: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    MATRIX[(y & 3) as usize][(x & 3) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_colour_darkens_to_ink() {
        for (index, (name, _)) in PALETTE.iter().enumerate() {
            let mut shade = PaletteIndex::try_from(index).unwrap();
            for _ in 0..8 {
                shade = darker(shade);
            }
            assert_eq!(shade, index_of(INK), "{name}");
        }
        assert_eq!(darker(TRANSPARENT), TRANSPARENT);
    }

    #[test]
    fn indices_round_trip() {
        for (name, color) in PALETTE {
            assert_eq!(color_at(index_of(color)), color, "{name}");
        }
        assert_eq!(color_at(TRANSPARENT).a, 0);
    }
}
