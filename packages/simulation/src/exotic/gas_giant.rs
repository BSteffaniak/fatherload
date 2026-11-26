//! The gas giant (Wave 13 planets-d; the space generator's gas giant tag): no ground at all,
//! a deep layered atmosphere you fly through, and the floating cities of the folk who live in
//! its sky.
//!
//! * **The sky**: the cloud tops billow at the datum ([`cloud_top`]); above them float three
//!   **sky cities** ([`cities`]): a deck of plating on a keel of hull and glowing lift bladders,
//!   dock pylons at both ends, towers and lanterns. The first is the settlement (its middle
//!   kept clear for the aliens' town: the Siphra, drifting bells of glass, settle gas giants),
//!   the others a quarter of a lap either way. Half a lap round stands **the Eye**: a wall of
//!   storm round a calm well, a cloud platform in it holding the Thunderhead's seal. An eighth
//!   of a lap round floats **the Builders' Aerostat**, a builder room on a platform held up by
//!   bladders.
//! * **The bands** under the cloud tops (concentric rings of gas down to the core chamber; a
//!   rig flies through them, punching through cloud banks that give at a touch of the drill):
//!   * **Cloud Tops**: cream ammonia cloud, pockets of glowing ammonia (skystone); rule:
//!     **jet streams** (lanes of wind 24 rows deep, east and west by turns).
//!   * **Storm Belt**: ochre storm cloud, pockets of charged gas (cloud silver); rule:
//!     **lightning** (columns flicker, then a bolt strikes down them).
//!   * **Pressure Deep**: dense blue haze, pockets of heavy gas (platinum); rule: **crushing
//!     pressure** (warned at the band's top, harder the deeper; high-gravity gear braces).
//!   * **Diamond Rain**: thin soot cloud, lumps of metallic hydrogen and diamond hail; rule:
//!     **diamond rain** (hailstones fall under growing shadows and leave diamond).
//! * **Everywhere**: the thick gas holds a craft up ([`super::ExoticRule::Buoyancy`]): falls
//!   are gentle, so a rig can dive the bands and climb back out.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

use super::{
    ExoticTile, air, exotic, lap_column, outpost_room_tile, roll, secret_tile, settlement_column,
    settlement_half, value_noise2, wrap_delta,
};
use crate::{
    planet::{
        PlanetGround, PlanetPalette, PlanetParams, PlanetStratum, PlanetTheme, ThemeWorld,
        wilds::{Cell, chunk_with, core_floor_row},
    },
    polar::PolarGrid,
    terrain::{ChunkPosition, MineralKind, Tile, TileKind, TilePosition},
};

/// The gas giant's bands (registry data: the generic generator's base under the clouds).
pub const STRATA: [PlanetStratum; 4] = [
    PlanetStratum {
        name: "Cloud Tops",
        share_permille: 220,
        rock: TileKind::Exotic(ExoticTile::CloudIce),
        accent: TileKind::Exotic(ExoticTile::CloudIce),
        accent_permille: 0,
        ores: &[],
    },
    PlanetStratum {
        name: "Storm Belt",
        share_permille: 260,
        rock: TileKind::Exotic(ExoticTile::StormCloud),
        accent: TileKind::Exotic(ExoticTile::StormCloud),
        accent_permille: 0,
        ores: &[],
    },
    PlanetStratum {
        name: "Pressure Deep",
        share_permille: 280,
        rock: TileKind::Exotic(ExoticTile::DeepHaze),
        accent: TileKind::Exotic(ExoticTile::DeepHaze),
        accent_permille: 0,
        ores: &[],
    },
    PlanetStratum {
        name: "Diamond Rain",
        share_permille: 240,
        rock: TileKind::Exotic(ExoticTile::SootCloud),
        accent: TileKind::Exotic(ExoticTile::MetallicHydrogen),
        accent_permille: 0,
        ores: &[(MineralKind::Diamond, 30)],
    },
];

