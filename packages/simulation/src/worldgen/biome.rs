//! Surface biomes and the surface heightmap of [`GeneratorVersion::V7`] worlds (WP-SURFACE).
//!
//! The surface of a V7 world is cut into **biome spans** of about [`BIOME_SPAN_COLUMNS`]
//! columns (jittered boundaries). A slow climate wave picks each span's biome (cold spans lean
//! to tundra and mountains, hot ones to desert and badlands, temperate ones to plains, forest,
//! wetlands and mountains), so neighbouring biomes make sense together. Each biome shapes its
//! own height profile:
//!
//! | Biome | Shape | Ground |
//! |---|---|---|
//! | [`Biome::Plains`] | gentle swells | grass on dirt |
//! | [`Biome::Forest`] | rolling hills | dirt under dense trees |
//! | [`Biome::Desert`] | dunes | sand over sandstone |
//! | [`Biome::Badlands`] | terraced mesas with cliffs | striped red rock |
//! | [`Biome::Tundra`] | low frozen hills | snow over dirt |
//! | [`Biome::Mountains`] | ridged peaks up to [`MAX_ELEVATION_ROWS`] high, ledges and cliffs, caves into the rock, mountain ore | granite, snow caps |
//! | [`Biome::Wetlands`] | basins below the lake line, decorative water ([`lake_row`]) | mud |
//!
//! Heights blend smoothly across span boundaries (over [`BIOME_BLEND_COLUMNS`] either side), and
//! everything is a pure integer function of the settings and the column, so any column (and so
//! any chunk) is evaluated on its own: chunk borders are seamless and the result never depends
//! on what was generated first.
//!
//! **Flat ground.** The town's columns (see [`super::streamed::TOWN_FLAT_FIRST_COLUMN`]) stay
//! flat at the classic surface row, and so do the **flat sites** ([`flat_sites_in`]): the
//! surface village sites ([`crate::village::placement`]), each a flat shelf as wide as its
//! village whose ground row is [`FlatSite::row`]. The heightmap eases into every site, so
//! villages (WP-VILLAGE) always stand on level ground.
//!
//! Vegetation ([`flora_at`]) and lakes are data for presentation only: deterministic, but no
//! gameplay reads them.
//!
//! [`GeneratorVersion::V7`]: super::GeneratorVersion::V7

use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::{
    terrain::{MineralKind, SurfaceMaterial, TileKind},
    village::placement,
};

use super::{
    WorldSettings,
    noise::{NOISE_MAX, centred, hash2, lattice, line_noise, value_noise},
    strata::SKY_ROWS,
    streamed::{TOWN_FLAT_FIRST_COLUMN, TOWN_FLAT_LAST_COLUMN},
};

/// Average width of one biome span, in columns.
pub const BIOME_SPAN_COLUMNS: i32 = 360;
/// How far a span boundary is moved from the regular grid, at most.
const SPAN_JITTER: i32 = 90;
/// Where the span grid starts: span 0 is centred on the town.
const SPAN_OFFSET: i32 = -60;
/// Columns either side of a span boundary over which two biomes' heights blend.
pub const BIOME_BLEND_COLUMNS: i32 = 48;
/// Highest a surface column rises above the classic surface row (mountain peaks).
pub const MAX_ELEVATION_ROWS: i32 = 80;
/// Lowest a surface column sinks below the classic surface row (lake beds).
pub const MAX_DIP_ROWS: i32 = 7;
/// Rows above row 0 a V7 world keeps for its mountains (its first row is `-SKY_HEADROOM_ROWS`;
/// above that is open sky, which rigs may still fly through without limit).
pub const SKY_HEADROOM_ROWS: i32 = 96;
/// The top row of decorative lake water: wherever the ground lies below it, the rows from it
/// down to the ground hold water ([`lake_row`]).
pub const LAKE_ROW: i32 = SKY_ROWS + 1;
/// Columns over which the heightmap eases into a site's (or the town's) flat ground.
pub const FLAT_FADE_COLUMNS: i32 = 28;
/// Columns over which the heightmap eases into the town's flat ground.
const TOWN_FADE_COLUMNS: i32 = 40;
/// Highest a site's flat ground sits above the classic surface (mountain shelves).
const MAX_SITE_ELEVATION_ROWS: i32 = 24;
/// Elevation (rows above the classic surface) above which mountain slopes are bare rock.
const TREELINE_ROWS: i32 = 22;
/// Elevation above which mountain slopes are snow-capped (give or take a few rows).
const SNOWLINE_ROWS: i32 = 36;

