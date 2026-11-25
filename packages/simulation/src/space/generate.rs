//! Sector generation: a pure function of `(world seed, sector)`.

use super::{
    AsteroidBelt, AsteroidOre, Atmosphere, BodyKind, CELL_TILES, Comet, Derelict, DerelictKind,
    HOME_SECTOR, Nebula, PLANET_RADIUS, Ring, SMALL_BODY_RADIUS, SYSTEM_MAX_EXTENT, Sector,
    SpaceBody, SpaceSector, SpaceTile, Star, StarSystem, Station, StationId, StationKind,
    SystemRef, TURN, Theme, home, interstellar, trig::polar,
};
use crate::{
    star_chart::{StarChart, StarClass, star_name},
    worldgen::noise::{RegionRng, hash2},
};

const SALT_LAYOUT: u64 = 0x5BAC_E000;
const SALT_SYSTEM: u64 = 0x5BAC_E100;
const SALT_SYSTEM_ID: u64 = 0x5BAC_E200;
const SALT_GROUND: u64 = 0x5BAC_E300;
const SALT_FREE: u64 = 0x5BAC_E400;
const SALT_DERELICT: u64 = 0x5BAC_E500;

/// Arc length of one belt segment at the belt's middle, tiles.
pub(super) const BELT_SEGMENT_TILES: i64 = 8_192;

/// Which cells of a sector hold systems: the first `systems` of `order`; the rest are free.
#[derive(Clone, Copy, Debug)]
pub(super) struct CellLayout {
    pub systems: u8,
    pub order: [u8; 4],
    pub home: bool,
}

impl CellLayout {
    /// The cells holding no system (none in the home sector).
    pub fn free_cells(&self) -> &[u8] {
        if self.home {
            &[]
        } else {
            &self.order[usize::from(self.systems)..]
        }
    }
}

/// The cell layout of `sector` (cheap: no system is generated).
pub(super) fn cell_layout(seed: u64, sector: SpaceSector) -> CellLayout {
    if sector == HOME_SECTOR {
        return CellLayout {
            systems: 1,
            order: [0, 1, 2, 3],
            home: true,
        };
    }
    let mut rng = RegionRng::new(seed, SALT_LAYOUT, sector.x, sector.y);
    let roll = rng.next_u64() % 100;
    let systems = if roll < 45 {
        1
    } else if roll < 85 {
        2
    } else {
        3
    };
    let mut order = [0_u8, 1, 2, 3];
    // Fisher-Yates.
    for index in (1..4).rev() {
        let pick = (rng.next_u64() % (index as u64 + 1)) as usize;
        order.swap(index, pick);
    }
    CellLayout {
        systems,
        order,
        home: false,
    }
}

/// System `index` of `sector`'s stable id (never zero; what a star chart names).
#[must_use]
pub const fn system_uid(seed: u64, sector: SpaceSector, index: u8) -> u64 {
    hash2(seed, SALT_SYSTEM_ID + index as u64, sector.x, sector.y) | 1
}

/// Generates sector `sector` of the space of a world seeded `seed`.
#[must_use]
pub fn generate_sector(seed: u64, sector: SpaceSector) -> Sector {
    let layout = cell_layout(seed, sector);
    let mut out = Sector {
        coords: sector,
        ..Sector::default()
    };
    if layout.home {
        out.systems.push(home::home_system(seed));
    } else {
        for index in 0..layout.systems {
            let cell = layout.order[usize::from(index)];
            out.systems.push(generate_system(
                seed,
                sector,
                index,
                &SystemPlan {
                    anchor: sector.cell_centre(cell),
                    jitter: true,
                    needs_planets: index == 0,
                    ..SystemPlan::default()
                },
            ));
        }
    }
    let mut free: Vec<u8> = layout.free_cells().to_vec();
    if let Some(mouth) = interstellar::wormhole_in(seed, sector) {
        // The mouth always takes the first free cell (`interstellar::mouth_centre`).
        if !free.is_empty() {
            free.remove(0);
        }
        out.wormholes.push(mouth);
    }
    if let Some((cell, hole)) = interstellar::black_hole_in(seed, sector, &free) {
        free.retain(|other| *other != cell);
        out.black_holes.push(hole);
    }
    for cell in free {
        drifting_derelicts(seed, sector, cell, &mut out.derelicts);
    }
    out.nebulae = interstellar::nebulae_overlapping(seed, sector);
    out
}

