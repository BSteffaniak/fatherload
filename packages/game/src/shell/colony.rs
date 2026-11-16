//! The lunar research colony's counter (WP-MOON): opened with the use button by the rig on
//! the landing pad or the pilot anywhere in the colony (`at_colony_counter`), one row per
//! [`ColonyOffer`], each sent as [`MoonCommand::Colony`]. The simulation validates the same
//! rule inside the tick, so the menu works the same online, in split-screen and by touch (rows
//! are tapped like every menu's).

use fatherload_simulation::{
    PlayerCommand,
    moon::{ColonyOffer, MoonCommand},
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

impl SeatUi {
    /// Opens the colony counter if the seat's pilot is inside the colony or its rig is on the
    /// landing pad (elsewhere in the colony the rig's use button lets the pilot out). Returns
    /// whether it opened.
    pub(super) fn open_colony(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        if !ctx.world.at_colony_counter(player) {
            return false;
        }
        self.modal = Some(ModalScreen::Colony);
        self.selected_menu_item = 0;
        "Welcome to Tranquility Station. Air, fuel and fair prices.".clone_into(&mut self.message);
        cues.push(SoundCue::Ui);
        true
    }

    /// One frame of the colony counter.
    pub(super) fn handle_colony(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(player_id) = ctx.player else {
            self.modal = None;
            return;
        };
        if !ctx.world.player_in_colony(player_id) {
            // Flew or walked off.
            self.modal = None;
            return;
        }
        if input.backs_out() {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let last = ColonyOffer::ALL.len() - 1;
        self.move_selection(input, last);
        if !input.confirm && !input.interact {
            return;
        }
        let offer = ColonyOffer::ALL[self.selected_menu_item.min(last)];
        if ctx.world.colony_offer_ready(player_id, offer) {
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command: PlayerCommand::Moon(MoonCommand::Colony(offer)),
            });
            self.message = colony_message(offer);
        } else {
            self.message = refusal(ctx, offer);
            cues.push(SoundCue::UiError);
        }
    }
}

/// Why `offer` cannot be taken now.
fn refusal(ctx: SeatContext<'_>, offer: ColonyOffer) -> String {
    let Some(player) = ctx.player() else {
        return "No rig.".to_owned();
    };
    if ctx
        .player
        .is_some_and(|id| !ctx.world.rig_within_service_reach(id))
    {
        return "Bring the rig closer: the colony services it on the pad.".to_owned();
    }
    offer.price(player).map_or_else(
        || {
            match offer {
                ColonyOffer::Refuel => "The tank is full.",
                ColonyOffer::Repair => "The hull is sound.",
                ColonyOffer::MeltIce => "No moon ice aboard (or the tank is full).",
                ColonyOffer::SellHaul => "Nothing lunar to sell.",
                ColonyOffer::Reactor | ColonyOffer::AlloyDrill => "Already fitted.",
                ColonyOffer::BuyWeapon(_) => "Already on your rack.",
                ColonyOffer::UpgradeWeapon(_) => "Buy it first (or it is fully tuned).",
            }
            .to_owned()
        },
        |price| format!("Needs {}.", price.label()),
    )
}

/// The toast after a colony deal is sent.
fn colony_message(offer: ColonyOffer) -> String {
    match offer {
        ColonyOffer::Refuel => "Fuel lines hiss. Imported, but it burns the same.".to_owned(),
        ColonyOffer::Repair => "The colony's arms patch the hull.".to_owned(),
        ColonyOffer::MeltIce => "The still melts your ice into premium fuel.".to_owned(),
        ColonyOffer::SellHaul => "The quartermaster weighs your haul and pays up.".to_owned(),
        ColonyOffer::Reactor => "Helium-3 reactor fitted: the engines sip fuel now.".to_owned(),
        ColonyOffer::AlloyDrill => "Alloy teeth fitted: the drill bites deeper.".to_owned(),
        ColonyOffer::BuyWeapon(weapon) => format!("{} bolted to your rack.", weapon.name()),
        ColonyOffer::UpgradeWeapon(weapon) => format!("{} retuned.", weapon.name()),
    }
}
