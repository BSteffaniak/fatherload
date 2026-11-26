//! The tiles of the outland planet themes (Wave 13 planets-c: fungal, machine, dead worlds).
//!
//! The [`crate::terrain::TileKind::Outland`] family, kind codes
//! `OUTLAND_TILE_CODE_BASE + OutlandTile::code` (the planets-c range `0x2E00..=0x31FF`).

use serde::{Deserialize, Serialize};

use crate::{
    item::{BlockKind, ItemKind},
    terrain::{ArtifactKind, MineralKind},
};

/// Kind-code base of [`crate::terrain::TileKind::Outland`] tiles in
/// [`crate::terrain::Tile::code`] (`0x2E00 + OutlandTile::code`), inside the planets-c range
/// `0x2E00..=0x31FF`.
pub const OUTLAND_TILE_CODE_BASE: u64 = 0x2E00;

/// A tile of an outland planet ([`crate::terrain::TileKind::Outland`]; append only, the code is
/// the index in [`OutlandTile::ALL`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum OutlandTile {
    // --- The fungal world ---
    /// Spore loam: soft violet soil furred with mould (the crust).
    SporeLoam,
    /// The Mycelium Mat: a felt of white threads. Dug out, it grows back.
    MyceliumMat,
    /// A knot of tough hyphae in the mat.
    Hyphae,
    /// A giant mushroom's stalk.
    Stalk,
    /// A giant mushroom's cap: springy flesh that throws back what lands on it.
    CapFlesh,
    /// The gills under a cap: soft, a drill parts them in a blink.
    Gills,
    /// A puffball on the ground: drilled, it bursts into a spore cloud.
    Puffball,
    /// Gillstone: the grottos' ribbed grey-violet rock.
    Gillstone,
    /// A glowcap: a bioluminescent mushroom. Drilled, it flashes and stuns the hunters round it.
    Glowcap,
    /// Lumen moss: a faint glow in the gillstone.
    Lumenmoss,
    /// Sporeheart: the deep's dense, spore-packed rock.
    Sporeheart,
    /// A spore vent: it swells, then blasts a column of spores up.
    SporeVent,
    /// A gold truffle (worth a radiant shell to collectors).
    Truffle,
    /// The Mother Cap's glowing flesh (the fungal landmark): springy like any cap.
    MotherCap,
    /// The Sporemother's crown, the fungal boss's relic.
    SporeCrown,
    // --- The machine world ---
    /// Scrap fill: packed scrap between the plates.
    Scrapfill,
    /// Hull plate: the planet's armour, riveted plate.
    HullPlate,
    /// A plate seam: a welded joint in the plating.
    PlateSeam,
    /// A conveyor belt running west: it carries what rests on it.
    ConveyorWest,
    /// A conveyor belt running east.
    ConveyorEast,
    /// A pylon: a mast of girders over the plates.
    Pylon,
    /// A beacon light on a pylon's top.
    Beacon,
    /// Cogwork: the Gear Works' packed machinery.
    Cogwork,
    /// A gear: it turns on the works' rhythm and grinds what touches it.
    Gear,
    /// Conduit: bundles of the Reactor Shell's cables.
    Conduit,
    /// Reactor casing: the core reactor's shielding.
    ReactorCasing,
    /// A dark power relay. Drilled, it flips (and so do its neighbours).
    RelayOff,
    /// A lit power relay. Drilled, it flips back.
    RelayOn,
    /// A power vault's seal: only power opens it.
    PowerSeal,
    /// A power vault's wall (and the core reactor's hall).
    VaultWall,
    /// The reactor heart, the core reactor's relic.
    ReactorHeart,
    // --- The dead world ---
    /// Loose ash: it pours into open space under it.
    AshDrift,
    /// Ash stone: compacted ash.
    AshStone,
    /// Petrified stone: a dead city's walls, turned to rock.
    Petrified,
    /// Petrified glass: a dead city's windows.
    PetrifiedGlass,
    /// A burial urn (worth a clay pot).
    Urn,
    /// Ossuary stone: the Necropolis's rock, packed with old bone.
    Ossuary,
    /// Tomb brick: the tombs' masonry.
    TombBrick,
    /// A tomb's seal: break it and its wardens wake.
    TombSeal,
    /// A sarcophagus in a tomb (worth a buried idol).
    Sarcophagus,
    /// Grave rock: the Grave Mantle's black stone.
    GraveRock,
    /// Soulstone: pale crystal the ghost lights wake in.
    Soulstone,
    /// The Hollow Throne's relic: the best of the builders' relics.
    ThroneRelic,
    // --- Every outland theme ---
    /// A small body's secret cache.
    Cache,
    /// A builders' outpost's dressed stone.
    BuilderStone,
    /// A builders' glyph: a glowing mark the galaxy story reads.
    BuilderGlyph,
    /// A builders' outpost's cache.
    BuilderCache,
    /// A theme boss's seal: while it stands, the boss waits in its lair.
    BossSeal,
    /// A boss's spent seal: the lair's boss is beaten.
    BossSealSpent,
    /// A boss's reward vault door: it opens when the boss falls.
    VaultDoor,
}

