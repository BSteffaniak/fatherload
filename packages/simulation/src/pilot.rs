//! The pilot on foot (expansion plan Part II, F5): data, tuning and the platformer step.
//!
//! A player who climbs out of their rig becomes a [`crate::entity::EntityKind::Pilot`] entity
//! ([`PilotState`] in the entity store, owned by the player). The rig stays where it was parked.
//! This module is pure: the body step ([`step_pilot`]) reads the world through the small
//! [`PilotTerrain`] trait so it can be tested on a hand-made grid, and the world side
//! (`world/pilot.rs`) wires it into the tick: exit / board, oxygen, health, the hand pick,
//! throws, pickups, death and respawn.
//!
//! # Body convention
//!
//! A pilot's [`crate::entity::Body`] `(x, y)` is the **middle of its feet**: the hitbox spans
//! `x - PILOT_HALF_WIDTH ..= x + PILOT_HALF_WIDTH` and `y - height ..= y`, where `height` is
//! [`PILOT_STAND_HEIGHT`] (about 0.8 x 1.6 tiles, so a 1-tile-wide shaft is passable) or
//! [`PILOT_CROUCH_HEIGHT`] while crouching or crawling (so a 1-tile-high crawlway is too).
//!
//! # Tick-rate safety
//!
//! The body is integrated in fixed internal steps of [`PILOT_STEP_SECONDS`] (240 Hz): a tick
//! of `dt` runs `round(dt / PILOT_STEP_SECONDS)` steps (see [`substeps_for`]). Every timer
//! (coyote time, jump buffer, run ramp, ledge climb, jetpack burn) counts down in seconds per
//! step, so 30, 60 and 120 Hz sessions trace the same path for the same held input.

use serde::{Deserialize, Serialize};

use crate::{
    base::BuiltTile,
    entity::Body,
    ids::PlayerId,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{SurfaceMaterial, TILE_SIZE, TileKind, TilePosition},
};

/// Half the hitbox width (px): 25 px wide, about 0.8 tiles.
pub const PILOT_HALF_WIDTH: f32 = 12.5;
/// Standing hitbox height (px), about 1.6 tiles.
pub const PILOT_STAND_HEIGHT: f32 = 50.0;
/// Crouching / crawling hitbox height (px): under a tile, so crawlways fit.
pub const PILOT_CROUCH_HEIGHT: f32 = 28.0;
/// The fixed internal integration step (seconds).
pub const PILOT_STEP_SECONDS: f32 = 1.0 / 240.0;

/// Walking speed (px/s, about 4.7 tiles a second).
pub const WALK_SPEED: f32 = 150.0;
/// Running speed (px/s): reached after holding a direction for [`RUN_AFTER_SECONDS`].
pub const RUN_SPEED: f32 = 235.0;
/// Crouch-walk and crawl speed (px/s).
pub const CROUCH_SPEED: f32 = 70.0;
/// Holding a direction this long breaks a walk into a run (every seat runs: no extra button).
pub const RUN_AFTER_SECONDS: f32 = 0.45;
/// Horizontal acceleration on the ground (px/s²).
pub const GROUND_ACCEL: f32 = 1_600.0;
/// Braking on the ground with no input (px/s²).
pub const GROUND_BRAKE: f32 = 2_200.0;
/// Horizontal acceleration in the air (px/s²).
pub const AIR_ACCEL: f32 = 1_000.0;
/// Braking in the air with no input (px/s²).
pub const AIR_BRAKE: f32 = 450.0;
/// Gravity on the pilot (px/s²).
pub const GRAVITY: f32 = 1_700.0;
/// Take-off speed of a jump (px/s): about 2.5 tiles high, so a 2-tile ledge is a jump away.
pub const JUMP_SPEED: f32 = 520.0;
/// Releasing jump while rising keeps this share of the upward speed (variable jump height).
pub const JUMP_CUT: f32 = 0.45;
/// Terminal fall speed (px/s).
pub const MAX_FALL_SPEED: f32 = 1_100.0;
/// A jump still works this long after walking off a ledge.
pub const COYOTE_SECONDS: f32 = 0.1;
/// A jump pressed this long before landing still happens on landing.
pub const JUMP_BUFFER_SECONDS: f32 = 0.12;
/// Ladder and rope climbing speed (px/s).
pub const CLIMB_SPEED: f32 = 125.0;
/// Landing faster than this hurts (px/s): a fall of about five tiles is free.
pub const SAFE_FALL_SPEED: f32 = 760.0;
/// Health lost per px/s of landing speed over [`SAFE_FALL_SPEED`] (a twelve-tile drop kills
/// an unarmoured pilot).
pub const FALL_DAMAGE_PER_SPEED: f32 = 0.3;
/// Pulling up onto a ledge takes this long.
pub const LEDGE_CLIMB_SECONDS: f32 = 0.22;
/// After letting go of a ledge the pilot will not grab one again for this long.
pub const LEDGE_REGRAB_SECONDS: f32 = 0.3;
/// Jetpack thrust (px/s², against gravity).
pub const JETPACK_ACCEL: f32 = 2_700.0;
/// Fastest jetpack climb (px/s).
pub const JETPACK_MAX_RISE: f32 = 260.0;
/// Seconds of burn per jetpack tier ([`PilotGear::jetpack`]).
pub const JETPACK_SECONDS_PER_TIER: f32 = 1.5;
/// A full jetpack recharges in this long while standing on the ground.
pub const JETPACK_RECHARGE_SECONDS: f32 = 1.2;
/// Swimming: gravity in water (px/s², a slow sink).
pub const SWIM_SINK: f32 = 260.0;
/// Swimming stroke acceleration (px/s²).
pub const SWIM_ACCEL: f32 = 1_100.0;
/// Fastest swimming speed (px/s).
pub const SWIM_SPEED: f32 = 120.0;
/// Water drag (per second).
pub const SWIM_DRAG: f32 = 3.5;

