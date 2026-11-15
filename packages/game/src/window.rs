//! The desktop window: resizable and maximizable like any native window, a borderless
//! fullscreen toggle, and the restored size/maximized state remembered across runs.
//!
//! Sizes here are logical screen points (what `get_screen_width` reports) unless noted. On macOS
//! GLFW window and monitor coordinates are points too; elsewhere they are physical pixels, so
//! values crossing that boundary go through [`DesktopWindow::window_coordinate_scale`].

use raylib::{RaylibHandle, RaylibThread, core::window};

use crate::{render_snapshot::ScreenSize, save::WindowSettings};

/// Smallest window the UI is laid out for, in logical points.
pub const MIN_WINDOW_WIDTH: i32 = 960;
/// Smallest window the UI is laid out for, in logical points.
pub const MIN_WINDOW_HEIGHT: i32 = 540;

const WINDOW_TITLE: &str = "Fatherload";

/// A saved windowed size made usable: at least the minimum size and, when the monitor size is
/// known, no larger than the monitor.
#[must_use]
pub fn restored_window_size(saved: WindowSettings, monitor: Option<(i32, i32)>) -> (i32, i32) {
    let (mut width, mut height) = (saved.width, saved.height);
    if let Some((monitor_width, monitor_height)) = monitor
        && monitor_width > 0
        && monitor_height > 0
    {
        width = width.min(monitor_width);
        height = height.min(monitor_height);
    }
    (width.max(MIN_WINDOW_WIDTH), height.max(MIN_WINDOW_HEIGHT))
}

/// A rectangle in GLFW window/monitor units: `(x, y, width, height)`.
pub type ScreenRect = (i32, i32, i32, i32);

/// How much of the window's top edge (its title bar) must be on the monitor to drag it.
const TITLE_GRAB: i32 = 48;

/// Where to move a window whose title bar is not on `monitor` (its work area or bounds), or
/// `None` when it can already be grabbed. raylib centres new windows using the framebuffer
/// size, which on Windows with display scaling (seen at 150%) can land the window thousands of
/// pixels off-screen: it opens, but nothing of it is visible. The fix centres the window on the
/// monitor, top-left anchored when it does not fit.
#[must_use]
pub fn recentred_window_position(window: ScreenRect, monitor: ScreenRect) -> Option<(i32, i32)> {
    let (x, y, width, height) = window;
    let (mx, my, mw, mh) = monitor;
    if mw <= 0 || mh <= 0 {
        return None;
    }
    let overlap_x = (x + width).min(mx + mw) - x.max(mx);
    let title_on_screen = overlap_x >= TITLE_GRAB && y >= my && y + TITLE_GRAB <= my + mh;
    if title_on_screen {
        return None;
    }
    Some((
        mx + ((mw - width) / 2).max(0),
        my + ((mh - height) / 2).max(0),
    ))
}

/// What the window looks like this frame, for deciding whether its size is the one to restore.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowObservation {
    pub screen: (i32, i32),
    pub monitor: Option<(i32, i32)>,
    pub maximized: bool,
    pub minimized: bool,
    pub borderless: bool,
}

/// Whether `observation` shows a plain window whose size should be restored next run: not
/// maximized, minimized or borderless, and not covering the whole monitor (which is how macOS
/// native fullscreen from the green button looks to GLFW).
#[must_use]
pub fn is_restorable_windowed_size(observation: WindowObservation) -> bool {
    let (width, height) = observation.screen;
    let covers_monitor = observation
        .monitor
        .is_some_and(|(monitor_width, monitor_height)| {
            width >= monitor_width && height >= monitor_height
        });
    !observation.maximized
        && !observation.minimized
        && !observation.borderless
        && !covers_monitor
        && width >= MIN_WINDOW_WIDTH
        && height >= MIN_WINDOW_HEIGHT
}

/// The open desktop window and the state needed to restore it.
pub struct DesktopWindow {
    borderless: bool,
    /// Whether to re-maximize when leaving borderless fullscreen.
    maximized_before_borderless: bool,
    settings: WindowSettings,
}

