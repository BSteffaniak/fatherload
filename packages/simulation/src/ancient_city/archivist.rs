//! The archivist: a builder machine that wakes in the Lantern Library (or at the Archive
//! Post) and rides with a rig as a little drone, the crew's companion from the Buried Megacity
//! down to the Heart.
//!
//! Each player has their own ([`Archivist`], saved and replicated in the buried city's shared
//! section, keyed by player, so split-screen seats and online partners each have one). It
//! **follows** its rig or its pilot on foot ([`follow_step`], a frame-relative point the
//! world moves every tick), **talks** ([`LINES`]: hints at the machinery and the vaults, a word
//! on every creature it first sees, the strata as they come, the alarm, low fuel and hull, and
//! the story: landmarks, charts and the Fathers' logs, then quiet musings when nothing happens
//! for a while), and **scans** ([`SCAN_MILLIS`]): every so often it marks the best ore near its
//! rig. Which line it says is chosen in the tick from what is happening (the people/story
//! engine's way: authored lines with stable ids, each said once, read by every peer from the
//! same state).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    descent::Stratum,
    enemy::EnemyKind,
    ids::SimulationTick,
    terrain::{TILE_SIZE, TilePosition},
};

use super::CityLandmark;

/// The archivist's name, as it gives it.
pub const ARCHIVIST_NAME: &str = "Index";
/// The drone moves and looks round on this step of world time.
pub const STEP_MILLIS: u64 = 250;
/// A rig or pilot wakes an archivist core this close (tiles: from the Archive Post's floor to
/// the core in its ceiling).
pub const WAKE_REACH_TILES: f32 = 9.0;
/// Least time between two of its lines (hints and remarks).
pub const LINE_GAP_MILLIS: u64 = 12_000;
/// Least time between two lines even for news that cannot wait (a landmark, a chart).
pub const URGENT_GAP_MILLIS: u64 = 3_500;
/// After this long without a word it muses (in the builders' world only).
pub const MUSE_AFTER_MILLIS: u64 = 150_000;
/// How long the game shows a line.
pub const LINE_SHOW_MILLIS: u64 = 7_000;
/// How often it scans for ore.
pub const SCAN_MILLIS: u64 = 18_000;
/// How far it scans (tiles).
pub const SCAN_REACH_TILES: i32 = 9;
/// Most ore tiles it marks at once.
pub const SCAN_MARKS: usize = 3;
/// How long the game shows its marks.
pub const SCAN_SHOW_MILLIS: u64 = 7_000;
/// Where it hovers from its rig: behind and above (px).
pub const HOVER_OFFSET: (f32, f32) = (22.0, -30.0);
/// Its top speed (px/s).
pub const FOLLOW_SPEED: f32 = 520.0;
/// How quickly it closes the gap (per second, a share of the distance).
pub const FOLLOW_RATE: f32 = 2.6;
/// Further than this from its rig it blinks back to it (tiles: a beacon jump, a lift ride).
pub const FOLLOW_SNAP_TILES: f32 = 18.0;

/// A rig's archivist (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct Archivist {
    /// When it woke.
    pub woke: SimulationTick,
    /// Where its drone hovers (local pixels of the world's frame), stepped every
    /// [`STEP_MILLIS`] of world time.
    pub x: f32,
    pub y: f32,
    /// Where it was a step ago (the game draws it gliding from there).
    #[serde(default)]
    pub from: (f32, f32),
    /// The lines it has said (by id).
    #[serde(default)]
    pub said: BTreeSet<u16>,
    /// Its last line and when it said it.
    #[serde(default)]
    pub line: Option<(u16, SimulationTick)>,
    /// The ore tiles it last marked, and when.
    #[serde(default)]
    pub scan: Vec<TilePosition>,
    #[serde(default)]
    pub scan_tick: SimulationTick,
    /// The next musing.
    #[serde(default)]
    pub musing: u8,
}

