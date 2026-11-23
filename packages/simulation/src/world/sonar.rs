//! The Sonar Suite (Wave 9 integration, expansion plan §14): one sonar for the Drowned and
//! Frozen's water (Descent IV, [`crate::drowned`]) and the Echo Caves' dark (Descent VI,
//! [`crate::far_strange`]).
//!
//! A scanner press is the sonar's ping wherever sonar works ([`WorldState::sonar_medium`]):
//! under water or anywhere in Descent IV the ping lights the water round the rig and maps the
//! tiles in its reach; in the Echo Caves it sends a wave that lights the cave as it passes and
//! that blind hunters hear. The Leviathan's Sonar Suite (boss-won, never sold) reaches further
//! in both ([`WorldState::sonar_reach_tiles`]). The renderer, the HUD and the creatures read one
//! light ([`WorldState::sonar_light_at`]): the brightest of every ping still sounding.
//!
//! Each medium keeps its own bookkeeping (`DrownedState::pings`, `FarStrange::pings`; both
//! shared, saved and replicated with their sections), so this module adds no state.

use super::WorldState;
use crate::{
    PlayerId,
    depth_progression::RigSystem,
    event::TickOutput,
    far_strange::{SONAR_PING_TILES, SONAR_SUITE_PING_TILES},
    terrain::TilePosition,
    workshop::DeepSupply,
};

/// Where a sonar ping travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SonarMedium {
    /// Water, or anywhere in the Drowned and Frozen: the ping lights the water round the rig.
    Water,
    /// The Echo Caves' total dark: a wave that lights the cave as it passes.
    EchoCaves,
}

impl WorldState {
    /// Whether `player_id`'s rig carries the Sonar Suite (the Leviathan's reward).
    #[must_use]
    pub fn has_sonar_suite(&self, player_id: PlayerId) -> bool {
        self.players
            .get(&player_id)
            .is_some_and(|player| player.hazard_gear.has_system(RigSystem::SonarSuite))
    }

    /// Where `player_id`'s sonar pings now, if anywhere: the Echo Caves first, then water (or
    /// any Descent IV stratum). `None` in plain rock and open air elsewhere.
    #[must_use]
    pub fn sonar_medium(&self, player_id: PlayerId) -> Option<SonarMedium> {
        let player = self.players.get(&player_id)?;
        if self.in_echo_caves(player.tile_in(self.frame())) {
            return Some(SonarMedium::EchoCaves);
        }
        self.drowned_sonar_here(player_id)
            .then_some(SonarMedium::Water)
    }

    /// How far `player_id`'s sonar reaches in `medium`, tiles (further with the Sonar Suite).
    #[must_use]
    pub fn sonar_reach_tiles(&self, player_id: PlayerId, medium: SonarMedium) -> i32 {
        match medium {
            SonarMedium::Water => self.sonar_reach(player_id),
            SonarMedium::EchoCaves => i32::from(if self.has_sonar_suite(player_id) {
                SONAR_SUITE_PING_TILES
            } else {
                SONAR_PING_TILES
            }),
        }
    }

    /// Sends a sonar ping from `player_id`'s rig into whatever medium it is in (a set piece, a
    /// tool or a test may call it; the scanner calls [`Self::sonar_scanner`]). Returns whether
    /// it pinged.
    pub fn sonar_ping(&mut self, player_id: PlayerId, output: &mut TickOutput) -> bool {
        match self.sonar_medium(player_id) {
            Some(SonarMedium::Water) => self.drowned_sonar_ping(player_id, output),
            Some(SonarMedium::EchoCaves) => self.echo_sonar_ping(player_id),
            None => false,
        }
    }

    /// The scanner's one sonar hook (an applied `UseScanner`): a ping in the Echo Caves, or in
    /// water at most once a second. In the Echo Caves a rig without the Sonar Suite spends an
    /// Echo Charge from its kit, if it holds one, on a ping as long as the Suite's.
    pub(super) fn sonar_scanner(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        match self.sonar_medium(player_id) {
            Some(SonarMedium::EchoCaves) => {
                let charged = !self.has_sonar_suite(player_id)
                    && self.take_supply(player_id, DeepSupply::EchoCharge);
                let _pinged = self.echo_sonar_ping_reaching(player_id, charged);
            }
            Some(SonarMedium::Water) => self.drowned_scanner(player_id, output),
            None => {}
        }
    }

    /// How brightly sonar lights local pixel `(x, y)` now (`0..=1`): the brightest of the
    /// water's pings and the Echo Caves' waves. What the renderer draws and the dark reads.
    #[must_use]
    pub fn sonar_light_at(&self, x: f32, y: f32) -> f32 {
        let tile = self.frame().tile_at(x, y);
        self.drowned_sonar_light_at(x, y)
            .max(self.echo_sonar_light_at(tile))
    }

    /// [`Self::sonar_light_at`] at a tile's centre.
    #[must_use]
    pub fn sonar_light_at_tile(&self, tile: TilePosition) -> f32 {
        let (x, y) = self.frame().tile_center(tile);
        self.sonar_light_at(x, y)
    }
}
