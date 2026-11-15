//! The pixel-perfect scaling policy shared by the world view and the UI.
//!
//! Fatherload's art is pixel art: every sprite, tile and UI frame is authored in *art pixels*. To
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
//!
//! # Phones
//!
//! A screen whose short side is under [`COMPACT_SHORT_SIDE_POINTS`] is a phone ([`is_compact`]),
//! held at arm's length and worked with thumbs. The rules above would draw a 16-art-pixel rig
//! about 3 mm wide and text 1 mm tall there, so phones get their own:
//!
//! - **World**: zoomed in to about [`COMPACT_WORLD_VIEW_HEIGHT`] world pixels vertically when
//!   held sideways, or about [`COMPACT_WORLD_VIEW_WIDTH`] horizontally when held upright (a
//!   tall view down the shaft).
//! - **UI**: about [`COMPACT_UI_POINTS_PER_PIXEL`] points per art pixel, so text and rows are
//!   finger-sized, on a canvas no smaller than [`MIN_COMPACT_UI_CANVAS`] (the size the
//!   split-screen layouts already support).

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

/// A screen whose short side is under this many points is a phone (desktop windows cannot be
/// made this small, and tablets are larger).
pub const COMPACT_SHORT_SIDE_POINTS: i32 = 520;

/// World pixels a phone held sideways shows vertically (15 tiles; see the module docs).
pub const COMPACT_WORLD_VIEW_HEIGHT: f32 = 480.0;

/// World pixels a phone held upright shows horizontally (12.5 tiles).
pub const COMPACT_WORLD_VIEW_WIDTH: f32 = 400.0;

/// Screen points per UI art pixel a phone aims for: the 7-pixel capitals come out about 11
/// points tall and a 26-pixel row about 42.
pub const COMPACT_UI_POINTS_PER_PIXEL: f32 = 1.6;

/// Smallest UI canvas (art pixels) a phone is given: what a split-screen seat's layouts
/// already fit in.
pub const MIN_COMPACT_UI_CANVAS: (i32, i32) = (232, 160);

/// Framebuffer pixels per point that the window system does not report as density: a browser
/// canvas is sized in device pixels, so its "screen" is already physical and the page's device
/// pixel ratio arrives here instead. 1 everywhere else.
static DISPLAY_SCALE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1.0_f32.to_bits());

