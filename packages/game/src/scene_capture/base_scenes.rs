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
    worldgen::biome::{self, Biome},
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
        name: "base_hillside_homestead",
        summary: "A claim on a hilltop terrace far from town: homestead, generator, chest and lamps on biome ground above the town's ground line, slopes falling away either side",
        setup: hillside_base,
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
        name: "base_refinery_workshop",
        summary: "An underground works: a refinery cooking ore beside two generators, and a workshop bench",
        setup: |stage| works(stage, false),
    },
    Scene {
        name: "base_workshop_menu",
        summary: "The workshop's menu: pressing hopper blocks into bombs, C4 and mining charges",
        setup: |stage| works(stage, true),
    },
    Scene {
        name: "wave2_outpost_skirmish",
        summary: "Every wave 2 system at once: an underground outpost, its rig outside the door firing a rivet gun at cave bats and a spitter, blasted loot drops on the floor",
        setup: outpost_skirmish,
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

const fn place(item: Buildable, x: i32, y: i32) -> BuildCommand {
    place_facing(item, x, y, Facing::Right)
}

const fn place_facing(item: Buildable, x: i32, y: i32, facing: Facing) -> BuildCommand {
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

/// A surface homestead claim west of town built through the real tick (for other tracks'
/// scenes, such as the world clock's sleep vote): `(homestead, its standing column, row)`.
pub(super) fn homestead_claim(
    world: &mut WorldState,
    player: PlayerId,
) -> (Option<StructureId>, i32, i32) {
    meadow(world);
    reveal(world, player, 4..48, 0..GROUND + 8);
    unlock_depth(world);
    if let Some(rig) = world.player_mut(player) {
        rig.credits = 50_000;
        rig.stow(ItemKind::Block(BlockKind::Stone), 20);
    }
    let row = GROUND - 1;
    build_homestead(world, player, row);
    (
        module_id(world, ModuleKind::Homestead),
        HOME_BEACON + 4,
        row,
    )
}

/// Runs one real tick of `command` from `player` standing at `from` (scenes of other tracks).
pub(super) fn act_at(
    world: &mut WorldState,
    player: PlayerId,
    from: (i32, i32),
    command: BuildCommand,
) {
    build(world, player, from, command);
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
    build_homestead_at(world, player, HOME_BEACON, row);
}

fn build_homestead_at(world: &mut WorldState, player: PlayerId, b: i32, row: i32) {
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

/// A hilltop (ground at least eight rows above the town's) away from villages, searching
/// outward from the town: its column and ground row.
fn find_hilltop(world: &WorldState) -> (i32, i32) {
    let settings = *world.world_settings();
    let terrain = world.terrain();
    let high = |x: i32| terrain.surface_row(x) <= GROUND - 8;
    for distance in (300..30_000).step_by(8) {
        for x in [distance, -distance] {
            let biome = biome::biome_at(&settings, x);
            if biome != Biome::Mountains
                && (x - 14..=x + 14).step_by(2).all(high)
                && !fatherload_simulation::worldgen::streamed::in_village_area(
                    &settings,
                    TilePosition {
                        x,
                        y: terrain.surface_row(x),
                    },
                    60,
                )
            {
                return (x, terrain.surface_row(x));
            }
        }
    }
    (HOME_BEACON, GROUND)
}

/// A homestead on a terrace cut into a hilltop: the rig levels a pad at the hill's ground row
/// (digging into the rise, filling the dips), then builds through the real tick.
fn hillside_base(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let (beacon, ground) = find_hilltop(world);
        for x in beacon - 12..beacon + 15 {
            for y in ground - 12..ground {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Air);
            }
            for y in ground..ground + 3 {
                let position = TilePosition { x, y };
                if !world.terrain().is_solid_at(position) {
                    let _ = world.terrain_mut().set_kind(position, TileKind::Dirt);
                }
            }
        }
        reveal(
            world,
            player,
            beacon - 50..beacon + 50,
            ground - 30..ground + 12,
        );
        unlock_depth(world);
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 50_000;
            rig.stow(ItemKind::Block(BlockKind::Stone), 20);
        }
        build_homestead_at(world, player, beacon, ground - 1);
        stand(world, player, beacon + 9, ground - 1);
        world.stream_terrain_near_players();
    });
    stage.settle(50);
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

