//! Space flight: the Star Drive, rotation in vacuum, landing, life support, docking and nav.
//!
//! Expansion plan Part III, F12 "Flight in space" (the Wave 11 flight track): the Star Drive's
//! cruise and warp, 360 degree craft rotation in vacuum, approach, re-entry heat and landing,
//! life support, docking with stations, and the navigation HUD's state.
//!
//! # The model
//!
//! * **Leaving home.** A rig carrying the Star Drive ([`crate::WorldState::space_access`])
//!   that climbs [`SPACE_ENTRY_ROWS`] above the Moon's top enters [`FlightPlace::Space`]
//!   with no loading screen: it appears just outside the home world's approach sphere
//!   ([`space::home_launch`]), moving away from it as fast as it climbed. The Moon stays
//!   where it is (part of the home world's sky). Flying back into the home world's approach
//!   sphere hands the rig back to the home sky over the column it left from.
//! * **Space positions** are exact [`WorldPos`] in **space pixels** (32 to a tile, like the
//!   ground), never `f32` absolutes: a rig's space position lives in [`RigFlight::pos`]. While
//!   a rig is away its home-world position is parked high over the Moon
//!   ([`crate::WorldState::space_park`]), where nothing reaches it.
//! * **Rotation is the player's.** In space the rig's existing attitude
//!   ([`crate::attitude`]) loses its lean limit and its angular drag: a spin keeps going until
//!   the player counter-thrusts. A laden bay spins up and down slower ([`rotation_inertia`]),
//!   and the rotation thrusters burn fuel. Gravity ([`space::gravity_at`], blended over every
//!   nearby body) pulls the rig but never turns it.
//! * **Thrust is craft-relative** (forward and back along the rig's up axis, strafe along its
//!   right axis); [`FlightControls::screen_relative`] (an accessibility option) pushes along
//!   the chart's axes instead.
//! * **The Star Drive** ([`StarDrive`]): **cruise** follows the craft's heading at a speed
//!   that ramps with the distance to the nearest surface ([`physics::cruise_limit`]), so a
//!   star system takes a minute or two to cross and the drive slows by itself near bodies;
//!   **warp** ([`WarpRun`]) crosses to another star along a straight line as a pure function
//!   of time (seamless and time-compressed), interrupted by any gravity well on the way.
//! * **Approach, re-entry and landing** ([`physics`]): inside a body's approach sphere the
//!   view zooms in; inside its atmosphere fast or steep flight heats the hull
//!   ([`RigFlight::heat`]); touching down slow and upright lands, tilted or fast scrapes and
//!   bounces.
//! * **Life support** ([`RigFlight::air`]): seconds of air in space and on airless bodies,
//!   refilled at home, at stations and on breathable worlds; warnings at half, a quarter and a
//!   tenth; at zero (or a wrecked hull) the **emergency beacon** tows the rig home for a fee.
//!   Never an unwinnable countdown.
//! * **Docking** with stations: come within [`DOCK_RANGE`] of the port, slow, align, use.
//! * **Assists** (all off by default; upgrades plus the seat's switches): the gyro and
//!   horizon hold of [`crate::attitude`], and auto-land ([`FlightGear::auto_land`]).
//! * **Navigation** ([`nav`], a ship upgrade: [`FlightGear::nav_suite`]): known systems,
//!   waypoints (a crew waypoint for co-op), scanner pings, distances and ETAs.
//!
//! Everything is a pure function of the world seed, the per-tick state and the inputs,
//! integrated in fixed internal steps ([`physics::FLIGHT_STEP_SECONDS`]) so every tick rate
//! agrees.
//!
//! # Seams (Wave 11 integration)
//!
//! * Regions (wired): a rig in free flight is `place::Place::Space` (its home-store slot parked
//!   over the Moon, its exact space position in [`RigFlight::pos`]); [`BodyRef`] names any
//!   flight body (suns, bodies, stations) and maps to the generator's body id, which **is**
//!   `place::BodyId::Space`'s payload ([`space::GeneratedSpace::space_body`] /
//!   [`space::GeneratedSpace::body_ref`]). Re-entry onto a generated body hands the rig to its
//!   store with `move_to_place(Place::OnBody(..))` (`world/body_flight.rs`), and climbing out
//!   of its strip hands it back.
//! * Space generator (wired): [`space::GeneratedSpace`] reads `crate::space::Space` (one
//!   shared cache per world seed); [`space::SpaceQuery`] is the interface this module reads.
//! * Polar (wired): [`FlightPlace::Landed`] is a rig on a body's round ground, moved by the
//!   ground code in the body's store; the circle landing in [`physics`] stays for bodies
//!   without ground (none today).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ids::PlayerId, position::WorldPos, terrain::TILE_SIZE};

