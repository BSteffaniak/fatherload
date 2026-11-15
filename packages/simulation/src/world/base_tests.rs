//! Base building scenarios (WP-BASE): claims, placement rules, deconstruction refunds, the
//! underground fuel station, power, storage, the uplink, lift ladders, home points,
//! permissions, saves, replication and determinism.

use serde::{Serialize, de::DeserializeOwned};

use super::{SectionKey, WorldImport};
use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationTick, TickInputs,
    WorldState,
    base::{
        BuildCommand, BuildRefusal, Buildable, BuiltTile, CLAIM_RADIUS, ClaimAccess,
        FUEL_STATION_CAPACITY, FUEL_STATION_STARTING_FUEL, FUEL_SYNTH_PER_PERIOD, Facing, Fixture,
        ModuleKind, ModuleService, StructureId,
    },
    item::{BlockKind, ItemKind},
    summary::PlayerScopedCommandOutcome,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::Band,
};

const PLAYER: PlayerId = PlayerId::new(0);
const OTHER: PlayerId = PlayerId::new(1);
/// The floor row of the test cavern; rigs park on row `FLOOR - 1`.
const FLOOR: i32 = 70;
/// The cavern's columns.
const LEFT: i32 = 10;
const RIGHT: i32 = 44;
/// The claim beacon's tile.
const BEACON: TilePosition = TilePosition {
    x: 26,
    y: FLOOR - 1,
};

fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

