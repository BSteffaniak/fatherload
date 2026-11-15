//! Cave ecosystems of [`GeneratorVersion::V6`](crate::worldgen::GeneratorVersion::V6) worlds
//! (Caves 2.0, [`crate::cave`]).
//!
//! The plane is split into [`CAVE_REGION`] cells; some cells hold one cave. A cave is built
//! from its region's RNG and the base layer only (plus the point-of-interest layer, which it
//! stays clear of), and its whole footprint stays inside its region, so a chunk consults
//! exactly the regions it overlaps and borders stay seamless.
//!
//! A cave is a small raster ([`CaveBiome`]) of [`BiomeCell`]s over its footprint:
//!
//! * `Open` / `Water`: the interior (always open; water is presentation and movement only);
//! * `Wall`: plain rock that worm tunnels and caverns of the cave layer may still open, so
//!   caves get natural entrances;
//! * `Shell(kind)`: a dressed wall tile (mycelium, crystal, obsidian, fossil rib) that also
//!   yields to the cave layer's openings;
//! * `Solid(kind)`: decor, loot and hazards that always stand (stalactites, mushrooms, caches,
//!   ore veins, ancient brick, lava pools, the bed under water).
//!
//! The interior is one connected area (smaller pieces are walled back up), and its feature's
//! centre is an open tile, so quests can bind to caves and every cave can be reached by
//! drilling (nothing in a cave is undrillable or harder than its band's drill).

use std::{collections::VecDeque, sync::Arc};

use crate::{
    cave::{CaveKind, CaveTile, cave_gem},
    terrain::{DeepStratum, TileKind, TilePosition},
    worldgen::{
        SPAWN_TILE_X,
        caves::{SPAWN_SHAFT_HALF_WIDTH, SPAWN_SHAFT_ROWS},
        features::{FeatureDetail, FeatureKind, TileRect, WorldFeature},
        noise::{NOISE_MAX, RegionRng, centred, hash2, line_noise, value_noise},
        strata::Band,
    },
};

use super::{
    base::Ctx,
    cache::MemoCache,
    index::{Layer, feature_id},
    regions::{STARTER_AREA, poi_regions_in, span},
};

/// Size of one cave region, in tiles.
pub const CAVE_REGION: (i32, i32) = (96, 64);
/// Tiles a cave's interior keeps from its region's sides (its walls fit inside the region).
const REGION_MARGIN: i32 = 4;
/// Tiles a cave's footprint keeps from any point of interest.
const POI_CLEARANCE: i32 = 3;
/// Anchors a region tries before giving up on its cave.
const PLACEMENT_TRIES: usize = 4;
/// Interior tiles a cave needs to be kept.
const MIN_INTERIOR: usize = 40;

const SALT_CAVE: u64 = 0xCA7E_B10E;
const SALT_EDGE: u64 = 0xCA7E_B1ED;
const SALT_DECOR: u64 = 0xCA7E_DEC0;
const SALT_LINE: u64 = 0xCA7E_7AB0;

/// One cell of a cave's raster.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BiomeCell {
    /// Not part of the cave (the cave layer decides).
    Outside,
    /// Open interior.
    Open,
    /// Open interior under water.
    Water,
    /// The cave's plain-rock wall (yields to cave-layer openings).
    Wall,
    /// A dressed wall tile (yields to cave-layer openings).
    Shell(TileKind),
    /// Decor, loot or hazard that always stands.
    Solid(TileKind),
}

impl BiomeCell {
    const fn is_open(self) -> bool {
        matches!(self, Self::Open | Self::Water)
    }

    const fn is_wall(self) -> bool {
        matches!(self, Self::Wall | Self::Shell(_))
    }
}

/// One generated cave.
#[derive(Debug)]
pub struct CaveBiome {
    pub kind: CaveKind,
    pub feature: WorldFeature,
    origin: TilePosition,
    width: i32,
    height: i32,
    cells: Vec<BiomeCell>,
}

impl CaveBiome {
    /// The cell at `(x, y)` (`Outside` beyond the footprint).
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> BiomeCell {
        let (lx, ly) = (x - self.origin.x, y - self.origin.y);
        if lx < 0 || ly < 0 || lx >= self.width || ly >= self.height {
            return BiomeCell::Outside;
        }
        self.cells[usize::try_from(ly * self.width + lx).unwrap_or(0)]
    }

    /// The footprint's bounds.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        self.feature.bounds
    }

    /// Every footprint cell with its position, row-major.
    pub fn cells(&self) -> impl Iterator<Item = (TilePosition, BiomeCell)> + '_ {
        let (origin, width) = (self.origin, self.width);
        self.cells.iter().enumerate().map(move |(index, cell)| {
            let index = i32::try_from(index).unwrap_or(0);
            (
                TilePosition {
                    x: origin.x + index % width,
                    y: origin.y + index / width,
                },
                *cell,
            )
        })
    }
}

/// A cave region's output.
#[derive(Debug, Default)]
pub struct CaveRegion {
    pub biome: Option<CaveBiome>,
}

static CAVES: MemoCache<CaveRegion> = MemoCache::new(2_048);

/// The cave of region `(rx, ry)`, if any (memoized). Empty on worlds without cave ecosystems.
pub fn cave_region(ctx: &Ctx, rx: i32, ry: i32) -> Arc<CaveRegion> {
    if !ctx.settings.generator.has_cave_biomes() {
        return Arc::new(CaveRegion::default());
    }
    CAVES.get_or_compute((ctx.settings, rx, ry), || CaveRegion {
        biome: build(ctx, rx, ry),
    })
}

