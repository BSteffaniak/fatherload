//! Where the builders' road goes (Wave 13 story): the story plan, a pure function of the world
//! seed and the system the road starts from.
//!
//! The road starts at the **anchor**: the system of the crew's first star chart from the
//! Archive of Stars ([`anchor_of`]; a world that read no chart starts from the system its
//! shallowest plinth would have charted, so the Descent's archive keys always point at the
//! first system). From there it runs outward along the **heading** (home to anchor), each
//! chapter's stop a little further down it:
//!
//! * a theme stop is the nearest landable planet of one of its themes round its point on the
//!   heading (searched ring by ring, the earlier theme preferred at equal distance; any planet
//!   if no theme is near), never a world an earlier chapter used;
//! * the species stop is the nearest species' first home ([`crate::alien::atlas`]);
//! * the lair stop is the faction boss's lair ([`crate::hostiles::boss_lair`]);
//! * the **Harbour** lies [`HARBOUR_REACH_SECTORS`] past the anchor along the heading (at
//!   least [`HARBOUR_MIN_SECTORS`] from home): the first system there, a red star preferred.
//!   Its ring of world-ships circles the star at [`HARBOUR_RING_TILES`]; the **gate** is on the
//!   ring, on the side facing home.
//!
//! Relics stand at a column of their world hashed from the seed, the world and the chapter,
//! `relic_rows` under its surface (the theme world's own surface: a theme registered by a
//! later track moves the relic onto its ground with it).
//!
//! A theme world with its own builders' outpost (Wave 13 integration: the fungal, machine and
//! dead worlds' glyph obelisks, the gas giant's aerostat, the locked world's observatory, the
//! ring world's station) holds the relic there instead ([`RelicSite::base`] names the
//! outpost's floor); other worlds keep the hashed column.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use crate::{
    flight::space::hash64,
    hostiles::Faction,
    space::{
        BodyKind, HOME_SECTOR, PLANET_RADIUS, Space, SpaceBodyId, SpaceSector, SpaceTile,
        StarSystem, SystemRef, Theme,
    },
    star_chart::{StarChart, StarClass, isqrt},
};

use super::chapters::{CHAPTER_COUNT, CHAPTERS, StopKind};

/// How far past the anchor (sectors) the Harbour lies.
pub const HARBOUR_REACH_SECTORS: i64 = 20;
/// The Harbour is never nearer home than this (sectors).
pub const HARBOUR_MIN_SECTORS: i64 = 24;
/// The radius of the Harbour's ring of world-ships round its star (tiles).
pub const HARBOUR_RING_TILES: i64 = 60_000;
/// A rig within this of the gate has reached the Harbour (tiles).
pub const HARBOUR_REACH_TILES: i64 = 30_000;
/// Within this of the gate the Harbour's space is calm (no roaming encounters, tiles).
pub const HARBOUR_CALM_TILES: i64 = 120_000;
/// How far (sectors) round a stop's point the road looks for its world.
pub const STOP_SEARCH_RINGS: i32 = 6;

const SALT_RELIC: u64 = 0x6A1A_0001;

/// Where a relic is on its world.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelicSite {
    /// Strip column (`0..surface_columns`).
    pub column: u32,
    /// Rows under its base (0: standing on it).
    pub rows: u16,
    /// The row it stands on when that is not the ground's surface at its column: a builder
    /// outpost's floor (a sky city's deck over a gas giant's clouds, an obelisk's plinth).
    /// `None`: the surface.
    pub base: Option<i32>,
}

impl RelicSite {
    /// The site's row on `ground`: `rows` under its base.
    #[must_use]
    pub fn row_on(self, ground: &crate::planet::PlanetGround) -> i32 {
        let column = i32::try_from(self.column).unwrap_or(0);
        self.base
            .unwrap_or_else(|| crate::planet::BodyGround::surface_row(ground, column))
            + i32::from(self.rows)
    }
}

/// One chapter's stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stop {
    pub system: SystemRef,
    /// The world, if the chapter has one.
    pub body: Option<SpaceBodyId>,
    /// Its theme (the world's).
    pub theme: Option<Theme>,
    /// The relic, if the chapter has one.
    pub relic: Option<RelicSite>,
    /// A point in flight (the lair's, the Harbour's gate), if the stop has one.
    pub point: Option<SpaceTile>,
}

/// The Harbour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Harbour {
    pub system: SystemRef,
    /// Its star.
    pub star: SpaceTile,
    /// The gate on its ring, facing home.
    pub gate: SpaceTile,
}

impl Harbour {
    /// Whether `tile` is within `radius` tiles of the gate.
    #[must_use]
    pub const fn near_gate(&self, tile: SpaceTile, radius: i64) -> bool {
        self.gate.within(tile, radius)
    }
}