/// The gas giant theme (registry entry [`super::GAS_GIANT`]).
pub const THEME: PlanetTheme = PlanetTheme {
    id: super::GAS_GIANT,
    name: "Gas Giant",
    palette: PlanetPalette {
        sky: [214, 176, 132],
        atmosphere: [236, 196, 140],
        surface: [240, 226, 196],
        rock: [196, 132, 76],
        deep: [70, 86, 120],
        core: [210, 226, 255],
    },
    crust: TileKind::Exotic(ExoticTile::CloudIce),
    crust_rows: 0,
    strata: &STRATA,
    core_ore: TileKind::Ore(MineralKind::Diamond),
    caves_per_256_columns: 0,
    hill_permille: 5,
};

/// The gas giant's world.
pub struct GasGiantWorld;

/// The registered gas giant world.
pub static GAS_GIANT_WORLD: GasGiantWorld = GasGiantWorld;

/// The top row of a sky city's deck.
pub const DECK_ROW: i32 = -40;
/// Rows of deck plating.
pub const DECK_ROWS: i32 = 2;
/// Most rows the cloud tops billow above the datum.
pub const BILLOW_ROWS: i32 = 8;
/// Rows of the cloud deck under the cloud tops (dense enough to read as a surface).
pub const DECK_CLOUD_ROWS: i32 = 6;
/// The Eye: half its width (inside the wall), the wall's thickness, its top and its floor.
pub const EYE_HALF: i64 = 34;
pub const EYE_WALL: i64 = 7;
pub const EYE_TOP: i32 = -34;
/// The arena platform's top row in the Eye.
pub const EYE_FLOOR: i32 = 18;
/// The Eye's bottom (a churning floor of storm wall).
pub const EYE_BOTTOM: i32 = 34;
/// The platform's half width.
pub const EYE_PLATFORM: i64 = 14;
/// The Aerostat's floor row.
pub const AEROSTAT_FLOOR: i32 = -18;
/// Extra sky over the hills (towers and the Eye's wall).
const EXTRA_SKY: i32 = 24;

/// A sky city: its middle column, half width, keel depth and whether it is the settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkyCity {
    pub centre: i64,
    pub half: i32,
    pub keel: i32,
    pub settlement: bool,
}

/// The three sky cities of `ground` (the settlement first).
#[must_use]
pub fn cities(ground: &PlanetGround) -> [SkyCity; 3] {
    let home = settlement_column(ground);
    let half = settlement_half(ground, 40) + 6;
    [
        SkyCity {
            centre: home,
            half,
            keel: 36,
            settlement: true,
        },
        SkyCity {
            centre: lap_column(ground, home, 250),
            half: 30,
            keel: 24,
            settlement: false,
        },
        SkyCity {
            centre: lap_column(ground, home, 750),
            half: 26,
            keel: 22,
            settlement: false,
        },
    ]
}

/// The city whose deck spans `column`, if any.
#[must_use]
pub fn city_at(ground: &PlanetGround, column: i64) -> Option<SkyCity> {
    let width = i64::from(ground.strip_width());
    cities(ground)
        .into_iter()
        .find(|city| wrap_delta(column, city.centre, width).abs() <= i64::from(city.half))
}

/// The Eye's middle column (half a lap from the settlement).
#[must_use]
pub fn eye_column(ground: &PlanetGround) -> i64 {
    lap_column(ground, settlement_column(ground), 500)
}

/// The Aerostat's middle column (an eighth of a lap from the settlement).
#[must_use]
pub fn aerostat_column(ground: &PlanetGround) -> i64 {
    lap_column(ground, settlement_column(ground), 125)
}

/// The cloud tops' row at `column`: billowing heads over the datum.
#[must_use]
pub fn cloud_top(ground: &PlanetGround, column: i64) -> i32 {
    let broad = ground.periodic_noise(60, column, 64);
    let fine = ground.periodic_noise(61, column, 16);
    let value = (broad * 3 + fine) / 4;
    // Rounded heads: the square makes the tops puff up and the gaps sag.
    let billow = value * value / 1_024;
    -i32::try_from(billow * i64::from(BILLOW_ROWS) / 1_024).unwrap_or(0)
}

