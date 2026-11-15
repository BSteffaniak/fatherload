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

/// The clock badge: a little half-disc sun dial (the sun travels its arc by day, the Moon by
/// night) beside the day count and the time. Right-aligned under the wallet panel at `top`.
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
) {
    let clock = snapshot.clock;
    let day = format!("DAY {}", clock.day());
    let (hours, minutes) = clock.hours_minutes();
    let time = format!("{hours:02}:{minutes:02}");
    let text_width =
        ui_kit::text_width(&day, TextStyle::Bold).max(ui_kit::text_width(&time, TextStyle::Body));
    let width = ui_kit::PANEL_PADDING * 2 + DIAL_WIDTH + 5 + text_width;
    let height = 26;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if top + height > (rect.y + rect.height) as i32 - 8 {
        return;
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
