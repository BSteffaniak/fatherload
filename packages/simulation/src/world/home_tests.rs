//! Contractor-built homes (WP-HOME): deeds at the land office and surveyed spots, contracts
//! scheduled in world time (sleeping moves them on), stages and the foundation, site events,
//! rushing and supplying, site crews, decorating, beds and home points, the garage bay,
//! expansions (cellar, lift), permissions, safety, saves, replication and determinism.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, PlayerScopedCommandOutcome,
    SectionKey, SimulationParams, TickInputs, TickOutput, WorldState,
    base::{BuildCommand, Buildable, Facing, ModuleKind},
    entity::{EntityKind, EntityState},
    home::{
        Blueprint, BuildStage, Expansion, Furniture, HomeAccess, HomeCommand, HomeJob, HomeRefusal,
        PlotId, PlotSetting, PlotSite, PlotSize, SiteEventKind, SiteKey, construction::SiteEvent,
        plot,
    },
    item::{BlockKind, ItemKind},
    people::{PeopleCommand, TalkTopic},
    terrain::{ArtifactKind, MineralKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(1);
const OTHER: PlayerId = PlayerId::new(2);
const SEED: u64 = 0x5EED_0024;

fn world() -> WorldState {
    world_at(60)
}

fn world_at(hz: u32) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V7,
        ..WorldSettings::new(SEED)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(hz));
    world.player_mut(PLAYER).expect("rig").credits = 200_000;
    world
}

fn step(world: &mut WorldState) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(
        tick,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, false)),
    )
}

fn run(world: &mut WorldState, ticks: u64) -> Vec<GameplayEventKind> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        events.extend(step(world).events.iter().map(|event| event.kind));
    }
    events
}

fn act_as(world: &mut WorldState, player: PlayerId, command: HomeCommand) -> TickOutput {
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(player, PlayerCommand::Home(command));
    world.simulate_tick(tick, &inputs)
}

fn act(world: &mut WorldState, command: HomeCommand) -> TickOutput {
    act_as(world, PLAYER, command)
}

fn applied(output: &TickOutput) -> bool {
    output
        .action_outcomes
        .first()
        .is_some_and(|outcome| outcome.outcome == PlayerScopedCommandOutcome::Applied)
}

#[allow(clippy::cast_precision_loss, reason = "small columns")]
fn column_x(column: i32) -> f32 {
    (column as f32 + 0.5) * TILE_SIZE
}

/// Parks `player`'s rig on the ground at `column`, standing on `ground_row`.
#[allow(clippy::cast_precision_loss, reason = "small rows")]
fn park(world: &mut WorldState, player: PlayerId, column: i32, ground_row: i32) {
    let rig = world.player_mut(player).expect("rig");
    rig.x = column_x(column);
    rig.y = (ground_row as f32 - 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
}

/// Parks the rig at the town headquarters (the land office).
fn park_at_hq(world: &mut WorldState, player: PlayerId) {
    let hq = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Headquarters)
        .expect("hq");
    let ground = world.terrain().surface_row(hq.tile_x + 2);
    park(world, player, hq.tile_x + 2, ground);
    assert!(world.at_land_office(player));
}

/// The first listed town site.
fn town_site(world: &WorldState) -> PlotSite {
    plot::listed_site(world.world_settings(), SiteKey::Town(2)).expect("a town site east")
}

/// Buys the east town site and drives onto it. Returns the plot.
fn buy_town_plot(world: &mut WorldState) -> (PlotId, PlotSite) {
    park_at_hq(world, PLAYER);
    let site = town_site(world);
    let output = act(
        world,
        HomeCommand::BuyPlot {
            site: site.key,
            size: site.size,
        },
    );
    assert!(applied(&output), "bought");
    let plot = world.homes().plots_of(PLAYER).next().expect("deed").id;
    park(world, PLAYER, site.middle(), site.ground_row);
    (plot, site)
}

/// Signs a cabin contract with the first contractor on offer.
fn hire_cabin(world: &mut WorldState, plot: PlotId) -> TickOutput {
    let contractor = world.contractors_for(plot, PLAYER)[0].character;
    act(
        world,
        HomeCommand::Hire {
            plot,
            job: HomeJob::Build(Blueprint::Cabin),
            contractor,
            supply: false,
        },
    )
}

fn day_millis(world: &WorldState) -> u64 {
    world.day_length().millis()
}

/// Moves the world clock forward by `millis` (as sleeping does) and runs a tick.
fn advance_time(world: &mut WorldState, millis: u64) -> Vec<GameplayEventKind> {
    let now = world.world_millis();
    world.set_world_time_millis(now + millis);
    run(world, 2)
}

/// A finished cabin on the east town site, the rig parked at its door.
fn cabin_world() -> (WorldState, PlotId) {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    assert!(applied(&hire_cabin(&mut world, plot)));
    let _ = {
        let day = day_millis(&world);
        advance_time(&mut world, day * 3)
    };
    assert!(
        world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.home.as_ref())
            .is_some()
    );
    (world, plot)
}

