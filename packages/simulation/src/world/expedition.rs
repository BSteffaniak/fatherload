//! The Fathers' expedition in the tick (Wave 8 story, see [`crate::expedition`]).
//!
//! Canisters read in order (or sealed until their turn), echoes played as rigs reach their depths, and
//! anomaly rooms entered.

use crate::{
    GameplayEventKind, PlayerId, SimulationTick, TickOutput,
    descent::{Descent, DescentBoss},
    expedition::{
        ECHO_GAP_MILLIS, ECHOES, Echo, ExpeditionState, LOGS, anomaly, log_bounty,
        progress::{StoryEvent, boss_bit, descent_bit},
        sites,
    },
    terrain::TilePosition,
    worldgen::{WorldSettings, descent::DescentLayout},
};

use super::WorldState;

/// Where [`WorldState::note_descent_boss_beaten`] keeps a noted win in
/// [`ExpeditionState::bosses`]: the eight bosses' bits shifted up past the beaten bits.
const BOSS_NOTED_SHIFT: u16 = 8;

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

    /// Marks the codex's finds made elsewhere (tests, scenes and tools, before play): the
    /// Crust's strongbox entries `diary` ([`crate::crust::CRUST_LOGS`] indices) and the Old
    /// Deep's story logs `deep_logs` (`old_deep::LOG_*`) as read.
    #[doc(hidden)]
    pub fn set_codex_finds_for_tests(&mut self, diary: &[u8], deep_logs: &[u8]) {
        self.crust.logs.extend(diary.iter().copied());
        self.old_deep.logs_read.extend(deep_logs.iter().copied());
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
        self.track_descent_progress(&settings, &actors, output);
    }

    /// Records that `boss`, a Descent's gate boss, has been beaten (Wave 10 story seam).
    ///
    /// The bosses whose fights keep their own state are read every tick
    /// ([`Self::descent_boss_beaten`]); a fight without such a flag (the Gatekeeper Colossus
    /// and the Heart, built by the Ancients and Heart tracks) calls this when it is won, inside
    /// the tick. The next expedition pass raises the [`StoryEvent::BossBeaten`] event, and
    /// beating the Heart lights the cable (the ending). Idempotent.
    pub const fn note_descent_boss_beaten(&mut self, boss: DescentBoss) {
        if !self.terrain.settings().generator.has_descents() {
            return;
        }
        self.expedition.bosses |= boss_bit(boss) << BOSS_NOTED_SHIFT;
    }

    /// Whether `boss`'s own fight state says it is beaten (or it was noted through
    /// [`Self::note_descent_boss_beaten`]).
    #[must_use]
    pub fn descent_boss_beaten(&self, boss: DescentBoss) -> bool {
        let own = match boss {
            DescentBoss::TheSinkhole => self.crust.deep_claim,
            DescentBoss::StarCore => self.old_deep.core.phase == crate::old_deep::CorePhase::Beaten,
            DescentBoss::MycelialMind => self.living_dark.mind.beaten,
            DescentBoss::Leviathan => self.drowned.leviathan.beaten,
            DescentBoss::SunSerpent => self.hollow.serpent.beaten,
            DescentBoss::TheGlitch => self.far_strange.glitch.beaten,
            // Wave 10 integration: Descent VII's gate fight and the Heart's own state.
            DescentBoss::GatekeeperColossus => self.garrison.colossus.beaten,
            DescentBoss::TheHeart => self.heart.fight.beaten,
        };
        own || self.expedition.bosses & (boss_bit(boss) << BOSS_NOTED_SHIFT) != 0
    }

    /// The crew's progress through the Descent (Wave 10): Descents reached by an actor (the
    /// Descents above count as passed), gate bosses beaten, and the ending once the Heart is.
    fn track_descent_progress(
        &mut self,
        settings: &WorldSettings,
        actors: &[(PlayerId, TilePosition)],
        output: &mut TickOutput,
    ) {
        let ground = crate::descent::fields::cached_layout(settings).tops[0];
        for (player_id, tile) in actors {
            if tile.y <= ground + 2 {
                continue;
            }
            let index = crate::depth_progression::descent_index_at_row(settings, tile.y);
            let descent = Descent::from_index(usize::from(index));
            if self.expedition.reached(descent) {
                continue;
            }
            for passed in Descent::ALL.into_iter().take(usize::from(index) + 1) {
                self.expedition.descents |= descent_bit(passed);
            }
            output.push_event(GameplayEventKind::Story {
                what: StoryEvent::DescentReached(descent),
                player_id: Some(*player_id),
            });
        }
        for boss in DescentBoss::ALL {
            if self.expedition.beat(boss) || !self.descent_boss_beaten(boss) {
                continue;
            }
            self.expedition.bosses |= boss_bit(boss);
            output.push_event(GameplayEventKind::Story {
                what: StoryEvent::BossBeaten(boss),
                player_id: None,
            });
        }
        if !self.expedition.cable_lit && self.expedition.beat(DescentBoss::TheHeart) {
            self.expedition.cable_lit = true;
            output.push_event(GameplayEventKind::Story {
                what: StoryEvent::CableLit,
                player_id: actors.first().map(|(player_id, _)| *player_id),
            });
        }
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
            self.reach_expedition_canister(*player_id, site.log.saturating_add(1), output);
        }
    }

    /// `player_id` reached a canister holding log `number` (1-based): it opens if its turn has
    /// come (and any sealed after it open in turn), or stays sealed until then. The story's own
    /// camps call it, and so do the strata's caches that hold the same logs (Wave 8
    /// integration: the Old Deep's expedition camps).
    pub(super) fn reach_expedition_canister(
        &mut self,
        player_id: PlayerId,
        number: u8,
        output: &mut TickOutput,
    ) {
        if number <= self.expedition.found || usize::from(number) > LOGS.len() {
            return;
        }
        if number == self.expedition.found + 1 {
            self.read_expedition_log(player_id, number, output);
            // Canisters reached early open in turn, all keyed off the one just read.
            while self.expedition.sealed.remove(&(self.expedition.found + 1)) {
                self.read_expedition_log(player_id, self.expedition.found + 1, output);
            }
        } else if self.expedition.sealed.insert(number) {
            output.push_event(GameplayEventKind::ExpeditionLogSealed {
                player_id,
                log: number,
            });
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
