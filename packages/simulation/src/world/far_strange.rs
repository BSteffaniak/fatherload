//! The far Strange Layers in the world (Wave 9 strange-b, Descent VI strata 4-6 and its gate;
//! the rules live in [`crate::far_strange`]).
//!
//! [`WorldState::advance_far_strange`] runs once per tick after the Living Dark's pass and
//! before the fluid pass. On worlds without Descents it returns at once. In order:
//!
//! 1. **Pings**: new sonar pings are announced, spent ones forgotten.
//! 2. **Amber**: rigs whose amber has worn off go free.
//! 3. **Keyed walls**: a filtered lamp opens the walls of its colour in reach and sight (they
//!    stay open while the light is on them); walls whose light left close again, never on a
//!    body ([`PRISM_CLOSE_MILLIS`]).
//! 4. **Echo quartz**: a rig striking a quartz pillar rings it (a free ping round it).
//! 5. **Time shifts** (a rig entering a slow pool or a fast lane) and **landmarks found**.
//! 6. **Creatures** and **The Glitch** (`world/enemies/far_strange.rs`).
//!
//! Hooks elsewhere: the scanner pings in the Echo Caves ([`WorldState::sonar_ping`], from
//! `PlayerCommand::UseScanner`), the drill refuses builder alloy without the Phase Drill
//! ([`WorldState::alloy_refuses`]), an ambered rig's movement runs slow
//! ([`WorldState::rig_ambered`]), time bubbles are charges (`world/bombs.rs`).

use crate::{
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::Stratum,
    enemy::EnemyKind,
    event::{GameplayEventKind, TickOutput},
    far_strange::{
        AMBER_MILLIS, FILTER_SWAP_REACH_TILES, FarLandmark, FarSchedule, FarStrange,
        FarStrangeAction, FarStrangeEvent, FarStrangeRefusal, HUSH_RADIUS_TILES, HUSHED_PING_SHARE,
        LampFilter, MAX_OPEN_KEYED, MAX_SONAR_PINGS, PING_NOISE_MILLIS, PRISM_CLOSE_MILLIS,
        PRISM_KEY_REACH_TILES, PRISM_RETRY_MILLIS, QUARTZ_PING_TILES, QUARTZ_RING_MILLIS,
        QUARTZ_RING_SPEED, SONAR_PING_TILES, SONAR_SUITE_PING_TILES, SonarPing, TIME_BUBBLE_PACK,
        rig_noise_tiles, sonar_alive, sonar_reveal,
    },
    ids::{PlayerId, SimulationTick},
    logistics::scaled_price,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::far_strange::{self as sites, FarStampKind},
    },
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Most keyed wall tiles that close in one tick.
const CLOSE_BUDGET: usize = 64;

