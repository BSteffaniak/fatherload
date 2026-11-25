//! The friendly aliens' scenes (Wave 12 aliens track): each species' first home and town on
//! its round ground, first contact, the talk panel before and after learning the tongue, an
//! elder's chapter half understood, the trader's page, and a home and monument raised.
//!
//! Every scene is a Deep Descent world after the ending (the flight scenes' world), the seat's
//! rig set down in a town of the species' first home near the home system, early afternoon.

#![allow(
    clippy::similar_names,
    reason = "scene staging names the player and its place"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    alien::{
        AlienSpecies, AlienState, AlienTown,
        atlas::first_home,
        state::{AlienHome, SpeciesLog},
        town::{self, AlienBuildingKind, towns_of},
    },
    entity::{EntityKind, EntityState},
    flight::{FlightPlace, StarDrive},
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    world::{SectionKey, SectionPayload, strip_point, strip_position},
};

use super::{Scene, SceneStage, flight_scenes};
use crate::shell::{ModalScreen, aliens::AlienPage};

pub(super) static ALIEN_SCENES: &[Scene] = &[
    Scene {
        name: "alien_vessi_town",
        summary: "A Vessi town on its ice world: spiral snow-vaults lit warm from inside round the song spire, the tall stilt-legged singers about their afternoon",
        setup: |stage| town_scene(stage, AlienSpecies::Vessi, 0),
    },
    Scene {
        name: "alien_tokkari_town",
        summary: "A Tokkari hold: terraced mound-halls of fused stone and the forge chimney, the six-limbed smiths scuttling about",
        setup: |stage| town_scene(stage, AlienSpecies::Tokkari, 0),
    },
    Scene {
        name: "alien_siphra_town",
        summary: "A Siphra town on its airless world: lantern-pods hung from crystal pylons round the memory pylon, the glass-bodied keepers drifting",
        setup: |stage| town_scene(stage, AlienSpecies::Siphra, 0),
    },
    Scene {
        name: "alien_vessi_homes",
        summary: "The Vessi town's end: dwellings, the plot for sale and the builder's workshop",
        setup: |stage| town_scene(stage, AlienSpecies::Vessi, 40),
    },
    Scene {
        name: "alien_first_contact",
        summary: "First contact: the rig rolls into a Tokkari hold and the HUD names the species",
        setup: first_contact,
    },
    Scene {
        name: "alien_talk_glyphs",
        summary: "Talking to a Vessi with no understanding of their tongue: every word in their ring glyphs",
        setup: |stage| talk_scene(stage, AlienSpecies::Vessi, town::TRADER, 0, AlienPage::Talk),
    },
    Scene {
        name: "alien_talk_fragments",
        summary: "The Tokkari elder's request half understood: the little words show through the notched runes",
        setup: |stage| {
            talk_scene(
                stage,
                AlienSpecies::Tokkari,
                town::ELDER,
                330,
                AlienPage::Talk,
            );
        },
    },
    Scene {
        name: "alien_talk_fluent",
        summary: "Talking to a Siphra after learning their tongue: every word readable",
        setup: |stage| {
            talk_scene(
                stage,
                AlienSpecies::Siphra,
                town::TEACHER,
                980,
                AlienPage::Talk,
            );
        },
    },
    Scene {
        name: "alien_trade",
        summary: "The Vessi trader's page: prized ores bought from the bay, their goods for sale",
        setup: |stage| {
            talk_scene(
                stage,
                AlienSpecies::Vessi,
                town::TRADER,
                450,
                AlienPage::Trade,
            );
        },
    },
    Scene {
        name: "alien_home_and_monument",
        summary: "A Siphra town with the player's lantern-pod home finished and the memory pylon's heart raised",
        setup: |stage| home_and_monument(stage, false),
    },
    Scene {
        name: "alien_monument_raised",
        summary: "The Siphra town's monument plinth crowned: the memory pylon's heart a chapter raised, standing for everyone",
        setup: |stage| home_and_monument(stage, true),
    },
];

/// The first home of `species` in the scene world and its towns.
fn first_towns(
    seed: u64,
    species: AlienSpecies,
) -> Option<(SpaceBodyKey, std::sync::Arc<Vec<std::sync::Arc<AlienTown>>>)> {
    let id = first_home(seed, species)?;
    let params = fatherload_simulation::place::planet_params_for(seed, id)?;
    Some((id, towns_of(&params)))
}

/// The seat's rig fitted for space and set down on `id`'s strip at `column` (air full).
fn set_down_at(world: &mut WorldState, player: PlayerId, id: SpaceBodyKey, column: i32, row: i32) {
    let Some(body_ref) = world.space().body_ref(id) else {
        return;
    };
    flight_scenes::launch(
        world,
        player,
        fatherload_simulation::position::WorldPos::default(),
        0.0,
        (0.0, 0.0),
        StarDrive::Off,
        true,
    );
    let _ = world.move_to_place(
        player,
        Place::OnBody(BodyId::Space(id)),
        strip_position(f64::from(column) + 0.5, f64::from(row - 2)),
    );
    let capacity = world.air_capacity_of(player);
    if let Some(rig) = world.player_mut(player) {
        rig.flight.place = FlightPlace::Landed(body_ref);
        rig.flight.air = capacity;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
}

/// Puts `log` in for `player` and `species` (understanding, met), through the replicated
/// section (as a client would receive it).
fn with_log(
    world: &mut WorldState,
    player: PlayerId,
    species: AlienSpecies,
    edit: impl FnOnce(&mut SpeciesLog, &mut AlienState),
) {
    let mut state = world.aliens().clone();
    let mut log = state.log(player, species);
    edit(&mut log, &mut state);
    *state.log_mut(player, species) = log;
    world.apply_section(SectionKey::Aliens, SectionPayload::Aliens(Box::new(state)));
}

/// A town of `species`, the rig `offset` columns from its middle.
fn town_scene(stage: &mut SceneStage<'_>, species: AlienSpecies, offset: i32) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let seed = world.world_seed();
        let Some((id, towns)) = first_towns(seed, species) else {
            return;
        };
        let found = &towns[0];
        set_down_at(
            world,
            seats[0].player,
            id,
            found.site.centre + offset,
            found.ground_row(),
        );
        with_log(world, seats[0].player, species, |log, _| {
            log.understanding = 200;
            log.met_millis = Some(1);
        });
    });
    stage.settle(150);
}

