//! Generic deterministic entities (expansion plan §3 F1).
//!
//! Every live world object that moves or expires on its own is an [`Entity`] in the world's
//! [`EntityStore`]: a stable [`EntityId`], a physics [`Body`], optional expiry, a replication
//! revision and per-kind [`EntityState`]. The store keeps entities in id order (ids only ever
//! grow), so every pass over them is deterministic, and maintains derived indices that are
//! never saved, hashed or replicated (they are rebuilt on load):
//!
//! * a spatial hash on the terrain chunk grid ([`SpatialCell`], [`ENTITY_CELL_TILES`] tiles a
//!   side) for area queries ([`EntityStore::ids_in_rect`], [`EntityStore::ids_in_radius`]);
//! * the awake set: entities whose kind never sleeps plus sleeping-capable bodies that are not
//!   resting ([`Body::asleep`]); a tick only visits awake entities;
//! * a per-kind set ([`EntityStore::ids_of`]);
//! * an expiry queue ([`Entity::expires_at`], [`EntityStore::take_expired`]).
//!
//! Gameplay mutates an entity only through [`EntityStore::update`], which bumps its
//! [`Entity::revision`] (the replication change marker) and keeps every index in step.
//!
//! The kinds themselves (what a gas cloud or a drop does each tick) live in
//! `world/entities.rs`; shared motion rules live in [`crate::physics`].

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    ids::{PlayerId, SimulationTick},
    item::ItemStack,
    terrain::{CHUNK_SIZE_TILES, TILE_SIZE},
};

/// Side, in tiles, of one spatial-hash cell: the terrain chunk grid.
pub const ENTITY_CELL_TILES: i32 = CHUNK_SIZE_TILES;

/// Stable identity of an entity for the whole session (never reused; saved and replicated).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EntityId(pub u64);

impl EntityId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Which kind of entity this is (the discriminant of [`EntityState`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EntityKind {
    /// A drifting gas cloud (world events, gas pockets, rare cargo).
    GasCloud,
    /// A falling boulder (loosened rock, cave-ins).
    Boulder,
    /// A floating item stack (WP-DROPS).
    Drop,
}

/// When in the tick an entity kind updates (`WorldState::simulate_tick` runs each phase at a
/// fixed point; within a phase entities update in id order).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum EntityPhase {
    /// Right after inputs, before hazard damage is read (gas clouds age here).
    Early,
    /// After drilling, bombs and hazard spawns, before quests read the tick.
    Late,
}

impl EntityKind {
    /// Every kind, in order.
    pub const ALL: [Self; 3] = [Self::GasCloud, Self::Boulder, Self::Drop];

    /// The tick phase this kind updates in.
    #[must_use]
    pub const fn phase(self) -> EntityPhase {
        match self {
            Self::GasCloud => EntityPhase::Early,
            Self::Boulder | Self::Drop => EntityPhase::Late,
        }
    }

    /// Whether a resting body of this kind may fall asleep (stop simulating until disturbed).
    #[must_use]
    pub const fn can_sleep(self) -> bool {
        matches!(self, Self::Drop)
    }

    /// Whether blasts push this kind ([`crate::physics::Blast`]).
    #[must_use]
    pub const fn feels_blasts(self) -> bool {
        matches!(self, Self::Drop)
    }
}

/// Position, motion and rest state shared by every entity, in world pixels.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Body {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    /// Position at the start of the last simulated step (presentation interpolation).
    pub previous_x: f32,
    pub previous_y: f32,
    /// Consecutive resting ticks (a sleep-capable body falls asleep after enough of them).
    pub rest_ticks: u16,
    /// Asleep: skipped by the tick until something wakes it (terrain change nearby, a blast,
    /// gameplay).
    pub asleep: bool,
}

