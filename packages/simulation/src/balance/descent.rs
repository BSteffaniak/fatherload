//! The descent probe (Part III F10): a bot plays a whole Deep Descent, 41 strata to the
//! Heart, and the report reads its clock.
//!
//! **What is real and what is modelled.** A 60-90 hour run cannot go through the fixed tick in
//! a report, and the V11 strata do not exist yet, so the probe is a model on top of real
//! numbers:
//!
//! * *Real*: every curve the bot meets is the game's own: the Descent ladders and their caps
//!   ([`crate::economy::upgrade_cost_for`] under [`ProgressionRules::Descent`],
//!   [`crate::depth_progression::core_tier_cap`]), the rig systems' prices and gates, the tank,
//!   bay and hull curves, the drill's chip rate and bite, fuel burn, the logistics prices
//!   (beacons, outpost fees and claims, travel costs, lift speeds) and the ore value curve.
//!   With `calibrate` the drill's real pace is measured through the tick (a rig drilling a
//!   column of hard rock, [`measure_dig`]) and the model's dig rate is scaled to match.
//! * *Modelled*: the strata themselves, through the [`StratumRegistry`] adapter. Until the F8
//!   stratum registry lands, [`PlanStrata`] derives each stratum's rock, ore, hazards, heat and
//!   content from the plan's §14 table ([`crate::depth_progression::PlanLayout`]); the
//!   integrator swaps in the real registry by implementing the trait.
//!
//! **The bot.** Before each stratum it must meet its entry needs (a drill that cuts its rock, a
//! hull for its hazards, a radiator for its heat, and the Descent's gate system); until then it
//! makes mining trips from its nearest base (the town, reached outposts, its own beacons every
//! [`BEACON_SPACING_ROWS`]), selling at the outpost and buying upgrades. Each stratum also
//! costs its authored content's minutes (creatures, landmark, mechanic, story). A trip whose
//! hazard damage would wreck the rig is a death (a rescue fee and half the cargo), after which
//! the bot buys hull first.
//!
//! **What it reports.** Time per stratum and per Descent against §14's hours, the "something
//! new every 20-40 minutes" check (the gaps between new things: a stratum's arrival, its
//! creatures, landmark and story beat, an upgrade tier, a gate), deaths, fuel on everyday trips
//! against expeditions, and the estimated whole run (target 60-90 h).

use std::fmt::Write as _;

use super::{BalanceProbe, ProbeContext, ReportSection, Table};
use crate::{
    ControlState, PlayerId, SimulationParams, WorldState,
    depth_progression::{
        DESCENT_COUNT, DescentLayout, DescentSpan, PlanLayout, ProgressionRules, RigSystem,
        core_tier_cap, depth_line_cap, descent_gate, descent_value_percent,
    },
    economy::{MAX_UPGRADE_LEVEL, UpgradeKind, upgrade_cost_for},
    logistics::{
        ANCIENT_LIFT_RESTORE_PRICE, BEACON_PRICE, OUTPOST_CLAIM_PRICE, OUTPOST_FUEL_UNIT,
        OUTPOST_SALE_PERCENT, scaled_price, travel_cost,
    },
    player::{
        cargo_capacity_for_level, drill_bite, drill_speed, engine_power, fuel_burn_efficiency,
        fuel_capacity_for_level, hull_capacity_for_level,
    },
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition, tile_durability},
    world::{AUTHORITATIVE_FUEL_BURN_PER_SECOND, MIN_DRILL_SECONDS_PER_CHIP},
};

/// One stratum as the descent bot meets it.
#[derive(Clone, Debug, PartialEq)]
pub struct DescentStratum {
    pub descent: u8,
    pub name: &'static str,
    pub top: i32,
    pub bottom: i32,
    /// The drill tier that cuts its rock at the comfortable pace (slower below it).
    pub rock_tier: u8,
    /// Its common rock, and how many times tougher than that tile it is (deep rock).
    pub rock: TileKind,
    pub toughness: f64,
    /// Credits one ore tile sells for here.
    pub ore_value: f64,
    /// Share of the tiles a mining rig digs that are ore.
    pub ore_share: f64,
    /// Hull per minute of mining a rig with hull tier equal to `rock_tier` takes.
    pub hazard_per_minute: f64,
    /// The radiator tier its heat needs.
    pub heat_tier: u8,
    /// New things it brings: its arrival, its creatures, its landmark, its mechanic and its
    /// story beat (the length of `markers`).
    pub novelties: u8,
    /// Minutes its authored content takes (creatures met, the landmark, the story beat).
    pub content_minutes: f64,
    /// The new things themselves (Wave 8), from the registry's real content: its arrival,
    /// creatures met here first, its signature mechanic and fluids, its screen effects, its
    /// landmarks, its story logs and the workshop recipe it opens. Upgrade tiers bought and
    /// the Descent's gate are added as the bot plays.
    pub markers: Vec<Novelty>,
}

impl DescentStratum {
    #[must_use]
    pub const fn rows(&self) -> i32 {
        self.bottom - self.top
    }
}

/// What kind of new thing a stratum brings (the "something new every 20-40 minutes" check,
/// expansion plan §13).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NoveltyKind {
    /// Arriving in a new stratum (its banner, rock, palette and music).
    Arrival,
    /// A creature kind met here for the first time.
    Creature,
    /// A rule that changes how it plays: the stratum's signature, fluids, a field.
    Mechanic,
    /// A set piece or a kind of place met here first (outpost, settlement, gate chamber,
    /// cavern, anomaly room, cave ecosystems, tombs).
    Landmark,
    /// A story beat or expedition log.
    StoryLog,
    /// An upgrade tier, rig system or workshop recipe.
    UpgradeTier,
    /// A screen effect (the light, a grade, a blur, ...).
    Effect,
    /// The Descent's gate set piece.
    Gate,
}

impl NoveltyKind {
    pub const ALL: [Self; 8] = [
        Self::Arrival,
        Self::Creature,
        Self::Mechanic,
        Self::Landmark,
        Self::StoryLog,
        Self::UpgradeTier,
        Self::Effect,
        Self::Gate,
    ];

    /// The report's short name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Arrival => "arrival",
            Self::Creature => "creature",
            Self::Mechanic => "mechanic",
            Self::Landmark => "landmark",
            Self::StoryLog => "story",
            Self::UpgradeTier => "upgrade",
            Self::Effect => "effect",
            Self::Gate => "gate",
        }
    }
}

/// One new thing a stratum brings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Novelty {
    pub kind: NoveltyKind,
    pub name: String,
}

impl Novelty {
    #[must_use]
    pub fn new(kind: NoveltyKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
        }
    }
}

/// A source of story logs per stratum (Wave 8 seam): the story track's expedition logs
/// register here, as the field effects' sources do in [`crate::field::STRATUM_FIELD_SOURCES`].
pub type StoryLogSource = fn(crate::descent::Stratum) -> Vec<&'static str>;

/// Every story log source the probe reads (the Descents' own story beats until the story
/// track appends its expedition logs).
pub const STORY_LOG_SOURCES: &[StoryLogSource] = &[descent_story_beats];

/// Each Descent's story beat, told in its first stratum.
#[must_use]
pub fn descent_story_beats(stratum: crate::descent::Stratum) -> Vec<&'static str> {
    let descent = stratum.descent();
    if descent.first_stratum() == stratum {
        vec![descent.story_beat()]
    } else {
        Vec::new()
    }
}

