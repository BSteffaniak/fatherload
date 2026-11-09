//! Quest rules inside the fixed tick: accepting, turning in and abandoning quests at the
//! giver's counter (town services), advancing objectives every tick, twists and failures,
//! rewards, reputation rebates and blueprint fragments.
//!
//! Solo quests live in their owner's [`crate::quest::QuestLog`] (private loadout); crew quests in
//! the shared [`CrewQuests`]. Both advance here, detached from the world while they step so a step can read
//! and change rigs freely.

use std::collections::{BTreeSet, VecDeque};

use crate::{
    command::PlayerCommand,
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    poi::{PoiReward, PoiTile, in_scanner_range, scanner_poi_range},
    quest::{
        ActiveQuest, BlueprintId, BlueprintUnlock, BoardOffer, CrewQuests, MAX_ACTIVE_QUESTS,
        Objective, ObjectiveProgress, Prerequisite, QUESTS, QuestDef, QuestFailure, QuestGiver,
        QuestId, QuestTarget, SURFACE_ROW, Twist, bind_quest, binding::tile_distance, board_offers,
    },
    service::SessionServiceRequest,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::FeatureId,
};

use super::WorldState;

/// The switch latch in crew mode: a rig has to really hold it.
const CREW_LATCH_MILLIS: u64 = 1_000;
/// How close (tiles) a rig must stay to an escorted haul.
const ESCORT_RADIUS: i32 = 4;
/// Rows below the surface after which the stay-down twist arms.
const STAY_DOWN_ARM_ROWS: i32 = 6;
/// Rows a relay leg must cover before the marker can go back to the rig that handed it over.
const RELAY_MIN_LEG_ROWS: i32 = 4;

/// The window (ms) a simultaneous objective allows. A crew with a rig for every spot gets the
/// tight crew window; a crew short of rigs gets a window stretched toward the solo one for each
/// spot nobody can stand on (so two rigs can still work a three-spot job with one runner).
fn simultaneous_window(
    crew: bool,
    rigs: usize,
    spots: usize,
    window_ms: u32,
    solo_window_ms: u32,
) -> u64 {
    let (window, solo) = (u64::from(window_ms), u64::from(solo_window_ms));
    if !crew {
        return solo;
    }
    let missing = spots.saturating_sub(rigs.max(1)) as u64;
    let stretch = spots.saturating_sub(1).max(1) as u64;
    window + solo.saturating_sub(window) * missing.min(stretch) / stretch
}

/// What this tick did that objectives care about, gathered while it ran.
#[derive(Clone, Debug, Default)]
pub(super) struct QuestNotes {
    /// Tiles drilled out: who, where and what the tile was.
    pub mined: Vec<(PlayerId, TilePosition, TileKind)>,
    /// Where bombs went off.
    pub detonations: Vec<TilePosition>,
}

/// Facts read from the tick's events.
#[derive(Default)]
struct EventFacts {
    scanned: BTreeSet<PlayerId>,
    bombed: BTreeSet<PlayerId>,
    rescued: BTreeSet<PlayerId>,
    vaults_opened: BTreeSet<FeatureId>,
}

impl EventFacts {
    fn read(output: &TickOutput) -> Self {
        let mut facts = Self::default();
        for event in &output.events {
            match event.kind {
                GameplayEventKind::ScannerUsed { player_id } => {
                    facts.scanned.insert(player_id);
                }
                GameplayEventKind::BombPlaced { player_id } => {
                    facts.bombed.insert(player_id);
                }
                GameplayEventKind::RescueTriggered { player_id } => {
                    facts.rescued.insert(player_id);
                }
                GameplayEventKind::VaultOpened { feature } => {
                    facts.vaults_opened.insert(feature);
                }
                _ => {}
            }
        }
        facts
    }
}

/// Whether one quest is on offer to a player, and why not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuestAvailability {
    Available,
    /// Prerequisites still missing.
    Locked(Vec<Prerequisite>),
    /// Already accepted.
    Active,
    Completed,
    /// The player has [`MAX_ACTIVE_QUESTS`] quests going.
    TooMany,
}

/// One row of a quest desk: a quest a giver offers (or will offer), and its board posting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestOffer {
    pub quest: QuestId,
    pub availability: QuestAvailability,
    /// The board posting (HQ board only).
    pub posting: Option<BoardOffer>,
}

impl WorldState {
    /// Quests accepted in crew mode (shared with every player).
    #[must_use]
    pub const fn crew_quests(&self) -> &CrewQuests {
        &self.progression.crew_quests
    }

    /// Every quest `player_id` should see: their solo quests, then every crew quest.
    #[must_use]
    pub fn visible_quests(&self, player_id: PlayerId) -> Vec<&ActiveQuest> {
        self.players
            .get(&player_id)
            .map(|player| player.quests.active.iter())
            .into_iter()
            .flatten()
            .chain(self.progression.crew_quests.active.iter())
            .collect()
    }

