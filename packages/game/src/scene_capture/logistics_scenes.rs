//! Depth logistics scenes (Part III F10): an ancient lift's cab carrying a rig up its rail, a
//! deep outpost the crew claimed and its counter, the travel network from a depth beacon, a
//! built lift on a dug shaft, a dive charge's plunge, and the upgrade shop of a Descent world
//! with the rig systems on its shelves.
//!
//! The lifts, outposts and beacons go in through the generator hook
//! ([`WorldState::install_logistics_sites`], stamping their tiles) and real
//! [`PlayerCommand::Logistics`] commands through the fixed tick, so the scenes also prove the
//! rules accept a real layout.

use fatherload_simulation::{
    PlayerCommand, PlayerId, TickInputs, WorldState,
    depth_progression::{ProgressionRules, RigSystem},
    economy::{SurfaceZone, UpgradeKind},
    logistics::{LogisticsCommand, OutpostService, TravelNode, sites::LogisticsSites},
    terrain::{TileKind, TilePosition},
};

use super::{Scene, SceneStage, base_scenes::stand, carve, park_rig, put_seat_inside, reveal};
use crate::shell::ModalScreen;

/// The scenes, chained after the main list.
pub const LOGISTICS_SCENES: &[Scene] = &[
    Scene {
        name: "logistics_ancient_lift",
        summary: "An ancient lift restored: its cab carries the rig up the builders' rail between station pockets, glyphs lit",
        setup: |stage| ancient_lift(stage, Shot::Ride),
    },
    Scene {
        name: "logistics_lift_station",
        summary: "At an ancient lift's lowest station: the dialog offers restoring it and the rides it will run",
        setup: |stage| ancient_lift(stage, Shot::StationMenu),
    },
    Scene {
        name: "logistics_outpost",
        summary: "A deep outpost the crew claimed: sign, flag, counter terminal, fuel pump and lamps, the lift beside it",
        setup: |stage| ancient_lift(stage, Shot::Outpost),
    },
    Scene {
        name: "logistics_outpost_counter",
        summary: "The outpost's counter: claimed prices for fuel, repairs and selling, dive charges, the travel network and the field kit",
        setup: |stage| ancient_lift(stage, Shot::Counter),
    },
    Scene {
        name: "logistics_beacon_travel",
        summary: "At a depth beacon: the travel network's destinations with their fuel and credit costs",
        setup: beacon_travel,
    },
    Scene {
        name: "logistics_built_lift",
        summary: "A built lift on a dug shaft: hazard-striped stations, the cab riding down with the rig",
        setup: built_lift,
    },
    Scene {
        name: "logistics_dive_charge",
        summary: "A dive charge's plunge: the rig falling down the shaft the charge cut",
        setup: dive_charge,
    },
    Scene {
        name: "shop_descent_systems",
        summary: "The upgrade shop on a Deep Descent world: deep tiers and the rig systems, the Pressure Hull on sale",
        setup: descent_shop,
    },
];

/// The outpost's middle column and floor row, and the lift beside it.
const OUTPOST_COLUMN: i32 = 70;
const OUTPOST_FLOOR: i32 = 96;
const LIFT_COLUMN: i32 = OUTPOST_COLUMN + 6;
const LIFT_TOP: i32 = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shot {
    Ride,
    StationMenu,
    Outpost,
    Counter,
}

/// Runs one logistics command for `player` through the fixed tick.
fn command(world: &mut WorldState, player: PlayerId, command: LogisticsCommand) {
    if let Err(refusal) = world.check_logistics(player, &command) {
        eprintln!(
            "scene logistics refused: {command:?}: {}",
            refusal.message()
        );
    }
    let mut inputs = TickInputs::default();
    inputs.act_as(player, PlayerCommand::Logistics(command));
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &inputs);
}

