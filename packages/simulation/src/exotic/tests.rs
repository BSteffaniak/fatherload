//! The exotic worlds' generation: registration, purity and digests, depth and strata, each
//! world's surface, sites, landmark, secret, rules and schedules.

use super::{
    ExoticRule, ExoticStyle, ExoticTile, boss, builder_outpost, gas_giant, landmark_site, locked,
    ring,
    rules::{self, Zone, zone_at},
    secret, settlement_site, style_of,
};
use crate::{
    planet::{BodyGround, PlanetGround, PlanetParams},
    terrain::{ChunkPosition, TileKind, TilePosition},
};

const SEED: u64 = 0x0D13_E000;

fn ground(style: ExoticStyle, radius: u32) -> PlanetGround {
    PlanetGround::new(PlanetParams::new(SEED, radius, style.theme()))
}

fn tile(ground: &PlanetGround, x: i32, y: i32) -> TileKind {
    ground.generated_tile(x, y).kind
}

/// A coarse map of `ground` round `centre` (for looking at it: `--ignored --nocapture`).
fn sketch(ground: &PlanetGround, centre: i32, half: i32, top: i32, bottom: i32, step: i32) {
    for y in (top..bottom).step_by(usize::try_from(step).unwrap_or(1)) {
        let row: String = (centre - half..centre + half)
            .step_by(usize::try_from(step).unwrap_or(1))
            .map(|x| match tile(ground, x, y) {
                TileKind::Air => ' ',
                TileKind::Fluid(_) => '~',
                TileKind::Ore(_) => '$',
                TileKind::Exotic(found) => match found {
                    ExoticTile::CloudIce => '.',
                    ExoticTile::StormCloud => ':',
                    ExoticTile::DeepHaze => '=',
                    ExoticTile::SootCloud => ',',
                    ExoticTile::SkyDeck => '_',
                    ExoticTile::CityHull => 'H',
                    ExoticTile::LiftBladder => 'o',
                    ExoticTile::CityTower => '|',
                    ExoticTile::CityLantern => '*',
                    ExoticTile::EyeWall => '@',
                    ExoticTile::BossSeal => 'S',
                    ExoticTile::TetherPlate => '#',
                    ExoticTile::BuilderPlate => 'B',
                    ExoticTile::BuilderBeacon => 'b',
                    ExoticTile::LandmarkRelic | ExoticTile::RelicCache => 'R',
                    ExoticTile::RingIce | ExoticTile::IceSpike | ExoticTile::NitrogenIce => 'i',
                    ExoticTile::Ringstone | ExoticTile::MesaStone => 'm',
                    ExoticTile::Monolith => 'M',
                    ExoticTile::Glowmoss | ExoticTile::LichenSoil => 'g',
                    ExoticTile::GeyserVent => 'V',
                    ExoticTile::FaultRubble => 'r',
                    ExoticTile::ConduitLive | ExoticTile::ConduitJunction => 'c',
                    ExoticTile::AmmoniaPocket
                    | ExoticTile::StormPocket
                    | ExoticTile::DeepPocket
                    | ExoticTile::DiamondHail => '$',
                    _ => 'x',
                },
                _ => '?',
            })
            .collect();
        println!("{y:>6} {row}");
    }
}

#[test]
#[ignore = "a look at the worlds (run with --nocapture)"]
fn sketch_the_worlds() {
    let gas = ground(ExoticStyle::GasGiant, 7_600);
    let city = gas_giant::cities(&gas)[0];
    println!("gas giant city");
    sketch(
        &gas,
        i32::try_from(city.centre).unwrap_or(0),
        80,
        -70,
        40,
        1,
    );
    println!("gas giant eye");
    sketch(
        &gas,
        i32::try_from(gas_giant::eye_column(&gas)).unwrap_or(0),
        60,
        -45,
        45,
        1,
    );
    println!("gas giant bands");
    sketch(&gas, 400, 120, 0, 2_500, 25);
    let lock = ground(ExoticStyle::Locked, 2_000);
    let width = lock.strip_width();
    println!("locked: day {width}");
    sketch(&lock, 0, 100, -40, 30, 1);
    println!("locked: twilight");
    sketch(&lock, width / 4, 100, -40, 30, 1);
    println!("locked: night");
    sketch(&lock, width / 2, 100, -40, 30, 1);
    println!("locked: gate");
    sketch(&lock, width * 3 / 4, 60, -45, 10, 1);
    println!("locked: strata");
    sketch(&lock, 0, 60, 0, 900, 12);
    let ringed = ground(ExoticStyle::Ring, 3_000);
    println!("ring: tether");
    sketch(
        &ringed,
        i32::try_from(ring::tether_column(&ringed)).unwrap_or(0),
        90,
        -140,
        20,
        2,
    );
    println!("ring: plains");
    sketch(&ringed, 300, 120, -140, 20, 2);
}

