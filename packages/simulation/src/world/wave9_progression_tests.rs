//! Wave 9 progression on Deep Descent worlds: the deep supplies of Descents IV-VI sold at the
//! deep counters and crafted at the workshops, the settlements' specialties, the bosses'
//! rewards for Descents IV-VI and the authored climate.

use crate::{
    PlayerCommand, PlayerId, SimulationParams, TickInputs, WorldState,
    depth_progression::{
        DescentLayout, OUTPOST_SPECIALTY_PERCENT, ProgressionRules, RigSystem, deep_climate,
        outpost_price_percent,
    },
    descent::{Descent, DescentBoss, DescentKey, Stratum},
    economy::{UpgradeKind, UpgradeLock, upgrade_lock_for, upgrade_offers_for},
    enemy::EnemyDifficulty,
    item::ItemKind,
    logistics::{LogisticsCommand, Outpost},
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
    terrain::{TileKind, TilePosition},
    workshop::{CraftOutput, DeepSupply, MAX_SUPPLY_STACK, RECIPES, supply_price},
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::logistics::LogisticsRefusal;

const PLAYER: PlayerId = PlayerId::new(0);

/// A Deep Descent world whose crew holds the Deep Claim and every gate reward down to the
/// Strange Layers (test worlds below Descent I need them, or the gates crush the rig).
fn deep_world(seed: u64) -> WorldState {
    let settings = WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    };
    let mut world =
        WorldState::new_game_with_settings(PLAYER, &settings, SimulationParams::default());
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

fn recipe_for(supply: DeepSupply) -> u8 {
    u8::try_from(
        RECIPES
            .iter()
            .position(
                |recipe| matches!(recipe.output, CraftOutput::Supply(made, _) if made == supply),
            )
            .expect("a recipe"),
    )
    .expect("index")
}

#[test]
fn deep_counters_sell_their_supplies_into_the_kit() {
    let mut world = deep_world(0x0FA1_9001);
    world.progression.deepest_tile_reached = world.descent_layout().floor_row() - 1;
    // The Hive Gate stocks ballast for the drowned below, not the Strange Layers' filters.
    let hive = outpost_of(&world, 2);
    park(&mut world, hive.landing());
    world.player_mut(PLAYER).expect("rig").credits = 1_000_000;
    let ballast = LogisticsCommand::BuySupply {
        outpost: hive.id,
        supply: DeepSupply::BallastTank,
    };
    let quote = world.check_logistics(PLAYER, &ballast).expect("stocked");
    assert!(quote.credits >= supply_price(DeepSupply::BallastTank, 2));
    assert_eq!(
        act(&mut world, ballast),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.supplies_of(PLAYER, DeepSupply::BallastTank), 1);
    assert_eq!(
        world.player(PLAYER).expect("rig").credits,
        1_000_000 - quote.credits
    );
    assert_eq!(
        world.check_logistics(
            PLAYER,
            &LogisticsCommand::BuySupply {
                outpost: hive.id,
                supply: DeepSupply::LampFilter,
            }
        ),
        Err(LogisticsRefusal::SupplyNotSoldHere)
    );
    // The kit holds a stack of each.
    for _ in 1..MAX_SUPPLY_STACK {
        assert_eq!(
            act(&mut world, ballast),
            PlayerScopedCommandOutcome::Applied
        );
    }
    assert_eq!(
        world.check_logistics(PLAYER, &ballast),
        Err(LogisticsRefusal::SupplyFull)
    );
    // The Descent mechanics spend them one at a time; an empty kit leaves no trace.
    for left in (0..MAX_SUPPLY_STACK).rev() {
        assert!(world.take_supply(PLAYER, DeepSupply::BallastTank));
        assert_eq!(world.supplies_of(PLAYER, DeepSupply::BallastTank), left);
    }
    assert!(!world.take_supply(PLAYER, DeepSupply::BallastTank));
    assert!(world.logistics().supplies.is_empty());
    // The Prism Relay sells every supply.
    let prism = outpost_of(&world, 5);
    park(&mut world, prism.landing());
    for supply in DeepSupply::ALL {
        let buy = LogisticsCommand::BuySupply {
            outpost: prism.id,
            supply,
        };
        assert_eq!(
            act(&mut world, buy),
            PlayerScopedCommandOutcome::Applied,
            "{supply:?}"
        );
        assert_eq!(world.supplies_of(PLAYER, supply), 1);
    }
}

#[test]
fn workshops_craft_deep_supplies_from_layer_resources() {
    let mut world = deep_world(0x0FA1_9002);
    world.progression.deepest_tile_reached = world.descent_layout().floor_row() - 1;
    let merrow = outpost_of(&world, 3);
    park(&mut world, merrow.landing());
    let heaters = LogisticsCommand::Craft {
        outpost: merrow.id,
        recipe: recipe_for(DeepSupply::FrostHeater),
    };
    assert_eq!(
        world.check_logistics(PLAYER, &heaters),
        Err(LogisticsRefusal::MissingResource {
            stratum: Stratum::BrineGlaciers,
            need: 5
        })
    );
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(ItemKind::Layer(Stratum::BrineGlaciers), 6);
    let credits = world.player(PLAYER).expect("rig").credits;
    assert_eq!(
        act(&mut world, heaters),
        PlayerScopedCommandOutcome::Applied
    );
    assert_eq!(world.supplies_of(PLAYER, DeepSupply::FrostHeater), 2);
    let rig = world.player(PLAYER).expect("rig");
    assert_eq!(rig.carried(ItemKind::Layer(Stratum::BrineGlaciers)), 1);
    assert_eq!(rig.credits, credits, "supplies cost only their resources");
    // A batch that would overflow the kit is refused before it eats the resources.
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::FrostHeater, MAX_SUPPLY_STACK - 3);
    world
        .player_mut(PLAYER)
        .expect("rig")
        .cargo_bay
        .add(ItemKind::Layer(Stratum::BrineGlaciers), 4);
    assert_eq!(
        world.check_logistics(PLAYER, &heaters),
        Err(LogisticsRefusal::SupplyFull)
    );
    assert_eq!(
        world
            .player(PLAYER)
            .expect("rig")
            .carried(ItemKind::Layer(Stratum::BrineGlaciers)),
        5
    );
    // The Hollow World's recipes are not known at Merrow City.
    let bait = LogisticsCommand::Craft {
        outpost: merrow.id,
        recipe: recipe_for(DeepSupply::BeastBait),
    };
    assert_eq!(
        world.check_logistics(PLAYER, &bait),
        Err(LogisticsRefusal::NotCraftedHere)
    );
}

