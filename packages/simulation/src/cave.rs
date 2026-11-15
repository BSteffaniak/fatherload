//! Caves 2.0 (expansion plan WP-CAVES): cave ecosystems, their tiles, light, loot, critters
//! and the spawn tags other systems read.
//!
//! [`GeneratorVersion::V6`](crate::worldgen::GeneratorVersion::V6) worlds place one cave
//! ecosystem ([`CaveKind`]) in some cells of a region grid (`worldgen::streamed::cave_biomes`).
//! Each cave is a self-contained footprint: open interior (some of it under water), walls dressed
//! in its own tiles ([`CaveTile`]), decor, a loot budget (treasure caches, rare ore veins,
//! relics) and the critters that live there. Everything about a cave is a pure function of the
//! world settings and its region, so every peer agrees without replication.
//!
//! # Reading caves (API for other tracks)
//!
//! * [`Terrain::cave_at`](crate::terrain::Terrain::cave_at): the [`CaveSample`] at a tile
//!   (kind, feature id, interior, water), `None` outside every cave and on worlds without cave
//!   ecosystems.
//! * [`Terrain::cave_kind_at`](crate::terrain::Terrain::cave_kind_at): just the kind.
//! * [`Terrain::water_at`](crate::terrain::Terrain::water_at): open water (flooded caves and
//!   underground lakes).
//! * [`CaveKind::spawn_tags`]: what may spawn in a cave of that kind (WP-ENEMIES reads this:
//!   e.g. `terrain.cave_kind_at(tile).map(CaveKind::spawn_tags)`).
//! * Caves are features too ([`FeatureKind::Cave`](crate::worldgen::FeatureKind::Cave) with
//!   [`FeatureDetail::Cave`](crate::worldgen::FeatureDetail::Cave)), so quests, stories and
//!   maps find them through the regional feature index (`Terrain::features_in`,
//!   `Terrain::nearest_feature`).

use serde::{Deserialize, Serialize};

use crate::{
    item::{ItemKind, ItemStack},
    terrain::{ArtifactKind, DeepStratum, MineralKind, TilePosition},
    worldgen::{Band, FeatureId, noise::hash2},
};

/// A cave ecosystem.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CaveKind {
    /// A huge open echo hall with stalactites, stalagmites and a buried cache.
    GrandCavern,
    /// A round hollow lined with glowing crystal and gem ore.
    CrystalGeode,
    /// A cave mostly under still water: an underground lake with a sunken cache and cave fish.
    FloodedCave,
    /// A long tube with an obsidian lining and lava pooling along its floor.
    LavaTube,
    /// A glowing mushroom cavern: bouncy caps, spore pockets and fireflies.
    FungalGrotto,
    /// Winding tunnels a giant worm left behind, ribbed with old bone.
    WormBurrows,
    /// A buried hall of ancient brick: pillars, glowing glyphs and a relic cache.
    AncientRuins,
    /// A tall, narrow crack with ledges, falling stalactites and something at the bottom.
    Chasm,
    /// Several small chambers joined by tunnels.
    CaveNetwork,
}

/// How a cave is lit (presentation, and what WP-ENEMIES may treat as "dark").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaveLight {
    /// Pitch dark: only rig lights, flares and critters light it.
    Dark,
    /// Dim: a few glowing tiles.
    Dim,
    /// Lit by its own glowing tiles (crystal, fungus, lava).
    Glowing,
}

/// What may spawn in a cave (read by enemy spawning, WP-ENEMIES).
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent spawn tags enemy spawning reads one by one"
)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CaveSpawnTags {
    /// Nests and hives fit here (enclosed, roomy).
    pub nests: bool,
    /// Swarmers (bats, mites) roost here.
    pub swarmers: bool,
    /// Aquatic creatures (open water).
    pub aquatic: bool,
    /// Heat-loving creatures (lava elementals, fire beetles).
    pub heat: bool,
    /// Machine sentries and drones (built places).
    pub machines: bool,
    /// Burrowers (soft walls, tunnels).
    pub burrowers: bool,
    /// Room for an elite lair (big open floor).
    pub elite_lair: bool,
    /// Relative danger, per mille of the band's base enemy density (1000 = normal).
    pub danger_per_mille: u16,
}