fn drifting_derelicts(seed: u64, sector: SpaceSector, cell: u8, out: &mut Vec<Derelict>) {
    let mut rng = RegionRng::new(seed, SALT_FREE + u64::from(cell), sector.x, sector.y);
    if !rng.chance(250) {
        return;
    }
    let centre = sector.cell_centre(cell);
    for _ in 0..rng.range(1, 2) {
        let reach = (CELL_TILES / 2 - 120_000) as i32;
        let at = centre.offset(
            i64::from(rng.range(-reach, reach)),
            i64::from(rng.range(-reach, reach)),
        );
        out.push(derelict(&mut rng, at));
    }
}

fn derelict(rng: &mut RegionRng, centre: SpaceTile) -> Derelict {
    let kind = [
        DerelictKind::Freighter,
        DerelictKind::Probe,
        DerelictKind::BuilderHulk,
        DerelictKind::Wreckfield,
    ][rng.pick(&[4, 3, 1, 2]).unwrap_or(0)];
    let size = match kind {
        DerelictKind::Freighter => rng.range(40, 90),
        DerelictKind::Probe => rng.range(6, 14),
        DerelictKind::BuilderHulk => rng.range(120, 260),
        DerelictKind::Wreckfield => rng.range(200, 600),
    } as u16;
    Derelict {
        kind,
        centre,
        size,
        seed: rng.next_u64() ^ SALT_DERELICT,
    }
}

// ---------------------------------------------------------------------------------------------
// Systems.
// ---------------------------------------------------------------------------------------------

/// How to generate one system.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SystemPlan {
    /// The cell centre (or the home system's fixed centre).
    pub anchor: SpaceTile,
    /// Whether the centre moves off the anchor (inside its cell).
    pub jitter: bool,
    /// Whether it must have a planet (a sector's first system: star charts point at it).
    pub needs_planets: bool,
    /// A forced star class and radius (home).
    pub star: Option<(StarClass, u32)>,
    /// The innermost orbits are taken (home: the home world and the Moon): `(bodies, outer
    /// edge)` are pushed before the generated slots.
    pub reserved: Option<u32>,
    pub home: bool,
}

/// Physical constants per star class.
struct StarProfile {
    radius: (i32, i32),
    /// Heat radius in tenths of the star's radius.
    heat_tenths: u32,
    temperature_k: u32,
    light: u16,
    flare_period_s: (i32, i32),
    gravity_milli_g: (i32, i32),
}

const fn profile(class: StarClass) -> StarProfile {
    match class {
        StarClass::Yellow => StarProfile {
            radius: (20_000, 30_000),
            heat_tenths: 25,
            temperature_k: 5_800,
            light: 1_000,
            flare_period_s: (1_800, 5_400),
            gravity_milli_g: (7_000, 10_000),
        },
        StarClass::Red => StarProfile {
            radius: (11_000, 18_000),
            heat_tenths: 18,
            temperature_k: 3_200,
            light: 450,
            flare_period_s: (300, 1_200),
            gravity_milli_g: (5_000, 8_000),
        },
        StarClass::Blue => StarProfile {
            radius: (40_000, 60_000),
            heat_tenths: 40,
            temperature_k: 15_000,
            light: 2_200,
            flare_period_s: (900, 2_400),
            gravity_milli_g: (12_000, 16_000),
        },
        StarClass::White => StarProfile {
            radius: (4_000, 6_000),
            heat_tenths: 30,
            temperature_k: 9_000,
            light: 700,
            flare_period_s: (0, 0),
            gravity_milli_g: (20_000, 30_000),
        },
        StarClass::Binary => StarProfile {
            radius: (14_000, 22_000),
            heat_tenths: 25,
            temperature_k: 5_000,
            light: 1_300,
            flare_period_s: (1_200, 3_600),
            gravity_milli_g: (6_000, 9_000),
        },
        StarClass::Nebula => StarProfile {
            radius: (18_000, 28_000),
            heat_tenths: 25,
            temperature_k: 6_500,
            light: 900,
            flare_period_s: (2_400, 7_200),
            gravity_milli_g: (7_000, 10_000),
        },
    }
}