#[doc(hidden)]
pub mod autopilot;
pub mod nav;
pub mod physics;
pub mod space;
pub mod systems;
#[cfg(test)]
mod tests;

pub use space::{BodyKind, FlightBody, GeneratedSpace, SpaceQuery, SpaceSystem, SystemRef};

// --- Leaving and returning ------------------------------------------------------------------

/// Rows above the Moon's top where space begins for a rig with the Star Drive.
pub const SPACE_ENTRY_ROWS: i32 = 400;
/// A rig parked while it is away hangs this many rows above the space line.
pub const PARK_ABOVE_ROWS: i32 = 40;
/// A rig handed back to the home sky appears this many rows under the space line.
pub const RETURN_BELOW_ROWS: i32 = 60;

// --- Thrust and spin -------------------------------------------------------------------------

/// Main thruster acceleration in space at engine tier 1 (px/s², scaled by the engine power).
pub const SPACE_THRUST: f32 = 520.0;
/// Side thrusters push this share of the main thruster.
pub const STRAFE_SHARE: f32 = 0.6;
/// Retro thrusters push this share of the main thruster.
pub const REVERSE_SHARE: f32 = 0.7;
/// Top impulse speed at engine tier 1 (px/s, about 40 tiles a second).
pub const IMPULSE_TOP_SPEED: f32 = 1_300.0;
/// Extra impulse top speed per unit of engine power past the first.
pub const IMPULSE_SPEED_PER_POWER: f32 = 350.0;
/// How fast a rig over its impulse top speed (out of cruise) sheds the excess (px/s²).
pub const IMPULSE_DECELERATION: f32 = 2_400.0;
/// Fuel a second at full main thrust in space.
pub const SPACE_FUEL_PER_SECOND: f32 = 4.0;
/// Fuel a second at full rotation thrust, as a share of [`SPACE_FUEL_PER_SECOND`].
pub const ROTATION_FUEL_SHARE: f32 = 0.25;
/// Fastest spin the rotation thrusters push a rig to in space (rad/s, 200 degrees a second).
pub const MAX_SPACE_SPIN: f32 = 3.5;
/// A full cargo bay's extra rotational inertia (a full bay spins up half as fast).
pub const CARGO_INERTIA: f32 = 1.0;

// --- Re-entry heat ---------------------------------------------------------------------------

/// Speed through air (radial, plus a share of tangential) a hull takes without heating (px/s).
pub const REENTRY_SAFE_SPEED: f32 = 450.0;
/// Share of tangential speed that heats (a shallow glide heats far less than a dive).
pub const TANGENTIAL_HEAT_SHARE: f32 = 0.3;
/// Heat gained a second per px/s over the safe speed, at the bottom of the air.
pub const HEAT_GAIN: f32 = 1.0 / 1_100.0;
/// Heat shed a second when nothing heats the hull.
pub const HEAT_COOL_PER_SECOND: f32 = 0.3;
/// Heat at which the hull starts to burn (1) and the most it holds.
pub const HEAT_BURN: f32 = 1.0;
pub const HEAT_MAX: f32 = 1.5;
/// Hull lost a second per unit of heat over [`HEAT_BURN`] (at [`HEAT_MAX`], 15 a second).
pub const HEAT_HULL_DAMAGE: f32 = 30.0;
/// The re-entry warning shows over this much heat.
pub const HEAT_WARNING: f32 = 0.6;
/// Each heat shield tier raises the safe speed by this share and divides the gain.
pub const SHIELD_SAFE_SHARE: f32 = 0.6;