/// The real content markers of every stratum, top to bottom (Wave 8).
///
/// Creatures met first there, its signature mechanic, fluids and field physics, its screen effects, landmarks
/// met first there (set pieces always count), story logs and the workshop recipes it opens.
#[must_use]
pub fn registry_markers() -> Vec<Vec<Novelty>> {
    use crate::descent::{LandmarkKind, Stratum};
    use std::collections::BTreeSet;
    let mut met_creatures = BTreeSet::new();
    let mut met_places = BTreeSet::new();
    let mut met_effects = BTreeSet::new();
    Stratum::ALL
        .iter()
        .map(|stratum| {
            let def = stratum.def();
            let mut markers = vec![Novelty::new(NoveltyKind::Arrival, def.name)];
            // Classic strata roam their band's roster; the rest their own table.
            let creatures: Vec<crate::enemy::EnemyKind> = if def.creatures.is_empty() {
                crate::enemy::band_roster(def.host_band)
                    .iter()
                    .map(|(kind, _)| *kind)
                    .collect()
            } else {
                def.creatures.iter().map(|(kind, _)| *kind).collect()
            };
            for kind in creatures {
                if met_creatures.insert(kind) {
                    markers.push(Novelty::new(NoveltyKind::Creature, kind.name()));
                }
            }
            markers.push(Novelty::new(NoveltyKind::Mechanic, def.signature));
            for rule in def.fluids {
                markers.push(Novelty::new(
                    NoveltyKind::Mechanic,
                    format!("{:?} pockets", rule.kind),
                ));
            }
            let field = &def.field;
            if field.gravity_milli != (0, 1000)
                || field.time_scale_permille != 1000
                || field.friction_permille != 1000
                || field.drag_permille != 1000
                || field.mirror_controls
            {
                markers.push(Novelty::new(NoveltyKind::Mechanic, "field physics"));
            }
            for level in field.screen {
                if met_effects.insert(level.effect) {
                    markers.push(Novelty::new(
                        NoveltyKind::Effect,
                        format!("{:?}", level.effect),
                    ));
                }
            }
            if field.light_permille < 700 && met_effects.insert(crate::descent::ScreenEffect::Fog) {
                markers.push(Novelty::new(NoveltyKind::Effect, "darkness"));
            }
            for kind in def.landmarks {
                let set_piece = matches!(
                    kind,
                    LandmarkKind::Outpost
                        | LandmarkKind::GateChamber
                        | LandmarkKind::HollowCavern
                        | LandmarkKind::AnomalyRoom
                        | LandmarkKind::Settlements
                );
                if set_piece || met_places.insert(*kind) {
                    markers.push(Novelty::new(NoveltyKind::Landmark, format!("{kind:?}")));
                }
            }
            for source in STORY_LOG_SOURCES {
                for log in source(*stratum) {
                    markers.push(Novelty::new(NoveltyKind::StoryLog, log));
                }
            }
            for recipe in crate::workshop::RECIPES
                .iter()
                .filter(|recipe| recipe.stratum() == *stratum)
            {
                markers.push(Novelty::new(
                    NoveltyKind::UpgradeTier,
                    format!("recipe: {}", recipe.name),
                ));
            }
            markers
        })
        .collect()
}

/// The strata the descent bot plays (the F8 stratum registry implements it: [`RegistryStrata`]).
pub trait StratumRegistry {
    /// Every stratum, top to bottom.
    fn strata(&self) -> Vec<DescentStratum>;

    /// The eight Descents' rows (rows below the ground line): the plan's table unless the
    /// registry knows its own.
    fn descents(&self) -> Vec<DescentSpan> {
        PlanLayout.descents().to_vec()
    }
}

/// The real stratum registry ([`crate::descent`]) on a Deep Descent world's seeded layout, as
/// the descent bot plays it (Wave 7 integration).
///
/// From the registry: each stratum's name, its seeded rows (counted from the ground line), its
/// common rock, its ores' values through the stratum (the Descent value curve applied), its
/// hazard weights, and its novelties and content time from its creatures, landmarks and field
/// mechanic. Still modelled like [`PlanStrata`] (the registry has no numbers for them yet):
/// the drill and radiator tiers each stratum asks for and how much tougher deep rock is.
#[derive(Clone, Copy, Debug)]
pub struct RegistryStrata {
    pub settings: crate::worldgen::WorldSettings,
}

impl RegistryStrata {
    /// The registry on the default seed's Deep Descent world.
    #[must_use]
    pub const fn default_world() -> Self {
        Self {
            settings: crate::worldgen::WorldSettings {
                generator: crate::worldgen::GeneratorVersion::DEEP_DESCENT,
                ..crate::worldgen::WorldSettings::new(crate::worldgen::DEFAULT_WORLD_SEED)
            },
        }
    }

    fn ground(&self) -> i32 {
        crate::descent::fields::cached_layout(&self.settings).tops[0]
    }
}

impl StratumRegistry for RegistryStrata {
    fn strata(&self) -> Vec<DescentStratum> {
        use crate::descent::{LandmarkKind, Stratum};
        let layout = crate::descent::fields::cached_layout(&self.settings);
        let ground = self.ground();
        let plan = PlanStrata.strata();
        let markers = registry_markers();
        let last = (Stratum::ALL.len() - 1) as f64;
        Stratum::ALL
            .iter()
            .zip(&plan)
            .zip(markers)
            .enumerate()
            .map(|(index, ((stratum, modelled), markers))| {
                let def = stratum.def();
                let (top, bottom) = layout.rows(*stratum);
                let descent = u8::try_from(stratum.descent().index()).unwrap_or(u8::MAX);
                let share = index as f64 / last;
                // The registry's ores through the middle of the stratum, by weight.
                let weights = def.ore_weights(500);
                let total: u32 = weights.iter().sum();
                let classic = if total == 0 {
                    modelled.ore_value * 100.0 / f64::from(descent_value_percent(descent))
                } else {
                    weights
                        .iter()
                        .zip(crate::worldgen::deposits::MINERALS)
                        .map(|(weight, mineral)| f64::from(*weight) * f64::from(mineral.value()))
                        .sum::<f64>()
                        / f64::from(total)
                };
                let hazard_weight: u32 = def.hazards.iter().map(|(_, weight)| weight).sum();
                let creatures = u8::try_from(def.creatures.len()).unwrap_or(u8::MAX);
                let set_pieces = u8::try_from(
                    def.landmarks
                        .iter()
                        .filter(|kind| {
                            matches!(
                                kind,
                                LandmarkKind::Outpost
                                    | LandmarkKind::GateChamber
                                    | LandmarkKind::AnomalyRoom
                                    | LandmarkKind::HollowCavern
                                    | LandmarkKind::Settlements
                            )
                        })
                        .count(),
                )
                .unwrap_or(u8::MAX);
                let mechanic = u8::from(!def.field.is_neutral() || !def.fluids.is_empty());
                DescentStratum {
                    descent,
                    name: def.name,
                    top: top - ground,
                    bottom: bottom + 1 - ground,
                    rock_tier: modelled.rock_tier,
                    rock: def.soft,
                    toughness: modelled.toughness,
                    ore_value: classic * f64::from(descent_value_percent(descent)) / 100.0,
                    ore_share: 0.2,
                    hazard_per_minute: modelled.hazard_per_minute
                        * (0.5 + f64::from(hazard_weight) / 16.0).min(2.0),
                    heat_tier: (1.0 + 20.0 * share).round() as u8,
                    novelties: u8::try_from(markers.len()).unwrap_or(u8::MAX),
                    content_minutes: 7.0f64.mul_add(
                        f64::from(creatures),
                        6.0f64.mul_add(f64::from(set_pieces + mechanic), 18.0),
                    ),
                    markers,
                }
            })
            .collect()
    }

