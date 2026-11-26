//! Wave 8 progression on Deep Descent worlds: the Pressure Rating and Headlamp ladders, rig
//! systems felt before they are sold, the gate crush's ramp, fair heat in the placeholder
//! strata, the deep shops at the outposts, the workshops and layer resources, and the haul's
//! Descent premium. Classic worlds keep every old rule.

use crate::{
    PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    depth_progression::{
        DEPTH_LINE_MAX_LEVEL, DescentLayout, GATE_DAMAGE_PER_SECOND, ProgressionRules, RigSystem,
        core_tier_cap, deep_ore_value, depth_line_cost, descent_value_percent,
        gate_damage_per_second, headlamp_light_scale, outpost_stocks, placeholder_heat_cap_units,
        pressure_fuel_percent,
    },
    descent::Stratum,
    economy::{UpgradeKind, UpgradeLock, shop_line_count, upgrade_offers_for},
    enemy::EnemyDifficulty,
    entity::{EntityKind, EntityState},
    item::ItemKind,
    logistics::{LogisticsCommand, Outpost},
    position::WorldPos,
    protection::{band_heat_units, heat_cooling, radiator_cooling},
    summary::PlayerScopedCommandOutcome,
    terrain::{MineralKind, TileKind, TilePosition},
    workshop::{CraftOutput, RECIPES, layer_resource_worth, part_credits},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::logistics::LogisticsRefusal;

const PLAYER: PlayerId = PlayerId::new(0);

fn deep_world(seed: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    // The crew holds the Deep Claim (the Sinkhole beaten), or Descent I's gate crushes it below.
    world.grant_deep_claim();
    world
}

/// Stands the rig in a carved pocket at `tile` (a stone floor under it).
fn park(world: &mut WorldState, tile: TilePosition) {
    world.teleport_rig_to(
        PLAYER,
        WorldPos::tile_center(i64::from(tile.x), i64::from(tile.y)),
    );
    for y in tile.y - 2..=tile.y + 1 {
        for x in tile.x - 2..=tile.x + 2 {
            let kind = if y == tile.y + 1 {
                TileKind::Stone
            } else {
                TileKind::Air
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    let rig = world.player_mut(PLAYER).expect("rig");
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
}

fn outpost_of(world: &WorldState, descent: u8) -> Outpost {
    world
        .logistics()
        .outposts
        .values()
        .find(|outpost| outpost.descent == descent)
        .cloned()
        .expect("an outpost in every Descent")
}

fn act(world: &mut WorldState, command: LogisticsCommand) -> PlayerScopedCommandOutcome {
    let inputs = TickInputs::default().act(PLAYER, PlayerCommand::Logistics(command));
    let output = world.simulate_tick(world.simulation_tick(), &inputs);
    output.action_outcomes.first().expect("outcome").outcome
}

fn offer_index(kind: UpgradeKind) -> u8 {
    u8::try_from(
        UpgradeKind::ALL
            .iter()
            .position(|line| *line == kind)
            .expect("line"),
    )
    .expect("index")
}

#[test]
fn the_wave_8_curves_climb_monotonically() {
    let mut last = 0;
    for level in 0..DEPTH_LINE_MAX_LEVEL {
        let pressure = depth_line_cost(true, level);
        let lamp = depth_line_cost(false, level);
        assert!(pressure > last, "pressure tier {level}");
        assert!(lamp < pressure, "the lamp is the cheaper line");
        last = pressure;
        assert!(headlamp_light_scale(level + 1) > headlamp_light_scale(level));
    }
    for rating in 0..DEPTH_LINE_MAX_LEVEL {
        assert_eq!(pressure_fuel_percent(rating, rating), 100, "rated");
        assert!(pressure_fuel_percent(rating, rating + 1) > 100);
        assert!(pressure_fuel_percent(0, rating + 1) >= pressure_fuel_percent(0, rating));
    }
    // Deep ore and layer resources are worth more the deeper they come from.
    for descent in 0..8 {
        for rank in 0..4 {
            assert!(deep_ore_value(descent, rank + 1) > deep_ore_value(descent, rank));
        }
        if descent > 0 {
            assert!(deep_ore_value(descent, 0) > deep_ore_value(descent - 1, 0));
            assert!(descent_value_percent(descent) > descent_value_percent(descent - 1));
        }
    }
    let mut worth = 0;
    for stratum in Stratum::ALL {
        assert!(layer_resource_worth(stratum) > worth);
        worth = layer_resource_worth(stratum);
    }
    // The gate crush ramps up and tops out at the old stub's rate.
    assert!(gate_damage_per_second(0) < gate_damage_per_second(20));
    assert!((gate_damage_per_second(400) - GATE_DAMAGE_PER_SECOND).abs() < 1e-4);
}

#[test]
fn placeholder_strata_below_descent_six_never_cook_a_rig_that_came_through_the_gates() {
    let world = deep_world(0x0FA1_4001);
    let layout = world.descent_layout();
    // Wave 9: Descents IV-VI have their own climate; VII-VIII stay capped placeholders.
    for (index, stratum) in Stratum::ALL.iter().enumerate().skip(32) {
        let span = layout.strata()[index];
        let descent = span.descent;
        let cap = placeholder_heat_cap_units(descent).expect("Descents VII-VIII are capped");
        // The radiator on sale before this Descent opened covers it with no shielding.
        let radiator = core_tier_cap(descent - 1);
        assert!(radiator_cooling(radiator) >= cap, "{stratum:?}");
        let middle = TilePosition {
            x: 30,
            y: i32::midpoint(span.top, span.bottom),
        };
        let band = world.terrain().band_at(middle);
        let excess = band_heat_units(band)
            .saturating_sub(heat_cooling(radiator, 0))
            .min(world.placeholder_heat_excess_cap(middle, radiator, 0));
        assert_eq!(
            excess, 0,
            "{stratum:?} cooks a rig with radiator {radiator}"
        );
        // A stock rig is still warned off, but never more than the cap's worth of burn.
        let stock = world.placeholder_heat_excess_cap(middle, 1, 0);
        assert!(stock <= cap, "{stratum:?}");
    }
    // Descents I-III keep their authored heat (no cap).
    let shallow = TilePosition {
        x: 30,
        y: layout.strata()[11].top + 5,
    };
    assert_eq!(world.placeholder_heat_excess_cap(shallow, 1, 0), u8::MAX);
}

#[test]
fn a_geared_rig_holds_its_hull_in_the_heart() {
    let mut world = deep_world(0x0FA1_4002);
    let layout = world.descent_layout();
    let heart = layout.descent(7).expect("the Heart");
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.radiator_level = core_tier_cap(6);
        rig.hull_level = core_tier_cap(6);
        rig.hazard_gear.rig_systems = u8::MAX;
        rig.hazard_gear.pressure_rating = DEPTH_LINE_MAX_LEVEL;
        rig.hull = rig.max_hull();
    }
    park(
        &mut world,
        TilePosition {
            x: 30,
            y: heart.top + 60,
        },
    );
    let hull = world.player(PLAYER).expect("rig").hull;
    for _ in 0..240 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let after = world.player(PLAYER).expect("rig").hull;
    assert!(
        after >= hull - 1.0,
        "lost {} hull in four seconds",
        hull - after
    );
}

#[test]
fn the_heart_key_is_never_sold_though_its_need_is_felt() {
    let mut world = deep_world(0x0FA1_4003);
    let layout = world.descent_layout();
    // The Heart Key (every gate system is a boss's reward now, Waves 8-10: the Gatekeeper
    // Colossus's): the crew holds the gates above the Heart.
    let hollow = layout.descent(7).expect("Descent VIII");
    let index = offer_index(UpgradeKind::HeartKey);
    world.progression.deepest_tile_reached = hollow.top + 30;
    let offer = upgrade_offers_for(
        world.player(PLAYER).expect("rig"),
        ProgressionRules::Descent,
    )[usize::from(index)];
    let lock = |world: &WorldState| {
        crate::economy::upgrade_lock_for(
            &offer,
            world.world_settings(),
            world.progression.deepest_tile_reached,
            ProgressionRules::Descent,
            world.logistics().felt_systems,
        )
    };
    let won = Some(UpgradeLock::Boss(
        crate::descent::DescentBoss::GatekeeperColossus,
    ));
    assert_eq!(lock(&world), won);
    // A dip past the gate: the crush starts gently and the need is felt.
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear.rig_systems = RigSystem::PressureHull.bit()
            | RigSystem::BioFilter.bit()
            | RigSystem::CryoHull.bit()
            | RigSystem::GravityAnchor.bit()
            | RigSystem::PhaseDrill.bit();
        rig.hazard_gear.pressure_rating = DEPTH_LINE_MAX_LEVEL;
        rig.hull_level = 12;
        rig.radiator_level = 12;
        rig.hazard_gear.heat_shield = 6;
        rig.hull = rig.max_hull();
    }
    park(
        &mut world,
        TilePosition {
            x: 30,
            y: hollow.top + 1,
        },
    );
    let before = world.player(PLAYER).expect("rig").hull;
    for _ in 0..60 {
        let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    }
    let shallow_loss = before - world.player(PLAYER).expect("rig").hull;
    assert!(shallow_loss > 0.0, "the gate bites");
    assert!(
        shallow_loss < GATE_DAMAGE_PER_SECOND * 0.6,
        "a dip is gentle: {shallow_loss}"
    );
    assert_ne!(
        world.logistics().felt_systems & RigSystem::HeartKey.bit(),
        0
    );
    // Felt, and still the Colossus's to give (Wave 10 ancients-b).
    assert_eq!(lock(&world), won);
}

#[test]
fn underrated_seals_burn_more_fuel() {
    let mut world = deep_world(0x0FA1_4004);
    let layout = world.descent_layout();
    let old_deep = layout.descent(1).expect("Descent II");
    let at = TilePosition {
        x: 30,
        y: old_deep.top + 40,
    };
    let burn = |rating: u8| {
        let mut world = world.clone();
        park(&mut world, at);
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.hazard_gear.pressure_rating = rating;
            rig.radiator_level = 10;
            rig.fuel = rig.fuel_capacity;
        }
        let before = world.player(PLAYER).expect("rig").fuel;
        let thrust = crate::ControlState::new(0.0, true, false);
        for _ in 0..30 {
            let inputs = TickInputs::default().hold_as(PLAYER, thrust);
            let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        }
        before - world.player(PLAYER).expect("rig").fuel
    };
    let underrated = burn(0);
    let rated = burn(1);
    assert!(rated > 0.0);
    assert!(
        underrated > rated * 1.25,
        "underrated {underrated} vs rated {rated}"
    );
    // The same rig on a classic world burns what it always did.
    let classic = WorldState::new_game(PLAYER);
    assert!((classic.pressure_burn_factor(PLAYER) - 1.0).abs() < f32::EPSILON);
    world.set_progression_rules(ProgressionRules::Classic);
    assert!((world.pressure_burn_factor(PLAYER) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn every_outpost_stocks_the_tiers_its_descent_needs() {
    let mut world = deep_world(0x0FA1_4005);
    let layout = world.descent_layout();
    // Everyone has been to the bottom: only the counters' stock limits what they sell.
    world.progression.deepest_tile_reached = layout.floor_row() - 1;
    world.logistics.felt_systems = u8::MAX;
    for descent in 0..8_u8 {
        let outpost = outpost_of(&world, descent);
        park(&mut world, outpost.landing());
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.credits = 50_000_000;
            rig.drill_strength = core_tier_cap(descent);
        }
        let drill = LogisticsCommand::OutpostUpgrade {
            outpost: outpost.id,
            offer: offer_index(UpgradeKind::Drill),
        };
        // The next drill tier is past this Descent's cap: a deeper outpost sells it.
        if descent < 7 {
            assert_eq!(
                world.check_logistics(PLAYER, &drill),
                Err(LogisticsRefusal::Locked(UpgradeLock::NotStockedHere)),
                "Descent {descent}"
            );
        }
        world.player_mut(PLAYER).expect("rig").drill_strength = core_tier_cap(descent) - 1;
        let quote = world.check_logistics(PLAYER, &drill).expect("stocked");
        let offer = upgrade_offers_for(
            world.player(PLAYER).expect("rig"),
            ProgressionRules::Descent,
        )[usize::from(offer_index(UpgradeKind::Drill))];
        assert_eq!(
            quote.credits,
            u32::try_from(
                u64::from(offer.cost)
                    * u64::from(crate::depth_progression::outpost_price_percent(
                        descent,
                        false,
                        UpgradeKind::Drill
                    ))
                    / 100
            )
            .expect("price")
        );
        // The ladders for pressure and the lamp stock the tier rated for the next Descent.
        assert!(outpost_stocks(
            descent,
            UpgradeKind::PressureRating,
            descent + 1
        ));
        assert!(!outpost_stocks(
            descent,
            UpgradeKind::PressureRating,
            descent + 2
        ));
        assert!(!outpost_stocks(descent, UpgradeKind::AltitudeIntake, 1));
        // Every rig system is on the counter once felt and reached.
        for system in RigSystem::ALL {
            assert!(outpost_stocks(
                descent,
                UpgradeKind::for_rig_system(system),
                1
            ));
        }
        let credits = world.player(PLAYER).expect("rig").credits;
        assert_eq!(act(&mut world, drill), PlayerScopedCommandOutcome::Applied);
        let rig = world.player(PLAYER).expect("rig");
        assert_eq!(rig.drill_strength, core_tier_cap(descent));
        assert_eq!(rig.credits, credits - quote.credits);
    }
    // Outside an outpost the counter is shut.
    park(
        &mut world,
        TilePosition {
            x: 10,
            y: layout.descent(1).expect("II").top + 100,
        },
    );
    let outpost = outpost_of(&world, 1);
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::OutpostUpgrade {
                outpost: outpost.id,
                offer: 0
            }
        ),
        Err(LogisticsRefusal::NotAtOutpost)
    );
    assert_eq!(
        shop_line_count(ProgressionRules::Descent),
        UpgradeKind::ALL.len()
    );
}

#[test]
fn workshops_turn_layer_resources_into_parts_and_supplies() {
    let mut world = deep_world(0x0FA1_4006);
    world.progression.deepest_tile_reached = world.descent_layout().floor_row() - 1;
    let crust = outpost_of(&world, 0);
    park(&mut world, crust.landing());
    let fins = u8::try_from(
        RECIPES
            .iter()
            .position(|recipe| recipe.output == CraftOutput::Part(UpgradeKind::Radiator))
            .expect("radiator fins"),
    )
    .expect("index");
    let craft = LogisticsCommand::Craft {
        outpost: crust.id,
        recipe: fins,
    };
    world.player_mut(PLAYER).expect("rig").credits = 100_000;
    assert_eq!(
        world.check_logistics(PLAYER, &craft),
        Err(LogisticsRefusal::MissingResource {
            stratum: Stratum::ClayBeds,
            need: 6
        })
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(ItemKind::Layer(Stratum::ClayBeds), 7);
    let radiator = world.player(PLAYER).expect("rig").radiator_level;
    let offer = upgrade_offers_for(
        world.player(PLAYER).expect("rig"),
        ProgressionRules::Descent,
    )[usize::from(offer_index(UpgradeKind::Radiator))];
    let quote = world.check_logistics(PLAYER, &craft).expect("craftable");
    assert_eq!(quote.credits, part_credits(offer.cost));
    assert!(
        quote.credits < offer.cost / 2,
        "a part saves most of the price"
    );
    assert_eq!(act(&mut world, craft), PlayerScopedCommandOutcome::Applied);
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.radiator_level, radiator + 1);
    assert_eq!(rig.carried(ItemKind::Layer(Stratum::ClayBeds)), 1);
    assert_eq!(rig.credits, 100_000 - quote.credits);
    // A Living Dark recipe is not known this high up.
    let bandage = u8::try_from(
        RECIPES
            .iter()
            .position(|recipe| recipe.stratum() == Stratum::FungalForest)
            .expect("bandage"),
    )
    .expect("index");
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::Craft {
                outpost: crust.id,
                recipe: bandage
            }
        ),
        Err(LogisticsRefusal::NotCraftedHere)
    );
    // A patch at the Living Dark's outpost.
    let dark = outpost_of(&world, 2);
    park(&mut world, dark.landing());
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear.rig_systems = u8::MAX;
        rig.hull_level = 10;
        rig.hull = rig.max_hull() * 0.25;
        rig.cargo_bay.add(ItemKind::Layer(Stratum::FungalForest), 5);
    }
    let low = world.player(PLAYER).expect("rig").hull;
    assert_eq!(
        act(
            &mut world,
            LogisticsCommand::Craft {
                outpost: dark.id,
                recipe: bandage
            }
        ),
        PlayerScopedCommandOutcome::Applied
    );
    let rig = world.player(PLAYER).expect("rig");
    assert!(
        rig.hull > rig.max_hull().mul_add(0.5, low),
        "{} -> {}",
        low,
        rig.hull
    );
    assert_eq!(rig.carried(ItemKind::Layer(Stratum::FungalForest)), 0);
}

