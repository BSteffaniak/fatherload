#![allow(
    clippy::cast_possible_truncation,
    reason = "player world positions intentionally convert to terrain tile coordinates"
)]

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::{CosmeticItem, CosmeticUnlocks, RigCosmetics},
    explosive::ExplosiveKind,
    item::{ItemContainer, ItemKind, Stowage},
    poi::PoiJournal,
    protection::HazardGear,
    quest::QuestLog,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TilePosition},
    weapon::{AmmoKind, MountedWeapon, RigWeaponState, WeaponKind, WeaponRack},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    /// The part of the rig's exact horizontal position (world pixels) that `x` cannot hold:
    /// `x + x_remainder` is where movement put the rig. Zero near spawn, where `f32` resolves
    /// the simulation's sub-pixel grid; far out in an endless world it keeps travel exact
    /// (see `integrate_x` in the world's movement).
    #[serde(default)]
    pub x_remainder: f32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub hull: f32,
    /// The cargo bay: minerals, artifacts, strategic materials and any other bay-stowed items
    /// ([`crate::item`]). Its capacity is [`Self::cargo_capacity`] slots.
    #[serde(default)]
    pub cargo_bay: ItemContainer,
    pub cargo_capacity: u32,
    /// Building blocks, kept apart from the cargo bay so plain blocks never crowd out ore.
    #[serde(default)]
    pub block_hopper: ItemContainer,
    /// Block hopper capacity, in blocks (grows with the cargo bay tier, see
    /// [`crate::world::drops::block_hopper_capacity_for_level`]).
    #[serde(default = "default_block_hopper_capacity")]
    pub block_hopper_capacity: u32,
    /// The "collect blocks" toggle: off, the magnet leaves plain block drops alone
    /// ([`crate::PlayerCommand::SetCollectBlocks`]).
    #[serde(default = "default_collect_blocks")]
    pub collect_blocks: bool,
    pub fuel_tank_level: u8,
    pub cargo_bay_level: u8,
    pub credits: u32,
    pub drill_strength: u8,
    pub engine_level: u8,
    pub hull_level: u8,
    pub radiator_level: u8,
    #[serde(default)]
    pub scanner_level: u8,
    /// Drill mount tier: 0 stock (drills only while resting on the ground), 1 gimbal mount
    /// (sideways drilling in flight), 2 crown bore (also drills upward into ceilings).
    #[serde(default)]
    pub drill_mount_level: u8,
    /// Hazard counter tiers (heat shielding, lava plating, gas scrubber, pressure bracing,
    /// boulder deflector).
    #[serde(default)]
    pub hazard_gear: HazardGear,
    /// Plain bombs carried. Special explosives are [`ItemKind::Explosive`] items in the cargo
    /// bay ([`ExplosiveKind::item`]; see [`Self::explosive_count`]).
    #[serde(default)]
    pub bombs: u32,
    #[serde(default)]
    pub loan_debt: u32,
    #[serde(default)]
    pub insured: bool,
    #[serde(default)]
    pub insurance_tier: u8,
    #[serde(default)]
    pub crafted_bulkheads: u8,
    #[serde(default)]
    pub crafted_sorters: u8,
    #[serde(default)]
    pub signal_relay_kits: u32,
    #[serde(default)]
    pub survey_drone_kits: u32,
    #[serde(default)]
    pub cargo_lift_kits: u32,
    #[serde(default)]
    pub tunnel_support_kits: u32,
    #[serde(default)]
    pub pump_station_kits: u32,
    #[serde(default)]
    pub ore_processor_kits: u32,
    /// Points of interest this player knows about and the logs they recovered.
    #[serde(default)]
    pub journal: PoiJournal,
    /// Side quests: active solo quests, history, reputation and blueprint fragments.
    #[serde(default)]
    pub quests: QuestLog,
    /// The paint job and attachments chosen at the garage (`None`: the player's default look,
    /// see [`RigCosmetics::default_for`]). Public: everyone sees a rig's look.
    #[serde(default)]
    pub cosmetics: Option<RigCosmetics>,
    /// Cosmetic items earned on top of the starter set (private).
    #[serde(default)]
    pub cosmetic_unlocks: CosmeticUnlocks,
    /// Villages visited and charted, and an active shrine blessing (private).
    #[serde(default)]
    pub villages: crate::village::VillageJournal,
    /// Owned hardpoint weapons and the capacitor (private; see [`crate::weapon`]).
    #[serde(default)]
    pub weapon_rack: WeaponRack,
    /// The weapon on the hardpoint (public: drawn on the rig).
    #[serde(default)]
    pub mounted_weapon: Option<MountedWeapon>,
    /// Capacitor energy, cooldown and aim (per tick).
    #[serde(default)]
    pub weapon: RigWeaponState,
}

