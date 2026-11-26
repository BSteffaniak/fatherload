//! Alien towns, tongues, trade, chapters and homes in the world (Wave 12 aliens track).
//!
//! A bot lands on each species' first home, walks into a town (first contact), talks to the
//! residents, studies, digs a lexicon stone, trades, plays the elder's whole arc (fetch,
//! explore, escort, raise the monument, the builders' trail) and buys a home; the state saves,
//! replicates and plays the same at any tick rate.

#![allow(
    clippy::float_cmp,
    reason = "positions are compared bit for bit on purpose"
)]

use std::sync::Arc;

use crate::{
    GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    alien::{
        AlienCommand, AlienEvent, AlienSpecies, AlienTile, AlienTown,
        atlas::first_home,
        language::{Fluency, Understanding},
        quest::{ChapterGoal, chapters},
        town::{self, AlienBuildingKind, towns_of},
        trade::AlienGood,
    },
    enemy::EnemyDifficulty,
    entity::{EntityId, EntityKind, EntityState},
    people::{CharacterId, PeopleCommand, TalkTopic},
    place::{BodyId, Place, SpaceBodyKey, StoreKey},
    position::WorldPos,
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    world::{SectionKey, strip_point, strip_position},
    worldgen::{GeneratorVersion, WorldSettings},
};

const SEED: u64 = 0xA11E_0012;
const PLAYER: PlayerId = PlayerId::new(1);

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

fn world_at(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    if let Some(rig) = world.player_mut(PLAYER) {
        // A rig that reached space: the Heart's upgrades and the hazard gear (a bare starter
        // rig freezes on an airless world's night side, Wave 12 bodylife's climate).
        crate::economy::max_out_upgrades(rig);
        rig.hazard_gear = crate::protection::HazardGear::MAXED;
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity;
        rig.credits = 50_000;
    }
    world
}

/// The first home of `species` and its towns.
fn first_towns(species: AlienSpecies) -> (SpaceBodyKey, Arc<Vec<Arc<AlienTown>>>) {
    let id = first_home(SEED, species).expect("a first home");
    let params =
        crate::place::planet_params_for(crate::space::SpaceSeed::from(SEED), id).expect("landable");
    (id, towns_of(&params))
}

fn place(id: SpaceBodyKey) -> Place {
    Place::OnBody(BodyId::Space(id))
}

/// Puts the rig on `id`'s strip at `column`, `rows_up` rows over row `row`.
fn put_rig(world: &mut WorldState, id: SpaceBodyKey, column: f64, row: i32, rows_up: f64) {
    world
        .move_to_place(
            PLAYER,
            place(id),
            strip_position(column + 0.5, f64::from(row) - rows_up),
        )
        .expect("on the body");
    // The rig came by test warp, not by flight: fill its air (life support is flight's).
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.flight.air = 1.0e7;
    }
}

fn step(world: &mut WorldState, command: Option<PlayerCommand>) -> Vec<GameplayEventKind> {
    let mut inputs = TickInputs::held(PLAYER, crate::ControlState::default());
    if let Some(command) = command {
        inputs = inputs.act(PLAYER, command);
    }
    let tick = world.simulation_tick();
    world
        .simulate_tick(tick, &inputs)
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(step(world, None));
    }
    events
}

fn alien_events(events: &[GameplayEventKind]) -> Vec<(AlienEvent, AlienSpecies, u32)> {
    events
        .iter()
        .filter_map(|event| match *event {
            GameplayEventKind::Alien {
                what,
                species,
                detail,
                ..
            } => Some((what, species, detail)),
            _ => None,
        })
        .collect()
}

fn saw(events: &[GameplayEventKind], what: AlienEvent) -> bool {
    alien_events(events)
        .iter()
        .any(|(seen, _, _)| *seen == what)
}

/// The NPC entity of `character` in the player's store.
fn npc_of(world: &WorldState, character: CharacterId) -> Option<EntityId> {
    world
        .store_entities(world.store_of(PLAYER))?
        .of_kind(EntityKind::Npc)
        .find(|entity| {
            matches!(entity.state, EntityState::Npc(state) if state.character == character)
        })
        .map(|entity| entity.id)
}

