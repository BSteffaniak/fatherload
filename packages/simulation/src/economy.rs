#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "service pricing intentionally converts small fuel/hull unit counts"
)]

use serde::{Deserialize, Serialize};

use crate::{
    player::{
        Player, cargo_capacity_for_level, drill_bite, drill_speed, engine_power,
        fuel_capacity_for_level, hull_capacity_for_level,
    },
    protection::{
        BOULDER_DEFLECTOR_MAX_LEVEL, GAS_SCRUBBER_MAX_LEVEL, HEAT_SHIELD_MAX_LEVEL,
        LAVA_PLATING_MAX_LEVEL, MAX_BAND_HEAT, PRESSURE_BRACING_MAX_LEVEL, heat_cooling,
        protection_fraction,
    },
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
    /// Hazard counter: extra cooling on top of the radiator (see [`crate::protection`]).
    HeatShield,
    /// Hazard counter: cuts lava burns.
    LavaPlating,
    /// Hazard counter: filters gas clouds.
    GasScrubber,
    /// Hazard counter: absorbs pressure pockets, pocket blasts and magma vents.
    PressureBracing,
    /// Hazard counter: deflects falling boulders and cave-ins.
    BoulderDeflector,
}

impl UpgradeKind {
    /// Every upgrade line, in shop order (index `i` is the shop's offer `i`).
    pub const ALL: [Self; UPGRADE_OFFER_COUNT] = [
        Self::Drill,
        Self::FuelTank,
        Self::CargoBay,
        Self::Engine,
        Self::Hull,
        Self::Radiator,
        Self::Scanner,
        Self::BombPack,
        Self::DrillMount,
        Self::HeatShield,
        Self::LavaPlating,
        Self::GasScrubber,
        Self::PressureBracing,
        Self::BoulderDeflector,
    ];

    /// Highest level this upgrade line reaches.
    #[must_use]
    pub const fn max_level(self) -> u8 {
        match self {
            Self::DrillMount => MAX_DRILL_MOUNT_LEVEL,
            Self::Scanner => MAX_SCANNER_LEVEL,
            Self::HeatShield => HEAT_SHIELD_MAX_LEVEL,
            Self::LavaPlating => LAVA_PLATING_MAX_LEVEL,
            Self::GasScrubber => GAS_SCRUBBER_MAX_LEVEL,
            Self::PressureBracing => PRESSURE_BRACING_MAX_LEVEL,
            Self::BoulderDeflector => BOULDER_DEFLECTOR_MAX_LEVEL,
            Self::Drill
            | Self::FuelTank
            | Self::CargoBay
            | Self::Engine
            | Self::Hull
            | Self::Radiator
            | Self::BombPack => MAX_UPGRADE_LEVEL,
        }
    }

    /// The level a fresh rig starts this line at (the stock part counts as tier 1 for the core
    /// lines; scanners, mounts and hazard counters start uninstalled).
    #[must_use]
    pub const fn starting_level(self) -> u8 {
        match self {
            Self::Drill
            | Self::FuelTank
            | Self::CargoBay
            | Self::Engine
            | Self::Hull
            | Self::Radiator => 1,
            Self::Scanner
            | Self::BombPack
            | Self::DrillMount
            | Self::HeatShield
            | Self::LavaPlating
            | Self::GasScrubber
            | Self::PressureBracing
            | Self::BoulderDeflector => 0,
        }
    }

