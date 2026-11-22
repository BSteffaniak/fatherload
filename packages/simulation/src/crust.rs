//! Descent I, the Crust, authored for real (Wave 8, expansion plan §14).
//!
//! The tiles, rules and state of the five Crust strata of a Deep Descent world
//! ([`GeneratorVersion::V11`](crate::worldgen::GeneratorVersion::V11)).
//!
//! Each stratum has one signature rule, introduced gently and then pushed:
//!
//! | Stratum | Signature | Where it lives |
//! |---|---|---|
//! | Topsoil | roots, burrows and rain-soaked **mud** that slows and sticks; the lost miner's cabin | [`CrustTile::RootMat`], `Surface(Mud)` grip ([`ground_grip`]) |
//! | Clay Beds | **aquifers** (sealed water pockets that flood tunnels when breached); clay pots to sell | the registry's fluid rules, `Artifact(ClayPot)` |
//! | Grey Stone | **old mine works**: rails to ride ([`CartRide`]), timber sets that collapse when undercut, first gas; Gravel Hollow | [`CrustTile::Rail`], [`CrustTile::Timber`], [`gravel_hollow`] |
//! | Slate Shelves | **slate slides**: slick shelves that slide when undercut; stalactite galleries | [`CrustTile::Slate`] |
//! | Basalt Columns | **lava tubes**: long natural tunnels and the first heat | [`CrustTile::TubeGlass`], [`lava_tube_at`] |
//! | Gate | **The Sinkhole**: extract its core and climb out before the collapse catches you | [`sinkhole_site`], [`SinkholeRun`] |
//!
//! Pure data and geometry live here; the tick is `world/crust.rs` and the generator is
//! [`crate::worldgen::descent::crust`]. Nothing here touches a world before V11: every rule
//! asks [`crust_rules`] first.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    descent::{Descent, Stratum},
    ids::{PlayerId, SimulationTick},
    terrain::{TileKind, TilePosition},
    worldgen::{TileRect, WorldSettings, noise::hash2},
};

/// Whether the Crust's authored rules run in a world of `settings` (Deep Descent worlds only;
/// every older world keeps its classic Crust bit for bit).
#[must_use]
pub const fn crust_rules(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

// --- Tiles -------------------------------------------------------------------------------------

/// A tile of the authored Crust ([`TileKind::Crust`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CrustTile {
    /// A tangle of old roots under the Topsoil's surface: soft, but stringy and slow to cut.
    RootMat,
    /// A timber set of the old mine works: the beam over a gallery or stope. Break it, or the
    /// ground under its post, and the set fails: the roof over it comes down after a creak.
    Timber,
    /// A rail bed of the old mine works: a rig resting on it with the stick held along it rides
    /// the line ([`CartRide`]).
    Rail,
    /// A slick slate shelf: rigs slide on it, and a shelf left hanging slides down.
    Slate,
    /// A basalt column: hexagonal, easier to split than the hard rock around it.
    BasaltColumn,
    /// The glassy crust lining a lava tube.
    TubeGlass,
    /// A log wall of the lost miner's cabin.
    CabinLog,
    /// A strongbox: drill it open for credits, a find and an expedition log.
    Strongbox,
    /// The Sinkhole's core: the anchor stone the Fathers' expedition was digging for.
    SinkholeCore,
    /// Loose rubble left by a collapse.
    Rubble,
    /// Gravel Hollow's packed gravel street.
    Gravel,
}

