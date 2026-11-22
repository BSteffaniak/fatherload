//! The Living Dark (expansion plan §14, Descent III; Wave 8): the rules that make its walls
//! alive.
//!
//! Pure data and math; the tick lives in `world/living_dark.rs`, the creatures' behaviour in
//! `world/enemies/living.rs`, the generated places in [`crate::worldgen::descent::living`].
//!
//! | Stratum | Signature rule | Counter-play |
//! |---|---|---|
//! | Fungal Forest | giant mushroom caps bounce rigs; spore pods (and spore puffers' lobs) burst into spore clouds that blur and colour-shift the screen and sting a hull | bounce on purpose; give pods a wide berth; the Bio-Filter sees through spores |
//! | Root Labyrinth | tunnels dug through the labyrinth grow back as roots ([`ROOT_REGROW_MILLIS`], faster deeper), closing the way back over minutes | root axes cut a scorched shaft up that never regrows; fire burns roots away for good |
//! | The Hive | two factions: the Amber Court (friendly: royal jelly, errands) and the Swarm (soldier packs, a brood matriarch); stealing from the Court angers it | errands raise the Court's standing; the last errand is a choice: slay the matriarch (the Swarm breaks) or broker a truce (the Swarm stands down, the Court cools) |
//! | Flesh Caverns | mined flesh heals back ([`FLESH_HEAL_MILLIS`]); gullets' acid tides rise and fall on a cycle ([`TIDE_PERIOD_MILLIS`]); a heartbeat drives the music | dig on, never back; time the tide; the Bio-Filter shrugs off acid |
//! | Glow Sea | near-total darkness: the only light is creatures' (lure fish, glow jellies, anglers' baits); the rig lamp is a candle ([`GLOW_SEA_LAMP_SCALE`]) | follow the lights, mind the anglers' baits; Lamp II |
//! | Gate | the Mycelial Mind: spore storms, possessed creatures, nerve nodes shielding it | kill the nodes, then the Mind; rewards the Bio-Filter and Lamp II |
//!
//! State: [`LivingDark`] (shared, saved, replicated as the appended `LivingDark` section and
//! hashed once anything happened: root axes, the Hive's standing and choices, the Mind's fight
//! and the spore clouds' expiries) and [`LivingSchedule`] (host bookkeeping like the fluids'
//! schedule: saved, never hashed or replicated: tiles waiting to regrow or heal, scorched
//! tiles that never will, the tides' applied levels).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    descent::Stratum,
    enemy::EnemyKind,
    ids::{PlayerId, SimulationTick},
    terrain::TilePosition,
};

// --- Spores ---------------------------------------------------------------------------------

/// How long a burst spore pod's cloud hangs in the air.
pub const SPORE_CLOUD_MILLIS: u64 = 9_000;
/// How long a spore ball's (or the Mind's orb's) cloud hangs.
pub const SPORE_SHOT_CLOUD_MILLIS: u64 = 6_500;
/// Half size of a spore cloud, `(columns, rows)`.
pub const SPORE_CLOUD_HALF: (i32, i32) = (4, 3);
/// Most spore clouds in the air at once (the oldest clears first).
pub const MAX_SPORE_CLOUDS: usize = 12;
/// Hull a second a spore cloud stings a rig without the Bio-Filter (at full strength).
///
/// Scaled by the depth's creature scale (`enemy::tier_scale`: about 4.5x in the Living Dark, so
/// a cloud sat in costs about 2 hull a second; the Mind's storms keep a rig in them, so they
/// add up).
pub const SPORE_DAMAGE_PER_SECOND: f32 = 0.4;
/// A rig brushing a spore pod (its body this close to the pod's tile, pixels) bursts it.
pub const SPORE_POD_BRUSH_PIXELS: f32 = 6.0;

// --- Roots and flesh ------------------------------------------------------------------------

