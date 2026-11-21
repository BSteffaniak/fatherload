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

pub mod arsenal;

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
/// It sits above the tallest V7 peaks (whose headroom is 96 rows). Worlds with a V8 sky use its
/// air density instead ([`PILOT_THIN_AIR_DENSITY`], see `WorldState::pilot_air_at`).
pub const THIN_AIR_ROW: i32 = -128;

/// The breathable air under which a pilot's air counts as thin in a sky.
///
/// See [`crate::environment::breathable_air`]: about 800 rows up in a V8 sky (the lower cloud
/// archipelago; about 1,000 in V9's), and everywhere in orbit and on the Moon, where there is
/// no air at all.
pub const PILOT_THIN_AIR_DENSITY: f32 = 0.5;

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
/// On foot, a counter only services the rig (fuel, repairs, selling its cargo, upgrades) while
/// it is parked within this distance of the pilot (px, ten tiles: outside the door).
pub const RIG_SERVICE_REACH: f32 = TILE_SIZE * 10.0;
/// Credits a pilot on foot pays to radio for a pickup ([`crate::PlayerCommand::Rescue`] on
/// foot): hauled straight back aboard the rig, wherever it is.
pub const PILOT_RECALL_FEE: u32 = 25;
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
/// A jetpack of at least this tier can hover (hold jump and down in the air).
pub const HOVER_JETPACK_TIER: u8 = 2;
/// Hovering burns this share of the jetpack's full burn rate.
pub const HOVER_BURN_SHARE: f32 = 0.45;
/// How quickly a hover settles the pilot's vertical speed (px/s²).
pub const HOVER_ACCEL: f32 = 2_400.0;

/// Fastest fall under a glider (px/s), by glider tier (index 1 and 2).
pub const GLIDE_FALL_SPEEDS: [f32; 3] = [MAX_FALL_SPEED, 105.0, 70.0];
/// Forward speed of a glide (px/s).
pub const GLIDE_SPEED: f32 = 265.0;
/// Horizontal acceleration while gliding (px/s²).
pub const GLIDE_ACCEL: f32 = 700.0;

/// Wall-slide speed with climbing boots (px/s).
pub const WALL_SLIDE_SPEED: f32 = 115.0;
/// The sideways kick of a wall jump (px/s).
pub const WALL_JUMP_PUSH: f32 = 230.0;
/// Extra safe landing speed per boots tier (px/s): boots soak longer falls.
pub const BOOTS_SAFE_FALL_PER_TIER: f32 = 130.0;

/// Rope reeling speed (px/s): holding up shortens the rope, down pays it out.
pub const ROPE_REEL_SPEED: f32 = 170.0;
/// Shortest rope (px).
pub const ROPE_MIN_LENGTH: f32 = TILE_SIZE * 0.75;
/// Grapple reach at tier 1 (px); each further tier adds [`GRAPPLE_RANGE_PER_TIER`].
pub const GRAPPLE_BASE_RANGE: f32 = TILE_SIZE * 8.0;
/// Extra grapple reach per tier above the first (px).
pub const GRAPPLE_RANGE_PER_TIER: f32 = TILE_SIZE * 3.0;
/// The rig's winch cable (px): how far a rig lowers its pilot down a shaft.
pub const WINCH_LENGTH: f32 = TILE_SIZE * 40.0;
/// The rig's winch reels the pilot this much faster than a hand-held rope.
pub const WINCH_REEL_FACTOR: f32 = 1.4;

/// Armour soaks this share of every strike and blast per armour tier.
pub const ARMOR_SHARE_PER_TIER: f32 = 0.15;
/// Highest armour tier sold.
pub const MAX_ARMOR_TIER: u8 = 3;
/// Cold levels each thermal-lining tier covers ([`crate::environment::cold_level`]).
pub const COLD_COVER_PER_THERMAL: u8 = 3;
/// Highest thermal-lining tier sold (four tiers cover the Moon's cold).
pub const MAX_THERMAL_TIER: u8 = 4;
/// Cold levels a bare pilot shrugs off (the low isles are fine in shirtsleeves).
pub const BASE_COLD_COVER: u8 = 1;
/// Health per second per cold level the lining does not cover.
pub const COLD_DAMAGE_PER_LEVEL: f32 = 2.0;
/// Heat levels ([`crate::protection::band_heat_level`]) a bare pilot shrugs off.
pub const BASE_HEAT_COVER: u8 = 3;
/// Heat levels each heat-weave tier covers.
pub const HEAT_COVER_PER_TIER: u8 = 2;
/// Highest heat-weave tier sold.
pub const MAX_HEAT_TIER: u8 = 3;
/// Health per second per heat level the weave does not cover.
pub const HEAT_DAMAGE_PER_LEVEL: f32 = 1.5;
/// Headlamp light radius (tiles) by tier; tier 0 is the helmet's own small lamp.
pub const HEADLAMP_RADIUS_TILES: [f32; 4] = [3.0, 5.5, 7.5, 10.0];
/// Highest headlamp tier sold.
pub const MAX_HEADLAMP_TIER: u8 = 3;
/// Highest climbing-boots tier sold.
pub const MAX_BOOTS_TIER: u8 = 2;
/// Highest glider tier sold.
pub const MAX_GLIDER_TIER: u8 = 2;
/// Highest grapple tier sold.
pub const MAX_GRAPPLE_TIER: u8 = 3;
/// Rig link tiers: 1 recalls the rig on autopilot, 2 also turns its gun into a sentry that
/// covers the pilot.
pub const MAX_RIG_LINK_TIER: u8 = 2;
/// The rig link tier that adds the sentry turret.
pub const RIG_SENTRY_TIER: u8 = 2;

