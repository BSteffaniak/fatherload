//! Wave 6 content in the world: the sky towns' land offices and their isle plots, the sky
//! towns' folk, the Moon's colonists (routines, talk and their offers), culture furniture
//! bought from people, and tomb trophies for the home.

use crate::{
    ControlState, PlayerCommand, PlayerId, PlayerScopedCommandOutcome, SimulationParams,
    TickInputs, TickOutput, WorldState,
    entity::{EntityKind, EntityState},
    home::{
        Blueprint, Expansion, Furniture, HomeCommand, HomeJob, HomeRefusal, PlotSetting, PlotSize,
        SiteKey, plot,
    },
    people::{PeopleCommand, TalkTopic, colony, offers},
    terrain::{TILE_SIZE, TilePosition},
    village::{self, BuildingKind, VillageId},
    worldgen::WorldSettings,
};

const PLAYER: PlayerId = PlayerId::new(1);
const SEED: u64 = 0x5EED_0006;

fn world() -> WorldState {
    let settings = WorldSettings::new(SEED);
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(30));
    world.player_mut(PLAYER).expect("rig").credits = 500_000;
    world
}

fn step(world: &mut WorldState) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(
        tick,
        &TickInputs::held(PLAYER, ControlState::new(0.0, false, false)),
    )
}

fn run(world: &mut WorldState, ticks: u64) {
    for _ in 0..ticks {
        let _ = step(world);
    }
}

fn act(world: &mut WorldState, command: PlayerCommand) -> TickOutput {
    let tick = world.simulation_tick();
    world.simulate_tick(tick, &TickInputs::default().act(PLAYER, command))
}

fn applied(output: &TickOutput) -> bool {
    output
        .action_outcomes
        .first()
        .is_some_and(|outcome| outcome.outcome == PlayerScopedCommandOutcome::Applied)
}

/// Parks the rig standing on `ground_row` at `column`, at rest.
#[allow(clippy::cast_precision_loss, reason = "columns and rows fit f32")]
fn park(world: &mut WorldState, column: i32, ground_row: i32) {
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.x = (column as f32 + 0.5) * TILE_SIZE;
    rig.y = (ground_row as f32 - 0.5) * TILE_SIZE;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.x_remainder = 0.0;
}

/// The first sky town (west to east) whose land office lists isle plots (and whose id names no
/// sky city: see `sky_town_ids_only_collide_with_cities_above_the_spawn`).
fn sky_town_with_plots(settings: &WorldSettings) -> (VillageId, Vec<plot::PlotSite>) {
    crate::worldgen::sky::towns::sky_towns_between(settings, -12_000, 12_000)
        .into_iter()
        .filter(|town| crate::worldgen::sky::continent::sky_city(settings, town.id).is_none())
        .find_map(|town| {
            let sites = plot::sky_sites_of(settings, town.id);
            (!sites.is_empty()).then_some((town.id, sites))
        })
        .expect("a sky town with isle plots")
}

/// The NPC entities of settlement `id` about in the world.
fn folk_of(
    world: &WorldState,
    id: VillageId,
) -> Vec<(crate::entity::EntityId, crate::entity::NpcState)> {
    world
        .entities()
        .of_kind(EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            EntityState::Npc(state) if state.character.village == id => Some((entity.id, state)),
            _ => None,
        })
        .collect()
}

