//! Test-mode debug views and the time scale (Wave 14 console, expansion plan §19 W14-D).
//!
//! Every view is off by default and only takes effect in a test-mode world: the shell keeps the
//! switches, but [`DebugViews::effective`] answers "off" for every view unless the world is in
//! test mode, so a normal world never draws them (and the palette that sets them only opens in
//! test mode). They are presentation only: nothing here reaches the simulation.
//!
//! The time scale is the local host's: it runs whole fixed ticks faster or slower (the frame's
//! real time is scaled before the session's accumulator turns it into ticks), never a varied
//! `dt`, so a scaled run is the same run as a normal one, tick for tick. Online sessions keep
//! real time (every peer runs the host's clock).

/// One debug view.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DebugView {
    /// Hit circles and contact boxes of rigs, creatures and targets.
    HitBoxes,
    /// The outlines of field zones (F9) with their preset names.
    FieldZones,
    /// The terrain chunk grid and the region (sector) cells.
    ChunkGrid,
    /// No fog: every tile drawn as if explored.
    MapReveal,
    /// The camera leaves the craft: the seat's stick pans it, rotate zooms it out and in.
    FreeCamera,
    /// Frames per second and the time the frame's ticks took.
    FrameStats,
}

impl DebugView {
    pub const ALL: [Self; 6] = [
        Self::HitBoxes,
        Self::FieldZones,
        Self::ChunkGrid,
        Self::MapReveal,
        Self::FreeCamera,
        Self::FrameStats,
    ];

    /// The palette's word for it (`view <word>`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::HitBoxes => "hitboxes",
            Self::FieldZones => "fields",
            Self::ChunkGrid => "grid",
            Self::MapReveal => "reveal",
            Self::FreeCamera => "freecam",
            Self::FrameStats => "fps",
        }
    }

    /// What it shows, for help and toasts.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::HitBoxes => "Hit and contact boxes",
            Self::FieldZones => "Field zones",
            Self::ChunkGrid => "Chunk and region grid",
            Self::MapReveal => "Map reveal (no fog)",
            Self::FreeCamera => "Free camera",
            Self::FrameStats => "FPS and tick time",
        }
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// The free camera's own place and zoom (world pixels of the seat's frame).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreeCamera {
    /// Offset of the camera's centre from the craft (pixels).
    pub offset: (f32, f32),
    /// Zoom-out steps (each halves the scale), 0..=[`MAX_ZOOM_OUT`].
    pub zoom_out: u8,
}

/// Most zoom-out steps of the free camera (1/16 on the desktop). Every step quadruples the
/// terrain chunks on screen (each a 512x512 texture painted on arrival), so the browser build
/// stops at 1/4: about two dozen chunks, which WebGL on a phone holds at 60 fps.
#[cfg(not(target_os = "emscripten"))]
pub const MAX_ZOOM_OUT: u8 = 4;
/// Most zoom-out steps of the free camera (the browser build's cap; see the desktop's).
#[cfg(target_os = "emscripten")]
pub const MAX_ZOOM_OUT: u8 = 2;

/// Pixels per second the free camera pans at full stick, before zoom.
const FREE_CAMERA_SPEED: f32 = 900.0;

impl FreeCamera {
    /// The scale multiplier the zoom gives.
    #[must_use]
    pub fn scale(self) -> f32 {
        0.5_f32.powi(i32::from(self.zoom_out))
    }

    /// Moves the camera by a frame of the seat's stick: `(horizontal, vertical)` in -1..=1
    /// (`vertical` positive is down).
    pub fn pan(&mut self, stick: (f32, f32), delta_seconds: f32) {
        let speed = FREE_CAMERA_SPEED * delta_seconds / self.scale();
        self.offset.0 = stick.0.mul_add(speed, self.offset.0);
        self.offset.1 = stick.1.mul_add(speed, self.offset.1);
    }

    /// One zoom step out (`out`) or in.
    pub fn zoom(&mut self, out: bool) {
        self.zoom_out = if out {
            (self.zoom_out + 1).min(MAX_ZOOM_OUT)
        } else {
            self.zoom_out.saturating_sub(1)
        };
    }
}

/// The debug views switched on (all off by default).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DebugViews {
    on: u8,
    pub free_camera: FreeCamera,
}

impl DebugViews {
    /// Whether `view` is switched on (whatever the world).
    #[must_use]
    pub const fn is_on(self, view: DebugView) -> bool {
        self.on & view.bit() != 0
    }

    /// Switches `view` on or off; returns the new state.
    pub const fn set(&mut self, view: DebugView, on: bool) -> bool {
        if on {
            self.on |= view.bit();
        } else {
            self.on &= !view.bit();
            if matches!(view, DebugView::FreeCamera) {
                self.free_camera = FreeCamera {
                    offset: (0.0, 0.0),
                    zoom_out: 0,
                };
            }
        }
        on
    }

