//! Window-less modes of the game binary (plan §11.4): a headless host and scripted bot clients,
//! built from exactly the pieces the app uses ([`HostNet`] + `HostSession`, [`OnlineClient`]).
//!
//! ```text
//! drillgame --headless-host [addr] [--duration SECS] [--lan]
//! drillgame --bot-client <addr> [--count N] [--duration SECS]
//! ```
//!
//! Both print `key=value` stats lines periodically. The host prints `HOST_READY addr=<addr>`
//! once bound. With `--duration` they end cleanly after that long (the host ends the session for
//! its clients; bots leave); bots also exit cleanly when the host ends the session.

use std::{
    io::Write,
    net::SocketAddr,
    time::{Duration, Instant},
};

use drillgame_netcode::{
    ClientPhase, EndReason, HostConfig, HostSession, NetTuning, SessionParams,
};
use drillgame_quic_transport::JoinTarget;
use drillgame_simulation::WorldState;

use super::{
    DEFAULT_PORT,
    bots::{BotBrain, BotCounters},
    client::{ClientLink, OnlineClient},
    host::{HostNet, HostNetStatus},
    random_token_seed,
};
use crate::local_game::new_world;

/// How often the stats line is printed.
const STATS_INTERVAL: Duration = Duration::from_secs(1);
/// Loop sleep between frames (the sessions run fixed ticks from measured real time).
const FRAME_SLEEP: Duration = Duration::from_millis(4);
/// How long a bot waits for the welcome before giving up.
const JOIN_TIMEOUT: Duration = Duration::from_secs(10);

/// A parsed headless command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadlessMode {
    /// `--headless-host [addr]`.
    Host {
        /// Bind address.
        bind: SocketAddr,
        /// Advertise on the LAN (`--lan`).
        lan: bool,
        /// Stop after this long.
        duration: Option<Duration>,
    },
    /// `--bot-client <addr> [--count N]`.
    Bots {
        /// Host address.
        addr: SocketAddr,
        /// Number of bots.
        count: usize,
        /// Stop after this long.
        duration: Option<Duration>,
    },
}

/// Parses the process arguments (without the program name). `Ok(None)` means "run the window".
///
/// # Errors
///
/// Returns a usage message for malformed headless arguments.
pub fn parse_args(args: &[String]) -> Result<Option<HeadlessMode>, String> {
    let mut iter = args.iter().peekable();
    let mut mode: Option<&str> = None;
    let mut addr: Option<SocketAddr> = None;
    let mut count = 1_usize;
    let mut duration = None;
    let mut lan = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--headless-host" => {
                mode = Some("host");
                if let Some(next) = iter.peek()
                    && !next.starts_with("--")
                {
                    addr = Some(parse_addr(next)?);
                    iter.next();
                }
            }
            "--bot-client" => {
                mode = Some("bots");
                let value = iter.next().ok_or("--bot-client needs a host address")?;
                addr = Some(parse_addr(value)?);
            }
            "--count" => {
                let value = iter.next().ok_or("--count needs a number")?;
                count = value
                    .parse()
                    .map_err(|_| format!("invalid --count {value}"))?;
            }
            "--duration" => {
                let value = iter.next().ok_or("--duration needs seconds")?;
                let seconds: f64 = value
                    .parse()
                    .map_err(|_| format!("invalid --duration {value}"))?;
                duration = Some(
                    Duration::try_from_secs_f64(seconds)
                        .map_err(|_| format!("invalid --duration {value}"))?,
                );
            }
            "--lan" => lan = true,
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
    }
    Ok(match mode {
        None => None,
        Some("host") => Some(HeadlessMode::Host {
            bind: addr.unwrap_or_else(|| {
                SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), DEFAULT_PORT)
            }),
            lan,
            duration,
        }),
        Some(_) => Some(HeadlessMode::Bots {
            addr: addr.ok_or("--bot-client needs a host address")?,
            count: count.max(1),
            duration,
        }),
    })
}

/// Command-line usage.
#[must_use]
pub fn usage() -> String {
    "usage: drillgame [--headless-host [addr] [--lan] | --bot-client <addr> [--count N]] [--duration SECS]".to_owned()
}

fn parse_addr(value: &str) -> Result<SocketAddr, String> {
    value.parse().or_else(|_| {
        format!("{value}:{DEFAULT_PORT}")
            .parse()
            .map_err(|_| format!("invalid address {value}"))
    })
}

/// Runs a headless mode to completion, printing to `out`.
///
/// # Errors
///
/// Returns a message if the mode cannot start or fails.
pub fn run(mode: &HeadlessMode, out: &mut impl Write) -> Result<(), String> {
    match mode {
        HeadlessMode::Host {
            bind,
            lan,
            duration,
        } => run_host(*bind, *lan, *duration, out),
        HeadlessMode::Bots {
            addr,
            count,
            duration,
        } => run_bots(*addr, *count, *duration, out),
    }
}