/// The caves whose footprints overlap `rect`.
#[must_use]
pub fn caves_in(ctx: &Ctx, rect: &TileRect) -> Vec<Arc<CaveRegion>> {
    if !ctx.settings.generator.has_cave_biomes() {
        return Vec::new();
    }
    let rows = (rect.min.y.max(0), rect.max.y.min(ctx.height - 1));
    if rows.0 > rows.1 {
        return Vec::new();
    }
    let mut found = Vec::new();
    for ry in span(rows.0, rows.1, CAVE_REGION.1) {
        for rx in span(rect.min.x, rect.max.x, CAVE_REGION.0) {
            let region = cave_region(ctx, rx, ry);
            if region
                .biome
                .as_ref()
                .is_some_and(|biome| biome.bounds().intersects(rect))
            {
                found.push(region);
            }
        }
    }
    found
}

/// The cave cell at `(x, y)` and its cave, if the tile is inside a cave's footprint.
#[must_use]
pub fn cave_cell_at(ctx: &Ctx, x: i32, y: i32) -> Option<(Arc<CaveRegion>, BiomeCell)> {
    if !ctx.settings.generator.has_cave_biomes() || y < 0 || y >= ctx.height {
        return None;
    }
    let region = cave_region(
        ctx,
        x.div_euclid(CAVE_REGION.0),
        y.div_euclid(CAVE_REGION.1),
    );
    let cell = region.biome.as_ref()?.cell(x, y);
    (cell != BiomeCell::Outside).then_some((region, cell))
}

#[cfg(test)]
pub(crate) fn clear_caches() {
    CAVES.clear();
}

// ---------------------------------------------------------------------------------------------
// Choosing a cave

/// Chance (per mille) that a region in `band` holds a cave.
const fn cave_chance(band: Band) -> u32 {
    match band {
        Band::Topsoil => 0,
        Band::Clay => 420,
        Band::Stone | Band::Slate => 560,
        Band::Basalt | Band::Deep(_) => 600,
    }
}

/// Relative weights of each kind ([`CaveKind::ALL`] order) in `band`. Kinds whose tiles are
/// harder than the band's drill get no weight (geodes and ruins from the grey stone, lava tubes
/// from the basalt).
const fn kind_weights(band: Band) -> [u32; 9] {
    // GrandCavern, CrystalGeode, FloodedCave, LavaTube, FungalGrotto, WormBurrows,
    // AncientRuins, Chasm, CaveNetwork
    match band {
        Band::Topsoil => [0; 9],
        Band::Clay => [12, 0, 18, 0, 30, 28, 0, 0, 26],
        Band::Stone => [20, 16, 16, 0, 16, 10, 8, 10, 14],
        Band::Slate => [20, 18, 12, 0, 10, 8, 12, 16, 12],
        Band::Basalt => [18, 14, 6, 26, 4, 7, 8, 15, 10],
        Band::Deep(stratum) => match stratum {
            DeepStratum::CrystalFaults => [12, 45, 6, 6, 4, 4, 6, 10, 7],
            DeepStratum::FossilOceans => [12, 8, 38, 0, 6, 18, 6, 4, 8],
            DeepStratum::PressureCathedrals => [36, 8, 8, 6, 4, 4, 6, 22, 6],
            DeepStratum::RadioactiveHollow => [10, 8, 8, 8, 40, 8, 6, 6, 6],
            DeepStratum::AncientMachineLayer => [10, 6, 6, 8, 4, 6, 44, 8, 8],
            DeepStratum::VoidGeodeFields => [12, 44, 6, 6, 6, 6, 6, 8, 6],
            DeepStratum::MantleStormZone => [10, 8, 0, 50, 0, 6, 8, 12, 6],
        },
    }
}

