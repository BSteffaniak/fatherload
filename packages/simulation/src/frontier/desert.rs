//! The dune world (desert dunes theme): rolling dunes with steep slip faces, oases ringed by
//! palms, the Sleeping Sentinel half buried in the sand, and four strata.
//!
//! | Stratum | Rock | Signature rule | Ores |
//! |---|---|---|---|
//! | Shifting Dunes | packed sand, loose dune sand | [`StratumRule::LooseSand`] | copper, iron, gold |
//! | Buried City | banded sandstone, ruins with cracked floors over caches | [`StratumRule::CrumblingFloors`] | silver, gold |
//! | Worm Galleries | wormstone, the wyrms' tunnels | [`StratumRule::EchoingGalleries`] | silver, emerald, ruby |
//! | Glass Deep | salt beds, desert glass | [`StratumRule::BrittleGlass`] | diamond, platinum, uranium |

use super::{FrontierTile, StratumRule};
use crate::{
    planet::{PlanetPalette, PlanetStratum, PlanetTheme},
    terrain::{MineralKind, TileKind},
};

const fn tile(tile: FrontierTile) -> TileKind {
    TileKind::Frontier(tile)
}

/// The dune world's strata, top to bottom (shares of the authored depth; the last runs on to
/// the core chamber).
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Shifting Dunes",
        share_permille: 250,
        rock: tile(FrontierTile::PackedSand),
        accent: tile(FrontierTile::DuneSand),
        accent_permille: 320,
        ores: &[
            (MineralKind::Copper, 110),
            (MineralKind::Iron, 70),
            (MineralKind::Gold, 18),
        ],
    },
    PlanetStratum {
        name: "Buried City",
        share_permille: 300,
        rock: tile(FrontierTile::BandedSandstone),
        accent: tile(FrontierTile::PackedSand),
        accent_permille: 160,
        ores: &[(MineralKind::Silver, 55), (MineralKind::Gold, 40)],
    },
    PlanetStratum {
        name: "Worm Galleries",
        share_permille: 250,
        rock: tile(FrontierTile::Wormstone),
        accent: tile(FrontierTile::BandedSandstone),
        accent_permille: 200,
        ores: &[
            (MineralKind::Silver, 50),
            (MineralKind::Emerald, 28),
            (MineralKind::Ruby, 12),
        ],
    },
    PlanetStratum {
        name: "Glass Deep",
        share_permille: 200,
        rock: tile(FrontierTile::SaltBed),
        accent: tile(FrontierTile::DesertGlass),
        accent_permille: 380,
        ores: &[
            (MineralKind::Diamond, 26),
            (MineralKind::Platinum, 30),
            (MineralKind::Uranium, 10),
        ],
    },
];

/// The strata's signature rules, by index.
pub const RULES: [StratumRule; 4] = [
    StratumRule::LooseSand,
    StratumRule::CrumblingFloors,
    StratumRule::EchoingGalleries,
    StratumRule::BrittleGlass,
];

/// The dune world's theme (registered in [`crate::planet::theme::PLANET_THEMES`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::DUNE_WORLD,
    name: "Dune World",
    palette: PlanetPalette {
        sky: [236, 178, 112],
        atmosphere: [255, 196, 120],
        surface: [226, 170, 98],
        rock: [176, 120, 72],
        deep: [204, 214, 220],
        core: [255, 220, 140],
    },
    crust: tile(FrontierTile::DuneSand),
    crust_rows: 4,
    strata: &STRATA,
    core_ore: TileKind::Ore(MineralKind::Gold),
    caves_per_256_columns: 1,
    hill_permille: 40,
};
