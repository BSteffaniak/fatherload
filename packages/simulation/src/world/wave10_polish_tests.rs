//! Wave 10 polish: the open items of Waves 8-9 fixed on Deep Descent worlds. Every deep supply
//! has an effect (ballast blows, flares dazzle, bait draws predators, time bubbles are one item,
//! lamp filter glass wears), the heat warning reads real danger, and the rest of the track's
//! fixes have their tests here too.

use crate::{
    ControlState, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams, TickInputs,
    WorldState,
    depth_progression::{DescentLayout, RigSystem, core_tier_cap},
    descent::{Descent, Stratum},
    enemy::{EnemyDifficulty, EnemyKind},
    explosive::ExplosiveKind,
    far_strange::{FarStrangeAction, LampFilter},
    fluid::FluidKind,
    logistics::LogisticsCommand,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TileKind, TilePosition},
    workshop::{DeepSupply, SupplyEvent},
    worldgen::{GeneratorVersion, TileRect, WorldSettings, descent::layout},
};

const PLAYER: PlayerId = PlayerId::new(0);
const SEED: u64 = 0x0A10_9011;
/// A column far from every set piece (test rooms are built by hand).
const FAR: i32 = -260;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(SEED)
    }
}

/// A V11 world at `hz` whose crew holds the Deep Claim and every gate reward down to the
/// Strange Layers (test worlds below Descent I need them), with a strong, cool rig.
fn deep_world(hz: u32) -> WorldState {
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings(), SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    world.grant_deep_claim();
    let rig = world.player_mut(PLAYER).expect("rig");
    for system in [
        RigSystem::PressureHull,
        RigSystem::BioFilter,
        RigSystem::LampII,
        RigSystem::SonarSuite,
        RigSystem::CryoHull,
        RigSystem::GravityAnchor,
    ] {
        crate::economy::grant_rig_system(rig, system);
    }
    rig.hazard_gear.pressure_rating = 7;
    rig.hazard_gear.heat_shield = 6;
    rig.radiator_level = 19;
    rig.hull_level = 15;
    rig.hull = rig.max_hull();
    world
}

fn middle_row(stratum: Stratum) -> i32 {
    let (top, bottom) = layout(&settings()).rows(stratum);
    top + (bottom - top) / 2
}

fn put_rig(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    world.stream_terrain_near_players();
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn fill(world: &mut WorldState, rect: TileRect, kind: TileKind) {
    for position in rect.positions() {
        let _ = world.terrain_mut().set_kind(position, kind);
    }
}

/// A sealed stone room `half` round `centre`, open inside (flooded when `water`), the rig
/// put in it at `rig`.
fn room(world: &mut WorldState, centre: TilePosition, half: (i32, i32), water: bool) {
    put_rig(world, centre);
    fill(
        world,
        TileRect::around(centre, half.0 + 2, half.1 + 2),
        TileKind::Stone,
    );
    let inside = if water {
        TileKind::Fluid(FluidKind::Water)
    } else {
        TileKind::Air
    };
    fill(world, TileRect::around(centre, half.0, half.1), inside);
    // The first ticks at a new depth pay its reward (a refit and an ore drop beside the rig):
    // let them pass, then clear the room again.
    let _ = ticks(world, 2);
    world
        .entities
        .retain(|entity| !matches!(entity.state, crate::entity::EntityState::Drop(_)));
    fill(world, TileRect::around(centre, half.0, half.1), inside);
}

/// Lets the first ticks at the rig's spot pass (a new depth's reward refits the rig and drops
/// ore beside it), then sweeps the drops away and restores the hull.
fn settle(world: &mut WorldState) {
    let _ = ticks(world, 2);
    world
        .entities
        .retain(|entity| !matches!(entity.state, crate::entity::EntityState::Drop(_)));
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.hull = rig.max_hull();
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn step(world: &mut WorldState, controls: ControlState) -> Vec<GameplayEventKind> {
    let inputs = TickInputs::held(PLAYER, controls);
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.events.into_iter().map(|event| event.kind).collect()
}

fn ticks(world: &mut WorldState, count: u64) -> Vec<GameplayEventKind> {
    (0..count)
        .flat_map(|_| step(world, ControlState::default()))
        .collect()
}

fn supply_events(events: &[GameplayEventKind]) -> Vec<SupplyEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            GameplayEventKind::Supply { what, .. } => Some(*what),
            _ => None,
        })
        .collect()
}

fn act(world: &mut WorldState, command: LogisticsCommand) -> PlayerScopedCommandOutcome {
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::Logistics(command));
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.action_outcomes.first().expect("outcome").outcome
}

fn rig_xy(world: &WorldState) -> (f32, f32) {
    let rig = world.player(PLAYER).expect("rig");
    (rig.x, rig.y)
}

