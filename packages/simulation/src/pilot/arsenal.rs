//! The pilot's personal arms (expansion plan WP-PILOT): what a pilot owns, carries and swings.
//!
//! Pure data. A player's [`PilotArsenal`] (private, on [`crate::Player::pilot_arsenal`]) holds
//! the personal weapons bought at a gunsmith (the town's Explosives Shack and village armories),
//! the personal ammunition they fire (separate from the rig's capacitor and rocket ammo) and
//! the weapon in the pilot's hands. Nothing equipped means the hand pick (which also swings at
//! creatures, [`WeaponKind::HandPick`]).
//!
//! The arms are [`WeaponKind`] variants ([`WeaponKind::is_personal`]) so their shots reuse the
//! projectile entities, hits, kill credit and damage model of the rig's weapons; they are never
//! mounted on a hardpoint. Firing lives in `world/pilot_arms.rs`.
//!
//! Throwables are the explosive belt the rig already carries (bombs, sticky charges, flares,
//! smoke, ...): a pilot throws them by hand (`WorldState::pilot_throw`), and gunsmiths sell
//! packs of them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::weapon::{AmmoKind, WeaponKind};

/// The personal weapons, in gunsmith (and quick-swap) order.
pub const PILOT_WEAPONS: [WeaponKind; 4] = [
    WeaponKind::Wrench,
    WeaponKind::Sidearm,
    WeaponKind::Shotgun,
    WeaponKind::Rifle,
];

/// Most of one ammunition kind a pilot can carry.
pub const MAX_PILOT_AMMO: u32 = 240;

/// What a pilot owns and carries (private loadout).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PilotArsenal {
    /// Personal weapons owned.
    #[serde(default)]
    pub owned: BTreeSet<WeaponKind>,
    /// Personal ammunition carried ([`AmmoKind::PILOT`]).
    #[serde(default)]
    pub ammo: BTreeMap<AmmoKind, u32>,
    /// The weapon in the pilot's hands (`None`: the hand pick).
    #[serde(default)]
    pub equipped: Option<WeaponKind>,
}

impl PilotArsenal {
    /// Rounds of `ammo` carried.
    #[must_use]
    pub fn ammo(&self, ammo: AmmoKind) -> u32 {
        self.ammo.get(&ammo).copied().unwrap_or(0)
    }

    /// Adds `count` rounds (capped at [`MAX_PILOT_AMMO`]); returns how many fit.
    pub fn add_ammo(&mut self, ammo: AmmoKind, count: u32) -> u32 {
        let have = self.ammo(ammo);
        let added = count.min(MAX_PILOT_AMMO.saturating_sub(have));
        if added > 0 {
            self.ammo.insert(ammo, have + added);
        }
        added
    }

    /// Spends one round of `ammo`; returns whether there was one.
    pub fn spend(&mut self, ammo: AmmoKind) -> bool {
        let have = self.ammo(ammo);
        if have == 0 {
            return false;
        }
        if have == 1 {
            self.ammo.remove(&ammo);
        } else {
            self.ammo.insert(ammo, have - 1);
        }
        true
    }

    /// Adds `weapon` (a personal arm) and puts it in the pilot's hands if they were empty.
    /// Returns `false` if it was already owned or is not a personal arm.
    pub fn receive(&mut self, weapon: WeaponKind) -> bool {
        if !PILOT_WEAPONS.contains(&weapon) || !self.owned.insert(weapon) {
            return false;
        }
        if self.equipped.is_none() {
            self.equipped = Some(weapon);
        }
        true
    }

    /// The next thing to hold after the current one, in [`PILOT_WEAPONS`] order with the hand
    /// pick (`None`) between the last and the first. `None` when nothing else is owned.
    #[must_use]
    pub fn next_after(&self, current: Option<WeaponKind>) -> Option<Option<WeaponKind>> {
        if self.owned.is_empty() {
            return None;
        }
        // The cycle: pick, then every owned weapon in order.
        let cycle: Vec<Option<WeaponKind>> = std::iter::once(None)
            .chain(
                PILOT_WEAPONS
                    .iter()
                    .copied()
                    .filter(|kind| self.owned.contains(kind))
                    .map(Some),
            )
            .collect();
        let index = cycle.iter().position(|held| *held == current).unwrap_or(0);
        Some(cycle[(index + 1) % cycle.len()])
    }
}

