//! Depth logistics scenarios (Part III F10): lift rides (built and ancient), outposts and
//! their counters, beacon travel and its costs, dive charges, the progression rules, the
//! Descent gates and rig systems, replication, saves and determinism.

use serde::{Serialize, de::DeserializeOwned};

use super::SectionKey;
use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    base::BuiltTile,
    depth_progression::{
        DESCENT_MAX_UPGRADE_LEVEL, GATE_DAMAGE_PER_SECOND, PlanLayout, ProgressionRules, RigSystem,
        core_tier_cap,
    },
    economy::{
        MAX_UPGRADE_LEVEL, UpgradeKind, UpgradeLock, shop_line_count, upgrade_cost, upgrade_lock,
        upgrade_lock_for, upgrade_offers, upgrade_offers_for, upgrade_tier_name,
    },
    logistics::{
        BEACON_PRICE, DIVE_CHARGE_PACK, LIFT_STATION_PRICE, LogisticsCommand, OUTPOST_FUEL_UNIT,
        OutpostService, TravelNode, sites::LogisticsSites, travel_cost,
    },
    summary::PlayerScopedCommandOutcome,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    upgrade_preview::{upgrade_preview, upgrade_preview_for},
    worldgen::GeneratorVersion,
};

const PLAYER: PlayerId = PlayerId::new(0);
/// The test shaft's column and its top and bottom rows (rigs stand on these rows).
const SHAFT: i32 = 30;
const TOP: i32 = 60;
const BOTTOM: i32 = 160;

fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

fn world_at(hz: u32) -> WorldState {
    let mut world = WorldState::new_game_with_params(PLAYER, SimulationParams::new(hz));
    world.player_mut(PLAYER).expect("player").credits = 1_000_000;
    world
}

/// A world with a one-wide shaft dug on `SHAFT` from `TOP` to `BOTTOM` (solid rock under the
/// bottom) and side pockets at the top and bottom to stand in.
fn shaft_world(hz: u32) -> WorldState {
    let mut world = world_at(hz);
    for y in TOP - 3..=BOTTOM + 2 {
        for x in SHAFT - 3..=SHAFT + 3 {
            let open = (x == SHAFT && (TOP..=BOTTOM).contains(&y))
                || ((y == TOP || y == BOTTOM) && (x - SHAFT).abs() <= 2);
            let kind = if open { TileKind::Air } else { TileKind::Stone };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    world
}

fn park(world: &mut WorldState, x: i32, row: i32) {
    let player = world.player_mut(PLAYER).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        player.x = (x as f32 + 0.5) * TILE_SIZE;
        player.y = (row as f32 + 0.5) * TILE_SIZE;
    }
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn act(
    world: &mut WorldState,
    command: LogisticsCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::Logistics(command));
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let outcome = output.action_outcomes.first().expect("outcome").outcome;
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

fn applied(world: &mut WorldState, command: LogisticsCommand) -> Vec<GameplayEventKind> {
    let verdict = world.check_logistics(PLAYER, &command);
    let (outcome, events) = act(world, command);
    assert_eq!(
        outcome,
        PlayerScopedCommandOutcome::Applied,
        "{command:?}: {verdict:?}"
    );
    events
}

fn idle(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, ControlState::default());
    world
        .simulate_tick(world.simulation_tick(), &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

/// A tick without controls: rigs hold still (the depths below the generated world's floor).
fn still(world: &mut WorldState) -> Vec<GameplayEventKind> {
    world
        .simulate_tick(world.simulation_tick(), &TickInputs::default())
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn rig_tile(world: &WorldState) -> TilePosition {
    world
        .player(PLAYER)
        .expect("player")
        .tile_position(TILE_SIZE)
}

/// Builds the two-station lift and returns its shaft id.
fn built_lift(world: &mut WorldState) -> u32 {
    park(world, SHAFT, BOTTOM);
    let credits = world.player(PLAYER).expect("player").credits;
    let events = applied(world, LogisticsCommand::PlaceLiftStation);
    assert!(events.contains(&GameplayEventKind::LogisticsPlaced {
        player_id: PLAYER,
        x: SHAFT,
        y: BOTTOM,
        beacon: false,
    }));
    let paid = credits - world.player(PLAYER).expect("player").credits;
    assert!(paid >= LIFT_STATION_PRICE, "paid {paid}");
    park(world, SHAFT, TOP);
    let _ = applied(world, LogisticsCommand::PlaceLiftStation);
    // The top station's floor seals the shaft mouth so a rig can stand on it.
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition {
                x: SHAFT,
                y: TOP + 1
            })
            .map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::Plating))
    );
    let lift = world.logistics().lift_on_column(SHAFT).expect("lift");
    assert_eq!(lift.stations.len(), 2);
    lift.id
}