/// A deep open cavern (rows 52..FLOOR, columns LEFT..=RIGHT) over a stone floor, far below the
/// town's columns' no-build rows, with the rig parked on the floor next to the beacon spot and
/// plenty of credits.
fn cavern() -> WorldState {
    let mut world = WorldState::from_import(&WorldImport::new_game(PLAYER));
    for y in 52..=FLOOR + 1 {
        for x in LEFT - 1..=RIGHT + 1 {
            let kind = if y >= FLOOR || !(LEFT..=RIGHT).contains(&x) {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    park(&mut world, PLAYER, BEACON.x, FLOOR - 1);
    world.player_mut(PLAYER).expect("player").credits = 50_000;
    world
}

fn park(world: &mut WorldState, player_id: PlayerId, tile_x: i32, tile_y: i32) {
    let player = world.player_mut(player_id).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small tile coordinates")]
    {
        player.x = (tile_x as f32 + 0.5) * TILE_SIZE;
        player.y = (tile_y as f32 + 0.5) * TILE_SIZE;
    }
    player.velocity_x = 0.0;
    player.velocity_y = 0.0;
}

fn tile(x: i32, y: i32) -> TilePosition {
    TilePosition { x, y }
}

fn place(item: Buildable, at: TilePosition) -> PlayerCommand {
    PlayerCommand::Build(BuildCommand::Place {
        item,
        at,
        facing: Facing::Right,
    })
}

fn remove(at: TilePosition) -> PlayerCommand {
    PlayerCommand::Build(BuildCommand::Remove { at })
}

fn use_module(structure: StructureId, service: ModuleService) -> PlayerCommand {
    PlayerCommand::Build(BuildCommand::Use { structure, service })
}

/// Runs one tick with `command` and returns its outcome and events.
fn act(
    world: &mut WorldState,
    player_id: PlayerId,
    command: PlayerCommand,
) -> (PlayerScopedCommandOutcome, Vec<GameplayEventKind>) {
    let inputs = TickInputs::default().act(player_id, command);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    let outcome = output
        .action_outcomes
        .first()
        .map(|outcome| outcome.outcome)
        .expect("one outcome");
    (
        outcome,
        output.events.into_iter().map(|event| event.kind).collect(),
    )
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "call sites build the command inline"
)]
fn applied(world: &mut WorldState, player_id: PlayerId, command: PlayerCommand) {
    let refused = command_refusal(world, player_id, &command);
    let (outcome, _) = act(world, player_id, command.clone());
    assert_eq!(
        outcome,
        PlayerScopedCommandOutcome::Applied,
        "{command:?}: {refused:?}"
    );
}

fn command_refusal(
    world: &WorldState,
    player_id: PlayerId,
    command: &PlayerCommand,
) -> Option<BuildRefusal> {
    match command {
        PlayerCommand::Build(build) => world.check_build(player_id, build).err(),
        _ => None,
    }
}

fn refusal(world: &WorldState, player_id: PlayerId, command: &PlayerCommand) -> BuildRefusal {
    let before = world.simulation_state_hash();
    let refusal = command_refusal(world, player_id, command).expect("refused");
    let mut copy = world.clone();
    let (outcome, _) = act(&mut copy, player_id, command.clone());
    assert_eq!(outcome, PlayerScopedCommandOutcome::IgnoredUnavailable);
    assert_eq!(world.simulation_state_hash(), before);
    refusal
}

/// A cavern with a claim staked at [`BEACON`].
fn claimed() -> WorldState {
    let mut world = cavern();
    applied(
        &mut world,
        PLAYER,
        place(Buildable::Module(ModuleKind::Beacon), BEACON),
    );
    world
}

fn module_of(world: &WorldState, kind: ModuleKind) -> StructureId {
    world
        .bases()
        .modules()
        .find(|module| module.kind == kind)
        .map(|module| module.id)
        .expect("module")
}

#[test]
fn a_beacon_stakes_a_claim_and_charges_for_it() {
    let mut world = cavern();
    let credits = world.player(PLAYER).expect("player").credits;
    let (outcome, events) = act(
        &mut world,
        PLAYER,
        place(Buildable::Module(ModuleKind::Beacon), BEACON),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::BasePlaced {
            item: Buildable::Module(ModuleKind::Beacon),
            ..
        }
    )));
    let claim = world.bases().claim_at(BEACON).expect("claim");
    assert_eq!(claim.owner, PLAYER);
    assert!(!claim.surface, "staked underground");
    assert_eq!(claim.access, ClaimAccess::Crew);
    assert!(
        world
            .bases()
            .claim_at(tile(BEACON.x + CLAIM_RADIUS, BEACON.y))
            .is_some()
    );
    assert!(
        world
            .bases()
            .claim_at(tile(BEACON.x + CLAIM_RADIUS + 1, BEACON.y))
            .is_none()
    );
    let player = world.player(PLAYER).expect("player");
    // 250 credits plus four blocks bought at the dirt price (the hopper is empty).
    assert_eq!(credits - player.credits, 250 + 4 * 2);
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario per placement rule")]
fn placement_rules_refuse_bad_spots_without_changing_the_world() {
    let mut world = cavern();
    // No claim yet.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Tile(BuiltTile::Plating),
                tile(BEACON.x + 1, FLOOR - 1)
            )
        ),
        BuildRefusal::NoClaim
    );
    // A beacon floating in the air needs a floor.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Beacon),
                tile(BEACON.x, FLOOR - 4)
            )
        ),
        BuildRefusal::NeedsFloor
    );
    // Into rock.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(Buildable::Module(ModuleKind::Beacon), tile(BEACON.x, FLOOR))
        ),
        BuildRefusal::Blocked
    );
    // Too far from the rig.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Beacon),
                tile(BEACON.x + 12, FLOOR - 1)
            )
        ),
        BuildRefusal::OutOfReach
    );
    // In town.
    let mut town = cavern();
    let (first, _) = crate::surface::town_span();
    let surface = town.terrain().surface_row(first + 4);
    park(&mut town, PLAYER, first + 4, surface - 1);
    assert_eq!(
        refusal(
            &town,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Beacon),
                tile(first + 4, surface - 1)
            )
        ),
        BuildRefusal::InTown
    );

    applied(
        &mut world,
        PLAYER,
        place(Buildable::Module(ModuleKind::Beacon), BEACON),
    );
    // A second claim overlapping the first.
    park(&mut world, PLAYER, BEACON.x + 6, FLOOR - 1);
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Beacon),
                tile(BEACON.x + 6, FLOOR - 1)
            )
        ),
        BuildRefusal::OverlapsClaim
    );
    park(&mut world, PLAYER, BEACON.x, FLOOR - 1);
    // A wall on the rig itself.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Tile(BuiltTile::Plating),
                tile(BEACON.x, FLOOR - 1)
            )
        ),
        BuildRefusal::Blocked
    );
    // A module spanning the claim's edge.
    park(&mut world, PLAYER, BEACON.x + 8, FLOOR - 1);
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Generator),
                tile(BEACON.x + CLAIM_RADIUS, FLOOR - 1)
            )
        ),
        BuildRefusal::LeavesClaim
    );
    // The homestead only goes on a surface claim.
    world.progression.deepest_tile_reached = FLOOR;
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Module(ModuleKind::Homestead),
                tile(BEACON.x + 2, FLOOR - 1)
            )
        ),
        BuildRefusal::SurfaceOnly
    );
    // A table needs a floor.
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Fixture(Fixture::Table),
                tile(BEACON.x + 3, FLOOR - 3)
            )
        ),
        BuildRefusal::NeedsFloor
    );
    // Broke.
    world.player_mut(PLAYER).expect("player").credits = 3;
    assert_eq!(
        refusal(
            &world,
            PLAYER,
            &place(
                Buildable::Tile(BuiltTile::Plating),
                tile(BEACON.x + 3, FLOOR - 1)
            )
        ),
        BuildRefusal::CannotAfford
    );
}