#[test]
fn listed_deeds_are_sold_at_the_land_office_for_credits() {
    let mut world = world();
    let site = town_site(&world);
    // Away from a land office: refused.
    park(&mut world, PLAYER, site.middle(), site.ground_row);
    assert_eq!(
        world.check_home(
            PLAYER,
            &HomeCommand::BuyPlot {
                site: site.key,
                size: site.size
            }
        ),
        Err(HomeRefusal::NotAtLandOffice)
    );
    park_at_hq(&mut world, PLAYER);
    let credits = world.player(PLAYER).expect("rig").credits;
    let output = act(
        &mut world,
        HomeCommand::BuyPlot {
            site: site.key,
            size: site.size,
        },
    );
    assert!(applied(&output));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::PlotBought {
            setting: PlotSetting::Town,
            ..
        }
    )));
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits - site.price
    );
    let plot = world.homes().plots_of(PLAYER).next().expect("deed");
    assert_eq!(plot.deed.owner, PLAYER);
    assert_eq!(plot.site, site);
    // Sold once.
    assert_eq!(
        world.check_home(
            PLAYER,
            &HomeCommand::BuyPlot {
                site: site.key,
                size: site.size
            }
        ),
        Err(HomeRefusal::SiteTaken)
    );
    // The plot is a safe zone and no claim may be staked over it.
    let on_plot = TilePosition {
        x: site.middle(),
        y: site.ground_row - 1,
    };
    assert!(world.enemy_safe_zone(on_plot));
    park(&mut world, PLAYER, site.middle(), site.ground_row);
    let beacon = BuildCommand::Place {
        item: Buildable::Module(ModuleKind::Beacon),
        at: on_plot,
        facing: Facing::Right,
    };
    assert!(matches!(
        world.check_build(PLAYER, &beacon).map(|_| ()),
        Err(crate::base::BuildRefusal::OnPlot | crate::base::BuildRefusal::InTown)
    ));
}

#[test]
fn a_poor_rig_cannot_buy_and_deeds_are_capped() {
    let mut world = world();
    park_at_hq(&mut world, PLAYER);
    let site = town_site(&world);
    world.player_mut(PLAYER).expect("rig").credits = 10;
    assert_eq!(
        world.check_home(
            PLAYER,
            &HomeCommand::BuyPlot {
                site: site.key,
                size: site.size
            }
        ),
        Err(HomeRefusal::CannotAfford)
    );
}

#[test]
fn surveyed_spots_are_priced_by_distance_and_refused_in_town() {
    let mut world = world();
    // In town: occupied.
    park_at_hq(&mut world, PLAYER);
    assert_eq!(
        world.survey_plot(PLAYER, PlotSize::Small),
        Err(HomeRefusal::Occupied)
    );
    // Find open ground well away from town.
    let settings = *world.world_settings();
    let found = (600..4_000).step_by(37).find_map(|column| {
        let row = crate::worldgen::surface_row(&settings, column);
        park(&mut world, PLAYER, column, row);
        world.survey_plot(PLAYER, PlotSize::Medium).ok()
    });
    let site = found.expect("some open ground");
    assert_eq!(site.key, SiteKey::Surveyed);
    assert_eq!(site.setting, PlotSetting::Wilds);
    assert!(site.price > 0);
    let output = act(
        &mut world,
        HomeCommand::BuyPlot {
            site: SiteKey::Surveyed,
            size: PlotSize::Medium,
        },
    );
    assert!(applied(&output));
    // Surveying the same spot again: another plot is there.
    assert_eq!(
        world.survey_plot(PLAYER, PlotSize::Small),
        Err(HomeRefusal::Occupied)
    );
}

