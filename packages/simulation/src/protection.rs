//! Hazard counters: the rig's protective gear lines and how much of each hazard they stop.
//!
//! Every hazard has a counter line bought at the upgrade shop, each tier stopping more of it and
//! the top tier stopping all of it:
//!
//! | Line | Counters |
//! |---|---|
//! | Heat Shielding | band heat (together with the radiator, see [`heat_excess`]) |
//! | Lava Plating | burns from drilling into or skimming past lava |
//! | Gas Scrubber | gas clouds |
//! | Pressure Bracing | pressure pockets, explosive pockets and magma vents |
//! | Boulder Deflector | falling boulders and cave-ins |
//!
//! Heat is modelled per band rather than per row, so it follows the world's strata whatever
//! their depth: each band has a heat level ([`band_heat_level`]) and the rig's cooling
//! ([`heat_cooling`]) covers every band up to that heat. Cooling is counted in thirds of a heat
//! level ([`HEAT_UNITS_PER_LEVEL`]) so every radiator tier helps. Bands hotter than the cooling
//! burn the hull in proportion to the excess.

use serde::{Deserialize, Serialize};

use crate::worldgen::Band;

/// Top Heat Shielding tier.
pub const HEAT_SHIELD_MAX_LEVEL: u8 = 6;
/// Top Lava Plating tier (lava immunity).
pub const LAVA_PLATING_MAX_LEVEL: u8 = 8;
/// Top Gas Scrubber tier (gas immunity).
pub const GAS_SCRUBBER_MAX_LEVEL: u8 = 6;
/// Top Pressure Bracing tier (pressure, blast and vent immunity).
pub const PRESSURE_BRACING_MAX_LEVEL: u8 = 6;
/// Top Boulder Deflector tier (boulder immunity).
pub const BOULDER_DEFLECTOR_MAX_LEVEL: u8 = 6;

/// Heat level of the hottest band.
pub const MAX_BAND_HEAT: u8 = 10;
/// Heat units in one band heat level. Cooling is counted in units so every radiator tier past
/// the fifth still buys a third of a level (no radiator tier is a dead purchase).
pub const HEAT_UNITS_PER_LEVEL: u8 = 3;
/// Hull damage per second for each heat unit a band runs above the rig's cooling (one whole
/// heat level burns 3 hull per second).
pub const HEAT_DAMAGE_PER_EXCESS_UNIT: f32 = 1.0;

/// The hazard counter tiers fitted to a rig (all zero on a stock rig and in old saves).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct HazardGear {
    #[serde(default)]
    pub heat_shield: u8,
    #[serde(default)]
    pub lava_plating: u8,
    #[serde(default)]
    pub gas_scrubber: u8,
    #[serde(default)]
    pub pressure_bracing: u8,
    #[serde(default)]
    pub boulder_deflector: u8,
}

impl HazardGear {
    /// Every counter at its top tier.
    pub const MAXED: Self = Self {
        heat_shield: HEAT_SHIELD_MAX_LEVEL,
        lava_plating: LAVA_PLATING_MAX_LEVEL,
        gas_scrubber: GAS_SCRUBBER_MAX_LEVEL,
        pressure_bracing: PRESSURE_BRACING_MAX_LEVEL,
        boulder_deflector: BOULDER_DEFLECTOR_MAX_LEVEL,
    };

    /// `damage` from a lava burn after the plating.
    #[must_use]
    pub fn lava_damage(self, damage: f32) -> f32 {
        protected_damage(damage, self.lava_plating, LAVA_PLATING_MAX_LEVEL)
    }

    /// `damage` from a gas cloud after the scrubber.
    #[must_use]
    pub fn gas_damage(self, damage: f32) -> f32 {
        protected_damage(damage, self.gas_scrubber, GAS_SCRUBBER_MAX_LEVEL)
    }

    /// `damage` from a pressure pocket, blast or vent after the bracing.
    #[must_use]
    pub fn pressure_damage(self, damage: f32) -> f32 {
        protected_damage(damage, self.pressure_bracing, PRESSURE_BRACING_MAX_LEVEL)
    }

    /// `damage` from a boulder or cave-in after the deflector.
    #[must_use]
    pub fn boulder_damage(self, damage: f32) -> f32 {
        protected_damage(damage, self.boulder_deflector, BOULDER_DEFLECTOR_MAX_LEVEL)
    }

    /// Fraction of a pressure kick (knockback) the bracing absorbs.
    #[must_use]
    pub fn pressure_absorbed(self) -> f32 {
        protection_fraction(self.pressure_bracing, PRESSURE_BRACING_MAX_LEVEL)
    }
}

/// Fraction of a hazard a counter of `level` out of `max_level` stops: `1 - (1 - l/m)²`, so the
/// first tiers pay off quickly and the top tier stops everything.
#[must_use]
pub fn protection_fraction(level: u8, max_level: u8) -> f32 {
    if max_level == 0 || level >= max_level {
        return if max_level == 0 { 0.0 } else { 1.0 };
    }
    let left = f32::from(max_level - level) / f32::from(max_level);
    left.mul_add(-left, 1.0)
}

/// `damage` after a counter of `level` out of `max_level` (exactly zero at the top tier).
#[must_use]
pub fn protected_damage(damage: f32, level: u8, max_level: u8) -> f32 {
    if level >= max_level && max_level > 0 {
        return 0.0;
    }
    (damage * (1.0 - protection_fraction(level, max_level))).max(0.0)
}

/// Heat level of `band`: the topsoil and clay are cool, then every band down runs one level
/// hotter, up to [`MAX_BAND_HEAT`] in the mantle storm zone.
#[must_use]
pub const fn band_heat_level(band: Band) -> u8 {
    #[allow(clippy::cast_possible_truncation, reason = "there are a dozen bands")]
    let heat = band.index().saturating_sub(1) as u8;
    heat
}

