//! How a [`Player`] splits across replication sections, and who may see each part.
//!
//! | Part | Section | Seen by |
//! |---|---|---|
//! | [`PlayerRig`]: pose, velocity, fuel, hull | per tick | everyone it is relevant to |
//! | [`RigBuild`]: the upgrade levels drawn on a rig (hull, radiator, scanner, bulkheads) | on-change [`super::SectionKey::RigBuild`] | everyone |
//! | [`PlayerInventory`]: cargo, credits, other upgrades, kits, debt, insurance | on-change [`super::SectionKey::Loadout`] | the owner only |
//!
//! [`split_player`] destructures the whole [`Player`] without `..`, so a new player field fails
//! to compile until it is assigned to one of the parts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    player::Player,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind},
};

/// The per-tick parts of a [`Player`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerRig {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
}

/// The public on-change parts of a [`Player`]: what other players' rendering shows of a rig
/// (its hull bar maximum and visible upgrades).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RigBuild {
    pub hull_level: u8,
    pub radiator_level: u8,
    pub scanner_level: u8,
    pub crafted_bulkheads: u8,
}

/// The private on-change parts of a [`Player`]: inventory, credits, upgrades and kits.
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the player's independent inventory fields"
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerInventory {
    pub fuel_capacity: f32,
    pub cargo: BTreeMap<MineralKind, u32>,
    pub artifacts: BTreeMap<ArtifactKind, u32>,
    pub materials: BTreeMap<StrategicResourceKind, u32>,
    pub cargo_capacity: u32,
    pub fuel_tank_level: u8,
    pub cargo_bay_level: u8,
    pub credits: u32,
    pub drill_strength: u8,
    pub engine_level: u8,
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
}

/// Splits a player into its per-tick, public and private parts (exhaustive: every field goes to
/// exactly one).
#[must_use]
pub fn split_player(player: &Player) -> (PlayerRig, RigBuild, PlayerInventory) {
    let Player {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        fuel_capacity,
        hull,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
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
    } = player.clone();
    (
        PlayerRig {
            x,
            y,
            velocity_x,
            velocity_y,
            fuel,
            hull,
        },
        RigBuild {
            hull_level,
            radiator_level,
            scanner_level,
            crafted_bulkheads,
        },
        PlayerInventory {
            fuel_capacity,
            cargo,
            artifacts,
            materials,
            cargo_capacity,
            fuel_tank_level,
            cargo_bay_level,
            credits,
            drill_strength,
            engine_level,
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
        },
    )
}

/// Reassembles a player from its three parts.
#[must_use]
pub fn join_player(rig: PlayerRig, build: RigBuild, inventory: PlayerInventory) -> Player {
    let PlayerRig {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        hull,
    } = rig;
    let RigBuild {
        hull_level,
        radiator_level,
        scanner_level,
        crafted_bulkheads,
    } = build;
    let PlayerInventory {
        fuel_capacity,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
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
    } = inventory;
    Player {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        fuel_capacity,
        hull,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
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
    }
}
