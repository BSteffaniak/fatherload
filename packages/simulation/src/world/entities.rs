//! The world's entities: spawning, the per-tick entity phases, waking, blasts and entity
//! replication (see [`crate::entity`] for the store and [`crate::physics`] for motion).
//!
//! # Tick
//!
//! `simulate_tick` runs [`EntityPhase::Early`] right after inputs (gas clouds age before hazard
//! damage reads them) and [`EntityPhase::Late`] after drilling, bombs and hazard spawns (expiry
//! first, then boulders and drops). Within a phase, awake entities update in id order.
//! Detonations push blast-feeling entities ([`EntityKind::feels_blasts`]) inside the bomb pass
//! (`push_from_blasts`, with every explosive kind's reach and push). At the end of the tick,
//! every tile the tick changed wakes the sleeping entities around it, so a drop resting on a
//! dug-out tile falls on the next tick.
//!
//! # Adding a kind (Wave 2)
//!
//! 1. Add a variant to [`crate::entity::EntityState`] / [`EntityKind`] with its phase, sleep
//!    and blast rules.
//! 2. Add its update to [`WorldState::advance_entities`]'s match (and a spawn helper here).
//! 3. Use [`crate::physics`] for motion; mutate only through [`EntityStore::update`].
//!
//! Replication, saves, keyframes, hashing and relevance then cover it automatically.

use crate::{
    entity::{
        Body, BoulderState, DropState, Entity, EntityDelta, EntityId, EntityKind, EntityPhase,
        EntityState, EntityStore, GasCloudState,
    },
    event::{GameplayEventKind, TickOutput},
    hazard::{FallingBoulder, HazardCloud},
    ids::SimulationTick,
    item::ItemStack,
    physics::{self, Blast, BodyParams},
    quantize::quantize_simulation_scalar,
    terrain::{TILE_SIZE, TileChange, TilePosition},
};

use super::{
    WorldState,
    hazards::BOULDER_HIT_DAMAGE,
    movement::{AUTHORITATIVE_GRAVITY, AUTHORITATIVE_PLAYER_RADIUS},
};

/// A sleep-capable body that has rested this long falls asleep.
pub const ENTITY_SLEEP_AFTER_MILLIS: u64 = 300;
/// Sleeping entities this close (a square, in pixels) to a changed tile's centre wake up.
const TERRAIN_WAKE_REACH: f32 = TILE_SIZE * 1.5;

impl WorldState {
    /// The world's entities.
    #[must_use]
    pub const fn entities(&self) -> &EntityStore {
        &self.entities
    }

    /// Mutable entities for building a scenario world before play (test harnesses, scene
    /// capture); gameplay only changes entities inside [`Self::simulate_tick`].
    pub const fn entities_mut(&mut self) -> &mut EntityStore {
        &mut self.entities
    }

    /// Spawns an entity now (inside a tick: during the transition to the next tick).
    pub(crate) fn spawn_entity(
        &mut self,
        body: Body,
        state: EntityState,
        expires_at: Option<SimulationTick>,
    ) -> EntityId {
        self.entities
            .spawn(self.simulation_tick, body, state, expires_at)
    }

    /// Spawns a gas cloud.
    pub(crate) fn spawn_gas_cloud(&mut self, cloud: HazardCloud) -> EntityId {
        spawn_gas_cloud_into(&mut self.entities, self.simulation_tick, cloud)
    }

    /// Spawns a falling boulder with its tick deadlines.
    pub(crate) fn spawn_boulder(
        &mut self,
        boulder: FallingBoulder,
        warning_deadline: SimulationTick,
        expiry_tick: SimulationTick,
    ) -> EntityId {
        self.spawn_entity(
            Body::at(boulder.x, boulder.y).moving(0.0, boulder.velocity_y),
            EntityState::Boulder(BoulderState {
                warning_seconds: boulder.warning_seconds,
                life: boulder.life,
                warning_deadline,
                expiry_tick,
            }),
            None,
        )
    }

    /// Spawns a floating item stack at `(x, y)` moving at `(velocity_x, velocity_y)`, which
    /// despawns after `lifetime_millis` (none: never). WP-DROPS adds pickup and merging.
    pub fn spawn_item_drop(
        &mut self,
        x: f32,
        y: f32,
        velocity: (f32, f32),
        stack: ItemStack,
        lifetime_millis: Option<u64>,
    ) -> EntityId {
        let expires_at = lifetime_millis.map(|millis| {
            SimulationTick::new(
                self.simulation_tick
                    .get()
                    .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
            )
        });
        self.spawn_entity(
            Body::at(x, y).moving(velocity.0, velocity.1),
            EntityState::Drop(DropState { stack }),
            expires_at,
        )
    }

