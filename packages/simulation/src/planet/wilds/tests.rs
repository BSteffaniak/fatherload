//! The wild worlds' generation (Wave 12 planets-a).

use super::{
    WildTile, glacier, is_small_body, jungle, landmark_column, life, ocean, secret_column,
    strata_rows,
};
use crate::{
    hash::StateHasher,
    planet::{BodyGround, PlanetGround, PlanetParams, PlanetThemeId, world_of},
    terrain::{ChunkPosition, TileKind},
};

const THEMES: [PlanetThemeId; 3] = [
    PlanetThemeId::JUNGLE,
    PlanetThemeId::OCEAN,
    PlanetThemeId::GLACIER,
];

fn ground(theme: PlanetThemeId, radius: u32) -> PlanetGround {
    PlanetGround::new(PlanetParams::new(0x5EED_0012, radius, theme))
}

fn kind(ground: &PlanetGround, column: i64, row: i32) -> TileKind {
    ground
        .generated_tile(i32::try_from(column).unwrap_or(0), row)
        .kind
}

/// A digest of `chunks` chunks along the surface and a column of chunks down to the core.
fn digest(ground: &PlanetGround) -> u64 {
    let mut hasher = StateHasher::new();
    let width = ground.strip_width();
    let chunks_across = width / 32;
    let bottom = ground.core_floor_row() / 32 + 1;
    for index in 0..24 {
        let x = (index * 97) % chunks_across;
        for y in [ground.strip_top() / 32, -1, 0, 1] {
            for tile in ground.chunk_tiles(ChunkPosition { x, y }) {
                hasher.write_u64(tile.code());
            }
        }
    }
    for y in 0..=bottom {
        for tile in ground.chunk_tiles(ChunkPosition { x: 3, y }) {
            hasher.write_u64(tile.code());
        }
    }
    hasher.finish().get()
}

/// Pins the wild worlds' generation: a body of each theme at two radii. Change only on
/// purpose (every wild planet's ground moves with it).
const GOLDEN_WILDS_DIGESTS: [(PlanetThemeId, u32, u64); 6] = [
    (PlanetThemeId::JUNGLE, 600, 0x268e_3823_337f_ae5d),
    (PlanetThemeId::JUNGLE, 3_000, 0x3bfd_52f3_998b_86cf),
    (PlanetThemeId::OCEAN, 600, 0xb9af_1fb5_79ee_2c5a),
    (PlanetThemeId::OCEAN, 3_000, 0xca05_26f9_ba8a_8283),
    (PlanetThemeId::GLACIER, 600, 0x4190_2462_e411_cb2e),
    (PlanetThemeId::GLACIER, 3_000, 0x5a6d_260d_85da_6ebb),
];

#[test]
fn the_wild_themes_are_registered_and_generate_purely() {
    for theme in THEMES {
        let world = world_of(theme).expect("a theme world");
        assert_eq!(world.theme(), theme);
        assert!(life::wild_theme(theme).is_some());
        let planet = crate::planet::theme(theme);
        assert_eq!(planet.id, theme);
        assert_eq!(planet.strata.len(), 3);
        assert_eq!(
            planet
                .strata
                .iter()
                .map(|stratum| u32::from(stratum.share_permille))
                .sum::<u32>(),
            1_000
        );
        let a = ground(theme, 700);
        let b = ground(theme, 700);
        for chunk in [
            ChunkPosition { x: 0, y: 0 },
            ChunkPosition { x: 5, y: -2 },
            ChunkPosition { x: 40, y: 3 },
        ] {
            assert_eq!(a.chunk_tiles(chunk), b.chunk_tiles(chunk), "{theme:?}");
        }
        assert_eq!(digest(&a), digest(&b));
    }
}

#[test]
#[ignore = "prints the wild worlds' digests to pin them"]
fn print_wild_digests() {
    for (theme, radius, _) in GOLDEN_WILDS_DIGESTS {
        println!(
            "{theme:?} {radius}: {:#018x}",
            digest(&ground(theme, radius))
        );
    }
}

#[test]
fn the_wild_worlds_match_their_golden_digests() {
    for (theme, radius, golden) in GOLDEN_WILDS_DIGESTS {
        let got = digest(&ground(theme, radius));
        assert_eq!(
            got, golden,
            "{theme:?} at radius {radius}: {got:#018x} (re-pin on purpose only)"
        );
    }
}

