//! What the things in space do (Wave 12 systems, expansion plan §16 "Star systems").
//!
//! Wave 11 drew stars, black holes, wormholes, nebulae, belts and comets; this module gives
//! them physics. Everything here is a pure function of the flight body list (itself a pure
//! function of the world seed), the rig's exact position and the world clock, so the host and
//! every predicting client agree bit for bit at any tick rate:
//!
//! * **Stars** ([`StarFacts`]): radiant heat by class that grows as the square of closeness
//!   past the star's heat radius ([`radiant_heat`]), light and a glare for the screen
//!   ([`star_glare`], blue stars blind the most), and **flares** ([`flare_at`]): a pure
//!   function of the world clock and the star's period and phase, telegraphed
//!   [`FLARE_WARNING_MILLIS`] ahead, dodged by flying out of [`FLARE_REACH_HEAT_RADII`] or by
//!   sheltering behind a body ([`shadowed`]). Stars can never be landed on (their surface
//!   burns and pushes back, [`super::physics`]).
//! * **Black holes** ([`HoleFacts`]): a real gravity well blended with every other source
//!   (it accelerates, never turns), a local **time scale** ([`crate::space::Space::
//!   time_scale_at`]) that slows the rig's own clock (air, the drift between the crew's clocks
//!   shown on the HUD: [`super::RigFlight::lag_seconds`]), three warnings ([`HoleWarning`])
//!   and an event horizon that is a fair rescue: the beacon tows the rig out, the hold lost.
//! * **Wormholes** ([`MouthFacts`]): flying into a mouth starts a short transit
//!   ([`super::StarDrive::Transit`], a closed form of time like the warp) that really moves the
//!   rig to the paired mouth ([`transit_exit`]); the crew's nav knows a pair once flown.
//! * **Nebulae**: sensor fog cuts the scanner ([`fog_range_share`]) and fades nav markers
//!   ([`marker_visibility`]).
//! * **Asteroids** ([`RockFacts`]) are solid and mineable from flight: the drill bites when
//!   the rig's underside rests against a rock ([`drill_contact`]), the Mining Beam
//!   ([`super::FlightGearKind::MiningBeam`]) reaches from the nose ([`beam_target`]); each
//!   rock yields its generator's units of its ore ([`asteroid_mineral`]) and stays depleted
//!   for the crew ([`SystemsShared::mined`]).
//! * **Comets** ([`CometFacts`]): flying through a tail gathers a rare resource
//!   ([`in_comet_tail`], [`COMET_MINERAL`]), a few units per comet.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    field::trig::hypot,
    position::WorldPos,
    space::{AsteroidId, AsteroidOre},
    terrain::{MineralKind, TILE_SIZE},
};

use super::{BodyKind, FlightBody, SystemRef};

// --- Stars ----------------------------------------------------------------------------------

/// Radiant heat a second at a star's heat radius (unshielded).
///
/// It falls with the square of the distance: at twice the heat radius a quarter, under the hull's cooling (so a rig can hang
/// there); inside it the hull heats past burning in a second or two.
pub const RADIANT_HEAT: f32 = 1.1;
/// Radiant heat stops counting beyond this many heat radii (a sum over far stars is noise).
pub const RADIANT_REACH_HEAT_RADII: f32 = 6.0;
/// A flare reaches this many heat radii from its star.
pub const FLARE_REACH_HEAT_RADII: f32 = 3.5;
/// A flare's heat a second at the heat radius at its peak (unshielded; falls with the square).
pub const FLARE_HEAT: f32 = 6.0;
/// How long before a flare the HUD warns (ms).
pub const FLARE_WARNING_MILLIS: u64 = 15_000;
/// How long a flare burns (ms), easing in and out over [`FLARE_EASE_MILLIS`].
pub const FLARE_BURST_MILLIS: u64 = 6_000;
pub const FLARE_EASE_MILLIS: u64 = 1_200;
/// Glare: a star of light 1,000 (the home sun) glares fully within this many radii.
pub const GLARE_RADII: f32 = 5.0;

