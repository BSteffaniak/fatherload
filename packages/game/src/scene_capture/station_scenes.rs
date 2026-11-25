//! Station scenes (Wave 12): stations from outside by owner (human, alien-run, derelict),
//! lining up at a port, and a docked station's hub: the concourse, the shipyard (a human yard's
//! lines and an alien yard's tech), the market, the bounty board, the bar (human and alien),
//! a derelict to strip, and two seats at two stations in split screen. A Deep Descent world
//! after the ending, its stations from the real galaxy.

use fatherload_simulation::{
    PlayerId, WorldState,
    attitude::RigAttitude,
    flight::{FlightPlace, RigFlight, SpaceQuery, StarDrive, space},
    position::WorldPos,
    space::SpaceSector,
    station::{
        Good, StationId, StationKind, StationOwner, StationProfile,
        profile::{profile_of, station_ref},
    },
    terrain::Terrain,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{Scene, SceneStage};
use crate::shell::{
    ModalScreen,
    station::{SeatStation, StationPage},
};

/// Seed of every station scene world.
const SCENE_SEED: u64 = 0x00C0_FFEE;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

/// A Deep Descent world after the ending, ten minutes on the clock.
fn space_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    let hz = u64::from(world.tick_rate_hz());
    world.set_simulation_tick(fatherload_simulation::SimulationTick::new(600 * hz));
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    world.set_heart(heart);
}

/// The first station near home whose profile passes `wanted`.
fn station_where(world: &WorldState, wanted: impl Fn(&StationProfile) -> bool) -> StationId {
    let galaxy = world.space();
    let space = galaxy.space();
    for reach in 0..6_i32 {
        for y in -reach..=reach {
            for x in -reach..=reach {
                if x.abs().max(y.abs()) != reach {
                    continue;
                }
                let sector = space.sector(SpaceSector::new(x, y));
                for system in &sector.systems {
                    for station in &system.stations {
                        let profile = profile_of(space.seed(), system, station);
                        if wanted(&profile) {
                            return profile.id;
                        }
                    }
                }
            }
        }
    }
    panic!("no such station near home");
}

const HALCYON: StationId = StationId {
    sector: fatherload_simulation::space::HOME_SECTOR,
    system: 0,
    index: 0,
};

fn alien(world: &WorldState) -> StationId {
    station_where(world, |profile| {
        matches!(profile.owner, StationOwner::Alien(_)) && profile.bounties
    })
}

fn alien_yard(world: &WorldState) -> StationId {
    station_where(world, |profile| {
        matches!(profile.owner, StationOwner::Alien(_))
    })
}

fn derelict(world: &WorldState) -> StationId {
    station_where(world, |profile| profile.owner == StationOwner::Derelict)
}

fn shipyard(world: &WorldState) -> StationId {
    station_where(world, |profile| {
        profile.owner == StationOwner::Human && profile.kind == StationKind::Shipyard
    })
}

/// `player`'s rig fitted and put at `pos` in space, turned `orientation`.
fn launch(world: &mut WorldState, player: PlayerId, pos: WorldPos, orientation: f32) {
    let mut heart = world.heart().clone();
    heart.star_drive.insert(player);
    world.set_heart(heart);
    let park = world.space_park().unwrap_or((0.0, 0.0));
    let capacity = fatherload_simulation::flight::air_capacity(
        fatherload_simulation::protection::HazardGear::MAXED.life_support,
    );
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hazard_gear.ship.engine = 1;
        rig.hazard_gear.ship.hold = 1;
        rig.hull = rig.max_hull() * 0.7;
        rig.fuel = rig.fuel_capacity * 0.55;
        rig.credits = 14_250;
        rig.x = park.0;
        rig.y = park.1;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.attitude = RigAttitude {
            orientation: fatherload_simulation::attitude::quantize_angle(orientation),
            angular_velocity: 0.0,
        };
        rig.flight = RigFlight {
            place: FlightPlace::Space,
            pos,
            drive: StarDrive::Off,
            air: capacity * 0.9,
            exit_column: 120,
            ..RigFlight::default()
        };
    }
}