/// Sets the display scale (see [`display_scale`]).
#[cfg_attr(not(target_os = "emscripten"), allow(dead_code))]
pub fn set_display_scale(scale: f32) {
    let scale = if scale.is_finite() {
        scale.clamp(0.25, 8.0)
    } else {
        1.0
    };
    DISPLAY_SCALE.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Screen units per point beyond the reported density: the browser's device pixel ratio, 1 on
/// native windows. Sizes meant in points (a finger-sized button) are multiplied by it.
#[must_use]
pub fn display_scale() -> f32 {
    f32::from_bits(DISPLAY_SCALE.load(std::sync::atomic::Ordering::Relaxed))
}

/// Whether a `width` x `height`-point screen is a phone (see [`COMPACT_SHORT_SIDE_POINTS`]).
#[must_use]
pub fn is_compact(width: i32, height: i32) -> bool {
    width.min(height) < COMPACT_SHORT_SIDE_POINTS
}

/// Physical pixels per art pixel for the world on a `physical_width` x `physical_height`
/// framebuffer at `density` physical pixels per point: the integer nearest to
/// `physical height / REFERENCE_ART_HEIGHT`, at least 1. Phones zoom in (see the module docs).
#[must_use]
pub fn world_art_pixel_size(physical_width: i32, physical_height: i32, density: f32) -> i32 {
    let density = density.max(1.0) * display_scale();
    let (width, height) = points((physical_width, physical_height), density);
    if !is_compact(width, height) {
        return nearest_pixel_size(physical_height, REFERENCE_ART_HEIGHT);
    }
    if height > width {
        nearest_pixel_size(
            physical_width,
            COMPACT_WORLD_VIEW_WIDTH / WORLD_PIXELS_PER_ART_PIXEL,
        )
    } else {
        nearest_pixel_size(
            physical_height,
            COMPACT_WORLD_VIEW_HEIGHT / WORLD_PIXELS_PER_ART_PIXEL,
        )
    }
}

/// Screen points per world pixel (what [`crate::render_snapshot::ViewSnapshot::world_scale`]
/// holds) for a `screen_width` x `screen_height`-point screen at `density` physical pixels per
/// point: the snapped art pixel size, in points, divided by [`WORLD_PIXELS_PER_ART_PIXEL`].
#[must_use]
pub fn world_scale(screen_width: i32, screen_height: i32, density: f32) -> f32 {
    let density = density.max(1.0);
    #[allow(clippy::cast_precision_loss, reason = "pixel sizes are tiny integers")]
    let art_pixel = world_art_pixel_size(
        physical(screen_width, density),
        physical(screen_height, density),
        density,
    ) as f32;
    art_pixel / (WORLD_PIXELS_PER_ART_PIXEL * density)
}

/// Physical pixels per UI art pixel for a `physical_width` x `physical_height` framebuffer at
/// `density` physical pixels per point.
#[must_use]
pub fn ui_pixel_size(physical_width: i32, physical_height: i32, density: f32) -> i32 {
    let density = density.max(1.0) * display_scale();
    let (width, height) = points((physical_width, physical_height), density);
    if is_compact(width, height) {
        return compact_ui_pixel_size(physical_width, physical_height, density);
    }
    let by_height = nearest_pixel_size(physical_height, REFERENCE_ART_HEIGHT);
    let by_width = (physical_width / MIN_UI_CANVAS_WIDTH).max(1);
    by_height.min(by_width)
}

/// A phone's UI pixel: the whole pixel size nearest [`COMPACT_UI_POINTS_PER_PIXEL`] points,
/// stepped down while the canvas would be smaller than [`MIN_COMPACT_UI_CANVAS`] (the short
/// side against its height, the long side against its width).
fn compact_ui_pixel_size(physical_width: i32, physical_height: i32, density: f32) -> i32 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a pixel size is a tiny integer"
    )]
    let mut pixel =
        ((COMPACT_UI_POINTS_PER_PIXEL * density).round() as i32).clamp(1, MAX_PIXEL_SIZE);
    let (min_width, min_height) = MIN_COMPACT_UI_CANVAS;
    while pixel > 1 && (physical_width / pixel < min_width || physical_height / pixel < min_height)
    {
        pixel -= 1;
    }
    pixel
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

/// The whole pixel size at which `art_pixels` art pixels come nearest to spanning `physical`
/// framebuffer pixels.
fn nearest_pixel_size(physical: i32, art_pixels: f32) -> i32 {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "framebuffer sizes are small integers"
    )]
    let size = (physical.max(1) as f32 / art_pixels.max(1.0)).round() as i32;
    size.clamp(1, MAX_PIXEL_SIZE)
}

