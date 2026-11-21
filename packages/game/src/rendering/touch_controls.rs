//! Draws the touch controls over the game: the floating stick, the thumb cluster and the small
//! buttons along the top, exactly where [`crate::input::touch::TouchLayout`] hit-tests them.
//!
//! The overlay for the frame is published with [`set_overlay`] before drawing, so the HUD can
//! make room for it ([`active`], [`thumb_corner`]).

use std::cell::RefCell;

use raylib::prelude::*;

use super::{
    pixel_art::palette,
    ui_kit::{self, TextStyle},
};
use crate::{
    input::touch::{TouchControl, TouchMode, TouchRect},
    touch_screen::TouchOverlay,
};

thread_local! {
    static OVERLAY: RefCell<Option<TouchOverlay>> = const { RefCell::new(None) };
}

/// Publishes the touch controls to draw this frame (`None`: touch is not the device in use).
pub fn set_overlay(overlay: Option<TouchOverlay>) {
    OVERLAY.with(|current| *current.borrow_mut() = overlay);
}

/// Whether the touch controls are on screen this frame.
#[must_use]
pub fn active() -> bool {
    OVERLAY.with(|current| current.borrow().is_some())
}

/// The width (canvas art pixels) the thumb cluster takes along the bottom edge, and whether it
/// sits on the left (left-handed) instead of the right. `None` without touch controls.
#[must_use]
pub fn thumb_corner() -> Option<(f32, bool)> {
    OVERLAY.with(|current| {
        let current = current.borrow();
        let overlay = current.as_ref()?;
        let scale = ui_kit::canvas().points_per_pixel().max(0.1);
        let canvas_width = ui_kit::canvas().width as f32;
        let middle = canvas_width * 0.5;
        // Top-row buttons are centred; everything else hangs off the thumb corner.
        let cluster: Vec<TouchRect> = overlay
            .layout
            .controls
            .iter()
            .filter(|(control, _)| !is_top_row(*control) && *control != TouchControl::Prompt)
            .map(|(_, rect)| *rect)
            .collect();
        let first = cluster.first()?;
        let mirrored = first.centre().0 / scale < middle;
        let width = cluster
            .iter()
            .map(|rect| {
                if mirrored {
                    (rect.x + rect.w) / scale
                } else {
                    canvas_width - rect.x / scale
                }
            })
            .fold(0.0_f32, f32::max);
        Some((width.ceil(), mirrored))
    })
}

/// On a screen taller than it is wide, the top of the thumb cluster in the current canvas's
/// art pixels (toasts rest above it). `None` sideways or without touch controls.
#[must_use]
pub fn upright_controls_top() -> Option<i32> {
    OVERLAY.with(|current| {
        let current = current.borrow();
        let overlay = current.as_ref()?;
        let canvas = ui_kit::canvas();
        let top = overlay
            .layout
            .controls
            .iter()
            .filter(|(control, _)| !is_top_row(*control) && *control != TouchControl::Prompt)
            .map(|(_, rect)| rect.y)
            .reduce(f32::min)?;
        let upright = canvas.height > canvas.width;
        upright
            .then(|| ((top * canvas.density - canvas.origin_y as f32) / canvas.pixel as f32) as i32)
    })
}

/// The small top-row buttons (pause, bag, map, ...) in the current canvas's art pixels, so a
/// banner across the top can step below them. Empty without touch controls.
#[must_use]
pub fn top_row_rects() -> Vec<Rectangle> {
    OVERLAY.with(|current| {
        let current = current.borrow();
        let Some(overlay) = current.as_ref() else {
            return Vec::new();
        };
        let canvas = ui_kit::canvas();
        let pixel = canvas.pixel.max(1) as f32;
        overlay
            .layout
            .controls
            .iter()
            .filter(|(control, _)| is_top_row(*control))
            .map(|(_, rect)| {
                Rectangle::new(
                    rect.x.mul_add(canvas.density, -(canvas.origin_x as f32)) / pixel,
                    rect.y.mul_add(canvas.density, -(canvas.origin_y as f32)) / pixel,
                    rect.w * canvas.density / pixel,
                    rect.h * canvas.density / pixel,
                )
            })
            .collect()
    })
}