    fn descents(&self) -> Vec<DescentSpan> {
        let ground = self.ground();
        crate::depth_progression::RegistryLayout::of(&self.settings)
            .descents()
            .iter()
            .map(|span| DescentSpan {
                top: span.top - ground,
                bottom: span.bottom - ground,
                ..*span
            })
            .collect()
    }
}

/// Classic mineral values the plan's ore ramps through.
const MINERAL_LADDER: [MineralKind; 10] = [
    MineralKind::Copper,
    MineralKind::Iron,
    MineralKind::Silver,
    MineralKind::Gold,
    MineralKind::Emerald,
    MineralKind::Ruby,
    MineralKind::Diamond,
    MineralKind::Platinum,
    MineralKind::Uranium,
    MineralKind::Mythril,
];

/// The plan's §14 strata as a [`StratumRegistry`].
///
/// Rock, ore, hazards and heat ramp with the stratum's place in the run, ore value follows the
/// Descent value curve, and every stratum carries the plan's 2-4 creatures, a landmark, a
/// mechanic and a story beat.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlanStrata;

impl StratumRegistry for PlanStrata {
    fn strata(&self) -> Vec<DescentStratum> {
        let strata = PlanLayout.strata();
        let last = (strata.len().max(2) - 1) as f64;
        strata
            .iter()
            .enumerate()
            .map(|(index, span)| {
                let share = index as f64 / last;
                // The classic ladder runs the whole way down (mythril at the Heart), and deep
                // ore is worth the Descent's percent of it.
                // A mining rig's haul is a mix: the Topsoil's averages iron and silver.
                let ladder = (1.5 + share * 7.5).min((MINERAL_LADDER.len() - 1) as f64);
                let low = f64::from(MINERAL_LADDER[ladder.floor() as usize].value());
                let high = f64::from(
                    MINERAL_LADDER[(ladder.ceil() as usize).min(MINERAL_LADDER.len() - 1)].value(),
                );
                let classic = (high - low).mul_add(ladder.fract(), low);
                let creatures = 2 + (index % 3) as u8;
                DescentStratum {
                    descent: span.descent,
                    name: span.name,
                    top: span.top,
                    bottom: span.bottom,
                    rock_tier: (1.0 + 21.0 * share.powf(0.85)).round() as u8,
                    rock: match index {
                        0 => TileKind::Dirt,
                        1 => TileKind::Clay,
                        2 | 3 => TileKind::Stone,
                        _ => TileKind::HardRock,
                    },
                    toughness: 1.0 + 0.1 * index.saturating_sub(11) as f64,
                    ore_value: classic * f64::from(descent_value_percent(span.descent)) / 100.0,
                    ore_share: 0.2,
                    hazard_per_minute: 0.8 * 1.1_f64.powi(i32::try_from(index).unwrap_or(i32::MAX)),
                    heat_tier: (1.0 + 20.0 * share).round() as u8,
                    novelties: 4 + creatures,
                    content_minutes: 24.0 + 7.0 * f64::from(creatures),
                    markers: {
                        let mut markers = vec![
                            Novelty::new(NoveltyKind::Arrival, span.name),
                            Novelty::new(NoveltyKind::Mechanic, "signature rule"),
                            Novelty::new(NoveltyKind::Landmark, "landmark"),
                            Novelty::new(NoveltyKind::StoryLog, "story beat"),
                        ];
                        for creature in 0..creatures {
                            markers.push(Novelty::new(
                                NoveltyKind::Creature,
                                format!("creature {}", creature + 1),
                            ));
                        }
                        markers
                    },
                }
            })
            .collect()
    }
}

/// Rows between the beacons the bot plants on its way down.
pub const BEACON_SPACING_ROWS: i32 = 350;
/// Fuel each drilled tile costs (`world/commands.rs`).
const DIG_FUEL_PER_TILE: f64 = 0.45;
/// Share of mining time spent on thrust (hops up to the next pocket of ore).
const THRUST_SHARE_MINING: f64 = 0.05;
/// Seconds of counter time per trip (selling, refuelling, buying).
const SERVICE_SECONDS: f64 = 25.0;
/// Rows per second a rig falls down an open shaft.
const FALL_ROWS_PER_SECOND: f64 = 16.0;
/// Rows per second a stock engine climbs (the climb cap rises with the engine).
const STOCK_CLIMB_ROWS_PER_SECOND: f64 = 7.0;
/// Seconds per tile a rig needs besides chipping (falling or driving onto the next tile),
/// unless calibrated through the tick.
const DEFAULT_DIG_OVERHEAD: f64 = 0.4;

/// How the descent probe runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescentConfig {
    /// Play only the first strata (tests).
    pub strata_limit: Option<usize>,
    /// Measure the drill's pace through the real tick.
    pub calibrate: bool,
    pub tick_rate_hz: u32,
}

impl Default for DescentConfig {
    fn default() -> Self {
        Self {
            strata_limit: None,
            calibrate: true,
            tick_rate_hz: crate::DEFAULT_TICK_RATE_HZ,
        }
    }
}

/// One stratum of the run.
#[derive(Clone, Debug, PartialEq)]
pub struct StratumRun {
    pub stratum: DescentStratum,
    /// Minutes spent in it (content, mining to afford what the next one needs, digging).
    pub minutes: f64,
    /// Hours from the start of the run when it was left.
    pub end_hours: f64,
    pub trips: u32,
    pub deaths: u32,
    /// Drill and hull tiers on arrival.
    pub drill_tier: u8,
    pub hull_tier: u8,
    /// Fuel an everyday trip from the nearest base burned, as a share of the tank (percent).
    pub everyday_fuel_percent: f64,
    /// The longest wait between new things inside it (minutes), counting from the last new
    /// thing before it and up to the next stratum's arrival.
    pub longest_gap_minutes: f64,
    /// Upgrade tiers and systems bought in it.
    pub purchases: u32,
    /// Of those, the tiers fitted as workshop parts (Wave 8).
    pub crafted: u32,
    /// Every new thing met in it (minutes since it began, kind, name), in order (Wave 8).
    pub events: Vec<(f64, NoveltyKind, String)>,
}

impl StratumRun {
    /// How many of its new things are of `kind`.
    #[must_use]
    pub fn count(&self, kind: NoveltyKind) -> usize {
        self.events
            .iter()
            .filter(|(_, each, _)| *each == kind)
            .count()
    }

    /// Whether something new came at least every [`PACING_MAX_GAP_MINUTES`] here.
    #[must_use]
    pub fn paced(&self) -> bool {
        self.longest_gap_minutes <= PACING_MAX_GAP_MINUTES
    }
}

/// The pacing target (expansion plan §13): something new about every 20-40 minutes, so no wait
/// between new things longer than this.
pub const PACING_MAX_GAP_MINUTES: f64 = 40.0;
/// Most deaths a stratum may cost the bot before the pacing check calls it unfair.
pub const PACING_MAX_DEATHS_PER_STRATUM: u32 = 2;
/// Most of the tank an everyday trip may burn before the check calls fuel unfair (percent).
pub const PACING_MAX_EVERYDAY_FUEL_PERCENT: f64 = 100.0;

/// The whole descent.
#[derive(Clone, Debug, PartialEq)]
pub struct DescentRun {
    pub strata: Vec<StratumRun>,
    /// Hours at which each Descent was finished.
    pub descent_hours: Vec<f64>,
    /// Fuel an expedition (from the Descent's outpost to its bottom and back, no beacons)
    /// would burn, as a share of the tank the bot had there (percent).
    pub expedition_fuel_percent: Vec<f64>,
    /// When each gate system was bought (hours).
    pub systems: Vec<(RigSystem, f64)>,
    /// Every gap between new things (minutes).
    pub gaps: Vec<f64>,
    /// The dig pace scale measured through the tick (or the default).
    pub dig_overhead: f64,
    pub deaths: u32,
}

