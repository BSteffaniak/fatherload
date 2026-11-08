//! Component-level tests.

use std::time::Duration;

use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::toy::{ToyAction, ToyControls, ToySim};
use crate::{
    ActionEnvelope, ActionId, ActionLedger, ClientClock, ClientConfig, ClientId, ClientPhase,
    ClientSession, ClockSpeed, CodecError, EventSource, HostConfig, HostSession, InputFrame,
    InputSeq, LinkConditions, LinkSim, MarginResponse, NetSimulation, NetTuning, PROTOCOL_VERSION,
    PlayerId, Pose, SeatId, SessionEpoch, SessionParams, SnapshotBuffer, Tick, VisualOffset,
};

fn params60() -> SessionParams {
    SessionParams::new(60)
}

fn toy_world(params: SessionParams) -> ToySim {
    ToySim::new(&params, 4, 32)
}

#[test]
fn ms_conversions_follow_tick_rate() {
    for rate in [30, 60, 120] {
        let params = SessionParams::new(rate);
        assert_eq!(params.ms_to_ticks_ceil(1000), u64::from(rate));
        assert_eq!(params.ms_to_ticks_ceil(0), 0);
        assert!(params.ms_to_ticks_ceil(1) >= 1);
    }
    let params = params60();
    assert_eq!(params.ms_to_ticks_ceil(100), 6);
    assert_eq!(params.ms_to_ticks_ceil(250), 15);
    assert_eq!(NetTuning::default().input_lead_target_ticks(&params), 2);
    assert_eq!(
        NetTuning::default().input_lead_target_ticks(&SessionParams::new(30)),
        1
    );
}

#[test]
fn codec_round_trips_and_rejects_wrong_version() {
    let message: ClientMessage<ToySim> = ClientMessage::Inputs {
        epoch: SessionEpoch(3),
        frames: vec![InputFrame {
            seq: InputSeq(9),
            target_tick: Tick(100),
            controls: ToyControls {
                move_x: -1,
                move_y: 1,
                drill: true,
            },
            actions: vec![ActionEnvelope {
                id: ActionId(4),
                action: ToyAction::Bomb,
            }],
        }],
        actions: Vec::new(),
        action_floor: ActionId(4),
    };
    let bytes = protocol::encode(&message).unwrap();
    assert_eq!(bytes[0], PROTOCOL_VERSION);
    let decoded: ClientMessage<ToySim> = protocol::decode(&bytes).unwrap();
    let ClientMessage::Inputs { epoch, frames, .. } = decoded else {
        panic!("wrong variant");
    };
    assert_eq!(epoch, SessionEpoch(3));
    assert_eq!(frames[0].controls.move_x, -1);
    assert_eq!(frames[0].actions[0].id, ActionId(4));

    let mut wrong = bytes;
    wrong[0] = PROTOCOL_VERSION.wrapping_add(1);
    assert!(matches!(
        protocol::decode::<ClientMessage<ToySim>>(&wrong),
        Err(CodecError::Version { .. })
    ));
    assert!(matches!(
        protocol::decode::<ClientMessage<ToySim>>(&[]),
        Err(CodecError::Empty)
    ));
}

#[test]
fn ledger_applies_each_action_once_and_stays_bounded() {
    let mut ledger = ActionLedger::new();
    assert!(ledger.try_apply(ActionId(2)));
    assert!(!ledger.try_apply(ActionId(2)));
    assert_eq!(ledger.watermark(), ActionId(0));
    assert!(ledger.try_apply(ActionId(1)));
    assert_eq!(ledger.watermark(), ActionId(2));
    assert_eq!(ledger.backlog(), 0);
    assert!(ledger.try_apply(ActionId(5)));
    ledger.raise_floor(ActionId(5));
    assert_eq!(ledger.watermark(), ActionId(5));
    assert!(!ledger.try_apply(ActionId(4)));
    assert_eq!(ledger.backlog(), 0);
}

