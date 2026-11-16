//! The sky generator of [`GeneratorVersion::V8`] worlds (expansion plan F6, WP-SKY, WP-MOON).
//!
//! A V8 world is a V7 world with a sky: everything here is a pure integer function of the
//! settings and tile coordinates, so chunks stay seamless, order-independent and identical on
//! every platform.
//!
//! **Bands.** The sky is cut into altitude bands ([`SkyBand`]), each defined as a fraction of
//! the climb harness's measurements ([`crate::climb`], [`SkyBands::calibrated`]), so fuel tank
//! tiers unlock the sky in order. Each sky generator has its own frozen band table
//! ([`SkyBands`], [`super::GeneratorVersion::sky_bands`]) and placement reads the world's:
//! V8's is [`V8_SKY_BANDS`] (the `*_BOTTOM_ROWS` constants), the next generator adopts
//! [`NEXT_SKY_BANDS`], recalibrated to the second fuel curve. In a V8 sky:
//!
//! | Band | Rows above row 0 | Holds |
//! |---|---|---|
//! | Low isles | [`LOW_ISLES_BOTTOM_ROWS`].. | clusters of small islands (tank tier ~2-4) |
//! | Cloud archipelago | [`ARCHIPELAGO_BOTTOM_ROWS`].. | chains of medium islands (mid tiers) |
//! | Dead-air gap | [`DEAD_AIR_BOTTOM_ROWS`].. | nothing: crosswinds and thin air |
//! | Sky continents | [`CONTINENTS_BOTTOM_ROWS`].. | continent footprints hundreds of tiles wide |
//! | Orbit | [`ORBIT_BOTTOM_ROWS`].. | nothing: near-zero gravity, no air |
//! | Lunar | [`MOON_BOTTOM_ROWS`].. | the Moon |
//!
//! **Placement.** Islands and continents are placed per cell of a 2D region grid (column x
//! altitude) per band, from a per-cell [`RegionRng`]; every land stays inside its cell, so a
//! chunk consults only the cells it overlaps. The Moon is one giant landmass
//! ([`moon`]): 2,000-2,900 tiles wide, its underside at [`MOON_BOTTOM_ROWS`], hanging above a
//! deterministic column ([`moon_column`]) 3,200-4,800 tiles from town.
//!
//! **Shapes.** An island is a turf top over soil over a rock underside in its palette
//! ([`SkyBiome`]), with crystal pockets and roots and crystal dangling below. On V8 worlds
//! continents are placeholder bodies (continent rock under turf); V9 worlds fill them
//! ([`continent`]: mountains, caves, lakes, an aether core, sky ores, a city, a temple and harpy
//! nests). The Moon is regolith over moon rock with craters.
//!
//! **Streaming.** A chunk that touches no land is air ([`chunk_is_empty`]); the chunk
//! assembler hands out one shared empty chunk for it, so empty sky costs no generation and no
//! terrain ops.
//!
//! **Features.** Islands, continents and the Moon are regional features
//! ([`FeatureKind::SkyIsland`], [`FeatureKind::SkyContinent`], [`FeatureKind::Moon`]), found by
//! the usual area and nearest-feature queries.
//!
//! **Updrafts.** Columns of rising air ([`Updraft`], [`updrafts_touching`], [`updraft_at`])
//! are placed on their own grid in the island and continent bands (never in the dead-air gap):
//! a free ride up for a rig that finds one.
//!
//! **Island content (V9, WP-SKY).** [`GeneratorVersion::V9`] keeps V8's bands and land
//! placement and fills the low isles and archipelago isles ([`isles`]): skystone and cloud
//! silver ore, sky caches, nests, ruins, springs pouring waterfalls into the void, vines,
//! keystones of crumbling isles, updraft vents and fuel buoys. One cell of every archipelago
//! block ([`towns`]) grows a wide, flat town isle with a sky town on it, and archipelago chains
//! carry jet streams and island vents breathe updrafts ([`winds`]).
//!
//! [`GeneratorVersion::V8`]: super::GeneratorVersion::V8
//! [`GeneratorVersion::V9`]: super::GeneratorVersion::V9
//! [`FeatureKind::SkyIsland`]: super::FeatureKind::SkyIsland
//! [`FeatureKind::SkyContinent`]: super::FeatureKind::SkyContinent
//! [`FeatureKind::Moon`]: super::FeatureKind::Moon

pub mod isles;
pub mod towns;
pub mod winds;

pub use isles::{BuoySite, IsleContent};
pub use towns::{SkyTownSite, sky_town_at, sky_town_site, sky_towns_touching};
pub use winds::{JetStream, jet_stream_at, jet_streams_touching, vent_updraft_at};

use crate::{
    climb::{CLIMB_CEILINGS, ClimbTable, ceiling_in, fraction},
    sky::{SkyBand, SkyBiome, SkyTile},
    terrain::{CHUNK_SIZE_TILES, TileKind, TilePosition},
    village::placement::TOWN_CENTER_COLUMN,
    worldgen::{
        Band, FeatureDetail, FeatureId, FeatureKind, TileRect, WorldFeature, WorldSettings,
        noise::{NOISE_MAX, RegionRng, centred, fractal_noise, hash2, line_noise},
        streamed::{
            cache::MemoCache,
            index::{Layer, feature_id},
        },
    },
};

// ---------------------------------------------------------------------------------------------
// Bands (rows above row 0), per generator version, each a fraction of a climb table.
// ---------------------------------------------------------------------------------------------

