//! Delayed blast cascades and every explosive kind (see [`crate::explosive`]). No rendering
//! dependencies.
//!
//! Charges live in the entity store as [`EntityKind::Charge`] entities (motion in the entity's
//! [`crate::entity::Body`], everything else in [`ChargeState`]); [`WorldState::bombs`] views
//! them as [`PlacedBomb`]s. Their motion is the shared [`physics::step_body`] /
//! [`physics::step_body_sticky`] with [`BodyParams::BOMB`], and blast kicks use
//! [`physics::apply_blast`]. The pass runs at its own point in the tick
//! ([`crate::entity::EntityPhase::Charges`]), never in the generic entity phases.

use std::collections::BTreeSet;

use crate::{
    entity::{Body, ChargeState, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::{
        BlastDirection, BlastShape, CLUSTER_BOMBLETS, Deployment, ExplosiveKind,
        FLARE_PULSE_MILLIS, FLARE_PULSES, FLARE_REVEAL_RADIUS, INCENDIARY_RING_MILLIS,
        INCENDIARY_RINGS, REMOTE_ARMING_MILLIS, REMOTE_FUSE, SEQUENCE_STEP_MILLIS,
        SMOKE_PULSE_MILLIS, SMOKE_PULSES, ThrowAim, blast_impulse, smoke_radius,
    },
    hazard::PlacedBomb,
    ids::{PlayerId, SimulationTick},
    physics::{self, BodyParams, line_clear},
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{BlastResult, SurfaceMaterial, TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::{WorldState, commands::bomb_tile_position, quests::QuestNotes};

/// Collision half-size, in world pixels (the fuse is cosmetic).
pub const BOMB_RADIUS: f32 = BodyParams::BOMB.radius;
const MAX_SPEED: f32 = BodyParams::BOMB.max_speed;
const IMPACT_SOUND_SPEED: f32 = 90.0;
/// Small but visible delay: a kicked bomb moves before its secondary explosion.
const CHAIN_DELAY_MILLIS: u64 = 220;
/// Limit work and network/particle traffic without dropping existing bombs.
pub const MAX_ACTIVE_BOMBS: usize = 256;
/// Rigs are far heavier than a bomb: they take this share of a blast's push.
const RIG_PUSH_SCALE: f32 = 0.55;
/// Largest sideways speed a blast leaves a rig with: under the wall-impact damage threshold
/// (three quarters of the safe landing speed), so being blown into a wall never hurts.
const RIG_PUSH_MAX_X: f32 = 230.0;
/// Largest upward speed a blast gives a rig (kept under the safe landing speed, so a blast on
/// its own never makes a rig crash-land).
const RIG_PUSH_MAX_LIFT: f32 = 260.0;
/// Largest downward speed a blast leaves a rig with (under the safe landing speed).
const RIG_PUSH_MAX_FALL: f32 = 300.0;
/// Bomblet launch velocities relative to the cluster bomb: a fan over the top half.
const BOMBLET_FAN: [(f32, f32); CLUSTER_BOMBLETS] = [
    (-230.0, -160.0),
    (-140.0, -250.0),
    (-50.0, -300.0),
    (50.0, -300.0),
    (140.0, -250.0),
    (230.0, -160.0),
];
/// Extra fuse per bomblet, so a cluster crackles instead of popping all at once.
const BOMBLET_STAGGER_MILLIS: u64 = 70;
/// Longest a throw preview follows the arc.
const PREVIEW_MILLIS: u64 = 1_600;

/// One primary detonation this tick: where, what kind, and who set it.
///
/// [`WorldState::push_from_blasts`] pushes rigs and every blast-feeling entity of the entity
/// store (drops) with it; `WorldState::enemies_feel_blasts` hurts, pushes and stuns enemies
/// with the same blasts.
#[derive(Clone, Copy, Debug)]
pub struct Blast {
    pub kind: ExplosiveKind,
    pub x: f32,
    pub y: f32,
    /// Who set the charge (blast damage and enemy kills are credited to them).
    pub owner: Option<PlayerId>,
}

impl WorldState {
    pub(super) fn advance_bombs(
        &mut self,
        next_tick: SimulationTick,
        notes: &mut QuestNotes,
        output: &mut TickOutput,
    ) {
        let mut bombs = self.take_charges();
        bombs.sort_by_key(|(_, bomb)| bomb.id);
        for (_, bomb) in &mut bombs {
            if bomb.stuck {
                bomb.previous_x = bomb.x;
                bomb.previous_y = bomb.y;
                if supported(&self.terrain, bomb) {
                    continue;
                }
                // The tile holding it is gone: it drops like any loose charge.
                bomb.stuck = false;
            }
            let motion = move_bomb(
                bomb,
                (&self.terrain, &self.fields),
                self.timing.delta_seconds,
                sticks(bomb),
            );
            if motion.stuck.is_some() {
                // `move_bomb` pointed `direction` at the surface (a shaped charge cuts into it).
                bomb.stuck = true;
                output.push_event(stuck_event(*bomb));
            } else if motion.impact {
                output.push_event(impact_event(*bomb));
            }
        }
        let (expiring, mut survivors): (Vec<_>, Vec<_>) = bombs
            .into_iter()
            .partition(|(_, bomb)| next_tick >= bomb.detonation_tick);
        // Only a charge's first detonation is a blast; a burning fire or flare just ticks on.
        let blasts: Vec<Blast> = expiring
            .iter()
            .filter(|(_, bomb)| !bomb.burning())
            .map(|(_, bomb)| Blast {
                kind: bomb.kind,
                x: bomb.x,
                y: bomb.y,
                owner: bomb.owner,
            })
            .collect();
        // Every blast in a tick sees the SAME pre-blast terrain for occlusion. Terrain is
        // cleared only after all impulses are resolved, so chain behavior is order-independent.
        let chain_delay = self.timing.rate.ticks_for_millis(CHAIN_DELAY_MILLIS);
        for source in &blasts {
            for (_, bomb) in &mut survivors {
                kick_bomb(bomb, source, &self.terrain, next_tick, chain_delay);
            }
        }
        self.push_from_blasts(&blasts);
        // Enemies take the same blasts (damage, pushes, EMP stuns, scares) on the same terrain;
        // every other damageable entity (practice targets) through the combat model.
        self.enemies_feel_blasts(&blasts, output);
        // Fire burns the Root Labyrinth's roots away for good (Wave 8).
        self.living_dark_blasts(&blasts, output);
        self.damage_from_blasts(&blasts, output);
        // Pilots on foot are pushed and hurt too (Part II F5).
        self.pilots_feel_blasts(&blasts, output);
        // Sealed explosives doors of tomb sites crack (WP-TOMBS).
        self.blast_tomb_seals(&blasts, output);
        let mut spawned = Vec::new();
        for (id, bomb) in expiring {
            if let Some(continuing) = self.expire_bomb(bomb, next_tick, notes, output, &mut spawned)
            {
                survivors.push((id, continuing));
            }
        }
        let room = MAX_ACTIVE_BOMBS.saturating_sub(survivors.len());
        survivors.extend(spawned.into_iter().take(room).map(|bomb| (None, bomb)));
        for (_, bomb) in &mut survivors {
            bomb.velocity_x = quantize(bomb.velocity_x.clamp(-MAX_SPEED, MAX_SPEED));
            bomb.velocity_y = quantize(bomb.velocity_y.clamp(-MAX_SPEED, MAX_SPEED));
            bomb.timer_seconds = if bomb.awaiting_remote() {
                0.0
            } else {
                self.timing
                    .rate
                    .seconds_until(next_tick, bomb.detonation_tick)
            };
        }
        self.store_charges(survivors);
    }

    /// Every live charge with its entity id (in id order), as [`PlacedBomb`] values for the
    /// pass to work on; [`Self::store_charges`] writes the outcome back.
    fn take_charges(&self) -> Vec<(Option<EntityId>, PlacedBomb)> {
        self.entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| {
                charge_view(entity.body, &entity.state).map(|bomb| (Some(entity.id), bomb))
            })
            .collect()
    }

    /// Writes the pass's charges back: changed ones are updated (a fresh revision, so
    /// replication resends only what moved or ticked), new ones spawned, missing ones
    /// despawned.
    fn store_charges(&mut self, charges: Vec<(Option<EntityId>, PlacedBomb)>) {
        let kept: BTreeSet<EntityId> = charges.iter().filter_map(|(id, _)| *id).collect();
        let gone: BTreeSet<EntityId> = self
            .entities
            .ids_of(EntityKind::Charge)
            .filter(|id| !kept.contains(id))
            .collect();
        let _removed = self.entities.despawn_many(&gone);
        for (id, bomb) in charges {
            let (body, state) = charge_parts(&bomb);
            match id {
                Some(id) => {
                    let unchanged = self.entities.get(id).is_some_and(|entity| {
                        entity.body == body && entity.state == EntityState::Charge(state)
                    });
                    if !unchanged {
                        let _ = self.entities.update(id, |entity| {
                            entity.body = body;
                            entity.state = EntityState::Charge(state);
                        });
                    }
                }
                None => {
                    let _ = self.entities.spawn(
                        self.simulation_tick,
                        body,
                        EntityState::Charge(state),
                        None,
                    );
                }
            }
        }
    }

    /// Adds a new charge (deployed this tick).
    pub(super) fn spawn_charge(&mut self, bomb: &PlacedBomb) -> EntityId {
        let (body, state) = charge_parts(bomb);
        self.entities
            .spawn(self.simulation_tick, body, EntityState::Charge(state), None)
    }

    /// Adds a charge as-is (building a scenario world before play: tests, capture scenes,
    /// save migration). Keeps the charge serial counter ahead of it.
    pub fn insert_charge(&mut self, bomb: PlacedBomb) {
        self.next_bomb_id = self.next_bomb_id.max(bomb.id.saturating_add(1));
        let _ = self.spawn_charge(&bomb);
    }

    /// Every live charge (bombs, sticky bombs, C4, bomblets, burning flares and fires) as a
    /// [`PlacedBomb`], in placement (serial) order.
    #[must_use]
    pub fn bombs(&self) -> Vec<PlacedBomb> {
        let mut bombs: Vec<PlacedBomb> = self
            .entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| charge_view(entity.body, &entity.state))
            .collect();
        bombs.sort_by_key(|bomb| bomb.id);
        bombs
    }

    /// Number of live charges.
    #[must_use]
    pub fn bomb_count(&self) -> usize {
        self.entities.count_of(EntityKind::Charge)
    }

    /// Pushes rigs and blast-feeling entities away from this tick's blasts (line of sight on
    /// the pre-blast terrain). Entities take the same push formula
    /// ([`crate::physics::Blast::explosive`]), uncapped, and are woken.
    pub(super) fn push_from_blasts(&mut self, blasts: &[Blast]) {
        for blast in blasts {
            if let Some(entity_blast) =
                crate::physics::Blast::explosive(blast.kind, blast.x, blast.y)
            {
                self.push_entities_with_blast(&entity_blast);
            }
            for (player_id, player) in &mut self.players {
                let Some((dx, dy)) = blast_impulse(
                    blast.kind,
                    (blast.x, blast.y),
                    (player.x, player.y),
                    player_id.get().is_multiple_of(2),
                ) else {
                    continue;
                };
                if !line_clear(&self.terrain, blast.x, blast.y, player.x, player.y) {
                    continue;
                }
                player.velocity_x = quantize(pushed_speed(
                    player.velocity_x,
                    dx * RIG_PUSH_SCALE,
                    RIG_PUSH_MAX_X,
                    RIG_PUSH_MAX_X,
                ));
                player.velocity_y = quantize(pushed_speed(
                    player.velocity_y,
                    dy * RIG_PUSH_SCALE,
                    RIG_PUSH_MAX_LIFT,
                    RIG_PUSH_MAX_FALL,
                ));
            }
        }
    }

    /// Runs out one charge's fuse: blasts, bursts, burns or pulses. Returns the charge again if
    /// it keeps going (a fire spreading, a flare burning).
    #[allow(clippy::too_many_lines, reason = "one arm per lingering charge kind")]
    fn expire_bomb(
        &mut self,
        mut bomb: PlacedBomb,
        next_tick: SimulationTick,
        notes: &mut QuestNotes,
        output: &mut TickOutput,
        spawned: &mut Vec<PlacedBomb>,
    ) -> Option<PlacedBomb> {
        let center = bomb_tile_position(self.frame(), bomb);
        let schedule = |world: &Self, millis: u64| {
            SimulationTick::new(
                next_tick
                    .get()
                    .saturating_add(world.timing.rate.ticks_for_millis(millis).max(1)),
            )
        };
        if bomb.burning() {
            match bomb.kind {
                ExplosiveKind::Incendiary => {
                    self.burn_ring(&bomb, i32::from(bomb.stage), output);
                    output.push_event(fire_event(bomb));
                    if bomb.stage >= INCENDIARY_RINGS {
                        return None;
                    }
                    bomb.stage += 1;
                    bomb.detonation_tick = schedule(self, INCENDIARY_RING_MILLIS);
                    return Some(bomb);
                }
                ExplosiveKind::Flare => {
                    self.flare_pulse(&bomb, center);
                    if bomb.stage >= FLARE_PULSES {
                        return None;
                    }
                    bomb.stage += 1;
                    bomb.detonation_tick = schedule(self, FLARE_PULSE_MILLIS);
                    return Some(bomb);
                }
                ExplosiveKind::Smoke => {
                    if bomb.stage >= SMOKE_PULSES {
                        return None;
                    }
                    bomb.stage += 1;
                    bomb.detonation_tick = schedule(self, SMOKE_PULSE_MILLIS);
                    return Some(bomb);
                }
                // A time bubble (Wave 9) swells, holds and thins like a smoke screen.
                ExplosiveKind::TimeBubble => {
                    if bomb.stage >= crate::far_strange::TIME_BUBBLE_PULSES {
                        return None;
                    }
                    bomb.stage += 1;
                    bomb.detonation_tick =
                        schedule(self, crate::far_strange::TIME_BUBBLE_PULSE_MILLIS);
                    return Some(bomb);
                }
                _ => return None,
            }
        }
        let blast = self.blast_terrain(&bomb, center);
        if blast.is_some() {
            notes.detonations.push(center);
        }
        output.push_event(if bomb.kind == ExplosiveKind::Bomb {
            explosion_event(bomb)
        } else {
            detonated_event(bomb)
        });
        if let Some(blast) = blast {
            // Bomb loot (WP-DROPS): the broken tiles spray out as drops.
            self.spawn_blast_drops(bomb.kind, bomb.x, bomb.y, &blast.broken);
            // A blast that frees the Star Core (it now drops intact instead of vanishing)
            // starts the escape just as drilling it does, so bombing it is no shortcut.
            if blast.broken.iter().any(|(_, kind)| {
                *kind == TileKind::Artifact(crate::terrain::ArtifactKind::StarCore)
            }) && let Some(player_id) =
                bomb.owner.or_else(|| self.players.keys().next().copied())
            {
                Self::start_escape(player_id, output);
            }
            self.vent_vault_seals(&blast.changed_tiles, output);
        }
        match bomb.kind {
            ExplosiveKind::Cluster => {
                self.release_bomblets(&bomb, next_tick, spawned);
                output.push_event(cluster_event(bomb));
                None
            }
            ExplosiveKind::Incendiary => {
                bomb.stage = 1;
                bomb.chain_triggered = false;
                bomb.detonation_tick = schedule(self, INCENDIARY_RING_MILLIS);
                Some(bomb)
            }
            ExplosiveKind::Flare => {
                self.flare_pulse(&bomb, center);
                bomb.stage = 1;
                bomb.chain_triggered = false;
                bomb.detonation_tick = schedule(self, FLARE_PULSE_MILLIS);
                Some(bomb)
            }
            ExplosiveKind::Smoke => {
                bomb.stage = 1;
                bomb.chain_triggered = false;
                bomb.detonation_tick = schedule(self, SMOKE_PULSE_MILLIS);
                Some(bomb)
            }
            ExplosiveKind::TimeBubble => {
                bomb.stage = 1;
                bomb.chain_triggered = false;
                bomb.detonation_tick = schedule(self, crate::far_strange::TIME_BUBBLE_PULSE_MILLIS);
                Some(bomb)
            }
            _ => None,
        }
    }

    /// Removes the terrain a charge's blast shape covers; `None` for charges with no blast.
    fn blast_terrain(&mut self, bomb: &PlacedBomb, center: TilePosition) -> Option<BlastResult> {
        match bomb.kind.blast() {
            BlastShape::None => None,
            // A mining charge (`LootSafe`) breaks ore and artifacts cleanly: they pop out intact
            // as drops (`world/drops.rs`, `ore_survives_blast`) instead of shattering.
            BlastShape::Diamond(radius) | BlastShape::LootSafe(radius) => {
                Some(self.detonate_bomb_at(center, radius))
            }
            BlastShape::Tunnel { length, half_width } => {
                let (step_x, step_y) = bomb.direction.step();
                // Lanes run side by side, perpendicular to the cut.
                let (side_x, side_y) = (step_y, step_x);
                let mut result = BlastResult::default();
                for lane in -half_width..=half_width {
                    for step in 0..length {
                        let position = TilePosition {
                            x: center.x + step_x * step + side_x * lane,
                            y: center.y + step_y * step + side_y * lane,
                        };
                        let Some(tile) = self.terrain.tile(position) else {
                            break;
                        };
                        if blast_proof(tile.kind) {
                            break;
                        }
                        merge(&mut result, self.terrain.blast_radius(position, 0));
                    }
                }
                Some(result)
            }
        }
    }

    /// One ring of an incendiary's fire: dirt, clay, gas and sky-island turf, soil and roots at
    /// Manhattan distance `radius` burn away (baked into blocks that drop); explosive pockets it reaches go off.
    fn burn_ring(&mut self, bomb: &PlacedBomb, radius: i32, output: &mut TickOutput) {
        let center = bomb_tile_position(self.frame(), *bomb);
        let mut changed = Vec::new();
        let mut broken = Vec::new();
        for y in center.y - radius..=center.y + radius {
            for x in center.x - radius..=center.x + radius {
                if (x - center.x).abs() + (y - center.y).abs() != radius {
                    continue;
                }
                let position = TilePosition { x, y };
                let Some(tile) = self.terrain.tile(position) else {
                    continue;
                };
                let reach = match tile.kind {
                    TileKind::Dirt
                    | TileKind::Clay
                    | TileKind::Gas
                    | TileKind::Surface(
                        SurfaceMaterial::Snow | SurfaceMaterial::Mud | SurfaceMaterial::Sand,
                    )
                    // A sky island's turf, soil and dangling roots burn like dirt.
                    | TileKind::Sky(
                        crate::sky::SkyTile::Turf(_)
                        | crate::sky::SkyTile::Soil
                        | crate::sky::SkyTile::Roots,
                    ) => 0,
                    TileKind::ExplosivePocket => 1,
                    _ => continue,
                };
                let blast = self.terrain.blast_radius(position, reach);
                changed.extend(blast.changed_tiles);
                broken.extend(blast.broken);
            }
        }
        self.spawn_blast_drops(bomb.kind, bomb.x, bomb.y, &broken);
        self.vent_vault_seals(&changed, output);
    }

    /// A lit flare charts the tiles around it for its owner.
    fn flare_pulse(&mut self, bomb: &PlacedBomb, center: TilePosition) {
        if let Some(owner) = bomb.owner
            && self.players.contains_key(&owner)
        {
            let _revealed = self.reveal_scanner_area(owner, center, FLARE_REVEAL_RADIUS);
        }
    }

    /// Bursts a cluster bomb into its fan of bomblets (identity in fan order).
    fn release_bomblets(
        &mut self,
        parent: &PlacedBomb,
        next_tick: SimulationTick,
        spawned: &mut Vec<PlacedBomb>,
    ) {
        for (index, (vx, vy)) in (0_u64..).zip(BOMBLET_FAN) {
            let fuse = ExplosiveKind::Bomblet
                .fuse_millis()
                .saturating_add(index * BOMBLET_STAGGER_MILLIS);
            spawned.push(PlacedBomb {
                id: self.next_bomb_id,
                x: parent.x,
                y: parent.y,
                previous_x: parent.x,
                previous_y: parent.y,
                velocity_x: quantize(parent.velocity_x.mul_add(0.3, vx)),
                velocity_y: quantize(parent.velocity_y.mul_add(0.3, vy)),
                angle: 0.0,
                detonation_tick: SimulationTick::new(
                    next_tick
                        .get()
                        .saturating_add(self.timing.rate.ticks_for_millis(fuse)),
                ),
                chain_triggered: false,
                timer_seconds: 0.0,
                kind: ExplosiveKind::Bomblet,
                owner: parent.owner,
                stuck: false,
                direction: parent.direction,
                stage: 0,
            });
            self.next_bomb_id = self.next_bomb_id.saturating_add(1);
        }
    }

    /// The charge `player_id` would deploy right now: kind, start pose and velocity.
    fn launch(
        &self,
        player_id: PlayerId,
        kind: ExplosiveKind,
        aim: ThrowAim,
    ) -> Option<PlacedBomb> {
        let player = self.players.get(&player_id)?;
        let (aim_x, aim_y) = aim.vector();
        let pose = match kind.deployment() {
            Deployment::Drop => (
                player.x,
                TILE_SIZE.mul_add(0.4, player.y),
                player.velocity_x * 0.25,
                player.velocity_y.max(0.0),
            ),
            Deployment::Throw => (
                player.x,
                player.y,
                aim_x.mul_add(kind.launch_speed(), player.velocity_x * 0.5),
                aim_y.mul_add(kind.launch_speed(), player.velocity_y * 0.5),
            ),
            Deployment::Place => (
                player.x,
                player.y,
                aim_x * kind.launch_speed(),
                aim_y * kind.launch_speed(),
            ),
        };
        Some(self.launch_from(player_id, kind, aim, pose))
    }

    /// A charge of `kind` owned by `player_id` starting at `(x, y)` with velocity
    /// `(velocity_x, velocity_y)` (`pose`), its fuse lit now. The rig launches from its own
    /// pose ([`Self::launch`]); a pilot on foot throws from its hands (`world/pilot.rs`).
    pub(super) fn launch_from(
        &self,
        player_id: PlayerId,
        kind: ExplosiveKind,
        aim: ThrowAim,
        pose: (f32, f32, f32, f32),
    ) -> PlacedBomb {
        let (aim_x, aim_y) = aim.vector();
        let (x, y, velocity_x, velocity_y) = pose;
        let fuse = if kind.remote() {
            REMOTE_FUSE
        } else {
            SimulationTick::new(
                self.simulation_tick
                    .get()
                    .saturating_add(self.timing.ticks_for_millis(kind.fuse_millis())),
            )
        };
        #[allow(
            clippy::cast_precision_loss,
            reason = "fuses are a few seconds of milliseconds"
        )]
        let timer_seconds = if kind.remote() {
            0.0
        } else {
            kind.fuse_millis() as f32 / 1_000.0
        };
        PlacedBomb {
            id: self.next_bomb_id,
            x: quantize(x),
            y: quantize(y),
            previous_x: quantize(x),
            previous_y: quantize(y),
            velocity_x: quantize(velocity_x),
            velocity_y: quantize(velocity_y),
            angle: 0.0,
            detonation_tick: fuse,
            chain_triggered: false,
            timer_seconds,
            kind,
            owner: Some(player_id),
            stuck: false,
            direction: BlastDirection::nearest(aim_x, aim_y),
            stage: 0,
        }
    }

    /// Resolves [`crate::PlayerCommand::DeployExplosive`].
    pub(super) fn deploy_explosive(
        &mut self,
        player_id: PlayerId,
        kind: ExplosiveKind,
        aim: ThrowAim,
    ) -> PlayerScopedCommandOutcome {
        if kind == ExplosiveKind::Bomblet {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if player.explosive_count(kind) == 0 || self.bomb_count() >= MAX_ACTIVE_BOMBS {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(bomb) = self.launch(player_id, kind, aim) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            let _taken = player.take_explosive(kind);
        }
        let _ = self.spawn_charge(&bomb);
        self.next_bomb_id = self.next_bomb_id.saturating_add(1);
        PlayerScopedCommandOutcome::Applied
    }

    /// Resolves [`crate::PlayerCommand::RemoteDetonate`]: arms every remote charge `player_id`
    /// placed that is still waiting: after [`REMOTE_ARMING_MILLIS`], all at once or
    /// [`SEQUENCE_STEP_MILLIS`] apart in
    /// placement order.
    pub(super) fn remote_detonate(
        &mut self,
        player_id: PlayerId,
        sequenced: bool,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        let step = if sequenced {
            self.timing.ticks_for_millis(SEQUENCE_STEP_MILLIS).max(1)
        } else {
            0
        };
        let first = self
            .simulation_tick
            .get()
            .saturating_add(self.timing.ticks_for_millis(REMOTE_ARMING_MILLIS).max(1));
        let mut charges: Vec<(u64, EntityId)> = self
            .entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| match entity.state {
                EntityState::Charge(state)
                    if state.owner == Some(player_id) && awaiting_remote(&state) =>
                {
                    Some((state.serial, entity.id))
                }
                _ => None,
            })
            .collect();
        if charges.is_empty() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        charges.sort_unstable();
        for (order, (_, id)) in (0_u64..).zip(charges) {
            let deadline = SimulationTick::new(first.saturating_add(order.saturating_mul(step)));
            let _ = self.entities.update(id, |entity| {
                if let EntityState::Charge(state) = &mut entity.state {
                    state.detonation_tick = deadline;
                }
            });
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Live smoke screens as `(x, y, radius)` in world pixels, in placement order.
    #[must_use]
    pub fn smoke_screens(&self) -> Vec<(f32, f32, f32)> {
        let mut screens: Vec<(u64, f32, f32, f32)> = self
            .entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| match entity.state {
                EntityState::Charge(state) if state.kind == ExplosiveKind::Smoke => {
                    let radius = smoke_radius(state.stage);
                    (radius > 0.0).then_some((state.serial, entity.body.x, entity.body.y, radius))
                }
                _ => None,
            })
            .collect();
        screens.sort_by_key(|screen| screen.0);
        screens
            .into_iter()
            .map(|(_, x, y, radius)| (x, y, radius))
            .collect()
    }

    /// Every live time bubble (Wave 9, the Time Sludge) as `(x, y, radius)` in local pixels,
    /// oldest first: creatures inside crawl ([`crate::far_strange::TIME_BUBBLE_TEMPO`]).
    #[must_use]
    pub fn time_bubbles(&self) -> Vec<(f32, f32, f32)> {
        let mut bubbles: Vec<(u64, f32, f32, f32)> = self
            .entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| match entity.state {
                EntityState::Charge(state) if state.kind == ExplosiveKind::TimeBubble => {
                    let radius = crate::far_strange::time_bubble_radius(state.stage);
                    (radius > 0.0).then_some((state.serial, entity.body.x, entity.body.y, radius))
                }
                _ => None,
            })
            .collect();
        bubbles.sort_by_key(|bubble| bubble.0);
        bubbles
            .into_iter()
            .map(|(_, x, y, radius)| (x, y, radius))
            .collect()
    }

    /// Whether world point `(x, y)` is hidden inside a smoke screen (creature senses, line of
    /// sight and auto-aim check this).
    #[must_use]
    pub fn smoke_at(&self, x: f32, y: f32) -> bool {
        self.smoke_screens().into_iter().any(|(sx, sy, radius)| {
            let (dx, dy) = (x - sx, y - sy);
            dx.mul_add(dx, dy * dy) <= radius * radius
        })
    }

    /// Remote charges `player_id` placed that are still waiting for the detonator.
    #[must_use]
    pub fn waiting_remote_charges(&self, player_id: PlayerId) -> usize {
        self.entities
            .of_kind(EntityKind::Charge)
            .filter(|entity| {
                matches!(entity.state, EntityState::Charge(state)
                    if state.owner == Some(player_id) && awaiting_remote(&state))
            })
            .count()
    }

    /// The path a charge of `kind` deployed by `player_id` along `aim` would follow from where
    /// the rig is now, one point per tick, ending where it sticks, settles, or after a moment.
    /// Runs the real bomb physics on the current terrain (presentation: the aim preview).
    #[must_use]
    pub fn preview_explosive_path(
        &self,
        player_id: PlayerId,
        kind: ExplosiveKind,
        aim: ThrowAim,
    ) -> Vec<(f32, f32)> {
        let Some(mut bomb) = self.launch(player_id, kind, aim) else {
            return Vec::new();
        };
        let ticks = self.timing.ticks_for_millis(PREVIEW_MILLIS);
        let mut path = vec![(bomb.x, bomb.y)];
        for _ in 0..ticks {
            let motion = move_bomb(
                &mut bomb,
                (&self.terrain, &self.fields),
                self.timing.delta_seconds,
                kind.sticks(),
            );
            path.push((bomb.x, bomb.y));
            let resting = bomb.velocity_x.abs() + bomb.velocity_y.abs() < 1.0;
            if motion.stuck.is_some() || resting {
                break;
            }
        }
        path
    }
}

