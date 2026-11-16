//! The second-act probe (wave 4): what the sky, the Moon, the pilot's kit and a home cost, and
//! where the climb gates sit, so Act II pacing reads at a glance.
//!
//! It is arithmetic over the shop's price ladder ([`upgrade_cost`]) and T16's measured climb
//! table ([`crate::climb`]), so it is instant and seed-independent:
//!
//! - **Climb gates**: the cheapest fuel tank, engine and altitude intake (credits from stock)
//!   whose full-tank climb reaches the low isles ([`SkyBands::island_reach_rows`] of the current generator's sky) and crosses
//!   the dead-air gap ([`SkyBands::dead_air_reach_rows`], the Act II gate), and the Moon
//!   checklist ([`moon::MoonRequirement`]: top tank, intakes, de-icer, the pressurized hull's
//!   vacuum seal and life support).
//! - **Sky refuels**: how far a sky town tops up rigs just short of the gap
//!   ([`milestone::sky_refuel_level`]), the honest-gate rule.
//! - **Act II purchases**: the Act II rig weapons (credits plus sky materials), the Moon
//!   colony's weapons, the pilot's Moon kit (a thermal lining that covers the Moon's cold, a
//!   suit and a jetpack) and homes (a cabin on a meadow, a sky villa, a lunar habitat).
//!
//! With the economy runs cached by the report, every credit figure is also shown as minutes
//! of late-game mining (the economy bot's earning rate over its last third).

use super::{BalanceProbe, ProbeContext, ReportSection, Table, grouped};
use crate::{
    climb::{ENGINE_TIERS, INTAKE_TIERS, TANK_TIERS, ceiling},
    economy::{UpgradeKind, upgrade_cost},
    home::{Blueprint, PlotSetting, PlotSize, plot::list_price},
    milestone, moon,
    pilot::PilotGearKind,
    player::{Player, fuel_capacity_for_level},
    weapon::WeaponKind,
    worldgen::{GeneratorVersion, sky::SkyBands},
};

/// The bands of the current generator's sky: the worlds the second act is played on.
const BANDS: SkyBands = match GeneratorVersion::CURRENT.sky_bands() {
    Some(bands) => bands,
    None => panic!("the current generator has a sky"),
};

/// Credits to raise `kind` from its stock level to `tier`.
#[must_use]
pub fn line_cost(kind: UpgradeKind, tier: u8) -> u32 {
    (kind.starting_level()..tier)
        .map(|level| upgrade_cost(kind, level))
        .sum()
}

/// A climb loadout and what it costs from stock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClimbLoadout {
    pub tank: u8,
    pub engine: u8,
    pub intake: u8,
    /// Rows above row 0 a full tank reaches (the climb table).
    pub reach: i32,
    /// Credits for the tank, engine and intake tiers from stock.
    pub credits: u32,
}

/// The cheapest loadout of the climb table whose full-tank climb reaches `rows`.
#[must_use]
pub fn cheapest_reaching(rows: i32) -> Option<ClimbLoadout> {
    let mut best: Option<ClimbLoadout> = None;
    for tank in TANK_TIERS {
        for engine in ENGINE_TIERS {
            for intake in INTAKE_TIERS {
                let reach = ceiling(tank, engine, intake);
                if reach < rows {
                    continue;
                }
                let credits = line_cost(UpgradeKind::FuelTank, tank)
                    + line_cost(UpgradeKind::Engine, engine)
                    + line_cost(UpgradeKind::AltitudeIntake, intake);
                if best.is_none_or(|found| credits < found.credits) {
                    best = Some(ClimbLoadout {
                        tank,
                        engine,
                        intake,
                        reach,
                        credits,
                    });
                }
            }
        }
    }
    best
}

/// The Moon checklist's credits from stock (top tank, intakes, de-icer, the vacuum seal and the
/// first life-support tier), with the cheapest engine that crosses the gap on that tank.
#[must_use]
pub fn moon_checklist_credits() -> u32 {
    let engine = ENGINE_TIERS
        .into_iter()
        .find(|engine| {
            ceiling(moon::MOON_FUEL_TANK_TIER, *engine, INTAKE_TIERS[3])
                >= BANDS.dead_air_reach_rows()
        })
        .unwrap_or(ENGINE_TIERS[ENGINE_TIERS.len() - 1]);
    line_cost(UpgradeKind::FuelTank, moon::MOON_FUEL_TANK_TIER)
        + line_cost(UpgradeKind::Engine, engine)
        + line_cost(UpgradeKind::AltitudeIntake, moon::MOON_INTAKE_TIER)
        + line_cost(UpgradeKind::DeIcer, moon::MOON_DE_ICER_TIER)
        + line_cost(UpgradeKind::PressurizedHull, moon::VACUUM_SEAL_TIER)
        + line_cost(UpgradeKind::LifeSupport, 1)
}