const fn is_top_row(control: TouchControl) -> bool {
    matches!(
        control,
        TouchControl::Pause
            | TouchControl::Inventory
            | TouchControl::Map
            | TouchControl::BuildExit
            | TouchControl::ExitRig
    )
}

/// Draws the published overlay on the whole-screen canvas.
pub(super) fn draw_touch_controls(draw: &mut RaylibDrawHandle<'_>) {
    OVERLAY.with(|current| {
        if let Some(overlay) = current.borrow().as_ref() {
            draw_overlay(draw, overlay);
        }
    });
}

fn draw_overlay(draw: &mut RaylibDrawHandle<'_>, overlay: &TouchOverlay) {
    let scale = ui_kit::canvas().points_per_pixel().max(0.1);
    let art = |value: f32| (value / scale).round() as i32;
    let layout = &overlay.layout;

    draw_stick(draw, overlay, scale);
    for (control, rect) in &layout.controls {
        // The prompt under the rig is drawn by the HUD; here it is only a hit area.
        if *control == TouchControl::Prompt {
            continue;
        }
        let (x, y) = rect.centre();
        let radius = art(rect.w.min(rect.h) * 0.5).max(4);
        let held = overlay.view.held.contains(control);
        let open = *control == TouchControl::Tools && overlay.view.tools_open;
        disc(
            draw,
            art(x),
            art(y),
            radius,
            face(held || open),
            rim(*control),
        );
        if let Some((aiming, (aim_x, aim_y))) = overlay.view.aim
            && aiming == *control
        {
            // The thumb's pull, as a knob riding out of the button.
            let reach = radius as f32;
            disc(
                draw,
                art(x) + (aim_x * reach) as i32,
                art(y) + (aim_y * reach) as i32,
                (radius / 3).max(2),
                with_alpha(palette::GOLD, 200),
                palette::INK,
            );
        }
        let label = label(*control);
        let style = if radius >= 14 {
            TextStyle::Bold
        } else {
            TextStyle::Body
        };
        ui_kit::text_centred(
            draw,
            label,
            art(x),
            art(y) - style.height() / 2,
            style,
            palette::BONE,
        );
    }
}

/// The floating stick: its ring and knob under the thumb, or a faint ring where the thumb
/// would rest.
fn draw_stick(draw: &mut RaylibDrawHandle<'_>, overlay: &TouchOverlay, scale: f32) {
    let layout = &overlay.layout;
    let zone = layout.stick_zone;
    if zone.w <= 0.0 || overlay.context.mode == TouchMode::Menu {
        return;
    }
    let art = |value: f32| (value / scale).round() as i32;
    let radius = art(layout.stick_radius()).max(6);
    let knob = (radius * 2 / 5).max(3);
    if let Some((base, thumb)) = overlay.view.stick {
        ring(
            draw,
            art(base.0),
            art(base.1),
            radius,
            with_alpha(palette::BONE, 150),
        );
        disc(
            draw,
            art(thumb.0),
            art(thumb.1),
            knob,
            with_alpha(palette::BONE, 150),
            palette::INK,
        );
    } else {
        // Resting: where a thumb naturally lands, towards the outer bottom corner.
        let mirrored = zone.centre().0 / scale > ui_kit::canvas().width as f32 * 0.5;
        let inset = layout.button * 1.7;
        let x = if mirrored {
            zone.x + zone.w - inset
        } else {
            zone.x + inset
        };
        let y = zone.y + zone.h - inset;
        ring(draw, art(x), art(y), radius, with_alpha(palette::BONE, 60));
        ring(draw, art(x), art(y), knob, with_alpha(palette::BONE, 60));
    }
}

const fn face(lit: bool) -> Color {
    if lit {
        with_alpha(palette::AMBER, 170)
    } else {
        with_alpha(palette::INK, 120)
    }
}

