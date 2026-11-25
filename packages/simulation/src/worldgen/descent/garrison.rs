//! Descent VII part B's generated places (Wave 10 ancients-b; the Sleeping Garrison, the Seal
//! Gate and the Gatekeeper's hall, [`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)
//! only).
//!
//! Pure functions of the settings and the tile (so chunks carve the same in any order), run
//! by `streamed::assemble` after the far Strange Layers' pass and before the expedition's story
//! pass:
//!
//! * **Forts** ([`forts_with`], [`fort_tile`]): at most one per [`FORT_REGION`] cell of the
//!   Sleeping Garrison, commoner deeper, plus the **Grand Bastion** ([`grand_bastion`]). A fort
//!   is a war-plate hall with gates in both side walls, war bays in its ceiling and walls (the
//!   machines climb out of them), ledges, and a **hold pylon** on its floor.
//! * **The Seal Court** ([`seal_court`]): a gilded court before the Seal Gate's door with six
//!   key altars, and the **Processional Way** ([`processional_way`]), a long tunnel at the
//!   court's floor that a rig digging down near it will hit.
//! * **The Gatekeeper's hall** ([`colossus_hall`]): seal stone that nothing digs, the **Seal
//!   Gate**'s door in its court-side wall ([`seal_door`]), an upper hall over a cracked floor
//!   ([`cracked_floor`]) and the crypt under it ([`hall_crypt`]), a cracked vault under its
//!   roof ([`cracked_vault`]). The collapses rewrite these tiles in play
//!   ([`collapse_writes`]).
//! * **The Heart Seal** ([`heart_seal`]): three ragged rows across every column under the
//!   Seal Gate's boundary zone that only a rig carrying the Heart Key drills.
//! * **Rock** ([`garrison_rock`]): war-plate scrap in the Sleeping Garrison, seal gold veins
//!   in the Seal Gate.

use crate::{
    cave::CaveTile,
    descent::{Descent, Stratum},
    garrison::{ColossusStage, FortKind},
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, ridge, value_noise},
    },
};

use super::{DescentLayout, landmarks, stratum_in};

/// The cells forts are rolled per, `(width, height)` in tiles.
pub const FORT_REGION: (i32, i32) = (120, 80);
/// `(width, height)` of a barracks fort (walls included).
pub const BARRACKS_SIZE: (i32, i32) = (48, 16);
/// `(width, height)` of the Grand Bastion.
pub const BASTION_SIZE: (i32, i32) = (70, 20);
/// `(width, height)` of the Gatekeeper's hall (seal stone shell included).
pub const HALL_SIZE: (i32, i32) = (88, 34);
/// Columns of the hall's crypt (the cracked floor's span), centred.
pub const CRYPT_WIDTH: i32 = 40;
/// `(width, height)` of the Seal Court before the door.
pub const COURT_SIZE: (i32, i32) = (32, 16);
/// How far the Processional Way runs out from the court, columns.
pub const WAY_LENGTH: i32 = 46;
/// Rows of the Heart Seal.
pub const HEART_SEAL_ROWS: i32 = 3;
/// Rows of the Seal Gate's door.
pub const DOOR_ROWS: i32 = 8;

const SALT_REGION: u64 = 0xA4C_0001;
const SALT_FIXED: u64 = 0xA4C_0002;
const SALT_SCRAP: u64 = 0xA4C_0003;
const SALT_GOLD: u64 = 0xA4C_0004;

const fn rect(x: i32, y: i32, (width, height): (i32, i32)) -> TileRect {
    TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + width - 1,
            y: y + height - 1,
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "small bounded rolls"
)]
const fn roll_range(roll: u64, shift: u32, low: i32, high: i32) -> i32 {
    if high <= low {
        return low;
    }
    low + ((roll >> shift) % ((high - low + 1) as u64)) as i32
}

/// Whether `stratum` is one of this pass's.
#[must_use]
pub const fn is_garrison(stratum: Stratum) -> bool {
    matches!(stratum, Stratum::SleepingGarrison | Stratum::TheSealGate)
}

/// The rows the pass touches (with slack for the ragged boundaries and the Heart Seal).
#[must_use]
pub const fn carve_rows(layout: &DescentLayout) -> (i32, i32) {
    let top = layout.rows(Stratum::SleepingGarrison).0 - 40;
    let bottom = layout.rows(Stratum::TheSealGate).1 + 48;
    (top, bottom)
}

