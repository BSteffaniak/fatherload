//! A small deterministic mining simulation implementing [`NetSimulation`].
//!
//! It exists only to exercise netcode end to end: players fly, fall and drill
//! through a tile grid (op-log terrain), drop bombs (per-tick entities), and
//! sell/buy through a per-player wallet and a shared market (on-change
//! sections). Wallets are private: only their owner's client receives them.
//! All gameplay math is integer fixed point, and every constant is authored
//! per second and converted with the session tick rate.

use std::collections::BTreeMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::hashing::stable_hash;
use crate::ids::{ActionId, PlayerId, Tick};
use crate::params::SessionParams;
use crate::simulation::{
    ActionEnvelope, NetSimulation, PlayerTickInput, Pose, Relevance, TickEvent, TickInputs,
    TickOutput,
};

/// Fixed-point units per tile.
pub const UNITS_PER_TILE: i64 = 1024;
/// Tiles of horizontal space per player column.
pub const COLUMN_WIDTH: i32 = 8;
/// Empty tile.
pub const TILE_EMPTY: u8 = 0;
/// Diggable dirt.
pub const TILE_DIRT: u8 = 1;
/// Undiggable bedrock.
pub const TILE_BEDROCK: u8 = 2;
const SURFACE_ROWS: i32 = 4;
/// Width and height, in tiles, of a terrain relevance chunk.
pub const TOY_CHUNK_TILES: i32 = 8;

/// Event kind discriminants.
pub mod event_kind {
    /// An action was applied.
    pub const ACTION_APPLIED: u32 = 1;
    /// A tile was dug.
    pub const TILE_DUG: u32 = 2;
    /// A bomb exploded.
    pub const EXPLODED: u32 = 3;
}

/// Per-tick constants derived from per-second rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyRules {
    /// Tick rate the constants were derived for.
    pub tick_rate_hz: u32,
    /// Horizontal speed per tick (units).
    pub move_speed: i64,
    /// Gravity per tick squared (units).
    pub gravity: i64,
    /// Thrust per tick squared (units).
    pub thrust: i64,
    /// Max vertical speed per tick (units).
    pub max_fall: i64,
    /// Ticks to dig one tile.
    pub dig_ticks: u32,
    /// Bomb fuse in ticks.
    pub bomb_fuse_ticks: u32,
}

impl ToyRules {
    /// Derives per-tick values from per-second authoring.
    #[must_use]
    pub fn new(params: &SessionParams) -> Self {
        let rate = i64::try_from(params.rate()).unwrap_or(i64::MAX);
        let per_tick = |tiles_per_sec: i64| (tiles_per_sec * UNITS_PER_TILE / rate).max(1);
        let rate_squared = rate * rate;
        let per_tick2 =
            |tiles_per_sec2: i64| (tiles_per_sec2 * UNITS_PER_TILE / rate_squared).max(1);
        Self {
            tick_rate_hz: params.tick_rate_hz,
            move_speed: per_tick(5),
            gravity: per_tick2(20),
            thrust: per_tick2(45),
            max_fall: per_tick(10),
            dig_ticks: u32::try_from(params.ms_to_ticks_ceil(150))
                .unwrap_or(u32::MAX)
                .max(1),
            bomb_fuse_ticks: u32::try_from(params.ms_to_ticks_ceil(1000))
                .unwrap_or(u32::MAX)
                .max(1),
        }
    }
}

/// Held controls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyControls {
    /// -1 left, 0, +1 right.
    pub move_x: i8,
    /// -1 thrust up, 0, +1 down.
    pub move_y: i8,
    /// Drill into solid tiles in the move direction.
    pub drill: bool,
}

/// One-shot actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToyAction {
    /// Drop a bomb (predicted).
    Bomb,
    /// Sell cargo (economy: not predicted).
    Sell,
    /// Buy an upgrade (economy: not predicted).
    Buy,
}

/// Events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToyEvent {
    /// An action was applied.
    ActionApplied {
        /// Its id.
        id: ActionId,
        /// What it was.
        action: ToyAction,
    },
    /// A tile was dug.
    TileDug {
        /// Column.
        x: i32,
        /// Row.
        y: i32,
    },
    /// A bomb exploded.
    Exploded {
        /// Column.
        x: i32,
        /// Row.
        y: i32,
    },
}

