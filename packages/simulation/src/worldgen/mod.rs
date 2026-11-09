//! Seeded world generation.
//!
//! A world is a pure function of its [`WorldSettings`] (seed, size, richness, hazard intensity
//! and generator version). [`generate`] runs a layered pipeline, each layer deterministic and
//! integer-only:
//!
//! 1. **Bands** ([`strata`]): the fixed order of shallow layers and deep strata, with per-seed
//!    thickness, wavy boundaries and interleaved transition fingers.
//! 2. **Rock mass**: fractal noise picks each band's common or contrasting rock, so routes have
//!    texture (soft channels, hard shelves) instead of per-tile static.
//! 3. **Caves** ([`caves`]): smoothed noise caverns, worm tunnels and big deep chambers, kept
//!    away from the surface crust, the town foundations and the spawn shaft.
//! 4. **Hazards** ([`hazards`]): clustered pockets chosen by band rules and scaled by intensity.
//! 5. **Deposits** ([`deposits`]): ore veins along noise ridges, clusters and rare motherlodes
//!    from per-mineral depth curves, artifact beds and strays, then per-band ore budgets and a
//!    guaranteed starter field near spawn.
//! 6. **Features** ([`features`]): the index of notable places, for quests and prefabs to bind
//!    to later.
//!
//! From [`GeneratorVersion::V2`], **points of interest** ([`pois`]) are stamped between steps 4
//! and 5: hand-authored [`prefabs`] (caches, derelicts, vaults, formations, rival claims) in a
//! region grid, kept clear of hazards, spawn and the town; regions left empty stay reserved
//! prefab slots. Deposits then flow around them and count their ore toward the band budgets.
//!
//! Region-based placement uses [`noise::RegionRng`] seeded per region and purpose, so the
//! result never depends on iteration order across platforms.

pub mod caves;
pub mod deposits;
pub mod features;
pub mod hazards;
pub mod noise;
#[cfg(test)]
mod poi_tests;
pub mod pois;
pub mod prefabs;
pub mod report;
pub mod strata;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use crate::{
    surface::{SURFACE_BUILDINGS, building_foundation_at},
    terrain::{Tile, TileKind, tile_durability},
};

pub use features::{
    FeatureDetail, FeatureId, FeatureIndex, FeatureKind, HazardKind, TileRect, WorldFeature,
};
pub use strata::{Band, band_at};

use noise::{NOISE_MAX, fractal_noise};
use strata::SKY_ROWS;

/// Which generator built a world. Stored with the world so tuning never changes old worlds.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum GeneratorVersion {
    /// The original fixed-depth hash generator (saves from before seeded generation).
    Legacy,
    /// Seeded layered generation, first version (no points of interest).
    V1,
    /// V1 plus hand-authored prefab points of interest (caches, derelicts, vaults, formations
    /// and rival claims) stamped into a region grid.
    V2,
    /// V2 with per-seed band thicknesses (layer depths differ noticeably between seeds) and
    /// chunkier, less specky deep ore on rich worlds.
    #[default]
    V3,
}

impl GeneratorVersion {
    /// The generator new worlds use.
    pub const CURRENT: Self = Self::V3;

    /// Whether this generator stamps points of interest.
    #[must_use]
    pub const fn has_points_of_interest(self) -> bool {
        matches!(self, Self::V2 | Self::V3)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
        }
    }
}

/// World width option. Depth is set by the band stack and does not change.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum WorldSize {
    #[default]
    Standard,
    Large,
}

impl WorldSize {
    pub const ALL: [Self; 2] = [Self::Standard, Self::Large];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Large => "Large",
        }
    }

    #[must_use]
    pub const fn width_tiles(self) -> i32 {
        match self {
            Self::Standard => 240,
            Self::Large => 360,
        }
    }
}

/// How much ore a world holds.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ResourceRichness {
    Lean,
    #[default]
    Standard,
    Rich,
}

