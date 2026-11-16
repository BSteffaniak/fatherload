//! Contracted construction over in-game days.
//!
//! A [`Construction`] is one signed contract on a plot: the job ([`super::HomeJob`]), the hired
//! contractor's terms, and when work started **in world time** (`WorldClock::world_millis`,
//! plan §11 decision 3): sleeping through the night moves the clock and so moves the work on,
//! and nothing happens while the game is closed. Progress is a pure function of the world time
//! ([`Construction::progress_per_mille`]), so it is deterministic, replicates for free and
//! survives saves.
//!
//! Work shows in stages ([`BuildStage`]: survey stakes, foundation, scaffolding and frame,
//! walls, roof, finishing). Paying to rush ([`Construction::rush_quote`]) brings the finish
//! forward; a materials shortage ([`SiteEventKind::Shortage`]) holds work until it is answered
//! (or the contractor sources the stock after [`SHORTAGE_PATIENCE_MILLIS`]).

use serde::{Deserialize, Serialize};

use super::HomeJob;
use crate::{
    item::{BlockKind, ItemKind},
    people::{
        CharacterId,
        contractor::{ContractQuote, ContractorProfile, ContractorStyle},
    },
    terrain::MineralKind,
    worldgen::noise::hash2,
};

/// A visible stage of the work.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BuildStage {
    /// Survey stakes and string lines on the bare plot.
    #[default]
    Survey,
    /// The ground levelled and a stone foundation laid.
    Foundation,
    /// Scaffolding and the timber frame.
    Frame,
    /// Walls going up.
    Walls,
    /// The roof on.
    Roof,
    /// Windows, paint and the garden.
    Finishing,
    /// Done: the keys are yours.
    Done,
}

impl BuildStage {
    pub const ALL: [Self; 7] = [
        Self::Survey,
        Self::Foundation,
        Self::Frame,
        Self::Walls,
        Self::Roof,
        Self::Finishing,
        Self::Done,
    ];

    /// The stage at `progress` (per mille).
    #[must_use]
    pub const fn at(progress: u32) -> Self {
        match progress {
            0..80 => Self::Survey,
            80..250 => Self::Foundation,
            250..480 => Self::Frame,
            480..700 => Self::Walls,
            700..880 => Self::Roof,
            880..1000 => Self::Finishing,
            _ => Self::Done,
        }
    }

    /// Where this stage starts (per mille).
    #[must_use]
    pub const fn start(self) -> u32 {
        match self {
            Self::Survey => 0,
            Self::Foundation => 80,
            Self::Frame => 250,
            Self::Walls => 480,
            Self::Roof => 700,
            Self::Finishing => 880,
            Self::Done => 1_000,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Survey => "Surveying",
            Self::Foundation => "Foundation",
            Self::Frame => "Scaffolding and frame",
            Self::Walls => "Walls",
            Self::Roof => "Roof",
            Self::Finishing => "Finishing touches",
            Self::Done => "Finished",
        }
    }

    /// The site board's line for the stage.
    #[must_use]
    pub const fn line(self) -> &'static str {
        match self {
            Self::Survey => "Stakes and string mark out the house.",
            Self::Foundation => "The ground is levelled and the footings poured.",
            Self::Frame => "Scaffolding up, the frame rising.",
            Self::Walls => "The walls are going up.",
            Self::Roof => "The roof is on: rain can't stop them now.",
            Self::Finishing => "Windows in, paint drying, the garden planted.",
            Self::Done => "Finished. The keys are yours.",
        }
    }
}

/// How long a materials shortage holds work before the contractor sources it themselves.
pub const SHORTAGE_PATIENCE_MILLIS_FRACTION: u64 = 4;
/// World milliseconds a shortage holds the work at most (a quarter of the standard day; see
/// [`Construction::shortage_patience`]).
pub const SHORTAGE_PATIENCE_MILLIS: u64 = 48 * 60_000 / SHORTAGE_PATIENCE_MILLIS_FRACTION;
/// Rushing skips this share (percent) of the work left.
pub const RUSH_SKIP_PERCENT: u64 = 50;
/// Rushing costs this percent of the contract price for every whole contract it skips.
pub const RUSH_PRICE_PERCENT: u64 = 80;
/// The cheapest rush.
pub const MIN_RUSH_PRICE: u32 = 60;
/// What one supplied block takes off the price (the contractor's stock price).
pub const SUPPLIED_BLOCK_DISCOUNT: u32 = 6;
/// At most this percent of the price comes off for supplied blocks.
pub const MAX_SUPPLY_DISCOUNT_PERCENT: u32 = 35;
/// Repeat clients (a finished job with the same contractor) get this percent off.
pub const REPEAT_CLIENT_DISCOUNT_PERCENT: u32 = 5;