#[test]
fn modules_unlock_with_depth() {
    let mut world = claimed();
    world.progression.deepest_tile_reached = 0;
    let station = place(
        Buildable::Module(ModuleKind::FuelStation),
        tile(BEACON.x + 2, FLOOR - 1),
    );
    assert_eq!(
        refusal(&world, PLAYER, &station),
        BuildRefusal::Locked(Band::Clay)
    );
    world.progression.deepest_tile_reached = FLOOR;
    applied(&mut world, PLAYER, station);
}

#[test]
fn walls_use_blocks_from_the_hopper_or_credits_and_refund_on_deconstruction() {
    let mut world = claimed();
    world
        .player_mut(PLAYER)
        .expect("player")
        .stow(ItemKind::Block(BlockKind::Stone), 1);
    let credits = world.player(PLAYER).expect("player").credits;
    let first = tile(BEACON.x + 2, FLOOR - 1);
    let second = tile(BEACON.x + 2, FLOOR - 2);
    let wall = Buildable::Tile(BuiltTile::Wall(BlockKind::Stone));
    applied(&mut world, PLAYER, place(wall, first));
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.credits, credits, "the block on hand paid for it");
    assert_eq!(player.block_hopper.count(BlockKind::Stone), 0);
    applied(&mut world, PLAYER, place(wall, second));
    assert_eq!(
        world.player(PLAYER).expect("player").credits,
        credits - 4,
        "no block left: bought"
    );
    assert_eq!(
        world.terrain().tile(first).map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::Wall(BlockKind::Stone)))
    );
    assert!(
        world.terrain().is_solid_at(first),
        "rigs collide with walls"
    );
    let (outcome, events) = act(&mut world, PLAYER, remove(first));
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::BaseRemoved { .. }))
    );
    assert_eq!(
        world.terrain().tile(first).map(|tile| tile.kind),
        Some(TileKind::Air)
    );
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .block_hopper
            .count(BlockKind::Stone),
        1,
        "the block comes back"
    );
}

