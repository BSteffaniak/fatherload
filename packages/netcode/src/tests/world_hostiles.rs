//! Space combat online (Wave 12 hostiles): two clients in space a few sectors from home, a
//! corsair attack on one of them and a swarm on the other, over a lossy link. Ships, shots and
//! the fight's state live in the shared `Hostiles` section and every client predicts them like
//! the rest of the world: every client converges on the host with no hash mismatch, holds the
//! host's ships and sees the shots fly.

use std::time::Duration;

use fatherload_simulation::{
    WorldState,
    flight::{
        FlightPlace, RigFlight,
        space::{galaxy_sector_of, galaxy_sector_origin, home_star, tiles},
    },
    hostiles::{EncounterKind, EncounterState, Faction, ShipClass, ShipMode},
};

use super::support::{assert_bounded, assert_converged};
use crate::harness::{Harness, HarnessConfig};
use crate::{LinkConditions, SessionParams};

#[test]
fn world_hostile_ships_and_shots_replicate_and_converge() {
    let mut harness: Harness<WorldState> = Harness::new(HarnessConfig::new(
        SessionParams::new(60),
        2,
        LinkConditions::new(60, 10, 2.0),
    ));
    harness.run_for(Duration::from_secs(2));
    assert!(harness.all_in_game());
    let mut world = harness.world().clone();
    let ids: Vec<_> = world.player_ids().collect();
    let (hx, hy) = galaxy_sector_of(home_star());
    let corner = galaxy_sector_origin((hx + 3, hy));
    for (index, id) in ids.iter().enumerate() {
        let rig = world.player_mut(*id).expect("rig");
        let max = rig.max_hull();
        rig.hull = max;
        rig.flight = RigFlight {
            place: FlightPlace::Space,
            pos: corner.offset(
                tiles(6_000.0) + if index == 0 { 0.0 } else { 3_000.0 },
                tiles(6_000.0),
            ),
            air: 600.0,
            ..RigFlight::default()
        };
    }
    let mut heart = world.heart().clone();
    heart.space_unlocked = true;
    heart.star_drive.extend(ids.iter().copied());
    world.set_heart(heart);
    // Corsairs on the first rig (already fighting), a swarm on the second.
    let _ = world.stage_encounter(
        ids[0],
        Faction::Pirates,
        EncounterKind::Roaming,
        &[ShipClass::Cutter, ShipClass::Marauder],
        3,
    );
    if let Some(second) = ids.get(1) {
        let _ = world.stage_encounter(
            *second,
            Faction::Swarm,
            EncounterKind::Roaming,
            &[ShipClass::Mite, ShipClass::Mite, ShipClass::Stinger],
            3,
        );
    }
    let mut hostiles = world.hostiles().clone();
    for encounter in &mut hostiles.encounters {
        encounter.state = EncounterState::Fighting;
    }
    for ship in &mut hostiles.ships {
        ship.mode = ShipMode::Attacking;
    }
    world.set_hostiles(hostiles);
    harness.host.new_epoch(world);
    let mut shots_seen = false;
    for _ in 0..8 {
        harness.run_for(Duration::from_secs(1));
        shots_seen |= !harness.world().hostiles().shots.is_empty();
        // Clients see the shots in flight too.
        for client in &harness.clients {
            if let Some(authority) = client.session.authority() {
                shots_seen |= !authority.hostiles().shots.is_empty();
            }
        }
    }
    harness.settle(Duration::from_secs(1));
    assert_converged(&harness);
    assert_bounded(&harness);
    assert!(shots_seen, "the ships fired");
    let host = harness.world().hostiles().clone();
    assert!(!host.ships.is_empty() || !host.wrecks.is_empty());
    for client in &harness.clients {
        assert_eq!(client.hash_mismatches, 0);
        let authority = client.session.authority().expect("authority");
        let classes = |space: &fatherload_simulation::hostiles::HostileSpace| {
            space
                .ships
                .iter()
                .map(|ship| (ship.id, ship.class))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            classes(authority.hostiles()),
            classes(&host),
            "the client's authoritative world holds the host's ships"
        );
        assert_eq!(authority.hostiles().encounters.len(), host.encounters.len());
        let predicted = client.session.predicted().expect("predicted");
        assert_eq!(
            predicted.hostiles().ships.len(),
            host.ships.len(),
            "and predicts the same ships"
        );
    }
}
