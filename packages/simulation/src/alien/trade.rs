//! What the species trade (Wave 12 aliens track).
//!
//! Each buys the ores it prizes and sells its own goods, priced from the home market, the
//! species' tastes, the speaker's understanding of the tongue and the body's own economy.
//!
//! The body's economy is the bodylife track's per-body market ([`body_price_permille`], wired
//! on integration): a town on a world rich in an ore pays less for it, one far from home or
//! short of it pays more.

use serde::{Deserialize, Serialize};

use crate::{place::SpaceBodyKey, terrain::MineralKind};

use super::{AlienSpecies, language::Fluency};

/// A good an alien trade hall sells (append-only; saved in events and commands).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AlienGood {
    /// A primer of the species' writing: understanding jumps by [`PRIMER_UNDERSTANDING`].
    Primer(AlienSpecies),
    /// A Vessi song-lantern for a home (a lantern piece).
    SongLantern,
    /// Vessi frost-down padding: hull damage cut for a day (the Ward blessing).
    FrostDown,
    /// Vessi glow pearls: every cargo sale pays a tenth more for a day (the Fortune blessing).
    GlowPearls,
    /// A Tokkari geode lamp for a home (a glowcap lamp piece).
    GeodeLamp,
    /// Tokkari hull plates: the hull patched to full.
    HullPlates,
    /// A Tokkari fuel-knot chart of the veins: thrusters sip fuel for a day (the Thrift
    /// blessing).
    VeinChart,
    /// A Siphra memory glass for a home (a star mobile piece).
    MemoryGlass,
    /// A Siphra air bloom: the rig's air topped up to full.
    AirBloom,
    /// A Siphra star memory: the nearest star system nobody in the crew has charted is added
    /// to the nav chart.
    StarMemory,
}

/// Understanding a primer teaches (per mille of the tongue).
pub const PRIMER_UNDERSTANDING: u16 = 150;

impl AlienGood {
    /// Every good, in order.
    pub const ALL: [Self; 12] = [
        Self::Primer(AlienSpecies::Vessi),
        Self::Primer(AlienSpecies::Tokkari),
        Self::Primer(AlienSpecies::Siphra),
        Self::SongLantern,
        Self::FrostDown,
        Self::GlowPearls,
        Self::GeodeLamp,
        Self::HullPlates,
        Self::VeinChart,
        Self::MemoryGlass,
        Self::AirBloom,
        Self::StarMemory,
    ];

    /// What it is called.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Primer(AlienSpecies::Vessi) => "Vessi song-primer",
            Self::Primer(AlienSpecies::Tokkari) => "Tokkari knot-primer",
            Self::Primer(AlienSpecies::Siphra) => "Siphra memory-primer",
            Self::SongLantern => "Song-lantern",
            Self::FrostDown => "Frost-down padding",
            Self::GlowPearls => "Glow pearls",
            Self::GeodeLamp => "Geode lamp",
            Self::HullPlates => "Tokkari hull plates",
            Self::VeinChart => "Fuel-knot vein chart",
            Self::MemoryGlass => "Memory glass",
            Self::AirBloom => "Air bloom",
            Self::StarMemory => "Star memory",
        }
    }

    /// What it does, in a line.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Primer(_) => "Their writing, word by word: you understand much more.",
            Self::SongLantern => "A lantern that hums a little. For a home.",
            Self::FrostDown => "Padding the cold can't get through: less hull damage for a day.",
            Self::GlowPearls => "Buyers love them: every sale pays a tenth more for a day.",
            Self::GeodeLamp => "A geode that glows from inside. For a home.",
            Self::HullPlates => "Fused-stone plates: the hull patched to full.",
            Self::VeinChart => "Knots that read the veins: thrusters sip fuel for a day.",
            Self::MemoryGlass => "A memory in glass that turns in the light. For a home.",
            Self::AirBloom => "A bloom that breathes out air: tanks topped up to full.",
            Self::StarMemory => "A star they remember: charted on your nav chart.",
        }
    }

    /// Base price (credits) before understanding and the body's economy.
    #[must_use]
    pub const fn base_price(self) -> u32 {
        // On the space credit scale (Wave 13).
        crate::endgame::economy::space_credits(match self {
            Self::Primer(_) => 240,
            Self::SongLantern | Self::GeodeLamp | Self::MemoryGlass => 180,
            Self::FrostDown | Self::GlowPearls | Self::VeinChart => 260,
            Self::HullPlates => 150,
            Self::AirBloom => 90,
            Self::StarMemory => 420,
        })
    }

    /// The home furniture it is, if it is a piece for a home.
    #[must_use]
    pub const fn furniture(self) -> Option<crate::home::decor::Furniture> {
        use crate::home::decor::Furniture;
        match self {
            Self::SongLantern => Some(Furniture::Lantern),
            Self::GeodeLamp => Some(Furniture::GlowcapLamp),
            Self::MemoryGlass => Some(Furniture::StarMobile),
            _ => None,
        }
    }

    /// The blessing it gives for a day, if it is one.
    #[must_use]
    pub const fn blessing(self) -> Option<crate::village::services::BlessingKind> {
        use crate::village::services::BlessingKind;
        match self {
            Self::FrostDown => Some(BlessingKind::Ward),
            Self::GlowPearls => Some(BlessingKind::Fortune),
            Self::VeinChart => Some(BlessingKind::Thrift),
            _ => None,
        }
    }
}