    /// Gas clouds, in spawn order.
    #[must_use]
    pub fn hazards(&self) -> Vec<HazardCloud> {
        self.entities
            .of_kind(EntityKind::GasCloud)
            .filter_map(|entity| match entity.state {
                EntityState::GasCloud(cloud) => Some(HazardCloud {
                    x: entity.body.x,
                    y: entity.body.y,
                    life: cloud.life,
                    radius: cloud.radius,
                }),
                _ => None,
            })
            .collect()
    }

    /// Falling boulders, in spawn order.
    #[must_use]
    pub fn falling_boulders(&self) -> Vec<FallingBoulder> {
        self.entities
            .of_kind(EntityKind::Boulder)
            .filter_map(|entity| match entity.state {
                EntityState::Boulder(boulder) => Some(FallingBoulder {
                    x: entity.body.x,
                    y: entity.body.y,
                    velocity_y: entity.body.velocity_y,
                    warning_seconds: boulder.warning_seconds,
                    life: boulder.life,
                }),
                _ => None,
            })
            .collect()
    }

    /// Adds gas clouds and boulders from the pre-entity save format (index-aligned boulder
    /// deadline lists; a missing deadline means "now").
    pub fn restore_legacy_live_entities(
        &mut self,
        hazards: Vec<HazardCloud>,
        boulders: Vec<FallingBoulder>,
        warning_deadlines: &[SimulationTick],
        expiry_ticks: &[SimulationTick],
    ) {
        for cloud in hazards {
            self.spawn_gas_cloud(cloud);
        }
        let now = self.simulation_tick;
        for (index, boulder) in boulders.into_iter().enumerate() {
            self.spawn_boulder(
                boulder,
                warning_deadlines.get(index).copied().unwrap_or(now),
                expiry_ticks.get(index).copied().unwrap_or(now),
            );
        }
    }

    /// Runs one entity phase of the tick producing `next_tick`.
    pub(super) fn advance_entities(
        &mut self,
        phase: EntityPhase,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if phase == EntityPhase::Late {
            let _expired = self.entities.take_expired(next_tick);
        }
        for id in self.entities.awake_in_phase(phase) {
            let Some(kind) = self.entities.get(id).map(Entity::kind) else {
                continue;
            };
            match kind {
                EntityKind::GasCloud => self.advance_gas_cloud(id),
                EntityKind::Boulder => self.advance_boulder(id, next_tick, output),
                EntityKind::Drop => self.advance_drop(id),
            }
        }
    }

    fn advance_gas_cloud(&mut self, id: EntityId) {
        let delta_seconds = self.timing.delta_seconds;
        let alive = self
            .entities
            .update(id, |entity| {
                let EntityState::GasCloud(cloud) = &mut entity.state else {
                    return true;
                };
                cloud.life -= delta_seconds;
                cloud.radius = 8.0f32.mul_add(delta_seconds, cloud.radius);
                cloud.life > 0.0
            })
            .unwrap_or(true);
        if !alive {
            self.entities.despawn(id);
        }
    }

