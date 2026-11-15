//! Explosive types: what each charge does, how it is deployed, sold and aimed.
//!
//! Every live explosive is a charge entity ([`crate::entity::EntityKind::Charge`], viewed as a
//! [`crate::PlacedBomb`]) with an [`ExplosiveKind`]; the bomb pass in `world::bombs` drives them
//! all (the shared body physics, chain reactions). Carried special explosives are
//! [`crate::item::ItemKind::Explosive`] items ([`ExplosiveKind::item`]). This module is pure data and math
//! (no world access), so the game can read the same tables for its HUD, shop and aim preview.
//!
//! # Integration seams for other tracks
//!
//! * [`blast_impulse`] is the one formula for "how hard does a blast at `source` push something
//!   at `target`". Rigs use it (bombs use the same numbers). Entities of the entity store
//!   whose kind [`crate::entity::EntityKind::feels_blasts`] (drops today; enemies later) are
//!   pushed by `WorldState::push_from_blasts` in `world/bombs.rs` through
//!   [`crate::physics::Blast::explosive`], which carries the same reach and peak push. That hook
//!   receives every blast of the tick (kind and position) before the terrain is cleared.
//! * [`ExplosiveKind::Emp`] pulses have no target yet: machines (enemy sentries, turrets) should
//!   react in the same `push_from_blasts` hook (`kind == Emp`) or to
//!   [`crate::GameplayEventKind::ExplosiveDetonated`] with `kind: Emp`. EMPs have no push
//!   reach, so `push_from_blasts` must check the kind before calling [`blast_impulse`].
//! * [`ExplosiveKind::Mining`] breaks ore and artifacts cleanly (see [`BlastShape::LootSafe`]):
//!   they pop out as drops and only about one ore tile in six shatters, where a plain blast
//!   shatters about a third (`world::drops::ore_survives_blast`).
//! * [`ExplosiveKind::Scare`] is a loud concussion with a light push and no terrain damage.
//!   Creatures (WP-ENEMIES) should flee from it: react in `push_from_blasts` (`kind == Scare`)
//!   or to [`crate::GameplayEventKind::ExplosiveDetonated`] with `kind: Scare`, within
//!   [`SCARE_RADIUS`] of the blast, for [`SCARE_MILLIS`].
//! * [`ExplosiveKind::Smoke`] bursts into a smoke screen that lingers where it landed:
//!   `WorldState::smoke_at(x, y)` answers whether a point is hidden (creature senses and line of
//!   sight should check it; auto-aim already does), and `WorldState::smoke_screens()` lists the
//!   live screens with their radius for presentation.

use serde::{Deserialize, Serialize};

use crate::terrain::TILE_SIZE;

/// The kind of a placed or thrown explosive.
///
/// [`Self::Bomb`] is the original timed gravity bomb (the serde default, so older saves and
/// replicas load unchanged). [`Self::Bomblet`] only ever comes out of a [`Self::Cluster`].
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum ExplosiveKind {
    /// Timed gravity bomb dropped at the rig's feet (the original bomb).
    #[default]
    Bomb,
    /// Thrown; sticks to the first surface it touches, then blows on a short fuse.
    Sticky,
    /// Thrown; bursts into a fan of bomblets.
    Cluster,
    /// One of a cluster bomb's submunitions (never sold or carried).
    Bomblet,
    /// Placed against a surface; waits for its owner's remote detonator.
    C4,
    /// Placed against a surface; blasts a long tunnel straight into it.
    Shaped,
    /// Dropped; a wide blast that pops ore and artifacts out intact as loot.
    Mining,
    /// Thrown; a small blast, then a spreading fire that eats soft ground.
    Incendiary,
    /// Thrown; an electromagnetic pulse (no blast) for machines.
    Emp,
    /// Thrown; burns bright for a while and charts the area around it.
    Flare,
    /// Thrown; bursts into a lingering smoke screen that hides what is inside it.
    Smoke,
    /// Thrown; a deafening bang with a light shove and no terrain damage that sends creatures
    /// running.
    Scare,
    /// A rocket launcher's warhead going off on impact (never sold or carried).
    Rocket,
}