    /// Quests `player_id` owns (solo and crew).
    fn owned_quests(&self, player_id: PlayerId) -> impl Iterator<Item = &ActiveQuest> {
        self.players
            .get(&player_id)
            .map(|player| player.quests.active.iter())
            .into_iter()
            .flatten()
            .chain(
                self.progression
                    .crew_quests
                    .active
                    .iter()
                    .filter(move |quest| quest.owner == player_id),
            )
    }

    /// Today's HQ board postings.
    #[must_use]
    pub fn board_postings(&self) -> Vec<BoardOffer> {
        board_offers(self.terrain.seed(), self.progression.town_event_day)
    }

    /// Whether `player_id` may accept `def` now.
    #[must_use]
    pub fn quest_availability(&self, player_id: PlayerId, def: &QuestDef) -> QuestAvailability {
        let Some(player) = self.players.get(&player_id) else {
            return QuestAvailability::Locked(Vec::new());
        };
        if self
            .owned_quests(player_id)
            .any(|quest| quest.quest == def.id)
        {
            return QuestAvailability::Active;
        }
        if !def.repeatable && player.quests.completed.contains(&def.id) {
            return QuestAvailability::Completed;
        }
        let missing: Vec<Prerequisite> = def
            .prerequisites
            .iter()
            .copied()
            .filter(|prerequisite| !prerequisite.met(player))
            .collect();
        if !missing.is_empty() {
            return QuestAvailability::Locked(missing);
        }
        if self.owned_quests(player_id).count() >= MAX_ACTIVE_QUESTS {
            return QuestAvailability::TooMany;
        }
        QuestAvailability::Available
    }

    /// What `giver`'s desk shows `player_id`: quests on offer, then the next locked ones (so
    /// players see what is coming). Completed quests are left out. The HQ board lists today's
    /// postings the player has not taken.
    #[must_use]
    pub fn quest_offers(&self, player_id: PlayerId, giver: QuestGiver) -> Vec<QuestOffer> {
        let Some(player) = self.players.get(&player_id) else {
            return Vec::new();
        };
        if giver == QuestGiver::Vale {
            let day = self.progression.town_event_day;
            return self
                .board_postings()
                .into_iter()
                .filter(|posting| {
                    player.quests.board_day != day
                        || !player.quests.board_taken.contains(&posting.slot)
                })
                .filter_map(|posting| {
                    let def = posting.quest.def()?;
                    Some(QuestOffer {
                        quest: posting.quest,
                        availability: self.quest_availability(player_id, def),
                        posting: Some(posting),
                    })
                })
                .collect();
        }
        let mut offers: Vec<QuestOffer> = QUESTS
            .iter()
            .filter(|def| def.giver == giver)
            .map(|def| QuestOffer {
                quest: def.id,
                availability: self.quest_availability(player_id, def),
                posting: None,
            })
            .filter(|offer| {
                !matches!(
                    offer.availability,
                    QuestAvailability::Completed | QuestAvailability::Active
                )
            })
            .collect();
        offers.sort_by_key(|offer| matches!(offer.availability, QuestAvailability::Locked(_)));
        offers
    }

    /// Resolves a quest service command (accept, turn in, abandon) with the events it
    /// raises, or `None` for every other command.
    pub(super) fn quest_command(
        &mut self,
        player_id: PlayerId,
        command: &PlayerCommand,
    ) -> Option<(PlayerScopedCommandOutcome, Vec<GameplayEventKind>)> {
        let PlayerCommand::Service(request) = command else {
            return None;
        };
        let events = match *request {
            SessionServiceRequest::AcceptQuest { quest, crew } => {
                self.accept_quest(player_id, quest, crew)
            }
            SessionServiceRequest::TurnInQuest { quest } => self.turn_in_quest(player_id, quest),
            SessionServiceRequest::AbandonQuest { quest } => self.abandon_quest(player_id, quest),
            _ => return None,
        };
        Some(events.map_or_else(
            || (PlayerScopedCommandOutcome::IgnoredUnavailable, Vec::new()),
            |events| (PlayerScopedCommandOutcome::Applied, events),
        ))
    }

    /// Whether `player_id` stands at `giver`'s counter.
    fn at_giver(&self, player_id: PlayerId, giver: QuestGiver) -> bool {
        self.player_surface_zone(player_id) == Some(giver.zone())
    }

