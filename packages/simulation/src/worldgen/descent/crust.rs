//! The authored Crust of a Deep Descent world (Wave 8).
//!
//! The generator behind the Crust's signature rules ([`crate::crust`]). Every function is a
//! pure function of the settings, the layout and the tile, so chunks carve identically in any
//! order.
//!
//! * **Topsoil:** root mats under the surface, lenses of rain-soaked mud, burrow tunnels
//!   (thicker in the "Burrow Warren" variant, muddier in "Rain Mud") and the lost miner's
//!   cabin ([`crate::crust::cabin`]), whose old entrance shaft is a column of rubble.
//! * **Clay Beds:** aquifers (the registry's sealed water rules) and clay pots
//!   (`Artifact(ClayPot)`), more of them in the "Potters' Beds" variant.
//! * **Grey Stone:** the old mine works ([`mine_works`]): a timbered gallery on a rail line,
//!   a stope with a strongbox, and a ramp down through a junction to a lower gallery.
//! * **Slate Shelves:** slate shelves in long thin bands ([`slate_shelf`]), and stalactites
//!   hung from cave ceilings (the galleries).
//! * **Basalt Columns:** basalt split into columns ([`basalt_column`]) and long lava tubes
//!   ([`crate::crust::lava_tube_cell`]) lined with tube glass, with rare lava pools.
//! * **The Sinkhole:** the gate chamber at the end of Descent I holds the core, and a funnel
//!   rises from it to the lip ([`sinkhole_tile`]).

use crate::{
    crust::{self, CrustTile, SinkholeSite},
    descent::Stratum,
    terrain::{SurfaceMaterial, TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, hash2, line_noise, ridge, value_noise},
    },
};

use super::{DescentLayout, variant};

const SALT_ROOTS: u64 = 0xC705_1001;
const SALT_MUD: u64 = 0xC705_1002;
const SALT_BURROW: u64 = 0xC705_1003;
const SALT_POT: u64 = 0xC705_1004;
const SALT_WORKS: u64 = 0xC705_1005;
const SALT_SHELF: u64 = 0xC705_1006;
const SALT_COLUMN: u64 = 0xC705_1007;
const SALT_STALACTITE: u64 = 0xC705_1008;
const SALT_LAVA: u64 = 0xC705_1009;

/// The size of a mine works region (columns, rows): each holds one works or none.
pub const WORKS_REGION: (i32, i32) = (128, 44);

/// One old mine works: a timbered gallery on a rail line, a stope along it, and a ramp down
/// through a junction to a lower gallery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MineWorks {
    /// The main line's rail row (the rig rides on top of it, in the row above).
    pub rail_row: i32,
    /// First and last column of the main line.
    pub first: i32,
    pub last: i32,
    /// The junction column: the ramp leaves the main line here, going east.
    pub junction: i32,
    /// Rows the ramp falls (one per column).
    pub drop: i32,
    /// Last column of the lower gallery.
    pub lower_last: i32,
    /// First and last column of the stope (a taller room on the main line).
    pub stope: (i32, i32),
    /// Whether the stope holds a strongbox.
    pub strongbox: bool,
}

/// Air rows of a gallery over its rail.
pub const GALLERY_AIR: i32 = 3;
/// Air rows of a stope over its rail.
pub const STOPE_AIR: i32 = 6;

