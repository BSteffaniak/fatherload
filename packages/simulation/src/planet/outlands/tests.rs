//! The outland worlds' generation (Wave 13 planets-c).

use super::{
    BossLair, OUTLAND_THEMES, OutlandTile, boss_lair, builder_outposts, dead, fungal, hooks,
    landmark_column, life, machine, theme_boss,
};
use crate::{
    hash::StateHasher,
    planet::{
        BodyGround, PlanetGround, PlanetParams, PlanetThemeId,
        wilds::{SECRET_DEPTH, is_small_body, secret_column, strata_rows},
        world_of,
    },
    terrain::{ChunkPosition, TileKind},
};

fn ground(theme: PlanetThemeId, radius: u32) -> PlanetGround {
    PlanetGround::new(PlanetParams::new(0x5EED_0013, radius, theme))
}

fn kind(ground: &PlanetGround, column: i64, row: i32) -> TileKind {
    ground
        .generated_tile(i32::try_from(column).unwrap_or(0), row)
        .kind
}

fn is(ground: &PlanetGround, column: i64, row: i32, tile: OutlandTile) -> bool {
    kind(ground, column, row) == TileKind::Outland(tile)
}

/// Counts `tile` in a window of `ground` (`columns` by `rows`, from `(0, top)`).
fn count(
    ground: &PlanetGround,
    tile: OutlandTile,
    columns: i64,
    rows: std::ops::Range<i32>,
) -> usize {
    let mut found = 0;
    for row in rows {
        for column in 0..columns {
            if is(ground, column, row, tile) {
                found += 1;
            }
        }
    }
    found
}

/// A digest of chunks along the surface and a column of chunks down to the core.
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

/// Pins the outland worlds' generation: a body of each theme at two radii. Change only on
/// purpose (every outland planet's ground moves with it).
const GOLDEN_OUTLAND_DIGESTS: [(PlanetThemeId, u32, u64); 6] = [
    (PlanetThemeId::FUNGAL, 600, 0xe365_07f9_a618_a44a),
    (PlanetThemeId::FUNGAL, 3_000, 0xe461_f6fa_0e0e_69eb),
    (PlanetThemeId::MACHINE, 600, 0xa79d_a782_7fd2_f8f1),
    (PlanetThemeId::MACHINE, 3_000, 0xa934_d5b4_63b7_24c6),
    (PlanetThemeId::DEAD, 600, 0x3d02_7dfc_bfdd_454f),
    (PlanetThemeId::DEAD, 3_000, 0xc0d7_e06b_f1ba_d795),
];

#[test]
fn the_outland_themes_are_registered_and_generate_purely() {
    for theme in OUTLAND_THEMES {
        assert!(super::is_outland(theme));
        let world = world_of(theme).expect("a theme world");
        assert_eq!(world.theme(), theme);
        assert!(life::outland_theme(theme).is_some());
        assert!(theme_boss(theme).is_some());
        let data = crate::planet::theme(theme);
        assert_eq!(data.id, theme);
        assert_eq!(data.strata.len(), 3);
        assert_eq!(
            data.strata
                .iter()
                .map(|stratum| u32::from(stratum.share_permille))
                .sum::<u32>(),
            1_000
        );
        let hooks = crate::planet::hooks(theme);
        assert_eq!(hooks.id(), theme);
        assert!(hooks.rules_own_hazards());
        assert!(!hooks.hazard_tags().is_empty());
        assert!(hooks.music_bed().name.starts_with("loop_planet_"));
        let a = ground(theme, 700);
        let b = ground(theme, 700);
        for chunk in [
            ChunkPosition { x: 0, y: -2 },
            ChunkPosition { x: 3, y: 0 },
            ChunkPosition { x: 7, y: 5 },
        ] {
            assert_eq!(a.chunk_tiles(chunk), b.chunk_tiles(chunk), "{theme:?}");
        }
        let other = PlanetGround::new(PlanetParams::new(99, 700, theme));
        assert_ne!(digest(&a), digest(&other), "{theme:?}: seeds differ");
    }
    // The other themes are untouched by the outlands.
    assert!(!super::is_outland(PlanetThemeId::JUNGLE));
    assert!(life::outland_theme(PlanetThemeId::BARREN).is_none());
}