/// Rides from the top station to the bottom one and returns the ticks it took.
fn ride_down(world: &mut WorldState, shaft: u32) -> u64 {
    park(world, SHAFT, TOP);
    let events = applied(
        world,
        LogisticsCommand::RideLift {
            shaft,
            to_row: BOTTOM,
        },
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::LiftRideStarted { .. }))
    );
    let mut ticks = 1;
    loop {
        let events = idle(world);
        ticks += 1;
        assert!(
            world.logistics().is_riding(PLAYER)
                || events.contains(&GameplayEventKind::LiftArrived {
                    player_id: PLAYER,
                    shaft
                })
        );
        if !world.logistics().is_riding(PLAYER) {
            break;
        }
        // The rider is pinned to the shaft column the whole way.
        assert_eq!(rig_tile(world).x, SHAFT);
        assert!(ticks < 10_000, "the ride never arrives");
    }
    ticks
}

#[test]
fn a_built_lift_carries_the_rig_between_its_stations() {
    let mut world = shaft_world(60);
    let shaft = built_lift(&mut world);
    let fuel_before = world.player(PLAYER).expect("player").fuel;
    let ticks = ride_down(&mut world, shaft);
    // 100 rows at 20 rows a second: five seconds.
    assert!((300..=303).contains(&ticks), "{ticks} ticks");
    assert_eq!(
        rig_tile(&world),
        TilePosition {
            x: SHAFT,
            y: BOTTOM
        }
    );
    assert!((world.player(PLAYER).expect("player").fuel - fuel_before).abs() < f32::EPSILON);
    // It stays put: standing on the bottom's rock.
    for _ in 0..30 {
        let _ = idle(&mut world);
    }
    assert_eq!(
        rig_tile(&world),
        TilePosition {
            x: SHAFT,
            y: BOTTOM
        }
    );
    // And back up, landing on the top station's floor.
    let _ = applied(
        &mut world,
        LogisticsCommand::RideLift { shaft, to_row: TOP },
    );
    while world.logistics().is_riding(PLAYER) {
        let _ = idle(&mut world);
    }
    for _ in 0..30 {
        let _ = idle(&mut world);
    }
    assert_eq!(rig_tile(&world), TilePosition { x: SHAFT, y: TOP });
}

#[test]
fn lift_rides_are_deterministic_and_tick_rate_safe() {
    let run = |hz: u32| {
        let mut world = shaft_world(hz);
        let shaft = built_lift(&mut world);
        let ticks = ride_down(&mut world, shaft);
        (world, ticks)
    };
    let (first, ticks_60) = run(60);
    let (second, again) = run(60);
    assert_eq!(ticks_60, again);
    assert_eq!(
        first.simulation_state_hash(),
        second.simulation_state_hash()
    );
    let (_, ticks_30) = run(30);
    let (_, ticks_120) = run(120);
    #[allow(clippy::cast_precision_loss, reason = "a few hundred ticks")]
    let seconds = |ticks: u64, hz: u32| ticks as f32 / hz as f32;
    for (ticks, hz) in [(ticks_30, 30), (ticks_120, 120)] {
        assert!(
            (seconds(ticks, hz) - seconds(ticks_60, 60)).abs() < 0.1,
            "{hz} Hz: {ticks}"
        );
    }
}

#[test]
fn a_blocked_shaft_refuses_the_ride() {
    let mut world = shaft_world(60);
    let shaft = built_lift(&mut world);
    let _ = world
        .terrain_mut()
        .set_kind(TilePosition { x: SHAFT, y: 100 }, TileKind::Stone);
    park(&mut world, SHAFT, TOP);
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::RideLift {
                shaft,
                to_row: BOTTOM
            }
        ),
        Err(super::logistics::LogisticsRefusal::ShaftBlocked)
    );
}