/// Credits for `kind`'s pilot gear tiers from none up to `tier`.
#[must_use]
pub fn gear_cost(kind: PilotGearKind, tier: u8) -> u32 {
    (0..tier).filter_map(|owned| kind.price(owned)).sum()
}

/// The thermal lining tier whose cover keeps out the Moon's cold
/// ([`crate::environment::MAX_COLD`]).
#[must_use]
pub fn moon_thermal_tier() -> u8 {
    (0..=crate::pilot::MAX_THERMAL_TIER)
        .find(|tier| {
            crate::pilot::BASE_COLD_COVER + tier * crate::pilot::COLD_COVER_PER_THERMAL
                >= crate::environment::MAX_COLD
        })
        .unwrap_or(crate::pilot::MAX_THERMAL_TIER)
}

/// One row of the purchases table: a name, credits and what else it costs.
fn purchase(name: &str, credits: u32, extra: &str, per_minute: Option<f64>) -> Vec<String> {
    vec![
        name.to_owned(),
        grouped(f64::from(credits)),
        extra.to_owned(),
        per_minute.map_or_else(
            || "-".to_owned(),
            |rate| format!("{:.0}", f64::from(credits) / rate.max(1.0)),
        ),
    ]
}

/// The Act II pacing section.
pub struct SecondActProbe;