/// Class weights, in `StarClass::ALL` order (yellow, red, blue, white, twin, nebula).
const CLASS_WEIGHTS: [u32; 6] = [26, 32, 8, 10, 13, 8];
/// Planet count weights, 0-8.
const PLANET_COUNT_WEIGHTS: [u32; 9] = [5, 9, 13, 16, 16, 14, 11, 9, 7];

#[allow(clippy::too_many_lines, reason = "one pass outward through a system")]
pub(super) fn generate_system(
    seed: u64,
    sector: SpaceSector,
    index: u8,
    plan: &SystemPlan,
) -> StarSystem {
    let mut rng = RegionRng::new(seed, SALT_SYSTEM + u64::from(index), sector.x, sector.y);
    let id = system_uid(seed, sector, index);
    let (class, forced_radius) = plan.star.map_or_else(
        || {
            let class = StarClass::ALL[rng.pick(&CLASS_WEIGHTS).unwrap_or(0)];
            (class, None)
        },
        |(class, radius)| (class, Some(radius)),
    );
    let star_profile = profile(class);
    let radius = forced_radius
        .unwrap_or_else(|| rng.range(star_profile.radius.0, star_profile.radius.1) as u32);
    let star = |rng: &mut RegionRng, centre: SpaceTile, radius: u32| {
        let period = rng.range(star_profile.flare_period_s.0, star_profile.flare_period_s.1);
        Star {
            centre,
            radius,
            heat_radius: radius * star_profile.heat_tenths / 10,
            temperature_k: star_profile.temperature_k,
            light: star_profile.light,
            flare_period_s: period as u32,
            flare_phase_s: if period > 0 {
                rng.range(0, period - 1) as u32
            } else {
                0
            },
            gravity_milli_g: rng.range(
                star_profile.gravity_milli_g.0,
                star_profile.gravity_milli_g.1,
            ) as u32,
        }
    };
    let mut stars = Vec::new();
    let heat_reach = if class == StarClass::Binary {
        let separation = i64::from(radius) * 2 + i64::from(rng.range(24_000, 60_000));
        let angle = rng.range(0, TURN as i32 - 1) as u32;
        let (dx, dy) = polar(separation / 2, angle);
        let second = radius * rng.range(70, 95) as u32 / 100;
        stars.push(star(&mut rng, SpaceTile::new(dx, dy), radius));
        stars.push(star(&mut rng, SpaceTile::new(-dx, -dy), second));
        separation / 2 + i64::from(stars[0].heat_radius)
    } else {
        stars.push(star(&mut rng, SpaceTile::new(0, 0), radius));
        i64::from(stars[0].heat_radius)
    };
    let star_temperature = star_profile.temperature_k as f64
        * if class == StarClass::Binary {
            1.19
        } else {
            1.0
        };

    let mut system = StarSystem {
        index,
        id,
        name: if plan.home {
            "Home".to_string()
        } else {
            star_name(id)
        },
        class,
        centre: SpaceTile::default(),
        stars,
        home: plan.home,
        ..StarSystem::default()
    };

    // Orbits outward, never overlapping: each slot is an annulus `[cursor, cursor + width]`.
    let mut cursor = heat_reach + i64::from(rng.range(30_000, 70_000));
    if let Some(edge) = plan.reserved {
        home::push_home_bodies(seed, &mut system);
        cursor = cursor.max(i64::from(edge) + i64::from(rng.range(40_000, 80_000)));
    }
    let mut target = rng.pick(&PLANET_COUNT_WEIGHTS).unwrap_or(3);
    if plan.needs_planets && target == 0 {
        target = 1;
    }
    let limit = SYSTEM_MAX_EXTENT - 60_000;
    let mut planets = 0;
    let mut slots = 0;
    let mut outer_edge = cursor;
    while planets < target && slots < 16 {
        slots += 1;
        // Planets first so a first system always gets one.
        let kind = if planets == 0 && plan.needs_planets && !plan.home {
            0
        } else {
            rng.pick(&[70, 14, 16]).unwrap_or(0)
        };
        let temperature = orbit_temperature(star_temperature, radius, cursor);
        let placed = match kind {
            1 => crater_rock(seed, &mut rng, &mut system, cursor, temperature, limit),
            2 if system.belts.len() < 2 => belt(&mut rng, &mut system, cursor, temperature, limit),
            _ => {
                let width = planet(seed, &mut rng, &mut system, cursor, temperature, limit);
                if width.is_some() {
                    planets += 1;
                }
                width
            }
        };
        let Some(width) = placed else {
            break;
        };
        outer_edge = cursor + width;
        cursor = outer_edge + i64::from(rng.range(25_000, 70_000)) + cursor / 12;
    }

    // The outer band: comets and derelicts beyond the last orbit.
    let band = outer_edge + 20_000;
    for chance in [450, 250] {
        if rng.chance(chance) && band + 40_000 < limit {
            let r = i64::from(rng.range(band as i32, (limit - 30_000) as i32));
            let angle = rng.range(0, TURN as i32 - 1) as u32;
            let length = i64::from(rng.range(20_000, 90_000)).min(limit - r);
            let (x, y) = polar(r, angle);
            let (tx, ty) = polar(r + length, angle);
            system.comets.push(Comet {
                centre: SpaceTile::new(x, y),
                nucleus_radius: rng.range(20, 80) as u16,
                tail_end: SpaceTile::new(tx, ty),
                tail_width: (length / 6) as u32,
            });
        }
    }
    for chance in [220, 120] {
        if rng.chance(chance) && band + 20_000 < limit {
            let r = i64::from(rng.range(band as i32, (limit - 10_000) as i32));
            let (x, y) = polar(r, rng.range(0, TURN as i32 - 1) as u32);
            let wreck = derelict(&mut rng, SpaceTile::new(x, y));
            system.derelicts.push(wreck);
        }
    }

    // Stations keep station with a planet, 60° ahead of or behind it on its orbit.
    let planet_indices: Vec<u8> = system
        .bodies
        .iter()
        .filter(|body| body.kind == BodyKind::Planet)
        .map(|body| body.index)
        .collect();
    for chance in [380, 120] {
        if planet_indices.is_empty() || !rng.chance(chance) {
            continue;
        }
        let planet = planet_indices[rng.range(0, planet_indices.len() as i32 - 1) as usize];
        let body = &system.bodies[usize::from(planet)];
        let lead = if rng.chance(500) {
            TURN / 6
        } else {
            TURN - TURN / 6
        };
        let angle = (body.orbit_angle + lead) % TURN;
        let (x, y) = polar(i64::from(body.orbit_radius), angle);
        let kind = [
            StationKind::TradePost,
            StationKind::Shipyard,
            StationKind::Outpost,
            StationKind::Relay,
        ][rng.pick(&[5, 2, 3, 2]).unwrap_or(0)];
        let station = Station {
            id: StationId {
                sector,
                system: index,
                index: system.stations.len() as u8,
            },
            kind,
            centre: SpaceTile::new(x, y),
            size: rng.range(60, 180) as u16,
            planet,
        };
        // Two stations never share a point.
        if system
            .stations
            .iter()
            .all(|other| !other.centre.within(station.centre, 2_000))
        {
            system.stations.push(station);
        }
    }

    // How far it all reaches.
    let mut extent = outer_edge;
    for comet in &system.comets {
        extent = extent.max(radius_of(comet.tail_end) + i64::from(comet.tail_width));
    }
    for wreck in &system.derelicts {
        extent = extent.max(radius_of(wreck.centre) + i64::from(wreck.size));
    }
    let extent = (extent + 16_000).min(SYSTEM_MAX_EXTENT);
    system.extent = extent as u32;

    if class == StarClass::Nebula {
        const COLOURS: [[u8; 3]; 4] = [
            [200, 120, 255],
            [255, 120, 190],
            [120, 200, 255],
            [140, 255, 200],
        ];
        system.nebula = Some(Nebula {
            id,
            centre: SpaceTile::default(),
            radius: extent + 150_000,
            colour: COLOURS[rng.range(0, 3) as usize],
            fog: rng.range(350, 600) as u16,
        });
    }

    // Place it in its cell.
    let mut centre = plan.anchor;
    if plan.jitter {
        let room = (CELL_TILES / 2 - extent - 32_000).max(0) as i32;
        centre = centre.offset(
            i64::from(rng.range(-room, room)),
            i64::from(rng.range(-room, room)),
        );
    }
    translate(&mut system, centre);
    system
}

