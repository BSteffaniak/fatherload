//! The shipyard: ship lines, planet gear and alien tech, their prices and what they do.
//!
//! * **Ship lines** ([`ShipLine`], five tiers each, fitted only at a station whose yard reaches
//!   that tier): the **hull** (takes less damage in space and from a world's hazards), the
//!   **engine** (more thrust and impulse speed in space), the **Star Drive range** (warps cost
//!   less fuel, so a tank reaches further), the **hold** (room for trade goods) and **life
//!   support** (more air).
//! * **Planet gear** ([`SpaceGear`], two tiers): heat shielding, cold insulation, toxic-air
//!   filters and high-gravity struts, each countering one of a world's hazard tags
//!   ([`super::hazard`]); tier 2 counters the severe form.
//! * **Alien tech** ([`AlienTech`], one of each, sold only by alien-run stations): the phase
//!   lattice (re-entry shielding), the graviton spool (a quicker warp spool), the xeno lens
//!   (the scanner reaches twice as far) and the symbiont weave (the hull knits itself in space).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use serde::{Deserialize, Serialize};

use super::profile::StationProfile;

/// Top tier of every ship line.
pub const LINE_MAX: u8 = 5;
/// Top tier of every planet gear.
pub const GEAR_MAX: u8 = 2;

/// A ship line.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ShipLine {
    Hull,
    Engine,
    /// Star Drive range: warps cost less fuel.
    DriveRange,
    /// The hold for trade goods.
    Hold,
    LifeSupport,
}

impl ShipLine {
    pub const ALL: [Self; 5] = [
        Self::Hull,
        Self::Engine,
        Self::DriveRange,
        Self::Hold,
        Self::LifeSupport,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hull => "Ship Hull",
            Self::Engine => "Ship Engine",
            Self::DriveRange => "Star Drive Range",
            Self::Hold => "Cargo Hold",
            Self::LifeSupport => "Life Support",
        }
    }

    /// The tier names, 1 to [`LINE_MAX`].
    #[must_use]
    pub const fn tier_name(self, tier: u8) -> &'static str {
        let names: [&str; 5] = match self {
            Self::Hull => [
                "Riveted Hull",
                "Ceramic Skin",
                "Laminate Hull",
                "Lattice Hull",
                "Starforged Hull",
            ],
            Self::Engine => [
                "Twin Ion Engine",
                "Plasma Engine",
                "Fusion Engine",
                "Torch Engine",
                "Sunfire Engine",
            ],
            Self::DriveRange => [
                "Tuned Coils",
                "Long Coils",
                "Folded Coils",
                "Deepwell Coils",
                "Horizon Coils",
            ],
            Self::Hold => [
                "Cargo Pod",
                "Twin Pods",
                "Freight Spine",
                "Bulk Hold",
                "Leviathan Hold",
            ],
            Self::LifeSupport => [
                "Scrubber Pack",
                "Algae Loop",
                "Closed Loop",
                "Biosphere Core",
                "Gardens Deck",
            ],
        };
        if tier == 0 {
            return "stock";
        }
        names[(tier as usize - 1) % 5]
    }

    /// Credits for tier `tier` (1-5): the first is about an hour's earnings on a first trip out
    /// (see [`super::balance`]), each next about 1.9 times the last; on the space credit scale
    /// ([`crate::endgame::economy::SPACE_CREDIT_SCALE`], Wave 13).
    #[must_use]
    pub const fn price(self, tier: u8) -> u32 {
        let base = match self {
            Self::Hull => 7_400,
            Self::Engine => 8_000,
            Self::DriveRange => 6_800,
            Self::Hold => 5_200,
            Self::LifeSupport => 6_000,
        };
        let steps: [u32; 5] = [100, 190, 360, 680, 1_300];
        let index = if tier == 0 { 0 } else { tier as usize - 1 };
        crate::endgame::economy::space_credits(
            base / 100 * steps[if index > 4 { 4 } else { index }],
        )
    }
}