/// How long a tunnel tile dug through the Root Labyrinth takes to grow back at its top
/// (halved by its bottom: the labyrinth tightens as you go down).
pub const ROOT_REGROW_MILLIS: u64 = 150_000;
/// How long a mined flesh tile takes to heal at the Flesh Caverns' top (halved by the bottom).
pub const FLESH_HEAL_MILLIS: u64 = 40_000;
/// A tile that cannot grow back yet (a body in it) tries again this much later.
pub const REGROW_RETRY_MILLIS: u64 = 4_000;
/// Most tiles that grow back in one tick (the regrowth budget).
pub const REGROW_BUDGET: usize = 24;
/// Most tiles waiting to grow back (the oldest are dropped past it).
pub const MAX_SCHEDULED_REGROWTH: usize = 16_384;
/// Most remembered scorched tiles.
pub const MAX_SCORCHED: usize = 16_384;
/// A root axe cuts this many rows straight up.
pub const ROOT_AXE_ROWS: i32 = 12;
/// ... this many columns wide (centred on the rig).
pub const ROOT_AXE_WIDTH: i32 = 3;
/// Root axes in a pack and the pack's base price (scaled by the Descent's price percent).
pub const ROOT_AXE_PACK: (u8, u32) = (3, 700);
/// Most root axes a rig carries.
pub const MAX_ROOT_AXES: u8 = 9;
/// Tiles of roots a fire blast burns away (radius).
pub const ROOT_BURN_RADIUS: i32 = 3;

// --- Acid tides and the heartbeat -----------------------------------------------------------

/// One full acid tide cycle (rise, high water, ebb, low water).
pub const TIDE_PERIOD_MILLIS: u64 = 90_000;
/// The rising share of the cycle; then high water, the ebb and low water.
pub const TIDE_RISE_MILLIS: u64 = 35_000;
pub const TIDE_HIGH_MILLIS: u64 = 10_000;
pub const TIDE_EBB_MILLIS: u64 = 35_000;
/// How often the tide moves (world time; tick-rate safe).
pub const TIDE_STEP_MILLIS: u64 = 500;
/// A gullet's high water, per mille of its basin's height.
pub const TIDE_HIGH_PERMILLE: i32 = 450;
/// Gullets farther than this from every rig (tiles) leave their tide be.
pub const TIDE_REACH_TILES: i32 = 64;

/// The tide's phase at `millis` of world time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum TidePhase {
    Rising,
    High,
    Ebbing,
    Low,
}

/// The acid tide's phase and its level, per mille of high water, at `millis` of world time.
#[must_use]
pub const fn tide_at(millis: u64) -> (TidePhase, u32) {
    let t = millis % TIDE_PERIOD_MILLIS;
    if t < TIDE_RISE_MILLIS {
        #[allow(clippy::cast_possible_truncation, reason = "per mille")]
        let level = (t * 1_000 / TIDE_RISE_MILLIS) as u32;
        (TidePhase::Rising, level)
    } else if t < TIDE_RISE_MILLIS + TIDE_HIGH_MILLIS {
        (TidePhase::High, 1_000)
    } else if t < TIDE_RISE_MILLIS + TIDE_HIGH_MILLIS + TIDE_EBB_MILLIS {
        let into = t - TIDE_RISE_MILLIS - TIDE_HIGH_MILLIS;
        #[allow(clippy::cast_possible_truncation, reason = "per mille")]
        let level = (1_000 - into * 1_000 / TIDE_EBB_MILLIS) as u32;
        (TidePhase::Ebbing, level)
    } else {
        (TidePhase::Low, 0)
    }
}

/// The Flesh Caverns' heartbeat at `along` per mille through the stratum, in beats a minute:
/// slow at its top, racing at its bottom, and quicker while the tide rises (the music's
/// tempo follows it).
#[must_use]
pub const fn heartbeat_bpm(along: u16, phase: TidePhase) -> u32 {
    let base = 56 + along as u32 * 44 / 1_000;
    match phase {
        TidePhase::Rising => base + 14,
        TidePhase::High => base + 22,
        TidePhase::Ebbing => base + 4,
        TidePhase::Low => base,
    }
}

// --- The Glow Sea ---------------------------------------------------------------------------

/// A rig lamp's reach in the Glow Sea without Lamp II (a candle in the dark).
pub const GLOW_SEA_LAMP_SCALE: f32 = 0.3;
/// The Glow Sea's light without any creature near (per one), as its field says.
pub const GLOW_SEA_AMBIENT: f32 = 0.06;
/// A rig lamp's reach before scaling, tiles (the renderer's stock lamp).
pub const GLOW_SEA_RIG_LAMP_TILES: f32 = 5.5;