/// Heat of `band` in heat units ([`HEAT_UNITS_PER_LEVEL`] per heat level).
#[must_use]
pub const fn band_heat_units(band: Band) -> u8 {
    band_heat_level(band).saturating_mul(HEAT_UNITS_PER_LEVEL)
}

/// Heat units a radiator of `radiator_level` covers: a full heat level per tier up to tier 5
/// (the old top radiator, which reached the first deep stratum), then a third of a level per
/// tier, so every tier cools a little more.
#[must_use]
pub const fn radiator_cooling(radiator_level: u8) -> u8 {
    let level = radiator_level.saturating_sub(1);
    if level <= 4 {
        level * HEAT_UNITS_PER_LEVEL
    } else {
        (4 * HEAT_UNITS_PER_LEVEL).saturating_add(level - 4)
    }
}

/// Heat units a rig with `radiator_level` and `heat_shield` tiers covers (each shielding tier
/// covers a full heat level).
#[must_use]
pub const fn heat_cooling(radiator_level: u8, heat_shield: u8) -> u8 {
    radiator_cooling(radiator_level)
        .saturating_add(heat_shield.saturating_mul(HEAT_UNITS_PER_LEVEL))
}

/// Heat units by which `band` runs hotter than the rig's cooling (zero when it is covered).
#[must_use]
pub const fn heat_excess(band: Band, radiator_level: u8, heat_shield: u8) -> u8 {
    band_heat_units(band).saturating_sub(heat_cooling(radiator_level, heat_shield))
}

/// `units` of cooling as heat levels for display (`"4"`, `"4.3"`), capped at the hottest band.
#[must_use]
pub fn heat_levels_label(units: u8) -> String {
    let units = units.min(MAX_BAND_HEAT * HEAT_UNITS_PER_LEVEL);
    if units.is_multiple_of(HEAT_UNITS_PER_LEVEL) {
        format!("{}", units / HEAT_UNITS_PER_LEVEL)
    } else {
        format!("{:.1}", f32::from(units) / f32::from(HEAT_UNITS_PER_LEVEL))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{economy::MAX_UPGRADE_LEVEL, terrain::DeepStratum};

    #[test]
    fn protection_grows_monotonically_to_immunity() {
        for max in [
            LAVA_PLATING_MAX_LEVEL,
            GAS_SCRUBBER_MAX_LEVEL,
            PRESSURE_BRACING_MAX_LEVEL,
            BOULDER_DEFLECTOR_MAX_LEVEL,
        ] {
            let mut previous = -1.0;
            for level in 0..=max {
                let fraction = protection_fraction(level, max);
                assert!(fraction > previous, "tier {level}/{max} adds protection");
                previous = fraction;
            }
            assert!(protection_fraction(0, max).abs() < f32::EPSILON);
            assert!(protected_damage(50.0, max, max).abs() < f32::EPSILON);
            assert!(protected_damage(50.0, max - 1, max) > 0.0);
        }
    }

    #[test]
    fn maxed_gear_is_immune_to_every_hazard() {
        let gear = HazardGear::MAXED;
        assert!(gear.lava_damage(100.0).abs() < f32::EPSILON);
        assert!(gear.gas_damage(100.0).abs() < f32::EPSILON);
        assert!(gear.pressure_damage(100.0).abs() < f32::EPSILON);
        assert!(gear.boulder_damage(100.0).abs() < f32::EPSILON);
        assert!((gear.pressure_absorbed() - 1.0).abs() < f32::EPSILON);
        let stock = HazardGear::default();
        assert!((stock.lava_damage(100.0) - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn heat_follows_the_bands() {
        assert_eq!(band_heat_level(Band::Topsoil), 0);
        assert_eq!(band_heat_level(Band::Clay), 0);
        assert_eq!(
            band_heat_level(Band::Deep(DeepStratum::MantleStormZone)),
            MAX_BAND_HEAT
        );
        let mut previous = 0;
        for band in Band::ALL {
            assert!(band_heat_level(band) >= previous, "{band:?} is no cooler");
            previous = band_heat_level(band);
        }
    }

    #[test]
    fn radiator_and_shielding_cover_progressively_hotter_bands() {
        // A stock rig is cool in the topsoil and clay but not in the stone.
        assert_eq!(heat_excess(Band::Clay, 1, 0), 0);
        assert!(heat_excess(Band::Stone, 1, 0) > 0);
        // The old top radiator still reaches the first deep stratum.
        assert_eq!(heat_excess(Band::Deep(DeepStratum::CrystalFaults), 5, 0), 0);
        // Every radiator tier cools more than the last (no dead purchases past tier 5).
        let mut previous = radiator_cooling(1);
        for level in 2..=MAX_UPGRADE_LEVEL {
            assert!(radiator_cooling(level) > previous, "radiator tier {level}");
            previous = radiator_cooling(level);
        }
        assert_eq!(heat_levels_label(radiator_cooling(5)), "4");
        assert_eq!(heat_levels_label(radiator_cooling(6)), "4.3");
        let mantle = Band::Deep(DeepStratum::MantleStormZone);
        assert!(heat_excess(mantle, MAX_UPGRADE_LEVEL, 0) > 0);
        assert_eq!(heat_excess(mantle, MAX_UPGRADE_LEVEL, 3), 0);
        assert_eq!(heat_excess(mantle, 5, HEAT_SHIELD_MAX_LEVEL), 0);
        for shield in 0..HEAT_SHIELD_MAX_LEVEL {
            assert!(heat_cooling(5, shield + 1) > heat_cooling(5, shield));
        }
    }
}
