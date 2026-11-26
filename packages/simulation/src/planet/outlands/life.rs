//! What it is like to be on an outland world (Wave 13 planets-c).
//!
//! Each stratum's signature rule and hazards, the creatures and where they live, the rhythms
//! of world time the rules keep (the gear works, the spore vents, the ghost lights) and the
//! events. Pure data and pure functions of the body and the world time; `world/outlands.rs`
//! plays them.

use super::{PlanetGround, PlanetThemeId, oroll};
use crate::{
    bodylife::WeatherId,
    enemy::EnemyKind,
    planet::wilds::{PlanetHazard, life::Habitat, wrap_delta},
};

/// A stratum's signature rule (the one rule that changes how you play there).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OutlandRule {
    /// Dug mycelium grows back into the tunnel once the rigs move on.
    Regrowth,
    /// A drilled glowcap flashes and stuns the hunters round it.
    Glowflash,
    /// Spore vents swell, then blast a rig up their chimneys.
    SporeVents,
    /// Conveyor belts carry what rests on them.
    Conveyors,
    /// Gears turn on the works' rhythm and grind what touches them.
    GrindingGears,
    /// Three relays in a vault's roof: light them all and its seals open.
    PowerRelays,
    /// Loose ash pours into open space under it.
    Ashfall,
    /// Break a tomb's seal and its wardens wake.
    TombWardens,
    /// Ghost-light wisps drift through the rock and chill the hull.
    GhostLights,
}

impl OutlandRule {
    /// One line for the HUD and the docs.
    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Regrowth => "The mycelium grows back into your tunnels: keep moving",
            Self::Glowflash => "Drill a glowcap and its flash stuns the hunters round it",
            Self::SporeVents => "Spore vents swell, then blast you up their chimneys",
            Self::Conveyors => "Conveyor belts carry you along: drill a belt to stop it",
            Self::GrindingGears => "The gears turn after a clank: slip past while they rest",
            Self::PowerRelays => {
                "Light a vault's three relays to open it (each flips its neighbours)"
            }
            Self::Ashfall => "Loose ash pours into your shaft behind you",
            Self::TombWardens => "Break a tomb's seal and its wardens wake",
            Self::GhostLights => "Ghost lights drift through the rock: their touch chills the hull",
        }
    }
}

/// An outland stratum's play rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutlandStratum {
    pub rule: OutlandRule,
    /// Hazards in force here (the planet gear and the Descent's systems counter them).
    pub hazards: &'static [PlanetHazard],
}

/// One of a theme's native creatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutlandCreature {
    pub kind: EnemyKind,
    pub habitat: Habitat,
    /// Spawn weight among the creatures whose habitat fits.
    pub weight: u32,
    /// How many come together (min, max).
    pub pack: (u8, u8),
    /// Neutral fauna: never attacks first (fights back when struck).
    pub neutral: bool,
}

/// An outland theme's life (see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutlandTheme {
    pub theme: PlanetThemeId,
    /// Hazards on the open surface.
    pub surface_hazards: &'static [PlanetHazard],
    pub strata: [OutlandStratum; 3],
    pub creatures: &'static [OutlandCreature],
    /// The theme's storm (a registered bodylife weather kind, [`super::hooks`]).
    pub storm: WeatherId,
}

const fn creature(
    kind: EnemyKind,
    habitat: Habitat,
    weight: u32,
    pack: (u8, u8),
    neutral: bool,
) -> OutlandCreature {
    OutlandCreature {
        kind,
        habitat,
        weight,
        pack,
        neutral,
    }
}

const FUNGAL_CREATURES: [OutlandCreature; 4] = [
    creature(EnemyKind::Capgrazer, Habitat::Surface, 5, (1, 3), true),
    creature(EnemyKind::Lanternfly, Habitat::CaveAir(1), 5, (2, 4), true),
    creature(EnemyKind::Sporeling, Habitat::Surface, 4, (2, 4), false),
    creature(EnemyKind::Rotmaw, Habitat::Caves(2), 4, (1, 1), false),
];