/// The pilot's personal gear (bought upgrades), kept on the player and copied onto the pilot
/// entity when it climbs out.
///
/// Every field defaults to 0 (old saves and loadouts read as "not owned").
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PilotGear {
    /// Jetpack tier: 0 none, each tier adds [`JETPACK_SECONDS_PER_TIER`] of burn; from
    /// [`HOVER_JETPACK_TIER`] it hovers.
    #[serde(default)]
    pub jetpack: u8,
    /// Suit tier: health, oxygen and pack space.
    #[serde(default)]
    pub suit: u8,
    /// Armour plates: each tier soaks [`ARMOR_SHARE_PER_TIER`] of strikes and blasts.
    #[serde(default)]
    pub armor: u8,
    /// Thermal lining against the cold of altitude ([`COLD_COVER_PER_THERMAL`] levels a tier).
    #[serde(default)]
    pub thermal: u8,
    /// Heat weave against the heat of the deep bands ([`HEAT_COVER_PER_TIER`] levels a tier).
    #[serde(default)]
    pub heat: u8,
    /// Headlamp tier ([`HEADLAMP_RADIUS_TILES`]).
    #[serde(default)]
    pub headlamp: u8,
    /// Climbing boots: wall slides and wall jumps, softer landings.
    #[serde(default)]
    pub boots: u8,
    /// Glider: hold jump while falling (once the jetpack is dry, or with down held).
    #[serde(default)]
    pub glider: u8,
    /// Grapple / rope launcher reach tier (0: none).
    #[serde(default)]
    pub grapple: u8,
    /// Rig link: recall (tier 1) and the sentry turret (tier [`RIG_SENTRY_TIER`]).
    #[serde(default)]
    pub rig_link: u8,
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

    /// The share of a strike or blast that gets through the armour.
    #[must_use]
    pub fn armor_factor(self) -> f32 {
        f32::from(self.armor.min(MAX_ARMOR_TIER)).mul_add(-ARMOR_SHARE_PER_TIER, 1.0)
    }

    /// Cold levels the suit covers.
    #[must_use]
    pub const fn cold_cover(self) -> u8 {
        BASE_COLD_COVER.saturating_add(self.thermal.saturating_mul(COLD_COVER_PER_THERMAL))
    }

    /// Heat levels the suit covers.
    #[must_use]
    pub const fn heat_cover(self) -> u8 {
        BASE_HEAT_COVER.saturating_add(self.heat.saturating_mul(HEAT_COVER_PER_TIER))
    }

    /// Headlamp radius in tiles (presentation and the light map).
    #[must_use]
    pub fn headlamp_radius_tiles(self) -> f32 {
        HEADLAMP_RADIUS_TILES[usize::from(self.headlamp.min(MAX_HEADLAMP_TIER))]
    }

    /// Whether the jetpack can hover.
    #[must_use]
    pub const fn can_hover(self) -> bool {
        self.jetpack >= HOVER_JETPACK_TIER
    }

    /// Grapple reach (px; 0 without a grapple).
    #[must_use]
    pub fn grapple_range(self) -> f32 {
        if self.grapple == 0 {
            return 0.0;
        }
        f32::from(self.grapple.min(MAX_GRAPPLE_TIER) - 1)
            .mul_add(GRAPPLE_RANGE_PER_TIER, GRAPPLE_BASE_RANGE)
    }

    /// Fastest safe landing (px/s).
    #[must_use]
    pub fn safe_fall_speed(self) -> f32 {
        f32::from(self.boots.min(MAX_BOOTS_TIER)).mul_add(BOOTS_SAFE_FALL_PER_TIER, SAFE_FALL_SPEED)
    }
}

