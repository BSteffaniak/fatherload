//! Endgame scenes (Wave 13 endgame): the Works counter at Halcyon Dock (the Halcyon Gate's
//! model and its parts), at a great yard (the Starwright hull on its cradle), at an alien-run
//! station (the tech vat and its marks) and at an outfitter; the shipyard's late tiers with
//! their bills of space materials; the Records (the hunt list and completion); the gate in
//! space beside Halcyon; the Starwright ship in flight; an estate with every wing on an alien
//! world; the cargo bay's space materials; the progression screen's Space group; split screen
//! and touch.

#![allow(
    clippy::suboptimal_flops,
    reason = "scene staging positions, not simulation arithmetic"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    alien::{AlienBuildingKind, AlienSpecies, atlas::first_home, state::AlienHome, town},
    endgame::{
        Career, EndgameState, Estate, StarwrightPart,
        materials::SPACE_MATERIALS,
        works::{ESTATE_WINGS, builds_starwright},
    },
    flight::{FlightPlace, SpaceQuery, StarDrive},
    hostiles::Faction,
    milestone::Milestone,
    place::{BodyId, Place},
    position::WorldPos,
    space::SpaceSector,
    station::{
        AlienTech, ShipGear, StationId, StationOwner, StationProfile,
        profile::{profile_of, station_ref},
    },
    world::strip_position,
};

use super::{Scene, SceneStage, flight_scenes};
use crate::{
    input::{devices::InputDevice, touch::TouchView},
    shell::{
        ModalScreen,
        station::{SeatStation, StationPage},
    },
};

const HALCYON: StationId = StationId {
    sector: fatherload_simulation::space::HOME_SECTOR,
    system: 0,
    index: 0,
};

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
                for system in &space.sector(SpaceSector::new(x, y)).systems {
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
    HALCYON
}

fn great_yard(world: &WorldState) -> StationId {
    station_where(world, builds_starwright)
}

fn alien_station(world: &WorldState) -> StationId {
    station_where(world, |profile| {
        matches!(profile.owner, StationOwner::Alien(_)) && profile.alien_tech
    })
}

fn body_of(world: &WorldState, id: StationId) -> Option<fatherload_simulation::flight::FlightBody> {
    let galaxy = world.space();
    galaxy.body(station_ref(&galaxy, id)?)
}

/// A late rig: every ship line at `tier`, all four alien techs, a bay of space materials and a
/// fat purse.
fn late_rig(world: &mut WorldState, player: PlayerId, tier: u8) {
    if let Some(rig) = world.player_mut(player) {
        rig.hazard_gear.ship = ShipGear {
            hull: tier,
            engine: tier,
            drive_range: tier,
            hold: tier,
            life_support: tier,
            heat_shielding: 2,
            cold_insulation: 2,
            toxic_filter: 1,
            gravity_struts: 1,
            alien: 0b1111,
        };
        rig.credits = 6_665_550;
        for (index, mineral) in SPACE_MATERIALS.iter().enumerate() {
            let units = [180, 120, 140, 70, 24, 9, 3, 8][index];
            rig.cargo_bay.add(*mineral, units);
        }
    }
}

/// A crew some way into the endgame: four gate parts delivered, the Gilded Maw beaten, worlds
/// landed and a career started.
fn lived_in(world: &mut WorldState, player: PlayerId, career: impl FnOnce(&mut Career)) {
    let mut endgame = EndgameState {
        gate_parts: 0b0000_1111,
        ..EndgameState::default()
    };
    let mut record = Career {
        mined: 1_840,
        comet_dust: 14,
        catalogue: 0b0111_1111,
        ..Career::default()
    };
    let galaxy = world.space();
    for x in 0..3 {
        for system in &galaxy.space().sector(SpaceSector::new(x, 1)).systems {
            for body in system.planets() {
                record
                    .landed
                    .insert(fatherload_simulation::space::SpaceBodyId {
                        sector: SpaceSector::new(x, 1),
                        system: system.index,
                        body: body.index,
                    });
                record.themes.insert(body.theme);
            }
        }
    }
    career(&mut record);
    endgame.careers.insert(player, record);
    world.set_endgame(endgame);
    let mut hostiles = world.hostiles().clone();
    hostiles.bosses_beaten.insert(Faction::Pirates);
    world.set_hostiles(hostiles);
}