/// The whole road.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryPlan {
    pub anchor: SystemRef,
    pub stops: Vec<Stop>,
    pub harbour: Harbour,
}

impl StoryPlan {
    /// The stop of chapter `chapter`.
    #[must_use]
    pub fn stop(&self, chapter: u8) -> Option<&Stop> {
        self.stops.get(usize::from(chapter))
    }
}

/// The system the road starts from: the crew's first chart's (resolved by the space
/// generator), else the one the Archive's first plinth would chart.
#[must_use]
pub fn anchor_of(seed: crate::space::SpaceSeed, charts: &[StarChart]) -> SystemRef {
    let space = Space::shared(seed);
    charts.first().map_or_else(
        || {
            let chart = crate::star_chart::chart_at_site(
                seed.seed,
                0,
                0,
                0,
                crate::ids::SimulationTick::new(0),
            );
            space.chart_system(&chart)
        },
        |chart| space.chart_system(chart),
    )
}

/// The plan of the world seeded `seed` whose road starts at `anchor` (memoized).
#[must_use]
pub fn plan_for(seed: crate::space::SpaceSeed, anchor: SystemRef) -> Arc<StoryPlan> {
    type PlanCache = Mutex<BTreeMap<(crate::space::SpaceSeed, SystemRef), Arc<StoryPlan>>>;
    static CACHE: PlanCache = Mutex::new(BTreeMap::new());
    if let Some(found) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(seed, anchor))
    {
        return Arc::clone(found);
    }
    let plan = Arc::new(compute_plan(seed, anchor));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 32 {
        cache.clear();
    }
    cache.insert((seed, anchor), Arc::clone(&plan));
    plan
}

/// The unit heading from home toward `anchor`, scaled by 1,024.
fn heading(anchor: SystemRef) -> (i64, i64) {
    let dx = i64::from(anchor.sector.x - HOME_SECTOR.x);
    let dy = i64::from(anchor.sector.y - HOME_SECTOR.y);
    if dx == 0 && dy == 0 {
        return (1_024, 0);
    }
    // An IEEE square root of an exact integer: the same bits on every platform.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "a few dozen sectors; the unit vector rounds to whole 1/1024ths"
    )]
    let unit = |part: i64| -> i64 {
        let length = ((dx * dx + dy * dy) as f64).sqrt();
        (part as f64 * 1_024.0 / length).round() as i64
    };
    (unit(dx), unit(dy))
}

/// The sector `distance` sectors from home along `heading`.
fn along(heading: (i64, i64), distance: i64) -> SpaceSector {
    let x = heading.0 * distance / 1_024;
    let y = heading.1 * distance / 1_024;
    SpaceSector::new(
        HOME_SECTOR.x + i32::try_from(x).unwrap_or(0),
        HOME_SECTOR.y + i32::try_from(y).unwrap_or(0),
    )
}

/// The sectors `ring` away (Chebyshev) round `centre`, in a fixed order.
fn ring_sectors(centre: SpaceSector, ring: i32) -> Vec<SpaceSector> {
    let mut out = Vec::new();
    for y in -ring..=ring {
        for x in -ring..=ring {
            if x.abs().max(y.abs()) == ring {
                out.push(SpaceSector::new(centre.x + x, centre.y + y));
            }
        }
    }
    out
}

/// Whether a body makes a stop: a landable planet with full strata (not home's).
fn fits(seed: crate::space::SpaceSeed, id: SpaceBodyId, kind: BodyKind, radius: u32) -> bool {
    kind == BodyKind::Planet
        && radius >= PLANET_RADIUS.0
        && crate::place::planet_params_for(seed, id).is_some()
}

/// The biggest landable planet of `system` (the anchor's world).
fn biggest_world(
    seed: crate::space::SpaceSeed,
    reference: SystemRef,
    system: &StarSystem,
) -> Option<SpaceBodyId> {
    system
        .bodies
        .iter()
        .filter_map(|body| {
            let id = SpaceBodyId {
                sector: reference.sector,
                system: reference.system,
                body: body.index,
            };
            fits(seed, id, body.kind, body.radius).then_some((body.radius, id))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, id)| id)
}