impl Archivist {
    /// A newly woken archivist hovering at `(x, y)`.
    #[must_use]
    pub fn woken(at: SimulationTick, x: f32, y: f32) -> Self {
        Self {
            woke: at,
            x,
            y,
            from: (x, y),
            scan_tick: at,
            ..Self::default()
        }
    }

    /// Whether it has said line `id`.
    #[must_use]
    pub fn has_said(&self, id: u16) -> bool {
        self.said.contains(&id)
    }
}

/// One step of an archivist's flight towards `target` over `dt` seconds: it closes a share of
/// the gap a second, no faster than [`FOLLOW_SPEED`], and blinks back if it fell far behind.
#[must_use]
pub fn follow_step(from: (f32, f32), target: (f32, f32), dt: f32) -> (f32, f32) {
    let (dx, dy) = (target.0 - from.0, target.1 - from.1);
    let distance = dx.hypot(dy);
    if distance > FOLLOW_SNAP_TILES * TILE_SIZE {
        return target;
    }
    if distance <= 0.25 {
        return target;
    }
    let step = (distance * FOLLOW_RATE * dt)
        .min(FOLLOW_SPEED * dt)
        .min(distance);
    (
        (dx / distance).mul_add(step, from.0),
        (dy / distance).mul_add(step, from.1),
    )
}

/// The point an archivist hovers at beside an actor at `(x, y)` facing `facing` (-1 left, 1
/// right): behind it and above.
#[must_use]
pub fn hover_point(x: f32, y: f32, facing: f32) -> (f32, f32) {
    let side = if facing < 0.0 { 1.0 } else { -1.0 };
    (x + side * HOVER_OFFSET.0, y + HOVER_OFFSET.1)
}

/// The kinds of puzzle vault (the generator's and the archivist's word for them).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VaultKind {
    /// A plate opens the door while weight is on it and a moment after: dash.
    Plate,
    /// The door's lever is up a crawlway only a pilot fits.
    Lever,
    /// Walls of coloured glass that open to the matching lamp light.
    Light,
    /// All three, one after another (the deepest vaults).
    Grand,
}

impl VaultKind {
    pub const ALL: [Self; 4] = [Self::Plate, Self::Lever, Self::Light, Self::Grand];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plate => "plate vault",
            Self::Lever => "crawlway vault",
            Self::Light => "light vault",
            Self::Grand => "grand vault",
        }
    }
}

/// What makes an archivist speak.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    /// It just woke.
    Woke,
    /// Its rig entered a stratum.
    Stratum(Stratum),
    /// A tram line is close.
    NearTram,
    /// A tower lift is close.
    NearLift,
    /// A piston is close.
    NearPiston,
    /// A crusher is close.
    NearCrusher,
    /// Its rig rides a tram, lift or piston.
    Riding(super::MoverKind),
    /// A puzzle vault of a kind is close.
    NearVault(VaultKind),
    /// A vault's lock gave.
    VaultOpened,
    /// A star chart was read (the crew's count after it).
    Chart(usize),
    /// The city's watch is up for its rig.
    AlarmWatch,
    /// The city hunts its rig.
    AlarmHunt,
    /// Fuel is low.
    LowFuel,
    /// The hull is low.
    LowHull,
    /// A creature it has not remarked on is in sight.
    Creature(EnemyKind),
    /// A landmark was found.
    Landmark(CityLandmark),
    /// The crew read another of the Fathers' logs (its number).
    Log(u8),
    /// It marked ore for the first time.
    Scanned,
    /// Nothing has happened for a while.
    Muse,
}

impl Trigger {
    /// Whether this news cannot wait out the usual gap between lines.
    #[must_use]
    pub const fn urgent(self) -> bool {
        matches!(
            self,
            Self::Woke
                | Self::Landmark(_)
                | Self::VaultOpened
                | Self::Chart(_)
                | Self::AlarmHunt
                | Self::LowHull
        )
    }
}

