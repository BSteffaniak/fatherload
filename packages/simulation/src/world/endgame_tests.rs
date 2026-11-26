//! World tests of the endgame (Wave 13 endgame): belt rocks give space materials and the
//! career counts them; ship lines' late tiers spend them; the Works counter (materials, alien
//! tech marks behind the tongue, the Starwright ship, the Heartline Drive behind the hunt list,
//! the Halcyon Gate and its jump, the outfitter); estates and neighbours on alien worlds;
//! worlds landed, completion and the space achievements; a late rig's purse at the shipyard;
//! and the endgame state saves, replicates and stays out of old worlds' hashes.

#![allow(
    clippy::float_cmp,
    reason = "effect factors are exact constants compared on purpose"
)]

use std::sync::{Arc, OnceLock};

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    alien::{AlienCommand, AlienSpecies, atlas::first_home, state::AlienHome, town},
    attitude::RigAttitude,
    endgame::{
        EndgameEvent, EndgameRefusal, StarwrightPart, WorkOrder,
        hunt::Quarry,
        materials::{SPACE_MATERIALS, is_space_material, station_stock},
        works::{
            GATE_PARTS, HEARTLINE_BILL, MARK_MAX, OUTFITTER, STARWRIGHT_DAMAGE, STARWRIGHT_ENGINE,
            gate_part_bill, line_bill, mark_bill,
        },
    },
    enemy::EnemyDifficulty,
    event::TickOutput,
    flight::{
        FlightCommand, FlightPlace, RIG_CONTACT, RigFlight, StarDrive, StationService,
        physics::up_angle,
        space::{FlightBody, GeneratedSpace, SpaceQuery, tiles},
        systems::{BodyExtra, SystemsEvent},
    },
    hostiles::Faction,
    milestone::Milestone,
    place::{BodyId, Place, SpaceBodyKey},
    position::WorldPos,
    space::{HOME_SECTOR, SpaceSector},
    station::{
        AlienTech, ShipLine, ShipUpgrade, StationAction, StationEvent, StationId, StationOwner,
        StationProfile, StationRefusal,
        profile::{profile_of, station_ref},
    },
    terrain::MineralKind,
    world::{SectionKey, strip_position},
    worldgen::{GeneratorVersion, WorldSettings},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x00C0_FFEE;
const HALCYON: StationId = StationId {
    sector: HOME_SECTOR,
    system: 0,
    index: 0,
};

fn world_of(seed: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(seed)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::new(30));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.insert(PLAYER);
    world.set_heart(heart);
    let rig = world.player_mut(PLAYER).expect("rig");
    crate::economy::max_out_upgrades(rig);
    rig.hazard_gear = crate::protection::HazardGear::MAXED;
    rig.hazard_gear.ship = crate::station::ShipGear::default();
    rig.hull = rig.max_hull();
    rig.fuel = rig.fuel_capacity;
    rig.credits = 0;
    world
}

fn tick(world: &mut WorldState, command: Option<PlayerCommand>) -> TickOutput {
    let mut inputs = TickInputs::held(PLAYER, ControlState::default());
    if let Some(command) = command {
        inputs = inputs.act(PLAYER, command);
    }
    world.simulate_tick(world.simulation_tick(), &inputs)
}

fn act(world: &mut WorldState, command: PlayerCommand) -> Vec<GameplayEventKind> {
    tick(world, Some(command))
        .events
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn run(world: &mut WorldState, ticks: u32) -> Vec<GameplayEventKind> {
    (0..ticks)
        .flat_map(|_| tick(world, None).events.into_iter().map(|event| event.kind))
        .collect()
}

fn hub(world: &mut WorldState, action: StationAction) -> Vec<GameplayEventKind> {
    act(
        world,
        PlayerCommand::Flight(FlightCommand::Station(StationService::Hub(action))),
    )
}

fn works(world: &mut WorldState, order: WorkOrder) -> Vec<GameplayEventKind> {
    hub(world, StationAction::Works(order))
}

fn endgame_events(events: &[GameplayEventKind]) -> Vec<(EndgameEvent, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Endgame { what, detail, .. } => Some((*what, *detail)),
            _ => None,
        })
        .collect()
}

