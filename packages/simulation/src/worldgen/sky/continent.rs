//! The content of a sky continent ([`GeneratorVersion::V9`], Wave 4 T21, expansion plan
//! WP-SKY "sky continents").
//!
//! A V8 continent is a placeholder slab of continent rock under turf. On a V9 world every
//! continent footprint ([`super::SkyLandKind::Continent`]) carries a [`ContinentPlan`], a pure
//! function of the land's shape seed and geometry, and its tiles come from it:
//!
//! * **Zones.** The width (less a rim at each edge) is cut into five zones, and one of a few
//!   layouts assigns them: a **sky city** over two adjacent zones (flattened street, foundation
//!   paving; buildings are presentation, like villages, see [`SkyCity`]), a **temple** zone (a
//!   flattened plain holding a sky temple, [`TemplePlan`]), a **mountain** zone (one to three
//!   peaks up to about 40 rows tall, storm glass veins, harpy nests on the summits) and a
//!   **lake** zone (a basin of open water a row below its shores, [`lake_water_at`]).
//! * **Body.** Turf, soil, island rock and then continent rock; winding **caves** below the
//!   surface layers; a bedrock-like **aether core** ([`crate::sky::SkyTile::AetherCore`], an
//!   ellipse in the middle of the body, indestructible) wrapped in glowing aether crystal;
//!   storm glass in the mountains and rare deep aether veins.
//! * **The temple** ([`TEMPLE`]) is a pilot-heavy dungeon: a shrine on the surface entered by
//!   crawling through a vent grate (rigs are stopped by grates, [`crate::poi::SpaceAccess`]
//!   `RequiresPilot`), a stair down into a lever hall, a gate the lever opens, a hall holding
//!   the keystone and a sentry, a crawlway down to the vault's keystone gate, and relic chests.
//!   Temple stone is indestructible, so the only way in is on foot (`world/sky_temples.rs` has
//!   the lever and keystone mechanism).
//! * **Harpy nests** ([`ContinentPlan::nests`]): woven nests with eggs on the tallest summits;
//!   the enemy director keeps a harpy nester on each (`world/enemies/sky.rs`).
//!
//! Everything is integer arithmetic on the land's fields, so chunks stay seamless and every
//! platform agrees. V8 continents (no plan) are unchanged.
//!
//! [`GeneratorVersion::V9`]: crate::worldgen::GeneratorVersion::V9

use crate::{
    base::BuiltTile,
    sky::{SkyBiome, SkyTile},
    terrain::{TileKind, TilePosition},
    village::VillageId,
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, RegionRng, fractal_noise, hash2, line_noise, ridge, value_noise},
    },
};

use super::SkyLand;

/// Columns at each edge of a continent left out of the zones (the rounded rim).
const RIM_COLUMNS: i32 = 30;
/// Zones a continent's width is cut into.
const ZONES: i32 = 5;
/// Columns a flattened area (city, temple plain, lake shore) blends back into the hills over.
const RAMP_COLUMNS: i32 = 8;
/// Rows of foundation paving under a city's street.
const STREET_ROWS: i32 = 2;
/// The tallest a peak rises above the continent's top row.
pub const MAX_PEAK_ROWS: i32 = 40;

const SALT_PLAN: u64 = 0x5C1E_C001;
const SALT_CAVE: u64 = 0x5C1E_C002;
const SALT_GLASS: u64 = 0x5C1E_C003;
const SALT_AETHER: u64 = 0x5C1E_C004;
const SALT_PEAK: u64 = 0x5C1E_C005;
const SALT_CITY: u64 = 0x5C1E_C006;

/// What a zone of a continent holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Zone {
    City,
    Temple,
    Mountain,
    Lake,
}