impl ResourceRichness {
    pub const ALL: [Self; 3] = [Self::Lean, Self::Standard, Self::Rich];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lean => "Lean",
            Self::Standard => "Standard",
            Self::Rich => "Rich",
        }
    }

    /// Deposit scale in percent of standard.
    #[must_use]
    pub const fn percent(self) -> u32 {
        match self {
            Self::Lean => 65,
            Self::Standard => 100,
            Self::Rich => 145,
        }
    }
}

/// How many hazards a world holds.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HazardIntensity {
    Calm,
    #[default]
    Standard,
    Brutal,
}

impl HazardIntensity {
    pub const ALL: [Self; 3] = [Self::Calm, Self::Standard, Self::Brutal];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Standard => "Standard",
            Self::Brutal => "Brutal",
        }
    }

    /// Hazard cluster scale in percent of standard.
    #[must_use]
    pub const fn percent(self) -> u32 {
        match self {
            Self::Calm => 45,
            Self::Standard => 100,
            Self::Brutal => 175,
        }
    }
}

/// Everything that defines a generated world.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct WorldSettings {
    pub seed: u64,
    pub size: WorldSize,
    pub richness: ResourceRichness,
    pub hazards: HazardIntensity,
    pub generator: GeneratorVersion,
}

/// Seed of the default world (tests, fixtures and scripted scenes).
pub const DEFAULT_WORLD_SEED: u64 = 0xD1_11_6A_4E;
/// Height in tiles of every world (the band stack).
pub const WORLD_HEIGHT_TILES: i32 = 220;
/// Column the first rig spawns above.
pub const SPAWN_TILE_X: i32 = 97;
/// Columns either side of the town whose topsoil is plain dirt (no clay lumps).
pub const TOWN_CLEAN_TOPSOIL_MARGIN: i32 = 20;

impl Default for WorldSettings {
    fn default() -> Self {
        Self::new(DEFAULT_WORLD_SEED)
    }
}

impl WorldSettings {
    /// Standard settings with the current generator.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            size: WorldSize::Standard,
            richness: ResourceRichness::Standard,
            hazards: HazardIntensity::Standard,
            generator: GeneratorVersion::CURRENT,
        }
    }

    /// Settings describing a world built by the original fixed-depth generator.
    #[must_use]
    pub const fn legacy(seed: u64) -> Self {
        Self {
            generator: GeneratorVersion::Legacy,
            ..Self::new(seed)
        }
    }

    #[must_use]
    pub const fn width_tiles(&self) -> i32 {
        self.size.width_tiles()
    }

    #[must_use]
    pub const fn height_tiles(&self) -> i32 {
        WORLD_HEIGHT_TILES
    }

    /// The seed as players see and share it (see [`parse_seed`]).
    #[must_use]
    pub fn seed_text(&self) -> String {
        format_seed(self.seed)
    }
}

/// Formats a seed as upper-case hex in groups of four, e.g. `D111-6A4E`.
#[must_use]
pub fn format_seed(seed: u64) -> String {
    let hex = format!("{seed:X}");
    let mut groups: Vec<&str> = Vec::new();
    let first = hex.len() % 4;
    if first > 0 {
        groups.push(&hex[..first]);
    }
    let mut start = first;
    while start < hex.len() {
        groups.push(&hex[start..start + 4]);
        start += 4;
    }
    groups.join("-")
}

/// Parses a typed or pasted seed.
///
/// Hex digits (with optional `-`, `_` or spaces between groups,
/// or a `0x` prefix) are read as the number [`format_seed`] prints; any other text is hashed, so
/// words work as seeds too. Returns `None` for empty text.
#[must_use]
pub fn parse_seed(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let unprefixed = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let digits: String = unprefixed
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    if !digits.is_empty()
        && digits.len() <= 16
        && digits.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(value) = u64::from_str_radix(&digits, 16)
    {
        return Some(value);
    }
    // FNV-1a over the text.
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    for byte in trimmed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    }
    Some(hash)
}

/// A generated world: its tiles (row-major) and feature index.
#[derive(Clone, Debug)]
pub struct GeneratedWorld {
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<Tile>,
    pub features: FeatureIndex,
}

