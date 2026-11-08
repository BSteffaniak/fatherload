//! In-process loopback tests through the real network thread and real Quinn sockets.

use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use drillgame_quic_transport::{
    Channel, ConnId, Direction, DisconnectReason, HostConfig, HostDescriptor, JoinTarget,
    LinkConditionerConfig, NetConfig, NetErrorKind, NetEvent, NetHandle,
};

const WAIT: Duration = Duration::from_secs(15);

fn test_config() -> NetConfig {
    NetConfig {
        stats_interval: None,
        close_timeout: Duration::from_secs(1),
        ..NetConfig::default()
    }
}

fn loopback() -> SocketAddr {
    "127.0.0.1:0".parse().expect("address parses")
}

/// Wait for the first event `pick` accepts, discarding others. Panics after [`WAIT`].
fn wait_for<T>(net: &NetHandle, what: &str, mut pick: impl FnMut(NetEvent) -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for {what}");
        if let Some(event) = net.recv_timeout(remaining.min(Duration::from_millis(50)))
            && let Some(found) = pick(event)
        {
            return found;
        }
    }
}

fn start_host(config: NetConfig) -> (NetHandle, HostDescriptor) {
    let host = NetHandle::start(config).expect("network thread starts");
    host.host(loopback(), HostConfig::default())
        .expect("host command sent");
    let descriptor = wait_for(&host, "HostReady", |event| match event {
        NetEvent::HostReady { descriptor, .. } => Some(descriptor),
        NetEvent::Error(error) => panic!("host failed: {error}"),
        _ => None,
    });
    (host, descriptor)
}

/// Join and return `(client handle, client-side conn, host-side conn)`.
fn join(host: &NetHandle, target: JoinTarget, config: NetConfig) -> (NetHandle, ConnId, ConnId) {
    let client = NetHandle::start(config).expect("network thread starts");
    client.join(target).expect("join command sent");
    let client_conn = wait_for(&client, "client Connected", |event| match event {
        NetEvent::Connected {
            conn, direction, ..
        } => {
            assert_eq!(direction, Direction::Outbound);
            Some(conn)
        }
        NetEvent::Error(error) => panic!("join failed: {error}"),
        _ => None,
    });
    let host_conn = wait_for(host, "host Connected", |event| match event {
        NetEvent::Connected {
            conn, direction, ..
        } => {
            assert_eq!(direction, Direction::Inbound);
            Some(conn)
        }
        _ => None,
    });
    (client, client_conn, host_conn)
}

fn join_descriptor(host: &NetHandle, descriptor: &HostDescriptor) -> (NetHandle, ConnId, ConnId) {
    join(
        host,
        JoinTarget::Descriptor(descriptor.clone()),
        test_config(),
    )
}

fn sequenced_payload(seq: u32, len: usize) -> Vec<u8> {
    let mut payload = vec![0_u8; len.max(4)];
    payload[..4].copy_from_slice(&seq.to_be_bytes());
    for (index, byte) in payload.iter_mut().enumerate().skip(4) {
        *byte = u8::try_from((index + seq as usize) % 251).expect("fits u8");
    }
    payload
}

fn check_payload(bytes: &[u8]) -> u32 {
    let seq = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    assert_eq!(
        bytes,
        sequenced_payload(seq, bytes.len()),
        "payload corrupt"
    );
    seq
}

const fn message_len(seq: u32) -> usize {
    if seq % 500 == 250 {
        1024 * 1024
    } else {
        16 + (seq as usize % 200)
    }
}

