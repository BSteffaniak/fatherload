//! The exotic worlds' scenes (Wave 13 planets-d): the gas giant, the tidally locked world and
//! the ring world of the flight scenes' galaxy, each seen from space, on its round ground (a
//! sky city, the day side and the night side, the Skyring), at its landmark with its boss
//! awake, down in its strata (a bolt flickering, the Diamond Rain, the Geyser Fields, the
//! conduits); its creatures lined up on its flat strip.

#![allow(
    clippy::missing_const_for_fn,
    clippy::cast_possible_truncation,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    enemy::EnemyDifficulty,
    exotic::{self, ExoticStyle, boss, gas_giant, locked, ring, rules},
    flight::{FlightBody, FlightPlace, SpaceQuery, StarDrive, nav},
    place::{BodyId, Place, StoreKey},
    planet::{BodyGround, PlanetGround, PlanetParams},
    space::Theme,
    terrain::{TILE_SIZE, Tile, TileChange, TileKind, TilePosition},
};

use super::{
    Scene, SceneStage,
    flight_scenes::{launch, space_world},
};

/// The flight scenes' galaxy's body of `style` (the nearest home with that tag).
fn exotic_body(world: &WorldState, style: ExoticStyle) -> Option<FlightBody> {
    let tag = match style {
        ExoticStyle::GasGiant => Theme::GasGiant,
        ExoticStyle::Locked => Theme::TidallyLocked,
        ExoticStyle::Ring => Theme::RingWorld,
    };
    let space = world.space();
    let mut rings: Vec<(i64, i64)> = (-5..=5_i64)
        .flat_map(|x| (-5..=5_i64).map(move |y| (x, y)))
        .collect();
    rings.sort_by_key(|(x, y)| x.abs().max(y.abs()));
    rings.into_iter().find_map(|cell| {
        let system = space.system_at(cell)?;
        space.bodies_of(&system).into_iter().find(|body| {
            space
                .space_body(body.body)
                .and_then(|id| space.space().body(id))
                .is_some_and(|found| found.theme == tag)
                && world.body_ground(body.body).is_some()
        })
    })
}

fn planet_of(world: &WorldState, body: FlightBody) -> Option<PlanetGround> {
    world
        .body_ground(body.body)
        .map(|ground| PlanetGround::new(ground.params))
}

/// `player`'s rig on `body`'s round ground at strip tile `(column, row)` (its feet), level,
/// still, landed.
fn put_on(world: &mut WorldState, player: PlayerId, body: FlightBody, column: i32, row: i32) {
    let Some(ground) = world.body_ground(body.body) else {
        return;
    };
    let at = fatherload_simulation::world::strip_position(f64::from(column) + 0.5, f64::from(row));
    let _ = world.move_to_place(player, Place::OnBody(BodyId::Space(ground.id)), at);
    if let Some(rig) = world.player_mut(player) {
        rig.flight.place = FlightPlace::Landed(body.body);
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.attitude = RigAttitude::LEVEL;
    }
}

/// Sets tiles of `body`'s store (scenes dig what a player would have).
fn carve(world: &mut WorldState, body: FlightBody, tiles: &[(i32, i32)], kind: TileKind) {
    let Some(ground) = world.body_ground(body.body) else {
        return;
    };
    for &(x, y) in tiles {
        let _ = world.apply_store_tile_change(
            StoreKey::Body(ground.id),
            &TileChange {
                position: TilePosition { x, y },
                tile: Tile {
                    kind,
                    durability: fatherload_simulation::terrain::tile_durability(kind),
                },
            },
        );
    }
}

fn rect(x: std::ops::Range<i32>, y: std::ops::Range<i32>) -> Vec<(i32, i32)> {
    y.flat_map(|row| x.clone().map(move |column| (column, row)))
        .collect()
}

fn set_millis(world: &mut WorldState, millis: u64) {
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
        millis * hz / 1_000,
    ));
}

