//! The builders' buried city's scenes (Wave 10 ancients-a, Descent VII strata 1-3): each
//! stratum's set pieces with their creatures and machinery at work (an avenue with its tram at a
//! station, a ride on it, a tower lift, the Lantern Library and the archivist, the alarm's
//! hunt, the Archive Post's residents, engine halls, a piston ride, crusher lines, piston
//! shafts and the Great Engine, archive stacks, every kind of vault, the Orrery and the star
//! charts page).

use fatherload_simulation::{
    PlayerId, WorldState,
    ancient_city::{
        ALARM_FULL, CityLandmark,
        archivist::VaultKind,
        movers::{CityMover, MILLI, StrokePhase},
    },
    depth_progression::{ProgressionRules, RigSystem},
    enemy::EnemyKind,
    people::deep_folk::{self, DeepSettlement},
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{
            self,
            ancient_city::{self as sites, CityStamp, CityStampKind},
            landmarks,
        },
    },
};

use super::{Scene, SceneStage, place_rig, reveal};
use crate::shell::ModalScreen;

/// Seed of every buried city scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;
/// Column the scenes look around.
const SCENE_X: i32 = 120;
/// Milliseconds a scene frame lasts.
const FRAME_MILLIS: u64 = 1_000 / 60;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// Swaps the staged world's terrain for the scene world, with the Descent rules.
fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.grant_deep_claim();
}

/// Fits the rig for Descent VII (every gate system through the Phase Drill).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.fuel_tank_level = 16;
        rig.fuel_capacity = fatherload_simulation::player::fuel_capacity_for_level(16);
        rig.fuel = rig.fuel_capacity * 0.75;
        rig.engine_level = 16;
        rig.drill_strength = 17;
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.rig_systems &= !RigSystem::HeartKey.bit();
        rig.radiator_level = 19;
        rig.hull_level = 17;
        rig.hull = rig.max_hull();
        rig.credits = 900_000;
    }
}

/// The rig at tile `(x, y)`, the land around it explored, the landmarks' cards seen (each has
/// its own scene).
fn stage_at(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
    deep_world(world);
    fit(world, player);
    place_rig(world, player, x, y);
    reveal(world, player, x - 60..x + 60, y - 36..y + 36);
    world.stream_terrain_near_players();
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    let mut city = world.ancient_city().clone();
    city.found = CityLandmark::ALL.into_iter().collect();
    world.set_ancient_city(city);
}

/// Every stamp of the buried city round the scene column.
fn stamps() -> Vec<CityStamp> {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = sites::carve_rows(&layout);
    sites::stamps_with(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition { x: -900, y: top },
            max: TilePosition {
                x: 1_100,
                y: bottom,
            },
        },
    )
}

/// The stamp of `kind` nearest the scene column.
fn stamp(kind: CityStampKind) -> Option<CityStamp> {
    stamps()
        .into_iter()
        .filter(|found| found.kind == kind)
        .min_by_key(|found| (found.bounds.center().x - SCENE_X).abs())
}