/// Brings the rig beside resident `slot` of `found` (waiting for them to step out if they
/// are indoors) and returns their entity.
fn beside(world: &mut WorldState, id: SpaceBodyKey, found: &AlienTown, slot: usize) -> EntityId {
    let character = found.population.characters[slot].id;
    // Back in town first (the residents only come out round a visitor).
    if world
        .alien_town_at(PLAYER)
        .is_none_or(|here| here.id != found.id)
    {
        put_rig(
            world,
            id,
            f64::from(found.site.centre),
            found.ground_row(),
            2.0,
        );
        run(world, 40);
    }
    for _ in 0..4_000 {
        if let Some(npc) = npc_of(world, character)
            && let Some((entity, state)) = world.npc(npc)
            && state.inside.is_none()
        {
            let frame = world.store_frame(world.store_of(PLAYER)).expect("frame");
            let x = state.x_at(world.simulation_tick(), 0.0);
            let (u, _) = strip_point(frame.world(x, entity.body.y));
            put_rig(world, id, u.floor(), found.ground_row(), 1.0);
            run(world, 3);
            if world.npcs_in_reach(PLAYER).contains(&npc) {
                return npc;
            }
        }
        run(world, 30);
    }
    let state = npc_of(world, character)
        .and_then(|npc| world.npc(npc).map(|(entity, state)| (entity.body, state)));
    let rig = world.player(PLAYER).map(|rig| (rig.x, rig.y));
    panic!(
        "{} never came out: {state:?}, rig {rig:?} in {:?}",
        found.population.characters[slot].name.full(),
        world.place_of(PLAYER)
    );
}

fn act(world: &mut WorldState, command: AlienCommand) -> Vec<GameplayEventKind> {
    let mut events = step(world, Some(PlayerCommand::Alien(command)));
    events.extend(run(world, 1));
    events
}

/// Moves the clock to the next day's late morning (everyone up and about).
fn next_day(world: &mut WorldState) {
    let hz = u64::from(world.timing().rate.hz());
    let day_ticks = u64::from(crate::people::clock::DEFAULT_DAY_LENGTH_MINUTES) * 60 * hz;
    let now = world.simulation_tick().get();
    let next = (now / day_ticks + 1) * day_ticks + day_ticks / 8;
    world.set_simulation_tick(crate::ids::SimulationTick::new(next));
    run(world, 2);
}

/// Talks to every resident of `found` once (the day's words) and takes a lesson.
fn study_a_day(world: &mut WorldState, id: SpaceBodyKey, found: &AlienTown) {
    for slot in 0..found.population.characters.len() {
        let npc = beside(world, id, found, slot);
        step(
            world,
            Some(PlayerCommand::People(PeopleCommand::Talk {
                npc,
                topic: TalkTopic::Chat,
            })),
        );
        step(world, Some(PlayerCommand::People(PeopleCommand::Goodbye)));
    }
    let teacher = beside(world, id, found, town::TEACHER);
    act(world, AlienCommand::Lesson { npc: teacher });
}

fn understanding(world: &WorldState, species: AlienSpecies) -> Understanding {
    world.understanding(PLAYER, species)
}

/// Learns until the tongue reaches `target`.
fn learn_to(world: &mut WorldState, id: SpaceBodyKey, found: &AlienTown, target: Understanding) {
    let species = found.species;
    let mut days = 0;
    while understanding(world, species) < target {
        study_a_day(world, id, found);
        next_day(world);
        days += 1;
        assert!(
            days < 30,
            "{species:?} stuck at {}",
            understanding(world, species)
        );
    }
}

fn give(world: &mut WorldState, mineral: MineralKind, count: u32) {
    if let Some(rig) = world.player_mut(PLAYER) {
        rig.cargo_bay.add(mineral, count);
    }
}

/// Carves an open pocket round strip tile `(column, row)` of the player's body.
fn carve(world: &mut WorldState, id: SpaceBodyKey, column: i32, row: i32) {
    world
        .with_store_ground(StoreKey::Body(id), |ground| {
            for dy in -3..=0 {
                for dx in -2..=2 {
                    ground.terrain_mut().set_kind(
                        TilePosition {
                            x: column + dx,
                            y: row + dy,
                        },
                        TileKind::Air,
                    );
                }
            }
        })
        .expect("the body's ground");
}