/// How a rig deploys an explosive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Deployment {
    /// Released at the rig's feet and left to fall.
    Drop,
    /// Lobbed along the aim direction.
    Throw,
    /// Pushed gently along the aim direction until it sticks to the surface it reaches.
    Place,
}

/// The terrain a detonation removes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlastShape {
    /// No terrain damage.
    None,
    /// A diamond (Manhattan radius) around the charge, like the original bomb.
    Diamond(i32),
    /// A diamond that breaks ore and artifact tiles cleanly: they drop as loot with little
    /// shattered (WP-DROPS), where a plain blast shatters a third of the ore.
    LootSafe(i32),
    /// A straight tunnel `length` tiles long and `2 * half_width + 1` tiles wide, starting at
    /// the charge and running along its [`BlastDirection`]. Each lane stops at the first tile
    /// no blast can break.
    Tunnel { length: i32, half_width: i32 },
}

/// A cardinal direction: the way a shaped charge cuts (into the surface it stuck to).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum BlastDirection {
    Up,
    #[default]
    Down,
    Left,
    Right,
}

impl BlastDirection {
    /// Unit tile step of this direction (screen coordinates: +y is down).
    #[must_use]
    pub const fn step(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }

    /// The cardinal direction nearest to a vector (ties prefer the vertical axis).
    #[must_use]
    pub fn nearest(x: f32, y: f32) -> Self {
        if x.abs() > y.abs() {
            if x < 0.0 { Self::Left } else { Self::Right }
        } else if y < 0.0 {
            Self::Up
        } else {
            Self::Down
        }
    }
}

/// A throw direction, one of [`ThrowAim::DIRECTIONS`] evenly spaced angles. 0 points right and
/// the index turns clockwise on screen (4 is straight down, 8 left, 12 straight up).
///
/// Commands carry the quantized index so a throw is bit-identical on every peer.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ThrowAim(pub u8);

/// `cos(45°)`.
const H: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// `(cos, sin)` of each [`ThrowAim`] index, as exact literals (no runtime trigonometry).
const AIM_TABLE: [(f32, f32); 16] = [
    (1.0, 0.0),
    (0.923_879_5, 0.382_683_43),
    (H, H),
    (0.382_683_43, 0.923_879_5),
    (0.0, 1.0),
    (-0.382_683_43, 0.923_879_5),
    (-H, H),
    (-0.923_879_5, 0.382_683_43),
    (-1.0, 0.0),
    (-0.923_879_5, -0.382_683_43),
    (-H, -H),
    (-0.382_683_43, -0.923_879_5),
    (0.0, -1.0),
    (0.382_683_43, -0.923_879_5),
    (H, -H),
    (0.923_879_5, -0.382_683_43),
];

impl ThrowAim {
    /// Number of distinct aim directions.
    pub const DIRECTIONS: u8 = 16;
    /// Straight right.
    pub const RIGHT: Self = Self(0);
    /// Straight down.
    pub const DOWN: Self = Self(4);
    /// Straight left.
    pub const LEFT: Self = Self(8);
    /// Straight up.
    pub const UP: Self = Self(12);

    /// Unit vector of this direction (screen coordinates: +y is down).
    #[must_use]
    pub const fn vector(self) -> (f32, f32) {
        AIM_TABLE[(self.0 % Self::DIRECTIONS) as usize]
    }

    /// The nearest aim to an arbitrary vector (a stick deflection), or `None` for a zero vector.
    #[must_use]
    pub fn from_vector(x: f32, y: f32) -> Option<Self> {
        if x.abs() < f32::EPSILON && y.abs() < f32::EPSILON {
            return None;
        }
        let mut best = (0_u8, f32::MIN);
        for index in 0..Self::DIRECTIONS {
            let (cx, cy) = Self(index).vector();
            let dot = cx.mul_add(x, cy * y);
            if dot > best.1 {
                best = (index, dot);
            }
        }
        Some(Self(best.0))
    }