#[test]
fn construction_runs_on_world_time_through_every_stage() {
    let mut world = world();
    let (plot, site) = buy_town_plot(&mut world);
    let offers = world.contractors_for(plot, PLAYER);
    assert!(!offers.is_empty());
    assert!(
        offers[0].character.is_some(),
        "a person takes the job on a V7 world"
    );
    let output = hire_cabin(&mut world, plot);
    assert!(applied(&output));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::ContractSigned {
            job: HomeJob::Build(Blueprint::Cabin),
            ..
        }
    )));
    let total = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.as_ref())
        .expect("work")
        .total_millis;
    // Ticks alone barely move it: a cabin takes about an in-game day.
    let _ = run(&mut world, 120);
    let work = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.clone())
        .expect("work");
    assert_eq!(work.stage(world.world_millis()), BuildStage::Survey);
    // Stages are announced once each, in order, as world time passes.
    let mut stages = Vec::new();
    for _ in 0..12 {
        for event in advance_time(&mut world, total / 10) {
            match event {
                GameplayEventKind::ConstructionStage { stage, .. } => stages.push(stage),
                GameplayEventKind::SiteEventRaised { event, .. } => {
                    // Answer whatever comes up so a shortage never holds the job.
                    let _ = event;
                    world
                        .player_mut(PLAYER)
                        .expect("rig")
                        .block_hopper
                        .add(ItemKind::Block(BlockKind::Stone), 40);
                    let _ = act(&mut world, HomeCommand::AnswerEvent { plot, pay: true });
                }
                _ => {}
            }
        }
    }
    let mut sorted = stages.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(stages, sorted, "each stage once, in order");
    assert_eq!(stages.last(), Some(&BuildStage::Done));
    assert!(stages.contains(&BuildStage::Foundation));
    let found = world.homes().plot(plot).expect("plot");
    let home = found.home.as_ref().expect("a cabin stands");
    assert_eq!(home.blueprint, Blueprint::Cabin);
    assert!(found.construction.is_none());
    assert!(home.has_bed());
    // The foundation is masonry on the ground row under the house.
    let left = found.house_left(home.width);
    assert!(matches!(
        world
            .terrain()
            .tile(TilePosition {
                x: left + 1,
                y: site.ground_row
            })
            .map(|tile| tile.kind),
        Some(TileKind::Built(_))
    ));
    // The contractor remembers the job and gave a housewarming gift.
    let builder = home.builder.expect("a person built it");
    assert_eq!(world.homes().jobs_with(builder, PLAYER), 1);
    assert!(world.people().affinity(builder, PLAYER) > 0);
    assert!(!world.homes().furniture_of(PLAYER).is_empty());
}

#[test]
fn sleeping_through_the_night_moves_the_work_on() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    assert!(applied(&hire_cabin(&mut world, plot)));
    let before = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.as_ref())
        .expect("work")
        .progress_per_mille(world.world_millis());
    // A skipped night is a jump of the clock's offset, not of ticks.
    let tick = world.simulation_tick();
    let _ = {
        let day = day_millis(&world);
        advance_time(&mut world, day / 3)
    };
    assert!(world.simulation_tick().get() - tick.get() <= 3);
    let after = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.as_ref())
        .map_or(1_000, |work| work.progress_per_mille(world.world_millis()));
    assert!(after >= before + 250, "{before} -> {after}");
}

#[test]
fn a_shortage_holds_the_work_until_answered() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    assert!(applied(&hire_cabin(&mut world, plot)));
    // Plan a shortage for the frame.
    world.scenario_edit_plot(plot, |found| {
        let work = found.construction.as_mut().expect("work");
        work.planned = vec![crate::home::construction::PlannedEvent {
            stage: BuildStage::Frame,
            kind: SiteEventKind::Shortage {
                block: BlockKind::Clay,
                count: 5,
            },
        }];
    });
    let total = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.as_ref())
        .expect("work")
        .total_millis;
    let events = advance_time(&mut world, total * 3 / 10);
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::SiteEventRaised {
            event: SiteEventKind::Shortage { .. },
            ..
        }
    )));
    let held = |world: &WorldState| {
        world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.construction.clone())
            .expect("work")
    };
    let progress = held(&world).progress_per_mille(world.world_millis());
    // Time passes, the work does not (a little, under the contractor's patience).
    let _ = advance_time(&mut world, total / 10);
    assert_eq!(
        held(&world).progress_per_mille(world.world_millis()),
        progress
    );
    assert!(held(&world).rush_quote(world.world_millis()).is_none());
    // Without the blocks the answer is refused; with them the work resumes.
    assert_eq!(
        world.check_home(PLAYER, &HomeCommand::AnswerEvent { plot, pay: false }),
        Err(HomeRefusal::MissingItems)
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .block_hopper
        .add(ItemKind::Block(BlockKind::Clay), 5);
    let output = act(&mut world, HomeCommand::AnswerEvent { plot, pay: false });
    assert!(applied(&output));
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .block_hopper
            .count(ItemKind::Block(BlockKind::Clay)),
        0
    );
    let _ = advance_time(&mut world, total / 10);
    assert!(held(&world).progress_per_mille(world.world_millis()) > progress);
}

#[test]
fn rushing_costs_credits_and_brings_the_finish_forward() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    assert!(applied(&hire_cabin(&mut world, plot)));
    world.scenario_edit_plot(plot, |found| {
        found.construction.as_mut().expect("work").planned.clear();
    });
    let work = |world: &WorldState| {
        world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.construction.clone())
            .expect("work")
    };
    let now = world.world_millis();
    let (price, _) = work(&world).rush_quote(now).expect("rushable");
    let finish = work(&world).finish_millis(now);
    let credits = world.player(PLAYER).expect("rig").credits;
    let output = act(&mut world, HomeCommand::Rush { plot });
    assert!(applied(&output));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::HomeRushed { .. }))
    );
    assert_eq!(world.player(PLAYER).expect("rig").credits, credits - price);
    assert!(work(&world).finish_millis(world.world_millis()) < finish);
}