/// The zone layouts (the city spans two adjacent zones; the temple never sits at an end, where
/// the body is thin).
const LAYOUTS: [[Zone; ZONES as usize]; 4] = [
    [
        Zone::Mountain,
        Zone::Temple,
        Zone::City,
        Zone::City,
        Zone::Lake,
    ],
    [
        Zone::Lake,
        Zone::City,
        Zone::City,
        Zone::Temple,
        Zone::Mountain,
    ],
    [
        Zone::Mountain,
        Zone::City,
        Zone::City,
        Zone::Temple,
        Zone::Lake,
    ],
    [
        Zone::Lake,
        Zone::Temple,
        Zone::City,
        Zone::City,
        Zone::Mountain,
    ],
];

/// One mountain peak: a dome `half_width` columns wide each side rising `height` rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Peak {
    pub center: i32,
    pub half_width: i32,
    pub height: i32,
}

/// A lake: open water from `level` down to the basin floor over `first..=last`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lake {
    pub center: i32,
    pub half_width: i32,
    /// The row of the water's surface (one row under the shore's turf).
    pub level: i32,
    /// Rows the basin sinks below the water's surface at its middle.
    pub depth: i32,
}

/// The aether core: an ellipse of indestructible core rock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Core {
    pub center_x: i32,
    pub center_y: i32,
    pub half_width: i32,
    pub half_height: i32,
}

impl Core {
    /// The ellipse test scaled by `percent` (100: the core itself), in integers.
    #[must_use]
    pub const fn contains(&self, x: i32, y: i32, percent: i64) -> bool {
        let dx = (x - self.center_x) as i64 * 100;
        let dy = (y - self.center_y) as i64 * 100;
        let a = self.half_width as i64 * percent;
        let b = self.half_height as i64 * percent;
        if a <= 0 || b <= 0 {
            return false;
        }
        dx * dx * b * b + dy * dy * a * a <= a * a * b * b
    }
}

/// A sky city's site: a flattened street over `first..=last` at `street_row` (the row people
/// and rigs stand on).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CitySite {
    pub first: i32,
    pub last: i32,
    pub street_row: i32,
}

/// A sky temple placed on a continent: [`TEMPLE`] stamped with its top-left corner at
/// `(left, top)`, mirrored left-right when `mirrored`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplePlan {
    pub left: i32,
    pub top: i32,
    pub mirrored: bool,
}

/// The sky temple's template, row by row.
///
/// Legend: `.` the land as generated (open sky above
/// the ground), ` ` hollow, `#` temple stone, `c` aether crystal, `=` a vent grate (pilots
/// crawl through, rigs cannot), `D` a lever gate, `K` the keystone gate, `L` the lever, `k` the
/// keystone, `C` a relic chest. Row [`TEMPLE_GROUND_ROW`] lies on the plain's ground line.
pub const TEMPLE: [&str; 23] = [
    "....########......................................",
    "...##c....c##.....................................",
    "...#        #.....................................",
    "...#        #.....................................",
    "...#        #.....................................",
    "...#        #.....................................",
    "...=        #.....................................",
    "#####       ######################################",
    "######       #####################################",
    "#######       ####################################",
    "########       ###################################",
    "#########         c#        c        #############",
    "##########         #                 #############",
    "##########         #                 #############",
    "##########L        D         k       #############",
    "##########         D        ###      =    =      #",
    "############################################     #",
    "##########################################       #",
    "####################################     #      ##",
    "####################################    cK       #",
    "####################################C C  K   #   #",
    "######################################c###########",
    "##################################################",
];
/// The template row that lies on the ground line of the temple's plain.
pub const TEMPLE_GROUND_ROW: i32 = 7;
/// The template's width in columns.
pub const TEMPLE_WIDTH: i32 = 50;
/// The template's height in rows.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "23 rows"
)]
pub const TEMPLE_HEIGHT: i32 = TEMPLE.len() as i32;
/// Where the temple's guards stand (template cells): a sentry in the keystone hall, a storm
/// elemental before the vault.
pub const TEMPLE_GUARDS: [(i32, i32); 2] = [(33, 14), (45, 19)];

impl TemplePlan {
    /// Every tile the temple covers.
    #[must_use]
    pub const fn rect(&self) -> TileRect {
        TileRect {
            min: TilePosition {
                x: self.left,
                y: self.top,
            },
            max: TilePosition {
                x: self.left + TEMPLE_WIDTH - 1,
                y: self.top + TEMPLE_HEIGHT - 1,
            },
        }
    }