const MACHINE_CREATURES: [OutlandCreature; 4] = [
    creature(EnemyKind::Scuttlebot, Habitat::Surface, 5, (1, 3), true),
    creature(EnemyKind::WardDrone, Habitat::Surface, 3, (1, 2), false),
    creature(EnemyKind::Cogspider, Habitat::Caves(1), 4, (1, 2), false),
    creature(EnemyKind::ArcMite, Habitat::CaveAir(2), 4, (2, 4), false),
];

const DEAD_CREATURES: [OutlandCreature; 4] = [
    creature(EnemyKind::Ashrunner, Habitat::Surface, 5, (2, 3), true),
    creature(EnemyKind::Bonepicker, Habitat::Surface, 4, (2, 3), false),
    creature(EnemyKind::Gravewarden, Habitat::Caves(1), 3, (1, 1), false),
    creature(EnemyKind::Shade, Habitat::CaveAir(2), 4, (1, 2), false),
];

/// The outland themes' life, by theme.
pub const OUTLAND_LIFE: [OutlandTheme; 3] = [
    OutlandTheme {
        theme: PlanetThemeId::FUNGAL,
        surface_hazards: &[],
        strata: [
            OutlandStratum {
                rule: OutlandRule::Regrowth,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::Glowflash,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::SporeVents,
                hazards: &[PlanetHazard::ToxicAir],
            },
        ],
        creatures: &FUNGAL_CREATURES,
        storm: super::hooks::SPORE_DRIFT,
    },
    OutlandTheme {
        theme: PlanetThemeId::MACHINE,
        surface_hazards: &[],
        strata: [
            OutlandStratum {
                rule: OutlandRule::Conveyors,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::GrindingGears,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::PowerRelays,
                hazards: &[PlanetHazard::Heat],
            },
        ],
        creatures: &MACHINE_CREATURES,
        storm: super::hooks::ION_STORM,
    },
    OutlandTheme {
        theme: PlanetThemeId::DEAD,
        surface_hazards: &[],
        strata: [
            OutlandStratum {
                rule: OutlandRule::Ashfall,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::TombWardens,
                hazards: &[],
            },
            OutlandStratum {
                rule: OutlandRule::GhostLights,
                hazards: &[PlanetHazard::Cold],
            },
        ],
        creatures: &DEAD_CREATURES,
        storm: super::hooks::GREY_WIND,
    },
];

/// The life of outland theme `theme` (`None` for other themes).
#[must_use]
pub fn outland_theme(theme: PlanetThemeId) -> Option<&'static OutlandTheme> {
    OUTLAND_LIFE.iter().find(|life| life.theme == theme)
}

/// The signature rule of the stratum at `(column, row)`, if it is an outland stratum.
#[must_use]
pub fn rule_at(ground: &PlanetGround, column: i32, row: i32) -> Option<OutlandRule> {
    let life = outland_theme(ground.params().theme)?;
    if row < crate::planet::BodyGround::surface_row(ground, column)
        || row >= ground.core_chamber_top()
    {
        return None;
    }
    Some(life.strata[ground.stratum_index(column, row).min(2)].rule)
}

/// The hazards in force at strip tile `(column, row)` (without weather).
#[must_use]
pub fn hazards_at(ground: &PlanetGround, column: i32, row: i32) -> &'static [PlanetHazard] {
    let Some(life) = outland_theme(ground.params().theme) else {
        return &[];
    };
    if row < crate::planet::BodyGround::surface_row(ground, column) {
        return life.surface_hazards;
    }
    if row >= ground.core_chamber_top() {
        return &[];
    }
    life.strata[ground.stratum_index(column, row).min(2)].hazards
}

/// A deterministic roll for life on `ground` (spawns, regrowth).
#[must_use]
pub const fn life_roll(ground: &PlanetGround, purpose: u64, a: i64, b: i64) -> u64 {
    oroll(ground, 0x1000 + purpose, a, b)
}

// --- The gear works' rhythm ------------------------------------------------------------------