/// Pilot health without a suit.
pub const BASE_HEALTH: f32 = 100.0;
/// Extra health per suit tier.
pub const HEALTH_PER_SUIT: f32 = 30.0;
/// Seconds of air without a suit.
pub const BASE_OXYGEN_SECONDS: f32 = 20.0;
/// Extra seconds of air per suit tier.
pub const OXYGEN_PER_SUIT: f32 = 15.0;
/// Air comes back this many times faster than it runs out.
pub const OXYGEN_REFILL_RATE: f32 = 4.0;
/// Health lost per second with no air left.
pub const SUFFOCATION_DAMAGE_PER_SECOND: f32 = 12.0;
/// Health regained per second once the pilot has been unhurt for [`REGEN_AFTER_SECONDS`].
pub const REGEN_PER_SECOND: f32 = 3.0;
/// Calm time before health regenerates.
pub const REGEN_AFTER_SECONDS: f32 = 5.0;
/// Invulnerability after a strike (seconds).
pub const STRIKE_GUARD_SECONDS: f32 = 0.6;
/// Health lost per second touching lava.
pub const LAVA_DAMAGE_PER_SECOND: f32 = 45.0;
/// Rows above this one have air too thin to breathe.
///
/// It sits above the tallest V7 peaks (whose headroom is 96 rows). Altitude physics (F6) may
/// replace this with an air-density lookup: see `WorldState::pilot_air_at`.
pub const THIN_AIR_ROW: i32 = -128;

/// Hand-pick reach from the pilot's chest (px).
pub const PICK_REACH: f32 = TILE_SIZE * 1.3;
/// Seconds to chip out a soft dirt-like tile (dirt, sand, snow, mud).
pub const PICK_SECONDS_SOFT: f32 = 0.9;
/// Seconds to chip out clay.
pub const PICK_SECONDS_CLAY: f32 = 1.6;
/// A swing at rock the pick cannot break clinks at most this often.
pub const PICK_CLINK_SECONDS: f32 = 0.5;

/// Personal pack capacity (items) without a suit.
pub const PACK_BASE_CAPACITY: u32 = 8;
/// Extra pack capacity per suit tier.
pub const PACK_PER_SUIT: u32 = 4;
/// A pilot collects loose drops within this distance of its middle (no magnet).
pub const PILOT_PICKUP_RADIUS: f32 = TILE_SIZE * 0.9;

/// A rig must be within this distance (centre to the pilot's middle) to board it.
pub const BOARD_REACH: f32 = TILE_SIZE * 1.6;
/// A rig moving faster than this (px/s) cannot be left.
pub const EXIT_MAX_SPEED: f32 = 90.0;
/// Seconds between two thrown bombs on foot.
pub const THROW_COOLDOWN_SECONDS: f32 = 0.35;
/// Throw speed on foot (px/s; lighter than a rig's launcher).
pub const THROW_SPEED: f32 = 330.0;

/// Highest jetpack tier sold.
pub const MAX_JETPACK_TIER: u8 = 3;
/// Highest suit tier sold.
pub const MAX_SUIT_TIER: u8 = 3;

/// The pilot's personal gear (bought upgrades), kept on the player and copied onto the pilot
/// entity when it climbs out.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PilotGear {
    /// Jetpack tier: 0 none, each tier adds [`JETPACK_SECONDS_PER_TIER`] of burn.
    #[serde(default)]
    pub jetpack: u8,
    /// Suit tier: health, oxygen and pack space.
    #[serde(default)]
    pub suit: u8,
}

impl PilotGear {
    /// Maximum health.
    #[must_use]
    pub fn max_health(self) -> f32 {
        f32::from(self.suit).mul_add(HEALTH_PER_SUIT, BASE_HEALTH)
    }

