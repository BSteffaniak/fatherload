//! The side-view tomb realizer (WP-TOMBS, T28): platformer rooms built on the pilot's own
//! movement, in the tile set of the site's culture ([`TombCulture`]).
//!
//! A floor is the plan's main path laid out west to east, every room on one doorway line
//! ([`FLOOR_ROW`]: three-tall doorways in the shared walls), with the vault above its parent
//! room. Each room kind has templates, picked from the room's roll, the culture and the danger:
//!
//! * **Entry**: the way back out on the first floor, a pillar, sometimes a secret cellar.
//! * **Trap halls** ([`TrapTemplate`]): spike pits (low ceilings and dart throwers firing at
//!   jump height deeper down), a blade gallery (two swinging blades under a low ceiling, a safe
//!   strip between them), a crusher run (crushers falling in a wave), a collapsing bridge
//!   (crumbling slabs over a spike pit, a solid slab mid-way), a sand basin (a sunken floor
//!   that fills from sluice grates on a long cycle; water in sky and lunar vaults) and a cursed
//!   corridor (fog vents bursting out of step).
//! * **Puzzles** ([`PuzzleTemplate`]): a stepped mound with the lever on top, or a pressure
//!   plate down a pit near the gate (the gate holds a few seconds after the plate is left).
//! * **Ambushes** ([`AmbushTemplate`]): floating ledges over the floor, or a mezzanine up a
//!   flight of steps; the ambushers drop from the ceiling.
//! * **Shrine**, flanked by pillars (sometimes over a secret cellar).
//! * **Treasure vault** above a puzzle or ambush room: a one-tile step and three two-wide ledges
//!   climb its parent's east end to a hole in the ceiling (a third of the time covered by a false
//!   wall that only the Trap-Sight Lantern shows); caches, and a guard deeper down.
//! * **Guardian's crypt**: a tall arena with two side platforms and a high one in the middle.
//! * **Stairs**.
//!
//! **Secrets** are false walls ([`TrapKind::FalseWall`], hidden): a secret cellar is a false
//! floor slab with a cache beside the drop. Everything on the main path is reachable by the
//! stock pilot (one-tile steps, two-row jumps, gaps of at most two tiles) and by the tombs bot.
//! Moving platforms are not built (no moving solid bodies in the simulation yet).

use crate::{
    base::BuiltTile,
    enemy::EnemyKind,
    terrain::{TileKind, TilePosition},
    worldgen::noise::RegionRng,
};

use super::{
    SiteStyle,
    culture::TombCulture,
    floor::{FloorPlan, RoomKind},
    realize::{
        Canvas, FloorRealizer, MAX_FLOOR_SIZE, RealizedFloor, RealizedRoom, SpawnRole, SpawnSlot,
        TrapKind, rect,
    },
    traps::BLADE_ROOM_ROWS,
};

const SALT_SIDE: u64 = 0x70B5_51DE_0028;

/// The doorway line: the solid floor row under every room's doorways.
pub const FLOOR_ROW: i32 = 30;
/// Interior rows of an ordinary room.
const ROOM_ROWS: i32 = 7;
/// Interior rows of the guardian's crypt.
const BOSS_ROWS: i32 = 11;
/// Interior rows of the vault above its parent.
const VAULT_ROWS: i32 = 5;
/// Columns the vault climb takes at its parent's east end.
const CLIMB_COLUMNS: i32 = 8;
/// Odds (per mille) that an entry, shrine or stairs room hides a secret cellar.
const SECRET_PER_MILLE: u32 = 220;

/// A trap hall's layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrapTemplate {
    SpikePits,
    Blades,
    Crushers,
    Bridge,
    SandBasin,
    FogVents,
}

impl TrapTemplate {
    pub const ALL: [Self; 6] = [
        Self::SpikePits,
        Self::Blades,
        Self::Crushers,
        Self::Bridge,
        Self::SandBasin,
        Self::FogVents,
    ];

    /// The least danger a hall needs before it rolls this layout.
    const fn min_danger(self) -> u8 {
        match self {
            Self::SpikePits | Self::Blades => 0,
            Self::Crushers | Self::Bridge | Self::SandBasin => 3,
            Self::FogVents => 4,
        }
    }