/// Where the bands of one generator version's sky start, in rows above row 0.
///
/// Every sky generator has its own entry ([`GeneratorVersion::sky_bands`]): placement reads
/// the world's entry, so a world keeps generating identically when the fuel curve (and with it
/// the climb table) changes. An entry is computed once from a climb table
/// ([`Self::calibrated`]) and then frozen: V8's is [`V8_SKY_BANDS`] (calibrated from
/// [`crate::climb::V8_CLIMB_CEILINGS`]); the next sky generator adopts [`NEXT_SKY_BANDS`]
/// (calibrated from the current [`crate::climb::CLIMB_CEILINGS`]) and freezes it the same
/// way.
///
/// [`GeneratorVersion::sky_bands`]: super::GeneratorVersion::sky_bands
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SkyBands {
    /// Floor of the low isles (small island clusters).
    pub low_isles: i32,
    /// Floor of the cloud archipelago (chains of medium islands).
    pub archipelago: i32,
    /// Floor of the dead-air gap (nothing: crosswinds and thin air).
    pub dead_air: i32,
    /// Floor of the sky continents.
    pub continents: i32,
    /// Floor of orbit (gravity falls to near zero above it).
    pub orbit: i32,
    /// The lowest point of the Moon's underside.
    pub moon: i32,
}

impl SkyBands {
    /// The bands calibrated from climb table `table` (rows above row 0, each rounded down to a
    /// multiple of 10), so fuel tank tiers unlock the sky in order:
    ///
    /// - low isles: 65 % of a tier 2 tank with a tier 4 engine (at least 200 rows: clear of
    ///   the tallest mountains);
    /// - cloud archipelago: the full reach of a tier 4 tank and engine (so the low isles are
    ///   the tank tier 2-4 band; at least three low isle cells tall);
    /// - dead-air gap: 90 % of the best a rig without intakes can do (top tank and engine), so
    ///   the whole archipelago is open to mid and late tiers;
    /// - sky continents: 85 % of a near-max rig (tier 13 tank, top engine, top intakes): no rig
    ///   without intakes gets there, nor a tier 12 tank with them;
    /// - orbit: 95 % of a near-max rig (at least a continent's height above the continents);
    /// - the Moon: 85 % of a maxed rig's still-air reach (the last climb through near-zero-g
    ///   orbit is cheap).
    #[must_use]
    pub const fn calibrated(table: &ClimbTable) -> Self {
        let low_isles = max(fraction(ceiling_in(table, 2, 4, 0), 65), 200);
        let archipelago = max(
            fraction(ceiling_in(table, 4, 4, 0), 100),
            low_isles + 3 * LOW_ISLE_CELL.1,
        );
        let dead_air = max(
            fraction(ceiling_in(table, 15, 15, 0), 90),
            archipelago + 3 * ARCHIPELAGO_CELL.1,
        );
        let continents = fraction(ceiling_in(table, 13, 15, 6), 85);
        let orbit = max(
            fraction(ceiling_in(table, 13, 15, 6), 95),
            continents + CONTINENT_BAND_MIN_ROWS,
        );
        let moon = max(
            fraction(ceiling_in(table, 15, 15, 6), 85),
            orbit + ORBIT_MIN_ROWS,
        );
        Self {
            low_isles,
            archipelago,
            dead_air,
            continents,
            orbit,
            moon,
        }
    }

    /// Rows of sky a world with these bands holds above row 0 (its first row is minus this):
    /// the Moon's surface plus some space, rounded up to whole chunks
    /// ([`super::streamed::top_row`]). Rigs may fly higher still (into open space with no
    /// tiles).
    #[must_use]
    pub const fn ceiling_rows(&self) -> i32 {
        let rows = self.moon + MOON_DEPTH_ROWS + MOON_HILL_ROWS + 2 + SPACE_ABOVE_MOON_ROWS;
        (rows + CHUNK_SIZE_TILES - 1) / CHUNK_SIZE_TILES * CHUNK_SIZE_TILES
    }

    /// Altitude a full-tank climb must top to count as reaching the low isles: 40 rows into
    /// their band, where the lowest isles' turf is (`milestone::island_capable`).
    #[must_use]
    pub const fn island_reach_rows(&self) -> i32 {
        self.low_isles + 40
    }

    /// Altitude a full-tank climb must top to cross the dead-air gap: the sky continents' floor
    /// (`milestone::crosses_dead_air`).
    #[must_use]
    pub const fn dead_air_reach_rows(&self) -> i32 {
        self.continents
    }

    /// The band holding altitude `altitude_rows` (rows above row 0; negative underground).
    #[must_use]
    pub const fn band_at(&self, altitude_rows: i32) -> SkyBand {
        SkyBand::at_altitude_in(altitude_rows, self)
    }
}

/// The bands of [`GeneratorVersion::V8`](super::GeneratorVersion::V8) skies. **Frozen.**
///
/// These are the values V8 worlds generate with (calibrated from the first fifteen-tier fuel curve's
/// climb table, [`crate::climb::V8_CLIMB_CEILINGS`]); never change them, or every V8 world's
/// sky moves.
pub const V8_SKY_BANDS: SkyBands = SkyBands {
    low_isles: 220,
    archipelago: 540,
    dead_air: 4_480,
    continents: 8_080,
    orbit: 9_030,
    moon: 10_320,
};

/// The bands for the **next** sky generator (the one after V8).
///
/// Calibrated by the same rule as V8's ([`SkyBands::calibrated`]) from the current fuel curve's climb table
/// ([`crate::climb::CLIMB_CEILINGS`]): the low isles open around tank tier 2-4, the cloud
/// archipelago spans the mid tiers, the dead-air gap is crossed only with a near-max tank and
/// altitude intakes, the continents sit near the max and the Moon at the top.
///
/// The bigger tanks of the second fuel curve climb about four times higher at the top, so these
/// bands are far taller than V8's (500 / 1,350 / 4,720 / 35,500 / 39,680 / 46,010 rows). A
/// generator adopting them returns them from [`GeneratorVersion::sky_bands`] and freezes the
/// values as literals like [`V8_SKY_BANDS`] (a test pins today's values). Its altitude physics
/// must follow its bands (the vacuum fade, cold and gravity fields read V8's constants today,
/// [`crate::environment`]), and the climb table must be re-measured if that physics changes
/// below the Moon.
///
/// [`GeneratorVersion::sky_bands`]: super::GeneratorVersion::sky_bands
pub const NEXT_SKY_BANDS: SkyBands = SkyBands::calibrated(&CLIMB_CEILINGS);