// --- 2. The heat warning reads real danger ---------------------------------------------------

#[test]
fn the_heat_warning_reads_the_heat_past_the_rigs_cooling_not_the_strata_heat() {
    let world = deep_world(30);
    let terrain = world.terrain();
    for descent in [
        Descent::DrownedAndFrozen,
        Descent::HollowWorld,
        Descent::StrangeLayers,
        // Wave 10 integration: the builders' world and the Heart are authored now too.
        Descent::Ancients,
        Descent::Heart,
    ] {
        let cap = core_tier_cap(u8::try_from(descent.index()).expect("index"));
        for stratum in descent.strata() {
            let tile = TilePosition {
                x: FAR,
                y: middle_row(stratum),
            };
            assert!(
                crate::crust::heat_units(terrain, tile) > 0,
                "{stratum:?} runs hot"
            );
            // The Descent's radiator cap and two shields cover it: no danger, no warning.
            assert_eq!(
                crate::crust::heat_danger_units(terrain, tile, cap, 2),
                0,
                "{stratum:?} is covered"
            );
            // A stock rig really is in danger there.
            assert!(
                crate::crust::heat_danger_units(terrain, tile, 1, 0) > 0,
                "{stratum:?} cooks a stock rig"
            );
        }
    }
}

#[test]
fn a_covered_rig_loses_no_hull_where_the_heat_warning_stays_off() {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::TheSunforge),
    };
    room(&mut world, centre, (4, 3), false);
    let cap = core_tier_cap(4);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.radiator_level = cap;
        rig.hazard_gear.heat_shield = 2;
    }
    let tile = world.player(PLAYER).expect("rig").tile_in(world.frame());
    assert_eq!(
        crate::crust::heat_danger_units(world.terrain(), tile, cap, 2),
        0
    );
    let before = world.player(PLAYER).expect("rig").hull;
    let _ = ticks(&mut world, 60);
    let after = world.player(PLAYER).expect("rig").hull;
    assert!(
        (before - after).abs() < 0.01,
        "no heat damage: {before} -> {after}"
    );
}

/// Wave 11 carryover: on foot the HEAT reading is the pilot's suit where the pilot stands, not
/// the parked rig's cooling: a fully cooled rig's pilot still burns in the Sunforge, and the
/// reading (`pilot_heat_danger_levels`, the HUD badge's) agrees with the burn.
#[test]
fn on_foot_the_heat_warning_reads_the_pilots_suit_not_the_rigs_cooling() {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::TheSunforge),
    };
    room(&mut world, centre, (6, 3), false);
    let cap = core_tier_cap(4);
    let _ = pilot_at(
        &mut world,
        TilePosition {
            x: centre.x + 2,
            y: centre.y + 2,
        },
    );
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.radiator_level = cap;
        rig.hazard_gear.heat_shield = 2;
    }
    let (_, body, state) = world.pilot_of(PLAYER).expect("on foot");
    let (mx, my) = state.middle(body);
    let tile = world.frame().tile_at(mx, my);
    let cover = state.gear.heat_cover();
    assert_eq!(
        crate::crust::heat_danger_units(world.terrain(), tile, cap, 2),
        0,
        "the rig's cooling covers it"
    );
    assert!(
        crate::crust::pilot_heat_danger_levels(world.terrain(), tile, cover) > 0,
        "a bare suit does not"
    );
    let before = world.pilot_of(PLAYER).expect("on foot").2.health;
    let _ = ticks(&mut world, 30);
    let after = world.pilot_of(PLAYER).expect("on foot").2.health;
    assert!(after < before, "and the pilot burns: {before} -> {after}");
    // A suit woven against it reads no danger, and does not burn.
    let needed = crate::crust::pilot_heat_danger_levels(world.terrain(), tile, 0);
    assert_eq!(
        crate::crust::pilot_heat_danger_levels(world.terrain(), tile, needed),
        0
    );
}

// --- 6. The deep settlements build their own ---------------------------------------------------

#[test]
fn merrow_city_the_sunfolk_kraal_and_parallax_station_build_in_their_own_styles() {
    use crate::village::{self, BuildingStyle, placement::underground_id};
    for seed in [0, 5, SEED] {
        let settings = WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(seed)
        };
        for (site, style) in [
            (crate::drowned::MERROW_SITE, BuildingStyle::Coral),
            (
                crate::worldgen::streamed::villages::SUNFOLK_SITE,
                BuildingStyle::Jungle,
            ),
            (crate::distortion::PARALLAX_SITE, BuildingStyle::Machine),
        ] {
            let found = village::village(&settings, underground_id(site)).expect("built");
            assert_eq!(found.style, style, "seed {seed:#x}: {}", found.name);
        }
        // Gravel Hollow keeps the deep folk's rock-cut halls.
        let hollow = village::village(&settings, underground_id(0)).expect("Gravel Hollow");
        assert_eq!(hollow.style, BuildingStyle::Carved);
    }
}