#[test]
fn link_sim_is_deterministic_and_reliable_mode_preserves_order() {
    let conditions = LinkConditions::new(50, 20, 10.0).with_chaos(5.0, 10.0);
    let run = |reliable: bool| {
        let mut link: LinkSim<u32> = LinkSim::new(conditions, reliable, 42);
        let mut out = Vec::new();
        for i in 0..2000u32 {
            let now = Duration::from_millis(u64::from(i));
            link.send(now, i);
            out.extend(link.poll(now));
        }
        out.extend(link.poll(Duration::from_secs(100)));
        out
    };
    assert_eq!(run(false), run(false));
    let unreliable = run(false);
    assert!(unreliable.len() < 2000 + 200, "duplicates bounded");
    assert!(
        unreliable.windows(2).any(|w| w[0] > w[1]),
        "reordering happens"
    );
    let reliable = run(true);
    assert_eq!(
        reliable,
        (0..2000).collect::<Vec<_>>(),
        "reliable is lossless and ordered"
    );
}

#[test]
fn clock_adjusts_speed_and_resyncs() {
    let params = params60();
    let tuning = NetTuning::default();
    let mut clock = ClientClock::new();
    clock.start(Tick(100));
    // In band (target 2): steady.
    assert_eq!(
        clock.on_margin(2, InputSeq(1), InputSeq(2), &params, &tuning),
        MarginResponse::Steady
    );
    // Late: speed up, then back to normal once at target.
    assert_eq!(
        clock.on_margin(0, InputSeq(2), InputSeq(3), &params, &tuning),
        MarginResponse::Adjusted(ClockSpeed::Fast)
    );
    assert_eq!(
        clock.on_margin(1, InputSeq(3), InputSeq(4), &params, &tuning),
        MarginResponse::Steady
    );
    assert_eq!(
        clock.on_margin(2, InputSeq(4), InputSeq(5), &params, &tuning),
        MarginResponse::Adjusted(ClockSpeed::Normal)
    );
    // Early: slow down.
    assert_eq!(
        clock.on_margin(6, InputSeq(5), InputSeq(6), &params, &tuning),
        MarginResponse::Adjusted(ClockSpeed::Slow)
    );
    // Far behind: hard resync forward; stale reports are then ignored.
    assert_eq!(
        clock.on_margin(-30, InputSeq(6), InputSeq(50), &params, &tuning),
        MarginResponse::Resynced { delta: 32 }
    );
    assert_eq!(
        clock.on_margin(-30, InputSeq(10), InputSeq(51), &params, &tuning),
        MarginResponse::Ignored
    );
    // Jump ticks are delivered on top of normal ticks, clamped per call.
    let due = clock.advance(Duration::from_millis(17), &params, &tuning);
    assert_eq!(due, 1 + tuning.max_catch_up_ticks);
}

#[test]
fn clock_speed_adjust_changes_tick_rate_by_percentage() {
    let params = params60();
    let tuning = NetTuning::default();
    let count = |speed_margin: Option<i64>| {
        let mut clock = ClientClock::new();
        clock.start(Tick(0));
        if let Some(margin) = speed_margin {
            let _ = clock.on_margin(margin, InputSeq(1), InputSeq(1), &params, &tuning);
        }
        let mut ticks = 0;
        for _ in 0..1000 {
            ticks += clock.advance(Duration::from_millis(10), &params, &tuning);
        }
        ticks
    };
    assert_eq!(count(None), 600);
    assert_eq!(count(Some(0)), 618);
    assert_eq!(count(Some(10)), 582);
}