    /// The template cell `(column, row)` as a world tile.
    #[must_use]
    pub const fn world(&self, (column, row): (i32, i32)) -> TilePosition {
        let x = if self.mirrored {
            self.left + TEMPLE_WIDTH - 1 - column
        } else {
            self.left + column
        };
        TilePosition {
            x,
            y: self.top + row,
        }
    }

    /// The template character at world tile `(x, y)` (`None` outside the temple).
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> Option<char> {
        let row = y - self.top;
        let mut column = x - self.left;
        if !(0..TEMPLE_HEIGHT).contains(&row) || !(0..TEMPLE_WIDTH).contains(&column) {
            return None;
        }
        if self.mirrored {
            column = TEMPLE_WIDTH - 1 - column;
        }
        TEMPLE[usize::try_from(row).ok()?]
            .as_bytes()
            .get(usize::try_from(column).ok()?)
            .map(|byte| char::from(*byte))
    }

    /// The vent grate a pilot crawls in through, from the ground outside.
    #[must_use]
    pub const fn entrance(&self) -> TilePosition {
        self.world((3, 6))
    }

    /// The temple's guard posts (world tiles).
    #[must_use]
    pub const fn guard_posts(&self) -> [TilePosition; 2] {
        [self.world(TEMPLE_GUARDS[0]), self.world(TEMPLE_GUARDS[1])]
    }

    /// Every world tile holding template character `wanted`, in template order.
    #[must_use]
    pub fn tiles_of(&self, wanted: char) -> Vec<TilePosition> {
        let mut found = Vec::new();
        for (row, line) in (0_i32..).zip(TEMPLE.iter()) {
            for (column, symbol) in (0_i32..).zip(line.chars()) {
                if symbol == wanted {
                    found.push(self.world((column, row)));
                }
            }
        }
        found
    }
}

/// The tile a temple template character stands for (`None`: the generated land shows through
/// for `.`; `Some(Air)` for hollows).
#[must_use]
pub const fn temple_tile(symbol: char) -> Option<TileKind> {
    Some(match symbol {
        ' ' => TileKind::Air,
        '#' => TileKind::Sky(SkyTile::TempleBrick),
        'c' => TileKind::Sky(SkyTile::AetherCrystal),
        '=' => TileKind::Built(BuiltTile::VentGrate),
        'D' => TileKind::Sky(SkyTile::TempleDoor),
        'K' => TileKind::Sky(SkyTile::KeystoneDoor),
        'L' => TileKind::Sky(SkyTile::TempleLever),
        'k' => TileKind::Sky(SkyTile::TempleKeystone),
        'C' => TileKind::Sky(SkyTile::TempleCache),
        _ => return None,
    })
}

/// Everything a V9 continent holds (see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContinentPlan {
    pub city: Option<CitySite>,
    pub temple: Option<TemplePlan>,
    pub lake: Option<Lake>,
    pub peaks: [Option<Peak>; 3],
    /// The summit column and row of each harpy nest (the nest's floor row).
    pub nests: [Option<TilePosition>; 2],
    pub core: Core,
    /// Columns of the plain the temple stands on (flattened to the land's top row).
    temple_plain: Option<(i32, i32)>,
}