    /// Relative odds per culture (sandstone loves sand and darts, catacombs fog, sky temples
    /// blades and bridges, lunar vaults crushers).
    const fn weight(self, culture: TombCulture) -> u32 {
        let row: [u32; 6] = match culture {
            TombCulture::Sandstone => [3, 2, 2, 2, 3, 1],
            TombCulture::Catacomb => [3, 2, 2, 2, 1, 3],
            TombCulture::SkyTemple => [2, 3, 2, 3, 2, 1],
            TombCulture::LunarVault => [2, 2, 3, 2, 2, 2],
        };
        row[self as usize]
    }

    const fn width(self, danger: u8) -> i32 {
        match self {
            Self::SpikePits | Self::Crushers => 13,
            Self::FogVents => 7 + 5 * fog_vents(danger),
            Self::Blades | Self::Bridge | Self::SandBasin => 14,
        }
    }

    const fn rows(self, danger: u8) -> i32 {
        match self {
            // Dart throwers deeper down fire at jump height under a low ceiling.
            Self::SpikePits => {
                if danger >= 5 {
                    4
                } else {
                    ROOM_ROWS
                }
            }
            Self::Blades => BLADE_ROOM_ROWS,
            Self::Crushers => 4,
            Self::Bridge | Self::SandBasin | Self::FogVents => ROOM_ROWS,
        }
    }
}

/// Cursed vents in a fog corridor: two, three deeper down.
const fn fog_vents(danger: u8) -> i32 {
    if danger >= 6 { 3 } else { 2 }
}

/// A puzzle room's layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PuzzleTemplate {
    LeverMound,
    PlatePit,
}

/// An ambush room's layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbushTemplate {
    Ledges,
    Gallery,
}

/// The layout a room of the floor was given.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    Plain,
    Trap(TrapTemplate),
    Puzzle(PuzzleTemplate),
    Ambush(AmbushTemplate),
}

/// One main-path room before it is drawn.
struct Planned {
    plan: usize,
    kind: RoomKind,
    layout: Layout,
    width: i32,
    min_width: i32,
    rows: i32,
    danger: u8,
    variant: u32,
    vault: Option<usize>,
}

/// The side-view realizer (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct SideViewRealizer;

impl SideViewRealizer {
    /// The layout and size of main-path room `index`.
    fn plan_room(plan: &FloorPlan, index: usize, culture: TombCulture) -> Planned {
        let node = &plan.rooms[index];
        let vault = plan.branches_of(index).first().copied();
        let mut rng = RegionRng::new(
            plan.seed,
            SALT_SIDE,
            i32::try_from(index).unwrap_or(0),
            i32::try_from(node.variant & 0x7FFF_FFFF).unwrap_or(0),
        );
        let wiggle = i32::try_from(node.variant % 3).unwrap_or(0);
        let (layout, min_width, rows) = match node.kind {
            RoomKind::Entry | RoomKind::Shrine => (Layout::Plain, 8, ROOM_ROWS),
            RoomKind::Stairs => (Layout::Plain, 7, ROOM_ROWS),
            RoomKind::Boss => (Layout::Plain, 20, BOSS_ROWS),
            RoomKind::Vault => (Layout::Plain, 8, VAULT_ROWS),
            RoomKind::TrapHall => {
                let weights: Vec<u32> = TrapTemplate::ALL
                    .iter()
                    .map(|template| {
                        if node.danger >= template.min_danger() {
                            template.weight(culture)
                        } else {
                            0
                        }
                    })
                    .collect();
                let template = TrapTemplate::ALL[rng.pick(&weights).unwrap_or(0)];
                (
                    Layout::Trap(template),
                    template.width(node.danger),
                    template.rows(node.danger),
                )
            }
            RoomKind::Puzzle => {
                let template = if rng.chance(500) {
                    PuzzleTemplate::LeverMound
                } else {
                    PuzzleTemplate::PlatePit
                };
                (Layout::Puzzle(template), 13, ROOM_ROWS)
            }
            RoomKind::Ambush => {
                let template = if vault.is_none() && rng.chance(450) {
                    AmbushTemplate::Gallery
                } else {
                    AmbushTemplate::Ledges
                };
                let rows = if template == AmbushTemplate::Gallery {
                    9
                } else {
                    ROOM_ROWS
                };
                (Layout::Ambush(template), 14, rows)
            }
        };
        // A vault's parent keeps its own content in the west and the climb in the east.
        let min_width = if vault.is_some() {
            min_width.max(9 + CLIMB_COLUMNS)
        } else {
            min_width
        };
        let width = match node.kind {
            RoomKind::TrapHall | RoomKind::Puzzle | RoomKind::Ambush => min_width + wiggle,
            _ => min_width,
        };
        Planned {
            plan: index,
            kind: node.kind,
            layout,
            width,
            min_width,
            rows,
            danger: node.danger,
            variant: node.variant,
            vault,
        }
    }
}