// --- 1. Every deep supply has an effect -------------------------------------------------------

/// A flooded room in the Underground Ocean with the rig in it at `hull_share` of its hull.
fn submerged_rig(hull_share: f32, tanks: u8) -> WorldState {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::UndergroundOcean),
    };
    room(&mut world, centre, (4, 8), true);
    put_rig(
        &mut world,
        TilePosition {
            x: FAR,
            y: centre.y + 6,
        },
    );
    settle(&mut world);
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hull = rig.max_hull() * hull_share;
    }
    if tanks > 0 {
        let _ = world
            .logistics
            .add_supplies(PLAYER, DeepSupply::BallastTank, tanks);
    }
    world
}

#[test]
fn a_ballast_tank_blows_when_the_sea_crushes_a_sub_and_it_surges_up() {
    let mut tanked = submerged_rig(0.25, 2);
    let mut bare = submerged_rig(0.25, 0);
    let start = rig_xy(&tanked).1;
    let events = ticks(&mut tanked, 45);
    let _ = ticks(&mut bare, 45);
    assert_eq!(
        supply_events(&events)
            .iter()
            .filter(|what| **what == SupplyEvent::BallastBlown)
            .count(),
        1,
        "one tank blows, not the whole kit"
    );
    assert_eq!(tanked.supplies_of(PLAYER, DeepSupply::BallastTank), 1);
    assert!(tanked.ballast_blowing(PLAYER));
    let (risen, still) = (start - rig_xy(&tanked).1, start - rig_xy(&bare).1);
    assert!(
        risen > still + 16.0,
        "the blown sub surged up ({risen} px) past the bare one ({still} px)"
    );
    // A healthy sub keeps its tanks.
    let mut healthy = submerged_rig(0.9, 2);
    let events = ticks(&mut healthy, 45);
    assert!(supply_events(&events).is_empty());
    assert_eq!(healthy.supplies_of(PLAYER, DeepSupply::BallastTank), 2);
}

/// A flooded room in the Pearl Abyss with a lure angler in sight of the rig.
fn angler_room(flares: u8) -> WorldState {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::PearlAbyss),
    };
    room(&mut world, centre, (9, 3), true);
    put_rig(
        &mut world,
        TilePosition {
            x: centre.x - 7,
            y: centre.y + 2,
        },
    );
    settle(&mut world);
    fill(
        &mut world,
        TileRect::around(centre, 9, 3),
        TileKind::Fluid(FluidKind::Water),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let at = world.frame().tile_center(TilePosition {
        x: centre.x + 1,
        y: centre.y + 2,
    });
    let _ = world.spawn_enemy(EnemyKind::LureAngler, at.0, at.1);
    if flares > 0 {
        let _ = world
            .logistics
            .add_supplies(PLAYER, DeepSupply::FlareBuoy, flares);
    }
    world
}

#[test]
fn a_flare_buoy_dazzles_the_lure_anglers_so_they_drag_nobody() {
    let mut flared = angler_room(1);
    let mut bare = angler_room(0);
    let start = rig_xy(&flared).0;
    let events = ticks(&mut flared, 20);
    let _ = ticks(&mut bare, 20);
    assert!(supply_events(&events).contains(&SupplyEvent::FlareLit));
    assert_eq!(flared.supplies_of(PLAYER, DeepSupply::FlareBuoy), 0);
    let (rig_x, rig_y) = rig_xy(&flared);
    assert!(
        flared.flare_dazzles(rig_x, rig_y),
        "the flare lights the rig"
    );
    assert!(
        rig_xy(&bare).0 > start + 2.0,
        "without a flare the lure drags the rig"
    );
    assert!(
        rig_xy(&flared).0 < rig_xy(&bare).0 - 1.0,
        "the dazzled angler drags it less"
    );
    // The flare burns out by world time.
    let _ = ticks(&mut flared, 30 * 46);
    assert!(
        flared
            .supply_effects()
            .lures_of(DeepSupply::FlareBuoy)
            .next()
            .is_none()
    );
}

/// A room in the Megafauna Jungle with a fangrunner hunting the rig.
fn jungle_room(baits: u8) -> WorldState {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::MegafaunaJungle),
    };
    room(&mut world, centre, (14, 3), false);
    put_rig(
        &mut world,
        TilePosition {
            x: centre.x - 10,
            y: centre.y + 2,
        },
    );
    settle(&mut world);
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let at = world.frame().tile_center(TilePosition {
        x: centre.x - 2,
        y: centre.y + 2,
    });
    let _ = world.spawn_enemy(EnemyKind::Fangrunner, at.0, at.1);
    if baits > 0 {
        let _ = world
            .logistics
            .add_supplies(PLAYER, DeepSupply::BeastBait, baits);
    }
    world
}