// --- The Hive -------------------------------------------------------------------------------

/// The Amber Court's standing at which it turns on the crew (its drones attack, its counter
/// only takes amends).
pub const COURT_HOSTILE_BELOW: i16 = -30;
/// The standing at which the Court honours the crew (half-price jelly).
pub const COURT_HONOURED_AT: i16 = 50;
/// Standing lost for each Court comb or jelly cell drilled inside its hall.
pub const COURT_THEFT_STANDING: i16 = 3;
/// Base price of a royal jelly draught (full hull, a quarter tank).
pub const JELLY_PRICE: u32 = 900;
/// Base price of the amends that bring a hostile Court back to [`COURT_AMENDS_STANDING`].
pub const AMENDS_PRICE: u32 = 4_000;
pub const COURT_AMENDS_STANDING: i16 = -10;
/// Base price of the tribute that buys the Swarm's truce.
pub const TRUCE_PRICE: u32 = 6_000;
/// Standing the Court loses when the crew brokers a truce with the Swarm.
pub const TRUCE_STANDING: i16 = 15;

/// One of the Court's errands, in the order the Court hands them out.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Errand {
    /// Cull Swarm soldiers and lancers.
    SoldiersAtTheGate,
    /// Break the stolen brood's jelly cells in the matriarch's gallery.
    TheStolenBrood,
    /// Slay the brood matriarch (or broker a truce at the counter).
    TheMatriarch,
}

impl Errand {
    pub const ALL: [Self; 3] = [
        Self::SoldiersAtTheGate,
        Self::TheStolenBrood,
        Self::TheMatriarch,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::SoldiersAtTheGate => "Soldiers at the Gate",
            Self::TheStolenBrood => "The Stolen Brood",
            Self::TheMatriarch => "The Matriarch",
        }
    }

    /// What the Court's steward asks.
    #[must_use]
    pub const fn brief(self) -> &'static str {
        match self {
            Self::SoldiersAtTheGate => {
                "The Swarm's soldiers raid our foragers. Cull eight of them, soldiers or lancers."
            }
            Self::TheStolenBrood => {
                "They carried our brood into the matriarch's gallery. Break six of the jelly \
                 cells there, so the young come home."
            }
            Self::TheMatriarch => {
                "The matriarch breeds an army. End her, and the Swarm is broken. Or buy her \
                 peace, if you think peace can be bought."
            }
        }
    }

    /// How many of the errand's deeds it takes.
    #[must_use]
    pub const fn goal(self) -> u16 {
        match self {
            Self::SoldiersAtTheGate => 8,
            Self::TheStolenBrood => 6,
            Self::TheMatriarch => 1,
        }
    }

    /// Standing and credits the Court gives for it.
    #[must_use]
    pub const fn reward(self) -> (i16, u32) {
        match self {
            Self::SoldiersAtTheGate => (20, 3_000),
            Self::TheStolenBrood => (15, 4_500),
            Self::TheMatriarch => (40, 9_000),
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// How the Swarm stands.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum SwarmState {
    /// At war: soldier packs roam the Hive.
    #[default]
    Raiding,
    /// The crew bought its peace: its soldiers stand down (they still live in the Hive).
    Truce,
    /// The matriarch is dead: no more soldiers are bred, and the Court's drones hold the halls.
    Broken,
}

/// The Hive's state: the Court's standing with the crew, the errands and the Swarm.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct HiveState {
    /// The Amber Court's standing (-100..=100).
    #[serde(default)]
    pub standing: i16,
    /// The errand the Court has handed out, if any (`None`: none yet, or all done).
    #[serde(default)]
    pub errand: Option<Errand>,
    /// Progress on the current errand.
    #[serde(default)]
    pub progress: u16,
    /// Errands turned in.
    #[serde(default)]
    pub done: BTreeSet<Errand>,
    #[serde(default)]
    pub swarm: SwarmState,
}

