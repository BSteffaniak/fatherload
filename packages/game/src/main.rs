#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod app;
mod audio;
mod game_state;
mod input;
mod input_mapping;
mod lan_discovery;
pub mod multiplayer;
mod online_cli;
mod online_session;
mod rendering;
mod save;
mod session;

use app::run;
use drillgame_simulation::{contract, economy, player, surface, terrain};

fn main() {
    if let Some(action) = online_cli::parse_online_cli_action(std::env::args().skip(1)) {
        match online_cli::run_online_cli_action(action) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("online CLI action failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    run();
}
