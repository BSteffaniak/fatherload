//! The macro layer stack: which band (shallow layer or deep stratum) a tile belongs to.
//!
//! Bands always come in the same order. In generated worlds each boundary between two bands sits
//! at a per-seed depth, waves along x, and frays into short interleaved fingers, so a band
//! transition reads as a ragged natural seam rather than a ruler line. V1/V2 worlds shift each
//! boundary a row or two; V3 worlds draw every band's thickness per seed (within bounds, with a
//! minimum gap) and stack them, so layer depths differ noticeably between seeds. Legacy worlds
//! keep the original fixed depths.

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
    }
    bases
}

/// Wave offset of boundary `index` in column `x`.
const fn boundary_wave(settings: &WorldSettings, index: usize, x: i32) -> i32 {
    let seed = settings.seed;
    let salt = SALT_WAVE + index as u64;
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

/// The band containing tile `(x, y)` (rows above the ground count as topsoil).
#[must_use]
pub const fn band_at(settings: &WorldSettings, x: i32, y: i32) -> Band {
    let bases = boundary_bases(settings);
    let mut index = 0;
    while index < BOUNDARY_COUNT && below_boundary(settings, bases[index], index, x, y) {
        index += 1;
    }
    Band::ALL[index]
}

/// Rows a boundary may occupy in this world: `(shallowest, deepest)` over every column.
#[must_use]
pub const fn boundary_depth_range(settings: &WorldSettings, index: usize) -> (i32, i32) {
    let spec = BOUNDARIES[index];
    let base = boundary_bases(settings)[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return (base, base);
    }
    (
        base - spec.wave - spec.finger,
        base + spec.wave + spec.finger,
    )
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
                boundary_column_depth(settings, boundary, x) + BOUNDARIES[boundary].finger + 1
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

    #[test]
    fn generated_bands_keep_their_order_in_every_column() {
        for seed in 0..40 {
            let settings = WorldSettings::new(seed);
            for x in (0..360).step_by(7) {
                let mut previous = 0;
                for y in 0..220 {
                    let index = band_at(&settings, x, y).index();
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
            let settings = WorldSettings::new(seed);
            assert_eq!(settings.generator, GeneratorVersion::V3);
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
        let settings = WorldSettings::new(42);
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
