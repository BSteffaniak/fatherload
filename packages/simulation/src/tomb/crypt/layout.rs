//! A crypt floor laid out on a room grid (WP-TOMBS T29; see the parent module).
//!
//! The plan's main path is placed cell by cell with a deterministic depth-first walk (each
//! room's candidate directions are ordered by a hash of the floor seed, the room and the
//! direction, east first more often than not), backtracking when it paints itself into a
//! corner; the vault takes a free side of its parent, the boss hall a free two-by-two block.
//! A plan that will not fit (it always does on the 13 x 4 grid, but the code does not rely on
//! it) falls back to a straight row.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    base::BuiltTile,
    terrain::{TileKind, TilePosition},
    tomb::{
        SiteStyle,
        floor::{FloorPlan, RoomKind},
        realize::{
            FloorRealizer, MAX_FLOOR_SIZE, RealizedFloor, RealizedRoom, SpawnRole, SpawnSlot,
        },
    },
    worldgen::{features::TileRect, noise::hash2},
};

use super::{CryptTrap, CryptTrapKind};

/// Cells of the room grid (columns, rows).
pub const GRID: (i32, i32) = (13, 4);
/// One cell with its shared walls (columns, rows): an 11 x 9 room interior.
const CELL: (i32, i32) = (12, 10);
/// Where the grid's top-left wall corner sits in the slot.
const ORIGIN: (i32, i32) = (2, 1);
/// Most placement steps before the walk gives up for the straight row.
const MAX_STEPS: u32 = 20_000;

const SALT_ORDER: u64 = 0x70B5_C4F7_0001;
const SALT_CONTENT: u64 = 0x70B5_C4F7_0002;

type Cell = (i32, i32);

/// The four sides, in a fixed order: east, south, west, north.
const SIDES: [Cell; 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// A crypt floor: the realized tiles plus the trap overlays (relative to the slot corner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptFloor {
    pub realized: RealizedFloor,
    pub traps: Vec<CryptTrap>,
    /// Each plan room's grid cells (rooms in plan order).
    pub cells: Vec<Vec<(i32, i32)>>,
}

/// The crypt realizer (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct CryptRealizer;

impl FloorRealizer for CryptRealizer {
    fn style(&self) -> SiteStyle {
        SiteStyle::TopDown
    }

    fn realize(&self, plan: &FloorPlan) -> RealizedFloor {
        layout(plan).realized
    }
}

/// The interior of grid cell `cell` (relative tiles, inclusive).
const fn cell_interior(cell: Cell) -> TileRect {
    let x0 = ORIGIN.0 + cell.0 * CELL.0 + 1;
    let y0 = ORIGIN.1 + cell.1 * CELL.1 + 1;
    TileRect {
        min: TilePosition { x: x0, y: y0 },
        max: TilePosition {
            x: x0 + CELL.0 - 2,
            y: y0 + CELL.1 - 2,
        },
    }
}

const fn in_grid(cell: Cell) -> bool {
    cell.0 >= 0 && cell.1 >= 0 && cell.0 < GRID.0 && cell.1 < GRID.1
}

/// A doorway between two rooms: the cell on each side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Link {
    from: Cell,
    to: Cell,
}

impl Link {
    const fn horizontal(self) -> bool {
        self.from.1 == self.to.1
    }

    /// The doorway's three open tiles in the shared wall.
    fn tiles(self) -> [TilePosition; 3] {
        if self.horizontal() {
            let x = ORIGIN.0 + self.from.0.max(self.to.0) * CELL.0;
            let mid = cell_interior(self.from).center().y;
            [-1, 0, 1].map(|dy| TilePosition { x, y: mid + dy })
        } else {
            let y = ORIGIN.1 + self.from.1.max(self.to.1) * CELL.1;
            let mid = cell_interior(self.from).center().x;
            [-1, 0, 1].map(|dx| TilePosition { x: mid + dx, y })
        }
    }

    /// The side of `from` the doorway is on.
    const fn side(self) -> Cell {
        (self.to.0 - self.from.0, self.to.1 - self.from.1)
    }
}

/// Placement of the plan's rooms on the grid.
struct Placement {
    /// Each plan room's cells.
    cells: Vec<Vec<Cell>>,
    /// Doorways: `(room a, room b, link)` with the link from a's cell to b's.
    links: Vec<(usize, usize, Link)>,
}