/// Per-tick player state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyPlayer {
    /// Position (units).
    pub x: i64,
    /// Position (units, +y is down).
    pub y: i64,
    /// Velocity (units per tick).
    pub vx: i64,
    /// Velocity (units per tick).
    pub vy: i64,
    /// Tile being drilled.
    pub dig: Option<(i32, i32)>,
    /// Ticks spent on `dig`.
    pub dig_progress: u32,
    /// Tiles carried.
    pub cargo: u32,
}

/// A live bomb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyBomb {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
    /// Ticks until it explodes.
    pub fuse: u32,
    /// Who dropped it.
    pub owner: PlayerId,
}

/// Per-tick entity section.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyEntities {
    /// Next entity id.
    pub next_id: u64,
    /// Relevant bombs.
    pub bombs: Vec<(u64, ToyBomb)>,
}

/// Per-player on-change section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyWallet {
    /// Money.
    pub credits: u64,
    /// Upgrades bought.
    pub upgrades: u32,
    /// Actions applied for this player.
    pub actions_applied: u64,
}

/// Shared on-change section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyMarket {
    /// Price per tile of cargo.
    pub price: u64,
    /// Sales so far.
    pub sales: u64,
}

impl Default for ToyMarket {
    fn default() -> Self {
        Self {
            price: 10,
            sales: 0,
        }
    }
}

/// Op-log entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToyTileOp {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
    /// New tile.
    pub tile: u8,
}

/// On-change section key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ToySectionKey {
    /// Shared market.
    Market,
    /// One player's wallet.
    Wallet(PlayerId),
}

/// On-change section payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToySection {
    /// Market.
    Market(ToyMarket),
    /// Wallet.
    Wallet(ToyWallet),
}

/// The toy world.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToySim {
    rules: ToyRules,
    tick: Tick,
    width: i32,
    height: i32,
    tiles: Vec<u8>,
    players: BTreeMap<PlayerId, ToyPlayer>,
    bombs: BTreeMap<u64, ToyBomb>,
    next_entity: u64,
    wallets: BTreeMap<PlayerId, ToyWallet>,
    market: ToyMarket,
}

impl ToySim {
    /// Creates a world with `columns` player columns.
    #[must_use]
    pub fn new(params: &SessionParams, columns: u32, height: i32) -> Self {
        let width = COLUMN_WIDTH * i32::try_from(columns.max(1)).unwrap_or(i32::MAX / COLUMN_WIDTH);
        let height = height.max(SURFACE_ROWS + 2);
        let mut tiles = vec![TILE_EMPTY; usize::try_from(width * height).unwrap_or(0)];
        for y in SURFACE_ROWS..height {
            for x in 0..width {
                let tile = if y == height - 1 {
                    TILE_BEDROCK
                } else {
                    TILE_DIRT
                };
                tiles[usize::try_from(y * width + x).unwrap_or(0)] = tile;
            }
        }
        Self {
            rules: ToyRules::new(params),
            tick: Tick(0),
            width,
            height,
            tiles,
            players: BTreeMap::new(),
            bombs: BTreeMap::new(),
            next_entity: 1,
            wallets: BTreeMap::new(),
            market: ToyMarket::default(),
        }
    }

    /// Derived rules.
    #[must_use]
    pub const fn rules(&self) -> &ToyRules {
        &self.rules
    }

    /// A player's state.
    #[must_use]
    pub fn player(&self, id: PlayerId) -> Option<&ToyPlayer> {
        self.players.get(&id)
    }

    /// A player's wallet.
    #[must_use]
    pub fn wallet(&self, id: PlayerId) -> Option<&ToyWallet> {
        self.wallets.get(&id)
    }

    /// The market.
    #[must_use]
    pub const fn market(&self) -> &ToyMarket {
        &self.market
    }

    /// Live bombs.
    #[must_use]
    pub fn bomb_count(&self) -> usize {
        self.bombs.len()
    }