    /// Whether this line is a hazard counter.
    #[must_use]
    pub const fn is_hazard_counter(self) -> bool {
        matches!(
            self,
            Self::HeatShield
                | Self::LavaPlating
                | Self::GasScrubber
                | Self::PressureBracing
                | Self::BoulderDeflector
        )
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

/// Top tier of the core upgrade lines (drill, fuel tank, cargo bay, engine, hull, radiator).
pub const MAX_UPGRADE_LEVEL: u8 = 15;
/// Top scanner tier.
pub const MAX_SCANNER_LEVEL: u8 = 10;
/// Highest drill mount tier (crown bore).
pub const MAX_DRILL_MOUNT_LEVEL: u8 = 2;
pub const REFUEL_UNIT_COST: u32 = 1;
pub const REPAIR_UNIT_COST: u32 = 2;

/// Number of shop upgrade lines (the length of [`upgrade_offers`]).
pub const UPGRADE_OFFER_COUNT: usize = 14;
/// Shop index of the drill mount line.
pub const DRILL_MOUNT_OFFER_INDEX: usize = 8;

#[must_use]
pub fn upgrade_offers(player: &Player) -> [UpgradeOffer; UPGRADE_OFFER_COUNT] {
    UpgradeKind::ALL.map(|kind| offer(kind, current_level(player, kind)))
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
    let gear = &mut player.hazard_gear;
    match kind {
        UpgradeKind::Drill => player.drill_strength += 1,
        UpgradeKind::FuelTank => {
            let level = player.fuel_tank_level;
            let added = fuel_capacity_for_level(level + 1) - fuel_capacity_for_level(level);
            player.fuel_tank_level += 1;
            // Rigs from older curves catch up to this tier's tank on their next purchase.
            player.fuel_capacity =
                (player.fuel_capacity + added).max(fuel_capacity_for_level(player.fuel_tank_level));
            player.fuel = player.fuel_capacity;
        }
        UpgradeKind::CargoBay => {
            let level = player.cargo_bay_level;
            let added = cargo_capacity_for_level(level + 1) - cargo_capacity_for_level(level);
            player.cargo_bay_level += 1;
            player.cargo_capacity = player
                .cargo_capacity
                .saturating_add(added)
                .max(cargo_capacity_for_level(player.cargo_bay_level));
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
        UpgradeKind::HeatShield => gear.heat_shield += 1,
        UpgradeKind::LavaPlating => gear.lava_plating += 1,
        UpgradeKind::GasScrubber => gear.gas_scrubber += 1,
        UpgradeKind::PressureBracing => gear.pressure_bracing += 1,
        UpgradeKind::BoulderDeflector => gear.boulder_deflector += 1,
    }
}

/// Raises every upgrade line of `player` (hazard counters included) to its top level (test
/// mode), refilling fuel and hull.
pub fn max_out_upgrades(player: &mut Player) {
    for kind in UpgradeKind::ALL {
        if kind == UpgradeKind::BombPack {
            continue;
        }
        while current_level(player, kind) < kind.max_level() {
            apply_upgrade(player, kind);
        }
    }
    player.fuel = player.fuel_capacity;
    player.hull = player.max_hull();
}

/// The level `player` has reached on the `kind` line (bomb packs are always level 0).
#[must_use]
pub const fn current_level(player: &Player, kind: UpgradeKind) -> u8 {
    let gear = player.hazard_gear;
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
        UpgradeKind::HeatShield => gear.heat_shield,
        UpgradeKind::LavaPlating => gear.lava_plating,
        UpgradeKind::GasScrubber => gear.gas_scrubber,
        UpgradeKind::PressureBracing => gear.pressure_bracing,
        UpgradeKind::BoulderDeflector => gear.boulder_deflector,
    }
}

fn offer(kind: UpgradeKind, current_level: u8) -> UpgradeOffer {
    UpgradeOffer {
        kind,
        name: upgrade_name(kind),
        level: current_level,
        cost: upgrade_cost(kind, current_level),
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
        UpgradeKind::HeatShield => "Heat Shielding",
        UpgradeKind::LavaPlating => "Lava Plating",
        UpgradeKind::GasScrubber => "Gas Scrubber",
        UpgradeKind::PressureBracing => "Pressure Bracing",
        UpgradeKind::BoulderDeflector => "Boulder Deflector",
    }
}

const fn upgrade_description(kind: UpgradeKind) -> &'static str {
    match kind {
        UpgradeKind::Drill => "Mine harder rock, faster: late tiers bite several layers per chip",
        UpgradeKind::FuelTank => "Carry more fuel and burn less of it",
        UpgradeKind::CargoBay => "Carry more ore",
        UpgradeKind::Engine => "Stronger lift and handling, even with a full bay",
        UpgradeKind::Hull => "Survive harder impacts",
        UpgradeKind::Radiator => "Cools the rig: each tier keeps it safe in hotter bands",
        UpgradeKind::Scanner => "Reveal nearby ore and hazards, further with every tier",
        UpgradeKind::BombPack => "Adds 3 underground blasting charges",
        UpgradeKind::DrillMount => {
            "Gimbal: drill sideways in flight. Crown Bore: thrust into a ceiling to drill up"
        }
        UpgradeKind::HeatShield => {
            "Each tier cools one more heat level on top of the radiator; with enough, even the \
             mantle runs cool"
        }
        UpgradeKind::LavaPlating => {
            "Cuts lava burns from drilling through or skimming past lava; the top tier is \
             lava-proof"
        }
        UpgradeKind::GasScrubber => {
            "Filters gas clouds from drilled pockets and blooms; the top tier breathes any gas"
        }
        UpgradeKind::PressureBracing => {
            "Absorbs pressure pocket kicks, pocket blasts and magma vents; the top tier shrugs \
             them off"
        }
        UpgradeKind::BoulderDeflector => {
            "Deflects falling boulders and cave-ins; the top tier is rockfall-proof"
        }
    }
}

const DRILL_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Steel Drill",
    "Carbide Drill",
    "Tempered Drill",
    "Diamond Drill",
    "Plasma Drill",
    "Twin-Bit Drill",
    "Sonic Drill",
    "Thermite Drill",
    "Graphene Drill",
    "Resonant Drill",
    "Fusion Drill",
    "Quantum Drill",
    "Singularity Drill",
    "Nova Drill",
    "Star Drill",
];
const FUEL_TANK_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Aux Fuel Tank",
    "Twin Fuel Tank",
    "Pressure Fuel Tank",
    "Deep Fuel Tank",
    "Endurance Fuel Tank",
    "Bladder Fuel Tank",
    "Cryo Fuel Tank",
    "Reserve Fuel Tank",
    "Expedition Fuel Tank",
    "Leviathan Fuel Tank",
    "Abyssal Fuel Tank",
    "Reactor Feed Tank",
    "Titan Fuel Tank",
    "Bottomless Fuel Tank",
    "Infinity Fuel Tank",
];
const CARGO_BAY_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Ore Basket",
    "Cargo Bay",
    "Expanded Cargo Bay",
    "Industrial Cargo Bay",
    "Vault Cargo Bay",
    "Freight Bay",
    "Compression Bay",
    "Twin Freight Bay",
    "Bulk Hauler Bay",
    "Mega Hold",
    "Colossus Hold",
    "Fold-Space Hold",
    "Leviathan Hold",
    "Titan Hold",
    "Bottomless Hold",
];
const ENGINE_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Lift Engine",
    "Tuned Engine",
    "Turbo Engine",
    "Vector Engine",
    "Fusion Engine",
    "Twin-Turbine Engine",
    "Ion Engine",
    "Plasma Engine",
    "Afterburner Engine",
    "Gravitic Engine",
    "Pulse Engine",
    "Antimatter Engine",
    "Warp Engine",
    "Titan Engine",
    "Star Engine",
];
const HULL_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Riveted Hull",
    "Reinforced Hull",
    "Titanium Hull",
    "Ablative Hull",
    "Composite Hull",
    "Ceramic Hull",
    "Tungsten Hull",
    "Graphene Hull",
    "Nanoweave Hull",
    "Fortress Hull",
    "Bastion Hull",
    "Adamant Hull",
    "Titan Hull",
    "Neutron Hull",
    "Star Hull",
];
const RADIATOR_TIERS: [&str; MAX_UPGRADE_LEVEL as usize] = [
    "Basic Radiator",
    "Copper Radiator",
    "Liquid Radiator",
    "Cryo Radiator",
    "Magma Radiator",
    "Twin Radiator",
    "Vapor Radiator",
    "Heat-Pipe Radiator",
    "Superfluid Radiator",
    "Phase Radiator",
    "Glacier Radiator",
    "Plasma Radiator",
    "Zero-Point Radiator",
    "Mantle Radiator",
    "Core Radiator",
];
const SCANNER_TIERS: [&str; MAX_SCANNER_LEVEL as usize] = [
    "Pulse Scanner",
    "Ore Scanner",
    "Hazard Scanner",
    "Artifact Scanner",
    "Deep Scanner",
    "Sonar Scanner",
    "Seismic Scanner",
    "Spectral Scanner",
    "Quantum Scanner",
    "Omni Scanner",
];
const HEAT_SHIELD_TIERS: [&str; HEAT_SHIELD_MAX_LEVEL as usize] = [
    "Foil Heat Shield",
    "Ceramic Heat Shield",
    "Ablative Heat Shield",
    "Cryo Heat Shield",
    "Magma Heat Shield",
    "Core Heat Shield",
];
const LAVA_PLATING_TIERS: [&str; LAVA_PLATING_MAX_LEVEL as usize] = [
    "Scorch Plating",
    "Basalt Plating",
    "Obsidian Plating",
    "Ceramic Lava Plating",
    "Tungsten Lava Plating",
    "Magma-Forged Plating",
    "Mantle Plating",
    "Lava Walker Plating",
];
const GAS_SCRUBBER_TIERS: [&str; GAS_SCRUBBER_MAX_LEVEL as usize] = [
    "Filter Mask",
    "Charcoal Scrubber",
    "Catalytic Scrubber",
    "Sealed Cabin",
    "Ion Scrubber",
    "Closed-Loop Life Support",
];
const PRESSURE_BRACING_TIERS: [&str; PRESSURE_BRACING_MAX_LEVEL as usize] = [
    "Strut Bracing",
    "Shock Bracing",
    "Hydraulic Bracing",
    "Blast Bracing",
    "Vent-Proof Bracing",
    "Bulwark Bracing",
];
const BOULDER_DEFLECTOR_TIERS: [&str; BOULDER_DEFLECTOR_MAX_LEVEL as usize] = [
    "Roll Cage",
    "Deflector Plate",
    "Sloped Deflector",
    "Kinetic Deflector",
    "Repulsor Deflector",
    "Rockfall Aegis",
];

