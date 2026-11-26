//! The exotic worlds' tiles ([`crate::terrain::TileKind::Exotic`], planets-d codes
//! `0x3200..=0x35FF`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use serde::{Deserialize, Serialize};

use super::ExoticStyle;
use crate::{
    item::ItemKind,
    terrain::{ArtifactKind, MineralKind},
};

/// Kind-code base of [`crate::terrain::TileKind::Exotic`] tiles: a tile's kind code is
/// `0x3200 + ExoticTile::code` (gas giant `0x32xx`, locked world `0x33xx`, ring world
/// `0x34xx`, shared `0x35xx`).
pub const EXOTIC_TILE_CODE_BASE: u64 = 0x3200;

/// A tile of an exotic world's ground.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ExoticTile {
    // --- The gas giant (codes 0x00..) ---
    /// Ammonia cloud of the Cloud Tops: soft as snow, gone at a touch of the drill.
    CloudIce,
    /// Ochre storm cloud of the Storm Belt.
    StormCloud,
    /// Blue-grey haze of the Pressure Deep, packed by the weight above.
    DeepHaze,
    /// Sooty carbon cloud of the Diamond Rain.
    SootCloud,
    /// A pocket of luminous ammonia gas: harvested as skystone.
    AmmoniaPocket,
    /// A pocket of charged storm gas: harvested as cloud silver.
    StormPocket,
    /// A pocket of heavy deep gas: harvested as platinum.
    DeepPocket,
    /// A hailstone of diamond (what the Diamond Rain leaves).
    DiamondHail,
    /// Metallic hydrogen: silver, cold and dense, deep in the Diamond Rain.
    MetallicHydrogen,
    /// A sky city's deck plating.
    SkyDeck,
    /// A sky city's hull (its keel under the deck).
    CityHull,
    /// A lift bladder under a sky city: a glowing gas cell that holds it up.
    LiftBladder,
    /// A sky city's tower wall.
    CityTower,
    /// A sky city's lantern.
    CityLantern,
    /// The Eye's storm wall: dense, churning cloud.
    EyeWall,
    // --- The locked world (codes 0x100..) ---
    /// Sun-baked hardpan of the day side.
    Hardpan,
    /// A salt crust over the day side's pans.
    SaltCrust,
    /// Sun glass: day-side sand fused by the endless noon.
    SunGlass,
    /// Mesa stone of the day side's flat-topped hills.
    MesaStone,
    /// Nitrogen ice of the night side.
    NitrogenIce,
    /// Frozen regolith of the night side.
    FrostRegolith,
    /// A spike of night-side ice.
    IceSpike,
    /// Lichen soil of the twilight ring.
    LichenSoil,
    /// Glowmoss over the twilight's soil.
    Glowmoss,
    /// Day-side vent rock of the Geyser Fields (sulphur-streaked).
    VentRock,
    /// Night-side cryo rock of the Geyser Fields (frost-veined).
    CryoRock,
    /// A geyser vent.
    GeyserVent,
    /// Fractured rock of the Stress Fractures.
    FractureRock,
    /// Fault rubble: loose rock a quake brings down.
    FaultRubble,
    /// Lodestone of the core.
    Lodestone,
    // --- The ring world (codes 0x200..) ---
    /// Ring dust of the plains.
    RingDust,
    /// Impact breccia.
    Breccia,
    /// Ring ice (the Skyring's islands; lenses buried in the breccia vent when drilled).
    RingIce,
    /// Ringstone of the Skyring's islands.
    Ringstone,
    /// A meteorite fallen from the ring: harvested as platinum.
    Meteorite,
    /// The Tether's builder plate: nothing breaks it.
    TetherPlate,
    /// A live builder conduit: it shocks a rig touching it.
    ConduitLive,
    /// A dead conduit (its junction cut).
    ConduitDead,
    /// A conduit junction: cut it and its run goes dark.
    ConduitJunction,
    /// The Dense Core's heavy rock.
    DenseRock,
    /// A fallen ring monolith on the plains.
    Monolith,
    /// Impact glass on a crater's rim.
    ImpactGlass,
    // --- Shared (codes 0x300..) ---
    /// A builder outpost's plating.
    BuilderPlate,
    /// A builder outpost's beacon (nothing breaks it; the story's marker).
    BuilderBeacon,
    /// A theme boss's seal: nothing breaks it while its boss lives.
    BossSeal,
    /// The Thunderhead's trophy: a storm heart (a wind astrolabe).
    StormTrophy,
    /// The Gloamwarden's trophy: its twin-lit heart (a star core).
    GloamTrophy,
    /// The Ring Warden's trophy: its anchor core (a machine sigil).
    AnchorTrophy,
    /// A secret's cache (a cloud pearl).
    RelicCache,
    /// A landmark's relic (a sky relic).
    LandmarkRelic,
}

