//! Story arcs at play time (WP-STORY): who offers what, binding an arc to the world from its
//! giver's settlement, what each player keeps, and the story text.
//!
//! Everything here is a pure function of the world settings, the terrain's feature index, the
//! giver (rebuilt from the seed) and the player's [`ArcLog`], so every peer agrees and the
//! story report (`--story-report --arcs`) samples exactly what play would offer.
//!
//! - **Offers** ([`offer_for`]): a person offers the follow-up they promised first, else their
//!   best-scoring arc ([`super::story_seeds`]' scoring: wants beat passing values, role fit
//!   nudges, a jitter salted by the [`OFFER_ROTATION_DAYS`] window) that the player has not
//!   finished with them, is not already running and whose prerequisites are met. Arcs the
//!   player finished with someone else score lower, so the stories stay fresh.
//! - **Binding** ([`bind_arc`]): the arc's quest is bound by the quest engine from the giver's
//!   settlement ([`crate::quest::binding::bind_quest_from`]), so places are near the person
//!   asking, anywhere in an endless world. Each bound target becomes an [`ArcPlace`] (a name and
//!   a "where" phrase: a village by name, the sky island "up among the clouds", a derelict
//!   "down in the Slate Shelves, 92m"); a [`crate::quest::Objective::Speak`] beat picks a
//!   contact who lives there. Names are kept as text, so the journal reads the same after the
//!   world moves on.
//! - **State** ([`ArcLog`], inside the player's [`crate::quest::QuestLog`]): the bound offer on
//!   the table, arcs in progress (each with its quest engine progress), promised follow-ups and
//!   the history of finished arcs (the journal's past stories and later dialogue).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    ArcId, ArcOutcome, ArcTemplate, MemoryFact, SeedSource, Slot, SlotBinding, StorySeed,
    VillageProblem, WorldFact, best_trigger, bind_slots, binds_want_focus,
};
use crate::{
    ids::PlayerId,
    people::{
        Character, CharacterId, PeopleRng, Population, VillageId,
        dialogue::{
            grammar::{self, Vars},
            lines::ToneLines,
        },
        salt,
        traits::Tone,
    },
    player::Player,
    poi::PoiJournal,
    quest::{
        ActiveQuest, Objective, ObjectiveProgress, QuestTarget, StoryPlace, TargetSpec,
        binding::{BoundTarget, bind_quest_from},
    },
    terrain::{Terrain, TilePosition},
    worldgen::sky::SkyBands,
    worldgen::{Band, FeatureDetail, FeatureId, FeatureKind, WorldSettings},
};

/// Most arcs one player may have running at once (on top of keeper quests).
pub const MAX_ACTIVE_ARCS: usize = 3;
/// Days a person's offer holds before their best arc is re-rolled.
pub const OFFER_ROTATION_DAYS: u32 = 3;
/// Finished arcs kept in the journal (the oldest drop off).
///
/// Who finished what with whom is kept for good in [`ArcLog::finished`] and in the givers'
/// memories. The log rides in the owner's loadout section, which is resent whole whenever
/// anything in it changes, so it stays small.
pub const HISTORY_LIMIT: usize = 12;
/// Score taken off an arc the player already finished with someone else.
const REPEAT_PENALTY: u32 = 25;

/// A place an arc was bound to, as the story names it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcPlace {
    /// What it is ("derelict rig", "sky island", "village").
    pub noun: String,
    /// What the story calls it ("Copperhollow", "the derelict rig", "the Moon").
    pub dest: String,
    /// Where it is ("down in the Slate Shelves, 92m down", "up among the clouds to the east").
    pub phrase: String,
    /// The band it lies in, "the sky" above the ground.
    pub band: String,
    /// Metres below the surface (0 at or above it).
    pub depth: i32,
    /// The place stands in for one the world lacks (a ridge for a sky island).
    #[serde(default)]
    pub stand_in: bool,
    /// The settlement there, if any (contacts live in it).
    #[serde(default)]
    pub settlement: Option<VillageId>,
}

/// A person an arc sends you to.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcContact {
    pub id: CharacterId,
    pub name: String,
    /// The settlement they live in.
    pub home: String,
}

/// Who and what an arc is about: everything its text needs, kept as text.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcCast {
    pub arc: ArcId,
    pub giver: CharacterId,
    /// The giver's short name.
    pub giver_name: String,
    /// The giver's full name.
    #[serde(default)]
    pub giver_full: String,
    /// Where the giver lives.
    pub home: String,
    /// What the giver calls the player.
    pub player: String,
    /// Why the giver offered it.
    pub source: SeedSource,
    /// One per [`ArcTemplate::slots`] entry.
    pub slots: Vec<SlotBinding>,
    /// One per quest target.
    pub places: Vec<ArcPlace>,
    #[serde(default)]
    pub contact: Option<ArcContact>,
}