/// The per-body economy's price factor (per mille) for alien trade on `body`.
///
/// The bodylife market's price of `mineral` there in the world of `seed` (its common ores
/// cheap, scarce ones dear, dearer far from home); goods (`None`) trade at the plain price.
#[must_use]
pub fn body_price_permille(seed: u64, body: SpaceBodyKey, mineral: Option<MineralKind>) -> u32 {
    let Some(mineral) = mineral else {
        return 1_000;
    };
    crate::place::planet_params_for(seed, body)
        .and_then(|params| crate::world::body_life_of(seed, Some(body), params))
        .map_or(1_000, |life| life.prices.mineral_permille(mineral))
}

/// Understanding's discount on what you buy and bonus on what you sell (per mille): a
/// stranger who only points pays more and gets less; a fluent speaker haggles.
#[must_use]
pub const fn fluency_permille(fluency: Fluency) -> (u32, u32) {
    // (buy price, sell price)
    match fluency {
        Fluency::Glyphs => (1_150, 850),
        Fluency::Fragments => (1_080, 920),
        Fluency::Conversant => (1_000, 1_000),
        Fluency::Fluent => (940, 1_060),
        Fluency::Native => (900, 1_100),
    }
}

/// What `species` pays for one `mineral` worth `market` credits at home, on `body`, to a
/// speaker of `fluency` (`None` if they do not buy it).
#[must_use]
pub fn ore_price(
    species: AlienSpecies,
    mineral: MineralKind,
    market: u32,
    economy_permille: u32,
    fluency: Fluency,
) -> Option<u32> {
    let taste = u64::from(species.def().values_ore(mineral)?);
    let economy = u64::from(economy_permille);
    let (_, sell) = fluency_permille(fluency);
    let price = u64::from(market) * taste / 1_000 * economy / 1_000 * u64::from(sell) / 1_000;
    Some(u32::try_from(price.max(1)).unwrap_or(u32::MAX))
}

/// What `good` costs where the economy is `economy_permille` ([`body_price_permille`]) for a
/// speaker of `fluency`.
#[must_use]
pub fn good_price(good: AlienGood, economy_permille: u32, fluency: Fluency) -> u32 {
    let economy = u64::from(economy_permille);
    let (buy, _) = fluency_permille(fluency);
    let price = u64::from(good.base_price()) * economy / 1_000 * u64::from(buy) / 1_000;
    u32::try_from(price.max(1)).unwrap_or(u32::MAX)
}