/// A star's physics (in [`FlightBody::extra`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StarFacts {
    /// Inside this (px from the centre) radiant heat burns any hull.
    pub heat_radius: f32,
    /// Light, per mille of the home sun's.
    pub light: u16,
    /// Seconds between flares (0: never flares).
    pub flare_period_s: u32,
    /// The flares' phase (s): a flare starts when `time % period == phase`.
    pub flare_phase_s: u32,
}

/// Where a star is in its flare cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlarePhase {
    /// Calm; the next flare in `next_millis` (`u64::MAX` for a star that never flares).
    Quiet { next_millis: u64 },
    /// A flare is coming in `in_millis` (inside [`FLARE_WARNING_MILLIS`]).
    Warning { in_millis: u64 },
    /// Burning: `strength` 0-1 (eased in and out), `left_millis` to go.
    Burst { strength: f32, left_millis: u64 },
}

impl FlarePhase {
    /// The flare's strength now (0 unless bursting).
    #[must_use]
    pub const fn strength(self) -> f32 {
        match self {
            Self::Burst { strength, .. } => strength,
            Self::Quiet { .. } | Self::Warning { .. } => 0.0,
        }
    }
}

/// The flare cycle of `star` at world millisecond `now`.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a few seconds of milliseconds")]
pub fn flare_at(star: &StarFacts, now: u64) -> FlarePhase {
    if star.flare_period_s == 0 {
        return FlarePhase::Quiet {
            next_millis: u64::MAX,
        };
    }
    let period = u64::from(star.flare_period_s) * 1_000;
    let phase = (u64::from(star.flare_phase_s) * 1_000) % period;
    // Milliseconds since the last flare began.
    let since = (now % period + period - phase) % period;
    if since < FLARE_BURST_MILLIS {
        let left = FLARE_BURST_MILLIS - since;
        let edge = since.min(left).min(FLARE_EASE_MILLIS);
        let strength = edge as f32 / FLARE_EASE_MILLIS as f32;
        return FlarePhase::Burst {
            strength: strength * strength * 2.0f32.mul_add(-strength, 3.0),
            left_millis: left,
        };
    }
    let next = period - since;
    if next <= FLARE_WARNING_MILLIS {
        FlarePhase::Warning { in_millis: next }
    } else {
        FlarePhase::Quiet { next_millis: next }
    }
}

/// Steady radiant heat a second at `pos` from every star in `bodies` (unshielded).
#[must_use]
pub fn radiant_heat(bodies: &[FlightBody], pos: WorldPos) -> f32 {
    let mut rate = 0.0;
    for body in bodies {
        let BodyExtra::Star(star) = body.extra else {
            continue;
        };
        if star.heat_radius <= 0.0 {
            continue;
        }
        let distance = pos.span(body.centre).max(body.radius);
        if distance > star.heat_radius * RADIANT_REACH_HEAT_RADII {
            continue;
        }
        let ratio = star.heat_radius / distance;
        rate = (RADIANT_HEAT * light_share(star.light) * ratio).mul_add(ratio, rate);
    }
    rate
}

/// A star's light as a heat factor (the home sun's is 1; blue stars run hot, red dwarfs cool).
fn light_share(light: u16) -> f32 {
    (f32::from(light) / 1_000.0).clamp(0.5, 2.0)
}

/// Flare heat a second at `pos` at world millisecond `now` (unshielded; 0 in a body's shadow),
/// and the strongest flare phase of a star whose reach covers `pos` (for the HUD).
#[must_use]
pub fn flare_heat(bodies: &[FlightBody], pos: WorldPos, now: u64) -> (f32, Option<FlareNear>) {
    let mut rate = 0.0;
    let mut nearest: Option<FlareNear> = None;
    for (index, body) in bodies.iter().enumerate() {
        let BodyExtra::Star(star) = body.extra else {
            continue;
        };
        if star.flare_period_s == 0 || star.heat_radius <= 0.0 {
            continue;
        }
        let distance = pos.span(body.centre).max(body.radius);
        if distance > star.heat_radius * FLARE_REACH_HEAT_RADII {
            continue;
        }
        let phase = flare_at(&star, now);
        let sheltered = shadowed(bodies, body.centre, pos, index);
        let ratio = star.heat_radius / distance;
        if !sheltered {
            rate = (FLARE_HEAT * phase.strength() * light_share(star.light) * ratio)
                .mul_add(ratio, rate);
        }
        let near = FlareNear {
            star: index,
            phase,
            sheltered,
        };
        if nearest.is_none_or(|best| near.sooner_than(&best)) {
            nearest = Some(near);
        }
    }
    (rate, nearest)
}