impl ArcCast {
    /// The arc template.
    #[must_use]
    pub fn template(&self) -> Option<&'static ArcTemplate> {
        self.arc.template()
    }

    /// Variables for the arc's lines.
    #[must_use]
    pub fn vars(&self) -> Vars {
        let mut vars = Vars::new();
        vars.set("giver", self.giver_name.clone());
        vars.set("village", self.home.clone());
        vars.set("player", self.player.clone());
        let mut place_target = None;
        for (slot, binding) in self
            .template()
            .map_or(&[][..], |template| template.slots)
            .iter()
            .zip(&self.slots)
        {
            match binding {
                SlotBinding::Absent { name, relation } => {
                    vars.set("focus", name.clone());
                    vars.set("focus_relation", relation.clone());
                }
                SlotBinding::Neighbour { name, relation, .. } => {
                    vars.set("other", name.clone());
                    vars.set("other_word", relation.clone());
                }
                SlotBinding::Place(fact) => {
                    if place_target.is_none() {
                        place_target = match slot {
                            Slot::Place { target, .. } => Some(usize::from(*target)),
                            _ => Some(0),
                        };
                        // Before binding (no places yet) the fact describes it.
                        vars.set("place", fact.kind.nouns()[0]);
                        vars.set("band", fact.band_name());
                        vars.set("depth", fact.depth().to_string());
                    }
                }
            }
        }
        if let Some(place) = place_target.and_then(|index| self.places.get(index)) {
            vars.set("place", place.noun.clone());
            vars.set("band", place.band.clone());
            vars.set("depth", place.depth.to_string());
        }
        if let Some(place) = self.places.first() {
            vars.set("dest", place.dest.clone());
            vars.set("where", place.phrase.clone());
        }
        if let Some(place) = self.places.get(1) {
            vars.set("dest2", place.dest.clone());
            vars.set("where2", place.phrase.clone());
        }
        if let Some(contact) = &self.contact {
            vars.set("contact", contact.name.clone());
        } else {
            vars.set("contact", "whoever's about");
        }
        vars
    }

    /// `template` filled with the cast (alternatives rolled from the arc and `salt_value`).
    #[must_use]
    pub fn render(&self, template: &str, salt_value: u64) -> String {
        let mut rng = PeopleRng::new(
            u64::from(self.giver.village.0) ^ (u64::from(self.giver.index) << 32),
            salt::STORY ^ 0xA2C0,
            u64::from(self.arc.0),
            salt_value,
        );
        grammar::expand(template, &self.vars(), &mut rng)
    }

    /// One of `lines` in `giver`'s voice, filled with the cast.
    #[must_use]
    pub fn voiced(&self, lines: &ToneLines, giver: Option<&Character>, salt_value: u64) -> String {
        let (seed, tone) = giver.map_or((0, Tone::Earnest), |giver| (giver.voice_seed, giver.tone));
        let mut rng = PeopleRng::new(seed, salt::STORY, u64::from(self.arc.0), salt_value);
        let options = lines.get(tone);
        if options.is_empty() {
            return String::new();
        }
        let template = rng.pick(options);
        grammar::expand(template, &self.vars(), &mut rng)
    }
}

/// An arc on offer or in progress.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcRun {
    pub cast: ArcCast,
    /// The quest engine's progress (quest id `ARC_QUEST_BASE + arc`).
    pub quest: ActiveQuest,
    /// The world day it was offered (then accepted).
    pub day: u32,
}

impl ArcRun {
    #[must_use]
    pub const fn arc(&self) -> ArcId {
        self.cast.arc
    }