impl CrustTile {
    pub const ALL: [Self; 11] = [
        Self::RootMat,
        Self::Timber,
        Self::Rail,
        Self::Slate,
        Self::BasaltColumn,
        Self::TubeGlass,
        Self::CabinLog,
        Self::Strongbox,
        Self::SinkholeCore,
        Self::Rubble,
        Self::Gravel,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RootMat => "root mat",
            Self::Timber => "timber set",
            Self::Rail => "rail bed",
            Self::Slate => "slate shelf",
            Self::BasaltColumn => "basalt column",
            Self::TubeGlass => "tube glass",
            Self::CabinLog => "cabin log",
            Self::Strongbox => "strongbox",
            Self::SinkholeCore => "sinkhole core",
            Self::Rubble => "rubble",
            Self::Gravel => "gravel",
        }
    }

    /// Stable code (tile codes, digests).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::RootMat => 0,
            Self::Timber => 1,
            Self::Rail => 2,
            Self::Slate => 3,
            Self::BasaltColumn => 4,
            Self::TubeGlass => 5,
            Self::CabinLog => 6,
            Self::Strongbox => 7,
            Self::SinkholeCore => 8,
            Self::Rubble => 9,
            Self::Gravel => 10,
        }
    }

    /// The tile with code `code` ([`Self::code`]).
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        let mut index = 0;
        while index < Self::ALL.len() {
            if Self::ALL[index].code() == code {
                return Some(Self::ALL[index]);
            }
            index += 1;
        }
        None
    }

    /// Drill strength needed (never above what a Descent I rig has where the tile generates).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::RootMat | Self::Timber | Self::CabinLog | Self::Strongbox | Self::Rubble => 1,
            Self::Rail | Self::Gravel | Self::SinkholeCore => 2,
            Self::Slate | Self::BasaltColumn | Self::TubeGlass => 3,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Rubble => 2,
            Self::Timber | Self::Strongbox => 3,
            Self::Rail | Self::CabinLog | Self::Gravel => 4,
            Self::RootMat | Self::Slate => 5,
            Self::TubeGlass => 6,
            Self::BasaltColumn => 8,
            Self::SinkholeCore => 12,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses (the drill timing table).
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::Rubble => 0.07,
            Self::Timber | Self::CabinLog | Self::Strongbox => 0.1,
            Self::RootMat => 0.11,
            Self::Rail | Self::Gravel => 0.12,
            Self::Slate | Self::TubeGlass => 0.14,
            Self::BasaltColumn => 0.16,
            Self::SinkholeCore => 0.18,
        }
    }

    /// Light the tile gives off, in tiles (presentation; `0` for none).
    #[must_use]
    pub const fn light_radius(self) -> u8 {
        match self {
            Self::SinkholeCore => 4,
            Self::TubeGlass => 1,
            _ => 0,
        }
    }
}

// --- Ground grip: mud and slate -----------------------------------------------------------------

/// How the ground under a resting rig grips it (Deep Descent worlds).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroundGrip {
    /// Ordinary ground.
    Firm,
    /// Rain-soaked mud: the drive bogs down and the thrusters fight the suction.
    Mud,
    /// A slick slate shelf: the rig keeps sliding.
    Slick,
}

/// Share of the drive's top speed left on mud.
pub const MUD_TOP_SPEED_SHARE: f32 = 0.45;
/// Share of the thrusters' lift left while pulling out of mud.
pub const MUD_LIFT_SHARE: f32 = 0.6;
/// How fast a sliding rig loses speed on slick slate with the stick neutral or against it
/// (per second; velocity is divided by `1 + rate * dt` each tick).
pub const SLICK_DRAG_PER_SECOND: f32 = 0.9;

/// The grip of the ground tile `kind` a rig rests on.
#[must_use]
pub const fn ground_grip(kind: TileKind) -> GroundGrip {
    match kind {
        TileKind::Surface(crate::terrain::SurfaceMaterial::Mud) => GroundGrip::Mud,
        TileKind::Crust(CrustTile::Slate) => GroundGrip::Slick,
        _ => GroundGrip::Firm,
    }
}

// --- Mine carts ---------------------------------------------------------------------------------

/// Fixed physics sub-steps of a cart ride per second of world time (tick-rate safe).
pub const CART_STEPS_PER_SECOND: u64 = 240;
/// A cart's progress along one rail tile, in thousandths.
pub const CART_TILE: i32 = 1_000;
/// Top speed of a cart, thousandths of a tile per second.
pub const CART_TOP_SPEED: i32 = 10_000;
/// Speed a cart starts at when a rig climbs aboard (at least), thousandths of a tile a second.
pub const CART_BOARD_SPEED: i32 = 2_000;
/// Pushing along the line (stick held the way it rolls), thousandths of a tile per second².
pub const CART_PUSH: i32 = 3_000;
/// Braking (stick held against it).
pub const CART_BRAKE: i32 = 7_000;
/// Rolling friction with the stick neutral.
pub const CART_FRICTION: i32 = 400;
/// Gravity along a ramp (one row per tile).
pub const CART_SLOPE: i32 = 4_500;