    /// The aim for digital directions: `horizontal` and `vertical` in `-1..=1` (vertical -1 is
    /// up), with `facing` (-1 or 1) used when no horizontal direction is held. Holding nothing
    /// lobs forward and slightly up; up lobs steeply; down throws down and forward.
    #[must_use]
    pub const fn from_digital(horizontal: i8, vertical: i8, facing: i8) -> Self {
        let right = if horizontal != 0 {
            horizontal > 0
        } else {
            facing >= 0
        };
        match (horizontal != 0, vertical) {
            // Straight up or down only when no side is held.
            (false, ..=-1) => {
                if right {
                    Self(13)
                } else {
                    Self(11)
                }
            }
            (false, 1..) => Self::DOWN,
            (_, 0) => {
                if right {
                    Self(15)
                } else {
                    Self(9)
                }
            }
            (true, ..=-1) => {
                if right {
                    Self(14)
                } else {
                    Self(10)
                }
            }
            (true, 1..) => {
                if right {
                    Self(1)
                } else {
                    Self(7)
                }
            }
        }
    }
}

/// One explosive pack sold at the explosives shack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplosiveOffer {
    pub kind: ExplosiveKind,
    /// Charges in the pack.
    pub count: u32,
    /// Price of the pack, in credits.
    pub cost: u32,
    /// Explosives shack level that stocks it.
    pub shack_level: u8,
}

/// Every pack the explosives shack sells besides plain bombs, in menu order.
///
/// Prices sit around the plain bomb's ~18 credits a charge, scaled by what each charge is worth:
/// flares are cheap utility, specialist charges cost two to three bombs, the big ones more.
pub const EXPLOSIVE_OFFERS: [ExplosiveOffer; 10] = [
    ExplosiveOffer {
        kind: ExplosiveKind::Flare,
        count: 5,
        cost: 40,
        shack_level: 0,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Smoke,
        count: 4,
        cost: 48,
        shack_level: 0,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Scare,
        count: 3,
        cost: 54,
        shack_level: 1,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Sticky,
        count: 3,
        cost: 75,
        shack_level: 0,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::C4,
        count: 3,
        cost: 120,
        shack_level: 1,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Cluster,
        count: 2,
        cost: 110,
        shack_level: 1,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Incendiary,
        count: 3,
        cost: 90,
        shack_level: 1,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Shaped,
        count: 2,
        cost: 130,
        shack_level: 2,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Mining,
        count: 2,
        // WP-DROPS: a mining charge is now a loot tool (its ore pops out nearly intact), priced
        // so bomb-diving with them stays in line with drilling (`world/drop_economy_tests.rs`).
        cost: 300,
        shack_level: 2,
    },
    ExplosiveOffer {
        kind: ExplosiveKind::Emp,
        count: 2,
        cost: 100,
        shack_level: 3,
    },
];

/// The shack's pack of `kind`, if it sells one.
#[must_use]
pub fn explosive_offer(kind: ExplosiveKind) -> Option<ExplosiveOffer> {
    EXPLOSIVE_OFFERS
        .into_iter()
        .find(|offer| offer.kind == kind)
}