#[test]
fn beast_bait_draws_a_hunting_predator_off_the_rig() {
    let mut baited = jungle_room(1);
    let mut bare = jungle_room(0);
    let events = ticks(&mut baited, 30 * 8);
    let _ = ticks(&mut bare, 30 * 8);
    assert!(supply_events(&events).contains(&SupplyEvent::BaitThrown));
    assert_eq!(baited.supplies_of(PLAYER, DeepSupply::BeastBait), 0);
    let bait = baited
        .supply_effects()
        .lures_of(DeepSupply::BeastBait)
        .next()
        .expect("bait lies in the world")
        .at;
    let (_, runner) = baited
        .enemies()
        .find(|(_, state)| state.kind == EnemyKind::Fangrunner)
        .expect("the runner");
    assert!(runner.target.is_none(), "it gave up the rig");
    assert_eq!(runner.home, (bait.x, bait.y), "it went for the bait");
    let lost = |world: &WorldState| {
        let rig = world.player(PLAYER).expect("rig");
        rig.max_hull() - rig.hull
    };
    assert!(
        lost(&baited) < lost(&bare),
        "the baited rig was bitten less ({} vs {})",
        lost(&baited),
        lost(&bare)
    );
}

#[test]
fn a_baited_predator_goes_back_to_its_old_home_when_the_bait_is_spent() {
    let mut world = jungle_room(1);
    let runner_home = |world: &WorldState| {
        world
            .enemies()
            .find(|(_, state)| state.kind == EnemyKind::Fangrunner)
            .map(|(_, state)| state.home)
            .expect("the runner")
    };
    let first_home = runner_home(&world);
    let _ = ticks(&mut world, 30 * 8);
    let bait = world
        .supply_effects()
        .lures_of(DeepSupply::BeastBait)
        .next()
        .expect("bait lies in the world")
        .at;
    assert_eq!(runner_home(&world), (bait.x, bait.y), "on the scent");
    assert_ne!(first_home, (bait.x, bait.y));
    assert_eq!(world.supply_effects().baited_homes.len(), 1);
    // The bait lasts 25 s from its throw; give it all of that and a little more.
    let _ = ticks(&mut world, 30 * 26);
    assert!(
        world
            .supply_effects()
            .lures_of(DeepSupply::BeastBait)
            .next()
            .is_none(),
        "the bait is spent"
    );
    assert_eq!(runner_home(&world), first_home, "it went home again");
    assert!(world.supply_effects().baited_homes.is_empty());
}

#[test]
fn time_bubble_bombs_are_the_rigs_time_bubble_charges() {
    let mut world = deep_world(30);
    world.progression.deepest_tile_reached = world.descent_layout().floor_row() - 1;
    let relay = world
        .logistics()
        .outposts
        .values()
        .find(|outpost| outpost.descent == 5)
        .cloned()
        .expect("the Prism Relay");
    put_rig(&mut world, relay.landing());
    world.player_mut(PLAYER).expect("rig").credits = 1_000_000;
    let charges = |world: &WorldState| {
        world
            .player(PLAYER)
            .expect("rig")
            .explosive_count(ExplosiveKind::TimeBubble)
    };
    let before = charges(&world);
    let buy = LogisticsCommand::BuySupply {
        outpost: relay.id,
        supply: DeepSupply::TimeBubbleBomb,
    };
    assert_eq!(act(&mut world, buy), PlayerScopedCommandOutcome::Applied);
    assert_eq!(
        charges(&world),
        before + 1,
        "a charge, thrown like any other"
    );
    assert_eq!(
        world
            .logistics()
            .supplies_of(PLAYER, DeepSupply::TimeBubbleBomb),
        0,
        "never a second item in the kit"
    );
    assert_eq!(
        u32::from(world.supplies_of(PLAYER, DeepSupply::TimeBubbleBomb)),
        charges(&world)
    );
    // An older save's kit bubbles fold into charges.
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::TimeBubbleBomb, 2);
    let _ = ticks(&mut world, 1);
    assert_eq!(charges(&world), before + 3);
    assert_eq!(
        world
            .logistics()
            .supplies_of(PLAYER, DeepSupply::TimeBubbleBomb),
        0
    );
}