/// The surface row at `column`: a city's deck, or the cloud tops.
#[must_use]
pub fn surface(ground: &PlanetGround, column: i64) -> i32 {
    if city_at(ground, column).is_some() {
        return DECK_ROW;
    }
    cloud_top(ground, column)
}

/// The band (0-3) at `(column, row)` below the cloud tops, measured from the cloud tops (not a
/// city's deck), with ragged edges.
#[must_use]
pub fn band_at(ground: &PlanetGround, column: i64, row: i32) -> usize {
    let depth = row - cloud_top(ground, column);
    let total = i64::from(ground.core_chamber_top());
    let mut share = 0_i64;
    for (index, stratum) in STRATA.iter().enumerate().take(STRATA.len() - 1) {
        share += i64::from(stratum.share_permille);
        let ragged = ground.periodic_noise(80 + index as u64, column, 32) * 10 / 1_024 - 5;
        if i64::from(depth) < total * share / 1_000 + ragged {
            return index;
        }
    }
    STRATA.len() - 1
}

/// The first row of band `band` (its top, at its mean depth) under the cloud tops at the
/// datum (the HUD's pressure gauge, tests).
#[must_use]
pub fn band_top_row(ground: &PlanetGround, band: usize) -> i32 {
    let total = i64::from(ground.core_chamber_top());
    let share: i64 = STRATA
        .iter()
        .take(band)
        .map(|stratum| i64::from(stratum.share_permille))
        .sum();
    i32::try_from(total * share / 1_000).unwrap_or(0)
}

/// A sky city's tile at `dx` columns from its middle, row `row`.
fn city_tile(ground: &PlanetGround, city: &SkyCity, dx: i64, row: i32) -> Option<Tile> {
    let half = i64::from(city.half);
    if dx.abs() > half {
        return None;
    }
    if (DECK_ROW..DECK_ROW + DECK_ROWS).contains(&row) {
        return Some(exotic(ExoticTile::SkyDeck));
    }
    // The keel: an upturned dome under the deck, its shell hull, its inside lift bladders
    // between ribs.
    let keel_top = DECK_ROW + DECK_ROWS;
    if row >= keel_top {
        let d = i64::from(row - keel_top);
        let k = i64::from(city.keel);
        let inside = |shrink: i64| {
            let (h, kk) = ((half - shrink).max(1), (k - shrink).max(1));
            dx * dx * kk * kk + d * d * h * h <= h * h * kk * kk
        };
        if !inside(0) {
            return None;
        }
        if !inside(2) || dx.rem_euclid(7) == 0 || d == 0 {
            return Some(exotic(ExoticTile::CityHull));
        }
        return Some(exotic(ExoticTile::LiftBladder));
    }
    // Over the deck: dock pylons at both ends, towers and lanterns.
    let up = DECK_ROW - row;
    if dx.abs() >= half - 1 {
        return match up {
            1..=6 => Some(exotic(ExoticTile::CityTower)),
            7 => Some(exotic(ExoticTile::CityLantern)),
            _ => None,
        };
    }
    // The settlement keeps its middle clear for the town (and for docking).
    let clear = if city.settlement {
        i64::from(settlement_half(ground, 40)) - 2
    } else {
        6
    };
    if dx.abs() <= clear {
        // A side city's spire at its middle.
        if !city.settlement && dx == 0 {
            return match up {
                1..=17 => Some(exotic(ExoticTile::CityTower)),
                18 => Some(exotic(ExoticTile::CityLantern)),
                _ => None,
            };
        }
        return None;
    }
    // Towers three wide every nine columns, of hashed heights.
    let slot = dx.div_euclid(9);
    let offset = dx.rem_euclid(9);
    if (3..=5).contains(&offset) {
        let height = 4 + i32::try_from(roll(ground, 3, city.centre, slot) % 10).unwrap_or(0);
        if up >= 1 && up <= height {
            // Windows: lanterns in the tower's middle column now and then.
            if offset == 4 && up % 3 == 0 {
                return Some(exotic(ExoticTile::CityLantern));
            }
            return Some(exotic(ExoticTile::CityTower));
        }
        if offset == 4 && up == height + 1 {
            return Some(exotic(ExoticTile::CityLantern));
        }
    }
    None
}