/// Where the low isles start in a V8 sky ([`V8_SKY_BANDS`]).
pub const LOW_ISLES_BOTTOM_ROWS: i32 = V8_SKY_BANDS.low_isles;
/// Where the cloud archipelago starts in a V8 sky ([`V8_SKY_BANDS`]).
pub const ARCHIPELAGO_BOTTOM_ROWS: i32 = V8_SKY_BANDS.archipelago;
/// Where the dead-air gap starts in a V8 sky ([`V8_SKY_BANDS`]).
pub const DEAD_AIR_BOTTOM_ROWS: i32 = V8_SKY_BANDS.dead_air;
/// Where the sky continents start in a V8 sky ([`V8_SKY_BANDS`]).
pub const CONTINENTS_BOTTOM_ROWS: i32 = V8_SKY_BANDS.continents;
/// Where the orbit band starts in a V8 sky ([`V8_SKY_BANDS`]).
pub const ORBIT_BOTTOM_ROWS: i32 = V8_SKY_BANDS.orbit;
/// The lowest point of the Moon's underside in a V8 sky ([`V8_SKY_BANDS`]).
pub const MOON_BOTTOM_ROWS: i32 = V8_SKY_BANDS.moon;
/// Thickness of the Moon at its centre (underside point to surface).
pub const MOON_DEPTH_ROWS: i32 = 240;
/// Rows of space kept above the Moon's surface before the world's first row.
const SPACE_ABOVE_MOON_ROWS: i32 = 704;
/// Rows of sky a V8 world holds above row 0 (its first row is `-SKY_CEILING_ROWS`); see
/// [`SkyBands::ceiling_rows`].
pub const SKY_CEILING_ROWS: i32 = V8_SKY_BANDS.ceiling_rows();

/// The bands of `settings`' sky (`None` on generators without a sky).
#[must_use]
pub const fn bands_of(settings: &WorldSettings) -> Option<SkyBands> {
    settings.generator.sky_bands()
}

/// The least band height that fits a continent.
const CONTINENT_BAND_MIN_ROWS: i32 = 200;
/// The least orbit band height.
const ORBIT_MIN_ROWS: i32 = 160;

const fn max(a: i32, b: i32) -> i32 {
    if a > b { a } else { b }
}

/// Converts an altitude (rows above row 0) to a tile row.
#[must_use]
pub const fn row_of(altitude_rows: i32) -> i32 {
    -altitude_rows
}

// ---------------------------------------------------------------------------------------------
// Lands.
// ---------------------------------------------------------------------------------------------

/// Low isle cells: `(columns, rows)`.
pub const LOW_ISLE_CELL: (i32, i32) = (192, 100);
/// Cloud archipelago cells.
pub const ARCHIPELAGO_CELL: (i32, i32) = (320, 176);
/// Sky continent cells (columns; one cell row spans the band).
pub const CONTINENT_CELL_COLUMNS: i32 = 1_400;
/// Hills on the Moon's surface rise at most this many rows above its centre surface row.
const MOON_HILL_ROWS: i32 = 16;

const SALT_LOW: u64 = 0x5C1E_0001;
const SALT_ARCHIPELAGO: u64 = 0x5C1E_0002;
const SALT_CONTINENT: u64 = 0x5C1E_0003;
const SALT_MOON: u64 = 0x5C1E_0004;
const SALT_SHAPE: u64 = 0x5C1E_0010;
const SALT_UPDRAFT: u64 = 0x5C1E_0020;

/// Feature-id band codes (`ry = code * 1000 + cell row`).
const ID_LOW: i32 = 1;
const ID_ARCHIPELAGO: i32 = 2;
const ID_CONTINENT: i32 = 4;
const ID_MOON: i32 = 6;

/// What a sky land is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SkyLandKind {
    /// A small low isle.
    Isle,
    /// A medium island of a cloud archipelago chain.
    ArchipelagoIsle,
    /// A sky continent footprint.
    Continent,
    /// The Moon.
    Moon,
    /// The wide, flat-topped island a sky town stands on (V9: one per archipelago block,
    /// see [`towns`]).
    TownIsle,
}

impl SkyLandKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Isle => "sky isle",
            Self::ArchipelagoIsle => "archipelago isle",
            Self::Continent => "sky continent",
            Self::Moon => "the Moon",
            Self::TownIsle => "town isle",
        }
    }

    /// Whether this is an island (a low isle, an archipelago isle or a town isle).
    #[must_use]
    pub const fn is_island(self) -> bool {
        matches!(self, Self::Isle | Self::ArchipelagoIsle | Self::TownIsle)
    }
}

/// One floating landmass: an island, a continent or the Moon.
///
/// Its shape is a pure function of these fields ([`Self::tile_at`]): `top_row` is the row of
/// its turf at `center_x`, the body spans `center_x - half_width..=center_x + half_width`, and
/// the underside hangs down to about `top_row + depth` at the centre.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkyLand {
    pub id: FeatureId,
    pub kind: SkyLandKind,
    pub biome: SkyBiome,
    pub center_x: i32,
    pub top_row: i32,
    pub half_width: i32,
    pub depth: i32,
    shape: u64,
    /// What a V9 sky grows on the island (`None` on V8 skies, continents and the Moon).
    content: Option<IsleContent>,
    /// The content of a V9 continent (`None` for islands, the Moon and V8 continents).
    plan: Option<continent::ContinentPlan>,
}

impl SkyLand {
    /// The content plan of a V9 continent ([`continent`]); `None` for everything else.
    #[must_use]
    pub const fn plan(&self) -> Option<continent::ContinentPlan> {
        self.plan
    }

    /// The land's shape seed (presentation varies props with it).
    #[must_use]
    pub const fn shape_seed(&self) -> u64 {
        self.shape
    }