    /// Tile at a cell (out of bounds is bedrock).
    #[must_use]
    pub fn tile(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return TILE_BEDROCK;
        }
        self.tiles[usize::try_from(y * self.width + x).unwrap_or(0)]
    }

    /// Number of empty tiles below the surface (dig progress indicator).
    #[must_use]
    #[allow(clippy::naive_bytecount)]
    pub fn dug_tiles(&self) -> usize {
        let start = usize::try_from(SURFACE_ROWS * self.width).unwrap_or(0);
        self.tiles[start..]
            .iter()
            .filter(|t| **t == TILE_EMPTY)
            .count()
    }

    fn set_tile(&mut self, x: i32, y: i32, tile: u8) {
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.tiles[usize::try_from(y * self.width + x).unwrap_or(0)] = tile;
        }
    }

    fn spawn(&self, id: PlayerId) -> ToyPlayer {
        let columns = (self.width / COLUMN_WIDTH).max(1);
        let column = i64::from(id.0 % u32::try_from(columns).unwrap_or(1));
        ToyPlayer {
            x: (column * i64::from(COLUMN_WIDTH) + i64::from(COLUMN_WIDTH / 2)) * UNITS_PER_TILE
                + UNITS_PER_TILE / 2,
            y: i64::from(SURFACE_ROWS - 1) * UNITS_PER_TILE + UNITS_PER_TILE / 2,
            ..ToyPlayer::default()
        }
    }

    fn cell(units: i64) -> i32 {
        i32::try_from(units.div_euclid(UNITS_PER_TILE)).unwrap_or(i32::MAX)
    }

    fn step_player(
        &mut self,
        id: PlayerId,
        controls: ToyControls,
        events: &mut Vec<TickEvent<ToyEvent>>,
        ops: &mut Vec<ToyTileOp>,
    ) {
        let rules = self.rules;
        let Some(mut p) = self.players.get(&id).copied() else {
            return;
        };
        let desired = i64::from(controls.move_x.signum()) * rules.move_speed;
        p.vx += (desired - p.vx) / 4;
        if desired == 0 && p.vx.abs() < 4 {
            p.vx = 0;
        }
        if controls.move_y < 0 {
            p.vy -= rules.thrust;
        } else {
            p.vy += rules.gravity;
        }
        p.vy = p.vy.clamp(-rules.max_fall, rules.max_fall);

        let mut dig_target = None;
        let cy = Self::cell(p.y);
        let nx = p.x + p.vx;
        if self.tile(Self::cell(nx), cy) == TILE_EMPTY {
            p.x = nx;
        } else {
            if controls.drill && controls.move_x != 0 && self.tile(Self::cell(nx), cy) == TILE_DIRT
            {
                dig_target = Some((Self::cell(nx), cy));
            }
            p.vx = 0;
        }
        let cx = Self::cell(p.x);
        let ny = p.y + p.vy;
        if self.tile(cx, Self::cell(ny)) == TILE_EMPTY && ny >= 0 {
            p.y = ny;
        } else {
            if p.vy > 0
                && controls.drill
                && controls.move_y > 0
                && self.tile(cx, Self::cell(ny)) == TILE_DIRT
            {
                dig_target = Some((cx, Self::cell(ny)));
            }
            p.vy = 0;
        }

        if let Some(target) = dig_target {
            if p.dig == Some(target) {
                p.dig_progress += 1;
            } else {
                p.dig = Some(target);
                p.dig_progress = 1;
            }
            if p.dig_progress >= rules.dig_ticks {
                self.set_tile(target.0, target.1, TILE_EMPTY);
                ops.push(ToyTileOp {
                    x: target.0,
                    y: target.1,
                    tile: TILE_EMPTY,
                });
                events.push(TickEvent {
                    player: Some(id),
                    kind: event_kind::TILE_DUG,
                    payload: ToyEvent::TileDug {
                        x: target.0,
                        y: target.1,
                    },
                });
                p.cargo += 1;
                p.dig = None;
                p.dig_progress = 0;
            }
        } else {
            p.dig = None;
            p.dig_progress = 0;
        }
        self.players.insert(id, p);
    }

    fn apply_action(
        &mut self,
        id: PlayerId,
        envelope: &ActionEnvelope<ToyAction>,
        events: &mut Vec<TickEvent<ToyEvent>>,
    ) {
        let Some(player) = self.players.get(&id).copied() else {
            return;
        };
        let wallet = self.wallets.entry(id).or_default();
        match envelope.action {
            ToyAction::Bomb => {
                let bomb = ToyBomb {
                    x: Self::cell(player.x),
                    y: Self::cell(player.y),
                    fuse: self.rules.bomb_fuse_ticks,
                    owner: id,
                };
                self.bombs.insert(self.next_entity, bomb);
                self.next_entity += 1;
            }
            ToyAction::Sell => {
                wallet.credits += u64::from(player.cargo) * self.market.price;
                self.market.sales += 1;
                self.market.price = 10 + self.market.sales % 5;
                if let Some(p) = self.players.get_mut(&id) {
                    p.cargo = 0;
                }
            }
            ToyAction::Buy => {
                if wallet.credits >= 25 {
                    wallet.credits -= 25;
                    wallet.upgrades += 1;
                }
            }
        }
        if let Some(wallet) = self.wallets.get_mut(&id) {
            wallet.actions_applied += 1;
        }
        events.push(TickEvent {
            player: Some(id),
            kind: event_kind::ACTION_APPLIED,
            payload: ToyEvent::ActionApplied {
                id: envelope.id,
                action: envelope.action,
            },
        });
    }

    fn step_bombs(&mut self, events: &mut Vec<TickEvent<ToyEvent>>, ops: &mut Vec<ToyTileOp>) {
        let mut exploded = Vec::new();
        for (id, bomb) in &mut self.bombs {
            bomb.fuse = bomb.fuse.saturating_sub(1);
            if bomb.fuse == 0 {
                exploded.push(*id);
            }
        }
        for id in exploded {
            let Some(bomb) = self.bombs.remove(&id) else {
                continue;
            };
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (x, y) = (bomb.x + dx, bomb.y + dy);
                    if self.tile(x, y) == TILE_DIRT {
                        self.set_tile(x, y, TILE_EMPTY);
                        ops.push(ToyTileOp {
                            x,
                            y,
                            tile: TILE_EMPTY,
                        });
                    }
                }
            }
            events.push(TickEvent {
                player: Some(bomb.owner),
                kind: event_kind::EXPLODED,
                payload: ToyEvent::Exploded {
                    x: bomb.x,
                    y: bomb.y,
                },
            });
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn units_to_world(units: i64) -> f32 {
    units as f32 / UNITS_PER_TILE as f32
}