// --- Landing ---------------------------------------------------------------------------------

/// Fastest touchdown (into the ground) that lands without damage (px/s, as on the home world).
pub const LANDING_SAFE_SPEED: f32 = 330.0;
/// Most tilt from the local up a touchdown lands with (radians, about 29 degrees).
pub const LANDING_TILT: f32 = 0.5;
/// Hull lost per px/s of a touchdown over the safe speed.
pub const CRASH_DAMAGE_SCALE: f32 = 0.11;
/// Hull a tilted touchdown scrapes off, per 100 px/s of impact.
pub const SCRAPE_DAMAGE: f32 = 6.0;
/// How much of the speed into the ground a bad touchdown gives back.
pub const BOUNCE_KEEP: f32 = 0.25;
/// Spin a tilted touchdown tips the rig with (rad/s, toward its lean).
pub const TIP_SPIN: f32 = 1.2;
/// A rig's half height over a surface (px).
pub const RIG_CONTACT: f32 = 14.0;

// --- Life support ----------------------------------------------------------------------------

/// Seconds of air with no life support upgrade, and per tier of [`crate::protection::
/// HazardGear::life_support`].
pub const AIR_BASE_SECONDS: f32 = 360.0;
pub const AIR_PER_TIER_SECONDS: f32 = 360.0;
/// Air a second refilled on a breathable world.
pub const AIR_REFILL_PER_SECOND: f32 = 30.0;
/// Shares of the tank the warnings fire at (percent).
pub const AIR_WARNINGS: [u8; 3] = [50, 25, 10];
/// The emergency beacon's fee: a base plus a share of the credits (never more than held).
pub const BEACON_FEE_BASE: u32 = 120;
pub const BEACON_FEE_PERCENT: u32 = 5;

// --- Docking ---------------------------------------------------------------------------------

/// How near a station's port a rig docks (px, 3 tiles).
pub const DOCK_RANGE: f32 = 3.0 * TILE_SIZE;
/// Fastest a rig may move and still dock (px/s).
pub const DOCK_SPEED: f32 = 70.0;
/// Most a rig may lean off the port's axis and still dock (radians, 15 degrees).
pub const DOCK_ALIGN: f32 = 0.26;
/// Speed an undocking rig backs off at (px/s).
pub const UNDOCK_SPEED: f32 = 60.0;

// --- Prices at a station (Wave 12 fills the station; these are the stub's) ------------------

/// Credits a unit of fuel at a station.
pub const STATION_FUEL_PRICE: u32 = 2;
/// Credits a point of hull at a station.
pub const STATION_REPAIR_PRICE: u32 = 4;

/// Where a rig is in the flight model.
///
/// As a `place::Place`: `Home` is `OnBody(Home)` (or the Moon), `Space`, `Approach` and
/// `Docked` are `Space`, and `Landed` is `OnBody(Space(..))`, the body's store
/// (`WorldState::place_of`).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FlightPlace {
    /// On or over the home world (its ground and sky, the Moon included): classic movement.
    #[default]
    Home,
    /// Free flight between bodies.
    Space,
    /// Inside a body's approach sphere (the view zooms in; re-entry heat in its air).
    Approach(BodyRef),
    /// On a generated body's round ground (its store, [`crate::place::Place::OnBody`]): the
    /// ground code moves the rig there (Wave 11 integration, `world/body_flight.rs`).
    Landed(BodyRef),
    /// Clamped to a station's port.
    Docked(BodyRef),
}

impl FlightPlace {
    /// Whether the rig is flying in space (flight replaces its classic movement). A rig
    /// [`Self::Landed`] on a body's ground is not: it drives and digs there with the ground
    /// code, in the body's store (Wave 11 integration).
    #[must_use]
    pub const fn away(self) -> bool {
        !matches!(self, Self::Home | Self::Landed(_))
    }