impl OutlandTile {
    /// Every outland tile, in code order.
    pub const ALL: [Self; 50] = [
        Self::SporeLoam,
        Self::MyceliumMat,
        Self::Hyphae,
        Self::Stalk,
        Self::CapFlesh,
        Self::Gills,
        Self::Puffball,
        Self::Gillstone,
        Self::Glowcap,
        Self::Lumenmoss,
        Self::Sporeheart,
        Self::SporeVent,
        Self::Truffle,
        Self::MotherCap,
        Self::SporeCrown,
        Self::Scrapfill,
        Self::HullPlate,
        Self::PlateSeam,
        Self::ConveyorWest,
        Self::ConveyorEast,
        Self::Pylon,
        Self::Beacon,
        Self::Cogwork,
        Self::Gear,
        Self::Conduit,
        Self::ReactorCasing,
        Self::RelayOff,
        Self::RelayOn,
        Self::PowerSeal,
        Self::VaultWall,
        Self::ReactorHeart,
        Self::AshDrift,
        Self::AshStone,
        Self::Petrified,
        Self::PetrifiedGlass,
        Self::Urn,
        Self::Ossuary,
        Self::TombBrick,
        Self::TombSeal,
        Self::Sarcophagus,
        Self::GraveRock,
        Self::Soulstone,
        Self::ThroneRelic,
        Self::Cache,
        Self::BuilderStone,
        Self::BuilderGlyph,
        Self::BuilderCache,
        Self::BossSeal,
        Self::BossSealSpent,
        Self::VaultDoor,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SporeLoam => "spore loam",
            Self::MyceliumMat => "mycelium mat",
            Self::Hyphae => "hyphae",
            Self::Stalk => "a mushroom stalk",
            Self::CapFlesh => "a springcap",
            Self::Gills => "mushroom gills",
            Self::Puffball => "a puffball",
            Self::Gillstone => "gillstone",
            Self::Glowcap => "a glowcap",
            Self::Lumenmoss => "lumen moss",
            Self::Sporeheart => "sporeheart",
            Self::SporeVent => "a spore vent",
            Self::Truffle => "a gold truffle",
            Self::MotherCap => "the Mother Cap",
            Self::SporeCrown => "the Sporemother's crown",
            Self::Scrapfill => "scrap fill",
            Self::HullPlate => "hull plate",
            Self::PlateSeam => "a plate seam",
            Self::ConveyorWest | Self::ConveyorEast => "a conveyor belt",
            Self::Pylon => "a pylon",
            Self::Beacon => "a beacon",
            Self::Cogwork => "cogwork",
            Self::Gear => "a gear",
            Self::Conduit => "conduit",
            Self::ReactorCasing => "reactor casing",
            Self::RelayOff => "a dark relay",
            Self::RelayOn => "a lit relay",
            Self::PowerSeal => "a power seal",
            Self::VaultWall => "a vault wall",
            Self::ReactorHeart => "the reactor heart",
            Self::AshDrift => "loose ash",
            Self::AshStone => "ash stone",
            Self::Petrified => "petrified stone",
            Self::PetrifiedGlass => "petrified glass",
            Self::Urn => "a burial urn",
            Self::Ossuary => "ossuary stone",
            Self::TombBrick => "tomb brick",
            Self::TombSeal => "a tomb seal",
            Self::Sarcophagus => "a sarcophagus",
            Self::GraveRock => "grave rock",
            Self::Soulstone => "soulstone",
            Self::ThroneRelic => "the throne relic",
            Self::Cache => "a buried cache",
            Self::BuilderStone => "builder stone",
            Self::BuilderGlyph => "a builder glyph",
            Self::BuilderCache => "a builders' cache",
            Self::BossSeal => "a lair seal",
            Self::BossSealSpent => "a spent lair seal",
            Self::VaultDoor => "a vault door",
        }
    }

    /// Its code (its index in [`Self::ALL`]).
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

    /// Whether no drill ever digs it: vault walls and seals (power or a boss opens them).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::PowerSeal
                | Self::VaultWall
                | Self::BossSeal
                | Self::BossSealSpent
                | Self::VaultDoor
        )
    }

    /// Drill strength needed (the classic ladder: 1 dirt, 2 clay, 3 stone, 4 hard rock).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by material")]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::SporeLoam
            | Self::MyceliumMat
            | Self::CapFlesh
            | Self::Gills
            | Self::Puffball
            | Self::Glowcap
            | Self::Lumenmoss
            | Self::Truffle
            | Self::AshDrift
            | Self::Urn
            | Self::MotherCap => 1,
            Self::Hyphae
            | Self::Stalk
            | Self::Gillstone
            | Self::Scrapfill
            | Self::AshStone
            | Self::PetrifiedGlass
            | Self::TombSeal
            | Self::Soulstone
            | Self::Cache
            | Self::BuilderCache => 2,
            Self::Sporeheart
            | Self::SporeVent
            | Self::HullPlate
            | Self::PlateSeam
            | Self::ConveyorWest
            | Self::ConveyorEast
            | Self::Cogwork
            | Self::Gear
            | Self::RelayOff
            | Self::RelayOn
            | Self::Petrified
            | Self::Ossuary
            | Self::TombBrick
            | Self::Sarcophagus
            | Self::ReactorHeart
            | Self::ThroneRelic
            | Self::SporeCrown => 3,
            _ => 4,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "grouped by material")]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::Gills | Self::Puffball | Self::AshDrift | Self::Lumenmoss => 1,
            Self::SporeLoam | Self::MyceliumMat | Self::Glowcap | Self::Urn | Self::Truffle => 2,
            Self::CapFlesh | Self::MotherCap | Self::Hyphae | Self::Scrapfill => 3,
            Self::Stalk | Self::AshStone | Self::PetrifiedGlass | Self::TombSeal => 4,
            Self::Gillstone
            | Self::Soulstone
            | Self::Cache
            | Self::BuilderCache
            | Self::RelayOff
            | Self::RelayOn => 5,
            Self::Sporeheart | Self::Cogwork | Self::Ossuary | Self::Sarcophagus => 7,
            Self::HullPlate
            | Self::PlateSeam
            | Self::ConveyorWest
            | Self::ConveyorEast
            | Self::Gear
            | Self::Petrified
            | Self::TombBrick
            | Self::ReactorHeart
            | Self::ThroneRelic
            | Self::SporeCrown => 8,
            Self::SporeVent | Self::Conduit | Self::GraveRock | Self::BuilderStone => 10,
            Self::Pylon | Self::ReactorCasing | Self::BuilderGlyph | Self::Beacon => 12,
            _ => 6,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self.hardness() {
            1 => 0.08,
            2 => 0.12,
            3 => 0.15,
            _ => 0.19,
        }
    }

    /// What a broken tile yields: plain ground breaks into the classic block of its hardness,
    /// treasure pays out as an artifact or a gem; flesh, spores, ash and relays give nothing.
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::Truffle => ItemKind::Artifact(ArtifactKind::RadiantShell),
            Self::SporeCrown => ItemKind::Artifact(ArtifactKind::ReactorBloom),
            Self::ReactorHeart => ItemKind::Artifact(ArtifactKind::MachineSigil),
            Self::Urn => ItemKind::Artifact(ArtifactKind::ClayPot),
            Self::Sarcophagus => ItemKind::Artifact(ArtifactKind::BuriedIdol),
            Self::ThroneRelic => ItemKind::Artifact(ArtifactKind::StarCore),
            Self::Cache => ItemKind::Artifact(ArtifactKind::VoidPearl),
            Self::BuilderCache => ItemKind::Artifact(ArtifactKind::OldCircuit),
            Self::Soulstone => ItemKind::Mineral(MineralKind::Emerald),
            Self::CapFlesh
            | Self::Gills
            | Self::Puffball
            | Self::Glowcap
            | Self::Lumenmoss
            | Self::AshDrift
            | Self::RelayOff
            | Self::RelayOn
            | Self::TombSeal
            | Self::MyceliumMat
            | Self::MotherCap
            | Self::PowerSeal
            | Self::VaultWall
            | Self::BossSeal
            | Self::BossSealSpent
            | Self::VaultDoor => return None,
            Self::SporeLoam | Self::AshStone | Self::Scrapfill => ItemKind::Block(BlockKind::Dirt),
            Self::Hyphae | Self::Stalk | Self::Gillstone | Self::PetrifiedGlass => {
                ItemKind::Block(BlockKind::Clay)
            }
            Self::Sporeheart
            | Self::SporeVent
            | Self::HullPlate
            | Self::PlateSeam
            | Self::ConveyorWest
            | Self::ConveyorEast
            | Self::Cogwork
            | Self::Gear
            | Self::Petrified
            | Self::Ossuary
            | Self::TombBrick => ItemKind::Block(BlockKind::Stone),
            Self::Pylon
            | Self::Beacon
            | Self::Conduit
            | Self::ReactorCasing
            | Self::GraveRock
            | Self::BuilderStone
            | Self::BuilderGlyph => ItemKind::Block(BlockKind::HardRock),
        })
    }

    /// Whether it is a treasure (a landmark's, a vault's, an outpost's or a secret's prize).
    #[must_use]
    pub const fn is_treasure(self) -> bool {
        matches!(
            self,
            Self::SporeCrown
                | Self::ReactorHeart
                | Self::ThroneRelic
                | Self::Cache
                | Self::BuilderCache
                | Self::Truffle
                | Self::Sarcophagus
                | Self::Urn
        )
    }

    /// Whether it pours into open space under it (the Ash Beds' rule).
    #[must_use]
    pub const fn is_loose(self) -> bool {
        matches!(self, Self::AshDrift)
    }

    /// Whether it is a springcap a rig bounces on.
    #[must_use]
    pub const fn is_springy(self) -> bool {
        matches!(self, Self::CapFlesh | Self::MotherCap)
    }

    /// The belt's push (-1 west, 1 east), if it is a conveyor.
    #[must_use]
    pub const fn conveyor(self) -> Option<i8> {
        match self {
            Self::ConveyorWest => Some(-1),
            Self::ConveyorEast => Some(1),
            _ => None,
        }
    }

    /// How much light it gives off in the dark (0-255; the renderer's glow).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one row per glow")]
    pub const fn glow(self) -> u8 {
        match self {
            Self::Glowcap => 220,
            Self::MotherCap => 200,
            Self::Beacon => 210,
            Self::RelayOn => 190,
            Self::ReactorHeart => 200,
            Self::BuilderGlyph => 170,
            Self::ThroneRelic | Self::BossSeal | Self::SporeCrown => 160,
            Self::Soulstone => 120,
            Self::Lumenmoss => 90,
            Self::SporeVent => 70,
            Self::Cache | Self::BuilderCache | Self::Truffle => 90,
            Self::RelayOff | Self::PowerSeal => 40,
            Self::VaultDoor => 50,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OutlandTile;

    #[test]
    fn codes_are_the_declaration_order_and_round_trip() {
        for (index, tile) in OutlandTile::ALL.iter().enumerate() {
            assert_eq!(tile.code(), index as u64);
            assert_eq!(OutlandTile::from_code(tile.code()), Some(*tile));
        }
        assert_eq!(OutlandTile::from_code(OutlandTile::ALL.len() as u64), None);
        // The family stays inside the planets-c kind-code range.
        assert!(super::OUTLAND_TILE_CODE_BASE + (OutlandTile::ALL.len() as u64) < 0x3200);
    }
}