/// Plays the elder's whole arc for `species` on its first home.
#[allow(clippy::too_many_lines, reason = "one arc, chapter by chapter")]
fn play_arc(world: &mut WorldState, species: AlienSpecies) {
    let (id, towns) = first_towns(species);
    let found = Arc::clone(&towns[0]);
    // Arriving in town is first contact.
    put_rig(
        world,
        id,
        f64::from(found.site.centre),
        found.ground_row(),
        2.0,
    );
    let events = run(world, 4);
    assert!(
        saw(&events, AlienEvent::FirstContact),
        "{species:?}: first contact"
    );
    assert!(world.aliens().log(PLAYER, species).met());
    assert!(understanding(world, species) > 0);
    // Residents are about, as NPC entities of the body's store, in the town.
    run(world, 40);
    let about = world
        .store_entities(StoreKey::Body(id))
        .expect("the body's entities")
        .count_of(EntityKind::Npc);
    assert!(about >= 4, "{species:?}: {about} residents about");
    let leaked = world
        .store_entities(StoreKey::Home)
        .expect("home")
        .of_kind(EntityKind::Npc)
        .filter(|entity| {
            matches!(entity.state, EntityState::Npc(state) if town::is_alien_town(state.character.village))
        })
        .count();
    assert_eq!(leaked, 0, "no alien resident leaks into the home world");
    let arc = chapters(species);
    for (index, chapter) in arc.iter().enumerate() {
        learn_to(world, id, &found, chapter.needs);
        let elder = beside(world, id, &found, town::ELDER);
        let events = act(world, AlienCommand::AskChapter { npc: elder });
        assert!(
            saw(&events, AlienEvent::ChapterOffered),
            "{species:?} chapter {index}: {:?}",
            alien_events(&events)
        );
        let events = act(world, AlienCommand::AcceptChapter { npc: elder });
        assert!(saw(&events, AlienEvent::ChapterAccepted));
        match chapter.goal {
            ChapterGoal::Deliver { mineral, count } => {
                let refused = act(world, AlienCommand::TurnInChapter { npc: elder });
                assert!(
                    saw(&refused, AlienEvent::Refused),
                    "nothing to hand over yet"
                );
                give(world, mineral, count);
            }
            ChapterGoal::ReachDepth { rows } => {
                let row = found.ground_row() + rows + 1;
                carve(world, id, found.site.centre, row);
                put_rig(world, id, f64::from(found.site.centre), row, 1.0);
                let events = run(world, 3);
                assert!(
                    saw(&events, AlienEvent::ChapterReached),
                    "{species:?}: deep enough"
                );
            }
            ChapterGoal::ReachFarSide => {
                let width = found.strip_width();
                let far = (found.site.centre + width / 2).rem_euclid(width);
                let ground = crate::planet::PlanetGround::new(found.params);
                let surface = crate::planet::BodyGround::surface_row(&ground, far);
                put_rig(world, id, f64::from(far), surface, 2.0);
                let events = run(world, 30);
                assert!(
                    saw(&events, AlienEvent::ChapterReached),
                    "{species:?}: the far side"
                );
            }
            ChapterGoal::Escort => {
                assert!(saw(&events, AlienEvent::PassengerAboard));
                let active = world
                    .aliens()
                    .log(PLAYER, species)
                    .active
                    .expect("under way");
                // The traveller is aboard: no longer about in town.
                let traveller = active.passenger.expect("a traveller aboard");
                run(world, 20);
                assert!(
                    npc_of(world, traveller).is_none(),
                    "{species:?}: the traveller rides along, not in town"
                );
                // To the next town, or (a wild world's one town) the far side of the body.
                let (column, row) = active.destination.map_or_else(
                    || {
                        let width = found.strip_width();
                        let far = (found.site.centre + width / 2).rem_euclid(width);
                        let ground = crate::planet::PlanetGround::new(found.params);
                        (far, crate::planet::BodyGround::surface_row(&ground, far))
                    },
                    |destination| {
                        let next = towns
                            .iter()
                            .find(|other| other.id == destination)
                            .expect("on the body");
                        (next.site.centre, next.ground_row())
                    },
                );
                put_rig(world, id, f64::from(column), row, 2.0);
                let events = run(world, 30);
                assert!(
                    saw(&events, AlienEvent::PassengerDelivered)
                        || saw(&events, AlienEvent::ChapterReached),
                    "{species:?}: the traveller arrives"
                );
                // Back to the elder.
                put_rig(
                    world,
                    id,
                    f64::from(found.site.centre),
                    found.ground_row(),
                    2.0,
                );
                run(world, 4);
            }
            ChapterGoal::Raise { mineral, count, .. } => give(world, mineral, count),
            ChapterGoal::Trail => {
                assert!(saw(&events, AlienEvent::ChapterCompleted));
                assert!(saw(&events, AlienEvent::TrailLead));
                continue;
            }
        }
        let elder = beside(world, id, &found, town::ELDER);
        let credits = world.player(PLAYER).expect("rig").credits;
        let events = act(world, AlienCommand::TurnInChapter { npc: elder });
        assert!(
            saw(&events, AlienEvent::ChapterCompleted),
            "{species:?} chapter {index}: {:?}",
            alien_events(&events)
        );
        if let ChapterGoal::Raise { credits: cost, .. } = chapter.goal {
            assert!(saw(&events, AlienEvent::MonumentRaised));
            assert!(world.aliens().monuments.contains(&found.id));
            assert_eq!(
                world.player(PLAYER).expect("rig").credits,
                credits - cost + chapter.credits
            );
        } else {
            assert_eq!(
                world.player(PLAYER).expect("rig").credits,
                credits + chapter.credits
            );
        }
    }
    let log = world.aliens().log(PLAYER, species);
    assert_eq!(log.chapters_done, 5);
    assert!(log.active.is_none());
    let lead = world
        .aliens()
        .trail_leads
        .iter()
        .find(|lead| lead.species == species)
        .expect("the builders' trail lead");
    assert_eq!(
        lead.system,
        super::aliens::trail_system(crate::space::SpaceSeed::from(SEED), species)
    );
    // The lead is on the nav chart.
    let space = crate::space::Space::shared(crate::space::SpaceSeed::from(SEED));
    let system = space.system(lead.system).expect("a real system");
    let reference = crate::flight::GeneratedSpace::reference(lead.system.sector, &system);
    assert!(world.known_systems().contains(&reference));
    let elder = beside(world, id, &found, town::ELDER);
    let events = act(world, AlienCommand::AskChapter { npc: elder });
    assert!(saw(&events, AlienEvent::Refused), "the arc is told");
}

