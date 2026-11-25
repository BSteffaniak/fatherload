//! The frontier worlds' scenes (Wave 12 planets-b): the dune, volcanic and crystal worlds of
//! the flight scenes' galaxy, each seen from space, on its round ground, at its landmark, in a
//! storm (and an eruption), down in its strata; its creatures lined up on its flat strip.

#![allow(
    clippy::missing_const_for_fn,
    reason = "scene staging: plain arithmetic on positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    flight::{FlightBody, FlightPlace, SpaceQuery, StarDrive, nav},
    frontier::{self, FrontierStyle, generate, rules},
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
fn frontier_body(world: &WorldState, style: FrontierStyle) -> Option<FlightBody> {
    let tag = match style {
        FrontierStyle::Desert => Theme::DesertDunes,
        FrontierStyle::Volcanic => Theme::Volcanic,
        FrontierStyle::Crystal => Theme::Crystal,
    };
    let space = world.space();
    let mut rings: Vec<(i64, i64)> = (-4..=4_i64)
        .flat_map(|x| (-4..=4_i64).map(move |y| (x, y)))
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

/// The ground of `body`.
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

/// The galaxy's frontier world of `style`, ready for a scene: space open, the rig fitted.
fn stage_world(
    world: &mut WorldState,
    player: PlayerId,
    style: FrontierStyle,
) -> Option<FlightBody> {
    space_world(world);
    world.enable_test_mode();
    let body = frontier_body(world, style)?;
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

/// Seen from space: coming down toward the body, nose up, its whole disc below.
fn from_space(stage: &mut SceneStage<'_>, style: FrontierStyle) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, style) else {
            return;
        };
        let altitude = body.radius * 1.6;
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

/// On the round ground at `column(planet)` on the surface; `millis` sets world time first.
fn on_surface(
    stage: &mut SceneStage<'_>,
    style: FrontierStyle,
    column: fn(&PlanetGround) -> i32,
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
        put_on(world, player, body, x, planet.surface_row(x) - 1);
    });
    stage.settle(90);
}

/// In a pocket dug at `depth` rows below the surface at `column`.
fn underground(
    stage: &mut SceneStage<'_>,
    style: FrontierStyle,
    column: fn(&PlanetGround) -> i32,
    depth: fn(&PlanetGround) -> i32,
) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, style) else {
            return;
        };
        let Some(planet) = planet_of(world, body) else {
            return;
        };
        let x = column(&planet);
        let floor = planet.surface_row(x) + depth(&planet);
        carve(
            world,
            body,
            &rect(x - 2..x + 3, floor - 3..floor),
            TileKind::Air,
        );
        put_on(world, player, body, x, floor - 1);
    });
    stage.settle(60);
}

/// On the settlement site's flat ground, near its far edge (the land beyond in view).
fn site(planet: &PlanetGround) -> i32 {
    frontier::settlement_site(planet).map_or(100, |site| site.column + site.width - 4)
}

/// Beside the landmark, its face in view (clear of the Sentinel's head and the spire's plinth).
fn landmark(planet: &PlanetGround) -> i32 {
    frontier::landmark(planet).map_or(100, |mark| {
        mark.column
            - if mark.style == FrontierStyle::Desert {
                17
            } else {
                7
            }
    })
}

/// Beside a geode of the Growing Geodes.
fn geode(planet: &PlanetGround) -> i32 {
    let start = site(planet);
    let shell = TileKind::Frontier(fatherload_simulation::frontier::FrontierTile::GeodeShell);
    (0..800)
        .map(|dx| start + dx * 3)
        .find(|x| {
            let y = planet.surface_row(*x) + stratum(planet, 2);
            planet.generated_tile(*x, y).kind == shell
        })
        .map_or(start, |x| x - 5)
}

/// Out among the crystal spires, past the settlement's cleared meadow.
fn spires(planet: &PlanetGround) -> i32 {
    site(planet) + 44
}

fn stratum(planet: &PlanetGround, index: i32) -> i32 {
    frontier::strata_rows(planet.grid().radius()) * [120, 380, 660, 880][usize::try_from(index).unwrap_or(0).min(3)] / 1_000
}

fn storm(planet: &PlanetGround) -> u64 {
    let style = frontier::style_of_ground(planet).unwrap_or(FrontierStyle::Desert);
    (0..2_000_u64)
        .map(|second| second * 1_000)
        .find(|millis| {
            let weather = rules::weather_at(style, planet.params().seed, *millis);
            weather.phase == rules::WeatherPhase::Storm && weather.strength_permille == 1_000
        })
        .unwrap_or(0)
}