/// A rig riding a rail line of the old mine works (per tick; replicated with the rig's pose so
/// it is predicted and reconciled like movement).
///
/// The rig sits over rail tile `rail` moving `dir` (`-1` west, `1` east), `progress`
/// thousandths of the way to the next rail tile `next` (the same row, one lower on a down
/// ramp or one higher on an up ramp), at `speed` thousandths of a tile per second. Each fixed
/// step integrates the stick, friction and the ramp's pull in whole numbers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CartRide {
    pub rail: TilePosition,
    pub next: TilePosition,
    pub dir: i8,
    pub progress: i32,
    pub speed: i32,
}

impl CartRide {
    /// The rail point the rig rides over, in thousandths of a tile: `(x, y)` of the tile
    /// tops' centres, interpolated along the step to `next`.
    #[must_use]
    pub const fn point_milli(&self) -> (i64, i64) {
        let from_x = self.rail.x as i64 * 1_000 + 500;
        let from_y = self.rail.y as i64 * 1_000;
        let to_x = self.next.x as i64 * 1_000 + 500;
        let to_y = self.next.y as i64 * 1_000;
        let progress = self.progress as i64;
        (
            from_x + (to_x - from_x) * progress / CART_TILE as i64,
            from_y + (to_y - from_y) * progress / CART_TILE as i64,
        )
    }

    /// Rows the line falls per tile on the current step (`1` down a ramp, `-1` up one).
    #[must_use]
    pub const fn slope(&self) -> i32 {
        self.next.y - self.rail.y
    }
}

/// The stick as a cart reads it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CartStick {
    /// `-1`, `0` or `1`.
    pub horizontal: i8,
    /// Held down: take the lower branch at a junction.
    pub down: bool,
}

/// The rail tile after `from` going `dir`: level first, then the lower branch, then the upper;
/// with `prefer_down` the lower branch wins a junction. `None` at the end of the line.
#[must_use]
pub fn next_rail(
    is_rail: impl Fn(TilePosition) -> bool,
    from: TilePosition,
    dir: i8,
    prefer_down: bool,
) -> Option<TilePosition> {
    let x = from.x + i32::from(dir);
    let level = TilePosition { x, y: from.y };
    let lower = TilePosition { x, y: from.y + 1 };
    let upper = TilePosition { x, y: from.y - 1 };
    let (first, second) = if prefer_down {
        (lower, level)
    } else {
        (level, lower)
    };
    [first, second, upper].into_iter().find(|at| is_rail(*at))
}

/// Whether rail tile `from` going `dir` forks (both a level and a lower rail follow).
#[must_use]
pub fn is_junction(is_rail: impl Fn(TilePosition) -> bool, from: TilePosition, dir: i8) -> bool {
    let x = from.x + i32::from(dir);
    is_rail(TilePosition { x, y: from.y }) && is_rail(TilePosition { x, y: from.y + 1 })
}

/// What one fixed cart step did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CartStep {
    Rolled,
    /// It moved onto a new rail tile; `switched` when it took the lower branch of a junction.
    Advanced {
        switched: bool,
    },
    /// The line ended (or broke): the ride is over at this speed (thousandths of a tile a
    /// second, signed by direction).
    Ended {
        velocity: i32,
    },
}

/// One fixed step of `ride` (1 / [`CART_STEPS_PER_SECOND`] s).
pub fn step_cart(
    ride: &mut CartRide,
    stick: CartStick,
    is_rail: impl Fn(TilePosition) -> bool,
) -> CartStep {
    let steps = i32::try_from(CART_STEPS_PER_SECOND).unwrap_or(240);
    let along = i32::from(stick.horizontal) * i32::from(ride.dir);
    let mut accel = match along {
        1 => CART_PUSH,
        -1 => -CART_BRAKE,
        _ => -CART_FRICTION,
    };
    accel += CART_SLOPE * ride.slope();
    ride.speed = (ride.speed + accel / steps).min(CART_TOP_SPEED);
    if ride.speed < 0 {
        // Braked (or rolled back) past a stop: the cart turns round on the spot.
        ride.speed = -ride.speed;
        ride.dir = -ride.dir;
        ride.progress = CART_TILE - ride.progress;
        std::mem::swap(&mut ride.rail, &mut ride.next);
    }
    ride.progress += ride.speed / steps;
    if ride.progress < CART_TILE {
        return CartStep::Rolled;
    }
    ride.progress -= CART_TILE;
    let arrived = ride.next;
    let junction = is_junction(&is_rail, arrived, ride.dir);
    if let Some(next) = next_rail(&is_rail, arrived, ride.dir, stick.down) {
        ride.rail = arrived;
        ride.next = next;
        CartStep::Advanced {
            switched: junction && next.y > arrived.y,
        }
    } else {
        ride.rail = arrived;
        ride.next = arrived;
        ride.progress = 0;
        CartStep::Ended {
            velocity: ride.speed * i32::from(ride.dir),
        }
    }
}