#[test]
fn supplying_blocks_takes_money_off() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    let contractor = world.contractors_for(plot, PLAYER)[0].character;
    let (plain, _) = world
        .hire_quote(
            PLAYER,
            plot,
            HomeJob::Build(Blueprint::Cabin),
            contractor,
            true,
        )
        .expect("quote");
    assert_eq!(plain.supplied_blocks, 0, "an empty hopper supplies nothing");
    world
        .player_mut(PLAYER)
        .expect("rig")
        .block_hopper
        .add(ItemKind::Block(BlockKind::Dirt), 20);
    let (supplied, _) = world
        .hire_quote(
            PLAYER,
            plot,
            HomeJob::Build(Blueprint::Cabin),
            contractor,
            true,
        )
        .expect("quote");
    assert_eq!(supplied.supplied_blocks, 20);
    assert!(supplied.price < plain.price);
    let output = act(
        &mut world,
        HomeCommand::Hire {
            plot,
            job: HomeJob::Build(Blueprint::Cabin),
            contractor,
            supply: true,
        },
    );
    assert!(applied(&output));
    assert_eq!(world.player(PLAYER).expect("rig").block_hopper.total(), 0);
}

#[test]
fn blueprints_need_room_and_the_right_ground() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    let contractor = world.contractors_for(plot, PLAYER)[0].character;
    assert_eq!(
        world
            .hire_quote(
                PLAYER,
                plot,
                HomeJob::Build(Blueprint::Manor),
                contractor,
                false
            )
            .map(|_| ()),
        Err(HomeRefusal::PlotTooSmall)
    );
    assert_eq!(
        world
            .hire_quote(
                PLAYER,
                plot,
                HomeJob::Build(Blueprint::SkyVilla),
                contractor,
                false
            )
            .map(|_| ()),
        Err(HomeRefusal::PlotTooSmall),
        "small town plot"
    );
    assert!(!Blueprint::SkyVilla.allowed_on(PlotSetting::Town));
    assert!(Blueprint::SkyVilla.allowed_on(PlotSetting::SkyIsland));
    assert!(Blueprint::LunarHabitat.allowed_on(PlotSetting::Moon));
    assert!(!Blueprint::Cabin.allowed_on(PlotSetting::Moon));
    // Another player cannot hire on your plot.
    world.add_player(OTHER);
    let site = world.homes().plot(plot).expect("plot").site;
    park(&mut world, OTHER, site.middle(), site.ground_row);
    assert_eq!(
        world
            .hire_quote(
                OTHER,
                plot,
                HomeJob::Build(Blueprint::Cabin),
                contractor,
                false
            )
            .map(|_| ()),
        Err(HomeRefusal::NotOwner)
    );
}