/// Puts a spawn slot of `role` in room `room`.
const fn spawn(room: usize, x: i32, y: i32, role: SpawnRole) -> SpawnSlot {
    SpawnSlot {
        room,
        at: TilePosition { x, y },
        role,
    }
}

const fn trap(kind: TrapKind, hidden: bool) -> TileKind {
    TileKind::Built(BuiltTile::TombTrap {
        kind: kind.index(),
        hidden,
    })
}

/// A dart thrower facing `facing` (-1 west, 1 east). Dart throwers are never hidden, so their
/// tile's `hidden` bit carries the facing instead: set fires west ([`dart_facing`]).
const fn dart_wall(facing: i8) -> TileKind {
    trap(TrapKind::DartWall, facing < 0)
}

/// Which way a dart thrower tile fires (-1 west, 1 east): its `hidden` bit (see [`dart_wall`]).
#[must_use]
pub const fn dart_facing(hidden: bool) -> i8 {
    if hidden { -1 } else { 1 }
}

/// Everything a room's drawing needs.
struct Draw<'a> {
    canvas: &'a mut Canvas,
    spawns: &'a mut Vec<SpawnSlot>,
    room: usize,
    left: i32,
    right: i32,
    top: i32,
    floor: i32,
    pillar: TileKind,
}

