//! Hostile factions and space combat (Wave 12 hostiles, expansion plan §16 "Hostile factions:
//! pirates, swarms, a machine fleet").
//!
//! Space is a place worth exploring, and some of it fights back. Three factions fly it, each
//! with its own look, its own way of fighting and its own boss:
//!
//! * **The Rustwake Corsairs** ([`Faction::Pirates`]): raiders who lurk in nebulae and asteroid
//!   belts. They close in, **hail** and **demand** a cut of the cargo; pay them
//!   ([`CombatCommand::PayTribute`]) and they leave, refuse (or shoot) and they fight: cutters
//!   circle and strafe, marauders hang back and fire bursts, the wounded flee. Their king flies
//!   the flagship *Gilded Maw* ([`ShipClass::PirateKing`]).
//! * **The Bloom** ([`Faction::Swarm`]): bio-swarm ships that **flock** (separation, alignment,
//!   cohesion) round the living worlds (jungle, fungal and ocean themes); mites dive and bite,
//!   stingers spit. The **Swarm Queen** broods them.
//! * **The Concord** ([`Faction::Machine`]): a disciplined machine fleet that flies **patrol
//!   routes** round machine worlds in **formations** (a lancer leading drones in a V), fires in
//!   synchronised volleys and charges telegraphed lances. Its capital ship is the **Arbiter**.
//!
//! # Combat
//!
//! * The flying rig has a **forward cannon** (fires along the nose: the player aims by turning
//!   the craft with its thrusters), a **turret** upgrade (fires along the seat's aim, relative to
//!   the craft; never chooses a target itself), **missiles** that lock only onto a ship inside a
//!   narrow cone round the nose when fired, and a **deflector shield** that soaks hits and
//!   recharges. There is no auto-aim in space: [`crate::input::WeaponControl::auto_aim`] is
//!   ignored. Gear: [`ShipGear`] (in [`crate::protection::HazardGear::ship`]).
//! * Ships and shots have **hit areas matching their drawn bodies** ([`ShipClass::hits`]: circles
//!   in the ship's frame that cover its sprite; the game's tests check the art against them).
//! * A ship destroyed leaves a **wreck** with credits and cargo ([`Wreck`]); fly through it to
//!   collect. Bosses leave their **tech** ([`BossTech`]).
//! * **Fair rules.** Every boss attack is **telegraphed** ([`Telegraph`]) for at least
//!   [`MIN_TELEGRAPH_STEPS`]; nothing is an unwinnable countdown. **Escape** is always possible:
//!   in a fight the warp button spools an **escape jump** ([`ESCAPE_SPOOL_STEPS`], no waypoint,
//!   fuel or alignment needed; incoming damage is halved while it spools), after which the rig
//!   is [`ESCAPE_JUMP_TILES`] away along its heading and the fight is over. A rig wrecked in a
//!   fight is towed home by the emergency beacon (as everywhere in space) and leaves a pod with
//!   half its cargo where it fell, to fly back for.
//! * **Pacing** ([`encounter`]): encounters scale with the distance from home (the galactic ring)
//!   and the rig's ship tier. The home sector and the space round stations are calm.
//! * **Bounties** ([`bounty_targets`], [`crate::WorldState::post_bounty`]): named elite ships the
//!   stations' boards post (the mission-target API for the stations track).
//! * **Ground raids** ([`raid`]): now and then a faction's dropship sets troopers down near a
//!   rig or by an alien town a rig is near, on a body; they are creatures with the usual
//!   body rules.
//!
//! # Determinism
//!
//! Everything runs in the home world pass in fixed **combat steps** of 1/120 s
//! ([`COMBAT_STEP_HZ`]): a tick runs `floor((t + 1) * 120 / hz) - floor(t * 120 / hz)` of them,
//! so every tick rate plays the same steps. Rolls are hashes of the world seed, ids and the step
//! ([`crate::flight::space::hash64`]); the trigonometry is [`crate::field::trig`]'s. All state is
//! the shared [`HostileSpace`] (world section `Hostiles`), replicated on change and predicted by
//! clients like the rest of the world.

#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "plain IEEE arithmetic: the same bits on every platform (no fused multiply-add or \
              platform hypot)"
)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "steps, ids and hashes fold into hash inputs and small counts"
)]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    flight::SystemRef, ids::PlayerId, item::ItemStack, position::WorldPos, terrain::TILE_SIZE,
};

pub mod ai;
pub mod boss;
pub mod combat;
pub mod encounter;
pub mod raid;
#[cfg(test)]
mod tests;

pub use encounter::{BountyTarget, Lair, boss_lair, bounty_targets};

// --- Time ------------------------------------------------------------------------------------

/// Combat steps a second: the hostiles' fixed internal step (tick-rate safe).
pub const COMBAT_STEP_HZ: u64 = 120;
/// Seconds in one combat step.
#[allow(clippy::cast_precision_loss, reason = "a small constant")]
pub const COMBAT_STEP_SECONDS: f32 = 1.0 / COMBAT_STEP_HZ as f32;

/// `seconds` as combat steps.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small positive durations"
)]
pub const fn steps(seconds: f32) -> u64 {
    (seconds * COMBAT_STEP_HZ as f32) as u64
}

/// The combat step that world tick `tick` starts at, at `hz` ticks a second.
#[must_use]
pub const fn step_at_tick(tick: u64, hz: u32) -> u64 {
    let hz = if hz == 0 { 1 } else { hz as u64 };
    tick.saturating_mul(COMBAT_STEP_HZ) / hz
}

// --- Rules ------------------------------------------------------------------------------------

/// Shortest warning any boss attack gives (steps; 0.9 s).
pub const MIN_TELEGRAPH_STEPS: u64 = 108;
/// An escape jump spools this long (steps; 2.5 s).
pub const ESCAPE_SPOOL_STEPS: u64 = 300;
/// An escape jump's length (tiles).
pub const ESCAPE_JUMP_TILES: f32 = 30_000.0;
/// Share of incoming damage taken while an escape jump spools.
pub const ESCAPE_DAMAGE_SHARE: f32 = 0.5;
/// Fuel an escape jump burns (never refused for want of fuel).
pub const ESCAPE_FUEL: f32 = 12.0;
/// Ships farther than this from every rig of their encounter give up (px).
pub const DISENGAGE_RANGE: f32 = 4_000.0;
/// A rig must stay within this of a wreck to collect it (px).
pub const WRECK_PICKUP_RANGE: f32 = 56.0;
/// Wrecks drift this long before they break up (steps; 5 minutes).
pub const WRECK_LIFE_STEPS: u64 = 120 * 300;
/// Most live ships at once (the budget across every encounter).
pub const MAX_SHIPS: usize = 64;
/// Most live shots at once.
pub const MAX_SHOTS: usize = 384;
/// The rig's hit circle in space (px; the drawn rig's body).
pub const RIG_HIT_RADIUS: f32 = 15.0;