const SALT_SPAN: u64 = 0xB10_0001;
const SALT_CLIMATE: u64 = 0xB10_0002;
const SALT_PICK: u64 = 0xB10_0003;
const SALT_HEIGHT: u64 = 0xB10_0010;
const SALT_EDGE: u64 = 0xB10_0004;
const SALT_MATERIAL: u64 = 0xB10_0020;
const SALT_ORE: u64 = 0xB10_0030;
const SALT_CAVE: u64 = 0xB10_0040;
const SALT_FLORA: u64 = 0xB10_0050;

/// Fixed-point unit of heights: one row.
const ONE: i64 = 1024;

/// A surface biome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Badlands,
    Tundra,
    Mountains,
    Wetlands,
}

impl Biome {
    /// Every biome.
    pub const ALL: [Self; 7] = [
        Self::Plains,
        Self::Forest,
        Self::Desert,
        Self::Badlands,
        Self::Tundra,
        Self::Mountains,
        Self::Wetlands,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plains => "Plains",
            Self::Forest => "Forest",
            Self::Desert => "Desert",
            Self::Badlands => "Badlands",
            Self::Tundra => "Tundra",
            Self::Mountains => "Mountains",
            Self::Wetlands => "Wetlands",
        }
    }

    /// Whether the biome's ground is cold enough for snow.
    #[must_use]
    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Tundra | Self::Mountains)
    }

    /// Whether the biome is dry (dust instead of rain).
    #[must_use]
    pub const fn is_dry(self) -> bool {
        matches!(self, Self::Desert | Self::Badlands)
    }
}

/// The biomes shaping one column: its own span's biome and, near a span boundary, the
/// neighbour it blends with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BiomeBlend {
    /// The biome of the column's span.
    pub primary: Biome,
    /// The neighbouring span's biome (the primary itself away from boundaries).
    pub secondary: Biome,
    /// Weight of `secondary`, `0..=128` (128 exactly on a boundary).
    pub mix: u8,
}

impl BiomeBlend {
    /// The biome that dominates the column (the primary unless the blend is past half).
    #[must_use]
    pub const fn dominant(self) -> Biome {
        if self.mix > 64 {
            self.secondary
        } else {
            self.primary
        }
    }
}

/// A flat settlement site ([`flat_sites_in`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct FlatSite {
    /// The site's middle column (its id: no two sites share one).
    pub center: i32,
    /// First and last column of its flat ground.
    pub first_column: i32,
    pub last_column: i32,
    /// The first ground row across the flat ground.
    pub row: i32,
    /// The biome around it.
    pub biome: Biome,
}

impl FlatSite {
    /// Whether column `x` is on the site's flat ground.
    #[must_use]
    pub const fn contains_column(&self, x: i32) -> bool {
        x >= self.first_column && x <= self.last_column
    }
}

/// Vegetation and surface props standing on a column's ground (presentation data).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flora {
    GrassTuft,
    Flowers,
    Bush,
    Oak,
    Pine,
    SnowPine,
    Cactus,
    DeadBush,
    Boulder,
    Reeds,
    LilyPad,
}

/// One column's flora: what stands there and a seed for its look (size, colour, lean).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloraSpot {
    pub flora: Flora,
    pub variant: u32,
}

// ---------------------------------------------------------------------------------------------
// Spans and climate.

/// The first column of biome span `span`.
const fn span_start(seed: u64, span: i32) -> i32 {
    span * BIOME_SPAN_COLUMNS
        + SPAN_OFFSET
        + centred(lattice(seed, SALT_SPAN, span, 0), SPAN_JITTER)
}

/// The span holding column `x`, with its first column and one past its last.
const fn span_of(seed: u64, x: i32) -> (i32, i32, i32) {
    let mut span = (x - SPAN_OFFSET).div_euclid(BIOME_SPAN_COLUMNS);
    if x < span_start(seed, span) {
        span -= 1;
    } else if x >= span_start(seed, span + 1) {
        span += 1;
    }
    (span, span_start(seed, span), span_start(seed, span + 1))
}

/// Spans in one climate cycle (cold, temperate, hot, temperate, back to cold).
pub const CLIMATE_CYCLE_SPANS: i32 = 12;

/// The climate zone of each position of a cycle: 0 cold, 1 temperate, 2 hot.
const CYCLE_ZONES: [u8; CLIMATE_CYCLE_SPANS as usize] = [0, 0, 1, 1, 1, 2, 2, 2, 1, 1, 1, 0];

