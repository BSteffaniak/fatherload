//! Speed lines and wake motes behind fast rigs (WP-FEEL presentation).
//!
//! Nothing shows below about six tiles a second (every stock-to-mid-tier rig). Above that, pale
//! streaks of art pixels stream back from the rig along its direction of travel and a few
//! bright motes trail in its wake; both grow in number, length and opacity with speed, reaching
//! full strength at god-mode speed. Like the other stateless particles, every streak is derived
//! from the tick clock and the rig's id, so the effect needs no state, looks the same in every
//! view and costs a few dozen rectangles per fast rig.

use raylib::prelude::*;

use fatherload_simulation::TILE_SIZE;

use crate::render_snapshot::PlayerPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, snap};

/// Speed (px/s) at which speed lines start to show.
pub const SPEED_LINES_FROM: f32 = 6.0 * TILE_SIZE;
/// Speed (px/s) at which speed lines reach full strength.
pub const SPEED_LINES_FULL: f32 = 22.0 * TILE_SIZE;
/// Most streaks behind one rig (at full strength).
const MAX_STREAKS: u32 = 18;
/// Most wake motes behind one rig (at full strength).
const MAX_MOTES: u32 = 10;

/// How strongly a rig moving at `(velocity_x, velocity_y)` shows speed lines (`0..=1`).
#[must_use]
pub fn speed_line_strength(velocity_x: f32, velocity_y: f32) -> f32 {
    let speed = velocity_x.hypot(velocity_y);
    ((speed - SPEED_LINES_FROM) / (SPEED_LINES_FULL - SPEED_LINES_FROM)).clamp(0.0, 1.0)
}

/// A small deterministic hash of `(seed, index)` in `0..1`.
fn unit_hash(seed: u64, index: u32) -> f32 {
    let mut value = seed ^ (u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    #[allow(clippy::cast_precision_loss, reason = "a 24-bit fraction")]
    let fraction = (value >> 40) as f32 / (1u64 << 24) as f32;
    fraction
}

/// Draws `player`'s speed lines and wake motes (nothing for a slow rig).
pub fn draw_speed_lines(draw: &mut impl RaylibDraw, player: &PlayerPresentation, ticks: u64) {
    let strength = speed_line_strength(player.velocity_x, player.velocity_y);
    if strength <= 0.0 {
        return;
    }
    let speed = player.velocity_x.hypot(player.velocity_y);
    // Direction of travel and its perpendicular.
    let (dir_x, dir_y) = (player.velocity_x / speed, player.velocity_y / speed);
    let (perp_x, perp_y) = (-dir_y, dir_x);
    #[allow(clippy::cast_precision_loss, reason = "the tick clock as seconds")]
    let seconds = ticks as f32 / 60.0;
    let seed = player.player_id.get().wrapping_mul(0x2545_f491_4f6c_dd1d);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a handful of streaks"
    )]
    let streaks = (strength * MAX_STREAKS as f32).ceil() as u32;
    let length = strength.mul_add(40.0, 10.0);
    let reach = strength.mul_add(70.0, 40.0);
    for index in 0..streaks {
        let lateral = (unit_hash(seed, index) - 0.5) * 44.0;
        let rate = unit_hash(seed, index + 100).mul_add(1.5, 2.0);
        // Each streak spawns beside the rig and streams back over its life.
        let age = unit_hash(seed, index + 200)
            .mul_add(1.0, seconds * rate)
            .fract();
        let back = age.mul_add(reach, 6.0);
        let alpha = strength.sqrt() * (1.0 - age) * 230.0;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "alpha is clamped to a byte"
        )]
        let color = with_alpha(
            if index % 3 == 0 { BONE } else { MIST },
            alpha.clamp(0.0, 255.0) as u8,
        );
        let (start_x, start_y) = (
            perp_x.mul_add(lateral, dir_x.mul_add(-back, player.x)),
            perp_y.mul_add(lateral, dir_y.mul_add(-back, player.y)),
        );
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a streak is a few dozen art pixels"
        )]
        let pixels = (length / ART).ceil() as u32;
        for pixel in 0..pixels {
            #[allow(clippy::cast_precision_loss, reason = "a few dozen art pixels")]
            let along = pixel as f32 * ART;
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(dir_x.mul_add(-along, start_x)),
                    snap(dir_y.mul_add(-along, start_y)),
                    ART,
                    ART,
                ),
                color,
            );
        }
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a handful of motes"
    )]
    let motes = (strength * MAX_MOTES as f32).ceil() as u32;
    for index in 0..motes {
        let age = unit_hash(seed, index + 300)
            .mul_add(1.0, seconds * 3.0)
            .fract();
        let lateral = (unit_hash(seed, index + 400) - 0.5) * 18.0;
        let back = age.mul_add(48.0, 12.0);
        let color = if age < 0.35 {
            CREAM
        } else if age < 0.7 {
            SKY_LIGHT
        } else {
            FOG
        };
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(perp_x.mul_add(lateral, dir_x.mul_add(-back, player.x))),
                snap(perp_y.mul_add(lateral, dir_y.mul_add(-back, player.y))),
                ART,
                ART,
            ),
            with_alpha(color, 220),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_lines_only_show_on_fast_rigs_and_peak_at_god_speed() {
        assert!(speed_line_strength(0.0, 0.0) <= 0.0);
        assert!(speed_line_strength(SPEED_LINES_FROM, 0.0) <= 0.0);
        let max_tier = speed_line_strength(11.4 * TILE_SIZE, 0.0);
        assert!(max_tier > 0.2 && max_tier < 0.5, "{max_tier}");
        assert!((speed_line_strength(25.0 * TILE_SIZE, 0.0) - 1.0).abs() < f32::EPSILON);
        assert!(
            speed_line_strength(0.0, -400.0) > 0.0,
            "climbing counts too"
        );
    }

    #[test]
    fn hashes_spread_over_the_unit_interval() {
        let values: Vec<f32> = (0..64).map(|index| unit_hash(7, index)).collect();
        assert!(values.iter().all(|value| (0.0..1.0).contains(value)));
        assert!(values.iter().any(|value| *value < 0.25));
        assert!(values.iter().any(|value| *value > 0.75));
    }
}