    #[must_use]
    pub fn template(&self) -> Option<&'static ArcTemplate> {
        self.cast.arc.template()
    }

    /// The current beat (the last once every beat is done).
    #[must_use]
    pub fn step(&self) -> usize {
        let steps = self.template().map_or(1, |template| template.stages.len());
        usize::from(self.quest.stage).min(steps.saturating_sub(1))
    }

    /// Every beat is done: go back to the giver.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.quest.ready
    }

    /// The journal's story text for the current beat (or the climax once ready).
    #[must_use]
    pub fn journal_text(&self) -> String {
        let Some(template) = self.template() else {
            return String::new();
        };
        if self.ready() {
            return self.cast.render(template.climax, 3);
        }
        template
            .steps
            .get(self.step())
            .map_or_else(String::new, |step| self.cast.render(step.journal, 3))
    }

    /// What to do now: the current beat's objectives with the story's names for places, each
    /// with whether it is done; once ready, going back to the giver.
    #[must_use]
    pub fn objective_lines(
        &self,
        owner: Option<&Player>,
        rate: crate::tick::TickRate,
    ) -> Vec<(String, bool)> {
        let Some(def) = self.quest.quest.def() else {
            return Vec::new();
        };
        let names: Vec<String> = self
            .cast
            .places
            .iter()
            .map(|place| place.dest.clone())
            .collect();
        let stage = usize::from(self.quest.stage);
        let mut lines: Vec<(String, bool)> = if self.ready() {
            // Deliveries are handed over at the giver.
            let last = def.stages.len().saturating_sub(1);
            let start = def.stage_start(last);
            def.stages
                .get(last)
                .copied()
                .unwrap_or(&[])
                .iter()
                .zip(self.quest.objectives.iter().skip(start))
                .filter(|(objective, _)| matches!(objective, Objective::Deliver { .. }))
                .map(|(objective, progress)| {
                    (
                        crate::quest::describe::objective_line_named(
                            def,
                            &self.quest,
                            *objective,
                            progress,
                            owner,
                            rate,
                            &names,
                        ),
                        false,
                    )
                })
                .collect()
        } else {
            let start = def.stage_start(stage);
            def.stages
                .get(stage)
                .copied()
                .unwrap_or(&[])
                .iter()
                .zip(self.quest.objectives.iter().skip(start))
                .map(|(objective, progress)| {
                    let line = match objective {
                        Objective::Speak { .. } if !speak_reaches(progress) => {
                            self.cast.contact.as_ref().map_or_else(
                                || {
                                    crate::quest::describe::objective_line_named(
                                        def,
                                        &self.quest,
                                        *objective,
                                        progress,
                                        owner,
                                        rate,
                                        &names,
                                    )
                                },
                                |contact| format!("Talk to {} in {}", contact.name, contact.home),
                            )
                        }
                        _ => crate::quest::describe::objective_line_named(
                            def,
                            &self.quest,
                            *objective,
                            progress,
                            owner,
                            rate,
                            &names,
                        ),
                    };
                    (line, progress.done)
                })
                .collect()
        };
        if self.ready() {
            lines.push((
                format!("Go back to {} in {}", self.cast.giver_name, self.cast.home),
                false,
            ));
        }
        lines
    }

    /// The beats already behind you, as the journal retells them.
    #[must_use]
    pub fn story_so_far(&self) -> Vec<String> {
        let Some(template) = self.template() else {
            return Vec::new();
        };
        let done = if self.ready() {
            template.steps.len()
        } else {
            self.step()
        };
        template
            .steps
            .iter()
            .take(done)
            .map(|step| self.cast.render(step.journal, 3))
            .collect()
    }
}

/// A finished (or abandoned) arc in the journal.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcRecord {
    pub cast: ArcCast,
    /// The outcome index it ended with.
    pub outcome: u8,
    /// The world day it ended.
    pub day: u32,
    /// It was given up rather than finished.
    #[serde(default)]
    pub abandoned: bool,
}

impl ArcRecord {
    /// The ending it took.
    #[must_use]
    pub fn ending(&self) -> Option<&'static ArcOutcome> {
        self.cast
            .template()?
            .outcomes
            .get(usize::from(self.outcome))
    }

    /// The journal's epilogue.
    #[must_use]
    pub fn epilogue(&self) -> String {
        self.ending()
            .map_or_else(String::new, |ending| self.cast.render(ending.epilogue, 4))
    }
}

/// One player's stories (WP-STORY), kept in their quest log.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct ArcLog {
    /// The arc a person just pitched, bound and waiting for a yes.
    #[serde(default)]
    pub offer: Option<Box<ArcRun>>,
    /// Arcs in progress, oldest first.
    #[serde(default)]
    pub active: Vec<ArcRun>,
    /// Finished and abandoned arcs, oldest first (at most [`HISTORY_LIMIT`]).
    #[serde(default)]
    pub history: Vec<ArcRecord>,
    /// Follow-ups people promised: `(giver, arc)`.
    #[serde(default)]
    pub follow_ups: BTreeSet<(CharacterId, ArcId)>,
    /// Arcs finished (not abandoned) with each giver, kept beyond the history limit.
    #[serde(default)]
    pub finished: BTreeSet<(CharacterId, ArcId)>,
}

