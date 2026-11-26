//! Part III, end to end (Wave 13 integration): the seams between the wave's tracks, and one
//! bot's whole galaxy.
//!
//! * The seams: every §16 planet theme is a real theme world (no space tag on a placeholder),
//!   with its look from orbit, its music, its settlements and (the six Wave 13 themes) its
//!   boss on the endgame's hunt list; the galaxy story's relics stand at the new worlds'
//!   builders' outposts; the completion screen counts the story's chapters.
//! * The journey: the galaxy story from its first chapter to the finale (test-mode warps
//!   between systems), then a landing on all twelve kinds of world, a theme boss beaten in its
//!   lair with the player's weapon, a home bought in an alien town on a Wave 13 world, and a
//!   procedural lead taken up and finished in a system the road never visited.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test bookkeeping"
)]

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, TestModeAction, WorldState,
    alien::{AlienCommand, AlienEvent, AlienSpecies, town},
    endgame::{EndgameEvent, hunt},
    entity::{EntityKind, EntityState},
    galaxy_story::{CHAPTER_COUNT, GalaxyEvent, LeadStep, leads_of},
    input::WeaponControl,
    place::{BodyId, Place, SpaceBodyKey, StoreKey, planet_theme_of},
    planet::{
        BodyGround, PlanetGround, PlanetThemeId,
        hooks::hooks,
        outlands::{self, boss_lair},
    },
    space::{SpaceSector, SystemRef, Theme},
};

use super::{galaxy_story_tests as story, galaxy_tour_tests as tour};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x00C0_FFEE;

/// The Wave 13 themes, by planet theme id.
const WAVE_13_THEMES: [PlanetThemeId; 6] = [
    PlanetThemeId::FUNGAL,
    PlanetThemeId::MACHINE,
    PlanetThemeId::DEAD,
    crate::exotic::GAS_GIANT,
    crate::exotic::LOCKED_WORLD,
    crate::exotic::RING_WORLD,
];

#[test]
fn every_planet_theme_is_a_real_world_with_its_look_music_and_settlements() {
    let placeholders = [
        PlanetThemeId::BARREN,
        PlanetThemeId::ROCKY,
        PlanetThemeId::ICE,
    ];
    let mut seen = Vec::new();
    for tag in Theme::PLANET {
        let id = planet_theme_of(tag);
        assert!(!placeholders.contains(&id), "{tag:?} is on a placeholder");
        assert!(!seen.contains(&id), "{tag:?} shares a theme");
        seen.push(id);
        let theme = hooks(id);
        assert_eq!(theme.id(), id);
        assert!(theme.orbital_look().is_some(), "{tag:?}: a face from orbit");
        assert_ne!(
            theme.music_bed().name,
            "bodylife_generic",
            "{tag:?}: its own music"
        );
        assert!(!theme.creature_spawns().is_empty(), "{tag:?}: creatures");
        assert!(theme.weather_table().len() >= 2, "{tag:?}: weather");
        // A planet of the theme has its settlement site and its strata.
        let ground = PlanetGround::new(crate::planet::PlanetParams::new(
            0x5EED_1300 + u64::from(id.0),
            2_400,
            id,
        ));
        assert!(
            theme.settlement_site(&ground).is_some(),
            "{tag:?}: a settlement"
        );
        let depth = ground.core_chamber_top() - ground.surface_row(0);
        assert!(depth >= 800, "{tag:?}: {depth} rows of strata");
    }
    assert_eq!(seen.len(), 12);
}

