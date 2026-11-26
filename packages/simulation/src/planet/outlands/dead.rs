//! The dead world (Wave 13 planets-c): a world that died. Ash plains and petrified cities on
//! the surface; below, the Ash Beds, the Necropolis and the Grave Mantle.
//!
//! * **Surface**: low ash plains (loose ash over ash stone) with long, gentle drifts, and in
//!   about half of the 192-column cells a **petrified city**: towers of petrified stone with
//!   floors every six rows (holes at alternating ends to fly up through), petrified windows, a
//!   door at street level, burial urns on their floors. Explore them.
//! * **Ash Beds** (the top third): ash stone with drifts of loose ash; signature rule
//!   **ashfall**: loose ash pours down into open space under it (a shaft dug through a drift
//!   fills in behind you; never onto a rig or a pilot).
//! * **The Necropolis**: ossuary stone round sealed **tombs** (echoing the home world's
//!   crypts): tomb brick, a sarcophagus and urns inside; signature rule **tomb wardens**: break
//!   a tomb's seal and its two gravewardens wake inside.
//! * **Grave Mantle**: black grave rock veined with soulstone; signature rule **ghost lights**:
//!   wisps of ghost light drift through the rock on slow paths of world time
//!   ([`super::life::wisps_near`]); one that touches a rig chills its hull (the cold gear
//!   counters it). They are seen before they bite.
//! * **Landmark: the Hollow Throne**, half a lap from the settlement: a stepped ziggurat of
//!   petrified stone 64 wide with a throne on its crown, a stair-shaft through it down to the
//!   tomb where the Ashen Sovereign waits ([`super::boss_lair`]).
//! * **Settlement site**: a swept stretch of plain (the Siphra keep the dead worlds' memory).

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

/// The dead world's strata (registry data).
pub const DEAD_STRATA: [PlanetStratum; 3] = [
    PlanetStratum {
        name: "Ash Beds",
        share_permille: 300,
        rock: TileKind::Outland(OutlandTile::AshStone),
        accent: TileKind::Outland(OutlandTile::AshDrift),
        accent_permille: 240,
        ores: &[
            (MineralKind::Iron, 90),
            (MineralKind::Silver, 45),
            (MineralKind::Gold, 18),
        ],
    },
    PlanetStratum {
        name: "The Necropolis",
        share_permille: 350,
        rock: TileKind::Outland(OutlandTile::Ossuary),
        accent: TileKind::Outland(OutlandTile::Petrified),
        accent_permille: 150,
        ores: &[
            (MineralKind::Gold, 40),
            (MineralKind::Emerald, 20),
            (MineralKind::Ruby, 16),
        ],
    },
    PlanetStratum {
        name: "Grave Mantle",
        share_permille: 350,
        rock: TileKind::Outland(OutlandTile::GraveRock),
        accent: TileKind::Outland(OutlandTile::Soulstone),
        accent_permille: 70,
        ores: &[
            (MineralKind::Diamond, 24),
            (MineralKind::Platinum, 24),
            (MineralKind::Mythril, 10),
        ],
    },
];

/// The dead theme (registry entry [`PlanetThemeId::DEAD`]).
pub const DEAD_THEME: PlanetTheme = PlanetTheme {
    id: PlanetThemeId::DEAD,
    name: "Dead World",
    palette: PlanetPalette {
        sky: [52, 50, 56],
        atmosphere: [130, 140, 130],
        surface: [118, 114, 110],
        rock: [88, 84, 84],
        deep: [40, 38, 46],
        core: [170, 255, 210],
    },
    crust: TileKind::Outland(OutlandTile::AshDrift),
    crust_rows: 3,
    strata: &DEAD_STRATA,
    core_ore: TileKind::Ore(MineralKind::Mythril),
    caves_per_256_columns: 1,
    hill_permille: 10,
};

/// The dead theme world.
pub struct DeadWorld;

/// The registered dead world.
pub static DEAD_WORLD: DeadWorld = DeadWorld;

/// Columns per city cell.
pub const CITY_CELL: i64 = 192;
/// Rows per storey of a petrified tower.
pub const STOREY: i32 = 6;
/// The Hollow Throne: half width at its base, step rows, steps.
pub const THRONE_HALF: i64 = 32;
const THRONE_STEP: i32 = 6;
const THRONE_STEPS: i32 = 5;
const THRONE_RISE: i64 = 40;
/// Columns and rows per tomb cell.
pub const TOMB_CELL: (i64, i32) = (72, 36);
/// A tomb's outer size.
pub const TOMB_BOX: (i64, i32) = (13, 7);
/// Extra sky over the plains (towers and the throne).
const EXTRA_SKY: i32 = 48;

