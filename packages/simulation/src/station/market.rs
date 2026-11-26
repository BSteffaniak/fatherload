//! A station's market: trade goods for the ship's hold, and ore from the cargo bay.
//!
//! Every crewed station trades a handful of [`Good`]s. What it makes it sells cheap
//! ([`Role::Makes`], about 30 % under the good's base), what it needs it buys dear
//! ([`Role::Needs`], about 35 % over), and the rest at about the base ([`Role::Trades`]); each
//! station's figures wobble a few percent of their own ([`economy_percent`], the per-station
//! economy hook). Buying from a station pays 8 % over its mid price and selling gets 8 % under.
//! Trading moves the price: every unit a station takes in lowers its price by 1 % and every unit
//! it sells raises it (at most 40 % either way), and the shift fades by 1 % every
//! [`SHIFT_FADE_MILLIS`] of world time. So a good run between a maker and a needer pays well,
//! once, and then the stations have to recover.
//!
//! TODO-SEAM(w12-systems): a per-body or per-system economy (shortages, booms) can fold its
//! figure into [`economy_percent`].

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use serde::{Deserialize, Serialize};

use super::{
    StationId, StationKind,
    profile::{StationOwner, StationProfile},
    station_hash,
};

/// A trade good (the ship's hold carries them; the cargo bay never does).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Good {
    WaterIce,
    Hydroponics,
    MachineParts,
    MedKits,
    ReactorCells,
    AlienSpice,
    XenoSilk,
    SalvagedAlloy,
    /// A builders' relic from a derelict (salvage only; collectors pay well).
    BuilderRelic,
}

impl Good {
    pub const ALL: [Self; 9] = [
        Self::WaterIce,
        Self::Hydroponics,
        Self::MachineParts,
        Self::MedKits,
        Self::ReactorCells,
        Self::AlienSpice,
        Self::XenoSilk,
        Self::SalvagedAlloy,
        Self::BuilderRelic,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::WaterIce => "Water Ice",
            Self::Hydroponics => "Hydroponic Greens",
            Self::MachineParts => "Machine Parts",
            Self::MedKits => "Med Kits",
            Self::ReactorCells => "Reactor Cells",
            Self::AlienSpice => "Alien Spice",
            Self::XenoSilk => "Xeno Silk",
            Self::SalvagedAlloy => "Salvaged Alloy",
            Self::BuilderRelic => "Builders' Relic",
        }
    }

    /// Base price, credits a unit.
    #[must_use]
    pub const fn base_price(self) -> u32 {
        match self {
            Self::WaterIce => 40,
            Self::Hydroponics => 55,
            Self::MachineParts => 110,
            Self::MedKits => 150,
            Self::ReactorCells => 240,
            Self::AlienSpice => 190,
            Self::XenoSilk => 320,
            Self::SalvagedAlloy => 130,
            Self::BuilderRelic => 600,
        }
    }

    /// Its wire id (the index in [`Self::ALL`]).
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// How a station deals in a good.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Role {
    /// It makes it: sells cheap.
    Makes,
    /// It needs it: buys dear.
    Needs,
    /// It trades it at about the base.
    Trades,
}

impl Role {
    #[must_use]
    pub const fn percent(self) -> u32 {
        match self {
            Self::Makes => 70,
            Self::Needs => 135,
            Self::Trades => 100,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Makes => "made here",
            Self::Needs => "wanted",
            Self::Trades => "",
        }
    }
}

/// Spread over and under the mid price (percent).
pub const SPREAD_PERCENT: u32 = 8;
/// A trade moves the price this much a unit (percent), to at most [`MAX_SHIFT`].
pub const SHIFT_PER_UNIT: i32 = 1;
pub const MAX_SHIFT: i32 = 40;
/// A shift fades by one percent every this many world milliseconds.
pub const SHIFT_FADE_MILLIS: u64 = 20_000;

const SALT_TWIST: u64 = 0x57A7_0101;
const SALT_WOBBLE: u64 = 0x57A7_0102;

