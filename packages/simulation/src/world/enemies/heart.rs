//! The Heart's creatures (Wave 10, Descent VIII): what sets them apart from the generic state
//! machine in `world/enemies.rs`, and the Heart's fight.
//!
//! * **Null moths** flutter to the rig carrying the most dark matter; a touch drinks a lump
//!   out of its bay ([`HeartEvent::MothDrank`]) and the moth flits off. **Upfall beetles** are
//!   full of dark matter: they hang on roofs (they fall up), drop on a rig straight below and
//!   float back up. **Vein gnawers** chew through the shale (the generic burrower).
//! * **Orbiters** circle the nearest gravity well and sling themselves out of the orbit at a
//!   rig in range (the generic lunge); **accretion motes** swirl tight round a well's point.
//!   **Horizon lurkers** sit on rims and fire bolts (generic ranged).
//! * **Mercury eels** live in the metal sea (they spawn only in it and swim back to it) and
//!   leap at a rig above ([`HeartEvent::EelLeapt`]); **rail leeches** crawl on the rails;
//!   **slag skimmers** skim low over the metal and spit slag.
//! * **Corona wisps** burn; **cable wardens** guard the cable (generic).
//! * **The Heart** ([`WorldState::advance_heart_fight`]): wakes when a living rig enters its
//!   chamber. Three phases, every beat telegraphed and every timing a pure function of world
//!   time since the phase began (so hosts and clients agree):
//!   1. **The coils**: six drive coils orbit the sealed core ([`coil_angle`]) and fire slow
//!      plasma bolts; every [`PULSE_PERIOD_MILLIS`] the drive pulls (a contracting ring of
//!      light for [`PULSE_TELL_MILLIS`] first). Break all six.
//!   2. **Ignition**: the core opens. Two beams sweep the chamber ([`beam_angle`], fading in
//!      for [`BEAM_TELL_MILLIS`] before they burn) and sparks fly. Strike the core.
//!   3. **Reforge** (below [`REFORGE_AT_PERMILLE`], Wave 11 carryover): the core reseals and
//!      calls three inner coils that orbit close the other way round ([`inner_coil_angle`]),
//!      firing faster, while the gravity pulses return. Break them and the core **rekindles**:
//!      a second ignition whose beams turn back the other way, faster.
//!   4. **Collapse** (below [`COLLAPSE_AT_PERMILLE`]): the core becomes a singularity and pulls
//!      steadily, armoured except while it exhales ([`breath_at`]: it brightens first, then
//!      pushes out and can be hurt).
//!   5. **Eclipse** (below [`ECLIPSE_AT_PERMILLE`], Wave 11 carryover): the breathing goes on,
//!      and one beam sweeps the chamber while the core holds its breath ([`eclipse_beam`]); the
//!      exhale, when it can be struck, is also when the beam goes dark.
//!
//!   No countdown: with no rig in the chamber for [`SLEEP_MILLIS`] it sinks back to sleep and
//!   heals. When it falls it **wakes for good**: the cable lights, space opens and every rig
//!   of the crew gets the Star Drive. On a Peaceful world the chamber yields unfought.

