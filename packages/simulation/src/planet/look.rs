//! How a theme's worlds look from orbit (Wave 13 art-audio).
//!
//! Plain presentation data the game's planet shader draws: land in three tones, seas, cloud
//! decks, polar caps, the night side's city lights, glowing cracks, weather bands and rings.
//! Nothing here touches generation or play.
//!
//! A theme gives its look through [`super::hooks::PlanetThemeHooks::orbital_look`], whose
//! default reads [`ORBITAL_LOOKS`] by the theme's id: a theme track either appends
//! `(MY_THEME, MY_LOOK)` there or overrides the hook. A theme with no look (the Wave 11
//! placeholders) is drawn from the space generator's tag instead (the game's preset per tag),
//! so a fungal, machine or gas-giant world keeps its own face until its theme brings one.

use super::theme::PlanetThemeId;

/// An sRGB colour.
pub type Rgb = [u8; 3];

/// A layer of the look: its colour and how much of it there is (per mille; what "how much"
/// means is the layer's own, see [`OrbitalLook`]).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Layer {
    pub colour: Rgb,
    pub permille: u16,
}

/// A body's rings as drawn (seen face on: the plane of flight is the rings' plane).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RingLook {
    /// The inner and outer lanes' colours.
    pub colours: [Rgb; 2],
    /// Where decorative rings lie when the body has none of its own (per mille of the radius,
    /// from the centre; `0` draws rings only round bodies the generator ringed).
    pub inner_permille: u16,
    pub outer_permille: u16,
}

/// A theme's face from orbit (see the module docs).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrbitalLook {
    /// Land, dark to light (low ground to high).
    pub land: [Rgb; 3],
    /// The atmosphere's glow round the rim.
    pub air: Rgb,
    /// Seas: their colour and the share of the surface under water.
    pub sea: Option<Layer>,
    /// Cloud decks: their colour and the share of the sky they cover.
    pub clouds: Option<Layer>,
    /// Polar caps: their colour and how far from each pole they reach (per mille of the way
    /// to the equator).
    pub caps: Option<Layer>,
    /// The night side's lights (towns, floating cities): their colour and how thickly they
    /// sit on the land.
    pub lights: Option<Layer>,
    /// Glowing cracks (lava, crystal light): their colour and how much of the ground glows.
    pub glow: Option<Layer>,
    /// Weather bands instead of land (gas giants).
    pub bands: bool,
    /// Rings.
    pub ring: Option<RingLook>,
    /// Feature size: 1 (a few continents) to 4 (fine-grained ground).
    pub detail: u8,
}

const fn layer(colour: Rgb, permille: u16) -> Layer {
    Layer { colour, permille }
}

/// The jungle world: green continents under thick humid cloud, shallow green seas.
pub const JUNGLE_LOOK: OrbitalLook = OrbitalLook {
    land: [[24, 70, 40], [46, 112, 52], [104, 160, 64]],
    air: [120, 190, 230],
    sea: Some(layer([28, 84, 96], 300)),
    clouds: Some(layer([236, 244, 236], 520)),
    caps: None,
    lights: Some(layer([255, 214, 140], 120)),
    glow: None,
    bands: false,
    ring: None,
    detail: 3,
};

/// The ocean archipelago: a blue world of scattered islands and reefs.
pub const OCEAN_LOOK: OrbitalLook = OrbitalLook {
    land: [[60, 120, 70], [176, 168, 112], [222, 210, 160]],
    air: [140, 200, 250],
    sea: Some(layer([22, 66, 138], 800)),
    clouds: Some(layer([240, 246, 252], 380)),
    caps: Some(layer([220, 236, 248], 120)),
    lights: Some(layer([255, 220, 150], 140)),
    glow: None,
    bands: false,
    ring: None,
    detail: 4,
};

/// The glacier: ice sheets and pale frozen seas, broad caps, thin cloud.
pub const GLACIER_LOOK: OrbitalLook = OrbitalLook {
    land: [[112, 132, 160], [178, 204, 226], [236, 244, 252]],
    air: [190, 226, 255],
    sea: Some(layer([70, 118, 150], 260)),
    clouds: Some(layer([246, 250, 255], 260)),
    caps: Some(layer([250, 252, 255], 520)),
    lights: Some(layer([170, 220, 255], 60)),
    glow: None,
    bands: false,
    ring: None,
    detail: 2,
};