/// The galaxy's exotic world of `style`, ready for a scene: space open, the rig fitted.
fn stage_world(world: &mut WorldState, player: PlayerId, style: ExoticStyle) -> Option<FlightBody> {
    space_world(world);
    world.enable_test_mode();
    let body = exotic_body(world, style)?;
    launch(
        world,
        player,
        body.centre,
        0.0,
        (0.0, 0.0),
        StarDrive::Off,
        true,
    );
    Some(body)
}

/// Seen from space: coming down toward the body, its whole disc (and a ring world's ring)
/// below.
fn from_space(stage: &mut SceneStage<'_>, style: ExoticStyle) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, style) else {
            return;
        };
        let altitude = body.radius * if style == ExoticStyle::Ring { 2.6 } else { 1.6 };
        let pos = body.centre.offset(altitude * 0.6, -altitude * 0.8);
        launch(
            world,
            player,
            pos,
            nav::bearing(body.centre, pos) + 0.3,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(player) {
            rig.flight.place = FlightPlace::Approach(body.body);
        }
    });
    stage.settle(10);
}

/// On the round ground at `column(planet)`, `rows_up` over its surface row from
/// `row(planet, column)`; `millis` sets world time first; `boss` wakes the world's hostiles.
fn at_place(
    stage: &mut SceneStage<'_>,
    style: ExoticStyle,
    place: fn(&PlanetGround) -> (i32, i32),
    millis: Option<fn(&PlanetGround) -> u64>,
    hostile: bool,
    settle: usize,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, style) else {
            return;
        };
        let Some(planet) = planet_of(world, body) else {
            return;
        };
        if let Some(millis) = millis {
            set_millis(world, millis(&planet));
        }
        if hostile {
            world.set_enemy_difficulty(EnemyDifficulty::Normal);
        }
        let (x, y) = place(&planet);
        put_on(world, player, body, x, y);
    });
    stage.settle(settle);
}

/// In a pocket dug at `depth` rows below the surface at `column`.
fn underground(
    stage: &mut SceneStage<'_>,
    style: ExoticStyle,
    column: fn(&PlanetGround) -> i32,
    depth: fn(&PlanetGround) -> i32,
    millis: Option<fn(&PlanetGround) -> u64>,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, style) else {
            return;
        };
        let Some(planet) = planet_of(world, body) else {
            return;
        };
        if let Some(millis) = millis {
            set_millis(world, millis(&planet));
        }
        let x = column(&planet);
        let floor = planet.surface_row(x) + depth(&planet);
        carve(
            world,
            body,
            &rect(x - 2..x + 3, floor - 3..floor),
            TileKind::Air,
        );
        carve(
            world,
            body,
            &rect(x - 2..x + 3, floor..floor + 1),
            TileKind::Exotic(exotic::ExoticTile::MetallicHydrogen),
        );
        put_on(world, player, body, x, floor - 1);
    });
    stage.settle(60);
}

fn stratum(planet: &PlanetGround, index: usize) -> i32 {
    planet.core_chamber_top() * [110, 360, 620, 870][index.min(3)] / 1_000
}

// --- Places ------------------------------------------------------------------------------

/// On the settlement's deck or ground, near its edge (the land beyond in view).
fn settlement(planet: &PlanetGround) -> (i32, i32) {
    let site = exotic::settlement_site(planet).expect("a site");
    let x = site.first + site.width - 6;
    (x, planet.surface_row(x) - 1)
}

/// Over the gas giant's cloud tops beside a side city.
fn cloud_tops(planet: &PlanetGround) -> (i32, i32) {
    let city = gas_giant::cities(planet)[1];
    let x = i32::try_from(city.centre).unwrap_or(0) + city.half + 22;
    (x, gas_giant::DECK_ROW + 4)
}