/// A flaring star whose reach covers the rig (the HUD's forecast).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlareNear {
    /// The star's index in the body list.
    pub star: usize,
    pub phase: FlarePhase,
    /// A body stands between the star and the rig.
    pub sheltered: bool,
}

impl FlareNear {
    const fn urgency(&self) -> u64 {
        match self.phase {
            FlarePhase::Burst { .. } => 0,
            FlarePhase::Warning { in_millis } => in_millis,
            FlarePhase::Quiet { next_millis } => next_millis,
        }
    }

    const fn sooner_than(&self, other: &Self) -> bool {
        self.urgency() < other.urgency()
    }
}

/// Whether a body of `bodies` (not `star` itself; worlds, stations and rocks) stands between the
/// star at `light` and `pos`: the segment passes within its radius.
#[must_use]
pub fn shadowed(bodies: &[FlightBody], light: WorldPos, pos: WorldPos, star: usize) -> bool {
    let (lx, ly) = light.delta_f64(pos);
    let length2 = lx.mul_add(lx, ly * ly);
    if length2 <= 0.0 {
        return false;
    }
    bodies.iter().enumerate().any(|(index, body)| {
        if index == star
            || !matches!(
                body.kind,
                BodyKind::Planet | BodyKind::Home | BodyKind::Station | BodyKind::Asteroid
            )
        {
            return false;
        }
        let (bx, by) = light.delta_f64(body.centre);
        let along = bx.mul_add(lx, by * ly) / length2;
        if !(0.0..1.0).contains(&along) {
            return false;
        }
        let (px, py) = (along.mul_add(-lx, bx), along.mul_add(-ly, by));
        let radius = f64::from(body.radius);
        px.mul_add(px, py * py) < radius * radius
    })
}

/// How hard the stars glare at `pos` (0-1).
///
/// The renderer's wash, scaled by the screen effects setting. Light over the home sun's (blue and white stars) glares from farther; a flare
/// doubles it while it burns.
#[must_use]
pub fn star_glare(bodies: &[FlightBody], pos: WorldPos, now: u64) -> f32 {
    let mut glare = 0.0_f32;
    for (index, body) in bodies.iter().enumerate() {
        let BodyExtra::Star(star) = body.extra else {
            continue;
        };
        let distance = pos.span(body.centre).max(body.radius);
        let reach = body.radius * GLARE_RADII * light_share(star.light);
        if distance > reach * 4.0 || shadowed(bodies, body.centre, pos, index) {
            continue;
        }
        let ratio = reach / distance;
        let flare = 1.0 + flare_at(&star, now).strength();
        glare = (ratio * ratio * flare).mul_add(0.5, glare);
    }
    glare.clamp(0.0, 1.0)
}

// --- Black holes ----------------------------------------------------------------------------

/// A black hole's physics (in [`FlightBody::extra`]; its radius is the event horizon).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoleFacts {
    /// The lensing (and time dilation) reach, px from the centre.
    pub lensing: f32,
    /// Time scale at the horizon (0-1).
    pub time_dilation: f32,
}

/// How deep in a black hole's well a rig is (the HUD's warning and the edge events).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HoleWarning {
    /// Inside the lensing: time runs slow, the pull is real.
    Well,
    /// The pull is past [`HOLE_STRAIN_SHARE`] of the rig's best thrust: leave now.
    Strain,
    /// The pull beats the rig's best thrust: only the beacon gets it out before the horizon.
    NoReturn,
}

