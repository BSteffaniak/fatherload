//! The garage: the Repair Garage's paint bay, opened from the repair counter's last row.
//!
//! The seat edits a draft look ([`SeatUi::garage`]) with live preview (the rig in the room and
//! the garage's preview panel both wear the draft), then applies it as one
//! [`SessionServiceRequest::CustomizeRig`]. Up/down picks a row; left/right (or LB/RB, or
//! confirm) cycles that row's option; Back returns to the counter without applying.

use fatherload_simulation::{
    CosmeticItem, PlayerId, RigCosmetics, SessionServiceRequest,
    cosmetics::{
        Decal, DrillStyle, FlameColor, FrontAttachment, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// The repair counter row that opens the garage.
pub const GARAGE_COUNTER_ROW: usize = 3;

/// One row of the garage panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GarageRow {
    Primary,
    Secondary,
    Trim,
    Flame,
    Drill,
    Pattern,
    Decal,
    Roof,
    Front,
    Rear,
    /// A random look from the player's unlocked items.
    Shuffle,
    /// Back to the player's default look.
    Default,
    /// Apply the draft (free).
    Apply,
}

impl GarageRow {
    /// Every row, in panel order.
    pub const ALL: [Self; 13] = [
        Self::Primary,
        Self::Secondary,
        Self::Trim,
        Self::Flame,
        Self::Drill,
        Self::Pattern,
        Self::Decal,
        Self::Roof,
        Self::Front,
        Self::Rear,
        Self::Shuffle,
        Self::Default,
        Self::Apply,
    ];

    /// The row's label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Primary => "Hull paint",
            Self::Secondary => "Chassis paint",
            Self::Trim => "Trim paint",
            Self::Flame => "Thruster flame",
            Self::Drill => "Drill style",
            Self::Pattern => "Pattern",
            Self::Decal => "Decal",
            Self::Roof => "Roof",
            Self::Front => "Nose",
            Self::Rear => "Tail",
            Self::Shuffle => "Surprise me",
            Self::Default => "Crew colours",
            Self::Apply => "Apply paint job",
        }
    }

    /// The item this row shows for `look` (option rows only).
    #[must_use]
    pub const fn item(self, look: &RigCosmetics) -> Option<CosmeticItem> {
        Some(match self {
            Self::Primary => CosmeticItem::Paint(look.primary),
            Self::Secondary => CosmeticItem::Paint(look.secondary),
            Self::Trim => CosmeticItem::Paint(look.trim),
            Self::Flame => CosmeticItem::Flame(look.flame),
            Self::Drill => CosmeticItem::Drill(look.drill_style),
            Self::Pattern => CosmeticItem::Pattern(look.pattern),
            Self::Decal => CosmeticItem::Decal(look.decal),
            Self::Roof => CosmeticItem::Roof(look.roof),
            Self::Front => CosmeticItem::Front(look.front),
            Self::Rear => CosmeticItem::Rear(look.rear),
            Self::Shuffle | Self::Default | Self::Apply => return None,
        })
    }

    /// `look` with this row's option moved `step` places.
    #[must_use]
    pub fn cycle(self, mut look: RigCosmetics, step: i32) -> RigCosmetics {
        match self {
            Self::Primary => look.primary = look.primary.cycled(step),
            Self::Secondary => look.secondary = look.secondary.cycled(step),
            Self::Trim => look.trim = look.trim.cycled(step),
            Self::Flame => look.flame = look.flame.cycled(step),
            Self::Drill => look.drill_style = look.drill_style.cycled(step),
            Self::Pattern => look.pattern = look.pattern.cycled(step),
            Self::Decal => look.decal = look.decal.cycled(step),
            Self::Roof => look.roof = look.roof.cycled(step),
            Self::Front => look.front = look.front.cycled(step),
            Self::Rear => look.rear = look.rear.cycled(step),
            Self::Shuffle | Self::Default | Self::Apply => {}
        }
        look
    }
}

/// A seat's open garage: the look being edited.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GarageDraft {
    pub look: RigCosmetics,
    /// Shuffle state (UI only, never gameplay).
    seed: u64,
}