/// The biome of span `span`: the town's span is plains with a forest to its left and a mild
/// biome to its right. Further out, a climate cycle of [`CLIMATE_CYCLE_SPANS`] spans (its phase
/// set by the seed) passes through cold, temperate and hot zones; each zone of each cycle
/// deals its spans from a shuffled deck holding every biome of that climate at least once, so
/// every biome appears in every cycle (about 4,300 columns) and the mix still differs from
/// cycle to cycle.
fn span_biome(seed: u64, span: i32) -> Biome {
    match span {
        0 => return Biome::Plains,
        -1 => return Biome::Forest,
        1 => {
            return match hash2(seed, SALT_PICK, span, 0) % 100 {
                0..40 => Biome::Plains,
                40..75 => Biome::Forest,
                _ => Biome::Wetlands,
            };
        }
        _ => {}
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "the phase is below the cycle length"
    )]
    let phase = (hash2(seed, SALT_CLIMATE, 0, 0) % CLIMATE_CYCLE_SPANS as u64) as i32;
    let shifted = span + phase;
    let position = shifted.rem_euclid(CLIMATE_CYCLE_SPANS);
    // A cold zone straddles two cycles: its first span (the last of a cycle) deals from the
    // next cycle's deck.
    let cycle =
        shifted.div_euclid(CLIMATE_CYCLE_SPANS) + i32::from(position == CLIMATE_CYCLE_SPANS - 1);
    let zone = CYCLE_ZONES[usize::try_from(position).unwrap_or(0)];
    // The span's slot among its zone's spans in this cycle (cold wraps around the cycle's
    // ends, so its spans count from position 11).
    let slot = (0..position)
        .filter(|earlier| CYCLE_ZONES[usize::try_from(*earlier).unwrap_or(0)] == zone)
        .count();
    let slot = if zone == 0 {
        if position == CLIMATE_CYCLE_SPANS - 1 {
            0
        } else {
            slot + 1
        }
    } else {
        slot
    };
    let pick = |salt: u64, options: &[Biome]| {
        let index = hash2(seed, SALT_PICK + salt, cycle, i32::from(zone)) % options.len() as u64;
        options[usize::try_from(index).unwrap_or(0)]
    };
    let mut deck: Vec<Biome> = match zone {
        0 => vec![
            Biome::Tundra,
            Biome::Mountains,
            pick(1, &[Biome::Tundra, Biome::Mountains, Biome::Forest]),
        ],
        2 => vec![
            Biome::Desert,
            Biome::Badlands,
            pick(
                1,
                &[
                    Biome::Desert,
                    Biome::Badlands,
                    Biome::Mountains,
                    Biome::Plains,
                ],
            ),
        ],
        _ => vec![
            Biome::Plains,
            Biome::Forest,
            Biome::Wetlands,
            Biome::Mountains,
            pick(1, &[Biome::Plains, Biome::Forest, Biome::Wetlands]),
            pick(
                2,
                &[
                    Biome::Forest,
                    Biome::Mountains,
                    Biome::Desert,
                    Biome::Plains,
                ],
            ),
        ],
    };
    // Fisher-Yates with the cycle's own hashes: the same shuffle for every span of the zone.
    for index in (1..deck.len()).rev() {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "deck indices are tiny"
        )]
        let other = (hash2(
            seed,
            SALT_PICK + 8,
            cycle * 16 + i32::from(zone),
            index as i32,
        ) % (index as u64 + 1)) as usize;
        deck.swap(index, other);
    }
    deck[slot % deck.len()]
}

/// Smooth noise along x scaled to `0..=ONE`.
fn wave(seed: u64, salt: u64, x: i32, cell: i32) -> i64 {
    i64::from(line_noise(seed, salt, x, cell)) * ONE / i64::from(NOISE_MAX)
}

/// `ONE` where `value` (`0..=ONE`) is at its middle, falling to 0 at the extremes.
const fn ridge01(value: i64) -> i64 {
    ONE - (2 * value - ONE).abs()
}

/// Smoothstep of `t` in `0..=ONE`.
const fn smoothstep(t: i64) -> i64 {
    let t = if t < 0 {
        0
    } else if t > ONE {
        ONE
    } else {
        t
    };
    t * t * (3 * ONE - 2 * t) / (ONE * ONE)
}

