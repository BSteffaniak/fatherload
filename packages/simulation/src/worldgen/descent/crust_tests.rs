//! Generator V11's authored Crust (Wave 8): every stratum carries its signature feature, the
//! landmarks are where they say they are, and older generators are untouched.

use super::{crust::*, layout};
use crate::{
    crust::{self as rules, CrustTile},
    descent::Stratum,
    terrain::{ArtifactKind, SurfaceMaterial, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings, streamed::generated_tile},
};

#[allow(
    clippy::unnecessary_wraps,
    reason = "reads like the terrain's own lookups"
)]
fn generated(settings: &WorldSettings, at: TilePosition) -> Option<crate::terrain::Tile> {
    Some(generated_tile(settings, at))
}

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// Counts the tiles of each kind in a window of a stratum's middle rows.
fn census(settings: &WorldSettings, stratum: Stratum, columns: i32) -> Vec<(TileKind, u32)> {
    let layout = layout(settings);
    let (top, bottom) = layout.rows(stratum);
    let mut counts: std::collections::BTreeMap<u64, (TileKind, u32)> =
        std::collections::BTreeMap::new();
    for y in (top + 20..bottom - 20).step_by(2) {
        for x in (-columns..columns).step_by(2) {
            if let Some(tile) = generated(settings, TilePosition { x, y }) {
                let entry = counts
                    .entry(
                        crate::terrain::Tile {
                            durability: 0,
                            ..tile
                        }
                        .code(),
                    )
                    .or_insert((tile.kind, 0));
                entry.1 += 1;
            }
        }
    }
    counts.into_values().collect()
}

fn count(census: &[(TileKind, u32)], wanted: impl Fn(TileKind) -> bool) -> u32 {
    census
        .iter()
        .filter(|(kind, _)| wanted(*kind))
        .map(|(_, count)| count)
        .sum()
}

#[test]
fn every_crust_stratum_carries_its_signature() {
    let settings = v11(3);
    let topsoil = census(&settings, Stratum::Topsoil, 300);
    assert!(
        count(&topsoil, |kind| kind
            == TileKind::Surface(SurfaceMaterial::Mud))
            > 20
    );
    let roots = (8..20)
        .flat_map(|y| (-400..400).map(move |x| TilePosition { x, y }))
        .filter(|at| {
            generated(&settings, *at)
                .is_some_and(|tile| tile.kind == TileKind::Crust(CrustTile::RootMat))
        })
        .count();
    assert!(roots > 20, "root mats under the surface: {roots}");
    let clay = census(&settings, Stratum::ClayBeds, 400);
    assert!(
        count(&clay, |kind| kind
            == TileKind::Artifact(ArtifactKind::ClayPot))
            > 5,
        "clay pots"
    );
    assert!(
        count(&clay, |kind| matches!(kind, TileKind::Fluid(_))) > 20,
        "aquifers"
    );
    let stone = census(&settings, Stratum::GreyStone, 600);
    assert!(count(&stone, |kind| kind == TileKind::Crust(CrustTile::Rail)) > 20);
    assert!(count(&stone, |kind| kind == TileKind::Crust(CrustTile::Timber)) > 20);
    let slate = census(&settings, Stratum::SlateShelves, 400);
    assert!(count(&slate, |kind| kind == TileKind::Crust(CrustTile::Slate)) > 100);
    let basalt = census(&settings, Stratum::BasaltColumns, 600);
    assert!(
        count(&basalt, |kind| kind
            == TileKind::Crust(CrustTile::BasaltColumn))
            > 200
    );
    assert!(
        count(&basalt, |kind| kind
            == TileKind::Crust(CrustTile::TubeGlass))
            > 20
    );
}