/// Whether `column` is kept clear: the settlement, the throne's rise, the outpost, a small
/// body's secret.
fn kept_clear(ground: &PlanetGround, column: i64, margin: i64) -> bool {
    let width = i64::from(ground.strip_width());
    let near = |centre: i64, half: i64| wrap_delta(column, centre, width).abs() <= half + margin;
    let settlement_half = i64::from(settlement_width(ground) / 2);
    near(settlement_column(ground) + settlement_half, settlement_half)
        || near(landmark_column(ground), THRONE_RISE)
        || near(outpost_column(ground), super::OUTPOST_HALF + 2)
        || (is_small_body(ground.params()) && near(secret_column(ground), 4))
}

/// A petrified city: its first column, width and the street row (its level ground).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct City {
    pub first: i64,
    pub width: i64,
}

/// The city of cell `cell`, if it has one.
#[must_use]
pub fn city_in_cell(ground: &PlanetGround, cell: i64) -> Option<City> {
    let width = i64::from(ground.strip_width());
    let cells = (width / CITY_CELL).max(1);
    // The lap's last, partial cell holds no city (it would straddle the seam).
    let all = ((width + CITY_CELL - 1) / CITY_CELL).max(1);
    let cell = cell.rem_euclid(all);
    if cell >= cells {
        return None;
    }
    let hash = oroll(ground, 80, cell, 0);
    if hash % 20 >= 11 {
        return None;
    }
    let city = City {
        first: cell * CITY_CELL + 16 + pick(hash, 8, 40),
        width: 56 + pick(hash, 16, 64),
    };
    if (city.first..city.first + city.width)
        .step_by(8)
        .any(|x| kept_clear(ground, x, 6))
    {
        return None;
    }
    Some(city)
}

/// The ash plain's natural row at `column` (low hills and long drifts).
fn plain(ground: &PlanetGround, column: i64) -> i32 {
    let drift = ground.periodic_noise(81, column, 64) * 3 / 1_024;
    -ground.hill(column) - i32::try_from(drift).unwrap_or(0)
}

/// The dead surface row at `column`: ash plains, levelled at the sites and under cities.
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    let width = i64::from(ground.strip_width());
    let natural = plain(ground, column);
    let half = i64::from(settlement_width(ground) / 2);
    let settlement = settlement_column(ground) + half;
    if let Some(weight) = site_weight(column, settlement, half, width) {
        return ease(natural, plain(ground, settlement), weight);
    }
    let landmark = landmark_column(ground);
    if let Some(weight) = site_weight(column, landmark, THRONE_RISE, width) {
        return ease(natural, plain(ground, landmark), weight);
    }
    let outpost = outpost_column(ground);
    if let Some(weight) = site_weight(column, outpost, super::OUTPOST_HALF, width) {
        return ease(natural, plain(ground, outpost), weight);
    }
    // A city stands on level streets.
    let cell = column.div_euclid(CITY_CELL);
    for offset in [0, -1] {
        if let Some(city) = city_in_cell(ground, cell + offset) {
            let half = city.width / 2;
            let centre = city.first + half;
            if let Some(weight) = site_weight(column, centre, half, width) {
                return ease(natural, plain(ground, centre), weight);
            }
        }
    }
    natural
}

/// A tower of a city: its first column, width and storeys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tower {
    pub first: i64,
    pub width: i64,
    pub storeys: i32,
    /// Its roof is broken open from this column on (a ruin), if it is.
    pub broken_from: Option<i64>,
}

/// The towers of `city`, west to east.
#[must_use]
pub fn towers(ground: &PlanetGround, city: &City) -> Vec<Tower> {
    let mut out = Vec::new();
    let mut x = city.first + 2;
    let mut index = 0;
    while x < city.first + city.width - 6 {
        let hash = oroll(ground, 82, city.first, index);
        let width = 7 + pick(hash, 8, 7);
        let width = width.min(city.first + city.width - x);
        if width < 6 {
            break;
        }
        let storeys = 1 + i32::try_from(pick(hash, 16, 4)).unwrap_or(0);
        out.push(Tower {
            first: x,
            width,
            storeys,
            broken_from: (pick(hash, 24, 3) == 0).then(|| x + 2 + pick(hash, 32, 3)),
        });
        x += width + 3 + pick(hash, 40, 4);
        index += 1;
    }
    out
}

