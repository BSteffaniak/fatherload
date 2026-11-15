//! Villages in the shell: walking into a village building, its counter (the building's deals,
//! bought as [`SessionServiceRequest::Village`]), talking to residents, and the arrival banner.
//!
//! Talking is a placeholder until WP-PEOPLE's NPC entities: residents stand still in their
//! rooms (and outdoors), and a conversation runs [`fatherload_simulation::people::talk`] against
//! a per-seat [`CharacterState`] that lives only in this session (it is not simulated, saved or
//! replicated yet), so repeat visits still vary sensibly.

use std::{collections::BTreeMap, sync::Arc};

use fatherload_simulation::{
    PlayerId, SessionServiceRequest,
    people::{CharacterId, CharacterState, TalkContext, talk},
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
/// What residents call a player in conversation (the sim has no player names yet).
const PLAYER_NAME: &str = "Driller";

/// The village building a seat is inside.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VillageInterior {
    pub village: VillageId,
    pub building: u8,
}

/// An open conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TalkView {
    pub speaker: CharacterId,
    pub name: String,
    /// "Trader, 34" style line.
    pub about: String,
    pub lines: Vec<String>,
    /// Opened from the counter menu (Back returns to it).
    pub from_counter: bool,
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

/// A seat's village state: where it is inside, its open conversation, its arrival banner and
/// what residents remember of this session's conversations.
#[derive(Clone, Debug, Default)]
pub struct SeatVillage {
    pub interior: Option<VillageInterior>,
    pub talk: Option<TalkView>,
    pub banner: Option<VillageBanner>,
    talk_states: BTreeMap<CharacterId, CharacterState>,
    /// The village the rig was in on the previous frame.
    last_village: Option<VillageId>,
}

impl SeatVillage {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            interior: None,
            talk: None,
            banner: None,
            talk_states: BTreeMap::new(),
            last_village: None,
        }
    }
}

/// Where the residents of `building` stand in its room (room units), with their indices. The
/// keeper stands behind the counter and is reached through the counter menu.
#[must_use]
pub fn room_residents(found: &Village, building: usize) -> Vec<(u16, f32)> {
    let Some(lot) = found.buildings.get(building) else {
        return Vec::new();
    };
    let spots: &[f32] = if lot.keeper.is_some() {
        &[230.0, 310.0]
    } else {
        &[200.0, 300.0, 400.0, 500.0]
    };
    lot.residents
        .iter()
        .filter(|resident| Some(**resident) != lot.keeper)
        .zip(spots)
        .map(|(resident, x)| (*resident, *x))
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

/// The counter rows of `building`: its deals, then a row to talk to the keeper.
#[must_use]
pub fn counter_rows(found: &Village, building: usize) -> usize {
    found.offers(building).len()
        + usize::from(
            found
                .buildings
                .get(building)
                .is_some_and(|lot| lot.keeper.is_some()),
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
    ) -> bool {
        if input.pause {
            return true;
        }
        let Some((found, building)) = self
            .village
            .interior
            .and_then(|interior| interior_village(ctx.world.world_settings(), interior))
        else {
            self.village.interior = None;
            return false;
        };
        let movement = input.horizontal;
        if movement.abs() > f32::EPSILON {
            self.interior_facing = movement.signum();
        }
        self.interior_x = movement
            .mul_add(185.0 * delta_seconds, self.interior_x)
            .clamp(42.0, 598.0);
        if input.backs_out() || (input.interact && self.interior_x < DOOR_REACH) {
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
        let near = room_residents(&found, building)
            .into_iter()
            .find(|(_, x)| (self.interior_x - x).abs() <= VILLAGE_REACH * 0.7);
        if let Some((resident, _)) = near {
            self.start_talk(ctx, &found, resident, false);
            cues.push(SoundCue::Ui);
        } else if lot.keeper.is_some() {
            "Walk to the counter, someone to talk to, or the door.".clone_into(&mut self.message);
        } else {
            "Walk up to someone to talk, or back to the door.".clone_into(&mut self.message);
        }
        false
    }

    /// Opens a conversation with resident `resident` of `found`.
    fn start_talk(
        &mut self,
        ctx: SeatContext<'_>,
        found: &Village,
        resident: u16,
        from_counter: bool,
    ) {
        let Some(character) = found.population.characters.get(usize::from(resident)) else {
            return;
        };
        let facts = village::village_facts(ctx.world.world_settings(), found);
        let talk_ctx = TalkContext {
            seed: ctx.world.world_settings().seed,
            day: ctx.world.progression().town_event_day,
            player: ctx.player.unwrap_or(PlayerId::new(1)),
            player_name: PLAYER_NAME,
            population: Some(&found.population),
            facts: &facts,
        };
        let state = self.village.talk_states.entry(character.id).or_default();
        let lines = talk(character, state, &talk_ctx)
            .into_iter()
            .map(|line| line.text)
            .collect();
        let role = character.role.name();
        let mut about = String::with_capacity(role.len() + 8);
        about.push_str(&role[..1].to_uppercase());
        about.push_str(&role[1..]);
        about.push_str(", ");
        about.push_str(&character.age.to_string());
        self.village.talk = Some(TalkView {
            speaker: character.id,
            name: character.name.full(),
            about,
            lines,
            from_counter,
        });
        self.modal = Some(ModalScreen::VillageTalk);
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
        let interior = self
            .village
            .interior
            .and_then(|interior| interior_village(ctx.world.world_settings(), interior));
        if modal == ModalScreen::VillageTalk {
            if input.backs_out() {
                // Back to the counter when the talk started there.
                let from_counter = self
                    .village
                    .talk
                    .take()
                    .is_some_and(|talk| talk.from_counter);
                self.modal = from_counter.then_some(ModalScreen::VillageCounter);
                return;
            }
            if (input.confirm || input.interact)
                && let (Some((found, _)), Some(talk)) = (&interior, &self.village.talk)
            {
                let (resident, from_counter) = (talk.speaker.index, talk.from_counter);
                let found = Arc::clone(found);
                self.start_talk(ctx, &found, resident, from_counter);
                cues.push(SoundCue::Ui);
            }
            return;
        }
        let Some((found, building)) = interior else {
            self.modal = None;
            return;
        };
        if input.backs_out() {
            self.modal = None;
            return;
        }
        let rows = counter_rows(&found, building);
        self.move_selection(input, rows.saturating_sub(1));
        if !(input.confirm || input.interact) {
            return;
        }
        let offers = found.offers(building);
        let lot = &found.buildings[building];
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
        } else if let Some(keeper) = lot.keeper {
            self.start_talk(ctx, &found, keeper, true);
            cues.push(SoundCue::Ui);
        }
    }

    /// The message (and cue) for a village deal that went through.
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
        let found = village::village(ctx.world.world_settings(), id)?;
        let building = usize::from(building);
        let deal = found.offers(building).get(usize::from(offer))?.deal;
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
        })
    }
}

/// The banner for arriving in `found`.
fn banner_for(found: &Village, first_visit: bool, tick: u64) -> VillageBanner {
    let kind = match found.setting {
        VillageSetting::Surface => found.size.name(),
        VillageSetting::Underground => "deep settlement",
    };
    let subtitle = if let village::Specialty::MineralBuyer(mineral) = found.specialty {
        format!(
            "{} {kind} - the assayer pays top price for {}",
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
        let Some(found) = village::village(world.world_settings(), village) else {
            return;
        };
        if let Some(state) = self.seats.get_mut(seat) {
            state.village.banner = Some(banner_for(&found, true, tick));
        }
    }
}
