use std::collections::BTreeSet;

use super::{
    home::{DEPARTURE_ABOVE_MOON, HOME_WORLD_BODY, MOON_BODY},
    report::{sectors_around_home, space_report},
    *,
};
use crate::{
    SimulationTick,
    hash::StateHasher,
    star_chart::{CHART_RING, chart_at_site},
    worldgen::{DEFAULT_WORLD_SEED, GeneratorVersion, WorldSettings},
};

fn bytes(seed: u64, sectors: &[SpaceSector]) -> Vec<u8> {
    let generated: Vec<Sector> = sectors
        .iter()
        .map(|coords| generate_sector(seed, *coords))
        .collect();
    postcard::to_allocvec(&generated).expect("encode")
}

fn digest(seed: u64) -> u64 {
    let mut hasher = StateHasher::new();
    for coords in sectors_around_home(2) {
        hasher.write_serialized(&generate_sector(seed, coords));
    }
    hasher.finish().get()
}

#[test]
fn space_is_a_pure_function_of_the_seed() {
    let mut sectors = sectors_around_home(1);
    sectors.extend([
        SpaceSector::new(40, -17),
        SpaceSector::new(-2_000_000, 1_999_999),
        SpaceSector::new(i32::MAX, i32::MIN),
    ]);
    let a = bytes(7, &sectors);
    assert_eq!(a, bytes(7, &sectors), "same seed, same bytes");
    assert_ne!(a, bytes(8, &sectors), "another seed, another sky");
    // The cache answers what the generator does.
    let space = Space::new(7);
    for coords in &sectors {
        assert_eq!(*space.sector(*coords), generate_sector(7, *coords));
    }
}

/// Pins the space generator. Change [`GOLDEN_SPACE_DIGEST`] only on purpose.
#[test]
fn space_generation_is_pinned() {
    let value = digest(DEFAULT_WORLD_SEED);
    assert_eq!(
        value, GOLDEN_SPACE_DIGEST,
        "space generation changed: {value:#018x}"
    );
}

#[test]
fn only_deep_descent_worlds_have_space() {
    for generator in GeneratorVersion::ALL {
        let settings = WorldSettings {
            generator,
            ..WorldSettings::new(1)
        };
        assert_eq!(
            Space::for_world(&settings).is_some(),
            generator == GeneratorVersion::V11,
            "{generator:?}"
        );
    }
}

