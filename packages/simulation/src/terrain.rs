#![allow(
    clippy::cast_sign_loss,
    reason = "terrain bounds are validated before indexing"
)]

use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

mod persistence;

use serde::{Deserialize, Serialize};

use crate::{
    cave::CaveTile,
    poi::PoiTile,
    position::Frame,
    surface::building_foundation_at,
    worldgen::{
        self, Band, FeatureId, FeatureIndex, FeatureKind, TileRect, WorldFeature, WorldSettings,
        streamed::GeneratedChunk,
    },
};

pub use persistence::{
    FullTerrain, MAX_TERRAIN_SIDE_TILES, TerrainRestoreError, TerrainSnapshot, TileDiffs,
};
use persistence::{GeneratedBase, generated_base};

/// World-space pixels per terrain tile.
pub const TILE_SIZE: f32 = 32.0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum MineralKind {
    Copper,
    Iron,
    Silver,
    Gold,
    Emerald,
    Ruby,
    Diamond,
    Platinum,
    Uranium,
    Mythril,
    /// Sky ore of the low isles and cloud archipelagos (WP-SKY, generator V9): pale blue
    /// stone threaded with light, about as hard as clay.
    Skystone,
    /// Sky ore of the cloud archipelagos' deeper rock: silver that rings like a bell, as hard as
    /// stone.
    CloudSilver,
}

impl MineralKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
            Self::Emerald => "Emerald",
            Self::Ruby => "Ruby",
            Self::Diamond => "Diamond",
            Self::Platinum => "Platinum",
            Self::Uranium => "Uranium",
            Self::Mythril => "Mythril",
            Self::Skystone => "Skystone",
            Self::CloudSilver => "Cloud Silver",
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Copper => 8,
            Self::Iron => 14,
            Self::Silver => 26,
            Self::Gold => 44,
            Self::Emerald => 78,
            Self::Ruby => 120,
            Self::Diamond => 190,
            Self::Platinum => 260,
            Self::Uranium => 360,
            Self::Mythril => 520,
            Self::Skystone => 58,
            Self::CloudSilver => 150,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum StrategicResourceKind {
    AncientAlloy,
    CoreShard,
    CrystalLens,
    PressurePearl,
    MachineRelic,
    VoidGlass,
    RadiantFossil,
    /// Glowing crystal from around a sky continent's aether core (V9 skies; Act II weapons).
    AetherCrystal,
    /// Lightning-fused glass from the sky continents' mountains (V9 skies; Act II weapons).
    StormGlass,
    /// Helium-3 from lunar regolith (WP-MOON): the colony pays well, the reactor runs on it.
    Helium3,
    /// Moon ice: melts into premium fuel at the lunar colony.
    MoonIce,
    /// Alien alloy from veins deep in the Moon: endgame drill and weapon tiers.
    AlienAlloy,
    // Night-only creature drops (WP-NIGHT, see `crate::night::night_material_value`).
    /// Dust-silver silk from a moth swarm.
    MothSilk,
    /// The smoke-dark hide of a shadow hound.
    ShadowPelt,
    /// A dune stalker's glassy fang.
    StalkerFang,
    /// The cold light of a will-o'-wisp, bottled.
    WispEssence,
    /// The heart of an umbral alpha (new-moon nights only).
    UmbralHeart,
}

impl StrategicResourceKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AncientAlloy => "Ancient Alloy",
            Self::CoreShard => "Core Shard",
            Self::CrystalLens => "Crystal Lens",
            Self::PressurePearl => "Pressure Pearl",
            Self::MachineRelic => "Machine Relic",
            Self::VoidGlass => "Void Glass",
            Self::RadiantFossil => "Radiant Fossil",
            Self::AetherCrystal => "Aether Crystal",
            Self::StormGlass => "Storm Glass",
            Self::Helium3 => "Helium-3",
            Self::MoonIce => "Moon Ice",
            Self::AlienAlloy => "Alien Alloy",
            Self::MothSilk => "Moth Silk",
            Self::ShadowPelt => "Shadow Pelt",
            Self::StalkerFang => "Stalker Fang",
            Self::WispEssence => "Wisp Essence",
            Self::UmbralHeart => "Umbral Heart",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DeepStratum {
    CrystalFaults,
    FossilOceans,
    PressureCathedrals,
    RadioactiveHollow,
    AncientMachineLayer,
    VoidGeodeFields,
    MantleStormZone,
}

impl DeepStratum {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CrystalFaults => "Crystal Faults",
            Self::FossilOceans => "Fossil Oceans",
            Self::PressureCathedrals => "Pressure Cathedrals",
            Self::RadioactiveHollow => "Radioactive Hollow",
            Self::AncientMachineLayer => "Ancient Machine Layer",
            Self::VoidGeodeFields => "Void Geode Fields",
            Self::MantleStormZone => "Mantle Storm Zone",
        }
    }

    #[must_use]
    pub const fn primary_hazard(self) -> TileKind {
        match self {
            Self::CrystalFaults | Self::AncientMachineLayer => TileKind::ExplosivePocket,
            Self::FossilOceans => TileKind::Gas,
            Self::PressureCathedrals | Self::VoidGeodeFields => TileKind::PressurePocket,
            Self::RadioactiveHollow => TileKind::MagmaVent,
            Self::MantleStormZone => TileKind::Lava,
        }
    }
}