/// Generates the world `settings` describe.
#[must_use]
pub fn generate(settings: &WorldSettings) -> GeneratedWorld {
    let (width, height) = (settings.width_tiles(), settings.height_tiles());
    if settings.generator == GeneratorVersion::Legacy {
        let tiles = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let kind = crate::terrain::legacy_tile_kind(x, y, settings.seed);
                Tile {
                    kind,
                    durability: tile_durability(kind),
                }
            })
            .collect();
        return GeneratedWorld {
            width,
            height,
            tiles,
            features: FeatureIndex::default(),
        };
    }
    let mut canvas = Canvas::new(settings);
    canvas.fill_rock();
    caves::carve(&mut canvas);
    hazards::place(&mut canvas);
    if settings.generator.has_points_of_interest() {
        pois::place(&mut canvas);
    }
    deposits::place(&mut canvas);
    canvas.index_layout_features();
    let tiles = canvas
        .kinds
        .iter()
        .map(|kind| Tile {
            kind: *kind,
            durability: tile_durability(*kind),
        })
        .collect();
    GeneratedWorld {
        width,
        height,
        tiles,
        features: canvas.features,
    }
}

/// The world being generated.
pub(crate) struct Canvas<'s> {
    pub settings: &'s WorldSettings,
    pub width: i32,
    pub height: i32,
    pub kinds: Vec<TileKind>,
    bands: Vec<u8>,
    pub features: FeatureIndex,
}

/// First and last column of the town (building lots).
pub(crate) const fn town_columns() -> (i32, i32) {
    let first = SURFACE_BUILDINGS[0];
    let last = SURFACE_BUILDINGS[SURFACE_BUILDINGS.len() - 1];
    (first.tile_x, last.tile_x + last.tile_width - 1)
}

impl<'s> Canvas<'s> {
    fn new(settings: &'s WorldSettings) -> Self {
        let (width, height) = (settings.width_tiles(), settings.height_tiles());
        let mut bands = Vec::with_capacity(usize::try_from(width * height).unwrap_or_default());
        for y in 0..height {
            for x in 0..width {
                #[allow(clippy::cast_possible_truncation, reason = "there are twelve bands")]
                bands.push(band_at(settings, x, y).index() as u8);
            }
        }
        Self {
            settings,
            width,
            height,
            kinds: vec![TileKind::Air; bands.len()],
            bands,
            features: FeatureIndex::default(),
        }
    }

    pub const fn seed(&self) -> u64 {
        self.settings.seed
    }

    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        usize::try_from(y * self.width + x).ok()
    }

    pub fn get(&self, x: i32, y: i32) -> Option<TileKind> {
        self.index(x, y).map(|index| self.kinds[index])
    }

    pub fn set(&mut self, x: i32, y: i32, kind: TileKind) {
        if let Some(index) = self.index(x, y) {
            self.kinds[index] = kind;
        }
    }

    pub fn band(&self, x: i32, y: i32) -> Band {
        let x = x.clamp(0, self.width - 1);
        let y = y.clamp(0, self.height - 1);
        self.index(x, y).map_or(Band::Topsoil, |index| {
            Band::from_index(self.bands[index].into())
        })
    }

    /// Whether `(x, y)` holds plain rock that deposits and hazards may replace.
    pub fn is_plain_rock(&self, x: i32, y: i32) -> bool {
        matches!(
            self.get(x, y),
            Some(TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock)
        )
    }

    /// Step 2: sky, town foundations and each band's rock mass.
    fn fill_rock(&mut self) {
        let seed = self.seed();
        for y in 0..self.height {
            for x in 0..self.width {
                let kind = if y < SKY_ROWS {
                    TileKind::Air
                } else if building_foundation_at(x, y) {
                    TileKind::Foundation
                } else {
                    let band = self.band(x, y);
                    let (cell_x, cell_y, contrast_per_mille) = rock_texture(band);
                    let mass = fractal_noise(seed, 0x0C0C_0000, x, y, cell_x, cell_y, 3);
                    // The starting drill only digs dirt: keep the topsoil around the town free of
                    // clay lumps so the first dives never hit a wall.
                    let (town_first, town_last) = town_columns();
                    let starter_ground = band == Band::Topsoil
                        && x >= town_first - TOWN_CLEAN_TOPSOIL_MARGIN
                        && x <= town_last + TOWN_CLEAN_TOPSOIL_MARGIN;
                    if !starter_ground && mass > contrast_threshold(contrast_per_mille) {
                        band.hard_material()
                    } else {
                        band.soft_material()
                    }
                };
                self.set(x, y, kind);
            }
        }
    }

    /// Step 6 (layout part): band transitions, plus reserved prefab slots in V1 worlds (V2+
    /// reserves its slots while placing POIs).
    fn index_layout_features(&mut self) {
        for index in 0..strata::BOUNDARY_COUNT {
            let depths: Vec<i32> = (0..self.width)
                .map(|x| strata::boundary_column_depth(self.settings, index, x))
                .collect();
            let (deepest_x, deepest) = depths
                .iter()
                .copied()
                .enumerate()
                .max_by_key(|(x, depth)| (*depth, -i64::try_from(*x).unwrap_or_default()))
                .unwrap_or_default();
            let shallowest = depths.iter().copied().min().unwrap_or(deepest);
            let deepest_x = i32::try_from(deepest_x).unwrap_or_default();
            let upper = Band::from_index(index);
            let lower = Band::from_index(index + 1);
            self.features.push(
                FeatureKind::StratumTransition,
                FeatureDetail::Transition { upper, lower },
                lower,
                crate::terrain::TilePosition {
                    x: deepest_x,
                    y: deepest,
                },
                TileRect {
                    min: crate::terrain::TilePosition {
                        x: 0,
                        y: shallowest,
                    },
                    max: crate::terrain::TilePosition {
                        x: self.width - 1,
                        y: deepest,
                    },
                },
                u32::try_from(self.width).unwrap_or_default(),
            );
        }
        if !self.settings.generator.has_points_of_interest() {
            deposits::reserve_prefab_slots(self);
        }
    }
}

