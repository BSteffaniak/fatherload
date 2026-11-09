//! Quest markers drawn in the world: a bobbing diamond over targets, corner frames around marked
//! spots, the escorted haul, the resting relay beacon and hold-and-climb switches.

use raylib::prelude::*;

use drillgame_simulation::{
    quest::describe::{QuestMarker, QuestMarkerKind},
    terrain::TILE_SIZE,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::TILE_ART;

use super::Pen;

/// Draws every marker of the view's quests.
pub fn draw_quest_markers(draw: &mut impl RaylibDraw, markers: &[QuestMarker], seconds: f32) {
    let blink = (seconds * 3.0) as i32 % 2 == 0;
    let bob = ((seconds * 2.5).sin() * 2.0).round() as i32;
    for marker in markers {
        let pen = Pen::at(
            marker.position.x as f32 * TILE_SIZE,
            marker.position.y as f32 * TILE_SIZE,
        );
        match marker.kind {
            QuestMarkerKind::Target => draw_diamond(draw, pen, bob, GOLD),
            QuestMarkerKind::Spot { lit, next } => {
                let color = if lit {
                    LIME
                } else if next && blink {
                    CREAM
                } else if next {
                    GOLD
                } else {
                    ORANGE
                };
                draw_frame(draw, pen, color);
                if next {
                    draw_diamond(draw, pen, bob, GOLD);
                }
            }
            QuestMarkerKind::Haul => draw_haul(draw, pen, seconds),
            QuestMarkerKind::Beacon => {
                draw_frame(draw, pen, if blink { MINT } else { TEAL });
                pen.rect(draw, 5, 9, 6, 6, INK);
                pen.rect(draw, 6, 10, 4, 4, if blink { MINT } else { TEAL });
                pen.px(draw, 7, 11, BONE);
            }
            QuestMarkerKind::Switch { held } => {
                let color = if held { LIME } else { RED };
                draw_frame(draw, pen, color);
                pen.boxed(draw, 4, 9, 8, 6, STEEL_DARK);
                let lever = if held { 9 } else { 6 };
                pen.rect(draw, lever, 4, 2, 6, INK);
                pen.rect(draw, lever, 3, 2, 2, color);
            }
        }
    }
}

/// A diamond floating above the tile.
fn draw_diamond(draw: &mut impl RaylibDraw, pen: Pen, bob: i32, color: Color) {
    let centre = TILE_ART / 2;
    let top = -13 + bob;
    for row in 0..7 {
        let half = 3 - (row - 3_i32).abs();
        pen.rect(draw, centre - half - 1, top + row, half * 2 + 2, 1, INK);
    }
    for row in 1..6 {
        let half = 2 - (row - 3_i32).abs();
        if half >= 0 {
            pen.rect(draw, centre - half, top + row, half * 2 + 1, 1, color);
        }
    }
    pen.px(draw, centre, top + 2, CREAM);
    // A short tail pointing down at the tile.
    pen.rect(draw, centre, top + 8, 1, 2, color);
}

/// Corner brackets around the tile.
fn draw_frame(draw: &mut impl RaylibDraw, pen: Pen, color: Color) {
    let end = TILE_ART - 1;
    for (x, y, dx, dy) in [
        (0, 0, 1, 1),
        (end, 0, -1, 1),
        (0, end, 1, -1),
        (end, end, -1, -1),
    ] {
        pen.rect(draw, x.min(x + dx * 3), y, 4, 1, color);
        pen.rect(draw, x, y.min(y + dy * 3), 1, 4, color);
    }
}

/// The escorted haul: a small tracked sled with a beacon light.
fn draw_haul(draw: &mut impl RaylibDraw, pen: Pen, seconds: f32) {
    let light = if (seconds * 4.0) as i32 % 2 == 0 {
        AMBER
    } else {
        RUST
    };
    pen.boxed(draw, 1, 5, 14, 8, COPPER);
    pen.rect(draw, 2, 6, 12, 2, SAND);
    pen.rect(draw, 0, 12, 16, 3, INK);
    for wheel in [2, 7, 12] {
        pen.rect(draw, wheel, 13, 2, 1, STEEL_LIGHT);
    }
    pen.rect(draw, 7, 2, 2, 3, INK);
    pen.rect(draw, 7, 2, 2, 2, light);
}
