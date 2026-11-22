//! Altitude moments in a V8 sky (expansion plan F6): the gameplay events a rig's flight raises
//! as it climbs into a new band, catches an updraft or flies into cold its de-icer does not
//! cover. The physics itself lives in [`crate::environment`] and the movement step.
//!
//! The events are stateless edges between the rig's position before and after its movement
//! step, so they replicate, predict and replay like any other event and need no saved state.

use crate::{
    environment::jet_stream_in_air,
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    sky::SkyBand,
    worldgen::sky::{updraft_at, vent_updraft_at},
};

use super::WorldState;

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
        let Some(bands) = settings.generator.sky_bands() else {
            return;
        };
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let frame = self.frame();
        let (row, column) = (|y: f32| frame.row(y), |x: f32| frame.col(x));
        let (from_row, to_row) = (row(before.1), row(player.y));
        if from_row == to_row && column(before.0) == column(player.x) {
            return;
        }
        let (from_band, to_band) = (
            SkyBand::at_row_in(from_row, &bands),
            SkyBand::at_row_in(to_row, &bands),
        );
        if to_band > from_band {
            output.push_event(GameplayEventKind::SkyBandEntered {
                player_id,
                band: to_band,
            });
        }
        // Classic updrafts and (V9) island vents that really lift this rig (free lift stops
        // at the dead-air gap, `environment::gap_lift_share`, and lets go of a laden rig that
        // cannot cross it, `milestone::free_lift_share`).
        let lift = |x: i32, row: i32| {
            #[allow(
                clippy::cast_precision_loss,
                reason = "altitudes are a few thousand rows"
            )]
            let share = crate::environment::gap_lift_share(-(row as f32), &bands)
                * crate::milestone::free_lift_share(player, -row, &bands);
            share > 0.0 && updraft_at(settings, x, row) + vent_updraft_at(settings, x, row) > 0
        };
        if !lift(column(before.0), from_row) && lift(column(player.x), to_row) {
            output.push_event(GameplayEventKind::UpdraftCaught { player_id });
        }
        // V9 jet streams (none inside the gap).
        if jet_stream_in_air(settings, column(before.0), from_row) == 0.0
            && jet_stream_in_air(settings, column(player.x), to_row) != 0.0
        {
            output.push_event(GameplayEventKind::JetStreamCaught { player_id });
        }
        let gear = player.hazard_gear;
        let frame = self.frame();
        let cold_before =
            crate::environment::environment_in(settings, frame, before.0, before.1).cold;
        let cold_after =
            crate::environment::environment_in(settings, frame, player.x, player.y).cold;
        if gear.cold_excess(cold_before) == 0 && gear.cold_excess(cold_after) > 0 {
            output.push_event(GameplayEventKind::RigIcing { player_id });
        }
    }
}
