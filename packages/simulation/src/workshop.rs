//! Layer resources and the deep outposts' workshops (Wave 8, Deep Descent worlds).
//!
//! Every stratum has a layer resource ([`crate::descent::StratumDef::layer_resource`]). On Deep
//! Descent worlds the drill now and then digs one out of a stratum's rock
//! ([`layer_yield`]: about one rock tile in [`LAYER_YIELD_ONE_IN`], decided by a hash of the
//! seed and the tile, so every client agrees) and it pops out as an
//! [`crate::item::ItemKind::Layer`] drop. Layer resources take no cargo slots.
//!
//! The outposts' workshops ([`crate::logistics::LogisticsCommand::Craft`]) turn them into
//! consumables (hull patches, fuel cells, dive charges, blasting charges) and **upgrade parts**:
//! the next tier of a line, fitted for the resources plus [`PART_PRICE_PERCENT`] of its shop
//! price. One recipe per stratum of Descents I-III ([`RECIPES`]), each sold at the outposts of
//! its Descent and every deeper one. Waves 9-10 append the deeper Descents' recipes.
//!
//! **Value curve.** [`layer_resource_worth`] is what one unit is worth in credits (it grows with
//! the Descent value curve and the stratum's place in its Descent): a part recipe saves about
//! its resources' worth over the shop price, and a consumable costs about what its effect
//! would at an outpost.

use crate::{
    depth_progression::descent_value_percent, descent::Stratum, economy::UpgradeKind,
    terrain::TilePosition,
};

/// About one rock tile in this many yields a layer resource when drilled on a Deep Descent
/// world.
pub const LAYER_YIELD_ONE_IN: u64 = 22;

/// Percent of a line's shop price an upgrade part recipe still costs in credits (the layer
/// resources stand in for the rest).
pub const PART_PRICE_PERCENT: u32 = 40;

/// Salt of the layer yield hash.
const YIELD_SALT: u64 = 0x1A7E_5EED_0BAD_C0DE;

/// Whether drilling the rock tile at `position` of a world with `seed` digs out a layer
/// resource (the caller checks that the world has Descents and that the tile was rock).
#[must_use]
pub const fn layer_yield(seed: u64, position: TilePosition) -> bool {
    let mut hash = seed ^ YIELD_SALT;
    hash ^= (position.x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    hash = hash.rotate_left(27);
    hash ^= (position.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    hash ^= hash >> 31;
    hash = hash.wrapping_mul(0x94D0_49BB_1331_11EB);
    hash ^= hash >> 29;
    hash.is_multiple_of(LAYER_YIELD_ONE_IN)
}

/// Credits one unit of `stratum`'s layer resource is worth (the value curve recipes are
/// balanced against).
#[must_use]
pub const fn layer_resource_worth(stratum: Stratum) -> u32 {
    let base = 50 + 12 * stratum.index() as u32;
    base * descent_value_percent(stratum.descent().index() as u8) / 100
}

/// What a recipe makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CraftOutput {
    /// Patches this percent of the hull's capacity on the spot.
    Repair { percent: u32 },
    /// Fills this percent of the tank on the spot.
    Refuel { percent: u32 },
    /// Dive charges into the rack (up to [`crate::logistics::MAX_DIVE_CHARGES`]).
    DiveCharges(u8),
    /// Blasting charges into the bomb rack.
    Bombs(u32),
    /// The next tier of an upgrade line, for [`PART_PRICE_PERCENT`] of its price on top of the
    /// resources (the tier must be on sale and stocked at the outpost).
    Part(UpgradeKind),
}

/// One workshop recipe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub name: &'static str,
    /// What it is, in the workshop's words.
    pub about: &'static str,
    /// Layer resources it takes (one or two kinds).
    pub inputs: &'static [(Stratum, u32)],
    pub output: CraftOutput,
}

impl Recipe {
    /// The Descent whose outposts first craft it (the deepest of its resources' Descents).
    #[must_use]
    pub const fn descent(&self) -> u8 {
        let mut deepest = 0_u8;
        let mut index = 0;
        while index < self.inputs.len() {
            let descent = self.inputs[index].0.descent().index() as u8;
            if descent > deepest {
                deepest = descent;
            }
            index += 1;
        }
        deepest
    }

