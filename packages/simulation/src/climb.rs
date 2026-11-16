//! The climb tables: how high a rig can fly, measured by the climb harness (expansion plan F6).
//!
//! Fuel is the key to the sky. Every altitude band of a sky ([`crate::worldgen::sky::SkyBands`])
//! is defined as a fraction of a climb table ([`crate::worldgen::sky::SkyBands::calibrated`]),
//! so fuel tank tiers unlock the sky in order. There are two tables:
//!
//! - [`CLIMB_CEILINGS`]: the current fuel curve ([`crate::player::fuel_capacity_for_level`]).
//!   The shop's sky gates read it ([`reach`], `milestone::climb_reach`), and the next sky
//!   generator's bands are calibrated from it ([`crate::worldgen::sky::NEXT_SKY_BANDS`]).
//! - [`V8_CLIMB_CEILINGS`]: frozen. Measured on the first fifteen-tier fuel curve
//!   ([`crate::player::fuel_capacity_v2`]); V8 worlds' bands were calibrated from it and are
//!   frozen as [`crate::worldgen::sky::V8_SKY_BANDS`] (a test checks the two agree).
//!
//! Rebalancing fuel tanks, engines or the altitude physics: re-run the harness report
//!
//! ```text
//! cargo test --release -p fatherload_simulation climb_table_report -- --ignored --nocapture
//! ```
//!
//! and paste its table into [`CLIMB_CEILINGS`] (that moves the gates, and the bands of the next
//! sky generator, so once a generator using it ships, freeze its bands like V8's). The regular
//! test `world::climb_tests::climb_table_matches_the_simulation` re-measures a sample of
//! entries and fails when the table and the simulation drift apart.
//!
//! **What is measured:** a rig with a full tank of fuel tank tier `t`, engine tier `e` and
//! altitude intake tier `i` (an empty bay, no rig parts) starts at rest 100 rows above row 0
//! (above every peak) in still air
//! (standard gravity, the altitude air-density profile without the vacuum of orbit, no wind, no
//! updrafts, no islands) and holds thrust until the tank is dry; the entry is the highest
//! altitude it reaches, in rows above row 0, rounded down to a multiple of 10.

/// The altitude (rows above row 0) every measured climb starts from, at rest.
pub const START_ALTITUDE_ROWS: i32 = 100;

/// The fuel tank tiers in the table (every tier).
pub const TANK_TIERS: [u8; 15] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
/// The engine tiers in the table.
pub const ENGINE_TIERS: [u8; 5] = [1, 4, 8, 12, 15];
/// The altitude intake tiers in the table.
pub const INTAKE_TIERS: [u8; 4] = [0, 2, 4, 6];

/// A climb table: measured ceilings `[tank][engine][intake]`, indexed like [`TANK_TIERS`],
/// [`ENGINE_TIERS`] and [`INTAKE_TIERS`], in rows above row 0.
pub type ClimbTable = [[[i32; 4]; 5]; 15];

