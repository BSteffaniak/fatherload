//! Unit tests of the station model: profiles, the shipyard's figures, markets, the board, the
//! bar, hazards and the first hour's economy.

use super::{
    balance::first_trip,
    bounty::{BoardContext, MissionKind, board_context, offer},
    hazard::{PlanetHazard, drain, exposure, hazards_of},
    market::{Good, MarketShift, goods_of, price},
    profile::{StationOwner, profile_of},
    shipyard::{AlienTech, GEAR_MAX, LINE_MAX, ShipGear, ShipLine, ShipUpgrade, SpaceGear},
    *,
};
use crate::space::{Atmosphere, HOME_SECTOR, Space, SpaceBody, SpaceSector, SystemRef};

const HALCYON: StationId = StationId {
    sector: HOME_SECTOR,
    system: 0,
    index: 0,
};

/// Every station within `reach` sectors of home, with its profile.
fn stations_near(space: &Space, reach: i32) -> Vec<StationProfile> {
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

fn halcyon_context(seed: u64) -> BoardContext {
    board_context(&Space::new(seed), HALCYON).expect("Halcyon Dock")
}

#[test]
fn halcyon_dock_is_a_human_hub_with_a_small_yard() {
    let space = Space::new(7);
    let home = space.system(SystemRef {
        sector: HOME_SECTOR,
        system: 0,
    });
    let home = home.expect("home system");
    let profile = profile_of(7, &home, &home.stations[0]);
    assert_eq!(profile.name, "Halcyon Dock");
    assert_eq!(profile.owner, StationOwner::Human);
    assert!(profile.halcyon && profile.market && profile.bounties && profile.bar);
    assert_eq!(profile.yard_tier, 2);
    assert_eq!(profile.gear_tier, 1);
    assert!(!profile.alien_tech && !profile.salvage);
    assert_eq!((profile.fuel_price, profile.repair_price), (2, 4));
}

#[test]
fn stations_come_in_every_owner_and_are_seeded() {
    let space = Space::new(0x00C0_FFEE);
    let stations = stations_near(&space, 4);
    assert!(stations.len() > 20, "{} stations", stations.len());
    let human = stations
        .iter()
        .filter(|s| s.owner == StationOwner::Human)
        .count();
    let alien = stations
        .iter()
        .filter(|s| matches!(s.owner, StationOwner::Alien(_)))
        .count();
    let derelict = stations
        .iter()
        .filter(|s| s.owner == StationOwner::Derelict)
        .count();
    assert!(
        human > 0 && alien > 0 && derelict > 0,
        "{human} {alien} {derelict}"
    );
    for station in &stations {
        // An alien-run station sells alien tech, a derelict nothing but salvage.
        assert_eq!(
            station.alien_tech,
            matches!(station.owner, StationOwner::Alien(_))
        );
        if station.owner == StationOwner::Derelict {
            assert!(station.salvage && !station.market && !station.bar && !station.dock_services);
            assert!(station.name.starts_with("Wreck of "));
        }
        assert!(!station.name.is_empty());
    }
    // A pure function of the seed.
    assert_eq!(stations, stations_near(&Space::new(0x00C0_FFEE), 4));
    // Shipyards to the top tier exist among them.
    assert!(stations.iter().any(|s| s.yard_tier == LINE_MAX));
}

#[test]
fn the_shipyard_prices_climb_and_the_gear_does_what_it_says() {
    for line in ShipLine::ALL {
        let mut last = 0;
        for tier in 1..=LINE_MAX {
            let price = line.price(tier);
            assert!(price > last, "{line:?} tier {tier}");
            last = price;
            assert_ne!(line.tier_name(tier), "stock");
        }
    }
    let mut gear = ShipGear::default();
    assert!((gear.damage_scale() - 1.0).abs() < f32::EPSILON);
    assert!((gear.engine_scale() - 1.0).abs() < f32::EPSILON);
    assert_eq!(gear.hold_capacity(), 8);
    for upgrade in ShipUpgrade::all() {
        for _ in 0..upgrade.max_level() + 2 {
            gear.fit(upgrade);
        }
        assert_eq!(upgrade.level(&gear), upgrade.max_level(), "{upgrade:?}");
        assert!(!upgrade.effect(0).is_empty());
    }
    assert_eq!(gear, ShipGear::MAXED);
    assert!(gear.damage_scale() < 0.6);
    assert!(gear.engine_scale() > 1.5);
    assert!(gear.drive_range_scale() > 2.0);
    assert!(gear.hold_capacity() >= 60);
    assert!(gear.extra_air() >= 1_000.0);
    for tech in AlienTech::ALL {
        assert!(gear.has(tech));
    }
    for kind in SpaceGear::ALL {
        assert_eq!(kind.counters().counter(), kind);
        assert_eq!(gear.gear(kind), GEAR_MAX);
    }
}

#[test]
fn markets_trade_their_own_goods_and_trades_move_the_price() {
    let seed = 11;
    let context = halcyon_context(seed);
    let halcyon = context.station.expect("profile");
    let goods = goods_of(&halcyon, seed);
    assert!(goods.len() >= 5);
    for (good, _) in &goods {
        let quote = price(seed, &halcyon, *good, MarketShift::default(), 0).expect("quote");
        assert!(quote.buy > quote.sell, "{good:?}: a spread");
    }
    // Not every good everywhere.
    assert!(price(seed, &halcyon, Good::XenoSilk, MarketShift::default(), 0).is_none());
    // Selling ten lowers the price; it recovers over time.
    let before = price(seed, &halcyon, Good::WaterIce, MarketShift::default(), 0).expect("ice");
    let shift = MarketShift::default().traded(0, -10);
    let after = price(seed, &halcyon, Good::WaterIce, shift, 0).expect("ice");
    assert!(after.sell < before.sell);
    let later = price(seed, &halcyon, Good::WaterIce, shift, 10 * 20_000).expect("ice");
    assert_eq!(later.sell, before.sell, "faded after ten fades");
    assert_eq!(shift.traded(0, -100).shift, -market::MAX_SHIFT);
}

#[test]
fn a_lot_pays_its_own_price_impact_so_selling_and_buying_back_always_loses() {
    let seed = 11;
    let context = halcyon_context(seed);
    let halcyon = context.station.expect("profile");
    for (good, _) in goods_of(&halcyon, seed) {
        for units in [1_u32, 8, 20, 44, 60] {
            let start = MarketShift::default();
            let (sold, paid) =
                market::trade_lot(seed, &halcyon, good, start, 0, units, false, 0).expect("sells");
            assert_eq!(sold, units);
            let after = start.traded(0, -i32::try_from(units).expect("units"));
            let (bought, cost) =
                market::trade_lot(seed, &halcyon, good, after, 0, units, true, u32::MAX)
                    .expect("buys");
            assert_eq!(bought, units);
            assert!(
                cost > paid,
                "{good:?} x{units}: sold for {paid}, bought back for {cost}"
            );
        }
        // A budget stops the lot.
        let one = price(seed, &halcyon, good, MarketShift::default(), 0)
            .expect("quote")
            .buy;
        let (bought, cost) = market::trade_lot(
            seed,
            &halcyon,
            good,
            MarketShift::default(),
            0,
            10,
            true,
            one * 3,
        )
        .expect("buys");
        assert!(bought <= 3 && cost <= one * 3, "{good:?}");
    }
}

#[test]
fn halcyons_first_board_is_a_scan_a_quota_and_a_delivery() {
    for seed in [1_u64, 7, 0x00C0_FFEE, 0x5EED_0011] {
        let context = halcyon_context(seed);
        let board: Vec<_> = (0..3).map(|slot| offer(seed, &context, slot, 0)).collect();
        assert!(matches!(
            board[0].map(|m| m.kind),
            Some(MissionKind::Scan { .. })
        ));
        assert!(matches!(
            board[1].map(|m| m.kind),
            Some(MissionKind::Mine { units: 12, .. })
        ));
        // A delivery when another crewed station is near, else another kind.
        assert!(board[2].is_some(), "seed {seed}");
        for mission in board.iter().flatten() {
            assert!(
                mission.reward >= 600 && mission.reward < 6_000,
                "{mission:?}"
            );
            assert!(!mission.ready());
        }
        // Later serials roll fresh missions, the same on every peer.
        let next = offer(seed, &context, 0, 1);
        assert_eq!(next, offer(seed, &halcyon_context(seed), 0, 1));
    }
}

#[test]
fn bars_have_patrons_and_rumours_about_near_worlds() {
    let seed = 0x00C0_FFEE;
    let space = Space::new(seed);
    let context = halcyon_context(seed);
    let profile = context.station.clone().expect("profile");
    let patrons = bar::patrons(&space, &profile, &context);
    assert_eq!(patrons.len(), usize::from(bar::PATRONS));
    assert!(
        patrons
            .iter()
            .all(|p| !p.name.is_empty() && !p.line.is_empty())
    );
    assert!(patrons.iter().all(|p| p.character.is_some() && !p.alien));
    assert!(patrons.iter().any(|p| p.rumour.is_some()));
    for rumour in patrons.iter().filter_map(|p| p.rumour.as_ref()) {
        assert!(context.bodies.iter().any(|b| b.id == rumour.body));
        assert!(rumour.text.contains(':'), "{}", rumour.text);
    }
    // Alien-run bars are mostly alien.
    let alien = stations_near(&space, 4)
        .into_iter()
        .find(|s| matches!(s.owner, StationOwner::Alien(_)))
        .expect("an alien station");
    let alien_context = board_context(&space, alien.id).expect("context");
    let alien_patrons = bar::patrons(&space, &alien, &alien_context);
    assert_eq!(alien_patrons.iter().filter(|p| p.alien).count(), 2);
}

fn body(temperature_c: i16, atmosphere: Atmosphere, gravity_milli_g: u16) -> SpaceBody {
    SpaceBody {
        gravity_milli_g,
        atmosphere,
        temperature_c,
        ..SpaceBody::default()
    }
}

#[test]
fn hazard_tags_come_from_the_world_and_gear_counters_them() {
    assert!(hazards_of(&body(15, Atmosphere::Breathable, 1_000)).is_empty());
    assert_eq!(
        hazards_of(&body(140, Atmosphere::Thin, 1_000)),
        vec![(PlanetHazard::Heat, 1)]
    );
    assert_eq!(
        hazards_of(&body(20, Atmosphere::Scalding, 1_000)),
        vec![(PlanetHazard::Heat, 2)]
    );
    assert_eq!(
        hazards_of(&body(-200, Atmosphere::Toxic, 2_000)),
        vec![
            (PlanetHazard::Cold, 2),
            (PlanetHazard::ToxicAir, 2),
            (PlanetHazard::HighGravity, 2)
        ]
    );
    let hot = hazards_of(&body(320, Atmosphere::Thin, 1_000));
    let mut gear = ShipGear::default();
    let bare = drain(&hot, &gear, false);
    assert!(bare.hull > 0.0 && bare.biting == PlanetHazard::Heat.bit());
    gear.fit(ShipUpgrade::Gear(SpaceGear::HeatShielding));
    let half = drain(&hot, &gear, false);
    assert!(
        bare.hull.mul_add(-0.5, half.hull).abs() < 1e-4,
        "tier 1 halves the severe heat"
    );
    gear.fit(ShipUpgrade::Gear(SpaceGear::HeatShielding));
    let shielded = drain(&hot, &gear, false);
    assert!(shielded.hull <= 0.0 && shielded.biting == 0);
    // High gravity costs fuel only while thrusting.
    let heavy = hazards_of(&body(15, Atmosphere::Thin, 1_500));
    let gear = ShipGear::default();
    assert!(drain(&heavy, &gear, false).fuel <= 0.0);
    assert!(drain(&heavy, &gear, true).fuel > 0.0);
    assert!(exposure(&ShipGear::MAXED, PlanetHazard::HighGravity, 2) <= 0.0);
    // Every hazard has its gear.
    for hazard in PlanetHazard::ALL {
        assert_eq!(hazard.counter().counters(), hazard);
    }
}

#[test]
fn a_first_trip_out_affords_the_first_ship_line_within_an_hour() {
    for seed in [1_u64, 7, 42, 0x00C0_FFEE, 0x5EED_0011] {
        let model = first_trip(seed);
        let minutes = model.first_line_minutes.unwrap_or(f32::INFINITY);
        assert!(
            (30.0..=60.0).contains(&minutes),
            "seed {seed}: the first Ship Engine tier after {minutes} min: {:#?}",
            model.steps
        );
    }
}

#[test]
fn codes_are_stable_and_in_range() {
    for (index, event) in StationEvent::ALL.iter().enumerate() {
        assert_eq!(usize::from(event.code()), index);
        assert!(960 + u32::from(event.code()) < 980);
        assert!(!event.headline().is_empty());
    }
    for refusal in StationRefusal::ALL {
        assert_eq!(StationRefusal::from_code(refusal.code()), Some(refusal));
    }
    for good in Good::ALL {
        assert!(good.base_price() > 0);
    }
}

/// Prints the first hour's model for the README (`cargo test -p fatherload_simulation
/// first_trip_report -- --ignored --nocapture`).
#[test]
#[ignore = "a report, not a check"]
fn first_trip_report() {
    for seed in [1_u64, 7, 42, 0x00C0_FFEE, 0x5EED_0011] {
        let model = first_trip(seed);
        println!(
            "seed {seed:#x}: first Ship Engine tier at {:?} min; {} credits over {} min",
            model.first_line_minutes,
            model.credits(),
            model.minutes()
        );
        for step in &model.steps {
            println!(
                "  {:5.1} min {:5} cr  {}",
                step.minutes, step.credits, step.label
            );
        }
    }
}
