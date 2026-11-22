//! The descent probe (Part III F10): a bot plays a whole Deep Descent, 41 strata to the
//! Heart, and the report reads its clock.
//!
//! **What is real and what is modelled.** A 60-90 hour run cannot go through the fixed tick in
//! a report, and the V11 strata do not exist yet, so the probe is a model on top of real
//! numbers:
//!
//! * *Real*: every curve the bot meets is the game's own: the Descent ladders and their caps
//!   ([`crate::economy::upgrade_cost_for`] under [`ProgressionRules::Descent`],
//!   [`crate::depth_progression::core_tier_cap`]), the rig systems' prices and gates, the tank,
//!   bay and hull curves, the drill's chip rate and bite, fuel burn, the logistics prices
//!   (beacons, outpost fees and claims, travel costs, lift speeds) and the ore value curve.
//!   With `calibrate` the drill's real pace is measured through the tick (a rig drilling a
//!   column of hard rock, [`measure_dig`]) and the model's dig rate is scaled to match.
//! * *Modelled*: the strata themselves, through the [`StratumRegistry`] adapter. Until the F8
//!   stratum registry lands, [`PlanStrata`] derives each stratum's rock, ore, hazards, heat and
//!   content from the plan's §14 table ([`crate::depth_progression::PlanLayout`]); the
//!   integrator swaps in the real registry by implementing the trait.
//!
//! **The bot.** Before each stratum it must meet its entry needs (a drill that cuts its rock, a
//! hull for its hazards, a radiator for its heat, and the Descent's gate system); until then it
//! makes mining trips from its nearest base (the town, reached outposts, its own beacons every
//! [`BEACON_SPACING_ROWS`]), selling at the outpost and buying upgrades. Each stratum also
//! costs its authored content's minutes (creatures, landmark, mechanic, story). A trip whose
//! hazard damage would wreck the rig is a death (a rescue fee and half the cargo), after which
//! the bot buys hull first.
//!
//! **What it reports.** Time per stratum and per Descent against §14's hours, the "something
//! new every 20-40 minutes" check (the gaps between new things: a stratum's arrival, its
//! creatures, landmark and story beat, an upgrade tier, a gate), deaths, fuel on everyday trips
//! against expeditions, and the estimated whole run (target 60-90 h).

use std::fmt::Write as _;

use super::{BalanceProbe, ProbeContext, ReportSection, Table};
use crate::{
    ControlState, PlayerId, SimulationParams, WorldState,
    depth_progression::{
        DESCENT_COUNT, DescentLayout, DescentSpan, PlanLayout, ProgressionRules, RigSystem,
        core_tier_cap, descent_gate, descent_value_percent,
    },
    economy::{MAX_UPGRADE_LEVEL, UpgradeKind, upgrade_cost_for},
    logistics::{
        ANCIENT_LIFT_RESTORE_PRICE, BEACON_PRICE, OUTPOST_CLAIM_PRICE, OUTPOST_FUEL_UNIT,
        OUTPOST_SALE_PERCENT, scaled_price, travel_cost,
    },
    player::{
        cargo_capacity_for_level, drill_bite, drill_speed, engine_power, fuel_burn_efficiency,
        fuel_capacity_for_level, hull_capacity_for_level,
    },
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition, tile_durability},
    world::{AUTHORITATIVE_FUEL_BURN_PER_SECOND, MIN_DRILL_SECONDS_PER_CHIP},
};

/// One stratum as the descent bot meets it.
#[derive(Clone, Debug, PartialEq)]
pub struct DescentStratum {
    pub descent: u8,
    pub name: &'static str,
    pub top: i32,
    pub bottom: i32,
    /// The drill tier that cuts its rock at the comfortable pace (slower below it).
    pub rock_tier: u8,
    /// Its common rock, and how many times tougher than that tile it is (deep rock).
    pub rock: TileKind,
    pub toughness: f64,
    /// Credits one ore tile sells for here.
    pub ore_value: f64,
    /// Share of the tiles a mining rig digs that are ore.
    pub ore_share: f64,
    /// Hull per minute of mining a rig with hull tier equal to `rock_tier` takes.
    pub hazard_per_minute: f64,
    /// The radiator tier its heat needs.
    pub heat_tier: u8,
    /// New things it brings: its arrival, its creatures, its landmark, its mechanic and its
    /// story beat.
    pub novelties: u8,
    /// Minutes its authored content takes (creatures met, the landmark, the story beat).
    pub content_minutes: f64,
}

impl DescentStratum {
    #[must_use]
    pub const fn rows(&self) -> i32 {
        self.bottom - self.top
    }
}

/// The strata the descent bot plays (the F8 stratum registry implements it: [`RegistryStrata`]).
pub trait StratumRegistry {
    /// Every stratum, top to bottom.
    fn strata(&self) -> Vec<DescentStratum>;

    /// The eight Descents' rows (rows below the ground line): the plan's table unless the
    /// registry knows its own.
    fn descents(&self) -> Vec<DescentSpan> {
        PlanLayout.descents().to_vec()
    }
}