    /// Flips `view`; returns the new state.
    pub const fn toggle(&mut self, view: DebugView) -> bool {
        let on = !self.is_on(view);
        self.set(view, on)
    }

    /// The views in effect for a world: none outside test mode.
    #[must_use]
    pub const fn effective(self, test_mode: bool) -> Self {
        if test_mode {
            self
        } else {
            Self {
                on: 0,
                free_camera: FreeCamera {
                    offset: (0.0, 0.0),
                    zoom_out: 0,
                },
            }
        }
    }

    /// Whether `view` takes effect in a world (`test_mode`: the world's flag).
    #[must_use]
    pub const fn shows(self, view: DebugView, test_mode: bool) -> bool {
        self.effective(test_mode).is_on(view)
    }
}

/// The FPS overlay's readings, kept by the frame loop.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// Frames per second, smoothed.
    pub fps: f32,
    /// Milliseconds the last frame's ticks took to simulate (smoothed).
    pub tick_ms: f32,
    /// Ticks the last frame ran.
    pub ticks: u32,
}

impl FrameStats {
    /// Folds in one frame: `delta_seconds` of real time, `ticks` run in `tick_seconds`.
    pub fn record(&mut self, delta_seconds: f32, ticks: u32, tick_seconds: f32) {
        const KEEP: f32 = 0.9;
        if delta_seconds > 0.0 {
            let fps = 1.0 / delta_seconds;
            self.fps = if self.fps > 0.0 {
                self.fps.mul_add(KEEP, fps * (1.0 - KEEP))
            } else {
                fps
            };
        }
        let millis = tick_seconds * 1_000.0;
        self.tick_ms = self.tick_ms.mul_add(KEEP, millis * (1.0 - KEEP));
        self.ticks = ticks;
    }
}

/// How fast the local world runs (whole ticks per real second, scaled).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TimeScale {
    /// No ticks.
    Paused,
    /// Half speed.
    Half,
    /// Real time.
    #[default]
    Normal,
    /// Twice as fast.
    Double,
    /// Four times as fast.
    Quadruple,
}

impl TimeScale {
    pub const ALL: [Self; 5] = [
        Self::Paused,
        Self::Half,
        Self::Normal,
        Self::Double,
        Self::Quadruple,
    ];

    /// The palette's word for it (`speed <word>`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Paused => "pause",
            Self::Half => "0.5",
            Self::Normal => "1",
            Self::Double => "2",
            Self::Quadruple => "4",
        }
    }

    /// Real time multiplier.
    #[must_use]
    pub const fn factor(self) -> f32 {
        match self {
            Self::Paused => 0.0,
            Self::Half => 0.5,
            Self::Normal => 1.0,
            Self::Double => 2.0,
            Self::Quadruple => 4.0,
        }
    }

    /// The real time the session's accumulator sees for a frame of `delta_seconds`: whole
    /// fixed ticks come out of it, so the scale changes how many ticks run, never their `dt`.
    #[must_use]
    pub fn scaled(self, delta_seconds: f32) -> f32 {
        delta_seconds.max(0.0) * self.factor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_is_off_by_default_and_none_shows_outside_test_mode() {
        let mut views = DebugViews::default();
        for view in DebugView::ALL {
            assert!(!views.is_on(view));
            assert!(!views.shows(view, true));
            views.set(view, true);
        }
        for view in DebugView::ALL {
            assert!(views.shows(view, true), "{view:?} in test mode");
            assert!(!views.shows(view, false), "{view:?} in a normal world");
        }
        assert_eq!(DebugViews::default().effective(true), DebugViews::default());
    }

    #[test]
    fn toggles_flip_and_the_free_camera_resets_when_closed() {
        let mut views = DebugViews::default();
        assert!(views.toggle(DebugView::FreeCamera));
        views.free_camera.pan((1.0, 0.0), 1.0);
        views.free_camera.zoom(true);
        assert!(views.free_camera.offset.0 > 0.0);
        assert!(!views.toggle(DebugView::FreeCamera));
        assert_eq!(views.free_camera, FreeCamera::default());
    }

    #[test]
    fn time_scale_scales_real_time_not_the_tick() {
        assert!(TimeScale::Paused.scaled(0.016).abs() < f32::EPSILON);
        assert!((TimeScale::Quadruple.scaled(0.25) - 1.0).abs() < f32::EPSILON);
        assert!((TimeScale::default().scaled(0.5) - 0.5).abs() < f32::EPSILON);
    }
}
