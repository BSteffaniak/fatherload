//! Night scenes (WP-NIGHT): each family of night monster in its biome under the night sky, the
//! umbral alpha on a new moon, a lit palisade base holding the night off, the night market's
//! lanterns and its counter, nocturnal ore glowing in a hillside, and a full-moon night falling
//! over the plains.

use fatherload_simulation::{
    EnemyKind, PlayerId, SimulationTick, WorldState,
    base::{BuildCommand, Buildable, BuiltTile, Facing, Fixture, ModuleKind},
    clock::{MoonPhase, WorldClock},
    enemy::EnemyMode,
    entity::{EntityId, EntityState},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    village::{BuildingKind, Village, VillageSetting},
    worldgen::biome::Biome,
};

use super::{
    Scene, SceneStage, base_scenes::act_at, place_rig, reveal, surface_scenes::find_biome,
    village_scenes::find_village,
};
use crate::{input::PlayerInput, shell::ModalScreen};

/// World time of slice `slice` (0..96, fractional) on the first day from day 1 whose Moon is
/// `moon`, for the world's seed.
fn time_of(world: &WorldState, slice: f32, moon: MoonPhase) -> u64 {
    let length = world.day_length();
    let seed = world.terrain().seed();
    let day = (0..8)
        .find(|day| {
            WorldClock::from_world_millis(day * length.millis(), length, seed).moon_phase() == moon
        })
        .unwrap_or(0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "a slice of a day in milliseconds"
    )]
    let into_day = (f64::from(slice) * length.millis() as f64 / 96.0) as u64;
    day * length.millis() + into_day
}

/// World pixel x of the centre of column `x`.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn column_x(x: i32) -> f32 {
    (x as f32 + 0.5) * TILE_SIZE
}

/// World pixel y of a walker of `kind` standing on ground row `ground`.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn standing_y(kind: EnemyKind, ground: i32) -> f32 {
    (ground as f32).mul_add(TILE_SIZE, -kind.stats().radius) - 1.0
}

/// World pixel y `rows` tiles above ground row `ground`.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn above(ground: i32, rows: f32) -> f32 {
    (ground as f32 - rows) * TILE_SIZE
}

/// Levels columns `xs` to a flat meadow whose ground is row `ground` (grass-topped dirt).
fn flatten(world: &mut WorldState, xs: std::ops::Range<i32>, ground: i32) {
    for x in xs {
        for y in ground - 30..ground {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
        for y in ground..ground + 4 {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Dirt);
        }
    }
}

/// Freezes an enemy in `mode` facing `facing` (its tells hold for the capture).
fn hold_mode(world: &mut WorldState, id: EntityId, mode: EnemyMode, facing: i8) {
    world.entities_mut().update(id, |entity| {
        if let EntityState::Enemy(state) = &mut entity.state {
            state.mode = mode;
            state.facing = facing;
            state.mode_until = SimulationTick::new(u64::MAX / 2);
            state.attack_ready = SimulationTick::new(u64::MAX / 2);
        }
    });
}

/// Sets an enemy moving.
fn launch(world: &mut WorldState, id: EntityId, velocity_x: f32, velocity_y: f32) {
    world.entities_mut().update(id, |entity| {
        entity.body.velocity_x = velocity_x;
        entity.body.velocity_y = velocity_y;
    });
}

/// Takes the night-falls banner down (portraits show the monsters, not the banner).
fn clear_banner(stage: &mut SceneStage<'_>) {
    for seat in 0..2 {
        if seat < stage.game.shell.seats.len() {
            stage.seat(seat).fanfare = None;
        }
    }
    stage.settle(1);
}

/// A night in `biome` under `moon`: seat 0's rig parked on the ground of the first stretch of
/// the biome, the area explored, the clock at 22:00, then `arrange` places the monsters
/// (given the rig's column and ground row), then `frames` frames of the night.
fn night_in(
    stage: &mut SceneStage<'_>,
    biome: Biome,
    moon: MoonPhase,
    frames: usize,
    arrange: impl FnOnce(&mut WorldState, PlayerId, i32, i32) + 'static,
) {
    stage.play(1, move |world, seats| {
        let player = seats[0].player;
        let x = find_biome(world, biome, 70);
        let ground = world.terrain().surface_row(x);
        place_rig(world, player, x, ground - 1);
        reveal(world, player, x - 40..x + 40, ground - 30..ground + 25);
        world.stream_terrain_near_players();
        let time = time_of(world, 88.0, moon);
        world.set_world_time_millis(time);
        arrange(world, player, x, ground);
    });
    stage.settle(frames);
    clear_banner(stage);
}