/// One of the archivist's lines.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Line {
    /// Stable id (events carry it; never renumber, append new lines).
    pub id: u16,
    pub trigger: LineTrigger,
    pub text: &'static str,
}

/// What a line answers (a [`Trigger`] pattern).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineTrigger {
    Woke,
    Stratum(Stratum),
    NearTram,
    NearLift,
    NearPiston,
    NearCrusher,
    Riding(super::MoverKind),
    NearVault(VaultKind),
    VaultOpened,
    /// The first chart, then the `n`th (said once each), then any later one.
    FirstChart,
    ChartCount(usize),
    AlarmWatch,
    AlarmHunt,
    LowFuel,
    LowHull,
    Creature(EnemyKind),
    Landmark(CityLandmark),
    /// A log from this number on (said once, in order).
    LogFrom(u8),
    Scanned,
    Muse,
}

impl LineTrigger {
    /// Whether this line answers `trigger`.
    #[must_use]
    pub fn answers(self, trigger: Trigger) -> bool {
        match (self, trigger) {
            (Self::Woke, Trigger::Woke)
            | (Self::NearTram, Trigger::NearTram)
            | (Self::NearLift, Trigger::NearLift)
            | (Self::NearPiston, Trigger::NearPiston)
            | (Self::NearCrusher, Trigger::NearCrusher)
            | (Self::VaultOpened, Trigger::VaultOpened)
            | (Self::AlarmWatch, Trigger::AlarmWatch)
            | (Self::AlarmHunt, Trigger::AlarmHunt)
            | (Self::LowFuel, Trigger::LowFuel)
            | (Self::LowHull, Trigger::LowHull)
            | (Self::Scanned, Trigger::Scanned)
            | (Self::Muse, Trigger::Muse) => true,
            (Self::Stratum(a), Trigger::Stratum(b)) => a == b,
            (Self::Riding(a), Trigger::Riding(b)) => a == b,
            (Self::NearVault(a), Trigger::NearVault(b)) => a == b,
            (Self::FirstChart, Trigger::Chart(count)) => count >= 1,
            (Self::ChartCount(n), Trigger::Chart(count)) => count >= n,
            (Self::Creature(a), Trigger::Creature(b)) => a == b,
            (Self::Landmark(a), Trigger::Landmark(b)) => a == b,
            (Self::LogFrom(from), Trigger::Log(number)) => number >= from,
            _ => false,
        }
    }
}

const fn line(id: u16, trigger: LineTrigger, text: &'static str) -> Line {
    Line { id, trigger, text }
}