impl CaveKind {
    pub const ALL: [Self; 9] = [
        Self::GrandCavern,
        Self::CrystalGeode,
        Self::FloodedCave,
        Self::LavaTube,
        Self::FungalGrotto,
        Self::WormBurrows,
        Self::AncientRuins,
        Self::Chasm,
        Self::CaveNetwork,
    ];

    /// The display name ("Found: Crystal Geode").
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GrandCavern => "Grand Cavern",
            Self::CrystalGeode => "Crystal Geode",
            Self::FloodedCave => "Flooded Cave",
            Self::LavaTube => "Lava Tube",
            Self::FungalGrotto => "Fungal Grotto",
            Self::WormBurrows => "Worm Burrows",
            Self::AncientRuins => "Ancient Ruins",
            Self::Chasm => "Chasm",
            Self::CaveNetwork => "Cave Network",
        }
    }

    /// A short flavour line for discovery toasts and the map.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::GrandCavern => "An echoing hall. Mind the stalactites",
            Self::CrystalGeode => "The walls glitter with crystal and gems",
            Self::FloodedCave => "Still black water. Something glints below",
            Self::LavaTube => "Obsidian walls and pools of lava",
            Self::FungalGrotto => "Glowing caps you can bounce on",
            Self::WormBurrows => "A giant worm's old tunnels",
            Self::AncientRuins => "Someone built this, long ago",
            Self::Chasm => "A long way down",
            Self::CaveNetwork => "Chambers strung together by tunnels",
        }
    }

    /// Stable small code (saves, ids and art tables).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::GrandCavern => 0,
            Self::CrystalGeode => 1,
            Self::FloodedCave => 2,
            Self::LavaTube => 3,
            Self::FungalGrotto => 4,
            Self::WormBurrows => 5,
            Self::AncientRuins => 6,
            Self::Chasm => 7,
            Self::CaveNetwork => 8,
        }
    }

    #[must_use]
    pub const fn light(self) -> CaveLight {
        match self {
            Self::CrystalGeode | Self::LavaTube | Self::FungalGrotto => CaveLight::Glowing,
            Self::FloodedCave | Self::AncientRuins => CaveLight::Dim,
            Self::GrandCavern | Self::WormBurrows | Self::Chasm | Self::CaveNetwork => {
                CaveLight::Dark
            }
        }
    }

    /// How dark the cave is with no light at all (0 = fully lit, 255 = black; presentation).
    #[must_use]
    pub const fn ambient_darkness(self) -> u8 {
        match self {
            Self::CrystalGeode => 170,
            Self::LavaTube => 150,
            Self::FungalGrotto => 190,
            Self::FloodedCave => 215,
            Self::AncientRuins => 225,
            Self::GrandCavern | Self::CaveNetwork => 244,
            Self::WormBurrows => 248,
            Self::Chasm => 250,
        }
    }

    /// The critters living in the cave.
    #[must_use]
    pub const fn critters(self) -> &'static [CritterKind] {
        match self {
            Self::GrandCavern => &[CritterKind::Bat, CritterKind::Firefly],
            Self::CrystalGeode => &[CritterKind::GlowMoth],
            Self::FloodedCave => &[CritterKind::CaveFish, CritterKind::Firefly],
            Self::LavaTube => &[CritterKind::EmberMoth],
            Self::FungalGrotto => &[CritterKind::Firefly, CritterKind::GlowMoth],
            Self::WormBurrows | Self::AncientRuins | Self::Chasm => &[CritterKind::Bat],
            Self::CaveNetwork => &[CritterKind::Firefly, CritterKind::Bat],
        }
    }

    /// What may spawn here (see [`CaveSpawnTags`]).
    #[must_use]
    pub const fn spawn_tags(self) -> CaveSpawnTags {
        let none = CaveSpawnTags {
            nests: false,
            swarmers: false,
            aquatic: false,
            heat: false,
            machines: false,
            burrowers: false,
            elite_lair: false,
            danger_per_mille: 1_000,
        };
        match self {
            Self::GrandCavern => CaveSpawnTags {
                nests: true,
                swarmers: true,
                elite_lair: true,
                danger_per_mille: 1_300,
                ..none
            },
            Self::CrystalGeode => CaveSpawnTags {
                danger_per_mille: 700,
                ..none
            },
            Self::FloodedCave => CaveSpawnTags {
                aquatic: true,
                danger_per_mille: 800,
                ..none
            },
            Self::LavaTube => CaveSpawnTags {
                heat: true,
                danger_per_mille: 1_400,
                ..none
            },
            Self::FungalGrotto => CaveSpawnTags {
                swarmers: true,
                nests: true,
                danger_per_mille: 900,
                ..none
            },
            Self::WormBurrows => CaveSpawnTags {
                burrowers: true,
                danger_per_mille: 1_200,
                ..none
            },
            Self::AncientRuins => CaveSpawnTags {
                machines: true,
                elite_lair: true,
                danger_per_mille: 1_200,
                ..none
            },
            Self::Chasm => CaveSpawnTags {
                swarmers: true,
                danger_per_mille: 1_100,
                ..none
            },
            Self::CaveNetwork => CaveSpawnTags {
                swarmers: true,
                burrowers: true,
                ..none
            },
        }
    }
}

