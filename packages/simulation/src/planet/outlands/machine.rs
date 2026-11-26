//! The machine world (Wave 13 planets-c): a planet-sized ancient machine of the builders.
//!
//! Riveted plate terraces and pylons on the surface; below, the Plate Crust, the Gear Works
//! and the Reactor Shell round the core reactor.
//!
//! * **Surface**: terraces of hull plate (flat plates in four-row steps, welded seams), a
//!   pylon of girders with a beacon on top in about half the 64-column cells.
//! * **Plate Crust** (the top third): scrap fill between armour plates, and long **conveyor
//!   galleries**; signature rule **conveyors**: a belt carries whatever rests on it (a rig, a
//!   pilot) along at [`crate::world::outlands::CONVEYOR_SPEED`]; drill a belt to stop it.
//! * **Gear Works**: packed cogwork round gear rooms where great **gears** stand; signature
//!   rule **grinding gears**: the works turn on one rhythm of world time (a clank, then
//!   [`crate::planet::outlands::life::GEAR_TURN_MILLIS`] of turning, then rest), and a turning
//!   gear grinds whatever touches it. Wait for the rest, then slip past.
//! * **Reactor Shell**: cable conduit and casing round sealed **power vaults**; signature rule
//!   **power relays**: a vault's three relays sit in its roof, under a control room. Drilling a
//!   relay flips it and its neighbours; light all three and the vault's seals open.
//! * **Landmark: the Reactor Spire**, half a lap from the settlement: a tower of reactor
//!   casing 12 wide and 64 tall with a beacon crown, a door at its foot and a shaft down to the
//!   core reactor's hall where the Reactor Warden waits ([`super::boss_lair`]).
//! * **Settlement site**: a level plate for the Tokkari's forges.

use super::{
    OutlandTile, air, boss_lair, lair_tile, landmark_column, oroll, outland, outpost_column,
    outpost_tile, pick, secret_tile,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, PlanetThemeId,
        SurfaceSite, ThemeWorld,
        wilds::{
            Cell, chunk_with, core_floor_row, ease, is_small_body, secret_column,
            settlement_column, settlement_site_of, settlement_width, site_weight, wrap_delta,
        },
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind},
};

/// The machine world's strata (registry data).
pub const MACHINE_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Plate Crust",
        share_permille: 300,
        rock: TileKind::Outland(OutlandTile::Scrapfill),
        accent: TileKind::Outland(OutlandTile::HullPlate),
        accent_permille: 260,
        ores: &[
            (MineralKind::Iron, 110),
            (MineralKind::Copper, 70),
            (MineralKind::Silver, 30),
        ],
    },
    PlanetStratum {
        name: "Gear Works",
        share_permille: 330,
        rock: TileKind::Outland(OutlandTile::Cogwork),
        accent: TileKind::Outland(OutlandTile::HullPlate),
        accent_permille: 120,
        ores: &[
            (MineralKind::Gold, 42),
            (MineralKind::Platinum, 22),
            (MineralKind::Uranium, 14),
        ],
    },
    PlanetStratum {
        name: "Reactor Shell",
        share_permille: 370,
        rock: TileKind::Outland(OutlandTile::Conduit),
        accent: TileKind::Outland(OutlandTile::ReactorCasing),
        accent_permille: 140,
        ores: &[
            (MineralKind::Platinum, 30),
            (MineralKind::Diamond, 20),
            (MineralKind::Mythril, 10),
        ],
    },
];

/// The machine theme (registry entry [`PlanetThemeId::MACHINE`]).
pub const MACHINE_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::MACHINE,
    name: "Machine World",
    palette: PlanetPalette {
        sky: [70, 86, 104],
        atmosphere: [150, 190, 220],
        surface: [128, 138, 150],
        rock: [96, 100, 110],
        deep: [60, 58, 70],
        core: [120, 220, 255],
    },
    crust: TileKind::Outland(OutlandTile::HullPlate),
    crust_rows: 3,
    strata: &MACHINE_STRATA,
    core_ore: TileKind::Ore(MineralKind::Platinum),
    caves_per_256_columns: 1,
    hill_permille: 24,
};

/// The machine theme world.
pub struct MachineWorld;

/// The registered machine world.
pub static MACHINE_WORLD: MachineWorld = MachineWorld;

