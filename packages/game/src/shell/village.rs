//! Villages in the shell: walking into a village building, its counter (the building's deals,
//! bought as [`SessionServiceRequest::Village`]), talking to people, and the arrival banner.
//!
//! People are the simulation's NPC entities (WP-PEOPLE). Talking, trading and giving gifts are
//! [`PeopleCommand`]s the seat sends for its player, naming the NPC entity: on the street (the
//! nearest person within reach, see [`talk_target`]), in a building's room (whoever is inside),
//! or at a counter (its keeper, when they are in). The conversation itself lives in the world's
//! people ledger, so every peer shows the same lines and the person remembers the player across
//! sessions; the seat only keeps which person it is talking to ([`TalkView`]).

use std::sync::Arc;

use fatherload_simulation::{
    PlayerCommand, PlayerId, SessionServiceRequest, WorldState,
    entity::{EntityId, EntityStore, NpcState},
    people::{
        CharacterId, PeopleCommand, TalkTopic,
        contractor::contractor_profile,
        offers::{self, giftable},
        routine,
    },
    village::{
        self, BuildingKind, Village, VillageDeal, VillageId, VillageService, VillageSetting,
    },
};

use super::{ModalScreen, SeatContext, SeatUi, ServiceAnimation, ShellRequest, SoundCue};
use crate::input::{PlayerInput, prompts::Prompt};

/// Room units (0..640) of a village counter's centre.
pub const VILLAGE_COUNTER_X: f32 = 440.0;
/// How close (room units) a rig must be to the counter, or a resident, to use them.
pub const VILLAGE_REACH: f32 = 60.0;
/// Room units left of which `E` uses the door.
const DOOR_REACH: f32 = 74.0;
/// How long the arrival banner stays up, in shell ticks (60 per second).
pub const BANNER_TICKS: u64 = 300;
/// How close (tiles) a person must be for the talk key to prefer them over a building door the
/// rig also stands at.
const TALK_OVER_DOOR_TILES: f32 = 1.5;

/// The village building a seat is inside.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VillageInterior {
    pub village: VillageId,
    pub building: u8,
}

/// An open conversation: who the seat is talking to. What was said lives in the world's people
/// ledger ([`WorldState::people`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TalkView {
    /// The NPC entity.
    pub npc: EntityId,
    pub speaker: CharacterId,
    pub name: String,
    /// "Trader, 34" style line.
    pub about: String,
    /// Opened from the counter menu (Back returns to it).
    pub from_counter: bool,
}

/// What a seat can say or do in a conversation (the talk panel's rows, in order).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TalkChoice {
    /// A story row (WP-STORY): ask for work, take a story on, or tell how one ended.
    Story(super::StoryChoice),
    Topic(TalkTopic),
    /// Open their trade list.
    Trade,
    /// Open the gift list (the cargo bay's ore, relics and materials).
    Gift,
    Goodbye,
}

impl TalkChoice {
    /// The row label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Story(story) => story.label(),
            Self::Topic(TalkTopic::Chat) => "Chat",
            Self::Topic(TalkTopic::Rumours) => "Ask about rumours",
            Self::Topic(TalkTopic::Life) => "Ask about their life",
            Self::Topic(TalkTopic::BuildingWork) => "Ask about building work",
            Self::Trade => "What do you have for me?",
            Self::Gift => "Give a gift",
            Self::Goodbye => "Goodbye",
        }
    }
}

/// The talk panel's rows for `speaker`: their story rows first (`story`: what they have for the
/// player, see [`WorldState::arc_talk`]), then the topics (a contractor also takes building
/// questions), trade, gifts and goodbye.
#[must_use]
pub fn talk_choices(
    speaker: Option<&fatherload_simulation::people::Character>,
    story: &fatherload_simulation::world::ArcTalk,
) -> Vec<TalkChoice> {
    let mut choices: Vec<TalkChoice> = super::story_choices(story)
        .into_iter()
        .map(TalkChoice::Story)
        .collect();
    choices.extend([
        TalkChoice::Topic(TalkTopic::Chat),
        TalkChoice::Topic(TalkTopic::Rumours),
        TalkChoice::Topic(TalkTopic::Life),
    ]);
    if speaker.is_some_and(|character| contractor_profile(character).is_some()) {
        choices.push(TalkChoice::Topic(TalkTopic::BuildingWork));
    }
    choices.extend([TalkChoice::Trade, TalkChoice::Gift, TalkChoice::Goodbye]);
    choices
}

