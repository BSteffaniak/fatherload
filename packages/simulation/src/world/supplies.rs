//! Deep supplies at work (Wave 10 polish; the table is on [`crate::workshop::SupplyEffects`]).
//!
//! Wave 9 sold eight deep supplies; three were spent where they bite (the Frost Heater, the
//! Shade Canopy, the Echo Charge). This pass gives the other five an effect each, spent
//! automatically from the kit (no new input: every seat, online and by touch):
//!
//! * **Ballast Tank**: a submerged rig at a third of its hull, or cracking under the abyss's
//!   pressure, blows one: [`BALLAST_BLOW_MILLIS`] of full lift that no current, whirlpool or
//!   lure holds ([`WorldState::apply_submarine`] reads [`WorldState::ballast_blowing`]).
//! * **Flare Buoy**: a lure angler's light catching a rig drops one where the rig is: it lights
//!   the black water and dazzles every angler in its light ([`WorldState::flare_dazzles`]: they
//!   drag nobody and notice nobody) for [`FLARE_MILLIS`].
//! * **Beast Bait**: a hunting predator of the Hollow World noticing a rig gets bait thrown past
//!   it: every hunting predator within [`BAIT_REACH_TILES`] gives up the rig and goes to the bait
//!   ([`WorldState::supply_notice`]) for [`BAIT_MILLIS`].
//! * **Time-Bubble Bomb**: the far track's time bubble under its supply name: bought or crafted
//!   it becomes an [`ExplosiveKind::TimeBubble`] charge (one item, two counters).
//! * **Lamp Filter**: the coloured glass a tinted lamp needs (`world/far_strange.rs` asks
//!   [`WorldState::check_filter_glass`]); a set lasts [`LAMP_FILTER_MILLIS`], the next set is
//!   fitted from the kit as one wears out, and without one the lamp goes white.
//!
//! All timing is world milliseconds turned into ticks ([`WorldState::after_millis`]), so every
//! tick rate agrees; positions are absolute tiles.

