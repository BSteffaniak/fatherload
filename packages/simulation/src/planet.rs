//! Planet ground (expansion plan §15 F12, Wave 11 polar track): the ground of a round body,
//! generated on its unrolled, wrapping strip.
//!
//! A planet is played on its **unrolled strip** ([`crate::polar`]): a flat world of
//! [`PolarGrid::surface_columns`] columns that wraps round at its ends, rows counting down from
//! the datum surface to the **core chamber**. That is the same kind of world as the home
//! planet, so a planet's terrain is an ordinary streamed [`crate::terrain::Terrain`]
//! ([`crate::terrain::Terrain::planet`]) and digging, caves, fluids, drops, rigs, pilots and
//! creatures run on it unchanged; only gravity's strength ([`PlanetParams::gravity`]),
//! landing ([`landing`]), the camera, rendering and the crossing to space use the polar
//! mapping.
//!
//! * **Generation** ([`PlanetGround::generated_tile`]) is a pure function of the
//!   [`PlanetParams`] (seed, radius, gravity, theme) and the strip tile: hills from periodic
//!   value noise (they wrap seamlessly), the theme's crust and two or three concentric strata
//!   with ragged boundaries, accent rock and ores, a few caves, and a ring-shaped core chamber
//!   above an indestructible core.
//! * **Rings halve toward the core**: below each halving row a tile spans `2^k` strip columns
//!   ([`PolarGrid::span_at`]). Generation is per ring tile (every strip cell of one tile reads
//!   the same), and the terrain keeps them together when ground is dug or built
//!   ([`crate::terrain::Terrain`] mirrors a solid write to the tile's other cells), so a ring
//!   of an inner band really has half the tiles.
//! * **Themes** come from the append-only registry in [`theme`]; Waves 12-13 add real ones.
//!
//! **Wired (Wave 11 integration):** a generated body's store (the regions track's
//! `place::BodyWorld`) is built with [`crate::terrain::Terrain::planet`] from
//! `place::planet_params_for` (the space generator's body: its seed, radius, gravity and
//! theme), its gravity zone the store's own field zone. [`BodyGround`] is the strip's query
//! interface. [`crate::WorldState::new_planet_world`] (a world that *is* one planet) stays for
//! the polar track's own tests and scenes.

pub mod hooks;
pub mod landing;
#[cfg(test)]
mod tests;
pub mod theme;
pub mod wilds;
pub mod world;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use serde::{Deserialize, Serialize};

pub use hooks::{PlanetThemeHooks, SecretSite, hooks};
pub use theme::{PLANET_THEMES, PlanetPalette, PlanetStratum, PlanetTheme, PlanetThemeId, theme};
pub use world::{SurfaceSite, ThemeWorld, world_of};

use crate::{
    polar::{MIN_LANDABLE_RADIUS, PolarGrid},
    terrain::{
        CHUNK_SIZE_TILES, ChunkPosition, MineralKind, Tile, TileKind, TilePosition, tile_durability,
    },
    worldgen::streamed::GeneratedChunk,
};

/// What defines a planet's ground (saved and replicated with its terrain).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PlanetParams {
    /// The body's own seed (the space generator derives it from the world seed and the
    /// body's coordinates).
    pub seed: u64,
    /// Datum radius in tiles (at least [`MIN_LANDABLE_RADIUS`]).
    pub radius: u32,
    /// Surface gravity in thousandths of standard gravity.
    pub gravity_milli: u16,
    /// The theme ([`theme`]).
    pub theme: PlanetThemeId,
    /// Who lives here (Wave 12 aliens track): `0` nobody, else an
    /// [`crate::alien::AlienSpecies::folk`] byte. The generator flattens and paves their
    /// towns' ground and buries their lexicon stones ([`crate::alien::town`]).
    #[serde(default)]
    pub folk: u8,
}