/// A far Strange Layers event at `tile`.
pub(super) const fn far_event(
    what: FarStrangeEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::FarStrange {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The far Strange Layers' shared state.
    #[must_use]
    pub const fn far_strange(&self) -> &FarStrange {
        &self.far_strange
    }

    /// The far Strange Layers' host bookkeeping (tests, tools).
    #[must_use]
    pub const fn far_schedule(&self) -> &FarSchedule {
        &self.far_schedule
    }

    /// The Rewrite Hall, The Glitch's arena (Deep Descent worlds).
    #[must_use]
    pub fn glitch_arena(&self) -> Option<TileRect> {
        sites::glitch_arena(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// `player_id`'s lamp filter.
    #[must_use]
    pub fn lamp_filter_of(&self, player_id: PlayerId) -> LampFilter {
        self.far_strange.filter_of(player_id)
    }

    /// Whether `player_id`'s rig is held in amber now (its motion and fuel burn run slow).
    #[must_use]
    pub fn rig_ambered(&self, player_id: PlayerId) -> bool {
        self.far_strange.is_ambered(player_id, self.simulation_tick)
    }

    /// How lit `tile` is by the Echo Caves' sonar waves now (`0..=1`);
    /// [`WorldState::sonar_light_at`] adds the water's pings.
    #[must_use]
    pub fn echo_sonar_light_at(&self, tile: TilePosition) -> f32 {
        let now = self.millis_at(self.simulation_tick);
        self.far_strange
            .pings
            .iter()
            .map(|ping| {
                let since = now.saturating_sub(self.millis_at(ping.tick));
                sonar_reveal(ping, since, tile.x, tile.y)
            })
            .fold(0.0_f32, f32::max)
    }

    /// Whether `tile` lies in the Echo Caves (total darkness, sonar shows the way).
    #[must_use]
    pub fn in_echo_caves(&self, tile: TilePosition) -> bool {
        self.deep_stratum_at(tile) == Some(Stratum::EchoCaves)
    }

    /// The far landmark holding `tile`, if any (the game names a `LandmarkFound` event's find
    /// with it).
    #[must_use]
    pub fn far_landmark_at(&self, tile: TilePosition) -> Option<FarLandmark> {
        let layout = self.descent_strata_layout()?;
        sites::stamps_with(
            self.terrain.settings(),
            &layout,
            &TileRect::around(tile, 1, 1),
        )
        .into_iter()
        .find_map(|stamp| match stamp.kind {
            FarStampKind::ClockHall => Some(FarLandmark::ClockHall),
            FarStampKind::Choir => Some(FarLandmark::Choir),
            FarStampKind::SpectrumVault => Some(FarLandmark::SpectrumVault),
            FarStampKind::GlitchArena => Some(FarLandmark::GlitchArena),
            _ => None,
        })
    }

    /// Whether the use button beside `player_id`'s rig swaps its lamp filter: a keyed wall (or
    /// one it opened) within [`FILTER_SWAP_REACH_TILES`], the rig not on foot.
    #[must_use]
    pub fn filter_swap_here(&self, player_id: PlayerId) -> bool {
        if self.is_on_foot(player_id) {
            return false;
        }
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let centre = player.tile_in(self.frame());
        if self.deep_stratum_at(centre) != Some(Stratum::PrismDesert) {
            return false;
        }
        let reach = FILTER_SWAP_REACH_TILES;
        let open =
            self.far_schedule.open_keyed.keys().any(|tile| {
                (tile.x - centre.x).abs() <= reach && (tile.y - centre.y).abs() <= reach
            });
        open || (-reach..=reach).any(|dy| {
            (-reach..=reach).any(|dx| {
                self.terrain
                    .tile(TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    })
                    .is_some_and(
                        |tile| matches!(tile.kind, TileKind::Cave(cave) if cave.is_keyed_wall()),
                    )
            })
        })
    }

    /// How far a blind hunter hears `player_id`'s rig (tiles): nothing while it rests or
    /// coasts, more under thrust and while drilling, far after a ping; nothing at all inside
    /// a hush moth's hush.
    #[must_use]
    pub fn rig_noise(&self, player_id: PlayerId) -> f32 {
        let Some(player) = self.players.get(&player_id) else {
            return 0.0;
        };
        if self.hushed(player.x, player.y) {
            return 0.0;
        }
        let speed = player.velocity_x.hypot(player.velocity_y);
        let thrusting = self.far_schedule.thrusting.contains(&player_id) || speed > 150.0;
        let drilling = self.active_drills.contains_key(&player_id);
        let pinged = self
            .far_schedule
            .pinged
            .get(&player_id)
            .is_some_and(|tick| {
                self.simulation_tick.get().saturating_sub(tick.get())
                    <= self.timing.ticks_for_millis(PING_NOISE_MILLIS)
            });
        rig_noise_tiles(speed, thrusting, drilling, pinged)
    }

    /// Whether local point `(x, y)` is inside a hush moth's hush.
    #[must_use]
    pub fn hushed(&self, x: f32, y: f32) -> bool {
        let reach = HUSH_RADIUS_TILES * TILE_SIZE;
        self.enemies().any(|(entity, state)| {
            state.kind == EnemyKind::HushMoth
                && (entity.body.x - x).hypot(entity.body.y - y) <= reach
        })
    }

    // --- Hooks --------------------------------------------------------------------------------

    /// Whether builder alloy refuses `player_id`'s drill on a `kind` tile (it yields only to
    /// the Phase Drill, Descent VI's reward).
    #[must_use]
    pub(super) fn alloy_refuses(&self, player_id: PlayerId, kind: TileKind) -> bool {
        kind == TileKind::Cave(CaveTile::BuilderAlloy)
            && !self
                .players
                .get(&player_id)
                .is_some_and(|player| player.hazard_gear.has_system(RigSystem::PhaseDrill))
    }

    /// A scanner press in the Echo Caves is the sonar's ping (through
    /// [`WorldState::sonar_scanner`], `world/sonar.rs`): its wave lights the cave for a
    /// moment ([`SONAR_PING_TILES`], [`SONAR_SUITE_PING_TILES`] with the Sonar Suite; a third
    /// as far inside a hush moth's hush). Blind hunters hear it. Returns whether it pinged.
    pub(super) fn echo_sonar_ping(&mut self, player_id: PlayerId) -> bool {
        self.echo_sonar_ping_reaching(player_id, false)
    }

    /// [`Self::echo_sonar_ping`], as far as the Sonar Suite's if `charged` (an Echo Charge).
    pub(super) fn echo_sonar_ping_reaching(&mut self, player_id: PlayerId, charged: bool) -> bool {
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let tile = player.tile_in(self.frame());
        if !self.in_echo_caves(tile) {
            return false;
        }
        let suite = charged || player.hazard_gear.has_system(RigSystem::SonarSuite);
        let mut reach = f32::from(if suite {
            SONAR_SUITE_PING_TILES
        } else {
            SONAR_PING_TILES
        });
        if self.hushed(player.x, player.y) {
            reach *= HUSHED_PING_SHARE;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a reach of a few dozen tiles"
        )]
        let reach = reach.round().max(2.0) as u8;
        self.push_ping(Some(player_id), tile, reach);
        self.far_schedule
            .pinged
            .insert(player_id, self.simulation_tick);
        true
    }

    /// Starts a ping at `tile` reaching `reach` tiles (players, quartz, shriekers).
    pub(super) fn push_ping(&mut self, player: Option<PlayerId>, tile: TilePosition, reach: u8) {
        let pings = &mut self.far_strange.pings;
        while pings.len() >= MAX_SONAR_PINGS {
            pings.remove(0);
        }
        pings.push(SonarPing {
            player,
            x: tile.x,
            y: tile.y,
            tick: self.simulation_tick,
            reach,
        });
    }

    /// An amber sloth's touch: `player_id`'s rig is held in amber for [`AMBER_MILLIS`].
    pub(super) fn amber_rig(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let until = self.after_millis(self.simulation_tick, AMBER_MILLIS);
        self.far_strange.ambered.insert(player_id, until);
        if let Some(player) = self.players.get(&player_id) {
            let tile = player.tile_in(self.frame());
            output.push_event(far_event(FarStrangeEvent::Ambered, Some(player_id), tile));
        }
    }

    // --- Commands -----------------------------------------------------------------------------

    /// Whether `player_id` (its rig on `tile`) may do `action`, and its price in credits.
    pub(super) fn check_far_strange(
        &self,
        player_id: PlayerId,
        tile: TilePosition,
        action: FarStrangeAction,
    ) -> Result<u32, FarStrangeRefusal> {
        if self.descent_strata_layout().is_none() {
            return Err(FarStrangeRefusal::NoFilters);
        }
        match action {
            FarStrangeAction::CycleFilter => {
                if self.is_on_foot(player_id) {
                    return Err(FarStrangeRefusal::OnFoot);
                }
                self.check_filter_glass(player_id, self.lamp_filter_of(player_id).next())?;
                Ok(0)
            }
            FarStrangeAction::SetFilter(filter) => {
                if self.is_on_foot(player_id) {
                    return Err(FarStrangeRefusal::OnFoot);
                }
                if self.lamp_filter_of(player_id) == filter {
                    return Err(FarStrangeRefusal::SameFilter);
                }
                self.check_filter_glass(player_id, filter)?;
                Ok(0)
            }
            FarStrangeAction::BuyTimeBubbles => {
                if self.logistics.outpost_at(tile).is_none() && !self.in_town(tile) {
                    return Err(FarStrangeRefusal::NoBubbleSeller);
                }
                Ok(scaled_price(
                    TIME_BUBBLE_PACK.1,
                    self.descent_of_row(tile.y),
                ))
            }
        }
    }

    /// Resolves a far Strange Layers command inside the tick (already checked and charged).
    pub(super) fn apply_far_strange(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        action: FarStrangeAction,
        output: &mut TickOutput,
    ) {
        let filter = match action {
            FarStrangeAction::CycleFilter => self.lamp_filter_of(player_id).next(),
            FarStrangeAction::SetFilter(filter) => filter,
            FarStrangeAction::BuyTimeBubbles => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.add_explosives(
                        crate::explosive::ExplosiveKind::TimeBubble,
                        TIME_BUBBLE_PACK.0,
                    );
                }
                output.push_event(far_event(FarStrangeEvent::Bought, Some(player_id), tile));
                return;
            }
        };
        if filter != LampFilter::White && !self.fit_filter_glass(player_id, tile, output) {
            return;
        }
        if filter == LampFilter::White {
            self.far_strange.filters.remove(&player_id);
        } else {
            self.far_strange.filters.insert(player_id, filter);
        }
        output.push_event(far_event(
            FarStrangeEvent::FilterSwapped,
            Some(player_id),
            tile,
        ));
    }

    // --- Staging (tests, scenes and dev tools) ----------------------------------------------

    /// Fits `filter` to `player_id`'s lamp before play (scenes, tests).
    pub fn set_lamp_filter(&mut self, player_id: PlayerId, filter: LampFilter) {
        if filter == LampFilter::White {
            self.far_strange.filters.remove(&player_id);
        } else {
            self.far_strange.filters.insert(player_id, filter);
        }
    }

    /// Starts a sonar ping at `tile` before play (scenes, tests).
    pub fn stage_sonar_ping(&mut self, player: Option<PlayerId>, tile: TilePosition, reach: u8) {
        self.push_ping(player, tile, reach);
    }

    /// Replaces the far Strange Layers' shared state (scenes, tests, tools).
    pub fn set_far_strange(&mut self, state: FarStrange) {
        self.far_strange = state;
    }

    /// Fires an enemy shot of `kind` from local point `from` at velocity `velocity` (tests).
    #[cfg(test)]
    pub(super) fn spawn_enemy_shot_for_tests(
        &mut self,
        kind: crate::enemy::ShotKind,
        from: (f32, f32),
        velocity: (f32, f32),
    ) -> crate::entity::EntityId {
        let expires = self.after_millis(self.simulation_tick, kind.life_millis());
        self.spawn_entity(
            crate::entity::Body::at(from.0, from.1).moving(velocity.0, velocity.1),
            crate::entity::EntityState::EnemyShot(crate::enemy::EnemyShotState {
                kind,
                damage: kind.damage(),
                shooter: EnemyKind::SplitterSpire,
            }),
            Some(expires),
        )
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of the far Strange Layers (see the module docs).
    pub(super) fn advance_far_strange(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.descent_strata_layout().is_none() {
            return;
        }
        self.advance_pings(output);
        let now = self.simulation_tick;
        self.far_strange.ambered.retain(|_, until| *until > now);
        self.advance_keyed_walls(next_tick, output);
        self.ring_quartz(output);
        self.note_time_shifts(output);
        self.note_far_landmarks(output);
        self.advance_far_creatures(next_tick, output);
        self.advance_glitch(next_tick, output);
    }

    /// Announces new pings and forgets spent ones (step 1).
    fn advance_pings(&mut self, output: &mut TickOutput) {
        if self.far_strange.pings.is_empty() {
            return;
        }
        let now = self.simulation_tick;
        let now_millis = self.millis_at(now);
        let fresh: Vec<SonarPing> = self
            .far_strange
            .pings
            .iter()
            .filter(|ping| ping.tick == now)
            .copied()
            .collect();
        for ping in fresh {
            output.push_event(far_event(
                FarStrangeEvent::SonarPing,
                ping.player,
                TilePosition {
                    x: ping.x,
                    y: ping.y,
                },
            ));
        }
        let millis: Vec<u64> = self
            .far_strange
            .pings
            .iter()
            .map(|ping| now_millis.saturating_sub(self.millis_at(ping.tick)))
            .collect();
        let mut index = 0;
        self.far_strange.pings.retain(|ping| {
            let alive = sonar_alive(ping, millis[index]);
            index += 1;
            alive
        });
        let horizon = self.timing.ticks_for_millis(PING_NOISE_MILLIS);
        self.far_schedule
            .pinged
            .retain(|_, tick| now.get().saturating_sub(tick.get()) <= horizon);
    }

    /// Opens keyed walls in a filtered lamp's light and closes those the light left (step 3).
    fn advance_keyed_walls(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let (top, bottom) = layout.rows(Stratum::PrismDesert);
        let frame = self.frame();
        let lamps: Vec<(PlayerId, LampFilter, f32, f32)> = self
            .players
            .iter()
            .filter(|(id, player)| player.hull > 0.0 && !self.is_on_foot(**id))
            .filter_map(|(id, player)| {
                let filter = self.far_strange.filter_of(*id);
                let tile = player.tile_in(frame);
                (filter != LampFilter::White && tile.y >= top - 30 && tile.y <= bottom + 30)
                    .then_some((*id, filter, player.x, player.y))
            })
            .collect();
        let close_at = self.after_millis(next_tick, PRISM_CLOSE_MILLIS);
        for (player_id, filter, x, y) in lamps {
            let Some(wall) = filter.wall() else {
                continue;
            };
            let centre = frame.tile_at(x, y);
            let mut opened = false;
            let reach = PRISM_KEY_REACH_TILES;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    if dx * dx + dy * dy > reach * reach {
                        continue;
                    }
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    let here = self.terrain.tile(at).map(|tile| tile.kind);
                    let open_ours = self
                        .far_schedule
                        .open_keyed
                        .get(&at)
                        .is_some_and(|(colour, _)| *colour == filter);
                    let closed_ours = here == Some(TileKind::Cave(wall));
                    if !(open_ours || closed_ours) || !self.lit_through(x, y, at, wall) {
                        continue;
                    }
                    if closed_ours && self.terrain.set_kind_by_rule(at, TileKind::Air) {
                        opened = true;
                    }
                    self.far_schedule.open_keyed.insert(at, (filter, close_at));
                }
            }
            if opened {
                output.push_event(far_event(
                    FarStrangeEvent::KeyedOpened,
                    Some(player_id),
                    centre,
                ));
            }
        }
        while self.far_schedule.open_keyed.len() > MAX_OPEN_KEYED {
            let Some(oldest) = self
                .far_schedule
                .open_keyed
                .iter()
                .min_by_key(|(tile, (_, due))| (*due, **tile))
                .map(|(tile, _)| *tile)
            else {
                break;
            };
            self.close_keyed(oldest, next_tick, true);
        }
        let due: Vec<TilePosition> = self
            .far_schedule
            .open_keyed
            .iter()
            .filter(|(_, (_, due))| *due <= next_tick)
            .map(|(tile, _)| *tile)
            .take(CLOSE_BUDGET)
            .collect();
        for tile in due {
            self.close_keyed(tile, next_tick, false);
        }
    }

    /// Closes the keyed wall at `tile` (unless a body is in it: then it tries again soon, or,
    /// `forced`, it is forgotten open).
    fn close_keyed(&mut self, tile: TilePosition, now: SimulationTick, forced: bool) {
        let Some((colour, _)) = self.far_schedule.open_keyed.get(&tile).copied() else {
            return;
        };
        let Some(wall) = colour.wall() else {
            self.far_schedule.open_keyed.remove(&tile);
            return;
        };
        let open = self
            .terrain
            .tile(tile)
            .is_some_and(|found| found.kind.is_open());
        if !open {
            // Filled some other way (a block placed): forget it.
            self.far_schedule.open_keyed.remove(&tile);
            return;
        }
        if self.tile_occupied(tile) {
            if forced {
                self.far_schedule.open_keyed.remove(&tile);
            } else {
                let retry = self.after_millis(now, PRISM_RETRY_MILLIS);
                self.far_schedule.open_keyed.insert(tile, (colour, retry));
            }
            return;
        }
        self.far_schedule.open_keyed.remove(&tile);
        let _ = self.terrain.set_kind_by_rule(tile, TileKind::Cave(wall));
    }

    /// Whether light from local point `(x, y)` reaches `target`: nothing solid on the way but
    /// keyed walls of the light's own colour (coloured glass lets its colour through).
    fn lit_through(&self, x: f32, y: f32, target: TilePosition, wall: CaveTile) -> bool {
        let frame = self.frame();
        let (tx, ty) = frame.tile_center(target);
        let (dx, dy) = (tx - x, ty - y);
        let distance = dx.hypot(dy);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few dozen steps"
        )]
        let steps = (distance / (TILE_SIZE * 0.5)).ceil().max(1.0) as u32;
        for step in 1..steps {
            #[allow(clippy::cast_precision_loss, reason = "a few dozen steps")]
            let t = step as f32 / steps as f32;
            let at = frame.tile_at(dx.mul_add(t, x), dy.mul_add(t, y));
            if at == target {
                return true;
            }
            let Some(tile) = self.terrain.tile(at) else {
                return false;
            };
            if tile.kind.is_open() || tile.kind == TileKind::Cave(wall) {
                continue;
            }
            return false;
        }
        true
    }

    /// Rigs striking echo quartz ring it: a ping round the struck pillar (step 4).
    fn ring_quartz(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let now = self.simulation_tick;
        let wait = self.timing.ticks_for_millis(QUARTZ_RING_MILLIS);
        let mut rung: Vec<(PlayerId, TilePosition)> = Vec::new();
        let speeds: std::collections::BTreeMap<PlayerId, f32> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.velocity_x.hypot(player.velocity_y)))
            .collect();
        for (player_id, player) in &self.players {
            let before = self
                .far_schedule
                .speeds
                .get(player_id)
                .copied()
                .unwrap_or(0.0);
            if player.velocity_x.hypot(player.velocity_y).max(before) < QUARTZ_RING_SPEED {
                continue;
            }
            if self
                .far_schedule
                .rung
                .get(player_id)
                .is_some_and(|tick| now.get().saturating_sub(tick.get()) < wait)
            {
                continue;
            }
            let centre = player.tile_in(frame);
            if !self.in_echo_caves(centre) {
                continue;
            }
            let reach = AUTHORITATIVE_PLAYER_RADIUS + 3.0;
            let struck = (-1..=1).find_map(|dy| {
                (-1..=1).find_map(|dx| {
                    let at = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    if self.terrain.tile(at).map(|tile| tile.kind)
                        != Some(TileKind::Cave(CaveTile::EchoQuartz))
                    {
                        return None;
                    }
                    let (left, top) = (frame.col_px(at.x), frame.row_px(at.y));
                    let near_x = player.x.clamp(left, left + TILE_SIZE);
                    let near_y = player.y.clamp(top, top + TILE_SIZE);
                    ((player.x - near_x).hypot(player.y - near_y) <= reach).then_some(at)
                })
            });
            if let Some(at) = struck {
                rung.push((*player_id, at));
            }
        }
        // Drilling quartz rings it too (the opening is in this tick's journal).
        let drilled: Vec<TilePosition> = self
            .terrain
            .journal_with_before()
            .filter(|(change, before)| {
                *before == TileKind::Cave(CaveTile::EchoQuartz) && change.tile.kind.is_open()
            })
            .map(|(change, _)| change.position)
            .take(2)
            .collect();
        self.far_schedule.speeds = speeds;
        for (player_id, at) in rung {
            self.far_schedule.rung.insert(player_id, now);
            self.push_ping(None, at, QUARTZ_PING_TILES);
            output.push_event(far_event(FarStrangeEvent::QuartzRang, Some(player_id), at));
        }
        for at in drilled {
            self.push_ping(None, at, QUARTZ_PING_TILES / 2);
            output.push_event(far_event(FarStrangeEvent::QuartzRang, None, at));
        }
    }

    /// A rig crossing into a slow pool or a fast lane is announced once (step 5).
    fn note_time_shifts(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let (top, bottom) = layout.rows(Stratum::TimeSludge);
        let frame = self.frame();
        let mut shifts: Vec<(PlayerId, i8, TilePosition)> = Vec::new();
        for (player_id, player) in &self.players {
            let tile = player.tile_in(frame);
            let pocket = if tile.y < top - 4 || tile.y > bottom + 4 {
                0
            } else {
                let scale = self.fields_at(player.x, player.y).time_scale;
                if scale < 0.8 {
                    -1
                } else {
                    i8::from(scale > 1.15)
                }
            };
            let before = self
                .far_schedule
                .pockets
                .get(player_id)
                .copied()
                .unwrap_or(0);
            if pocket != before {
                shifts.push((*player_id, pocket, tile));
            }
        }
        for (player_id, pocket, tile) in shifts {
            if pocket == 0 {
                self.far_schedule.pockets.remove(&player_id);
            } else {
                self.far_schedule.pockets.insert(player_id, pocket);
                output.push_event(far_event(FarStrangeEvent::TimeShift, Some(player_id), tile));
            }
        }
    }

    /// A rig reaching one of the far landmarks finds it: its story beat is told once (step 5).
    fn note_far_landmarks(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let settings = *self.terrain.settings();
        let (top, bottom) = sites::carve_rows(&layout);
        let frame = self.frame();
        let mut finds: Vec<(FarLandmark, PlayerId, TilePosition)> = Vec::new();
        for (player_id, player) in &self.players {
            let tile = player.tile_in(frame);
            if tile.y < top || tile.y > bottom {
                continue;
            }
            for stamp in sites::stamps_with(&settings, &layout, &TileRect::around(tile, 1, 1)) {
                let landmark = match stamp.kind {
                    FarStampKind::ClockHall => FarLandmark::ClockHall,
                    FarStampKind::Choir => FarLandmark::Choir,
                    FarStampKind::SpectrumVault => FarLandmark::SpectrumVault,
                    FarStampKind::GlitchArena
                        if sites::arena_inside(&stamp.bounds).contains(tile) =>
                    {
                        FarLandmark::GlitchArena
                    }
                    _ => continue,
                };
                if !self.far_strange.found.contains(&landmark)
                    && !finds.iter().any(|(found, ..)| *found == landmark)
                {
                    finds.push((landmark, *player_id, tile));
                }
            }
        }
        for (landmark, player_id, tile) in finds {
            self.far_strange.found.insert(landmark);
            output.push_event(far_event(
                FarStrangeEvent::LandmarkFound,
                Some(player_id),
                tile,
            ));
        }
    }
}