fn saw(events: &[GameplayEventKind], what: EndgameEvent) -> bool {
    endgame_events(events).iter().any(|(seen, _)| *seen == what)
}

fn refused(events: &[GameplayEventKind], why: EndgameRefusal) -> bool {
    endgame_events(events).contains(&(EndgameEvent::Refused, why.code()))
}

fn station_refused(events: &[GameplayEventKind], why: StationRefusal) -> bool {
    events.iter().any(|event| {
        matches!(event, GameplayEventKind::Station { what: StationEvent::Refused, detail, .. }
            if *detail == why.code())
    })
}

fn give(world: &mut WorldState, materials: &[(MineralKind, u16)]) {
    let rig = world.player_mut(PLAYER).expect("rig");
    for (mineral, units) in materials {
        rig.cargo_bay.add(*mineral, u32::from(*units));
    }
}

fn credit(world: &mut WorldState, credits: u32) {
    world.player_mut(PLAYER).expect("rig").credits = credits;
}

/// The rig put straight into space at `pos`, still, turned `orientation`.
fn launch_to(world: &mut WorldState, pos: WorldPos, orientation: f32) {
    let park = world.space_park().expect("park");
    let capacity = world.air_capacity_of(PLAYER);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.flight = RigFlight {
        place: FlightPlace::Space,
        pos,
        air: capacity,
        exit_column: 12,
        ..RigFlight::default()
    };
    rig.attitude = RigAttitude {
        orientation,
        angular_velocity: 0.0,
    };
    rig.x = park.0;
    rig.y = park.1;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn dock_at(world: &mut WorldState, station: StationId) {
    let galaxy = world.space();
    let body = galaxy
        .body(station_ref(&galaxy, station).expect("station ref"))
        .expect("station body");
    let port = body.port().expect("port");
    launch_to(world, port.offset(10.0, 0.0), body.dock_orientation());
    let _ = act(world, PlayerCommand::Flight(FlightCommand::Use));
    assert_eq!(
        world.rig_flight(PLAYER).expect("rig").place,
        FlightPlace::Docked(body.body),
        "docked at {station:?}"
    );
}

/// Every station within `reach` sectors of home on the world seeded `seed`.
fn stations_near(seed: u64, reach: i32) -> Vec<StationProfile> {
    let galaxy = GeneratedSpace::new(crate::space::SpaceSeed::from(seed));
    let space = galaxy.space();
    let mut out = Vec::new();
    for y in -reach..=reach {
        for x in -reach..=reach {
            let sector = space.sector(SpaceSector::new(x, y));
            for system in &sector.systems {
                for station in &system.stations {
                    out.push(profile_of(space.seed(), system, station));
                }
            }
        }
    }
    out
}

/// A people-run shipyard of every tier near home (it builds the Starwright line).
fn great_yard() -> StationProfile {
    static YARD: OnceLock<StationProfile> = OnceLock::new();
    YARD.get_or_init(|| {
        stations_near(SEED, 4)
            .into_iter()
            .find(crate::endgame::works::builds_starwright)
            .expect("a tier-5 human shipyard near home")
    })
    .clone()
}

fn fit(world: &mut WorldState, line: ShipLine) -> Vec<GameplayEventKind> {
    hub(world, StationAction::Fit(ShipUpgrade::Line(line)))
}

#[test]
fn a_belt_rock_gives_its_space_material_and_the_career_counts_it() {
    let mut world = world_of(SEED);
    // A first tick primes the milestones (silently).
    run(&mut world, 1);
    let generated = GeneratedSpace::new(crate::space::SpaceSeed::from(SEED));
    let empty = crate::flight::systems::SystemsShared::default();
    let mut rock: Option<FlightBody> = None;
    'search: for reach in 0..4_i32 {
        for y in -reach..=reach {
            for x in -reach..=reach {
                for system in &generated.space().sector(SpaceSector::new(x, y)).systems {
                    for belt in &system.belts {
                        let mid = i64::from(belt.inner / 2 + belt.outer / 2);
                        let at = system.centre.offset(mid, 0).pos();
                        if let Some(found) = generated
                            .rocks_near(at, 6_000, &empty)
                            .into_iter()
                            .find(|found| found.radius >= tiles(20.0))
                        {
                            rock = Some(found);
                            break 'search;
                        }
                    }
                }
            }
        }
    }
    let rock = rock.expect("a belt rock near home");
    let BodyExtra::Rock(facts) = rock.extra else {
        unreachable!()
    };
    let pos = rock.centre.offset(0.0, -(rock.radius + RIG_CONTACT + 1.0));
    launch_to(&mut world, pos, up_angle(0.0, -1.0));
    let events = run(&mut world, 90);
    let mined = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                GameplayEventKind::SpaceSystems {
                    what: SystemsEvent::OreMined,
                    ..
                }
            )
        })
        .count();
    assert!(mined >= 2, "the drill bit ({mined})");
    let material = crate::endgame::materials::belt_material(facts.ore);
    assert!(is_space_material(material));
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.cargo_bay.mineral(material) >= 2,
        "{material:?} in the bay"
    );
    for home in crate::worldgen::deposits::MINERALS {
        assert_eq!(rig.cargo_bay.mineral(home), 0, "no home stand-ins");
    }
    let career = world.career(PLAYER);
    assert_eq!(usize::try_from(career.mined).expect("count"), mined);
    let index = SPACE_MATERIALS
        .iter()
        .position(|each| *each == material)
        .expect("listed");
    assert_ne!(career.catalogue & (1 << index), 0, "catalogued");
    assert!(events.iter().any(|event| matches!(
        event,
        GameplayEventKind::MilestoneEarned {
            milestone: Milestone::SpaceMiner,
            ..
        }
    )));
    assert!(world.completion(PLAYER).per_mille() > 0);
}