/// The real stratum registry ([`crate::descent`]) on a Deep Descent world's seeded layout, as
/// the descent bot plays it (Wave 7 integration).
///
/// From the registry: each stratum's name, its seeded rows (counted from the ground line), its
/// common rock, its ores' values through the stratum (the Descent value curve applied), its
/// hazard weights, and its novelties and content time from its creatures, landmarks and field
/// mechanic. Still modelled like [`PlanStrata`] (the registry has no numbers for them yet):
/// the drill and radiator tiers each stratum asks for and how much tougher deep rock is.
#[derive(Clone, Copy, Debug)]
pub struct RegistryStrata {
    pub settings: crate::worldgen::WorldSettings,
}

impl RegistryStrata {
    /// The registry on the default seed's Deep Descent world.
    #[must_use]
    pub const fn default_world() -> Self {
        Self {
            settings: crate::worldgen::WorldSettings {
                generator: crate::worldgen::GeneratorVersion::DEEP_DESCENT,
                ..crate::worldgen::WorldSettings::new(crate::worldgen::DEFAULT_WORLD_SEED)
            },
        }
    }

    fn ground(&self) -> i32 {
        crate::descent::fields::cached_layout(&self.settings).tops[0]
    }
}

impl StratumRegistry for RegistryStrata {
    fn strata(&self) -> Vec<DescentStratum> {
        use crate::descent::{LandmarkKind, Stratum};
        let layout = crate::descent::fields::cached_layout(&self.settings);
        let ground = self.ground();
        let plan = PlanStrata.strata();
        let last = (Stratum::ALL.len() - 1) as f64;
        Stratum::ALL
            .iter()
            .zip(&plan)
            .enumerate()
            .map(|(index, (stratum, modelled))| {
                let def = stratum.def();
                let (top, bottom) = layout.rows(*stratum);
                let descent = u8::try_from(stratum.descent().index()).unwrap_or(u8::MAX);
                let share = index as f64 / last;
                // The registry's ores through the middle of the stratum, by weight.
                let weights = def.ore_weights(500);
                let total: u32 = weights.iter().sum();
                let classic = if total == 0 {
                    modelled.ore_value * 100.0 / f64::from(descent_value_percent(descent))
                } else {
                    weights
                        .iter()
                        .zip(crate::worldgen::deposits::MINERALS)
                        .map(|(weight, mineral)| f64::from(*weight) * f64::from(mineral.value()))
                        .sum::<f64>()
                        / f64::from(total)
                };
                let hazard_weight: u32 = def.hazards.iter().map(|(_, weight)| weight).sum();
                let creatures = u8::try_from(def.creatures.len()).unwrap_or(u8::MAX);
                let set_pieces = u8::try_from(
                    def.landmarks
                        .iter()
                        .filter(|kind| {
                            matches!(
                                kind,
                                LandmarkKind::Outpost
                                    | LandmarkKind::GateChamber
                                    | LandmarkKind::AnomalyRoom
                                    | LandmarkKind::HollowCavern
                                    | LandmarkKind::Settlements
                            )
                        })
                        .count(),
                )
                .unwrap_or(u8::MAX);
                let mechanic = u8::from(!def.field.is_neutral() || !def.fluids.is_empty());
                DescentStratum {
                    descent,
                    name: def.name,
                    top: top - ground,
                    bottom: bottom + 1 - ground,
                    rock_tier: modelled.rock_tier,
                    rock: def.soft,
                    toughness: modelled.toughness,
                    ore_value: classic * f64::from(descent_value_percent(descent)) / 100.0,
                    ore_share: 0.2,
                    hazard_per_minute: modelled.hazard_per_minute
                        * (0.5 + f64::from(hazard_weight) / 16.0).min(2.0),
                    heat_tier: (1.0 + 20.0 * share).round() as u8,
                    // Arrival, the story beat, each creature, each set piece and the mechanic.
                    novelties: 2 + creatures + set_pieces + mechanic,
                    content_minutes: 7.0f64.mul_add(
                        f64::from(creatures),
                        6.0f64.mul_add(f64::from(set_pieces + mechanic), 18.0),
                    ),
                }
            })
            .collect()
    }

    fn descents(&self) -> Vec<DescentSpan> {
        let ground = self.ground();
        crate::depth_progression::RegistryLayout::of(&self.settings)
            .descents()
            .iter()
            .map(|span| DescentSpan {
                top: span.top - ground,
                bottom: span.bottom - ground,
                ..*span
            })
            .collect()
    }
}

/// Classic mineral values the plan's ore ramps through.
const MINERAL_LADDER: [MineralKind; 10] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
];

/// The plan's §14 strata as a [`StratumRegistry`].
///
/// Rock, ore, hazards and heat ramp with the stratum's place in the run, ore value follows the
/// Descent value curve, and every stratum carries the plan's 2-4 creatures, a landmark, a
/// mechanic and a story beat.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlanStrata;