impl DesktopWindow {
    /// Opens a resizable, high-DPI window at the saved size (maximized if it was).
    #[must_use]
    pub fn open(saved: WindowSettings) -> (RaylibHandle, RaylibThread, Self) {
        let (width, height) = restored_window_size(saved, None);
        // In a browser the page sizes the canvas (in device pixels) every frame, see
        // `browser::resize_canvas`: raylib's own resize handling and its half-finished web
        // high-DPI mode would fight that.
        #[cfg(target_os = "emscripten")]
        let (mut raylib, thread) = raylib::init()
            .size(width, height)
            .title(WINDOW_TITLE)
            .build();
        #[cfg(not(target_os = "emscripten"))]
        let (mut raylib, thread) = raylib::init()
            .size(width, height)
            .resizable()
            .highdpi()
            .title(WINDOW_TITLE)
            .build();
        let mut desktop = Self {
            borderless: false,
            maximized_before_borderless: false,
            settings: WindowSettings {
                width,
                height,
                maximized: saved.maximized,
            },
        };
        let scale = desktop.window_coordinate_scale(&raylib);
        // A phone's browser is smaller than the desktop minimum.
        #[cfg(not(target_os = "emscripten"))]
        raylib.set_window_min_size(
            scaled(MIN_WINDOW_WIDTH, scale),
            scaled(MIN_WINDOW_HEIGHT, scale),
        );
        let (fit_width, fit_height) = restored_window_size(saved, desktop.monitor_size(&raylib));
        if (fit_width, fit_height) != (width, height) {
            raylib.set_window_size(scaled(fit_width, scale), scaled(fit_height, scale));
            desktop.settings.width = fit_width;
            desktop.settings.height = fit_height;
        }
        #[cfg(not(target_os = "emscripten"))]
        desktop.keep_on_screen(&mut raylib, scale);
        if saved.maximized {
            raylib.maximize_window();
        }
        (raylib, thread, desktop)
    }

    /// The current logical screen size, which every layout reads each frame.
    #[must_use]
    pub fn screen_size(raylib: &RaylibHandle) -> (i32, i32) {
        (
            raylib.get_screen_width().max(1),
            raylib.get_screen_height().max(1),
        )
    }

