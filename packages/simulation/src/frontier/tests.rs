//! Frontier world generation and rules tests (Wave 12 planets-b).

use super::*;
use crate::{
    planet::{BodyGround, PlanetParams},
    terrain::TileKind,
};

fn ground(style: FrontierStyle, radius: u32) -> PlanetGround {
    PlanetGround::new(PlanetParams::new(0x00F0_0D5E, radius, style.theme()))
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::Lava => '~',
        TileKind::Fluid(_) => '=',
        TileKind::Gas => 'g',
        TileKind::Ore(_) => '$',
        TileKind::Bedrock => '#',
        TileKind::Frontier(tile) => match tile {
            FrontierTile::DuneSand | FrontierTile::Ash | FrontierTile::QuartzSoil => '.',
            FrontierTile::PackedSand | FrontierTile::Pumice | FrontierTile::ShardStone => ':',
            FrontierTile::BandedSandstone | FrontierTile::Basalt | FrontierTile::PrismaticStone => {
                '-'
            }
            FrontierTile::RuinBrick => 'B',
            FrontierTile::RuinFloor => '_',
            FrontierTile::RuinCache | FrontierTile::MagmaHeart | FrontierTile::GeodeCore => '*',
            FrontierTile::DesertGlass | FrontierTile::Crystal => 'c',
            FrontierTile::SpireCrystal => '|',
            FrontierTile::Wormstone | FrontierTile::TubeCrust | FrontierTile::GeodeShell => 'o',
            FrontierTile::PalmTrunk => 'I',
            FrontierTile::PalmFrond => 'T',
            FrontierTile::SunGlyph | FrontierTile::SunLens => '@',
            FrontierTile::Obsidian => 'X',
            FrontierTile::MagmaRock => 'm',
            FrontierTile::PrismCw | FrontierTile::PrismCcw => '/',
            FrontierTile::Receptor | FrontierTile::ReceptorLit => 'R',
            FrontierTile::VaultSeal => '%',
            _ => '&',
        },
        _ => '?',
    }
}

#[test]
#[ignore = "a picture for eyes"]
fn dump() {
    for style in FrontierStyle::ALL {
        let g = ground(style, 1_000);
        let lm = landmark(&g).unwrap();
        let settle = settlement_site(&g).unwrap();
        println!(
            "{style:?} landmark {lm:?} settle {settle:?} strata {}",
            strata_rows(1_000)
        );
        for (label, x0) in [
            ("landmark", lm.column - 60),
            ("settle", settle.column - 20),
            ("any", 900),
        ] {
            println!("-- {label}");
            for y in g.strip_top()..60 {
                let line: String = (x0..x0 + 130)
                    .map(|x| glyph(g.generated_tile(x, y).kind))
                    .collect();
                println!("{y:5} {line}");
            }
        }
    }
}