    /// The stratum this recipe is known for (its first resource's).
    #[must_use]
    pub const fn stratum(&self) -> Stratum {
        self.inputs[0].0
    }

    /// What its resources are worth together ([`layer_resource_worth`]).
    #[must_use]
    pub const fn resource_worth(&self) -> u32 {
        let mut total = 0;
        let mut index = 0;
        while index < self.inputs.len() {
            let (stratum, count) = self.inputs[index];
            total += layer_resource_worth(stratum) * count;
            index += 1;
        }
        total
    }
}

/// Every workshop recipe, in workshop order (a recipe's index is its id in
/// [`crate::logistics::LogisticsCommand::Craft`]; append only).
pub const RECIPES: [Recipe; 17] = [
    // Descent I: The Crust.
    Recipe {
        name: "Root-rope Splice",
        about: "Topsoil roots twisted into hull lashings: patches 30% of the hull",
        inputs: &[(Stratum::Topsoil, 5)],
        output: CraftOutput::Repair { percent: 30 },
    },
    Recipe {
        name: "Kiln-fired Radiator Fins",
        about: "Clay fins fired in the outpost's kiln: the next Radiator tier",
        inputs: &[(Stratum::ClayBeds, 6)],
        output: CraftOutput::Part(UpgradeKind::Radiator),
    },
    Recipe {
        name: "Timber-cased Dive Charges",
        about: "Old mine timber packed around a charge: two dive charges",
        inputs: &[(Stratum::GreyStone, 5)],
        output: CraftOutput::DiveCharges(2),
    },
    Recipe {
        name: "Shale Hull Plates",
        about: "Split slate riveted over the plating: the next Hull tier",
        inputs: &[(Stratum::SlateShelves, 6)],
        output: CraftOutput::Part(UpgradeKind::Hull),
    },
    Recipe {
        name: "Basalt Wool Lagging",
        about: "Spun basalt fibre bound with kiln clay: the next Heat Shielding tier",
        inputs: &[(Stratum::BasaltColumns, 6), (Stratum::ClayBeds, 2)],
        output: CraftOutput::Part(UpgradeKind::HeatShield),
    },
    // Descent II: The Old Deep.
    Recipe {
        name: "Ground Lens Array",
        about: "Fault crystal ground into lenses: the next Headlamp tier",
        inputs: &[(Stratum::CrystalFaults, 6)],
        output: CraftOutput::Part(UpgradeKind::Headlamp),
    },
    Recipe {
        name: "Amber Fuel Cells",
        about: "Fossil resin rendered into fuel: fills half the tank",
        inputs: &[(Stratum::FossilOceans, 5)],
        output: CraftOutput::Refuel { percent: 50 },
    },
    Recipe {
        name: "Pearl Pressure Seals",
        about: "Cathedral pearls ground into gaskets: the next Pressure Rating tier",
        inputs: &[(Stratum::PressureCathedrals, 6)],
        output: CraftOutput::Part(UpgradeKind::PressureRating),
    },
    Recipe {
        name: "Isotope Charges",
        about: "Hot isotopes in lead casings: four blasting charges",
        inputs: &[(Stratum::RadioactiveHollow, 5)],
        output: CraftOutput::Bombs(4),
    },
    Recipe {
        name: "Relic Drill Teeth",
        about: "Builder relics set with crystal: the next Drill tier",
        inputs: &[
            (Stratum::AncientMachineLayer, 6),
            (Stratum::CrystalFaults, 2),
        ],
        output: CraftOutput::Part(UpgradeKind::Drill),
    },
    Recipe {
        name: "Void-glass Tank Liner",
        about: "Geode glass that holds more than its size: the next Fuel Tank tier",
        inputs: &[(Stratum::VoidGeodeFields, 6)],
        output: CraftOutput::Part(UpgradeKind::FuelTank),
    },
    Recipe {
        name: "Mantle Engine Core",
        about: "A mantle shard lagged in basalt wool: the next Engine tier",
        inputs: &[(Stratum::MantleStormZone, 6), (Stratum::BasaltColumns, 2)],
        output: CraftOutput::Part(UpgradeKind::Engine),
    },
    // Descent III: The Living Dark.
    Recipe {
        name: "Spore-silk Bandage",
        about: "Fungal silk that knits into torn plating: patches 60% of the hull",
        inputs: &[(Stratum::FungalForest, 5)],
        output: CraftOutput::Repair { percent: 60 },
    },
    Recipe {
        name: "Heartwood Cargo Racks",
        about: "Root heartwood, light and strong: the next Cargo Bay tier",
        inputs: &[(Stratum::RootLabyrinth, 6)],
        output: CraftOutput::Part(UpgradeKind::CargoBay),
    },
    Recipe {
        name: "Royal Jelly Fuel",
        about: "The friendly hive's jelly burns clean: fills 80% of the tank",
        inputs: &[(Stratum::TheHive, 5)],
        output: CraftOutput::Refuel { percent: 80 },
    },
    Recipe {
        name: "Sinew Hull Weave",
        about: "Living sinew woven over shale plates: the next Hull tier",
        inputs: &[(Stratum::FleshCaverns, 6), (Stratum::SlateShelves, 2)],
        output: CraftOutput::Part(UpgradeKind::Hull),
    },
    Recipe {
        name: "Lure-light Lantern",
        about: "A lure fish's light in a crystal globe: the next Headlamp tier",
        inputs: &[(Stratum::GlowSea, 6), (Stratum::CrystalFaults, 2)],
        output: CraftOutput::Part(UpgradeKind::Headlamp),
    },
];

