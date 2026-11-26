//! What the crew did in the endgame (Wave 13 endgame).
//!
//! Saved with the world and replicated as the shared [`crate::world::SectionKey::Endgame`]
//! section; hashed only once something happened, so older worlds keep their hashes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::CosmeticItem,
    flight::SystemRef,
    home::blueprint::Expansion,
    ids::PlayerId,
    people::VillageId,
    space::{SpaceBodyId, Theme},
    station::AlienTech,
    terrain::MineralKind,
};

use super::works::{GATE_PARTS, StarwrightPart};

/// An order at a station's Works counter ([`crate::station::StationAction::Works`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WorkOrder {
    /// Buy `units` of a common space material the station stocks.
    BuyMaterial(MineralKind, u16),
    /// Raise an alien tech to its next mark (Mk II, Mk III) at an alien-run station.
    AlienMark(AlienTech),
    /// Commission a Starwright part at a human shipyard.
    Starwright(StarwrightPart),
    /// Fit the Heartline Drive (the capstone) at a human shipyard.
    Heartline,
    /// Deliver part `index` of the Halcyon Gate at Halcyon Dock.
    GatePart(u8),
    /// Ride the finished Halcyon Gate to a system the crew knows.
    GateJump(SystemRef),
    /// Buy a cosmetic from the outfitter.
    Cosmetic(CosmeticItem),
}

/// Why a Works order was refused (the [`EndgameEvent::Refused`] detail).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EndgameRefusal {
    /// Not docked, or this station does not do that.
    NotHere,
    /// Not enough credits.
    Credits,
    /// The bill's materials are not in the cargo bay.
    Materials,
    /// Something must come first (a line at tier 5, a boss, a tongue, the part before).
    Locked,
    /// Already done (the top mark, the part delivered, the item owned).
    Done,
    /// The cargo bay has no room.
    BayFull,
    /// Not a home of yours, or not finished.
    NoHome,
    /// The neighbours already called today.
    AlreadyToday,
    /// The gate is not finished, or the destination is not known.
    NoRoute,
}

impl EndgameRefusal {
    pub const ALL: [Self; 9] = [
        Self::NotHere,
        Self::Credits,
        Self::Materials,
        Self::Locked,
        Self::Done,
        Self::BayFull,
        Self::NoHome,
        Self::AlreadyToday,
        Self::NoRoute,
    ];

    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|refusal| refusal.code() == code)
    }

    /// A HUD line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NotHere => "not offered here",
            Self::Credits => "not enough credits",
            Self::Materials => "the bill's materials aren't in the bay",
            Self::Locked => "something must come first",
            Self::Done => "already done",
            Self::BayFull => "no room in the cargo bay",
            Self::NoHome => "not a finished home of yours",
            Self::AlreadyToday => "the neighbours already called today",
            Self::NoRoute => "the gate can't reach there",
        }
    }
}

/// What happened in the endgame (netcode `1060 + code`; 1060-1079 is this track's range).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EndgameEvent {
    /// Space materials bought (detail: credits spent).
    MaterialsBought,
    /// An alien tech raised a mark (detail: the new mark, 2 or 3).
    MarkRaised,
    /// A Starwright part fitted (detail: [`StarwrightPart::code`]).
    StarwrightFitted,
    /// The Heartline Drive fitted.
    HeartlineFitted,
    /// A part of the Halcyon Gate delivered (detail: the part's index).
    GatePartDelivered,
    /// The Halcyon Gate is finished (detail: parts).
    GateFinished,
    /// A ride through the gate began (detail: the target system's id, low bits).
    GateJumped,
    /// A cosmetic bought (detail: credits spent).
    CosmeticBought,
    /// A home on an alien world grew (detail: the expansions it has now).
    EstateExpanded,
    /// The neighbours called (detail: units of the gift).
    NeighboursCalled,
    /// A world landed on for the first time (detail: worlds landed).
    WorldLanded,
    /// A boss on the hunt list fell (detail: the target's code).
    QuarryDown,
    /// Refused (detail: an [`EndgameRefusal`] code).
    Refused,
}

impl EndgameEvent {
    pub const ALL: [Self; 13] = [
        Self::MaterialsBought,
        Self::MarkRaised,
        Self::StarwrightFitted,
        Self::HeartlineFitted,
        Self::GatePartDelivered,
        Self::GateFinished,
        Self::GateJumped,
        Self::CosmeticBought,
        Self::EstateExpanded,
        Self::NeighboursCalled,
        Self::WorldLanded,
        Self::QuarryDown,
        Self::Refused,
    ];