/// Distance of `tile` from the (still relative) system centre.
fn radius_of(tile: SpaceTile) -> i64 {
    // Exact integers: a float distance's last bit could round a placement differently on
    // another platform, and the extent decides where the whole system sits.
    SpaceTile::default().distance_ceil(tile)
}

fn translate(system: &mut StarSystem, by: SpaceTile) {
    let shift = |tile: &mut SpaceTile| {
        tile.x += by.x;
        tile.y += by.y;
    };
    shift(&mut system.centre);
    for star in &mut system.stars {
        shift(&mut star.centre);
    }
    for body in &mut system.bodies {
        shift(&mut body.centre);
    }
    for comet in &mut system.comets {
        shift(&mut comet.centre);
        shift(&mut comet.tail_end);
    }
    for station in &mut system.stations {
        shift(&mut station.centre);
    }
    for wreck in &mut system.derelicts {
        shift(&mut wreck.centre);
    }
    if let Some(nebula) = &mut system.nebula {
        shift(&mut nebula.centre);
    }
}

/// Equilibrium temperature (kelvin) at `distance` tiles from a star of `star_k` kelvin and
/// `radius` tiles (scaled so the home world, 260,000 tiles from its 24,000-tile sun, is
/// temperate).
fn orbit_temperature(star_k: f64, radius: u32, distance: i64) -> i32 {
    let ratio = f64::from(radius) / (2.0 * distance.max(1) as f64);
    (star_k * ratio.sqrt() * 0.231) as i32
}