#[test]
#[ignore = "prints the digests to pin"]
fn print_outland_digests() {
    for (theme, radius, _) in GOLDEN_OUTLAND_DIGESTS {
        println!(
            "{theme:?} {radius}: {:#018x}",
            digest(&ground(theme, radius))
        );
    }
}

#[test]
fn the_outland_worlds_match_their_golden_digests() {
    for (theme, radius, expected) in GOLDEN_OUTLAND_DIGESTS {
        assert_eq!(
            digest(&ground(theme, radius)),
            expected,
            "{theme:?} r{radius}: outland generation changed"
        );
    }
}

#[test]
fn strata_run_800_to_2500_rows_on_planets_and_to_the_core_on_small_bodies() {
    for theme in OUTLAND_THEMES {
        let small = ground(theme, 600);
        assert!(is_small_body(small.params()));
        assert_eq!(
            small.core_floor_row(),
            small.bottom_row() - crate::planet::CORE_FLOOR_ROWS + 1,
            "a small body's strata reach its core"
        );
        for radius in [1_500, 5_000, 9_000] {
            let big = ground(theme, radius);
            assert_eq!(big.core_floor_row(), strata_rows(radius));
            assert!((800..=2_500).contains(&big.core_floor_row()));
        }
        let big = ground(theme, 5_000);
        assert_eq!(
            kind(&big, 100, big.core_floor_row() + 40),
            TileKind::Bedrock
        );
        let top = big.core_chamber_top();
        let open = (0..400)
            .filter(|column| kind(&big, *column, top + 3).is_open())
            .count();
        assert!(open > 300, "{theme:?}: {open}");
        let names: Vec<&str> = (0..big.core_chamber_top())
            .step_by(5)
            .map(|row| big.layer_name(200, row))
            .collect();
        for stratum in crate::planet::theme(theme).strata {
            assert!(names.contains(&stratum.name), "{theme:?} {}", stratum.name);
        }
        // Every stratum's rule is its own.
        let rules: Vec<_> = life::outland_theme(theme)
            .expect("life")
            .strata
            .iter()
            .map(|stratum| stratum.rule)
            .collect();
        for (index, rule) in rules.iter().enumerate() {
            assert!(!rules[index + 1..].contains(rule));
        }
    }
}

#[test]
fn the_fungal_world_has_mushrooms_grottos_vents_and_the_mother_cap() {
    let planet = ground(PlanetThemeId::FUNGAL, 3_000);
    let width = i64::from(planet.strip_width());
    // Giant mushrooms: stalks under springy caps, puffballs on the ground.
    let mushrooms = (0..width / fungal::MUSHROOM_CELL)
        .filter_map(|cell| fungal::mushroom_in_cell(&planet, cell))
        .collect::<Vec<_>>();
    assert!(mushrooms.len() > 100, "{}", mushrooms.len());
    let mushroom = mushrooms[0];
    let centre = mushroom.column + mushroom.width / 2;
    assert!(is(&planet, centre, mushroom.top, OutlandTile::CapFlesh));
    assert!(is(
        &planet,
        mushroom.column,
        mushroom.top + 3,
        OutlandTile::Stalk
    ));
    assert!(count(&planet, OutlandTile::Puffball, 4_000, -10..10) > 5);
    // The grottos glow; the vents breathe up open chimneys; the mat regrows (the rock is mat).
    let grottos = planet.core_floor_row() / 2;
    assert!(
        count(
            &planet,
            OutlandTile::Glowcap,
            4_000,
            grottos - 100..grottos + 100
        ) > 30
    );
    let deep = planet.core_floor_row() * 5 / 6;
    let vents = count(&planet, OutlandTile::SporeVent, 4_000, deep..deep + 60);
    assert!(vents > 4, "{vents}");
    let vent = (0..width)
        .find_map(|column| fungal::vent_in_cell(&planet, column, deep + 15))
        .expect("a vent");
    assert!(is(&planet, vent.column, vent.row, OutlandTile::SporeVent));
    assert!(
        kind(&planet, vent.column, vent.row - 3).is_open(),
        "its chimney"
    );
    assert_eq!(
        kind(&planet, 10, planet.surface_row(10) + 20),
        TileKind::Outland(OutlandTile::MyceliumMat)
    );
    // The Mother Cap: a stalk with a door, a dome of glowing flesh.
    let landmark = landmark_column(&planet);
    let floor = super::landmark_floor(&planet);
    assert!(is(&planet, landmark + 3, floor - 20, OutlandTile::Stalk));
    assert!(kind(&planet, landmark - 3, floor - 2).is_open(), "the door");
    let mut cap = 0;
    for row in floor - fungal::MOTHER_HEIGHT - 14..=floor - fungal::MOTHER_HEIGHT {
        for column in landmark - 30..=landmark + 30 {
            if is(&planet, column, row, OutlandTile::MotherCap) {
                cap += 1;
            }
        }
    }
    assert!(cap > 20, "glowing spots on the cap: {cap}");
}