/// The world a headless host serves: a fresh dig site with no players yet; joining bots get a
/// few bombs so they exercise explosives.
#[must_use]
pub fn headless_world() -> WorldState {
    let mut world = new_world();
    let players: Vec<_> = world.player_ids().collect();
    for player in players {
        world.remove_player(player);
    }
    let mut template = world.player_template().clone();
    template.bombs = template.bombs.max(5);
    world.set_player_template(template);
    world
}

/// Runs a headless host until `duration` elapses (forever without one).
///
/// # Errors
///
/// Returns a message if the host endpoint cannot be started or bound.
pub fn run_host(
    bind: SocketAddr,
    lan: bool,
    duration: Option<Duration>,
    out: &mut impl Write,
) -> Result<(), String> {
    let world = headless_world();
    let config = HostConfig {
        params: SessionParams::new(world.tick_rate_hz()),
        tuning: NetTuning::default(),
        start_in_lobby: false,
        token_seed: random_token_seed(),
        ..HostConfig::default()
    };
    let mut session = HostSession::new(world, config);
    let mut net = HostNet::start(bind, lan).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut last = started;
    let mut last_stats = started;
    let mut announced = false;
    loop {
        let now = Instant::now();
        let dt = now - last;
        last = now;
        net.pump(&mut session, dt.as_secs_f32());
        let output = session.advance(dt);
        net.send(output.outbound);
        for notice in net.take_notices() {
            let _ignored = writeln!(out, "host notice: {notice}");
        }
        match net.status() {
            HostNetStatus::Ready { addr, .. } if !announced => {
                announced = true;
                let _ignored = writeln!(out, "HOST_READY addr={addr}");
                let _ignored = out.flush();
            }
            HostNetStatus::Failed(error) => return Err(format!("hosting failed: {error}")),
            _ => {}
        }
        if now - last_stats >= STATS_INTERVAL {
            last_stats = now;
            let (up, down) = net.byte_rates();
            let rtts: Vec<u128> = net
                .connections()
                .values()
                .filter_map(|connection| connection.stats.map(|stats| stats.rtt.as_millis()))
                .collect();
            let rtt = if rtts.is_empty() {
                0
            } else {
                rtts.iter().sum::<u128>() / rtts.len() as u128
            };
            let _ignored = writeln!(
                out,
                "host tick={} clients={} corrections_per_s=- rtt_ms={rtt} up_bytes_per_s={up:.0} down_bytes_per_s={down:.0}",
                session.tick().0,
                net.connections().len(),
            );
            let _ignored = out.flush();
        }
        if duration.is_some_and(|duration| now - started >= duration) {
            session.end_session(EndReason::HostClosed);
            net.send(session.take_outbound());
            net.shutdown_blocking();
            let _ignored = writeln!(out, "host ended tick={}", session.tick().0);
            return Ok(());
        }
        std::thread::sleep(FRAME_SLEEP);
    }
}

struct Bot {
    client: Option<OnlineClient>,
    brain: BotBrain,
    joined: bool,
}