#[test]
fn a_late_rig_buys_its_way_to_tier_two_but_tier_three_waits_for_belt_ore() {
    let mut world = world_of(SEED);
    let yard = great_yard();
    dock_at(&mut world, yard.id);
    // The Descent probe's purse leaving the Heart.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a purse of some millions"
    )]
    let purse = crate::endgame::economy::late_purse().purse as u32;
    credit(&mut world, purse);
    for line in ShipLine::ALL {
        for _ in 0..5 {
            let _ = fit(&mut world, line);
        }
    }
    let gear = world.ship_gear(PLAYER);
    for line in ShipLine::ALL {
        assert_eq!(gear.line(line), 2, "{line:?}: credits alone stop at tier 2");
    }
    // A head start (Wave 14 finish: space pays twice what it did near home, so the purse
    // buys a little less of it; about two fifths are left).
    let left = world.player(PLAYER).expect("rig").credits;
    assert!(left > purse / 3, "the purse is a head start ({left} left)");
    let events = fit(&mut world, ShipLine::Engine);
    assert!(station_refused(&events, StationRefusal::Materials));
    // With the bill's belt ore aboard, tier 3 fits and the ore is spent.
    let bill = line_bill(ShipLine::Engine, 3);
    give(&mut world, bill);
    let events = fit(&mut world, ShipLine::Engine);
    assert!(
        events.iter().any(|event| matches!(
            event,
            GameplayEventKind::Station {
                what: StationEvent::ShipUpgraded,
                detail: 3,
                ..
            }
        )),
        "{events:?}"
    );
    let rig = world.player(PLAYER).expect("rig");
    for (mineral, _) in bill {
        assert_eq!(rig.cargo_bay.mineral(*mineral), 0, "{mineral:?} spent");
    }
    // Common materials can be bought at the yard's market, at three times their value.
    if let Some((mineral, price)) = station_stock(&yard).first().copied() {
        let before = world.player(PLAYER).expect("rig").credits;
        let events = works(&mut world, WorkOrder::BuyMaterial(mineral, 10));
        assert!(saw(&events, EndgameEvent::MaterialsBought), "{events:?}");
        let rig = world.player(PLAYER).expect("rig");
        assert_eq!(rig.cargo_bay.mineral(mineral), 10);
        assert_eq!(rig.credits, before - 10 * price);
        assert_eq!(price, mineral.value() * 3);
    }
    // Rare materials are never for sale.
    let events = works(
        &mut world,
        WorkOrder::BuyMaterial(MineralKind::Voidglass, 1),
    );
    assert!(refused(&events, EndgameRefusal::NotHere));
}