/// Rows per terrace step.
pub const TERRACE_ROWS: i32 = 4;
/// Columns per pylon cell.
pub const PYLON_CELL: i64 = 64;
/// The Reactor Spire: half width, height, its rise.
pub const SPIRE_HALF: i64 = 6;
pub const SPIRE_HEIGHT: i32 = 64;
const SPIRE_RISE: i64 = 32;
/// Extra sky over the hills (the spire).
const EXTRA_SKY: i32 = 72;
/// A power vault's outer size (columns, rows), the control room's rows over it.
pub const VAULT_BOX: (i64, i32) = (11, 6);
pub const CONTROL_ROWS: i32 = 3;
/// The relays' columns from the vault's first column.
pub const RELAY_OFFSETS: [i64; 3] = [2, 5, 8];
/// Columns and rows per power vault cell.
pub const VAULT_CELL: (i64, i32) = (120, 60);
/// Columns and rows per conveyor gallery cell; per gear room cell.
const GALLERY_CELL: (i64, i32) = (80, 30);
const GEAR_CELL: (i64, i32) = (64, 28);

/// Whether `column` is kept clear: the settlement (and a straight shaft under it), the
/// spire's rise, the outpost, a small body's secret.
fn kept_clear(ground: &PlanetGround, column: i64, margin: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let near = |centre: i64, half: i64| wrap_delta(column, centre, width).abs() <= half + margin;
    let settlement_half = i64::from(settlement_width(ground) / 2);
    near(settlement_column(ground) + settlement_half, settlement_half)
        || near(landmark_column(ground), SPIRE_RISE)
        || near(outpost_column(ground), super::OUTPOST_HALF + 2)
        || (is_small_body(ground.params()) && near(secret_column(ground), 4))
}

/// The terraced surface row at `column`, levelled at the sites.
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let step = |column: i64| -(ground.hill(column).div_euclid(TERRACE_ROWS) * TERRACE_ROWS);
    let natural = step(column);
    let half = i64::from(settlement_width(ground) / 2);
    let settlement = settlement_column(ground) + half;
    if site_weight(column, settlement, half + 12, width).is_some() {
        return step(settlement);
    }
    let landmark = landmark_column(ground);
    if site_weight(column, landmark, SPIRE_RISE, width).is_some() {
        return step(landmark);
    }
    let outpost = outpost_column(ground);
    if let Some(weight) = site_weight(column, outpost, super::OUTPOST_HALF, width) {
        return ease(natural, step(outpost), weight);
    }
    natural
}

/// A pylon of cell `cell`: its column and height.
#[must_use]
pub fn pylon_in_cell(ground: &PlanetGround, cell: i64) -> Option<(i64, i32)> {
    let width = i64::from(ground.strip_width());
    let cells = (width / PYLON_CELL).max(1);
    let cell = cell.rem_euclid(cells);
    let hash = oroll(ground, 40, cell, 0);
    if hash % 2 == 1 {
        return None;
    }
    let column = cell * PYLON_CELL + 8 + pick(hash, 8, 48);
    if kept_clear(ground, column, 8) {
        return None;
    }
    Some((column, 12 + i32::try_from(pick(hash, 16, 15)).unwrap_or(0)))
}

/// A power vault in the Reactor Shell: its outer box in ring tiles (deep down a ring tile
/// spans several strip columns, so the vault keeps its shape in the ring).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerVault {
    /// The box's first strip column (a ring tile's first) and top (roof) row.
    pub column: i64,
    pub row: i32,
    /// Strip columns one of its ring tiles spans.
    pub span: i64,
    /// The relays' starting states (bit `i` lit), never all three.
    pub start: u8,
}

impl PowerVault {
    /// The strip tiles of its three relays (each its ring tile's first column), west to east.
    #[must_use]
    pub fn relays(&self, width: i64) -> [(i32, i32); 3] {
        RELAY_OFFSETS.map(|offset| {
            (
                i32::try_from((self.column + offset * self.span).rem_euclid(width)).unwrap_or(0),
                self.row,
            )
        })
    }

    /// Ring tiles from the box's first column to strip column `column` (any of a ring tile's
    /// strip columns reads its tile).
    #[must_use]
    pub const fn ring_dx(&self, column: i64, width: i64) -> i64 {
        wrap_delta(column, self.column, width).div_euclid(self.span)
    }
}