impl Draw<'_> {
    fn wall(&mut self, x: i32, y: i32) {
        let wall = self.canvas.wall();
        self.canvas.put(x, y, wall);
    }

    fn pillar(&mut self, x: i32, from: i32, to: i32) {
        for y in from..=to {
            let pillar = self.pillar;
            self.canvas.put(x, y, pillar);
        }
    }

    fn spawn(&mut self, x: i32, y: i32, role: SpawnRole) {
        self.spawns.push(spawn(self.room, x, y, role));
    }

    /// A two-deep spike pit two tiles wide at `x`.
    fn spike_pit(&mut self, x: i32, hidden: bool) {
        for dx in 0..2 {
            self.canvas.put(x + dx, self.floor, TileKind::Air);
            self.canvas
                .put(x + dx, self.floor + 1, trap(TrapKind::Spikes, hidden));
        }
    }

    /// A secret cellar: a false floor slab at `x` over a one-row drop with a cache beside it.
    fn cellar(&mut self, x: i32) {
        self.canvas
            .put(x, self.floor, trap(TrapKind::FalseWall, true));
        self.canvas.put(x, self.floor + 1, TileKind::Air);
        self.canvas.put(
            x + 1,
            self.floor + 1,
            TileKind::Built(BuiltTile::TombCache { open: false }),
        );
    }

    fn trap_hall(&mut self, template: TrapTemplate, danger: u8, variant: u32) {
        let (left, right, floor) = (self.left, self.right, self.floor);
        let width = right - left + 1;
        let danger_i = i32::from(danger);
        match template {
            TrapTemplate::SpikePits => {
                let pits = (1 + danger_i / 4).clamp(1, 3);
                let span = width / (pits + 1);
                for pit in 1..=pits {
                    let hidden = (variant >> pit).trailing_zeros() >= 2;
                    self.spike_pit(left + span * pit - 1, hidden);
                }
                if danger >= 5 {
                    // Dart throwers above the doorways fire across the hall at jump height.
                    self.canvas.put(right + 1, floor - 4, dart_wall(-1));
                    if danger >= 9 {
                        self.canvas.put(left - 1, floor - 4, dart_wall(1));
                    }
                }
            }
            TrapTemplate::Blades => {
                // Two pivots in the ceiling, a safe strip between their low swings.
                for x in [left + 3, left + 10] {
                    self.canvas
                        .put(x, floor - BLADE_ROOM_ROWS - 1, trap(TrapKind::Blade, false));
                }
                if danger >= 8 {
                    self.spike_pit(left + 6, variant & 0x40 != 0);
                }
            }
            TrapTemplate::Crushers => {
                let count = (2 + danger_i / 5).clamp(2, 4).min((width - 4) / 3);
                for index in 0..count {
                    self.canvas.put(
                        left + 2 + index * 3,
                        floor - 5,
                        trap(TrapKind::Crusher, false),
                    );
                }
            }
            TrapTemplate::Bridge => {
                let (from, to) = (left + 3, right - 3);
                let middle = i32::midpoint(from, to);
                for x in from..=to {
                    if x == middle {
                        continue;
                    }
                    let hidden = danger >= 8 && (variant >> (x - from)).trailing_zeros() >= 2;
                    self.canvas.put(x, floor, trap(TrapKind::Crumble, hidden));
                    self.canvas.put(x, floor + 1, trap(TrapKind::Spikes, false));
                }
            }
            TrapTemplate::SandBasin => {
                for x in left + 2..=right - 2 {
                    self.canvas.put(x, floor, TileKind::Air);
                    self.canvas.put(x, floor + 1, TileKind::Air);
                }
                for x in [left + 3, i32::midpoint(left, right), right - 3] {
                    self.canvas
                        .put(x, floor + 2, trap(TrapKind::SandSluice, false));
                }
                if danger >= 9 {
                    self.canvas.put(right + 1, floor - 4, dart_wall(-1));
                }
            }
            TrapTemplate::FogVents => {
                // Five apart: a pilot fits between two clouds.
                for index in 0..fog_vents(danger) {
                    self.canvas
                        .put(left + 3 + index * 5, floor, trap(TrapKind::FogVent, false));
                }
            }
        }
        if danger >= 4 {
            self.spawn(right - 1, floor - 1, SpawnRole::Guard);
        }
    }

    /// The puzzle's mechanism and the gate on the doorway east.
    fn puzzle(&mut self, template: PuzzleTemplate, channel: u8, gated: bool, danger: u8) {
        let (left, right, floor) = (self.left, self.right, self.floor);
        let door = right + 1;
        if gated {
            self.canvas.put(
                door,
                floor - 4,
                TileKind::Built(BuiltTile::GateLintel(channel)),
            );
            for dy in 1..=3 {
                self.canvas
                    .put(door, floor - dy, TileKind::Built(BuiltTile::Gate));
            }
        }
        match template {
            PuzzleTemplate::LeverMound => {
                // A one, a two, then a three-high plateau with the lever at its east edge.
                let mound = left + 2;
                self.wall(mound, floor - 1);
                self.wall(mound + 1, floor - 1);
                self.wall(mound + 1, floor - 2);
                for x in mound + 2..=mound + 4 {
                    for dy in 1..=3 {
                        self.wall(x, floor - dy);
                    }
                }
                self.canvas.put(
                    mound + 4,
                    floor - 4,
                    TileKind::Built(BuiltTile::Lever { channel, on: false }),
                );
                if danger >= 5 {
                    self.spawn(mound + 3, floor - 5, SpawnRole::Guard);
                }
            }
            PuzzleTemplate::PlatePit => {
                // The plate sits down a pit a short dash from the gate.
                // A short dash from the gate (under a vault climb's ledges, if there is one:
                // they start high enough).
                let pit = (right - 6).max(left + 2);
                self.canvas.put(pit, floor, TileKind::Air);
                self.canvas.put(pit + 1, floor, TileKind::Air);
                self.canvas.put(
                    pit,
                    floor + 1,
                    TileKind::Built(BuiltTile::PressurePlate {
                        channel,
                        down: false,
                    }),
                );
                if danger >= 5 {
                    self.spawn(left + 2, floor - 1, SpawnRole::Guard);
                }
            }
        }
    }

    fn ambush(&mut self, template: AmbushTemplate, danger: u8, content_right: i32) {
        let (left, top, floor) = (self.left, self.top, self.floor);
        let count = (1 + i32::from(danger) / 4).clamp(1, 4);
        match template {
            AmbushTemplate::Ledges => {
                self.wall(left + 3, floor - 3);
                self.wall(left + 4, floor - 3);
                if content_right - left >= 10 {
                    self.wall(content_right - 3, floor - 3);
                    self.wall(content_right - 2, floor - 3);
                }
            }
            AmbushTemplate::Gallery => {
                // Steps up to a mezzanine over the room's east half.
                let mid = i32::midpoint(left, content_right);
                self.wall(mid - 2, floor - 1);
                self.wall(mid - 1, floor - 1);
                self.wall(mid - 1, floor - 2);
                for x in mid + 1..=content_right {
                    self.wall(x, floor - 4);
                }
            }
        }
        let span = (content_right - left - 3).max(1);
        for index in 0..count {
            self.spawn(
                left + 2 + (index * span) / count,
                top + 1,
                SpawnRole::Ambusher,
            );
        }
        if danger >= 5 {
            self.spawn(left + 1, floor - 1, SpawnRole::Guard);
        }
    }

    /// The climb at the room's east end to a hole in its ceiling, and the vault above.
    fn vault(&mut self, caches: i32, danger: u8, secret: bool, vault_room: usize) -> RealizedRoom {
        let (right, top, floor) = (self.right, self.top, self.floor);
        // A one-tile step, then two-wide ledges two rows apart climbing west to the top row.
        self.wall(right - 1, floor - 1);
        let mut ledge = right - 1;
        let mut step = 1;
        while floor - 1 - 2 * step >= top {
            let (x, y) = (right - 1 - 2 * step, floor - 1 - 2 * step);
            self.wall(x - 1, y);
            self.wall(x, y);
            ledge = x - 1;
            step += 1;
        }
        let vault_floor = top - 1;
        let vault_left = (ledge - 5).max(self.left);
        let interior = rect(
            vault_left,
            vault_floor - VAULT_ROWS,
            vault_left + 9,
            vault_floor - 1,
        );
        self.canvas.open(interior);
        let hole = if secret {
            trap(TrapKind::FalseWall, true)
        } else {
            TileKind::Air
        };
        self.canvas.put(ledge, vault_floor, hole);
        self.canvas.put(ledge + 1, vault_floor, hole);
        // Caches along the vault floor, clear of the hole.
        let mut placed = 0;
        for x in (interior.min.x..=interior.max.x).step_by(2) {
            if placed >= caches {
                break;
            }
            if (ledge - 1..=ledge + 2).contains(&x) {
                continue;
            }
            self.canvas.put(
                x,
                vault_floor - 1,
                TileKind::Built(BuiltTile::TombCache { open: false }),
            );
            placed += 1;
        }
        if danger >= 6 {
            self.spawns.push(spawn(
                vault_room,
                interior.max.x - 1,
                vault_floor - 2,
                SpawnRole::Guard,
            ));
        }
        RealizedRoom {
            plan: 0,
            kind: RoomKind::Vault,
            interior,
        }
    }
}