impl DescentRun {
    #[must_use]
    pub fn total_hours(&self) -> f64 {
        self.strata.last().map_or(0.0, |run| run.end_hours)
    }

    /// Share of gaps between new things that are at most 40 minutes (percent).
    #[must_use]
    pub fn gaps_within_40(&self) -> f64 {
        if self.gaps.is_empty() {
            return 100.0;
        }
        self.gaps.iter().filter(|gap| **gap <= 40.0).count() as f64 / self.gaps.len() as f64 * 100.0
    }

    #[must_use]
    pub fn longest_gap(&self) -> f64 {
        self.gaps.iter().copied().fold(0.0, f64::max)
    }

    /// The strata of the first `descents` Descents that fail the pacing check (Wave 8): a wait
    /// between new things over [`PACING_MAX_GAP_MINUTES`], more than
    /// [`PACING_MAX_DEATHS_PER_STRATUM`] deaths, or everyday trips burning more than
    /// [`PACING_MAX_EVERYDAY_FUEL_PERCENT`] of the tank. Each with why.
    #[must_use]
    pub fn pacing_failures(&self, descents: u8) -> Vec<(String, String)> {
        self.strata
            .iter()
            .filter(|run| run.stratum.descent < descents)
            .filter_map(|run| {
                let mut why = Vec::new();
                if !run.paced() {
                    why.push(format!("a {:.0}-minute wait", run.longest_gap_minutes));
                }
                if run.deaths > PACING_MAX_DEATHS_PER_STRATUM {
                    why.push(format!("{} deaths", run.deaths));
                }
                if run.everyday_fuel_percent > PACING_MAX_EVERYDAY_FUEL_PERCENT {
                    why.push(format!(
                        "everyday trips burn {:.0}% of the tank",
                        run.everyday_fuel_percent
                    ));
                }
                (!why.is_empty()).then(|| (run.stratum.name.to_owned(), why.join(", ")))
            })
            .collect()
    }

    /// Whether the first `descents` Descents pass the pacing check (the integrator keeps
    /// [`crate::worldgen::DEEP_DESCENT_FOR_NEW_WORLDS`] on only if Descents I-III do).
    #[must_use]
    pub fn pacing_passes(&self, descents: u8) -> bool {
        self.strata.iter().any(|run| run.stratum.descent < descents)
            && self.pacing_failures(descents).is_empty()
    }
}

/// Seconds the drill spends chipping out one tile of `rock` (`toughness` times its durability)
/// at drill tier `drill`, by the chip formula (`world/commands.rs`).
fn chip_seconds(rock: TileKind, toughness: f64, drill: u8) -> f64 {
    let durability = f64::from(tile_durability(rock)) * toughness;
    let chips = (durability / f64::from(drill_bite(drill))).ceil();
    let period = (f64::from(crate::world::base_drill_seconds_per_chip(rock))
        / f64::from(drill_speed(drill)))
    .max(f64::from(MIN_DRILL_SECONDS_PER_CHIP));
    chips * period
}

/// Seconds a rig of drill tier `drill` takes per tile of `stratum` (slower with a drill under
/// its rock's tier), plus `overhead` for moving onto the next tile.
fn tile_seconds(stratum: &DescentStratum, drill: u8, overhead: f64) -> f64 {
    let slowdown = 2.0_f64.powf(f64::from(stratum.rock_tier.saturating_sub(drill)) / 3.0);
    chip_seconds(stratum.rock, stratum.toughness, drill).mul_add(slowdown, overhead)
}

/// Seconds per row a rig of drill tier `drill` takes to drill straight down `rows` rows of hard
/// rock, measured through the real tick (`None` if it could not cut it).
#[must_use]
pub fn measure_dig(drill: u8, rows: i32, tick_rate_hz: u32) -> Option<f64> {
    let player = PlayerId::new(0);
    let mut world = WorldState::new_game_with_params(player, SimulationParams::new(tick_rate_hz));
    let column = 30;
    let top = 60;
    for y in top - 3..top + rows + 3 {
        for x in column - 2..=column + 2 {
            let kind = if y < top && x == column {
                TileKind::Air
            } else {
                TileKind::HardRock
            };
            let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
        }
    }
    {
        let rig = world.player_mut(player)?;
        rig.drill_strength = drill;
        rig.engine_level = drill.min(MAX_UPGRADE_LEVEL);
        rig.fuel = rig.fuel_capacity;
        rig.x = (column as f32 + 0.5) * TILE_SIZE;
        rig.y = (top as f32 - 0.5) * TILE_SIZE;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
    }
    let target = top + rows - 1;
    let limit = u64::from(tick_rate_hz) * 600;
    let drill_down = ControlState::new(0.0, false, true);
    let mut ticks = 0_u64;
    while ticks < limit {
        let inputs = crate::TickInputs::default().hold_as(player, drill_down);
        let _ = world.simulate_tick(world.simulation_tick(), &inputs);
        if let Some(rig) = world.player_mut(player) {
            rig.fuel = rig.fuel_capacity;
            rig.hull = rig.max_hull();
        }
        ticks += 1;
        if world.player(player)?.tile_position(TILE_SIZE).y >= target {
            return Some(ticks as f64 / f64::from(tick_rate_hz) / f64::from(rows - 1));
        }
    }
    None
}

/// The tick-measured seconds per row a rig spends besides chipping (falling onto the next
/// tile, the drill biting again), the median over a few drill tiers.
fn dig_overhead(config: &DescentConfig) -> f64 {
    if !config.calibrate {
        return DEFAULT_DIG_OVERHEAD;
    }
    let mut extra: Vec<f64> = [4_u8, 8, 12, 15]
        .iter()
        .filter_map(|drill| {
            measure_dig(*drill, 24, config.tick_rate_hz)
                .map(|measured| (measured - chip_seconds(TileKind::HardRock, 1.0, *drill)).max(0.0))
        })
        .collect();
    extra.sort_by(f64::total_cmp);
    extra
        .get(extra.len() / 2)
        .copied()
        .unwrap_or(DEFAULT_DIG_OVERHEAD)
}

/// The bot's rig and purse.
#[derive(Clone, Debug)]
struct Rig {
    drill: u8,
    tank: u8,
    bay: u8,
    engine: u8,
    hull: u8,
    radiator: u8,
    systems: u8,
    /// Pressure Rating and Headlamp tiers (Wave 8).
    pressure: u8,
    lamp: u8,
    /// Layer resources aboard, by stratum (index into [`crate::descent::Stratum::ALL`]).
    layer_units: [f64; crate::descent::STRATUM_COUNT],
    credits: f64,
}

impl Rig {
    /// Whether the bay holds what `recipe` takes.
    fn holds(&self, recipe: &crate::workshop::Recipe) -> bool {
        recipe
            .inputs
            .iter()
            .all(|(stratum, count)| self.layer_units[stratum.index()] >= f64::from(*count))
    }

    /// Takes what `recipe` takes out of the bay.
    fn spend(&mut self, recipe: &crate::workshop::Recipe) {
        for (stratum, count) in recipe.inputs {
            self.layer_units[stratum.index()] -= f64::from(*count);
        }
    }
}

impl Rig {
    const fn level(&self, kind: UpgradeKind) -> u8 {
        match kind {
            UpgradeKind::Drill => self.drill,
            UpgradeKind::FuelTank => self.tank,
            UpgradeKind::CargoBay => self.bay,
            UpgradeKind::Engine => self.engine,
            UpgradeKind::Hull => self.hull,
            UpgradeKind::PressureRating => self.pressure,
            UpgradeKind::Headlamp => self.lamp,
            _ => self.radiator,
        }
    }

