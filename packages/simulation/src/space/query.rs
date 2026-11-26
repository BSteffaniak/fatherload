//! [`Space`]: the generator behind a bounded sector cache, and the queries the other space
//! tracks read (flight, regions, polar, rendering, the nav HUD).

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex, PoisonError},
};

use super::{
    Asteroid, BodyKind, HOME_SECTOR, SPACE_SECTOR_TILES, Sector, SpaceBody, SpaceBodyId,
    SpaceSector, SpaceSeed, SpaceTile, StarSystem, StationId, StationKind, SystemRef, Theme,
    WormholeId, WormholeMouth,
    asteroids::{MAX_ASTEROIDS_PER_QUERY, asteroids_in_segment, segments_near},
    generate::generate_sector,
    home::{HOME_WORLD_BODY, MOON_BODY, home_departure},
    resolve_chart,
};
use crate::{
    position::WorldPos,
    star_chart::{StarChart, StarClass},
    terrain::TILE_SIZE,
    worldgen::WorldSettings,
};

/// Sectors kept generated (a few dozen: a craft's neighbourhood plus a warp's two ends).
pub const CACHE_SECTORS: usize = 96;
/// Widest query, tiles (four sectors): wider asks are clamped.
pub const MAX_QUERY_RADIUS: i64 = 4 * SPACE_SECTOR_TILES;
/// Weakest pull a gravity source counts at, milli-g (its influence radius ends there).
pub const MIN_PULL_MILLI_G: u32 = 2;
/// How far gravity sources are looked for, sectors (Chebyshev).
const GRAVITY_SECTORS: i32 = 1;

impl From<WorldPos> for SpaceTile {
    fn from(pos: WorldPos) -> Self {
        Self::of_pos(pos)
    }
}

#[derive(Default)]
struct SectorCache {
    sectors: BTreeMap<SpaceSector, (u64, Arc<Sector>)>,
    clock: u64,
}

/// A world's space: its seed and a bounded cache of generated sectors. Cloning gives an empty
/// cache (the cache is never state: every answer is a pure function of the seed).
pub struct Space {
    seed: SpaceSeed,
    cache: Mutex<SectorCache>,
}

impl fmt::Debug for Space {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Space")
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl Clone for Space {
    fn clone(&self) -> Self {
        Self::new(self.seed)
    }
}

impl PartialEq for Space {
    fn eq(&self, other: &Self) -> bool {
        self.seed == other.seed
    }
}

impl Eq for Space {}

/// A body found near a point.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyHit {
    pub id: SpaceBodyId,
    pub kind: BodyKind,
    pub theme: Theme,
    pub centre: SpaceTile,
    pub radius: u32,
    /// Distance from the point to the body's centre, tiles.
    pub distance: f64,
}

impl BodyHit {
    /// Altitude above its surface, tiles (negative inside).
    #[must_use]
    pub fn altitude(&self) -> f64 {
        self.distance - f64::from(self.radius)
    }
}

/// What pulls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GravityKind {
    Star { system: SystemRef, star: u8 },
    Body(SpaceBodyId),
    BlackHole(u64),
}

/// One source of radial gravity (stars, landable bodies, black holes). The flight track
/// blends them ([`Self::pull_g`], or [`Space::gravity_g`] for the sum).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GravitySource {
    pub kind: GravityKind,
    pub centre: SpaceTile,
    /// Radius, tiles (an event horizon for a black hole).
    pub radius: u32,
    /// Pull at the surface, milli-g.
    pub surface_milli_g: u32,
}

impl GravitySource {
    /// How far its pull reaches, tiles: until it weakens to [`MIN_PULL_MILLI_G`], and never
    /// past one sector (so [`Space::gravity_sources_near`] finds every source that reaches).
    #[must_use]
    pub fn influence(&self) -> f64 {
        let reach = f64::from(self.radius)
            * (f64::from(self.surface_milli_g) / f64::from(MIN_PULL_MILLI_G)).sqrt();
        reach.min(SPACE_SECTOR_TILES as f64)
    }

    /// `g·μ`-style strength: surface pull × radius², milli-g·tiles² (a mass for blending).
    #[must_use]
    pub fn strength(&self) -> f64 {
        f64::from(self.surface_milli_g) * f64::from(self.radius) * f64::from(self.radius)
    }