/// The deep stratum of row `depth` in a legacy (fixed-depth) world.
const fn legacy_deep_stratum(depth: i32) -> Option<DeepStratum> {
    match depth {
        90..=109 => Some(DeepStratum::CrystalFaults),
        110..=129 => Some(DeepStratum::FossilOceans),
        130..=149 => Some(DeepStratum::PressureCathedrals),
        150..=169 => Some(DeepStratum::RadioactiveHollow),
        170..=189 => Some(DeepStratum::AncientMachineLayer),
        190..=209 => Some(DeepStratum::VoidGeodeFields),
        210.. => Some(DeepStratum::MantleStormZone),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ArtifactKind {
    Fossil,
    OldCircuit,
    BuriedIdol,
    StarCore,
    CrystalPrism,
    RadiantShell,
    PressureGlyph,
    ReactorBloom,
    MachineSigil,
    VoidPearl,
    MantleHeart,
    /// A sky artifact (WP-SKY, V9): a brass astrolabe from the first aeronauts, found in island
    /// ruins and caches.
    WindAstrolabe,
    /// A sky artifact: a pearl grown in a cloud, found in the archipelagos' nests and caches.
    CloudPearl,
    /// A harpy's egg, taken from a nest on a sky continent's peaks (V9 skies).
    HarpyEgg,
    /// A relic from a sky temple's vault (V9 skies).
    SkyRelic,
    /// A relic of the Moon's first visitors, from caches in the lunar rock and habitats.
    LunarRelic,
    /// The Warden's eye: the alien facility guardian's core (WP-MOON boss).
    WardenEye,
    /// A clay pot from the Crust's Clay Beds (Wave 8, Deep Descent worlds): somebody shaped the
    /// beds and left pots in them; collectors in town pay for each one.
    ClayPot,
}

impl ArtifactKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fossil => "Fossil",
            Self::OldCircuit => "Old Circuit",
            Self::BuriedIdol => "Buried Idol",
            Self::StarCore => "Star Core",
            Self::CrystalPrism => "Crystal Prism",
            Self::RadiantShell => "Radiant Shell",
            Self::PressureGlyph => "Pressure Glyph",
            Self::ReactorBloom => "Reactor Bloom",
            Self::MachineSigil => "Machine Sigil",
            Self::VoidPearl => "Void Pearl",
            Self::MantleHeart => "Mantle Heart",
            Self::WindAstrolabe => "Wind Astrolabe",
            Self::CloudPearl => "Cloud Pearl",
            Self::HarpyEgg => "Harpy Egg",
            Self::SkyRelic => "Sky Relic",
            Self::LunarRelic => "Lunar Relic",
            Self::WardenEye => "Warden's Eye",
            Self::ClayPot => "Clay Pot",
        }
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Fossil => 130,
            Self::OldCircuit => 240,
            Self::BuriedIdol => 420,
            Self::StarCore => 760,
            Self::CrystalPrism => 900,
            Self::RadiantShell => 980,
            Self::PressureGlyph => 1_080,
            Self::ReactorBloom => 1_180,
            Self::MachineSigil => 1_260,
            Self::VoidPearl => 1_420,
            Self::MantleHeart => 1_650,
            Self::WindAstrolabe => 640,
            Self::CloudPearl => 880,
            Self::HarpyEgg => 1_200,
            Self::SkyRelic => 2_400,
            Self::LunarRelic => 1_900,
            Self::WardenEye => 4_800,
            Self::ClayPot => 70,
        }
    }

    /// Whether it comes from the Moon (the lunar colony buys these).
    #[must_use]
    pub const fn is_lunar(self) -> bool {
        matches!(self, Self::LunarRelic | Self::WardenEye)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum TileKind {
    Air,
    Dirt,
    Clay,
    Stone,
    HardRock,
    Foundation,
    Lava,
    Gas,
    ExplosivePocket,
    PressurePocket,
    MagmaVent,
    Ore(MineralKind),
    Artifact(ArtifactKind),
    /// A point-of-interest fixture (crates, wreckage, vault parts, formations, outposts).
    Poi(PoiTile),
    /// The floor of a deep world: nothing drills, blasts or digs through it (see
    /// [`UNBREAKABLE_HARDNESS`]).
    Bedrock,
    /// A cave-ecosystem tile (crystal, fungus, obsidian, ruins, stalactites, caches; see
    /// [`crate::cave`]).
    Cave(CaveTile),
    /// A solid building piece placed in a claim (WP-BASE, [`crate::base`]): no drill or blast
    /// breaks it; it comes down only by deconstruction.
    Built(crate::base::BuiltTile),
    /// A surface-biome material (sand, sandstone, red rock, snow, granite, mud): the crusts
    /// and mountain masses of [`GeneratorVersion::V7`](worldgen::GeneratorVersion::V7) worlds.
    Surface(SurfaceMaterial),
    /// A sky-land tile (floating islands, sky continents and the Moon of
    /// [`GeneratorVersion::V8`](worldgen::GeneratorVersion::V8) worlds; see [`crate::sky`]).
    Sky(crate::sky::SkyTile),
    /// A tile of the filled Moon ([`GeneratorVersion::V9`](worldgen::GeneratorVersion::V9)
    /// worlds; see [`crate::moon`]).
    Lunar(crate::moon::LunarTile),
    /// A fluid cell (Part III F9, [`crate::fluid`]): open ground for everything that moves; the
    /// tile's `durability` byte is the fluid's level ([`crate::fluid::FLUID_FULL`] per full
    /// tile). Only V11+ worlds generate fluid; any world can hold it once it flows.
    Fluid(crate::fluid::FluidKind),
    /// A tile of the authored Crust (Wave 8, Descent I of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see [`crate::crust`]):
    /// root mats, mine timber and rails, slate shelves, basalt columns, tube glass, the cabin,
    /// strongboxes, the Sinkhole's core, rubble and Gravel Hollow's gravel.
    Crust(crate::crust::CrustTile),
    /// A tile of the Old Deep's strata (Wave 8, Descent II of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see
    /// [`crate::old_deep`]).
    Deep(crate::old_deep::DeepTile),
    /// A tile of the Hollow World's strata (Wave 9, Descent V of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see
    /// [`crate::hollow`]): sky stone and amber, jungle loam, ferns, trunks and tar, the isles'
    /// rock, sun rock and glass, the Sun Well and the inner sun.
    Hollow(crate::hollow::HollowTile),
    /// A tile of the distortion strata (Wave 9, Descent VI part A of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see
    /// [`crate::distortion`]): mirror glass and mirror ore, flow stone, data ore, rotten blocks.
    Warped(crate::distortion::WarpedTile),
    /// A tile of the builders' buried city (Wave 10 ancients-a, Descent VII strata 1-3 of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see
    /// [`crate::ancient_city`]): city stone, tower glass, tram rails and platforms, lamps, lift
    /// rails, archivist cores, engine plating, belts, piston housings, archive stone and
    /// shelving, star glass and star chart plinths.
    City(crate::ancient_city::CityTile),
    /// A tile of the Heart's strata (Wave 10, Descent VIII of
    /// [`GeneratorVersion::V11`](worldgen::GeneratorVersion::V11) worlds; see [`crate::heart`]):
    /// vein shale and dark matter, event glass and singularities, the metal sea's rails and
    /// pylons, star rock, the Heart's shell and seal, the builders' cable.
    Core(crate::heart::CoreTile),
}

/// The ground materials surface biomes are made of ([`crate::worldgen::biome`]).
///
/// Hardness follows the classic ladder: sand, snow and mud dig like dirt, sandstone and red
/// rock like clay, and mountain granite like stone (an upgraded drill, or fly over).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SurfaceMaterial {
    /// Desert dunes and beaches.
    Sand,
    /// Layered desert crust and mesa bands.
    Sandstone,
    /// Badland mesa rock.
    RedRock,
    /// Snow caps and tundra ground.
    Snow,
    /// The body of a mountain.
    Granite,
    /// Wetland lake beds and bogs.
    Mud,
}

impl SurfaceMaterial {
    /// Every material, in code order.
    pub const ALL: [Self; 6] = [
        Self::Sand,
        Self::Sandstone,
        Self::RedRock,
        Self::Snow,
        Self::Granite,
        Self::Mud,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sand => "sand",
            Self::Sandstone => "sandstone",
            Self::RedRock => "red rock",
            Self::Snow => "snow",
            Self::Granite => "granite",
            Self::Mud => "mud",
        }
    }

    /// Stable code (part of [`Tile::code`] and so of terrain digests).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Sand => 0,
            Self::Sandstone => 1,
            Self::RedRock => 2,
            Self::Snow => 3,
            Self::Granite => 4,
            Self::Mud => 5,
        }
    }

    /// Drill strength needed to dig it.
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::Sand | Self::Snow | Self::Mud => 1,
            Self::Sandstone | Self::RedRock => 2,
            Self::Granite => 3,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Snow => 1,
            Self::Sand => 2,
            Self::Mud => 3,
            Self::Sandstone => 4,
            Self::RedRock => 5,
            Self::Granite => 8,
        }
    }
}

impl TileKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Air => "air",
            Self::Dirt => "dirt",
            Self::Clay => "clay",
            Self::Stone => "stone",
            Self::HardRock => "hard rock",
            Self::Foundation => "foundation",
            Self::Lava => "lava",
            Self::Gas => "gas",
            Self::ExplosivePocket => "explosive pocket",
            Self::PressurePocket => "pressure pocket",
            Self::MagmaVent => "magma vent",
            Self::Ore(mineral) => mineral.name(),
            Self::Artifact(artifact) => artifact.name(),
            Self::Poi(tile) => tile.name(),
            Self::Bedrock => "bedrock",
            Self::Cave(tile) => tile.name(),
            Self::Built(tile) => tile.name(),
            Self::Surface(material) => material.name(),
            Self::Sky(tile) => tile.name(),
            Self::Lunar(tile) => tile.name(),
            Self::Fluid(fluid) => fluid.name(),
            Self::Crust(tile) => tile.name(),
            Self::Deep(tile) => tile.name(),
            Self::Hollow(tile) => tile.name(),
            Self::Warped(tile) => tile.name(),
            Self::City(tile) => tile.name(),
            Self::Core(tile) => tile.name(),
        }
    }

    /// Whether bodies move through the tile: open air or fluid ([`Self::Fluid`]). Everything
    /// else is solid ground.
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Air | Self::Fluid(_))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tile {
    pub kind: TileKind,
    pub durability: u8,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct TilePosition {
    pub x: i32,
    pub y: i32,
}

/// Width and height, in tiles, of one terrain chunk (the unit of discovery storage and of
/// terrain relevance refreshes).
pub const CHUNK_SIZE_TILES: i32 = 16;

/// A square block of [`CHUNK_SIZE_TILES`] x [`CHUNK_SIZE_TILES`] tiles.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ChunkPosition {
    pub x: i32,
    pub y: i32,
}

impl ChunkPosition {
    /// The chunk containing `tile`.
    #[must_use]
    pub const fn of(tile: TilePosition) -> Self {
        Self {
            x: tile.x.div_euclid(CHUNK_SIZE_TILES),
            y: tile.y.div_euclid(CHUNK_SIZE_TILES),
        }
    }

    /// The chunk's top-left tile.
    #[must_use]
    pub const fn origin(self) -> TilePosition {
        TilePosition {
            x: self.x * CHUNK_SIZE_TILES,
            y: self.y * CHUNK_SIZE_TILES,
        }
    }

    /// Index of `tile` inside its chunk, `0..CHUNK_SIZE_TILES^2`, row-major.
    #[must_use]
    pub const fn local_index(tile: TilePosition) -> usize {
        (tile.y.rem_euclid(CHUNK_SIZE_TILES) * CHUNK_SIZE_TILES
            + tile.x.rem_euclid(CHUNK_SIZE_TILES)) as usize
    }
}

/// One terrain tile write: the op-log replication unit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TileChange {
    pub position: TilePosition,
    pub tile: Tile,
}

impl Tile {
    /// Stable 64-bit code of the tile's contents (kind, payload and durability).
    #[must_use]
    pub const fn code(self) -> u64 {
        let kind: u64 = match self.kind {
            TileKind::Air => 0,
            TileKind::Dirt => 1,
            TileKind::Clay => 2,
            TileKind::Stone => 3,
            TileKind::HardRock => 4,
            TileKind::Foundation => 5,
            TileKind::Lava => 6,
            TileKind::Gas => 7,
            TileKind::ExplosivePocket => 8,
            TileKind::PressurePocket => 9,
            TileKind::MagmaVent => 10,
            TileKind::Ore(mineral) => 0x100 | mineral as u64,
            TileKind::Artifact(artifact) => 0x200 | artifact as u64,
            TileKind::Poi(tile) => 0x300 | tile.code(),
            TileKind::Bedrock => 11,
            TileKind::Cave(tile) => 0x400 | tile.code(),
            TileKind::Built(tile) => BUILT_TILE_CODE_BASE | tile.code(),
            TileKind::Surface(material) => 0x500 | material.code(),
            TileKind::Sky(tile) => 0x600 | tile.code(),
            TileKind::Lunar(tile) => 0x700 | tile.code(),
            TileKind::Fluid(fluid) => FLUID_TILE_CODE_BASE | fluid.code(),
            TileKind::Crust(tile) => CRUST_TILE_CODE_BASE | tile.code(),
            TileKind::Deep(tile) => crate::old_deep::DEEP_TILE_CODE_BASE | tile.code(),
            TileKind::Hollow(tile) => crate::hollow::HOLLOW_TILE_CODE_BASE | tile.code(),
            TileKind::Warped(tile) => crate::distortion::WARPED_TILE_CODE_BASE | tile.code(),
            TileKind::City(tile) => crate::ancient_city::CITY_TILE_CODE_BASE | tile.code(),
            TileKind::Core(tile) => crate::heart::CORE_TILE_CODE_BASE | tile.code(),
        };
        (kind << 8) | self.durability as u64
    }
}

