//! A replica built from a keyframe plus per-tick, op-log and on-change section updates must
//! reach the authority's exact state hash at every tick.

use std::collections::BTreeMap;

use serde::{Serialize, de::DeserializeOwned};

use super::{
    SectionKey, SectionPayload, WorldState,
    determinism_tests::{scripted_inputs, scripted_world},
};
use crate::{LiveEntities, PlayerId, PlayerTickState, SimulationTick, TickOutput};

/// Round-trips a value through the binary wire codec, as the network would.
fn wire<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let bytes = postcard::to_stdvec(value).expect("encode");
    postcard::from_bytes(&bytes).expect("decode")
}

/// Tracks which on-change sections the replica already has, like a per-client baseline.
#[derive(Default)]
struct Baseline {
    hashes: BTreeMap<SectionKey, u64>,
    sent: usize,
    /// `Some(viewer)`: replicate only what a client playing `viewer` may see.
    viewer: Option<PlayerId>,
}

impl Baseline {
    fn sees_section(&self, key: SectionKey) -> bool {
        self.viewer
            .is_none_or(|viewer| key.visible_to(Some(viewer)))
    }

    fn sees_player(&self, player_id: PlayerId) -> bool {
        self.viewer.is_none_or(|viewer| viewer == player_id)
    }

    fn changed_sections(&mut self, authority: &WorldState) -> Vec<(SectionKey, SectionPayload)> {
        let mut changed = Vec::new();
        for key in authority.section_keys() {
            if !self.sees_section(key) {
                continue;
            }
            let hash = authority.section_hash(key).expect("listed section exists");
            if self.hashes.get(&key) != Some(&hash) {
                self.hashes.insert(key, hash);
                changed.push((key, authority.section(key).expect("listed section exists")));
            }
        }
        self.sent += changed.len();
        changed
    }
}

fn replicate_tick(
    replica: &mut WorldState,
    authority: &WorldState,
    output: &TickOutput,
    baseline: &mut Baseline,
) {
    replica.set_simulation_tick(authority.simulation_tick());
    let players: Vec<(PlayerId, PlayerTickState)> = authority
        .player_ids()
        .map(|id| (id, authority.player_tick_state(id).expect("player")))
        .collect();
    for (id, state) in wire(&players) {
        replica.apply_player_tick_state(id, state);
    }
    let entities: LiveEntities = wire(&authority.live_entities());
    replica.apply_live_entities(entities);
    for op in wire(&output.terrain_ops) {
        replica.apply_tile_change(&op);
    }
    for discovery in wire(&output.discoveries) {
        if baseline.sees_player(discovery.player_id) {
            replica.apply_discovery(&discovery);
        }
    }
    for (key, payload) in wire(&baseline.changed_sections(authority)) {
        replica.apply_section(key, payload);
    }
}

#[test]
fn keyframe_plus_section_updates_reach_identical_hashes() {
    let mut authority = scripted_world();
    let mut replica: WorldState = wire(&authority);
    let mut baseline = Baseline::default();
    // The keyframe already carries every section.
    let _ = baseline.changed_sections(&authority);
    let initial_sent = baseline.sent;
    assert_eq!(
        replica.simulation_state_hash(),
        authority.simulation_state_hash()
    );

    let mut ops_seen = 0;
    for tick in 0..6_000 {
        let inputs = scripted_inputs(tick);
        let output = authority.simulate_tick(SimulationTick::new(tick), &inputs);
        ops_seen += output.terrain_ops.len();
        replicate_tick(&mut replica, &authority, &output, &mut baseline);
        assert_eq!(
            replica.simulation_state_hash(),
            authority.simulation_state_hash(),
            "replica diverged at tick {}",
            tick + 1
        );
    }
    assert!(ops_seen > 0, "script changed terrain");
    // A late keyframe (terrain digest recomputed from scratch on decode) matches too.
    let late: WorldState = wire(&authority);
    assert_eq!(late.terrain().digest(), authority.terrain().digest());
    assert_eq!(
        late.simulation_state_hash(),
        authority.simulation_state_hash()
    );
    let updates = baseline.sent - initial_sent;
    assert!(updates > 0, "on-change sections were resent");
    assert!(
        updates < 6_000 * authority.section_keys().len() / 4,
        "on-change sections are not resent every tick: {updates}"
    );
}

#[test]
fn replica_follows_roster_changes() {
    let mut authority = scripted_world();
    let mut replica: WorldState = wire(&authority);
    let mut baseline = Baseline::default();
    let _ = baseline.changed_sections(&authority);
    let joined = PlayerId::new(7);
    for tick in 0..600 {
        if tick == 100 {
            authority.add_player(joined);
        }
        if tick == 400 {
            authority.remove_player(PlayerId::new(2));
        }
        let inputs = scripted_inputs(tick);
        let output = authority.simulate_tick(SimulationTick::new(tick), &inputs);
        // Roster first, as netcode does: drop departed players, add new ones.
        let roster: Vec<PlayerId> = authority.player_ids().collect();
        let departed: Vec<PlayerId> = replica
            .player_ids()
            .filter(|id| !roster.contains(id))
            .collect();
        for id in departed {
            replica.remove_player(id);
        }
        for id in &roster {
            replica.add_player(*id);
        }
        replicate_tick(&mut replica, &authority, &output, &mut baseline);
        assert_eq!(
            replica.simulation_state_hash(),
            authority.simulation_state_hash(),
            "replica diverged at tick {}",
            tick + 1
        );
    }
    assert!(replica.player(joined).is_some());
    assert!(replica.player(PlayerId::new(2)).is_none());
}

#[test]
fn a_client_replica_without_other_players_private_state_matches_the_visible_hash() {
    let viewer = PlayerId::new(1);
    let mut authority = scripted_world();
    let mut replica: WorldState = wire(&authority.redacted_for(Some(viewer)));
    let mut baseline = Baseline {
        viewer: Some(viewer),
        ..Baseline::default()
    };
    let _ = baseline.changed_sections(&authority);
    let mut full_hash_differed = false;
    for tick in 0..3_000 {
        let inputs = scripted_inputs(tick);
        let output = authority.simulate_tick(SimulationTick::new(tick), &inputs);
        replicate_tick(&mut replica, &authority, &output, &mut baseline);
        assert_eq!(
            replica.visible_state_hash(Some(viewer)),
            authority.visible_state_hash(Some(viewer)),
            "client replica diverged at tick {}",
            tick + 1
        );
        full_hash_differed |= replica.simulation_state_hash() != authority.simulation_state_hash();
    }
    assert!(
        full_hash_differed,
        "the other rig's private state really was withheld"
    );
    let other = PlayerId::new(2);
    assert_ne!(
        replica.player(other).map(|rig| rig.credits),
        authority.player(other).map(|rig| rig.credits)
    );
    assert_eq!(
        replica.discovered_tile_count(other),
        0,
        "other players' discovery is private"
    );
    assert_eq!(
        replica.discovered_tile_count(viewer),
        authority.discovered_tile_count(viewer)
    );
}

#[test]
fn every_shared_section_exists_and_round_trips() {
    let world = scripted_world();
    let keys = world.section_keys();
    for key in SectionKey::SHARED {
        assert!(keys.contains(&key), "{key:?} listed");
    }
    for key in keys {
        let payload = world.section(key).expect("listed section exists");
        let mut copy = world.clone();
        copy.apply_section(key, wire(&payload));
        assert_eq!(
            copy.section_hash(key),
            world.section_hash(key),
            "{key:?} round-trips"
        );
        assert_eq!(copy.simulation_state_hash(), world.simulation_state_hash());
    }
}
