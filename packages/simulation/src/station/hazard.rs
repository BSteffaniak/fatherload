//! A world's hazard tags and the planet gear that counters them.
//!
//! Every landable body carries zero or more [`PlanetHazard`] tags, each mild (1) or severe (2)
//! ([`hazards_of`]). On the body's ground a rig without the gear for a tag pays for it every
//! second ([`HazardDrain`]): heat cooks the hull, cold burns fuel to keep the rig warm, toxic
//! air eats into the air supply, and high gravity makes every second of thrust cost more fuel.
//! Gear of a lower tier than the tag's severity halves the drain; a tier at or over it stops it.
//!
//! The tags come from the body's physical parameters (temperature, air and gravity) the space
//! generator rolled, joined by its planet theme's own tags
//! ([`crate::planet::PlanetThemeHooks::hazard_tags`]: the jungle's spores, the volcanic world's
//! heat and ash, the glacier's cold, ...). A theme whose world rules apply its hazards zone by
//! zone ([`crate::planet::PlanetThemeHooks::rules_own_hazards`]: the wild and frontier worlds)
//! reads the same planet gear through [`gear_tier`], and the flat drain stands back on its
//! ground so nothing bites twice (Wave 12 integration).

use serde::{Deserialize, Serialize};

use super::shipyard::{ShipGear, SpaceGear};
use crate::space::{Atmosphere, SpaceBody};

/// A world's hazard tag.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PlanetHazard {
    Heat,
    Cold,
    ToxicAir,
    HighGravity,
}

impl PlanetHazard {
    pub const ALL: [Self; 4] = [Self::Heat, Self::Cold, Self::ToxicAir, Self::HighGravity];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Heat => "heat",
            Self::Cold => "cold",
            Self::ToxicAir => "toxic air",
            Self::HighGravity => "high gravity",
        }
    }

    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// Its bit (a set of tags as a byte).
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// The gear that counters it.
    #[must_use]
    pub const fn counter(self) -> SpaceGear {
        match self {
            Self::Heat => SpaceGear::HeatShielding,
            Self::Cold => SpaceGear::ColdInsulation,
            Self::ToxicAir => SpaceGear::ToxicFilter,
            Self::HighGravity => SpaceGear::GravityStruts,
        }
    }
}

/// Mean surface temperature (°C) from which a world is hot, and severely so.
pub const HOT_C: i16 = 120;
pub const SCORCHING_C: i16 = 300;
/// Mean surface temperature (°C) at and below which a world is cold, and severely so.
pub const COLD_C: i16 = -90;
pub const FROZEN_C: i16 = -170;
/// Surface gravity (milli-g) from which a world is heavy, and severely so.
pub const HEAVY_MILLI_G: u16 = 1_400;
pub const CRUSHING_MILLI_G: u16 = 1_900;

/// A world's hazard tags with their severity (1 mild, 2 severe), in [`PlanetHazard::ALL`]
/// order.
#[must_use]
pub fn hazards_of(body: &SpaceBody) -> Vec<(PlanetHazard, u8)> {
    if body.is_home() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let heat = if body.atmosphere == Atmosphere::Scalding || body.temperature_c >= SCORCHING_C {
        2
    } else {
        u8::from(body.temperature_c >= HOT_C)
    };
    if heat > 0 {
        out.push((PlanetHazard::Heat, heat));
    }
    let cold = if body.temperature_c <= FROZEN_C {
        2
    } else {
        u8::from(body.temperature_c <= COLD_C)
    };
    if cold > 0 {
        out.push((PlanetHazard::Cold, cold));
    }
    let toxic = match body.atmosphere {
        Atmosphere::Toxic => 2,
        Atmosphere::Spores => 1,
        _ => 0,
    };
    if toxic > 0 {
        out.push((PlanetHazard::ToxicAir, toxic));
    }
    let heavy = if body.gravity_milli_g >= CRUSHING_MILLI_G {
        2
    } else {
        u8::from(body.gravity_milli_g >= HEAVY_MILLI_G)
    };
    if heavy > 0 {
        out.push((PlanetHazard::HighGravity, heavy));
    }
    // The theme's own tags (the worst severity of each wins).
    let theme = crate::planet::hooks::hooks(crate::place::planet_theme_of(body.theme));
    for &(hazard, severity) in theme.hazard_tags() {
        match out.iter_mut().find(|(tag, _)| *tag == hazard) {
            Some(entry) => entry.1 = entry.1.max(severity),
            None => out.push((hazard, severity)),
        }
    }
    out.sort_by_key(|(hazard, _)| *hazard);
    out
}

