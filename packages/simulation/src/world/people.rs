//! People in the world (WP-PEOPLE): NPC entities that live out their routines in villages and
//! the town, and what players do with them (talk, trade, gifts) inside the tick.
//!
//! # Population
//!
//! Every [`PEOPLE_REFRESH_MILLIS`] (and every tick while nobody is about), `advance_people`
//! spawns the residents of every settlement
//! (a village, an underground settlement, the town or a wanderers' camp, see
//! [`crate::people::routine`] and [`crate::people::camps`]) that an
//! actor ([`WorldState::people_actor_position`]) is within [`SPAWN_MARGIN_COLUMNS`] of, at the
//! place their routine has them at that moment, and despawns the people of settlements no actor
//! is within [`DESPAWN_MARGIN_COLUMNS`] of. Characters are regenerated from the seed, so a
//! person who despawns and respawns is the same person; everything that happened with them
//! lives in the [`crate::people::PeopleLedger`].
//!
//! # Walking
//!
//! Every tick each person's routine stop is read from the clock ([`WorldState::people_clock`]):
//! they walk toward it at their own pace on the street, step into their building when they reach
//! its door (hidden, `NpcState::inside`) and step out when the routine moves on. Once there they
//! potter: short strolls within the stop's roam, with pauses. Someone in a conversation stops and
//! faces the player. Motion is plain integer-seeded arithmetic on the fixed tick, and an entity
//! is only touched when something changed, so resting people cost no replication.
//!
//! # Actions
//!
//! [`crate::people::PeopleCommand`]s name the NPC entity they address. They need the actor
//! within [`TALK_REACH_TILES`] of the person outdoors, or at the door of the building the person
//! is inside (a keeper behind a counter, family at home). Talking runs the dialogue system on the
//! person's saved [`crate::people::CharacterState`]; trades take credits through the same paths
//! as village counters; gifts come out of the cargo bay. Goodwill changes with every interaction
//! and is remembered per player.
//!
//! People have no health: weapons, blasts and enemies never touch them, and every settlement is
//! an enemy safe zone.

use std::{collections::BTreeMap, collections::BTreeSet, sync::Arc};

use crate::{
    entity::{Body, EntityId, EntityKind, EntityState, NpcState},
    event::GameplayEventKind,
    ids::{PlayerId, SimulationTick},
    people::{
        self, CharacterId, ClockSource, DayTime, MemoryFact, PeopleCommand, Standing, TalkContext,
        TalkTopic,
        dialogue::Topic,
        ledger::{Conversation, ConversationLine, Speaker},
        offers::{self, GiftReaction, Perk, PersonDeal},
        routine::{Activity, Place, Settlement, TOWN_VILLAGE},
    },
    poi::PoiKnowledge,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TilePosition},
    village::{self, VillageId},
    worldgen::noise::hash2,
};

use super::{
    WorldState,
    villages::{CHART_RADIUS_COLUMNS, RUMOUR_RADIUS_COLUMNS},
};

/// How often populations are refreshed (spawned near actors, sent away from empty settlements).
pub const PEOPLE_REFRESH_MILLIS: u64 = 500;
/// People spawn while an actor is within this many columns of their settlement's span ...
pub const SPAWN_MARGIN_COLUMNS: i32 = 90;
/// ... and leave once every actor is further than this.
pub const DESPAWN_MARGIN_COLUMNS: i32 = 130;
/// Rows above or below a settlement's ground within which an actor brings its people out ...
pub const NEAR_ROWS: i32 = 14;
/// ... and the extra rows beyond that before they leave again (no churn at the boundary).
pub const KEEP_EXTRA_ROWS: i32 = 30;
/// How close (tiles) an actor must be to talk to someone outdoors.
pub const TALK_REACH_TILES: f32 = 3.0;
/// How long a person waits, mid-conversation, for a player who walked off (they then carry on).
const CONVERSATION_PATIENCE_TILES: f32 = 8.0;
/// Shortest and longest idle pause between strolls.
const IDLE_MILLIS: (u64, u64) = (2_500, 9_000);
/// Salt for strolls and pauses.
const SALT_STROLL: u64 = 0x5045_4F50_4C45_0040;
/// Most map sites a map secret reveals (one more with [`Perk::Surveyor`]).
const SECRET_SITES: usize = 1;
/// Diamond radius of map revealed around a map secret's site.
const SECRET_REVEAL_RADIUS: i32 = 5;
/// What people call a player whose name they do not know.
pub const DEFAULT_PLAYER_NAME: &str = "Driller";

impl WorldState {
    /// What players did with people (memories, conversations, perks).
    #[must_use]
    pub const fn people(&self) -> &people::PeopleLedger {
        &self.people
    }

    /// The time of day people live by: T14's world clock (Part II F4) as a [`DayTime`], so
    /// routines follow the world's day length and a skipped night wakes the village at dawn.
    /// This is the only place people ask the clock.
    #[must_use]
    pub fn people_clock(&self) -> DayTime {
        self.clock().day_time()
    }

    /// Where `player` stands for talking to people, in world pixels: the pilot's middle while
    /// on foot (Part II F5), else the rig ([`Self::actor_position`]). Reach checks and
    /// population follow it.
    #[must_use]
    pub fn people_actor_position(&self, player: PlayerId) -> Option<(f32, f32)> {
        self.actor_position(player)
    }

    /// The person entity `npc`'s state, if it is a person.
    #[must_use]
    pub fn npc(&self, npc: EntityId) -> Option<(&crate::entity::Entity, NpcState)> {
        let entity = self.entities.get(npc)?;
        match entity.state {
            EntityState::Npc(state) => Some((entity, state)),
            _ => None,
        }
    }

    /// Where person `npc` stands right now (world pixels, feet): their body moved along their
    /// current leg.
    #[must_use]
    pub fn npc_position(&self, npc: EntityId) -> Option<(f32, f32)> {
        let (entity, state) = self.npc(npc)?;
        Some((state.x_at(self.simulation_tick, 0.0), entity.body.y))
    }

    fn npc_body(&self, body: Body, state: &NpcState) -> Body {
        Body {
            x: state.x_at(self.simulation_tick, 0.0),
            ..body
        }
    }