impl FloorRealizer for SideViewRealizer {
    fn style(&self) -> SiteStyle {
        SiteStyle::SideView
    }

    #[allow(clippy::too_many_lines, reason = "one room kind after another")]
    fn realize(&self, plan: &FloorPlan) -> RealizedFloor {
        let culture = TombCulture::of_setting(plan.setting);
        let mut canvas = Canvas::with_masonry(culture.masonry());
        let pillar = TileKind::Built(BuiltTile::TombPillar(culture.masonry()));
        let floor = FLOOR_ROW;
        let path = plan.main_path();
        let mut planned: Vec<Planned> = path
            .iter()
            .map(|index| Self::plan_room(plan, *index, culture))
            .collect();
        // Fit the slot: narrow the widest rolled rooms back toward their minimum.
        let budget = MAX_FLOOR_SIZE.0 - 6;
        while planned.iter().map(|room| room.width + 1).sum::<i32>() > budget {
            let Some(room) = planned
                .iter_mut()
                .filter(|room| room.width > room.min_width)
                .max_by_key(|room| room.width - room.min_width)
            else {
                break;
            };
            room.width -= 1;
        }

        let mut rooms: Vec<RealizedRoom> = Vec::new();
        let mut spawns = Vec::new();
        let mut start = TilePosition { x: 5, y: floor - 1 };
        let mut x = 2;
        let mut puzzles = 0_u8;
        let mut vaults = Vec::new();
        for (step, room) in planned.iter().enumerate() {
            let (left, right) = (x, x + room.width - 1);
            let top = floor - room.rows;
            let interior = rect(left, top, right, floor - 1);
            canvas.open(interior);
            let last = step + 1 == planned.len();
            if !last {
                canvas.open(rect(right + 1, floor - 3, right + 1, floor - 1));
            }
            let room_index = rooms.len();
            let mut shown = interior;
            let content_right = if room.vault.is_some() {
                right - CLIMB_COLUMNS
            } else {
                right
            };
            let secret = RegionRng::new(
                plan.seed,
                SALT_SIDE ^ 0x5EC2,
                i32::try_from(step).unwrap_or(0),
                7,
            )
            .chance(SECRET_PER_MILLE);
            let mut draw = Draw {
                canvas: &mut canvas,
                spawns: &mut spawns,
                room: room_index,
                left,
                right,
                top,
                floor,
                pillar,
            };
            match (room.kind, room.layout) {
                (RoomKind::Entry, _) => {
                    start = TilePosition {
                        x: left + 3,
                        y: floor - 1,
                    };
                    if plan.floor == 0 {
                        draw.canvas
                            .put(left, floor - 1, TileKind::Built(BuiltTile::TombExit));
                    }
                    draw.pillar(right - 1, floor - 3, floor - 1);
                    if secret {
                        draw.cellar(left + 5);
                    }
                }
                (RoomKind::TrapHall, Layout::Trap(template)) => {
                    draw.trap_hall(template, room.danger, room.variant);
                    if template == TrapTemplate::SandBasin {
                        // The basin is part of the room (the sand fills from its floor).
                        shown.max.y = floor + 1;
                    }
                }
                (RoomKind::Puzzle, Layout::Puzzle(template)) => {
                    let channel = puzzles % crate::mechanism::CHANNELS;
                    puzzles += 1;
                    draw.puzzle(template, channel, !last, room.danger);
                }
                (RoomKind::Ambush, Layout::Ambush(template)) => {
                    draw.ambush(template, room.danger, content_right);
                }
                (RoomKind::Shrine, _) => {
                    draw.canvas.put(
                        i32::midpoint(left, right),
                        floor - 1,
                        TileKind::Built(BuiltTile::TombShrine { lit: false }),
                    );
                    draw.pillar(left + 1, floor - 4, floor - 1);
                    draw.pillar(right - 1, floor - 4, floor - 1);
                    if secret {
                        draw.cellar(left + 2);
                    }
                }
                (RoomKind::Boss, _) => {
                    for x in [
                        left + 2,
                        left + 3,
                        left + 4,
                        right - 4,
                        right - 3,
                        right - 2,
                    ] {
                        draw.wall(x, floor - 4);
                    }
                    for x in left + 8..=left + 11 {
                        draw.wall(x, floor - 7);
                    }
                    draw.spawn(left + 14, floor - 3, SpawnRole::Boss);
                }
                (RoomKind::Stairs, _) => {
                    draw.canvas
                        .put(left + 3, floor - 1, TileKind::Built(BuiltTile::TombStairs));
                    draw.pillar(left + 1, floor - 3, floor - 1);
                    if secret {
                        draw.cellar(left + 5);
                    }
                }
                _ => {}
            }
            rooms.push(RealizedRoom {
                plan: room.plan,
                kind: room.kind,
                interior: shown,
            });
            if let Some(branch) = room.vault {
                let caches = (2 + i32::from(plan.rooms[branch].danger) / 4).clamp(2, 4);
                let hidden = plan.rooms[branch].variant.is_multiple_of(3);
                let frame = Frame {
                    room: room_index,
                    left,
                    right,
                    top,
                };
                vaults.push((branch, caches, hidden, plan.rooms[branch].danger, frame));
            }
            x += room.width + 1;
        }
        // A clean masonry hull round the whole floor: two courses under the floor line and
        // over the rooms, so pits and vents never poke out into the sealed rock.
        canvas.hull(rect(
            1,
            FLOOR_ROW - BOSS_ROWS - VAULT_ROWS - 4,
            x,
            floor + 3,
        ));
        // The vaults go after the main path (their room indices follow it).
        for (branch, caches, hidden, danger, frame) in vaults {
            let vault_room = rooms.len();
            let mut draw = Draw {
                canvas: &mut canvas,
                spawns: &mut spawns,
                room: frame.room,
                left: frame.left,
                right: frame.right,
                top: frame.top,
                floor,
                pillar,
            };
            let mut vault = draw.vault(caches, danger, hidden, vault_room);
            vault.plan = branch;
            rooms.push(vault);
        }
        RealizedFloor {
            style: SiteStyle::SideView,
            size: (x + 2, floor + 4),
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

/// A room's frame, kept to draw its vault once the main path is laid out.
#[derive(Clone, Copy)]
struct Frame {
    room: usize,
    left: i32,
    right: i32,
    top: i32,
}

/// Health fraction above which a side-view guardian is in its first phase.
pub const GUARDIAN_PHASE_TWO: f32 = 0.6;
/// Health fraction at or below which it is enraged (its third phase).
pub const GUARDIAN_PHASE_THREE: f32 = 0.3;

/// A side-view guardian's phase (1-3) at health fraction `fraction`.
///
/// The Sunwrought Colossus adds thrown rubble and raises mummies in phase 2 and enrages in 3;
/// the Hollow Matriarch adds hex rings and crypt bats in phase 2 and dives in 3.
#[must_use]
pub fn guardian_phase(fraction: f32) -> u8 {
    if fraction > GUARDIAN_PHASE_TWO {
        1
    } else if fraction > GUARDIAN_PHASE_THREE {
        2
    } else {
        3
    }
}

/// The side-view bestiary ([`super::realize::bestiary`] for [`SiteStyle::SideView`]).
///
/// Mummies and skeletal archers guard, animated statues stand dormant in some rooms, crypt bats
/// and mummies spring the ambushes, and the guardian is the Sunwrought Colossus or the Hollow
/// Matriarch (by the spawn's roll: each run of a tomb wakes one of the two).
#[must_use]
pub const fn bestiary(role: SpawnRole, variant: u32) -> EnemyKind {
    match role {
        SpawnRole::Guard => match variant % 5 {
            0 | 4 => EnemyKind::Mummy,
            2 => EnemyKind::AnimatedStatue,
            _ => EnemyKind::SkeletalArcher,
        },
        SpawnRole::Ambusher => match variant % 5 {
            0 | 1 => EnemyKind::CryptBat,
            2 | 3 => EnemyKind::Mummy,
            _ => EnemyKind::SkeletalArcher,
        },
        SpawnRole::Boss => {
            if variant.is_multiple_of(2) {
                EnemyKind::SunColossus
            } else {
                EnemyKind::HollowMatriarch
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tomb::{SiteSetting, floor::plan_floor, realize::realizer_for};

    fn site(setting: SiteSetting, tier: u8) -> crate::tomb::TombSite {
        crate::tomb::TombSite {
            setting,
            ..crate::tomb::floor::tests::site(SiteStyle::SideView, tier)
        }
    }

    #[test]
    fn floors_use_the_culture_masonry_and_every_template_shows_up() {
        let mut seen = std::collections::BTreeSet::new();
        for setting in SiteSetting::ALL {
            let culture = TombCulture::of_setting(setting);
            for tier in 1..=5 {
                let site = site(setting, tier);
                for run in 0..6 {
                    for floor in 0..site.floors() {
                        let plan = plan_floor(&site, run, floor);
                        let realized = realizer_for(SiteStyle::SideView).realize(&plan);
                        assert!(realized.cells.iter().any(|(_, kind)| *kind
                            == TileKind::Built(BuiltTile::TombWall(culture.masonry()))));
                        for (_, kind) in &realized.cells {
                            if let TileKind::Built(BuiltTile::TombTrap { kind, .. }) = kind {
                                seen.insert(*kind);
                            }
                        }
                    }
                }
            }
        }
        for kind in TrapKind::ALL {
            assert!(seen.contains(&kind.index()), "{kind:?} never built");
        }
    }

    #[test]
    fn traps_sit_in_masonry_where_their_shapes_need_room() {
        for tier in 1..=5 {
            let site = site(SiteSetting::Mesa, tier);
            for run in 0..8 {
                for floor in 0..site.floors() {
                    let plan = plan_floor(&site, run, floor);
                    let realized = realizer_for(SiteStyle::SideView).realize(&plan);
                    let cell = |x: i32, y: i32| realized.cell(TilePosition { x, y });
                    for (tile, cell_kind) in &realized.cells {
                        let TileKind::Built(BuiltTile::TombTrap { kind, hidden }) = *cell_kind
                        else {
                            continue;
                        };
                        match TrapKind::from_index(kind) {
                            TrapKind::Blade => {
                                // Open below the pivot down to the floor.
                                for dy in 1..=BLADE_ROOM_ROWS {
                                    assert_eq!(cell(tile.x, tile.y + dy), Some(TileKind::Air));
                                }
                            }
                            TrapKind::Crusher => {
                                assert_eq!(cell(tile.x, tile.y + 1), Some(TileKind::Air));
                            }
                            TrapKind::DartWall => {
                                let facing = i32::from(dart_facing(hidden));
                                assert_eq!(
                                    cell(tile.x + facing, tile.y),
                                    Some(TileKind::Air),
                                    "a dart thrower faces open air at {tile:?}"
                                );
                            }
                            TrapKind::FogVent | TrapKind::SandSluice => {
                                assert_eq!(cell(tile.x, tile.y - 1), Some(TileKind::Air));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_guardian_room_wakes_a_side_view_guardian() {
        let kinds: std::collections::BTreeSet<EnemyKind> = (0..8)
            .map(|variant| bestiary(SpawnRole::Boss, variant))
            .collect();
        assert!(kinds.contains(&EnemyKind::SunColossus));
        assert!(kinds.contains(&EnemyKind::HollowMatriarch));
        for variant in 0..20 {
            assert!(!bestiary(SpawnRole::Guard, variant).stats().elite);
        }
    }

    /// The floor as text (`#` masonry, `|` pillars, traps by initial, lower case hidden).
    pub fn ascii(realized: &RealizedFloor, from_row: i32) -> String {
        let mut out = String::new();
        for y in from_row..realized.size.1 {
            for x in 0..realized.size.0 {
                let tile = TilePosition { x, y };
                let glyph = match realized.cell(tile) {
                    None => ' ',
                    Some(TileKind::Air) => {
                        if realized.spawns.iter().any(|spawn| spawn.at == tile) {
                            'e'
                        } else if realized.start == tile {
                            '@'
                        } else {
                            '.'
                        }
                    }
                    Some(TileKind::Built(BuiltTile::TombWall(_))) => '#',
                    Some(TileKind::Built(BuiltTile::TombPillar(_))) => '|',
                    Some(TileKind::Built(BuiltTile::TombTrap { kind, hidden })) => {
                        let glyph = ['^', 'B', 'C', 'K', 'D', 'S', 'F', 'W'][usize::from(kind % 8)];
                        if hidden {
                            glyph.to_ascii_lowercase()
                        } else {
                            glyph
                        }
                    }
                    Some(TileKind::Built(BuiltTile::TombCache { .. })) => '$',
                    Some(TileKind::Built(BuiltTile::TombShrine { .. })) => 'A',
                    Some(TileKind::Built(BuiltTile::TombStairs)) => '>',
                    Some(TileKind::Built(BuiltTile::TombExit)) => '<',
                    Some(TileKind::Built(BuiltTile::Gate)) => 'G',
                    Some(TileKind::Built(BuiltTile::GateLintel(_))) => 'L',
                    Some(TileKind::Built(BuiltTile::Lever { .. })) => '/',
                    Some(TileKind::Built(BuiltTile::PressurePlate { .. })) => '_',
                    Some(_) => '?',
                };
                out.push(glyph);
            }
            out.push('\n');
        }
        out
    }

    #[test]
    #[ignore = "debug: prints a few floors"]
    fn print_floors() {
        for (setting, tier, run) in [
            (SiteSetting::Mesa, 1, 0),
            (SiteSetting::Deep, 3, 1),
            (SiteSetting::Moon, 5, 2),
        ] {
            let site = site(setting, tier);
            for floor in [0, site.floors() - 1] {
                let plan = plan_floor(&site, run, floor);
                let realized = realizer_for(SiteStyle::SideView).realize(&plan);
                println!(
                    "{setting:?} tier {tier} run {run} floor {floor} {:?}",
                    realized.size
                );
                println!("{}", ascii(&realized, 8));
            }
        }
    }
}