#[allow(clippy::too_many_lines, reason = "every rule of a system")]
fn check_system(sector: &Sector, system: &StarSystem) {
    let label = format!("{:?} system {}", sector.coords, system.index);
    assert!(i64::from(system.extent) <= SYSTEM_MAX_EXTENT, "{label}");
    // Inside its sector (and, away from home, inside one cell).
    let origin = sector.coords.origin();
    let extent = i64::from(system.extent);
    for (low, value) in [(origin.x, system.centre.x), (origin.y, system.centre.y)] {
        assert!(
            value - extent >= low && value + extent <= low + SPACE_SECTOR_TILES,
            "{label}"
        );
        if !system.home {
            let cell = (value - low) / CELL_TILES;
            let cell_low = low + cell * CELL_TILES;
            assert!(
                value - extent >= cell_low && value + extent <= cell_low + CELL_TILES,
                "{label} leaves its cell"
            );
        }
    }
    // Annuli around the star never overlap: bodies with their extent, belts.
    let mut annuli: Vec<(i64, i64, String)> = Vec::new();
    for body in system.bodies.iter().filter(|body| body.parent.is_none()) {
        let (r, e) = (i64::from(body.orbit_radius), i64::from(body.extent));
        annuli.push((r - e, r + e, format!("body {}", body.index)));
        let measured = system.centre.distance(body.centre);
        assert!((measured - r as f64).abs() < 4.0, "{label}: orbit placed");
    }
    for belt in &system.belts {
        annuli.push((
            i64::from(belt.inner),
            i64::from(belt.outer),
            format!("belt {}", belt.index),
        ));
    }
    annuli.sort();
    for pair in annuli.windows(2) {
        assert!(
            pair[0].1 < pair[1].0,
            "{label}: {} and {} overlap",
            pair[0].2,
            pair[1].2
        );
    }
    let first = annuli.first().map_or(i64::MAX, |annulus| annulus.0);
    for star in &system.stars {
        let reach = system.centre.distance(star.centre) as i64 + i64::from(star.heat_radius);
        assert!(reach < first, "{label}: an orbit inside the heat");
    }
    for body in &system.bodies {
        assert!(body.radius >= MIN_LANDABLE_RADIUS, "{label}");
        match body.kind {
            BodyKind::Planet => {
                assert!((PLANET_RADIUS.0..=PLANET_RADIUS.1).contains(&body.radius));
                assert!(Theme::PLANET.contains(&body.theme));
                assert!((800..=2_500).contains(&body.strata_rows));
            }
            BodyKind::Moon | BodyKind::CraterRock => {
                assert!((SMALL_BODY_RADIUS.0..=SMALL_BODY_RADIUS.1).contains(&body.radius));
            }
            BodyKind::HomeWorld | BodyKind::HomeMoon => assert!(system.home),
        }
        assert_eq!(body.rows, body.radius);
        assert_eq!(body.surface_columns % 32, 0);
        assert!(body.surface_columns > body.radius * 6);
        assert!(body.surface_columns < body.radius * 7);
        assert_eq!(
            body.theme == Theme::TidallyLocked,
            body.kind != BodyKind::Moon && body.day_seconds == 0 && !body.is_home()
        );
    }
    // Moons around each planet: their own annuli, outside the planet and its ring, inside its
    // extent.
    for planet in system.bodies.iter().filter(|body| body.parent.is_none()) {
        let mut moons: Vec<(i64, i64)> = system
            .bodies
            .iter()
            .filter(|moon| moon.parent == Some(planet.index))
            .map(|moon| {
                let measured = planet.centre.distance(moon.centre);
                assert!((measured - f64::from(moon.orbit_radius)).abs() < 4.0);
                (
                    i64::from(moon.orbit_radius - moon.radius),
                    i64::from(moon.orbit_radius + moon.radius),
                )
            })
            .collect();
        moons.sort_unstable();
        let floor = i64::from(planet.ring.map_or(planet.radius, |ring| ring.outer));
        for (index, (low, high)) in moons.iter().enumerate() {
            assert!(*low > floor, "{label}: a moon inside its planet");
            assert!(
                *high < i64::from(planet.extent),
                "{label}: a moon past the extent"
            );
            if index > 0 {
                assert!(moons[index - 1].1 < *low, "{label}: moon orbits overlap");
            }
        }
    }
    // Stations, comets and derelicts stay clear of every body.
    for station in &system.stations {
        for body in &system.bodies {
            assert!(
                !station.centre.within(body.centre, i64::from(body.extent)),
                "{label}: a station inside a body's reach"
            );
        }
    }
    for point in system
        .comets
        .iter()
        .map(|comet| comet.centre)
        .chain(system.derelicts.iter().map(|wreck| wreck.centre))
    {
        assert!(system.centre.distance(point) <= f64::from(system.extent));
        for body in system.bodies.iter().filter(|body| body.parent.is_none()) {
            assert!(
                !point.within(body.centre, i64::from(body.extent)),
                "{label}"
            );
        }
    }
}