/// A piece of pilot gear sold on a gunsmith's rack (the Explosives Shack's arms rack in town;
/// [`crate::SessionServiceRequest::BuyPilotGear`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PilotGearKind {
    Jetpack,
    Suit,
    Armor,
    Thermal,
    HeatWeave,
    Headlamp,
    Boots,
    Glider,
    Grapple,
    RigLink,
}

impl PilotGearKind {
    pub const ALL: [Self; 10] = [
        Self::Jetpack,
        Self::Suit,
        Self::Armor,
        Self::Thermal,
        Self::HeatWeave,
        Self::Headlamp,
        Self::Boots,
        Self::Glider,
        Self::Grapple,
        Self::RigLink,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Jetpack => "Jetpack",
            Self::Suit => "Pilot Suit",
            Self::Armor => "Armour Plates",
            Self::Thermal => "Thermal Lining",
            Self::HeatWeave => "Heat Weave",
            Self::Headlamp => "Headlamp",
            Self::Boots => "Climbing Boots",
            Self::Glider => "Glider",
            Self::Grapple => "Grapple Launcher",
            Self::RigLink => "Rig Link",
        }
    }

    /// The tier a player with `gear` owns.
    #[must_use]
    pub const fn tier(self, gear: PilotGear) -> u8 {
        match self {
            Self::Jetpack => gear.jetpack,
            Self::Suit => gear.suit,
            Self::Armor => gear.armor,
            Self::Thermal => gear.thermal,
            Self::HeatWeave => gear.heat,
            Self::Headlamp => gear.headlamp,
            Self::Boots => gear.boots,
            Self::Glider => gear.glider,
            Self::Grapple => gear.grapple,
            Self::RigLink => gear.rig_link,
        }
    }

    /// `gear` with one more tier of this piece.
    #[must_use]
    pub const fn upgraded(self, mut gear: PilotGear) -> PilotGear {
        match self {
            Self::Jetpack => gear.jetpack += 1,
            Self::Suit => gear.suit += 1,
            Self::Armor => gear.armor += 1,
            Self::Thermal => gear.thermal += 1,
            Self::HeatWeave => gear.heat += 1,
            Self::Headlamp => gear.headlamp += 1,
            Self::Boots => gear.boots += 1,
            Self::Glider => gear.glider += 1,
            Self::Grapple => gear.grapple += 1,
            Self::RigLink => gear.rig_link += 1,
        }
        gear
    }

    /// Whether the piece only does anything on a world with a sky (the thermal lining keeps
    /// out the high cold, which only V8+ skies have). Gunsmiths refuse to sell such gear on
    /// skyless worlds ([`crate::worldgen::GeneratorVersion::has_sky`]).
    #[must_use]
    pub const fn needs_sky(self) -> bool {
        matches!(self, Self::Thermal)
    }

    /// Highest tier sold.
    #[must_use]
    pub const fn max_tier(self) -> u8 {
        match self {
            Self::Jetpack => MAX_JETPACK_TIER,
            Self::Suit => MAX_SUIT_TIER,
            Self::Armor => MAX_ARMOR_TIER,
            Self::Thermal => MAX_THERMAL_TIER,
            Self::HeatWeave => MAX_HEAT_TIER,
            Self::Headlamp => MAX_HEADLAMP_TIER,
            Self::Boots => MAX_BOOTS_TIER,
            Self::Glider => MAX_GLIDER_TIER,
            Self::Grapple => MAX_GRAPPLE_TIER,
            Self::RigLink => MAX_RIG_LINK_TIER,
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
            (Self::Armor, 0) => 900,
            (Self::Armor, 1) => 3_600,
            (Self::Armor, _) => 12_000,
            (Self::Thermal, 0) => 1_500,
            (Self::Thermal, 1) => 5_000,
            (Self::Thermal, 2) => 16_000,
            (Self::Thermal, _) => 40_000,
            (Self::HeatWeave, 0) => 1_100,
            (Self::HeatWeave, 1) => 4_200,
            (Self::HeatWeave, _) => 13_000,
            (Self::Headlamp, 0) => 350,
            (Self::Headlamp, 1) => 1_450,
            (Self::Headlamp, _) => 4_800,
            (Self::Boots, 0) => 700,
            (Self::Boots, _) => 3_800,
            (Self::Glider, 0) => 1_600,
            (Self::Glider, _) => 6_500,
            (Self::Grapple, 0) => 1_400,
            (Self::Grapple, 1) => 4_000,
            (Self::Grapple, _) => 10_500,
            (Self::RigLink, 0) => 2_500,
            (Self::RigLink, _) => 9_000,
        })
    }

    /// One line on what the next tier does.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Jetpack => "Hold jump in the air to fly; +1.5 s burn. Mk II hovers (jump + down)",
            Self::Suit => "+30 health, +15 s of air, +4 pack space",
            Self::Armor => "Soaks 15% more of every bite, shot and blast",
            Self::Thermal => "Keeps out 3 more levels of the high cold",
            Self::HeatWeave => "Keeps out 2 more levels of the deep heat",
            Self::Headlamp => "A wider, brighter beam for caves and nights",
            Self::Boots => "Slide down walls, kick off them; longer safe falls",
            Self::Glider => "Hold jump while falling to glide; Mk II glides further",
            Self::Grapple => "Fire a hook at rock, reel in and swing; +3 tiles reach a tier",
            Self::RigLink => "Mk I: recall the rig to you. Mk II: its gun covers you",
        }
    }
}