#[test]
fn fixtures_layer_a_back_wall_behind_a_front_fixture() {
    let mut world = claimed();
    let spot = tile(BEACON.x + 3, FLOOR - 1);
    applied(
        &mut world,
        PLAYER,
        place(Buildable::BackWall(BlockKind::Clay), spot),
    );
    applied(
        &mut world,
        PLAYER,
        place(Buildable::Fixture(Fixture::Plant), spot),
    );
    let fixtures = world.bases().fixtures_at(spot);
    assert_eq!(fixtures.back_wall, Some(BlockKind::Clay));
    assert_eq!(fixtures.front, Some((Fixture::Plant, Facing::Right)));
    assert!(!world.terrain().is_solid_at(spot), "fixtures are passable");
    // Deconstruct takes the front fixture first, then the back wall.
    applied(&mut world, PLAYER, remove(spot));
    assert_eq!(world.bases().fixtures_at(spot).front, None);
    applied(&mut world, PLAYER, remove(spot));
    assert!(world.bases().fixtures_at(spot).is_empty());
    assert_eq!(
        refusal(&world, PLAYER, &remove(spot)),
        BuildRefusal::NothingThere
    );
}

#[test]
fn built_walls_shrug_off_blasts_and_drills() {
    let mut world = claimed();
    let spot = tile(BEACON.x + 2, FLOOR - 1);
    applied(
        &mut world,
        PLAYER,
        place(Buildable::Tile(BuiltTile::Plating), spot),
    );
    let blast = world.detonate_bomb_at(spot, 3);
    assert!(!blast.changed_tiles.contains(&spot));
    assert!(crate::terrain::is_indestructible(TileKind::Built(
        BuiltTile::Plating
    )));
    assert_eq!(
        world.terrain_mut().chip_by(spot, 9, true),
        crate::terrain::MineResult::Blocked
    );
}

/// A cavern with a claim and an underground fuel station two tiles right of the beacon.
fn with_fuel_station() -> (WorldState, StructureId) {
    let mut world = claimed();
    world.progression.deepest_tile_reached = FLOOR;
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::FuelStation),
            tile(BEACON.x + 2, FLOOR - 1),
        ),
    );
    let station = module_of(&world, ModuleKind::FuelStation);
    (world, station)
}

#[test]
fn the_underground_fuel_station_refuels_a_rig_at_depth() {
    let (mut world, station) = with_fuel_station();
    assert_eq!(
        world.bases().module(station).expect("station").fuel,
        FUEL_STATION_STARTING_FUEL
    );
    // Drive to the station, nearly dry, deep underground.
    park(&mut world, PLAYER, BEACON.x + 3, FLOOR - 1);
    let player = world.player_mut(PLAYER).expect("player");
    player.fuel = 0.0;
    let credits = player.credits;
    assert!(world.failure_state(PLAYER).is_some(), "stranded");
    let (outcome, events) = act(
        &mut world,
        PLAYER,
        use_module(station, ModuleService::Refuel),
    );
    assert_eq!(outcome, PlayerScopedCommandOutcome::Applied);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::BaseServiceUsed {
            service: ModuleService::Refuel,
            ..
        }
    )));
    let player = world.player(PLAYER).expect("player");
    #[allow(clippy::cast_precision_loss, reason = "small reserve")]
    let expected = (FUEL_STATION_STARTING_FUEL as f32).min(player.fuel_capacity);
    assert!((player.fuel - expected).abs() < f32::EPSILON);
    assert_eq!(player.credits, credits, "your own station's fuel is free");
    assert!(world.failure_state(PLAYER).is_none(), "no longer stranded");
    let left = world.bases().module(station).expect("station").fuel;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small whole amount of fuel"
    )]
    let pumped = expected as u32;
    assert_eq!(left, FUEL_STATION_STARTING_FUEL - pumped);
    // Away from the station it refuses.
    park(&mut world, PLAYER, BEACON.x - 6, FLOOR - 1);
    world.player_mut(PLAYER).expect("player").fuel = 1.0;
    assert_eq!(
        refusal(&world, PLAYER, &use_module(station, ModuleService::Refuel)),
        BuildRefusal::NotAtModule
    );
}