impl BalanceProbe for SecondActProbe {
    fn name(&self) -> &'static str {
        "second-act"
    }

    #[allow(clippy::too_many_lines, reason = "one table after another")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let mut section = ReportSection::new(
            "Second act: sky, Moon, pilot and home costs",
            "Arithmetic over the shop's price ladder and the measured climb table (instant, \
             seed-independent). \"Minutes\" divide a price by the economy bot's late-game \
             earning rate (its last third), a feel for how long a player saves for it.",
        );
        let late_rate = {
            let runs = context.economy();
            let rates: Vec<f64> = runs
                .iter()
                .map(|run| run.earned_between(run.minutes * 2 / 3, run.minutes))
                .filter(|rate| *rate > 0.0)
                .collect();
            (!rates.is_empty()).then(|| super::mean(&rates))
        };
        if let Some(rate) = late_rate {
            section.metric("second_act.late_credits_per_minute", rate);
        }

        let mut gates = Table::new(
            "Cheapest climb loadout (credits from stock) per gate:",
            &[
                "Gate",
                "Rows",
                "Tank / engine / intake",
                "Reach",
                "Credits",
                "Minutes",
            ],
        );
        for (name, key, rows) in [
            ("Low isles", "isles", BANDS.island_reach_rows()),
            ("Dead-air gap", "dead_air", BANDS.dead_air_reach_rows()),
        ] {
            let Some(loadout) = cheapest_reaching(rows) else {
                gates.row(vec![
                    name.to_owned(),
                    rows.to_string(),
                    "unreachable".to_owned(),
                ]);
                continue;
            };
            section.metric(
                format!("second_act.{key}.credits"),
                f64::from(loadout.credits),
            );
            section.metric(format!("second_act.{key}.tank"), f64::from(loadout.tank));
            gates.row(vec![
                name.to_owned(),
                grouped(f64::from(rows)),
                format!("{} / {} / {}", loadout.tank, loadout.engine, loadout.intake),
                grouped(f64::from(loadout.reach)),
                grouped(f64::from(loadout.credits)),
                late_rate.map_or_else(
                    || "-".to_owned(),
                    |rate| format!("{:.0}", f64::from(loadout.credits) / rate.max(1.0)),
                ),
            ]);
        }
        let moon_credits = moon_checklist_credits();
        section.metric("second_act.moon.credits", f64::from(moon_credits));
        gates.row(vec![
            "The Moon (checklist)".to_owned(),
            "-".to_owned(),
            format!(
                "{} / - / {} + de-icer {}, hull {}, life support 1",
                moon::MOON_FUEL_TANK_TIER,
                moon::MOON_INTAKE_TIER,
                moon::MOON_DE_ICER_TIER,
                moon::VACUUM_SEAL_TIER
            ),
            "-".to_owned(),
            grouped(f64::from(moon_credits)),
            late_rate.map_or_else(
                || "-".to_owned(),
                |rate| format!("{:.0}", f64::from(moon_credits) / rate.max(1.0)),
            ),
        ]);
        section.tables.push(gates);

        // Sky refuels for rigs just short of the gap, from the highest sky town's altitude.
        let mut refuels = Table::new(
            "Sky town refuel ceiling (share of the tank) for rigs that cannot cross the gap:",
            &[
                "Tank / engine / intake",
                "Reach",
                "At 3,000 rows",
                "At 4,400 rows",
            ],
        );
        for (tank, engine, intake) in [(6, 8, 0), (10, 12, 4), (12, 12, 6), (13, 15, 6)] {
            let mut rig = Player::new(0.0, 0.0);
            rig.fuel_tank_level = tank;
            rig.engine_level = engine;
            rig.hazard_gear.altitude_intake = intake;
            rig.fuel_capacity = fuel_capacity_for_level(tank);
            let share = |rows: i32| {
                format!(
                    "{:.0} %",
                    milestone::sky_refuel_level(&rig, rows, Some(&BANDS)) / rig.fuel_capacity
                        * 100.0
                )
            };
            refuels.row(vec![
                format!("{tank} / {engine} / {intake}"),
                grouped(f64::from(milestone::climb_reach(&rig))),
                share(3_000),
                share(4_400),
            ]);
        }
        section.tables.push(refuels);

        let mut purchases = Table::new(
            "Act II purchases:",
            &["Purchase", "Credits", "Also", "Minutes"],
        );
        for weapon in WeaponKind::ALL
            .into_iter()
            .filter(|weapon| weapon.act_two())
        {
            let materials = weapon
                .materials()
                .iter()
                .map(|(kind, count)| format!("{count} {}", kind.name()))
                .collect::<Vec<_>>()
                .join(", ");
            purchases.row(purchase(
                weapon.name(),
                weapon.cost(),
                &materials,
                late_rate,
            ));
        }
        for weapon in [WeaponKind::LunarCutter, WeaponKind::LowGMortar] {
            let price = moon::weapon_price(weapon);
            purchases.row(purchase(
                weapon.name(),
                price.credits,
                &format!("{} helium-3, {} alien alloy", price.helium, price.alloy),
                late_rate,
            ));
        }
        let thermal = moon_thermal_tier();
        let kit = gear_cost(PilotGearKind::Thermal, thermal)
            + gear_cost(PilotGearKind::Suit, 2)
            + gear_cost(PilotGearKind::Jetpack, 1);
        section.metric("second_act.pilot_moon_kit.credits", f64::from(kit));
        purchases.row(purchase(
            "Pilot's Moon kit",
            kit,
            &format!("thermal lining {thermal}, suit 2, jetpack 1"),
            late_rate,
        ));
        for (name, size, setting, blueprint) in [
            (
                "Cabin on a meadow",
                PlotSize::Small,
                PlotSetting::Meadow,
                Blueprint::Cabin,
            ),
            (
                "Sky villa",
                Blueprint::SkyVilla.min_plot(),
                PlotSetting::SkyIsland,
                Blueprint::SkyVilla,
            ),
            (
                "Lunar habitat",
                Blueprint::LunarHabitat.min_plot(),
                PlotSetting::Moon,
                Blueprint::LunarHabitat,
            ),
        ] {
            let credits = list_price(size, setting) + blueprint.job().price;
            purchases.row(purchase(
                name,
                credits,
                &format!(
                    "plot {} + build",
                    grouped(f64::from(list_price(size, setting)))
                ),
                late_rate,
            ));
        }
        section.tables.push(purchases);
        section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act_two_gates_climb_in_price_and_the_moon_is_the_summit() {
        let isles = cheapest_reaching(BANDS.island_reach_rows()).expect("the isles");
        let gap = cheapest_reaching(BANDS.dead_air_reach_rows()).expect("the gap");
        let moon = moon_checklist_credits();
        assert!(isles.credits < gap.credits, "{isles:?} {gap:?}");
        assert!(gap.credits < moon, "{gap:?} {moon}");
        // The isles are an early hop; the gap and the Moon are late-game goals, but the Moon is
        // not an absurd grind on top of the gap (the hull and life support are its new costs).
        assert!(isles.tank <= 4, "{isles:?}");
        assert!(gap.tank >= 10, "{gap:?}");
        assert!(moon < gap.credits * 6, "{gap:?} {moon}");
        // A pilot can walk the Moon with a thermal lining it can buy.
        assert!(moon_thermal_tier() <= crate::pilot::MAX_THERMAL_TIER);
        assert!(
            crate::pilot::BASE_COLD_COVER
                + moon_thermal_tier() * crate::pilot::COLD_COVER_PER_THERMAL
                >= crate::environment::MAX_COLD
        );
    }

    #[test]
    fn sky_refuels_stay_below_a_full_tank_for_rigs_short_of_the_gap() {
        let mut rig = Player::new(0.0, 0.0);
        rig.fuel_tank_level = 10;
        rig.engine_level = 12;
        rig.hazard_gear.altitude_intake = 4;
        rig.fuel_capacity = fuel_capacity_for_level(10);
        assert!(!milestone::crosses_dead_air(&rig, &BANDS));
        assert!(milestone::sky_refuel_level(&rig, 4_400, Some(&BANDS)) < rig.fuel_capacity);
    }
}
