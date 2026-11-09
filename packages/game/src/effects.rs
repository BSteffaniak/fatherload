//! Presentation-only effects driven by gameplay events: exactly-once event delivery, sound cues
//! and short-lived particles. Nothing here feeds back into the simulation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use fatherload_simulation::{GameplayEventKind, PlayerId, WorldState};

use crate::{
    audio::cues::{PlacedCue, event_cue},
    runtime::{RuntimeEvent, RuntimeEventKind},
    shell::SoundCue,
};

const PRESENTED_EVENT_HISTORY: usize = 4_096;
const DAMAGE_CUE_COOLDOWN_SECONDS: f32 = 0.5;
const SPARK_LIFE_SECONDS: f32 = 0.45;
/// How long a bomb blast stays on screen.
pub const EXPLOSION_SECONDS: f32 = 0.7;

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
}

/// Transient presentation effects.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    sparks: Vec<SparkParticle>,
    explosions: Vec<Explosion>,
    /// Bomb positions seen last frame, so a detonation can be placed where its bomb was.
    known_bombs: Vec<(f32, f32)>,
    damage_cooldowns: BTreeMap<PlayerId, f32>,
    /// Items in each local player's bay last frame (a pickup is growth).
    cargo_totals: BTreeMap<PlayerId, u32>,
    spawn_counter: u32,
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

    /// Clears everything (new world).
    pub fn clear(&mut self) {
        self.sparks.clear();
        self.explosions.clear();
        self.known_bombs.clear();
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
        for cooldown in self.damage_cooldowns.values_mut() {
            *cooldown -= delta_seconds;
        }
        self.damage_cooldowns.retain(|_, cooldown| *cooldown > 0.0);
    }

    /// Turns freshly presented events into placed sound cues and particles. Events of
    /// `local_players` (or world-wide events) make sound; see `audio::cues::event_cue` for the
    /// event -> cue table.
    pub fn apply(
        &mut self,
        events: &[RuntimeEvent],
        world: &WorldState,
        local_players: &[PlayerId],
    ) -> Vec<PlacedCue> {
        let mut cues = Vec::new();
        // Several detonations in one frame share one pass over the vanished bombs.
        let mut blasted = false;
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
                GameplayEventKind::BombDetonated => {
                    let blasts = self.spawn_explosions(world);
                    if blasts.is_empty() && !blasted {
                        // A bomb placed and detonated between two frames was never seen: still
                        // heard, just not placed.
                        cues.push(PlacedCue::interface(SoundCue::Explosion));
                    }
                    blasted = true;
                    for (x, y) in blasts {
                        cues.push(PlacedCue::world(SoundCue::Explosion, x, y));
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
        self.known_bombs = world.bombs().iter().map(|bomb| (bomb.x, bomb.y)).collect();
        cues
    }

    /// Blasts where bombs seen last frame have disappeared; returns where they went off.
    fn spawn_explosions(&mut self, world: &WorldState) -> Vec<(f32, f32)> {
        let remaining = world.bombs();
        let gone: Vec<(f32, f32)> = self
            .known_bombs
            .iter()
            .copied()
            .filter(|(x, y)| {
                !remaining
                    .iter()
                    .any(|bomb| (bomb.x - x).abs() < 1.0 && (bomb.y - y).abs() < 1.0)
            })
            .collect();
        let mut blasts = Vec::new();
        for (x, y) in gone {
            if self
                .explosions
                .iter()
                .any(|explosion| explosion.age < 0.05 && (explosion.x - x).abs() < 1.0)
            {
                continue;
            }
            self.explosions.push(Explosion { x, y, age: 0.0 });
            self.spawn_sparks(x, y);
            self.spawn_sparks(x, y - 8.0);
            blasts.push((x, y));
        }
        blasts
    }

    fn spawn_sparks(&mut self, x: f32, y: f32) {
        for index in 0..6_u8 {
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
        | GameplayEventKind::BlueprintCompleted { player_id, .. } => Some(player_id),
        GameplayEventKind::EscapeTunnelChanged
        | GameplayEventKind::HazardChanged
        | GameplayEventKind::BouldersChanged
        | GameplayEventKind::BombDetonated
        | GameplayEventKind::VaultOpened { .. }
        | GameplayEventKind::VaultLockReset { .. } => None,
    }
}

/// Everything in a rig's bay: ore, artifacts and materials.
fn cargo_total(player: &fatherload_simulation::Player) -> u32 {
    player
        .cargo
        .values()
        .chain(player.artifacts.values())
        .chain(player.materials.values())
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
    fn unseen_detonations_still_sound_once_and_pickups_need_cargo_growth() {
        let local = PlayerId::new(1);
        let mut world = WorldState::new_game(local);
        let mut effects = Effects::default();
        let world_event = |tick, kind| RuntimeEvent {
            player: None,
            ..event(tick, local, kind)
        };
        let blasts = [
            world_event(1, GameplayEventKind::BombDetonated),
            world_event(1, GameplayEventKind::BombDetonated),
        ];
        assert_eq!(
            effects.apply(&blasts, &world, &[local]),
            vec![PlacedCue::interface(SoundCue::Explosion)]
        );
        // Selling (the bay shrinks or stays) is not a pickup; mining (it grows) is.
        let changed = [event(
            2,
            local,
            GameplayEventKind::CargoChanged { player_id: local },
        )];
        assert!(effects.apply(&changed, &world, &[local]).is_empty());
        if let Some(player) = world.player_mut(local) {
            *player
                .cargo
                .entry(fatherload_simulation::terrain::MineralKind::Copper)
                .or_default() += 1;
        }
        assert_eq!(
            effects.apply(&changed, &world, &[local]),
            vec![PlacedCue::player(SoundCue::Pickup, local)]
        );
    }
}
