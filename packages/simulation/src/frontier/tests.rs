//! Frontier world generation and rules tests (Wave 12 planets-b).

use super::*;
use crate::{
    planet::{BodyGround, PlanetParams},
    terrain::{TileKind, TilePosition},
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

/// A digest of `ground`'s generated tiles: a sparse sweep of the whole strip down to the
/// core and a dense window round the surface, its landmark and its settlement.
fn digest(ground: &PlanetGround) -> u64 {
    let width = ground.strip_width();
    let mut hash = 0xCBF2_9CE4_8422_2325_u64;
    let mut eat = |x: i32, y: i32| {
        let code = ground.generated_tile(x, y).code();
        hash = (hash ^ code ^ (u64::from(x.cast_unsigned()) << 32) ^ u64::from(y.cast_unsigned()))
            .wrapping_mul(0x0100_0000_01B3);
    };
    for x in (0..width).step_by(97) {
        for y in (ground.strip_top()..=ground.bottom_row()).step_by(13) {
            eat(x, y);
        }
    }
    let landmark = landmark(ground).expect("a frontier world").column;
    let settlement = settlement_site(ground).expect("a frontier world").column;
    for x0 in [0, landmark - 40, settlement - 10] {
        for x in x0..x0 + 80 {
            for y in -50..60 {
                eat(x, y);
            }
        }
    }
    hash
}

/// The theme digests: generation of every frontier theme at radius 600.
const THEME_DIGESTS: [(FrontierStyle, u64); 3] = [
    (FrontierStyle::Desert, 0x5025_1c50_e20c_1843),
    (FrontierStyle::Volcanic, 0x2341_f441_f8a7_6cf2),
    (FrontierStyle::Crystal, 0x0aa7_fd7b_ed64_40c9),
];

#[test]
fn generation_is_a_pure_function_and_each_theme_keeps_its_digest() {
    let mut moved = Vec::new();
    for (style, golden) in THEME_DIGESTS {
        let first = digest(&ground(style, 600));
        let again = digest(&PlanetGround::new(PlanetParams::new(
            0x00F0_0D5E,
            600,
            style.theme(),
        )));
        assert_eq!(first, again, "{style:?} generates the same twice");
        let other = digest(&PlanetGround::new(PlanetParams::new(7, 600, style.theme())));
        assert_ne!(first, other, "{style:?}: another seed, another world");
        println!("{style:?} digest {first:#018x}");
        moved.extend((first != golden).then(|| format!("{style:?}: {first:#018x}")));
    }
    assert!(moved.is_empty(), "the theme digests moved: {moved:?}");
}

#[test]
fn every_frontier_theme_is_registered_and_space_bodies_with_its_tag_get_it() {
    use crate::{
        planet::{PLANET_THEMES, theme},
        space::Theme,
    };
    for style in FrontierStyle::ALL {
        let registered = theme(style.theme());
        assert_eq!(registered.id, style.theme());
        assert_eq!(style_of(registered.id), Some(style));
        assert_eq!(registered.strata.len(), 4);
        assert!(PLANET_THEMES.iter().any(|found| found.id == style.theme()));
    }
    assert_eq!(
        crate::place::planet_theme_of(Theme::DesertDunes),
        DUNE_WORLD
    );
    assert_eq!(
        crate::place::planet_theme_of(Theme::Volcanic),
        VOLCANIC_WORLD
    );
    assert_eq!(crate::place::planet_theme_of(Theme::Crystal), CRYSTAL_WORLD);
    // A real generated body of each tag grounds as its frontier world.
    let seed = 0x0D3E_9DE5;
    let space = crate::space::Space::shared(seed);
    let mut found = std::collections::BTreeSet::new();
    for coords in crate::space::report::sectors_around_home(3) {
        let sector = space.sector(coords);
        for system in &sector.systems {
            for body in &system.bodies {
                let id = crate::space::SpaceBodyId {
                    sector: coords,
                    system: system.index,
                    body: body.index,
                };
                if let Some(params) = crate::place::planet_params_for(seed, id)
                    && let Some(style) = style_of(params.theme)
                {
                    found.insert(style);
                    let tag = match style {
                        FrontierStyle::Desert => Theme::DesertDunes,
                        FrontierStyle::Volcanic => Theme::Volcanic,
                        FrontierStyle::Crystal => Theme::Crystal,
                    };
                    assert_eq!(body.theme, tag);
                }
            }
        }
    }
    assert_eq!(found.len(), 3, "every frontier world near home: {found:?}");
}

#[test]
fn the_strip_wraps_seamlessly_round_every_frontier_world() {
    for style in FrontierStyle::ALL {
        let g = ground(style, 800);
        let width = g.strip_width();
        for x in [0, 1, 255, 511, width / 3, width - 1] {
            for y in [g.strip_top() + 2, -30, -10, -1, 0, 3, 40, 200] {
                assert_eq!(
                    g.generated_tile(x, y),
                    g.generated_tile(x + width, y),
                    "{style:?} ({x}, {y})"
                );
                assert_eq!(g.generated_tile(x, y), g.generated_tile(x - width, y));
            }
            assert_eq!(g.surface_row(x), g.surface_row(x + width));
        }
    }
}

#[test]
fn the_strata_scale_with_the_radius_and_run_down_to_the_core_chamber() {
    assert_eq!(strata_rows(400), 213);
    assert_eq!(strata_rows(1_000), 533);
    assert_eq!(strata_rows(1_500), 800);
    assert_eq!(strata_rows(5_000), 1_500);
    assert_eq!(strata_rows(10_000), 2_500);
    assert_eq!(strata_rows(40_000), 2_500);
    for style in FrontierStyle::ALL {
        for radius in [400, 1_000, 5_000] {
            let g = ground(style, radius);
            let column = 777 % g.strip_width();
            let surface = g.surface_row(column);
            let mut seen = Vec::new();
            for row in surface..g.core_chamber_top() {
                let index = g.stratum_index(column, row);
                if seen.last() != Some(&index) {
                    seen.push(index);
                }
            }
            assert_eq!(seen, vec![0, 1, 2, 3], "{style:?} R={radius}");
            // The first three share the authored depth; the fourth runs on to the core.
            let deep = (surface..g.core_chamber_top())
                .find(|row| g.stratum_index(column, *row) == 3)
                .expect("the last stratum");
            let authored = strata_rows(radius).min(g.core_chamber_top() - 8);
            assert!(
                (deep - surface - authored * 800 / 1_000).abs() <= 4 + surface.abs(),
                "{style:?} R={radius}: deep at {deep}"
            );
            assert_eq!(
                g.stratum_index(column, g.core_chamber_top() - 1),
                3,
                "{style:?} R={radius}"
            );
        }
    }
}

trait CodeKind {
    fn code_kind(self) -> u64;
}

impl CodeKind for TileKind {
    fn code_kind(self) -> u64 {
        crate::terrain::Tile {
            kind: self,
            durability: 0,
        }
        .code()
            >> 8
    }
}

#[test]
fn every_stratum_has_its_materials_ores_and_one_signature_rule() {
    for style in FrontierStyle::ALL {
        let g = ground(style, 1_000);
        let theme = g.theme();
        let rules = style.rules();
        let mut distinct = rules.to_vec();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "{style:?}: one rule a stratum");
        for (index, stratum) in theme.strata.iter().enumerate() {
            assert!(!stratum.ores.is_empty(), "{}", stratum.name);
            assert!(matches!(stratum.rock, TileKind::Frontier(_)));
            assert!(!rules[index].hint().is_empty());
            // Its rock, accent and ores all turn up in a sweep of it.
            let mut kinds = std::collections::BTreeSet::new();
            for x in (0..g.strip_width()).step_by(61) {
                let surface = g.surface_row(x);
                for row in (surface + 4..g.core_chamber_top()).step_by(2) {
                    if g.stratum_index(x, row) == index {
                        kinds.insert(g.generated_tile(x, row).kind.code_kind());
                    }
                }
            }
            for wanted in [stratum.rock, stratum.accent] {
                assert!(
                    kinds.contains(&wanted.code_kind()),
                    "{}: {wanted:?}",
                    stratum.name
                );
            }
            assert!(
                stratum
                    .ores
                    .iter()
                    .any(|(mineral, _)| kinds.contains(&TileKind::Ore(*mineral).code_kind())),
                "{}: no ore",
                stratum.name
            );
        }
    }
}