// --- Factions and ships -----------------------------------------------------------------------

/// A hostile faction.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Faction {
    /// The Rustwake Corsairs: raiders of nebulae and belts.
    #[default]
    Pirates,
    /// The Bloom: bio-swarm ships of the living worlds.
    Swarm,
    /// The Concord: the machine fleet.
    Machine,
}

impl Faction {
    pub const ALL: [Self; 3] = [Self::Pirates, Self::Swarm, Self::Machine];

    /// The faction's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pirates => "Rustwake Corsairs",
            Self::Swarm => "the Bloom",
            Self::Machine => "the Concord",
        }
    }

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// From [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Pirates),
            1 => Some(Self::Swarm),
            2 => Some(Self::Machine),
            _ => None,
        }
    }

    /// The faction's boss.
    #[must_use]
    pub const fn boss(self) -> ShipClass {
        match self {
            Self::Pirates => ShipClass::PirateKing,
            Self::Swarm => ShipClass::SwarmQueen,
            Self::Machine => ShipClass::Arbiter,
        }
    }

    /// What its boss leaves behind.
    #[must_use]
    pub const fn tech(self) -> BossTech {
        match self {
            Self::Pirates => BossTech::CorsairTurret,
            Self::Swarm => BossTech::LivingShield,
            Self::Machine => BossTech::LanceMissiles,
        }
    }
}

/// A ship's hull class.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum ShipClass {
    /// Pirates: a fast light raider that circles and strafes.
    #[default]
    Cutter,
    /// Pirates: a gunship that hangs back and fires bursts.
    Marauder,
    /// Pirates' boss: the Pirate King's flagship, the *Gilded Maw*.
    PirateKing,
    /// Swarm: a small diver that bites.
    Mite,
    /// Swarm: a spitter that keeps its distance.
    Stinger,
    /// Swarm's boss: the Swarm Queen.
    SwarmQueen,
    /// Machine: a drone that flies in formation and fires in volleys.
    Drone,
    /// Machine: a lancer that leads a formation and charges lances.
    Lancer,
    /// Machine's boss: the capital ship Arbiter.
    Arbiter,
}

/// A hit circle in a ship's frame (px; x right, y toward the tail: the nose is at negative y).
///
/// The tables are authored at two px an art pixel; a ship is drawn and hit at
/// [`ShipClass::hull_scale`] times that ([`ShipClass::hit_circles`] scales them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitCircle {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

const fn hit(x: f32, y: f32, r: f32) -> HitCircle {
    HitCircle { x, y, r }
}

const HITS_CUTTER: [HitCircle; 8] = [
    hit(1.0, -9.0, 5.1),
    hit(-1.0, -5.0, 5.4),
    hit(-1.0, 7.0, 9.4),
    hit(9.0, 9.0, 5.1),
    hit(-11.0, 11.0, 5.1),
    hit(-15.0, 15.0, 5.1),
    hit(15.0, 15.0, 5.1),
    hit(-1.0, 21.0, 3.4),
];
const HITS_MARAUDER: [HitCircle; 12] = [
    hit(3.0, -23.0, 12.8),
    hit(-1.0, -19.0, 16.5),
    hit(27.0, -19.0, 3.4),
    hit(-27.0, -9.0, 6.6),
    hit(27.0, -9.0, 6.6),
    hit(1.0, 3.0, 26.3),
    hit(-17.0, 5.0, 17.4),
    hit(17.0, 5.0, 17.4),
    hit(1.0, 13.0, 21.8),
    hit(-27.0, 15.0, 7.4),
    hit(27.0, 15.0, 7.4),
    hit(11.0, 37.0, 3.4),
];
const HITS_KING: [HitCircle; 20] = [
    hit(1.0, -107.0, 20.7),
    hit(1.0, -79.0, 31.6),
    hit(1.0, -55.0, 40.2),
    hit(69.0, -45.0, 9.4),
    hit(-63.0, -31.0, 15.4),
    hit(1.0, -25.0, 44.1),
    hit(63.0, -23.0, 15.4),
    hit(-61.0, -7.0, 17.3),
    hit(63.0, -3.0, 15.4),
    hit(1.0, 11.0, 44.1),
    hit(63.0, 15.0, 15.4),
    hit(-61.0, 27.0, 16.5),
    hit(61.0, 29.0, 15.5),
    hit(1.0, 47.0, 44.1),
    hit(-55.0, 85.0, 11.1),
    hit(57.0, 85.0, 10.2),
    hit(1.0, 89.0, 43.4),
    hit(1.0, 101.0, 37.9),
    hit(-55.0, 105.0, 12.8),
    hit(55.0, 105.0, 12.8),
];
const HITS_MITE: [HitCircle; 7] = [
    hit(-3.0, -7.0, 2.2),
    hit(3.0, -7.0, 2.2),
    hit(-9.0, -3.0, 3.4),
    hit(9.0, -3.0, 3.4),
    hit(-9.0, -1.0, 3.4),
    hit(1.0, 1.0, 6.6),
    hit(-1.0, 5.0, 5.1),
];
const HITS_STINGER: [HitCircle; 8] = [
    hit(-1.0, -15.0, 6.6),
    hit(-15.0, -7.0, 5.1),
    hit(15.0, -5.0, 5.4),
    hit(-13.0, -3.0, 5.1),
    hit(-1.0, -1.0, 9.4),
    hit(1.0, -1.0, 9.4),
    hit(-1.0, 15.0, 7.4),
    hit(1.0, 17.0, 6.6),
];
const HITS_QUEEN: [HitCircle; 19] = [
    hit(5.0, -83.0, 23.5),
    hit(-1.0, -81.0, 26.6),
    hit(85.0, -71.0, 7.4),
    hit(-83.0, -67.0, 9.4),
    hit(-75.0, -55.0, 16.5),
    hit(73.0, -51.0, 19.1),
    hit(-61.0, -29.0, 31.4),
    hit(1.0, -29.0, 38.2),
    hit(61.0, -29.0, 31.4),
    hit(-63.0, -19.0, 29.1),
    hit(65.0, -15.0, 25.4),
    hit(45.0, 1.0, 11.1),
    hit(-71.0, 33.0, 10.2),
    hit(73.0, 33.0, 10.2),
    hit(-1.0, 45.0, 58.2),
    hit(-59.0, 53.0, 23.4),
    hit(57.0, 53.0, 25.4),
    hit(-63.0, 61.0, 19.1),
    hit(73.0, 73.0, 7.4),
];
const HITS_DRONE: [HitCircle; 8] = [
    hit(-1.0, -9.0, 6.6),
    hit(-7.0, -3.0, 6.6),
    hit(1.0, -1.0, 12.0),
    hit(-1.0, 5.0, 11.6),
    hit(-11.0, 11.0, 5.1),
    hit(11.0, 11.0, 5.1),
    hit(-15.0, 13.0, 3.4),
    hit(-1.0, 17.0, 3.4),
];
const HITS_LANCER: [HitCircle; 12] = [
    hit(1.0, -35.0, 3.4),
    hit(-1.0, -29.0, 5.1),
    hit(1.0, -21.0, 6.6),
    hit(-1.0, -7.0, 11.1),
    hit(3.0, -7.0, 9.4),
    hit(-1.0, 11.0, 17.3),
    hit(1.0, 21.0, 20.9),
    hit(-21.0, 25.0, 6.6),
    hit(21.0, 27.0, 9.4),
    hit(-23.0, 31.0, 7.4),
    hit(-1.0, 31.0, 14.6),
    hit(23.0, 31.0, 7.4),
];
const HITS_ARBITER: [HitCircle; 19] = [
    hit(1.0, -143.0, 9.4),
    hit(1.0, -123.0, 20.3),
    hit(1.0, -95.0, 27.3),
    hit(1.0, -39.0, 53.1),
    hit(-59.0, -15.0, 24.5),
    hit(61.0, -15.0, 23.4),
    hit(-11.0, 9.0, 72.9),
    hit(1.0, 9.0, 80.1),
    hit(25.0, 17.0, 59.4),
    hit(-67.0, 49.0, 17.3),
    hit(69.0, 53.0, 15.4),
    hit(1.0, 65.0, 56.7),
    hit(1.0, 97.0, 51.4),
    hit(-53.0, 113.0, 10.7),
    hit(53.0, 113.0, 10.7),
    hit(1.0, 117.0, 39.6),
    hit(-61.0, 127.0, 13.5),
    hit(61.0, 129.0, 14.0),
    hit(-63.0, 137.0, 10.2),
];

