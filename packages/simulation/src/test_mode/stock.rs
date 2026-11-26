//! What a fully stocked test-mode rig carries (Wave 14, W14-A): the registry of stocked
//! consumables and their stacks.
//!
//! **Future-proof by construction.** [`StockItem::all`] walks the registries' own `ALL` lists:
//! every carried explosive kind ([`ExplosiveKind::ALL`] with a carried item, plus plain bombs),
//! every rig and pilot ammunition ([`AmmoKind::ALL`], [`AmmoKind::PILOT`]), every deep supply
//! ([`DeepSupply::ALL`]; the Time-Bubble Bomb is the time-bubble charge) and every crafted
//! infrastructure kit ([`RecipeKind::ALL`], matched exhaustively in [`StockItem::kit`]), plus the
//! dive charges and the ship's missile rack. A kind appended to any of those lists is stocked
//! the moment it exists; the world's stock audit (`WorldState::test_stock_gaps`) fails a test
//! for anything left at zero.
//!
//! The gear side (upgrades, rig parts, pilot gear, cosmetics, gate rewards, space lines, alien
//! tech, star charts) has no stack: the world fits all of it (`world/test_stock.rs`).
//!
//! The game has no carried repair kits or fuel cans: repairs and refuels are services (and the
//! outposts' workshop recipes patch and fill on the spot). The invulnerable hull and infinite
//! fuel toggles keep a test rig whole and full instead.

use serde::{Deserialize, Serialize};

use crate::{
    explosive::ExplosiveKind,
    logistics::MAX_DIVE_CHARGES,
    pilot::arsenal::MAX_PILOT_AMMO,
    service::RecipeKind,
    weapon::AmmoKind,
    workshop::{DeepSupply, MAX_SUPPLY_STACK},
};

/// The stack a test rig carries of anything without a stack limit of its own (bombs, special
/// explosives, rockets, infrastructure kits).
pub const TEST_STACK: u32 = 99;

/// The stocking revision this build stocks to (see
/// [`super::TestModeState::stock_revision`]). Raise it when stocking gains something an older
/// test-mode save should pick up on load.
pub const STOCK_REVISION: u8 = 1;

/// One stocked consumable (a registry entry; see the module docs).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StockItem {
    /// Plain bombs ([`crate::Player::bombs`]).
    Bombs,
    /// A special explosive carried in the cargo bay (never [`ExplosiveKind::Bomb`],
    /// [`ExplosiveKind::Bomblet`] or [`ExplosiveKind::Rocket`]).
    Explosive(ExplosiveKind),
    /// Rig ammunition in the cargo bay ([`AmmoKind::ALL`]).
    RigAmmo(AmmoKind),
    /// The pilot's own ammunition ([`AmmoKind::PILOT`]).
    PilotAmmo(AmmoKind),
    /// A deep supply in the kit (the Time-Bubble Bomb is listed as its explosive).
    Supply(DeepSupply),
    /// Dive charges in the rack (Wave 8 lifts and dives).
    DiveCharges,
    /// A crafted infrastructure kit (one of [`StockItem::kit`]'s recipes).
    Kit(RecipeKind),
    /// Missiles in the ship's rack (space combat).
    ShipMissiles,
}