#[allow(
    clippy::too_many_lines,
    reason = "the outpost is laid out piece by piece"
)]
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

/// A second underground claim fitted out for industry: two generators run a refinery that is
/// cooking a load of ore, next to a workshop bench; `menu` opens the workshop's menu.
#[allow(
    clippy::too_many_lines,
    reason = "the works are laid out piece by piece"
)]
fn works(stage: &mut SceneStage<'_>, menu: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        for y in FLOOR - 10..FLOOR + 3 {
            for x in ROOM_LEFT - 4..ROOM_RIGHT + 5 {
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x, y }, TileKind::Stone);
            }
        }
        carve(world, ROOM_LEFT + 2..ROOM_RIGHT - 1, FLOOR - 6..FLOOR);
        reveal(
            world,
            player,
            ROOM_LEFT - 8..ROOM_RIGHT + 8,
            FLOOR - 12..FLOOR + 4,
        );
        unlock_depth(world);
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 50_000;
            rig.stow(ItemKind::Block(BlockKind::Stone), 60);
            rig.cargo_bay
                .add(fatherload_simulation::terrain::MineralKind::Gold, 6);
            rig.cargo_bay
                .add(fatherload_simulation::terrain::MineralKind::Silver, 4);
        }
        let row = FLOOR - 1;
        build(
            world,
            player,
            (107, row),
            place(Buildable::Module(ModuleKind::Beacon), 107, row),
        );
        for y in FLOOR - 5..FLOOR {
            for x in ROOM_LEFT + 3..ROOM_RIGHT - 2 {
                let block = if y == FLOOR - 5 {
                    BlockKind::HardRock
                } else {
                    BlockKind::Clay
                };
                build(
                    world,
                    player,
                    (x, row),
                    place(Buildable::BackWall(block), x, y),
                );
            }
        }
        let steps = [
            (
                (101, row),
                place(Buildable::Module(ModuleKind::Generator), 99, row),
            ),
            (
                (103, row),
                place(Buildable::Module(ModuleKind::Generator), 101, row),
            ),
            (
                (110, row),
                place(Buildable::Module(ModuleKind::Refinery), 109, row),
            ),
            (
                (113, row),
                place(Buildable::Module(ModuleKind::Workshop), 113, row),
            ),
            (
                (106, row),
                place(Buildable::Fixture(Fixture::Crates), 105, row),
            ),
            (
                (104, row - 2),
                place_facing(
                    Buildable::Fixture(Fixture::Light),
                    104,
                    FLOOR - 6,
                    Facing::Up,
                ),
            ),
            (
                (110, row - 2),
                place_facing(
                    Buildable::Fixture(Fixture::Light),
                    111,
                    FLOOR - 6,
                    Facing::Up,
                ),
            ),
            (
                (106, row),
                place(Buildable::Fixture(Fixture::Banner), 107, FLOOR - 4),
            ),
        ];
        for (from, command) in steps {
            build(world, player, from, command);
        }
        if let Some(refinery) = module_id(world, ModuleKind::Refinery) {
            build(
                world,
                player,
                (110, row),
                BuildCommand::Use {
                    structure: refinery,
                    service: fatherload_simulation::base::ModuleService::RefineOre,
                },
            );
        }
        // Let a few lumps cook so ingots stack in the moulds.
        for _ in 0..(60 * 7) {
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &TickInputs::default());
        }
        stand(world, player, if menu { 114 } else { 107 }, row);
    });
    if menu {
        let workshop = module_id(stage.game.world(), ModuleKind::Workshop);
        let seat = stage.seat(0);
        seat.base_module = workshop;
        seat.modal = Some(ModalScreen::BaseModule);
        seat.selected_menu_item = 1;
    }
    stage.settle(50);
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
            rig.x = TILE_SIZE.mul_add(-0.3, rig.x);
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