#[test]
fn mine_works_lay_rails_under_timber_and_fork_at_a_junction() {
    let settings = v11(5);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::GreyStone);
    let works = (-6..6)
        .flat_map(|rx| (0..40).map(move |ry| (rx, ry)))
        .find_map(|(rx, ry)| mine_works(settings.seed, top, bottom, rx, ry))
        .expect("a works");
    let rail = |x: i32, y: i32| works.tile(x, y) == Some(TileKind::Crust(CrustTile::Rail));
    // A junction: level and lower rails both follow the junction column.
    assert!(rail(works.junction + 1, works.rail_row));
    assert!(rail(works.junction + 1, works.rail_row + 1));
    assert!(rules::is_junction(
        |at| rail(at.x, at.y),
        TilePosition {
            x: works.junction,
            y: works.rail_row
        },
        1
    ));
    // Timber over the gallery's air, rail under it.
    let x = works.first + 2;
    assert_eq!(works.tile(x, works.rail_row - 1), Some(TileKind::Air));
    assert!(matches!(
        works.tile(x, works.rail_row - 4),
        Some(TileKind::Crust(CrustTile::Timber) | TileKind::Air)
    ));
}

#[test]
fn the_cabin_and_the_sinkhole_are_stamped() {
    let settings = v11(9);
    let cabin = rules::cabin(&settings).expect("a cabin");
    let wall = generated(&settings, cabin.min).map(|tile| tile.kind);
    assert_eq!(wall, Some(TileKind::Crust(CrustTile::CabinLog)));
    let box_tile = TilePosition {
        x: cabin.max.x - 2,
        y: cabin.max.y - 1,
    };
    assert_eq!(
        generated(&settings, box_tile).map(|tile| tile.kind),
        Some(TileKind::Crust(CrustTile::Strongbox))
    );
    let site = rules::sinkhole_site(&settings).expect("a sinkhole");
    assert_eq!(
        generated(&settings, site.core).map(|tile| tile.kind),
        Some(TileKind::Crust(CrustTile::SinkholeCore))
    );
    // The funnel is open from the chamber up to the choked lip.
    let mid = TilePosition {
        x: site.core.x,
        y: site.chamber.min.y - 20,
    };
    assert!(generated(&settings, mid).is_some_and(|tile| tile.kind.is_open()));
    assert_eq!(
        crate::worldgen::descent::stratum_at(&settings, site.core.x, site.core.y),
        Stratum::BasaltColumns
    );
}

#[test]
fn lava_tubes_are_long_open_tunnels_in_the_basalt() {
    let settings = v11(11);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::BasaltColumns);
    let mut longest = 0;
    for y in top..bottom {
        let mut run = 0;
        for x in -400..400 {
            if rules::lava_tube_at(&settings, x, y) {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
    }
    assert!(longest >= 60, "the longest tube row runs {longest} columns");
}

#[test]
fn older_generators_have_no_crust_tiles() {
    for generator in [
        GeneratorVersion::V10,
        GeneratorVersion::V9,
        GeneratorVersion::V6,
    ] {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(3)
        };
        assert!(rules::cabin(&settings).is_none());
        assert!(rules::sinkhole_site(&settings).is_none());
        for y in (10..200).step_by(3) {
            for x in (-200..200).step_by(3) {
                let kind = generated(&settings, TilePosition { x, y }).map(|tile| tile.kind);
                assert!(
                    !matches!(kind, Some(TileKind::Crust(_))),
                    "{generator:?} {x},{y}"
                );
            }
        }
    }
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::Dirt => '.',
        TileKind::Clay => ',',
        TileKind::Stone => ':',
        TileKind::HardRock => '#',
        TileKind::Ore(_) => 'o',
        TileKind::Artifact(_) => 'A',
        TileKind::Fluid(_) => '~',
        TileKind::Lava => 'L',
        TileKind::Gas => 'g',
        TileKind::Surface(SurfaceMaterial::Mud) => 'm',
        TileKind::Cave(crate::cave::CaveTile::Stalactite) => 'v',
        TileKind::Crust(tile) => match tile {
            CrustTile::RootMat => 'r',
            CrustTile::Timber => 'T',
            CrustTile::Rail => '=',
            CrustTile::Slate => '-',
            CrustTile::BasaltColumn => '|',
            CrustTile::TubeGlass => 'G',
            CrustTile::CabinLog => 'H',
            CrustTile::Strongbox => 'S',
            CrustTile::SinkholeCore => '@',
            CrustTile::Rubble => '%',
            CrustTile::Gravel => '_',
        },
        _ => '?',
    }
}

