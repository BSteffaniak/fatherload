//! Realizers: a [`FloorPlan`] laid out as tiles for one [`SiteStyle`] (WP-TOMBS).
//!
//! **The seam T28 and T29 build on.** A realizer ([`FloorRealizer`]) turns the style-agnostic
//! room graph into a [`RealizedFloor`]: tile cells relative to the run slot's corner (anything
//! not written stays the slot's sealed rock), the rooms' rectangles, where the party arrives,
//! enemy spawn slots ([`SpawnSlot`] with a [`SpawnRole`]) and nothing else. The world stamps the
//! cells into the run slot, spawns guards at once, springs ambushes when a member walks into
//! the room and hands boss and minion kinds out of [`bestiary`].
//!
//! * [`SideViewRealizer`] (T28 replaces the contents, keeps the trait): platformer rooms in a
//!   row on one floor line, doorways three tiles tall, the vault up a stair of ledges above its
//!   parent room, trap halls as spike pits, puzzle rooms gated by a lever or a pressure plate,
//!   the shrine, caches, the boss room and the stairs. Everything is reachable with the stock
//!   pilot (one-tile steps and two-tile jumps).
//! * [`TopDownRealizer`] (T29 replaces it): a row of square rooms seen from above with the
//!   vault north of its parent, walked in the pilot's top-down placeholder mode (no gravity,
//!   8-way): spike floors, lever gates on the east doorways, the shrine, caches, the boss and
//!   the stairs.
//!
//! Tiles are [`BuiltTile`] tomb pieces (masonry [`BuiltTile::TombWall`] in the style's index,
//! traps [`BuiltTile::TombTrap`], the shrine, stairs, exit and caches) plus the ruin
//! mechanisms of WP-PILOT (levers, plates, gates). A floor fits [`MAX_FLOOR_SIZE`].

use serde::{Deserialize, Serialize};

use crate::{
    base::BuiltTile,
    enemy::EnemyKind,
    terrain::{TileKind, TilePosition},
    worldgen::{features::TileRect, noise::RegionRng},
};

use super::{
    SiteStyle,
    floor::{FloorPlan, RoomKind},
};

/// Largest floor a realizer may build (columns, rows): the run slot minus a margin.
pub const MAX_FLOOR_SIZE: (i32, i32) = (164, 44);
/// Trap kinds the tile code leaves room for ([`BuiltTile::TombTrap`]).
pub const TRAP_KINDS: u8 = 16;

const SALT_REALIZE: u64 = 0x70B5_4EA1_0002;

/// What a trap tile does.
///
/// T28 (side view: swinging blades, crushers, dart walls, rising sand, cursed fog) and T29
/// (top down: arrow plates, flame jets, rolling boulders, pits) append kinds here and their
/// behaviour in `world/tombs.rs` (`advance_traps`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TrapKind {
    /// Floor spikes (the placeholder trap of both styles): hurt a pilot standing in them.
    Spikes,
}

impl TrapKind {
    /// The kind of tile index `index` (unknown kinds read as spikes).
    #[must_use]
    pub const fn from_index(index: u8) -> Self {
        let _ = index;
        Self::Spikes
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Spikes => 0,
        }
    }

    /// Damage per second to a pilot inside the trap (before relics and difficulty).
    #[must_use]
    pub const fn damage_per_second(self) -> f32 {
        match self {
            Self::Spikes => 30.0,
        }
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

/// The realizer for `style` (the one place T28 / T29 swap theirs in).
#[must_use]
pub fn realizer_for(style: SiteStyle) -> &'static dyn FloorRealizer {
    match style {
        SiteStyle::SideView => &SideViewRealizer,
        SiteStyle::TopDown => &TopDownRealizer,
    }
}

