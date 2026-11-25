//! The crystal world: gentle quartz hills bristling with crystal spires (the tallest is the
//! Choir Spire), light wells down to sealed geode vaults, and four strata.
//!
//! | Stratum | Rock | Signature rule | Ores |
//! |---|---|---|---|
//! | Shard Meadows | shard stone, resonant crystal | [`StratumRule::Resonance`] | silver, emerald |
//! | Prism Beds | prismatic stone, prisms, light wells, sealed vaults | [`StratumRule::LightBeams`] | gold, ruby |
//! | Growing Geodes | shard stone, geodes lined with crystal | [`StratumRule::Regrowth`] | diamond, platinum |
//! | Heavy Lattice | dense lattice, crystal | [`StratumRule::HeavyLattice`] | mythril, diamond |

use super::{FrontierTile, StratumRule};
use crate::{
    planet::{PlanetPalette, PlanetStratum, PlanetTheme},
    terrain::{MineralKind, TileKind},
};

const fn tile(tile: FrontierTile) -> TileKind {
    TileKind::Frontier(tile)
}

/// The crystal world's strata, top to bottom.
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Shard Meadows",
        share_permille: 250,
        rock: tile(FrontierTile::ShardStone),
        accent: tile(FrontierTile::Crystal),
        accent_permille: 240,
        ores: &[(MineralKind::Silver, 60), (MineralKind::Emerald, 30)],
    },
    PlanetStratum {
        name: "Prism Beds",
        share_permille: 300,
        rock: tile(FrontierTile::PrismaticStone),
        accent: tile(FrontierTile::ShardStone),
        accent_permille: 160,
        ores: &[(MineralKind::Gold, 40), (MineralKind::Ruby, 22)],
    },
    PlanetStratum {
        name: "Growing Geodes",
        share_permille: 250,
        rock: tile(FrontierTile::ShardStone),
        accent: tile(FrontierTile::PrismaticStone),
        accent_permille: 150,
        ores: &[(MineralKind::Diamond, 28), (MineralKind::Platinum, 30)],
    },
    PlanetStratum {
        name: "Heavy Lattice",
        share_permille: 200,
        rock: tile(FrontierTile::DenseLattice),
        accent: tile(FrontierTile::Crystal),
        accent_permille: 180,
        ores: &[(MineralKind::Mythril, 14), (MineralKind::Diamond, 34)],
    },
];

/// The strata's signature rules, by index.
pub const RULES: [StratumRule; 4] = [
    StratumRule::Resonance,
    StratumRule::LightBeams,
    StratumRule::Regrowth,
    StratumRule::HeavyLattice,
];

/// The crystal world's theme (registered in [`crate::planet::theme::PLANET_THEMES`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::CRYSTAL_WORLD,
    name: "Crystal World",
    palette: PlanetPalette {
        sky: [70, 60, 130],
        atmosphere: [180, 150, 255],
        surface: [210, 200, 236],
        rock: [140, 120, 190],
        deep: [80, 60, 140],
        core: [200, 255, 250],
    },
    crust: tile(FrontierTile::QuartzSoil),
    crust_rows: 3,
    strata: &STRATA,
    core_ore: TileKind::Frontier(FrontierTile::GeodeCore),
    caves_per_256_columns: 1,
    hill_permille: 20,
};