impl ContinentPlan {
    /// The plan of a continent with these fields (pure).
    #[must_use]
    pub(super) fn new(
        center_x: i32,
        half_width: i32,
        top_row: i32,
        depth: i32,
        shape: u64,
    ) -> Self {
        let mut rng = RegionRng::new(shape, SALT_PLAN, center_x, top_row);
        let first = center_x - half_width + RIM_COLUMNS;
        let zone_width = (2 * (half_width - RIM_COLUMNS) / ZONES).max(1);
        let zone = |index: i32| {
            (
                first + index * zone_width,
                first + (index + 1) * zone_width - 1,
            )
        };
        let layout = LAYOUTS[usize::try_from(rng.range(0, 3)).unwrap_or(0)];
        let mut city = None;
        let mut temple = None;
        let mut temple_plain = None;
        let mut lake = None;
        let mut peaks = [None; 3];
        let mut city_span: Option<(i32, i32)> = None;
        for (index, kind) in (0_i32..).zip(layout) {
            let (start, end) = zone(index);
            match kind {
                Zone::City => {
                    city_span = Some(city_span.map_or((start, end), |(s, _)| (s, end)));
                }
                Zone::Temple if end - start + 1 >= TEMPLE_WIDTH + 2 => {
                    let slack = end - start + 1 - TEMPLE_WIDTH;
                    let left = start + rng.range(0, slack.max(1) - 1);
                    temple = Some(TemplePlan {
                        left,
                        top: top_row - TEMPLE_GROUND_ROW,
                        mirrored: rng.chance(500),
                    });
                    temple_plain = Some((left - 6, left + TEMPLE_WIDTH + 5));
                }
                Zone::Temple => {}
                Zone::Mountain => {
                    let count = rng.range(1, 3);
                    let width = end - start + 1;
                    for (slot, peak) in (0_i32..).zip(peaks.iter_mut()) {
                        if slot >= count {
                            break;
                        }
                        let half_width = rng.range(14, (width / 3).clamp(15, 34));
                        // Peaks stay inside their zone, clear of the flattened neighbours.
                        let low = start + half_width;
                        let high = (end - half_width).max(low);
                        *peak = Some(Peak {
                            center: rng.range(low, high),
                            half_width,
                            height: rng.range(16, MAX_PEAK_ROWS - 2),
                        });
                    }
                }
                Zone::Lake => {
                    let width = end - start + 1;
                    let half_width = rng.range(10, (width / 2 - RAMP_COLUMNS).clamp(11, 26));
                    lake = Some(Lake {
                        center: i32::midpoint(start, end),
                        half_width,
                        level: top_row + 1,
                        depth: rng.range(4, 8),
                    });
                }
            }
        }
        if let Some((start, end)) = city_span {
            city = Some(CitySite {
                first: start + 6,
                last: end - 6,
                street_row: top_row,
            });
        }
        // Nests are settled on the finished land's summits ([`ledge_nests`]).
        let nests = [None; 2];
        let core = Core {
            center_x: center_x + rng.range(-half_width / 8, half_width / 8),
            center_y: top_row + depth * 3 / 5,
            half_width: (half_width / 4).max(12),
            half_height: (depth / 6).max(5),
        };
        Self {
            city,
            temple,
            lake,
            peaks,
            nests,
            core,
            temple_plain,
        }
    }

    /// Whether column `x` lies within `margin` columns of a flattened area (city or temple
    /// plain) or the lake's shores: `Some(blend)` where 0 is fully flat and `RAMP_COLUMNS` is
    /// back to the hills.
    fn flat_blend(&self, x: i32) -> Option<i32> {
        let spans = [
            self.city.map(|city| (city.first - 2, city.last + 2)),
            self.temple_plain,
            self.lake.map(|lake| {
                (
                    lake.center - lake.half_width - 3,
                    lake.center + lake.half_width + 3,
                )
            }),
        ];
        spans
            .into_iter()
            .flatten()
            .filter_map(|(first, last)| {
                let outside = (first - x).max(x - last);
                (outside < RAMP_COLUMNS).then_some(outside.max(0))
            })
            .min()
    }

