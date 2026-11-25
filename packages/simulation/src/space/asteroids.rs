//! Belt asteroids, generated per angular segment on demand (a belt holds thousands; nothing
//! stores them). A segment's asteroids are a pure function of the seed, the system, the belt
//! and the segment index, so an [`AsteroidId`] names one rock forever (Wave 12 keeps the set
//! of mined ones).

use super::{
    ASTEROID_MAX_RADIUS, Asteroid, AsteroidBelt, AsteroidId, AsteroidOre, SpaceTile, StarSystem,
    TURN,
    trig::{angle_of, polar},
};
use crate::worldgen::noise::RegionRng;

const SALT_ASTEROID: u64 = 0x5BAC_ED00;
/// Most asteroids one query returns (a query wider than a few segments is a mistake).
pub const MAX_ASTEROIDS_PER_QUERY: usize = 4_096;

/// Segments around `belt`.
#[must_use]
pub const fn belt_segments(belt: &AsteroidBelt) -> u32 {
    belt.segments
}

/// The asteroids of `belt`'s segment `segment` in `system` (whose sector is `id.sector`).
#[must_use]
pub fn asteroids_in_segment(
    seed: u64,
    system: &StarSystem,
    sector: super::SpaceSector,
    belt: &AsteroidBelt,
    segment: u32,
) -> Vec<Asteroid> {
    let segments = belt.segments.max(1);
    let segment = segment % segments;
    let mut rng = RegionRng::new(
        seed ^ system.id,
        SALT_ASTEROID + u64::from(belt.index),
        segment as i32,
        0,
    );
    let low = u64::from(segment) * u64::from(TURN) * 64 / u64::from(segments);
    let high = (u64::from(segment) + 1) * u64::from(TURN) * 64 / u64::from(segments);
    let density = i32::from(belt.density.max(1));
    let count = rng.range(density / 2, density);
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        // Angles in 1/64 of a TURN unit: finer than the trig table, rounded to it.
        let fine = low + rng.next_u64() % (high - low).max(1);
        let angle = (fine / 64) as u32;
        let r = i64::from(rng.range(belt.inner as i32, belt.outer as i32));
        let (dx, dy) = polar(r, angle);
        // Along-arc jitter inside the angle step, so rocks don't line up on spokes.
        let along = ((fine % 64) as i64 * r / (64 * i64::from(TURN) / 7)).min(r);
        let (jx, jy) = polar(along, angle + TURN / 4);
        let size_roll = rng.next_u64() % 1_000;
        let radius = if size_roll < 900 {
            rng.range(3, 40)
        } else if size_roll < 990 {
            rng.range(40, 150)
        } else {
            rng.range(150, i32::from(ASTEROID_MAX_RADIUS))
        } as u16;
        let ore = if rng.chance(600) {
            belt.ore
        } else {
            AsteroidOre::ALL[rng.pick(&[30, 22, 18, 12, 8, 6, 4]).unwrap_or(0)]
        };
        let yield_units = (u32::from(radius) * u32::from(radius) / 8 + 4).min(u32::from(u16::MAX));
        out.push(Asteroid {
            id: AsteroidId {
                sector,
                system: system.index,
                belt: belt.index,
                segment,
                index: index as u8,
            },
            centre: system.centre.offset(dx + jx, dy + jy),
            radius,
            ore,
            yield_units: yield_units as u16,
        });
    }
    out
}

/// The segments of `belt` a disc of `radius` tiles around `at` can touch (padded by one
/// segment each side; empty when the disc misses the belt's annulus).
pub(super) fn segments_near(
    system: &StarSystem,
    belt: &AsteroidBelt,
    at: SpaceTile,
    radius: i64,
) -> Vec<u32> {
    let reach = radius + i64::from(ASTEROID_MAX_RADIUS) + 1_000;
    let (dx, dy) = system.centre.delta(at);
    let distance = system.centre.distance(at);
    if distance + (reach as f64) < f64::from(belt.inner)
        || distance - (reach as f64) > f64::from(belt.outer)
    {
        return Vec::new();
    }
    let segments = belt.segments.max(1);
    if distance <= reach as f64 * 2.0 {
        return (0..segments).collect();
    }
    // The disc subtends asin(reach / distance) <= reach / (distance - reach) radians.
    let half_turns = (reach as f64) / (distance - reach as f64) / std::f64::consts::TAU;
    if half_turns >= 0.5 {
        return (0..segments).collect();
    }
    let centre = angle_of(dx, dy) / f64::from(TURN) * f64::from(segments);
    let half = half_turns * f64::from(segments) + 1.0;
    let low = (centre - half).floor() as i64;
    let high = (centre + half).ceil() as i64;
    let span = (high - low + 1).min(i64::from(segments));
    (0..span)
        .map(|step| (low + step).rem_euclid(i64::from(segments)) as u32)
        .collect()
}