impl HoleWarning {
    /// Its bit in [`super::RigFlight::alerts`] (bits 4-6; re-armed when the rig climbs out).
    #[must_use]
    pub const fn alert_bit(self) -> u8 {
        match self {
            Self::Well => 1 << 4,
            Self::Strain => 1 << 5,
            Self::NoReturn => 1 << 6,
        }
    }

    /// A HUD line.
    #[must_use]
    pub const fn line(self) -> &'static str {
        match self {
            Self::Well => "GRAVITY WELL - time runs slow here",
            Self::Strain => "ESCAPE THRUST FAILING - burn away now",
            Self::NoReturn => "PAST THE POINT OF NO RETURN - call the beacon",
        }
    }
}

/// Share of the rig's best thrust a hole's pull reaches before [`HoleWarning::Strain`].
pub const HOLE_STRAIN_SHARE: f32 = 0.55;

/// The deepest hole warning at `pos` for a rig whose main thruster pushes `thrust` px/s², and
/// that hole's index in `bodies`.
#[must_use]
pub fn hole_warning(
    bodies: &[FlightBody],
    pos: WorldPos,
    thrust: f32,
) -> Option<(HoleWarning, usize)> {
    let mut deepest: Option<(HoleWarning, usize)> = None;
    for (index, body) in bodies.iter().enumerate() {
        let BodyExtra::Hole(hole) = body.extra else {
            continue;
        };
        let distance = pos.span(body.centre);
        if distance >= hole.lensing {
            continue;
        }
        let pull = body.gravity_at(distance);
        let level = if pull >= thrust {
            HoleWarning::NoReturn
        } else if pull >= thrust * HOLE_STRAIN_SHARE {
            HoleWarning::Strain
        } else {
            HoleWarning::Well
        };
        if deepest.is_none_or(|(best, _)| level > best) {
            deepest = Some((level, index));
        }
    }
    deepest
}

/// The distance (px from the centre) where a hole's pull matches `thrust`: the point of no
/// return for that rig (the HUD draws it as a ring).
#[must_use]
pub fn no_return_radius(hole: &FlightBody, thrust: f32) -> f32 {
    if thrust <= 0.0 || hole.surface_gravity <= 0.0 {
        return hole.radius;
    }
    (hole.radius * (hole.surface_gravity / thrust).sqrt()).max(hole.radius)
}

// --- Wormholes -----------------------------------------------------------------------------

/// How long a wormhole transit takes (ms): a short tunnel, really crossing the distance.
pub const TRANSIT_MILLIS: u32 = 3_200;
/// A transit drops out this far past the far mouth's rim (tiles), along the heading it came in
/// with, so it never falls straight back in.
pub const TRANSIT_EXIT_TILES: f32 = 240.0;
/// Speed a rig leaves a transit with (px/s), along its entry heading.
pub const TRANSIT_EXIT_SPEED: f32 = 400.0;

/// A wormhole mouth's physics (in [`FlightBody::extra`]; its radius is the mouth's).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouthFacts {
    /// The far mouth's centre and radius (px).
    pub exit: WorldPos,
    pub exit_radius: f32,
    /// This mouth's galactic sector, and the far one's.
    pub sector: (i64, i64),
    pub exit_sector: (i64, i64),
}

/// Where a rig entering `mouth` moving along `(vx, vy)` comes out: past the far mouth's rim
/// along the same heading (straight "up" when it barely moved).
#[must_use]
pub fn transit_exit(mouth: &MouthFacts, velocity: (f32, f32)) -> (WorldPos, (f32, f32)) {
    let speed = hypot(velocity.0, velocity.1);
    let (ux, uy) = if speed > 1.0 {
        (velocity.0 / speed, velocity.1 / speed)
    } else {
        (0.0, -1.0)
    };
    let reach = TRANSIT_EXIT_TILES.mul_add(TILE_SIZE, mouth.exit_radius);
    (
        mouth.exit.offset(ux * reach, uy * reach),
        (ux * TRANSIT_EXIT_SPEED, uy * TRANSIT_EXIT_SPEED),
    )
}