impl ArcLog {
    /// Whether nothing story-related has happened.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offer.is_none()
            && self.active.is_empty()
            && self.history.is_empty()
            && self.follow_ups.is_empty()
            && self.finished.is_empty()
    }

    /// The running arc `arc`, if any.
    #[must_use]
    pub fn running(&self, arc: ArcId) -> Option<&ArcRun> {
        self.active.iter().find(|run| run.arc() == arc)
    }

    /// The arc `giver` gave this player that is still running, if any.
    #[must_use]
    pub fn from_giver(&self, giver: CharacterId) -> Option<&ArcRun> {
        self.active.iter().find(|run| run.cast.giver == giver)
    }

    /// Whether `arc` was finished with `giver`.
    #[must_use]
    pub fn finished_with(&self, giver: CharacterId, arc: ArcId) -> bool {
        self.finished.contains(&(giver, arc))
    }

    /// Whether `arc` was finished with anyone.
    #[must_use]
    pub fn finished_any(&self, arc: ArcId) -> bool {
        self.finished.iter().any(|(_, done)| *done == arc)
    }

    /// Files a finished or abandoned arc.
    pub fn record(&mut self, record: ArcRecord) {
        if !record.abandoned {
            self.finished.insert((record.cast.giver, record.cast.arc));
        }
        self.history.push(record);
        let excess = self.history.len().saturating_sub(HISTORY_LIMIT);
        if excess > 0 {
            self.history.drain(..excess);
        }
    }
}

/// What deciding a person's offer needs from the world.
#[derive(Clone, Copy, Debug)]
pub struct OfferContext<'a> {
    pub settings: &'a WorldSettings,
    /// The world's local pixel frame, which the asking rig's position is measured in (F7).
    pub frame: crate::position::Frame,
    /// The giver's settlement's people.
    pub population: &'a Population,
    /// Story-worthy places near them (scoring and slot text before binding).
    pub facts: &'a [WorldFact],
    pub problems: &'a [VillageProblem],
    /// What the asking player is known for.
    pub history: &'a [MemoryFact],
    /// The world day (offers rotate every [`OFFER_ROTATION_DAYS`]).
    pub day: u32,
}

/// Whether `template` may be offered to `player` at all: its prerequisites are met and, on a
/// world with a sky, the rig can actually climb to its sky places (the measured climb table:
/// [`crate::milestone::island_capable`] for the isles, the dead-air gate
/// [`crate::milestone::crosses_dead_air`] for the great sky lands and the Moon). Worlds
/// without a sky bind stand-ins on the ground, so only the authored prerequisites count there.
pub(crate) fn unlocked(
    template: &ArcTemplate,
    player: &Player,
    settings: &WorldSettings,
    frame: crate::position::Frame,
) -> bool {
    template
        .prerequisites
        .iter()
        .all(|prerequisite| prerequisite.met_in(player, settings, frame))
        && settings
            .generator
            .sky_bands()
            .is_none_or(|bands| sky_reachable(template, player, &bands))
}

/// Whether `player`'s rig climbs to every sky place `template` sends it to in a sky with
/// `bands`.
fn sky_reachable(template: &ArcTemplate, player: &Player, bands: &SkyBands) -> bool {
    template.targets.iter().all(|target| match target {
        TargetSpec::Story(StoryPlace::SkyIsland) => crate::milestone::island_capable(player, bands),
        TargetSpec::Story(StoryPlace::SkyContinent | StoryPlace::Moon) => {
            crate::milestone::crosses_dead_air(player, bands)
        }
        TargetSpec::Story(StoryPlace::SkyTown) => crate::milestone::sky_town_capable(player, bands),
        _ => true,
    })
}

