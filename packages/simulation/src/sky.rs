//! The sky lands (expansion plan Part II, F6 / WP-SKY / WP-MOON): tile kinds, island
//! palettes and the altitude bands of [`GeneratorVersion::V8`] worlds.
//!
//! Placement (which island hangs where) lives in [`crate::worldgen::sky`]; the physics of
//! altitude (thin air, cold, wind, updrafts, gravity fields) in [`crate::environment`]. This
//! module holds the shared vocabulary: [`SkyTile`] (the tiles islands, continents and the Moon
//! are made of), [`SkyBiome`] (an island's palette) and [`SkyBand`] (which altitude band a row
//! lies in).
//!
//! [`GeneratorVersion::V8`]: crate::worldgen::GeneratorVersion::V8

use serde::{Deserialize, Serialize};

use crate::worldgen::sky::{SkyBands, V8_SKY_BANDS};

/// The palette of one floating island: its turf and the colour of its rock underside.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SkyBiome {
    /// Green turf over slate-blue rock (the common isle).
    Meadow,
    /// Pink-flowered turf over lilac rock.
    Blossom,
    /// Snow-dusted turf over pale frost rock (higher, colder bands).
    Frost,
    /// Golden grass over warm amber sandstone.
    Amber,
}

impl SkyBiome {
    /// Every palette, in code order.
    pub const ALL: [Self; 4] = [Self::Meadow, Self::Blossom, Self::Frost, Self::Amber];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Meadow => "meadow",
            Self::Blossom => "blossom",
            Self::Frost => "frost",
            Self::Amber => "amber",
        }
    }

    /// Stable code (part of [`SkyTile::code`]).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Meadow => 0,
            Self::Blossom => 1,
            Self::Frost => 2,
            Self::Amber => 3,
        }
    }
}

/// A tile of the sky lands ([`crate::terrain::TileKind::Sky`]).
///
/// Islands are a turf top over a few rows of soil over a rock underside, with crystal pockets
/// in the rock and roots and crystal hanging under it. Sky continents are placeholder
/// continent rock under turf (Wave 4 fills them); the Moon is grey regolith over moon rock.
/// Every sky tile can be mined (hardness on the classic ladder) and breaks into the classic
/// block of the same hardness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SkyTile {
    /// The grass (or snow, or flowers) on top of an island or continent.
    Turf(SkyBiome),
    /// Soil under the turf.
    Soil,
    /// An island's rock underside, coloured by its palette.
    Rock(SkyBiome),
    /// Roots dangling under an island (soft).
    Roots,
    /// Sky crystal: pockets in island rock and stalactites hanging below it.
    Crystal,
    /// The placeholder body of a sky continent.
    ContinentRock,
    /// The Moon's grey surface dust.
    Regolith,
    /// The Moon's rocky body and underside.
    MoonRock,
    /// A sky treasure cache bound in brass (WP-SKY, V9): drill it open for credits and sky
    /// loot.
    Cache,
    /// A twig nest on an archipelago isle: drill it for a feather-light find (sometimes a
    /// cloud pearl).
    Nest,
    /// Carved white stone of the aeronauts' ruins.
    RuinBrick,
    /// A broken ruin pillar standing on an island's turf.
    RuinPillar,
    /// A mossy spring at an island's rim: it pours a waterfall into the void below.
    Spring,
    /// Leafy vines dangling under an island (soft, like roots).
    Vines,
    /// The glowing aether keystone that holds a crumbling isle together: break it and the
    /// island falls apart.
    Keystone,
    /// A vent in an island's turf that breathes a column of rising air (an updraft).
    Vent,
    /// The landing pad of a fuel buoy: a rig parked on it can buy a refill.
    BuoyPad,
}

