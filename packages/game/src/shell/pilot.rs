//! The pilot on foot in the shell (Part II F5): the one exit / board button, walking into
//! buildings, and the messages that teach both.
//!
//! * **Use** (`E`, controller `X`, the arrow-keys seat's Right Ctrl) climbs out of a slow rig
//!   anywhere a building, village door or base module does not claim the button first, and on
//!   foot boards the rig when the pilot stands beside it (boarding comes first on foot, so a rig
//!   parked at a door is still easy to get back into).
//! * **Walking in**: on foot, pushing up at a town or village door walks straight into the
//!   building (rigs keep the use-button flow). Leaving drops the pilot outside the door with a
//!   latch, so holding up never bounces it straight back in; the latch clears once the pilot
//!   steps away from every door.
//! * **Radio for a pickup**: use pressed twice away from the rig hauls the pilot back aboard
//!   for a small fee (`PlayerCommand::Rescue` on foot), so a pilot is never stuck in a pit or
//!   beside a wrecked rig.

use fatherload_simulation::PlayerCommand;

use super::{SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::{PlayerInput, prompts::Prompt};

/// Seconds a first use press away from the rig keeps the pickup radio armed.
pub const PICKUP_CONFIRM_SECONDS: f32 = 3.0;

impl SeatContext<'_> {
    /// Whether the seat's player is out of its rig.
    pub(super) fn on_foot(&self) -> bool {
        self.player
            .is_some_and(|player| self.world.is_on_foot(player))
    }
}

