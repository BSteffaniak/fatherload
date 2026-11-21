//! The touch screen as an input device: samples fingers from raylib, keeps the game context the
//! on-screen controls depend on and hands the overlay to rendering.
//!
//! The mapping itself is [`crate::input::touch`]; this is the part that knows about raylib, the
//! shell and the render snapshot.

use fatherload_simulation::{WorldState, terrain::TILE_SIZE};
use raylib::prelude::MouseButton;

use crate::{
    input::{
        PlayerInput,
        touch::{
            BuildTap, KIT_COUNT, PointerEvent, TouchContext, TouchInput, TouchLayout, TouchMode,
            TouchPilot, TouchPoint, TouchScreen, TouchSettings, TouchView,
        },
    },
    pixel_scale,
    render_snapshot::{ScreenSize, ViewSnapshot},
    rendering::pointer::{self, MenuHit},
    shell::ShellState,
};

/// Seconds after the last finger lifts during which the mouse is still ignored: touch screens
/// replay touches as mouse events, some of them a little late.
const MOUSE_QUIET_SECONDS: f32 = 0.75;

/// What rendering needs to draw the on-screen controls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TouchOverlay {
    pub layout: TouchLayout,
    pub view: TouchView,
    pub context: TouchContext,
}

/// One frame of touch input for the first seat.
#[derive(Clone, Copy, Debug, Default)]
pub struct TouchSample {
    pub input: PlayerInput,
    /// A finger went down this frame.
    pub used: bool,
    /// Fingers are (or just were) on the screen: mouse events are echoes of them and must not
    /// aim, fire or point.
    pub owns_pointer: bool,
}

/// How often the safe-area insets are read again (frames): they only change when the device
/// is turned.
#[cfg(target_os = "emscripten")]
const INSET_REFRESH_FRAMES: u32 = 30;

/// The touch screen. Keep one for the app.
#[derive(Debug, Default)]
pub struct TouchDevice {
    input: TouchInput,
    context: TouchContext,
    build_tap: BuildTap,
    settings: TouchSettings,
    /// Development aid (`--touch`): the left mouse button is a finger.
    mouse_is_finger: bool,
    /// Touch should be the device in use from the first frame (before any finger lands).
    announce: bool,
    /// Seconds since a finger was last down (`None`: never).
    quiet_seconds: Option<f32>,
    /// Safe-area insets in points: left, top, right, bottom.
    insets: [f32; 4],
    /// Frames until the insets are read again.
    #[cfg(target_os = "emscripten")]
    inset_countdown: u32,
    overlay: TouchOverlay,
}

impl TouchDevice {
    #[must_use]
    pub fn new(mouse_is_finger: bool) -> Self {
        // A phone or tablet browser shows the touch controls before the first touch.
        #[cfg(target_os = "emscripten")]
        let touch_first = crate::browser::touch_first();
        #[cfg(not(target_os = "emscripten"))]
        let touch_first = false;
        Self {
            mouse_is_finger,
            announce: mouse_is_finger || touch_first,
            ..Self::default()
        }
    }

