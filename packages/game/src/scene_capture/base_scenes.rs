//! Base building scenes (WP-BASE): a surface homestead and its interior, an underground fuel
//! station outpost, the build-mode UI (keyboard and controller, a fitting and a refused ghost, a
//! claim beacon's proposed claim), a module menu and two crewmates building in split-screen.
//!
//! Every structure is built through the real fixed tick ([`fatherload_simulation::PlayerCommand::Build`]),
//! so the scenes also prove the placement rules accept a real layout.

use fatherload_simulation::{
    PlayerCommand, PlayerId, SectionKey, SectionPayload, TickInputs, WorldState,
    base::{BuildCommand, Buildable, BuiltTile, Facing, Fixture, ModuleKind, StructureId},
    item::{BlockKind, ItemKind},
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{ARROWS, Scene, SceneStage, WASD, XBOX, carve, reveal, tile_centre};
use crate::shell::{BuildDraft, ModalScreen};

/// The scenes, chained after the main list.
pub const BASE_SCENES: &[Scene] = &[
    Scene {
        name: "base_surface_homestead",
        summary: "A surface claim: homestead with chimney smoke, generator, chest, lamps, banner and a stone wall",
        setup: |stage| surface_base(stage, false),
    },
    Scene {
        name: "base_homestead_interior",
        summary: "Inside the homestead: the cutaway room with bed, fireplace, trophy shelf and pantry",
        setup: |stage| surface_base(stage, true),
    },
    Scene {
        name: "base_underground_fuel_station",
        summary: "An underground outpost: fuel station and generator in a plated room with lamps, a bed, a chest and a lift ladder shaft",
        setup: |stage| underground_base(stage, Ui::None),
    },
    Scene {
        name: "base_build_ui",
        summary: "Build mode (keyboard): plating ghost fitting in the claim, the piece strip, cost and controls",
        setup: |stage| underground_base(stage, Ui::BuildFits),
    },
    Scene {
        name: "base_build_refused",
        summary: "Build mode (controller): a repair bay that does not fit, outlined red with the reason",
        setup: |stage| underground_base(stage, Ui::BuildRefused),
    },
    Scene {
        name: "base_fuel_station_menu",
        summary: "The underground fuel station's menu: reserve, power, free refuel and a supply drop",
        setup: |stage| underground_base(stage, Ui::StationMenu),
    },
    Scene {
        name: "base_claim_beacon",
        summary: "Build mode with the claim beacon: the claim it would stake outlined on open ground",
        setup: claim_preview,
    },
    Scene {
        name: "base_coop_split",
        summary: "Two crewmates in split-screen: one building walls in build mode, one at the generator",
        setup: coop_split,
    },
];

/// Lets every module unlock: the world's deepest dig reaches far down.
fn unlock_depth(world: &mut WorldState) {
    if let Some(SectionPayload::Progression(mut progression)) =
        world.section(SectionKey::Progression)
    {
        progression.deepest_tile_reached = progression.deepest_tile_reached.max(400);
        world.apply_section(
            SectionKey::Progression,
            SectionPayload::Progression(progression),
        );
    }
}

/// Puts `player`'s rig at tile `(x, y)` (centre), at rest.
fn stand(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    if let Some(rig) = world.player_mut(player) {
        rig.x = tile_centre(x);
        rig.y = tile_centre(y);
        rig.x_remainder = 0.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

/// Runs one real tick with `command` from `player` standing at `from`.
fn build(world: &mut WorldState, player: PlayerId, from: (i32, i32), command: BuildCommand) {
    stand(world, player, from.0, from.1);
    if let Err(refusal) = world.check_build(player, &command) {
        eprintln!("scene build refused: {command:?}: {}", refusal.message());
    }
    let inputs = TickInputs {
        players: std::iter::once((
            player,
            fatherload_simulation::input::PlayerTickInput {
                controls: None,
                actions: vec![fatherload_simulation::TickAction {
                    sequence: fatherload_simulation::InputSequence::new(0),
                    command: PlayerCommand::Build(command),
                }],
            },
        ))
        .collect(),
    };
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(tick, &inputs);
}

fn place(item: Buildable, x: i32, y: i32) -> BuildCommand {
    place_facing(item, x, y, Facing::Right)
}

fn place_facing(item: Buildable, x: i32, y: i32, facing: Facing) -> BuildCommand {
    BuildCommand::Place {
        item,
        at: TilePosition { x, y },
        facing,
    }
}

fn module_id(world: &WorldState, kind: ModuleKind) -> Option<StructureId> {
    world
        .bases()
        .modules()
        .find(|module| module.kind == kind)
        .map(|module| module.id)
}

/// The surface row of the left meadow (a flat strip west of town).
const GROUND: i32 = 5;
/// The surface claim's beacon column.
const HOME_BEACON: i32 = 24;

/// A surface homestead west of town, its rig outside or (`inside`) in the cutaway room.
fn surface_base(stage: &mut SceneStage<'_>, inside: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 4..48, 0..GROUND + 8);
        unlock_depth(world);
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 50_000;
            rig.stow(ItemKind::Block(BlockKind::Stone), 20);
        }
        let row = GROUND - 1;
        build_homestead(world, player, row);
        stand(
            world,
            player,
            if inside {
                HOME_BEACON + 5
            } else {
                HOME_BEACON + 9
            },
            row,
        );
    });
    stage.settle(50);
}