/// The side of the spawn column away from the outposts' lift (the hall and the Bastion sit
/// there, clear of the cable).
const fn away_side(settings: &WorldSettings) -> i32 {
    if landmarks::outpost_column(settings) >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    }
}

// --- Forts -----------------------------------------------------------------------------------

/// A fort.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fort {
    pub kind: FortKind,
    /// Its walls.
    pub bounds: TileRect,
    /// How deep in the Sleeping Garrison it is, per mille.
    pub along: u16,
}

impl Fort {
    /// The open inside (inside the walls, above the floor).
    #[must_use]
    pub const fn inside(&self) -> TileRect {
        fort_inside(&self.bounds)
    }

    /// The hold pylon's bottom-left tile (the fort's key).
    #[must_use]
    pub const fn pylon(&self) -> TilePosition {
        fort_pylon(&self.bounds)
    }

    /// Where the fort's machines climb out: the open tile under each ceiling bay and beside
    /// each wall bay.
    #[must_use]
    pub fn bay_spawns(&self) -> Vec<TilePosition> {
        let inside = self.inside();
        let mut spawns = Vec::new();
        for x in ceiling_bay_columns(&inside) {
            spawns.push(TilePosition {
                x,
                y: inside.min.y + 1,
            });
        }
        let row = wall_bay_row(&inside);
        spawns.push(TilePosition {
            x: inside.min.x + 1,
            y: row,
        });
        spawns.push(TilePosition {
            x: inside.max.x - 1,
            y: row,
        });
        spawns
    }
}

/// The open inside of a fort with walls `bounds`.
#[must_use]
pub const fn fort_inside(bounds: &TileRect) -> TileRect {
    TileRect {
        min: TilePosition {
            x: bounds.min.x + 2,
            y: bounds.min.y + 2,
        },
        max: TilePosition {
            x: bounds.max.x - 2,
            y: bounds.max.y - 2,
        },
    }
}

/// The hold pylon's bottom-left tile of a fort with walls `bounds` (two wide, three tall, on
/// the floor at the middle).
#[must_use]
pub const fn fort_pylon(bounds: &TileRect) -> TilePosition {
    let inside = fort_inside(bounds);
    TilePosition {
        x: i32::midpoint(inside.min.x, inside.max.x),
        y: inside.max.y,
    }
}

/// The columns of a fort's ceiling bays (every ninth column from the fourth).
fn ceiling_bay_columns(inside: &TileRect) -> impl Iterator<Item = i32> + use<> {
    let (first, last) = (inside.min.x + 4, inside.max.x - 4);
    (first..=last).step_by(9)
}

/// The row of a fort's wall bays (above the gates).
const fn wall_bay_row(inside: &TileRect) -> i32 {
    inside.max.y - 6
}

/// The Grand Bastion (one per Deep Descent world, about halfway down the Sleeping Garrison).
#[must_use]
pub const fn grand_bastion(settings: &WorldSettings, layout: &DescentLayout) -> Option<Fort> {
    if !settings.generator.has_descents() {
        return None;
    }
    let (top, bottom) = layout.rows(Stratum::SleepingGarrison);
    let roll = hash2(settings.seed, SALT_FIXED, 1, 0);
    let centre = crate::worldgen::SPAWN_TILE_X + away_side(settings) * roll_range(roll, 0, 70, 120);
    let middle = top + (bottom - top) * 11 / 20 + roll_range(roll, 12, -8, 8);
    Some(Fort {
        kind: FortKind::Bastion,
        bounds: rect(
            centre - BASTION_SIZE.0 / 2,
            middle - BASTION_SIZE.1 / 2,
            BASTION_SIZE,
        ),
        along: 550,
    })
}