#[test]
fn the_starwright_ship_and_the_heartline_drive() {
    let mut world = world_of(SEED);
    let yard = great_yard();
    dock_at(&mut world, yard.id);
    credit(&mut world, 200_000_000);
    // Locked until the hull line is at tier 5.
    let events = works(&mut world, WorkOrder::Starwright(StarwrightPart::Hull));
    assert!(refused(&events, EndgameRefusal::Locked), "{events:?}");
    for line in [ShipLine::Hull, ShipLine::Engine] {
        for tier in 1..=5 {
            give(&mut world, line_bill(line, tier));
            let _ = fit(&mut world, line);
        }
        assert_eq!(world.ship_gear(PLAYER).line(line), 5);
    }
    let before = world.ship_effects(PLAYER);
    // Locked behind its materials next.
    let events = works(&mut world, WorkOrder::Starwright(StarwrightPart::Hull));
    assert!(refused(&events, EndgameRefusal::Materials), "{events:?}");
    for part in StarwrightPart::ALL {
        give(&mut world, part.bill());
        let events = works(&mut world, WorkOrder::Starwright(part));
        assert!(saw(&events, EndgameEvent::StarwrightFitted), "{events:?}");
    }
    let after = world.ship_effects(PLAYER);
    assert_eq!(after.damage_scale, STARWRIGHT_DAMAGE);
    // The hull softens a fight too (Wave 14 finish).
    assert!((before.combat_scale - 1.0).abs() < f32::EPSILON);
    assert_eq!(
        after.combat_scale,
        crate::endgame::works::STARWRIGHT_COMBAT_DAMAGE
    );
    assert_eq!(after.engine_scale, STARWRIGHT_ENGINE);
    assert!(after.damage_scale < before.damage_scale);
    assert!(after.engine_scale > before.engine_scale);
    assert!(world.career(PLAYER).starwright_ship());
    let events = works(&mut world, WorkOrder::Starwright(StarwrightPart::Hull));
    assert!(refused(&events, EndgameRefusal::Done));
    // The Heartline Drive waits for every faction boss on the hunt list.
    give(&mut world, HEARTLINE_BILL);
    let events = works(&mut world, WorkOrder::Heartline);
    assert!(refused(&events, EndgameRefusal::Locked), "{events:?}");
    let mut hostiles = world.hostiles.clone();
    hostiles.bosses_beaten.extend(Faction::ALL);
    world.set_hostiles(hostiles);
    // ... and three of the theme worlds' bosses (Wave 13 integration: six are registered).
    assert_eq!(world.heartline_theme_bosses(), 3);
    let events = works(&mut world, WorkOrder::Heartline);
    assert!(refused(&events, EndgameRefusal::Locked), "{events:?}");
    let mut output = crate::event::TickOutput::default();
    for boss in &crate::endgame::hunt::THEME_BOSSES[..3] {
        assert!(world.record_theme_boss(boss.theme, Some(PLAYER), &mut output));
    }
    let events = works(&mut world, WorkOrder::Heartline);
    assert!(saw(&events, EndgameEvent::HeartlineFitted), "{events:?}");
    let capstone = world.ship_effects(PLAYER);
    assert_eq!(capstone.range_scale, after.range_scale * 2.0);
    assert!(capstone.spool_millis < after.spool_millis);
    // The ship's damage share is what flight takes: a rock bump hurts less.
    assert!(world.career(PLAYER).heartline);
}