/// A ship class's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipStats {
    pub hull: f32,
    pub shield: f32,
    /// Shield regained a second, once [`SHIELD_REGEN_DELAY_STEPS`] passed since a hit.
    pub shield_regen: f32,
    /// Top speed (px/s).
    pub top_speed: f32,
    /// Acceleration (px/s²).
    pub thrust: f32,
    /// Fastest turn (rad/s).
    pub turn_rate: f32,
    /// What it fires (`None`: it bites or rams).
    pub gun: Option<ShotKind>,
    /// Steps between shots (or between bursts).
    pub fire_steps: u64,
    /// Shots in a burst.
    pub burst: u8,
    /// Range it opens fire at (px).
    pub range: f32,
    /// Damage of its body on contact (bites, rams).
    pub contact_damage: f32,
    /// Credits its wreck holds.
    pub credits: u32,
    /// Units of ore its wreck holds.
    pub cargo: u32,
    /// Drawn size (px, width x length; the game's sprite covers this).
    pub size: (f32, f32),
}

/// How much bigger than their art the fighters are drawn and hit ([`ShipClass::hull_scale`]).
pub const HULL_SCALE: f32 = 1.5;

/// Steps after a hit before a shield recharges (3 s).
pub const SHIELD_REGEN_DELAY_STEPS: u64 = 360;