/// Every line the archivist has (see the module docs). Ids are stable.
pub const LINES: [Line; 62] = [
    line(
        1,
        LineTrigger::Woke,
        "Oh. A reader. I am Index, keeper of the Lantern Library. You are late. They said \
         someone would follow them. I will come with you; the city has changed, and so have I.",
    ),
    // --- Strata ---------------------------------------------------------------------------
    line(
        2,
        LineTrigger::Stratum(Stratum::BuriedMegacity),
        "The Megacity. Mind the paving: the city still keeps order, and it does not like \
         holes in itself. The trams and lifts still run, if you can catch them.",
    ),
    line(
        3,
        LineTrigger::Stratum(Stratum::EngineHalls),
        "The Engine Halls. Listen: everything here keeps one beat. Count it, and the halls \
         are a staircase. Lose it, and they are a mill.",
    ),
    line(
        4,
        LineTrigger::Stratum(Stratum::ArchiveOfStars),
        "The Archive of Stars. My home. The vaults keep the charts; each lock is a lesson. \
         The builders thought anyone who could not learn should not leave.",
    ),
    line(
        5,
        LineTrigger::Stratum(Stratum::SleepingGarrison),
        "The garrison. They are only sleeping, reader. Whatever you build here, build it \
         facing the dark.",
    ),
    line(
        6,
        LineTrigger::Stratum(Stratum::TheSealGate),
        "The Seal Gate. It will want something from every chapter of your way down. I hope \
         you kept your keys.",
    ),
    line(
        7,
        LineTrigger::Stratum(Stratum::DarkMatterVeins),
        "Below the gate. My records end here. Everything after this, we read together.",
    ),
    line(
        8,
        LineTrigger::Stratum(Stratum::TheHeart),
        "The Heart. Can you hear it? Not a heart. An engine, idling, for longer than the sea \
         has been salt.",
    ),
    // --- Machinery -------------------------------------------------------------------------
    line(
        10,
        LineTrigger::NearTram,
        "A tram line. Wait on the platform: the car stops, you drive in, and it carries you. \
         Drive out at any stop. They keep their timetable to the second.",
    ),
    line(
        11,
        LineTrigger::NearLift,
        "A tower lift. The cab calls at every floor and both avenues. Step in when it waits; \
         step out where you like. It is quieter than drilling through the tower.",
    ),
    line(
        12,
        LineTrigger::NearPiston,
        "Pistons. Settle on a head and it lifts you. Drive off at the top onto a ledge, or \
         thrust off while it rises and it will throw you higher than your engines could.",
    ),
    line(
        13,
        LineTrigger::NearCrusher,
        "A crusher. Its lamp glows before every slam; then you have a whole beat. Do not \
         stop underneath to admire the craftsmanship.",
    ),
    line(
        14,
        LineTrigger::Riding(super::MoverKind::Tram),
        "Thirty-one lines once ran here. Nine still do. I kept the timetables, in case \
         anyone came.",
    ),
    line(
        15,
        LineTrigger::Riding(super::MoverKind::Lift),
        "Going down. The builders built up, you know. Then the world grew over them.",
    ),
    line(
        16,
        LineTrigger::Riding(super::MoverKind::Piston),
        "Up we go. Hold on to something. I am holding on to you.",
    ),
    // --- Vaults ----------------------------------------------------------------------------
    line(
        20,
        LineTrigger::NearVault(VaultKind::Plate),
        "A plate vault. The plate opens the door while it bears weight, and a breath after. \
         Rest on it, then go. Quickly.",
    ),
    line(
        21,
        LineTrigger::NearVault(VaultKind::Lever),
        "This lock's lever is up a crawlway. Your rig will not fit, but you will. Hop out, \
         crawl in, pull it, and come back for the rig.",
    ),
    line(
        22,
        LineTrigger::NearVault(VaultKind::Light),
        "Coloured glass. It opens only to its own light. Your lamp takes filters: red, \
         green, blue. Try them, close to the glass.",
    ),
    line(
        23,
        LineTrigger::NearVault(VaultKind::Grand),
        "A grand vault: a plate, a crawlway and three colours of glass, one after another. \
         The builders kept their furthest stars in these.",
    ),
    line(
        24,
        LineTrigger::VaultOpened,
        "It gave. Inside, on the plinth: a chart. Go close and I will read it to you.",
    ),
    // --- Charts ----------------------------------------------------------------------------
    line(
        30,
        LineTrigger::FirstChart,
        "A star chart. A real one: a sun out past the hanging stone, and the way to it. The \
         builders did not come from below. They came down. You can go up.",
    ),
    line(
        31,
        LineTrigger::ChartCount(3),
        "Three charts. I am drawing them for you in the journal: home in the middle, and the \
         sky getting larger.",
    ),
    line(
        32,
        LineTrigger::ChartCount(6),
        "Six. Ines would be glad. She copied the first one by hand, and then asked me for a \
         better pen.",
    ),
    line(
        33,
        LineTrigger::ChartCount(10),
        "Ten charts. That is more of the sky than anyone has known since the builders. Keep \
         them. One day a drive will need them.",
    ),
    // --- Alarm, fuel and hull --------------------------------------------------------------
    line(
        40,
        LineTrigger::AlarmWatch,
        "The city has noticed you cutting it. Ease off the paving and the glass; ride \
         instead, and it will forget.",
    ),
    line(
        41,
        LineTrigger::AlarmHunt,
        "Now you have done it. The drones and sentinels are coming for you. Move, and let \
         the city calm down.",
    ),
    line(
        42,
        LineTrigger::LowFuel,
        "Your tank is low. The Archive Post sells fuel; so does any beacon you have set. I \
         would rather not walk.",
    ),
    line(
        43,
        LineTrigger::LowHull,
        "Your hull is failing. Please go back. I have only just met you.",
    ),
    // --- Creatures -------------------------------------------------------------------------
    line(
        50,
        LineTrigger::Creature(EnemyKind::MaintenanceDrone),
        "A maintenance drone. Harmless while the city is calm. It mends what you break, and \
         it remembers who broke it.",
    ),
    line(
        51,
        LineTrigger::Creature(EnemyKind::ScrapCrawler),
        "A scrap crawler. It eats metal and steals ore from open bays. Kill it and you get \
         your ore back.",
    ),
    line(
        52,
        LineTrigger::Creature(EnemyKind::StreetSentinel),
        "A street sentinel. It sees only what its lamp shows: stay out of the beam, or be \
         quick.",
    ),
    line(
        53,
        LineTrigger::Creature(EnemyKind::CogRoller),
        "A cog roller. It charges along the floors. It cannot stop; step aside and let the \
         wall do it for you.",
    ),
    line(
        54,
        LineTrigger::Creature(EnemyKind::SparkWisp),
        "A spark wisp, drawn to running engines. Its jolt drinks fuel. Keep it off you.",
    ),
    line(
        55,
        LineTrigger::Creature(EnemyKind::RivetHound),
        "A rivet hound. The builders' watchdogs. They pounce from a long way off; watch for \
         the crouch.",
    ),
    line(
        56,
        LineTrigger::Creature(EnemyKind::ArchiveWarden),
        "An archive warden. Slow, armoured, and it reads you with a beam before it fires. \
         Do not stand where it is looking.",
    ),
    line(
        57,
        LineTrigger::Creature(EnemyKind::PageMoth),
        "Page moths. They live on the shelving. They nibble; they are not dangerous, only \
         rude.",
    ),
    line(
        58,
        LineTrigger::Creature(EnemyKind::IndexSpider),
        "An index spider. It keeps the shelves in order and does not like visitors. It \
         drops from the ceilings.",
    ),
    // --- Landmarks -------------------------------------------------------------------------
    line(
        60,
        LineTrigger::Landmark(CityLandmark::LanternLibrary),
        "My library. Every lamp in it has burned since the builders left. I lit the last one \
         myself, the day the Fathers went down.",
    ),
    line(
        61,
        LineTrigger::Landmark(CityLandmark::ArchivePost),
        "The Lamplighters. Good people. They keep the street lamps burning for a city that \
         does not need them, which is why I like them.",
    ),
    line(
        62,
        LineTrigger::Landmark(CityLandmark::GreatEngine),
        "The Great Engine. It is a governor, reader: it keeps something far below from \
         waking all at once.",
    ),
    line(
        63,
        LineTrigger::Landmark(CityLandmark::Orrery),
        "The Orrery. The builders' sky. Look at the line of light from the hanging stone. \
         That was their road.",
    ),
    line(
        64,
        LineTrigger::Landmark(CityLandmark::FirstVault),
        "Your first vault. The builders would have given you a certificate. I give you \
         my respect, which is rarer.",
    ),
    // --- The Fathers' logs -----------------------------------------------------------------
    line(
        70,
        LineTrigger::LogFrom(61),
        "Ines's hand. She wrote neatly even here. She asked me what the trams were for and \
         did not laugh when I said: for going to work.",
    ),
    line(
        71,
        LineTrigger::LogFrom(62),
        "I did say they were late. I meant it kindly. Everyone is late, down here.",
    ),
    line(
        72,
        LineTrigger::LogFrom(63),
        "Yes. The world is a housing, and the stone in your sky is a dock. Amos took that \
         better than I expected. He only sat down for an hour.",
    ),
    line(
        73,
        LineTrigger::LogFrom(64),
        "His leg. I could not lift him; I am very small. Ines set it with a pipe from the \
         halls. He walked on it to the Heart.",
    ),
    line(
        74,
        LineTrigger::LogFrom(66),
        "She read the charts aloud to me, every one, so I would hear them in a human voice. \
         No one had done that before.",
    ),
    line(
        75,
        LineTrigger::LogFrom(67),
        "Ines stayed. She is in the Archive still, reading. If you find her, tell her I kept \
         her pen.",
    ),
    line(
        76,
        LineTrigger::LogFrom(68),
        "Amos went on alone. He asked me not to follow. I am following you instead; I hope \
         that is allowed.",
    ),
    line(
        77,
        LineTrigger::LogFrom(73),
        "We are past my records now. I am reading his logs as you do, for the first time.",
    ),
    // --- Scans and musings ------------------------------------------------------------------
    line(
        80,
        LineTrigger::Scanned,
        "I have marked the best ore near you. I will keep looking while you drive; it is \
         what I am for, mostly.",
    ),
    line(
        81,
        LineTrigger::Muse,
        "Did you know the builders had no word for 'down'? Only 'toward the heart' and \
         'toward the dock'.",
    ),
    line(
        82,
        LineTrigger::Muse,
        "I counted the Fathers in twice. Six went down. I have counted every reader since: \
         you are the seventh.",
    ),
    line(
        83,
        LineTrigger::Muse,
        "The Lamplighters ask me what the builders looked like. Tall, I say. Tired. Very \
         far from home.",
    ),
    line(
        84,
        LineTrigger::Muse,
        "When the drive wakes, the cable will sing. I would like to hear that. I was built \
         to hear that.",
    ),
    line(
        85,
        LineTrigger::Muse,
        "I am not lonely. I have an archive. Though it is nicer with someone to read to.",
    ),
    line(
        86,
        LineTrigger::Muse,
        "Your town is built on the lid of an engine. Do they know? They should come and see.",
    ),
    line(
        87,
        LineTrigger::Muse,
        "Dov's radio still hums in my memory. He tuned it to the cable, once, and the cable \
         answered.",
    ),
    line(
        88,
        LineTrigger::Muse,
        "Teo asked if I could be repaired. I said I was not broken. He fixed my squeak anyway.",
    ),
    line(
        89,
        LineTrigger::Muse,
        "Some of the stars on the charts are gone now. Some are new. The sky is an archive \
         that rewrites itself.",
    ),
    line(
        90,
        LineTrigger::Muse,
        "Thank you for waking me. A library is only a room until someone opens a book.",
    ),
    line(
        91,
        LineTrigger::Riding(super::MoverKind::Tram),
        "Next stop: anywhere you like. There are no fares. There is no one left to collect \
         them.",
    ),
];