/// A wormhole pair the crew has flown (the nav HUD shows both ends from then on).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnownWormhole {
    /// The mouth flown into and its sector.
    pub mouth: WorldPos,
    pub sector: (i64, i64),
    /// The far mouth and its sector.
    pub exit: WorldPos,
    pub exit_sector: (i64, i64),
}

// --- Nebulae --------------------------------------------------------------------------------

/// Share of the scanner's reach left in `fog` per mille (70 % gone in the thickest fog).
#[must_use]
pub fn fog_range_share(fog: u16) -> f32 {
    (f32::from(fog.min(1_000)) / 1_000.0).mul_add(-0.7, 1.0)
}

/// How visible nav markers are in `fog` per mille (they fade to 15 %).
#[must_use]
pub fn marker_visibility(fog: u16) -> f32 {
    (f32::from(fog.min(1_000)) / 1_000.0).mul_add(-0.85, 1.0)
}

// --- Asteroids and comets -----------------------------------------------------------------

/// Seconds a unit of ore takes to drill out of a rock in contact, at drill tier 1 (each tier
/// past it is a quarter faster).
pub const CONTACT_MINE_SECONDS: f32 = 1.6;
/// Seconds a unit takes under the Mining Beam.
pub const BEAM_MINE_SECONDS: f32 = 1.0;
/// The Mining Beam's reach from the nose (px past the rock's surface) and its cone (radians).
pub const BEAM_RANGE: f32 = 14.0 * TILE_SIZE;
pub const BEAM_CONE: f32 = 0.22;
/// Fuel a second the beam burns.
pub const BEAM_FUEL_PER_SECOND: f32 = 1.0;
/// The drill bites when the rig's underside faces the rock within this angle (radians).
pub const DRILL_FACING: f32 = 0.9;
/// A rig counts as resting against a rock within this many px of its surface.
pub const ROCK_CONTACT_SLACK: f32 = 6.0;
/// Speed (into a rock) past which a bump dents the hull, and the damage per px/s over it.
pub const ROCK_SAFE_SPEED: f32 = 330.0;
pub const ROCK_DAMAGE_SCALE: f32 = 0.05;
pub const ROCK_DAMAGE_MAX: f32 = 30.0;
/// How far round the rig asteroids are looked up each tick (tiles, plus the tick's travel).
pub const ROCK_QUERY_TILES: i64 = 480;
/// Seconds a unit of comet dust takes to gather in a tail, and the most a comet gives the crew.
pub const COMET_GATHER_SECONDS: f32 = 2.5;
pub const COMET_HARVEST_CAP: u16 = 6;
/// What a comet's tail gives (a stand-in until Wave 13 adds space items).
pub const COMET_MINERAL: MineralKind = MineralKind::Mythril;

/// A rock of a belt (in [`FlightBody::extra`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RockFacts {
    pub id: AsteroidId,
    pub ore: AsteroidOre,
    pub yield_units: u16,
}

/// A comet (in [`FlightBody::extra`]; its radius is the nucleus').
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CometFacts {
    pub key: CometKey,
    /// The tail's far end and its width there (px).
    pub tail_end: WorldPos,
    pub tail_width: f32,
}

/// A comet's stable name: its system and its index there.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct CometKey {
    pub system: SystemRef,
    pub index: u8,
}

/// The cargo an asteroid's ore becomes (Wave 13's space items replace these
/// stand-ins when a track adds them; the mapping is the one place to change).
#[must_use]
pub const fn asteroid_mineral(ore: AsteroidOre) -> MineralKind {
    match ore {
        AsteroidOre::Iron => MineralKind::Iron,
        AsteroidOre::Nickel => MineralKind::Silver,
        AsteroidOre::Ice => MineralKind::Skystone,
        AsteroidOre::Cobalt => MineralKind::Ruby,
        AsteroidOre::Platinum => MineralKind::Platinum,
        AsteroidOre::Iridium => MineralKind::Diamond,
        AsteroidOre::Voidglass => MineralKind::Uranium,
    }
}

