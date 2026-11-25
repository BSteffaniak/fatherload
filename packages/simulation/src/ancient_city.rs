//! The builders' buried city (expansion plan §14, Descent VII strata 1-3; Wave 10 track
//! ancients-a): the Buried Megacity, the Engine Halls and the Archive of Stars.
//!
//! Pure data and math; the tick lives in `world/ancient_city.rs`, the creatures' behaviour in
//! `world/enemies/ancient_city.rs`, the generated places in
//! [`crate::worldgen::descent::ancient_city`], the moving machinery in [`movers`], the
//! companion in [`archivist`] and the star charts in [`crate::star_chart`].
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | Buried Megacity | **a city that still keeps order**: avenues with **trams** that run on a timetable and towers with **lifts** between the avenues ([`movers`]); drilling the city's own fabric (paving, tower glass, lamps) raises the **city alarm** ([`CityAlarm`]), and an alarmed rig is hunted by the city's drones and sentinels | ride the trams and lifts instead of cutting through; let the alarm cool; the **archivist** ([`archivist`]) wakes in the Lantern Library or at the Archive Post and rides with you |
//! | Engine Halls | **the Great Engine's beat**: every piston, crusher and belt runs on one stroke ([`engine_beat_millis`], quicker deeper). **Pistons** lift a rig resting on their heads and fling one that drives off a rising head; **crushers** slam on a telegraphed beat; **belts** carry | count the beat: ride the pistons up (or launch off them), pass the crushers between slams |
//! | The Archive of Stars | **sealed vaults**: unbreakable rooms whose locks are puzzles of the builders' mechanisms (plates, levers up a crawlway, colour-keyed light); each holds a **star chart** ([`crate::star_chart`]), the first coordinates of the sky beyond the Moon | read the lock, solve it, read the chart at its plinth |
//!
//! State: [`AncientCity`] (shared, saved, replicated as the appended `AncientCity` section and
//! hashed once anything happened: rides, alarms, every rig's archivist, the star charts found,
//! vaults opened, landmarks found) and [`CitySchedule`] (host bookkeeping: crusher hits, loot a
//! scrap crawler carries, the alarm's hunts; saved, never hashed or replicated).

pub mod archivist;
pub mod movers;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    descent::Stratum,
    ids::{PlayerId, SimulationTick},
    item::{ItemKind, ItemStack},
    star_chart::StarChart,
    terrain::{MineralKind, TilePosition},
};

pub use archivist::Archivist;
pub use movers::{CityRide, MoverKind, MoverRef};

/// Kind-code range of [`crate::terrain::TileKind::City`] tiles in
/// [`crate::terrain::Tile::code`] (`0x1A00 | CityTile::code`), clear of every other family
/// (Wave 10 ancients-a).
pub const CITY_TILE_CODE_BASE: u64 = 0x1A00;

/// A tile of the builders' buried city ([`crate::terrain::TileKind::City`], V11 worlds only).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CityTile {
    // --- Buried Megacity ---
    /// Builder paving and masonry: the avenues' floors and the towers' slabs.
    CityStone,
    /// A tower's window glass (lit from inside; it shatters easily, and loudly).
    TowerGlass,
    /// A tram line's rail bed (unbreakable: the line holds).
    TramRail,
    /// A tram station's platform (unbreakable).
    Platform,
    /// A street lamp hanging from an avenue's ceiling (it still burns).
    StreetLamp,
    /// Ancient alloy ore in the city's footings (the Megacity's layer resource).
    AlloyOre,
    /// A lift shaft's guide rail (unbreakable).
    LiftRail,
    /// The archivist's dormant core (the Lantern Library and the Archive Post; unbreakable).
    ArchivistCore,
    // --- Engine Halls ---
    /// Engine plating: the halls' walls and floors.
    EngineSteel,
    /// A belt running left.
    ConveyorLeft,
    /// A belt running right.
    ConveyorRight,
    /// A piston's housing under its head, or a crusher's over it (unbreakable).
    PistonHousing,
    /// Engine core ore (the Engine Halls' layer resource).
    EngineCoreOre,
    /// A coolant pipe through the rock.
    Pipe,
    // --- The Archive of Stars ---
    /// Archive shelving: the stacks (it crumbles, paper and wire).
    ArchiveShelf,
    /// Archive masonry, deep blue and starred.
    ArchiveStone,
    /// Star glass: the orrery's lenses and the Archive's layer resource.
    StarGlass,
    /// A star chart's plinth, unread (unbreakable; a rig or pilot beside it reads the chart).
    ChartPlinth,
    /// A plinth whose chart has been read.
    ChartPlinthRead,
}

