//! The desktop window: resizable and maximizable like any native window, a borderless
//! fullscreen toggle, and the restored size/maximized state remembered across runs.
//!
//! Sizes here are logical screen points (what `get_screen_width` reports) unless noted. On macOS
//! GLFW window and monitor coordinates are points too; elsewhere they are physical pixels, so
//! values crossing that boundary go through [`DesktopWindow::window_coordinate_scale`].

use raylib::{RaylibHandle, RaylibThread, core::window};

use crate::save::WindowSettings;

/// Smallest window the UI is laid out for, in logical points.
pub const MIN_WINDOW_WIDTH: i32 = 960;
/// Smallest window the UI is laid out for, in logical points.
pub const MIN_WINDOW_HEIGHT: i32 = 540;

const WINDOW_TITLE: &str = "Drillgame";

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
    fn window_units_round_trip_through_the_dpi_scale() {
        assert_eq!(scaled(960, 1.5), 1440);
        assert_eq!(unscaled(2880, 2.0), 1440);
        assert_eq!(unscaled(scaled(1280, 1.25), 1.25), 1280);
    }
}
