//! Story seeds: character needs + world facts → arcs built from authored beats.
//!
//! An [`ArcTemplate`] is authored data: what can trigger it (a want, a fear, a relationship, an
//! absent person, a village problem, a value, a quirk, a player history fact), the slots it
//! binds (a person, places), its beats (stages of the quest engine's own
//! [`Objective`] primitives over [`TargetSpec`]s), outcomes with branching effects, and pitch
//! and thanks lines with a variant for every [`Tone`]. Every arc states a hook, a twist and a
//! consequence (lint-checked).
//!
//! [`story_seeds`] scores every (resident, arc) pair whose trigger matches and whose slots
//! bind, then keeps the best seed per resident (and at most two per arc), so a village's seeds
//! read like its people. [`StorySeed::quest_def`] turns a seed into a
//! [`QuestDef`]: the quest engine binds and runs it unchanged. (Pinning a place slot to its exact
//! feature, character givers, outcomes and the bulk of the arcs are Wave 3 / WP-STORY; this
//! module is the data model and the first templates.)

use serde::{Deserialize, Serialize};

use super::{
    PeopleRng,
    character::{AbsentFate, Character, CharacterId, Role, WantFocus},
    dialogue::{
        grammar::{self, Vars},
        lines::ToneLines,
    },
    memory::MemoryFact,
    population::{Population, RelationshipKind},
    salt,
    traits::{Fear, Quirk, Tone, Value, Want},
};
use crate::{
    poi::PoiKind,
    quest::{
        Objective, QuestDef, QuestGiver, QuestId, QuestReward, QuestText, QuestTier, TargetSpec,
        Twist,
    },
    worldgen::{Band, FeatureKind, WorldFeature},
};

pub mod arcs;

pub use arcs::ARC_TEMPLATES;

/// Stable id of an arc template (saved in memories; never reused).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ArcId(pub u16);

impl ArcId {
    #[must_use]
    pub fn template(self) -> Option<&'static ArcTemplate> {
        ARC_TEMPLATES.iter().find(|template| template.id == self)
    }
}

/// A kind of place the world offers stories.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum WorldFactKind {
    Derelict,
    SealedVault,
    Formation,
    RivalClaim,
    SupplyCache,
    Cavern,
    DeepChamber,
    Motherlode,
    ArtifactBed,
    HazardPocket,
}

impl WorldFactKind {
    pub const ALL: [Self; 10] = [
        Self::Derelict,
        Self::SealedVault,
        Self::Formation,
        Self::RivalClaim,
        Self::SupplyCache,
        Self::Cavern,
        Self::DeepChamber,
        Self::Motherlode,
        Self::ArtifactBed,
        Self::HazardPocket,
    ];