/// The fort of region `(rx, ry)`, if it rolled one.
#[allow(clippy::cast_possible_wrap, reason = "per mille")]
fn region_fort(
    settings: &WorldSettings,
    layout: &DescentLayout,
    bastion: Option<&Fort>,
    rx: i32,
    ry: i32,
) -> Option<Fort> {
    let (width, height) = FORT_REGION;
    let centre_row = ry * height + height / 2;
    if layout.stratum_of_row(centre_row) != Stratum::SleepingGarrison {
        return None;
    }
    let along = layout.along(centre_row);
    let roll = hash2(settings.seed, SALT_REGION, rx, ry);
    // A third of the cells at the top, most of them at the bottom.
    let chance = 330 + u64::from(along) * 4 / 10;
    if roll % 1_000 >= chance {
        return None;
    }
    let (fw, fh) = BARRACKS_SIZE;
    let x = rx * width + roll_range(roll, 16, 4, width - fw - 4);
    let y = ry * height + roll_range(roll, 32, 4, height - fh - 4);
    let bounds = rect(x, y, BARRACKS_SIZE);
    // Never over the Bastion, and wholly inside the stratum's rows.
    let (top, bottom) = layout.rows(Stratum::SleepingGarrison);
    if bastion.is_some_and(|bastion| bastion.bounds.expanded(8).intersects(&bounds))
        || bounds.min.y < top + 24
        || bounds.max.y > bottom - 24
    {
        return None;
    }
    Some(Fort {
        kind: FortKind::Barracks,
        bounds,
        along,
    })
}

/// Every fort whose walls overlap `area` (the Bastion first, then by region). Empty for worlds
/// without Descents.
#[must_use]
pub fn forts_with(settings: &WorldSettings, layout: &DescentLayout, area: &TileRect) -> Vec<Fort> {
    if !settings.generator.has_descents() {
        return Vec::new();
    }
    let (first, last) = layout.rows(Stratum::SleepingGarrison);
    if area.max.y < first - 30 || area.min.y > last + 30 {
        return Vec::new();
    }
    let bastion = grand_bastion(settings, layout);
    let mut found: Vec<Fort> = bastion
        .iter()
        .copied()
        .filter(|fort| fort.bounds.intersects(area))
        .collect();
    let (width, height) = FORT_REGION;
    for ry in area.min.y.div_euclid(height)..=area.max.y.div_euclid(height) {
        for rx in area.min.x.div_euclid(width)..=area.max.x.div_euclid(width) {
            if let Some(fort) = region_fort(settings, layout, bastion.as_ref(), rx, ry)
                && fort.bounds.intersects(area)
            {
                found.push(fort);
            }
        }
    }
    found
}

/// The fort whose pylon is `pylon`, if any.
#[must_use]
pub fn fort_of_pylon(
    settings: &WorldSettings,
    layout: &DescentLayout,
    pylon: TilePosition,
) -> Option<Fort> {
    forts_with(settings, layout, &TileRect::around(pylon, 2, 2))
        .into_iter()
        .find(|fort| fort.pylon() == pylon)
}

/// The tile `fort` puts at `(x, y)`, or `None` outside it.
#[must_use]
pub fn fort_tile(fort: &Fort, x: i32, y: i32) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !fort.bounds.contains(at) {
        return None;
    }
    let inside = fort.inside();
    let plate = TileKind::Cave(CaveTile::WarPlate);
    if !inside.contains(at) {
        // Gates four rows tall at the floor in both side walls.
        let side = x < inside.min.x || x > inside.max.x;
        if side && y >= inside.max.y - 3 && y <= inside.max.y {
            return Some(TileKind::Air);
        }
        // Ceiling bays (the inner ceiling row), wall bays (the inner wall column).
        if y == inside.min.y - 1 && ceiling_bay_columns(&inside).any(|column| column == x) {
            return Some(TileKind::Cave(CaveTile::WarBay));
        }
        if y == wall_bay_row(&inside) && (x == inside.min.x - 1 || x == inside.max.x + 1) {
            return Some(TileKind::Cave(CaveTile::WarBay));
        }
        return Some(plate);
    }
    // The hold pylon: two wide, three tall on the floor at the middle.
    let pylon = fort.pylon();
    if (x == pylon.x || x == pylon.x + 1) && y <= pylon.y && y > pylon.y - 3 {
        return Some(TileKind::Cave(CaveTile::HoldPylon));
    }
    // Ledges: a war-plate shelf at a third and two thirds of the width, halfway up (footing
    // for turrets and rigs).
    let wide = inside.width();
    let ledge_row = inside.min.y + inside.height() / 2;
    for lx in [inside.min.x + wide / 4, inside.max.x - wide / 4] {
        if y == ledge_row && (x - lx).abs() <= 3 {
            return Some(plate);
        }
    }
    // The Bastion's racks: rows of sleeping machines' frames along its back wall are drawn by
    // the game; here a second, higher ledge in the middle.
    if fort.kind == FortKind::Bastion && y == inside.min.y + 4 && (x - pylon.x).abs() <= 5 {
        return Some(plate);
    }
    Some(TileKind::Air)
}