#[test]
fn every_wave_13_theme_boss_is_on_the_hunt_list() {
    let list = hunt::hunt_list();
    assert_eq!(list.len(), 3 + WAVE_13_THEMES.len());
    for theme in WAVE_13_THEMES {
        let quarry = hunt::Quarry::Theme(theme.0);
        assert!(list.contains(&quarry), "{theme:?}");
        assert_ne!(quarry.name(), "Unknown");
        assert_ne!(quarry.whereabouts(), "");
    }
    for boss in &outlands::THEME_BOSSES {
        assert_eq!(hunt::theme_boss_of(boss.kind), Some(boss.theme.0));
    }
    for (style, kind, _) in crate::exotic::boss::THEME_BOSSES {
        assert_eq!(hunt::theme_boss_of(kind), Some(style.theme().0));
    }
    assert_eq!(hunt::theme_boss_of(crate::enemy::EnemyKind::CaveBat), None);
    // Every quarry has somewhere to head for: a lair, or the nearest world of its kind with a
    // boss's lair on it.
    let world = story::world_with(&[PLAYER]);
    for (quarry, beaten, system) in world.hunt() {
        assert!(!beaten, "{quarry:?}");
        assert!(system.is_some(), "{quarry:?}: a waypoint");
        if let hunt::Quarry::Theme(theme) = quarry {
            let boss = hunt::THEME_BOSSES
                .iter()
                .find(|boss| boss.theme == theme)
                .expect("registered");
            let id = hunt::nearest_theme_world(SEED, boss.world).expect("a world");
            let params = crate::place::planet_params_for(SEED, id).expect("landable");
            assert_eq!(params.theme.0, theme);
            let ground = PlanetGround::new(params);
            let has_lair =
                boss_lair(&ground).is_some() || crate::exotic::boss::arena(&ground).is_some();
            assert!(has_lair, "{quarry:?}: its world has the boss's lair");
        }
    }
}

/// A space body tagged `tag` near home (rings 0-6), a full planet if one is near.
fn body_of(tag: Theme) -> SpaceBodyKey {
    let space = crate::space::Space::shared(SEED);
    let mut best: Option<(u32, SpaceBodyKey)> = None;
    for ring in 0..=6_i32 {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = SpaceSector::new(x, y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        let key = SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if body.theme == tag
                            && crate::place::planet_params_for(SEED, key).is_some()
                            && best.is_none_or(|(radius, _)| body.radius > radius)
                        {
                            best = Some((body.radius, key));
                        }
                    }
                }
            }
        }
        if best.is_some_and(|(radius, _)| radius >= 1_500) {
            break;
        }
    }
    best.unwrap_or_else(|| panic!("no {tag:?} world near home"))
        .1
}

#[test]
fn the_story_relics_stand_at_the_new_worlds_builders_outposts() {
    for tag in [
        Theme::Fungal,
        Theme::MachineWorld,
        Theme::DeadWorld,
        Theme::GasGiant,
        Theme::TidallyLocked,
        Theme::RingWorld,
    ] {
        let id = body_of(tag);
        for rows in [0_u16, 15] {
            let site = crate::galaxy_story::plan::outpost_relic(SEED, id, rows)
                .unwrap_or_else(|| panic!("{tag:?}: an outpost"));
            let params = crate::place::planet_params_for(SEED, id).expect("landable");
            let ground = PlanetGround::new(params);
            let width = ground.strip_width();
            let (column, floor) = outlands::builder_outposts(&ground).first().map_or_else(
                || {
                    let outpost = crate::exotic::builder_outpost(&ground).expect("an outpost");
                    (outpost.column, outpost.floor)
                },
                |outpost| (outpost.glyph.0, outpost.site.floor),
            );
            let off = (i32::try_from(site.column).expect("column") - column).rem_euclid(width);
            let off = off.min(width - off);
            assert!(
                off <= 6,
                "{tag:?}: the relic is {off} columns off the outpost"
            );
            assert_eq!(site.base, Some(floor), "{tag:?}");
            assert_eq!(site.row_on(&ground), floor + i32::from(rows));
            if rows == 0 {
                // It stands on solid ground: the outpost's floor under the rig.
                let below = ground.generated_tile(
                    i32::try_from(site.column).expect("column"),
                    site.row_on(&ground),
                );
                assert!(!below.kind.is_open(), "{tag:?}: {below:?} under the relic");
            }
        }
    }
    // A Wave 12 world keeps its hashed column.
    assert!(crate::galaxy_story::plan::outpost_relic(SEED, body_of(Theme::Jungle), 0).is_none());
}