    /// Its pull at `at`, in g (home world surface = 1), pointing at its centre: inverse
    /// square outside, falling linearly to zero at the centre inside. Zero beyond its
    /// influence. Uses the exact [`WorldPos`] delta, so it is smooth at any distance.
    #[must_use]
    pub fn pull_g(&self, at: WorldPos) -> (f64, f64) {
        let (dx, dy) = at.delta_f64(self.centre.pos());
        let tile = f64::from(TILE_SIZE);
        let (dx, dy) = (dx / tile, dy / tile);
        // Not `hypot` (platform libm) nor `mul_add` (a software fma on some browsers): plain
        // IEEE arithmetic, the same bits on every peer. The flight track steps with this.
        #[allow(
            clippy::suboptimal_flops,
            clippy::imprecise_flops,
            reason = "bit-identical on every platform"
        )]
        let distance = (dx * dx + dy * dy).sqrt();
        if distance < 1e-6 || distance > self.influence() {
            return (0.0, 0.0);
        }
        let radius = f64::from(self.radius).max(1.0);
        let surface = f64::from(self.surface_milli_g) / 1_000.0;
        let g = if distance >= radius {
            surface * (radius / distance) * (radius / distance)
        } else {
            surface * distance / radius
        };
        (g * dx / distance, g * dy / distance)
    }
}

/// A station found by [`Space::nearest_station`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StationHit {
    pub id: StationId,
    pub kind: StationKind,
    pub centre: SpaceTile,
    pub distance: f64,
}

/// What the scanner saw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContactKind {
    Star {
        system: SystemRef,
        star: u8,
        class: StarClass,
    },
    Body(SpaceBodyId),
    Station(StationId),
    Derelict {
        kind: super::DerelictKind,
        seed: u64,
    },
    Comet {
        system: SystemRef,
        index: u8,
    },
    BlackHole(u64),
    Wormhole(WormholeId),
}

/// One scanner contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScanContact {
    pub kind: ContactKind,
    pub centre: SpaceTile,
    pub distance: f64,
    /// Signal strength, per mille (1,000 on top of it, 0 at the edge of what fog allows).
    pub signal: u16,
}

/// The spaces [`Space::shared`] keeps (a few worlds' seeds at once: a host and its tools).
const SHARED_SPACES: usize = 4;

static SHARED: Mutex<Vec<Arc<Space>>> = Mutex::new(Vec::new());

impl Space {
    /// The process-wide space of the world seeded `seed` (Wave 11 integration: one cache per
    /// world, shared by flight, the body stores and the renderer). The cache is never state:
    /// every answer is a pure function of the seed, so sharing it changes nothing.
    #[must_use]
    pub fn shared(seed: SpaceSeed) -> Arc<Self> {
        let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = shared.iter().position(|space| space.seed == seed) {
            let space = shared.remove(index);
            shared.push(Arc::clone(&space));
            return space;
        }
        let space = Arc::new(Self::new(seed));
        if shared.len() >= SHARED_SPACES {
            shared.remove(0);
        }
        shared.push(Arc::clone(&space));
        space
    }

    /// The space of a world seeded `seed`.
    #[must_use]
    pub fn new(seed: SpaceSeed) -> Self {
        Self {
            seed,
            cache: Mutex::new(SectorCache::default()),
        }
    }

    /// The space of a world with `settings`, if its generator has space (V11+).
    #[must_use]
    pub fn for_world(settings: &WorldSettings) -> Option<Self> {
        super::has_space(settings.generator).then(|| Self::new(SpaceSeed::current(settings.seed)))
    }