#[test]
fn deep_supplies_replicate_and_save() {
    let mut world = deep_world(0x0FA1_9003);
    let _ = world
        .logistics
        .add_supplies(PLAYER, DeepSupply::TimeBubbleBomb, 3);
    let bytes = postcard::to_stdvec(world.logistics()).expect("encode");
    let decoded: crate::logistics::Logistics = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(decoded.supplies_of(PLAYER, DeepSupply::TimeBubbleBomb), 3);
    let json = serde_json::to_string(world.logistics()).expect("json");
    let back: crate::logistics::Logistics = serde_json::from_str(&json).expect("json back");
    assert_eq!(&back, world.logistics());
    // An older save without the field loads with an empty kit.
    let mut value: serde_json::Value = serde_json::from_str(&json).expect("value");
    value.as_object_mut().expect("object").remove("supplies");
    let old: crate::logistics::Logistics = serde_json::from_value(value).expect("old save");
    assert!(old.supplies.is_empty());
}

#[test]
fn merrow_city_sells_its_seals_cheaper() {
    let mut world = deep_world(0x0FA1_9004);
    world.progression.deepest_tile_reached = world.descent_layout().floor_row() - 1;
    world.logistics.felt_systems = u8::MAX;
    let merrow = outpost_of(&world, 3);
    park(&mut world, merrow.landing());
    {
        let rig = world.player_mut(PLAYER).expect("rig");
        rig.hazard_gear.pressure_rating = 2;
        rig.credits = 10_000_000;
    }
    let index = UpgradeKind::ALL
        .iter()
        .position(|kind| *kind == UpgradeKind::PressureRating)
        .expect("line");
    let (offer, price) = world
        .outpost_offer(PLAYER, &merrow, index)
        .expect("stocked");
    assert_eq!(
        outpost_price_percent(3, merrow.claimed(), UpgradeKind::PressureRating),
        OUTPOST_SPECIALTY_PERCENT
    );
    assert_eq!(
        u64::from(price),
        u64::from(offer.cost) * u64::from(OUTPOST_SPECIALTY_PERCENT) / 100
    );
    assert!(price < offer.cost);
}