    const fn level_mut(&mut self, kind: UpgradeKind) -> &mut u8 {
        match kind {
            UpgradeKind::Drill => &mut self.drill,
            UpgradeKind::FuelTank => &mut self.tank,
            UpgradeKind::CargoBay => &mut self.bay,
            UpgradeKind::Engine => &mut self.engine,
            UpgradeKind::Hull => &mut self.hull,
            UpgradeKind::PressureRating => &mut self.pressure,
            UpgradeKind::Headlamp => &mut self.lamp,
            _ => &mut self.radiator,
        }
    }
}

/// Minutes a dip past a gate takes to feel its need (and climb back) before its rig system
/// goes on sale (Wave 8).
const GATE_DIP_MINUTES: f64 = 3.0;

/// The thruster burn factor of seals rated `rating` in Descent `descent`.
fn pressure_factor(rating: u8, descent: u8) -> f64 {
    f64::from(crate::depth_progression::pressure_fuel_percent(
        rating, descent,
    )) / 100.0
}

/// What one mining trip from `base_row` to a face at `face_row` in `stratum` yields.
struct Trip {
    seconds: f64,
    credits: f64,
    fuel: f64,
    damage: f64,
    /// Layer resources dug out on the way (Wave 8).
    layer_units: f64,
}

fn trip(rig: &Rig, stratum: &DescentStratum, base_row: i32, face_row: i32, overhead: f64) -> Trip {
    let distance = f64::from((face_row - base_row).max(0));
    let climb = STOCK_CLIMB_ROWS_PER_SECOND
        * f64::from(engine_power(rig.engine).min(crate::player::ENGINE_CLIMB_CAP));
    let travel_seconds = distance / FALL_ROWS_PER_SECOND + distance / climb;
    let cargo = f64::from(cargo_capacity_for_level(rig.bay));
    let tiles = cargo / stratum.ore_share;
    let dig_seconds = tiles * tile_seconds(stratum, rig.drill, overhead);
    let burn = fuel_burn_efficiency(rig.tank);
    // Below its Pressure Rating the thrusters burn more (Wave 8).
    let fuel = tiles * DIG_FUEL_PER_TILE
        + (distance / climb + dig_seconds * THRUST_SHARE_MINING)
            * f64::from(AUTHORITATIVE_FUEL_BURN_PER_SECOND)
            * f64::from(burn)
            * pressure_factor(rig.pressure, stratum.descent);
    let mining_minutes = dig_seconds / 60.0;
    let guard = 0.88_f64.powi(i32::from(rig.hull) - i32::from(stratum.rock_tier));
    Trip {
        seconds: travel_seconds + dig_seconds + SERVICE_SECONDS,
        credits: cargo * stratum.ore_value,
        fuel,
        damage: stratum.hazard_per_minute * mining_minutes * guard,
        layer_units: tiles * (1.0 - stratum.ore_share) / crate::workshop::LAYER_YIELD_ONE_IN as f64,
    }
}