fn ancient_world() -> (WorldState, LogisticsSites) {
    let mut world = world_at(60);
    let mut sites = LogisticsSites::default();
    sites.place_outpost(SHAFT - 6, BOTTOM + 1, 1, "Test Camp");
    sites.place_ancient_lift(SHAFT, TOP, BOTTOM, 50);
    // Rock around the rail so the test does not depend on the generated caves.
    for y in TOP - 2..=BOTTOM + 2 {
        for x in SHAFT - 12..=SHAFT + 3 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    world.install_logistics_sites(&sites, true);
    (world, sites)
}

#[test]
fn ancient_lifts_restore_from_their_bottom_station_then_run() {
    let (mut world, _) = ancient_world();
    let shaft = world.logistics().lift_on_column(SHAFT).expect("lift").id;
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x: SHAFT, y: 80 })
            .map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::RuinWall))
    );
    park(&mut world, SHAFT, TOP);
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::RideLift {
                shaft,
                to_row: BOTTOM
            }
        ),
        Err(super::logistics::LogisticsRefusal::NotRestored)
    );
    assert_eq!(
        world.check_logistics(PLAYER, &LogisticsCommand::RestoreLift { shaft }),
        Err(super::logistics::LogisticsRefusal::NotBottomStation)
    );
    park(&mut world, SHAFT, BOTTOM);
    let events = applied(&mut world, LogisticsCommand::RestoreLift { shaft });
    assert!(events.contains(&GameplayEventKind::LiftRestored {
        player_id: PLAYER,
        shaft
    }));
    // Up through the rail (110 rows at 45 a second).
    let _ = applied(
        &mut world,
        LogisticsCommand::RideLift { shaft, to_row: TOP },
    );
    let mut ticks = 0;
    while world.logistics().is_riding(PLAYER) {
        let _ = idle(&mut world);
        ticks += 1;
    }
    assert!((130..=140).contains(&ticks), "{ticks}");
    for _ in 0..30 {
        let _ = idle(&mut world);
    }
    assert_eq!(rig_tile(&world), TilePosition { x: SHAFT, y: TOP });
}

#[test]
fn outposts_join_the_network_and_charge_less_once_claimed() {
    let (mut world, sites) = ancient_world();
    let site = &sites.outposts[0];
    let outpost = world
        .logistics()
        .outposts
        .keys()
        .copied()
        .next()
        .expect("outpost");
    assert!(!world.logistics().outposts[&outpost].reached);
    park(&mut world, site.column, site.floor_row - 1);
    let events = idle(&mut world);
    assert!(events.contains(&GameplayEventKind::OutpostReached {
        player_id: PLAYER,
        outpost
    }));
    assert!(
        world
            .logistics()
            .travel_nodes()
            .contains(&TravelNode::Outpost(outpost))
    );
    // Unclaimed fuel costs three credits a unit.
    let refuel = LogisticsCommand::Outpost {
        outpost,
        service: OutpostService::Refuel,
    };
    world.player_mut(PLAYER).expect("player").fuel = 100.0;
    let before = world.player(PLAYER).expect("player").credits;
    let _ = applied(&mut world, refuel);
    let player = world.player(PLAYER).expect("player");
    let missing = player.fuel_capacity - 100.0;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small"
    )]
    let units = missing.ceil() as u32;
    assert_eq!(before - player.credits, units * OUTPOST_FUEL_UNIT[0]);
    assert!((player.fuel - player.fuel_capacity).abs() < f32::EPSILON);
    // A home needs a claim; the claim makes fuel cheap.
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::Outpost {
                outpost,
                service: OutpostService::SetHome
            }
        ),
        Err(super::logistics::LogisticsRefusal::NotClaimed)
    );
    let events = applied(
        &mut world,
        LogisticsCommand::Outpost {
            outpost,
            service: OutpostService::Claim,
        },
    );
    assert!(events.contains(&GameplayEventKind::OutpostClaimed {
        player_id: PLAYER,
        outpost
    }));
    world.player_mut(PLAYER).expect("player").fuel = 100.0;
    let before = world.player(PLAYER).expect("player").credits;
    let _ = applied(&mut world, refuel);
    assert_eq!(
        before - world.player(PLAYER).expect("player").credits,
        units * OUTPOST_FUEL_UNIT[1]
    );
    // Selling keeps a cut; a home there brings rescues back to it.
    world
        .player_mut(PLAYER)
        .expect("player")
        .add_cargo(MineralKind::Gold);
    let _ = applied(
        &mut world,
        LogisticsCommand::Outpost {
            outpost,
            service: OutpostService::SellCargo,
        },
    );
    assert_eq!(world.player(PLAYER).expect("player").cargo_used(), 0);
    let _ = applied(
        &mut world,
        LogisticsCommand::Outpost {
            outpost,
            service: OutpostService::SetHome,
        },
    );
    let home = world.rescue_home(PLAYER).expect("home");
    #[allow(clippy::cast_possible_truncation, reason = "world pixels")]
    let home_tile = TilePosition {
        x: (home.0 / TILE_SIZE).floor() as i32,
        y: (home.1 / TILE_SIZE).floor() as i32,
    };
    assert!(world.logistics().outposts[&outpost].contains(home_tile));
}