/// The enemy kind for a spawn slot: the placeholder tomb bestiary.
///
/// T28 (mummies, crypt bats, skeletal archers, animated statues, tomb guardians) and T29
/// (skeleton packs, spectres, cultists, golems, the crypt lord) return their own kinds here.
/// Top-down placeholders fly (no floor to stand on seen from above).
#[must_use]
pub const fn bestiary(style: SiteStyle, role: SpawnRole, variant: u32) -> EnemyKind {
    match (style, role) {
        (SiteStyle::SideView, SpawnRole::Guard) => {
            if variant.is_multiple_of(2) {
                EnemyKind::Shellback
            } else {
                EnemyKind::AcidSpitter
            }
        }
        (SiteStyle::SideView | SiteStyle::TopDown, SpawnRole::Ambusher) => EnemyKind::CaveBat,
        (SiteStyle::SideView, SpawnRole::Boss) => EnemyKind::AncientGuardian,
        (SiteStyle::TopDown, SpawnRole::Guard) => EnemyKind::MagmaWisp,
        (SiteStyle::TopDown, SpawnRole::Boss) => EnemyKind::WillOWisp,
    }
}

/// The boss's health multiplier over its kind's stock health.
pub const BOSS_HEALTH_SCALE: f32 = 4.0;

/// A cell writer that keeps the latest write per tile and walls in every open cell.
struct Canvas {
    style: SiteStyle,
    cells: std::collections::BTreeMap<TilePosition, TileKind>,
}

impl Canvas {
    const fn new(style: SiteStyle) -> Self {
        Self {
            style,
            cells: std::collections::BTreeMap::new(),
        }
    }

    const fn wall(&self) -> TileKind {
        TileKind::Built(BuiltTile::TombWall(self.style.index()))
    }

    fn put(&mut self, x: i32, y: i32, kind: TileKind) {
        let _ = self.cells.insert(TilePosition { x, y }, kind);
    }

    /// Opens `rect` (inclusive) to air.
    fn open(&mut self, rect: TileRect) {
        for y in rect.min.y..=rect.max.y {
            for x in rect.min.x..=rect.max.x {
                self.put(x, y, TileKind::Air);
            }
        }
    }

