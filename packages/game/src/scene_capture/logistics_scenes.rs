//! Depth logistics scenes (Part III F10): an ancient lift's cab carrying a rig up its rail, a
//! deep outpost the crew claimed and its counter, the travel network from a depth beacon, a
//! built lift on a dug shaft, a dive charge's plunge, and the upgrade shop of a Descent world
//! with the rig systems on its shelves. Wave 9: Merrow City's deep counter, its supplies and
//! its workshop.
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
    Scene {
        name: "outpost_deep_shop",
        summary: "Wave 8: a deep outpost's counter selling the tiers its Descent needs (Pressure Rating, Headlamp, core tiers to the cap, a felt rig system)",
        setup: |stage| outpost_trade(stage, crate::shell::logistics::LogisticsGroup::Shop),
    },
    Scene {
        name: "outpost_workshop",
        summary: "Wave 8: the outpost's workshop turning layer resources into parts and supplies, the bay's layer resources beside it",
        setup: |stage| outpost_trade(stage, crate::shell::logistics::LogisticsGroup::Workshop),
    },
    Scene {
        name: "new_world_deep_descent",
        summary: "Wave 8: the New World dialog defaulting to the Deep Descent, with Classic (V10) a step away on the World row",
        setup: |stage| {
            let shell = stage.shell();
            shell.open_world_setup(crate::shell::SetupPurpose::Local { seats: 1 });
            if let Some(setup) = shell.world_setup.as_mut() {
                setup.settings.seed = 0x00DE_E9DE_5CE7;
                setup.set_deep_descent(true);
                setup.selected = 7;
            }
        },
    },
    Scene {
        name: "layer_resource_drops",
        summary: "Wave 8: layer resources popping out of drilled Clay Beds rock beside the clay blocks, glowing in their stratum's colour",
        setup: layer_drops,
    },
    Scene {
        name: "deep_counter_supplies",
        summary: "Wave 9: Merrow City's counter in the Drowned and Frozen: its cheaper seals and engines, the deep supplies for the strata around it and the rig's supply kit",
        setup: |stage| deep_counter(stage, crate::shell::logistics::LogisticsGroup::Supplies),
    },
    Scene {
        name: "deep_counter_shop",
        summary: "Wave 9: Merrow City's deep shop selling Descent IV's tiers, its specialty lines at the settlement's price",
        setup: |stage| deep_counter(stage, crate::shell::logistics::LogisticsGroup::Shop),
    },
    Scene {
        name: "deep_workshop_supplies",
        summary: "Wave 9: the Merrow workshop crafting ballast tanks, frost heaters and coral seals from the Drowned and Frozen's layer resources",
        setup: |stage| deep_counter(stage, crate::shell::logistics::LogisticsGroup::Workshop),
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

/// An outpost of the Old Deep on a Deep Descent ruleset, reached, the crew dug to the bottom of
/// it and a gate felt, the bay holding layer resources: the dialog scrolled to `group`.
fn outpost_trade(stage: &mut SceneStage<'_>, group: crate::shell::logistics::LogisticsGroup) {
    use fatherload_simulation::{descent::Stratum, item::ItemKind};
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_progression_rules(ProgressionRules::Descent);
        let _ = outpost_world(world, player);
        stand(world, player, OUTPOST_COLUMN, OUTPOST_FLOOR - 1);
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        // Everyone has been down to the Living Dark and felt its crush.
        let row = fatherload_simulation::depth_progression::DescentLayout::strata(
            &world.descent_layout(),
        )
        .iter()
        .find(|stratum| stratum.descent == 2)
        .map_or(2_800, |stratum| stratum.top + 20);
        if let Some(fatherload_simulation::SectionPayload::Progression(mut progression)) =
            world.section(fatherload_simulation::SectionKey::Progression)
        {
            progression.deepest_tile_reached = row;
            world.apply_section(
                fatherload_simulation::SectionKey::Progression,
                fatherload_simulation::SectionPayload::Progression(progression),
            );
        }
        if let Some(fatherload_simulation::SectionPayload::Logistics(mut logistics)) =
            world.section(fatherload_simulation::SectionKey::Logistics)
        {
            logistics.felt_systems |= RigSystem::PressureHull.bit();
            world.apply_section(
                fatherload_simulation::SectionKey::Logistics,
                fatherload_simulation::SectionPayload::Logistics(logistics),
            );
        }
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 81_400;
            rig.drill_strength = 8;
            rig.fuel_tank_level = 7;
            rig.cargo_bay_level = 7;
            rig.engine_level = 6;
            rig.hull_level = 7;
            rig.radiator_level = 8;
            rig.hazard_gear.heat_shield = 3;
            rig.hull = rig.max_hull() * 0.5;
            for (stratum, count) in [
                (Stratum::ClayBeds, 7),
                (Stratum::GreyStone, 3),
                (Stratum::SlateShelves, 6),
                (Stratum::BasaltColumns, 4),
                (Stratum::CrystalFaults, 9),
                (Stratum::FossilOceans, 2),
                (Stratum::PressureCathedrals, 6),
            ] {
                rig.cargo_bay.add(ItemKind::Layer(stratum), count);
            }
        }
    });
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Logistics);
    seat.selected_menu_item = 0;
    stage.settle(6);
    // Select the first row of the section on show (the dialog scrolls to it).
    let wanted = match group {
        crate::shell::logistics::LogisticsGroup::Workshop => Some(UpgradeKind::Radiator),
        _ => Some(UpgradeKind::PressureRating),
    };
    let rows = &stage.seat(0).logistics.rows;
    let selected = rows
        .iter()
        .rposition(|row| {
            row.group == group
                && match (group, row.command) {
                    (
                        crate::shell::logistics::LogisticsGroup::Shop,
                        LogisticsCommand::OutpostUpgrade { offer, .. },
                    ) => UpgradeKind::ALL.get(usize::from(offer)).copied() == wanted,
                    (crate::shell::logistics::LogisticsGroup::Workshop, _) => row.refusal.is_none(),
                    _ => false,
                }
        })
        .or_else(|| rows.iter().position(|row| row.group == group))
        .unwrap_or(0);
    stage.seat(0).selected_menu_item = selected;
    // The bay's ore woke a milestone banner: the shot is about the counter.
    stage.seat(0).rewards.banners.clear();
    stage.settle(4);
}