#[test]
fn snapshot_buffer_interpolates_and_caps_extrapolation() {
    let mut buffer = SnapshotBuffer::new(8);
    let player = PlayerId(1);
    buffer.push(player, Tick(10), Pose::new(0.0, 0.0, 0));
    buffer.push(player, Tick(12), Pose::new(2.0, 0.0, 0));
    buffer.push(player, Tick(12), Pose::new(99.0, 0.0, 0));
    let mid = buffer.sample(player, 11.0, 6.0).unwrap();
    assert!((mid.x - 1.0).abs() < 1e-6);
    let extrapolated = buffer.sample(player, 14.0, 6.0).unwrap();
    assert!((extrapolated.x - 4.0).abs() < 1e-6);
    let frozen = buffer.sample(player, 100.0, 6.0).unwrap();
    assert!(
        (frozen.x - 8.0).abs() < 1e-6,
        "extrapolation capped at 6 ticks"
    );
    for t in 13..40 {
        buffer.push(player, Tick(t), Pose::default());
    }
    assert!(buffer.depth(player) <= 8);
}

#[test]
fn visual_offset_decays_by_half_life() {
    let mut offset = VisualOffset::default();
    offset.add(1.0, 0.0);
    offset.decay(Duration::from_millis(50), 50);
    assert!((offset.x - 0.5).abs() < 1e-4);
    offset.decay(Duration::from_millis(500), 50);
    assert!(offset.magnitude() < 1e-3);
}

#[test]
fn host_advances_only_on_fixed_ticks_with_exact_accumulator() {
    for rate in [30u32, 60, 120] {
        let params = SessionParams::new(rate);
        let mut host = HostSession::new(
            toy_world(params),
            HostConfig {
                params,
                ..HostConfig::default()
            },
        );
        host.add_local_player(SeatId(0));
        // Odd frame times must still produce exactly `rate` ticks per second.
        let mut total = 0;
        for _ in 0..1000 {
            total += host.advance(Duration::from_millis(7)).ticks_run;
        }
        assert_eq!(u64::from(total), u64::from(rate) * 7);
        assert_eq!(host.tick(), Tick(u64::from(rate) * 7));
    }
}

#[test]
fn host_catch_up_is_clamped() {
    let params = params60();
    let mut host = HostSession::new(toy_world(params), HostConfig::default());
    let output = host.advance(Duration::from_secs(2));
    assert_eq!(output.ticks_run, NetTuning::default().max_catch_up_ticks);
    assert_eq!(host.stats().catch_up_clamps, 1);
}

#[test]
fn host_never_advances_on_packet_arrival() {
    let params = params60();
    let mut host = HostSession::new(toy_world(params), HostConfig::default());
    let client = ClientId(1);
    host.client_connected(client);
    host.handle_client_message(
        client,
        ClientMessage::Hello {
            token: None,
            name: "a".into(),
            version: PROTOCOL_VERSION,
        },
    );
    let before = host.tick();
    for seq in 1..50 {
        host.handle_client_message(
            client,
            ClientMessage::Inputs {
                epoch: host.epoch(),
                frames: vec![InputFrame {
                    seq: InputSeq(seq),
                    target_tick: Tick(seq),
                    controls: ToyControls::default(),
                    actions: Vec::new(),
                }],
                actions: Vec::new(),
                action_floor: ActionId(1),
            },
        );
    }
    assert_eq!(host.tick(), before);
}

fn welcomed_host(params: SessionParams) -> (HostSession<ToySim>, ClientId, PlayerId) {
    let mut host = HostSession::new(
        toy_world(params),
        HostConfig {
            params,
            ..HostConfig::default()
        },
    );
    let client = ClientId(7);
    host.client_connected(client);
    host.handle_client_message(
        client,
        ClientMessage::Hello {
            token: None,
            name: "a".into(),
            version: PROTOCOL_VERSION,
        },
    );
    let player = host.client_player(client).unwrap();
    let _ = host.take_outbound();
    (host, client, player)
}

fn inputs(epoch: SessionEpoch, frames: Vec<InputFrame<ToySim>>) -> ClientMessage<ToySim> {
    ClientMessage::Inputs {
        epoch,
        frames,
        actions: Vec::new(),
        action_floor: ActionId(1),
    }
}