    /// Seconds of air.
    #[must_use]
    pub fn max_oxygen(self) -> f32 {
        f32::from(self.suit).mul_add(OXYGEN_PER_SUIT, BASE_OXYGEN_SECONDS)
    }

    /// Seconds of jetpack burn (0 without a jetpack).
    #[must_use]
    pub fn jetpack_seconds(self) -> f32 {
        f32::from(self.jetpack) * JETPACK_SECONDS_PER_TIER
    }

    /// Personal pack capacity in items.
    #[must_use]
    pub fn pack_capacity(self) -> u32 {
        PACK_BASE_CAPACITY + u32::from(self.suit) * PACK_PER_SUIT
    }
}

/// A piece of pilot gear sold on the Explosives Shack's arms rack
/// ([`crate::SessionServiceRequest::BuyPilotGear`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PilotGearKind {
    Jetpack,
    Suit,
}

impl PilotGearKind {
    pub const ALL: [Self; 2] = [Self::Jetpack, Self::Suit];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Jetpack => "Jetpack",
            Self::Suit => "Pilot Suit",
        }
    }

    /// The tier a player with `gear` owns.
    #[must_use]
    pub const fn tier(self, gear: PilotGear) -> u8 {
        match self {
            Self::Jetpack => gear.jetpack,
            Self::Suit => gear.suit,
        }
    }

    /// Highest tier sold.
    #[must_use]
    pub const fn max_tier(self) -> u8 {
        match self {
            Self::Jetpack => MAX_JETPACK_TIER,
            Self::Suit => MAX_SUIT_TIER,
        }
    }

    /// Price of the next tier after `owned` (`None` when maxed).
    #[must_use]
    pub const fn price(self, owned: u8) -> Option<u32> {
        if owned >= self.max_tier() {
            return None;
        }
        Some(match (self, owned) {
            (Self::Jetpack, 0) => 1_200,
            (Self::Jetpack, 1) => 4_500,
            (Self::Jetpack, _) => 14_000,
            (Self::Suit, 0) => 800,
            (Self::Suit, 1) => 3_200,
            (Self::Suit, _) => 11_000,
        })
    }

    /// One line on what the next tier does.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Jetpack => "Hold jump in the air to fly; +1.5 s burn, recharges on the ground",
            Self::Suit => "+30 health, +15 s of air, +4 pack space",
        }
    }
}

/// What the pilot's body is doing (movement mode).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum PilotMode {
    /// On the ground or in the air under gravity.
    #[default]
    Free,
    /// On a ladder, rope or lift ladder.
    Climb,
    /// In water.
    Swim,
    /// Hanging from a ledge (pulling up while `ledge_timer > 0`).
    Ledge,
}

/// The pose the renderer draws (derived from the state and body, see [`PilotState::pose`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PilotPose {
    Idle,
    Walk,
    Run,
    Jump,
    Fall,
    Climb,
    Crouch,
    Crawl,
    Swim,
    Hurt,
    Jetpack,
    Hang,
}

/// The hand pick's progress on one tile.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PickProgress {
    pub target: TilePosition,
    /// Seconds of chipping so far.
    pub seconds: f32,
}

/// A pilot entity's per-kind state (see the module docs). Public: everyone sees a pilot.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent movement flags of one platformer body"
)]
pub struct PilotState {
    /// The player this pilot belongs to (its rig is that player's [`crate::Player`]).
    pub owner: PlayerId,
    pub health: f32,
    pub max_health: f32,
    /// Seconds of air left.
    pub oxygen: f32,
    pub max_oxygen: f32,
    /// Seconds of jetpack burn left.
    pub jetpack_fuel: f32,
    /// Seconds of burn when full (0: no jetpack).
    pub jetpack_capacity: f32,
    /// The gear it climbed out with (drawn: the suit trim, the jetpack).
    pub gear: PilotGear,
    /// -1 facing left, 1 facing right.
    pub facing: i8,
    pub mode: PilotMode,
    pub crouching: bool,
    pub grounded: bool,
    /// The jetpack fired during the last step.
    pub jetting: bool,
    /// Jump was held during the last step (edge detection).
    pub jump_held: bool,
    /// Coyote time left (seconds).
    pub coyote: f32,
    /// Buffered jump time left (seconds).
    pub jump_buffer: f32,
    /// Seconds the current direction has been held (walk breaks into a run).
    pub run_time: f32,
    /// Pull-up time left while climbing onto a ledge (0: hanging still).
    pub ledge_timer: f32,
    /// Which side the ledge is on (-1 left, 1 right).
    pub ledge_side: i8,
    /// Time before a ledge can be grabbed again.
    pub ledge_cooldown: f32,
    /// Hurt flash and strike guard left (seconds).
    pub hurt: f32,
    /// Seconds since the pilot was last hurt (health regenerates after a while).
    pub calm: f32,
    /// The hand pick's current tile.
    pub pick: Option<PickProgress>,
    /// Time before the pick may clink on rock again / before the next throw.
    pub pick_cooldown: f32,
    pub throw_cooldown: f32,
    /// Seconds spent in the current pose family (animation clock, wraps).
    pub anim: f32,
}

