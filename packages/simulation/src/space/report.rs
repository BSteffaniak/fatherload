//! `--space-report`: a text summary of the space around home, for sanity-checking the
//! generator's distribution (deterministic: a pure function of seed and radius).

use std::{collections::BTreeMap, fmt::Write as _};

use super::{
    BodyKind, HOME_SECTOR, Sector, SpaceSector, Theme, WormholeId, generate_sector,
    home::home_departure,
};
use crate::star_chart::StarClass;

/// Radius histogram buckets, tiles (`[low, high)`).
const BUCKETS: [(u32, u32); 7] = [
    (400, 700),
    (700, 1_001),
    (1_500, 3_000),
    (3_000, 5_000),
    (5_000, 7_500),
    (7_500, 9_000),
    (9_000, 10_001),
];

/// The sectors within `radius` sectors of home (Chebyshev), row by row.
#[must_use]
pub fn sectors_around_home(radius: i32) -> Vec<SpaceSector> {
    let mut out = Vec::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            out.push(SpaceSector::new(HOME_SECTOR.x + x, HOME_SECTOR.y + y));
        }
    }
    out
}

/// The report for the sectors within `radius` sectors of home on `seed`.
#[must_use]
pub fn space_report(seed: u64, radius: i32) -> String {
    let sectors: Vec<Sector> = sectors_around_home(radius)
        .into_iter()
        .map(|coords| generate_sector(seed, coords))
        .collect();
    report_of(seed, radius, &sectors)
}

#[allow(clippy::too_many_lines)]
fn report_of(seed: u64, radius: i32, sectors: &[Sector]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Space report: seed {seed:#x}, {} sectors within {radius} of home ({} tiles a side)",
        sectors.len(),
        super::SPACE_SECTOR_TILES
    );
    let mut classes: BTreeMap<StarClass, usize> = BTreeMap::new();
    let mut kinds: BTreeMap<BodyKind, usize> = BTreeMap::new();
    let mut themes: BTreeMap<Theme, (usize, i64)> = BTreeMap::new();
    let mut histogram = [0_usize; BUCKETS.len()];
    let mut smallest = u32::MAX;
    let mut largest = 0;
    let (mut belts, mut segments, mut comets, mut derelicts) = (0, 0_u64, 0, 0);
    let mut stations: BTreeMap<&str, usize> = BTreeMap::new();
    let mut nebulae = std::collections::BTreeSet::new();
    let mut black_holes = 0;
    let mut wormholes: BTreeMap<WormholeId, (SpaceSector, SpaceSector)> = BTreeMap::new();
    let mut systems = 0;
    let mut planets_per_system = [0_usize; 10];
    for sector in sectors {
        let reach = sector.coords.chebyshev(HOME_SECTOR);
        for system in &sector.systems {
            systems += 1;
            *classes.entry(system.class).or_default() += 1;
            planets_per_system[system.planet_count().min(9)] += 1;
            for body in &system.bodies {
                *kinds.entry(body.kind).or_default() += 1;
                let entry = themes.entry(body.theme).or_insert((0, i64::MAX));
                entry.0 += 1;
                entry.1 = entry.1.min(reach);
                smallest = smallest.min(body.radius);
                largest = largest.max(body.radius);
                if let Some(bucket) = BUCKETS
                    .iter()
                    .position(|(low, high)| (*low..*high).contains(&body.radius))
                {
                    histogram[bucket] += 1;
                }
            }
            belts += system.belts.len();
            segments += system
                .belts
                .iter()
                .map(|belt| u64::from(belt.segments))
                .sum::<u64>();
            comets += system.comets.len();
            derelicts += system.derelicts.len();
            for station in &system.stations {
                *stations.entry(station.kind.name()).or_default() += 1;
            }
            if let Some(nebula) = system.nebula {
                nebulae.insert(nebula.id);
            }
        }
        derelicts += sector.derelicts.len();
        black_holes += sector.black_holes.len();
        for nebula in &sector.nebulae {
            nebulae.insert(nebula.id);
        }
        for mouth in &sector.wormholes {
            wormholes.insert(mouth.id, (sector.coords, mouth.exit_sector));
        }
    }
    let _ = writeln!(out, "\nsystems: {systems}");
    for (class, count) in &classes {
        let _ = writeln!(out, "  {:<12} {count}", class.name());
    }
    let _ = write!(out, "planets per system:");
    for (count, systems) in planets_per_system.iter().enumerate() {
        let _ = write!(out, " {count}:{systems}");
    }
    let _ = writeln!(out, "\n\nbodies:");
    for (kind, count) in &kinds {
        let _ = writeln!(out, "  {:<12} {count}", kind.name());
    }
    let _ = writeln!(out, "  radius {smallest}..={largest} tiles");
    let _ = writeln!(out, "\nthemes (count, nearest sector from home):");
    for theme in Theme::PLANET
        .iter()
        .chain(&[Theme::Barren, Theme::Home, Theme::Moon])
    {
        match themes.get(theme) {
            Some((count, nearest)) => {
                let _ = writeln!(out, "  {:<18} {count:>5}  {nearest}", theme.name());
            }
            None => {
                let _ = writeln!(out, "  {:<18}     -  none", theme.name());
            }
        }
    }
    // The ground each tag lands on (the planet theme registry; Wave 12 planets-a).
    let _ = writeln!(out, "\nground themes (tag -> planet theme: strata):");
    for theme in Theme::PLANET {
        let ground = crate::planet::theme(crate::place::planet_theme_of(theme));
        let strata: Vec<&str> = ground.strata.iter().map(|stratum| stratum.name).collect();
        let world = if crate::planet::world_of(ground.id).is_some() {
            " (world)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {:<18} {}{world}: {}",
            theme.name(),
            ground.name,
            strata.join(", ")
        );
    }
    let _ = writeln!(out, "\nradius histogram (tiles):");
    let total = histogram.iter().sum::<usize>().max(1);
    for ((low, high), count) in BUCKETS.iter().zip(histogram) {
        let bar = "#".repeat(count * 50 / total);
        let _ = writeln!(out, "  {low:>5}..{high:<6} {count:>5} {bar}");
    }
    let _ = writeln!(
        out,
        "\nbelts: {belts} ({segments} segments)  comets: {comets}  derelicts: {derelicts}  \
         black holes: {black_holes}  nebulae: {}",
        nebulae.len()
    );
    let _ = write!(out, "stations:");
    for (kind, count) in &stations {
        let _ = write!(out, " {kind} {count}");
    }
    let _ = writeln!(out, "\n\nwormhole pairs ({}):", wormholes.len());
    for (id, (here, there)) in &wormholes {
        let _ = writeln!(
            out,
            "  regions {:?}<->{:?}: sector ({}, {}) <-> ({}, {}), {:.0} sectors",
            id.a,
            id.b,
            here.x,
            here.y,
            there.x,
            there.y,
            here.distance(*there)
        );
    }
    if let Some(home) = sectors
        .iter()
        .find(|sector| sector.coords == HOME_SECTOR)
        .and_then(|sector| sector.systems.first())
    {
        let departure = home_departure(home);
        let _ = writeln!(
            out,
            "\nhome: sun at ({}, {}), {} bodies, departure above the Moon at ({}, {})",
            home.centre.x,
            home.centre.y,
            home.bodies.len(),
            departure.x,
            departure.y
        );
    }
    out
}