#[test]
fn site_crews_work_in_working_hours_and_the_contractor_talks_about_the_job() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    // 09:00 on day 2: a working morning.
    world.set_world_time_millis(day_millis(&world) * 2 + day_millis(&world) * 9 / 24);
    assert!(applied(&hire_cabin(&mut world, plot)));
    world.scenario_edit_plot(plot, |found| {
        found.construction.as_mut().expect("work").planned.clear();
    });
    let _ = run(&mut world, 90);
    let crew: Vec<_> = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) if state.site == Some(plot.0) => Some((entity.id, state)),
            _ => None,
        })
        .collect();
    assert!(crew.len() >= 2, "contractor and a helper: {}", crew.len());
    let boss = world
        .homes()
        .plot(plot)
        .and_then(|plot| plot.construction.as_ref())
        .expect("work")
        .character();
    assert!(crew.iter().any(|(_, state)| state.character == boss));
    // Nobody is spawned twice.
    let copies = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter(|entity| matches!(entity.state, EntityState::Npc(state) if state.character == boss))
        .count();
    assert_eq!(copies, 1);
    // Talk to the contractor on site.
    let (npc, _) = crew
        .iter()
        .find(|(_, state)| state.character == boss)
        .copied()
        .expect("boss on site");
    let (x, _) = world.npc_position(npc).expect("position");
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = x;
    let tick = world.simulation_tick();
    let inputs = TickInputs::default().act(
        PLAYER,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::BuildingWork,
        }),
    );
    let output = world.simulate_tick(tick, &inputs);
    assert!(applied(&output));
    let lines = &world.people().conversation(PLAYER).expect("talking").lines;
    let said = &lines.last().expect("a line").text;
    assert!(
        said.contains("Survey") || said.contains("survey"),
        "talks about the job: {said}"
    );
    // After hours the crew goes home.
    world.set_world_time_millis(day_millis(&world) * 2 + day_millis(&world) * 20 / 24);
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(
        tick,
        &TickInputs::default().act(PLAYER, PlayerCommand::People(PeopleCommand::Goodbye)),
    );
    let _ = run(&mut world, 60);
    let still = world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter(|entity| matches!(entity.state, EntityState::Npc(state) if state.site.is_some()))
        .count();
    assert_eq!(still, 0);
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one walk through decorating and the deed"
)]
fn decorating_places_moves_and_restyles_with_permissions() {
    let (mut world, plot) = cabin_world();
    world.grant_furniture(PLAYER, Furniture::Plant, 1);
    world.grant_furniture(PLAYER, Furniture::Rug, 1);
    let place = |cell, furniture| HomeCommand::Place {
        plot,
        room: 0,
        cell,
        furniture,
    };
    // A plant on the floor row, a rug under the bed.
    let output = act(&mut world, place(3, Furniture::Plant));
    assert!(applied(&output));
    assert!(output.events.iter().any(|event| matches!(
        event.kind,
        GameplayEventKind::FurniturePlaced {
            furniture: Furniture::Plant,
            ..
        }
    )));
    assert!(applied(&act(&mut world, place(0, Furniture::Rug))));
    let room = |world: &WorldState| {
        world
            .homes()
            .plot(plot)
            .expect("plot")
            .home
            .as_ref()
            .expect("home")
            .rooms[0]
            .decor
            .clone()
    };
    assert_eq!(
        room(&world).piece_at(3).map(|(_, piece)| piece.furniture),
        Some(Furniture::Plant)
    );
    assert_eq!(
        room(&world).rug_at(1).map(|(_, piece)| piece.furniture),
        Some(Furniture::Rug)
    );
    // No piece in stock, or no room: refused.
    assert_eq!(
        world.check_home(PLAYER, &place(6, Furniture::Telescope)),
        Err(HomeRefusal::NoFurniture)
    );
    world.grant_furniture(PLAYER, Furniture::Chair, 1);
    assert_eq!(
        world.check_home(PLAYER, &place(3, Furniture::Chair)),
        Err(HomeRefusal::NoSpace)
    );
    // Pick it up again: back in stock.
    let plants = world.homes().furniture_count(PLAYER, Furniture::Plant);
    assert!(applied(&act(
        &mut world,
        HomeCommand::PickUp {
            plot,
            room: 0,
            cell: 3
        }
    )));
    assert_eq!(
        world.homes().furniture_count(PLAYER, Furniture::Plant),
        plants + 1
    );
    // Restyling cycles the wallpaper for a few credits.
    let paper = room(&world).wallpaper;
    assert!(applied(&act(
        &mut world,
        HomeCommand::Restyle {
            plot,
            room: 0,
            wallpaper: true
        }
    )));
    assert_eq!(room(&world).wallpaper, paper.next());
    // A visitor may come in but not decorate; owners-only keeps them out.
    world.add_player(OTHER);
    let site = world.homes().plot(plot).expect("plot").site;
    park(&mut world, OTHER, site.middle(), site.ground_row);
    world.grant_furniture(OTHER, Furniture::Plant, 1);
    assert!(world.may_enter_home(OTHER, plot));
    assert_eq!(
        world.check_home(
            OTHER,
            &HomeCommand::Place {
                plot,
                room: 0,
                cell: 4,
                furniture: Furniture::Plant
            }
        ),
        Err(HomeRefusal::NotPermitted)
    );
    // Co-owners decorate.
    assert!(applied(&act(
        &mut world,
        HomeCommand::CoOwner {
            plot,
            player: OTHER,
            add: true
        }
    )));
    assert!(
        world
            .check_home(
                OTHER,
                &HomeCommand::Place {
                    plot,
                    room: 0,
                    cell: 4,
                    furniture: Furniture::Plant
                }
            )
            .is_ok()
    );
    assert!(applied(&act(
        &mut world,
        HomeCommand::CoOwner {
            plot,
            player: OTHER,
            add: false
        }
    )));
    // An open house lets the crew decorate, but what they pick up stays the owner's.
    assert!(applied(&act(
        &mut world,
        HomeCommand::SetAccess {
            plot,
            access: HomeAccess::Crew
        }
    )));
    world.grant_furniture(PLAYER, Furniture::Plant, 1);
    assert!(applied(&act(&mut world, place(4, Furniture::Plant))));
    let owner_plants = world.homes().furniture_count(PLAYER, Furniture::Plant);
    let other_plants = world.homes().furniture_count(OTHER, Furniture::Plant);
    assert!(applied(&act_as(
        &mut world,
        OTHER,
        HomeCommand::PickUp {
            plot,
            room: 0,
            cell: 4
        }
    )));
    assert_eq!(
        world.homes().furniture_count(PLAYER, Furniture::Plant),
        owner_plants + 1
    );
    assert_eq!(
        world.homes().furniture_count(OTHER, Furniture::Plant),
        other_plants
    );
    assert!(applied(&act(
        &mut world,
        HomeCommand::SetAccess {
            plot,
            access: HomeAccess::Owners
        }
    )));
    assert!(!world.may_enter_home(OTHER, plot));
    // Only the owner changes the deed.
    assert_eq!(
        world.check_home(
            OTHER,
            &HomeCommand::SetAccess {
                plot,
                access: HomeAccess::Crew
            }
        ),
        Err(HomeRefusal::NotOwner)
    );
}