/// Theme weights by orbit temperature, in `Theme::PLANET` order (jungle, ocean, ice, desert,
/// volcanic, crystal, fungal, machine, gas giant, locked, ring, dead).
const fn theme_weights(kelvin: i32) -> [u32; 12] {
    if kelvin > 360 {
        [0, 0, 0, 18, 30, 8, 0, 6, 0, 16, 0, 10]
    } else if kelvin > 290 {
        [16, 10, 0, 16, 6, 6, 10, 6, 0, 10, 6, 6]
    } else if kelvin > 200 {
        [18, 22, 4, 4, 0, 6, 16, 8, 4, 4, 12, 6]
    } else if kelvin > 130 {
        [4, 4, 16, 0, 0, 10, 8, 10, 14, 4, 10, 10]
    } else {
        [0, 0, 24, 0, 0, 8, 2, 6, 24, 0, 6, 8]
    }
}

/// Small-body theme weights by temperature: barren, ice, volcanic, crystal, dead.
const fn small_theme_weights(kelvin: i32) -> [u32; 5] {
    if kelvin > 330 {
        [5, 0, 3, 1, 1]
    } else if kelvin > 220 {
        [6, 1, 1, 2, 2]
    } else {
        [5, 4, 0, 2, 1]
    }
}

const SMALL_THEMES: [Theme; 5] = [
    Theme::Barren,
    Theme::Ice,
    Theme::Volcanic,
    Theme::Crystal,
    Theme::DeadWorld,
];

/// Planet radius range per theme.
const fn theme_radius(theme: Theme) -> (i32, i32) {
    match theme {
        Theme::GasGiant => (7_500, 10_000),
        Theme::RingWorld => (2_500, 6_500),
        Theme::Jungle => (2_500, 8_000),
        Theme::OceanArchipelago => (3_000, 9_000),
        Theme::Ice | Theme::TidallyLocked => (1_500, 6_000),
        Theme::DesertDunes | Theme::MachineWorld => (2_000, 7_000),
        Theme::Volcanic | Theme::DeadWorld => (1_500, 6_500),
        Theme::Crystal => (1_500, 4_500),
        Theme::Fungal => (2_000, 6_000),
        Theme::Barren | Theme::Home | Theme::Moon => (1_500, 10_000),
    }
}