    /// Rows the top may rise above `top_row`.
    const fn rise(&self) -> i32 {
        let base = match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle => 2 + self.half_width / 10,
            SkyLandKind::Continent => {
                if self.plan.is_some() {
                    continent::MAX_PEAK_ROWS + 4
                } else {
                    18
                }
            }
            SkyLandKind::Moon => MOON_HILL_ROWS + 2,
            SkyLandKind::TownIsle => 2,
        };
        // Island props (nests, ruin pillars) stand on the turf.
        if self.content.is_some() {
            base + isles::PROP_ROWS
        } else {
            base
        }
    }

    /// Rows anything may hang below `top_row + depth`.
    const fn hang(&self) -> i32 {
        match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                let vines = if self.content.is_some() {
                    isles::VINE_EXTRA_ROWS
                } else {
                    0
                };
                self.depth / 2 + 12 + vines
            }
            SkyLandKind::Continent | SkyLandKind::Moon => 16,
        }
    }

    /// The island's V9 content (`None` on V8 skies, continents and the Moon).
    #[must_use]
    pub const fn content(&self) -> Option<&IsleContent> {
        self.content.as_ref()
    }

    /// Every tile the land can occupy.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.center_x - self.half_width,
                y: self.top_row - self.rise(),
            },
            max: TilePosition {
                x: self.center_x + self.half_width,
                y: self.top_row + self.depth + self.hang(),
            },
        }
    }

    /// The top surface row (turf or regolith) of column `x`, if the land covers it.
    #[must_use]
    pub fn surface_row(&self, x: i32) -> Option<i32> {
        let dx = x - self.center_x;
        if dx.abs() > self.half_width {
            return None;
        }
        Some(self.top(x, dx))
    }

    /// The underside row of column `x` (the lowest body row, before anything dangling), if
    /// the land covers it.
    #[must_use]
    pub fn underside_row(&self, x: i32) -> Option<i32> {
        let dx = x - self.center_x;
        if dx.abs() > self.half_width {
            return None;
        }
        let top = self.top(x, dx);
        Some(self.bottom(x, dx, top))
    }

    fn top(&self, x: i32, dx: i32) -> i32 {
        let hw = i64::from(self.half_width.max(1));
        let dx64 = i64::from(dx);
        let falloff =
            |rows: i64| i32::try_from((hw * hw - dx64 * dx64) * rows / (hw * hw)).unwrap_or(0);
        match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle => {
                // A V9 buoy's pad is flat (five columns at the pad's row).
                if let Some(pad) = self.content.as_ref().and_then(|content| content.buoy)
                    && (x - pad.x).abs() <= 2
                {
                    return pad.y;
                }
                // (Props raise `rise` on V9 islands; the dome keeps V8's shape.)
                let dome_rows = 2 + self.half_width / 10 - 2;
                let dome = falloff(i64::from(dome_rows));
                let bump = centred(line_noise(self.shape, SALT_SHAPE, x, 7), 1);
                self.top_row - dome + bump
            }
            SkyLandKind::TownIsle => {
                // Flat under the town, rounding off over the last few columns.
                let edge = self.half_width - dx.abs();
                self.top_row + (3 - edge).max(0)
            }
            SkyLandKind::Continent => {
                let hills = fractal_noise(self.shape, SALT_SHAPE, x, 0, 56, 1, 3) * 16 / NOISE_MAX;
                let edge = (self.half_width - dx.abs()).clamp(0, 28);
                let base = self.top_row - hills + (28 - edge) / 4;
                self.plan
                    .as_ref()
                    .map_or(base, |plan| plan.surface(x, base, self.top_row))
            }
            SkyLandKind::Moon => {
                let hills = fractal_noise(self.shape, SALT_SHAPE, x, 0, 120, 1, 3) * MOON_HILL_ROWS
                    / NOISE_MAX;
                // The limb rounds off over the last 160 columns.
                let edge = (self.half_width - dx.abs()).clamp(0, 160);
                let limb = (160 - edge) * (160 - edge) / 640;
                self.top_row - hills + limb + crater_depth(self.shape, x)
            }
        }
    }

    fn bottom(&self, x: i32, dx: i32, top: i32) -> i32 {
        let hw = i64::from(self.half_width.max(1));
        let dx64 = i64::from(dx);
        let lens =
            i32::try_from(i64::from(self.depth) * (hw * hw - dx64 * dx64) / (hw * hw)).unwrap_or(0);
        let jag = match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                // A teardrop: an extra keel under the middle third.
                let third = (self.half_width / 3).max(1);
                let keel = (third - dx.abs()).max(0) * (self.depth / 2) / third;
                keel + centred(line_noise(self.shape, SALT_SHAPE + 1, x, 3), 2)
            }
            SkyLandKind::Continent => centred(line_noise(self.shape, SALT_SHAPE + 1, x, 9), 5),
            SkyLandKind::Moon => centred(line_noise(self.shape, SALT_SHAPE + 1, x, 23), 8),
        };
        (self.top_row + lens + jag).max(top + 1)
    }

    /// The tile of this land at `(x, y)`, or `None` where it has none (air around it).
    #[must_use]
    pub fn tile_at(&self, x: i32, y: i32) -> Option<TileKind> {
        let dx = x - self.center_x;
        if dx.abs() > self.half_width || y < self.top_row - self.rise() {
            return None;
        }
        let top = self.top(x, dx);
        if let Some(content) = &self.content {
            return content.tile_at(self, x, dx, y, top);
        }
        if let Some(plan) = &self.plan {
            // A V9 continent: the temple shows through everything, nests sit on summits.
            if let Some(temple) = plan.temple
                && let Some(symbol) = temple.cell(x, y)
                && let Some(tile) = continent::temple_tile(symbol)
            {
                return (tile != TileKind::Air).then_some(tile);
            }
            if y < top {
                return plan.decor(x, y, top).map(TileKind::Sky);
            }
            let bottom = self.bottom(x, dx, top);
            if y <= bottom {
                return plan.body(self, x, y, y - top, bottom);
            }
            return self.dangling(x, dx, y - bottom).map(TileKind::Sky);
        }
        if y < top {
            return None;
        }
        let bottom = self.bottom(x, dx, top);
        let layer = y - top;
        if y <= bottom {
            return Some(TileKind::Sky(self.body_tile(x, y, layer)));
        }
        self.dangling(x, dx, y - bottom).map(TileKind::Sky)
    }

    fn body_tile(&self, x: i32, y: i32, layer: i32) -> SkyTile {
        match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                let soil = 2 + i32::from(hash2(self.shape, SALT_SHAPE + 2, x, 0).is_multiple_of(3));
                if layer == 0 {
                    SkyTile::Turf(self.biome)
                } else if layer <= soil {
                    SkyTile::Soil
                } else if layer >= 4 && hash2(self.shape, SALT_SHAPE + 3, x, y).is_multiple_of(47) {
                    SkyTile::Crystal
                } else {
                    SkyTile::Rock(self.biome)
                }
            }
            SkyLandKind::Continent => {
                if layer == 0 {
                    SkyTile::Turf(self.biome)
                } else if layer <= 3 {
                    SkyTile::Soil
                } else if layer <= 6 {
                    SkyTile::Rock(self.biome)
                } else if hash2(self.shape, SALT_SHAPE + 3, x, y).is_multiple_of(89) {
                    SkyTile::Crystal
                } else {
                    SkyTile::ContinentRock
                }
            }
            SkyLandKind::Moon => {
                if layer <= 4 {
                    SkyTile::Regolith
                } else {
                    SkyTile::MoonRock
                }
            }
        }
    }

    /// Roots and crystal hanging `below` rows under the underside of column `x`.
    fn dangling(&self, x: i32, dx: i32, below: i32) -> Option<SkyTile> {
        let roll = hash2(self.shape, SALT_SHAPE + 4, x, 0);
        match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                if roll.is_multiple_of(4) {
                    let length = 1
                        + i32::try_from((roll >> 8) % 4).unwrap_or(0)
                        + (self.half_width - dx.abs()) / 8;
                    (below <= length).then_some(SkyTile::Roots)
                } else if roll % 9 == 1 && dx.abs() * 2 < self.half_width {
                    let length = 1 + i32::try_from((roll >> 8) % 3).unwrap_or(0);
                    (below <= length).then_some(SkyTile::Crystal)
                } else {
                    None
                }
            }
            SkyLandKind::Continent => {
                let length = 2 + i32::try_from((roll >> 8) % 8).unwrap_or(0);
                (roll.is_multiple_of(7) && below <= length).then_some(SkyTile::Roots)
            }
            SkyLandKind::Moon => {
                let length = 1 + i32::try_from((roll >> 8) % 6).unwrap_or(0);
                (roll.is_multiple_of(13) && below <= length).then_some(SkyTile::Crystal)
            }
        }
    }

    /// The land as a regional feature.
    #[must_use]
    pub fn feature(&self) -> WorldFeature {
        let (kind, detail) = match self.kind {
            SkyLandKind::Isle | SkyLandKind::ArchipelagoIsle | SkyLandKind::TownIsle => {
                (FeatureKind::SkyIsland, FeatureDetail::SkyIsland(self.biome))
            }
            SkyLandKind::Continent => (FeatureKind::SkyContinent, FeatureDetail::None),
            SkyLandKind::Moon => (FeatureKind::Moon, FeatureDetail::None),
        };
        WorldFeature {
            id: self.id,
            kind,
            detail,
            band: Band::Topsoil,
            center: TilePosition {
                x: self.center_x,
                y: self.top_row,
            },
            bounds: self.bounds(),
            size: u32::try_from(self.half_width * 2 + 1).unwrap_or(0),
        }
    }

    /// The altitude band the land hangs in.
    #[must_use]
    pub const fn band(&self) -> SkyBand {
        SkyBand::at_row(self.top_row)
    }
}