/// A pilot's one-shot action ([`crate::PlayerCommand::Pilot`], WP-PILOT).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PilotAction {
    /// Use the lever, chest or keycard door within [`crate::mechanism::USE_REACH`].
    Use,
    /// The rope button: let go of a rope; beside the rig, take its winch cable; elsewhere fire
    /// the grapple along `(aim_x, aim_y)` (quantized, +y down; zero aims up and forward).
    Rope { aim_x: i8, aim_y: i8 },
    /// Call the rig over on autopilot ([`PilotGear::rig_link`]), or call it off.
    RecallRig,
    /// Hold `weapon` (`None`: the hand pick). Must be an owned personal arm.
    Equip(Option<crate::weapon::WeaponKind>),
    /// Quick-swap to the next owned personal arm (the pick sits between the last and first).
    CycleWeapon,
}

/// How a rope action went ([`crate::GameplayEventKind::PilotRope`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RopeEvent {
    /// The grapple hook caught on rock.
    Hooked,
    /// The grapple found nothing in reach.
    Missed,
    /// The rig's winch cable clipped on.
    Winched,
    /// The pilot let go of the rope.
    Released,
}

/// How a rig recall went ([`crate::GameplayEventKind::RigRecall`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RecallOutcome {
    /// The rig lifted off toward the pilot.
    Started,
    /// The rig pulled up beside the pilot.
    Arrived,
    /// No open path for the rig (or the recall was called off).
    NoPath,
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
    /// On a rope ([`PilotState::tether`]): a grapple hook in the rock or the rig's winch.
    /// Up reels in, down pays out, sideways swings.
    Tether,
}

/// How the pilot's body moves: the side-view platformer, or a crypt run's top-down mode.
///
/// WP-TOMBS: no gravity, eight-way walking; [`step_pilot`] hands it to [`step_top_down`], the
/// placeholder T29 replaces with the real top-down movement, dodge roll and collision.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum PilotView {
    #[default]
    Side,
    TopDown,
}

/// Run-only changes to the pilot's body from tomb relics (WP-TOMBS, [`crate::tomb::relic`]).
///
/// Set by the world while its owner is in a run (all zero otherwise). Public: everyone's
/// prediction steps the same body.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PilotBoons {
    /// Extra jumps in mid-air.
    pub extra_jumps: u8,
    /// Walk and run speed, percent.
    pub move_pct: i16,
    /// Jump speed, percent.
    pub jump_pct: i16,
    /// The glider's slower fall and faster glide, percent.
    pub glide_pct: i16,
    /// No fall damage.
    pub featherfall: bool,
    /// Hidden traps show and the headlamp reaches further.
    pub trap_sight: bool,
}

impl PilotBoons {
    /// The boons of a set of relic effects.
    #[must_use]
    pub const fn of(effects: crate::tomb::RelicEffects) -> Self {
        Self {
            extra_jumps: effects.extra_jumps,
            move_pct: effects.move_pct,
            jump_pct: effects.jump_pct,
            glide_pct: effects.glide_pct,
            featherfall: effects.featherfall,
            trap_sight: effects.trap_sight,
        }
    }
}

/// `percent` as a scale (never below a tenth).
fn boon_scale(percent: i16) -> f32 {
    (1.0 + f32::from(percent) / 100.0).max(0.1)
}

/// What a rope is tied to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TetherKind {
    /// A grapple hook bitten into the rock.
    Grapple,
    /// The pilot's own rig's winch cable (the anchor follows the rig).
    Winch,
}

