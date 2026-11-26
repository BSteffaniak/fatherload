//! The Hollow World's generated places on a Deep Descent world.

use super::*;
use crate::worldgen::{GeneratorVersion, WorldSettings, descent::layout, streamed::generated_tile};

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(0x5744_0905)
    }
}

fn kind(settings: &WorldSettings, x: i32, y: i32) -> TileKind {
    generated_tile(settings, TilePosition { x, y }).kind
}

#[test]
fn the_jungle_has_open_halls_with_fern_floors_trees_and_tar() {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::MegafaunaJungle);
    let (mut open, mut fern, mut trunk, mut canopy, mut tar, mut total) = (0, 0, 0, 0, 0, 0);
    for y in (top + 20..bottom - 20).step_by(3) {
        for x in (-300..300).step_by(2) {
            total += 1;
            match kind(&settings, x, y) {
                TileKind::Air => open += 1,
                TileKind::Hollow(HollowTile::Fern) => fern += 1,
                TileKind::Hollow(HollowTile::Trunk) => trunk += 1,
                TileKind::Hollow(HollowTile::Canopy) => canopy += 1,
                TileKind::Hollow(HollowTile::Tar) => tar += 1,
                _ => {}
            }
        }
    }
    assert!(open * 100 / total > 20, "halls: {open} of {total}");
    assert!(
        fern > 20 && trunk > 10 && canopy > 10 && tar > 0,
        "{fern} {trunk} {canopy} {tar}"
    );
}

#[test]
fn isles_hang_in_the_isle_sea_with_grass_on_their_undersides() {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::UpsideDownIsles);
    let area = TileRect {
        min: TilePosition { x: -600, y: top },
        max: TilePosition { x: 600, y: bottom },
    };
    let isles = isles_touching(&settings, &layout, &area);
    assert!(isles.len() >= 6, "{} isles", isles.len());
    for isle in isles.iter().take(6) {
        let centre = isle.bounds.min.x + isle.bounds.width() / 2;
        assert_eq!(
            kind(&settings, centre, isle.underside()),
            TileKind::Hollow(HollowTile::IsleGrass)
        );
        // Open air under it, where its gravity points up.
        assert_eq!(kind(&settings, centre, isle.underside() + 4), TileKind::Air);
    }
    assert!(isles.iter().any(|isle| isle.stalk.is_some()));
}

#[test]
fn the_sun_well_rings_the_inner_sun_with_pillars_and_doorways() {
    let settings = settings();
    let layout = layout(&settings);
    let centre = sun_well(&settings, &layout).expect("the well");
    assert_eq!(
        kind(&settings, centre.x, centre.y),
        TileKind::Hollow(HollowTile::InnerSun)
    );
    for pillar in shade_pillars(centre) {
        assert_eq!(
            kind(&settings, pillar.x, pillar.y),
            TileKind::Hollow(HollowTile::ShadePillar)
        );
    }
    // The doorways through the wall, and wall elsewhere on the ring.
    assert_eq!(
        kind(&settings, centre.x, centre.y - WELL_RADIUS + 1),
        TileKind::Air
    );
    let diagonal = WELL_RADIUS * 707 / 1_000;
    assert_eq!(
        kind(&settings, centre.x + diagonal, centre.y + diagonal),
        TileKind::Hollow(HollowTile::WellWall)
    );
    // Open between the sun and the ring.
    assert_eq!(kind(&settings, centre.x + 10, centre.y), TileKind::Air);
}

#[test]
fn the_forge_sky_is_open_with_stones_to_hop_between() {
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::TheSunforge);
    let (mut open, mut stone, mut glass) = (0, 0, 0);
    for y in (top + 30..bottom - 60).step_by(2) {
        for x in (200..500).step_by(2) {
            match kind(&settings, x, y) {
                TileKind::Air => open += 1,
                TileKind::Hollow(HollowTile::SunRock) | TileKind::HardRock => stone += 1,
                TileKind::Hollow(HollowTile::SunGlass) => glass += 1,
                _ => {}
            }
        }
    }
    assert!(open > stone, "{open} open, {stone} stone");
    assert!(stone > open / 10, "stones to hop between: {stone}");
    let _ = glass;
}

#[test]
fn the_breach_hangs_under_the_rock_sky() {
    let settings = settings();
    let layout = layout(&settings);
    let breach = breach(&settings, &layout).expect("the breach");
    assert_eq!(
        kind(&settings, breach.center().x, breach.max.y),
        TileKind::Hollow(HollowTile::BreachTimber)
    );
    assert_eq!(
        kind(&settings, breach.center().x, breach.max.y + 2),
        TileKind::Air
    );
}

#[test]
fn the_sunfolk_valley_holds_the_village_hall() {
    let settings = settings();
    let (site, center) =
        crate::worldgen::streamed::villages::sunfolk_hall(&settings).expect("the hall");
    // The hall's air and its floor.
    assert_eq!(kind(&settings, center, site.top + 3), TileKind::Air);
    assert!(matches!(
        kind(&settings, center, site.floor_row()),
        TileKind::Hollow(HollowTile::Fern) | TileKind::Foundation
    ));
    let village =
        crate::worldgen::streamed::villages::village(&settings, site.id()).expect("the village");
    assert_eq!(village.name, crate::hollow::SUNFOLK_NAME);
    assert_ne!(village.population.characters, []);
}

/// An ASCII picture of the Hollow World's strata (`--ignored --nocapture`).
#[test]
#[ignore = "a picture for eyes, not an assertion"]
fn dump_hollow_world() {
    let settings = settings();
    let layout = layout(&settings);
    let (first, last) = layout.descent_rows(Descent::HollowWorld);
    for y in (first..=last).step_by(6) {
        let row: String = (-240..240)
            .step_by(3)
            .map(|x| match kind(&settings, x, y) {
                TileKind::Air => ' ',
                TileKind::Hollow(HollowTile::Fern) => '"',
                TileKind::Hollow(HollowTile::Trunk) => '|',
                TileKind::Hollow(HollowTile::Canopy) => '*',
                TileKind::Hollow(HollowTile::Tar) => '~',
                TileKind::Hollow(HollowTile::IsleRock | HollowTile::IsleGrass) => 'I',
                TileKind::Hollow(HollowTile::InnerSun) => 'O',
                TileKind::Hollow(HollowTile::WellWall | HollowTile::ShadePillar) => '#',
                TileKind::Lava => '=',
                _ => '.',
            })
            .collect();
        println!("{y:6} {row}");
    }
}
