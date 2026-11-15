//! The village arrival banner: a ribbon with the village's name, how it is built and what it
//! is known for, sliding in when a rig drives into a village ("NEW VILLAGE DISCOVERED" on the
//! first visit, "WELCOME BACK TO" afterwards).

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::shell::{BANNER_TICKS, VillageBanner};

/// Draws `banner` (if it is still showing at `now`) near the top of the current UI canvas.
pub(super) fn draw_village_banner(
    draw: &mut RaylibDrawHandle<'_>,
    banner: &VillageBanner,
    now: u64,
) {
    if !banner.showing(now) {
        return;
    }
    let age = now.saturating_sub(banner.tick);
    let canvas = ui_kit::canvas();
    let width = (canvas.width - 32).min(340);
    let (heading, color) = if banner.first_visit {
        ("NEW VILLAGE DISCOVERED", GOLD)
    } else {
        ("WELCOME BACK TO", MIST)
    };
    let heading_y = 5;
    let title_y = heading_y + TextStyle::Body.line_height() + 1;
    let subtitle_y = title_y + TextStyle::Title.line_height() + 2;
    let height = subtitle_y + TextStyle::Body.line_height() + 5;
    // Slides down, holds, then lifts away.
    let enter = i32::try_from(14_u64.saturating_sub(age)).unwrap_or(0) * 4;
    let leave = i32::try_from(age.saturating_sub(BANNER_TICKS - 14)).unwrap_or(0) * 5;
    let x = (canvas.width - width) / 2;
    let y = 72 - enter - leave;
    // Ribbon tails behind the panel.
    for (tail_x, notch_x) in [(x - 12, x - 12), (x + width - 4, x + width + 7)] {
        draw.draw_rectangle(tail_x, y + 8, 16, height - 10, INK);
        draw.draw_rectangle(tail_x + 1, y + 9, 14, height - 12, MAROON);
        draw.draw_rectangle(notch_x, y + 8 + (height - 10) / 2 - 2, 5, 4, INK);
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        if banner.first_visit {
            PanelStyle::Accent
        } else {
            PanelStyle::Standard
        },
    );
    if banner.first_visit {
        let twinkle = (age / 6).is_multiple_of(2);
        for (sx, sy) in [(x + 6, y + 6), (x + width - 9, y + 6)] {
            let spark = if twinkle { CREAM } else { GOLD };
            draw.draw_rectangle(sx, sy + 1, 3, 1, spark);
            draw.draw_rectangle(sx + 1, sy, 1, 3, spark);
        }
    }
    let centre = x + width / 2;
    ui_kit::text_centred(draw, heading, centre, y + heading_y, TextStyle::Body, color);
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(&banner.title, width - 20, TextStyle::Title),
        centre,
        y + title_y,
        TextStyle::Title,
        CREAM,
    );
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(&banner.subtitle, width - 16, TextStyle::Body),
        centre,
        y + subtitle_y,
        TextStyle::Body,
        SKY_LIGHT,
    );
}
