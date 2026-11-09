//! The macro layer stack: which band (shallow layer or deep stratum) a tile belongs to.
//!
//! Bands always come in the same order. In generated worlds each boundary between two bands sits
//! at a per-seed depth, waves along x, and frays into short interleaved fingers, so a band
//! transition reads as a ragged natural seam rather than a ruler line. V1/V2 worlds shift each
//! boundary a row or two; V3 worlds draw every band's thickness per seed (within bounds, with a
//! minimum gap) and stack them, so layer depths differ noticeably between seeds. V4 worlds are
//! about four times deeper: every band (topsoil included) draws its thickness from the same kind
//! of cumulative model, each boundary is a wide transition zone where lenses of the band below
//! thicken over many rows until they take over, and a solid bedrock floor closes the bottom.
//! Legacy worlds keep the original fixed depths.
//!
//! Depth-dependent rules authored against the classic ~220-row stack (mineral depth curves,
//! hazard depth gates, heat, deep-run pressure, quest depths) read rows through
//! [`nominal_row`] / [`scaled_row`], which map a world's own band layout onto the classic
//! nominal rows, so they keep their meaning in deeper worlds.

use serde::{Deserialize, Serialize};

use crate::terrain::{DeepStratum, TileKind};

use super::{
    GeneratorVersion, WorldSettings,
    noise::{centred, hash2, line_noise, value_noise},
};

/// Deepest tile row that is always open sky; ground starts on the next row.
pub const SKY_ROWS: i32 = 5;

/// One layer of the world, top to bottom.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Band {
    Topsoil,
    Clay,
    Stone,
    Slate,
    Basalt,
    Deep(DeepStratum),
}

impl Band {
    /// Every band, top to bottom.
    pub const ALL: [Self; 12] = [
        Self::Topsoil,
        Self::Clay,
        Self::Stone,
        Self::Slate,
        Self::Basalt,
        Self::Deep(DeepStratum::CrystalFaults),
        Self::Deep(DeepStratum::FossilOceans),
        Self::Deep(DeepStratum::PressureCathedrals),
        Self::Deep(DeepStratum::RadioactiveHollow),
        Self::Deep(DeepStratum::AncientMachineLayer),
        Self::Deep(DeepStratum::VoidGeodeFields),
        Self::Deep(DeepStratum::MantleStormZone),
    ];

    /// Position in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Topsoil => 0,
            Self::Clay => 1,
            Self::Stone => 2,
            Self::Slate => 3,
            Self::Basalt => 4,
            Self::Deep(stratum) => 5 + stratum as usize,
        }
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        if index < Self::ALL.len() {
            Self::ALL[index]
        } else {
            Self::Deep(DeepStratum::MantleStormZone)
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Topsoil => "Topsoil",
            Self::Clay => "Clay Beds",
            Self::Stone => "Grey Stone",
            Self::Slate => "Slate Shelves",
            Self::Basalt => "Basalt Deeps",
            Self::Deep(stratum) => stratum.name(),
        }
    }

    #[must_use]
    pub const fn deep_stratum(self) -> Option<DeepStratum> {
        match self {
            Self::Deep(stratum) => Some(stratum),
            _ => None,
        }
    }

    /// Drill strength that digs this band's common rock (its soft material and every ore the
    /// band's depth curves allow), so each band is passable once the rig has this drill.
    #[must_use]
    pub const fn required_drill_strength(self) -> u8 {
        match self {
            Self::Topsoil => 1,
            Self::Clay => 2,
            Self::Stone | Self::Slate => 3,
            Self::Basalt | Self::Deep(_) => 4,
        }
    }

    /// The band's common rock (route channels, the host of ore and pockets).
    #[must_use]
    pub const fn soft_material(self) -> TileKind {
        match self {
            Self::Topsoil => TileKind::Dirt,
            Self::Clay => TileKind::Clay,
            Self::Stone
            | Self::Slate
            | Self::Deep(DeepStratum::CrystalFaults | DeepStratum::FossilOceans) => TileKind::Stone,
            Self::Basalt | Self::Deep(_) => TileKind::HardRock,
        }
    }

    /// The band's tougher rock (shelves and lumps that shape routes).
    #[must_use]
    pub const fn hard_material(self) -> TileKind {
        match self {
            Self::Topsoil => TileKind::Clay,
            Self::Clay | Self::Basalt | Self::Deep(_) => TileKind::Stone,
            Self::Stone | Self::Slate => TileKind::HardRock,
        }
    }
}

/// One boundary between consecutive bands.
#[derive(Clone, Copy)]
struct BoundarySpec {
    /// Legacy (and average) first row of the lower band.
    nominal: i32,
    /// Per-seed shift of the whole boundary, `-jitter..=jitter` rows.
    jitter: i32,
    /// Amplitude of the large wave along x.
    wave: i32,
    /// Amplitude of the short interleaved fingers.
    finger: i32,
}