impl HiveState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the Court has turned on the crew.
    #[must_use]
    pub const fn court_hostile(&self) -> bool {
        self.standing < COURT_HOSTILE_BELOW
    }

    /// Whether the Court honours the crew.
    #[must_use]
    pub const fn honoured(&self) -> bool {
        self.standing >= COURT_HONOURED_AT
    }

    /// Whether the current errand's deeds are done (ready to turn in).
    #[must_use]
    pub fn errand_ready(&self) -> bool {
        self.errand
            .is_some_and(|errand| self.progress >= errand.goal())
    }

    /// The next errand the Court will hand out.
    #[must_use]
    pub fn next_errand(&self) -> Option<Errand> {
        if self.errand.is_some() || self.swarm != SwarmState::Raiding {
            return None;
        }
        Errand::ALL
            .into_iter()
            .find(|errand| !self.done.contains(errand))
    }

    /// Changes the standing by `delta`, clamped.
    pub fn shift(&mut self, delta: i16) {
        self.standing = self.standing.saturating_add(delta).clamp(-100, 100);
    }

    /// Whether `kind` hunts rigs given the Hive's politics (Court drones only when the Court
    /// is hostile; the Swarm not under a truce).
    #[must_use]
    pub const fn hostile_kind(&self, kind: EnemyKind) -> bool {
        match kind {
            EnemyKind::HiveDrone => self.court_hostile(),
            EnemyKind::HiveSoldier | EnemyKind::AcidLancer | EnemyKind::SwarmMatriarch => {
                !matches!(self.swarm, SwarmState::Truce)
            }
            _ => true,
        }
    }
}

// --- The Mycelial Mind ----------------------------------------------------------------------

/// Nerve nodes that shield the Mind.
pub const MIND_NODES: usize = 3;
/// How long the Mind stays exposed once its nodes are dead, before they regrow.
pub const MIND_EXPOSED_MILLIS: u64 = 30_000;
/// Time between the Mind's spore storms (shorter once it has been exposed).
pub const MIND_STORM_MILLIS: u64 = 8_000;
/// Time between possessions.
pub const MIND_POSSESS_MILLIS: u64 = 10_000;
/// How far the Mind reaches to possess a creature, tiles.
pub const MIND_POSSESS_TILES: f32 = 30.0;
/// The Mind sleeps again (and heals) once no rig has been in its hollow for this long.
pub const MIND_SLEEP_MILLIS: u64 = 30_000;
/// [`crate::enemy::EnemyState::phase`] of a creature the Mind possesses (no other Living Dark
/// creature uses the crypt lord's phase byte).
pub const POSSESSED_PHASE: u8 = 0xF0;

/// The Mycelial Mind's fight.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct MindState {
    /// Beaten: the gate is open and its rewards handed out.
    #[serde(default)]
    pub beaten: bool,
    /// The Mind's entity while it is awake.
    #[serde(default)]
    pub entity: Option<u64>,
    /// Its living nerve nodes.
    #[serde(default)]
    pub nodes: Vec<u64>,
    /// Exposed (nodes dead) until this tick.
    #[serde(default)]
    pub exposed_until: Option<SimulationTick>,
    /// How many times its nodes have fallen (each time it storms faster).
    #[serde(default)]
    pub cycles: u8,
    #[serde(default)]
    pub next_storm: SimulationTick,
    #[serde(default)]
    pub next_possession: SimulationTick,
    /// The last tick a rig was in its hollow.
    #[serde(default)]
    pub last_visited: SimulationTick,
}

impl MindState {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the Mind is awake.
    #[must_use]
    pub const fn awake(&self) -> bool {
        self.entity.is_some()
    }

    /// Whether its nodes still shield it.
    #[must_use]
    pub const fn shielded(&self) -> bool {
        !self.nodes.is_empty()
    }
}

// --- The world state ------------------------------------------------------------------------

/// The Living Dark's shared state (see the module docs).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LivingDark {
    /// Root axes aboard each rig.
    #[serde(default)]
    pub axes: BTreeMap<PlayerId, u8>,
    #[serde(default)]
    pub hive: HiveState,
    #[serde(default)]
    pub mind: MindState,
    /// Spore clouds in the air: their field zone ids and when each clears.
    #[serde(default)]
    pub spores: BTreeMap<u32, SimulationTick>,
}