// --- The Seal Gate and the Gatekeeper's hall --------------------------------------------------

/// The Gatekeeper's hall (one per Deep Descent world, at the bottom of the Seal Gate, away
/// from the outposts' lift).
#[must_use]
pub const fn colossus_hall(settings: &WorldSettings, layout: &DescentLayout) -> Option<TileRect> {
    if !settings.generator.has_descents() {
        return None;
    }
    let stratum = Descent::Ancients.last_stratum();
    let end = layout.rows(stratum).1 + 1;
    let (wave, half) = super::boundary_shape(stratum.index());
    // Its bottom just above the boundary's transition zone, like the gate chamber.
    let bottom = end - wave - half - 2;
    let roll = hash2(settings.seed, SALT_FIXED, 2, 0);
    let centre = crate::worldgen::SPAWN_TILE_X + away_side(settings) * roll_range(roll, 0, 62, 78);
    Some(rect(
        centre - HALL_SIZE.0 / 2,
        bottom - HALL_SIZE.1 + 1,
        HALL_SIZE,
    ))
}

/// The side (-1 left, 1 right) of the hall its door and court are on: towards the spawn.
#[must_use]
pub const fn door_side(settings: &WorldSettings) -> i32 {
    -away_side(settings)
}

/// The hall's upper floor's top row (rigs stand on it in stage one).
#[must_use]
pub const fn upper_floor_row(hall: &TileRect) -> i32 {
    hall.min.y + 18
}

/// The hall's crypt floor's top row (stage two's floor).
#[must_use]
pub const fn crypt_floor_row(hall: &TileRect) -> i32 {
    hall.min.y + 30
}

/// The open inside of the hall at its widest (upper hall and crypt; the vault rows count once
/// they fall): what "in the hall" means.
#[must_use]
pub const fn hall_inside(hall: &TileRect) -> TileRect {
    TileRect {
        min: TilePosition {
            x: hall.min.x + 2,
            y: hall.min.y + 2,
        },
        max: TilePosition {
            x: hall.max.x - 2,
            y: crypt_floor_row(hall) - 1,
        },
    }
}

/// The upper hall's open air (stage one).
#[must_use]
pub const fn upper_hall(hall: &TileRect) -> TileRect {
    TileRect {
        min: TilePosition {
            x: hall.min.x + 2,
            y: hall.min.y + 4,
        },
        max: TilePosition {
            x: hall.max.x - 2,
            y: upper_floor_row(hall) - 1,
        },
    }
}

/// The cracked floor over the crypt (two rows; it falls in the first collapse).
#[must_use]
pub const fn cracked_floor(hall: &TileRect) -> TileRect {
    let centre = i32::midpoint(hall.min.x, hall.max.x);
    TileRect {
        min: TilePosition {
            x: centre - CRYPT_WIDTH / 2,
            y: upper_floor_row(hall),
        },
        max: TilePosition {
            x: centre + CRYPT_WIDTH / 2 - 1,
            y: upper_floor_row(hall) + 1,
        },
    }
}

/// The crypt under the cracked floor (open air from the start, sealed in until it falls).
#[must_use]
pub const fn hall_crypt(hall: &TileRect) -> TileRect {
    let floor = cracked_floor(hall);
    TileRect {
        min: TilePosition {
            x: floor.min.x,
            y: floor.max.y + 1,
        },
        max: TilePosition {
            x: floor.max.x,
            y: crypt_floor_row(hall) - 1,
        },
    }
}

/// The cracked vault under the hall's roof (two rows; it falls in the second collapse).
#[must_use]
pub const fn cracked_vault(hall: &TileRect) -> TileRect {
    TileRect {
        min: TilePosition {
            x: hall.min.x + 2,
            y: hall.min.y + 2,
        },
        max: TilePosition {
            x: hall.max.x - 2,
            y: hall.min.y + 3,
        },
    }
}