fn movers_of(stamp: &CityStamp) -> Vec<CityMover> {
    let settings = settings();
    let layout = descent::layout(&settings);
    sites::movers_with(&settings, &layout, &stamp.bounds)
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

fn now(world: &WorldState) -> u64 {
    world.millis_at(world.simulation_tick())
}

fn column_of(milli: i64) -> i32 {
    i32::try_from(milli.div_euclid(MILLI)).unwrap_or(0)
}

/// Frames until `millis` from `world`'s present.
fn frames_until(world: &WorldState, millis: u64) -> usize {
    usize::try_from(millis.saturating_sub(now(world)) / FRAME_MILLIS).unwrap_or(0)
}

/// The first avenue with a tram line, and the line.
fn tram_avenue() -> Option<(CityStamp, CityMover)> {
    let mut avenues: Vec<CityStamp> = stamps()
        .into_iter()
        .filter(|found| found.kind == CityStampKind::Avenue && found.tram)
        .collect();
    avenues.sort_by_key(|found| (found.bounds.center().x - SCENE_X).abs());
    let avenue = avenues.into_iter().next()?;
    let tram = movers_of(&avenue)
        .into_iter()
        .find(|mover| matches!(mover, CityMover::Tram { .. }))?;
    Some((avenue, tram))
}

/// Where the tram next waits at a stop from `millis` on: `(world millis it arrives, its
/// column there)`.
fn next_stop(tram: &CityMover, millis: u64) -> Option<(u64, i32)> {
    let state = tram.track_at(millis)?;
    if state.dwelling.is_some() {
        return Some((millis, column_of(state.pos)));
    }
    let arrive = millis + state.until_millis + 20;
    let there = tram.track_at(arrive)?;
    Some((arrive, column_of(there.pos)))
}

/// An avenue with its tram pulling into a station, the rig waiting on the platform, the city's
/// machines about.
fn avenue(stage: &mut SceneStage<'_>, ride: bool) {
    let mut wait = 0;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some((avenue, tram)) = tram_avenue() else {
            return;
        };
        stage_at(world, player, avenue.bounds.center().x, avenue.floor - 1);
        let Some((arrive, column)) = next_stop(&tram, now(world) + 1_200) else {
            return;
        };
        let floor = avenue.floor;
        if ride {
            // Aboard as it pulls in, then on as it leaves.
            place_rig(world, player, column, floor - 1);
            wait = frames_until(world, arrive) + 300;
        } else {
            place_rig(world, player, column - 4, floor - 1);
            wait = frames_until(world, arrive).saturating_sub(20);
            spawn(world, EnemyKind::MaintenanceDrone, column + 6, floor - 5);
            spawn(world, EnemyKind::StreetSentinel, column - 12, floor - 2);
            spawn(world, EnemyKind::ScrapCrawler, column + 10, floor - 1);
        }
        reveal(
            world,
            player,
            column - 60..column + 60,
            floor - 30..floor + 30,
        );
        world.wake_archivist_for(player);
    });
    stage.settle(wait.clamp(10, 520));
}

/// A tower: its floors and glass, the lift cab carrying the rig down between the avenues.
fn tower_lift(stage: &mut SceneStage<'_>) {
    let mut wait = 0;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(tower) = stamp(CityStampKind::Tower) else {
            return;
        };
        stage_at(world, player, tower.shaft, tower.floor - 1);
        let Some(lift) = movers_of(&tower)
            .into_iter()
            .find(|mover| matches!(mover, CityMover::Lift { .. }))
        else {
            return;
        };
        // In the cab at whatever floor it next waits at; then ride a moment.
        if let Some(state) = lift.track_at(now(world) + 300) {
            let arrive = if state.dwelling.is_some() {
                now(world) + 300
            } else {
                now(world) + 300 + state.until_millis + 20
            };
            if let Some(there) = lift.track_at(arrive) {
                let row = column_of(there.pos);
                place_rig(world, player, tower.shaft, row - 1);
                wait = frames_until(world, arrive) + 220;
            }
        }
        let middle = tower.bounds.center();
        reveal(
            world,
            player,
            tower.bounds.min.x - 30..tower.bounds.max.x + 30,
            middle.y - 40..middle.y + 40,
        );
    });
    stage.settle(wait.clamp(10, 520));
}

/// The Lantern Library: its lamps, the archivist's core, and the archivist waking and speaking.
fn lantern_library(stage: &mut SceneStage<'_>, card: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(library) = sites::lantern_library(&settings, &layout) else {
            return;
        };
        let core = sites::library_core(&library);
        stage_at(world, player, core.x - 3, core.y);
        if card {
            // This scene shows the library's card (and the archivist's greeting).
            let mut city = world.ancient_city().clone();
            city.found.clear();
            world.set_ancient_city(city);
        }
        reveal(
            world,
            player,
            library.bounds.min.x - 20..library.bounds.max.x + 20,
            library.bounds.min.y - 10..library.bounds.max.y + 10,
        );
    });
    stage.settle(if card { 40 } else { 200 });
}

