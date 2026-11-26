//! Space stations as hubs (Wave 12 stations; expansion plan §16 WP-SPACE "space stations as
//! hubs: shipyards, markets, bounty boards" and "progression in space").
//!
//! Docking (Wave 11 flight) opens a station's hub. What a station offers depends on who runs
//! it ([`StationOwner`]) and what it is for (the generator's [`StationKind`]), both pure
//! functions of the world seed and the station's id ([`profile`]):
//!
//! * **Shipyard** ([`shipyard`]): the ship lines (hull, engine, Star Drive range, hold, life
//!   support; [`ShipLine`]), the planet gear that counters a world's hazard tags ([`SpaceGear`]:
//!   heat shielding, cold insulation, toxic-air filters, high-gravity struts) and, at
//!   alien-run stations only, alien tech ([`AlienTech`]). Fitted gear lives in the rig's loadout
//!   ([`crate::protection::HazardGear::ship`], [`ShipGear`]).
//! * **Market** ([`market`]): trade goods bought and sold for the ship's hold
//!   ([`market::Good`]); every station trades its own goods at its own prices
//!   ([`market::price`], the per-station economy hook), and buys the rig's ore at its own rate.
//! * **Bounty board** ([`bounty`]): three missions a station offers at a time (deliver goods to
//!   another station, scan a body, mine an ore quota on a body); taken, worked anywhere, and
//!   paid out at a station's board. Never a countdown: a mission waits as long as it takes.
//! * **Repair, refuel, save** (Wave 11's counter, priced by the station).
//! * **The bar** ([`bar`]): a few patrons (the people engine's characters), small talk, and for
//!   the price of a round a rumour pointing at a world (its system goes on the nav chart).
//! * **Salvage** at a derelict station: once per world, credits and salvage in the hold.
//!
//! Planet hazards ([`hazard`]): a world's hazard tags (heat, cold, toxic air, high gravity),
//! what they do to a rig on its ground, and the gear that counters each.
//!
//! The crew's shared station state (holds, missions, boards, market shifts, salvage and
//! rumours heard, bodies scanned) is the world's [`StationLedger`] (the shared
//! `SectionKey::Stations` section). Everything is integer-priced and hash-seeded: the same seed
//! gives the same stations, boards, prices and patrons on every peer.
//!
//! # The galaxy round them (wired on the Wave 12 integration)
//!
//! * Alien-run stations are run by the aliens track's species ([`profile::AlienKin::species`]).
//! * The boards post the hostile factions' targets ([`bounty::MissionKind::Bounty`]); the
//!   shipyard sells their counters (the game's hub lists the hostiles' ship gear).
//! * [`hazard::hazards_of`] joins a body's physical tags with its planet theme's own; a theme
//!   that applies its hazards zone by zone reads the planet gear through [`hazard::gear_tier`].
//! * Belt ore mined from flight credits a mine quota ([`crate::WorldState::credit_asteroid_ore`]).
//! * Still open (Wave 13): a per-system economy folding into [`market::economy_percent`].

pub mod balance;
pub mod bar;
pub mod bounty;
pub mod hazard;
pub mod ledger;
pub mod market;
pub mod profile;
pub mod shipyard;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use bounty::{Mission, MissionId, MissionKind};
pub use hazard::PlanetHazard;
pub use ledger::StationLedger;
pub use market::Good;
pub use profile::{StationOwner, StationProfile};
pub use shipyard::{AlienTech, ShipGear, ShipLine, ShipUpgrade, SpaceGear};

pub use crate::space::{StationId, StationKind};

/// A hub action at the station the rig is docked at
/// ([`crate::flight::StationService::Hub`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum StationAction {
    /// Fit the next tier of a ship line, planet gear or an alien tech.
    Fit(ShipUpgrade),
    /// Buy `units` of a good into the hold.
    BuyGood(Good, u16),
    /// Sell `units` of a good from the hold.
    SellGood(Good, u16),
    /// Sell every ore and relic in the cargo bay at the station's rate.
    SellOre,
    /// Take the board's mission in `slot` (0-2).
    Accept(u8),
    /// Give up the player's active mission `index` (its consigned goods go back).
    Abandon(u8),
    /// Collect the rewards of every finished mission (and hand over deliveries here).
    TurnIn,
    /// Buy patron `index` a round (their rumour, if they have one).
    BuyRound(u8),
    /// Strip a derelict station (once per world).
    Salvage,
}

