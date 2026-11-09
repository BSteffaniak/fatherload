#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "service pricing intentionally converts small fuel/hull unit counts"
)]

use serde::{Deserialize, Serialize};

use crate::{
    player::Player,
    terrain::DeepStratum,
    worldgen::{Band, WorldSettings, strata::band_reached},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum TownBuilding {
    Depot,
    Mechanic,
    Bank,
    ScannerLab,
    SalvageYard,
    ExplosivesShack,
}

impl TownBuilding {
    pub const ALL: [Self; 6] = [
        Self::Depot,
        Self::Mechanic,
        Self::Bank,
        Self::ScannerLab,
        Self::SalvageYard,
        Self::ExplosivesShack,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Depot => "Depot Expansion",
            Self::Mechanic => "Mechanic Garage",
            Self::Bank => "Bank Office",
            Self::ScannerLab => "Scanner Lab",
            Self::SalvageYard => "Salvage Yard",
            Self::ExplosivesShack => "Explosives Shack",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TownDevelopment {
    #[serde(default)]
    pub depot_level: u8,
    #[serde(default)]
    pub mechanic_level: u8,
    #[serde(default)]
    pub bank_level: u8,
    #[serde(default)]
    pub scanner_lab_level: u8,
    #[serde(default)]
    pub salvage_yard_level: u8,
    #[serde(default)]
    pub explosives_shack_level: u8,
    #[serde(default)]
    pub reputation: u32,
}

impl TownDevelopment {
    #[must_use]
    pub const fn level(&self, building: TownBuilding) -> u8 {
        match building {
            TownBuilding::Depot => self.depot_level,
            TownBuilding::Mechanic => self.mechanic_level,
            TownBuilding::Bank => self.bank_level,
            TownBuilding::ScannerLab => self.scanner_lab_level,
            TownBuilding::SalvageYard => self.salvage_yard_level,
            TownBuilding::ExplosivesShack => self.explosives_shack_level,
        }
    }

    pub const fn level_mut(&mut self, building: TownBuilding) -> &mut u8 {
        match building {
            TownBuilding::Depot => &mut self.depot_level,
            TownBuilding::Mechanic => &mut self.mechanic_level,
            TownBuilding::Bank => &mut self.bank_level,
            TownBuilding::ScannerLab => &mut self.scanner_lab_level,
            TownBuilding::SalvageYard => &mut self.salvage_yard_level,
            TownBuilding::ExplosivesShack => &mut self.explosives_shack_level,
        }
    }

    #[must_use]
    pub fn upgrade_cost(&self, building: TownBuilding) -> u32 {
        350 + u32::from(self.level(building)) * 275
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeepClaimStatus {
    #[default]
    Locked,
    Unlocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SurfaceZone {
    Fuel,
    Repair,
    Depot,
    Headquarters,
    Shop,
    Bank,
    Explosives,
    Salvage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpgradeKind {
    Drill,
    FuelTank,
    CargoBay,
    Engine,
    Hull,
    Radiator,
    Scanner,
    BombPack,
    /// Late-game drill mount: tier 1 (gimbal) drills sideways in flight, tier 2 (crown bore)
    /// also drills upward.
    DrillMount,
}

impl UpgradeKind {
    /// Highest level this upgrade line reaches.
    #[must_use]
    pub const fn max_level(self) -> u8 {
        match self {
            Self::DrillMount => MAX_DRILL_MOUNT_LEVEL,
            _ => MAX_UPGRADE_LEVEL,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpgradeOffer {
    pub kind: UpgradeKind,
    pub name: &'static str,
    pub level: u8,
    pub cost: u32,
    pub description: &'static str,
}

pub const MAX_UPGRADE_LEVEL: u8 = 5;
/// Highest drill mount tier (crown bore).
pub const MAX_DRILL_MOUNT_LEVEL: u8 = 2;
pub const REFUEL_UNIT_COST: u32 = 1;
pub const REPAIR_UNIT_COST: u32 = 2;

/// Number of shop upgrade lines (the length of [`upgrade_offers`]).
pub const UPGRADE_OFFER_COUNT: usize = 9;

#[must_use]
pub fn upgrade_offers(player: &Player) -> [UpgradeOffer; UPGRADE_OFFER_COUNT] {
    [
        offer(UpgradeKind::Drill, player.drill_strength),
        offer(UpgradeKind::FuelTank, player.fuel_tank_level),
        offer(UpgradeKind::CargoBay, player.cargo_bay_level),
        offer(UpgradeKind::Engine, player.engine_level),
        offer(UpgradeKind::Hull, player.hull_level),
        offer(UpgradeKind::Radiator, player.radiator_level),
        offer(UpgradeKind::Scanner, player.scanner_level),
        offer(UpgradeKind::BombPack, 0),
        offer(UpgradeKind::DrillMount, player.drill_mount_level),
    ]
}

/// The band a crew must have reached before the shop sells the level of `kind` after
/// `current_level`, if that level is gated.
///
/// The drill mount is late-game gear:
/// the gimbal unlocks once someone reaches the first deep stratum, the crown bore once someone
/// reaches the pressure cathedrals.
#[must_use]
pub const fn upgrade_unlock_band(kind: UpgradeKind, current_level: u8) -> Option<Band> {
    match (kind, current_level) {
        (UpgradeKind::DrillMount, 0) => Some(Band::Deep(DeepStratum::CrystalFaults)),
        (UpgradeKind::DrillMount, _) => Some(Band::Deep(DeepStratum::PressureCathedrals)),
        _ => None,
    }
}

/// The band still to reach before the shop sells `offer`, or `None` when it is on sale: the
/// world (`settings`) has been dug to `deepest_row` by someone, and maxed lines are never
/// locked.
#[must_use]
pub const fn upgrade_locked_by(
    offer: &UpgradeOffer,
    settings: &WorldSettings,
    deepest_row: i32,
) -> Option<Band> {
    if upgrade_maxed(offer) {
        return None;
    }
    match upgrade_unlock_band(offer.kind, offer.level) {
        Some(band) if !band_reached(settings, deepest_row, band) => Some(band),
        _ => None,
    }
}

/// Whether `offer` is already at its line's top level (bomb packs never max out).
#[must_use]
pub const fn upgrade_maxed(offer: &UpgradeOffer) -> bool {
    !matches!(offer.kind, UpgradeKind::BombPack) && offer.level >= offer.kind.max_level()
}

pub fn refuel_amount(player: &mut Player, fraction: f32) -> u32 {
    let missing = (player.fuel_capacity - player.fuel).ceil().max(0.0);
    let target = (missing * fraction).ceil().max(1.0).min(missing);
    let cost = affordable_service_cost(target, REFUEL_UNIT_COST, player.credits);
    let fuel_added = cost / REFUEL_UNIT_COST;
    player.credits -= cost;
    player.fuel = (player.fuel + fuel_added as f32).min(player.fuel_capacity);
    cost
}

#[cfg(test)]
pub fn refuel(player: &mut Player) -> u32 {
    refuel_amount(player, 1.0)
}

pub fn repair_amount(player: &mut Player, fraction: f32) -> u32 {
    let missing = (player.max_hull() - player.hull).ceil().max(0.0);
    let target = (missing * fraction).ceil().max(1.0).min(missing);
    let cost = affordable_service_cost(target, REPAIR_UNIT_COST, player.credits);
    let hull_added = cost / REPAIR_UNIT_COST;
    player.credits -= cost;
    player.hull = (player.hull + hull_added as f32).min(player.max_hull());
    cost
}

#[cfg(test)]
#[allow(
    clippy::missing_const_for_fn,
    reason = "uses player max hull calculation"
)]
pub fn repair(player: &mut Player) -> u32 {
    repair_amount(player, 1.0)
}

#[cfg(test)]
pub fn sell_cargo(player: &mut Player) -> u32 {
    let mineral_payout: u32 = player
        .cargo
        .iter()
        .map(|(mineral, count)| mineral.value() * count)
        .sum();
    let artifact_payout: u32 = player
        .artifacts
        .iter()
        .map(|(artifact, count)| artifact.value() * count)
        .sum();
    let payout = mineral_payout + artifact_payout;
    player.credits += payout;
    player.cargo.clear();
    player.artifacts.clear();
    payout
}

/// Buys the upgrade offer at `index` for `player`.
///
/// # Errors
///
/// Returns a [`PurchaseError`] when the offer does not exist, is already maxed, or the player
/// cannot afford it.
pub fn buy_upgrade(player: &mut Player, index: usize) -> Result<UpgradeOffer, PurchaseError> {
    let offers = upgrade_offers(player);
    let Some(offer) = offers.get(index).copied() else {
        return Err(PurchaseError::InvalidSelection);
    };

    if upgrade_maxed(&offer) {
        return Err(PurchaseError::MaxLevel);
    }

    if player.credits < offer.cost {
        return Err(PurchaseError::NotEnoughCredits);
    }

    player.credits -= offer.cost;
    apply_upgrade(player, offer.kind);
    Ok(offer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PurchaseError {
    InvalidSelection,
    MaxLevel,
    NotEnoughCredits,
}

fn apply_upgrade(player: &mut Player, kind: UpgradeKind) {
    match kind {
        UpgradeKind::Drill => player.drill_strength += 1,
        UpgradeKind::FuelTank => {
            player.fuel_tank_level += 1;
            player.fuel_capacity += 50.0;
            player.fuel = player.fuel_capacity;
        }
        UpgradeKind::CargoBay => {
            player.cargo_bay_level += 1;
            player.cargo_capacity += 8;
        }
        UpgradeKind::Engine => player.engine_level += 1,
        UpgradeKind::Hull => {
            player.hull_level += 1;
            player.hull = player.max_hull();
        }
        UpgradeKind::Radiator => player.radiator_level += 1,
        UpgradeKind::Scanner => player.scanner_level += 1,
        UpgradeKind::BombPack => player.bombs += 3,
        UpgradeKind::DrillMount => {
            player.drill_mount_level = (player.drill_mount_level + 1).min(MAX_DRILL_MOUNT_LEVEL);
        }
    }
}

/// Raises every upgrade line of `player` to its top level (test mode), refilling fuel and hull.
pub fn max_out_upgrades(player: &mut Player) {
    for kind in [
        UpgradeKind::Drill,
        UpgradeKind::FuelTank,
        UpgradeKind::CargoBay,
        UpgradeKind::Engine,
        UpgradeKind::Hull,
        UpgradeKind::Radiator,
        UpgradeKind::Scanner,
        UpgradeKind::DrillMount,
    ] {
        while current_level(player, kind) < kind.max_level() {
            apply_upgrade(player, kind);
        }
    }
    player.fuel = player.fuel_capacity;
    player.hull = player.max_hull();
}

const fn current_level(player: &Player, kind: UpgradeKind) -> u8 {
    match kind {
        UpgradeKind::Drill => player.drill_strength,
        UpgradeKind::FuelTank => player.fuel_tank_level,
        UpgradeKind::CargoBay => player.cargo_bay_level,
        UpgradeKind::Engine => player.engine_level,
        UpgradeKind::Hull => player.hull_level,
        UpgradeKind::Radiator => player.radiator_level,
        UpgradeKind::Scanner => player.scanner_level,
        UpgradeKind::DrillMount => player.drill_mount_level,
        UpgradeKind::BombPack => 0,
    }
}

fn offer(kind: UpgradeKind, current_level: u8) -> UpgradeOffer {
    let next_level = current_level.saturating_add(1).min(kind.max_level());
    UpgradeOffer {
        kind,
        name: upgrade_name(kind),
        level: current_level,
        cost: upgrade_cost(kind, next_level),
        description: upgrade_description(kind),
    }
}

const fn upgrade_name(kind: UpgradeKind) -> &'static str {
    match kind {
        UpgradeKind::Drill => "Drill Bit",
        UpgradeKind::FuelTank => "Fuel Tank",
        UpgradeKind::CargoBay => "Cargo Bay",
        UpgradeKind::Engine => "Engine",
        UpgradeKind::Hull => "Hull Plating",
        UpgradeKind::Radiator => "Radiator",
        UpgradeKind::Scanner => "Mineral Scanner",
        UpgradeKind::BombPack => "Bomb Pack",
        UpgradeKind::DrillMount => "Drill Mount",
    }
}

const fn upgrade_description(kind: UpgradeKind) -> &'static str {
    match kind {
        UpgradeKind::Drill => "Mine harder and faster",
        UpgradeKind::FuelTank => "Carry more fuel",
        UpgradeKind::CargoBay => "Carry more ore",
        UpgradeKind::Engine => "Stronger lift and handling",
        UpgradeKind::Hull => "Survive harder impacts",
        UpgradeKind::Radiator => "Resist deep heat pressure",
        UpgradeKind::Scanner => "Reveal nearby ore and hazards",
        UpgradeKind::BombPack => "Adds 3 underground blasting charges",
        UpgradeKind::DrillMount => {
            "Gimbal: drill sideways in flight. Crown Bore: thrust into a ceiling to drill up"
        }
    }
}

#[must_use]
pub fn upgrade_tier_name(kind: UpgradeKind, level: u8) -> &'static str {
    let next_level = level.saturating_add(1).min(kind.max_level());
    match (kind, next_level) {
        (UpgradeKind::Drill, 1) => "Steel Drill",
        (UpgradeKind::Drill, 2) => "Carbide Drill",
        (UpgradeKind::Drill, 3) => "Diamond Drill",
        (UpgradeKind::Drill, 4) => "Plasma Drill",
        (UpgradeKind::Drill, _) => "Star Drill",
        (UpgradeKind::FuelTank, 1) => "Aux Fuel Tank",
        (UpgradeKind::FuelTank, 2) => "Twin Fuel Tank",
        (UpgradeKind::FuelTank, 3) => "Pressure Fuel Tank",
        (UpgradeKind::FuelTank, 4) => "Deep Fuel Tank",
        (UpgradeKind::FuelTank, _) => "Endurance Fuel Tank",
        (UpgradeKind::CargoBay, 1) => "Ore Basket",
        (UpgradeKind::CargoBay, 2) => "Cargo Bay",
        (UpgradeKind::CargoBay, 3) => "Expanded Cargo Bay",
        (UpgradeKind::CargoBay, 4) => "Industrial Cargo Bay",
        (UpgradeKind::CargoBay, _) => "Vault Cargo Bay",
        (UpgradeKind::Engine, 1) => "Lift Engine",
        (UpgradeKind::Engine, 2) => "Tuned Engine",
        (UpgradeKind::Engine, 3) => "Turbo Engine",
        (UpgradeKind::Engine, 4) => "Vector Engine",
        (UpgradeKind::Engine, _) => "Fusion Engine",
        (UpgradeKind::Hull, 1) => "Riveted Hull",
        (UpgradeKind::Hull, 2) => "Reinforced Hull",
        (UpgradeKind::Hull, 3) => "Titanium Hull",
        (UpgradeKind::Hull, 4) => "Ablative Hull",
        (UpgradeKind::Hull, _) => "Star Hull",
        (UpgradeKind::Radiator, 1) => "Basic Radiator",
        (UpgradeKind::Radiator, 2) => "Copper Radiator",
        (UpgradeKind::Radiator, 3) => "Liquid Radiator",
        (UpgradeKind::Radiator, 4) => "Cryo Radiator",
        (UpgradeKind::Radiator, _) => "Magma Radiator",
        (UpgradeKind::Scanner, 1) => "Pulse Scanner",
        (UpgradeKind::Scanner, 2) => "Ore Scanner",
        (UpgradeKind::Scanner, 3) => "Hazard Scanner",
        (UpgradeKind::Scanner, 4) => "Artifact Scanner",
        (UpgradeKind::Scanner, _) => "Deep Scanner",
        (UpgradeKind::BombPack, _) => "Pack of 3 Bombs",
        (UpgradeKind::DrillMount, 1) => "Gimbal Drill Mount",
        (UpgradeKind::DrillMount, _) => "Crown Bore Mount",
    }
}

fn upgrade_cost(kind: UpgradeKind, next_level: u8) -> u32 {
    let base = match kind {
        UpgradeKind::Drill => 105,
        UpgradeKind::FuelTank => 85,
        UpgradeKind::CargoBay => 95,
        UpgradeKind::Engine => 100,
        UpgradeKind::Hull => 90,
        UpgradeKind::Radiator => 115,
        UpgradeKind::Scanner => 135,
        UpgradeKind::BombPack => 80,
        // Late-game: 6,000 cr for the gimbal, 24,000 cr for the crown bore.
        UpgradeKind::DrillMount => 6_000,
    };
    base * u32::from(next_level) * u32::from(next_level)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "service amount is bounded by small fuel/hull capacities"
)]
fn affordable_service_cost(missing: f32, unit_cost: u32, credits: u32) -> u32 {
    let requested = missing as u32 * unit_cost;
    requested.min(credits)
}

#[cfg(test)]
mod tests {
    use crate::{
        economy::{refuel, repair, sell_cargo},
        player::Player,
        terrain::{ArtifactKind, MineralKind},
    };

    #[test]
    fn depot_sells_minerals_and_artifacts() {
        let mut player = Player::new(0.0, 0.0);
        assert!(player.add_cargo(MineralKind::Gold));
        assert!(player.add_artifact(ArtifactKind::Fossil));

        let payout = sell_cargo(&mut player);

        assert_eq!(
            payout,
            MineralKind::Gold.value() + ArtifactKind::Fossil.value()
        );
        assert_eq!(player.credits, payout);
        assert_eq!(player.cargo_used(), 0);
    }

    #[test]
    fn service_costs_are_limited_by_available_credits() {
        let mut player = Player::new(0.0, 0.0);
        player.credits = 10;
        player.fuel = 0.0;
        player.hull = 0.0;

        assert_eq!(refuel(&mut player), 10);
        assert_eq!(player.credits, 0);
        assert_eq!(repair(&mut player), 0);
    }
}