struct Walker<'a> {
    plan: &'a FloorPlan,
    path: Vec<usize>,
    occupied: BTreeSet<Cell>,
    cells: Vec<Vec<Cell>>,
    links: Vec<(usize, usize, Link)>,
    steps: u32,
}

impl Walker<'_> {
    /// Candidate sides out of room `index`, ordered by the floor's hash (east weighted first).
    fn sides(&self, index: usize) -> Vec<Cell> {
        let mut sides: Vec<(u64, Cell)> = SIDES
            .iter()
            .enumerate()
            .map(|(order, side)| {
                let roll = hash2(
                    self.plan.seed,
                    SALT_ORDER,
                    i32::try_from(index).unwrap_or(0),
                    i32::try_from(order).unwrap_or(0),
                ) % 1000;
                // East is preferred: the floor winds but trends one way.
                let bias = if *side == (1, 0) { 0 } else { 450 };
                (roll + bias, *side)
            })
            .collect();
        sides.sort_unstable();
        sides.into_iter().map(|(_, side)| side).collect()
    }

    /// The cells a room of `kind` takes when entered from `from` through `side`.
    fn footprints(&self, kind: RoomKind, from: Cell, side: Cell) -> Vec<Vec<Cell>> {
        let to = (from.0 + side.0, from.1 + side.1);
        let free = |cell: &Cell| in_grid(*cell) && !self.occupied.contains(cell);
        if kind == RoomKind::Boss {
            let mut blocks = Vec::new();
            for (ax, ay) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let corner = (to.0 - ax, to.1 - ay);
                let block = vec![
                    corner,
                    (corner.0 + 1, corner.1),
                    (corner.0, corner.1 + 1),
                    (corner.0 + 1, corner.1 + 1),
                ];
                if block.iter().all(free) {
                    blocks.push(block);
                }
            }
            blocks
        } else if free(&to) {
            vec![vec![to]]
        } else {
            Vec::new()
        }
    }

    fn place(&mut self, step: usize) -> bool {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return false;
        }
        if step == self.path.len() {
            return self.place_branches();
        }
        let index = self.path[step];
        let previous = self.path[step - 1];
        let kind = self.plan.rooms[index].kind;
        // A puzzle's way on is a gate hung under a lintel: an east or west doorway.
        let gated = self.plan.rooms[previous].kind == RoomKind::Puzzle;
        let from_cells = self.cells[previous].clone();
        for side in self.sides(index) {
            if gated && side.1 != 0 {
                continue;
            }
            for &from in &from_cells {
                for footprint in self.footprints(kind, from, side) {
                    let to = (from.0 + side.0, from.1 + side.1);
                    for cell in &footprint {
                        self.occupied.insert(*cell);
                    }
                    self.cells[index].clone_from(&footprint);
                    self.links.push((previous, index, Link { from, to }));
                    if self.place(step + 1) {
                        return true;
                    }
                    self.links.pop();
                    self.cells[index].clear();
                    for cell in &footprint {
                        self.occupied.remove(cell);
                    }
                }
            }
        }
        false
    }

    /// The branch rooms (the vault) on a free side of their parents.
    fn place_branches(&mut self) -> bool {
        let mut placed = Vec::new();
        for (index, room) in self.plan.rooms.iter().enumerate() {
            if !room.branch {
                continue;
            }
            let Some(parent) = (0..self.plan.rooms.len()).find(|parent| {
                self.plan.rooms[*parent]
                    .links
                    .iter()
                    .any(|link| usize::from(*link) == index)
            }) else {
                continue;
            };
            let mut found = None;
            'search: for side in self.sides(index) {
                for &from in &self.cells[parent] {
                    let to = (from.0 + side.0, from.1 + side.1);
                    if in_grid(to) && !self.occupied.contains(&to) {
                        found = Some(Link { from, to });
                        break 'search;
                    }
                }
            }
            let Some(link) = found else {
                for (cell, index, _) in placed {
                    self.occupied.remove(&cell);
                    self.cells[index] = Vec::new();
                    self.links.pop();
                }
                return false;
            };
            self.occupied.insert(link.to);
            self.cells[index] = vec![link.to];
            self.links.push((parent, index, link));
            placed.push((link.to, index, link));
        }
        true
    }
}