#[test]
fn reliable_messages_stay_ordered_across_thousands_including_large() {
    const COUNT: u32 = 3_000;
    let (host, descriptor) = start_host(test_config());
    let (client_a, a_conn, host_a) = join_descriptor(&host, &descriptor);
    let (client_b, b_conn, host_b) = join_descriptor(&host, &descriptor);

    for seq in 0..COUNT {
        let payload = sequenced_payload(seq, message_len(seq));
        client_a
            .send_reliable(a_conn, payload.clone())
            .expect("send");
        client_b
            .send_reliable(b_conn, payload.clone())
            .expect("send");
        host.send_reliable(host_a, payload.clone()).expect("send");
        host.send_reliable(host_b, payload).expect("send");
    }

    // Host receives both clients' streams, each in order.
    let mut next: BTreeMap<ConnId, u32> = BTreeMap::new();
    wait_for(&host, "all client messages", |event| {
        if let NetEvent::Message {
            conn,
            channel,
            bytes,
        } = event
        {
            assert_eq!(channel, Channel::Reliable);
            let expected = next.entry(conn).or_insert(0);
            assert_eq!(check_payload(&bytes), *expected, "out of order on {conn}");
            assert_eq!(bytes.len(), message_len(*expected));
            *expected += 1;
        }
        (next.get(&host_a) == Some(&COUNT) && next.get(&host_b) == Some(&COUNT)).then_some(())
    });

    for (client, conn) in [(&client_a, a_conn), (&client_b, b_conn)] {
        let mut expected = 0;
        wait_for(client, "all host messages", |event| {
            if let NetEvent::Message {
                conn: from, bytes, ..
            } = event
            {
                assert_eq!(from, conn);
                assert_eq!(check_payload(&bytes), expected, "out of order");
                expected += 1;
            }
            (expected == COUNT).then_some(())
        });
    }
}

#[test]
fn datagrams_are_delivered_and_oversized_ones_fall_back_to_the_stream() {
    let (host, descriptor) = start_host(test_config());
    let (client, client_conn, _host_conn) = join_descriptor(&host, &descriptor);

    for seq in 0..100 {
        client
            .send_unreliable(client_conn, sequenced_payload(seq, 64))
            .expect("send");
    }
    // Larger than any datagram: must arrive once via the control stream, still `Unreliable`.
    client
        .send_unreliable(client_conn, sequenced_payload(1_000, 200_000))
        .expect("send");

    let mut small = 0;
    wait_for(&host, "oversized unreliable message", |event| match event {
        NetEvent::Message { channel, bytes, .. } => {
            assert_eq!(channel, Channel::Unreliable);
            let seq = check_payload(&bytes);
            if seq == 1_000 {
                assert_eq!(bytes.len(), 200_000);
                Some(())
            } else {
                small += 1;
                None
            }
        }
        _ => None,
    });
    // Collect stragglers briefly; loopback should deliver nearly all small datagrams.
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        if let Some(NetEvent::Message { .. }) = host.recv_timeout(Duration::from_millis(20)) {
            small += 1;
        }
    }
    assert!(small >= 50, "only {small}/100 datagrams arrived");
}

#[test]
fn host_accepts_many_clients() {
    const CLIENTS: usize = 12;
    let (host, descriptor) = start_host(test_config());
    let mut clients = Vec::new();
    let mut host_conns = Vec::new();
    for _ in 0..CLIENTS {
        let (client, client_conn, host_conn) = join_descriptor(&host, &descriptor);
        clients.push((client, client_conn));
        host_conns.push(host_conn);
    }
    host_conns.sort_unstable();
    host_conns.dedup();
    assert_eq!(host_conns.len(), CLIENTS);

    for (index, conn) in host_conns.iter().enumerate() {
        host.send_reliable(*conn, vec![u8::try_from(index).expect("fits")])
            .expect("send");
    }
    for (client, _conn) in &clients {
        wait_for(client, "broadcast", |event| {
            matches!(event, NetEvent::Message { .. }).then_some(())
        });
    }
}

