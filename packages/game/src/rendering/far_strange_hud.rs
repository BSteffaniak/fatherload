//! The far Strange Layers on screen (Wave 9 strange-b): one small panel per seat with what the
//! stratum's rule is doing to its rig right now.
//!
//! * Time Sludge: the time the rig is living at (slow pool, fast lane), amber, time bubbles
//!   aboard;
//! * Echo Caves: how loud the rig is to the blind hunters, and the ping key's reminder;
//! * Prism Desert: the lamp filter fitted, in its colour;
//! * The Rewrite Hall: The Glitch's rule now, and the rule being written while it rewrites.

use fatherload_simulation::{
    ExplosiveKind,
    descent::Stratum,
    far_strange::{LampFilter, rig_noise_tiles},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// One line of the panel.
struct Line {
    text: String,
    color: Color,
}

const fn filter_color(filter: LampFilter) -> Color {
    match filter {
        LampFilter::White => CREAM,
        LampFilter::Red => RED,
        LampFilter::Green => LIME,
        LampFilter::Blue => SKY,
    }
}

/// Draws the seat's far Strange Layers panel at the right of its view, from `top` down.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "a few lines of text; one block per rule shown"
)]
pub(super) fn draw_far_strange_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    let terrain = snapshot.terrain;
    if !terrain.settings().generator.has_descents() || view.pilot.is_some() {
        return;
    }
    let frame = crate::view_frame::frame();
    let rig_tile = frame.tile_at(view.player.x, view.player.y);
    let stratum = terrain.stratum_at(rig_tile);
    let far = snapshot.far_strange;
    let mut title: Option<(&str, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    // The Rewrite Hall comes first: the fight's rule is what matters there.
    if let Some(arena) = snapshot.glitch_arena
        && arena.contains(rig_tile)
        && far.glitch.awake()
    {
        let rule = far.glitch.rule;
        title = Some(("THE GLITCH", super::world::far_rule_color(rule)));
        lines.push(Line {
            text: format!("Rule: {}", rule.label()),
            color: super::world::far_rule_color(rule),
        });
        if let Some((next, until)) = far.glitch.rewriting {
            let left = until.get().saturating_sub(snapshot.tick) as f32
                / snapshot.tick_rate.hz().max(1) as f32;
            lines.push(Line {
                text: format!("REWRITING -> {} ({left:.1} s)", next.label()),
                color: super::world::far_rule_color(next),
            });
        }
    }
    match stratum {
        Stratum::TimeSludge if title.is_none() => {
            let scale = view.fields.time_scale;
            let (text, color) = if far.is_ambered(
                view.player_id,
                fatherload_simulation::SimulationTick::new(snapshot.tick),
            ) {
                ("HELD IN AMBER".to_owned(), AMBER)
            } else if scale < 0.8 {
                (format!("Time x{scale:.2}: a slow pool"), GOLD)
            } else if scale > 1.15 {
                (format!("Time x{scale:.2}: a fast lane"), SKY_LIGHT)
            } else {
                (format!("Time x{scale:.2}"), SAND)
            };
            title = Some(("TIME SLUDGE", AMBER));
            lines.push(Line { text, color });
            lines.push(Line {
                text: format!(
                    "Time bubbles aboard: {}",
                    view.player.explosive_count(ExplosiveKind::TimeBubble)
                ),
                color: CREAM,
            });
        }
        Stratum::EchoCaves if title.is_none() => {
            let speed = view.player.velocity_x.hypot(view.player.velocity_y);
            let noise = rig_noise_tiles(
                speed,
                view.player.velocity_y < -30.0 || speed > 150.0,
                view.active_drill.is_some(),
                false,
            );
            let (text, color) = if noise <= 0.0 {
                ("Silent: the hunters cannot hear you".to_owned(), MINT)
            } else if noise < 10.0 {
                (format!("Heard {noise:.0} tiles off"), GOLD)
            } else {
                (format!("LOUD: heard {noise:.0} tiles off"), RED)
            };
            title = Some(("ECHO CAVES", SKY_LIGHT));
            lines.push(Line { text, color });
            lines.push(Line {
                text: "Scan to ping: the cave shows for a moment".to_owned(),
                color: FOG,
            });
        }
        Stratum::PrismDesert if title.is_none() => {
            let filter = far.filter_of(view.player_id);
            title = Some(("PRISM DESERT", filter_color(filter)));
            lines.push(Line {
                text: format!("Lamp filter: {}", filter.name()),
                color: filter_color(filter),
            });
            lines.push(Line {
                text: "Walls open to light of their colour".to_owned(),
                color: FOG,
            });
        }
        _ => {}
    }
    let Some((heading, heading_color)) = title else {
        return;
    };
    let width = 220;
    let height = 18 + lines.len() as i32 * 11;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    let inner = x + ui_kit::PANEL_PADDING;
    ui_kit::text(
        draw,
        heading,
        inner,
        top + 5,
        TextStyle::Bold,
        heading_color,
    );
    let mut y = top + 17;
    for line in lines {
        ui_kit::text(draw, &line.text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}