/// Seconds a unit of ore takes to drill at `drill_level` (1-based) in contact.
#[must_use]
pub fn contact_mine_seconds(drill_level: u8) -> f32 {
    CONTACT_MINE_SECONDS / f32::from(drill_level.max(1) - 1).mul_add(0.25, 1.0)
}

/// The rock the rig rests its drill against: within [`ROCK_CONTACT_SLACK`] of its surface with
/// the underside (the rig's down axis) facing it within [`DRILL_FACING`].
#[must_use]
pub fn drill_contact(
    bodies: &[FlightBody],
    pos: WorldPos,
    orientation: f32,
    contact: f32,
) -> Option<usize> {
    let (up, _) = super::physics::axes(orientation);
    bodies.iter().position(|body| {
        if body.kind != BodyKind::Asteroid {
            return false;
        }
        let (dx, dy) = pos.delta(body.centre);
        let distance = hypot(dx, dy);
        if distance <= 0.0 || distance > body.radius + contact + ROCK_CONTACT_SLACK {
            return false;
        }
        // The rock lies along the rig's down axis (-up).
        let facing = (-up.0).mul_add(dx / distance, -up.1 * dy / distance);
        facing >= DRILL_FACING.cos_det()
    })
}

/// The rock the Mining Beam hits from the nose: the nearest whose surface is within
/// [`BEAM_RANGE`] and whose centre lies within [`BEAM_CONE`] of the nose (or the beam passes
/// through it).
#[must_use]
pub fn beam_target(bodies: &[FlightBody], pos: WorldPos, orientation: f32) -> Option<usize> {
    let (up, _) = super::physics::axes(orientation);
    let mut best: Option<(usize, f32)> = None;
    for (index, body) in bodies.iter().enumerate() {
        if body.kind != BodyKind::Asteroid {
            continue;
        }
        let (dx, dy) = pos.delta(body.centre);
        let distance = hypot(dx, dy);
        let gap = distance - body.radius;
        if gap > BEAM_RANGE || distance <= 0.0 {
            continue;
        }
        let along = up.0.mul_add(dx, up.1 * dy);
        if along <= 0.0 {
            continue;
        }
        let across = up.0.mul_add(dy, -up.1 * dx).abs();
        let in_cone = across <= body.radius || across / along <= BEAM_CONE;
        if in_cone && best.is_none_or(|(_, nearest)| gap < nearest) {
            best = Some((index, gap));
        }
    }
    best.map(|(index, _)| index)
}

/// The comet whose tail `pos` is inside, if any (the tail widens from the nucleus to its end).
#[must_use]
pub fn in_comet_tail(bodies: &[FlightBody], pos: WorldPos) -> Option<usize> {
    bodies.iter().position(|body| {
        let BodyExtra::Comet(comet) = body.extra else {
            return false;
        };
        let (tx, ty) = body.centre.delta_f64(comet.tail_end);
        let (px, py) = body.centre.delta_f64(pos);
        let length2 = tx.mul_add(tx, ty * ty);
        if length2 <= 0.0 {
            return false;
        }
        let along = px.mul_add(tx, py * ty) / length2;
        if !(0.0..=1.0).contains(&along) {
            return false;
        }
        let (ox, oy) = (along.mul_add(-tx, px), along.mul_add(-ty, py));
        let half = along.mul_add(
            f64::from(comet.tail_width) - f64::from(body.radius),
            f64::from(body.radius),
        ) * 0.5;
        ox.mul_add(ox, oy * oy) <= half.max(f64::from(body.radius)).powi(2)
    })
}

/// Most units a rock of `ore` gives up, whatever its size.
///
/// The rare ores (which stand in for deep home ores until space items exist) run out fast, so
/// one boulder near home never fills a late rig's bay with deep-tier ore (Wave 12
/// integration's balance pass).
#[must_use]
pub const fn ore_yield_cap(ore: AsteroidOre) -> u16 {
    match ore {
        AsteroidOre::Platinum => 40,
        AsteroidOre::Iridium => 20,
        AsteroidOre::Voidglass => 12,
        _ => u16::MAX,
    }
}

