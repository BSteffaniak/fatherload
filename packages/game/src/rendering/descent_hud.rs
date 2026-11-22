//! Deep Descent HUD (generator V11): the "entering <stratum>" banner and the stratum label
//! beside the depth ruler's marker.

use fatherload_simulation::{
    terrain::Terrain,
    worldgen::strata::SKY_ROWS,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    terrain::tile_art::stratum_rock_color,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    render_snapshot::ViewSnapshot,
    shell::{STRATUM_BANNER_TICKS, StratumBanner},
};

/// Draws `banner` (if it is still showing at `now`) near the top of the current UI canvas: a
/// slim ribbon in the stratum's rock colour with its name and its Descent.
pub(super) fn draw_stratum_banner(
    draw: &mut RaylibDrawHandle<'_>,
    banner: &StratumBanner,
    now: u64,
) {
    if !banner.showing(now) {
        return;
    }
    let age = now.saturating_sub(banner.tick);
    let canvas = ui_kit::canvas();
    let width = (canvas.width - 32).min(300);
    let heading_y = 5;
    let title_y = heading_y + TextStyle::Body.line_height() + 1;
    let subtitle_y = title_y + TextStyle::Title.line_height() + 2;
    let height = subtitle_y + TextStyle::Body.line_height() + 5;
    let enter = i32::try_from(12_u64.saturating_sub(age)).unwrap_or(0) * 4;
    let leave = i32::try_from(age.saturating_sub(STRATUM_BANNER_TICKS - 12)).unwrap_or(0) * 5;
    let x = (canvas.width - width) / 2;
    let y = 64 - enter - leave;
    let rock = stratum_rock_color(banner.stratum);
    // Rock-coloured tails behind the panel.
    for tail_x in [x - 10, x + width - 6] {
        draw.draw_rectangle(tail_x, y + 7, 16, height - 10, INK);
        draw.draw_rectangle(tail_x + 1, y + 8, 14, height - 12, rock);
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        if banner.new_descent {
            PanelStyle::Accent
        } else {
            PanelStyle::Standard
        },
    );
    let centre = x + width / 2;
    let heading_color = if banner.new_descent { GOLD } else { FOG };
    ui_kit::text_centred(
        draw,
        &banner.heading(),
        centre,
        y + heading_y,
        TextStyle::Body,
        heading_color,
    );
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(banner.stratum.name(), width - 20, TextStyle::Title),
        centre,
        y + title_y,
        TextStyle::Title,
        CREAM,
    );
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(banner.subtitle(), width - 16, TextStyle::Body),
        centre,
        y + subtitle_y,
        TextStyle::Body,
        SKY_LIGHT,
    );
}

/// Names the Descent and stratum beside the depth ruler's marker (`x` is the ruler's left
/// edge, `marker_y` the marker's centre row), for a rig underground in a Deep Descent world.
pub(super) fn draw_ruler_label(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &Terrain,
    view: &ViewSnapshot<'_>,
    (x, marker_y): (i32, i32),
) {
    if !terrain.settings().generator.has_descents() || view.hud.altitude.is_some() {
        return;
    }
    let tile = view.player.tile_in(crate::view_frame::frame());
    if tile.y <= SKY_ROWS {
        return;
    }
    let stratum = terrain.stratum_at(tile);
    let label = format!("{}  {}", stratum.descent().numeral(), stratum.name());
    let width = ui_kit::text_width(&label, TextStyle::Body) + 8;
    let height = TextStyle::Body.line_height() + 4;
    let left = x - 8 - width;
    let top = marker_y - height / 2;
    ui_kit::panel(
        draw,
        Rectangle::new(left as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    draw.draw_rectangle(
        left + 2,
        top + 2,
        2,
        height - 4,
        stratum_rock_color(stratum),
    );
    ui_kit::text(draw, &label, left + 6, top + 2, TextStyle::Body, BONE);
}