#[test]
fn strata_run_800_to_2500_rows_on_planets_and_to_the_core_on_small_bodies() {
    assert_eq!(strata_rows(1_500), 800);
    assert_eq!(strata_rows(8_000), 2_500);
    assert_eq!(strata_rows(10_000), 2_500);
    assert!((1_600..1_800).contains(&strata_rows(5_000)));
    for theme in THEMES {
        let small = ground(theme, 600);
        assert!(is_small_body(small.params()));
        assert_eq!(
            small.core_floor_row(),
            small.bottom_row() - crate::planet::CORE_FLOOR_ROWS + 1,
            "a small body's strata reach its core"
        );
        let big = ground(theme, 5_000);
        assert_eq!(big.core_floor_row(), strata_rows(5_000));
        // Under the core floor: the body's indestructible heart.
        assert_eq!(
            kind(&big, 100, big.core_floor_row() + 40),
            TileKind::Bedrock
        );
        // The core chamber is open and floored with the core ore here and there.
        let top = big.core_chamber_top();
        let open = (0..400)
            .filter(|column| kind(&big, *column, top + 3).is_open())
            .count();
        assert!(open > 300, "{theme:?}: {open}");
        // Every stratum shows up on the way down.
        let names: Vec<&str> = (0..big.core_chamber_top())
            .step_by(5)
            .map(|row| big.layer_name(200, row))
            .collect();
        for stratum in crate::planet::theme(theme).strata {
            assert!(names.contains(&stratum.name), "{theme:?} {}", stratum.name);
        }
    }
}

#[test]
fn the_jungle_has_trees_vines_puffcaps_and_lifesap() {
    let jungle = ground(PlanetThemeId::JUNGLE, 3_000);
    let width = i64::from(jungle.strip_width());
    let mut trunks = 0;
    let mut canopy = 0;
    let mut vines = 0;
    for column in 0..2_000_i64 {
        let surface = jungle.surface_row(i32::try_from(column).unwrap_or(0));
        for row in surface - 40..surface {
            match kind(&jungle, column, row) {
                TileKind::Wild(WildTile::Trunk) => trunks += 1,
                TileKind::Wild(WildTile::Canopy) => canopy += 1,
                _ => {}
            }
            if jungle::vine_at(&jungle, column, row) {
                assert!(kind(&jungle, column, row).is_open(), "vines hang in air");
                vines += 1;
            }
        }
    }
    assert!(
        trunks > 500 && canopy > 1_000 && vines > 200,
        "{trunks} {canopy} {vines}"
    );
    // The Spore Caves' puffcaps and the Heartwood's lifesap.
    let count = |tile: WildTile, rows: std::ops::Range<i32>| {
        (0..width.min(3_000))
            .step_by(3)
            .flat_map(|column| rows.clone().map(move |row| (column, row)))
            .filter(|(column, row)| kind(&jungle, *column, *row) == TileKind::Wild(tile))
            .count()
    };
    let floor = jungle.core_floor_row();
    assert!(count(WildTile::Puffcap, floor / 3 + 10..floor * 2 / 3 - 10) > 20);
    assert!(count(WildTile::Lifesap, floor * 2 / 3 + 10..floor - 20) > 10);
    assert!(count(WildTile::LivingRoot, 6..floor / 3 - 10) > 100);
    // The Elder Tree and its seed.
    let elder = landmark_column(&jungle);
    let site = world_of(PlanetThemeId::JUNGLE)
        .and_then(|world| world.landmark_site(&jungle))
        .expect("a landmark");
    assert_eq!(
        kind(&jungle, elder, site.floor - 1),
        TileKind::Wild(WildTile::ElderSeed)
    );
    assert_eq!(
        kind(&jungle, elder + 3, site.floor - 20),
        TileKind::Wild(WildTile::ElderBark)
    );
}

