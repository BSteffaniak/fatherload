#![allow(
    clippy::cast_possible_truncation,
    reason = "player world positions intentionally convert to terrain tile coordinates"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    poi::PoiJournal,
    protection::HazardGear,
    quest::QuestLog,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TilePosition},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Player {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub fuel_capacity: f32,
    pub hull: f32,
    pub cargo: BTreeMap<MineralKind, u32>,
    pub artifacts: BTreeMap<ArtifactKind, u32>,
    #[serde(default)]
    pub materials: BTreeMap<StrategicResourceKind, u32>,
    pub cargo_capacity: u32,
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
            fuel: 100.0,
            fuel_capacity: 100.0,
            hull: 100.0,
            cargo: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            materials: BTreeMap::new(),
            cargo_capacity: 12,
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
        }
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
        self.cargo.values().sum::<u32>() + self.artifacts.values().sum::<u32>()
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

        *self.cargo.entry(mineral).or_default() += 1;
        true
    }

    pub fn add_material(&mut self, material: StrategicResourceKind, count: u32) {
        *self.materials.entry(material).or_default() += count;
    }

    pub fn add_artifact(&mut self, artifact: ArtifactKind) -> bool {
        if !self.has_cargo_space() {
            return false;
        }

        *self.artifacts.entry(artifact).or_default() += 1;
        true
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

/// Thrust and handling multiplier of an engine at `level`: +0.28 per tier up to tier 5, then
/// +0.2 per tier (about 4x stock at the top tier).
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

/// Horizontal speed multiplier cap: the top engines lift far more but do not skate faster
/// than this (keeps steps well under a tile per tick at every supported tick rate).
pub const ENGINE_HANDLING_CAP: f32 = 2.4;
/// Climb speed multiplier cap (as [`ENGINE_HANDLING_CAP`], for climbing).
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