/// The dune world: banded sand seas and dark rock, a dusty sky.
pub const DUNE_LOOK: OrbitalLook = OrbitalLook {
    land: [[140, 86, 52], [204, 150, 92], [240, 210, 150]],
    air: [240, 180, 110],
    sea: None,
    clouds: Some(layer([226, 196, 150], 160)),
    caps: Some(layer([236, 226, 210], 70)),
    lights: Some(layer([255, 200, 120], 70)),
    glow: None,
    bands: false,
    ring: None,
    detail: 2,
};

/// The volcanic world: black basalt split by glowing lava, ash in the sky.
pub const VOLCANIC_LOOK: OrbitalLook = OrbitalLook {
    land: [[30, 24, 28], [72, 40, 40], [118, 92, 86]],
    air: [210, 100, 60],
    sea: None,
    clouds: Some(layer([96, 88, 92], 300)),
    caps: None,
    lights: None,
    glow: Some(layer([255, 120, 40], 320)),
    bands: false,
    ring: None,
    detail: 3,
};

/// The crystal world: violet plains veined with lit crystal, a pink haze.
pub const CRYSTAL_LOOK: OrbitalLook = OrbitalLook {
    land: [[62, 40, 92], [120, 80, 160], [196, 168, 232]],
    air: [240, 150, 220],
    sea: None,
    clouds: Some(layer([250, 220, 250], 140)),
    caps: Some(layer([236, 230, 255], 160)),
    lights: Some(layer([190, 255, 250], 90)),
    glow: Some(layer([140, 255, 240], 220)),
    bands: false,
    ring: Some(RingLook {
        colours: [[200, 180, 240], [150, 120, 200]],
        inner_permille: 0,
        outer_permille: 0,
    }),
    detail: 3,
};

/// The registered looks by theme id (append `(YOUR_THEME, YOUR_LOOK)` at the end; ids 9-14
/// are the Wave 13 themes').
pub static ORBITAL_LOOKS: &[(PlanetThemeId, OrbitalLook)] = &[
    (PlanetThemeId::JUNGLE, JUNGLE_LOOK),
    (PlanetThemeId::OCEAN, OCEAN_LOOK),
    (PlanetThemeId::GLACIER, GLACIER_LOOK),
    (crate::frontier::DUNE_WORLD, DUNE_LOOK),
    (crate::frontier::VOLCANIC_WORLD, VOLCANIC_LOOK),
    (crate::frontier::CRYSTAL_WORLD, CRYSTAL_LOOK),
];

/// The registered look of theme `id`, if it has one.
#[must_use]
pub fn registered(id: PlanetThemeId) -> Option<OrbitalLook> {
    ORBITAL_LOOKS
        .iter()
        .find(|(theme, _)| *theme == id)
        .map(|(_, look)| *look)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wave_12_theme_has_a_look_and_the_placeholders_fall_back() {
        use crate::planet::hooks::hooks;
        for id in [
            PlanetThemeId::JUNGLE,
            PlanetThemeId::OCEAN,
            PlanetThemeId::GLACIER,
            crate::frontier::DUNE_WORLD,
            crate::frontier::VOLCANIC_WORLD,
            crate::frontier::CRYSTAL_WORLD,
        ] {
            let look = hooks(id).orbital_look().expect("a look");
            assert!((1..=4).contains(&look.detail));
            for layer in [look.sea, look.clouds, look.caps, look.lights, look.glow]
                .into_iter()
                .flatten()
            {
                assert!(layer.permille <= 1_000);
            }
        }
        for id in [
            PlanetThemeId::BARREN,
            PlanetThemeId::ROCKY,
            PlanetThemeId::ICE,
        ] {
            assert!(hooks(id).orbital_look().is_none());
        }
        // One look a theme.
        let mut ids: Vec<u16> = ORBITAL_LOOKS.iter().map(|(id, _)| id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ORBITAL_LOOKS.len());
    }
}
