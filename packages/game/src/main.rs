#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod app;
mod audio;
mod effects;
mod input;
mod input_mapping;
mod local_game;
mod online;
mod pixel_scale;
mod render_snapshot;
mod rendering;
mod runtime;
mod save;
mod scene_capture;
mod shell;
mod window;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if input::pads::wants_list(&args) {
        input::pads::list_gamepads();
        return;
    }
    match scene_capture::parse_args(&args) {
        Ok(Some(options)) => {
            if let Err(error) = scene_capture::run(&options) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return;
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
    match online::headless::parse_args(&args) {
        Ok(None) => app::run(),
        Ok(Some(mode)) => {
            let mut stdout = std::io::stdout();
            if let Err(error) = online::headless::run(&mode, &mut stdout) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