    fn accept_quest(
        &mut self,
        player_id: PlayerId,
        quest_id: QuestId,
        crew_requested: bool,
    ) -> Option<Vec<GameplayEventKind>> {
        let def = quest_id.def()?;
        if !self.at_giver(player_id, def.giver)
            || self.quest_availability(player_id, def) != QuestAvailability::Available
        {
            return None;
        }
        let day = self.progression.town_event_day;
        let (variant, salt, slot) = if def.repeatable {
            let posting = self
                .board_postings()
                .into_iter()
                .find(|posting| posting.quest == quest_id)?;
            let log = &self.players.get(&player_id)?.quests;
            if log.board_day == day && log.board_taken.contains(&posting.slot) {
                return None;
            }
            (posting.variant, posting.salt, Some(posting.slot))
        } else {
            (0, 0, None)
        };
        let taken: BTreeSet<FeatureId> = self
            .players
            .values()
            .flat_map(|player| player.quests.active.iter())
            .chain(self.progression.crew_quests.active.iter())
            .flat_map(|quest| quest.targets.iter().filter_map(|target| target.feature))
            .collect();
        let player = self.players.get(&player_id)?;
        let (targets, objectives) = bind_quest(
            def,
            &self.terrain,
            player.tile_position(TILE_SIZE),
            &player.journal,
            salt,
            &taken,
        )?;
        let now = self.simulation_tick.get();
        let deadline = def
            .active_twists(variant)
            .into_iter()
            .find_map(|twist| match twist {
                Twist::TimeLimit { seconds } => Some(
                    now.saturating_add(self.timing.ticks_for_millis(u64::from(seconds) * 1_000)),
                ),
                _ => None,
            });
        let crew = crew_requested && def.is_coop() && self.players.len() >= 2;
        let quest = ActiveQuest {
            quest: quest_id,
            owner: player_id,
            crew,
            variant,
            accepted_tick: now,
            deadline,
            stage: 0,
            targets,
            objectives,
            damage_taken: 0,
            last_hull: hull_tenths(player.hull),
            carrying: false,
            descended: false,
            ready: false,
            helpers: BTreeSet::new(),
        };
        let player = self.players.get_mut(&player_id)?;
        if crew {
            self.progression.crew_quests.active.push(quest);
        } else {
            player.quests.active.push(quest);
        }
        if let Some(slot) = slot {
            if player.quests.board_day != day {
                player.quests.board_day = day;
                player.quests.board_taken.clear();
            }
            player.quests.board_taken.insert(slot);
        }
        Some(vec![GameplayEventKind::QuestAccepted {
            player_id,
            quest: quest_id,
        }])
    }

    /// Removes and returns a quest `player_id` owns.
    fn take_owned_quest(&mut self, player_id: PlayerId, quest_id: QuestId) -> Option<ActiveQuest> {
        if let Some(player) = self.players.get_mut(&player_id)
            && let Some(index) = player
                .quests
                .active
                .iter()
                .position(|quest| quest.quest == quest_id)
        {
            return Some(player.quests.active.remove(index));
        }
        let crew = &mut self.progression.crew_quests.active;
        let index = crew
            .iter()
            .position(|quest| quest.quest == quest_id && quest.owner == player_id)?;
        Some(crew.remove(index))
    }

    fn abandon_quest(
        &mut self,
        player_id: PlayerId,
        quest_id: QuestId,
    ) -> Option<Vec<GameplayEventKind>> {
        let def = quest_id.def()?;
        if !self.at_giver(player_id, def.giver) {
            return None;
        }
        self.take_owned_quest(player_id, quest_id)?;
        Some(Vec::new())
    }

    fn turn_in_quest(
        &mut self,
        player_id: PlayerId,
        quest_id: QuestId,
    ) -> Option<Vec<GameplayEventKind>> {
        let def = quest_id.def()?;
        if !self.at_giver(player_id, def.giver) {
            return None;
        }
        let quest = self
            .owned_quests(player_id)
            .find(|quest| quest.quest == quest_id)?
            .clone();
        if !quest.ready {
            return None;
        }
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
        let player = self.players.get(&player_id)?;
        if deliveries
            .iter()
            .any(|(item, count)| item.held(player) < *count)
        {
            return None;
        }
        self.take_owned_quest(player_id, quest_id)?;
        let player = self.players.get_mut(&player_id)?;
        for (item, count) in deliveries {
            item.take(player, count);
        }
        let mut events = self.grant_quest_reward(player_id, def);
        let share = def.reward.credits / 4;
        for helper in quest.helpers.iter().filter(|helper| **helper != player_id) {
            if let Some(player) = self.players.get_mut(helper) {
                player.credits = player.credits.saturating_add(share);
                *player.quests.reputation.entry(def.giver).or_default() +=
                    def.reward.reputation / 2;
            }
        }
        let log = &mut self.players.get_mut(&player_id)?.quests;
        if def.repeatable {
            log.board_completed = log.board_completed.saturating_add(1);
        } else {
            log.completed.insert(quest_id);
        }
        events.push(GameplayEventKind::QuestCompleted {
            player_id,
            quest: quest_id,
        });
        Some(events)
    }