#[test]
fn beacons_travel_for_fuel_and_credits_by_distance() {
    let mut world = shaft_world(60);
    park(&mut world, SHAFT, BOTTOM);
    let credits = world.player(PLAYER).expect("player").credits;
    let events = applied(&mut world, LogisticsCommand::PlaceBeacon);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::LogisticsPlaced { beacon: true, .. }
    )));
    assert!(credits - world.player(PLAYER).expect("player").credits >= BEACON_PRICE);
    let beacon = world
        .logistics()
        .beacons
        .keys()
        .copied()
        .next()
        .expect("beacon");
    // A second one right beside it is refused.
    park(&mut world, SHAFT + 2, BOTTOM);
    assert_eq!(
        world.check_logistics(PLAYER, &LogisticsCommand::PlaceBeacon),
        Err(super::logistics::LogisticsRefusal::TooCloseToBeacon)
    );
    // From the beacon up to the town.
    park(&mut world, SHAFT, BOTTOM);
    let quote = world
        .check_logistics(
            PLAYER,
            &LogisticsCommand::Travel {
                to: TravelNode::Surface,
            },
        )
        .expect("quote");
    let surface_row = world.player_template().tile_position(TILE_SIZE).y;
    assert_eq!(
        (quote.fuel, quote.credits),
        travel_cost(surface_row - BOTTOM)
    );
    let (fuel, credits) = {
        let player = world.player(PLAYER).expect("player");
        (player.fuel, player.credits)
    };
    let events = applied(
        &mut world,
        LogisticsCommand::Travel {
            to: TravelNode::Surface,
        },
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::BeaconTravelled { .. }))
    );
    let player = world.player(PLAYER).expect("player");
    assert_eq!(credits - player.credits, quote.credits);
    #[allow(clippy::cast_precision_loss, reason = "small")]
    let burned = quote.fuel as f32;
    assert!((fuel - player.fuel - burned).abs() < 0.01);
    assert_eq!(
        rig_tile(&world),
        world.player_template().tile_position(TILE_SIZE)
    );
    assert_eq!(world.travel_stop_of(PLAYER), Some(TravelNode::Surface));
    // And back down to the beacon.
    let _ = applied(
        &mut world,
        LogisticsCommand::Travel {
            to: TravelNode::Beacon(beacon),
        },
    );
    assert_eq!(
        rig_tile(&world),
        TilePosition {
            x: SHAFT,
            y: BOTTOM
        }
    );
    // Nowhere to travel from in the middle of the rock.
    park(&mut world, SHAFT, 110);
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::Travel {
                to: TravelNode::Surface
            }
        ),
        Err(super::logistics::LogisticsRefusal::NotAtStop)
    );
    // Broke rigs stay put.
    park(&mut world, SHAFT, BOTTOM);
    world.player_mut(PLAYER).expect("player").credits = 0;
    assert!(matches!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::Travel {
                to: TravelNode::Surface
            }
        ),
        Err(super::logistics::LogisticsRefusal::NotEnoughCredits { .. })
    ));
}