fn build(ctx: &Ctx, rx: i32, ry: i32) -> Option<CaveBiome> {
    let region = TileRect {
        min: TilePosition {
            x: rx.checked_mul(CAVE_REGION.0)?,
            y: ry.checked_mul(CAVE_REGION.1)?,
        },
        max: TilePosition {
            x: rx * CAVE_REGION.0 + CAVE_REGION.0 - 1,
            y: ry * CAVE_REGION.1 + CAVE_REGION.1 - 1,
        },
    };
    if region.max.y < 0 || region.min.y >= ctx.height {
        return None;
    }
    let mut rng = RegionRng::new(ctx.seed(), SALT_CAVE, rx, ry);
    let probe = TilePosition {
        x: region.min.x + CAVE_REGION.0 / 2,
        y: (region.min.y + CAVE_REGION.1 / 2).clamp(0, ctx.height - 1),
    };
    if !rng.chance(cave_chance(ctx.band(probe.x, probe.y))) {
        return None;
    }
    // Stay clear of spawn's shaft.
    let spawn = TileRect {
        min: TilePosition {
            x: SPAWN_TILE_X - SPAWN_SHAFT_HALF_WIDTH - 6,
            y: 0,
        },
        max: TilePosition {
            x: SPAWN_TILE_X + SPAWN_SHAFT_HALF_WIDTH + 6,
            y: SPAWN_SHAFT_ROWS + 8,
        },
    };
    if region.intersects(&spawn) {
        return None;
    }
    // A few tries: a cave that would crowd a point of interest moves elsewhere in its region.
    for _ in 0..PLACEMENT_TRIES {
        let anchor = TilePosition {
            x: region.min.x + rng.range(24, CAVE_REGION.0 - 25),
            y: region.min.y + rng.range(16, CAVE_REGION.1 - 17),
        };
        if anchor.y >= ctx.floor_row(anchor.x) - 12 || anchor.y < 0 {
            continue;
        }
        let band = ctx.band(anchor.x, anchor.y);
        let Some(pick) = rng.pick(&kind_weights(band)) else {
            continue;
        };
        let kind = CaveKind::ALL[pick];
        let mut canvas = Canvas::new(ctx, region, band, anchor);
        match kind {
            CaveKind::GrandCavern => grand_cavern(&mut canvas, &mut rng),
            CaveKind::CrystalGeode => crystal_geode(&mut canvas, &mut rng),
            CaveKind::FloodedCave => flooded_cave(&mut canvas, &mut rng),
            CaveKind::LavaTube => lava_tube(&mut canvas, &mut rng),
            CaveKind::FungalGrotto => fungal_grotto(&mut canvas, &mut rng),
            CaveKind::WormBurrows => worm_burrows(&mut canvas, &mut rng),
            CaveKind::AncientRuins => ancient_ruins(&mut canvas, &mut rng),
            CaveKind::Chasm => chasm(&mut canvas, &mut rng),
            CaveKind::CaveNetwork => cave_network(&mut canvas, &mut rng),
        }
        if let Some(biome) = canvas.finish(kind, rx, ry) {
            return Some(biome);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// The canvas a cave is drawn on (its whole region)

struct Canvas<'c> {
    ctx: &'c Ctx,
    region: TileRect,
    band: Band,
    anchor: TilePosition,
    cells: Vec<BiomeCell>,
}

impl<'c> Canvas<'c> {
    fn new(ctx: &'c Ctx, region: TileRect, band: Band, anchor: TilePosition) -> Self {
        Self {
            ctx,
            region,
            band,
            anchor,
            cells: vec![
                BiomeCell::Outside;
                usize::try_from(CAVE_REGION.0 * CAVE_REGION.1).unwrap_or(0)
            ],
        }
    }

    const fn seed(&self) -> u64 {
        self.ctx.seed()
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        self.region
            .contains(TilePosition { x, y })
            .then(|| {
                usize::try_from((y - self.region.min.y) * CAVE_REGION.0 + (x - self.region.min.x))
                    .ok()
            })
            .flatten()
    }

    fn get(&self, x: i32, y: i32) -> BiomeCell {
        self.index(x, y)
            .map_or(BiomeCell::Outside, |index| self.cells[index])
    }

    fn set(&mut self, x: i32, y: i32, cell: BiomeCell) {
        if let Some(index) = self.index(x, y) {
            self.cells[index] = cell;
        }
    }

    /// Whether the cave may own `(x, y)` at all: base rock below the crust, clear of the town,
    /// the spawn shaft and the bedrock floor, inside the region.
    fn ownable(&self, x: i32, y: i32) -> bool {
        self.region.contains(TilePosition { x, y })
            && y >= 0
            && y < self.ctx.height
            && self.ctx.fixed_kind(x, y).is_none()
            && y < self.ctx.floor_row(x) - 2
    }

    /// Whether the interior may open at `(x, y)` (room left for walls inside the region).
    fn openable(&self, x: i32, y: i32) -> bool {
        let inner = self.region.expanded(-REGION_MARGIN);
        inner.contains(TilePosition { x, y })
            && self.ownable(x, y)
            && self.ctx.cave_allowed(x, y)
            && !STARTER_AREA.expanded(3).contains(TilePosition { x, y })
    }

    fn open(&mut self, x: i32, y: i32) {
        if self.openable(x, y) {
            self.set(x, y, BiomeCell::Open);
        }
    }

    fn is_open(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_open()
    }

    /// Opens a ragged ellipse.
    fn ellipse(&mut self, cx: i32, cy: i32, hw: i32, hh: i32) {
        let (wide, tall) = (i64::from(hw.max(1)), i64::from(hh.max(1)));
        for dy in -hh - 2..=hh + 2 {
            for dx in -hw - 2..=hw + 2 {
                let (x, y) = (cx + dx, cy + dy);
                let d = i64::from(dx * dx) * tall * tall + i64::from(dy * dy) * wide * wide;
                let r2 = wide * wide * tall * tall;
                let edge = i64::from(value_noise(self.seed(), SALT_EDGE, x, y, 3, 3));
                if d <= r2 * (52_000 + edge / 4) / 65_536 {
                    self.open(x, y);
                }
            }
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, radius: i32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius + radius / 2 {
                    self.open(cx + dx, cy + dy);
                }
            }
        }
    }

    /// Carves a jittered tunnel from `from` to `to`.
    fn tunnel(&mut self, rng: &mut RegionRng, from: TilePosition, to: TilePosition, radius: i32) {
        let (mut x, mut y) = (from.x, from.y);
        let mut guard = 0;
        while (x, y) != (to.x, to.y) && guard < 400 {
            guard += 1;
            self.disc(x, y, radius);
            let (dx, dy) = (to.x - x, to.y - y);
            if rng.chance(220) {
                // A sideways wobble.
                if dx.abs() >= dy.abs() {
                    y += if rng.chance(500) { 1 } else { -1 };
                } else {
                    x += if rng.chance(500) { 1 } else { -1 };
                }
            } else if dx.abs() > dy.abs() || (dx.abs() == dy.abs() && rng.chance(500)) {
                x += dx.signum();
            } else {
                y += dy.signum();
            }
        }
        self.disc(to.x, to.y, radius);
    }

    /// Walls every ownable outside cell within `thickness` (Chebyshev) of the interior.
    fn wall(&mut self, thickness: i32) {
        let snapshot = self.cells.clone();
        let open_at = |x: i32, y: i32| {
            self.index(x, y)
                .is_some_and(|index| snapshot[index].is_open())
        };
        let mut walls = Vec::new();
        for y in self.region.min.y..=self.region.max.y {
            for x in self.region.min.x..=self.region.max.x {
                if self.get(x, y) != BiomeCell::Outside || !self.ownable(x, y) {
                    continue;
                }
                let near = (-thickness..=thickness)
                    .any(|dy| (-thickness..=thickness).any(|dx| open_at(x + dx, y + dy)));
                if near {
                    walls.push((x, y));
                }
            }
        }
        for (x, y) in walls {
            self.set(x, y, BiomeCell::Wall);
        }
    }

    /// Distance (Chebyshev, up to `limit`) from wall cell `(x, y)` to the interior.
    fn depth_into_wall(&self, x: i32, y: i32, limit: i32) -> i32 {
        for distance in 1..=limit {
            for dy in -distance..=distance {
                for dx in -distance..=distance {
                    if (dx.abs() == distance || dy.abs() == distance)
                        && self.is_open(x + dx, y + dy)
                    {
                        return distance;
                    }
                }
            }
        }
        limit + 1
    }

    /// Wall cells that touch the interior (4-neighbour), in row-major order.
    fn inner_walls(&self) -> Vec<(i32, i32)> {
        self.positions()
            .filter(|&(x, y)| {
                self.get(x, y).is_wall()
                    && [(0, 1), (0, -1), (1, 0), (-1, 0)]
                        .iter()
                        .any(|(dx, dy)| self.is_open(x + dx, y + dy))
            })
            .collect()
    }

    /// Open cells with a wall (or decor) right below.
    fn floors(&self) -> Vec<(i32, i32)> {
        self.positions()
            .filter(|&(x, y)| self.is_open(x, y) && self.solid_below(x, y))
            .collect()
    }

    /// Open cells with plain wall right above (where stalactites hang).
    fn ceilings(&self) -> Vec<(i32, i32)> {
        self.positions()
            .filter(|&(x, y)| {
                self.get(x, y) == BiomeCell::Open
                    && self.get(x, y - 1) == BiomeCell::Wall
                    && self.is_open(x, y + 1)
                    && self.is_open(x, y + 2)
            })
            .collect()
    }

    fn solid_below(&self, x: i32, y: i32) -> bool {
        matches!(
            self.get(x, y + 1),
            BiomeCell::Wall | BiomeCell::Shell(_) | BiomeCell::Solid(_)
        )
    }

    fn positions(&self) -> impl Iterator<Item = (i32, i32)> + use<> {
        let region = self.region;
        (region.min.y..=region.max.y)
            .flat_map(move |y| (region.min.x..=region.max.x).map(move |x| (x, y)))
    }

    fn decor_roll(&self, x: i32, y: i32, salt: u64, per_mille: u32) -> bool {
        hash2(self.seed(), SALT_DECOR ^ salt, x, y) % 1_000 < u64::from(per_mille)
    }

    /// Hangs stalactites from `per_mille` of the plain ceiling cells.
    fn stalactites(&mut self, per_mille: u32) {
        for (x, y) in self.ceilings() {
            if self.decor_roll(x, y, 1, per_mille) && !self.get(x - 1, y).is_wall() {
                self.set(x, y, BiomeCell::Solid(TileKind::Cave(CaveTile::Stalactite)));
            }
        }
    }

    /// Puts `tile` on a floor cell (the one `pick` chooses among them).
    fn cache(&mut self, rng: &mut RegionRng, tile: CaveTile, under_water: bool) {
        let floors: Vec<(i32, i32)> = self
            .floors()
            .into_iter()
            .filter(|&(x, y)| (self.get(x, y) == BiomeCell::Water) == under_water)
            .filter(|&(x, y)| self.is_open(x, y - 1))
            .collect();
        if floors.is_empty() {
            return;
        }
        let pick = rng.range(0, i32::try_from(floors.len()).unwrap_or(1) - 1);
        let (x, y) = floors[usize::try_from(pick).unwrap_or(0)];
        self.set(x, y, BiomeCell::Solid(TileKind::Cave(tile)));
    }

    /// Grows a vein of `count` tiles of the band's gem into the walls, from an inner wall cell.
    fn ore_vein(&mut self, rng: &mut RegionRng, count: usize) {
        let walls = self.inner_walls();
        if walls.is_empty() {
            return;
        }
        let pick = rng.range(0, i32::try_from(walls.len()).unwrap_or(1) - 1);
        let start = walls[usize::try_from(pick).unwrap_or(0)];
        let ore = BiomeCell::Solid(TileKind::Ore(cave_gem(self.band)));
        let mut queue = VecDeque::from([start]);
        let mut placed = 0;
        while let Some((x, y)) = queue.pop_front() {
            if placed >= count || !self.get(x, y).is_wall() {
                continue;
            }
            self.set(x, y, ore);
            placed += 1;
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                if rng.chance(700) {
                    queue.push_back((x + dx, y + dy));
                }
            }
        }
    }

    /// Fills a small blob of `kind` deep in the walls (spore gas pockets).
    fn pocket(&mut self, rng: &mut RegionRng, kind: TileKind, count: usize) {
        let candidates: Vec<(i32, i32)> = self
            .positions()
            .filter(|&(x, y)| {
                self.get(x, y) == BiomeCell::Wall && self.depth_into_wall(x, y, 2) == 2
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let pick = rng.range(0, i32::try_from(candidates.len()).unwrap_or(1) - 1);
        let (sx, sy) = candidates[usize::try_from(pick).unwrap_or(0)];
        let mut placed = 0;
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1), (1, 1)] {
            if placed >= count {
                break;
            }
            if self.get(sx + dx, sy + dy) == BiomeCell::Wall
                && self.depth_into_wall(sx + dx, sy + dy, 1) > 1
            {
                self.set(sx + dx, sy + dy, BiomeCell::Solid(kind));
                placed += 1;
            }
        }
    }

    /// Keeps only the biggest 4-connected open area (smaller ones become wall again), and
    /// returns its cells.
    fn keep_largest_area(&mut self) -> Vec<(i32, i32)> {
        let mut seen = vec![false; self.cells.len()];
        let mut best: Vec<(i32, i32)> = Vec::new();
        let mut others: Vec<(i32, i32)> = Vec::new();
        for (x, y) in self.positions() {
            let Some(index) = self.index(x, y) else {
                continue;
            };
            if seen[index] || !self.cells[index].is_open() {
                continue;
            }
            let mut area = Vec::new();
            let mut queue = VecDeque::from([(x, y)]);
            seen[index] = true;
            while let Some((cx, cy)) = queue.pop_front() {
                area.push((cx, cy));
                for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if let Some(next) = self.index(nx, ny)
                        && !seen[next]
                        && self.cells[next].is_open()
                    {
                        seen[next] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }
            if area.len() > best.len() {
                others.append(&mut best);
                best = area;
            } else {
                others.extend(area);
            }
        }
        for (x, y) in others {
            self.set(x, y, BiomeCell::Wall);
        }
        best
    }

    /// Closes the raster, indexes the cave and crops it to its footprint.
    fn finish(mut self, kind: CaveKind, rx: i32, ry: i32) -> Option<CaveBiome> {
        let interior = self.keep_largest_area();
        if interior.len() < MIN_INTERIOR {
            return None;
        }
        let mut bounds: Option<TileRect> = None;
        for (x, y) in self.positions() {
            if self.get(x, y) == BiomeCell::Outside {
                continue;
            }
            let here = TilePosition { x, y };
            bounds = Some(bounds.map_or(
                TileRect {
                    min: here,
                    max: here,
                },
                |b| TileRect {
                    min: TilePosition {
                        x: b.min.x.min(x),
                        y: b.min.y.min(y),
                    },
                    max: TilePosition {
                        x: b.max.x.max(x),
                        y: b.max.y.max(y),
                    },
                },
            ));
        }
        let bounds = bounds?;
        let near_poi = poi_regions_in(self.ctx, &bounds.expanded(POI_CLEARANCE + 8))
            .iter()
            .filter_map(|region| region.poi.as_ref())
            .any(|poi| {
                poi.feature
                    .bounds
                    .expanded(POI_CLEARANCE)
                    .intersects(&bounds)
            });
        if near_poi {
            return None;
        }
        let anchor = self.anchor;
        let center = interior
            .iter()
            .copied()
            .min_by_key(|&(x, y)| ((x - anchor.x).pow(2) + (y - anchor.y).pow(2), y, x))
            .map(|(x, y)| TilePosition { x, y })?;
        let (width, height) = (bounds.width(), bounds.height());
        let mut cells = Vec::with_capacity(usize::try_from(width * height).unwrap_or(0));
        for y in bounds.min.y..=bounds.max.y {
            for x in bounds.min.x..=bounds.max.x {
                cells.push(self.get(x, y));
            }
        }
        let feature = WorldFeature {
            id: feature_id(Layer::Cave, rx, ry, 0),
            kind: FeatureKind::Cave,
            detail: FeatureDetail::Cave(kind),
            band: self.ctx.band(center.x, center.y),
            center,
            bounds,
            size: u32::try_from(interior.len()).unwrap_or(u32::MAX),
        };
        Some(CaveBiome {
            kind,
            feature,
            origin: bounds.min,
            width,
            height,
            cells,
        })
    }

    /// Turns every wall cell within `depth` of the interior for which `pick` answers into
    /// that cell.
    fn dress(
        &mut self,
        depth: i32,
        mut pick: impl FnMut(&Self, i32, i32, i32) -> Option<BiomeCell>,
    ) {
        let mut changes = Vec::new();
        for (x, y) in self.positions() {
            if self.get(x, y) != BiomeCell::Wall {
                continue;
            }
            let distance = self.depth_into_wall(x, y, depth);
            if distance <= depth
                && let Some(cell) = pick(self, x, y, distance)
            {
                changes.push((x, y, cell));
            }
        }
        for (x, y, cell) in changes {
            self.set(x, y, cell);
        }
    }

    /// The plain rock at `(x, y)` (for walls that must always stand).
    fn rock(&self, x: i32, y: i32) -> TileKind {
        self.ctx.rock_kind(x, y, self.ctx.band(x, y))
    }
}

/// The band allows tiles of `hardness`.
const fn fits(band: Band, hardness: u8) -> bool {
    hardness <= band.required_drill_strength()
}

// ---------------------------------------------------------------------------------------------
// The kinds

/// A huge echo hall: a flat-floored ellipse, stalagmites, stalactites, a cache and a gem vein.
fn grand_cavern(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (cx, cy) = (canvas.anchor.x, canvas.anchor.y);
    let (hw, hh) = (rng.range(14, 20), rng.range(7, 10));
    canvas.ellipse(cx, cy, hw, hh);
    // A flatter floor: the lowest rows close up.
    let floor = cy + hh * 2 / 3;
    for (x, y) in canvas.positions() {
        if y > floor && canvas.is_open(x, y) {
            canvas.set(x, y, BiomeCell::Outside);
        }
    }
    canvas.wall(2);
    // Stalagmites rising from the floor.
    for (x, y) in canvas.floors() {
        if canvas.decor_roll(x, y, 2, 140) {
            let height = 1 + i32::try_from(hash2(canvas.seed(), SALT_DECOR, x, y) % 3).unwrap_or(0);
            for step in 0..height {
                if canvas.is_open(x, y - step) && canvas.is_open(x, y - step - 1) {
                    canvas.set(x, y - step, BiomeCell::Wall);
                }
            }
        }
    }
    canvas.stalactites(200);
    if rng.chance(800) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
    if rng.chance(700) {
        let count = usize::try_from(rng.range(5, 8)).unwrap_or(5);
        canvas.ore_vein(rng, count);
    }
}

/// A round hollow lined with glowing crystal, spikes reaching in, gems in the shell.
fn crystal_geode(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (cx, cy) = (canvas.anchor.x, canvas.anchor.y);
    let radius = rng.range(5, 7);
    canvas.ellipse(cx, cy, radius + 1, radius);
    canvas.wall(2);
    let crystal = BiomeCell::Shell(TileKind::Cave(CaveTile::Crystal));
    canvas.dress(1, |_, _, _, _| Some(crystal));
    // Spikes growing inward.
    let mut spikes = Vec::new();
    for (x, y) in canvas.positions() {
        if !canvas.is_open(x, y) {
            continue;
        }
        let touches = [(0, 1), (0, -1), (1, 0), (-1, 0)]
            .iter()
            .any(|(dx, dy)| canvas.get(x + dx, y + dy) == crystal);
        if touches && canvas.decor_roll(x, y, 3, 260) {
            spikes.push((x, y));
        }
    }
    for (x, y) in spikes {
        canvas.set(x, y, BiomeCell::Solid(TileKind::Cave(CaveTile::Crystal)));
    }
    // Gems embedded in the outer shell.
    let gem = BiomeCell::Solid(TileKind::Ore(cave_gem(canvas.band)));
    let budget = rng.range(5, 9);
    let mut placed = 0;
    for (x, y) in canvas.positions() {
        if placed >= budget {
            break;
        }
        if canvas.get(x, y) == BiomeCell::Wall
            && canvas.depth_into_wall(x, y, 2) == 2
            && canvas.decor_roll(x, y, 4, 120)
        {
            canvas.set(x, y, gem);
            placed += 1;
        }
    }
    if rng.chance(350) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
}

/// An underground lake: a wide hollow mostly under water, a silt bed, glowing algae, a sunken
/// cache.
fn flooded_cave(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (cx, cy) = (canvas.anchor.x, canvas.anchor.y);
    let (hw, hh) = (rng.range(10, 15), rng.range(5, 7));
    canvas.ellipse(cx, cy, hw, hh);
    canvas.wall(2);
    let water_line = cy - hh / 3;
    for (x, y) in canvas.positions() {
        if y >= water_line && canvas.get(x, y) == BiomeCell::Open {
            canvas.set(x, y, BiomeCell::Water);
        }
    }
    // Below the water line the walls always stand (no tunnel drains the lake), the bed is silt
    // and glowing algae grows along it.
    canvas.dress(2, |canvas, x, y, distance| {
        if y < water_line {
            return None;
        }
        let beside_water = [(0, 1), (0, -1), (1, 0), (-1, 0)]
            .iter()
            .any(|(dx, dy)| canvas.get(x + dx, y + dy) == BiomeCell::Water);
        if distance == 1 && beside_water && canvas.get(x, y - 1) == BiomeCell::Water {
            if canvas.decor_roll(x, y, 5, 220) {
                return Some(BiomeCell::Solid(TileKind::Cave(CaveTile::GlowAlgae)));
            }
            return Some(BiomeCell::Solid(TileKind::Cave(CaveTile::Silt)));
        }
        Some(BiomeCell::Solid(canvas.rock(x, y)))
    });
    canvas.stalactites(140);
    if rng.chance(750) {
        canvas.cache(rng, CaveTile::TreasureCache, true);
    }
    if rng.chance(400) {
        let count = usize::try_from(rng.range(4, 6)).unwrap_or(4);
        canvas.ore_vein(rng, count);
    }
}

/// A long meandering tube lined with obsidian, lava pooling along its floor.
fn lava_tube(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let length = rng.range(52, 80);
    let start = (canvas.region.min.x + CAVE_REGION.0 / 2 - length / 2).max(canvas.region.min.x + 6);
    let cy = canvas.anchor.y;
    let seed = canvas.seed();
    for x in start..start + length {
        let y = cy + centred(line_noise(seed, SALT_LINE, x, 19), 3);
        let radius = 2 + i32::from(line_noise(seed, SALT_LINE + 1, x, 9) > NOISE_MAX * 3 / 4);
        canvas.disc(x, y, radius);
    }
    canvas.wall(1);
    // The lining always stands, so the tube keeps its shape and its lava its floor.
    let obsidian = BiomeCell::Solid(TileKind::Cave(CaveTile::Obsidian));
    canvas.dress(1, |_, _, _, _| Some(obsidian));
    for (x, y) in canvas.floors() {
        if line_noise(seed, SALT_LINE + 2, x, 6) > NOISE_MAX * 11 / 20 {
            canvas.set(x, y, BiomeCell::Solid(TileKind::Lava));
        }
    }
    if rng.chance(550) {
        let count = usize::try_from(rng.range(4, 7)).unwrap_or(4);
        canvas.ore_vein(rng, count);
    }
    if rng.chance(300) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
}

/// A glowing mushroom cavern: mycelium floors, giant mushrooms with bouncy caps, glowcaps and
/// spore pockets.
fn fungal_grotto(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (cx, cy) = (canvas.anchor.x, canvas.anchor.y);
    let (hw, hh) = (rng.range(10, 14), rng.range(6, 8));
    canvas.ellipse(cx, cy, hw, hh);
    canvas.wall(2);
    let mycelium = BiomeCell::Shell(TileKind::Cave(CaveTile::Mycelium));
    canvas.dress(1, |canvas, x, y, _| {
        let floor = canvas.is_open(x, y - 1);
        (floor || canvas.decor_roll(x, y, 6, 300)).then_some(mycelium)
    });
    // Giant mushrooms, spaced along the floor.
    let mut floors = canvas.floors();
    floors.sort_by_key(|&(x, y)| (hash2(canvas.seed(), SALT_DECOR ^ 7, x, y), x, y));
    let wanted = rng.range(2, 4);
    let mut stems: Vec<i32> = Vec::new();
    for (x, y) in floors {
        if i32::try_from(stems.len()).unwrap_or(0) >= wanted {
            break;
        }
        if stems.iter().any(|stem| (stem - x).abs() < 9) {
            continue;
        }
        let height = rng.range(3, 5);
        if !(0..=height + 1).all(|step| canvas.is_open(x, y - step)) {
            continue;
        }
        for step in 0..height {
            canvas.set(
                x,
                y - step,
                BiomeCell::Solid(TileKind::Cave(CaveTile::MushroomStem)),
            );
        }
        let cap_row = y - height;
        let reach = rng.range(2, 3);
        for dx in -reach..=reach {
            if canvas.is_open(x + dx, cap_row) {
                canvas.set(
                    x + dx,
                    cap_row,
                    BiomeCell::Solid(TileKind::Cave(CaveTile::MushroomCap)),
                );
            }
        }
        for dx in [-reach - 1, reach + 1] {
            if canvas.is_open(x + dx, cap_row + 1) && canvas.is_open(x + dx, cap_row + 2) {
                canvas.set(
                    x + dx,
                    cap_row + 1,
                    BiomeCell::Solid(TileKind::Cave(CaveTile::MushroomCap)),
                );
            }
        }
        stems.push(x);
    }
    for (x, y) in canvas.floors() {
        if canvas.get(x, y) == BiomeCell::Open
            && canvas.is_open(x, y - 1)
            && canvas.decor_roll(x, y, 8, 300)
        {
            canvas.set(x, y, BiomeCell::Solid(TileKind::Cave(CaveTile::GlowCap)));
        }
    }
    let pockets = rng.range(1, 2);
    for _ in 0..pockets {
        canvas.pocket(rng, TileKind::Gas, 4);
    }
    if rng.chance(400) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
}

/// A giant worm's tunnels: a bulb where they meet, ribbed with old bone.
fn worm_burrows(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (cx, cy) = (canvas.anchor.x, canvas.anchor.y);
    canvas.disc(cx, cy, 3);
    let mut rib_rings = Vec::new();
    let tunnels = rng.range(2, 3);
    for tunnel in 0..tunnels {
        let mut angle = rng.range(0, 15) + tunnel * 5;
        let (mut x, mut y) = (i64::from(cx) * 256, i64::from(cy) * 256);
        let length = rng.range(30, 48);
        for step in 0..length {
            let (tx, ty) = (
                i32::try_from(x >> 8).unwrap_or(cx),
                i32::try_from(y >> 8).unwrap_or(cy),
            );
            let radius = if step % 9 < 6 { 2 } else { 1 };
            canvas.disc(tx, ty, radius);
            if step % 5 == 2 {
                rib_rings.push((tx, ty, radius + 1));
            }
            if rng.chance(260) {
                angle += if rng.chance(500) { 1 } else { -1 };
            }
            let (dx, dy) = crate::worldgen::caves::HEADINGS
                [usize::try_from(angle.rem_euclid(16)).unwrap_or(0)];
            // Worms prefer to wander sideways.
            x += i64::from(dx);
            y += i64::from(dy) * 2 / 3;
        }
    }
    canvas.wall(2);
    if fits(canvas.band, CaveTile::FossilRib.hardness()) {
        let rib = BiomeCell::Shell(TileKind::Cave(CaveTile::FossilRib));
        for (rx, ry, radius) in rib_rings {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let (x, y) = (rx + dx, ry + dy);
                    if canvas.get(x, y) == BiomeCell::Wall && canvas.depth_into_wall(x, y, 1) == 1 {
                        canvas.set(x, y, rib);
                    }
                }
            }
        }
    }
    if rng.chance(500) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
}