fn dump(settings: &WorldSettings, x0: i32, y0: i32, width: i32, height: i32) {
    for y in y0..y0 + height {
        let line: String = (x0..x0 + width)
            .map(|x| {
                generated(settings, TilePosition { x, y }).map_or('X', |tile| glyph(tile.kind))
            })
            .collect();
        println!("{y:5} {line}");
    }
}

#[test]
#[ignore = "prints maps of the Crust's features"]
fn print_crust() {
    let settings = v11(crate::worldgen::DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    for stratum in Stratum::ALL.into_iter().take(5) {
        let (top, bottom) = layout.rows(stratum);
        println!("{stratum:?} {top}..{bottom}");
    }
    if let Some(cabin) = rules::cabin(&settings) {
        println!("cabin {cabin:?}");
        dump(&settings, cabin.min.x - 20, 4, 60, cabin.max.y + 6);
    }
    let (top, bottom) = layout.rows(Stratum::GreyStone);
    if let Some(works) = (-4..4)
        .flat_map(|rx| (0..40).map(move |ry| (rx, ry)))
        .find_map(|(rx, ry)| mine_works(settings.seed, top, bottom, rx, ry))
    {
        println!("works {works:?}");
        dump(&settings, works.first - 4, works.rail_row - 10, 130, 26);
    }
    let (top, _) = layout.rows(Stratum::SlateShelves);
    dump(&settings, -60, top + 40, 120, 40);
    let (top, _) = layout.rows(Stratum::BasaltColumns);
    dump(&settings, -100, top + 10, 160, 40);
    if let Some(site) = rules::sinkhole_site(&settings) {
        println!("sinkhole {site:?}");
        dump(
            &settings,
            site.chamber.min.x - 12,
            site.lip_row - 4,
            site.chamber.max.x - site.chamber.min.x + 24,
            site.chamber.max.y - site.lip_row + 8,
        );
    }
    let (top, _) = layout.rows(Stratum::ClayBeds);
    dump(&settings, -60, top + 30, 120, 30);
}

/// Share of solid tiles that are ore, and their mean value, over a stratum's rows.
fn ore_share(settings: &WorldSettings, top: i32, bottom: i32) -> (f64, f64) {
    let (mut solid, mut ore, mut value) = (0_u32, 0_u32, 0_u64);
    for y in (top + 8..bottom - 8).step_by(3) {
        for x in (-500..500).step_by(3) {
            let kind = generated_tile(settings, TilePosition { x, y }).kind;
            if kind.is_open() {
                continue;
            }
            solid += 1;
            match kind {
                TileKind::Ore(mineral) => {
                    ore += 1;
                    value += u64::from(mineral.value());
                }
                TileKind::Artifact(artifact) => {
                    ore += 1;
                    value += u64::from(artifact.value());
                }
                _ => {}
            }
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "a report")]
    let mean = value as f64 / f64::from(ore.max(1));
    (f64::from(ore) / f64::from(solid.max(1)), mean)
}

#[test]
#[ignore = "prints the Crust's ore share against V10's bands"]
fn print_ore_share() {
    let settings = v11(crate::worldgen::DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    for stratum in Stratum::ALL.into_iter().take(5) {
        let (top, bottom) = layout.rows(stratum);
        let (share, value) = ore_share(&settings, top, bottom);
        println!(
            "V11 {stratum:?}: {:.1}% ore, mean {value:.0} cr",
            share * 100.0
        );
    }
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(crate::worldgen::DEFAULT_WORLD_SEED)
    };
    for band in crate::worldgen::Band::ALL.into_iter().take(5) {
        let (top, bottom) = crate::worldgen::strata::band_layout_rows(&classic, band);
        let (share, value) = ore_share(&classic, top, bottom);
        println!(
            "V10 {band:?}: {:.1}% ore, mean {value:.0} cr",
            share * 100.0
        );
    }
}