#[test]
fn the_ocean_is_a_sea_with_islands_ruins_and_smokers() {
    let sea = ground(PlanetThemeId::OCEAN, 3_000);
    let width = i64::from(sea.strip_width());
    let mut land = 0;
    let mut water = 0;
    for column in 0..width {
        let surface = sea.surface_row(i32::try_from(column).unwrap_or(0));
        if surface <= ocean::SEA_LEVEL {
            land += 1;
        } else {
            water += 1;
            assert!(
                matches!(
                    kind(&sea, column, ocean::SEA_LEVEL),
                    TileKind::Fluid(crate::fluid::FluidKind::Water)
                        | TileKind::Wild(WildTile::Coral | WildTile::RuinStone)
                        | TileKind::Air
                ),
                "the sea at column {column}: {:?}",
                kind(&sea, column, ocean::SEA_LEVEL)
            );
        }
    }
    assert!(
        land > width / 8 && water > width / 3,
        "{land} land, {water} sea"
    );
    // The settlement's island is dry and level.
    let site = world_of(PlanetThemeId::OCEAN)
        .and_then(|world| world.settlement_site(&sea))
        .expect("a site");
    assert!(site.floor < ocean::SEA_LEVEL);
    for column in site.first..site.first + site.width {
        assert_eq!(sea.surface_row(column), site.floor);
        assert!(kind(&sea, i64::from(column), site.floor - 1).is_open());
        assert!(!kind(&sea, i64::from(column), site.floor).is_open());
    }
    // The Drowned Spire rises out of the sea with a relic in its dry room.
    let spire = landmark_column(&sea);
    assert_eq!(
        kind(&sea, spire, ocean::SEA_LEVEL - 1),
        TileKind::Wild(WildTile::RuinRelic)
    );
    assert_eq!(
        kind(&sea, spire, ocean::SEA_LEVEL + 2),
        TileKind::Fluid(crate::fluid::FluidKind::Water)
    );
    // Halls, relics and smokers below.
    let floor = sea.core_floor_row();
    let count = |tile: WildTile, rows: std::ops::Range<i32>| {
        (0..width.min(6_000))
            .step_by(2)
            .flat_map(|column| rows.clone().map(move |row| (column, row)))
            .filter(|(column, row)| kind(&sea, *column, *row) == TileKind::Wild(tile))
            .count()
    };
    assert!(count(WildTile::RuinRelic, floor / 3..floor * 2 / 3) > 3);
    assert!(count(WildTile::Smoker, floor * 2 / 3..floor - 10) > 10);
}

#[test]
fn the_glacier_has_frozen_seas_crevasses_icicles_and_the_titan() {
    let ice = ground(PlanetThemeId::GLACIER, 3_000);
    let width = i64::from(ice.strip_width());
    let mut sea = 0;
    let mut crevasses = 0;
    for column in 0..width {
        if glacier::frozen_sea_at(&ice, column) {
            sea += 1;
            let rows = glacier::sea_rows(&ice, column);
            assert_eq!(kind(&ice, column, 0), TileKind::Wild(WildTile::SeaIce));
            assert!(kind(&ice, column, glacier::LID_ROWS + rows / 2).is_open());
        }
    }
    for cell in 0..width / 64 {
        if let Some((column, _, depth, _)) = glacier::crevasse_in(&ice, cell) {
            crevasses += 1;
            let surface = ice.surface_row(i32::try_from(column).unwrap_or(0));
            assert!(kind(&ice, column, surface + depth / 2).is_open());
        }
    }
    assert!(sea > width / 10 && crevasses > 20, "{sea} {crevasses}");
    let titan = landmark_column(&ice);
    let floor = ice.surface_row(i32::try_from(titan).unwrap_or(0));
    assert_eq!(
        kind(&ice, titan + 8, floor + 15),
        TileKind::Wild(WildTile::TitanHeart)
    );
    let core = ice.core_floor_row();
    let icicles = (0..width.min(4_000))
        .step_by(2)
        .flat_map(|column| (core * 2 / 3..core - 10).map(move |row| (column, row)))
        .filter(|(column, row)| kind(&ice, *column, *row) == TileKind::Wild(WildTile::Icicle))
        .count();
    assert!(icicles > 20, "{icicles}");
}

#[test]
fn every_wild_world_has_a_level_clear_settlement_site() {
    for theme in THEMES {
        for radius in [450, 2_000, 6_000] {
            let planet = ground(theme, radius);
            let site = world_of(theme)
                .and_then(|world| world.settlement_site(&planet))
                .expect("a site");
            for column in site.first..site.first + site.width {
                assert_eq!(planet.surface_row(column), site.floor, "{theme:?} {radius}");
                for row in site.floor - 12..site.floor {
                    assert!(
                        kind(&planet, i64::from(column), row).is_open()
                            && !matches!(kind(&planet, i64::from(column), row), TileKind::Fluid(_)),
                        "{theme:?} {radius}: ({column}, {row}) is {:?}",
                        kind(&planet, i64::from(column), row)
                    );
                }
            }
        }
    }
}

#[test]
fn small_bodies_hide_a_secret_under_a_cairn() {
    for theme in THEMES {
        let planet = ground(theme, 500);
        let column = secret_column(&planet);
        let surface = planet.surface_row(i32::try_from(column).unwrap_or(0));
        let cache = (surface..surface + super::SECRET_DEPTH + 6)
            .any(|row| kind(&planet, column, row) == TileKind::Wild(WildTile::Cache));
        assert!(cache, "{theme:?}");
        assert!(!kind(&planet, column, surface - 1).is_open(), "the cairn");
        // A planet has none.
        let big = ground(theme, 3_000);
        let column = secret_column(&big);
        let surface = big.surface_row(i32::try_from(column).unwrap_or(0));
        assert!(
            !(surface..surface + 30)
                .any(|row| kind(&big, column, row) == TileKind::Wild(WildTile::Cache))
        );
    }
}