/// Ids from here on are the story track's lines.
///
/// Id `STORY_LINE_BASE + n` is `ARCHIVIST_LINES[n]` (Wave 10 integration: the companion speaks
/// the story's words about the Fathers, the Descents and after the end in its musings).
pub const STORY_LINE_BASE: u16 = 1_000;

/// The line with id `id`, if any (its own, or the story's past [`STORY_LINE_BASE`]).
#[must_use]
pub fn line_text(id: u16) -> Option<&'static str> {
    if let Some(index) = id.checked_sub(STORY_LINE_BASE) {
        return crate::expedition::archivist::ARCHIVIST_LINES
            .get(usize::from(index))
            .map(|line| line.line);
    }
    LINES
        .iter()
        .find(|line| line.id == id)
        .map(|line| line.text)
}

/// The story's line the companion muses on next.
///
/// For a crew with `state` standing in `here`: one it has not said: the story's remark for its next musings, see
/// [`crate::expedition::archivist::remark`], as its id past [`STORY_LINE_BASE`].
#[must_use]
pub fn story_musing(
    archivist: &Archivist,
    state: &crate::expedition::ExpeditionState,
    here: crate::descent::Descent,
) -> Option<u16> {
    let tries = u32::try_from(crate::expedition::archivist::ARCHIVIST_LINES.len()).unwrap_or(0) * 2;
    (0..tries)
        .filter_map(|offset| {
            crate::expedition::archivist::remark(
                state,
                here,
                u32::from(archivist.musing).wrapping_add(offset),
            )
        })
        .filter_map(story_line_id)
        .find(|id| !archivist.has_said(*id))
}

