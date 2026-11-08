//! The game's named palette: 44 colours every sprite, tile and UI element draws from.
//!
//! Ramps run dark to light. Pick colours by role, never raw RGB, so the whole game stays in one
//! cohesive set (see `rendering/ART_GUIDE.md` for the swatches and roles).

use raylib::prelude::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::new(r, g, b, 255)
}

// Neutrals: outlines, UI panels, text.
/// Outline and deepest shadow.
pub const INK: Color = rgb(0x0e, 0x0b, 0x14);
pub const NIGHT: Color = rgb(0x1a, 0x16, 0x26);
pub const SLATE_DARK: Color = rgb(0x28, 0x24, 0x3a);
pub const SLATE: Color = rgb(0x3c, 0x38, 0x54);
pub const SLATE_LIGHT: Color = rgb(0x5a, 0x56, 0x77);
pub const FOG: Color = rgb(0x89, 0x87, 0xa6);
pub const MIST: Color = rgb(0xc1, 0xc2, 0xd6);
/// Brightest neutral: body text, specular highlights.
pub const BONE: Color = rgb(0xf3, 0xef, 0xe4);

// Earth: dirt, clay, sand.
pub const SOIL_DARK: Color = rgb(0x3a, 0x24, 0x18);
pub const SOIL: Color = rgb(0x5c, 0x3a, 0x22);
pub const LOAM: Color = rgb(0x80, 0x54, 0x2f);
pub const CLAY: Color = rgb(0xa8, 0x65, 0x3f);
pub const SAND: Color = rgb(0xd4, 0x9b, 0x5c);

// Stone: rock strata, concrete.
pub const STONE_DARK: Color = rgb(0x34, 0x30, 0x2f);
pub const STONE: Color = rgb(0x57, 0x52, 0x50);
pub const STONE_LIGHT: Color = rgb(0x85, 0x7e, 0x76);

// Cool blues: sky, water, glass, diamond.
pub const DEEP_BLUE: Color = rgb(0x1b, 0x2a, 0x4e);
pub const OCEAN: Color = rgb(0x29, 0x4d, 0x89);
pub const SKY: Color = rgb(0x4d, 0x8e, 0xd5);
pub const SKY_LIGHT: Color = rgb(0x8a, 0xc5, 0xee);
pub const ICE: Color = rgb(0xcb, 0xed, 0xf8);

// Greens: plants, emerald, "ok" states.
pub const PINE: Color = rgb(0x1d, 0x3b, 0x2b);
pub const MOSS: Color = rgb(0x2e, 0x6a, 0x39);
pub const LEAF: Color = rgb(0x4e, 0xa3, 0x48);
pub const LIME: Color = rgb(0x9b, 0xd8, 0x5a);

// Warm ramp: lava, fire, fuel, gold, warnings.
pub const MAROON: Color = rgb(0x59, 0x1a, 0x25);
pub const RED: Color = rgb(0xb8, 0x30, 0x2f);
pub const ORANGE: Color = rgb(0xe8, 0x70, 0x2a);
pub const AMBER: Color = rgb(0xf2, 0xa9, 0x3b);
pub const GOLD: Color = rgb(0xff, 0xd7, 0x5e);
pub const CREAM: Color = rgb(0xff, 0xf2, 0xb2);

// Purples and pinks: mythril, ruby, artifacts.
pub const PLUM: Color = rgb(0x3d, 0x1f, 0x4b);
pub const VIOLET: Color = rgb(0x6e, 0x3f, 0xa1);
pub const LAVENDER: Color = rgb(0xa9, 0x7d, 0xe1);
pub const ROSE: Color = rgb(0xdf, 0x5a, 0x8b);
pub const PINK: Color = rgb(0xf6, 0xa4, 0xc1);

// Teals: uranium glow, screens, accents.
pub const TEAL_DARK: Color = rgb(0x15, 0x49, 0x4e);
pub const TEAL: Color = rgb(0x2a, 0x8a, 0x82);
pub const MINT: Color = rgb(0x6f, 0xe0, 0xc0);

// Metals: rig hull, machinery, copper and iron.
pub const RUST: Color = rgb(0x8a, 0x3d, 0x1f);
pub const COPPER: Color = rgb(0xc5, 0x6b, 0x35);
pub const STEEL_DARK: Color = rgb(0x49, 0x51, 0x63);
pub const STEEL: Color = rgb(0x79, 0x84, 0x97);
pub const STEEL_LIGHT: Color = rgb(0xb4, 0xbe, 0xcb);

/// Fully transparent; `.` and space in sprite grids.
pub const CLEAR: Color = Color::new(0, 0, 0, 0);

/// Every palette colour with its name, in ramp order (docs, tests, debug swatches).
pub const PALETTE: [(&str, Color); 44] = [
    ("INK", INK),
    ("NIGHT", NIGHT),
    ("SLATE_DARK", SLATE_DARK),
    ("SLATE", SLATE),
    ("SLATE_LIGHT", SLATE_LIGHT),
    ("FOG", FOG),
    ("MIST", MIST),
    ("BONE", BONE),
    ("SOIL_DARK", SOIL_DARK),
    ("SOIL", SOIL),
    ("LOAM", LOAM),
    ("CLAY", CLAY),
    ("SAND", SAND),
    ("STONE_DARK", STONE_DARK),
    ("STONE", STONE),
    ("STONE_LIGHT", STONE_LIGHT),
    ("DEEP_BLUE", DEEP_BLUE),
    ("OCEAN", OCEAN),
    ("SKY", SKY),
    ("SKY_LIGHT", SKY_LIGHT),
    ("ICE", ICE),
    ("PINE", PINE),
    ("MOSS", MOSS),
    ("LEAF", LEAF),
    ("LIME", LIME),
    ("MAROON", MAROON),
    ("RED", RED),
    ("ORANGE", ORANGE),
    ("AMBER", AMBER),
    ("GOLD", GOLD),
    ("CREAM", CREAM),
    ("PLUM", PLUM),
    ("VIOLET", VIOLET),
    ("LAVENDER", LAVENDER),
    ("ROSE", ROSE),
    ("PINK", PINK),
    ("TEAL_DARK", TEAL_DARK),
    ("TEAL", TEAL),
    ("MINT", MINT),
    ("RUST", RUST),
    ("COPPER", COPPER),
    ("STEEL_DARK", STEEL_DARK),
    ("STEEL", STEEL),
    ("STEEL_LIGHT", STEEL_LIGHT),
];

/// `color` with alpha `alpha` (for overlays and fades; keep the RGB from the palette).
#[must_use]
pub const fn with_alpha(color: Color, alpha: u8) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}

/// Whether `color` is a palette colour (any alpha).
#[cfg(test)]
#[must_use]
pub fn is_palette_color(color: Color) -> bool {
    color.a == 0
        || PALETTE
            .iter()
            .any(|(_, entry)| entry.r == color.r && entry.g == color.g && entry.b == color.b)
}