    /// The person entity of `character`, if they are about.
    #[must_use]
    pub fn npc_of(&self, character: CharacterId) -> Option<EntityId> {
        self.entities
            .of_kind(EntityKind::Npc)
            .find(|entity| matches!(entity.state, EntityState::Npc(state) if state.character == character))
            .map(|entity| entity.id)
    }

    /// The people outdoors nearest `player`'s actor within talking reach, nearest first (the
    /// interact prompt and the shell's talk key use the first).
    #[must_use]
    pub fn npcs_in_reach(&self, player: PlayerId) -> Vec<EntityId> {
        let Some((x, y)) = self.people_actor_position(player) else {
            return Vec::new();
        };
        let reach = TALK_REACH_TILES * TILE_SIZE;
        let mut found: Vec<(u32, EntityId)> = self
            .entities
            .ids_in_radius(x, y, reach * 2.0)
            .into_iter()
            .filter_map(|id| self.npc(id))
            .filter(|(entity, state)| {
                state.inside.is_none() && outdoor_reach(self.npc_body(entity.body, state), x, y)
            })
            .map(|(entity, state)| {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a few tiles of pixels"
                )]
                let distance = (state.x_at(self.simulation_tick, 0.0) - x).abs() as u32;
                (distance, entity.id)
            })
            .collect();
        found.sort_unstable();
        found.into_iter().map(|(_, id)| id).collect()
    }

    /// The people inside lot `lot` of settlement `village` (a village building's room).
    #[must_use]
    pub fn npcs_inside(&self, village: VillageId, lot: usize) -> Vec<(EntityId, NpcState)> {
        self.entities
            .of_kind(EntityKind::Npc)
            .filter_map(|entity| match entity.state {
                EntityState::Npc(state)
                    if state.character.village == village
                        && state.inside.map(usize::from) == Some(lot) =>
                {
                    Some((entity.id, state))
                }
                _ => None,
            })
            .collect()
    }

    /// Whether `player`'s actor can talk to the person `npc`: within reach outdoors, or at the
    /// door of the building they are inside.
    #[must_use]
    #[allow(
        clippy::option_if_let_else,
        reason = "outdoors and indoors read best as a match"
    )]
    pub fn can_reach_npc(&self, player: PlayerId, npc: EntityId) -> bool {
        let Some((entity, state)) = self.npc(npc) else {
            return false;
        };
        let Some((x, y)) = self.people_actor_position(player) else {
            return false;
        };
        match state.inside {
            None => outdoor_reach(self.npc_body(entity.body, &state), x, y),
            Some(lot) => {
                state.character.village != TOWN_VILLAGE
                    && village::building_in(self.terrain.settings(), self.frame(), x, y)
                        .is_some_and(|(found, building)| {
                            found.id == state.character.village && building == usize::from(lot)
                        })
            }
        }
    }

    /// The name people call `player` by (their crew name, or [`DEFAULT_PLAYER_NAME`]).
    #[must_use]
    pub fn person_name_for(&self, player: PlayerId) -> String {
        self.crew_member(player)
            .map(|member| member.name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| DEFAULT_PLAYER_NAME.to_owned())
    }

    /// Keeps people in the settlements near actors, then walks everyone one tick.
    pub(super) fn advance_people(&mut self, next_tick: SimulationTick) {
        let settings = *self.terrain.settings();
        if !settings.generator.has_villages() {
            return;
        }
        let period = self.timing.ticks_for_millis(PEOPLE_REFRESH_MILLIS).max(1);
        // Also straight away while nobody is about (a fresh or loaded world, or a rig that just
        // warped in), so people are there from the first tick a rig stands near them.
        if next_tick.get().is_multiple_of(period) || self.entities.count_of(EntityKind::Npc) == 0 {
            self.refresh_population(next_tick);
        }
        self.walk_people(next_tick);
    }

    /// Settlement ids near any actor (`margin` columns beyond their span, `rows` above or below
    /// their ground), in id order.
    fn settlements_near_actors(
        &self,
        (margin, rows): (i32, i32),
    ) -> BTreeMap<VillageId, Arc<Settlement>> {
        let settings = *self.terrain.settings();
        let mut found = BTreeMap::new();
        let actors: Vec<(f32, f32)> = self
            .players
            .keys()
            .filter_map(|player| self.people_actor_position(*player))
            .collect();
        for (x, y) in actors {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "world pixels fit tiles in i32 within the streamed limit"
            )]
            let (column, row) = (self.frame().col(x), self.frame().row(y));
            let reach = margin + 80;
            let mut ids: Vec<VillageId> =
                village::villages_between(&settings, column - reach, column + reach)
                    .iter()
                    .map(|found| found.id)
                    .collect();
            // Sky towns (WP-SKY) near the actor, in two dimensions.
            ids.extend(
                village::sky_towns_in(
                    &settings,
                    &crate::worldgen::TileRect::around(
                        crate::terrain::TilePosition { x: column, y: row },
                        reach,
                        rows + 30,
                    ),
                )
                .iter()
                .map(|found| found.id),
            );
            ids.extend(
                people::camps::camps_between(&settings, column - reach, column + reach)
                    .iter()
                    .map(|camp| camp.id),
            );
            // Sky cities (T21, V9 sky continents).
            ids.extend(
                crate::worldgen::sky::continent::sky_cities_between(
                    &settings,
                    column - reach,
                    column + reach,
                )
                .iter()
                .map(|city| city.id),
            );
            ids.push(TOWN_VILLAGE);
            // The Moon's colonists (Wave 6), on worlds whose Moon has a colony.
            ids.extend(people::colony::colony_settlement_id(&settings));
            // Cathedral Station and the Amber Court (Wave 9). Nobody at the Court comes out to
            // talk while it is hostile to the crew.
            let court_hostile = self.living_dark.hive.court_hostile();
            ids.extend(
                people::deep_folk::settlement_ids(&settings)
                    .into_iter()
                    .filter(|id| !(court_hostile && people::deep_folk::is_hive_folk(*id))),
            );
            for id in ids {
                if found.contains_key(&id) {
                    continue;
                }
                let Some(settlement) = people::routine::settlement(&settings, id) else {
                    continue;
                };
                let near_columns = column >= settlement.first_column - margin
                    && column <= settlement.last_column + margin;
                let near_rows = (row - settlement.ground_row).abs() <= rows;
                if near_columns && near_rows {
                    found.insert(id, settlement);
                }
            }
        }
        found
    }

    fn refresh_population(&mut self, next_tick: SimulationTick) {
        let keep =
            self.settlements_near_actors((DESPAWN_MARGIN_COLUMNS, NEAR_ROWS + KEEP_EXTRA_ROWS));
        // Site crews (WP-HOME) are managed by `world/homes.rs`; their village copies step aside.
        let crew = self.crew_characters();
        let mut leaving = BTreeSet::new();
        let mut present = BTreeSet::new();
        for entity in self.entities.of_kind(EntityKind::Npc) {
            let EntityState::Npc(state) = entity.state else {
                continue;
            };
            if state.site.is_some() {
                present.insert(state.character);
                continue;
            }
            if crew.contains(&state.character) {
                leaving.insert(entity.id);
                continue;
            }
            if keep.contains_key(&state.character.village) && present.insert(state.character) {
                continue;
            }
            // Away from every actor (or a duplicate): gone until someone comes back.
            leaving.insert(entity.id);
        }
        if !leaving.is_empty() {
            self.entities.despawn_many(&leaving);
        }
        let spawn = self.settlements_near_actors((SPAWN_MARGIN_COLUMNS, NEAR_ROWS));
        let time = self.people_clock();
        for settlement in spawn.values() {
            for index in 0..settlement.residents.len() {
                let character = settlement.residents[index].character;
                if present.contains(&character) {
                    continue;
                }
                let Some(stop) = settlement.stop_at(index, time) else {
                    continue;
                };
                let column = settlement.place_column(stop.place);
                let spread = i32::try_from(index % 5).unwrap_or(0) - 2;
                let (x, inside) = match stop.place {
                    Place::Inside(lot) => (column_px(self.frame(), column), u8::try_from(lot).ok()),
                    Place::Outside { roam, .. } => {
                        let shift = spread * i32::from(roam.min(2));
                        (column_px(self.frame(), column + shift), None)
                    }
                };
                let state = NpcState {
                    character,
                    activity: stop.activity,
                    inside,
                    going_inside: inside,
                    from_x: x,
                    target_x: x,
                    depart: next_tick,
                    arrive: next_tick,
                    settled: false,
                    facing: if index % 2 == 0 { 1 } else { -1 },
                    talking_to: None,
                    idle_until: next_tick,
                    site: None,
                };
                self.spawn_entity(
                    Body::at(x, ground_px(self.frame(), settlement.ground_row)),
                    EntityState::Npc(state),
                    None,
                );
            }
        }
    }

    /// Walks everyone toward their routine's stop, one tick (to `next_tick`).
    ///
    /// A walk is a leg (`from_x` at `depart` to `target_x` at `arrive`) that positions are read
    /// from ([`NpcState::x_at`]), so a person's entity changes only when a leg starts or ends,
    /// they step through a door, or a conversation holds them: walking people cost a record per
    /// leg, not per tick, on the wire.
    #[allow(
        clippy::too_many_lines,
        reason = "one walking step reads best as a single pass"
    )]
    fn walk_people(&mut self, next_tick: SimulationTick) {
        let settings = *self.terrain.settings();
        let time = self.people_clock();
        let ids: Vec<EntityId> = self.entities.ids_of(EntityKind::Npc).collect();
        let mut settlements: BTreeMap<VillageId, Option<Arc<Settlement>>> = BTreeMap::new();
        for id in ids {
            let Some((entity, state)) = self.npc(id) else {
                continue;
            };
            let body = entity.body;
            let village = state.character.village;
            if state.site.is_some() {
                // On a building site: `world/homes.rs` walks them.
                continue;
            }
            let Some(settlement) = settlements
                .entry(village)
                .or_insert_with(|| people::routine::settlement(&settings, village))
                .clone()
            else {
                continue;
            };
            let index = usize::from(state.character.index);
            let Some(stop) = settlement.stop_at(index, time) else {
                continue;
            };
            let Some(resident) = settlement.residents.get(index) else {
                continue;
            };
            let speed = f32::from(resident.pace) / 100.0 * TILE_SIZE;
            let x = state.x_at(next_tick, 0.0);
            let walking = next_tick.get() < state.arrive.get();
            let mut next = state;
            next.activity = stop.activity;
            let wanted_inside = match stop.place {
                Place::Inside(lot) => u8::try_from(lot).ok(),
                Place::Outside { .. } => None,
            };
            let anchor = column_px(self.frame(), settlement.place_column(stop.place));
            let roam = match stop.place {
                Place::Outside { roam, .. } => f32::from(roam) * TILE_SIZE,
                Place::Inside(_) => 0.0,
            };
            // A conversation holds them on the spot, facing the player.
            if let Some(player) = state.talking_to {
                let listening = self
                    .people
                    .conversation(player)
                    .is_some_and(|conversation| conversation.npc == id)
                    && self.people_actor_position(player).is_some_and(|(px, py)| {
                        (px - x).abs() <= CONVERSATION_PATIENCE_TILES * TILE_SIZE
                            && (py - body.y).abs() <= CONVERSATION_PATIENCE_TILES * TILE_SIZE
                    });
                if listening {
                    if walking {
                        next.stand(x, next_tick);
                    }
                    if let Some((px, _)) = self.people_actor_position(player)
                        && state.inside.is_none()
                    {
                        next.facing = if px < x { -1 } else { 1 };
                    }
                    self.commit_npc(id, body, state, next);
                    continue;
                }
                next.talking_to = None;
            }
            if let Some(inside) = state.inside {
                if wanted_inside != Some(inside) {
                    // Out of the door.
                    let door = settlement
                        .lots
                        .get(usize::from(inside))
                        .map_or(anchor, |lot| column_px(self.frame(), lot.door_column()));
                    next.inside = None;
                    next.going_inside = None;
                    next.stand(door, next_tick);
                    next.settled = true;
                }
                self.commit_npc(id, body, state, next);
                continue;
            }
            if wanted_inside.is_some() {
                // Off to a building: walk to its door, then in.
                next.going_inside = wanted_inside;
                if (state.target_x - anchor).abs() > 0.5 {
                    next.walk(x, anchor, speed, next_tick, self.timing.rate.hz());
                } else if !walking {
                    next.inside = wanted_inside;
                    next.settled = true;
                }
                self.commit_npc(id, body, state, next);
                continue;
            }
            next.going_inside = None;
            let strayed = (state.target_x - anchor).abs() > roam + TILE_SIZE;
            if stop.activity != state.activity || strayed || state.going_inside.is_some() {
                // The routine moved on: head for the new stop.
                // Spread out a little within the stop's roam, so a crowd does not stack.
                #[allow(clippy::cast_precision_loss, reason = "a small spread")]
                let shift = (i32::from(state.character.index) % 5 - 2) as f32 * 0.25 * roam;
                next.walk(
                    x,
                    snap_px(anchor + shift),
                    speed,
                    next_tick,
                    self.timing.rate.hz(),
                );
            } else if !walking && !state.settled {
                // Arrived: a pause before the next stroll.
                let roll = hash2(
                    self.terrain.seed() ^ SALT_STROLL ^ 0x51,
                    u64::from(state.character.index) | (u64::from(village.0) << 16),
                    i32::try_from(next_tick.get() % 0x7FFF_FFFF).unwrap_or(0),
                    1,
                );
                let span = IDLE_MILLIS.1 - IDLE_MILLIS.0;
                let pause = IDLE_MILLIS.0 + roll % span.max(1);
                next.settled = true;
                next.idle_until =
                    SimulationTick::new(next_tick.get() + self.timing.ticks_for_millis(pause));
            } else if !walking && next_tick.get() >= state.idle_until.get() && roam > 0.0 {
                // A stroll somewhere within the stop's roam.
                let roll = hash2(
                    self.terrain.seed() ^ SALT_STROLL,
                    u64::from(state.character.index) | (u64::from(village.0) << 16),
                    i32::try_from(next_tick.get() % 0x7FFF_FFFF).unwrap_or(0),
                    0,
                );
                #[allow(clippy::cast_precision_loss, reason = "a stroll is a few tiles")]
                let offset = ((roll % 2001) as f32 / 1000.0 - 1.0) * roam;
                next.walk(
                    x,
                    snap_px(anchor + offset),
                    speed,
                    next_tick,
                    self.timing.rate.hz(),
                );
            }
            self.commit_npc(id, body, state, next);
        }
    }

    /// Writes a person's new state (and their body at the start of the current leg, for the
    /// spatial index and relevance) when anything changed.
    fn commit_npc(&mut self, id: EntityId, body: Body, before: NpcState, after: NpcState) {
        if before == after {
            return;
        }
        let mut next_body = body;
        next_body.x = after.from_x;
        next_body.previous_x = after.from_x;
        next_body.previous_y = body.y;
        next_body.velocity_x = 0.0;
        self.entities.update(id, |entity| {
            entity.body = next_body;
            entity.state = EntityState::Npc(after);
        });
    }

    /// Applies a people command, or `None` if `command` is not one.
    #[allow(clippy::option_if_let_else, reason = "one arm per command")]
    pub(super) fn people_command(
        &mut self,
        player_id: PlayerId,
        command: &crate::command::PlayerCommand,
    ) -> Option<(PlayerScopedCommandOutcome, Vec<GameplayEventKind>)> {
        let crate::command::PlayerCommand::People(command) = command else {
            return None;
        };
        let mut events = Vec::new();
        let outcome = match *command {
            PeopleCommand::Goodbye => {
                let ended = self.people.end_conversation(player_id);
                if let Some(npc) = ended {
                    self.release_npc(npc, player_id);
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                }
            }
            PeopleCommand::Talk { npc, topic } => self.talk_to(player_id, npc, topic, &mut events),
            PeopleCommand::Trade { npc, offer } => {
                self.trade_with(player_id, npc, offer, &mut events)
            }
            PeopleCommand::Gift { npc, item } => self.gift_to(player_id, npc, item, &mut events),
            PeopleCommand::AskWork { npc } => self.ask_work(player_id, npc, &mut events),
            PeopleCommand::AcceptArc { npc } => self.accept_arc(player_id, npc, &mut events),
            PeopleCommand::TurnInArc { npc, outcome } => {
                self.turn_in_arc(player_id, npc, outcome, &mut events)
            }
            PeopleCommand::AbandonArc { arc } => self.abandon_arc(player_id, arc, &mut events),
        };
        Some((outcome, events))
    }

    /// Lets a person walk on (their conversation with `player` is over).
    fn release_npc(&mut self, npc: EntityId, player: PlayerId) {
        let held = self
            .npc(npc)
            .is_some_and(|(_, state)| state.talking_to == Some(player));
        if held {
            self.entities.update(npc, |entity| {
                if let EntityState::Npc(state) = &mut entity.state {
                    state.talking_to = None;
                }
            });
        }
    }

    /// The person behind `npc` if `player_id` can reach them: their settlement and character.
    pub(super) fn reachable_person(
        &self,
        player_id: PlayerId,
        npc: EntityId,
    ) -> Option<(Arc<Settlement>, people::Character, NpcState)> {
        if !self.can_reach_npc(player_id, npc) {
            return None;
        }
        let (_, state) = self.npc(npc)?;
        let settlement =
            people::routine::settlement(self.terrain.settings(), state.character.village)?;
        let character = settlement
            .population
            .characters
            .get(usize::from(state.character.index))?
            .clone();
        Some((settlement, character, state))
    }

    /// Opens (or continues) `player_id`'s conversation with `npc` with `lines`.
    pub(super) fn converse(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        character: CharacterId,
        lines: Vec<ConversationLine>,
    ) {
        let journal = self.people.players.entry(player_id).or_default();
        match &mut journal.conversation {
            Some(conversation) if conversation.npc == npc => conversation.push(lines),
            other => {
                let mut conversation = Conversation {
                    npc,
                    character,
                    lines: Vec::new(),
                    exchange: 0,
                };
                conversation.push(lines);
                let previous = other.replace(conversation).map(|old| old.npc);
                if let Some(previous) = previous.filter(|previous| *previous != npc) {
                    self.release_npc(previous, player_id);
                }
            }
        }
        let facing_x = self.people_actor_position(player_id).map(|(x, _)| x);
        let now = self.simulation_tick;
        self.entities.update(npc, |entity| {
            if let EntityState::Npc(state) = &mut entity.state {
                let x = state.x_at(now, 0.0);
                state.talking_to = Some(player_id);
                if let Some(at) = facing_x {
                    state.facing = if at < x { -1 } else { 1 };
                }
                // Standing still to talk.
                if state.inside.is_none() {
                    state.stand(x, now);
                    entity.body.x = x;
                    entity.body.previous_x = x;
                }
            }
        });
    }

    /// Reports a change of standing since `before`.
    pub(super) fn warm(
        &self,
        player_id: PlayerId,
        npc: EntityId,
        character: CharacterId,
        before: Standing,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let after = self.people.standing(character, player_id);
        if after != before {
            let rank = |standing: Standing| match standing {
                Standing::Cold => 0,
                Standing::Stranger => 1,
                Standing::Acquainted => 2,
                Standing::Friendly => 3,
                Standing::Close => 4,
            };
            events.push(GameplayEventKind::AffinityChanged {
                player_id,
                npc: npc.get(),
                standing: after,
                warmer: rank(after) > rank(before),
            });
        }
    }

    /// Goodwill `amount` (before the charmer bonus) from `character` toward `player_id`.
    pub(super) fn add_goodwill(
        &mut self,
        player_id: PlayerId,
        character: CharacterId,
        amount: i16,
    ) {
        let amount = if amount > 0 && self.people.has_perk(player_id, Perk::Charmer) {
            amount + (amount + 1) / 2
        } else {
            amount
        };
        self.people
            .characters
            .entry(character)
            .or_default()
            .hear_about(player_id, amount);
    }

    /// Passes the change in `character`'s affinity for `player_id` since `before` on to their
    /// circle (family shares half, friends and sweethearts a third, rivals a quarter the other
    /// way; [`people::Population::reputation_ripple`]): word gets around a settlement. One step
    /// only, so a rumour never echoes back.
    pub(super) fn spread_word(
        &mut self,
        player_id: PlayerId,
        settlement: &Settlement,
        character: CharacterId,
        before: i16,
    ) {
        let delta = self.people.affinity(character, player_id) - before;
        if delta == 0 {
            return;
        }
        let mut ripple = settlement.population.reputation_ripple(character, delta);
        // Stories change relationships (WP-STORY `Relationship` endings): a feud ended or a
        // romance begun passes word on its new terms.
        for resident in &settlement.population.characters {
            if let Some(bond) = self.people.bond(character, resident.id) {
                ripple.retain(|(other, _)| *other != resident.id);
                let share = bond.map_or(0, |kind| kind.reputation_share(delta));
                if share != 0 {
                    ripple.push((resident.id, share));
                }
            }
        }
        for (other, share) in ripple {
            self.people
                .characters
                .entry(other)
                .or_default()
                .hear_about(player_id, share);
        }
    }

    pub(super) fn talk_context_facts(
        &self,
        settlement: &Settlement,
    ) -> Arc<Vec<people::WorldFact>> {
        let settings = self.terrain.settings();
        if settlement.village == TOWN_VILLAGE || people::camps::is_camp(settlement.village) {
            return Arc::new(Vec::new());
        }
        village::village(settings, settlement.village).map_or_else(
            || Arc::new(Vec::new()),
            |found| village::village_facts(settings, &found),
        )
    }

    #[allow(clippy::too_many_lines, reason = "one flat match over the topics")]
    fn talk_to(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        topic: TalkTopic,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some((settlement, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let id = character.id;
        let time = self.people_clock();
        let facts = self.talk_context_facts(&settlement);
        let player_name = self.person_name_for(player_id);
        let ctx = TalkContext {
            seed: self.terrain.seed(),
            day: time.day,
            player: player_id,
            player_name: &player_name,
            population: Some(&settlement.population),
            facts: &facts,
        };
        let before = self.people.standing(id, player_id);
        let affinity_before = self.people.affinity(id, player_id);
        let first_meeting = before == Standing::Stranger;
        let continuing = self
            .people
            .conversation(player_id)
            .is_some_and(|conversation| conversation.npc == npc);
        // A contractor building for this player talks about the job (WP-HOME).
        let site_line = self.site_talk(id, player_id);
        // The settlements that remember the Fathers' expedition (Wave 8 integration: Gravel
        // Hollow; Wave 9: Cathedral Station's and the Amber Court's residents, and any
        // settlement registered by name, see `expedition::memories`).
        let remembers = remembering(self.terrain.settings(), &settlement);
        let resident = people::deep_folk::resident_speaker(&character);
        let mut lines: Vec<ConversationLine> = Vec::new();
        let mut marked = None;
        {
            let state = self.people.characters.entry(id).or_default();
            let said = |line: Option<people::dialogue::Line>| {
                line.map(|line| ConversationLine {
                    speaker: Speaker::Them,
                    text: line.text,
                })
            };
            match topic {
                TalkTopic::Chat => {
                    if continuing {
                        lines.push(you("Tell me more."));
                    }
                    let visit = state.memory(player_id).talks;
                    let spoken = people::talk(&character, state, &ctx);
                    if let Some((remembered, speaker)) = resident {
                        // A deep settlement's resident (Wave 9): their own greeting, the
                        // day's word, then what they remember of the Fathers, in their voice.
                        if !continuing {
                            lines.extend(people::deep_folk::greeting(&character).map(|text| {
                                ConversationLine {
                                    speaker: Speaker::Them,
                                    text: text.to_owned(),
                                }
                            }));
                        }
                        // Residents with their own talk say it (the Archive Post and Heart's
                        // Edge, Wave 11 carryover); hive folk keep to their own words; station
                        // folk chat as well.
                        if let Some(text) = people::deep_folk::chatter(&character, time.day, visit)
                        {
                            lines.push(ConversationLine {
                                speaker: Speaker::Them,
                                text: text.to_owned(),
                            });
                        } else if !people::deep_folk::is_hive_folk(id.village) {
                            lines.extend(spoken.into_iter().skip(1).map(|line| ConversationLine {
                                speaker: Speaker::Them,
                                text: line.text,
                            }));
                        }
                        lines.extend(resident_memory(
                            &self.expedition,
                            remembered,
                            speaker,
                            visit,
                        ));
                    } else {
                        // Mid-conversation the greeting is already said.
                        let skip = usize::from(continuing && spoken.len() > 1);
                        lines.extend(spoken.into_iter().skip(skip).map(|line| ConversationLine {
                            speaker: Speaker::Them,
                            text: line.text,
                        }));
                        // Every other visit the talk turns to the Fathers: in the speaker's
                        // own voice where they speak for a memory, else passed on.
                        if let Some(remembered) = remembers
                            && visit % 2 == 1
                        {
                            let own =
                                remembered
                                    .speaker_for_role(character.role)
                                    .and_then(|speaker| {
                                        resident_memory(
                                            &self.expedition,
                                            remembered,
                                            speaker,
                                            visit / 2,
                                        )
                                    });
                            lines.extend(own.or_else(|| {
                                fathers_memory(&self.expedition, remembered, visit / 2)
                            }));
                        }
                    }
                }
                TalkTopic::Rumours => {
                    lines.push(you("Heard anything interesting lately?"));
                    if state.memory(player_id).first_met_day.is_none() {
                        state.remember(player_id, time.day, MemoryFact::Met);
                    }
                    lines.extend(said(people::say(&character, state, &ctx, Topic::Rumour)));
                    if let Some(remembered) = remembers {
                        let visit = state.memory(player_id).talks;
                        lines.extend(fathers_memory(&self.expedition, remembered, visit));
                    }
                }
                TalkTopic::Life => {
                    lines.push(you("What keeps you busy, these days?"));
                    if state.memory(player_id).first_met_day.is_none() {
                        state.remember(player_id, time.day, MemoryFact::Met);
                    }
                    let topic = match state.memory(player_id).talks % 3 {
                        0 => Topic::Want,
                        1 => Topic::Value,
                        _ => Topic::Fear,
                    };
                    lines.extend(said(people::say(&character, state, &ctx, Topic::Work)));
                    lines.extend(said(people::say(&character, state, &ctx, topic)));
                    state.players.entry(player_id).or_default().talks += 1;
                }
                TalkTopic::BuildingWork => {
                    lines.push(you("Do you take building work?"));
                    if state.memory(player_id).first_met_day.is_none() {
                        state.remember(player_id, time.day, MemoryFact::Met);
                    }
                    let text = site_line.unwrap_or_else(|| {
                        people::contractor::contractor_profile(&character).map_or_else(
                            || {
                                "Building? Not me. Ask someone with calluses in the right places."
                                    .to_owned()
                            },
                            |profile| profile.pitch(&character),
                        )
                    });
                    lines.push(ConversationLine {
                        speaker: Speaker::Them,
                        text,
                    });
                }
            }
        }
        // A story beat that sent the player to this person lands here (WP-STORY).
        lines.extend(self.arc_contact_lines(player_id, id));
        if topic == TalkTopic::Rumours {
            marked = self.free_rumour(player_id, &settlement, id, time.day);
            match marked {
                Some((feature, note)) => {
                    lines.push(note_line(note));
                    events.push(GameplayEventKind::RumourMarked {
                        player_id,
                        npc: npc.get(),
                        feature,
                    });
                }
                None => lines.push(note_line(
                    "Nothing new to mark on your map today.".to_owned(),
                )),
            }
        }
        // A friendly word once a day counts as goodwill.
        let today = time.day;
        let journal = self.people.players.entry(player_id).or_default();
        let fresh = journal.chat_days.get(&id) != Some(&today);
        if fresh {
            journal.chat_days.insert(id, today);
            self.add_goodwill(player_id, id, 2);
        }
        self.converse(player_id, npc, id, lines);
        events.push(GameplayEventKind::PersonTalked {
            player_id,
            npc: npc.get(),
            first_meeting,
        });
        self.spread_word(player_id, &settlement, id, affinity_before);
        self.warm(player_id, npc, id, before, events);
        let _ = marked;
        PlayerScopedCommandOutcome::Applied
    }

    /// Once a day per person (unless they hold a grudge), a rumour marks the nearest unknown
    /// site around their home on the player's map as an echo. Returns the site and a note.
    fn free_rumour(
        &mut self,
        player_id: PlayerId,
        settlement: &Settlement,
        character: CharacterId,
        day: u32,
    ) -> Option<(crate::worldgen::FeatureId, String)> {
        if self.people.standing(character, player_id) == Standing::Cold {
            return None;
        }
        let journal = self.people.players.entry(player_id).or_default();
        if journal.rumour_days.get(&character) == Some(&day) {
            return None;
        }
        let around = TilePosition {
            x: settlement.center_column(),
            y: settlement.ground_row,
        };
        let site = self
            .unknown_sites_near(player_id, around, RUMOUR_RADIUS_COLUMNS)
            .into_iter()
            .next()?;
        self.people
            .players
            .entry(player_id)
            .or_default()
            .rumour_days
            .insert(character, day);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.journal.learn(site.id, PoiKnowledge::Echo);
        }
        Some((site.id, direction_note(around, site.center, "an echo")))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "validation, one arm per kind of deal, then the conversation"
    )]
    fn trade_with(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        row: u8,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some((settlement, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let id = character.id;
        let list = offers::offers_of(&character);
        let Some(offer) = list.get(usize::from(row)).copied() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let before = self.people.standing(id, player_id);
        let affinity_before = self.people.affinity(id, player_id);
        // A finished story can open someone's door wider: their rows sell as to a friend.
        let gate = if self.people.offers_unlocked(id, player_id)
            && matches!(before, Standing::Stranger | Standing::Acquainted)
        {
            Standing::Friendly
        } else {
            before
        };
        if !offers::standing_allows(gate, offer.needs) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        if offer.once
            && self
                .people
                .journal(player_id)
                .is_some_and(|journal| journal.bought.contains(&(id, row)))
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let haggler = self.people.has_perk(player_id, Perk::Haggler);
        let price = offers::price_for(&offer, before, haggler);
        let credits_before = self
            .players
            .get(&player_id)
            .map_or(0, |player| player.credits);
        let around = TilePosition {
            x: settlement.center_column(),
            y: settlement.ground_row,
        };
        let surveyor = usize::from(self.people.has_perk(player_id, Perk::Surveyor));
        let (outcome, note) = match offer.deal {
            PersonDeal::Goods(deal) => {
                // Metered patch-ups follow the same discounts as priced rows: close friends
                // get a fifth off, friends a tenth, and a haggler a tenth more.
                let metered = |percent: u32| {
                    let percent = match before {
                        Standing::Close => percent * 80 / 100,
                        Standing::Friendly => percent * 90 / 100,
                        _ => percent,
                    };
                    if haggler {
                        percent * offers::HAGGLER_PERCENT / 100
                    } else {
                        percent
                    }
                };
                let deal = match deal {
                    village::VillageDeal::Repair { percent } => village::VillageDeal::Repair {
                        percent: metered(percent),
                    },
                    village::VillageDeal::Refuel { percent } => village::VillageDeal::Refuel {
                        percent: metered(percent),
                    },
                    other => other,
                };
                (self.apply_goods(player_id, deal, price), None)
            }
            PersonDeal::MapSecret => {
                let sites: Vec<_> = self
                    .unknown_sites_near(player_id, around, CHART_RADIUS_COLUMNS)
                    .into_iter()
                    .take(SECRET_SITES + surveyor)
                    .collect();
                if sites.is_empty() || !self.charge(player_id, price) {
                    (PlayerScopedCommandOutcome::IgnoredUnavailable, None)
                } else {
                    for site in &sites {
                        let _ =
                            self.reveal_scanner_area(player_id, site.center, SECRET_REVEAL_RADIUS);
                    }
                    if let Some(player) = self.players.get_mut(&player_id) {
                        for site in &sites {
                            player.journal.learn(site.id, PoiKnowledge::Identified);
                        }
                    }
                    let note = direction_note(around, sites[0].center, "a secret site");
                    (PlayerScopedCommandOutcome::Applied, Some(note))
                }
            }
            PersonDeal::Rumour => {
                let sites: Vec<_> = self
                    .unknown_sites_near(player_id, around, RUMOUR_RADIUS_COLUMNS)
                    .into_iter()
                    .take(1 + surveyor)
                    .collect();
                if sites.is_empty() || !self.charge(player_id, price) {
                    (PlayerScopedCommandOutcome::IgnoredUnavailable, None)
                } else {
                    if let Some(player) = self.players.get_mut(&player_id) {
                        for site in &sites {
                            player.journal.learn(site.id, PoiKnowledge::Echo);
                        }
                    }
                    let note = direction_note(around, sites[0].center, "an echo");
                    (PlayerScopedCommandOutcome::Applied, Some(note))
                }
            }
            PersonDeal::Apprenticeship(perk) => {
                if self.people.has_perk(player_id, perk) || !self.charge(player_id, price) {
                    (PlayerScopedCommandOutcome::IgnoredUnavailable, None)
                } else {
                    self.people
                        .players
                        .entry(player_id)
                        .or_default()
                        .perks
                        .insert(perk);
                    (
                        PlayerScopedCommandOutcome::Applied,
                        Some(format!("Learned: {} ({}).", perk.name(), perk.effect())),
                    )
                }
            }
            PersonDeal::Furniture(piece) => {
                if self.charge(player_id, price) {
                    self.grant_furniture(player_id, piece, 1);
                    (
                        PlayerScopedCommandOutcome::Applied,
                        Some(format!(
                            "The {} goes to your furniture stock.",
                            piece.name()
                        )),
                    )
                } else {
                    (PlayerScopedCommandOutcome::IgnoredUnavailable, None)
                }
            }
        };
        if outcome != PlayerScopedCommandOutcome::Applied {
            return outcome;
        }
        let spent = credits_before.saturating_sub(
            self.players
                .get(&player_id)
                .map_or(0, |player| player.credits),
        );
        let time = self.people_clock();
        let player_name = self.person_name_for(player_id);
        let facts = self.talk_context_facts(&settlement);
        let ctx = TalkContext {
            seed: self.terrain.seed(),
            day: time.day,
            player: player_id,
            player_name: &player_name,
            population: Some(&settlement.population),
            facts: &facts,
        };
        let mut lines = vec![you(&format!(
            "I'll take the {}.",
            offer.title().to_lowercase()
        ))];
        {
            let state = self.people.characters.entry(id).or_default();
            if state.memory(player_id).first_met_day.is_none() {
                state.remember(player_id, time.day, MemoryFact::Met);
            }
            state.remember(player_id, time.day, MemoryFact::Traded);
            lines.extend(
                people::say(&character, state, &ctx, Topic::Thanks).map(|line| ConversationLine {
                    speaker: Speaker::Them,
                    text: line.text,
                }),
            );
        }
        let bought = if spent == 0 {
            format!("Done: {}.", offer.title())
        } else {
            format!("Bought {} for {spent} credits.", offer.title())
        };
        lines.push(note_line(bought));
        lines.extend(note.map(note_line));
        if offer.once {
            self.people
                .players
                .entry(player_id)
                .or_default()
                .bought
                .insert((id, row));
        }
        self.add_goodwill(player_id, id, 1);
        self.converse(player_id, npc, id, lines);
        events.push(GameplayEventKind::PersonTraded {
            player_id,
            npc: npc.get(),
            kind: offer.kind,
        });
        self.spread_word(player_id, &settlement, id, affinity_before);
        self.warm(player_id, npc, id, before, events);
        PlayerScopedCommandOutcome::Applied
    }

    fn gift_to(
        &mut self,
        player_id: PlayerId,
        npc: EntityId,
        item: crate::item::ItemKind,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        if !offers::giftable(item) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some((settlement, character, _)) = self.reachable_person(player_id, npc) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if player.cargo_bay.remove(item, 1) == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let id = character.id;
        let before = self.people.standing(id, player_id);
        let affinity_before = self.people.affinity(id, player_id);
        let reaction = offers::gift_reaction(&character, item);
        let liked = reaction != GiftReaction::Indifferent;
        let time = self.people_clock();
        let player_name = self.person_name_for(player_id);
        let facts = self.talk_context_facts(&settlement);
        let ctx = TalkContext {
            seed: self.terrain.seed(),
            day: time.day,
            player: player_id,
            player_name: &player_name,
            population: Some(&settlement.population),
            facts: &facts,
        };
        let mut lines = vec![you(&format!(
            "I brought you something: {}.",
            item_name(item)
        ))];
        {
            let state = self.people.characters.entry(id).or_default();
            if state.memory(player_id).first_met_day.is_none() {
                state.remember(player_id, time.day, MemoryFact::Met);
            }
            state.remember(player_id, time.day, MemoryFact::Gift { liked });
            lines.extend(
                people::say(&character, state, &ctx, Topic::Gift { liked }).map(|line| {
                    ConversationLine {
                        speaker: Speaker::Them,
                        text: line.text,
                    }
                }),
            );
        }
        match reaction {
            GiftReaction::Loved => {
                self.add_goodwill(player_id, id, 6);
                lines.push(note_line(format!("{} loved it.", character.name.short())));
            }
            GiftReaction::Liked => self.add_goodwill(player_id, id, 1),
            GiftReaction::Indifferent => {}
        }
        self.converse(player_id, npc, id, lines);
        events.push(GameplayEventKind::PersonGifted {
            player_id,
            npc: npc.get(),
            reaction,
        });
        events.push(GameplayEventKind::CargoChanged { player_id });
        self.spread_word(player_id, &settlement, id, affinity_before);
        self.warm(player_id, npc, id, before, events);
        PlayerScopedCommandOutcome::Applied
    }

    /// The activity of every person about, for tests and reports: `(character, activity,
    /// inside)`.
    #[must_use]
    pub fn people_about(&self) -> Vec<(CharacterId, Activity, Option<u8>)> {
        self.entities
            .of_kind(EntityKind::Npc)
            .filter_map(|entity| match entity.state {
                EntityState::Npc(state) => Some((state.character, state.activity, state.inside)),
                _ => None,
            })
            .collect()
    }
}

/// Whether an actor at `(x, y)` is close enough to talk to a person standing at `body`.
fn outdoor_reach(body: Body, x: f32, y: f32) -> bool {
    let reach = TALK_REACH_TILES * TILE_SIZE;
    // People stand on the street (their body is at their feet); rigs hover a little above it.
    (body.x - x).abs() <= reach && (body.y - TILE_SIZE - y).abs() <= reach + TILE_SIZE
}

/// World pixel of a column's centre.
#[allow(
    clippy::cast_precision_loss,
    reason = "columns fit f32 within the streamed limit"
)]
fn column_px(frame: crate::position::Frame, column: i32) -> f32 {
    (frame.rel_col(column) + 0.5) * TILE_SIZE
}

