//! Points of interest on the minimap and the mine map, and the collection log page.
//!
//! What a player sees of a POI is the clearer of their journal (sites visited, scanner pulses)
//! and a live reading of their scanner while the POI is in range: low scanner tiers only show a
//! blinking "strange echo" near the site, higher tiers its outline (silhouette) and icon.

use drillgame_simulation::{
    Player,
    poi::{PoiKind, PoiKnowledge, PoiTile, PoiVariant, poi_reading},
    terrain::{TILE_SIZE, Terrain, TileKind},
    worldgen::WorldFeature,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::super::{
    layout::{Section, SectionItem},
    pixel_art::sprites::poi::{ICON_ECHO, known_poi_icon, poi_color, poi_icon},
    terrain::tile_art::hash,
    ui_kit,
};

/// One POI the player can see on a map.
#[derive(Clone, Copy, Debug)]
pub(super) struct PoiMarker {
    pub feature: WorldFeature,
    pub kind: PoiKind,
    pub knowledge: PoiKnowledge,
}

impl PoiMarker {
    /// Where the marker is drawn, in tiles: the centre for a known site, a point a couple of
    /// tiles off for an echo (the scanner cannot pin it down).
    fn anchor(&self) -> (i32, i32) {
        let center = self.feature.center;
        if self.knowledge == PoiKnowledge::Echo {
            let h = hash(center.x, center.y, 131);
            (
                center.x + (h % 5).cast_signed() - 2,
                center.y + ((h >> 4) % 5).cast_signed() - 2,
            )
        } else {
            (center.x, center.y)
        }
    }
}

/// Every POI `player` can see, with what they know about it.
pub(super) fn markers(terrain: &Terrain, player: &Player) -> Vec<PoiMarker> {
    if !terrain.settings().generator.has_points_of_interest() {
        return Vec::new();
    }
    let center = player.tile_position(TILE_SIZE);
    terrain
        .features()
        .pois()
        .filter_map(|feature| {
            let kind = feature.poi_kind()?;
            let knowledge = poi_reading(
                &player.journal,
                feature.id,
                &feature.bounds,
                center,
                player.scanner_level,
                false,
            )?;
            Some(PoiMarker {
                feature: *feature,
                kind,
                knowledge,
            })
        })
        .collect()
}

/// Whether the echo marker is in its lit phase.
fn echo_lit() -> bool {
    (ui_kit::seconds() * 2.0) as i32 % 2 == 0
}

/// Small markers on the minimap; `to_map` maps a tile to a minimap pixel, `None` when it is
/// outside the minimap's window.
pub(super) fn draw_minimap_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[PoiMarker],
    to_map: impl Fn(i32, i32) -> Option<(i32, i32)>,
) {
    for marker in markers {
        let (tile_x, tile_y) = marker.anchor();
        let Some((x, y)) = to_map(tile_x, tile_y) else {
            continue;
        };
        if marker.knowledge == PoiKnowledge::Echo {
            if echo_lit() {
                draw.draw_rectangle(x - 1, y - 1, 3, 3, INK);
                draw.draw_rectangle(x, y, 1, 1, MINT);
            }
            continue;
        }
        draw.draw_rectangle(x - 2, y - 2, 4, 4, INK);
        draw.draw_rectangle(x - 1, y - 1, 2, 2, poi_color(marker.kind));
    }
}

/// Outlines (silhouettes) and icons on the mine map, clipped to `clip`. `(x0, y0)` is the
/// map's top-left canvas pixel showing tile `origin`; each tile is `pixel` canvas pixels.
pub(super) fn draw_map_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[PoiMarker],
    clip: Rectangle,
    origin: (i32, i32),
    pixel: i32,
) {
    ui_kit::begin_clip(clip);
    draw_clipped_map_markers(draw, markers, (clip.x as i32, clip.y as i32), origin, pixel);
    ui_kit::end_clip();
}

