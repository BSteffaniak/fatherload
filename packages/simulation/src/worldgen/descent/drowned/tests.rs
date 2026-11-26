//! The Drowned and Frozen's generated places: every stratum's rock and set pieces are in the
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

fn count_in(settings: &WorldSettings, rect: &TileRect, kind: TileKind) -> usize {
    rect.positions()
        .filter(|at| kind_at(settings, at.x, at.y) == kind)
        .count()
}

const DROWNED: [Stratum; 5] = [
    Stratum::UndergroundOcean,
    Stratum::BrineGlaciers,
    Stratum::GeyserFields,
    Stratum::MerrowCity,
    Stratum::PearlAbyss,
];

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::Fluid(FluidKind::Water) => '~',
        TileKind::Fluid(_) => '=',
        TileKind::Cave(CaveTile::Seabed) => '.',
        TileKind::Cave(CaveTile::Kelp) => 'k',
        TileKind::Cave(CaveTile::Coral) => 'c',
        TileKind::Cave(CaveTile::BrineIce) => 'i',
        TileKind::Cave(CaveTile::PackIce) => 'I',
        TileKind::Cave(CaveTile::ThermalVent) => 'V',
        TileKind::Cave(CaveTile::FrozenFigure) => 'F',
        TileKind::Cave(CaveTile::Sinter) => 's',
        TileKind::Cave(CaveTile::GeyserVent) => 'G',
        TileKind::Cave(CaveTile::PillowBasalt) => 'b',
        TileKind::Cave(CaveTile::PearlClam) => 'P',
        TileKind::Cave(CaveTile::MerrowStone) => 'm',
        TileKind::Built(_) => '#',
        TileKind::Foundation => '_',
        TileKind::Ore(_) => '$',
        TileKind::HardRock => ':',
        _ => '?',
    }
}

#[test]
#[ignore = "prints an ASCII dump of each drowned stratum and set piece for review"]
fn dump_drowned_strata() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let dump = |label: &str, rect: TileRect| {
        println!("{label} {rect:?}");
        for y in rect.min.y..=rect.max.y {
            let line: String = (rect.min.x..=rect.max.x)
                .map(|x| glyph(kind_at(&settings, x, y)))
                .collect();
            println!("{line}");
        }
    };
    for stratum in DROWNED {
        let (top, bottom) = layout.rows(stratum);
        let row = top + (bottom - top) / 2;
        dump(
            &format!("{stratum:?} rows {top}..{bottom}"),
            TileRect::around(TilePosition { x: 60, y: row }, 90, 22),
        );
    }
    let city = merrow_city(&settings).expect("city");
    dump("Merrow City", city.bounds().expanded(2));
    dump(
        "Leviathan arena",
        leviathan_arena(&settings, &layout)
            .expect("arena")
            .expanded(2),
    );
    dump(
        "Old Faithful",
        old_faithful(&settings, &layout)
            .expect("geyser")
            .expanded(2),
    );
    dump(
        "Grotto",
        eel_grotto(&settings, &layout).expect("grotto").expanded(2),
    );
    dump(
        "Wreck",
        drowned_sub(&settings, &layout).expect("wreck").expanded(2),
    );
    dump(
        "Figures",
        frozen_figures(&settings, &layout)
            .expect("figures")
            .expanded(2),
    );
}

#[test]
fn every_drowned_stratum_has_its_own_rock_and_set_pieces() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let window = |stratum: Stratum| {
        let (top, bottom) = layout.rows(stratum);
        TileRect {
            min: TilePosition {
                x: -120,
                y: top + 20,
            },
            max: TilePosition {
                x: 260,
                y: bottom - 20,
            },
        }
    };
    let ocean = window(Stratum::UndergroundOcean);
    assert!(count_in(&settings, &ocean, TileKind::Cave(CaveTile::Seabed)) > 2_000);
    assert!(
        count_in(&settings, &ocean, WATER) > 1_500,
        "the ocean is mostly sea in its basins"
    );
    assert!(count_in(&settings, &ocean, TileKind::Cave(CaveTile::Kelp)) > 20);
    let glaciers = window(Stratum::BrineGlaciers);
    assert!(count_in(&settings, &glaciers, TileKind::Cave(CaveTile::BrineIce)) > 2_000);
    assert!(count_in(&settings, &glaciers, TileKind::Cave(CaveTile::ThermalVent)) > 0);
    let geysers = window(Stratum::GeyserFields);
    assert!(count_in(&settings, &geysers, TileKind::Cave(CaveTile::Sinter)) > 2_000);
    assert!(count_in(&settings, &geysers, TileKind::Cave(CaveTile::GeyserVent)) >= 6);
    let abyss = window(Stratum::PearlAbyss);
    assert!(count_in(&settings, &abyss, TileKind::Cave(CaveTile::PillowBasalt)) > 2_000);
    assert!(count_in(&settings, &abyss, TileKind::Cave(CaveTile::PearlClam)) > 0);
}