/// The name of the tier the shop sells next on the `kind` line, when `level` is fitted (the top
/// tier's name once the line is maxed).
#[must_use]
pub fn upgrade_tier_name(kind: UpgradeKind, level: u8) -> &'static str {
    let tiers: &[&'static str] = match kind {
        UpgradeKind::Drill => &DRILL_TIERS,
        UpgradeKind::FuelTank => &FUEL_TANK_TIERS,
        UpgradeKind::CargoBay => &CARGO_BAY_TIERS,
        UpgradeKind::Engine => &ENGINE_TIERS,
        UpgradeKind::Hull => &HULL_TIERS,
        UpgradeKind::Radiator => &RADIATOR_TIERS,
        UpgradeKind::Scanner => &SCANNER_TIERS,
        UpgradeKind::HeatShield => &HEAT_SHIELD_TIERS,
        UpgradeKind::LavaPlating => &LAVA_PLATING_TIERS,
        UpgradeKind::GasScrubber => &GAS_SCRUBBER_TIERS,
        UpgradeKind::PressureBracing => &PRESSURE_BRACING_TIERS,
        UpgradeKind::BoulderDeflector => &BOULDER_DEFLECTOR_TIERS,
        UpgradeKind::BombPack => return "Pack of 3 Bombs",
        UpgradeKind::DrillMount => {
            return if level == 0 {
                "Gimbal Drill Mount"
            } else {
                "Crown Bore Mount"
            };
        }
    };
    let next_level = level.saturating_add(1).clamp(1, kind.max_level());
    tiers[usize::from(next_level - 1).min(tiers.len() - 1)]
}