#[test]
fn the_machine_world_has_terraces_belts_gears_and_power_vaults() {
    let planet = ground(PlanetThemeId::MACHINE, 3_000);
    let width = i64::from(planet.strip_width());
    // Terraces: the surface steps in whole plates.
    let steps = (0..width)
        .map(|column| planet.surface_row(i32::try_from(column).unwrap_or(0)))
        .collect::<Vec<_>>();
    assert!(
        steps
            .iter()
            .all(|row| row.rem_euclid(machine::TERRACE_ROWS) == 0 || steps.contains(row))
    );
    assert!(count(&planet, OutlandTile::Pylon, 4_000, -40..0) > 30);
    assert!(count(&planet, OutlandTile::Beacon, 4_000, -40..0) > 5);
    let crust = planet.core_floor_row() / 6;
    let belts = count(
        &planet,
        OutlandTile::ConveyorEast,
        4_000,
        crust - 30..crust + 30,
    ) + count(
        &planet,
        OutlandTile::ConveyorWest,
        4_000,
        crust - 30..crust + 30,
    );
    assert!(belts > 30, "{belts}");
    let works = planet.core_floor_row() / 2;
    assert!(count(&planet, OutlandTile::Gear, 4_000, works - 30..works + 30) > 30);
    // Power vaults in the Reactor Shell: three relays in the roof, never all lit, seals between.
    let shell = planet.core_floor_row() * 5 / 6;
    let vault = (0..width / machine::VAULT_CELL.0)
        .flat_map(|cell| [shell - 30, shell, shell + 30].map(|row| (cell, row)))
        .find_map(|(cell, row)| machine::vault_in_cell(&planet, cell * machine::VAULT_CELL.0, row))
        .expect("a vault");
    let relays = vault.relays(width);
    let lit = relays
        .iter()
        .filter(|(column, row)| is(&planet, i64::from(*column), *row, OutlandTile::RelayOn))
        .count();
    let dark = relays
        .iter()
        .filter(|(column, row)| is(&planet, i64::from(*column), *row, OutlandTile::RelayOff))
        .count();
    assert_eq!(lit + dark, 3, "{vault:?}");
    assert!(lit < 3);
    let ring = |tiles: i64| vault.column + tiles * vault.span;
    assert!(is(&planet, ring(1), vault.row, OutlandTile::PowerSeal));
    assert!(is(&planet, ring(0), vault.row + 2, OutlandTile::VaultWall));
    assert!(
        kind(&planet, ring(4), vault.row - 1).is_open(),
        "the control room"
    );
    assert!(is(
        &planet,
        ring(machine::VAULT_BOX.0 / 2),
        vault.row + machine::VAULT_BOX.1 - 2,
        OutlandTile::BuilderCache
    ));
    // The Reactor Spire: casing, a door, a shaft, a beacon crown.
    let landmark = landmark_column(&planet);
    let floor = super::landmark_floor(&planet);
    assert!(is(
        &planet,
        landmark + 4,
        floor - 30,
        OutlandTile::ReactorCasing
    ));
    assert!(
        kind(&planet, landmark, floor - 30).is_open(),
        "the spire's shaft"
    );
    assert!(is(
        &planet,
        landmark,
        floor - machine::SPIRE_HEIGHT - 5,
        OutlandTile::Beacon
    ));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "each of the world's features in turn"
)]
fn the_dead_world_has_cities_tombs_and_the_hollow_throne() {
    let planet = ground(PlanetThemeId::DEAD, 3_000);
    let width = i64::from(planet.strip_width());
    let cities = (0..width / dead::CITY_CELL)
        .filter_map(|cell| dead::city_in_cell(&planet, cell))
        .collect::<Vec<_>>();
    assert!(cities.len() >= 3, "{}", cities.len());
    let city = cities[0];
    let towers = dead::towers(&planet, &city);
    assert!(towers.len() >= 3);
    let tower = towers[0];
    let street = planet.surface_row(i32::try_from(tower.first).unwrap_or(0));
    assert!(is(
        &planet,
        tower.first + tower.width - 1,
        street - 2,
        OutlandTile::Petrified
    ));
    assert!(
        kind(&planet, tower.first, street - 2).is_open(),
        "a door at street level"
    );
    assert!(
        kind(&planet, tower.first + tower.width / 2, street - 3).is_open(),
        "inside"
    );
    let span = city.first + city.width;
    let glass = (city.first..span)
        .flat_map(|column| (-40..0).map(move |row| (column, row)))
        .filter(|(column, row)| is(&planet, *column, *row, OutlandTile::PetrifiedGlass))
        .count();
    assert!(glass > 10, "{glass}");
    // The Necropolis's tombs: brick, a seal, a sarcophagus.
    let necropolis = planet.core_floor_row() / 2;
    let seals = count(
        &planet,
        OutlandTile::TombSeal,
        4_000,
        necropolis - 36..necropolis + 36,
    );
    assert!(seals > 6, "{seals}");
    assert!(
        count(
            &planet,
            OutlandTile::Sarcophagus,
            4_000,
            necropolis - 36..necropolis + 36
        ) > 3
    );
    // A tomb deep in a ring band where a ring tile spans two strip columns keeps its shape.
    let deep = (0..4_000_i64)
        .step_by(usize::try_from(dead::TOMB_CELL.0).unwrap_or(72))
        .flat_map(|column| (0..40).map(move |cell| (column, cell * dead::TOMB_CELL.1 + 20)))
        .find_map(|(column, row)| {
            (planet.grid().span_at(row) > 1 && planet.stratum_index(5, row) == 1)
                .then(|| dead::tomb_at(&planet, column + 30, row))
                .flatten()
        });
    if let Some(tomb) = deep {
        for (column, row) in tomb.seal(width) {
            assert!(
                is(&planet, i64::from(column), row, OutlandTile::TombSeal),
                "{tomb:?}"
            );
        }
    }
    // Ash on the plains; the Ash Beds hold drifts of it.
    let column = 5;
    let surface = planet.surface_row(column);
    assert!(is(
        &planet,
        i64::from(column),
        surface,
        OutlandTile::AshDrift
    ));
    // The throne: steps of petrified stone, a shaft down its heart.
    let landmark = landmark_column(&planet);
    let floor = super::landmark_floor(&planet);
    assert!(is(
        &planet,
        landmark + 20,
        floor - 2,
        OutlandTile::Petrified
    ));
    assert!(
        kind(&planet, landmark, floor - 10).is_open(),
        "the stair-shaft"
    );
    // Ghost lights drift only through the Grave Mantle, the same at the same time.
    let mantle = planet.core_floor_row() * 5 / 6;
    let wisps = (0..20)
        .flat_map(|cell| life::wisps_near(&planet, cell * 120, mantle, 40_000))
        .count();
    assert!(wisps > 10, "{wisps}");
    let _ = width;
    assert_eq!(
        life::wisps_near(&planet, 300, mantle, 12_345),
        life::wisps_near(&planet, 300, mantle, 12_345)
    );
    assert_ne!(
        life::wisps_near(&planet, 300, mantle, 12_345),
        life::wisps_near(&planet, 300, mantle, 18_345)
    );
    assert!(
        life::wisps_near(&planet, 300, 40, 12_345).is_empty(),
        "not in the Ash Beds"
    );
}