/// Docked at `pick(world)` with the hub open on `page`, row `selected` picked.
fn docked(
    stage: &mut SceneStage<'_>,
    pick: fn(&WorldState) -> StationId,
    page: StationPage,
    selected: usize,
    tier: u8,
    career: fn(&mut Career),
) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let player = seats[0].player;
        let station = pick(world);
        let Some(body) = body_of(world, station) else {
            return;
        };
        let port = body.port().unwrap_or(body.centre);
        flight_scenes::launch(
            world,
            player,
            port,
            body.dock_orientation(),
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        late_rig(world, player, tier);
        if let Some(rig) = world.player_mut(player) {
            rig.flight.place = FlightPlace::Docked(body.body);
        }
        lived_in(world, player, career);
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

/// Flying near Halcyon Dock with the gate finished (`parts`) beside it.
fn gate_in_space(stage: &mut SceneStage<'_>, parts: u16) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let player = seats[0].player;
        let Some(body) = body_of(world, HALCYON) else {
            return;
        };
        let (sin, cos) = body.port_angle.sin_cos();
        let reach = body.radius * 9.0;
        let gate = body.centre.offset(-sin * reach, cos * reach);
        // Between the dock and the gate, a little out.
        let (dx, dy) = body.centre.delta(gate);
        let pos = body
            .centre
            .offset(dx * 0.5 + cos * reach * 0.6, dy * 0.5 + sin * reach * 0.6);
        flight_scenes::launch(world, player, pos, 0.0, (0.0, 0.0), StarDrive::Off, true);
        late_rig(world, player, 5);
        lived_in(world, player, |career| career.starwright = 0b11);
        let mut endgame = world.endgame().clone();
        endgame.gate_parts = parts;
        world.set_endgame(endgame);
    });
    stage.settle(30);
}

/// The Starwright ship with the Heartline Drive cruising out by a world.
fn starwright_flight(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let player = seats[0].player;
        let Some(body) = body_of(world, HALCYON) else {
            return;
        };
        let pos = body.centre.offset(body.radius * 30.0, -body.radius * 12.0);
        flight_scenes::launch(world, player, pos, 0.6, (40.0, -25.0), StarDrive::Off, true);
        late_rig(world, player, 5);
        lived_in(world, player, |career| {
            career.starwright = StarwrightPart::Hull.bit() | StarwrightPart::Engine.bit();
            career.heartline = true;
        });
    });
    stage.settle(20);
}

/// A finished Siphra home with every estate wing, the rig at its door.
fn estate(stage: &mut SceneStage<'_>, wings: usize) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let player = seats[0].player;
        let seed = world.world_seed();
        let species = AlienSpecies::Siphra;
        let Some(id) = first_home(seed, species) else {
            return;
        };
        let Some(params) = fatherload_simulation::place::planet_params_for(seed, id) else {
            return;
        };
        let towns = town::towns_of(&params);
        let found = &towns[0];
        let Some(plot) = found
            .buildings
            .iter()
            .find(|building| building.kind == AlienBuildingKind::HomePlot)
            .copied()
        else {
            return;
        };
        let Some(body_ref) = world.space().body_ref(id) else {
            return;
        };
        flight_scenes::launch(
            world,
            player,
            WorldPos::default(),
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        let _ = world.move_to_place(
            player,
            Place::OnBody(BodyId::Space(id)),
            strip_position(
                f64::from(plot.door_column() - 14) + 0.5,
                f64::from(found.ground_row() - 2),
            ),
        );
        let capacity = world.air_capacity_of(player);
        if let Some(rig) = world.player_mut(player) {
            rig.flight.place = FlightPlace::Landed(body_ref);
            rig.flight.air = capacity;
        }
        let mut aliens = world.aliens().clone();
        aliens.log_mut(player, species).understanding = 760;
        aliens.log_mut(player, species).met_millis = Some(1);
        let mut home = fatherload_simulation::home::Home::built(
            fatherload_simulation::home::blueprint::Blueprint::Cottage,
            3,
            None,
            1,
        );
        for wing in ESTATE_WINGS.iter().take(wings) {
            home.expand(*wing);
        }
        aliens.homes.push(AlienHome {
            owner: player,
            town: found.id,
            body: id,
            plot: plot.index,
            species,
            builder: found.population.characters[town::BUILDER].id,
            paid: 56_000,
            bought_millis: 0,
            ready_millis: 1,
            home: Some(home),
        });
        world.apply_section(
            fatherload_simulation::world::SectionKey::Aliens,
            fatherload_simulation::world::SectionPayload::Aliens(Box::new(aliens)),
        );
        let mut endgame = world.endgame().clone();
        endgame.estates.insert(
            (found.id, plot.index),
            Estate {
                expansions: ESTATE_WINGS.iter().take(wings).copied().collect(),
                visited_day: None,
                gifts: 12,
            },
        );
        world.set_endgame(endgame);
    });
    stage.settle(150);
}

/// The progression screen with the Space group's first achievements earned.
fn achievements(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let player = seats[0].player;
        flight_scenes::launch(
            world,
            player,
            WorldPos::default(),
            0.0,
            (0.0, 0.0),
            StarDrive::Off,
            true,
        );
        if let Some(rig) = world.player_mut(player) {
            rig.milestones.primed = true;
            for (tick, milestone) in Milestone::ALL.into_iter().enumerate() {
                let space =
                    milestone.group() == fatherload_simulation::milestone::MilestoneGroup::Space;
                let early = matches!(
                    milestone,
                    Milestone::SpaceMiner
                        | Milestone::CometChaser
                        | Milestone::FirstWorld
                        | Milestone::TenWorlds
                        | Milestone::FirstContactMade
                        | Milestone::FluentTongue
                        | Milestone::PirateKingBeaten
                        | Milestone::TopShipLine
                        | Milestone::AlienHome
                );
                if !space || early {
                    let _ = rig.earn_milestone(milestone, tick as u64 * 60);
                }
            }
        }
    });
    stage.settle(5);
    stage.seat(0).modal = Some(ModalScreen::Achievements);
    let index = Milestone::ALL
        .iter()
        .position(|milestone| *milestone == Milestone::StarwrightShip)
        .unwrap_or(0);
    stage.seat(0).selected_menu_item = index;
}