impl NetSimulation for ToySim {
    type Controls = ToyControls;
    type Action = ToyAction;
    type Event = ToyEvent;
    type PlayerState = ToyPlayer;
    type Entities = ToyEntities;
    type Op = ToyTileOp;
    type Chunk = (i32, i32);
    type ChunkSnapshot = Vec<u8>;
    type SectionKey = ToySectionKey;
    type Section = ToySection;
    type Keyframe = Self;

    fn tick(&self) -> Tick {
        self.tick
    }

    fn set_tick(&mut self, tick: Tick) {
        self.tick = tick;
    }

    fn tick_rate_hz(&self) -> Option<u32> {
        Some(self.rules.tick_rate_hz)
    }

    fn player_ids(&self) -> Vec<PlayerId> {
        self.players.keys().copied().collect()
    }

    fn add_player(&mut self, id: PlayerId) {
        if !self.players.contains_key(&id) {
            let spawn = self.spawn(id);
            self.players.insert(id, spawn);
        }
        self.wallets.entry(id).or_default();
    }

    fn remove_player(&mut self, id: PlayerId) {
        self.players.remove(&id);
        self.wallets.remove(&id);
    }

    fn simulate_tick(
        &mut self,
        inputs: &TickInputs<ToyControls, ToyAction>,
    ) -> TickOutput<ToyEvent, ToyTileOp> {
        let mut events = Vec::new();
        let mut ops = Vec::new();
        let neutral = PlayerTickInput {
            controls: ToyControls::default(),
            actions: Vec::new(),
        };
        let ids = self.player_ids();
        for id in ids {
            let input = inputs.players.get(&id).unwrap_or(&neutral);
            for envelope in &input.actions {
                self.apply_action(id, envelope, &mut events);
            }
            self.step_player(id, input.controls, &mut events, &mut ops);
        }
        self.step_bombs(&mut events, &mut ops);
        self.tick = inputs.tick;
        TickOutput { events, ops }
    }

    fn predicts_action(action: &ToyAction) -> bool {
        matches!(action, ToyAction::Bomb)
    }

    fn player_pose(&self, id: PlayerId) -> Option<Pose> {
        self.players.get(&id).map(|p| {
            let mode = p.dig.map_or(0, |(x, y)| {
                u32::try_from(y * self.width + x)
                    .unwrap_or(0)
                    .saturating_add(1)
            });
            Pose::new(units_to_world(p.x), units_to_world(p.y), mode)
        })
    }

    fn player_state(&self, id: PlayerId) -> Option<ToyPlayer> {
        self.players.get(&id).copied()
    }

    fn apply_player_state(&mut self, id: PlayerId, state: ToyPlayer) {
        self.players.insert(id, state);
        self.wallets.entry(id).or_default();
    }

