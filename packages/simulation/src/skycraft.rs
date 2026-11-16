//! Sky crafting (expansion plan WP-SKY, T20): the first Act II upgrades, wrought at sky town
//! counters from sky ore.
//!
//! Three recipe lines are sold only in sky towns ([`crate::village::VillageDeal::SkyCraft`]),
//! each tier paid in credits **and** sky ore from the cargo bay ([`SkyCost`]):
//!
//! | Recipe | Where | Tiers | What it does |
//! |---|---|---|---|
//! | [`SkyRecipe::AetherThrusters`] | airship docks | 1-3 | side thrust that bites in thin air and grips crosswinds |
//! | [`SkyRecipe::PressurizedHull`] | cloud-farmer villages | 1 (of 3) | a sealed cabin: the cold of the high sky bites less |
//! | [`SkyRecipe::DeIcer`] | sky monasteries | the De-Icer line | the shop's De-Icer tiers, cheaper in credits, paid partly in cloud silver |
//!
//! Aether thrusters and the pressurized hull live in [`HazardGear`] (`aether_thrusters`,
//! `pressurized_hull`, serde defaults keep old saves loading); the De-Icer recipe raises the
//! shop line's `de_icer` tier. None of them changes how high a rig climbs on a tank (the climb
//! harness and the dead-air gate, `milestone::crosses_dead_air`, stay honest): thrusters work
//! sideways, the hull keeps out cold.
//!
//! **Seams.** Pressurized hull tiers 2-3 ([`PRESSURIZED_HULL_MAX_LEVEL`]) are reserved for the
//! continents and the Moon (T21/T22): read `HazardGear::pressurized_hull` for "sealed for
//! orbit". Aether thrusters' effect is [`aether_drive_factor`] and [`aether_wind_grip`].

use serde::{Deserialize, Serialize};

use crate::{
    economy::{UpgradeKind, upgrade_cost},
    player::Player,
    protection::HazardGear,
    terrain::MineralKind,
};

/// Top aether thruster tier.
pub const AETHER_THRUSTERS_MAX_LEVEL: u8 = 3;
/// Top pressurized hull tier (tiers above [`PRESSURIZED_HULL_SKY_LEVEL`] are a seam for the
/// sky continents and the Moon).
pub const PRESSURIZED_HULL_MAX_LEVEL: u8 = 3;
/// The pressurized hull tiers sky towns can seal.
pub const PRESSURIZED_HULL_SKY_LEVEL: u8 = 1;
/// Cold levels each pressurized hull tier keeps out (on top of the de-icer).
pub const PRESSURIZED_HULL_COLD_COVER: u8 = 1;

/// A sky-only recipe line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SkyRecipe {
    AetherThrusters,
    PressurizedHull,
    DeIcer,
}

impl SkyRecipe {
    pub const ALL: [Self; 3] = [Self::AetherThrusters, Self::PressurizedHull, Self::DeIcer];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AetherThrusters => "Aether Thrusters",
            Self::PressurizedHull => "Pressurized Hull",
            Self::DeIcer => "Cloud-Silver De-Icer",
        }
    }

    /// What the line does, for the counter.
    #[must_use]
    pub const fn effect(self) -> &'static str {
        match self {
            Self::AetherThrusters => "side thrust that bites in thin air and grips crosswinds",
            Self::PressurizedHull => "a sealed cabin: the high sky's cold bites less",
            Self::DeIcer => "the next De-Icer tier, paid partly in cloud silver",
        }
    }

    /// The highest tier sky towns craft.
    #[must_use]
    pub const fn max_tier(self) -> u8 {
        match self {
            Self::AetherThrusters => AETHER_THRUSTERS_MAX_LEVEL,
            Self::PressurizedHull => PRESSURIZED_HULL_SKY_LEVEL,
            Self::DeIcer => crate::environment::DE_ICER_MAX_LEVEL,
        }
    }

    /// The tier `gear` has of this line.
    #[must_use]
    pub const fn level(self, gear: &HazardGear) -> u8 {
        match self {
            Self::AetherThrusters => gear.aether_thrusters,
            Self::PressurizedHull => gear.pressurized_hull,
            Self::DeIcer => gear.de_icer,
        }
    }
}

/// What one tier of a recipe costs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SkyCost {
    pub credits: u32,
    pub skystone: u32,
    pub cloud_silver: u32,
}

/// The cost of tier `tier` (1-based) of `recipe`.
#[must_use]
pub fn cost(recipe: SkyRecipe, tier: u8) -> SkyCost {
    let t = u32::from(tier.max(1));
    match recipe {
        SkyRecipe::AetherThrusters => SkyCost {
            credits: 700 * t * t + 300,
            skystone: 8 * t + 2,
            cloud_silver: 4 * t - 2,
        },
        SkyRecipe::PressurizedHull => SkyCost {
            credits: 2_400 * t,
            skystone: 12 * t,
            cloud_silver: 8 * t,
        },
        SkyRecipe::DeIcer => SkyCost {
            // 60 % of the shop's price, the rest in cloud silver.
            credits: upgrade_cost(UpgradeKind::DeIcer, tier.saturating_sub(1)) * 60 / 100,
            skystone: 2 * t,
            cloud_silver: 2 * t,
        },
    }
}

/// Why a craft was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CraftRefusal {
    /// Only the next tier can be crafted (or the line is done).
    WrongTier,
    NotEnoughCredits,
    /// The cargo bay lacks the sky ore.
    MissingOre,
}

