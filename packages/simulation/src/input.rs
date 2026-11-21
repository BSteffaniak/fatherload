//! Per-tick simulation inputs: quantized held controls plus one-shot actions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    command::{PlayerCommand, TickAction},
    ids::{InputSequence, PlayerId},
};

/// Held controls, quantized so every peer feeds the simulation identical values.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ControlState {
    horizontal_milli: i16,
    pub thrust: bool,
    pub drill_down: bool,
    /// The hardpoint weapon's trigger and aim.
    #[serde(default)]
    pub weapon: WeaponControl,
    /// The on-foot jump button (Part II F5). In the rig it does nothing; on foot `thrust`
    /// (up) also jumps where there is nothing to climb, so seats without a jump key can play.
    #[serde(default)]
    pub jump: bool,
}

/// A seat's weapon trigger and aim for one tick (held, so firing is predicted like movement).
///
/// The aim is a quantized direction (`i8` per axis, +y down): free aim from the mouse or the
/// right stick. With `auto_aim` (the split-screen arrow-keys seat) the simulation picks the
/// nearest target inside a cone around `aim` (the rig's facing) and fires along `aim` when
/// there is none.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WeaponControl {
    pub fire: bool,
    pub auto_aim: bool,
    pub aim_x: i8,
    pub aim_y: i8,
}

impl ControlState {
    const HORIZONTAL_SCALE: f32 = 1_000.0;

    #[must_use]
    pub fn new(horizontal: f32, thrust: bool, drill_down: bool) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped control input is deliberately quantized for deterministic simulation"
        )]
        let horizontal_milli =
            (horizontal.clamp(-1.0, 1.0) * Self::HORIZONTAL_SCALE).round() as i16;
        Self {
            horizontal_milli,
            thrust,
            drill_down,
            weapon: WeaponControl::default(),
            jump: false,
        }
    }

    /// The same controls with the on-foot jump button `jump`.
    #[must_use]
    pub const fn with_jump(mut self, jump: bool) -> Self {
        self.jump = jump;
        self
    }

    /// The same controls with `weapon` as the weapon trigger and aim.
    #[must_use]
    pub const fn with_weapon(mut self, weapon: WeaponControl) -> Self {
        self.weapon = weapon;
        self
    }

    #[must_use]
    pub fn horizontal(self) -> f32 {
        f32::from(self.horizontal_milli) / Self::HORIZONTAL_SCALE
    }
}

/// One player's input for one tick: held controls and one-shot actions.
///
/// A player without controls (absent from [`TickInputs`] or `controls: None`) is not moved this
/// tick.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlayerTickInput {
    pub controls: Option<ControlState>,
    pub actions: Vec<TickAction>,
}

/// Every player's input for one tick, in deterministic order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TickInputs {
    pub players: BTreeMap<PlayerId, PlayerTickInput>,
}

impl TickInputs {
    /// Adds `controls` for `player` (bots in the balance harness).
    #[must_use]
    pub fn hold_as(mut self, player: PlayerId, controls: ControlState) -> Self {
        self.players.entry(player).or_default().controls = Some(controls);
        self
    }

    /// Adds a one-shot `command` for `player`, sequenced after its previous actions (bots in
    /// the balance harness).
    pub fn act_as(&mut self, player: PlayerId, command: PlayerCommand) {
        let input = self.players.entry(player).or_default();
        let sequence = InputSequence::new(u32::try_from(input.actions.len()).unwrap_or(u32::MAX));
        input.actions.push(TickAction { sequence, command });
    }
}

#[cfg(test)]
impl TickInputs {
    /// Inputs holding `controls` for `player`.
    pub(crate) fn held(player: PlayerId, controls: ControlState) -> Self {
        Self::default().hold(player, controls)
    }

    /// Adds `controls` for `player`.
    pub(crate) fn hold(mut self, player: PlayerId, controls: ControlState) -> Self {
        self.players.entry(player).or_default().controls = Some(controls);
        self
    }

    /// Adds a one-shot `command` for `player`, sequenced after its previous actions.
    pub(crate) fn act(mut self, player: PlayerId, command: PlayerCommand) -> Self {
        let input = self.players.entry(player).or_default();
        let sequence = InputSequence::new(u32::try_from(input.actions.len()).unwrap_or(u32::MAX));
        input.actions.push(TickAction { sequence, command });
        self
    }
}
