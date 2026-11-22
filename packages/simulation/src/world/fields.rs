//! The world side of field effects (F9, see [`crate::field`]): the per-point query and the
//! authored zones.

use crate::{
    field::{FieldPreset, FieldSample, FieldZone, FieldZones},
    ids::PlayerId,
    summary::PlayerScopedCommandOutcome,
};

use super::WorldState;

/// Half-size (tiles) of a dev zone placed through test mode.
pub const DEV_ZONE_HALF_SIZE: (i32, i32) = (9, 6);

impl WorldState {
    /// The fields at world pixel `(x, y)`: the stratum profile blended with every zone over
    /// it ([`crate::field::sample`]). [`FieldSample::NEUTRAL`] everywhere in a world without
    /// zones today.
    #[must_use]
    pub fn fields_at(&self, x: f32, y: f32) -> FieldSample {
        crate::field::sample_in(
            &self.fields,
            self.terrain.settings(),
            self.terrain.frame(),
            x,
            y,
        )
    }

    /// The fields where `player_id`'s rig (or, on foot, its pilot) is.
    #[must_use]
    pub fn fields_for(&self, player_id: PlayerId) -> FieldSample {
        self.actor_position(player_id)
            .map_or(FieldSample::NEUTRAL, |(x, y)| self.fields_at(x, y))
    }

    /// The authored field zones.
    #[must_use]
    pub const fn field_zones(&self) -> &FieldZones {
        &self.fields
    }

    /// Adds an authored field zone before play (scenes, tests, world setup, generator hooks)
    /// and returns its id. In play, zones change only inside the tick (test mode's
    /// [`crate::TestModeAction::PlaceFieldZone`]).
    pub fn place_field_zone(&mut self, zone: FieldZone) -> u32 {
        self.fields.insert(zone)
    }

    /// Removes every authored field zone before play.
    pub fn clear_field_zones(&mut self) {
        self.fields.clear();
    }

    /// Test mode: a zone of `preset` centred on `player_id`'s rig or pilot.
    pub(super) fn place_dev_field_zone(
        &mut self,
        player_id: PlayerId,
        preset: FieldPreset,
    ) -> PlayerScopedCommandOutcome {
        let Some((x, y)) = self.actor_position(player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let frame = self.frame();
        let tile = (frame.col(x), frame.row(y));
        self.fields
            .insert(FieldZone::of_preset(preset, tile, DEV_ZONE_HALF_SIZE));
        PlayerScopedCommandOutcome::Applied
    }
}