/// The Eye's tile at `dx` from its middle, row `row` (`None` outside its well).
fn eye_tile(ground: &PlanetGround, column: i64, dx: i64, row: i32) -> Option<Tile> {
    if !(EYE_TOP..=EYE_BOTTOM + 3).contains(&row) || dx.abs() >= EYE_HALF + EYE_WALL {
        return None;
    }
    // The wall's inner face churns (a few columns in and out by row).
    let churn = value_noise2(ground, 62, column, row, 8, 4) * 4 / 1_024 - 2;
    if dx.abs() >= EYE_HALF + churn {
        // The wall's top billows over the cloud tops.
        let top = EYE_TOP + i32::try_from(dx.abs() - EYE_HALF).unwrap_or(0);
        return (row >= top).then_some(exotic(ExoticTile::EyeWall));
    }
    // The churning floor of the well.
    if row >= EYE_BOTTOM {
        return Some(exotic(ExoticTile::EyeWall));
    }
    // The arena platform and the seal on it.
    if dx.abs() <= EYE_PLATFORM && (EYE_FLOOR..EYE_FLOOR + 3).contains(&row) {
        return Some(exotic(if row == EYE_FLOOR {
            ExoticTile::CloudIce
        } else {
            ExoticTile::EyeWall
        }));
    }
    if dx == 0 && row == EYE_FLOOR - 1 {
        return Some(exotic(ExoticTile::BossSeal));
    }
    // The landmark's relic on the platform's edge.
    if dx == EYE_PLATFORM - 2 && row == EYE_FLOOR - 1 {
        return Some(exotic(ExoticTile::LandmarkRelic));
    }
    Some(air())
}

/// The Aerostat's tile at `dx` from its middle, row `row`.
fn aerostat_tile(dx: i64, row: i32) -> Option<Tile> {
    if let Some(tile) = outpost_room_tile(dx, row, AEROSTAT_FLOOR) {
        return Some(tile);
    }
    if dx.abs() <= 9 && (AEROSTAT_FLOOR..AEROSTAT_FLOOR + 2).contains(&row) {
        return Some(exotic(ExoticTile::BuilderPlate));
    }
    // Two bladders hold it up.
    let d = i64::from(row - (AEROSTAT_FLOOR + 4));
    for centre in [-5_i64, 5] {
        let x = dx - centre;
        if x * x * 4 + d * d * 9 <= 36 {
            return Some(exotic(ExoticTile::LiftBladder));
        }
    }
    None
}

/// The builder outpost of a gas giant (see [`super::builder_outpost`]).
#[must_use]
pub fn outpost(ground: &PlanetGround) -> super::BuilderOutpost {
    let column = i32::try_from(aerostat_column(ground)).unwrap_or(0);
    super::BuilderOutpost {
        style: super::ExoticStyle::GasGiant,
        column,
        floor: AEROSTAT_FLOOR,
        beacon: TilePosition {
            x: column,
            y: AEROSTAT_FLOOR - 1,
        },
    }
}

/// The band's cloud, its pocket, how much of the band is cloud (per 1,024 of the noise) and
/// the noise's cell size.
const fn band_clouds(band: usize) -> (ExoticTile, ExoticTile, i64, (i64, i32)) {
    match band {
        0 => (
            ExoticTile::CloudIce,
            ExoticTile::AmmoniaPocket,
            560,
            (32, 8),
        ),
        1 => (
            ExoticTile::StormCloud,
            ExoticTile::StormPocket,
            520,
            (16, 10),
        ),
        2 => (ExoticTile::DeepHaze, ExoticTile::DeepPocket, 500, (32, 6)),
        _ => (ExoticTile::SootCloud, ExoticTile::DiamondHail, 600, (16, 8)),
    }
}