#[test]
fn a_bot_plays_every_species_arc_to_the_builders_trail() {
    let mut world = world_at(30);
    for species in AlienSpecies::ALL {
        play_arc(&mut world, species);
        assert!(understanding(&world, species) >= 600);
    }
    assert_eq!(world.aliens().trail_leads.len(), 3);
    assert_eq!(world.aliens().monuments.len(), 3);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one walk through every way of learning"
)]
fn talking_trading_lessons_and_lexicon_stones_teach_the_tongue() {
    let mut world = world_at(30);
    let species = AlienSpecies::Tokkari;
    let (id, towns) = first_towns(species);
    let found = Arc::clone(&towns[0]);
    put_rig(
        &mut world,
        id,
        f64::from(found.site.centre),
        found.ground_row(),
        2.0,
    );
    run(&mut world, 40);
    let start = understanding(&world, species);
    // A chat teaches once a day per resident.
    let npc = beside(&mut world, id, &found, 4);
    let talk = PlayerCommand::People(PeopleCommand::Talk {
        npc,
        topic: TalkTopic::Chat,
    });
    let events = step(&mut world, Some(talk.clone()));
    assert!(saw(&events, AlienEvent::Learned));
    let after_chat = understanding(&world, species);
    assert!(after_chat > start);
    let events = step(&mut world, Some(talk));
    assert!(
        !saw(&events, AlienEvent::Learned),
        "the same resident twice a day teaches once"
    );
    // Their talk is their own (the greeting in their tongue first).
    let conversation = world.people().conversation(PLAYER).expect("a conversation");
    assert!(
        conversation
            .lines
            .iter()
            .any(|line| line.text.starts_with(species.def().greeting.0))
    );
    // A lesson a day.
    let teacher = beside(&mut world, id, &found, town::TEACHER);
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = act(&mut world, AlienCommand::Lesson { npc: teacher });
    assert!(saw(&events, AlienEvent::Lesson));
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits - crate::alien::language::LESSON_PRICE
    );
    let events = act(&mut world, AlienCommand::Lesson { npc: teacher });
    assert!(saw(&events, AlienEvent::Refused), "one lesson a day");
    // Only the lexicon keeper teaches.
    let trader = beside(&mut world, id, &found, town::TRADER);
    let events = act(&mut world, AlienCommand::Lesson { npc: trader });
    assert!(saw(&events, AlienEvent::Refused));
    // Trade: they buy what they prize, better from a good speaker; a primer teaches.
    give(&mut world, MineralKind::Iron, 10);
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = act(
        &mut world,
        AlienCommand::Sell {
            npc: trader,
            mineral: MineralKind::Iron,
            count: 10,
        },
    );
    assert!(saw(&events, AlienEvent::Sold));
    let paid = world.player(PLAYER).expect("rig").credits - credits;
    let market = world.canonical_mineral_market_value(MineralKind::Iron) * 10;
    assert!(
        paid > market,
        "the Tokkari prize iron: {paid} over {market}"
    );
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .cargo_bay
            .count(MineralKind::Iron),
        0
    );
    give(&mut world, MineralKind::Ruby, 1);
    let events = act(
        &mut world,
        AlienCommand::Sell {
            npc: trader,
            mineral: MineralKind::Ruby,
            count: 1,
        },
    );
    assert!(
        saw(&events, AlienEvent::Refused),
        "the Tokkari have no use for ruby"
    );
    let before = understanding(&world, species);
    let events = act(
        &mut world,
        AlienCommand::Buy {
            npc: trader,
            good: AlienGood::Primer(species),
        },
    );
    assert!(saw(&events, AlienEvent::Bought));
    assert!(understanding(&world, species) >= before + crate::alien::trade::PRIMER_UNDERSTANDING);
    let events = act(
        &mut world,
        AlienCommand::Buy {
            npc: trader,
            good: AlienGood::StarMemory,
        },
    );
    assert!(
        saw(&events, AlienEvent::Refused),
        "Siphra goods are not sold here"
    );
    let hull = world.player(PLAYER).expect("rig").max_hull();
    world.player_mut(PLAYER).expect("rig").hull = hull * 0.3;
    act(
        &mut world,
        AlienCommand::Buy {
            npc: trader,
            good: AlienGood::HullPlates,
        },
    );
    // Patched to full (the tick after may take a hair for the world's own climate).
    assert!(world.player(PLAYER).expect("rig").hull >= hull * 0.99);
    // A lexicon stone dug out teaches the most.
    let (column, row) = found.site.stones[1];
    carve(&mut world, id, column, row - 1);
    put_rig(&mut world, id, f64::from(column), row - 1, 0.0);
    run(&mut world, 30);
    let before = understanding(&world, species);
    let mut events = Vec::new();
    for _ in 0..600 {
        let mut controls = crate::ControlState::default();
        controls.drill_down = true;
        let tick = world.simulation_tick();
        events.extend(
            world
                .simulate_tick(tick, &TickInputs::held(PLAYER, controls))
                .events
                .into_iter()
                .map(|event| event.kind),
        );
        if saw(&events, AlienEvent::LexiconFound) {
            break;
        }
    }
    let (x, y) = {
        let rig = world.player(PLAYER).expect("rig");
        (rig.x, rig.y)
    };
    let frame = world.store_frame(StoreKey::Body(id)).expect("frame");
    let at = world
        .store_terrain(StoreKey::Body(id))
        .and_then(|terrain| terrain.tile(TilePosition { x: column, y: row }));
    assert!(
        saw(&events, AlienEvent::LexiconFound),
        "the stone dug out: rig at {:?}, stone ({column}, {row}) now {at:?}",
        strip_point(frame.world(x, y))
    );
    assert!(
        understanding(&world, species)
            >= before + crate::alien::language::LEXICON_STONE_UNDERSTANDING
    );
    assert!(Fluency::of(understanding(&world, species)) > Fluency::Glyphs);
    // Stones and paving are this track's tiles.
    assert_eq!(
        AlienTile::from_code(AlienTile::LexiconStone(species).code()),
        Some(AlienTile::LexiconStone(species))
    );
}