#[test]
fn a_dive_charge_cuts_a_shaft_and_lands_softly() {
    let mut world = world_at(60);
    // A rig on a long column of stone with an open cave 30 rows down.
    for y in 40..=120 {
        for x in SHAFT - 2..=SHAFT + 2 {
            let kind = if y == 40 || (60..100).contains(&y) {
                TileKind::Air
            } else {
                TileKind::Stone
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let _ = world.terrain_mut().set_kind(
        TilePosition { x: SHAFT, y: 45 },
        TileKind::Ore(MineralKind::Silver),
    );
    park(&mut world, SHAFT, 40);
    assert_eq!(
        world.check_logistics(PLAYER, &LogisticsCommand::FireDiveCharge),
        Err(super::logistics::LogisticsRefusal::NoCharges)
    );
    // Charges sell at outposts and in town only.
    assert_eq!(
        world.check_logistics(PLAYER, &LogisticsCommand::BuyDiveCharges),
        Err(super::logistics::LogisticsRefusal::NoSeller)
    );
    let spawn = world.player_template().tile_position(TILE_SIZE);
    park(&mut world, spawn.x, spawn.y);
    let _ = applied(&mut world, LogisticsCommand::BuyDiveCharges);
    assert_eq!(
        world.logistics().dive_charges_of(PLAYER),
        DIVE_CHARGE_PACK.0
    );
    park(&mut world, SHAFT, 40);
    let hull = world.player(PLAYER).expect("player").hull;
    let events = applied(&mut world, LogisticsCommand::FireDiveCharge);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::DiveChargeFired { rows: 19, .. }))
    );
    assert_eq!(
        world.logistics().dive_charges_of(PLAYER),
        DIVE_CHARGE_PACK.0 - 1
    );
    assert_eq!(
        world
            .terrain()
            .tile(TilePosition { x: SHAFT, y: 59 })
            .map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .carried(crate::item::ItemKind::Mineral(MineralKind::Silver)),
        1
    );
    // The same plunge without the dive's soft landing.
    let mut hard = world.clone();
    hard.logistics.dives.clear();
    for _ in 0..600 {
        let _ = idle(&mut world);
        let _ = idle(&mut hard);
    }
    let tile = rig_tile(&world);
    assert_eq!(tile.x, SHAFT);
    assert_eq!(tile.y, 99, "landed on the cave floor");
    assert!(!world.logistics().is_diving(PLAYER));
    let soft = hull - world.player(PLAYER).expect("player").hull;
    let crash = hull - hard.player(PLAYER).expect("player").hull;
    assert!(
        crash > soft + 15.0,
        "dive lost {soft}, a plain fall {crash}"
    );
}

#[test]
fn classic_worlds_keep_their_ladders_and_never_sell_rig_systems() {
    let world = world_at(60);
    assert_eq!(world.progression_rules(), ProgressionRules::Classic);
    let player = world.player(PLAYER).expect("player");
    let classic = upgrade_offers(player);
    let explicit = upgrade_offers_for(player, ProgressionRules::Classic);
    assert_eq!(classic, explicit);
    for offer in classic {
        assert_eq!(offer.cost, upgrade_cost(offer.kind, offer.level));
        assert_eq!(offer.max_level, offer.kind.max_level());
        if offer.kind.is_core() {
            assert_eq!(offer.max_level, MAX_UPGRADE_LEVEL);
        }
        let lock = upgrade_lock(&offer, world.world_settings(), 100_000);
        if offer.kind.is_descent_only() {
            assert_eq!(lock, Some(UpgradeLock::DescentOnly));
        } else {
            assert_ne!(lock, Some(UpgradeLock::DescentOnly));
        }
    }
    for generator in [
        GeneratorVersion::V1,
        GeneratorVersion::V5,
        GeneratorVersion::V10,
    ] {
        assert_eq!(
            ProgressionRules::for_generator(generator),
            ProgressionRules::Classic
        );
    }
    // A classic world never hashes the (empty) logistics.
    assert!(world.logistics().is_empty());
    // Its shop lists its own 18 lines: the rig systems are the trailing offers it hides.
    let lines = shop_line_count(ProgressionRules::Classic);
    assert_eq!(lines, 18);
    assert_eq!(
        shop_line_count(ProgressionRules::Descent),
        classic.len(),
        "Deep Descent shops list every line"
    );
    assert!(
        classic[..lines]
            .iter()
            .all(|offer| offer.kind.rig_system().is_none())
    );
    assert!(
        classic[lines..]
            .iter()
            .all(|offer| offer.kind.is_descent_only())
    );
    // A maxed classic core line previews nothing and still names its own top tier.
    let mut maxed = player.clone();
    maxed.drill_strength = MAX_UPGRADE_LEVEL;
    assert!(upgrade_preview(&maxed, UpgradeKind::Drill, None).is_none());
    let offer = upgrade_offers(&maxed)[0];
    let top = upgrade_tier_name(UpgradeKind::Drill, offer.level.min(offer.max_level - 1));
    assert_ne!(
        top,
        upgrade_tier_name(UpgradeKind::Drill, MAX_UPGRADE_LEVEL),
        "a maxed classic drill must not be named after the first deep tier"
    );
}

