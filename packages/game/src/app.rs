//! The raylib desktop app: samples devices, runs [`LocalGame::frame`] and draws its snapshot.

use crate::{
    audio::AudioBus,
    input::{
        PlayerInput, combine_player_input, read_gamepad_input, read_input,
        read_input_with_arrow_aliases, read_secondary_keyboard_input,
    },
    local_game::{FrameInput, LocalGame, SaveTarget},
    render_snapshot::{SCREEN_HEIGHT, SCREEN_WIDTH},
    rendering::GameRenderer,
    save::{load_settings, save_settings},
};

const TARGET_FPS: u32 = 60;

/// Samples every local seat's input: seat 0 is the primary keyboard (plus arrow keys and the
/// first gamepad when playing alone), seat 1 the secondary keyboard plus the first gamepad, and
/// further seats the remaining gamepads.
fn sample_frame_input(
    raylib: &mut raylib::RaylibHandle,
    seat_count: usize,
    exit_requested: bool,
) -> FrameInput {
    if seat_count <= 1 {
        let keyboard = read_input_with_arrow_aliases(raylib, exit_requested);
        let primary = read_gamepad_input(raylib, 0)
            .map_or(keyboard, |gamepad| combine_player_input(keyboard, gamepad));
        return FrameInput::primary(primary);
    }
    let primary = read_input(raylib, exit_requested);
    let other_seats = (1..seat_count)
        .map(|seat| {
            if seat == 1 {
                let keyboard = read_secondary_keyboard_input(raylib);
                read_gamepad_input(raylib, 0)
                    .map_or(keyboard, |gamepad| combine_player_input(keyboard, gamepad))
            } else {
                i32::try_from(seat - 1)
                    .ok()
                    .and_then(|pad| read_gamepad_input(raylib, pad))
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<PlayerInput>>();
    FrameInput {
        primary,
        other_seats,
        clipboard: None,
    }
}

pub fn run() {
    let (mut raylib, thread) = raylib::init()
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .highdpi()
        .title("Drillgame")
        .build();
    raylib.set_target_fps(TARGET_FPS);
    raylib.set_exit_key(None);

    let mut game = LocalGame::new(load_settings(), SaveTarget::UserFiles);
    if game.shell.fullscreen {
        raylib.toggle_fullscreen();
    }
    let audio = match AudioBus::new() {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("Audio disabled: {error}");
            None
        }
    };
    let mut renderer = GameRenderer::new(&mut raylib, &thread, game.world().terrain());

    while !game.should_exit() {
        let delta_seconds = raylib.get_frame_time();
        let exit_requested = raylib.window_should_close();
        let mut input = sample_frame_input(&mut raylib, game.seats().len(), exit_requested);
        if input.primary.paste {
            input.clipboard = raylib.get_clipboard_text().ok();
        }
        renderer.apply_ui_input(input.primary);
        let fullscreen_before = game.shell.fullscreen;

        let report = game.frame(&input, delta_seconds);

        if game.shell.fullscreen != fullscreen_before {
            raylib.toggle_fullscreen();
        }
        if let Some(text) = &report.clipboard
            && let Err(error) = raylib.set_clipboard_text(text)
        {
            eprintln!("Clipboard copy failed: {error}");
        }
        if report.settings_changed
            && let Err(error) = save_settings(game.shell.settings())
        {
            eprintln!("Settings save failed: {error}");
        }
        if let Some(audio) = &audio {
            audio.set_volume(game.shell.master_volume);
            audio.play(&report.sound_cues);
        }

        let snapshot = game.snapshot((SCREEN_WIDTH, SCREEN_HEIGHT));
        renderer.sync(&mut raylib, &thread, &snapshot, &report);
        let mut draw = raylib.begin_drawing(&thread);
        renderer.render(&mut draw, &game.shell, &snapshot);
    }
    // Tell remote players the session is over (host) or leave it (client) before exiting.
    game.shutdown();
}