/// `current + push`, except that a push never carries a rig past `max_negative` /
/// `max_positive` in its direction (a rig already moving faster keeps its own speed): blasts
/// alone never cause crash or wall damage.
fn pushed_speed(current: f32, push: f32, max_negative: f32, max_positive: f32) -> f32 {
    let pushed = current + push;
    if push > 0.0 {
        pushed.min(current.max(max_positive))
    } else {
        pushed.max(current.min(-max_negative))
    }
}

/// The [`PlacedBomb`] view of a charge entity (`None` for other kinds).
const fn charge_view(body: Body, state: &EntityState) -> Option<PlacedBomb> {
    let EntityState::Charge(charge) = state else {
        return None;
    };
    Some(PlacedBomb {
        id: charge.serial,
        x: body.x,
        y: body.y,
        velocity_x: body.velocity_x,
        velocity_y: body.velocity_y,
        previous_x: body.previous_x,
        previous_y: body.previous_y,
        angle: charge.angle,
        detonation_tick: charge.detonation_tick,
        chain_triggered: charge.chain_triggered,
        timer_seconds: charge.timer_seconds,
        kind: charge.kind,
        owner: charge.owner,
        stuck: charge.stuck,
        direction: charge.direction,
        stage: charge.stage,
    })
}

/// Splits a [`PlacedBomb`] into its entity body and charge state.
const fn charge_parts(bomb: &PlacedBomb) -> (Body, ChargeState) {
    (
        Body {
            x: bomb.x,
            y: bomb.y,
            velocity_x: bomb.velocity_x,
            velocity_y: bomb.velocity_y,
            previous_x: bomb.previous_x,
            previous_y: bomb.previous_y,
            rest_ticks: 0,
            asleep: false,
        },
        ChargeState {
            serial: bomb.id,
            angle: bomb.angle,
            detonation_tick: bomb.detonation_tick,
            chain_triggered: bomb.chain_triggered,
            timer_seconds: bomb.timer_seconds,
            kind: bomb.kind,
            owner: bomb.owner,
            stuck: bomb.stuck,
            direction: bomb.direction,
            stage: bomb.stage,
        },
    )
}