/// The cargo a player can hand over as gifts, in bay order.
#[must_use]
pub fn gift_items(
    player: &fatherload_simulation::Player,
) -> Vec<(fatherload_simulation::item::ItemKind, u32)> {
    player
        .cargo_bay
        .iter()
        .filter(|(kind, count)| giftable(*kind) && *count > 0)
        .collect()
}

/// The person `player`'s rig would talk to with the talk key: the nearest person within reach,
/// unless the rig stands at a building door and they are more than
/// [`TALK_OVER_DOOR_TILES`] away (then the key enters the building).
#[must_use]
pub fn talk_target(world: &WorldState, player: PlayerId) -> Option<EntityId> {
    let npc = *world.npcs_in_reach(player).first()?;
    if world.player_village_building(player).is_some() {
        let (x, _) = world.people_actor_position(player)?;
        let (npc_x, _) = world.npc_position(npc)?;
        if (npc_x - x).abs() > TALK_OVER_DOOR_TILES * fatherload_simulation::TILE_SIZE {
            return None;
        }
    }
    Some(npc)
}

/// The people inside lot `lot` of `village` (the keeper excluded when they are behind the
/// counter), in resident order.
#[must_use]
pub fn people_inside(
    entities: &EntityStore,
    village: VillageId,
    lot: usize,
    keeper: Option<u16>,
) -> Vec<(EntityId, NpcState)> {
    let mut found: Vec<(EntityId, NpcState)> = entities
        .of_kind(fatherload_simulation::entity::EntityKind::Npc)
        .filter_map(|entity| match entity.state {
            fatherload_simulation::entity::EntityState::Npc(state)
                if state.character.village == village
                    && state.inside.map(usize::from) == Some(lot)
                    && Some(state.character.index) != keeper =>
            {
                Some((entity.id, state))
            }
            _ => None,
        })
        .collect();
    found.sort_by_key(|(_, state)| state.character.index);
    found
}

/// The keeper of `lot`, if they are in (behind their counter).
#[must_use]
pub fn keeper_inside(entities: &EntityStore, found: &Village, lot: usize) -> Option<EntityId> {
    let keeper = found.buildings.get(lot)?.keeper?;
    entities
        .of_kind(fatherload_simulation::entity::EntityKind::Npc)
        .find(|entity| {
            matches!(entity.state, fatherload_simulation::entity::EntityState::Npc(state)
                if state.character.village == found.id
                    && state.character.index == keeper
                    && state.inside.map(usize::from) == Some(lot))
        })
        .map(|entity| entity.id)
}

/// The arrival banner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VillageBanner {
    pub village: VillageId,
    pub title: String,
    pub subtitle: String,
    /// The rig's first visit ("Discovered") rather than a return ("Welcome back").
    pub first_visit: bool,
    /// `ShellState::update_ticks` when it went up.
    pub tick: u64,
}

impl VillageBanner {
    /// Whether the banner is still showing at `now`.
    #[must_use]
    pub const fn showing(&self, now: u64) -> bool {
        now.saturating_sub(self.tick) < BANNER_TICKS
    }
}

/// A seat's village state: where it is inside, its open conversation and its arrival banner.
#[derive(Clone, Debug, Default)]
pub struct SeatVillage {
    pub interior: Option<VillageInterior>,
    pub talk: Option<TalkView>,
    pub banner: Option<VillageBanner>,
    /// The village the rig was in on the previous frame.
    last_village: Option<VillageId>,
    /// Frames the world keeps ticking for a people command this seat just sent (a single-player
    /// world otherwise rests while every seat is indoors).
    pending_frames: u8,
}