/// How far a crater lowers the Moon's surface at column `x`.
fn crater_depth(shape: u64, x: i32) -> i32 {
    const CELL: i32 = 96;
    let cell = x.div_euclid(CELL);
    let mut depth = 0;
    for cx in cell - 1..=cell + 1 {
        let roll = hash2(shape, SALT_SHAPE + 5, cx, 0);
        if roll % 10 >= 6 {
            continue;
        }
        let radius = 8 + i32::try_from((roll >> 8) % 33).unwrap_or(0);
        let center = cx * CELL + CELL / 2 + i32::try_from((roll >> 16) % 33).unwrap_or(0) - 16;
        let d = (x - center).abs();
        if d < radius {
            depth = depth.max((radius * radius - d * d) / (3 * radius));
        } else if d <= radius + 2 {
            depth = depth.min(-1);
        }
    }
    depth
}

/// The palette of an island hanging at altitude `altitude_rows`, from a roll.
const fn biome_for(bands: &SkyBands, altitude_rows: i32, roll: u64) -> SkyBiome {
    let cold = altitude_rows >= bands.archipelago;
    match roll % 10 {
        0..=4 => {
            if cold && roll.is_multiple_of(3) {
                SkyBiome::Frost
            } else {
                SkyBiome::Meadow
            }
        }
        5 | 6 => SkyBiome::Blossom,
        7 => {
            if cold {
                SkyBiome::Frost
            } else {
                SkyBiome::Amber
            }
        }
        _ => SkyBiome::Amber,
    }
}

/// The altitude range `(bottom, top)` (rows above row 0) islands of a band may use.
const fn band_rows(bands: &SkyBands, id_band: i32) -> (i32, i32) {
    match id_band {
        ID_LOW => (bands.low_isles + 8, bands.archipelago - 8),
        ID_ARCHIPELAGO => (bands.archipelago + 8, bands.dead_air - 24),
        _ => (bands.continents + 16, bands.orbit - 60),
    }
}

/// Cell rows of a band with cells `cell_rows` tall.
const fn band_cell_rows(bands: &SkyBands, id_band: i32, cell_rows: i32) -> i32 {
    let (bottom, top) = band_rows(bands, id_band);
    let rows = (top - bottom) / cell_rows;
    if rows < 1 { 1 } else { rows }
}

/// The altitude range `(bottom, top)` of cell row `ry` of a band.
const fn cell_altitudes(bands: &SkyBands, id_band: i32, ry: i32, cell_rows: i32) -> (i32, i32) {
    let (bottom, _) = band_rows(bands, id_band);
    (bottom + ry * cell_rows, bottom + (ry + 1) * cell_rows - 1)
}