    /// Pays `def`'s reward to `player_id`. Returns blueprint events.
    fn grant_quest_reward(
        &mut self,
        player_id: PlayerId,
        def: &QuestDef,
    ) -> Vec<GameplayEventKind> {
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
        let mut events = Vec::new();
        if let Some(player) = self.players.get_mut(&player_id) {
            for (material, count) in reward.materials {
                player.add_material(*material, *count);
            }
            *player.quests.reputation.entry(def.giver).or_default() += reward.reputation;
        }
        if let Some((blueprint, count)) = reward.fragment {
            for _ in 0..count {
                events.extend(self.grant_blueprint_fragment(player_id, blueprint));
            }
        }
        events
    }

    /// Adds a blueprint fragment to `player_id`; a finished blueprint unlocks what it draws.
    pub(super) fn grant_blueprint_fragment(
        &mut self,
        player_id: PlayerId,
        blueprint: BlueprintId,
    ) -> Option<GameplayEventKind> {
        let player = self.players.get_mut(&player_id)?;
        let finished = player.quests.add_fragment(blueprint)?;
        if let BlueprintUnlock::Part(part) = finished.unlock() {
            self.progression.rig_part_inventory.insert(part);
        }
        Some(GameplayEventKind::BlueprintCompleted {
            player_id,
            blueprint: finished,
        })
    }

    /// Percent a giver hands back on `player_id`'s purchase (reputation perk).
    pub(super) fn quest_rebate_percent(&self, player_id: PlayerId, giver: QuestGiver) -> u32 {
        self.players
            .get(&player_id)
            .map_or(0, |player| giver.rebate_percent(player.quests.rank(giver)))
    }

    /// Hands back the reputation rebate on `spent` credits.
    pub(super) fn apply_quest_rebate(
        &mut self,
        player_id: PlayerId,
        giver: QuestGiver,
        spent: u32,
    ) {
        let rebate = spent.saturating_mul(self.quest_rebate_percent(player_id, giver)) / 100;
        if rebate > 0
            && let Some(player) = self.players.get_mut(&player_id)
        {
            player.credits = player.credits.saturating_add(rebate);
        }
    }

    /// Advances every quest one tick (called near the end of the tick).
    pub(super) fn advance_quests(
        &mut self,
        next_tick: SimulationTick,
        notes: &QuestNotes,
        output: &mut TickOutput,
    ) {
        self.track_quest_history();
        let any_active = !self.progression.crew_quests.active.is_empty()
            || self
                .players
                .values()
                .any(|player| !player.quests.active.is_empty());
        if !any_active {
            return;
        }
        let facts = EventFacts::read(output);
        let tick = next_tick.get();
        let mut events = Vec::new();
        let owners: Vec<PlayerId> = self.players.keys().copied().collect();
        for owner in owners {
            let Some(player) = self.players.get_mut(&owner) else {
                continue;
            };
            let mut quests = std::mem::take(&mut player.quests.active);
            quests.retain_mut(|quest| {
                self.step_quest_or_fail(quest, tick, notes, &facts, &mut events)
            });
            if let Some(player) = self.players.get_mut(&owner) {
                quests.append(&mut player.quests.active);
                player.quests.active = quests;
            }
        }
        let mut crew = std::mem::take(&mut self.progression.crew_quests.active);
        crew.retain(|quest| self.players.contains_key(&quest.owner));
        crew.retain_mut(|quest| self.step_quest_or_fail(quest, tick, notes, &facts, &mut events));
        crew.append(&mut self.progression.crew_quests.active);
        self.progression.crew_quests.active = crew;
        for event in events {
            output.push_event(event);
        }
    }

    /// Keeps each player's deepest row and visited POI kinds (quest prerequisites).
    fn track_quest_history(&mut self) {
        let Self {
            players, terrain, ..
        } = self;
        let has_pois = terrain.settings().generator.has_points_of_interest();
        for player in players.values_mut() {
            let row = player.tile_position(TILE_SIZE).y;
            if row > player.quests.deepest_row {
                player.quests.deepest_row = row;
            }
            if has_pois && player.journal.visited_count() > 0 {
                let features = terrain.features();
                let kinds: Vec<_> = player
                    .journal
                    .known
                    .iter()
                    .filter(|(_, knowledge)| **knowledge == crate::poi::PoiKnowledge::Visited)
                    .filter_map(|(feature, _)| features.get(*feature)?.poi_kind())
                    .filter(|kind| !player.quests.visited_kinds.contains(kind))
                    .collect();
                player.quests.visited_kinds.extend(kinds);
            }
        }
    }