const BOUNDARIES: [BoundarySpec; 11] = [
    BoundarySpec {
        nominal: 14,
        jitter: 1,
        wave: 1,
        finger: 1,
    },
    BoundarySpec {
        nominal: 28,
        jitter: 2,
        wave: 2,
        finger: 1,
    },
    BoundarySpec {
        nominal: 46,
        jitter: 2,
        wave: 4,
        finger: 2,
    },
    BoundarySpec {
        nominal: 65,
        jitter: 2,
        wave: 4,
        finger: 2,
    },
    BoundarySpec {
        nominal: 90,
        jitter: 3,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 110,
        jitter: 2,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 130,
        jitter: 2,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 150,
        jitter: 2,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 170,
        jitter: 2,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 190,
        jitter: 2,
        wave: 5,
        finger: 2,
    },
    BoundarySpec {
        nominal: 210,
        jitter: 2,
        wave: 3,
        finger: 1,
    },
];

/// Number of band boundaries (one fewer than bands).
pub const BOUNDARY_COUNT: usize = BOUNDARIES.len();
/// Number of bands.
pub const BAND_COUNT: usize = BOUNDARY_COUNT + 1;

const SALT_SHIFT: u64 = 0x5743_0001;
const SALT_WAVE: u64 = 0x5743_0100;
const SALT_FINGER: u64 = 0x5743_0200;
const SALT_THICKNESS: u64 = 0x5743_0300;

/// Seed-chosen shift of boundary `index` in V1/V2 worlds (their small thickness variation).
const fn boundary_shift(seed: u64, index: usize) -> i32 {
    jitter(seed, index, BOUNDARIES[index].jitter)
}

/// A per-seed offset of boundary `index` in `-amplitude..=amplitude`.
const fn jitter(seed: u64, index: usize, amplitude: i32) -> i32 {
    let span = (amplitude * 2 + 1).unsigned_abs() as u64;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "the remainder is below a small span"
    )]
    let shift = (hash2(seed, SALT_SHIFT, index as i32, 0) % span) as i32 - amplitude;
    shift
}

/// V3 band thickness model, for the bands between the first and last boundary (clay beds down
/// to the void geode fields): `(minimum rows, base share of the spare rows)`.
///
/// Each world draws a weight per band (a tenth to three times its base share) and splits the spare
/// rows between the first and last boundary by those weights, so every band's thickness varies
/// noticeably between seeds while the stack keeps its order, its overall depth and a minimum
/// gap. The minimum leaves room for both bounding boundaries' waves and fingers, so a column
/// never skips a band.
const V3_THICKNESS: [(i32, u64); BOUNDARY_COUNT - 1] = [
    (9, 5),
    (12, 6),
    (14, 5),
    (16, 9),
    (16, 4),
    (16, 4),
    (16, 4),
    (16, 4),
    (16, 4),
    (14, 6),
];
/// Per-seed shift of the first (topsoil / clay) boundary in V3 worlds.
const V3_FIRST_JITTER: i32 = 1;
/// Per-seed shift of the last (mantle) boundary in V3 worlds.
const V3_LAST_JITTER: i32 = 3;

/// V4 band thickness model for every band, topsoil to mantle: `(minimum rows, base share of the
/// spare rows)`.
///
/// Each world draws a weight per band (from 0.4 to 1.6 times its base share) and splits the rows
/// between the ground line and the bedrock floor that the minimums leave by those weights. The
/// shares match the spare rows of a mean world, so a band's mean thickness is about its minimum
/// plus its share. Every minimum leaves room for both bounding transition zones (see
/// [`V4_BOUNDARIES`]), so a column never skips a band.
const V4_THICKNESS: [(i32, u64); BAND_COUNT] = [
    (24, 8),
    (34, 14),
    (42, 18),
    (46, 20),
    (56, 24),
    (58, 24),
    (58, 24),
    (58, 24),
    (58, 24),
    (58, 24),
    (58, 24),
    (50, 18),
];

/// Shape of each V4 boundary: `(wave amplitude, half height of the transition zone)`.
///
/// Inside the transition zone, lenses and fingers of the lower band grow from rare specks at the
/// top of the zone to the whole rock mass at its bottom, so moving between bands takes a few
/// screens of mixed ground instead of a single seam.
const V4_BOUNDARIES: [(i32, i32); BOUNDARY_COUNT] = [
    (3, 4),
    (6, 8),
    (8, 10),
    (8, 10),
    (10, 12),
    (10, 12),
    (10, 12),
    (10, 12),
    (10, 12),
    (10, 12),
    (10, 12),
];