/// Surface temperature range per theme, °C.
const fn theme_temperature(theme: Theme) -> (i32, i32) {
    match theme {
        Theme::Jungle => (18, 45),
        Theme::OceanArchipelago => (5, 35),
        Theme::Ice => (-200, -15),
        Theme::DesertDunes => (30, 80),
        Theme::Volcanic => (120, 600),
        Theme::Crystal => (-150, 60),
        Theme::Fungal => (5, 40),
        Theme::MachineWorld => (-100, 90),
        Theme::GasGiant => (-220, -20),
        Theme::TidallyLocked => (-60, 120),
        Theme::RingWorld => (-10, 35),
        Theme::DeadWorld | Theme::Barren => (-180, 90),
        Theme::Home => (15, 15),
        Theme::Moon => (-120, -120),
    }
}

fn atmosphere(rng: &mut RegionRng, theme: Theme) -> Atmosphere {
    use Atmosphere::{Breathable, Scalding, Spores, Thick, Thin, Toxic, Vacuum};
    let options: &[(Atmosphere, u32)] = match theme {
        Theme::Jungle => &[(Breathable, 3), (Thick, 2)],
        Theme::OceanArchipelago | Theme::Home => &[(Breathable, 1)],
        Theme::Ice => &[(Thin, 3), (Vacuum, 1)],
        Theme::DesertDunes | Theme::TidallyLocked => &[(Thin, 2), (Breathable, 1)],
        Theme::Volcanic => &[(Toxic, 2), (Scalding, 2)],
        Theme::Crystal => &[(Vacuum, 2), (Thin, 1)],
        Theme::Fungal => &[(Spores, 1)],
        Theme::MachineWorld => &[(Thin, 2), (Toxic, 1), (Vacuum, 1)],
        Theme::GasGiant => &[(Thick, 1)],
        Theme::RingWorld => &[(Breathable, 2), (Thin, 1)],
        Theme::DeadWorld => &[(Vacuum, 3), (Thin, 1), (Toxic, 1)],
        Theme::Barren | Theme::Moon => &[(Vacuum, 1)],
    };
    let weights: Vec<u32> = options.iter().map(|(_, weight)| *weight).collect();
    options[rng.pick(&weights).unwrap_or(0)].0
}

/// `2πR` rounded to a multiple of 32 (one terrain chunk), at least 32.
#[must_use]
pub const fn surface_columns(radius: u32) -> u32 {
    // 710/113 is 2π to 8e-8.
    let columns = (radius as u64 * 710 + 56) / 113;
    let chunks = (columns + 16) / 32;
    (if chunks == 0 { 1 } else { chunks } * 32) as u32
}

#[allow(clippy::too_many_arguments)]
fn body(
    seed: u64,
    rng: &mut RegionRng,
    system: &StarSystem,
    kind: BodyKind,
    theme: Theme,
    radius: u32,
    kelvin: i32,
) -> SpaceBody {
    let index = system.bodies.len() as u8;
    let (low, high) = theme_temperature(theme);
    let adjust = match theme {
        Theme::Volcanic => 180,
        Theme::DesertDunes => 15,
        Theme::Jungle => 8,
        _ => 0,
    };
    let temperature = (kelvin - 273 + adjust + rng.range(-12, 12)).clamp(low, high);
    let gravity = if theme == Theme::GasGiant {
        rng.range(1_800, 2_600)
    } else {
        let base = 350 + radius as i32 * 12 / 100 + rng.range(-120, 120);
        let scaled = match theme {
            Theme::Crystal => base * 8 / 10,
            Theme::MachineWorld => base * 11 / 10,
            _ => base,
        };
        scaled.clamp(120, 2_600)
    };
    let day = match theme {
        Theme::TidallyLocked => 0,
        Theme::GasGiant => rng.range(300, 900),
        _ if kind == BodyKind::Moon && rng.chance(400) => 0,
        _ => rng.range(480, 2_400),
    };
    let strata = if kind == BodyKind::Planet {
        if theme == Theme::GasGiant {
            800
        } else {
            rng.range(800, 2_500)
        }
    } else {
        rng.range(160, 320)
    };
    SpaceBody {
        index,
        kind,
        parent: None,
        theme,
        centre: SpaceTile::default(),
        radius,
        orbit_radius: 0,
        orbit_angle: 0,
        extent: radius,
        gravity_milli_g: gravity as u16,
        day_seconds: day as u32,
        atmosphere: atmosphere(rng, theme),
        temperature_c: temperature as i16,
        surface_columns: surface_columns(radius),
        rows: radius,
        strata_rows: strata as u16,
        ring: None,
        ground_seed: hash2(seed ^ system.id, SALT_GROUND, i32::from(index), 0),
    }
}

