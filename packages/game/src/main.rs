#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod app;
mod audio;
mod effects;
mod input;
mod input_mapping;
mod local_game;
mod render_snapshot;
mod rendering;
mod runtime;
mod save;
mod shell;

fn main() {
    app::run();
}