/// A buried hall of brick: pillars (some broken), glyphs, rubble, a relic cache in the middle
/// and a crumbling corridor out.
fn ancient_ruins(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let (width, height) = (rng.range(14, 20), rng.range(5, 7));
    let (left, top) = (canvas.anchor.x - width / 2, canvas.anchor.y - height / 2);
    let (right, bottom) = (left + width - 1, top + height - 1);
    for y in top..=bottom {
        for x in left..=right {
            canvas.open(x, y);
        }
    }
    // A corridor out of one end, at floor level.
    let east = rng.chance(500);
    let door_x = if east { right + 1 } else { left - 1 };
    let reach = rng.range(8, 14);
    let end = TilePosition {
        x: if east { door_x + reach } else { door_x - reach },
        y: bottom - 1 + rng.range(-4, 2),
    };
    canvas.open(door_x, bottom);
    canvas.open(door_x, bottom - 1);
    canvas.tunnel(
        rng,
        TilePosition {
            x: door_x,
            y: bottom - 1,
        },
        end,
        1,
    );
    canvas.wall(2);
    // The hall's brick: walls, floor and vault, one tile thick, with glyphs and crumbling.
    for y in top - 1..=bottom + 1 {
        for x in left - 1..=right + 1 {
            let edge = x == left - 1 || x == right + 1 || y == top - 1 || y == bottom + 1;
            if !edge || canvas.get(x, y).is_open() || canvas.get(x, y) == BiomeCell::Outside {
                continue;
            }
            let cell = if canvas.decor_roll(x, y, 9, 110) {
                BiomeCell::Wall
            } else if canvas.decor_roll(x, y, 10, 160) {
                BiomeCell::Solid(TileKind::Cave(CaveTile::RuinGlyph))
            } else {
                BiomeCell::Solid(TileKind::Cave(CaveTile::RuinBrick))
            };
            canvas.set(x, y, cell);
        }
    }
    // Pillars every few columns rising from the floor (rigs fly over them), some broken to
    // stumps.
    let spacing = rng.range(4, 5);
    let mut x = left + 2;
    while x <= right - 2 {
        if (x - canvas.anchor.x).abs() > 1 {
            let rise = if rng.chance(350) {
                rng.range(1, 2)
            } else {
                height - 2
            };
            for y in bottom - rise + 1..=bottom {
                canvas.set(x, y, BiomeCell::Solid(TileKind::Cave(CaveTile::RuinPillar)));
            }
        }
        x += spacing;
    }
    // Rubble along the floor.
    for x in left..=right {
        if canvas.get(x, bottom) == BiomeCell::Open && canvas.decor_roll(x, bottom, 11, 120) {
            canvas.set(x, bottom, BiomeCell::Wall);
        }
    }
    canvas.set(
        canvas.anchor.x,
        bottom,
        BiomeCell::Solid(TileKind::Cave(CaveTile::RelicCache)),
    );
}