    /// The surface row of column `x` given the hilly `base` row and the land's `top_row`.
    pub(super) fn surface(&self, x: i32, base: i32, top_row: i32) -> i32 {
        let mut top = self.flat_blend(x).map_or(base, |blend| {
            top_row + (base - top_row) * blend / RAMP_COLUMNS
        });
        for peak in self.peaks.iter().flatten() {
            let dx = x - peak.center;
            if dx.abs() >= peak.half_width {
                continue;
            }
            // A flat summit two columns either side (the nest ledge), else a dome with jagged
            // shoulders.
            let hw = i64::from(peak.half_width);
            let d = i64::from(dx.abs().saturating_sub(2).max(0));
            let rise =
                i32::try_from(i64::from(peak.height) * (hw * hw - d * d) / (hw * hw)).unwrap_or(0);
            let jag = if dx.abs() <= 2 {
                0
            } else {
                (line_noise(u64::from(peak.center.unsigned_abs()), SALT_PEAK, x, 3) * 3 / NOISE_MAX)
                    .min(rise / 3)
            };
            top = top.min(top_row - rise + jag);
        }
        if let Some(lake) = self.lake {
            let dx = x - lake.center;
            if dx.abs() < lake.half_width {
                let hw = i64::from(lake.half_width);
                let d = i64::from(dx);
                let bowl = i32::try_from(i64::from(lake.depth) * (hw * hw - d * d) / (hw * hw))
                    .unwrap_or(0);
                top = lake.level + bowl.max(1);
            }
        }
        top
    }

    /// The decoration standing on the surface at `(x, y)` (`top` is the surface row): harpy
    /// nests on the summits.
    pub(super) fn decor(&self, x: i32, y: i32, top: i32) -> Option<SkyTile> {
        for nest in self.nests.iter().flatten() {
            let dx = x - nest.x;
            if dx.abs() > 2 || top != nest.y {
                continue;
            }
            if y == top - 1 {
                return Some(match dx.abs() {
                    2 => SkyTile::HarpyNest,
                    1 => SkyTile::HarpyEgg,
                    _ => {
                        if hash2(u64::from(nest.x.unsigned_abs()), SALT_PEAK, nest.y, 0)
                            .is_multiple_of(2)
                        {
                            SkyTile::HarpyEgg
                        } else {
                            return None;
                        }
                    }
                });
            }
        }
        None
    }

    /// The body tile at `(x, y)`, `layer` rows under the surface, or `None` for a cave (open
    /// air inside the body). `bottom` is the underside row of the column.
    pub(super) fn body(
        &self,
        land: &SkyLand,
        x: i32,
        y: i32,
        layer: i32,
        bottom: i32,
    ) -> Option<TileKind> {
        if let Some(temple) = self.temple
            && let Some(symbol) = temple.cell(x, y)
            && let Some(tile) = temple_tile(symbol)
        {
            return (tile != TileKind::Air).then_some(tile);
        }
        if self.core.contains(x, y, 100) {
            return Some(TileKind::Sky(SkyTile::AetherCore));
        }
        let shape = land.shape;
        if self.core.contains(x, y, 135) {
            return Some(TileKind::Sky(
                if hash2(shape, SALT_AETHER, x, y).is_multiple_of(2) {
                    SkyTile::AetherCrystal
                } else {
                    SkyTile::ContinentRock
                },
            ));
        }
        let under_city = self
            .city
            .is_some_and(|city| x >= city.first - 2 && x <= city.last + 2);
        if under_city && layer < STREET_ROWS {
            return Some(TileKind::Foundation);
        }
        let in_lake = self
            .lake
            .is_some_and(|lake| (x - lake.center).abs() < lake.half_width);
        // Caves: thin winding tunnels through the body, clear of the surface layers, the
        // underside, the city's foundations and the core's shell.
        if layer >= 8 && y <= bottom - 6 && !(under_city && layer < 14) && !in_lake {
            let noise = fractal_noise(shape, SALT_CAVE, x, y * 2, 44, 22, 2);
            let wobble = value_noise(shape, SALT_CAVE + 1, x, y, 30, 16);
            if ridge(noise) > NOISE_MAX - NOISE_MAX / 14 - wobble / 40 {
                return None;
            }
        }
        let mountain = y < land.top_row - 2;
        let tile = if layer == 0 {
            if in_lake {
                SkyTile::Soil
            } else if mountain && y < land.top_row - 18 {
                SkyTile::Turf(SkyBiome::Frost)
            } else {
                SkyTile::Turf(land.biome)
            }
        } else if layer <= 3 && !mountain {
            SkyTile::Soil
        } else if mountain
            && layer >= 2
            && ridge(value_noise(shape, SALT_GLASS, x, y, 9, 7)) > NOISE_MAX - NOISE_MAX / 16
        {
            SkyTile::StormGlass
        } else if layer <= 6 || mountain {
            if (4..=14).contains(&layer) && hash2(shape, SALT_GLASS + 1, x, y).is_multiple_of(83) {
                SkyTile::StormGlass
            } else {
                SkyTile::Rock(land.biome)
            }
        } else if layer >= 20 && hash2(shape, SALT_AETHER + 1, x, y).is_multiple_of(97) {
            SkyTile::AetherCrystal
        } else if hash2(shape, super::SALT_SHAPE + 3, x, y).is_multiple_of(89) {
            SkyTile::Crystal
        } else {
            SkyTile::ContinentRock
        };
        Some(TileKind::Sky(tile))
    }