/// Fills `xs` x `ys` with stone (a clean slab for a scene).
fn stone(world: &mut WorldState, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>) {
    for y in ys {
        for x in xs.clone() {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
}

/// A slab of stone with an outpost and an ancient lift from row 40 down to it, reached and
/// revealed; the rig parked at the outpost.
fn outpost_world(world: &mut WorldState, player: PlayerId) -> LogisticsSites {
    stone(
        world,
        OUTPOST_COLUMN - 30..LIFT_COLUMN + 30,
        LIFT_TOP - 8..OUTPOST_FLOOR + 10,
    );
    let mut sites = LogisticsSites::default();
    sites.place_outpost(OUTPOST_COLUMN, OUTPOST_FLOOR, 1, "Cathedral Station");
    sites.place_ancient_lift(LIFT_COLUMN, LIFT_TOP, OUTPOST_FLOOR - 1, 28);
    world.install_logistics_sites(&sites, true);
    // A side tunnel out of the outpost and a cave pocket at the lift's top.
    carve(
        world,
        OUTPOST_COLUMN - 24..OUTPOST_COLUMN - 4,
        OUTPOST_FLOOR - 2..OUTPOST_FLOOR,
    );
    carve(
        world,
        LIFT_COLUMN - 6..LIFT_COLUMN - 1,
        LIFT_TOP - 1..LIFT_TOP + 1,
    );
    reveal(
        world,
        player,
        OUTPOST_COLUMN - 40..LIFT_COLUMN + 40,
        LIFT_TOP - 12..OUTPOST_FLOOR + 12,
    );
    if let Some(rig) = world.player_mut(player) {
        rig.credits = 38_000;
        rig.fuel = rig.fuel_capacity * 0.3;
        rig.hull = rig.max_hull() * 0.6;
        rig.add_cargo(fatherload_simulation::terrain::MineralKind::Ruby);
        rig.add_cargo(fatherload_simulation::terrain::MineralKind::Gold);
    }
    sites
}

fn ancient_lift(stage: &mut SceneStage<'_>, shot: Shot) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let _ = outpost_world(world, player);
        let shaft = world
            .logistics()
            .lift_on_column(LIFT_COLUMN)
            .map_or(0, |lift| lift.id);
        let outpost = world
            .logistics()
            .outposts
            .keys()
            .copied()
            .next()
            .unwrap_or(0);
        // Reach and claim the outpost from inside it.
        stand(world, player, OUTPOST_COLUMN, OUTPOST_FLOOR - 1);
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        if shot != Shot::StationMenu {
            command(
                world,
                player,
                LogisticsCommand::Outpost {
                    outpost,
                    service: OutpostService::Claim,
                },
            );
        }
        match shot {
            Shot::Ride => {
                stand(world, player, LIFT_COLUMN, OUTPOST_FLOOR - 1);
                command(world, player, LogisticsCommand::RestoreLift { shaft });
                command(
                    world,
                    player,
                    LogisticsCommand::RideLift {
                        shaft,
                        to_row: LIFT_TOP,
                    },
                );
                // Up most of the way.
                for _ in 0..38 {
                    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
                }
            }
            Shot::StationMenu => stand(world, player, LIFT_COLUMN, OUTPOST_FLOOR - 1),
            Shot::Outpost | Shot::Counter => {
                stand(world, player, OUTPOST_COLUMN - 1, OUTPOST_FLOOR - 1);
            }
        }
    });
    if matches!(shot, Shot::StationMenu | Shot::Counter) {
        let seat = stage.seat(0);
        seat.modal = Some(ModalScreen::Logistics);
        seat.selected_menu_item = 0;
    }
    stage.settle(if shot == Shot::Ride { 4 } else { 30 });
}