/// A rig drilling through Clay Beds rock on a Deep Descent world with layer resource drops
/// popping out around it.
fn layer_drops(stage: &mut SceneStage<'_>) {
    use fatherload_simulation::{descent::Stratum, item::ItemKind};
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_progression_rules(ProgressionRules::Descent);
        let column = 64;
        stone(world, column - 30..column + 30, 40..80);
        carve(world, column - 8..column + 8, 54..60);
        reveal(world, player, column - 40..column + 40, 36..84);
        stand(world, player, column, 59);
        // What a dig through the Clay Beds left: blocks, ore and two layer resources.
        let drops = [
            (column - 4, 57, ItemKind::Layer(Stratum::ClayBeds)),
            (column + 3, 56, ItemKind::Layer(Stratum::ClayBeds)),
            (column + 5, 58, ItemKind::Layer(Stratum::Topsoil)),
            (column - 6, 58, ItemKind::Layer(Stratum::CrystalFaults)),
            (
                column - 2,
                58,
                ItemKind::Mineral(fatherload_simulation::terrain::MineralKind::Iron),
            ),
            (
                column + 1,
                58,
                ItemKind::Block(fatherload_simulation::item::BlockKind::Clay),
            ),
        ];
        let frame = world.frame();
        for (x, y, kind) in drops {
            let (px, py) = frame.tile_center(TilePosition { x, y });
            let _ = world.spawn_item_drop(
                px,
                py,
                (0.0, 0.0),
                fatherload_simulation::item::ItemStack::new(kind, 1),
                None,
            );
        }
    });
    stage.settle(30);
}

