//! Caves on the minimap and the mine map, and the "Caves found" collection page (WP-CAVES).
//!
//! A cave a player has entered (their journal's `caves`) gets a marker in its kind's colour:
//! a dot on the minimap, its footprint's outline and name on the mine map.

use fatherload_simulation::{Player, cave::CaveKind, terrain::Terrain, worldgen::WorldFeature};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::super::{
    layout::{Section, SectionItem},
    terrain::tile_art::cave_kind_color,
    ui_kit::{self, TextStyle},
};

/// One cave the player has found.
#[derive(Clone, Copy, Debug)]
pub(super) struct CaveMarker {
    pub feature: WorldFeature,
    pub kind: CaveKind,
}

/// Every cave `player` has found, in id order.
pub(super) fn markers(terrain: &Terrain, player: &Player) -> Vec<CaveMarker> {
    player
        .journal
        .caves
        .iter()
        .filter_map(|(id, kind)| {
            terrain.feature(*id).map(|feature| CaveMarker {
                feature,
                kind: *kind,
            })
        })
        .collect()
}

/// Small diamonds on the minimap; `to_map` maps a tile to a minimap pixel, `None` when it is
/// outside the minimap's window.
pub(super) fn draw_minimap_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[CaveMarker],
    to_map: impl Fn(i32, i32) -> Option<(i32, i32)>,
) {
    for marker in markers {
        let center = marker.feature.center;
        let Some((x, y)) = to_map(center.x, center.y) else {
            continue;
        };
        draw.draw_rectangle(x - 1, y - 2, 3, 5, INK);
        draw.draw_rectangle(x - 2, y - 1, 5, 3, INK);
        draw.draw_rectangle(x, y - 1, 1, 3, cave_kind_color(marker.kind));
        draw.draw_rectangle(x - 1, y, 3, 1, cave_kind_color(marker.kind));
    }
}

/// Footprint outlines and names on the mine map, clipped to `clip`. `(x0, y0)` is the map's
/// top-left canvas pixel showing tile `origin`; each tile is `pixel` canvas pixels.
pub(super) fn draw_map_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[CaveMarker],
    clip: Rectangle,
    origin: (i32, i32),
    pixel: i32,
) {
    ui_kit::begin_clip(clip);
    let (x0, y0) = (clip.x as i32, clip.y as i32);
    for marker in markers {
        let bounds = marker.feature.bounds;
        let color = cave_kind_color(marker.kind);
        let x = x0 + (bounds.min.x - origin.0) * pixel;
        let y = y0 + (bounds.min.y - origin.1) * pixel;
        let (width, height) = (bounds.width() * pixel, bounds.height() * pixel);
        draw.draw_rectangle_lines(x - 1, y - 1, width + 2, height + 2, INK);
        // Dashed outline: a cave's footprint is rough, not a room.
        for step in (0..width).step_by(4) {
            draw.draw_rectangle(x + step, y, 2, 1, color);
            draw.draw_rectangle(x + step, y + height - 1, 2, 1, color);
        }
        for step in (0..height).step_by(4) {
            draw.draw_rectangle(x, y + step, 1, 2, color);
            draw.draw_rectangle(x + width - 1, y + step, 1, 2, color);
        }
        let label = marker.kind.name();
        let label_width = ui_kit::text_width(label, TextStyle::Body);
        let center = marker.feature.center;
        ui_kit::text(
            draw,
            label,
            x0 + (center.x - origin.0) * pixel - label_width / 2,
            y - 10,
            TextStyle::Body,
            color,
        );
    }
    ui_kit::end_clip();
}

/// The "Caves found" collection page.
pub(super) fn collection_section(terrain: &Terrain, player: &Player) -> Option<Section> {
    if !terrain.settings().generator.has_cave_biomes() {
        return None;
    }
    let markers = markers(terrain, player);
    let mut items = Vec::new();
    if markers.is_empty() {
        items.push(SectionItem::text(
            "No caves found yet. Grand caverns, crystal geodes, flooded caves, fungal grottos \
             and stranger places wait in the rock; bring flares for the dark ones.",
        ));
    }
    for kind in CaveKind::ALL {
        let found: Vec<&CaveMarker> = markers
            .iter()
            .filter(|marker| marker.kind == kind)
            .collect();
        let Some(deepest) = found.iter().map(|marker| marker.feature.center.y).max() else {
            continue;
        };
        items.push(SectionItem::stat(
            kind.name(),
            format!("{} found, deepest {deepest} m", found.len()),
            cave_kind_color(kind),
        ));
    }
    Some(Section::new(
        format!(
            "Caves found ({} of {} kinds)",
            CaveKind::ALL
                .iter()
                .filter(|kind| markers.iter().any(|marker| marker.kind == **kind))
                .count(),
            CaveKind::ALL.len()
        ),
        MINT,
        items,
    ))
}
