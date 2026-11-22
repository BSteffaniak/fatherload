//! Test-mode no-clip flight: rigs fly freely through rock, with no gravity, collision, damage or
//! drilling (see [`crate::test_mode::TestModeState::no_clip`]).

use crate::{
    ids::PlayerId,
    quantize::quantize_simulation_scalar,
    terrain::{TILE_SIZE, TileKind, TilePosition, is_indestructible},
};

use super::{
    WorldState,
    movement::{
        AuthoritativeMovementStep, DriveTuning, authoritative_player_collides,
        clamp_authoritative_player_x, clamp_authoritative_player_y, integrate_x,
    },
};

/// Top no-clip flight speed along either axis (px/s): the god-mode drive speed.
pub const NO_CLIP_SPEED: f32 = 25.0 * TILE_SIZE;
/// Rows above (and columns either side of) a rig left in rock searched for open air.
const FREE_SEARCH_ROWS: i32 = 4;

impl WorldState {
    /// Whether rigs are flying in no-clip mode (test mode only).
    #[must_use]
    pub fn no_clip_active(&self) -> bool {
        self.test_mode.is_some_and(|state| state.no_clip)
    }

    /// One tick of no-clip flight for `player_id`: `horizontal` and `vertical` (`-1..=1`,
    /// positive is right and down) steer the eased drive on each axis.
    pub(crate) fn step_no_clip_flight(
        &mut self,
        player_id: PlayerId,
        horizontal: f32,
        vertical: f32,
    ) -> Option<AuthoritativeMovementStep> {
        let rate = self.timing.rate;
        let delta_seconds = self.timing.delta_seconds;
        let tuning = DriveTuning::new(NO_CLIP_SPEED, true);
        let player = self.players.get_mut(&player_id)?;
        player.velocity_x =
            quantize_simulation_scalar(tuning.step_tick(player.velocity_x, horizontal, rate));
        player.velocity_y =
            quantize_simulation_scalar(tuning.step_tick(player.velocity_y, vertical, rate));
        // Horizontal travel keeps the endless-world remainder (see `integrate_x`).
        let (next_x, next_remainder) = integrate_x(
            player.x,
            player.x_remainder,
            quantize_simulation_scalar(player.velocity_x * delta_seconds),
        );
        let next_y = player.velocity_y.mul_add(delta_seconds, player.y);
        let clamped_x = clamp_authoritative_player_x(&self.terrain, next_x);
        let clamped_y = clamp_authoritative_player_y(&self.terrain, next_y);
        // Stop against the world's edges.
        if clamped_x.to_bits() == next_x.to_bits() {
            player.x_remainder = next_remainder;
        } else {
            player.velocity_x = 0.0;
            player.x_remainder = 0.0;
        }
        if clamped_y.to_bits() != next_y.to_bits() {
            player.velocity_y = 0.0;
        }
        let (x, y) = (
            quantize_simulation_scalar(clamped_x),
            quantize_simulation_scalar(clamped_y),
        );
        player.x = x;
        player.y = y;
        Some(AuthoritativeMovementStep {
            player_id,
            x,
            y,
            velocity_x: player.velocity_x,
            velocity_y: player.velocity_y,
        })
    }

    /// Frees every rig left inside rock when no-clip flight ends: into open air within a few
    /// tiles above it if there is any, otherwise by clearing the tile it sits in (moving up past
    /// bedrock first). Each rig is left at rest.
    pub(super) fn free_rigs_from_rock(&mut self) {
        let stuck: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| authoritative_player_collides(&self.terrain, player.x, player.y))
            .map(|(id, _)| *id)
            .collect();
        for player_id in stuck {
            let Some(tile) = self
                .players
                .get(&player_id)
                .map(|player| player.tile_in(self.frame()))
            else {
                continue;
            };
            let open = (0..=FREE_SEARCH_ROWS)
                .flat_map(|up| [0, -1, 1].map(move |side| (side, up)))
                .map(|(side, up)| TilePosition {
                    x: tile.x + side,
                    y: tile.y - up,
                })
                .find(|candidate| {
                    let (x, y) = self.frame().tile_center(*candidate);
                    !authoritative_player_collides(&self.terrain, x, y)
                });
            let target = open.unwrap_or_else(|| {
                // Climb past indestructible rows (bedrock), then clear the tile.
                let mut target = tile;
                while self
                    .terrain
                    .tile(target)
                    .is_some_and(|tile| is_indestructible(tile.kind))
                {
                    target.y -= 1;
                }
                let _cleared = self.terrain.set_kind(target, TileKind::Air);
                target
            });
            self.set_active_drill(player_id, None);
            let (x, y) = self.frame().tile_center(target);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.x = x;
                player.x_remainder = 0.0;
                player.y = y;
                player.velocity_x = 0.0;
                player.velocity_y = 0.0;
            }
        }
    }
}