impl ExoticTile {
    /// Every tile, in declaration order.
    pub const ALL: [Self; 50] = [
        Self::CloudIce,
        Self::StormCloud,
        Self::DeepHaze,
        Self::SootCloud,
        Self::AmmoniaPocket,
        Self::StormPocket,
        Self::DeepPocket,
        Self::DiamondHail,
        Self::MetallicHydrogen,
        Self::SkyDeck,
        Self::CityHull,
        Self::LiftBladder,
        Self::CityTower,
        Self::CityLantern,
        Self::EyeWall,
        Self::Hardpan,
        Self::SaltCrust,
        Self::SunGlass,
        Self::MesaStone,
        Self::NitrogenIce,
        Self::FrostRegolith,
        Self::IceSpike,
        Self::LichenSoil,
        Self::Glowmoss,
        Self::VentRock,
        Self::CryoRock,
        Self::GeyserVent,
        Self::FractureRock,
        Self::FaultRubble,
        Self::Lodestone,
        Self::RingDust,
        Self::Breccia,
        Self::RingIce,
        Self::Ringstone,
        Self::Meteorite,
        Self::TetherPlate,
        Self::ConduitLive,
        Self::ConduitDead,
        Self::ConduitJunction,
        Self::DenseRock,
        Self::Monolith,
        Self::ImpactGlass,
        Self::BuilderPlate,
        Self::BuilderBeacon,
        Self::BossSeal,
        Self::StormTrophy,
        Self::GloamTrophy,
        Self::AnchorTrophy,
        Self::RelicCache,
        Self::LandmarkRelic,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CloudIce => "ammonia cloud",
            Self::StormCloud => "storm cloud",
            Self::DeepHaze => "deep haze",
            Self::SootCloud => "soot cloud",
            Self::AmmoniaPocket => "ammonia pocket",
            Self::StormPocket => "storm gas pocket",
            Self::DeepPocket => "deep gas pocket",
            Self::DiamondHail => "diamond hail",
            Self::MetallicHydrogen => "metallic hydrogen",
            Self::SkyDeck => "sky deck",
            Self::CityHull => "city hull",
            Self::LiftBladder => "lift bladder",
            Self::CityTower => "city tower",
            Self::CityLantern => "city lantern",
            Self::EyeWall => "storm wall",
            Self::Hardpan => "sunbaked hardpan",
            Self::SaltCrust => "salt crust",
            Self::SunGlass => "sun glass",
            Self::MesaStone => "mesa stone",
            Self::NitrogenIce => "nitrogen ice",
            Self::FrostRegolith => "frozen regolith",
            Self::IceSpike => "ice spike",
            Self::LichenSoil => "lichen soil",
            Self::Glowmoss => "glowmoss",
            Self::VentRock => "vent rock",
            Self::CryoRock => "cryo rock",
            Self::GeyserVent => "geyser vent",
            Self::FractureRock => "fractured rock",
            Self::FaultRubble => "fault rubble",
            Self::Lodestone => "lodestone",
            Self::RingDust => "ring dust",
            Self::Breccia => "impact breccia",
            Self::RingIce => "ring ice",
            Self::Ringstone => "ringstone",
            Self::Meteorite => "meteorite",
            Self::TetherPlate => "tether plate",
            Self::ConduitLive => "live conduit",
            Self::ConduitDead => "dead conduit",
            Self::ConduitJunction => "conduit junction",
            Self::DenseRock => "dense rock",
            Self::Monolith => "ring monolith",
            Self::ImpactGlass => "impact glass",
            Self::BuilderPlate => "builder plate",
            Self::BuilderBeacon => "builder beacon",
            Self::BossSeal => "boss seal",
            Self::StormTrophy => "storm heart",
            Self::GloamTrophy => "gloam heart",
            Self::AnchorTrophy => "anchor core",
            Self::RelicCache => "relic cache",
            Self::LandmarkRelic => "landmark relic",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`], under
    /// [`EXOTIC_TILE_CODE_BASE`]). Never renumber: gas giant `0x00..`, locked `0x100..`,
    /// ring `0x200..`, shared `0x300..`.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::CloudIce => 0x00,
            Self::StormCloud => 0x01,
            Self::DeepHaze => 0x02,
            Self::SootCloud => 0x03,
            Self::AmmoniaPocket => 0x04,
            Self::StormPocket => 0x05,
            Self::DeepPocket => 0x06,
            Self::DiamondHail => 0x07,
            Self::MetallicHydrogen => 0x08,
            Self::SkyDeck => 0x09,
            Self::CityHull => 0x0A,
            Self::LiftBladder => 0x0B,
            Self::CityTower => 0x0C,
            Self::CityLantern => 0x0D,
            Self::EyeWall => 0x0E,
            Self::Hardpan => 0x100,
            Self::SaltCrust => 0x101,
            Self::SunGlass => 0x102,
            Self::MesaStone => 0x103,
            Self::NitrogenIce => 0x104,
            Self::FrostRegolith => 0x105,
            Self::IceSpike => 0x106,
            Self::LichenSoil => 0x107,
            Self::Glowmoss => 0x108,
            Self::VentRock => 0x109,
            Self::CryoRock => 0x10A,
            Self::GeyserVent => 0x10B,
            Self::FractureRock => 0x10C,
            Self::FaultRubble => 0x10D,
            Self::Lodestone => 0x10E,
            Self::RingDust => 0x200,
            Self::Breccia => 0x201,
            Self::RingIce => 0x202,
            Self::Ringstone => 0x203,
            Self::Meteorite => 0x204,
            Self::TetherPlate => 0x205,
            Self::ConduitLive => 0x206,
            Self::ConduitDead => 0x207,
            Self::ConduitJunction => 0x208,
            Self::DenseRock => 0x209,
            Self::Monolith => 0x20A,
            Self::ImpactGlass => 0x20B,
            Self::BuilderPlate => 0x300,
            Self::BuilderBeacon => 0x301,
            Self::BossSeal => 0x302,
            Self::StormTrophy => 0x303,
            Self::GloamTrophy => 0x304,
            Self::AnchorTrophy => 0x305,
            Self::RelicCache => 0x306,
            Self::LandmarkRelic => 0x307,
        }
    }

    /// The tile with code `code`, if any.
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

    /// The world the tile belongs to (`None` for the shared tiles).
    #[must_use]
    pub const fn style(self) -> Option<ExoticStyle> {
        match self.code() >> 8 {
            0 => Some(ExoticStyle::GasGiant),
            1 => Some(ExoticStyle::Locked),
            2 => Some(ExoticStyle::Ring),
            _ => None,
        }
    }

    /// Whether nothing breaks it (the Tether, a beacon, a boss's seal).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::TetherPlate | Self::BuilderBeacon | Self::BossSeal
        )
    }

    /// Whether it is a cloud (soft, gone at a touch; the gas giant's banks).
    #[must_use]
    pub const fn is_cloud(self) -> bool {
        matches!(
            self,
            Self::CloudIce | Self::StormCloud | Self::DeepHaze | Self::SootCloud | Self::EyeWall
        )
    }

    /// Drill strength needed to dig it.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by toughness")]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::CloudIce
            | Self::StormCloud
            | Self::DeepHaze
            | Self::SootCloud
            | Self::EyeWall
            | Self::AmmoniaPocket
            | Self::StormPocket
            | Self::DeepPocket
            | Self::LiftBladder
            | Self::Glowmoss
            | Self::LichenSoil
            | Self::RingDust
            | Self::FaultRubble
            | Self::SaltCrust => 1,
            Self::Hardpan
            | Self::SunGlass
            | Self::NitrogenIce
            | Self::FrostRegolith
            | Self::IceSpike
            | Self::VentRock
            | Self::CryoRock
            | Self::Breccia
            | Self::RingIce
            | Self::Ringstone
            | Self::ConduitDead
            | Self::ImpactGlass
            | Self::DiamondHail
            | Self::Meteorite
            | Self::RelicCache
            | Self::LandmarkRelic => 2,
            Self::MesaStone
            | Self::FractureRock
            | Self::GeyserVent
            | Self::SkyDeck
            | Self::CityHull
            | Self::CityTower
            | Self::CityLantern
            | Self::ConduitLive
            | Self::ConduitJunction
            | Self::Monolith
            | Self::MetallicHydrogen
            | Self::StormTrophy
            | Self::GloamTrophy
            | Self::AnchorTrophy => 3,
            _ => 4,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one row per toughness")]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::CloudIce
            | Self::StormCloud
            | Self::DeepHaze
            | Self::SootCloud
            | Self::Glowmoss
            | Self::FaultRubble => 1,
            Self::EyeWall
            | Self::AmmoniaPocket
            | Self::StormPocket
            | Self::DeepPocket
            | Self::LiftBladder
            | Self::LichenSoil
            | Self::RingDust
            | Self::SaltCrust => 2,
            Self::Hardpan | Self::NitrogenIce | Self::FrostRegolith | Self::IceSpike => 3,
            Self::SunGlass | Self::RingIce | Self::Breccia | Self::DiamondHail => 4,
            Self::VentRock
            | Self::CryoRock
            | Self::Ringstone
            | Self::ImpactGlass
            | Self::ConduitDead
            | Self::Meteorite
            | Self::RelicCache
            | Self::LandmarkRelic => 5,
            Self::MesaStone
            | Self::FractureRock
            | Self::SkyDeck
            | Self::CityHull
            | Self::CityTower
            | Self::CityLantern
            | Self::ConduitJunction
            | Self::ConduitLive
            | Self::Monolith => 7,
            Self::GeyserVent
            | Self::MetallicHydrogen
            | Self::StormTrophy
            | Self::GloamTrophy
            | Self::AnchorTrophy
            | Self::BuilderPlate => 8,
            Self::Lodestone | Self::DenseRock => 10,
            _ => 6,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        if self.is_cloud() {
            return 0.04;
        }
        match self.hardness() {
            1 => 0.08,
            2 => 0.12,
            3 => 0.15,
            _ => 0.19,
        }
    }

    /// The item a broken tile yields: the gas harvest, the meteorites, the prizes.
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        match self {
            Self::AmmoniaPocket => Some(ItemKind::Mineral(MineralKind::Skystone)),
            Self::StormPocket => Some(ItemKind::Mineral(MineralKind::CloudSilver)),
            Self::DeepPocket | Self::Meteorite => Some(ItemKind::Mineral(MineralKind::Platinum)),
            Self::DiamondHail => Some(ItemKind::Mineral(MineralKind::Diamond)),
            Self::StormTrophy => Some(ItemKind::Artifact(ArtifactKind::WindAstrolabe)),
            Self::GloamTrophy => Some(ItemKind::Artifact(ArtifactKind::StarCore)),
            Self::AnchorTrophy => Some(ItemKind::Artifact(ArtifactKind::MachineSigil)),
            Self::RelicCache => Some(ItemKind::Artifact(ArtifactKind::CloudPearl)),
            Self::LandmarkRelic => Some(ItemKind::Artifact(ArtifactKind::SkyRelic)),
            _ => None,
        }
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one row per glow")]
    pub const fn glow(self) -> u8 {
        match self {
            Self::BuilderBeacon => 230,
            Self::StormTrophy | Self::GloamTrophy | Self::AnchorTrophy => 210,
            Self::BossSeal => 170,
            Self::CityLantern => 200,
            Self::LiftBladder => 150,
            Self::ConduitLive | Self::ConduitJunction => 120,
            Self::AmmoniaPocket | Self::StormPocket | Self::DeepPocket => 110,
            Self::DiamondHail => 90,
            Self::Glowmoss => 80,
            Self::GeyserVent => 70,
            Self::MetallicHydrogen | Self::Meteorite | Self::RelicCache | Self::LandmarkRelic => 60,
            Self::SunGlass | Self::ImpactGlass => 30,
            _ => 0,
        }
    }

    /// Whether a quake brings it down when it hangs over open ground.
    #[must_use]
    pub const fn is_loose(self) -> bool {
        matches!(self, Self::FaultRubble)
    }
}