#[test]
fn a_tinted_lamp_needs_lamp_filter_glass_and_a_set_wears_out() {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::PrismDesert),
    };
    room(&mut world, centre, (4, 3), false);
    let cycle = LogisticsCommand::FarStrange(FarStrangeAction::CycleFilter);
    // No glass: refused, and the lamp stays white.
    assert!(world.check_logistics(PLAYER, &cycle).is_err());
    let _ = act(&mut world, cycle);
    assert_eq!(world.lamp_filter_of(PLAYER), LampFilter::White);
    // One set: fitted, and the swaps within its life cost nothing more.
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::LampFilter, 1);
    assert_eq!(act(&mut world, cycle), PlayerScopedCommandOutcome::Applied);
    assert_ne!(world.lamp_filter_of(PLAYER), LampFilter::White);
    assert_eq!(world.supplies_of(PLAYER, DeepSupply::LampFilter), 0);
    assert_eq!(act(&mut world, cycle), PlayerScopedCommandOutcome::Applied);
    assert_ne!(world.lamp_filter_of(PLAYER), LampFilter::White);
    // The set wears out: with none left the lamp goes white.
    let now = world.simulation_tick();
    world
        .logistics
        .supply_effects
        .filter_sets
        .insert(PLAYER, now.next());
    let events = ticks(&mut world, 3);
    assert!(supply_events(&events).contains(&SupplyEvent::FilterWornOut));
    assert_eq!(world.lamp_filter_of(PLAYER), LampFilter::White);
}

/// Wave 11 carryover: a lamp an older save tinted before lamps needed glass has no set fitted;
/// it is given one set's life on the next tick and then wears off like any other.
#[test]
fn an_older_saves_tinted_lamp_without_glass_wears_off() {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::PrismDesert),
    };
    room(&mut world, centre, (4, 3), false);
    world.set_lamp_filter(PLAYER, LampFilter::Red);
    assert!(world.supply_effects().filter_sets.is_empty(), "an old save");
    let _ = ticks(&mut world, 1);
    let until = *world
        .supply_effects()
        .filter_sets
        .get(&PLAYER)
        .expect("a set's life is given");
    assert_eq!(world.lamp_filter_of(PLAYER), LampFilter::Red);
    // Run the clock to the end of that set: with no glass in the kit the lamp goes white.
    world
        .logistics
        .supply_effects
        .filter_sets
        .insert(PLAYER, world.simulation_tick().next());
    assert!(until > world.simulation_tick());
    let events = ticks(&mut world, 3);
    assert!(supply_events(&events).contains(&SupplyEvent::FilterWornOut));
    assert_eq!(world.lamp_filter_of(PLAYER), LampFilter::White);
    assert!(world.supply_effects().filter_sets.is_empty());
}

#[test]
fn supplies_at_work_save_replicate_and_hash() {
    let mut world = deep_world(30);
    world.stage_supply_lure(DeepSupply::FlareBuoy, PLAYER, TilePosition { x: 3, y: 900 });
    world.stage_filter_glass(PLAYER);
    let json = serde_json::to_string(&world).expect("save");
    let loaded: WorldState = serde_json::from_str(&json).expect("load");
    assert_eq!(loaded.supply_effects(), world.supply_effects());
    assert_eq!(
        loaded.simulation_state_hash(),
        world.simulation_state_hash()
    );
    // A world with nothing at work hashes differently from one with a flare burning.
    let bare = deep_world(30);
    assert_ne!(bare.simulation_state_hash(), world.simulation_state_hash());
}

// --- 5. The Underground Ocean is one sea -------------------------------------------------------