/// A tall narrow crack: wandering walls, ledges, stalactites, a small chamber at the bottom.
fn chasm(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let seed = canvas.seed();
    let height = rng.range(34, 50);
    let top = (canvas.region.min.y + CAVE_REGION.1 / 2 - height / 2).max(canvas.region.min.y + 5);
    let cx = canvas.anchor.x;
    for y in top..top + height {
        let center = cx + centred(line_noise(seed, SALT_LINE + 3, y, 11), 5);
        let taper = (y - top).min(top + height - 1 - y);
        let half = (1 + i32::from(line_noise(seed, SALT_LINE + 4, y, 5) > NOISE_MAX / 2))
            .min(taper / 2 + 1)
            .max(1);
        for x in center - half..=center + half {
            canvas.open(x, y);
        }
    }
    let bottom = top + height;
    let floor_x = cx + centred(line_noise(seed, SALT_LINE + 3, bottom, 11), 5);
    canvas.ellipse(floor_x, bottom - 1, 5, 2);
    canvas.wall(2);
    // Ledges jutting out from alternating sides.
    let mut y = top + rng.range(6, 9);
    let mut east = rng.chance(500);
    while y < bottom - 6 {
        let open: Vec<i32> = (canvas.region.min.x..=canvas.region.max.x)
            .filter(|&x| canvas.is_open(x, y))
            .collect();
        if open.len() >= 3 {
            let take = if east {
                open.last().copied()
            } else {
                open.first().copied()
            };
            if let Some(x) = take {
                canvas.set(x, y, BiomeCell::Wall);
            }
        }
        east = !east;
        y += rng.range(6, 9);
    }
    canvas.stalactites(260);
    if rng.chance(600) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
    if rng.chance(600) {
        let count = usize::try_from(rng.range(4, 7)).unwrap_or(4);
        canvas.ore_vein(rng, count);
    }
}