/// What the next tier of `kind` changes for `player`, as `"now -> next"` (for the shop's
/// details footer), or `None` for maxed lines and lines without a single headline number.
#[must_use]
pub fn upgrade_effect(player: &Player, kind: UpgradeKind) -> Option<String> {
    let level = current_level(player, kind);
    if level >= kind.max_level() {
        return None;
    }
    let next = level + 1;
    let gear = player.hazard_gear;
    let percent = |fraction: f32| (fraction * 100.0).round() as u32;
    Some(match kind {
        UpgradeKind::Drill => format!(
            "Dig rate x{:.1} -> x{:.1}",
            drill_speed(level) * f32::from(drill_bite(level)),
            drill_speed(next) * f32::from(drill_bite(next)),
        ),
        UpgradeKind::FuelTank => format!(
            "Tank {:.0} -> {:.0} fuel",
            fuel_capacity_for_level(level),
            fuel_capacity_for_level(next)
        ),
        UpgradeKind::CargoBay => format!(
            "Bay {} -> {} slots",
            cargo_capacity_for_level(level),
            cargo_capacity_for_level(next)
        ),
        UpgradeKind::Engine => format!(
            "Thrust x{:.2} -> x{:.2}",
            engine_power(level),
            engine_power(next)
        ),
        UpgradeKind::Hull => format!(
            "Plating {:.0} -> {:.0} hull",
            hull_capacity_for_level(level),
            hull_capacity_for_level(next)
        ),
        UpgradeKind::Radiator => format!(
            "Cools heat {} -> {} of {MAX_BAND_HEAT} with your shielding",
            heat_cooling(level, gear.heat_shield),
            heat_cooling(next, gear.heat_shield)
        ),
        UpgradeKind::HeatShield => format!(
            "Cools heat {} -> {} of {MAX_BAND_HEAT} with your radiator",
            heat_cooling(player.radiator_level, level),
            heat_cooling(player.radiator_level, next)
        ),
        UpgradeKind::LavaPlating
        | UpgradeKind::GasScrubber
        | UpgradeKind::PressureBracing
        | UpgradeKind::BoulderDeflector => format!(
            "Stops {}% -> {}% of it",
            percent(protection_fraction(level, kind.max_level())),
            percent(protection_fraction(next, kind.max_level()))
        ),
        UpgradeKind::Scanner | UpgradeKind::BombPack | UpgradeKind::DrillMount => return None,
    })
}