/// Price of a personal weapon at a gunsmith.
#[must_use]
pub const fn weapon_price(weapon: WeaponKind) -> u32 {
    weapon.cost()
}

/// Whether `weapon` swings rather than shoots.
#[must_use]
pub const fn is_melee(weapon: WeaponKind) -> bool {
    matches!(weapon, WeaponKind::Wrench | WeaponKind::HandPick)
}

/// The gunsmith's purchase of personal arms ([`crate::SessionServiceRequest::PilotArms`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PilotArmsPurchase {
    /// A personal weapon ([`PILOT_WEAPONS`]).
    Weapon(WeaponKind),
    /// One pack of personal ammunition ([`AmmoKind::pack`]).
    Ammo(AmmoKind),
}

impl PilotArmsPurchase {
    /// The town gunsmith's list, in rack order.
    pub const TOWN: [Self; 6] = [
        Self::Weapon(WeaponKind::Wrench),
        Self::Weapon(WeaponKind::Sidearm),
        Self::Weapon(WeaponKind::Shotgun),
        Self::Weapon(WeaponKind::Rifle),
        Self::Ammo(AmmoKind::Cartridge),
        Self::Ammo(AmmoKind::Shell),
    ];

    /// The price of this purchase.
    #[must_use]
    pub const fn price(self) -> u32 {
        match self {
            Self::Weapon(weapon) => weapon_price(weapon),
            Self::Ammo(ammo) => ammo.pack().1,
        }
    }

    /// A short label for counters.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Weapon(weapon) => weapon.name(),
            Self::Ammo(ammo) => ammo.name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_swap_cycles_the_pick_and_every_owned_weapon() {
        let mut arsenal = PilotArsenal::default();
        assert_eq!(arsenal.next_after(None), None, "nothing to swap to");
        assert!(arsenal.receive(WeaponKind::Rifle));
        assert_eq!(
            arsenal.equipped,
            Some(WeaponKind::Rifle),
            "first gun in hand"
        );
        assert!(!arsenal.receive(WeaponKind::Rifle));
        assert!(
            !arsenal.receive(WeaponKind::Railgun),
            "rig weapons are not arms"
        );
        assert!(arsenal.receive(WeaponKind::Wrench));
        assert_eq!(arsenal.next_after(None), Some(Some(WeaponKind::Wrench)));
        assert_eq!(
            arsenal.next_after(Some(WeaponKind::Wrench)),
            Some(Some(WeaponKind::Rifle))
        );
        assert_eq!(arsenal.next_after(Some(WeaponKind::Rifle)), Some(None));
    }

    #[test]
    fn ammo_is_capped_and_spent_one_at_a_time() {
        let mut arsenal = PilotArsenal::default();
        assert!(!arsenal.spend(AmmoKind::Shell));
        assert_eq!(arsenal.add_ammo(AmmoKind::Shell, 300), MAX_PILOT_AMMO);
        assert_eq!(arsenal.add_ammo(AmmoKind::Shell, 5), 0);
        assert!(arsenal.spend(AmmoKind::Shell));
        assert_eq!(arsenal.ammo(AmmoKind::Shell), MAX_PILOT_AMMO - 1);
        for weapon in PILOT_WEAPONS {
            assert!(weapon.is_personal());
            assert!(!WeaponKind::ALL.contains(&weapon));
            let rig = WeaponKind::RivetGun.stats(0);
            let dps = |stats: crate::weapon::WeaponStats| {
                stats.damage * f32::from(stats.pellets.max(1)) * 1_000.0
                    / f32::from(u16::try_from(stats.cooldown_millis).unwrap_or(u16::MAX))
            };
            if !is_melee(weapon) {
                assert!(
                    dps(weapon.stats(0)) < dps(rig),
                    "{weapon:?} out-damages the starter rig gun"
                );
            }
        }
    }
}