impl Body {
    /// A body at rest at `(x, y)`.
    #[must_use]
    pub const fn at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            velocity_x: 0.0,
            velocity_y: 0.0,
            previous_x: x,
            previous_y: y,
            rest_ticks: 0,
            asleep: false,
        }
    }

    /// The same body moving at `(velocity_x, velocity_y)`.
    #[must_use]
    pub const fn moving(mut self, velocity_x: f32, velocity_y: f32) -> Self {
        self.velocity_x = velocity_x;
        self.velocity_y = velocity_y;
        self
    }

    /// Position interpolated between the last two steps (`alpha` 0: previous, 1: current).
    #[must_use]
    pub fn lerp_position(&self, alpha: f32) -> (f32, f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        (
            (self.x - self.previous_x).mul_add(alpha, self.previous_x),
            (self.y - self.previous_y).mul_add(alpha, self.previous_y),
        )
    }

    /// Wakes the body (it simulates again from the next update).
    pub const fn wake(&mut self) {
        self.asleep = false;
        self.rest_ticks = 0;
    }
}

/// A gas cloud's per-kind state.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GasCloudState {
    /// Seconds left; it is harmless while above six (still forming).
    pub life: f32,
    /// Radius in pixels; grows while the cloud lives.
    pub radius: f32,
}

/// A falling boulder's per-kind state.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoulderState {
    /// Seconds until it drops (presentation; the deadline is authoritative).
    pub warning_seconds: f32,
    /// Seconds until it crumbles (presentation; the deadline is authoritative).
    pub life: f32,
    /// Tick it starts falling at.
    pub warning_deadline: SimulationTick,
    /// Tick it crumbles at.
    pub expiry_tick: SimulationTick,
}

/// A floating item stack's per-kind state (WP-DROPS: pickup, magnet and merging live in
/// `world/drops.rs`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DropState {
    pub stack: ItemStack,
    /// The rig whose magnet is pulling it in (it flies free of gravity and rock toward that
    /// rig), if any.
    #[serde(default)]
    pub pulled_by: Option<PlayerId>,
    /// A rig that never auto-collects it (the rig that dumped it out of its hopper).
    #[serde(default)]
    pub ignored_by: Option<PlayerId>,
    /// It sits in a rig's pickup reach but that rig's bay or hopper had no room for it (drawn
    /// as a "full" drop; the rig was told once).
    #[serde(default)]
    pub blocked: bool,
}

impl DropState {
    /// A fresh drop of `stack` (not pulled, ignored or blocked).
    #[must_use]
    pub const fn new(stack: ItemStack) -> Self {
        Self {
            stack,
            pulled_by: None,
            ignored_by: None,
            blocked: false,
        }
    }
}

/// Per-kind entity state.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum EntityState {
    GasCloud(GasCloudState),
    Boulder(BoulderState),
    Drop(DropState),
}

impl EntityState {
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        match self {
            Self::GasCloud(_) => EntityKind::GasCloud,
            Self::Boulder(_) => EntityKind::Boulder,
            Self::Drop(_) => EntityKind::Drop,
        }
    }
}

/// One live entity. This whole struct is the replication record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entity {
    pub id: EntityId,
    pub body: Body,
    pub spawn_tick: SimulationTick,
    /// Tick the store despawns it at, if any ([`EntityStore::take_expired`]).
    pub expires_at: Option<SimulationTick>,
    /// Change marker, unique per change within the store (spawn or [`EntityStore::update`]):
    /// replication sends an entity again only when this differs from what the client has.
    pub revision: u64,
    pub state: EntityState,
}

impl Entity {
    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.state.kind()
    }
}

/// A cell of the spatial hash ([`ENTITY_CELL_TILES`] tiles a side). Ordered row-major (by `y`,
/// then `x`) so a row of cells is one contiguous range.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SpatialCell {
    pub y: i32,
    pub x: i32,
}

impl SpatialCell {
    /// The cell holding world pixel `(x, y)`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "world pixels saturate into tile coordinates"
    )]
    pub fn of(x: f32, y: f32) -> Self {
        let tile_x = (x / TILE_SIZE).floor() as i32;
        let tile_y = (y / TILE_SIZE).floor() as i32;
        Self {
            x: tile_x.div_euclid(ENTITY_CELL_TILES),
            y: tile_y.div_euclid(ENTITY_CELL_TILES),
        }
    }
}

/// Entities spawned, changed or gone since a receiver's baseline (relevance-filtered entity
/// replication: an entity leaving relevance is "despawned" for that receiver).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EntityDelta {
    /// New or changed entities, in id order (full records).
    pub updates: Vec<Entity>,
    /// Entities the receiver must drop.
    pub despawned: Vec<EntityId>,
}

