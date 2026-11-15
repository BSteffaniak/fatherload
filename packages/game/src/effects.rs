//! Presentation-only effects driven by gameplay events: exactly-once event delivery, sound cues
//! and short-lived particles. Nothing here feeds back into the simulation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use fatherload_simulation::{
    BlastDirection, ExplosiveKind, GameplayEventKind, PlayerId, WeaponKind, WorldState,
    event::ImpactSurface, item::ItemKind,
};

use crate::{
    audio::cues::{PlacedCue, detonation_cue, event_cue, impact_cue, weapon_cue},
    runtime::{RuntimeEvent, RuntimeEventKind},
    shell::SoundCue,
};

const PRESENTED_EVENT_HISTORY: usize = 4_096;
const DAMAGE_CUE_COOLDOWN_SECONDS: f32 = 0.5;
const SPARK_LIFE_SECONDS: f32 = 0.45;
/// How long a bomb blast stays on screen.
pub const EXPLOSION_SECONDS: f32 = 0.7;
/// How long a picked-up drop takes to fly into its rig (WP-DROPS).
pub const PICKUP_FLIGHT_SECONDS: f32 = 0.22;
/// Most pickup flights alive at once (a hoovered blast spray).
const MAX_PICKUP_FLIGHTS: usize = 96;
/// Most pickup sounds one frame plays per player (a spray hoovered in one tick is a flurry,
/// not a wall of identical chimes).
const PICKUP_CUES_PER_FRAME: usize = 3;
/// How long a muzzle flash (and a railgun's tracer) stays on screen.
pub const SHOT_SECONDS: f32 = 0.16;
/// How long a railgun tracer lingers.
pub const RAIL_SECONDS: f32 = 0.4;

/// A weapon shot's flash in world space, `age` seconds old: the muzzle flash at `(x, y)`
/// pointing toward `(end_x, end_y)` (a hitscan shot's full tracer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotFlash {
    pub weapon: WeaponKind,
    pub x: f32,
    pub y: f32,
    pub end_x: f32,
    pub end_y: f32,
    pub age: f32,
}

impl ShotFlash {
    /// How long this flash stays on screen.
    #[must_use]
    pub fn lifetime(&self) -> f32 {
        if self.weapon == WeaponKind::Railgun {
            RAIL_SECONDS
        } else {
            SHOT_SECONDS
        }
    }
}

/// Delivers each runtime event to presentation at most once, keyed by its stable identity
/// `(epoch, tick, player, kind, ordinal)`. Host events are already unique; a predicting client
/// will hand the same event over twice (predicted, then authoritative).
#[derive(Clone, Debug, Default)]
pub struct EventFeed {
    seen: BTreeSet<(u64, u64, Option<u64>, u32, u32)>,
    order: VecDeque<(u64, u64, Option<u64>, u32, u32)>,
}

impl EventFeed {
    /// Returns the events not presented before, in order.
    pub fn present(&mut self, events: Vec<RuntimeEvent>) -> Vec<RuntimeEvent> {
        let mut fresh = Vec::with_capacity(events.len());
        for event in events {
            let key = event.key();
            if !self.seen.insert(key) {
                continue;
            }
            self.order.push_back(key);
            while self.order.len() > PRESENTED_EVENT_HISTORY {
                if let Some(expired) = self.order.pop_front() {
                    self.seen.remove(&expired);
                }
            }
            fresh.push(event);
        }
        fresh
    }
}

/// A short-lived spark particle in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparkParticle {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub life: f32,
}

/// A bomb blast in world space, `age` seconds old.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Explosion {
    pub x: f32,
    pub y: f32,
    pub age: f32,
    /// What went off (the plain bomb for [`GameplayEventKind::BombDetonated`]); decides the
    /// look: a shaped charge's jet, an EMP's ring, a mining charge's wide dust.
    pub kind: ExplosiveKind,
    /// Which way a shaped charge cut.
    pub direction: BlastDirection,
}