/// In the boss's arena, at its edge.
fn arena(planet: &PlanetGround) -> (i32, i32) {
    let arena = boss::arena(planet).expect("an arena");
    if exotic::style_of_ground(planet) == Some(ExoticStyle::GasGiant) {
        return (arena.centre - 10, gas_giant::EYE_FLOOR - 1);
    }
    let x = arena.home.x - 8;
    (x, planet.surface_row(x) - 1)
}

/// On the locked world's day side.
fn day_side(planet: &PlanetGround) -> (i32, i32) {
    let x = 160;
    (x, planet.surface_row(x) - 1)
}

/// On the locked world's night side.
fn night_side(planet: &PlanetGround) -> (i32, i32) {
    let x = planet.strip_width() / 2 - 140;
    (x, planet.surface_row(x) - 1)
}

/// Over a geyser vent of the Geyser Fields (the pocket is its chimney).
fn geyser(planet: &PlanetGround) -> (i32, i32) {
    let vent = (0..400)
        .flat_map(|cx| (0..40).map(move |cy| (cx, cy)))
        .find_map(|(cx, cy)| {
            locked::vent_in(planet, cx, cy)
                .filter(|vent| planet.stratum_index(vent.x, vent.y) == 1 && vent.x > 40)
        })
        .unwrap_or(TilePosition { x: 100, y: 300 });
    (vent.x + 4, vent.y)
}

/// World time while that vent blows.
fn geyser_blowing(planet: &PlanetGround) -> u64 {
    let (x, y) = geyser(planet);
    let vent = TilePosition { x: x - 4, y };
    (0..30_000_u64)
        .step_by(50)
        .find(|millis| rules::geyser_phase(planet, vent, *millis) == rules::GeyserPhase::Blowing)
        .map_or(0, |millis| millis + 800)
}

/// On the ring world's plains, out from the settlement.
fn plains(planet: &PlanetGround) -> (i32, i32) {
    let site = exotic::settlement_site(planet).expect("a site");
    let x = site.first + site.width + 60;
    (x, planet.surface_row(x) - 1)
}

/// On a Skyring island.
fn skyring(planet: &PlanetGround) -> (i32, i32) {
    (0..2_000)
        .flat_map(|cx| (-8..-2).map(move |cy| (cx, cy)))
        .find_map(|(cx, cy)| ring::island_in(planet, cx, cy))
        .map_or((100, ring::SKYRING_BOTTOM), |(x, y, _, hh, _)| {
            (
                i32::try_from(x).unwrap_or(0),
                y - i32::try_from(hh).unwrap_or(2) - 2,
            )
        })
}

/// A world time when a meteor lands near the plains spot (its shadow half grown).
fn meteor_falling(planet: &PlanetGround) -> u64 {
    let (x, y) = plains(planet);
    rules::fallers_near(
        planet,
        ExoticStyle::Ring,
        TilePosition { x, y },
        1,
        5_000,
        400_000,
        0.0,
    )
    .into_iter()
    .find(|faller| (faller.column - x).abs() < 16)
    .map_or(0, |faller| {
        faller.land_millis - rules::FALL_TELL_MILLIS / 2 - 1_500
    })
}

/// A column in the Storm Belt and a world time while a bolt flickers beside it.
fn storm_belt(planet: &PlanetGround) -> i32 {
    let probe = TilePosition {
        x: 300,
        y: planet.surface_row(300) + stratum(planet, 1),
    };
    rules::bolts_near(planet, probe, 2, 20_000, 200_000, 0.0)
        .first()
        .map_or(300, |bolt| bolt.column + 5)
}

fn bolt_telling(planet: &PlanetGround) -> u64 {
    let probe = TilePosition {
        x: 300,
        y: planet.surface_row(300) + stratum(planet, 1),
    };
    rules::bolts_near(planet, probe, 2, 20_000, 200_000, 0.0)
        .first()
        .map_or(0, |bolt| bolt.tell_millis + 600 - 1_000)
}