/// A pilot's rope: where it is anchored and how long it is.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Tether {
    pub kind: TetherKind,
    /// The anchor (world px). A winch's follows its rig each tick.
    pub anchor_x: f32,
    pub anchor_y: f32,
    /// Current rope length (px).
    pub length: f32,
    /// Longest it may pay out (px).
    pub max_length: f32,
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
    /// Holding altitude on the jetpack.
    Hover,
    /// Under the glider.
    Glide,
    /// Hanging on a rope.
    Rope,
    /// Sliding down a wall (climbing boots).
    WallSlide,
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
    // --- WP-PILOT (all default for pilots saved before it) -------------------------------
    /// The weapon in the pilot's hands (`None`: the hand pick). Mirrors the private
    /// [`arsenal::PilotArsenal::equipped`] so everyone sees what the pilot holds.
    #[serde(default)]
    pub weapon: Option<crate::weapon::WeaponKind>,
    /// Time before the held weapon fires or swings again (seconds).
    #[serde(default)]
    pub fire_cooldown: f32,
    /// The latest aim (quantized, +y down; drawn as the arm and gun).
    #[serde(default)]
    pub aim_x: i8,
    #[serde(default)]
    pub aim_y: i8,
    /// A melee swing's or a shot's animation time left (seconds).
    #[serde(default)]
    pub swing: f32,
    /// The trigger was held last tick (semi-automatic arms want a fresh pull).
    #[serde(default)]
    pub trigger_held: bool,
    /// Hovering on the jetpack during the last step.
    #[serde(default)]
    pub hovering: bool,
    /// Gliding during the last step.
    #[serde(default)]
    pub gliding: bool,
    /// Pressed against a wall in the air last step (-1 left, 1 right, 0 none).
    #[serde(default)]
    pub wall: i8,
    /// The rope the pilot hangs on, if any ([`PilotMode::Tether`]).
    #[serde(default)]
    pub tether: Option<Tether>,
    /// The rig is flying itself to the pilot ([`PilotGear::rig_link`]).
    #[serde(default)]
    pub recalling: bool,
    /// What the suit is failing to keep out: 0 nothing, 1 cold, 2 heat (edge-detected events).
    #[serde(default)]
    pub exposure: u8,
    // --- WP-TOMBS (all default for pilots saved before it) -------------------------------
    /// Side view, or the top-down mode of a crypt run.
    #[serde(default)]
    pub view: PilotView,
    /// Relic boons while its owner is in a tomb run.
    #[serde(default)]
    pub boons: PilotBoons,
    /// Mid-air jumps used since the pilot last stood on something.
    #[serde(default)]
    pub air_jumps: u8,
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
            weapon: None,
            fire_cooldown: 0.0,
            aim_x: if facing < 0 { -127 } else { 127 },
            aim_y: 0,
            swing: 0.0,
            trigger_held: false,
            hovering: false,
            gliding: false,
            wall: 0,
            tether: None,
            recalling: false,
            exposure: 0,
            view: PilotView::Side,
            boons: PilotBoons {
                extra_jumps: 0,
                move_pct: 0,
                jump_pct: 0,
                glide_pct: 0,
                featherfall: false,
                trap_sight: false,
            },
            air_jumps: 0,
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
            PilotMode::Tether if !self.grounded => return PilotPose::Rope,
            PilotMode::Free | PilotMode::Tether => {}
        }
        if self.hovering {
            return PilotPose::Hover;
        }
        if self.jetting {
            return PilotPose::Jetpack;
        }
        if self.gliding {
            return PilotPose::Glide;
        }
        if self.wall != 0 && !self.grounded && body.velocity_y > 0.0 && self.gear.boots > 0 {
            return PilotPose::WallSlide;
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
    /// Where a winch rope is anchored this tick (the rig's belly), resolved by the world; a
    /// grapple's anchor lives in [`PilotState::tether`].
    pub anchor: Option<(f32, f32)>,
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
    /// The downward pull (px/s²) on a body whose feet are at world pixel `(x, y)`: [`GRAVITY`]
    /// by default (and everywhere on the ground); the world scales it by a V8 sky's gravity
    /// fields and lessens it in updrafts ([`crate::environment::Environment::body_gravity`]),
    /// so the pilot leaps high on the Moon and floats in orbit. Read once per tick.
    fn gravity_at(&self, x: f32, y: f32) -> f32 {
        let _ = (x, y);
        GRAVITY
    }
}