/// Plays the descent through `registry`'s strata under `config`.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the bot's whole run in order")]
pub fn run_descent(registry: &dyn StratumRegistry, config: &DescentConfig) -> DescentRun {
    let rules = ProgressionRules::Descent;
    let mut strata = registry.strata();
    let descents = registry.descents();
    if let Some(limit) = config.strata_limit {
        strata.truncate(limit);
    }
    let overhead = dig_overhead(config);
    let mut rig = Rig {
        drill: 1,
        tank: 1,
        bay: 1,
        engine: 1,
        hull: 1,
        radiator: 1,
        systems: 0,
        pressure: 0,
        lamp: 0,
        layer_units: [0.0; crate::descent::STRATUM_COUNT],
        credits: 0.0,
    };
    let mut clock_minutes = 0.0_f64;
    let mut last_novelty = 0.0_f64;
    let mut gaps = Vec::new();
    let mut runs = Vec::new();
    let mut descent_hours = vec![0.0; DESCENT_COUNT];
    let mut expedition = vec![0.0; DESCENT_COUNT];
    let mut systems = Vec::new();
    let mut beacon_rows: Vec<i32> = Vec::new();
    let mut claimed: Vec<u8> = Vec::new();
    let mut deaths_total = 0;
    let mut felt = 0_u8;
    for (index, stratum) in strata.iter().enumerate() {
        let start = clock_minutes;
        let drill_tier = rig.drill;
        let hull_tier = rig.hull;
        let descent = stratum.descent;
        // New things met in this stratum, in minutes since it began.
        let mut events: Vec<(f64, NoveltyKind, String)> = Vec::new();
        let outpost_row = descents
            .get(usize::from(descent))
            .map_or(0, |span| if descent == 0 { 0 } else { span.top + 40 });
        // The stratum's content minutes, and digging through it.
        let rows = f64::from(stratum.rows());
        let dig_minutes = rows * tile_seconds(stratum, rig.drill, overhead) / 60.0;
        let mut minutes = dig_minutes;
        let new_descent = index > 0 && strata[index - 1].descent != descent;
        // The Descent's ancient lift restored once (it shortens the trips back up).
        if new_descent {
            rig.credits -=
                f64::from(scaled_price(ANCIENT_LIFT_RESTORE_PRICE, descent)).min(rig.credits);
        }
        // What the next stratum asks for.
        let next = strata.get(index + 1);
        // A rig rated for the next stratum: a drill for its rock, a hull for its hazards, a
        // radiator for its heat, a bay, tank and engine for its longer trips, seals for its
        // Descent's pressure, and the gate system when it opens a new Descent.
        let needs = next.map_or((0, 0, 0, 0, None, descent), |next| {
            let gate = if next.descent == descent {
                None
            } else {
                descent_gate(next.descent)
            };
            (
                next.rock_tier.saturating_sub(1),
                next.rock_tier.saturating_sub(2),
                next.heat_tier,
                next.rock_tier.saturating_sub(3),
                gate,
                next.descent,
            )
        });
        let cap = core_tier_cap(descent);
        let line_cap = depth_line_cap(descent);
        // The tier a player is comfortable at by this stratum (Wave 8 bot).
        let comfort = u8::try_from(index + 2).unwrap_or(u8::MAX);
        // The bot mines the upper part of the stratum it is in.
        let face = stratum.top + stratum.rows() / 3;
        let mut trips = 0_u32;
        let mut deaths = 0_u32;
        let mut purchases = 0_u32;
        let mut crafted = 0_u32;
        let mut everyday = 0.0_f64;
        let mut stalled = 0_u32;
        // A part from the workshop pays only part of a tier's price (resources for the rest),
        // when a recipe for the line is known by this Descent's outposts.
        let part_for = |kind: UpgradeKind| {
            crate::workshop::RECIPES.iter().filter(move |recipe| {
                recipe.descent() <= descent
                    && recipe.output == crate::workshop::CraftOutput::Part(kind)
            })
        };
        // Mine until the next stratum's needs are bought (and a comfort tier or two).
        for _ in 0..4_000 {
            // A beacon whenever the next one is affordable: trips start near the face.
            let next_beacon = beacon_rows.last().copied().unwrap_or(0) + BEACON_SPACING_ROWS;
            let beacon_price = f64::from(scaled_price(BEACON_PRICE, descent));
            if next_beacon <= stratum.bottom && rig.credits >= beacon_price {
                rig.credits -= beacon_price;
                beacon_rows.push(next_beacon);
            }
            let base = beacon_rows
                .iter()
                .copied()
                .filter(|row| *row <= face)
                .max()
                .unwrap_or(0);
            // Sell and refuel at the crew's outpost once claimed (a short jump), else in town
            // (the jump home): the town pays in full, an outpost keeps its cut.
            let at_outpost = claimed.contains(&descent) && outpost_row <= face;
            let (sale, jump) = if at_outpost {
                (
                    f64::from(OUTPOST_SALE_PERCENT[1]) / 100.0,
                    (base - outpost_row).abs(),
                )
            } else {
                (1.0, base)
            };
            let tank = f64::from(fuel_capacity_for_level(rig.tank));
            let preview = trip(&rig, stratum, base, face, overhead);
            // The tank must carry an everyday trip with a reserve.
            let tank_short = preview.fuel > tank * 0.6;
            let lacks = |rig: &Rig| {
                let (drill, hull, heat, haul, gate, next_descent) = needs;
                let mut wanted: Vec<UpgradeKind> = [
                    (UpgradeKind::Drill, drill),
                    (UpgradeKind::Hull, hull),
                    (UpgradeKind::Radiator, heat),
                    (UpgradeKind::CargoBay, haul),
                    (UpgradeKind::FuelTank, haul),
                    (UpgradeKind::Engine, haul),
                ]
                .into_iter()
                .filter(|(kind, tier)| rig.level(*kind) < (*tier).min(cap))
                .map(|(kind, _)| kind)
                .collect();
                if tank_short && rig.tank < cap && !wanted.contains(&UpgradeKind::FuelTank) {
                    wanted.push(UpgradeKind::FuelTank);
                }
                // Seals rated for where the rig works (on sale once the Descent is reached).
                if rig.pressure < descent.max(next_descent).min(line_cap) {
                    wanted.push(UpgradeKind::PressureRating);
                }
                let system = gate.filter(|system| rig.systems & system.bit() == 0);
                (wanted, system)
            };
            // Buy what is needed, cheapest first, then comfort lines while the purse is fat.
            loop {
                let (core, system) = lacks(&rig);
                if let Some(system) = system
                    && core.is_empty()
                {
                    // Felt before it can be bought: a dip past the gate first.
                    if felt & system.bit() == 0 {
                        felt |= system.bit();
                        minutes += GATE_DIP_MINUTES;
                    }
                    let price = f64::from(system.price());
                    if rig.credits >= price {
                        rig.credits -= price;
                        rig.systems |= system.bit();
                        systems.push((system, (clock_minutes + minutes) / 60.0));
                        purchases += 1;
                        events.push((minutes, NoveltyKind::UpgradeTier, system.name().to_owned()));
                        continue;
                    }
                    break;
                }
                let essential = !core.is_empty();
                let wanted = core
                    .into_iter()
                    .chain(
                        [
                            UpgradeKind::CargoBay,
                            UpgradeKind::FuelTank,
                            UpgradeKind::Engine,
                            UpgradeKind::Drill,
                        ]
                        .into_iter()
                        // Wave 8: a player buys the bay, tank and drill that make trips pay
                        // (about a tier per stratum), not only what the next stratum demands.
                        .filter(|kind| rig.level(*kind) < comfort.max(rig.drill).min(cap)),
                    )
                    .chain((rig.lamp < line_cap).then_some(UpgradeKind::Headlamp))
                    .min_by_key(|kind| upgrade_cost_for(*kind, rig.level(*kind), rules));
                let Some(kind) = wanted else {
                    break;
                };
                let shop = f64::from(upgrade_cost_for(kind, rig.level(kind), rules));
                // A workshop part when the bay holds the resources for one.
                let part = part_for(kind).find(|recipe| rig.holds(recipe));
                let price = part.map_or(shop, |_| {
                    f64::from(crate::workshop::part_credits(
                        u32::try_from(shop as u64).unwrap_or(u32::MAX),
                    ))
                });
                if rig.credits >= price && (essential || rig.credits >= price * 1.5) {
                    rig.credits -= price;
                    if let Some(recipe) = part {
                        rig.spend(recipe);
                        crafted += 1;
                    }
                    *rig.level_mut(kind) += 1;
                    purchases += 1;
                    events.push((
                        minutes,
                        NoveltyKind::UpgradeTier,
                        format!(
                            "{} tier {}{}",
                            kind.name(),
                            rig.level(kind),
                            if part.is_some() { " (crafted)" } else { "" }
                        ),
                    ));
                } else {
                    break;
                }
            }
            if !claimed.contains(&descent) && outpost_row <= face && descent > 0 {
                let price = f64::from(scaled_price(OUTPOST_CLAIM_PRICE, descent));
                if rig.credits >= price * 3.0 {
                    rig.credits -= price;
                    claimed.push(descent);
                }
            }
            let (core, system) = lacks(&rig);
            if core.is_empty() && system.is_none() && trips > 0 {
                break;
            }
            // One mining trip.
            let run = trip(&rig, stratum, base, face, overhead);
            trips += 1;
            everyday = everyday.max(run.fuel / tank * 100.0);
            if let Some(slot) = rig.layer_units.get_mut(index) {
                *slot += run.layer_units;
            }
            let hull = f64::from(hull_capacity_for_level(rig.hull));
            // Two jumps on the beacon network (out and back), and the fuel to refill.
            let (jump_fuel, jump_credits) = if jump > 0 {
                let (fuel, credits) = travel_cost(jump);
                (f64::from(fuel) * 2.0, f64::from(credits) * 2.0)
            } else {
                (0.0, 0.0)
            };
            let refuel = (run.fuel + jump_fuel) * f64::from(OUTPOST_FUEL_UNIT[1]);
            let wrecked = run.damage > hull * 0.8;
            let gain = if wrecked {
                deaths += 1;
                let fee = (50.0 + 3.0 * f64::from(face)).min(rig.credits);
                run.credits * sale * 0.5 - fee - refuel
            } else {
                run.credits * sale - refuel - jump_credits
            };
            rig.credits = (rig.credits + gain).max(0.0);
            if gain <= 0.0 {
                stalled += 1;
                if stalled > 40 {
                    break;
                }
            }
            // A wreck teaches: hull first.
            if wrecked && rig.hull < cap {
                let price = f64::from(upgrade_cost_for(UpgradeKind::Hull, rig.hull, rules));
                if rig.credits >= price {
                    rig.credits -= price;
                    rig.hull += 1;
                    purchases += 1;
                    events.push((
                        minutes,
                        NoveltyKind::UpgradeTier,
                        format!("Hull Plating tier {}", rig.hull),
                    ));
                }
            }
            minutes += run.seconds / 60.0;
        }
        // The stratum's authored content: its markers spread over its time (arrival first),
        // each costing its share of the content minutes.
        minutes += stratum.content_minutes;
        let content: Vec<&Novelty> = stratum.markers.iter().collect();
        let count = content.len().max(1) as f64;
        for (slot, marker) in content.iter().enumerate() {
            let at = if marker.kind == NoveltyKind::Arrival {
                0.0
            } else {
                minutes * slot as f64 / count
            };
            events.push((at, marker.kind, marker.name.clone()));
        }
        let leaving_descent = strata
            .get(index + 1)
            .is_none_or(|next| next.descent != descent);
        if leaving_descent {
            events.push((minutes, NoveltyKind::Gate, "gate".to_owned()));
        }
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        // The gaps: from the last new thing before this stratum, between its new things, and
        // to its end (the next stratum's arrival is new).
        let mut longest = 0.0_f64;
        for (at, ..) in &events {
            let absolute = start + at;
            let gap = absolute - last_novelty;
            gaps.push(gap);
            longest = longest.max(gap);
            last_novelty = absolute;
        }
        clock_minutes += minutes;
        let tail = clock_minutes - last_novelty;
        longest = longest.max(tail);
        deaths_total += deaths;
        if leaving_descent {
            descent_hours[usize::from(descent)] = clock_minutes / 60.0;
            if let Some(span) = descents.get(usize::from(descent)) {
                let rows = f64::from(span.rows() - 40);
                let climb = STOCK_CLIMB_ROWS_PER_SECOND
                    * f64::from(engine_power(rig.engine).min(crate::player::ENGINE_CLIMB_CAP));
                let fuel = (rows / climb + 120.0)
                    * f64::from(AUTHORITATIVE_FUEL_BURN_PER_SECOND)
                    * f64::from(fuel_burn_efficiency(rig.tank))
                    * pressure_factor(rig.pressure, descent)
                    + 300.0 * DIG_FUEL_PER_TILE;
                expedition[usize::from(descent)] =
                    fuel / f64::from(fuel_capacity_for_level(rig.tank)) * 100.0;
            }
        }
        runs.push(StratumRun {
            stratum: stratum.clone(),
            minutes,
            end_hours: clock_minutes / 60.0,
            trips,
            deaths,
            drill_tier,
            hull_tier,
            everyday_fuel_percent: everyday,
            longest_gap_minutes: longest,
            purchases,
            crafted,
            events,
        });
    }
    DescentRun {
        strata: runs,
        descent_hours,
        expedition_fuel_percent: expedition,
        systems,
        gaps,
        dig_overhead: overhead,
        deaths: deaths_total,
    }
}