/// Rock texture of a band: noise cell size and the share (per mille) of contrasting rock.
const fn rock_texture(band: Band) -> (i32, i32, u32) {
    use crate::terrain::DeepStratum;
    match band {
        Band::Topsoil => (9, 5, 70),
        Band::Clay => (16, 8, 220),
        Band::Stone => (18, 9, 260),
        Band::Slate => (28, 6, 360),
        Band::Basalt => (14, 10, 300),
        Band::Deep(DeepStratum::CrystalFaults) => (10, 16, 360),
        Band::Deep(DeepStratum::FossilOceans) => (30, 7, 300),
        Band::Deep(DeepStratum::PressureCathedrals) => (12, 18, 320),
        Band::Deep(DeepStratum::RadioactiveHollow) => (16, 10, 300),
        Band::Deep(DeepStratum::AncientMachineLayer) => (24, 8, 340),
        Band::Deep(DeepStratum::VoidGeodeFields) => (14, 14, 280),
        Band::Deep(DeepStratum::MantleStormZone) => (20, 8, 320),
    }
}

/// Noise threshold above which roughly `per_mille` of fractal noise samples lie.
///
/// Fractal noise is bell-shaped, so the share is mapped through a small measured table rather
/// than linearly.
pub(crate) const fn contrast_threshold(per_mille: u32) -> i32 {
    // (share above, threshold) pairs measured over fractal_noise with 3 octaves.
    const TABLE: [(u32, i32); 11] = [
        (0, NOISE_MAX + 1),
        (20, 59_800),
        (50, 55_300),
        (100, 50_900),
        (150, 47_800),
        (200, 45_300),
        (300, 40_900),
        (400, 37_000),
        (500, 33_300),
        (700, 25_500),
        (1000, 0),
    ];
    let mut index = 1;
    while index < TABLE.len() && TABLE[index].0 < per_mille {
        index += 1;
    }
    if index >= TABLE.len() {
        return 0;
    }
    let (low_share, low_threshold) = TABLE[index - 1];
    let (high_share, high_threshold) = TABLE[index];
    let span = (high_share - low_share).cast_signed();
    if span == 0 {
        return high_threshold;
    }
    let along = (per_mille - low_share).cast_signed();
    low_threshold + (high_threshold - low_threshold) * along / span
}