/// Measured ceilings on the current fuel curve, `[tank][engine][intake]` indexed like
/// [`TANK_TIERS`], [`ENGINE_TIERS`] and [`INTAKE_TIERS`], in rows above row 0.
///
/// Above a few thousand rows only altitude intakes keep a rig climbing (without them every
/// tank stalls where thin air leaves the engine too weak to lift it).
#[rustfmt::skip]
pub const CLIMB_CEILINGS: ClimbTable = [
    [[430, 450, 480, 520], [590, 650, 710, 790], [740, 830, 920, 1060], [790, 880, 990, 1140], [790, 880, 990, 1140]], // tank 1
    [[550, 600, 650, 720], [780, 870, 980, 1130], [990, 1130, 1290, 1530], [1050, 1210, 1390, 1650], [1050, 1210, 1390, 1650]], // tank 2
    [[720, 800, 890, 1020], [1030, 1190, 1360, 1620], [1320, 1550, 1820, 2220], [1410, 1660, 1970, 2410], [1410, 1660, 1970, 2410]], // tank 3
    [[860, 1050, 1200, 1410], [1350, 1590, 1870, 2290], [1740, 2100, 2530, 3170], [1860, 2260, 2730, 3440], [1860, 2260, 2730, 3440]], // tank 4
    [[930, 1280, 1590, 1910], [1730, 2090, 2520, 3150], [2250, 2790, 3440, 4400], [2410, 3000, 3720, 4800], [2410, 3000, 3720, 4800]], // tank 5
    [[950, 1400, 2060, 2540], [2010, 2710, 3330, 4250], [2860, 3640, 4580, 5990], [3070, 3930, 4970, 6540], [3070, 3930, 4970, 6540]], // tank 6
    [[960, 1460, 2530, 3330], [2090, 3460, 4330, 5640], [3240, 4690, 6020, 8000], [3870, 5080, 6540, 8740], [3870, 5080, 6540, 8740]], // tank 7
    [[960, 1480, 2860, 4320], [2100, 4370, 5580, 7380], [3300, 5990, 7800, 10540], [4370, 6490, 8500, 11530], [4820, 6490, 8500, 11530]], // tank 8
    [[960, 1490, 3070, 5540], [2100, 5500, 7130, 9570], [3300, 7580, 10040, 13730], [4410, 8230, 10960, 15040], [5240, 8230, 10960, 15040]], // tank 9
    [[960, 1490, 3190, 7080], [2100, 6770, 9060, 12330], [3300, 9570, 12840, 17760], [4410, 10400, 14030, 19480], [5240, 10400, 14030, 19480]], // tank 10
    [[960, 1490, 3230, 9020], [2100, 7530, 11500, 15830], [3300, 12050, 16390, 22900], [4410, 13130, 17930, 25110], [5240, 13130, 17930, 25110]], // tank 11
    [[960, 1490, 3250, 11490], [2100, 7820, 14600, 20310], [3300, 15210, 20920, 29480], [4410, 16590, 22920, 32340], [5240, 16590, 22920, 32340]], // tank 12
    [[960, 1490, 3250, 14710], [2100, 7900, 18630, 26130], [3300, 19290, 26800, 38060], [4420, 21070, 29420, 41770], [5250, 21070, 29420, 41770]], // tank 13
    [[960, 1490, 3250, 16730], [2100, 7910, 21170, 29830], [3300, 21850, 30520, 43490], [4420, 23900, 33530, 47750], [5250, 23900, 33530, 47750]], // tank 14
    [[960, 1490, 3250, 18910], [2100, 7920, 23880, 33780], [3300, 24590, 34490, 49320], [4420, 26900, 37930, 54140], [5250, 26900, 37930, 54140]], // tank 15
];