fn shadow_hounds(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Plains,
        MoonPhase::WaxingGibbous,
        10,
        |world, _, x, _| {
            for (dx, mode, facing) in [
                (7, EnemyMode::Windup, -1),
                (11, EnemyMode::Chase, -1),
                (-9, EnemyMode::Chase, 1),
            ] {
                let column = x + dx;
                let ground = world.terrain().surface_row(column);
                let hound = world.spawn_enemy(
                    EnemyKind::ShadowHound,
                    column_x(column),
                    standing_y(EnemyKind::ShadowHound, ground),
                );
                if mode == EnemyMode::Windup {
                    hold_mode(world, hound, mode, facing);
                }
            }
        },
    );
}

/// Night monsters pressing on the rig stop at contact (master's body rules hold for wave 4's
/// families): two hounds dropped right beside it and a will-o'-wisp over it, all hunting it,
/// are pushed out to touching (they still bite), never drawn inside it.
fn night_contact(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Plains,
        MoonPhase::WaxingGibbous,
        24,
        |world, player, x, _| {
            let Some((rx, ry)) = world.player(player).map(|rig| (rig.x, rig.y)) else {
                return;
            };
            let now = world.simulation_tick();
            for (kind, dx, dy) in [
                (EnemyKind::ShadowHound, 10.0, 0.0),
                (EnemyKind::ShadowHound, -12.0, 0.0),
                (EnemyKind::WillOWisp, 2.0, -26.0),
            ] {
                let column = x + if dx > 0.0 { 1 } else { -1 };
                let ground = world.terrain().surface_row(column);
                let y = if kind == EnemyKind::ShadowHound {
                    standing_y(kind, ground)
                } else {
                    ry + dy
                };
                let id = world.spawn_enemy(kind, rx + dx, y);
                world.entities_mut().update(id, |entity| {
                    if let EntityState::Enemy(state) = &mut entity.state {
                        state.mode = EnemyMode::Chase;
                        state.target = Some(player);
                        state.last_seen = now;
                        state.attack_ready = now;
                    }
                });
            }
        },
    );
}

fn dune_stalkers(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Desert,
        MoonPhase::FirstQuarter,
        6,
        |world, _, x, ground| {
            // One bursting out of the sand toward the rig ...
            let erupting =
                world.spawn_enemy(EnemyKind::DuneStalker, column_x(x + 5), above(ground, 1.2));
            hold_mode(world, erupting, EnemyMode::Strike, -1);
            launch(world, erupting, -120.0, -160.0);
            // ... and one rippling under the dune, gathering itself.
            let column = x - 8;
            let row = world.terrain().surface_row(column) + 1;
            let buried =
                world.spawn_enemy(EnemyKind::DuneStalker, column_x(column), above(row, -0.5));
            hold_mode(world, buried, EnemyMode::Windup, 1);
        },
    );
}

fn will_o_wisps(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Wetlands,
        MoonPhase::WaningCrescent,
        8,
        |world, _, x, ground| {
            for (dx, rows, mode) in [
                (6, 3.5, EnemyMode::Windup),
                (10, 5.0, EnemyMode::Idle),
                (-7, 4.0, EnemyMode::Idle),
                (-12, 2.5, EnemyMode::Idle),
            ] {
                let wisp =
                    world.spawn_enemy(EnemyKind::WillOWisp, column_x(x + dx), above(ground, rows));
                if mode == EnemyMode::Windup {
                    hold_mode(world, wisp, mode, -1);
                }
            }
        },
    );
}

fn moth_swarms(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Forest,
        MoonPhase::FirstQuarter,
        14,
        |world, _, x, ground| {
            for (dx, rows) in [(3, 2.5), (-4, 3.0), (9, 4.0)] {
                let _ =
                    world.spawn_enemy(EnemyKind::MothSwarm, column_x(x + dx), above(ground, rows));
            }
        },
    );
}