impl PilotState {
    /// A fresh pilot for `owner` wearing `gear`: full health, air and jetpack.
    #[must_use]
    pub fn new(owner: PlayerId, gear: PilotGear, facing: i8) -> Self {
        Self {
            owner,
            health: gear.max_health(),
            max_health: gear.max_health(),
            oxygen: gear.max_oxygen(),
            max_oxygen: gear.max_oxygen(),
            jetpack_fuel: gear.jetpack_seconds(),
            jetpack_capacity: gear.jetpack_seconds(),
            gear,
            facing: if facing < 0 { -1 } else { 1 },
            mode: PilotMode::Free,
            crouching: false,
            grounded: false,
            jetting: false,
            jump_held: false,
            coyote: 0.0,
            jump_buffer: 0.0,
            run_time: 0.0,
            ledge_timer: 0.0,
            ledge_side: 0,
            ledge_cooldown: 0.0,
            hurt: 0.0,
            calm: REGEN_AFTER_SECONDS,
            pick: None,
            pick_cooldown: 0.0,
            throw_cooldown: 0.0,
            anim: 0.0,
        }
    }

    /// Current hitbox height.
    #[must_use]
    pub const fn height(&self) -> f32 {
        if self.crouching {
            PILOT_CROUCH_HEIGHT
        } else {
            PILOT_STAND_HEIGHT
        }
    }

    /// The pose to draw for a pilot with this state and `body`.
    #[must_use]
    pub fn pose(&self, body: &Body) -> PilotPose {
        if self.hurt > STRIKE_GUARD_SECONDS * 0.5 {
            return PilotPose::Hurt;
        }
        match self.mode {
            PilotMode::Climb => return PilotPose::Climb,
            PilotMode::Swim => return PilotPose::Swim,
            PilotMode::Ledge => return PilotPose::Hang,
            PilotMode::Free => {}
        }
        if self.jetting {
            return PilotPose::Jetpack;
        }
        let moving = body.velocity_x.abs() > 12.0;
        if self.crouching {
            return if moving {
                PilotPose::Crawl
            } else {
                PilotPose::Crouch
            };
        }
        if !self.grounded {
            return if body.velocity_y < 0.0 {
                PilotPose::Jump
            } else {
                PilotPose::Fall
            };
        }
        if body.velocity_x.abs() > (WALK_SPEED + RUN_SPEED) * 0.5 {
            PilotPose::Run
        } else if moving {
            PilotPose::Walk
        } else {
            PilotPose::Idle
        }
    }

    /// The pilot's middle (chest height), for reach, pickups, boarding and enemy sight.
    #[must_use]
    pub const fn middle(&self, body: &Body) -> (f32, f32) {
        (body.x, self.height().mul_add(-0.5, body.y))
    }

    /// Where its head is (breathing).
    #[must_use]
    pub fn head(&self, body: &Body) -> (f32, f32) {
        (body.x, body.y - self.height() + 6.0)
    }
}

/// Held on-foot controls for one tick (from the seat's [`crate::ControlState`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "independent held buttons")]
pub struct PilotControls {
    /// -1..=1.
    pub horizontal: f32,
    /// Up: climb, and jump where there is nothing to climb. Up held together with
    /// [`Self::jump`] jumps rather than grabbing a ladder (keyboards send both for Space).
    pub up: bool,
    /// Down: crouch, climb down, dive, let go of a ledge.
    pub down: bool,
    /// The dedicated jump button (also the jetpack, held in the air).
    pub jump: bool,
}

/// What the world looks like to a pilot's body.
pub trait PilotTerrain {
    /// Whether the tile stops a pilot (solid terrain; vent grates do not, see
    /// [`pilot_passable`]; outside the world counts as solid).
    fn blocks(&self, tile: TilePosition) -> bool;
    /// Whether the tile holds a ladder, rope or lift ladder.
    fn climbable(&self, tile: TilePosition) -> bool;
    /// Whether the tile is open water.
    fn water(&self, tile: TilePosition) -> bool;
    /// The highest one-way platform top (a parked rig's roof) a body spanning
    /// `left..right` crosses while its feet move down from `from_y` to `to_y`, if any.
    fn platform_top(&self, left: f32, right: f32, from_y: f32, to_y: f32) -> Option<f32>;
}

/// Whether a solid tile of `kind` lets a pilot through: vent grates (a crawlway's covers) are
/// solid to rigs, drops, charges and enemies but pilots crawl through them.
#[must_use]
pub const fn pilot_passable(kind: TileKind) -> bool {
    matches!(kind, TileKind::Built(BuiltTile::VentGrate))
}