#[test]
fn the_gates_of_descents_four_to_six_are_won_from_their_bosses() {
    let world = deep_world(0x0FA1_9005);
    let settings = *world.world_settings();
    let rules = ProgressionRules::Descent;
    let rig = world.player(PLAYER).expect("rig");
    let mut stripped = rig.clone();
    stripped.hazard_gear.rig_systems = 0;
    for (descent, boss, systems) in [
        (
            Descent::DrownedAndFrozen,
            DescentBoss::Leviathan,
            &[RigSystem::SonarSuite, RigSystem::CryoHull][..],
        ),
        (
            Descent::HollowWorld,
            DescentBoss::SunSerpent,
            &[RigSystem::GravityAnchor][..],
        ),
        (
            Descent::StrangeLayers,
            DescentBoss::TheGlitch,
            &[RigSystem::PhaseDrill][..],
        ),
    ] {
        let gate = descent.last_stratum().def().gate.expect("a gate");
        assert_eq!(gate.boss, boss);
        for system in systems {
            assert_eq!(system.boss_reward(), Some(boss), "{system:?}");
            let kind = UpgradeKind::for_rig_system(*system);
            let index = UpgradeKind::ALL
                .iter()
                .position(|line| *line == kind)
                .expect("line");
            let offer = upgrade_offers_for(&stripped, rules)[index];
            assert_eq!(
                upgrade_lock_for(&offer, &settings, i32::MAX, rules, u8::MAX),
                Some(UpgradeLock::Boss(boss)),
                "{system:?} is never sold"
            );
            let key = match system {
                RigSystem::SonarSuite => DescentKey::SonarSuite,
                RigSystem::CryoHull => DescentKey::CryoHull,
                RigSystem::GravityAnchor => DescentKey::GravityAnchor,
                _ => DescentKey::PhaseDrill,
            };
            assert!(gate.rewards.contains(&key), "{system:?}");
        }
    }
}

#[test]
fn a_rig_with_its_descents_caps_holds_its_hull_through_four_to_six() {
    let mut world = deep_world(0x0FA1_9006);
    let layout = world.descent_layout();
    for (index, stratum) in Stratum::ALL.iter().enumerate() {
        let Some(climate) = deep_climate(*stratum) else {
            continue;
        };
        let span = layout.strata()[index];
        let cap = crate::depth_progression::core_tier_cap(span.descent);
        {
            let rig = world.player_mut(PLAYER).expect("rig");
            rig.radiator_level = cap;
            rig.hazard_gear.heat_shield = crate::depth_progression::CLIMATE_MAX_SHIELD_TIERS;
            rig.hull = rig.max_hull();
        }
        let at = TilePosition {
            x: 30,
            y: i32::midpoint(span.top, span.bottom),
        };
        park(&mut world, at);
        let before = world.player(PLAYER).expect("rig").hull;
        for _ in 0..120 {
            let _ = world.simulate_tick(world.simulation_tick(), &TickInputs::default());
            if let Some(rig) = world.player_mut(PLAYER) {
                rig.fuel = rig.fuel_capacity;
            }
        }
        let lost = before - world.player(PLAYER).expect("rig").hull;
        assert!(
            lost < 1.0,
            "{stratum:?} ({} heat units) cost {lost} hull in two seconds",
            climate.heat_units
        );
    }
}