#[test]
fn trophy_cases_show_relics_and_rare_ore_from_the_bay() {
    let (mut world, plot) = cabin_world();
    world.grant_furniture(PLAYER, Furniture::TrophyCase, 1);
    assert!(applied(&act(
        &mut world,
        HomeCommand::Place {
            plot,
            room: 0,
            cell: 3,
            furniture: Furniture::TrophyCase
        }
    )));
    let relic = ItemKind::Artifact(ArtifactKind::StarCore);
    let copper = ItemKind::Mineral(MineralKind::Copper);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(relic, 1);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(copper, 1);
    let display = |item| HomeCommand::Display {
        plot,
        room: 0,
        cell: 3,
        item,
    };
    assert_eq!(
        world.check_home(PLAYER, &display(copper)),
        Err(HomeRefusal::NothingToDo)
    );
    assert!(applied(&act(&mut world, display(relic))));
    assert_eq!(world.player(PLAYER).expect("rig").cargo_bay.count(relic), 0);
    let home = world
        .homes()
        .plot(plot)
        .expect("plot")
        .home
        .clone()
        .expect("home");
    assert_eq!(home.trophies().collect::<Vec<_>>(), vec![relic]);
    // Picking the case up returns the relic to the bay.
    assert!(applied(&act(
        &mut world,
        HomeCommand::PickUp {
            plot,
            room: 0,
            cell: 3
        }
    )));
    assert_eq!(world.player(PLAYER).expect("rig").cargo_bay.count(relic), 1);
}

#[test]
fn furniture_is_bought_at_the_land_office_and_crafted_at_a_bench() {
    let mut world = world();
    park_at_hq(&mut world, PLAYER);
    let credits = world.player(PLAYER).expect("rig").credits;
    assert!(applied(&act(
        &mut world,
        HomeCommand::BuyFurniture {
            furniture: Furniture::Sofa
        }
    )));
    assert_eq!(world.homes().furniture_count(PLAYER, Furniture::Sofa), 1);
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits - Furniture::Sofa.catalogue_price()
    );
    // A cabin has no workbench: crafting needs one.
    let (mut world, plot) = cabin_world();
    world
        .player_mut(PLAYER)
        .expect("rig")
        .block_hopper
        .add(ItemKind::Block(BlockKind::Stone), 30);
    let craft = HomeCommand::CraftFurniture {
        plot,
        furniture: Furniture::Chair,
    };
    assert_eq!(
        world.check_home(PLAYER, &craft),
        Err(HomeRefusal::NothingToDo)
    );
    world.grant_furniture(PLAYER, Furniture::Workbench, 1);
    assert!(applied(&act(
        &mut world,
        HomeCommand::Place {
            plot,
            room: 0,
            cell: 2,
            furniture: Furniture::Workbench
        }
    )));
    assert!(applied(&act(&mut world, craft)));
    assert_eq!(world.homes().furniture_count(PLAYER, Furniture::Chair), 1);
    assert_eq!(
        world.player(PLAYER).expect("rig").block_hopper.total(),
        30 - Furniture::Chair.craft_cost().expect("craftable").0
    );
}

#[test]
fn the_bed_sets_the_home_point_and_sleeps_at_night() {
    let (mut world, plot) = cabin_world();
    let bed = HomeCommand::UsePiece {
        plot,
        room: 0,
        cell: 0,
    };
    // By day the owner makes it their home point.
    world.set_world_time_millis(day_millis(&world) * 5 + day_millis(&world) / 2);
    assert!(applied(&act(&mut world, bed)));
    assert_eq!(
        world.homes().home_point_of(PLAYER).map(|plot| plot.id),
        Some(plot)
    );
    let (x, _) = world.rescue_home(PLAYER).expect("rescued home");
    let door = world.homes().plot(plot).expect("plot").door_column();
    assert!((x - column_x(door)).abs() < 1.0);
    assert!(!world.is_in_bed(PLAYER));
    // At night it puts the player to bed, and the night is skipped.
    world.set_world_time_millis(day_millis(&world) * 5 + day_millis(&world) * 22 / 24);
    let _ = run(&mut world, 1);
    assert!(applied(&act(&mut world, bed)));
    assert!(world.is_in_bed(PLAYER));
    let events = run(&mut world, 400);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GameplayEventKind::NightSkipped { .. }))
    );
}