/// Cart steps due by world tick `tick` at `hz` (whole steps since tick 0).
#[must_use]
pub fn cart_steps_by(tick: u64, hz: u32) -> u64 {
    let steps = u128::from(tick) * u128::from(CART_STEPS_PER_SECOND) / u128::from(hz.max(1));
    u64::try_from(steps).unwrap_or(u64::MAX)
}

// --- Timber sets and slate shelves --------------------------------------------------------------

/// How long a failing timber set creaks before the roof comes down.
pub const SUPPORT_CREAK_MILLIS: u64 = 1_600;
/// Columns either side of a failed set's beam that come down with it.
pub const SUPPORT_SPAN: i32 = 2;
/// Rows of roof over a failed beam that fall with it.
pub const SUPPORT_ROOF_ROWS: i32 = 2;
/// How long a slate shelf left hanging shudders before it slides.
pub const SLIDE_WARNING_MILLIS: u64 = 1_200;
/// Tiles of shelf one slide brings down at most.
pub const SLIDE_MAX_TILES: usize = 14;

// --- Lava tubes and heat ------------------------------------------------------------------------

/// Heat units a lava tube runs over its stratum (one heat level).
pub const LAVA_TUBE_HEAT_UNITS: u8 = crate::protection::HEAT_UNITS_PER_LEVEL;
/// Rows between the lava tube bands of the Basalt Columns.
pub const TUBE_BAND_ROWS: i32 = 34;
/// Columns of one tube segment's cell (a cell holds one long tube or none).
pub const TUBE_CELL_COLUMNS: i32 = 160;
/// Half height of a lava tube's air.
pub const TUBE_HALF_HEIGHT: i32 = 2;

const SALT_TUBE: u64 = 0xC705_7001;

/// A lava tube segment: its centreline row at its first column, its column span and its wave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LavaTube {
    pub first: i32,
    pub last: i32,
    pub row: i32,
}

/// The lava tube of band `band` (rows from the stratum's top, [`TUBE_BAND_ROWS`] apart) and
/// cell `cell`, if one runs there, for a Basalt Columns stratum spanning `top..=bottom`.
#[must_use]
pub const fn lava_tube(seed: u64, top: i32, bottom: i32, band: i32, cell: i32) -> Option<LavaTube> {
    let roll = hash2(seed, SALT_TUBE, cell, band);
    // Two cells in three hold a tube; tubes run 70-150 columns.
    if roll.is_multiple_of(3) {
        return None;
    }
    let row = top + 14 + band * TUBE_BAND_ROWS + ((roll >> 8) % 9) as i32;
    if row + TUBE_HALF_HEIGHT + 4 > bottom - 30 {
        return None;
    }
    let length = 70 + ((roll >> 16) % 81) as i32;
    let start = cell * TUBE_CELL_COLUMNS + ((roll >> 32) % 20) as i32;
    Some(LavaTube {
        first: start,
        last: start + length,
        row,
    })
}

/// The centreline row of `tube` in column `x` (a gentle wave, never more than two rows off).
#[must_use]
pub const fn tube_centre(seed: u64, tube: &LavaTube, x: i32) -> i32 {
    let wave = crate::worldgen::noise::line_noise(seed, SALT_TUBE ^ 0xFF, x, 23);
    tube.row + crate::worldgen::noise::centred(wave, 2)
}

/// Whether `(x, y)` lies inside a lava tube's air (`Some(false)`), its glass lining
/// (`Some(true)`) or neither (`None`), for a Basalt Columns stratum spanning `top..=bottom`.
#[must_use]
pub const fn lava_tube_cell(seed: u64, top: i32, bottom: i32, x: i32, y: i32) -> Option<bool> {
    if y < top || y > bottom {
        return None;
    }
    let band = (y - top - 14 + TUBE_BAND_ROWS / 2).div_euclid(TUBE_BAND_ROWS);
    let cell = x.div_euclid(TUBE_CELL_COLUMNS);
    let mut found = None;
    let mut dc = -1;
    while dc <= 0 {
        if let Some(tube) = lava_tube(seed, top, bottom, band, cell + dc)
            && x >= tube.first
            && x <= tube.last
        {
            let centre = tube_centre(seed, &tube, x);
            // The tube narrows to a point at both ends.
            let from_end = if x - tube.first < tube.last - x {
                x - tube.first
            } else {
                tube.last - x
            };
            let half = if from_end < 3 { 1 } else { TUBE_HALF_HEIGHT };
            let dy = y - centre;
            if dy >= -half && dy <= half {
                return Some(false);
            }
            if dy == -half - 1 || dy == half + 1 {
                found = Some(true);
            }
        }
        dc += 1;
    }
    found
}