/// Rows of bedrock under every column of a V4 world at least; the floor's top waves up to
/// [`BEDROCK_WAVE_ROWS`] rows higher.
pub const BEDROCK_MIN_ROWS: i32 = 6;
/// Extra rows the bedrock floor's top rises by in places.
pub const BEDROCK_WAVE_ROWS: i32 = 3;
const SALT_BEDROCK: u64 = 0x5743_0400;
const SALT_LENS: u64 = 0x5743_0500;

/// This world's base row of every boundary (before waves and fingers): fixed in legacy worlds,
/// nominal plus a small shift in V1/V2, and the cumulative thickness model in V3.
#[must_use]
pub const fn boundary_bases(settings: &WorldSettings) -> [i32; BOUNDARY_COUNT] {
    let mut bases = [0; BOUNDARY_COUNT];
    let seed = settings.seed;
    match settings.generator {
        GeneratorVersion::Legacy | GeneratorVersion::V1 | GeneratorVersion::V2 => {
            let legacy = matches!(settings.generator, GeneratorVersion::Legacy);
            let mut index = 0;
            while index < BOUNDARY_COUNT {
                bases[index] = BOUNDARIES[index].nominal;
                if !legacy {
                    bases[index] += boundary_shift(seed, index);
                }
                index += 1;
            }
        }
        GeneratorVersion::V3 => {
            let last_index = BOUNDARY_COUNT - 1;
            let first = BOUNDARIES[0].nominal + jitter(seed, 0, V3_FIRST_JITTER);
            let last = BOUNDARIES[last_index].nominal + jitter(seed, last_index, V3_LAST_JITTER);
            let mut weights = [0_u64; BOUNDARY_COUNT - 1];
            let mut total_weight = 0;
            let mut minimum = 0;
            let mut band = 0;
            while band < V3_THICKNESS.len() {
                let (rows, share) = V3_THICKNESS[band];
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap,
                    reason = "band indices are tiny"
                )]
                let roll = hash2(seed, SALT_THICKNESS, band as i32, 0) % 291;
                weights[band] = share * (10 + roll);
                total_weight += weights[band];
                minimum += rows;
                band += 1;
            }
            let spare = (last - first - minimum).unsigned_abs() as u64;
            let mut extra = [0_u64; BOUNDARY_COUNT - 1];
            let mut handed_out = 0;
            band = 0;
            while band < V3_THICKNESS.len() {
                extra[band] = spare * weights[band] / total_weight;
                handed_out += extra[band];
                band += 1;
            }
            // Rounding leftovers go one row each to the shallowest bands.
            band = 0;
            while handed_out < spare {
                extra[band % V3_THICKNESS.len()] += 1;
                handed_out += 1;
                band += 1;
            }
            bases[0] = first;
            band = 0;
            while band < V3_THICKNESS.len() {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap,
                    reason = "extra rows are below the world height"
                )]
                let thickness = V3_THICKNESS[band].0 + extra[band] as i32;
                bases[band + 1] = bases[band] + thickness;
                band += 1;
            }
        }
        GeneratorVersion::V4 => {
            let thickness = v4_thicknesses(settings);
            let mut row = SKY_ROWS;
            let mut index = 0;
            while index < BOUNDARY_COUNT {
                row += thickness[index];
                bases[index] = row;
                index += 1;
            }
        }
    }
    bases
}

/// First row below the V4 band layout: the bedrock floor starts here or, in places, up to
/// [`BEDROCK_WAVE_ROWS`] rows lower (see [`bedrock_top_row`]).
#[must_use]
pub const fn v4_floor_row(settings: &WorldSettings) -> i32 {
    settings.height_tiles() - BEDROCK_MIN_ROWS - BEDROCK_WAVE_ROWS
}

/// Each band's thickness in a V4 world, top to bottom (the mantle runs down to the floor).
const fn v4_thicknesses(settings: &WorldSettings) -> [i32; BAND_COUNT] {
    let seed = settings.seed;
    let mut weights = [0_u64; BAND_COUNT];
    let mut total_weight = 0;
    let mut minimum = 0;
    let mut band = 0;
    while band < BAND_COUNT {
        let (rows, share) = V4_THICKNESS[band];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "band indices are tiny"
        )]
        let roll = hash2(seed, SALT_THICKNESS ^ 0x0400, band as i32, 0) % 121;
        weights[band] = share * (40 + roll);
        total_weight += weights[band];
        minimum += rows;
        band += 1;
    }
    let spare = (v4_floor_row(settings) - SKY_ROWS - minimum).unsigned_abs() as u64;
    let mut thickness = [0_i32; BAND_COUNT];
    let mut handed_out = 0;
    band = 0;
    while band < BAND_COUNT {
        let extra = spare * weights[band] / total_weight;
        handed_out += extra;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "extra rows are below the world height"
        )]
        let rows = V4_THICKNESS[band].0 + extra as i32;
        thickness[band] = rows;
        band += 1;
    }
    // Rounding leftovers go one row each to the deepest bands.
    band = BAND_COUNT;
    while handed_out < spare {
        band = if band == 0 { BAND_COUNT - 1 } else { band - 1 };
        thickness[band] += 1;
        handed_out += 1;
    }
    thickness
}