fn umbral_alpha(stage: &mut SceneStage<'_>) {
    night_in(
        stage,
        Biome::Badlands,
        MoonPhase::New,
        8,
        |world, _, x, _| {
            let column = x + 9;
            let ground = world.terrain().surface_row(column);
            let alpha = world.spawn_enemy(
                EnemyKind::UmbralAlpha,
                column_x(column),
                standing_y(EnemyKind::UmbralAlpha, ground),
            );
            hold_mode(world, alpha, EnemyMode::Windup, -1);
            for dx in [5, 14] {
                let column = x + dx;
                let ground = world.terrain().surface_row(column);
                let _ = world.spawn_enemy(
                    EnemyKind::ShadowHound,
                    column_x(column),
                    standing_y(EnemyKind::ShadowHound, ground),
                );
            }
        },
    );
}

/// Places `item` at `(x, y)` from the rig standing at `from` through the real tick.
fn put(
    world: &mut WorldState,
    player: PlayerId,
    from: (i32, i32),
    item: Buildable,
    at: (i32, i32),
) {
    let facing = if matches!(item, Buildable::Fixture(Fixture::Light)) {
        Facing::Down
    } else {
        Facing::Right
    };
    act_at(
        world,
        player,
        from,
        BuildCommand::Place {
            item,
            at: TilePosition { x: at.0, y: at.1 },
            facing,
        },
    );
}

/// A lit palisade base out on the plains at night: a claim fenced with palisades either side,
/// lamps on posts and a generator, the rig inside; shadow hounds lurk at the edge of the light
/// beyond the fence and a moth swarm circles a lamp.
fn palisade_base(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let x = find_biome(world, Biome::Plains, 70);
        let ground = world.terrain().surface_row(x);
        flatten(world, x - 26..x + 27, ground);
        reveal(world, player, x - 40..x + 40, ground - 30..ground + 25);
        world.stream_terrain_near_players();
        if let Some(rig) = world.player_mut(player) {
            rig.credits = 100_000;
        }
        let row = ground - 1;
        put(
            world,
            player,
            (x, row),
            Buildable::Module(ModuleKind::Beacon),
            (x, row),
        );
        put(
            world,
            player,
            (x - 3, row),
            Buildable::Module(ModuleKind::Generator),
            (x - 6, row),
        );
        put(
            world,
            player,
            (x + 3, row),
            Buildable::Module(ModuleKind::Storage),
            (x + 3, row),
        );
        for side in [-1, 1] {
            let fence = x + side * 9;
            for dy in 0..3 {
                put(
                    world,
                    player,
                    (x + side * 5, row),
                    Buildable::Tile(BuiltTile::Palisade),
                    (fence, row - dy),
                );
            }
            // A lamp hung from a plank post just inside the fence.
            let post = x + side * 7;
            put(
                world,
                player,
                (x + side * 4, row),
                Buildable::Tile(BuiltTile::Plank),
                (post, row - 3),
            );
            put(
                world,
                player,
                (x + side * 4, row),
                Buildable::Fixture(Fixture::Light),
                (post, row - 2),
            );
        }
        put(
            world,
            player,
            (x, row),
            Buildable::Fixture(Fixture::Banner),
            (x + 1, row - 1),
        );
        place_rig(world, player, x - 2, row);
        let time = time_of(world, 87.0, MoonPhase::WaxingGibbous);
        world.set_world_time_millis(time);
        // Hounds held off at the edge of the light, eyes in the dark.
        for (dx, facing) in [(12, -1), (14, -1), (17, -1), (-13, 1), (-16, 1)] {
            let column = x + dx;
            let hound = world.spawn_enemy(
                EnemyKind::ShadowHound,
                column_x(column),
                standing_y(EnemyKind::ShadowHound, ground),
            );
            hold_mode(world, hound, EnemyMode::Idle, facing);
        }
        let _moths = world.spawn_enemy(EnemyKind::MothSwarm, column_x(x + 7), above(ground, 3.5));
    });
    stage.settle(16);
    clear_banner(stage);
}

/// A surface village at night with its trading post turned night market.
fn night_market_street(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let found = find_village(world, |found: &Village| {
            found.setting == VillageSetting::Surface
                && found
                    .buildings
                    .iter()
                    .any(|b| b.kind == BuildingKind::Market)
        });
        let market = found
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Market)
            .map_or(found.plaza.0, |b| b.first_column + b.width / 2);
        super::village_scenes::park_in(world, player, &found, market - 7);
        world.stream_terrain_near_players();
        let time = time_of(world, 86.0, MoonPhase::WaxingGibbous);
        world.set_world_time_millis(time);
    });
    stage.settle(20);
    stage.seat(0).village.banner = None;
    clear_banner(stage);
}