/// One tile sample of a cave ([`crate::terrain::Terrain::cave_at`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaveSample {
    pub kind: CaveKind,
    /// The cave's feature id (stable; `Terrain::feature` returns its bounds and centre).
    pub feature: FeatureId,
    /// Inside the cave's open interior (not its walls).
    pub interior: bool,
    /// Open water here (generated; a dug-out tile inside a lake fills with water too).
    pub water: bool,
}

/// A tile only cave ecosystems generate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CaveTile {
    /// Glowing crystal (geode linings and spikes).
    Crystal,
    /// A giant mushroom's stalk.
    MushroomStem,
    /// A giant mushroom's cap: rigs bounce off it.
    MushroomCap,
    /// Small glowing mushrooms on a fungal floor.
    GlowCap,
    /// Soft, fibrous fungal soil.
    Mycelium,
    /// Volcanic glass lining a lava tube.
    Obsidian,
    /// Dressed ancient brick.
    RuinBrick,
    /// An ancient pillar.
    RuinPillar,
    /// A brick carved with a glowing glyph.
    RuinGlyph,
    /// A hanging spike of rock: it falls when a rig passes below.
    Stalactite,
    /// A buried treasure cache: drill it open for credits and loot.
    TreasureCache,
    /// An ancient relic chest (ruins): drill it open for a relic and credits.
    RelicCache,
    /// A rib of old bone in a worm burrow's wall.
    FossilRib,
    /// Faintly glowing algae on a flooded cave's floor.
    GlowAlgae,
    /// Soft silt on a flooded cave's bed.
    Silt,
}