#[test]
fn alien_tech_marks_are_talked_through_in_the_stations_tongue() {
    let mut world = world_of(SEED);
    let alien = stations_near(SEED, 3)
        .into_iter()
        .find(|station| matches!(station.owner, StationOwner::Alien(_)) && station.alien_tech)
        .expect("an alien-run station near home");
    let StationOwner::Alien(kin) = alien.owner else {
        unreachable!()
    };
    dock_at(&mut world, alien.id);
    credit(&mut world, 100_000_000);
    // A mark needs Mk I first.
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(refused(&events, EndgameRefusal::Locked));
    let _ = hub(
        &mut world,
        StationAction::Fit(ShipUpgrade::Alien(AlienTech::XenoLens)),
    );
    assert_eq!(world.ship_effects(PLAYER).lens, 2.0);
    // Mk II: only once the kin's tongue reads Conversant.
    give(&mut world, mark_bill(AlienTech::XenoLens, 2));
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(refused(&events, EndgameRefusal::Locked), "{events:?}");
    world.aliens.log_mut(PLAYER, kin.species()).understanding =
        crate::alien::language::Fluency::Conversant.threshold();
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(
        endgame_events(&events).contains(&(EndgameEvent::MarkRaised, 2)),
        "{events:?}"
    );
    assert_eq!(world.ship_effects(PLAYER).lens, 2.5);
    // Mk III: Fluent.
    give(&mut world, mark_bill(AlienTech::XenoLens, 3));
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(refused(&events, EndgameRefusal::Locked));
    world.aliens.log_mut(PLAYER, kin.species()).understanding =
        crate::alien::language::Fluency::Fluent.threshold();
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(endgame_events(&events).contains(&(EndgameEvent::MarkRaised, MARK_MAX.into())));
    assert_eq!(world.ship_effects(PLAYER).lens, 3.0);
    assert_eq!(world.alien_tech_mark(PLAYER, AlienTech::XenoLens), MARK_MAX);
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(refused(&events, EndgameRefusal::Done));
    // A human yard does not raise marks.
    dock_at(&mut world, HALCYON);
    let events = works(&mut world, WorkOrder::AlienMark(AlienTech::XenoLens));
    assert!(refused(&events, EndgameRefusal::NotHere));
}

#[test]
fn the_halcyon_gate_is_built_part_by_part_and_rides_to_known_systems() {
    let mut world = world_of(SEED);
    dock_at(&mut world, HALCYON);
    credit(&mut world, 200_000_000);
    // The keystone goes in last.
    give(&mut world, gate_part_bill(GATE_PARTS - 1));
    let events = works(&mut world, WorkOrder::GatePart(GATE_PARTS - 1));
    assert!(refused(&events, EndgameRefusal::Locked), "{events:?}");
    // Not finished: no ride.
    let target = crate::flight::space::HOME_SYSTEM;
    let events = works(&mut world, WorkOrder::GateJump(target));
    assert!(refused(&events, EndgameRefusal::NoRoute));
    for part in 0..GATE_PARTS {
        give(&mut world, gate_part_bill(part));
        let events = works(&mut world, WorkOrder::GatePart(part));
        assert!(
            endgame_events(&events).contains(&(EndgameEvent::GatePartDelivered, part.into())),
            "part {part}: {events:?}"
        );
        if part + 1 == GATE_PARTS {
            assert!(saw(&events, EndgameEvent::GateFinished));
        }
    }
    assert!(world.endgame().gate_open());
    let events = works(&mut world, WorkOrder::GatePart(0));
    assert!(refused(&events, EndgameRefusal::Done));
    // Only to a system the crew knows.
    let galaxy = world.space();
    let far = galaxy
        .systems_near(
            galaxy
                .system_at((3, 0))
                .map_or_else(|| WorldPos::tile_center(0, 0), |system| system.star),
            0,
        )
        .into_iter()
        .next()
        .expect("a system three sectors out");
    let events = works(&mut world, WorkOrder::GateJump(far.system));
    assert!(refused(&events, EndgameRefusal::NoRoute), "unknown");
    let _ = world.flight.found(far.system);
    let events = works(&mut world, WorkOrder::GateJump(far.system));
    assert!(saw(&events, EndgameEvent::GateJumped), "{events:?}");
    assert!(matches!(
        world.rig_flight(PLAYER).expect("rig").drive,
        StarDrive::Transit(_)
    ));
    // Six seconds of streak, then free flight near the target's star (outside its approach).
    run(&mut world, 30 * 7);
    let flight = world.rig_flight(PLAYER).expect("rig");
    assert_eq!(flight.place, FlightPlace::Space);
    assert!(!matches!(flight.drive, StarDrive::Transit(_)));
    let distance = flight.pos.span(far.star);
    assert!(
        distance < tiles(120_000.0),
        "arrived {} tiles from the star",
        distance / tiles(1.0)
    );
}