/// First bedrock row of column `x` in a V4 world (`None` in older worlds, which have no floor).
#[must_use]
pub const fn bedrock_top_row(settings: &WorldSettings, x: i32) -> Option<i32> {
    if !matches!(settings.generator, GeneratorVersion::V4) {
        return None;
    }
    let wave = (line_noise(settings.seed, SALT_BEDROCK, x, 9) * 2
        + line_noise(settings.seed, SALT_BEDROCK ^ 0xFF, x, 3))
        / 3;
    let rise = wave * (BEDROCK_WAVE_ROWS + 1) / (super::noise::NOISE_MAX + 1);
    Some(settings.height_tiles() - BEDROCK_MIN_ROWS - rise)
}

/// Whether tile `(x, y)` is part of the bedrock floor.
#[must_use]
pub const fn is_bedrock(settings: &WorldSettings, x: i32, y: i32) -> bool {
    match bedrock_top_row(settings, x) {
        Some(top) => y >= top,
        None => false,
    }
}

/// `(wave amplitude, fringe)` of boundary `index`: how far its line waves along x, and how many
/// rows either side of that line the two bands interleave (fingers in V1-V3, a transition zone
/// in V4).
const fn boundary_shape(settings: &WorldSettings, index: usize) -> (i32, i32) {
    if matches!(settings.generator, GeneratorVersion::V4) {
        V4_BOUNDARIES[index]
    } else {
        (BOUNDARIES[index].wave, BOUNDARIES[index].finger)
    }
}

/// Wave offset of boundary `index` in column `x`.
const fn boundary_wave(settings: &WorldSettings, index: usize, x: i32) -> i32 {
    let seed = settings.seed;
    let salt = SALT_WAVE + index as u64;
    if matches!(settings.generator, GeneratorVersion::V4) {
        // Longer, deeper swells for the deeper stack.
        let wave = (line_noise(seed, salt, x, 53) * 2 + line_noise(seed, salt ^ 0xFF, x, 17)) / 3;
        return centred(wave, V4_BOUNDARIES[index].0);
    }
    let wave = (line_noise(seed, salt, x, 31) * 2 + line_noise(seed, salt ^ 0xFF, x, 11)) / 3;
    centred(wave, BOUNDARIES[index].wave)
}

/// First row of the band below boundary `index` in column `x`, before fingering.
#[must_use]
pub const fn boundary_column_depth(settings: &WorldSettings, index: usize, x: i32) -> i32 {
    let base = boundary_bases(settings)[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return base;
    }
    base + boundary_wave(settings, index, x)
}

/// Whether tile `(x, y)` lies below boundary `index`, whose base row is `base`.
const fn below_boundary(settings: &WorldSettings, base: i32, index: usize, x: i32, y: i32) -> bool {
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return y >= base;
    }
    if matches!(settings.generator, GeneratorVersion::V4) {
        return below_v4_boundary(settings, base, index, x, y);
    }
    let spec = BOUNDARIES[index];
    let reach = spec.wave + spec.finger + 1;
    if y >= base + reach {
        return true;
    }
    if y < base - reach {
        return false;
    }
    let finger = centred(
        value_noise(settings.seed, SALT_FINGER + index as u64, x, y, 3, 2),
        spec.finger,
    );
    y >= base + boundary_wave(settings, index, x) + finger
}

/// [`below_boundary`] for V4 worlds: a transition zone `half` rows either side of the wavy
/// boundary line, in which lenses of the lower band (smooth noise blobs, a little stretched
/// sideways, with fine fingers on their rims) get likelier row by row.
const fn below_v4_boundary(
    settings: &WorldSettings,
    base: i32,
    index: usize,
    x: i32,
    y: i32,
) -> bool {
    let (wave, half) = V4_BOUNDARIES[index];
    if y >= base + wave + half {
        return true;
    }
    if y < base - wave - half {
        return false;
    }
    let line = base + boundary_wave(settings, index, x);
    let top = line - half;
    if y < top {
        return false;
    }
    if y >= line + half {
        return true;
    }
    // How far through the zone this row is, 0..NOISE_MAX.
    let along = (y - top) * super::noise::NOISE_MAX / (half * 2);
    let salt = SALT_LENS + index as u64;
    let lens = value_noise(settings.seed, salt, x, y, 9, 4);
    let fringe = value_noise(settings.seed, salt ^ 0xFF, x, y, 3, 2);
    let mixed = (lens * 3 + fringe) / 4;
    // Stretch the bell-shaped noise so the first lenses appear near the top of the zone and the
    // last pockets of the upper band linger near its bottom.
    let spread = 32_768 + (mixed - 32_768) * 2;
    spread < along
}