    /// Wire code offset (netcode adds 1060).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// A short HUD line.
    #[must_use]
    pub const fn headline(self) -> &'static str {
        match self {
            Self::MaterialsBought => "MATERIALS LOADED",
            Self::MarkRaised => "ALIEN TECH MARK RAISED",
            Self::StarwrightFitted => "STARWRIGHT PART FITTED",
            Self::HeartlineFitted => "THE HEARTLINE DRIVE IS YOURS",
            Self::GatePartDelivered => "GATE PART DELIVERED",
            Self::GateFinished => "THE HALCYON GATE IS OPEN",
            Self::GateJumped => "GATE JUMP",
            Self::CosmeticBought => "OUTFITTED",
            Self::EstateExpanded => "YOUR ESTATE GREW",
            Self::NeighboursCalled => "THE NEIGHBOURS CALLED",
            Self::WorldLanded => "NEW WORLD",
            Self::QuarryDown => "HUNT LIST: QUARRY DOWN",
            Self::Refused => "UNAVAILABLE",
        }
    }
}

/// One player's endgame record.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Career {
    /// Every world landed on.
    #[serde(default)]
    pub landed: BTreeSet<SpaceBodyId>,
    /// The planet themes of those worlds.
    #[serde(default)]
    pub themes: BTreeSet<Theme>,
    /// Alien tech marks over Mk I, by [`AlienTech`] (0-2: Mk I to Mk III).
    #[serde(default)]
    pub marks: [u8; 4],
    /// Starwright parts fitted ([`StarwrightPart::bit`]).
    #[serde(default)]
    pub starwright: u8,
    /// The Heartline Drive.
    #[serde(default)]
    pub heartline: bool,
    /// Space material units mined from belts and comets (the achievements' count).
    #[serde(default)]
    pub mined: u32,
    /// Comet dust gathered.
    #[serde(default)]
    pub comet_dust: u32,
    /// Cosmetics bought at outfitters.
    #[serde(default)]
    pub outfitted: u16,
    /// Space materials catalogued: mined at least once (bit per index in
    /// [`super::materials::SPACE_MATERIALS`]).
    #[serde(default)]
    pub catalogue: u16,
}

impl Career {
    /// The mark of `tech` (1-3) if fitted.
    #[must_use]
    pub const fn mark(&self, tech: AlienTech) -> u8 {
        1 + self.marks[tech as usize]
    }

    /// Whether the Starwright part is fitted.
    #[must_use]
    pub const fn has_starwright(&self, part: StarwrightPart) -> bool {
        self.starwright & part.bit() != 0
    }

    /// Whether the whole Starwright ship is fitted.
    #[must_use]
    pub const fn starwright_ship(&self) -> bool {
        self.has_starwright(StarwrightPart::Hull) && self.has_starwright(StarwrightPart::Engine)
    }

    /// Whether nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// A home on an alien world grown into an estate.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Estate {
    /// The expansions raised, in order.
    #[serde(default)]
    pub expansions: Vec<Expansion>,
    /// The people-clock day the neighbours last called.
    #[serde(default)]
    pub visited_day: Option<u32>,
    /// Gifts the neighbours left (units, all time).
    #[serde(default)]
    pub gifts: u32,
}

/// The crew's endgame state (world field `endgame`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EndgameState {
    /// Each player's record (kept when they leave, like their alien tongues).
    #[serde(default, with = "crate::pair_list")]
    pub careers: BTreeMap<PlayerId, Career>,
    /// The Halcyon Gate's parts delivered ([`GATE_PARTS`] bits) and who delivered each.
    #[serde(default)]
    pub gate_parts: u16,
    #[serde(default, with = "crate::pair_list")]
    pub gate_builders: BTreeMap<u8, PlayerId>,
    /// Theme bosses beaten, by planet theme (TODO-SEAM(w13-planets-c/d): the theme tracks
    /// call [`crate::WorldState::record_theme_boss`] when theirs falls).
    #[serde(default)]
    pub theme_bosses: BTreeSet<u16>,
    /// Homes on alien worlds grown into estates, by (town, plot).
    #[serde(default, with = "crate::pair_list")]
    pub estates: BTreeMap<(VillageId, u8), Estate>,
}

impl EndgameState {
    /// Whether nothing happened yet (not hashed then).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// `player`'s record (empty if none).
    #[must_use]
    pub fn career(&self, player: PlayerId) -> Career {
        self.careers.get(&player).cloned().unwrap_or_default()
    }

    /// `player`'s record to change.
    pub fn career_mut(&mut self, player: PlayerId) -> &mut Career {
        self.careers.entry(player).or_default()
    }

    /// Whether part `index` of the gate is delivered.
    #[must_use]
    pub const fn gate_part(&self, index: u8) -> bool {
        index < GATE_PARTS && self.gate_parts & (1 << index) != 0
    }

    /// Parts delivered.
    #[must_use]
    pub const fn gate_parts_done(&self) -> u8 {
        #[allow(clippy::cast_possible_truncation, reason = "at most sixteen bits")]
        {
            self.gate_parts.count_ones() as u8
        }
    }

    /// Whether the Halcyon Gate is finished.
    #[must_use]
    pub const fn gate_open(&self) -> bool {
        self.gate_parts_done() >= GATE_PARTS
    }

    /// The estate at (`town`, `plot`).
    #[must_use]
    pub fn estate(&self, town: VillageId, plot: u8) -> Estate {
        self.estates.get(&(town, plot)).cloned().unwrap_or_default()
    }
}