#[test]
fn each_surface_has_its_own_shape() {
    // Dunes: oases hold water ringed by palms.
    let dunes = ground(FrontierStyle::Desert, 1_000);
    let pools = generate::oases(&dunes);
    assert!(!pools.is_empty());
    let oasis = i32::try_from(pools[0]).expect("column");
    assert!(
        matches!(dunes.generated_tile(oasis, 2).kind, TileKind::Fluid(_)),
        "an oasis holds water"
    );
    assert!(
        (oasis - 24..oasis + 26).any(|x| (-12..0).any(
            |y| dunes.generated_tile(x, y).kind == TileKind::Frontier(FrontierTile::PalmTrunk)
        )),
        "palms ring an oasis"
    );
    // The Sleeping Sentinel's glowing eyes over the sand.
    let sentinel = landmark(&dunes).expect("landmark");
    assert!((sentinel.column - 6..sentinel.column + 6).any(|x| {
        (sentinel.top_row..0)
            .any(|y| dunes.generated_tile(x, y).kind == TileKind::Frontier(FrontierTile::SunGlyph))
    }));
    assert_eq!(
        dunes
            .generated_tile(sentinel.treasure.x, sentinel.treasure.y)
            .kind,
        TileKind::Frontier(FrontierTile::RuinCache)
    );
    // Volcanic: lava seas at the datum, cones with craters, the Caldera's lava lake.
    let volcanic = ground(FrontierStyle::Volcanic, 1_000);
    let seas = (0..volcanic.strip_width())
        .filter(|x| volcanic.generated_tile(*x, 0).kind == TileKind::Lava)
        .count();
    assert!(seas > 0, "lava seas");
    let caldera = landmark(&volcanic).expect("landmark");
    assert!(
        (caldera.column - 10..caldera.column + 10).any(|x| (caldera.top_row..caldera.top_row + 8)
            .any(|y| volcanic.generated_tile(x, y).kind == TileKind::Lava)),
        "the Caldera's lake"
    );
    assert_eq!(
        volcanic
            .generated_tile(caldera.treasure.x, caldera.treasure.y)
            .kind,
        TileKind::Frontier(FrontierTile::MagmaHeart)
    );
    // Crystal: spires over the meadows, the Choir Spire.
    let crystal = ground(FrontierStyle::Crystal, 1_000);
    let spires = (0..crystal.strip_width())
        .filter(|x| {
            crystal.generated_tile(*x, crystal.surface_row(*x) - 3).kind
                == TileKind::Frontier(FrontierTile::SpireCrystal)
        })
        .count();
    assert!(spires > 20, "{spires} spire columns");
    let choir = landmark(&crystal).expect("landmark");
    assert_eq!(
        crystal
            .generated_tile(choir.column + 2, choir.top_row + 2)
            .kind,
        TileKind::Frontier(FrontierTile::SpireCrystal)
    );
    assert_eq!(
        crystal
            .generated_tile(choir.treasure.x, choir.treasure.y)
            .kind,
        TileKind::Frontier(FrontierTile::GeodeCore)
    );
}