/// Two seats docked: one at Halcyon's Works, one at a great yard's Records.
fn split(stage: &mut SceneStage<'_>) {
    stage.play(2, |world, seats| {
        flight_scenes::space_world(world);
        for (seat, station) in [(0, HALCYON), (1, great_yard(world))] {
            let player = seats[seat].player;
            let Some(body) = body_of(world, station) else {
                continue;
            };
            let port = body.port().unwrap_or(body.centre);
            flight_scenes::launch(
                world,
                player,
                port,
                body.dock_orientation(),
                (0.0, 0.0),
                StarDrive::Off,
                true,
            );
            late_rig(world, player, 3);
            if let Some(rig) = world.player_mut(player) {
                rig.flight.place = FlightPlace::Docked(body.body);
            }
        }
        lived_in(world, seats[0].player, |_| {});
    });
    stage.settle(6);
    for (seat, page, row) in [(0, StationPage::Works, 0), (1, StationPage::Records, 0)] {
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

const fn no_career(_: &mut Career) {}

const fn starwright_hull(career: &mut Career) {
    career.starwright = StarwrightPart::Hull.bit();
}

const fn marked(career: &mut Career) {
    career.marks[AlienTech::XenoLens as usize] = 2;
    career.marks[AlienTech::PhaseLattice as usize] = 1;
}

pub const ENDGAME_SCENES: &[Scene] = &[
    Scene {
        name: "endgame_works_halcyon",
        summary: "Halcyon Dock's Works: common space materials, the Halcyon Gate's parts (four of eight in, the model lit to match), the outfitter",
        setup: |stage| docked(stage, |_| HALCYON, StationPage::Works, 4, 3, no_career),
    },
    Scene {
        name: "endgame_works_starwright",
        summary: "A great yard's Works: the Starwright Hull fitted on its cradle, the Engine and the Heartline Drive with their bills",
        setup: |stage| docked(stage, great_yard, StationPage::Works, 4, 5, starwright_hull),
    },
    Scene {
        name: "endgame_works_alien",
        summary: "An alien-run station's Works: alien tech marks behind the tongue (the xeno lens at Mk III next), the grown vat's pods ringed per mark",
        setup: |stage| docked(stage, alien_station, StationPage::Works, 2, 3, marked),
    },
    Scene {
        name: "endgame_shipyard_bills",
        summary: "A great yard's shipyard at tier 2: the tier 3 lines' bills of belt ore beside their prices",
        setup: |stage| docked(stage, great_yard, StationPage::Shipyard, 1, 2, no_career),
    },
    Scene {
        name: "endgame_records",
        summary: "The Records: the hunt list's banners (the Gilded Maw beaten, gold; the Swarm Queen and the Arbiter at large) and the completion dial",
        setup: |stage| docked(stage, great_yard, StationPage::Records, 3, 3, no_career),
    },
    Scene {
        name: "endgame_gate_building",
        summary: "The Halcyon Gate rising beside Halcyon Dock: four ring segments in, the rest scaffold",
        setup: |stage| gate_in_space(stage, 0b0000_1111),
    },
    Scene {
        name: "endgame_gate_open",
        summary: "The Halcyon Gate finished: six segments, the lens breathing, the keystone",
        setup: |stage| gate_in_space(stage, 0b1111_1111),
    },
    Scene {
        name: "endgame_starwright_flight",
        summary: "The Starwright ship in flight: white-gold hull plates, twin gold engine bells and the Heartline's keel line",
        setup: starwright_flight,
    },
    Scene {
        name: "endgame_estate",
        summary: "An estate on a Siphra world with every wing: annexes, a kitchen chimney, a greenhouse, a garage, an upper storey, a gallery and an observatory",
        setup: |stage| estate(stage, ESTATE_WINGS.len()),
    },
    Scene {
        name: "endgame_estate_young",
        summary: "A home on a Siphra world with its first two wings",
        setup: |stage| estate(stage, 2),
    },
    Scene {
        name: "endgame_achievements",
        summary: "The progression screen's Space group: the first space achievements earned, the Starwright ship's selected",
        setup: achievements,
    },
    Scene {
        name: "endgame_split",
        summary: "Split screen: one seat at Halcyon's Works, the other at a great yard's Records",
        setup: split,
    },
    Scene {
        name: "touch_endgame",
        summary: "Touch at the Works: Halcyon's rows to tap over the gate's model",
        setup: |stage| {
            docked(stage, |_| HALCYON, StationPage::Works, 4, 3, no_career);
            stage.use_devices(&[InputDevice::Touch]);
            stage.touch_view = TouchView::default();
        },
    },
];