#[test]
fn the_underground_oceans_basins_are_joined_into_one_sea() {
    use crate::worldgen::descent::{
        drowned::{DrownedStamp, DrownedStampKind, drowned_tile, stamps_with},
        landmarks::outpost_column,
    };
    for seed in [SEED, 0, 7] {
        let settings = WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(seed)
        };
        let layout = layout(&settings);
        let (top, bottom) = layout.rows(Stratum::UndergroundOcean);
        let column = outpost_column(&settings);
        // One side of the outposts' column (the lift's column stays dry between the halves).
        let area = TileRect {
            min: TilePosition {
                x: column + 40,
                y: top,
            },
            max: TilePosition {
                x: column + 640,
                y: bottom,
            },
        };
        let stamps = stamps_with(&settings, &layout, &area);
        let basins: Vec<&DrownedStamp> = stamps
            .iter()
            .filter(|stamp| {
                stamp.kind == DrownedStampKind::Basin && area.contains(stamp.bounds.center())
            })
            .collect();
        assert!(
            basins.len() >= 8,
            "seed {seed}: a sea of basins ({})",
            basins.len()
        );
        // Sail from the first basin's heart (through water and the passages over it): every
        // basin is reached.
        let tile = |at: TilePosition| {
            drowned_tile(
                &settings,
                &layout,
                None,
                &stamps,
                at.x,
                at.y,
                TileKind::Stone,
            )
        };
        let water = |at: TilePosition| tile(at) == Some(TileKind::Fluid(FluidKind::Water));
        let open = |at: TilePosition| water(at) || tile(at) == Some(TileKind::Air);
        let flood = |start: TilePosition, through: &dyn Fn(TilePosition) -> bool| {
            let mut seen = std::collections::BTreeSet::from([start]);
            let mut queue = vec![start];
            while let Some(at) = queue.pop() {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let next = TilePosition {
                        x: at.x + dx,
                        y: at.y + dy,
                    };
                    if area.expanded(40).contains(next) && !seen.contains(&next) && through(next) {
                        seen.insert(next);
                        queue.push(next);
                    }
                }
            }
            seen
        };
        let start = basins
            .iter()
            .map(|basin| basin.bounds.center())
            .find(|centre| water(*centre))
            .expect("a basin's heart is water");
        let seen = flood(start, &open);
        let reached = basins
            .iter()
            .filter(|basin| {
                seen.iter()
                    .any(|at| water(*at) && basin.bounds.contains(*at))
            })
            .count();
        assert!(
            reached * 10 >= basins.len() * 9,
            "seed {seed}: one sea joins {reached} of {} basins",
            basins.len()
        );
        // No water joins two basins: the passages run over the water, so a basin drilled out
        // from below drains alone, never the whole sea down the shaft (which also kept the
        // fluid pass busy for minutes).
        let home = basins
            .iter()
            .find(|basin| basin.bounds.contains(start))
            .expect("the start's basin");
        for at in flood(start, &water) {
            assert!(
                home.bounds.contains(at),
                "seed {seed}: the water of {:?} runs on to {at:?}",
                home.bounds
            );
        }
    }
}

// --- 4. The deep's forces on a pilot on foot ---------------------------------------------------

/// The pilot's position.
fn pilot_xy(world: &WorldState) -> (f32, f32) {
    let (_, body, _) = world.pilot_of(PLAYER).expect("on foot");
    (body.x, body.y)
}

/// Puts the pilot out on foot at the centre of `tile` with the rig parked beside it (the
/// rig's teleport may move the frame, so the pixel is read after it).
fn pilot_at(world: &mut WorldState, tile: TilePosition) -> (f32, f32) {
    put_rig(
        world,
        TilePosition {
            x: tile.x - 3,
            ..tile
        },
    );
    let (x, y) = world.frame().tile_center(tile);
    let _ = world.place_pilot(PLAYER, x, y);
    (x, y)
}

/// A tile of the Underground Ocean where a basin's gyre runs mostly sideways.
fn gyre_tile(world: &WorldState) -> TilePosition {
    let (top, bottom) = layout(&settings()).rows(Stratum::UndergroundOcean);
    for y in (top + 20..bottom - 20).step_by(3) {
        for x in (-900..900).step_by(5) {
            let tile = TilePosition { x, y };
            let (px, py) = world.frame().tile_center(tile);
            let (cx, cy) = world.current_at(px, py);
            if cx.abs() > 30.0 && cx.abs() > cy.abs() * 3.0 {
                return tile;
            }
        }
    }
    panic!("no gyre in the ocean");
}

#[test]
fn a_basins_current_carries_a_swimming_pilot_unless_the_anchor_plants_its_boots() {
    let mut world = deep_world(30);
    let tile = gyre_tile(&world);
    let (x, y) = pilot_at(&mut world, tile);
    let (cx, _) = world.current_at(x, y);
    fill(
        &mut world,
        TileRect::around(tile, 4, 4),
        TileKind::Fluid(FluidKind::Water),
    );
    let start = pilot_xy(&world).0;
    let _ = ticks(&mut world, 30);
    let moved = pilot_xy(&world).0 - start;
    assert!(
        moved * cx.signum() > 4.0,
        "the gyre ({cx} px/s) carried the swimmer ({moved} px)"
    );
    // The anchor, engaged on foot, plants the pilot's boots.
    let anchor = LogisticsCommand::Hollow(crate::hollow::HollowAction::ToggleAnchor);
    assert_eq!(act(&mut world, anchor), PlayerScopedCommandOutcome::Applied);
    assert!(world.pilot_anchored(PLAYER));
    let start = pilot_xy(&world).0;
    let _ = ticks(&mut world, 30);
    assert!(
        (pilot_xy(&world).0 - start).abs() < 1.0,
        "anchored, the current does not move it"
    );
}