impl GarageDraft {
    /// A draft starting from the look the rig wears now.
    #[must_use]
    pub fn of(player_id: PlayerId, current: Option<RigCosmetics>) -> Self {
        Self {
            look: RigCosmetics::resolve(current, player_id),
            seed: player_id.get().wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    /// Mixes `salt` (the tick the garage opened on) into the shuffle state, so "Surprise me"
    /// does not deal the same looks every visit.
    #[must_use]
    pub const fn salted(mut self, salt: u64) -> Self {
        self.seed = (self.seed ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9)) | 1;
        self
    }

    const fn next(&mut self) -> u64 {
        // xorshift64*
        self.seed ^= self.seed >> 12;
        self.seed ^= self.seed << 25;
        self.seed ^= self.seed >> 27;
        self.seed.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<T: Copy>(&mut self, options: &[T], allowed: impl Fn(T) -> bool) -> Option<T> {
        let allowed: Vec<T> = options
            .iter()
            .copied()
            .filter(|item| allowed(*item))
            .collect();
        if allowed.is_empty() {
            return None;
        }
        let index = usize::try_from(self.next() % allowed.len() as u64).unwrap_or_default();
        Some(allowed[index])
    }

    /// A random look from the items `allowed` lets the player use: three different paints and
    /// a random pick for every other slot.
    fn shuffle(&mut self, allowed: impl Fn(CosmeticItem) -> bool) {
        let paint = |paint: PaintColor| allowed(CosmeticItem::Paint(paint));
        let mut look = self.look;
        if let Some(primary) = self.pick(PaintColor::ALL, paint) {
            look.primary = primary;
        }
        if let Some(secondary) = self.pick(PaintColor::ALL, |p| paint(p) && p != look.primary) {
            look.secondary = secondary;
        }
        if let Some(trim) = self.pick(PaintColor::ALL, |p| {
            paint(p) && p != look.primary && p != look.secondary
        }) {
            look.trim = trim;
        }
        look.flame = self
            .pick(FlameColor::ALL, |item| allowed(CosmeticItem::Flame(item)))
            .unwrap_or(look.flame);
        look.drill_style = self
            .pick(DrillStyle::ALL, |item| allowed(CosmeticItem::Drill(item)))
            .unwrap_or(look.drill_style);
        look.pattern = self
            .pick(HullPattern::ALL, |item| {
                allowed(CosmeticItem::Pattern(item))
            })
            .unwrap_or(look.pattern);
        look.decal = self
            .pick(Decal::ALL, |item| allowed(CosmeticItem::Decal(item)))
            .unwrap_or(look.decal);
        look.roof = self
            .pick(RoofAttachment::ALL, |item| {
                allowed(CosmeticItem::Roof(item))
            })
            .unwrap_or(look.roof);
        look.front = self
            .pick(FrontAttachment::ALL, |item| {
                allowed(CosmeticItem::Front(item))
            })
            .unwrap_or(look.front);
        look.rear = self
            .pick(RearAttachment::ALL, |item| {
                allowed(CosmeticItem::Rear(item))
            })
            .unwrap_or(look.rear);
        self.look = look;
    }
}

/// Whether `player` may use `item` (a starter item, one they unlocked, or anything in a
/// test-mode world).
#[must_use]
pub fn item_available(
    test_mode: bool,
    player: &fatherload_simulation::Player,
    item: CosmeticItem,
) -> bool {
    test_mode || player.cosmetic_unlocks.has(item)
}

/// The request applying `look` for `player_id`: `None` when it is their default look, so the
/// rig keeps following the per-player default.
#[must_use]
pub fn customize_request(player_id: PlayerId, look: RigCosmetics) -> SessionServiceRequest {
    let cosmetics = (look != RigCosmetics::default_for(player_id)).then_some(look);
    SessionServiceRequest::CustomizeRig { cosmetics }
}

impl SeatUi {
    /// Opens the garage with a draft of the rig's current look.
    pub(super) fn open_garage(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        let (Some(player_id), Some(player)) = (ctx.player, ctx.player()) else {
            return;
        };
        self.garage = Some(
            GarageDraft::of(player_id, player.cosmetics).salted(ctx.world.simulation_tick().get()),
        );
        self.modal = Some(ModalScreen::Garage);
        self.selected_menu_item = 0;
        "Iona: Pick your colours. The rig in the bay shows the job before you commit."
            .clone_into(&mut self.message);
        cues.push(SoundCue::Ui);
    }

    /// One frame of input in the garage. Always consumes the input.
    pub(super) fn handle_garage(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if input.backs_out() {
            self.garage = None;
            self.clear_fresh_unlocks();
            self.modal = Some(ModalScreen::Repair);
            self.selected_menu_item = GARAGE_COUNTER_ROW;
            cues.push(SoundCue::UiBack);
            return;
        }
        let (Some(player_id), Some(player)) = (ctx.player, ctx.player()) else {
            return;
        };
        let Some(mut draft) = self.garage else {
            self.garage = Some(GarageDraft::of(player_id, player.cosmetics));
            return;
        };
        let test_mode = ctx.world.is_test_mode();
        let last = GarageRow::ALL.len() - 1;
        if input.menu_up {
            self.selected_menu_item = self.selected_menu_item.saturating_sub(1);
        }
        if input.menu_down {
            self.selected_menu_item = (self.selected_menu_item + 1).min(last);
        }
        let row = GarageRow::ALL[self.selected_menu_item.min(last)];
        let step = i32::from(input.menu_right || input.tab_next)
            - i32::from(input.menu_left || input.tab_previous)
            + i32::from(input.confirm && row.item(&draft.look).is_some());
        if step != 0 && row.item(&draft.look).is_some() {
            draft.look = row.cycle(draft.look, step.signum());
            cues.push(SoundCue::UiMove);
        }
        if input.confirm {
            match row {
                GarageRow::Shuffle => {
                    draft.shuffle(|item| item_available(test_mode, player, item));
                    cues.push(SoundCue::Ui);
                }
                GarageRow::Default => {
                    draft.look = RigCosmetics::default_for(player_id);
                    cues.push(SoundCue::Ui);
                }
                GarageRow::Apply => {
                    let locked = draft
                        .look
                        .items()
                        .into_iter()
                        .find(|item| !item_available(test_mode, player, *item));
                    if let Some(item) = locked {
                        self.message = format!(
                            "{} is locked. Earn it out there, then come back.",
                            item.label()
                        );
                        cues.push(SoundCue::UiError);
                    } else {
                        self.submit_service(
                            customize_request(player_id, draft.look),
                            ctx,
                            requests,
                        );
                    }
                }
                _ => {}
            }
        }
        self.garage = Some(draft);
    }

    /// The look the seat's rig shows while the garage is open (live preview).
    #[must_use]
    pub const fn garage_preview(&self) -> Option<RigCosmetics> {
        match (self.modal, self.garage) {
            (Some(ModalScreen::Garage), Some(draft)) => Some(draft.look),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_cycle_their_own_option_only() {
        let look = RigCosmetics::default_for(PlayerId::new(1));
        let next = GarageRow::Trim.cycle(look, 1);
        assert_eq!(next.trim, look.trim.cycled(1));
        assert_eq!(next.primary, look.primary);
        assert_eq!(GarageRow::Apply.cycle(look, 1), look);
        assert!(GarageRow::Apply.item(&look).is_none());
    }

    #[test]
    fn the_default_look_is_applied_as_none() {
        let id = PlayerId::new(3);
        assert_eq!(
            customize_request(id, RigCosmetics::default_for(id)),
            SessionServiceRequest::CustomizeRig { cosmetics: None }
        );
        let custom = GarageRow::Primary.cycle(RigCosmetics::default_for(id), 1);
        assert_eq!(
            customize_request(id, custom),
            SessionServiceRequest::CustomizeRig {
                cosmetics: Some(custom)
            }
        );
    }

    #[test]
    fn shuffles_use_only_allowed_items_and_three_paints() {
        let mut draft = GarageDraft::of(PlayerId::new(1), None);
        for _ in 0..50 {
            draft.shuffle(CosmeticItem::is_starter);
            let look = draft.look;
            assert!(look.items().iter().all(|item| item.is_starter()));
            assert_ne!(look.primary, look.secondary);
            assert_ne!(look.trim, look.primary);
            assert_ne!(look.trim, look.secondary);
        }
    }
}