/// The id the companion says the story's line `line` by (see [`STORY_LINE_BASE`]).
#[must_use]
pub fn story_line_id(line: &crate::expedition::archivist::ArchivistLine) -> Option<u16> {
    let index = crate::expedition::archivist::ARCHIVIST_LINES
        .iter()
        .position(|candidate| candidate.id == line.id)?;
    STORY_LINE_BASE.checked_add(u16::try_from(index).ok()?)
}

/// The archivist's line answering `trigger` it has not said yet, if any (musings in order;
/// a log remark is the newest it has not said whose log the crew has read).
#[must_use]
pub fn line_for(archivist: &Archivist, trigger: Trigger) -> Option<&'static Line> {
    match trigger {
        Trigger::Log(_) => LINES
            .iter()
            .filter(|line| line.trigger.answers(trigger) && !archivist.has_said(line.id))
            .max_by_key(|line| match line.trigger {
                LineTrigger::LogFrom(from) => from,
                _ => 0,
            }),
        Trigger::Chart(count) => LINES
            .iter()
            .filter(|line| line.trigger.answers(trigger) && !archivist.has_said(line.id))
            .max_by_key(|line| match line.trigger {
                LineTrigger::ChartCount(n) if n <= count => n,
                _ => 0,
            }),
        _ => LINES
            .iter()
            .find(|line| line.trigger.answers(trigger) && !archivist.has_said(line.id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_lines_short() {
        let ids: BTreeSet<u16> = LINES.iter().map(|line| line.id).collect();
        assert_eq!(ids.len(), LINES.len());
        for line in &LINES {
            assert!(line.text.len() <= 240, "line {} is long", line.id);
            assert_eq!(line_text(line.id), Some(line.text));
        }
    }

    #[test]
    fn it_says_each_line_once_and_muses_in_order() {
        let mut archivist = Archivist::default();
        let first = line_for(&archivist, Trigger::Muse).expect("a musing");
        archivist.said.insert(first.id);
        let second = line_for(&archivist, Trigger::Muse).expect("another");
        assert!(second.id > first.id);
        let woke = line_for(&archivist, Trigger::Woke).expect("its greeting");
        archivist.said.insert(woke.id);
        assert!(line_for(&archivist, Trigger::Woke).is_none());
        // The newest unread log remark first.
        let remark = line_for(&archivist, Trigger::Log(64)).expect("a remark");
        assert_eq!(remark.trigger, LineTrigger::LogFrom(64));
        assert!(line_for(&archivist, Trigger::Log(10)).is_none());
        // Charts: the first, then the count it reached.
        assert_eq!(
            line_for(&archivist, Trigger::Chart(1)).map(|line| line.id),
            Some(30)
        );
        assert_eq!(
            line_for(&archivist, Trigger::Chart(4)).map(|line| line.id),
            Some(31)
        );
    }

    #[test]
    fn the_drone_follows_and_blinks_back_when_left_behind() {
        let mut at = (0.0, 0.0);
        for _ in 0..12 {
            at = follow_step(at, (100.0, 0.0), 0.25);
        }
        assert!((at.0 - 100.0).abs() < 2.0, "{at:?}");
        assert_eq!(
            follow_step((0.0, 0.0), (5_000.0, 0.0), 1.0 / 60.0),
            (5_000.0, 0.0)
        );
    }
}