fn draw_clipped_map_markers(
    draw: &mut RaylibDrawHandle<'_>,
    markers: &[PoiMarker],
    (x0, y0): (i32, i32),
    origin: (i32, i32),
    pixel: i32,
) {
    for marker in markers
        .iter()
        .filter(|marker| marker.knowledge != PoiKnowledge::Echo)
    {
        let bounds = marker.feature.bounds;
        let x = x0 + (bounds.min.x - origin.0) * pixel;
        let y = y0 + (bounds.min.y - origin.1) * pixel;
        let (width, height) = (bounds.width() * pixel, bounds.height() * pixel);
        draw.draw_rectangle_lines(x - 1, y - 1, width + 2, height + 2, INK);
        draw.draw_rectangle_lines(x, y, width, height, poi_color(marker.kind));
    }
    for marker in markers {
        let (tile_x, tile_y) = marker.anchor();
        let icon = known_poi_icon(marker.kind, marker.knowledge);
        let x = x0 + (tile_x - origin.0) * pixel + pixel / 2 - icon.width() / 2;
        let y = y0 + (tile_y - origin.1) * pixel + pixel / 2 - icon.height() / 2;
        if marker.knowledge == PoiKnowledge::Echo {
            ui_kit::icon_frame(draw, icon, ICON_ECHO.frame_at(ui_kit::seconds()), x, y);
        } else {
            ui_kit::icon(draw, icon, x, y);
        }
    }
}

/// Whether a vault's doors are still shut.
fn vault_sealed(terrain: &Terrain, feature: &WorldFeature) -> bool {
    feature.bounds.positions().any(|position| {
        terrain
            .tile(position)
            .is_some_and(|tile| tile.kind == TileKind::Poi(PoiTile::VaultDoor))
    })
}

/// The collection log: sites visited, scanner contacts and recovered logs.
pub(super) fn collection_sections(terrain: &Terrain, player: &Player) -> Vec<Section> {
    let markers = markers(terrain, player);
    let visited: Vec<&PoiMarker> = markers
        .iter()
        .filter(|marker| marker.knowledge == PoiKnowledge::Visited)
        .collect();
    let identified = markers
        .iter()
        .filter(|marker| marker.knowledge == PoiKnowledge::Identified)
        .count();
    let echoes = markers
        .iter()
        .filter(|marker| marker.knowledge == PoiKnowledge::Echo)
        .count();
    let total = if terrain.settings().generator.has_points_of_interest() {
        terrain.features().pois().count()
    } else {
        0
    };
    let mut sites = vec![SectionItem::icon_stat(
        &ICON_ECHO,
        "Scanner contacts",
        format!(
            "{identified} identified, {echoes} echo{}",
            if echoes == 1 { "" } else { "es" }
        ),
        MIST,
    )];
    if visited.is_empty() {
        sites.push(SectionItem::text(
            "No sites visited yet. A scanner hears strange echoes from them; better scanners \
             show their outline on the map.",
        ));
    }
    for marker in &visited {
        let feature = &marker.feature;
        let variant = feature.poi();
        let title = variant.map_or_else(|| marker.kind.name().to_owned(), PoiVariant::title);
        sites.push(SectionItem::icon_stat(
            poi_icon(marker.kind),
            title,
            format!("{}, {} m", feature.band.name(), feature.center.y),
            MIST,
        ));
        if let Some(PoiVariant::Vault(puzzle)) = variant {
            sites.push(SectionItem::text(if vault_sealed(terrain, feature) {
                format!("Sealed. {}", puzzle.hint())
            } else {
                "Open.".to_owned()
            }));
        }
    }
    let mut logs = Vec::new();
    if player.journal.logs.is_empty() {
        logs.push(SectionItem::text(
            "Derelict rigs carry logs. Drill into a wreck's core to recover one.",
        ));
    }
    for log in &player.journal.logs {
        let entry = log.entry();
        logs.push(SectionItem::stat(entry.title, "", BONE));
        logs.push(SectionItem::text(entry.text));
    }
    vec![
        Section::new(
            format!("Sites visited ({} of {total})", visited.len()),
            GOLD,
            sites,
        ),
        Section::new(
            format!("Recovered logs ({})", player.journal.logs.len()),
            PINK,
            logs,
        ),
    ]
}
