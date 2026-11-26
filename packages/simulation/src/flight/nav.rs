//! Navigation: waypoints, the scanner ping, bearings, distances and ETAs (the nav HUD, a ship
//! upgrade: [`super::FlightGear::nav_suite`]).

use serde::{Deserialize, Serialize};

use crate::{position::WorldPos, star_chart::StarClass, terrain::TILE_SIZE};

use super::{
    BodyRef,
    physics::{
        CRUISE_MAX_SPEED, CRUISE_TAU_SECONDS, WARP_ARRIVAL, WARP_SPOOL_MILLIS, up_angle,
        warp_duration_millis,
    },
    space::{SpaceQuery, SystemRef},
};

/// What a waypoint points at.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum NavTarget {
    /// A star system (its star): a warp's target.
    System(SystemRef),
    /// One body (a world, the home world or a station).
    Body(BodyRef),
}

impl NavTarget {
    /// The system the target belongs to.
    #[must_use]
    pub const fn system(self) -> SystemRef {
        match self {
            Self::System(system) => system,
            Self::Body(body) => body.system,
        }
    }

    /// Where the target is.
    #[must_use]
    pub fn position(self, space: &dyn SpaceQuery) -> Option<WorldPos> {
        match self {
            Self::System(system) => space.system(system).map(|found| found.star),
            Self::Body(body) => space.body(body).map(|found| found.centre),
        }
    }
}

/// A scanner ping: when, where and how far it reached.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ScanPing {
    /// World millisecond of the ping.
    pub at: u64,
    pub centre: WorldPos,
    /// Reach (px).
    pub range: f32,
    /// It went out inside a nebula's fog (its reach halved).
    pub fogged: bool,
}

/// A ping's reach by nav suite tier (tiles; tier 0 is the stock scanner).
pub const PING_RANGE_TILES: [f32; 4] = [40_000.0, 120_000.0, 400_000.0, 1_200_000.0];
/// A nebula's fog cuts a ping's reach to this share.
pub const NEBULA_FOG: f32 = 0.5;
/// How far a nebula's fog reaches from its star (px).
pub const NEBULA_REACH: f32 = 220_000.0 * TILE_SIZE;
/// How long a ping shows on the HUD (ms).
pub const PING_SHOW_MILLIS: u64 = 12_000;

/// A ping's reach (px) from a rig with nav suite tier `tier`.
#[must_use]
pub fn ping_range(tier: u8, fogged: bool) -> f32 {
    let base = PING_RANGE_TILES[usize::from(tier.min(3))] * TILE_SIZE;
    if fogged { base * NEBULA_FOG } else { base }
}

/// Whether `pos` is inside a nebula's fog.
#[must_use]
pub fn in_nebula(space: &dyn SpaceQuery, pos: WorldPos) -> bool {
    // The space generator's fog (interstellar nebulae and nebula stars' clouds), or a nebula
    // star's neighbourhood.
    space.fog_at(pos) >= NEBULA_FOG_PERMILLE
        || space
            .systems_near(pos, 1)
            .iter()
            .any(|system| system.class == StarClass::Nebula && pos.span(system.star) < NEBULA_REACH)
}

/// Fog (per mille) from which the scanner's range halves.
pub const NEBULA_FOG_PERMILLE: u16 = 250;

/// The orientation that faces `to` from `from` (radians, clockwise from up).
#[must_use]
pub fn bearing(from: WorldPos, to: WorldPos) -> f32 {
    let (dx, dy) = from.delta(to);
    up_angle(dx, dy)
}

/// A rough ETA in seconds to cover `distance` px (presentation only: the HUD's readout).
///
/// Within cruise reach it models the cruise's exponential ramp up and down; farther it is
/// a warp's time plus the cruise in from the system's edge.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "an approximate readout")]
pub fn eta_seconds(distance: f64, warp: bool) -> f32 {
    let tau = f64::from(CRUISE_TAU_SECONDS);
    // The cruise leaves and arrives a body's approach sphere about 1,500 tiles from its
    // surface (a speed of gap / tau) and tops out at the cruise's maximum.
    let edge = 1_500.0 * f64::from(TILE_SIZE);
    let cruise = |distance: f64| -> f64 {
        if distance <= edge {
            return distance / (edge / tau);
        }
        let half = distance * 0.5;
        let top = f64::from(CRUISE_MAX_SPEED) * tau;
        if half <= top {
            2.0 * tau * crate::polar::trig::ln(half / edge)
        } else {
            2.0f64.mul_add(
                tau * crate::polar::trig::ln(top / edge),
                2.0f64.mul_add(-top, distance) / f64::from(CRUISE_MAX_SPEED),
            )
        }
    };
    if warp {
        let jump = f64::from(warp_duration_millis(distance) + WARP_SPOOL_MILLIS as u32) / 1_000.0;
        (jump + cruise(f64::from(WARP_ARRIVAL))) as f32
    } else {
        cruise(distance) as f32
    }
}

/// Formats `seconds` as "1m 20s" (or "45s").
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a readout"
)]
pub fn format_eta(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u32;
    if total >= 3_600 {
        format!("{}h {:02}m", total / 3_600, (total / 60) % 60)
    } else if total >= 60 {
        format!("{}m {:02}s", total / 60, total % 60)
    } else {
        format!("{total}s")
    }
}

/// Formats a distance in px as tiles, thousands of tiles or galactic sectors.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "a readout")]
pub fn format_distance(distance: f64) -> String {
    let tiles = distance / f64::from(TILE_SIZE);
    let sectors = super::physics::sectors_in(distance);
    if sectors >= 1.0 {
        format!("{sectors:.1} sectors")
    } else if tiles >= 10_000.0 {
        format!("{:.0}k tiles", tiles / 1_000.0)
    } else {
        format!("{tiles:.0} tiles")
    }
}