/// One turn of the gear works: a clank, the turning, then the rest (milliseconds).
pub const GEAR_CYCLE_MILLIS: u64 = 8_000;
/// The clank that warns of the turning.
pub const GEAR_CLANK_MILLIS: u64 = 1_000;
/// How long the gears turn after the clank.
pub const GEAR_TURN_MILLIS: u64 = 4_500;

/// Where the gear works are in their cycle at world time `millis`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GearPhase {
    /// Still, the clank sounding: they turn next.
    Clank,
    Turning,
    Resting,
}

/// The gear works' phase at world time `millis` (one rhythm for a whole body).
#[must_use]
pub const fn gear_phase(millis: u64) -> GearPhase {
    let t = millis % GEAR_CYCLE_MILLIS;
    if t < GEAR_CLANK_MILLIS {
        GearPhase::Clank
    } else if t < GEAR_CLANK_MILLIS + GEAR_TURN_MILLIS {
        GearPhase::Turning
    } else {
        GearPhase::Resting
    }
}

// --- The spore vents' breath ------------------------------------------------------------------

/// One breath of a spore vent: it rests, swells, then blasts (milliseconds).
pub const VENT_CYCLE_MILLIS: u64 = 9_000;
pub const VENT_SWELL_MILLIS: u64 = 2_000;
pub const VENT_BLAST_MILLIS: u64 = 2_500;

/// A spore vent's phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VentPhase {
    Resting,
    /// It swells and hisses: the blast comes next.
    Swelling,
    Blasting,
}

/// The phase of the vent at strip column `column` at world time `millis` (each vent keeps its
/// own offset in the cycle).
#[must_use]
pub const fn vent_phase(ground: &PlanetGround, column: i64, millis: u64) -> VentPhase {
    let offset = life_roll(ground, 40, column, 0) % VENT_CYCLE_MILLIS;
    let t = (millis + offset) % VENT_CYCLE_MILLIS;
    let rest = VENT_CYCLE_MILLIS - VENT_SWELL_MILLIS - VENT_BLAST_MILLIS;
    if t < rest {
        VentPhase::Resting
    } else if t < rest + VENT_SWELL_MILLIS {
        VentPhase::Swelling
    } else {
        VentPhase::Blasting
    }
}

// --- Ghost lights ------------------------------------------------------------------------------

/// Columns and rows per wisp cell in the Grave Mantle.
pub const WISP_CELL: (i64, i64) = (40, 28);
/// How long a wisp takes to cross its cell and back, across and down (milliseconds).
const WISP_PERIODS: (u64, u64) = (23_000, 17_000);

/// A ghost-light wisp at a moment: its centre in thousandths of a tile (strip column, row).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Wisp {
    pub x_milli: i64,
    pub y_milli: i64,
    /// Which cell's wisp it is (stable across time: the renderer's identity).
    pub id: u64,
}

impl Wisp {
    /// Its centre in tiles (strip column, row).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
    pub fn tiles(&self) -> (f32, f32) {
        (self.x_milli as f32 / 1_000.0, self.y_milli as f32 / 1_000.0)
    }
}

/// A triangle wave in `0..=1000` with period `period` at time `t`.
const fn triangle(t: u64, period: u64) -> i64 {
    let phase = (t % period) * 2_000 / period;
    let value = if phase > 1_000 { 2_000 - phase } else { phase };
    value.cast_signed()
}