/// The band containing tile `(x, y)` (rows above the ground count as topsoil).
#[must_use]
pub const fn band_at(settings: &WorldSettings, x: i32, y: i32) -> Band {
    band_at_with_bases(settings, &boundary_bases(settings), x, y)
}

/// [`band_at`] with this world's [`boundary_bases`] already computed (bulk passes).
#[must_use]
pub const fn band_at_with_bases(
    settings: &WorldSettings,
    bases: &[i32; BOUNDARY_COUNT],
    x: i32,
    y: i32,
) -> Band {
    let mut index = 0;
    while index < BOUNDARY_COUNT && below_boundary(settings, bases[index], index, x, y) {
        index += 1;
    }
    Band::ALL[index]
}

/// Rows a boundary may occupy in this world: `(shallowest, deepest)` over every column.
#[must_use]
pub const fn boundary_depth_range(settings: &WorldSettings, index: usize) -> (i32, i32) {
    let (wave, fringe) = boundary_shape(settings, index);
    let base = boundary_bases(settings)[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return (base, base);
    }
    (base - wave - fringe, base + wave + fringe)
}

/// The shallowest row `band` reaches anywhere in this world (where its top boundary rises
/// highest).
#[must_use]
pub const fn band_shallowest_row(settings: &WorldSettings, band: Band) -> i32 {
    match band.index() {
        0 => SKY_ROWS + 1,
        index => boundary_depth_range(settings, index - 1).0,
    }
}

/// The first row of column `x` that lies inside `band` whatever the boundary fingers do there
/// (the band's top, pushed past any interleaving with the band above).
#[must_use]
pub const fn band_solid_top_row(settings: &WorldSettings, band: Band, x: i32) -> i32 {
    match band.index() {
        0 => SKY_ROWS + 1,
        index => {
            let boundary = index - 1;
            if matches!(settings.generator, GeneratorVersion::Legacy) {
                BOUNDARIES[boundary].nominal
            } else {
                boundary_column_depth(settings, boundary, x)
                    + boundary_shape(settings, boundary).1
                    + 1
            }
        }
    }
}

/// Whether a crew whose deepest tile row is `deepest_row` has reached `band` in this world.
#[must_use]
pub const fn band_reached(settings: &WorldSettings, deepest_row: i32, band: Band) -> bool {
    deepest_row >= band_shallowest_row(settings, band)
}

/// The rows `band` spans in this world before waves and fingers, `(first, last)`.
///
/// Placement passes use it to put things inside a band. V1/V2 (and legacy) worlds keep the nominal rows they always used; V3
/// worlds use their own band thicknesses.
#[must_use]
pub const fn band_layout_rows(settings: &WorldSettings, band: Band) -> (i32, i32) {
    let index = band.index();
    let last_band = Band::ALL.len() - 1;
    if matches!(settings.generator, GeneratorVersion::V4) {
        let bases = boundary_bases(settings);
        let top = if index == 0 {
            SKY_ROWS
        } else {
            bases[index - 1]
        };
        let bottom = if index < last_band {
            bases[index] - 1
        } else {
            v4_floor_row(settings) - 1
        };
        return (top, bottom);
    }
    if !matches!(settings.generator, GeneratorVersion::V3) {
        let top = nominal_band_top(band);
        let bottom = if index < last_band {
            nominal_band_top(Band::ALL[index + 1]) - 1
        } else {
            super::WORLD_HEIGHT_TILES - 1
        };
        return (top, bottom);
    }
    let bases = boundary_bases(settings);
    let top = if index == 0 {
        SKY_ROWS
    } else {
        bases[index - 1]
    };
    let bottom = if index < last_band {
        bases[index] - 1
    } else {
        super::WORLD_HEIGHT_TILES - 1
    };
    (top, bottom)
}