/// Whether a solid tile of `kind` lets a pilot through: vent grates (a crawlway's covers) are
/// solid to rigs, drops, charges and enemies but pilots crawl through them.
#[must_use]
pub const fn pilot_passable(kind: TileKind) -> bool {
    match kind {
        TileKind::Built(
            BuiltTile::VentGrate | BuiltTile::TombPillar(_) | BuiltTile::TombTrap { .. },
        ) => true,
        // The alien facility's energy veils (WP-MOON).
        TileKind::Lunar(tile) => tile.pilot_passable(),
        _ => false,
    }
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
    /// The jetpack started hovering this tick.
    pub hover_started: bool,
    /// The glider opened this tick.
    pub glide_started: bool,
    /// Kicked off a wall this tick (climbing boots).
    pub wall_jumped: bool,
    /// Let go of a rope with the jump button this tick.
    pub rope_released: bool,
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
    if state.view == PilotView::TopDown {
        for _ in 0..steps {
            step_top_down(body, state, world, controls, step);
        }
        return report;
    }
    let gravity = world.gravity_at(body.x, body.y);
    let (was_hovering, was_gliding) = (state.hovering, state.gliding);
    for _ in 0..steps {
        step_once(body, state, world, (controls, gravity), step, &mut report);
    }
    report.hover_started = state.hovering && !was_hovering;
    report.glide_started = state.gliding && !was_gliding;
    report
}

/// Walking speed in the top-down mode (px/s).
pub const TOP_DOWN_SPEED: f32 = 170.0;

/// One internal step of the top-down placeholder mode (WP-TOMBS crypts).
///
/// No gravity: the held directions walk eight ways (up is `controls.up`, down
/// `controls.down`), swept against the same box as the side view. **T29 replaces this** with
/// the real top-down movement (dodge roll, its own collision footprint and camera); the
/// [`PilotView`] flag on the state is the switch, and nothing else in the body step needs to
/// know.
pub fn step_top_down(
    body: &mut Body,
    state: &mut PilotState,
    world: &impl PilotTerrain,
    controls: PilotControls,
    dt: f32,
) {
    let horizontal = controls.horizontal.clamp(-1.0, 1.0);
    let vertical = f32::from(i8::from(controls.down) - i8::from(controls.up));
    state.anim = (state.anim + dt) % 600.0;
    state.hurt = (state.hurt - dt).max(0.0);
    state.mode = PilotMode::Free;
    state.grounded = true;
    state.crouching = false;
    state.jetting = false;
    state.hovering = false;
    state.gliding = false;
    state.tether = None;
    state.air_jumps = 0;
    state.jump_held = controls.jump;
    if horizontal.abs() > 0.2 {
        state.facing = if horizontal < 0.0 { -1 } else { 1 };
        state.run_time += dt;
    } else {
        state.run_time = 0.0;
    }
    let length = horizontal.hypot(vertical);
    let speed = TOP_DOWN_SPEED * boon_scale(state.boons.move_pct);
    let (vx, vy) = if length > 0.2 {
        (
            horizontal / length.max(1.0) * speed,
            vertical / length.max(1.0) * speed,
        )
    } else {
        (0.0, 0.0)
    };
    body.velocity_x = vx;
    body.velocity_y = vy;
    let height = state.height();
    for (delta, horizontal_axis) in [(vx * dt, true), (vy * dt, false)] {
        if delta == 0.0 {
            continue;
        }
        let pieces = (delta.abs() / SWEEP).ceil().max(1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few pieces"
        )]
        let count = pieces as u32;
        let part = delta / pieces;
        for _ in 0..count {
            let (next_x, next_y) = if horizontal_axis {
                (body.x + part, body.y)
            } else {
                (body.x, body.y + part)
            };
            if box_blocked(world, next_x, next_y, height) {
                if horizontal_axis {
                    body.velocity_x = 0.0;
                } else {
                    body.velocity_y = 0.0;
                }
                break;
            }
            body.x = next_x;
            body.y = next_y;
        }
    }
    body.x = quantize(body.x);
    body.y = quantize(body.y);
    body.velocity_x = quantize(body.velocity_x);
    body.velocity_y = quantize(body.velocity_y);
}