impl ShipClass {
    pub const ALL: [Self; 9] = [
        Self::Cutter,
        Self::Marauder,
        Self::PirateKing,
        Self::Mite,
        Self::Stinger,
        Self::SwarmQueen,
        Self::Drone,
        Self::Lancer,
        Self::Arbiter,
    ];

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// From [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Cutter),
            1 => Some(Self::Marauder),
            2 => Some(Self::PirateKing),
            3 => Some(Self::Mite),
            4 => Some(Self::Stinger),
            5 => Some(Self::SwarmQueen),
            6 => Some(Self::Drone),
            7 => Some(Self::Lancer),
            8 => Some(Self::Arbiter),
            _ => None,
        }
    }

    /// Its faction.
    #[must_use]
    pub const fn faction(self) -> Faction {
        match self {
            Self::Cutter | Self::Marauder | Self::PirateKing => Faction::Pirates,
            Self::Mite | Self::Stinger | Self::SwarmQueen => Faction::Swarm,
            Self::Drone | Self::Lancer | Self::Arbiter => Faction::Machine,
        }
    }

    /// Whether it is a faction's boss.
    #[must_use]
    pub const fn is_boss(self) -> bool {
        matches!(self, Self::PirateKing | Self::SwarmQueen | Self::Arbiter)
    }

    /// How much bigger than its authored art (two px an art pixel) the ship is drawn and hit
    /// (Wave 13 art-audio: fighters read at a fight's zoom; the bosses are big already). The
    /// sprite, the hit circles, the reach and the drawn size all scale together.
    #[must_use]
    pub const fn hull_scale(self) -> f32 {
        if self.is_boss() { 1.0 } else { HULL_SCALE }
    }

    /// Space px a sprite's art pixel covers.
    #[must_use]
    pub const fn art_scale(self) -> f32 {
        2.0 * self.hull_scale()
    }

    /// The hit circles as drawn and hit ([`Self::hits`] scaled by [`Self::hull_scale`]).
    pub fn hit_circles(self) -> impl Iterator<Item = HitCircle> {
        let scale = self.hull_scale();
        self.hits().iter().map(move |circle| HitCircle {
            x: circle.x * scale,
            y: circle.y * scale,
            r: circle.r * scale,
        })
    }

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cutter => "Corsair Cutter",
            Self::Marauder => "Corsair Marauder",
            Self::PirateKing => "The Gilded Maw",
            Self::Mite => "Bloom Mite",
            Self::Stinger => "Bloom Stinger",
            Self::SwarmQueen => "The Swarm Queen",
            Self::Drone => "Concord Drone",
            Self::Lancer => "Concord Lancer",
            Self::Arbiter => "The Arbiter",
        }
    }

    /// Its numbers.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one block of numbers per class")]
    pub const fn stats(self) -> ShipStats {
        match self {
            Self::Cutter => ShipStats {
                hull: 60.0,
                shield: 0.0,
                shield_regen: 0.0,
                top_speed: 1_050.0,
                thrust: 900.0,
                turn_rate: 3.2,
                gun: Some(ShotKind::Bolt),
                fire_steps: 70,
                burst: 1,
                range: 620.0,
                contact_damage: 8.0,
                credits: 90,
                cargo: 2,
                size: (66.0, 72.0),
            },
            Self::Marauder => ShipStats {
                hull: 140.0,
                shield: 40.0,
                shield_regen: 8.0,
                top_speed: 760.0,
                thrust: 620.0,
                turn_rate: 2.0,
                gun: Some(ShotKind::Bolt),
                fire_steps: 260,
                burst: 3,
                range: 760.0,
                contact_damage: 12.0,
                credits: 220,
                cargo: 5,
                size: (108.0, 120.0),
            },
            Self::PirateKing => ShipStats {
                hull: 1_500.0,
                shield: 250.0,
                shield_regen: 12.0,
                top_speed: 420.0,
                thrust: 300.0,
                turn_rate: 0.7,
                gun: Some(ShotKind::Broadside),
                fire_steps: 0,
                burst: 0,
                range: 900.0,
                contact_damage: 40.0,
                credits: 4_000,
                cargo: 30,
                size: (160.0, 288.0),
            },
            Self::Mite => ShipStats {
                hull: 18.0,
                shield: 0.0,
                shield_regen: 0.0,
                top_speed: 1_150.0,
                thrust: 1_300.0,
                turn_rate: 5.0,
                gun: None,
                fire_steps: 0,
                burst: 0,
                range: 0.0,
                contact_damage: 7.0,
                credits: 12,
                cargo: 0,
                size: (36.0, 36.0),
            },
            Self::Stinger => ShipStats {
                hull: 48.0,
                shield: 0.0,
                shield_regen: 0.0,
                top_speed: 880.0,
                thrust: 800.0,
                turn_rate: 3.0,
                gun: Some(ShotKind::Spit),
                fire_steps: 190,
                burst: 1,
                range: 700.0,
                contact_damage: 6.0,
                credits: 40,
                cargo: 1,
                size: (60.0, 72.0),
            },
            Self::SwarmQueen => ShipStats {
                hull: 1_400.0,
                shield: 0.0,
                shield_regen: 0.0,
                top_speed: 520.0,
                thrust: 420.0,
                turn_rate: 1.3,
                gun: Some(ShotKind::Spore),
                fire_steps: 0,
                burst: 0,
                range: 900.0,
                contact_damage: 34.0,
                credits: 3_600,
                cargo: 26,
                size: (192.0, 224.0),
            },
            Self::Drone => ShipStats {
                hull: 50.0,
                shield: 20.0,
                shield_regen: 6.0,
                top_speed: 920.0,
                thrust: 820.0,
                turn_rate: 2.8,
                gun: Some(ShotKind::Pulse),
                fire_steps: 240,
                burst: 1,
                range: 700.0,
                contact_damage: 6.0,
                credits: 70,
                cargo: 2,
                size: (54.0, 60.0),
            },
            Self::Lancer => ShipStats {
                hull: 160.0,
                shield: 60.0,
                shield_regen: 10.0,
                top_speed: 700.0,
                thrust: 560.0,
                turn_rate: 1.6,
                gun: Some(ShotKind::Lance),
                fire_steps: 480,
                burst: 1,
                range: 900.0,
                contact_damage: 12.0,
                credits: 260,
                cargo: 6,
                size: (96.0, 144.0),
            },
            Self::Arbiter => ShipStats {
                hull: 1_700.0,
                shield: 450.0,
                shield_regen: 16.0,
                top_speed: 300.0,
                thrust: 220.0,
                turn_rate: 0.55,
                gun: Some(ShotKind::Pulse),
                fire_steps: 0,
                burst: 0,
                range: 1_000.0,
                contact_damage: 40.0,
                credits: 5_000,
                cargo: 34,
                size: (176.0, 320.0),
            },
        }
    }

    /// Hit areas in the ship's frame as authored (two px an art pixel: see
    /// [`Self::hit_circles`] for the drawn scale), matching its drawn body (the nose at
    /// negative y).
    #[must_use]
    pub const fn hits(self) -> &'static [HitCircle] {
        match self {
            Self::Cutter => &HITS_CUTTER,
            Self::Marauder => &HITS_MARAUDER,
            Self::PirateKing => &HITS_KING,
            Self::Mite => &HITS_MITE,
            Self::Stinger => &HITS_STINGER,
            Self::SwarmQueen => &HITS_QUEEN,
            Self::Drone => &HITS_DRONE,
            Self::Lancer => &HITS_LANCER,
            Self::Arbiter => &HITS_ARBITER,
        }
    }

    /// Radius that encloses every hit circle (a cheap first test).
    #[must_use]
    pub fn reach(self) -> f32 {
        self.hits()
            .iter()
            .map(|circle| (circle.x * circle.x + circle.y * circle.y).sqrt() + circle.r)
            .fold(0.0, f32::max)
            * self.hull_scale()
    }

    /// The bounty a station pays for one (credits).
    #[must_use]
    pub const fn bounty(self) -> u32 {
        self.stats().credits * 3
    }
}