#[test]
fn systems_are_well_formed_and_never_overlap() {
    for seed in [DEFAULT_WORLD_SEED, 1, 2, 99] {
        for coords in sectors_around_home(4) {
            let sector = generate_sector(seed, coords);
            assert!(!sector.systems.is_empty());
            assert!(sector.systems.len() <= 3);
            assert!(
                sector.systems[0].planet_count() >= 1 || coords == HOME_SECTOR,
                "a first system has worlds"
            );
            for system in &sector.systems {
                check_system(&sector, system);
            }
            // Systems never overlap each other.
            for (index, a) in sector.systems.iter().enumerate() {
                for b in &sector.systems[index + 1..] {
                    let gap = i64::from(a.extent) + i64::from(b.extent);
                    assert!(!a.centre.within(b.centre, gap), "{coords:?}");
                }
            }
            // The interstellar things keep out of every system.
            let things = sector
                .black_holes
                .iter()
                .map(|hole| (hole.centre, i64::from(hole.lensing_radius)))
                .chain(
                    sector
                        .wormholes
                        .iter()
                        .map(|mouth| (mouth.centre, i64::from(mouth.radius))),
                )
                .chain(
                    sector
                        .derelicts
                        .iter()
                        .map(|wreck| (wreck.centre, i64::from(wreck.size))),
                );
            for (centre, reach) in things {
                assert_eq!(SpaceSector::of_tile(centre), coords);
                for system in &sector.systems {
                    assert!(
                        !centre.within(system.centre, i64::from(system.extent) + reach),
                        "{coords:?}: something inside a system"
                    );
                }
            }
            for hole in &sector.black_holes {
                assert!(coords.chebyshev(HOME_SECTOR) >= 3);
                assert!(hole.lensing_radius > hole.horizon);
            }
        }
    }
}

#[test]
fn every_theme_appears_near_home() {
    for seed in [DEFAULT_WORLD_SEED, 1, 2, 3, 4, 5, 6, 7] {
        let mut seen = BTreeSet::new();
        for coords in sectors_around_home(3) {
            for system in &generate_sector(seed, coords).systems {
                for body in &system.bodies {
                    seen.insert(body.theme);
                }
            }
        }
        for theme in Theme::PLANET {
            assert!(
                seen.contains(&theme),
                "seed {seed}: no {theme:?} within 3 sectors"
            );
        }
    }
}

#[test]
fn the_home_system_holds_home_and_the_moon() {
    let space = Space::new(DEFAULT_WORLD_SEED);
    let home = space.home_system();
    assert!(home.home);
    assert_eq!(home.class, crate::star_chart::StarClass::Yellow);
    let world = space.body(HOME_WORLD_BODY).expect("home");
    let moon = space.body(MOON_BODY).expect("moon");
    assert_eq!(world.kind, BodyKind::HomeWorld);
    assert_eq!(moon.kind, BodyKind::HomeMoon);
    assert_eq!(moon.parent, Some(0));
    // Straight up: the Moon above home, home above the sun.
    assert_eq!(moon.centre.x, world.centre.x);
    assert!(moon.centre.y < world.centre.y);
    assert!(world.centre.y < home.centre.y);
    // Leaving the sky comes out just above the Moon.
    let departure = SpaceTile::of_pos(space.home_departure());
    assert_eq!(departure.x, moon.centre.x);
    assert_eq!(
        moon.centre.y - departure.y,
        i64::from(moon.radius + DEPARTURE_ABOVE_MOON)
    );
    let near = space.bodies_near(space.home_departure(), 2_000);
    assert_eq!(near.first().map(|hit| hit.id), Some(MOON_BODY));
    // Every seed shares the home layout (only the sister planets differ).
    let other = Space::new(5).home_system();
    for (a, b) in other.bodies.iter().zip(&home.bodies).take(2) {
        assert_eq!((a.kind, a.centre, a.radius), (b.kind, b.centre, b.radius));
    }
    // Home's sun is at its sector's centre and generation goes on from there.
    assert_eq!(home.centre, HOME_SECTOR.centre());
    assert!(home.bodies.len() > 2 || home.planet_count() >= 1);
}