    /// Samples the fingers and maps them. `world_at` turns a screen point into a world pixel
    /// in the first seat's view (build mode points at tiles).
    pub fn sample(
        &mut self,
        raylib: &raylib::RaylibHandle,
        settings: TouchSettings,
        screen: ScreenSize,
        world_at: impl Fn(f32, f32) -> Option<(f32, f32)>,
        delta_seconds: f32,
    ) -> TouchSample {
        self.settings = settings;
        self.refresh_insets();
        let points = self.points(raylib);
        let screen = touch_screen(screen, self.insets);
        ui_kit_insets(screen);
        let layout =
            TouchLayout::compute(screen, self.settings, self.context, self.input.tools_open());
        let frame = self
            .input
            .update(&points, &layout, screen, self.context, delta_seconds);
        let mut input = frame.input;
        if self.context.mode == TouchMode::Build {
            for event in &frame.events {
                let PointerEvent::Tap { x, y } = *event else {
                    continue;
                };
                let Some(world) = world_at(x, y) else {
                    continue;
                };
                input.build_pointer = Some(world);
                #[allow(clippy::cast_possible_truncation, reason = "tile indices fit i32")]
                let tile = (
                    (world.0 / TILE_SIZE).floor() as i32,
                    (world.1 / TILE_SIZE).floor() as i32,
                );
                input.pointer_primary |= self.build_tap.tap(tile);
            }
        } else {
            self.build_tap.reset();
        }
        if self.context.mode == TouchMode::Menu {
            for event in &frame.events {
                if let PointerEvent::Tap { x, y } = *event {
                    match pointer::hit(x, y) {
                        MenuHit::Row(row) => input.menu_pick = Some(row),
                        MenuHit::Outside => input.back = true,
                        MenuHit::Dialog | MenuHit::Nothing => {}
                    }
                }
            }
        }
        self.quiet_seconds = if frame.engaged {
            Some(0.0)
        } else {
            self.quiet_seconds
                .map(|seconds| seconds + delta_seconds.max(0.0))
        };
        #[cfg(target_os = "emscripten")]
        if frame.used && !frame.view.held.is_empty() {
            crate::browser::vibrate();
        }
        // The layout may have changed with this frame's presses (the tools fan).
        self.overlay = TouchOverlay {
            layout: TouchLayout::compute(
                screen,
                self.settings,
                self.context,
                self.input.tools_open(),
            ),
            view: frame.view,
            context: self.context,
        };
        TouchSample {
            input,
            used: frame.used || std::mem::take(&mut self.announce),
            owns_pointer: self.mouse_is_finger
                || self
                    .quiet_seconds
                    .is_some_and(|seconds| seconds < MOUSE_QUIET_SECONDS),
        }
    }

    /// Records what the first seat is doing, from the frame just simulated, for the next
    /// frame's controls.
    pub fn observe(
        &mut self,
        shell: &ShellState,
        world: &WorldState,
        view: Option<&ViewSnapshot<'_>>,
    ) {
        let waiting_charges = view.map_or(0, |view| world.waiting_remote_charges(view.player_id));
        self.context = touch_context(shell, view, waiting_charges);
    }

    /// Hands this frame's controls to rendering while touch is the first seat's device.
    pub fn publish(&self, shell: &ShellState) {
        crate::rendering::touch_controls::set_overlay(
            shell
                .prompt_device(0)
                .is_touch()
                .then(|| self.overlay.clone()),
        );
    }

    /// Reads the platform's safe-area insets now and then.
    #[cfg_attr(not(target_os = "emscripten"), allow(clippy::unused_self))]
    #[cfg_attr(
        not(target_os = "emscripten"),
        allow(clippy::missing_const_for_fn, clippy::needless_pass_by_ref_mut)
    )]
    fn refresh_insets(&mut self) {
        #[cfg(target_os = "emscripten")]
        {
            if self.inset_countdown == 0 {
                self.insets = crate::browser::safe_insets();
                self.inset_countdown = INSET_REFRESH_FRAMES;
            }
            self.inset_countdown -= 1;
        }
    }

    fn points(&self, raylib: &raylib::RaylibHandle) -> Vec<TouchPoint> {
        let mut points: Vec<TouchPoint> = (0..raylib.get_touch_point_count())
            .map(|index| {
                let position = raylib.get_touch_position(index);
                TouchPoint {
                    id: raylib.get_touch_point_id(index),
                    x: position.x,
                    y: position.y,
                }
            })
            .collect();
        #[cfg(target_os = "emscripten")]
        if !points.is_empty() && crate::browser::active_touches() == 0 {
            points.clear();
        }
        if self.mouse_is_finger
            && points.is_empty()
            && raylib.is_mouse_button_down(MouseButton::MOUSE_BUTTON_LEFT)
        {
            let position = raylib.get_mouse_position();
            points.push(TouchPoint {
                id: 0,
                x: position.x,
                y: position.y,
            });
        }
        points
    }
}