/// Inside the trading post at night: the night market's counter.
fn night_market_counter(stage: &mut SceneStage<'_>) {
    let mut chosen = None;
    stage.play(1, |world, seats| {
        let found = find_village(world, |found: &Village| {
            found.setting == VillageSetting::Surface
                && found
                    .buildings
                    .iter()
                    .any(|b| b.kind == BuildingKind::Market)
        });
        let building = found
            .buildings
            .iter()
            .position(|lot| lot.kind == BuildingKind::Market)
            .expect("market");
        super::village_scenes::park_in(
            world,
            seats[0].player,
            &found,
            found.buildings[building].first_column + 2,
        );
        if let Some(rig) = world.player_mut(seats[0].player) {
            use fatherload_simulation::terrain::StrategicResourceKind;
            rig.cargo_bay.add(StrategicResourceKind::ShadowPelt, 3);
            rig.cargo_bay.add(StrategicResourceKind::MothSilk, 5);
            rig.cargo_bay.add(StrategicResourceKind::WispEssence, 1);
        }
        let time = time_of(world, 86.0, MoonPhase::WaxingGibbous);
        world.set_world_time_millis(time);
        chosen = Some((found, building));
    });
    stage.settle(5);
    let (found, building) = chosen.expect("staged");
    super::village_scenes::inside(stage, &found, building, crate::shell::VILLAGE_COUNTER_X);
    stage.seat(0).modal = Some(ModalScreen::VillageCounter);
    stage.settle(4);
    clear_banner(stage);
}

/// A hillside at night with shallow silver, emerald and diamond seams glowing through the turf.
fn nocturnal_ore(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let x = find_biome(world, Biome::Forest, 70);
        let ground = world.terrain().surface_row(x);
        place_rig(world, player, x, ground - 1);
        world.stream_terrain_near_players();
        let seams = [
            (4, 2, MineralKind::Silver),
            (5, 2, MineralKind::Silver),
            (5, 3, MineralKind::Silver),
            (-6, 3, MineralKind::Emerald),
            (-7, 4, MineralKind::Emerald),
            (9, 5, MineralKind::Diamond),
            (-2, 6, MineralKind::Silver),
            (-1, 6, MineralKind::Silver),
            (12, 3, MineralKind::Platinum),
            (2, 8, MineralKind::Copper),
        ];
        for (dx, dy, mineral) in seams {
            let column = x + dx;
            let row = world.terrain().surface_row(column) + dy;
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x: column, y: row }, TileKind::Ore(mineral));
        }
        reveal(world, player, x - 40..x + 40, ground - 30..ground + 1);
        let time = time_of(world, 2.0, MoonPhase::WaningCrescent);
        world.set_world_time_millis(time);
    });
    // The night shows the seams on the map within a second.
    stage.settle(70);
    clear_banner(stage);
}

/// Full moon over the plains: night falling as the banner warns of a restless night, the Moon
/// huge in the sky and the first hounds rising in the distance.
fn full_moon(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let column = world.moon_anchor().column;
        // The plains nearest the Moon's column, so the full Moon hangs overhead.
        let x = (0..400)
            .flat_map(|step| [column + step * 8, column - step * 8])
            .find(|x| {
                fatherload_simulation::worldgen::biome::biome_at(world.world_settings(), *x)
                    == Biome::Plains
            })
            .unwrap_or(column);
        let ground = world.terrain().surface_row(x);
        place_rig(world, player, x, ground - 1);
        reveal(world, player, x - 40..x + 40, ground - 30..ground + 25);
        world.stream_terrain_near_players();
        // Just before the monsters rise: the banner comes up a moment into the scene.
        let time = time_of(world, 75.49, MoonPhase::Full);
        world.set_world_time_millis(time);
        for dx in [13, 17, -15] {
            let column = x + dx;
            let ground = world.terrain().surface_row(column);
            let _ = world.spawn_enemy(
                EnemyKind::ShadowHound,
                column_x(column),
                standing_y(EnemyKind::ShadowHound, ground),
            );
        }
        let _moths = world.spawn_enemy(EnemyKind::MothSwarm, column_x(x - 6), above(ground, 4.0));
    });
    stage.hold(50, PlayerInput::default());
}