#[test]
fn the_outfitter_sells_space_paints_once() {
    let mut world = world_of(SEED);
    dock_at(&mut world, HALCYON);
    let (item, price) = OUTFITTER[0];
    let events = works(&mut world, WorkOrder::Cosmetic(item));
    assert!(refused(&events, EndgameRefusal::Credits));
    credit(&mut world, price);
    let events = works(&mut world, WorkOrder::Cosmetic(item));
    assert!(saw(&events, EndgameEvent::CosmeticBought), "{events:?}");
    let rig = world.player(PLAYER).expect("rig");
    assert!(rig.cosmetic_unlocks.has(item));
    assert_eq!(rig.credits, 0);
    let events = works(&mut world, WorkOrder::Cosmetic(item));
    assert!(refused(&events, EndgameRefusal::Done));
    // Space paints are never village vendor stock.
    assert!(!item.is_vendor_stock());
}

#[test]
#[allow(clippy::too_many_lines, reason = "the estate's whole story in order")]
fn estates_grow_on_alien_worlds_and_the_neighbours_call_once_a_day() {
    const ALIEN_SEED: u64 = 0xA11E_0012;
    let mut world = world_of(ALIEN_SEED);
    let species = AlienSpecies::Vessi;
    let id: SpaceBodyKey = first_home(ALIEN_SEED, species).expect("a first home");
    let params = crate::place::planet_params_for(crate::space::SpaceSeed::from(ALIEN_SEED), id)
        .expect("landable");
    let found: Arc<town::AlienTown> = Arc::clone(&town::towns_of(&params)[0]);
    let plot = found
        .buildings
        .iter()
        .find(|building| building.kind == town::AlienBuildingKind::HomePlot)
        .copied()
        .expect("a plot");
    // A finished home of the player's in the town (bought and built as the aliens track's
    // tests show).
    world.aliens.homes.push(AlienHome {
        owner: PLAYER,
        town: found.id,
        body: id,
        plot: plot.index,
        species,
        builder: found
            .keeper(town::BUILDER)
            .map_or_default(|character| character.id),
        paid: 0,
        bought_millis: 0,
        ready_millis: 0,
        home: Some(crate::home::Home::built(
            crate::home::blueprint::Blueprint::Cottage,
            3,
            None,
            0,
        )),
    });
    world
        .move_to_place(
            PLAYER,
            Place::OnBody(BodyId::Space(id)),
            strip_position(
                f64::from(plot.door_column()) + 0.5,
                f64::from(found.ground_row()) - 1.0,
            ),
        )
        .expect("on the body");
    world.player_mut(PLAYER).expect("rig").flight.air = 1.0e7;
    run(&mut world, 3);
    let expand = AlienCommand::ExpandHome {
        town: found.id,
        plot: plot.index,
    };
    let call = AlienCommand::CallNeighbours {
        town: found.id,
        plot: plot.index,
    };
    let rooms = |world: &WorldState| {
        world
            .aliens()
            .home_at(found.id, plot.index)
            .and_then(|home| home.home.as_ref())
            .map_or(0, |home| home.rooms.len())
    };
    let before = rooms(&world);
    // No credits yet.
    let events = act(&mut world, PlayerCommand::Alien(expand));
    assert!(refused(&events, EndgameRefusal::Credits), "{events:?}");
    let (wing, bill) = world.estate_quote(found.id, plot.index).expect("a wing");
    credit(&mut world, bill.credits);
    give(&mut world, bill.materials);
    let events = act(&mut world, PlayerCommand::Alien(expand));
    assert!(
        endgame_events(&events).contains(&(EndgameEvent::EstateExpanded, 1)),
        "{events:?}"
    );
    assert_eq!(rooms(&world), before + 1, "{wing:?} raised");
    assert_eq!(
        world.endgame().estate(found.id, plot.index).expansions,
        vec![wing]
    );
    // The neighbours call: a gift, a few words, then not again today.
    let understanding = world.understanding(PLAYER, species);
    let cargo = world.player(PLAYER).expect("rig").cargo_used();
    let events = act(&mut world, PlayerCommand::Alien(call));
    assert!(saw(&events, EndgameEvent::NeighboursCalled), "{events:?}");
    assert!(
        world.player(PLAYER).expect("rig").cargo_used() > cargo,
        "a gift"
    );
    assert!(world.understanding(PLAYER, species) > understanding);
    let events = act(&mut world, PlayerCommand::Alien(call));
    assert!(refused(&events, EndgameRefusal::AlreadyToday));
    let hz = u64::from(world.timing().rate.hz());
    let day = u64::from(crate::people::clock::DEFAULT_DAY_LENGTH_MINUTES) * 60 * hz;
    let next = world.simulation_tick().get() + day;
    world.set_simulation_tick(crate::ids::SimulationTick::new(next));
    run(&mut world, 2);
    let events = act(&mut world, PlayerCommand::Alien(call));
    assert!(saw(&events, EndgameEvent::NeighboursCalled), "the next day");
    // At the town's middle (by the builder's workshop, where the Homes page is opened) the
    // order is taken: refused only for the purse, not for the place.
    world
        .move_to_place(
            PLAYER,
            Place::OnBody(BodyId::Space(id)),
            strip_position(
                f64::from(found.site.centre) + 0.5,
                f64::from(found.ground_row()) - 1.0,
            ),
        )
        .expect("on the body");
    run(&mut world, 2);
    world.player_mut(PLAYER).expect("rig").credits = 0;
    let events = act(&mut world, PlayerCommand::Alien(expand));
    assert!(refused(&events, EndgameRefusal::Credits), "{events:?}");
    // Far from the town, nothing happens.
    let width = found.strip_width();
    let far = (plot.door_column() + width / 2).rem_euclid(width);
    world
        .move_to_place(
            PLAYER,
            Place::OnBody(BodyId::Space(id)),
            strip_position(f64::from(far) + 0.5, f64::from(found.ground_row()) - 40.0),
        )
        .expect("on the body");
    run(&mut world, 2);
    let events = act(&mut world, PlayerCommand::Alien(expand));
    assert!(refused(&events, EndgameRefusal::NotHere), "{events:?}");
    // The home and the estate count: the achievement and the completion.
    assert!(world.owns_alien_home(PLAYER));
    assert!(world.space_facts(PLAYER).alien_home);
}

