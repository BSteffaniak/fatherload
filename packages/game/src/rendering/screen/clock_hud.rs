//! The world clock on screen (Part II F4): each seat's sun dial and day count under the wallet
//! panel, the "in bed" banner and the vote it waits on, the screens fading out as the crew falls
//! asleep and back in on the new morning.

use raylib::prelude::*;

use fatherload_simulation::clock::{DayPhase, SLEEP_FADE_MILLIS, WorldClock};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::super::{
    pixel_art::sprites::icons,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// The dial's size in UI art pixels.
const DIAL_WIDTH: i32 = 27;
const DIAL_HEIGHT: i32 = 14;
/// Seconds the screens take to fade back in after a skipped night (the "Day N" card shows as
/// long).
const WAKE_SECONDS: f32 = 2.4;

/// The sky colour of the dial's face for a phase.
const fn dial_sky(phase: DayPhase) -> Color {
    match phase {
        DayPhase::Dawn => ROSE,
        DayPhase::Day => SKY,
        DayPhase::Dusk => VIOLET,
        DayPhase::Night => DEEP_BLUE,
    }
}

/// Where the clock badge goes: right-aligned under the wallet panel at `top` in view `rect`,
/// stepped left of any of `avoid` (the phone's thumb cluster: on a landscape phone its upper
/// buttons rise level with the badge) it would cover.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "UI art pixels"
)]
pub(in crate::rendering) fn badge_rect(
    rect: Rectangle,
    top: i32,
    (width, height): (i32, i32),
    avoid: &[Rectangle],
) -> Rectangle {
    let mut x = rect.x + rect.width - 16.0 - width as f32;
    for _ in 0..avoid.len() {
        let badge = Rectangle::new(x, top as f32, width as f32, height as f32);
        let Some(left) = avoid
            .iter()
            .filter(|control| overlaps(control, &badge))
            .map(|control| control.x)
            .reduce(f32::min)
        else {
            break;
        };
        x = (left - 4.0 - width as f32).max(rect.x + 4.0);
    }
    Rectangle::new(x.round(), top as f32, width as f32, height as f32)
}

/// Whether two rectangles share any area.
fn overlaps(a: &Rectangle, b: &Rectangle) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// The clock badge: a little half-disc sun dial (the sun travels its arc by day, the Moon by
/// night) beside the day count and the time. Right-aligned under the wallet panel at `top`
/// ([`badge_rect`]: left of the thumb cluster's buttons in `avoid`). While night monsters are
/// about (`night_safe` is `Some`, WP-NIGHT) a line under it says whether the seat is SAFE (in a
/// town, camp, home, lamp light or behind palisades) or EXPOSED. Returns where it was drawn.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "UI art pixels"
)]
pub(super) fn draw_clock_badge(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    rect: Rectangle,
    top: i32,
    night_safe: Option<bool>,
    avoid: &[Rectangle],
) -> Option<Rectangle> {
    let clock = snapshot.clock;
    let day = format!("DAY {}", clock.day());
    let (hours, minutes) = clock.hours_minutes();
    let time = format!("{hours:02}:{minutes:02}");
    let label_width = if night_safe.is_some() {
        ui_kit::text_width("EXPOSED", TextStyle::Body)
    } else {
        0
    };
    let text_width = ui_kit::text_width(&day, TextStyle::Bold)
        .max(ui_kit::text_width(&time, TextStyle::Body))
        .max(label_width);
    let width = ui_kit::PANEL_PADDING * 2 + DIAL_WIDTH + 5 + text_width;
    let height = if night_safe.is_some() { 36 } else { 26 };
    let x = badge_rect(rect, top, (width, height), avoid).x as i32;
    if top + height > (rect.y + rect.height) as i32 - 8 {
        return None;
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    let dial_x = x + ui_kit::PANEL_PADDING;
    let dial_bottom = top + 5 + DIAL_HEIGHT;
    draw_dial(draw, clock, dial_x, dial_bottom);
    let text_x = dial_x + DIAL_WIDTH + 5;
    ui_kit::text(draw, &day, text_x, top + 5, TextStyle::Bold, GOLD);
    let time_color = if clock.is_night() { LAVENDER } else { MIST };
    ui_kit::text(draw, &time, text_x, top + 15, TextStyle::Body, time_color);
    if let Some(safe) = night_safe {
        let (label, color) = if safe {
            ("SAFE", MINT)
        } else {
            ("EXPOSED", ORANGE)
        };
        let row = top + 25;
        // A little lamp (safe) or a pair of eyes in the dark (exposed).
        let icon_x = dial_x + 8;
        if safe {
            draw.draw_rectangle(icon_x + 2, row + 1, 1, 2, INK);
            draw.draw_rectangle(icon_x, row + 3, 5, 4, INK);
            draw.draw_rectangle(icon_x + 1, row + 4, 3, 2, GOLD);
        } else {
            draw.draw_rectangle(icon_x - 1, row + 3, 3, 2, ORANGE);
            draw.draw_rectangle(icon_x + 5, row + 3, 3, 2, ORANGE);
        }
        ui_kit::text(draw, label, text_x, row, TextStyle::Body, color);
    }
    Some(Rectangle::new(
        x as f32,
        top as f32,
        width as f32,
        height as f32,
    ))
}

/// The dial: a half disc of sky over a horizon line, with the sun (day) or the Moon (night)
/// on its arc.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "UI art pixels"
)]
fn draw_dial(draw: &mut RaylibDrawHandle<'_>, clock: WorldClock, x: i32, bottom: i32) {
    let radius = DIAL_HEIGHT - 1;
    let cx = x + DIAL_WIDTH / 2;
    let sky = dial_sky(clock.phase());
    for row in 0..=radius {
        let half = (((radius * radius - row * row) as f32).sqrt()).round() as i32;
        let y = bottom - row;
        draw.draw_rectangle(cx - half - 1, y, half * 2 + 3, 1, INK);
        if row < radius {
            draw.draw_rectangle(cx - half, y, half * 2 + 1, 1, sky);
        }
    }
    // Ground under the horizon.
    draw.draw_rectangle(cx - radius - 1, bottom + 1, radius * 2 + 3, 2, INK);
    draw.draw_rectangle(cx - radius, bottom + 1, radius * 2 + 1, 1, MOSS);
    // The sun by day; the Moon walks the same arc through the night.
    let angle = clock.sun_angle();
    let (body_angle, body, glow) = if angle <= std::f32::consts::PI {
        (angle, GOLD, CREAM)
    } else {
        (angle - std::f32::consts::PI, BONE, MIST)
    };
    let reach = (radius - 3) as f32;
    let bx = cx as f32 - reach * body_angle.cos();
    let by = bottom as f32 - 1.0 - reach * body_angle.sin();
    let (bx, by) = (bx.round() as i32, by.round() as i32);
    draw.draw_rectangle(bx - 2, by - 1, 5, 3, INK);
    draw.draw_rectangle(bx - 1, by - 2, 3, 5, INK);
    draw.draw_rectangle(bx - 1, by - 1, 3, 3, body);
    draw.draw_pixel(bx, by, glow);
}

