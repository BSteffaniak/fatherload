//! Per-player map discovery and scanner reveals.

use crate::{
    ids::PlayerId,
    terrain::{TILE_SIZE, TilePosition},
};

use super::WorldState;

impl WorldState {
    pub fn reveal_scanner_area(
        &mut self,
        player_id: PlayerId,
        center: TilePosition,
        radius: i32,
    ) -> usize {
        self.reveal_scanner_positions(player_id, center, radius)
            .len()
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "world positions are converted to terrain tile coordinates"
    )]
    pub fn reveal_tiles_near_player(&mut self, player_id: PlayerId) -> Vec<TilePosition> {
        let Some(player) = self.players.get(&player_id) else {
            return Vec::new();
        };
        let center = TilePosition {
            x: (player.x / TILE_SIZE).floor() as i32,
            y: (player.y / TILE_SIZE).floor() as i32,
        };
        self.reveal_rectangular_area(player_id, center, 4, 3)
    }

    pub(super) fn reveal_rectangular_area(
        &mut self,
        player_id: PlayerId,
        center: TilePosition,
        radius_x: i32,
        radius_y: i32,
    ) -> Vec<TilePosition> {
        let discovered = self.discovered_tiles.entry(player_id).or_default();
        let mut visible = Vec::new();
        for y in center.y - radius_y.max(0)..=center.y + radius_y.max(0) {
            for x in center.x - radius_x.max(0)..=center.x + radius_x.max(0) {
                let position = TilePosition { x, y };
                discovered.insert(position);
                visible.push(position);
            }
        }
        visible
    }

    pub(super) fn reveal_scanner_positions(
        &mut self,
        player_id: PlayerId,
        center: TilePosition,
        radius: i32,
    ) -> Vec<TilePosition> {
        let discovered = self.discovered_tiles.entry(player_id).or_default();
        let radius = radius.max(0);
        let mut revealed = Vec::new();
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if (x - center.x).abs() + (y - center.y).abs() <= radius {
                    let position = TilePosition { x, y };
                    if discovered.insert(position) {
                        revealed.push(position);
                    }
                }
            }
        }
        revealed
    }
}