#[test]
fn rock_on_deep_descent_worlds_yields_layer_resources_and_ore_its_premium() {
    let mut world = deep_world(0x0FA1_4007);
    let layout = world.descent_layout();
    let clay = layout.strata()[1];
    let row = clay.top + 10;
    let before = world.entities.of_kind(EntityKind::Drop).count();
    for x in -60..60 {
        world.descent_dig_extras(PLAYER, TilePosition { x, y: row }, TileKind::Clay);
    }
    let layer_drops: Vec<ItemKind> = world
        .entities
        .of_kind(EntityKind::Drop)
        .filter_map(|entity| match entity.state {
            EntityState::Drop(drop) => Some(drop.stack.kind),
            _ => None,
        })
        .filter(|kind| matches!(kind, ItemKind::Layer(_)))
        .collect();
    assert!(
        world.entities.of_kind(EntityKind::Drop).count() > before,
        "some rock yields a layer resource"
    );
    assert!(layer_drops.len() < 20, "but rarely: {}", layer_drops.len());
    assert!(
        layer_drops
            .iter()
            .all(|kind| *kind == ItemKind::Layer(Stratum::ClayBeds)),
        "{layer_drops:?}"
    );
    // Ore from the Old Deep pays that Descent's premium.
    let deep_row = layout.descent(1).expect("II").top + 50;
    world.descent_dig_extras(
        PLAYER,
        TilePosition { x: 3, y: deep_row },
        TileKind::Ore(MineralKind::Gold),
    );
    assert_eq!(world.haul_premium_percent(PLAYER), descent_value_percent(1));
    let mut plain = world.clone();
    plain.logistics.haul_descent.clear();
    for world in [&mut world, &mut plain] {
        world
            .player_mut(PLAYER)
            .expect("rig")
            .cargo_bay
            .add(ItemKind::Mineral(MineralKind::Gold), 20);
    }
    let premium = {
        let before = world.player(PLAYER).expect("rig").credits;
        let _ = world.sell_canonical_cargo(PLAYER);
        world.player(PLAYER).expect("rig").credits - before
    };
    let base = {
        let before = plain.player(PLAYER).expect("rig").credits;
        let _ = plain.sell_canonical_cargo(PLAYER);
        plain.player(PLAYER).expect("rig").credits - before
    };
    assert!(premium > base, "{premium} vs {base}");
    assert_eq!(world.haul_premium_percent(PLAYER), 100, "a sale resets it");
    // Classic worlds: no layer resources, no premium.
    let mut classic = WorldState::new_game(PLAYER);
    let drops = classic.entities.of_kind(EntityKind::Drop).count();
    for x in -60..60 {
        classic.descent_dig_extras(PLAYER, TilePosition { x, y: 40 }, TileKind::Stone);
    }
    assert_eq!(classic.entities.of_kind(EntityKind::Drop).count(), drops);
    assert_eq!(classic.haul_premium_percent(PLAYER), 100);
}