impl StockItem {
    /// Every stocked consumable, walking the registries' `ALL` lists (see the module docs).
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut items = vec![Self::Bombs];
        items.extend(
            ExplosiveKind::ALL
                .into_iter()
                .filter(|kind| kind.item().is_some())
                .map(Self::Explosive),
        );
        items.extend(AmmoKind::ALL.into_iter().map(Self::RigAmmo));
        items.extend(AmmoKind::PILOT.into_iter().map(Self::PilotAmmo));
        items.extend(
            DeepSupply::ALL
                .into_iter()
                .filter(|supply| *supply != DeepSupply::TimeBubbleBomb)
                .map(Self::Supply),
        );
        items.push(Self::DiveCharges);
        items.extend(
            RecipeKind::ALL
                .into_iter()
                .filter(|recipe| Self::kit(*recipe))
                .map(Self::Kit),
        );
        items.push(Self::ShipMissiles);
        items
    }

    /// Whether `recipe` makes a carried kit (a count on the rig) rather than a one-off rig
    /// modification. Exhaustive, so a new recipe must say which it is.
    #[must_use]
    pub const fn kit(recipe: RecipeKind) -> bool {
        match recipe {
            RecipeKind::SignalRelayKit
            | RecipeKind::SurveyDroneKit
            | RecipeKind::CargoLiftKit
            | RecipeKind::TunnelSupportKit
            | RecipeKind::PumpStationKit
            | RecipeKind::OreProcessorKit => true,
            RecipeKind::ReinforcedBulkhead
            | RecipeKind::AuxiliaryTank
            | RecipeKind::ExpandedSorter => false,
        }
    }

    /// The stack a fully stocked rig carries. The ship's rack depends on its fitted gear, so
    /// [`Self::ShipMissiles`] answers the most a rack holds; the world fills the real rack.
    #[must_use]
    pub const fn stack(self) -> u32 {
        match self {
            // The Time-Bubble Bomb is also a deep supply: its kit's stack.
            Self::Explosive(ExplosiveKind::TimeBubble) | Self::Supply(_) => MAX_SUPPLY_STACK as u32,
            Self::DiveCharges => MAX_DIVE_CHARGES as u32,
            Self::PilotAmmo(_) => MAX_PILOT_AMMO,
            Self::ShipMissiles => 8,
            Self::Bombs | Self::Explosive(_) | Self::RigAmmo(_) | Self::Kit(_) => TEST_STACK,
        }
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bombs => "Bombs",
            Self::Explosive(kind) => kind.name(),
            Self::RigAmmo(ammo) | Self::PilotAmmo(ammo) => ammo.name(),
            Self::Supply(supply) => supply.name(),
            Self::DiveCharges => "Dive Charges",
            Self::Kit(recipe) => recipe.name(),
            Self::ShipMissiles => "Ship Missiles",
        }
    }

    /// A stable lower-case id (`"bombs"`, `"sticky_bomb"`, `"ballast_tank"`): the command
    /// palette's word for it (W14-D). Unique across [`Self::all`].
    #[must_use]
    pub fn slug(self) -> String {
        let base = match self {
            Self::PilotAmmo(ammo) => format!("pilot {}", ammo.name()),
            _ => self.name().to_owned(),
        };
        base.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .split('_')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("_")
    }

    /// The item named `slug` (see [`Self::slug`]).
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::all().into_iter().find(|item| item.slug() == slug)
    }
}

/// What the god-mode toolkit did (netcode codes `1120 + code`, the stock track's range
/// 1120-1139).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TestStockEvent {
    /// A rig was stocked with everything (`StockAll`, or a world brought up to date).
    Stocked,
    /// A rig's consumables were refilled (`Restock`).
    Restocked,
    /// A creature was spawned beside a rig.
    CreatureSpawned,
    /// Creatures near a rig were removed.
    EnemiesCleared,
    /// A Descent gate or the ending was set.
    ProgressSet,
    /// The story and codex were unlocked or reset.
    StorySet,
    /// The home clock or the weather was set.
    SkySet,
}

impl TestStockEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 7] = [
        Self::Stocked,
        Self::Restocked,
        Self::CreatureSpawned,
        Self::EnemiesCleared,
        Self::ProgressSet,
        Self::StorySet,
        Self::SkySet,
    ];

    /// Wire code (netcode adds 1120).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_walks_every_carried_kind_and_slugs_are_unique() {
        let all = StockItem::all();
        for kind in ExplosiveKind::SELECTABLE {
            let wanted = if kind == ExplosiveKind::Bomb {
                StockItem::Bombs
            } else {
                StockItem::Explosive(kind)
            };
            assert!(all.contains(&wanted), "{kind:?} is not stocked");
        }
        for supply in DeepSupply::ALL {
            if supply != DeepSupply::TimeBubbleBomb {
                assert!(all.contains(&StockItem::Supply(supply)));
            }
        }
        for ammo in AmmoKind::ALL {
            assert!(all.contains(&StockItem::RigAmmo(ammo)));
        }
        for ammo in AmmoKind::PILOT {
            assert!(all.contains(&StockItem::PilotAmmo(ammo)));
        }
        let slugs: std::collections::BTreeSet<String> =
            all.iter().map(|item| item.slug()).collect();
        assert_eq!(slugs.len(), all.len(), "{slugs:?}");
        for item in &all {
            assert_eq!(StockItem::from_slug(&item.slug()), Some(*item));
            assert!(item.stack() > 0);
        }
        assert_eq!(StockItem::Bombs.slug(), "bombs");
        assert_eq!(
            StockItem::Supply(DeepSupply::BallastTank).slug(),
            "ballast_tank"
        );
    }
}