/// Split-screen at night: one crewmate safe under the lamps of a home, one out on the open
/// ground with hounds closing in.
fn coop_night(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        let (first, second) = (seats[0].player, seats[1].player);
        let x = find_biome(world, Biome::Tundra, 70);
        let ground = world.terrain().surface_row(x);
        flatten(world, x - 10..x + 11, ground);
        world.stream_terrain_near_players();
        if let Some(rig) = world.player_mut(first) {
            rig.credits = 100_000;
        }
        let row = ground - 1;
        put(
            world,
            first,
            (x, row),
            Buildable::Module(ModuleKind::Beacon),
            (x, row),
        );
        put(
            world,
            first,
            (x + 2, row),
            Buildable::Module(ModuleKind::Bed),
            (x + 2, row),
        );
        put(
            world,
            first,
            (x - 2, row),
            Buildable::Fixture(Fixture::Light),
            (x - 3, row - 1),
        );
        place_rig(world, first, x - 1, row);
        reveal(world, first, x - 40..x + 40, ground - 30..ground + 25);
        let far = x + 60;
        let far_ground = world.terrain().surface_row(far);
        place_rig(world, second, far, far_ground - 1);
        reveal(
            world,
            second,
            far - 40..far + 40,
            far_ground - 30..far_ground + 25,
        );
        world.stream_terrain_near_players();
        let time = time_of(world, 88.0, MoonPhase::FirstQuarter);
        world.set_world_time_millis(time);
        for dx in [6, 9, -8] {
            let column = far + dx;
            let ground = world.terrain().surface_row(column);
            let _ = world.spawn_enemy(
                EnemyKind::ShadowHound,
                column_x(column),
                standing_y(EnemyKind::ShadowHound, ground),
            );
        }
    });
    stage.settle(12);
    clear_banner(stage);
}

/// The scenes, chained after the main list.
pub const NIGHT_SCENES: &[Scene] = &[
    Scene {
        name: "night_shadow_hounds",
        summary: "Shadow hounds on the plains at night: one gathering to pounce at the rig, its mint eyes glinting, the pack closing in from both sides",
        setup: shadow_hounds,
    },
    Scene {
        name: "night_enemies_contact",
        summary: "Shadow hounds and a will-o'-wisp hunting the rig at night press on it and stop at contact, never inside it",
        setup: night_contact,
    },
    Scene {
        name: "night_dune_stalkers",
        summary: "Dune stalkers in the desert at night: one erupting from the sand at the rig, another rippling under the dune",
        setup: dune_stalkers,
    },
    Scene {
        name: "night_will_o_wisps",
        summary: "Will-o'-wisps drifting over the wetlands at night, one brightening to throw wispfire, their cold light on the marsh",
        setup: will_o_wisps,
    },
    Scene {
        name: "night_moth_swarms",
        summary: "Moth swarms drawn to the rig's headlights in the forest at night",
        setup: moth_swarms,
    },
    Scene {
        name: "night_umbral_alpha",
        summary: "A new-moon night in the badlands: the umbral alpha winding up to charge with its hounds at its heels",
        setup: umbral_alpha,
    },
    Scene {
        name: "night_palisade_base",
        summary: "A lit palisade base at night: a claim fenced with palisades and lamps, hounds held off at the edge of the light, moths circling a lamp",
        setup: palisade_base,
    },
    Scene {
        name: "night_market_street",
        summary: "A village at night: the trading post strung with lanterns under its NIGHT MARKET pennant, day counters hung with SHUT boards",
        setup: night_market_street,
    },
    Scene {
        name: "night_market_counter",
        summary: "The night market's counter: night goods bought well, flares, scare charges, a moonlit ward and tonight's curio",
        setup: night_market_counter,
    },
    Scene {
        name: "night_nocturnal_ore",
        summary: "Nocturnal ore at night: shallow silver, emerald, diamond and platinum seams glowing in a forest hillside (copper stays dark)",
        setup: nocturnal_ore,
    },
    Scene {
        name: "night_full_moon",
        summary: "A full moon rising over the plains as night falls: the NIGHT FALLS banner warns of a restless night, hounds in the distance",
        setup: full_moon,
    },
    Scene {
        name: "night_coop_split",
        summary: "Split-screen at night: one rig safe at a lit home, the other out on the tundra with hounds closing in",
        setup: coop_night,
    },
];
