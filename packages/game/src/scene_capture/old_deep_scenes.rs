//! Descent II, the Old Deep (Wave 8): one scene per stratum with its signature rule at work, its
//! materials and a native creature, plus the set pieces: the Cathedral Lift's valves, the
//! Glowmother's den, a Fathers' log and the Star Core fight (shielded, then open).

use fatherload_simulation::{
    EnemyKind, PlayerId, TILE_SIZE, WorldState,
    depth_progression::ProgressionRules,
    descent::Stratum,
    old_deep::{self, DeepTile, sites},
    terrain::{Terrain, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings, descent, descent::landmarks},
};

use super::{Scene, SceneStage, carve, place_rig, reveal, tile_centre};
use crate::input::PlayerInput;

/// Seed of every Old Deep scene world.
const SCENE_SEED: u64 = 0x01D_DEE9;
/// The column the stamped scenes are built around.
const SCENE_X: i32 = -260;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the Old Deep scene world, with its logistics.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
}

/// Fits the rig with Descent II's kit (every protection but no Descent systems).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 12;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(12);
        rig.fuel = rig.fuel_capacity * 0.8;
        rig.engine_level = 10;
        rig.drill_strength = 8;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear {
            rig_systems: 0,
            ..fatherload_simulation::protection::HazardGear::MAXED
        };
        rig.radiator_level = 15;
        rig.hull_level = 12;
        rig.hull = rig.max_hull();
    }
}

const fn middle(stratum: Stratum) -> i32 {
    let (top, bottom) = descent::layout(&settings()).rows(stratum);
    i32::midpoint(top, bottom)
}

fn set(world: &mut WorldState, x: i32, y: i32, kind: TileKind) {
    let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
}

const fn deep(tile: DeepTile) -> TileKind {
    TileKind::Deep(tile)
}

/// A rig in a chamber `half_w` x `half_h` around `(x, y)`, everything near explored.
fn chamber(
    world: &mut WorldState,
    player: PlayerId,
    (x, y): (i32, i32),
    (half_w, half_h): (i32, i32),
) {
    deep_world(world);
    fit(world, player);
    carve(
        world,
        x - half_w..x + half_w + 1,
        y - half_h..y + half_h + 1,
    );
    place_rig(world, player, x, y + half_h);
    finish(world, player, (x - 50..x + 50, y - 30..y + 30));
}

/// Streams the terrain round the rig, then marks `area` explored (tiles of chunks not yet
/// loaded cannot be revealed).
fn finish(
    world: &mut WorldState,
    player: PlayerId,
    area: (std::ops::Range<i32>, std::ops::Range<i32>),
) {
    world.stream_terrain_near_players();
    reveal(world, player, area.0, area.1);
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let _ = world.spawn_enemy(kind, tile_centre(x), tile_centre(y));
}

/// The first region (scanning out from the scene column through the middle of `stratum`) whose
/// feature `find` returns.
fn find_feature<T>(
    stratum: Stratum,
    find: impl Fn(&WorldSettings, &descent::DescentLayout, i32, i32) -> Option<T>,
) -> Option<T> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(stratum);
    let (w, h) = sites::region_size(stratum);
    let row = i32::midpoint(top, bottom);
    (0..80).find_map(|step| {
        let dx = (step / 3) * if step % 2 == 0 { 1 } else { -1 };
        let dy = (step % 3) - 1;
        let rx = SCENE_X.div_euclid(w) + dx;
        let ry = row.div_euclid(h) + dy;
        let _ = (top, bottom);
        find(&settings, &layout, rx, ry)
    })
}

