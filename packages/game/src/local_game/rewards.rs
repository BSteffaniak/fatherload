//! Upgrade reward moments for local seats (WP-FEEL part 2): the world's `UpgradeInstalled` and
//! `MilestoneEarned` events become each seat's purchase moment, HUD toast and milestone banner
//! (see `shell::rewards`). The fanfares play from the same events through `audio::cues`.

use fatherload_simulation::{GameplayEventKind, PlayerId};

use super::LocalGame;
use crate::runtime::RuntimeEventKind;

impl LocalGame {
    /// Hands each local seat the upgrades its rig was fitted with and the milestones it
    /// earned this frame, then advances every seat's reward moments.
    pub(super) fn announce_rewards(&mut self, players: &[PlayerId]) {
        let world = self.runtime.world();
        let mut installs = Vec::new();
        let mut milestones = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::UpgradeInstalled {
                    player_id,
                    upgrade,
                    level,
                } => {
                    if let (Some(seat), Some(player)) = (
                        players.iter().position(|other| *other == player_id),
                        world.player(player_id),
                    ) {
                        installs.push((seat, player.clone(), upgrade, level));
                    }
                }
                GameplayEventKind::MilestoneEarned {
                    player_id,
                    milestone,
                } => {
                    if let Some(seat) = players.iter().position(|other| *other == player_id) {
                        milestones.push((seat, milestone));
                    }
                }
                _ => {}
            }
        }
        for (seat, player, upgrade, level) in installs {
            self.shell.upgrade_installed(seat, &player, upgrade, level);
        }
        for (seat, milestone) in milestones {
            self.shell.milestone_earned(seat, milestone);
        }
        self.shell.advance_rewards();
    }
}