/// Kind-code range of [`TileKind::Built`] tiles in [`Tile::code`] (`0xB00 | BuiltTile::code`),
/// kept clear of the ranges other tile families use.
pub const BUILT_TILE_CODE_BASE: u64 = 0xB00;

/// Kind-code range of [`TileKind::Crust`] tiles in [`Tile::code`] (`0xC00 | CrustTile::code`,
/// Wave 8), clear of every other family's range.
pub const CRUST_TILE_CODE_BASE: u64 = 0xC00;

/// Kind-code range of [`TileKind::Fluid`] tiles in [`Tile::code`] (`0xA00 | FluidKind::code`;
/// the tile's durability byte is the fluid's level).
pub const FLUID_TILE_CODE_BASE: u64 = 0xA00;

/// Order-independent digest contribution of one tile at one index (bounded terrain).
const fn tile_digest(index: usize, tile: Tile) -> u64 {
    // splitmix64 finalizer over the index and tile code.
    let mut z = (index as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(tile.code().wrapping_mul(0xD6E8_FEB8_6659_FD93));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Order-independent digest contribution of one edited tile of a streamed terrain (keyed by
/// position, so it is the same on 32- and 64-bit targets).
const fn position_digest(position: TilePosition, tile: Tile) -> u64 {
    let key = ((position.x as u32 as u64) << 32) | (position.y as u32 as u64);
    let mut z = key
        .wrapping_mul(0xA24B_AED4_963E_E407)
        .wrapping_add(tile.code().wrapping_mul(0x9FB2_1C65_1E98_DF25));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A terrain grid with its generation settings.
///
/// Two storage models share one API:
///
/// * **Bounded** (generators up to V4): a fixed `width` x `height` grid of tiles, the
///   compatibility path for every older world.
/// * **Streamed** ([`GeneratorVersion::V5`](worldgen::GeneratorVersion::V5)): unbounded columns
///   (to [`worldgen::streamed::STREAMED_COLUMN_LIMIT`] either side) and a finite height. Chunks
///   are generated on demand from the settings; only the tiles players changed are stored
///   ("edits", per chunk). Chunks near players are kept loaded for fast access
///   ([`Self::stream_around`]); unloading a chunk keeps its edits, so nothing is lost. Which
///   chunks are loaded never affects any tile, hash or result.
///
/// Serializes as a [`TerrainSnapshot`]: the settings, the size, a digest and only the tiles
/// that differ from freshly generated terrain (see [`persistence`]).
#[derive(Clone, Debug)]
pub struct Terrain {
    height: i32,
    /// How the terrain was generated (seed, size, richness, hazards, generator version).
    settings: WorldSettings,
    storage: Storage,
    /// Bounded: running sum of [`tile_digest`] over every tile. Streamed: running sum of
    /// [`position_digest`] over every edit. Maintained on each write so hashing the world does
    /// not walk the tiles.
    digest: u64,
    /// Tile writes recorded while a simulation tick runs (never serialized or hashed).
    journal: Option<Vec<TileChange>>,
    /// The kind each journalled write replaced, in journal order (Wave 8: the Living Dark
    /// tells a mined root or a burst spore pod from any other opening; never serialized).
    journal_before: Vec<TileKind>,
    /// The local pixel space every gameplay position is measured in (F7, see
    /// [`crate::position`]): local pixel `(x, y)` is in tile [`Frame::tile_at`]. Saved with
    /// the terrain; owned by the world (it moves only in `WorldState::recentre_frame`).
    frame: Frame,
    /// A streamed world's column limit raised past its generator's (test and tool worlds far
    /// from the origin). Carried by the terrain snapshot (saves and keyframes), not hashed.
    column_limit_override: Option<i32>,
}

#[derive(Clone, Debug)]
enum Storage {
    Bounded(BoundedTiles),
    Streamed(StreamedTiles),
}

#[derive(Clone, Debug)]
struct BoundedTiles {
    width: i32,
    tiles: Vec<Tile>,
    /// The freshly generated terrain and feature index of the settings at this size, shared
    /// through a per-process cache and computed on first use (never serialized, replicated or
    /// hashed).
    base: OnceLock<Arc<GeneratedBase>>,
}

#[derive(Clone, Debug, Default)]
struct StreamedTiles {
    /// Chunks kept in memory for fast access (a cache: never serialized or hashed).
    loaded: BTreeMap<ChunkPosition, LoadedChunk>,
    /// Every tile that differs from generation, by chunk and [`ChunkPosition::local_index`].
    edits: BTreeMap<ChunkPosition, BTreeMap<u16, Tile>>,
}

#[derive(Clone, Debug)]
struct LoadedChunk {
    base: Arc<GeneratedChunk>,
    /// The chunk's current tiles once it has edits (copy-on-write; `None` reads `base`).
    tiles: Option<Arc<Vec<Tile>>>,
}

impl LoadedChunk {
    fn new(base: Arc<GeneratedChunk>, edits: Option<&BTreeMap<u16, Tile>>) -> Self {
        let tiles = edits.filter(|edits| !edits.is_empty()).map(|edits| {
            let mut tiles = base.tiles().to_vec();
            for (local, tile) in edits {
                if let Some(slot) = tiles.get_mut(usize::from(*local)) {
                    *slot = *tile;
                }
            }
            Arc::new(tiles)
        });
        Self { base, tiles }
    }

    fn tile(&self, local: usize) -> Tile {
        self.tiles
            .as_ref()
            .map_or_else(|| self.base.tile(local), |tiles| tiles[local])
    }
}

/// Where a tile lives.
#[derive(Clone, Copy, Debug)]
enum Slot {
    Bounded(usize),
    Streamed(ChunkPosition, usize),
}

/// The heap the browser build budgets for a streamed world's terrain and the generator's
/// per-process caches (Wave 7 F8 streaming budget).
///
/// The emscripten build grows its memory up to 2 GiB; the simulation's terrain share must stay
/// well inside that so a Deep Descent world explored to the bottom never runs the tab out of
/// memory. Measured natively by `tests/deep_descent_memory.rs` (an upper bound for the 32-bit
/// WASM heap).
pub const WASM_TERRAIN_BUDGET_BYTES: usize = 256 << 20;

/// Chunks kept loaded around each centre by [`Terrain::stream_around`]: `(columns, rows)` of
/// chunks either side (about two screens).
pub const STREAM_LOAD_RADIUS_CHUNKS: (i32, i32) = (4, 3);
/// Loaded chunks further than this from every centre are unloaded (hysteresis over
/// [`STREAM_LOAD_RADIUS_CHUNKS`], so a rig moving back and forth does not thrash).
pub const STREAM_KEEP_RADIUS_CHUNKS: (i32, i32) = (6, 5);

impl Terrain {
    #[cfg(test)]
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        Self::new_seeded(width, height, 0xD1_11_6A_4E)
    }

    /// A `width` x `height` terrain from the legacy fixed-depth generator (small test and
    /// harness worlds).
    #[must_use]
    pub fn new_seeded(width: i32, height: i32, seed: u64) -> Self {
        let settings = WorldSettings::legacy(seed);
        Self::from_base(
            settings,
            width,
            height,
            generated_base(&settings, width, height),
        )
    }

    /// Generates the world `settings` describe (see [`crate::worldgen`]): a streamed terrain
    /// for streamed generators, otherwise the bounded grid.
    #[must_use]
    pub fn generate(settings: &WorldSettings) -> Self {
        if settings.generator.is_streamed() {
            return Self::streamed(*settings);
        }
        let (width, height) = (settings.width_tiles(), settings.height_tiles());
        Self::from_base(
            *settings,
            width,
            height,
            generated_base(settings, width, height),
        )
    }

    /// A streamed terrain holding exactly the generated world (no edits).
    fn streamed(settings: WorldSettings) -> Self {
        Self {
            height: settings.height_tiles(),
            settings,
            storage: Storage::Streamed(StreamedTiles::default()),
            digest: 0,
            journal: None,
            journal_before: Vec::new(),
            frame: Frame::for_generator(settings.generator),
            column_limit_override: None,
        }
    }

    /// A terrain holding exactly the generated `base`.
    fn from_base(
        settings: WorldSettings,
        width: i32,
        height: i32,
        base: Arc<GeneratedBase>,
    ) -> Self {
        let mut terrain = Self::from_tiles(settings, width, height, base.tiles.clone());
        if let Storage::Bounded(bounded) = &mut terrain.storage {
            bounded.base = OnceLock::from(base);
        }
        terrain
    }

    /// A bounded terrain from explicit tiles (the digest is computed here).
    fn from_tiles(settings: WorldSettings, width: i32, height: i32, tiles: Vec<Tile>) -> Self {
        let digest = tiles.iter().enumerate().fold(0_u64, |sum, (index, tile)| {
            sum.wrapping_add(tile_digest(index, *tile))
        });
        Self {
            height,
            settings,
            storage: Storage::Bounded(BoundedTiles {
                width,
                tiles,
                base: OnceLock::new(),
            }),
            digest,
            journal: None,
            journal_before: Vec::new(),
            frame: Frame::for_generator(settings.generator),
            column_limit_override: None,
        }
    }

    /// How this terrain was generated.
    #[must_use]
    pub const fn settings(&self) -> &WorldSettings {
        &self.settings
    }

    /// Replaces the generation settings record (replicas following the authority's session
    /// section; the tiles are not regenerated).
    pub fn set_settings(&mut self, settings: WorldSettings) {
        if self.settings == settings {
            return;
        }
        if settings.generator.is_streamed() != self.is_streamed() {
            // A different storage model: start over from the new settings (the authority's
            // terrain follows through keyframes and chunk refreshes).
            *self = Self::generate(&settings);
            return;
        }
        self.settings = settings;
        match &mut self.storage {
            Storage::Bounded(bounded) => bounded.base = OnceLock::new(),
            Storage::Streamed(streamed) => streamed.loaded.clear(),
        }
    }

    /// Whether this terrain is streamed (unbounded columns, generated on demand).
    #[must_use]
    pub const fn is_streamed(&self) -> bool {
        matches!(self.storage, Storage::Streamed(_))
    }

    /// The first column of the world (0 for bounded terrain).
    #[must_use]
    pub const fn min_x(&self) -> i32 {
        match &self.storage {
            Storage::Bounded(_) => 0,
            Storage::Streamed(_) => -self.column_limit(),
        }
    }

    /// The last column of the world.
    #[must_use]
    pub const fn max_x(&self) -> i32 {
        match &self.storage {
            Storage::Bounded(bounded) => bounded.width - 1,
            Storage::Streamed(_) => self.column_limit() - 1,
        }
    }

    /// How far a streamed world extends either side of column 0: its generator's
    /// [`GeneratorVersion::column_limit`](worldgen::GeneratorVersion::column_limit), unless a
    /// test world raised it ([`Self::set_column_limit_for_tests`]).
    #[must_use]
    pub const fn column_limit(&self) -> i32 {
        match self.column_limit_override {
            Some(limit) => limit,
            None => self.settings.generator.column_limit(),
        }
    }

    /// Raises (or lowers) a streamed world's column limit, for test and tool worlds far from
    /// the origin (F7 precision tests at 10^9 tiles). Carried by the terrain snapshot, so saves
    /// and keyframes keep it; not hashed.
    #[doc(hidden)]
    pub const fn set_column_limit_for_tests(&mut self, limit: i32) {
        self.column_limit_override = Some(if limit < 1 { 1 } else { limit });
    }

    /// The local pixel frame of the world's positions (see [`crate::position`]).
    #[must_use]
    pub const fn frame(&self) -> Frame {
        self.frame
    }

    /// Sets the local pixel frame record. Only the world moves it (shifting every position
    /// with it, `WorldState::move_frame`); restoring a saved terrain sets it from the save.
    pub(crate) const fn set_frame(&mut self, frame: Frame) {
        self.frame = frame;
    }

    /// Whether column `x` is inside the world.
    #[must_use]
    pub const fn contains_column(&self, x: i32) -> bool {
        x >= self.min_x() && x <= self.max_x()
    }

    /// The first row of the world: 0, except for biome worlds
    /// ([`GeneratorVersion::V7`](worldgen::GeneratorVersion::V7)), whose mountains rise into
    /// rows above 0 (see [`worldgen::streamed::top_row`]). Rows above it are open sky (no
    /// tiles), which rigs may fly through without limit.
    #[must_use]
    pub const fn min_y(&self) -> i32 {
        match &self.storage {
            Storage::Bounded(_) => 0,
            Storage::Streamed(_) => worldgen::streamed::top_row(&self.settings),
        }
    }

    /// Whether `position` is inside the world.
    #[must_use]
    pub const fn contains(&self, position: TilePosition) -> bool {
        self.contains_column(position.x) && position.y >= self.min_y() && position.y < self.height
    }

    /// The first ground row of column `x` (the surface heightmap; see
    /// [`worldgen::surface_row`]).
    #[must_use]
    pub fn surface_row(&self, x: i32) -> i32 {
        worldgen::surface_row(&self.settings, x)
    }

    /// The band (shallow layer or deep stratum) containing `position`.
    #[must_use]
    pub const fn band_at(&self, position: TilePosition) -> Band {
        worldgen::band_at(&self.settings, position.x, position.y)
    }

    /// The classic nominal row (of the ~220-row stack depth rules were authored against) that
    /// row `row` of this world corresponds to (see [`worldgen::strata::nominal_row`]). The
    /// identity before generator V4.
    #[must_use]
    pub const fn nominal_row(&self, row: i32) -> i32 {
        worldgen::strata::nominal_row(&self.settings, row)
    }

    /// The first row of this world at the classic nominal row `nominal` (see
    /// [`worldgen::strata::scaled_row`]).
    #[must_use]
    pub const fn scaled_row(&self, nominal: i32) -> i32 {
        worldgen::strata::scaled_row(&self.settings, nominal)
    }

    /// [`Self::nominal_row`] for a continuous depth in tile rows.
    #[must_use]
    pub fn nominal_depth(&self, rows: f32) -> f32 {
        worldgen::strata::nominal_depth(&self.settings, rows)
    }

    /// The Deep Descent stratum containing `position` (see [`worldgen::descent::stratum_at`]):
    /// a V11 world's own stratum, or the stratum standing for an older world's band.
    #[must_use]
    pub const fn stratum_at(&self, position: TilePosition) -> crate::descent::Stratum {
        worldgen::descent::stratum_at(&self.settings, position.x, position.y)
    }

    /// The name of the layer at `position` as players see it: a V11 world's stratum, an older
    /// world's band.
    #[must_use]
    pub const fn layer_name(&self, position: TilePosition) -> &'static str {
        if self.settings.generator.has_descents() {
            self.stratum_at(position).name()
        } else {
            self.band_at(position).name()
        }
    }

    /// The creatures that roam at `position`, with spawn weights: a V11 stratum's registry
    /// table, or the classic roster of an older world's band.
    #[must_use]
    pub const fn creature_roster(
        &self,
        position: TilePosition,
    ) -> &'static [(crate::enemy::EnemyKind, u32)] {
        if self.settings.generator.has_descents() {
            self.stratum_at(position).def().creatures
        } else {
            crate::enemy::band_roster(self.band_at(position))
        }
    }

    /// The deep stratum containing `position`, if it is below the shallow layers.
    #[must_use]
    pub const fn deep_stratum_at(&self, position: TilePosition) -> Option<DeepStratum> {
        self.band_at(position).deep_stratum()
    }

    /// The generator's whole feature index (caverns, motherlodes, artifact beds, hazard
    /// clusters, band transitions, prefab slots). Empty for legacy worlds.
    ///
    /// A streamed world has no finite index: this is empty for it. Use the regional queries
    /// that work for every world instead: [`Self::feature`], [`Self::features_in`],
    /// [`Self::pois_touching`], [`Self::poi_at`] and [`Self::nearest_feature`].
    #[must_use]
    pub fn features(&self) -> &FeatureIndex {
        static EMPTY: OnceLock<FeatureIndex> = OnceLock::new();
        match &self.storage {
            Storage::Bounded(_) => &self.base().features,
            Storage::Streamed(_) => EMPTY.get_or_init(FeatureIndex::default),
        }
    }

    /// The feature with id `id`, in any world.
    #[must_use]
    pub fn feature(&self, id: FeatureId) -> Option<WorldFeature> {
        if self.is_streamed() {
            worldgen::streamed::feature(&self.settings, id)
        } else {
            self.features().get(id).copied()
        }
    }

    /// Every feature (of `kinds`, or every kind) whose centre lies in `rect`, in id order.
    #[must_use]
    pub fn features_in(&self, rect: &TileRect, kinds: Option<&[FeatureKind]>) -> Vec<WorldFeature> {
        if self.is_streamed() {
            return worldgen::streamed::features_in(&self.settings, rect, kinds);
        }
        self.features()
            .all()
            .iter()
            .filter(|feature| {
                rect.contains(feature.center)
                    && kinds.is_none_or(|kinds| kinds.contains(&feature.kind))
            })
            .copied()
            .collect()
    }

    /// The points of interest whose bounds overlap `rect`, in id order.
    #[must_use]
    pub fn pois_touching(&self, rect: &TileRect) -> Vec<WorldFeature> {
        if !self.settings.generator.has_points_of_interest() {
            return Vec::new();
        }
        if self.is_streamed() {
            return worldgen::streamed::pois_touching(&self.settings, rect);
        }
        self.features()
            .pois()
            .filter(|feature| feature.bounds.intersects(rect))
            .copied()
            .collect()
    }

    /// The cave ecosystem at `position` ([`crate::cave`]): its kind, feature id, whether the
    /// tile is in its open interior and whether that is water. `None` outside every cave and on
    /// worlds without cave ecosystems (generators before V6). A pure function of the settings.
    #[must_use]
    pub fn cave_at(&self, position: TilePosition) -> Option<crate::cave::CaveSample> {
        // The run vault of a tomb world (WP-TOMBS) is no cave: no water, critters or tags.
        if worldgen::tomb_sites::in_vault(&self.settings, position) {
            return None;
        }
        worldgen::streamed::cave_at(&self.settings, position)
    }

    /// The kind of cave ecosystem at `position`, walls included (spawn tags for enemies:
    /// `terrain.cave_kind_at(tile).map(CaveKind::spawn_tags)`).
    #[must_use]
    pub fn cave_kind_at(&self, position: TilePosition) -> Option<crate::cave::CaveKind> {
        self.cave_at(position).map(|sample| sample.kind)
    }

    /// The caves whose footprints overlap `rect` (each with its raster of open, water and
    /// wall cells; empty on worlds without cave ecosystems).
    #[must_use]
    pub fn caves_touching(&self, rect: &TileRect) -> Vec<Arc<worldgen::streamed::CaveRegion>> {
        worldgen::streamed::caves_touching(&self.settings, rect)
    }

    /// Whether `position` is open water: a flooded cave's generated water (or a V9 sky
    /// continent's lake) over an open tile.
    #[must_use]
    pub fn water_at(&self, position: TilePosition) -> bool {
        (self.cave_at(position).is_some_and(|sample| sample.water)
            || worldgen::sky::continent::lake_water_at(&self.settings, position))
            && self
                .tile(position)
                .is_some_and(|tile| tile.kind == TileKind::Air)
    }

    /// The point of interest whose bounds contain `position` (lowest id if several overlap).
    #[must_use]
    pub fn poi_at(&self, position: TilePosition) -> Option<WorldFeature> {
        if self.is_streamed() {
            return self
                .pois_touching(&TileRect {
                    min: position,
                    max: position,
                })
                .into_iter()
                .next();
        }
        self.features().poi_at(position).copied()
    }

    /// The feature of `kind` accepted by `accept` whose centre is nearest `from` (ties: lowest
    /// id). Streamed worlds search outward at most `max_columns` columns either side; bounded
    /// worlds search their whole index.
    pub fn nearest_feature(
        &self,
        from: TilePosition,
        kind: FeatureKind,
        max_columns: i32,
        mut accept: impl FnMut(&WorldFeature) -> bool,
    ) -> Option<WorldFeature> {
        if self.is_streamed() {
            return worldgen::streamed::nearest_feature(
                &self.settings,
                from,
                kind,
                max_columns,
                accept,
            );
        }
        self.features()
            .of_kind(kind)
            .filter(|feature| accept(feature))
            .min_by_key(|feature| {
                let dx = i64::from(feature.center.x - from.x);
                let dy = i64::from(feature.center.y - from.y);
                (dx * dx + dy * dy, feature.id)
            })
            .copied()
    }

    /// Order-independent digest of the terrain, maintained incrementally: of every tile for
    /// bounded terrain, of every edit for streamed terrain (whose generated tiles are a pure
    /// function of the settings, which the world hash covers separately).
    #[must_use]
    pub const fn digest(&self) -> u64 {
        self.digest
    }

    /// Bounded tiles: how many. Streamed: how many edits (the stored tiles).
    #[must_use]
    pub fn stored_tile_count(&self) -> usize {
        match &self.storage {
            Storage::Bounded(bounded) => bounded.tiles.len(),
            Storage::Streamed(streamed) => streamed.edits.values().map(BTreeMap::len).sum(),
        }
    }

    /// Chunks currently held in memory (streamed terrain; 0 for bounded terrain).
    #[must_use]
    pub fn loaded_chunk_count(&self) -> usize {
        match &self.storage {
            Storage::Bounded(_) => 0,
            Storage::Streamed(streamed) => streamed.loaded.len(),
        }
    }

    /// Chunks with at least one edit (streamed terrain; 0 for bounded terrain).
    #[must_use]
    pub fn edited_chunk_count(&self) -> usize {
        match &self.storage {
            Storage::Bounded(_) => 0,
            Storage::Streamed(streamed) => streamed.edits.len(),
        }
    }

    /// The chunks currently held in memory (streamed terrain).
    pub fn loaded_chunks(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        let loaded = match &self.storage {
            Storage::Bounded(_) => None,
            Storage::Streamed(streamed) => Some(streamed.loaded.keys().copied()),
        };
        loaded.into_iter().flatten()
    }

    /// The chunks with edits (streamed terrain).
    pub fn edited_chunks(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        let edited = match &self.storage {
            Storage::Bounded(_) => None,
            Storage::Streamed(streamed) => Some(streamed.edits.keys().copied()),
        };
        edited.into_iter().flatten()
    }

    /// Keeps streamed chunks loaded around `centers` (tile positions, e.g. every rig or a
    /// camera): loads every chunk within [`STREAM_LOAD_RADIUS_CHUNKS`] of a centre and unloads
    /// loaded chunks beyond [`STREAM_KEEP_RADIUS_CHUNKS`] of every centre. Edits survive
    /// unloading. Returns how many chunks were loaded. A no-op for bounded terrain.
    ///
    /// Purely a performance and memory measure: tiles read the same loaded or not.
    pub fn stream_around(&mut self, centers: &[TilePosition]) -> usize {
        let settings = self.settings;
        let chunk_rows = worldgen::streamed::chunk_rows(&settings);
        let first_chunk_row = worldgen::streamed::first_chunk_row(&settings);
        let (min_x, max_x) = (self.min_x(), self.max_x());
        let Storage::Streamed(streamed) = &mut self.storage else {
            return 0;
        };
        let centers: Vec<ChunkPosition> = centers
            .iter()
            .map(|position| {
                ChunkPosition::of(TilePosition {
                    x: position.x.clamp(min_x, max_x),
                    y: position.y,
                })
            })
            .collect();
        let near = |chunk: &ChunkPosition, (radius_x, radius_y): (i32, i32)| {
            centers.iter().any(|center| {
                (chunk.x - center.x).abs() <= radius_x && (chunk.y - center.y).abs() <= radius_y
            })
        };
        streamed
            .loaded
            .retain(|chunk, _| near(chunk, STREAM_KEEP_RADIUS_CHUNKS));
        let (min_chunk, max_chunk) = (
            ChunkPosition::of(TilePosition { x: min_x, y: 0 }).x,
            ChunkPosition::of(TilePosition { x: max_x, y: 0 }).x,
        );
        let mut loaded = 0;
        let (radius_x, radius_y) = STREAM_LOAD_RADIUS_CHUNKS;
        for center in &centers {
            for y in (center.y - radius_y).max(first_chunk_row)
                ..=(center.y + radius_y).min(chunk_rows - 1)
            {
                for x in (center.x - radius_x).max(min_chunk)..=(center.x + radius_x).min(max_chunk)
                {
                    let chunk = ChunkPosition { x, y };
                    if streamed.loaded.contains_key(&chunk) {
                        continue;
                    }
                    let base = worldgen::streamed::generated_chunk(&settings, chunk);
                    streamed
                        .loaded
                        .insert(chunk, LoadedChunk::new(base, streamed.edits.get(&chunk)));
                    loaded += 1;
                }
            }
        }
        loaded
    }

    fn write_tile(&mut self, slot: Slot, position: TilePosition, tile: Tile) {
        match slot {
            Slot::Bounded(index) => {
                let Storage::Bounded(bounded) = &mut self.storage else {
                    return;
                };
                let old = bounded.tiles[index];
                bounded.tiles[index] = tile;
                self.digest = self
                    .digest
                    .wrapping_sub(tile_digest(index, old))
                    .wrapping_add(tile_digest(index, tile));
                if self.journal.is_some() {
                    self.journal_before.push(old.kind);
                }
            }
            Slot::Streamed(chunk, local) => {
                let settings = self.settings;
                let Storage::Streamed(streamed) = &mut self.storage else {
                    return;
                };
                let edits = &mut streamed.edits;
                let loaded = streamed.loaded.entry(chunk).or_insert_with(|| {
                    LoadedChunk::new(
                        worldgen::streamed::generated_chunk(&settings, chunk),
                        edits.get(&chunk),
                    )
                });
                let generated = loaded.base.tile(local);
                let old = loaded.tile(local);
                if self.journal.is_some() {
                    self.journal_before.push(old.kind);
                }
                if loaded.tiles.is_none() {
                    loaded.tiles = Some(Arc::new(loaded.base.tiles().to_vec()));
                }
                if let Some(tiles) = loaded.tiles.as_mut() {
                    Arc::make_mut(tiles)[local] = tile;
                }
                if old != generated {
                    self.digest = self.digest.wrapping_sub(position_digest(position, old));
                }
                if tile != generated {
                    self.digest = self.digest.wrapping_add(position_digest(position, tile));
                }
                let key = u16::try_from(local).unwrap_or(u16::MAX);
                if tile == generated {
                    if let Some(chunk_edits) = edits.get_mut(&chunk) {
                        chunk_edits.remove(&key);
                        if chunk_edits.is_empty() {
                            edits.remove(&chunk);
                        }
                    }
                } else {
                    edits.entry(chunk).or_default().insert(key, tile);
                }
            }
        }
        if let Some(journal) = self.journal.as_mut() {
            journal.push(TileChange { position, tile });
        }
    }

    /// Starts recording every tile write (see [`Self::finish_journal`]).
    pub(crate) fn start_journal(&mut self) {
        self.journal = Some(Vec::new());
        self.journal_before.clear();
    }

    /// The tile writes recorded so far this tick with the kind each one replaced.
    pub(crate) fn journal_with_before(&self) -> impl Iterator<Item = (&TileChange, TileKind)> {
        self.journal()
            .iter()
            .zip(self.journal_before.iter().copied())
    }

    /// The tile writes recorded so far this tick (empty when not recording).
    pub(crate) fn journal(&self) -> &[TileChange] {
        self.journal.as_deref().unwrap_or(&[])
    }

    /// Stops recording and returns the tile writes since [`Self::start_journal`], in order.
    pub(crate) fn finish_journal(&mut self) -> Vec<TileChange> {
        self.journal_before.clear();
        self.journal.take().unwrap_or_default()
    }

    /// Applies one replicated tile write.
    pub fn apply_tile_change(&mut self, change: &TileChange) -> bool {
        self.set_tile_from_network(change.position, change.tile.kind, change.tile.durability)
    }

    /// The in-world tiles of `chunk`, row-major (refreshes send [`Self::chunk_diffs`] instead).
    #[must_use]
    pub fn chunk_tiles(&self, chunk: ChunkPosition) -> Vec<Tile> {
        self.chunk_positions(chunk)
            .filter_map(|position| self.tile(position))
            .collect()
    }

    /// The in-world tile positions of `chunk`, row-major.
    fn chunk_positions(&self, chunk: ChunkPosition) -> impl Iterator<Item = TilePosition> + use<> {
        let origin = chunk.origin();
        let xs = origin.x.max(self.min_x())..(origin.x + CHUNK_SIZE_TILES).min(self.max_x() + 1);
        (origin.y.max(self.min_y())..(origin.y + CHUNK_SIZE_TILES).min(self.height))
            .flat_map(move |y| xs.clone().map(move |x| TilePosition { x, y }))
    }

    /// Bounded width in tiles. A streamed terrain reports the span between [`Self::min_x`] and
    /// [`Self::max_x`]; code that walks or clamps columns should use those instead.
    #[must_use]
    pub const fn width(&self) -> i32 {
        // Saturating: a far test world's raised column limit spans more than i32 (F7).
        self.max_x().saturating_sub(self.min_x()).saturating_add(1)
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.height
    }

    /// Every tile of a bounded terrain, row-major (`None` for streamed terrain, which has no
    /// finite grid).
    #[must_use]
    pub fn bounded_tiles(&self) -> Option<&[Tile]> {
        match &self.storage {
            Storage::Bounded(bounded) => Some(&bounded.tiles),
            Storage::Streamed(_) => None,
        }
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.settings.seed
    }

    /// Extends a bounded terrain downwards with freshly generated rows (old saves of shallower
    /// worlds). Streamed terrain always has its full height.
    pub fn ensure_depth(&mut self, minimum_height: i32) {
        if minimum_height <= self.height {
            return;
        }
        let settings = self.settings;
        let Storage::Bounded(bounded) = &mut self.storage else {
            return;
        };
        let base = generated_base(&settings, bounded.width, minimum_height);
        let start = bounded.tiles.len();
        bounded.tiles.extend_from_slice(&base.tiles[start..]);
        for (index, tile) in bounded.tiles.iter().enumerate().skip(start) {
            self.digest = self.digest.wrapping_add(tile_digest(index, *tile));
        }
        bounded.base = OnceLock::from(base);
        self.height = minimum_height;
    }

    #[must_use]
    pub fn tile(&self, position: TilePosition) -> Option<Tile> {
        self.slot(position).map(|slot| self.read(slot))
    }

    fn read(&self, slot: Slot) -> Tile {
        match (&self.storage, slot) {
            (Storage::Bounded(bounded), Slot::Bounded(index)) => bounded.tiles[index],
            (Storage::Streamed(streamed), Slot::Streamed(chunk, local)) => {
                if let Some(loaded) = streamed.loaded.get(&chunk) {
                    return loaded.tile(local);
                }
                let key = u16::try_from(local).unwrap_or(u16::MAX);
                streamed
                    .edits
                    .get(&chunk)
                    .and_then(|edits| edits.get(&key))
                    .copied()
                    .unwrap_or_else(|| {
                        worldgen::streamed::generated_chunk(&self.settings, chunk).tile(local)
                    })
            }
            _ => Tile {
                kind: TileKind::Bedrock,
                durability: 0,
            },
        }
    }

    #[must_use]
    pub fn is_solid_at(&self, position: TilePosition) -> bool {
        // Above the world is open sky; below it and past its sides is bedrock (no tile).
        self.tile(position).is_some_and(|tile| !tile.kind.is_open())
    }

    /// The fluid at `position`, if it holds any ([`crate::fluid`]).
    #[must_use]
    pub fn fluid_at(&self, position: TilePosition) -> Option<crate::fluid::FluidCell> {
        self.tile(position).and_then(crate::fluid::FluidCell::of)
    }

    #[must_use]
    pub fn is_lava_at(&self, position: TilePosition) -> bool {
        self.tile(position)
            .is_some_and(|tile| tile.kind == TileKind::Lava)
    }

    #[must_use]
    pub fn hardness_at(&self, position: TilePosition) -> Option<u8> {
        self.tile(position).map(|tile| tile_hardness(tile.kind))
    }

    pub fn set_tile_from_network(
        &mut self,
        position: TilePosition,
        kind: TileKind,
        durability: u8,
    ) -> bool {
        let Some(slot) = self.slot(position) else {
            return false;
        };
        self.write_tile(slot, position, Tile { kind, durability });
        true
    }

    /// Writes a fresh tile of `kind` at `position`. Refused (returns `false`) outside the world
    /// and over unbreakable tiles (the bedrock floor), so no gameplay effect breaches the floor.
    pub fn set_kind(&mut self, position: TilePosition, kind: TileKind) -> bool {
        let Some(slot) = self.slot(position) else {
            return false;
        };
        let current = self.read(slot).kind;
        if is_unbreakable(current) && kind != current {
            return false;
        }
        self.write_tile(
            slot,
            position,
            Tile {
                kind,
                durability: tile_durability(kind),
            },
        );
        true
    }

    /// Writes a fresh tile of `kind` at `position` even over an unbreakable tile: the world's
    /// own rules (the Moon's facility lighting a pylon, opening its gate and seal; never a
    /// drill, blast or pick). Refused only outside the world.
    pub(crate) fn set_kind_by_rule(&mut self, position: TilePosition, kind: TileKind) -> bool {
        let Some(slot) = self.slot(position) else {
            return false;
        };
        self.write_tile(
            slot,
            position,
            Tile {
                kind,
                durability: tile_durability(kind),
            },
        );
        true
    }

    /// Takes one point of durability off the tile at `position` (one drill chip).
    pub fn chip(&mut self, position: TilePosition) -> MineResult {
        self.chip_by(position, 1, false)
    }

    /// Takes `bite` points of durability off the tile at `position` (one drill chip from a drill
    /// that bites `bite` deep).
    ///
    /// With `unrestricted` (a test-mode rig) building foundations and undrillable vault fixtures
    /// give way too; nothing ever digs an [`is_indestructible`] tile or a tile outside the world.
    pub fn chip_by(&mut self, position: TilePosition, bite: u8, unrestricted: bool) -> MineResult {
        let Some(slot) = self.slot(position) else {
            return MineResult::Blocked;
        };
        let before = self.read(slot);
        let mut tile = before;
        let result = Self::chip_tile(&mut tile, bite.max(1), unrestricted);
        if tile != before {
            self.write_tile(slot, position, tile);
        }
        result
    }

    /// Whether no drill, not even a test-mode rig's, can dig the tile at `position` (outside the
    /// world, or an [`is_indestructible`] tile).
    #[must_use]
    pub fn is_indestructible_at(&self, position: TilePosition) -> bool {
        self.tile(position)
            .is_none_or(|tile| is_indestructible(tile.kind))
    }

    fn chip_tile(tile: &mut Tile, bite: u8, unrestricted: bool) -> MineResult {
        if tile.kind.is_open() || is_indestructible(tile.kind) {
            return MineResult::Blocked;
        }

        if tile.kind == TileKind::Foundation && !unrestricted {
            return MineResult::TooDangerous;
        }

        if tile.kind == TileKind::Gas {
            tile.kind = TileKind::Air;
            tile.durability = 0;
            return MineResult::Exploded;
        }

        if tile.kind == TileKind::ExplosivePocket {
            tile.kind = TileKind::Air;
            tile.durability = 0;
            return MineResult::Blast;
        }

        if let TileKind::Poi(poi) = tile.kind
            && poi.hardness() == crate::poi::UNDRILLABLE
            && !unrestricted
        {
            return MineResult::Blocked;
        }

        tile.durability = tile.durability.saturating_sub(bite);
        if tile.durability > 0 {
            return MineResult::Chipped;
        }

        if let TileKind::Poi(poi) = tile.kind
            && let Some(lit) = poi.activated()
        {
            tile.kind = TileKind::Poi(lit);
            tile.durability = lit.durability();
            return MineResult::Activated(lit);
        }

        let mined = tile.kind;
        tile.kind = TileKind::Air;
        MineResult::Mined(mined)
    }

    pub fn blast_radius(&mut self, center: TilePosition, radius: i32) -> BlastResult {
        let mut result = BlastResult::default();
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if (x - center.x).abs() + (y - center.y).abs() > radius {
                    continue;
                }
                let position = TilePosition { x, y };
                let Some(slot) = self.slot(position) else {
                    continue;
                };
                let before = self.read(slot).kind;
                let left = match before {
                    // Fluid rides out a blast (the blast opens its walls; the fluid flows).
                    TileKind::Air
                    | TileKind::Fluid(_)
                    | TileKind::Lava
                    | TileKind::Foundation
                    | TileKind::Bedrock
                    | TileKind::Built(_) => {
                        continue;
                    }
                    TileKind::Sky(sky) if sky.is_indestructible() => continue,
                    TileKind::Poi(poi) => match poi.after_blast() {
                        None => continue,
                        Some(left) => left,
                    },
                    // Cave caches shrug off blasts like POI loot: a bomb has no owner to pay.
                    // Builder alloy and the keyed walls shrug them off too (Wave 9).
                    TileKind::Cave(cave) if cave.is_cache() || cave.blast_proof() => continue,
                    // So do sky caches and nests (WP-SKY): drill them, or bring their island down.
                    TileKind::Sky(sky) if sky.is_cache() => continue,
                    // The Old Deep's fixtures and the Star Core itself (only a drill strikes
                    // the core; bombs still break pylons and pistons).
                    // The Sun Well and the inner sun (Wave 9) shrug off every blast.
                    TileKind::Hollow(tile) if tile.indestructible() => continue,
                    // The buried city's machinery and plinths (Wave 10).
                    TileKind::City(tile) if tile.indestructible() => continue,
                    TileKind::Deep(tile)
                        if tile.indestructible()
                            || matches!(tile, crate::old_deep::DeepTile::FirstCore) =>
                    {
                        continue;
                    }
                    _ => None,
                };
                let tile = left.map_or(
                    Tile {
                        kind: TileKind::Air,
                        durability: 0,
                    },
                    |poi| Tile {
                        kind: TileKind::Poi(poi),
                        durability: poi.durability(),
                    },
                );
                self.write_tile(slot, position, tile);
                if tile.kind == TileKind::Air {
                    result.cleared += 1;
                    result.broken.push((position, before));
                }
                result.changed_tiles.push(position);
            }
        }
        result
    }

    const fn slot(&self, position: TilePosition) -> Option<Slot> {
        if position.y < self.min_y() || position.y >= self.height {
            return None;
        }
        match &self.storage {
            Storage::Bounded(bounded) => {
                if position.x < 0 || position.x >= bounded.width {
                    return None;
                }
                Some(Slot::Bounded(
                    (position.y * bounded.width + position.x) as usize,
                ))
            }
            Storage::Streamed(_) => {
                let limit = self.column_limit();
                if position.x < -limit || position.x >= limit {
                    return None;
                }
                Some(Slot::Streamed(
                    ChunkPosition::of(position),
                    ChunkPosition::local_index(position),
                ))
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlastResult {
    pub cleared: u32,
    pub changed_tiles: Vec<TilePosition>,
    /// Every tile the blast turned to air, with what it was (WP-DROPS turns these into loot).
    pub broken: Vec<(TilePosition, TileKind)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MineResult {
    Blocked,
    TooDangerous,
    Exploded,
    Blast,
    Chipped,
    Mined(TileKind),
    /// A vault switch or relay was drilled through and lit up (it stays in place).
    Activated(PoiTile),
}

/// A tile of the original fixed-depth generator ([`GeneratorVersion::Legacy`] worlds).
pub(crate) fn legacy_tile_kind(x: i32, y: i32, seed: u64) -> TileKind {
    if y <= 4 {
        return TileKind::Air;
    }

    if building_foundation_at(x, y) {
        return TileKind::Foundation;
    }

    if cave_air(x, y, seed) {
        return TileKind::Air;
    }

    if explosive_pocket(x, y, seed) {
        return TileKind::ExplosivePocket;
    }

    if pressure_pocket(x, y, seed) {
        return TileKind::PressurePocket;
    }

    if magma_vent(x, y, seed) {
        return TileKind::MagmaVent;
    }

    if lava_pocket(x, y, seed) {
        return TileKind::Lava;
    }

    if gas_pocket(x, y, seed) {
        return TileKind::Gas;
    }

    if let Some(stratum) = legacy_deep_stratum(y)
        && seeded_hash(x, y, seed ^ 0xD33F).is_multiple_of(29)
    {
        return stratum.primary_hazard();
    }

    if artifact_spot(x, y, seed) {
        return TileKind::Artifact(artifact_at_depth(x, y));
    }

    if (46..=64).contains(&y) && (x + y).rem_euclid(5) == 0 {
        return ore_or_base_tile(x, y, TileKind::HardRock, seed);
    }

    let base = match y {
        5..=13 => TileKind::Dirt,
        14..=27 => TileKind::Clay,
        28..=64 | 90..=129 => TileKind::Stone,
        _ => TileKind::HardRock,
    };

    ore_or_base_tile(x, y, base, seed)
}

const fn cave_air(x: i32, y: i32, seed: u64) -> bool {
    if y < 12 {
        return false;
    }

    let cavern_mod = match y {
        0..=27 => 61,
        28..=47 => 43,
        48..=67 => 37,
        _ => 31,
    };
    let cavern = seeded_hash(x / 5, y / 4, seed) % cavern_mod;
    let seam = y > 44 && seeded_hash(x / 9, y, seed ^ 0xF00D).is_multiple_of(19);
    let tunnel = seeded_hash(x, y, seed ^ 0xA5A5) % 97;
    cavern == 0 || tunnel == 0 || seam
}

const fn explosive_pocket(x: i32, y: i32, seed: u64) -> bool {
    y > 28 && seeded_hash(x / 2, y / 2, seed ^ 0xE770).is_multiple_of(173)
}

const fn pressure_pocket(x: i32, y: i32, seed: u64) -> bool {
    y > 34 && y < 76 && seeded_hash(x / 3, y / 2, seed ^ 0x9A5).is_multiple_of(149)
}

const fn magma_vent(x: i32, y: i32, seed: u64) -> bool {
    y > 54 && (x + y).rem_euclid(7) == 0 && seeded_hash(x, y / 4, seed ^ 0xA11).is_multiple_of(41)
}

const fn lava_pocket(x: i32, y: i32, seed: u64) -> bool {
    if y < 48 {
        return false;
    }

    let pocket = seeded_hash(x / 4, y / 3, seed ^ 0x1A5A) % 53;
    pocket == 0 || (pocket <= 2 && y > 68)
}

const fn gas_pocket(x: i32, y: i32, seed: u64) -> bool {
    if y < 24 || y > 74 {
        return false;
    }

    let divisor = if y > 52 { 59 } else { 83 };
    seeded_hash(x / 3, y / 3, seed ^ 0x6A5).is_multiple_of(divisor)
}

const fn artifact_spot(x: i32, y: i32, seed: u64) -> bool {
    y > 20 && seeded_hash(x, y, seed ^ 0x5EED).is_multiple_of(211)
}

const fn artifact_at_depth(x: i32, y: i32) -> ArtifactKind {
    if let Some(stratum) = legacy_deep_stratum(y) {
        return match stratum {
            DeepStratum::CrystalFaults => ArtifactKind::CrystalPrism,
            DeepStratum::FossilOceans => ArtifactKind::RadiantShell,
            DeepStratum::PressureCathedrals => ArtifactKind::PressureGlyph,
            DeepStratum::RadioactiveHollow => ArtifactKind::ReactorBloom,
            DeepStratum::AncientMachineLayer => ArtifactKind::MachineSigil,
            DeepStratum::VoidGeodeFields => ArtifactKind::VoidPearl,
            DeepStratum::MantleStormZone => ArtifactKind::MantleHeart,
        };
    }
    match y {
        0..=30 => ArtifactKind::Fossil,
        31..=50 => ArtifactKind::OldCircuit,
        51..=70 => {
            if (x + y).rem_euclid(2) == 0 {
                ArtifactKind::BuriedIdol
            } else {
                ArtifactKind::OldCircuit
            }
        }
        _ => ArtifactKind::StarCore,
    }
}

const fn ore_or_base_tile(x: i32, y: i32, base: TileKind, seed: u64) -> TileKind {
    if !patterned_ore(x, y, seed) {
        return base;
    }

    if let Some(stratum) = legacy_deep_stratum(y) {
        return TileKind::Ore(match stratum {
            DeepStratum::CrystalFaults | DeepStratum::VoidGeodeFields => MineralKind::Diamond,
            DeepStratum::FossilOceans => MineralKind::Platinum,
            DeepStratum::PressureCathedrals | DeepStratum::AncientMachineLayer => {
                MineralKind::Mythril
            }
            DeepStratum::RadioactiveHollow => MineralKind::Uranium,
            DeepStratum::MantleStormZone => MineralKind::Ruby,
        });
    }

    match y {
        0..=16 => TileKind::Ore(MineralKind::Copper),
        17..=26 => TileKind::Ore(if (x + y).rem_euclid(3) == 0 {
            MineralKind::Silver
        } else {
            MineralKind::Iron
        }),
        27..=42 => TileKind::Ore(if (x * 5 + y).rem_euclid(5) == 0 {
            MineralKind::Gold
        } else {
            MineralKind::Silver
        }),
        43..=62 => TileKind::Ore(match (x * 13 + y * 7).rem_euclid(7) {
            0 => MineralKind::Ruby,
            1 | 2 => MineralKind::Emerald,
            _ => MineralKind::Gold,
        }),
        _ => TileKind::Ore(match (x * 19 + y * 23).rem_euclid(13) {
            0 => MineralKind::Mythril,
            1 => MineralKind::Uranium,
            2 | 3 => MineralKind::Platinum,
            4 | 5 => MineralKind::Diamond,
            6 | 7 => MineralKind::Ruby,
            _ => MineralKind::Emerald,
        }),
    }
}

const fn patterned_ore(x: i32, y: i32, seed: u64) -> bool {
    let vein_a = seeded_hash(x / 2, y / 2, seed ^ 0xC0DE) % 31;
    let vein_b = seeded_hash((x + y) / 3, y / 2, seed ^ 0xBEEF) % 43;
    let pocket = seeded_hash(x / 4, y / 3, seed ^ 0xFACE) % 23;
    vein_a <= 2 || vein_b <= 1 || pocket <= 1
}

const fn seeded_hash(x: i32, y: i32, seed: u64) -> u64 {
    let mut value =
        seed ^ ((x as u64).wrapping_mul(0x9E37_79B1)) ^ ((y as u64).wrapping_mul(0x85EB_CA77));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value
}

/// Hardness of tiles nothing can ever dig (the bedrock floor): above every drill strength,
/// test mode's included.
pub const UNBREAKABLE_HARDNESS: u8 = u8::MAX;

/// Whether `kind` can never be dug, drilled or blasted away by anything.
#[must_use]
pub const fn is_unbreakable(kind: TileKind) -> bool {
    match kind {
        TileKind::Bedrock | TileKind::Sky(crate::sky::SkyTile::AetherCore) => true,
        // The colony, the alien facility and the Moon's core (WP-MOON).
        TileKind::Lunar(tile) => tile.indestructible(),
        // The Star Core arena, the shielded core and the Cathedral Lift's fixtures (Wave 8).
        TileKind::Deep(tile) => tile.indestructible(),
        TileKind::Hollow(tile) => tile.indestructible(),
        // The Heart's shell and body, the wells' points, the pylons and the cable (Wave 10).
        TileKind::Core(tile) => tile.indestructible(),
        // The Prism Desert's colour-keyed walls open only to light (Wave 9).
        // ... and Descent VII part B's fixtures (Wave 10).
        TileKind::Cave(tile) => tile.is_keyed_wall() || tile.is_garrison_fixture(),
        _ => false,
    }
}

/// Whether no drill can ever dig `kind`, not even a test-mode rig's: the bedrock floor and
/// placed building pieces (which come down only by deconstruction).
#[must_use]
pub const fn is_indestructible(kind: TileKind) -> bool {
    is_unbreakable(kind)
        || match kind {
            TileKind::Built(_) => true,
            // Sky temples and the aether core (V9 continents).
            TileKind::Sky(tile) => tile.is_indestructible(),
            _ => false,
        }
}

/// Drill strength needed to dig a tile.
#[must_use]
#[allow(clippy::match_same_arms, reason = "grouped by material family")]
pub const fn tile_hardness(kind: TileKind) -> u8 {
    match kind {
        TileKind::Bedrock | TileKind::Built(_) => UNBREAKABLE_HARDNESS,
        TileKind::Air
        | TileKind::Fluid(_)
        | TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent
        | TileKind::Foundation => 0,
        TileKind::Dirt | TileKind::Ore(MineralKind::Copper | MineralKind::Iron) => 1,
        TileKind::Clay
        | TileKind::Ore(MineralKind::Silver | MineralKind::Gold | MineralKind::Skystone) => 2,
        TileKind::Stone
        | TileKind::Ore(MineralKind::Emerald | MineralKind::Ruby | MineralKind::CloudSilver)
        | TileKind::Artifact(
            ArtifactKind::Fossil | ArtifactKind::OldCircuit | ArtifactKind::WindAstrolabe,
        ) => 3,
        TileKind::HardRock
        | TileKind::Ore(
            MineralKind::Diamond
            | MineralKind::Platinum
            | MineralKind::Uranium
            | MineralKind::Mythril,
        )
        | TileKind::Artifact(
            ArtifactKind::BuriedIdol | ArtifactKind::StarCore | ArtifactKind::CloudPearl,
        ) => 4,
        TileKind::Poi(tile) => tile.hardness(),
        TileKind::Cave(tile) => tile.hardness(),
        TileKind::Surface(material) => material.hardness(),
        TileKind::Sky(tile) => tile.hardness(),
        TileKind::Lunar(tile) => tile.hardness(),
        TileKind::Crust(tile) => tile.hardness(),
        TileKind::Deep(tile) => tile.hardness(),
        TileKind::Hollow(tile) => tile.hardness(),
        TileKind::Warped(tile) => tile.hardness(),
        TileKind::City(tile) => tile.hardness(),
        TileKind::Core(tile) => tile.hardness(),
        TileKind::Artifact(
            ArtifactKind::CrystalPrism
            | ArtifactKind::RadiantShell
            | ArtifactKind::PressureGlyph
            | ArtifactKind::ReactorBloom
            | ArtifactKind::MachineSigil
            | ArtifactKind::VoidPearl
            | ArtifactKind::MantleHeart
            | ArtifactKind::SkyRelic
            | ArtifactKind::LunarRelic
            | ArtifactKind::WardenEye,
        ) => 5,
        TileKind::Artifact(ArtifactKind::HarpyEgg) => 1,
        TileKind::Artifact(ArtifactKind::ClayPot) => 2,
    }
}

/// Chips a freshly generated tile takes to dig.
#[must_use]
pub const fn tile_durability(kind: TileKind) -> u8 {
    match kind {
        TileKind::Air => 0,
        // A fresh fluid tile is a full one (its durability byte is its level).
        TileKind::Fluid(_) => crate::fluid::FLUID_FULL,
        TileKind::Dirt => 2,
        TileKind::Clay => 4,
        TileKind::Stone => 7,
        TileKind::HardRock | TileKind::Foundation => 11,
        TileKind::Lava
        | TileKind::Gas
        | TileKind::ExplosivePocket
        | TileKind::PressurePocket
        | TileKind::MagmaVent => 1,
        TileKind::Ore(mineral) => tile_hardness(TileKind::Ore(mineral)) + 2,
        TileKind::Artifact(artifact) => tile_hardness(TileKind::Artifact(artifact)) + 5,
        TileKind::Poi(tile) => tile.durability(),
        TileKind::Cave(tile) => tile.durability(),
        TileKind::Bedrock | TileKind::Built(_) => u8::MAX,
        TileKind::Surface(material) => material.durability(),
        TileKind::Sky(tile) => tile.durability(),
        TileKind::Lunar(tile) => tile.durability(),
        TileKind::Crust(tile) => tile.durability(),
        TileKind::Deep(tile) => tile.durability(),
        TileKind::Hollow(tile) => tile.durability(),
        TileKind::Warped(tile) => tile.durability(),
        TileKind::City(tile) => tile.durability(),
        TileKind::Core(tile) => tile.durability(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Terrain, TileKind, TilePosition};

    #[test]
    fn generated_world_contains_caves_and_hazards() {
        let terrain = Terrain::new(120, 90);
        let mut air_below_surface = 0;
        let mut lava = 0;
        let mut gas = 0;

        for y in 8..terrain.height() {
            for x in 0..terrain.width() {
                match terrain
                    .tile(TilePosition { x, y })
                    .expect("tile exists")
                    .kind
                {
                    TileKind::Air => air_below_surface += 1,
                    TileKind::Lava => lava += 1,
                    TileKind::Gas => gas += 1,
                    _ => {}
                }
            }
        }

        assert!(air_below_surface > 0);
        assert!(lava > 0);
        assert!(gas > 0);
    }
    #[test]
    fn deeper_biomes_have_more_fractured_air() {
        let terrain = Terrain::new(120, 90);
        let shallow_air = count_air(&terrain, 12, 28);
        let deep_air = count_air(&terrain, 58, 84);
        assert!(deep_air > shallow_air);
    }

    fn count_air(terrain: &Terrain, min_y: i32, max_y: i32) -> u32 {
        let mut total = 0;
        for y in min_y..=max_y {
            for x in 0..terrain.width() {
                if terrain
                    .tile(TilePosition { x, y })
                    .is_some_and(|tile| tile.kind == TileKind::Air)
                {
                    total += 1;
                }
            }
        }
        total
    }
}