impl CaveTile {
    pub const ALL: [Self; 15] = [
        Self::Crystal,
        Self::MushroomStem,
        Self::MushroomCap,
        Self::GlowCap,
        Self::Mycelium,
        Self::Obsidian,
        Self::RuinBrick,
        Self::RuinPillar,
        Self::RuinGlyph,
        Self::Stalactite,
        Self::TreasureCache,
        Self::RelicCache,
        Self::FossilRib,
        Self::GlowAlgae,
        Self::Silt,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crystal => "crystal",
            Self::MushroomStem => "mushroom stalk",
            Self::MushroomCap => "mushroom cap",
            Self::GlowCap => "glowcap",
            Self::Mycelium => "mycelium",
            Self::Obsidian => "obsidian",
            Self::RuinBrick => "ancient brick",
            Self::RuinPillar => "ancient pillar",
            Self::RuinGlyph => "glyph stone",
            Self::Stalactite => "stalactite",
            Self::TreasureCache => "treasure cache",
            Self::RelicCache => "relic cache",
            Self::FossilRib => "fossil rib",
            Self::GlowAlgae => "glow algae",
            Self::Silt => "silt",
        }
    }

    /// Stable code (tile codes, digests).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Crystal => 0,
            Self::MushroomStem => 1,
            Self::MushroomCap => 2,
            Self::GlowCap => 3,
            Self::Mycelium => 4,
            Self::Obsidian => 5,
            Self::RuinBrick => 6,
            Self::RuinPillar => 7,
            Self::RuinGlyph => 8,
            Self::Stalactite => 9,
            Self::TreasureCache => 10,
            Self::RelicCache => 11,
            Self::FossilRib => 12,
            Self::GlowAlgae => 13,
            Self::Silt => 14,
        }
    }

    /// The tile with code `code` ([`Self::code`]).
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

    /// Drill strength needed (never above the band a cave of the kind generates in).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::MushroomStem
            | Self::MushroomCap
            | Self::GlowCap
            | Self::Mycelium
            | Self::GlowAlgae
            | Self::Silt => 1,
            Self::Stalactite | Self::TreasureCache => 2,
            Self::Crystal
            | Self::RuinBrick
            | Self::RuinPillar
            | Self::RuinGlyph
            | Self::RelicCache
            | Self::FossilRib => 3,
            Self::Obsidian => 4,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::MushroomCap | Self::GlowCap | Self::GlowAlgae | Self::Silt => 2,
            Self::MushroomStem | Self::Mycelium | Self::Stalactite => 3,
            Self::TreasureCache => 4,
            Self::Crystal | Self::FossilRib => 6,
            Self::RuinBrick | Self::RuinGlyph | Self::RelicCache => 8,
            Self::RuinPillar => 9,
            Self::Obsidian => 10,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses (see the drill timing table).
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::MushroomCap | Self::GlowCap | Self::GlowAlgae | Self::Silt => 0.08,
            Self::MushroomStem | Self::Mycelium | Self::Stalactite => 0.1,
            Self::TreasureCache | Self::RelicCache => 0.12,
            Self::Crystal | Self::FossilRib => 0.17,
            Self::RuinBrick | Self::RuinPillar | Self::RuinGlyph => 0.16,
            Self::Obsidian => 0.2,
        }
    }

    /// Light the tile gives off, in tiles (presentation; `0` for none).
    #[must_use]
    pub const fn light_radius(self) -> u8 {
        match self {
            Self::Crystal | Self::GlowCap => 3,
            Self::RuinGlyph | Self::GlowAlgae => 2,
            Self::TreasureCache | Self::RelicCache => 1,
            _ => 0,
        }
    }

    /// Whether this is a cache that pays out when drilled open.
    #[must_use]
    pub const fn is_cache(self) -> bool {
        matches!(self, Self::TreasureCache | Self::RelicCache)
    }
}

/// An ambient cave critter (a harmless entity, see [`crate::entity::CritterState`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CritterKind {
    /// A blinking green-gold light drifting in loops.
    Firefly,
    /// A pale moth that glows softly.
    GlowMoth,
    /// A small bat that flits about and scatters from rigs.
    Bat,
    /// A blind white fish circling in cave water.
    CaveFish,
    /// A spark-winged moth of the lava tubes.
    EmberMoth,
}

impl CritterKind {
    pub const ALL: [Self; 5] = [
        Self::Firefly,
        Self::GlowMoth,
        Self::Bat,
        Self::CaveFish,
        Self::EmberMoth,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Firefly => "firefly",
            Self::GlowMoth => "glow moth",
            Self::Bat => "cave bat",
            Self::CaveFish => "cave fish",
            Self::EmberMoth => "ember moth",
        }
    }

    /// Lives in water (and only there).
    #[must_use]
    pub const fn aquatic(self) -> bool {
        matches!(self, Self::CaveFish)
    }

    /// Light it gives off, in tiles (presentation).
    #[must_use]
    pub const fn light_radius(self) -> f32 {
        match self {
            Self::Firefly => 1.3,
            Self::GlowMoth => 1.1,
            Self::EmberMoth => 1.5,
            Self::Bat | Self::CaveFish => 0.0,
        }
    }
}

/// What a cave cache holds: credits for the opener, loot spilled as item drops.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaveLoot {
    pub credits: u32,
    /// Spilled as floating item drops (`WorldState::spawn_item_drop`; WP-DROPS collects them).
    pub items: Vec<ItemStack>,
}