/// What a shot is.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum ShotKind {
    /// The rig's forward cannon.
    #[default]
    Cannon,
    /// The rig's turret.
    Turret,
    /// The rig's missile (locks only onto a ship in the nose's cone when fired).
    Missile,
    /// A pirate's bolt.
    Bolt,
    /// The Pirate King's broadside shell.
    Broadside,
    /// A stinger's glob.
    Spit,
    /// The Swarm Queen's spore.
    Spore,
    /// A machine's pulse.
    Pulse,
    /// A lancer's (or the Arbiter's) lance: a fast heavy bolt down a telegraphed line.
    Lance,
}

impl ShotKind {
    pub const ALL: [Self; 9] = [
        Self::Cannon,
        Self::Turret,
        Self::Missile,
        Self::Bolt,
        Self::Broadside,
        Self::Spit,
        Self::Spore,
        Self::Pulse,
        Self::Lance,
    ];

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// From [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Cannon),
            1 => Some(Self::Turret),
            2 => Some(Self::Missile),
            3 => Some(Self::Bolt),
            4 => Some(Self::Broadside),
            5 => Some(Self::Spit),
            6 => Some(Self::Spore),
            7 => Some(Self::Pulse),
            8 => Some(Self::Lance),
            _ => None,
        }
    }

    /// Whether a rig fired it (it hits ships; a ship's hits rigs: no friendly fire).
    #[must_use]
    pub const fn from_rig(self) -> bool {
        matches!(self, Self::Cannon | Self::Turret | Self::Missile)
    }

    /// Muzzle speed (px/s, on top of the shooter's velocity for the rig's guns).
    #[must_use]
    pub const fn speed(self) -> f32 {
        match self {
            Self::Cannon => 1_750.0,
            Self::Turret => 1_500.0,
            Self::Missile => 650.0,
            Self::Bolt => 1_000.0,
            Self::Broadside => 620.0,
            Self::Spit => 640.0,
            Self::Spore => 380.0,
            Self::Pulse => 900.0,
            Self::Lance => 2_600.0,
        }
    }

    /// Lifetime (steps).
    #[must_use]
    pub const fn life_steps(self) -> u64 {
        match self {
            Self::Cannon | Self::Turret => 84,
            Self::Missile => 540,
            Self::Bolt => 110,
            Self::Broadside => 240,
            Self::Spit => 150,
            Self::Spore => 360,
            Self::Pulse => 120,
            Self::Lance => 60,
        }
    }

    /// Base damage (a hostile's is scaled by the encounter's danger).
    #[must_use]
    pub const fn damage(self) -> f32 {
        match self {
            Self::Cannon => 10.0,
            Self::Turret => 6.0,
            Self::Missile => 48.0,
            Self::Bolt => 9.0,
            Self::Broadside => 16.0,
            Self::Spit => 12.0,
            Self::Spore => 14.0,
            Self::Pulse => 8.0,
            Self::Lance => 34.0,
        }
    }

    /// The shot's own hit radius (px; matches its drawn size).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "each shot's size reads by itself")]
    pub const fn radius(self) -> f32 {
        match self {
            Self::Cannon | Self::Turret | Self::Bolt | Self::Pulse => 3.0,
            Self::Missile => 5.0,
            Self::Broadside | Self::Spit => 6.0,
            Self::Spore => 9.0,
            Self::Lance => 5.0,
        }
    }
}

/// Who fired a shot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Shooter {
    Rig(PlayerId),
    Ship(u32),
}

/// A shot in flight.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Shot {
    pub id: u32,
    pub kind: ShotKind,
    pub from: Shooter,
    pub pos: WorldPos,
    pub vx: f32,
    pub vy: f32,
    /// The step it burns out at.
    pub expires: u64,
    /// A missile's lock (a ship id).
    #[serde(default)]
    pub lock: Option<u32>,
    /// Damage it deals (the kind's, scaled by tier or danger).
    pub damage: f32,
}

/// A boss or lancer attack being wound up (drawn as its warning).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Attack {
    /// The Pirate King's broadside: a fan of shells from the side facing the rig.
    Broadside,
    /// The Pirate King's ram down a locked line.
    Ram,
    /// The Pirate King calls cutters.
    Escorts,
    /// The Swarm Queen's ring of spores (with a gap to slip through).
    SporeRing,
    /// The Swarm Queen's lunge at a locked point.
    Lunge,
    /// The Swarm Queen's brood of mites.
    Brood,
    /// A lance down a locked line (lancers and the Arbiter).
    Lance,
    /// The Arbiter's turret volleys.
    Volley,
    /// The Arbiter launches drones.
    Launch,
}

impl Attack {
    pub const ALL: [Self; 9] = [
        Self::Broadside,
        Self::Ram,
        Self::Escorts,
        Self::SporeRing,
        Self::Lunge,
        Self::Brood,
        Self::Lance,
        Self::Volley,
        Self::Launch,
    ];

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// From [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Broadside),
            1 => Some(Self::Ram),
            2 => Some(Self::Escorts),
            3 => Some(Self::SporeRing),
            4 => Some(Self::Lunge),
            5 => Some(Self::Brood),
            6 => Some(Self::Lance),
            7 => Some(Self::Volley),
            8 => Some(Self::Launch),
            _ => None,
        }
    }

    /// The HUD's warning.
    #[must_use]
    pub const fn warning(self) -> &'static str {
        match self {
            Self::Broadside => "BROADSIDE: get off its flank",
            Self::Ram => "RAM: move off the line",
            Self::Escorts => "CUTTERS LAUNCHING",
            Self::SporeRing => "SPORE RING: find the gap",
            Self::Lunge => "LUNGE: move off the mark",
            Self::Brood => "BROOD HATCHING",
            Self::Lance => "LANCE: move off the line",
            Self::Volley => "VOLLEY",
            Self::Launch => "DRONES LAUNCHING",
        }
    }
}

/// An attack being wound up: it fires at [`Self::fire_at`] along [`Self::angle`] (locked when
/// the warning began), or at [`Self::mark`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Telegraph {
    pub attack: Attack,
    /// The step the warning began.
    pub since: u64,
    /// The step it fires.
    pub fire_at: u64,
    /// The locked direction (radians, clockwise from up).
    pub angle: f32,
    /// The locked point (lunges and rams aim here).
    pub mark: WorldPos,
}

