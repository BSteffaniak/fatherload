//! Realizers: a [`FloorPlan`] laid out as tiles for one [`SiteStyle`] (WP-TOMBS).
//!
//! **The seam T28 and T29 build on.** A realizer ([`FloorRealizer`]) turns the style-agnostic
//! room graph into a [`RealizedFloor`]: tile cells relative to the run slot's corner (anything
//! not written stays the slot's sealed rock), the rooms' rectangles, where the party arrives,
//! enemy spawn slots ([`SpawnSlot`] with a [`SpawnRole`]) and nothing else. The world stamps the
//! cells into the run slot, spawns guards at once, springs ambushes when a member walks into
//! the room and hands boss and minion kinds out of [`bestiary`].
//!
//! * [`SideViewRealizer`] (T28, [`super::side_view`]): platformer rooms in the site's culture
//!   with trap halls, puzzles, ambushes, secrets, the vault climb, the guardian's arena.
//! * [`super::crypt::CryptRealizer`] (T29): the crypt's room grid seen from above, with
//!   trap overlays, lever gates, the shrine, caches, the crypt lord and the stairs.
//!
//! Tiles are [`BuiltTile`] tomb pieces (masonry [`BuiltTile::TombWall`] in the style's index,
//! traps [`BuiltTile::TombTrap`], the shrine, stairs, exit and caches) plus the ruin
//! mechanisms of WP-PILOT (levers, plates, gates). A floor fits [`MAX_FLOOR_SIZE`].

use serde::{Deserialize, Serialize};

use crate::{
    base::BuiltTile,
    enemy::EnemyKind,
    terrain::{TileKind, TilePosition},
    worldgen::features::TileRect,
};

use super::{
    SiteStyle,
    floor::{FloorPlan, RoomKind},
};

/// Largest floor a realizer may build (columns, rows): the run slot minus a margin.
pub const MAX_FLOOR_SIZE: (i32, i32) = (164, 44);
/// Trap kinds the tile code leaves room for ([`BuiltTile::TombTrap`]).
pub const TRAP_KINDS: u8 = 16;

/// What a trap tile does.
///
/// T28 appended the side-view traps (indices 1-7; their timing and hazards are pure functions
/// in [`super::traps`], applied by `world/tomb_traps.rs`); T29 (top down: arrow plates, flame
/// jets, rolling boulders, pits) appends from index 8 and puts its behaviour in
/// `WorldState::advance_traps`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TrapKind {
    /// Floor spikes (the placeholder trap of both styles): hurt a pilot standing in them.
    Spikes,
    /// T28: the pivot of a swinging blade, set in a ceiling; the blade sweeps below it.
    Blade,
    /// T28: a crusher's housing in a ceiling: it shakes, slams its block down to the floor,
    /// holds, and winds back up.
    Crusher,
    /// T28: a floor slab that crumbles a moment after a pilot steps on it and sets again a
    /// few seconds later (solid until then; a hidden one looks like plain masonry).
    Crumble,
    /// T28: a dart thrower in a wall: it glows, then looses a dart across the room.
    DartWall,
    /// T28: a sluice grate in a sunken room's floor: sand (or water) pours in, rises, holds and
    /// drains on a long cycle.
    SandSluice,
    /// T28: a cursed vent in the floor that gathers and bursts a cloud of grave fog.
    FogVent,
    /// T28: a false wall: masonry a pilot walks through, hiding a secret (trap sight shows it).
    FalseWall,
}

impl TrapKind {
    /// Every kind, by index.
    pub const ALL: [Self; 8] = [
        Self::Spikes,
        Self::Blade,
        Self::Crusher,
        Self::Crumble,
        Self::DartWall,
        Self::SandSluice,
        Self::FogVent,
        Self::FalseWall,
    ];

    /// The kind of tile index `index` (unknown kinds read as spikes).
    #[must_use]
    pub const fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Blade,
            2 => Self::Crusher,
            3 => Self::Crumble,
            4 => Self::DartWall,
            5 => Self::SandSluice,
            6 => Self::FogVent,
            7 => Self::FalseWall,
            _ => Self::Spikes,
        }
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Spikes => 0,
            Self::Blade => 1,
            Self::Crusher => 2,
            Self::Crumble => 3,
            Self::DartWall => 4,
            Self::SandSluice => 5,
            Self::FogVent => 6,
            Self::FalseWall => 7,
        }
    }

    /// A short name (the trap's tile and cues).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spikes => "Spikes",
            Self::Blade => "Swinging Blade",
            Self::Crusher => "Crusher",
            Self::Crumble => "Crumbling Floor",
            Self::DartWall => "Dart Wall",
            Self::SandSluice => "Sand Sluice",
            Self::FogVent => "Cursed Vent",
            Self::FalseWall => "False Wall",
        }
    }

    /// Damage per second to a pilot standing inside the trap's own tile (before relics and
    /// difficulty). The moving traps hurt through their hazard shapes instead
    /// ([`super::traps`]); a pilot never stands inside their tiles.
    #[must_use]
    pub const fn damage_per_second(self) -> f32 {
        match self {
            Self::Spikes => 30.0,
            Self::Blade
            | Self::Crusher
            | Self::Crumble
            | Self::DartWall
            | Self::SandSluice
            | Self::FogVent
            | Self::FalseWall => 0.0,
        }
    }

    /// Whether a pilot passes into the trap's tile: spikes (they hurt) and false walls (they
    /// hide a secret). The rest are set in masonry and stop a pilot like it.
    #[must_use]
    pub const fn pilot_passable(self) -> bool {
        matches!(self, Self::Spikes | Self::FalseWall)
    }
}