#[allow(
    clippy::missing_const_for_fn,
    reason = "BTreeMap cannot be constructed in const fn"
)]
impl Player {
    #[must_use]
    pub fn new(spawn_x: f32, spawn_y: f32) -> Self {
        Self {
            x: spawn_x,
            y: spawn_y,
            velocity_x: 0.0,
            velocity_y: 0.0,
            x_remainder: 0.0,
            fuel: 100.0,
            fuel_capacity: 100.0,
            hull: 100.0,
            cargo_bay: ItemContainer::new(),
            cargo_capacity: 12,
            block_hopper: ItemContainer::new(),
            block_hopper_capacity: BLOCK_HOPPER_BASE_CAPACITY,
            collect_blocks: true,
            fuel_tank_level: 1,
            cargo_bay_level: 1,
            credits: 0,
            drill_strength: 1,
            engine_level: 1,
            hull_level: 1,
            radiator_level: 1,
            scanner_level: 0,
            drill_mount_level: 0,
            hazard_gear: HazardGear::default(),
            bombs: 0,
            loan_debt: 0,
            insured: false,
            insurance_tier: 0,
            crafted_bulkheads: 0,
            crafted_sorters: 0,
            signal_relay_kits: 0,
            survey_drone_kits: 0,
            cargo_lift_kits: 0,
            tunnel_support_kits: 0,
            pump_station_kits: 0,
            ore_processor_kits: 0,
            journal: PoiJournal::default(),
            quests: QuestLog::default(),
            cosmetics: None,
            cosmetic_unlocks: CosmeticUnlocks::default(),
            villages: crate::village::VillageJournal::default(),
            weapon_rack: WeaponRack::default(),
            mounted_weapon: None,
            weapon: RigWeaponState {
                energy: crate::weapon::battery_capacity(0),
                ..RigWeaponState::default()
            },
        }
    }

    /// Adds `weapon` to the rack at Mk I (a purchase, quest reward or picked-up weapon item)
    /// and mounts it if the hardpoint is empty. Returns `false` if it was already owned.
    pub fn receive_weapon(&mut self, weapon: WeaponKind) -> bool {
        if self.weapon_rack.owned.contains_key(&weapon) {
            return false;
        }
        self.weapon_rack.owned.insert(weapon, 0);
        if self.mounted_weapon.is_none() {
            self.mounted_weapon = Some(MountedWeapon {
                kind: weapon,
                mk: 0,
            });
        }
        true
    }

    /// Mounts an owned weapon (`None` holsters it). Returns whether the hardpoint changed.
    pub fn mount_weapon(&mut self, weapon: Option<WeaponKind>) -> bool {
        let next = match weapon {
            None => None,
            Some(kind) => match self.weapon_rack.mk(kind) {
                Some(mk) => Some(MountedWeapon { kind, mk }),
                None => return false,
            },
        };
        let changed = next != self.mounted_weapon;
        self.mounted_weapon = next;
        changed
    }

    /// Capacitor capacity at the rack's battery level.
    #[must_use]
    pub fn battery_capacity(&self) -> f32 {
        crate::weapon::battery_capacity(self.weapon_rack.battery_level)
    }

    /// Rounds of `ammo` carried.
    #[must_use]
    pub fn ammo(&self, ammo: AmmoKind) -> u32 {
        self.cargo_bay.count(ammo.item())
    }

    /// Unlocks a cosmetic item for this player (quest rewards, milestones, shops). Returns
    /// `true` if it was newly unlocked. Call it only from inside the fixed tick.
    pub fn unlock_cosmetic(&mut self, item: CosmeticItem) -> bool {
        self.cosmetic_unlocks.unlock(item)
    }

    #[must_use]
    pub fn tile_position(&self, tile_size: f32) -> TilePosition {
        TilePosition {
            x: (self.x / tile_size).floor() as i32,
            y: (self.y / tile_size).floor() as i32,
        }
    }

    /// Whether the drill mount lets the rig drill sideways while airborne.
    #[must_use]
    pub const fn drills_sideways_in_flight(&self) -> bool {
        self.drill_mount_level >= 1
    }

    /// Whether the drill mount lets the rig drill upward into a ceiling.
    #[must_use]
    pub const fn drills_upward(&self) -> bool {
        self.drill_mount_level >= 2
    }

    #[must_use]
    pub fn cargo_used(&self) -> u32 {
        self.cargo_bay.slots_used()
    }

    #[must_use]
    pub fn has_cargo_space(&self) -> bool {
        self.cargo_used() < self.cargo_capacity
    }

