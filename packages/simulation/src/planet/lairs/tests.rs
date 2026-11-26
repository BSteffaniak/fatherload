//! The Wave 12 worlds' theme-boss lairs (Wave 14 finish).

use super::{
    LAIR_BOSSES, LAIR_TILE_CODE_BASE, Lair, LairEvent, LairTile, WayIn, lair_boss, lair_of,
};
use crate::{
    fluid::FluidKind,
    planet::{
        BodyGround, PlanetGround, PlanetParams, PlanetThemeId,
        outlands::{LAIR_HALF_WIDTH, OutlandTile},
        wilds::wrap_delta,
    },
    terrain::{Tile, TileKind},
};

fn ground(theme: PlanetThemeId, radius: u32, seed: u64) -> PlanetGround {
    PlanetGround::new(PlanetParams::new(seed, radius, theme))
}

fn tile(ground: &PlanetGround, column: i64, row: i32) -> Tile {
    let width = i64::from(ground.strip_width());
    ground.generated_tile(i32::try_from(column.rem_euclid(width)).unwrap_or(0), row)
}

fn kind(ground: &PlanetGround, column: i64, row: i32) -> TileKind {
    tile(ground, column, row).kind
}

/// The six Wave 12 themes, in theme order.
fn themes() -> [PlanetThemeId; 6] {
    [
        PlanetThemeId::JUNGLE,
        PlanetThemeId::OCEAN,
        PlanetThemeId::GLACIER,
        crate::frontier::DUNE_WORLD,
        crate::frontier::VOLCANIC_WORLD,
        crate::frontier::CRYSTAL_WORLD,
    ]
}

#[test]
fn every_wave_12_world_has_one_boss_and_the_codes_stay_in_range() {
    for (boss, theme) in LAIR_BOSSES.iter().zip(themes()) {
        assert_eq!(boss.theme, theme);
        assert_eq!(lair_boss(theme), Some(boss));
        assert!(boss.kind.is_lair_boss(), "{:?}", boss.kind);
        assert!(boss.relic.is_relic());
        assert!(boss.relic.reward().is_some());
    }
    // No outland or exotic theme has one here (theirs live in their own modules).
    for theme in [
        PlanetThemeId::FUNGAL,
        PlanetThemeId::MACHINE,
        PlanetThemeId::DEAD,
        crate::exotic::GAS_GIANT,
    ] {
        assert!(lair_boss(theme).is_none());
    }
    for (index, tile) in LairTile::ALL.iter().enumerate() {
        assert_eq!(tile.code(), index as u64);
        assert_eq!(LairTile::from_code(tile.code()), Some(*tile));
        let full = Tile {
            kind: TileKind::Lair(*tile),
            durability: tile.durability(),
        };
        assert_eq!(Tile::from_code(full.code()), Some(full), "{tile:?}");
    }
    assert!(LAIR_TILE_CODE_BASE + (LairTile::ALL.len() as u64) < 0x3C00);
    for (index, event) in LairEvent::ALL.iter().enumerate() {
        assert_eq!(usize::from(event.code()), index);
    }
    assert!(LairEvent::ALL.len() <= 20);
}