/// Price of the tier after `level` on the `kind` line: the first purchase is cheap and every
/// tier after it costs a fixed percentage more, so the top tiers line up with deep cargo runs.
#[must_use]
pub fn upgrade_cost(kind: UpgradeKind, level: u8) -> u32 {
    let (first, growth_percent): (u64, u64) = match kind {
        UpgradeKind::Drill => (420, 155),
        UpgradeKind::FuelTank => (340, 150),
        UpgradeKind::CargoBay => (380, 152),
        UpgradeKind::Engine => (400, 152),
        UpgradeKind::Hull => (360, 150),
        UpgradeKind::Radiator => (460, 150),
        UpgradeKind::Scanner => (135, 175),
        UpgradeKind::BombPack => return 80,
        // Late-game: 6,000 cr for the gimbal, 24,000 cr for the crown bore.
        UpgradeKind::DrillMount => (6_000, 400),
        UpgradeKind::HeatShield => (900, 200),
        UpgradeKind::LavaPlating => (800, 175),
        UpgradeKind::GasScrubber => (450, 185),
        UpgradeKind::PressureBracing => (600, 190),
        UpgradeKind::BoulderDeflector => (350, 185),
    };
    // Tiers bought on this line so far (the stock part is free).
    let bought = level
        .saturating_sub(kind.starting_level())
        .min(kind.max_level());
    let mut cost = first;
    for _ in 0..bought {
        cost = cost * growth_percent / 100;
    }
    let step = match cost {
        0..1_000 => 10,
        1_000..10_000 => 50,
        10_000..100_000 => 250,
        _ => 1_000,
    };
    u32::try_from(cost.div_ceil(step) * step).unwrap_or(u32::MAX)
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