fn crystal_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let y = middle(Stratum::CrystalFaults);
        chamber(world, player, (SCENE_X, y), (9, 3));
        // A prism on the floor ahead of the rig, a crystal fault running off into the rock and
        // latent gems behind it: the lamp splits into beams that light them.
        let floor = y + 4;
        set(world, SCENE_X + 4, y + 3, deep(DeepTile::Prism));
        for step in 0..9 {
            set(
                world,
                SCENE_X + 5 + step,
                floor + step / 2,
                deep(DeepTile::ResonantCrystal),
            );
            set(
                world,
                SCENE_X + 6 + step,
                floor + step / 2,
                deep(DeepTile::ResonantCrystal),
            );
        }
        for (dx, dy) in [(7, 2), (10, 4), (5, 3), (3, -5), (12, -3), (8, 6)] {
            set(world, SCENE_X + dx, y + dy, deep(DeepTile::LatentGem));
        }
        for dx in [-8, -3] {
            set(world, SCENE_X + dx, y - 4, deep(DeepTile::ResonantCrystal));
            set(
                world,
                SCENE_X + dx + 1,
                y - 4,
                deep(DeepTile::ResonantCrystal),
            );
        }
        spawn(world, EnemyKind::PrismMoth, SCENE_X - 8, y - 3);
        world.stream_terrain_near_players();
    });
    // The first tick's deep reward drops a pocket of ore near the rig: clear any that landed
    // in the chamber, so the lamp reaches the prism.
    stage.settle(4);
    stage.play_on(|world| {
        let y = middle(Stratum::CrystalFaults);
        for row in y - 3..=y + 3 {
            for column in SCENE_X - 9..=SCENE_X + 9 {
                if matches!(
                    world
                        .terrain()
                        .tile(TilePosition { x: column, y: row })
                        .map(|t| t.kind),
                    Some(TileKind::Ore(_))
                ) {
                    set(world, column, row, TileKind::Air);
                }
            }
        }
        // And no lava pocket under the chamber floor.
        for row in y + 4..=y + 9 {
            for column in SCENE_X - 12..=SCENE_X + 12 {
                if world
                    .terrain()
                    .tile(TilePosition { x: column, y: row })
                    .map(|t| t.kind)
                    == Some(TileKind::Lava)
                {
                    set(world, column, row, TileKind::Stone);
                }
            }
        }
    });
    stage.settle(330);
}

fn fossil_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let y = middle(Stratum::FossilOceans);
        chamber(world, player, (SCENE_X, y), (10, 3));
        // A serpent skeleton half dug out of its shale bed beside the rig, an amber blob with a
        // trapped moth, and a gas basin breathing at high tide.
        for dy in 4..8 {
            for dx in -10..=10 {
                set(world, SCENE_X + dx, y + dy, deep(DeepTile::Shale));
            }
        }
        for (dx, dy) in [
            (0, 4),
            (1, 4),
            (2, 5),
            (3, 5),
            (4, 4),
            (5, 4),
            (6, 5),
            (7, 5),
            (8, 4),
            (9, 4),
        ] {
            set(world, SCENE_X + dx - 4, y + dy, deep(DeepTile::FossilBone));
        }
        for dx in -3..3 {
            set(world, SCENE_X + dx, y + 4 - 1, TileKind::Air);
        }
        for dy in -3..=-1 {
            for dx in -10..=-7 {
                set(world, SCENE_X + dx, y + dy, deep(DeepTile::Amber));
            }
        }
        for dx in 7..=10 {
            set(world, SCENE_X + dx, y + 3, TileKind::Gas);
        }
        spawn(world, EnemyKind::FossilEel, SCENE_X + 6, y);
        world.stream_terrain_near_players();
        let hz = world.tick_rate_hz();
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
            old_deep::GAS_TIDE_HIGH.0 * u64::from(hz) / 1_000 - 4,
        ));
    });
    stage.settle(330);
}

fn cathedral_nave_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let site = sites::cathedral(&settings(), &descent::layout(&settings())).expect("a nave");
        let x = site.bounds.min.x + 16;
        let floor = site.bounds.max.y;
        place_rig(world, player, x, floor - 1);
        finish(
            world,
            player,
            (
                site.bounds.min.x - 6..site.bounds.max.x + 6,
                site.bounds.min.y - 4..floor + 4,
            ),
        );
        spawn(world, EnemyKind::JetRay, x + 10, floor - 12);
        world.stream_terrain_near_players();
    });
    stage.settle(300);
}

fn cathedral_lift_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let site = sites::cathedral(&settings(), &descent::layout(&settings())).expect("a nave");
        let valve = site.valves[0];
        place_rig(world, player, valve.x + 1, valve.y);
        finish(
            world,
            player,
            (
                site.bounds.min.x - 6..site.bounds.max.x + 6,
                site.bounds.min.y - 4..site.bounds.max.y + 4,
            ),
        );
        world.stream_terrain_near_players();
    });
    // The floor valve opens; then the rig climbs to the right ledge's valve and holds it.
    stage.settle(220);
    stage.play_on(|world| {
        if let Some(site) = world.cathedral_site() {
            let valve = site.valves[1];
            let player = world.player_ids().next();
            if let Some(player) = player {
                world.teleport_rig_to(
                    player,
                    fatherload_simulation::position::WorldPos::tile_center(
                        i64::from(valve.x - 1),
                        i64::from(valve.y),
                    ),
                );
            }
        }
    });
    stage.settle(90);
}