#[test]
fn powered_stations_synthesize_fuel_and_supply_drops_fill_them() {
    let (mut world, station) = with_fuel_station();
    assert!(
        world.base_module_powered(station),
        "the beacon's cell runs it"
    );
    let start = world.bases().module(station).expect("station").fuel;
    let ticks = world
        .timing()
        .rate
        .ticks_for_millis(crate::base::FUEL_SYNTH_PERIOD_MILLIS);
    for _ in 0..ticks * 2 {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
    }
    let after = world.bases().module(station).expect("station").fuel;
    assert_eq!(after, start + 2 * FUEL_SYNTH_PER_PERIOD);
    // A supply drop fills the rest for credits.
    park(&mut world, PLAYER, BEACON.x + 3, FLOOR - 1);
    let credits = world.player(PLAYER).expect("player").credits;
    let (units, price) = world.supply_drop_offer(station, credits);
    assert_eq!(units, FUEL_STATION_CAPACITY - after);
    applied(
        &mut world,
        PLAYER,
        use_module(station, ModuleService::OrderSupply),
    );
    assert_eq!(
        world.bases().module(station).expect("station").fuel,
        FUEL_STATION_CAPACITY
    );
    assert_eq!(
        world.player(PLAYER).expect("player").credits,
        credits - price
    );
}

#[test]
fn modules_draw_power_in_order_and_generators_supply_more() {
    let (mut world, station) = with_fuel_station();
    park(&mut world, PLAYER, BEACON.x - 4, FLOOR - 1);
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::RepairBay),
            tile(BEACON.x - 6, FLOOR - 1),
        ),
    );
    let bay = module_of(&world, ModuleKind::RepairBay);
    assert!(world.base_module_powered(station));
    assert!(
        !world.base_module_powered(bay),
        "the beacon's cell is spoken for"
    );
    world.player_mut(PLAYER).expect("player").hull = 10.0;
    assert_eq!(
        refusal(&world, PLAYER, &use_module(bay, ModuleService::Repair)),
        BuildRefusal::Unpowered
    );
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::Generator),
            tile(BEACON.x - 3, FLOOR - 1),
        ),
    );
    assert!(world.base_module_powered(bay));
    applied(&mut world, PLAYER, use_module(bay, ModuleService::Repair));
    let player = world.player(PLAYER).expect("player");
    assert!(player.hull > 10.0);
}

#[test]
fn storage_takes_cargo_and_gives_it_back() {
    let mut world = claimed();
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::Storage),
            tile(BEACON.x + 1, FLOOR - 1),
        ),
    );
    let chest = module_of(&world, ModuleKind::Storage);
    let player = world.player_mut(PLAYER).expect("player");
    player.cargo_bay.add(MineralKind::Gold, 5);
    player.stow(ItemKind::Block(BlockKind::Clay), 7);
    applied(
        &mut world,
        PLAYER,
        use_module(chest, ModuleService::DepositCargo),
    );
    applied(
        &mut world,
        PLAYER,
        use_module(chest, ModuleService::DepositBlocks),
    );
    let stored = &world.bases().module(chest).expect("chest").storage;
    assert_eq!(stored.mineral(MineralKind::Gold), 5);
    assert_eq!(stored.count(BlockKind::Clay), 7);
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .cargo_bay
            .mineral(MineralKind::Gold),
        0
    );
    // A chest with things in it does not come down.
    assert_eq!(
        refusal(&world, PLAYER, &remove(tile(BEACON.x + 1, FLOOR - 1))),
        BuildRefusal::NotEmpty
    );
    // The beacon does not come down while its claim has structures.
    assert_eq!(
        refusal(&world, PLAYER, &remove(BEACON)),
        BuildRefusal::NotEmpty
    );
    applied(
        &mut world,
        PLAYER,
        use_module(chest, ModuleService::WithdrawAll),
    );
    assert_eq!(
        world
            .player(PLAYER)
            .expect("player")
            .cargo_bay
            .mineral(MineralKind::Gold),
        5
    );
    assert!(
        world
            .bases()
            .module(chest)
            .expect("chest")
            .storage
            .is_empty()
    );
    let credits = world.player(PLAYER).expect("player").credits;
    let paid = world.bases().module(chest).expect("chest").paid_credits;
    assert_eq!(paid, 150 + 6 * 2, "the price plus six blocks bought");
    applied(&mut world, PLAYER, remove(tile(BEACON.x + 1, FLOOR - 1)));
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.credits, credits + paid * 75 / 100, "75% back");
    // Now the bare claim can be taken down too.
    applied(&mut world, PLAYER, remove(BEACON));
    assert!(world.bases().claim_at(BEACON).is_none());
}

