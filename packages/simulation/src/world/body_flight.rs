//! Landing on and taking off from a generated body's round ground (Wave 11 integration: the
//! seam between flight, the regions' body stores and the polar ground).
//!
//! * **Landing (re-entry ends on the ground).** A rig flying in space ([`crate::flight`]) that
//!   comes down inside the top of a landable body's ground strip
//!   ([`crate::planet::PlanetGround::strip_top`], [`LAND_INSIDE_ROWS`] in) is handed over to
//!   that body's store with [`WorldState::move_to_place`]: its space position becomes the strip
//!   point under it ([`crate::polar::PolarGrid::from_world`]), its velocity is turned into the
//!   strip's frame (along the ground and toward the core) and its orientation is measured from
//!   the local up there (`orientation - local_frame(u).angle`). Nothing levels it: the player
//!   lands it with the polar track's landing skill (touchdown, slide, scrape, tip, legs).
//! * **On the ground** the rig drives, digs and flies with the ordinary ground code in the
//!   body's ground pass (radial gravity is the planet's gravity zone, the store's own field
//!   zone). Its flight state reads [`FlightPlace::Landed`] (not "away": the classic movement
//!   runs), and life support keeps counting (refilling on a breathable world).
//! * **Taking off.** Climbing [`TAKE_OFF_ABOVE_ROWS`] over the strip's top hands the rig back
//!   to space at the matching point round the body ([`crate::polar::PolarGrid::to_world`]),
//!   turned and moving as it left, in the body's approach sphere; its home-world position is
//!   parked over the Moon again ([`WorldState::space_park`]).
//! * **The beacon** (air out, hull wrecked, or the rescue / beacon command) tows a rig on a
//!   body home to the spawn point, as in space.
//!
//! Every step is a pure function of the tick's state, so it runs alike on the host and in
//! every client's prediction.

use crate::{
    attitude::{RigAttitude, quantize_angle},
    event::{GameplayEventKind, TickOutput},
    field::trig::wrap_angle,
    flight::{
        AIR_REFILL_PER_SECOND, AIR_WARNINGS, BodyRef, FlightEvent, FlightPlace, RigFlight,
        StarDrive, beacon_fee,
        space::{BodyKind, SpaceQuery},
    },
    ids::PlayerId,
    place::{BodyId, Place, SpaceBodyKey},
    planet::{PlanetGround, PlanetParams},
    polar::offset_world,
    position::WorldPos,
    terrain::TILE_SIZE,
};

use super::{WorldState, flight::AIR_REARM_SHARE};

/// A rig is handed to a body's ground this many rows inside the top of its strip.
pub const LAND_INSIDE_ROWS: f64 = 8.0;
/// A rig on a body's ground leaves for space this many rows over the top of its strip (more
/// than [`LAND_INSIDE_ROWS`], so a rig hovering at the line never flickers between the two).
pub const TAKE_OFF_ABOVE_ROWS: f64 = 2.0;

/// A landable body's ground, as the handover needs it.
#[derive(Clone, Copy, Debug)]
pub struct BodyGroundInfo {
    pub body: BodyRef,
    pub id: SpaceBodyKey,
    pub params: PlanetParams,
    pub centre: WorldPos,
    /// The strip's top row (negative: rows above the datum surface).
    pub strip_top: i32,
}

impl BodyGroundInfo {
    /// Radius (tiles) at which a falling rig is handed to the ground.
    #[must_use]
    pub fn land_radius_tiles(&self) -> f64 {
        f64::from(self.params.radius) - f64::from(self.strip_top) - LAND_INSIDE_ROWS
    }
}

/// The strip position (absolute, in the body store's coordinates) of strip point `(u, v)`.
#[must_use]
pub fn strip_position(u: f64, v: f64) -> WorldPos {
    let tile = f64::from(TILE_SIZE);
    offset_world(WorldPos::default(), u * tile, v * tile)
}

/// The strip point `(u, v)` (columns, rows) of an absolute strip position.
#[must_use]
pub fn strip_point(position: WorldPos) -> (f64, f64) {
    let tile = f64::from(TILE_SIZE);
    let (dx, dy) = WorldPos::default().delta_f64(position);
    (dx / tile, dy / tile)
}

impl WorldState {
    /// The round ground of flight body `body`, if it is a landable generated body with one.
    #[must_use]
    pub fn body_ground(&self, body: BodyRef) -> Option<BodyGroundInfo> {
        let space = self.space();
        let id = space.space_body(body)?;
        let params = crate::place::planet_params_for(self.home_seed(), id)?;
        let found = space.body(body)?;
        if found.kind != BodyKind::Planet {
            return None;
        }
        Some(BodyGroundInfo {
            body,
            id,
            params,
            centre: found.centre,
            strip_top: PlanetGround::new(params).strip_top(),
        })
    }

