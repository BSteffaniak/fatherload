//! The style-agnostic floor grammar of a tomb run (WP-TOMBS).
//!
//! A floor is a small room graph ([`FloorPlan`]): a main path from the [`RoomKind::Entry`] to
//! the [`RoomKind::Stairs`], made of trap halls, puzzle rooms and ambushes with a
//! [`RoomKind::Shrine`] checkpoint in the middle, a [`RoomKind::Vault`] branching off the path,
//! and on the last floor a [`RoomKind::Boss`] room before the way out. Plans are pure functions
//! of the site, the site's run counter and the floor number ([`plan_floor`]), so a run is fresh
//! every time but every peer builds exactly the same floor.
//!
//! Plans know nothing about tiles: a [`super::realize::FloorRealizer`] lays them out for one
//! [`SiteStyle`]. Each room carries a `variant` roll (which puzzle, which trap pattern) and a
//! `danger` (difficulty ramping per floor and along the path) for realizers and spawn tables.

use serde::{Deserialize, Serialize};

use crate::worldgen::noise::RegionRng;

use super::{SiteStyle, TombSite};

const SALT_FLOOR: u64 = 0x70B5_F100_0001;

/// What a room is for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RoomKind {
    /// Where the party arrives (on the first floor: the way back out, [`crate::base::BuiltTile::TombExit`]).
    Entry,
    /// A hall of traps to cross.
    TrapHall,
    /// A gate held shut by a mechanism (a lever or a pressure plate) somewhere in the room.
    Puzzle,
    /// A quiet room that springs an ambush when the party walks in.
    Ambush,
    /// A shrine checkpoint: banks unbanked loot, revives the downed, saves the run.
    Shrine,
    /// The treasure vault: caches of run loot, off the main path.
    Vault,
    /// The tomb's guardian (last floor only).
    Boss,
    /// The way down (or, on the last floor, out with everything).
    Stairs,
}

impl RoomKind {
    pub const ALL: [Self; 8] = [
        Self::Entry,
        Self::TrapHall,
        Self::Puzzle,
        Self::Ambush,
        Self::Shrine,
        Self::Vault,
        Self::Boss,
        Self::Stairs,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Entry => "Entry",
            Self::TrapHall => "Trap Hall",
            Self::Puzzle => "Puzzle Room",
            Self::Ambush => "Ambush",
            Self::Shrine => "Shrine",
            Self::Vault => "Treasure Vault",
            Self::Boss => "Guardian's Crypt",
            Self::Stairs => "Stairs",
        }
    }
}

/// One room of a floor plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoomNode {
    pub kind: RoomKind,
    /// Rooms this one opens onto (indices into [`FloorPlan::rooms`]); the main path links each
    /// room to the next, a branch room is linked from its parent.
    pub links: Vec<u8>,
    /// Off the main path (the vault).
    pub branch: bool,
    /// Difficulty of this room: the floor's difficulty plus its place along the path.
    pub danger: u8,
    /// A free roll for realizers (which puzzle, which trap pattern, how wide).
    pub variant: u32,
}

/// A floor's room graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FloorPlan {
    pub style: SiteStyle,
    pub tier: u8,
    /// 0-based floor number.
    pub floor: u8,
    /// Floors of this run.
    pub floors: u8,
    /// The floor's seed (realizers roll from it).
    pub seed: u64,
    /// The floor's difficulty (tier and depth into the run).
    pub difficulty: u8,
    /// Rooms; the main path is every non-branch room in order, entry first.
    pub rooms: Vec<RoomNode>,
}

impl FloorPlan {
    /// Whether this is the run's last floor (it has the boss).
    #[must_use]
    pub const fn is_last(&self) -> bool {
        self.floor + 1 >= self.floors
    }

    /// The main path's room indices, entry first.
    #[must_use]
    pub fn main_path(&self) -> Vec<usize> {
        let mut path = Vec::new();
        let mut at = 0_usize;
        loop {
            path.push(at);
            let Some(next) = self.rooms[at]
                .links
                .iter()
                .map(|link| usize::from(*link))
                .find(|link| !self.rooms[*link].branch)
            else {
                break;
            };
            at = next;
        }
        path
    }

    /// The branch rooms off main-path room `room`.
    #[must_use]
    pub fn branches_of(&self, room: usize) -> Vec<usize> {
        self.rooms[room]
            .links
            .iter()
            .map(|link| usize::from(*link))
            .filter(|link| self.rooms[*link].branch)
            .collect()
    }

    /// How many rooms of `kind` the plan has.
    #[must_use]
    pub fn count(&self, kind: RoomKind) -> usize {
        self.rooms.iter().filter(|room| room.kind == kind).count()
    }
}

/// The seed of floor `floor` of run `run` of a site with seed `site_seed`.
#[must_use]
pub const fn floor_seed(site_seed: u64, run: u32, floor: u8) -> u64 {
    #[allow(clippy::cast_possible_wrap, reason = "run counters are raw bits")]
    let run = run as i32;
    crate::worldgen::noise::hash2(site_seed, SALT_FLOOR, run, floor as i32)
}

/// The difficulty of floor `floor` (0-based) of a tier-`tier` site.
#[must_use]
pub const fn floor_difficulty(tier: u8, floor: u8) -> u8 {
    tier.saturating_mul(2).saturating_add(floor)
}

/// Middle rooms (between the entry and the stairs, shrine included) for a floor.
const fn middle_rooms(tier: u8, floor: u8) -> usize {
    let count = 3 + (floor as usize) / 2 + (tier as usize) / 2;
    if count > 7 { 7 } else { count }
}