/// Flattens the meadow west of town to the classic ground line.
fn meadow(world: &mut WorldState) {
    for x in 2..46 {
        for y in 0..GROUND {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
        for y in GROUND..GROUND + 3 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Dirt);
        }
    }
}

fn build_homestead(world: &mut WorldState, player: PlayerId, row: i32) {
    let b = HOME_BEACON;
    let steps = [
        (
            (b, row),
            place(Buildable::Module(ModuleKind::Beacon), b, row),
        ),
        (
            (b + 1, row),
            place(Buildable::Module(ModuleKind::Homestead), b + 2, row),
        ),
        (
            (b + 6, row),
            place(Buildable::Module(ModuleKind::Storage), b + 10, row),
        ),
        (
            (b + 6, row),
            place_facing(Buildable::Fixture(Fixture::Light), b + 9, row, Facing::Down),
        ),
        (
            (b - 2, row),
            place(Buildable::Module(ModuleKind::Generator), b - 7, row),
        ),
        (
            (b - 2, row),
            place_facing(Buildable::Fixture(Fixture::Light), b - 1, row, Facing::Down),
        ),
        (
            (b - 2, row),
            place(Buildable::Fixture(Fixture::Plant), b - 3, row),
        ),
        (
            (b - 2, row),
            place(Buildable::Fixture(Fixture::Crates), b - 4, row),
        ),
        (
            (b - 4, row),
            place(
                Buildable::Tile(BuiltTile::Wall(BlockKind::Stone)),
                b - 9,
                row,
            ),
        ),
        (
            (b - 4, row),
            place(
                Buildable::Tile(BuiltTile::Wall(BlockKind::Stone)),
                b - 9,
                row - 1,
            ),
        ),
        (
            (b - 4, row),
            place(
                Buildable::Tile(BuiltTile::Wall(BlockKind::Stone)),
                b - 10,
                row,
            ),
        ),
    ];
    for (from, command) in steps {
        build(world, player, from, command);
    }
    if let Some(home) = module_id(world, ModuleKind::Homestead) {
        build(
            world,
            player,
            (b + 4, row),
            BuildCommand::Use {
                structure: home,
                service: fatherload_simulation::base::ModuleService::SetHome,
            },
        );
    }
}

/// What the underground scene shows on top of the outpost.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Ui {
    None,
    BuildFits,
    BuildRefused,
    StationMenu,
}