impl SkyTile {
    /// Every sky tile, in code order.
    pub const ALL: [Self; 23] = [
        Self::Turf(SkyBiome::Meadow),
        Self::Turf(SkyBiome::Blossom),
        Self::Turf(SkyBiome::Frost),
        Self::Turf(SkyBiome::Amber),
        Self::Soil,
        Self::Rock(SkyBiome::Meadow),
        Self::Rock(SkyBiome::Blossom),
        Self::Rock(SkyBiome::Frost),
        Self::Rock(SkyBiome::Amber),
        Self::Roots,
        Self::Crystal,
        Self::ContinentRock,
        Self::Regolith,
        Self::MoonRock,
        Self::Cache,
        Self::Nest,
        Self::RuinBrick,
        Self::RuinPillar,
        Self::Spring,
        Self::Vines,
        Self::Keystone,
        Self::Vent,
        Self::BuoyPad,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Turf(_) => "sky turf",
            Self::Soil => "sky soil",
            Self::Rock(_) => "isle rock",
            Self::Roots => "hanging roots",
            Self::Crystal => "sky crystal",
            Self::ContinentRock => "continent rock",
            Self::Regolith => "regolith",
            Self::MoonRock => "moon rock",
            Self::Cache => "sky cache",
            Self::Nest => "nest",
            Self::RuinBrick => "ruin stone",
            Self::RuinPillar => "ruin pillar",
            Self::Spring => "sky spring",
            Self::Vines => "hanging vines",
            Self::Keystone => "aether keystone",
            Self::Vent => "updraft vent",
            Self::BuoyPad => "buoy pad",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`], in the `0x600` family).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Turf(biome) => biome.code(),
            Self::Soil => 0x08,
            Self::Rock(biome) => 0x10 | biome.code(),
            Self::Roots => 0x18,
            Self::Crystal => 0x19,
            Self::ContinentRock => 0x1A,
            Self::Regolith => 0x1B,
            Self::MoonRock => 0x1C,
            Self::Cache => 0x1D,
            Self::Nest => 0x1E,
            Self::RuinBrick => 0x1F,
            Self::RuinPillar => 0x20,
            Self::Spring => 0x21,
            Self::Vines => 0x22,
            Self::Keystone => 0x23,
            Self::Vent => 0x24,
            Self::BuoyPad => 0x25,
        }
    }

    /// The tile with code `code` ([`Self::code`]), if any (saves and chunk diffs decode sky
    /// tiles with it).
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        let mut index = 0;
        while index < Self::ALL.len() {
            if Self::ALL[index].code() == code {
                return Some(Self::ALL[index]);
            }
            index += 1;
        }
        None
    }

    /// Drill strength needed to dig it (the classic ladder: soil like dirt, isle rock like
    /// clay, crystal and moon rock like stone, continent rock like hard rock).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::Turf(_)
            | Self::Soil
            | Self::Roots
            | Self::Regolith
            | Self::Nest
            | Self::Vines
            | Self::Spring => 1,
            Self::Rock(_) | Self::Cache | Self::RuinBrick | Self::Vent | Self::Keystone => 2,
            Self::Crystal | Self::MoonRock | Self::RuinPillar | Self::BuoyPad => 3,
            Self::ContinentRock => 4,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Roots | Self::Vines | Self::Nest => 1,
            Self::Turf(_) | Self::Soil | Self::Regolith | Self::Spring => 2,
            Self::Rock(_) | Self::Cache | Self::Keystone | Self::Vent => 4,
            Self::RuinBrick => 5,
            Self::Crystal | Self::RuinPillar => 6,
            Self::MoonRock => 7,
            Self::ContinentRock | Self::BuoyPad => 9,
        }
    }

    /// Whether drilling this tile opens it for loot instead of dropping a block (sky caches
    /// and nests, see `world/sky_isles.rs`).
    #[must_use]
    pub const fn is_cache(self) -> bool {
        matches!(self, Self::Cache | Self::Nest)
    }
}

/// What a sky town is (WP-SKY, V9): one stands on the town isle of every archipelago block
/// ([`crate::worldgen::sky::towns`]), built with the village grammar in a sky culture.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SkyTownKind {
    /// Aeronauts' moorings: an airship dock where aether thrusters are fitted.
    AirshipDock,
    /// A market floating on the clouds: sky ore bought dear, smugglers' wares.
    FloatingMarket,
    /// A sky monastery: blessings for the high air, de-icers wrought from cloud silver.
    Monastery,
    /// Cloud farmers harvesting condensed vapour: cheap fuel, pressurized hulls.
    CloudFarm,
}

impl SkyTownKind {
    pub const ALL: [Self; 4] = [
        Self::AirshipDock,
        Self::FloatingMarket,
        Self::Monastery,
        Self::CloudFarm,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AirshipDock => "airship dock",
            Self::FloatingMarket => "floating market",
            Self::Monastery => "sky monastery",
            Self::CloudFarm => "cloud-farmer village",
        }
    }

    /// Stable code (map markers, events).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::AirshipDock => 0,
            Self::FloatingMarket => 1,
            Self::Monastery => 2,
            Self::CloudFarm => 3,
        }
    }
}

/// The altitude bands of a V8 sky, bottom to top (see [`crate::worldgen::sky`] for their rows,
/// which are fractions of the climb harness's measurements).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SkyBand {
    /// The ground and the air just above it (mountains, villages, the town).
    Ground,
    /// Small island clusters, reachable with a fuel tank around tier 2-4.
    LowIsles,
    /// Chains of medium islands (mid tiers); updrafts help the climb.
    Archipelago,
    /// No islands, strong crosswinds and thin air: only near-max tanks with high-altitude
    /// intakes cross it.
    DeadAir,
    /// The great sky lands, hundreds of tiles wide, with slightly lower gravity.
    Continents,
    /// The near-zero-g band under the Moon: no air, drifting, inertia-heavy flight.
    Orbit,
    /// The Moon and the space around it: low gravity, no air.
    Lunar,
}

impl SkyBand {
    /// Every band, bottom to top.
    pub const ALL: [Self; 7] = [
        Self::Ground,
        Self::LowIsles,
        Self::Archipelago,
        Self::DeadAir,
        Self::Continents,
        Self::Orbit,
        Self::Lunar,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ground => "Ground",
            Self::LowIsles => "Low Isles",
            Self::Archipelago => "Cloud Archipelago",
            Self::DeadAir => "Dead-Air Gap",
            Self::Continents => "Sky Continents",
            Self::Orbit => "Orbit",
            Self::Lunar => "The Moon",
        }
    }

    /// The altitude (rows above row 0) where the band starts in a V8 sky.
    #[must_use]
    pub const fn bottom_rows(self) -> i32 {
        self.bottom_rows_in(&V8_SKY_BANDS)
    }

    /// The altitude (rows above row 0) where the band starts in a sky with `bands`.
    #[must_use]
    pub const fn bottom_rows_in(self, bands: &SkyBands) -> i32 {
        match self {
            Self::Ground => i32::MIN,
            Self::LowIsles => bands.low_isles,
            Self::Archipelago => bands.archipelago,
            Self::DeadAir => bands.dead_air,
            Self::Continents => bands.continents,
            Self::Orbit => bands.orbit,
            Self::Lunar => bands.moon,
        }
    }

    /// The band holding altitude `altitude_rows` (rows above row 0; negative underground) in a
    /// V8 sky.
    #[must_use]
    pub const fn at_altitude(altitude_rows: i32) -> Self {
        Self::at_altitude_in(altitude_rows, &V8_SKY_BANDS)
    }

    /// The band holding altitude `altitude_rows` in a sky with `bands`.
    #[must_use]
    pub const fn at_altitude_in(altitude_rows: i32, bands: &SkyBands) -> Self {
        if altitude_rows >= bands.moon {
            Self::Lunar
        } else if altitude_rows >= bands.orbit {
            Self::Orbit
        } else if altitude_rows >= bands.continents {
            Self::Continents
        } else if altitude_rows >= bands.dead_air {
            Self::DeadAir
        } else if altitude_rows >= bands.archipelago {
            Self::Archipelago
        } else if altitude_rows >= bands.low_isles {
            Self::LowIsles
        } else {
            Self::Ground
        }
    }

    /// The band of tile row `row` (row 0 is the classic surface line; rows above it are
    /// negative) in a V8 sky.
    #[must_use]
    pub const fn at_row(row: i32) -> Self {
        Self::at_altitude(row.saturating_neg())
    }

    /// Stable code (event payloads, the wire).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Ground => 0,
            Self::LowIsles => 1,
            Self::Archipelago => 2,
            Self::DeadAir => 3,
            Self::Continents => 4,
            Self::Orbit => 5,
            Self::Lunar => 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_tile_codes_are_distinct_and_fit_the_family() {
        let mut codes: Vec<u64> = SkyTile::ALL.iter().map(|tile| tile.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), SkyTile::ALL.len());
        assert!(codes.iter().all(|code| *code < 0x100));
        for tile in SkyTile::ALL {
            assert_eq!(SkyTile::from_code(tile.code()), Some(tile));
        }
        assert_eq!(SkyTile::from_code(0xFF), None);
    }

    #[test]
    fn bands_stack_bottom_to_top() {
        for pair in SkyBand::ALL.windows(2) {
            assert!(pair[0].bottom_rows() < pair[1].bottom_rows(), "{pair:?}");
            assert_eq!(SkyBand::at_altitude(pair[1].bottom_rows()), pair[1]);
            assert_eq!(SkyBand::at_altitude(pair[1].bottom_rows() - 1), pair[0]);
        }
        assert_eq!(SkyBand::at_row(10), SkyBand::Ground);
    }
}