#[allow(
    clippy::too_many_lines,
    reason = "one platformer step keeps its mode rules side by side"
)]
fn step_once(
    body: &mut Body,
    state: &mut PilotState,
    world: &impl PilotTerrain,
    (controls, gravity): (PilotControls, f32),
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
    if wet
        && state.mode != PilotMode::Swim
        && state.mode != PilotMode::Ledge
        && state.mode != PilotMode::Tether
    {
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
    if state.mode != PilotMode::Free || state.grounded {
        state.hovering = false;
        state.gliding = false;
    }
    match state.mode {
        PilotMode::Tether => {
            step_tether(
                body,
                state,
                controls,
                (gravity, horizontal, jump_pressed),
                dt,
                report,
            );
        }
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
            } * boon_scale(state.boons.move_pct);
            if state.grounded {
                state.air_jumps = 0;
            }
            let jump_speed = JUMP_SPEED * boon_scale(state.boons.jump_pct);
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
                body.velocity_y = -jump_speed;
                state.jump_buffer = 0.0;
                state.coyote = 0.0;
                state.grounded = false;
                report.jumped = true;
            } else if jump_pressed
                && !state.grounded
                && state.gear.boots > 0
                && state.wall != 0
                && !state.crouching
            {
                // Climbing boots: kick off the wall the pilot is sliding down.
                body.velocity_y = -JUMP_SPEED * 0.92;
                body.velocity_x = -f32::from(state.wall) * WALL_JUMP_PUSH;
                state.facing = -state.wall;
                state.run_time = 0.0;
                state.wall = 0;
                state.jump_buffer = 0.0;
                report.wall_jumped = true;
                report.jumped = true;
            } else if jump_pressed
                && controls.jump
                && !state.grounded
                && !state.crouching
                && state.air_jumps < state.boons.extra_jumps
            {
                // Spring Heels (WP-TOMBS relics): another jump in mid-air.
                body.velocity_y = -jump_speed * 0.9;
                state.air_jumps += 1;
                state.jump_buffer = 0.0;
                report.jumped = true;
            } else if jump_released && body.velocity_y < 0.0 && !state.jetting {
                body.velocity_y *= JUMP_CUT;
            }
            // Jetpack, hover and glider: hold jump in the air. Jump with down hovers (a
            // hovering jetpack) or glides; plain jump burns the jetpack, then glides once dry.
            let airborne = !state.grounded;
            let fuel = state.jetpack_capacity > 0.0 && state.jetpack_fuel > 0.0;
            let want_hover =
                airborne && jump_input && controls.down && state.gear.can_hover() && fuel;
            let want_glide = airborne
                && jump_input
                && state.gear.glider > 0
                && body.velocity_y > 0.0
                && !want_hover
                && (controls.down || !fuel);
            let can_jet = airborne
                && jump_input
                && !want_hover
                && !want_glide
                && fuel
                && (state.jetting || body.velocity_y > -150.0);
            state.hovering = want_hover;
            state.gliding = want_glide;
            if want_hover {
                state.jetting = false;
                state.jetpack_fuel = HOVER_BURN_SHARE.mul_add(-dt, state.jetpack_fuel).max(0.0);
                if state.jetpack_fuel <= 0.0 {
                    report.jet_empty = true;
                }
                body.velocity_y = approach(body.velocity_y, 0.0, HOVER_ACCEL * dt);
            } else if want_glide {
                state.jetting = false;
                let glide = boon_scale(state.boons.glide_pct);
                let fall =
                    GLIDE_FALL_SPEEDS[usize::from(state.gear.glider.min(MAX_GLIDER_TIER))] / glide;
                body.velocity_y = gravity.mul_add(dt, body.velocity_y).min(fall);
                let target = if pressing {
                    horizontal * GLIDE_SPEED * glide
                } else {
                    f32::from(state.facing) * GLIDE_SPEED * 0.55 * glide
                };
                body.velocity_x = approach(body.velocity_x, target, GLIDE_ACCEL * dt);
            } else if can_jet {
                if !state.jetting {
                    report.jet_started = true;
                }
                state.jetting = true;
                state.jetpack_fuel = (state.jetpack_fuel - dt).max(0.0);
                if state.jetpack_fuel <= 0.0 {
                    report.jet_empty = true;
                }
                body.velocity_y = (gravity - JETPACK_ACCEL)
                    .mul_add(dt, body.velocity_y)
                    .max(-JETPACK_MAX_RISE);
            } else if state.grounded && body.velocity_y >= 0.0 {
                // Standing: the floor holds the pilot (no creeping into it).
                state.jetting = false;
                body.velocity_y = 0.0;
            } else {
                state.jetting = false;
                body.velocity_y = gravity.mul_add(dt, body.velocity_y).min(MAX_FALL_SPEED);
                // Climbing boots: pressing into a wall on the way down slides slowly.
                let into_wall = pressing && (horizontal > 0.0) == (state.wall > 0);
                if state.gear.boots > 0 && state.wall != 0 && into_wall {
                    body.velocity_y = body.velocity_y.min(WALL_SLIDE_SPEED);
                }
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
    if state.mode == PilotMode::Tether {
        apply_rope(body, state, world, height);
    }
    state.grounded = state.mode != PilotMode::Swim && grounded(world, body, height);
    state.wall = if state.mode == PilotMode::Free && !state.grounded {
        wall
    } else {
        0
    };
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

/// Fall damage for a landing at `speed` px/s in `gear` (climbing boots soak more).
#[must_use]
pub fn fall_damage_in(speed: f32, gear: PilotGear) -> f32 {
    ((speed - gear.safe_fall_speed()) * FALL_DAMAGE_PER_SPEED).max(0.0)
}

/// One internal step on a rope ([`PilotMode::Tether`]): reel in with up, pay out with down,
/// swing sideways in the air (walk on the ground), and let go with a fresh jump press.
fn step_tether(
    body: &mut Body,
    state: &mut PilotState,
    controls: PilotControls,
    (gravity, horizontal, jump_pressed): (f32, f32, bool),
    dt: f32,
    report: &mut PilotStepReport,
) {
    let Some(mut rope) = state.tether else {
        state.mode = PilotMode::Free;
        return;
    };
    if rope.kind == TetherKind::Winch
        && let Some((x, y)) = controls.anchor
    {
        rope.anchor_x = x;
        rope.anchor_y = y;
    }
    // A fresh press of the dedicated jump button lets go (with a little hop).
    if controls.jump && jump_pressed {
        state.tether = None;
        state.mode = PilotMode::Free;
        state.jump_buffer = 0.0;
        body.velocity_y = body.velocity_y.min(-JUMP_SPEED * 0.55);
        report.rope_released = true;
        return;
    }
    let reel = if rope.kind == TetherKind::Winch {
        ROPE_REEL_SPEED * WINCH_REEL_FACTOR
    } else {
        ROPE_REEL_SPEED
    };
    if controls.up && !controls.jump {
        rope.length = reel.mul_add(-dt, rope.length).max(ROPE_MIN_LENGTH);
    } else if controls.down {
        rope.length = reel.mul_add(dt, rope.length).min(rope.max_length);
    }
    let pressing = horizontal.abs() > 0.2;
    body.velocity_x = if state.grounded {
        if pressing {
            approach(body.velocity_x, horizontal * WALK_SPEED, GROUND_ACCEL * dt)
        } else {
            approach(body.velocity_x, 0.0, GROUND_BRAKE * dt)
        }
    } else if pressing {
        // Pumping a swing.
        approach(
            body.velocity_x,
            horizontal * RUN_SPEED,
            AIR_ACCEL * 0.6 * dt,
        )
    } else {
        body.velocity_x
    };
    body.velocity_y = if state.grounded && body.velocity_y >= 0.0 {
        0.0
    } else {
        gravity.mul_add(dt, body.velocity_y).min(MAX_FALL_SPEED)
    };
    rope.length = quantize(rope.length);
    state.tether = Some(rope);
}

/// Holds a roped pilot within its rope's length of the anchor: pulls the body back onto the
/// rope's circle (where it fits) and takes away the speed that would stretch it.
fn apply_rope(body: &mut Body, state: &PilotState, world: &impl PilotTerrain, height: f32) {
    let Some(rope) = state.tether else {
        return;
    };
    let (mx, my) = (body.x, height.mul_add(-0.5, body.y));
    let (dx, dy) = (mx - rope.anchor_x, my - rope.anchor_y);
    let distance = dx.hypot(dy);
    if distance <= rope.length || distance < 0.01 {
        return;
    }
    let (ux, uy) = (dx / distance, dy / distance);
    let x = ux.mul_add(rope.length, rope.anchor_x);
    let feet = height.mul_add(0.5, uy.mul_add(rope.length, rope.anchor_y));
    if !box_blocked(world, x, feet, height) {
        body.x = x;
        body.y = feet;
    } else if !box_blocked(world, body.x, feet, height) {
        body.y = feet;
    } else if !box_blocked(world, x, body.y, height) {
        body.x = x;
    }
    let radial = body.velocity_x.mul_add(ux, body.velocity_y * uy);
    if radial > 0.0 {
        body.velocity_x = (-radial).mul_add(ux, body.velocity_x);
        body.velocity_y = (-radial).mul_add(uy, body.velocity_y);
    }
}

/// Where a rope's anchor is, as the renderer draws it: a grapple's hook, or the rig's winch
/// (resolved each tick into the tether).
#[must_use]
pub fn rope_anchor(state: &PilotState) -> Option<(f32, f32)> {
    state.tether.map(|rope| (rope.anchor_x, rope.anchor_y))
}

#[cfg(test)]
mod tests;