/// What a ship is doing.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ShipMode {
    /// Flying in to its target.
    #[default]
    Closing,
    /// Pirates: holding off and hailing (the demand is the encounter's).
    Hailing,
    /// Fighting.
    Attacking,
    /// Machines: flying the patrol route in formation, not yet alerted.
    Patrolling,
    /// Hurt or paid: flying away (it vanishes far off).
    Leaving,
    /// A boss or a lancer's lunge, ram or charge in progress until the step.
    Charging { until: u64 },
}

/// One hostile ship.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct HostileShip {
    pub id: u32,
    pub class: ShipClass,
    pub pos: WorldPos,
    pub vx: f32,
    pub vy: f32,
    /// Orientation (radians, clockwise from up: the nose).
    pub angle: f32,
    pub hull: f32,
    pub shield: f32,
    /// The encounter it belongs to.
    pub encounter: u32,
    /// The rig it is after.
    pub target: Option<PlayerId>,
    pub mode: ShipMode,
    /// The step it may fire again.
    pub ready_at: u64,
    /// Shots left in the burst.
    pub burst_left: u8,
    /// The step it was last hit.
    pub hit_at: u64,
    /// Who hit it last (bounties and kills credit them).
    pub hit_by: Option<PlayerId>,
    /// Formation slot (machines) or flock index.
    pub slot: u8,
    /// A boss's phase (0, 1) or a mite's dive state.
    pub phase: u8,
    /// The step a boss starts its next attack (or a mite its next dive).
    pub next_attack: u64,
    pub telegraph: Option<Telegraph>,
    /// Which attack in the boss's cycle comes next.
    pub cycle: u8,
    /// A posted bounty this ship is (its id).
    #[serde(default)]
    pub bounty: Option<u64>,
}

impl HostileShip {
    /// A fresh ship of `class` at `pos`.
    #[must_use]
    pub const fn new(id: u32, class: ShipClass, pos: WorldPos, angle: f32, encounter: u32) -> Self {
        let stats = class.stats();
        Self {
            id,
            class,
            pos,
            vx: 0.0,
            vy: 0.0,
            angle,
            hull: stats.hull,
            shield: stats.shield,
            encounter,
            target: None,
            mode: ShipMode::Closing,
            ready_at: 0,
            burst_left: 0,
            hit_at: 0,
            hit_by: None,
            slot: 0,
            phase: 0,
            next_attack: 0,
            telegraph: None,
            cycle: 0,
            bounty: None,
        }
    }

    /// Share of hull left (0-1).
    #[must_use]
    pub fn hull_share(&self) -> f32 {
        (self.hull / self.class.stats().hull).clamp(0.0, 1.0)
    }
}

/// A wreck adrift: collect it by flying through it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Wreck {
    pub id: u32,
    pub class: ShipClass,
    pub pos: WorldPos,
    pub vx: f32,
    pub vy: f32,
    pub credits: u32,
    pub cargo: Vec<ItemStack>,
    /// The step it breaks up.
    pub expires: u64,
    /// A boss's tech (granted to the crew on collection).
    #[serde(default)]
    pub tech: Option<BossTech>,
    /// A rig's own lost cargo pod (only its owner can collect it).
    #[serde(default)]
    pub owner: Option<PlayerId>,
}

/// Tech a boss leaves (granted to the whole crew).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum BossTech {
    /// The Pirate King's twin turret: the turret fires a twin-linked pair (and fits without a
    /// turret upgrade).
    #[default]
    CorsairTurret,
    /// The Swarm Queen's living shield: the deflector regrows twice as fast.
    LivingShield,
    /// The Arbiter's lance missiles: missiles hit harder and the rack holds two more.
    LanceMissiles,
}

impl BossTech {
    pub const ALL: [Self; 3] = [Self::CorsairTurret, Self::LivingShield, Self::LanceMissiles];

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CorsairTurret => "Corsair Twin Turret",
            Self::LivingShield => "Living Shield",
            Self::LanceMissiles => "Lance Missiles",
        }
    }

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

// --- Encounters -------------------------------------------------------------------------------

/// What kind of encounter it is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum EncounterKind {
    /// A random encounter of open space.
    Roaming,
    /// A machine patrol met on its route.
    Patrol,
    /// A boss in its lair.
    Boss,
    /// A posted bounty target and its escort (the bounty's id).
    Bounty(u64),
}

/// How an encounter stands.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum EncounterState {
    /// Ships closing in.
    Closing,
    /// Pirates demand `credits`, until the step `until`.
    Demand { credits: u32, until: u64 },
    /// A fight.
    Fighting,
    /// Paid off: the ships leave.
    Paid,
}

/// One encounter: ships after a rig.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Encounter {
    pub id: u32,
    pub faction: Faction,
    pub kind: EncounterKind,
    /// The rig it found.
    pub target: PlayerId,
    pub state: EncounterState,
    /// Danger (the galactic ring; scales damage).
    pub danger: u8,
    /// Where it began.
    pub origin: WorldPos,
    /// The step it began.
    pub started: u64,
}

impl Encounter {
    /// Damage scale of its ships' shots and bodies: [`HOSTILE_DAMAGE`] at danger 1, +12 % a
    /// ring.
    #[must_use]
    pub fn damage_scale(&self) -> f32 {
        f32::from(self.danger.saturating_sub(1)).mul_add(0.12, 1.0) * HOSTILE_DAMAGE
    }
}

// --- The rig's side ---------------------------------------------------------------------------

/// A rig's space weapons and shield (private loadout, in
/// [`crate::protection::HazardGear::ship`]). Every rig with the Star Drive has the tier-1
/// cannon and deflector.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ShipGear {
    /// Cannon upgrades (0-2: tiers 1-3).
    #[serde(default)]
    pub cannon: u8,
    /// Turret (0 none, 1-2).
    #[serde(default)]
    pub turret: u8,
    /// Missile rack (0 none, 1-3).
    #[serde(default)]
    pub missiles: u8,
    /// Deflector upgrades (0-3).
    #[serde(default)]
    pub shield: u8,
}

impl ShipGear {
    /// Everything fitted (test mode).
    pub const MAXED: Self = Self {
        cannon: 2,
        turret: 2,
        missiles: 3,
        shield: 3,
    };

    /// A rough tier for pacing (0-10).
    #[must_use]
    pub const fn tier(self) -> u8 {
        self.cannon + self.turret + self.missiles + self.shield
    }
}