/// The underground outpost's floor row (rigs park on `FLOOR - 1`).
const FLOOR: i32 = 66;
const ROOM_LEFT: i32 = 96;
const ROOM_RIGHT: i32 = 118;

/// A plated outpost room deep underground with a fuel station.
fn underground_base(stage: &mut SceneStage<'_>, ui: Ui) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        build_outpost(world, player);
        let row = FLOOR - 1;
        stand(
            world,
            player,
            if ui == Ui::StationMenu { 108 } else { 106 },
            row,
        );
        if let Some(rig) = world.player_mut(player) {
            rig.fuel = rig.fuel_capacity * 0.18;
        }
    });
    match ui {
        Ui::None => {}
        Ui::BuildFits => {
            let selected = palette_index(Buildable::Tile(BuiltTile::Plating));
            stage.seat(0).build = Some(BuildDraft {
                selected,
                facing: Facing::Right,
                offset: (-4, -3),
                pointed: None,
            });
        }
        Ui::BuildRefused => {
            stage.use_devices(&[XBOX]);
            let selected = palette_index(Buildable::Module(ModuleKind::RepairBay));
            stage.seat(0).build = Some(BuildDraft {
                selected,
                facing: Facing::Right,
                offset: (1, 0),
                pointed: None,
            });
        }
        Ui::StationMenu => {
            let station = module_id(stage.game.world(), ModuleKind::FuelStation);
            let seat = stage.seat(0);
            seat.base_module = station;
            seat.modal = Some(ModalScreen::BaseModule);
            seat.selected_menu_item = 0;
        }
    }
    stage.settle(50);
}

fn palette_index(item: Buildable) -> usize {
    Buildable::PALETTE
        .iter()
        .position(|candidate| *candidate == item)
        .unwrap_or(0)
}