#[test]
fn every_star_chart_resolves_to_a_real_system() {
    for seed in [DEFAULT_WORLD_SEED, 1, 42] {
        let space = Space::new(seed);
        for index in 0..120 {
            let depth = (index * 37 % 1_001) as u16;
            let chart = chart_at_site(
                seed,
                index * 13 - 700,
                9_400 + index * 7,
                depth,
                SimulationTick::new(0),
            );
            let distance = chart.distance();
            assert!(distance >= 1 && distance <= u32::try_from(CHART_RING.1).unwrap_or(0) + 1);
            let reference = space.chart_system(&chart);
            assert_eq!(
                reference.sector,
                SpaceSector::new(chart.sector.0, chart.sector.1)
            );
            let system = space.system(reference).expect("a real system");
            assert_eq!(system.id, chart.system);
            assert_eq!(system.class, chart.class);
            assert_eq!(system.planet_count(), usize::from(chart.worlds));
            assert!(chart.worlds >= 1);
            assert_eq!(system.name, chart.name);
        }
        // A chart from before Wave 11 (an id hashed from its plinth) still leads somewhere.
        let mut legacy = chart_at_site(seed, 5, 9_900, 500, SimulationTick::new(0));
        legacy.system = crate::star_chart::system_id(seed, 5, 9_900);
        let reference = resolve_chart(seed, &legacy);
        assert!(space.system(reference).is_some());
    }
}

#[test]
fn wormholes_pair_both_ways() {
    let seed = DEFAULT_WORLD_SEED;
    let space = Space::new(seed);
    let mut found = 0;
    for coords in sectors_around_home(40) {
        let Some(mouth) = interstellar::wormhole_in(seed, coords) else {
            continue;
        };
        found += 1;
        assert!(coords.chebyshev(mouth.exit_sector) >= 24, "far apart");
        let far = space.sector(mouth.exit_sector);
        let back = far.wormholes.first().expect("the far mouth");
        assert_eq!(back.id, mouth.id);
        assert_eq!(back.exit, mouth.centre);
        assert_eq!(back.exit_sector, coords);
        assert_eq!(
            space.wormhole_at(mouth.centre.pos()).map(|m| m.id),
            Some(mouth.id)
        );
    }
    assert!(found >= 4, "only {found} wormhole mouths within 40 sectors");
    for x in -64..64 {
        for y in -8..8 {
            let region = (x * 37, y * 1_001);
            let partner = interstellar::partner(seed, region);
            assert_ne!(partner, region);
            assert_eq!(interstellar::partner(seed, partner), region);
        }
    }
}