/// Planet gear (each counters one hazard tag).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SpaceGear {
    HeatShielding,
    ColdInsulation,
    ToxicFilter,
    GravityStruts,
}

impl SpaceGear {
    pub const ALL: [Self; 4] = [
        Self::HeatShielding,
        Self::ColdInsulation,
        Self::ToxicFilter,
        Self::GravityStruts,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HeatShielding => "Heat Shielding",
            Self::ColdInsulation => "Cold Insulation",
            Self::ToxicFilter => "Toxic-Air Filters",
            Self::GravityStruts => "High-Gravity Struts",
        }
    }

    /// Credits for tier `tier` (1-2).
    #[must_use]
    pub const fn price(self, tier: u8) -> u32 {
        let base = match self {
            Self::HeatShielding => 2_400,
            Self::ColdInsulation => 2_000,
            Self::ToxicFilter => 2_200,
            Self::GravityStruts => 2_600,
        };
        crate::endgame::economy::space_credits(if tier >= 2 { base * 3 } else { base })
    }
}

/// Alien tech (one of each; alien-run stations only).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum AlienTech {
    /// Re-entry heat as if two more shield tiers were fitted.
    PhaseLattice,
    /// The warp spools in half the time.
    GravitonSpool,
    /// The scanner pings twice as far and scans bodies from twice as far.
    XenoLens,
    /// The hull knits itself in space (up to most of its plating).
    SymbiontWeave,
}

impl AlienTech {
    pub const ALL: [Self; 4] = [
        Self::PhaseLattice,
        Self::GravitonSpool,
        Self::XenoLens,
        Self::SymbiontWeave,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PhaseLattice => "Phase Lattice",
            Self::GravitonSpool => "Graviton Spool",
            Self::XenoLens => "Xeno Lens",
            Self::SymbiontWeave => "Symbiont Weave",
        }
    }

    /// Its bit in [`ShipGear::alien`].
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    #[must_use]
    pub const fn price(self) -> u32 {
        crate::endgame::economy::space_credits(match self {
            Self::PhaseLattice => 7_500,
            Self::GravitonSpool => 9_000,
            Self::XenoLens => 6_500,
            Self::SymbiontWeave => 11_000,
        })
    }

    /// What it does (the shop line).
    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::PhaseLattice => "re-entry heat as if two shield tiers more",
            Self::GravitonSpool => "the warp spools in half the time",
            Self::XenoLens => "scanner reach doubled",
            Self::SymbiontWeave => "the hull knits itself in space",
        }
    }
}

/// Anything the shipyard fits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ShipUpgrade {
    Line(ShipLine),
    Gear(SpaceGear),
    Alien(AlienTech),
}