    /// The world seed.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed.seed
    }

    /// The galaxy's key: its seed and generator (Wave 14 finish).
    #[must_use]
    pub const fn space_seed(&self) -> SpaceSeed {
        self.seed
    }

    /// Sector `coords`, generated once and cached.
    #[must_use]
    pub fn sector(&self, coords: SpaceSector) -> Arc<Sector> {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache.clock += 1;
        let clock = cache.clock;
        if let Some((stamp, sector)) = cache.sectors.get_mut(&coords) {
            *stamp = clock;
            return Arc::clone(sector);
        }
        let sector = Arc::new(generate_sector(self.seed, coords));
        if cache.sectors.len() >= CACHE_SECTORS {
            // Evict the least recently used.
            if let Some(oldest) = cache
                .sectors
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(key, _)| *key)
            {
                cache.sectors.remove(&oldest);
            }
        }
        cache.sectors.insert(coords, (clock, Arc::clone(&sector)));
        sector
    }

    /// How many sectors are cached.
    #[must_use]
    pub fn cached_sectors(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sectors
            .len()
    }

    /// Sectors whose square lies within `radius` tiles of `at` (clamped to
    /// [`MAX_QUERY_RADIUS`]), nearest rows first.
    #[must_use]
    pub fn sectors_near(&self, at: impl Into<SpaceTile>, radius: i64) -> Vec<Arc<Sector>> {
        let at = at.into();
        let radius = radius.clamp(0, MAX_QUERY_RADIUS);
        let low = SpaceSector::of_tile(at.offset(-radius, -radius));
        let high = SpaceSector::of_tile(at.offset(radius, radius));
        let r2 = i128::from(radius) * i128::from(radius);
        let mut out = Vec::new();
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                let coords = SpaceSector::new(x, y);
                if coords.distance2_to(at) <= r2 {
                    out.push(self.sector(coords));
                }
            }
        }
        out
    }

    /// Systems whose reach comes within `radius` tiles of `at`, nearest centre first.
    #[must_use]
    pub fn systems_near(&self, at: impl Into<SpaceTile>, radius: i64) -> Vec<(SystemRef, f64)> {
        let at = at.into();
        let mut out = Vec::new();
        for sector in self.sectors_near(at, radius) {
            for system in &sector.systems {
                let distance = system.centre.distance(at);
                if distance - f64::from(system.extent) <= radius as f64 {
                    out.push((
                        SystemRef {
                            sector: sector.coords,
                            system: system.index,
                        },
                        distance,
                    ));
                }
            }
        }
        out.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        out
    }

    /// System `reference`.
    #[must_use]
    pub fn system(&self, reference: SystemRef) -> Option<StarSystem> {
        self.sector(reference.sector)
            .system(reference.system)
            .cloned()
    }

    /// Body `id`.
    #[must_use]
    pub fn body(&self, id: SpaceBodyId) -> Option<SpaceBody> {
        self.sector(id.sector)
            .system(id.system)?
            .bodies
            .get(usize::from(id.body))
            .cloned()
    }

    /// Landable bodies whose surface comes within `radius` tiles of `at`, nearest first.
    #[must_use]
    pub fn bodies_near(&self, at: impl Into<SpaceTile>, radius: i64) -> Vec<BodyHit> {
        let at = at.into();
        let mut out = Vec::new();
        for sector in self.sectors_near(at, radius) {
            for system in &sector.systems {
                if system.centre.distance(at) - f64::from(system.extent) > radius as f64 {
                    continue;
                }
                for body in &system.bodies {
                    let distance = body.centre.distance(at);
                    if distance - f64::from(body.radius) <= radius as f64 {
                        out.push(BodyHit {
                            id: SpaceBodyId {
                                sector: sector.coords,
                                system: system.index,
                                body: body.index,
                            },
                            kind: body.kind,
                            theme: body.theme,
                            centre: body.centre,
                            radius: body.radius,
                            distance,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| a.altitude().total_cmp(&b.altitude()).then(a.id.cmp(&b.id)));
        out
    }

    /// The gravity sources whose pull reaches `at` (stars, bodies, black holes).
    #[must_use]
    pub fn gravity_sources_near(&self, at: impl Into<SpaceTile>) -> Vec<GravitySource> {
        let at = at.into();
        let here = SpaceSector::of_tile(at);
        let mut out = Vec::new();
        for y in here.y - GRAVITY_SECTORS..=here.y + GRAVITY_SECTORS {
            for x in here.x - GRAVITY_SECTORS..=here.x + GRAVITY_SECTORS {
                let sector = self.sector(SpaceSector::new(x, y));
                let mut push = |source: GravitySource| {
                    if source.centre.distance(at) <= source.influence() {
                        out.push(source);
                    }
                };
                for system in &sector.systems {
                    let reference = SystemRef {
                        sector: sector.coords,
                        system: system.index,
                    };
                    for (index, star) in system.stars.iter().enumerate() {
                        push(GravitySource {
                            kind: GravityKind::Star {
                                system: reference,
                                star: index as u8,
                            },
                            centre: star.centre,
                            radius: star.radius,
                            surface_milli_g: star.gravity_milli_g,
                        });
                    }
                    for body in &system.bodies {
                        push(GravitySource {
                            kind: GravityKind::Body(SpaceBodyId {
                                sector: sector.coords,
                                system: system.index,
                                body: body.index,
                            }),
                            centre: body.centre,
                            radius: body.radius,
                            surface_milli_g: u32::from(body.gravity_milli_g),
                        });
                    }
                }
                for hole in &sector.black_holes {
                    push(GravitySource {
                        kind: GravityKind::BlackHole(hole.id),
                        centre: hole.centre,
                        radius: hole.horizon,
                        surface_milli_g: hole.gravity_milli_g,
                    });
                }
            }
        }
        out
    }

    /// The summed radial pull at `at`, in g (see [`GravitySource::pull_g`]).
    #[must_use]
    pub fn gravity_g(&self, at: WorldPos) -> (f64, f64) {
        self.gravity_sources_near(at)
            .iter()
            .map(|source| source.pull_g(at))
            .fold((0.0, 0.0), |sum, pull| (sum.0 + pull.0, sum.1 + pull.1))
    }

    /// The nearest station within `max_sectors` sectors (searched ring by ring outward).
    #[must_use]
    pub fn nearest_station(
        &self,
        at: impl Into<SpaceTile>,
        max_sectors: i32,
    ) -> Option<StationHit> {
        let at = at.into();
        let here = SpaceSector::of_tile(at);
        let mut best: Option<StationHit> = None;
        for ring in 0..=max_sectors.max(0) {
            // Anything in a ring further out is at least (ring - 1) sectors away.
            if let Some(found) = &best
                && found.distance < (i64::from(ring - 1) * SPACE_SECTOR_TILES) as f64
            {
                break;
            }
            for y in here.y - ring..=here.y + ring {
                for x in here.x - ring..=here.x + ring {
                    if (x - here.x).abs() != ring && (y - here.y).abs() != ring {
                        continue;
                    }
                    let sector = self.sector(SpaceSector::new(x, y));
                    for station in sector.systems.iter().flat_map(|system| &system.stations) {
                        let distance = station.centre.distance(at);
                        if best.as_ref().is_none_or(|found| {
                            distance
                                .total_cmp(&found.distance)
                                .then(station.id.cmp(&found.id))
                                .is_lt()
                        }) {
                            best = Some(StationHit {
                                id: station.id,
                                kind: station.kind,
                                centre: station.centre,
                                distance,
                            });
                        }
                    }
                }
            }
        }
        best
    }

    /// Sensor fog at `at`, per mille (the thickest nebula there).
    #[must_use]
    pub fn fog_at(&self, at: impl Into<SpaceTile>) -> u16 {
        self.nebula_at(at).map_or(0, |(fog, _)| fog)
    }

    /// The thickest nebula at `at`: its fog there (per mille) and colour.
    #[must_use]
    pub fn nebula_at(&self, at: impl Into<SpaceTile>) -> Option<(u16, [u8; 3])> {
        let at = at.into();
        let here = SpaceSector::of_tile(at);
        let mut best: Option<(u16, [u8; 3])> = None;
        let mut consider = |fog: u16, colour: [u8; 3]| {
            if fog > 0 && best.is_none_or(|(thickest, _)| fog > thickest) {
                best = Some((fog, colour));
            }
        };
        let sector = self.sector(here);
        for nebula in &sector.nebulae {
            consider(nebula.fog_at(at), nebula.colour);
        }
        // A star's own nebula can spill a little past its sector.
        for y in here.y - 1..=here.y + 1 {
            for x in here.x - 1..=here.x + 1 {
                let sector = self.sector(SpaceSector::new(x, y));
                for nebula in sector.systems.iter().filter_map(|system| system.nebula) {
                    consider(nebula.fog_at(at), nebula.colour);
                }
            }
        }
        best
    }

    /// How fast time runs at `at` (1.0 but near a black hole).
    #[must_use]
    pub fn time_scale_at(&self, at: impl Into<SpaceTile>) -> f32 {
        let at = at.into();
        let sector = self.sector(SpaceSector::of_tile(at));
        sector
            .black_holes
            .iter()
            .map(|hole| hole.time_scale_at(at))
            .fold(1.0, f32::min)
    }

    /// The wormhole mouth `at` is inside, if any.
    #[must_use]
    pub fn wormhole_at(&self, at: impl Into<SpaceTile>) -> Option<WormholeMouth> {
        let at = at.into();
        let sector = self.sector(SpaceSector::of_tile(at));
        sector
            .wormholes
            .iter()
            .find(|mouth| mouth.centre.within(at, i64::from(mouth.radius)))
            .copied()
    }

    /// Belt asteroids touching the disc of `radius` tiles around `at` (at most
    /// [`MAX_ASTEROIDS_PER_QUERY`]; keep the radius to what a craft can reach or see).
    #[must_use]
    pub fn asteroids_near(&self, at: impl Into<SpaceTile>, radius: i64) -> Vec<Asteroid> {
        let at = at.into();
        let mut out = Vec::new();
        for sector in self.sectors_near(at, radius) {
            for system in &sector.systems {
                for belt in &system.belts {
                    for segment in segments_near(system, belt, at, radius) {
                        for rock in asteroids_in_segment(
                            self.seed.seed,
                            system,
                            sector.coords,
                            belt,
                            segment,
                        ) {
                            if rock.centre.within(at, radius + i64::from(rock.radius)) {
                                out.push(rock);
                                if out.len() >= MAX_ASTEROIDS_PER_QUERY {
                                    return out;
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// The scanner: what lies within `range` tiles of `at`, through the fog. Fog where the
    /// scanner is shortens its range by up to 70 %, fog around a target hides it by up to
    /// 50 % more; stars show from four times as far, black holes (their lensing) twice.
    /// Nearest first.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one pass over every kind of contact")]
    pub fn scan(&self, at: impl Into<SpaceTile>, range: i64) -> Vec<ScanContact> {
        let at = at.into();
        let range = range.clamp(0, MAX_QUERY_RADIUS / 4);
        let fog_here = i64::from(self.fog_at(at));
        let base = range * (1_000 - fog_here * 7 / 10) / 1_000;
        let mut out = Vec::new();
        let mut consider = |space: &Self, kind: ContactKind, centre: SpaceTile, boost: i64| {
            let reach = base * boost;
            if !centre.within(at, reach) {
                return;
            }
            let fog = i64::from(space.fog_at(centre));
            let reach = reach * (1_000 - fog / 2) / 1_000;
            let distance = centre.distance(at);
            if distance > reach as f64 || reach == 0 {
                return;
            }
            out.push(ScanContact {
                kind,
                centre,
                distance,
                signal: ((1.0 - distance / reach as f64) * 1_000.0) as u16,
            });
        };
        for sector in self.sectors_near(at, base * 4) {
            for system in &sector.systems {
                let reference = SystemRef {
                    sector: sector.coords,
                    system: system.index,
                };
                for (index, star) in system.stars.iter().enumerate() {
                    consider(
                        self,
                        ContactKind::Star {
                            system: reference,
                            star: index as u8,
                            class: system.class,
                        },
                        star.centre,
                        4,
                    );
                }
                if system.centre.distance(at) - f64::from(system.extent) > base as f64 {
                    continue;
                }
                for body in &system.bodies {
                    consider(
                        self,
                        ContactKind::Body(SpaceBodyId {
                            sector: sector.coords,
                            system: system.index,
                            body: body.index,
                        }),
                        body.centre,
                        1,
                    );
                }
                for station in &system.stations {
                    consider(self, ContactKind::Station(station.id), station.centre, 1);
                }
                for wreck in &system.derelicts {
                    consider(
                        self,
                        ContactKind::Derelict {
                            kind: wreck.kind,
                            seed: wreck.seed,
                        },
                        wreck.centre,
                        1,
                    );
                }
                for (index, comet) in system.comets.iter().enumerate() {
                    consider(
                        self,
                        ContactKind::Comet {
                            system: reference,
                            index: index as u8,
                        },
                        comet.centre,
                        1,
                    );
                }
            }
            for wreck in &sector.derelicts {
                consider(
                    self,
                    ContactKind::Derelict {
                        kind: wreck.kind,
                        seed: wreck.seed,
                    },
                    wreck.centre,
                    1,
                );
            }
            for hole in &sector.black_holes {
                consider(self, ContactKind::BlackHole(hole.id), hole.centre, 2);
            }
            for mouth in &sector.wormholes {
                consider(self, ContactKind::Wormhole(mouth.id), mouth.centre, 1);
            }
        }
        out.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        out
    }

    /// The home system.
    #[must_use]
    pub fn home_system(&self) -> StarSystem {
        self.sector(HOME_SECTOR).systems[0].clone()
    }

    /// The home world's id in space (`BodyId::Home` maps here).
    #[must_use]
    pub const fn home_world(&self) -> SpaceBodyId {
        HOME_WORLD_BODY
    }

    /// The Moon's id in space (`BodyId::Moon` maps here).
    #[must_use]
    pub const fn moon(&self) -> SpaceBodyId {
        MOON_BODY
    }

    /// Where a craft leaving the top of the home world's sky enters space: just above the
    /// Moon (see [`super::home::DEPARTURE_ABOVE_MOON`]).
    #[must_use]
    pub fn home_departure(&self) -> WorldPos {
        home_departure(&self.sector(HOME_SECTOR).systems[0]).pos()
    }

    /// The system a star chart leads to (see [`resolve_chart`]).
    #[must_use]
    pub fn chart_system(&self, chart: &StarChart) -> SystemRef {
        resolve_chart(self.seed.seed, chart)
    }
}