/// Elevation (fixed-point rows above the classic surface) biome `biome` gives column `x` of a
/// span from `start` to `end` (exclusive).
fn biome_elevation(seed: u64, biome: Biome, x: i32, start: i32, end: i32) -> i64 {
    let salt = SALT_HEIGHT + biome as u64 * 16;
    match biome {
        Biome::Plains => -ONE + 4 * wave(seed, salt, x, 48) + wave(seed, salt + 1, x, 13),
        Biome::Forest => -ONE + 9 * wave(seed, salt, x, 64) + 2 * wave(seed, salt + 1, x, 17),
        Biome::Desert => {
            let dune = wave(seed, salt, x, 34);
            -ONE + 8 * dune * dune / ONE + wave(seed, salt + 1, x, 9)
        }
        Biome::Badlands => {
            let mass = wave(seed, salt, x, 80);
            let base = 2 * ONE + 26 * mass * mass / ONE + wave(seed, salt + 1, x, 23) * 3;
            let step = 5 * ONE;
            let terrace = base.div_euclid(step) * step;
            terrace + (base - terrace) / 6 + wave(seed, salt + 2, x, 6) / 2
        }
        Biome::Tundra => -ONE + 5 * wave(seed, salt, x, 56) + wave(seed, salt + 1, x, 11),
        Biome::Mountains => {
            let edge = i64::from((x - start).min(end - 1 - x).max(0));
            let envelope = smoothstep(edge * ONE / 120);
            let broad = ridge01(wave(seed, salt, x, 84));
            let sharp = ridge01(wave(seed, salt + 1, x, 27));
            let shape = (broad * broad / ONE * 7 + sharp * 3) / 10;
            let peak = 8 * ONE + 72 * shape;
            let height = envelope * peak / ONE;
            // Ledges: part of the height snaps to steps, leaving short cliffs between them.
            let step = 6 * ONE;
            let ledge = height.div_euclid(step) * step;
            (height * 3 + ledge * 2) / 5 + wave(seed, salt + 2, x, 9) * 2
        }
        Biome::Wetlands => {
            let basin = (wave(seed, salt, x, 44) - ONE * 3 / 10).max(0) * 10 / 7;
            ONE / 2 - 7 * basin + wave(seed, salt + 1, x, 7) / 2
        }
    }
}