/// The rows the nominal mapping pins together: this world's first row of every band (and the
/// row past the last band) against the classic nominal rows.
const fn nominal_anchors(
    settings: &WorldSettings,
) -> ([i32; BAND_COUNT + 1], [i32; BAND_COUNT + 1]) {
    let bases = boundary_bases(settings);
    let mut world = [0; BAND_COUNT + 1];
    let mut nominal = [0; BAND_COUNT + 1];
    world[0] = SKY_ROWS;
    nominal[0] = SKY_ROWS;
    let mut index = 0;
    while index < BOUNDARY_COUNT {
        world[index + 1] = bases[index];
        nominal[index + 1] = BOUNDARIES[index].nominal;
        index += 1;
    }
    world[BAND_COUNT] = v4_floor_row(settings);
    nominal[BAND_COUNT] = super::WORLD_HEIGHT_TILES;
    (world, nominal)
}

/// Linear interpolation of `value` from the rows `from` onto the rows `to` (slope 1 past either
/// end).
const fn remap(value: i32, from: &[i32; BAND_COUNT + 1], to: &[i32; BAND_COUNT + 1]) -> i32 {
    if value <= from[0] {
        return to[0] + (value - from[0]);
    }
    let mut index = 0;
    while index < BAND_COUNT {
        let (low, high) = (from[index], from[index + 1]);
        if value < high {
            let span = high - low;
            if span <= 0 {
                return to[index];
            }
            return to[index] + (value - low) * (to[index + 1] - to[index]) / span;
        }
        index += 1;
    }
    to[BAND_COUNT] + (value - from[BAND_COUNT])
}

/// The classic nominal row (of the ~220-row stack the depth rules were authored against) that
/// row `row` of this world corresponds to: the same relative position inside the same band.
///
/// The identity in every world before V4.
#[must_use]
pub const fn nominal_row(settings: &WorldSettings, row: i32) -> i32 {
    if !matches!(settings.generator, GeneratorVersion::V4) {
        return row;
    }
    let (world, nominal) = nominal_anchors(settings);
    remap(row, &world, &nominal)
}

/// The inverse of [`nominal_row`]: the first row of this world that maps at or past the classic
/// nominal row `nominal`. The identity in every world before V4.
#[must_use]
pub const fn scaled_row(settings: &WorldSettings, nominal: i32) -> i32 {
    if !matches!(settings.generator, GeneratorVersion::V4) {
        return nominal;
    }
    let (world, nominal_rows) = nominal_anchors(settings);
    let mut row = remap(nominal, &nominal_rows, &world);
    // Integer rounding: settle on the first row that maps at or past `nominal`.
    while remap(row - 1, &world, &nominal_rows) >= nominal && row > 0 {
        row -= 1;
    }
    while remap(row, &world, &nominal_rows) < nominal {
        row += 1;
    }
    row
}

/// [`nominal_row`] for a continuous depth in tile rows (e.g. a rig's world-pixel depth divided by
/// the tile size), interpolating between rows.
#[must_use]
pub fn nominal_depth(settings: &WorldSettings, rows: f32) -> f32 {
    if !matches!(settings.generator, GeneratorVersion::V4) {
        return rows;
    }
    let (world, nominal) = nominal_anchors(settings);
    #[allow(
        clippy::cast_precision_loss,
        reason = "rows are far below f32's integer range"
    )]
    let to_f32 = |value: i32| value as f32;
    if rows <= to_f32(world[0]) {
        return rows;
    }
    for index in 0..BAND_COUNT {
        let (low, high) = (to_f32(world[index]), to_f32(world[index + 1]));
        if rows < high {
            let span = (high - low).max(1.0);
            return (rows - low).mul_add(
                (to_f32(nominal[index + 1]) - to_f32(nominal[index])) / span,
                to_f32(nominal[index]),
            );
        }
    }
    to_f32(nominal[BAND_COUNT]) + (rows - to_f32(world[BAND_COUNT]))
}

/// How many of this world's rows a span of `nominal_rows` classic rows covers on average (quest
/// relay legs and similar distances authored against the classic stack).
#[must_use]
pub const fn scaled_span(settings: &WorldSettings, nominal_rows: i32) -> i32 {
    if !matches!(settings.generator, GeneratorVersion::V4) {
        return nominal_rows;
    }
    let world_rows = v4_floor_row(settings) - SKY_ROWS;
    let classic_rows = super::WORLD_HEIGHT_TILES - SKY_ROWS;
    nominal_rows * world_rows / classic_rows
}