    /// How people describe it (no article; templates add one with `{a place}`).
    #[must_use]
    pub const fn nouns(self) -> &'static [&'static str] {
        match self {
            Self::Derelict => &["old derelict rig", "abandoned rig", "rusted-out survey rig"],
            Self::SealedVault => &[
                "sealed vault",
                "vault door nobody's opened",
                "old-world vault",
            ],
            Self::Formation => &[
                "strange rock formation",
                "crystal formation",
                "formation that hums",
            ],
            Self::RivalClaim => &[
                "claim-jumper's camp",
                "abandoned claim",
                "staked-out rival claim",
            ],
            Self::SupplyCache => &["forgotten supply cache", "stash of old supplies"],
            Self::Cavern => &["cavern big enough to fly in", "huge open cave"],
            Self::DeepChamber => &["deep chamber", "hollow nobody's mapped"],
            Self::Motherlode => &["motherlode", "seam fat with ore"],
            Self::ArtifactBed => &["bed of old relics", "nest of strange artifacts"],
            Self::HazardPocket => &["gas pocket", "nasty hazard pocket"],
        }
    }

    /// The fact kind a generated feature represents.
    #[must_use]
    pub const fn of_feature(kind: FeatureKind, poi: Option<PoiKind>) -> Option<Self> {
        match (kind, poi) {
            (_, Some(PoiKind::DerelictRig)) => Some(Self::Derelict),
            (_, Some(PoiKind::Vault)) => Some(Self::SealedVault),
            (_, Some(PoiKind::Formation)) => Some(Self::Formation),
            (_, Some(PoiKind::RivalClaim)) => Some(Self::RivalClaim),
            (_, Some(PoiKind::SupplyCache)) => Some(Self::SupplyCache),
            (FeatureKind::LargeCavern, None) => Some(Self::Cavern),
            (FeatureKind::Chamber, None) => Some(Self::DeepChamber),
            (FeatureKind::Motherlode, None) => Some(Self::Motherlode),
            (FeatureKind::ArtifactBed, None) => Some(Self::ArtifactBed),
            (FeatureKind::HazardCluster, None) => Some(Self::HazardPocket),
            _ => None,
        }
    }

    /// The fact kind a quest target spec binds to.
    #[must_use]
    pub const fn of_target(spec: TargetSpec) -> Option<Self> {
        match spec {
            TargetSpec::Poi { kind, .. } => {
                Self::of_feature(FeatureKind::PointOfInterest, Some(kind))
            }
            TargetSpec::Feature { kind, .. } => Self::of_feature(kind, None),
        }
    }
}

/// A fact about the world a story can hang on: a kind of place, its band and depth.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct WorldFact {
    pub kind: WorldFactKind,
    /// [`Band::index`].
    pub band: u8,
    /// The tile row of its centre.
    pub row: i32,
    /// The feature it came from, when it came from the feature index.
    pub feature: Option<crate::worldgen::FeatureId>,
}

impl WorldFact {
    /// The fact a generated feature represents (`None` for seams and prefab slots).
    #[must_use]
    pub fn from_feature(feature: &WorldFeature) -> Option<Self> {
        let kind = WorldFactKind::of_feature(feature.kind, feature.poi_kind())?;
        Some(Self {
            kind,
            band: u8::try_from(feature.band.index()).unwrap_or(u8::MAX),
            row: feature.center.y,
            feature: Some(feature.id),
        })
    }

    /// Facts for every story-worthy feature in a world's feature index.
    #[must_use]
    pub fn from_features(features: &[WorldFeature]) -> Vec<Self> {
        features.iter().filter_map(Self::from_feature).collect()
    }

    /// "the Slate Shelves".
    #[must_use]
    pub fn band_name(self) -> String {
        format!("the {}", Band::from_index(usize::from(self.band)).name())
    }

    /// Depth in metres (one tile row is a metre on the depth gauge).
    #[must_use]
    pub const fn depth(self) -> i32 {
        if self.row < 0 { 0 } else { self.row }
    }
}

/// A village's problem (WP-VILLAGE owns village state; this is the story input).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum VillageProblem {
    /// The old mine caved in and the village lost its living.
    SealedMine,
    /// Someone went missing.
    MissingPerson,
    /// Something is coming up out of the caves at night.
    CaveSwarm,
    /// The well ran dry.
    DryWell,
    /// Two families are feuding.
    Feud,
}

impl VillageProblem {
    pub const ALL: [Self; 5] = [
        Self::SealedMine,
        Self::MissingPerson,
        Self::CaveSwarm,
        Self::DryWell,
        Self::Feud,
    ];
}

/// A player history fact an arc can grow from.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HistoryKind {
    FoundArtifact,
    FoundPerson,
    ReachedDepth,
    HelpedVillage,
}

impl HistoryKind {
    #[must_use]
    pub const fn matches(self, fact: &MemoryFact) -> bool {
        matches!(
            (self, fact),
            (Self::FoundArtifact, MemoryFact::FoundArtifact { .. })
                | (Self::FoundPerson, MemoryFact::FoundPerson { .. })
                | (Self::ReachedDepth, MemoryFact::ReachedDepth { .. })
                | (Self::HelpedVillage, MemoryFact::HelpedVillage)
        )
    }
}