#[test]
fn descent_worlds_preview_the_deep_tiers() {
    let world = world_at(60);
    let mut player = world.player(PLAYER).expect("player").clone();
    player.drill_strength = MAX_UPGRADE_LEVEL;
    player.fuel_tank_level = MAX_UPGRADE_LEVEL;
    assert!(
        upgrade_preview_for(&player, UpgradeKind::Drill, None, ProgressionRules::Classic).is_none()
    );
    for kind in [UpgradeKind::Drill, UpgradeKind::FuelTank] {
        let preview = upgrade_preview_for(&player, kind, None, ProgressionRules::Descent)
            .expect("a deep tier to preview");
        assert_eq!(preview.to_level, MAX_UPGRADE_LEVEL + 1, "{kind:?}");
        assert!(
            preview
                .bars
                .iter()
                .all(|bar| bar.next >= bar.now && bar.next <= bar.max + 1e-3),
            "{kind:?}: {:?}",
            preview.bars
        );
    }
    player.drill_strength = DESCENT_MAX_UPGRADE_LEVEL;
    assert!(
        upgrade_preview_for(&player, UpgradeKind::Drill, None, ProgressionRules::Descent).is_none()
    );
}

#[test]
fn the_town_stop_ends_above_the_rooftops() {
    let mut world = world_at(60);
    let home = world.player_template().tile_position(TILE_SIZE);
    park(&mut world, home.x, home.y);
    assert_eq!(world.travel_stop_of(PLAYER), Some(TravelNode::Surface));
    // High over the town (a sky isle, the Moon) is not the town.
    park(&mut world, home.x, home.y - 400);
    assert_eq!(world.travel_stop_of(PLAYER), None);
    assert_eq!(
        world.check_logistics(PLAYER, &LogisticsCommand::BuyDiveCharges),
        Err(super::logistics::LogisticsRefusal::NoSeller)
    );
}

#[test]
fn descent_worlds_gate_deep_tiers_and_rig_systems_by_depth() {
    let mut world = world_at(60);
    world.set_progression_rules(ProgressionRules::Descent);
    let settings = *world.world_settings();
    let rules = ProgressionRules::Descent;
    let mut player = world.player(PLAYER).expect("player").clone();
    player.drill_strength = core_tier_cap(0);
    let drill = upgrade_offers_for(&player, rules)[0];
    assert_eq!(drill.max_level, DESCENT_MAX_UPGRADE_LEVEL);
    assert_eq!(
        upgrade_lock_for(&drill, &settings, 10, rules, 0),
        Some(UpgradeLock::Descent(1))
    );
    assert_eq!(upgrade_lock_for(&drill, &settings, 1_200, rules, 0), None);
    // Past the classic top the ladder keeps going, dearer each tier.
    player.drill_strength = MAX_UPGRADE_LEVEL;
    let deep = upgrade_offers_for(&player, rules)[0];
    assert!(deep.cost > upgrade_cost(UpgradeKind::Drill, MAX_UPGRADE_LEVEL - 1));
    assert_eq!(
        upgrade_lock_for(&deep, &settings, 1_200, rules, 0),
        Some(UpgradeLock::Descent(4))
    );
    let index = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::CryoHull)
        .expect("line");
    let hull = upgrade_offers_for(&player, rules)[index];
    let sold_from = RigSystem::CryoHull.sold_from_row(&PlanLayout);
    assert!(matches!(
        upgrade_lock_for(&hull, &settings, sold_from - 1, rules, 0),
        Some(UpgradeLock::Depth { .. })
    ));
    // Wave 8: reached is not enough; the crew must have felt the crush first.
    assert_eq!(
        upgrade_lock_for(&hull, &settings, sold_from, rules, 0),
        Some(UpgradeLock::Unfelt(RigSystem::CryoHull))
    );
    assert_eq!(
        upgrade_lock_for(
            &hull,
            &settings,
            sold_from,
            rules,
            RigSystem::CryoHull.bit()
        ),
        None
    );
    // The Star Core's Pressure Hull is won, never sold (Wave 8 integration).
    let index = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::DescentPressureHull)
        .expect("line");
    let pressure = upgrade_offers_for(&player, rules)[index];
    assert_eq!(
        upgrade_lock_for(&pressure, &settings, i32::MAX, rules, u8::MAX),
        Some(UpgradeLock::Boss(crate::descent::DescentBoss::StarCore))
    );
}

