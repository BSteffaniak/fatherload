//! The volcanic world: basalt islands in lava seas under smoking cones (the biggest is the
//! Caldera, a lava lake in its crater), and four strata.
//!
//! | Stratum | Rock | Signature rule | Ores |
//! |---|---|---|---|
//! | Ash Fields | pumice, loose ash, gas pockets | [`StratumRule::ToxicAsh`] | copper, iron |
//! | Basalt Flows | flow basalt, scoria, lava tubes | [`StratumRule::TubeDraughts`] | silver, gold |
//! | Obsidian Mantle | basalt, obsidian veins | [`StratumRule::Splinters`] | ruby, gold, platinum |
//! | Magma Chambers | magma rock, lava pools, magma hearts | [`StratumRule::SearingHeat`] | diamond, uranium, mythril |

use super::{FrontierTile, StratumRule};
use crate::{
    planet::{PlanetPalette, PlanetStratum, PlanetTheme},
    terrain::{MineralKind, TileKind},
};

const fn tile(tile: FrontierTile) -> TileKind {
    TileKind::Frontier(tile)
}

/// The volcanic world's strata, top to bottom.
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Ash Fields",
        share_permille: 250,
        rock: tile(FrontierTile::Pumice),
        accent: tile(FrontierTile::Ash),
        accent_permille: 300,
        ores: &[(MineralKind::Copper, 90), (MineralKind::Iron, 100)],
    },
    PlanetStratum {
        name: "Basalt Flows",
        share_permille: 300,
        rock: tile(FrontierTile::Basalt),
        accent: tile(FrontierTile::Scoria),
        accent_permille: 220,
        ores: &[(MineralKind::Silver, 45), (MineralKind::Gold, 40)],
    },
    PlanetStratum {
        name: "Obsidian Mantle",
        share_permille: 250,
        rock: tile(FrontierTile::Basalt),
        accent: tile(FrontierTile::Obsidian),
        accent_permille: 340,
        ores: &[
            (MineralKind::Ruby, 40),
            (MineralKind::Gold, 30),
            (MineralKind::Platinum, 25),
        ],
    },
    PlanetStratum {
        name: "Magma Chambers",
        share_permille: 200,
        rock: tile(FrontierTile::MagmaRock),
        accent: tile(FrontierTile::Basalt),
        accent_permille: 160,
        ores: &[
            (MineralKind::Diamond, 25),
            (MineralKind::Uranium, 20),
            (MineralKind::Mythril, 10),
        ],
    },
];

/// The strata's signature rules, by index.
pub const RULES: [StratumRule; 4] = [
    StratumRule::ToxicAsh,
    StratumRule::TubeDraughts,
    StratumRule::Splinters,
    StratumRule::SearingHeat,
];

/// The volcanic world's theme (registered in [`crate::planet::theme::PLANET_THEMES`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::VOLCANIC_WORLD,
    name: "Volcanic World",
    palette: PlanetPalette {
        sky: [92, 44, 40],
        atmosphere: [255, 110, 50],
        surface: [70, 64, 70],
        rock: [58, 48, 52],
        deep: [140, 40, 24],
        core: [255, 140, 40],
    },
    crust: tile(FrontierTile::Ash),
    crust_rows: 3,
    strata: &STRATA,
    core_ore: TileKind::Frontier(FrontierTile::MagmaHeart),
    caves_per_256_columns: 2,
    hill_permille: 60,
};