impl SeatVillage {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            interior: None,
            talk: None,
            banner: None,
            last_village: None,
            pending_frames: 0,
        }
    }

    /// Whether a people command from this seat may still be waiting for its tick.
    #[must_use]
    pub const fn people_pending(&self) -> bool {
        self.pending_frames > 0
    }

    /// One frame passed.
    pub const fn tick_pending(&mut self) {
        self.pending_frames = self.pending_frames.saturating_sub(1);
    }
}

/// Frames the world keeps ticking after a people command (a few ticks of round trip).
const PEOPLE_PENDING_FRAMES: u8 = 20;

/// One person standing in a building's room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomPerson {
    pub npc: EntityId,
    /// Their resident index in the village.
    pub resident: u16,
    /// Where they stand (room units, 0..640).
    pub x: f32,
}

/// Where the people inside `building` stand in its room (room units). The keeper stands behind
/// the counter and is reached through the counter menu; everyone else gets a spot along the
/// floor (an inn on a busy evening fills up).
#[must_use]
pub fn room_residents(entities: &EntityStore, found: &Village, building: usize) -> Vec<RoomPerson> {
    let Some(lot) = found.buildings.get(building) else {
        return Vec::new();
    };
    let spots: &[f32] = if lot.keeper.is_some() {
        &[170.0, 230.0, 290.0, 345.0, 540.0, 595.0, 140.0, 520.0]
    } else {
        &[190.0, 260.0, 330.0, 400.0, 470.0, 540.0, 150.0, 595.0]
    };
    people_inside(entities, found.id, building, lot.keeper)
        .into_iter()
        .zip(spots)
        .map(|((npc, state), x)| RoomPerson {
            npc,
            resident: state.character.index,
            x: *x,
        })
        .collect()
}

/// The village and building a seat is inside, if it still exists in this world.
#[must_use]
pub fn interior_village(
    settings: &fatherload_simulation::worldgen::WorldSettings,
    interior: VillageInterior,
) -> Option<(Arc<Village>, usize)> {
    let found = village::village(settings, interior.village)?;
    let building = usize::from(interior.building);
    (building < found.buildings.len()).then_some((found, building))
}

/// [`interior_village`] as stories left it (its prosperity, which prices the counter and fills
/// the meeting hall's stars; [`fatherload_simulation::people::PeopleLedger::village_now`]).
#[must_use]
pub fn interior_village_now(
    settings: &fatherload_simulation::worldgen::WorldSettings,
    people: &fatherload_simulation::people::PeopleLedger,
    interior: VillageInterior,
) -> Option<(Arc<Village>, usize)> {
    interior_village(settings, interior)
        .map(|(found, building)| (people.village_now(found), building))
}

/// The counter rows of `building` at `time` (WP-NIGHT: the night market's stock after dark,
/// none at a shut counter): its deals, then a row to talk to the keeper, then (in a meeting
/// hall) the land office (WP-HOME).
#[must_use]
pub fn counter_rows(
    found: &Village,
    building: usize,
    time: fatherload_simulation::village::services::MarketTime,
) -> usize {
    found.offers_at(building, time).len()
        + usize::from(
            found
                .buildings
                .get(building)
                .is_some_and(|lot| lot.keeper.is_some()),
        )
        + usize::from(
            found
                .buildings
                .get(building)
                .is_some_and(|lot| lot.kind == BuildingKind::Hall),
        )
}

/// The name plate of a village building ("Trading Post · Copperhollow").
#[must_use]
pub fn building_title(found: &Village, building: usize) -> String {
    found.buildings.get(building).map_or_else(
        || found.name.clone(),
        |lot| {
            if lot.kind == BuildingKind::House {
                let family = lot
                    .residents
                    .first()
                    .and_then(|index| found.population.characters.get(usize::from(*index)))
                    .map_or("A", |character| character.name.family.as_str());
                format!("The {family} home, {}", found.name)
            } else {
                format!("{}, {}", lot.kind.label(), found.name)
            }
        },
    )
}