#[test]
fn the_cargo_uplink_sells_remotely_for_a_fee() {
    let mut world = claimed();
    world.progression.deepest_tile_reached = 400;
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::Generator),
            tile(BEACON.x - 3, FLOOR - 1),
        ),
    );
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::CargoUplink),
            tile(BEACON.x + 2, FLOOR - 1),
        ),
    );
    let uplink = module_of(&world, ModuleKind::CargoUplink);
    park(&mut world, PLAYER, BEACON.x + 2, FLOOR - 1);
    world
        .player_mut(PLAYER)
        .expect("player")
        .cargo_bay
        .add(MineralKind::Silver, 10);
    let value = world.mineral_market_value(MineralKind::Silver) * 10;
    let credits = world.player(PLAYER).expect("player").credits;
    applied(
        &mut world,
        PLAYER,
        use_module(uplink, ModuleService::SellCargo),
    );
    let player = world.player(PLAYER).expect("player");
    assert_eq!(player.credits - credits, value - value * 15 / 100);
    assert!(!player.cargo_bay.has_minerals());
}

#[test]
fn lift_ladders_climb_without_fuel_and_break_falls() {
    let mut world = claimed();
    let column = BEACON.x + 4;
    for y in FLOOR - 8..FLOOR {
        applied(
            &mut world,
            PLAYER,
            place(Buildable::Fixture(Fixture::Ladder), tile(column, y)),
        );
    }
    park(&mut world, PLAYER, column, FLOOR - 1);
    world.player_mut(PLAYER).expect("player").fuel = 0.0;
    let start_y = world.player(PLAYER).expect("player").y;
    for _ in 0..60 {
        let tick = world.simulation_tick();
        let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, true, false));
        let _ = world.simulate_tick(tick, &inputs);
    }
    let player = world.player(PLAYER).expect("player");
    assert!(
        player.y < 2.0f32.mul_add(-TILE_SIZE, start_y),
        "climbed with an empty tank"
    );
    assert!(player.fuel <= 0.0);
    // Let go: it slides down the ladder at a safe speed.
    let hull = player.hull;
    for _ in 0..240 {
        let tick = world.simulation_tick();
        let inputs = TickInputs::held(PLAYER, ControlState::new(0.0, false, false));
        let _ = world.simulate_tick(tick, &inputs);
        assert!(world.player(PLAYER).expect("player").velocity_y <= crate::base::LIFT_SLIDE_SPEED);
    }
    assert!((world.player(PLAYER).expect("player").hull - hull).abs() < f32::EPSILON);
}