/// The power vault of the cell holding strip tile `(column, row)`, if it has one.
#[must_use]
pub fn vault_in_cell(ground: &PlanetGround, column: i64, row: i32) -> Option<PowerVault> {
    let width = i64::from(ground.strip_width());
    let cells = (width / VAULT_CELL.0).max(1);
    let cx = column.div_euclid(VAULT_CELL.0);
    let cy = row.div_euclid(VAULT_CELL.1);
    let hash = oroll(ground, 50, cx.rem_euclid(cells), i64::from(cy));
    if hash % 10 >= 4 {
        return None;
    }
    let row = cy * VAULT_CELL.1 + 10 + i32::try_from(pick(hash, 16, 38)).unwrap_or(0);
    // One ring band from its control room to its floor (never across a halving row).
    let span = i64::from(ground.grid().span_at(row - CONTROL_ROWS));
    if span != i64::from(ground.grid().span_at(row + VAULT_BOX.1)) {
        return None;
    }
    let room = (VAULT_CELL.0 - 13 - VAULT_BOX.0 * span).max(1);
    let raw = cx * VAULT_CELL.0 + 12 + pick(hash, 8, room.cast_unsigned());
    let vault = PowerVault {
        column: (raw - raw.rem_euclid(span)).rem_euclid(width),
        row,
        span,
        start: u8::try_from(pick(hash, 24, 7)).unwrap_or(0),
    };
    // Never across the strip's seam (each tile finds its vault through its own cell).
    if vault.column + VAULT_BOX.0 * span >= width
        || cx.rem_euclid(width.div_euclid(VAULT_CELL.0) + 1) >= cells
        || kept_clear(ground, vault.column, 64)
        || kept_clear(ground, vault.column + VAULT_BOX.0 * span, 64)
    {
        return None;
    }
    Some(vault)
}

/// The vault whose box or control room holds strip tile `(column, row)`, if any (generation
/// and the relay rule).
#[must_use]
pub fn vault_at(ground: &PlanetGround, column: i64, row: i32) -> Option<PowerVault> {
    // A vault's box and control room lie inside its own cell.
    let width = i64::from(ground.strip_width());
    let vault = vault_in_cell(ground, column, row)?;
    let dx = vault.ring_dx(column, width);
    ((0..VAULT_BOX.0).contains(&dx)
        && (vault.row - CONTROL_ROWS..vault.row + VAULT_BOX.1).contains(&row))
    .then_some(vault)
}

/// A vault's tile at `(dx, dy)` from its box's first column and roof row.
fn vault_tile(ground: &PlanetGround, vault: &PowerVault, dx: i64, dy: i32) -> Option<Tile> {
    let (box_w, box_h) = VAULT_BOX;
    if !(0..box_w).contains(&dx) || !(-CONTROL_ROWS..box_h).contains(&dy) {
        return None;
    }
    if dy < 0 {
        // The control room over the roof.
        return Some(air());
    }
    if dy == 0 {
        if let Some(index) = RELAY_OFFSETS.iter().position(|offset| *offset == dx) {
            return Some(outland(if vault.start & (1 << index) != 0 {
                OutlandTile::RelayOn
            } else {
                OutlandTile::RelayOff
            }));
        }
        if dx > 0 && dx < box_w - 1 {
            return Some(outland(OutlandTile::PowerSeal));
        }
    }
    if dx == 0 || dx == box_w - 1 || dy == 0 || dy == box_h - 1 {
        return Some(outland(OutlandTile::VaultWall));
    }
    // Inside: the builders' cache and core ore along the floor.
    if dy == box_h - 2 {
        if dx == box_w / 2 {
            return Some(outland(OutlandTile::BuilderCache));
        }
        if dx % 2 == 1 {
            return Some(super::tile(ground.data_theme().core_ore));
        }
    }
    Some(air())
}

/// What a conveyor gallery puts at a strip tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Gallery {
    /// The gallery's open air.
    Open,
    /// Its floor: a belt.
    Belt(OutlandTile),
}