/// The nearest world round `centre` of one of `themes`, not in `used`: the least of its ring
/// plus twice its theme's place in the list (a world of no listed theme counts as the list's
/// length: any world does when no theme is near), the first found among equals.
fn theme_world(
    seed: crate::space::SpaceSeed,
    space: &Space,
    centre: SpaceSector,
    themes: &[Theme],
    used: &[SpaceBodyId],
) -> Option<(SpaceBodyId, Theme)> {
    let stranger = i32::try_from(themes.len()).unwrap_or(8) * 2 + 2;
    let mut best: Option<(i32, SpaceBodyId, Theme)> = None;
    for ring in 0..=STOP_SEARCH_RINGS {
        // Nothing further out can cost less than what is found.
        if best.is_some_and(|(cost, ..)| ring >= cost) {
            break;
        }
        for coords in ring_sectors(centre, ring) {
            if coords == HOME_SECTOR {
                continue;
            }
            let sector = space.sector(coords);
            for system in &sector.systems {
                for body in &system.bodies {
                    let id = SpaceBodyId {
                        sector: coords,
                        system: system.index,
                        body: body.index,
                    };
                    if used.contains(&id) || !fits(seed, id, body.kind, body.radius) {
                        continue;
                    }
                    let rank = themes
                        .iter()
                        .position(|theme| *theme == body.theme)
                        .map_or(stranger, |rank| i32::try_from(rank).unwrap_or(8) * 2);
                    let cost = ring + rank;
                    if best.is_none_or(|(best_cost, ..)| cost < best_cost) {
                        best = Some((cost, id, body.theme));
                    }
                }
            }
        }
    }
    best.map(|(_, id, theme)| (id, theme))
}

/// The relic of chapter `chapter` on world `id` with `columns` surface columns.
fn relic_at(
    seed: crate::space::SpaceSeed,
    id: SpaceBodyId,
    columns: u32,
    chapter: u8,
    rows: u16,
) -> RelicSite {
    let roll = hash64(
        seed.seed,
        SALT_RELIC ^ u64::from(chapter),
        i64::from(id.sector.x) * 64 + i64::from(id.system),
        i64::from(id.sector.y) * 64 + i64::from(id.body),
    );
    RelicSite {
        column: u32::try_from(roll % u64::from(columns.max(1))).unwrap_or(0),
        rows,
        base: None,
    }
}

/// Columns from an outland world's obelisk to where its relic stands on the plinth (clear of
/// the obelisk itself).
const OBELISK_SIDE_COLUMNS: i64 = 4;

/// The relic of world `id` at its builders' outpost, if the world has one (Wave 13
/// integration: the fungal, machine and dead worlds' glyph obelisks over their sealed caches,
/// the gas giant's aerostat, the locked world's observatory and the ring world's station). A
/// relic stands (or lies `rows` under) the obelisk's plinth beside it, or the outpost's floor at
/// its beacon.
pub(crate) fn outpost_relic(
    seed: crate::space::SpaceSeed,
    id: SpaceBodyId,
    rows: u16,
) -> Option<RelicSite> {
    let params = crate::place::planet_params_for(seed, id)?;
    let ground = crate::planet::PlanetGround::new(params);
    let width = i64::from(ground.strip_width().max(1));
    let (column, base) =
        if let Some(outpost) = crate::planet::outlands::builder_outposts(&ground).first() {
            (
                i64::from(outpost.glyph.0) + OBELISK_SIDE_COLUMNS,
                outpost.site.floor,
            )
        } else {
            let outpost = crate::exotic::builder_outpost(&ground)?;
            (i64::from(outpost.column), outpost.floor)
        };
    Some(RelicSite {
        column: u32::try_from(column.rem_euclid(width)).unwrap_or(0),
        rows,
        base: Some(base),
    })
}

/// The space generator's reference of flight's `system` (a lair names a flight reference).
fn space_ref(space: &Space, system: crate::flight::space::SystemRef) -> Option<SystemRef> {
    let coords = SpaceSector::new(
        i32::try_from(system.sector.0).ok()?,
        i32::try_from(system.sector.1).ok()?,
    );
    space
        .sector(coords)
        .systems
        .iter()
        .find(|found| crate::flight::space::GeneratedSpace::reference(coords, found) == system)
        .map(|found| SystemRef {
            sector: coords,
            system: found.index,
        })
}

fn harbour_of(seed: crate::space::SpaceSeed, space: &Space, anchor: SystemRef) -> Harbour {
    let way = heading(anchor);
    let anchor_distance = {
        let dx = i64::from(anchor.sector.x - HOME_SECTOR.x);
        let dy = i64::from(anchor.sector.y - HOME_SECTOR.y);
        isqrt(dx * dx + dy * dy)
    };
    let distance = (anchor_distance + HARBOUR_REACH_SECTORS).max(HARBOUR_MIN_SECTORS);
    let centre = along(way, distance);
    let mut found: Option<(bool, SystemRef, SpaceTile)> = None;
    'rings: for ring in 0..=4 {
        for coords in ring_sectors(centre, ring) {
            for system in &space.sector(coords).systems {
                let red = system.class == StarClass::Red;
                if found.is_none() || red && !found.is_some_and(|(was_red, ..)| was_red) {
                    found = Some((
                        red,
                        SystemRef {
                            sector: coords,
                            system: system.index,
                        },
                        system.centre,
                    ));
                }
                if red {
                    break 'rings;
                }
            }
        }
        if found.is_some() && ring >= 1 {
            break;
        }
    }
    let (_, system, star) = found.unwrap_or_else(|| {
        (
            false,
            SystemRef {
                sector: centre,
                system: 0,
            },
            centre.centre(),
        )
    });
    // The gate faces home: back along the heading from the star.
    let gate = star.offset(
        -way.0 * HARBOUR_RING_TILES / 1_024,
        -way.1 * HARBOUR_RING_TILES / 1_024,
    );
    let _ = seed;
    Harbour { system, star, gate }
}