    /// Steps one quest; returns whether it is still active (a failed quest is recorded and
    /// dropped).
    fn step_quest_or_fail(
        &mut self,
        quest: &mut ActiveQuest,
        tick: u64,
        notes: &QuestNotes,
        facts: &EventFacts,
        events: &mut Vec<GameplayEventKind>,
    ) -> bool {
        match self.step_quest(quest, tick, notes, facts, events) {
            Ok(()) => true,
            Err(reason) => {
                if let Some(player) = self.players.get_mut(&quest.owner) {
                    *player.quests.failures.entry(quest.quest).or_default() += 1;
                }
                events.push(GameplayEventKind::QuestFailed {
                    player_id: quest.owner,
                    quest: quest.quest,
                    reason,
                });
                false
            }
        }
    }

    /// Checks twists, then advances the current stage's objectives.
    fn step_quest(
        &mut self,
        quest: &mut ActiveQuest,
        tick: u64,
        notes: &QuestNotes,
        facts: &EventFacts,
        events: &mut Vec<GameplayEventKind>,
    ) -> Result<(), QuestFailure> {
        let Some(def) = quest.def() else {
            return Ok(());
        };
        let Some(owner) = self.players.get(&quest.owner) else {
            return Ok(());
        };
        let owner_tile = owner.tile_position(TILE_SIZE);
        let owner_hull = hull_tenths(owner.hull);
        let twists = def.active_twists(quest.variant);
        let has_parcel = def
            .objectives()
            .any(|objective| matches!(objective, Objective::PickUp { .. }));
        let damage = quest.last_hull.saturating_sub(owner_hull);
        quest.last_hull = owner_hull;
        if !quest.ready && (quest.carrying || !has_parcel) {
            quest.damage_taken = quest.damage_taken.saturating_add(damage);
        }
        if owner_tile.y > SURFACE_ROW + STAY_DOWN_ARM_ROWS {
            quest.descended = true;
        }
        if !quest.ready {
            check_twists(quest, &twists, tick, owner_tile, facts)?;
        }
        if quest.ready {
            return Ok(());
        }
        let rigs: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .filter(|(id, _)| quest.crew || **id == quest.owner)
            .map(|(id, player)| (*id, player.tile_position(TILE_SIZE)))
            .collect();
        let stage = usize::from(quest.stage);
        let Some(objectives) = def.stages.get(stage) else {
            quest.ready = true;
            return Ok(());
        };
        let start = def.stage_start(stage);
        let mut helpers = Vec::new();
        for (offset, objective) in objectives.iter().enumerate() {
            let index = start + offset;
            let Some(mut progress) = quest.objectives.get(index).cloned() else {
                continue;
            };
            if progress.done {
                continue;
            }
            self.step_objective(
                *objective,
                &mut progress,
                quest,
                &rigs,
                tick,
                notes,
                facts,
                &mut helpers,
            );
            if let Some(slot) = quest.objectives.get_mut(index) {
                *slot = progress;
            }
        }
        if quest.crew {
            quest
                .helpers
                .extend(helpers.into_iter().filter(|helper| *helper != quest.owner));
        }
        // Deliveries count once the owner carries the goods (they are handed over at turn-in).
        let owner = self.players.get(&quest.owner);
        let stage_done = objectives
            .iter()
            .enumerate()
            .all(|(offset, objective)| match objective {
                Objective::Deliver { item, count } => {
                    owner.is_some_and(|player| item.held(player) >= *count)
                }
                _ => quest
                    .objectives
                    .get(start + offset)
                    .is_some_and(|progress| progress.done),
            });
        if stage_done {
            quest.stage = quest.stage.saturating_add(1);
            if usize::from(quest.stage) >= def.stages.len() {
                quest.ready = true;
                events.push(GameplayEventKind::QuestReady {
                    player_id: quest.owner,
                    quest: quest.quest,
                });
            } else {
                events.push(GameplayEventKind::QuestStageDone {
                    player_id: quest.owner,
                    quest: quest.quest,
                });
            }
        }
        Ok(())
    }

