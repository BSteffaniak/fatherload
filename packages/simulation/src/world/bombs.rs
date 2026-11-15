//! Deterministic loose-bomb physics, delayed blast cascades and every explosive kind built on
//! them (see [`crate::explosive`]). No rendering dependencies.

use crate::{
    event::{GameplayEventKind, TickOutput},
    explosive::{
        BlastDirection, BlastShape, CLUSTER_BOMBLETS, Deployment, ExplosiveKind,
        FLARE_PULSE_MILLIS, FLARE_PULSES, FLARE_REVEAL_RADIUS, INCENDIARY_RING_MILLIS,
        INCENDIARY_RINGS, REMOTE_FUSE, SEQUENCE_STEP_MILLIS, ThrowAim, blast_impulse,
    },
    hazard::PlacedBomb,
    ids::{PlayerId, SimulationTick},
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{BlastResult, TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::{WorldState, commands::bomb_tile_position, quests::QuestNotes};

/// Collision half-size, in world pixels (the fuse is cosmetic).
pub const BOMB_RADIUS: f32 = 6.0;
const GRAVITY: f32 = 560.0;
const MAX_SPEED: f32 = 600.0;
const BOUNCE: f32 = 0.28;
const REST_SPEED: f32 = 28.0;
const IMPACT_SOUND_SPEED: f32 = 90.0;
/// Small but visible delay: a kicked bomb moves before its secondary explosion.
const CHAIN_DELAY_MILLIS: u64 = 220;
/// Limit work and network/particle traffic without dropping existing bombs.
pub const MAX_ACTIVE_BOMBS: usize = 256;
/// Rigs are far heavier than a bomb: they take this share of a blast's push.
const RIG_PUSH_SCALE: f32 = 0.55;
/// Largest sideways speed a blast gives a rig.
const RIG_PUSH_MAX_X: f32 = 300.0;
/// Largest upward speed a blast gives a rig (kept under the safe landing speed, so a blast on
/// its own never makes a rig crash-land).
const RIG_PUSH_MAX_LIFT: f32 = 260.0;
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
/// This is the seam for blast impulses on things other than bombs and rigs: the entities track
/// should push its entities from [`WorldState::push_from_blasts`] using
/// [`crate::explosive::blast_impulse`].
#[derive(Clone, Copy, Debug)]
pub struct Blast {
    pub kind: ExplosiveKind,
    pub x: f32,
    pub y: f32,
}

impl WorldState {
    pub(super) fn advance_bombs(
        &mut self,
        next_tick: SimulationTick,
        notes: &mut QuestNotes,
        output: &mut TickOutput,
    ) {
        let mut bombs = std::mem::take(&mut self.bombs);
        bombs.sort_by_key(|bomb| bomb.id);
        for bomb in &mut bombs {
            if bomb.stuck {
                bomb.previous_x = bomb.x;
                bomb.previous_y = bomb.y;
                if supported(&self.terrain, bomb) {
                    continue;
                }
                // The tile holding it is gone: it drops like any loose charge.
                bomb.stuck = false;
            }
            let motion = move_bomb(bomb, &self.terrain, self.timing.delta_seconds, sticks(bomb));
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
            .partition(|bomb| next_tick >= bomb.detonation_tick);
        // Only a charge's first detonation is a blast; a burning fire or flare just ticks on.
        let blasts: Vec<Blast> = expiring
            .iter()
            .filter(|bomb| !bomb.burning())
            .map(|bomb| Blast {
                kind: bomb.kind,
                x: bomb.x,
                y: bomb.y,
            })
            .collect();
        // Every blast in a tick sees the SAME pre-blast terrain for occlusion. Terrain is
        // cleared only after all impulses are resolved, so chain behavior is order-independent.
        let chain_delay = self.timing.rate.ticks_for_millis(CHAIN_DELAY_MILLIS);
        for source in &blasts {
            for bomb in &mut survivors {
                kick_bomb(bomb, source, &self.terrain, next_tick, chain_delay);
            }
        }
        self.push_from_blasts(&blasts);
        let mut spawned = Vec::new();
        for bomb in expiring {
            if let Some(continuing) = self.expire_bomb(bomb, next_tick, notes, output, &mut spawned)
            {
                survivors.push(continuing);
            }
        }
        let room = MAX_ACTIVE_BOMBS.saturating_sub(survivors.len());
        survivors.extend(spawned.into_iter().take(room));
        for bomb in &mut survivors {
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
        self.bombs = survivors;
    }

    /// Pushes rigs away from this tick's blasts (line of sight on the pre-blast terrain).
    ///
    /// Integration seam: generic entities (drops, enemies) get pushed here too once they exist,
    /// with the same [`blast_impulse`] formula.
    pub(super) fn push_from_blasts(&mut self, blasts: &[Blast]) {
        for blast in blasts {
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
                let push_x = (dx * RIG_PUSH_SCALE).clamp(-RIG_PUSH_MAX_X, RIG_PUSH_MAX_X);
                let push_y = (dy * RIG_PUSH_SCALE).max(-RIG_PUSH_MAX_LIFT);
                player.velocity_x = quantize(player.velocity_x + push_x);
                player.velocity_y = quantize(player.velocity_y + push_y);
            }
        }
    }

    /// Runs out one charge's fuse: blasts, bursts, burns or pulses. Returns the charge again if
    /// it keeps going (a fire spreading, a flare burning).
    fn expire_bomb(
        &mut self,
        mut bomb: PlacedBomb,
        next_tick: SimulationTick,
        notes: &mut QuestNotes,
        output: &mut TickOutput,
        spawned: &mut Vec<PlacedBomb>,
    ) -> Option<PlacedBomb> {
        let center = bomb_tile_position(bomb);
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
                    self.burn_ring(center, i32::from(bomb.stage), output);
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
            _ => None,
        }
    }

    /// Removes the terrain a charge's blast shape covers; `None` for charges with no blast.
    fn blast_terrain(&mut self, bomb: &PlacedBomb, center: TilePosition) -> Option<BlastResult> {
        match bomb.kind.blast() {
            BlastShape::None => None,
            BlastShape::Diamond(radius) => Some(self.detonate_bomb_at(center, radius)),
            BlastShape::LootSafe(radius) => {
                let mut result = BlastResult::default();
                for y in center.y - radius..=center.y + radius {
                    for x in center.x - radius..=center.x + radius {
                        if (x - center.x).abs() + (y - center.y).abs() > radius {
                            continue;
                        }
                        let position = TilePosition { x, y };
                        if self.terrain.tile(position).is_some_and(|tile| {
                            matches!(tile.kind, TileKind::Ore(_) | TileKind::Artifact(_))
                        }) {
                            continue;
                        }
                        merge(&mut result, self.terrain.blast_radius(position, 0));
                    }
                }
                Some(result)
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

    /// One ring of an incendiary's fire: dirt, clay and gas at Manhattan distance `radius` burn
    /// away; explosive pockets it reaches go off.
    fn burn_ring(&mut self, center: TilePosition, radius: i32, output: &mut TickOutput) {
        let mut changed = Vec::new();
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
                    TileKind::Dirt | TileKind::Clay | TileKind::Gas => 0,
                    TileKind::ExplosivePocket => 1,
                    _ => continue,
                };
                changed.extend(self.terrain.blast_radius(position, reach).changed_tiles);
            }
        }
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
        let (x, y, velocity_x, velocity_y) = match kind.deployment() {
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
        Some(PlacedBomb {
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
        })
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
        if player.explosive_count(kind) == 0 || self.bombs.len() >= MAX_ACTIVE_BOMBS {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(bomb) = self.launch(player_id, kind, aim) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            let _taken = player.take_explosive(kind);
        }
        self.bombs.push(bomb);
        self.next_bomb_id = self.next_bomb_id.saturating_add(1);
        PlayerScopedCommandOutcome::Applied
    }

    /// Resolves [`crate::PlayerCommand::RemoteDetonate`]: arms every remote charge `player_id`
    /// placed that is still waiting, all on the next tick or [`SEQUENCE_STEP_MILLIS`] apart in
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
        let first = self.simulation_tick.get().saturating_add(1);
        let mut charges: Vec<&mut PlacedBomb> = self
            .bombs
            .iter_mut()
            .filter(|bomb| bomb.owner == Some(player_id) && bomb.awaiting_remote())
            .collect();
        if charges.is_empty() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        charges.sort_by_key(|bomb| bomb.id);
        for (order, bomb) in (0_u64..).zip(charges) {
            bomb.detonation_tick =
                SimulationTick::new(first.saturating_add(order.saturating_mul(step)));
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Remote charges `player_id` placed that are still waiting for the detonator.
    #[must_use]
    pub fn waiting_remote_charges(&self, player_id: PlayerId) -> usize {
        self.bombs
            .iter()
            .filter(|bomb| bomb.owner == Some(player_id) && bomb.awaiting_remote())
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
                &self.terrain,
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

fn merge(into: &mut BlastResult, blast: BlastResult) {
    into.cleared += blast.cleared;
    into.changed_tiles.extend(blast.changed_tiles);
}

/// Whether no blast breaks `kind` (shaped-charge lanes stop at it).
const fn blast_proof(kind: TileKind) -> bool {
    match kind {
        TileKind::Foundation | TileKind::Bedrock => true,
        TileKind::Poi(poi) => poi.after_blast().is_none(),
        _ => false,
    }
}

/// Whether a loose charge sticks on its next contact (stuck once, it stays put).
const fn sticks(bomb: &PlacedBomb) -> bool {
    bomb.kind.sticks() && !bomb.stuck
}

/// Whether the surface a stuck charge clings to is still there.
fn supported(terrain: &Terrain, bomb: &PlacedBomb) -> bool {
    let (dx, dy) = bomb.direction_hint();
    collides(terrain, dx.mul_add(2.0, bomb.x), dy.mul_add(2.0, bomb.y))
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

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world coordinates address terrain"
)]
fn solid(terrain: &Terrain, x: f32, y: f32) -> bool {
    let position = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    // The sky is open; the side edges and the bottom are not.
    if !terrain.contains_column(position.x) || position.y >= terrain.height() {
        return true;
    }
    position.y >= 0 && terrain.is_solid_at(position)
}

fn collides(terrain: &Terrain, x: f32, y: f32) -> bool {
    [-BOMB_RADIUS, 0.0, BOMB_RADIUS].into_iter().any(|dx| {
        [-BOMB_RADIUS, 0.0, BOMB_RADIUS]
            .into_iter()
            .any(|dy| solid(terrain, x + dx, y + dy))
    })
}

/// What one tick of motion did.
#[derive(Clone, Copy, Debug, Default)]
struct Motion {
    /// Hit something hard enough to be heard.
    impact: bool,
    /// Stuck to a surface on this side (sticky charges only).
    stuck: Option<BlastDirection>,
}

/// Axis-separated swept steps never travel more than two pixels, even at the speed cap.
/// Resting bombs are checked each tick: removing their support wakes them automatically.
/// A `sticky` charge stops dead on its first contact instead of bouncing.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "bounded physics substep count, at most 128"
)]
fn move_bomb(bomb: &mut PlacedBomb, terrain: &Terrain, dt: f32, sticky: bool) -> Motion {
    bomb.previous_x = bomb.x;
    bomb.previous_y = bomb.y;
    if collides(terrain, bomb.x, bomb.y) {
        // A drop starts at the rig's feet and can slightly overlap the supporting tile.
        // Only lift out locally; do not teleport bombs through walls or search entire caves.
        let lift =
            (1_u8..=32).find(|offset| !collides(terrain, bomb.x, bomb.y - f32::from(*offset)));
        if let Some(lift) = lift {
            bomb.y -= f32::from(lift);
            bomb.previous_y = bomb.y;
        } else {
            bomb.velocity_x = 0.0;
            bomb.velocity_y = 0.0;
            return Motion::default();
        }
    }
    bomb.velocity_x = bomb.velocity_x.clamp(-MAX_SPEED, MAX_SPEED);
    bomb.velocity_y = GRAVITY
        .mul_add(dt, bomb.velocity_y)
        .clamp(-MAX_SPEED, MAX_SPEED);
    let steps =
        ((bomb.velocity_x.abs().max(bomb.velocity_y.abs()) * dt / 2.0).ceil() as u32).clamp(1, 128);
    let step = dt / steps as f32;
    let mut impact: f32 = 0.0;
    let mut stuck = None;
    for _ in 0..steps {
        let x = bomb.velocity_x.mul_add(step, bomb.x);
        if collides(terrain, x, bomb.y) {
            if sticky {
                stuck = Some(if bomb.velocity_x < 0.0 {
                    BlastDirection::Left
                } else {
                    BlastDirection::Right
                });
                break;
            }
            impact = impact.max(bomb.velocity_x.abs());
            bomb.velocity_x *= -BOUNCE;
        } else {
            bomb.x = x;
        }
        let y = bomb.velocity_y.mul_add(step, bomb.y);
        if collides(terrain, bomb.x, y) {
            if sticky {
                stuck = Some(if bomb.velocity_y < 0.0 {
                    BlastDirection::Up
                } else {
                    BlastDirection::Down
                });
                break;
            }
            impact = impact.max(bomb.velocity_y.abs());
            let hit_floor = bomb.velocity_y > 0.0;
            bomb.velocity_y = if bomb.velocity_y.abs() < REST_SPEED {
                0.0
            } else {
                -bomb.velocity_y * BOUNCE
            };
            // Friction on contact with a floor, not in mid-air.
            if hit_floor {
                bomb.velocity_x *= (12.0f32.mul_add(-step, 1.0)).max(0.0);
                if bomb.velocity_x.abs() < 3.0 {
                    bomb.velocity_x = 0.0;
                }
            }
        } else {
            bomb.y = y;
        }
    }
    if let Some(side) = stuck {
        bomb.velocity_x = 0.0;
        bomb.velocity_y = 0.0;
        bomb.direction = side;
    }
    bomb.x = quantize(bomb.x);
    bomb.y = quantize(bomb.y);
    bomb.velocity_x = quantize(bomb.velocity_x);
    bomb.velocity_y = quantize(bomb.velocity_y);
    bomb.angle = quantize(
        (bomb.angle + (bomb.x - bomb.previous_x) / BOMB_RADIUS).rem_euclid(std::f32::consts::TAU),
    );
    Motion {
        impact: impact >= IMPACT_SOUND_SPEED,
        stuck,
    }
}

fn kick_bomb(
    bomb: &mut PlacedBomb,
    source: &Blast,
    terrain: &Terrain,
    tick: SimulationTick,
    delay: u64,
) {
    let reach = source.kind.push_reach();
    if reach <= 0.0 {
        return;
    }
    let dx = bomb.x - source.x;
    let dy = bomb.y - source.y;
    let distance = dx.hypot(dy);
    if distance >= reach || !line_clear(terrain, source.x, source.y, bomb.x, bomb.y) {
        return;
    }
    if !bomb.stuck {
        // Coincident bombs split reproducibly in opposite directions instead of dividing by
        // zero.
        let (nx, ny) = if distance < 1.0 {
            (if bomb.id.is_multiple_of(2) { 0.7 } else { -0.7 }, -0.7)
        } else {
            (dx / distance, dy / distance)
        };
        let strength = source.kind.push_strength() * (1.0 - distance / reach);
        bomb.velocity_x = nx.mul_add(strength, bomb.velocity_x);
        bomb.velocity_y += strength.mul_add(ny, -strength * 0.3);
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

/// Sample at <= half a collision radius, excluding the source's own tile. A rock wall blocks
/// the kick/chain trigger even if that rock is about to be removed by this blast.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "blast rays are bounded to a few tiles"
)]
fn line_clear(terrain: &Terrain, x: f32, y: f32, end_x: f32, end_y: f32) -> bool {
    let distance = (end_x - x).hypot(end_y - y);
    let steps = (distance / 3.0).ceil().max(1.0) as u32;
    let origin = (
        (x / TILE_SIZE).floor() as i32,
        (y / TILE_SIZE).floor() as i32,
    );
    (1..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        let px = (end_x - x).mul_add(t, x);
        let py = (end_y - y).mul_add(t, y);
        let tile = (
            (px / TILE_SIZE).floor() as i32,
            (py / TILE_SIZE).floor() as i32,
        );
        tile == origin || !solid(terrain, px, py)
    })
}