/// Places the plan's rooms (see the module docs).
fn place(plan: &FloorPlan) -> Placement {
    let path = plan.main_path();
    let start_row = i32::try_from(hash2(plan.seed, SALT_ORDER, -1, -1) % 4).unwrap_or(0);
    let mut walker = Walker {
        plan,
        path: path.clone(),
        occupied: BTreeSet::new(),
        cells: vec![Vec::new(); plan.rooms.len()],
        links: Vec::new(),
        steps: 0,
    };
    for row in (0..GRID.1).map(|offset| (start_row + offset) % GRID.1) {
        let start = (0, row);
        walker.occupied.clear();
        walker.links.clear();
        walker.cells = vec![Vec::new(); plan.rooms.len()];
        walker.steps = 0;
        walker.occupied.insert(start);
        walker.cells[path[0]] = vec![start];
        if walker.place(1) {
            return Placement {
                cells: walker.cells,
                links: walker.links,
            };
        }
    }
    straight_row(plan, &path)
}

/// The fallback: the main path along row 1, the boss hall over rows 1 and 2, the vault
/// north of its parent.
fn straight_row(plan: &FloorPlan, path: &[usize]) -> Placement {
    let mut cells = vec![Vec::new(); plan.rooms.len()];
    let mut links = Vec::new();
    let mut column = 0;
    let mut previous: Option<(usize, Cell)> = None;
    for &index in path {
        let cell = (column, 1);
        cells[index] = if plan.rooms[index].kind == RoomKind::Boss {
            vec![cell, (column + 1, 1), (column, 2), (column + 1, 2)]
        } else {
            vec![cell]
        };
        if let Some((before, from)) = previous {
            links.push((before, index, Link { from, to: cell }));
        }
        let exit = if plan.rooms[index].kind == RoomKind::Boss {
            (column + 1, 1)
        } else {
            cell
        };
        previous = Some((index, exit));
        column += if plan.rooms[index].kind == RoomKind::Boss {
            2
        } else {
            1
        };
        for branch in plan.branches_of(index) {
            let up = (cell.0, 0);
            cells[branch] = vec![up];
            links.push((index, branch, Link { from: cell, to: up }));
        }
    }
    Placement { cells, links }
}

/// A cell writer: the latest write per tile wins; walls go round every open tile at the end.
struct Canvas {
    cells: BTreeMap<TilePosition, TileKind>,
}

impl Canvas {
    const WALL: TileKind = TileKind::Built(BuiltTile::TombWall(1));

    fn put(&mut self, tile: TilePosition, kind: TileKind) {
        let _ = self.cells.insert(tile, kind);
    }

    fn open(&mut self, rect: TileRect) {
        for y in rect.min.y..=rect.max.y {
            for x in rect.min.x..=rect.max.x {
                self.put(TilePosition { x, y }, TileKind::Air);
            }
        }
    }

    fn is_open(&self, tile: TilePosition) -> bool {
        self.cells.get(&tile) == Some(&TileKind::Air)
    }

    fn finish(mut self) -> Vec<(TilePosition, TileKind)> {
        let open: Vec<TilePosition> = self
            .cells
            .iter()
            .filter(|(_, kind)| **kind != Self::WALL)
            .map(|(tile, _)| *tile)
            .collect();
        for tile in open {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    self.cells
                        .entry(TilePosition {
                            x: tile.x + dx,
                            y: tile.y + dy,
                        })
                        .or_insert(Self::WALL);
                }
            }
        }
        self.cells.into_iter().collect()
    }
}

/// What one room's builder knows.
struct Room {
    index: usize,
    kind: RoomKind,
    interior: TileRect,
    /// The doorways into this room: `(side of this room, open tiles)`.
    doors: Vec<(Cell, [TilePosition; 3])>,
    /// The doorway the main path enters by and the one it leaves by (none at the ends).
    entry: Option<Cell>,
    exit: Option<Cell>,
    danger: i32,
    variant: u32,
}

impl Room {
    const fn centre(&self) -> TilePosition {
        self.interior.center()
    }

    /// Whether `tile` is within `reach` tiles (Chebyshev) of a doorway tile.
    fn near_door(&self, tile: TilePosition, reach: i32) -> bool {
        self.doors.iter().any(|(_, tiles)| {
            tiles
                .iter()
                .any(|door| (door.x - tile.x).abs() <= reach && (door.y - tile.y).abs() <= reach)
        })
    }