fn descent_shop(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_progression_rules(ProgressionRules::Descent);
        park_rig(world, player, 97, 4);
        let row = RigSystem::PressureHull
            .sold_from_row(&world.descent_layout())
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

/// Merrow City's outpost (Descent IV) on a Deep Descent ruleset: the crew dug into the Hollow
/// World, a rig fitted for the Drowned and Frozen with a few supplies in its kit and the
/// Descent's layer resources in its bay; the dialog scrolled to `group` (Wave 9).
#[allow(
    clippy::too_many_lines,
    reason = "the outpost, the crew's progress, the rig and the selection in order"
)]
fn deep_counter(stage: &mut SceneStage<'_>, group: crate::shell::logistics::LogisticsGroup) {
    use crate::shell::logistics::LogisticsGroup;
    use fatherload_simulation::{descent::Stratum, item::ItemKind, workshop::DeepSupply};
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        world.set_progression_rules(ProgressionRules::Descent);
        stone(
            world,
            OUTPOST_COLUMN - 30..LIFT_COLUMN + 30,
            LIFT_TOP - 8..OUTPOST_FLOOR + 10,
        );
        let mut sites = LogisticsSites::default();
        sites.place_outpost(OUTPOST_COLUMN, OUTPOST_FLOOR, 3, "Merrow Air Dock");
        sites.place_ancient_lift(LIFT_COLUMN, LIFT_TOP, OUTPOST_FLOOR - 1, 28);
        world.install_logistics_sites(&sites, true);
        carve(
            world,
            OUTPOST_COLUMN - 24..OUTPOST_COLUMN - 4,
            OUTPOST_FLOOR - 2..OUTPOST_FLOOR,
        );
        reveal(
            world,
            player,
            OUTPOST_COLUMN - 40..LIFT_COLUMN + 40,
            LIFT_TOP - 12..OUTPOST_FLOOR + 12,
        );
        stand(world, player, OUTPOST_COLUMN, OUTPOST_FLOOR - 1);
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
        // The crew has been down to the top of the Hollow World.
        let row = fatherload_simulation::depth_progression::DescentLayout::strata(
            &world.descent_layout(),
        )
        .iter()
        .find(|stratum| stratum.descent == 4)
        .map_or(5_900, |stratum| stratum.top + 20);
        if let Some(fatherload_simulation::SectionPayload::Progression(mut progression)) =
            world.section(fatherload_simulation::SectionKey::Progression)
        {
            progression.deepest_tile_reached = row;
            world.apply_section(
                fatherload_simulation::SectionKey::Progression,
                fatherload_simulation::SectionPayload::Progression(progression),
            );
        }
        if let Some(fatherload_simulation::SectionPayload::Logistics(mut logistics)) =
            world.section(fatherload_simulation::SectionKey::Logistics)
        {
            let _ = logistics.add_supplies(player, DeepSupply::BallastTank, 3);
            let _ = logistics.add_supplies(player, DeepSupply::FrostHeater, 1);
            world.apply_section(
                fatherload_simulation::SectionKey::Logistics,
                fatherload_simulation::SectionPayload::Logistics(logistics),
            );
        }
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 912_600;
            rig.drill_strength = 14;
            rig.fuel_tank_level = 15;
            rig.cargo_bay_level = 14;
            rig.engine_level = 13;
            rig.hull_level = 11;
            rig.radiator_level = 14;
            rig.hazard_gear.heat_shield = 2;
            rig.hazard_gear.pressure_rating = 2;
            rig.hazard_gear.headlamp = 3;
            for system in [
                RigSystem::PressureHull,
                RigSystem::BioFilter,
                RigSystem::LampII,
            ] {
                fatherload_simulation::economy::grant_rig_system(rig, system);
            }
            rig.hull = rig.max_hull() * 0.7;
            rig.fuel = rig.fuel_capacity * 0.45;
            for (stratum, count) in [
                (Stratum::UndergroundOcean, 7),
                (Stratum::BrineGlaciers, 5),
                (Stratum::GeyserFields, 3),
                (Stratum::MerrowCity, 6),
                (Stratum::PressureCathedrals, 2),
            ] {
                rig.cargo_bay.add(ItemKind::Layer(stratum), count);
            }
        }
    });
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Logistics);
    seat.selected_menu_item = 0;
    stage.settle(6);
    let rows = &stage.seat(0).logistics.rows;
    let selected = rows
        .iter()
        .position(|row| {
            row.group == group
                && match (group, row.command) {
                    (LogisticsGroup::Shop, LogisticsCommand::OutpostUpgrade { offer, .. }) => {
                        UpgradeKind::ALL.get(usize::from(offer)).copied()
                            == Some(UpgradeKind::PressureRating)
                    }
                    (LogisticsGroup::Supplies, LogisticsCommand::BuySupply { supply, .. }) => {
                        supply == DeepSupply::FlareBuoy
                    }
                    (LogisticsGroup::Workshop, LogisticsCommand::Craft { recipe, .. }) => {
                        fatherload_simulation::workshop::recipe(recipe).is_some_and(|found| {
                            matches!(
                                found.output,
                                fatherload_simulation::workshop::CraftOutput::Supply(
                                    DeepSupply::BallastTank,
                                    _
                                )
                            )
                        })
                    }
                    _ => false,
                }
        })
        .or_else(|| rows.iter().position(|row| row.group == group))
        .unwrap_or(0);
    stage.seat(0).selected_menu_item = selected;
    stage.seat(0).rewards.banners.clear();
    stage.settle(4);
}