/// Who an enemy spawn slot is for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SpawnRole {
    /// Waits in its room from the start.
    Guard,
    /// Springs out when a member walks into the room.
    Ambusher,
    /// The tomb's guardian (boss room, last floor).
    Boss,
}

/// An enemy spawn slot of a realized floor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpawnSlot {
    /// Index of the room in [`RealizedFloor::rooms`].
    pub room: usize,
    /// The open tile the enemy appears in (relative to the slot corner).
    pub at: TilePosition,
    pub role: SpawnRole,
}

/// A realized room: which plan room, and its interior (relative tiles, inclusive).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealizedRoom {
    /// Index into [`FloorPlan::rooms`].
    pub plan: usize,
    pub kind: RoomKind,
    pub interior: TileRect,
}

/// A floor as tiles, relative to the run slot's top-left corner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RealizedFloor {
    pub style: SiteStyle,
    /// Size of the box the cells fill (columns, rows).
    pub size: (i32, i32),
    /// Every tile written; the rest of the slot stays sealed rock.
    pub cells: Vec<(TilePosition, TileKind)>,
    pub rooms: Vec<RealizedRoom>,
    /// Where the party's feet stand on arrival: the open tile above the floor.
    pub start: TilePosition,
    pub spawns: Vec<SpawnSlot>,
}

impl RealizedFloor {
    /// The room whose interior holds `tile` (relative), if any.
    #[must_use]
    pub fn room_at(&self, tile: TilePosition) -> Option<usize> {
        self.rooms
            .iter()
            .position(|room| room.interior.contains(tile))
    }

    /// The tile kind written at `tile` (relative), if any.
    #[must_use]
    pub fn cell(&self, tile: TilePosition) -> Option<TileKind> {
        self.cells
            .iter()
            .find(|(at, _)| *at == tile)
            .map(|(_, kind)| *kind)
    }
}

/// Lays a floor plan out as tiles for one style. Implementations must be pure (the same plan
/// always gives the same floor on every peer) and stay inside [`MAX_FLOOR_SIZE`].
pub trait FloorRealizer: Sync {
    /// The style this realizer builds.
    fn style(&self) -> SiteStyle;
    /// The floor for `plan`.
    fn realize(&self, plan: &FloorPlan) -> RealizedFloor;
}

pub use super::side_view::SideViewRealizer;

/// The realizer for `style` (the one place T28 / T29 swap theirs in).
#[must_use]
pub fn realizer_for(style: SiteStyle) -> &'static dyn FloorRealizer {
    match style {
        SiteStyle::SideView => &SideViewRealizer,
        // T29: the crypt's room grid.
        SiteStyle::TopDown => &super::crypt::CryptRealizer,
    }
}

/// The enemy kind for a spawn slot: each style's own bestiary.
///
/// Side view: T28's mummies, crypt bats, skeletal archers, animated statues and the two tomb
/// guardians ([`super::side_view::bestiary`]); top down: T29's skeleton packs, spectres,
/// cultists, golems and the crypt lord ([`super::crypt::bestiary`]).
#[must_use]
pub const fn bestiary(style: SiteStyle, role: SpawnRole, variant: u32) -> EnemyKind {
    match (style, role) {
        // T28: mummies, crypt bats, skeletal archers, animated statues and two guardians.
        (SiteStyle::SideView, _) => super::side_view::bestiary(role, variant),
        // T29: skeleton packs, spectres, cultists, golems and the crypt lord.
        (SiteStyle::TopDown, role) => super::crypt::bestiary(role, variant),
    }
}

/// The boss's health multiplier over its kind's stock health, by site tier.
///
/// A mesa tomb's guardian (tier 1, entered with starter gear) has half its stock health, a
/// lunar one half again as much. The probe (`--balance-report --only tombs`) showed a flat 1.5x
/// guardian ending nearly every tier-1 run at the boss.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a handful of tiers")]
pub fn boss_health_scale(site_tier: u8) -> f32 {
    f32::from(site_tier.clamp(1, super::MAX_TIER)).mul_add(0.25, 0.25)
}

/// A cell writer that keeps the latest write per tile and walls in every open cell.
pub(super) struct Canvas {
    /// The masonry set walls are written in (the style's index, or a T28 culture's set).
    masonry: u8,
    pub(super) cells: std::collections::BTreeMap<TilePosition, TileKind>,
}