fn frame(
    seq: u64,
    tick: u64,
    move_x: i8,
    actions: Vec<ActionEnvelope<ToyAction>>,
) -> InputFrame<ToySim> {
    InputFrame {
        seq: InputSeq(seq),
        target_tick: Tick(tick),
        controls: ToyControls {
            move_x,
            move_y: 0,
            drill: false,
        },
        actions,
    }
}

#[test]
fn host_holds_controls_then_goes_neutral_and_applies_late_actions_once() {
    let params = params60();
    let (mut host, client, player) = welcomed_host(params);
    let tick = |host: &HostSession<ToySim>| host.tick().0;
    let epoch = host.epoch();
    host.handle_client_message(
        client,
        inputs(epoch, vec![frame(1, tick(&host) + 1, 1, vec![])]),
    );
    let _ = host.advance(params.tick_duration());
    assert_eq!(host.client_stats(client).unwrap().frames_on_time, 1);
    // No more input: held for held_input_ms (6 ticks), then neutral.
    for _ in 0..10 {
        let _ = host.advance(params.tick_duration());
    }
    let stats = host.client_stats(client).unwrap().clone();
    assert_eq!(stats.held_ticks, 6);
    assert_eq!(stats.neutral_ticks, 4);

    // A late frame with an action: controls dropped, action applied next tick.
    let late = frame(
        2,
        tick(&host) - 3,
        1,
        vec![ActionEnvelope {
            id: ActionId(1),
            action: ToyAction::Bomb,
        }],
    );
    host.handle_client_message(client, inputs(epoch, vec![late.clone()]));
    host.handle_client_message(client, inputs(epoch, vec![late]));
    let output = host.advance(params.tick_duration());
    assert_eq!(
        output
            .events
            .iter()
            .filter(|e| e.kind == crate::toy::event_kind::ACTION_APPLIED)
            .count(),
        1
    );
    assert_eq!(host.world().bomb_count(), 1);
    assert_eq!(host.world().wallet(player).unwrap().actions_applied, 1);
    // Re-sending it again (redundancy) never re-applies it.
    let again = frame(
        2,
        tick(&host) - 4,
        1,
        vec![ActionEnvelope {
            id: ActionId(1),
            action: ToyAction::Bomb,
        }],
    );
    host.handle_client_message(client, inputs(epoch, vec![again]));
    let _ = host.advance(params.tick_duration());
    assert_eq!(host.world().wallet(player).unwrap().actions_applied, 1);
}

#[test]
fn host_acks_only_after_simulation_and_reports_margin() {
    let params = params60();
    let (mut host, client, _) = welcomed_host(params);
    let epoch = host.epoch();
    let now = host.tick().0;
    host.handle_client_message(client, inputs(epoch, vec![frame(1, now + 3, 0, vec![])]));
    // Advance one tick: frame due (snapshot 30Hz at 60Hz means every 2nd tick).
    let mut frames = Vec::new();
    for _ in 0..4 {
        for (_, message) in host.advance(params.tick_duration()).outbound {
            if let ServerMessage::Frame(frame) = message {
                frames.push(*frame);
            }
        }
    }
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].ack_seq, None, "not simulated yet at tick +2");
    assert_eq!(frames[0].input_margin, Some(3));
    assert_eq!(frames[1].ack_seq, Some(InputSeq(1)));
    assert_eq!(frames[1].input_margin, None);
}

#[test]
fn host_rejects_stale_epoch_and_far_future_inputs() {
    let params = params60();
    let (mut host, client, _) = welcomed_host(params);
    let stale = SessionEpoch(host.epoch().0 - 1);
    host.handle_client_message(client, inputs(stale, vec![frame(1, 5, 0, vec![])]));
    assert_eq!(host.stats().stale_epoch_inputs, 1);
    let epoch = host.epoch();
    host.handle_client_message(client, inputs(epoch, vec![frame(2, 100_000, 0, vec![])]));
    assert_eq!(host.client_stats(client).unwrap().frames_too_far_ahead, 1);
    assert_eq!(host.buffered_inputs(client), 0);
}

