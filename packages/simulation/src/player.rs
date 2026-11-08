#![allow(
    clippy::cast_possible_truncation,
    reason = "player world positions intentionally convert to terrain tile coordinates"
)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TilePosition};

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
        }
    }

    #[must_use]
    pub fn tile_position(&self, tile_size: f32) -> TilePosition {
        TilePosition {
            x: (self.x / tile_size).floor() as i32,
            y: (self.y / tile_size).floor() as i32,
        }
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
        f32::from(self.crafted_bulkheads).mul_add(
            15.0,
            100.0 + f32::from(self.hull_level.saturating_sub(1)) * 35.0,
        )
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