/// The climb table V8 skies were calibrated from. Frozen.
///
/// Measured on the first fifteen-tier fuel curve ([`crate::player::fuel_capacity_v2`], 100 to
/// ~3,000 fuel); V8 worlds' bands ([`crate::worldgen::sky::V8_SKY_BANDS`]) must never move.
#[rustfmt::skip]
pub const V8_CLIMB_CEILINGS: ClimbTable = [
    [[210, 210, 210, 210], [280, 290, 290, 300], [340, 360, 370, 390], [360, 380, 390, 410], [360, 380, 390, 410]], // tank 1
    [[260, 270, 270, 270], [350, 360, 380, 400], [430, 450, 480, 510], [450, 480, 510, 550], [450, 480, 510, 550]], // tank 2
    [[320, 330, 340, 360], [440, 460, 490, 530], [540, 580, 630, 700], [570, 620, 670, 750], [570, 620, 670, 750]], // tank 3
    [[400, 420, 440, 460], [540, 590, 640, 700], [680, 750, 830, 940], [720, 800, 890, 1010], [720, 800, 890, 1010]], // tank 4
    [[480, 510, 550, 600], [670, 740, 820, 920], [840, 950, 1070, 1240], [900, 1010, 1150, 1340], [900, 1010, 1150, 1340]], // tank 5
    [[580, 630, 690, 760], [820, 920, 1030, 1200], [1040, 1190, 1370, 1630], [1100, 1270, 1470, 1760], [1100, 1270, 1470, 1760]], // tank 6
    [[690, 770, 850, 960], [990, 1130, 1300, 1530], [1260, 1480, 1730, 2100], [1350, 1580, 1860, 2270], [1350, 1580, 1860, 2270]], // tank 7
    [[810, 930, 1040, 1210], [1190, 1390, 1610, 1940], [1530, 1820, 2170, 2680], [1630, 1950, 2340, 2910], [1630, 1950, 2340, 2910]], // tank 8
    [[880, 1110, 1270, 1500], [1430, 1690, 2000, 2450], [1840, 2230, 2700, 3400], [1960, 2400, 2920, 3700], [1960, 2400, 2920, 3700]], // tank 9
    [[930, 1260, 1550, 1870], [1700, 2050, 2460, 3080], [2210, 2730, 3360, 4300], [2360, 2940, 3640, 4680], [2360, 2940, 3640, 4680]], // tank 10
    [[950, 1370, 1890, 2320], [1940, 2490, 3040, 3860], [2650, 3340, 4170, 5420], [2840, 3600, 4530, 5910], [2840, 3600, 4530, 5910]], // tank 11
    [[960, 1440, 2290, 2880], [2060, 3030, 3760, 4840], [3090, 4090, 5200, 6850], [3420, 4420, 5650, 7480], [3420, 4420, 5650, 7480]], // tank 12
    [[960, 1470, 2640, 3600], [2100, 3710, 4670, 6120], [3280, 5050, 6510, 8690], [4100, 5460, 7080, 9510], [4140, 5460, 7080, 9510]], // tank 13
    [[960, 1480, 2790, 4050], [2100, 4130, 5250, 6920], [3300, 5640, 7330, 9860], [4320, 6110, 7980, 10790], [4580, 6110, 7980, 10790]], // tank 14
    [[960, 1480, 2910, 4530], [2100, 4570, 5850, 7770], [3300, 6270, 8200, 11100], [4390, 6800, 8940, 12150], [4980, 6800, 8940, 12150]], // tank 15
];

/// Index of `tier` in `tiers` (a compile-time error when it is not in the table).
const fn index_of(tiers: &[u8], tier: u8) -> usize {
    let mut index = 0;
    while index < tiers.len() {
        if tiers[index] == tier {
            return index;
        }
        index += 1;
    }
    panic!("tier not in the climb table");
}

/// The measured ceiling (rows above row 0) of a rig with fuel tank tier `tank`, engine tier
/// `engine` and intake tier `intake`; each must be one of the table's tiers.
///
/// # Panics
///
/// Panics (at compile time in const contexts) for a tier the table does not hold.
#[must_use]
pub const fn ceiling(tank: u8, engine: u8, intake: u8) -> i32 {
    ceiling_in(&CLIMB_CEILINGS, tank, engine, intake)
}

/// [`ceiling`] read from `table`.
///
/// # Panics
///
/// Panics (at compile time in const contexts) for a tier the table does not hold.
#[must_use]
pub const fn ceiling_in(table: &ClimbTable, tank: u8, engine: u8, intake: u8) -> i32 {
    table[index_of(&TANK_TIERS, tank)][index_of(&ENGINE_TIERS, engine)]
        [index_of(&INTAKE_TIERS, intake)]
}

/// The table's ceiling for any loadout, in rows above row 0.
///
/// `tank` is clamped to the table, and `engine` and `intake` are rounded **down** to the nearest
/// tier the table measured, so the answer never overstates a climb. The shop's sky gates read
/// it (`milestone::climb_reach`).
#[must_use]
pub fn reach(tank: u8, engine: u8, intake: u8) -> i32 {
    let floor_tier =
        |tiers: &[u8], level: u8| tiers.iter().rposition(|tier| *tier <= level).unwrap_or(0);
    let tank = usize::from(tank.clamp(TANK_TIERS[0], TANK_TIERS[TANK_TIERS.len() - 1]) - 1);
    CLIMB_CEILINGS[tank][floor_tier(&ENGINE_TIERS, engine)][floor_tier(&INTAKE_TIERS, intake)]
}

/// `percent` of `rows`, rounded down to a multiple of 10 rows.
#[must_use]
pub const fn fraction(rows: i32, percent: i32) -> i32 {
    rows * percent / 100 / 10 * 10
}
