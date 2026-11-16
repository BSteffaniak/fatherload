//! Story arcs in the tick (WP-STORY): people pitch arcs, players take them on, the quest engine
//! runs their beats, contacts answer, and endings change the world.
//!
//! - **Asking** (`PeopleCommand::AskWork`): a person with an arc for the player pitches it in
//!   their own voice; the arc is bound to the world from their settlement right then
//!   ([`crate::people::story::run::bind_arc`]) and kept as the player's open offer, so the
//!   pitch names the real places. Someone whose arc is running says how it is going instead
//!   (the current beat's nudge), or asks how it went once it is ready.
//! - **Taking it on** (`AcceptArc`): the offer moves into the player's running arcs (at most
//!   [`MAX_ACTIVE_ARCS`]); the quest engine steps it every tick ([`WorldState::advance_quests`]),
//!   reporting beats as `ArcStepDone` and the last as `ArcClimax`.
//! - **Contacts**: a `Speak` beat is done when the player talks to the contact it named (any
//!   topic), who answers with the beat's contact line.
//! - **Endings** (`TurnInArc`): back at the giver, the player picks the default ending or one of
//!   the arc's choices; the quest reward is paid (its reputation becomes goodwill with the
//!   giver), the ending's effects apply (goodwill, mood, a settlement's fortunes, a changed
//!   relationship, an unlocked offer, a promised follow-up, credits, a cosmetic, goodwill
//!   across the settlement), the giver remembers it and word spreads.
//! - **Giving up** (`AbandonArc`, or a failed twist): the abandoned ending's effects, a
//!   remembered disappointment, and the journal keeps the story.

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    entity::EntityId,
    event::GameplayEventKind,
    ids::PlayerId,
    people::{
        self, ArcId, ArcRecord, ArcRun, Character, CharacterId, MemoryFact, StorySeed,
        ledger::{ConversationLine, Speaker},
        routine::Settlement,
        story::{
            ArcEffect,
            run::{BindContext, MAX_ACTIVE_ARCS, OfferContext, bind_arc, offer_for},
        },
    },
    poi::PoiReward,
    quest::{
        Objective,
        describe::{QuestMarker, QuestMarkerKind, quest_markers},
    },
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TilePosition},
    worldgen::FeatureId,
};

use super::{
    WorldState,
    people::{note_line, you},
    quests::hull_tenths,
};

/// What a conversation can do about stories with one person (the talk panel's story rows).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArcTalk {
    /// Nothing for this player right now.
    Nothing,
    /// They have an arc for the player: ask them.
    HasWork(ArcId),
    /// They just pitched `arc`: take it on, or say goodbye.
    Pitched(ArcId),
    /// The arc they gave is under way.
    Running(ArcId),
    /// The arc they gave is done: pick an ending, as (outcome index, label).
    Ready {
        arc: ArcId,
        endings: Vec<(u8, &'static str)>,
    },
}

/// What the player says when asking for work.
const ASK_LINES: [&str; 3] = [
    "Need a hand with anything?",
    "Anything I can do for you?",
    "You look like someone with a problem. Can I help?",
];
/// What a person with nothing for the player says.
const NO_WORK_LINES: [&str; 4] = [
    "Not today. But ask me again; something always comes up out here.",
    "I'm all right for now, truly. Go dig something.",
    "Nothing yet. Check back in a few days. Trouble keeps a schedule.",
    "Me? I'm fine. Ask around, though. Half this place has a story.",
];
/// What a giver says when the player comes back with a finished arc.
const READY_LINES: [&str; 3] = [
    "You're back! Sit down. Tell me everything.",
    "Well? Don't keep me waiting. How did it go?",
    "I can see it on your face. Go on, then.",
];
/// What a giver says when the player takes their arc on.
const ACCEPT_LINES: [&str; 4] = [
    "Thank you. Really. I'll be here, worrying.",
    "I owe you one already. Mind how you go.",
    "Good. That's a weight off. Come back in one piece.",
    "Wonderful. I'll put the kettle on for when you're back.",
];