/// The station as flight sees it.
fn body_of(world: &WorldState, id: StationId) -> fatherload_simulation::flight::FlightBody {
    let galaxy = world.space();
    let body = station_ref(&galaxy, id).expect("station ref");
    galaxy.body(body).expect("station body")
}

/// Outside station `pick(world)`, `distance` tiles off its port on its axis, facing in.
fn outside(stage: &mut SceneStage<'_>, pick: fn(&WorldState) -> StationId, distance: f32) {
    stage.play(1, |world, seats| {
        space_world(world);
        let body = body_of(world, pick(world));
        let port = body.port().expect("port");
        let (dx, dy) = body.centre.delta(port);
        let length = dx.hypot(dy).max(1.0);
        let reach = space::tiles(distance);
        let pos = port.offset(dx / length * reach, dy / length * reach);
        launch(world, seats[0].player, pos, body.dock_orientation() + 0.1);
    });
    stage.settle(8);
}

/// Beside station `pick(world)`, `distance` tiles off its centre across its port's axis, the
/// rig turned so the whole station sits beside it in view.
fn beside(stage: &mut SceneStage<'_>, pick: fn(&WorldState) -> StationId, distance: f32) {
    stage.play(1, |world, seats| {
        space_world(world);
        let body = body_of(world, pick(world));
        let port = body.port().expect("port");
        let (dx, dy) = body.centre.delta(port);
        let length = dx.hypot(dy).max(1.0);
        let reach = space::tiles(distance);
        // Across the port's axis, a little toward the port.
        let pos = body.centre.offset(
            (-dy / length).mul_add(reach, dx / length * reach * 0.3),
            (dx / length).mul_add(reach, dy / length * reach * 0.3),
        );
        let bearing = fatherload_simulation::flight::nav::bearing(pos, body.centre);
        launch(
            world,
            seats[0].player,
            pos,
            bearing - std::f32::consts::FRAC_PI_2,
        );
    });
    stage.settle(8);
}

/// Some state for the hub to show: a mission under way and one done, goods in the hold, a
/// rumour heard.
fn lived_in(world: &mut WorldState, player: PlayerId, station: StationId) {
    let board = world.station_board(station);
    let mut ledger = world.station_ledger().clone();
    let mut missions: Vec<_> = board.iter().flatten().copied().take(2).collect();
    if let Some(first) = missions.first_mut() {
        first.progress = first.kind.goal();
    }
    if let Some(second) = missions.get_mut(1) {
        second.progress = second.kind.goal() / 2;
    }
    if !missions.is_empty() {
        ledger.missions.insert(player, missions);
    }
    let mut serials = ledger.serials(station);
    serials[0] += 1;
    serials[1] += 1;
    ledger.boards.insert(station, serials);
    let hold = ledger.holds.entry(player).or_default();
    hold.add(Good::MedKits, 3);
    hold.add(Good::WaterIce, 2);
    ledger.heard.insert((station, 0));
    world.set_station_ledger(ledger);
}

/// Docked at `pick(world)` with the hub open on `page`, row `selected` picked.
fn docked(
    stage: &mut SceneStage<'_>,
    pick: fn(&WorldState) -> StationId,
    page: StationPage,
    selected: usize,
) {
    stage.play(1, |world, seats| {
        space_world(world);
        let station = pick(world);
        let body = body_of(world, station);
        let port = body.port().expect("port");
        launch(world, seats[0].player, port, body.dock_orientation());
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.flight.place = FlightPlace::Docked(body.body);
            rig.cargo_bay.add(
                fatherload_simulation::item::ItemKind::Mineral(
                    fatherload_simulation::terrain::MineralKind::Silver,
                ),
                14,
            );
        }
        lived_in(world, seats[0].player, station);
    });
    stage.settle(6);
    let seat = stage.seat(0);
    seat.modal = Some(ModalScreen::Station);
    seat.station = SeatStation {
        page,
        abandon_armed: None,
    };
    seat.selected_menu_item = selected;
    stage.settle(2);
}

