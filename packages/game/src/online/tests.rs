//! In-process integration test (plan §11.3): a `HostSession` and two `ClientSession`s over the
//! REAL `quic_transport` network threads on localhost, through the same net bridge, host adapter
//! and client adapter the app uses.

use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    time::{Duration, Instant},
};

use drillgame_netcode::{
    ActionId, ClientPhase, EndReason, HostConfig, HostSession, NetSimulation, NetTuning, PlayerId,
    SessionParams, Tick,
    world::{WorldEvent, to_sim_player},
};
use drillgame_quic_transport::JoinTarget;
use drillgame_simulation::{ControlState, PlayerCommand, WorldState};

use super::{
    client::{ClientLink, OnlineClient},
    headless::headless_world,
    host::HostNet,
};

/// Real-time frame step for the test loop.
const FRAME: Duration = Duration::from_millis(5);
/// Upper bound for any single wait in the test.
const WAIT: Duration = Duration::from_secs(8);

struct Host {
    session: HostSession<WorldState>,
    net: HostNet,
    hashes: BTreeMap<Tick, u64>,
    resolved: BTreeMap<PlayerId, Vec<ActionId>>,
    last: Instant,
}

impl Host {
    fn start() -> Self {
        let world = headless_world();
        let tuning = NetTuning {
            debug_hash_interval: 1,
            ..NetTuning::default()
        };
        let config = HostConfig {
            params: SessionParams::new(world.tick_rate_hz()),
            tuning,
            record_tick_hashes: true,
            token_seed: super::random_token_seed(),
            ..HostConfig::default()
        };
        let net = HostNet::start("127.0.0.1:0".parse().expect("addr"), false)
            .expect("network thread starts");
        Self {
            session: HostSession::new(world, config),
            net,
            hashes: BTreeMap::new(),
            resolved: BTreeMap::new(),
            last: Instant::now(),
        }
    }

    fn step(&mut self) {
        let now = Instant::now();
        let dt = now - self.last;
        self.last = now;
        self.net.pump(&mut self.session, dt.as_secs_f32());
        let output = self.session.advance(dt);
        for (tick, hash) in output.tick_hashes {
            self.hashes.insert(tick, hash);
        }
        while self.hashes.len() > 4_096 {
            self.hashes.pop_first();
        }
        for event in output.events {
            if let (Some(player), WorldEvent::ActionResolved { id, .. }) =
                (event.player, &event.payload)
            {
                self.resolved.entry(player).or_default().push(*id);
            }
        }
        self.net.send(output.outbound);
    }

    fn addr(&self) -> Option<SocketAddr> {
        self.net.addr()
    }
}

struct Peer {
    client: OnlineClient,
    frame: u64,
    scripted: bool,
    last: Instant,
    hash_matches: u64,
    frames_seen: u64,
}

impl Peer {
    fn join(
        addr: SocketAddr,
        name: &str,
        token: Option<drillgame_netcode::ReconnectToken>,
    ) -> Self {
        Self {
            client: OnlineClient::join(JoinTarget::Addr(addr), name, token, headless_world())
                .expect("client network thread starts"),
            frame: 0,
            scripted: true,
            last: Instant::now(),
            hash_matches: 0,
            frames_seen: 0,
        }
    }

    /// Scripted play: drill, with a scanner pulse and a bomb every couple of seconds.
    fn step(&mut self, host: &Host) {
        let now = Instant::now();
        let dt = now - self.last;
        self.last = now;
        self.client.pump(dt.as_secs_f32());
        if self.client.session().phase() == ClientPhase::InGame {
            self.frame += 1;
            let mut actions = Vec::new();
            if self.scripted {
                match self.frame % 120 {
                    30 => actions.push(PlayerCommand::UseScanner),
                    90 => actions.push(PlayerCommand::PlaceBomb),
                    _ => {}
                }
            }
            let horizontal = if (self.frame / 200) % 2 == 0 {
                0.0
            } else {
                0.6
            };
            self.client
                .submit(ControlState::new(horizontal, false, true), actions);
        }
        let _frame = self.client.advance(dt);
        // Compare right after a server frame was applied: the authority then is exactly the
        // host's world after that tick. (A welcome keyframe already contains the joining
        // player, which the host adds after recording that tick's hash.)
        let frames = self.client.session().stats().frames_received;
        let fresh_frame = frames != self.frames_seen;
        self.frames_seen = frames;
        if fresh_frame
            && let Some(authority) = self.client.session().authority()
            && let Some(expected) = host.hashes.get(&authority.tick())
        {
            assert_eq!(
                authority.state_hash(),
                *expected,
                "client authority diverged from the host at tick {}",
                authority.tick().0
            );
            self.hash_matches += 1;
        }
    }

    fn player(&self) -> Option<PlayerId> {
        self.client.session().player()
    }
}