impl SeatUi {
    /// Shows the arrival banner when the seat's rig enters a village (the first visit is
    /// announced by [`super::ShellState::village_discovered`]).
    pub(super) fn track_village(&mut self, ctx: SeatContext<'_>, tick: u64) {
        let current = ctx
            .player
            .and_then(|player| ctx.world.player_village(player));
        let id = current.as_ref().map(|found| found.id);
        if id != self.village.last_village
            && let Some(found) = &current
        {
            let first_visit = self
                .village
                .banner
                .as_ref()
                .is_some_and(|banner| banner.village == found.id && banner.first_visit);
            self.village.banner = Some(banner_for(found, first_visit, tick));
        }
        self.village.last_village = id;
    }

    /// Walks into the village building at the rig's door, if there is one.
    pub(super) fn try_enter_village(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        let Some((found, building)) = ctx
            .player
            .and_then(|player| ctx.world.player_village_building(player))
        else {
            return false;
        };
        self.village.interior = Some(VillageInterior {
            village: found.id,
            building: u8::try_from(building).unwrap_or(u8::MAX),
        });
        self.village.talk = None;
        self.interior_x = 82.0;
        self.interior_facing = 1.0;
        self.modal = None;
        self.selected_menu_item = 0;
        cues.push(SoundCue::Ui);
        let lot = &found.buildings[building];
        self.message = if lot.keeper.is_some() {
            format!(
                "Entered the {}. Walk to the counter and press {}; {} leaves.",
                lot.kind.label(),
                super::key_for(Prompt::Use, ctx.device),
                super::key_for(Prompt::Back, ctx.device),
            )
        } else {
            format!(
                "{} Walk up to someone and press {} to talk.",
                building_title(&found, building),
                super::key_for(Prompt::Use, ctx.device),
            )
        };
        true
    }

