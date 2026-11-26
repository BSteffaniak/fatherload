//! The friendly alien species in the world (Wave 12 aliens track; see [`crate::alien`]).
//!
//! # Where
//!
//! A settled body's towns are part of its ground (the planet generator flattens and paves them
//! and buries their lexicon stones). Their residents are NPC entities of the body's store: the
//! people pass runs in a body's ground pass when the body has towns (`world/people.rs` asks
//! [`WorldState::alien_towns_here`]), so they walk their schedules, talk, take gifts and
//! remember players like every villager, in the planet's local frame.
//!
//! # Rules (all inside the tick, so they hold online)
//!
//! * **First contact**: a player whose actor steps into a town of a species it never met meets
//!   it ([`AlienEvent::FirstContact`]): the greeting is learned (understanding
//!   [`FIRST_CONTACT_UNDERSTANDING`]).
//! * **Learning** ([`WorldState::learn_tongue`]): the day's first chat with each resident,
//!   a trade a day per town, liked gifts, a lesson a day at the lexicon house, lexicon stones
//!   dug out from under the towns, primers and finished chapters. Crossing a [`Fluency`] level
//!   raises [`AlienEvent::FluencyRose`].
//! * **Commands** ([`AlienCommand`], [`WorldState::alien_command`]): the actor must reach the
//!   resident (the people engine's reach), and the resident must keep the right trade: the
//!   trader sells and buys, the lexicon keeper teaches, the elder gives chapters, the builder
//!   sells home plots.
//! * **Chapters**: explore and escort goals are checked each ground pass; fetch and build are
//!   handed over at an elder of the species.
//! * **Homes**: a plot bought is built in world time (a quarter of a day,
//!   [`HOME_BUILD_DAY_DIVISOR`], as contracted homes are), and becomes a [`crate::home::Home`] handed over in the world pass. An owner
//!   rests there (air topped up, hull patched), and the emergency beacon on that body tows the
//!   owner's rig to it instead of home.

use std::sync::Arc;

use crate::{
    TickOutput,
    alien::{
        AlienCommand, AlienEvent, AlienRefusal, AlienSpecies, AlienState, AlienTown,
        language::{
            FIRST_CONTACT_UNDERSTANDING, FULL_UNDERSTANDING, Fluency, LESSON_PRICE,
            LESSON_UNDERSTANDING, LEXICON_STONE_UNDERSTANDING, QUEST_UNDERSTANDING,
            TRADE_UNDERSTANDING, Understanding,
        },
        quest::{self, ChapterGoal},
        state::{ActiveChapter, AlienHome, TrailLead},
        town::{self, AlienBuildingKind},
        trade::{self, AlienGood},
    },
    entity::EntityId,
    event::GameplayEventKind,
    ids::PlayerId,
    people::{Character, CharacterId, VillageId},
    place::{SpaceBodyKey, StoreKey},
    summary::PlayerScopedCommandOutcome,
    terrain::TILE_SIZE,
};

use super::WorldState;

/// The share of a day (1 / this) a builder takes to raise a home in an alien town: six hours
/// of the world clock.
pub const HOME_BUILD_DAY_DIVISOR: u64 = 4;
/// Columns beyond a town's level span that still count as arriving in it.
pub const TOWN_ARRIVAL_MARGIN: i64 = 10;
/// Rows above or below a town's ground that still count as being in it.
pub const TOWN_ARRIVAL_ROWS: i32 = 16;
/// Columns either side of the far side's point that count as standing there.
pub const FAR_SIDE_COLUMNS: i64 = 30;
/// Rows above the surface that still count as standing on the far side.
pub const FAR_SIDE_ROWS: i32 = 12;
/// Columns beyond its town an explore-under goal still counts.
pub const UNDER_TOWN_MARGIN: i64 = 24;
/// Reach (columns, rows) round a home plot for resting.
pub const HOME_REACH: (i64, i32) = (4, 8);
/// Hull share a rest at home patches.
pub const REST_HULL_SHARE: f32 = 0.25;

/// What a home plot in a town of `species` costs (land and building).
#[must_use]
pub const fn home_price(species: AlienSpecies) -> u32 {
    match species {
        AlienSpecies::Vessi => 2_400,
        AlienSpecies::Tokkari => 2_000,
        AlienSpecies::Siphra => 2_800,
    }
}

/// The species of `character`, if one of the friendly aliens.
#[must_use]
pub const fn alien_species_of(character: &Character) -> Option<AlienSpecies> {
    AlienSpecies::of_culture(character.culture)
}

impl WorldState {
    /// What players did with the friendly alien species (Wave 12).
    #[must_use]
    pub const fn aliens(&self) -> &AlienState {
        &self.aliens
    }