/// Whether the hand pick can break a tile of `kind`, and how long it takes (soft dirt and clay
/// only; the rest is the rig's job).
#[must_use]
pub const fn pick_seconds(kind: TileKind) -> Option<f32> {
    match kind {
        TileKind::Dirt
        | TileKind::Surface(SurfaceMaterial::Sand | SurfaceMaterial::Snow | SurfaceMaterial::Mud) => {
            Some(PICK_SECONDS_SOFT)
        }
        TileKind::Clay => Some(PICK_SECONDS_CLAY),
        _ => None,
    }
}

/// Internal steps for a tick of `dt` seconds (at least one).
#[must_use]
pub fn substeps_for(dt: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a tick is a small positive number of steps"
    )]
    let steps = (dt / PILOT_STEP_SECONDS).round().clamp(1.0, 64.0) as u32;
    steps
}

/// What one tick of [`step_pilot`] did (events, damage).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent one-tick happenings"
)]
pub struct PilotStepReport {
    pub jumped: bool,
    /// The jetpack lit this tick (it was off the step before).
    pub jet_started: bool,
    /// The jetpack ran dry this tick.
    pub jet_empty: bool,
    pub grabbed_ledge: bool,
    pub climbed_ledge: bool,
    /// Landed on something this tick.
    pub landed: bool,
    /// The fastest landing this tick (px/s), for fall damage.
    pub landing_speed: f32,
    /// Went into water this tick.
    pub splashed: bool,
}

const FLUSH: f32 = 0.25;
const INSET: f32 = 0.1;
/// Longest swept piece of one step (px).
const SWEEP: f32 = 6.0;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
fn tile_of(value: f32) -> i32 {
    (value / TILE_SIZE).floor() as i32
}

/// The world pixel of a tile boundary.
#[allow(clippy::cast_precision_loss, reason = "tile indices are small")]
fn edge(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE
}

/// Whether a box with feet at `(x, feet)` and `height` overlaps anything that blocks a pilot.
#[must_use]
pub fn box_blocked(world: &impl PilotTerrain, x: f32, feet: f32, height: f32) -> bool {
    let left = tile_of(x - PILOT_HALF_WIDTH + INSET);
    let right = tile_of(x + PILOT_HALF_WIDTH - INSET);
    let top = tile_of(feet - height + INSET);
    let bottom = tile_of(feet - INSET);
    (top..=bottom).any(|ty| (left..=right).any(|tx| world.blocks(TilePosition { x: tx, y: ty })))
}

fn grounded(world: &impl PilotTerrain, body: &Body, height: f32) -> bool {
    box_blocked(world, body.x, body.y + 1.0, height)
        || world
            .platform_top(
                body.x - PILOT_HALF_WIDTH,
                body.x + PILOT_HALF_WIDTH,
                body.y - 1.0,
                body.y + 1.0,
            )
            .is_some()
}

fn climbable_here(world: &impl PilotTerrain, body: &Body, height: f32) -> bool {
    let column = tile_of(body.x);
    let middle = tile_of(height.mul_add(-0.5, body.y));
    let feet = tile_of(body.y - 2.0);
    world.climbable(TilePosition {
        x: column,
        y: middle,
    }) || world.climbable(TilePosition { x: column, y: feet })
}

fn climbable_below(world: &impl PilotTerrain, body: &Body) -> bool {
    world.climbable(TilePosition {
        x: tile_of(body.x),
        y: tile_of(body.y + 2.0),
    })
}

fn in_water(world: &impl PilotTerrain, body: &Body, height: f32) -> bool {
    world.water(TilePosition {
        x: tile_of(body.x),
        y: tile_of(height.mul_add(-0.5, body.y)),
    })
}

fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

/// Advances a pilot body one tick of `dt` seconds (see the module docs) and returns what
/// happened. Positions and velocities are quantized after every internal step.
pub fn step_pilot(
    body: &mut Body,
    state: &mut PilotState,
    world: &impl PilotTerrain,
    controls: PilotControls,
    dt: f32,
) -> PilotStepReport {
    let steps = substeps_for(dt);
    #[allow(clippy::cast_precision_loss, reason = "a handful of steps")]
    let step = dt / steps as f32;
    let mut report = PilotStepReport::default();
    body.previous_x = body.x;
    body.previous_y = body.y;
    for _ in 0..steps {
        step_once(body, state, world, controls, step, &mut report);
    }
    report
}

