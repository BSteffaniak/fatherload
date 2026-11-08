//! The pixel-perfect scaling policy shared by the world view and the UI.
//!
//! Drillgame's art is pixel art: every sprite, tile and UI frame is authored in *art pixels*. To
//! keep it crisp (no shimmering, no blurred or uneven pixels) one art pixel always covers a whole
//! number of physical framebuffer pixels. Two integer scales are chosen from the framebuffer
//! size every frame:
//!
//! - **World**: one art pixel is [`WORLD_PIXELS_PER_ART_PIXEL`] world pixels (a 32-world-pixel
//!   tile is 16 art pixels). The world zoom is snapped so an art pixel spans
//!   [`world_art_pixel_size`] physical pixels: `round(physical height / REFERENCE_ART_HEIGHT)`.
//!   That keeps the visible-world rule of [`WORLD_VIEW_HEIGHT`] exactly at
//!   720 / 1080 / 1440 / 2160 physical pixels and within a rounding step of it otherwise (e.g.
//!   900 px rounds to 3 px per art pixel and shows 600 world pixels instead of 720).
//! - **UI**: screen-space UI is laid out on a virtual canvas in art pixels and drawn through a
//!   camera that scales it by [`ui_pixel_size`] physical pixels (also
//!   `round(physical height / REFERENCE_ART_HEIGHT)`, but limited so the canvas never gets
//!   narrower than [`MIN_UI_CANVAS_WIDTH`]). At 1280x720 the canvas is 640x360 art pixels.
//!
//! Positions are snapped as well: the world camera to whole physical pixels
//! ([`snap_to_pixel_grid`]) and the UI to whole art pixels (layout works in integers).

/// World pixels that fit vertically on the screen, whatever the window height. A taller window
/// draws the same slice of the mine larger and crisper instead of revealing more of it; wider
/// windows still see more horizontally. Because the zoom is snapped (see the module docs) this
/// span holds exactly at 720/1080/1440/2160 physical pixels and within one rounding step
/// elsewhere. Tune it to zoom the whole game in (smaller) or out (larger).
pub const WORLD_VIEW_HEIGHT: f32 = 720.0;

/// World pixels per art pixel: tiles are 32 world pixels and 16 art pixels.
pub const WORLD_PIXELS_PER_ART_PIXEL: f32 = 2.0;

/// Art pixels that fit vertically at the reference scale (720 world pixels / 2).
pub const REFERENCE_ART_HEIGHT: f32 = WORLD_VIEW_HEIGHT / WORLD_PIXELS_PER_ART_PIXEL;

/// Narrowest UI canvas (art pixels) the layout must support; the UI scale is lowered for very
/// narrow windows so the canvas stays at least this wide.
pub const MIN_UI_CANVAS_WIDTH: i32 = 448;

/// Largest pixel scale ever chosen (8K-class displays).
pub const MAX_PIXEL_SIZE: i32 = 12;

/// Physical pixels per art pixel for a framebuffer `physical_height` pixels tall: the integer
/// nearest to `physical_height / REFERENCE_ART_HEIGHT`, at least 1.
#[must_use]
pub fn world_art_pixel_size(physical_height: i32) -> i32 {
    nearest_pixel_size(physical_height)
}

/// Screen points per world pixel (what [`crate::render_snapshot::ViewSnapshot::world_scale`]
/// holds) for a `screen_height`-point screen at `density` physical pixels per point: the snapped
/// art pixel size, in points, divided by [`WORLD_PIXELS_PER_ART_PIXEL`].
#[must_use]
pub fn world_scale(screen_height: i32, density: f32) -> f32 {
    let density = density.max(1.0);
    let physical_height = physical(screen_height, density);
    #[allow(clippy::cast_precision_loss, reason = "pixel sizes are tiny integers")]
    let art_pixel = world_art_pixel_size(physical_height) as f32;
    art_pixel / (WORLD_PIXELS_PER_ART_PIXEL * density)
}

/// Physical pixels per UI art pixel for a `physical_width` x `physical_height` framebuffer.
#[must_use]
pub fn ui_pixel_size(physical_width: i32, physical_height: i32) -> i32 {
    let by_height = nearest_pixel_size(physical_height);
    let by_width = (physical_width / MIN_UI_CANVAS_WIDTH).max(1);
    by_height.min(by_width)
}