    /// Advances one objective primitive.
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "one arm per primitive keeps every objective rule in one place"
    )]
    fn step_objective(
        &mut self,
        objective: Objective,
        progress: &mut ObjectiveProgress,
        quest: &mut ActiveQuest,
        rigs: &[(PlayerId, TilePosition)],
        tick: u64,
        notes: &QuestNotes,
        facts: &EventFacts,
        helpers: &mut Vec<PlayerId>,
    ) {
        let target = |index: u8| quest.targets.get(usize::from(index)).copied();
        let inside = |target: QuestTarget, margin: i32| {
            let area = target.bounds.expanded(margin);
            rigs.iter()
                .filter(move |(_, tile)| area.contains(*tile))
                .map(|(id, _)| *id)
        };
        let ticks = |millis: u64| self.timing.ticks_for_millis(millis);
        match objective {
            Objective::Deliver { .. } => {}
            Objective::Reach { target: index } => {
                if let Some(target) = target(index) {
                    let arrived: Vec<PlayerId> = inside(target, 1).collect();
                    if !arrived.is_empty() {
                        helpers.extend(arrived);
                        progress.done = true;
                    }
                }
            }
            Objective::ReachDepth { row } => {
                if let Some((id, _)) = rigs.iter().find(|(_, tile)| tile.y >= row) {
                    helpers.push(*id);
                    progress.done = true;
                }
            }
            Objective::Dig {
                target: index,
                count,
            } => {
                if let Some(target) = target(index) {
                    let area = target.bounds.expanded(1);
                    for (player, position, _) in &notes.mined {
                        if area.contains(*position) && rigs.iter().any(|(id, _)| id == player) {
                            progress.count += 1;
                            helpers.push(*player);
                        }
                    }
                    progress.done = progress.count >= u64::from(count);
                }
            }
            Objective::Scan { target: index } => {
                if let Some(target) = target(index) {
                    for (id, tile) in rigs {
                        if !facts.scanned.contains(id) {
                            continue;
                        }
                        let level = self
                            .players
                            .get(id)
                            .map_or(0, |player| player.scanner_level);
                        let range = scanner_poi_range(level, false).max(10);
                        if in_scanner_range(&target.bounds, *tile, range) {
                            helpers.push(*id);
                            progress.done = true;
                        }
                    }
                }
            }
            Objective::OpenPoi { target: index } => {
                if let Some(target) = target(index) {
                    if target
                        .feature
                        .is_some_and(|feature| facts.vaults_opened.contains(&feature))
                    {
                        progress.done = true;
                    }
                    for (player, position, kind) in &notes.mined {
                        let loot = matches!(
                            kind,
                            TileKind::Poi(
                                PoiTile::SupplyCrate
                                    | PoiTile::SalvageCore
                                    | PoiTile::VaultCache
                                    | PoiTile::OutpostSalvage
                            )
                        );
                        if loot
                            && target.bounds.expanded(1).contains(*position)
                            && rigs.iter().any(|(id, _)| id == player)
                        {
                            helpers.push(*player);
                            progress.done = true;
                        }
                    }
                }
            }
            Objective::Survive {
                target: index,
                seconds,
            } => {
                if let Some(target) = target(index) {
                    let present: Vec<PlayerId> = inside(target, 2).collect();
                    if !present.is_empty() {
                        progress.count += 1;
                        helpers.extend(present);
                    }
                    progress.done = progress.count >= ticks(u64::from(seconds) * 1_000);
                }
            }
            Objective::PickUp { target: index, .. } => {
                if let Some(target) = target(index)
                    && inside(target, 1).any(|id| id == quest.owner)
                {
                    quest.carrying = true;
                    progress.done = true;
                }
            }
            Objective::ReturnToSurface => {
                if rigs
                    .iter()
                    .any(|(id, tile)| *id == quest.owner && tile.y <= SURFACE_ROW)
                {
                    progress.done = true;
                }
            }
            Objective::Build {
                target: index,
                kind,
            } => {
                if let Some(target) = target(index) {
                    let area = target.bounds.expanded(3);
                    progress.done = self
                        .infrastructure
                        .iter()
                        .any(|item| item.kind == kind && area.contains(item.position));
                }
            }
            Objective::Detonate { spots, .. } => {
                for blast in &notes.detonations {
                    let next = usize::try_from(progress.count).unwrap_or(usize::MAX);
                    let Some(expected) = progress.spots.get(next) else {
                        break;
                    };
                    if tile_distance(*blast, *expected) <= 2 {
                        progress.count += 1;
                    } else if progress
                        .spots
                        .iter()
                        .skip(next + 1)
                        .any(|spot| tile_distance(*blast, *spot) <= 2)
                    {
                        // Out of order: the sequence starts over.
                        progress.count = 0;
                    }
                }
                progress.done = progress.count >= u64::from(spots);
            }
            Objective::RecoverLog { target: index } => {
                if let Some(target) = target(index) {
                    for (player, position, kind) in &notes.mined {
                        if *kind == TileKind::Poi(PoiTile::SalvageCore)
                            && target.bounds.expanded(1).contains(*position)
                            && rigs.iter().any(|(id, _)| id == player)
                        {
                            helpers.push(*player);
                            progress.done = true;
                        }
                    }
                }
            }
            Objective::Simultaneous {
                window_ms,
                solo_window_ms,
                ..
            } => {
                for (spot, mark) in progress.spots.iter().zip(progress.marks.iter_mut()) {
                    if let Some((id, _)) = rigs
                        .iter()
                        .find(|(_, tile)| tile_distance(*tile, *spot) <= 1)
                    {
                        *mark = tick + 1;
                        helpers.push(*id);
                    }
                }
                let window_ms = simultaneous_window(
                    quest.crew,
                    rigs.len(),
                    progress.spots.len(),
                    window_ms,
                    solo_window_ms,
                );
                progress.window_ms = u32::try_from(window_ms).unwrap_or(u32::MAX);
                let window = ticks(window_ms);
                let lit = progress.marks.iter().all(|mark| *mark > 0);
                let first = progress.marks.iter().copied().min().unwrap_or(0);
                let last = progress.marks.iter().copied().max().unwrap_or(0);
                progress.done = lit && last - first <= window;
            }
            Objective::Relay {
                target: index,
                leg_rows,
                solo_cooldown_ms,
            } => {
                let Some(target) = target(index) else {
                    return;
                };
                let cooldown = ticks(u64::from(solo_cooldown_ms));
                if let Some(carrier) = progress.carrier {
                    let Some((_, position)) = rigs.iter().find(|(id, _)| *id == carrier) else {
                        progress.carrier = None;
                        progress.last_carrier = None;
                        return;
                    };
                    progress.position = Some(*position);
                    helpers.push(carrier);
                    if target.bounds.expanded(1).contains(*position) {
                        progress.done = true;
                        return;
                    }
                    // A rig can't take the marker straight back: the leg it handed over has
                    // to be carried a few rows first, or two rigs flying side by side would
                    // pass it back and forth every tick and never need a real relay.
                    let leg = (position.y - progress.anchor).abs();
                    if quest.crew
                        && let Some((next, _)) = rigs.iter().find(|(id, tile)| {
                            *id != carrier
                                && tile_distance(*tile, *position) <= 1
                                && (progress.last_carrier != Some(*id) || leg >= RELAY_MIN_LEG_ROWS)
                        })
                    {
                        progress.carrier = Some(*next);
                        progress.last_carrier = Some(carrier);
                        progress.anchor = position.y;
                        return;
                    }
                    if leg > i32::from(leg_rows) {
                        // Too hot to hold: it drops here and cools.
                        progress.carrier = None;
                        progress.last_carrier = Some(carrier);
                        progress.timer = tick + cooldown;
                    }
                } else if let Some(resting) = progress.position {
                    let taker = rigs.iter().find(|(id, tile)| {
                        tile_distance(*tile, resting) <= 1
                            && (progress.last_carrier != Some(*id) || tick >= progress.timer)
                    });
                    if let Some((id, _)) = taker {
                        progress.carrier = Some(*id);
                        progress.anchor = resting.y;
                        helpers.push(*id);
                    }
                }
            }
            Objective::HoldAndClimb {
                goal,
                solo_latch_ms,
                ..
            } => {
                let (Some(switch), Some(goal)) = (progress.spots.first().copied(), target(goal))
                else {
                    return;
                };
                let holder = rigs
                    .iter()
                    .find(|(_, tile)| tile_distance(*tile, switch) <= 1)
                    .map(|(id, _)| *id);
                let anchored = self
                    .infrastructure
                    .iter()
                    .any(|item| tile_distance(item.position, switch) <= 2);
                if holder.is_some() || anchored {
                    let latch = if quest.crew {
                        CREW_LATCH_MILLIS
                    } else {
                        u64::from(solo_latch_ms)
                    };
                    progress.timer = tick + ticks(latch);
                    helpers.extend(holder);
                }
                let held = progress.timer > tick;
                let climber: Vec<PlayerId> = inside(goal, 1).collect();
                if held && !climber.is_empty() {
                    helpers.extend(climber);
                    progress.done = true;
                }
            }
            Objective::Escort {
                step_ms,
                crew_step_ms,
                ..
            } => {
                let Some(haul) = progress.position else {
                    return;
                };
                if haul.y <= SURFACE_ROW {
                    progress.done = true;
                    return;
                }
                let near: Vec<PlayerId> = rigs
                    .iter()
                    .filter(|(_, tile)| tile_distance(*tile, haul) <= ESCORT_RADIUS)
                    .map(|(id, _)| *id)
                    .collect();
                if near.is_empty() || tick < progress.timer {
                    return;
                }
                if let Some(step) = self.escort_step(haul) {
                    progress.position = Some(step);
                    let pace = if quest.crew && near.len() >= 2 {
                        crew_step_ms
                    } else {
                        step_ms
                    };
                    progress.timer = tick + ticks(u64::from(pace));
                    helpers.extend(near);
                    progress.done = step.y <= SURFACE_ROW;
                }
            }
            Objective::SplitDelivery {
                item,
                count,
                zones,
                window_ms,
                solo_window_ms,
            } => {
                progress.counts.resize(2, 0);
                progress.marks.resize(2, 0);
                let window = ticks(u64::from(if quest.crew {
                    window_ms
                } else {
                    solo_window_ms
                }));
                for (zone_index, zone) in zones.iter().enumerate() {
                    if progress.counts[zone_index] >= count {
                        continue;
                    }
                    for (id, _) in rigs {
                        if self.player_surface_zone(*id) != Some(*zone) {
                            continue;
                        }
                        let Some(player) = self.players.get_mut(id) else {
                            continue;
                        };
                        let taken = item.take(player, count - progress.counts[zone_index]);
                        if taken > 0 {
                            progress.counts[zone_index] += taken;
                            helpers.push(*id);
                        }
                        if progress.counts[zone_index] >= count {
                            progress.marks[zone_index] = tick + 1;
                            break;
                        }
                    }
                }
                // A finished half spoils if the other half takes too long.
                for zone_index in 0..2 {
                    let other = 1 - zone_index;
                    let mark = progress.marks[zone_index];
                    if mark > 0 && progress.marks[other] == 0 && tick + 1 - mark > window {
                        progress.marks[zone_index] = 0;
                        progress.counts[zone_index] = 0;
                    }
                }
                progress.done = progress.marks.iter().all(|mark| *mark > 0);
            }
        }
    }

    /// The haul's next tile on the shortest open path to the surface, if any.
    fn escort_step(&self, from: TilePosition) -> Option<TilePosition> {
        const REACH: i32 = 70;
        let min_x = (from.x - REACH).max(0);
        let max_x = (from.x + REACH).min(self.terrain.width() - 1);
        let max_y = (from.y + 12).min(self.terrain.height() - 1);
        let width = usize::try_from(max_x - min_x + 1).ok()?;
        let height = usize::try_from(max_y + 1).ok()?;
        let index = |position: TilePosition| -> Option<usize> {
            if position.x < min_x || position.x > max_x || position.y < 0 || position.y > max_y {
                return None;
            }
            let x = usize::try_from(position.x - min_x).ok()?;
            let y = usize::try_from(position.y).ok()?;
            Some(y * width + x)
        };
        let mut came_from: Vec<Option<TilePosition>> = vec![None; width * height];
        let mut seen = vec![false; width * height];
        let mut queue = VecDeque::new();
        seen[index(from)?] = true;
        queue.push_back(from);
        while let Some(current) = queue.pop_front() {
            if current.y <= SURFACE_ROW && current != from {
                let mut step = current;
                while let Some(previous) = came_from[index(step)?] {
                    if previous == from {
                        return Some(step);
                    }
                    step = previous;
                }
                return Some(step);
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TilePosition {
                    x: current.x + dx,
                    y: current.y + dy,
                };
                let Some(slot) = index(next) else {
                    continue;
                };
                if seen[slot] || self.terrain.is_solid_at(next) {
                    continue;
                }
                seen[slot] = true;
                came_from[slot] = Some(current);
                queue.push_back(next);
            }
        }
        None
    }
}