/// Rolling into a Tokkari hold for the first time.
fn first_contact(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let seed = world.world_seed();
        let Some((id, towns)) = first_towns(seed, AlienSpecies::Tokkari) else {
            return;
        };
        let found = &towns[0];
        set_down_at(
            world,
            seats[0].player,
            id,
            found.first_column() + 6,
            found.ground_row(),
        );
    });
    stage.settle(40);
}

/// Talking to resident `slot` of a town of `species` with `understanding`, on `page`.
fn talk_scene(
    stage: &mut SceneStage<'_>,
    species: AlienSpecies,
    slot: usize,
    understanding: u16,
    page: AlienPage,
) {
    let mut character = None;
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let seed = world.world_seed();
        let Some((id, towns)) = first_towns(seed, species) else {
            return;
        };
        let found = &towns[0];
        set_down_at(
            world,
            seats[0].player,
            id,
            found.site.centre,
            found.ground_row(),
        );
        with_log(world, seats[0].player, species, |log, _| {
            log.understanding = understanding;
            log.met_millis = Some(1);
            if slot == town::ELDER {
                log.offered = Some(1);
                log.chapters_done = 1;
            }
        });
        if let Some(rig) = world.player_mut(seats[0].player) {
            rig.cargo_bay.add(species.def().valued_ores[0].0, 6);
            rig.cargo_bay.add(species.def().valued_ores[1].0, 3);
        }
        character = Some((id, found.population.characters[slot].id));
    });
    stage.settle(60);
    let Some((id, character)) = character else {
        return;
    };
    // Beside them (they stand still once spoken to).
    for _ in 0..30 {
        let player = stage.world().player_ids().next();
        let spot = {
            let world = stage.world();
            world
                .store_entities(StoreKey::Body(id))
                .and_then(|entities| {
                    entities
                        .of_kind(EntityKind::Npc)
                        .find_map(|entity| match entity.state {
                            EntityState::Npc(state)
                                if state.character == character && state.inside.is_none() =>
                            {
                                let frame = world.store_frame(StoreKey::Body(id))?;
                                let x = state.x_at(world.simulation_tick(), 0.0);
                                Some(strip_point(frame.world(x, entity.body.y)))
                            }
                            _ => None,
                        })
                })
        };
        if let (Some(player), Some((u, v))) = (player, spot) {
            stage.play_on(|world| {
                let _ = world.move_to_place(
                    player,
                    Place::OnBody(BodyId::Space(id)),
                    strip_position(u.floor() + 1.5, v - 2.0),
                );
            });
            stage.settle(3);
            if stage.world().npcs_in_reach(player).iter().any(|npc| {
                stage
                    .world()
                    .npc(*npc)
                    .is_some_and(|(_, state)| state.character == character)
            }) {
                break;
            }
        }
        stage.settle(30);
    }
    stage.hold(
        1,
        crate::input::PlayerInput {
            interact: true,
            ..crate::input::PlayerInput::default()
        },
    );
    stage.settle(6);
    let seat = stage.seat(0);
    if seat.modal == Some(ModalScreen::AlienTalk) {
        seat.alien.page = page;
        seat.selected_menu_item = 0;
    }
    stage.settle(2);
}

/// The player's home finished and the town's monument raised, the rig at the home (or at the
/// monument: the two stand too far apart for one view).
fn home_and_monument(stage: &mut SceneStage<'_>, at_monument: bool) {
    stage.play(1, |world, seats| {
        flight_scenes::space_world(world);
        let seed = world.world_seed();
        let species = AlienSpecies::Siphra;
        let Some((id, towns)) = first_towns(seed, species) else {
            return;
        };
        let found = &towns[0];
        let plot = found
            .buildings
            .iter()
            .find(|building| building.kind == AlienBuildingKind::HomePlot)
            .copied();
        let monument = found.building(AlienBuildingKind::Monument).copied();
        let column = match (plot, monument) {
            // Beside the plinth, not on it.
            (_, Some(monument)) if at_monument => monument.door_column() - 4,
            (Some(plot), _) if !at_monument => plot.door_column(),
            _ => found.site.centre,
        };
        set_down_at(world, seats[0].player, id, column, found.ground_row());
        let player = seats[0].player;
        with_log(world, player, species, |log, state| {
            log.understanding = 640;
            log.met_millis = Some(1);
            log.chapters_done = 4;
            state.monuments.insert(found.id);
            if let Some(plot) = plot {
                state.homes.push(AlienHome {
                    owner: player,
                    town: found.id,
                    body: id,
                    plot: plot.index,
                    species,
                    builder: found.population.characters[town::BUILDER].id,
                    paid: 2_800,
                    bought_millis: 0,
                    ready_millis: 1,
                    home: Some(fatherload_simulation::home::Home::built(
                        fatherload_simulation::home::blueprint::Blueprint::Cottage,
                        3,
                        None,
                        1,
                    )),
                });
            }
        });
    });
    stage.settle(150);
}
