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
        // Milestone rewards (WP-FEEL part 2).
        PaintColor::Jade => ramp(MINT, TEAL, TEAL_DARK, PINE),
        PaintColor::Glowcap => ramp(CREAM, LIME, LEAF, MOSS),
        PaintColor::Chrome => ramp(BONE, MIST, STEEL_LIGHT, STEEL),
        PaintColor::Abyss => ramp(OCEAN, DEEP_BLUE, NIGHT, INK),
        PaintColor::Magma => ramp(GOLD, ORANGE, RED, MAROON),
        // Expedition milestone rewards (wave 8 story).
        PaintColor::Khaki => ramp(SAND, CLAY, LOAM, SOIL_DARK),
        PaintColor::Ochre => ramp(CREAM, AMBER, RUST, SOIL),
        PaintColor::Bronze => ramp(COPPER, RUST, SOIL, SOIL_DARK),
        PaintColor::Lumen => ramp(ICE, MINT, TEAL, DEEP_BLUE),
        PaintColor::Phantom => ramp(BONE, MIST, FOG, SLATE),
        PaintColor::Anomaly => ramp(LAVENDER, VIOLET, PLUM, INK),
        PaintColor::Glitch => ramp(LIME, PINK, PLUM, NIGHT),
        // The finale's milestone rewards (wave 10 story).
        PaintColor::Quartz => ramp(ICE, LAVENDER, VIOLET, PLUM),
        PaintColor::Spore => ramp(LIME, LEAF, MOSS, NIGHT),
        PaintColor::Brine => ramp(ICE, SKY, OCEAN, DEEP_BLUE),
        PaintColor::InnerSun => ramp(CREAM, GOLD, AMBER, RUST),
        PaintColor::Prism => ramp(PINK, LAVENDER, SKY, OCEAN),
        PaintColor::Alloy => ramp(STEEL_LIGHT, SAND, COPPER, SLATE_DARK),
        PaintColor::Heartlight => ramp(BONE, CREAM, GOLD, AMBER),
        PaintColor::Rubble => ramp(FOG, SLATE_LIGHT, SLATE, SLATE_DARK),
        PaintColor::Corona => ramp(CREAM, ORANGE, RED, PLUM),
        PaintColor::Mycelium => ramp(BONE, PINK, PLUM, NIGHT),
        PaintColor::DeepScale => ramp(MINT, TEAL, OCEAN, NIGHT),
        PaintColor::SerpentGold => ramp(GOLD, ORANGE, RUST, MAROON),
        PaintColor::DeadPixel => ramp(SKY_LIGHT, LIME, PINK, INK),
        PaintColor::GateGold => ramp(CREAM, GOLD, COPPER, SOIL_DARK),
        PaintColor::Nacre => ramp(BONE, PINK, LAVENDER, SLATE),
        PaintColor::Jungle => ramp(LEAF, MOSS, PINE, SOIL_DARK),
        PaintColor::Mirrorglass => ramp(BONE, ICE, STEEL_LIGHT, SLATE),
        PaintColor::Lamplight => ramp(CREAM, AMBER, COPPER, SOIL),
        PaintColor::DriveWhite => ramp(BONE, MIST, ICE, SKY),
        PaintColor::FathersLoad => ramp(CREAM, RED, MAROON, SOIL_DARK),
        PaintColor::Starward => ramp(BONE, LAVENDER, DEEP_BLUE, INK),
    }
}

/// A paint's bright identity colour: name tags, the split-screen HUD title, minimap markers and
/// garage swatches. Always readable on the dark UI panels.
#[must_use]
pub const fn paint_accent(paint: PaintColor) -> Color {
    match paint {
        PaintColor::Gold
        | PaintColor::InnerSun
        | PaintColor::SerpentGold
        | PaintColor::GateGold => GOLD,
        PaintColor::Tangerine | PaintColor::Magma | PaintColor::Corona => ORANGE,
        PaintColor::Ochre | PaintColor::Lamplight => AMBER,
        PaintColor::Phantom | PaintColor::DriveWhite => MIST,
        PaintColor::Crimson | PaintColor::FathersLoad => RED,
        PaintColor::Rose => ROSE,
        PaintColor::Blossom | PaintColor::Prism | PaintColor::Mycelium | PaintColor::Nacre => PINK,
        PaintColor::Violet | PaintColor::Anomaly | PaintColor::Quartz | PaintColor::Starward => {
            LAVENDER
        }
        PaintColor::Sky => SKY_LIGHT,
        PaintColor::Cobalt | PaintColor::Abyss | PaintColor::Brine => SKY,
        PaintColor::Teal
        | PaintColor::Mint
        | PaintColor::Jade
        | PaintColor::Lumen
        | PaintColor::DeepScale => MINT,
        PaintColor::Lime
        | PaintColor::Glowcap
        | PaintColor::Glitch
        | PaintColor::Spore
        | PaintColor::DeadPixel => LIME,
        PaintColor::Forest | PaintColor::Jungle => LEAF,
        PaintColor::Copper | PaintColor::Bronze => COPPER,
        PaintColor::Sand | PaintColor::Khaki | PaintColor::Alloy => SAND,
        PaintColor::Pearl | PaintColor::Chrome | PaintColor::Mirrorglass => BONE,
        PaintColor::Steel => STEEL_LIGHT,
        PaintColor::Graphite | PaintColor::Rubble => FOG,
        PaintColor::Stealth => SLATE_LIGHT,
        PaintColor::Heartlight => CREAM,
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
        // Milestone rewards (WP-FEEL part 2).
        FlameColor::Ember => flame(MAROON, RED, ORANGE, AMBER),
        FlameColor::Plasma => flame(VIOLET, LAVENDER, ICE, BONE),
        FlameColor::Aether => flame(OCEAN, MINT, ICE, BONE),
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