use super::WorldState;
use crate::{
    PlayerId,
    enemy::{EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::ExplosiveKind,
    far_strange::{FarStrangeRefusal, LampFilter},
    fluid::SUBMARINE_SHARE,
    ids::SimulationTick,
    terrain::{TILE_SIZE, TilePosition},
    workshop::{
        BAIT_MILLIS, BAIT_REACH_TILES, BAIT_THROW_TILES, BALLAST_BLOW_HULL_SHARE,
        BALLAST_BLOW_MILLIS, BaitedHome, DeepSupply, FLARE_MILLIS, FLARE_REACH_TILES,
        LAMP_FILTER_MILLIS, MAX_SUPPLY_LURES, MAX_SUPPLY_STACK, SupplyEffects, SupplyEvent,
        SupplyLure,
    },
};

/// A supply event at `tile` for `player_id`.
const fn supply_event(
    what: SupplyEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Supply {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

/// Whether `kind` is a predator of the Hollow World that Beast Bait draws off.
#[must_use]
pub const fn bait_draws(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::Fangrunner
            | EnemyKind::StoopRoc
            | EnemyKind::Hangclaw
            | EnemyKind::ShadeStalker
            | EnemyKind::CinderDrake
    )
}

/// Whether `mode` is a creature hunting (chasing or attacking).
const fn hunting(mode: EnemyMode) -> bool {
    matches!(
        mode,
        EnemyMode::Chase | EnemyMode::Windup | EnemyMode::Strike | EnemyMode::Recover
    )
}

impl WorldState {
    /// What the spent deep supplies are still doing (the renderer draws flares and bait).
    #[must_use]
    pub const fn supply_effects(&self) -> &SupplyEffects {
        &self.logistics.supply_effects
    }

    /// Whether `player_id`'s Ballast Tank is blowing now.
    #[must_use]
    pub fn ballast_blowing(&self, player_id: PlayerId) -> bool {
        self.logistics
            .supply_effects
            .blowing(player_id, self.simulation_tick)
    }

    /// Whether a burning Flare Buoy lights (and dazzles anglers at) local pixel `(x, y)`.
    #[must_use]
    pub fn flare_dazzles(&self, x: f32, y: f32) -> bool {
        let reach = FLARE_REACH_TILES * TILE_SIZE;
        let frame = self.frame();
        self.logistics
            .supply_effects
            .lures_of(DeepSupply::FlareBuoy)
            .any(|flare| {
                let (fx, fy) = frame.tile_center(flare.at);
                (fx - x).hypot(fy - y) <= reach
            })
    }

    /// The bait whose scent holds a predator at local pixel `(x, y)`, if any.
    fn bait_near(&self, x: f32, y: f32) -> Option<TilePosition> {
        let reach = BAIT_REACH_TILES * TILE_SIZE;
        let frame = self.frame();
        self.logistics
            .supply_effects
            .lures_of(DeepSupply::BeastBait)
            .find(|bait| {
                let (bx, by) = frame.tile_center(bait.at);
                (bx - x).hypot(by - y) <= reach
            })
            .map(|bait| bait.at)
    }

    /// The supplies' say on who a creature notices (`Some(None)`: nobody): an angler dazzled by
    /// a flare, a predator on the scent of bait. `None` leaves the other hooks to decide.
    #[allow(
        clippy::option_option,
        reason = "None leaves the other hooks; Some(None) notices nobody"
    )]
    pub(super) fn supply_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        if self.logistics.supply_effects.lures.is_empty() {
            return None;
        }
        let held = (state.kind == EnemyKind::LureAngler && self.flare_dazzles(body.x, body.y))
            || (bait_draws(state.kind) && self.bait_near(body.x, body.y).is_some());
        held.then_some(None)
    }

    // --- The kit, with the time bubble folded in ------------------------------------------

    /// How many of `supply` `player_id` carries: the kit's count, or for the Time-Bubble Bomb
    /// the rig's time-bubble charges (one item: the far track's bubble is the supply).
    #[must_use]
    pub fn supplies_of(&self, player_id: PlayerId, supply: DeepSupply) -> u8 {
        if supply == DeepSupply::TimeBubbleBomb {
            return self.players.get(&player_id).map_or(0, |player| {
                u8::try_from(player.explosive_count(ExplosiveKind::TimeBubble)).unwrap_or(u8::MAX)
            });
        }
        self.logistics.supplies_of(player_id, supply)
    }

    /// Puts `count` of `supply` aboard `player_id`'s rig (up to the stack); returns how many
    /// went in. Time-Bubble Bombs go to the rig's charges.
    pub(super) fn add_supplies(
        &mut self,
        player_id: PlayerId,
        supply: DeepSupply,
        count: u8,
    ) -> u8 {
        if supply == DeepSupply::TimeBubbleBomb {
            let room = MAX_SUPPLY_STACK.saturating_sub(self.supplies_of(player_id, supply));
            let added = count.min(room);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.add_explosives(ExplosiveKind::TimeBubble, u32::from(added));
            }
            return added;
        }
        self.logistics.add_supplies(player_id, supply, count)
    }

    /// Spends one `supply` from `player_id`'s kit, if it holds one (Wave 9: the seam the
    /// Descent mechanics call when a ballast tank is blown, a heater lit, a filter fitted...).
    /// A Time-Bubble Bomb is a charge taken from the rig. Call it from inside the tick only.
    pub fn take_supply(&mut self, player_id: PlayerId, supply: DeepSupply) -> bool {
        if supply == DeepSupply::TimeBubbleBomb {
            return self
                .players
                .get_mut(&player_id)
                .is_some_and(|player| player.take_explosive(ExplosiveKind::TimeBubble));
        }
        self.logistics.take_supply(player_id, supply)
    }

    // --- Lamp filter glass ----------------------------------------------------------------

    /// Whether `player_id` may tint its lamp `filter`: white needs nothing, a colour needs a
    /// fitted set of glass or one in the kit.
    pub(super) fn check_filter_glass(
        &self,
        player_id: PlayerId,
        filter: LampFilter,
    ) -> Result<(), FarStrangeRefusal> {
        if filter == LampFilter::White
            || self
                .logistics
                .supply_effects
                .filter_fitted(player_id, self.simulation_tick)
            || self.supplies_of(player_id, DeepSupply::LampFilter) > 0
        {
            Ok(())
        } else {
            Err(FarStrangeRefusal::NoFilterGlass)
        }
    }

    /// Makes sure `player_id`'s lamp has glass fitted, fitting a set from the kit if it has
    /// none; whether it has glass now.
    pub(super) fn fit_filter_glass(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        output: &mut TickOutput,
    ) -> bool {
        let now = self.simulation_tick;
        if self.logistics.supply_effects.filter_fitted(player_id, now) {
            return true;
        }
        if !self.take_supply(player_id, DeepSupply::LampFilter) {
            return false;
        }
        let until = self.after_millis(now, LAMP_FILTER_MILLIS);
        self.logistics
            .supply_effects
            .filter_sets
            .insert(player_id, until);
        output.push_event(supply_event(
            SupplyEvent::FilterFitted,
            Some(player_id),
            tile,
        ));
        true
    }

    /// Fits `player_id`'s lamp with a set of filter glass before play (scenes, tests).
    pub fn stage_filter_glass(&mut self, player_id: PlayerId) {
        let until = self.after_millis(self.simulation_tick, LAMP_FILTER_MILLIS);
        self.logistics
            .supply_effects
            .filter_sets
            .insert(player_id, until);
    }

    // --- The tick ---------------------------------------------------------------------------

    /// One tick of the supplies at work (after the Hollow World's pass).
    pub(super) fn advance_supplies(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_descents() {
            return;
        }
        self.fold_kit_time_bubbles();
        self.sweep_spent_supplies(next_tick);
        self.glaze_unglazed_lamps(next_tick);
        self.wear_filter_glass(next_tick, output);
        self.blow_ballast(next_tick, output);
        self.throw_bait(next_tick, output);
        self.hold_baited_beasts();
    }

    /// Time-Bubble Bombs an older kit still holds become the rig's charges.
    fn fold_kit_time_bubbles(&mut self) {
        let holders: Vec<(PlayerId, u8)> = self
            .logistics
            .supplies
            .keys()
            .map(|id| {
                (
                    *id,
                    self.logistics.supplies_of(*id, DeepSupply::TimeBubbleBomb),
                )
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        for (player_id, count) in holders {
            for _ in 0..count {
                let _ = self
                    .logistics
                    .take_supply(player_id, DeepSupply::TimeBubbleBomb);
            }
            if let Some(player) = self.players.get_mut(&player_id) {
                player.add_explosives(ExplosiveKind::TimeBubble, u32::from(count));
            }
        }
    }

    /// A lamp tinted with no glass fitted (an older save's, from before lamps needed Lamp Filter
    /// glass) gets one set's worth from now, so it wears off like any other instead of staying
    /// tinted for ever (Wave 11 carryover migration).
    fn glaze_unglazed_lamps(&mut self, next_tick: SimulationTick) {
        let unglazed: Vec<PlayerId> = self
            .far_strange
            .filters
            .iter()
            .filter(|(id, filter)| {
                **filter != LampFilter::White
                    && !self.logistics.supply_effects.filter_sets.contains_key(*id)
            })
            .map(|(id, _)| *id)
            .collect();
        for player_id in unglazed {
            let until = self.after_millis(next_tick, LAMP_FILTER_MILLIS);
            self.logistics
                .supply_effects
                .filter_sets
                .insert(player_id, until);
        }
    }

    /// Spent flares, bait and blows leave the section.
    fn sweep_spent_supplies(&mut self, next_tick: SimulationTick) {
        let effects = &mut self.logistics.supply_effects;
        effects.lures.retain(|lure| next_tick < lure.until);
        effects.ballast_blows.retain(|_, until| next_tick < *until);
    }

    /// Worn-out filter glass is replaced from the kit, or the lamp goes white.
    fn wear_filter_glass(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let worn: Vec<PlayerId> = self
            .logistics
            .supply_effects
            .filter_sets
            .iter()
            .filter(|(_, until)| next_tick >= **until)
            .map(|(id, _)| *id)
            .collect();
        let frame = self.frame();
        for player_id in worn {
            self.logistics.supply_effects.filter_sets.remove(&player_id);
            let tile = self
                .players
                .get(&player_id)
                .map_or(TilePosition { x: 0, y: 0 }, |player| player.tile_in(frame));
            if self.lamp_filter_of(player_id) == LampFilter::White {
                continue;
            }
            if self.take_supply(player_id, DeepSupply::LampFilter) {
                let until = self.after_millis(next_tick, LAMP_FILTER_MILLIS);
                self.logistics
                    .supply_effects
                    .filter_sets
                    .insert(player_id, until);
                output.push_event(supply_event(
                    SupplyEvent::FilterFitted,
                    Some(player_id),
                    tile,
                ));
            } else {
                self.set_lamp_filter(player_id, LampFilter::White);
                output.push_event(supply_event(
                    SupplyEvent::FilterWornOut,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    /// A submerged rig in trouble (a third of its hull, or the abyss's pressure cracking it)
    /// blows a Ballast Tank.
    fn blow_ballast(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let frame = self.frame();
        let troubled: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .filter(|(id, player)| {
                player.hull > 0.0
                    && !self.is_on_foot(**id)
                    && !self.logistics.supply_effects.blowing(**id, next_tick)
                    && self.logistics.supplies_of(**id, DeepSupply::BallastTank) > 0
                    && self
                        .submersion(
                            player.x,
                            player.y,
                            super::movement::AUTHORITATIVE_PLAYER_RADIUS,
                        )
                        .share
                        >= SUBMARINE_SHARE
                    && (player.hull <= player.max_hull() * BALLAST_BLOW_HULL_SHARE
                        || self
                            .abyss_pressure_for(**id)
                            .is_some_and(|(pressure, capacity)| pressure > capacity))
            })
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .collect();
        for (player_id, tile) in troubled {
            if !self.take_supply(player_id, DeepSupply::BallastTank) {
                continue;
            }
            let until = self.after_millis(next_tick, BALLAST_BLOW_MILLIS);
            self.logistics
                .supply_effects
                .ballast_blows
                .insert(player_id, until);
            output.push_event(supply_event(
                SupplyEvent::BallastBlown,
                Some(player_id),
                tile,
            ));
        }
    }

    /// A lure angler's light caught `player_id`'s rig: it drops a Flare Buoy from its kit if no
    /// flare already lights it. Returns whether a flare now dazzles the anglers round it.
    pub(super) fn light_flare(&mut self, player_id: PlayerId, output: &mut TickOutput) -> bool {
        let Some((x, y, tile)) = self
            .players
            .get(&player_id)
            .map(|player| (player.x, player.y, player.tile_in(self.frame())))
        else {
            return false;
        };
        if self.flare_dazzles(x, y) {
            return true;
        }
        if !self.take_supply(player_id, DeepSupply::FlareBuoy) {
            return false;
        }
        let now = self.simulation_tick;
        let until = self.after_millis(now, FLARE_MILLIS);
        self.push_supply_lure(SupplyLure {
            supply: DeepSupply::FlareBuoy,
            owner: player_id,
            at: tile,
            since: now,
            until,
        });
        output.push_event(supply_event(SupplyEvent::FlareLit, Some(player_id), tile));
        true
    }

    fn push_supply_lure(&mut self, lure: SupplyLure) {
        let lures = &mut self.logistics.supply_effects.lures;
        lures.push(lure);
        if lures.len() > MAX_SUPPLY_LURES {
            let extra = lures.len() - MAX_SUPPLY_LURES;
            lures.drain(..extra);
        }
    }

    /// A predator of the Hollow World hunting a rig that carries Beast Bait gets bait thrown
    /// past it (one bait a rig at a time).
    fn throw_bait(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let frame = self.frame();
        let mut throws: Vec<(PlayerId, TilePosition)> = Vec::new();
        for (entity, state) in self.enemies() {
            if !bait_draws(state.kind) || !hunting(state.mode) {
                continue;
            }
            let Some(target) = state.target else {
                continue;
            };
            if throws.iter().any(|(id, _)| *id == target)
                || self.logistics.supplies_of(target, DeepSupply::BeastBait) == 0
                || self.bait_near(entity.body.x, entity.body.y).is_some()
            {
                continue;
            }
            let Some(rig) = self.players.get(&target).filter(|rig| rig.hull > 0.0) else {
                continue;
            };
            let beast = frame.tile_at(entity.body.x, entity.body.y);
            let away = if entity.body.x >= rig.x { 1 } else { -1 };
            throws.push((
                target,
                TilePosition {
                    x: beast.x + away * BAIT_THROW_TILES,
                    y: beast.y,
                },
            ));
        }
        for (player_id, at) in throws {
            if !self.take_supply(player_id, DeepSupply::BeastBait) {
                continue;
            }
            let until = self.after_millis(next_tick, BAIT_MILLIS);
            self.push_supply_lure(SupplyLure {
                supply: DeepSupply::BeastBait,
                owner: player_id,
                at,
                since: next_tick,
                until,
            });
            output.push_event(supply_event(SupplyEvent::BaitThrown, Some(player_id), at));
        }
    }

    /// Predators on the scent of bait give up their prey and go to it; a predator whose bait is
    /// spent (or out of reach) goes back to the home it had before.
    fn hold_baited_beasts(&mut self) {
        self.release_baited_beasts();
        if self
            .logistics
            .supply_effects
            .lures_of(DeepSupply::BeastBait)
            .next()
            .is_none()
        {
            return;
        }
        let held: Vec<(crate::entity::EntityId, TilePosition)> = self
            .enemies()
            .filter(|(_, state)| bait_draws(state.kind) && state.mode != EnemyMode::Stunned)
            .filter_map(|(entity, state)| {
                let bait = self.bait_near(entity.body.x, entity.body.y)?;
                (state.target.is_some() || state.home != (bait.x, bait.y))
                    .then_some((entity.id, bait))
            })
            .collect();
        for (id, bait) in held {
            let known = self
                .logistics
                .supply_effects
                .baited_homes
                .iter()
                .any(|baited| baited.entity == id);
            if !known && let Some(home) = self.enemy_home(id) {
                self.logistics
                    .supply_effects
                    .baited_homes
                    .push(BaitedHome { entity: id, home });
            }
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.target = None;
                    state.home = (bait.x, bait.y);
                    state.waypoint = None;
                    if hunting(state.mode) {
                        state.mode = EnemyMode::Return;
                    }
                }
            });
        }
    }

    /// The home tile of enemy `id`, if it is one.
    fn enemy_home(&self, id: crate::entity::EntityId) -> Option<(i32, i32)> {
        self.entities
            .get(id)
            .and_then(|entity| match &entity.state {
                EntityState::Enemy(state) => Some(state.home),
                _ => None,
            })
    }

    /// Baited predators no longer held by live bait go back to their old homes; records of
    /// beasts that are gone are dropped.
    fn release_baited_beasts(&mut self) {
        if self.logistics.supply_effects.baited_homes.is_empty() {
            return;
        }
        let records = std::mem::take(&mut self.logistics.supply_effects.baited_homes);
        let mut kept = Vec::with_capacity(records.len());
        for record in records {
            let Some((x, y)) = self.entities.get(record.entity).and_then(|entity| {
                matches!(entity.state, EntityState::Enemy(_))
                    .then_some((entity.body.x, entity.body.y))
            }) else {
                continue;
            };
            if self.bait_near(x, y).is_some() {
                kept.push(record);
                continue;
            }
            self.entities.update(record.entity, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.home = record.home;
                    state.waypoint = None;
                }
            });
        }
        self.logistics.supply_effects.baited_homes = kept;
    }

    // --- Staging (tests, scenes) ------------------------------------------------------------

    /// Lays a supply lure (a flare or bait) at `at` for its full time (tests and scenes).
    pub fn stage_supply_lure(&mut self, supply: DeepSupply, owner: PlayerId, at: TilePosition) {
        let now = self.simulation_tick;
        let millis = if supply == DeepSupply::BeastBait {
            BAIT_MILLIS
        } else {
            FLARE_MILLIS
        };
        let until = self.after_millis(now, millis);
        self.push_supply_lure(SupplyLure {
            supply,
            owner,
            at,
            since: now,
            until,
        });
    }
}