/// Every lair: an open hall with the seal on its floor, a sealed vault holding the relic,
/// a shaft up to the landmark's floor.
fn assert_lair(planet: &PlanetGround, lair: &BossLair) {
    let (middle, row) = lair.middle();
    assert!(kind(planet, middle + 10, row).is_open());
    assert!(is(
        planet,
        i64::from(lair.seal.0),
        lair.seal.1,
        OutlandTile::BossSeal
    ));
    for (column, door_row) in lair.door {
        assert!(is(
            planet,
            i64::from(column),
            door_row,
            OutlandTile::VaultDoor
        ));
    }
    assert!(is(
        planet,
        i64::from(lair.relic.0),
        lair.relic.1,
        lair.boss.relic
    ));
    let floor = super::landmark_floor(planet);
    for shaft in floor + 2..lair.top {
        assert!(
            kind(planet, middle, shaft).is_open(),
            "the shaft at {shaft}"
        );
    }
}

#[test]
fn every_theme_boss_has_a_lair_under_its_landmark_and_none_on_small_bodies() {
    for theme in OUTLAND_THEMES {
        for radius in [1_500, 3_000, 6_000] {
            let planet = ground(theme, radius);
            let lair = boss_lair(&planet).expect("a lair");
            assert_eq!(lair.centre, landmark_column(&planet));
            assert_eq!(lair.boss.theme, theme);
            assert_lair(&planet, &lair);
        }
        assert!(boss_lair(&ground(theme, 600)).is_none());
    }
    let kinds: Vec<_> = super::THEME_BOSSES.iter().map(|boss| boss.kind).collect();
    assert!(
        kinds
            .iter()
            .all(|kind| kind.is_outland_boss() && kind.stats().elite)
    );
}