    /// Moves every spawn slot standing in something solid to the nearest open cell of its
    /// room's canvas (within three tiles).
    fn settle(&self, spawns: &mut [SpawnSlot]) {
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
    fn finish(mut self) -> Vec<(TilePosition, TileKind)> {
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

const fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> TileRect {
    TileRect {
        min: TilePosition { x: x0, y: y0 },
        max: TilePosition { x: x1, y: y1 },
    }
}

/// The side-view placeholder realizer (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct SideViewRealizer;

/// Floor row of every side-view room (the solid floor tiles); rooms open above it.
const SIDE_FLOOR_ROW: i32 = 28;
/// Interior height of a side-view room (the boss room is taller).
const SIDE_ROOM_HEIGHT: i32 = 9;

impl SideViewRealizer {
    fn width_of(kind: RoomKind, variant: u32) -> i32 {
        match kind {
            RoomKind::Entry | RoomKind::Shrine => 9,
            RoomKind::Stairs => 8,
            RoomKind::Boss => 18,
            RoomKind::Vault => 10,
            RoomKind::TrapHall | RoomKind::Puzzle | RoomKind::Ambush => {
                12 + i32::try_from(variant % 5).unwrap_or(0)
            }
        }
    }
}

impl FloorRealizer for SideViewRealizer {
    fn style(&self) -> SiteStyle {
        SiteStyle::SideView
    }

    #[allow(clippy::too_many_lines, reason = "one room kind after another")]
    fn realize(&self, plan: &FloorPlan) -> RealizedFloor {
        let style = SiteStyle::SideView;
        let mut canvas = Canvas::new(style);
        let mut rng = RegionRng::new(plan.seed, SALT_REALIZE, i32::from(plan.floor), 1);
        let floor_row = SIDE_FLOOR_ROW;
        let mut rooms = Vec::new();
        let mut spawns = Vec::new();
        let mut start = TilePosition {
            x: 4,
            y: floor_row - 1,
        };
        let mut x = 2;
        let path = plan.main_path();
        let mut puzzles = 0_u8;
        for (step, &index) in path.iter().enumerate() {
            let node = &plan.rooms[index];
            let width = Self::width_of(node.kind, node.variant);
            let height = if node.kind == RoomKind::Boss {
                SIDE_ROOM_HEIGHT + 3
            } else {
                SIDE_ROOM_HEIGHT
            };
            let interior = rect(x, floor_row - height, x + width - 1, floor_row - 1);
            canvas.open(interior);
            // The doorway into the next room: three tiles tall in the shared wall.
            let door_x = x + width;
            if step + 1 < path.len() {
                canvas.open(rect(door_x, floor_row - 3, door_x, floor_row - 1));
            }
            let room_index = rooms.len();
            rooms.push(RealizedRoom {
                plan: index,
                kind: node.kind,
                interior,
            });
            let danger = i32::from(node.danger);
            match node.kind {
                RoomKind::Entry => {
                    start = TilePosition {
                        x: x + 3,
                        y: floor_row - 1,
                    };
                    if plan.floor == 0 {
                        // The way back out stands against the back wall.
                        canvas.put(x, floor_row - 1, TileKind::Built(BuiltTile::TombExit));
                    }
                }
                RoomKind::TrapHall => {
                    // Spike pits two deep across the floor, a safe ledge between each.
                    let pits = (1 + danger / 4).clamp(1, 3);
                    let span = width / (pits + 1);
                    for pit in 1..=pits {
                        let px = x + span * pit - 1;
                        let hidden = (node.variant >> pit).trailing_zeros() >= 2;
                        for dx in 0..2 {
                            canvas.put(px + dx, floor_row, TileKind::Air);
                            canvas.put(
                                px + dx,
                                floor_row + 1,
                                TileKind::Built(BuiltTile::TombTrap {
                                    kind: TrapKind::Spikes.index(),
                                    hidden,
                                }),
                            );
                        }
                    }
                    if danger >= 4 {
                        spawns.push(SpawnSlot {
                            room: room_index,
                            at: TilePosition {
                                x: x + width - 3,
                                y: floor_row - 2,
                            },
                            role: SpawnRole::Guard,
                        });
                    }
                }
                RoomKind::Puzzle => {
                    let channel = puzzles % crate::mechanism::CHANNELS;
                    puzzles += 1;
                    if step + 1 < path.len() {
                        // The doorway onward is barred; its lintel is the wall tile above.
                        canvas.put(
                            door_x,
                            floor_row - 4,
                            TileKind::Built(BuiltTile::GateLintel(channel)),
                        );
                        for dy in 1..=3 {
                            canvas.put(door_x, floor_row - dy, TileKind::Built(BuiltTile::Gate));
                        }
                    }
                    let mechanism_x = door_x - 4 - i32::try_from(node.variant % 4).unwrap_or(0);
                    if node.variant & 0x10 == 0 {
                        canvas.put(
                            mechanism_x,
                            floor_row - 1,
                            TileKind::Built(BuiltTile::Lever { channel, on: false }),
                        );
                    } else {
                        canvas.put(
                            mechanism_x,
                            floor_row,
                            TileKind::Built(BuiltTile::PressurePlate {
                                channel,
                                down: false,
                            }),
                        );
                    }
                }
                RoomKind::Ambush => {
                    // A ledge to fight from (unless the vault stair takes the room); the
                    // ambushers drop from the ceiling.
                    if plan.branches_of(index).is_empty() {
                        canvas.put(x + width / 2, floor_row - 3, canvas.wall());
                        canvas.put(x + width / 2 + 1, floor_row - 3, canvas.wall());
                    }
                    let count = (2 + danger / 3).clamp(2, 6);
                    for index in 0..count {
                        spawns.push(SpawnSlot {
                            room: room_index,
                            at: TilePosition {
                                x: x + 2 + (index * (width - 4)) / count.max(1),
                                y: floor_row - height + 2,
                            },
                            role: SpawnRole::Ambusher,
                        });
                    }
                }
                RoomKind::Shrine => {
                    canvas.put(
                        x + width / 2,
                        floor_row - 1,
                        TileKind::Built(BuiltTile::TombShrine { lit: false }),
                    );
                }
                RoomKind::Boss => {
                    spawns.push(SpawnSlot {
                        room: room_index,
                        at: TilePosition {
                            x: x + width / 2,
                            y: floor_row - 4,
                        },
                        role: SpawnRole::Boss,
                    });
                    canvas.put(x + 3, floor_row - 4, canvas.wall());
                    canvas.put(x + width - 4, floor_row - 4, canvas.wall());
                }
                RoomKind::Stairs => {
                    canvas.put(
                        x + width / 2,
                        floor_row - 1,
                        TileKind::Built(BuiltTile::TombStairs),
                    );
                }
                RoomKind::Vault => {}
            }
            // A branch room (the vault) sits on top of this one, up a stair of ledges.
            for branch in plan.branches_of(index) {
                let vault_width = Self::width_of(RoomKind::Vault, 0).min(width);
                let vault_left = x + (width - vault_width) / 2;
                let vault_floor = floor_row - height - 1;
                let vault = rect(
                    vault_left,
                    vault_floor - 5,
                    vault_left + vault_width - 1,
                    vault_floor - 1,
                );
                canvas.open(vault);
                // Ledges two tiles apart up the room's left side, then a hole in its ceiling.
                // A one-tile step, then two-wide ledges two rows apart climbing right, the
                // top one on the room's top row with the hole right above it: walkers on the
                // floor pass under the ledges, a climber hops onto the vault floor.
                canvas.put(vault_left, floor_row - 1, canvas.wall());
                let mut top_ledge = vault_left;
                let mut step = 1;
                while floor_row - 1 - 2 * step >= floor_row - height {
                    let (ledge_x, ledge_y) = (vault_left + 2 * step, floor_row - 1 - 2 * step);
                    canvas.put(ledge_x, ledge_y, canvas.wall());
                    canvas.put(ledge_x + 1, ledge_y, canvas.wall());
                    top_ledge = ledge_x;
                    step += 1;
                }
                canvas.put(top_ledge, vault_floor, TileKind::Air);
                canvas.put(top_ledge + 1, vault_floor, TileKind::Air);
                // Caches along the vault floor, clear of the hole.
                let caches = (2 + usize::from(plan.rooms[branch].danger) / 4).clamp(2, 4);
                for offset in [0, 2, 4, 6].into_iter().take(caches) {
                    canvas.put(
                        vault_left + offset,
                        vault_floor - 1,
                        TileKind::Built(BuiltTile::TombCache { open: false }),
                    );
                }
                rooms.push(RealizedRoom {
                    plan: branch,
                    kind: RoomKind::Vault,
                    interior: vault,
                });
            }
            let _ = rng.next_u64();
            x += width + 1;
        }
        RealizedFloor {
            style,
            size: (x + 2, floor_row + 4),
            cells: {
                canvas.settle(&mut spawns);
                canvas.finish()
            },
            rooms,
            start,
            spawns,
        }
    }
}

/// The top-down placeholder realizer (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct TopDownRealizer;

/// Interior size of a top-down room (columns, rows).
const CRYPT_ROOM: (i32, i32) = (13, 9);
/// Top row of the main path's rooms (the vault row is above).
const CRYPT_PATH_TOP: i32 = CRYPT_ROOM.1 + 3;

impl FloorRealizer for TopDownRealizer {
    fn style(&self) -> SiteStyle {
        SiteStyle::TopDown
    }

    #[allow(clippy::too_many_lines, reason = "one room kind after another")]
    fn realize(&self, plan: &FloorPlan) -> RealizedFloor {
        let style = SiteStyle::TopDown;
        let mut canvas = Canvas::new(style);
        let (width, height) = CRYPT_ROOM;
        let top = CRYPT_PATH_TOP;
        let bottom = top + height - 1;
        let mid = top + height / 2;
        let mut rooms = Vec::new();
        let mut spawns = Vec::new();
        let mut start = TilePosition { x: 4, y: mid };
        let path = plan.main_path();
        let mut puzzles = 0_u8;
        for (step, &index) in path.iter().enumerate() {
            let node = &plan.rooms[index];
            let x = 2 + i32::try_from(step).unwrap_or(0) * (width + 1);
            let interior = rect(x, top, x + width - 1, bottom);
            canvas.open(interior);
            let door_x = x + width;
            if step + 1 < path.len() {
                // A three-row doorway east.
                canvas.open(rect(door_x, mid - 1, door_x, mid + 1));
            }
            let room_index = rooms.len();
            rooms.push(RealizedRoom {
                plan: index,
                kind: node.kind,
                interior,
            });
            let danger = i32::from(node.danger);
            let centre = TilePosition {
                x: x + width / 2,
                y: mid,
            };
            match node.kind {
                RoomKind::Entry => {
                    start = TilePosition { x: x + 3, y: mid };
                    if plan.floor == 0 {
                        canvas.put(x - 1, mid, TileKind::Built(BuiltTile::TombExit));
                    }
                }
                RoomKind::TrapHall => {
                    // A spike floor with one safe lane that changes row each column band.
                    let lane = i32::try_from(node.variant % 3).unwrap_or(0) - 1;
                    for column in x + 2..x + width - 2 {
                        let band = (column - x) / 3;
                        let safe = mid + if band % 2 == 0 { lane } else { -lane };
                        for row in top..=bottom {
                            if (row - safe).abs() <= 1 {
                                continue;
                            }
                            let hidden = (node.variant >> (column & 7)).trailing_zeros() >= 2;
                            canvas.put(
                                column,
                                row,
                                TileKind::Built(BuiltTile::TombTrap {
                                    kind: TrapKind::Spikes.index(),
                                    hidden,
                                }),
                            );
                        }
                    }
                    if danger >= 4 {
                        spawns.push(SpawnSlot {
                            room: room_index,
                            at: TilePosition {
                                x: x + width - 2,
                                y: top + 1,
                            },
                            role: SpawnRole::Guard,
                        });
                    }
                }
                RoomKind::Puzzle => {
                    let channel = puzzles % crate::mechanism::CHANNELS;
                    puzzles += 1;
                    if step + 1 < path.len() {
                        canvas.put(
                            door_x,
                            mid - 2,
                            TileKind::Built(BuiltTile::GateLintel(channel)),
                        );
                        for dy in -1..=1 {
                            canvas.put(door_x, mid + dy, TileKind::Built(BuiltTile::Gate));
                        }
                    }
                    // A lever on a plinth in the room.
                    let lever_x = door_x - 3 - i32::try_from(node.variant % 4).unwrap_or(0);
                    let lever_y = if node.variant & 0x10 == 0 {
                        top + 1
                    } else {
                        bottom - 1
                    };
                    canvas.put(
                        lever_x,
                        lever_y,
                        TileKind::Built(BuiltTile::Lever { channel, on: false }),
                    );
                }
                RoomKind::Ambush => {
                    let count = (2 + danger / 3).clamp(2, 6);
                    for index in 0..count {
                        spawns.push(SpawnSlot {
                            room: room_index,
                            at: TilePosition {
                                x: x + 2 + (index * (width - 4)) / count.max(1),
                                y: if index % 2 == 0 { top + 1 } else { bottom - 1 },
                            },
                            role: SpawnRole::Ambusher,
                        });
                    }
                    // Four pillars to dodge around.
                    for (dx, dy) in [(-3, -2), (3, -2), (-3, 2), (3, 2)] {
                        canvas.put(centre.x + dx, centre.y + dy, canvas.wall());
                    }
                }
                RoomKind::Shrine => {
                    canvas.put(
                        centre.x,
                        centre.y,
                        TileKind::Built(BuiltTile::TombShrine { lit: false }),
                    );
                }
                RoomKind::Boss => {
                    spawns.push(SpawnSlot {
                        room: room_index,
                        at: TilePosition {
                            x: centre.x + 3,
                            y: centre.y,
                        },
                        role: SpawnRole::Boss,
                    });
                }
                RoomKind::Stairs => {
                    canvas.put(centre.x, centre.y, TileKind::Built(BuiltTile::TombStairs));
                }
                RoomKind::Vault => {}
            }
            // The vault is the room north of its parent, through a doorway in the wall.
            for branch in plan.branches_of(index) {
                let vault = rect(x, top - height - 1, x + width - 1, top - 2);
                canvas.open(vault);
                canvas.open(rect(centre.x - 1, top - 1, centre.x + 1, top - 1));
                let caches = (2 + i32::from(plan.rooms[branch].danger) / 4).clamp(2, 5);
                for cache in 0..caches {
                    canvas.put(
                        x + 2 + cache * 2,
                        vault.min.y + 1,
                        TileKind::Built(BuiltTile::TombCache { open: false }),
                    );
                }
                rooms.push(RealizedRoom {
                    plan: branch,
                    kind: RoomKind::Vault,
                    interior: vault,
                });
            }
        }
        let columns = 2 + i32::try_from(path.len()).unwrap_or(0) * (width + 1) + 2;
        RealizedFloor {
            style,
            size: (columns, bottom + 3),
            cells: {
                canvas.settle(&mut spawns);
                canvas.finish()
            },
            rooms,
            start,
            spawns,
        }
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