    /// A boulder hangs until its warning deadline, then falls; it shatters on a rig (damaging
    /// it, less with the boulder deflector) or on rock, and crumbles at its expiry.
    fn advance_boulder(
        &mut self,
        id: EntityId,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let timing = self.timing;
        let Some((mut body, state)) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Boulder(state) => Some((entity.body, state)),
            _ => None,
        }) else {
            return;
        };
        // Interpolation hook: the step starts here (gas clouds and hanging boulders keep still).
        body.previous_x = body.x;
        body.previous_y = body.y;
        if next_tick >= state.expiry_tick {
            self.entities.despawn(id);
            return;
        }
        if next_tick >= state.warning_deadline {
            body.velocity_y = quantize_simulation_scalar(
                (AUTHORITATIVE_GRAVITY * 0.8)
                    .mul_add(timing.delta_seconds, body.velocity_y)
                    .min(520.0),
            );
            body.y =
                quantize_simulation_scalar(body.velocity_y.mul_add(timing.delta_seconds, body.y));
            let hit_player = self.players.iter().find_map(|(player_id, player)| {
                let dx = player.x - body.x;
                let dy = player.y - body.y;
                (dx.mul_add(dx, dy * dy) <= (AUTHORITATIVE_PLAYER_RADIUS + 8.0).powi(2))
                    .then_some((*player_id, player.hazard_gear))
            });
            if let Some((player_id, gear)) = hit_player {
                // The boulder shatters on the rig; the deflector turns aside its weight.
                self.entities.despawn(id);
                if self.damage_player(player_id, gear.boulder_damage(BOULDER_HIT_DAMAGE)) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                }
                return;
            }
            #[allow(
                clippy::cast_possible_truncation,
                reason = "bounded boulder coordinates intentionally map to terrain tiles"
            )]
            let tile = TilePosition {
                x: (body.x / TILE_SIZE).floor() as i32,
                y: (body.y / TILE_SIZE).floor() as i32,
            };
            if self.terrain.is_solid_at(tile) {
                self.entities.despawn(id);
                return;
            }
        }
        self.entities.update(id, |entity| {
            entity.body = body;
            if let EntityState::Boulder(boulder) = &mut entity.state {
                boulder.warning_seconds =
                    timing.rate.seconds_until(next_tick, state.warning_deadline);
                boulder.life = timing.rate.seconds_until(next_tick, state.expiry_tick);
            }
        });
    }

    /// A drop falls, bounces and slides under [`BodyParams::DROP`], and sleeps once it rests.
    fn advance_drop(&mut self, id: EntityId) {
        let delta_seconds = self.timing.delta_seconds;
        let sleep_after = u16::try_from(self.timing.ticks_for_millis(ENTITY_SLEEP_AFTER_MILLIS))
            .unwrap_or(u16::MAX);
        let terrain = &self.terrain;
        self.entities.update(id, |entity| {
            let params = BodyParams::DROP;
            let _report = physics::step_body(&mut entity.body, &params, terrain, delta_seconds);
            physics::settle(&mut entity.body, &params, terrain, sleep_after);
        });
    }

    /// Pushes every blast-feeling entity in reach of `blast` (in id order; waking it).
    pub(crate) fn push_entities_with_blast(&mut self, blast: &Blast) {
        let terrain = &self.terrain;
        for id in self.entities.ids_in_radius(blast.x, blast.y, blast.reach) {
            let feels = self
                .entities
                .get(id)
                .is_some_and(|entity| entity.kind().feels_blasts());
            if !feels {
                continue;
            }
            self.entities.update(id, |entity| {
                physics::apply_blast(&mut entity.body, blast, terrain, id.get() % 2 == 1)
            });
        }
    }

    /// Wakes sleeping entities around every changed tile.
    pub(super) fn wake_entities_near(&mut self, changes: &[TileChange]) {
        if self.entities.awake_count() == self.entities.len() {
            return;
        }
        for change in changes {
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let (x, y) = (
                (change.position.x as f32 + 0.5) * TILE_SIZE,
                (change.position.y as f32 + 0.5) * TILE_SIZE,
            );
            self.entities.wake_around(x, y, TERRAIN_WAKE_REACH);
        }
    }

    /// `(id, revision)` of the entities within `radius` tiles of `center` (tile units; `None`:
    /// every entity), in id order: what a client should hold. A client holding the same pairs
    /// is up to date.
    #[must_use]
    pub fn entity_revisions(
        &self,
        center: Option<(f32, f32)>,
        radius: Option<f32>,
    ) -> Vec<(EntityId, u64)> {
        match (center, radius) {
            (Some((cx, cy)), Some(radius)) => {
                let (x, y, reach) = (cx * TILE_SIZE, cy * TILE_SIZE, radius * TILE_SIZE);
                self.entities.revisions(
                    Some((x - reach, y - reach, x + reach, y + reach)),
                    |ex, ey| (ex - x).hypot(ey - y) <= reach,
                )
            }
            _ => self.entities.revisions(None, |_, _| true),
        }
    }

    /// One entity's replication record.
    #[must_use]
    pub fn entity_record(&self, id: EntityId) -> Option<Entity> {
        self.entities.get(id).cloned()
    }

    /// Applies replicated entity spawns, changes and despawns.
    pub fn apply_entity_delta(&mut self, delta: EntityDelta) {
        self.entities.apply_delta(delta);
    }

    /// Every entity as one delta (a full copy for a replica that has none).
    #[must_use]
    pub fn full_entity_delta(&self) -> EntityDelta {
        EntityDelta {
            updates: self.entities.iter().cloned().collect(),
            despawned: Vec::new(),
        }
    }
}

/// Spawns a gas cloud into `store` (for callers holding another borrow of the world).
pub(super) fn spawn_gas_cloud_into(
    store: &mut EntityStore,
    tick: SimulationTick,
    cloud: HazardCloud,
) -> EntityId {
    store.spawn(
        tick,
        Body::at(cloud.x, cloud.y),
        EntityState::GasCloud(GasCloudState {
            life: cloud.life,
            radius: cloud.radius,
        }),
        None,
    )
}
