//! Presentation-only effects driven by gameplay events: exactly-once event delivery, sound cues
//! and short-lived particles. Nothing here feeds back into the simulation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use fatherload_simulation::{
    BlastDirection, ExplosiveKind, GameplayEventKind, PlayerId, WeaponKind, WorldState,
    event::ImpactSurface, item::ItemKind, milestone::Milestone,
};

use crate::{
    audio::cues::{PlacedCue, detonation_cue, event_cue, impact_cue, weapon_cue},
    runtime::{RuntimeEvent, RuntimeEventKind},
    shell::SoundCue,
};

const PRESENTED_EVENT_HISTORY: usize = 4_096;
const DAMAGE_CUE_COOLDOWN_SECONDS: f32 = 0.5;
/// Least time between two of one rig's sky cues of a kind (band swell, updraft whoosh, icing
/// crackle): a rig hovering on a band's floor or skimming an updraft's edge crosses it over and
/// over, and each crossing is an event.
const SKY_CUE_COOLDOWN_SECONDS: f32 = 4.0;
/// How long a milestone's fanfare stays claimed: a predicting client can be handed the same
/// milestone twice (predicted, then authoritative a few ticks apart), and a milestone is earned
/// only once per rig, so a repeat inside this window is always that duplicate.
pub const MILESTONE_REPEAT_SECONDS: f32 = 20.0;
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
    /// An electric bolt from `(x, y)` to `(end_x, end_y)` rather than a muzzle flash: an arc
    /// rifle's chain link (`weapon` is `ArcRifle`) or, with `lightning`, a storm elemental's arc (T21).
    pub bolt: bool,
    /// A storm elemental's lightning (a fatter, longer-lived bolt).
    pub lightning: bool,
}

impl ShotFlash {
    /// How long this flash stays on screen.
    #[must_use]
    pub fn lifetime(&self) -> f32 {
        if self.lightning {
            LIGHTNING_SECONDS
        } else if self.bolt {
            ARC_SECONDS
        } else if self.weapon == WeaponKind::Railgun {
            RAIL_SECONDS
        } else {
            SHOT_SECONDS
        }
    }
}

/// How long an arc rifle's chain link stays on screen.
pub const ARC_SECONDS: f32 = 0.24;
/// How long a storm elemental's lightning stays on screen.
pub const LIGHTNING_SECONDS: f32 = 0.36;

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

/// Keeps at most one reward fanfare per rig in a frame: buying a tier that earns a milestone
/// fires both events on the same tick, and two brass fanfares on top of each other are noise.
/// The Act II fanfare beats the upgrade fanfare, which beats a milestone's.
fn one_fanfare_per_rig(cues: &mut Vec<PlacedCue>) {
    const fn rank(cue: SoundCue) -> Option<u8> {
        match cue {
            SoundCue::ActTwoFanfare => Some(3),
            SoundCue::UpgradeFanfare => Some(2),
            SoundCue::MilestoneFanfare => Some(1),
            _ => None,
        }
    }
    let mut best: BTreeMap<PlayerId, u8> = BTreeMap::new();
    for cue in cues.iter() {
        if let (Some(rank), crate::audio::cues::SoundSource::Player(player)) =
            (rank(cue.cue), cue.source)
        {
            let entry = best.entry(player).or_insert(rank);
            *entry = (*entry).max(rank);
        }
    }
    let mut kept: BTreeSet<PlayerId> = BTreeSet::new();
    cues.retain(|cue| match (rank(cue.cue), cue.source) {
        (Some(rank), crate::audio::cues::SoundSource::Player(player)) => {
            best.get(&player) == Some(&rank) && kept.insert(player)
        }
        _ => true,
    });
}

/// Transient presentation effects.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    sparks: Vec<SparkParticle>,
    explosions: Vec<Explosion>,
    pickups: Vec<PickupFlight>,
    shots: Vec<ShotFlash>,
    damage_cooldowns: BTreeMap<PlayerId, f32>,
    /// Per rig and sky cue kind (0 band, 1 updraft, 2 icing): seconds until it may play again.
    sky_cue_cooldowns: BTreeMap<(PlayerId, u8), f32>,
    /// Milestones whose fanfare played recently ([`MILESTONE_REPEAT_SECONDS`]).
    milestone_cooldowns: BTreeMap<(PlayerId, Milestone), f32>,
    /// Items in each local player's bay last frame (a pickup is growth).
    cargo_totals: BTreeMap<PlayerId, u32>,
    spawn_counter: u32,
    presented_blasts: BTreeSet<u64>,
    blast_order: VecDeque<u64>,
}