/// Ship gear a station's shipyard sells ([`CombatCommand::BuyGear`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ShipGearKind {
    Cannon,
    Turret,
    Missiles,
    Shield,
}

impl ShipGearKind {
    pub const ALL: [Self; 4] = [Self::Cannon, Self::Turret, Self::Missiles, Self::Shield];

    /// Shop name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cannon => "Ship Cannon",
            Self::Turret => "Turret",
            Self::Missiles => "Missile Rack",
            Self::Shield => "Deflector",
        }
    }

    /// Top tier.
    #[must_use]
    pub const fn max_level(self) -> u8 {
        match self {
            Self::Cannon | Self::Turret => 2,
            Self::Missiles | Self::Shield => 3,
        }
    }

    /// The tier fitted.
    #[must_use]
    pub const fn level(self, gear: ShipGear) -> u8 {
        match self {
            Self::Cannon => gear.cannon,
            Self::Turret => gear.turret,
            Self::Missiles => gear.missiles,
            Self::Shield => gear.shield,
        }
    }

    /// Credits for the tier after `level`.
    #[must_use]
    pub const fn price(self, level: u8) -> u32 {
        let base = match self {
            Self::Cannon => 2_400,
            Self::Turret => 3_200,
            Self::Missiles => 2_800,
            Self::Shield => 2_600,
        };
        // On the space credit scale (Wave 13).
        crate::endgame::economy::space_credits(base * (level as u32 + 1))
    }

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// Hostiles' damage over the base numbers of shots and bodies.
///
/// Applied over [`ShotKind::damage`] and [`ShipStats::contact_damage`]: space is late game, and
/// a rig that reached it carries a deep hull (the numbers are tuned so a boss fight costs a
/// well-kept rig a good share of it).
pub const HOSTILE_DAMAGE: f32 = 3.0;

/// A rig's live combat state (in [`HostileSpace::rigs`]).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RigCombat {
    /// The step the cannon may fire again.
    pub cannon_at: u64,
    /// The step the turret may fire again.
    pub turret_at: u64,
    /// Missiles in the rack.
    pub missiles: u8,
    /// The step the next missile reloads.
    pub reload_at: u64,
    /// Deflector charge.
    pub shield: f32,
    /// The step the rig was last hit.
    pub hit_at: u64,
    /// The step an escape jump began spooling.
    pub escape: Option<u64>,
    /// The step the next roll for an encounter comes.
    pub next_roll: u64,
    /// The last trigger state (missiles fire on a press).
    pub firing: bool,
}

/// Steps between missile reloads (20 s).
pub const MISSILE_RELOAD_STEPS: u64 = 2_400;
/// Base deflector charge, and per upgrade.
pub const SHIELD_BASE: f32 = 120.0;
pub const SHIELD_PER_TIER: f32 = 110.0;
/// Deflector regained a second once [`SHIELD_REGEN_DELAY_STEPS`] passed since a hit.
pub const SHIELD_REGEN: f32 = 24.0;

/// A rig's deflector capacity.
#[must_use]
pub fn shield_capacity(gear: ShipGear) -> f32 {
    f32::from(gear.shield.min(3)).mul_add(SHIELD_PER_TIER, SHIELD_BASE)
}

/// Missiles a rig's rack holds.
#[must_use]
pub const fn missile_capacity(gear: ShipGear, tech: bool) -> u8 {
    if gear.missiles == 0 {
        0
    } else {
        gear.missiles * 2 + if tech { 2 } else { 0 }
    }
}

/// A combat action ([`crate::PlayerCommand::Combat`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CombatCommand {
    /// Fire a missile (it locks onto a ship within the nose's cone, if any).
    FireMissile,
    /// Pay the pirates' demand.
    PayTribute,
    /// Spool an escape jump (the warp button in a fight does this too).
    EscapeJump,
    /// Buy ship gear at a docked station's shipyard.
    BuyGear(ShipGearKind),
}

/// What happened in a fight (netcode `980 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum HostileEvent {
    /// Ships found a rig (detail: the faction).
    EncounterStarted,
    /// Pirates hail with a demand (detail: credits).
    Hailed,
    /// The demand was paid (detail: credits).
    TributePaid,
    /// Pirates refused or shot: a fight.
    TurnedHostile,
    /// Hostiles pulled the rig out of cruise.
    Interdicted,
    /// A ship was destroyed (detail: its class).
    ShipDestroyed,
    /// The rig was hit (detail: hull damage).
    RigHit,
    /// The rig's deflector is down.
    ShieldDown,
    /// A gun fired (detail: the shot kind).
    Fired,
    /// A rig's shot hit a ship (detail: 1 on its shield, 0 on the hull).
    ShipHit,
    /// An escape jump began spooling.
    EscapeSpooling,
    /// The escape jump went.
    Escaped,
    /// An encounter is over (the ships left, were beaten or the rig got away).
    EncounterOver,
    /// A wreck was collected (detail: credits).
    WreckLooted,
    /// A boss appeared (detail: its class).
    BossAppeared,
    /// A boss winds up an attack (detail: the [`Attack`]).
    BossTelegraph,
    /// A boss is beaten (detail: its faction).
    BossDefeated,
    /// A bounty was claimed (detail: the reward).
    BountyClaimed,
    /// A faction's dropship set troopers down on a body (detail: the faction).
    RaidLanded,
    /// Ship gear was bought (detail: the gear kind).
    GearBought,
}

impl HostileEvent {
    pub const ALL: [Self; 20] = [
        Self::EncounterStarted,
        Self::Hailed,
        Self::TributePaid,
        Self::TurnedHostile,
        Self::Interdicted,
        Self::ShipDestroyed,
        Self::RigHit,
        Self::ShieldDown,
        Self::Fired,
        Self::ShipHit,
        Self::EscapeSpooling,
        Self::Escaped,
        Self::EncounterOver,
        Self::WreckLooted,
        Self::BossAppeared,
        Self::BossTelegraph,
        Self::BossDefeated,
        Self::BountyClaimed,
        Self::RaidLanded,
        Self::GearBought,
    ];