/// Whether tile `(x, y)` of a V11 world is the air of a lava tube.
#[must_use]
pub fn lava_tube_at(settings: &WorldSettings, x: i32, y: i32) -> bool {
    if !crust_rules(settings) {
        return false;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let (top, bottom) = layout.rows(Stratum::BasaltColumns);
    matches!(
        lava_tube_cell(settings.seed, top, bottom, x, y),
        Some(false)
    )
}

/// Heat units of `stratum` on a Deep Descent world.
///
/// The Crust is authored: cool down to the slate, then the first heat in the Basalt Columns
/// (one radiator tier covers it; its lava tubes need a second). Every other stratum runs to a
/// gate-appropriate curve (Wave 8 fix: placeholders no longer borrow their host band's heat,
/// which cooked a stock rig): a rig fitted to the previous Descent's tier cap is cool at the
/// top of a Descent, and the heat climbs through it to just under what the Descent's own cap
/// covers.
#[must_use]
pub fn stratum_heat_units(stratum: Stratum) -> u8 {
    use crate::{depth_progression::core_tier_cap, protection::radiator_cooling};
    match stratum {
        Stratum::Topsoil | Stratum::ClayBeds | Stratum::GreyStone | Stratum::SlateShelves => 0,
        Stratum::BasaltColumns => crate::protection::HEAT_UNITS_PER_LEVEL,
        _ => {
            let descent = stratum.descent();
            let index = u8::try_from(descent.index()).unwrap_or(7);
            let low = radiator_cooling(core_tier_cap(index.saturating_sub(1)));
            let high = radiator_cooling(core_tier_cap(index))
                .saturating_sub(2)
                .max(low);
            let strata: Vec<Stratum> = descent.strata().collect();
            let position = strata.iter().position(|each| *each == stratum).unwrap_or(0);
            let last = strata.len().saturating_sub(1).max(1);
            let span = u32::from(high - low);
            let step =
                span * u32::try_from(position).unwrap_or(0) / u32::try_from(last).unwrap_or(1);
            low.saturating_add(u8::try_from(step).unwrap_or(u8::MAX))
        }
    }
}

// --- Landmarks: the cabin, Gravel Hollow and the Sinkhole ---------------------------------------

const SALT_CABIN: u64 = 0xC705_C001;

/// `(width, height)` of the lost miner's cabin, walls included.
pub const CABIN_SIZE: (i32, i32) = (12, 6);

/// The lost miner's cabin of a V11 world: buried in the Topsoil a short way from the spawn,
/// on the side away from the outposts' cable.
#[must_use]
pub fn cabin(settings: &WorldSettings) -> Option<TileRect> {
    if !crust_rules(settings) {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let (top, bottom) = layout.rows(Stratum::Topsoil);
    let roll = hash2(settings.seed, SALT_CABIN, 0, 0);
    let cable = crate::worldgen::descent::landmarks::outpost_column(settings);
    let side = if cable >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    };
    let along = 34 + i32::try_from((roll >> 8) % 40).unwrap_or(0);
    let center = crate::worldgen::SPAWN_TILE_X + side * along;
    let depth = 30 + i32::try_from((roll >> 20) % 24).unwrap_or(0);
    let y = (top + depth).min(bottom - CABIN_SIZE.1 - 8);
    let x = center - CABIN_SIZE.0 / 2;
    Some(TileRect {
        min: TilePosition { x, y },
        max: TilePosition {
            x: x + CABIN_SIZE.0 - 1,
            y: y + CABIN_SIZE.1 - 1,
        },
    })
}

/// The name of the Descent I outpost and the village around it.
pub const GRAVEL_HOLLOW_NAME: &str = "Gravel Hollow";

/// Gravel Hollow's settlement hall: its centre column and first air row.
///
/// The hall is [`crate::village::placement::HALL_WIDTH`] wide and
/// [`crate::village::placement::HALL_AIR_ROWS`] tall, beside the Descent I outpost on the
/// spawn side, sharing its floor. `None` before V11.
#[must_use]
pub fn gravel_hollow(settings: &WorldSettings) -> Option<(i32, i32)> {
    use crate::village::placement::{HALL_AIR_ROWS, HALL_WIDTH};
    use crate::worldgen::descent::landmarks::{OUTPOST_SIZE, outpost, outpost_column};
    if !crust_rules(settings) {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let hall = outpost(settings, &layout, Descent::Crust)?;
    let floor = hall.bounds.max.y;
    let column = outpost_column(settings);
    let side = if column >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    };
    // Clear of the outpost's door by a short gravel lane.
    let center = column + side * (OUTPOST_SIZE.0 / 2 + 4 + HALL_WIDTH / 2);
    Some((center, floor - HALL_AIR_ROWS))
}

/// The Sinkhole of a V11 world: the core chamber at the end of Descent I (the Descent's gate
/// chamber), the funnel shaft over it and the collapse zone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinkholeSite {
    /// The core chamber (walls included): Descent I's gate chamber.
    pub chamber: TileRect,
    /// Where the core sits (on the chamber floor, mid-room).
    pub core: TilePosition,
    /// The row a rig must climb above (or the zone's side it must leave) to be clear.
    pub lip_row: i32,
    /// Everything the collapse may fill: the chamber, the funnel and a margin either side.
    pub zone: TileRect,
}