/// The mine works of region `(rx, ry)`, if it has one (`top..=bottom`: the Grey Stone's
/// rows).
#[must_use]
pub const fn mine_works(seed: u64, top: i32, bottom: i32, rx: i32, ry: i32) -> Option<MineWorks> {
    let (width, height) = WORKS_REGION;
    let roll = hash2(seed, SALT_WORKS, rx, ry);
    // Three regions in five hold a works.
    if roll % 5 >= 3 {
        return None;
    }
    let rail_row = ry * height + 12 + ((roll >> 8) % 12) as i32;
    // Clear of the stratum's ragged edges (and room for the lower gallery).
    if rail_row < top + 22 || rail_row + 16 > bottom - 22 {
        return None;
    }
    let first = rx * width + 4 + ((roll >> 16) % 10) as i32;
    let length = 72 + ((roll >> 24) % 40) as i32;
    let last = first + length;
    let junction = first + 22 + ((roll >> 32) % 20) as i32;
    let drop = 6 + ((roll >> 40) % 6) as i32;
    let lower_end = junction + drop + 18 + ((roll >> 44) % 24) as i32;
    let region_end = rx * width + width - 4;
    let lower_last = if lower_end < region_end {
        lower_end
    } else {
        region_end
    };
    let stope_first = first + 6 + ((roll >> 50) % 8) as i32;
    Some(MineWorks {
        rail_row,
        first,
        last,
        junction,
        drop,
        lower_last,
        stope: (stope_first, stope_first + 9),
        strongbox: !(roll >> 58).is_multiple_of(3),
    })
}

impl MineWorks {
    /// Every tile the works may write.
    #[must_use]
    pub const fn bounds(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.first,
                y: self.rail_row - STOPE_AIR - 1,
            },
            max: TilePosition {
                x: if self.lower_last > self.last {
                    self.lower_last
                } else {
                    self.last
                },
                y: self.rail_row + self.drop + 1,
            },
        }
    }

    /// The tile the works writes at `(x, y)`, if any.
    #[must_use]
    pub const fn tile(&self, x: i32, y: i32) -> Option<TileKind> {
        // The ramp and the lower gallery (east of the junction).
        if x > self.junction && x <= self.lower_last {
            let step = x - self.junction;
            let rail = if step <= self.drop {
                self.rail_row + step
            } else {
                self.rail_row + self.drop
            };
            if y == rail {
                return Some(TileKind::Crust(CrustTile::Rail));
            }
            // The main line's own rail and air win where the two overlap.
            let on_main =
                x <= self.last && y >= self.rail_row - GALLERY_AIR - 1 && y <= self.rail_row;
            if !on_main {
                if y < rail && y >= rail - GALLERY_AIR {
                    return Some(TileKind::Air);
                }
                if y == rail - GALLERY_AIR - 1 {
                    return Some(TileKind::Crust(CrustTile::Timber));
                }
            }
        }
        if x < self.first || x > self.last {
            return None;
        }
        let in_stope = x >= self.stope.0 && x <= self.stope.1;
        let air = if in_stope { STOPE_AIR } else { GALLERY_AIR };
        if y == self.rail_row {
            return Some(TileKind::Crust(CrustTile::Rail));
        }
        if y < self.rail_row && y >= self.rail_row - air {
            if in_stope && self.strongbox && y == self.rail_row - 1 && x == self.stope.1 - 1 {
                return Some(TileKind::Crust(CrustTile::Strongbox));
            }
            return Some(TileKind::Air);
        }
        if y == self.rail_row - air - 1 {
            return Some(TileKind::Crust(CrustTile::Timber));
        }
        None
    }
}

/// The mine works whose tiles may touch `rect`.
#[must_use]
pub fn works_touching(
    settings: &WorldSettings,
    layout: &DescentLayout,
    rect: &TileRect,
) -> Vec<MineWorks> {
    let (top, bottom) = layout.rows(Stratum::GreyStone);
    if rect.max.y < top || rect.min.y > bottom {
        return Vec::new();
    }
    let (width, height) = WORKS_REGION;
    let mut found = Vec::new();
    for ry in (rect.min.y - height).div_euclid(height)..=(rect.max.y + height).div_euclid(height) {
        for rx in (rect.min.x - width).div_euclid(width)..=rect.max.x.div_euclid(width) {
            if let Some(works) = mine_works(settings.seed, top, bottom, rx, ry)
                && works.bounds().intersects(rect)
            {
                found.push(works);
            }
        }
    }
    found
}