    /// `player`'s understanding of `species`' tongue (per mille).
    #[must_use]
    pub fn understanding(&self, player: PlayerId, species: AlienSpecies) -> Understanding {
        self.aliens.understanding(player, species)
    }

    /// What `species` pays `player` for one `mineral` where the player is (`None` if they do
    /// not buy it): the home market's price, their taste, the body's economy and the player's
    /// understanding (what the trade panel shows and the sale pays).
    #[must_use]
    pub fn alien_ore_price(
        &self,
        player: PlayerId,
        species: AlienSpecies,
        mineral: crate::terrain::MineralKind,
    ) -> Option<u32> {
        let fluency = Fluency::of(self.understanding(player, species));
        let market = self.canonical_mineral_market_value(mineral);
        trade::ore_price(species, mineral, market, self.body_of(player), fluency)
    }

    /// What `good` of `species` costs `player` where the player is.
    #[must_use]
    pub fn alien_good_price(
        &self,
        player: PlayerId,
        species: AlienSpecies,
        good: AlienGood,
    ) -> u32 {
        let fluency = Fluency::of(self.understanding(player, species));
        trade::good_price(good, self.body_of(player), fluency)
    }

    /// The space body `player` is on (the default key off a body).
    fn body_of(&self, player: PlayerId) -> SpaceBodyKey {
        match self.store_of(player) {
            StoreKey::Body(key) => key,
            StoreKey::Home | StoreKey::Space => SpaceBodyKey::default(),
        }
    }

    /// Registers the towns of every body store this world holds, so their ids resolve
    /// ([`town::town_by_id`]): after a load, a keyframe or a replicated section.
    pub fn register_alien_towns(&self) {
        for (_, body) in self.places.stores() {
            if let Some(planet) = body.planet() {
                town::register(planet.params());
            }
        }
        if let Some(planet) = self.terrain.planet_ground() {
            town::register(planet.params());
        }
    }

    /// The towns of `store`'s body (none for the home store, free flight or an unsettled
    /// body).
    #[must_use]
    pub fn alien_towns_of_store(&self, store: StoreKey) -> Arc<Vec<Arc<AlienTown>>> {
        self.store_terrain(store)
            .and_then(crate::terrain::Terrain::planet_ground)
            .map_or_else(
                || Arc::new(Vec::new()),
                |planet| town::towns_of(planet.params()),
            )
    }

    /// The towns of the ground the running pass works on (the people pass on a body reads
    /// them; outside the tick, the home store's: none).
    #[must_use]
    pub fn alien_towns_here(&self) -> Arc<Vec<Arc<AlienTown>>> {
        self.terrain.planet_ground().map_or_else(
            || Arc::new(Vec::new()),
            |planet| town::towns_of(planet.params()),
        )
    }

    /// Whether the running pass's ground has alien towns.
    #[must_use]
    pub fn has_alien_towns_here(&self) -> bool {
        self.terrain
            .planet_ground()
            .is_some_and(|planet| planet.params().folk != 0)
    }

    /// The strip width of the running pass's ground (0 off a planet).
    fn strip_width_here(&self) -> i32 {
        self.terrain
            .planet_ground()
            .map_or(0, crate::planet::PlanetGround::strip_width)
    }

    /// The space body the running pass's ground is (a body store's key).
    const fn body_here(&self) -> Option<SpaceBodyKey> {
        match self.active_store() {
            StoreKey::Body(key) => Some(key),
            StoreKey::Home | StoreKey::Space => None,
        }
    }

