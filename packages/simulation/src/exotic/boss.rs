//! The exotic worlds' theme bosses: one big readable creature at each world's landmark,
//! telegraphed, fair and winnable (`world/enemies/exotic.rs` runs the fight).
//!
//! | World | Boss | Arena | Attacks (each told first) | Opening |
//! |---|---|---|---|---|
//! | gas giant | **the Thunderhead**, a living storm cloud with a lightning heart | the Eye's calm well, over its cloud platform | a **bolt salvo** (three columns flicker, then strike; five when it rages); a **gale** (it inhales, then blows craft away from it); a **dive** at the nearest rig (its heart flares) | after a salvo its heart lies open |
//! | locked world | **the Gloamwarden**, a colossus half scorched and half frozen | the Gloam Gate's level arena | a **sun slam** (it raises its burning arm; the shockwave runs along the ground: be off it); **frost shards** (columns frost over, then shards fall down them) | after a slam its chest cracks open |
//! | ring world | **the Ring Warden**, a builder sentinel ring round a core | the Tether's plaza | a **beam sweep** (a row glows across the arena, then burns: change height); a **drone call** (two tether drones, at most three) | after a sweep its core vents |
//!
//! * **Waking**: a live rig steps into the arena while the seal still stands ([`super::ExoticTile::BossSeal`]).
//!   Leaving the arena (every rig well outside it) puts the boss back to sleep at full health:
//!   nothing is lost by running and nothing counts down.
//! * **Telling**: every attack winds up for at least [`TELL_MILLIS`] (at least
//!   [`RAGE_TELL_MILLIS`] once it rages below half health), with an event and marks where it
//!   will land ([`BossMark`]); then it strikes once and rests.
//! * **Winning**: it takes [`EXPOSED_DAMAGE_FACTOR`] times the damage while its weak point is
//!   open (after its signature attack). Beaten, its seal turns into its trophy (an artifact to
//!   dig out) and it drops its hoard; the terrain keeps the win for good.
//! * **Peaceful** worlds have no creatures, so a rig that reaches the arena opens the seal.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use serde::{Deserialize, Serialize};

use super::{ExoticStyle, ExoticTile, gas_giant, locked, ring, style_of_ground};
use crate::{
    enemy::EnemyKind,
    planet::{BodyGround, PlanetGround},
    terrain::TilePosition,
};

/// The bosses of the exotic themes, for the endgame's hunt list (TODO-SEAM w13-endgame: the
/// hunt reads this list and [`crate::WorldState::exotic_boss_beaten`] for each body it
/// names).
pub const THEME_BOSSES: [(ExoticStyle, EnemyKind, &str); 3] = [
    (
        ExoticStyle::GasGiant,
        EnemyKind::Thunderhead,
        "the Thunderhead",
    ),
    (
        ExoticStyle::Locked,
        EnemyKind::Gloamwarden,
        "the Gloamwarden",
    ),
    (ExoticStyle::Ring, EnemyKind::RingWarden, "the Ring Warden"),
];

/// A boss's attack windup (milliseconds), and while it rages.
pub const TELL_MILLIS: u64 = 1_500;
pub const RAGE_TELL_MILLIS: u64 = 1_100;
/// How long it rests between attacks.
pub const REST_MILLIS: u64 = 1_700;
/// How long a strike lasts (a gale blows longer).
pub const STRIKE_MILLIS: u64 = 400;
pub const GALE_MILLIS: u64 = 1_800;
/// How long its weak point stays open after its signature attack.
pub const EXPOSED_MILLIS: u64 = 2_600;
/// Damage multiplier while the weak point is open.
pub const EXPOSED_DAMAGE_FACTOR: f32 = 2.5;
/// Hull its blows take (before the tier's scale): a bolt, the slam's shockwave, a shard, the
/// beam.
pub const BOLT_DAMAGE: f32 = 18.0;
pub const SLAM_DAMAGE: f32 = 22.0;
pub const SHARD_DAMAGE: f32 = 16.0;
pub const BEAM_DAMAGE: f32 = 20.0;
/// How far the slam's shockwave runs (tiles either side) and how high over the floor it
/// reaches (rows: be higher and it passes under you).
pub const SLAM_REACH: i32 = 16;
pub const SLAM_HEIGHT_ROWS: i32 = 2;
/// The gale's push (pixels a second, away from the boss).
pub const GALE_PUSH: f32 = 240.0;
/// The most tether drones it keeps.
pub const MAX_DRONES: usize = 3;
/// The tier its health and blows scale by.
pub const BOSS_TIER: u8 = 10;