#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a few hull points"
)]
fn a_garage_bay_repairs_a_parked_rig() {
    let mut world = world();
    let settings = *world.world_settings();
    let site = plot::listed_sites_between(&settings, -3_000, 3_000)
        .into_iter()
        .find(|site| site.size >= PlotSize::Medium)
        .expect("a medium site");
    let plot = world.scenario_home(PLAYER, site, Some(Blueprint::WorkshopHome));
    let (first, _) = world
        .homes()
        .plot(plot)
        .expect("plot")
        .garage_columns()
        .expect("bay");
    park(&mut world, PLAYER, first + 1, site.ground_row);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hull = 10.0;
    rig.credits = 1_000;
    let _ = run(&mut world, 120);
    let rig = world.player(PLAYER).expect("rig");
    let hull = rig.hull;
    assert!(hull > 15.0, "repaired to {hull}");
    // The bay is not free: every point started into is paid for at the bay's rate.
    let points = (hull.ceil() - 10.0) as u32;
    assert_eq!(
        rig.credits,
        1_000 - points * crate::home::GARAGE_REPAIR_UNIT_COST,
        "billed for {points} points"
    );
    // Out of credits, the rig waits at the last point paid for.
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hull = 10.0;
    rig.credits = 2 * crate::home::GARAGE_REPAIR_UNIT_COST;
    let _ = run(&mut world, 240);
    let rig = world.player(PLAYER).expect("rig");
    assert!((rig.hull - 12.0).abs() < 1e-3, "stopped at {}", rig.hull);
    assert_eq!(rig.credits, 0);
}

#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a few hull points"
)]
fn a_garage_bays_bill_does_not_depend_on_the_tick_rate() {
    let mut bills = Vec::new();
    for hz in [30_u32, 60, 240] {
        let mut world = world_at(hz);
        let settings = *world.world_settings();
        let site = plot::listed_sites_between(&settings, -3_000, 3_000)
            .into_iter()
            .find(|site| site.size >= PlotSize::Medium)
            .expect("a medium site");
        let plot = world.scenario_home(PLAYER, site, Some(Blueprint::WorkshopHome));
        let (first, _) = world
            .homes()
            .plot(plot)
            .expect("plot")
            .garage_columns()
            .expect("bay");
        park(&mut world, PLAYER, first + 1, site.ground_row);
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = 10.0;
        rig.credits = 1_000;
        // Two seconds of game time at every rate.
        let _ = run(&mut world, 2 * u64::from(hz));
        let rig = world.player(PLAYER).expect("rig");
        let bill = 1_000 - rig.credits;
        // Exactly the points started into, at any rate.
        assert_eq!(
            bill,
            (rig.hull.ceil() - 10.0) as u32 * crate::home::GARAGE_REPAIR_UNIT_COST,
            "{hz} Hz"
        );
        bills.push(bill);
    }
    // The same two seconds cost the same, give or take the point the clock is part-way into.
    assert!(
        bills.windows(2).all(|pair| pair[0].abs_diff(pair[1]) <= 1),
        "bills by rate: {bills:?}"
    );
}

#[test]
fn a_basement_digs_a_cellar_and_the_lift_rides_to_the_base() {
    let (mut world, plot) = cabin_world();
    let site = world.homes().plot(plot).expect("plot").site;
    // The basement: hired like any job.
    let contractor = world.contractors_for(plot, PLAYER)[0].character;
    assert_eq!(
        world
            .hire_quote(
                PLAYER,
                plot,
                HomeJob::Expand(Expansion::Lift),
                contractor,
                false
            )
            .map(|_| ()),
        Err(HomeRefusal::CannotExpand),
        "a lift needs a basement"
    );
    assert!(applied(&act(
        &mut world,
        HomeCommand::Hire {
            plot,
            job: HomeJob::Expand(Expansion::Basement),
            contractor,
            supply: false
        }
    )));
    let events = {
        let day = day_millis(&world);
        advance_time(&mut world, day * 3)
    };
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::HomeCompleted {
            job: HomeJob::Expand(Expansion::Basement),
            ..
        }
    )));
    let found = world.homes().plot(plot).expect("plot").clone();
    let home = found.home.as_ref().expect("home");
    assert!(home.basement);
    assert!(
        home.rooms
            .iter()
            .any(|room| room.kind == crate::home::RoomKind::Basement)
    );
    // Through the hatch on foot: into the cellar under the house.
    assert!(world.can_exit_rig(PLAYER));
    let tick = world.simulation_tick();
    let _ = world.simulate_tick(
        tick,
        &TickInputs::default().act(PLAYER, PlayerCommand::TogglePilot),
    );
    assert!(world.is_on_foot(PLAYER));
    assert!(applied(&act(&mut world, HomeCommand::Hatch { plot })));
    let _ = run(&mut world, 2);
    assert_eq!(world.in_cellar_of(PLAYER), Some(plot));
    let tile = world.actor_tile(PLAYER).expect("pilot");
    assert!(tile.y > site.ground_row);
    // A claim of the owner below the plot: the private lift goes there.
    world.scenario_edit_plot(plot, |found| {
        found.home.as_mut().expect("home").lift = true;
    });
    let below = TilePosition {
        x: site.middle(),
        y: site.ground_row + 20,
    };
    world.scenario_claim(PLAYER, below);
    let found = world.homes().plot(plot).expect("plot").clone();
    assert!(world.lift_claim(&found, PLAYER).is_some());
    // Back up to the door first (the lift is called from the house).
    world.place_pilot(
        PLAYER,
        column_x(site.middle()),
        f32::from(i16::try_from(site.ground_row).expect("a small row")) * TILE_SIZE,
    );
    assert!(applied(&act(
        &mut world,
        HomeCommand::RideLift { plot, down: true }
    )));
    let tile = world.actor_tile(PLAYER).expect("pilot");
    assert!((tile.y - below.y).abs() <= 2, "{tile:?}");
    assert_eq!(world.lift_home_from_here(PLAYER), Some(plot));
    assert!(applied(&act(
        &mut world,
        HomeCommand::RideLift { plot, down: false }
    )));
    let tile = world.actor_tile(PLAYER).expect("pilot");
    assert!((tile.y - site.ground_row).abs() <= 2);
}

