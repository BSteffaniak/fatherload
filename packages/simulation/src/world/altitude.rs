//! Altitude moments in a V8 sky (expansion plan F6): the gameplay events a rig's flight raises
//! as it climbs into a new band, catches an updraft or flies into cold its de-icer does not
//! cover. The physics itself lives in [`crate::environment`] and the movement step.
//!
//! The events are stateless edges between the rig's position before and after its movement
//! step, so they replicate, predict and replay like any other event and need no saved state.

use crate::{
    environment::{cold_excess, environment_at},
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    sky::SkyBand,
    terrain::TILE_SIZE,
    worldgen::sky::updraft_at,
};

use super::WorldState;

/// The tile row of world pixel y `y`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "pixel positions map to tiles"
)]
fn row(y: f32) -> i32 {
    (y / TILE_SIZE).floor() as i32
}

/// The tile column of world pixel x `x`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "pixel positions map to tiles"
)]
fn column(x: f32) -> i32 {
    (x / TILE_SIZE).floor() as i32
}

impl WorldState {
    /// Pushes the altitude events of `player_id`'s move from `before` (world pixels) to where
    /// the rig is now: [`GameplayEventKind::SkyBandEntered`] climbing into a higher band,
    /// [`GameplayEventKind::UpdraftCaught`] entering an updraft and
    /// [`GameplayEventKind::RigIcing`] crossing into uncovered cold.
    pub(super) fn push_altitude_events(
        &self,
        player_id: PlayerId,
        before: (f32, f32),
        output: &mut TickOutput,
    ) {
        let settings = self.terrain.settings();
        if !settings.generator.has_sky() {
            return;
        }
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let (from_row, to_row) = (row(before.1), row(player.y));
        if from_row == to_row && column(before.0) == column(player.x) {
            return;
        }
        let (from_band, to_band) = (SkyBand::at_row(from_row), SkyBand::at_row(to_row));
        if to_band > from_band {
            output.push_event(GameplayEventKind::SkyBandEntered {
                player_id,
                band: to_band,
            });
        }
        if updraft_at(settings, column(before.0), from_row) == 0
            && updraft_at(settings, column(player.x), to_row) > 0
        {
            output.push_event(GameplayEventKind::UpdraftCaught { player_id });
        }
        let de_icer = player.hazard_gear.de_icer;
        let cold_before = environment_at(settings, before.0, before.1).cold;
        let cold_after = environment_at(settings, player.x, player.y).cold;
        if cold_excess(cold_before, de_icer) == 0 && cold_excess(cold_after, de_icer) > 0 {
            output.push_event(GameplayEventKind::RigIcing { player_id });
        }
    }
}