/// Fuse of a remote charge that has not been triggered yet ("never").
pub const REMOTE_FUSE: crate::SimulationTick = crate::SimulationTick::new(u64::MAX);
/// Gap between charges when a remote detonator fires them in sequence.
pub const SEQUENCE_STEP_MILLIS: u64 = 300;
/// Arming delay between the remote detonator's click and the first charge going off (the
/// charges beep while they arm).
///
/// Besides the anticipation, this keeps online play smooth: a detonation is decided by one
/// player's action, which every other client (and, when its input arrives late, the player's
/// own client) only learns about from the host a round trip later. A blast that went off on
/// the very next tick would push rigs before their clients knew about it and show up as a
/// correction; with the arming delay the armed charges' deadlines replicate before the blast,
/// so every client predicts the push exactly.
pub const REMOTE_ARMING_MILLIS: u64 = 250;
/// Bomblets released by one cluster bomb.
pub const CLUSTER_BOMBLETS: usize = 6;
/// Fire rings an incendiary burns after its blast (ring `n` burns at Manhattan radius `n`).
pub const INCENDIARY_RINGS: u8 = 3;
/// Time between an incendiary's fire rings.
pub const INCENDIARY_RING_MILLIS: u64 = 450;
/// Pulses a lit flare burns for (one a second).
pub const FLARE_PULSES: u8 = 40;
/// Time between a flare's pulses (each charts the tiles around it for its owner).
pub const FLARE_PULSE_MILLIS: u64 = 1_000;
/// Manhattan radius, in tiles, a flare charts for its owner on each pulse.
pub const FLARE_REVEAL_RADIUS: i32 = 6;
/// Light radius of a lit flare, in world pixels (presentation).
pub const FLARE_LIGHT_RADIUS: f32 = TILE_SIZE * 7.0;
/// Pulses a smoke screen lingers for after it bursts.
pub const SMOKE_PULSES: u8 = 24;
/// Time between a smoke screen's pulses (it grows, holds, then thins out).
pub const SMOKE_PULSE_MILLIS: u64 = 500;
/// Widest a smoke screen gets, in world pixels.
pub const SMOKE_MAX_RADIUS: f32 = TILE_SIZE * 4.5;
/// How far a scare charge's bang carries for creatures, in world pixels.
pub const SCARE_RADIUS: f32 = TILE_SIZE * 10.0;
/// How long creatures flee from a scare charge's bang.
pub const SCARE_MILLIS: u64 = 6_000;

/// Radius of a smoke screen at pulse `stage` (1 right after the burst): it billows out over the
/// first pulses, holds, and thins over the last few. 0 before it bursts and once it is gone.
#[must_use]
pub fn smoke_radius(stage: u8) -> f32 {
    if stage == 0 || stage > SMOKE_PULSES {
        return 0.0;
    }
    let grow = (f32::from(stage) / 4.0).min(1.0);
    let fade = (f32::from(SMOKE_PULSES + 1 - stage) / 4.0).min(1.0);
    SMOKE_MAX_RADIUS * grow.sqrt() * fade.sqrt()
}

impl ExplosiveKind {
    /// Every kind a rig can carry and select, in the selection cycle's order.
    pub const SELECTABLE: [Self; 11] = [
        Self::Bomb,
        Self::Sticky,
        Self::Cluster,
        Self::C4,
        Self::Shaped,
        Self::Mining,
        Self::Incendiary,
        Self::Emp,
        Self::Flare,
        Self::Smoke,
        Self::Scare,
    ];

