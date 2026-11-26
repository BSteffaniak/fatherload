//! How a theme's worlds look from orbit (Wave 13 art-audio).
//!
//! Plain presentation data the game's planet shader draws: land in three tones, seas, cloud
//! decks, polar caps, the night side's city lights, glowing cracks, weather bands and rings.
//! Nothing here touches generation or play.
//!
//! A theme gives its look through [`super::hooks::PlanetThemeHooks::orbital_look`], whose
//! default reads [`ORBITAL_LOOKS`] by the theme's id: a theme track either appends
//! `(MY_THEME, MY_LOOK)` there or overrides the hook. All twelve Part III themes are
//! registered; a theme with no look (the Wave 11 placeholders) is drawn from the space
//! generator's tag instead (the game's preset per tag).

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

/// The fungal world: plum and rose caps under a spore-pink haze, glowcap light in the folds.
pub const FUNGAL_LOOK: OrbitalLook = OrbitalLook {
    land: [[70, 36, 74], [150, 78, 120], [214, 150, 170]],
    air: [220, 150, 230],
    sea: None,
    clouds: Some(layer([246, 190, 226], 320)),
    caps: None,
    lights: Some(layer([200, 170, 255], 60)),
    glow: Some(layer([150, 255, 190], 200)),
    bands: false,
    ring: None,
    detail: 3,
};

/// The machine world: plated steel in a fine grid, its seams lit teal, no weather to speak of.
pub const MACHINE_LOOK: OrbitalLook = OrbitalLook {
    land: [[40, 46, 58], [92, 104, 120], [160, 172, 186]],
    air: [110, 220, 220],
    sea: None,
    clouds: None,
    caps: None,
    lights: Some(layer([90, 240, 230], 550)),
    glow: Some(layer([255, 170, 60], 90)),
    bands: false,
    ring: None,
    detail: 4,
};

/// The dead world: ash plains and grey dust, the faint embers of the Hollow Throne's ruins.
pub const DEAD_LOOK: OrbitalLook = OrbitalLook {
    land: [[44, 38, 40], [92, 80, 74], [150, 142, 136]],
    air: [150, 150, 160],
    sea: None,
    clouds: Some(layer([170, 166, 170], 140)),
    caps: Some(layer([200, 200, 206], 80)),
    lights: Some(layer([255, 170, 90], 25)),
    glow: Some(layer([120, 220, 210], 40)),
    bands: false,
    ring: None,
    detail: 3,
};

/// The gas giant: amber and cream weather bands, the floating cities' lights on the night
/// side, faint dusty rings.
pub const GAS_GIANT_LOOK: OrbitalLook = OrbitalLook {
    land: [[150, 92, 50], [214, 160, 90], [244, 226, 190]],
    air: [250, 200, 140],
    sea: None,
    clouds: Some(layer([252, 240, 220], 200)),
    caps: None,
    lights: Some(layer([255, 214, 140], 60)),
    glow: None,
    bands: true,
    ring: Some(RingLook {
        colours: [[226, 200, 150], [170, 120, 80]],
        inner_permille: 1_400,
        outer_permille: 1_950,
    }),
    detail: 2,
};

/// The tidally locked world: a scorched day side of salt pans and glass, a frozen night of
/// nitrogen ice (the caps), a thin twilight of settlements between.
pub const LOCKED_LOOK: OrbitalLook = OrbitalLook {
    land: [[84, 48, 36], [186, 120, 72], [244, 220, 186]],
    air: [255, 190, 130],
    sea: None,
    clouds: Some(layer([236, 236, 246], 90)),
    caps: Some(layer([200, 226, 250], 420)),
    lights: Some(layer([255, 210, 150], 40)),
    glow: Some(layer([255, 150, 70], 70)),
    bands: false,
    ring: None,
    detail: 2,
};

/// The ring world: grey breccia and pale ring dust under its great ring of ice, the Tether's
/// conduits lit along the ground.
pub const RING_LOOK: OrbitalLook = OrbitalLook {
    land: [[60, 62, 72], [118, 120, 132], [196, 200, 210]],
    air: [180, 200, 240],
    sea: None,
    clouds: Some(layer([236, 240, 250], 80)),
    caps: Some(layer([226, 236, 250], 120)),
    lights: Some(layer([140, 220, 255], 120)),
    glow: Some(layer([120, 200, 255], 60)),
    bands: false,
    ring: Some(RingLook {
        colours: [[230, 236, 250], [150, 160, 190]],
        inner_permille: 1_300,
        outer_permille: 2_100,
    }),
    detail: 3,
};

/// The registered looks by theme id (append `(YOUR_THEME, YOUR_LOOK)` at the end).
pub static ORBITAL_LOOKS: &[(PlanetThemeId, OrbitalLook)] = &[
    (PlanetThemeId::JUNGLE, JUNGLE_LOOK),
    (PlanetThemeId::OCEAN, OCEAN_LOOK),
    (PlanetThemeId::GLACIER, GLACIER_LOOK),
    (crate::frontier::DUNE_WORLD, DUNE_LOOK),
    (crate::frontier::VOLCANIC_WORLD, VOLCANIC_LOOK),
    (crate::frontier::CRYSTAL_WORLD, CRYSTAL_LOOK),
    // Wave 13 integration: the outland and exotic worlds.
    (PlanetThemeId::FUNGAL, FUNGAL_LOOK),
    (PlanetThemeId::MACHINE, MACHINE_LOOK),
    (PlanetThemeId::DEAD, DEAD_LOOK),
    (crate::exotic::GAS_GIANT, GAS_GIANT_LOOK),
    (crate::exotic::LOCKED_WORLD, LOCKED_LOOK),
    (crate::exotic::RING_WORLD, RING_LOOK),
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
    fn every_part_iii_theme_has_a_look_and_the_placeholders_fall_back() {
        use crate::planet::hooks::hooks;
        for id in [
            PlanetThemeId::JUNGLE,
            PlanetThemeId::OCEAN,
            PlanetThemeId::GLACIER,
            crate::frontier::DUNE_WORLD,
            crate::frontier::VOLCANIC_WORLD,
            crate::frontier::CRYSTAL_WORLD,
            PlanetThemeId::FUNGAL,
            PlanetThemeId::MACHINE,
            PlanetThemeId::DEAD,
            crate::exotic::GAS_GIANT,
            crate::exotic::LOCKED_WORLD,
            crate::exotic::RING_WORLD,
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