impl Explosion {
    /// A plain bomb blast at `(x, y)`.
    #[must_use]
    pub const fn bomb(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            age: 0.0,
            kind: ExplosiveKind::Bomb,
            direction: BlastDirection::Down,
        }
    }
}

/// A collected drop flying from where it was into its rig (presentation of
/// [`GameplayEventKind::ItemsCollected`]; the item is already in the bay).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickupFlight {
    pub item: ItemKind,
    /// How many were picked up (a stack shows its count on the way in).
    pub count: u32,
    /// Where the drop was, in world pixels.
    pub from_x: f32,
    pub from_y: f32,
    /// The rig it flies into (followed every frame).
    pub player: PlayerId,
    /// Seconds since the pickup.
    pub age: f32,
}

impl PickupFlight {
    /// Progress of the flight, 0 at the drop to 1 at the rig (eased in, so it accelerates into
    /// the hatch).
    #[must_use]
    pub fn progress(&self) -> f32 {
        let t = (self.age / PICKUP_FLIGHT_SECONDS).clamp(0.0, 1.0);
        t * t
    }
}

/// Transient presentation effects.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    sparks: Vec<SparkParticle>,
    explosions: Vec<Explosion>,
    pickups: Vec<PickupFlight>,
    shots: Vec<ShotFlash>,
    damage_cooldowns: BTreeMap<PlayerId, f32>,
    /// Items in each local player's bay last frame (a pickup is growth).
    cargo_totals: BTreeMap<PlayerId, u32>,
    spawn_counter: u32,
    presented_blasts: BTreeSet<u64>,
    blast_order: VecDeque<u64>,
}

impl Effects {
    /// Live sparks.
    #[must_use]
    pub fn sparks(&self) -> &[SparkParticle] {
        &self.sparks
    }

    /// Live bomb blasts.
    #[must_use]
    pub fn explosions(&self) -> &[Explosion] {
        &self.explosions
    }

    /// Drops flying into their rigs.
    #[must_use]
    pub fn pickups(&self) -> &[PickupFlight] {
        &self.pickups
    }

    /// Live muzzle flashes and tracers.
    #[must_use]
    pub fn shots(&self) -> &[ShotFlash] {
        &self.shots
    }

    /// Clears everything (new world).
    pub fn clear(&mut self) {
        self.sparks.clear();
        self.explosions.clear();
        self.pickups.clear();
        self.shots.clear();
        self.presented_blasts.clear();
        self.blast_order.clear();
        self.damage_cooldowns.clear();
        self.cargo_totals.clear();
    }

    /// Ages particles and cooldowns.
    pub fn update(&mut self, delta_seconds: f32) {
        for spark in &mut self.sparks {
            spark.x = spark.velocity_x.mul_add(delta_seconds, spark.x);
            spark.y = spark.velocity_y.mul_add(delta_seconds, spark.y);
            spark.velocity_y = 420.0f32.mul_add(delta_seconds, spark.velocity_y);
            spark.life -= delta_seconds;
        }
        self.sparks.retain(|spark| spark.life > 0.0);
        for explosion in &mut self.explosions {
            explosion.age += delta_seconds;
        }
        self.explosions
            .retain(|explosion| explosion.age < EXPLOSION_SECONDS);
        for pickup in &mut self.pickups {
            pickup.age += delta_seconds;
        }
        self.pickups
            .retain(|pickup| pickup.age < PICKUP_FLIGHT_SECONDS);
        for shot in &mut self.shots {
            shot.age += delta_seconds;
        }
        self.shots.retain(|shot| shot.age < shot.lifetime());
        for cooldown in self.damage_cooldowns.values_mut() {
            *cooldown -= delta_seconds;
        }
        self.damage_cooldowns.retain(|_, cooldown| *cooldown > 0.0);
    }