impl ShipUpgrade {
    /// Every upgrade, in shop order.
    #[must_use]
    pub fn all() -> Vec<Self> {
        ShipLine::ALL
            .into_iter()
            .map(Self::Line)
            .chain(SpaceGear::ALL.into_iter().map(Self::Gear))
            .chain(AlienTech::ALL.into_iter().map(Self::Alien))
            .collect()
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Line(line) => line.name(),
            Self::Gear(gear) => gear.name(),
            Self::Alien(tech) => tech.name(),
        }
    }

    /// The tier fitted.
    #[must_use]
    pub const fn level(self, gear: &ShipGear) -> u8 {
        match self {
            Self::Line(line) => gear.line(line),
            Self::Gear(kind) => gear.gear(kind),
            Self::Alien(tech) => (gear.alien & tech.bit() != 0) as u8,
        }
    }

    /// Top tier.
    #[must_use]
    pub const fn max_level(self) -> u8 {
        match self {
            Self::Line(_) => LINE_MAX,
            Self::Gear(_) => GEAR_MAX,
            Self::Alien(_) => 1,
        }
    }

    /// Credits for the next tier after `level`.
    #[must_use]
    pub const fn price(self, level: u8) -> u32 {
        match self {
            Self::Line(line) => line.price(level + 1),
            Self::Gear(gear) => gear.price(level + 1),
            Self::Alien(tech) => tech.price(),
        }
    }

    /// The highest tier `station` fits of it (0: not sold there).
    #[must_use]
    pub const fn sold_to(self, station: &StationProfile) -> u8 {
        match self {
            Self::Line(_) => station.yard_tier,
            Self::Gear(_) => station.gear_tier,
            Self::Alien(_) => station.alien_tech as u8,
        }
    }

    /// The next tier's effect, as a shop line.
    #[must_use]
    pub fn effect(self, level: u8) -> String {
        let next = level.saturating_add(1);
        match self {
            Self::Line(ShipLine::Hull) => format!(
                "space damage -{}%",
                100 - (damage_scale(next) * 100.0).round() as u32
            ),
            Self::Line(ShipLine::Engine) => format!(
                "thrust and impulse +{}%",
                ((engine_scale(next) - 1.0) * 100.0).round() as u32
            ),
            Self::Line(ShipLine::DriveRange) => format!(
                "warp range +{}%",
                ((drive_range_scale(next) - 1.0) * 100.0).round() as u32
            ),
            Self::Line(ShipLine::Hold) => format!("hold {} units", hold_capacity(next)),
            Self::Line(ShipLine::LifeSupport) => {
                format!("+{} s of air", u32::from(next) * AIR_PER_LINE_TIER as u32)
            }
            Self::Gear(gear) => format!(
                "counters {}{}",
                gear.counters().name(),
                if next >= 2 { " (severe too)" } else { "" }
            ),
            Self::Alien(tech) => tech.effect().to_owned(),
        }
    }
}

/// The ship's fitted lines, planet gear and alien tech (in the rig's loadout,
/// [`crate::protection::HazardGear::ship`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ShipGear {
    #[serde(default)]
    pub hull: u8,
    #[serde(default)]
    pub engine: u8,
    #[serde(default)]
    pub drive_range: u8,
    #[serde(default)]
    pub hold: u8,
    #[serde(default)]
    pub life_support: u8,
    #[serde(default)]
    pub heat_shielding: u8,
    #[serde(default)]
    pub cold_insulation: u8,
    #[serde(default)]
    pub toxic_filter: u8,
    #[serde(default)]
    pub gravity_struts: u8,
    /// Alien tech fitted ([`AlienTech::bit`]).
    #[serde(default)]
    pub alien: u8,
}

impl ShipGear {
    /// Everything at its top tier (test mode).
    pub const MAXED: Self = Self {
        hull: LINE_MAX,
        engine: LINE_MAX,
        drive_range: LINE_MAX,
        hold: LINE_MAX,
        life_support: LINE_MAX,
        heat_shielding: GEAR_MAX,
        cold_insulation: GEAR_MAX,
        toxic_filter: GEAR_MAX,
        gravity_struts: GEAR_MAX,
        alien: 0b1111,
    };

    #[must_use]
    pub const fn line(&self, line: ShipLine) -> u8 {
        match line {
            ShipLine::Hull => self.hull,
            ShipLine::Engine => self.engine,
            ShipLine::DriveRange => self.drive_range,
            ShipLine::Hold => self.hold,
            ShipLine::LifeSupport => self.life_support,
        }
    }

    #[must_use]
    pub const fn gear(&self, gear: SpaceGear) -> u8 {
        match gear {
            SpaceGear::HeatShielding => self.heat_shielding,
            SpaceGear::ColdInsulation => self.cold_insulation,
            SpaceGear::ToxicFilter => self.toxic_filter,
            SpaceGear::GravityStruts => self.gravity_struts,
        }
    }

    /// Whether `tech` is fitted.
    #[must_use]
    pub const fn has(&self, tech: AlienTech) -> bool {
        self.alien & tech.bit() != 0
    }