/// Small chambers strung together by narrow tunnels.
fn cave_network(canvas: &mut Canvas<'_>, rng: &mut RegionRng) {
    let count = rng.range(3, 5);
    let mut rooms = vec![canvas.anchor];
    for _ in 1..count {
        rooms.push(TilePosition {
            x: canvas.region.min.x + rng.range(14, CAVE_REGION.0 - 15),
            y: canvas.region.min.y + rng.range(10, CAVE_REGION.1 - 11),
        });
    }
    // Chain them nearest-first from the anchor so tunnels stay short.
    let mut ordered = vec![rooms[0]];
    let mut rest: Vec<TilePosition> = rooms[1..].to_vec();
    while !rest.is_empty() {
        let last = *ordered.last().unwrap_or(&rooms[0]);
        let (index, _) = rest
            .iter()
            .enumerate()
            .min_by_key(|(_, room)| ((room.x - last.x).abs() + (room.y - last.y).abs(), room.x))
            .unwrap_or((0, &last));
        ordered.push(rest.remove(index));
    }
    for room in &ordered {
        let radius = rng.range(3, 5);
        canvas.ellipse(room.x, room.y, radius + 1, radius);
    }
    for pair in ordered.windows(2) {
        canvas.tunnel(rng, pair[0], pair[1], 1);
    }
    canvas.wall(2);
    canvas.stalactites(120);
    if rng.chance(350) {
        canvas.cache(rng, CaveTile::TreasureCache, false);
    }
    if rng.chance(500) {
        let count = usize::try_from(rng.range(3, 5)).unwrap_or(3);
        canvas.ore_vein(rng, count);
    }
}