/// A line from `lines` for this person, this day and this many exchanges.
fn pick(lines: &[&'static str], character: &Character, day: u32, exchange: u32) -> &'static str {
    let index = (character.voice_seed ^ u64::from(day).wrapping_mul(31) ^ u64::from(exchange))
        % lines.len() as u64;
    lines[usize::try_from(index).unwrap_or(0)]
}

const fn them(text: String) -> ConversationLine {
    ConversationLine {
        speaker: Speaker::Them,
        text,
    }
}

impl WorldState {
    /// `player_id`'s stories: the open offer, running arcs, history and follow-ups.
    #[must_use]
    pub fn arc_log(&self, player_id: PlayerId) -> Option<&people::ArcLog> {
        self.players
            .get(&player_id)
            .map(|player| &player.quests.arcs)
    }

    /// The settlement `character` lives in.
    fn settlement_of(&self, character: CharacterId) -> Option<Arc<Settlement>> {
        people::routine::settlement(self.terrain.settings(), character.village)
    }

    /// The arc `character` (in `settlement`) would offer `player_id` now, if any.
    fn arc_offer_in(
        &self,
        player_id: PlayerId,
        settlement: &Settlement,
        character: &Character,
    ) -> Option<StorySeed> {
        let player = self.players.get(&player_id)?;
        let facts = self.talk_context_facts(settlement);
        let problems: Vec<people::VillageProblem> =
            crate::village::village(self.terrain.settings(), settlement.village)
                .and_then(|found| found.problem)
                .into_iter()
                .collect();
        let history: Vec<MemoryFact> = self
            .people
            .characters
            .get(&character.id)
            .map(|state| {
                state
                    .memory(player_id)
                    .events
                    .into_iter()
                    .map(|event| event.fact)
                    .collect()
            })
            .unwrap_or_default();
        let ctx = OfferContext {
            settings: self.terrain.settings(),
            population: &settlement.population,
            facts: &facts,
            problems: &problems,
            history: &history,
            day: self.people_clock().day,
        };
        offer_for(character, &ctx, &player.quests.arcs, player)
    }

    /// The arc `character` would offer `player_id` now, if any (the talk panel's "has work").
    #[must_use]
    pub fn arc_offer(&self, player_id: PlayerId, character: CharacterId) -> Option<StorySeed> {
        let settlement = self.settlement_of(character)?;
        let person = settlement
            .population
            .characters
            .get(usize::from(character.index))?;
        self.arc_offer_in(player_id, &settlement, person)
    }

    /// What the talk panel can do about stories with the person behind `npc`.
    #[must_use]
    pub fn arc_talk(&self, player_id: PlayerId, npc: EntityId) -> ArcTalk {
        let Some((_, state)) = self.npc(npc) else {
            return ArcTalk::Nothing;
        };
        let character = state.character;
        let Some(log) = self.arc_log(player_id) else {
            return ArcTalk::Nothing;
        };
        if let Some(run) = log.from_giver(character) {
            if run.ready() {
                let endings = run
                    .template()
                    .map(|template| {
                        template
                            .turn_in_choices()
                            .into_iter()
                            .filter_map(|index| {
                                Some((
                                    u8::try_from(index).ok()?,
                                    template.outcomes.get(index)?.label,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                return ArcTalk::Ready {
                    arc: run.arc(),
                    endings,
                };
            }
            return ArcTalk::Running(run.arc());
        }
        if let Some(offer) = &log.offer
            && offer.cast.giver == character
        {
            return ArcTalk::Pitched(offer.arc());
        }
        self.arc_offer(player_id, character)
            .map_or(ArcTalk::Nothing, |seed| ArcTalk::HasWork(seed.arc))
    }

    /// Features every running quest and arc already uses (new arcs avoid them).
    fn taken_features(&self) -> BTreeSet<FeatureId> {
        self.players
            .values()
            .flat_map(|player| {
                player
                    .quests
                    .active
                    .iter()
                    .chain(player.quests.arcs.active.iter().map(|run| &run.quest))
            })
            .chain(self.progression.crew_quests.active.iter())
            .flat_map(|quest| quest.targets.iter().filter_map(|target| target.feature))
            .collect()
    }

    /// Binds `seed` from `settlement` for `player_id`.
    pub(super) fn bind_offer(
        &self,
        player_id: PlayerId,
        settlement: &Settlement,
        character: &Character,
        seed: &StorySeed,
    ) -> Option<ArcRun> {
        let player = self.players.get(&player_id)?;
        let taken = self.taken_features();
        let player_name = self.person_name_for(player_id);
        let ctx = BindContext {
            terrain: &self.terrain,
            origin: TilePosition {
                x: settlement.center_column(),
                y: settlement.ground_row - 1,
            },
            home: settlement.village,
            home_name: &settlement.population.village_name,
            journal: &player.journal,
            taken: &taken,
            owner: player_id,
            player_name: &player_name,
            tick: self.simulation_tick.get(),
            day: self.people_clock().day,
            hull_tenths: hull_tenths(player.hull),
        };
        bind_arc(seed, character, &ctx)
    }

    /// `PeopleCommand::AskWork`.
    pub(super) fn ask_work(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some((settlement, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let id = character.id;
        let day = self.people_clock().day;
        let exchange = self
            .people
            .conversation(player_id)
            .map_or(0, |conversation| conversation.exchange);
        let Some(log) = self.arc_log(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let mut lines = Vec::new();
        if let Some(run) = log.from_giver(id).cloned() {
            lines.push(you("About that favour..."));
            if run.ready() {
                lines.push(them(
                    pick(&READY_LINES, &character, day, exchange).to_owned(),
                ));
            } else if let Some(step) = run
                .template()
                .and_then(|template| template.steps.get(run.step()))
            {
                lines.push(them(run.cast.render(step.nudge, u64::from(exchange))));
                if let Some(contact) = &run.cast.contact
                    && !step.contact.is_empty()
                {
                    lines.push(note_line(format!(
                        "Find {} in {}.",
                        contact.name, contact.home
                    )));
                }
            }
        } else if let Some(offer) = log.offer.clone().filter(|offer| offer.cast.giver == id) {
            // Asked again before answering: the same pitch, shorter.
            lines.push(you(pick(&ASK_LINES, &character, day, exchange)));
            if let Some(template) = offer.template() {
                lines.push(them(offer.cast.voiced(
                    &template.pitch,
                    Some(&character),
                    1,
                )));
                lines.push(note_line(format!(
                    "\"{}\": take it on, or say goodbye.",
                    template.title
                )));
            }
        } else {
            lines.push(you(pick(&ASK_LINES, &character, day, exchange)));
            let run = self
                .arc_offer_in(player_id, &settlement, &character)
                .and_then(|seed| self.bind_offer(player_id, &settlement, &character, &seed));
            match run {
                Some(run) => {
                    let template = run.template();
                    if let Some(template) = template {
                        lines.push(them(run.cast.voiced(&template.pitch, Some(&character), 1)));
                        lines.push(note_line(format!(
                            "New story, \"{}\": take it on, or say goodbye.",
                            template.title
                        )));
                    }
                    events.push(GameplayEventKind::ArcOffered {
                        player_id,
                        arc: run.arc(),
                    });
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.quests.arcs.offer = Some(Box::new(run));
                    }
                }
                None => lines.push(them(
                    pick(&NO_WORK_LINES, &character, day, exchange).to_owned(),
                )),
            }
        }
        self.converse(player_id, npc, id, lines);
        PlayerScopedCommandOutcome::Applied
    }

    /// `PeopleCommand::AcceptArc`.
    pub(super) fn accept_arc(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some((_, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let now = self.simulation_tick.get();
        let day = self.people_clock().day;
        let ticks_per_second = self.timing.ticks_for_millis(1_000);
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let hull = hull_tenths(player.hull);
        let log = &mut player.quests.arcs;
        let pitched = log
            .offer
            .as_ref()
            .is_some_and(|offer| offer.cast.giver == character.id);
        if !pitched || log.active.len() >= MAX_ACTIVE_ARCS {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(mut run) = log.offer.take().map(|run| *run) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if log.running(run.arc()).is_some() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        // The offer carried a time limit in seconds; it starts counting now.
        run.quest.deadline = run
            .quest
            .deadline
            .map(|seconds| now.saturating_add(seconds.saturating_mul(ticks_per_second)));
        run.quest.accepted_tick = now;
        run.quest.last_hull = hull;
        run.day = day;
        let arc = run.arc();
        log.follow_ups.remove(&(character.id, arc));
        log.active.push(run);
        let exchange = self
            .people
            .conversation(player_id)
            .map_or(0, |conversation| conversation.exchange);
        let lines = vec![
            you("I'll do it."),
            them(pick(&ACCEPT_LINES, &character, day, exchange).to_owned()),
            note_line("Written in your journal.".to_owned()),
        ];
        self.converse(player_id, npc, character.id, lines);
        events.push(GameplayEventKind::ArcAccepted { player_id, arc });
        PlayerScopedCommandOutcome::Applied
    }

    /// `PeopleCommand::TurnInArc`.
    #[allow(
        clippy::too_many_lines,
        reason = "validation, deliveries, rewards, effects and the conversation in order"
    )]
    pub(super) fn turn_in_arc(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        outcome: u8,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some((settlement, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(run) = self
            .arc_log(player_id)
            .and_then(|log| log.from_giver(character.id))
            .filter(|run| run.ready())
            .cloned()
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let (Some(template), Some(def)) = (run.template(), run.quest.quest.def()) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let index = usize::from(outcome);
        if !template.turn_in_choices().contains(&index) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(ending) = template.outcomes.get(index) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let deliveries: Vec<_> = def
            .stages
            .last()
            .copied()
            .unwrap_or(&[])
            .iter()
            .filter_map(|objective| match objective {
                Objective::Deliver { item, count } => Some((*item, *count)),
                _ => None,
            })
            .collect();
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let missing: Vec<String> = deliveries
            .iter()
            .filter(|(item, count)| item.held(player) < *count)
            .map(|(item, count)| format!("{} x{}", item.name(), count - item.held(player)))
            .collect();
        if !missing.is_empty() {
            // Not without the goods: they say so rather than the row doing nothing.
            let lines = vec![
                you(&format!("{}.", ending.label)),
                them("And the goods? Bring them with you and we'll settle it.".to_owned()),
                note_line(format!("Still to hand over: {}.", missing.join(", "))),
            ];
            self.converse(player_id, npc, character.id, lines);
            return PlayerScopedCommandOutcome::Applied;
        }
        let day = self.people_clock().day;
        let before = self.people.standing(character.id, player_id);
        let affinity_before = self.people.affinity(character.id, player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            for (item, count) in &deliveries {
                item.take(player, *count);
            }
            player
                .quests
                .arcs
                .active
                .retain(|other| other.arc() != run.arc());
        }
        // The quest's reward: credits and goods, its reputation as goodwill with the giver.
        let reward = def.reward;
        self.grant_poi_reward(
            player_id,
            &PoiReward {
                credits: reward.credits,
                bombs: u8::try_from(reward.bombs).unwrap_or(u8::MAX),
                fitting: reward.fitting,
                kit: reward.kit,
                ..PoiReward::default()
            },
        );
        if let Some(player) = self.players.get_mut(&player_id) {
            for (material, count) in reward.materials {
                player.add_material(*material, *count);
            }
        }
        if let Some((blueprint, count)) = reward.fragment {
            for _ in 0..count {
                events.extend(self.grant_blueprint_fragment(player_id, blueprint));
            }
        }
        self.add_goodwill(
            player_id,
            character.id,
            i16::try_from(reward.reputation).unwrap_or(i16::MAX),
        );
        let extras = self.apply_arc_effects(player_id, &run, &settlement, ending.effects, day);
        self.people
            .characters
            .entry(character.id)
            .or_default()
            .remember(
                player_id,
                day,
                MemoryFact::ArcCompleted {
                    arc: run.arc(),
                    outcome,
                },
            );
        let exchange = self
            .people
            .conversation(player_id)
            .map_or(0, |conversation| conversation.exchange);
        let reply = crate::people::story::run::ending_reply(
            &run.cast,
            ending,
            &character,
            u64::from(exchange),
        );
        let mut summary = vec![reward.summary()];
        summary.extend(extras);
        let lines = vec![
            you(&format!("{}.", ending.label)),
            them(reply),
            note_line(format!(
                "Story's end, \"{}\": {}.",
                template.title,
                summary.join(", ")
            )),
        ];
        self.converse(player_id, npc, character.id, lines);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.quests.arcs.record(ArcRecord {
                cast: run.cast.clone(),
                outcome,
                day,
                abandoned: false,
            });
        }
        self.spread_word(player_id, &settlement, character.id, affinity_before);
        self.warm(player_id, npc, character.id, before, events);
        events.push(GameplayEventKind::ArcCompleted {
            player_id,
            arc: run.arc(),
            outcome,
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// `PeopleCommand::AbandonArc` (anywhere: the journal's "give up").
    pub(super) fn abandon_arc(
        &mut self,
        player_id: PlayerId,
        arc: ArcId,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(index) = player
            .quests
            .arcs
            .active
            .iter()
            .position(|run| run.arc() == arc)
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let run = player.quests.arcs.active.remove(index);
        self.arc_lost(player_id, run);
        events.push(GameplayEventKind::ArcAbandoned { player_id, arc });
        PlayerScopedCommandOutcome::Applied
    }

    /// An arc given up or failed: its abandoned ending, a remembered disappointment and the
    /// journal entry. (The caller already took it out of the running arcs.)
    pub(super) fn arc_lost(&mut self, player_id: PlayerId, run: ArcRun) {
        let day = self.people_clock().day;
        let giver = run.cast.giver;
        let outcome = run
            .template()
            .and_then(crate::people::ArcTemplate::abandoned_outcome);
        if let (Some(index), Some(settlement)) = (outcome, self.settlement_of(giver))
            && let Some(ending) = run
                .template()
                .and_then(|template| template.outcomes.get(index))
        {
            let affinity_before = self.people.affinity(giver, player_id);
            let _ = self.apply_arc_effects(player_id, &run, &settlement, ending.effects, day);
            self.spread_word(player_id, &settlement, giver, affinity_before);
        }
        self.people.characters.entry(giver).or_default().remember(
            player_id,
            day,
            MemoryFact::ArcAbandoned { arc: run.arc() },
        );
        if let Some(player) = self.players.get_mut(&player_id) {
            player.quests.arcs.record(ArcRecord {
                cast: run.cast,
                outcome: outcome
                    .and_then(|index| u8::try_from(index).ok())
                    .unwrap_or(u8::MAX),
                day,
                abandoned: true,
            });
        }
    }

    /// Applies an ending's effects. Returns extra reward notes ("+120 cr", "Moss Green paint").
    fn apply_arc_effects(
        &mut self,
        player_id: PlayerId,
        run: &ArcRun,
        settlement: &Settlement,
        effects: &[ArcEffect],
        day: u32,
    ) -> Vec<String> {
        let giver = run.cast.giver;
        let mut notes = Vec::new();
        for effect in effects {
            match *effect {
                ArcEffect::Reputation(amount) => self.add_goodwill(player_id, giver, amount),
                ArcEffect::Mood(delta) => self
                    .people
                    .characters
                    .entry(giver)
                    .or_default()
                    .shift_mood(day, delta),
                ArcEffect::Prosperity(delta) => {
                    let shift = self
                        .people
                        .prosperity
                        .entry(settlement.village)
                        .or_default();
                    *shift = shift.saturating_add(i16::from(delta));
                }
                ArcEffect::Relationship(kind) => {
                    let other = run.cast.slots.iter().find_map(|binding| match binding {
                        people::SlotBinding::Neighbour { id: Some(id), .. } => Some(*id),
                        _ => None,
                    });
                    if let Some(other) = other {
                        self.people.bonds.insert((giver, other), kind);
                    }
                }
                ArcEffect::UnlockOffer => {
                    self.people
                        .players
                        .entry(player_id)
                        .or_default()
                        .unlocked
                        .insert(giver);
                    notes.push(format!(
                        "{}'s special offers open to you",
                        run.cast.giver_name
                    ));
                }
                // The giver's memory of the ending is recorded by the turn-in itself.
                ArcEffect::Remember => {}
                ArcEffect::FollowUp(next) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.quests.arcs.follow_ups.insert((giver, next));
                    }
                    notes.push(format!("{} will have more for you", run.cast.giver_name));
                }
                ArcEffect::Credits(credits) => {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.credits = player.credits.saturating_add(credits);
                    }
                    notes.push(format!("+{credits} cr"));
                }
                ArcEffect::Cosmetic(item) => {
                    let new = self
                        .players
                        .get_mut(&player_id)
                        .is_some_and(|player| player.unlock_cosmetic(item));
                    if new {
                        notes.push(format!("{} unlocked", item.label()));
                    }
                }
                ArcEffect::Settlement(amount) => {
                    for resident in &settlement.population.characters {
                        if resident.id != giver {
                            self.people
                                .characters
                                .entry(resident.id)
                                .or_default()
                                .hear_about(player_id, amount);
                        }
                    }
                    notes.push(format!(
                        "{} thinks well of you",
                        settlement.population.village_name
                    ));
                }
            }
        }
        notes
    }

    /// A story beat that sent `player_id` to `character` lands: marks every `Speak` objective
    /// of a running arc that names them and returns what they say.
    pub(super) fn arc_contact_lines(
        &mut self,
        player_id: PlayerId,
        character: CharacterId,
    ) -> Vec<ConversationLine> {
        let Some(player) = self.players.get_mut(&player_id) else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for run in &mut player.quests.arcs.active {
            if run.quest.ready
                || run
                    .cast
                    .contact
                    .as_ref()
                    .is_none_or(|contact| contact.id != character)
            {
                continue;
            }
            let (Some(template), Some(def)) = (run.cast.arc.template(), run.quest.quest.def())
            else {
                continue;
            };
            let stage = usize::from(run.quest.stage);
            let start = def.stage_start(stage);
            let Some(objectives) = def.stages.get(stage) else {
                continue;
            };
            let mut found = false;
            for (offset, objective) in objectives.iter().enumerate() {
                if let Objective::Speak { .. } = objective
                    && let Some(progress) = run.quest.objectives.get_mut(start + offset)
                    && !progress.done
                {
                    progress.done = true;
                    found = true;
                }
            }
            if found {
                if let Some(step) = template.steps.get(stage)
                    && !step.contact.is_empty()
                {
                    lines.push(them(run.cast.render(step.contact, 5)));
                }
                // Not every Speak beat carries a message (some ask questions, some fetch an
                // answer), so the note says only that the right person was found.
                let name = run
                    .cast
                    .contact
                    .as_ref()
                    .map_or("them", |contact| contact.name.as_str());
                lines.push(note_line(format!(
                    "\"{}\": you found {name}. Written in your journal.",
                    template.title
                )));
            }
        }
        lines
    }

    /// Map and world markers of `player_id`'s running arcs: the current beat's places, and the
    /// giver's home once an arc is ready to tell.
    #[must_use]
    pub fn arc_markers(&self, player_id: PlayerId) -> Vec<QuestMarker> {
        let Some(log) = self.arc_log(player_id) else {
            return Vec::new();
        };
        let tick = self.simulation_tick.get();
        let mut markers = Vec::new();
        for run in &log.active {
            if run.ready() {
                if let Some(settlement) = self.settlement_of(run.cast.giver) {
                    markers.push(QuestMarker {
                        quest: run.quest.quest,
                        position: TilePosition {
                            x: settlement.center_column(),
                            y: settlement.ground_row - 1,
                        },
                        kind: QuestMarkerKind::Target,
                    });
                }
            } else {
                markers.extend(quest_markers(&run.quest, tick));
            }
        }
        markers
    }

    /// Where the person `character` is right now, in world pixels (their NPC entity), if they
    /// are about.
    #[must_use]
    pub fn person_position(&self, character: CharacterId) -> Option<(f32, f32)> {
        let npc = self.npc_of(character)?;
        self.npc_position(npc)
    }
}

/// The world pixel of a tile's centre (scene and test helpers).
#[must_use]
pub fn tile_center_px(tile: TilePosition) -> (f32, f32) {
    #[allow(
        clippy::cast_precision_loss,
        reason = "tiles fit f32 within the streamed limit"
    )]
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}