#[test]
fn rescues_land_at_the_home_point() {
    let mut world = claimed();
    applied(
        &mut world,
        PLAYER,
        place(
            Buildable::Module(ModuleKind::Bed),
            tile(BEACON.x + 2, FLOOR - 1),
        ),
    );
    let bed = module_of(&world, ModuleKind::Bed);
    park(&mut world, PLAYER, BEACON.x + 2, FLOOR - 1);
    applied(&mut world, PLAYER, use_module(bed, ModuleService::SetHome));
    // Wander off and get stranded.
    park(&mut world, PLAYER, LEFT + 1, FLOOR - 1);
    world.player_mut(PLAYER).expect("player").fuel = 0.0;
    applied(&mut world, PLAYER, PlayerCommand::Rescue);
    let player = world.player(PLAYER).expect("player");
    assert_eq!(
        player.tile_position(TILE_SIZE),
        tile(BEACON.x + 3, FLOOR - 1)
    );
}

#[test]
fn claim_access_controls_who_builds_and_uses() {
    let mut world = claimed();
    world.add_player(OTHER);
    park(&mut world, OTHER, BEACON.x - 1, FLOOR - 1);
    world.player_mut(OTHER).expect("other").credits = 1_000;
    let spot = tile(BEACON.x - 3, FLOOR - 1);
    // Crew access (the default): co-op builds together.
    applied(
        &mut world,
        OTHER,
        place(Buildable::Tile(BuiltTile::Plank), spot),
    );
    let beacon = world.bases().claim_at(BEACON).expect("claim").beacon;
    assert_eq!(
        refusal(
            &world,
            OTHER,
            &use_module(beacon, ModuleService::CycleAccess)
        ),
        BuildRefusal::NotPermitted,
        "only the owner changes access"
    );
    applied(
        &mut world,
        PLAYER,
        use_module(beacon, ModuleService::CycleAccess),
    );
    assert_eq!(
        world.bases().claim_at(BEACON).expect("claim").access,
        ClaimAccess::Services
    );
    assert_eq!(
        refusal(&world, OTHER, &remove(spot)),
        BuildRefusal::NotPermitted
    );
    applied(
        &mut world,
        PLAYER,
        use_module(beacon, ModuleService::CycleAccess),
    );
    assert_eq!(
        refusal(
            &world,
            OTHER,
            &use_module(beacon, ModuleService::CycleAccess)
        ),
        BuildRefusal::NotPermitted
    );
}

#[test]
fn claims_are_limited_and_safe_zones_cover_them() {
    let world = claimed();
    assert!(world.bases().is_safe_zone(BEACON));
    assert!(
        world
            .bases()
            .is_safe_zone(tile(BEACON.x + CLAIM_RADIUS + 3, BEACON.y))
    );
    assert!(!world.bases().is_safe_zone(tile(BEACON.x + 40, BEACON.y)));
    assert_eq!(crate::base::claim_limit(0), 2);
    assert!(crate::base::claim_limit(30) <= crate::base::MAX_CLAIMS);
}

/// Builds a small base with every kind of piece through the tick.
fn build_a_base(world: &mut WorldState) {
    world.progression.deepest_tile_reached = 400;
    world
        .player_mut(PLAYER)
        .expect("player")
        .stow(ItemKind::Block(BlockKind::Stone), 20);
    let commands = [
        place(Buildable::Module(ModuleKind::Beacon), BEACON),
        place(
            Buildable::Module(ModuleKind::FuelStation),
            tile(BEACON.x + 2, FLOOR - 1),
        ),
        place(
            Buildable::Module(ModuleKind::Generator),
            tile(BEACON.x - 3, FLOOR - 1),
        ),
        place(
            Buildable::Tile(BuiltTile::Wall(BlockKind::Stone)),
            tile(BEACON.x - 6, FLOOR - 1),
        ),
        place(
            Buildable::Tile(BuiltTile::Glass),
            tile(BEACON.x - 6, FLOOR - 2),
        ),
        place(
            Buildable::BackWall(BlockKind::Stone),
            tile(BEACON.x - 4, FLOOR - 3),
        ),
        place(
            Buildable::Fixture(Fixture::Light),
            tile(BEACON.x - 4, FLOOR - 4),
        ),
    ];
    for command in commands {
        applied(world, PLAYER, command);
    }
}