/// The ring colour says what kind of button it is.
const fn rim(control: TouchControl) -> Color {
    let color = match control {
        TouchControl::Fire | TouchControl::CycleWeapon => palette::RED,
        TouchControl::Bomb | TouchControl::CycleExplosive | TouchControl::Detonate => {
            palette::ORANGE
        }
        TouchControl::Scan | TouchControl::Rope => palette::MINT,
        TouchControl::Use
        | TouchControl::BuildPlace
        | TouchControl::Build
        | TouchControl::Kit(_) => palette::GOLD,
        TouchControl::Tools
        | TouchControl::Thrust
        | TouchControl::Jump
        | TouchControl::Roll
        | TouchControl::ExitRig
        | TouchControl::BoardRig
        | TouchControl::Prompt
        | TouchControl::Pause
        | TouchControl::Inventory
        | TouchControl::Map
        | TouchControl::Back
        | TouchControl::TabPrevious
        | TouchControl::TabNext
        | TouchControl::BuildRemove
        | TouchControl::BuildRotate
        | TouchControl::BuildPrevious
        | TouchControl::BuildNext
        | TouchControl::BuildExit => palette::MIST,
    };
    with_alpha(color, 210)
}

/// The word on a button.
const fn label(control: TouchControl) -> &'static str {
    match control {
        TouchControl::Fire => "FIRE",
        TouchControl::CycleWeapon => "GUN",
        TouchControl::Bomb => "BOMB",
        TouchControl::CycleExplosive => "NEXT",
        TouchControl::Detonate => "DET",
        TouchControl::Scan => "SCAN",
        TouchControl::Tools => "KIT",
        TouchControl::Kit(0) => "RELAY",
        TouchControl::Kit(1) => "DRONE",
        TouchControl::Kit(2) => "LIFT",
        TouchControl::Kit(3) => "PROP",
        TouchControl::Kit(4) => "PUMP",
        TouchControl::Kit(_) => "ORE",
        TouchControl::Build => "BUILD",
        TouchControl::Thrust => "FLY",
        TouchControl::Jump => "JUMP",
        TouchControl::Roll => "ROLL",
        TouchControl::ExitRig => "OUT",
        TouchControl::BoardRig => "IN",
        TouchControl::Rope => "ROPE",
        TouchControl::Prompt => "",
        TouchControl::Use => "USE",
        TouchControl::Pause => "II",
        TouchControl::Inventory => "BAG",
        TouchControl::Map => "MAP",
        TouchControl::Back => "BACK",
        TouchControl::TabPrevious | TouchControl::BuildPrevious => "<",
        TouchControl::TabNext | TouchControl::BuildNext => ">",
        TouchControl::BuildPlace => "PLACE",
        TouchControl::BuildRemove => "DEL",
        TouchControl::BuildRotate => "TURN",
        TouchControl::BuildExit => "DONE",
    }
}

const fn with_alpha(color: Color, alpha: u8) -> Color {
    Color {
        r: color.r,
        g: color.g,
        b: color.b,
        a: alpha,
    }
}

/// Half-width of a pixel disc of `radius` at row `dy` (`None` outside it).
fn half_width(radius: i32, dy: i32) -> Option<i32> {
    (dy.abs() <= radius).then(|| {
        let reach = (radius * radius - dy * dy) as f32;
        (reach.sqrt() + 0.35) as i32
    })
}

/// A pixel disc: `fill` inside a one-pixel `edge`.
fn disc(draw: &mut impl RaylibDraw, x: i32, y: i32, radius: i32, fill: Color, edge: Color) {
    for dy in -radius..=radius {
        let Some(outer) = half_width(radius, dy) else {
            continue;
        };
        match half_width(radius - 1, dy) {
            Some(inner) if inner < outer => {
                draw.draw_rectangle(x - inner, y + dy, inner * 2 + 1, 1, fill);
                draw.draw_rectangle(x - outer, y + dy, outer - inner, 1, edge);
                draw.draw_rectangle(x + inner + 1, y + dy, outer - inner, 1, edge);
            }
            Some(inner) => {
                // Steep sides: the edge is the outermost pixel of the row.
                draw.draw_rectangle(x - inner + 1, y + dy, inner * 2 - 1, 1, fill);
                draw.draw_rectangle(x - inner, y + dy, 1, 1, edge);
                draw.draw_rectangle(x + inner, y + dy, 1, 1, edge);
            }
            None => draw.draw_rectangle(x - outer, y + dy, outer * 2 + 1, 1, edge),
        }
    }
}

/// A one-pixel pixel ring.
fn ring(draw: &mut impl RaylibDraw, x: i32, y: i32, radius: i32, color: Color) {
    disc(draw, x, y, radius, Color::BLANK, color);
}