/// Rows from the chamber's floor up to the Sinkhole's lip.
pub const SINKHOLE_DEPTH_ROWS: i32 = 64;
/// Columns either side of the chamber the collapse zone reaches.
pub const SINKHOLE_ZONE_MARGIN: i32 = 10;

/// The Sinkhole of `settings` (`None` before V11).
#[must_use]
pub fn sinkhole_site(settings: &WorldSettings) -> Option<SinkholeSite> {
    if !crust_rules(settings) {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let chamber =
        crate::worldgen::descent::landmarks::gate_chamber(settings, &layout, Descent::Crust).bounds;
    let floor = chamber.max.y;
    let core = TilePosition {
        x: i32::midpoint(chamber.min.x, chamber.max.x),
        y: floor - 1,
    };
    let lip_row = floor - SINKHOLE_DEPTH_ROWS;
    Some(SinkholeSite {
        chamber,
        core,
        lip_row,
        zone: TileRect {
            min: TilePosition {
                x: chamber.min.x - SINKHOLE_ZONE_MARGIN,
                y: lip_row,
            },
            max: TilePosition {
                x: chamber.max.x + SINKHOLE_ZONE_MARGIN,
                y: floor - 1,
            },
        },
    })
}

/// Half width of the Sinkhole's funnel `rows_up` rows over the chamber's ceiling: wide at the
/// lip, narrowing to the chamber's throat.
#[must_use]
pub const fn funnel_half_width(rows_up: i32) -> i32 {
    3 + rows_up / 6
}

/// Rows a second the collapse front climbs (a stock rig climbs about 7).
pub const SINKHOLE_FRONT_ROWS_PER_SECOND: u32 = 3;
/// Warning before the front starts to climb once the core is out.
pub const SINKHOLE_RUMBLE_MILLIS: u64 = 3_000;
/// Hull share a rig caught by the collapse loses (it is dug out and set on the lip).
pub const SINKHOLE_CAUGHT_HULL_SHARE: f32 = 0.3;
/// The drill tier Drill II sets a rig to (at least).
pub const DRILL_II_LEVEL: u8 = 7;

/// A Sinkhole collapse in progress.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SinkholeRun {
    /// Who pulled the core out.
    pub started_by: PlayerId,
    /// The tick the core came out.
    pub started: SimulationTick,
    /// The deepest open row the collapse has not filled yet (it climbs).
    pub front_row: i32,
    /// Rigs that were in the zone when it began (each must get clear).
    pub inside: BTreeSet<PlayerId>,
    /// Rigs that got clear.
    pub escaped: BTreeSet<PlayerId>,
    /// Rigs the collapse caught.
    pub caught: BTreeSet<PlayerId>,
}

// --- Story --------------------------------------------------------------------------------------