    /// The flight body a body store is the ground of.
    #[must_use]
    pub fn body_ground_of(&self, id: SpaceBodyKey) -> Option<BodyGroundInfo> {
        self.body_ground(self.space().body_ref(id)?)
    }

    /// Hands `player_id`'s rig over to a body's ground when it came down inside the top of
    /// the body's strip (see the module docs). Called after the rig's flight step.
    pub(super) fn land_on_body_ground(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let flight = player.flight;
        if !flight.away() || matches!(flight.drive, StarDrive::Warp(_) | StarDrive::Transit(_)) {
            return;
        }
        let Some(body) = flight.place.body() else {
            return;
        };
        let Some(ground) = self.body_ground(body) else {
            return;
        };
        let grid = ground.params.grid();
        let (dx, dy) = ground.centre.delta_f64(flight.pos);
        #[allow(
            clippy::suboptimal_flops,
            clippy::imprecise_flops,
            reason = "plain IEEE arithmetic: the same bits on every peer"
        )]
        let distance = (dx * dx + dy * dy).sqrt() / f64::from(TILE_SIZE);
        if distance > ground.land_radius_tiles() {
            return;
        }
        let (u, v) = grid.from_world(ground.centre, flight.pos);
        let frame = grid.local_frame(u);
        #[allow(clippy::cast_possible_truncation, reason = "unit vectors and an angle")]
        let (tangent, down, angle) = (
            (frame.tangent.0 as f32, frame.tangent.1 as f32),
            (frame.down.0 as f32, frame.down.1 as f32),
            frame.angle as f32,
        );
        let cap = super::AUTHORITATIVE_ABSOLUTE_MAX_SPEED;
        let along = flight
            .velocity_x
            .mul_add(tangent.0, flight.velocity_y * tangent.1);
        let toward_core = flight
            .velocity_x
            .mul_add(down.0, flight.velocity_y * down.1);
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.velocity_x = along.clamp(-cap, cap);
        player.velocity_y = toward_core.clamp(-cap, cap);
        player.x_remainder = 0.0;
        player.attitude = RigAttitude {
            orientation: quantize_angle(wrap_angle(player.attitude.orientation - angle)),
            angular_velocity: player.attitude.angular_velocity,
        };
        player.flight = RigFlight {
            place: FlightPlace::Landed(body),
            drive: StarDrive::Off,
            velocity_x: 0.0,
            velocity_y: 0.0,
            heat: 0.0,
            ..flight
        };
        // A fresh landing warns of the world's hazards afresh (Wave 12 stations).
        self.stations.warned.remove(&player_id);
        let place = Place::OnBody(BodyId::Space(ground.id));
        let _ = self.move_to_place(player_id, place, strip_position(u, v));
    }

    /// A body store's ground pass (after the rigs moved): rigs climbing out of the strip take
    /// off into space, life support counts on, and the beacon tows the stranded home.
    pub(super) fn advance_body_flight(&mut self, output: &mut TickOutput) {
        let crate::place::StoreKey::Body(id) = self.active_store() else {
            return;
        };
        let Some(ground) = self.body_ground_of(id) else {
            return;
        };
        let seconds = self.timing.delta_seconds;
        let frame = self.frame();
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in ids {
            // The world's hazard tags, countered by the ship's planet gear (Wave 12 stations).
            self.body_hazards(player_id, id, seconds, output);
            let on_foot = self.is_on_foot(player_id);
            let capacity = self.air_capacity_of(player_id);
            // The body's own air where the rig stands, and the weather fouling it (Wave 12
            // bodylife).
            let breathable = self
                .players
                .get(&player_id)
                .is_some_and(|player| self.body_breathable_at(player.x, player.y).unwrap_or(false));
            let Some(player) = self.players.get_mut(&player_id) else {
                continue;
            };
            // Life support (the pilot on foot breathes the rig's air too).
            let flight = &mut player.flight;
            if breathable {
                flight.air = AIR_REFILL_PER_SECOND
                    .mul_add(seconds, flight.air)
                    .min(capacity);
            } else {
                flight.air = (flight.air - seconds).max(0.0);
            }
            if flight.air > capacity * AIR_REARM_SHARE {
                flight.alerts &= !0b111;
            }
            for (bit, percent) in AIR_WARNINGS.iter().enumerate() {
                let mask = 1_u8 << bit;
                if flight.alerts & mask == 0 && flight.air <= capacity * f32::from(*percent) / 100.0
                {
                    flight.alerts |= mask;
                    output.push_event(GameplayEventKind::Flight {
                        what: FlightEvent::AirLow,
                        player_id,
                        detail: u32::from(*percent),
                    });
                }
            }
            if flight.air <= 0.0 || player.hull <= 0.0 {
                self.body_beacon(player_id, output);
                continue;
            }
            // Taking off: the rig (not a pilot on foot) over the top of the strip.
            let (_, v) = strip_point(frame.world(player.x, player.y));
            if on_foot || v > f64::from(ground.strip_top) - TAKE_OFF_ABOVE_ROWS {
                continue;
            }
            self.take_off_from_body(player_id, &ground, output);
        }
    }

    /// `player_id`'s rig leaves `ground`'s strip for space (see the module docs).
    fn take_off_from_body(
        &mut self,
        player_id: PlayerId,
        ground: &BodyGroundInfo,
        output: &mut TickOutput,
    ) {
        let Some(park) = self.space_park_at_home() else {
            return;
        };
        let frame = self.frame();
        let grid = ground.params.grid();
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let (u, v) = strip_point(frame.world(player.x, player.y));
        let local = grid.local_frame(u);
        #[allow(clippy::cast_possible_truncation, reason = "unit vectors and an angle")]
        let (tangent, down, angle) = (
            (local.tangent.0 as f32, local.tangent.1 as f32),
            (local.down.0 as f32, local.down.1 as f32),
            local.angle as f32,
        );
        let (along, toward_core) = (player.velocity_x, player.velocity_y);
        player.flight = RigFlight {
            place: FlightPlace::Approach(ground.body),
            pos: grid.to_world(ground.centre, u, v),
            velocity_x: along.mul_add(tangent.0, toward_core * down.0),
            velocity_y: along.mul_add(tangent.1, toward_core * down.1),
            drive: StarDrive::Off,
            heat: 0.0,
            ..player.flight
        };
        player.attitude = RigAttitude {
            orientation: quantize_angle(wrap_angle(player.attitude.orientation + angle)),
            angular_velocity: player.attitude.angular_velocity,
        };
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        player.x_remainder = 0.0;
        output.push_event(GameplayEventKind::Flight {
            what: FlightEvent::TookOff,
            player_id,
            detail: 0,
        });
        // Into free flight's store (Wave 12 systems), or parked home past its ground.
        let pos = grid.to_world(ground.centre, u, v);
        if super::flight::within_space_store(pos) {
            let _ = self.move_to_place(player_id, Place::Space, pos);
        } else {
            let _ = self.move_to_place(player_id, Place::HOME, park);
        }
    }

    /// The emergency beacon on a body: the rig is towed home (the spawn point) for the fee,
    /// with at least half a hull and half a tank.
    pub(super) fn body_beacon(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        // A home in an alien town on this body (Wave 12): the beacon tows the rig there.
        if let crate::place::StoreKey::Body(body) = self.active_store()
            && let Some((x, y)) = self.alien_home_rescue_point(player_id, body)
            && let Some(player) = self.players.get_mut(&player_id)
        {
            let fee = beacon_fee(player.credits);
            player.credits -= fee;
            player.x = x;
            player.y = y;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
            player.x_remainder = 0.0;
            player.attitude = RigAttitude::LEVEL;
            player.hull = player.hull.max(player.max_hull() * 0.5);
            player.fuel = player.fuel.max(player.fuel_capacity * 0.5);
            let capacity = player.flight.air;
            player.flight.air = capacity.max(1.0);
            output.push_event(GameplayEventKind::Flight {
                what: FlightEvent::BeaconReturn,
                player_id,
                detail: fee,
            });
            let refill = self.air_capacity_of(player_id);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.flight.air = refill;
            }
            return;
        }
        let home = self
            .home_frame()
            .world(self.player_template.x, self.player_template.y);
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let fee = beacon_fee(player.credits);
        player.credits -= fee;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        player.x_remainder = 0.0;
        player.attitude = RigAttitude::LEVEL;
        player.hull = player.hull.max(player.max_hull() * 0.5);
        player.fuel = player.fuel.max(player.fuel_capacity * 0.5);
        player.flight = RigFlight {
            exit_column: player.flight.exit_column,
            ..RigFlight::default()
        };
        output.push_event(GameplayEventKind::Flight {
            what: FlightEvent::BeaconReturn,
            player_id,
            detail: fee,
        });
        let _ = self.move_to_place(player_id, Place::HOME, home);
    }

    /// Where an away rig is parked in the home world, as an absolute home position (also from
    /// inside another store's ground pass).
    #[must_use]
    pub fn space_park_at_home(&self) -> Option<WorldPos> {
        let settings = self.home_settings();
        if !settings.generator.has_descents() {
            return None;
        }
        let moon = crate::worldgen::sky::moon(&settings)?;
        let column = crate::worldgen::sky::moon_column(&settings)?;
        let line = moon.top_row - crate::flight::SPACE_ENTRY_ROWS;
        Some(WorldPos::tile_center(
            i64::from(column),
            i64::from(line - crate::flight::PARK_ABOVE_ROWS),
        ))
    }
}