impl LivingDark {
    /// Whether nothing has happened in the Living Dark (left out of the state hash then, so
    /// every older world's hash is unchanged).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.axes.is_empty()
            && self.hive.is_default()
            && self.mind.is_default()
            && self.spores.is_empty()
    }

    /// Root axes aboard `player`'s rig.
    #[must_use]
    pub fn axes_of(&self, player: PlayerId) -> u8 {
        self.axes.get(&player).copied().unwrap_or(0)
    }

    /// Forgets a leaving player's axes.
    pub fn forget(&mut self, player: PlayerId) {
        self.axes.remove(&player);
    }
}

/// What grows back into a tile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Regrowth {
    /// A living root (the Root Labyrinth).
    Root,
    /// Flesh (the Flesh Caverns).
    Flesh,
}

/// Host bookkeeping of the Living Dark (see the module docs): saved, never hashed or
/// replicated (clients see its effects as terrain writes).
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct LivingSchedule {
    /// Tiles waiting to grow back, by due tick.
    #[serde(default)]
    pub due: BTreeMap<(SimulationTick, TilePosition), Regrowth>,
    /// The due tick of each waiting tile.
    #[serde(default)]
    pub waiting: BTreeMap<TilePosition, SimulationTick>,
    /// Tiles that never grow back (cut by a root axe or burnt), and when they were scorched.
    #[serde(default)]
    pub scorched: BTreeMap<TilePosition, SimulationTick>,
    /// The tide level last applied to each gullet (keyed by its top-left tile).
    #[serde(default)]
    pub tides: BTreeMap<TilePosition, u16>,
    /// The last tide step run (world-time steps since the start).
    #[serde(default)]
    pub tide_step: u64,
}

impl LivingSchedule {
    /// Whether nothing waits.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.due.is_empty()
    }

    /// Schedules `position` to grow back as `kind` at `due` (a tile already waiting keeps its
    /// first time).
    pub fn schedule(&mut self, position: TilePosition, due: SimulationTick, kind: Regrowth) {
        if self.scorched.contains_key(&position) || self.waiting.contains_key(&position) {
            return;
        }
        self.waiting.insert(position, due);
        self.due.insert((due, position), kind);
        while self.due.len() > MAX_SCHEDULED_REGROWTH {
            if let Some(((_, dropped), _)) = self.due.pop_first() {
                self.waiting.remove(&dropped);
            }
        }
    }

    /// Cancels `position`'s regrowth.
    pub fn cancel(&mut self, position: TilePosition) {
        if let Some(due) = self.waiting.remove(&position) {
            self.due.remove(&(due, position));
        }
    }

    /// Marks `position` scorched: it never grows back.
    pub fn scorch(&mut self, position: TilePosition, now: SimulationTick) {
        self.cancel(position);
        self.scorched.insert(position, now);
        while self.scorched.len() > MAX_SCORCHED {
            // Forget the earliest scorched tile.
            if let Some(oldest) = self
                .scorched
                .iter()
                .min_by_key(|(position, tick)| (**tick, **position))
                .map(|(position, _)| *position)
            {
                self.scorched.remove(&oldest);
            }
        }
    }

    /// Whether `position` waits to grow back.
    #[must_use]
    pub fn is_waiting(&self, position: TilePosition) -> bool {
        self.waiting.contains_key(&position)
    }
}

/// How long a tile dug in `stratum` at `along` per mille through it takes to grow back, or
/// `None` where nothing grows back.
#[must_use]
pub const fn regrowth_of(stratum: Stratum, along: u16) -> Option<(Regrowth, u64)> {
    let along = along as u64;
    match stratum {
        Stratum::RootLabyrinth => Some((
            Regrowth::Root,
            ROOT_REGROW_MILLIS - ROOT_REGROW_MILLIS * along / 2_000,
        )),
        Stratum::FleshCaverns => Some((
            Regrowth::Flesh,
            FLESH_HEAL_MILLIS - FLESH_HEAL_MILLIS * along / 2_000,
        )),
        _ => None,
    }
}

// --- Events ---------------------------------------------------------------------------------