impl StratumRegistry for PlanStrata {
    fn strata(&self) -> Vec<DescentStratum> {
        let strata = PlanLayout.strata();
        let last = (strata.len().max(2) - 1) as f64;
        strata
            .iter()
            .enumerate()
            .map(|(index, span)| {
                let share = index as f64 / last;
                // The classic ladder runs the whole way down (mythril at the Heart), and deep
                // ore is worth the Descent's percent of it.
                // A mining rig's haul is a mix: the Topsoil's averages iron and silver.
                let ladder = (1.5 + share * 7.5).min((MINERAL_LADDER.len() - 1) as f64);
                let low = f64::from(MINERAL_LADDER[ladder.floor() as usize].value());
                let high = f64::from(
                    MINERAL_LADDER[(ladder.ceil() as usize).min(MINERAL_LADDER.len() - 1)].value(),
                );
                let classic = (high - low).mul_add(ladder.fract(), low);
                let creatures = 2 + (index % 3) as u8;
                DescentStratum {
                    descent: span.descent,
                    name: span.name,
                    top: span.top,
                    bottom: span.bottom,
                    rock_tier: (1.0 + 21.0 * share.powf(0.85)).round() as u8,
                    rock: match index {
                        0 => TileKind::Dirt,
                        1 => TileKind::Clay,
                        2 | 3 => TileKind::Stone,
                        _ => TileKind::HardRock,
                    },
                    toughness: 1.0 + 0.1 * index.saturating_sub(11) as f64,
                    ore_value: classic * f64::from(descent_value_percent(span.descent)) / 100.0,
                    ore_share: 0.2,
                    hazard_per_minute: 0.8 * 1.1_f64.powi(i32::try_from(index).unwrap_or(i32::MAX)),
                    heat_tier: (1.0 + 20.0 * share).round() as u8,
                    novelties: 4 + creatures,
                    content_minutes: 24.0 + 7.0 * f64::from(creatures),
                }
            })
            .collect()
    }
}

/// Rows between the beacons the bot plants on its way down.
pub const BEACON_SPACING_ROWS: i32 = 350;
/// Fuel each drilled tile costs (`world/commands.rs`).
const DIG_FUEL_PER_TILE: f64 = 0.45;
/// Share of mining time spent on thrust (hops up to the next pocket of ore).
const THRUST_SHARE_MINING: f64 = 0.05;
/// Seconds of counter time per trip (selling, refuelling, buying).
const SERVICE_SECONDS: f64 = 25.0;
/// Rows per second a rig falls down an open shaft.
const FALL_ROWS_PER_SECOND: f64 = 16.0;
/// Rows per second a stock engine climbs (the climb cap rises with the engine).
const STOCK_CLIMB_ROWS_PER_SECOND: f64 = 7.0;
/// Seconds per tile a rig needs besides chipping (falling or driving onto the next tile),
/// unless calibrated through the tick.
const DEFAULT_DIG_OVERHEAD: f64 = 0.4;

/// How the descent probe runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescentConfig {
    /// Play only the first strata (tests).
    pub strata_limit: Option<usize>,
    /// Measure the drill's pace through the real tick.
    pub calibrate: bool,
    pub tick_rate_hz: u32,
}

impl Default for DescentConfig {
    fn default() -> Self {
        Self {
            strata_limit: None,
            calibrate: true,
            tick_rate_hz: crate::DEFAULT_TICK_RATE_HZ,
        }
    }
}

/// One stratum of the run.
#[derive(Clone, Debug, PartialEq)]
pub struct StratumRun {
    pub stratum: DescentStratum,
    /// Minutes spent in it (content, mining to afford what the next one needs, digging).
    pub minutes: f64,
    /// Hours from the start of the run when it was left.
    pub end_hours: f64,
    pub trips: u32,
    pub deaths: u32,
    /// Drill and hull tiers on arrival.
    pub drill_tier: u8,
    pub hull_tier: u8,
    /// Fuel an everyday trip from the nearest base burned, as a share of the tank (percent).
    pub everyday_fuel_percent: f64,
    /// The longest wait between new things inside it (minutes).
    pub longest_gap_minutes: f64,
    /// Upgrade tiers and systems bought in it.
    pub purchases: u32,
}

/// The whole descent.
#[derive(Clone, Debug, PartialEq)]
pub struct DescentRun {
    pub strata: Vec<StratumRun>,
    /// Hours at which each Descent was finished.
    pub descent_hours: Vec<f64>,
    /// Fuel an expedition (from the Descent's outpost to its bottom and back, no beacons)
    /// would burn, as a share of the tank the bot had there (percent).
    pub expedition_fuel_percent: Vec<f64>,
    /// When each gate system was bought (hours).
    pub systems: Vec<(RigSystem, f64)>,
    /// Every gap between new things (minutes).
    pub gaps: Vec<f64>,
    /// The dig pace scale measured through the tick (or the default).
    pub dig_overhead: f64,
    pub deaths: u32,
}

impl DescentRun {
    #[must_use]
    pub fn total_hours(&self) -> f64 {
        self.strata.last().map_or(0.0, |run| run.end_hours)
    }

    /// Share of gaps between new things that are at most 40 minutes (percent).
    #[must_use]
    pub fn gaps_within_40(&self) -> f64 {
        if self.gaps.is_empty() {
            return 100.0;
        }
        self.gaps.iter().filter(|gap| **gap <= 40.0).count() as f64 / self.gaps.len() as f64 * 100.0
    }