#[test]
fn asteroid_queries_find_exactly_the_rocks_in_reach() {
    let seed = 1;
    let space = Space::new(seed);
    let mut checked = 0;
    for coords in sectors_around_home(2) {
        let sector = space.sector(coords);
        for system in &sector.systems {
            for belt in &system.belts {
                let rocks = asteroids_in_segment(seed, system, coords, belt, 3);
                assert_eq!(rocks, asteroids_in_segment(seed, system, coords, belt, 3));
                let Some(rock) = rocks.first() else {
                    continue;
                };
                for radius in [500_i64, 6_000, 30_000] {
                    let at = rock.centre.offset(radius / 3, -radius / 4);
                    let fast: BTreeSet<AsteroidId> = space
                        .asteroids_near(at, radius)
                        .iter()
                        .filter(|found| {
                            found.id.system == system.index
                                && found.id.belt == belt.index
                                && found.id.sector == coords
                        })
                        .map(|found| found.id)
                        .collect();
                    let slow: BTreeSet<AsteroidId> = (0..belt.segments)
                        .flat_map(|segment| {
                            asteroids_in_segment(seed, system, coords, belt, segment)
                        })
                        .filter(|found| found.centre.within(at, radius + i64::from(found.radius)))
                        .map(|found| found.id)
                        .collect();
                    assert_eq!(fast, slow);
                    assert!(fast.contains(&rock.id) || radius < i64::from(rock.radius));
                }
                for rock in &rocks {
                    assert!(rock.radius <= ASTEROID_MAX_RADIUS);
                    let r = system.centre.distance(rock.centre);
                    assert!(r >= f64::from(belt.inner) - 50.0 && r <= f64::from(belt.outer) + 50.0);
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no belts near home");
}

#[test]
fn gravity_pulls_toward_bodies_at_their_surface_strength() {
    let space = Space::new(DEFAULT_WORLD_SEED);
    let home = space.body(HOME_WORLD_BODY).expect("home");
    // Standing on the home world's far side from the Moon, the home world's 1 g dominates.
    let on_surface = home.centre.offset(0, i64::from(home.radius)).pos();
    let (gx, gy) = space.gravity_g(on_surface);
    // (The sun, 250,000 tiles below, takes a little back.)
    assert!(gy < -0.85 && gy > -1.1, "pull {gx} {gy}");
    assert!(gx.abs() < 0.05);
    for source in space.gravity_sources_near(on_surface) {
        assert!(source.influence() > 0.0);
    }
    // The pull weakens with height.
    let high = space.gravity_g(home.centre.offset(0, i64::from(home.radius) * 3).pos());
    assert!(high.1.abs() < gy.abs());
    // Inside a body the pull fades to zero at the centre.
    let source = space
        .gravity_sources_near(home.centre)
        .into_iter()
        .find(|source| source.kind == query::GravityKind::Body(HOME_WORLD_BODY))
        .expect("home pulls");
    let centre = source.pull_g(home.centre.offset(0, i64::from(home.radius / 2)).pos());
    assert!((centre.1 + 0.5).abs() < 0.01, "{centre:?}");
}

#[test]
fn queries_find_systems_stations_and_scan_contacts() {
    let space = Space::new(DEFAULT_WORLD_SEED);
    let departure = space.home_departure();
    let systems = space.systems_near(departure, 10_000);
    assert_eq!(
        systems.first().map(|(reference, _)| reference.sector),
        Some(HOME_SECTOR)
    );
    let station = space
        .nearest_station(departure, 6)
        .expect("a station within six sectors");
    assert!(station.distance > 0.0);
    // The nearest is the nearest: nothing in the search area beats it.
    for coords in sectors_around_home(1) {
        for other in space
            .sector(coords)
            .systems
            .iter()
            .flat_map(|system| &system.stations)
        {
            assert!(other.centre.distance(SpaceTile::of_pos(departure)) >= station.distance);
        }
    }
    let contacts = space.scan(departure, 400_000);
    assert!(
        contacts
            .iter()
            .any(|contact| matches!(contact.kind, query::ContactKind::Body(id) if id == MOON_BODY))
    );
    assert!(
        contacts
            .iter()
            .any(|contact| matches!(contact.kind, query::ContactKind::Star { .. }))
    );
    assert!(
        contacts
            .windows(2)
            .all(|pair| pair[0].distance <= pair[1].distance)
    );
    assert_eq!(space.fog_at(departure), 0, "home is clear of nebulae");
    assert!((space.time_scale_at(departure) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn nebulae_fog_the_scanner_and_black_holes_slow_time() {
    let seed = DEFAULT_WORLD_SEED;
    let space = Space::new(seed);
    let mut fogged = None;
    let mut hole = None;
    for coords in sectors_around_home(12) {
        let sector = space.sector(coords);
        if fogged.is_none() {
            fogged = sector
                .nebulae
                .iter()
                .find(|nebula| nebula.fog >= 400)
                .copied();
        }
        if hole.is_none() {
            hole = sector.black_holes.first().copied();
        }
    }
    let nebula = fogged.expect("a thick nebula within 12 sectors");
    assert!(space.fog_at(nebula.centre) >= nebula.fog);
    assert!(space.nebula_at(nebula.centre).is_some());
    assert_eq!(nebula.fog_at(nebula.centre.offset(nebula.radius, 0)), 0);
    let hole = hole.expect("a black hole within 12 sectors");
    let at_horizon = space.time_scale_at(hole.centre.offset(i64::from(hole.horizon), 0));
    assert!((at_horizon - f32::from(hole.time_dilation) / 1_000.0).abs() < 0.01);
    let outside = space.time_scale_at(hole.centre.offset(i64::from(hole.lensing_radius) + 10, 0));
    assert!((outside - 1.0).abs() < f32::EPSILON);
    let halfway = space.time_scale_at(hole.centre.offset(
        i64::from(u32::midpoint(hole.horizon, hole.lensing_radius)),
        0,
    ));
    assert!(halfway > at_horizon && halfway < 1.0);
}

#[test]
fn the_cache_stays_bounded() {
    let space = Space::new(3);
    for x in 0..(query::CACHE_SECTORS as i32 * 2) {
        let _ = space.sector(SpaceSector::new(x, 0));
    }
    assert_eq!(space.cached_sectors(), query::CACHE_SECTORS);
    let fresh = space.clone();
    assert_eq!(fresh.cached_sectors(), 0);
    assert_eq!(fresh.seed(), space.seed());
}

#[test]
fn the_report_summarises_the_neighbourhood() {
    let text = space_report(DEFAULT_WORLD_SEED, 2);
    assert!(text.contains("systems:"));
    assert!(text.contains("radius histogram"));
    assert!(text.contains("wormhole pairs"));
    assert!(text.contains("departure above the Moon"));
    assert_eq!(text, space_report(DEFAULT_WORLD_SEED, 2));
}

/// A sector generates in well under a few milliseconds (release builds only: debug is ~20x
/// slower and the suite runs on a loaded machine).
#[cfg(not(debug_assertions))]
#[test]
fn sectors_generate_quickly() {
    let start = std::time::Instant::now();
    let sectors = sectors_around_home(6);
    for coords in &sectors {
        let _ = generate_sector(DEFAULT_WORLD_SEED, *coords);
    }
    let average = start.elapsed().as_secs_f64() * 1_000.0 / sectors.len() as f64;
    assert!(average < 3.0, "{average:.3} ms a sector");
}

/// Fog really hides things: from inside a thick nebula the scanner misses bodies it would see
/// from clear space, and every contact it keeps lies within the fog-shortened range.
#[test]
fn fog_shortens_what_the_scanner_sees() {
    let seed = DEFAULT_WORLD_SEED;
    let space = Space::new(seed);
    let range = 400_000_i64;
    let mut checked = 0;
    let mut nebulae: Vec<Nebula> = Vec::new();
    for coords in sectors_around_home(12) {
        for system in &space.sector(coords).systems {
            nebulae.extend(system.nebula.filter(|nebula| nebula.fog >= 400));
        }
    }
    for nebula in nebulae {
        let at = nebula.centre;
        let fog = i64::from(space.fog_at(at));
        assert!(fog >= i64::from(nebula.fog));
        let reach = range * (1_000 - fog * 7 / 10) / 1_000;
        let contacts = space.scan(at, range);
        for contact in &contacts {
            let boost = match contact.kind {
                query::ContactKind::Star { .. } => 4,
                query::ContactKind::BlackHole(_) => 2,
                _ => 1,
            };
            assert!(
                contact.distance <= (reach * boost) as f64,
                "{contact:?} beyond the fogged range {reach}"
            );
            assert!(contact.signal <= 1_000);
        }
        // Bodies between the fogged and the clear range are hidden.
        let all = space
            .bodies_near(at, range)
            .into_iter()
            .filter(|hit| hit.distance < range as f64)
            .count();
        let hidden = space
            .bodies_near(at, range)
            .into_iter()
            .filter(|hit| hit.distance > reach as f64 && hit.distance < range as f64)
            .count();
        if hidden > 0 {
            let body_contacts = contacts
                .iter()
                .filter(|contact| matches!(contact.kind, query::ContactKind::Body(_)))
                .count();
            assert!(body_contacts + hidden <= all, "fog hid nothing at {at:?}");
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "no nebula star with bodies past the fogged range"
    );
}