/// The span's own elevation at `x`, blended with the neighbouring span's near a boundary, and
/// the blend (before any flattening).
fn raw_elevation(seed: u64, x: i32) -> (i64, BiomeBlend) {
    let (span, start, end) = span_of(seed, x);
    let primary = span_biome(seed, span);
    let own = biome_elevation(seed, primary, x, start, end);
    let (neighbour, distance) = if x - start < BIOME_BLEND_COLUMNS {
        (span - 1, x - start)
    } else if end - 1 - x < BIOME_BLEND_COLUMNS {
        (span + 1, end - 1 - x)
    } else {
        return (
            own,
            BiomeBlend {
                primary,
                secondary: primary,
                mix: 0,
            },
        );
    };
    let secondary = span_biome(seed, neighbour);
    let (n_start, n_end) = if neighbour < span {
        (span_start(seed, neighbour), start)
    } else {
        (end, span_start(seed, neighbour + 1))
    };
    let other = biome_elevation(seed, secondary, x, n_start, n_end);
    // Half and half on the boundary, all own at the blend's far edge.
    let t = i64::from(distance + BIOME_BLEND_COLUMNS) * ONE / i64::from(2 * BIOME_BLEND_COLUMNS);
    let weight = smoothstep(t);
    let elevation = (own * weight + other * (ONE - weight)) / ONE;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the secondary weight is at most half of ONE, scaled to 0..=128"
    )]
    let mix = ((ONE - weight) * 256 / ONE).clamp(0, 128) as u8;
    (
        elevation,
        BiomeBlend {
            primary,
            secondary,
            mix,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Flat sites.

/// The flat site of surface village site `site` ([`placement::surface_site`]), if it exists.
///
/// The site's flat ground is the village's span; its row is the raw biome elevation at the
/// village's centre, clamped so lake sites sit on a raised bank and mountain sites on a shelf
/// partway up.
fn village_flat_site(seed: u64, site: i32) -> Option<FlatSite> {
    let village = placement::surface_site(seed, site)?;
    let (elevation, blend) = raw_elevation(seed, village.center);
    let rows = i32::try_from(elevation.div_euclid(ONE)).unwrap_or(0);
    let rows = rows.clamp(0, MAX_SITE_ELEVATION_ROWS);
    Some(FlatSite {
        center: village.center,
        first_column: village.first,
        last_column: village.last,
        row: SKY_ROWS - rows,
        biome: blend.dominant(),
    })
}

/// Every flat settlement site whose flat ground overlaps columns `first_column..=last_column`,
/// left to right. Empty for worlds without biomes.
///
/// **The village hook (WP-VILLAGE):** the sites are the surface villages' sites
/// ([`placement::surface_site`]), so every village stands on flat ground (the heightmap eases
/// into it over [`FLAT_FADE_COLUMNS`]) whose row is [`FlatSite::row`] and whose biome picks
/// the village's landscape ([`crate::village::layout::site_landscape`]).
#[must_use]
pub fn flat_sites_in(
    settings: &WorldSettings,
    first_column: i32,
    last_column: i32,
) -> Vec<FlatSite> {
    if !settings.generator.has_biomes() || last_column < first_column {
        return Vec::new();
    }
    let (first_site, last_site) = placement::sites_between(first_column, last_column);
    (first_site..=last_site)
        .filter_map(|site| village_flat_site(settings.seed, site))
        .filter(|site| site.last_column >= first_column && site.first_column <= last_column)
        .collect()
}

/// The flat site whose flat ground holds column `x`, if any.
#[must_use]
pub fn flat_site_at(settings: &WorldSettings, x: i32) -> Option<FlatSite> {
    flat_sites_in(settings, x, x).into_iter().next()
}

/// The flat site nearest column `x` (by its centre; ties: the left one), searching at most
/// `max_columns` either side.
#[must_use]
pub fn nearest_flat_site(settings: &WorldSettings, x: i32, max_columns: i32) -> Option<FlatSite> {
    flat_sites_in(settings, x - max_columns, x + max_columns)
        .into_iter()
        .min_by_key(|site| ((site.center - x).abs(), site.center))
}

/// Weight (`0..=ONE`) of flat ground at `x` for flat ground spanning `first..=last` that the
/// heightmap fades into over `fade` columns.
fn flat_weight(x: i32, first: i32, last: i32, fade: i32) -> i64 {
    let distance = if x < first {
        first - x
    } else if x > last {
        x - last
    } else {
        return ONE;
    };
    if distance >= fade {
        return 0;
    }
    ONE - smoothstep(i64::from(distance) * ONE / i64::from(fade))
}

// ---------------------------------------------------------------------------------------------
// Columns.

/// Everything generation and presentation read about one surface column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Column {
    /// The first ground row.
    pub surface_row: i32,
    pub blend: BiomeBlend,
    /// The biome whose materials the column's ground uses (the blend's dominant biome, with a
    /// little jitter so biome edges interleave instead of meeting on a straight line).
    pub biome: Biome,
    /// Whether the column is on flat town or site ground.
    pub flat: bool,
}

/// Computes column `x` of a biome world (no cache).
fn compute_column(seed: u64, x: i32) -> Column {
    let (mut elevation, blend) = raw_elevation(seed, x);
    let mut flat_weight_max = flat_weight(
        x,
        TOWN_FLAT_FIRST_COLUMN,
        TOWN_FLAT_LAST_COLUMN,
        TOWN_FADE_COLUMNS,
    );
    elevation = elevation * (ONE - flat_weight_max) / ONE;
    let (first_site, last_site) =
        placement::sites_between(x - FLAT_FADE_COLUMNS, x + FLAT_FADE_COLUMNS);
    for site in first_site..=last_site {
        if let Some(site) = village_flat_site(seed, site) {
            let weight = flat_weight(x, site.first_column, site.last_column, FLAT_FADE_COLUMNS);
            if weight > 0 {
                let target = i64::from(SKY_ROWS - site.row) * ONE;
                elevation = (elevation * (ONE - weight) + target * weight) / ONE;
                flat_weight_max = flat_weight_max.max(weight);
            }
        }
    }
    let rows = i32::try_from((elevation + ONE / 2).div_euclid(ONE)).unwrap_or(0);
    let surface_row = SKY_ROWS - rows.clamp(-MAX_DIP_ROWS, MAX_ELEVATION_ROWS);
    let jitter = centred(line_noise(seed, SALT_EDGE, x, 5), 40);
    let biome = if i32::from(blend.mix) + jitter > 64 {
        blend.secondary
    } else {
        blend.primary
    };
    Column {
        surface_row,
        blend,
        biome,
        flat: flat_weight_max >= ONE,
    }
}

const COLUMN_CACHE_SIZE: usize = 2_048;

type CachedColumn = Option<(WorldSettings, i32, Column)>;

thread_local! {
    /// Direct-mapped memo of recent columns (a cache: a miss only costs time).
    static COLUMNS: RefCell<Vec<CachedColumn>> = RefCell::new(vec![None; COLUMN_CACHE_SIZE]);
}

/// Column `x` of a biome world (memoized per thread).
#[must_use]
pub fn column(settings: &WorldSettings, x: i32) -> Column {
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "the slot is the low bits of the column"
    )]
    let slot = (x as u32 as usize) % COLUMN_CACHE_SIZE;
    COLUMNS.with(|cache| {
        if let Ok(cache) = cache.try_borrow()
            && let Some((cached_settings, cached_x, column)) = cache[slot]
            && cached_x == x
            && cached_settings == *settings
        {
            return column;
        }
        let column = compute_column(settings.seed, x);
        if let Ok(mut cache) = cache.try_borrow_mut() {
            cache[slot] = Some((*settings, x, column));
        }
        column
    })
}