/// Whether strip tile `(first, row)` is in a conveyor gallery (its air, or its belt floor).
fn gallery_at(ground: &PlanetGround, first: i64, row: i32) -> Option<Gallery> {
    let width = i64::from(ground.strip_width());
    let cells = (width / GALLERY_CELL.0).max(1);
    let cx = first.div_euclid(GALLERY_CELL.0);
    let cy = row.div_euclid(GALLERY_CELL.1);
    for ox in [0, -1] {
        let cell = cx + ox;
        let hash = oroll(ground, 60, cell.rem_euclid(cells), i64::from(cy));
        if hash % 10 >= 6 {
            continue;
        }
        let start = cell * GALLERY_CELL.0 + pick(hash, 8, 40);
        let length = 24 + pick(hash, 16, 40);
        let floor = cy * GALLERY_CELL.1 + 8 + i32::try_from(pick(hash, 24, 16)).unwrap_or(0);
        let dx = wrap_delta(first, start, width);
        if !(0..length).contains(&dx) || kept_clear(ground, start, 4) {
            continue;
        }
        if row == floor {
            let east = (hash >> 40) & 1 == 0;
            return Some(Gallery::Belt(if east {
                OutlandTile::ConveyorEast
            } else {
                OutlandTile::ConveyorWest
            }));
        }
        if (floor - 3..floor).contains(&row) {
            return Some(Gallery::Open);
        }
    }
    None
}

/// The gear of a gear room near strip tile `(first, row)`, if the tile is part of one: a
/// toothed disc (`Some(true)`), or the room's open air (`Some(false)`).
fn gear_room_at(ground: &PlanetGround, first: i64, row: i32, span: i64) -> Option<bool> {
    let width = i64::from(ground.strip_width());
    let cells = (width / GEAR_CELL.0).max(1);
    let cx = first.div_euclid(GEAR_CELL.0);
    let cy = row.div_euclid(GEAR_CELL.1);
    for ox in -1..=1 {
        for oy in -1..=1 {
            let (cell_x, cell_y) = (cx + ox, cy + oy);
            let hash = oroll(ground, 70, cell_x.rem_euclid(cells), i64::from(cell_y));
            if hash % 10 >= 5 {
                continue;
            }
            let centre_x = cell_x * GEAR_CELL.0 + 12 + pick(hash, 8, 40);
            let centre_y =
                cell_y * GEAR_CELL.1 + 6 + i32::try_from(pick(hash, 16, 16)).unwrap_or(0);
            if kept_clear(ground, centre_x, 12) {
                continue;
            }
            let dx = wrap_delta(first, centre_x, width).div_euclid(span.max(1));
            let dy = i64::from(row - centre_y);
            // The room: an ellipse 9 by 5 (in ring tiles).
            let room = dx * dx * 25 + dy * dy * 81 <= 81 * 25;
            if !room {
                continue;
            }
            // Two gears on its floor, one at each end, and one hung from the roof between.
            for (gx, gy, radius) in [(-5_i64, 2_i64, 2_i64), (5, 2, 2), (0, -3, 2)] {
                let (ddx, ddy) = (dx - gx, dy - gy);
                let distance2 = ddx * ddx + ddy * ddy;
                let tooth = (ddx == 0 || ddy == 0 || ddx.abs() == ddy.abs())
                    && distance2 <= (radius + 1) * (radius + 1) + 1;
                if distance2 <= radius * radius + 1 || tooth {
                    return Some(true);
                }
            }
            return Some(false);
        }
    }
    None
}

/// The Reactor Spire's tile at `(column, row)`, if any.
fn spire_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let dx = wrap_delta(column, landmark_column(ground), width);
    if dx.abs() > SPIRE_HALF + 2 {
        return None;
    }
    let floor = super::landmark_floor(ground);
    let top = floor - SPIRE_HEIGHT;
    if row >= floor || row < top - 5 {
        return None;
    }
    // The beacon crown: a mast and a lamp.
    if row < top {
        if dx == 0 {
            return Some(outland(if row <= top - 4 {
                OutlandTile::Beacon
            } else {
                OutlandTile::Pylon
            }));
        }
        return None;
    }
    // Flared buttresses at the foot.
    let flare = i64::from((row - (floor - 10)).max(0)) / 4;
    if dx.abs() > SPIRE_HALF + flare.min(2) {
        return None;
    }
    // The hollow core and the door at the foot (west).
    let shaft = dx.abs() <= super::LAIR_SHAFT_HALF && row > top + 2;
    let door = dx < 0 && (floor - 4..floor).contains(&row);
    if shaft || door {
        return Some(air());
    }
    // Lit windows up the faces.
    if dx.abs() == SPIRE_HALF - 1 && (row - top).rem_euclid(6) == 3 {
        return Some(outland(OutlandTile::Beacon));
    }
    Some(outland(OutlandTile::ReactorCasing))
}