#[test]
#[allow(clippy::too_many_lines, reason = "one walk through the land office")]
fn sky_towns_list_isle_plots_at_their_land_office() {
    let mut world = world();
    let settings = *world.world_settings();
    let (town, sites) = sky_town_with_plots(&settings);
    // Listed sites are pure in the settings: the same every time, found again by key, on a
    // floating isle's level turf, small or medium (no estates in the clouds).
    assert_eq!(sites, plot::sky_sites_of(&settings, town));
    for site in &sites {
        assert!(matches!(site.key, SiteKey::SkyIsle(found, _) if found == town));
        assert_eq!(site.setting, PlotSetting::SkyIsland);
        assert!(matches!(site.size, PlotSize::Small | PlotSize::Medium));
        assert_eq!(plot::listed_site(&settings, site.key), Some(*site));
        for x in site.left..=site.right() {
            let at = TilePosition {
                x,
                y: site.ground_row,
            };
            assert!(world.terrain().is_solid_at(at), "{site:?}: turf under {x}");
        }
    }
    // The ground offices' list never shows them.
    let middle = sites[0].middle();
    assert!(
        plot::listed_sites_between(&settings, middle - 50, middle + 50)
            .iter()
            .all(|site| !matches!(site.key, SiteKey::SkyIsle(..)))
    );
    // The town's hall is its land office.
    let found = village::village(&settings, town).expect("the sky town");
    let hall = found
        .buildings
        .iter()
        .find(|lot| lot.kind == BuildingKind::Hall)
        .expect("a hall");
    park(
        &mut world,
        hall.first_column + hall.width / 2,
        found.ground_row,
    );
    assert!(world.at_land_office(PLAYER));
    assert_eq!(world.at_sky_land_office(PLAYER), Some(town));
    assert_eq!(world.land_office_listing(PLAYER, middle, 2_400), sites);
    let catalogue = world.land_office_catalogue(PLAYER);
    assert!(catalogue.contains(&Furniture::CloudHammock));
    // Buying an isle plot and a hammock.
    let site = sites[0];
    let bought = act(
        &mut world,
        PlayerCommand::Home(HomeCommand::BuyPlot {
            site: site.key,
            size: site.size,
        }),
    );
    assert!(applied(&bought), "{:?}", bought.action_outcomes);
    let plot = world.homes().plot_of_site(site.key).expect("a deed").id;
    let hammock = act(
        &mut world,
        PlayerCommand::Home(HomeCommand::BuyFurniture {
            furniture: Furniture::CloudHammock,
        }),
    );
    assert!(applied(&hammock));
    assert_eq!(
        world
            .homes()
            .furniture_count(PLAYER, Furniture::CloudHammock),
        1
    );
    // The town's own builders take the work up there.
    let builders = world.contractors_for(plot, PLAYER);
    assert!(
        builders.iter().any(|offer| offer.from == found.name),
        "{builders:?}"
    );
    // A ground office doesn't sell sky furniture, nor the sky town's isle plots.
    let hq = crate::surface::SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == crate::economy::SurfaceZone::Headquarters)
        .expect("hq");
    let ground = world.terrain().surface_row(hq.tile_x + 2);
    park(&mut world, hq.tile_x + 2, ground);
    assert_eq!(world.at_sky_land_office(PLAYER), None);
    // (Refused before the deed is looked up, so even the plot bought above answers this.)
    let other = sites[sites.len() - 1];
    assert_eq!(
        world.check_home(
            PLAYER,
            &HomeCommand::BuyPlot {
                site: other.key,
                size: other.size,
            },
        ),
        Err(HomeRefusal::NotAtLandOffice)
    );
    assert_eq!(
        world.check_home(
            PLAYER,
            &HomeCommand::BuyFurniture {
                furniture: Furniture::CloudHammock,
            },
        ),
        Err(HomeRefusal::NoFurniture)
    );
    // A finished cloud cottage on the isle: no cellar under a floating island.
    let built = world.scenario_home(
        PLAYER,
        sites[sites.len() - 1],
        Some(Blueprint::CloudCottage),
    );
    let site = world.homes().plot(built).expect("plot").site;
    park(&mut world, site.middle(), site.ground_row);
    let cellar = world.check_home(
        PLAYER,
        &HomeCommand::Hire {
            plot: built,
            job: HomeJob::Expand(Expansion::Basement),
            contractor: None,
            supply: false,
        },
    );
    assert_eq!(cellar, Err(HomeRefusal::CannotExpand));
}

#[test]
fn sky_town_folk_come_out_to_meet_rigs() {
    let mut world = world();
    let settings = *world.world_settings();
    let (town, _) = sky_town_with_plots(&settings);
    let found = village::village(&settings, town).expect("the sky town");
    let settlement = crate::people::routine::settlement(&settings, town).expect("its people");
    assert_eq!(
        settlement.population.village_name,
        found.population.village_name
    );
    park(&mut world, found.center_column(), found.ground_row);
    run(&mut world, 30);
    assert!(
        !folk_of(&world, town).is_empty(),
        "the sky town's folk are about"
    );
}