/// The recipe with id `index`.
#[must_use]
pub fn recipe(index: u8) -> Option<&'static Recipe> {
    RECIPES.get(usize::from(index))
}

/// Whether an outpost in Descent `outpost_descent` crafts `recipe` (its Descent's recipes and
/// every shallower one's).
#[must_use]
pub const fn outpost_crafts(outpost_descent: u8, recipe: &Recipe) -> bool {
    recipe.descent() <= outpost_descent
}

/// The credits a part recipe costs on top of its resources, given the line's shop price.
#[must_use]
pub const fn part_credits(shop_price: u32) -> u32 {
    let credits = shop_price as u64 * PART_PRICE_PERCENT as u64 / 100;
    let rounded = credits.div_ceil(10) * 10;
    if rounded > u32::MAX as u64 {
        u32::MAX
    } else {
        rounded as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stratum_of_descents_one_to_three_has_a_recipe() {
        for stratum in Stratum::ALL.iter().take(17) {
            assert!(
                RECIPES.iter().any(|recipe| recipe.stratum() == *stratum),
                "{stratum:?} has no recipe"
            );
        }
        for recipe in &RECIPES {
            assert!(!recipe.inputs.is_empty() && recipe.inputs.len() <= 2);
            assert!(recipe.inputs.iter().all(|(_, count)| *count > 0));
            assert!(recipe.descent() <= 2, "{}", recipe.name);
            assert!(outpost_crafts(recipe.descent(), recipe));
            if recipe.descent() > 0 {
                assert!(!outpost_crafts(recipe.descent() - 1, recipe));
            }
        }
    }

    #[test]
    fn layer_worth_climbs_with_depth() {
        let mut last = 0;
        for stratum in Stratum::ALL {
            let worth = layer_resource_worth(stratum);
            assert!(worth > last, "{stratum:?}: {worth} after {last}");
            last = worth;
        }
    }

    #[test]
    fn the_yield_is_rare_and_spread_out() {
        let mut hits = 0;
        for y in 0..200 {
            for x in -100..100 {
                if layer_yield(7, TilePosition { x, y }) {
                    hits += 1;
                }
            }
        }
        let expected = 40_000 / LAYER_YIELD_ONE_IN;
        assert!(
            hits > expected * 8 / 10 && hits < expected * 12 / 10,
            "{hits} hits, about {expected} expected"
        );
        // A different seed digs elsewhere.
        let other = (0..400)
            .filter(|x| {
                layer_yield(8, TilePosition { x: *x, y: 3 })
                    != layer_yield(7, TilePosition { x: *x, y: 3 })
            })
            .count();
        assert!(other > 10);
    }
}