/// A small story beat during construction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SiteEventKind {
    /// The timber merchant came up short: work stops until `count` blocks are brought (or the
    /// stock is paid for), or the contractor finds some after a long wait.
    Shortage { block: BlockKind, count: u32 },
    /// The contractor would love a piece of `item` for the house (a keystone, a lintel
    /// ornament): bringing it makes the job one quality better.
    Request { item: ItemKind },
    /// A story from the contractor (line `index` of their anecdotes).
    Anecdote { index: u8 },
}

impl SiteEventKind {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Shortage { .. } => "Materials shortage",
            Self::Request { .. } => "A request from the contractor",
            Self::Anecdote { .. } => "A word from the contractor",
        }
    }

    /// Whether it holds work until answered.
    #[must_use]
    pub const fn holds_work(self) -> bool {
        matches!(self, Self::Shortage { .. })
    }

    /// What the contractor says.
    #[must_use]
    pub fn line(self, style: ContractorStyle) -> String {
        match self {
            Self::Shortage { block, count } => match style {
                ContractorStyle::Grumpy => format!(
                    "Merchant shorted me again. {count} {}s or we stand about. Your choice.",
                    block.name().to_lowercase()
                ),
                ContractorStyle::Chatty => format!(
                    "You'll never believe it: the wagon came without the {}s! {count} would do. Or I'll send to the valley, but that's half a day.",
                    block.name().to_lowercase()
                ),
                _ => format!(
                    "We're short {count} {}s. Bring them, or pay for the stock, and we carry on.",
                    block.name().to_lowercase()
                ),
            },
            Self::Request { item } => format!(
                "I've a notion: a piece of {} set over the door. Bring me one and I'll make the place sing.",
                item_name(item)
            ),
            Self::Anecdote { index } => anecdote(style, index).to_owned(),
        }
    }
}

fn item_name(item: ItemKind) -> String {
    match item {
        ItemKind::Mineral(mineral) => mineral.name().to_lowercase(),
        ItemKind::Artifact(artifact) => artifact.name().to_lowercase(),
        other => other.name().to_lowercase(),
    }
}

/// A contractor's anecdote, by style.
#[must_use]
pub const fn anecdote(style: ContractorStyle, index: u8) -> &'static str {
    let lines: [&str; 3] = match style {
        ContractorStyle::FastAndLoose => [
            "Built a whole barn in an afternoon once. Leans a bit. The cows don't mind.",
            "Plumb lines are a suggestion. Gravity sorts out the rest.",
            "If a nail bends, you hit it harder. That's the whole trade.",
        ],
        ContractorStyle::SlowAndPerfect => [
            "My master made me plane one board for a month. I still have it. It's perfect.",
            "A house is a promise to the next hundred winters. I don't break promises.",
            "Measure twice. Then measure again, in case the first two were showing off.",
        ],
        ContractorStyle::Chatty => [
            "Did I tell you about my cousin's chimney? Swallows nested in it, then an owl, then my cousin.",
            "This beam came from a tree that remembers the old mine. You can hear it creak stories.",
            "The lads bet you'd put a trophy case in. I said a piano. Don't tell me, I like suspense.",
        ],
        ContractorStyle::Grumpy => [
            "Rained all morning. Sun all afternoon. Both wrong.",
            "Somebody keeps flying a rig over the scaffold. If it's you: don't.",
            "It's going well. Don't say anything. You'll jinx it.",
        ],
        ContractorStyle::Steady => [
            "Footings are dry and square. Good start to anything.",
            "We'll have the frame up by the end of the week. Or sooner, weather willing.",
            "Folk keep stopping to look. That's a good sign for a house.",
        ],
    };
    lines[(index % 3) as usize]
}

/// One fired event and whether it was answered.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SiteEvent {
    pub kind: SiteEventKind,
    /// The stage it fired in.
    pub stage: BuildStage,
    /// World time it fired at.
    pub at_millis: u64,
    /// Answered (or let go: a shortage the contractor sourced, an anecdote heard).
    pub answered: bool,
}

/// An event planned for a stage of a contract (rolled at signing).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlannedEvent {
    pub stage: BuildStage,
    pub kind: SiteEventKind,
}