/// Nominal (legacy) first row of `band`.
#[must_use]
pub const fn nominal_band_top(band: Band) -> i32 {
    match band.index() {
        0 => SKY_ROWS,
        index => BOUNDARIES[index - 1].nominal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bands_match_the_original_depths() {
        let legacy = WorldSettings::legacy(1);
        assert_eq!(band_at(&legacy, 3, 13), Band::Topsoil);
        assert_eq!(band_at(&legacy, 3, 14), Band::Clay);
        assert_eq!(band_at(&legacy, 3, 45), Band::Stone);
        assert_eq!(band_at(&legacy, 3, 46), Band::Slate);
        assert_eq!(band_at(&legacy, 3, 89), Band::Basalt);
        assert_eq!(
            band_at(&legacy, 3, 90),
            Band::Deep(DeepStratum::CrystalFaults)
        );
        assert_eq!(
            band_at(&legacy, 3, 215),
            Band::Deep(DeepStratum::MantleStormZone)
        );
    }

    const fn v3(seed: u64) -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V3,
            ..WorldSettings::new(seed)
        }
    }

    #[test]
    fn generated_bands_keep_their_order_in_every_column() {
        for seed in 0..40 {
            for settings in [v3(seed), WorldSettings::new(seed)] {
                check_band_order(&settings);
            }
        }
    }

    fn check_band_order(settings: &WorldSettings) {
        let seed = settings.seed;
        {
            for x in (0..settings.width_tiles()).step_by(7) {
                let mut previous = 0;
                for y in 0..settings.height_tiles() {
                    let index = band_at(settings, x, y).index();
                    // Fingers may interleave neighbours, but never skip a band or jump back
                    // more than one.
                    assert!(index + 1 >= previous, "seed {seed} x {x} y {y}");
                    assert!(index <= previous + 1, "seed {seed} x {x} y {y}");
                    previous = previous.max(index);
                }
                assert_eq!(previous, Band::ALL.len() - 1);
            }
        }
    }

    #[test]
    fn v3_band_thicknesses_vary_by_seed_within_bounds() {
        let mut tops = vec![Vec::new(); Band::ALL.len()];
        for seed in 0..200 {
            let settings = v3(seed);
            let bases = boundary_bases(&settings);
            assert!((13..=15).contains(&bases[0]), "seed {seed}: {bases:?}");
            assert!(
                (207..=213).contains(&bases[BOUNDARY_COUNT - 1]),
                "seed {seed}: {bases:?}"
            );
            for (band, (minimum, _)) in V3_THICKNESS.iter().enumerate() {
                let thickness = bases[band + 1] - bases[band];
                assert!(
                    thickness >= *minimum,
                    "seed {seed}: band {} is {thickness} rows",
                    band + 1
                );
            }
            for band in Band::ALL {
                tops[band.index()].push(band_layout_rows(&settings, band).0);
            }
        }
        // Every deep stratum's top moves by more than a few rows between seeds.
        for band in Band::ALL.iter().skip(3).take(8) {
            let rows = &tops[band.index()];
            let spread = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
            assert!(spread >= 12, "{band:?} top only varies by {spread} rows");
        }
    }

    #[test]
    fn v4_worlds_are_about_four_times_deeper_with_varied_thick_bands() {
        let mut tops = vec![Vec::new(); Band::ALL.len()];
        for seed in 0..200 {
            for size in super::super::WorldSize::ALL {
                let settings = WorldSettings {
                    size,
                    ..WorldSettings::new(seed)
                };
                assert_eq!(settings.generator, GeneratorVersion::V4);
                assert!(settings.height_tiles() >= 4 * super::super::WORLD_HEIGHT_TILES * 9 / 10);
                let mut total = 0;
                for (band, (minimum, _)) in V4_THICKNESS.iter().enumerate() {
                    let (top, bottom) = band_layout_rows(&settings, Band::ALL[band]);
                    let thickness = bottom - top + 1;
                    assert!(
                        thickness >= *minimum,
                        "seed {seed}: band {band} is {thickness} rows"
                    );
                    // Room for both bounding transition zones.
                    let above = band
                        .checked_sub(1)
                        .map_or(0, |index| V4_BOUNDARIES[index].0 + V4_BOUNDARIES[index].1);
                    let below = V4_BOUNDARIES
                        .get(band)
                        .map_or(BEDROCK_WAVE_ROWS, |(wave, half)| wave + half);
                    assert!(thickness > above + below, "seed {seed}: band {band}");
                    total += thickness;
                }
                // The bands fill the world from the ground line to the floor.
                assert_eq!(total, v4_floor_row(&settings) - SKY_ROWS);
                if size == super::super::WorldSize::Standard {
                    for band in Band::ALL {
                        tops[band.index()].push(band_layout_rows(&settings, band).0);
                    }
                }
            }
        }
        // Every band below the topsoil moves by a good few rows between seeds.
        for band in Band::ALL.iter().skip(2) {
            let rows = &tops[band.index()];
            let spread = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
            assert!(spread >= 20, "{band:?} top only varies by {spread} rows");
        }
    }

    #[test]
    fn v4_transitions_interleave_over_many_rows() {
        let settings = WorldSettings::new(7);
        for index in 1..BOUNDARY_COUNT {
            let (shallowest, deepest) = boundary_depth_range(&settings, index);
            let (upper, lower) = (Band::ALL[index], Band::ALL[index + 1]);
            // Rows of the zone where both bands show up in the same row (lenses and fingers).
            let mixed_rows = (shallowest..=deepest)
                .filter(|y| {
                    let bands: Vec<Band> = (0..settings.width_tiles())
                        .map(|x| band_at(&settings, x, *y))
                        .collect();
                    bands.contains(&upper) && bands.contains(&lower)
                })
                .count();
            assert!(
                mixed_rows >= 12,
                "boundary {index}: {mixed_rows} mixed rows"
            );
            // Somewhere a lens of the lower band sits above a pocket of the upper band.
            let lensed = (0..settings.width_tiles()).any(|x| {
                let mut seen_lower = false;
                (shallowest..=deepest).any(|y| {
                    let band = band_at(&settings, x, y);
                    seen_lower |= band == lower;
                    seen_lower && band == upper
                })
            });
            assert!(lensed, "boundary {index} never interleaves");
        }
    }

    #[test]
    fn v4_worlds_end_in_a_bedrock_floor_and_older_worlds_do_not() {
        let settings = WorldSettings::new(11);
        let height = settings.height_tiles();
        let mut tops = Vec::new();
        for x in 0..settings.width_tiles() {
            let top = bedrock_top_row(&settings, x).expect("v4 has a floor");
            assert!(top >= v4_floor_row(&settings) && top <= height - BEDROCK_MIN_ROWS);
            assert!(is_bedrock(&settings, x, height - 1));
            assert!(is_bedrock(&settings, x, top));
            assert!(!is_bedrock(&settings, x, top - 1));
            tops.push(top);
        }
        assert!(tops.iter().min() != tops.iter().max(), "the floor is flat");
        assert_eq!(bedrock_top_row(&v3(11), 4), None);
        assert!(!is_bedrock(&v3(11), 4, 219));
    }

    #[test]
    fn nominal_rows_map_bands_onto_the_classic_stack() {
        for seed in 0..20 {
            let settings = WorldSettings::new(seed);
            for band in Band::ALL {
                let (top, bottom) = band_layout_rows(&settings, band);
                let classic_top = nominal_band_top(band);
                assert_eq!(nominal_row(&settings, top), classic_top, "{band:?}");
                assert!(nominal_row(&settings, bottom) < classic_top.max(SKY_ROWS) + 60);
                assert_eq!(scaled_row(&settings, classic_top), top, "{band:?}");
            }
            let mut previous = nominal_row(&settings, 0);
            for row in 1..settings.height_tiles() {
                let nominal = nominal_row(&settings, row);
                assert!(nominal >= previous, "seed {seed} row {row}");
                previous = nominal;
                // Scaling back lands on the first row with that nominal row.
                let scaled = scaled_row(&settings, nominal);
                assert!(scaled <= row && nominal_row(&settings, scaled) == nominal);
                let depth = nominal_depth(&settings, f32::from(i16::try_from(row).unwrap_or(0)));
                assert!(
                    (depth - f32::from(i16::try_from(nominal).unwrap_or(0))).abs() < 1.0,
                    "seed {seed} row {row}"
                );
            }
            assert_eq!(nominal_row(&settings, v4_floor_row(&settings)), 220);
            assert!(scaled_span(&settings, 22) > 70);
        }
        // Older worlds keep their rows.
        let classic = v3(3);
        for row in [0, 14, 90, 219] {
            assert_eq!(nominal_row(&classic, row), row);
            assert_eq!(scaled_row(&classic, row), row);
        }
        assert_eq!(scaled_span(&classic, 22), 22);
    }

    #[test]
    fn older_generators_keep_their_boundaries() {
        let v2 = WorldSettings {
            generator: GeneratorVersion::V2,
            ..WorldSettings::new(9)
        };
        for (index, base) in boundary_bases(&v2).iter().enumerate() {
            let spec = BOUNDARIES[index];
            assert!((base - spec.nominal).abs() <= spec.jitter);
        }
        assert_eq!(
            band_layout_rows(&v2, Band::Deep(DeepStratum::CrystalFaults)),
            (90, 109)
        );
    }

    #[test]
    fn boundaries_wave_between_columns() {
        let settings = v3(42);
        let depths: Vec<i32> = (0..240)
            .map(|x| boundary_column_depth(&settings, 4, x))
            .collect();
        let min = depths.iter().min().copied().unwrap_or_default();
        let max = depths.iter().max().copied().unwrap_or_default();
        assert!(max - min >= 3, "boundary is flat: {min}..{max}");
        let (low, high) = boundary_depth_range(&settings, 4);
        assert!(low <= min && max <= high);
    }
}