/// The archivist at work: hovering by its rig in an avenue, its scan marks round ore, a word
/// on a creature in view.
fn archivist(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some((avenue, _)) = tram_avenue() else {
            return;
        };
        let x = avenue.bounds.min.x + 12;
        stage_at(world, player, x, avenue.floor - 1);
        world.wake_archivist_for(player);
        // Ore under the street for its scan to find.
        for dx in [-2, 3, 5] {
            let _ = world.terrain_mut().set_kind(
                TilePosition {
                    x: x + dx,
                    y: avenue.floor + 2,
                },
                fatherload_simulation::terrain::TileKind::Ore(
                    fatherload_simulation::terrain::MineralKind::Mythril,
                ),
            );
        }
        spawn(world, EnemyKind::ScrapCrawler, x + 7, avenue.floor - 1);
        reveal(
            world,
            player,
            x - 40..x + 40,
            avenue.floor - 20..avenue.floor + 20,
        );
    });
    stage.settle(30);
    stage.play_on(|world| {
        let player = world.player_ids().next().unwrap_or(PlayerId::new(0));
        world.stage_archivist_line(player, 51);
        let Some(rig) = world.player(player).map(|rig| rig.tile_in(world.frame())) else {
            return;
        };
        let tick = world.simulation_tick();
        let mut city = world.ancient_city().clone();
        if let Some(mine) = city.archivists.get_mut(&player) {
            mine.scan_tick = tick;
            mine.scan = [-2, 3, 5]
                .into_iter()
                .map(|dx| TilePosition {
                    x: rig.x + dx,
                    y: rig.y + 3,
                })
                .collect();
        }
        world.set_ancient_city(city);
    });
    stage.settle(20);
}

/// The city hunting a rig that cut through a tower: drones and a sentinel closing in.
fn alarm_hunt(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some((avenue, _)) = tram_avenue() else {
            return;
        };
        let x = avenue.bounds.max.x - 20;
        stage_at(world, player, x, avenue.floor - 1);
        world.stage_city_alarm(player, ALARM_FULL);
        spawn(world, EnemyKind::MaintenanceDrone, x - 8, avenue.floor - 5);
        spawn(world, EnemyKind::MaintenanceDrone, x + 9, avenue.floor - 4);
        spawn(world, EnemyKind::StreetSentinel, x + 12, avenue.floor - 2);
        reveal(
            world,
            player,
            x - 40..x + 40,
            avenue.floor - 20..avenue.floor + 20,
        );
    });
    stage.settle(90);
}

/// The Archive Post: Descent VII's outpost hall and its Lamplighters and machines.
fn archive_post(stage: &mut SceneStage<'_>, talk: bool) {
    let Some((ground, first, last)) = deep_folk::hall(&settings(), DeepSettlement::ArchivePost)
    else {
        return;
    };
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        deep_world(world);
        fit(world, player);
        let middle = i32::midpoint(first, last);
        place_rig(world, player, middle, ground - 1);
        reveal(
            world,
            player,
            first - 40..last + 40,
            ground - 22..ground + 10,
        );
        world.stream_terrain_near_players();
        world.wake_archivist_for(player);
        let mut city = world.ancient_city().clone();
        city.found = CityLandmark::ALL.into_iter().collect();
        world.set_ancient_city(city);
    });
    stage.settle(150);
    stage.seat(0).village.banner = None;
    if talk {
        super::people_scenes::open_talk(stage, |character| {
            character.role == fatherload_simulation::people::Role::Scholar
        });
    }
}