#[test]
fn an_emptied_bay_carries_no_haul_premium_into_the_next_trip() {
    let mut world = deep_world(0x0FA1_4008);
    let deep_row = world.descent_layout().descent(2).expect("III").top + 50;
    world.descent_dig_extras(
        PLAYER,
        TilePosition { x: 3, y: deep_row },
        TileKind::Ore(MineralKind::Gold),
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(ItemKind::Mineral(MineralKind::Gold), 3);
    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    assert_eq!(
        world.haul_premium_percent(PLAYER),
        descent_value_percent(2),
        "ore aboard keeps its premium"
    );
    // The ore is gone some other way than a counter sale (a wreck, a village trade): the
    // next haul from the Crust pays Crust prices.
    let _ = world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .take_all(MineralKind::Gold);
    let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
    assert_eq!(world.haul_premium_percent(PLAYER), 100);
    assert_eq!(
        world.logistics().haul_premium_percent(PLAYER),
        world.haul_premium_percent(PLAYER),
        "the sale counters read the same premium"
    );
}

#[test]
fn the_registry_names_a_layer_resource_for_every_recipe() {
    for recipe in &RECIPES {
        for (stratum, _) in recipe.inputs {
            assert_ne!(stratum.def().layer_resource.name, "");
        }
    }
}