    /// Wire code offset (netcode adds 980; 980-999 is this track's range).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// A short HUD line (empty for events the HUD does not announce).
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one line per event")]
    pub const fn headline(self) -> &'static str {
        match self {
            Self::EncounterStarted => "CONTACT",
            Self::Hailed => "HAILED: PAY OR FIGHT",
            Self::TributePaid => "TRIBUTE PAID",
            Self::TurnedHostile => "HOSTILE",
            Self::Interdicted => "INTERDICTED",
            Self::ShipDestroyed => "KILL",
            Self::RigHit => "HIT",
            Self::ShieldDown => "DEFLECTOR DOWN",
            Self::Fired => "",
            Self::ShipHit => "",
            Self::EscapeSpooling => "ESCAPE JUMP SPOOLING",
            Self::Escaped => "ESCAPED",
            Self::EncounterOver => "ALL CLEAR",
            Self::WreckLooted => "SALVAGE",
            Self::BossAppeared => "WARNING: CAPITAL CONTACT",
            Self::BossTelegraph => "",
            Self::BossDefeated => "BOSS DEFEATED",
            Self::BountyClaimed => "BOUNTY CLAIMED",
            Self::RaidLanded => "RAIDERS LANDED",
            Self::GearBought => "SHIPYARD",
        }
    }
}

// --- Bounties ---------------------------------------------------------------------------------

/// A bounty a station posted for a player.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PostedBounty {
    pub target: BountyTarget,
    /// Who took it (the reward goes to them).
    pub hunter: PlayerId,
    /// The encounter it spawned (once met).
    #[serde(default)]
    pub met: bool,
}

/// How a bounty stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BountyStatus {
    /// Never posted in this world.
    Unknown,
    /// Posted and the target still flies.
    Posted,
    /// The target was destroyed and the reward paid.
    Claimed,
}

// --- Ground raids -----------------------------------------------------------------------------

/// A raid under way on a body's ground.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Raid {
    pub store: crate::place::StoreKey,
    pub faction: Faction,
    /// The troopers' entity ids.
    pub troopers: Vec<u64>,
    /// Where they landed (strip tile).
    pub column: i32,
    pub row: i32,
    /// The step it landed.
    pub landed: u64,
}

// --- The world field --------------------------------------------------------------------------

/// Everything hostile in space (a world field, the shared `Hostiles` section; see the module
/// docs).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct HostileSpace {
    #[serde(default)]
    pub ships: Vec<HostileShip>,
    #[serde(default)]
    pub shots: Vec<Shot>,
    #[serde(default)]
    pub wrecks: Vec<Wreck>,
    #[serde(default)]
    pub encounters: Vec<Encounter>,
    /// Each rig's guns, rack, deflector and escape jump.
    #[serde(default, with = "crate::pair_list")]
    pub rigs: BTreeMap<PlayerId, RigCombat>,
    /// The next id for ships, shots, wrecks and encounters.
    #[serde(default)]
    pub next_id: u32,
    /// Bosses beaten (by faction).
    #[serde(default)]
    pub bosses_beaten: BTreeSet<Faction>,
    /// Tech the crew won.
    #[serde(default)]
    pub tech: BTreeSet<BossTech>,
    /// Ships destroyed, by faction (stations' standing and milestones read it).
    #[serde(default, with = "crate::pair_list")]
    pub kills: BTreeMap<Faction, u32>,
    /// Bounties posted and not yet claimed.
    #[serde(default)]
    pub bounties: Vec<PostedBounty>,
    /// Bounties claimed (ids).
    #[serde(default)]
    pub bounties_claimed: BTreeSet<u64>,
    /// Ground raids under way.
    #[serde(default)]
    pub raids: Vec<Raid>,
    /// The step each body store may next see a raid (stores keyed by their netcode code).
    #[serde(default, with = "crate::pair_list")]
    pub raid_ready: BTreeMap<u64, u64>,
    /// The machine patrols met (patrol ids), so a beaten patrol does not come back.
    #[serde(default)]
    pub patrols_met: BTreeSet<u64>,
}

impl HostileSpace {
    /// Whether nothing was ever recorded (the state hash skips it).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// A fresh id.
    pub const fn take_id(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(1);
        self.next_id
    }

    /// The encounter `id`.
    #[must_use]
    pub fn encounter(&self, id: u32) -> Option<&Encounter> {
        self.encounters.iter().find(|encounter| encounter.id == id)
    }

    /// The fight `player` is in, if any (the first encounter after it).
    #[must_use]
    pub fn engaged(&self, player: PlayerId) -> Option<&Encounter> {
        self.encounters
            .iter()
            .find(|encounter| encounter.target == player)
    }

    /// Whether `player` faces a pirate demand.
    #[must_use]
    pub fn demand_for(&self, player: PlayerId) -> Option<u32> {
        self.encounters
            .iter()
            .find_map(|encounter| match encounter.state {
                EncounterState::Demand { credits, .. } if encounter.target == player => {
                    Some(credits)
                }
                _ => None,
            })
    }

    /// The live boss, if any.
    #[must_use]
    pub fn boss(&self) -> Option<&HostileShip> {
        self.ships.iter().find(|ship| ship.class.is_boss())
    }

    /// Ships of encounter `id`.
    pub fn ships_of(&self, id: u32) -> impl Iterator<Item = &HostileShip> {
        self.ships.iter().filter(move |ship| ship.encounter == id)
    }

    /// Forgets a player who left.
    pub fn forget(&mut self, player: PlayerId) {
        self.rigs.remove(&player);
        self.encounters
            .retain(|encounter| encounter.target != player);
        let live: BTreeSet<u32> = self.encounters.iter().map(|e| e.id).collect();
        self.ships.retain(|ship| live.contains(&ship.encounter));
        self.bounties.retain(|bounty| bounty.hunter != player);
        self.shots.retain(|shot| shot.from != Shooter::Rig(player));
    }

    /// Whether `tech` was won.
    #[must_use]
    pub fn has_tech(&self, tech: BossTech) -> bool {
        self.tech.contains(&tech)
    }
}

/// `tiles` as px.
#[must_use]
pub const fn tiles(tiles: f32) -> f32 {
    tiles * TILE_SIZE
}

/// A bounty's id for `system` and `index` on `day` (stable).
#[must_use]
pub const fn bounty_id(system: SystemRef, day: u32, index: u8) -> u64 {
    crate::flight::space::hash64(system.id, 0xB0_17E5, day as i64, index as i64) >> 1
}