/// Hull in tenths (integer damage accounting).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "hull is a small non-negative quantized value"
)]
fn hull_tenths(hull: f32) -> u32 {
    (hull.max(0.0) * 10.0).round() as u32
}

/// Fails a quest whose twist caught the owner out this tick.
fn check_twists(
    quest: &ActiveQuest,
    twists: &[Twist],
    tick: u64,
    owner_tile: TilePosition,
    facts: &EventFacts,
) -> Result<(), QuestFailure> {
    for twist in twists {
        match *twist {
            Twist::TimeLimit { .. } => {
                if quest.deadline.is_some_and(|deadline| tick >= deadline) {
                    return Err(QuestFailure::OutOfTime);
                }
            }
            Twist::Fragile { max_damage } => {
                if quest.damage_taken > u32::from(max_damage) * 10 {
                    return Err(QuestFailure::TooMuchDamage);
                }
            }
            Twist::NoScanner => {
                if facts.scanned.contains(&quest.owner) {
                    return Err(QuestFailure::UsedScanner);
                }
            }
            Twist::NoBombs => {
                if facts.bombed.contains(&quest.owner) {
                    return Err(QuestFailure::UsedBombs);
                }
            }
            Twist::StayDown => {
                if quest.descended && owner_tile.y <= SURFACE_ROW {
                    return Err(QuestFailure::Surfaced);
                }
            }
        }
    }
    if quest.carrying && facts.rescued.contains(&quest.owner) {
        return Err(QuestFailure::Rescued);
    }
    Ok(())
}

#[cfg(test)]
mod window_tests {
    use super::simultaneous_window;

    #[test]
    fn simultaneous_windows_stretch_for_a_crew_short_of_rigs() {
        // Solo: always the solo window.
        assert_eq!(simultaneous_window(false, 3, 3, 4_000, 45_000), 45_000);
        // A rig per spot: the tight crew window.
        assert_eq!(simultaneous_window(true, 3, 3, 4_000, 45_000), 4_000);
        assert_eq!(simultaneous_window(true, 4, 2, 3_000, 25_000), 3_000);
        // Two rigs on three spots: halfway to the solo window.
        assert_eq!(simultaneous_window(true, 2, 3, 4_000, 45_000), 24_500);
        // One rig in a crew quest: as forgiving as solo.
        assert_eq!(simultaneous_window(true, 1, 3, 4_000, 45_000), 45_000);
    }
}
