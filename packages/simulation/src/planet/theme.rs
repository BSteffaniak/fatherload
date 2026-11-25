//! The planet theme registry (Wave 11 placeholder themes; Waves 12-13 add the real ones).
//!
//! A theme is plain data: a palette for the renderer, a crust, two or three strata with their
//! rock and ores, a core ore and how cavernous the ground is. [`PLANET_THEMES`] is append-only:
//! a theme's [`PlanetThemeId`] is its index and is saved with every planet, so new themes go at
//! the end and existing ones never move.

use serde::{Deserialize, Serialize};

use crate::{
    moon::LunarTile,
    terrain::{MineralKind, SurfaceMaterial, TileKind},
};

/// A theme's id: its index in [`PLANET_THEMES`] (saved, replicated).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PlanetThemeId(pub u16);

impl PlanetThemeId {
    /// A grey, airless moon of regolith and rock.
    pub const BARREN: Self = Self(0);
    /// A brown rocky world of dirt, stone and hard rock.
    pub const ROCKY: Self = Self(1);
    /// A frozen world of snow and granite.
    pub const ICE: Self = Self(2);
    /// A jungle world: towering canopy, roots, spore caves and heartwood (Wave 12 planets-a,
    /// [`crate::planet::wilds::jungle`]).
    pub const JUNGLE: Self = Self(3);
    /// An ocean archipelago: islands over a planet-wide sea, reefs, drowned ruins and the
    /// trench (Wave 12 planets-a, [`crate::planet::wilds::ocean`]).
    pub const OCEAN: Self = Self(4);
    /// A glacier world: ice sheets, crevasses, frozen seas and permafrost caves (Wave 12
    /// planets-a, [`crate::planet::wilds::glacier`]; the space generator's ice tag).
    pub const GLACIER: Self = Self(5);
}

/// Colours the renderer draws a theme's world with (sRGB).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetPalette {
    /// The sky seen from the ground.
    pub sky: [u8; 3],
    /// The atmosphere's glow seen from space (a dead world's is faint).
    pub atmosphere: [u8; 3],
    /// The surface crust.
    pub surface: [u8; 3],
    /// The body's rock.
    pub rock: [u8; 3],
    /// The deep rock near the core.
    pub deep: [u8; 3],
    /// The core chamber's glow.
    pub core: [u8; 3],
}

/// One stratum of a theme: a concentric ring band of the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanetStratum {
    pub name: &'static str,
    /// Share of the depth from the surface to the core chamber (per mille; the last stratum
    /// takes whatever is left).
    pub share_permille: u16,
    /// The stratum's common rock.
    pub rock: TileKind,
    /// A contrasting rock in patches, and how much of the stratum it takes (per mille).
    pub accent: TileKind,
    pub accent_permille: u16,
    /// Ores and their chance per tile (per ten thousand).
    pub ores: &'static [(MineralKind, u16)],
}

/// A planet theme (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanetTheme {
    pub id: PlanetThemeId,
    pub name: &'static str,
    pub palette: PlanetPalette,
    /// The top rows of ground.
    pub crust: TileKind,
    pub crust_rows: i32,
    pub strata: &'static [PlanetStratum],
    /// The ore lining the core chamber's floor.
    pub core_ore: TileKind,
    /// How many caves per 256 surface columns.
    pub caves_per_256_columns: u16,
    /// Hill height as a share of the radius (per mille), capped by the generator.
    pub hill_permille: u16,
}

const BARREN_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Dust Plains",
        share_permille: 250,
        rock: TileKind::Lunar(LunarTile::Rock),
        accent: TileKind::Lunar(LunarTile::MareDust),
        accent_permille: 180,
        ores: &[(MineralKind::Iron, 90), (MineralKind::Silver, 30)],
    },
    PlanetStratum {
        name: "Basalt Mantle",
        share_permille: 450,
        rock: TileKind::Lunar(LunarTile::Basalt),
        accent: TileKind::Lunar(LunarTile::Rock),
        accent_permille: 250,
        ores: &[
            (MineralKind::Gold, 45),
            (MineralKind::Platinum, 20),
            (MineralKind::Emerald, 15),
        ],
    },
    PlanetStratum {
        name: "Deep Core Rock",
        share_permille: 300,
        rock: TileKind::Lunar(LunarTile::DeepRock),
        accent: TileKind::HardRock,
        accent_permille: 200,
        ores: &[(MineralKind::Diamond, 25), (MineralKind::Uranium, 12)],
    },
];