/// The Seal Gate's door: the hall's court-side wall at the upper floor, [`DOOR_ROWS`] tall.
#[must_use]
pub const fn seal_door(settings: &WorldSettings, hall: &TileRect) -> TileRect {
    let x = if door_side(settings) < 0 {
        hall.min.x
    } else {
        hall.max.x - 1
    };
    let floor = upper_floor_row(hall);
    TileRect {
        min: TilePosition {
            x,
            y: floor - DOOR_ROWS,
        },
        max: TilePosition {
            x: x + 1,
            y: floor - 1,
        },
    }
}

/// The Seal Court before the door.
#[must_use]
pub const fn seal_court(settings: &WorldSettings, hall: &TileRect) -> TileRect {
    let floor = upper_floor_row(hall);
    let x = if door_side(settings) < 0 {
        hall.min.x - COURT_SIZE.0
    } else {
        hall.max.x + 1
    };
    rect(x, floor - COURT_SIZE.1, COURT_SIZE)
}

/// The Processional Way: a tunnel at the court's floor running out from the court.
#[must_use]
pub const fn processional_way(settings: &WorldSettings, hall: &TileRect) -> TileRect {
    let court = seal_court(settings, hall);
    let floor = upper_floor_row(hall);
    let x = if door_side(settings) < 0 {
        court.min.x - WAY_LENGTH
    } else {
        court.max.x + 1
    };
    rect(x, floor - 6, (WAY_LENGTH, 6))
}

/// The six key altars' columns in the court (each two wide, three tall), in [`crate::garrison::SealKey`]
/// order from the door outwards.
#[must_use]
pub fn altar_columns(settings: &WorldSettings, hall: &TileRect) -> [i32; 6] {
    let court = seal_court(settings, hall);
    let mut columns = [0; 6];
    for (index, column) in (0_i32..).zip(columns.iter_mut()) {
        *column = if door_side(settings) < 0 {
            court.max.x - 4 - index * 4
        } else {
            court.min.x + 3 + index * 4
        };
    }
    columns
}

/// Where the Colossus stands in `stage`: its core's tile column and its feet's row (stage
/// three: the row it hovers at).
#[must_use]
pub const fn colossus_post(
    settings: &WorldSettings,
    hall: &TileRect,
    stage: ColossusStage,
) -> TilePosition {
    let centre = i32::midpoint(hall.min.x, hall.max.x);
    match stage {
        // At the far end from the door, on the upper floor.
        ColossusStage::Guard => TilePosition {
            x: if door_side(settings) < 0 {
                hall.max.x - 11
            } else {
                hall.min.x + 11
            },
            y: upper_floor_row(hall),
        },
        ColossusStage::Fallen => TilePosition {
            x: centre,
            y: crypt_floor_row(hall),
        },
        ColossusStage::Unbound => TilePosition {
            x: centre,
            y: hall.min.y + 12,
        },
    }
}

/// The tile the hall puts at `(x, y)` in its first shape (stage one), or `None` outside it.
#[must_use]
pub const fn hall_tile(
    settings: &WorldSettings,
    hall: &TileRect,
    x: i32,
    y: i32,
) -> Option<TileKind> {
    let at = TilePosition { x, y };
    if !hall.contains(at) {
        return None;
    }
    let stone = TileKind::Cave(CaveTile::SealStone);
    if seal_door(settings, hall).contains(at) {
        return Some(TileKind::Cave(CaveTile::SealDoor));
    }
    if cracked_vault(hall).contains(at) {
        return Some(TileKind::Cave(CaveTile::CrackedVault));
    }
    if upper_hall(hall).contains(at) {
        return Some(TileKind::Air);
    }
    if cracked_floor(hall).contains(at) {
        return Some(TileKind::Cave(CaveTile::CrackedFloor));
    }
    if hall_crypt(hall).contains(at) {
        return Some(TileKind::Air);
    }
    Some(stone)
}