/// The arc `character` offers the player whose log is `log`, with why and its slots bound, or
/// `None` when they have nothing for them (busy with an arc they gave, or nothing fits).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "follow-ups, then scoring, in one pass"
)]
pub fn offer_for(
    character: &Character,
    ctx: &OfferContext<'_>,
    log: &ArcLog,
    player: &Player,
) -> Option<StorySeed> {
    if log.from_giver(character.id).is_some() {
        return None;
    }
    let window = u64::from(ctx.day / OFFER_ROTATION_DAYS);
    let running: BTreeSet<ArcId> = log.active.iter().map(ArcRun::arc).collect();
    // A promised follow-up comes first.
    for (giver, arc) in &log.follow_ups {
        if *giver != character.id || running.contains(arc) || log.finished_with(*giver, *arc) {
            continue;
        }
        let Some(template) = arc.template() else {
            continue;
        };
        if !unlocked(template, player, ctx.settings, ctx.frame) {
            continue;
        }
        let mut rng = PeopleRng::new(
            character.voice_seed ^ window,
            salt::STORY,
            u64::from(arc.0),
            0,
        );
        if let Some(bindings) = bind_slots(template, character, ctx.population, ctx.facts, &mut rng)
        {
            return Some(StorySeed {
                arc: *arc,
                giver: character.id,
                source: SeedSource::FollowUp(*arc),
                bindings,
                score: u32::MAX,
            });
        }
    }
    let mut best: Option<StorySeed> = None;
    for template in super::ARC_TEMPLATES.iter() {
        if running.contains(&template.id)
            || log.finished_with(character.id, template.id)
            || !unlocked(template, player, ctx.settings, ctx.frame)
        {
            continue;
        }
        // The great sky lands are the heart of these stories: a world without a sky doesn't
        // offer them (their stand-in still binds, should one be promised as a follow-up).
        if !ctx.settings.generator.has_sky()
            && template
                .targets
                .iter()
                .any(|target| matches!(target, TargetSpec::Story(StoryPlace::SkyContinent)))
        {
            continue;
        }
        let Some((mut weight, mut source)) = best_trigger(
            template,
            character,
            ctx.population,
            ctx.problems,
            ctx.history,
            true,
        ) else {
            continue;
        };
        let mut rng = PeopleRng::new(
            character.voice_seed ^ window,
            salt::STORY,
            u64::from(template.id.0),
            0,
        );
        let Some(bindings) = bind_slots(template, character, ctx.population, ctx.facts, &mut rng)
        else {
            continue;
        };
        if matches!(source, SeedSource::Want(_))
            && !binds_want_focus(character, ctx.population, &bindings)
        {
            let Some(other) = best_trigger(
                template,
                character,
                ctx.population,
                ctx.problems,
                ctx.history,
                false,
            ) else {
                continue;
            };
            (weight, source) = other;
        }
        let role_fit = if template.roles.contains(&character.role) {
            8
        } else {
            0
        };
        let jitter = u32::try_from(rng.below(12)).unwrap_or(0);
        let penalty = if log.finished_any(template.id) {
            REPEAT_PENALTY
        } else {
            0
        };
        let score = (weight + role_fit + jitter).saturating_sub(penalty);
        let better = best.as_ref().is_none_or(|current| {
            score > current.score || (score == current.score && template.id < current.arc)
        });
        if better {
            best = Some(StorySeed {
                arc: template.id,
                giver: character.id,
                source,
                bindings,
                score,
            });
        }
    }
    best
}

/// Where an arc is bound from: the giver's settlement.
#[derive(Clone, Copy, Debug)]
pub struct BindContext<'a> {
    pub terrain: &'a Terrain,
    /// The settlement's centre on its ground.
    pub origin: TilePosition,
    /// The giver's settlement.
    pub home: VillageId,
    /// The giver's settlement's name.
    pub home_name: &'a str,
    /// The owner's map journal (unvisited places first).
    pub journal: &'a PoiJournal,
    /// Places other quests already use (avoided where possible).
    pub taken: &'a BTreeSet<FeatureId>,
    pub owner: PlayerId,
    /// What the giver calls the owner.
    pub player_name: &'a str,
    pub tick: u64,
    pub day: u32,
    /// The owner's hull in tenths (fragile twists count from here).
    pub hull_tenths: u32,
}

const fn direction(from: i32, to: i32) -> &'static str {
    if to < from - 8 {
        "west"
    } else if to > from + 8 {
        "east"
    } else {
        "near"
    }
}

/// "a short hop east", "a long haul west", ...
fn distance_phrase(from: i32, to: i32) -> String {
    let dir = direction(from, to);
    if dir == "near" {
        return "right nearby".to_owned();
    }
    let span = (to - from).abs();
    let how = match span {
        0..=250 => "a short way",
        251..=700 => "a fair way",
        701..=1_600 => "a long haul",
        _ => "a very long haul",
    };
    format!("{how} {dir}")
}

