//! How a [`Player`] splits across replication sections, and who may see each part.
//!
//! | Part | Section | Seen by |
//! |---|---|---|
//! | [`PlayerRig`]: pose, velocity, fuel, hull | per tick | everyone it is relevant to |
//! | [`RigBuild`]: the upgrade levels drawn on a rig (drill, engine, cargo bay, hull, radiator, scanner, bulkheads, drill mount) and its cosmetics | on-change [`super::SectionKey::RigBuild`] | everyone |
//! | [`PlayerInventory`]: cargo bay, block hopper, credits, other upgrades, kits, debt, insurance, POI journal, quest log, cosmetic unlocks, village journal, pilot gear and personal pack, milestones | on-change [`super::SectionKey::Loadout`] | the owner only |
//!
//! [`split_player`] destructures the whole [`Player`] without `..`, so a new player field fails
//! to compile until it is assigned to one of the parts.

use serde::{Deserialize, Serialize};

use crate::{
    cosmetics::{CosmeticUnlocks, RigCosmetics},
    item::ItemContainer,
    milestone::MilestoneLog,
    player::Player,
    poi::PoiJournal,
    protection::HazardGear,
    quest::QuestLog,
    village::VillageJournal,
    weapon::{MountedWeapon, RigWeaponState, WeaponRack},
};

/// The per-tick parts of a [`Player`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerRig {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    /// See [`Player::x_remainder`].
    #[serde(default)]
    pub x_remainder: f32,
    /// See [`Player::attitude`].
    #[serde(default)]
    pub attitude: crate::attitude::RigAttitude,
    /// See [`Player::cart`].
    #[serde(default)]
    pub cart: Option<crate::crust::CartRide>,
    /// See [`Player::water`].
    #[serde(default)]
    pub water: crate::drowned::RigWater,
    /// See [`Player::flight`].
    #[serde(default)]
    pub flight: crate::flight::RigFlight,
    pub fuel: f32,
    pub hull: f32,
    /// Capacitor energy, weapon cooldown, aim and trigger.
    #[serde(default)]
    pub weapon: RigWeaponState,
}

/// The public on-change parts of a [`Player`]: what other players' rendering shows of a rig
/// (its hull bar maximum, visible upgrades and cosmetics).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RigBuild {
    /// Drill tier (the drill head drawn on the rig).
    pub drill_strength: u8,
    /// Engine tier (the thrusters drawn on the rig).
    pub engine_level: u8,
    /// Cargo bay tier (the cargo pods drawn on the rig).
    pub cargo_bay_level: u8,
    pub hull_level: u8,
    pub radiator_level: u8,
    pub scanner_level: u8,
    pub crafted_bulkheads: u8,
    /// Drill mount tier (the gimbal / crown bore is drawn on the rig, and the drill pose
    /// depends on it).
    #[serde(default)]
    pub drill_mount_level: u8,
    /// The garage paint job and attachments (`None`: the player's default look).
    #[serde(default)]
    pub cosmetics: Option<RigCosmetics>,
    /// The weapon on the hardpoint.
    #[serde(default)]
    pub mounted_weapon: Option<MountedWeapon>,
}

/// The private on-change parts of a [`Player`]: inventory, credits, upgrades and kits.
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the player's independent inventory fields"
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerInventory {
    pub fuel_capacity: f32,
    pub cargo_bay: ItemContainer,
    pub cargo_capacity: u32,
    pub block_hopper: ItemContainer,
    pub block_hopper_capacity: u32,
    /// The "collect blocks" toggle.
    #[serde(default = "crate::world::player_sections::default_true")]
    pub collect_blocks: bool,
    pub fuel_tank_level: u8,
    pub credits: u32,
    /// Hazard counter tiers.
    #[serde(default)]
    pub hazard_gear: HazardGear,
    pub bombs: u32,
    pub loan_debt: u32,
    pub insured: bool,
    pub insurance_tier: u8,
    pub crafted_sorters: u8,
    pub signal_relay_kits: u32,
    pub survey_drone_kits: u32,
    pub cargo_lift_kits: u32,
    pub tunnel_support_kits: u32,
    pub pump_station_kits: u32,
    pub ore_processor_kits: u32,
    pub journal: PoiJournal,
    pub quests: QuestLog,
    /// Cosmetic items earned on top of the starter set.
    #[serde(default)]
    pub cosmetic_unlocks: CosmeticUnlocks,
    /// Villages visited and charted, and an active shrine blessing.
    #[serde(default)]
    pub villages: VillageJournal,
    /// Owned weapons and the capacitor level.
    #[serde(default)]
    pub weapon_rack: WeaponRack,
    /// The pilot's gear (jetpack, suit).
    #[serde(default)]
    pub pilot_gear: crate::pilot::PilotGear,
    /// The pilot's personal pack.
    #[serde(default)]
    pub pilot_pack: ItemContainer,
    /// The pilot's personal arms and ammo.
    #[serde(default)]
    pub pilot_arsenal: crate::pilot::arsenal::PilotArsenal,
    /// Milestones earned and personal records.
    #[serde(default)]
    pub milestones: MilestoneLog,
    /// The lunar record and the colony's one-off upgrades (WP-MOON).
    #[serde(default)]
    pub lunar: crate::moon::LunarProgress,
}