#[test]
fn disconnect_flushes_reliable_messages_and_reports_reasons() {
    let (host, descriptor) = start_host(test_config());
    let (client_a, a_conn, host_a) = join_descriptor(&host, &descriptor);
    let (client_b, _b_conn, host_b) = join_descriptor(&host, &descriptor);

    for seq in 0..200 {
        client_a
            .send_reliable(a_conn, sequenced_payload(seq, 1_000))
            .expect("send");
    }
    client_a.disconnect(a_conn, "bye").expect("disconnect");

    let mut received = 0;
    let reason = wait_for(&host, "client A disconnect", |event| match event {
        NetEvent::Message { conn, bytes, .. } if conn == host_a => {
            assert_eq!(check_payload(&bytes), received);
            received += 1;
            None
        }
        NetEvent::Disconnected { conn, reason } if conn == host_a => Some(reason),
        _ => None,
    });
    assert_eq!(received, 200, "every message precedes Disconnected");
    assert_eq!(reason, DisconnectReason::Remote("bye".to_owned()));
    let local = wait_for(
        &client_a,
        "client A local disconnect",
        |event| match event {
            NetEvent::Disconnected { reason, .. } => Some(reason),
            _ => None,
        },
    );
    assert_eq!(local, DisconnectReason::Local("bye".to_owned()));

    host.disconnect(host_b, "kicked").expect("disconnect");
    let kicked = wait_for(&client_b, "client B kicked", |event| match event {
        NetEvent::Disconnected { reason, .. } => Some(reason),
        _ => None,
    });
    assert_eq!(kicked, DisconnectReason::Remote("kicked".to_owned()));

    // Sending to a closed connection reports an error instead of panicking or hanging.
    host.send_reliable(host_a, b"late".to_vec()).expect("send");
    let error = wait_for(&host, "send error", |event| match event {
        NetEvent::Error(error) => Some(error),
        _ => None,
    });
    assert_eq!(error.kind, NetErrorKind::Send);
}