#[test]
fn bases_survive_a_save_round_trip() {
    let mut world = cavern();
    build_a_base(&mut world);
    let json = serde_json::to_string(&world).expect("json");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.bases(), world.bases());
    assert_eq!(
        loaded
            .terrain()
            .tile(tile(BEACON.x - 6, FLOOR - 2))
            .map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::Glass))
    );
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // A save from before bases existed loads with none.
    let mut old: serde_json::Value = serde_json::from_str(&json).expect("value");
    old.as_object_mut().expect("object").remove("bases");
    let old: WorldState = serde_json::from_value(old).expect("old save");
    assert!(old.bases().is_empty());
}

#[test]
fn replicas_follow_builds_through_terrain_ops_and_the_bases_section() {
    let mut authority = cavern();
    let mut replica: WorldState = wire(&authority);
    let mut seen = authority.section_hash(SectionKey::Bases);
    let mut step = |authority: &mut WorldState, replica: &mut WorldState, command| {
        let inputs = TickInputs::default().act(PLAYER, command);
        let output = authority.simulate_tick(authority.simulation_tick(), &inputs);
        replica.set_simulation_tick(authority.simulation_tick());
        let ids: Vec<PlayerId> = authority.player_ids().collect();
        for id in ids {
            replica.apply_player_tick_state(id, authority.player_tick_state(id).expect("rig"));
        }
        for op in wire(&output.terrain_ops) {
            replica.apply_tile_change(&op);
        }
        for discovery in wire(&output.discoveries) {
            replica.apply_discovery(&discovery);
        }
        replica.apply_live_entities(wire(&authority.live_entities()));
        for key in authority.section_keys() {
            let hash = authority.section_hash(key);
            if key == SectionKey::Bases && hash == seen {
                continue;
            }
            if key == SectionKey::Bases {
                seen = hash;
            }
            replica.apply_section(key, wire(&authority.section(key).expect("section")));
        }
    };
    authority.progression.deepest_tile_reached = 400;
    replica.progression.deepest_tile_reached = 400;
    for command in [
        place(Buildable::Module(ModuleKind::Beacon), BEACON),
        place(
            Buildable::Module(ModuleKind::FuelStation),
            tile(BEACON.x + 2, FLOOR - 1),
        ),
        place(
            Buildable::Tile(BuiltTile::Plating),
            tile(BEACON.x - 3, FLOOR - 1),
        ),
        place(
            Buildable::Fixture(Fixture::Door),
            tile(BEACON.x - 2, FLOOR - 1),
        ),
    ] {
        step(&mut authority, &mut replica, command);
        for key in authority.section_keys() {
            assert_eq!(
                replica.section_hash(key),
                authority.section_hash(key),
                "{key:?}"
            );
        }
        assert_eq!(replica.terrain().digest(), authority.terrain().digest());
        assert_eq!(
            replica.simulation_state_hash(),
            authority.simulation_state_hash()
        );
    }
    assert_eq!(replica.bases(), authority.bases());
    assert_eq!(
        replica
            .terrain()
            .tile(tile(BEACON.x - 3, FLOOR - 1))
            .map(|tile| tile.kind),
        Some(TileKind::Built(BuiltTile::Plating))
    );
}

#[test]
fn building_is_deterministic() {
    let run = || {
        let mut world = cavern();
        build_a_base(&mut world);
        for _ in 0..400 {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &TickInputs::default());
        }
        world.simulation_state_hash()
    };
    assert_eq!(run(), run());
    // And a world with a base hashes differently from one without.
    let mut bare = cavern();
    for _ in 0..407 {
        let tick = bare.simulation_tick();
        let _ = bare.simulate_tick(tick, &TickInputs::default());
    }
    assert_ne!(run(), bare.simulation_state_hash());
    let _ = SimulationTick::new(0);
}