/// Why a hub action was refused (an event's detail).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum StationRefusal {
    /// Not docked.
    NotDocked,
    /// This station does not offer it.
    NotHere,
    Credits,
    /// The hold (or the cargo bay) has no room.
    HoldFull,
    /// Nothing to sell, or the station has none of it.
    Nothing,
    /// The top tier is fitted (or this yard fits no higher).
    Maxed,
    /// Three missions under way already.
    TooManyMissions,
    /// Already salvaged.
    Stripped,
}

impl StationRefusal {
    pub const ALL: [Self; 8] = [
        Self::NotDocked,
        Self::NotHere,
        Self::Credits,
        Self::HoldFull,
        Self::Nothing,
        Self::Maxed,
        Self::TooManyMissions,
        Self::Stripped,
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
            Self::NotDocked => "dock first",
            Self::NotHere => "this station doesn't offer that",
            Self::Credits => "not enough credits",
            Self::HoldFull => "no room in the hold",
            Self::Nothing => "nothing to trade",
            Self::Maxed => "nothing higher fitted here",
            Self::TooManyMissions => "three missions under way already",
            Self::Stripped => "already stripped bare",
        }
    }
}

/// What happened at a station or to a mission (netcode `960 + code`; 960-979 is this track's
/// range).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum StationEvent {
    /// A ship line's next tier was fitted (detail: the new tier).
    ShipUpgraded,
    /// Planet gear fitted (detail: the new tier).
    GearFitted,
    /// An alien tech fitted.
    AlienTechFitted,
    /// Goods bought (detail: credits spent).
    GoodsBought,
    /// Goods sold (detail: credits earned).
    GoodsSold,
    /// Ore sold (detail: credits earned).
    OreSold,
    MissionAccepted,
    /// A mission moved on (detail: its progress).
    MissionProgress,
    /// A mission's work is done: collect at a board.
    MissionReady,
    /// Rewards paid (detail: credits).
    MissionPaid,
    MissionAbandoned,
    /// A patron's rumour (detail: the patron).
    RumourHeard,
    /// A derelict stripped (detail: credits found).
    Salvaged,
    /// A hub action was refused (detail: [`StationRefusal::code`]).
    Refused,
    /// A body was scanned for the first time (detail: the body index).
    BodyScanned,
    /// A planet's hazard bites a rig without the gear for it (detail: [`PlanetHazard::code`]).
    HazardBiting,
}

impl StationEvent {
    pub const ALL: [Self; 16] = [
        Self::ShipUpgraded,
        Self::GearFitted,
        Self::AlienTechFitted,
        Self::GoodsBought,
        Self::GoodsSold,
        Self::OreSold,
        Self::MissionAccepted,
        Self::MissionProgress,
        Self::MissionReady,
        Self::MissionPaid,
        Self::MissionAbandoned,
        Self::RumourHeard,
        Self::Salvaged,
        Self::Refused,
        Self::BodyScanned,
        Self::HazardBiting,
    ];

    /// Wire code offset (netcode adds 960).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// A short HUD line.
    #[must_use]
    pub const fn headline(self) -> &'static str {
        match self {
            Self::ShipUpgraded => "SHIP UPGRADED",
            Self::GearFitted => "GEAR FITTED",
            Self::AlienTechFitted => "ALIEN TECH FITTED",
            Self::GoodsBought => "GOODS LOADED",
            Self::GoodsSold => "GOODS SOLD",
            Self::OreSold => "ORE SOLD",
            Self::MissionAccepted => "MISSION ACCEPTED",
            Self::MissionProgress => "MISSION PROGRESS",
            Self::MissionReady => "MISSION COMPLETE: COLLECT AT ANY BOARD",
            Self::MissionPaid => "BOUNTY PAID",
            Self::MissionAbandoned => "MISSION ABANDONED",
            Self::RumourHeard => "RUMOUR: SYSTEM CHARTED",
            Self::Salvaged => "SALVAGE RECOVERED",
            Self::Refused => "UNAVAILABLE",
            Self::BodyScanned => "BODY SCANNED",
            Self::HazardBiting => "HAZARD: FIT THE RIGHT GEAR",
        }
    }
}

/// A 64-bit mix of a station's id under `seed` and `salt` (every station roll starts here).
#[must_use]
pub const fn station_hash(seed: u64, id: StationId, salt: u64) -> u64 {
    let packed =
        ((id.sector.x.cast_unsigned() as u64) << 32) | (id.sector.y.cast_unsigned() as u64);
    let mut z = seed
        ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ packed.rotate_left(17).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ ((id.system as u64) << 8 | id.index as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