/// Whether the theme of `body` applies its hazards itself (the flat drain stands back).
#[must_use]
pub fn theme_rules_hazards(body: &SpaceBody) -> bool {
    crate::planet::hooks::hooks(crate::place::planet_theme_of(body.theme)).rules_own_hazards()
}

/// The planet-gear tier `gear` holds against `hazard` (0 bare, 1 the first tier, 2 the
/// second): what the theme worlds' own hazard rules read (Wave 12 integration).
#[must_use]
pub const fn gear_tier(gear: &crate::protection::HazardGear, hazard: PlanetHazard) -> u8 {
    gear.ship.gear(hazard.counter())
}

/// How much of a tag of `severity` gets through `gear` (1 all, 0.5 half, 0 none).
#[must_use]
pub const fn exposure(gear: &ShipGear, hazard: PlanetHazard, severity: u8) -> f32 {
    let tier = gear.gear(hazard.counter());
    if tier >= severity {
        0.0
    } else if tier > 0 {
        0.5
    } else {
        1.0
    }
}

// Heat and the fuel drains are shares of the rig's own hull and tank (as the supply line's
// strain is), so a late rig's big hull and tank do not dwarf them.

/// Heat: a share of the rig's hull a second per severity (severe, bare: the hull is gone in
/// about twelve minutes).
pub const HEAT_HULL_SHARE_PER_SECOND: f32 = 0.000_7;
/// Cold: a share of the tank a second per severity (the heaters; severe, bare: about 17
/// minutes from full).
pub const COLD_FUEL_SHARE_PER_SECOND: f32 = 0.000_5;
/// Toxic air: extra air a second per severity (the seals corrode).
pub const TOXIC_AIR_PER_SECOND: f32 = 1.0;
/// High gravity: an extra share of the tank a second of thrust per severity.
pub const GRAVITY_FUEL_SHARE_PER_SECOND: f32 = 0.000_5;

/// What a world's hazards take from a rig in one second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HazardDrain {
    /// A share of the rig's hull ([`crate::Player::max_hull`]).
    pub hull: f32,
    /// A share of the rig's tank.
    pub fuel: f32,
    /// Seconds of air.
    pub air: f32,
    /// Tags biting (not fully countered), as [`PlanetHazard::bit`]s.
    pub biting: u8,
}

/// The drain of `hazards` on a rig with `gear` for one second (`thrusting`: its main
/// thrusters fire).
#[must_use]
pub fn drain(hazards: &[(PlanetHazard, u8)], gear: &ShipGear, thrusting: bool) -> HazardDrain {
    let mut out = HazardDrain::default();
    for &(hazard, severity) in hazards {
        let share = exposure(gear, hazard, severity);
        if share <= 0.0 {
            continue;
        }
        let weight = f32::from(severity) * share;
        match hazard {
            PlanetHazard::Heat => {
                out.hull =
                    (HEAT_HULL_SHARE_PER_SECOND * weight).mul_add(gear.damage_scale(), out.hull);
            }
            PlanetHazard::Cold => out.fuel = COLD_FUEL_SHARE_PER_SECOND.mul_add(weight, out.fuel),
            PlanetHazard::ToxicAir => out.air = TOXIC_AIR_PER_SECOND.mul_add(weight, out.air),
            PlanetHazard::HighGravity => {
                if thrusting {
                    out.fuel = GRAVITY_FUEL_SHARE_PER_SECOND.mul_add(weight, out.fuel);
                }
            }
        }
        out.biting |= hazard.bit();
    }
    out
}
