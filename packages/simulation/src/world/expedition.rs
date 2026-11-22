//! The Fathers' expedition in the tick (Wave 8 story, see [`crate::expedition`]).
//!
//! Canisters read in order (or sealed until their turn), echoes played as rigs reach their depths, and
//! anomaly rooms entered.

use crate::{
    GameplayEventKind, PlayerId, SimulationTick, TickOutput,
    expedition::{
        ECHO_GAP_MILLIS, ECHOES, Echo, ExpeditionState, LOGS, anomaly, log_bounty, sites,
    },
    terrain::TilePosition,
    worldgen::{WorldSettings, descent::DescentLayout},
};

use super::WorldState;

/// The terrain row echo `echo` plays at in a world with `layout`.
#[must_use]
pub fn echo_row(layout: &DescentLayout, echo: &Echo) -> i32 {
    let (top, bottom) = layout.rows(echo.stratum);
    top + (bottom - top) * i32::from(echo.along) / 1_000
}

impl WorldState {
    /// What the crew has found of the Fathers' expedition.
    #[must_use]
    pub const fn expedition(&self) -> &ExpeditionState {
        &self.expedition
    }

    /// Replaces the expedition state (tests, scenes and tools, before play).
    #[doc(hidden)]
    pub fn set_expedition(&mut self, state: ExpeditionState) {
        self.expedition = state;
    }

    /// The terrain row the next unheard echo plays at, if any are left (Deep Descent worlds).
    #[must_use]
    pub fn next_echo_row(&self) -> Option<i32> {
        let settings = self.terrain.settings();
        if !settings.generator.has_descents() {
            return None;
        }
        let echo = ECHOES.get(usize::from(self.expedition.echoes))?;
        Some(echo_row(
            &crate::descent::fields::cached_layout(settings),
            echo,
        ))
    }

    /// The site of the next log to read, if any are left (Deep Descent worlds): the trail's
    /// lead for the journal.
    #[must_use]
    pub fn next_log_site(&self) -> Option<sites::ExpeditionSite> {
        let settings = self.terrain.settings();
        sites::sites(settings)
            .into_iter()
            .find(|site| site.log == self.expedition.found)
    }

    /// Canisters, echoes and anomaly rooms (Deep Descent worlds only).
    pub(super) fn advance_expedition(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let settings: WorldSettings = *self.terrain.settings();
        if !settings.generator.has_descents() {
            return;
        }
        let actors: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .filter_map(|id| self.actor_tile(*id).map(|tile| (*id, tile)))
            .collect();
        if actors.is_empty() {
            return;
        }
        self.read_canisters(&settings, &actors, output);
        self.play_echoes(&settings, &actors, next_tick, output);
        self.enter_anomalies(&settings, &actors, output);
    }

    /// Opens the canister an actor stands at if its turn has come, or seals it until then.
    fn read_canisters(
        &mut self,
        settings: &WorldSettings,
        actors: &[(PlayerId, TilePosition)],
        output: &mut TickOutput,
    ) {
        let sites = sites::sites(settings);
        for (player_id, tile) in actors {
            let Some(site) = sites.iter().find(|site| site.holds(*tile)) else {
                continue;
            };
            let number = site.log.saturating_add(1);
            if number <= self.expedition.found {
                continue;
            }
            if number == self.expedition.found + 1 {
                self.read_expedition_log(*player_id, number, output);
                // Canisters reached early open in turn, all keyed off the one just read.
                while self.expedition.sealed.remove(&(self.expedition.found + 1)) {
                    self.read_expedition_log(*player_id, self.expedition.found + 1, output);
                }
            } else if self.expedition.sealed.insert(number) {
                output.push_event(GameplayEventKind::ExpeditionLogSealed {
                    player_id: *player_id,
                    log: number,
                });
            }
        }
    }

    /// Reads log `number` (the next one) for the crew, paying its canister's credits to
    /// `player_id`.
    fn read_expedition_log(&mut self, player_id: PlayerId, number: u8, output: &mut TickOutput) {
        let Some(log) = LOGS.get(usize::from(number.saturating_sub(1))) else {
            return;
        };
        self.expedition.found = number;
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_add(log_bounty(log.descent()));
        }
        output.push_event(GameplayEventKind::ExpeditionLogFound {
            player_id,
            log: number,
        });
    }

    /// Plays the next echo once a rig reaches its depth (one at a time, [`ECHO_GAP_MILLIS`]
    /// apart).
    fn play_echoes(
        &mut self,
        settings: &WorldSettings,
        actors: &[(PlayerId, TilePosition)],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if next_tick.get() < self.expedition.next_echo_tick {
            return;
        }
        let Some(echo) = ECHOES.get(usize::from(self.expedition.echoes)) else {
            return;
        };
        let layout = crate::descent::fields::cached_layout(settings);
        let row = echo_row(&layout, echo);
        let Some((player_id, _)) = actors
            .iter()
            .find(|(_, tile)| tile.y >= row && tile.y <= layout.floor)
        else {
            return;
        };
        self.expedition.echoes += 1;
        self.expedition.next_echo_tick = next_tick
            .get()
            .saturating_add(self.timing.ticks_for_millis(ECHO_GAP_MILLIS));
        output.push_event(GameplayEventKind::ExpeditionEcho {
            player_id: *player_id,
            echo: self.expedition.echoes,
        });
    }

    /// Records the anomaly rooms actors stand in.
    fn enter_anomalies(
        &mut self,
        settings: &WorldSettings,
        actors: &[(PlayerId, TilePosition)],
        output: &mut TickOutput,
    ) {
        for (player_id, tile) in actors {
            let Some(room) = anomaly::room_holding(settings, *tile) else {
                continue;
            };
            if !self.expedition.anomalies.insert(room.region) {
                continue;
            }
            let bit = room.kind.bit();
            let first_of_kind = self.expedition.anomaly_kinds & bit == 0;
            self.expedition.anomaly_kinds |= bit;
            output.push_event(GameplayEventKind::AnomalyEntered {
                player_id: *player_id,
                kind: room.kind,
                first_of_kind,
            });
        }
    }
}
