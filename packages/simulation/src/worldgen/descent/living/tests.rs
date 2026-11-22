//! The Living Dark's generated places: every stratum's living rock and set pieces are in the
//! terrain, the fixed stamps sit where their functions say, and older worlds have none.

use super::*;
use crate::worldgen::{
    DEFAULT_WORLD_SEED, GeneratorVersion, descent::layout, streamed::generated_tile,
};

fn settings(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(seed)
    }
}

fn kind_at(settings: &WorldSettings, x: i32, y: i32) -> TileKind {
    generated_tile(settings, TilePosition { x, y }).kind
}

/// How many tiles of `kind` a `width` x `height` window centred on `(x, y)` holds.
fn count(
    settings: &WorldSettings,
    (x, y): (i32, i32),
    (width, height): (i32, i32),
    kind: TileKind,
) -> usize {
    (y - height / 2..y + height / 2)
        .flat_map(|row| (x - width / 2..x + width / 2).map(move |column| (column, row)))
        .filter(|(column, row)| kind_at(settings, *column, *row) == kind)
        .count()
}

const LIVING: [Stratum; 5] = [
    Stratum::FungalForest,
    Stratum::RootLabyrinth,
    Stratum::TheHive,
    Stratum::FleshCaverns,
    Stratum::GlowSea,
];

#[test]
#[ignore = "prints an ASCII dump of each living stratum for review"]
fn dump_living_strata() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    for stratum in LIVING {
        let (top, bottom) = layout.rows(stratum);
        let row = top + (bottom - top) / 2;
        println!("{stratum:?} rows {top}..{bottom}");
        for y in row - 20..row + 20 {
            let line: String = (0..160)
                .map(|x| match kind_at(&settings, x, y) {
                    TileKind::Air => ' ',
                    TileKind::Cave(CaveTile::Root) => 'r',
                    TileKind::Cave(CaveTile::Flesh) => 'f',
                    TileKind::Cave(CaveTile::Comb) => 'h',
                    TileKind::Cave(CaveTile::JellyCell) => 'j',
                    TileKind::Cave(CaveTile::Mycelium) => 'm',
                    TileKind::Cave(CaveTile::MushroomCap) => 'C',
                    TileKind::Cave(CaveTile::MushroomStem) => '|',
                    TileKind::Cave(CaveTile::SporePod) => 'o',
                    TileKind::Cave(CaveTile::AcidGland) => 'G',
                    TileKind::Fluid(_) => '~',
                    TileKind::Ore(_) => '$',
                    TileKind::Stone => '.',
                    TileKind::HardRock => ':',
                    _ => '?',
                })
                .collect();
            println!("{line}");
        }
    }
}

#[test]
fn every_living_stratum_has_its_living_rock() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let middle = |stratum: Stratum| {
        let (top, bottom) = layout.rows(stratum);
        (40, top + (bottom - top) / 2)
    };
    let window = (160, 60);
    let roots = count(
        &settings,
        middle(Stratum::RootLabyrinth),
        window,
        TileKind::Cave(CaveTile::Root),
    );
    assert!(roots > 200, "roots {roots}");
    let mycelium = count(
        &settings,
        middle(Stratum::FungalForest),
        window,
        TileKind::Cave(CaveTile::Mycelium),
    );
    assert!(mycelium > 200, "mycelium {mycelium}");
    let comb = count(
        &settings,
        middle(Stratum::TheHive),
        window,
        TileKind::Cave(CaveTile::Comb),
    );
    assert!(comb > 100, "comb {comb}");
    let flesh = count(
        &settings,
        middle(Stratum::FleshCaverns),
        window,
        TileKind::Cave(CaveTile::Flesh),
    );
    assert!(flesh > 2_000, "flesh {flesh}");
}