impl Canvas {
    /// A canvas whose walls are masonry set `masonry` ([`BuiltTile::TombWall`]).
    pub(super) const fn with_masonry(masonry: u8) -> Self {
        Self {
            masonry,
            cells: std::collections::BTreeMap::new(),
        }
    }

    pub(super) const fn wall(&self) -> TileKind {
        TileKind::Built(BuiltTile::TombWall(self.masonry))
    }

    pub(super) fn put(&mut self, x: i32, y: i32, kind: TileKind) {
        let _ = self.cells.insert(TilePosition { x, y }, kind);
    }

    /// Writes masonry into every cell of `rect` (inclusive) not written yet (a room's hull).
    pub(super) fn hull(&mut self, rect: TileRect) {
        let wall = self.wall();
        for y in rect.min.y..=rect.max.y {
            for x in rect.min.x..=rect.max.x {
                self.cells.entry(TilePosition { x, y }).or_insert(wall);
            }
        }
    }

    /// Opens `rect` (inclusive) to air.
    pub(super) fn open(&mut self, rect: TileRect) {
        for y in rect.min.y..=rect.max.y {
            for x in rect.min.x..=rect.max.x {
                self.put(x, y, TileKind::Air);
            }
        }
    }

    /// Moves every spawn slot standing in something solid to the nearest open cell of its
    /// room's canvas (within three tiles).
    pub(super) fn settle(&self, spawns: &mut [SpawnSlot]) {
        for spawn in spawns {
            if self.cells.get(&spawn.at) == Some(&TileKind::Air) {
                continue;
            }
            let found = (1..=3).find_map(|reach| {
                (-reach..=reach)
                    .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
                    .map(|(dx, dy)| TilePosition {
                        x: spawn.at.x + dx,
                        y: spawn.at.y + dy,
                    })
                    .find(|tile| self.cells.get(tile) == Some(&TileKind::Air))
            });
            if let Some(tile) = found {
                spawn.at = tile;
            }
        }
    }

    /// Walls in every open (non-wall) cell's neighbours that were never written, then returns
    /// the cells in tile order.
    pub(super) fn finish(mut self) -> Vec<(TilePosition, TileKind)> {
        let wall = self.wall();
        let open: Vec<TilePosition> = self
            .cells
            .iter()
            .filter(|(_, kind)| **kind != wall)
            .map(|(tile, _)| *tile)
            .collect();
        for tile in open {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let near = TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    };
                    self.cells.entry(near).or_insert(wall);
                }
            }
        }
        self.cells.into_iter().collect()
    }
}

pub(super) const fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> TileRect {
    TileRect {
        min: TilePosition { x: x0, y: y0 },
        max: TilePosition { x: x1, y: y1 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tomb::floor::plan_floor;

    #[test]
    fn every_plan_realizes_inside_the_slot_with_every_room() {
        for style in SiteStyle::ALL {
            for tier in 1..=5 {
                let site = crate::tomb::floor::tests::site(style, tier);
                for run in 0..4 {
                    for floor in 0..site.floors() {
                        let plan = plan_floor(&site, run, floor);
                        let realized = realizer_for(style).realize(&plan);
                        assert_eq!(realized.style, style);
                        assert!(realized.size.0 <= MAX_FLOOR_SIZE.0, "{:?}", realized.size);
                        assert!(realized.size.1 <= MAX_FLOOR_SIZE.1, "{:?}", realized.size);
                        for (tile, _) in &realized.cells {
                            assert!(
                                tile.x >= 0
                                    && tile.y >= 0
                                    && tile.x < MAX_FLOOR_SIZE.0
                                    && tile.y < MAX_FLOOR_SIZE.1,
                                "{tile:?} outside the slot"
                            );
                        }
                        assert_eq!(realized.rooms.len(), plan.rooms.len());
                        assert_eq!(realized.cell(realized.start), Some(TileKind::Air));
                        let has = |kind: BuiltTile| {
                            realized
                                .cells
                                .iter()
                                .any(|(_, cell)| *cell == TileKind::Built(kind))
                        };
                        assert!(has(BuiltTile::TombStairs));
                        assert!(has(BuiltTile::TombShrine { lit: false }));
                        assert!(has(BuiltTile::TombCache { open: false }));
                        assert_eq!(has(BuiltTile::TombExit), floor == 0);
                        assert_eq!(
                            realized
                                .spawns
                                .iter()
                                .filter(|spawn| spawn.role == SpawnRole::Boss)
                                .count(),
                            usize::from(plan.is_last())
                        );
                        for spawn in &realized.spawns {
                            assert_eq!(realized.cell(spawn.at), Some(TileKind::Air));
                        }
                        // Pure.
                        assert_eq!(realized, realizer_for(style).realize(&plan));
                    }
                }
            }
        }
    }
}
