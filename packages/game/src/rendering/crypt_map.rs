//! The crypt's room-by-room minimap (WP-TOMBS T29): while the view's player is in a top-down
//! crypt run it takes the minimap's corner (and on a touch screen the top-left, under the
//! gauges, since the thumb cluster owns the bottom right). Each room the party has walked into
//! shows as a block coloured by what it is (the shrine, the vault, the lord's hall, the
//! stairs), its doorways as notches, the party as dots and the room the player stands in
//! outlined; rooms nobody has reached stay dark.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map scaling converts tiles to UI pixels"
)]

use fatherload_simulation::{
    DiscoveredTiles,
    base::BuiltTile,
    terrain::{TileKind, TilePosition},
    tomb::{RoomKind, SiteStyle},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "UI code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// A room's colour on the map.
const fn room_color(kind: RoomKind) -> Color {
    match kind {
        RoomKind::Entry => SLATE_LIGHT,
        RoomKind::TrapHall => RUST,
        RoomKind::Puzzle => TEAL,
        RoomKind::Ambush => MAROON,
        RoomKind::Shrine => AMBER,
        RoomKind::Vault => GOLD,
        RoomKind::Boss => RED,
        RoomKind::Stairs => MINT,
    }
}

/// Draws the crypt minimap for `view` when its player is in a top-down crypt; returns whether
/// it did (the caller draws the ordinary minimap otherwise).
#[allow(clippy::too_many_lines, reason = "rooms, doorways, then the party")]
pub(super) fn draw_crypt_minimap(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    explored: &DiscoveredTiles,
    touch: bool,
) -> bool {
    let Some(run) = snapshot
        .tombs
        .run_of(view.player_id)
        .filter(|run| run.site.style == SiteStyle::TopDown)
    else {
        return false;
    };
    if run.rooms.is_empty() {
        return true;
    }
    let (mut min, mut max) = (run.rooms[0].interior.min, run.rooms[0].interior.max);
    for room in &run.rooms {
        min.x = min.x.min(room.interior.min.x);
        min.y = min.y.min(room.interior.min.y);
        max.x = max.x.max(room.interior.max.x);
        max.y = max.y.max(room.interior.max.y);
    }
    let (floor_w, floor_h) = ((max.x - min.x + 3) as f32, (max.y - min.y + 3) as f32);
    let rect = view_rect(view);
    let width = if touch {
        72.0
    } else {
        (rect.width / 4.0).clamp(64.0, 120.0)
    };
    let scale = (width / floor_w).min(48.0 / floor_h);
    let (map_w, map_h) = (floor_w * scale, floor_h * scale);
    let (x, y) = if touch {
        (rect.x + 8.0, rect.y + 86.0)
    } else {
        (
            rect.x + rect.width - map_w - 22.0,
            rect.y + rect.height - map_h - 10.0,
        )
    };
    ui_kit::panel(
        draw,
        Rectangle::new(x - 4.0, y - 13.0, map_w + 8.0, map_h + 17.0),
        PanelStyle::Standard,
    );
    ui_kit::text(
        draw,
        &format!("FLOOR {}/{}", run.floor + 1, run.site.floors()),
        x as i32,
        (y - 11.0) as i32,
        TextStyle::Body,
        FOG,
    );
    draw.draw_rectangle_v(Vector2::new(x, y), Vector2::new(map_w, map_h), NIGHT);
    let to_map = |tile: TilePosition| {
        Vector2::new(
            x + (tile.x - min.x + 1) as f32 * scale,
            y + (tile.y - min.y + 1) as f32 * scale,
        )
    };
    let here = snapshot
        .pilots
        .iter()
        .find(|pilot| pilot.player_id == view.player_id)
        .map(|pilot| {
            let (px, py) = pilot.middle();
            crate::view_frame::frame().tile_at(px, py)
        });
    for room in &run.rooms {
        if !explored.contains(room.interior.center()) {
            continue;
        }
        let at = to_map(room.interior.min);
        let size = Vector2::new(
            room.interior.width() as f32 * scale,
            room.interior.height() as f32 * scale,
        );
        draw.draw_rectangle_v(at, size, with_alpha(room_color(room.kind), 200));
        if here.is_some_and(|tile| room.interior.contains(tile)) {
            draw.draw_rectangle_lines_ex(
                Rectangle::new(at.x - 1.0, at.y - 1.0, size.x + 2.0, size.y + 2.0),
                1.0,
                BONE,
            );
        }
        // Doorways: open (or gated) tiles in the ring of wall round the room.
        let ring = room.interior.expanded(1);
        for ty in ring.min.y..=ring.max.y {
            for tx in ring.min.x..=ring.max.x {
                if room.interior.contains(TilePosition { x: tx, y: ty }) {
                    continue;
                }
                let tile = TilePosition { x: tx, y: ty };
                let door = match snapshot.terrain.tile(tile).map(|found| found.kind) {
                    Some(TileKind::Air) => Some(BONE),
                    Some(TileKind::Built(BuiltTile::Gate)) => Some(STEEL),
                    _ => None,
                };
                if let Some(color) = door {
                    let at = to_map(tile);
                    draw.draw_rectangle_v(at, Vector2::new(scale.max(1.0), scale.max(1.0)), color);
                }
            }
        }
    }
    // The party.
    for member in &run.members {
        let Some(pilot) = snapshot
            .pilots
            .iter()
            .find(|pilot| pilot.player_id == member.player)
        else {
            continue;
        };
        let (px, py) = pilot.middle();
        let tile = crate::view_frame::frame().tile_at(px, py);
        let at = to_map(tile);
        let color = if member.is_downed() {
            RED
        } else if member.player == view.player_id {
            CREAM
        } else {
            SKY_LIGHT
        };
        draw.draw_rectangle_v(
            Vector2::new(at.x - 1.0, at.y - 1.0),
            Vector2::new(3.0, 3.0),
            color,
        );
    }
    true
}

/// The view's rectangle in UI art pixels (as the rest of the HUD measures it).
fn view_rect(view: &ViewSnapshot<'_>) -> Rectangle {
    super::screen::view_rect(view)
}