#[test]
fn the_inner_skys_wind_carries_an_airborne_pilot() {
    let mut world = deep_world(30);
    let (top, bottom) = layout(&settings()).rows(Stratum::InnerSky);
    let windy = |world: &WorldState, tile: TilePosition| {
        (-2..=2).all(|dy| {
            let (px, py) = world.frame().tile_center(TilePosition {
                x: tile.x,
                y: tile.y + dy,
            });
            world
                .hollow_wind_at(px, py)
                .is_some_and(|wind| wind.abs() > 40.0)
        })
    };
    let tile = (top + 10..bottom - 10)
        .step_by(2)
        .find_map(|y| {
            (-600..600)
                .step_by(7)
                .map(|x| TilePosition { x, y })
                .find(|tile| windy(&world, *tile))
        })
        .expect("a windy stretch of the Inner Sky");
    let (x, y) = pilot_at(&mut world, tile);
    let wind = world.hollow_wind_at(x, y).expect("wind");
    let start = pilot_xy(&world).0;
    let _ = ticks(&mut world, 10);
    let moved = pilot_xy(&world).0 - start;
    assert!(
        moved * wind.signum() > 3.0,
        "the wind ({wind} px/s) carried the falling pilot ({moved} px)"
    );
}

#[test]
fn an_erupting_geyser_throws_a_pilot_up_its_shaft() {
    use crate::{
        drowned::{GeyserPhase, geyser_phase},
        worldgen::descent::drowned::{DrownedStampKind, stamps_with},
    };
    let settings = settings();
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::GeyserFields);
    let area = TileRect {
        min: TilePosition { x: -900, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    let chimney = stamps_with(&settings, &layout, &area)
        .into_iter()
        .find(|stamp| stamp.kind == DrownedStampKind::Chimney)
        .expect("a chimney");
    let (vent_row, left, right) = chimney.vents().expect("its vents");
    let column = i32::midpoint(left, right);
    let mut world = deep_world(30);
    let _ = pilot_at(
        &mut world,
        TilePosition {
            x: column,
            y: vent_row - 4,
        },
    );
    // Wait for the vent to blow.
    let mut thrown = false;
    for _ in 0..30 * 20 {
        let _ = ticks(&mut world, 1);
        let now = world.millis_at(world.simulation_tick());
        if geyser_phase(settings.seed, column, false, now) == GeyserPhase::Erupting {
            let (_, body, _) = world.pilot_of(PLAYER).expect("on foot");
            if body.velocity_y < -200.0 {
                thrown = true;
                break;
            }
        }
    }
    assert!(thrown, "the jet threw the pilot up");
}

#[test]
fn the_glaciers_cold_slows_a_pilots_walk_but_not_with_the_cryo_hull() {
    let mut world = deep_world(30);
    let centre = TilePosition {
        x: FAR,
        y: middle_row(Stratum::BrineGlaciers),
    };
    room(&mut world, centre, (14, 3), false);
    let floor = world.frame().tile_center(TilePosition {
        x: centre.x - 10,
        y: centre.y + 3,
    });
    let walk = |world: &mut WorldState| {
        let _ = world.place_pilot(PLAYER, floor.0, floor.1);
        let _ = ticks(world, 10);
        let start = pilot_xy(world).0;
        let inputs = TickInputs::held(PLAYER, ControlState::new(1.0, false, false));
        for _ in 0..45 {
            let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        }
        pilot_xy(world).0 - start
    };
    // With the Cryo Hull the suit is warm.
    assert!(world.pilot_chill(PLAYER, floor.0, floor.1).abs() < f32::EPSILON);
    let warm = walk(&mut world);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .hazard_gear
        .rig_systems &= !RigSystem::CryoHull.bit();
    let chill = world.pilot_chill(PLAYER, floor.0, floor.1);
    assert!(chill > 0.1, "the cold bites ({chill})");
    let cold = walk(&mut world);
    assert!(
        cold < warm * 0.95,
        "the cold pilot walked {cold} px, the warm one {warm} px"
    );
}

/// Review fix: a walker's hit ellipse rises over its feet as the game draws it, so a shot
/// through its upper body lands and a shot through the rock under its feet misses (the shot's
/// offset from the body's centre, not the other way round).
#[test]
fn a_walkers_hit_area_rises_over_its_feet_as_it_is_drawn() {
    let mut checked = 0;
    for kind in EnemyKind::ALL {
        let area = kind.hit_area();
        let (half_w, half_h) = area.half;
        // Walkers whose drawn body clearly rises over the centre.
        if area.centre_dy > -0.2 * half_h || half_h <= 0.0 {
            continue;
        }
        let centre = (0.0_f32, 0.0_f32);
        let across = |y: f32| {
            super::weapons::segment_hits_area(centre, &area, (-half_w * 3.0, y), (half_w * 3.0, y))
        };
        // Through the upper drawn body.
        let upper = half_h.mul_add(-0.8, area.centre_dy);
        assert!(across(upper), "{kind:?}: a shot at {upper} misses its head");
        // The mirror of it, under the feet (past the terrain circle too).
        let under = -upper;
        if under > area.radius {
            assert!(
                !across(under),
                "{kind:?}: a shot at {under} hits under its feet"
            );
        }
        assert!(area.contains(0.0, upper), "{kind:?}");
        checked += 1;
    }
    assert!(checked >= 5, "only {checked} tall walkers");
}

// --- Wave 11 carryover: the supply line (deep expeditions strain the tank) ---------------------

/// The fuel a rig at `tile` burns thrusting up for a second, its tank full.
fn thrust_a_second(world: &mut WorldState, tile: TilePosition) -> f32 {
    room(world, tile, (3, 6), false);
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.fuel = rig.fuel_capacity;
    let before = rig.fuel;
    for _ in 0..30 {
        let _ = step(world, ControlState::new(0.0, true, false));
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.velocity_y = 0.0;
    }
    before - world.player(PLAYER).expect("rig").fuel
}

/// Wave 11 carryover (§13 "Making a long dig fun"): far below every supply stop a second of
/// thrust also costs a share of the tank, so the long climb home from a new Descent's bottom is
/// stressful whatever the tier; a beacon planted above (an everyday trip's stop) takes the
/// strain away. The Crust and older worlds never feel it.
#[test]
fn far_below_every_supply_stop_thrust_strains_the_tank_and_a_beacon_eases_it() {
    use crate::depth_progression::{SUPPLY_FREE_ROWS, SUPPLY_STRAIN_MAX, supply_strain};
    let mut world = deep_world(30);
    let settings = settings();
    // The spot of the Deep Descent furthest below every stop (no beacons planted yet).
    let top = layout(&settings).rows(Stratum::CrystalFaults).0;
    let bottom = layout(&settings).rows(Stratum::TheHeart).1;
    let row = (top..bottom)
        .step_by(4)
        .max_by(|a, b| {
            let strain = |row: i32| world.logistics().supply_strain_at(&settings, row);
            strain(*a).total_cmp(&strain(*b))
        })
        .expect("rows");
    assert!(
        world.logistics().supply_strain_at(&settings, row) > SUPPLY_STRAIN_MAX * 0.5,
        "somewhere far below the stops"
    );
    let tile = TilePosition { x: FAR, y: row };
    let strain = world.logistics().supply_strain_at(&settings, row);
    assert!(strain > 0.0);
    let strained = thrust_a_second(&mut world, tile);
    let at = world.player(PLAYER).expect("rig").tile_in(world.frame());
    assert!(
        world.supply_strain_for(PLAYER) > 0.0,
        "row {row} rig {at:?} stop {} strain {strain}",
        world.logistics().supply_row_above(at.y)
    );
    // A beacon a few rows over the rig: the stop is right here now.
    let _ = world.logistics.beacons.insert(
        9_999,
        crate::logistics::DepthBeacon {
            id: 9_999,
            owner: PLAYER,
            at: TilePosition {
                x: FAR,
                y: row - 20,
            },
        },
    );
    assert!(world.supply_strain_for(PLAYER).abs() < f32::EPSILON);
    let calm = thrust_a_second(&mut world, tile);
    let capacity = world.player(PLAYER).expect("rig").fuel_capacity;
    let extra = strained - calm;
    assert!(
        (extra - strain * capacity).abs() <= (strain * capacity).mul_add(0.1, 0.5),
        "a second far out costs {extra} more (strain {strain} of {capacity})"
    );
    // The Heart's chamber is a destination: no strain in the final fight's arena, though the
    // open deep around it is far below Heart's Edge.
    let heart = world.heart_centre().expect("the Heart");
    let floor = TilePosition {
        x: heart.x,
        y: heart.y + 10,
    };
    put_rig(&mut world, floor);
    assert!(world.logistics().supply_strain_at(&settings, floor.y) > 0.0);
    assert!(world.supply_strain_for(PLAYER).abs() < f32::EPSILON);
    // Calm within reach of a stop, none in the Crust.
    assert!(supply_strain(SUPPLY_FREE_ROWS, 3).abs() < f32::EPSILON);
    assert!(supply_strain(5_000, 0).abs() < f32::EPSILON);
    assert!(supply_strain(SUPPLY_FREE_ROWS + 500, 4) < supply_strain(SUPPLY_FREE_ROWS + 900, 4));
    assert!((supply_strain(50_000, 6) - SUPPLY_STRAIN_MAX).abs() < f32::EPSILON);
    // Older worlds keep their fuel rules.
    let classic = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::V10,
            ..WorldSettings::new(SEED)
        },
        SimulationParams::new(30),
    );
    assert!(
        classic
            .logistics()
            .supply_strain_at(classic.terrain().settings(), 4_000)
            .abs()
            < f32::EPSILON
    );
}