/// Places a planet (and its moons) in the annulus starting at `cursor`; returns its width.
fn planet(
    seed: u64,
    rng: &mut RegionRng,
    system: &mut StarSystem,
    cursor: i64,
    kelvin: i32,
    limit: i64,
) -> Option<i64> {
    let theme = Theme::PLANET[rng.pick(&theme_weights(kelvin)).unwrap_or(0)];
    let (low, high) = theme_radius(theme);
    let radius = rng.range(low, high) as u32;
    debug_assert!(radius >= PLANET_RADIUS.0 && radius <= PLANET_RADIUS.1);
    let mut planet = body(seed, rng, system, BodyKind::Planet, theme, radius, kelvin);
    let r = i64::from(radius);
    let planet_ring = match theme {
        Theme::RingWorld => Some(Ring {
            inner: radius * 13 / 10,
            outer: radius * rng.range(18, 24) as u32 / 10,
        }),
        Theme::GasGiant if rng.chance(300) => Some(Ring {
            inner: radius * 12 / 10,
            outer: radius * rng.range(15, 20) as u32 / 10,
        }),
        _ => None,
    };
    planet.ring = planet_ring;
    let moon_count = match theme {
        Theme::GasGiant => rng.range(1, 4),
        Theme::RingWorld => rng.range(0, 1),
        _ => rng.pick(&[40, 35, 18, 7]).unwrap_or(0) as i32,
    };
    // Moons outside the ring, in their own annuli.
    let mut moon_cursor = r.max(planet_ring.map_or(0, |ring| i64::from(ring.outer))) + 5_000;
    let mut moons = Vec::new();
    for _ in 0..moon_count {
        let moon_radius = rng.range(SMALL_BODY_RADIUS.0 as i32, SMALL_BODY_RADIUS.1 as i32);
        let orbit = moon_cursor + i64::from(moon_radius) + i64::from(rng.range(0, 4_000));
        moons.push((moon_radius as u32, orbit));
        moon_cursor = orbit + i64::from(moon_radius) + i64::from(rng.range(3_000, 7_000));
    }
    let extent = moons
        .last()
        .map_or(r + 6_000, |(moon_radius, orbit)| {
            orbit + i64::from(*moon_radius) + 4_000
        })
        .max(planet_ring.map_or(0, |ring| i64::from(ring.outer) + 4_000))
        .max(r + 6_000);
    let orbit = cursor + extent;
    if orbit + extent > limit {
        return None;
    }
    let angle = rng.range(0, TURN as i32 - 1) as u32;
    let (x, y) = polar(orbit, angle);
    planet.centre = SpaceTile::new(x, y);
    planet.orbit_radius = orbit as u32;
    planet.orbit_angle = angle;
    planet.extent = extent as u32;
    let parent = planet.index;
    let centre = planet.centre;
    system.bodies.push(planet);
    for (moon_radius, moon_orbit) in moons {
        let theme = SMALL_THEMES[rng.pick(&small_theme_weights(kelvin)).unwrap_or(0)];
        let mut moon = body(
            seed,
            rng,
            system,
            BodyKind::Moon,
            theme,
            moon_radius,
            kelvin,
        );
        let angle = rng.range(0, TURN as i32 - 1) as u32;
        let (dx, dy) = polar(moon_orbit, angle);
        moon.parent = Some(parent);
        moon.centre = centre.offset(dx, dy);
        moon.orbit_radius = moon_orbit as u32;
        moon.orbit_angle = angle;
        moon.extent = moon_radius + 1_000;
        system.bodies.push(moon);
    }
    Some(extent * 2)
}