#[test]
fn the_three_themes_are_registered_and_the_space_tags_land_on_them() {
    use crate::space::Theme;
    for (tag, style) in [
        (Theme::GasGiant, ExoticStyle::GasGiant),
        (Theme::TidallyLocked, ExoticStyle::Locked),
        (Theme::RingWorld, ExoticStyle::Ring),
    ] {
        let id = crate::place::planet_theme_of(tag);
        assert_eq!(id, style.theme());
        assert_eq!(style_of(id), Some(style));
        let data = crate::planet::theme(id);
        assert_eq!(data.id, id);
        assert_eq!(data.strata.len(), 4);
        let hooks = crate::planet::hooks(id);
        assert_eq!(hooks.id(), id);
        assert!(hooks.world().is_some(), "{style:?} builds its own chunks");
        assert!(!hooks.weather_table().is_empty());
        assert!(!hooks.creature_spawns().is_empty());
        assert!(hooks.music_bed().name.starts_with("pd_"));
    }
    // The reserved slots (planets-c's 9-11) keep their ids.
    for id in 9..12 {
        assert_eq!(
            crate::planet::theme(crate::planet::PlanetThemeId(id)).id.0,
            id
        );
    }
}

/// The exotic worlds' generation is pinned: a digest of a strip of chunks of each world.
#[test]
fn generation_is_pure_and_pinned_by_a_digest() {
    let digest = |style: ExoticStyle| {
        let ground = ground(style, 600);
        let mut hash = 0xCBF2_9CE4_8422_2325_u64;
        for cx in 0..12 {
            for cy in -12..40 {
                for tile in ground.chunk_tiles(ChunkPosition { x: cx, y: cy }) {
                    hash ^= tile.code();
                    hash = hash.wrapping_mul(0x0100_0000_01B3);
                }
            }
        }
        hash
    };
    let digests = ExoticStyle::ALL.map(digest);
    assert_eq!(
        digests,
        ExoticStyle::ALL.map(digest),
        "regenerates bit for bit"
    );
    assert_eq!(
        digests, THEME_DIGESTS,
        "the exotic generation changed (update THEME_DIGESTS only on purpose): {digests:#x?}"
    );
}

/// Each world at R = 600 (see the digest test).
const THEME_DIGESTS: [u64; 3] = [
    0x9f6f_62df_c077_a1a8,
    0x6e39_b962_bedb_9bf3,
    0xbf2f_c4d2_5aad_2073,
];