#[test]
fn alien_towns_stand_on_the_wave_13_worlds_the_species_settle() {
    for species in AlienSpecies::ALL {
        for tag in species.def().habitats {
            let id = planet_theme_of(*tag);
            if !WAVE_13_THEMES.contains(&id) {
                continue;
            }
            for radius in [1_600_u32, 2_400] {
                let params = crate::planet::PlanetParams {
                    folk: species.folk(),
                    ..crate::planet::PlanetParams::new(
                        0x7A11_1300 + u64::from(radius) + u64::from(id.0),
                        radius,
                        id,
                    )
                };
                let ground = PlanetGround::new(params);
                let towns = town::towns_of(&params);
                assert!(!towns.is_empty(), "{species:?} on {tag:?} R {radius}");
                for found in towns.iter() {
                    for column in found.first_column()..=found.last_column() {
                        assert_eq!(
                            ground.surface_row(column),
                            found.ground_row(),
                            "{species:?} on {tag:?}: level"
                        );
                        let above = ground.generated_tile(column, found.ground_row() - 1);
                        assert!(above.kind.is_open(), "{species:?} on {tag:?}: open sky");
                    }
                }
            }
        }
    }
}

#[test]
fn the_completion_screen_counts_the_galaxy_story() {
    let mut world = story::world_with(&[PLAYER]);
    let facts = world.completion_facts(PLAYER);
    assert_eq!(facts.story_total, CHAPTER_COUNT);
    assert_eq!(facts.story_done, 0);
    assert_eq!(facts.theme_bosses_total, 6);
    world.galaxy_story.chapter = 4;
    assert_eq!(world.completion_facts(PLAYER).story_done, 4);
    world.galaxy_story.chapter = CHAPTER_COUNT;
    assert_eq!(world.completion_facts(PLAYER).story_done, CHAPTER_COUNT);
}

// --- The journey ------------------------------------------------------------------------------

/// The whole builders' road, goal by goal (warps between systems).
fn walk_the_road(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let mut all = story::run(world, 3);
    let mut guard = 0;
    while let Some(goal) = story::goal(world) {
        let before = (world.galaxy_story().chapter, world.galaxy_story().step);
        all.extend(story::do_goal(world, goal));
        let after = (world.galaxy_story().chapter, world.galaxy_story().step);
        assert_ne!(
            before,
            after,
            "goal {goal:?} of chapter {} done",
            before.0 + 1
        );
        guard += 1;
        assert!(guard < 64, "the road ends");
    }
    all
}

/// The theme boss of the fungal world nearest home, fought in its lair with the rig's own
/// railgun aimed at it (test-mode armour: the fight's fairness is the planets-c bot's).
fn beat_the_sporemother(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let id = body_of(Theme::Fungal);
    let store = StoreKey::Body(id);
    let _ = tour::act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::OnBody(BodyId::Space(
            id,
        )))),
    );
    let _ = tour::run(world, 20);
    let ground = *world
        .store_terrain(store)
        .and_then(|terrain| terrain.planet_ground())
        .expect("round ground");
    let lair = boss_lair(&ground).expect("a lair on a full planet");
    let width = i64::from(ground.strip_width());
    let (middle, _) = lair.middle();
    let column = (middle + 12).rem_euclid(width) as f64;
    tour::put_rig(world, id, column, lair.floor, 1.0);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        crate::economy::max_out_upgrades(rig);
        rig.weapon_rack.battery_level = crate::weapon::MAX_BATTERY_LEVEL;
        let _ = rig.receive_weapon(crate::weapon::WeaponKind::Railgun);
        let _ = rig.mount_weapon(Some(crate::weapon::WeaponKind::Railgun));
    }
    let mut events = tour::act(
        world,
        PlayerCommand::TestMode(TestModeAction::SetInvulnerable(true)),
    );
    for tick in 0..tour::seconds(300.0) {
        let boss = world.store_entities(store).and_then(|entities| {
            entities
                .of_kind(EntityKind::Enemy)
                .find(|entity| {
                    matches!(entity.state, EntityState::Enemy(state)
                        if state.kind == lair.boss.kind)
                })
                .map(|entity| (entity.body.x, entity.body.y))
        });
        let rig = world.store_actor_position(PLAYER).map(|(x, y, _)| (x, y));
        let mut controls = ControlState::default();
        if let (Some((bx, by)), Some((rx, ry))) = (boss, rig) {
            let (ax, ay) = (bx - rx, by - (ry - crate::weapon::MUZZLE_RISE));
            let length = ax.hypot(ay).max(1.0);
            controls = controls.with_weapon(WeaponControl {
                fire: tick % 6 < 3,
                auto_aim: false,
                aim_x: (ax / length * 127.0) as i8,
                aim_y: (ay / length * 127.0) as i8,
            });
        }
        let seen = tour::tick(world, controls, None);
        let won = seen.iter().any(|event| {
            matches!(
                event,
                GameplayEventKind::Outlands {
                    what: outlands::OutlandsEvent::BossDefeated,
                    ..
                }
            )
        });
        events.extend(seen);
        if let Some(rig) = world.player_mut(PLAYER) {
            rig.fuel = rig.fuel_capacity;
        }
        if won {
            eprintln!("the Sporemother fell after {:.1} s", tick as f32 / 30.0);
            break;
        }
    }
    events.extend(tour::act(
        world,
        PlayerCommand::TestMode(TestModeAction::SetInvulnerable(false)),
    ));
    events
}

