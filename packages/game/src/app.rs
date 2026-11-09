//! The raylib desktop app: samples devices, runs [`LocalGame::frame`] and draws its snapshot.

use raylib::prelude::MouseButton;

use crate::{
    audio::{
        AudioBus,
        director::{AudioFrame, SeatAudio},
    },
    input::{
        PlayerInput,
        pads::sample_pads,
        read_input, read_input_with_arrow_aliases, read_secondary_keyboard_input,
        router::{DeviceFrame, InputRouter},
    },
    local_game::{FrameInput, LocalGame, SaveTarget},
    rendering::GameRenderer,
    runtime::MAX_LOCAL_SEATS,
    save::{load_settings, save_settings},
    shell::RunMode,
    window::DesktopWindow,
};

const TARGET_FPS: u32 = 60;

/// Samples every input device for this frame. Playing alone, the main keyboard reads WASD and
/// the arrow keys; in split-screen the arrow keys are their own scheme (see `input::seats`).
fn sample_devices(
    raylib: &mut raylib::RaylibHandle,
    seat_count: usize,
    exit_requested: bool,
) -> DeviceFrame {
    let solo = seat_count <= 1;
    let keyboard = if solo {
        read_input_with_arrow_aliases(raylib, exit_requested)
    } else {
        read_input(raylib, exit_requested)
    };
    let arrow_keys = if solo {
        PlayerInput::default()
    } else {
        read_secondary_keyboard_input(raylib)
    };
    let mouse_used = [
        MouseButton::MOUSE_BUTTON_LEFT,
        MouseButton::MOUSE_BUTTON_RIGHT,
        MouseButton::MOUSE_BUTTON_MIDDLE,
    ]
    .into_iter()
    .any(|button| raylib.is_mouse_button_pressed(button));
    DeviceFrame {
        keyboard,
        arrow_keys,
        mouse_used,
        pads: sample_pads(raylib),
    }
}

/// Routes this frame's devices to seats, reports controller hot-plug as toasts and adds a
/// split-screen seat when a spare controller claims one.
fn sample_frame_input(
    raylib: &mut raylib::RaylibHandle,
    router: &mut InputRouter,
    game: &mut LocalGame,
    delta_seconds: f32,
    exit_requested: bool,
) -> FrameInput {
    let devices = sample_devices(raylib, game.seats().len(), exit_requested);
    let output = router.route(&devices, game.seats().len(), delta_seconds);
    for notice in &output.notices {
        game.shell.message = notice.message();
    }
    if let Some(pad) = output.wants_new_seat
        && let Some(seat) = game.add_seat_for_controller()
    {
        router.seat_added(pad, seat);
        game.shell.message = format!(
            "Controller {} joined as Player {}.",
            u32::from(pad) + 1,
            seat + 1
        );
    }
    let mut seats = output.seats.into_iter();
    FrameInput {
        primary: seats.next().unwrap_or_default(),
        other_seats: seats.collect(),
        clipboard: None,
        devices: output.devices,
        gamepads: router.pad_names().clone(),
    }
}

/// What audio needs from each seat this frame: its thrust, whether it is driving its rig and
/// when its toast last changed.
fn seat_audio(input: &FrameInput, game: &LocalGame) -> Vec<SeatAudio> {
    input
        .seat_inputs()
        .iter()
        .enumerate()
        .take(game.seats().len().max(1))
        .map(|(seat, sampled)| SeatAudio {
            thrust: sampled.thrust,
            playing: game.shell.gameplay_input_enabled(seat),
            message_tick: game.shell.seat(seat).message_tick,
        })
        .collect()
}

pub fn run() {
    let settings = load_settings();
    let (mut raylib, thread, mut window) = DesktopWindow::open(settings.window);
    raylib.set_target_fps(TARGET_FPS);
    raylib.set_exit_key(None);

    let mut saved_window = settings.window;
    let mut game = LocalGame::new(settings, SaveTarget::UserFiles);
    window.set_fullscreen(&mut raylib, game.shell.fullscreen);
    let mut audio = match AudioBus::new() {
        Ok(audio) => Some(audio),
        Err(error) => {
            eprintln!("Audio disabled: {error}");
            None
        }
    };
    let mut renderer = GameRenderer::new(&mut raylib, &thread, game.world().terrain());
    let mut router = InputRouter::new(MAX_LOCAL_SEATS);

    while !game.should_exit() {
        let delta_seconds = raylib.get_frame_time();
        let exit_requested = raylib.window_should_close();
        let mut input = sample_frame_input(
            &mut raylib,
            &mut router,
            &mut game,
            delta_seconds,
            exit_requested,
        );
        if input.primary.paste {
            input.clipboard = raylib.get_clipboard_text().ok();
        }
        renderer.apply_ui_input(&input.seat_inputs(), &game.shell);
        window.track(&raylib);
        game.shell.window = window.settings();

        let report = game.frame(&input, delta_seconds);

        window.set_fullscreen(&mut raylib, game.shell.fullscreen);
        if let Some(text) = &report.clipboard
            && let Err(error) = raylib.set_clipboard_text(text)
        {
            eprintln!("Clipboard copy failed: {error}");
        }
        if report.settings_changed {
            saved_window = game.shell.window;
            if let Err(error) = save_settings(game.shell.settings()) {
                eprintln!("Settings save failed: {error}");
            }
        }

        // Laid out against this frame's real size: the window may have been resized,
        // maximized or switched to fullscreen since the last frame.
        let snapshot = game.snapshot(DesktopWindow::screen(&raylib));
        if let Some(audio) = &mut audio {
            let seats = seat_audio(&input, &game);
            let frame = AudioFrame {
                snapshot: &snapshot,
                cues: &report.sound_cues,
                seats: &seats,
                world_running: game.shell.world_should_tick()
                    && game.shell.run_mode == RunMode::Playing,
                message_tick: game.shell.message_tick,
                delta_seconds,
            };
            audio.update(&frame, game.shell.master_volume);
        }
        renderer.sync(&mut raylib, &thread, &snapshot, &report);
        let mut draw = raylib.begin_drawing(&thread);
        renderer.render(&mut draw, &game.shell, &snapshot);
    }
    // Remember a resized or (un)maximized window for the next run.
    game.shell.window = window.settings();
    if game.shell.window != saved_window
        && let Err(error) = save_settings(game.shell.settings())
    {
        eprintln!("Settings save failed: {error}");
    }
    // Tell remote players the session is over (host) or leave it (client) before exiting.
    game.shutdown();
}
