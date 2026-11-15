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
        read_input, read_input_with_arrow_aliases, read_secondary_keyboard_input, release_mouse,
        router::{DeviceFrame, InputRouter},
    },
    local_game::{FrameInput, LocalGame, SaveTarget},
    rendering::{
        GameRenderer,
        pointer::{self, MenuHit},
    },
    runtime::MAX_LOCAL_SEATS,
    save::{load_settings, save_settings},
    shell::RunMode,
    touch_screen::{TouchDevice, TouchSample},
    window::DesktopWindow,
};

#[cfg(not(target_os = "emscripten"))]
const TARGET_FPS: u32 = 60;

/// Seconds the mouse may rest before the keyboard seat stops aiming at it (its weapon falls
/// back to auto-aim).
const MOUSE_AIM_IDLE_SECONDS: f32 = 4.0;

/// Whether the mouse is currently someone's aim: it moved or clicked recently.
#[derive(Clone, Copy, Debug, Default)]
struct MouseAim {
    idle_seconds: f32,
    seen: bool,
}

impl MouseAim {
    /// The pointer position while the mouse is in use.
    fn sample(&mut self, raylib: &raylib::RaylibHandle, delta_seconds: f32) -> Option<(f32, f32)> {
        let moved = raylib.get_mouse_delta();
        let used = moved.x.abs() + moved.y.abs() > 0.5
            || raylib.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT);
        if used {
            self.idle_seconds = 0.0;
            self.seen = true;
        } else {
            self.idle_seconds += delta_seconds.max(0.0);
        }
        let position = raylib.get_mouse_position();
        (self.seen && self.idle_seconds < MOUSE_AIM_IDLE_SECONDS)
            .then_some((position.x, position.y))
    }
}

/// Samples every input device for this frame. Playing alone, the main keyboard reads WASD and
/// the arrow keys; in split-screen the arrow keys are their own scheme (see `input::seats`).
fn sample_devices(
    raylib: &mut raylib::RaylibHandle,
    seat_count: usize,
    exit_requested: bool,
    pointer: Option<(f32, f32)>,
    touch: TouchSample,
) -> DeviceFrame {
    let solo = seat_count <= 1;
    let mut keyboard = if solo {
        read_input_with_arrow_aliases(raylib, exit_requested)
    } else {
        read_input(raylib, exit_requested)
    };
    keyboard.pointer = pointer;
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
    if touch.owns_pointer {
        release_mouse(&mut keyboard, raylib);
    }
    DeviceFrame {
        keyboard,
        arrow_keys,
        mouse_used: mouse_used && !touch.owns_pointer,
        pads: sample_pads(raylib),
        touch: touch.input,
        touch_used: touch.used,
    }
}