/// Names the place a target was bound to.
#[allow(clippy::too_many_lines, reason = "one arm per kind of place")]
fn describe_place(
    terrain: &Terrain,
    origin: TilePosition,
    home_name: &str,
    target: &QuestTarget,
    bound: BoundTarget,
) -> ArcPlace {
    let settings = terrain.settings();
    let ground = terrain.surface_row(target.center.x);
    let depth = (target.center.y - ground).max(0);
    let band_name = |row: i32| {
        if row < ground {
            "the sky".to_owned()
        } else {
            let band = crate::worldgen::strata::band_at(settings, target.center.x, row);
            format!("the {}", band.name())
        }
    };
    let away = distance_phrase(origin.x, target.center.x);
    match bound {
        BoundTarget::Town => ArcPlace {
            noun: "town".into(),
            dest: crate::people::routine::TOWN_NAME.into(),
            phrase: format!("back in town ({away})"),
            band: "the surface".into(),
            depth: 0,
            stand_in: false,
            settlement: Some(crate::people::routine::TOWN_VILLAGE),
        },
        BoundTarget::Village(id) => ArcPlace {
            noun: "village".into(),
            dest: home_name.to_owned(),
            phrase: format!("here in {home_name}"),
            band: "the surface".into(),
            depth: 0,
            stand_in: false,
            settlement: Some(id),
        },
        BoundTarget::Camp(id) => {
            let kind = crate::people::camps::camp(settings, id)
                .map_or("wanderers' camp", |camp| camp.kind.label());
            ArcPlace {
                noun: "camp".into(),
                dest: format!("the {kind}"),
                phrase: format!("out on the trail ({away})"),
                band: "the surface".into(),
                depth: 0,
                stand_in: false,
                settlement: Some(id),
            }
        }
        BoundTarget::Tomb(site) => {
            let noun = site.style.noun().to_lowercase();
            let (where_, band) = match site.setting {
                crate::tomb::SiteSetting::Mesa => (
                    format!("cut into a mesa ({away})"),
                    "the surface".to_owned(),
                ),
                crate::tomb::SiteSetting::OldQuarter => (
                    format!("under an old village quarter, {depth}m down ({away})"),
                    band_name(target.center.y),
                ),
                _ => (
                    format!("{depth}m down in {} ({away})", band_name(target.center.y)),
                    band_name(target.center.y),
                ),
            };
            ArcPlace {
                noun: format!("sealed {noun}"),
                dest: format!("the sealed {noun}"),
                phrase: where_,
                band,
                depth,
                stand_in: false,
                settlement: None,
            }
        }
        BoundTarget::Ruin(_) => ArcPlace {
            noun: "sealed ruin".into(),
            dest: "the sealed ruin".into(),
            phrase: format!("{depth}m down in {} ({away})", band_name(target.center.y)),
            band: band_name(target.center.y),
            depth,
            stand_in: false,
            settlement: None,
        },
        BoundTarget::Market(id) => {
            let name = crate::village::village(settings, id)
                .map_or_else(|| home_name.to_owned(), |found| found.name.clone());
            ArcPlace {
                noun: "night market".into(),
                dest: format!("the night market in {name}"),
                phrase: if name == home_name {
                    format!("here in {home_name}, after dusk")
                } else {
                    format!("{away} along the trail, after dusk")
                },
                band: "the surface".into(),
                depth: 0,
                stand_in: false,
                settlement: Some(id),
            }
        }
        BoundTarget::Plot(site) => ArcPlace {
            noun: "homestead".into(),
            dest: match site.setting {
                crate::home::PlotSetting::Town => "the empty plot at the edge of town".into(),
                crate::home::PlotSetting::Village(_) => "the empty plot outside the village".into(),
                crate::home::PlotSetting::Lakeside => "the empty lakeside plot".into(),
                crate::home::PlotSetting::Clifftop => "the empty clifftop plot".into(),
                crate::home::PlotSetting::Meadow => "the empty meadow plot".into(),
                _ => "the empty plot".into(),
            },
            phrase: format!("on the land office's books ({away})"),
            band: "the surface".into(),
            depth: 0,
            stand_in: false,
            settlement: None,
        },
        BoundTarget::SkyTown(id) => {
            let up = (ground - target.center.y).max(0);
            let name = crate::village::village(settings, id)
                .map_or_else(|| "a sky town".to_owned(), |found| found.name.clone());
            ArcPlace {
                noun: "sky town".into(),
                dest: name,
                phrase: format!("up in the cloud archipelago, {up}m up ({away})"),
                band: "the sky".into(),
                depth: 0,
                stand_in: false,
                settlement: Some(id),
            }
        }
        BoundTarget::Ridge => ArcPlace {
            noun: "high ridge".into(),
            dest: "the high ridge".into(),
            phrase: format!(
                "up on the highest ground {} of {home_name}",
                match direction(origin.x, target.center.x) {
                    "near" => "close",
                    dir => dir,
                }
            ),
            band: "the surface".into(),
            depth: 0,
            stand_in: true,
            settlement: None,
        },
        BoundTarget::Feature(feature) => match feature.kind {
            FeatureKind::Village => {
                let id = feature.village();
                let (name, underground) = id
                    .and_then(|id| crate::village::village(settings, id))
                    .map_or_else(
                        || ("a village".to_owned(), false),
                        |found| {
                            (
                                found.name.clone(),
                                found.setting == crate::village::VillageSetting::Underground,
                            )
                        },
                    );
                if underground {
                    ArcPlace {
                        noun: "underground settlement".into(),
                        dest: name,
                        phrase: format!(
                            "in its great hall {depth}m down in {}",
                            band_name(target.center.y)
                        ),
                        band: band_name(target.center.y),
                        depth,
                        stand_in: false,
                        settlement: id,
                    }
                } else {
                    ArcPlace {
                        noun: "village".into(),
                        dest: name,
                        phrase: format!("{away} along the trail"),
                        band: "the surface".into(),
                        depth: 0,
                        stand_in: false,
                        settlement: id,
                    }
                }
            }
            FeatureKind::SkyIsland => {
                let up = (ground - target.center.y).max(0);
                let biome = match feature.detail {
                    FeatureDetail::SkyIsland(biome) => biome.name().to_owned(),
                    _ => "floating".to_owned(),
                };
                ArcPlace {
                    noun: "sky island".into(),
                    dest: format!("the {biome} island"),
                    phrase: format!("up among the clouds, {up}m up ({away})"),
                    band: "the sky".into(),
                    depth: 0,
                    stand_in: false,
                    settlement: None,
                }
            }
            FeatureKind::SkyContinent => {
                let up = (ground - target.center.y).max(0);
                ArcPlace {
                    noun: "sky land".into(),
                    dest: "the great sky land".into(),
                    phrase: format!("past the dead air, {up}m up ({away})"),
                    band: "the sky".into(),
                    depth: 0,
                    stand_in: false,
                    settlement: None,
                }
            }
            FeatureKind::Moon => {
                // A filled Moon's story place is the research colony: its colonists are the
                // story's contacts (Wave 6).
                let colony = crate::people::colony::colony_settlement_id(settings);
                ArcPlace {
                    noun: "Moon".into(),
                    dest: if colony.is_some() {
                        "the Moon's colony".into()
                    } else {
                        "the Moon".into()
                    },
                    phrase: "at the very top of the sky".into(),
                    band: "the sky".into(),
                    depth: 0,
                    stand_in: false,
                    settlement: colony,
                }
            }
            _ => {
                let noun = feature.poi_kind().map_or_else(
                    || {
                        crate::quest::TargetSpec::Feature {
                            kind: feature.kind,
                            bands: (0, 0),
                        }
                        .noun()
                    },
                    |kind| {
                        crate::quest::TargetSpec::Poi {
                            kind,
                            bands: (0, 0),
                            unopened: false,
                        }
                        .noun()
                    },
                );
                let band = format!("the {}", Band::name(feature.band));
                ArcPlace {
                    noun: noun.to_owned(),
                    dest: format!("the {noun}"),
                    phrase: format!(
                        "{depth}m down in {band} ({})",
                        if away == "right nearby" {
                            "right below"
                        } else {
                            away.as_str()
                        }
                    ),
                    band,
                    depth,
                    stand_in: false,
                    settlement: None,
                }
            }
        },
    }
}

