//! Ground raids: a faction's dropship sets troopers down on a body.
//!
//! They land near where people are ([`RaidSite`]): by an alien town a rig is in or near (Wave
//! 12 integration: the raiders come for the town, and its folk's safe ground does not hold
//! them back), else near a rig on the ground. A player's base or home still keeps them out.
//!
//! The troopers are ordinary creatures of the body's store (raid corsairs, swarm skitters and
//! machine troopers: [`crate::enemy::EnemyKind::RaidCorsair`] and its siblings): they keep the body rules every creature keeps (`step_body`,
//! `free_from_rock`, `keep_off_bodies`, `site_fits`, contact boxes matching the drawn body,
//! line-of-sight noticing), in the body's local frame where gravity points at its core.
//!
//! Pacing: a body store with someone on its ground rolls every [`RAID_ROLL_STEPS`] once the
//! store's cooldown ([`RAID_REST_STEPS`]) has passed; bodies in the home sector are never
//! raided; the chance rises with the galactic ring.

use crate::{enemy::EnemyKind, space::Theme};

use super::{Faction, steps};

/// Steps between raid rolls on an occupied body (20 s).
pub const RAID_ROLL_STEPS: u64 = steps(20.0);
/// Steps a body rests after a raid (6 minutes).
pub const RAID_REST_STEPS: u64 = steps(360.0);
/// Most troopers in one raid.
pub const MAX_TROOPERS: u32 = 6;
/// Troopers land this many columns from the site (either side).
pub const LANDING_OFFSET: i32 = 15;
/// Troopers drop from this many rows above the ground.
pub const DROP_ROWS: i32 = 10;
/// Columns past an alien town's edge within which a rig draws a raid onto the town.
pub const TOWN_RAID_REACH: i32 = 90;

/// A place raiders come for: an alien town (its middle and ground) or a rig on the ground.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RaidSite {
    /// Strip column and the row of its ground.
    pub column: i32,
    pub row: i32,
}

/// The chance a raid roll lands at ring `ring`, per mille.
#[must_use]
pub const fn raid_chance_permille(ring: u8) -> u32 {
    match ring {
        0 | 1 => 0,
        _ => {
            let chance = 60 + 40 * (ring as u32 - 2);
            if chance > 300 { 300 } else { chance }
        }
    }
}

/// The faction that raids a body of `theme`.
#[must_use]
pub const fn raiders_of(theme: Theme, roll: u64) -> Faction {
    match theme {
        Theme::Jungle | Theme::Fungal | Theme::OceanArchipelago => Faction::Swarm,
        Theme::MachineWorld | Theme::DeadWorld => Faction::Machine,
        _ => {
            if roll.is_multiple_of(5) {
                Faction::Machine
            } else {
                Faction::Pirates
            }
        }
    }
}

/// The trooper a faction drops.
#[must_use]
pub const fn trooper(faction: Faction) -> EnemyKind {
    match faction {
        Faction::Pirates => EnemyKind::RaidCorsair,
        Faction::Swarm => EnemyKind::SwarmSkitter,
        Faction::Machine => EnemyKind::MachineTrooper,
    }
}

/// How many troopers land at ring `ring`.
#[must_use]
pub const fn trooper_count(ring: u8) -> u32 {
    let count = 2 + ring as u32 / 2;
    if count > MAX_TROOPERS {
        MAX_TROOPERS
    } else {
        count
    }
}

/// Whether `kind` is a raid trooper.
#[must_use]
pub const fn is_trooper(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::RaidCorsair | EnemyKind::SwarmSkitter | EnemyKind::MachineTrooper
    )
}