impl CityTile {
    /// Every city tile, in code order.
    pub const ALL: [Self; 19] = [
        Self::CityStone,
        Self::TowerGlass,
        Self::TramRail,
        Self::Platform,
        Self::StreetLamp,
        Self::AlloyOre,
        Self::LiftRail,
        Self::ArchivistCore,
        Self::EngineSteel,
        Self::ConveyorLeft,
        Self::ConveyorRight,
        Self::PistonHousing,
        Self::EngineCoreOre,
        Self::Pipe,
        Self::ArchiveShelf,
        Self::ArchiveStone,
        Self::StarGlass,
        Self::ChartPlinth,
        Self::ChartPlinthRead,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CityStone => "city stone",
            Self::TowerGlass => "tower glass",
            Self::TramRail => "tram rail",
            Self::Platform => "tram platform",
            Self::StreetLamp => "street lamp",
            Self::AlloyOre => "ancient alloy ore",
            Self::LiftRail => "lift rail",
            Self::ArchivistCore => "archivist core",
            Self::EngineSteel => "engine plating",
            Self::ConveyorLeft | Self::ConveyorRight => "belt",
            Self::PistonHousing => "piston housing",
            Self::EngineCoreOre => "engine core ore",
            Self::Pipe => "coolant pipe",
            Self::ArchiveShelf => "archive shelving",
            Self::ArchiveStone => "archive stone",
            Self::StarGlass => "star glass",
            Self::ChartPlinth | Self::ChartPlinthRead => "star chart plinth",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`]). Never renumber.
    #[must_use]
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// The tile with code `code`, if any.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        if code < Self::ALL.len() as u64 {
            #[allow(clippy::cast_possible_truncation, reason = "checked above")]
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// Whether no drill or blast breaks it (the machinery's fixed parts and the plinths).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::TramRail
                | Self::Platform
                | Self::LiftRail
                | Self::ArchivistCore
                | Self::PistonHousing
                | Self::ChartPlinth
                | Self::ChartPlinthRead
        )
    }

    /// Whether drilling it out raises the Megacity's alarm (the city's own fabric).
    #[must_use]
    pub const fn alarms(self) -> bool {
        matches!(self, Self::CityStone | Self::TowerGlass | Self::StreetLamp)
    }

    /// Drill strength needed (Descent VII wants the deep drills; glass and shelving give
    /// easily).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::TowerGlass | Self::ArchiveShelf => 3,
            Self::StreetLamp | Self::Pipe | Self::StarGlass => 4,
            Self::ConveyorLeft | Self::ConveyorRight => 5,
            Self::AlloyOre | Self::EngineCoreOre | Self::ArchiveStone => 6,
            _ => 7,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::TowerGlass | Self::ArchiveShelf => 3,
            Self::StreetLamp | Self::Pipe => 5,
            Self::StarGlass => 7,
            Self::ConveyorLeft | Self::ConveyorRight => 8,
            Self::AlloyOre | Self::EngineCoreOre => 10,
            Self::ArchiveStone => 12,
            _ => 13,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::TowerGlass | Self::ArchiveShelf => 0.06,
            Self::StreetLamp | Self::Pipe => 0.1,
            Self::StarGlass | Self::ConveyorLeft | Self::ConveyorRight => 0.14,
            Self::AlloyOre | Self::EngineCoreOre => 0.18,
            Self::ArchiveStone => 0.2,
            _ => 0.22,
        }
    }

    /// The item a broken tile yields (the layer resources; glass, shelving and lamps give
    /// none).
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::AlloyOre => ItemKind::Layer(Stratum::BuriedMegacity),
            Self::EngineCoreOre => ItemKind::Layer(Stratum::EngineHalls),
            Self::StarGlass => ItemKind::Layer(Stratum::ArchiveOfStars),
            Self::CityStone | Self::EngineSteel | Self::ArchiveStone => {
                ItemKind::Block(crate::item::BlockKind::HardRock)
            }
            _ => return None,
        })
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::StreetLamp => 230,
            Self::ArchivistCore => 200,
            Self::ChartPlinth => 190,
            Self::StarGlass => 140,
            Self::TowerGlass => 70,
            Self::Platform => 60,
            Self::EngineCoreOre => 90,
            Self::ChartPlinthRead => 40,
            _ => 0,
        }
    }

    /// The belt's direction (-1 left, 1 right), if it is one.
    #[must_use]
    pub const fn belt(self) -> Option<i8> {
        match self {
            Self::ConveyorLeft => Some(-1),
            Self::ConveyorRight => Some(1),
            _ => None,
        }
    }
}

// --- The Megacity's alarm ----------------------------------------------------------------------

/// The alarm's full scale.
pub const ALARM_FULL: u32 = 1_000;
/// What one drilled tile of the city's fabric adds (a few tiles is a warning, a hole through a
/// tower is a hunt).
pub const ALARM_PER_TILE: u32 = 140;
/// How fast the alarm cools, per second.
pub const ALARM_COOL_PER_SECOND: u32 = 45;
/// Alarm at which the city starts to watch (the HUD warns).
pub const ALARM_WATCH: u32 = 400;
/// How long a full alarm's hunt lasts.
pub const ALARM_HUNT_MILLIS: u64 = 20_000;
/// How far the city's drones and sentinels come from to hunt an alarmed rig, tiles.
pub const ALARM_HUNT_TILES: f32 = 40.0;

/// A rig's standing with the city's watch: its alarm level when last raised and since when it
/// has been cooling (tick-rate safe: the level now is computed from world time).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct CityAlarm {
    /// The level when last raised (per mille of [`ALARM_FULL`]).
    pub level: u32,
    /// World milliseconds of the last raise.
    pub since_millis: u64,
    /// World milliseconds the hunt ends, if the alarm went full.
    pub hunt_until_millis: Option<u64>,
}

impl CityAlarm {
    /// The level at world time `now_millis`.
    #[must_use]
    pub const fn level_at(&self, now_millis: u64) -> u32 {
        let elapsed = now_millis.saturating_sub(self.since_millis);
        let cooled = elapsed.saturating_mul(ALARM_COOL_PER_SECOND as u64) / 1_000;
        if cooled >= self.level as u64 {
            0
        } else {
            #[allow(clippy::cast_possible_truncation, reason = "below the level")]
            let left = self.level - cooled as u32;
            left
        }
    }

    /// Whether the city hunts this rig at `now_millis`.
    #[must_use]
    pub const fn hunting(&self, now_millis: u64) -> bool {
        match self.hunt_until_millis {
            Some(until) => now_millis < until,
            None => false,
        }
    }

    /// Raises the alarm by `amount` at `now_millis`; returns whether it went full (a hunt
    /// begins).
    pub fn raise(&mut self, amount: u32, now_millis: u64) -> bool {
        let level = (self.level_at(now_millis) + amount).min(ALARM_FULL);
        self.level = level;
        self.since_millis = now_millis;
        if level >= ALARM_FULL && !self.hunting(now_millis) {
            self.hunt_until_millis = Some(now_millis + ALARM_HUNT_MILLIS);
            return true;
        }
        false
    }

    /// Whether nothing is left of it at `now_millis` (forgotten then).
    #[must_use]
    pub const fn spent(&self, now_millis: u64) -> bool {
        self.level_at(now_millis) == 0 && !self.hunting(now_millis)
    }
}

// --- The Engine Halls' beat --------------------------------------------------------------------

/// The Great Engine's stroke at the Engine Halls' top and bottom (gently, then pushed).
pub const ENGINE_BEAT_MILLIS: (u64, u64) = (4_400, 3_000);

/// The engine's stroke `along` per mille through the Engine Halls, rounded to 100 ms so a
/// hall's machinery and the music agree on a clean tempo.
#[must_use]
pub const fn engine_beat_millis(along: u16) -> u64 {
    let along = if along > 1_000 { 1_000 } else { along as u64 };
    let (top, bottom) = ENGINE_BEAT_MILLIS;
    let beat = top - (top - bottom) * along / 1_000;
    beat / 100 * 100
}

/// The beat's tempo in beats a minute (the music follows it in the Engine Halls).
#[must_use]
pub const fn engine_bpm(beat_millis: u64) -> u32 {
    if beat_millis == 0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a few dozen beats a minute"
    )]
    let bpm = (60_000 / beat_millis) as u32;
    bpm
}

/// A belt's carrying speed, pixels a second.
pub const BELT_SPEED: f32 = 120.0;
/// Hull a crusher's slam takes (scaled by the depth's creature scale in the tick).
pub const CRUSH_DAMAGE: f32 = 16.0;
/// How long a crusher's warning glows before it slams (part of its cycle).
pub const CRUSHER_TELL_MILLIS: u64 = 1_200;
/// How long a piston hisses before it rises.
pub const PISTON_TELL_MILLIS: u64 = 900;

// --- Scrap crawlers and spark wisps --------------------------------------------------------------

/// Most ore a scrap crawler carries off in one bite.
pub const CRAWLER_THEFT: u32 = 2;
/// Fuel a spark wisp's jolt drains.
pub const WISP_FUEL_DRAIN: f32 = 3.0;

// --- Landmarks and their story -------------------------------------------------------------------

/// The buried city's landmarks a crew can find (each found once, with its story beat).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CityLandmark {
    /// Buried Megacity: the Lantern Library, the archivist's tower.
    LanternLibrary,
    /// Buried Megacity: the Archive Post, Descent VII's outpost and its Lamplighters.
    ArchivePost,
    /// Engine Halls: the Great Engine, its flywheel still turning.
    GreatEngine,
    /// The Archive of Stars: the Orrery, a hall of the builders' sky.
    Orrery,
    /// The Archive of Stars: the first puzzle vault a crew opens.
    FirstVault,
}

impl CityLandmark {
    pub const ALL: [Self; 5] = [
        Self::LanternLibrary,
        Self::ArchivePost,
        Self::GreatEngine,
        Self::Orrery,
        Self::FirstVault,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LanternLibrary => "The Lantern Library",
            Self::ArchivePost => "The Archive Post",
            Self::GreatEngine => "The Great Engine",
            Self::Orrery => "The Orrery",
            Self::FirstVault => "A Sealed Vault",
        }
    }

    /// The stratum it stands in.
    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::LanternLibrary | Self::ArchivePost => Stratum::BuriedMegacity,
            Self::GreatEngine => Stratum::EngineHalls,
            Self::Orrery | Self::FirstVault => Stratum::ArchiveOfStars,
        }
    }

    /// Its story beat: what the crew reads there (the builders' marks, the Fathers' traces).
    #[must_use]
    pub const fn beat(self) -> &'static str {
        match self {
            Self::LanternLibrary => {
                "A tower of lamps that never went out. At its top a builder machine wakes, \
                 counts you twice and says, in a voice like a librarian's: 'Visitors. The last \
                 ones signed the register.' Six names, in the Fathers' hands, the ink still dark."
            }
            Self::ArchivePost => {
                "A hall of lamplighters and patient machines. Their grandparents came down \
                 after a lost crew and stayed to keep the street lamps burning. 'The Fathers \
                 slept here,' the keeper says, 'and asked the way to the stars.'"
            }
            Self::GreatEngine => {
                "A flywheel the size of a hill, still turning after a hundred thousand years. \
                 Every piston in the halls keeps its beat. Chalked on its housing: 'It isn't \
                 an engine for moving things. It is a clock for something waiting.' - I."
            }
            Self::Orrery => {
                "A hall of glass spheres, each a star, each on its arm. At its heart the \
                 hanging stone and the cable, and from the stone a line of light running out \
                 past every sphere. The builders did not come from below. They came down."
            }
            Self::FirstVault => {
                "The lock gives, and inside, on a plinth, a sheet of light: a star you have \
                 never seen, and numbers that are a way to it. Someone scratched beside it: \
                 'Amos copied this one. He said we would need it on the way home.'"
            }
        }
    }
}

// --- The world state -------------------------------------------------------------------------------

/// The buried city's shared state (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct AncientCity {
    /// Rigs riding a tram, a lift or a piston.
    #[serde(default)]
    pub rides: BTreeMap<PlayerId, CityRide>,
    /// Each rig's standing with the Megacity's watch (absent: calm).
    #[serde(default)]
    pub alarms: BTreeMap<PlayerId, CityAlarm>,
    /// Each player's archivist (absent until it wakes for them).
    #[serde(default)]
    pub archivists: BTreeMap<PlayerId, Archivist>,
    /// The star charts the crew has read, in the order they were found (the seam Wave 11's
    /// space generator reads: [`crate::star_chart`]).
    #[serde(default)]
    pub charts: Vec<StarChart>,
    /// Vaults opened (each vault's anchor tile).
    #[serde(default)]
    pub vaults: BTreeSet<TilePosition>,
    /// Landmarks the crew has found.
    #[serde(default)]
    pub found: BTreeSet<CityLandmark>,
}

impl AncientCity {
    /// Whether nothing has happened here (left out of the state hash then, so every older
    /// world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rides.is_empty()
            && self.alarms.is_empty()
            && self.archivists.is_empty()
            && self.charts.is_empty()
            && self.vaults.is_empty()
            && self.found.is_empty()
    }

    /// Whether `player` rides a tram, lift or piston.
    #[must_use]
    pub fn is_riding(&self, player: PlayerId) -> bool {
        self.rides.contains_key(&player)
    }

    /// `player`'s alarm level at world time `now_millis`.
    #[must_use]
    pub fn alarm_of(&self, player: PlayerId, now_millis: u64) -> u32 {
        self.alarms
            .get(&player)
            .map_or(0, |alarm| alarm.level_at(now_millis))
    }

    /// Whether the city hunts `player` at `now_millis`.
    #[must_use]
    pub fn hunted(&self, player: PlayerId, now_millis: u64) -> bool {
        self.alarms
            .get(&player)
            .is_some_and(|alarm| alarm.hunting(now_millis))
    }

    /// Forgets a leaving player's ride and alarm (its archivist waits for it to come back).
    pub fn forget(&mut self, player: PlayerId) {
        self.rides.remove(&player);
        self.alarms.remove(&player);
    }
}

impl crate::position::Reframe for AncientCity {
    fn reframe(&mut self, shift: crate::position::FrameShift) {
        for archivist in self.archivists.values_mut() {
            let (x, y) = shift.point((archivist.x, archivist.y));
            archivist.x = x;
            archivist.y = y;
            archivist.from = shift.point(archivist.from);
        }
    }
}

/// Host bookkeeping of the buried city: saved, never hashed or replicated (clients see its
/// effects as entity updates and events).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct CitySchedule {
    /// When each rig was last struck by a crusher (one hit a slam).
    #[serde(default)]
    pub crushed: BTreeMap<PlayerId, SimulationTick>,
    /// What each scrap crawler carries off (dropped where it dies).
    #[serde(default)]
    pub stolen: BTreeMap<u64, ItemStack>,
    /// When each rig last rode off a piston (no re-boarding the same head mid-launch).
    #[serde(default)]
    pub launched: BTreeMap<PlayerId, SimulationTick>,
    /// The expedition logs each rig's archivist has already remarked on (the count found).
    #[serde(default)]
    pub logs_seen: BTreeMap<PlayerId, u8>,
    /// Each rider's stick this tick (`(horizontal, thrust)`; rebuilt every tick from the held
    /// controls, never saved).
    #[serde(skip)]
    pub sticks: BTreeMap<PlayerId, (f32, bool)>,
}

impl CitySchedule {
    /// Whether nothing waits.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.crushed.is_empty() && self.stolen.is_empty() && self.launched.is_empty()
    }
}

/// The ore a scrap crawler would carry off `bay` (the most valuable mineral, up to
/// [`CRAWLER_THEFT`]).
#[must_use]
pub fn crawler_pick(bay: &[ItemStack]) -> Option<ItemStack> {
    bay.iter()
        .filter_map(|stack| match stack.kind {
            ItemKind::Mineral(mineral) if stack.count > 0 => Some((mineral, stack.count)),
            _ => None,
        })
        .max_by_key(|(mineral, _)| (mineral.value(), *mineral as u8))
        .map(|(mineral, count)| {
            ItemStack::new(ItemKind::Mineral(mineral), count.min(CRAWLER_THEFT))
        })
}

/// Whether `mineral` is worth an archivist's scan mark.
#[must_use]
pub const fn scan_worthy(mineral: MineralKind) -> bool {
    mineral.value() >= MineralKind::Gold.value()
}

// --- Events ------------------------------------------------------------------------------------------

/// What a [`crate::event::GameplayEventKind::AncientCity`] event reports (netcode codes
/// `630 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CityEvent {
    /// A rig boarded a tram (`detail`: the stop index).
    TramBoarded,
    /// A rig stepped off a tram.
    TramAlighted,
    /// A tram carrying a rig pulled into a station (`detail`: the stop index).
    TramArrived,
    /// A rig boarded a lift cab.
    LiftBoarded,
    /// A rig stepped out of a lift cab.
    LiftAlighted,
    /// A rig settled onto a piston's head.
    PistonBoarded,
    /// A rig drove off a rising piston and was flung (`detail`: its upward speed, px/s).
    PistonLaunched,
    /// A crusher slammed onto a rig.
    Crushed,
    /// The city noticed a rig cutting its fabric (`detail`: the alarm per mille).
    AlarmRising,
    /// The alarm went full: the city's drones and sentinels hunt the rig.
    AlarmHunt,
    /// A rig's archivist woke.
    ArchivistWoke,
    /// A rig's archivist spoke (`detail`: the line's id, [`archivist::line`]).
    ArchivistSpoke,
    /// A rig's archivist marked ore nearby (`detail`: how many tiles).
    ArchivistScanned,
    /// A star chart was read (`detail`: its index in the crew's list).
    ChartRead,
    /// A puzzle vault's lock gave.
    VaultOpened,
    /// A landmark was found (`detail`: its index in [`CityLandmark::ALL`]).
    LandmarkFound,
    /// A scrap crawler carried off ore (`detail`: how much).
    CargoStolen,
    /// A spark wisp's jolt drained fuel.
    FuelJolted,
}

impl CityEvent {
    /// Offset of the event's netcode code from 630.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// The buried city's authored content in `stratum` for the descent probe: `(mechanics, set
/// pieces)` beyond what the registry already says, and the new things' names.
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8, &'static [&'static str]) {
    match stratum {
        Stratum::BuriedMegacity => (
            3,
            3,
            &[
                "trams",
                "tower lifts",
                "the city alarm",
                "the archivist companion",
                "the Lantern Library",
                "the Archive Post's Lamplighters",
            ],
        ),
        Stratum::EngineHalls => (
            3,
            2,
            &[
                "the Great Engine's beat",
                "riding the pistons",
                "crushers and belts",
                "piston shafts",
                "the Great Engine",
            ],
        ),
        Stratum::ArchiveOfStars => (
            3,
            2,
            &[
                "plate vaults",
                "crawlway lever vaults",
                "light vaults",
                "star charts",
                "the Orrery",
            ],
        ),
        _ => (0, 0, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_tiles_round_trip_their_codes() {
        for (index, tile) in CityTile::ALL.into_iter().enumerate() {
            assert_eq!(tile.code(), index as u64);
            assert_eq!(CityTile::from_code(tile.code()), Some(tile));
            if tile.indestructible() {
                assert_eq!(tile.hardness(), crate::terrain::UNBREAKABLE_HARDNESS);
                assert!(!tile.alarms());
            }
        }
        assert!(CityTile::from_code(CityTile::ALL.len() as u64).is_none());
    }

    #[test]
    fn the_alarm_rises_cools_and_hunts_once() {
        let mut alarm = CityAlarm::default();
        assert!(!alarm.raise(ALARM_PER_TILE, 0));
        assert_eq!(alarm.level_at(0), ALARM_PER_TILE);
        // It cools at its rate.
        assert_eq!(
            alarm.level_at(1_000),
            ALARM_PER_TILE - ALARM_COOL_PER_SECOND
        );
        assert!(alarm.spent(10_000));
        // Enough tiles at once go full: a hunt, once.
        let mut went_full = 0;
        for _ in 0..10 {
            if alarm.raise(ALARM_PER_TILE, 20_000) {
                went_full += 1;
            }
        }
        assert_eq!(went_full, 1);
        assert!(alarm.hunting(20_000 + ALARM_HUNT_MILLIS - 1));
        assert!(!alarm.hunting(20_000 + ALARM_HUNT_MILLIS));
    }

    #[test]
    fn the_engine_beats_faster_deeper() {
        assert_eq!(engine_beat_millis(0), ENGINE_BEAT_MILLIS.0);
        assert_eq!(engine_beat_millis(1_000), ENGINE_BEAT_MILLIS.1);
        assert!(engine_beat_millis(500) < engine_beat_millis(0));
        assert!(engine_bpm(engine_beat_millis(1_000)) > engine_bpm(engine_beat_millis(0)));
    }

    #[test]
    fn a_crawler_takes_the_best_ore() {
        let bay = [
            ItemStack::new(ItemKind::Mineral(MineralKind::Copper), 9),
            ItemStack::new(ItemKind::Mineral(MineralKind::Mythril), 5),
        ];
        assert_eq!(
            crawler_pick(&bay),
            Some(ItemStack::new(
                ItemKind::Mineral(MineralKind::Mythril),
                CRAWLER_THEFT
            ))
        );
        assert!(crawler_pick(&[]).is_none());
    }
}