/// The world's creatures in a row on its flat strip (drawn flat by the ordinary renderer).
fn creatures(stage: &mut SceneStage<'_>, style: ExoticStyle) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let params = PlanetParams::new(0x00D1_3E5E, 1_200, style.theme());
        let mut planet = WorldState::new_planet_world(
            player,
            params,
            fatherload_simulation::SimulationParams::new(world.timing().rate.hz()),
        );
        planet.enable_test_mode();
        planet.set_enemy_difficulty(EnemyDifficulty::Normal);
        *world = planet;
        let ground = *world.planet().expect("planet");
        let site = exotic::settlement_site(&ground).expect("site");
        let x0 = site.first + 6;
        let floor = site.floor;
        let (x, y) = world.frame().centre_of(x0 - 6, floor - 1);
        if let Some(rig) = world.player_mut(player) {
            rig.x = x;
            rig.y = y;
        }
        let mut column = x0;
        for kind in style.creatures().into_iter().chain([style.boss()]) {
            let wide = i32::try_from((kind.contact_half().0 / TILE_SIZE) as i64).unwrap_or(1);
            column += wide + 2;
            let flying = matches!(
                kind.stats().locomotion,
                fatherload_simulation::enemy::Locomotion::Flying
            );
            let lift = if flying {
                3 + i32::try_from((kind.contact_half().1 / TILE_SIZE) as i64).unwrap_or(0)
            } else {
                1
            };
            let (x, y) = world.frame().centre_of(column, floor - lift);
            let _ = world.spawn_enemy(kind, x, TILE_SIZE.mul_add(0.5, y) - kind.stats().radius);
            column += wide + 1;
        }
        // Keep the line still for the picture (the bosses wake only in their arenas).
        super::reveal(world, player, x0 - 30..column + 20, floor - 24..floor + 10);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
}