const ROCKY_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Red Soil",
        share_permille: 220,
        rock: TileKind::Dirt,
        accent: TileKind::Clay,
        accent_permille: 250,
        ores: &[(MineralKind::Copper, 110), (MineralKind::Iron, 70)],
    },
    PlanetStratum {
        name: "Stone Mantle",
        share_permille: 480,
        rock: TileKind::Stone,
        accent: TileKind::Surface(SurfaceMaterial::RedRock),
        accent_permille: 200,
        ores: &[
            (MineralKind::Silver, 50),
            (MineralKind::Gold, 35),
            (MineralKind::Ruby, 12),
        ],
    },
    PlanetStratum {
        name: "Iron Deep",
        share_permille: 300,
        rock: TileKind::HardRock,
        accent: TileKind::Stone,
        accent_permille: 150,
        ores: &[(MineralKind::Platinum, 30), (MineralKind::Mythril, 8)],
    },
];

const ICE_STRATA: [PlanetStratum; 2] = [
    PlanetStratum {
        name: "Glacier Shelf",
        share_permille: 400,
        rock: TileKind::Lunar(LunarTile::Ice),
        accent: TileKind::Surface(SurfaceMaterial::Snow),
        accent_permille: 200,
        ores: &[(MineralKind::Silver, 60), (MineralKind::Emerald, 20)],
    },
    PlanetStratum {
        name: "Frozen Granite",
        share_permille: 600,
        rock: TileKind::Surface(SurfaceMaterial::Granite),
        accent: TileKind::Lunar(LunarTile::Ice),
        accent_permille: 150,
        ores: &[(MineralKind::Diamond, 20), (MineralKind::Platinum, 25)],
    },
];

/// Every theme, by id (append only).
pub const PLANET_THEMES: [PlanetTheme; 9] = [
    PlanetTheme {
        id: PlanetThemeId::BARREN,
        name: "Barren Moon",
        palette: PlanetPalette {
            sky: [10, 12, 22],
            atmosphere: [90, 96, 120],
            surface: [168, 168, 160],
            rock: [112, 110, 108],
            deep: [70, 64, 72],
            core: [255, 170, 90],
        },
        crust: TileKind::Lunar(LunarTile::Regolith),
        crust_rows: 3,
        strata: &BARREN_STRATA,
        core_ore: TileKind::Ore(MineralKind::Platinum),
        caves_per_256_columns: 1,
        hill_permille: 20,
    },
    PlanetTheme {
        id: PlanetThemeId::ROCKY,
        name: "Rocky World",
        palette: PlanetPalette {
            sky: [150, 110, 90],
            atmosphere: [230, 150, 110],
            surface: [150, 92, 60],
            rock: [118, 96, 84],
            deep: [80, 60, 58],
            core: [255, 120, 60],
        },
        crust: TileKind::Surface(SurfaceMaterial::Sand),
        crust_rows: 2,
        strata: &ROCKY_STRATA,
        core_ore: TileKind::Ore(MineralKind::Ruby),
        caves_per_256_columns: 2,
        hill_permille: 28,
    },
    PlanetTheme {
        id: PlanetThemeId::ICE,
        name: "Ice World",
        palette: PlanetPalette {
            sky: [120, 160, 200],
            atmosphere: [170, 220, 255],
            surface: [236, 244, 250],
            rock: [150, 176, 196],
            deep: [80, 100, 130],
            core: [140, 220, 255],
        },
        crust: TileKind::Surface(SurfaceMaterial::Snow),
        crust_rows: 4,
        strata: &ICE_STRATA,
        core_ore: TileKind::Ore(MineralKind::Diamond),
        caves_per_256_columns: 1,
        hill_permille: 15,
    },
    // Wave 12 planets-a: the wild worlds.
    crate::planet::wilds::jungle::JUNGLE_THEME,
    crate::planet::wilds::ocean::OCEAN_THEME,
    crate::planet::wilds::glacier::GLACIER_THEME,
    // Wave 12 planets-b: the frontier worlds (`crate::frontier`).
    crate::frontier::desert::THEME,
    crate::frontier::volcanic::THEME,
    crate::frontier::crystal::THEME,
];

/// The theme with id `id` (an unknown id, from a newer build, reads as the first theme).
#[must_use]
pub fn theme(id: PlanetThemeId) -> &'static PlanetTheme {
    PLANET_THEMES
        .get(usize::from(id.0))
        .unwrap_or(&PLANET_THEMES[0])
}