#[test]
fn landing_on_worlds_counts_toward_completion_and_the_achievements() {
    let mut world = world_of(SEED);
    run(&mut world, 1);
    let galaxy = world.space();
    let worlds: Vec<SpaceBodyKey> = (0..3_i32)
        .flat_map(|x| {
            let coords = SpaceSector::new(x, 1);
            galaxy
                .space()
                .sector(coords)
                .systems
                .iter()
                .flat_map(|system| {
                    system
                        .planets()
                        .map(|body| SpaceBodyKey {
                            sector: coords,
                            system: system.index,
                            body: body.index,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .filter(|key| {
            crate::place::planet_params_for(crate::space::SpaceSeed::from(SEED), *key).is_some()
        })
        .take(2)
        .collect();
    assert_eq!(worlds.len(), 2, "two worlds near home");
    let mut landed_events = 0;
    for key in &worlds {
        let body = galaxy.body_ref(*key).expect("a body ref");
        world.player_mut(PLAYER).expect("rig").flight.place = FlightPlace::Landed(body);
        let events = run(&mut world, 1);
        landed_events += endgame_events(&events)
            .iter()
            .filter(|(what, _)| *what == EndgameEvent::WorldLanded)
            .count();
        // Once a world: standing there longer counts nothing new.
        let again = run(&mut world, 2);
        assert!(!saw(&again, EndgameEvent::WorldLanded));
        if key == &worlds[0] {
            assert!(
                events.iter().any(|event| matches!(
                    event,
                    GameplayEventKind::MilestoneEarned {
                        milestone: Milestone::FirstWorld,
                        ..
                    }
                )),
                "{events:?}"
            );
        }
    }
    assert_eq!(landed_events, 2);
    let career = world.career(PLAYER);
    assert_eq!(career.landed.len(), 2);
    assert!(!career.themes.is_empty());
    let completion = world.completion(PLAYER);
    let line = completion
        .categories
        .iter()
        .find(|category| category.name == "Worlds landed")
        .expect("a worlds line");
    assert_eq!(line.done, 2);
    assert!(!completion.complete());
}

#[test]
fn the_hunt_list_names_every_boss_and_the_theme_seam_records_theirs() {
    let mut world = world_of(SEED);
    let hunt = world.hunt();
    assert_eq!(
        hunt.iter()
            .filter(|(quarry, ..)| matches!(quarry, Quarry::Faction(_)))
            .count(),
        3
    );
    assert!(hunt.iter().all(|(_, beaten, _)| !beaten));
    assert!(
        hunt.iter()
            .all(|(quarry, _, lair)| !matches!(quarry, Quarry::Faction(_)) || lair.is_some()),
        "every faction boss has a lair to point at"
    );
    let mut hostiles = world.hostiles.clone();
    hostiles.bosses_beaten.insert(Faction::Pirates);
    world.set_hostiles(hostiles);
    assert!(world.quarry_beaten(Quarry::Faction(Faction::Pirates)));
    assert_eq!(world.space_facts(PLAYER).faction_bosses, 1);
    let mut output = TickOutput::default();
    assert!(world.record_theme_boss(9, Some(PLAYER), &mut output));
    assert!(!world.record_theme_boss(9, Some(PLAYER), &mut output));
    assert_eq!(output.events.len(), 1);
    assert!(world.quarry_beaten(Quarry::Theme(9)));
}

#[test]
fn the_endgame_state_saves_replicates_and_stays_out_of_old_worlds_hashes() {
    let mut world = world_of(SEED);
    let fresh_hash = world.simulation_state_hash();
    assert!(world.endgame().is_empty());
    let mut same = world_of(SEED);
    same.set_endgame(crate::endgame::EndgameState::default());
    assert_eq!(
        same.simulation_state_hash(),
        fresh_hash,
        "an empty endgame is not hashed"
    );
    dock_at(&mut world, HALCYON);
    credit(&mut world, 200_000_000);
    give(&mut world, gate_part_bill(0));
    let _ = works(&mut world, WorkOrder::GatePart(0));
    assert!(world.endgame().gate_part(0));
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.endgame(), world.endgame());
    assert!(SectionKey::SHARED.contains(&SectionKey::Endgame));
    let mut replica = world_of(SEED);
    let payload = world.section(SectionKey::Endgame).expect("section");
    replica.apply_section(SectionKey::Endgame, payload);
    assert_eq!(replica.endgame(), world.endgame());
    let bytes = postcard::to_allocvec(world.endgame()).expect("postcard");
    let back: crate::endgame::EndgameState = postcard::from_bytes(&bytes).expect("postcard");
    assert_eq!(&back, world.endgame());
}