    /// Fits the next tier of `upgrade` (no checks: the caller priced it).
    pub const fn fit(&mut self, upgrade: ShipUpgrade) {
        match upgrade {
            ShipUpgrade::Line(line) => {
                let slot = match line {
                    ShipLine::Hull => &mut self.hull,
                    ShipLine::Engine => &mut self.engine,
                    ShipLine::DriveRange => &mut self.drive_range,
                    ShipLine::Hold => &mut self.hold,
                    ShipLine::LifeSupport => &mut self.life_support,
                };
                if *slot < LINE_MAX {
                    *slot += 1;
                }
            }
            ShipUpgrade::Gear(gear) => {
                let slot = match gear {
                    SpaceGear::HeatShielding => &mut self.heat_shielding,
                    SpaceGear::ColdInsulation => &mut self.cold_insulation,
                    SpaceGear::ToxicFilter => &mut self.toxic_filter,
                    SpaceGear::GravityStruts => &mut self.gravity_struts,
                };
                if *slot < GEAR_MAX {
                    *slot += 1;
                }
            }
            ShipUpgrade::Alien(tech) => self.alien |= tech.bit(),
        }
    }

    /// Share of space and hazard damage the hull takes.
    #[must_use]
    pub fn damage_scale(&self) -> f32 {
        damage_scale(self.hull)
    }

    /// Thrust and impulse speed scale in space.
    #[must_use]
    pub fn engine_scale(&self) -> f32 {
        engine_scale(self.engine)
    }

    /// Warp range scale (a warp costs its fuel over this).
    #[must_use]
    pub fn drive_range_scale(&self) -> f32 {
        drive_range_scale(self.drive_range)
    }

    /// Units of trade goods the hold carries.
    #[must_use]
    pub const fn hold_capacity(&self) -> u32 {
        hold_capacity(self.hold)
    }

    /// Seconds of air life support adds.
    #[must_use]
    pub fn extra_air(&self) -> f32 {
        f32::from(self.life_support) * AIR_PER_LINE_TIER
    }
}

/// Each hull tier takes this share off space damage (to at most half off).
pub const HULL_DAMAGE_STEP: f32 = 0.1;
/// Each engine tier adds this to thrust and impulse speed in space.
pub const ENGINE_STEP: f32 = 0.12;
/// Each drive range tier adds this to the warp range (warps cost less fuel).
pub const DRIVE_RANGE_STEP: f32 = 0.25;
/// The hold's stock room and each tier's extra (units of trade goods).
pub const HOLD_BASE: u32 = 8;
pub const HOLD_PER_TIER: u32 = 12;
/// Seconds of air each life support tier adds.
pub const AIR_PER_LINE_TIER: f32 = 240.0;
/// The symbiont weave knits this much hull a second in space, up to this share of the plating.
pub const WEAVE_HULL_PER_SECOND: f32 = 0.6;
pub const WEAVE_HULL_SHARE: f32 = 0.7;

/// Share of space damage a hull of tier `tier` takes.
#[must_use]
pub fn damage_scale(tier: u8) -> f32 {
    f32::from(tier.min(LINE_MAX))
        .mul_add(-HULL_DAMAGE_STEP, 1.0)
        .max(0.5)
}

/// Thrust scale of an engine of tier `tier`.
#[must_use]
pub fn engine_scale(tier: u8) -> f32 {
    f32::from(tier.min(LINE_MAX)).mul_add(ENGINE_STEP, 1.0)
}

/// Warp range scale of drive range tier `tier`.
#[must_use]
pub fn drive_range_scale(tier: u8) -> f32 {
    f32::from(tier.min(LINE_MAX)).mul_add(DRIVE_RANGE_STEP, 1.0)
}

/// Units a hold of tier `tier` carries.
#[must_use]
pub const fn hold_capacity(tier: u8) -> u32 {
    let tier = if tier > LINE_MAX { LINE_MAX } else { tier };
    HOLD_BASE + HOLD_PER_TIER * tier as u32
}

impl SpaceGear {
    /// The hazard it counters.
    #[must_use]
    pub const fn counters(self) -> super::PlanetHazard {
        match self {
            Self::HeatShielding => super::PlanetHazard::Heat,
            Self::ColdInsulation => super::PlanetHazard::Cold,
            Self::ToxicFilter => super::PlanetHazard::ToxicAir,
            Self::GravityStruts => super::PlanetHazard::HighGravity,
        }
    }
}