/// A framebuffer size in logical screen points.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "screen sizes are small integers"
)]
fn points(physical: (i32, i32), density: f32) -> (i32, i32) {
    let density = density.max(0.25);
    (
        (physical.0 as f32 / density).round() as i32,
        (physical.1 as f32 / density).round() as i32,
    )
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
        let desktop = |height| world_art_pixel_size(height * 16 / 9, height, 1.0);
        assert_eq!(desktop(720), 2);
        assert_eq!(desktop(1080), 3);
        assert_eq!(desktop(1440), 4);
        assert_eq!(desktop(2160), 6);
        assert_eq!(desktop(540), 2, "1.5 rounds up");
        assert_eq!(desktop(900), 3, "2.5 rounds up");
        assert_eq!(desktop(100), 1);
        assert_eq!(desktop(100_000), MAX_PIXEL_SIZE);
        for height in [540, 600, 720, 768, 900, 1000, 1080, 1200, 1440, 1600, 2160] {
            for density in [1.0_f32, 2.0] {
                let zoom = wide(height, density) * density * WORLD_PIXELS_PER_ART_PIXEL;
                assert_near(zoom, zoom.round());
            }
        }
    }

    #[test]
    fn visible_world_stays_near_the_reference_height() {
        for height in [720, 1080, 1440, 2160] {
            assert_near(height as f32 / wide(height, 1.0), 720.0);
        }
        // A Retina 1280x720 window has a 1440-pixel framebuffer: same view.
        assert_near(720.0 / wide(720, 2.0), 720.0);
        for height in 540..=2400 {
            let visible = height as f32 / wide(height, 1.0);
            assert!((480.0..=1080.0).contains(&visible), "{height}: {visible}");
        }
    }

    /// The world scale of a 16:9 screen `height` points tall.
    fn wide(height: i32, density: f32) -> f32 {
        world_scale(height * 16 / 9, height, density)
    }

    /// Phones as (width, height, density) held sideways; `upright` swaps the sides.
    const PHONES: &[(i32, i32, f32)] = &[
        (844, 390, 3.0),
        (932, 430, 3.0),
        (667, 375, 2.0),
        (915, 412, 2.625),
        (800, 360, 3.0),
        // A phone browser that reports no pixel ratio.
        (844, 390, 1.0),
    ];

    #[test]
    fn desktops_and_tablets_are_not_phones() {
        for (width, height) in [
            (960, 540),
            (1280, 720),
            (1180, 820),
            (820, 1180),
            (744, 1133),
        ] {
            assert!(!is_compact(width, height), "{width}x{height}");
        }
        for (width, height, _) in PHONES {
            assert!(is_compact(*width, *height) && is_compact(*height, *width));
        }
    }

    #[test]
    fn phones_zoom_in_on_whole_pixels_and_still_show_a_playable_slice() {
        for (width, height, density) in PHONES {
            let sideways = world_scale(*width, *height, *density);
            let zoom = sideways * density * WORLD_PIXELS_PER_ART_PIXEL;
            assert_near(zoom, zoom.round());
            let tall = *height as f32 / sideways;
            assert!(
                (380.0..=600.0).contains(&tall),
                "{width}x{height}@{density}: {tall} world pixels tall"
            );
            let desktop_rule = (*height as f32 * density / REFERENCE_ART_HEIGHT)
                .round()
                .max(1.0)
                / (WORLD_PIXELS_PER_ART_PIXEL * density);
            assert!(
                sideways > desktop_rule,
                "a phone shows the world larger than the desktop rule would"
            );

            let upright = world_scale(*height, *width, *density);
            let zoom = upright * density * WORLD_PIXELS_PER_ART_PIXEL;
            assert_near(zoom, zoom.round());
            let across = *height as f32 / upright;
            assert!(
                (320.0..=520.0).contains(&across),
                "{height}x{width}@{density}: {across} world pixels across"
            );
            assert!(
                *width as f32 / upright > across * 1.5,
                "upright shows a tall slice of the shaft"
            );
        }
    }

    #[test]
    fn phone_ui_is_finger_sized_on_a_canvas_the_layouts_fit() {
        for (width, height, density) in PHONES {
            for (width, height) in [(*width, *height), (*height, *width)] {
                let physical = (physical(width, *density), physical(height, *density));
                let pixel = ui_pixel_size(physical.0, physical.1, *density);
                let points_per_pixel = pixel as f32 / density;
                // Without a pixel ratio an upright phone has too few pixels for larger text:
                // it keeps a canvas the layouts fit instead.
                let coarse_upright = *density < 1.5 && height > width;
                assert!(
                    coarse_upright || (1.3..=2.0).contains(&points_per_pixel),
                    "{width}x{height}@{density}: {points_per_pixel} points per art pixel"
                );
                assert!(
                    physical.0 / pixel >= MIN_COMPACT_UI_CANVAS.0
                        && physical.1 / pixel >= MIN_COMPACT_UI_CANVAS.1,
                    "{width}x{height}@{density}: canvas {}x{}",
                    physical.0 / pixel,
                    physical.1 / pixel
                );
            }
        }
    }

    #[test]
    fn ui_canvas_is_reference_sized_and_never_too_narrow() {
        assert_eq!(ui_pixel_size(1280, 720, 1.0), 2);
        assert_eq!(ui_pixel_size(1920, 1080, 1.0), 3);
        assert_eq!(ui_pixel_size(2560, 1440, 1.0), 4);
        assert_eq!(
            ui_pixel_size(2560, 1440, 2.0),
            4,
            "a Retina 1280x720 window"
        );
        assert_eq!(ui_pixel_size(960, 540, 1.0), 2);
        assert_eq!(
            ui_pixel_size(800, 1080, 1.0),
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
            let size = ui_pixel_size(width, height, 1.0);
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