#[test]
fn the_strip_wraps_seamlessly() {
    for style in ExoticStyle::ALL {
        let ground = ground(style, 600);
        let width = ground.strip_width();
        for y in [-30, -5, 0, 3, 40, 200] {
            for x in [0, 5, 17] {
                assert_eq!(
                    tile(&ground, x, y),
                    tile(&ground, x + width, y),
                    "{style:?} ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn strata_run_800_to_2500_rows_on_planets_and_to_the_core_on_small_bodies() {
    for style in ExoticStyle::ALL {
        let small = ground(style, 600);
        assert_eq!(
            small.core_floor_row(),
            small.bottom_row() - crate::planet::CORE_FLOOR_ROWS + 1,
            "{style:?}: a small body is strata to its true core"
        );
        for radius in [1_500_u32, 5_000, 9_000] {
            let big = ground(style, radius);
            let depth = big.core_floor_row();
            assert!(
                (800..=2_500).contains(&depth),
                "{style:?} R={radius}: {depth} rows of strata"
            );
        }
    }
    // Deeper on a bigger body.
    assert!(
        ground(ExoticStyle::Ring, 5_000).core_chamber_top()
            > ground(ExoticStyle::Ring, 2_500).core_chamber_top()
    );
}

#[test]
fn every_stratum_has_its_rule_and_the_zones_find_them() {
    for style in ExoticStyle::ALL {
        let ground = ground(style, 5_000);
        let depth = ground.core_chamber_top();
        let column = 37;
        let mut seen = Vec::new();
        for row in (0..depth).step_by(10) {
            if let Zone::Stratum(index) =
                zone_at(&ground, style, TilePosition { x: column, y: row })
                && !seen.contains(&index)
            {
                seen.push(index);
            }
        }
        assert_eq!(seen, vec![0, 1, 2, 3], "{style:?}");
        for index in 0..4 {
            let rule = style.rules()[index];
            assert!(!rule.hint().is_empty());
        }
    }
}

#[test]
fn the_gas_giant_has_no_ground_but_clouds_cities_and_bands_you_fly_through() {
    let ground = ground(ExoticStyle::GasGiant, 7_600);
    // Mostly open gas under the cloud deck: a rig can fall through the bands.
    let mut open = 0;
    let mut total = 0;
    let mut pockets = 0;
    for row in (40..2_000).step_by(7) {
        for column in (0..600).step_by(5) {
            total += 1;
            match tile(&ground, column, row) {
                TileKind::Air => open += 1,
                TileKind::Exotic(
                    ExoticTile::AmmoniaPocket
                    | ExoticTile::StormPocket
                    | ExoticTile::DeepPocket
                    | ExoticTile::DiamondHail,
                ) => pockets += 1,
                _ => {}
            }
        }
    }
    assert!(open * 100 / total >= 35, "{open} of {total} open");
    assert!(pockets > 0, "gas pockets to harvest");
    // Three cities float above the cloud tops on decks over keels.
    for city in gas_giant::cities(&ground) {
        let c = i32::try_from(city.centre).unwrap_or(0);
        assert_eq!(ground.surface_row(c), gas_giant::DECK_ROW);
        assert_eq!(
            tile(&ground, c, gas_giant::DECK_ROW),
            TileKind::Exotic(ExoticTile::SkyDeck)
        );
        assert!(gas_giant::DECK_ROW < gas_giant::cloud_top(&ground, city.centre) - 20);
        assert!(matches!(
            tile(&ground, c, gas_giant::DECK_ROW + 6),
            TileKind::Exotic(ExoticTile::LiftBladder | ExoticTile::CityHull)
        ));
    }
    // The settlement is the first city's clear, level deck.
    let site = settlement_site(&ground).expect("a site");
    assert_eq!(site.floor, gas_giant::DECK_ROW);
    for x in site.first..site.first + site.width {
        assert_eq!(ground.surface_row(x), gas_giant::DECK_ROW, "column {x}");
        assert_eq!(tile(&ground, x, gas_giant::DECK_ROW - 1), TileKind::Air);
    }
}

#[test]
fn the_locked_world_has_a_day_side_a_twilight_ring_and_a_night_side() {
    let ground = ground(ExoticStyle::Locked, 2_000);
    let width = i64::from(ground.strip_width());
    assert_eq!(locked::side_of(&ground, 0), locked::Side::Day);
    assert_eq!(locked::side_of(&ground, width / 4), locked::Side::Twilight);
    assert_eq!(locked::side_of(&ground, width / 2), locked::Side::Night);
    assert_eq!(
        locked::side_of(&ground, width * 3 / 4),
        locked::Side::Twilight
    );
    // The surfaces differ: hardpan by day, glowmoss in the twilight, nitrogen ice by night.
    let top = |column: i64| {
        let c = i32::try_from(column).unwrap_or(0);
        tile(&ground, c, ground.surface_row(c))
    };
    let day: Vec<TileKind> = (0..40).map(|x| top(x * 3)).collect();
    assert!(day.iter().any(|kind| matches!(
        kind,
        TileKind::Exotic(ExoticTile::Hardpan | ExoticTile::SaltCrust)
    )));
    assert_eq!(top(width / 4), TileKind::Exotic(ExoticTile::Glowmoss));
    let night: Vec<TileKind> = (0..40).map(|x| top(width / 2 + x * 3)).collect();
    assert!(night.iter().any(|kind| matches!(
        kind,
        TileKind::Exotic(ExoticTile::NitrogenIce | ExoticTile::FrostRegolith)
    )));
    // The halves play apart: the crust burns by day and freezes by night.
    let probe = |column: i64| TilePosition {
        x: i32::try_from(column).unwrap_or(0),
        y: ground.surface_row(i32::try_from(column).unwrap_or(0)) - 1,
    };
    let noon = rules::exposure_at(&ground, probe(0));
    let midnight = rules::exposure_at(&ground, probe(width / 2));
    let dusk = rules::exposure_at(&ground, probe(width / 4));
    assert!(noon.heat > 0.0 && noon.cold == 0.0);
    assert!(midnight.cold > 0.0 && midnight.heat == 0.0);
    assert!(dusk.heat == 0.0 && dusk.cold == 0.0, "the twilight is mild");
    // The settlement stands in the twilight.
    let site = settlement_site(&ground).expect("a site");
    assert_eq!(
        locked::side_of(&ground, i64::from(site.centre())),
        locked::Side::Twilight
    );
}

#[test]
fn the_ring_world_has_a_skyring_of_debris_and_the_tether() {
    let ground = ground(ExoticStyle::Ring, 3_000);
    let mut islands = 0;
    for column in (0..ground.strip_width()).step_by(4) {
        for row in (ring::SKYRING_TOP..=ring::SKYRING_BOTTOM).step_by(2) {
            if !tile(&ground, column, row).is_open() {
                islands += 1;
            }
        }
    }
    assert!(islands > 100, "{islands} debris tiles in the Skyring");
    assert!(
        ground.strip_top() < ring::SKYRING_TOP,
        "the strip's sky holds the ring"
    );
    let tether = i32::try_from(ring::tether_column(&ground)).unwrap_or(0);
    let floor = ground.surface_row(tether);
    for row in ring::ANCHOR_ROW..floor {
        assert_eq!(
            tile(&ground, tether, row),
            TileKind::Exotic(ExoticTile::TetherPlate),
            "the Tether at row {row}"
        );
    }
    assert!(crate::terrain::is_unbreakable(TileKind::Exotic(
        ExoticTile::TetherPlate
    )));
}

#[test]
fn each_world_has_a_landmark_a_boss_arena_an_outpost_and_level_sites() {
    for style in ExoticStyle::ALL {
        let ground = ground(style, 3_000);
        let landmark = landmark_site(&ground).expect("a landmark");
        let arena = boss::arena(&ground).expect("an arena");
        assert_eq!(
            tile(&ground, arena.seal.x, arena.seal.y),
            TileKind::Exotic(ExoticTile::BossSeal),
            "{style:?}"
        );
        assert!(arena.contains(arena.home, ground.strip_width(), 0));
        assert!(arena.contains(
            TilePosition {
                x: landmark.centre(),
                y: landmark.floor - 1
            },
            ground.strip_width(),
            0
        ));
        let outpost = builder_outpost(&ground).expect("an outpost");
        assert_eq!(
            tile(&ground, outpost.beacon.x, outpost.beacon.y),
            TileKind::Exotic(ExoticTile::BuilderBeacon),
            "{style:?} outpost"
        );
        let site = settlement_site(&ground).expect("a site");
        if style != ExoticStyle::GasGiant {
            let rows: Vec<i32> = (site.first..site.first + site.width)
                .map(|x| ground.surface_row(x))
                .collect();
            let spread = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
            assert!(spread <= 1, "{style:?} settlement spread {spread}");
            for x in site.first..site.first + site.width {
                assert_eq!(tile(&ground, x, ground.surface_row(x) - 1), TileKind::Air);
            }
        }
        // The theme's own site finder is what the aliens build on.
        let hooks = crate::planet::hooks(style.theme());
        assert_eq!(
            hooks.settlement_site(&ground).map(|(centre, _)| centre),
            Some(site.centre())
        );
    }
}

#[test]
fn small_bodies_hide_one_secret_and_planets_none() {
    for style in ExoticStyle::ALL {
        let small = ground(style, 600);
        let found = secret(&small).expect("a secret on a small body");
        assert_eq!(
            tile(&small, found.prize.x, found.prize.y),
            TileKind::Exotic(ExoticTile::RelicCache),
            "{style:?}"
        );
        assert!(secret(&ground(style, 3_000)).is_none());
    }
}

#[test]
fn hazards_are_tagged_for_the_planet_gear() {
    use crate::station::hazard::PlanetHazard as Gear;
    let tags = |style: ExoticStyle| crate::planet::hooks(style.theme()).hazard_tags();
    assert!(
        tags(ExoticStyle::GasGiant)
            .iter()
            .any(|(h, _)| *h == Gear::HighGravity)
    );
    assert!(
        tags(ExoticStyle::Locked)
            .iter()
            .any(|(h, _)| *h == Gear::Heat)
    );
    assert!(
        tags(ExoticStyle::Locked)
            .iter()
            .any(|(h, _)| *h == Gear::Cold)
    );
    assert!(
        tags(ExoticStyle::Ring)
            .iter()
            .any(|(h, _)| *h == Gear::HighGravity)
    );
    for style in ExoticStyle::ALL {
        assert!(crate::planet::hooks(style.theme()).rules_own_hazards());
        assert!(!super::hazards(style).is_empty());
    }
    assert_eq!(
        ExoticRule::CrushingPressure.hazard(),
        Some(crate::frontier::PlanetHazard::HighGravity)
    );
}

#[test]
fn every_tile_round_trips_its_code() {
    for found in ExoticTile::ALL {
        assert_eq!(ExoticTile::from_code(found.code()), Some(found));
        let kind = TileKind::Exotic(found);
        let tile = crate::terrain::Tile {
            kind,
            durability: crate::terrain::tile_durability(kind),
        };
        let code = tile.code();
        assert_eq!((code >> 8) & !0xFF, (0x3200 + found.code()) & !0xFF);
    }
    let mut codes: Vec<u64> = ExoticTile::ALL.iter().map(|t| t.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), ExoticTile::ALL.len());
}

#[test]
fn the_schedules_are_pure_and_tell_before_they_strike() {
    let gas = ground(ExoticStyle::GasGiant, 7_600);
    let belt = gas_giant::band_top_row(&gas, 1) + 80;
    let probe = TilePosition { x: 300, y: belt };
    const { assert!(rules::BOLT_TELL_MILLIS >= 1_000, "a fair tell") };
    let bolts = rules::bolts_near(&gas, probe, 3, 0, 60_000, 0.0);
    assert!(!bolts.is_empty(), "the Storm Belt has bolts");
    assert_eq!(bolts, rules::bolts_near(&gas, probe, 3, 0, 60_000, 0.0));
    for bolt in &bolts {
        assert_eq!(
            bolt.strike_millis() - bolt.tell_millis,
            rules::BOLT_TELL_MILLIS
        );
    }
    let hail_row = gas_giant::band_top_row(&gas, 3) + 60;
    let hail = rules::fallers_near(
        &gas,
        ExoticStyle::GasGiant,
        TilePosition {
            x: 300,
            y: hail_row,
        },
        3,
        0,
        60_000,
        0.0,
    );
    assert!(!hail.is_empty(), "diamond hail falls in the Diamond Rain");
    let ringed = ground(ExoticStyle::Ring, 3_000);
    let meteors = rules::fallers_near(
        &ringed,
        ExoticStyle::Ring,
        TilePosition { x: 300, y: -2 },
        3,
        0,
        60_000,
        0.0,
    );
    assert!(!meteors.is_empty(), "meteors fall on the plains");
    for faller in meteors.iter().chain(&hail) {
        assert!(faller.land_millis >= rules::FALL_TELL_MILLIS);
    }
    // Geysers hiss before they blow; quakes rumble before they shock.
    let lock = ground(ExoticStyle::Locked, 2_000);
    let vent = (0..200)
        .flat_map(|cx| (0..40).filter_map(move |cy| locked::vent_in(&lock, cx, cy)))
        .find(|vent| lock.stratum_index(vent.x, vent.y) == 1)
        .expect("a vent in the Geyser Fields");
    let mut hissed = false;
    for millis in (0..30_000).step_by(100) {
        match rules::geyser_phase(&lock, vent, millis) {
            rules::GeyserPhase::Hissing => hissed = true,
            rules::GeyserPhase::Blowing => assert!(hissed || millis < 3_000),
            rules::GeyserPhase::Quiet => {}
        }
    }
    let shocks: Vec<u64> = (0..200_000_u64)
        .step_by(50)
        .filter(|millis| rules::quake_shock(&lock, *millis, millis + 50))
        .collect();
    assert!(shocks.len() >= 4, "{shocks:?}");
    for shock in shocks {
        if shock > rules::QUAKE_RUMBLE_MILLIS {
            assert_eq!(
                rules::quake_at(&lock, shock - 1_000),
                Some(false),
                "rumbling first"
            );
        }
    }
}

#[test]
fn the_boss_marks_land_where_told() {
    let ground = ground(ExoticStyle::GasGiant, 7_600);
    let arena = boss::arena(&ground).expect("an arena");
    let target = TilePosition {
        x: arena.centre + 5,
        y: arena.floor - 4,
    };
    let width = ground.strip_width();
    let marks = boss::marks(&arena, boss::BossAttack::BoltSalvo, target, false);
    assert_eq!(marks.len(), 3);
    assert!(
        marks
            .iter()
            .any(|mark| boss::mark_hits(mark, target, width))
    );
    let away = TilePosition {
        x: target.x + 3,
        y: target.y,
    };
    assert!(
        !marks.iter().any(|mark| boss::mark_hits(mark, away, width)),
        "a gap to dodge into"
    );
    // The slam passes under a rig in the air.
    let lock = super::tests::ground(ExoticStyle::Locked, 2_000);
    let arena = boss::arena(&lock).expect("an arena");
    let slam = boss::marks(&arena, boss::BossAttack::SunSlam, arena.home, false);
    let standing = TilePosition {
        x: arena.home.x + 2,
        y: arena.floor - 1,
    };
    let flying = TilePosition {
        x: arena.home.x + 2,
        y: arena.floor - 5,
    };
    assert!(
        slam.iter()
            .any(|mark| boss::mark_hits(mark, standing, lock.strip_width()))
    );
    assert!(
        !slam
            .iter()
            .any(|mark| boss::mark_hits(mark, flying, lock.strip_width()))
    );
    assert_eq!(boss::THEME_BOSSES.len(), 3);
}

#[test]
fn real_space_bodies_of_each_tag_ground_as_their_world_and_the_report_lists_them() {
    use crate::space::{Space, Theme};
    let space = Space::shared(0x00C0_FFEE);
    let mut found = Vec::new();
    for coords in crate::space::report::sectors_around_home(2) {
        let sector = space.sector(coords);
        for system in &sector.systems {
            for body in &system.bodies {
                if matches!(
                    body.theme,
                    Theme::GasGiant | Theme::TidallyLocked | Theme::RingWorld
                ) && !found.contains(&body.theme)
                {
                    found.push(body.theme);
                    let id = crate::place::planet_theme_of(body.theme);
                    assert!(style_of(id).is_some());
                }
            }
        }
    }
    assert!(found.len() >= 2, "{found:?}");
    let report = crate::space::report::space_report(0x00C0_FFEE, 1);
    for name in ["Gas Giant", "Locked World", "Ring World"] {
        assert!(report.contains(name), "the space report names {name}");
    }
}

#[test]
fn the_locked_worlds_climate_scorches_by_day_freezes_by_night_and_is_mild_in_the_twilight() {
    use crate::bodylife::{
        BodyLife,
        climate::{COLD_START_C, HEAT_START_C},
    };
    // A lone locked world keeps its sides (no spinning day), and the climate the HUD reads
    // and the rig's radiator and de-icer answer agrees with the sun-and-shadow rule.
    let ground = ground(ExoticStyle::Locked, 2_000);
    let width = i64::from(ground.strip_width());
    let life = BodyLife::of_planet(*ground.params());
    assert!(life.clock.tidally_locked());
    let at = |life: &BodyLife, column: i64| {
        life.climate
            .temperature_at(0, life.clock.elevation(0, column), 0)
    };
    // Whatever its star's heat, a locked body's terminator stays mild (the space body's mean
    // is held to the theme's band) while noon scorches and midnight freezes.
    for mean_c in [0, 20] {
        let mut life = life.clone();
        life.climate.mean_c = mean_c;
        let (noon, dusk, midnight) = (at(&life, 0), at(&life, width / 4), at(&life, width / 2));
        assert!(noon > HEAT_START_C, "noon {noon}");
        assert!(midnight < COLD_START_C, "midnight {midnight}");
        assert!((-20..=40).contains(&dusk), "terminator {dusk}");
    }
    // Other themes keep a lone world's spinning day.
    let rocky = BodyLife::of_planet(PlanetParams::new(
        SEED,
        2_000,
        crate::planet::PlanetThemeId::ROCKY,
    ));
    assert!(!rocky.clock.tidally_locked());
}