/// What can make a character offer an arc.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Trigger {
    Want(Want),
    Fear(Fear),
    Value(Value),
    Quirk(Quirk),
    /// They have a neighbour they relate to this way.
    Relationship(RelationshipKind),
    /// They talk about an absent person with this fate.
    Absent(AbsentFate),
    /// Their village has this problem.
    Problem(VillageProblem),
    /// The player has done this.
    History(HistoryKind),
}

impl Trigger {
    /// How strongly a match on this trigger pulls (a want beats a passing value).
    #[must_use]
    pub const fn weight(self) -> u32 {
        match self {
            Self::Want(_) => 30,
            Self::Absent(_) => 28,
            Self::Problem(_) => 24,
            Self::Relationship(_) => 22,
            Self::Fear(_) => 20,
            Self::History(_) => 18,
            Self::Quirk(_) => 14,
            Self::Value(_) => 10,
        }
    }
}

/// Why a seed exists (for the report and for later dialogue).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SeedSource {
    Want(Want),
    Fear(Fear),
    Value(Value),
    Quirk(Quirk),
    Relationship {
        kind: RelationshipKind,
        other: CharacterId,
    },
    Absent {
        fate: AbsentFate,
        name: String,
    },
    Problem(VillageProblem),
    History(String),
}

/// A slot an arc binds when seeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    /// The giver's absent person with this fate (`{focus}`, `{focus_relation}`).
    Absent(AbsentFate),
    /// A neighbour the giver relates to this way, or an absent stand-in with the matching fate
    /// (`{other}`, `{other_word}`).
    Neighbour(RelationshipKind),
    /// A world fact of `kind` for quest target `target` (`{place}`, `{band}`, `{depth}`; the
    /// first place slot fills them).
    Place { target: u8, kind: WorldFactKind },
}

/// What a slot was bound to.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum SlotBinding {
    Absent {
        name: String,
        relation: String,
    },
    Neighbour {
        id: Option<CharacterId>,
        name: String,
        relation: String,
    },
    Place(WorldFact),
}

/// When an outcome applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutcomeWhen {
    /// The arc's quest is completed (the default ending).
    Completed,
    /// The player picks this ending at turn-in.
    Choice,
    /// The arc is abandoned or failed.
    Abandoned,
}

/// An effect of an outcome on the world (applied by WP-STORY).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArcEffect {
    /// Reputation with the giver.
    Reputation(i16),
    /// A mood shift for the giver.
    Mood(i16),
    /// The giver's village prospers (or shrinks).
    Prosperity(i8),
    /// The giver's relationship with the neighbour slot becomes this kind (`None`: ends).
    Relationship(Option<RelationshipKind>),
    /// The giver's unique offer becomes available.
    UnlockOffer,
    /// The giver remembers it (a [`MemoryFact::ArcCompleted`], or the fitting specific fact).
    Remember,
    /// Another arc becomes available from the same giver.
    FollowUp(ArcId),
}

/// One ending of an arc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArcOutcome {
    pub label: &'static str,
    pub when: OutcomeWhen,
    pub effects: &'static [ArcEffect],
}

/// An authored arc. See the module docs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArcTemplate {
    pub id: ArcId,
    pub title: &'static str,
    /// The setup that makes it worth doing.
    pub hook: &'static str,
    /// The turn the story takes.
    pub twist: &'static str,
    /// How the world changes.
    pub consequence: &'static str,
    pub tier: QuestTier,
    /// Any one of these can seed it.
    pub triggers: &'static [Trigger],
    /// Roles that suit giving it (a scoring nudge).
    pub roles: &'static [Role],
    pub slots: &'static [Slot],
    /// Quest targets (bound by the quest engine).
    pub targets: &'static [TargetSpec],
    /// Quest stages: one beat each.
    pub stages: &'static [&'static [Objective]],
    /// One title per stage.
    pub beats: &'static [&'static str],
    pub twists: &'static [Twist],
    pub reward: QuestReward,
    pub outcomes: &'static [ArcOutcome],
    /// The giver's pitch (arc variables: see [`ARC_VARS`]).
    pub pitch: ToneLines,
    /// The giver's thanks on completion.
    pub thanks: ToneLines,
    /// Quest log lines (neutral, no variables).
    pub log: QuestText,
}