/// Publishes the touch controls for a staged frame (scene captures): what the first seat would
/// see on `screen` with fingers as in `view`, or nothing when its device is not the touch
/// screen.
#[cfg(not(target_os = "emscripten"))]
pub fn publish_staged(
    shell: &ShellState,
    world: &WorldState,
    seat_view: Option<&ViewSnapshot<'_>>,
    screen: ScreenSize,
    view: &TouchView,
) {
    let overlay = shell.prompt_device(0).is_touch().then(|| {
        let waiting = seat_view.map_or(0, |seat| world.waiting_remote_charges(seat.player_id));
        let context = touch_context(shell, seat_view, waiting);
        TouchOverlay {
            layout: TouchLayout::compute(
                touch_screen(screen, [0.0; 4]),
                shell.touch,
                context,
                view.tools_open,
            ),
            view: view.clone(),
            context,
        }
    });
    crate::rendering::touch_controls::set_overlay(overlay);
}

/// Tells the HUD where the safe area is (it keeps its panels inside it).
fn ui_kit_insets(screen: TouchScreen) {
    crate::rendering::set_safe_insets(screen.insets);
}

/// The layout's screen for a window of `screen` points with `insets` (points: left, top,
/// right, bottom).
fn touch_screen(screen: ScreenSize, insets: [f32; 4]) -> TouchScreen {
    let unit = pixel_scale::display_scale();
    let density = screen.density.max(1.0);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "screen sizes are small"
    )]
    let ui_pixel = pixel_scale::ui_pixel_size(
        (screen.width as f32 * density).round() as i32,
        (screen.height as f32 * density).round() as i32,
        density,
    ) as f32;
    #[allow(clippy::cast_precision_loss, reason = "screen sizes are small")]
    TouchScreen {
        width: screen.width as f32,
        height: screen.height as f32,
        insets: insets.map(|inset| inset * unit),
        points_per_ui_pixel: ui_pixel / density,
        unit,
    }
}

/// What the first seat is doing, as the touch controls see it.
#[must_use]
pub fn touch_context(
    shell: &ShellState,
    view: Option<&ViewSnapshot<'_>>,
    waiting_charges: usize,
) -> TouchContext {
    let seat = shell.seat(0);
    let mode = if shell.global_overlay() || seat.modal.is_some() {
        TouchMode::Menu
    } else if seat.build.is_some() {
        TouchMode::Build
    } else if seat.inside() || seat.home.interior.is_some() {
        TouchMode::Interior
    } else {
        TouchMode::Play
    };
    let Some(view) = view.filter(|_| mode == TouchMode::Play) else {
        return TouchContext {
            mode,
            ..TouchContext::default()
        };
    };
    let player = view.player;
    let kits: [u32; KIT_COUNT] = [
        player.signal_relay_kits,
        player.survey_drone_kits,
        player.cargo_lift_kits,
        player.tunnel_support_kits,
        player.pump_station_kits,
        player.ore_processor_kits,
    ];
    TouchContext {
        mode,
        armed: player.mounted_weapon.is_some(),
        charges_out: waiting_charges > 0,
        prompt: view.failure.is_some()
            || view.surface_zone.is_some()
            || view.village_door.is_some()
            || view.nearby_module.is_some()
            || view.buoy.is_some()
            || view.in_colony
            || view.home_door.is_some()
            || view.plot_board.is_some(),
        kits: kits.map(|count| count > 0),
        pilot: TouchPilot::new(
            view.pilot.is_some(),
            view.can_exit,
            view.can_board && !view.fixture_first,
        ),
        pilot_armed: !player.pilot_arsenal.owned.is_empty(),
        rope: view.pilot.as_ref().is_some_and(|pilot| {
            pilot.state.gear.grapple > 0 || pilot.state.tether.is_some() || view.can_board
        }),
        // A fixture in reach, or a rig link to call the rig over with (WP-PILOT).
        usable: view.pilot_usable.is_some()
            || view
                .pilot
                .as_ref()
                .is_some_and(|pilot| pilot.state.gear.rig_link > 0),
        crypt: view.pilot.as_ref().is_some_and(|pilot| {
            pilot.state.view == fatherload_simulation::pilot::PilotView::TopDown
        }),
    }
}