/// [`PlacedBomb::awaiting_remote`] for a charge's state.
fn awaiting_remote(state: &ChargeState) -> bool {
    state.kind.remote() && state.detonation_tick == REMOTE_FUSE
}

fn merge(into: &mut BlastResult, blast: BlastResult) {
    into.cleared += blast.cleared;
    into.changed_tiles.extend(blast.changed_tiles);
    into.broken.extend(blast.broken);
}

/// Whether no blast breaks `kind` (shaped-charge lanes stop at it).
const fn blast_proof(kind: TileKind) -> bool {
    match kind {
        TileKind::Foundation | TileKind::Bedrock | TileKind::Built(_) => true,
        TileKind::Sky(sky) => sky.is_indestructible() || sky.is_cache(),
        TileKind::Poi(poi) => poi.after_blast().is_none(),
        // Cave caches shrug off blasts (`Terrain::blast_radius` skips them), so lanes stop too.
        // Builder alloy and the Prism Desert's keyed walls too (Wave 9).
        TileKind::Cave(cave) => cave.is_cache() || cave.blast_proof(),
        // The buried city's machinery and plinths (Wave 10).
        TileKind::City(tile) => tile.indestructible(),
        _ => false,
    }
}

/// Whether a loose charge sticks on its next contact (stuck once, it stays put).
const fn sticks(bomb: &PlacedBomb) -> bool {
    bomb.kind.sticks() && !bomb.stuck
}