    /// Whether `(x, y)` is lake water: an open tile from the water's surface down to the basin.
    #[must_use]
    pub fn water(&self, land: &SkyLand, x: i32, y: i32) -> bool {
        self.lake.is_some_and(|lake| {
            (x - lake.center).abs() < lake.half_width
                && y >= lake.level
                && land.surface_row(x).is_some_and(|top| y < top)
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Sky cities.
// ---------------------------------------------------------------------------------------------

/// Set on every sky city's [`VillageId`] (village ids are small; camps use bit 31).
pub const SKY_CITY_FLAG: u32 = 1 << 30;

/// What a building of a sky city is (presentation picks the art; people pick lots).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SkyBuildingKind {
    /// A tall aeronaut's tower with a mooring mast.
    Tower,
    /// A domed hall.
    Hall,
    /// A market of awnings.
    Market,
    /// A cloud-monastery with a bell.
    Monastery,
    /// A hangar for airships.
    Hangar,
    /// A plain home with a garden roof.
    Home,
}

impl SkyBuildingKind {
    pub const ALL: [Self; 6] = [
        Self::Tower,
        Self::Hall,
        Self::Market,
        Self::Monastery,
        Self::Hangar,
        Self::Home,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tower => "aeronaut tower",
            Self::Hall => "domed hall",
            Self::Market => "cloud market",
            Self::Monastery => "sky monastery",
            Self::Hangar => "airship hangar",
            Self::Home => "home",
        }
    }
}

/// One building of a sky city (drawn by the game; the street under it is foundation).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkyBuilding {
    pub kind: SkyBuildingKind,
    pub first_column: i32,
    pub width: i32,
    /// Rows it rises above the street.
    pub height: i32,
}

/// A sky city: the street, its buildings and its plaza (a bigger settlement grammar than a
/// village: towers, halls, a market, a monastery and hangars along a long street).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkyCity {
    pub id: VillageId,
    pub first_column: i32,
    pub last_column: i32,
    /// The row people and rigs stand on (the top of the foundation paving).
    pub street_row: i32,
    /// The open plaza's columns.
    pub plaza: (i32, i32),
    pub buildings: Vec<SkyBuilding>,
}

impl SkyCity {
    /// Whether the city's area (with `margin` columns and rows around it) holds `tile`.
    #[must_use]
    pub const fn contains(&self, tile: TilePosition, margin: i32) -> bool {
        tile.x >= self.first_column - margin
            && tile.x <= self.last_column + margin
            && tile.y >= self.street_row - 16 - margin
            && tile.y <= self.street_row + 6 + margin
    }

    /// How many people live in it (one per two buildings, at least four).
    #[must_use]
    pub fn population(&self) -> usize {
        (self.buildings.len() / 2).clamp(4, 9)
    }
}

/// The id of the sky city on the continent of continent cell `rx`.
#[must_use]
#[allow(clippy::cast_sign_loss, reason = "zigzag encoding")]
pub const fn sky_city_id(rx: i32) -> VillageId {
    let zigzag = ((rx << 1) ^ (rx >> 31)) as u32;
    VillageId(SKY_CITY_FLAG | (zigzag & !(SKY_CITY_FLAG | crate::people::camps::CAMP_FLAG)))
}