/// A signed contract under way on a plot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Construction {
    pub job: HomeJob,
    pub contractor: ContractorProfile,
    /// The town builders' guild (worlds without people): no crew walks the site.
    #[serde(default)]
    pub guild: bool,
    pub quote: ContractQuote,
    /// What the client paid (after discounts).
    pub price_paid: u32,
    /// Blocks the client supplied at signing.
    #[serde(default)]
    pub supplied_blocks: u32,
    /// World time work started.
    pub started_millis: u64,
    /// World time the job takes without rushes or holds.
    pub total_millis: u64,
    /// Work brought forward by rushes.
    #[serde(default)]
    pub rushed_millis: u64,
    /// Work lost to answered or expired holds.
    #[serde(default)]
    pub held_millis: u64,
    /// While a shortage holds work: when the hold began.
    #[serde(default)]
    pub hold_since: Option<u64>,
    /// Events planned at signing (fire as the work reaches their stage).
    #[serde(default)]
    pub planned: Vec<PlannedEvent>,
    /// Events that fired.
    #[serde(default)]
    pub events: Vec<SiteEvent>,
    /// The last stage announced (stage events fire once each).
    #[serde(default)]
    pub announced: BuildStage,
    /// Quality bonus from answered requests (the finished home's quality is the quote's plus
    /// this, at most 5).
    #[serde(default)]
    pub bonus_quality: u8,
}

impl Construction {
    /// World milliseconds of work done at `now`.
    #[must_use]
    pub const fn worked_millis(&self, now: u64) -> u64 {
        let elapsed = now.saturating_sub(self.started_millis);
        let held = self.held_millis
            + match self.hold_since {
                Some(since) => now.saturating_sub(since),
                None => 0,
            };
        elapsed
            .saturating_sub(held)
            .saturating_add(self.rushed_millis)
    }

    /// Progress at world time `now`, in thousandths (1000 when done).
    #[must_use]
    pub fn progress_per_mille(&self, now: u64) -> u32 {
        let total = self.total_millis.max(1);
        let done = self.worked_millis(now).saturating_mul(1_000) / total;
        u32::try_from(done.min(1_000)).unwrap_or(1_000)
    }

    #[must_use]
    pub fn stage(&self, now: u64) -> BuildStage {
        BuildStage::at(self.progress_per_mille(now))
    }

    /// World milliseconds of work left at `now`.
    #[must_use]
    pub const fn remaining_millis(&self, now: u64) -> u64 {
        self.total_millis.saturating_sub(self.worked_millis(now))
    }

    /// World time the work finishes at, if nothing holds it from `now` on.
    #[must_use]
    pub const fn finish_millis(&self, now: u64) -> u64 {
        now.saturating_add(self.remaining_millis(now))
    }

    /// `(credits, millis skipped)` to rush the work at `now`; `None` once it is done or held.
    #[must_use]
    pub fn rush_quote(&self, now: u64) -> Option<(u32, u64)> {
        if self.hold_since.is_some() {
            return None;
        }
        let left = self.remaining_millis(now);
        if left == 0 {
            return None;
        }
        let skip = (left * RUSH_SKIP_PERCENT / 100).max(left.min(10_000));
        let price = u64::from(self.quote.price) * skip * RUSH_PRICE_PERCENT
            / (100 * self.total_millis.max(1));
        let price = u32::try_from(price).unwrap_or(u32::MAX).max(MIN_RUSH_PRICE);
        Some((price, skip))
    }

    /// The open event, if one waits for an answer.
    #[must_use]
    pub fn open_event(&self) -> Option<(usize, SiteEvent)> {
        self.events
            .iter()
            .enumerate()
            .rev()
            .find(|(_, event)| !event.answered)
            .map(|(index, event)| (index, *event))
    }

    /// The finished home's quality (1-5).
    #[must_use]
    pub fn quality(&self) -> u8 {
        self.quote.quality.saturating_add(self.bonus_quality).min(5)
    }

    /// How long a shortage holds work at most, scaled to this contract (a quarter of a day of
    /// the standard length, or less on short jobs).
    #[must_use]
    pub fn shortage_patience(&self) -> u64 {
        SHORTAGE_PATIENCE_MILLIS.min(self.total_millis / 3).max(1)
    }

    /// The character who took the job (meaningless for the guild).
    #[must_use]
    pub const fn character(&self) -> CharacterId {
        self.contractor.character
    }
}

/// Rolls the story beats for a contract signed on plot `plot` (deterministic in the world
/// seed, the plot and the job number): long jobs get more.
#[must_use]
pub fn plan_events(
    seed: u64,
    plot: u32,
    serial: u32,
    job: HomeJob,
    tenth_days: u32,
) -> Vec<PlannedEvent> {
    const SALT: u64 = 0x484F_4D45_0000_0001;
    let roll = |n: i32| hash2(seed ^ SALT, u64::from(plot) << 32 | u64::from(serial), n, 0);
    let mut planned = vec![PlannedEvent {
        stage: BuildStage::Foundation,
        kind: SiteEventKind::Anecdote {
            index: u8::try_from(roll(1) % 3).unwrap_or(0),
        },
    }];
    let long = tenth_days >= 15 || matches!(job, HomeJob::Build(_));
    if long && roll(2) % 100 < 55 {
        let block = [BlockKind::Dirt, BlockKind::Clay, BlockKind::Stone]
            [usize::try_from(roll(3) % 3).unwrap_or(0)];
        let count = 8 + u32::try_from(roll(4) % 12).unwrap_or(0);
        planned.push(PlannedEvent {
            stage: BuildStage::Frame,
            kind: SiteEventKind::Shortage { block, count },
        });
    }
    if long && roll(5) % 100 < 60 {
        let item = match roll(6) % 4 {
            0 => ItemKind::Mineral(MineralKind::Silver),
            1 => ItemKind::Mineral(MineralKind::Gold),
            2 => ItemKind::Mineral(MineralKind::Emerald),
            _ => ItemKind::Mineral(MineralKind::Iron),
        };
        planned.push(PlannedEvent {
            stage: BuildStage::Walls,
            kind: SiteEventKind::Request { item },
        });
    }
    if tenth_days >= 40 {
        planned.push(PlannedEvent {
            stage: BuildStage::Roof,
            kind: SiteEventKind::Anecdote {
                index: u8::try_from(roll(7) % 3).unwrap_or(0) + 1,
            },
        });
    }
    planned
}