/// The gas under the cloud tops at `(first, row)` (`top` the cloud tops' row there).
fn band_tile(ground: &PlanetGround, first: i64, row: i32, top: i32) -> Tile {
    let depth = row - top;
    // The cloud deck: dense enough to read as the giant's face (with holes to fall through).
    if depth < DECK_CLOUD_ROWS {
        let deck = value_noise2(ground, 63, first, row, 8, 3);
        return if deck > 300 {
            exotic(ExoticTile::CloudIce)
        } else {
            air()
        };
    }
    let band = band_at(ground, first, row);
    let (cloud, pocket, threshold, (cell_w, cell_h)) = band_clouds(band);
    let purpose = 64 + band as u64;
    let density = value_noise2(ground, purpose, first, row, cell_w, cell_h);
    if density < threshold {
        return air();
    }
    // Gas pockets cluster in the clouds' hearts.
    let heart = value_noise2(ground, 70 + band as u64, first, row, 8, 4);
    if density > threshold + 140 && heart > 820 {
        return exotic(pocket);
    }
    if band == 3 && heart < 140 {
        return exotic(ExoticTile::MetallicHydrogen);
    }
    exotic(cloud)
}

impl ThemeWorld for GasGiantWorld {
    fn theme(&self) -> crate::planet::PlanetThemeId {
        super::GAS_GIANT
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
        let middle = i64::from(origin.x) + 8;
        let near = |centre: i64, reach: i64| wrap_delta(middle, centre, width).abs() <= reach;
        let city_list = cities(ground);
        let near_cities: Vec<SkyCity> = city_list
            .into_iter()
            .filter(|city| near(city.centre, i64::from(city.half) + 16))
            .collect();
        let eye = eye_column(ground);
        let near_eye = near(eye, EYE_HALF + EYE_WALL + 16);
        let aerostat = aerostat_column(ground);
        let near_aerostat = near(aerostat, 32);
        chunk_with(
            ground,
            chunk,
            |column| surface(ground, column),
            |cell: &Cell| {
                if cell.in_core(ground) {
                    return cell.base;
                }
                for city in &near_cities {
                    if let Some(tile) = city_tile(
                        ground,
                        city,
                        wrap_delta(cell.first, city.centre, width),
                        cell.row,
                    ) {
                        return tile;
                    }
                }
                if near_eye
                    && let Some(tile) = eye_tile(
                        ground,
                        cell.first,
                        wrap_delta(cell.first, eye, width),
                        cell.row,
                    )
                {
                    return tile;
                }
                if near_aerostat
                    && let Some(tile) =
                        aerostat_tile(wrap_delta(cell.first, aerostat, width), cell.row)
                {
                    return tile;
                }
                let top = cloud_top(ground, cell.first);
                if let Some(tile) = secret_tile(
                    ground,
                    cell.first,
                    cell.row,
                    top,
                    exotic(ExoticTile::EyeWall),
                    exotic(ExoticTile::CityLantern),
                ) {
                    return tile;
                }
                if cell.row < top {
                    return air();
                }
                band_tile(ground, cell.first, cell.row, top)
            },
        )
    }

    fn settlement_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let city = cities(ground)[0];
        let half = settlement_half(ground, 40) - 2;
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(city.centre - i64::from(half)).unwrap_or(0),
            width: half * 2,
            floor: DECK_ROW,
        })
    }

    fn landmark_site(&self, ground: &PlanetGround) -> Option<crate::planet::SurfaceSite> {
        let centre = eye_column(ground);
        let half = i32::try_from(EYE_PLATFORM).unwrap_or(14);
        Some(crate::planet::SurfaceSite {
            first: i32::try_from(centre).unwrap_or(0) - half,
            width: half * 2 + 1,
            floor: EYE_FLOOR,
        })
    }
}