/// The Descents the pacing check covers (I-III: Wave 8's authored content).
pub const PACING_DESCENTS: u8 = 3;

/// §14's target hours at the end of each Descent.
pub const TARGET_HOURS: [(f64, f64); DESCENT_COUNT] = [
    (5.0, 8.0),
    (13.0, 18.0),
    (22.0, 29.0),
    (31.0, 40.0),
    (42.0, 53.0),
    (53.0, 66.0),
    (66.0, 82.0),
    (75.0, 90.0),
];

/// Builds the report section for `run`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "the section's tables and notes in order"
)]
pub fn descent_section(run: &DescentRun) -> ReportSection {
    let mut section = ReportSection::new(
        "Descent (Part III F10)",
        format!(
            "A bot plays the Deep Descent through the stratum registry's 41 strata (the default \
             seed's V11 layout) on the Descent ladders, rig systems and logistics prices. The drill moves \
             on to the next tile in {:.2} s besides chipping (measured through the tick). Whole run: {:.1} h (target 60-90 h); {:.0}% of \
             the gaps between new things are 40 minutes or less (longest {:.0} min); {} deaths.",
            run.dig_overhead,
            run.total_hours(),
            run.gaps_within_40(),
            run.longest_gap(),
            run.deaths
        ),
    );
    let mut per_descent = Table::new(
        "Per Descent: hours at its end against the plan's target, and fuel on everyday trips against an expedition",
        &[
            "Descent",
            "Ends at (h)",
            "Target (h)",
            "Everyday trip fuel (max % of tank)",
            "Expedition fuel (% of tank)",
            "Deaths",
        ],
    );
    for (index, span) in PlanLayout.descents().iter().enumerate() {
        let runs: Vec<&StratumRun> = run
            .strata
            .iter()
            .filter(|stratum| usize::from(stratum.stratum.descent) == index)
            .collect();
        if runs.is_empty() {
            continue;
        }
        let everyday = runs
            .iter()
            .map(|stratum| stratum.everyday_fuel_percent)
            .fold(0.0, f64::max);
        let deaths: u32 = runs.iter().map(|stratum| stratum.deaths).sum();
        let (low, high) = TARGET_HOURS[index];
        per_descent.row(vec![
            format!("{} {}", span.numeral(), span.name),
            format!("{:.1}", run.descent_hours[index]),
            format!("{low:.0}-{high:.0}"),
            format!("{everyday:.0}"),
            format!("{:.0}", run.expedition_fuel_percent[index]),
            deaths.to_string(),
        ]);
        section.metric(
            format!("descent.d{index}.end_hours"),
            run.descent_hours[index],
        );
        section.metric(format!("descent.d{index}.everyday_fuel_percent"), everyday);
        section.metric(
            format!("descent.d{index}.expedition_fuel_percent"),
            run.expedition_fuel_percent[index],
        );
    }
    section.tables.push(per_descent);
    let mut per_stratum = Table::new(
        "Per stratum",
        &[
            "Stratum",
            "Rows",
            "Minutes",
            "Leaves at (h)",
            "Drill / hull on arrival",
            "Trips",
            "Deaths",
            "Longest gap (min)",
        ],
    );
    for stratum in &run.strata {
        per_stratum.row(vec![
            format!(
                "{} {}",
                crate::depth_progression::numeral(stratum.stratum.descent),
                stratum.stratum.name
            ),
            stratum.stratum.rows().to_string(),
            format!("{:.0}", stratum.minutes),
            format!("{:.1}", stratum.end_hours),
            format!("{} / {}", stratum.drill_tier, stratum.hull_tier),
            stratum.trips.to_string(),
            stratum.deaths.to_string(),
            format!("{:.0}", stratum.longest_gap_minutes),
        ]);
    }
    section.tables.push(per_stratum);
    // Wave 8: the "something new every 20-40 minutes" check on Descents I-III, from the
    // registry's real content markers and what the bot bought on the way.
    let mut pacing = Table::new(
        "Descent I-III pacing: new things per stratum (real content markers), the longest wait, deaths and fuel stress",
        &[
            "Stratum",
            "Minutes",
            "New things",
            "Creatures",
            "Mechanics",
            "Landmarks",
            "Story",
            "Upgrades (crafted)",
            "Effects",
            "Longest gap (min)",
            "20-40 min check",
            "Deaths",
            "Everyday fuel (% of tank)",
        ],
    );
    for stratum in run
        .strata
        .iter()
        .filter(|stratum| stratum.stratum.descent < PACING_DESCENTS)
    {
        pacing.row(vec![
            format!(
                "{} {}",
                crate::depth_progression::numeral(stratum.stratum.descent),
                stratum.stratum.name
            ),
            format!("{:.0}", stratum.minutes),
            stratum.events.len().to_string(),
            stratum.count(NoveltyKind::Creature).to_string(),
            stratum.count(NoveltyKind::Mechanic).to_string(),
            stratum.count(NoveltyKind::Landmark).to_string(),
            stratum.count(NoveltyKind::StoryLog).to_string(),
            format!(
                "{} ({})",
                stratum.count(NoveltyKind::UpgradeTier),
                stratum.crafted
            ),
            stratum.count(NoveltyKind::Effect).to_string(),
            format!("{:.0}", stratum.longest_gap_minutes),
            if stratum.paced() { "pass" } else { "FAIL" }.to_owned(),
            stratum.deaths.to_string(),
            format!("{:.0}", stratum.everyday_fuel_percent),
        ]);
    }
    if !pacing.rows.is_empty() {
        section.tables.push(pacing);
    }
    let failures = run.pacing_failures(PACING_DESCENTS);
    section.notes.push(if failures.is_empty() {
        "Descents I-III pass the pacing check: something new at least every 40 minutes, at most \
         two deaths a stratum, everyday trips within one tank."
            .to_owned()
    } else {
        format!(
            "Descents I-III pacing check fails in: {}.",
            failures
                .iter()
                .map(|(name, why)| format!("{name} ({why})"))
                .collect::<Vec<_>>()
                .join("; ")
        )
    });
    section.metric(
        "descent.pacing_passes_d1_3",
        if run.pacing_passes(PACING_DESCENTS) {
            1.0
        } else {
            0.0
        },
    );
    let mut systems = String::new();
    for (system, hours) in &run.systems {
        let _ = write!(systems, "{} at {hours:.1} h; ", system.name());
    }
    if !systems.is_empty() {
        section
            .notes
            .push(format!("Gate systems bought: {systems}"));
    }
    let long: Vec<String> = run
        .strata
        .iter()
        .filter(|stratum| stratum.longest_gap_minutes > 40.0)
        .map(|stratum| stratum.stratum.name.to_owned())
        .collect();
    section.notes.push(if long.is_empty() {
        "Every stratum brings something new within 40 minutes.".to_owned()
    } else {
        format!("Strata with a gap over 40 minutes: {}.", long.join(", "))
    });
    section.metric("descent.total_hours", run.total_hours());
    section.metric("descent.gaps_within_40_percent", run.gaps_within_40());
    section.metric("descent.longest_gap_minutes", run.longest_gap());
    section.metric("descent.deaths", f64::from(run.deaths));
    section.metric("descent.dig_overhead", run.dig_overhead);
    section
}