/// World pixel of the top of row `row` (where feet stand on the first solid row).
#[allow(clippy::cast_precision_loss, reason = "rows are small")]
fn ground_px(frame: crate::position::Frame, row: i32) -> f32 {
    frame.rel_row(row) * TILE_SIZE
}

/// A position rounded to whole pixels (strolls end on clean positions).
const fn snap_px(x: f32) -> f32 {
    x.round()
}

pub(super) fn you(text: &str) -> ConversationLine {
    ConversationLine {
        speaker: Speaker::You,
        text: text.to_owned(),
    }
}

pub(super) const fn note_line(text: String) -> ConversationLine {
    ConversationLine {
        speaker: Speaker::Note,
        text,
    }
}

/// "Marked on your map: an echo 140 columns east, deep down."
fn direction_note(from: TilePosition, to: TilePosition, what: &str) -> String {
    let dx = to.x - from.x;
    let side = if dx < 0 { "west" } else { "east" };
    let depth = to.y - from.y;
    let depth = match depth {
        i32::MIN..=-10 => "up high",
        -9..=15 => "near the surface",
        16..=80 => "below ground",
        _ => "deep down",
    };
    format!(
        "Marked on your map: {what} {} columns {side}, {depth}.",
        dx.unsigned_abs()
    )
}