fn crater_rock(
    seed: u64,
    rng: &mut RegionRng,
    system: &mut StarSystem,
    cursor: i64,
    kelvin: i32,
    limit: i64,
) -> Option<i64> {
    let theme = SMALL_THEMES[rng.pick(&small_theme_weights(kelvin)).unwrap_or(0)];
    let radius = rng.range(SMALL_BODY_RADIUS.0 as i32, SMALL_BODY_RADIUS.1 as i32) as u32;
    let mut rock = body(
        seed,
        rng,
        system,
        BodyKind::CraterRock,
        theme,
        radius,
        kelvin,
    );
    let extent = i64::from(radius) + 3_000;
    let orbit = cursor + extent;
    if orbit + extent > limit {
        return None;
    }
    let angle = rng.range(0, TURN as i32 - 1) as u32;
    let (x, y) = polar(orbit, angle);
    rock.centre = SpaceTile::new(x, y);
    rock.orbit_radius = orbit as u32;
    rock.orbit_angle = angle;
    rock.extent = extent as u32;
    system.bodies.push(rock);
    Some(extent * 2)
}

fn belt(
    rng: &mut RegionRng,
    system: &mut StarSystem,
    cursor: i64,
    kelvin: i32,
    limit: i64,
) -> Option<i64> {
    let width = i64::from(rng.range(12_000, 40_000));
    if cursor + width > limit {
        return None;
    }
    let weights: [u32; 7] = if kelvin > 260 {
        [30, 22, 4, 14, 8, 6, 2]
    } else if kelvin > 160 {
        [22, 16, 18, 12, 8, 6, 3]
    } else {
        [10, 8, 40, 8, 6, 6, 4]
    };
    let ore = AsteroidOre::ALL[rng.pick(&weights).unwrap_or(0)];
    let middle = cursor + width / 2;
    let circumference = middle * 710 / 113;
    let segments = (circumference / BELT_SEGMENT_TILES).max(64) as u32;
    system.belts.push(AsteroidBelt {
        index: system.belts.len() as u8,
        inner: cursor as u32,
        outer: (cursor + width) as u32,
        density: rng.range(4, 16) as u8,
        ore,
        segments,
    });
    Some(width)
}

// ---------------------------------------------------------------------------------------------
// Star charts.
// ---------------------------------------------------------------------------------------------

/// The real system a star chart points at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChartTarget {
    pub system: SystemRef,
    pub id: u64,
    pub class: StarClass,
    /// Its planets (at least one).
    pub planets: u8,
}

/// The system a chart for `sector` names.
///
/// Picked by `roll` among the sector's systems with planets (a sector's first system always
/// has one). `star_chart::chart_at_site` uses this,
/// so a chart's id, class and world count are the generator's.
#[must_use]
pub fn chart_target(seed: u64, sector: (i32, i32), roll: u64) -> ChartTarget {
    let coords = SpaceSector::new(sector.0, sector.1);
    let generated = generate_sector(seed, coords);
    let candidates: Vec<&StarSystem> = generated
        .systems
        .iter()
        .filter(|system| system.planet_count() > 0)
        .collect();
    let fallback = &generated.systems[0];
    let pick = if candidates.is_empty() {
        fallback
    } else {
        candidates[(roll % candidates.len() as u64) as usize]
    };
    ChartTarget {
        system: SystemRef {
            sector: coords,
            system: pick.index,
        },
        id: pick.id,
        class: pick.class,
        planets: pick.planet_count().clamp(1, usize::from(u8::MAX)) as u8,
    }
}

/// The system `chart` points at.
///
/// Charts read since Wave 11 carry the generator's id and resolve exactly; an older chart (its id hashed from its plinth) resolves to a system of
/// its sector picked by its id, so every chart leads somewhere real.
#[must_use]
pub fn resolve_chart(seed: u64, chart: &StarChart) -> SystemRef {
    let coords = SpaceSector::new(chart.sector.0, chart.sector.1);
    let generated = generate_sector(seed, coords);
    let index = generated
        .systems
        .iter()
        .find(|system| system.id == chart.system)
        .map_or_else(
            || (chart.system % generated.systems.len().max(1) as u64) as u8,
            |system| system.index,
        );
    SystemRef {
        sector: coords,
        system: index,
    }
}