#[test]
fn landmarks_settlement_sites_and_the_small_bodies_secrets() {
    for style in FrontierStyle::ALL {
        for radius in [400, 900, 5_000] {
            let g = ground(style, radius);
            // The settlement site: flat, dry, open sky.
            let site = settlement_site(&g).expect("a site");
            for x in site.column..site.column + site.width {
                assert_eq!(g.surface_row(x), site.surface_row, "{style:?} R={radius}");
                assert_eq!(
                    g.generated_tile(x, site.surface_row - 1).kind,
                    TileKind::Air
                );
                assert!(!g.generated_tile(x, site.surface_row).kind.is_open());
            }
            // The landmark stands away from the settlement.
            let landmark = landmark(&g).expect("a landmark");
            let width = g.strip_width();
            let apart = (landmark.column - site.column).rem_euclid(width);
            assert!(apart.min(width - apart) > 64, "{style:?} R={radius}");
            // One secret on a small body, none on a planet.
            let secret = secret(&g);
            if radius <= SECRET_MAX_RADIUS {
                let secret = secret.expect("a secret");
                let prize = match style {
                    FrontierStyle::Desert => FrontierTile::RuinCache,
                    FrontierStyle::Volcanic => FrontierTile::MagmaHeart,
                    FrontierStyle::Crystal => FrontierTile::GeodeCore,
                };
                assert_eq!(
                    g.generated_tile(secret.prize.x, secret.prize.y).kind,
                    TileKind::Frontier(prize),
                    "{style:?} R={radius}"
                );
                // Its marker stands on the surface over it.
                let surface = g.surface_row(secret.column);
                assert!(!g.generated_tile(secret.column, surface - 1).kind.is_open());
            } else {
                assert!(secret.is_none());
            }
        }
    }
}