/// Whether plain rock at `(x, y)` of the Slate Shelves (`top..=bottom`) is a slate shelf: long
/// bands two rows thick, nine rows apart, gently tilted.
#[must_use]
pub const fn slate_shelf(seed: u64, top: i32, x: i32, y: i32, sliding: bool) -> bool {
    let tilt = (line_noise(seed, SALT_SHELF, x, 61) - NOISE_MAX / 2) * 6 / NOISE_MAX;
    let row = y - top + tilt;
    let spacing = if sliding { 7 } else { 9 };
    if row.rem_euclid(spacing) >= 2 {
        return false;
    }
    let band = row.div_euclid(spacing);
    value_noise(seed, SALT_SHELF ^ 0xFF, x, band * 13, 23, 1) > NOISE_MAX * 9 / 20
}

/// Whether plain basalt at `(x, y)` splits into a column (two columns in three).
#[must_use]
pub const fn basalt_column(seed: u64, x: i32, y: i32) -> bool {
    let column = x.div_euclid(3);
    let segment = (y + (hash2(seed, SALT_COLUMN, column, 0) % 7) as i32).div_euclid(9);
    !hash2(seed, SALT_COLUMN, column, segment).is_multiple_of(3)
}

/// What the Crust's base pass writes over plain rock (or carves) at `(x, y)` of `stratum`, given
/// the tile so far (`current`, plain rock or open). `None` leaves the tile.
///
/// `surface` is the column's surface row; `open_above` / `solid_above` read the tile above.
#[must_use]
#[allow(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    reason = "a per-tile rule with the neighbour facts the chunk pass already knows"
)]
pub fn base_tile(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    x: i32,
    y: i32,
    surface: i32,
    plain: bool,
    open: bool,
    open_above: bool,
    solid_above: bool,
) -> Option<TileKind> {
    let seed = settings.seed;
    let (top, bottom) = layout.rows(stratum);
    match stratum {
        Stratum::Topsoil => {
            let depth = y - surface;
            let variant = variant(settings, stratum);
            if plain {
                if open_above && !hash2(seed, SALT_MUD, x, y).is_multiple_of(3) {
                    // Mud pools on cave floors.
                    return Some(TileKind::Surface(SurfaceMaterial::Mud));
                }
                let mud_share = if variant == 1 { 640 } else { 700 };
                if (3..=36).contains(&depth)
                    && value_noise(seed, SALT_MUD, x, y, 9, 4) > NOISE_MAX * mud_share / 1_000
                {
                    return Some(TileKind::Surface(SurfaceMaterial::Mud));
                }
                let root_depth = if variant == 0 { 14 } else { 9 };
                if (1..=root_depth).contains(&depth)
                    && value_noise(seed, SALT_ROOTS, x, y, 6, 3) > NOISE_MAX * 11 / 20
                {
                    return Some(TileKind::Crust(CrustTile::RootMat));
                }
                let burrow_width = if variant == 2 { 2_600 } else { 1_500 };
                if depth >= 7
                    && y < bottom - 8
                    && ridge(value_noise(seed, SALT_BURROW, x, y * 2, 15, 9))
                        > NOISE_MAX - burrow_width
                {
                    return Some(TileKind::Air);
                }
            }
            None
        }
        Stratum::ClayBeds => {
            if plain {
                let share = if variant(settings, stratum) == 0 {
                    90
                } else {
                    140
                };
                if hash2(seed, SALT_POT, x, y).is_multiple_of(share) {
                    return Some(TileKind::Artifact(crate::terrain::ArtifactKind::ClayPot));
                }
            }
            None
        }
        Stratum::SlateShelves => {
            let galleries = variant(settings, stratum) == 0;
            if plain && slate_shelf(seed, top, x, y, !galleries) {
                return Some(TileKind::Crust(CrustTile::Slate));
            }
            if open && solid_above {
                let share = if galleries { 2 } else { 5 };
                if hash2(seed, SALT_STALACTITE, x, y).is_multiple_of(share) {
                    return Some(TileKind::Cave(crate::cave::CaveTile::Stalactite));
                }
            }
            None
        }
        Stratum::BasaltColumns => {
            match crust::lava_tube_cell(seed, top, bottom, x, y) {
                Some(false) => return Some(TileKind::Air),
                Some(true) if !open => {
                    // A rare pool of lava in a tube's floor.
                    let floor = crust::lava_tube_cell(seed, top, bottom, x, y - 1) == Some(false);
                    if floor && hash2(seed, SALT_LAVA, x.div_euclid(2), y).is_multiple_of(41) {
                        return Some(TileKind::Lava);
                    }
                    return Some(TileKind::Crust(CrustTile::TubeGlass));
                }
                _ => {}
            }
            let forest = variant(settings, stratum) == 1;
            if plain && (forest || y > top + 6) && basalt_column(seed, x, y) {
                return Some(TileKind::Crust(CrustTile::BasaltColumn));
            }
            None
        }
        _ => None,
    }
}