/// Variables arc lines may use.
pub const ARC_VARS: &[&str] = &[
    "giver",
    "focus",
    "focus_relation",
    "other",
    "other_word",
    "place",
    "band",
    "depth",
    "village",
    "player",
];

impl ArcTemplate {
    /// The arc variables its slots provide (plus the always-present ones).
    #[must_use]
    pub fn provided_vars(&self) -> Vec<&'static str> {
        let mut vars = vec!["giver", "village", "player"];
        for slot in self.slots {
            match slot {
                Slot::Absent(_) => vars.extend(["focus", "focus_relation"]),
                Slot::Neighbour(_) => vars.extend(["other", "other_word"]),
                Slot::Place { .. } => vars.extend(["place", "band", "depth"]),
            }
        }
        vars
    }

    /// The arc as a quest definition the engine can bind and run, handed out by `giver` (a
    /// keeper for now; WP-STORY adds character givers).
    #[must_use]
    pub const fn quest_def(&'static self, id: QuestId, giver: QuestGiver) -> QuestDef {
        QuestDef {
            id,
            title: self.title,
            giver,
            tier: self.tier,
            hook: self.hook,
            prerequisites: &[],
            targets: self.targets,
            stages: self.stages,
            twists: self.twists,
            reward: self.reward,
            text: self.log,
            repeatable: false,
        }
    }
}

/// A seeded story: an arc, the resident offering it, why, and what its slots bound to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorySeed {
    pub arc: ArcId,
    pub giver: CharacterId,
    pub source: SeedSource,
    /// One per [`ArcTemplate::slots`] entry.
    pub bindings: Vec<SlotBinding>,
    pub score: u32,
}

impl StorySeed {
    /// The arc template.
    ///
    /// # Panics
    ///
    /// Never for seeds made by [`story_seeds`].
    #[must_use]
    pub fn template(&self) -> &'static ArcTemplate {
        self.arc.template().expect("seeded arcs exist")
    }

    /// Variables for the arc's lines.
    #[must_use]
    pub fn vars(&self, giver: &Character, village: &str, player: &str) -> Vars {
        let mut vars = Vars::new();
        vars.set("giver", giver.name.short());
        vars.set("village", village);
        vars.set("player", player);
        let mut placed = false;
        for binding in &self.bindings {
            match binding {
                SlotBinding::Absent { name, relation } => {
                    vars.set("focus", name.clone());
                    vars.set("focus_relation", relation.clone());
                }
                SlotBinding::Neighbour { name, relation, .. } => {
                    vars.set("other", name.clone());
                    vars.set("other_word", relation.clone());
                }
                SlotBinding::Place(fact) if !placed => {
                    placed = true;
                    vars.set("place", fact.kind.nouns()[0]);
                    vars.set("band", fact.band_name());
                    vars.set("depth", fact.depth().to_string());
                }
                SlotBinding::Place(_) => {}
            }
        }
        vars
    }

    fn line(&self, lines: &ToneLines, giver: &Character, village: &str, player: &str) -> String {
        let mut rng = PeopleRng::new(giver.voice_seed, salt::STORY, u64::from(self.arc.0), 1);
        let tone: Tone = giver.tone;
        let template = rng.pick(lines.get(tone));
        grammar::expand(template, &self.vars(giver, village, player), &mut rng)
    }

    /// The giver's pitch, in their voice.
    #[must_use]
    pub fn pitch(&self, giver: &Character, village: &str, player: &str) -> String {
        self.line(&self.template().pitch, giver, village, player)
    }

    /// The giver's thanks, in their voice.
    #[must_use]
    pub fn thanks(&self, giver: &Character, village: &str, player: &str) -> String {
        self.line(&self.template().thanks, giver, village, player)
    }

    /// The seed as a quest definition (see [`ArcTemplate::quest_def`]).
    #[must_use]
    pub fn quest_def(&self, id: QuestId, giver: QuestGiver) -> QuestDef {
        self.template().quest_def(id, giver)
    }
}