/// The weakest gravity a planet's ground plays with (standard gravities).
///
/// It stays above the field model's weightless line ([`crate::field::WEIGHTLESS_GRAVITY`]), so a rig on the
/// smallest moon still has a "down" to stand on.
pub const MIN_PLANET_GRAVITY: f32 = 0.25;
/// The strongest gravity a planet's ground plays with (standard gravities).
pub const MAX_PLANET_GRAVITY: f32 = 2.5;

impl PlanetParams {
    /// A planet of `radius` tiles with theme `theme` and the placeholder gravity for its size
    /// ([`Self::placeholder_gravity_milli`]).
    #[must_use]
    pub const fn new(seed: u64, radius: u32, theme: PlanetThemeId) -> Self {
        Self {
            seed,
            radius: if radius < MIN_LANDABLE_RADIUS {
                MIN_LANDABLE_RADIUS
            } else {
                radius
            },
            gravity_milli: Self::placeholder_gravity_milli(radius),
            theme,
            folk: 0,
        }
    }

    /// Placeholder surface gravity by size (per mille g): small moons pull a third of the
    /// home world, a 10,000-tile planet about a full g. The space generator sets its own.
    #[must_use]
    pub const fn placeholder_gravity_milli(radius: u32) -> u16 {
        let milli = 300 + radius / 14;
        #[allow(clippy::cast_possible_truncation, reason = "at most 1,200")]
        let milli = if milli > 1_200 { 1_200 } else { milli as u16 };
        milli
    }

    /// Surface gravity in standard gravities, clamped to [`MIN_PLANET_GRAVITY`]..=
    /// [`MAX_PLANET_GRAVITY`].
    #[must_use]
    pub fn gravity(&self) -> f32 {
        (f32::from(self.gravity_milli) / 1_000.0).clamp(MIN_PLANET_GRAVITY, MAX_PLANET_GRAVITY)
    }

    /// The body's polar grid.
    #[must_use]
    pub fn grid(&self) -> PolarGrid {
        PolarGrid::new(self.radius.max(MIN_LANDABLE_RADIUS))
    }
}

/// A body's ground as a generator of its unrolled strip (see the module docs).
pub trait BodyGround {
    /// The generated tile at strip tile `(column, row)` (columns wrap).
    fn generated_tile(&self, column: i32, row: i32) -> Tile;
    /// The first ground row of strip column `column` (columns wrap).
    fn surface_row(&self, column: i32) -> i32;
    /// The strip's width in columns (one lap round the body).
    fn strip_columns(&self) -> u32;
    /// The first row of the strip (the highest hilltop's sky; above it is open space).
    fn top_row(&self) -> i32;
    /// Rows from row 0 to the strip's bottom (the core): the terrain's height.
    fn height(&self) -> i32;
    /// Surface gravity, standard gravities (it points at the body's centre: "down" in the
    /// strip).
    fn gravity(&self) -> f32;
}

/// Rows of the core chamber (open, above the core floor).
pub const CORE_CHAMBER_ROWS: i32 = 8;
/// Rows of indestructible core floor under the chamber (the strip's last rows).
pub const CORE_FLOOR_ROWS: i32 = 2;
/// Hill-noise lattice spacing (columns) of the fine and the broad octave (both divide 512, so
/// the noise wraps seamlessly round every planet).
const HILL_FINE: i64 = 32;
const HILL_BROAD: i64 = 128;
/// Most hill rows above the datum.
pub const MAX_HILL_ROWS: i32 = 40;
/// Open rows the strip keeps above its highest possible hill (the low sky).
pub const SKY_ROWS: i32 = 24;

/// A planet's ground generator: its params, polar grid and theme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetGround {
    params: PlanetParams,
    grid: PolarGrid,
    /// The strip's first row (the low sky over the highest hill).
    top: i32,
    /// The body's secret (Wave 12: one on every small body, [`hooks::default_secret`]).
    secret: Option<SecretSite>,
    /// The first row of the indestructible core floor (the grid's core, or a theme world's
    /// strata bottom: [`ThemeWorld::core_floor_row`]).
    core_floor: i32,
    /// The alien towns on its ground (Wave 12 aliens track; none unless
    /// [`PlanetParams::folk`] names a species).
    towns: [Option<crate::alien::town::TownSite>; 3],
}