/// What the lost miner's cabin writes at `(x, y)` (`cabin`: its walls' rectangle).
#[must_use]
pub const fn cabin_tile(cabin: &TileRect, surface: i32, x: i32, y: i32) -> Option<TileKind> {
    let door_column = cabin.min.x + 2;
    // The old entrance shaft, filled in with rubble, from just under the surface to the roof.
    if x == door_column && y > surface && y < cabin.min.y {
        return Some(TileKind::Crust(CrustTile::Rubble));
    }
    if !cabin.contains(TilePosition { x, y }) {
        return None;
    }
    let wall = x == cabin.min.x || x == cabin.max.x || y == cabin.min.y || y == cabin.max.y;
    if wall {
        return Some(TileKind::Crust(CrustTile::CabinLog));
    }
    // The miner's strongbox by the far wall, his table (a stub of log) in the middle.
    if y == cabin.max.y - 1 && x == cabin.max.x - 2 {
        return Some(TileKind::Crust(CrustTile::Strongbox));
    }
    if y == cabin.max.y - 1 && x == cabin.min.x + 6 {
        return Some(TileKind::Crust(CrustTile::CabinLog));
    }
    Some(TileKind::Air)
}

/// What the Sinkhole writes at `(x, y)` over the stamped gate chamber.
///
/// The funnel from the chamber's ceiling up to the lip (choked with rubble at the top, its
/// walls crumbling), the core on the chamber floor and the Fathers' strongbox beside it.
#[must_use]
pub const fn sinkhole_tile(site: &SinkholeSite, x: i32, y: i32) -> Option<TileKind> {
    let centre = site.core.x;
    if y == site.core.y && x == centre {
        return Some(TileKind::Crust(CrustTile::SinkholeCore));
    }
    if y == site.core.y && x == site.chamber.min.x + 4 {
        return Some(TileKind::Crust(CrustTile::Strongbox));
    }
    let ceiling = site.chamber.min.y;
    if y > ceiling || y < site.lip_row {
        return None;
    }
    let rows_up = ceiling - y;
    let half = crust::funnel_half_width(rows_up);
    let dx = x - centre;
    let dx = if dx < 0 { -dx } else { dx };
    if dx > half + 1 {
        return None;
    }
    if y < site.lip_row + 3 || dx == half + 1 {
        // The choked top and the crumbling walls.
        return Some(TileKind::Crust(CrustTile::Rubble));
    }
    Some(TileKind::Air)
}

/// The Sinkhole site if it reaches `rect`.
#[must_use]
pub fn sinkhole_touching(settings: &WorldSettings, rect: &TileRect) -> Option<SinkholeSite> {
    crust::sinkhole_site(settings).filter(|site| {
        let funnel = TileRect {
            min: TilePosition {
                x: site.chamber.min.x,
                y: site.lip_row,
            },
            max: site.chamber.max,
        };
        funnel.intersects(rect)
    })
}