/// The goods a station deals in, with how.
#[must_use]
pub fn goods_of(station: &StationProfile, seed: u64) -> Vec<(Good, Role)> {
    use Good::{
        AlienSpice, BuilderRelic, Hydroponics, MachineParts, MedKits, ReactorCells, SalvagedAlloy,
        WaterIce, XenoSilk,
    };
    use Role::{Makes, Needs, Trades};
    if !station.market {
        return Vec::new();
    }
    let mut goods: Vec<(Good, Role)> = if station.halcyon {
        vec![
            (WaterIce, Needs),
            (Hydroponics, Makes),
            (MachineParts, Trades),
            (MedKits, Makes),
            (ReactorCells, Trades),
            (SalvagedAlloy, Needs),
            (BuilderRelic, Needs),
        ]
    } else {
        match (station.owner, station.kind) {
            (StationOwner::Alien(_), _) => vec![
                (WaterIce, Trades),
                (MachineParts, Needs),
                (MedKits, Needs),
                (AlienSpice, Makes),
                (XenoSilk, Makes),
                (BuilderRelic, Needs),
            ],
            (_, StationKind::TradePost) => vec![
                (WaterIce, Trades),
                (Hydroponics, Trades),
                (MachineParts, Trades),
                (MedKits, Trades),
                (ReactorCells, Trades),
                (AlienSpice, Needs),
                (XenoSilk, Needs),
                (SalvagedAlloy, Trades),
            ],
            (_, StationKind::Shipyard) => vec![
                (WaterIce, Trades),
                (MachineParts, Makes),
                (MedKits, Trades),
                (ReactorCells, Needs),
                (SalvagedAlloy, Needs),
            ],
            (_, StationKind::Outpost) => vec![
                (WaterIce, Makes),
                (Hydroponics, Needs),
                (MachineParts, Needs),
                (MedKits, Needs),
                (ReactorCells, Makes),
            ],
            (_, StationKind::Relay) => vec![
                (WaterIce, Needs),
                (Hydroponics, Makes),
                (MedKits, Needs),
                (ReactorCells, Trades),
            ],
        }
    };
    // One plain good per station takes a side of its own.
    let twist = station_hash(seed, station.id, SALT_TWIST);
    let plain: Vec<usize> = goods
        .iter()
        .enumerate()
        .filter(|(_, (_, role))| *role == Trades)
        .map(|(index, _)| index)
        .collect();
    if !plain.is_empty() {
        let index = plain[(twist % plain.len() as u64) as usize];
        goods[index].1 = if twist & (1 << 40) == 0 { Makes } else { Needs };
    }
    goods
}

/// The per-station economy hook: the station's percent of a good's base price (its role, and a
/// wobble of up to 8 % its own).
#[must_use]
pub fn economy_percent(seed: u64, station: StationId, good: Good, role: Role) -> u32 {
    let wobble = (station_hash(seed, station, SALT_WOBBLE ^ u64::from(good.code())) % 17) as u32;
    role.percent() + wobble - 8
}

/// A station's price shift on one good (the crew's trades there).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MarketShift {
    /// Percent the price moved (negative: the station is stocked up).
    pub shift: i32,
    /// World millisecond of the last trade.
    pub at: u64,
}

impl MarketShift {
    /// The shift left at world millisecond `now` (fading toward 0).
    #[must_use]
    pub fn at_time(self, now: u64) -> i32 {
        let faded =
            i32::try_from(now.saturating_sub(self.at) / SHIFT_FADE_MILLIS).unwrap_or(MAX_SHIFT);
        if self.shift > 0 {
            (self.shift - faded).max(0)
        } else {
            (self.shift + faded).min(0)
        }
    }

    /// After a trade of `units` (positive: the station sold them, negative: it took them in).
    #[must_use]
    pub fn traded(self, now: u64, units: i32) -> Self {
        Self {
            shift: (self.at_time(now) + units * SHIFT_PER_UNIT).clamp(-MAX_SHIFT, MAX_SHIFT),
            at: now,
        }
    }
}

/// A good's prices at a station: what the player pays a unit, and what the player gets.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Quote {
    pub buy: u32,
    pub sell: u32,
    pub role: Option<Role>,
}

/// `good`'s quote at `station` with the crew's `shift` there at world millisecond `now`
/// (`None`: the station does not deal in it).
#[must_use]
pub fn price(
    seed: u64,
    station: &StationProfile,
    good: Good,
    shift: MarketShift,
    now: u64,
) -> Option<Quote> {
    let role = goods_of(station, seed)
        .into_iter()
        .find(|(found, _)| *found == good)?
        .1;
    let percent = i64::from(economy_percent(seed, station.id, good, role));
    let moved = i64::from(100 + shift.at_time(now));
    let mid = (i64::from(good.base_price()) * percent * moved / 10_000).max(1);
    let spread = i64::from(SPREAD_PERCENT);
    let buy = u32::try_from((mid * (100 + spread) + 99) / 100).unwrap_or(u32::MAX);
    let sell = u32::try_from((mid * (100 - spread) / 100).max(1)).unwrap_or(1);
    Some(Quote {
        buy,
        sell,
        role: Some(role),
    })
}

/// What `station` pays for one unit of ore worth `value` at home.
#[must_use]
pub const fn ore_price(station: &StationProfile, value: u32) -> u32 {
    value.saturating_mul(station.ore_percent) / 100
}