    /// Every kind, bomblets included.
    pub const ALL: [Self; 13] = [
        Self::Bomb,
        Self::Sticky,
        Self::Cluster,
        Self::Bomblet,
        Self::C4,
        Self::Shaped,
        Self::Mining,
        Self::Incendiary,
        Self::Emp,
        Self::Flare,
        Self::Smoke,
        Self::Scare,
        Self::Rocket,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bomb => "Bomb",
            Self::Sticky => "Sticky bomb",
            Self::Cluster => "Cluster bomb",
            Self::Bomblet => "Bomblet",
            Self::C4 => "C4 charge",
            Self::Shaped => "Shaped charge",
            Self::Mining => "Mining charge",
            Self::Incendiary => "Incendiary",
            Self::Emp => "EMP charge",
            Self::Flare => "Flare",
            Self::Smoke => "Smoke charge",
            Self::Scare => "Scare charge",
            Self::Rocket => "Rocket",
        }
    }

    /// The carried item for this kind (`None` for plain bombs, which stay a rig count, and
    /// bomblets, which are never carried). Special explosives are
    /// [`crate::item::ItemKind::Explosive`] items in the rig's cargo bay (no slot cost).
    #[must_use]
    pub const fn item(self) -> Option<crate::item::ItemKind> {
        match self.item_id() {
            Some(id) => Some(crate::item::ItemKind::Explosive(id)),
            None => None,
        }
    }

    /// The stable item id of this kind (see [`Self::item`]). Ids are saved: never renumber.
    #[must_use]
    pub const fn item_id(self) -> Option<crate::item::ExplosiveId> {
        let id = match self {
            Self::Bomb | Self::Bomblet | Self::Rocket => return None,
            Self::Sticky => 1,
            Self::Cluster => 2,
            Self::C4 => 3,
            Self::Shaped => 4,
            Self::Mining => 5,
            Self::Incendiary => 6,
            Self::Emp => 7,
            Self::Flare => 8,
            Self::Smoke => 9,
            Self::Scare => 10,
        };
        Some(crate::item::ExplosiveId(id))
    }

    /// The kind an explosive item id names (`None` for unknown ids).
    #[must_use]
    pub const fn from_item_id(id: crate::item::ExplosiveId) -> Option<Self> {
        Some(match id.0 {
            1 => Self::Sticky,
            2 => Self::Cluster,
            3 => Self::C4,
            4 => Self::Shaped,
            5 => Self::Mining,
            6 => Self::Incendiary,
            7 => Self::Emp,
            8 => Self::Flare,
            9 => Self::Smoke,
            10 => Self::Scare,
            _ => return None,
        })
    }

    /// Lower-case plural for counts ("3 sticky bombs").
    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Bomb => "bombs",
            Self::Sticky => "sticky bombs",
            Self::Cluster => "cluster bombs",
            Self::Bomblet => "bomblets",
            Self::C4 => "C4 charges",
            Self::Shaped => "shaped charges",
            Self::Mining => "mining charges",
            Self::Incendiary => "incendiaries",
            Self::Emp => "EMP charges",
            Self::Flare => "flares",
            Self::Smoke => "smoke charges",
            Self::Scare => "scare charges",
            Self::Rocket => "rockets",
        }
    }

    /// One line on what the charge does (shop rows, the selection HUD).
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Bomb => {
                "Timed charge dropped at your feet. Rolls, bounces and sets off neighbours."
            }
            Self::Sticky => "Thrown. Sticks to the first wall, floor or ceiling it hits.",
            Self::Cluster => "Thrown. Bursts into a fan of bomblets that pepper a wide area.",
            Self::Bomblet => "A cluster bomb's submunition.",
            Self::C4 => {
                "Placed on a surface. Sits until you trigger the remote: all at once, or in sequence."
            }
            Self::Shaped => "Placed on a surface. Cuts a long, narrow tunnel straight into it.",
            Self::Mining => {
                "Dropped. A wide blast that clears rock but leaves ore and relics standing."
            }
            Self::Incendiary => {
                "Thrown. A small blast, then fire that eats dirt and clay around it."
            }
            Self::Emp => "Thrown. An electromagnetic pulse: no blast, but machines will feel it.",
            Self::Flare => {
                "Thrown. Burns bright for forty seconds and charts the tunnels around it."
            }
            Self::Smoke => "Thrown. Bursts into a smoke screen that hides you for a dozen seconds.",
            Self::Scare => {
                "Thrown. A deafening bang that breaks nothing but sends critters running."
            }
            Self::Rocket => "A rocket launcher's warhead.",
        }
    }

    /// How a rig deploys it.
    #[must_use]
    pub const fn deployment(self) -> Deployment {
        match self {
            Self::Bomb | Self::Bomblet | Self::Mining | Self::Rocket => Deployment::Drop,
            Self::Sticky
            | Self::Cluster
            | Self::Incendiary
            | Self::Emp
            | Self::Flare
            | Self::Smoke
            | Self::Scare => Deployment::Throw,
            Self::C4 | Self::Shaped => Deployment::Place,
        }
    }

    /// Whether it sticks to the first surface it touches instead of bouncing.
    #[must_use]
    pub const fn sticks(self) -> bool {
        matches!(self, Self::Sticky | Self::C4 | Self::Shaped)
    }

    /// Whether it waits for its owner's remote detonator instead of a fuse.
    #[must_use]
    pub const fn remote(self) -> bool {
        matches!(self, Self::C4)
    }

    /// Fuse from deployment to detonation (remote charges: none, see [`REMOTE_FUSE`]).
    #[must_use]
    pub const fn fuse_millis(self) -> u64 {
        match self {
            Self::Bomb => 2_400,
            Self::Sticky => 2_000,
            Self::Cluster | Self::Shaped => 1_800,
            Self::Bomblet => 650,
            Self::C4 | Self::Rocket => 0,
            Self::Mining => 3_000,
            Self::Incendiary | Self::Emp => 1_500,
            Self::Flare => 350,
            Self::Smoke => 900,
            Self::Scare => 1_200,
        }
    }

    /// Launch speed along the aim (world pixels per second) for thrown and placed charges.
    #[must_use]
    pub const fn launch_speed(self) -> f32 {
        match self.deployment() {
            Deployment::Drop => 0.0,
            Deployment::Throw => 310.0,
            Deployment::Place => 170.0,
        }
    }

    /// Terrain the detonation removes.
    #[must_use]
    pub const fn blast(self) -> BlastShape {
        match self {
            Self::Bomb | Self::Sticky => BlastShape::Diamond(2),
            Self::Cluster | Self::Bomblet | Self::Incendiary | Self::Rocket => {
                BlastShape::Diamond(1)
            }
            Self::C4 => BlastShape::Diamond(3),
            Self::Shaped => BlastShape::Tunnel {
                length: 11,
                half_width: 1,
            },
            Self::Mining => BlastShape::LootSafe(4),
            Self::Emp | Self::Flare | Self::Smoke | Self::Scare => BlastShape::None,
        }
    }

    /// Reach of the blast's push on loose charges and rigs, in world pixels (0: no push).
    #[must_use]
    pub const fn push_reach(self) -> f32 {
        match self {
            Self::Bomb | Self::Sticky => TILE_SIZE * 4.0,
            Self::Cluster | Self::Incendiary | Self::Shaped | Self::Scare | Self::Rocket => {
                TILE_SIZE * 3.0
            }
            Self::Bomblet => TILE_SIZE * 2.0,
            Self::C4 => TILE_SIZE * 5.0,
            Self::Mining => TILE_SIZE * 6.0,
            Self::Emp | Self::Flare | Self::Smoke => 0.0,
        }
    }

    /// Peak push speed (world pixels per second) right at the charge.
    #[must_use]
    pub const fn push_strength(self) -> f32 {
        match self {
            Self::Bomb | Self::Sticky => 460.0,
            Self::Cluster | Self::Incendiary | Self::Shaped | Self::Rocket => 360.0,
            Self::Bomblet => 220.0,
            Self::C4 => 540.0,
            Self::Mining => 420.0,
            Self::Scare => 200.0,
            Self::Emp | Self::Flare | Self::Smoke => 0.0,
        }
    }

    /// Distance within which the blast sets off other charges, in world pixels.
    #[must_use]
    pub const fn chain_reach(self) -> f32 {
        match self {
            Self::Bomb | Self::Sticky | Self::Cluster | Self::Incendiary | Self::Shaped => {
                TILE_SIZE * 2.5
            }
            Self::Bomblet | Self::Rocket => TILE_SIZE * 1.5,
            Self::C4 | Self::Mining => TILE_SIZE * 3.0,
            Self::Emp | Self::Flare | Self::Smoke | Self::Scare => 0.0,
        }
    }

    /// Whether another blast can set it off early (a lit flare or a smoke canister just gets
    /// knocked about).
    #[must_use]
    pub const fn chains(self) -> bool {
        !matches!(self, Self::Flare | Self::Smoke)
    }

    /// Whether it ticks audibly while its fuse burns (flares and smoke do not; a remote charge
    /// only once its detonator has fired, see [`crate::PlacedBomb::awaiting_remote`]).
    #[must_use]
    pub const fn has_audible_fuse(self) -> bool {
        !matches!(self, Self::Flare | Self::Smoke)
    }

    /// Whether it is a weapon of destruction for "no bombs" jobs (flares and smoke are utility).
    #[must_use]
    pub const fn counts_as_bomb(self) -> bool {
        !matches!(self, Self::Flare | Self::Smoke)
    }
}