/// How far towards its surface a stuck charge probes for it. A charge stops within one swept
/// step (at most two pixels) of what it hit; the extra pixel absorbs quantization so a charge
/// stopped a full step short is never mistaken for one whose wall is gone.
const SUPPORT_PROBE: f32 = 3.0;

/// Whether the surface a stuck charge clings to is still there.
fn supported(terrain: &Terrain, bomb: &PlacedBomb) -> bool {
    let (dx, dy) = bomb.direction_hint();
    physics::collides(
        terrain,
        dx.mul_add(SUPPORT_PROBE, bomb.x),
        dy.mul_add(SUPPORT_PROBE, bomb.y),
        BOMB_RADIUS,
    )
}

impl PlacedBomb {
    /// Unit vector towards the surface a stuck charge clings to.
    #[allow(clippy::cast_precision_loss, reason = "unit steps")]
    const fn direction_hint(&self) -> (f32, f32) {
        let (x, y) = self.direction.step();
        (x as f32, y as f32)
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn explosion_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::BombDetonated {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn detonated_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::ExplosiveDetonated {
        bomb_id: bomb.id,
        kind: bomb.kind,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
        direction: bomb.direction,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn cluster_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::ClusterSplit {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn fire_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::FireSpread {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn stuck_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::ChargeStuck {
        bomb_id: bomb.id,
        kind: bomb.kind,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world positions become event pixels"
)]
const fn impact_event(bomb: PlacedBomb) -> GameplayEventKind {
    GameplayEventKind::BombBounced {
        bomb_id: bomb.id,
        x: bomb.x.round() as i32,
        y: bomb.y.round() as i32,
    }
}

/// What one tick of motion did.
#[derive(Clone, Copy, Debug, Default)]
struct Motion {
    /// Hit something hard enough to be heard.
    impact: bool,
    /// Stuck to a surface on this side (sticky charges only).
    stuck: Option<BlastDirection>,
}

/// One tick of a loose charge's motion: the shared body physics ([`BodyParams::BOMB`]), sticky
/// for a charge that sticks on its next contact (it stops dead there and points `direction` at
/// the surface), plus the cosmetic roll angle. Resting charges are checked each tick, so
/// removing their support wakes them automatically.
fn move_bomb(
    bomb: &mut PlacedBomb,
    (terrain, zones): (&Terrain, &crate::field::FieldZones),
    dt: f32,
    sticky: bool,
) -> Motion {
    let (mut body, _) = charge_parts(bomb);
    // Gravity bombs feel the sky's gravity fields: they float in orbit and arc far on the Moon
    // (and fall sideways or up in a field, F9).
    let params = BodyParams::BOMB.at_fields(terrain, zones, body.x, body.y);
    let report = if sticky {
        physics::step_body_sticky(&mut body, &params, terrain, dt)
    } else {
        physics::step_body(&mut body, &params, terrain, dt)
    };
    bomb.x = body.x;
    bomb.y = body.y;
    bomb.velocity_x = body.velocity_x;
    bomb.velocity_y = body.velocity_y;
    bomb.previous_x = body.previous_x;
    bomb.previous_y = body.previous_y;
    if report.wedged {
        return Motion::default();
    }
    if let Some(side) = report.stuck {
        bomb.direction = side;
    }
    bomb.angle = quantize(
        (bomb.angle + (bomb.x - bomb.previous_x) / BOMB_RADIUS).rem_euclid(std::f32::consts::TAU),
    );
    Motion {
        impact: report.impact_speed >= IMPACT_SOUND_SPEED,
        stuck: report.stuck,
    }
}

/// A blast's effect on another charge: a loose one is pushed ([`physics::apply_blast`], the
/// same kick every blast-feeling body gets), and one close enough has its fuse cut short
/// (chain reaction), stuck or not. A rock wall in between blocks both.
fn kick_bomb(
    bomb: &mut PlacedBomb,
    source: &Blast,
    terrain: &Terrain,
    tick: SimulationTick,
    delay: u64,
) {
    let Some(blast) = physics::Blast::explosive(source.kind, source.x, source.y) else {
        return;
    };
    let distance = (bomb.x - source.x).hypot(bomb.y - source.y);
    if distance >= blast.reach || !line_clear(terrain, source.x, source.y, bomb.x, bomb.y) {
        return;
    }
    if !bomb.stuck {
        // Coincident charges split reproducibly in opposite directions (by serial parity).
        let (mut body, _) = charge_parts(bomb);
        let _ = physics::apply_blast(&mut body, &blast, terrain, !bomb.id.is_multiple_of(2));
        bomb.velocity_x = body.velocity_x;
        bomb.velocity_y = body.velocity_y;
    }
    if distance <= source.kind.chain_reach() && bomb.kind.chains() && !bomb.burning() {
        bomb.chain_triggered = true;
        // Small deterministic staggering keeps a pile readable and preserves a visible fuse.
        let stagger = bomb.id % 4 * (delay / 4).max(1);
        let deadline = SimulationTick::new(
            tick.get()
                .saturating_add(delay.max(1))
                .saturating_add(stagger),
        );
        bomb.detonation_tick = bomb.detonation_tick.min(deadline);
    }
}

/// Test access to the charges as a plain list: edits made through the guard are written back
/// (matched to their entities by serial) when it drops.
#[cfg(test)]
pub struct BombsMut<'w> {
    world: &'w mut WorldState,
    bombs: Vec<PlacedBomb>,
}

#[cfg(test)]
impl std::ops::Deref for BombsMut<'_> {
    type Target = Vec<PlacedBomb>;
    fn deref(&self) -> &Self::Target {
        &self.bombs
    }
}

#[cfg(test)]
impl std::ops::DerefMut for BombsMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bombs
    }
}

#[cfg(test)]
impl Drop for BombsMut<'_> {
    fn drop(&mut self) {
        let by_serial: std::collections::BTreeMap<u64, EntityId> = self
            .world
            .entities
            .of_kind(EntityKind::Charge)
            .filter_map(|entity| match entity.state {
                EntityState::Charge(state) => Some((state.serial, entity.id)),
                _ => None,
            })
            .collect();
        let charges: Vec<(Option<EntityId>, PlacedBomb)> = std::mem::take(&mut self.bombs)
            .into_iter()
            .map(|bomb| (by_serial.get(&bomb.id).copied(), bomb))
            .collect();
        for (_, bomb) in &charges {
            self.world.next_bomb_id = self.world.next_bomb_id.max(bomb.id.saturating_add(1));
        }
        self.world.store_charges(charges);
    }
}

#[cfg(test)]
impl WorldState {
    /// The charges as an editable list (tests).
    pub(crate) fn bombs_mut(&mut self) -> BombsMut<'_> {
        let bombs = self.bombs();
        BombsMut { world: self, bombs }
    }
}