/// A tower's tile at `(dx, row)`: `dx` columns from its first column, `street` its ground row.
fn tower_tile(
    ground: &PlanetGround,
    tower: &Tower,
    dx: i64,
    row: i32,
    street: i32,
) -> Option<Tile> {
    if !(0..tower.width).contains(&dx) || row >= street {
        return None;
    }
    let height = tower.storeys * STOREY;
    let up = street - row;
    if up > height {
        return None;
    }
    let roof = up == height;
    if roof
        && tower
            .broken_from
            .is_some_and(|from| tower.first + dx >= from)
    {
        return Some(air());
    }
    let wall = dx == 0 || dx == tower.width - 1;
    let storey_floor = up % STOREY == 0;
    if wall {
        // A door on the ground floor's west wall; windows on every storey.
        if dx == 0 && (1..=3).contains(&up) {
            return Some(air());
        }
        if (up % STOREY == 3 || up % STOREY == 4) && up > STOREY / 2 {
            return Some(outland(OutlandTile::PetrifiedGlass));
        }
        return Some(outland(OutlandTile::Petrified));
    }
    if roof || storey_floor {
        // Each floor has a hole at alternating ends to fly up through.
        let storey = up / STOREY;
        let hole_west = storey % 2 == 1;
        let hole = if hole_west {
            (1..=2).contains(&dx)
        } else {
            (tower.width - 3..=tower.width - 2).contains(&dx)
        };
        if hole && !roof {
            return Some(air());
        }
        return Some(outland(OutlandTile::Petrified));
    }
    // An urn on some floors, standing on the storey's floor.
    if up % STOREY == 1
        && dx == tower.width / 2
        && oroll(ground, 83, tower.first, i64::from(up)).is_multiple_of(3)
    {
        return Some(outland(OutlandTile::Urn));
    }
    Some(air())
}

/// The Hollow Throne's tile at `(column, row)`, if any.
fn throne_tile(ground: &PlanetGround, column: i64, row: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let dx = wrap_delta(column, landmark_column(ground), width);
    if dx.abs() > THRONE_HALF {
        return None;
    }
    let floor = super::landmark_floor(ground);
    let up = floor - row;
    let top = THRONE_STEP * THRONE_STEPS;
    if up <= 0 || up > top + 9 {
        return None;
    }
    // The stair-shaft down through its heart (opening on the crown).
    if dx.abs() <= super::LAIR_SHAFT_HALF && up <= top {
        return Some(air());
    }
    if up > top {
        // The throne on the crown: a seat and a high back, between two soulstone braziers.
        let back = dx.abs() <= 1 && up <= top + 8;
        let seat = dx.abs() <= 3 && up == top + 2;
        let arms = dx.abs() == 3 && up <= top + 3;
        if back || seat || arms {
            return Some(outland(OutlandTile::Petrified));
        }
        if dx.abs() == 8 && up <= top + 2 {
            return Some(outland(if up == top + 2 {
                OutlandTile::Soulstone
            } else {
                OutlandTile::Petrified
            }));
        }
        return None;
    }
    // The steps: each narrower by six columns.
    let step = (up - 1) / THRONE_STEP;
    let half = THRONE_HALF - i64::from(step) * 6;
    if dx.abs() > half {
        return None;
    }
    // A carved frieze of glass along each step's face.
    if dx.abs() == half && (up - 1) % THRONE_STEP == 2 {
        return Some(outland(OutlandTile::PetrifiedGlass));
    }
    Some(outland(OutlandTile::Petrified))
}

/// A sealed tomb in the Necropolis: its outer box in ring tiles (it keeps its shape where a
/// ring tile spans several strip columns).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Tomb {
    /// The box's first strip column (a ring tile's first) and top row.
    pub column: i64,
    pub row: i32,
    /// Strip columns one of its ring tiles spans.
    pub span: i64,
    /// Its seal is on its east wall (else its west).
    pub seal_east: bool,
}

impl Tomb {
    /// The seal's tiles (two, on the floor storey of its door wall).
    #[must_use]
    pub fn seal(&self, width: i64) -> [(i32, i32); 2] {
        let x = if self.seal_east {
            self.column + (TOMB_BOX.0 - 1) * self.span
        } else {
            self.column
        };
        let x = i32::try_from(x.rem_euclid(width)).unwrap_or(0);
        let floor = self.row + TOMB_BOX.1 - 2;
        [(x, floor), (x, floor - 1)]
    }

    /// The tomb's inside middle (where its wardens wake).
    #[must_use]
    pub const fn middle(&self) -> (i64, i32) {
        (
            self.column + TOMB_BOX.0 / 2 * self.span,
            self.row + TOMB_BOX.1 / 2,
        )
    }
}