/// What a rock gives up in all: its size's yield, under its ore's cap.
#[must_use]
pub const fn rock_yield(rock: &crate::space::Asteroid) -> u16 {
    let cap = ore_yield_cap(rock.ore);
    if rock.yield_units < cap {
        rock.yield_units
    } else {
        cap
    }
}

/// A rock as a flight body (`None` once mined out).
#[must_use]
pub fn rock_body(rock: &crate::space::Asteroid, mined: u16) -> Option<FlightBody> {
    if mined >= rock_yield(rock) {
        return None;
    }
    let system = SystemRef {
        id: u64::from(rock.id.system),
        sector: (i64::from(rock.id.sector.x), i64::from(rock.id.sector.y)),
    };
    Some(FlightBody {
        body: super::BodyRef {
            system,
            index: super::space::ROCK_INDEX,
        },
        kind: BodyKind::Asteroid,
        centre: rock.centre.pos(),
        radius: f32::from(rock.radius) * TILE_SIZE,
        surface_gravity: 0.0,
        atmosphere: 0.0,
        approach: 0.0,
        corona_heat: 0.0,
        port_angle: 0.0,
        class: crate::star_chart::StarClass::default(),
        extra: BodyExtra::Rock(RockFacts {
            id: rock.id,
            ore: rock.ore,
            yield_units: rock_yield(rock),
        }),
    })
}

/// A body's Wave 12 physics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BodyExtra {
    #[default]
    None,
    Star(StarFacts),
    Hole(HoleFacts),
    Mouth(MouthFacts),
    Rock(RockFacts),
    Comet(CometFacts),
}

/// Deterministic cosine of a constant angle (no platform maths).
trait CosDet {
    fn cos_det(self) -> f32;
}

impl CosDet for f32 {
    fn cos_det(self) -> f32 {
        crate::field::trig::sin_cos(self).1
    }
}

// --- The crew's shared state ---------------------------------------------------------------

/// The crew's record of space's things (a world field in the shared `SpaceSystems` section):
/// asteroids mined, comet dust gathered and wormholes flown.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SystemsShared {
    /// Units taken from each asteroid (a rock at its yield is mined out and gone).
    #[serde(default, with = "crate::pair_list")]
    pub mined: BTreeMap<AsteroidId, u16>,
    /// Units of dust gathered from each comet.
    #[serde(default, with = "crate::pair_list")]
    pub comets: BTreeMap<CometKey, u16>,
    /// Wormhole pairs flown, in the order found (each once, from whichever end).
    #[serde(default)]
    pub wormholes: Vec<KnownWormhole>,
}

/// Most mined asteroids the record keeps (beyond it the lowest-keyed rock regrows).
pub const MAX_MINED_RECORDS: usize = 16_384;

impl SystemsShared {
    /// Whether nothing was ever recorded (the state hash skips it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mined.is_empty() && self.comets.is_empty() && self.wormholes.is_empty()
    }

    /// Units already taken from `rock`.
    #[must_use]
    pub fn mined_from(&self, rock: AsteroidId) -> u16 {
        self.mined.get(&rock).copied().unwrap_or(0)
    }

    /// Records a wormhole pair (once, from either end). Returns whether it was new.
    pub fn found_wormhole(&mut self, link: KnownWormhole) -> bool {
        let known = self.wormholes.iter().any(|seen| {
            (seen.sector == link.sector && seen.exit_sector == link.exit_sector)
                || (seen.sector == link.exit_sector && seen.exit_sector == link.sector)
        });
        if known {
            return false;
        }
        self.wormholes.push(link);
        true
    }

    /// The far end of the known wormhole whose mouth is in `sector`, if the crew flew it.
    #[must_use]
    pub fn wormhole_from(&self, sector: (i64, i64)) -> Option<(WorldPos, (i64, i64))> {
        self.wormholes.iter().find_map(|link| {
            if link.sector == sector {
                Some((link.exit, link.exit_sector))
            } else if link.exit_sector == sector {
                Some((link.mouth, link.sector))
            } else {
                None
            }
        })
    }
}