/// Routes this frame's devices to seats, reports controller hot-plug as toasts and adds a
/// split-screen seat when a spare controller claims one. `view` is the first seat's view as
/// last drawn (touches and the mouse point into it). While the touch screen owns the pointer
/// the mouse neither aims nor clicks.
fn sample_frame_input(
    raylib: &mut raylib::RaylibHandle,
    router: &mut InputRouter,
    game: &mut LocalGame,
    (delta_seconds, mouse, touch): (f32, &mut MouseAim, &mut TouchDevice),
    exit_requested: bool,
    view: Option<(PointerView, (f32, f32))>,
) -> FrameInput {
    let touch = touch.sample(
        raylib,
        game.shell.touch,
        DesktopWindow::screen(raylib),
        |x, y| view.and_then(|(view, _)| view.world_at(x, y)),
        delta_seconds,
    );
    let pointer = mouse
        .sample(raylib, delta_seconds)
        .filter(|_| !touch.owns_pointer);
    let devices = sample_devices(raylib, game.seats().len(), exit_requested, pointer, touch);
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
    let mut input = FrameInput {
        primary: seats.next().unwrap_or_default(),
        other_seats: seats.collect(),
        clipboard: None,
        devices: output.devices,
        gamepads: router.pad_names().clone(),
    };
    if !touch.owns_pointer {
        let mouse = raylib.get_mouse_position();
        point_with_mouse(&mut input.primary, view, (mouse.x, mouse.y));
        // A click on a menu row picks it.
        if input.primary.pointer_primary
            && let MenuHit::Row(row) = pointer::hit(mouse.x, mouse.y)
        {
            input.primary.menu_pick = Some(row);
        }
    }
    input
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

/// Whether the left mouse button stands in for a finger (`--touch`, a development aid for
/// trying the touch controls on a desktop).
fn touch_with_mouse() -> bool {
    std::env::args().any(|argument| argument == "--touch")
}

pub fn run() {
    let app = App::new();
    #[cfg(not(target_os = "emscripten"))]
    {
        let mut app = app;
        while app.frame(None) {}
        app.game.shutdown();
    }
    #[cfg(target_os = "emscripten")]
    {
        let mut app = Some(app);
        crate::browser::run(move |delta| {
            let running = app.as_mut().is_some_and(|app| app.frame(Some(delta)));
            if !running && let Some(app) = app.take() {
                app.game.shutdown();
            }
            running
        });
    }
}

/// Where the main keyboard seat's view was last drawn, to turn the mouse into a world point.
#[derive(Clone, Copy, Debug)]
struct PointerView {
    viewport: crate::render_snapshot::Viewport,
    world_scale: f32,
    camera: crate::render_snapshot::CameraPosition,
}

impl PointerView {
    /// The world pixel under logical screen point `(x, y)`, if it is inside the view.
    fn world_at(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let v = self.viewport;
        #[allow(clippy::cast_precision_loss, reason = "screen sizes are small")]
        let inside = x >= v.x as f32
            && y >= v.y as f32
            && x < (v.x + v.width) as f32
            && y < (v.y + v.height) as f32;
        #[allow(clippy::cast_precision_loss, reason = "screen sizes are small")]
        inside.then(|| {
            (
                (x - v.x as f32) / self.world_scale + self.camera.x,
                (y - v.y as f32) / self.world_scale + self.camera.y,
            )
        })
    }
}

/// The mouse points in the main keyboard seat's view when it moves or clicks (build mode's
/// cursor). `last` is that view as last drawn and where the mouse was then.
fn point_with_mouse(
    input: &mut PlayerInput,
    last: Option<(PointerView, (f32, f32))>,
    mouse: (f32, f32),
) {
    let Some((view, last)) = last else {
        return;
    };
    let moved = (mouse.0 - last.0).abs() + (mouse.1 - last.1).abs() > 0.5;
    if moved || input.pointer_primary || input.pointer_secondary {
        input.build_pointer = view.world_at(mouse.0, mouse.1);
    }
}

struct App {
    /// The main keyboard seat's view as last drawn, and the mouse position then (build mode
    /// points with the mouse).
    pointer: Option<(PointerView, (f32, f32))>,
    renderer: GameRenderer,
    audio: Option<AudioBus>,
    game: LocalGame,
    window: DesktopWindow,
    saved_window: crate::save::WindowSettings,
    router: InputRouter,
    mouse: MouseAim,
    touch: TouchDevice,
    thread: raylib::RaylibThread,
    raylib: raylib::RaylibHandle,
}

impl App {
    fn new() -> Self {
        let settings = load_settings();
        let (mut raylib, thread, mut window) = DesktopWindow::open(settings.window);
        #[cfg(not(target_os = "emscripten"))]
        raylib.set_target_fps(TARGET_FPS);
        #[cfg(target_os = "emscripten")]
        raylib.set_target_fps(0);
        raylib.set_exit_key(None);

        let saved_window = settings.window;
        let mut game = LocalGame::new(settings, SaveTarget::UserFiles);
        game.set_profile(crate::profile::load_or_create());
        window.set_fullscreen(&mut raylib, game.shell.fullscreen);
        let audio = match AudioBus::new() {
            Ok(audio) => Some(audio),
            Err(error) => {
                eprintln!("Audio disabled: {error}");
                None
            }
        };
        let renderer = GameRenderer::new(&mut raylib, &thread, game.world().terrain());
        let router = InputRouter::new(MAX_LOCAL_SEATS);

        Self {
            pointer: None,
            renderer,
            audio,
            game,
            window,
            saved_window,
            router,
            mouse: MouseAim::default(),
            touch: TouchDevice::new(touch_with_mouse()),
            thread,
            raylib,
        }
    }

    /// One complete frame; only the desktop loop blocks between frames.
    fn frame(&mut self, browser_delta: Option<f32>) -> bool {
        let Self {
            pointer,
            raylib,
            thread,
            window,
            saved_window,
            game,
            audio,
            renderer,
            router,
            mouse,
            touch,
        } = self;
        if game.should_exit() {
            // Remember a resized or (un)maximized window for the next run.
            game.shell.window = window.settings();
            if game.shell.window != *saved_window
                && let Err(error) = save_settings(game.shell.settings())
            {
                eprintln!("Settings save failed: {error}");
            }
            // Tell remote players the session is over (host) or leave it (client) before exiting.
            return false;
        }
        #[cfg(target_os = "emscripten")]
        {
            if crate::browser::take_suspend_request() {
                game.save_browser_checkpoint();
            }
            if crate::browser::is_hidden() {
                return true;
            }
            crate::browser::resize_canvas(raylib);
        }
        let delta_seconds = browser_delta.unwrap_or_else(|| raylib.get_frame_time());
        #[cfg(not(target_os = "emscripten"))]
        let exit_requested = raylib.window_should_close();
        // raylib's web WindowShouldClose sleeps internally; RAF owns scheduling here.
        #[cfg(target_os = "emscripten")]
        let exit_requested = false;
        let mut input = sample_frame_input(
            raylib,
            router,
            game,
            (delta_seconds, mouse, touch),
            exit_requested,
            *pointer,
        );
        if input.primary.paste {
            input.clipboard = raylib.get_clipboard_text().ok();
        }
        let mouse = raylib.get_mouse_position();
        renderer.apply_ui_input(&input.seat_inputs(), &game.shell);
        window.track(raylib);
        game.shell.window = window.settings();

        let report = game.frame(&input, delta_seconds);

        window.set_fullscreen(raylib, game.shell.fullscreen);
        if let Some(text) = &report.clipboard
            && let Err(error) = raylib.set_clipboard_text(text)
        {
            eprintln!("Clipboard copy failed: {error}");
        }
        if report.settings_changed {
            *saved_window = game.shell.window;
            if let Err(error) = save_settings(game.shell.settings()) {
                eprintln!("Settings save failed: {error}");
            }
        }

        // Laid out against this frame's real size: the window may have been resized,
        // maximized or switched to fullscreen since the last frame.
        let snapshot = game.snapshot(DesktopWindow::screen(raylib));
        *pointer = snapshot.views.first().map(|view| {
            (
                PointerView {
                    viewport: view.viewport,
                    world_scale: view.world_scale,
                    camera: view.camera,
                },
                (mouse.x, mouse.y),
            )
        });
        touch.observe(&game.shell, game.world(), snapshot.views.first());
        if let Some(audio) = audio {
            let seats = seat_audio(&input, game);
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
        touch.publish(&game.shell);
        renderer.sync(raylib, thread, &snapshot, &report);
        let mut draw = raylib.begin_drawing(thread);
        renderer.render(&mut draw, &game.shell, &snapshot);
        true
    }
}