    #[must_use]
    pub fn max_hull(&self) -> f32 {
        f32::from(self.crafted_bulkheads).mul_add(15.0, hull_capacity_for_level(self.hull_level))
    }

    pub fn add_cargo(&mut self, mineral: MineralKind) -> bool {
        if !self.has_cargo_space() {
            return false;
        }

        self.cargo_bay.add(mineral, 1);
        true
    }

    /// Charges of `kind` carried (plain bombs come from [`Self::bombs`], the rest are explosive
    /// items in the cargo bay).
    #[must_use]
    pub fn explosive_count(&self, kind: ExplosiveKind) -> u32 {
        if kind == ExplosiveKind::Bomb {
            return self.bombs;
        }
        kind.item().map_or(0, |item| self.cargo_bay.count(item))
    }

    /// Adds `count` charges of `kind` (plain bombs go to [`Self::bombs`], the rest into the
    /// cargo bay as explosive items, which take no slots).
    pub fn add_explosives(&mut self, kind: ExplosiveKind, count: u32) {
        if kind == ExplosiveKind::Bomb {
            self.bombs = self.bombs.saturating_add(count);
        } else if let Some(item) = kind.item() {
            self.cargo_bay.add(item, count);
        }
    }

    /// Takes one charge of `kind`, returning whether there was one.
    pub fn take_explosive(&mut self, kind: ExplosiveKind) -> bool {
        if kind == ExplosiveKind::Bomb {
            if self.bombs == 0 {
                return false;
            }
            self.bombs -= 1;
            return true;
        }
        kind.item()
            .is_some_and(|item| self.cargo_bay.remove(item, 1) == 1)
    }

    /// Every explosive kind carried with its count, in selection order (plain bombs first,
    /// when any): what the inventory screen lists.
    #[must_use]
    pub fn carried_explosives(&self) -> Vec<(ExplosiveKind, u32)> {
        ExplosiveKind::SELECTABLE
            .into_iter()
            .map(|kind| (kind, self.explosive_count(kind)))
            .filter(|(_, count)| *count > 0)
            .collect()
    }

    pub fn add_material(&mut self, material: StrategicResourceKind, count: u32) {
        self.cargo_bay.add(material, count);
    }

    pub fn add_artifact(&mut self, artifact: ArtifactKind) -> bool {
        if !self.has_cargo_space() {
            return false;
        }

        self.cargo_bay.add(artifact, 1);
        true
    }

    /// The container `kind` is stowed in.
    #[must_use]
    pub const fn container(&self, kind: ItemKind) -> &ItemContainer {
        match kind.stowage() {
            Stowage::CargoBay => &self.cargo_bay,
            Stowage::BlockHopper => &self.block_hopper,
        }
    }

    /// Capacity, in slots, of the container `kind` is stowed in.
    #[must_use]
    pub const fn container_capacity(&self, kind: ItemKind) -> u32 {
        match kind.stowage() {
            Stowage::CargoBay => self.cargo_capacity,
            Stowage::BlockHopper => self.block_hopper_capacity,
        }
    }

    /// How many of `count` units of `kind` would fit right now.
    #[must_use]
    pub fn room_for(&self, kind: ItemKind, count: u32) -> u32 {
        self.container(kind)
            .accepts(kind, count, self.container_capacity(kind))
    }

    /// Stows as many of `count` units of `kind` as fit in its container (cargo bay or block
    /// hopper) and returns how many were taken; the rest stays wherever it came from (a drop
    /// left floating when the bay is full).
    pub fn stow(&mut self, kind: ItemKind, count: u32) -> u32 {
        let accepted = self.room_for(kind, count);
        match kind.stowage() {
            Stowage::CargoBay => self.cargo_bay.add(kind, accepted),
            Stowage::BlockHopper => self.block_hopper.add(kind, accepted),
        }
        accepted
    }

    /// Units of `kind` carried (in whichever container it is stowed in).
    #[must_use]
    pub fn carried(&self, kind: ItemKind) -> u32 {
        self.container(kind).count(kind)
    }
}

/// Block hopper capacity of a stock rig, in blocks.
pub const BLOCK_HOPPER_BASE_CAPACITY: u32 = 60;

const fn default_block_hopper_capacity() -> u32 {
    BLOCK_HOPPER_BASE_CAPACITY
}

const fn default_collect_blocks() -> bool {
    true
}