    /// Turns freshly presented events into placed sound cues and particles. Events of
    /// `local_players` (or world-wide events) make sound; see `audio::cues::event_cue` for the
    /// event -> cue table.
    #[allow(
        clippy::too_many_lines,
        reason = "one dispatch over every presented gameplay event"
    )]
    pub fn apply(
        &mut self,
        events: &[RuntimeEvent],
        world: &WorldState,
        local_players: &[PlayerId],
    ) -> Vec<PlacedCue> {
        let mut cues = Vec::new();
        let mut blast_cues = 0;
        let mut impact_cues = 0;
        let mut pickup_cues: BTreeMap<PlayerId, usize> = BTreeMap::new();
        let mut enemy_cues = 0;
        for event in events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            let local = event
                .player
                .is_none_or(|player| local_players.contains(&player));
            match gameplay.kind {
                GameplayEventKind::CargoChanged { player_id }
                | GameplayEventKind::PoiOpened { player_id, .. } => {
                    if let Some(player) = world.player(player_id) {
                        self.spawn_sparks(player.x, player.y + 18.0);
                    }
                }
                GameplayEventKind::BombDetonated { bomb_id, x, y } => {
                    if !self.presented_blasts.insert(bomb_id) {
                        continue;
                    }
                    self.blast_order.push_back(bomb_id);
                    if self.blast_order.len() > PRESENTED_EVENT_HISTORY
                        && let Some(expired) = self.blast_order.pop_front()
                    {
                        self.presented_blasts.remove(&expired);
                    }
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    if self.explosions.len() < 64 {
                        self.explosions.push(Explosion::bomb(x, y));
                        self.spawn_blast_debris(x, y);
                    }
                    if blast_cues < 8 {
                        cues.push(PlacedCue::world(SoundCue::Explosion, x, y));
                        blast_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::ExplosiveDetonated {
                    bomb_id,
                    kind,
                    x,
                    y,
                    direction,
                } => {
                    if !self.remember_blast(bomb_id) {
                        continue;
                    }
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    if self.explosions.len() < 64 {
                        self.explosions.push(Explosion {
                            x,
                            y,
                            age: 0.0,
                            kind,
                            direction,
                        });
                        match kind {
                            ExplosiveKind::Emp | ExplosiveKind::Smoke => {}
                            ExplosiveKind::Flare
                            | ExplosiveKind::Incendiary
                            | ExplosiveKind::Scare => {
                                self.spawn_sparks(x, y);
                            }
                            _ => self.spawn_blast_debris(x, y),
                        }
                    }
                    if blast_cues < 8 {
                        cues.push(PlacedCue::world(detonation_cue(kind), x, y));
                        blast_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::ChargeStuck { x, y, .. }
                | GameplayEventKind::ClusterSplit { x, y, .. }
                | GameplayEventKind::FireSpread { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    self.spawn_sparks(x, y);
                    if impact_cues < 4
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::ItemsCollected {
                    player_id,
                    item,
                    count,
                    x,
                    y,
                } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (from_x, from_y) = (x as f32, y as f32);
                    if self.pickups.len() < MAX_PICKUP_FLIGHTS {
                        self.pickups.push(PickupFlight {
                            item,
                            count,
                            from_x,
                            from_y,
                            player: player_id,
                            age: 0.0,
                        });
                    }
                    let heard = pickup_cues.entry(player_id).or_default();
                    if !local || !local_players.contains(&player_id) {
                        continue;
                    }
                    if *heard < PICKUP_CUES_PER_FRAME
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        *heard += 1;
                        cues.push(PlacedCue::player(cue, player_id));
                    }
                    continue;
                }
                GameplayEventKind::StalactiteFell { x, y } => {
                    // Tile coordinates: crack at the tile's centre.
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (x as f32 + 0.5) * fatherload_simulation::terrain::TILE_SIZE,
                        (y as f32 + 0.5) * fatherload_simulation::terrain::TILE_SIZE,
                    );
                    self.spawn_sparks(x, y);
                    if impact_cues < 4 {
                        cues.push(PlacedCue::world(SoundCue::StalactiteCrack, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::WeaponFired {
                    player_id,
                    weapon,
                    x,
                    y,
                    end_x,
                    end_y,
                } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y, end_x, end_y) = (x as f32, y as f32, end_x as f32, end_y as f32);
                    if self.shots.len() < 96 {
                        self.shots.push(ShotFlash {
                            weapon,
                            x,
                            y,
                            end_x,
                            end_y,
                            age: 0.0,
                        });
                    }
                    // Everyone hears every gun: local rigs from their seat, others in the world.
                    let cue = weapon_cue(weapon);
                    cues.push(if local_players.contains(&player_id) {
                        PlacedCue::player(cue, player_id)
                    } else {
                        PlacedCue::world(cue, x, y)
                    });
                    continue;
                }
                GameplayEventKind::ProjectileImpact { x, y, surface, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    if surface == ImpactSurface::Terrain {
                        self.spawn_dust(x, y);
                    } else {
                        self.spawn_sparks(x, y);
                    }
                    if impact_cues < 6 {
                        cues.push(PlacedCue::world(impact_cue(surface), x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::EnemyAlerted { x, y, .. }
                | GameplayEventKind::EnemyWindup { x, y, .. }
                | GameplayEventKind::EnemyFired { x, y, .. }
                | GameplayEventKind::EnemyHurt { x, y, .. }
                | GameplayEventKind::EnemyKilled { x, y, .. }
                | GameplayEventKind::EnemyStunned { x, y, .. } => {
                    // Enemy sounds come from the enemy (heard by every nearby seat); hits and
                    // deaths throw sparks or debris.
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    match gameplay.kind {
                        GameplayEventKind::EnemyKilled { .. } => self.spawn_blast_debris(x, y),
                        GameplayEventKind::EnemyHurt { .. }
                        | GameplayEventKind::EnemyStunned { .. } => self.spawn_sparks(x, y),
                        _ => {}
                    }
                    if enemy_cues < 6
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        enemy_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::EntityDestroyed { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    self.spawn_blast_debris(x, y);
                    cues.push(PlacedCue::world(SoundCue::TargetDestroyed, x, y));
                    continue;
                }
                GameplayEventKind::EntityDamaged { .. } => continue,
                // Base building (WP-BASE): heard where the piece went, by the whole crew.
                GameplayEventKind::BasePlaced { x, y, .. }
                | GameplayEventKind::BaseRemoved { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (x as f32 + 0.5) * fatherload_simulation::TILE_SIZE,
                        (y as f32 + 0.5) * fatherload_simulation::TILE_SIZE,
                    );
                    self.spawn_sparks(x, y + 8.0);
                    if impact_cues < 4
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::BombBounced { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    self.spawn_sparks(x, y + 6.0);
                    if impact_cues < 4 {
                        cues.push(PlacedCue::world(SoundCue::BombBounce, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                _ => {}
            }
            let Some(cue) = event_cue(&gameplay.kind) else {
                continue;
            };
            let player = gameplay_player(&gameplay.kind);
            if !local || player.is_some_and(|player| !local_players.contains(&player)) {
                continue;
            }
            match gameplay.kind {
                GameplayEventKind::PlayerDamaged { player_id } => {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        self.damage_cooldowns.entry(player_id)
                    {
                        entry.insert(DAMAGE_CUE_COOLDOWN_SECONDS);
                    } else {
                        continue;
                    }
                }
                GameplayEventKind::CargoChanged { player_id } => {
                    // Selling empties the bay (coins play for that); only growth is a pickup.
                    let total = world.player(player_id).map_or(0, cargo_total);
                    let previous = self.cargo_totals.insert(player_id, total);
                    if previous.is_none_or(|previous| total <= previous) {
                        continue;
                    }
                }
                _ => {}
            }
            cues.push(player.map_or_else(
                || PlacedCue::interface(cue),
                |player| PlacedCue::player(cue, player),
            ));
        }
        for player_id in local_players {
            if let Some(player) = world.player(*player_id) {
                self.cargo_totals.insert(*player_id, cargo_total(player));
            }
        }
        cues
    }

    /// Records a detonation as presented; `false` if it was already (a predicting client sees
    /// it twice).
    fn remember_blast(&mut self, bomb_id: u64) -> bool {
        if !self.presented_blasts.insert(bomb_id) {
            return false;
        }
        self.blast_order.push_back(bomb_id);
        if self.blast_order.len() > PRESENTED_EVENT_HISTORY
            && let Some(expired) = self.blast_order.pop_front()
        {
            self.presented_blasts.remove(&expired);
        }
        true
    }

    /// A puff of rock chips where a shot struck terrain.
    fn spawn_dust(&mut self, x: f32, y: f32) {
        for grain in 0..5_u8 {
            if self.sparks.len() >= 768 {
                break;
            }
            let angle = f32::from(grain).mul_add(1.25, 3.6);
            let speed = f32::from(grain).mul_add(18.0, 60.0);
            self.sparks.push(SparkParticle {
                x,
                y,
                velocity_x: angle.cos() * speed,
                velocity_y: angle.sin().mul_add(speed, -70.0),
                life: SPARK_LIFE_SECONDS * 0.6,
            });
        }
    }

    fn spawn_blast_debris(&mut self, x: f32, y: f32) {
        for grain in 0..16_u8 {
            if self.sparks.len() >= 768 {
                break;
            }
            let angle = f32::from(grain) / 16.0 * std::f32::consts::TAU;
            let speed = 90.0 + f32::from(grain % 4) * 28.0;
            self.sparks.push(SparkParticle {
                x,
                y,
                velocity_x: angle.cos() * speed,
                velocity_y: angle.sin().mul_add(speed, -80.0),
                life: SPARK_LIFE_SECONDS,
            });
        }
    }

    fn spawn_sparks(&mut self, x: f32, y: f32) {
        for index in 0..6_u8 {
            if self.sparks.len() >= 768 {
                break;
            }
            self.spawn_counter = self.spawn_counter.wrapping_add(1);
            let spread = f32::from(index) - 2.5;
            #[allow(
                clippy::cast_precision_loss,
                reason = "small cycling counter only varies particle speed"
            )]
            let jitter = (self.spawn_counter % 7) as f32;
            self.sparks.push(SparkParticle {
                x,
                y,
                velocity_x: spread * 38.0,
                velocity_y: -150.0 - jitter * 12.0,
                life: SPARK_LIFE_SECONDS,
            });
        }
    }
}

/// The player an event is about, if any.
const fn gameplay_player(kind: &GameplayEventKind) -> Option<PlayerId> {
    match *kind {
        GameplayEventKind::DrillStarted { player_id }
        | GameplayEventKind::PurchaseCompleted { player_id }
        | GameplayEventKind::CargoChanged { player_id }
        | GameplayEventKind::BombPlaced { player_id }
        | GameplayEventKind::InfrastructurePlaced { player_id }
        | GameplayEventKind::ScannerUsed { player_id }
        | GameplayEventKind::EscapeStarted { player_id }
        | GameplayEventKind::DeepRewardGranted { player_id }
        | GameplayEventKind::RescueTriggered { player_id }
        | GameplayEventKind::PlayerDamaged { player_id }
        | GameplayEventKind::PoiOpened { player_id, .. }
        | GameplayEventKind::PoiDiscovered { player_id, .. }
        | GameplayEventKind::TestModeChanged { player_id }
        | GameplayEventKind::QuestAccepted { player_id, .. }
        | GameplayEventKind::QuestStageDone { player_id, .. }
        | GameplayEventKind::QuestReady { player_id, .. }
        | GameplayEventKind::QuestCompleted { player_id, .. }
        | GameplayEventKind::QuestFailed { player_id, .. }
        | GameplayEventKind::BlueprintCompleted { player_id, .. }
        | GameplayEventKind::RigCustomized { player_id }
        | GameplayEventKind::VillageDiscovered { player_id, .. }
        | GameplayEventKind::VillageServiceUsed { player_id, .. }
        | GameplayEventKind::ExplosiveDeployed { player_id, .. }
        | GameplayEventKind::RemoteDetonation { player_id }
        | GameplayEventKind::ItemsCollected { player_id, .. }
        | GameplayEventKind::PickupBlocked { player_id, .. }
        | GameplayEventKind::BlocksDumped { player_id }
        | GameplayEventKind::CollectBlocksToggled { player_id, .. }
        | GameplayEventKind::CaveDiscovered { player_id, .. }
        | GameplayEventKind::CaveCacheOpened { player_id, .. }
        | GameplayEventKind::RigStruck { player_id, .. }
        | GameplayEventKind::EnemyDifficultyChanged { player_id, .. }
        | GameplayEventKind::WeaponFired { player_id, .. }
        | GameplayEventKind::WeaponDry { player_id, .. }
        | GameplayEventKind::WeaponMounted { player_id, .. }
        | GameplayEventKind::BasePlaced { player_id, .. }
        | GameplayEventKind::BaseRemoved { player_id, .. }
        | GameplayEventKind::BaseServiceUsed { player_id, .. } => Some(player_id),
        GameplayEventKind::EscapeTunnelChanged
        | GameplayEventKind::HazardChanged
        | GameplayEventKind::BouldersChanged
        | GameplayEventKind::BombDetonated { .. }
        | GameplayEventKind::BombBounced { .. }
        | GameplayEventKind::ChargeStuck { .. }
        | GameplayEventKind::ExplosiveDetonated { .. }
        | GameplayEventKind::ClusterSplit { .. }
        | GameplayEventKind::FireSpread { .. }
        | GameplayEventKind::VaultOpened { .. }
        | GameplayEventKind::VaultLockReset { .. }
        | GameplayEventKind::StalactiteFell { .. }
        | GameplayEventKind::EnemyAlerted { .. }
        | GameplayEventKind::EnemyWindup { .. }
        | GameplayEventKind::EnemyFired { .. }
        | GameplayEventKind::EnemyHurt { .. }
        | GameplayEventKind::EnemyKilled { .. }
        | GameplayEventKind::EnemyStunned { .. }
        | GameplayEventKind::ProjectileImpact { .. }
        | GameplayEventKind::EntityDamaged { .. }
        | GameplayEventKind::EntityDestroyed { .. } => None,
    }
}

/// Everything in a rig's bay that counts as cargo: ore, artifacts and materials (not the
/// explosives, weapons or ammo it also carries).
fn cargo_total(player: &fatherload_simulation::Player) -> u32 {
    use fatherload_simulation::item::ItemKind;
    player
        .cargo_bay
        .iter()
        .filter(|(kind, _)| {
            matches!(
                kind,
                ItemKind::Mineral(_) | ItemKind::Artifact(_) | ItemKind::Material(_)
            )
        })
        .map(|(_, count)| count)
        .sum()
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::{GameplayEvent, GameplayEventId, SimulationTick};

    use super::*;

    fn event(tick: u64, player: PlayerId, kind: GameplayEventKind) -> RuntimeEvent {
        RuntimeEvent {
            epoch: 1,
            tick,
            player: Some(player),
            kind: 10,
            ordinal: 0,
            event: RuntimeEventKind::Gameplay(GameplayEvent {
                id: GameplayEventId {
                    tick: SimulationTick::new(tick),
                    ordinal: 0,
                },
                kind,
            }),
        }
    }

    #[test]
    fn events_are_presented_exactly_once() {
        let player = PlayerId::new(1);
        let damaged = GameplayEventKind::PlayerDamaged { player_id: player };
        let mut feed = EventFeed::default();
        assert_eq!(feed.present(vec![event(5, player, damaged)]).len(), 1);
        assert!(feed.present(vec![event(5, player, damaged)]).is_empty());
        assert_eq!(feed.present(vec![event(6, player, damaged)]).len(), 1);
    }

    #[test]
    fn continuous_damage_plays_a_throttled_cue_for_local_players_only() {
        let local = PlayerId::new(1);
        let remote = PlayerId::new(2);
        let world = WorldState::new_game(local);
        let mut effects = Effects::default();
        let burst: Vec<RuntimeEvent> = (0..10)
            .map(|tick| {
                event(
                    tick,
                    local,
                    GameplayEventKind::PlayerDamaged { player_id: local },
                )
            })
            .collect();
        assert_eq!(
            effects.apply(&burst, &world, &[local]),
            vec![PlacedCue::player(SoundCue::Damage, local)]
        );
        let remote_hit = [event(
            20,
            remote,
            GameplayEventKind::PlayerDamaged { player_id: remote },
        )];
        assert!(effects.apply(&remote_hit, &world, &[local]).is_empty());
        effects.update(1.0);
        assert_eq!(
            effects.apply(&burst, &world, &[local]),
            vec![PlacedCue::player(SoundCue::Damage, local)]
        );
    }

    #[test]
    fn a_large_blast_batch_caps_sound_particles_and_explosions() {
        let local = PlayerId::new(1);
        let world = WorldState::new_game(local);
        let events: Vec<_> = (0..256_u64)
            .map(|id| {
                event(
                    1,
                    local,
                    GameplayEventKind::BombDetonated {
                        bomb_id: id,
                        x: 200,
                        y: 300,
                    },
                )
            })
            .collect();
        let mut effects = Effects::default();
        let cues = effects.apply(&events, &world, &[local]);
        assert_eq!(cues.len(), 8);
        assert_eq!(effects.explosions.len(), 64);
        assert!(effects.sparks.len() <= 768);
        assert!(effects.apply(&events, &world, &[local]).is_empty());
        effects.clear();
        assert_eq!(effects.apply(&events[..1], &world, &[local]).len(), 1);
    }

    #[test]
    fn unseen_detonations_still_sound_once_and_pickups_need_cargo_growth() {
        let local = PlayerId::new(1);
        let mut world = WorldState::new_game(local);
        let mut effects = Effects::default();
        let world_event = |tick, kind| RuntimeEvent {
            player: None,
            ..event(tick, local, kind)
        };
        let blasts = [
            world_event(
                1,
                GameplayEventKind::BombDetonated {
                    bomb_id: 1,
                    x: 120,
                    y: 80,
                },
            ),
            world_event(
                1,
                GameplayEventKind::BombDetonated {
                    bomb_id: 1,
                    x: 120,
                    y: 80,
                },
            ),
        ];
        assert_eq!(
            effects.apply(&blasts, &world, &[local]),
            vec![PlacedCue::world(SoundCue::Explosion, 120.0, 80.0)]
        );
        // Selling (the bay shrinks or stays) is not a pickup; mining (it grows) is.
        let changed = [event(
            2,
            local,
            GameplayEventKind::CargoChanged { player_id: local },
        )];
        assert!(effects.apply(&changed, &world, &[local]).is_empty());
        if let Some(player) = world.player_mut(local) {
            player
                .cargo_bay
                .add(fatherload_simulation::terrain::MineralKind::Copper, 1);
        }
        assert_eq!(
            effects.apply(&changed, &world, &[local]),
            vec![PlacedCue::player(SoundCue::Pickup, local)]
        );
    }
}