/// The low isles of cell `(rx, ry)`: a cluster of up to three small islands.
fn low_isle_cell(seed: u64, bands: &SkyBands, rx: i32, ry: i32) -> Vec<SkyLand> {
    let mut rng = RegionRng::new(seed, SALT_LOW, rx, ry);
    if !rng.chance(640) {
        return Vec::new();
    }
    let (alt_bottom, alt_top) = cell_altitudes(bands, ID_LOW, ry, LOW_ISLE_CELL.1);
    let x0 = rx * LOW_ISLE_CELL.0;
    let count = rng.range(1, 3);
    let mut lands: Vec<SkyLand> = Vec::new();
    // A cluster: the first isle anywhere in the cell, the rest near it.
    let cluster_x = rng.range(x0 + 40, x0 + LOW_ISLE_CELL.0 - 40);
    let cluster_alt = rng.range(alt_bottom + 40, alt_top - 24);
    for local in 0..count {
        let half_width = rng.range(4, 12);
        let depth = rng.range(3, 6) + half_width / 2;
        let center_x = (cluster_x + rng.range(-34, 34))
            .clamp(x0 + half_width + 2, x0 + LOW_ISLE_CELL.0 - half_width - 3);
        let top_alt =
            (cluster_alt + rng.range(-22, 22)).clamp(alt_bottom + depth * 2 + 10, alt_top - 6);
        let land = SkyLand {
            id: feature_id(
                Layer::Sky,
                rx,
                ID_LOW * 1000 + ry,
                u16::try_from(local).unwrap_or(0),
            ),
            kind: SkyLandKind::Isle,
            biome: biome_for(bands, top_alt, rng.next_u64()),
            center_x,
            top_row: row_of(top_alt),
            half_width,
            depth,
            shape: rng.next_u64(),
            content: None,
            plan: None,
        };
        if lands
            .iter()
            .all(|other| !other.bounds().expanded(2).intersects(&land.bounds()))
        {
            lands.push(land);
        }
    }
    lands
}

/// The archipelago isles of cell `(rx, ry)`: a chain of two to four medium islands stepping
/// across the cell.
fn archipelago_cell(seed: u64, bands: &SkyBands, rx: i32, ry: i32) -> Vec<SkyLand> {
    let mut rng = RegionRng::new(seed, SALT_ARCHIPELAGO, rx, ry);
    if !rng.chance(720) {
        return Vec::new();
    }
    let (alt_bottom, alt_top) = cell_altitudes(bands, ID_ARCHIPELAGO, ry, ARCHIPELAGO_CELL.1);
    let x0 = rx * ARCHIPELAGO_CELL.0;
    let x_end = x0 + ARCHIPELAGO_CELL.0 - 4;
    let count = rng.range(2, 4);
    let mut lands = Vec::new();
    let mut cursor = x0 + rng.range(6, 40);
    let mut alt = rng.range(alt_bottom + 70, alt_top - 30);
    let step = if rng.chance(500) { 1 } else { -1 };
    for local in 0..count {
        let half_width = rng.range(10, 26);
        let depth = rng.range(6, 12) + half_width / 3;
        let center_x = cursor + half_width;
        if center_x + half_width >= x_end {
            break;
        }
        let top_alt = alt.clamp(alt_bottom + depth * 2 + 12, alt_top - 8);
        lands.push(SkyLand {
            id: feature_id(
                Layer::Sky,
                rx,
                ID_ARCHIPELAGO * 1000 + ry,
                u16::try_from(local).unwrap_or(0),
            ),
            kind: SkyLandKind::ArchipelagoIsle,
            biome: biome_for(bands, top_alt, rng.next_u64()),
            center_x,
            top_row: row_of(top_alt),
            half_width,
            depth,
            shape: rng.next_u64(),
            content: None,
            plan: None,
        });
        cursor = center_x + half_width + rng.range(8, 26);
        alt += step * rng.range(6, 20);
    }
    lands
}

/// Island cells generated per process (a pure function of the key, so evictions only cost
/// time): every chunk, environment sample and feature query of the island bands reads them.
static CELLS: MemoCache<Vec<SkyLand>> = MemoCache::new(1_024);

/// The islands of cell `(rx, ry)` of band `id_band` (low isles or the archipelago), with their
/// V9 content and town isle on generators that have them.
fn cell_lands(
    settings: &WorldSettings,
    id_band: i32,
    rx: i32,
    ry: i32,
) -> std::sync::Arc<Vec<SkyLand>> {
    CELLS.get_or_compute((*settings, rx, id_band * 1_000 + ry), || {
        let seed = settings.seed;
        let rich = settings.generator.has_sky_isles();
        let Some(bands) = bands_of(settings) else {
            return Vec::new();
        };
        let bands = &bands;
        match id_band {
            ID_LOW => {
                let lands = low_isle_cell(seed, bands, rx, ry);
                if rich {
                    isles::furnish_low_cell(seed, rx, ry, lands)
                } else {
                    lands
                }
            }
            _ => {
                if rich && towns::is_town_cell(seed, rx, ry) {
                    isles::furnish_archipelago_cell(towns::town_cell(seed, rx, ry))
                } else {
                    let lands = archipelago_cell(seed, bands, rx, ry);
                    if rich {
                        isles::furnish_archipelago_cell(lands)
                    } else {
                        lands
                    }
                }
            }
        }
    })
}

/// The continent cell of a continent's feature id.
fn continent_cell_of(id: FeatureId) -> Option<i32> {
    let (layer, rx, ry, _) = super::streamed::index::decode(id)?;
    (layer == Layer::Sky && ry.div_euclid(1000) == ID_CONTINENT).then_some(rx)
}

