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

use fatherload_simulation::PlayerCommand;

use super::{SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::{PlayerInput, prompts::Prompt};

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
        if !ctx.world.can_board_rig(player) {
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
            "On foot. {} jumps, the trigger swings your pick; {} beside the rig boards it.",
            super::key_for(Prompt::Jump, ctx.device),
            super::key_for(Prompt::Use, ctx.device),
        );
        true
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