// --- Events ---------------------------------------------------------------------------------

/// What space's things did to a rig (netcode `860 + code`; 860-879 is this track's range).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum SystemsEvent {
    /// A star's radiance heats the hull (edge).
    StarHeat,
    /// A flare is coming (detail: seconds until it).
    FlareWarning,
    /// A flare struck the rig in the open.
    FlareStruck,
    /// A flare burned past while the rig sheltered behind a body.
    FlareSheltered,
    /// Inside a black hole's well (time runs slow).
    HoleWell,
    /// A black hole's pull is beating the thrusters.
    HoleStrain,
    /// Past the point of no return: call the beacon.
    HoleNoReturn,
    /// The horizon: the beacon's tug pulled the rig out, the hold lost (detail: the fee).
    HoleRescued,
    /// Into a wormhole's mouth.
    WormholeEntered,
    /// Out of the far mouth (detail: 1 when the crew first learned the pair).
    WormholeExited,
    /// Into a nebula's sensor fog (detail: fog per mille).
    NebulaEntered,
    /// Out of the fog.
    NebulaLeft,
    /// A unit of ore drilled or beamed out of an asteroid (detail: the `AsteroidOre` index).
    OreMined,
    /// An asteroid mined out (it crumbles).
    AsteroidDepleted,
    /// The cargo bay is full (mining stops).
    HoldFull,
    /// A unit of dust gathered in a comet's tail (detail: units the comet has given).
    CometDust,
    /// A rock dented the hull (detail: damage).
    RockBump,
    /// The parking hold took over (an idle rig holds station).
    ParkingHold,
    /// Coming down to the Moon handed the rig to the sky over it.
    MoonApproach,
}

impl SystemsEvent {
    pub const ALL: [Self; 19] = [
        Self::StarHeat,
        Self::FlareWarning,
        Self::FlareStruck,
        Self::FlareSheltered,
        Self::HoleWell,
        Self::HoleStrain,
        Self::HoleNoReturn,
        Self::HoleRescued,
        Self::WormholeEntered,
        Self::WormholeExited,
        Self::NebulaEntered,
        Self::NebulaLeft,
        Self::OreMined,
        Self::AsteroidDepleted,
        Self::HoldFull,
        Self::CometDust,
        Self::RockBump,
        Self::ParkingHold,
        Self::MoonApproach,
    ];

    /// Wire code offset (netcode adds 860).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// A short HUD line.
    #[must_use]
    pub const fn headline(self) -> &'static str {
        match self {
            Self::StarHeat => "STAR HEAT: HULL WARMING",
            Self::FlareWarning => "SOLAR FLARE INCOMING",
            Self::FlareStruck => "SOLAR FLARE",
            Self::FlareSheltered => "FLARE PASSED: SHELTERED",
            Self::HoleWell => "GRAVITY WELL",
            Self::HoleStrain => "ESCAPE THRUST FAILING",
            Self::HoleNoReturn => "NO RETURN: CALL THE BEACON",
            Self::HoleRescued => "EVENT HORIZON: TUG RESCUE, HOLD LOST",
            Self::WormholeEntered => "WORMHOLE TRANSIT",
            Self::WormholeExited => "TRANSIT COMPLETE",
            Self::NebulaEntered => "NEBULA: SENSORS FOGGED",
            Self::NebulaLeft => "SENSORS CLEAR",
            Self::OreMined => "ORE",
            Self::AsteroidDepleted => "ASTEROID MINED OUT",
            Self::HoldFull => "CARGO BAY FULL",
            Self::CometDust => "COMET DUST",
            Self::RockBump => "ROCK IMPACT",
            Self::ParkingHold => "PARKING HOLD",
            Self::MoonApproach => "MOON APPROACH",
        }
    }
}