#[test]
#[allow(clippy::too_many_lines, reason = "a home from plot to beacon")]
fn homes_in_alien_towns_are_bought_built_in_world_time_rested_in_and_tow_the_beacon() {
    let mut world = world_at(30);
    let species = AlienSpecies::Vessi;
    let (id, towns) = first_towns(species);
    let found = Arc::clone(&towns[0]);
    put_rig(
        &mut world,
        id,
        f64::from(found.site.centre),
        found.ground_row(),
        2.0,
    );
    run(&mut world, 40);
    let plot = found
        .buildings
        .iter()
        .find(|building| building.kind == AlienBuildingKind::HomePlot)
        .copied()
        .expect("a plot for sale");
    let builder = beside(&mut world, id, &found, town::BUILDER);
    // Only the builder sells plots, and only plots.
    let trader = beside(&mut world, id, &found, town::TRADER);
    let events = act(
        &mut world,
        AlienCommand::BuyHome {
            npc: trader,
            plot: plot.index,
        },
    );
    assert!(saw(&events, AlienEvent::Refused));
    let builder = {
        let _ = builder;
        beside(&mut world, id, &found, town::BUILDER)
    };
    let not_a_plot = found
        .building(AlienBuildingKind::TradeHall)
        .expect("a hall")
        .index;
    let events = act(
        &mut world,
        AlienCommand::BuyHome {
            npc: builder,
            plot: not_a_plot,
        },
    );
    assert!(saw(&events, AlienEvent::Refused));
    let credits = world.player(PLAYER).expect("rig").credits;
    let events = act(
        &mut world,
        AlienCommand::BuyHome {
            npc: builder,
            plot: plot.index,
        },
    );
    assert!(saw(&events, AlienEvent::HomeBought));
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits - super::aliens::home_price(species)
    );
    let events = act(
        &mut world,
        AlienCommand::BuyHome {
            npc: builder,
            plot: plot.index,
        },
    );
    assert!(saw(&events, AlienEvent::Refused), "sold");
    // Still building: no rest yet.
    put_rig(
        &mut world,
        id,
        f64::from(plot.door_column()),
        found.ground_row(),
        1.0,
    );
    run(&mut world, 3);
    let rest = AlienCommand::RestAtHome {
        town: found.id,
        plot: plot.index,
    };
    let events = act(&mut world, rest);
    assert!(saw(&events, AlienEvent::Refused));
    // A quarter of a day later the builder hands it over.
    let hz = u64::from(world.timing().rate.hz());
    let quarter = u64::from(crate::people::clock::DEFAULT_DAY_LENGTH_MINUTES) * 60 * hz / 4;
    let now = world.simulation_tick().get();
    world.set_simulation_tick(crate::ids::SimulationTick::new(now + quarter + 1));
    let events = run(&mut world, 2);
    assert!(saw(&events, AlienEvent::HomeBuilt));
    let home = world
        .aliens()
        .home_at(found.id, plot.index)
        .expect("a home");
    assert_eq!(home.owner, PLAYER);
    assert!(
        home.home
            .as_ref()
            .is_some_and(|home| !home.rooms.is_empty())
    );
    // Resting tops up the air.
    world.player_mut(PLAYER).expect("rig").flight.air = 5.0;
    let events = act(&mut world, rest);
    assert!(saw(&events, AlienEvent::Rested));
    assert!(world.player(PLAYER).expect("rig").flight.air > 100.0);
    // Out of air far round the world, the beacon tows the rig to its home here.
    let width = found.strip_width();
    let far = (found.site.centre + width / 2).rem_euclid(width);
    let ground = crate::planet::PlanetGround::new(found.params);
    let surface = crate::planet::BodyGround::surface_row(&ground, far);
    put_rig(&mut world, id, f64::from(far), surface, 2.0);
    run(&mut world, 5);
    world.player_mut(PLAYER).expect("rig").flight.air = 0.0;
    run(&mut world, 2);
    assert_eq!(
        world.store_of(PLAYER),
        StoreKey::Body(id),
        "still on the body"
    );
    let frame = world.store_frame(StoreKey::Body(id)).expect("frame");
    let rig = world.player(PLAYER).expect("rig");
    let (u, _) = strip_point(frame.world(rig.x, rig.y));
    #[allow(clippy::cast_possible_truncation, reason = "a strip column")]
    let column = u.floor() as i64;
    assert!(
        town::wrapped_distance(column, plot.door_column(), width) <= 4,
        "towed home to column {u}"
    );
    assert!(rig.flight.air > 0.0);
}