/// A gift's display name.
fn item_name(item: crate::item::ItemKind) -> String {
    use crate::item::ItemKind;
    match item {
        ItemKind::Mineral(mineral) => format!("a nugget of {}", mineral.name().to_lowercase()),
        ItemKind::Artifact(artifact) => format!("a {}", artifact.name()),
        ItemKind::Material(material) => format!("some {}", material.name().to_lowercase()),
        _ => "a little something".to_owned(),
    }
}

/// What a settlement's people pass on about the Fathers' expedition on a crew's `visit` (Wave
/// 8 integration: `expedition::memories`, opened as the crew reads the trail): anyone's line,
/// with its speaker named.
fn fathers_memory(
    expedition: &crate::expedition::ExpeditionState,
    settlement: crate::expedition::memories::Settlement,
    visit: u32,
) -> Option<ConversationLine> {
    crate::expedition::memories::line_for_visit(settlement, expedition, visit).map(|memory| {
        ConversationLine {
            speaker: Speaker::Them,
            text: format!("The Fathers? We still tell of them. {}", memory.told()),
        }
    })
}

/// What `speaker` of `settlement` remembers of the Fathers, said in their own voice (Wave 9).
fn resident_memory(
    expedition: &crate::expedition::ExpeditionState,
    settlement: crate::expedition::memories::Settlement,
    speaker: &str,
    visit: u32,
) -> Option<ConversationLine> {
    crate::expedition::memories::line_by_for_visit(settlement, speaker, expedition, visit).map(
        |memory| ConversationLine {
            speaker: Speaker::Them,
            text: memory.line.to_owned(),
        },
    )
}

/// The memories `settlement`'s people tell, if it remembers the Fathers (Deep Descent worlds):
/// a deep settlement with residents, or one registered by its name.
fn remembering(
    settings: &crate::worldgen::WorldSettings,
    settlement: &Settlement,
) -> Option<crate::expedition::memories::Settlement> {
    if !settings.generator.has_descents() {
        return None;
    }
    people::deep_folk::DeepSettlement::of_id(settlement.village)
        .map(people::deep_folk::DeepSettlement::remembering)
        .or_else(|| {
            crate::expedition::memories::Settlement::of_place_name(
                &settlement.population.village_name,
            )
        })
}