/// The tile the court and the Processional Way put at `(x, y)`, or `None` outside them.
#[must_use]
pub fn court_tile(settings: &WorldSettings, hall: &TileRect, x: i32, y: i32) -> Option<TileKind> {
    let at = TilePosition { x, y };
    let court = seal_court(settings, hall);
    let way = processional_way(settings, hall);
    let floor = upper_floor_row(hall);
    let brick = TileKind::Cave(CaveTile::RuinBrick);
    if court.contains(at) {
        // A gilded shell one tile thick (drillable: rigs dig in from anywhere), open inside.
        let edge = x == court.min.x || x == court.max.x || y == court.min.y;
        // The court opens on the door's side and on the way's side.
        let door_edge = if door_side(settings) < 0 {
            x == court.max.x
        } else {
            x == court.min.x
        };
        let way_edge = !door_edge && (x == court.min.x || x == court.max.x);
        if way_edge && y >= floor - 5 {
            return Some(TileKind::Air);
        }
        if door_edge {
            return Some(TileKind::Air);
        }
        if edge {
            return Some(if (x + y).rem_euclid(5) == 0 {
                TileKind::Cave(CaveTile::SealGold)
            } else {
                brick
            });
        }
        // The six key altars: two wide, two tall, standing on the floor.
        if y >= floor - 3
            && altar_columns(settings, hall)
                .iter()
                .any(|column| x == *column || x == *column + 1)
        {
            return Some(TileKind::Cave(CaveTile::KeyAltar));
        }
        return Some(TileKind::Air);
    }
    if y == floor && x >= court.min.x && x <= court.max.x {
        // The court's floor: the seal stone the gate stands on.
        return Some(TileKind::Cave(CaveTile::SealStone));
    }
    if way.contains(at) {
        // Brick roof, open below; a pillar every twelve columns.
        if y == way.min.y {
            return Some(brick);
        }
        if (x - way.min.x).rem_euclid(12) == 0 && y < way.max.y - 2 {
            return Some(TileKind::Cave(CaveTile::RuinPillar));
        }
        return Some(TileKind::Air);
    }
    if y == floor && x >= way.min.x && x <= way.max.x {
        return Some(brick);
    }
    None
}

/// The tiles a collapse into `stage` writes over the hall: `(tile, new kind)`, in order.
///
/// The first collapse drops the cracked floor and piles rubble ramps at the crypt's ends; the
/// second drops the cracked vault and leaves three rubble platforms in the air. Solid writes
/// onto a body are skipped by the caller.
#[must_use]
pub fn collapse_writes(hall: &TileRect, stage: ColossusStage) -> Vec<(TilePosition, TileKind)> {
    let rubble = TileKind::Cave(CaveTile::CollapseRubble);
    let mut writes = Vec::new();
    match stage {
        ColossusStage::Guard => {}
        ColossusStage::Fallen => {
            let floor = cracked_floor(hall);
            for at in floor.positions() {
                writes.push((at, TileKind::Air));
            }
            // Ramps of rubble from the crypt floor up at both ends, eight tiles high at the
            // wall, stepping down one a column.
            let crypt = hall_crypt(hall);
            let bottom = crypt.max.y;
            for step in 0..8 {
                let height = 8 - step;
                for column in [crypt.min.x + step, crypt.max.x - step] {
                    for row in 0..height {
                        writes.push((
                            TilePosition {
                                x: column,
                                y: bottom - row,
                            },
                            rubble,
                        ));
                    }
                }
            }
        }
        ColossusStage::Unbound => {
            for at in cracked_vault(hall).positions() {
                writes.push((at, TileKind::Air));
            }
            let centre = i32::midpoint(hall.min.x, hall.max.x);
            let platforms = [
                (centre - 15, hall.min.y + 12),
                (centre, hall.min.y + 8),
                (centre + 15, hall.min.y + 12),
            ];
            for (px, py) in platforms {
                for dx in -3..=2 {
                    writes.push((TilePosition { x: px + dx, y: py }, rubble));
                }
            }
        }
    }
    writes
}

/// The hall's floor that opens on the way down once the Colossus falls (the crypt floor's
/// middle, every row of the shell under it).
#[must_use]
pub const fn victory_shaft(hall: &TileRect) -> TileRect {
    let centre = i32::midpoint(hall.min.x, hall.max.x);
    TileRect {
        min: TilePosition {
            x: centre - 2,
            y: crypt_floor_row(hall),
        },
        max: TilePosition {
            x: centre + 1,
            y: hall.max.y,
        },
    }
}

// --- The Heart Seal --------------------------------------------------------------------------

/// Whether `(x, y)` is in the Heart Seal: three ragged rows just under the Seal Gate's
/// boundary zone, across every column (Descent VIII's door: the Heart Key).
#[must_use]
pub const fn heart_seal(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let index = Stratum::TheSealGate.index();
    let (_, half) = super::boundary_shape(index);
    let top = super::boundary_column_depth(settings, layout, index, x) + half;
    y >= top && y < top + HEART_SEAL_ROWS
}