/// The ghost-light wisps of the Grave Mantle near strip tile `(column, row)` at `millis`.
///
/// The cell's and its neighbours': pure functions of the body and the world time, so the
/// rules and the renderer agree.
#[must_use]
pub fn wisps_near(ground: &PlanetGround, column: i64, row: i32, millis: u64) -> Vec<Wisp> {
    if ground.params().theme != PlanetThemeId::DEAD {
        return Vec::new();
    }
    let width = i64::from(ground.strip_width());
    let cells = (width / WISP_CELL.0).max(1);
    let cx = column.rem_euclid(width).div_euclid(WISP_CELL.0);
    let cy = i64::from(row).div_euclid(WISP_CELL.1);
    let mut out = Vec::new();
    for ox in -1..=1 {
        for oy in -1..=1 {
            let cell_x = (cx + ox).rem_euclid(cells);
            let cell_y = cy + oy;
            let hash = life_roll(ground, 50, cell_x, cell_y);
            if hash.is_multiple_of(3) {
                continue;
            }
            let shift = hash >> 8;
            let tx = triangle(
                millis + shift % WISP_PERIODS.0,
                WISP_PERIODS.0 + (shift >> 20) % 4_000,
            );
            let ty = triangle(
                millis + (shift >> 12) % WISP_PERIODS.1,
                WISP_PERIODS.1 + (shift >> 32) % 3_000,
            );
            let x_milli = cell_x * WISP_CELL.0 * 1_000 + 4_000 + tx * (WISP_CELL.0 - 8);
            let y_milli = cell_y * WISP_CELL.1 * 1_000 + 3_000 + ty * (WISP_CELL.1 - 6);
            // Only in the Grave Mantle (its band, not the strata above or the core).
            let wisp_column = i32::try_from(x_milli.div_euclid(1_000)).unwrap_or(0);
            let wisp_row = i32::try_from(y_milli.div_euclid(1_000)).unwrap_or(0);
            if rule_at(ground, wisp_column, wisp_row) != Some(OutlandRule::GhostLights) {
                continue;
            }
            // Folded to the near side of the strip's seam.
            let near = column + wrap_delta(x_milli.div_euclid(1_000), column, width);
            out.push(Wisp {
                x_milli: near * 1_000 + x_milli.rem_euclid(1_000),
                y_milli,
                id: hash,
            });
        }
    }
    out
}

// --- Events -----------------------------------------------------------------------------------

/// Something an outland world did (netcode codes 1000-1019).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum OutlandsEvent {
    /// A mushroom cap threw a rig back up.
    CapBounce,
    /// The mycelium grew back into a tunnel round a rig.
    MyceliumRegrew,
    /// A drilled glowcap flashed (the hunters round it are stunned).
    GlowFlash,
    /// A spore vent swells under a rig (it blasts next).
    VentSwelling,
    /// A spore vent blasted a rig up its chimney.
    VentBlast,
    /// A puffball burst into a spore cloud.
    PuffballBurst,
    /// The gear works clanked (the gears turn next).
    GearsClank,
    /// A turning gear ground a rig's hull.
    GearGrind,
    /// A power relay flipped.
    RelayFlipped,
    /// A power vault's seals opened.
    VaultOpened,
    /// Loose ash poured into the open round a rig.
    AshPoured,
    /// A tomb's seal broke and its wardens woke.
    TombOpened,
    /// A ghost light's touch chilled a hull.
    WispChill,
    /// A hazard bit a hull (spore air, reactor heat).
    HazardBite,
    /// A relic, a builders' cache or a treasure was dug out.
    RelicFound,
    /// A small body's secret cache was dug out.
    SecretCache,
    /// A theme boss woke in its lair.
    BossWoke,
    /// A theme boss is calling its brood (they come next).
    BossSummons,
    /// A theme boss fell: its seal is spent and its vault open.
    BossDefeated,
    /// The theme's storm is rising.
    StormRising,
}

impl OutlandsEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 20] = [
        Self::CapBounce,
        Self::MyceliumRegrew,
        Self::GlowFlash,
        Self::VentSwelling,
        Self::VentBlast,
        Self::PuffballBurst,
        Self::GearsClank,
        Self::GearGrind,
        Self::RelayFlipped,
        Self::VaultOpened,
        Self::AshPoured,
        Self::TombOpened,
        Self::WispChill,
        Self::HazardBite,
        Self::RelicFound,
        Self::SecretCache,
        Self::BossWoke,
        Self::BossSummons,
        Self::BossDefeated,
        Self::StormRising,
    ];

    /// Offset of the event's netcode code from 1000.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// The event with offset `code`.
    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        if (code as usize) < Self::ALL.len() {
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }
}
