//! The world side of planet ground (Wave 11 polar track, [`crate::planet`]).
//!
//! Building a world on a round body, its gravity, the wrap of its frame round the strip, and
//! the per-tick rule that keeps its gravity.

use crate::{
    SimulationParams,
    field::{FieldProfile, FieldZone},
    ids::PlayerId,
    planet::PlanetParams,
    position::Frame,
    terrain::Terrain,
};

use super::{WorldImport, WorldState};

/// The column a new planet world's rig is set down above.
pub const PLANET_SPAWN_COLUMN: i32 = 64;
/// Rows above the surface a new planet world's rig starts at.
pub const PLANET_SPAWN_HEIGHT_ROWS: i32 = 3;
/// The tile bounds of a planet's gravity zone: every column a strip's positions reach, every
/// row (its sky included).
const PLANET_ZONE_MIN: (i32, i32) = (
    -crate::terrain::PLANET_COLUMN_LIMIT,
    -crate::terrain::PLANET_COLUMN_LIMIT,
);
const PLANET_ZONE_MAX: (i32, i32) = (
    crate::terrain::PLANET_COLUMN_LIMIT,
    crate::terrain::PLANET_COLUMN_LIMIT,
);

/// The field zone that carries a planet's gravity.
///
/// F12 radial gravity: in strip coordinates the pull toward the body's centre is straight
/// down, at the body's strength. Rotation is
/// free on a round body: the player turns the craft with its thrusters, and nothing levels it
/// but its tracks on the ground ([`crate::planet::landing`]).
#[must_use]
pub fn planet_gravity_zone(params: &PlanetParams) -> FieldZone {
    let mut profile = FieldProfile::NEUTRAL.with_gravity(0.0, params.gravity());
    profile.free_rotation = true;
    FieldZone::new(PLANET_ZONE_MIN, PLANET_ZONE_MAX, 0, profile)
}

/// Whether `zone` is a planet's gravity zone.
fn is_planet_zone(zone: &FieldZone) -> bool {
    (zone.min_x, zone.min_y) == PLANET_ZONE_MIN
        && (zone.max_x, zone.max_y) == PLANET_ZONE_MAX
        && zone.preset.is_none()
}

impl WorldState {
    /// A brand-new single-rig world on the round body `params` describe (Wave 11): its
    /// terrain is the body's unrolled strip ([`Terrain::planet`]), its gravity the body's, the
    /// rig hovering just above the ground at [`PLANET_SPAWN_COLUMN`].
    ///
    /// **TODO-SEAM (w11-regions):** until a world holds many places, a planet is a world of its
    /// own; the regions track builds a body's region with [`Terrain::planet`] and
    /// [`planet_gravity_zone`] instead.
    #[must_use]
    pub fn new_planet_world(
        player_id: PlayerId,
        params: PlanetParams,
        sim: SimulationParams,
    ) -> Self {
        let terrain = Terrain::planet(params);
        let mut import =
            WorldImport::new_game_with_settings(player_id, &Terrain::planet_settings(params.seed));
        let frame = terrain.frame();
        let surface = terrain.surface_row(PLANET_SPAWN_COLUMN);
        let (x, y) = frame.centre_of(PLANET_SPAWN_COLUMN, surface - PLANET_SPAWN_HEIGHT_ROWS);
        import.player.x = x;
        import.player.y = y;
        import.terrain = terrain;
        let mut world = Self::from_import_with_params(&import, sim);
        world.fields.insert(planet_gravity_zone(&params));
        world
    }

    /// The planet this world is the ground of, if any.
    #[must_use]
    pub const fn planet(&self) -> Option<&crate::planet::PlanetGround> {
        self.terrain.planet_ground()
    }

    /// Keeps a planet's gravity zone in place (a test-mode "clear zones" must not switch a
    /// body's gravity off).
    pub(super) fn keep_planet_gravity(&mut self) {
        let Some(params) = self.planet().map(|planet| *planet.params()) else {
            return;
        };
        if !self.fields.iter().any(|(_, zone)| is_planet_zone(zone)) {
            self.fields.insert(planet_gravity_zone(&params));
        }
    }

    /// The frame a planet world's frame is labelled with: its sector column folded into one
    /// lap of the strip (`0..lap_sectors`). Any other world's frame is its own.
    pub(super) const fn wrapped_frame(&self, frame: Frame) -> Frame {
        let Some(planet) = self.planet() else {
            return frame;
        };
        Frame {
            sector_x: frame.sector_x.rem_euclid(planet.grid().lap_sectors()),
            ..frame
        }
    }

    /// The frame a move to `next` really shifts positions by: on a planet, the nearest copy of
    /// `next` round the strip (moving by whole laps moves nothing: the strip wraps).
    pub(super) const fn nearest_lap_frame(&self, next: Frame) -> Frame {
        let Some(planet) = self.planet() else {
            return next;
        };
        let lap = planet.grid().lap_sectors();
        let current = self.frame().sector_x;
        let mut delta = (next.sector_x - current).rem_euclid(lap);
        if delta > lap / 2 {
            delta -= lap;
        }
        Frame {
            sector_x: current + delta,
            ..next
        }
    }

    /// The planet rules of one tick: the body's gravity stays in place, and everything with a
    /// local position stays on the copy of the strip nearest the frame.
    pub(super) fn tick_planet(&mut self) {
        if self.planet().is_some() {
            self.keep_planet_gravity();
            self.wrap_to_nearest_lap();
        }
    }

    /// Moves every rig, entity and lost-cargo site that has drifted more than half a lap from
    /// the frame's origin by whole laps back to the copy of the strip nearest it.
    ///
    /// The frame follows its anchor (the first player) and is relabelled by whole laps, so
    /// without this a drop left on the ground, a creature or another player's rig would be a
    /// lap away in local pixels after the anchor drove once round the body, invisible and out
    /// of reach though the strip says it is the same place. A lap is a whole number of
    /// sectors, so the move is exact; entities that move get a new revision, so replicas
    /// receive them.
    pub(super) fn wrap_to_nearest_lap(&mut self) {
        use crate::position::Reframe;
        let Some(planet) = self.planet() else {
            return;
        };
        #[allow(
            clippy::cast_precision_loss,
            reason = "a lap is at most a few hundred sectors: exact in f32"
        )]
        let lap_px = planet.grid().lap_sectors() as f32 * crate::position::SECTOR_PX;
        let half = lap_px * 0.5;
        let shift_for = |x: f32| -> Option<crate::position::FrameShift> {
            if !x.is_finite() || (-half..half).contains(&x) {
                return None;
            }
            let laps = ((x + half) / lap_px).floor();
            Some(crate::position::FrameShift {
                dx: laps * lap_px,
                dy: 0.0,
            })
        };
        for player in self.players.values_mut() {
            if let Some(shift) = shift_for(player.x) {
                player.reframe(shift);
            }
        }
        for site in self.lost_cargo_sites.values_mut() {
            if let Some(shift) = shift_for(site.x) {
                site.reframe(shift);
            }
        }
        let far: Vec<_> = self
            .entities
            .iter()
            .filter_map(|entity| shift_for(entity.body.x).map(|shift| (entity.id, shift)))
            .collect();
        for (id, shift) in far {
            self.entities.update(id, |entity| entity.reframe(shift));
        }
    }
}