/// What a [`crate::event::GameplayEventKind::LivingDark`] event reports (netcode codes
/// `440 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum LivingDarkEvent {
    /// A spore pod or spore ball burst into a cloud.
    SporeBurst,
    /// Roots grew back over a dug tunnel.
    RootsRegrew,
    /// Flesh healed over a mined tile.
    FleshHealed,
    /// A root axe cut a shaft up.
    RootAxeFired,
    /// Fire burnt roots away.
    RootsBurned,
    /// A gullet's acid tide began to rise.
    TideRising,
    /// The tide began to ebb.
    TideEbbing,
    /// The Amber Court's standing changed.
    CourtStanding,
    /// An errand was turned in.
    ErrandDone,
    /// The matriarch fell: the Swarm is broken.
    SwarmBroken,
    /// The crew bought the Swarm's truce.
    SwarmTruce,
    /// The Court turned on the crew.
    CourtHostile,
    /// The Mycelial Mind woke.
    MindAwakened,
    /// The Mind possessed a creature.
    MindPossessed,
    /// The Mind's nodes fell: it is exposed.
    MindExposed,
    /// The Mind fell.
    MindDefeated,
    /// The gate's rewards (the Bio-Filter and Lamp II) were fitted to a rig.
    GateRewards,
    /// A rig bounced off a mushroom cap or a cap crawler.
    CapBounce,
    /// A royal jelly draught or root axes were bought.
    Bought,
    /// The Mind's nodes grew back.
    MindRegrew,
}

impl LivingDarkEvent {
    /// Offset of the event's netcode code from 440.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

// --- Commands -------------------------------------------------------------------------------

/// A service at the Amber Court's counter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CourtService {
    /// A royal jelly draught: the hull mended, a quarter tank.
    RoyalJelly,
    /// Take the next errand.
    TakeErrand,
    /// Turn in the current errand.
    TurnIn,
    /// Pay the Swarm's tribute through the Court: a truce (the last errand's other way).
    BrokerTruce,
    /// Make amends with a hostile Court.
    MakeAmends,
}

impl CourtService {
    pub const ALL: [Self; 5] = [
        Self::RoyalJelly,
        Self::TakeErrand,
        Self::TurnIn,
        Self::BrokerTruce,
        Self::MakeAmends,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RoyalJelly => "Royal jelly",
            Self::TakeErrand => "Take an errand",
            Self::TurnIn => "Report the errand",
            Self::BrokerTruce => "Broker a truce with the Swarm",
            Self::MakeAmends => "Make amends",
        }
    }
}

/// A Living Dark command (sent as `LogisticsCommand::LivingDark`, so the depth logistics
/// dialog serves it with every input already wired).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LivingDarkAction {
    /// Buy a pack of root axes (at an outpost, in town or at the Court).
    BuyRootAxes,
    /// Fire a root axe: cut a scorched shaft straight up.
    FireRootAxe,
    /// A service at the Amber Court's counter.
    Court(CourtService),
}

/// Why a Living Dark command is refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LivingDarkRefusal {
    /// Root axes sell at outposts, in town and at the Court.
    NoAxeSeller,
    AxesFull,
    NoAxes,
    /// Nothing above the rig an axe can cut.
    NothingToCut,
    /// Step inside the Amber Court's hall.
    NotAtCourt,
    /// The Court is hostile: it only takes amends.
    CourtHostile,
    /// The Court is not hostile: no amends needed.
    NoAmendsNeeded,
    /// No errand to hand out (or one is under way).
    NoErrand,
    /// The errand's deeds are not done yet.
    ErrandUnfinished,
    /// The truce is only for the matriarch's errand, while the Swarm raids.
    NoTruce,
    /// A sound hull and a full tank.
    NothingToMend,
}

impl LivingDarkRefusal {
    /// The refusal in words.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::NoAxeSeller => "Root axes sell at outposts, in town and at the Amber Court.",
            Self::AxesFull => "The axe rack is full.",
            Self::NoAxes => "No root axes aboard.",
            Self::NothingToCut => "Nothing above to cut.",
            Self::NotAtCourt => "Step inside the Amber Court's hall.",
            Self::CourtHostile => "The Court will not deal with thieves. Make amends first.",
            Self::NoAmendsNeeded => "The Court holds nothing against the crew.",
            Self::NoErrand => "The Court has no errand for you now.",
            Self::ErrandUnfinished => "The errand is not done yet.",
            Self::NoTruce => "Only the matriarch's errand can end in a truce.",
            Self::NothingToMend => "The rig needs nothing.",
        }
        .to_owned()
    }
}