impl CaveLoot {
    /// Total sale value (credits plus the items' list value).
    #[must_use]
    pub fn value(&self) -> u32 {
        self.credits.saturating_add(
            self.items
                .iter()
                .map(|stack| stack.kind.base_value().saturating_mul(stack.count))
                .sum(),
        )
    }
}

const SALT_CACHE: u64 = 0xCA7E_C0DE;

/// Depth tier of `band` for loot scaling (0 in the clay up to 6 deep down).
const fn loot_tier(band: Band) -> u32 {
    match band {
        Band::Topsoil | Band::Clay => 0,
        Band::Stone => 1,
        Band::Slate => 2,
        Band::Basalt => 3,
        Band::Deep(
            DeepStratum::CrystalFaults
            | DeepStratum::FossilOceans
            | DeepStratum::PressureCathedrals,
        ) => 4,
        Band::Deep(DeepStratum::RadioactiveHollow | DeepStratum::AncientMachineLayer) => 5,
        Band::Deep(DeepStratum::VoidGeodeFields | DeepStratum::MantleStormZone) => 6,
    }
}

/// The gem a cave in `band` grows and hoards.
#[must_use]
pub const fn cave_gem(band: Band) -> MineralKind {
    match loot_tier(band) {
        0 => MineralKind::Silver,
        1 => MineralKind::Gold,
        2 => MineralKind::Emerald,
        3 => MineralKind::Ruby,
        4 => MineralKind::Diamond,
        5 => MineralKind::Platinum,
        _ => MineralKind::Uranium,
    }
}

/// The relic an ancient-ruins cache in `band` holds.
#[must_use]
pub const fn cave_relic(band: Band) -> ArtifactKind {
    match band {
        Band::Topsoil | Band::Clay | Band::Stone => ArtifactKind::OldCircuit,
        Band::Slate | Band::Basalt => ArtifactKind::BuriedIdol,
        Band::Deep(DeepStratum::CrystalFaults) => ArtifactKind::CrystalPrism,
        Band::Deep(DeepStratum::FossilOceans) => ArtifactKind::RadiantShell,
        Band::Deep(DeepStratum::PressureCathedrals) => ArtifactKind::PressureGlyph,
        Band::Deep(DeepStratum::RadioactiveHollow) => ArtifactKind::ReactorBloom,
        Band::Deep(DeepStratum::AncientMachineLayer) => ArtifactKind::MachineSigil,
        Band::Deep(DeepStratum::VoidGeodeFields) => ArtifactKind::VoidPearl,
        Band::Deep(DeepStratum::MantleStormZone) => ArtifactKind::MantleHeart,
    }
}

/// The deterministic contents of the cache `tile` at `position` in `band`.
///
/// A treasure cache pays credits that grow with depth plus a handful of the band's gem; a relic
/// cache (ancient ruins) pays more credits and always holds a relic. Tuned as a top-up next to
/// ore sales, like point-of-interest crates.
#[must_use]
pub fn cache_loot(seed: u64, position: TilePosition, band: Band, tile: CaveTile) -> CaveLoot {
    let roll = hash2(seed, SALT_CACHE, position.x, position.y);
    let tier = loot_tier(band);
    let gem = cave_gem(band);
    match tile {
        CaveTile::TreasureCache => {
            let base = 60 + tier * tier * 55;
            let credits = base + u32::try_from(roll % u64::from(base / 2 + 1)).unwrap_or(0);
            let gems = 2 + u32::try_from((roll >> 16) % 3).unwrap_or(0);
            CaveLoot {
                credits,
                items: vec![ItemStack {
                    kind: ItemKind::Mineral(gem),
                    count: gems,
                }],
            }
        }
        CaveTile::RelicCache => {
            let base = 120 + tier * tier * 80;
            let credits = base + u32::try_from(roll % u64::from(base / 2 + 1)).unwrap_or(0);
            let mut items = vec![ItemStack {
                kind: ItemKind::Artifact(cave_relic(band)),
                count: 1,
            }];
            if (roll >> 24).is_multiple_of(2) {
                items.push(ItemStack {
                    kind: ItemKind::Mineral(gem),
                    count: 1 + u32::try_from((roll >> 32) % 2).unwrap_or(0),
                });
            }
            CaveLoot { credits, items }
        }
        _ => CaveLoot::default(),
    }
}