#[allow(
    clippy::too_many_lines,
    reason = "one platformer step keeps its mode rules side by side"
)]
fn step_once(
    body: &mut Body,
    state: &mut PilotState,
    world: &impl PilotTerrain,
    controls: PilotControls,
    dt: f32,
    report: &mut PilotStepReport,
) {
    let horizontal = controls.horizontal.clamp(-1.0, 1.0);
    let pressing = horizontal.abs() > 0.2;
    let jump_input = controls.jump || controls.up;
    let jump_pressed = jump_input && !state.jump_held;
    let jump_released = !jump_input && state.jump_held;
    state.jump_held = jump_input;
    state.anim = (state.anim + dt) % 600.0;
    state.hurt = (state.hurt - dt).max(0.0);
    state.ledge_cooldown = (state.ledge_cooldown - dt).max(0.0);
    if pressing {
        let side: i8 = if horizontal < 0.0 { -1 } else { 1 };
        if side == state.facing {
            state.run_time += dt;
        } else {
            state.run_time = 0.0;
        }
        state.facing = side;
    } else {
        state.run_time = 0.0;
    }
    if jump_pressed {
        state.jump_buffer = JUMP_BUFFER_SECONDS;
    } else {
        state.jump_buffer = (state.jump_buffer - dt).max(0.0);
    }

    // Crouching: held down on the ground (not on a ladder); stays down under a low ceiling.
    let wants_crouch = controls.down && state.grounded && state.mode == PilotMode::Free;
    if wants_crouch {
        state.crouching = true;
    } else if state.crouching && !box_blocked(world, body.x, body.y, PILOT_STAND_HEIGHT) {
        state.crouching = false;
    }
    let height = state.height();

    // Water.
    let wet = in_water(world, body, height);
    if wet && state.mode != PilotMode::Swim && state.mode != PilotMode::Ledge {
        state.mode = PilotMode::Swim;
        state.jetting = false;
        report.splashed = true;
    } else if !wet && state.mode == PilotMode::Swim {
        state.mode = PilotMode::Free;
        if jump_input {
            // Kick out of the water onto the bank.
            body.velocity_y = body.velocity_y.min(-JUMP_SPEED * 0.75);
        }
    }

    // Ladders and ropes.
    let climbable = climbable_here(world, body, height);
    if state.mode == PilotMode::Free
        && !state.crouching
        && ((controls.up && !controls.jump && climbable)
            || (controls.down && {
                let below = climbable_below(world, body);
                below || (climbable && !state.grounded)
            }))
    {
        state.mode = PilotMode::Climb;
        state.jetting = false;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
    }

    let was_grounded = state.grounded;
    match state.mode {
        PilotMode::Ledge => {
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
            if state.ledge_timer > 0.0 {
                state.ledge_timer -= dt;
                if state.ledge_timer <= 0.0 {
                    finish_ledge_climb(body, state, world);
                    report.climbed_ledge = true;
                }
                return;
            }
            let away = pressing && (horizontal < 0.0) != (state.ledge_side < 0);
            if (jump_pressed || controls.up) && !away {
                // Mantle: a fresh jump, or up held (the arrow-keys seat jumps with up).
                state.ledge_timer = LEDGE_CLIMB_SECONDS;
            } else if controls.down || away {
                state.mode = PilotMode::Free;
                state.ledge_cooldown = LEDGE_REGRAB_SECONDS;
            }
            return;
        }
        PilotMode::Climb => {
            let vertical = f32::from(i8::from(controls.down) - i8::from(controls.up));
            body.velocity_y = vertical * CLIMB_SPEED;
            body.velocity_x = horizontal * CLIMB_SPEED * 0.6;
            if controls.jump && jump_pressed {
                state.mode = PilotMode::Free;
                body.velocity_y = -JUMP_SPEED * 0.7;
                body.velocity_x = horizontal * WALK_SPEED;
                report.jumped = true;
            } else if !climbable && !climbable_below(world, body) {
                state.mode = PilotMode::Free;
                if controls.up {
                    // Topped out: a little hop onto the floor beside the ladder's end.
                    body.velocity_y = -260.0;
                }
            } else if state.grounded && controls.down && !climbable_below(world, body) {
                state.mode = PilotMode::Free;
            }
        }
        PilotMode::Swim => {
            let drag = (-SWIM_DRAG * dt).exp();
            body.velocity_x *= drag;
            body.velocity_y *= drag;
            body.velocity_x = horizontal.mul_add(SWIM_ACCEL * dt, body.velocity_x);
            let stroke = if jump_input {
                -SWIM_ACCEL
            } else if controls.down {
                SWIM_ACCEL * 0.6
            } else {
                SWIM_SINK
            };
            body.velocity_y = stroke.mul_add(dt, body.velocity_y);
            body.velocity_x = body.velocity_x.clamp(-SWIM_SPEED, SWIM_SPEED);
            body.velocity_y = body.velocity_y.clamp(-SWIM_SPEED * 1.4, SWIM_SPEED * 1.4);
        }
        PilotMode::Free => {
            if state.grounded {
                state.coyote = COYOTE_SECONDS;
            } else {
                state.coyote = (state.coyote - dt).max(0.0);
            }
            let top = if state.crouching {
                CROUCH_SPEED
            } else if state.run_time >= RUN_AFTER_SECONDS {
                RUN_SPEED
            } else {
                WALK_SPEED
            };
            let (accel, brake) = if state.grounded {
                (GROUND_ACCEL, GROUND_BRAKE)
            } else {
                (AIR_ACCEL, AIR_BRAKE)
            };
            body.velocity_x = if pressing {
                let target = horizontal * top;
                // Turning around brakes as hard as stopping.
                let rate = if body.velocity_x * target < 0.0 {
                    brake
                } else {
                    accel
                };
                approach(body.velocity_x, target, rate * dt)
            } else {
                approach(body.velocity_x, 0.0, brake * dt)
            };
            if state.jump_buffer > 0.0 && (state.grounded || state.coyote > 0.0) && !state.crouching
            {
                body.velocity_y = -JUMP_SPEED;
                state.jump_buffer = 0.0;
                state.coyote = 0.0;
                state.grounded = false;
                report.jumped = true;
            } else if jump_released && body.velocity_y < 0.0 && !state.jetting {
                body.velocity_y *= JUMP_CUT;
            }
            // Jetpack: hold jump in the air once the jump has run out of push.
            let can_jet = !state.grounded
                && jump_input
                && state.jetpack_capacity > 0.0
                && state.jetpack_fuel > 0.0
                && (state.jetting || body.velocity_y > -150.0);
            if can_jet {
                if !state.jetting {
                    report.jet_started = true;
                }
                state.jetting = true;
                state.jetpack_fuel = (state.jetpack_fuel - dt).max(0.0);
                if state.jetpack_fuel <= 0.0 {
                    report.jet_empty = true;
                }
                body.velocity_y = (GRAVITY - JETPACK_ACCEL)
                    .mul_add(dt, body.velocity_y)
                    .max(-JETPACK_MAX_RISE);
            } else if state.grounded && body.velocity_y >= 0.0 {
                // Standing: the floor holds the pilot (no creeping into it).
                state.jetting = false;
                body.velocity_y = 0.0;
            } else {
                state.jetting = false;
                body.velocity_y = GRAVITY.mul_add(dt, body.velocity_y).min(MAX_FALL_SPEED);
            }
        }
    }
    if state.grounded && state.jetpack_capacity > 0.0 {
        state.jetpack_fuel = (state.jetpack_capacity / JETPACK_RECHARGE_SECONDS)
            .mul_add(dt, state.jetpack_fuel)
            .min(state.jetpack_capacity);
    }

    // Integrate: horizontal then vertical, swept in short pieces.
    let dx = body.velocity_x * dt;
    let mut wall: i8 = 0;
    let pieces = (dx.abs() / SWEEP).ceil().max(1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few pieces"
    )]
    let count = pieces as u32;
    let part = dx / pieces;
    for _ in 0..count {
        if part == 0.0 {
            break;
        }
        let next = body.x + part;
        if box_blocked(world, next, body.y, height) {
            let flush = if part > 0.0 {
                edge(tile_of(next + PILOT_HALF_WIDTH - INSET)) - PILOT_HALF_WIDTH - FLUSH
            } else {
                edge(tile_of(next - PILOT_HALF_WIDTH + INSET) + 1) + PILOT_HALF_WIDTH + FLUSH
            };
            if !box_blocked(world, flush, body.y, height) && (flush - body.x) * part >= 0.0 {
                body.x = flush;
            }
            body.velocity_x = 0.0;
            wall = if part > 0.0 { 1 } else { -1 };
            break;
        }
        body.x = next;
    }

    // Pressing into a wall counts as touching it even while flush (no movement this step).
    if wall == 0 && pressing {
        let side: i8 = if horizontal < 0.0 { -1 } else { 1 };
        if box_blocked(
            world,
            f32::from(side).mul_add(FLUSH + 0.5, body.x),
            body.y,
            height,
        ) {
            wall = side;
        }
    }

    let fall_speed = body.velocity_y;
    let dy = body.velocity_y * dt;
    let pieces = (dy.abs() / SWEEP).ceil().max(1.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few pieces"
    )]
    let count = pieces as u32;
    let part = dy / pieces;
    let mut landed = false;
    for _ in 0..count {
        if part == 0.0 {
            break;
        }
        let next = body.y + part;
        if part > 0.0
            && state.mode != PilotMode::Climb
            && let Some(top) = world.platform_top(
                body.x - PILOT_HALF_WIDTH,
                body.x + PILOT_HALF_WIDTH,
                body.y,
                next,
            )
        {
            body.y = top - FLUSH;
            body.velocity_y = 0.0;
            landed = true;
            break;
        }
        if box_blocked(world, body.x, next, height) {
            if part > 0.0 {
                let flush = edge(tile_of(next - INSET)) - FLUSH;
                if !box_blocked(world, body.x, flush, height) && flush >= body.y - 0.01 {
                    body.y = flush;
                }
                landed = true;
            } else {
                let flush = edge(tile_of(next - height + INSET) + 1) + height + FLUSH;
                if !box_blocked(world, body.x, flush, height) && flush <= body.y + 0.01 {
                    body.y = flush;
                }
            }
            body.velocity_y = 0.0;
            break;
        }
        body.y = next;
    }
    if landed {
        if state.mode == PilotMode::Climb {
            state.mode = PilotMode::Free;
        }
        state.jetting = false;
    }
    state.grounded = state.mode != PilotMode::Swim && grounded(world, body, height);
    // A landing is the step the pilot becomes grounded, whether it touched down this step or
    // ended the step within a pixel of the floor (its speed is the one it fell at).
    if state.grounded && !was_grounded && state.mode == PilotMode::Free {
        report.landed = true;
        report.landing_speed = report.landing_speed.max(fall_speed);
    }

    // Ledge grab: falling against a wall whose top is at hand height.
    if wall != 0
        && state.mode == PilotMode::Free
        && !state.grounded
        && !state.crouching
        && state.ledge_cooldown <= 0.0
        && body.velocity_y >= -40.0
        && pressing
        && (horizontal > 0.0) == (wall > 0)
        && let Some(top) = ledge_top(world, body, wall)
    {
        state.mode = PilotMode::Ledge;
        state.ledge_side = wall;
        state.ledge_timer = 0.0;
        state.jetting = false;
        body.y = top + PILOT_STAND_HEIGHT - 8.0;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        report.grabbed_ledge = true;
    }

    body.x = quantize(body.x);
    body.y = quantize(body.y);
    body.velocity_x = quantize(body.velocity_x);
    body.velocity_y = quantize(body.velocity_y);
}