    /// The current screen for laying out a frame: logical size plus the display density
    /// (framebuffer pixels per point, 2.0 on Retina).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen sizes are small integers"
    )]
    pub fn screen(raylib: &RaylibHandle) -> ScreenSize {
        let (width, height) = Self::screen_size(raylib);
        let density_x = raylib.get_render_width().max(1) as f32 / width as f32;
        let density_y = raylib.get_render_height().max(1) as f32 / height as f32;
        ScreenSize {
            width,
            height,
            density: density_x.min(density_y).max(1.0),
        }
    }

    /// Enters or leaves borderless fullscreen. Borderless windowed mode covers the monitor
    /// without changing the display mode or the high-DPI scaling, unlike exclusive fullscreen,
    /// so it switches instantly on macOS and the UI keeps its logical size. Leaving restores the
    /// previous windowed size (raylib keeps it) and re-maximizes if the window was maximized.
    pub fn set_fullscreen(&mut self, raylib: &mut RaylibHandle, fullscreen: bool) {
        if fullscreen == self.borderless {
            return;
        }
        if fullscreen {
            self.maximized_before_borderless = raylib.is_window_maximized();
            raylib.toggle_borderless_windowed();
            self.borderless = true;
        } else {
            raylib.toggle_borderless_windowed();
            self.borderless = false;
            if self.maximized_before_borderless {
                raylib.maximize_window();
            }
        }
    }

    /// Records the window's restorable size and maximized state; call once per frame.
    pub fn track(&mut self, raylib: &RaylibHandle) {
        let observation = WindowObservation {
            screen: Self::screen_size(raylib),
            monitor: self.monitor_size(raylib),
            maximized: raylib.is_window_maximized(),
            minimized: raylib.is_window_minimized(),
            borderless: self.borderless,
        };
        if is_restorable_windowed_size(observation) {
            self.settings.width = observation.screen.0;
            self.settings.height = observation.screen.1;
        }
        if !observation.borderless && !observation.minimized {
            self.settings.maximized = observation.maximized;
        }
    }

    /// The size/maximized state to persist.
    #[must_use]
    pub const fn settings(&self) -> WindowSettings {
        self.settings
    }

    /// Moves the window back onto its monitor when raylib placed it out of reach.
    #[cfg(not(target_os = "emscripten"))]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "window positions are small integers"
    )]
    fn keep_on_screen(&self, raylib: &mut RaylibHandle, scale: f32) {
        let monitor = window::get_current_monitor();
        if monitor < 0 || monitor >= window::get_monitor_count() {
            return;
        }
        let origin = window::get_monitor_position(monitor);
        let monitor_rect = (
            origin.x.round() as i32,
            origin.y.round() as i32,
            window::get_monitor_width(monitor),
            window::get_monitor_height(monitor),
        );
        let position = raylib.get_window_position();
        let window_rect = (
            position.x.round() as i32,
            position.y.round() as i32,
            scaled(self.settings.width, scale),
            scaled(self.settings.height, scale),
        );
        if let Some((x, y)) = recentred_window_position(window_rect, monitor_rect) {
            raylib.set_window_position(x, y);
        }
    }

    /// The current monitor's size in logical points, if raylib can report it.
    fn monitor_size(&self, raylib: &RaylibHandle) -> Option<(i32, i32)> {
        let monitor = window::get_current_monitor();
        if monitor < 0 || monitor >= window::get_monitor_count() {
            return None;
        }
        let scale = self.window_coordinate_scale(raylib);
        let width = window::get_monitor_width(monitor);
        let height = window::get_monitor_height(monitor);
        (width > 0 && height > 0).then(|| (unscaled(width, scale), unscaled(height, scale)))
    }

    /// GLFW window/monitor units per logical point: 1 on macOS (both are points), the DPI scale
    /// elsewhere (both are physical pixels).
    #[allow(
        clippy::unused_self,
        reason = "kept on the window so callers do not reach for raylib globals"
    )]
    fn window_coordinate_scale(&self, raylib: &RaylibHandle) -> f32 {
        if cfg!(target_os = "macos") {
            1.0
        } else {
            let dpi = raylib.get_window_scale_dpi();
            dpi.x.max(dpi.y).max(1.0)
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "window sizes are small integers"
)]
fn scaled(points: i32, scale: f32) -> i32 {
    (points as f32 * scale).round() as i32
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "window sizes are small integers"
)]
fn unscaled(units: i32, scale: f32) -> i32 {
    (units as f32 / scale.max(f32::EPSILON)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(width: i32, height: i32) -> WindowSettings {
        WindowSettings {
            width,
            height,
            maximized: false,
        }
    }

    #[test]
    fn saved_size_is_clamped_between_minimum_and_monitor() {
        assert_eq!(restored_window_size(saved(1280, 720), None), (1280, 720));
        assert_eq!(
            restored_window_size(saved(400, 300), Some((1920, 1080))),
            (MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)
        );
        assert_eq!(
            restored_window_size(saved(4000, 3000), Some((1512, 982))),
            (1512, 982)
        );
        assert_eq!(
            restored_window_size(saved(1600, 900), Some((0, 0))),
            (1600, 900),
            "an unknown monitor size does not shrink the window"
        );
    }

    #[test]
    fn only_plain_windows_are_remembered() {
        let plain = WindowObservation {
            screen: (1440, 810),
            monitor: Some((1728, 1117)),
            ..WindowObservation::default()
        };
        assert!(is_restorable_windowed_size(plain));
        for observation in [
            WindowObservation {
                maximized: true,
                ..plain
            },
            WindowObservation {
                minimized: true,
                ..plain
            },
            WindowObservation {
                borderless: true,
                ..plain
            },
            WindowObservation {
                screen: (1728, 1117),
                ..plain
            },
            WindowObservation {
                screen: (0, 0),
                ..plain
            },
        ] {
            assert!(!is_restorable_windowed_size(observation), "{observation:?}");
        }
    }

    #[test]
    fn off_screen_windows_are_brought_back_and_visible_ones_left_alone() {
        let monitor = (0, 0, 1920, 1080);
        // Seen on a 150% Windows desktop: raylib opened the window 21,845 px down.
        assert_eq!(
            recentred_window_position((-7, 21_845, 1295, 737), monitor),
            Some((312, 171))
        );
        assert_eq!(
            recentred_window_position((-40_000, 100, 1440, 810), monitor),
            Some((240, 135))
        );
        assert_eq!(
            recentred_window_position((200, -30, 1440, 810), monitor),
            Some((240, 135)),
            "a title bar above the screen cannot be grabbed"
        );
        assert_eq!(
            recentred_window_position((240, 135, 1440, 810), monitor),
            None
        );
        assert_eq!(
            recentred_window_position((1800, 500, 1440, 810), monitor),
            None,
            "partly off the side is fine while the title bar can be grabbed"
        );
        // A second monitor to the right.
        assert_eq!(
            recentred_window_position((-7, 21_845, 1295, 737), (1920, 0, 2560, 1440)),
            Some((2552, 351))
        );
        assert_eq!(
            recentred_window_position((0, 0, 3000, 2000), monitor),
            None,
            "too big but grabbable stays"
        );
        assert_eq!(
            recentred_window_position((0, 5000, 3000, 2000), monitor),
            Some((0, 0))
        );
    }

    #[test]
    fn window_units_round_trip_through_the_dpi_scale() {
        assert_eq!(scaled(960, 1.5), 1440);
        assert_eq!(unscaled(2880, 2.0), 1440);
        assert_eq!(unscaled(scaled(1280, 1.25), 1.25), 1280);
    }
}
