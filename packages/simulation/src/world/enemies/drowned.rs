//! The Drowned and Frozen's creatures (Wave 9, Descent IV): what sets them apart from the
//! generic state machine in `world/enemies.rs`, the eel-dragon and the Leviathan's fight.
//!
//! * **Reef sharks** circle their prey between dashes instead of hanging back.
//! * **Ink squids** bite, ink the water black (a dark field zone) and dart away.
//! * **Frost mites'** bites frost the hull ([`super::super::drowned::MITE_FROST_PERMILLE`]).
//! * **Tusk seals** belly-slide: a charge that runs straight on until it meets a wall.
//! * **Scald beetles** ride the geysers' jets up onto whatever is above.
//! * **Sinter snails** seal the tunnels behind them with sinter (rarely, like every digger).
//! * **Spinepuffs** only notice what comes within their short sight; then they swell into
//!   spines (the renderer draws them inflated while they chase).
//! * **Gulper eels'** gulp stalls the rig they bite.
//! * **Lure anglers** drag rigs in their light (`world/drowned.rs`).
//! * **The eel-dragon** ([`WorldState::advance_eel_dragon`]) wakes when a rig nears its grotto
//!   and, besides its lunges, crackles and then pulses electricity through the water round it.
//! * **The Leviathan** ([`WorldState::advance_leviathan`]): wakes when a rig enters its arena
//!   and swims its own pattern (its pose is a pure function of its move and the world time, so
//!   every tick rate agrees). Circling (above 2/3 of its health): it laps the arena and, after
//!   a telegraph (it stops, its eyes flare, it roars), charges across it. The maelstrom (above
//!   1/3): it alternates charges with sinking to the floor and opening its maw, the whirlpool
//!   pulling rigs in while the open maw takes double damage, then spits pressure bubbles.
//!   Thrashing: faster telegraphs, each charge shakes boulders off the roof, and it calls two
//!   reef sharks once. Nothing is on a clock: an arena empty for
//!   [`LEVIATHAN_SLEEP_MILLIS`] puts it back to sleep, healed.

use crate::{
    drowned::{
        DrownedEvent, DrownedLore, EEL_BOUNTY, EEL_PULSE_DAMAGE, EEL_PULSE_MILLIS,
        EEL_PULSE_TELL_MILLIS, EEL_PULSE_TILES, EEL_WAKE_TILES, GEYSER_LIFT_SPEED,
        LEVIATHAN_ATTACK_GAP_MILLIS, LEVIATHAN_BUBBLES, LEVIATHAN_CHARGE_MILLIS,
        LEVIATHAN_LAP_MILLIS, LEVIATHAN_MAW_MILLIS, LEVIATHAN_RISE_MILLIS, LEVIATHAN_ROCKFALL,
        LEVIATHAN_SHARKS, LEVIATHAN_SINK_MILLIS, LEVIATHAN_SLEEP_MILLIS, LEVIATHAN_SPIT_MILLIS,
        LEVIATHAN_TELL_MILLIS, LeviathanMove, LeviathanPhase, LeviathanState,
    },
    enemy::{EnemyKind, EnemyMode, EnemyState, ShotKind, tier_scale},
    entity::{Body, EntityId, EntityState},
    event::{GameplayEventKind, TickOutput},
    field::trig,
    ids::{PlayerId, SimulationTick},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{TileRect, descent::drowned::ARENA_SIZE},
};

use super::{Intent, WorldState};
use crate::world::drowned::drowned_event;