#[test]
fn the_fixed_set_pieces_are_carved_where_their_functions_say() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    // The grotto and the arena are flooded.
    let grotto = eel_grotto(&settings, &layout).expect("grotto");
    assert!(count_in(&settings, &grotto, WATER) > 300);
    let arena = leviathan_arena(&settings, &layout).expect("arena");
    assert!(count_in(&settings, &arena, WATER) > 1_500);
    assert_eq!(
        kind_at(&settings, arena.center().x, arena.min.y + AIR_ROWS + 1),
        TileKind::Air,
        "a breath of air under the arena's roof"
    );
    // Old Faithful: vents in its throat and an open shaft over them.
    let geyser = old_faithful(&settings, &layout).expect("geyser");
    let cx = geyser.min.x + geyser.width() / 2;
    assert_eq!(
        kind_at(&settings, cx, geyser.max.y),
        TileKind::Cave(CaveTile::GeyserVent)
    );
    let open = (geyser.min.y + 6..geyser.max.y)
        .filter(|row| kind_at(&settings, cx, *row).is_open())
        .count();
    assert!(open > 30, "the shaft is open ({open} rows)");
    // The frozen figures stand round the cable.
    let figures = frozen_figures(&settings, &layout).expect("figures");
    assert!(count_in(&settings, &figures, TileKind::Cave(CaveTile::FrozenFigure)) >= 4);
    // The wreck holds its cache.
    let wreck = drowned_sub(&settings, &layout).expect("wreck");
    assert_eq!(
        count_in(&settings, &wreck, TileKind::Cave(CaveTile::TreasureCache)),
        1
    );
}

#[test]
fn merrow_city_is_a_dry_bubble_round_the_outpost_with_moon_pools() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let site = merrow_city(&settings).expect("city");
    // The city's air under the dome is dry, its floor is reef stone, glass rings it.
    let inside = TileRect {
        min: TilePosition {
            x: site.dome.min.x + 2,
            y: site.floor - 6,
        },
        max: TilePosition {
            x: site.dome.max.x - 2,
            y: site.floor - 1,
        },
    };
    assert_eq!(
        count_in(&settings, &inside, WATER),
        0,
        "no sea in the streets"
    );
    let glass = count_in(
        &settings,
        &site.bounds(),
        TileKind::Built(crate::base::BuiltTile::Glass),
    );
    assert!(glass > 60, "the bubble's glass ({glass})");
    assert!(
        count_in(&settings, &site.bounds(), WATER) > 400,
        "the sea round it"
    );
    // The moon pools open from the floor into the harbour's water.
    for left in site.moon_pools() {
        assert!(kind_at(&settings, left + 1, site.floor).is_open());
        assert_eq!(kind_at(&settings, left + 1, site.floor + 2), WATER);
    }
    // The outpost still stands.
    assert!(matches!(
        kind_at(&settings, site.outpost.min.x, site.outpost.min.y + 2),
        TileKind::Built(_) | TileKind::Air
    ));
}

#[test]
fn stamps_are_deterministic_and_absent_from_older_worlds() {
    let settings = settings(4_242);
    let layout = layout(&settings);
    let (first, last) = layout.descent_rows(Descent::DrownedAndFrozen);
    let area = TileRect {
        min: TilePosition { x: -200, y: first },
        max: TilePosition { x: 300, y: last },
    };
    assert_eq!(
        stamps_with(&settings, &layout, &area),
        stamps_with(&settings, &layout, &area)
    );
    assert!(stamps_with(&settings, &layout, &area).len() > 10);
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(4_242)
    };
    assert_eq!(stamps_with(&classic, &layout, &area), []);
    assert!(merrow_city(&classic).is_none());
}