#[test]
fn every_outland_world_has_a_level_settlement_and_a_builders_outpost() {
    for theme in OUTLAND_THEMES {
        for radius in [450, 2_000, 6_000] {
            let planet = ground(theme, radius);
            let site = world_of(theme)
                .and_then(|world| world.settlement_site(&planet))
                .expect("a site");
            for column in site.first..site.first + site.width {
                assert_eq!(planet.surface_row(column), site.floor, "{theme:?} {radius}");
                for row in site.floor - 12..site.floor {
                    assert!(
                        kind(&planet, i64::from(column), row).is_open(),
                        "{theme:?} {radius}: ({column}, {row}) is {:?}",
                        kind(&planet, i64::from(column), row)
                    );
                }
            }
            let outposts = builder_outposts(&planet);
            assert_eq!(outposts.len(), 1);
            let outpost = outposts[0];
            let (gx, gy) = outpost.glyph;
            assert!(
                is(&planet, i64::from(gx), gy, OutlandTile::BuilderGlyph),
                "{theme:?}"
            );
            let (cx, cy) = outpost.cache;
            assert!(
                is(&planet, i64::from(cx), cy, OutlandTile::BuilderCache),
                "{theme:?}"
            );
        }
    }
    assert!(
        builder_outposts(&PlanetGround::new(PlanetParams::new(
            1,
            900,
            PlanetThemeId::JUNGLE
        )))
        .is_empty()
    );
}

#[test]
fn small_bodies_hide_a_secret_and_planets_do_not() {
    for theme in OUTLAND_THEMES {
        let planet = ground(theme, 500);
        let column = secret_column(&planet);
        let surface = planet.surface_row(i32::try_from(column).unwrap_or(0));
        assert!(
            (surface..surface + SECRET_DEPTH + 6).any(|row| is(
                &planet,
                column,
                row,
                OutlandTile::Cache
            )),
            "{theme:?}"
        );
        assert!(!kind(&planet, column, surface - 1).is_open(), "the marker");
        let big = ground(theme, 3_000);
        let column = secret_column(&big);
        let surface = big.surface_row(i32::try_from(column).unwrap_or(0));
        assert!(!(surface..surface + 30).any(|row| is(&big, column, row, OutlandTile::Cache)));
    }
}

#[test]
fn the_strip_wraps_seamlessly() {
    for theme in OUTLAND_THEMES {
        let planet = ground(theme, 800);
        let width = i64::from(planet.strip_width());
        for row in -20..60 {
            assert_eq!(
                kind(&planet, width + 5, row),
                kind(&planet, 5, row),
                "{theme:?}"
            );
        }
    }
}

#[test]
fn weather_is_registered_and_each_theme_blows_its_own() {
    use crate::bodylife::weather::{forecast, weather_kind};
    for id in [
        hooks::SPORE_DRIFT,
        hooks::GLOWFALL,
        hooks::ION_STORM,
        hooks::GREY_WIND,
        hooks::GHOSTLIGHT,
    ] {
        assert!(hooks::OUTLAND_WEATHER_IDS.contains(&id.0));
        assert_eq!(weather_kind(id).id, id);
        assert_eq!(weather_kind(id).track, "planets_c");
    }
    for theme in OUTLAND_THEMES {
        let table = crate::planet::hooks(theme).weather_table();
        let storm = life::outland_theme(theme).expect("life").storm;
        let mut seen = 0;
        for spell in 0..400_u64 {
            let now = forecast(
                77,
                table,
                crate::space::Atmosphere::Thin,
                spell * crate::bodylife::weather::SPELL_MILLIS + 60_000,
                false,
            );
            if now.kind == storm {
                seen += 1;
            }
        }
        assert!((40..300).contains(&seen), "{theme:?}: {seen}");
    }
}