/// Splits a player into its per-tick, public and private parts (exhaustive: every field goes to
/// exactly one).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive destructure, field by field"
)]
pub fn split_player(player: &Player) -> (PlayerRig, RigBuild, PlayerInventory) {
    let Player {
        x,
        y,
        velocity_x,
        velocity_y,
        x_remainder,
        attitude,
        cart,
        water,
        flight,
        fuel,
        fuel_capacity,
        hull,
        cargo_bay,
        cargo_capacity,
        block_hopper,
        block_hopper_capacity,
        collect_blocks,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
        drill_mount_level,
        hazard_gear,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_bulkheads,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
        journal,
        quests,
        cosmetics,
        cosmetic_unlocks,
        villages,
        weapon_rack,
        mounted_weapon,
        weapon,
        pilot_gear,
        pilot_pack,
        pilot_arsenal,
        milestones,
        lunar,
    } = player.clone();
    (
        PlayerRig {
            x,
            y,
            velocity_x,
            velocity_y,
            x_remainder,
            attitude,
            cart,
            water,
            flight,
            fuel,
            hull,
            weapon,
        },
        RigBuild {
            drill_strength,
            engine_level,
            cargo_bay_level,
            hull_level,
            radiator_level,
            scanner_level,
            crafted_bulkheads,
            drill_mount_level,
            cosmetics,
            mounted_weapon,
        },
        PlayerInventory {
            fuel_capacity,
            cargo_bay,
            cargo_capacity,
            block_hopper,
            block_hopper_capacity,
            collect_blocks,
            fuel_tank_level,
            credits,
            hazard_gear,
            bombs,
            loan_debt,
            insured,
            insurance_tier,
            crafted_sorters,
            signal_relay_kits,
            survey_drone_kits,
            cargo_lift_kits,
            tunnel_support_kits,
            pump_station_kits,
            ore_processor_kits,
            journal,
            quests,
            cosmetic_unlocks,
            villages,
            weapon_rack,
            pilot_gear,
            pilot_pack,
            pilot_arsenal,
            milestones,
            lunar,
        },
    )
}

/// Reassembles a player from its three parts.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one exhaustive destructure, field by field"
)]
pub fn join_player(rig: PlayerRig, build: RigBuild, inventory: PlayerInventory) -> Player {
    let PlayerRig {
        x,
        y,
        velocity_x,
        velocity_y,
        x_remainder,
        attitude,
        cart,
        water,
        flight,
        fuel,
        hull,
        weapon,
    } = rig;
    let RigBuild {
        drill_strength,
        engine_level,
        cargo_bay_level,
        hull_level,
        radiator_level,
        scanner_level,
        crafted_bulkheads,
        drill_mount_level,
        cosmetics,
        mounted_weapon,
    } = build;
    let PlayerInventory {
        fuel_capacity,
        cargo_bay,
        cargo_capacity,
        block_hopper,
        block_hopper_capacity,
        collect_blocks,
        fuel_tank_level,
        credits,
        hazard_gear,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
        journal,
        quests,
        cosmetic_unlocks,
        villages,
        weapon_rack,
        pilot_gear,
        pilot_pack,
        pilot_arsenal,
        milestones,
        lunar,
    } = inventory;
    Player {
        x,
        y,
        velocity_x,
        velocity_y,
        x_remainder,
        attitude,
        cart,
        water,
        flight,
        fuel,
        fuel_capacity,
        hull,
        cargo_bay,
        cargo_capacity,
        block_hopper,
        block_hopper_capacity,
        collect_blocks,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
        drill_mount_level,
        hazard_gear,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_bulkheads,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
        journal,
        quests,
        cosmetics,
        cosmetic_unlocks,
        villages,
        weapon_rack,
        mounted_weapon,
        weapon,
        pilot_gear,
        pilot_pack,
        pilot_arsenal,
        milestones,
        lunar,
    }
}

pub const fn default_true() -> bool {
    true
}