#[test]
fn shutdown_is_prompt_and_notifies_peers() {
    let (host, descriptor) = start_host(test_config());
    let (client_a, _, _) = join_descriptor(&host, &descriptor);
    let (client_b, _, _) = join_descriptor(&host, &descriptor);

    let started = Instant::now();
    let remaining = host.shutdown();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown took {:?}",
        started.elapsed()
    );
    assert_eq!(remaining.last(), Some(&NetEvent::ShutdownComplete));
    let local_disconnects = remaining
        .iter()
        .filter(|event| {
            matches!(
                event,
                NetEvent::Disconnected { reason: DisconnectReason::Local(reason), .. }
                    if reason == "shutdown"
            )
        })
        .count();
    assert_eq!(local_disconnects, 2);

    for client in [&client_a, &client_b] {
        let reason = wait_for(client, "host shutdown", |event| match event {
            NetEvent::Disconnected { reason, .. } => Some(reason),
            _ => None,
        });
        assert_eq!(reason, DisconnectReason::Remote("shutdown".to_owned()));
    }

    // An idle handle drops promptly too.
    let started = Instant::now();
    drop(client_a);
    drop(NetHandle::start(test_config()).expect("starts"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn conditioner_delays_reliable_messages_and_preserves_order() {
    let latency = Duration::from_millis(150);
    let config = NetConfig {
        link: LinkConditionerConfig {
            latency,
            jitter: Duration::from_millis(40),
            seed: Some(7),
            ..LinkConditionerConfig::default()
        },
        ..test_config()
    };
    let (host, descriptor) = start_host(config);
    let (client, _client_conn, host_conn) = join_descriptor(&host, &descriptor);

    let sent_at = Instant::now();
    for seq in 0..300 {
        host.send_reliable(host_conn, sequenced_payload(seq, 32))
            .expect("send");
    }
    let mut expected = 0;
    let mut first_arrival = None;
    wait_for(&client, "delayed messages", |event| {
        if let NetEvent::Message { bytes, .. } = event {
            first_arrival.get_or_insert_with(|| sent_at.elapsed());
            assert_eq!(check_payload(&bytes), expected, "conditioner reordered");
            expected += 1;
        }
        (expected == 300).then_some(())
    });
    let first = first_arrival.expect("arrived");
    assert!(
        first >= latency - Duration::from_millis(40),
        "first message arrived after only {first:?}"
    );
}

#[test]
fn conditioner_drops_and_duplicates_unreliable_messages() {
    let lossy = NetConfig {
        link: "loss=100,seed=1".parse().expect("spec parses"),
        ..test_config()
    };
    let (host, descriptor) = start_host(test_config());
    let (client, client_conn, _) = join(&host, JoinTarget::Descriptor(descriptor.clone()), lossy);
    for seq in 0..20 {
        client
            .send_unreliable(client_conn, sequenced_payload(seq, 16))
            .expect("send");
    }
    client
        .send_reliable(client_conn, b"sentinel".to_vec())
        .expect("send");
    let mut unreliable = 0;
    wait_for(&host, "sentinel", |event| match event {
        NetEvent::Message {
            channel: Channel::Unreliable,
            ..
        } => {
            unreliable += 1;
            None
        }
        NetEvent::Message { bytes, .. } => (bytes == b"sentinel").then_some(()),
        _ => None,
    });
    std::thread::sleep(Duration::from_millis(100));
    unreliable += host
        .drain()
        .iter()
        .filter(|event| matches!(event, NetEvent::Message { .. }))
        .count();
    assert_eq!(unreliable, 0, "loss=100 must drop every datagram");

    let duplicating = NetConfig {
        link: "dup=100,seed=1".parse().expect("spec parses"),
        ..test_config()
    };
    let (dup_client, dup_conn, _) = join(&host, JoinTarget::Descriptor(descriptor), duplicating);
    for seq in 0..20 {
        dup_client
            .send_unreliable(dup_conn, sequenced_payload(seq, 16))
            .expect("send");
    }
    let mut copies = 0;
    wait_for(&host, "duplicated datagrams", |event| {
        if matches!(
            event,
            NetEvent::Message {
                channel: Channel::Unreliable,
                ..
            }
        ) {
            copies += 1;
        }
        (copies > 20).then_some(())
    });
}

#[test]
fn direct_ip_join_works_and_wrong_certificate_is_rejected() {
    let (host, descriptor) = start_host(test_config());
    let (client, client_conn, _) =
        join(&host, JoinTarget::Addr(descriptor.host_addr), test_config());
    client
        .send_reliable(client_conn, b"hello".to_vec())
        .expect("send");
    wait_for(&host, "direct-ip hello", |event| match event {
        NetEvent::Message { bytes, .. } => (bytes == b"hello").then_some(()),
        _ => None,
    });

    let (_other_host, other_descriptor) = start_host(test_config());
    let forged = HostDescriptor {
        certificate_der: other_descriptor.certificate_der,
        ..descriptor
    };
    let rejected = NetHandle::start(test_config()).expect("starts");
    rejected
        .join(JoinTarget::Descriptor(forged))
        .expect("join sent");
    let error = wait_for(&rejected, "join error", |event| match event {
        NetEvent::Error(error) => Some(error),
        NetEvent::Connected { .. } => panic!("connected despite certificate mismatch"),
        _ => None,
    });
    assert_eq!(error.kind, NetErrorKind::Join);
}

#[test]
fn frame_size_limit_is_enforced_on_send_and_receive() {
    let strict = NetConfig {
        max_frame_bytes: 1_024,
        ..test_config()
    };
    let (host, descriptor) = start_host(strict.clone());
    let (strict_client, strict_conn, _) =
        join(&host, JoinTarget::Descriptor(descriptor.clone()), strict);
    strict_client
        .send_reliable(strict_conn, vec![0; 2_048])
        .expect("send");
    let error = wait_for(&strict_client, "send error", |event| match event {
        NetEvent::Error(error) => Some(error),
        _ => None,
    });
    assert_eq!(error.kind, NetErrorKind::Send);

    // A peer with a larger limit sending an oversized frame is a protocol violation.
    let (loose_client, loose_conn, host_conn) = join_descriptor(&host, &descriptor);
    loose_client
        .send_reliable(loose_conn, vec![0; 2_048])
        .expect("send");
    let reason = wait_for(&host, "protocol disconnect", |event| match event {
        NetEvent::Disconnected { conn, reason } if conn == host_conn => Some(reason),
        _ => None,
    });
    assert!(
        matches!(reason, DisconnectReason::Protocol(_)),
        "{reason:?}"
    );
    wait_for(&loose_client, "client sees disconnect", |event| {
        matches!(event, NetEvent::Disconnected { .. }).then_some(())
    });
}

#[test]
fn stats_are_reported_periodically() {
    let config = NetConfig {
        stats_interval: Some(Duration::from_millis(100)),
        ..test_config()
    };
    let (host, descriptor) = start_host(config);
    let (_client, _, host_conn) = join_descriptor(&host, &descriptor);
    let stats = wait_for(&host, "stats", |event| match event {
        NetEvent::Stats { conn, stats } if conn == host_conn => Some(stats),
        _ => None,
    });
    assert!(stats.sent_packets > 0);
    assert!(stats.max_datagram_size.is_some());
    assert!((0.0..=1.0).contains(&stats.loss));
}