#[test]
fn a_light_well_lights_its_receptor_once_dug_a_path() {
    let g = ground(FrontierStyle::Crystal, 1_000);
    let wells = generate::light_wells(&g);
    assert!(!wells.is_empty(), "a crystal world has light wells");
    for well in wells {
        let mut dug = std::collections::BTreeSet::new();
        let read = |dug: &std::collections::BTreeSet<TilePosition>, at: TilePosition| {
            if dug.contains(&at) {
                Some(TileKind::Air)
            } else {
                Some(g.generated_tile(at.x, at.y).kind)
            }
        };
        assert_eq!(
            read(&dug, well.receptor),
            Some(TileKind::Frontier(FrontierTile::Receptor))
        );
        let dark = rules::trace_beam(&well, |at| read(&dug, at));
        assert!(!dark.lit, "the stone blocks the light at first");
        // Dig the light a path: down from the lens to the prism, then across.
        for y in well.lens_row + 1..well.prism.y {
            dug.insert(TilePosition { x: well.column, y });
        }
        let way = if well.east { 1 } else { -1 };
        let mut x = well.prism.x + way;
        while x != well.receptor.x {
            dug.insert(TilePosition { x, y: well.prism.y });
            x += way;
        }
        let lit = rules::trace_beam(&well, |at| read(&dug, at));
        assert!(
            lit.lit,
            "the beam turns at the prism and reaches the receptor"
        );
        assert!(lit.path.contains(&well.prism));
        // The vault is sealed all round with its prize inside.
        let (x0, x1, y0, y1) = well.vault;
        assert_eq!(
            g.generated_tile(x0, y0).kind,
            TileKind::Frontier(FrontierTile::VaultSeal)
        );
        assert!((x0..=x1).any(
            |x| g.generated_tile(x, y1 - 1).kind == TileKind::Frontier(FrontierTile::GeodeCore)
        ));
    }
}

#[test]
fn the_weather_telegraphs_every_storm_and_is_a_pure_schedule() {
    for style in FrontierStyle::ALL {
        let mut phases = Vec::new();
        let mut last = None;
        for second in 0..1_200_u64 {
            let weather = rules::weather_at(style, 99, second * 1_000);
            assert_eq!(weather, rules::weather_at(style, 99, second * 1_000));
            if last != Some(weather.phase) {
                phases.push((second, weather.phase));
                last = Some(weather.phase);
            }
            if weather.phase == rules::WeatherPhase::Storm {
                assert!(weather.strength_permille <= 1_000);
            }
        }
        // Calm, rising, storm, calm, ...: a storm never breaks without its telegraph.
        for pair in phases.windows(2) {
            let (at, phase) = pair[1];
            if phase == rules::WeatherPhase::Storm {
                assert_eq!(pair[0].1, rules::WeatherPhase::Rising);
                assert!(at - pair[0].0 >= rules::RISING_MILLIS / 1_000 - 1);
            }
        }
        assert!(
            phases
                .iter()
                .filter(|(_, phase)| *phase == rules::WeatherPhase::Storm)
                .count()
                >= 3,
            "{style:?}: {phases:?}"
        );
    }
}