/// One of a boss's attacks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BossAttack {
    BoltSalvo,
    Gale,
    Dive,
    SunSlam,
    FrostShards,
    BeamSweep,
    DroneCall,
}

impl BossAttack {
    /// The attack its weak point opens after.
    #[must_use]
    pub const fn opens(self) -> bool {
        matches!(self, Self::BoltSalvo | Self::SunSlam | Self::BeamSweep)
    }

    /// Its index in the fight state.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The attack of code `code`.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::BoltSalvo),
            1 => Some(Self::Gale),
            2 => Some(Self::Dive),
            3 => Some(Self::SunSlam),
            4 => Some(Self::FrostShards),
            5 => Some(Self::BeamSweep),
            6 => Some(Self::DroneCall),
            _ => None,
        }
    }
}

/// The attack pattern of `style`'s boss (it cycles).
#[must_use]
pub const fn pattern(style: ExoticStyle) -> &'static [BossAttack] {
    match style {
        ExoticStyle::GasGiant => &[BossAttack::BoltSalvo, BossAttack::Gale, BossAttack::Dive],
        ExoticStyle::Locked => &[
            BossAttack::SunSlam,
            BossAttack::FrostShards,
            BossAttack::FrostShards,
        ],
        ExoticStyle::Ring => &[
            BossAttack::BeamSweep,
            BossAttack::DroneCall,
            BossAttack::BeamSweep,
        ],
    }
}

/// Where a boss's fight is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Arena {
    pub style: ExoticStyle,
    /// The seal's tile (the trophy's, once won).
    pub seal: TilePosition,
    /// Where the boss waits (its home tile).
    pub home: TilePosition,
    /// The arena's middle column and half width (columns), its top and its floor rows.
    pub centre: i32,
    pub half_width: i32,
    pub top: i32,
    pub floor: i32,
}

impl Arena {
    /// Whether `tile` is in the arena (`margin` tiles round it too).
    #[must_use]
    pub fn contains(&self, tile: TilePosition, width: i32, margin: i32) -> bool {
        let dx = super::wrap_delta(i64::from(tile.x), i64::from(self.centre), i64::from(width));
        dx.abs() <= i64::from(self.half_width + margin)
            && tile.y >= self.top - margin
            && tile.y <= self.floor + margin
    }
}

/// The boss arena of `ground`, if it is an exotic world.
#[must_use]
pub fn arena(ground: &PlanetGround) -> Option<Arena> {
    let style = style_of_ground(ground)?;
    Some(match style {
        ExoticStyle::GasGiant => {
            let centre = i32::try_from(gas_giant::eye_column(ground)).unwrap_or(0);
            Arena {
                style,
                seal: TilePosition {
                    x: centre,
                    y: gas_giant::EYE_FLOOR - 1,
                },
                home: TilePosition {
                    x: centre,
                    y: gas_giant::EYE_FLOOR - 14,
                },
                centre,
                half_width: i32::try_from(gas_giant::EYE_HALF).unwrap_or(34) - 2,
                top: gas_giant::EYE_TOP,
                floor: gas_giant::EYE_FLOOR + 2,
            }
        }
        ExoticStyle::Locked => {
            let centre = i32::try_from(locked::gate_column(ground)).unwrap_or(0);
            let floor = ground.surface_row(centre);
            Arena {
                style,
                seal: TilePosition {
                    x: centre,
                    y: floor - 1,
                },
                home: TilePosition {
                    x: centre - 10,
                    y: floor - 4,
                },
                centre,
                half_width: i32::try_from(locked::GATE_HALF).unwrap_or(40),
                top: floor - 36,
                floor,
            }
        }
        ExoticStyle::Ring => {
            let centre = i32::try_from(ring::tether_column(ground)).unwrap_or(0);
            let floor = ground.surface_row(centre);
            Arena {
                style,
                seal: TilePosition {
                    x: centre + i32::try_from(ring::SEAL_OFFSET).unwrap_or(9),
                    y: floor - 1,
                },
                home: TilePosition {
                    x: centre - 14,
                    y: floor - 14,
                },
                centre,
                half_width: i32::try_from(ring::PLAZA_HALF).unwrap_or(40),
                top: floor - 40,
                floor,
            }
        }
    })
}