/// The descent probe (`--only descent`).
pub struct DescentProbe;

impl BalanceProbe for DescentProbe {
    fn name(&self) -> &'static str {
        "descent"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let config = DescentConfig {
            tick_rate_hz: context.config.tick_rate_hz,
            ..DescentConfig::default()
        };
        // The real stratum registry on the default seed's Deep Descent world (Wave 7
        // integration); `PlanStrata` stays for comparison and tests.
        descent_section(&run_descent(&RegistryStrata::default_world(), &config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_registry_covers_41_strata_to_the_heart() {
        let strata = PlanStrata.strata();
        assert_eq!(strata.len(), crate::depth_progression::STRATUM_COUNT);
        assert_eq!(strata.last().map(|stratum| stratum.bottom), Some(13_000));
        for pair in strata.windows(2) {
            assert!(pair[1].rock_tier >= pair[0].rock_tier);
            assert!(
                pair[1].ore_value >= pair[0].ore_value * 0.99,
                "{}",
                pair[1].name
            );
        }
    }

    #[test]
    fn the_real_registry_covers_41_seeded_strata_to_the_heart() {
        let registry = RegistryStrata::default_world();
        let strata = registry.strata();
        assert_eq!(strata.len(), crate::depth_progression::STRATUM_COUNT);
        assert_eq!(strata[0].top, 0);
        assert_eq!(strata[0].name, "Topsoil");
        assert_eq!(strata.last().map(|stratum| stratum.name), Some("The Heart"));
        let bottom = strata.last().map_or(0, |stratum| stratum.bottom);
        assert!((12_900..=13_100).contains(&bottom), "{bottom}");
        for pair in strata.windows(2) {
            assert_eq!(pair[0].bottom, pair[1].top, "contiguous");
            assert!(pair[1].descent >= pair[0].descent);
        }
        assert!(strata.iter().all(|stratum| stratum.ore_value > 0.0));
        let descents = registry.descents();
        assert_eq!(descents.len(), DESCENT_COUNT);
        assert_eq!(descents.last().map(|span| span.bottom), Some(bottom));
        // A short run on it.
        let config = DescentConfig {
            strata_limit: Some(6),
            calibrate: false,
            tick_rate_hz: 60,
        };
        let run = run_descent(&registry, &config);
        assert_eq!(run.strata.len(), 6);
        assert!(run.total_hours() > 1.0);
    }

    #[test]
    fn a_short_descent_runs_and_reports() {
        let config = DescentConfig {
            strata_limit: Some(8),
            calibrate: false,
            tick_rate_hz: 60,
        };
        let run = run_descent(&PlanStrata, &config);
        assert_eq!(run.strata.len(), 8);
        assert!(run.total_hours() > 1.0, "{} h", run.total_hours());
        assert!(run.strata.iter().all(|stratum| stratum.minutes > 0.0));
        // Upgrades happen on the way down.
        let last = run.strata.last().expect("stratum");
        assert!(last.drill_tier > 1);
        let section = descent_section(&run);
        assert!(section.metrics.contains_key("descent.total_hours"));
        assert!(section.tables.len() >= 2);
        // Same config, same run.
        assert_eq!(run_descent(&PlanStrata, &config), run);
    }

    #[test]
    fn the_pacing_check_reads_real_content_markers_for_descents_one_to_three() {
        let registry = RegistryStrata::default_world();
        let strata = registry.strata();
        // Every stratum arrives, has its signature mechanic, and the Crust meets creatures.
        for stratum in &strata {
            assert_eq!(
                stratum.markers.first().map(|marker| marker.kind),
                Some(NoveltyKind::Arrival)
            );
            assert!(
                stratum
                    .markers
                    .iter()
                    .any(|marker| marker.kind == NoveltyKind::Mechanic),
                "{}",
                stratum.name
            );
        }
        assert!(strata[..5].iter().any(|stratum| {
            stratum
                .markers
                .iter()
                .any(|marker| marker.kind == NoveltyKind::Creature)
        }));
        // Each Descent opens with its story beat, and the workshop's recipes are new things.
        assert!(
            strata[5]
                .markers
                .iter()
                .any(|marker| marker.kind == NoveltyKind::StoryLog)
        );
        assert!(
            strata[1]
                .markers
                .iter()
                .any(|marker| marker.name.starts_with("recipe: "))
        );
        // A short run over Descents I-III: the table, the check and its metric.
        let config = DescentConfig {
            strata_limit: Some(17),
            calibrate: false,
            tick_rate_hz: 60,
        };
        let run = run_descent(&registry, &config);
        assert_eq!(run.strata.len(), 17);
        for stratum in &run.strata {
            assert!(stratum.events.windows(2).all(|pair| pair[0].0 <= pair[1].0));
            assert!(stratum.longest_gap_minutes > 0.0);
        }
        // The bot buys seals for the Old Deep and fits workshop parts on the way.
        assert!(run.strata.iter().any(|stratum| {
            stratum.events.iter().any(|(_, kind, name)| {
                *kind == NoveltyKind::UpgradeTier && name.starts_with("Pressure Rating")
            })
        }));
        assert!(
            run.strata
                .iter()
                .map(|stratum| stratum.crafted)
                .sum::<u32>()
                > 0
        );
        let section = descent_section(&run);
        assert!(section.metrics.contains_key("descent.pacing_passes_d1_3"));
        assert!(
            section
                .tables
                .iter()
                .any(|table| table.caption.starts_with("Descent I-III pacing"))
        );
        let failures = run.pacing_failures(PACING_DESCENTS);
        assert_eq!(run.pacing_passes(PACING_DESCENTS), failures.is_empty());
    }

    #[test]
    fn the_tick_measures_the_drill_close_to_its_formula() {
        let measured = measure_dig(10, 12, 60).expect("cuts hard rock");
        let formula = chip_seconds(TileKind::HardRock, 1.0, 10);
        assert!(
            measured >= formula * 0.9 && measured < formula + 1.0,
            "{measured} vs {formula}"
        );
    }

    /// `cargo test --release -p fatherload_simulation descent_report -- --ignored --nocapture`
    #[test]
    #[ignore = "prints the full descent report"]
    fn descent_report() {
        let run = run_descent(&RegistryStrata::default_world(), &DescentConfig::default());
        let report = super::super::BalanceReport {
            header: "Descent probe".to_owned(),
            sections: vec![descent_section(&run)],
        };
        println!("{}", report.to_markdown());
    }
}