/// Whether `id` names a sky city.
#[must_use]
pub const fn is_sky_city(id: VillageId) -> bool {
    id.0 & SKY_CITY_FLAG != 0 && id.0 & crate::people::camps::CAMP_FLAG == 0
}

/// The continent cell a sky city id names.
#[must_use]
#[allow(clippy::cast_possible_wrap, reason = "zigzag decoding")]
const fn sky_city_cell(id: VillageId) -> i32 {
    let zigzag = id.0 & !SKY_CITY_FLAG;
    ((zigzag >> 1) as i32) ^ -((zigzag & 1) as i32)
}

/// Lays out the city on `site` of the continent of cell `rx` (pure).
fn layout_city(site: CitySite, rx: i32, shape: u64) -> SkyCity {
    let mut rng = RegionRng::new(shape, SALT_CITY, rx, site.street_row);
    let middle = i32::midpoint(site.first, site.last);
    let plaza = (middle - 6, middle + 6);
    let mut buildings = Vec::new();
    let mut cursor = site.first + 1;
    // A monastery and a hangar in every city, the rest rolled.
    let mut must = vec![
        SkyBuildingKind::Monastery,
        SkyBuildingKind::Hangar,
        SkyBuildingKind::Market,
    ];
    while cursor < site.last - 6 {
        if cursor + 6 > plaza.0 && cursor < plaza.1 {
            cursor = plaza.1 + 2;
            continue;
        }
        let kind = must.pop().unwrap_or_else(|| {
            *[
                SkyBuildingKind::Tower,
                SkyBuildingKind::Hall,
                SkyBuildingKind::Home,
                SkyBuildingKind::Home,
                SkyBuildingKind::Tower,
            ]
            .get(usize::try_from(rng.range(0, 4)).unwrap_or(0))
            .unwrap_or(&SkyBuildingKind::Home)
        });
        let (width, height) = match kind {
            SkyBuildingKind::Tower => (rng.range(3, 4), rng.range(8, 12)),
            SkyBuildingKind::Hall => (rng.range(6, 8), rng.range(5, 6)),
            SkyBuildingKind::Market => (rng.range(6, 9), rng.range(3, 4)),
            SkyBuildingKind::Monastery => (rng.range(7, 9), rng.range(7, 9)),
            SkyBuildingKind::Hangar => (rng.range(8, 11), rng.range(5, 6)),
            SkyBuildingKind::Home => (rng.range(4, 5), rng.range(3, 5)),
        };
        if cursor + width > site.last - 1 {
            break;
        }
        if cursor + width > plaza.0 && cursor < plaza.1 {
            cursor = plaza.1 + 2;
            continue;
        }
        buildings.push(SkyBuilding {
            kind,
            first_column: cursor,
            width,
            height,
        });
        cursor += width + rng.range(1, 3);
    }
    SkyCity {
        id: sky_city_id(rx),
        first_column: site.first,
        last_column: site.last,
        street_row: site.street_row,
        plaza,
        buildings,
    }
}

/// The sky city `id` of a world with `settings` (`None` if `id` is no sky city or its
/// continent has none). Only V9 skies have cities.
#[must_use]
pub fn sky_city(settings: &WorldSettings, id: VillageId) -> Option<SkyCity> {
    if !is_sky_city(id) || !settings.generator.has_sky_content() {
        return None;
    }
    let rx = sky_city_cell(id);
    let land = super::continent_cell(settings, rx)?;
    let plan = land.plan()?;
    plan.city.map(|site| layout_city(site, rx, land.shape))
}

/// The sky city of continent `land` (`None` on V8 or without a city).
#[must_use]
pub fn city_of(land: &SkyLand) -> Option<SkyCity> {
    let plan = land.plan()?;
    let rx = super::continent_cell_of(land.id)?;
    plan.city.map(|site| layout_city(site, rx, land.shape))
}