/// Picks the contact for a [`Objective::Speak`] beat: the neighbour the arc is about when it
/// asks for them and they live there, else someone living at the place (never the giver, nor
/// the person the story is about), the same person for the same world, giver and arc.
fn pick_contact(
    settings: &WorldSettings,
    place: &ArcPlace,
    giver: CharacterId,
    arc: ArcId,
    slots: &[SlotBinding],
) -> Option<ArcContact> {
    let settlement = crate::people::routine::settlement(settings, place.settlement?)?;
    let other = slots.iter().find_map(|binding| match binding {
        SlotBinding::Neighbour { id: Some(id), .. } => Some(*id),
        _ => None,
    });
    let wants_other = arc
        .template()
        .is_some_and(|template| template.contact_other);
    if wants_other
        && let Some(person) = other.and_then(|id| {
            settlement
                .population
                .characters
                .iter()
                .find(|person| person.id == id)
        })
    {
        return Some(ArcContact {
            id: person.id,
            name: person.name.short().to_owned(),
            home: settlement.population.village_name.clone(),
        });
    }
    let people: Vec<&Character> = settlement
        .population
        .characters
        .iter()
        .filter(|person| person.id != giver && Some(person.id) != other)
        .collect();
    if people.is_empty() {
        return None;
    }
    let roll = crate::worldgen::noise::hash2(
        settings.seed,
        salt::STORY ^ 0xC0_17AC,
        i32::from(arc.0),
        i32::try_from(u32::from(giver.index) ^ giver.village.0.rotate_left(5)).unwrap_or(0),
    );
    let person = people[usize::try_from(roll % people.len() as u64).unwrap_or(0)];
    Some(ArcContact {
        id: person.id,
        name: person.name.short().to_owned(),
        home: settlement.population.village_name.clone(),
    })
}