/// The hall is open and dry, its seal on the floor, its vault sealed round the relic, its
/// way in open from the surface (or the spire's dry room) to the hall, and nothing of the sea,
/// lava or a fluid inside.
fn assert_lair(planet: &PlanetGround, lair: &Lair) {
    let theme = planet.params().theme;
    let (middle, row) = lair.middle();
    assert_eq!(kind(planet, middle, row), TileKind::Air, "{theme:?}");
    assert_eq!(
        kind(planet, i64::from(lair.seal.0), lair.seal.1),
        TileKind::Outland(OutlandTile::BossSeal),
        "{theme:?}"
    );
    for (column, door_row) in lair.door {
        assert_eq!(
            kind(planet, i64::from(column), door_row),
            TileKind::Outland(OutlandTile::VaultDoor),
            "{theme:?}"
        );
    }
    assert_eq!(
        kind(planet, i64::from(lair.relic.0), lair.relic.1),
        TileKind::Lair(lair.boss.relic),
        "{theme:?}"
    );
    // A floor under the whole hall, a wall round it.
    for dx in -LAIR_HALF_WIDTH..=LAIR_HALF_WIDTH {
        assert!(
            !kind(planet, lair.centre + dx, lair.floor).is_open(),
            "{theme:?} floor at {dx}"
        );
        for row in lair.top..lair.floor {
            let found = kind(planet, lair.centre + dx, row);
            assert!(
                !matches!(found, TileKind::Fluid(_) | TileKind::Lava),
                "{theme:?}: {found:?} in the hall at {dx}, {row}"
            );
        }
    }
    // The lamps light it.
    let lamps = (-LAIR_HALF_WIDTH - 2..=LAIR_HALF_WIDTH + 2)
        .flat_map(|dx| (lair.top - 2..=lair.floor).map(move |row| (dx, row)))
        .filter(|(dx, row)| kind(planet, lair.centre + dx, *row) == TileKind::Lair(LairTile::Lamp))
        .count();
    assert!(lamps >= 6, "{theme:?}: {lamps} lamps");
    // The way in: open all the way down, walled against the sea and the lava.
    for row in lair.shaft_top..lair.top {
        let found = kind(planet, lair.shaft, row);
        assert_eq!(found, TileKind::Air, "{theme:?} shaft at row {row}");
        for side in [-3_i64, 3] {
            let wall = kind(planet, lair.shaft + side, row);
            assert!(
                !matches!(wall, TileKind::Fluid(_) | TileKind::Lava),
                "{theme:?}: {wall:?} beside the shaft at row {row}"
            );
        }
    }
    let width = i64::from(planet.strip_width());
    assert!(lair.contains(lair.centre, row, width));
    assert!(!lair.contains(lair.centre + LAIR_HALF_WIDTH + 3, row, width));
}

#[test]
fn every_wave_12_planet_has_its_lair_under_its_landmark() {
    for theme in themes() {
        for (seed, radius) in [
            (0x5EED_0014_u64, 3_000_u32),
            (7, 5_000),
            (0x00C0_FFEE, 1_600),
        ] {
            let planet = ground(theme, radius, seed);
            let lair = lair_of(&planet).unwrap_or_else(|| panic!("{theme:?} has no lair"));
            assert_eq!(lair.boss.theme, theme);
            let landmark = super::landmark_column(&planet).expect("a landmark");
            assert_eq!(
                wrap_delta(lair.centre, landmark, i64::from(planet.strip_width())),
                0
            );
            assert_lair(&planet, &lair);
        }
    }
}

#[test]
fn small_bodies_keep_their_secret_and_have_no_lair() {
    for theme in themes() {
        assert!(lair_of(&ground(theme, 800, 3)).is_none(), "{theme:?}");
    }
}

#[test]
fn the_tidecallers_well_runs_dry_down_the_drowned_spire() {
    let planet = ground(PlanetThemeId::OCEAN, 3_000, 0x5EED_0014);
    let lair = lair_of(&planet).expect("a lair");
    let WayIn::Well { top } = lair.boss.way_in else {
        panic!("the ocean's way in is a well");
    };
    assert_eq!(lair.shaft, lair.centre);
    // The spire's dry room floor stands over it (drill through it to drop in).
    assert!(!kind(&planet, lair.shaft, top - 1).is_open());
    // The sea is outside the well's walls (the spire's interior is water below its floor).
    let seabed = planet.surface_row(i32::try_from(lair.centre).unwrap_or(0));
    assert!(seabed > top + 2, "the spire stands in the sea");
    let mut water_beside = false;
    for row in top..seabed {
        assert_eq!(kind(&planet, lair.shaft, row), TileKind::Air);
        assert!(!kind(&planet, lair.shaft + 3, row).is_open());
        water_beside |= matches!(
            kind(&planet, lair.shaft + 5, row),
            TileKind::Fluid(FluidKind::Water)
        );
    }
    assert!(water_beside, "the well runs through the sea");
}

#[test]
fn lairs_are_pure_functions_of_the_body() {
    for theme in themes() {
        let a = ground(theme, 3_000, 11);
        let b = ground(theme, 3_000, 11);
        let lair = lair_of(&a).expect("a lair");
        assert_eq!(lair_of(&b), Some(lair));
        for row in lair.top - 3..lair.floor + 3 {
            for dx in -40..=40 {
                assert_eq!(
                    tile(&a, lair.centre + dx, row),
                    tile(&b, lair.centre + dx, row)
                );
            }
        }
    }
}