/// A home bought from the builder of an alien town on a Wave 13 world.
fn buy_a_home_on_a_new_world(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let space = crate::space::Space::shared(SEED);
    let (id, species, found) = (0..=8_i32)
        .flat_map(|ring| {
            (-ring..=ring).flat_map(move |y| {
                (-ring..=ring)
                    .filter(move |x| x.abs().max(y.abs()) == ring)
                    .map(move |x| SpaceSector::new(x, y))
            })
        })
        .flat_map(|sector| {
            space
                .sector(sector)
                .systems
                .iter()
                .flat_map(|system| {
                    system
                        .bodies
                        .iter()
                        .map(|body| SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .find_map(|key| {
            let params = crate::place::planet_params_for(SEED, key)?;
            if !WAVE_13_THEMES.contains(&params.theme) {
                return None;
            }
            let species = AlienSpecies::from_folk(params.folk)?;
            let found = std::sync::Arc::clone(town::towns_of(&params).first()?);
            Some((key, species, found))
        })
        .expect("an alien town on a Wave 13 world near home");
    let place = Place::OnBody(BodyId::Space(id));
    let mut events = tour::act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(place)),
    );
    tour::put_rig(
        world,
        id,
        f64::from(found.site.centre),
        found.ground_row(),
        2.0,
    );
    events.extend(tour::run(world, 40));
    let plot = found
        .buildings
        .iter()
        .find(|building| building.kind == crate::alien::AlienBuildingKind::HomePlot)
        .copied()
        .expect("a plot for sale");
    world.player_mut(PLAYER).expect("rig").credits += super::aliens::home_price(species) * 2;
    let builder = tour::beside(world, id, &found, town::BUILDER);
    events.extend(tour::act(
        world,
        PlayerCommand::Alien(AlienCommand::BuyHome {
            npc: builder,
            plot: plot.index,
        }),
    ));
    assert!(
        tour::alien_saw(&events, AlienEvent::HomeBought),
        "a home bought from the {species:?} on {:?}",
        planet_theme_of(space.body(id).expect("body").theme)
    );
    // The town's builder raises it in world time (a quarter of a day).
    for _ in 0..tour::seconds(3_600.0) {
        events.extend(tour::run(world, 30));
        if world.owns_alien_home(PLAYER) {
            break;
        }
    }
    events
}

/// A lost ship's lead in a system the road never visited, flown to and paid.
fn a_lead_in_a_fresh_system(world: &mut WorldState) -> Vec<GameplayEventKind> {
    let space = crate::space::Space::shared(SEED);
    let visited = world.galaxy_story().visited.clone();
    let (system, lead) = (1..6_i32)
        .flat_map(|ring| {
            (-ring..=ring).flat_map(move |y| {
                (-ring..=ring)
                    .filter(move |x| x.abs().max(y.abs()) == ring)
                    .map(move |x| SpaceSector::new(x, y))
            })
        })
        .flat_map(|coords| {
            space
                .sector(coords)
                .systems
                .iter()
                .map(move |system| SystemRef {
                    sector: coords,
                    system: system.index,
                })
                .collect::<Vec<_>>()
        })
        .filter(|system| !visited.contains(system))
        .find_map(|system| {
            leads_of(SEED, system)
                .into_iter()
                .find(|lead| matches!(lead.steps.as_slice(), [LeadStep::ReachPoint(_)]))
                .map(|lead| (system, lead))
        })
        .expect("a lost ship in a fresh system");
    let LeadStep::ReachPoint(point) = lead.steps[0] else {
        unreachable!()
    };
    let star = space.system(system).expect("system").centre;
    let mut events = story::act(
        world,
        PlayerCommand::TestMode(TestModeAction::WarpToPlace(Place::Space)),
    );
    story::launch(world, star.offset(0, -40_000));
    events.extend(story::run(world, 20));
    let index = world
        .galaxy_story()
        .leads
        .iter()
        .position(|record| record.system == system && record.index == lead.index)
        .expect("picked up in the discovery log");
    let credits = world.player(PLAYER).expect("rig").credits;
    story::launch(world, point.offset(1_000, 0));
    let done = story::run(world, 20);
    assert!(
        story::galaxy(&done).contains(&(GalaxyEvent::LeadDone, index as u32)),
        "the lead is finished"
    );
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        credits + lead.credits
    );
    events.extend(done);
    events
}

#[test]
#[allow(clippy::too_many_lines, reason = "the whole of Part III, leg by leg")]
fn a_bot_plays_part_iii_from_the_road_to_a_fresh_system() {
    let mut world = story::world_with(&[PLAYER]);
    world.player_mut(PLAYER).expect("rig").hazard_gear.arms = crate::hostiles::ShipGear::MAXED;
    // 1. The builders' road, first chapter to the finale.
    let road = walk_the_road(&mut world);
    let story_events = story::galaxy(&road);
    assert!(world.galaxy_story().finished());
    assert!(world.galaxy_story().choice.is_some());
    for chapter in 0..CHAPTER_COUNT {
        assert!(
            story_events.contains(&(GalaxyEvent::ChapterDone, u32::from(chapter))),
            "chapter {chapter} closed"
        );
    }
    let remarks = story_events
        .iter()
        .filter(|(what, _)| *what == GalaxyEvent::ArchivistRemark)
        .count();
    assert!(
        remarks >= 5,
        "the archivist talks along the road: {remarks}"
    );
    assert_eq!(
        world.completion_facts(PLAYER).story_done,
        CHAPTER_COUNT,
        "the completion screen counts the road"
    );
    // 2. A landing on all twelve kinds of world, each alive with its own creatures.
    for tag in Theme::PLANET {
        let native = tour::tour_theme(&mut world, tag);
        eprintln!("{tag:?}: met {native:?}");
        let store = StoreKey::Body(tour::body_of_theme(tag));
        let theme = world
            .store_terrain(store)
            .and_then(|terrain| terrain.planet_ground())
            .map(|ground| ground.params().theme);
        assert_eq!(theme, Some(planet_theme_of(tag)), "{tag:?}");
    }
    let career = world
        .endgame
        .careers
        .get(&PLAYER)
        .expect("a career")
        .clone();
    for tag in Theme::PLANET {
        assert!(career.themes.contains(&tag), "landed on a {tag:?} world");
    }
    // 3. A theme boss beaten in its lair: the hunt list has it down.
    let fight = beat_the_sporemother(&mut world);
    let quarry = hunt::Quarry::Theme(PlanetThemeId::FUNGAL.0);
    assert!(world.quarry_beaten(quarry), "the Sporemother is down");
    assert!(fight.iter().any(|event| matches!(
        event,
        GameplayEventKind::Endgame {
            what: EndgameEvent::QuarryDown,
            ..
        }
    )));
    assert!(world.completion_facts(PLAYER).theme_bosses >= 1);
    // 4. A home in an alien town on a Wave 13 world.
    let _ = buy_a_home_on_a_new_world(&mut world);
    assert!(world.owns_alien_home(PLAYER));
    // 5. After the road, a fresh system still holds a lead to follow.
    let _ = a_lead_in_a_fresh_system(&mut world);
}