use crate::{
    enemy::{DamageOutcome, EnemyKind, EnemyMode, EnemyState, ShotKind, tier_scale},
    entity::{Body, EntityId, EntityState},
    event::{GameplayEventKind, TickOutput},
    heart::{
        BEAM_DAMAGE_PER_SECOND, BEAM_HALF_WIDTH_TILES, BREATHING_MAX_SPARKS,
        BREATHING_SPARK_MILLIS, BreathState, CHAMBER_RADIUS, COIL_COUNT, COIL_FIRE_MILLIS,
        COIL_ORBIT_TILES, COLLAPSE_AT_PERMILLE, COLLAPSE_PULL, CORE_TOUCH_DAMAGE_PER_SECOND,
        ECLIPSE_AT_PERMILLE, EXHALE_PUSH, HeartEvent, HeartPhase, INNER_COIL_COUNT,
        INNER_FIRE_MILLIS, INNER_ORBIT_TILES, MAX_SPARKS, PULSE_PULL, PulseState,
        REFORGE_AT_PERMILLE, SLEEP_MILLIS, SPARK_MILLIS, breath_at, coil_angle, fight_beams,
        inner_coil_angle, pulse_at,
    },
    ids::{PlayerId, SimulationTick},
    item::ItemKind,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

#[cfg(doc)]
use crate::heart::{
    BEAM_TELL_MILLIS, PULSE_PERIOD_MILLIS, PULSE_TELL_MILLIS, beam_angle, eclipse_beam,
};

use super::{Intent, WorldState, direction};
use crate::world::heart::heart_event;

/// How long a null moth flits off after it drinks.
pub const MOTH_FLEE_MILLIS: u64 = 3_500;
/// Rows a mercury eel keeps under the metal's surface while it swims.
const EEL_DEPTH_TILES: f32 = 1.2;

impl WorldState {
    // --- Hooks for the generic state machine ---------------------------------------------------

    /// Whether the director may place `kind` (the boss and its parts live only in its fight).
    pub(in crate::world) const fn heart_spawn_allowed(kind: EnemyKind) -> bool {
        !matches!(
            kind,
            EnemyKind::StarHeart | EnemyKind::DriveCoil | EnemyKind::HeartSpark
        )
    }

    /// The Heart's say on where `kind` can appear: `Some(fits)` decides, `None` leaves it to
    /// the generic habitat.
    pub(in crate::world) fn heart_habitat(
        &self,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> Option<bool> {
        if !kind.is_heart() {
            return None;
        }
        // Nothing roams into the Heart's chamber: the fight is the drive's.
        if self.in_heart_chamber(tile) {
            return Some(false);
        }
        let metal = |at: TilePosition| {
            self.terrain
                .tile(at)
                .is_some_and(|t| t.kind == TileKind::Fluid(crate::fluid::FluidKind::LiquidMetal))
        };
        let air = |at: TilePosition| {
            self.terrain
                .tile(at)
                .is_some_and(|t| t.kind == TileKind::Air)
        };
        let up = |at: TilePosition, rows: i32| TilePosition {
            x: at.x,
            y: at.y - rows,
        };
        match kind {
            // In the metal, a little under its surface.
            EnemyKind::MercuryEel => Some(metal(tile) && metal(up(tile, 1))),
            // Under a roof, hanging from it.
            EnemyKind::UpfallBeetle => {
                Some(air(tile) && (1..=2).any(|rows| self.terrain.is_solid_at(up(tile, rows))))
            }
            // On a rail.
            EnemyKind::RailLeech => Some(
                air(tile)
                    && self
                        .terrain
                        .tile(TilePosition {
                            x: tile.x,
                            y: tile.y + 1,
                        })
                        .is_some_and(
                            |t| matches!(t.kind, TileKind::Core(core) if core.is_mag_rail()),
                        ),
            ),
            // Low over the metal sea.
            EnemyKind::SlagSkimmer => Some(
                air(tile)
                    && (2..=5).any(|rows| {
                        metal(TilePosition {
                            x: tile.x,
                            y: tile.y + rows,
                        })
                    }),
            ),
            EnemyKind::StarHeart | EnemyKind::DriveCoil | EnemyKind::HeartSpark => Some(false),
            _ => None,
        }
    }

    /// Whether the Heart's fight carries `kind` itself (the core and its coils).
    pub(in crate::world) const fn heart_holds(kind: EnemyKind) -> bool {
        matches!(kind, EnemyKind::StarHeart | EnemyKind::DriveCoil)
    }

    /// The Heart creatures' own ways (`None` leaves the generic behaviour).
    #[allow(clippy::too_many_lines, reason = "one arm per creature's way")]
    pub(super) fn heart_behave(
        &self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
    ) -> Option<Intent> {
        let spec = state.kind.stats();
        let frame = self.frame();
        let (home_x, home_y) = frame.centre_of(state.home.0, state.home.1);
        let target = state
            .target
            .and_then(|player| self.players.get(&player))
            .map(|rig| (rig.x, rig.y));
        match state.kind {
            EnemyKind::NullMoth if matches!(state.mode, EnemyMode::Idle | EnemyMode::Chase) => {
                // Drawn to the rig carrying the most dark matter near it.
                let reach = spec.sight_tiles * TILE_SIZE;
                let lure = self
                    .players
                    .iter()
                    .filter(|(_, rig)| rig.hull > 0.0)
                    .map(|(id, rig)| {
                        let dark = rig
                            .cargo_bay
                            .count(ItemKind::Layer(crate::descent::Stratum::DarkMatterVeins));
                        (*id, rig.x, rig.y, dark)
                    })
                    .filter(|(_, x, y, dark)| *dark > 0 && (x - body.x).hypot(y - body.y) < reach)
                    .max_by_key(|(id, _, _, dark)| (*dark, std::cmp::Reverse(*id)));
                let (player, x, y, _) = lure?;
                state.target = Some(player);
                state.last_seen = now;
                state.mode = EnemyMode::Chase;
                Some(Intent {
                    goal: Some((x, y)),
                    speed: spec.speed,
                })
            }
            EnemyKind::UpfallBeetle => {
                let below = target.is_some_and(|(x, y)| {
                    y > body.y + TILE_SIZE && (x - body.x).abs() < TILE_SIZE * 2.5
                });
                match state.mode {
                    // Lined up over prey: the generic lunge drops it.
                    EnemyMode::Chase if below => None,
                    EnemyMode::Windup | EnemyMode::Strike => None,
                    // Otherwise it floats back up to the roof (it falls up).
                    _ => Some(Intent {
                        goal: Some((body.x, TILE_SIZE.mul_add(-3.0, body.y))),
                        speed: spec.speed * 0.7,
                    }),
                }
            }
            EnemyKind::Orbiter | EnemyKind::AccretionMote => {
                let in_range = target.is_some_and(|(x, y)| {
                    (x - body.x).hypot(y - body.y) <= spec.attack_range_tiles * TILE_SIZE
                });
                if state.kind == EnemyKind::Orbiter
                    && (matches!(state.mode, EnemyMode::Windup | EnemyMode::Strike)
                        || (state.mode == EnemyMode::Chase && in_range))
                {
                    return None;
                }
                if state.kind == EnemyKind::AccretionMote
                    && state.mode == EnemyMode::Chase
                    && in_range
                {
                    return None;
                }
                let (well, _) = self.well_at(body.x, body.y)?;
                let (cx, cy) = frame.tile_center(well.centre);
                #[allow(clippy::cast_precision_loss, reason = "a well's radius in tiles")]
                let ring = well.radius as f32
                    * if state.kind == EnemyKind::Orbiter {
                        0.72
                    } else {
                        0.42
                    }
                    * TILE_SIZE;
                // Ahead along its ring (a smooth orbit, each one its own way round).
                let angle = crate::field::trig::atan2(body.y - cy, body.x - cx);
                let way: f32 = if state.seed & 1 == 0 { 1.0 } else { -1.0 };
                let (sin, cos) = crate::field::trig::sin_cos(crate::field::trig::wrap_angle(
                    way.mul_add(0.5, angle),
                ));
                Some(Intent {
                    goal: Some((cos.mul_add(ring, cx), sin.mul_add(ring, cy))),
                    speed: spec.speed,
                })
            }
            EnemyKind::MercuryEel => match state.mode {
                EnemyMode::Windup | EnemyMode::Strike => None,
                EnemyMode::Chase
                    if target.is_some_and(|(x, y)| {
                        y < body.y
                            && (x - body.x).hypot(y - body.y) <= spec.attack_range_tiles * TILE_SIZE
                    }) =>
                {
                    None
                }
                _ => {
                    // Back under the metal near home, weaving.
                    let surface = self.metal_surface_above(frame.tile_at(home_x, home_y));
                    let depth = surface.map_or(home_y, |row| {
                        EEL_DEPTH_TILES.mul_add(
                            TILE_SIZE,
                            frame.tile_center(TilePosition { x: 0, y: row }).1,
                        )
                    });
                    let sway: f32 = if (now.get() / 90 + u64::from(state.seed)).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    };
                    state.mode = if state.mode == EnemyMode::Chase {
                        EnemyMode::Chase
                    } else {
                        EnemyMode::Idle
                    };
                    Some(Intent {
                        goal: Some((sway.mul_add(TILE_SIZE * 5.0, home_x), depth.max(home_y))),
                        speed: spec.speed * 0.8,
                    })
                }
            },
            _ => None,
        }
    }

    /// The row of the metal's surface over `tile` (the first metal row going up from it), if
    /// it is in metal.
    fn metal_surface_above(&self, tile: TilePosition) -> Option<i32> {
        let metal = |row: i32| {
            self.terrain
                .tile(TilePosition { x: tile.x, y: row })
                .is_some_and(|t| t.kind == TileKind::Fluid(crate::fluid::FluidKind::LiquidMetal))
        };
        if !metal(tile.y) {
            return None;
        }
        let mut row = tile.y;
        while row > tile.y - 40 && metal(row - 1) {
            row -= 1;
        }
        Some(row)
    }

    /// A Heart creature's bite landed on `player_id` (after the generic damage): a null moth
    /// drinks a lump of dark matter and flits off.
    pub(in crate::world) fn heart_bite(
        &mut self,
        body: &Body,
        state: &mut EnemyState,
        player_id: PlayerId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if state.kind != EnemyKind::NullMoth {
            return;
        }
        let frame = self.frame();
        let Some(rig) = self.players.get_mut(&player_id) else {
            return;
        };
        let taken = rig
            .cargo_bay
            .remove(ItemKind::Layer(crate::descent::Stratum::DarkMatterVeins), 1);
        if taken > 0 {
            state.mode = EnemyMode::Flee;
            state.mode_until = self.after_millis(now, MOTH_FLEE_MILLIS);
            output.push_event(heart_event(
                HeartEvent::MothDrank,
                Some(player_id),
                frame.tile_at(body.x, body.y),
            ));
        }
    }

    /// The Heart's say on a hit to enemy `id` (after resistances): the core is sealed while its
    /// coils orbit and armoured in the collapse except while it exhales; the reforge's inner
    /// coils are shielded except while the drive pulls (Wave 11 carryover: they glow then).
    pub(in crate::world) fn heart_damage(
        &self,
        id: EntityId,
        state: &EnemyState,
    ) -> Option<DamageOutcome> {
        let fight = &self.heart.fight;
        let elapsed = self
            .millis_at(self.simulation_tick)
            .saturating_sub(fight.phase_millis);
        if state.kind == EnemyKind::DriveCoil && fight.inner.contains(&Some(id.get())) {
            let open = pulse_at(elapsed) != PulseState::Calm;
            return (!open).then_some(DamageOutcome::Ignored);
        }
        if state.kind != EnemyKind::StarHeart {
            return None;
        }
        let open = match fight.phase {
            HeartPhase::Ignition => true,
            HeartPhase::Collapse | HeartPhase::Eclipse => breath_at(elapsed) == BreathState::Exhale,
            _ => false,
        };
        (!open).then_some(DamageOutcome::Ignored)
    }

    /// A Heart creature died: a coil broke, a spark went out, the Heart woke.
    pub(in crate::world) fn heart_kill(
        &mut self,
        id: u64,
        kind: EnemyKind,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        match kind {
            EnemyKind::DriveCoil => {
                let fight = &mut self.heart.fight;
                if let Some(slot) = fight
                    .coils
                    .iter_mut()
                    .chain(fight.inner.iter_mut())
                    .find(|slot| **slot == Some(id))
                {
                    *slot = None;
                    output.push_event(heart_event(HeartEvent::CoilBroken, by, tile));
                }
            }
            EnemyKind::HeartSpark => self.heart.fight.sparks.retain(|spark| *spark != id),
            EnemyKind::StarHeart => {
                self.heart.fight.entity = None;
                let millis = self.millis_at(self.simulation_tick.next());
                self.heart_wakes(by, millis, output);
            }
            _ => {}
        }
    }

    // --- The Heart's fight ----------------------------------------------------------------------

    /// The core's body and state while it is awake.
    fn heart_parts(&self) -> Option<(EntityId, Body, EnemyState)> {
        let id = EntityId(self.heart.fight.entity?);
        let (body, state) = self.enemy_parts(id)?;
        Some((id, body, state))
    }

    /// Living rigs inside the chamber, with their positions.
    fn rigs_in_chamber(&self) -> Vec<(PlayerId, f32, f32)> {
        let frame = self.frame();
        self.players
            .iter()
            .filter(|(_, rig)| rig.hull > 0.0)
            .filter(|(_, rig)| self.in_heart_chamber(rig.tile_in(frame)))
            .map(|(id, rig)| (*id, rig.x, rig.y))
            .collect()
    }

    /// Where coil `index` stands at `elapsed` ms into the coils phase (local pixels).
    fn coil_point(centre: (f32, f32), index: usize, elapsed: u64) -> (f32, f32) {
        let (sin, cos) = crate::field::trig::sin_cos(coil_angle(index, elapsed));
        let radius = COIL_ORBIT_TILES * TILE_SIZE;
        (cos.mul_add(radius, centre.0), sin.mul_add(radius, centre.1))
    }

    /// Where the reforge's inner coil `index` stands at `elapsed` ms into it (local pixels).
    fn inner_coil_point(centre: (f32, f32), index: usize, elapsed: u64) -> (f32, f32) {
        let (sin, cos) = crate::field::trig::sin_cos(inner_coil_angle(index, elapsed));
        let radius = INNER_ORBIT_TILES * TILE_SIZE;
        (cos.mul_add(radius, centre.0), sin.mul_add(radius, centre.1))
    }

    /// Moves the coils of `ring` to their orbit points (`point(index)`) and fires each one's
    /// bolt in turn every `fire_millis` at the nearest rig; a coil gone is forgotten. Returns
    /// the ring with the gone ones cleared.
    #[allow(clippy::too_many_arguments, reason = "one ring's beat")]
    fn turn_coil_ring(
        &mut self,
        ring: &[Option<u64>],
        point: impl Fn(usize) -> (f32, f32),
        fire_millis: u64,
        (elapsed, previous): (u64, u64),
        inside: &[(PlayerId, f32, f32)],
        output: &mut TickOutput,
    ) -> Vec<Option<u64>> {
        let count = ring.len().max(1) as u64;
        let mut kept = ring.to_vec();
        for (index, slot) in ring.iter().enumerate() {
            let Some(id) = *slot else {
                continue;
            };
            let id = EntityId(id);
            if self.entities.get(id).is_none() {
                kept[index] = None;
                continue;
            }
            let (x, y) = point(index);
            self.entities.update(id, |entity| {
                entity.body.previous_x = entity.body.x;
                entity.body.previous_y = entity.body.y;
                entity.body.x = quantize(x);
                entity.body.y = quantize(y);
                entity.body.velocity_x = 0.0;
                entity.body.velocity_y = 0.0;
            });
            // Each fires a slow bolt at the nearest rig in turn round the ring.
            let offset = index as u64 * (fire_millis / count);
            if (elapsed + offset) / fire_millis != (previous + offset) / fire_millis
                && elapsed > 2_000
                && let Some(aim) = Self::nearest(inside, (x, y))
                && let Some((body, state)) = self.enemy_parts(id)
            {
                self.fire(id, &body, &state, ShotKind::Fireball, 1, aim, output);
            }
        }
        kept
    }

    /// The gravity pulses (the coils' and the reforge's): told, then pulling.
    fn heart_pulses(
        &mut self,
        (elapsed, previous): (u64, u64),
        inside: &[(PlayerId, f32, f32)],
        centre: (f32, f32),
        centre_tile: TilePosition,
        output: &mut TickOutput,
    ) {
        let pulse = pulse_at(elapsed);
        if pulse != pulse_at(previous) {
            match pulse {
                PulseState::Tell => {
                    output.push_event(heart_event(HeartEvent::PulseTell, None, centre_tile));
                }
                PulseState::Pull => {
                    output.push_event(heart_event(HeartEvent::Pulse, None, centre_tile));
                }
                PulseState::Calm => {}
            }
        }
        if pulse == PulseState::Pull {
            self.pull_rigs(inside, centre, PULSE_PULL * self.timing.delta_seconds);
        }
    }

    /// The fight's beams at `elapsed` burn the rigs in their path ([`fight_beams`]).
    fn heart_beams_burn(
        &mut self,
        elapsed: u64,
        inside: &[(PlayerId, f32, f32)],
        centre: (f32, f32),
        scale: f32,
        output: &mut TickOutput,
    ) {
        let fight = &self.heart.fight;
        let beams = fight_beams(fight.phase, fight.rekindled, elapsed);
        let seconds = self.timing.delta_seconds;
        for (angle, strength) in beams {
            if strength < 1.0 {
                continue;
            }
            let (sin, cos) = crate::field::trig::sin_cos(angle);
            for (player_id, x, y) in inside {
                let (dx, dy) = ((*x - centre.0) / TILE_SIZE, (*y - centre.1) / TILE_SIZE);
                // Along the beam (it runs out from the core, not behind it) and across it.
                let along = dx.mul_add(cos, dy * sin);
                let across = dx.mul_add(sin, -dy * cos).abs();
                #[allow(clippy::cast_precision_loss, reason = "a chamber's radius")]
                let in_chamber = dx.hypot(dy) <= CHAMBER_RADIUS as f32;
                if along >= 0.0
                    && across <= BEAM_HALF_WIDTH_TILES
                    && in_chamber
                    && self.damage_player(*player_id, BEAM_DAMAGE_PER_SECOND * scale * seconds)
                {
                    output.push_event(GameplayEventKind::PlayerDamaged {
                        player_id: *player_id,
                    });
                }
            }
        }
    }

    /// The core's health share, per mille (0 once it is gone).
    fn heart_health(&self) -> u16 {
        self.heart_parts()
            .map_or(0, |(_, _, state)| health_permille(&state))
    }

    /// Calls the reforge's inner coils round the core.
    fn call_inner_coils(&mut self, centre: (f32, f32)) {
        let Some(centre_tile) = self.heart_centre() else {
            return;
        };
        let tier = self.tier_at(centre_tile);
        let core = self.heart.fight.entity.map(EntityId);
        let inner = (0..INNER_COIL_COUNT)
            .map(|index| {
                let point = Self::inner_coil_point(centre, index, 0);
                Some(
                    self.spawn_enemy_with(EnemyKind::DriveCoil, point, tier, None, core)
                        .get(),
                )
            })
            .collect();
        self.heart.fight.inner = inner;
    }

    /// One tick of the Heart's fight (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the fight's beats in order")]
    pub(in crate::world) fn advance_heart_fight(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.heart.fight.beaten {
            return;
        }
        let Some(centre_tile) = self.heart_centre() else {
            return;
        };
        let frame = self.frame();
        let centre = frame.tile_center(centre_tile);
        let inside = self.rigs_in_chamber();
        if !inside.is_empty() {
            self.heart.fight.last_visited = next_tick;
        }
        let millis = self.millis_at(next_tick);
        let before = self.millis_at(self.simulation_tick);
        if self.heart.fight.phase == HeartPhase::Asleep {
            let Some((player, _, _)) = inside.first().copied() else {
                return;
            };
            // A peaceful world has no creatures: the chamber yields to the first rig.
            if !self.enemies.difficulty.hostile() {
                self.heart_wakes(Some(player), millis, output);
                return;
            }
            self.wake_heart_fight(centre, millis, Some(player), output);
            return;
        }
        // Nobody stayed: it sinks back to sleep and heals.
        let idle = next_tick
            .get()
            .saturating_sub(self.heart.fight.last_visited.get());
        if inside.is_empty() && idle > self.timing.ticks_for_millis(SLEEP_MILLIS) {
            self.put_heart_to_sleep();
            output.push_event(heart_event(HeartEvent::HeartSlept, None, centre_tile));
            return;
        }
        if self.heart_parts().is_none() {
            // Its core is gone without waking (a save from before the fight, a despawn).
            self.put_heart_to_sleep();
            return;
        }
        let phase = self.heart.fight.phase;
        let elapsed = millis.saturating_sub(self.heart.fight.phase_millis);
        let previous = before.saturating_sub(self.heart.fight.phase_millis);
        let seconds = self.timing.delta_seconds;
        let scale = tier_scale(self.tier_at(centre_tile));
        // Every phase: the core's touch burns and throws a rig back.
        for (player_id, x, y) in &inside {
            let (dx, dy) = (*x - centre.0, *y - centre.1);
            let distance = dx.hypot(dy);
            if distance < EnemyKind::StarHeart.contact_half().0 + 14.0 {
                if self.damage_player(*player_id, CORE_TOUCH_DAMAGE_PER_SECOND * scale * seconds) {
                    output.push_event(GameplayEventKind::PlayerDamaged {
                        player_id: *player_id,
                    });
                }
                if let Some(rig) = self.players.get_mut(player_id) {
                    let ((nx, ny), _) = direction(centre, (*x, *y));
                    rig.velocity_x = quantize(nx.mul_add(260.0, rig.velocity_x));
                    rig.velocity_y = quantize(ny.mul_add(260.0, rig.velocity_y));
                }
            }
        }
        match phase {
            HeartPhase::Coils => {
                // The coils ride their orbit; a broken one is gone.
                let ring = self.heart.fight.coils.clone();
                self.heart.fight.coils = self.turn_coil_ring(
                    &ring,
                    |index| Self::coil_point(centre, index, elapsed),
                    COIL_FIRE_MILLIS,
                    (elapsed, previous),
                    &inside,
                    output,
                );
                self.heart_pulses((elapsed, previous), &inside, centre, centre_tile, output);
                if self.heart.fight.coils_left() == 0 {
                    self.begin_heart_phase(HeartPhase::Ignition, millis);
                    output.push_event(heart_event(HeartEvent::Ignition, None, centre_tile));
                }
            }
            HeartPhase::Ignition => {
                // The beams, once they have faded in (back the other way once rekindled).
                self.heart_beams_burn(elapsed, &inside, centre, scale, output);
                self.throw_sparks(centre, millis, output, centre_tile);
                let health = self.heart_health();
                if !self.heart.fight.rekindled && health < REFORGE_AT_PERMILLE {
                    // The core reseals and calls its inner coils.
                    self.begin_heart_phase(HeartPhase::Reforge, millis);
                    self.call_inner_coils(centre);
                    output.push_event(heart_event(HeartEvent::Reforged, None, centre_tile));
                } else if self.heart.fight.rekindled && health < COLLAPSE_AT_PERMILLE {
                    self.begin_heart_phase(HeartPhase::Collapse, millis);
                    output.push_event(heart_event(HeartEvent::Collapse, None, centre_tile));
                }
            }
            HeartPhase::Reforge => {
                let ring = self.heart.fight.inner.clone();
                self.heart.fight.inner = self.turn_coil_ring(
                    &ring,
                    |index| Self::inner_coil_point(centre, index, elapsed),
                    INNER_FIRE_MILLIS,
                    (elapsed, previous),
                    &inside,
                    output,
                );
                self.heart_pulses((elapsed, previous), &inside, centre, centre_tile, output);
                if self.heart.fight.inner.iter().flatten().next().is_none() {
                    // Rekindled: the second ignition.
                    self.heart.fight.rekindled = true;
                    self.begin_heart_phase(HeartPhase::Ignition, millis);
                    output.push_event(heart_event(HeartEvent::Ignition, None, centre_tile));
                }
            }
            HeartPhase::Collapse | HeartPhase::Eclipse => {
                let breath = breath_at(elapsed);
                let was = breath_at(previous);
                if breath != was {
                    match breath {
                        BreathState::Tell => {
                            output.push_event(heart_event(
                                HeartEvent::ExhaleTell,
                                None,
                                centre_tile,
                            ));
                        }
                        BreathState::Exhale => {
                            output.push_event(heart_event(HeartEvent::Exhale, None, centre_tile));
                            // The exhale's ring of light blows the sparks out (Wave 11
                            // carryover): in the long breathing phases they never swarm, and
                            // the window to strike is clear of them.
                            for spark in std::mem::take(&mut self.heart.fight.sparks) {
                                self.entities.despawn(EntityId(spark));
                            }
                        }
                        BreathState::Held => {}
                    }
                }
                if breath == BreathState::Exhale {
                    self.pull_rigs(&inside, centre, -EXHALE_PUSH * seconds);
                } else {
                    self.pull_rigs(&inside, centre, COLLAPSE_PULL * seconds);
                }
                self.throw_sparks(centre, millis, output, centre_tile);
                if phase == HeartPhase::Eclipse {
                    self.heart_beams_burn(elapsed, &inside, centre, scale, output);
                } else if self.heart_health() < ECLIPSE_AT_PERMILLE {
                    self.begin_heart_phase(HeartPhase::Eclipse, millis);
                    output.push_event(heart_event(HeartEvent::Eclipse, None, centre_tile));
                }
            }
            HeartPhase::Asleep | HeartPhase::Awakened => {}
        }
    }

    /// The rig of `rigs` nearest `from`.
    fn nearest(rigs: &[(PlayerId, f32, f32)], from: (f32, f32)) -> Option<(f32, f32)> {
        rigs.iter().map(|(_, x, y)| (*x, *y)).min_by(|a, b| {
            let da = (a.0 - from.0).hypot(a.1 - from.1);
            let db = (b.0 - from.0).hypot(b.1 - from.1);
            da.total_cmp(&db)
        })
    }

    /// Nudges every rig of `rigs` toward `centre` by `dv` px/s (away for a negative `dv`).
    fn pull_rigs(&mut self, rigs: &[(PlayerId, f32, f32)], centre: (f32, f32), dv: f32) {
        for (player_id, x, y) in rigs {
            let ((nx, ny), distance) = direction((*x, *y), centre);
            if distance < TILE_SIZE {
                continue;
            }
            if let Some(rig) = self.players.get_mut(player_id) {
                rig.velocity_x = quantize(nx.mul_add(dv, rig.velocity_x));
                rig.velocity_y = quantize(ny.mul_add(dv, rig.velocity_y));
            }
        }
    }

    /// The ignited (and collapsing) drive throws sparks now and then.
    fn throw_sparks(
        &mut self,
        centre: (f32, f32),
        millis: u64,
        output: &mut TickOutput,
        centre_tile: TilePosition,
    ) {
        let entities = &self.entities;
        self.heart
            .fight
            .sparks
            .retain(|spark| entities.get(EntityId(*spark)).is_some());
        let (period, most) = if self.heart.fight.phase.breathing() {
            (BREATHING_SPARK_MILLIS, BREATHING_MAX_SPARKS)
        } else {
            (SPARK_MILLIS, MAX_SPARKS)
        };
        if millis.saturating_sub(self.heart.fight.spark_millis) < period
            || self.heart.fight.sparks.len() >= most
        {
            return;
        }
        self.heart.fight.spark_millis = millis;
        let tier = self.tier_at(centre_tile);
        let side: f32 = if (millis / SPARK_MILLIS).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let parent = self.heart.fight.entity.map(EntityId);
        let spark = self.spawn_enemy_with(
            EnemyKind::HeartSpark,
            (
                side.mul_add(TILE_SIZE * 4.0, centre.0),
                TILE_SIZE.mul_add(-3.0, centre.1),
            ),
            tier,
            None,
            parent,
        );
        self.heart.fight.sparks.push(spark.get());
        output.push_event(heart_event(HeartEvent::SparkBorn, None, centre_tile));
    }

    /// Starts `phase` at world time `millis`.
    const fn begin_heart_phase(&mut self, phase: HeartPhase, millis: u64) {
        self.heart.fight.phase = phase;
        self.heart.fight.phase_millis = millis;
        self.heart.fight.spark_millis = millis;
    }

    /// Wakes the fight: the core and its six coils.
    fn wake_heart_fight(
        &mut self,
        centre: (f32, f32),
        millis: u64,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let Some(centre_tile) = self.heart_centre() else {
            return;
        };
        let tier = self.tier_at(centre_tile);
        let core = self.spawn_enemy_with(EnemyKind::StarHeart, centre, tier, None, None);
        self.entities.update(core, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.mode = EnemyMode::Chase;
            }
        });
        let mut coils = Vec::with_capacity(COIL_COUNT);
        for index in 0..COIL_COUNT {
            let point = Self::coil_point(centre, index, 0);
            let coil = self.spawn_enemy_with(EnemyKind::DriveCoil, point, tier, None, Some(core));
            coils.push(Some(coil.get()));
        }
        let fight = &mut self.heart.fight;
        fight.entity = Some(core.get());
        fight.coils = coils;
        fight.sparks.clear();
        self.begin_heart_phase(HeartPhase::Coils, millis);
        output.push_event(heart_event(HeartEvent::HeartStirred, by, centre_tile));
    }

    /// The fight ends without a winner: everything goes, it heals for next time.
    pub(in crate::world) fn put_heart_to_sleep(&mut self) {
        let fight = std::mem::take(&mut self.heart.fight);
        if let Some(entity) = fight.entity {
            self.entities.despawn(EntityId(entity));
        }
        for coil in fight.coils.into_iter().chain(fight.inner).flatten() {
            self.entities.despawn(EntityId(coil));
        }
        for spark in fight.sparks {
            self.entities.despawn(EntityId(spark));
        }
        self.heart.fight.last_visited = fight.last_visited;
    }

    /// The Heart wakes for good: the cable lights, space opens, every rig gets the Star Drive.
    pub(in crate::world) fn heart_wakes(
        &mut self,
        by: Option<PlayerId>,
        millis: u64,
        output: &mut TickOutput,
    ) {
        if self.heart.fight.beaten {
            return;
        }
        let last_visited = self.heart.fight.last_visited;
        self.put_heart_to_sleep();
        let fight = &mut self.heart.fight;
        fight.last_visited = last_visited;
        fight.phase = HeartPhase::Awakened;
        fight.phase_millis = millis;
        fight.beaten = true;
        self.heart.space_unlocked = true;
        self.heart.cable_lit_millis = Some(millis);
        let tile = self.heart_centre().unwrap_or(TilePosition { x: 0, y: 0 });
        output.push_event(heart_event(HeartEvent::HeartAwakened, by, tile));
        output.push_event(heart_event(HeartEvent::CableLit, by, tile));
        output.push_event(heart_event(HeartEvent::SpaceUnlocked, by, tile));
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in players {
            if self.heart.star_drive.insert(player_id) {
                output.push_event(heart_event(
                    HeartEvent::StarDriveFitted,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    // --- Staging (scenes, tests) ----------------------------------------------------------------

    /// Wakes the Heart's fight now (a rig must be in the chamber for it to stay awake) and puts
    /// it on `phase` at `elapsed` ms into it (scenes, tests).
    pub fn stage_heart_phase(&mut self, phase: HeartPhase, elapsed: u64) {
        let Some(centre_tile) = self.heart_centre() else {
            return;
        };
        let centre = self.frame().tile_center(centre_tile);
        let millis = self.millis_at(self.simulation_tick);
        let mut sink = TickOutput::default();
        if self.heart.fight.entity.is_none() {
            self.wake_heart_fight(centre, millis, None, &mut sink);
        }
        self.heart.fight.last_visited = self.simulation_tick;
        if phase == HeartPhase::Awakened {
            self.heart_wakes(None, millis.saturating_sub(elapsed), &mut sink);
            return;
        }
        if phase != HeartPhase::Coils {
            for coil in std::mem::take(&mut self.heart.fight.coils)
                .into_iter()
                .flatten()
            {
                self.entities.despawn(EntityId(coil));
            }
            self.heart.fight.coils = vec![None; COIL_COUNT];
        }
        self.begin_heart_phase(phase, millis.saturating_sub(elapsed));
        if phase == HeartPhase::Reforge && self.heart.fight.inner.is_empty() {
            self.call_inner_coils(centre);
        }
        let share = match phase {
            HeartPhase::Collapse => Some(0.4),
            HeartPhase::Eclipse => Some(0.2),
            HeartPhase::Reforge => Some(0.75),
            _ => None,
        };
        if let Some(share) = share
            && let Some((id, _, mut state)) = self.heart_parts()
        {
            state.health = state.max_health * share;
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
        }
    }
}

/// An enemy's health share, per mille.
fn health_permille(state: &EnemyState) -> u16 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a health share in per mille"
    )]
    let share = (state.health / state.max_health.max(1.0) * 1_000.0).clamp(0.0, 1_000.0) as u16;
    share
}