/// The first ground row of column `x` of a biome world.
#[must_use]
pub fn surface_row(settings: &WorldSettings, x: i32) -> i32 {
    column(settings, x).surface_row
}

/// The biome of column `x` (plains for worlds without biomes).
#[must_use]
pub fn biome_at(settings: &WorldSettings, x: i32) -> Biome {
    if settings.generator.has_biomes() {
        column(settings, x).biome
    } else {
        Biome::Plains
    }
}

/// The biomes blending at column `x` (all plains for worlds without biomes).
#[must_use]
pub fn biome_blend(settings: &WorldSettings, x: i32) -> BiomeBlend {
    if settings.generator.has_biomes() {
        column(settings, x).blend
    } else {
        BiomeBlend {
            primary: Biome::Plains,
            secondary: Biome::Plains,
            mix: 0,
        }
    }
}

/// The top row of decorative lake water over column `x`, if any.
///
/// Water fills rows `LAKE_ROW..surface_row` wherever the generated ground lies below
/// [`LAKE_ROW`]. Presentation only: rigs pass through it. `None` for worlds without biomes.
#[must_use]
pub fn lake_row(settings: &WorldSettings, x: i32) -> Option<i32> {
    (settings.generator.has_biomes() && surface_row(settings, x) > LAKE_ROW).then_some(LAKE_ROW)
}

/// The row above which mountain slopes at column `x` wear snow.
const fn snowline_row(seed: u64, x: i32) -> i32 {
    SKY_ROWS - SNOWLINE_ROWS + centred(line_noise(seed, SALT_MATERIAL + 9, x, 23), 4)
}

// ---------------------------------------------------------------------------------------------
// Ground.

/// The biome ground at `(x, y)` of a biome world, ore included.
///
/// For a tile at or below the column's surface that generation would otherwise fill with its
/// band's rock: the biome crust near the surface, the mass of a hill, mesa or mountain above
/// the classic surface, and the shallow biome ore in it. `None` where the band's rock (and
/// the classic generation) takes over.
#[must_use]
pub fn ground_kind(settings: &WorldSettings, x: i32, y: i32) -> Option<TileKind> {
    // Below every biome's crust and ore (the deep rows skip the column lookup).
    if y > SKY_ROWS + MAX_DIP_ROWS + 14 {
        return None;
    }
    let column = column(settings, x);
    let depth = y - column.surface_row;
    if depth < 0 {
        return None;
    }
    if let Some(mineral) = biome_ore(settings.seed, column.biome, x, y, depth, y <= SKY_ROWS) {
        return Some(TileKind::Ore(mineral));
    }
    ground_material(settings, x, y)
}