/// The top (world y) of a ledge on side `wall` the pilot can hang from: the wall tile at hand
/// height is solid, the two tiles above it are open (room to stand), and hanging there fits.
fn ledge_top(world: &impl PilotTerrain, body: &Body, wall: i8) -> Option<f32> {
    let column = if wall > 0 {
        tile_of(body.x + PILOT_HALF_WIDTH + FLUSH + 1.0)
    } else {
        tile_of(body.x - PILOT_HALF_WIDTH - FLUSH - 1.0)
    };
    let hands = body.y - PILOT_STAND_HEIGHT;
    let row = tile_of(hands + 14.0);
    let solid = |y: i32| world.blocks(TilePosition { x: column, y });
    if !solid(row) || solid(row - 1) || solid(row - 2) {
        return None;
    }
    #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
    let top = row as f32 * TILE_SIZE;
    if top < hands - 6.0 || top > hands + 18.0 {
        return None;
    }
    let hang_feet = top + PILOT_STAND_HEIGHT - 8.0;
    (!box_blocked(world, body.x, hang_feet, PILOT_STAND_HEIGHT)).then_some(top)
}

/// Pulls a hanging pilot up onto its ledge (standing if there is room, else crouched).
fn finish_ledge_climb(body: &mut Body, state: &mut PilotState, world: &impl PilotTerrain) {
    state.mode = PilotMode::Free;
    state.ledge_timer = 0.0;
    let column = if state.ledge_side > 0 {
        tile_of(body.x + PILOT_HALF_WIDTH + FLUSH + 1.0)
    } else {
        tile_of(body.x - PILOT_HALF_WIDTH - FLUSH - 1.0)
    };
    let top = (body.y - PILOT_STAND_HEIGHT + 8.0).round();
    #[allow(clippy::cast_precision_loss, reason = "tile columns are small")]
    let x = if state.ledge_side > 0 {
        (column as f32).mul_add(TILE_SIZE, PILOT_HALF_WIDTH + 2.0)
    } else {
        ((column + 1) as f32).mul_add(TILE_SIZE, -PILOT_HALF_WIDTH - 2.0)
    };
    let feet = top - FLUSH;
    if !box_blocked(world, x, feet, PILOT_STAND_HEIGHT) {
        body.x = x;
        body.y = feet;
    } else if !box_blocked(world, x, feet, PILOT_CROUCH_HEIGHT) {
        body.x = x;
        body.y = feet;
        state.crouching = true;
    } else {
        state.ledge_cooldown = LEDGE_REGRAB_SECONDS;
        return;
    }
    body.velocity_x = 0.0;
    body.velocity_y = 0.0;
    state.grounded = true;
}

/// Fall damage for a landing at `speed` px/s (0 when safe).
#[must_use]
pub fn fall_damage(speed: f32) -> f32 {
    ((speed - SAFE_FALL_SPEED) * FALL_DAMAGE_PER_SPEED).max(0.0)
}

#[cfg(test)]
mod tests;