    fn entities(&self, relevance: &Relevance) -> ToyEntities {
        ToyEntities {
            next_id: self.next_entity,
            bombs: self
                .bombs
                .iter()
                .filter(|(_, bomb)| {
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (bomb.x as f32 + 0.5, bomb.y as f32 + 0.5);
                    relevance.contains(x, y)
                })
                .map(|(id, bomb)| (*id, *bomb))
                .collect(),
        }
    }

    fn apply_entities(&mut self, entities: ToyEntities) {
        self.next_entity = entities.next_id;
        self.bombs = entities.bombs.into_iter().collect();
    }

    fn apply_op(&mut self, op: &ToyTileOp) {
        self.set_tile(op.x, op.y, op.tile);
    }

    fn op_chunk(&self, op: &ToyTileOp) -> Option<(i32, i32)> {
        Some((
            op.x.div_euclid(TOY_CHUNK_TILES),
            op.y.div_euclid(TOY_CHUNK_TILES),
        ))
    }

    fn chunk_relevant(&self, chunk: &(i32, i32), relevance: &Relevance) -> bool {
        #[allow(clippy::cast_precision_loss)]
        let (left, top, size) = (
            (chunk.0 * TOY_CHUNK_TILES) as f32,
            (chunk.1 * TOY_CHUNK_TILES) as f32,
            TOY_CHUNK_TILES as f32,
        );
        relevance.touches_box(left, top, left + size, top + size)
    }

    fn chunk_snapshot(&self, chunk: &(i32, i32)) -> Option<Vec<u8>> {
        let (left, top) = (chunk.0 * TOY_CHUNK_TILES, chunk.1 * TOY_CHUNK_TILES);
        Some(
            (top..top + TOY_CHUNK_TILES)
                .flat_map(|y| (left..left + TOY_CHUNK_TILES).map(move |x| (x, y)))
                .map(|(x, y)| self.tile(x, y))
                .collect(),
        )
    }

    fn apply_chunk_snapshot(&mut self, chunk: &(i32, i32), snapshot: Vec<u8>) {
        let (left, top) = (chunk.0 * TOY_CHUNK_TILES, chunk.1 * TOY_CHUNK_TILES);
        let positions = (top..top + TOY_CHUNK_TILES)
            .flat_map(|y| (left..left + TOY_CHUNK_TILES).map(move |x| (x, y)));
        for ((x, y), tile) in positions.zip(snapshot) {
            self.set_tile(x, y, tile);
        }
    }

    fn section_keys(&self) -> Vec<ToySectionKey> {
        std::iter::once(ToySectionKey::Market)
            .chain(self.wallets.keys().map(|id| ToySectionKey::Wallet(*id)))
            .collect()
    }

    fn section_hash(&self, key: &ToySectionKey) -> Option<u64> {
        self.section(key).map(|section| stable_hash(&section))
    }

    fn section(&self, key: &ToySectionKey) -> Option<ToySection> {
        match key {
            ToySectionKey::Market => Some(ToySection::Market(self.market)),
            ToySectionKey::Wallet(id) => self.wallets.get(id).map(|w| ToySection::Wallet(*w)),
        }
    }

    fn apply_section(&mut self, key: &ToySectionKey, section: ToySection) {
        match (key, section) {
            (ToySectionKey::Market, ToySection::Market(market)) => self.market = market,
            (ToySectionKey::Wallet(id), ToySection::Wallet(wallet)) => {
                self.wallets.insert(*id, wallet);
            }
            _ => {}
        }
    }

    fn remove_section(&mut self, key: &ToySectionKey) {
        if let ToySectionKey::Wallet(id) = key {
            self.wallets.remove(id);
        }
    }

    fn section_visible(&self, key: &ToySectionKey, viewer: Option<PlayerId>) -> bool {
        match key {
            ToySectionKey::Market => true,
            ToySectionKey::Wallet(owner) => Some(*owner) == viewer,
        }
    }

    fn keyframe(&self) -> Self {
        self.clone()
    }

    fn keyframe_for(&self, viewer: Option<PlayerId>) -> Self {
        let mut world = self.clone();
        for (owner, wallet) in &mut world.wallets {
            if Some(*owner) != viewer {
                *wallet = ToyWallet::default();
            }
        }
        world
    }

    fn from_keyframe(keyframe: Self) -> Self {
        keyframe
    }

    fn state_hash(&self) -> u64 {
        stable_hash(self)
    }

    fn visible_state_hash(&self, viewer: Option<PlayerId>) -> u64 {
        stable_hash(&self.keyframe_for(viewer))
    }
}