impl SeatUi {
    /// The use button on foot: boards the rig when the pilot is beside it. Returns whether it
    /// did.
    pub(super) fn try_board(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        // A lever or chest nearer than the rig takes the press (WP-PILOT).
        if !ctx.world.can_board_rig(player) || ctx.world.pilot_fixture_first(player) {
            return false;
        }
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::TogglePilot,
        });
        cues.push(SoundCue::Ui);
        "Climbing aboard. Your pack empties into the rig.".clone_into(&mut self.message);
        true
    }

    /// The use button in the rig with nothing else claiming it: climbs out. Returns whether it
    /// did (a rig still moving says so instead).
    pub(super) fn try_exit(
        &mut self,
        ctx: SeatContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        if ctx.on_foot() {
            return false;
        }
        if !ctx.world.can_exit_rig(player) {
            "Slow down to climb out.".clone_into(&mut self.message);
            return true;
        }
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::TogglePilot,
        });
        self.message = format!(
            "On foot. {} jumps, the trigger swings your pick or fires your gun, {} throws a rope; {} beside the rig boards it.",
            super::key_for(Prompt::Jump, ctx.device),
            super::key_for(Prompt::Rope, ctx.device),
            super::key_for(Prompt::Use, ctx.device),
        );
        true
    }

    /// The use button on foot at a ruin's lever, chest or keycard door (WP-PILOT): works it.
    /// Returns whether one was in reach.
    pub(super) fn try_use_fixture(
        &mut self,
        ctx: SeatContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        use fatherload_simulation::mechanism::{Usable, keycard_item, keycard_name};
        let Some(player) = ctx.player else {
            return false;
        };
        let Some((_, usable)) = ctx.world.pilot_usable(player) else {
            return false;
        };
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Pilot(fatherload_simulation::pilot::PilotAction::Use),
        });
        let line = match usable {
            Usable::Lever { on: false, .. } => "The lever grinds over. Somewhere a gate moves.",
            Usable::Lever { on: true, .. } => "You throw the lever back.",
            Usable::Chest => "You heave the old lid open.",
            Usable::KeycardDoor { color } => {
                let carried = ctx.player().is_some_and(|player| {
                    let card = keycard_item(color);
                    player.pilot_pack.count(card) > 0 || player.carried(card) > 0
                });
                if carried {
                    "The reader blinks green and the door slides away."
                } else {
                    self.message = format!(
                        "Locked. It wants the {} keycard: search the ruin's chests.",
                        keycard_name(color)
                    );
                    return true;
                }
            }
            Usable::TempleFitting(fitting) => match fitting {
                fatherload_simulation::sky::SkyTile::TempleLever => {
                    "The temple lever groans over. Stone grinds somewhere below."
                }
                fatherload_simulation::sky::SkyTile::TempleKeystone => {
                    "You prise the keystone off its plinth."
                }
                fatherload_simulation::sky::SkyTile::KeystoneDoor => {
                    let carried = ctx.player().is_some_and(|player| {
                        player.pilot_pack.count(fatherload_simulation::item::TEMPLE_KEYSTONE) > 0
                            || player.carried(fatherload_simulation::item::TEMPLE_KEYSTONE) > 0
                    });
                    if carried {
                        "The keystone settles into its socket and the vault gate rolls aside."
                    } else {
                        "A keystone-shaped socket. The temple's keystone must be somewhere above."
                    }
                }
                _ => "You break the relic chest's seal.",
            },
        };
        line.clone_into(&mut self.message);
        true
    }

    /// The use button on foot away from the rig: the first press says how to board and arms
    /// the radio (with a rig link it also calls the rig over on autopilot, WP-PILOT); a second
    /// press within [`PICKUP_CONFIRM_SECONDS`] radios for a pickup ([`PlayerCommand::Rescue`]
    /// on foot: hauled back aboard for a small fee), the way out of a pit the pick cannot dig
    /// or back to a wrecked rig.
    pub(super) fn radio_pickup(&mut self, ctx: SeatContext<'_>, requests: &mut Vec<ShellRequest>) {
        let use_key = super::key_for(Prompt::Use, ctx.device);
        let linked = ctx.player.is_some_and(|player| {
            ctx.world
                .pilot_of(player)
                .is_some_and(|(_, _, state)| state.gear.rig_link > 0)
        });
        let wrecked_now = ctx.player().is_some_and(|player| player.hull <= 0.0);
        if linked && !wrecked_now && self.pickup_armed <= 0.0 {
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command: PlayerCommand::Pilot(fatherload_simulation::pilot::PilotAction::RecallRig),
            });
            self.pickup_armed = PICKUP_CONFIRM_SECONDS;
            self.message = format!(
                "Rig link: calling the rig over. Press {use_key} again to radio for a pickup ({} cr).",
                fatherload_simulation::pilot::PILOT_RECALL_FEE
            );
            return;
        }
        if self.pickup_armed > 0.0 {
            self.pickup_armed = 0.0;
            requests.push(ShellRequest::Action {
                seat: ctx.seat,
                command: PlayerCommand::Rescue,
            });
            self.message = format!(
                "Pickup called: hauled back aboard for {} credits.",
                fatherload_simulation::pilot::PILOT_RECALL_FEE
            );
            return;
        }
        self.pickup_armed = PICKUP_CONFIRM_SECONDS;
        let wrecked = ctx.player().is_some_and(|player| player.hull <= 0.0);
        self.message = if wrecked {
            format!(
                "Your rig is wrecked. Press {use_key} again to radio for a pickup ({} cr).",
                fatherload_simulation::pilot::PILOT_RECALL_FEE
            )
        } else {
            format!(
                "Walk up to your rig to board it, or press {use_key} again to radio for a pickup ({} cr).",
                fatherload_simulation::pilot::PILOT_RECALL_FEE
            )
        };
    }

    /// On foot outdoors: walks into the building whose door the pilot pushes up against.
    /// Returns whether the seat went inside.
    pub(super) fn walk_in(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        if !ctx.on_foot() || self.modal.is_some() {
            return false;
        }
        let zone = ctx.current_zone();
        let village = ctx
            .player
            .and_then(|player| ctx.world.player_village_building(player));
        if zone.is_none() && village.is_none() {
            self.door_latch = false;
            return false;
        }
        if self.door_latch || !input.thrust || input.jump {
            return false;
        }
        if let Some(zone) = zone {
            self.enter_interior(zone, ctx.device, cues);
            self.door_latch = true;
            return true;
        }
        let entered = self.try_enter_village(ctx, cues);
        self.door_latch |= entered;
        entered
    }
}