/// The trophy the seal of `style`'s boss turns into.
#[must_use]
pub const fn trophy(style: ExoticStyle) -> ExoticTile {
    match style {
        ExoticStyle::GasGiant => ExoticTile::StormTrophy,
        ExoticStyle::Locked => ExoticTile::GloamTrophy,
        ExoticStyle::Ring => ExoticTile::AnchorTrophy,
    }
}

/// A mark the renderer draws while a boss tells an attack (and the rules strike).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BossMark {
    /// A column of the arena (a bolt, a falling shard): `column`, from `top` to `bottom`.
    Column { column: i32, top: i32, bottom: i32 },
    /// A row across the arena (the beam): `row`, from `left` to `right` columns.
    Row { row: i32, left: i32, right: i32 },
    /// The floor's shockwave: `left..=right` columns along `floor`.
    Floor { floor: i32, left: i32, right: i32 },
    /// A gale round the boss (`centre`).
    Gale { centre: TilePosition },
}

/// Where `attack` lands when told at `target` (the rig it picked) in `arena`, raging or not.
#[must_use]
pub fn marks(arena: &Arena, attack: BossAttack, target: TilePosition, rage: bool) -> Vec<BossMark> {
    let left = arena.centre - arena.half_width;
    let right = arena.centre + arena.half_width;
    match attack {
        BossAttack::BoltSalvo | BossAttack::FrostShards => {
            let (count, gap) = match (attack, rage) {
                (BossAttack::BoltSalvo, false) => (3, 7),
                (BossAttack::BoltSalvo, true) => (5, 6),
                (_, false) => (3, 5),
                (_, true) => (4, 5),
            };
            let first = target.x - gap * (count - 1) / 2;
            (0..count)
                .map(|index| BossMark::Column {
                    column: (first + index * gap).clamp(left, right),
                    top: arena.top,
                    bottom: arena.floor,
                })
                .collect()
        }
        BossAttack::SunSlam => vec![BossMark::Floor {
            floor: arena.floor,
            left: (arena.home.x - SLAM_REACH).max(left),
            right: (arena.home.x + SLAM_REACH).min(right),
        }],
        BossAttack::BeamSweep => vec![BossMark::Row {
            row: target.y.clamp(arena.top, arena.floor - 1),
            left,
            right,
        }],
        BossAttack::Gale => vec![BossMark::Gale { centre: arena.home }],
        BossAttack::Dive | BossAttack::DroneCall => Vec::new(),
    }
}

/// Whether `tile` is struck by `mark` (with `width` the strip's width).
#[must_use]
pub fn mark_hits(mark: &BossMark, tile: TilePosition, width: i32) -> bool {
    let dx =
        |column: i32| super::wrap_delta(i64::from(tile.x), i64::from(column), i64::from(width));
    match *mark {
        BossMark::Column {
            column,
            top,
            bottom,
        } => dx(column).abs() <= 1 && (top..=bottom).contains(&tile.y),
        BossMark::Row { row, left, right } => {
            (tile.y - row).abs() <= 1 && dx(left) >= 0 && dx(right) <= 0
        }
        BossMark::Floor { floor, left, right } => {
            tile.y < floor && tile.y >= floor - SLAM_HEIGHT_ROWS && dx(left) >= 0 && dx(right) <= 0
        }
        BossMark::Gale { .. } => false,
    }
}