/// The Crust's expedition logs (the Fathers' expedition thread, Descent I: "the world was
/// shaped"), one per strongbox kind, in the order a dig finds them.
pub const CRUST_LOGS: [(&str, &str); 6] = [
    (
        "The cabin",
        "Day 1. Hired on with the Fathers' survey. Their foreman says the ground here was \
         poured, not grown. I said ground is ground. He laughed and gave me a lamp.",
    ),
    (
        "The clay",
        "The clay holds water like a jar. Somebody shaped these beds, layer on layer, and left \
         pots in them for us to find. The Fathers took every pot north to be read.",
    ),
    (
        "The works",
        "We follow the old rails down. Older than any company. The timbers are new, ours; the \
         rails are not. Gravel Hollow feeds us and asks no questions.",
    ),
    (
        "The shelves",
        "Slate in sheets, flat as a table and stacked to the dark. Laid down. Poured. I am \
         starting to believe the foreman.",
    ),
    (
        "The tubes",
        "The tubes run straight for a mile and then stop, as if whatever melted them had \
         somewhere better to be. Hot down here. The Fathers say hotter below.",
    ),
    (
        "The Sinkhole",
        "Found it. A stone that hums, set in the floor like a cork. The foreman says it holds \
         the crust up over the deep. We pull it at dawn. If you read this: run.",
    ),
];

/// The log a strongbox at `tile` holds: the stratum it sits in picks the entry.
#[must_use]
pub fn log_for(settings: &WorldSettings, tile: TilePosition) -> u8 {
    let stratum = crate::worldgen::descent::stratum_at(settings, tile.x, tile.y);
    match stratum {
        Stratum::Topsoil => 0,
        Stratum::ClayBeds => 1,
        Stratum::GreyStone => 2,
        Stratum::SlateShelves => 3,
        Stratum::BasaltColumns
            if sinkhole_site(settings).is_some_and(|site| site.zone.expanded(4).contains(tile)) =>
        {
            5
        }
        _ => 4,
    }
}

/// Credits a strongbox pays out.
pub const STRONGBOX_CREDITS: u32 = 120;

// --- World state --------------------------------------------------------------------------------

/// The Crust's world state (Deep Descent worlds; empty and unhashed elsewhere).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CrustState {
    /// Timber sets that are failing: the beam tile and the tick the roof comes down.
    #[serde(default)]
    pub collapses: BTreeMap<TilePosition, SimulationTick>,
    /// Slate shelves left hanging: a shelf tile and the tick it slides.
    #[serde(default)]
    pub slides: BTreeMap<TilePosition, SimulationTick>,
    /// The Sinkhole collapse in progress.
    #[serde(default)]
    pub sinkhole: Option<SinkholeRun>,
    /// Whether the crew beat the Sinkhole (its gate is passed: the Deep Claim).
    #[serde(default)]
    pub deep_claim: bool,
    /// Rigs that already got their Drill II.
    #[serde(default)]
    pub drill_ii: BTreeSet<PlayerId>,
    /// Expedition logs the crew has read ([`CRUST_LOGS`] indices).
    #[serde(default)]
    pub logs: BTreeSet<u8>,
    /// Rigs warned about the missing Deep Claim (one warning per trip below).
    #[serde(default)]
    pub claim_warned: BTreeSet<PlayerId>,
    /// Rigs currently in a lava tube (for the "into the heat" cue).
    #[serde(default)]
    pub in_tube: BTreeSet<PlayerId>,
}

impl CrustState {
    /// Whether nothing has happened yet (left out of the state hash then).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.collapses.is_empty()
            && self.slides.is_empty()
            && self.sinkhole.is_none()
            && !self.deep_claim
            && self.drill_ii.is_empty()
            && self.logs.is_empty()
            && self.claim_warned.is_empty()
            && self.in_tube.is_empty()
    }

    /// Forgets a player who left.
    pub fn forget(&mut self, player: PlayerId) {
        self.claim_warned.remove(&player);
        self.in_tube.remove(&player);
        if let Some(run) = &mut self.sinkhole {
            run.inside.remove(&player);
        }
    }
}

