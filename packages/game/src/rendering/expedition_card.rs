//! The expedition's touches on the story moment card (Wave 8 story): a brass canister for a
//! log, a needle of static for a radio echo, turning tape reels and a voice trace for a ghost
//! recording, and a slow prism edge for an anomaly. Everything moves slowly (no flashing).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "card pixels are small integers"
)]

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use crate::shell::StoryMomentKind;

/// A deterministic wiggle in `-1..=1` for sample `index` at card age `age`.
fn trace(index: i32, age: u64, speed: f32) -> f32 {
    let t = age as f32 * speed + index as f32 * 0.37;
    (t.sin() * 0.6 + (t * 2.3 + 1.1).sin() * 0.3 + (t * 5.1).sin() * 0.1).clamp(-1.0, 1.0)
}

/// Draws the kind's decoration over the card's panel at `(x, y, width, height)`.
pub(super) fn decorate(
    draw: &mut RaylibDrawHandle<'_>,
    kind: StoryMomentKind,
    (x, y, width, height): (i32, i32, i32, i32),
    age: u64,
    color: Color,
) {
    match kind {
        StoryMomentKind::ExpeditionLog => {
            // The opened cipher canister on the card's left corner, its lid beside it.
            let (cx, cy) = (x + 8, y + 5);
            draw.draw_rectangle(cx, cy, 7, 11, INK);
            draw.draw_rectangle(cx + 1, cy + 1, 5, 9, GOLD);
            draw.draw_rectangle(cx + 1, cy + 4, 5, 1, AMBER);
            draw.draw_rectangle(cx + 1, cy + 7, 5, 1, AMBER);
            draw.draw_rectangle(cx + 2, cy + 2, 1, 1, CREAM);
            draw.draw_rectangle(cx + 8, cy + 8, 5, 2, COPPER);
        }
        StoryMomentKind::RadioEcho => {
            // A strip of static across the card's foot, the signal breaking through it.
            let base = y + height - 5;
            for index in 0..(width - 24) / 2 {
                let level = trace(index, age, 0.21) * 3.0;
                let px = x + 12 + index * 2;
                draw.draw_rectangle(
                    px,
                    base - level.abs() as i32,
                    1,
                    1 + 2 * level.abs() as i32,
                    with_alpha(color, 120),
                );
            }
            // An antenna in the corner, its tip blinking slowly.
            draw.draw_rectangle(x + 10, y + 4, 1, 9, STEEL_LIGHT);
            draw.draw_rectangle(x + 7, y + 12, 7, 2, STEEL);
            if (age / 30).is_multiple_of(2) {
                draw.draw_rectangle(x + 9, y + 3, 3, 2, RED);
            }
        }
        StoryMomentKind::GhostRecording => {
            // Two tape reels turning slowly and the voice trace between them.
            for reel in 0..2 {
                let rx = x + 10 + reel * 14;
                let ry = y + 9;
                draw.draw_circle(rx, ry, 5.0, INK);
                draw.draw_circle(rx, ry, 4.0, SLATE);
                let spoke = (age as f32 * 0.05 + reel as f32).rem_euclid(std::f32::consts::TAU);
                draw.draw_line(
                    rx,
                    ry,
                    rx + (spoke.cos() * 3.0) as i32,
                    ry + (spoke.sin() * 3.0) as i32,
                    MIST,
                );
                draw.draw_circle(rx, ry, 1.0, BONE);
            }
            draw.draw_rectangle(x + 10, y + 14, 14, 1, SOIL);
            let base = y + height - 6;
            for index in 0..(width - 24) {
                let voice = trace(index, age, 0.12) * (trace(index / 9, age, 0.05).abs() * 4.0);
                draw.draw_rectangle(
                    x + 12 + index,
                    base + voice as i32,
                    1,
                    1,
                    with_alpha(MIST, 150),
                );
            }
        }
        StoryMomentKind::Anomaly => {
            // A prism edge creeping slowly round the frame.
            let colours = [VIOLET, LAVENDER, SKY_LIGHT, MINT];
            let perimeter = 2 * (width + height);
            let head = ((age as i32) * 2).rem_euclid(perimeter.max(1));
            for step in 0..40 {
                let at = (head - step * 2).rem_euclid(perimeter.max(1));
                let (px, py) = if at < width {
                    (x + at, y)
                } else if at < width + height {
                    (x + width - 1, y + at - width)
                } else if at < 2 * width + height {
                    (x + width - 1 - (at - width - height), y + height - 1)
                } else {
                    (x, y + height - 1 - (at - 2 * width - height))
                };
                let tint = colours[(step / 10) as usize % colours.len()];
                draw.draw_rectangle(px, py, 2, 2, with_alpha(tint, 200 - step as u8 * 4));
            }
        }
        StoryMomentKind::Begun
        | StoryMomentKind::Climax
        | StoryMomentKind::Ended
        | StoryMomentKind::Lost
        | StoryMomentKind::Remembered
        | StoryMomentKind::DeepLore => {}
    }
}