#[test]
fn host_rejects_wrong_protocol_version() {
    let params = params60();
    let mut host = HostSession::new(toy_world(params), HostConfig::default());
    host.client_connected(ClientId(1));
    host.handle_client_message(
        ClientId(1),
        ClientMessage::Hello {
            token: None,
            name: "old".into(),
            version: PROTOCOL_VERSION + 1,
        },
    );
    let out = host.take_outbound();
    assert!(matches!(out[0].1, ServerMessage::Rejected { .. }));
    assert_eq!(host.client_player(ClientId(1)), None);
}

#[test]
fn local_seats_can_adopt_existing_world_players() {
    let params = params60();
    let mut world = toy_world(params);
    world.add_player(PlayerId(7));
    let mut host = HostSession::new(world, HostConfig::default());
    assert!(
        !host.adopt_local_player(SeatId(0), PlayerId(3)),
        "not in world"
    );
    assert!(host.adopt_local_player(SeatId(0), PlayerId(7)));
    assert_eq!(host.seat_player(SeatId(0)), Some(PlayerId(7)));
    assert!(
        !host.adopt_local_player(SeatId(1), PlayerId(7)),
        "already owned"
    );
    assert!(
        !host.adopt_local_player(SeatId(0), PlayerId(7)),
        "seat bound"
    );
    let fresh = host.add_local_player(SeatId(1));
    assert!(fresh.0 > 7, "new players never collide with adopted ids");
    assert_eq!(host.world().player_ids(), vec![PlayerId(7), fresh]);
    assert!(host.roster().iter().all(|entry| entry.local));
}

#[test]
fn host_output_reports_terrain_ops_for_local_presentation() {
    let params = params60();
    let mut host = HostSession::new(toy_world(params), HostConfig::default());
    let a = host.add_local_player(SeatId(0));
    let mut ops = 0;
    for _ in 0..120 {
        host.submit_local_input(
            a,
            ToyControls {
                move_x: 0,
                move_y: 1,
                drill: true,
            },
            Vec::new(),
        );
        let output = host.advance(params.tick_duration());
        assert!(output.ops.iter().all(|(tick, _)| *tick <= host.tick()));
        ops += output.ops.len();
    }
    assert!(ops > 0, "drilling produced terrain ops");
}

#[test]
fn local_seats_play_without_any_clients() {
    let params = params60();
    let mut host = HostSession::new(toy_world(params), HostConfig::default());
    let a = host.add_local_player(SeatId(0));
    let b = host.add_local_player(SeatId(1));
    assert_ne!(a, b);
    assert_eq!(host.add_local_player(SeatId(0)), a);
    let start = host.world().player(a).unwrap().x;
    for _ in 0..30 {
        host.submit_local_input(
            a,
            ToyControls {
                move_x: 1,
                move_y: 0,
                drill: false,
            },
            Vec::new(),
        );
        let _ = host.advance(params.tick_duration());
    }
    assert!(host.world().player(a).unwrap().x > start);
    host.submit_local_input(b, ToyControls::default(), vec![ToyAction::Bomb]);
    let _ = host.advance(params.tick_duration());
    assert_eq!(host.world().bomb_count(), 1);
    host.set_paused(true);
    let tick = host.tick();
    let _ = host.advance(Duration::from_secs(1));
    assert_eq!(host.tick(), tick);
}

#[test]
fn client_rejects_stale_epoch_frames() {
    let params = params60();
    let (mut host, client_id, _) = welcomed_host(params);
    // Re-welcome a client session through the host so it has a world.
    let mut session: ClientSession<ToySim> = ClientSession::new(ClientConfig::default());
    host.client_connected(ClientId(8));
    for message in session.take_outbound() {
        host.handle_client_message(ClientId(8), message);
    }
    for (to, message) in host.take_outbound() {
        if to == ClientId(8) {
            session.handle_server_message(message);
        }
    }
    assert_eq!(session.phase(), ClientPhase::InGame);
    let mut frames = Vec::new();
    for _ in 0..4 {
        for (to, message) in host.advance(params.tick_duration()).outbound {
            if to == ClientId(8) {
                frames.push(message);
            }
        }
    }
    let _ = client_id;
    // Epoch change on the host; old frames delivered afterwards are rejected.
    host.new_epoch(toy_world(params));
    for (to, message) in host.take_outbound() {
        if to == ClientId(8) {
            session.handle_server_message(message);
        }
    }
    assert_eq!(session.epoch(), host.epoch());
    let before = session.stats().stale_frames;
    for message in frames {
        session.handle_server_message(message);
    }
    assert!(session.stats().stale_frames > before);
    assert_eq!(session.stats().frames_received, 0);
}