/// Whether `area` (grown by two tiles) touches a row of the Heart Seal in any of its columns.
#[must_use]
pub fn crosses_heart_seal(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    let index = Stratum::TheSealGate.index();
    let (_, half) = super::boundary_shape(index);
    let grown = area.expanded(2);
    (grown.min.x..=grown.max.x).any(|x| {
        let top = super::boundary_column_depth(settings, layout, index, x) + half;
        top <= grown.max.y && top + HEART_SEAL_ROWS > grown.min.y
    })
}

/// Whether `area` crosses one of this pass's set pieces (a fort, the hall, the court or the
/// way), with a margin (later passes keep their rooms off them).
#[must_use]
pub fn crosses_set_pieces(
    settings: &WorldSettings,
    layout: &DescentLayout,
    area: &TileRect,
) -> bool {
    if !settings.generator.has_descents() {
        return false;
    }
    let grown = area.expanded(3);
    if forts_with(settings, layout, &grown)
        .iter()
        .any(|fort| fort.bounds.expanded(3).intersects(area))
    {
        return true;
    }
    colossus_hall(settings, layout).is_some_and(|hall| {
        hall.expanded(3).intersects(area)
            || seal_court(settings, &hall).expanded(3).intersects(area)
            || processional_way(settings, &hall)
                .expanded(3)
                .intersects(area)
    }) || crosses_heart_seal(settings, layout, area)
}

// --- Rock ------------------------------------------------------------------------------------

/// Whether `kind` is plain rock the rock pass may dress.
const fn plain_rock(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}

/// Whether a set piece may carve over `kind`.
const fn carvable(kind: TileKind) -> bool {
    !matches!(
        kind,
        TileKind::Built(_) | TileKind::Poi(_) | TileKind::Bedrock | TileKind::Fluid(_)
    )
}

/// The rock pass: war-plate scrap in the Sleeping Garrison, seal gold veins in the Seal Gate.
#[must_use]
pub fn garrison_rock(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    if !plain_rock(base) {
        return None;
    }
    let along = i32::from(layout.along(y));
    let seed = settings.seed;
    match stratum {
        Stratum::SleepingGarrison => hash2(seed, SALT_SCRAP, x, y)
            .is_multiple_of(71)
            .then_some(TileKind::Cave(CaveTile::WarPlate)),
        Stratum::TheSealGate => {
            let line = ridge(value_noise(seed, SALT_GOLD, x, y, 14, 10));
            (line > NOISE_MAX - 900 - along).then_some(TileKind::Cave(CaveTile::SealGold))
        }
        _ => None,
    }
}

/// What a chunk of this pass needs: the forts touching it and the hall.
#[derive(Clone, Debug, Default)]
pub struct ChunkPlan {
    pub forts: Vec<Fort>,
    pub hall: Option<TileRect>,
}

/// The plan for the chunk covering `area`.
#[must_use]
pub fn plan_for(settings: &WorldSettings, layout: &DescentLayout, area: &TileRect) -> ChunkPlan {
    ChunkPlan {
        forts: forts_with(settings, layout, area),
        hall: colossus_hall(settings, layout),
    }
}