pub const EXOTIC_SCENES: &[Scene] = &[
    // The gas giant.
    Scene {
        name: "exotic_gas_giant_from_space",
        summary: "The gas giant from space: a vast banded cream and ochre disc with a warm glow",
        setup: |stage| from_space(stage, ExoticStyle::GasGiant),
    },
    Scene {
        name: "exotic_gas_giant_sky_city",
        summary: "Docked at a sky city: its plated deck, lift bladders glowing under the keel, towers and lanterns, the cloud tops far below",
        setup: |stage| at_place(stage, ExoticStyle::GasGiant, settlement, None, false, 60),
    },
    Scene {
        name: "exotic_gas_giant_cloud_tops",
        summary: "Over the gas giant's billowing cloud tops beside a side city's keel",
        setup: |stage| at_place(stage, ExoticStyle::GasGiant, cloud_tops, None, false, 30),
    },
    Scene {
        name: "exotic_gas_giant_eye",
        summary: "The Eye: a storm wall round a calm well, the Thunderhead awake over its cloud platform, its told bolts glowing",
        setup: |stage| at_place(stage, ExoticStyle::GasGiant, arena, None, true, 150),
    },
    Scene {
        name: "exotic_gas_giant_storm_belt",
        summary: "The Storm Belt: ochre storm cloud and charged gas pockets, a column flickering before its bolt",
        setup: |stage| {
            underground(
                stage,
                ExoticStyle::GasGiant,
                storm_belt,
                |p| stratum(p, 1),
                Some(bolt_telling),
            );
        },
    },
    Scene {
        name: "exotic_gas_giant_diamond_rain",
        summary: "The Diamond Rain: thin soot cloud, lumps of metallic hydrogen and diamond hail, the pressure gauge in the red",
        setup: |stage| {
            underground(
                stage,
                ExoticStyle::GasGiant,
                |_| 400,
                |p| stratum(p, 3),
                None,
            );
        },
    },
    Scene {
        name: "exotic_gas_giant_creatures",
        summary: "The gas giant's creatures: a drift ray, a gasbag, a storm eel, a vapour stinger and the Thunderhead",
        setup: |stage| creatures(stage, ExoticStyle::GasGiant),
    },
    // The locked world.
    Scene {
        name: "exotic_locked_from_space",
        summary: "The tidally locked world from space: one face lit, one dark, the terminator between",
        setup: |stage| from_space(stage, ExoticStyle::Locked),
    },
    Scene {
        name: "exotic_locked_day_side",
        summary: "The scorched day side: cracked hardpan, salt pans and mesas under the endless noon, the heat warning",
        setup: |stage| at_place(stage, ExoticStyle::Locked, day_side, None, false, 60),
    },
    Scene {
        name: "exotic_locked_twilight",
        summary: "The twilight ring at the settlement: lichen hills and glowmoss, meltwater pools, the sun low on the horizon",
        setup: |stage| at_place(stage, ExoticStyle::Locked, settlement, None, false, 60),
    },
    Scene {
        name: "exotic_locked_night_side",
        summary: "The frozen night side: nitrogen ice, pressure ridges and ice spikes under the stars",
        setup: |stage| at_place(stage, ExoticStyle::Locked, night_side, None, false, 60),
    },
    Scene {
        name: "exotic_locked_gloam_gate",
        summary: "The Gloam Gate: an arch half scorched, half frozen, the Gloamwarden awake on its arena",
        setup: |stage| at_place(stage, ExoticStyle::Locked, arena, None, true, 150),
    },
    Scene {
        name: "exotic_locked_geyser",
        summary: "The Geyser Fields: a vent's plume blowing up its chimney through vent rock",
        setup: |stage| {
            at_place(
                stage,
                ExoticStyle::Locked,
                geyser,
                Some(geyser_blowing),
                false,
                20,
            );
        },
    },
    Scene {
        name: "exotic_locked_fractures",
        summary: "The Stress Fractures: fractured rock, cracks roofed with fault rubble",
        setup: |stage| underground(stage, ExoticStyle::Locked, |_| 200, |p| stratum(p, 2), None),
    },
    Scene {
        name: "exotic_locked_creatures",
        summary: "The locked world's creatures: a dusk grazer, a sunbasker, a scorch mite, a frost stalker and the Gloamwarden",
        setup: |stage| creatures(stage, ExoticStyle::Locked),
    },
    // The ring world.
    Scene {
        name: "exotic_ring_from_space",
        summary: "The ring world from space: a blue-violet disc inside its great ring",
        setup: |stage| from_space(stage, ExoticStyle::Ring),
    },
    Scene {
        name: "exotic_ring_plains",
        summary: "The ringfall plains: ring dust, craters with glassy rims, a fallen monolith, a meteor's shadow growing",
        setup: |stage| {
            at_place(
                stage,
                ExoticStyle::Ring,
                plains,
                Some(meteor_falling),
                false,
                45,
            );
        },
    },
    Scene {
        name: "exotic_ring_skyring",
        summary: "On the Skyring: floating islands of ring ice and ringstone in the low sky, the plains far below",
        setup: |stage| at_place(stage, ExoticStyle::Ring, skyring, None, false, 30),
    },
    Scene {
        name: "exotic_ring_tether",
        summary: "The Tether: the builders' column rising from its plaza, the Ring Warden awake, its beam row glowing",
        setup: |stage| at_place(stage, ExoticStyle::Ring, arena, None, true, 150),
    },
    Scene {
        name: "exotic_ring_conduits",
        summary: "The Tether Roots: ringstone threaded with live builder conduits and their junctions",
        setup: |stage| underground(stage, ExoticStyle::Ring, |_| 300, |p| stratum(p, 2), None),
    },
    Scene {
        name: "exotic_ring_creatures",
        summary: "The ring world's creatures: a ringhopper, a shardback, a tether drone, a void mite and the Ring Warden",
        setup: |stage| creatures(stage, ExoticStyle::Ring),
    },
];