fn run_until(
    host: &mut Host,
    peers: &mut [&mut Peer],
    what: &str,
    mut done: impl FnMut(&Host, &[&mut Peer]) -> bool,
) {
    let deadline = Instant::now() + WAIT;
    while !done(host, peers) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: host {:?} clients {:?}",
            host.net.status(),
            peers
                .iter()
                .map(|peer| (peer.client.link().clone(), peer.client.session().phase()))
                .collect::<Vec<_>>()
        );
        host.step();
        for peer in peers.iter_mut() {
            peer.step(host);
        }
        std::thread::sleep(FRAME);
    }
}

fn run_for(host: &mut Host, peers: &mut [&mut Peer], duration: Duration) {
    let end = Instant::now() + duration;
    run_until(host, peers, "the scripted play window", |_, _| {
        Instant::now() >= end
    });
}

#[test]
fn host_and_two_clients_play_over_real_quic_rejoin_and_end_cleanly() {
    let mut host = Host::start();
    let deadline = Instant::now() + WAIT;
    while host.addr().is_none() {
        assert!(Instant::now() < deadline, "host endpoint never bound");
        host.step();
        std::thread::sleep(FRAME);
    }
    let addr = host.addr().expect("bound");
    let mut alice = Peer::join(addr, "alice", None);
    let mut bob = Peer::join(addr, "bob", None);

    run_until(
        &mut host,
        &mut [&mut alice, &mut bob],
        "both welcomes",
        |_, peers| {
            peers
                .iter()
                .all(|peer| peer.client.session().phase() == ClientPhase::InGame)
        },
    );
    let alice_player = alice.player().expect("alice has a player");
    let bob_player = bob.player().expect("bob has a player");
    assert_ne!(alice_player, bob_player);

    // A few seconds of scripted play, then a quiet settle so every action resolves.
    run_for(
        &mut host,
        &mut [&mut alice, &mut bob],
        Duration::from_millis(2_500),
    );
    alice.scripted = false;
    bob.scripted = false;
    run_for(
        &mut host,
        &mut [&mut alice, &mut bob],
        Duration::from_millis(800),
    );

    for peer in [&alice, &bob] {
        let player = peer.player().expect("joined");
        let issued = peer.client.session().actions_issued();
        assert!(issued >= 2, "the script issued actions: {issued}");
        let resolved = host.resolved.get(&player).cloned().unwrap_or_default();
        let unique: BTreeSet<ActionId> = resolved.iter().copied().collect();
        assert_eq!(
            unique.len(),
            resolved.len(),
            "no action of player {player} resolved twice"
        );
        assert_eq!(
            u64::try_from(resolved.len()).expect("small"),
            issued,
            "every action of player {player} resolved exactly once"
        );
        assert!(
            peer.hash_matches > 10,
            "authority hashes were compared against the host: {}",
            peer.hash_matches
        );
        assert_eq!(peer.client.session().stats().hash_mismatches, 0);
    }
    assert!(
        host.session
            .world()
            .player(to_sim_player(alice_player))
            .is_some()
            && host
                .session
                .world()
                .player(to_sim_player(bob_player))
                .is_some()
    );

    // Bob's connection drops (no Leave): the host reserves his player for the token.
    let token = bob
        .client
        .reconnect_token()
        .expect("bob has a reconnect token");
    bob.client.drop_connection();
    run_until(
        &mut host,
        &mut [&mut alice, &mut bob],
        "the host to notice bob left",
        |host, _| {
            host.session
                .roster()
                .iter()
                .any(|entry| entry.player == bob_player && !entry.connected)
        },
    );
    assert!(matches!(bob.client.link(), ClientLink::Closed(_)));

    let mut bob_again = Peer::join(addr, "bob", Some(token));
    run_until(
        &mut host,
        &mut [&mut alice, &mut bob_again],
        "bob's rejoin",
        |_, peers| peers[1].client.session().phase() == ClientPhase::InGame,
    );
    assert_eq!(
        bob_again.player(),
        Some(bob_player),
        "the reconnect token restores the same player"
    );
    run_for(
        &mut host,
        &mut [&mut alice, &mut bob_again],
        Duration::from_millis(500),
    );
    assert!(bob_again.hash_matches > 0);

    // The host ends the session: both clients learn why, then their connections close.
    host.session.end_session(EndReason::HostClosed);
    let outbound = host.session.take_outbound();
    host.net.send(outbound);
    let Host { net, .. } = host;
    net.shutdown_blocking();
    let deadline = Instant::now() + WAIT;
    while !(alice.client.finished() && bob_again.client.finished()) {
        assert!(
            Instant::now() < deadline,
            "clients never saw the session end"
        );
        alice.client.pump(0.005);
        bob_again.client.pump(0.005);
        std::thread::sleep(FRAME);
    }
    for peer in [&alice, &bob_again] {
        assert_eq!(
            peer.client.session().phase(),
            ClientPhase::Ended(EndReason::HostClosed),
            "the client was told the host ended the session"
        );
    }
    alice.client.shutdown_blocking();
    bob_again.client.shutdown_blocking();
    bob.client.shutdown_blocking();
}