/// The continent footprint of cell `rx`, if it has one (with its content plan on V9 worlds).
fn continent_cell(settings: &WorldSettings, rx: i32) -> Option<SkyLand> {
    let bands = &bands_of(settings)?;
    let mut rng = RegionRng::new(settings.seed, SALT_CONTINENT, rx, 0);
    if !rng.chance(520) {
        return None;
    }
    let (alt_bottom, alt_top) = band_rows(bands, ID_CONTINENT);
    let x0 = rx * CONTINENT_CELL_COLUMNS;
    let half_width = rng.range(160, 340);
    let depth = rng.range(50, 100).min((alt_top - alt_bottom) / 2);
    let center_x = rng.range(
        x0 + half_width + 20,
        x0 + CONTINENT_CELL_COLUMNS - half_width - 20,
    );
    let top_alt = rng.range(alt_bottom + depth + 16, alt_top - 20);
    let biome = if rng.chance(500) {
        SkyBiome::Meadow
    } else {
        SkyBiome::Frost
    };
    let shape = rng.next_u64();
    let top_row = row_of(top_alt);
    let mut land = SkyLand {
        id: feature_id(Layer::Sky, rx, ID_CONTINENT * 1000, 0),
        kind: SkyLandKind::Continent,
        biome,
        center_x,
        top_row,
        half_width,
        depth,
        shape,
        plan: settings
            .generator
            .has_sky_content()
            .then(|| continent::ContinentPlan::new(center_x, half_width, top_row, depth, shape)),
        content: None,
    };
    // Nests go only on summits whose ledge really is the surface (needs the finished shape).
    if let Some(mut plan) = land.plan {
        plan.nests = continent::ledge_nests(&land, &plan);
        land.plan = Some(plan);
    }
    Some(land)
}

/// The Moon of a world with a sky (`None` on older generators).
///
/// One giant landmass about 2,000-2,900 tiles wide whose underside's lowest point sits at the world's
/// [`SkyBands::moon`] ([`MOON_BOTTOM_ROWS`] in V8), hanging above [`moon_column`].
#[must_use]
pub fn moon(settings: &WorldSettings) -> Option<SkyLand> {
    let bands = bands_of(settings)?;
    let mut rng = RegionRng::new(settings.seed, SALT_MOON, 0, 0);
    let side = if rng.chance(500) { 1 } else { -1 };
    let distance = rng.range(3_200, 4_800);
    let half_width = rng.range(1_000, 1_450);
    Some(SkyLand {
        id: feature_id(Layer::Sky, 0, ID_MOON * 1000, 0),
        kind: SkyLandKind::Moon,
        biome: SkyBiome::Frost,
        center_x: TOWN_CENTER_COLUMN + side * distance,
        top_row: row_of(bands.moon + MOON_DEPTH_ROWS),
        half_width,
        depth: MOON_DEPTH_ROWS,
        shape: rng.next_u64(),
        content: None,
        plan: None,
    })
}

/// The column the Moon hangs above (`None` on generators without a sky).
#[must_use]
pub fn moon_column(settings: &WorldSettings) -> Option<i32> {
    moon(settings).map(|moon| moon.center_x)
}

/// The cell indices along one axis covering `first..=last` with cells `size` long.
const fn cells(first: i32, last: i32, size: i32) -> std::ops::RangeInclusive<i32> {
    first.div_euclid(size)..=last.div_euclid(size)
}

/// Cell rows `0..rows` of a band whose cells (`cell_rows` tall) can reach tile rows
/// `rect.min.y..=rect.max.y`.
fn band_cells(
    bands: &SkyBands,
    id_band: i32,
    cell_rows: i32,
    rect: &TileRect,
) -> std::ops::Range<i32> {
    let (alt_low, alt_high) = (-rect.max.y, -rect.min.y);
    let (bottom, _) = band_rows(bands, id_band);
    let rows = band_cell_rows(bands, id_band, cell_rows);
    // One cell row of margin below: a V9 island's props may poke a row above its cell (lands
    // are kept by their bounds, so V8 results are unchanged).
    let first = ((alt_low - bottom).div_euclid(cell_rows) - 1).clamp(0, rows);
    let last = ((alt_high - bottom).div_euclid(cell_rows) + 1).clamp(0, rows);
    first..last
}

/// Every sky land (islands, continents, the Moon) whose bounds overlap `rect`, in id order.
/// Empty on generators without a sky.
#[must_use]
pub fn lands_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<SkyLand> {
    let mut found = Vec::new();
    let Some(bands) = bands_of(settings) else {
        return found;
    };
    if rect.min.y > row_of(bands.low_isles) {
        return found;
    }
    let bands = &bands;
    for ry in band_cells(bands, ID_LOW, LOW_ISLE_CELL.1, rect) {
        for rx in cells(rect.min.x, rect.max.x, LOW_ISLE_CELL.0) {
            found.extend(cell_lands(settings, ID_LOW, rx, ry).iter().copied());
        }
    }
    for ry in band_cells(bands, ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1, rect) {
        for rx in cells(rect.min.x, rect.max.x, ARCHIPELAGO_CELL.0) {
            found.extend(cell_lands(settings, ID_ARCHIPELAGO, rx, ry).iter().copied());
        }
    }
    let (continent_bottom, continent_top) = band_rows(bands, ID_CONTINENT);
    if -rect.max.y <= continent_top && -rect.min.y >= continent_bottom {
        for rx in cells(rect.min.x, rect.max.x, CONTINENT_CELL_COLUMNS) {
            found.extend(continent_cell(settings, rx));
        }
    }
    found.extend(moon(settings));
    found.retain(|land| land.bounds().intersects(rect));
    found.sort_by_key(|land| land.id);
    found
}

/// The sky land with feature id `id`, if this world has it.
#[must_use]
pub fn land(settings: &WorldSettings, id: FeatureId) -> Option<SkyLand> {
    let (layer, rx, ry, local) = super::streamed::index::decode(id)?;
    if layer != Layer::Sky {
        return None;
    }
    let bands = &bands_of(settings)?;
    let (band, row) = (ry.div_euclid(1000), ry.rem_euclid(1000));
    let lands = match band {
        ID_LOW | ID_ARCHIPELAGO => cell_lands(settings, band, rx, row).to_vec(),
        ID_CONTINENT => continent_cell(settings, rx).into_iter().collect(),
        ID_MOON => moon(settings).into_iter().collect(),
        _ => Vec::new(),
    };
    let _ = local;
    lands.into_iter().find(|land| land.id == id)
}