fn hollow_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let y = middle(Stratum::RadioactiveHollow);
        chamber(world, player, (SCENE_X, y), (10, 4));
        for dx in -12..=12_i32 {
            for dy in [-5_i32, 5] {
                if (dx + dy).rem_euclid(3) != 0 {
                    set(world, SCENE_X + dx, y + dy, deep(DeepTile::GlowRock));
                }
            }
        }
        for dx in [-9, -2, 6] {
            set(world, SCENE_X + dx, y + 5, deep(DeepTile::IsotopeCluster));
        }
        set(world, SCENE_X + 5, y + 4, deep(DeepTile::MutationRelic));
        set(world, SCENE_X + 5, y + 5, deep(DeepTile::GlowRock));
        spawn(world, EnemyKind::GlowGrub, SCENE_X + 3, y + 4);
        spawn(world, EnemyKind::GlowGrub, SCENE_X - 6, y + 4);
        world.set_dose_for_tests(player, 620);
        world.stream_terrain_near_players();
    });
    stage.settle(330);
}

fn glowmother_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let (centre, (half_w, half_h)) =
            sites::glowmother_den(&settings(), &descent::layout(&settings())).expect("a den");
        // Off to one side of the den floor, so she rises clear of the rig and its prompt.
        place_rig(world, player, centre.x - 9, centre.y + half_h - 3);
        finish(
            world,
            player,
            (
                centre.x - half_w - 20..centre.x + half_w + 20,
                centre.y - 20..centre.y + 20,
            ),
        );
        world.set_dose_for_tests(player, 300);
        world.stream_terrain_near_players();
    });
    stage.settle(420);
}

fn machine_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let hall = find_feature(Stratum::AncientMachineLayer, sites::machine_hall).expect("a hall");
        let x = hall.bounds.min.x + 8;
        place_rig(world, player, x, hall.floor() - 1);
        finish(
            world,
            player,
            (
                hall.bounds.min.x - 12..hall.bounds.max.x + 12,
                hall.bounds.min.y - 10..hall.bounds.max.y + 10,
            ),
        );
        spawn(
            world,
            EnemyKind::PistonWarden,
            hall.bounds.max.x - 6,
            hall.floor() - 1,
        );
        world.stream_terrain_near_players();
    });
    stage.settle(330);
}

fn geode_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let geode = find_feature(Stratum::VoidGeodeFields, sites::geode).expect("a geode");
        let c = geode.centre;
        place_rig(world, player, c.x - 2, c.y - 1);
        finish(
            world,
            player,
            (
                c.x - geode.radius - 12..c.x + geode.radius + 12,
                c.y - geode.radius - 8..c.y + geode.radius + 8,
            ),
        );
        spawn(world, EnemyKind::VoidJelly, c.x + 3, c.y - 3);
        world.stream_terrain_near_players();
    });
    stage.settle(330);
}

fn storm_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let y = middle(Stratum::MantleStormZone);
        chamber(world, player, (SCENE_X, y), (11, 4));
        for dx in -12..=12 {
            set(world, SCENE_X + dx, y + 5, deep(DeepTile::CinderRock));
        }
        set(world, SCENE_X + 6, y + 5, deep(DeepTile::StormVent));
        set(world, SCENE_X - 7, y + 5, deep(DeepTile::StormVent));
        spawn(world, EnemyKind::MagmaSalamander, SCENE_X - 3, y + 4);
        world.stream_terrain_near_players();
        let hz = world.tick_rate_hz();
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
            (old_deep::STORM_BREAKS_AT + 2_000) * u64::from(hz) / 1_000,
        ));
    });
    stage.settle(330);
}

fn arena_scene(stage: &mut SceneStage<'_>, exposed: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let arena =
            sites::star_core_arena(&settings(), &descent::layout(&settings())).expect("an arena");
        // On top of the core itself: the pylons' beams converge under the rig.
        place_rig(world, player, arena.core.x, arena.core.y - 1);
        finish(
            world,
            player,
            (
                arena.bounds.min.x - 8..arena.bounds.max.x + 8,
                arena.bounds.min.y - 6..arena.bounds.max.y + 6,
            ),
        );
        world.stream_terrain_near_players();
    });
    stage.settle(330);
    if exposed {
        stage.play_on(|world| {
            if let Some(arena) = world.star_core_arena() {
                for pylon in arena.pylons {
                    let _ = world.terrain_mut().set_kind(pylon, TileKind::Air);
                }
            }
        });
        stage.settle(40);
    } else {
        stage.settle(100);
    }
}