    /// Local pixel x of strip column `column` on the running pass's ground: on a planet, the
    /// copy of the column nearest the frame's origin (a strip wraps; see `world/planet.rs`).
    #[must_use]
    pub fn strip_column_px(&self, column: i32) -> f32 {
        let frame = self.frame();
        let mut relative = frame.rel_col(column);
        let width = self.strip_width_here();
        if width > 0 {
            #[allow(
                clippy::cast_precision_loss,
                reason = "a lap of a few tens of thousands"
            )]
            let lap = width as f32;
            let half = lap / 2.0;
            relative = (relative + half).rem_euclid(lap) - half;
        }
        (relative + 0.5) * TILE_SIZE
    }

    /// `player`'s actor tile in its own store (the pilot on foot, else the rig).
    fn actor_tile_anywhere(&self, player: PlayerId) -> Option<crate::terrain::TilePosition> {
        let (x, y, _) = self.store_actor_position(player)?;
        let frame = self.store_frame(self.store_of(player))?;
        Some(frame.tile_at(x, y))
    }

    /// The town `player`'s actor is in (its level span and a margin), outside or inside the
    /// tick.
    #[must_use]
    pub fn alien_town_at(&self, player: PlayerId) -> Option<Arc<AlienTown>> {
        let store = self.store_of(player);
        let tile = self.actor_tile_anywhere(player)?;
        let towns = self.alien_towns_of_store(store);
        let width = self
            .store_terrain(store)
            .and_then(crate::terrain::Terrain::planet_ground)
            .map_or(0, crate::planet::PlanetGround::strip_width);
        towns
            .iter()
            .find(|found| {
                town::wrapped_distance(i64::from(tile.x), found.site.centre, width)
                    <= i64::from(found.site.half_width) + TOWN_ARRIVAL_MARGIN
                    && (tile.y - found.ground_row()).abs() <= TOWN_ARRIVAL_ROWS
            })
            .cloned()
    }

    /// The person behind `npc` if they are an alien resident `player` can reach: their town,
    /// character and resident index.
    fn alien_resident(
        &self,
        player: PlayerId,
        npc: EntityId,
    ) -> Option<(Arc<AlienTown>, Character, usize)> {
        if !self.can_reach_npc(player, npc) {
            return None;
        }
        let (_, state) = self.npc(npc)?;
        let found = town::town_by_id(state.character.village)?;
        let index = usize::from(state.character.index);
        let character = found.population.characters.get(index)?.clone();
        Some((found, character, index))
    }

    /// Adds `amount` understanding of `species` for `player` (capped), raising the learning
    /// and level events.
    pub(super) fn learn_tongue(
        &mut self,
        player: PlayerId,
        species: AlienSpecies,
        amount: Understanding,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let log = self.aliens.log_mut(player, species);
        let before = log.understanding;
        let after = before.saturating_add(amount).min(FULL_UNDERSTANDING);
        log.understanding = after;
        if after > before {
            events.push(GameplayEventKind::Alien {
                what: AlienEvent::Learned,
                player_id: player,
                species,
                detail: u32::from(after - before),
            });
        }
        let level = Fluency::of(after);
        if level > Fluency::of(before) {
            events.push(GameplayEventKind::Alien {
                what: AlienEvent::FluencyRose,
                player_id: player,
                species,
                detail: u32::from(level.index()),
            });
        }
    }

    /// First contact with `species` (once per player): the greeting learned.
    fn meet_species(
        &mut self,
        player: PlayerId,
        species: AlienSpecies,
        found: VillageId,
        events: &mut Vec<GameplayEventKind>,
    ) {
        if self.aliens.log(player, species).met() {
            return;
        }
        let now = self.world_millis();
        self.aliens.log_mut(player, species).met_millis = Some(now);
        events.push(GameplayEventKind::Alien {
            what: AlienEvent::FirstContact,
            player_id: player,
            species,
            detail: found.0,
        });
        self.learn_tongue(player, species, FIRST_CONTACT_UNDERSTANDING, events);
    }

    /// A chat with an alien resident (from `world/people.rs`): first contact if it is the
    /// first, and words learned once a day per resident.
    pub(super) fn alien_chatted(
        &mut self,
        player: PlayerId,
        character: &Character,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let Some(species) = alien_species_of(character) else {
            return;
        };
        self.meet_species(player, species, character.id.village, events);
        let day = self.people_clock().day;
        let log = self.aliens.log_mut(player, species);
        if log.chat_day != day {
            log.chat_day = day;
            log.chatted.clear();
        }
        if log.chatted.insert(character.id) {
            // A chat teaches a little more of a tongue already half known (the ear is tuned).
            let tuned = log.understanding / 40;
            let amount = crate::alien::language::CHAT_UNDERSTANDING + tuned.min(12);
            self.learn_tongue(player, species, amount, events);
        }
    }

    /// A gift to an alien resident (from `world/people.rs`): a liked one teaches words.
    pub(super) fn alien_gifted(
        &mut self,
        player: PlayerId,
        character: &Character,
        liked: bool,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let Some(species) = alien_species_of(character) else {
            return;
        };
        self.meet_species(player, species, character.id.village, events);
        if liked {
            self.learn_tongue(
                player,
                species,
                crate::alien::language::GIFT_UNDERSTANDING,
                events,
            );
        }
    }

    /// A lexicon stone dug out by `player` (from the drilling step).
    pub(super) fn lexicon_found(
        &mut self,
        player: PlayerId,
        species: AlienSpecies,
        output: &mut TickOutput,
    ) {
        let mut events = Vec::new();
        self.aliens.log_mut(player, species).stones =
            self.aliens.log(player, species).stones.saturating_add(1);
        events.push(GameplayEventKind::Alien {
            what: AlienEvent::LexiconFound,
            player_id: player,
            species,
            detail: 0,
        });
        self.learn_tongue(player, species, LEXICON_STONE_UNDERSTANDING, &mut events);
        for event in events {
            output.push_event(event);
        }
    }

    fn refuse(
        player: PlayerId,
        species: AlienSpecies,
        why: AlienRefusal,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        events.push(GameplayEventKind::Alien {
            what: AlienEvent::Refused,
            player_id: player,
            species,
            detail: why.code(),
        });
        PlayerScopedCommandOutcome::IgnoredUnavailable
    }

    /// Applies an alien command (`None` if `command` is not one).
    pub(super) fn alien_command(
        &mut self,
        player: PlayerId,
        command: &crate::command::PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        let crate::command::PlayerCommand::Alien(command) = command else {
            return None;
        };
        let mut events = Vec::new();
        let outcome = self.apply_alien_command(player, *command, &mut events);
        for event in events {
            output.push_event(event);
        }
        Some(outcome)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per command reads best together"
    )]
    fn apply_alien_command(
        &mut self,
        player: PlayerId,
        command: AlienCommand,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        match command {
            AlienCommand::AbandonChapter { species } => {
                let log = self.aliens.log_mut(player, species);
                if log.active.take().is_some() || log.offered.take().is_some() {
                    PlayerScopedCommandOutcome::Applied
                } else {
                    PlayerScopedCommandOutcome::IgnoredUnavailable
                }
            }
            AlienCommand::RestAtHome { town: id, plot } => {
                self.rest_at_home(player, id, plot, events)
            }
            AlienCommand::Sell {
                npc,
                mineral,
                count,
            } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::TRADER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let fluency = Fluency::of(self.understanding(player, species));
                let market = self.canonical_mineral_market_value(mineral);
                let Some(unit) = trade::ore_price(
                    species,
                    mineral,
                    market,
                    self.body_here().unwrap_or_default(),
                    fluency,
                ) else {
                    return Self::refuse(player, species, AlienRefusal::DoNotBuy, events);
                };
                let count = count.max(1);
                let bonus = self.sale_bonus_percent(player);
                let Some(rig) = self.players.get_mut(&player) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                if rig.cargo_bay.count(mineral) < count {
                    return Self::refuse(player, species, AlienRefusal::NotEnoughCargo, events);
                }
                rig.cargo_bay.remove(mineral, count);
                let payout = u64::from(unit) * u64::from(count) * u64::from(100 + bonus) / 100;
                let payout = u32::try_from(payout).unwrap_or(u32::MAX);
                rig.credits = rig.credits.saturating_add(payout);
                self.progression.total_earnings =
                    self.progression.total_earnings.saturating_add(payout);
                events.push(GameplayEventKind::CargoChanged { player_id: player });
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::Sold,
                    player_id: player,
                    species,
                    detail: payout,
                });
                self.traded_with(player, &found, events);
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::Buy { npc, good } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::TRADER || !species.def().goods.contains(&good) {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let fluency = Fluency::of(self.understanding(player, species));
                let price = trade::good_price(good, self.body_here().unwrap_or_default(), fluency);
                if !self.charge(player, price) {
                    return Self::refuse(player, species, AlienRefusal::NotEnoughCredits, events);
                }
                self.apply_good(player, good, self.body_here().unwrap_or_default(), events);
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::Bought,
                    player_id: player,
                    species,
                    detail: price,
                });
                self.traded_with(player, &found, events);
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::Lesson { npc } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::TEACHER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let day = self.people_clock().day;
                if self.aliens.log(player, species).lesson_day == Some(day) {
                    return Self::refuse(player, species, AlienRefusal::LessonToday, events);
                }
                if !self.charge(player, LESSON_PRICE) {
                    return Self::refuse(player, species, AlienRefusal::NotEnoughCredits, events);
                }
                self.meet_species(player, species, found.id, events);
                self.aliens.log_mut(player, species).lesson_day = Some(day);
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::Lesson,
                    player_id: player,
                    species,
                    detail: LESSON_PRICE,
                });
                self.learn_tongue(player, species, LESSON_UNDERSTANDING, events);
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::AskChapter { npc } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::ELDER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                self.meet_species(player, species, found.id, events);
                let log = self.aliens.log(player, species);
                if log.active.is_some() {
                    return Self::refuse(player, species, AlienRefusal::ChapterState, events);
                }
                let Some(def) = quest::chapter(species, log.chapters_done) else {
                    return Self::refuse(player, species, AlienRefusal::ArcFinished, events);
                };
                if log.understanding < def.needs {
                    return Self::refuse(player, species, AlienRefusal::NeedsUnderstanding, events);
                }
                self.aliens.log_mut(player, species).offered = Some(log.chapters_done);
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::ChapterOffered,
                    player_id: player,
                    species,
                    detail: u32::from(log.chapters_done),
                });
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::AcceptChapter { npc } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::ELDER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let log = self.aliens.log(player, species);
                let (Some(chapter), None) = (log.offered, log.active) else {
                    return Self::refuse(player, species, AlienRefusal::ChapterState, events);
                };
                let Some(def) = quest::chapter(species, chapter) else {
                    return Self::refuse(player, species, AlienRefusal::ArcFinished, events);
                };
                let mut active = ActiveChapter {
                    chapter,
                    town: found.id,
                    body: self.body_here().unwrap_or_default(),
                    ..ActiveChapter::default()
                };
                if def.goal == ChapterGoal::Escort {
                    let traveller = found
                        .population
                        .characters
                        .get(quest::TRAVELLER_SLOT)
                        .or_else(|| found.population.characters.last())
                        .map(|character| character.id);
                    active.passenger = traveller;
                    active.destination = next_town(&found).map(|next| next.id);
                }
                let log = self.aliens.log_mut(player, species);
                log.offered = None;
                log.active = Some(active);
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::ChapterAccepted,
                    player_id: player,
                    species,
                    detail: u32::from(chapter),
                });
                if active.passenger.is_some() {
                    events.push(GameplayEventKind::Alien {
                        what: AlienEvent::PassengerAboard,
                        player_id: player,
                        species,
                        detail: u32::from(chapter),
                    });
                }
                if def.goal == ChapterGoal::Trail {
                    // The builders' trail is told, not done: listening is the chapter.
                    self.complete_chapter(player, &found, events);
                }
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::TurnInChapter { npc } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::ELDER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let Some(active) = self.aliens.log(player, species).active else {
                    return Self::refuse(player, species, AlienRefusal::ChapterState, events);
                };
                let Some(def) = quest::chapter(species, active.chapter) else {
                    return Self::refuse(player, species, AlienRefusal::ArcFinished, events);
                };
                match def.goal {
                    ChapterGoal::Deliver { mineral, count } => {
                        let Some(rig) = self.players.get_mut(&player) else {
                            return PlayerScopedCommandOutcome::UnknownPlayer;
                        };
                        if rig.cargo_bay.count(mineral) < count {
                            return Self::refuse(
                                player,
                                species,
                                AlienRefusal::NotEnoughCargo,
                                events,
                            );
                        }
                        rig.cargo_bay.remove(mineral, count);
                        events.push(GameplayEventKind::CargoChanged { player_id: player });
                    }
                    ChapterGoal::Raise {
                        mineral,
                        count,
                        credits,
                    } => {
                        let Some(rig) = self.players.get(&player) else {
                            return PlayerScopedCommandOutcome::UnknownPlayer;
                        };
                        if rig.cargo_bay.count(mineral) < count {
                            return Self::refuse(
                                player,
                                species,
                                AlienRefusal::NotEnoughCargo,
                                events,
                            );
                        }
                        if rig.credits < credits {
                            return Self::refuse(
                                player,
                                species,
                                AlienRefusal::NotEnoughCredits,
                                events,
                            );
                        }
                        if let Some(rig) = self.players.get_mut(&player) {
                            rig.cargo_bay.remove(mineral, count);
                            rig.credits -= credits;
                        }
                        events.push(GameplayEventKind::CargoChanged { player_id: player });
                        // The monument stands in the giver's town, for everyone, from now on.
                        if self.aliens.monuments.insert(active.town) {
                            events.push(GameplayEventKind::Alien {
                                what: AlienEvent::MonumentRaised,
                                player_id: player,
                                species,
                                detail: active.town.0,
                            });
                        }
                    }
                    ChapterGoal::ReachDepth { .. }
                    | ChapterGoal::ReachFarSide
                    | ChapterGoal::Escort => {
                        if !active.reached {
                            return Self::refuse(player, species, AlienRefusal::NotDone, events);
                        }
                    }
                    ChapterGoal::Trail => {}
                }
                self.complete_chapter(player, &found, events);
                PlayerScopedCommandOutcome::Applied
            }
            AlienCommand::BuyHome { npc, plot } => {
                let Some((found, _, index)) = self.alien_resident(player, npc) else {
                    return Self::refuse(
                        player,
                        AlienSpecies::Vessi,
                        AlienRefusal::NotInReach,
                        events,
                    );
                };
                let species = found.species;
                if index != town::BUILDER {
                    return Self::refuse(player, species, AlienRefusal::NotTheirTrade, events);
                }
                let is_plot = found
                    .buildings
                    .get(usize::from(plot))
                    .is_some_and(|building| building.kind == AlienBuildingKind::HomePlot);
                if !is_plot || self.aliens.home_at(found.id, plot).is_some() {
                    return Self::refuse(player, species, AlienRefusal::PlotTaken, events);
                }
                let price = home_price(species);
                if !self.charge(player, price) {
                    return Self::refuse(player, species, AlienRefusal::NotEnoughCredits, events);
                }
                let now = self.world_millis();
                let builder = found
                    .keeper(town::BUILDER)
                    .map_or_else(CharacterId::default, |character| character.id);
                self.aliens.homes.push(AlienHome {
                    owner: player,
                    town: found.id,
                    body: self.body_here().unwrap_or_default(),
                    plot,
                    species,
                    builder,
                    paid: price,
                    bought_millis: now,
                    ready_millis: now + self.clock().day_millis() / HOME_BUILD_DAY_DIVISOR,
                    home: None,
                });
                self.add_goodwill(player, builder, 8);
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::HomeBought,
                    player_id: player,
                    species,
                    detail: price,
                });
                PlayerScopedCommandOutcome::Applied
            }
        }
    }

    /// A trade with `found`'s trader taught words (once a day per town).
    fn traded_with(
        &mut self,
        player: PlayerId,
        found: &AlienTown,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let species = found.species;
        self.meet_species(player, species, found.id, events);
        let day = self.people_clock().day;
        let log = self.aliens.log_mut(player, species);
        if log.traded_day.get(&found.id) != Some(&day) {
            log.traded_day.insert(found.id, day);
            self.learn_tongue(player, species, TRADE_UNDERSTANDING, events);
        }
        if let Some(trader) = found.keeper(town::TRADER) {
            self.add_goodwill(player, trader.id, 1);
        }
    }

    /// What `good` does for `player`.
    fn apply_good(
        &mut self,
        player: PlayerId,
        good: AlienGood,
        body: SpaceBodyKey,
        events: &mut Vec<GameplayEventKind>,
    ) {
        if let AlienGood::Primer(species) = good {
            self.learn_tongue(player, species, trade::PRIMER_UNDERSTANDING, events);
        }
        if let Some(piece) = good.furniture() {
            self.homes.add_furniture(player, piece, 1);
        }
        if let Some(kind) = good.blessing() {
            let until = crate::ids::SimulationTick::new(
                self.simulation_tick.get().saturating_add(
                    self.timing
                        .ticks_for_millis(crate::village::services::BLESSING_MILLIS),
                ),
            );
            if let Some(rig) = self.players.get_mut(&player) {
                rig.villages.blessing = Some(crate::village::Blessing { kind, until });
            }
        }
        match good {
            AlienGood::HullPlates => {
                if let Some(rig) = self.players.get_mut(&player) {
                    rig.hull = rig.max_hull();
                }
            }
            AlienGood::AirBloom => {
                let capacity = self.air_capacity_of(player);
                if let Some(rig) = self.players.get_mut(&player) {
                    rig.flight.air = capacity;
                }
            }
            AlienGood::StarMemory => {
                if let Some(system) = self.uncharted_system_near(body) {
                    self.chart_found_system(system);
                }
            }
            _ => {}
        }
    }

    /// Adds the generator's system `system` to the crew's found systems (the nav chart).
    fn chart_found_system(&mut self, system: crate::space::SystemRef) {
        let space = crate::space::Space::shared(self.home_settings().seed);
        let Some(found) = space.system(system) else {
            return;
        };
        let reference = crate::flight::GeneratedSpace::reference(system.sector, &found);
        if !self.flight.found_systems.contains(&reference)
            && self.flight.found_systems.len() < crate::flight::MAX_FOUND_SYSTEMS
        {
            self.flight.found_systems.push(reference);
        }
    }

    /// The nearest star system to `body` the crew has not charted.
    fn uncharted_system_near(&self, body: SpaceBodyKey) -> Option<crate::space::SystemRef> {
        let known = self.known_systems();
        let space = crate::space::Space::shared(self.home_settings().seed);
        let origin = body.sector;
        for ring in 0..=4_i32 {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dx.abs().max(dy.abs()) != ring {
                        continue;
                    }
                    let coords = crate::space::SpaceSector::new(origin.x + dx, origin.y + dy);
                    for system in &space.sector(coords).systems {
                        let reference = crate::space::SystemRef {
                            sector: coords,
                            system: system.index,
                        };
                        let flight_ref = crate::flight::GeneratedSpace::reference(coords, system);
                        if !system.home && !known.contains(&flight_ref) {
                            return Some(reference);
                        }
                    }
                }
            }
        }
        None
    }

    /// Finishes `player`'s chapter of `found`'s species: credits, words, goodwill and, at the
    /// arc's end, the builders' trail lead.
    fn complete_chapter(
        &mut self,
        player: PlayerId,
        found: &AlienTown,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let species = found.species;
        let log = self.aliens.log_mut(player, species);
        let Some(active) = log.active.take() else {
            return;
        };
        log.chapters_done = log.chapters_done.max(active.chapter + 1);
        let Some(def) = quest::chapter(species, active.chapter) else {
            return;
        };
        if let Some(rig) = self.players.get_mut(&player) {
            rig.credits = rig.credits.saturating_add(def.credits);
        }
        if let Some(elder) = found.keeper(town::ELDER) {
            self.add_goodwill(player, elder.id, 10);
        }
        events.push(GameplayEventKind::Alien {
            what: AlienEvent::ChapterCompleted,
            player_id: player,
            species,
            detail: u32::from(active.chapter),
        });
        self.learn_tongue(player, species, QUEST_UNDERSTANDING, events);
        if def.goal == ChapterGoal::Trail {
            let system = trail_system(self.home_settings().seed, species);
            self.chart_found_system(system);
            let now = self.world_millis();
            self.aliens.trail_leads.push(TrailLead {
                species,
                system,
                player,
                told_millis: now,
            });
            events.push(GameplayEventKind::Alien {
                what: AlienEvent::TrailLead,
                player_id: player,
                species,
                detail: u32::try_from(self.aliens.trail_leads.len() - 1).unwrap_or(u32::MAX),
            });
        }
    }

    fn rest_at_home(
        &mut self,
        player: PlayerId,
        id: VillageId,
        plot: u8,
        events: &mut Vec<GameplayEventKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some(found) = town::town_by_id(id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let species = found.species;
        let Some(home) = self
            .aliens
            .home_at(id, plot)
            .filter(|home| home.owner == player)
        else {
            return Self::refuse(player, species, AlienRefusal::NoHome, events);
        };
        if home.home.is_none() {
            return Self::refuse(player, species, AlienRefusal::StillBuilding, events);
        }
        let Some(building) = found.buildings.get(usize::from(plot)).copied() else {
            return Self::refuse(player, species, AlienRefusal::NoHome, events);
        };
        let at_home = self.store_of(player) == StoreKey::Body(self.body_here().unwrap_or_default())
            && self.actor_tile_anywhere(player).is_some_and(|tile| {
                town::wrapped_distance(
                    i64::from(tile.x),
                    building.door_column(),
                    found.strip_width(),
                ) <= i64::from(building.width / 2) + HOME_REACH.0
                    && (tile.y - found.ground_row()).abs() <= HOME_REACH.1
            });
        if !at_home {
            return Self::refuse(player, species, AlienRefusal::NotInReach, events);
        }
        let capacity = self.air_capacity_of(player);
        if let Some(rig) = self.players.get_mut(&player) {
            rig.flight.air = capacity;
            let max = rig.max_hull();
            rig.hull = max.mul_add(REST_HULL_SHARE, rig.hull).min(max);
        }
        events.push(GameplayEventKind::Alien {
            what: AlienEvent::Rested,
            player_id: player,
            species,
            detail: u32::from(plot),
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// Where the beacon tows `player`'s rig on `body` instead of home: the door of a finished
    /// home of theirs there (local pixels of the running pass's ground).
    #[must_use]
    pub fn alien_home_rescue_point(
        &self,
        player: PlayerId,
        body: SpaceBodyKey,
    ) -> Option<(f32, f32)> {
        let home = self
            .aliens
            .homes_of(player)
            .find(|home| home.body == body && home.home.is_some())?;
        let found = town::town_by_id(home.town)?;
        let building = found.buildings.get(usize::from(home.plot))?;
        let x = self.strip_column_px(building.door_column());
        let y = self.frame().rel_row(found.ground_row() - 2) * TILE_SIZE;
        Some((x, y))
    }

    /// The aliens' part of a body's ground pass (Wave 12): first contacts as players walk into
    /// towns, and chapters' explore and escort goals.
    pub(super) fn advance_aliens_on_body(&mut self, output: &mut TickOutput) {
        let Some(body) = self.body_here() else {
            return;
        };
        let width = self.strip_width_here();
        let towns = self.alien_towns_here();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        let mut events = Vec::new();
        for player in players {
            let Some((x, y)) = self.actor_position(player) else {
                continue;
            };
            let tile = self.frame().tile_at(x, y);
            let column = i64::from(tile.x);
            for found in towns.iter() {
                if town::wrapped_distance(column, found.site.centre, width)
                    <= i64::from(found.site.half_width) + TOWN_ARRIVAL_MARGIN
                    && (tile.y - found.ground_row()).abs() <= TOWN_ARRIVAL_ROWS
                {
                    self.meet_species(player, found.species, found.id, &mut events);
                }
            }
            self.advance_chapter_goals(player, body, tile, &towns, width, &mut events);
        }
        for event in events {
            output.push_event(event);
        }
    }

    fn advance_chapter_goals(
        &mut self,
        player: PlayerId,
        body: SpaceBodyKey,
        tile: crate::terrain::TilePosition,
        towns: &[Arc<AlienTown>],
        width: i32,
        events: &mut Vec<GameplayEventKind>,
    ) {
        let column = i64::from(tile.x);
        let surface = self.terrain.planet_ground().map_or(0, |planet| {
            crate::planet::BodyGround::surface_row(planet, tile.x)
        });
        for species in AlienSpecies::ALL {
            let Some(active) = self.aliens.log(player, species).active else {
                continue;
            };
            if active.body != body || active.reached {
                continue;
            }
            let Some(def) = quest::chapter(species, active.chapter) else {
                continue;
            };
            let Some(giver) = towns.iter().find(|found| found.id == active.town) else {
                continue;
            };
            let far_side = (giver.site.centre + width / 2).rem_euclid(width.max(1));
            let on_far_side = town::wrapped_distance(column, far_side, width) <= FAR_SIDE_COLUMNS
                && tile.y <= surface + 1
                && tile.y >= surface - FAR_SIDE_ROWS;
            let reached = match def.goal {
                ChapterGoal::ReachDepth { rows } => {
                    town::wrapped_distance(column, giver.site.centre, width)
                        <= i64::from(giver.site.half_width) + UNDER_TOWN_MARGIN
                        && tile.y >= giver.ground_row() + rows
                }
                ChapterGoal::ReachFarSide => on_far_side,
                ChapterGoal::Escort => active.destination.map_or(on_far_side, |destination| {
                    towns.iter().any(|found| {
                        found.id == destination
                            && town::wrapped_distance(column, found.site.centre, width)
                                <= i64::from(found.site.half_width)
                            && (tile.y - found.ground_row()).abs() <= TOWN_ARRIVAL_ROWS
                    })
                }),
                ChapterGoal::Deliver { .. } | ChapterGoal::Raise { .. } | ChapterGoal::Trail => {
                    false
                }
            };
            if !reached {
                continue;
            }
            let log = self.aliens.log_mut(player, species);
            if let Some(active) = log.active.as_mut() {
                active.reached = true;
                let delivered = active.passenger.take().is_some();
                events.push(GameplayEventKind::Alien {
                    what: AlienEvent::ChapterReached,
                    player_id: player,
                    species,
                    detail: u32::from(active.chapter),
                });
                if delivered {
                    events.push(GameplayEventKind::Alien {
                        what: AlienEvent::PassengerDelivered,
                        player_id: player,
                        species,
                        detail: u32::from(active.chapter),
                    });
                }
            }
        }
    }

    /// The aliens' part of the world pass: homes handed over when their builders finish (world
    /// time, so sleeping moves them on and every tick rate agrees).
    pub(super) fn advance_alien_homes(
        &mut self,
        next_tick: crate::ids::SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.aliens.homes.is_empty() {
            return;
        }
        let now = self.clock_at(next_tick).world_millis();
        let mut finished = Vec::new();
        for home in &mut self.aliens.homes {
            if home.home.is_none() && now >= home.ready_millis {
                home.home = Some(crate::home::Home::built(
                    crate::home::blueprint::Blueprint::Cottage,
                    3,
                    Some(home.builder),
                    now,
                ));
                finished.push((home.owner, home.species, home.plot, home.builder));
            }
        }
        for (owner, species, plot, builder) in finished {
            self.add_goodwill(owner, builder, crate::home::HANDOVER_GOODWILL);
            output.push_event(GameplayEventKind::Alien {
                what: AlienEvent::HomeBuilt,
                player_id: owner,
                species,
                detail: u32::from(plot),
            });
        }
    }
}

/// The next town of `found`'s body (the escort's destination), if the body has another.
fn next_town(found: &AlienTown) -> Option<Arc<AlienTown>> {
    let towns = town::towns_of(&found.params);
    if towns.len() < 2 {
        return None;
    }
    let index = usize::from(found.site.index);
    towns.get((index + 1) % towns.len()).cloned()
}

/// The system the builders' trail lead of `species` names in the world of `seed`: the first
/// system of the sector its direction points at, a long warp out.
#[must_use]
pub fn trail_system(seed: u64, species: AlienSpecies) -> crate::space::SystemRef {
    let (dx, dy) = quest::trail_offset(species);
    let jitter = |salt: u64| -> i32 {
        let roll = crate::people::PeopleRng::new(seed, 0x5452_4149_4C00, salt, 0).next_u64();
        i32::try_from(roll % 3).unwrap_or(0) - 1
    };
    let coords = crate::space::SpaceSector::new(
        crate::space::HOME_SECTOR.x + dx + jitter(u64::from(species.index()) * 2),
        crate::space::HOME_SECTOR.y + dy + jitter(u64::from(species.index()) * 2 + 1),
    );
    let space = crate::space::Space::shared(seed);
    let sector = space.sector(coords);
    let system = sector
        .systems
        .iter()
        .find(|system| system.planet_count() > 0)
        .or_else(|| sector.systems.first())
        .map_or(0, |system| system.index);
    crate::space::SystemRef {
        sector: coords,
        system,
    }
}