/// Wave 2 together: the fuel-station outpost (WP-BASE) with a dug-out gallery west of its door,
/// loot drops (WP-DROPS) lying where a blast threw them, and the rig at the door firing its rivet
/// gun (WP-WEAPONS) at cave bats and an acid spitter (WP-ENEMIES) coming up the gallery.
fn outpost_skirmish(stage: &mut SceneStage<'_>) {
    use fatherload_simulation::{
        AmmoKind, EnemyKind, WeaponKind,
        item::ItemStack,
        terrain::{ArtifactKind, MineralKind},
        weapon::MAX_BATTERY_LEVEL,
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        build_outpost(world, player);
        let row = FLOOR - 1;
        // The gallery west of the door, with a lumpy roof.
        for x in ROOM_LEFT - 30..=ROOM_LEFT {
            let roof =
                FLOOR - 7 + i32::from(x.rem_euclid(7) == 3) + i32::from(x.rem_euclid(11) == 5);
            for y in FLOOR - 12..FLOOR + 2 {
                let kind = if (roof..FLOOR).contains(&y) {
                    TileKind::Air
                } else {
                    TileKind::Stone
                };
                let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
            }
        }
        reveal(
            world,
            player,
            ROOM_LEFT - 36..ROOM_RIGHT + 8,
            FLOOR - 16..FLOOR + 16,
        );
        stand(world, player, ROOM_LEFT - 3, row);
        if let Some(rig) = world.player_mut(player) {
            let _ = rig.receive_weapon(WeaponKind::RivetGun);
            rig.weapon_rack.battery_level = MAX_BATTERY_LEVEL;
            rig.weapon.energy = rig.battery_capacity();
            rig.cargo_bay.add(AmmoKind::Rocket.item(), 4);
        }
        // Loot on the gallery floor: what a charge further in threw out.
        let floor_y = TILE_SIZE.mul_add(f32::from(i16::try_from(FLOOR).unwrap_or(0)), -7.0);
        for (index, stack) in [
            ItemStack::new(ItemKind::Mineral(MineralKind::Gold), 3),
            ItemStack::new(ItemKind::Mineral(MineralKind::Emerald), 1),
            ItemStack::new(ItemKind::Block(BlockKind::Stone), 4),
            ItemStack::new(ItemKind::Artifact(ArtifactKind::Fossil), 1),
            ItemStack::new(ItemKind::Mineral(MineralKind::Silver), 2),
            ItemStack::new(ItemKind::Block(BlockKind::Dirt), 6),
        ]
        .into_iter()
        .enumerate()
        {
            let column = ROOM_LEFT - 19 + 2 * i32::try_from(index).unwrap_or(0);
            let _ = world.spawn_item_drop(tile_centre(column), floor_y, (0.0, 0.0), stack, None);
        }
        // The hostiles coming up the gallery.
        for (column, height) in [
            (ROOM_LEFT - 12, 4),
            (ROOM_LEFT - 15, 5),
            (ROOM_LEFT - 18, 3),
        ] {
            let _ = world.spawn_enemy(
                EnemyKind::CaveBat,
                tile_centre(column),
                tile_centre(FLOOR - height),
            );
        }
        let spitter = EnemyKind::AcidSpitter;
        let _ = world.spawn_enemy(
            spitter,
            tile_centre(ROOM_LEFT - 20),
            TILE_SIZE.mul_add(
                f32::from(i16::try_from(FLOOR).unwrap_or(0)),
                -spitter.stats().radius,
            ) - 1.0,
        );
    });
    stage.use_devices(&[XBOX]);
    stage.settle(6);
    let firing = crate::input::PlayerInput {
        fire: true,
        aim_x: -1.0,
        aim_y: -0.12,
        ..crate::input::PlayerInput::default()
    };
    stage.hold(22, firing);
}