    /// The body the rig is at, if any.
    #[must_use]
    pub const fn body(self) -> Option<BodyRef> {
        match self {
            Self::Home | Self::Space => None,
            Self::Approach(body) | Self::Landed(body) | Self::Docked(body) => Some(body),
        }
    }
}

/// A body: a star, planet, the home world or a station of one system.
///
/// A generated body's `BodyRef` maps to the space generator's id, the payload of
/// `place::BodyId::Space` ([`space::GeneratedSpace::space_body`]); the home world is
/// [`space::HOME_BODY`] (`place::BodyId::Home`).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct BodyRef {
    pub system: SystemRef,
    /// 0 the star, 1.. the worlds in orbit order, [`space::STATION_INDEX`] the station.
    pub index: u8,
}

/// One rig's flight state: per tick, predicted and replicated with its pose.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RigFlight {
    pub place: FlightPlace,
    /// The rig's space position (meaningful while [`FlightPlace::away`]).
    pub pos: WorldPos,
    /// Velocity in space, px/s.
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub drive: StarDrive,
    /// Hull heat from re-entry or a star's corona (0 cold, [`HEAT_BURN`] burning).
    pub heat: f32,
    /// Seconds of air left (full while at home).
    pub air: f32,
    /// The home column the rig left from (it comes home over it).
    pub exit_column: i32,
    /// Warnings already raised on this tank of air (bits: [`AIR_WARNINGS`] in order, then
    /// heat; Wave 12 systems: bits 4-6 a black hole's [`systems::HoleWarning`]s, bit 7 a
    /// flare's warning).
    pub alerts: u8,
    /// Seconds the rig's own clock has fallen behind the crew's near black holes (Wave 12
    /// systems: time dilation; the HUD shows it).
    #[serde(default)]
    pub lag_seconds: f32,
    /// Progress (0-1) of the unit of ore being drilled or beamed out of an asteroid (Wave 12).
    #[serde(default)]
    pub mining: f32,
    /// Progress (0-1) of the unit of dust being gathered in a comet's tail (Wave 12).
    #[serde(default)]
    pub gathering: f32,
    /// Wave 12 systems' edge marks: bit 0 inside a nebula, bit 1 the parking hold holds
    /// ([`physics::MARK_HOLDING`]), bit 2 a full hold announced, bit 3 a star's radiance
    /// announced.
    #[serde(default)]
    pub marks: u8,
}

impl RigFlight {
    /// Whether the rig is away from the home world.
    #[must_use]
    pub const fn away(&self) -> bool {
        self.place.away()
    }

    /// Speed in space (px/s).
    #[must_use]
    pub fn speed(&self) -> f32 {
        crate::field::trig::hypot(self.velocity_x, self.velocity_y)
    }
}

/// The Star Drive's mode.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub enum StarDrive {
    /// Impulse flight on the thrusters.
    #[default]
    Off,
    /// Cruise along the heading at `speed` px/s.
    Cruise { speed: f32 },
    /// Charging a warp to `target` since world millisecond `since` (the craft coasts and must
    /// stay aligned).
    Spool { target: SystemRef, since: u64 },
    /// In warp.
    Warp(WarpRun),
    /// Through a wormhole (Wave 12 systems): a short tunnel crossing to the paired mouth as a
    /// pure function of time, like the warp.
    Transit(TransitRun),
}

/// A wormhole transit: the run to the far mouth and the heading the rig leaves it along.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TransitRun {
    pub run: WarpRun,
    /// Unit heading out of the far mouth (the one the rig came in with).
    pub heading_x: f32,
    pub heading_y: f32,
}

