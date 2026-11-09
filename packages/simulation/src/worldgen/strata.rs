//! The macro layer stack: which band (shallow layer or deep stratum) a tile belongs to.
//!
//! Bands always come in the same order. In generated worlds each boundary between two bands sits
//! at a per-seed depth (thickness varies within bounds), waves along x, and frays into short
//! interleaved fingers, so a band transition reads as a ragged natural seam rather than a ruler
//! line. Legacy worlds keep the original fixed depths.

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

/// Seed-chosen shift of boundary `index` (thickness variation).
const fn boundary_shift(seed: u64, index: usize) -> i32 {
    let spec = BOUNDARIES[index];
    let span = (spec.jitter * 2 + 1).unsigned_abs() as u64;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "the remainder is below a small span"
    )]
    let shift = (hash2(seed, SALT_SHIFT, index as i32, 0) % span) as i32 - spec.jitter;
    shift
}

/// First row of the band below boundary `index` in column `x`, before fingering.
#[must_use]
pub const fn boundary_column_depth(settings: &WorldSettings, index: usize, x: i32) -> i32 {
    let spec = BOUNDARIES[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return spec.nominal;
    }
    let seed = settings.seed;
    let salt = SALT_WAVE + index as u64;
    let wave = (line_noise(seed, salt, x, 31) * 2 + line_noise(seed, salt ^ 0xFF, x, 11)) / 3;
    spec.nominal + boundary_shift(seed, index) + centred(wave, spec.wave)
}

/// Deepest reach of boundary `index` above or below its nominal row in any world.
const fn boundary_reach(index: usize) -> i32 {
    let spec = BOUNDARIES[index];
    spec.jitter + spec.wave + spec.finger + 1
}

/// Whether tile `(x, y)` lies below boundary `index`.
const fn below_boundary(settings: &WorldSettings, index: usize, x: i32, y: i32) -> bool {
    let spec = BOUNDARIES[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return y >= spec.nominal;
    }
    let reach = boundary_reach(index);
    if y >= spec.nominal + reach {
        return true;
    }
    if y < spec.nominal - reach {
        return false;
    }
    let finger = centred(
        value_noise(settings.seed, SALT_FINGER + index as u64, x, y, 3, 2),
        spec.finger,
    );
    y >= boundary_column_depth(settings, index, x) + finger
}

/// The band containing tile `(x, y)` (rows above the ground count as topsoil).
#[must_use]
pub const fn band_at(settings: &WorldSettings, x: i32, y: i32) -> Band {
    let mut index = 0;
    while index < BOUNDARY_COUNT && below_boundary(settings, index, x, y) {
        index += 1;
    }
    Band::ALL[index]
}

/// Rows a boundary may occupy in this world: `(shallowest, deepest)` over every column.
#[must_use]
pub const fn boundary_depth_range(settings: &WorldSettings, index: usize) -> (i32, i32) {
    let spec = BOUNDARIES[index];
    if matches!(settings.generator, GeneratorVersion::Legacy) {
        return (spec.nominal, spec.nominal);
    }
    let shift = boundary_shift(settings.seed, index);
    let base = spec.nominal + shift;
    (
        base - spec.wave - spec.finger,
        base + spec.wave + spec.finger,
    )
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