impl EntityDelta {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.updates.is_empty() && self.despawned.is_empty()
    }
}

/// Derived lookup structures (never saved, hashed or replicated).
#[derive(Clone, Debug, Default)]
struct Indices {
    cells: BTreeMap<SpatialCell, Vec<EntityId>>,
    /// Each entity's cell, aligned with `EntityStore::entities`.
    cell_of: Vec<SpatialCell>,
    awake: BTreeSet<EntityId>,
    by_kind: BTreeMap<EntityKind, BTreeSet<EntityId>>,
    expiries: BTreeSet<(SimulationTick, EntityId)>,
}

impl Indices {
    fn insert(&mut self, entity: &Entity) -> SpatialCell {
        let cell = SpatialCell::of(entity.body.x, entity.body.y);
        self.cells.entry(cell).or_default().push(entity.id);
        if is_awake(entity) {
            self.awake.insert(entity.id);
        }
        self.by_kind
            .entry(entity.kind())
            .or_default()
            .insert(entity.id);
        if let Some(tick) = entity.expires_at {
            self.expiries.insert((tick, entity.id));
        }
        cell
    }

    fn remove(&mut self, entity: &Entity, cell: SpatialCell) {
        if let Some(bucket) = self.cells.get_mut(&cell) {
            bucket.retain(|id| *id != entity.id);
            if bucket.is_empty() {
                self.cells.remove(&cell);
            }
        }
        self.awake.remove(&entity.id);
        if let Some(set) = self.by_kind.get_mut(&entity.kind()) {
            set.remove(&entity.id);
            if set.is_empty() {
                self.by_kind.remove(&entity.kind());
            }
        }
        if let Some(tick) = entity.expires_at {
            self.expiries.remove(&(tick, entity.id));
        }
    }
}

const fn is_awake(entity: &Entity) -> bool {
    !(entity.kind().can_sleep() && entity.body.asleep)
}

/// The world's entities, in id order, with derived spatial / awake / kind / expiry indices.
#[derive(Clone, Debug, Default)]
pub struct EntityStore {
    next_id: u64,
    next_revision: u64,
    entities: Vec<Entity>,
    indices: Indices,
}

/// Serialized form of an [`EntityStore`] (its indices are rebuilt on load).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct EntityStoreData {
    next_id: u64,
    next_revision: u64,
    entities: Vec<Entity>,
}

impl Serialize for EntityStore {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Borrowed<'a> {
            next_id: u64,
            next_revision: u64,
            entities: &'a [Entity],
        }
        Borrowed {
            next_id: self.next_id,
            next_revision: self.next_revision,
            entities: &self.entities,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EntityStore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let data = EntityStoreData::deserialize(deserializer)?;
        Ok(Self::from_parts(
            data.next_id,
            data.next_revision,
            data.entities,
        ))
    }
}

impl PartialEq for EntityStore {
    fn eq(&self, other: &Self) -> bool {
        self.next_id == other.next_id
            && self.next_revision == other.next_revision
            && self.entities == other.entities
    }
}