/// The sleep layer over one view: the "in bed" banner (and who the vote waits for), the fade to
/// dark while everyone is drifting off, and the morning fading back in with the new day's card.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI art pixels and alpha"
)]
pub(super) fn draw_sleep_overlay(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let fade_seconds = SLEEP_FADE_MILLIS as f32 / 1_000.0;
    if let Some(left) = snapshot.sleep_countdown {
        let dark = (1.0 - left / fade_seconds).clamp(0.0, 1.0);
        draw.draw_rectangle_rec(rect, with_alpha(INK, (dark * 235.0) as u8));
    }
    let woke = snapshot
        .since_night_skipped
        .filter(|seconds| *seconds < WAKE_SECONDS);
    if let Some(seconds) = woke {
        let dark = (1.0 - seconds / WAKE_SECONDS).clamp(0.0, 1.0);
        draw.draw_rectangle_rec(rect, with_alpha(INK, (dark * dark * 235.0) as u8));
        let title = format!("DAY {}", snapshot.clock.day());
        let cx = (rect.x + rect.width * 0.5) as i32;
        let y = (rect.y + rect.height * 0.32) as i32;
        let alpha = ((1.0 - seconds / WAKE_SECONDS) * 2.0).clamp(0.0, 1.0);
        ui_kit::text_centred(
            draw,
            &title,
            cx,
            y,
            TextStyle::Title,
            with_alpha(GOLD, (alpha * 255.0) as u8),
        );
        ui_kit::text_centred(
            draw,
            "Good morning!",
            cx,
            y + 22,
            TextStyle::Bold,
            with_alpha(CREAM, (alpha * 255.0) as u8),
        );
        return;
    }
    if !snapshot.clock_state.sleepers.contains_key(&view.player_id) {
        return;
    }
    let awake = snapshot
        .players
        .iter()
        .filter(|player| {
            !snapshot
                .clock_state
                .sleepers
                .contains_key(&player.player_id)
        })
        .count();
    let line = if snapshot.sleep_countdown.is_some() {
        "Drifting off...".to_owned()
    } else if awake == 1 {
        "Waiting for 1 crewmate to turn in".to_owned()
    } else {
        format!("Waiting for {awake} crewmates to turn in")
    };
    let title = "ZZZ... IN BED";
    let hint = "Drive off to get up";
    let width = ui_kit::text_width(title, TextStyle::Bold)
        .max(ui_kit::text_width(&line, TextStyle::Body))
        .max(ui_kit::text_width(hint, TextStyle::Body))
        + 30;
    let height = 42;
    let x = (rect.x + (rect.width - width as f32) * 0.5) as i32;
    let y = (rect.y + rect.height * 0.22) as i32;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    ui_kit::icon(draw, &icons::MOON, x + 7, y + 6);
    ui_kit::text(draw, title, x + 19, y + 7, TextStyle::Bold, LAVENDER);
    ui_kit::text(draw, &line, x + 7, y + 19, TextStyle::Body, BONE);
    ui_kit::text(draw, hint, x + 7, y + 29, TextStyle::Body, FOG);
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    #[test]
    fn the_clock_badge_steps_left_of_the_thumb_cluster() {
        // A landscape phone view (844 x 390 art pixels): the badge under the wallet, and the
        // cluster's upper buttons (KIT and SCAN, GUN) rising level with it.
        let view = Rectangle::new(0.0, 0.0, 844.0, 390.0);
        let size = (150, 26);
        let alone = badge_rect(view, 132, size, &[]);
        assert!((alone.x - (844.0 - 16.0 - 150.0)).abs() < f32::EPSILON);
        let cluster = [
            Rectangle::new(690.0, 154.0, 46.0, 46.0),
            Rectangle::new(775.0, 140.0, 46.0, 46.0),
            Rectangle::new(760.0, 210.0, 60.0, 50.0),
        ];
        let placed = badge_rect(view, 132, size, &cluster);
        assert!(
            cluster.iter().all(|button| !overlaps(button, &placed)),
            "{placed:?}"
        );
        assert!(placed.x >= view.x && placed.x + placed.width <= view.x + view.width);
        // Buttons below the badge (a portrait phone's cluster) leave it where it was.
        let low = [Rectangle::new(700.0, 300.0, 60.0, 60.0)];
        assert!((badge_rect(view, 132, size, &low).x - alone.x).abs() < f32::EPSILON);
    }
}