impl ThemeWorld for MachineWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::MACHINE
    }

    fn core_floor_row(&self, params: &PlanetParams, grid: &PolarGrid) -> Option<i32> {
        Some(core_floor_row(params, grid))
    }

    fn extra_sky_rows(&self) -> i32 {
        EXTRA_SKY
    }

    fn surface_row(&self, ground: &PlanetGround, column: i64) -> i32 {
        surface(ground, column)
    }

    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile> {
        let width = i64::from(ground.strip_width());
        let origin = chunk.origin();
        let first_cell = i64::from(origin.x).div_euclid(PYLON_CELL) - 1;
        let pylons: Vec<(i64, i32)> = (first_cell..=first_cell + 2)
            .filter_map(|cell| pylon_in_cell(ground, cell))
            .collect();
        let near_landmark = wrap_delta(i64::from(origin.x) + 16, landmark_column(ground), width)
            .abs()
            <= super::LAIR_HALF_WIDTH + super::VAULT_WIDTH + 40;
        let lair = if near_landmark {
            boss_lair(ground)
        } else {
            None
        };
        let near_outpost =
            wrap_delta(i64::from(origin.x) + 16, outpost_column(ground), width).abs() <= 40;
        let outpost_floor = surface(ground, outpost_column(ground));
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                if near_landmark {
                    if let Some(tile) = spire_tile(ground, cell.first, cell.row) {
                        return tile;
                    }
                    if let Some(lair) = &lair
                        && let Some(tile) =
                            lair_tile(ground, lair, cell, outland(OutlandTile::ReactorCasing))
                    {
                        return tile;
                    }
                }
                if near_outpost && let Some(tile) = outpost_tile(ground, cell, outpost_floor) {
                    return tile;
                }
                if let Some(tile) = secret_tile(
                    ground,
                    cell,
                    outland(OutlandTile::ReactorCasing),
                    outland(OutlandTile::Beacon),
                ) {
                    return tile;
                }
                if cell.row < cell.surface {
                    if cell.span == 1 {
                        for (column, height) in &pylons {
                            let dx = wrap_delta(cell.first, *column, width);
                            let floor = surface(ground, *column);
                            if (0..=1).contains(&dx) && (floor - height..floor).contains(&cell.row)
                            {
                                return outland(if cell.row == floor - height {
                                    OutlandTile::Beacon
                                } else {
                                    OutlandTile::Pylon
                                });
                            }
                        }
                    }
                    return cell.base;
                }
                // Welded seams down the plates.
                if cell.depth < 3 && cell.first.rem_euclid(12) == 0 {
                    return outland(OutlandTile::PlateSeam);
                }
                match cell.stratum {
                    0 if cell.depth > 6 => match gallery_at(ground, cell.first, cell.row) {
                        Some(Gallery::Belt(belt)) => outland(belt),
                        Some(Gallery::Open) => air(),
                        None => cell.base,
                    },
                    1 => match gear_room_at(ground, cell.first, cell.row, cell.span) {
                        Some(true) => outland(OutlandTile::Gear),
                        Some(false) => air(),
                        None => cell.base,
                    },
                    2 => {
                        if let Some(vault) = vault_at(ground, cell.first, cell.row) {
                            let dx = vault.ring_dx(cell.first, width);
                            if let Some(tile) = vault_tile(ground, &vault, dx, cell.row - vault.row)
                            {
                                return tile;
                            }
                        }
                        cell.base
                    }
                    _ => cell.base,
                }
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        Some(settlement_site_of(ground, |column| surface(ground, column)))
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        let centre = landmark_column(ground);
        Some(SurfaceSite {
            first: i32::try_from(centre - SPIRE_HALF).unwrap_or(0),
            width: i32::try_from(SPIRE_HALF * 2 + 1).unwrap_or(13),
            floor: surface(ground, centre),
        })
    }
}
