//! A body's economy (Wave 12 bodylife): what its markets pay, in the one universal currency.
//!
//! **Credits are universal**: a rig's purse is the same at home, on any body and at any
//! station. What changes is what goods are worth where:
//!
//! * **The body's price model** ([`BodyPrices`]) is a pure function of the body's seed, its
//!   theme's ores and how far it lies from home: ore its own ground is full of sells cheap
//!   there ([`COMMON_PERMILLE`]), ore it lacks sells dear ([`SCARCE_PERMILLE`]), more so the
//!   further from home ([`DISTANCE_PERMILLE_PER_SECTOR`], capped), each with a little seeded
//!   jitter so no two bodies agree exactly. Artifacts are curiosities off the home world
//!   ([`ARTIFACT_PERMILLE`]).
//! * **A settlement's local market** ([`LocalMarket`]) adds its own jitter, and every local
//!   day ([`super::clock::BodyClock::local_day`]) it **wants** two goods (they pay
//!   [`WANTED_PERMILLE`] more) and has a **glut** of one (it pays [`GLUT_PERMILLE`] less), so a
//!   hauler who reads the boards and flies between settlements earns more than one who dumps
//!   everything at the nearest counter.
//!
//! Selling ([`crate::WorldState::sell_cargo_at_market`]) pays the rig's minerals and artifacts
//! at the market's prices. The home world's market (its depot, the canonical market and its
//! premiums) never applies on a body: the home depot's commands are refused away from home.
//! The aliens' trade halls price ore by the body's [`BodyPrices`] (Wave 12 integration,
//! `alien::trade::body_price_permille`); a settlement's own [`LocalMarket`] counter (its
//! boards' wanted and glut days) is ready for Wave 13's settlements to open.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "bounded clock, price and weather arithmetic: columns, permille and milliseconds"
)]

use crate::terrain::{ArtifactKind, MineralKind};

/// What ore a body's own ground is rich in sells for there (per mille of its home value).
pub const COMMON_PERMILLE: u32 = 720;
/// What ore a body lacks sells for there, before distance (per mille).
pub const SCARCE_PERMILLE: u32 = 1_250;
/// What every scarce good gains a galactic sector from home (per mille).
pub const DISTANCE_PERMILLE_PER_SECTOR: u32 = 40;
/// The most distance adds (per mille).
pub const MAX_DISTANCE_PERMILLE: u32 = 600;
/// Artifacts off the home world (per mille of their home value).
pub const ARTIFACT_PERMILLE: u32 = 1_200;
/// Seeded jitter either way (per mille).
pub const JITTER_PERMILLE: u32 = 80;
/// A wanted good pays this much more (per mille).
pub const WANTED_PERMILLE: u32 = 600;
/// A glutted good pays this much less (per mille).
pub const GLUT_PERMILLE: u32 = 400;
/// Goods a settlement wants each local day.
pub const WANTED_PER_DAY: usize = 2;

/// Every mineral, in order (the price list's rows).
pub const MINERALS: [MineralKind; 12] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
    MineralKind::Skystone,
    MineralKind::CloudSilver,
];

const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A seeded jitter in `1000 - JITTER ..= 1000 + JITTER`.
const fn jitter(seed: u64, salt: u64) -> u32 {
    let roll =
        mix(seed ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93)) % (2 * JITTER_PERMILLE as u64 + 1);
    (1_000 - JITTER_PERMILLE as u64 + roll) as u32
}

/// `value` at `permille`.
const fn at_permille(value: u32, permille: u32) -> u32 {
    let scaled = value as u64 * permille as u64 / 1_000;
    if scaled > u32::MAX as u64 {
        u32::MAX
    } else {
        scaled as u32
    }
}

/// A body's price model (see the module docs).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BodyPrices {
    /// The body's seed (its ground seed).
    pub seed: u64,
    /// Per mille of home value, by mineral ([`MINERALS`] order).
    pub minerals: [u32; MINERALS.len()],
    /// Per mille of home value for artifacts.
    pub artifacts: u32,
}