#[test]
fn sky_town_ids_only_collide_with_cities_above_the_spawn() {
    // Sky towns and sky cities share the sky flag in their ids. Cities only stand in cells
    // inside the streamed world, so a sky town id names a city only in block column 0 (the
    // towns straight above the spawn), where the city keeps the id as before.
    for seed in [SEED, 11, 0x5EED] {
        let settings = WorldSettings::new(seed);
        let towns = crate::worldgen::sky::towns::sky_towns_between(&settings, -40_000, 40_000);
        assert!(towns.len() > 100);
        for town in &towns {
            let city = crate::worldgen::sky::continent::sky_city(&settings, town.id);
            assert!(city.is_none() || town.block.0 == 0, "{:?}", town.block);
            let settlement = crate::people::routine::settlement(&settings, town.id);
            if city.is_none() {
                let found = village::village(&settings, town.id).expect("the town");
                assert_eq!(
                    settlement.map(|settlement| settlement.population.village_name.clone()),
                    Some(found.population.village_name.clone()),
                    "{:?}: the town's own people",
                    town.block
                );
            }
        }
    }
}

#[test]
fn colonists_live_talk_and_trade_at_the_moon_colony() {
    let mut world = world();
    let settings = *world.world_settings();
    let id = colony::colony_settlement_id(&settings).expect("a colony");
    let settlement = crate::people::routine::settlement(&settings, id).expect("the colony");
    park(
        &mut world,
        settlement.center_column(),
        settlement.ground_row,
    );
    run(&mut world, 30);
    let folk = folk_of(&world, id);
    assert!(!folk.is_empty(), "colonists are about");
    // Talk to one out in the colony.
    let (npc, state) = folk
        .iter()
        .find(|(_, state)| state.inside.is_none())
        .copied()
        .expect("someone outdoors");
    let (x, y) = world.npc_position(npc).expect("position");
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.x = x;
        rig.y = y - TILE_SIZE;
    }
    let talked = act(
        &mut world,
        PlayerCommand::People(PeopleCommand::Talk {
            npc,
            topic: TalkTopic::Chat,
        }),
    );
    assert!(applied(&talked), "{:?}", talked.action_outcomes);
    // The quartermaster and the hydroponist sell the colony's furniture.
    let character = crate::people::routine::character(&settings, state.character).expect("who");
    let list = offers::offers_of(&character);
    if let Some(row) = list
        .iter()
        .position(|offer| matches!(offer.deal, offers::PersonDeal::Furniture(_)))
    {
        let offers::PersonDeal::Furniture(piece) = list[row].deal else {
            unreachable!("matched above")
        };
        world.add_goodwill(PLAYER, character.id, 60);
        let before = world.homes().furniture_count(PLAYER, piece);
        let bought = act(
            &mut world,
            PlayerCommand::People(PeopleCommand::Trade {
                npc,
                offer: u8::try_from(row).expect("a short list"),
            }),
        );
        assert!(applied(&bought), "{:?}", bought.action_outcomes);
        assert_eq!(world.homes().furniture_count(PLAYER, piece), before + 1);
    }
    // Every colonist is a lunar folk with a trade from the colony's pool.
    for person in &settlement.population.characters {
        assert!(crate::people::Role::LUNAR_POOL.contains(&person.role));
        assert_eq!(person.culture, crate::people::Culture::Lunar);
    }
    // The station's quartermaster and hydroponist always sell the moon globe.
    for role in [
        crate::people::Role::Quartermaster,
        crate::people::Role::Hydroponist,
    ] {
        let person = settlement
            .population
            .characters
            .iter()
            .find(|person| person.role == role)
            .expect("at every station");
        assert!(offers::offers_of(person).iter().any(|offer| matches!(
            offer.deal,
            offers::PersonDeal::Furniture(crate::home::Furniture::MoonGlobe)
        )));
    }
    // Saves keep the colony's people and what they remember.
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.people(), world.people());
}

#[test]
fn makers_sell_their_cultures_signature_furniture() {
    // Every culture's makers and traders offer its signature piece, appended after the rows
    // they always had (so old trade indices still name the same rows).
    for culture in crate::people::Culture::EVERY {
        let population = crate::people::generate_population(SEED, VillageId(7), culture, 24);
        let makers: Vec<_> = population
            .characters
            .iter()
            .filter(|person| offers::sells_signature(person.role))
            .collect();
        for person in &makers {
            let list = offers::offers_of(person);
            let rows: Vec<_> = list
                .iter()
                .filter_map(|offer| match offer.deal {
                    offers::PersonDeal::Furniture(piece) => Some(piece),
                    _ => None,
                })
                .collect();
            assert!(
                rows.contains(&offers::signature_furniture(culture)),
                "{culture:?} {:?} sells {rows:?}",
                person.role
            );
            assert!(!matches!(list[0].deal, offers::PersonDeal::Furniture(_)));
        }
    }
}