#[test]
fn alien_state_saves_replicates_and_plays_the_same_at_any_tick_rate() {
    let play = |hz: u32| {
        let mut world = world_at(hz);
        let species = AlienSpecies::Siphra;
        let (id, towns) = first_towns(species);
        let found = Arc::clone(&towns[0]);
        put_rig(
            &mut world,
            id,
            f64::from(found.site.centre),
            found.ground_row(),
            2.0,
        );
        run(&mut world, hz * 2);
        let elder = beside(&mut world, id, &found, town::ELDER);
        act(&mut world, AlienCommand::AskChapter { npc: elder });
        act(&mut world, AlienCommand::AcceptChapter { npc: elder });
        give(&mut world, MineralKind::Emerald, 3);
        act(&mut world, AlienCommand::TurnInChapter { npc: elder });
        world
    };
    let a = play(30);
    let b = play(30);
    assert_eq!(a.simulation_state_hash(), b.simulation_state_hash());
    assert_eq!(a.aliens(), b.aliens());
    let c = play(60);
    assert_eq!(
        a.aliens().log(PLAYER, AlienSpecies::Siphra).chapters_done,
        c.aliens().log(PLAYER, AlienSpecies::Siphra).chapters_done
    );
    assert_eq!(
        a.aliens().log(PLAYER, AlienSpecies::Siphra).understanding,
        c.aliens().log(PLAYER, AlienSpecies::Siphra).understanding
    );
    // The section is shared and replicates.
    assert!(SectionKey::SHARED.contains(&SectionKey::Aliens));
    let mut replica = world_at(30);
    let payload = a.section(SectionKey::Aliens).expect("the section");
    replica.apply_section(SectionKey::Aliens, payload);
    assert_eq!(replica.aliens(), a.aliens());
    assert_eq!(
        a.section_hash(SectionKey::Aliens),
        replica.section_hash(SectionKey::Aliens)
    );
    // Saves keep it (JSON), and a reloaded world plays on identically.
    let json = serde_json::to_string(&a).expect("saves");
    let mut loaded: WorldState = serde_json::from_str(&json).expect("loads");
    loaded.register_alien_towns();
    assert_eq!(loaded.aliens(), a.aliens());
    assert_eq!(loaded.simulation_state_hash(), a.simulation_state_hash());
    let mut original = a;
    run(&mut original, 60);
    run(&mut loaded, 60);
    assert_eq!(
        loaded.simulation_state_hash(),
        original.simulation_state_hash()
    );
    // A world nobody took to the aliens hashes as before (the section is skipped while empty).
    let fresh = world_at(30);
    assert!(fresh.aliens().is_empty());
}

#[test]
fn speech_reads_through_each_players_own_understanding() {
    let mut world = world_at(30);
    let two = PlayerId::new(2);
    world.add_player(two);
    let species = AlienSpecies::Vessi;
    let (id, towns) = first_towns(species);
    let found = Arc::clone(&towns[0]);
    put_rig(
        &mut world,
        id,
        f64::from(found.site.centre),
        found.ground_row(),
        2.0,
    );
    run(&mut world, 40);
    learn_to(&mut world, id, &found, 420);
    let line = chapters(species)[0].ask;
    let mine =
        crate::alien::language::readable_permille(line, species, understanding(&world, species));
    let theirs =
        crate::alien::language::readable_permille(line, species, world.understanding(two, species));
    assert!(
        mine > theirs,
        "the player who studied reads more ({mine} > {theirs})"
    );
    assert_eq!(theirs, 0, "a stranger reads only glyphs");
    let _ = WorldPos::default();
    let _ = TILE_SIZE;
}