/// `(credits off, blocks used)` for supplying blocks from a hopper holding `held` toward a job
/// needing `needed` with list price `price`.
#[must_use]
pub fn supply_discount(held: u32, needed: u32, price: u32) -> (u32, u32) {
    let used = held.min(needed);
    let cap = price * MAX_SUPPLY_DISCOUNT_PERCENT / 100;
    ((used * SUPPLIED_BLOCK_DISCOUNT).min(cap), used)
}

#[cfg(test)]
mod tests {
    use super::{BuildStage, Construction, SiteEventKind, plan_events, supply_discount};
    use crate::{
        home::{Blueprint, HomeJob},
        people::{
            CharacterId,
            contractor::{ContractQuote, ContractorProfile, ContractorStyle},
        },
    };

    fn contract(total: u64) -> Construction {
        Construction {
            job: HomeJob::Build(Blueprint::Cabin),
            contractor: ContractorProfile {
                character: CharacterId::default(),
                style: ContractorStyle::Steady,
                speed_percent: 100,
                quality: 3,
                price_percent: 100,
            },
            guild: true,
            quote: ContractQuote {
                tenth_days: 10,
                price: 2_500,
                quality: 3,
            },
            price_paid: 2_500,
            supplied_blocks: 0,
            started_millis: 1_000,
            total_millis: total,
            rushed_millis: 0,
            held_millis: 0,
            hold_since: None,
            planned: Vec::new(),
            events: Vec::new(),
            announced: BuildStage::Survey,
            bonus_quality: 0,
        }
    }

    #[test]
    fn stages_follow_progress_in_order() {
        let work = contract(100_000);
        let mut last = BuildStage::Survey;
        for step in 0..=110 {
            let stage = work.stage(1_000 + step * 1_000);
            assert!(stage >= last);
            last = stage;
        }
        assert_eq!(last, BuildStage::Done);
        assert_eq!(work.stage(1_000), BuildStage::Survey);
        assert_eq!(work.stage(1_000 + 50_000), BuildStage::Walls);
    }

    #[test]
    fn holds_stop_the_clock_and_rushes_bring_it_forward() {
        let mut work = contract(100_000);
        work.hold_since = Some(11_000);
        assert_eq!(work.progress_per_mille(11_000), 100);
        assert_eq!(work.progress_per_mille(61_000), 100, "held");
        assert!(work.rush_quote(61_000).is_none(), "no rushing a held job");
        work.held_millis = 50_000;
        work.hold_since = None;
        assert_eq!(work.progress_per_mille(61_000), 100);
        let (price, skip) = work.rush_quote(61_000).expect("rushable");
        assert_eq!(skip, 45_000);
        assert!(price >= super::MIN_RUSH_PRICE);
        work.rushed_millis += skip;
        assert_eq!(work.progress_per_mille(61_000), 550);
    }

    #[test]
    fn events_are_deterministic_and_long_jobs_get_more() {
        let a = plan_events(7, 3, 0, HomeJob::Build(Blueprint::Manor), 70);
        assert_eq!(
            a,
            plan_events(7, 3, 0, HomeJob::Build(Blueprint::Manor), 70)
        );
        assert!(a.len() >= 2);
        assert!(matches!(a[0].kind, SiteEventKind::Anecdote { .. }));
        let any_shortage = (0..40).any(|seed| {
            plan_events(seed, 1, 0, HomeJob::Build(Blueprint::House), 30)
                .iter()
                .any(|event| matches!(event.kind, SiteEventKind::Shortage { .. }))
        });
        assert!(any_shortage);
    }

    #[test]
    fn supplying_blocks_is_capped() {
        assert_eq!(supply_discount(10, 30, 2_500), (60, 10));
        assert_eq!(supply_discount(1_000, 260, 1_000), (350, 260));
    }
}