/// The sky-land tile at `position` (`None` where the sky is open air, and on generators
/// without a sky).
#[must_use]
pub fn tile_at(settings: &WorldSettings, position: TilePosition) -> Option<TileKind> {
    let rect = TileRect {
        min: position,
        max: position,
    };
    lands_touching(settings, &rect)
        .iter()
        .find_map(|land| land.tile_at(position.x, position.y))
}

/// Whether `rect` touches no sky land (its tiles are all air).
#[must_use]
pub fn chunk_is_empty(settings: &WorldSettings, rect: &TileRect) -> bool {
    lands_touching(settings, rect).is_empty()
}

/// Every island, continent and the Moon as features whose centre lies in `rect` (unsorted).
pub(crate) fn layer_features(
    settings: &WorldSettings,
    rect: &TileRect,
    out: &mut Vec<WorldFeature>,
) {
    // A land's centre is its top row at its middle column: gather every land whose bounds
    // reach the rect (a superset), then the caller keeps the centres inside.
    out.extend(
        lands_touching(settings, rect)
            .iter()
            .map(SkyLand::feature)
            .filter(|feature| rect.contains(feature.center)),
    );
}

// ---------------------------------------------------------------------------------------------
// Updrafts.
// ---------------------------------------------------------------------------------------------

/// Updraft cells: `(columns, rows)`.
pub const UPDRAFT_CELL: (i32, i32) = (144, 192);
/// The lowest altitude an updraft cell starts at.
pub const UPDRAFT_FLOOR_ROWS: i32 = 140;

/// A column of rising air: inside `rect` a rig (or a drop, or a bomb) is pushed up by
/// `strength` px/s², tapering off over the top rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Updraft {
    pub rect: TileRect,
    /// Upward acceleration at full strength, px/s² (above the rig's gravity of 440: a free
    /// ride up).
    pub strength: i32,
}

/// Rows over which an updraft fades out at its top.
pub const UPDRAFT_TAPER_ROWS: i32 = 16;

impl Updraft {
    /// The upward acceleration (px/s²) at tile `(x, y)` (0 outside).
    #[must_use]
    pub const fn acceleration_at(&self, x: i32, y: i32) -> i32 {
        if !self.rect.contains(TilePosition { x, y }) {
            return 0;
        }
        let from_top = y - self.rect.min.y;
        if from_top < UPDRAFT_TAPER_ROWS {
            self.strength * (from_top + 1) / (UPDRAFT_TAPER_ROWS + 1)
        } else {
            self.strength
        }
    }
}

/// Cell rows of the updraft grid (from [`UPDRAFT_FLOOR_ROWS`] to the orbit band).
const fn updraft_cell_rows(bands: &SkyBands) -> i32 {
    (bands.orbit - UPDRAFT_FLOOR_ROWS) / UPDRAFT_CELL.1
}

/// The updraft of cell `(rx, ry)`, if it has one (none in the dead-air gap).
fn updraft_cell(seed: u64, bands: &SkyBands, rx: i32, ry: i32) -> Option<Updraft> {
    let alt_bottom = UPDRAFT_FLOOR_ROWS + ry * UPDRAFT_CELL.1;
    let alt_top = alt_bottom + UPDRAFT_CELL.1 - 1;
    let band = bands.band_at(alt_bottom);
    if matches!(band, SkyBand::DeadAir | SkyBand::Orbit | SkyBand::Lunar)
        || bands.band_at(alt_top) == SkyBand::DeadAir
    {
        return None;
    }
    let mut rng = RegionRng::new(seed, SALT_UPDRAFT, rx, ry);
    let per_mille = if band == SkyBand::Archipelago {
        520
    } else {
        400
    };
    if !rng.chance(per_mille) {
        return None;
    }
    let width = rng.range(4, 9);
    let height = rng.range(80, 170);
    let x0 = rx * UPDRAFT_CELL.0 + rng.range(4, UPDRAFT_CELL.0 - width - 4);
    let bottom_alt = rng.range(alt_bottom, alt_top - height);
    Some(Updraft {
        rect: TileRect {
            min: TilePosition {
                x: x0,
                y: row_of(bottom_alt + height),
            },
            max: TilePosition {
                x: x0 + width - 1,
                y: row_of(bottom_alt),
            },
        },
        strength: rng.range(560, 900),
    })
}

/// Every updraft overlapping `rect` (empty on generators without a sky).
#[must_use]
pub fn updrafts_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<Updraft> {
    let mut found = Vec::new();
    let Some(bands) = bands_of(settings) else {
        return found;
    };
    let (alt_low, alt_high) = (-rect.max.y, -rect.min.y);
    let rows = updraft_cell_rows(&bands);
    let first = (alt_low - UPDRAFT_FLOOR_ROWS)
        .div_euclid(UPDRAFT_CELL.1)
        .clamp(0, rows);
    let last = ((alt_high - UPDRAFT_FLOOR_ROWS).div_euclid(UPDRAFT_CELL.1) + 1).clamp(0, rows);
    for ry in first..last {
        for rx in cells(rect.min.x, rect.max.x, UPDRAFT_CELL.0) {
            if let Some(updraft) = updraft_cell(settings.seed, &bands, rx, ry)
                && updraft.rect.intersects(rect)
            {
                found.push(updraft);
            }
        }
    }
    found
}

/// The updraft acceleration (px/s², upward) at tile `(x, y)`: cheap enough to call per body
/// per tick (one cell hash).
#[must_use]
pub fn updraft_at(settings: &WorldSettings, x: i32, y: i32) -> i32 {
    let Some(bands) = bands_of(settings) else {
        return 0;
    };
    let altitude = -y;
    if altitude < UPDRAFT_FLOOR_ROWS {
        return 0;
    }
    let ry = (altitude - UPDRAFT_FLOOR_ROWS).div_euclid(UPDRAFT_CELL.1);
    if ry >= updraft_cell_rows(&bands) {
        return 0;
    }
    updraft_cell(settings.seed, &bands, x.div_euclid(UPDRAFT_CELL.0), ry)
        .map_or(0, |updraft| updraft.acceleration_at(x, y))
}

pub mod continent;

#[cfg(test)]
mod isle_tests;
#[cfg(test)]
mod tests;