    /// One frame inside a village building (walking, the door, the counter and residents).
    pub(super) fn handle_village_interior(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        if input.pause {
            return true;
        }
        let Some((found, building)) = self.village.interior.and_then(|interior| {
            interior_village_now(ctx.world.world_settings(), ctx.world.people(), interior)
        }) else {
            self.village.interior = None;
            return false;
        };
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            self.interior_facing = movement.signum();
        }
        let at_door = self.interior_x <= 42.5 && movement < -0.5;
        self.interior_stride = if movement.abs() > 0.2 {
            self.interior_stride + delta_seconds
        } else {
            0.0
        };
        self.interior_x = movement
            .mul_add(185.0 * delta_seconds, self.interior_x)
            .clamp(42.0, 598.0);
        // A pilot walks out through the door (Part II F5); a rig uses it (or Back).
        let walked_out = at_door && ctx.on_foot();
        if input.backs_out() || walked_out || (input.interact && self.interior_x < DOOR_REACH) {
            self.door_latch = true;
            self.village.interior = None;
            self.village.talk = None;
            self.modal = None;
            cues.push(SoundCue::Ui);
            "Back outside.".clone_into(&mut self.message);
            return false;
        }
        if !input.interact {
            return false;
        }
        let lot = &found.buildings[building];
        if lot.keeper.is_some() && (self.interior_x - VILLAGE_COUNTER_X).abs() <= VILLAGE_REACH {
            self.selected_menu_item = 0;
            self.modal = Some(ModalScreen::VillageCounter);
            cues.push(SoundCue::Ui);
            return false;
        }
        let near = room_residents(ctx.world.entities(), &found, building)
            .into_iter()
            .find(|person| (self.interior_x - person.x).abs() <= VILLAGE_REACH * 0.7);
        if let Some(person) = near {
            self.start_talk(ctx, person.npc, false, requests);
            cues.push(SoundCue::Ui);
        } else if lot.keeper.is_some() {
            "Walk to the counter, someone to talk to, or the door.".clone_into(&mut self.message);
        } else {
            "Walk up to someone to talk, or back to the door.".clone_into(&mut self.message);
        }
        false
    }

    /// Opens a conversation with the person `npc` (on the street, in a room or at a counter):
    /// sends the opening chat for the seat's player and shows the talk panel.
    pub(super) fn start_talk(
        &mut self,
        ctx: SeatContext<'_>,
        npc: EntityId,
        from_counter: bool,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some((_, state)) = ctx.world.npc(npc) else {
            return;
        };
        let Some(character) = routine::character(ctx.world.world_settings(), state.character)
        else {
            return;
        };
        let role = character.role.name();
        let mut about = String::with_capacity(role.len() + 8);
        about.push_str(&role[..1].to_uppercase());
        about.push_str(&role[1..]);
        about.push_str(", ");
        about.push_str(&character.age.to_string());
        if contractor_profile(&character).is_some() {
            about.push_str(" - takes building work");
        }
        self.village.talk = Some(TalkView {
            npc,
            speaker: character.id,
            name: character.name.full(),
            about,
            from_counter,
        });
        self.selected_menu_item = 0;
        self.modal = Some(ModalScreen::VillageTalk);
        self.send_people(
            ctx,
            PeopleCommand::Talk {
                npc,
                topic: TalkTopic::Chat,
            },
            requests,
        );
    }

    /// Sends a people command for the seat's player.
    fn send_people(
        &mut self,
        ctx: SeatContext<'_>,
        command: PeopleCommand,
        requests: &mut Vec<ShellRequest>,
    ) {
        if ctx.player.is_some() {
            self.village.pending_frames = PEOPLE_PENDING_FRAMES;
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command: PlayerCommand::People(command),
            });
        }
    }

    /// Talks to the person the seat's rig is near on the street, if any. Returns whether a
    /// conversation opened.
    pub(super) fn try_talk_outdoors(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(npc) = ctx.player.and_then(|player| talk_target(ctx.world, player)) else {
            return false;
        };
        self.start_talk(ctx, npc, false, requests);
        cues.push(SoundCue::Ui);
        true
    }

    /// Ends the seat's conversation: tells the world, then back to the counter when the talk
    /// started there.
    fn end_talk(&mut self, ctx: SeatContext<'_>, requests: &mut Vec<ShellRequest>) {
        let from_counter = self
            .village
            .talk
            .take()
            .is_some_and(|talk| talk.from_counter);
        self.send_people(ctx, PeopleCommand::Goodbye, requests);
        self.selected_menu_item = 0;
        self.modal = from_counter.then_some(ModalScreen::VillageCounter);
    }

    /// The talk panel, the trade list and the gift list.
    #[allow(
        clippy::too_many_lines,
        reason = "the three people panels share their guards and read best together"
    )]
    fn handle_people_modal(
        &mut self,
        modal: ModalScreen,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(talk) = self.village.talk.clone() else {
            self.modal = None;
            return;
        };
        let speaker = routine::character(ctx.world.world_settings(), talk.speaker);
        // The person walked off (despawned) or the world forgot them: close quietly.
        if ctx.world.npc(talk.npc).is_none() {
            self.village.talk = None;
            self.modal = None;
            return;
        }
        match modal {
            ModalScreen::VillageTalk => {
                if input.backs_out() {
                    self.end_talk(ctx, requests);
                    cues.push(SoundCue::UiBack);
                    return;
                }
                let story = ctx
                    .player
                    .map_or(fatherload_simulation::world::ArcTalk::Nothing, |player| {
                        ctx.world.arc_talk(player, talk.npc)
                    });
                let choices = talk_choices(speaker.as_ref(), &story);
                self.move_selection(input, choices.len().saturating_sub(1));
                if !(input.confirm || input.interact) {
                    return;
                }
                match choices.get(self.selected_menu_item).copied() {
                    Some(TalkChoice::Story(choice)) => {
                        let command = match choice {
                            super::StoryChoice::Ask { .. } => {
                                PeopleCommand::AskWork { npc: talk.npc }
                            }
                            super::StoryChoice::Accept => {
                                PeopleCommand::AcceptArc { npc: talk.npc }
                            }
                            super::StoryChoice::Ending { outcome, .. } => {
                                PeopleCommand::TurnInArc {
                                    npc: talk.npc,
                                    outcome,
                                }
                            }
                        };
                        self.send_people(ctx, command, requests);
                        // The rows change with the answer: back to the top.
                        self.selected_menu_item = 0;
                        cues.push(SoundCue::Ui);
                    }
                    Some(TalkChoice::Topic(topic)) => {
                        self.send_people(
                            ctx,
                            PeopleCommand::Talk {
                                npc: talk.npc,
                                topic,
                            },
                            requests,
                        );
                        cues.push(SoundCue::Ui);
                    }
                    Some(TalkChoice::Trade) => {
                        self.selected_menu_item = 0;
                        self.modal = Some(ModalScreen::PersonTrade);
                        cues.push(SoundCue::Ui);
                    }
                    Some(TalkChoice::Gift) => {
                        let any = ctx
                            .player()
                            .is_some_and(|player| !gift_items(player).is_empty());
                        if any {
                            self.selected_menu_item = 0;
                            self.modal = Some(ModalScreen::PersonGift);
                            cues.push(SoundCue::Ui);
                        } else {
                            "Nothing in the bay to give: bring ore or a relic."
                                .clone_into(&mut self.message);
                            cues.push(SoundCue::UiError);
                        }
                    }
                    Some(TalkChoice::Goodbye) | None => {
                        self.end_talk(ctx, requests);
                        cues.push(SoundCue::UiBack);
                    }
                }
            }
            ModalScreen::PersonTrade => {
                if input.backs_out() {
                    self.selected_menu_item = 0;
                    self.modal = Some(ModalScreen::VillageTalk);
                    return;
                }
                let list = speaker.as_ref().map_or_else(Vec::new, offers::offers_of);
                self.move_selection(input, list.len().saturating_sub(1));
                if (input.confirm || input.interact) && self.selected_menu_item < list.len() {
                    self.send_people(
                        ctx,
                        PeopleCommand::Trade {
                            npc: talk.npc,
                            offer: u8::try_from(self.selected_menu_item).unwrap_or(u8::MAX),
                        },
                        requests,
                    );
                    // Back to the conversation to hear how it went.
                    self.selected_menu_item = 0;
                    self.modal = Some(ModalScreen::VillageTalk);
                    cues.push(SoundCue::Ui);
                }
            }
            ModalScreen::PersonGift => {
                if input.backs_out() {
                    self.selected_menu_item = 0;
                    self.modal = Some(ModalScreen::VillageTalk);
                    return;
                }
                let items = ctx.player().map_or_else(Vec::new, gift_items);
                self.move_selection(input, items.len().saturating_sub(1));
                if (input.confirm || input.interact)
                    && let Some((item, _)) = items.get(self.selected_menu_item).copied()
                {
                    self.send_people(
                        ctx,
                        PeopleCommand::Gift {
                            npc: talk.npc,
                            item,
                        },
                        requests,
                    );
                    self.selected_menu_item = 0;
                    self.modal = Some(ModalScreen::VillageTalk);
                    cues.push(SoundCue::Ui);
                }
            }
            _ => {}
        }
    }

    /// The village counter and conversation panels. Returns `true` while one is open.
    pub(super) fn handle_village_modal(
        &mut self,
        modal: ModalScreen,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if matches!(
            modal,
            ModalScreen::VillageTalk | ModalScreen::PersonTrade | ModalScreen::PersonGift
        ) {
            self.handle_people_modal(modal, input, ctx, cues, requests);
            return;
        }
        let interior = self.village.interior.and_then(|interior| {
            interior_village_now(ctx.world.world_settings(), ctx.world.people(), interior)
        });
        let Some((found, building)) = interior else {
            self.modal = None;
            return;
        };
        if input.backs_out() {
            self.modal = None;
            return;
        }
        let time = ctx.world.market_time();
        let rows = counter_rows(&found, building, time);
        self.move_selection(input, rows.saturating_sub(1));
        if !(input.confirm || input.interact) {
            return;
        }
        let offers = found.offers_at(building, time);
        let lot = &found.buildings[building];
        let land_office = offers.len() + usize::from(lot.keeper.is_some());
        if lot.kind == BuildingKind::Hall && self.selected_menu_item == land_office {
            self.open_home(super::homes::HomeScreen::LandOffice, None);
            cues.push(SoundCue::Ui);
            return;
        }
        if offers.get(self.selected_menu_item).is_some() {
            let Some(service) = VillageService::of(lot.kind) else {
                return;
            };
            let request = SessionServiceRequest::Village {
                village: found.id,
                building: u8::try_from(building).unwrap_or(u8::MAX),
                offer: u8::try_from(self.selected_menu_item).unwrap_or(u8::MAX),
                service,
            };
            self.submit_service(request, ctx, requests);
            cues.push(SoundCue::Ui);
        } else if lot.keeper.is_some() {
            if let Some(npc) = keeper_inside(ctx.world.entities(), &found, building) {
                self.start_talk(ctx, npc, true, requests);
                cues.push(SoundCue::Ui);
            } else {
                self.message = keeper_away(ctx.world, &found, building);
                cues.push(SoundCue::UiError);
            }
        }
    }

    /// Why an inn room was refused (`None` for every other deal, which keep the generic
    /// message): rooms are let from dusk, you are already in bed, or you are short of credits.
    pub(super) fn village_room_refusal(
        request: SessionServiceRequest,
        ctx: SeatContext<'_>,
    ) -> Option<String> {
        let SessionServiceRequest::Village {
            village: id,
            building,
            offer,
            ..
        } = request
        else {
            return None;
        };
        let found = ctx.world.village_now(id)?;
        let deal = found
            .offers_at(usize::from(building), ctx.world.market_time())
            .get(usize::from(offer))?
            .deal;
        if deal != VillageDeal::Room {
            return None;
        }
        Some(if !ctx.world.clock().phase().sleepable() {
            "No rooms till dusk: come back when the sun goes down.".to_owned()
        } else if ctx.player.is_some_and(|player| ctx.world.is_in_bed(player)) {
            "You already have a room for the night.".to_owned()
        } else {
            format!(
                "A room is {} credits for the night.",
                fatherload_simulation::clock::INN_ROOM_PRICE
            )
        })
    }

    /// The message (and cue) for a village deal that went through.
    #[allow(
        clippy::too_many_lines,
        reason = "one message per kind of village deal"
    )]
    pub(super) fn village_resolved(
        &mut self,
        request: SessionServiceRequest,
        ctx: SeatContext<'_>,
        spent: u32,
        earned: u32,
    ) -> Option<(String, SoundCue)> {
        let SessionServiceRequest::Village {
            village: id,
            building,
            offer,
            ..
        } = request
        else {
            return None;
        };
        let found = ctx.world.village_now(id)?;
        let building = usize::from(building);
        let deal = found
            .offers_at(building, ctx.world.market_time())
            .get(usize::from(offer))?
            .deal;
        let keeper = found
            .buildings
            .get(building)
            .and_then(|lot| lot.keeper)
            .and_then(|index| found.population.characters.get(usize::from(index)))
            .map_or_else(
                || "The keeper".to_owned(),
                |who| who.name.short().to_owned(),
            );
        Some(match deal {
            VillageDeal::SellCargo { .. } => (
                format!("{keeper}: {earned} credits for the lot. Pleasure doing business."),
                SoundCue::Sell,
            ),
            VillageDeal::SellNightGoods { .. } => (
                format!("The night trader counts out {earned} credits by lantern light."),
                SoundCue::Sell,
            ),
            VillageDeal::Refuel { .. } => {
                self.service_animation = Some(ServiceAnimation::Fuel);
                self.service_animation_seconds = 1.4;
                (
                    format!("Tank filled for {spent} credits."),
                    SoundCue::Upgrade,
                )
            }
            VillageDeal::Repair { .. } => {
                self.service_animation = Some(ServiceAnimation::Repair);
                self.service_animation_seconds = 1.4;
                (
                    format!("Hull mended for {spent} credits."),
                    SoundCue::Upgrade,
                )
            }
            VillageDeal::Part(part) => (
                format!(
                    "{} added to the parts locker ({spent} credits).",
                    part.title()
                ),
                SoundCue::Upgrade,
            ),
            VillageDeal::Cosmetic(item) => (
                format!("{} unlocked at the garage paint bay.", item.label()),
                SoundCue::Upgrade,
            ),
            VillageDeal::Explosives { kind, count } => (
                format!("{count} {} stowed ({spent} credits).", kind.name()),
                SoundCue::Upgrade,
            ),
            VillageDeal::PilotArms(purchase) => (
                format!(
                    "{keeper} hands over the {} ({spent} credits).",
                    purchase.name().to_lowercase()
                ),
                SoundCue::Upgrade,
            ),
            VillageDeal::ChartRegion => (
                format!("{keeper} inks nearby sites and villages onto your map."),
                SoundCue::Ui,
            ),
            VillageDeal::Blessing(kind) => (
                format!(
                    "{}: {} for the next ten minutes.",
                    kind.name(),
                    kind.effect()
                ),
                SoundCue::Ui,
            ),
            VillageDeal::Rumour => (
                "The regulars swear something's buried out there: an echo marked on your map."
                    .to_owned(),
                SoundCue::Ui,
            ),
            VillageDeal::Room => (
                format!("{keeper} hands you a key. Sleep well ({spent} credits)."),
                SoundCue::BedDown,
            ),
            VillageDeal::SkyCraft { recipe, tier } => (
                format!(
                    "{keeper} fits your {} {} ({spent} credits and the sky ore).",
                    recipe.name(),
                    village::services::tier_numeral(tier)
                ),
                SoundCue::Upgrade,
            ),
        })
    }
}