#[test]
fn the_strip_wraps_seamlessly() {
    for theme in THEMES {
        let planet = ground(theme, 800);
        let width = i64::from(planet.strip_width());
        for row in -20..40 {
            assert_eq!(
                kind(&planet, width + 5, row),
                kind(&planet, 5, row),
                "{theme:?}"
            );
        }
        let a = planet.surface_row(i32::try_from(width - 1).unwrap_or(0));
        let b = planet.surface_row(0);
        assert!((a - b).abs() <= 6, "{theme:?}: {a} -> {b}");
    }
}

#[test]
fn hazards_are_tagged_per_theme() {
    use life::PlanetHazard;
    let tags = |theme: PlanetThemeId, gravity: u16| {
        let mut params = PlanetParams::new(1, 3_000, theme);
        params.gravity_milli = gravity;
        life::body_hazards(&params)
    };
    assert_eq!(
        tags(PlanetThemeId::JUNGLE, 900),
        vec![PlanetHazard::ToxicAir]
    );
    assert_eq!(
        tags(PlanetThemeId::OCEAN, 900),
        vec![PlanetHazard::Heat, PlanetHazard::Pressure]
    );
    assert_eq!(tags(PlanetThemeId::GLACIER, 900), vec![PlanetHazard::Cold]);
    assert!(tags(PlanetThemeId::GLACIER, 1_600).contains(&PlanetHazard::HighGravity));
    // Every hazard but high gravity has a counter from the Descent already.
    for hazard in PlanetHazard::ALL {
        assert_eq!(
            hazard.descent_counter().is_none(),
            matches!(hazard, PlanetHazard::Heat | PlanetHazard::HighGravity)
        );
    }
}

#[test]
fn weather_and_day_are_pure_functions_of_the_body_and_time() {
    for theme in THEMES {
        let params = PlanetParams::new(77, 2_000, theme);
        let mut storms = 0;
        for minute in 0..600_u64 {
            let millis = minute * 60_000;
            let weather = life::weather_at(&params, millis).expect("a wild theme has weather");
            assert_eq!(Some(weather), life::weather_at(&params, millis));
            if weather.intensity > 0 {
                storms += 1;
            }
            assert_eq!(
                life::is_night(&params, millis),
                life::is_night(&params, millis)
            );
        }
        assert!((60..400).contains(&storms), "{theme:?}: {storms}");
    }
    assert!(life::weather_at(&PlanetParams::new(1, 900, PlanetThemeId::BARREN), 0).is_none());
}

#[test]
fn the_space_generators_tags_get_the_wild_worlds() {
    use crate::space::Theme;
    assert_eq!(
        crate::place::planet_theme_of(Theme::Jungle),
        PlanetThemeId::JUNGLE
    );
    assert_eq!(
        crate::place::planet_theme_of(Theme::OceanArchipelago),
        PlanetThemeId::OCEAN
    );
    assert_eq!(
        crate::place::planet_theme_of(Theme::Ice),
        PlanetThemeId::GLACIER
    );
}

#[test]
fn the_wild_themes_appear_in_the_space_report() {
    let text = crate::space::report::space_report(0x5EED, 2);
    for line in [
        "Jungle World (world): Root Tangle, Spore Caves, Heartwood Deep",
        "Ocean Archipelago (world): Reef Shelf, Drowned Ruins, The Trench",
        "Glacier World (world): Glacier Ice, Under-Ice Sea, Permafrost Caves",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}

#[test]
#[ignore = "prints an ASCII map round each landmark"]
fn print_landmarks() {
    for theme in THEMES {
        let planet = ground(theme, 3_000);
        let centre = landmark_column(&planet);
        let floor = planet.surface_row(i32::try_from(centre).unwrap_or(0));
        println!("{theme:?} landmark at {centre}, floor {floor}");
        for row in floor - 64..floor + 26 {
            let line: String = (centre - 40..centre + 40)
                .map(|column| match kind(&planet, column, row) {
                    TileKind::Air => ' ',
                    TileKind::Fluid(_) => '~',
                    TileKind::Wild(
                        WildTile::ElderBark | WildTile::RuinStone | WildTile::TitanBone,
                    ) => '#',
                    TileKind::Wild(WildTile::Canopy) => '*',
                    TileKind::Wild(WildTile::LivingRoot) => 'r',
                    TileKind::Wild(
                        WildTile::ElderSeed | WildTile::RuinRelic | WildTile::TitanHeart,
                    ) => '@',
                    TileKind::Wild(WildTile::BlueIce) => 'b',
                    _ => '.',
                })
                .collect();
            println!("{row:5} {line}");
        }
    }
}