/// Two seats docked at two stations: one in Halcyon's shipyard, one at an alien bar.
fn split(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        space_world(world);
        for (seat, station) in [(0, HALCYON), (1, alien(world))] {
            let body = body_of(world, station);
            let port = body.port().expect("port");
            launch(world, seats[seat].player, port, body.dock_orientation());
            if let Some(rig) = world.player_mut(seats[seat].player) {
                rig.flight.place = FlightPlace::Docked(body.body);
            }
            lived_in(world, seats[seat].player, station);
        }
    });
    stage.settle(6);
    for (seat, page, row) in [(0, StationPage::Shipyard, 1), (1, StationPage::Bar, 0)] {
        let ui = stage.seat(seat);
        ui.modal = Some(ModalScreen::Station);
        ui.station = SeatStation {
            page,
            abandon_armed: None,
        };
        ui.selected_menu_item = row;
    }
    stage.settle(2);
}

pub const STATION_SCENES: &[Scene] = &[
    Scene {
        name: "station_exterior_human",
        summary: "Halcyon Dock from outside: a human hub, its solar wings, lit ring and green port light",
        setup: |stage| beside(stage, |_| HALCYON, 16.0),
    },
    Scene {
        name: "station_exterior_alien",
        summary: "An alien-run station from outside: a grown bulb, curving fins, breathing pods and a violet port",
        setup: |stage| beside(stage, alien, 16.0),
    },
    Scene {
        name: "station_exterior_derelict",
        summary: "A derelict station: a torn wing, a holed ring, dead windows, one dim lamp and debris adrift",
        setup: |stage| beside(stage, derelict, 16.0),
    },
    Scene {
        name: "station_docking",
        summary: "Lining up at an alien-run station's port: the docking guide's checks",
        setup: |stage| outside(stage, alien, 2.5),
    },
    Scene {
        name: "station_concourse",
        summary: "Docked at Halcyon Dock: the hub's counters over the concourse (spacers passing, the directory arch, a window on space)",
        setup: |stage| docked(stage, |_| HALCYON, StationPage::Hub, 0),
    },
    Scene {
        name: "station_shipyard",
        summary: "A human shipyard's yard: every ship line to tier 5, planet gear, flight gear; a hull on the gantry",
        setup: |stage| docked(stage, shipyard, StationPage::Shipyard, 1),
    },
    Scene {
        name: "station_shipyard_alien",
        summary: "An alien-run station's yard: planet gear and alien tech (phase lattice, graviton spool, xeno lens, symbiont weave)",
        setup: |stage| docked(stage, alien_yard, StationPage::Shipyard, 4),
    },
    Scene {
        name: "station_market",
        summary: "Halcyon Dock's market: ore for sale, goods made here and wanted here, the hold's goods to sell",
        setup: |stage| docked(stage, |_| HALCYON, StationPage::Market, 2),
    },
    Scene {
        name: "station_bounty_board",
        summary: "Halcyon Dock's bounty board: a finished mission to collect, one under way, the slots' fresh offers; pinned notices",
        setup: |stage| docked(stage, |_| HALCYON, StationPage::Board, 3),
    },
    Scene {
        name: "station_bar",
        summary: "Halcyon Dock's bar: three patrons (people-engine characters), a rumour heard, bottles and a steady neon sign",
        setup: |stage| docked(stage, |_| HALCYON, StationPage::Bar, 0),
    },
    Scene {
        name: "station_bar_alien",
        summary: "An alien-run station's bar: its kin at the counter, a human spacer, rumours for a round",
        setup: |stage| docked(stage, alien, StationPage::Bar, 1),
    },
    Scene {
        name: "station_derelict_salvage",
        summary: "Docked at a derelict: dark, scorched plating, one dimming lamp; strip the wreck",
        setup: |stage| docked(stage, derelict, StationPage::Hub, 0),
    },
    Scene {
        name: "station_split",
        summary: "Split screen at two stations: one seat in Halcyon's shipyard, the other at an alien bar",
        setup: split,
    },
];