impl EntityStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a store from saved parts (sorts by id, drops duplicate ids, keeps the counters
    /// ahead of every entity, rebuilds indices).
    #[must_use]
    pub fn from_parts(next_id: u64, next_revision: u64, mut entities: Vec<Entity>) -> Self {
        entities.sort_by_key(|entity| entity.id);
        entities.dedup_by_key(|entity| entity.id);
        let next_id = entities
            .last()
            .map_or(next_id, |last| next_id.max(last.id.0.saturating_add(1)));
        let next_revision = entities.iter().fold(next_revision, |next, entity| {
            next.max(entity.revision.saturating_add(1))
        });
        let mut store = Self {
            next_id,
            next_revision,
            entities,
            indices: Indices::default(),
        };
        store.rebuild_indices();
        store
    }

    fn rebuild_indices(&mut self) {
        let mut indices = Indices::default();
        indices.cell_of = self
            .entities
            .iter()
            .map(|entity| indices.insert(entity))
            .collect();
        self.indices = indices;
    }

    /// The id the next spawn gets.
    #[must_use]
    pub const fn next_id(&self) -> u64 {
        self.next_id
    }

    /// The revision the next change gets.
    #[must_use]
    pub const fn next_revision(&self) -> u64 {
        self.next_revision
    }

    /// Sets both counters (a replica following its authority's counters).
    pub const fn set_counters(&mut self, next_id: u64, next_revision: u64) {
        self.next_id = next_id;
        self.next_revision = next_revision;
    }

    const fn take_revision(&mut self) -> u64 {
        let revision = self.next_revision;
        self.next_revision = self.next_revision.saturating_add(1);
        revision
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Entities that will update next tick.
    #[must_use]
    pub fn awake_count(&self) -> usize {
        self.indices.awake.len()
    }

    /// Every entity, in id order.
    #[must_use]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Entity> + ExactSizeIterator {
        self.entities.iter()
    }

    fn position(&self, id: EntityId) -> Option<usize> {
        self.entities
            .binary_search_by_key(&id, |entity| entity.id)
            .ok()
    }

    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.position(id).map(|index| &self.entities[index])
    }

    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.position(id).is_some()
    }

    /// Spawns an entity at `tick` and returns its id.
    pub fn spawn(
        &mut self,
        tick: SimulationTick,
        body: Body,
        state: EntityState,
        expires_at: Option<SimulationTick>,
    ) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let entity = Entity {
            id,
            body,
            spawn_tick: tick,
            expires_at,
            revision: self.take_revision(),
            state,
        };
        let cell = self.indices.insert(&entity);
        // Ids grow, so a spawn always appends in order.
        self.entities.push(entity);
        self.indices.cell_of.push(cell);
        id
    }

    /// Removes an entity, returning it.
    pub fn despawn(&mut self, id: EntityId) -> Option<Entity> {
        let index = self.position(id)?;
        let entity = self.entities.remove(index);
        let cell = self.indices.cell_of.remove(index);
        self.indices.remove(&entity, cell);
        Some(entity)
    }

    /// Mutates one entity: gives it a new revision and keeps every index in step.
    pub fn update<R>(&mut self, id: EntityId, change: impl FnOnce(&mut Entity) -> R) -> Option<R> {
        let index = self.position(id)?;
        let before_awake = is_awake(&self.entities[index]);
        let before_expiry = self.entities[index].expires_at;
        let revision = self.take_revision();
        let entity = &mut self.entities[index];
        let result = change(entity);
        entity.id = id;
        entity.revision = revision;
        let entity = &self.entities[index];
        let cell = SpatialCell::of(entity.body.x, entity.body.y);
        let old_cell = self.indices.cell_of[index];
        if cell != old_cell {
            if let Some(bucket) = self.indices.cells.get_mut(&old_cell) {
                bucket.retain(|other| *other != id);
                if bucket.is_empty() {
                    self.indices.cells.remove(&old_cell);
                }
            }
            self.indices.cells.entry(cell).or_default().push(id);
            self.indices.cell_of[index] = cell;
        }
        let awake = is_awake(entity);
        if awake != before_awake {
            if awake {
                self.indices.awake.insert(id);
            } else {
                self.indices.awake.remove(&id);
            }
        }
        if entity.expires_at != before_expiry {
            if let Some(tick) = before_expiry {
                self.indices.expiries.remove(&(tick, id));
            }
            if let Some(tick) = entity.expires_at {
                self.indices.expiries.insert((tick, id));
            }
        }
        Some(result)
    }

    /// Ids of every entity of `kind`, in id order.
    #[must_use]
    pub fn ids_of(&self, kind: EntityKind) -> impl DoubleEndedIterator<Item = EntityId> + '_ {
        self.indices
            .by_kind
            .get(&kind)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    /// Entities of `kind`, in id order.
    pub fn of_kind(&self, kind: EntityKind) -> impl Iterator<Item = &Entity> + '_ {
        self.ids_of(kind).filter_map(|id| self.get(id))
    }

    /// Number of entities of `kind`.
    #[must_use]
    pub fn count_of(&self, kind: EntityKind) -> usize {
        self.indices.by_kind.get(&kind).map_or(0, BTreeSet::len)
    }

    /// Awake entities updating in `phase`, in id order (a snapshot: updates may spawn or
    /// despawn freely while it is walked).
    #[must_use]
    pub fn awake_in_phase(&self, phase: EntityPhase) -> Vec<EntityId> {
        self.indices
            .awake
            .iter()
            .copied()
            .filter(|id| {
                self.get(*id)
                    .is_some_and(|entity| entity.kind().phase() == phase)
            })
            .collect()
    }

    /// Removes every entity whose expiry is at or before `tick` and returns them in
    /// (expiry, id) order.
    #[allow(
        clippy::needless_collect,
        reason = "the due list releases the index borrow before despawning"
    )]
    pub fn take_expired(&mut self, tick: SimulationTick) -> Vec<Entity> {
        let due: Vec<EntityId> = self
            .indices
            .expiries
            .iter()
            .take_while(|(expiry, _)| *expiry <= tick)
            .map(|(_, id)| *id)
            .collect();
        let mut removed = self.despawn_many(&due.iter().copied().collect());
        // `despawn_many` returns id order; callers get (expiry, id) order.
        removed.sort_by_key(|entity| (entity.expires_at, entity.id));
        removed
    }

    /// Removes every entity in `ids` in one compacting pass (a burst of expiries or pickups
    /// costs one walk over the store, not one shift per entity) and returns them in id order.
    pub fn despawn_many(&mut self, ids: &BTreeSet<EntityId>) -> Vec<Entity> {
        if ids.is_empty() {
            return Vec::new();
        }
        if ids.len() == 1 {
            return ids.iter().filter_map(|id| self.despawn(*id)).collect();
        }
        let mut removed = Vec::new();
        let entities = std::mem::take(&mut self.entities);
        let cells = std::mem::take(&mut self.indices.cell_of);
        self.entities
            .reserve(entities.len().saturating_sub(ids.len()));
        self.indices
            .cell_of
            .reserve(cells.len().saturating_sub(ids.len()));
        for (entity, cell) in entities.into_iter().zip(cells) {
            if ids.contains(&entity.id) {
                self.indices.remove(&entity, cell);
                removed.push(entity);
            } else {
                self.entities.push(entity);
                self.indices.cell_of.push(cell);
            }
        }
        removed
    }

    /// Cells overlapping the pixel rectangle.
    fn cells_in_rect(
        &self,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    ) -> impl Iterator<Item = &Vec<EntityId>> + '_ {
        let low = SpatialCell::of(left, top);
        let high = SpatialCell::of(right, bottom);
        (low.y..=high.y).flat_map(move |y| {
            self.indices
                .cells
                .range(SpatialCell { x: low.x, y }..=SpatialCell { x: high.x, y })
                .map(|(_, ids)| ids)
        })
    }

    /// Ids of entities inside the pixel rectangle (inclusive), in id order.
    #[must_use]
    pub fn ids_in_rect(&self, left: f32, top: f32, right: f32, bottom: f32) -> Vec<EntityId> {
        let mut ids: Vec<EntityId> = self
            .cells_in_rect(left, top, right, bottom)
            .flatten()
            .copied()
            .filter(|id| {
                self.get(*id).is_some_and(|entity| {
                    (left..=right).contains(&entity.body.x)
                        && (top..=bottom).contains(&entity.body.y)
                })
            })
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Ids of entities within `radius` pixels of `(x, y)`, in id order.
    #[must_use]
    pub fn ids_in_radius(&self, x: f32, y: f32, radius: f32) -> Vec<EntityId> {
        let mut ids = self.ids_in_rect(x - radius, y - radius, x + radius, y + radius);
        ids.retain(|id| {
            self.get(*id).is_some_and(|entity| {
                let dx = entity.body.x - x;
                let dy = entity.body.y - y;
                dx.mul_add(dx, dy * dy) <= radius * radius
            })
        });
        ids
    }

    /// The entity nearest to `(x, y)` within `radius` pixels that `accept` takes (ties: the
    /// lower id), found without allocating: one pass over the overlapping cells. The answer
    /// does not depend on the order entities sit in the index (WP-DROPS merging runs it for
    /// every drop that lands).
    #[must_use]
    pub fn nearest_in_radius(
        &self,
        x: f32,
        y: f32,
        radius: f32,
        accept: impl Fn(&Entity) -> bool,
    ) -> Option<EntityId> {
        let mut best: Option<(f32, EntityId)> = None;
        for id in self
            .cells_in_rect(x - radius, y - radius, x + radius, y + radius)
            .flatten()
        {
            let Some(entity) = self.get(*id) else {
                continue;
            };
            let dx = entity.body.x - x;
            let dy = entity.body.y - y;
            let distance = dx.mul_add(dx, dy * dy);
            if distance > radius * radius {
                continue;
            }
            let better = best.is_none_or(|(best_distance, best_id)| {
                distance < best_distance || (distance <= best_distance && *id < best_id)
            });
            if better && accept(entity) {
                best = Some((distance, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Wakes every sleeping entity within `reach` pixels (a square) of world pixel `(x, y)` (a
    /// tile changed under it, a blast went off); returns how many woke.
    pub fn wake_around(&mut self, x: f32, y: f32, reach: f32) -> usize {
        let sleeping: Vec<EntityId> = self
            .ids_in_rect(x - reach, y - reach, x + reach, y + reach)
            .into_iter()
            .filter(|id| self.get(*id).is_some_and(|entity| !is_awake(entity)))
            .collect();
        for id in &sleeping {
            self.update(*id, |entity| entity.body.wake());
        }
        sleeping.len()
    }

    /// `(id, revision)` of every entity whose position satisfies `relevant`, looked up through
    /// the cells overlapping the pixel rectangle (`None`: every entity). In id order.
    #[must_use]
    pub fn revisions(
        &self,
        area: Option<(f32, f32, f32, f32)>,
        relevant: impl Fn(f32, f32) -> bool,
    ) -> Vec<(EntityId, u64)> {
        let mut out: Vec<(EntityId, u64)> = match area {
            None => self
                .entities
                .iter()
                .filter(|entity| relevant(entity.body.x, entity.body.y))
                .map(|entity| (entity.id, entity.revision))
                .collect(),
            Some((left, top, right, bottom)) => self
                .cells_in_rect(left, top, right, bottom)
                .flatten()
                .filter_map(|id| self.get(*id))
                .filter(|entity| relevant(entity.body.x, entity.body.y))
                .map(|entity| (entity.id, entity.revision))
                .collect(),
        };
        out.sort_unstable_by_key(|(id, _)| *id);
        out
    }

    /// Applies a replicated delta: inserts or replaces each update, removes each despawn.
    pub fn apply_delta(&mut self, delta: EntityDelta) {
        let EntityDelta { updates, despawned } = delta;
        self.despawn_many(&despawned.into_iter().collect());
        for entity in updates {
            self.next_id = self.next_id.max(entity.id.0.saturating_add(1));
            self.next_revision = self.next_revision.max(entity.revision.saturating_add(1));
            match self
                .entities
                .binary_search_by_key(&entity.id, |other| other.id)
            {
                Ok(index) => {
                    let old = std::mem::replace(&mut self.entities[index], entity);
                    let cell = self.indices.cell_of[index];
                    self.indices.remove(&old, cell);
                    self.indices.cell_of[index] = self.indices.insert(&self.entities[index]);
                }
                Err(index) => {
                    let cell = self.indices.insert(&entity);
                    self.entities.insert(index, entity);
                    self.indices.cell_of.insert(index, cell);
                }
            }
        }
    }

    /// Keeps only the entities `keep` accepts (a replica dropping what left relevance).
    pub fn retain(&mut self, mut keep: impl FnMut(&Entity) -> bool) {
        let gone: BTreeSet<EntityId> = self
            .entities
            .iter()
            .filter(|entity| !keep(entity))
            .map(|entity| entity.id)
            .collect();
        self.despawn_many(&gone);
    }

    /// Presentation positions interpolated between two snapshots of the store (for example
    /// the previous and current replicated frames): entities present in both move from their
    /// old to their new position, new ones appear in place. In id order.
    #[must_use]
    pub fn interpolated_positions(&self, previous: &Self, alpha: f32) -> Vec<(EntityId, f32, f32)> {
        let alpha = alpha.clamp(0.0, 1.0);
        self.entities
            .iter()
            .map(|entity| {
                let (x, y) =
                    previous
                        .get(entity.id)
                        .map_or((entity.body.x, entity.body.y), |old| {
                            (
                                (entity.body.x - old.body.x).mul_add(alpha, old.body.x),
                                (entity.body.y - old.body.y).mul_add(alpha, old.body.y),
                            )
                        });
                (entity.id, x, y)
            })
            .collect()
    }

    /// Checks every derived index against a fresh rebuild (tests).
    #[cfg(test)]
    pub(crate) fn assert_indices_consistent(&self) {
        let rebuilt = Self::from_parts(self.next_id, self.next_revision, self.entities.clone());
        let normalize = |cells: &BTreeMap<SpatialCell, Vec<EntityId>>| {
            cells
                .iter()
                .map(|(cell, ids)| {
                    let mut ids = ids.clone();
                    ids.sort_unstable();
                    (*cell, ids)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            normalize(&self.indices.cells),
            normalize(&rebuilt.indices.cells)
        );
        assert_eq!(self.indices.cell_of, rebuilt.indices.cell_of);
        assert_eq!(self.indices.awake, rebuilt.indices.awake);
        assert_eq!(self.indices.by_kind, rebuilt.indices.by_kind);
        assert_eq!(self.indices.expiries, rebuilt.indices.expiries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{item::ItemKind, terrain::MineralKind};

    fn drop_state() -> EntityState {
        EntityState::Drop(DropState::new(ItemStack::new(
            ItemKind::Mineral(MineralKind::Gold),
            1,
        )))
    }

    fn cloud(life: f32) -> EntityState {
        EntityState::GasCloud(GasCloudState { life, radius: 8.0 })
    }

    #[test]
    fn spawns_get_increasing_ids_and_indices_follow_every_change() {
        let mut store = EntityStore::new();
        let tick = SimulationTick::new(5);
        let a = store.spawn(tick, Body::at(10.0, 10.0), drop_state(), None);
        let b = store.spawn(tick, Body::at(2_000.0, 900.0), cloud(3.0), None);
        let c = store.spawn(
            tick,
            Body::at(40.0, 12.0),
            drop_state(),
            Some(SimulationTick::new(9)),
        );
        assert!(a < b && b < c);
        assert_eq!(store.ids_in_radius(20.0, 10.0, 40.0), vec![a, c]);
        assert_eq!(
            store.ids_of(EntityKind::Drop).collect::<Vec<_>>(),
            vec![a, c]
        );
        store.update(a, |entity| {
            entity.body.x = 1_990.0;
            entity.body.y = 905.0;
            entity.body.asleep = true;
        });
        assert_eq!(store.get(a).map(|entity| entity.revision), Some(3));
        assert_eq!(store.ids_in_radius(2_000.0, 900.0, 30.0), vec![a, b]);
        assert_eq!(store.awake_count(), 2, "the asleep drop left the awake set");
        store.assert_indices_consistent();
        let expired = store.take_expired(SimulationTick::new(9));
        assert_eq!(
            expired.iter().map(|entity| entity.id).collect::<Vec<_>>(),
            vec![c]
        );
        assert!(store.despawn(b).is_some());
        assert_eq!(store.len(), 1);
        store.assert_indices_consistent();
        // A cloud never sleeps, even if flagged.
        let d = store.spawn(
            tick,
            Body {
                asleep: true,
                ..Body::at(0.0, 0.0)
            },
            cloud(1.0),
            None,
        );
        assert!(store.awake_in_phase(EntityPhase::Early).contains(&d));
        assert_eq!(store.wake_around(1_995.0, 905.0, 48.0), 1);
        store.assert_indices_consistent();
    }

    #[test]
    fn indices_and_area_queries_match_brute_force_under_random_churn() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        #[allow(clippy::cast_precision_loss, reason = "test coordinates")]
        let coordinate = move |value: u64| (value % 6_000) as f32 - 1_000.0;
        let mut store = EntityStore::new();
        for step in 0..3_000_u64 {
            match next() % 5 {
                0 | 1 => {
                    let body = Body::at(coordinate(next()), coordinate(next()));
                    let state = if next() % 2 == 0 {
                        drop_state()
                    } else {
                        cloud(1.0)
                    };
                    let expiry = (next() % 3 == 0).then(|| SimulationTick::new(step + next() % 50));
                    store.spawn(SimulationTick::new(step), body, state, expiry);
                }
                2 | 3 if !store.is_empty() => {
                    let pick = usize::try_from(next()).unwrap_or(0) % store.len();
                    let id = store
                        .iter()
                        .nth(pick)
                        .map(|entity| entity.id)
                        .expect("picked");
                    let (x, y, asleep) = (coordinate(next()), coordinate(next()), next() % 2 == 0);
                    store.update(id, |entity| {
                        entity.body.x = x;
                        entity.body.y = y;
                        entity.body.asleep = asleep;
                    });
                }
                _ => {
                    let _ = store.take_expired(SimulationTick::new(step));
                    let first = store.iter().next().map(|entity| entity.id);
                    if let Some(id) = first {
                        store.despawn(id);
                    }
                    // A burst removal of a few scattered entities (one compacting pass).
                    if store.len() > 3 {
                        let burst: BTreeSet<EntityId> = (0..3)
                            .filter_map(|_| {
                                let pick = usize::try_from(next()).unwrap_or(0) % store.len();
                                store.iter().nth(pick).map(|entity| entity.id)
                            })
                            .collect();
                        let removed = store.despawn_many(&burst);
                        assert_eq!(removed.len(), burst.len());
                        assert!(burst.iter().all(|id| !store.contains(*id)));
                    }
                }
            }
            if step % 250 == 0 {
                store.assert_indices_consistent();
                let (left, top) = (coordinate(next()), coordinate(next()));
                let (right, bottom) = (left + 900.0, top + 700.0);
                let brute: Vec<EntityId> = store
                    .iter()
                    .filter(|entity| {
                        (left..=right).contains(&entity.body.x)
                            && (top..=bottom).contains(&entity.body.y)
                    })
                    .map(|entity| entity.id)
                    .collect();
                assert_eq!(store.ids_in_rect(left, top, right, bottom), brute);
            }
        }
        store.assert_indices_consistent();
    }

    #[test]
    fn stores_round_trip_and_rebuild_their_indices() {
        let mut store = EntityStore::new();
        for index in 0..50_u16 {
            let x = f32::from(index) * 37.0;
            store.spawn(
                SimulationTick::new(1),
                Body::at(x, x * 0.5),
                drop_state(),
                None,
            );
        }
        store.despawn(EntityId(7));
        let json = serde_json::to_string(&store).expect("json");
        let back: EntityStore = serde_json::from_str(&json).expect("back");
        assert_eq!(back, store);
        back.assert_indices_consistent();
        let bytes = postcard::to_allocvec(&store).expect("postcard");
        let back: EntityStore = postcard::from_bytes(&bytes).expect("back");
        assert_eq!(back, store);
        assert_eq!(
            back.ids_in_rect(0.0, 0.0, 400.0, 400.0),
            store.ids_in_rect(0.0, 0.0, 400.0, 400.0)
        );
    }

    #[test]
    fn deltas_rebuild_an_identical_replica() {
        let mut authority = EntityStore::new();
        let mut replica = EntityStore::new();
        let tick = SimulationTick::new(1);
        let ids: Vec<EntityId> = (0..10_u16)
            .map(|index| {
                authority.spawn(
                    tick,
                    Body::at(f32::from(index) * 50.0, 0.0),
                    drop_state(),
                    None,
                )
            })
            .collect();
        replica.apply_delta(EntityDelta {
            updates: authority.iter().cloned().collect(),
            despawned: Vec::new(),
        });
        authority.update(ids[3], |entity| entity.body.y = 99.0);
        authority.despawn(ids[5]);
        let changed = authority.get(ids[3]).cloned().expect("still there");
        replica.apply_delta(EntityDelta {
            updates: vec![changed],
            despawned: vec![ids[5]],
        });
        assert_eq!(replica, authority);
        replica.assert_indices_consistent();
        let positions = authority.interpolated_positions(&EntityStore::new(), 0.5);
        assert_eq!(positions.len(), 9);
    }
}