/// Every sky city whose street overlaps columns `first..=last` (west to east). Empty on
/// worlds without sky content.
#[must_use]
pub fn sky_cities_between(settings: &WorldSettings, first: i32, last: i32) -> Vec<SkyCity> {
    if !settings.generator.has_sky_content() || first > last {
        return Vec::new();
    }
    let size = super::CONTINENT_CELL_COLUMNS;
    (first.div_euclid(size)..=last.div_euclid(size))
        .filter_map(|rx| sky_city(settings, sky_city_id(rx)))
        .filter(|city| city.last_column >= first && city.first_column <= last)
        .collect()
}

/// The sky city whose area (plus `margin`) holds `tile`, if any.
#[must_use]
pub fn sky_city_at(settings: &WorldSettings, tile: TilePosition, margin: i32) -> Option<SkyCity> {
    if !settings.generator.has_sky_content() {
        return None;
    }
    let altitude = -tile.y;
    if !(super::CONTINENTS_BOTTOM_ROWS - 40..=super::ORBIT_BOTTOM_ROWS + 40).contains(&altitude) {
        return None;
    }
    sky_cities_between(settings, tile.x - margin, tile.x + margin)
        .into_iter()
        .find(|city| city.contains(tile, margin))
}

/// The harpy nests of continent `land` planned as `plan`: up to two summits, tallest first,
/// whose flat five-column ledge really is the land's surface. (A taller neighbour's shoulder
/// or high hills can bury a lower summit; a nest planned there would have no eggs, and its
/// nester would rage from the start.) Each nest is the ledge's middle column and its surface
/// row (the nest's floor).
pub(super) fn ledge_nests(land: &SkyLand, plan: &ContinentPlan) -> [Option<TilePosition>; 2] {
    let mut summits: Vec<Peak> = plan.peaks.iter().flatten().copied().collect();
    summits.sort_by_key(|peak| (-peak.height, peak.center));
    let mut nests = [None; 2];
    let mut found = summits.into_iter().filter_map(|peak| {
        let floor = land.top_row - peak.height;
        (peak.center - 2..=peak.center + 2)
            .all(|x| land.surface_row(x) == Some(floor))
            .then_some(TilePosition {
                x: peak.center,
                y: floor,
            })
    });
    for nest in &mut nests {
        *nest = found.next();
    }
    nests
}

/// Every V9 continent overlapping `rect` that has a plan, with its plan.
#[must_use]
pub fn continents_touching(
    settings: &WorldSettings,
    rect: &TileRect,
) -> Vec<(SkyLand, ContinentPlan)> {
    if !settings.generator.has_sky_content() {
        return Vec::new();
    }
    super::lands_touching(settings, rect)
        .into_iter()
        .filter_map(|land| land.plan().map(|plan| (land, plan)))
        .collect()
}

/// Every sky temple whose rect overlaps `rect` (with its continent).
#[must_use]
pub fn temples_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<(SkyLand, TemplePlan)> {
    continents_touching(settings, rect)
        .into_iter()
        .filter_map(|(land, plan)| plan.temple.map(|temple| (land, temple)))
        .filter(|(_, temple)| temple.rect().intersects(rect))
        .collect()
}

/// Every harpy nest within `rect` (the nest's floor tile).
#[must_use]
pub fn nests_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<TilePosition> {
    continents_touching(settings, rect)
        .into_iter()
        .flat_map(|(_, plan)| plan.nests)
        .flatten()
        .filter(|nest| rect.contains(*nest))
        .collect()
}

/// Whether `position` is lake water on a V9 continent (an open tile under a lake's surface).
#[must_use]
pub fn lake_water_at(settings: &WorldSettings, position: TilePosition) -> bool {
    if !settings.generator.has_sky_content() {
        return false;
    }
    let altitude = -position.y;
    if !(super::CONTINENTS_BOTTOM_ROWS..=super::ORBIT_BOTTOM_ROWS).contains(&altitude) {
        return false;
    }
    let rect = TileRect {
        min: position,
        max: position,
    };
    super::lands_touching(settings, &rect).iter().any(|land| {
        land.plan()
            .is_some_and(|plan| plan.water(land, position.x, position.y))
    })
}

#[cfg(test)]
mod tests;