/// A lava bomb of the Caldera that falls on dry slope (the eruption scene watches it come).
fn eruption_bomb(planet: &PlanetGround) -> Option<rules::LavaBomb> {
    let cone = generate::volcano_cones(planet)
        .into_iter()
        .find(|cone| cone.caldera)?;
    (1..1_000_u64)
        .flat_map(|second| {
            rules::bombs_landing(planet, cone.index, second * 1_000, second * 1_000 + 1_000)
        })
        .find(|bomb| {
            let dx = bomb.column - cone.centre;
            dx > cone.crater + 4 && dx < cone.radius - 8
        })
}

/// A world time a second and a half before a bomb's shadow is half grown (the scene settles
/// that long).
fn eruption(planet: &PlanetGround) -> u64 {
    eruption_bomb(planet).map_or(0, |bomb| bomb.land.saturating_sub(700 + 1_500))
}

/// A few tiles downhill of where that bomb will land.
fn bomb_side(planet: &PlanetGround) -> i32 {
    eruption_bomb(planet).map_or_else(
        || caldera_side(planet),
        |bomb| i32::try_from(bomb.column + 7).unwrap_or(0),
    )
}

/// Beside a cone, close enough to watch its bombs come down.
fn caldera_side(planet: &PlanetGround) -> i32 {
    generate::volcano_cones(planet)
        .into_iter()
        .find(|cone| cone.caldera)
        .map_or(100, |cone| {
            i32::try_from(cone.centre + cone.crater + 5).unwrap_or(100)
        })
}

/// A light well with the light dug its path: the beam turning at its prism, the vault open.
fn light_well(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(body) = stage_world(world, player, FrontierStyle::Crystal) else {
            return;
        };
        let Some(planet) = planet_of(world, body) else {
            return;
        };
        let Some(well) = generate::light_wells(&planet).first().copied() else {
            return;
        };
        let way = if well.east { 1 } else { -1 };
        let mut path: Vec<(i32, i32)> = (well.lens_row + 1..well.prism.y)
            .map(|y| (well.column, y))
            .collect();
        let mut x = well.prism.x + way;
        while x != well.receptor.x {
            path.push((x, well.prism.y));
            x += way;
        }
        // A room to stand in beside the beam's run.
        path.extend(rect(
            well.column - way * 4..well.column - way * 4 + 3,
            well.prism.y - 3..well.prism.y + 1,
        ));
        carve(world, body, &path, TileKind::Air);
        put_on(world, player, body, well.column - way * 3, well.prism.y);
    });
    stage.settle(90);
}

/// The world's creatures in a row on its flat strip (drawn flat by the ordinary renderer).
fn creatures(stage: &mut SceneStage<'_>, style: FrontierStyle) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let params = PlanetParams::new(0x00F0_0D5E, 1_000, style.theme());
        let mut planet = WorldState::new_planet_world(
            player,
            params,
            fatherload_simulation::SimulationParams::new(world.timing().rate.hz()),
        );
        planet.enable_test_mode();
        planet.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
        *world = planet;
        let ground = *world.planet().expect("planet");
        let site = frontier::settlement_site(&ground).expect("site");
        let x0 = site.column + 12;
        let floor = site.surface_row;
        if let Some(rig) = world.player_mut(player) {
            let (x, y) = world_point(x0 - 5, floor - 1);
            rig.x = x;
            rig.y = y;
        }
        for (index, kind) in frontier::creatures(style).into_iter().enumerate() {
            let column = x0 + i32::try_from(index).unwrap_or(0) * 4;
            let flying = matches!(
                kind.stats().locomotion,
                fatherload_simulation::enemy::Locomotion::Flying
            );
            let (x, y) = world_point(column, floor - if flying { 4 } else { 1 });
            let _ = world.spawn_enemy(kind, x, TILE_SIZE.mul_add(0.5, y) - kind.stats().radius);
        }
        super::reveal(world, player, x0 - 30..x0 + 40, floor - 20..floor + 10);
        world.stream_terrain_near_players();
    });
    stage.settle(2);
}

/// The local pixel centre of tile `(x, y)` in a fresh planet world (its frame at the origin).
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn world_point(x: i32, y: i32) -> (f32, f32) {
    ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE)
}