/// The biome rock or soil at `(x, y)`, without its ore.
///
/// What [`ground_kind`] fills the tile with, or the host an ore pod there sits in (renderers
/// draw ore over it). `None` above the surface and where the band's rock takes over, and for
/// worlds without biomes.
#[must_use]
pub fn ground_material(settings: &WorldSettings, x: i32, y: i32) -> Option<TileKind> {
    // Below every biome's crust (renderers ask for every tile they paint).
    if !settings.generator.has_biomes() || y > SKY_ROWS + MAX_DIP_ROWS + 3 {
        return None;
    }
    let column = column(settings, x);
    let seed = settings.seed;
    let depth = y - column.surface_row;
    if depth < 0 {
        return None;
    }
    let elevated = y <= SKY_ROWS;
    let texture = value_noise(seed, SALT_MATERIAL, x, y, 7, 4);
    let surface = |material| Some(TileKind::Surface(material));
    match column.biome {
        Biome::Plains | Biome::Forest => {
            if depth < 3 {
                Some(TileKind::Dirt)
            } else if elevated {
                Some(if texture > NOISE_MAX * 7 / 10 {
                    TileKind::Clay
                } else {
                    TileKind::Dirt
                })
            } else {
                None
            }
        }
        Biome::Desert => {
            let sand = 2 + centred(line_noise(seed, SALT_MATERIAL + 1, x, 7), 1);
            if depth < sand + 1 {
                surface(SurfaceMaterial::Sand)
            } else if y < SKY_ROWS + 5 {
                surface(SurfaceMaterial::Sandstone)
            } else {
                None
            }
        }
        Biome::Badlands => {
            // Drifted sand only on the low ground between the mesas.
            if depth == 0 && y >= SKY_ROWS - 1 {
                return surface(SurfaceMaterial::Sand);
            }
            if y >= SKY_ROWS + 6 {
                return None;
            }
            let wobble = centred(line_noise(seed, SALT_MATERIAL + 3, x, 31), 2);
            match (y + wobble).rem_euclid(7) {
                3 => surface(SurfaceMaterial::Sandstone),
                6 => Some(TileKind::Clay),
                _ => surface(SurfaceMaterial::RedRock),
            }
        }
        Biome::Tundra => {
            if depth == 0 {
                surface(SurfaceMaterial::Snow)
            } else if depth < 3 || elevated {
                Some(TileKind::Dirt)
            } else {
                None
            }
        }
        Biome::Mountains => {
            let altitude = SKY_ROWS - y;
            if depth < 2 && y < snowline_row(seed, x) {
                surface(SurfaceMaterial::Snow)
            } else if depth < 2 && altitude < TREELINE_ROWS {
                Some(TileKind::Dirt)
            } else if y <= SKY_ROWS + 2 {
                if texture > NOISE_MAX * 74 / 100 {
                    Some(TileKind::Stone)
                } else {
                    surface(SurfaceMaterial::Granite)
                }
            } else {
                None
            }
        }
        Biome::Wetlands => {
            if depth < 3 {
                surface(SurfaceMaterial::Mud)
            } else {
                None
            }
        }
    }
}

/// Shallow biome ore: pods in mountain masses (richer higher up) and a light sprinkle of each
/// biome's own minerals in its crust.
const fn biome_ore(
    seed: u64,
    biome: Biome,
    x: i32,
    y: i32,
    depth: i32,
    elevated: bool,
) -> Option<MineralKind> {
    if depth < 1 || depth > 14 {
        return None;
    }
    let (pod_threshold, mask_threshold) = match (biome, elevated) {
        (Biome::Mountains, true) => (NOISE_MAX * 86 / 100, NOISE_MAX * 40 / 100),
        (Biome::Badlands, true) => (NOISE_MAX * 88 / 100, NOISE_MAX * 50 / 100),
        (Biome::Plains | Biome::Forest, _) | (Biome::Mountains, false) => return None,
        _ => (NOISE_MAX * 91 / 100, NOISE_MAX * 60 / 100),
    };
    if value_noise(seed, SALT_ORE, x, y, 3, 3) < pod_threshold
        || value_noise(seed, SALT_ORE + 1, x, y, 13, 9) < mask_threshold
    {
        return None;
    }
    let pick = hash2(seed, SALT_ORE + 2, x.div_euclid(4), y.div_euclid(3)) % 100;
    Some(match biome {
        Biome::Mountains => {
            let altitude = SKY_ROWS - y;
            match altitude {
                ..14 => {
                    if pick < 60 {
                        MineralKind::Iron
                    } else {
                        MineralKind::Copper
                    }
                }
                14..32 => {
                    if pick < 65 {
                        MineralKind::Silver
                    } else {
                        MineralKind::Iron
                    }
                }
                32..50 => {
                    if pick < 60 {
                        MineralKind::Gold
                    } else {
                        MineralKind::Silver
                    }
                }
                _ => {
                    if pick < 55 {
                        MineralKind::Emerald
                    } else {
                        MineralKind::Gold
                    }
                }
            }
        }
        Biome::Desert => {
            if pick < 35 {
                MineralKind::Gold
            } else {
                MineralKind::Copper
            }
        }
        Biome::Badlands => {
            if pick < 55 {
                MineralKind::Copper
            } else {
                MineralKind::Iron
            }
        }
        Biome::Tundra => {
            if pick < 45 {
                MineralKind::Silver
            } else {
                MineralKind::Iron
            }
        }
        Biome::Wetlands | Biome::Plains | Biome::Forest => MineralKind::Iron,
    })
}

