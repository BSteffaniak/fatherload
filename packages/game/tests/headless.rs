#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Headless smoke test: the real game binary as `--headless-host` plus a `--bot-client`, two
//! processes over QUIC on localhost. Both must play for a couple of seconds and exit cleanly.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_drillgame");
const DEADLINE: Duration = Duration::from_secs(30);

/// Kills the child if the test panics before it exits.
struct Guard(Option<Child>);

impl Guard {
    fn wait(&mut self, deadline: Instant) -> std::process::ExitStatus {
        let child = self.0.as_mut().expect("child");
        loop {
            if let Some(status) = child.try_wait().expect("child status") {
                self.0 = None;
                return status;
            }
            assert!(Instant::now() < deadline, "child did not exit in time");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ignored = child.kill();
            let _ignored = child.wait();
        }
    }
}

/// Forwards a child's stdout lines over a channel (so reads never block the test).
fn lines(child: &mut Child) -> mpsc::Receiver<String> {
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

#[test]
fn headless_host_and_bot_client_play_and_exit_cleanly() {
    let deadline = Instant::now() + DEADLINE;
    let mut host = Guard(Some(
        Command::new(BIN)
            .args(["--headless-host", "127.0.0.1:0", "--duration", "4"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("host starts"),
    ));
    let host_lines = lines(host.0.as_mut().expect("host"));
    let addr = loop {
        let line = host_lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("host announces its address");
        if let Some(addr) = line.strip_prefix("HOST_READY addr=") {
            break addr.to_owned();
        }
    };

    let mut bot = Guard(Some(
        Command::new(BIN)
            .args(["--bot-client", &addr, "--count", "1", "--duration", "2"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("bot starts"),
    ));
    let bot_lines = lines(bot.0.as_mut().expect("bot"));

    let bot_status = bot.wait(deadline);
    let bot_output: Vec<String> = bot_lines.try_iter().collect();
    assert!(bot_status.success(), "bot exited cleanly: {bot_output:?}");
    assert!(
        bot_output
            .iter()
            .any(|line| line.starts_with("bots tick=") && line.contains("clients=1/1")),
        "bot printed in-game stats: {bot_output:?}"
    );
    assert!(
        bot_output.iter().any(|line| line.starts_with("bots left")),
        "bot left at the end of its run: {bot_output:?}"
    );

    let host_status = host.wait(deadline);
    let host_output: Vec<String> = host_lines.try_iter().collect();
    assert!(
        host_status.success(),
        "host exited cleanly: {host_output:?}"
    );
    assert!(
        host_output
            .iter()
            .any(|line| line.starts_with("host tick=") && line.contains("clients=1")),
        "host saw the bot: {host_output:?}"
    );
    assert!(
        host_output
            .iter()
            .any(|line| line.starts_with("host ended")),
        "host ended the session: {host_output:?}"
    );
}
