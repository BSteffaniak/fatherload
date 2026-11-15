//! Build mode and base module menus (WP-BASE).
//!
//! **Build mode** ([`SeatUi::build`]) parks the seat's rig and moves a grid cursor instead:
//! directions step it a tile at a time (it starts in front of the rig and stays within
//! [`BUILD_REACH`]), and on the main keyboard the mouse points it. Confirm (or a left click)
//! places the selected piece, the remove button (X, a right click, a controller's use button)
//! deconstructs, rotate turns the piece, and previous/next (Z/Q, the wheel, LB/RB) pick from
//! [`Buildable::PALETTE`]; number keys 1-3 jump to its groups. Every action is checked with
//! [`WorldState::check_build`] first, so a refused one buzzes with its reason instead of being
//! sent; the host checks it again inside the tick.
//!
//! **Module menus** ([`ModalScreen::BaseModule`], module in [`SeatUi::base_module`]) open with
//! the use button at a module outdoors: one row per service, sent as a
//! [`BuildCommand::Use`]. A stranded rig at a fuel station can still open it.

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    base::{
        BUILD_REACH, BuildCommand, BuildGroup, Buildable, Facing, ModuleKind, ModuleService,
        StructureId,
    },
    terrain::{TILE_SIZE, TilePosition},
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::{PlayerInput, prompts::Prompt};

/// A seat's build mode: the selected piece, its facing and the cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildDraft {
    /// Index into [`Buildable::PALETTE`].
    pub selected: usize,
    pub facing: Facing,
    /// Cursor offset from the rig's tile (keyboard and controller).
    pub offset: (i32, i32),
    /// The tile the mouse points at, while the mouse leads.
    pub pointed: Option<TilePosition>,
}

impl Default for BuildDraft {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildDraft {
    /// The claim beacon selected, the cursor beside the rig.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            selected: 0,
            facing: Facing::Right,
            offset: (1, 0),
            pointed: None,
        }
    }

    /// The selected piece.
    #[must_use]
    pub const fn item(&self) -> Buildable {
        Buildable::PALETTE[self.selected % Buildable::PALETTE.len()]
    }

    /// The cursor tile for a rig standing on `rig`.
    #[must_use]
    pub fn target(&self, rig: TilePosition) -> TilePosition {
        self.pointed.unwrap_or(TilePosition {
            x: rig.x + self.offset.0,
            y: rig.y + self.offset.1,
        })
    }

    /// The place command for the cursor.
    #[must_use]
    pub fn place_command(&self, at: TilePosition) -> BuildCommand {
        BuildCommand::Place {
            item: self.item(),
            at,
            facing: self.facing.within(self.item().facings()),
        }
    }

    fn cycle(&mut self, step: isize) {
        let count = Buildable::PALETTE.len().cast_signed();
        self.selected = (self.selected.cast_signed() + step)
            .rem_euclid(count)
            .cast_unsigned();
        self.facing = self.facing.within(self.item().facings());
    }

    fn jump_to(&mut self, group: BuildGroup) {
        if let Some(index) = Buildable::PALETTE.iter().position(|item| {
            item.group() == group && *item != Buildable::Module(ModuleKind::Beacon)
        }) {
            self.selected = index;
        }
    }

    fn step_cursor(&mut self, rig: TilePosition, dx: i32, dy: i32) {
        if let Some(pointed) = self.pointed.take() {
            self.offset = (pointed.x - rig.x, pointed.y - rig.y);
        }
        self.offset = (
            (self.offset.0 + dx).clamp(-BUILD_REACH, BUILD_REACH),
            (self.offset.1 + dy).clamp(-BUILD_REACH, BUILD_REACH),
        );
    }
}

impl SeatUi {
    /// Opens build mode.
    pub(super) fn open_build(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        self.build = Some(self.last_build.unwrap_or_default());
        self.modal = None;
        cues.push(SoundCue::Ui);
        self.message = format!(
            "Build mode. {} place, {} take down, {} pick, {} rotate, {} done.",
            super::key_for(Prompt::BuildPlace, ctx.device),
            super::key_for(Prompt::BuildRemove, ctx.device),
            super::key_for(Prompt::BuildCycle, ctx.device),
            super::key_for(Prompt::BuildRotate, ctx.device),
            super::key_for(Prompt::BuildMode, ctx.device),
        );
    }