/// This pass's tile at `(x, y)` over `base`: the Heart Seal, a set piece's tile, else the rock
/// pass, else `None` (leave `base`).
#[must_use]
pub fn garrison_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    plan: &ChunkPlan,
    x: i32,
    y: i32,
    base: TileKind,
) -> Option<TileKind> {
    // The seal first and over everything but bedrock: anything left in its rows would be a way
    // into Descent VIII without the Heart Key.
    if base != TileKind::Bedrock && heart_seal(settings, layout, x, y) {
        return Some(TileKind::Cave(CaveTile::HeartSeal));
    }
    if let Some(hall) = &plan.hall {
        // The hall clears everything but bedrock (nothing may wall its fight in or open its
        // shell); the court and way carve like any set piece.
        if base != TileKind::Bedrock
            && let Some(kind) = hall_tile(settings, hall, x, y)
        {
            return Some(kind);
        }
        if carvable(base)
            && let Some(kind) = court_tile(settings, hall, x, y)
        {
            return Some(kind);
        }
    }
    if carvable(base) {
        for fort in &plan.forts {
            if let Some(kind) = fort_tile(fort, x, y) {
                return Some(kind);
            }
        }
    }
    let stratum = stratum_in(settings, layout, x, y);
    if !is_garrison(stratum) {
        return None;
    }
    garrison_rock(settings, layout, stratum, x, y, base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::{GeneratorVersion, WorldSettings};

    fn settings(seed: u64) -> WorldSettings {
        WorldSettings {
            seed,
            generator: GeneratorVersion::V11,
            ..WorldSettings::default()
        }
    }

    #[test]
    fn the_hall_sits_above_the_heart_seal_and_the_court_beside_its_door() {
        for seed in [1_u64, 7, 99, 2024] {
            let settings = settings(seed);
            let layout = crate::descent::fields::cached_layout(&settings);
            let hall = colossus_hall(&settings, &layout).expect("a hall");
            for x in hall.min.x..=hall.max.x {
                assert!(
                    !heart_seal(&settings, &layout, x, hall.max.y),
                    "seed {seed}: the seal under the hall's floor"
                );
            }
            assert!(!crosses_heart_seal(&settings, &layout, &hall));
            let door = seal_door(&settings, &hall);
            let court = seal_court(&settings, &hall);
            assert_eq!(door.height(), DOOR_ROWS);
            assert!(court.expanded(1).intersects(&door));
            // The lift's rail keeps clear of the hall and its court.
            let lift = landmarks::lift_column(&settings);
            assert!(
                lift < hall.min.x - 4 || lift > hall.max.x + 4,
                "seed {seed}"
            );
            let way = processional_way(&settings, &hall);
            assert!(lift < way.min.x - 2 || lift > way.max.x + 2, "seed {seed}");
            assert!(
                stratum_in(&settings, &layout, hall.center().x, hall.min.y) == Stratum::TheSealGate
            );
        }
    }

    #[test]
    fn forts_roll_in_the_garrison_with_a_pylon_bays_and_gates() {
        let settings = settings(5);
        let layout = crate::descent::fields::cached_layout(&settings);
        let (top, bottom) = layout.rows(Stratum::SleepingGarrison);
        let area = TileRect {
            min: TilePosition { x: -600, y: top },
            max: TilePosition { x: 800, y: bottom },
        };
        let forts = forts_with(&settings, &layout, &area);
        assert!(forts.len() >= 4, "{} forts", forts.len());
        assert!(forts.iter().any(|fort| fort.kind == FortKind::Bastion));
        for fort in &forts {
            let pylon = fort.pylon();
            assert_eq!(
                fort_tile(fort, pylon.x, pylon.y),
                Some(TileKind::Cave(CaveTile::HoldPylon))
            );
            assert_eq!(fort_of_pylon(&settings, &layout, pylon), Some(*fort));
            assert!(fort.bay_spawns().len() >= 4);
            for spawn in fort.bay_spawns() {
                assert_eq!(fort_tile(fort, spawn.x, spawn.y), Some(TileKind::Air));
            }
            let inside = fort.inside();
            assert_eq!(
                fort_tile(fort, fort.bounds.min.x, inside.max.y),
                Some(TileKind::Air),
                "a gate"
            );
        }
    }

    #[test]
    fn the_collapses_open_the_crypt_then_the_vault() {
        let settings = settings(3);
        let layout = crate::descent::fields::cached_layout(&settings);
        let hall = colossus_hall(&settings, &layout).expect("a hall");
        let first = collapse_writes(&hall, ColossusStage::Fallen);
        let floor = cracked_floor(&hall);
        assert!(
            floor
                .positions()
                .all(|at| first.contains(&(at, TileKind::Air)))
        );
        assert!(
            first
                .iter()
                .any(|(_, kind)| *kind == TileKind::Cave(CaveTile::CollapseRubble))
        );
        let second = collapse_writes(&hall, ColossusStage::Unbound);
        assert!(
            cracked_vault(&hall)
                .positions()
                .all(|at| second.contains(&(at, TileKind::Air)))
        );
        // The posts are open air in their stage's hall.
        for stage in ColossusStage::ALL {
            let post = colossus_post(&settings, &hall, stage);
            let above = TilePosition {
                x: post.x,
                y: post.y - 1,
            };
            assert!(hall_inside(&hall).contains(above), "{stage:?}");
        }
    }
}