fn log_scene(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let cache = sites::expedition_camp(
            &settings(),
            &descent::layout(&settings()),
            Stratum::PressureCathedrals,
        )
        .expect("a camp");
        // The crew has read the trail up to this camp's log, so its canister opens (the
        // camps hold the expedition's own logs, Wave 8 integration).
        let log = fatherload_simulation::old_deep::expedition_log_of_camp(sites::camp_log(
            Stratum::PressureCathedrals,
        ))
        .unwrap_or(1);
        world.set_expedition(fatherload_simulation::expedition::ExpeditionState {
            found: log - 1,
            ..world.expedition().clone()
        });
        place_rig(world, player, cache.x, cache.y - 1);
        finish(
            world,
            player,
            (cache.x - 30..cache.x + 30, cache.y - 16..cache.y + 12),
        );
        world.stream_terrain_near_players();
    });
    stage.hold(
        150,
        PlayerInput {
            drill_down: true,
            ..PlayerInput::default()
        },
    );
    stage.settle(30);
}

pub const OLD_DEEP_SCENES: &[Scene] = &[
    Scene {
        name: "old_deep_crystal_faults",
        summary: "Wave 8 Crystal Faults: the headlamp reaches a prism and splits into three beams \
                  that light latent gems behind a resonant fault; a prism moth",
        setup: crystal_scene,
    },
    Scene {
        name: "old_deep_fossil_oceans",
        summary: "Wave 8 Fossil Oceans: a serpent skeleton half dug out of its shale bed, an amber \
                  blob, a gas basin at high tide (the tide panel), a fossil eel",
        setup: fossil_scene,
    },
    Scene {
        name: "old_deep_pressure_cathedrals",
        summary: "Wave 8 Pressure Cathedrals: the great nave, pillars and ledges, pressure jets \
                  firing on their cycle, a jet ray",
        setup: cathedral_nave_scene,
    },
    Scene {
        name: "old_deep_cathedral_lift",
        summary: "Wave 8 the Cathedral Lift set piece: the floor valve open, the rig holding the \
                  ledge valve (its ring filling) while the jets are calmed",
        setup: cathedral_lift_scene,
    },
    Scene {
        name: "old_deep_radioactive_hollow",
        summary: "Wave 8 Radioactive Hollow: glow rock and isotopes, a mutation relic, glow grubs, \
                  the dose meter in its sick step and the screen's green grade",
        setup: hollow_scene,
    },
    Scene {
        name: "old_deep_glowmother",
        summary: "Wave 8 Descent II's mini-boss: the Glowmother wakes in her den",
        setup: glowmother_scene,
    },
    Scene {
        name: "old_deep_machine_layer",
        summary: "Wave 8 Ancient Machine Layer: an awake builder hall, belts rolling and pistons \
                  slamming, a builder tablet in the wall, a piston warden",
        setup: machine_scene,
    },
    Scene {
        name: "old_deep_void_geodes",
        summary: "Wave 8 Void Geode Fields: inside a geode the weak pull points out to the shell; \
                  the rig floats, a void jelly drifts",
        setup: geode_scene,
    },
    Scene {
        name: "old_deep_mantle_storm",
        summary: "Wave 8 Mantle Storm Zone: a heat storm raging in an open cave, storm vents \
                  pouring molten metal, embers in the air, a magma salamander",
        setup: storm_scene,
    },
    Scene {
        name: "old_deep_star_core",
        summary: "Wave 8 the Star Core (Descent II gate) awake: its shield fed by four pylons, \
                  flares and sparks, the boss panel",
        setup: |stage| arena_scene(stage, false),
    },
    Scene {
        name: "old_deep_star_core_exposed",
        summary: "Wave 8 the Star Core with its pylons broken: the core lies open to the drill",
        setup: |stage| arena_scene(stage, true),
    },
    Scene {
        name: "old_deep_fathers_log",
        summary: "Wave 8 a Fathers' expedition cache drilled open: its expedition log card",
        setup: log_scene,
    },
];

#[allow(dead_code, reason = "kept for scenes that want a plain tile size")]
const fn tile_px() -> f32 {
    TILE_SIZE
}