impl StarDrive {
    /// Whether the drive is cruising or warping (the thrusters are off).
    #[must_use]
    pub const fn engaged(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// A warp: a straight line from `origin` to `arrival`, crossed as a pure function of time.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WarpRun {
    pub origin: WorldPos,
    pub arrival: WorldPos,
    pub target: SystemRef,
    /// World millisecond the warp left.
    pub start: u64,
    pub duration_millis: u32,
}

/// Flight gear fitted to a rig (private loadout, in [`crate::protection::HazardGear`]). Sold at
/// stations ([`StationService::Buy`]); test mode fits everything.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FlightGear {
    /// The navigation suite (0 none: no star chart HUD; 1-3 widen the scanner).
    #[serde(default)]
    pub nav_suite: u8,
    /// Re-entry heat shielding tiers (0-3).
    #[serde(default)]
    pub reentry_shield: u8,
    /// Auto-land fitted (0 or 1); it works only while the seat switches it on.
    #[serde(default)]
    pub auto_land: u8,
    /// The Mining Beam fitted (0 or 1; Wave 12 systems): the trigger mines asteroids from the
    /// nose ([`systems::beam_target`]).
    #[serde(default)]
    pub mining_beam: u8,
}

impl FlightGear {
    /// Everything fitted (test mode).
    pub const MAXED: Self = Self {
        nav_suite: NAV_SUITE_MAX,
        reentry_shield: REENTRY_SHIELD_MAX,
        auto_land: 1,
        mining_beam: 1,
    };
}

/// Top nav suite tier.
pub const NAV_SUITE_MAX: u8 = 3;
/// Top re-entry shield tier.
pub const REENTRY_SHIELD_MAX: u8 = 3;

/// Flight gear a station sells.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FlightGearKind {
    NavSuite,
    ReentryShield,
    Gyro,
    HorizonHold,
    AutoLand,
    /// Wave 12 systems: mines asteroids from the nose.
    MiningBeam,
}

impl FlightGearKind {
    pub const ALL: [Self; 6] = [
        Self::NavSuite,
        Self::ReentryShield,
        Self::Gyro,
        Self::HorizonHold,
        Self::AutoLand,
        Self::MiningBeam,
    ];

    /// Shop name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NavSuite => "Nav Suite",
            Self::ReentryShield => "Re-entry Shield",
            Self::Gyro => "Gyro Stabiliser",
            Self::HorizonHold => "Horizon Hold",
            Self::AutoLand => "Auto-Land",
            Self::MiningBeam => "Mining Beam",
        }
    }

    /// Top tier.
    #[must_use]
    pub const fn max_level(self) -> u8 {
        match self {
            Self::NavSuite => NAV_SUITE_MAX,
            Self::ReentryShield => REENTRY_SHIELD_MAX,
            Self::Gyro | Self::HorizonHold | Self::AutoLand | Self::MiningBeam => 1,
        }
    }

    /// The tier fitted to `gear`.
    #[must_use]
    pub const fn level(self, gear: &crate::protection::HazardGear) -> u8 {
        match self {
            Self::NavSuite => gear.flight.nav_suite,
            Self::ReentryShield => gear.flight.reentry_shield,
            Self::Gyro => gear.attitude.gyro_stabiliser,
            Self::HorizonHold => gear.attitude.horizon_hold,
            Self::AutoLand => gear.flight.auto_land,
            Self::MiningBeam => gear.flight.mining_beam,
        }
    }

    /// Credits for the next tier after `level`.
    #[must_use]
    pub const fn price(self, level: u8) -> u32 {
        let base = match self {
            Self::NavSuite => 2_500,
            Self::ReentryShield => 3_000,
            Self::Gyro => 1_800,
            Self::HorizonHold => 2_200,
            Self::AutoLand => 2_800,
            Self::MiningBeam => 3_400,
        };
        base * (level as u32 + 1)
    }
}

/// The seat's flight switches for one tick (bits in [`crate::ControlState`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FlightControls {
    /// Auto-land on (works only when fitted).
    pub auto_land: bool,
    /// Accessibility: thrust along the chart's axes instead of the craft's (the seat's camera
    /// stays chart-up too).
    pub screen_relative: bool,
}