pub const FRONTIER_SCENES: &[Scene] = &[
    // The dune world.
    Scene {
        name: "frontier_dunes_from_space",
        summary: "The dune world from space: a sand-gold disc with its warm glow, coming down nose up",
        setup: |stage| from_space(stage, FrontierStyle::Desert),
    },
    Scene {
        name: "frontier_dunes_surface",
        summary: "On the dune world's round ground: dunes with steep slip faces rolling to the curved horizon",
        setup: |stage| on_surface(stage, FrontierStyle::Desert, site, None),
    },
    Scene {
        name: "frontier_dunes_sentinel",
        summary: "The Sleeping Sentinel: a giant stone head half buried in the sand, its glyph eyes glowing",
        setup: |stage| on_surface(stage, FrontierStyle::Desert, landmark, None),
    },
    Scene {
        name: "frontier_dunes_sandstorm",
        summary: "A sandstorm on the dune world: the tan veil and blowing grit (screen effects at full), the HUD line naming it",
        setup: |stage| on_surface(stage, FrontierStyle::Desert, site, Some(storm)),
    },
    Scene {
        name: "frontier_dunes_buried_city",
        summary: "Down in the Buried City: banded sandstone, ruins with cracked floors over their caches",
        setup: |stage| underground(stage, FrontierStyle::Desert, site, |p| stratum(p, 1)),
    },
    Scene {
        name: "frontier_dunes_glass_deep",
        summary: "The Glass Deep: salt beds veined with green desert glass",
        setup: |stage| underground(stage, FrontierStyle::Desert, site, |p| stratum(p, 3)),
    },
    Scene {
        name: "frontier_dunes_creatures",
        summary: "The dune world's creatures: a dune skipper, a mirage moth, the dune wyrm and a glass scorpion",
        setup: |stage| creatures(stage, FrontierStyle::Desert),
    },
    // The volcanic world.
    Scene {
        name: "frontier_volcanic_from_space",
        summary: "The volcanic world from space: dark rock and lava seas under an orange glow",
        setup: |stage| from_space(stage, FrontierStyle::Volcanic),
    },
    Scene {
        name: "frontier_volcanic_caldera",
        summary: "Beside the Caldera: the great cone and its lava lake, the lava seas below",
        setup: |stage| on_surface(stage, FrontierStyle::Volcanic, caldera_side, None),
    },
    Scene {
        name: "frontier_volcanic_eruption",
        summary: "The Caldera erupting: lava bombs arcing down onto their reddening shadows (the telegraph), ash falling",
        setup: |stage| on_surface(stage, FrontierStyle::Volcanic, bomb_side, Some(eruption)),
    },
    Scene {
        name: "frontier_volcanic_basalt_flows",
        summary: "The Basalt Flows: columnar basalt, red scoria, a glassy lava tube",
        setup: |stage| underground(stage, FrontierStyle::Volcanic, site, |p| stratum(p, 1)),
    },
    Scene {
        name: "frontier_volcanic_magma_chambers",
        summary: "The Magma Chambers: glowing cracked magma rock, lava pooled on the floors, the HEAT warning",
        setup: |stage| underground(stage, FrontierStyle::Volcanic, site, |p| stratum(p, 3)),
    },
    Scene {
        name: "frontier_volcanic_creatures",
        summary: "The volcanic world's creatures: a basalt tortoise, an ember strider, cinder bats and a magma slug",
        setup: |stage| creatures(stage, FrontierStyle::Volcanic),
    },
    // The crystal world.
    Scene {
        name: "frontier_crystal_from_space",
        summary: "The crystal world from space: a violet disc with a pale halo",
        setup: |stage| from_space(stage, FrontierStyle::Crystal),
    },
    Scene {
        name: "frontier_crystal_surface",
        summary: "On the crystal world's round ground: quartz hills bristling with ice-blue spires",
        setup: |stage| on_surface(stage, FrontierStyle::Crystal, spires, None),
    },
    Scene {
        name: "frontier_crystal_choir_spire",
        summary: "The Choir Spire: two tall crystal prongs over a plinth, lenses at their tips",
        setup: |stage| on_surface(stage, FrontierStyle::Crystal, landmark, None),
    },
    Scene {
        name: "frontier_crystal_glitterfrost",
        summary: "A glitterfrost on the crystal world: a pale veil, slow sparkles, the cold warning",
        setup: |stage| on_surface(stage, FrontierStyle::Crystal, site, Some(storm)),
    },
    Scene {
        name: "frontier_crystal_light_well",
        summary: "A light well dug its path: the beam down from the lens, turned at the prism into the lit receptor, the vault open",
        setup: light_well,
    },
    Scene {
        name: "frontier_crystal_geodes",
        summary: "The Growing Geodes: shard stone, geodes lined with amethyst",
        setup: |stage| underground(stage, FrontierStyle::Crystal, geode, |p| stratum(p, 2)),
    },
    Scene {
        name: "frontier_crystal_creatures",
        summary: "The crystal world's creatures: a glint moth, a chime drifter, a shard crab and a geode mimic",
        setup: |stage| creatures(stage, FrontierStyle::Crystal),
    },
];