/// Runs `count` scripted bot clients against `addr` until `duration` elapses or the host ends
/// the session.
///
/// # Errors
///
/// Returns a message if a bot cannot start, cannot join in time, or is disconnected without the
/// host ending the session.
pub fn run_bots(
    addr: SocketAddr,
    count: usize,
    duration: Option<Duration>,
    out: &mut impl Write,
) -> Result<(), String> {
    let fallback = headless_world();
    let mut bots = (0..count)
        .map(|index| {
            OnlineClient::join(
                JoinTarget::Addr(addr),
                format!("bot-{index}"),
                None,
                fallback.clone(),
            )
            .map(|client| Bot {
                client: Some(client),
                brain: BotBrain::new(index),
                joined: false,
            })
            .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let mut last = started;
    let mut last_stats = started;
    loop {
        let now = Instant::now();
        let dt = now - last;
        last = now;
        for bot in &mut bots {
            let Some(client) = bot.client.as_mut() else {
                continue;
            };
            client.pump(dt.as_secs_f32());
            if client.session().phase() == ClientPhase::InGame
                && let Some(player) = client.player()
            {
                bot.joined = true;
                let (controls, actions) =
                    bot.brain.decide(client.world(), player, dt.as_secs_f32());
                client.submit(controls, actions);
            }
            let _frame = client.advance(dt);
            for notice in client.take_notices() {
                let _ignored = writeln!(out, "bot notice: {notice}");
            }
        }
        if let Some(result) = finished(&mut bots, now - started, out) {
            return result;
        }
        if now - last_stats >= STATS_INTERVAL {
            last_stats = now;
            print_bot_stats(&bots, out);
        }
        if duration.is_some_and(|duration| now - started >= duration) {
            for bot in &mut bots {
                if let Some(client) = bot.client.take() {
                    client.leave_blocking();
                }
            }
            let _ignored = writeln!(out, "bots left count={count}");
            return Ok(());
        }
        std::thread::sleep(FRAME_SLEEP);
    }
}

/// `Some(result)` once every bot is done: all ended by the host (success) or one failed.
fn finished(
    bots: &mut [Bot],
    elapsed: Duration,
    out: &mut impl Write,
) -> Option<Result<(), String>> {
    let mut all_ended = true;
    for (index, bot) in bots.iter_mut().enumerate() {
        let Some(client) = bot.client.as_ref() else {
            continue;
        };
        if !bot.joined && elapsed > JOIN_TIMEOUT {
            return Some(Err(format!(
                "bot-{index} did not join within {JOIN_TIMEOUT:?}"
            )));
        }
        match (client.session().phase(), client.link()) {
            (ClientPhase::Ended(EndReason::HostClosed), _) => {}
            (ClientPhase::Rejected(reason), _) => {
                return Some(Err(format!("bot-{index} was rejected: {reason:?}")));
            }
            (_, ClientLink::Closed(reason)) => {
                return Some(Err(format!("bot-{index} lost its connection: {reason}")));
            }
            _ => all_ended = false,
        }
    }
    if all_ended {
        for bot in bots {
            if let Some(client) = bot.client.take() {
                client.shutdown_blocking();
            }
        }
        let _ignored = writeln!(out, "bots done: the host ended the session");
        return Some(Ok(()));
    }
    None
}

fn print_bot_stats(bots: &[Bot], out: &mut impl Write) {
    let clients: Vec<&OnlineClient> = bots.iter().filter_map(|bot| bot.client.as_ref()).collect();
    let in_game = clients
        .iter()
        .filter(|client| client.session().phase() == ClientPhase::InGame)
        .count();
    let tick = clients
        .iter()
        .map(|client| client.session().view().client_tick.0)
        .max()
        .unwrap_or(0);
    let server = clients
        .iter()
        .map(|client| client.session().view().server_tick.0)
        .max()
        .unwrap_or(0);
    let corrections: f32 = clients
        .iter()
        .map(|client| client.corrections_per_second())
        .sum();
    let rtts: Vec<u128> = clients
        .iter()
        .filter_map(|client| client.link_stats().map(|stats| stats.rtt.as_millis()))
        .collect();
    let rtt = if rtts.is_empty() {
        0
    } else {
        rtts.iter().sum::<u128>() / rtts.len() as u128
    };
    let (up, down) = clients.iter().fold((0.0, 0.0), |(up, down), client| {
        let (client_up, client_down) = client.byte_rates();
        (up + client_up, down + client_down)
    });
    let totals = bots.iter().map(|bot| bot.brain.counters()).fold(
        BotCounters::default(),
        |sum, counters| BotCounters {
            cycles: sum.cycles + counters.cycles,
            bombs: sum.bombs + counters.bombs,
            sales: sum.sales + counters.sales,
            refuels: sum.refuels + counters.refuels,
            rescues: sum.rescues + counters.rescues,
            actions: sum.actions + counters.actions,
        },
    );
    let _ignored = writeln!(
        out,
        "bots tick={tick} server_tick={server} clients={in_game}/{} corrections_per_s={corrections:.1} rtt_ms={rtt} up_bytes_per_s={up:.0} down_bytes_per_s={down:.0} actions={} bombs={} sales={} refuels={} rescues={} cycles={}",
        bots.len(),
        totals.actions,
        totals.bombs,
        totals.sales,
        totals.refuels,
        totals.rescues,
        totals.cycles,
    );
    let _ignored = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn no_headless_flags_runs_the_window() {
        assert_eq!(parse_args(&[]), Ok(None));
    }

    #[test]
    fn parses_host_and_bot_modes() {
        assert_eq!(
            parse_args(&args(&["--headless-host"])),
            Ok(Some(HeadlessMode::Host {
                bind: SocketAddr::new(std::net::Ipv4Addr::UNSPECIFIED.into(), DEFAULT_PORT),
                lan: false,
                duration: None,
            }))
        );
        assert_eq!(
            parse_args(&args(&[
                "--headless-host",
                "127.0.0.1:0",
                "--duration",
                "2.5",
                "--lan"
            ])),
            Ok(Some(HeadlessMode::Host {
                bind: "127.0.0.1:0".parse().expect("addr"),
                lan: true,
                duration: Some(Duration::from_millis(2_500)),
            }))
        );
        assert_eq!(
            parse_args(&args(&["--bot-client", "10.0.0.2", "--count", "3"])),
            Ok(Some(HeadlessMode::Bots {
                addr: SocketAddr::new([10, 0, 0, 2].into(), DEFAULT_PORT),
                count: 3,
                duration: None,
            }))
        );
        assert!(parse_args(&args(&["--bot-client"])).is_err());
        assert!(parse_args(&args(&["--bogus"])).is_err());
    }
}