    #[must_use]
    pub fn longest_gap(&self) -> f64 {
        self.gaps.iter().copied().fold(0.0, f64::max)
    }
}

/// Seconds the drill spends chipping out one tile of `rock` (`toughness` times its durability)
/// at drill tier `drill`, by the chip formula (`world/commands.rs`).
fn chip_seconds(rock: TileKind, toughness: f64, drill: u8) -> f64 {
    let durability = f64::from(tile_durability(rock)) * toughness;
    let chips = (durability / f64::from(drill_bite(drill))).ceil();
    let period = (f64::from(crate::world::base_drill_seconds_per_chip(rock))
        / f64::from(drill_speed(drill)))
    .max(f64::from(MIN_DRILL_SECONDS_PER_CHIP));
    chips * period
}

/// Seconds a rig of drill tier `drill` takes per tile of `stratum` (slower with a drill under
/// its rock's tier), plus `overhead` for moving onto the next tile.
fn tile_seconds(stratum: &DescentStratum, drill: u8, overhead: f64) -> f64 {
    let slowdown = 2.0_f64.powf(f64::from(stratum.rock_tier.saturating_sub(drill)) / 3.0);
    chip_seconds(stratum.rock, stratum.toughness, drill).mul_add(slowdown, overhead)
}

/// Seconds per row a rig of drill tier `drill` takes to drill straight down `rows` rows of hard
/// rock, measured through the real tick (`None` if it could not cut it).
#[must_use]
pub fn measure_dig(drill: u8, rows: i32, tick_rate_hz: u32) -> Option<f64> {
    let player = PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(player, SimulationParams::new(tick_rate_hz));
    let column = 30;
    let top = 60;
    for y in top - 3..top + rows + 3 {
        for x in column - 2..=column + 2 {
            let kind = if y < top && x == column {
                TileKind::Air
            } else {
                TileKind::HardRock
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    {
        let rig = world.player_mut(player)?;
        rig.drill_strength = drill;
        rig.engine_level = drill.min(MAX_UPGRADE_LEVEL);
        rig.fuel = rig.fuel_capacity;
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (top as f32 - 0.5) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    let target = top + rows - 1;
    let limit = u64::from(tick_rate_hz) * 600;
    let drill_down = ControlState::new(0.0, false, true);
    let mut ticks = 0_u64;
    while ticks < limit {
        let inputs = crate::TickInputs::default().hold_as(player, drill_down);
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        if let Some(rig) = world.player_mut(player) {
            rig.fuel = rig.fuel_capacity;
            rig.hull = rig.max_hull();
        }
        ticks += 1;
        if world.player(player)?.tile_position(TILE_SIZE).y >= target {
            return Some(ticks as f64 / f64::from(tick_rate_hz) / f64::from(rows - 1));
        }
    }
    None
}

/// The tick-measured seconds per row a rig spends besides chipping (falling onto the next
/// tile, the drill biting again), the median over a few drill tiers.
fn dig_overhead(config: &DescentConfig) -> f64 {
    if !config.calibrate {
        return DEFAULT_DIG_OVERHEAD;
    }
    let mut extra: Vec<f64> = [4_u8, 8, 12, 15]
        .iter()
        .filter_map(|drill| {
            measure_dig(*drill, 24, config.tick_rate_hz)
                .map(|measured| (measured - chip_seconds(TileKind::HardRock, 1.0, *drill)).max(0.0))
        })
        .collect();
    extra.sort_by(f64::total_cmp);
    extra
        .get(extra.len() / 2)
        .copied()
        .unwrap_or(DEFAULT_DIG_OVERHEAD)
}

/// The bot's rig and purse.
#[derive(Clone, Debug)]
struct Rig {
    drill: u8,
    tank: u8,
    bay: u8,
    engine: u8,
    hull: u8,
    radiator: u8,
    systems: u8,
    credits: f64,
}

impl Rig {
    const fn level(&self, kind: UpgradeKind) -> u8 {
        match kind {
            UpgradeKind::Drill => self.drill,
            UpgradeKind::FuelTank => self.tank,
            UpgradeKind::CargoBay => self.bay,
            UpgradeKind::Engine => self.engine,
            UpgradeKind::Hull => self.hull,
            _ => self.radiator,
        }
    }

    const fn level_mut(&mut self, kind: UpgradeKind) -> &mut u8 {
        match kind {
            UpgradeKind::Drill => &mut self.drill,
            UpgradeKind::FuelTank => &mut self.tank,
            UpgradeKind::CargoBay => &mut self.bay,
            UpgradeKind::Engine => &mut self.engine,
            UpgradeKind::Hull => &mut self.hull,
            _ => &mut self.radiator,
        }
    }
}

/// What one mining trip from `base_row` to a face at `face_row` in `stratum` yields.
struct Trip {
    seconds: f64,
    credits: f64,
    fuel: f64,
    damage: f64,
}

fn trip(rig: &Rig, stratum: &DescentStratum, base_row: i32, face_row: i32, overhead: f64) -> Trip {
    let distance = f64::from((face_row - base_row).max(0));
    let climb = STOCK_CLIMB_ROWS_PER_SECOND
        * f64::from(engine_power(rig.engine).min(crate::player::ENGINE_CLIMB_CAP));
    let travel_seconds = distance / FALL_ROWS_PER_SECOND + distance / climb;
    let cargo = f64::from(cargo_capacity_for_level(rig.bay));
    let tiles = cargo / stratum.ore_share;
    let dig_seconds = tiles * tile_seconds(stratum, rig.drill, overhead);
    let burn = fuel_burn_efficiency(rig.tank);
    let fuel = tiles * DIG_FUEL_PER_TILE
        + (distance / climb + dig_seconds * THRUST_SHARE_MINING)
            * f64::from(AUTHORITATIVE_FUEL_BURN_PER_SECOND)
            * f64::from(burn);
    let mining_minutes = dig_seconds / 60.0;
    let guard = 0.88_f64.powi(i32::from(rig.hull) - i32::from(stratum.rock_tier));
    Trip {
        seconds: travel_seconds + dig_seconds + SERVICE_SECONDS,
        credits: cargo * stratum.ore_value,
        fuel,
        damage: stratum.hazard_per_minute * mining_minutes * guard,
    }
}

/// Plays the descent through `registry`'s strata under `config`.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the bot's whole run in order")]
pub fn run_descent(registry: &dyn StratumRegistry, config: &DescentConfig) -> DescentRun {
    let rules = ProgressionRules::Descent;
    let mut strata = registry.strata();
    let descents = registry.descents();
    if let Some(limit) = config.strata_limit {
        strata.truncate(limit);
    }
    let overhead = dig_overhead(config);
    let mut rig = Rig {
        drill: 1,
        tank: 1,
        bay: 1,
        engine: 1,
        hull: 1,
        radiator: 1,
        systems: 0,
        credits: 0.0,
    };
    let mut clock_minutes = 0.0_f64;
    let mut last_novelty = 0.0_f64;
    let mut gaps = Vec::new();
    let mut runs = Vec::new();
    let mut descent_hours = vec![0.0; DESCENT_COUNT];
    let mut expedition = vec![0.0; DESCENT_COUNT];
    let mut systems = Vec::new();
    let mut beacon_rows: Vec<i32> = Vec::new();
    let mut claimed: Vec<u8> = Vec::new();
    let mut deaths_total = 0;
    let note = |at: f64, last: &mut f64, gaps: &mut Vec<f64>| {
        gaps.push(at - *last);
        *last = at;
    };
    for (index, stratum) in strata.iter().enumerate() {
        let start = clock_minutes;
        let drill_tier = rig.drill;
        let hull_tier = rig.hull;
        let descent = stratum.descent;
        // Arrival is new; the rest of its content is spread over its time below.
        note(clock_minutes, &mut last_novelty, &mut gaps);
        let outpost_row = descents
            .get(usize::from(descent))
            .map_or(0, |span| if descent == 0 { 0 } else { span.top + 40 });
        // The stratum's content minutes, and digging through it.
        let rows = f64::from(stratum.rows());
        let dig_minutes = rows * tile_seconds(stratum, rig.drill, overhead) / 60.0;
        let mut minutes = stratum.content_minutes + dig_minutes;
        // The Descent's ancient lift restored once (it shortens the trips back up).
        if index > 0 && strata[index - 1].descent != descent {
            rig.credits -=
                f64::from(scaled_price(ANCIENT_LIFT_RESTORE_PRICE, descent)).min(rig.credits);
        }
        // What the next stratum asks for.
        let next = strata.get(index + 1);
        // A rig rated for the next stratum: a drill for its rock, a hull for its hazards, a
        // radiator for its heat, a bay, tank and engine for its longer trips, and the gate
        // system when it opens a new Descent.
        let needs = next.map_or((0, 0, 0, 0, None), |next| {
            let gate = if next.descent == descent {
                None
            } else {
                descent_gate(next.descent)
            };
            (
                next.rock_tier.saturating_sub(1),
                next.rock_tier.saturating_sub(2),
                next.heat_tier,
                next.rock_tier.saturating_sub(3),
                gate,
            )
        });
        let cap = core_tier_cap(descent);
        // The bot mines the upper part of the stratum it is in.
        let face = stratum.top + stratum.rows() / 3;
        let mut trips = 0_u32;
        let mut deaths = 0_u32;
        let mut purchases = 0_u32;
        let mut everyday = 0.0_f64;
        let mut stalled = 0_u32;
        // Mine until the next stratum's needs are bought (and a comfort tier or two).
        for _ in 0..4_000 {
            // A beacon whenever the next one is affordable: trips start near the face.
            let next_beacon = beacon_rows.last().copied().unwrap_or(0) + BEACON_SPACING_ROWS;
            let beacon_price = f64::from(scaled_price(BEACON_PRICE, descent));
            if next_beacon <= stratum.bottom && rig.credits >= beacon_price {
                rig.credits -= beacon_price;
                beacon_rows.push(next_beacon);
            }
            let base = beacon_rows
                .iter()
                .copied()
                .filter(|row| *row <= face)
                .max()
                .unwrap_or(0);
            // Sell and refuel at the crew's outpost once claimed (a short jump), else in town
            // (the jump home): the town pays in full, an outpost keeps its cut.
            let at_outpost = claimed.contains(&descent) && outpost_row <= face;
            let (sale, jump) = if at_outpost {
                (
                    f64::from(OUTPOST_SALE_PERCENT[1]) / 100.0,
                    (base - outpost_row).abs(),
                )
            } else {
                (1.0, base)
            };
            let tank = f64::from(fuel_capacity_for_level(rig.tank));
            let preview = trip(&rig, stratum, base, face, overhead);
            // The tank must carry an everyday trip with a reserve.
            let tank_short = preview.fuel > tank * 0.6;
            let lacks = |rig: &Rig| {
                let (drill, hull, heat, haul, gate) = needs;
                let mut core: Vec<UpgradeKind> = [
                    (UpgradeKind::Drill, drill),
                    (UpgradeKind::Hull, hull),
                    (UpgradeKind::Radiator, heat),
                    (UpgradeKind::CargoBay, haul),
                    (UpgradeKind::FuelTank, haul),
                    (UpgradeKind::Engine, haul),
                ]
                .into_iter()
                .filter(|(kind, tier)| rig.level(*kind) < (*tier).min(cap))
                .map(|(kind, _)| kind)
                .collect();
                if tank_short && rig.tank < cap && !core.contains(&UpgradeKind::FuelTank) {
                    core.push(UpgradeKind::FuelTank);
                }
                let system = gate.filter(|system| rig.systems & system.bit() == 0);
                (core, system)
            };
            // Buy what is needed, cheapest first, then comfort lines while the purse is fat.
            loop {
                let (core, system) = lacks(&rig);
                if let Some(system) = system
                    && core.is_empty()
                {
                    let price = f64::from(system.price());
                    if rig.credits >= price {
                        rig.credits -= price;
                        rig.systems |= system.bit();
                        systems.push((system, (clock_minutes + minutes) / 60.0));
                        purchases += 1;
                        continue;
                    }
                    break;
                }
                let essential = !core.is_empty();
                let wanted = core
                    .into_iter()
                    .chain(
                        [
                            UpgradeKind::CargoBay,
                            UpgradeKind::FuelTank,
                            UpgradeKind::Engine,
                        ]
                        .into_iter()
                        .filter(|kind| rig.level(*kind) < rig.drill.min(cap)),
                    )
                    .min_by_key(|kind| upgrade_cost_for(*kind, rig.level(*kind), rules));
                let Some(kind) = wanted else {
                    break;
                };
                let price = f64::from(upgrade_cost_for(kind, rig.level(kind), rules));
                if rig.credits >= price && (essential || rig.credits >= price * 1.5) {
                    rig.credits -= price;
                    *rig.level_mut(kind) += 1;
                    purchases += 1;
                } else {
                    break;
                }
            }
            if !claimed.contains(&descent) && outpost_row <= face && descent > 0 {
                let price = f64::from(scaled_price(OUTPOST_CLAIM_PRICE, descent));
                if rig.credits >= price * 3.0 {
                    rig.credits -= price;
                    claimed.push(descent);
                }
            }
            let (core, system) = lacks(&rig);
            if core.is_empty() && system.is_none() && trips > 0 {
                break;
            }
            // One mining trip.
            let run = trip(&rig, stratum, base, face, overhead);
            trips += 1;
            everyday = everyday.max(run.fuel / tank * 100.0);
            let hull = f64::from(hull_capacity_for_level(rig.hull));
            // Two jumps on the beacon network (out and back), and the fuel to refill.
            let (jump_fuel, jump_credits) = if jump > 0 {
                let (fuel, credits) = travel_cost(jump);
                (f64::from(fuel) * 2.0, f64::from(credits) * 2.0)
            } else {
                (0.0, 0.0)
            };
            let refuel = (run.fuel + jump_fuel) * f64::from(OUTPOST_FUEL_UNIT[1]);
            let wrecked = run.damage > hull * 0.8;
            let gain = if wrecked {
                deaths += 1;
                let fee = (50.0 + 3.0 * f64::from(face)).min(rig.credits);
                run.credits * sale * 0.5 - fee - refuel
            } else {
                run.credits * sale - refuel - jump_credits
            };
            rig.credits = (rig.credits + gain).max(0.0);
            if gain <= 0.0 {
                stalled += 1;
                if stalled > 40 {
                    break;
                }
            }
            // A wreck teaches: hull first.
            if wrecked && rig.hull < cap {
                let price = f64::from(upgrade_cost_for(UpgradeKind::Hull, rig.hull, rules));
                if rig.credits >= price {
                    rig.credits -= price;
                    rig.hull += 1;
                    purchases += 1;
                }
            }
            minutes += run.seconds / 60.0;
        }
        // Spread the stratum's new things over its time.
        let firsts = f64::from(stratum.novelties.saturating_sub(1)) + f64::from(purchases).min(4.0);
        if firsts > 0.0 {
            for step in 1..=(firsts as u32) {
                let at = start + minutes * f64::from(step) / (firsts + 1.0);
                note(at, &mut last_novelty, &mut gaps);
            }
        }
        clock_minutes += minutes;
        deaths_total += deaths;
        let longest = gaps
            .iter()
            .rev()
            .take(firsts as usize + 1)
            .copied()
            .fold(0.0, f64::max);
        let leaving_descent = strata
            .get(index + 1)
            .is_none_or(|next| next.descent != descent);
        if leaving_descent {
            descent_hours[usize::from(descent)] = clock_minutes / 60.0;
            // A Descent's gate set piece.
            note(clock_minutes, &mut last_novelty, &mut gaps);
            if let Some(span) = descents.get(usize::from(descent)) {
                let rows = f64::from(span.rows() - 40);
                let climb = STOCK_CLIMB_ROWS_PER_SECOND
                    * f64::from(engine_power(rig.engine).min(crate::player::ENGINE_CLIMB_CAP));
                let fuel = (rows / climb + 120.0)
                    * f64::from(AUTHORITATIVE_FUEL_BURN_PER_SECOND)
                    * f64::from(fuel_burn_efficiency(rig.tank))
                    + 300.0 * DIG_FUEL_PER_TILE;
                expedition[usize::from(descent)] =
                    fuel / f64::from(fuel_capacity_for_level(rig.tank)) * 100.0;
            }
        }
        runs.push(StratumRun {
            stratum: stratum.clone(),
            minutes,
            end_hours: clock_minutes / 60.0,
            trips,
            deaths,
            drill_tier,
            hull_tier,
            everyday_fuel_percent: everyday,
            longest_gap_minutes: longest,
            purchases,
        });
    }
    DescentRun {
        strata: runs,
        descent_hours,
        expedition_fuel_percent: expedition,
        systems,
        gaps,
        dig_overhead: overhead,
        deaths: deaths_total,
    }
}

/// §14's target hours at the end of each Descent.
pub const TARGET_HOURS: [(f64, f64); DESCENT_COUNT] = [
    (5.0, 8.0),
    (13.0, 18.0),
    (22.0, 29.0),
    (31.0, 40.0),
    (42.0, 53.0),
    (53.0, 66.0),
    (66.0, 82.0),
    (75.0, 90.0),
];

/// Builds the report section for `run`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the section's tables and notes in order"
)]
pub fn descent_section(run: &DescentRun) -> ReportSection {
    let mut section = ReportSection::new(
        "Descent (Part III F10)",
        format!(
            "A bot plays the Deep Descent through the stratum registry's 41 strata (the default \
             seed's V11 layout) on the Descent ladders, rig systems and logistics prices. The drill moves \
             on to the next tile in {:.2} s besides chipping (measured through the tick). Whole run: {:.1} h (target 60-90 h); {:.0}% of \
             the gaps between new things are 40 minutes or less (longest {:.0} min); {} deaths.",
            run.dig_overhead,
            run.total_hours(),
            run.gaps_within_40(),
            run.longest_gap(),
            run.deaths
        ),
    );
    let mut per_descent = Table::new(
        "Per Descent: hours at its end against the plan's target, and fuel on everyday trips against an expedition",
        &[
            "Descent",
            "Ends at (h)",
            "Target (h)",
            "Everyday trip fuel (max % of tank)",
            "Expedition fuel (% of tank)",
            "Deaths",
        ],
    );
    for (index, span) in PlanLayout.descents().iter().enumerate() {
        let runs: Vec<&StratumRun> = run
            .strata
            .iter()
            .filter(|stratum| usize::from(stratum.stratum.descent) == index)
            .collect();
        if runs.is_empty() {
            continue;
        }
        let everyday = runs
            .iter()
            .map(|stratum| stratum.everyday_fuel_percent)
            .fold(0.0, f64::max);
        let deaths: u32 = runs.iter().map(|stratum| stratum.deaths).sum();
        let (low, high) = TARGET_HOURS[index];
        per_descent.row(vec![
            format!("{} {}", span.numeral(), span.name),
            format!("{:.1}", run.descent_hours[index]),
            format!("{low:.0}-{high:.0}"),
            format!("{everyday:.0}"),
            format!("{:.0}", run.expedition_fuel_percent[index]),
            deaths.to_string(),
        ]);
        section.metric(
            format!("descent.d{index}.end_hours"),
            run.descent_hours[index],
        );
        section.metric(format!("descent.d{index}.everyday_fuel_percent"), everyday);
        section.metric(
            format!("descent.d{index}.expedition_fuel_percent"),
            run.expedition_fuel_percent[index],
        );
    }
    section.tables.push(per_descent);
    let mut per_stratum = Table::new(
        "Per stratum",
        &[
            "Stratum",
            "Rows",
            "Minutes",
            "Leaves at (h)",
            "Drill / hull on arrival",
            "Trips",
            "Deaths",
            "Longest gap (min)",
        ],
    );
    for stratum in &run.strata {
        per_stratum.row(vec![
            format!(
                "{} {}",
                crate::depth_progression::numeral(stratum.stratum.descent),
                stratum.stratum.name
            ),
            stratum.stratum.rows().to_string(),
            format!("{:.0}", stratum.minutes),
            format!("{:.1}", stratum.end_hours),
            format!("{} / {}", stratum.drill_tier, stratum.hull_tier),
            stratum.trips.to_string(),
            stratum.deaths.to_string(),
            format!("{:.0}", stratum.longest_gap_minutes),
        ]);
    }
    section.tables.push(per_stratum);
    let mut systems = String::new();
    for (system, hours) in &run.systems {
        let _ = write!(systems, "{} at {hours:.1} h; ", system.name());
    }
    if !systems.is_empty() {
        section
            .notes
            .push(format!("Gate systems bought: {systems}"));
    }
    let long: Vec<String> = run
        .strata
        .iter()
        .filter(|stratum| stratum.longest_gap_minutes > 40.0)
        .map(|stratum| stratum.stratum.name.to_owned())
        .collect();
    section.notes.push(if long.is_empty() {
        "Every stratum brings something new within 40 minutes.".to_owned()
    } else {
        format!("Strata with a gap over 40 minutes: {}.", long.join(", "))
    });
    section.metric("descent.total_hours", run.total_hours());
    section.metric("descent.gaps_within_40_percent", run.gaps_within_40());
    section.metric("descent.longest_gap_minutes", run.longest_gap());
    section.metric("descent.deaths", f64::from(run.deaths));
    section.metric("descent.dig_overhead", run.dig_overhead);
    section
}

/// The descent probe (`--only descent`).
pub struct DescentProbe;

impl BalanceProbe for DescentProbe {
    fn name(&self) -> &'static str {
        "descent"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let config = DescentConfig {
            tick_rate_hz: context.config.tick_rate_hz,
            ..DescentConfig::default()
        };
        // The real stratum registry on the default seed's Deep Descent world (Wave 7
        // integration); `PlanStrata` stays for comparison and tests.
        descent_section(&run_descent(&RegistryStrata::default_world(), &config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_registry_covers_41_strata_to_the_heart() {
        let strata = PlanStrata.strata();
        assert_eq!(strata.len(), crate::depth_progression::STRATUM_COUNT);
        assert_eq!(strata.last().map(|stratum| stratum.bottom), Some(13_000));
        for pair in strata.windows(2) {
            assert!(pair[1].rock_tier >= pair[0].rock_tier);
            assert!(
                pair[1].ore_value >= pair[0].ore_value * 0.99,
                "{}",
                pair[1].name
            );
        }
    }

    #[test]
    fn the_real_registry_covers_41_seeded_strata_to_the_heart() {
        let registry = RegistryStrata::default_world();
        let strata = registry.strata();
        assert_eq!(strata.len(), crate::depth_progression::STRATUM_COUNT);
        assert_eq!(strata[0].top, 0);
        assert_eq!(strata[0].name, "Topsoil");
        assert_eq!(strata.last().map(|stratum| stratum.name), Some("The Heart"));
        let bottom = strata.last().map_or(0, |stratum| stratum.bottom);
        assert!((12_900..=13_100).contains(&bottom), "{bottom}");
        for pair in strata.windows(2) {
            assert_eq!(pair[0].bottom, pair[1].top, "contiguous");
            assert!(pair[1].descent >= pair[0].descent);
        }
        assert!(strata.iter().all(|stratum| stratum.ore_value > 0.0));
        let descents = registry.descents();
        assert_eq!(descents.len(), DESCENT_COUNT);
        assert_eq!(descents.last().map(|span| span.bottom), Some(bottom));
        // A short run on it.
        let config = DescentConfig {
            strata_limit: Some(6),
            calibrate: false,
            tick_rate_hz: 60,
        };
        let run = run_descent(&registry, &config);
        assert_eq!(run.strata.len(), 6);
        assert!(run.total_hours() > 1.0);
    }

    #[test]
    fn a_short_descent_runs_and_reports() {
        let config = DescentConfig {
            strata_limit: Some(8),
            calibrate: false,
            tick_rate_hz: 60,
        };
        let run = run_descent(&PlanStrata, &config);
        assert_eq!(run.strata.len(), 8);
        assert!(run.total_hours() > 1.0, "{} h", run.total_hours());
        assert!(run.strata.iter().all(|stratum| stratum.minutes > 0.0));
        // Upgrades happen on the way down.
        let last = run.strata.last().expect("stratum");
        assert!(last.drill_tier > 1);
        let section = descent_section(&run);
        assert!(section.metrics.contains_key("descent.total_hours"));
        assert!(section.tables.len() >= 2);
        // Same config, same run.
        assert_eq!(run_descent(&PlanStrata, &config), run);
    }

    #[test]
    fn the_tick_measures_the_drill_close_to_its_formula() {
        let measured = measure_dig(10, 12, 60).expect("cuts hard rock");
        let formula = chip_seconds(TileKind::HardRock, 1.0, 10);
        assert!(
            measured >= formula * 0.9 && measured < formula + 1.0,
            "{measured} vs {formula}"
        );
    }

    /// `cargo test --release -p fatherload_simulation descent_report -- --ignored --nocapture`
    #[test]
    #[ignore = "prints the full descent report"]
    fn descent_report() {
        let run = run_descent(&RegistryStrata::default_world(), &DescentConfig::default());
        let report = super::super::BalanceReport {
            header: "Descent probe".to_owned(),
            sections: vec![descent_section(&run)],
        };
        println!("{}", report.to_markdown());
    }
}