/// An engine hall at work: pistons rising in turn, crushers over the belts, the halls'
/// creatures, the rig riding a piston up.
fn engine_hall(stage: &mut SceneStage<'_>) {
    let mut wait = 0;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(hall) = stamp(CityStampKind::EngineHall) else {
            return;
        };
        let floor = hall.bounds.max.y - 1;
        stage_at(world, player, hall.bounds.center().x, floor - 1);
        // On the first piston, timed so it is rising as the picture is taken.
        if let Some(CityMover::Piston { column, cycle, .. }) = movers_of(&hall).into_iter().find(
            |mover| matches!(mover, CityMover::Piston { floor_row, .. } if *floor_row == floor),
        ) {
            place_rig(world, player, column, floor - 1);
            let period = cycle.period_millis;
            let start = now(world);
            let rising = (0..period / 20)
                .map(|step| start + 200 + step * 20)
                .find(|millis| {
                    CityMover::Piston {
                        anchor: fatherload_simulation::ancient_city::MoverRef {
                            kind: fatherload_simulation::ancient_city::MoverKind::Piston,
                            x: column,
                            y: floor,
                        },
                        column,
                        floor_row: floor,
                        cycle,
                    }
                    .piston_top(*millis)
                    .is_some_and(|(_, _, phase)| phase == StrokePhase::Held)
                })
                .unwrap_or(start);
            wait = frames_until(world, rising);
        }
        let middle = hall.bounds.center();
        spawn(world, EnemyKind::CogRoller, middle.x + 6, floor - 1);
        spawn(world, EnemyKind::SparkWisp, middle.x + 2, middle.y - 4);
        spawn(world, EnemyKind::RivetHound, middle.x + 12, floor - 1);
        reveal(
            world,
            player,
            hall.bounds.min.x - 20..hall.bounds.max.x + 20,
            hall.bounds.min.y - 10..hall.bounds.max.y + 10,
        );
    });
    stage.settle(wait.clamp(10, 300));
}

/// A set piece of `kind` shown whole with the rig at its door (crusher lines, piston shafts,
/// the Great Engine, stacks, vaults, the Orrery).
fn set_piece(stage: &mut SceneStage<'_>, kind: CityStampKind, creatures: &'static [EnemyKind]) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(found) = stamp(kind) else {
            return;
        };
        let b = found.bounds;
        let floor = b.max.y - 1;
        let door = if kind == CityStampKind::GreatEngine {
            // The flywheel is the picture: stand under it.
            b.center().x - 9
        } else if found.mirrored {
            b.max.x - 1
        } else {
            b.min.x + 1
        };
        stage_at(world, player, door, floor - 1);
        for (index, creature) in creatures.iter().enumerate() {
            let offset = i32::try_from(index).unwrap_or(0) * 5 + 6;
            let x = if found.mirrored {
                b.max.x - offset
            } else {
                b.min.x + offset
            };
            spawn(world, *creature, x, floor - 2);
        }
        let middle = b.center();
        reveal(
            world,
            player,
            b.min.x - 20..b.max.x + 20,
            middle.y - 30..middle.y + 30,
        );
    });
    stage.settle(300);
}

/// A puzzle vault of `kind`, the rig outside its lock (the plinth's light inside).
fn vault(stage: &mut SceneStage<'_>, kind: VaultKind) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let Some(found) = stamp(CityStampKind::Vault(kind)) else {
            return;
        };
        let b = found.bounds;
        let local = match kind {
            VaultKind::Plate => (2, 5),
            // Between the second gate and the glass, so both gates, the three colours and the
            // plinth are all in view.
            VaultKind::Grand => (20, 5),
            VaultKind::Light => (1, 5),
            VaultKind::Lever => (12, 6),
        };
        let x = if found.mirrored {
            b.max.x - local.0
        } else {
            b.min.x + local.0
        };
        stage_at(world, player, x, b.min.y + local.1);
        let middle = b.center();
        reveal(
            world,
            player,
            b.min.x - 20..b.max.x + 20,
            middle.y - 24..middle.y + 24,
        );
        if kind == VaultKind::Light {
            world.set_lamp_filter(player, fatherload_simulation::far_strange::LampFilter::Red);
        }
        spawn(world, EnemyKind::PageMoth, middle.x, middle.y - 6);
    });
    stage.settle(300);
}

/// The Orrery: its spheres circling, the plinths' light, and three charts read.
fn orrery(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(found) = sites::orrery(&settings, &layout) else {
            return;
        };
        let plinths = sites::orrery_plinths(&found);
        stage_at(world, player, plinths[1].x - 3, plinths[1].y);
        reveal(
            world,
            player,
            found.bounds.min.x - 20..found.bounds.max.x + 20,
            found.bounds.min.y - 10..found.bounds.max.y + 10,
        );
        spawn(
            world,
            EnemyKind::ArchiveWarden,
            plinths[2].x + 4,
            plinths[2].y,
        );
        spawn(
            world,
            EnemyKind::IndexSpider,
            plinths[1].x + 6,
            plinths[1].y,
        );
    });
    stage.settle(300);
}

