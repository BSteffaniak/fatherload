//! The climb table: how high a rig can fly, measured by the climb harness (expansion plan F6).
//!
//! Fuel is the key to the sky. Every altitude band of a V8 sky ([`crate::worldgen::sky`]) is
//! defined as a fraction of these measurements, so rebalancing fuel tanks, engines or the
//! altitude physics moves the islands with them: re-run the harness report
//!
//! ```text
//! cargo test --release -p fatherload_simulation climb_table_report -- --ignored --nocapture
//! ```
//!
//! and paste its table here (that changes what V8 generates, so it ships as a new generator
//! version once V8 worlds exist in the wild). The regular test
//! `world::climb_tests::climb_table_matches_the_simulation` re-measures a sample of entries and
//! fails when the table and the simulation drift apart.
//!
//! **What is measured:** a rig with a full tank of fuel tank tier `t`, engine tier `e` and
//! altitude intake tier `i` (an empty bay, no rig parts) starts at rest 100 rows above row 0
//! (above every peak) in still air
//! (standard gravity, the altitude air-density profile, no wind, no updrafts, no islands) and
//! holds thrust until the tank is dry; the entry is the highest altitude it reaches, in rows
//! above row 0, rounded down to a multiple of 10.

/// The fuel tank tiers in the table (every tier).
pub const TANK_TIERS: [u8; 15] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
/// The engine tiers in the table.
pub const ENGINE_TIERS: [u8; 5] = [1, 4, 8, 12, 15];
/// The altitude intake tiers in the table.
pub const INTAKE_TIERS: [u8; 4] = [0, 2, 4, 6];

/// Measured ceilings, `[tank][engine][intake]` indexed like [`TANK_TIERS`], [`ENGINE_TIERS`]
/// and [`INTAKE_TIERS`], in rows above row 0.
#[rustfmt::skip]
pub const CLIMB_CEILINGS: [[[i32; 4]; 5]; 15] = [
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
    CLIMB_CEILINGS[index_of(&TANK_TIERS, tank)][index_of(&ENGINE_TIERS, engine)]
        [index_of(&INTAKE_TIERS, intake)]
}

/// The table's ceiling for any loadout (rows above row 0): `tank` clamped to the table, and
/// `engine` and `intake` rounded **down** to the nearest tier the table measured, so the answer
/// never overstates a climb. The shop's sky gates read it (`milestone::climb_reach`).
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