impl Effects {
    /// Moves every effect kept in world pixels with a move of the world's frame (F7), so a
    /// re-centred frame never shows as a jump.
    pub fn reframe(&mut self, shift: fatherload_simulation::position::FrameShift) {
        for spark in &mut self.sparks {
            shift.apply(&mut spark.x, &mut spark.y);
        }
        for blast in &mut self.explosions {
            shift.apply(&mut blast.x, &mut blast.y);
        }
        for flight in &mut self.pickups {
            shift.apply(&mut flight.from_x, &mut flight.from_y);
        }
        for shot in &mut self.shots {
            shift.apply(&mut shot.x, &mut shot.y);
            shift.apply(&mut shot.end_x, &mut shot.end_y);
        }
    }

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
        self.sky_cue_cooldowns.clear();
        self.milestone_cooldowns.clear();
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
        for cooldown in self.sky_cue_cooldowns.values_mut() {
            *cooldown -= delta_seconds;
        }
        self.sky_cue_cooldowns.retain(|_, cooldown| *cooldown > 0.0);
        for cooldown in self.milestone_cooldowns.values_mut() {
            *cooldown -= delta_seconds;
        }
        self.milestone_cooldowns
            .retain(|_, cooldown| *cooldown > 0.0);
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
        // A skipped night already plays its lullaby: no midnight bell on top of it.
        let night_skipped = events.iter().any(|event| {
            matches!(
                &event.event,
                RuntimeEventKind::Gameplay(gameplay)
                    if matches!(gameplay.kind, GameplayEventKind::NightSkipped { .. })
            )
        });
        // A world sound is heard only by seats in the store it happened in (Wave 13 QA): the
        // cues each event adds are tagged with its store (at the next event, as the arms below
        // `continue`, and after the last).
        let mut stamped = 0;
        let mut previous_store = fatherload_simulation::place::StoreKey::Home;
        for event in events {
            stamp_cue_stores(&mut cues, &mut stamped, previous_store);
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            previous_store = gameplay.store;
            // Routed by place (Wave 13 QA): a player-less event of a body or of free flight
            // is heard only when a local seat is there.
            let local = event.player.map_or_else(
                || {
                    gameplay.store.is_home()
                        || local_players
                            .iter()
                            .any(|player| world.store_of(*player) == gameplay.store)
                },
                |player| local_players.contains(&player),
            );
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
                            ExplosiveKind::Emp
                            | ExplosiveKind::Smoke
                            | ExplosiveKind::TimeBubble => {}
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
                GameplayEventKind::IsleCollapsing { x, y } => {
                    // A keystone breaking: sparks of aether and the groan, from the island.
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                    );
                    self.spawn_sparks(x, y);
                    if impact_cues < 4 {
                        cues.push(PlacedCue::world(SoundCue::IsleCollapse, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::StalactiteFell { x, y } => {
                    // Tile coordinates: crack at the tile's centre.
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
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
                            bolt: false,
                            lightning: false,
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
                GameplayEventKind::PilotFired {
                    player_id,
                    weapon,
                    x,
                    y,
                    end_x,
                    end_y,
                } => {
                    // A pilot's shot (WP-PILOT): the same muzzle flash as a rig gun, smaller;
                    // melee swings throw a few sparks where they land.
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y, end_x, end_y) = (x as f32, y as f32, end_x as f32, end_y as f32);
                    if fatherload_simulation::pilot::arsenal::is_melee(weapon) {
                        self.spawn_dust(end_x, end_y);
                    } else if self.shots.len() < 96 {
                        self.shots.push(ShotFlash {
                            weapon,
                            x,
                            y,
                            end_x,
                            end_y,
                            age: 0.0,
                            bolt: false,
                            lightning: false,
                        });
                    }
                    let cue = weapon_cue(weapon);
                    cues.push(if local_players.contains(&player_id) {
                        PlacedCue::player(cue, player_id)
                    } else {
                        PlacedCue::world(cue, x, y)
                    });
                    continue;
                }
                GameplayEventKind::ChestOpened { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                    );
                    self.spawn_sparks(x, y - 8.0);
                    self.spawn_sparks(x, y - 8.0);
                }
                GameplayEventKind::MechanismUsed {
                    x,
                    y,
                    mechanism:
                        fatherload_simulation::mechanism::MechanismEvent::GateOpened
                        | fatherload_simulation::mechanism::MechanismEvent::GateClosed,
                    ..
                } => {
                    // Grit shaken off the lintel as the gate moves.
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::terrain::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 1.0)
                            * fatherload_simulation::terrain::TILE_SIZE,
                    );
                    self.spawn_dust(x, y);
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
                // A night monster burrowing away at dawn (WP-NIGHT): a puff of dust where it
                // went, heard from there.
                GameplayEventKind::NightMonsterBurrowed { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    self.spawn_sparks(x, y + 6.0);
                    if enemy_cues < 6 {
                        cues.push(PlacedCue::world(SoundCue::Burrow, x, y));
                        enemy_cues += 1;
                    }
                    continue;
                }
                // Top-down crypts (WP-TOMBS T29): traps by tile, the rest by world pixel; heard
                // from where they happen.
                GameplayEventKind::CryptTrap { x, y, .. }
                | GameplayEventKind::CryptPitFall { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
                    );
                    if matches!(gameplay.kind, GameplayEventKind::CryptPitFall { .. }) {
                        self.spawn_dust(x, y);
                    }
                    if impact_cues < 4
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::CryptRoll { x, y, .. }
                | GameplayEventKind::CryptLordPhase { x, y, .. }
                | GameplayEventKind::CryptTell { x, y, .. }
                | GameplayEventKind::CryptSlam { x, y, .. }
                | GameplayEventKind::CryptSummon { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    match gameplay.kind {
                        GameplayEventKind::CryptSlam { .. } => self.spawn_blast_debris(x, y),
                        GameplayEventKind::CryptSummon { .. } => self.spawn_dust(x, y),
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
                // Depth logistics (Part III F10): the dive charge's blast under the rig, and a
                // shower of sparks at both ends of a beacon jump. Their cues are the rig's own
                // (below).
                GameplayEventKind::DiveChargeFired { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32 + 24.0);
                    self.spawn_blast_debris(x, y);
                    self.spawn_dust(x, y);
                }
                GameplayEventKind::BeaconTravelled {
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                    ..
                } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (from_x, from_y, to_x, to_y) =
                        (from_x as f32, from_y as f32, to_x as f32, to_y as f32);
                    self.spawn_sparks(from_x, from_y);
                    self.spawn_sparks(to_x, to_y);
                    self.spawn_sparks(to_x, to_y - 8.0);
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
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
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
                // The world clock: the dawn chorus, dusk crickets and the midnight bell are
                // heard only by seats whose rig is out under the sky (not deep underground).
                GameplayEventKind::Dawn { .. }
                | GameplayEventKind::Dusk { .. }
                | GameplayEventKind::NewDay { .. } => {
                    let under_sky = local_players
                        .iter()
                        .filter_map(|player| world.player(*player))
                        .any(|player| {
                            let tile = player.tile_in(crate::view_frame::frame());
                            tile.y < world.terrain().surface_row(tile.x) + CLOCK_CUE_DEPTH_ROWS
                        });
                    let bell_during_skip =
                        night_skipped && matches!(gameplay.kind, GameplayEventKind::NewDay { .. });
                    if under_sky
                        && !bell_during_skip
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::interface(cue));
                    }
                    continue;
                }
                // Sky continents and Act II weapons (T21).
                GameplayEventKind::LightningArc {
                    x, y, end_x, end_y, ..
                }
                | GameplayEventKind::ArcChained {
                    x, y, end_x, end_y, ..
                } => {
                    let lightning = matches!(gameplay.kind, GameplayEventKind::LightningArc { .. });
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y, end_x, end_y) = (x as f32, y as f32, end_x as f32, end_y as f32);
                    if self.shots.len() < 96 {
                        self.shots.push(ShotFlash {
                            weapon: WeaponKind::ArcRifle,
                            x,
                            y,
                            end_x,
                            end_y,
                            age: 0.0,
                            bolt: true,
                            lightning,
                        });
                    }
                    self.spawn_sparks(end_x, end_y);
                    if impact_cues < 6
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, end_x, end_y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::FlakBurst { x, y, .. }
                | GameplayEventKind::GravityThrow { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    if matches!(gameplay.kind, GameplayEventKind::FlakBurst { .. }) {
                        if self.explosions.len() < 64 {
                            self.explosions.push(Explosion {
                                x,
                                y,
                                age: 0.0,
                                kind: ExplosiveKind::Bomblet,
                                direction: BlastDirection::Down,
                            });
                        }
                        self.spawn_blast_debris(x, y);
                    } else {
                        self.spawn_sparks(x, y);
                    }
                    if blast_cues < 8
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        blast_cues += 1;
                    }
                    continue;
                }
                // Fluids (Part III F9): a burst of spray where a pocket opens, a puff where
                // fluids react (steam, fizz); heard where it happens.
                GameplayEventKind::FluidBreached { x, y, .. }
                | GameplayEventKind::FluidReacted { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (x as f32 + 0.5) * fatherload_simulation::TILE_SIZE,
                        (y as f32 + 0.5) * fatherload_simulation::TILE_SIZE,
                    );
                    self.spawn_dust(x, y);
                    if impact_cues < 4
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        impact_cues += 1;
                    }
                    continue;
                }
                GameplayEventKind::TempleLeverPulled { x, y, .. }
                | GameplayEventKind::TempleGateOpened { x, y, .. }
                | GameplayEventKind::RelicChestOpened { x, y, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded tile coordinates")]
                    let (x, y) = (
                        (crate::view_frame::frame().rel_col(x) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
                        (crate::view_frame::frame().rel_row(y) + 0.5)
                            * fatherload_simulation::TILE_SIZE,
                    );
                    self.spawn_sparks(x, y);
                    if let Some(cue) = event_cue(&gameplay.kind) {
                        cues.push(PlacedCue::world(cue, x, y));
                    }
                    continue;
                }
                GameplayEventKind::SkyEnemyAction { x, y, action, .. } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded event world pixels")]
                    let (x, y) = (x as f32, y as f32);
                    if action == fatherload_simulation::enemy::SkyEnemyAction::AirshipDowned
                        && self.explosions.len() < 64
                    {
                        self.explosions.push(Explosion::bomb(x, y));
                        self.spawn_blast_debris(x, y);
                    }
                    if enemy_cues < 6
                        && let Some(cue) = event_cue(&gameplay.kind)
                    {
                        cues.push(PlacedCue::world(cue, x, y));
                        enemy_cues += 1;
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
                GameplayEventKind::SkyBandEntered { player_id, .. }
                | GameplayEventKind::UpdraftCaught { player_id }
                | GameplayEventKind::JetStreamCaught { player_id }
                | GameplayEventKind::RigIcing { player_id } => {
                    let kind = match gameplay.kind {
                        GameplayEventKind::SkyBandEntered { .. } => 0,
                        GameplayEventKind::UpdraftCaught { .. } => 1,
                        GameplayEventKind::JetStreamCaught { .. } => 3,
                        _ => 2,
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        self.sky_cue_cooldowns.entry((player_id, kind))
                    {
                        entry.insert(SKY_CUE_COOLDOWN_SECONDS);
                    } else {
                        continue;
                    }
                }
                GameplayEventKind::MilestoneEarned {
                    player_id,
                    milestone,
                } => {
                    if self
                        .milestone_cooldowns
                        .insert((player_id, milestone), MILESTONE_REPEAT_SECONDS)
                        .is_some()
                    {
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
        stamp_cue_stores(&mut cues, &mut stamped, previous_store);
        for player_id in local_players {
            if let Some(player) = world.player(*player_id) {
                self.cargo_totals.insert(*player_id, cargo_total(player));
            }
        }
        one_fanfare_per_rig(&mut cues);
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
#[allow(clippy::too_many_lines, reason = "one arm per gameplay event kind")]
const fn gameplay_player(kind: &GameplayEventKind) -> Option<PlayerId> {
    match *kind {
        GameplayEventKind::DrillStarted { player_id }
        | GameplayEventKind::LiftRideStarted { player_id, .. }
        | GameplayEventKind::LiftArrived { player_id, .. }
        | GameplayEventKind::LiftRestored { player_id, .. }
        | GameplayEventKind::LogisticsPlaced { player_id, .. }
        | GameplayEventKind::OutpostReached { player_id, .. }
        | GameplayEventKind::OutpostClaimed { player_id, .. }
        | GameplayEventKind::OutpostServiced { player_id, .. }
        | GameplayEventKind::BeaconTravelled { player_id, .. }
        | GameplayEventKind::DiveChargeFired { player_id, .. }
        | GameplayEventKind::DescentGateBlocked { player_id, .. }
        | GameplayEventKind::CartBoarded { player_id }
        | GameplayEventKind::CartSwitched { player_id }
        | GameplayEventKind::CartLeft { player_id, .. }
        | GameplayEventKind::StrongboxOpened { player_id, .. }
        | GameplayEventKind::SinkholeRumble { player_id }
        | GameplayEventKind::SinkholeEscaped { player_id }
        | GameplayEventKind::SinkholeCaught { player_id }
        | GameplayEventKind::SinkholeReset { player_id }
        | GameplayEventKind::DeepClaimGranted { player_id }
        | GameplayEventKind::DeepClaimMissing { player_id }
        | GameplayEventKind::LavaTubeEntered { player_id }
        | GameplayEventKind::ExpeditionLogFound { player_id, .. }
        | GameplayEventKind::ExpeditionLogSealed { player_id, .. }
        | GameplayEventKind::ExpeditionEcho { player_id, .. }
        | GameplayEventKind::AnomalyEntered { player_id, .. }
        | GameplayEventKind::DistortionLandmarkFound { player_id, .. }
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
        | GameplayEventKind::BaseServiceUsed { player_id, .. }
        | GameplayEventKind::GotUp { player_id }
        | GameplayEventKind::SleepRuleChanged { player_id, .. }
        | GameplayEventKind::SkyBandEntered { player_id, .. }
        | GameplayEventKind::UpdraftCaught { player_id }
        | GameplayEventKind::RigIcing { player_id }
        | GameplayEventKind::SkyCacheOpened { player_id, .. }
        | GameplayEventKind::BuoyRefuelled { player_id }
        | GameplayEventKind::JetStreamCaught { player_id }
        | GameplayEventKind::PilotExited { player_id, .. }
        | GameplayEventKind::PilotBoarded { player_id, .. }
        | GameplayEventKind::PilotJumped { player_id }
        | GameplayEventKind::PilotLanded { player_id, .. }
        | GameplayEventKind::PilotLedgeGrab { player_id }
        | GameplayEventKind::PilotSplashed { player_id }
        | GameplayEventKind::RigSubmerged { player_id, .. }
        | GameplayEventKind::RigSurfaced { player_id, .. }
        | GameplayEventKind::PilotHurt { player_id, .. }
        | GameplayEventKind::PilotDied { player_id, .. }
        | GameplayEventKind::PilotOxygenLow { player_id, .. }
        | GameplayEventKind::JetpackIgnited { player_id }
        | GameplayEventKind::JetpackEmpty { player_id }
        | GameplayEventKind::PickStruck { player_id, .. }
        | GameplayEventKind::PickClinked { player_id }
        | GameplayEventKind::PackTransferred { player_id, .. }
        | GameplayEventKind::PersonTalked { player_id, .. }
        | GameplayEventKind::PersonTraded { player_id, .. }
        | GameplayEventKind::PersonGifted { player_id, .. }
        | GameplayEventKind::AffinityChanged { player_id, .. }
        | GameplayEventKind::RumourMarked { player_id, .. }
        | GameplayEventKind::PeopleInteraction { player_id }
        | GameplayEventKind::ArcOffered { player_id, .. }
        | GameplayEventKind::ArcAccepted { player_id, .. }
        | GameplayEventKind::ArcStepDone { player_id, .. }
        | GameplayEventKind::ArcClimax { player_id, .. }
        | GameplayEventKind::ArcCompleted { player_id, .. }
        | GameplayEventKind::ArcAbandoned { player_id, .. }
        | GameplayEventKind::UpgradeInstalled { player_id, .. }
        | GameplayEventKind::MilestoneEarned { player_id, .. }
        | GameplayEventKind::ArcChained { player_id, .. }
        | GameplayEventKind::GravityThrow { player_id, .. }
        | GameplayEventKind::TetherAttached { player_id, .. }
        | GameplayEventKind::TetherReleased { player_id }
        | GameplayEventKind::FlakBurst { player_id, .. }
        | GameplayEventKind::TempleLeverPulled { player_id, .. }
        | GameplayEventKind::TempleGateOpened { player_id, .. }
        | GameplayEventKind::RelicChestOpened { player_id, .. }
        | GameplayEventKind::MoonVacuumBreach { player_id, .. }
        | GameplayEventKind::MoonLanded { player_id }
        | GameplayEventKind::ColonyServiceUsed { player_id, .. }
        | GameplayEventKind::FacilityPylon { player_id, .. }
        | GameplayEventKind::FacilityGateOpened { player_id }
        | GameplayEventKind::MoonCoreReached { player_id }
        | GameplayEventKind::UplinkBeamed { player_id, .. }
        | GameplayEventKind::PilotFired { player_id, .. }
        | GameplayEventKind::PilotWeaponDry { player_id, .. }
        | GameplayEventKind::PilotWeaponSwapped { player_id, .. }
        | GameplayEventKind::PilotRope { player_id, .. }
        | GameplayEventKind::PilotGlide { player_id, .. }
        | GameplayEventKind::ChestOpened { player_id, .. }
        | GameplayEventKind::RigRecall { player_id, .. }
        | GameplayEventKind::PilotExposed { player_id, .. }
        | GameplayEventKind::PilotWallJumped { player_id }
        | GameplayEventKind::PlotBought { player_id, .. }
        | GameplayEventKind::ContractSigned { player_id, .. }
        | GameplayEventKind::SiteEventAnswered { player_id, .. }
        | GameplayEventKind::FurniturePlaced { player_id, .. }
        | GameplayEventKind::FurnitureRemoved { player_id, .. }
        | GameplayEventKind::HomeRushed { player_id, .. }
        | GameplayEventKind::HomeInteraction { player_id }
        | GameplayEventKind::TombEntered { player_id, .. }
        | GameplayEventKind::TombFloorCleared { player_id, .. }
        | GameplayEventKind::TombRelicPicked { player_id, .. }
        | GameplayEventKind::TombShrineRested { player_id, .. }
        | GameplayEventKind::TombCacheOpened { player_id, .. }
        | GameplayEventKind::TombMemberDowned { player_id, .. }
        | GameplayEventKind::TombRunEnded { player_id, .. }
        | GameplayEventKind::CryptRoll { player_id, .. }
        | GameplayEventKind::CryptPitFall { player_id, .. }
        // Descent II, the Old Deep (Wave 8).
        | GameplayEventKind::ResonanceChain { player_id, .. }
        | GameplayEventKind::FossilBoneFreed { player_id, .. }
        | GameplayEventKind::SkeletonCompleted { player_id, .. }
        | GameplayEventKind::AmberStuck { player_id }
        | GameplayEventKind::CathedralValveOpened { player_id, .. }
        | GameplayEventKind::CathedralLiftRestored { player_id }
        | GameplayEventKind::DoseWarning { player_id, .. }
        | GameplayEventKind::MutationGained { player_id, .. }
        | GameplayEventKind::PistonStruck { player_id }
        | GameplayEventKind::DeepLogRead { player_id, .. }
        | GameplayEventKind::StarCoreBeaten { player_id }
        | GameplayEventKind::GlowmotherDefeated { player_id }
        | GameplayEventKind::HeartLandmarkFound { player_id, .. }
        | GameplayEventKind::PlaceChanged { player_id, .. }
        | GameplayEventKind::Flight { player_id, .. }
        | GameplayEventKind::SpaceSystems { player_id, .. }
        | GameplayEventKind::BodyLife { player_id, .. }
        | GameplayEventKind::Alien { player_id, .. }
        | GameplayEventKind::Station { player_id, .. } => Some(player_id),
        // Building news reaches the plot's owner wherever they are.
        GameplayEventKind::ConstructionStage { owner, .. }
        | GameplayEventKind::HomeCompleted { owner, .. }
        | GameplayEventKind::SiteEventRaised { owner, .. } => Some(owner),
        GameplayEventKind::WardenDefeated { by }
        | GameplayEventKind::TombGuardianDefeated { by } => by,
        GameplayEventKind::LightningArc { struck, .. } => struck,
        GameplayEventKind::WardenAwakened { .. } | GameplayEventKind::WardenStaggered { .. } => {
            None
        }
        GameplayEventKind::MechanismUsed { player_id, .. }
        | GameplayEventKind::TombSeal { player_id, .. }
        | GameplayEventKind::LivingDark { player_id, .. }
        | GameplayEventKind::Drowned { player_id, .. }
        | GameplayEventKind::Hollow { player_id, .. }
        | GameplayEventKind::Distortion { player_id, .. }
        | GameplayEventKind::FarStrange { player_id, .. }
        | GameplayEventKind::Supply { player_id, .. }
        | GameplayEventKind::AncientCity { player_id, .. }
        | GameplayEventKind::Garrison { player_id, .. }
        | GameplayEventKind::Heart { player_id, .. }
        | GameplayEventKind::Story { player_id, .. }
        | GameplayEventKind::Planet { player_id, .. }
        | GameplayEventKind::Wilds { player_id, .. }
        | GameplayEventKind::Frontier { player_id, .. }
        | GameplayEventKind::Hostile { player_id, .. } => player_id,
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
        | GameplayEventKind::IsleCollapsing { .. }
        | GameplayEventKind::EnemyAlerted { .. }
        | GameplayEventKind::EnemyWindup { .. }
        | GameplayEventKind::EnemyFired { .. }
        | GameplayEventKind::EnemyHurt { .. }
        | GameplayEventKind::EnemyKilled { .. }
        | GameplayEventKind::EnemyStunned { .. }
        | GameplayEventKind::ProjectileImpact { .. }
        | GameplayEventKind::EntityDamaged { .. }
        | GameplayEventKind::EntityDestroyed { .. }
        | GameplayEventKind::Dawn { .. }
        | GameplayEventKind::Dusk { .. }
        | GameplayEventKind::NewDay { .. }
        | GameplayEventKind::NightSkipped { .. }
        | GameplayEventKind::SkyEnemyAction { .. }
        | GameplayEventKind::NightFell { .. }
        | GameplayEventKind::NightSurvived { .. }
        | GameplayEventKind::RareNightSpawn { .. }
        | GameplayEventKind::NightMonsterBurrowed { .. }
        | GameplayEventKind::TombAmbush { .. }
        | GameplayEventKind::TombTrapSprung { .. }
        | GameplayEventKind::TombGuardianPhase { .. }
        | GameplayEventKind::CryptTrap { .. }
        | GameplayEventKind::CryptLordPhase { .. }
        | GameplayEventKind::CryptTell { .. }
        | GameplayEventKind::CryptSlam { .. }
        | GameplayEventKind::CryptSummon { .. }
        | GameplayEventKind::FluidBreached { .. }
        | GameplayEventKind::FluidReacted { .. }
        | GameplayEventKind::SupportCreaked { .. }
        | GameplayEventKind::SupportCollapsed { .. }
        | GameplayEventKind::SlateShudder { .. }
        | GameplayEventKind::SlateSlid { .. }
        | GameplayEventKind::CrustCreature { .. }
        | GameplayEventKind::GasTideTurned { .. }
        | GameplayEventKind::MachineryWoke { .. }
        | GameplayEventKind::HeatStormChanged { .. }
        | GameplayEventKind::LavaPoured { .. }
        | GameplayEventKind::StarCoreAwoke
        | GameplayEventKind::StarCorePhase { .. }
        | GameplayEventKind::GlowmotherAwoke { .. } => None,
    }
}

/// Rows under the local ground a rig still hears the day's sounds (dawn, dusk, midnight).
const CLOCK_CUE_DEPTH_ROWS: i32 = 8;

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

/// Tags the cues added since `stamped` with `store` (the store of the event that made them).
fn stamp_cue_stores(
    cues: &mut [PlacedCue],
    stamped: &mut usize,
    store: fatherload_simulation::place::StoreKey,
) {
    for cue in cues.iter_mut().skip(*stamped) {
        cue.store = store;
    }
    *stamped = cues.len();
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
                store: fatherload_simulation::place::StoreKey::Home,
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
    fn sky_cues_are_throttled_per_rig_and_kind() {
        // A rig hovering on a band's floor or an updraft's edge crosses it every few ticks.
        let local = PlayerId::new(1);
        let world = WorldState::new_game(local);
        let mut effects = Effects::default();
        let crossings: Vec<RuntimeEvent> = (0..6)
            .flat_map(|tick| {
                [
                    event(
                        tick,
                        local,
                        GameplayEventKind::SkyBandEntered {
                            player_id: local,
                            band: fatherload_simulation::sky::SkyBand::LowIsles,
                        },
                    ),
                    event(
                        tick,
                        local,
                        GameplayEventKind::UpdraftCaught { player_id: local },
                    ),
                ]
            })
            .collect();
        assert_eq!(
            effects.apply(&crossings, &world, &[local]),
            vec![
                PlacedCue::player(SoundCue::SkyBand, local),
                PlacedCue::player(SoundCue::Updraft, local)
            ]
        );
        effects.update(1.0);
        assert!(effects.apply(&crossings, &world, &[local]).is_empty());
        effects.update(SKY_CUE_COOLDOWN_SECONDS);
        assert_eq!(effects.apply(&crossings, &world, &[local]).len(), 2);
    }

    #[test]
    fn reward_fanfares_play_once_per_rig_and_milestones_never_repeat() {
        use fatherload_simulation::economy::UpgradeKind;
        let local = PlayerId::new(1);
        let world = WorldState::new_game(local);
        let mut effects = Effects::default();
        // Buying a tier that earns a milestone: one fanfare, the upgrade's.
        let purchase = [
            event(
                3,
                local,
                GameplayEventKind::UpgradeInstalled {
                    player_id: local,
                    upgrade: UpgradeKind::Drill,
                    level: 5,
                },
            ),
            event(
                3,
                local,
                GameplayEventKind::MilestoneEarned {
                    player_id: local,
                    milestone: Milestone::DrillTier5,
                },
            ),
        ];
        assert_eq!(
            effects.apply(&purchase, &world, &[local]),
            vec![PlacedCue::player(SoundCue::UpgradeFanfare, local)]
        );
        // The Act II unlock beats the upgrade fanfare.
        let act_two = [
            event(
                9,
                local,
                GameplayEventKind::UpgradeInstalled {
                    player_id: local,
                    upgrade: UpgradeKind::FuelTank,
                    level: 13,
                },
            ),
            event(
                9,
                local,
                GameplayEventKind::MilestoneEarned {
                    player_id: local,
                    milestone: Milestone::ActTwoUnlocked,
                },
            ),
        ];
        assert_eq!(
            effects.apply(&act_two, &world, &[local]),
            vec![PlacedCue::player(SoundCue::ActTwoFanfare, local)]
        );
        // A predicting client handed the same milestone again a few ticks later stays quiet.
        let depth = |tick| {
            [event(
                tick,
                local,
                GameplayEventKind::MilestoneEarned {
                    player_id: local,
                    milestone: Milestone::ReachedStone,
                },
            )]
        };
        assert_eq!(
            effects.apply(&depth(20), &world, &[local]),
            vec![PlacedCue::player(SoundCue::MilestoneFanfare, local)]
        );
        effects.update(0.1);
        assert!(effects.apply(&depth(23), &world, &[local]).is_empty());
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