fn build_outpost(world: &mut WorldState, player: PlayerId) {
    // A cavern room in solid stone, with a shaft up from its right end.
    for y in FLOOR - 14..FLOOR + 3 {
        for x in ROOM_LEFT - 4..ROOM_RIGHT + 5 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Stone);
        }
    }
    carve(world, ROOM_LEFT..ROOM_RIGHT + 1, FLOOR - 9..FLOOR);
    carve(world, ROOM_RIGHT - 1..ROOM_RIGHT, FLOOR - 14..FLOOR - 9);
    reveal(
        world,
        player,
        ROOM_LEFT - 8..ROOM_RIGHT + 8,
        FLOOR - 16..FLOOR + 4,
    );
    unlock_depth(world);
    if let Some(rig) = world.player_mut(player) {
        rig.credits = 50_000;
        rig.stow(ItemKind::Block(BlockKind::Stone), 40);
    }
    let row = FLOOR - 1;
    let beacon = 107;
    build(
        world,
        player,
        (beacon, row),
        place(Buildable::Module(ModuleKind::Beacon), beacon, row),
    );
    // Back walls over most of the room.
    for y in FLOOR - 8..FLOOR {
        for x in ROOM_LEFT + 1..ROOM_RIGHT {
            let block = if y <= FLOOR - 7 {
                BlockKind::HardRock
            } else {
                BlockKind::Stone
            };
            build(
                world,
                player,
                (x, row),
                place(Buildable::BackWall(block), x, y),
            );
        }
    }
    // Plated end walls with a door, and a plank mezzanine.
    for y in (FLOOR - 8..FLOOR - 1).filter(|y| *y != FLOOR - 4) {
        build(
            world,
            player,
            (ROOM_LEFT + 3, row),
            place(Buildable::Tile(BuiltTile::Plating), ROOM_LEFT + 1, y),
        );
    }
    build(
        world,
        player,
        (ROOM_LEFT + 3, row),
        place(Buildable::Fixture(Fixture::Door), ROOM_LEFT + 1, row),
    );
    for x in ROOM_LEFT + 2..ROOM_LEFT + 7 {
        build(
            world,
            player,
            (x + 2, row),
            place(Buildable::Tile(BuiltTile::Plank), x, FLOOR - 5),
        );
    }
    build(
        world,
        player,
        (ROOM_LEFT + 4, row),
        place(Buildable::Tile(BuiltTile::Glass), ROOM_LEFT + 1, FLOOR - 4),
    );
    let steps = [
        (
            (103, row),
            place(Buildable::Module(ModuleKind::Generator), 102, row),
        ),
        (
            (106, row),
            place(Buildable::Module(ModuleKind::FuelStation), 109, row),
        ),
        (
            (112, row),
            place(Buildable::Module(ModuleKind::Bed), 113, row),
        ),
        (
            (112, row),
            place(Buildable::Module(ModuleKind::Storage), 115, row),
        ),
        (
            (100, row),
            place(Buildable::Fixture(Fixture::Table), 99, row),
        ),
        (
            (100, row),
            place(Buildable::Fixture(Fixture::Crates), 100, FLOOR - 6),
        ),
        (
            (100, row),
            place(Buildable::Fixture(Fixture::Plant), 102, FLOOR - 6),
        ),
        (
            (103, row - 2),
            place_facing(
                Buildable::Fixture(Fixture::Light),
                101,
                FLOOR - 9,
                Facing::Up,
            ),
        ),
        (
            (108, row - 2),
            place_facing(
                Buildable::Fixture(Fixture::Light),
                108,
                FLOOR - 9,
                Facing::Up,
            ),
        ),
        (
            (114, row - 2),
            place_facing(
                Buildable::Fixture(Fixture::Light),
                115,
                FLOOR - 9,
                Facing::Up,
            ),
        ),
        (
            (112, row),
            place(Buildable::Fixture(Fixture::Banner), 112, FLOOR - 6),
        ),
    ];
    for (from, command) in steps {
        build(world, player, from, command);
    }
    // A lift ladder up the shaft (to the claim's top edge).
    for y in FLOOR - 11..FLOOR {
        build(
            world,
            player,
            (ROOM_RIGHT - 2, (y + 4).min(row)),
            place(Buildable::Fixture(Fixture::Ladder), ROOM_RIGHT - 1, y),
        );
    }
    if let Some(bed) = module_id(world, ModuleKind::Bed) {
        build(
            world,
            player,
            (113, row),
            BuildCommand::Use {
                structure: bed,
                service: fatherload_simulation::base::ModuleService::SetHome,
            },
        );
    }
}

/// The claim beacon selected on open meadow: the claim it would stake.
fn claim_preview(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        meadow(world);
        reveal(world, player, 4..48, 0..GROUND + 8);
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 5_000;
        }
        stand(world, player, 26, GROUND - 1);
    });
    stage.seat(0).build = Some(BuildDraft {
        selected: palette_index(Buildable::Module(ModuleKind::Beacon)),
        facing: Facing::Right,
        offset: (2, 0),
        pointed: None,
    });
    stage.settle(30);
}

/// Two seats at the outpost: seat 1 building a wall, seat 2 at the generator.
fn coop_split(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        let player = seats[0].player;
        build_outpost(world, player);
        let row = FLOOR - 1;
        stand(world, player, 105, row);
        let other = seats[1].player;
        reveal(
            world,
            other,
            ROOM_LEFT - 8..ROOM_RIGHT + 8,
            FLOOR - 16..FLOOR + 4,
        );
        stand(world, other, 103, row);
        if let Some(rig) = world.player_mut(other) {
            rig.x -= TILE_SIZE * 0.3;
        }
    });
    stage.use_devices(&[WASD, ARROWS]);
    stage.seat(0).build = Some(BuildDraft {
        selected: palette_index(Buildable::Tile(BuiltTile::Wall(BlockKind::Clay))),
        facing: Facing::Right,
        offset: (0, -3),
        pointed: None,
    });
    stage.settle(40);
}