#[test]
fn descent_gates_crush_rigs_without_their_system() {
    let mut world = world_at(60);
    world.set_progression_rules(ProgressionRules::Descent);
    // Descent III starts at row 2,700 on the plan's table: stand a rig there in a pocket.
    let row = 2_750;
    world.terrain_mut().ensure_depth(row + 10);
    for y in row - 2..=row + 1 {
        for x in SHAFT - 1..=SHAFT + 1 {
            let kind = if y == row + 1 {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    park(&mut world, SHAFT, row);
    world.player_mut(PLAYER).expect("player").hull = 1_000.0;
    // The same rig with the Pressure Hull fitted measures the depth's other hazards.
    let mut fitted = world.clone();
    crate::economy::grant_rig_system(
        fitted.player_mut(PLAYER).expect("player"),
        RigSystem::PressureHull,
    );
    let events = still(&mut world);
    assert!(events.contains(&GameplayEventKind::DescentGateBlocked {
        player_id: PLAYER,
        descent: 2,
        system: RigSystem::PressureHull,
    }));
    for _ in 0..59 {
        let events = still(&mut world);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, GameplayEventKind::DescentGateBlocked { .. })),
            "warned once"
        );
    }
    for _ in 0..60 {
        let events = still(&mut fitted);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, GameplayEventKind::DescentGateBlocked { .. }))
        );
    }
    let hull = |world: &WorldState| world.player(PLAYER).expect("player").hull;
    let gate = hull(&fitted) - hull(&world);
    assert!(
        (gate - GATE_DAMAGE_PER_SECOND).abs() < 1.0,
        "the gate took {gate} in a second"
    );
}

#[test]
fn the_shop_sells_rig_systems_on_descent_worlds() {
    let mut world = world_at(60);
    world.set_progression_rules(ProgressionRules::Descent);
    let index = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::CryoHull)
        .expect("line");
    // Stand in the shop with the world dug deep enough.
    let shop = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Shop)
        .expect("shop");
    let spawn_row = world.player_template().tile_position(TILE_SIZE).y;
    park(&mut world, shop.tile_x + 2, spawn_row);
    let buy = PlayerCommand::BuyUpgrade { index };
    let inputs = TickInputs::default().act(PLAYER, buy.clone());
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "too shallow"
    );
    world.progression.deepest_tile_reached = RigSystem::CryoHull.sold_from_row(&PlanLayout);
    let inputs = TickInputs::default().act(PLAYER, buy.clone());
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::IgnoredUnavailable,
        "reached but not felt yet (Wave 8)"
    );
    world.logistics.felt_systems |= RigSystem::CryoHull.bit();
    world.player_mut(PLAYER).expect("player").credits = 10_000_000;
    let inputs = TickInputs::default().act(PLAYER, buy);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    assert_eq!(
        output.action_outcomes[0].outcome,
        PlayerScopedCommandOutcome::Applied
    );
    assert!(
        world
            .player(PLAYER)
            .expect("player")
            .hazard_gear
            .has_system(RigSystem::CryoHull)
    );
}

#[test]
fn logistics_replicate_and_save() {
    let (mut world, _) = ancient_world();
    world.set_progression_rules(ProgressionRules::Descent);
    park(&mut world, SHAFT, BOTTOM);
    let _ = applied(&mut world, LogisticsCommand::PlaceBeacon);
    let shaft = world.logistics().lift_on_column(SHAFT).expect("lift").id;
    let _ = applied(&mut world, LogisticsCommand::RestoreLift { shaft });
    let _ = applied(
        &mut world,
        LogisticsCommand::RideLift { shaft, to_row: TOP },
    );
    assert!(SectionKey::SHARED.contains(&SectionKey::Logistics));
    let payload = wire(&world.section(SectionKey::Logistics).expect("section"));
    let mut replica = WorldState::new_game(PLAYER);
    replica.apply_section(SectionKey::Logistics, payload);
    assert_eq!(replica.logistics(), world.logistics());
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.logistics(), world.logistics());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // The ride goes on identically in both.
    let mut copy = loaded;
    for _ in 0..200 {
        let _ = idle(&mut world);
        let _ = idle(&mut copy);
    }
    assert_eq!(copy.simulation_state_hash(), world.simulation_state_hash());
    assert!(!world.logistics().is_riding(PLAYER));
}