impl Player {
    /// Lifts fuel tank and cargo bay capacities stored under the old five-tier curves (+50 fuel
    /// and +8 slots a tier) onto the current curves for the fitted tiers, keeping any bonus
    /// earned on top of the old curve. Rigs already on the current curves are untouched, so this
    /// is safe to run on every load.
    pub fn catch_up_capacity_curves(&mut self) {
        self.block_hopper_capacity =
            self.block_hopper_capacity
                .max(crate::world::drops::block_hopper_capacity_for_level(
                    self.cargo_bay_level,
                ));
        let cargo = cargo_capacity_for_level(self.cargo_bay_level);
        if self.cargo_capacity < cargo {
            let legacy = 12 + 8 * tiers_past_first(self.cargo_bay_level);
            self.cargo_capacity = cargo + self.cargo_capacity.saturating_sub(legacy);
        }
        let fuel = fuel_capacity_for_level(self.fuel_tank_level);
        if self.fuel_capacity < fuel {
            #[allow(clippy::cast_precision_loss, reason = "a handful of tiers")]
            let legacy = (tiers_past_first(self.fuel_tank_level) as f32).mul_add(50.0, 100.0);
            self.fuel_capacity = fuel + (self.fuel_capacity - legacy).max(0.0);
        }
    }
}

/// Upgrade tiers bought past the first (tier 1 is the stock part).
const fn tiers_past_first(level: u8) -> u32 {
    level.saturating_sub(1) as u32
}

/// Cargo slots of a cargo bay at `level`: 12 stock, each tier adding two more slots than the
/// last (+8, +10, +12, ...), about 300 at the top tier.
#[must_use]
pub const fn cargo_capacity_for_level(level: u8) -> u32 {
    let n = tiers_past_first(level);
    12 + 8 * n + n * n.saturating_sub(1)
}

/// Fuel of a fuel tank at `level`: 100 stock, each tier adding 25 more than the last (+50,
/// +75, +100, ...), about 3,000 at the top tier.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "tank sizes are a few thousand units, exact in f32"
)]
pub const fn fuel_capacity_for_level(level: u8) -> f32 {
    let n = tiers_past_first(level);
    (100 + 50 * n + 25 * n * n.saturating_sub(1) / 2) as f32
}

/// Hull integrity of hull plating at `level` (before crafted bulkheads): 100 stock, each tier
/// adding 20 more than the last (+40, +60, +80, ...), about 2,500 at the top tier.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "hull sizes are a few thousand units, exact in f32"
)]
pub const fn hull_capacity_for_level(level: u8) -> f32 {
    let n = tiers_past_first(level);
    (100 + 40 * n + 10 * n * n.saturating_sub(1)) as f32
}

/// Thrust multiplier of an engine at `level`: +0.28 per tier up to tier 5, then +0.2 per tier
/// (about 4x stock at the top tier).
///
/// Horizontal top speed has its own, steeper curve (`crate::world::engine_top_speed_tiles`,
/// about 5x stock at the top tier).
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a handful of tiers")]
pub const fn engine_power(level: u8) -> f32 {
    let n = tiers_past_first(level);
    if n <= 4 {
        (n as f32).mul_add(0.28, 1.0)
    } else {
        ((n - 4) as f32).mul_add(0.2, 2.12)
    }
}

/// Former horizontal speed multiplier cap, kept for API compatibility.
///
/// Horizontal speed now follows the eased drive model (`crate::world::DriveTuning`) and fast
/// rigs are swept in collision sub-steps, so gameplay no longer reads this; a test checks that
/// no tier of the new top-speed table is slower than this old capped curve.
pub const ENGINE_HANDLING_CAP: f32 = 2.4;
/// Climb speed multiplier cap: the top engines lift far more but do not climb faster than this.
pub const ENGINE_CLIMB_CAP: f32 = 3.0;

/// Fuel burned per second of thrust relative to stock, for a fuel tank at `level` (better
/// injectors come with every tank tier, never below 40 %).
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a handful of tiers")]
pub const fn fuel_burn_efficiency(level: u8) -> f32 {
    (tiers_past_first(level) as f32)
        .mul_add(-0.05, 1.0)
        .max(0.4)
}

/// Chip-rate multiplier of a drill at `level`: +0.4 per tier up to tier 5, then +0.5 per tier.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a handful of tiers")]
pub const fn drill_speed(level: u8) -> f32 {
    let n = tiers_past_first(level);
    if n <= 4 {
        (n as f32).mul_add(0.4, 1.0)
    } else {
        ((n - 4) as f32).mul_add(0.5, 2.6)
    }
}

/// Durability a drill at `level` takes off a tile per chip: 1 up to tier 5, then one more every
/// three tiers (5 at the top tier, so hard rock falls in three chips).
#[must_use]
pub const fn drill_bite(level: u8) -> u8 {
    if level < 6 { 1 } else { 2 + (level - 6) / 3 }
}