/// The best matching trigger of `template` for `character`, as (weight, source).
fn best_trigger(
    template: &ArcTemplate,
    character: &Character,
    population: &Population,
    problems: &[VillageProblem],
    history: &[MemoryFact],
) -> Option<(u32, SeedSource)> {
    let mut best: Option<(u32, SeedSource)> = None;
    for trigger in template.triggers {
        let source = match *trigger {
            Trigger::Want(want) if character.want == want => Some(SeedSource::Want(want)),
            Trigger::Fear(fear) if character.fear == fear => Some(SeedSource::Fear(fear)),
            Trigger::Value(value) if character.values.contains(&value) => {
                Some(SeedSource::Value(value))
            }
            Trigger::Quirk(quirk) if character.quirks.contains(&quirk) => {
                Some(SeedSource::Quirk(quirk))
            }
            Trigger::Relationship(kind) => population
                .relationships_of(character.id)
                .find(|edge| edge.kind == kind)
                .map(|edge| SeedSource::Relationship {
                    kind,
                    other: edge.to,
                }),
            Trigger::Absent(fate) => character
                .absent
                .iter()
                .find(|person| person.fate == fate)
                .map(|person| SeedSource::Absent {
                    fate,
                    name: person.name.short().to_owned(),
                }),
            Trigger::Problem(problem) if problems.contains(&problem) => {
                Some(SeedSource::Problem(problem))
            }
            Trigger::History(kind) => history
                .iter()
                .find(|fact| kind.matches(fact))
                .map(|fact| SeedSource::History(format!("{fact:?}"))),
            _ => None,
        };
        if let Some(source) = source {
            let weight = trigger.weight();
            if best.as_ref().is_none_or(|(score, _)| weight > *score) {
                best = Some((weight, source));
            }
        }
    }
    best
}

const fn neighbour_fallback(kind: RelationshipKind) -> Option<AbsentFate> {
    match kind {
        RelationshipKind::Rival => Some(AbsentFate::OldRival),
        RelationshipKind::Owes => Some(AbsentFate::Creditor),
        RelationshipKind::Estranged => Some(AbsentFate::Estranged),
        _ => None,
    }
}