#[test]
fn hazards_are_tagged_for_the_planet_gear() {
    use crate::station::hazard::PlanetHazard as Gear;
    let tags = |theme| crate::planet::hooks(theme).hazard_tags().to_vec();
    assert_eq!(tags(PlanetThemeId::FUNGAL), vec![(Gear::ToxicAir, 1)]);
    assert_eq!(tags(PlanetThemeId::MACHINE), vec![(Gear::Heat, 1)]);
    assert_eq!(tags(PlanetThemeId::DEAD), vec![(Gear::Cold, 1)]);
    // Each stratum's hazards are ones the gear (or the Descent's systems) counter.
    for theme in OUTLAND_THEMES {
        for stratum in life::outland_theme(theme).expect("life").strata {
            for hazard in stratum.hazards {
                assert!(hazard.gear_hazard().is_some(), "{theme:?} {hazard:?}");
            }
        }
    }
}

#[test]
fn the_space_generators_tags_get_the_outland_worlds() {
    use crate::space::Theme;
    assert_eq!(
        crate::place::planet_theme_of(Theme::Fungal),
        PlanetThemeId::FUNGAL
    );
    assert_eq!(
        crate::place::planet_theme_of(Theme::MachineWorld),
        PlanetThemeId::MACHINE
    );
    assert_eq!(
        crate::place::planet_theme_of(Theme::DeadWorld),
        PlanetThemeId::DEAD
    );
}

#[test]
fn the_outland_themes_appear_in_the_space_report() {
    let text = crate::space::report::space_report(0x5EED, 2);
    for line in [
        "Fungal World (world): Mycelium Mat, Glowcap Grottos, Sporeheart Deep",
        "Machine World (world): Plate Crust, Gear Works, Reactor Shell",
        "Dead World (world): Ash Beds, The Necropolis, Grave Mantle",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}

#[test]
fn the_rhythms_telegraph_before_they_bite() {
    use life::{GearPhase, VentPhase, gear_phase, vent_phase};
    // The gears always clank before they turn.
    let mut last = gear_phase(0);
    for millis in (0..40_000).step_by(50) {
        let now = gear_phase(millis);
        if now == GearPhase::Turning && last != GearPhase::Turning {
            assert_eq!(last, GearPhase::Clank);
        }
        last = now;
    }
    // A vent always swells before it blasts.
    let planet = ground(PlanetThemeId::FUNGAL, 2_000);
    let mut last = vent_phase(&planet, 77, 0);
    for millis in (0..60_000).step_by(50) {
        let now = vent_phase(&planet, 77, millis);
        if now == VentPhase::Blasting && last != VentPhase::Blasting {
            assert_eq!(last, VentPhase::Swelling);
        }
        last = now;
    }
}

#[test]
#[ignore = "prints an ASCII map round each landmark and lair"]
fn print_landmarks() {
    for theme in OUTLAND_THEMES {
        let planet = ground(theme, 3_000);
        let centre = landmark_column(&planet);
        let floor = planet.surface_row(i32::try_from(centre).unwrap_or(0));
        println!("{theme:?} landmark at {centre}, floor {floor}");
        for row in floor - 76..floor + 74 {
            let line: String = (centre - 48..centre + 48)
                .map(|column| match kind(&planet, column, row) {
                    TileKind::Air => ' ',
                    TileKind::Outland(
                        OutlandTile::Stalk | OutlandTile::ReactorCasing | OutlandTile::Petrified,
                    ) => '#',
                    TileKind::Outland(OutlandTile::CapFlesh) => '*',
                    TileKind::Outland(OutlandTile::MotherCap | OutlandTile::Beacon) => 'o',
                    TileKind::Outland(OutlandTile::BossSeal) => 'S',
                    TileKind::Outland(OutlandTile::VaultDoor) => 'D',
                    TileKind::Outland(OutlandTile::VaultWall) => 'W',
                    TileKind::Outland(
                        OutlandTile::SporeCrown
                        | OutlandTile::ReactorHeart
                        | OutlandTile::ThroneRelic,
                    ) => '@',
                    _ => '.',
                })
                .collect();
            println!("{row:5} {line}");
        }
    }
}