/// The star charts page with a handful of charts read and one selected.
fn star_charts(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let Some(found) = sites::orrery(&settings, &layout) else {
            return;
        };
        stage_at(
            world,
            player,
            found.bounds.center().x,
            found.bounds.max.y - 6,
        );
        for plinth in sites::orrery_plinths(&found) {
            world.stage_star_chart(plinth);
        }
        for vault in stamps().iter().filter_map(sites::vault_plinth).take(5) {
            world.stage_star_chart(vault);
        }
    });
    stage.settle(4);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::StarCharts);
    seat.selected_menu_item = 3;
    seat.message.clear();
    stage.settle(3);
}

/// The "Ask Index" page (Wave 11 carryover): the archivist asked about the ore nearby, its
/// answer under the list and in the speech box.
fn ask_index(stage: &mut SceneStage<'_>) {
    archivist(stage);
    stage.play_on(|world| {
        let player = world.player_ids().next().unwrap_or(PlayerId::new(0));
        let mut journal = world.expedition().clone();
        journal.found = journal.found.max(66);
        journal.descents = 0b0111_1111;
        world.set_expedition(journal);
        let answer = fatherload_simulation::ancient_city::questions::ore_answer(
            fatherload_simulation::ancient_city::questions::OreFind::Ore(
                fatherload_simulation::terrain::MineralKind::Mythril,
            ),
            5,
        );
        world.stage_archivist_line(player, answer);
    });
    stage.settle(2);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Archivist);
    seat.selected_menu_item = 0;
    seat.message.clear();
    seat.fanfare = None;
    stage.settle(3);
    // The journal's milestones were earned long ago: no medal over the page.
    stage.seat(0).fanfare = None;
}

/// The alloy portals framing the way in under the seal.
fn portals(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let levels = sites::avenue_levels(&settings, &layout);
        let Some(first) = levels.first().copied() else {
            return;
        };
        let centre = SCENE_X.div_euclid(sites::PORTAL_SPACING) * sites::PORTAL_SPACING
            + sites::PORTAL_SPACING / 2
            + sites::PORTAL_SPACING;
        // Find the portal's floor (its stone sill) above the first avenue.
        let mut y = first - 40;
        while y < first {
            if world
                .terrain()
                .tile(TilePosition { x: centre, y })
                .is_some_and(|tile| {
                    tile.kind
                        == fatherload_simulation::terrain::TileKind::City(
                            fatherload_simulation::ancient_city::CityTile::StreetLamp,
                        )
                })
            {
                break;
            }
            y += 1;
        }
        stage_at(world, player, centre, y + 7);
        reveal(world, player, centre - 40..centre + 40, y - 20..y + 30);
    });
    stage.settle(60);
}