impl FlightControls {
    /// As bits (wire form).
    #[must_use]
    pub const fn bits(self) -> u8 {
        (self.auto_land as u8) | ((self.screen_relative as u8) << 1)
    }

    /// From [`Self::bits`].
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            auto_land: bits & 1 != 0,
            screen_relative: bits & 2 != 0,
        }
    }
}

/// A flight action ([`crate::PlayerCommand::Flight`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FlightCommand {
    /// Engage cruise, or drop back to the thrusters (also aborts a spooling warp).
    ToggleCruise,
    /// Spool a warp to the waypoint's system (aligned, clear of gravity wells, fuel aboard).
    Warp,
    /// The use button in space: dock at a port in reach, or undock.
    Use,
    /// The emergency beacon: towed home for a fee.
    Beacon,
    /// Set (or clear) the player's own waypoint.
    SetWaypoint(Option<nav::NavTarget>),
    /// Share the player's waypoint with the crew (or clear the crew's).
    ShareWaypoint,
    /// A docked station's service.
    Station(StationService),
}

/// A station's services (the stub Wave 12 fills).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StationService {
    Refuel,
    Repair,
    Buy(FlightGearKind),
}

/// What happened in flight (netcode `800 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FlightEvent {
    EnteredSpace,
    ReturnedHome,
    CruiseEngaged,
    /// Cruise dropped out near a body (or was switched off).
    CruiseDropped,
    WarpSpooling,
    WarpEngaged,
    WarpArrived,
    /// A gravity well or a turn broke the warp.
    WarpInterrupted,
    /// Warp refused (see [`WarpRefusal`] in the detail).
    WarpRefused,
    ApproachBody,
    Landed,
    TookOff,
    /// Hull heating (re-entry or a star).
    HeatWarning,
    /// A touchdown scraped the hull or a crash dented it.
    HullScraped,
    /// Air low (detail: percent left).
    AirLow,
    BeaconReturn,
    Docked,
    Undocked,
    ScannerPing,
    /// A station service or a waypoint change went through.
    NavUpdated,
}

impl FlightEvent {
    pub const ALL: [Self; 20] = [
        Self::EnteredSpace,
        Self::ReturnedHome,
        Self::CruiseEngaged,
        Self::CruiseDropped,
        Self::WarpSpooling,
        Self::WarpEngaged,
        Self::WarpArrived,
        Self::WarpInterrupted,
        Self::WarpRefused,
        Self::ApproachBody,
        Self::Landed,
        Self::TookOff,
        Self::HeatWarning,
        Self::HullScraped,
        Self::AirLow,
        Self::BeaconReturn,
        Self::Docked,
        Self::Undocked,
        Self::ScannerPing,
        Self::NavUpdated,
    ];

    /// Wire code offset (netcode adds 800; 800-819 is this track's range).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// A short HUD line.
    #[must_use]
    pub const fn headline(self) -> &'static str {
        match self {
            Self::EnteredSpace => "OPEN SPACE",
            Self::ReturnedHome => "HOME SKY",
            Self::CruiseEngaged => "CRUISE ENGAGED",
            Self::CruiseDropped => "CRUISE DISENGAGED",
            Self::WarpSpooling => "WARP SPOOLING",
            Self::WarpEngaged => "WARP",
            Self::WarpArrived => "WARP COMPLETE",
            Self::WarpInterrupted => "WARP INTERRUPTED",
            Self::WarpRefused => "WARP UNAVAILABLE",
            Self::ApproachBody => "APPROACH",
            Self::Landed => "TOUCHDOWN",
            Self::TookOff => "LIFT-OFF",
            Self::HeatWarning => "HULL HEATING",
            Self::HullScraped => "HARD LANDING",
            Self::AirLow => "AIR LOW",
            Self::BeaconReturn => "EMERGENCY BEACON: TOWED HOME",
            Self::Docked => "DOCKED",
            Self::Undocked => "UNDOCKED",
            Self::ScannerPing => "SCANNER PING",
            Self::NavUpdated => "NAV UPDATED",
        }
    }
}