/// Whether `player` can craft tier `tier` of `recipe` now.
///
/// # Errors
///
/// The [`CraftRefusal`] that stops it.
pub fn can_craft(player: &Player, recipe: SkyRecipe, tier: u8) -> Result<SkyCost, CraftRefusal> {
    if tier != recipe.level(&player.hazard_gear) + 1 || tier > recipe.max_tier() {
        return Err(CraftRefusal::WrongTier);
    }
    let cost = cost(recipe, tier);
    if player.credits < cost.credits {
        return Err(CraftRefusal::NotEnoughCredits);
    }
    if player.cargo_bay.mineral(MineralKind::Skystone) < cost.skystone
        || player.cargo_bay.mineral(MineralKind::CloudSilver) < cost.cloud_silver
    {
        return Err(CraftRefusal::MissingOre);
    }
    Ok(cost)
}

/// Crafts tier `tier` of `recipe` for `player`: takes the credits and the ore, fits the part.
///
/// # Errors
///
/// The [`CraftRefusal`] that stops it (nothing changes then).
pub fn craft(player: &mut Player, recipe: SkyRecipe, tier: u8) -> Result<SkyCost, CraftRefusal> {
    let cost = can_craft(player, recipe, tier)?;
    player.credits -= cost.credits;
    player
        .cargo_bay
        .remove(MineralKind::Skystone, cost.skystone);
    player
        .cargo_bay
        .remove(MineralKind::CloudSilver, cost.cloud_silver);
    let gear = &mut player.hazard_gear;
    match recipe {
        SkyRecipe::AetherThrusters => gear.aether_thrusters = tier,
        SkyRecipe::PressurizedHull => gear.pressurized_hull = tier,
        SkyRecipe::DeIcer => gear.de_icer = tier,
    }
    Ok(cost)
}

/// How hard aether thrusters of `level` push sideways in thin air: the share of the lost
/// side thrust they win back (multiplies the drive's thin-air efficiency up toward 1).
#[must_use]
pub fn aether_drive_factor(level: u8) -> f32 {
    [0.0, 0.35, 0.6, 0.85][usize::from(level.min(AETHER_THRUSTERS_MAX_LEVEL))]
}

/// How much of the wind's push aether thrusters of `level` hold off.
///
/// Crosswinds and the dead-air gap's gales; 0 without them. Jet streams are still a ride: the rig coasts with
/// `1 - grip` of the air's speed.
#[must_use]
pub fn aether_wind_grip(level: u8) -> f32 {
    [0.0, 0.15, 0.3, 0.45][usize::from(level.min(AETHER_THRUSTERS_MAX_LEVEL))]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig_with_ore(skystone: u32, cloud_silver: u32, credits: u32) -> Player {
        let mut player = Player::new(0.0, 0.0);
        player.credits = credits;
        player.cargo_bay.add(MineralKind::Skystone, skystone);
        player.cargo_bay.add(MineralKind::CloudSilver, cloud_silver);
        player
    }

    #[test]
    fn crafting_takes_credits_and_ore_and_fits_the_next_tier() {
        let mut player = rig_with_ore(100, 100, 100_000);
        assert_eq!(
            craft(&mut player, SkyRecipe::AetherThrusters, 2),
            Err(CraftRefusal::WrongTier)
        );
        let cost = craft(&mut player, SkyRecipe::AetherThrusters, 1).expect("crafts");
        assert_eq!(player.hazard_gear.aether_thrusters, 1);
        assert_eq!(player.credits, 100_000 - cost.credits);
        assert_eq!(
            player.cargo_bay.mineral(MineralKind::Skystone),
            100 - cost.skystone
        );
        craft(&mut player, SkyRecipe::PressurizedHull, 1).expect("seals");
        assert_eq!(
            craft(&mut player, SkyRecipe::PressurizedHull, 2),
            Err(CraftRefusal::WrongTier),
            "tier 2 is the continents' to seal"
        );
        craft(&mut player, SkyRecipe::DeIcer, 1).expect("de-icer");
        assert_eq!(player.hazard_gear.de_icer, 1);
    }

    #[test]
    fn crafting_without_ore_or_credits_changes_nothing() {
        let mut player = rig_with_ore(1, 0, 100_000);
        let before = player.clone();
        assert_eq!(
            craft(&mut player, SkyRecipe::AetherThrusters, 1),
            Err(CraftRefusal::MissingOre)
        );
        assert_eq!(player, before);
        let mut poor = rig_with_ore(100, 100, 10);
        assert_eq!(
            craft(&mut poor, SkyRecipe::AetherThrusters, 1),
            Err(CraftRefusal::NotEnoughCredits)
        );
    }

    #[test]
    fn recipes_climb_in_price_and_the_de_icer_is_cheaper_in_credits() {
        for recipe in SkyRecipe::ALL {
            for tier in 1..recipe.max_tier() {
                assert!(cost(recipe, tier + 1).credits > cost(recipe, tier).credits);
            }
        }
        for tier in 1..=crate::environment::DE_ICER_MAX_LEVEL {
            assert!(
                cost(SkyRecipe::DeIcer, tier).credits < upgrade_cost(UpgradeKind::DeIcer, tier - 1)
            );
        }
        assert!(aether_drive_factor(3) > aether_drive_factor(1));
        assert!(aether_wind_grip(0).abs() < f32::EPSILON);
    }
}