/// The push a blast of `kind` at `source` gives something at `target` (world pixels), as a
/// velocity change, or `None` when it is out of reach. Callers check line of sight.
///
/// Coincident objects are pushed straight up-and-sideways by `tie_break` (any stable per-object
/// bit, e.g. id parity) instead of dividing by zero. The push fades linearly to nothing at
/// [`ExplosiveKind::push_reach`] and always carries a little lift, so loose things hop.
#[must_use]
pub fn blast_impulse(
    kind: ExplosiveKind,
    source: (f32, f32),
    target: (f32, f32),
    tie_break: bool,
) -> Option<(f32, f32)> {
    let reach = kind.push_reach();
    if reach <= 0.0 {
        return None;
    }
    let dx = target.0 - source.0;
    let dy = target.1 - source.1;
    let distance = dx.hypot(dy);
    if distance >= reach {
        return None;
    }
    let (nx, ny) = if distance < 1.0 {
        (if tie_break { 0.7 } else { -0.7 }, -0.7)
    } else {
        (dx / distance, dy / distance)
    };
    let strength = kind.push_strength() * (1.0 - distance / reach);
    Some((nx * strength, strength.mul_add(ny, -strength * 0.3)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_table_is_unit_length_and_round_trips() {
        for index in 0..ThrowAim::DIRECTIONS {
            let (x, y) = ThrowAim(index).vector();
            assert!((x.hypot(y) - 1.0).abs() < 1e-5, "{index}");
            assert_eq!(
                ThrowAim::from_vector(x * 3.0, y * 3.0),
                Some(ThrowAim(index))
            );
        }
        assert_eq!(ThrowAim::from_vector(0.0, 0.0), None);
        assert_eq!(ThrowAim::DOWN.vector(), (0.0, 1.0));
        assert_eq!(ThrowAim::UP.vector(), (0.0, -1.0));
    }

    #[test]
    fn digital_aim_follows_held_directions_and_facing() {
        assert_eq!(ThrowAim::from_digital(0, 0, 1), ThrowAim(15));
        assert_eq!(ThrowAim::from_digital(0, 0, -1), ThrowAim(9));
        assert_eq!(ThrowAim::from_digital(1, 1, -1), ThrowAim(1));
        assert_eq!(ThrowAim::from_digital(0, 1, 1), ThrowAim::DOWN);
        assert_eq!(ThrowAim::from_digital(-1, -1, 1), ThrowAim(10));
        assert_eq!(ThrowAim::from_digital(0, -1, -1), ThrowAim(11));
    }

    #[test]
    fn every_selectable_kind_but_the_bomb_is_sold_and_prices_scale_with_power() {
        for kind in ExplosiveKind::SELECTABLE {
            assert_eq!(explosive_offer(kind).is_some(), kind != ExplosiveKind::Bomb);
        }
        assert!(explosive_offer(ExplosiveKind::Bomblet).is_none());
        let per_charge = |kind| {
            let offer = explosive_offer(kind).expect("sold");
            offer.cost / offer.count
        };
        // A plain bomb is about 18 credits a charge.
        assert!(per_charge(ExplosiveKind::Flare) < 18);
        for kind in [
            ExplosiveKind::Sticky,
            ExplosiveKind::C4,
            ExplosiveKind::Cluster,
            ExplosiveKind::Shaped,
            ExplosiveKind::Mining,
        ] {
            assert!(per_charge(kind) > 18, "{kind:?}");
        }
        assert!(per_charge(ExplosiveKind::Mining) > per_charge(ExplosiveKind::Sticky));
    }

    #[test]
    fn impulses_fade_with_distance_and_lift() {
        let near =
            blast_impulse(ExplosiveKind::Bomb, (0.0, 0.0), (10.0, 0.0), false).expect("in reach");
        let far =
            blast_impulse(ExplosiveKind::Bomb, (0.0, 0.0), (40.0, 0.0), false).expect("in reach");
        assert!(near.0 > far.0 && far.0 > 0.0);
        assert!(near.1 < 0.0, "a blast always lifts");
        assert!(blast_impulse(ExplosiveKind::Bomb, (0.0, 0.0), (500.0, 0.0), false).is_none());
        assert!(blast_impulse(ExplosiveKind::Emp, (0.0, 0.0), (1.0, 0.0), false).is_none());
    }
}