/// How long an ink squid flees after inking the water.
const SQUID_FLEE_MILLIS: u64 = 2_400;
/// How often a sinter snail may seal a tunnel tile behind it.
const SNAIL_SEAL_MILLIS: u64 = 6_000;
/// A shark circles its prey this far out between dashes, tiles.
const SHARK_CIRCLE_TILES: f32 = 4.5;
/// How much of its speed a gulper's gulp leaves a rig.
const GULP_KEEP: f32 = 0.15;
/// The Leviathan's swim: half its lap's width and height, tiles (its body stays in the water).
#[allow(clippy::cast_precision_loss, reason = "forty tiles")]
const SWIM_RADII: (f32, f32) = ((ARENA_SIZE.0 / 2 - 12) as f32, 8.0);
/// Its telegraph while it thrashes (faster).
const THRASH_TELL_MILLIS: u64 = 1_000;
/// How far past its target a charge carries, tiles.
const CHARGE_OVERSHOOT_TILES: f32 = 8.0;

impl WorldState {
    /// The Drowned and Frozen's say on who `state`'s creature notices: the Leviathan knows
    /// every rig in its arena; `None` leaves the generic perception.
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn drowned_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        if state.kind != EnemyKind::Leviathan {
            return None;
        }
        let arena = self.leviathan_arena()?;
        let frame = self.frame();
        Some(
            self.players
                .iter()
                .filter(|(_, player)| {
                    player.hull > 0.0 && arena.expanded(2).contains(player.tile_in(frame))
                })
                .map(|(id, player)| ((player.x - body.x).hypot(player.y - body.y), *id))
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                .map(|(_, id)| id),
        )
    }

    /// Whether the Drowned and Frozen lets `kind` roam (its bosses live in their set pieces).
    pub(in crate::world) const fn drowned_spawn_allowed(kind: EnemyKind) -> bool {
        !matches!(kind, EnemyKind::EelDragon | EnemyKind::Leviathan)
    }

    /// Whether a sea creature of `kind` may appear swimming at `tile`: deep water all round.
    pub(in crate::world) fn drowned_swim_site(&self, kind: EnemyKind, tile: TilePosition) -> bool {
        if !kind.is_drowned() || !kind.can_swim() {
            return false;
        }
        let wet = |dx: i32, dy: i32| {
            self.terrain
                .fluid_at(TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                })
                .is_some_and(|cell| cell.level >= crate::fluid::FLUID_FULL / 2)
        };
        wet(0, 0)
            && (-2..=2)
                .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| wet(*dx, *dy))
                .count()
                >= 14
    }

    /// A Drowned creature's own chase, `Some` to replace the generic one: a reef shark circles
    /// its prey while its dash recharges.
    pub(super) fn drowned_chase(
        &self,
        state: &EnemyState,
        target: (f32, f32),
        ready: bool,
        now: SimulationTick,
    ) -> Option<Intent> {
        match state.kind {
            EnemyKind::ReefShark if !ready => Some(Intent {
                goal: Some(self.orbit(target, state, now, SHARK_CIRCLE_TILES * TILE_SIZE)),
                speed: state.kind.stats().speed * 1.15,
            }),
            _ => None,
        }
    }

    /// After a Drowned creature moved: the seal's straight slide, the beetle's geyser ride, the
    /// snail's sealing.
    pub(super) fn drowned_after_move(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        now: SimulationTick,
    ) {
        match state.kind {
            EnemyKind::TuskSeal if state.mode == EnemyMode::Strike => {
                // The belly slide runs straight on (ice or not) until it meets a wall.
                let dash = state.kind.stats().dash_speed;
                let ahead = f32::from(state.facing.signum())
                    .mul_add(state.kind.stats().radius + 2.0, body.x);
                if crate::physics::solid_at(&self.terrain, ahead, body.y) {
                    state.mode = EnemyMode::Recover;
                    state.mode_until = self.after(now, 500);
                    body.velocity_x = 0.0;
                } else {
                    body.velocity_x = quantize(f32::from(state.facing.signum()) * dash);
                }
            }
            EnemyKind::ScaldBeetle => {
                if let Some(jet) = self.geyser_jet_at(body.x, body.y)
                    && jet.phase == crate::drowned::GeyserPhase::Erupting
                {
                    body.velocity_y = quantize(body.velocity_y.min(-GEYSER_LIFT_SPEED * 0.8));
                }
            }
            EnemyKind::SinterSnail if !state.mode.hostile() => {
                let every = self.timing.ticks_for_millis(SNAIL_SEAL_MILLIS).max(1);
                if (now.get() + id.get()).is_multiple_of(every) {
                    self.snail_seals(body, state);
                }
            }
            _ => {}
        }
    }

    /// A sinter snail crusts over the open tile behind it (never onto a body or into fluid).
    fn snail_seals(&mut self, body: &Body, state: &EnemyState) {
        let frame = self.frame();
        let here = frame.tile_at(body.x, body.y);
        let behind = TilePosition {
            x: here.x - i32::from(state.facing.signum()),
            y: here.y,
        };
        if self.terrain.tile(behind).map(|tile| tile.kind) != Some(TileKind::Air)
            || self.deep_stratum_at(behind) != Some(crate::descent::Stratum::GeyserFields)
        {
            return;
        }
        let (cx, cy) = frame.tile_center(behind);
        let occupied =
            self.players.values().any(|player| {
                (player.x - cx).abs() < TILE_SIZE && (player.y - cy).abs() < TILE_SIZE
            }) || self.pilots().any(|(_, pilot, _)| {
                (pilot.x - cx).abs() < TILE_SIZE && (pilot.y - cy).abs() < TILE_SIZE * 1.5
            });
        if !occupied {
            let _ = self
                .terrain
                .set_kind(behind, TileKind::Cave(crate::cave::CaveTile::Sinter));
        }
    }

    /// A Drowned creature's bite landed on `player_id`'s rig.
    pub(super) fn drowned_bite(
        &mut self,
        body: &Body,
        state: &mut EnemyState,
        player_id: PlayerId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        match state.kind {
            EnemyKind::FrostMite => self.mite_bite(player_id),
            EnemyKind::InkSquid => {
                let tile = self.frame().tile_at(body.x, body.y);
                self.spawn_ink_cloud(tile, output);
                state.mode = EnemyMode::Flee;
                state.mode_until = self.after(now, SQUID_FLEE_MILLIS);
            }
            EnemyKind::GulperEel => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.velocity_x = quantize(player.velocity_x * GULP_KEEP);
                    player.velocity_y = quantize(player.velocity_y * GULP_KEEP);
                }
            }
            _ => {}
        }
    }

    /// Damage multiplier on `kind` now: the Leviathan's open maw takes double.
    pub(in crate::world) fn drowned_damage_factor(&self, kind: EnemyKind) -> f32 {
        if kind == EnemyKind::Leviathan && self.drowned.leviathan.maw_open() {
            crate::drowned::MAW_DAMAGE_FACTOR
        } else {
            1.0
        }
    }

    // --- The eel-dragon -----------------------------------------------------------------------

    /// One tick of the eel-dragon (see the module docs).
    pub(in crate::world) fn advance_eel_dragon(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let eel = self.drowned.eel;
        if eel.beaten || !self.enemies.difficulty.hostile() {
            return;
        }
        let Some(grotto) = self.eel_grotto() else {
            return;
        };
        let frame = self.frame();
        if let Some(entity) = eel.entity
            && self.entities.get(EntityId(entity)).is_none()
        {
            // Gone some other way (despawned far from every rig): it sleeps.
            self.drowned.eel.entity = None;
            self.drowned.eel.pulse_at = None;
            return;
        }
        let near: Option<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .find(|(_, player)| {
                grotto
                    .expanded(EEL_WAKE_TILES)
                    .contains(player.tile_in(frame))
            })
            .map(|(id, _)| *id);
        let Some(entity) = eel.entity else {
            if let Some(player) = near {
                let centre = grotto.center();
                let (x, y) = frame.tile_center(centre);
                let tier = self.tier_at(centre);
                let id = self.spawn_enemy_with(EnemyKind::EelDragon, (x, y), tier, None, None);
                self.drowned.eel.entity = Some(id.get());
                self.drowned.eel.next_pulse = self.after_millis(now, EEL_PULSE_MILLIS);
                output.push_event(drowned_event(
                    DrownedEvent::EelDragonAwoke,
                    Some(player),
                    centre,
                ));
            }
            return;
        };
        let Some((body, state)) = self.enemy_parts(EntityId(entity)) else {
            return;
        };
        let at = frame.tile_at(body.x, body.y);
        if let Some(pulse_at) = eel.pulse_at {
            if now >= pulse_at {
                // The pulse: every rig in the water near it is jolted.
                let reach = EEL_PULSE_TILES * TILE_SIZE;
                let damage = EEL_PULSE_DAMAGE * tier_scale(state.tier);
                let struck: Vec<PlayerId> = self
                    .players
                    .iter()
                    .filter(|(_, player)| {
                        (player.x - body.x).hypot(player.y - body.y) <= reach
                            && self.submersion(player.x, player.y, 8.0).kind.is_some()
                    })
                    .map(|(id, _)| *id)
                    .collect();
                for player_id in struck {
                    if self.damage_player(player_id, damage) {
                        output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                    }
                }
                self.drowned.eel.pulse_at = None;
                self.drowned.eel.next_pulse = self.after_millis(now, EEL_PULSE_MILLIS);
            }
        } else if now >= eel.next_pulse && state.mode.hostile() {
            self.drowned.eel.pulse_at = Some(self.after_millis(now, EEL_PULSE_TELL_MILLIS));
            output.push_event(drowned_event(DrownedEvent::EelDragonPulse, None, at));
        }
    }

    /// The eel-dragon fell: its bounty to the killer (or every rig near the grotto).
    fn eel_dragon_fell_inner(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        self.drowned.eel.beaten = true;
        self.drowned.eel.entity = None;
        self.drowned.eel.pulse_at = None;
        if let Some(player) = by.and_then(|id| self.players.get_mut(&id)) {
            player.credits = player.credits.saturating_add(EEL_BOUNTY);
        }
        output.push_event(drowned_event(DrownedEvent::EelDragonDefeated, by, tile));
    }

    // --- The Leviathan ------------------------------------------------------------------------

    /// The arena's centre in local pixels and the Leviathan's swim radii in pixels.
    fn leviathan_frame(&self, arena: &TileRect) -> ((f32, f32), (f32, f32)) {
        let centre = self.frame().tile_center(arena.center());
        (centre, (SWIM_RADII.0 * TILE_SIZE, SWIM_RADII.1 * TILE_SIZE))
    }

    /// Where it surfaces its maw: on the arena's floor, mid-arena (an offset from the centre).
    fn maw_offset(arena: &TileRect) -> (f32, f32) {
        let floor = crate::worldgen::descent::drowned::arena_floor_row(arena);
        #[allow(clippy::cast_precision_loss, reason = "a few dozen rows")]
        let rows = (floor - arena.center().y) as f32 - 1.6;
        (0.0, rows * TILE_SIZE)
    }

    /// The point of its lap at `angle` (radians), an offset from the arena's centre.
    fn lap_point(radii: (f32, f32), angle: f32) -> (f32, f32) {
        let (sin, cos) = trig::sin_cos(angle);
        (radii.0 * cos, radii.1 * sin)
    }

    /// Its pose now (an offset from the arena's centre), a pure function of its move and the
    /// world time since the move began.
    #[allow(
        clippy::cast_precision_loss,
        reason = "milliseconds and pixels are small"
    )]
    fn leviathan_offset(&self, arena: &TileRect, state: &LeviathanState) -> (f32, f32) {
        let (_, radii) = self.leviathan_frame(arena);
        let elapsed = self
            .millis_at(self.simulation_tick)
            .saturating_sub(self.millis_at(state.since)) as f32;
        let lerp = |from: (f32, f32), to: (f32, f32), millis: u64| {
            let t = (elapsed / millis.max(1) as f32).clamp(0.0, 1.0);
            // Ease in and out.
            let t = t * t * 2.0f32.mul_add(-t, 3.0);
            (
                (to.0 - from.0).mul_add(t, from.0),
                (to.1 - from.1).mul_add(t, from.1),
            )
        };
        let point = |p: (i32, i32)| (p.0 as f32, p.1 as f32);
        match state.action {
            LeviathanMove::Circle => {
                let turn = std::f32::consts::TAU * elapsed / LEVIATHAN_LAP_MILLIS as f32;
                let start = state.angle_milli as f32 / 1_000.0;
                let angle = if state.clockwise {
                    start + turn
                } else {
                    start - turn
                };
                Self::lap_point(radii, trig::wrap_angle(angle))
            }
            LeviathanMove::ChargeTell { at, .. } => point(at),
            LeviathanMove::Charge { from, to } => {
                lerp(point(from), point(to), LEVIATHAN_CHARGE_MILLIS)
            }
            LeviathanMove::Sink { from } => {
                lerp(point(from), Self::maw_offset(arena), LEVIATHAN_SINK_MILLIS)
            }
            LeviathanMove::Maw | LeviathanMove::Spit => Self::maw_offset(arena),
            LeviathanMove::Rise => lerp(
                Self::maw_offset(arena),
                Self::lap_point(radii, std::f32::consts::FRAC_PI_2),
                LEVIATHAN_RISE_MILLIS,
            ),
        }
    }

    /// The maelstrom's eye (local pixels) while its maw is open, else `None`.
    pub(in crate::world) fn maelstrom_centre(&self) -> Option<(f32, f32)> {
        let state = &self.drowned.leviathan;
        if !state.awake() || !matches!(state.action, LeviathanMove::Maw) {
            return None;
        }
        let arena = self.leviathan_arena()?;
        let (centre, _) = self.leviathan_frame(&arena);
        let (dx, dy) = Self::maw_offset(&arena);
        Some((centre.0 + dx, centre.1 + dy))
    }

    /// The Leviathan's pose in local pixels and its move (presentation and tests).
    #[must_use]
    pub fn leviathan_pose(&self) -> Option<((f32, f32), LeviathanMove)> {
        let state = &self.drowned.leviathan;
        let entity = state.entity?;
        let body = self.entities.get(EntityId(entity))?.body;
        Some(((body.x, body.y), state.action))
    }

    /// The nearest rig in the arena to `from` (pixels from the arena's centre) and its offset.
    fn leviathan_target(&self, arena: &TileRect, from: (f32, f32)) -> Option<(f32, f32)> {
        let (centre, _) = self.leviathan_frame(arena);
        let frame = self.frame();
        self.players
            .values()
            .filter(|player| player.hull > 0.0 && arena.expanded(2).contains(player.tile_in(frame)))
            .map(|player| (player.x - centre.0, player.y - centre.1))
            .min_by(|a, b| {
                (a.0 - from.0)
                    .hypot(a.1 - from.1)
                    .total_cmp(&(b.0 - from.0).hypot(b.1 - from.1))
            })
    }

    /// One tick of the Leviathan's fight (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the fight's beats in order")]
    pub(in crate::world) fn advance_leviathan(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.drowned.leviathan.beaten {
            // The gate stays open for the whole crew: a rig that joins later is fitted too.
            self.grant_leviathan_rewards();
            return;
        }
        let Some(arena) = self.leviathan_arena() else {
            return;
        };
        let frame = self.frame();
        let inside: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| {
                player.hull > 0.0 && arena.expanded(2).contains(player.tile_in(frame))
            })
            .map(|(id, _)| *id)
            .collect();
        if !inside.is_empty() {
            self.drowned.leviathan.last_visited = now;
        }
        let centre_tile = arena.center();
        if let Some(entity) = self.drowned.leviathan.entity
            && self.entities.get(EntityId(entity)).is_none()
        {
            self.put_leviathan_to_sleep();
        }
        if !self.drowned.leviathan.awake() {
            // A peaceful world has no creatures, so no Leviathan to fight: it yields its gate
            // to the first rig in its arena.
            if !self.enemies.difficulty.hostile() {
                if let Some(player) = inside.first().copied() {
                    self.leviathan_fell(centre_tile, Some(player), output);
                }
                return;
            }
            let Some(player) = inside.first().copied() else {
                return;
            };
            let (centre, radii) = self.leviathan_frame(&arena);
            let start = Self::lap_point(radii, 0.0);
            let tier = self.tier_at(centre_tile);
            let id = self.spawn_enemy_with(
                EnemyKind::Leviathan,
                (centre.0 + start.0, centre.1 + start.1),
                tier,
                None,
                None,
            );
            self.drowned.leviathan = LeviathanState {
                entity: Some(id.get()),
                since: now,
                last_visited: now,
                clockwise: true,
                ..LeviathanState::default()
            };
            output.push_event(drowned_event(
                DrownedEvent::LeviathanAwoke,
                Some(player),
                centre_tile,
            ));
            return;
        }
        let idle = now
            .get()
            .saturating_sub(self.drowned.leviathan.last_visited.get());
        if inside.is_empty() && idle > self.timing.ticks_for_millis(LEVIATHAN_SLEEP_MILLIS) {
            self.put_leviathan_to_sleep();
            return;
        }
        let Some(entity) = self.drowned.leviathan.entity.map(EntityId) else {
            return;
        };
        let Some((_, enemy)) = self.enemy_parts(entity) else {
            return;
        };
        // Its phase follows its health.
        let phase = LeviathanPhase::of(enemy.health_fraction());
        if phase != self.drowned.leviathan.phase {
            self.drowned.leviathan.phase = phase;
            output.push_event(drowned_event(
                DrownedEvent::LeviathanPhase,
                None,
                centre_tile,
            ));
            if phase == LeviathanPhase::Thrashing && !self.drowned.leviathan.sharks_called {
                self.drowned.leviathan.sharks_called = true;
                self.call_sharks(&arena);
            }
        }
        let state = self.drowned.leviathan;
        let elapsed = self
            .millis_at(now)
            .saturating_sub(self.millis_at(state.since));
        let here = self.leviathan_offset(&arena, &state);
        #[allow(clippy::cast_possible_truncation, reason = "pixel offsets in an arena")]
        let pixel = |p: (f32, f32)| (p.0.round() as i32, p.1.round() as i32);
        let tell = if phase == LeviathanPhase::Thrashing {
            THRASH_TELL_MILLIS
        } else {
            LEVIATHAN_TELL_MILLIS
        };
        let next = match state.action {
            LeviathanMove::Circle => {
                let gap = if phase == LeviathanPhase::Thrashing {
                    LEVIATHAN_ATTACK_GAP_MILLIS * 3 / 5
                } else {
                    LEVIATHAN_ATTACK_GAP_MILLIS
                };
                if elapsed < gap || inside.is_empty() {
                    None
                } else if phase == LeviathanPhase::Maelstrom && state.attacks % 2 == 1 {
                    Some(LeviathanMove::Sink { from: pixel(here) })
                } else {
                    self.leviathan_target(&arena, here).map(|target| {
                        output.push_event(drowned_event(
                            DrownedEvent::LeviathanCharge,
                            None,
                            centre_tile,
                        ));
                        LeviathanMove::ChargeTell {
                            at: pixel(here),
                            target: pixel(target),
                        }
                    })
                }
            }
            LeviathanMove::ChargeTell { at, target } if elapsed >= tell => {
                let (_, radii) = self.leviathan_frame(&arena);
                #[allow(clippy::cast_precision_loss, reason = "pixel offsets")]
                let (ax, ay) = (at.0 as f32, at.1 as f32);
                #[allow(clippy::cast_precision_loss, reason = "pixel offsets")]
                let (tx, ty) = (target.0 as f32, target.1 as f32);
                let (dx, dy) = (tx - ax, ty - ay);
                let length = dx.hypot(dy).max(1.0);
                let reach = CHARGE_OVERSHOOT_TILES * TILE_SIZE;
                let end = (
                    (dx / length).mul_add(reach, tx).clamp(-radii.0, radii.0),
                    (dy / length)
                        .mul_add(reach, ty)
                        .clamp(-radii.1 * 1.4, Self::maw_offset(&arena).1),
                );
                Some(LeviathanMove::Charge {
                    from: at,
                    to: pixel(end),
                })
            }
            LeviathanMove::Charge { .. } if elapsed >= LEVIATHAN_CHARGE_MILLIS => {
                if phase == LeviathanPhase::Thrashing {
                    self.shake_the_roof(&arena, &inside);
                }
                Some(LeviathanMove::Circle)
            }
            LeviathanMove::Sink { .. } if elapsed >= LEVIATHAN_SINK_MILLIS => {
                Some(LeviathanMove::Maw)
            }
            LeviathanMove::Maw if elapsed >= LEVIATHAN_MAW_MILLIS => {
                self.spit_bubbles(entity, &enemy, &inside, output);
                Some(LeviathanMove::Spit)
            }
            LeviathanMove::Spit if elapsed >= LEVIATHAN_SPIT_MILLIS => Some(LeviathanMove::Rise),
            LeviathanMove::Rise if elapsed >= LEVIATHAN_RISE_MILLIS => Some(LeviathanMove::Circle),
            _ => None,
        };
        if let Some(action) = next {
            let lev = &mut self.drowned.leviathan;
            if matches!(action, LeviathanMove::Circle) {
                // Pick its lap up where it is.
                let ((), radii) = ((), (SWIM_RADII.0 * TILE_SIZE, SWIM_RADII.1 * TILE_SIZE));
                let angle = trig::atan2(here.1 / radii.1, here.0 / radii.0);
                #[allow(clippy::cast_possible_truncation, reason = "radians times a thousand")]
                let milli = (angle * 1_000.0).round() as i32;
                lev.angle_milli = milli;
                lev.clockwise = !lev.clockwise;
            }
            if matches!(
                action,
                LeviathanMove::ChargeTell { .. } | LeviathanMove::Sink { .. }
            ) {
                lev.attacks = lev.attacks.wrapping_add(1);
            }
            lev.action = action;
            lev.since = now;
        }
        // Move its body to its pose (the velocity is the step, for smooth drawing).
        let state = self.drowned.leviathan;
        let (centre, _) = self.leviathan_frame(&arena);
        let pose = self.leviathan_offset(&arena, &state);
        // Its pose for the renderer: 1 roaring before a charge, 2 its maw agape.
        let look = match state.action {
            LeviathanMove::ChargeTell { .. } => 1,
            LeviathanMove::Maw | LeviathanMove::Spit => 2,
            _ => 0,
        };
        let dt = self.timing.delta_seconds.max(1e-3);
        self.entities.update(entity, |body_entity| {
            let (x, y) = (quantize(centre.0 + pose.0), quantize(centre.1 + pose.1));
            body_entity.body.velocity_x = quantize((x - body_entity.body.x) / dt);
            body_entity.body.velocity_y = quantize((y - body_entity.body.y) / dt);
            body_entity.body.previous_x = body_entity.body.x;
            body_entity.body.previous_y = body_entity.body.y;
            body_entity.body.x = x;
            body_entity.body.y = y;
            if let EntityState::Enemy(enemy) = &mut body_entity.state {
                if (x - body_entity.body.previous_x).abs() > 0.5 {
                    enemy.facing = if x < body_entity.body.previous_x {
                        -1
                    } else {
                        1
                    };
                }
                // It never tires: always hunting while awake.
                if enemy.mode != EnemyMode::Stunned {
                    enemy.mode = EnemyMode::Chase;
                }
                enemy.phase = look;
            }
        });
    }

    /// It spits pressure bubbles at every rig in its arena.
    fn spit_bubbles(
        &mut self,
        entity: EntityId,
        enemy: &EnemyState,
        inside: &[PlayerId],
        output: &mut TickOutput,
    ) {
        let Some((body, _)) = self.enemy_parts(entity) else {
            return;
        };
        let targets: Vec<(f32, f32)> = inside
            .iter()
            .filter_map(|id| self.players.get(id).map(|player| (player.x, player.y)))
            .collect();
        for aim in targets {
            self.fire(
                entity,
                &body,
                enemy,
                ShotKind::PressureBubble,
                LEVIATHAN_BUBBLES,
                aim,
                output,
            );
        }
    }

    /// Its thrash shakes boulders off the arena's roof over the rigs.
    fn shake_the_roof(&mut self, arena: &TileRect, inside: &[PlayerId]) {
        let frame = self.frame();
        let roof = frame.row_px(arena.min.y + crate::worldgen::descent::drowned::AIR_ROWS + 2);
        let columns: Vec<f32> = inside
            .iter()
            .filter_map(|id| self.players.get(id).map(|player| player.x))
            .collect();
        for x in columns {
            for index in 0..LEVIATHAN_ROCKFALL {
                #[allow(clippy::cast_precision_loss, reason = "a few tiles")]
                let offset = (index - LEVIATHAN_ROCKFALL / 2) as f32 * TILE_SIZE * 2.5;
                self.spawn_falling_boulder(x + offset, roof, 900, 4_000);
            }
        }
    }

    /// It calls its reef sharks from the arena's sides.
    fn call_sharks(&mut self, arena: &TileRect) {
        let frame = self.frame();
        let centre = arena.center();
        let tier = self.tier_at(centre);
        for index in 0..LEVIATHAN_SHARKS {
            let side = if index % 2 == 0 { -1 } else { 1 };
            let tile = TilePosition {
                x: centre.x + side * (ARENA_SIZE.0 / 2 - 8),
                y: centre.y,
            };
            let at = frame.tile_center(tile);
            let _ = self.spawn_enemy_with(EnemyKind::ReefShark, at, tier, None, None);
        }
    }

    /// The Leviathan sinks back to sleep: its body goes, it heals.
    fn put_leviathan_to_sleep(&mut self) {
        let entity = self.drowned.leviathan.entity.take();
        self.drowned.leviathan = LeviathanState::default();
        if let Some(entity) = entity {
            self.entities.despawn(EntityId(entity));
        }
    }

    /// The eel-dragon fell.
    pub(in crate::world) fn eel_dragon_fell(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        self.eel_dragon_fell_inner(tile, by, output);
    }

    /// The Leviathan fell: the gate opens, every rig is fitted with the Sonar Suite and the
    /// Cryo Hull, and the trench's floor shows what lies under it.
    pub(in crate::world) fn leviathan_fell(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let entity = self.drowned.leviathan.entity.take();
        if let Some(entity) = entity {
            self.entities.despawn(EntityId(entity));
        }
        self.drowned.leviathan = LeviathanState {
            beaten: true,
            ..LeviathanState::default()
        };
        self.grant_leviathan_rewards();
        output.push_event(drowned_event(DrownedEvent::LeviathanDefeated, by, tile));
        let rigs: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in rigs {
            output.push_event(drowned_event(
                DrownedEvent::GateRewards,
                Some(player_id),
                tile,
            ));
        }
        if !self.drowned.found(DrownedLore::AbyssFloor) {
            self.drowned.lore |= DrownedLore::AbyssFloor.bit();
            output.push_event(drowned_event(DrownedEvent::LoreFound, by, tile));
        }
    }
}