/// The plan computed afresh (tests check the cache against it).
#[cfg(test)]
pub(super) fn compute_for_tests(seed: crate::space::SpaceSeed, anchor: SystemRef) -> StoryPlan {
    compute_plan(seed, anchor)
}

fn compute_plan(seed: crate::space::SpaceSeed, anchor: SystemRef) -> StoryPlan {
    let space = Space::shared(seed);
    let way = heading(anchor);
    let anchor_distance = {
        let dx = i64::from(anchor.sector.x - HOME_SECTOR.x);
        let dy = i64::from(anchor.sector.y - HOME_SECTOR.y);
        isqrt(dx * dx + dy * dy)
    };
    let harbour = harbour_of(seed, &space, anchor);
    let mut used: Vec<SpaceBodyId> = Vec::new();
    let mut stops = Vec::with_capacity(usize::from(CHAPTER_COUNT));
    // How far down the road (sectors past the anchor) each theme stop's point lies.
    let mut theme_stops = 0_i64;
    for (index, chapter) in CHAPTERS.iter().enumerate() {
        let index = u8::try_from(index).unwrap_or(u8::MAX);
        let stop = match chapter.stop {
            StopKind::Anchor => {
                let body = space
                    .system(anchor)
                    .and_then(|system| biggest_world(seed, anchor, &system))
                    .or_else(|| {
                        theme_world(seed, &space, anchor.sector, &[], &used).map(|(id, _)| id)
                    });
                let system = body.map_or(anchor, SpaceBodyId::system_ref);
                stop_with_world(seed, &space, system, body, index, chapter.relic_rows)
            }
            StopKind::Theme(themes) => {
                theme_stops += 1;
                let centre = along(way, anchor_distance + theme_stops * 3);
                let found = theme_world(seed, &space, centre, themes, &used);
                let system = found.map_or(anchor, |(id, _)| id.system_ref());
                stop_with_world(
                    seed,
                    &space,
                    system,
                    found.map(|(id, _)| id),
                    index,
                    chapter.relic_rows,
                )
            }
            StopKind::Species => {
                let home = crate::alien::atlas::first_homes(seed.seed)
                    .into_iter()
                    .flatten()
                    .next();
                let system = home.map_or(anchor, SpaceBodyId::system_ref);
                Stop {
                    system,
                    body: home,
                    theme: home.and_then(|id| space.body(id)).map(|body| body.theme),
                    relic: None,
                    point: None,
                }
            }
            StopKind::Lair(faction) => lair_stop(seed, &space, faction).unwrap_or(Stop {
                system: anchor,
                body: None,
                theme: None,
                relic: None,
                point: None,
            }),
            StopKind::Harbour => Stop {
                system: harbour.system,
                body: None,
                theme: None,
                relic: None,
                point: Some(harbour.gate),
            },
        };
        if let Some(body) = stop.body {
            used.push(body);
        }
        stops.push(stop);
    }
    StoryPlan {
        anchor,
        stops,
        harbour,
    }
}

fn stop_with_world(
    seed: crate::space::SpaceSeed,
    space: &Space,
    system: SystemRef,
    body: Option<SpaceBodyId>,
    chapter: u8,
    rows: u16,
) -> Stop {
    let found = body.and_then(|id| space.body(id).map(|found| (id, found)));
    Stop {
        system,
        body,
        theme: found.as_ref().map(|(_, found)| found.theme),
        relic: found.map(|(id, found)| {
            outpost_relic(seed, id, rows)
                .unwrap_or_else(|| relic_at(seed, id, found.surface_columns, chapter, rows))
        }),
        point: None,
    }
}

fn lair_stop(seed: crate::space::SpaceSeed, space: &Space, faction: Faction) -> Option<Stop> {
    let lair = crate::hostiles::boss_lair(seed, faction)?;
    let system = space_ref(space, lair.system)?;
    Some(Stop {
        system,
        body: None,
        theme: None,
        relic: None,
        point: Some(SpaceTile::of_pos(lair.pos)),
    })
}