/// Where the keeper of `lot` is when they are not behind their counter ("Mara is at the inn.").
fn keeper_away(world: &WorldState, found: &Village, lot: usize) -> String {
    let Some(keeper) = found.buildings.get(lot).and_then(|lot| lot.keeper) else {
        return "Nobody minds this counter.".to_owned();
    };
    let name = found
        .population
        .characters
        .get(usize::from(keeper))
        .map_or_else(
            || "The keeper".to_owned(),
            |who| who.name.short().to_owned(),
        );
    let character = CharacterId {
        village: found.id,
        index: keeper,
    };
    match world.npc_of(character).and_then(|npc| world.npc(npc)) {
        Some((_, state)) => format!(
            "{name} is out ({}). Try again later.",
            state.activity.phrase()
        ),
        None => format!("{name} is out. Try again later."),
    }
}

/// The banner for arriving in `found`.
fn banner_for(found: &Village, first_visit: bool, tick: u64) -> VillageBanner {
    let kind = match found.setting {
        VillageSetting::Surface => found.size.name(),
        VillageSetting::Underground => "settlement",
        VillageSetting::Sky(kind) => kind.name(),
    };
    let subtitle = if let village::Specialty::MineralBuyer(mineral) = found.specialty {
        format!(
            "{} {kind} - top prices paid for {}",
            capitalized(found.style.name()),
            mineral.name()
        )
    } else {
        format!(
            "{} {kind} - known for its {}",
            capitalized(found.style.name()),
            found.specialty.name()
        )
    };
    VillageBanner {
        village: found.id,
        title: found.name.clone(),
        subtitle,
        first_visit,
        tick,
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl super::ShellState {
    /// Seat `seat`'s rig entered `village` for the first time: raise the discovery banner.
    pub fn village_discovered(
        &mut self,
        seat: usize,
        world: &fatherload_simulation::WorldState,
        village: VillageId,
    ) {
        let tick = self.update_ticks;
        let Some(found) = world.village_now(village) else {
            return;
        };
        if let Some(state) = self.seats.get_mut(seat) {
            state.village.banner = Some(banner_for(&found, true, tick));
        }
    }
}