impl BodyPrices {
    /// The prices of a body seeded `seed` whose ground is rich in `common` ores, `sectors`
    /// galactic sectors (Chebyshev) from home.
    #[must_use]
    pub fn new(seed: u64, common: &[MineralKind], sectors: u32) -> Self {
        let distance = (sectors * DISTANCE_PERMILLE_PER_SECTOR).min(MAX_DISTANCE_PERMILLE);
        let mut minerals = [0; MINERALS.len()];
        for (index, mineral) in MINERALS.iter().enumerate() {
            let base = if common.contains(mineral) {
                COMMON_PERMILLE
            } else {
                SCARCE_PERMILLE + distance
            };
            minerals[index] = base * jitter(seed, index as u64 + 1) / 1_000;
        }
        Self {
            seed,
            minerals,
            artifacts: ARTIFACT_PERMILLE * jitter(seed, 99) / 1_000,
        }
    }

    /// Per mille of home value `mineral` sells for here.
    #[must_use]
    pub fn mineral_permille(&self, mineral: MineralKind) -> u32 {
        MINERALS
            .iter()
            .position(|found| *found == mineral)
            .map_or(1_000, |index| self.minerals[index])
    }

    /// What one `mineral` sells for here (credits).
    #[must_use]
    pub fn mineral_price(&self, mineral: MineralKind) -> u32 {
        at_permille(mineral.value(), self.mineral_permille(mineral))
    }

    /// What one `artifact` sells for here (credits).
    #[must_use]
    pub const fn artifact_price(&self, artifact: ArtifactKind) -> u32 {
        at_permille(artifact.value(), self.artifacts)
    }
}

/// A settlement's (or a station's) market on a body (see the module docs).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LocalMarket {
    pub prices: BodyPrices,
    /// The settlement's own seed (its id).
    pub settlement: u64,
    /// The local day its boards were chalked for.
    pub day: u64,
}

impl LocalMarket {
    /// The market of settlement `settlement` (any stable id) on a body priced `prices`, on
    /// local day `day`.
    #[must_use]
    pub const fn new(prices: BodyPrices, settlement: u64, day: u64) -> Self {
        Self {
            prices,
            settlement,
            day,
        }
    }

    /// The goods it wants today (they pay [`WANTED_PERMILLE`] more).
    #[must_use]
    pub fn wanted(&self) -> Vec<MineralKind> {
        let mut out = Vec::with_capacity(WANTED_PER_DAY);
        let mut salt = 0_u64;
        while out.len() < WANTED_PER_DAY && salt < 64 {
            let roll = mix(self.settlement ^ mix(self.day.wrapping_add(salt)));
            #[allow(clippy::cast_possible_truncation, reason = "an index under 12")]
            let pick = MINERALS[(roll % MINERALS.len() as u64) as usize];
            if !out.contains(&pick) {
                out.push(pick);
            }
            salt += 1;
        }
        out
    }

    /// The good it has too much of today (it pays [`GLUT_PERMILLE`] less); never a wanted one.
    #[must_use]
    pub fn glut(&self) -> MineralKind {
        let wanted = self.wanted();
        let mut salt = 100_u64;
        loop {
            let roll = mix(self.settlement ^ mix(self.day.wrapping_add(salt)) ^ 0x6C75_7474);
            #[allow(clippy::cast_possible_truncation, reason = "an index under 12")]
            let pick = MINERALS[(roll % MINERALS.len() as u64) as usize];
            if !wanted.contains(&pick) {
                return pick;
            }
            salt += 1;
        }
    }

    /// Per mille of home value `mineral` sells for here today.
    #[must_use]
    pub fn mineral_permille(&self, mineral: MineralKind) -> u32 {
        let mut permille = self.prices.mineral_permille(mineral);
        let index = MINERALS
            .iter()
            .position(|found| *found == mineral)
            .unwrap_or(0);
        permille = permille * jitter(self.settlement, 200 + index as u64) / 1_000;
        if self.wanted().contains(&mineral) {
            permille = permille * (1_000 + WANTED_PERMILLE) / 1_000;
        } else if self.glut() == mineral {
            permille = permille * (1_000 - GLUT_PERMILLE) / 1_000;
        }
        permille
    }

    /// What one `mineral` sells for here today (credits).
    #[must_use]
    pub fn mineral_price(&self, mineral: MineralKind) -> u32 {
        at_permille(mineral.value(), self.mineral_permille(mineral))
    }

    /// What one `artifact` sells for here (credits).
    #[must_use]
    pub const fn artifact_price(&self, artifact: ArtifactKind) -> u32 {
        self.prices.artifact_price(artifact)
    }
}