    /// One frame of build-mode input. Returns `true` when the seat asked for the pause menu.
    pub(super) fn handle_build(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(mut draft) = self.build else {
            return false;
        };
        if input.build_mode || input.backs_out() || input.pause || ctx.failed() {
            self.last_build = Some(draft);
            self.build = None;
            cues.push(SoundCue::UiBack);
            return input.pause;
        }
        let Some((player_id, rig)) = ctx.player.and_then(|id| {
            ctx.world
                .player(id)
                .map(|rig| (id, rig.tile_position(TILE_SIZE)))
        }) else {
            return false;
        };
        let before = draft.selected;
        if input.tab_next || input.cycle_explosive || input.ui_scroll < -0.5 {
            draft.cycle(1);
        }
        if input.tab_previous || input.build_previous || input.ui_scroll > 0.5 {
            draft.cycle(-1);
        }
        if let Some(key) = input.selected_upgrade {
            draft.jump_to(match key {
                0 => BuildGroup::Structure,
                1 => BuildGroup::Interior,
                _ => BuildGroup::Modules,
            });
        }
        if draft.selected != before {
            cues.push(SoundCue::UiMove);
        }
        if input.build_rotate {
            let allowed = draft.item().facings();
            let turned = draft.facing.next_in(allowed);
            if turned != draft.facing {
                draft.facing = turned;
                cues.push(SoundCue::UiMove);
            }
        }
        let dx = i32::from(input.menu_right) - i32::from(input.menu_left);
        let dy = i32::from(input.menu_down) - i32::from(input.menu_up);
        if dx != 0 || dy != 0 {
            draft.step_cursor(rig, dx, dy);
        }
        if let Some((x, y)) = input.build_pointer {
            #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
            let tile = TilePosition {
                x: (x / TILE_SIZE).floor() as i32,
                y: (y / TILE_SIZE).floor() as i32,
            };
            draft.pointed = Some(tile);
        }
        let target = draft.target(rig);
        let place = input.confirm || input.pointer_primary;
        let remove = !place && (input.build_remove || input.pointer_secondary || input.interact);
        let command = if place {
            Some(draft.place_command(target))
        } else if remove {
            Some(BuildCommand::Remove { at: target })
        } else {
            None
        };
        if let Some(command) = command {
            match ctx.world.check_build(player_id, &command) {
                Ok(_) => requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Build(command),
                }),
                Err(refusal) => {
                    self.message = refusal.message();
                    cues.push(SoundCue::UiError);
                }
            }
        }
        self.build = Some(draft);
        false
    }

    /// Opens the menu of the module the seat's rig stands at, if any. Returns whether one
    /// opened.
    pub(super) fn open_base_module(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        let Some(module) = module_at_rig(ctx.world, ctx.player) else {
            return false;
        };
        self.base_module = Some(module);
        self.modal = Some(ModalScreen::BaseModule);
        self.selected_menu_item = 0;
        cues.push(SoundCue::Ui);
        true
    }

    /// One frame of a module menu.
    pub(super) fn handle_base_module(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let module = self
            .base_module
            .and_then(|id| ctx.world.bases().module(id))
            .filter(|module| {
                ctx.player
                    .and_then(|id| ctx.world.player(id))
                    .is_some_and(|rig| module.reachable_from(rig.tile_position(TILE_SIZE)))
            });
        let Some(module) = module else {
            // Taken down, or the rig drove off.
            self.modal = None;
            self.base_module = None;
            return;
        };
        if input.backs_out() {
            self.modal = None;
            self.base_module = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        let services = module.kind.services();
        let last = services.len().saturating_sub(1);
        self.move_selection(input, last);
        if !input.confirm && !input.interact {
            return;
        }
        let (Some(service), Some(player_id)) =
            (services.get(self.selected_menu_item).copied(), ctx.player)
        else {
            return;
        };
        let command = BuildCommand::Use {
            structure: module.id,
            service,
        };
        match ctx.world.check_build(player_id, &command) {
            Ok(_) => {
                requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Build(command),
                });
                self.message = service_message(module.kind, service);
            }
            Err(refusal) => {
                self.message = refusal.message();
                cues.push(SoundCue::UiError);
            }
        }
    }
}

/// The module a rig of `player` can use where it stands.
#[must_use]
pub fn module_at_rig(world: &WorldState, player: Option<PlayerId>) -> Option<StructureId> {
    let rig = world.player(player?)?.tile_position(TILE_SIZE);
    world
        .bases()
        .module_reachable_from(rig)
        .filter(|module| !module.kind.services().is_empty())
        .map(|module| module.id)
}

/// The toast after a module service is sent.
fn service_message(kind: ModuleKind, service: ModuleService) -> String {
    match service {
        ModuleService::Refuel => "Topping up from the station's reserve.".to_owned(),
        ModuleService::OrderSupply => "Supply drop ordered: the reserve is filling.".to_owned(),
        ModuleService::Repair => "The repair arms get to work.".to_owned(),
        ModuleService::DepositCargo => format!("Cargo stowed in the {}.", kind.name()),
        ModuleService::DepositBlocks => format!("Blocks stowed in the {}.", kind.name()),
        ModuleService::WithdrawAll => format!("Loaded up from the {}.", kind.name()),
        ModuleService::SellCargo => "Cargo beamed to the depot.".to_owned(),
        ModuleService::SetHome => {
            "Home sweet home. The rescue crew will bring you here.".to_owned()
        }
        ModuleService::CycleAccess => "Claim access changed.".to_owned(),
    }
}