/// Something a Crust creature did that the game cues.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CrustCreatureAction {
    /// A timber beetle chewed through a timber set.
    BeetleChewed,
    /// A mud leech latched on and drinks fuel.
    LeechLatched,
    /// A mine bat colony took wing.
    BatsRoused,
    /// A burrow mole broke out of the ground.
    MoleSurfaced,
    /// A slate skink shed its tail and fled.
    SkinkFled,
    /// A tube crawler spat a cinder.
    CrawlerSpat,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(tiles: &[(i32, i32)]) -> impl Fn(TilePosition) -> bool + '_ {
        move |at| tiles.contains(&(at.x, at.y))
    }

    #[test]
    fn codes_round_trip() {
        for tile in CrustTile::ALL {
            assert_eq!(CrustTile::from_code(tile.code()), Some(tile));
        }
    }

    #[test]
    fn rails_follow_ramps_and_junctions() {
        let rails = [(0, 5), (1, 5), (2, 5), (2, 6), (3, 6), (3, 5)];
        let is_rail = line(&rails);
        let from = TilePosition { x: 1, y: 5 };
        assert_eq!(
            next_rail(&is_rail, from, 1, false),
            Some(TilePosition { x: 2, y: 5 })
        );
        assert_eq!(
            next_rail(&is_rail, from, 1, true),
            Some(TilePosition { x: 2, y: 6 })
        );
        assert!(is_junction(&is_rail, from, 1));
        assert!(!is_junction(&is_rail, TilePosition { x: 0, y: 5 }, -1));
        assert_eq!(
            next_rail(&is_rail, TilePosition { x: 0, y: 5 }, -1, false),
            None
        );
    }

    #[test]
    fn a_cart_gathers_speed_downhill_and_stops_at_the_end() {
        // A level run, a long ramp down, then the end of the line.
        let mut rails = vec![(0, 0), (1, 0), (2, 0)];
        for step in 0..8 {
            rails.push((3 + step, 1 + step));
        }
        let is_rail = line(&rails);
        let mut ride = CartRide {
            rail: TilePosition { x: 0, y: 0 },
            next: TilePosition { x: 1, y: 0 },
            dir: 1,
            progress: 0,
            speed: CART_BOARD_SPEED,
        };
        let mut top = 0;
        let mut ended = None;
        for _ in 0..CART_STEPS_PER_SECOND * 20 {
            match step_cart(&mut ride, CartStick::default(), &is_rail) {
                CartStep::Ended { velocity } => {
                    ended = Some(velocity);
                    break;
                }
                CartStep::Rolled | CartStep::Advanced { .. } => {}
            }
            top = top.max(ride.speed);
        }
        assert!(top > CART_BOARD_SPEED * 2, "the ramp speeds it up: {top}");
        assert!(ended.is_some_and(|velocity| velocity > 0));
        assert_eq!(ride.rail, TilePosition { x: 10, y: 8 });
    }

    #[test]
    fn braking_turns_a_cart_round() {
        let rails: Vec<(i32, i32)> = (0..20).map(|x| (x, 0)).collect();
        let is_rail = line(&rails);
        let mut ride = CartRide {
            rail: TilePosition { x: 10, y: 0 },
            next: TilePosition { x: 11, y: 0 },
            dir: 1,
            progress: 0,
            speed: 3_000,
        };
        let back = CartStick {
            horizontal: -1,
            down: false,
        };
        for _ in 0..CART_STEPS_PER_SECOND * 2 {
            let _ = step_cart(&mut ride, back, &is_rail);
        }
        assert_eq!(ride.dir, -1);
        assert!(ride.speed > 0);
    }

    #[test]
    fn heat_is_gate_appropriate() {
        use crate::{depth_progression::core_tier_cap, protection::radiator_cooling};
        assert_eq!(stratum_heat_units(Stratum::Topsoil), 0);
        assert_eq!(stratum_heat_units(Stratum::SlateShelves), 0);
        // One radiator tier covers the Basalt Columns.
        assert!(stratum_heat_units(Stratum::BasaltColumns) <= radiator_cooling(2));
        for descent in Descent::ALL.into_iter().skip(1) {
            let index = u8::try_from(descent.index()).unwrap_or(7);
            let first = descent.first_stratum();
            assert!(
                stratum_heat_units(first) <= radiator_cooling(core_tier_cap(index - 1)),
                "{first:?} is cool for a rig fitted to the Descent before"
            );
            for stratum in descent.strata() {
                assert!(
                    stratum_heat_units(stratum) <= radiator_cooling(core_tier_cap(index)),
                    "{stratum:?} is covered by its own Descent's cap"
                );
            }
        }
    }
}