/// The tomb of the cell holding strip tile `(column, row)`, if it has one there.
#[must_use]
pub fn tomb_at(ground: &PlanetGround, column: i64, row: i32) -> Option<Tomb> {
    let width = i64::from(ground.strip_width());
    let cells = (width / TOMB_CELL.0).max(1);
    let cx = column.div_euclid(TOMB_CELL.0);
    let cy = row.div_euclid(TOMB_CELL.1);
    if cx >= cells && width >= TOMB_CELL.0 {
        return None;
    }
    let hash = oroll(ground, 90, cx.rem_euclid(cells), i64::from(cy));
    if hash % 20 >= 9 {
        return None;
    }
    let row0 = cy * TOMB_CELL.1 + 6 + i32::try_from(pick(hash, 16, 20)).unwrap_or(0);
    // One ring band from its roof to its floor.
    let span = i64::from(ground.grid().span_at(row0));
    if span != i64::from(ground.grid().span_at(row0 + TOMB_BOX.1)) {
        return None;
    }
    let room = (TOMB_CELL.0 - 9 - TOMB_BOX.0 * span).max(1);
    let raw = cx * TOMB_CELL.0 + 8 + pick(hash, 8, room.cast_unsigned());
    let tomb = Tomb {
        column: raw - raw.rem_euclid(span),
        row: row0,
        span,
        seal_east: (hash >> 30) & 1 == 0,
    };
    if tomb.column + TOMB_BOX.0 * span >= width
        || kept_clear(ground, tomb.column, 40)
        || kept_clear(ground, tomb.column + TOMB_BOX.0 * span, 40)
    {
        return None;
    }
    let dx = wrap_delta(column, tomb.column, width).div_euclid(span);
    ((0..TOMB_BOX.0).contains(&dx) && (tomb.row..tomb.row + TOMB_BOX.1).contains(&row))
        .then_some(tomb)
}

/// A tomb's tile at `(dx, dy)` from its box's corner.
const fn tomb_tile(tomb: &Tomb, dx: i64, dy: i32) -> Tile {
    let (box_w, box_h) = TOMB_BOX;
    let door_x = if tomb.seal_east { box_w - 1 } else { 0 };
    if dx == door_x && (dy == box_h - 2 || dy == box_h - 3) {
        return outland(OutlandTile::TombSeal);
    }
    if dx == 0 || dx == box_w - 1 || dy == 0 || dy == box_h - 1 {
        return outland(OutlandTile::TombBrick);
    }
    if dy == box_h - 2 {
        if dx == box_w / 2 {
            return outland(OutlandTile::Sarcophagus);
        }
        if dx == 2 || dx == box_w - 3 {
            return outland(OutlandTile::Urn);
        }
    }
    air()
}

impl ThemeWorld for DeadWorld {
    fn theme(&self) -> PlanetThemeId {
        PlanetThemeId::DEAD
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
        let cell = i64::from(origin.x).div_euclid(CITY_CELL);
        let mut cities: Vec<(City, i32, Vec<Tower>)> = Vec::new();
        for offset in -1..=1 {
            if let Some(city) = city_in_cell(ground, cell + offset) {
                let street = surface(ground, city.first + city.width / 2);
                let towers = towers(ground, &city);
                cities.push((city, street, towers));
            }
        }
        let near_landmark = wrap_delta(i64::from(origin.x) + 16, landmark_column(ground), width)
            .abs()
            <= THRONE_HALF + super::LAIR_HALF_WIDTH + 30;
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
                    if let Some(tile) = throne_tile(ground, cell.first, cell.row) {
                        return tile;
                    }
                    if let Some(lair) = &lair
                        && let Some(tile) =
                            lair_tile(ground, lair, cell, outland(OutlandTile::TombBrick))
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
                    outland(OutlandTile::GraveRock),
                    outland(OutlandTile::Soulstone),
                ) {
                    return tile;
                }
                if cell.span == 1 && cell.row < cell.surface {
                    for (city, street, towers) in &cities {
                        for tower in towers {
                            let dx = wrap_delta(cell.first, tower.first, width);
                            if let Some(tile) = tower_tile(ground, tower, dx, cell.row, *street) {
                                return tile;
                            }
                        }
                        let _ = city;
                    }
                }
                if cell.row < cell.surface {
                    return cell.base;
                }
                if cell.stratum == 1
                    && let Some(tomb) = tomb_at(ground, cell.first, cell.row)
                {
                    let dx = wrap_delta(cell.first, tomb.column, width).div_euclid(tomb.span);
                    return tomb_tile(&tomb, dx, cell.row - tomb.row);
                }
                cell.base
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        Some(settlement_site_of(ground, |column| surface(ground, column)))
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<SurfaceSite> {
        let centre = landmark_column(ground);
        Some(SurfaceSite {
            first: i32::try_from(centre - THRONE_HALF).unwrap_or(0),
            width: i32::try_from(THRONE_HALF * 2 + 1).unwrap_or(65),
            floor: surface(ground, centre),
        })
    }
}