/// A splitmix64 step (stable hashing for generation).
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[allow(clippy::cast_sign_loss, reason = "hash inputs")]
pub(crate) const fn hash3(seed: u64, purpose: u64, a: i64, b: i64) -> u64 {
    mix(mix(mix(seed ^ purpose.wrapping_mul(0xA24B_AED4_963E_E407)) ^ a as u64) ^ b as u64)
}

impl PlanetGround {
    /// The ground of the planet `params` describe.
    #[must_use]
    pub fn new(params: PlanetParams) -> Self {
        let grid = params.grid();
        let world = world_of(params.theme);
        let grid_floor = grid.bottom_row() - CORE_FLOOR_ROWS + 1;
        let core_floor = world
            .and_then(|world| world.core_floor_row(&params, &grid))
            .unwrap_or(grid_floor)
            .min(grid_floor);
        let mut ground = Self {
            params,
            grid,
            top: 0,
            secret: None,
            core_floor,
            towns: [None; 3],
        };
        ground.top =
            -ground.max_hill_rows() - SKY_ROWS - world.map_or(0, world::ThemeWorld::extra_sky_rows);
        ground.secret = ground.hooks().secret(&ground);
        ground.towns = crate::alien::town::sites(&ground);
        ground
    }

    /// The theme's hooks ([`hooks::PlanetThemeHooks`]): generation goes through them.
    #[must_use]
    pub fn hooks(&self) -> &'static dyn PlanetThemeHooks {
        hooks::hooks(self.params.theme)
    }

    /// The theme's data (its hooks' [`hooks::PlanetThemeHooks::data`]).
    #[must_use]
    pub fn data_theme(&self) -> &'static PlanetTheme {
        self.hooks().data()
    }

    /// The body's secret, if it has one.
    #[must_use]
    pub const fn secret(&self) -> Option<SecretSite> {
        self.secret
    }

    /// The strip's first row (see [`BodyGround::top_row`]).
    #[must_use]
    pub const fn strip_top(&self) -> i32 {
        self.top
    }

    /// The strip's width in columns (see [`BodyGround::strip_columns`]).
    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        reason = "a few hundred thousand columns at most"
    )]
    pub const fn strip_width(&self) -> i32 {
        self.grid.surface_columns() as i32
    }

    #[must_use]
    pub const fn params(&self) -> &PlanetParams {
        &self.params
    }

    #[must_use]
    pub const fn grid(&self) -> &PolarGrid {
        &self.grid
    }

    #[must_use]
    pub fn theme(&self) -> &'static PlanetTheme {
        self.data_theme()
    }

    /// Hill rows the generator may raise above the datum.
    #[must_use]
    #[allow(clippy::cast_possible_wrap, reason = "bounded radii")]
    pub fn max_hill_rows(&self) -> i32 {
        let permille = i64::from(self.theme().hill_permille);
        i32::try_from(i64::from(self.grid.radius()) * permille / 1_000)
            .unwrap_or(MAX_HILL_ROWS)
            .clamp(3, MAX_HILL_ROWS)
    }

    /// The last row of the strip (the core floor's bottom).
    #[must_use]
    pub const fn bottom_row(&self) -> i32 {
        self.grid.bottom_row()
    }

    /// The first row of the core floor (indestructible).
    #[must_use]
    pub const fn core_floor_row(&self) -> i32 {
        self.core_floor
    }

    /// The first (top) row of the core chamber.
    #[must_use]
    pub const fn core_chamber_top(&self) -> i32 {
        self.core_floor_row() - CORE_CHAMBER_ROWS
    }

    /// Whether strip row `row` is in the core chamber (open ground above the core floor).
    #[must_use]
    pub const fn in_core_chamber(&self, row: i32) -> bool {
        row >= self.core_chamber_top() && row < self.core_floor_row()
    }

    /// Periodic value noise at `column` with lattice spacing `spacing` (`0..=1024`): wraps
    /// with the strip.
    #[allow(clippy::cast_possible_truncation, reason = "a 10-bit value")]
    #[must_use]
    pub fn periodic_noise(&self, purpose: u64, column: i64, spacing: i64) -> i64 {
        let width = i64::from(self.grid.surface_columns());
        let column = column.rem_euclid(width);
        let cells = width / spacing;
        let cell = column / spacing;
        let t = column - cell * spacing;
        let value = |index: i64| -> i64 {
            (hash3(self.params.seed, purpose, index.rem_euclid(cells), 0) >> 54).cast_signed()
        };
        let (a, b) = (value(cell), value(cell + 1));
        // Smoothstep in integers: s = t^2 (3 spacing - 2 t) / spacing^3, scaled to 1024.
        let s = t * t * (3 * spacing - 2 * t) * 1_024 / (spacing * spacing * spacing);
        a + (b - a) * s / 1_024
    }

    /// Hill height (rows above the datum) at strip column `column`: a frontier world's own
    /// surface (Wave 12 planets-b), or the theme's surface shaper
    /// ([`hooks::PlanetThemeHooks::surface_rows`]), kept within the hill limit.
    #[must_use]
    pub fn hill(&self, column: i64) -> i32 {
        // A frontier world's surface has a shape of its own (Wave 12 planets-b).
        if let Some(style) = crate::frontier::style_of(self.params.theme) {
            return crate::frontier::generate::surface_height(self, style, column);
        }
        self.hooks()
            .surface_rows(self, column)
            .clamp(-MAX_HILL_ROWS, self.max_hill_rows())
    }

    /// The default surface shaper: two octaves of periodic value noise (rows above the datum
    /// at strip `column`).
    #[must_use]
    pub fn rolling_hills(&self, column: i64) -> i32 {
        let fine = self.periodic_noise(1, column, HILL_FINE);
        let broad = self.periodic_noise(2, column, HILL_BROAD);
        let max = i64::from(self.max_hill_rows());
        #[allow(clippy::cast_possible_truncation, reason = "at most MAX_HILL_ROWS")]
        let rows = ((broad * 3 + fine) * max / (4 * 1_023)) as i32;
        rows
    }

    /// Depth (rows below the surface) where stratum `index` ends, ragged per ring tile.
    #[allow(clippy::cast_possible_truncation, reason = "bounded depths")]
    fn stratum_end(&self, index: usize, column: i64) -> i32 {
        let strata = self.hooks().strata();
        let depth = i64::from(self.strata_depth());
        let share: i64 = strata[..=index]
            .iter()
            .map(|stratum| i64::from(stratum.share_permille))
            .sum();
        let ragged = self.periodic_noise(10 + index as u64, column, HILL_FINE) * 6 / 1_024 - 3;
        (depth * share / 1_000 + ragged) as i32
    }

    /// Rows the theme's strata shares divide: down to the core chamber, or a frontier world's
    /// authored depth ([`crate::frontier::strata_rows`]; its last stratum runs on below it to
    /// the core chamber).
    #[must_use]
    pub fn strata_depth(&self) -> i32 {
        match crate::frontier::style_of(self.params.theme) {
            Some(_) => crate::frontier::strata_rows(self.grid.radius())
                .min(self.core_chamber_top() - CORE_CHAMBER_ROWS),
            None => self.core_chamber_top(),
        }
    }

    /// The stratum (index into the theme's strata) at strip tile `(column, row)`.
    #[must_use]
    pub fn stratum_index(&self, column: i32, row: i32) -> usize {
        let strata = self.hooks().strata();
        let column = i64::from(column);
        let depth = row - self.surface_row_of(column);
        (0..strata.len())
            .find(|index| depth < self.stratum_end(*index, column))
            .unwrap_or(strata.len() - 1)
    }

    /// The stratum's name at `(column, row)` (above ground: the theme's name).
    #[must_use]
    pub fn layer_name(&self, column: i32, row: i32) -> &'static str {
        if row < self.surface_row(column) {
            return self.theme().name;
        }
        if row >= self.core_chamber_top() {
            return "Core Chamber";
        }
        self.hooks().strata()[self.stratum_index(column, row)].name
    }

    fn surface_row_of(&self, column: i64) -> i32 {
        let raw = self.raw_surface_row(column);
        // A theme world levels its own settlement site (where its town stands).
        if self.params.folk == 0 || world_of(self.params.theme).is_some() {
            return raw;
        }
        crate::alien::town::flattened_row(&self.towns, self.strip_width(), column, raw)
    }

    /// The surface row at `column` before any town levels it (the theme world's own surface,
    /// or the hills alone).
    #[must_use]
    pub fn raw_surface_row(&self, column: i64) -> i32 {
        if let Some(world) = world_of(self.params.theme) {
            return world.surface_row(self, column);
        }
        -self.hill(column)
    }

    /// The alien town sites on this ground (Wave 12 aliens track).
    #[must_use]
    pub const fn town_sites(&self) -> &[Option<crate::alien::town::TownSite>; 3] {
        &self.towns
    }

    /// The caves: `(centre column, centre row, half width, half height)` in strip tiles.
    pub(crate) fn caves(&self) -> Vec<(i64, i32, i64, i32)> {
        let width = i64::from(self.grid.surface_columns());
        let count = width * i64::from(self.theme().caves_per_256_columns) / 256;
        let depth = i64::from(self.core_chamber_top());
        (0..count)
            .map(|index| {
                let roll = hash3(self.params.seed, 20, index, 0);
                #[allow(clippy::cast_possible_wrap, reason = "bounded rolls")]
                let column = (roll % width.cast_unsigned()).cast_signed();
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap,
                    reason = "bounded rows"
                )]
                let row = (8 + (roll >> 20) % (depth * 7 / 10).max(1).cast_unsigned()) as i32;
                #[allow(clippy::cast_possible_wrap, reason = "small sizes")]
                let half_width = 4 + ((roll >> 40) % 8) as i64;
                #[allow(clippy::cast_possible_truncation, reason = "small sizes")]
                let half_height = 2 + ((roll >> 50) % 4) as i32;
                (column, row, half_width, half_height)
            })
            .collect()
    }

    /// Whether strip tile `(column, row)` lies in one of `caves` (see [`Self::in_cave`]).
    pub(crate) fn in_cave_at(&self, caves: &[(i64, i32, i64, i32)], column: i64, row: i32) -> bool {
        row > 4 && self.in_cave(caves, column, row)
    }

    /// Whether strip tile `(column, row)` lies in a cave (its physical shape: an inner ring's
    /// strip columns are narrower, so the cave keeps its shape in ring tiles).
    pub(crate) fn in_cave(&self, caves: &[(i64, i32, i64, i32)], column: i64, row: i32) -> bool {
        let width = i64::from(self.grid.surface_columns());
        let span = i64::from(self.grid.span_at(row));
        caves.iter().any(|&(cx, cy, hw, hh)| {
            let mut dx = (column - cx).rem_euclid(width);
            if dx > width / 2 {
                dx = width - dx;
            }
            let dx = dx / span;
            let dy = i64::from(row - cy);
            // (dx / hw)^2 + (dy / hh)^2 <= 1, in integers.
            let hh = i64::from(hh);
            dx * dx * hh * hh + dy * dy * hw * hw <= hw * hw * hh * hh
        })
    }

    /// An alien town's tile at `(column, row)` (its paving, a lexicon stone), if any.
    fn town_tile(&self, column: i64, row: i32, surface: i32) -> Option<TileKind> {
        if self.params.folk == 0 {
            return None;
        }
        crate::alien::town::town_tile(&self.towns, self.strip_width(), column, row, surface)
    }

    /// Whether caves keep clear of `(column, row)` (under an alien town).
    fn keeps_caves_out(&self, column: i64, row: i32) -> bool {
        self.params.folk != 0
            && crate::alien::town::keeps_caves_out(&self.towns, self.strip_width(), column, row)
    }

    pub(crate) const fn tile_of_kind(kind: TileKind) -> Tile {
        Tile {
            kind,
            durability: tile_durability(kind),
        }
    }

    /// The generated tile at strip tile `(column, row)` with the cave list precomputed.
    pub(crate) fn generated_with(
        &self,
        caves: &[(i64, i32, i64, i32)],
        column: i32,
        row: i32,
    ) -> Tile {
        let air = Self::tile_of_kind(TileKind::Air);
        if row > self.bottom_row() {
            return Self::tile_of_kind(TileKind::Bedrock);
        }
        if row >= self.core_floor_row() {
            return Self::tile_of_kind(TileKind::Bedrock);
        }
        let theme = self.theme();
        // Every cell of one ring tile reads the same: generate at the tile's first column.
        let tile = self.grid.tile_of(i64::from(column), row);
        let first = i64::from(self.grid.first_strip_column(tile));
        if self.in_core_chamber(row) {
            // The chamber's floor row is lined with the theme's core ore, with gaps.
            if row == self.core_floor_row() - 1
                && hash3(self.params.seed, 30, first, 0).is_multiple_of(3)
            {
                return Self::tile_of_kind(theme.core_ore);
            }
            // A few pillars hold the roof.
            if hash3(self.params.seed, 31, first, 0).is_multiple_of(23) {
                return Self::tile_of_kind(TileKind::Lunar(crate::moon::LunarTile::DeepRock));
            }
            return air;
        }
        let surface = self.surface_row_of(first);
        let hooks = self.hooks();
        // Landmarks, ruins and secrets (Wave 12 theme hooks) come before the strata.
        let first_column = i32::try_from(first).unwrap_or(0);
        if let Some(kind) = hooks.feature_tile(self, first_column, row, row - surface) {
            return Self::tile_of_kind(kind);
        }
        // A frontier world's own features (Wave 12 planets-b): oases, lava seas, spires,
        // landmarks, ruins, tubes, geodes, light wells.
        if let Some(style) = crate::frontier::style_of(self.params.theme)
            && let Some(tile) =
                crate::frontier::generate::feature_tile(self, style, caves, first, row, surface)
        {
            return tile;
        }
        if row < surface {
            return air;
        }
        if let Some(kind) = self.town_tile(first, row, surface) {
            return Self::tile_of_kind(kind);
        }
        if row > 4 && !self.keeps_caves_out(first, row) && self.in_cave(caves, first, row) {
            return air;
        }
        let depth = row - surface;
        if depth < theme.crust_rows {
            return Self::tile_of_kind(theme.crust);
        }
        let stratum = &hooks.strata()[self.stratum_index(first_column, row)];
        let roll = hash3(self.params.seed, 40, first, i64::from(row));
        let ore_roll = (roll % 10_000) as u16;
        if let Some(ore) = hooks.ore_at(stratum, ore_roll) {
            return Self::tile_of_kind(ore);
        }
        // Accent rock in patches (coarse noise on the ring tile).
        let patch = hash3(self.params.seed, 41, first / 4, i64::from(row / 3)) % 1_000;
        if patch < u64::from(stratum.accent_permille) {
            return Self::tile_of_kind(stratum.accent);
        }
        Self::tile_of_kind(stratum.rock)
    }

    /// The generated tiles of `chunk` (row-major, [`ChunkPosition::local_index`] order;
    /// columns wrap, rows past the strip read as bedrock).
    #[must_use]
    pub fn chunk_tiles(&self, chunk: ChunkPosition) -> Vec<Tile> {
        if let Some(world) = world_of(self.params.theme) {
            return world.chunk_tiles(self, chunk);
        }
        let caves = self.caves();
        let origin = chunk.origin();
        let mut tiles = Vec::with_capacity((CHUNK_SIZE_TILES * CHUNK_SIZE_TILES) as usize);
        for y in origin.y..origin.y + CHUNK_SIZE_TILES {
            for x in origin.x..origin.x + CHUNK_SIZE_TILES {
                tiles.push(self.generated_with(&caves, x, y));
            }
        }
        tiles
    }

    /// The generated chunk `chunk` (memoized per process; a pure function of the params).
    #[must_use]
    pub fn generated_chunk(&self, chunk: ChunkPosition) -> Arc<GeneratedChunk> {
        let wrapped = ChunkPosition {
            x: chunk.x.rem_euclid(
                i32::try_from(self.grid.surface_columns()).unwrap_or(i32::MAX) / CHUNK_SIZE_TILES,
            ),
            y: chunk.y,
        };
        let key = (self.params, wrapped.x, wrapped.y);
        {
            let cache = CHUNKS.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(chunk) = cache.get(&key) {
                return Arc::clone(chunk);
            }
        }
        let generated = Arc::new(GeneratedChunk::from_tiles(self.chunk_tiles(wrapped)));
        let mut cache = CHUNKS.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= CHUNK_CACHE_CAPACITY {
            // A plain cache: forget everything (every entry regenerates bit for bit).
            cache.clear();
        }
        Arc::clone(cache.entry(key).or_insert(generated))
    }

    /// Whether `position` (strip tile, any column) is in the strip.
    #[must_use]
    pub fn contains_row(&self, row: i32) -> bool {
        row >= self.top_row() && row <= self.bottom_row()
    }

    /// A good place to set a rig down: the centre of strip column `column`, `rows_up` rows
    /// above its surface.
    #[must_use]
    pub fn landing_tile(&self, column: i32, rows_up: i32) -> TilePosition {
        TilePosition {
            x: column,
            y: self.surface_row(column) - rows_up,
        }
    }

    /// The richest ore of the theme (tests and the bot's sanity checks).
    #[must_use]
    pub fn ores(&self) -> Vec<MineralKind> {
        self.hooks()
            .strata()
            .iter()
            .flat_map(|stratum| stratum.ores.iter().map(|(mineral, _)| *mineral))
            .collect()
    }
}

/// Generated planet chunks cached per process (keyed by params and wrapped chunk).
static CHUNKS: Mutex<BTreeMap<(PlanetParams, i32, i32), Arc<GeneratedChunk>>> =
    Mutex::new(BTreeMap::new());
/// Most planet chunks cached at once.
const CHUNK_CACHE_CAPACITY: usize = 8_192;

impl BodyGround for PlanetGround {
    fn generated_tile(&self, column: i32, row: i32) -> Tile {
        let chunk = ChunkPosition::of(TilePosition { x: column, y: row });
        self.generated_chunk(chunk)
            .tile(ChunkPosition::local_index(TilePosition {
                x: column,
                y: row,
            }))
    }

    fn surface_row(&self, column: i32) -> i32 {
        let tile = self.grid.tile_of(i64::from(column), 0);
        self.surface_row_of(i64::from(self.grid.first_strip_column(tile)))
    }

    fn strip_columns(&self) -> u32 {
        self.grid.surface_columns()
    }

    fn top_row(&self) -> i32 {
        self.top
    }

    fn height(&self) -> i32 {
        self.bottom_row() + 1
    }

    fn gravity(&self) -> f32 {
        self.params.gravity()
    }
}