/// Largest whole pixel size at which a `canvas` (art pixels) fits inside `physical` pixels, at
/// least 1. Building interiors use it to show their fixed-size room as large as possible.
#[must_use]
pub fn fit_pixel_size(physical: (i32, i32), canvas: (i32, i32)) -> i32 {
    let by_width = physical.0 / canvas.0.max(1);
    let by_height = physical.1 / canvas.1.max(1);
    by_width.min(by_height).clamp(1, MAX_PIXEL_SIZE)
}

/// Snaps a world coordinate so it lands on a whole physical pixel at `physical_zoom` physical
/// pixels per world pixel.
#[must_use]
pub fn snap_to_pixel_grid(value: f32, physical_zoom: f32) -> f32 {
    if physical_zoom <= f32::EPSILON {
        return value;
    }
    (value * physical_zoom).round() / physical_zoom
}

fn nearest_pixel_size(physical_height: i32) -> i32 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "framebuffer heights are small integers"
    )]
    let size = (physical_height.max(1) as f32 / REFERENCE_ART_HEIGHT).round() as i32;
    size.clamp(1, MAX_PIXEL_SIZE)
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "screen sizes are small integers"
)]
fn physical(points: i32, density: f32) -> i32 {
    (points.max(1) as f32 * density).round() as i32
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, reason = "test sizes are small")]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn art_pixels_are_whole_physical_pixels() {
        assert_eq!(world_art_pixel_size(720), 2);
        assert_eq!(world_art_pixel_size(1080), 3);
        assert_eq!(world_art_pixel_size(1440), 4);
        assert_eq!(world_art_pixel_size(2160), 6);
        assert_eq!(world_art_pixel_size(540), 2, "1.5 rounds up");
        assert_eq!(world_art_pixel_size(900), 3, "2.5 rounds up");
        assert_eq!(world_art_pixel_size(100), 1);
        assert_eq!(world_art_pixel_size(100_000), MAX_PIXEL_SIZE);
        for height in [540, 600, 720, 768, 900, 1000, 1080, 1200, 1440, 1600, 2160] {
            for density in [1.0_f32, 2.0] {
                let zoom = world_scale(height, density) * density * WORLD_PIXELS_PER_ART_PIXEL;
                assert_near(zoom, zoom.round());
            }
        }
    }

    #[test]
    fn visible_world_stays_near_the_reference_height() {
        for height in [720, 1080, 1440, 2160] {
            assert_near(height as f32 / world_scale(height, 1.0), 720.0);
        }
        // A Retina 1280x720 window has a 1440-pixel framebuffer: same view.
        assert_near(720.0 / world_scale(720, 2.0), 720.0);
        for height in 540..=2400 {
            let visible = height as f32 / world_scale(height, 1.0);
            assert!((480.0..=1080.0).contains(&visible), "{height}: {visible}");
        }
    }

    #[test]
    fn ui_canvas_is_reference_sized_and_never_too_narrow() {
        assert_eq!(ui_pixel_size(1280, 720), 2);
        assert_eq!(ui_pixel_size(1920, 1080), 3);
        assert_eq!(ui_pixel_size(2560, 1440), 4);
        assert_eq!(ui_pixel_size(960, 540), 2);
        assert_eq!(
            ui_pixel_size(800, 1080),
            1,
            "narrow windows keep a usable canvas"
        );
        for (width, height) in [
            (960, 540),
            (1280, 720),
            (1920, 1080),
            (2560, 1080),
            (1512, 982),
        ] {
            let size = ui_pixel_size(width, height);
            assert!(width / size >= MIN_UI_CANVAS_WIDTH, "{width}x{height}");
        }
    }

    #[test]
    fn rooms_fit_at_the_largest_whole_scale() {
        assert_eq!(fit_pixel_size((1280, 720), (320, 180)), 4);
        assert_eq!(fit_pixel_size((1920, 1080), (320, 180)), 6);
        assert_eq!(fit_pixel_size((960, 540), (320, 180)), 3);
        assert_eq!(fit_pixel_size((2560, 1080), (320, 180)), 6);
        assert_eq!(fit_pixel_size((200, 100), (320, 180)), 1);
    }

    #[test]
    fn snapping_lands_on_physical_pixels() {
        assert_near(snap_to_pixel_grid(10.2, 1.5), 10.0);
        assert_near(snap_to_pixel_grid(10.4, 1.5), 32.0 / 3.0);
        assert_near(snap_to_pixel_grid(3.3, 0.0), 3.3);
    }
}
