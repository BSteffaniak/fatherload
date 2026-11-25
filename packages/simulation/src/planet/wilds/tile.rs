//! The tiles of the wild planet themes (jungle, ocean archipelago, glacier): the
//! [`crate::terrain::TileKind::Wild`] family, codes `0x2000 | WildTile::code` (the planets-a
//! range `0x2000..=0x23FF`).

use serde::{Deserialize, Serialize};

use crate::{
    item::{BlockKind, ItemKind},
    terrain::{ArtifactKind, MineralKind},
};

/// Kind-code range of [`crate::terrain::TileKind::Wild`] tiles in
/// [`crate::terrain::Tile::code`] (`0x2000 | WildTile::code`), clear of every other family.
pub const WILD_TILE_CODE_BASE: u64 = 0x2000;

/// A tile of a wild planet ([`crate::terrain::TileKind::Wild`]; append only, the code is the
/// index in [`WildTile::ALL`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum WildTile {
    // --- Jungle ---
    /// Dark, rich jungle loam: the crust (grass grows on it).
    JungleLoam,
    /// Root Tangle soil: loam knotted with rootlets.
    RootSoil,
    /// A living root: tough, fibrous, and it snags tracks that rub against it.
    LivingRoot,
    /// A giant tree's trunk.
    Trunk,
    /// Canopy leaves: soft, a drill goes through them in a blink.
    Canopy,
    /// Spore Caves stone: grey-green, damp and furred with moss.
    SporeStone,
    /// Mycelium threads through the spore stone.
    Mycel,
    /// A puffcap: a spore-filled cap on a cave floor. Drill it and it bursts.
    Puffcap,
    /// Heartwood: the world tree's ancient root mass, as hard as hard rock.
    Heartwood,
    /// Amber resin with something caught in it (a fossil).
    AmberResin,
    /// Lifesap: a glowing sap node. Drilling it mends the hull.
    Lifesap,
    /// The Elder Tree's bark (the jungle's landmark).
    ElderBark,
    /// The Elder Tree's seed, in the hollow at its foot.
    ElderSeed,
    // --- Ocean archipelago ---
    /// Pale sea sand: beaches and the seabed.
    SeaSand,
    /// The Reef Shelf's limestone.
    Limestone,
    /// Branching coral on the reef.
    Coral,
    /// Brain coral boulders on the reef.
    BrainCoral,
    /// Dark silt stone: the Drowned Ruins' rock.
    SiltStone,
    /// The sunken ruins' cut stone (the walls of the sealed halls).
    RuinStone,
    /// Ruin stone carved with a glowing glyph.
    RuinGlyph,
    /// A relic in a sealed hall.
    RuinRelic,
    /// The Trench's black basalt.
    TrenchBasalt,
    /// Vent rock round a black smoker, crusted with sulphides.
    VentRock,
    /// A black smoker's chimney: it scalds what comes close.
    Smoker,
    /// A giant pearl shell on the seabed.
    Pearlshell,
    // --- Glacier ---
    /// Glacier ice: slick to drive on; tunnels through it refreeze at night.
    GlacierIce,
    /// Old blue ice, harder and slicker.
    BlueIce,
    /// Sea ice: the lid of a frozen sea.
    SeaIce,
    /// Thin rime: a refrozen tunnel. One bite breaks it.
    Rime,
    /// Permafrost: frozen soil and gravel.
    Permafrost,
    /// A frost crystal: a cluster of clear crystal in the permafrost (worth a diamond).
    FrostCrystal,
    /// An icicle hanging in an ice cave: it falls on what passes under it.
    Icicle,
    /// The frozen titan's bone (the glacier's landmark).
    TitanBone,
    /// The frozen titan's heart.
    TitanHeart,
    // --- Any wild theme ---
    /// A small body's secret cache.
    Cache,
}