#[test]
fn prediction_events_are_deduplicated_against_authority() {
    let params = params60();
    let mut host = HostSession::new(
        toy_world(params),
        HostConfig {
            params,
            ..HostConfig::default()
        },
    );
    let mut session: ClientSession<ToySim> = ClientSession::new(ClientConfig::default());
    host.client_connected(ClientId(1));
    let pump = |host: &mut HostSession<ToySim>,
                session: &mut ClientSession<ToySim>,
                dt: Duration,
                actions: Vec<ToyAction>| {
        let out = host.advance(dt);
        for (_, message) in out.outbound {
            session.handle_server_message(message);
        }
        for message in session.advance(dt, &ToyControls::default(), actions) {
            host.handle_client_message(ClientId(1), message);
        }
    };
    for message in session.take_outbound() {
        host.handle_client_message(ClientId(1), message);
    }
    let dt = params.tick_duration();
    for _ in 0..10 {
        pump(&mut host, &mut session, dt, Vec::new());
    }
    let _ = session.drain_events();
    pump(&mut host, &mut session, dt, vec![ToyAction::Bomb]);
    for _ in 0..120 {
        pump(&mut host, &mut session, dt, Vec::new());
    }
    let events = session.drain_events();
    let applied: Vec<_> = events
        .iter()
        .filter(|e| e.event.kind == crate::toy::event_kind::ACTION_APPLIED)
        .collect();
    assert_eq!(applied.len(), 1, "played exactly once: {events:?}");
    assert_eq!(applied[0].source, EventSource::Predicted);
    let exploded = events
        .iter()
        .filter(|e| e.event.kind == crate::toy::event_kind::EXPLODED)
        .count();
    assert_eq!(exploded, 1);
    assert!(session.stats().deduplicated_events >= 2);
    assert_eq!(session.predicted_event_keys(), 0);
}

#[test]
fn toy_sim_replicates_through_trait_surface() {
    let params = params60();
    let mut host_world = toy_world(params);
    host_world.add_player(PlayerId(0));
    let mut copy = host_world.clone();
    let mut inputs = crate::TickInputs::empty(Tick(1));
    inputs.players.insert(
        PlayerId(0),
        crate::PlayerTickInput {
            controls: ToyControls {
                move_x: 0,
                move_y: 1,
                drill: true,
            },
            actions: vec![ActionEnvelope {
                id: ActionId(1),
                action: ToyAction::Bomb,
            }],
        },
    );
    let mut ops = Vec::new();
    for tick in 1..200 {
        inputs.tick = Tick(tick);
        let out = host_world.simulate_tick(&inputs);
        ops.extend(out.ops);
        inputs
            .players
            .get_mut(&PlayerId(0))
            .unwrap()
            .actions
            .clear();
    }
    assert!(!ops.is_empty(), "the bot dug something");
    copy.set_tick(host_world.tick());
    copy.apply_player_state(PlayerId(0), host_world.player_state(PlayerId(0)).unwrap());
    copy.apply_entities(host_world.entities(&crate::Relevance::everything()));
    for op in &ops {
        copy.apply_op(op);
    }
    for key in host_world.section_keys() {
        copy.apply_section(&key, host_world.section(&key).unwrap());
    }
    assert_eq!(copy.state_hash(), host_world.state_hash());
}