/// Binds `seed` (an arc `giver` offers) to the world from the giver's settlement.
///
/// The result holds the quest's targets and objectives, the places named and the contact.
/// `None` when a target has no candidate in this world (the lint keeps that from happening for
/// authored arcs).
#[must_use]
pub fn bind_arc(seed: &StorySeed, giver: &Character, ctx: &BindContext<'_>) -> Option<ArcRun> {
    let template = seed.arc.template()?;
    let def = seed.arc.quest().def()?;
    let salt_value = (giver.voice_seed % 1_000_003) | 1;
    let (targets, mut objectives, bound) = bind_quest_from(
        def,
        ctx.terrain,
        ctx.origin,
        ctx.journal,
        salt_value,
        ctx.taken,
        Some(ctx.home),
    )?;
    let places: Vec<ArcPlace> = targets
        .iter()
        .zip(&bound)
        .map(|(target, what)| describe_place(ctx.terrain, ctx.origin, ctx.home_name, target, *what))
        .collect();
    let settings = ctx.terrain.settings();
    let mut contact = None;
    for (objective, progress) in def.objectives().zip(objectives.iter_mut()) {
        if let Objective::Relay { .. } = objective {
            // The marker starts with the giver, not at a town counter.
            progress.position = Some(ctx.origin);
            progress.anchor = ctx.origin.y;
        }
        if let Objective::Speak { target } = objective {
            let found = places.get(usize::from(target)).and_then(|place| {
                pick_contact(settings, place, giver.id, seed.arc, &seed.bindings)
            });
            if found.is_none() {
                // Nobody lives there (a stand-in place): reaching it counts.
                progress.anchor = 1;
            }
            if contact.is_none() {
                contact = found;
            }
        }
    }
    let deadline = template.twists.iter().find_map(|twist| match twist {
        crate::quest::Twist::TimeLimit { seconds } => Some(u64::from(*seconds)),
        _ => None,
    });
    Some(ArcRun {
        cast: ArcCast {
            arc: seed.arc,
            giver: giver.id,
            giver_name: giver.name.short().to_owned(),
            giver_full: giver.name.full(),
            home: ctx.home_name.to_owned(),
            player: ctx.player_name.to_owned(),
            source: seed.source.clone(),
            slots: seed.bindings.clone(),
            places,
            contact,
        },
        quest: ActiveQuest {
            quest: seed.arc.quest(),
            owner: ctx.owner,
            crew: false,
            variant: 0,
            accepted_tick: ctx.tick,
            // Seconds until accepted: the world turns them into a deadline tick then.
            deadline,
            stage: 0,
            targets,
            objectives,
            damage_taken: 0,
            last_hull: ctx.hull_tenths,
            carrying: false,
            descended: false,
            ready: false,
            helpers: BTreeSet::new(),
        },
        day: ctx.day,
    })
}

/// How a giver opens their thanks when the arc authored none, by voice.
const fn thanks_openers(tone: Tone) -> &'static [&'static str] {
    match tone {
        Tone::Gruff => &[
            "Hm. Good work.",
            "Well. You came back. Good.",
            "Knew you'd manage. Mostly knew.",
            "Sit. You've earned the chair.",
        ],
        Tone::Cheerful => &[
            "You did it! You actually did it!",
            "Oh, look at you!",
            "There you are! I've been watching the trail all day!",
            "Hooray! Sorry. Hooray, quietly.",
        ],
        Tone::Nervous => &[
            "Oh, thank goodness. You're back.",
            "You're all right! Oh, good.",
            "I didn't worry. Much. Constantly, actually.",
            "Is it done? It's done? Oh, I can breathe again.",
        ],
        Tone::Poetic => &[
            "So the story comes home.",
            "And here you are, with the ending.",
            "Every road bends back to the door it left from.",
            "The trail brought you back. It usually knows best.",
        ],
        Tone::Dry => &[
            "Well. Look at that.",
            "Huh. It actually worked.",
            "And with all your limbs. Impressive.",
            "I had a small bet against you. I'm glad to lose it.",
        ],
        Tone::Earnest => &[
            "Thank you. Truly.",
            "I won't forget this.",
            "You didn't have to do this, and you did.",
            "I owe you more than I can say.",
        ],
    }
}

/// What `giver` says when the player picks `ending`: for the default ending the arc's thanks
/// (or an opener in the giver's voice), then the ending's own line.
#[must_use]
pub fn ending_reply(
    cast: &ArcCast,
    ending: &ArcOutcome,
    giver: &Character,
    salt_value: u64,
) -> String {
    let line = cast.render(ending.line, salt_value);
    if ending.when != super::OutcomeWhen::Completed {
        return line;
    }
    let mut opener = cast
        .template()
        .map(|template| cast.voiced(&template.thanks, Some(giver), salt_value))
        .unwrap_or_default();
    if opener.is_empty() {
        let options = thanks_openers(giver.tone);
        let index = (giver.voice_seed ^ salt_value) % options.len() as u64;
        options[usize::try_from(index).unwrap_or(0)].clone_into(&mut opener);
    }
    if line.is_empty() {
        opener
    } else {
        format!("{opener} {line}")
    }
}

/// Whether a Speak objective was bound without a contact (reaching the place counts).
#[must_use]
pub const fn speak_reaches(progress: &ObjectiveProgress) -> bool {
    progress.anchor == 1
}