/// Whether `(x, y)` is carved out by a mountain cave.
///
/// Winding tunnels and the odd chamber in the mass of a mountain or mesa above the classic
/// surface (they sometimes break out onto a slope, so a cave mouth is a way in).
#[must_use]
pub fn mountain_cave(settings: &WorldSettings, x: i32, y: i32) -> bool {
    if y > SKY_ROWS - 3 {
        return false;
    }
    let column = column(settings, x);
    if !matches!(column.biome, Biome::Mountains | Biome::Badlands)
        || column.flat
        || y < column.surface_row + 3
    {
        return false;
    }
    let seed = settings.seed;
    if value_noise(seed, SALT_CAVE + 1, x, y, 64, 32) < NOISE_MAX * 45 / 100 {
        return false;
    }
    let tunnel = super::noise::ridge(value_noise(seed, SALT_CAVE, x, y, 24, 9));
    if tunnel > NOISE_MAX - 2_600 {
        return true;
    }
    value_noise(seed, SALT_CAVE + 2, x, y, 13, 7) > NOISE_MAX * 92 / 100
}

// ---------------------------------------------------------------------------------------------
// Flora.

/// The vegetation or prop standing on column `x`'s generated ground, if any.
///
/// Presentation data: a renderer draws it only while that ground is undug. Nothing grows on
/// the town's or a site's flat ground except grass and flowers. `None` for worlds without
/// biomes.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one table of what grows in each biome reads best in one place"
)]
pub fn flora_at(settings: &WorldSettings, x: i32) -> Option<FloraSpot> {
    if !settings.generator.has_biomes() {
        return None;
    }
    let column = column(settings, x);
    let seed = settings.seed;
    let roll = hash2(seed, SALT_FLORA, x, 0);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the variant is any 32 bits of the hash"
    )]
    let variant = (roll >> 32) as u32;
    let pick = roll % 1000;
    // Trees keep apart: a tree only where neither close neighbour rolled one first.
    let tree_room = |chance: u64| {
        pick < chance && (1..=3).all(|step| hash2(seed, SALT_FLORA, x - step, 0) % 1000 >= chance)
    };
    let spot = |flora| Some(FloraSpot { flora, variant });
    if column.flat {
        return match pick {
            0..90 => spot(Flora::GrassTuft),
            90..130 if column.biome == Biome::Plains || column.biome == Biome::Forest => {
                spot(Flora::Flowers)
            }
            _ => None,
        };
    }
    if lake_row(settings, x).is_some() {
        return (pick < 160).then_some(FloraSpot {
            flora: Flora::LilyPad,
            variant,
        });
    }
    match column.biome {
        Biome::Plains => match pick {
            0..30 if tree_room(30) => spot(Flora::Oak),
            30..100 => spot(Flora::Bush),
            100..230 => spot(Flora::Flowers),
            230..520 => spot(Flora::GrassTuft),
            _ => None,
        },
        Biome::Forest => {
            let dense_room = hash2(seed, SALT_FLORA, x - 1, 0) % 1000 >= 420;
            if pick < 420 && dense_room {
                spot(if pick.is_multiple_of(3) {
                    Flora::Pine
                } else {
                    Flora::Oak
                })
            } else if (420..540).contains(&pick) {
                spot(Flora::Bush)
            } else if pick < 820 {
                spot(Flora::GrassTuft)
            } else {
                None
            }
        }
        Biome::Desert => match pick {
            0..70 if tree_room(70) => spot(Flora::Cactus),
            70..150 => spot(Flora::DeadBush),
            150..175 => spot(Flora::Boulder),
            _ => None,
        },
        Biome::Badlands => match pick {
            0..80 => spot(Flora::DeadBush),
            80..130 => spot(Flora::Boulder),
            130..160 if tree_room(160) => spot(Flora::Cactus),
            _ => None,
        },
        Biome::Tundra => match pick {
            0..90 if tree_room(90) => spot(Flora::SnowPine),
            90..140 => spot(Flora::Boulder),
            140..220 => spot(Flora::DeadBush),
            _ => None,
        },
        Biome::Mountains => {
            let altitude = SKY_ROWS - column.surface_row;
            if column.surface_row < snowline_row(seed, x) {
                (pick < 40).then_some(FloraSpot {
                    flora: Flora::Boulder,
                    variant,
                })
            } else if altitude < TREELINE_ROWS {
                if tree_room(260) {
                    spot(if altitude > TREELINE_ROWS / 2 {
                        Flora::SnowPine
                    } else {
                        Flora::Pine
                    })
                } else if pick < 420 {
                    spot(Flora::GrassTuft)
                } else {
                    None
                }
            } else {
                match pick {
                    0..60 => spot(Flora::Boulder),
                    60..140 => spot(Flora::GrassTuft),
                    _ => None,
                }
            }
        }
        Biome::Wetlands => match pick {
            0..380 => spot(Flora::Reeds),
            380..470 => spot(Flora::Bush),
            470..520 if tree_room(50) => spot(Flora::Oak),
            520..800 => spot(Flora::GrassTuft),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests;