/// Why a warp was refused (the event's detail).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WarpRefusal {
    /// No waypoint, or the waypoint is not another star system.
    NoTarget,
    /// The heading is off the target's bearing.
    NotAligned,
    /// A body's gravity well is too near.
    GravityWell,
    /// Not enough fuel.
    Fuel,
    /// Not in free flight (docked, landed or at home).
    NotFree,
}

impl WarpRefusal {
    pub const ALL: [Self; 5] = [
        Self::NoTarget,
        Self::NotAligned,
        Self::GravityWell,
        Self::Fuel,
        Self::NotFree,
    ];

    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::NoTarget),
            1 => Some(Self::NotAligned),
            2 => Some(Self::GravityWell),
            3 => Some(Self::Fuel),
            4 => Some(Self::NotFree),
            _ => None,
        }
    }

    /// A HUD line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NoTarget => "set a star system as your waypoint",
            Self::NotAligned => "turn to face the waypoint",
            Self::GravityWell => "too close to a gravity well",
            Self::Fuel => "not enough fuel",
            Self::NotFree => "undock or take off first",
        }
    }
}

/// The crew's shared flight state (a world field, the shared `Flight` section): waypoints,
/// systems found, the last scanner ping of each player.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FlightShared {
    /// Each player's own waypoint.
    #[serde(default, with = "crate::pair_list")]
    pub waypoints: BTreeMap<PlayerId, nav::NavTarget>,
    /// The crew's shared waypoint (co-op).
    #[serde(default)]
    pub crew_waypoint: Option<nav::NavTarget>,
    /// Systems the crew found by flying or scanning (the Archive's charts are known too, see
    /// [`crate::WorldState::known_systems`]), in the order found.
    #[serde(default)]
    pub found_systems: Vec<SystemRef>,
    /// Each player's last scanner ping.
    #[serde(default, with = "crate::pair_list")]
    pub pings: BTreeMap<PlayerId, nav::ScanPing>,
}

/// Most systems the crew's found list keeps.
pub const MAX_FOUND_SYSTEMS: usize = 1_024;

impl FlightShared {
    /// Whether nothing was ever recorded (the state hash skips it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.waypoints.is_empty()
            && self.crew_waypoint.is_none()
            && self.found_systems.is_empty()
            && self.pings.is_empty()
    }

    /// Records `system` as found (no repeats). Returns whether it was new.
    pub fn found(&mut self, system: SystemRef) -> bool {
        if self.found_systems.contains(&system) || self.found_systems.len() >= MAX_FOUND_SYSTEMS {
            return false;
        }
        self.found_systems.push(system);
        true
    }

    /// Forgets a player who left.
    pub fn forget(&mut self, player: PlayerId) {
        self.waypoints.remove(&player);
        self.pings.remove(&player);
    }

    /// `player`'s waypoint: their own, else the crew's.
    #[must_use]
    pub fn waypoint_of(&self, player: PlayerId) -> Option<nav::NavTarget> {
        self.waypoints.get(&player).copied().or(self.crew_waypoint)
    }
}

/// Rotational inertia of a rig with `cargo_ratio` (0-1) of its bay full.
#[must_use]
pub const fn rotation_inertia(cargo_ratio: f32) -> f32 {
    cargo_ratio.clamp(0.0, 1.0).mul_add(CARGO_INERTIA, 1.0)
}

/// Seconds of air a rig with life support tier `tier` carries.
#[must_use]
pub fn air_capacity(tier: u8) -> f32 {
    f32::from(tier.min(3)).mul_add(AIR_PER_TIER_SECONDS, AIR_BASE_SECONDS)
}

/// The emergency beacon's fee for a rig holding `credits`.
#[must_use]
pub const fn beacon_fee(credits: u32) -> u32 {
    let fee = BEACON_FEE_BASE.saturating_add(credits / 100 * BEACON_FEE_PERCENT);
    if fee < credits { fee } else { credits }
}