fn beacon_travel(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let _ = outpost_world(world, player);
        stand(world, player, OUTPOST_COLUMN, OUTPOST_FLOOR - 1);
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        // Beacons down the side tunnel and in the lift's top pocket.
        stand(world, player, LIFT_COLUMN - 4, LIFT_TOP);
        command(world, player, LogisticsCommand::PlaceBeacon);
        stand(world, player, OUTPOST_COLUMN - 20, OUTPOST_FLOOR - 1);
        command(world, player, LogisticsCommand::PlaceBeacon);
        if let Some(rig) = world.player_mut(player) {
            rig.fuel = rig.fuel_capacity * 0.7;
        }
    });
    let travel = stage
        .world()
        .logistics()
        .beacons
        .values()
        .find(|beacon| beacon.at.y == LIFT_TOP)
        .map(|beacon| TravelNode::Beacon(beacon.id));
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Logistics);
    stage.settle(10);
    // Select the jump to the upper beacon.
    let selected = stage
        .seat(0)
        .logistics
        .rows
        .iter()
        .position(|row| Some(row.command) == travel.map(|to| LogisticsCommand::Travel { to }))
        .unwrap_or(0);
    stage.seat(0).selected_menu_item = selected;
    stage.settle(4);
}

/// The built lift's shaft column, top and bottom rows.
const SHAFT: i32 = 60;
const SHAFT_TOP: i32 = 50;
const SHAFT_BOTTOM: i32 = 84;

fn built_lift(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        stone(
            world,
            SHAFT - 30..SHAFT + 30,
            SHAFT_TOP - 10..SHAFT_BOTTOM + 8,
        );
        carve(world, SHAFT..SHAFT + 1, SHAFT_TOP..SHAFT_BOTTOM + 1);
        carve(world, SHAFT - 8..SHAFT, SHAFT_TOP - 1..SHAFT_TOP + 1);
        carve(
            world,
            SHAFT + 1..SHAFT + 10,
            SHAFT_BOTTOM - 1..SHAFT_BOTTOM + 1,
        );
        carve(world, SHAFT + 1..SHAFT + 6, 66..68);
        reveal(
            world,
            player,
            SHAFT - 40..SHAFT + 40,
            SHAFT_TOP - 14..SHAFT_BOTTOM + 12,
        );
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 20_000;
        }
        for row in [SHAFT_BOTTOM, 67, SHAFT_TOP] {
            stand(world, player, SHAFT, row);
            command(world, player, LogisticsCommand::PlaceLiftStation);
        }
        let shaft = world
            .logistics()
            .lift_on_column(SHAFT)
            .map_or(0, |lift| lift.id);
        stand(world, player, SHAFT, SHAFT_TOP);
        command(
            world,
            player,
            LogisticsCommand::RideLift { shaft, to_row: 67 },
        );
        for _ in 0..40 {
            let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        }
    });
    stage.settle(4);
}

fn dive_charge(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let column = 64;
        stone(world, column - 30..column + 30, 40..110);
        carve(world, column - 6..column + 6, 46..48);
        carve(world, column - 10..column + 10, 74..92);
        reveal(world, player, column - 40..column + 40, 30..110);
        let spawn = world.player_template().clone();
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 5_000;
            rig.x = spawn.x;
            rig.y = spawn.y;
        }
        command(world, player, LogisticsCommand::BuyDiveCharges);
        stand(world, player, column, 47);
        command(world, player, LogisticsCommand::FireDiveCharge);
        let held =
            TickInputs::default().hold_as(player, fatherload_simulation::ControlState::default());
        for _ in 0..22 {
            let _ = world.simulate_tick(world.simulation_tick(), &held);
        }
    });
    stage.settle(2);
}

fn descent_shop(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_progression_rules(ProgressionRules::Descent);
        park_rig(world, player, 97, 4);
        let row = RigSystem::PressureHull
            .sold_from_row(world.descent_layout())
            .max(0);
        if let Some(fatherload_simulation::SectionPayload::Progression(mut progression)) =
            world.section(fatherload_simulation::SectionKey::Progression)
        {
            progression.deepest_tile_reached = row;
            world.apply_section(
                fatherload_simulation::SectionKey::Progression,
                fatherload_simulation::SectionPayload::Progression(progression),
            );
        }
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 72_500;
            rig.drill_strength = 15;
            rig.fuel_tank_level = 13;
            rig.hull_level = 12;
        }
    });
    stage.settle(5);
    put_seat_inside(stage, 0, SurfaceZone::Shop, Some(ModalScreen::Shop));
    stage.seat(0).selected_menu_item = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::DescentPressureHull)
        .unwrap_or(0);
    stage.settle(4);
}