/// Plans floor `floor` (0-based) of run `run` of `site` (pure).
#[must_use]
pub fn plan_floor(site: &TombSite, run: u32, floor: u8) -> FloorPlan {
    let floors = site.floors();
    let seed = floor_seed(site.seed, run, floor);
    let difficulty = floor_difficulty(site.tier, floor);
    let mut rng = RegionRng::new(seed, SALT_FLOOR, i32::from(floor), 0);
    let last = floor + 1 >= floors;

    // The middle of the main path: the first three are a shuffle of one of each challenge,
    // the rest rolled; the shrine goes in the middle.
    let count = middle_rooms(site.tier, floor);
    let mut challenges = vec![RoomKind::TrapHall, RoomKind::Puzzle, RoomKind::Ambush];
    for index in (1..challenges.len()).rev() {
        let pick = usize::try_from(rng.next_u64() % (index as u64 + 1)).unwrap_or(0);
        challenges.swap(index, pick);
    }
    while challenges.len() < count - 1 {
        let kind = match rng.pick(&[4, 3, 3 + u32::from(difficulty / 3)]) {
            Some(0) => RoomKind::TrapHall,
            Some(1) => RoomKind::Puzzle,
            _ => RoomKind::Ambush,
        };
        challenges.push(kind);
    }
    let shrine_at = challenges.len().div_ceil(2);
    challenges.insert(shrine_at, RoomKind::Shrine);

    let mut kinds = vec![RoomKind::Entry];
    kinds.extend(challenges);
    if last {
        kinds.push(RoomKind::Boss);
    }
    kinds.push(RoomKind::Stairs);

    let path_len = kinds.len();
    let mut rooms: Vec<RoomNode> = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| RoomNode {
            kind: *kind,
            links: if index + 1 < path_len {
                vec![u8::try_from(index + 1).unwrap_or(u8::MAX)]
            } else {
                Vec::new()
            },
            branch: false,
            danger: difficulty.saturating_add(u8::try_from(index * 2 / path_len).unwrap_or(0)),
            variant: u32::try_from(rng.next_u64() & 0xFFFF_FFFF).unwrap_or(0),
        })
        .collect();

    // The vault branches off a puzzle or ambush room in the second half of the path (a trap
    // hall's pits would sit under the climb).
    let first = path_len / 2;
    let mut candidates: Vec<usize> = (first..path_len - 1)
        .filter(|index| matches!(rooms[*index].kind, RoomKind::Puzzle | RoomKind::Ambush))
        .collect();
    if candidates.is_empty() {
        candidates = (1..path_len - 1)
            .filter(|index| matches!(rooms[*index].kind, RoomKind::Puzzle | RoomKind::Ambush))
            .collect();
    }
    let parent = if candidates.is_empty() {
        1
    } else {
        candidates[usize::try_from(rng.next_u64() % candidates.len() as u64).unwrap_or(0)]
    };
    let vault = rooms.len();
    rooms.push(RoomNode {
        kind: RoomKind::Vault,
        links: Vec::new(),
        branch: true,
        danger: rooms[parent].danger,
        variant: u32::try_from(rng.next_u64() & 0xFFFF_FFFF).unwrap_or(0),
    });
    rooms[parent]
        .links
        .push(u8::try_from(vault).unwrap_or(u8::MAX));

    FloorPlan {
        style: site.style,
        tier: site.tier,
        floor,
        floors,
        seed,
        difficulty,
        rooms,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        terrain::TilePosition,
        tomb::{SiteId, SiteOpening, SiteSetting},
    };

    pub fn site(style: SiteStyle, tier: u8) -> TombSite {
        TombSite {
            id: SiteId::new(SiteSetting::Deep, 1, 2),
            setting: SiteSetting::Deep,
            style,
            tier,
            opening: SiteOpening::Key,
            origin: TilePosition { x: 0, y: 0 },
            door: TilePosition { x: 5, y: 5 },
            mirrored: false,
            seed: 0x1234_5678,
        }
    }

    #[test]
    fn plans_follow_the_grammar() {
        for style in SiteStyle::ALL {
            for tier in 1..=5 {
                let site = site(style, tier);
                for run in 0..6 {
                    for floor in 0..site.floors() {
                        let plan = plan_floor(&site, run, floor);
                        let path = plan.main_path();
                        assert_eq!(plan.rooms[path[0]].kind, RoomKind::Entry);
                        assert_eq!(plan.rooms[*path.last().unwrap()].kind, RoomKind::Stairs);
                        assert_eq!(plan.count(RoomKind::Shrine), 1);
                        assert_eq!(plan.count(RoomKind::Vault), 1);
                        assert_eq!(plan.count(RoomKind::Boss), usize::from(plan.is_last()));
                        for kind in [RoomKind::TrapHall, RoomKind::Puzzle, RoomKind::Ambush] {
                            assert!(plan.count(kind) >= 1, "{kind:?} missing");
                        }
                        // Every room is reachable: the path plus one branch.
                        assert_eq!(path.len() + 1, plan.rooms.len());
                        if plan.is_last() {
                            assert_eq!(plan.rooms[path[path.len() - 2]].kind, RoomKind::Boss);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn plans_are_deterministic_and_fresh_per_run() {
        let site = site(SiteStyle::SideView, 3);
        assert_eq!(plan_floor(&site, 4, 1), plan_floor(&site, 4, 1));
        let differs = (0..8).any(|run| plan_floor(&site, run, 0) != plan_floor(&site, run + 1, 0));
        assert!(differs, "runs should differ");
        // Difficulty ramps per floor.
        assert!(plan_floor(&site, 0, 2).difficulty > plan_floor(&site, 0, 0).difficulty);
    }
}