/// Binds every slot of `template` for `character`, or `None` if one cannot bind.
fn bind_slots(
    template: &ArcTemplate,
    character: &Character,
    population: &Population,
    facts: &[WorldFact],
    rng: &mut PeopleRng,
) -> Option<Vec<SlotBinding>> {
    let mut bindings = Vec::with_capacity(template.slots.len());
    for slot in template.slots {
        let binding = match *slot {
            Slot::Absent(fate) => {
                let person = character.absent.iter().find(|person| person.fate == fate)?;
                SlotBinding::Absent {
                    name: person.name.short().to_owned(),
                    relation: person.relation.clone(),
                }
            }
            Slot::Neighbour(kind) => {
                // The want's own focus first, then any neighbour of the kind, then an absent
                // stand-in.
                let focused = match character.want_focus {
                    WantFocus::Neighbour(id) => population
                        .relationship(character.id, id)
                        .filter(|edge| edge.kind == kind),
                    _ => None,
                };
                let edge = focused.or_else(|| {
                    population
                        .relationships_of(character.id)
                        .find(|edge| edge.kind == kind)
                });
                if let Some(edge) = edge {
                    let other = population.get(edge.to)?;
                    SlotBinding::Neighbour {
                        id: Some(other.id),
                        name: other.name.short().to_owned(),
                        relation: kind.word(other.pronouns).to_owned(),
                    }
                } else {
                    let fate = neighbour_fallback(kind)?;
                    let person = character.absent.iter().find(|person| person.fate == fate)?;
                    SlotBinding::Neighbour {
                        id: None,
                        name: person.name.short().to_owned(),
                        relation: person.relation.clone(),
                    }
                }
            }
            Slot::Place { target, kind } => {
                let spec = template.targets.get(usize::from(target))?;
                let (first, last) = spec.bands();
                let gap = |fact: &&WorldFact| {
                    u32::from(first.saturating_sub(fact.band))
                        + u32::from(fact.band.saturating_sub(last))
                };
                let matching: Vec<&WorldFact> =
                    facts.iter().filter(|fact| fact.kind == kind).collect();
                let nearest = matching.iter().map(gap).min();
                let options: Vec<&WorldFact> = matching
                    .into_iter()
                    .filter(|fact| Some(gap(fact)) == nearest)
                    .collect();
                if options.is_empty() {
                    // No world facts to hand (or none of the kind): describe the kind of place
                    // the quest engine will bind, in the middle of the wanted bands.
                    let band = first.midpoint(last);
                    SlotBinding::Place(WorldFact {
                        kind,
                        band,
                        row: nominal_row(band),
                        feature: None,
                    })
                } else {
                    SlotBinding::Place(**rng.pick(&options))
                }
            }
        };
        bindings.push(binding);
    }
    Some(bindings)
}

/// A rough classic-world row for the middle of a band (when no fact gives a real one).
const fn nominal_row(band: u8) -> i32 {
    match band {
        0 => 8,
        1 => 25,
        2 => 55,
        3 => 85,
        4 => 115,
        _ => 140 + (band as i32 - 5) * 40,
    }
}

/// Story seeds for a village: at most one per resident and two per arc, best first.
///
/// `facts` are places in the world (see [`WorldFact::from_features`]); `problems` the village's
/// troubles; `history` what the asking player is known for. `salt` reshuffles near-ties (a new
/// day, a new set of seeds).
#[must_use]
pub fn story_seeds(
    population: &Population,
    facts: &[WorldFact],
    problems: &[VillageProblem],
    history: &[MemoryFact],
    salt_value: u64,
) -> Vec<StorySeed> {
    let mut candidates = Vec::new();
    for character in &population.characters {
        for template in ARC_TEMPLATES {
            let Some((weight, source)) =
                best_trigger(template, character, population, problems, history)
            else {
                continue;
            };
            let mut rng = PeopleRng::new(
                character.voice_seed ^ salt_value,
                salt::STORY,
                u64::from(template.id.0),
                0,
            );
            let Some(bindings) = bind_slots(template, character, population, facts, &mut rng)
            else {
                continue;
            };
            let role_fit = if template.roles.contains(&character.role) {
                8
            } else {
                0
            };
            let jitter = u32::try_from(rng.below(10)).unwrap_or(0);
            candidates.push(StorySeed {
                arc: template.id,
                giver: character.id,
                source,
                bindings,
                score: weight + role_fit + jitter,
            });
        }
    }
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.giver.cmp(&b.giver))
            .then(a.arc.cmp(&b.arc))
    });
    let mut seeds: Vec<StorySeed> = Vec::new();
    for candidate in candidates {
        let giver_taken = seeds.iter().any(|seed| seed.giver == candidate.giver);
        let arc_count = seeds
            .iter()
            .filter(|seed| seed.arc == candidate.arc)
            .count();
        if !giver_taken && arc_count < 2 {
            seeds.push(candidate);
        }
    }
    seeds
}