pub const ANCIENT_CITY_SCENES: &[Scene] = &[
    Scene {
        name: "city_megacity_avenue",
        summary: "Descent VII, the Buried Megacity: a lamp-lit avenue, its tram pulling into the station where the rig waits, a drone, a sentinel and a scrap crawler about",
        setup: |stage| avenue(stage, false),
    },
    Scene {
        name: "city_tram_ride",
        summary: "Riding a Megacity tram: the rig aboard the cream-and-teal car as it pulls out under its wire",
        setup: |stage| avenue(stage, true),
    },
    Scene {
        name: "city_tower_lift",
        summary: "A Megacity tower: glass floors on builder alloy frames, the lift cab carrying the rig between the avenues",
        setup: tower_lift,
    },
    Scene {
        name: "city_lantern_library",
        summary: "The Lantern Library: the archivist wakes in its tower of lamps and greets the rig, with the library's story card",
        setup: |stage| lantern_library(stage, true),
    },
    Scene {
        name: "city_lantern_library_hall",
        summary: "The Lantern Library's floors and lamps, the archivist hovering by the rig",
        setup: |stage| lantern_library(stage, false),
    },
    Scene {
        name: "city_archivist",
        summary: "The archivist companion: its drone by the rig, its scan marks round ore under the street, and a word on the scrap crawler (the seat's speech box)",
        setup: archivist,
    },
    Scene {
        name: "city_alarm_hunt",
        summary: "The city's alarm gone full: maintenance drones and a street sentinel hunt the rig (the HUD says HUNTED)",
        setup: alarm_hunt,
    },
    Scene {
        name: "city_archive_post",
        summary: "The Archive Post, Descent VII's outpost: the Lamplighters and the builders' machines in their hall",
        setup: |stage| archive_post(stage, false),
    },
    Scene {
        name: "city_archive_post_talk",
        summary: "Talking with Curator Thessaly at the Archive Post: her memory of Ines Calloway",
        setup: |stage| archive_post(stage, true),
    },
    Scene {
        name: "city_portals",
        summary: "The way into Descent VII: builder alloy portals framing the street under the alloy seal",
        setup: portals,
    },
    Scene {
        name: "city_engine_hall",
        summary: "An Engine Hall at the Great Engine's beat: pistons rising in turn (the rig riding one up), crushers over the belts, a cog roller, a spark wisp and a rivet hound",
        setup: engine_hall,
    },
    Scene {
        name: "city_crusher_line",
        summary: "A crusher line: a low corridor of crushers slamming in a wave over a belt, their lamps warning before each slam",
        setup: |stage| set_piece(stage, CityStampKind::CrusherLine, &[EnemyKind::CogRoller]),
    },
    Scene {
        name: "city_piston_shaft",
        summary: "A piston shaft: a zig-zag of pistons and ledges to ride up to the cache at the top",
        setup: |stage| set_piece(stage, CityStampKind::PistonShaft, &[EnemyKind::SparkWisp]),
    },
    Scene {
        name: "city_great_engine",
        summary: "The Great Engine: its flywheel turning on the beat between two piston staircases up to the gallery's relic",
        setup: |stage| {
            set_piece(
                stage,
                CityStampKind::GreatEngine,
                &[EnemyKind::RivetHound, EnemyKind::SparkWisp],
            );
        },
    },
    Scene {
        name: "city_archive_stacks",
        summary: "Descent VII, the Archive of Stars: shelf halls under star glass lamps, page moths, an index spider and an archive warden",
        setup: |stage| {
            set_piece(
                stage,
                CityStampKind::Stacks,
                &[
                    EnemyKind::PageMoth,
                    EnemyKind::PageMoth,
                    EnemyKind::IndexSpider,
                    EnemyKind::ArchiveWarden,
                ],
            );
        },
    },
    Scene {
        name: "city_vault_plate",
        summary: "A plate vault: the rig on the plate outside the gate, the chart's plinth glowing inside",
        setup: |stage| vault(stage, VaultKind::Plate),
    },
    Scene {
        name: "city_vault_lever",
        summary: "A crawlway vault: the gate the lever up the crawlway opens, the plinth inside",
        setup: |stage| vault(stage, VaultKind::Lever),
    },
    Scene {
        name: "city_vault_light",
        summary: "A light vault: red, green and blue glass between the rig (its lamp filtered red) and the plinth",
        setup: |stage| vault(stage, VaultKind::Light),
    },
    Scene {
        name: "city_vault_grand",
        summary: "A grand vault: plate, gate, plate, gate and three colours of glass before the plinth",
        setup: |stage| vault(stage, VaultKind::Grand),
    },
    Scene {
        name: "city_orrery",
        summary: "The Orrery: the builders' sky circling on its arms, the hanging stone's line of light, the plinths' gold light, a warden and a spider",
        setup: orrery,
    },
    Scene {
        name: "city_ask_index",
        summary: "Wave 11 carryover: the Ask Index page: questions for the archivist (ore, finds, the lore it knows) and its answer about the ore under the street",
        setup: ask_index,
    },
    Scene {
        name: "city_star_charts",
        summary: "The star charts page: every chart read listed, the charted sky drawn beside it, one star selected",
        setup: star_charts,
    },
];