    /// The way the path crosses the room: `Some(true)` east-west, `Some(false)` north-south,
    /// `None` for a turn (or an end room).
    fn traversal(&self) -> Option<bool> {
        let entry = self.entry?;
        let exit = self.exit?;
        match (entry.1 == 0, exit.1 == 0) {
            (true, true) => Some(true),
            (false, false) => Some(false),
            _ => None,
        }
    }

    /// The interior tile just inside a doorway on `side`.
    const fn inside_door(&self, side: Cell) -> TilePosition {
        let centre = self.centre();
        match side {
            (1, _) => TilePosition {
                x: self.interior.max.x,
                y: centre.y,
            },
            (-1, _) => TilePosition {
                x: self.interior.min.x,
                y: centre.y,
            },
            (_, 1) => TilePosition {
                x: centre.x,
                y: self.interior.max.y,
            },
            _ => TilePosition {
                x: centre.x,
                y: self.interior.min.y,
            },
        }
    }
}

/// Lays `plan` out as a crypt floor (pure).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one room kind after another")]
pub fn layout(plan: &FloorPlan) -> CryptFloor {
    let placement = place(plan);
    let mut canvas = Canvas {
        cells: BTreeMap::new(),
    };
    let mut rooms: Vec<Room> = Vec::new();
    for (index, cells) in placement.cells.iter().enumerate() {
        let node = &plan.rooms[index];
        let interior = cells
            .iter()
            .map(|cell| cell_interior(*cell))
            .reduce(|a, b| TileRect {
                min: TilePosition {
                    x: a.min.x.min(b.min.x),
                    y: a.min.y.min(b.min.y),
                },
                max: TilePosition {
                    x: a.max.x.max(b.max.x),
                    y: a.max.y.max(b.max.y),
                },
            })
            .unwrap_or(TileRect {
                min: TilePosition { x: 3, y: 2 },
                max: TilePosition { x: 3, y: 2 },
            });
        canvas.open(interior);
        rooms.push(Room {
            index,
            kind: node.kind,
            interior,
            doors: Vec::new(),
            entry: None,
            exit: None,
            danger: i32::from(node.danger),
            variant: node.variant,
        });
    }
    for &(a, b, link) in &placement.links {
        let tiles = link.tiles();
        for tile in tiles {
            canvas.put(tile, TileKind::Air);
        }
        let side = link.side();
        rooms[a].doors.push((side, tiles));
        rooms[b].doors.push(((-side.0, -side.1), tiles));
        if !plan.rooms[b].branch {
            rooms[a].exit = Some(side);
            rooms[b].entry = Some((-side.0, -side.1));
        }
    }

    let mut spawns = Vec::new();
    let mut traps: Vec<CryptTrap> = Vec::new();
    let mut start = TilePosition { x: 8, y: 6 };
    let mut puzzles = 0_u8;
    for room in &rooms {
        let centre = room.centre();
        let (x0, y0, x1, y1) = (
            room.interior.min.x,
            room.interior.min.y,
            room.interior.max.x,
            room.interior.max.y,
        );
        let room_index = u8::try_from(room.index).unwrap_or(u8::MAX);
        let roll = |salt: i32| hash2(plan.seed, SALT_CONTENT, i32::from(room_index), salt);
        let mut trap =
            |at: TilePosition, kind: CryptTrapKind, dir: (i8, i8), phase: u64, hidden| {
                traps.push(CryptTrap {
                    at,
                    kind,
                    dir,
                    phase: u16::try_from(phase % 60_000).unwrap_or(0),
                    hidden,
                    room: room_index,
                });
            };
        match room.kind {
            RoomKind::Entry => {
                start = centre;
                if plan.floor == 0 {
                    // The way back out, in a corner clear of the doorways.
                    canvas.put(
                        TilePosition { x: x0, y: y0 },
                        TileKind::Built(BuiltTile::TombExit),
                    );
                }
            }
            RoomKind::Shrine => {
                canvas.put(
                    centre,
                    TileKind::Built(BuiltTile::TombShrine { lit: false }),
                );
            }
            RoomKind::Stairs => {
                canvas.put(centre, TileKind::Built(BuiltTile::TombStairs));
            }
            RoomKind::Vault => {
                let caches = (2 + room.danger / 4).clamp(2, 5);
                let door_north = room.doors.iter().any(|(side, _)| *side == (0, -1));
                let row = if door_north { y1 - 1 } else { y0 + 1 };
                for cache in 0..caches {
                    canvas.put(
                        TilePosition {
                            x: x0 + 1 + cache * 2,
                            y: row,
                        },
                        TileKind::Built(BuiltTile::TombCache { open: false }),
                    );
                }
                if room.danger >= 6 {
                    spawns.push(SpawnSlot {
                        room: room.index,
                        at: centre,
                        role: SpawnRole::Guard,
                    });
                }
            }
            RoomKind::Puzzle => {
                let channel = puzzles % crate::mechanism::CHANNELS;
                puzzles += 1;
                if let Some(exit) = room.exit
                    && let Some((_, tiles)) = room.doors.iter().find(|(side, _)| *side == exit)
                {
                    let lintel = TilePosition {
                        x: tiles[0].x,
                        y: tiles[0].y - 1,
                    };
                    canvas.put(lintel, TileKind::Built(BuiltTile::GateLintel(channel)));
                    for tile in tiles {
                        canvas.put(*tile, TileKind::Built(BuiltTile::Gate));
                    }
                }
                // The lever in a corner clear of the doorways.
                let corners = [
                    TilePosition {
                        x: x0 + 1,
                        y: y0 + 1,
                    },
                    TilePosition {
                        x: x1 - 1,
                        y: y0 + 1,
                    },
                    TilePosition {
                        x: x0 + 1,
                        y: y1 - 1,
                    },
                    TilePosition {
                        x: x1 - 1,
                        y: y1 - 1,
                    },
                ];
                let pick = usize::try_from(room.variant % 4).unwrap_or(0);
                let lever = (0..4)
                    .map(|offset| corners[(pick + offset) % 4])
                    .find(|tile| !room.near_door(*tile, 2))
                    .unwrap_or(corners[pick]);
                canvas.put(
                    lever,
                    TileKind::Built(BuiltTile::Lever { channel, on: false }),
                );
                if room.danger >= 5 {
                    // Spikes round the lever: time the reach.
                    for (dx, dy) in [
                        (-1, 0),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                        (-1, -1),
                        (1, 1),
                        (-1, 1),
                        (1, -1),
                    ] {
                        let at = TilePosition {
                            x: lever.x + dx,
                            y: lever.y + dy,
                        };
                        if room.interior.contains(at) && !room.near_door(at, 1) {
                            trap(at, CryptTrapKind::Spikes, (0, 0), 0, false);
                        }
                    }
                    for dx in [-2, 2] {
                        spawns.push(SpawnSlot {
                            room: room.index,
                            at: TilePosition {
                                x: centre.x + dx,
                                y: centre.y,
                            },
                            role: SpawnRole::Guard,
                        });
                    }
                }
            }
            RoomKind::Ambush => {
                for (dx, dy) in [(-3, -2), (3, -2), (-3, 2), (3, 2)] {
                    canvas.put(
                        TilePosition {
                            x: centre.x + dx,
                            y: centre.y + dy,
                        },
                        Canvas::WALL,
                    );
                }
                let spots = [
                    TilePosition {
                        x: x0 + 1,
                        y: y0 + 1,
                    },
                    TilePosition {
                        x: x1 - 1,
                        y: y1 - 1,
                    },
                    TilePosition {
                        x: x1 - 1,
                        y: y0 + 1,
                    },
                    TilePosition {
                        x: x0 + 1,
                        y: y1 - 1,
                    },
                    TilePosition {
                        x: centre.x,
                        y: y0 + 1,
                    },
                    TilePosition {
                        x: centre.x,
                        y: y1 - 1,
                    },
                ];
                let count = (2 + room.danger / 3).clamp(2, 6);
                for spot in spots.iter().take(usize::try_from(count).unwrap_or(2)) {
                    spawns.push(SpawnSlot {
                        room: room.index,
                        at: *spot,
                        role: SpawnRole::Ambusher,
                    });
                }
            }
            RoomKind::Boss => {
                spawns.push(SpawnSlot {
                    room: room.index,
                    at: centre,
                    role: SpawnRole::Boss,
                });
                for (dx, dy) in [(-6, -5), (6, -5), (-6, 5), (6, 5)] {
                    canvas.put(
                        TilePosition {
                            x: centre.x + dx,
                            y: centre.y + dy,
                        },
                        Canvas::WALL,
                    );
                }
            }
            RoomKind::TrapHall => {
                let traversal = room.traversal();
                let theme = room.variant % 5;
                let mut themes = vec![theme];
                if room.danger >= 7 {
                    let second = (room.variant >> 8) % 5;
                    if second != theme {
                        themes.push(second);
                    }
                }
                for theme in themes {
                    // Arrows and boulders need a straight crossing to fly across; a turning
                    // room gets flames instead.
                    let theme = match (theme, traversal) {
                        (1 | 2, None) => 3,
                        _ => theme,
                    };
                    let horizontal = traversal.unwrap_or(true);
                    match theme {
                        0 => {
                            // A spike field rippling along the way across.
                            for y in y0..=y1 {
                                for x in x0..=x1 {
                                    let at = TilePosition { x, y };
                                    let along = if horizontal { x - x0 } else { y - y0 };
                                    let inner = if horizontal {
                                        (2..=x1 - x0 - 2).contains(&along)
                                    } else {
                                        (2..=y1 - y0 - 2).contains(&along)
                                    };
                                    if inner && !room.near_door(at, 0) {
                                        let phase = u64::try_from(along).unwrap_or(0) * 240;
                                        trap(at, CryptTrapKind::Spikes, (0, 0), phase, false);
                                    }
                                }
                            }
                        }
                        1 => {
                            // Plates in three lines across the way, a gap in each; each fires
                            // along its line from both walls.
                            for line in 0..3 {
                                let gap = i32::try_from(roll(line) % 9).unwrap_or(0);
                                for k in 0..9 {
                                    let at = if horizontal {
                                        TilePosition {
                                            x: x0 + 2 + line * 3,
                                            y: y0 + k,
                                        }
                                    } else {
                                        TilePosition {
                                            x: x0 + 1 + k,
                                            y: y0 + 2 + line * 2,
                                        }
                                    };
                                    if k == gap || !room.interior.contains(at) {
                                        continue;
                                    }
                                    let hidden =
                                        room.danger >= 6 && roll(40 + k + line * 9) % 3 == 0;
                                    let dir = if horizontal { (0, 1) } else { (1, 0) };
                                    trap(at, CryptTrapKind::ArrowPlate, dir, 0, hidden);
                                }
                            }
                        }
                        2 => {
                            // Boulder chutes along one wall, rolling across the way.
                            for (k, offset) in [2, 5, 8].into_iter().enumerate() {
                                let (at, dir) = if horizontal {
                                    (
                                        TilePosition {
                                            x: x0 + offset,
                                            y: y0,
                                        },
                                        (0, 1),
                                    )
                                } else {
                                    (
                                        TilePosition {
                                            x: x0,
                                            y: y0 + offset - 1,
                                        },
                                        (1, 0),
                                    )
                                };
                                if room.near_door(at, 1) {
                                    continue;
                                }
                                let phase = u64::try_from(k).unwrap_or(0) * 1_200;
                                trap(at, CryptTrapKind::BoulderChute, dir, phase, false);
                            }
                        }
                        3 => {
                            // Flame vents in lines across the way, taking turns.
                            for (k, offset) in [2, 4, 6, 8].into_iter().enumerate() {
                                let phase = u64::try_from(k).unwrap_or(0) * 700;
                                if horizontal {
                                    for y in y0..=y1 {
                                        let at = TilePosition { x: x0 + offset, y };
                                        if !room.near_door(at, 0) {
                                            trap(at, CryptTrapKind::FlameJet, (0, 0), phase, false);
                                        }
                                    }
                                } else {
                                    for x in x0..=x1 {
                                        let at = TilePosition {
                                            x,
                                            y: y0 + offset - 1,
                                        };
                                        if !room.near_door(at, 0) {
                                            trap(at, CryptTrapKind::FlameJet, (0, 0), phase, false);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            // Cracked floors round a safe winding lane two tiles wide.
                            let lane = safe_lane(room, plan.seed);
                            for y in y0..=y1 {
                                for x in x0..=x1 {
                                    let at = TilePosition { x, y };
                                    if lane.contains(&at) || room.near_door(at, 1) {
                                        continue;
                                    }
                                    let open = roll(100 + (y - y0) * 16 + (x - x0)) % 4 == 0;
                                    let kind = if open {
                                        CryptTrapKind::Pit
                                    } else {
                                        CryptTrapKind::CrackedFloor
                                    };
                                    let hidden =
                                        !open && room.danger >= 5 && roll(400 + x + y) % 2 == 0;
                                    trap(at, kind, (0, 0), 0, hidden);
                                }
                            }
                        }
                    }
                }
                if room.danger >= 4 {
                    // A guard waits on the far side.
                    let far = room.exit.map_or(centre, |side| room.inside_door(side));
                    let at = TilePosition {
                        x: i32::midpoint(far.x, centre.x),
                        y: i32::midpoint(far.y, centre.y) + 1,
                    };
                    spawns.push(SpawnSlot {
                        room: room.index,
                        at,
                        role: SpawnRole::Guard,
                    });
                }
            }
        }
    }
    // One trap per tile (the latest wins), never under a fixture, in tile order.
    let mut by_tile: BTreeMap<TilePosition, CryptTrap> = BTreeMap::new();
    for trap in traps {
        if canvas.is_open(trap.at) {
            by_tile.insert(trap.at, trap);
        }
    }
    let traps: Vec<CryptTrap> = by_tile.into_values().collect();
    // Spawns stand on open floor, off traps and pits.
    for spawn in &mut spawns {
        let free = |tile: &TilePosition| {
            canvas.is_open(*tile) && !traps.iter().any(|trap| trap.at == *tile)
        };
        if free(&spawn.at) {
            continue;
        }
        let interior = rooms[spawn.room].interior;
        if let Some(tile) = (1..=6).find_map(|reach| {
            (-reach..=reach)
                .flat_map(|dy| (-reach..=reach).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| TilePosition {
                    x: spawn.at.x + dx,
                    y: spawn.at.y + dy,
                })
                .find(|tile| interior.contains(*tile) && free(tile))
        }) {
            spawn.at = tile;
        }
    }
    let max = canvas
        .cells
        .keys()
        .fold((0, 0), |(mx, my), tile| (mx.max(tile.x), my.max(tile.y)));
    let realized_rooms = rooms
        .iter()
        .map(|room| RealizedRoom {
            plan: room.index,
            kind: room.kind,
            interior: room.interior,
        })
        .collect();
    let realized = RealizedFloor {
        style: SiteStyle::TopDown,
        size: (
            (max.0 + 3).min(MAX_FLOOR_SIZE.0),
            (max.1 + 3).min(MAX_FLOOR_SIZE.1),
        ),
        cells: canvas.finish(),
        rooms: realized_rooms,
        start,
        spawns,
    };
    CryptFloor {
        realized,
        traps,
        cells: placement.cells,
    }
}

/// A safe lane two tiles wide from the room's entry doorway to its exit (a deterministic
/// wiggly walk), for pit floors.
fn safe_lane(room: &Room, seed: u64) -> BTreeSet<TilePosition> {
    let from = room
        .entry
        .map_or_else(|| room.centre(), |side| room.inside_door(side));
    let to = room
        .exit
        .map_or_else(|| room.centre(), |side| room.inside_door(side));
    // Walk the longer axis a line at a time; the cross position wanders by up to a tile per
    // line (and lands on the exit's on the last), and each line fills the run between.
    let horizontal = (to.x - from.x).abs() >= (to.y - from.y).abs();
    let (along_from, along_to, mut cross, cross_to, cross_min, cross_max) = if horizontal {
        (
            from.x,
            to.x,
            from.y,
            to.y,
            room.interior.min.y + 1,
            room.interior.max.y - 1,
        )
    } else {
        (
            from.y,
            to.y,
            from.x,
            to.x,
            room.interior.min.x + 1,
            room.interior.max.x - 1,
        )
    };
    let tile = |along: i32, cross: i32| {
        if horizontal {
            TilePosition { x: along, y: cross }
        } else {
            TilePosition { x: cross, y: along }
        }
    };
    let mut lane = BTreeSet::new();
    let step = (along_to - along_from).signum();
    let mut along = along_from;
    loop {
        let target = if along == along_to {
            cross_to
        } else {
            let wiggle =
                i32::try_from(hash2(seed, SALT_CONTENT, along, cross) % 3).unwrap_or(1) - 1;
            (cross + wiggle).clamp(cross_min.min(cross_max), cross_max.max(cross_min))
        };
        for between in cross.min(target)..=cross.max(target) {
            lane.insert(tile(along, between));
            // Two wide.
            let wide = tile(along, between + 1);
            if room.interior.contains(wide) {
                lane.insert(wide);
            }
        }
        cross = target;
        if along == along_to || step == 0 {
            break;
        }
        along += step;
    }
    lane.insert(to);
    lane
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