#[test]
fn volcanoes_rumble_before_they_erupt_and_their_bombs_fall_near_them() {
    let g = ground(FrontierStyle::Volcanic, 1_000);
    let seed = g.params().seed;
    let cones = generate::volcano_cones(&g);
    assert!(cones.iter().any(|cone| cone.caldera));
    for cone in &cones {
        let mut last = rules::EruptionPhase::Quiet;
        let mut erupted = 0;
        for tenth in 0..4_000_u64 {
            let millis = tenth * 100;
            let (phase, _) = rules::eruption_at(seed, cone.index, millis);
            if phase == rules::EruptionPhase::Erupting && last != phase && tenth > 0 {
                assert_eq!(last, rules::EruptionPhase::Rumbling, "a telegraph first");
                erupted += 1;
            }
            last = phase;
        }
        assert!(erupted >= 1, "cone {} erupts within 400 s", cone.index);
        // Every bomb lands within reach of its cone, after its shadow showed.
        let mut bombs = Vec::new();
        for window in (0..400_u64).map(|second| second * 1_000) {
            bombs.extend(rules::bombs_landing(&g, cone.index, window, window + 1_000));
        }
        assert!(!bombs.is_empty());
        let width = i64::from(g.strip_width());
        for bomb in bombs {
            let mut dx = (bomb.column - cone.centre).rem_euclid(width);
            if dx > width / 2 {
                dx -= width;
            }
            assert!(dx.abs() <= cone.radius + 28);
            assert_eq!(bomb.land - bomb.launch, rules::BOMB_FLIGHT_MILLIS);
            assert!(
                rules::bombs_in_flight(&g, cone.index, bomb.land - 1).contains(&bomb),
                "its shadow shows before it lands"
            );
        }
    }
}

#[test]
fn planet_hazards_are_tagged_for_the_planet_gear() {
    use PlanetHazard::{Cold, Heat, HighGravity, ToxicAir};
    assert_eq!(hazards(FrontierStyle::Desert), vec![Heat]);
    assert_eq!(hazards(FrontierStyle::Volcanic), vec![Heat, ToxicAir]);
    assert_eq!(hazards(FrontierStyle::Crystal), vec![Cold, HighGravity]);
    // The Magma Chambers burn past the best classic cooling: the planet gear's job.
    let g = ground(FrontierStyle::Volcanic, 1_000);
    let column = 300;
    let deep = g.surface_row(column) + strata_rows(1_000) * 900 / 1_000;
    let heat = rules::heat_units(&g, TilePosition { x: column, y: deep }).expect("frontier");
    let best = crate::protection::heat_cooling(
        crate::depth_progression::DESCENT_MAX_UPGRADE_LEVEL,
        crate::protection::HEAT_SHIELD_MAX_LEVEL,
    );
    assert!(heat > best, "{heat} vs {best}");
    // Every other world's heat is the classic one.
    let moon = PlanetGround::new(PlanetParams::new(
        1,
        400,
        crate::planet::PlanetThemeId::BARREN,
    ));
    assert!(rules::heat_units(&moon, TilePosition { x: 0, y: 50 }).is_none());
    // Each style's surface exposure carries its tags.
    for (style, wanted) in [
        (FrontierStyle::Volcanic, ToxicAir),
        (FrontierStyle::Crystal, Cold),
    ] {
        let g = ground(style, 1_000);
        let exposure = rules::exposure_at(
            &g,
            TilePosition {
                x: 40,
                y: g.surface_row(40) - 1,
            },
            rules::Weather::default(),
        );
        assert!(exposure.hazards().contains(&wanted), "{style:?}");
    }
}

#[test]
fn each_world_has_neutral_fauna_and_hunters_in_its_rosters() {
    for style in FrontierStyle::ALL {
        let own = creatures(style);
        assert!(own.iter().any(|kind| kind.is_frontier_fauna()));
        assert!(own.iter().any(|kind| !kind.is_frontier_fauna()));
        let mut zones = vec![rules::Zone::Surface, rules::Zone::CoreChamber];
        zones.extend((0..4).map(rules::Zone::Stratum));
        let mut seen = std::collections::BTreeSet::new();
        for zone in zones {
            let roster = roster(style, zone);
            assert!(!roster.is_empty());
            for (kind, weight) in roster {
                assert!(own.contains(kind), "{style:?} {zone:?} {kind:?}");
                assert!(*weight > 0);
                seen.insert(*kind);
            }
        }
        assert_eq!(
            seen.len(),
            own.len(),
            "{style:?}: every creature roams somewhere"
        );
    }
}

#[test]
fn the_space_report_names_each_frontier_ground() {
    let text = crate::space::report::space_report(0x0D3E_9DE5, 2);
    for line in [
        "desert dunes       -> Dune World: Shifting Dunes, Buried City, Worm Galleries, Glass Deep",
        "volcanic           -> Volcanic World: Ash Fields, Basalt Flows, Obsidian Mantle, Magma Chambers",
        "crystal            -> Crystal World: Shard Meadows, Prism Beds, Growing Geodes, Heavy Lattice",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}