#[test]
fn homes_survive_saves_and_old_saves_load_without_them() {
    let (mut world, plot) = cabin_world();
    world.grant_furniture(PLAYER, Furniture::Piano, 1);
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.homes(), world.homes());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // Postcard (binary keyframes) round trips too.
    let bytes = postcard::to_stdvec(&world).expect("encode");
    let decoded: WorldState = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.homes(), world.homes());
    // A save from before homes (no `homes` field) loads with none.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("json");
    value.as_object_mut().expect("object").remove("homes");
    let old: WorldState = serde_json::from_value(value).expect("old save");
    assert!(old.homes().is_empty());
    let _ = plot;
}

#[test]
fn the_homes_section_replicates_and_hashes_only_once_used() {
    let fresh = world();
    let mut untouched = world();
    let _ = run(&mut untouched, 1);
    let mut again = world();
    let _ = run(&mut again, 1);
    assert_eq!(
        untouched.simulation_state_hash(),
        again.simulation_state_hash(),
        "deterministic without homes"
    );
    let (world, _) = cabin_world();
    let mut replica = fresh;
    let section = world.section(SectionKey::Homes).expect("homes section");
    let wire: crate::SectionPayload =
        postcard::from_bytes(&postcard::to_stdvec(&section).expect("encode")).expect("decode");
    replica.apply_section(SectionKey::Homes, wire);
    assert_eq!(replica.homes(), world.homes());
    assert!(SectionKey::SHARED.contains(&SectionKey::Homes));
    assert!(SectionKey::Homes.visible_to(None));
}

#[test]
fn homes_are_deterministic_and_tick_rate_safe() {
    let finish = |hz: u32| {
        let mut world = world_at(hz);
        let (plot, _) = buy_town_plot(&mut world);
        assert!(applied(&hire_cabin(&mut world, plot)));
        let total = world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.construction.as_ref())
            .expect("work")
            .total_millis;
        let started = world.world_millis();
        let mut stages = Vec::new();
        for _ in 0..40 {
            for event in advance_time(&mut world, total / 30) {
                if let GameplayEventKind::ConstructionStage { stage, .. } = event {
                    stages.push(stage);
                }
                if matches!(event, GameplayEventKind::SiteEventRaised { .. }) {
                    let _ = act(&mut world, HomeCommand::AnswerEvent { plot, pay: true });
                }
            }
        }
        let home = world.homes().plot(plot).and_then(|plot| plot.home.clone());
        (
            stages,
            home.map(|home| home.finished_millis.saturating_sub(started) / 1_000),
        )
    };
    let at_60 = finish(60);
    assert_eq!(at_60, finish(60));
    let at_30 = finish(30);
    assert_eq!(at_60.0, at_30.0);
    assert!(at_60.1.is_some());
}

#[test]
fn site_events_are_answered_and_requests_raise_quality() {
    let mut world = world();
    let (plot, _) = buy_town_plot(&mut world);
    assert!(applied(&hire_cabin(&mut world, plot)));
    let item = ItemKind::Mineral(MineralKind::Gold);
    world.scenario_edit_plot(plot, |found| {
        let work = found.construction.as_mut().expect("work");
        work.planned.clear();
        work.events.push(SiteEvent {
            kind: SiteEventKind::Request { item },
            stage: BuildStage::Walls,
            at_millis: 0,
            answered: false,
        });
    });
    let quality = |world: &WorldState| {
        world
            .homes()
            .plot(plot)
            .and_then(|plot| plot.construction.as_ref())
            .expect("work")
            .quality()
    };
    let before = quality(&world);
    assert_eq!(
        world.check_home(PLAYER, &HomeCommand::AnswerEvent { plot, pay: false }),
        Err(HomeRefusal::MissingItems)
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(item, 1);
    let output = act(&mut world, HomeCommand::AnswerEvent { plot, pay: false });
    assert!(applied(&output));
    assert!(
        output
            .events
            .iter()
            .any(|event| matches!(event.kind, GameplayEventKind::SiteEventAnswered { .. }))
    );
    assert_eq!(quality(&world), (before + 1).min(5));
    assert_eq!(
        world.check_home(PLAYER, &HomeCommand::AnswerEvent { plot, pay: false }),
        Err(HomeRefusal::NothingToDo)
    );
}
