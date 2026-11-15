//! Cosmetic names to palette colours: paint ramps, flame ramps and each paint's accent colour.

use fatherload_simulation::cosmetics::{FlameColor, PaintColor};
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

/// A four-step paint ramp, lit to deep.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ramp {
    pub light: Color,
    pub body: Color,
    pub shade: Color,
    pub deep: Color,
}

const fn ramp(light: Color, body: Color, shade: Color, deep: Color) -> Ramp {
    Ramp {
        light,
        body,
        shade,
        deep,
    }
}

/// The palette ramp of a paint colour.
#[must_use]
pub const fn paint_ramp(paint: PaintColor) -> Ramp {
    match paint {
        PaintColor::Gold => ramp(CREAM, GOLD, AMBER, ORANGE),
        PaintColor::Tangerine => ramp(AMBER, ORANGE, RUST, MAROON),
        PaintColor::Crimson => ramp(ROSE, RED, MAROON, PLUM),
        PaintColor::Rose => ramp(PINK, ROSE, RED, MAROON),
        PaintColor::Blossom => ramp(BONE, PINK, ROSE, MAROON),
        PaintColor::Violet => ramp(LAVENDER, VIOLET, PLUM, NIGHT),
        PaintColor::Sky => ramp(ICE, SKY_LIGHT, SKY, OCEAN),
        PaintColor::Cobalt => ramp(SKY, OCEAN, DEEP_BLUE, NIGHT),
        PaintColor::Teal => ramp(MINT, TEAL, TEAL_DARK, NIGHT),
        PaintColor::Mint => ramp(ICE, MINT, TEAL, TEAL_DARK),
        PaintColor::Lime => ramp(LIME, LEAF, MOSS, PINE),
        PaintColor::Forest => ramp(LEAF, MOSS, PINE, NIGHT),
        PaintColor::Copper => ramp(SAND, COPPER, RUST, SOIL_DARK),
        PaintColor::Sand => ramp(CREAM, SAND, CLAY, LOAM),
        PaintColor::Pearl => ramp(BONE, MIST, FOG, SLATE_LIGHT),
        PaintColor::Steel => ramp(STEEL_LIGHT, STEEL, STEEL_DARK, SLATE_DARK),
        PaintColor::Graphite => ramp(SLATE_LIGHT, SLATE, SLATE_DARK, NIGHT),
        PaintColor::Stealth => ramp(SLATE, SLATE_DARK, NIGHT, INK),
    }
}

/// A paint's bright identity colour: name tags, the split-screen HUD title, minimap markers and
/// garage swatches. Always readable on the dark UI panels.
#[must_use]
pub const fn paint_accent(paint: PaintColor) -> Color {
    match paint {
        PaintColor::Gold => GOLD,
        PaintColor::Tangerine => ORANGE,
        PaintColor::Crimson => RED,
        PaintColor::Rose => ROSE,
        PaintColor::Blossom => PINK,
        PaintColor::Violet => LAVENDER,
        PaintColor::Sky => SKY_LIGHT,
        PaintColor::Cobalt => SKY,
        PaintColor::Teal | PaintColor::Mint => MINT,
        PaintColor::Lime => LIME,
        PaintColor::Forest => LEAF,
        PaintColor::Copper => COPPER,
        PaintColor::Sand => SAND,
        PaintColor::Pearl => BONE,
        PaintColor::Steel => STEEL_LIGHT,
        PaintColor::Graphite => FOG,
        PaintColor::Stealth => SLATE_LIGHT,
    }
}

/// A flame ramp: `deep` edge, `mid` body, `bright` inner, `core` hottest pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlameRamp {
    pub deep: Color,
    pub mid: Color,
    pub bright: Color,
    pub core: Color,
}

const fn flame(deep: Color, mid: Color, bright: Color, core: Color) -> FlameRamp {
    FlameRamp {
        deep,
        mid,
        bright,
        core,
    }
}

/// The colours [`FlameColor::Prism`] cycles through.
const PRISM: [FlameColor; 5] = [
    FlameColor::Classic,
    FlameColor::Gold,
    FlameColor::Green,
    FlameColor::Blue,
    FlameColor::Violet,
];

/// The palette ramp of a flame colour; `step` animates the prism flame.
#[must_use]
pub const fn flame_ramp(color: FlameColor, step: usize) -> FlameRamp {
    match color {
        FlameColor::Classic => flame(RED, ORANGE, GOLD, CREAM),
        FlameColor::Gold => flame(ORANGE, AMBER, GOLD, CREAM),
        FlameColor::Blue => flame(OCEAN, SKY, SKY_LIGHT, ICE),
        FlameColor::Teal => flame(TEAL_DARK, TEAL, MINT, ICE),
        FlameColor::Green => flame(MOSS, LEAF, LIME, CREAM),
        FlameColor::Violet => flame(PLUM, VIOLET, LAVENDER, PINK),
        FlameColor::Pink => flame(MAROON, ROSE, PINK, BONE),
        FlameColor::White => flame(SKY, SKY_LIGHT, ICE, BONE),
        FlameColor::Prism => flame_ramp(PRISM[step % PRISM.len()], 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_paint_has_a_distinct_ramp() {
        let mut seen = Vec::new();
        for paint in PaintColor::ALL {
            let ramp = paint_ramp(*paint);
            assert!(!seen.contains(&ramp), "{paint:?} repeats a ramp");
            seen.push(ramp);
        }
    }

    #[test]
    fn colours_come_from_the_palette() {
        let on_palette = |color: Color| PALETTE.iter().any(|(_, entry)| *entry == color);
        for paint in PaintColor::ALL {
            let ramp = paint_ramp(*paint);
            for color in [ramp.light, ramp.body, ramp.shade, ramp.deep] {
                assert!(on_palette(color), "{paint:?}");
            }
            assert!(on_palette(paint_accent(*paint)));
        }
        for color in FlameColor::ALL {
            for step in 0..5 {
                let ramp = flame_ramp(*color, step);
                for color in [ramp.deep, ramp.mid, ramp.bright, ramp.core] {
                    assert!(on_palette(color));
                }
            }
        }
    }
}