impl WildTile {
    /// Every wild tile, in code order.
    pub const ALL: [Self; 35] = [
        Self::JungleLoam,
        Self::RootSoil,
        Self::LivingRoot,
        Self::Trunk,
        Self::Canopy,
        Self::SporeStone,
        Self::Mycel,
        Self::Puffcap,
        Self::Heartwood,
        Self::AmberResin,
        Self::Lifesap,
        Self::ElderBark,
        Self::ElderSeed,
        Self::SeaSand,
        Self::Limestone,
        Self::Coral,
        Self::BrainCoral,
        Self::SiltStone,
        Self::RuinStone,
        Self::RuinGlyph,
        Self::RuinRelic,
        Self::TrenchBasalt,
        Self::VentRock,
        Self::Smoker,
        Self::Pearlshell,
        Self::GlacierIce,
        Self::BlueIce,
        Self::SeaIce,
        Self::Rime,
        Self::Permafrost,
        Self::FrostCrystal,
        Self::Icicle,
        Self::TitanBone,
        Self::TitanHeart,
        Self::Cache,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::JungleLoam => "jungle loam",
            Self::RootSoil => "root soil",
            Self::LivingRoot => "living root",
            Self::Trunk => "tree trunk",
            Self::Canopy => "canopy leaves",
            Self::SporeStone => "spore stone",
            Self::Mycel => "mycelium",
            Self::Puffcap => "a puffcap",
            Self::Heartwood => "heartwood",
            Self::AmberResin => "amber resin",
            Self::Lifesap => "lifesap",
            Self::ElderBark => "the Elder Tree's bark",
            Self::ElderSeed => "the Elder Tree's seed",
            Self::SeaSand => "sea sand",
            Self::Limestone => "limestone",
            Self::Coral => "coral",
            Self::BrainCoral => "brain coral",
            Self::SiltStone => "silt stone",
            Self::RuinStone => "ruin stone",
            Self::RuinGlyph => "a glyph stone",
            Self::RuinRelic => "a drowned relic",
            Self::TrenchBasalt => "trench basalt",
            Self::VentRock => "vent rock",
            Self::Smoker => "a black smoker",
            Self::Pearlshell => "a pearl shell",
            Self::GlacierIce => "glacier ice",
            Self::BlueIce => "blue ice",
            Self::SeaIce => "sea ice",
            Self::Rime => "rime",
            Self::Permafrost => "permafrost",
            Self::FrostCrystal => "a frost crystal",
            Self::Icicle => "an icicle",
            Self::TitanBone => "the titan's bone",
            Self::TitanHeart => "the titan's heart",
            Self::Cache => "a buried cache",
        }
    }

    /// Its code (its index in [`Self::ALL`], which is its declaration order).
    #[must_use]
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// The tile with code `code`.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        if code < Self::ALL.len() as u64 {
            #[allow(clippy::cast_possible_truncation, reason = "checked above")]
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// Drill strength needed (the classic ladder: 1 dirt, 2 clay, 3 stone, 4 hard rock).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by material")]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::JungleLoam
            | Self::RootSoil
            | Self::Canopy
            | Self::Puffcap
            | Self::Lifesap
            | Self::SeaSand
            | Self::Rime
            | Self::Icicle => 1,
            Self::Trunk
            | Self::SporeStone
            | Self::Mycel
            | Self::ElderSeed
            | Self::Limestone
            | Self::Coral
            | Self::BrainCoral
            | Self::RuinRelic
            | Self::Pearlshell
            | Self::GlacierIce
            | Self::SeaIce
            | Self::TitanHeart
            | Self::Cache => 2,
            Self::LivingRoot
            | Self::AmberResin
            | Self::SiltStone
            | Self::VentRock
            | Self::BlueIce
            | Self::Permafrost => 3,
            Self::Heartwood
            | Self::ElderBark
            | Self::RuinStone
            | Self::RuinGlyph
            | Self::TrenchBasalt
            | Self::Smoker
            | Self::FrostCrystal
            | Self::TitanBone => 4,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by material")]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Canopy | Self::Puffcap | Self::Rime | Self::Icicle => 1,
            Self::JungleLoam | Self::SeaSand | Self::Lifesap => 2,
            Self::RootSoil | Self::Coral => 3,
            Self::Mycel | Self::BrainCoral | Self::SeaIce => 4,
            Self::Trunk
            | Self::SporeStone
            | Self::Limestone
            | Self::Pearlshell
            | Self::GlacierIce
            | Self::Cache => 5,
            Self::AmberResin | Self::ElderSeed | Self::RuinRelic | Self::TitanHeart => 6,
            Self::SiltStone | Self::BlueIce => 7,
            Self::LivingRoot | Self::VentRock | Self::Permafrost => 8,
            Self::FrostCrystal => 9,
            Self::RuinGlyph | Self::Smoker => 10,
            Self::TrenchBasalt => 11,
            Self::Heartwood | Self::RuinStone | Self::TitanBone => 12,
            Self::ElderBark => 14,
        }
    }

    /// What a broken tile yields: plain material breaks into the classic block of its
    /// hardness, treasure pays out as an artifact or a gem, leaves, sap and ice give nothing.
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::AmberResin => ItemKind::Artifact(ArtifactKind::Fossil),
            Self::ElderSeed => ItemKind::Artifact(ArtifactKind::RadiantShell),
            Self::RuinRelic => ItemKind::Artifact(ArtifactKind::PressureGlyph),
            Self::Pearlshell => ItemKind::Artifact(ArtifactKind::CloudPearl),
            Self::TitanHeart => ItemKind::Artifact(ArtifactKind::MantleHeart),
            Self::Cache => ItemKind::Artifact(ArtifactKind::StarCore),
            Self::FrostCrystal => ItemKind::Mineral(MineralKind::Diamond),
            Self::Canopy
            | Self::Puffcap
            | Self::Lifesap
            | Self::Rime
            | Self::Icicle
            | Self::GlacierIce
            | Self::BlueIce
            | Self::SeaIce => return None,
            Self::JungleLoam | Self::RootSoil | Self::SeaSand => ItemKind::Block(BlockKind::Dirt),
            Self::Trunk | Self::Mycel | Self::Coral | Self::BrainCoral | Self::Limestone => {
                ItemKind::Block(BlockKind::Clay)
            }
            Self::LivingRoot
            | Self::SporeStone
            | Self::SiltStone
            | Self::VentRock
            | Self::Permafrost => ItemKind::Block(BlockKind::Stone),
            Self::Heartwood
            | Self::ElderBark
            | Self::RuinStone
            | Self::RuinGlyph
            | Self::TrenchBasalt
            | Self::Smoker
            | Self::TitanBone => ItemKind::Block(BlockKind::HardRock),
        })
    }

    /// Whether a rig slides on it (glacier physics: little traction, almost no braking).
    #[must_use]
    pub const fn is_slick(self) -> bool {
        matches!(
            self,
            Self::GlacierIce | Self::BlueIce | Self::SeaIce | Self::Rime
        )
    }

    /// Whether it is a treasure (a landmark's or a secret's prize).
    #[must_use]
    pub const fn is_treasure(self) -> bool {
        matches!(
            self,
            Self::ElderSeed | Self::RuinRelic | Self::TitanHeart | Self::Cache
        )
    }

    /// How much light it gives off in the dark (0-255; the renderer's glow).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::Lifesap => 220,
            Self::RuinGlyph => 160,
            Self::Smoker => 140,
            Self::ElderSeed | Self::TitanHeart | Self::RuinRelic | Self::Cache => 120,
            Self::Puffcap => 90,
            Self::FrostCrystal => 70,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WildTile;

    #[test]
    fn codes_are_the_declaration_order_and_round_trip() {
        for (index, tile) in WildTile::ALL.iter().enumerate() {
            assert_eq!(tile.code(), index as u64);
            assert_eq!(WildTile::from_code(tile.code()), Some(*tile));
        }
        assert_eq!(WildTile::from_code(WildTile::ALL.len() as u64), None);
    }
}
