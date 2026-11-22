//! The far Strange Layers' creatures (Wave 9 strange-b, Descent VI): what sets them apart from
//! the generic state machine in `world/enemies.rs`, and The Glitch's fight.
//!
//! * **Tempo** ([`WorldState::far_tempo`]): tick mites live at twice the world's speed, amber
//!   sloths at half, and every creature inside a time bubble at a quarter. A creature's motion
//!   integrates with its own seconds and its timers (attacks, wind-ups, recoveries, paths) run
//!   on its own clock ([`WorldState::shift_enemy_timers`]), deterministically and the same at
//!   every tick rate.
//! * **Hearing** ([`WorldState::far_notice`]): echo stalkers are blind and hunt by the noise a
//!   rig makes (`WorldState::rig_noise`); hush moths drift to the loudest rig (and their hush
//!   silences it); The Glitch and its shards know every rig in their arena.
//! * **Colours**: a prism scarab's shell cracks only in the light of its colour
//!   ([`WorldState::far_damage_scale`]); a splitter spire fans one bolt of each colour
//!   ([`prism_shot`]); a lens wraith is unseen (auto-aim too) except in its own colour of light.
//! * **Rewinds** ([`WorldState::try_rewind`]): a rewind newt badly hurt rewinds three seconds
//!   (position and health), twice in its life, never inside a time bubble.
//! * **Lifespans and screams** ([`WorldState::advance_far_creatures`]): tick mites crumble after
//!   [`TICK_MITE_LIFE_SECONDS`]; a shrieker that finds prey screams, pinging the cave and
//!   calling every echo stalker near its prey.
//! * **The Glitch** ([`WorldState::advance_glitch`]): wakes when a rig enters the Rewrite Hall.
//!   At each health threshold it **rewrites the arena's rule** ([`GlitchRule`]: gravity up,
//!   mirroring, time warp, sideways gravity with pixel rot), telegraphed for
//!   [`GLITCH_REWRITE_MILLIS`] while it flickers, cannot attack and cannot be hurt. It blinks
//!   across the arena (its ghost shows where first) and, from its third rule, sheds shards. Its
//!   death fits every rig with the Phase Drill.

use crate::{
    depth_progression::RigSystem,
    enemy::{DamageKind, DamageSource, EnemyKind, EnemyMode, EnemyState, ShotKind},
    entity::{Body, EntityId, EntityState},
    event::TickOutput,
    far_strange::{
        FarStrangeEvent, GLITCH_BLINK_MILLIS, GLITCH_BLINK_TELL_MILLIS, GLITCH_REWRITE_MILLIS,
        GLITCH_SHARD_MILLIS, GLITCH_SLEEP_MILLIS, GLITCH_THRESHOLDS, GLITCH_TIME_WARP, GlitchRule,
        LampFilter, MAX_GLITCH_SHARDS, NewtMoment, REWIND_BELOW, REWIND_CHARGES, REWIND_MILLIS,
        REWIND_SAMPLE_MILLIS, SCARAB_MATCHED, SCARAB_UNMATCHED, SHRIEK_CALL_TILES, SHRIEK_MILLIS,
        TICK_MITE_LIFE_SECONDS, TIME_BUBBLE_TEMPO, creature_tempo, next_rule, scarab_shell,
    },
    field::{FieldProfile, FieldZone, ScreenEffect},
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{TileRect, descent::far_strange::arena_inside, noise::hash2},
};

use super::WorldState;

/// Most moments a rewind newt remembers.
const NEWT_MEMORY: usize = 8;
/// A shrieker's scream pings this far, tiles.
const SHRIEK_PING_TILES: u8 = 16;
/// How far a pilot on foot is heard while it walks, tiles.
const PILOT_NOISE_TILES: f32 = 4.0;

/// The bolt a splitter spire fires as shot `index` of its fan: red, green, blue.
#[must_use]
pub const fn prism_shot(index: usize) -> ShotKind {
    match index % 3 {
        0 => ShotKind::PrismRed,
        1 => ShotKind::PrismGreen,
        _ => ShotKind::PrismBlue,
    }
}

/// The field departure The Glitch's `rule` writes over its arena (`None` for the baseline).
#[must_use]
pub const fn glitch_zone_profile(rule: GlitchRule) -> Option<FieldProfile> {
    let neutral = FieldProfile::NEUTRAL;
    let mut profile = neutral;
    match rule {
        GlitchRule::Baseline => return None,
        GlitchRule::GravityUp => {
            profile = profile.with_gravity(0.0, -1.0);
            profile.free_rotation = true;
            profile.screen = profile
                .screen
                .with(ScreenEffect::ColourGrade, 30)
                .with(ScreenEffect::Scanlines, 25)
                .graded([255, 110, 220]);
        }
        GlitchRule::Mirrored => {
            profile.mirror_controls = true;
            profile.screen = profile
                .screen
                .with(ScreenEffect::Mirror, 100)
                .with(ScreenEffect::Chromatic, 30)
                .with(ScreenEffect::ColourGrade, 20)
                .graded([120, 255, 230]);
        }
        GlitchRule::TimeWarp => {
            profile.time_scale = GLITCH_TIME_WARP;
            profile.screen = profile
                .screen
                .with(ScreenEffect::Wobble, 22)
                .with(ScreenEffect::Bloom, 25)
                .with(ScreenEffect::ColourGrade, 28)
                .graded([160, 220, 255]);
        }
        GlitchRule::Sideways => {
            profile = profile.with_gravity(1.0, 0.0);
            profile.free_rotation = true;
            profile.screen = profile
                .screen
                .with(ScreenEffect::Pixelate, 55)
                .with(ScreenEffect::Palette, 40)
                .with(ScreenEffect::Scanlines, 30);
        }
    }
    Some(profile)
}

impl WorldState {
    /// The far Strange Layers' say on who `state`'s creature notices: `Some(answer)` overrides
    /// the generic perception, `None` leaves it (see the module docs).
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn far_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        match state.kind {
            EnemyKind::EchoStalker | EnemyKind::HushMoth => Some(self.hear(body, state.kind)),
            EnemyKind::TheGlitch | EnemyKind::GlitchShard => {
                let inside = arena_inside(&self.glitch_arena()?).expanded(1);
                let frame = self.frame();
                Some(
                    self.players
                        .iter()
                        .filter(|(_, player)| player.hull > 0.0)
                        .filter(|(_, player)| inside.contains(player.tile_in(frame)))
                        .map(|(id, player)| ((player.x - body.x).hypot(player.y - body.y), *id))
                        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                        .map(|(_, id)| id),
                )
            }
            _ => None,
        }
    }

    /// The rig (or pilot) a blind hunter at `body` hears: the nearest within its noise's reach
    /// along an open path, or half that reach through rock (a stalker); the loudest in reach
    /// (a hush moth, which feeds on sound).
    fn hear(&self, body: &Body, kind: EnemyKind) -> Option<PlayerId> {
        let hearing = kind.stats().hearing_tiles;
        let mut best: Option<(f32, PlayerId)> = None;
        for (player_id, player) in &self.players {
            if player.hull <= 0.0 || self.is_on_foot(*player_id) {
                continue;
            }
            let noise = self.rig_noise(*player_id).min(hearing);
            self.consider_noise(
                body,
                kind,
                *player_id,
                (player.x, player.y),
                noise,
                &mut best,
            );
        }
        for (_, pilot_body, pilot) in self.pilots() {
            let walking = pilot_body.velocity_x.abs() > 20.0;
            let noise = if walking && !self.hushed(pilot_body.x, pilot_body.y) {
                PILOT_NOISE_TILES
            } else {
                0.0
            };
            let point = pilot.middle(pilot_body);
            self.consider_noise(body, kind, pilot.owner, point, noise, &mut best);
        }
        best.map(|(_, id)| id)
    }

    fn consider_noise(
        &self,
        body: &Body,
        kind: EnemyKind,
        player_id: PlayerId,
        (x, y): (f32, f32),
        noise_tiles: f32,
        best: &mut Option<(f32, PlayerId)>,
    ) {
        if noise_tiles <= 0.0 {
            return;
        }
        let distance = (x - body.x).hypot(y - body.y);
        let reach = noise_tiles * TILE_SIZE;
        if distance > reach {
            return;
        }
        if self.hostile_safe_zone(Some(kind), self.frame().tile_at(x, y)) {
            return;
        }
        let heard =
            distance <= reach * 0.5 || self.sound_reaches((body.x, body.y), (x, y), noise_tiles);
        if !heard {
            return;
        }
        // Stalkers go for the nearest; moths for the loudest (the nearest of the loudest).
        let score = if kind == EnemyKind::HushMoth {
            (noise_tiles * TILE_SIZE).mul_add(-4.0, distance)
        } else {
            distance
        };
        if best.is_none_or(|(nearest, _)| score < nearest) {
            *best = Some((score, player_id));
        }
    }

    /// How fast `state`'s creature at `body` lives now: its own tempo, slowed inside a time
    /// bubble (see the module docs).
    #[must_use]
    pub(in crate::world) fn far_tempo(&self, body: &Body, state: &EnemyState) -> f32 {
        let own = creature_tempo(state.kind);
        if self.entities.count_of(crate::entity::EntityKind::Charge) == 0 {
            return own;
        }
        let bubbled = self
            .time_bubbles()
            .into_iter()
            .any(|(x, y, radius)| (body.x - x).hypot(body.y - y) <= radius);
        if bubbled {
            own * TIME_BUBBLE_TEMPO
        } else {
            own
        }
    }

    /// Whether `state`'s creature at `body` is inside a time bubble.
    fn in_time_bubble(&self, body: &Body) -> bool {
        self.time_bubbles()
            .into_iter()
            .any(|(x, y, radius)| (body.x - x).hypot(body.y - y) <= radius)
    }

    /// After a creature living at `tempo` took its tick: its timers move to its own clock. Its
    /// clock advances `floor((now + 1) * tempo) - floor(now * tempo)` ticks this tick, so a
    /// timer still ahead of it is pushed back (slow) or drawn in (fast) by the difference.
    pub(in crate::world) fn shift_enemy_timers(
        &mut self,
        id: EntityId,
        now: SimulationTick,
        tempo: f32,
    ) {
        #[allow(clippy::cast_precision_loss, reason = "tick counts far below 2^52")]
        let clock = |tick: u64| (tick as f64 * f64::from(tempo)).floor();
        let advanced = clock(now.get() + 1) - clock(now.get());
        #[allow(clippy::cast_possible_truncation, reason = "a handful of ticks")]
        let adjust = 1 - advanced as i64;
        if adjust == 0 {
            return;
        }
        let shift = |tick: SimulationTick| {
            if tick <= now {
                return tick;
            }
            let moved = if adjust > 0 {
                tick.get().saturating_add(adjust.unsigned_abs())
            } else {
                tick.get()
                    .saturating_sub(adjust.unsigned_abs())
                    .max(now.get())
            };
            SimulationTick::new(moved)
        };
        self.entities.update(id, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.attack_ready = shift(state.attack_ready);
                state.mode_until = shift(state.mode_until);
                state.next_path = shift(state.next_path);
            }
        });
    }

    /// Whether the far Strange Layers let `kind` spawn roaming (The Glitch and its shards only
    /// appear in its arena).
    pub(in crate::world) const fn far_spawn_allowed(kind: EnemyKind) -> bool {
        !matches!(kind, EnemyKind::TheGlitch | EnemyKind::GlitchShard)
    }

    /// Whether `state`'s creature shrugs off every hit now (The Glitch mid-rewrite).
    pub(in crate::world) const fn far_ignores_damage(&self, state: &EnemyState) -> bool {
        matches!(state.kind, EnemyKind::TheGlitch) && self.far_strange.glitch.rewriting()
    }

    /// The far Strange Layers' damage multiplier on creature `id`: a prism scarab cracks only in
    /// the light of its shell's colour (blasts always land).
    pub(in crate::world) fn far_damage_scale(
        &self,
        id: EntityId,
        state: &EnemyState,
        kind: DamageKind,
        source: DamageSource,
    ) -> f32 {
        if state.kind != EnemyKind::PrismScarab || kind == DamageKind::Explosive {
            return 1.0;
        }
        let shell = scarab_shell(id.get(), self.millis_at(self.simulation_tick));
        match source.player() {
            Some(player) if self.lamp_filter_of(player) == shell => SCARAB_MATCHED,
            _ => SCARAB_UNMATCHED,
        }
    }

    /// The colour of light lens wraith `id` shows in.
    #[must_use]
    pub const fn lens_wraith_colour(id: u64) -> LampFilter {
        LampFilter::keyed(id.wrapping_mul(7).wrapping_add(1))
    }

    /// Whether `player_id` cannot see creature `id` (a lens wraith outside its colour of light:
    /// auto-aim skips it, and the game does not draw it for that seat).
    #[must_use]
    pub fn far_unseen_by(&self, player_id: PlayerId, id: EntityId, state: &EnemyState) -> bool {
        state.kind == EnemyKind::LensWraith
            && self.lamp_filter_of(player_id) != Self::lens_wraith_colour(id.get())
    }

    /// A rewind newt badly hurt rewinds to where it was [`REWIND_MILLIS`] ago, with the health
    /// it had then (twice in its life, never inside a time bubble). Returns whether it did.
    pub(in crate::world) fn try_rewind(
        &mut self,
        id: EntityId,
        body: &mut Body,
        state: &mut EnemyState,
        output: &mut TickOutput,
    ) -> bool {
        if state.kind != EnemyKind::RewindNewt
            || state.health <= 0.0
            || state.health >= state.max_health * REWIND_BELOW
            || state.phase >= REWIND_CHARGES
            || self.in_time_bubble(body)
        {
            return false;
        }
        let now = self.simulation_tick;
        let back = self.timing.ticks_for_millis(REWIND_MILLIS);
        let Some(moment) = self.far_schedule.newts.get(&id.get()).and_then(|moments| {
            moments
                .iter()
                .rev()
                .find(|moment| now.get().saturating_sub(moment.tick.get()) >= back)
                .or_else(|| moments.first())
                .copied()
        }) else {
            return false;
        };
        // Never back into rock dug shut since (the body rules).
        if crate::physics::collides(&self.terrain, moment.x, moment.y, state.kind.stats().radius) {
            return false;
        }
        body.x = moment.x;
        body.y = moment.y;
        body.previous_x = moment.x;
        body.previous_y = moment.y;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        state.health = moment.health.max(state.health);
        state.phase = state.phase.saturating_add(1);
        state.mode = EnemyMode::Chase;
        self.far_schedule.newts.remove(&id.get());
        let tile = self.frame().tile_at(body.x, body.y);
        output.push_event(super::super::far_strange::far_event(
            FarStrangeEvent::Rewound,
            None,
            tile,
        ));
        true
    }

    /// A far Strange Layers creature died (kill credit `by`): The Glitch's fight moves on.
    pub(in crate::world) fn far_kill(
        &mut self,
        id: u64,
        kind: EnemyKind,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        self.far_schedule.newts.remove(&id);
        self.far_schedule.shrieked.remove(&id);
        match kind {
            EnemyKind::GlitchShard => self.far_strange.glitch.shards.retain(|shard| *shard != id),
            EnemyKind::TheGlitch => self.glitch_fell(tile, by, output),
            _ => {}
        }
    }

    /// Lifespans, newts' memories and shriekers' screams (see the module docs).
    pub(in crate::world) fn advance_far_creatures(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let creatures: Vec<(EntityId, Body, EnemyState)> = self
            .enemies()
            .filter(|(_, state)| {
                matches!(
                    state.kind,
                    EnemyKind::TickMite | EnemyKind::RewindNewt | EnemyKind::Shrieker
                )
            })
            .map(|(entity, state)| (entity.id, entity.body, *state))
            .collect();
        if creatures.is_empty() {
            if !self.far_schedule.newts.is_empty() {
                self.far_schedule.newts.clear();
            }
            return;
        }
        let second = self.timing.ticks_for_millis(1_000).max(1);
        let sample = self.timing.ticks_for_millis(REWIND_SAMPLE_MILLIS).max(1);
        let frame = self.frame();
        for (id, body, state) in creatures {
            match state.kind {
                EnemyKind::TickMite => {
                    if !(now.get() + id.get()).is_multiple_of(second) || self.in_time_bubble(&body)
                    {
                        continue;
                    }
                    let age = state.pattern.saturating_add(1);
                    if age >= TICK_MITE_LIFE_SECONDS {
                        self.entities.despawn(id);
                        output.push_event(super::super::far_strange::far_event(
                            FarStrangeEvent::MiteCrumbled,
                            None,
                            frame.tile_at(body.x, body.y),
                        ));
                    } else {
                        self.entities.update(id, |entity| {
                            if let EntityState::Enemy(state) = &mut entity.state {
                                state.pattern = age;
                            }
                        });
                    }
                }
                EnemyKind::RewindNewt => {
                    if !now.get().is_multiple_of(sample) {
                        continue;
                    }
                    let moments = self.far_schedule.newts.entry(id.get()).or_default();
                    moments.push(NewtMoment {
                        tick: now,
                        x: body.x,
                        y: body.y,
                        health: state.health,
                    });
                    while moments.len() > NEWT_MEMORY {
                        moments.remove(0);
                    }
                }
                EnemyKind::Shrieker => self.maybe_shriek(id, &body, &state, now, output),
                _ => {}
            }
        }
        // Newts gone some other way leave the memory too.
        let entities = &self.entities;
        self.far_schedule
            .newts
            .retain(|id, _| entities.get(EntityId(*id)).is_some());
    }

    /// A shrieker that has found prey screams: a ping round it, and every echo stalker near
    /// its prey comes running.
    fn maybe_shriek(
        &mut self,
        id: EntityId,
        body: &Body,
        state: &EnemyState,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if state.mode != EnemyMode::Chase {
            return;
        }
        let Some(prey) = state.target else {
            return;
        };
        let wait = self.timing.ticks_for_millis(SHRIEK_MILLIS);
        if self
            .far_schedule
            .shrieked
            .get(&id.get())
            .is_some_and(|tick| now.get().saturating_sub(tick.get()) < wait)
        {
            return;
        }
        let Some((px, py)) = self.actor_position(prey) else {
            return;
        };
        self.far_schedule.shrieked.insert(id.get(), now);
        let tile = self.frame().tile_at(body.x, body.y);
        self.push_ping(None, tile, SHRIEK_PING_TILES);
        let reach = SHRIEK_CALL_TILES * TILE_SIZE;
        let called: Vec<EntityId> = self
            .enemies()
            .filter(|(entity, other)| {
                other.kind == EnemyKind::EchoStalker
                    && (entity.body.x - px).hypot(entity.body.y - py) <= reach
            })
            .map(|(entity, _)| entity.id)
            .collect();
        for stalker in called {
            self.entities.update(stalker, |entity| {
                if let EntityState::Enemy(stalker) = &mut entity.state
                    && !matches!(stalker.mode, EnemyMode::Stunned | EnemyMode::Flee)
                {
                    stalker.mode = EnemyMode::Chase;
                    stalker.target = Some(prey);
                    stalker.last_seen = now;
                }
            });
        }
        output.push_event(super::super::far_strange::far_event(
            FarStrangeEvent::Shriek,
            Some(prey),
            tile,
        ));
    }

    // --- The Glitch ---------------------------------------------------------------------------

    /// The Glitch's body and state while it is awake.
    fn glitch_parts(&self) -> Option<(EntityId, Body, EnemyState)> {
        let id = EntityId(self.far_strange.glitch.entity?);
        let (body, state) = self.enemy_parts(id)?;
        Some((id, body, state))
    }

    /// Writes `rule` over the arena (its field zone; none for the baseline).
    fn write_glitch_rule(&mut self, rule: GlitchRule, arena: &TileRect) {
        if let Some(zone) = self.far_strange.glitch.zone.take() {
            let _ = self.fields.remove(zone);
        }
        self.far_strange.glitch.rule = rule;
        if let Some(profile) = glitch_zone_profile(rule) {
            // Over the whole hall, walls and floor included, so the rule holds at full strength
            // up to every surface inside it (feathered from the hall's own edge, a rig resting
            // on the floor or a wall felt only a fraction of it and never fell up or sideways).
            let zone = FieldZone::new(
                (arena.min.x, arena.min.y),
                (arena.max.x, arena.max.y),
                1,
                profile,
            );
            self.far_strange.glitch.zone = Some(self.fields.insert(zone));
        }
    }

    /// One tick of The Glitch's fight (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the fight's beats in order")]
    pub(in crate::world) fn advance_glitch(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.far_strange.glitch.beaten {
            // The gate stays open for the whole crew: a rig that joins later is fitted too.
            for player in self.players.values_mut() {
                if !player.hazard_gear.has_system(RigSystem::PhaseDrill) {
                    crate::economy::grant_rig_system(player, RigSystem::PhaseDrill);
                }
            }
            return;
        }
        let Some(arena) = self.glitch_arena() else {
            return;
        };
        let inside_rect = arena_inside(&arena);
        let frame = self.frame();
        let inside: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .filter(|(_, player)| inside_rect.contains(player.tile_in(frame)))
            .map(|(id, _)| *id)
            .collect();
        if !inside.is_empty() {
            self.far_strange.glitch.last_visited = now;
        }
        let centre = inside_rect.center();
        if self.far_strange.glitch.entity.is_some() && self.glitch_parts().is_none() {
            self.put_glitch_to_sleep();
        }
        if !self.far_strange.glitch.awake() {
            let Some(player) = inside.first().copied() else {
                return;
            };
            // A peaceful world has no creatures: the hall yields its gate to the first rig.
            if !self.enemies.difficulty.hostile() {
                self.glitch_fell(centre, Some(player), output);
                return;
            }
            let tier = self.tier_at(centre);
            let (x, y) = frame.tile_center(TilePosition {
                x: centre.x,
                y: centre.y - 2,
            });
            let id = self.spawn_enemy_with(EnemyKind::TheGlitch, (x, y), tier, None, None);
            let glitch = &mut self.far_strange.glitch;
            glitch.entity = Some(id.get());
            glitch.rewriting = None;
            glitch.blink_to = None;
            glitch.shards.clear();
            self.write_glitch_rule(GlitchRule::Baseline, &arena);
            self.far_strange.glitch.next_blink = self.after_millis(now, GLITCH_BLINK_MILLIS);
            self.far_strange.glitch.next_shards = self.after_millis(now, GLITCH_SHARD_MILLIS);
            output.push_event(super::super::far_strange::far_event(
                FarStrangeEvent::GlitchAwoke,
                Some(player),
                centre,
            ));
            return;
        }
        let idle = now
            .get()
            .saturating_sub(self.far_strange.glitch.last_visited.get());
        if inside.is_empty() && idle > self.timing.ticks_for_millis(GLITCH_SLEEP_MILLIS) {
            self.put_glitch_to_sleep();
            return;
        }
        let Some((id, mut body, mut state)) = self.glitch_parts() else {
            return;
        };
        // It never leaves its hall.
        let (min_x, min_y) = frame.tile_center(inside_rect.min);
        let (max_x, max_y) = frame.tile_center(inside_rect.max);
        let clamped = (
            body.x.clamp(min_x + TILE_SIZE, max_x - TILE_SIZE),
            body.y.clamp(min_y + TILE_SIZE, max_y - TILE_SIZE),
        );
        let mut moved = clamped != (body.x, body.y);
        if moved {
            body.x = clamped.0;
            body.y = clamped.1;
            body.velocity_x = 0.0;
            body.velocity_y = 0.0;
        }
        // Rewrites: at each health threshold, the next rule (telegraphed, then written).
        let rule = self.far_strange.glitch.rule;
        if let Some((next, until)) = self.far_strange.glitch.rewriting {
            if now >= until {
                self.far_strange.glitch.rewriting = None;
                self.write_glitch_rule(next, &arena);
                state.mode = EnemyMode::Chase;
                state.attack_ready = self.after_millis(now, 900);
                moved = true;
                output.push_event(super::super::far_strange::far_event(
                    FarStrangeEvent::GlitchRewrote,
                    None,
                    centre,
                ));
            } else {
                // Mid-rewrite it flickers in place: no attacks, no moves.
                state.mode = EnemyMode::Stunned;
                state.mode_until = until;
                body.velocity_x = 0.0;
                body.velocity_y = 0.0;
                moved = true;
            }
        } else if let Some(next) = next_rule(rule) {
            let threshold =
                GLITCH_THRESHOLDS[usize::from(rule.index()).min(GLITCH_THRESHOLDS.len() - 1)];
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a health share in per mille"
            )]
            let health = (state.health / state.max_health.max(1.0) * 1_000.0) as u16;
            if health < threshold {
                let until = self.after_millis(now, GLITCH_REWRITE_MILLIS);
                self.far_strange.glitch.rewriting = Some((next, until));
                self.far_strange.glitch.blink_to = None;
                state.mode = EnemyMode::Stunned;
                state.mode_until = until;
                moved = true;
                output.push_event(super::super::far_strange::far_event(
                    FarStrangeEvent::GlitchRewriting,
                    None,
                    centre,
                ));
            }
        }
        // Blinks: its ghost shows where for a moment, then it is there.
        if !self.far_strange.glitch.rewriting() {
            match self.far_strange.glitch.blink_to {
                None if now >= self.far_strange.glitch.next_blink => {
                    let here = frame.tile_at(body.x, body.y);
                    let spots = [
                        (inside_rect.min.x + 6, inside_rect.min.y + 4),
                        (inside_rect.max.x - 6, inside_rect.min.y + 4),
                        (inside_rect.min.x + 8, inside_rect.max.y - 5),
                        (inside_rect.max.x - 8, inside_rect.max.y - 5),
                        (centre.x, centre.y - 3),
                    ];
                    let roll = hash2(
                        self.terrain.seed(),
                        0x611C_0B11,
                        i32::try_from(now.get() & 0x7FFF_FFFF).unwrap_or(0),
                        0,
                    );
                    let start = usize::try_from(roll % 5).unwrap_or(0);
                    let pick = (0..spots.len())
                        .map(|offset| spots[(start + offset) % spots.len()])
                        .find(|(x, y)| (x - here.x).abs() + (y - here.y).abs() > 8)
                        .unwrap_or(spots[start]);
                    self.far_strange.glitch.blink_to = Some(pick);
                    self.far_strange.glitch.next_blink =
                        self.after_millis(now, GLITCH_BLINK_TELL_MILLIS);
                }
                Some((tx, ty)) if now >= self.far_strange.glitch.next_blink => {
                    let (x, y) = frame.tile_center(TilePosition { x: tx, y: ty });
                    body.x = x;
                    body.y = y;
                    body.previous_x = x;
                    body.previous_y = y;
                    body.velocity_x = 0.0;
                    body.velocity_y = 0.0;
                    moved = true;
                    self.far_strange.glitch.blink_to = None;
                    // Quicker blinks as the rules pile up.
                    let rules = u64::from(self.far_strange.glitch.rule.index());
                    let wait = GLITCH_BLINK_MILLIS - GLITCH_BLINK_MILLIS * rules / 8;
                    self.far_strange.glitch.next_blink = self.after_millis(now, wait);
                    output.push_event(super::super::far_strange::far_event(
                        FarStrangeEvent::GlitchBlinked,
                        None,
                        TilePosition { x: tx, y: ty },
                    ));
                }
                _ => {}
            }
        }
        if moved {
            self.entities.update(id, |entity| {
                entity.body = body;
                entity.state = EntityState::Enemy(state);
            });
        }
        // Shards, from its third rule on.
        let entities = &self.entities;
        self.far_strange
            .glitch
            .shards
            .retain(|shard| entities.get(EntityId(*shard)).is_some());
        if self.far_strange.glitch.rule.index() >= GlitchRule::Mirrored.index()
            && !self.far_strange.glitch.rewriting()
            && now >= self.far_strange.glitch.next_shards
        {
            self.far_strange.glitch.next_shards = self.after_millis(now, GLITCH_SHARD_MILLIS);
            let tier = self.tier_at(centre);
            for side in [-1.0_f32, 1.0] {
                if self.far_strange.glitch.shards.len() >= MAX_GLITCH_SHARDS {
                    break;
                }
                let shard = self.spawn_enemy_with(
                    EnemyKind::GlitchShard,
                    (side.mul_add(TILE_SIZE * 3.0, body.x), body.y),
                    tier,
                    None,
                    Some(id),
                );
                self.far_strange.glitch.shards.push(shard.get());
            }
        }
    }

    /// Stages The Glitch's fight on `rule` (scenes, tests): it must be awake (a rig in its hall
    /// for a tick); `rewriting` starts the telegraph of the rule after it.
    pub fn stage_glitch_rule(&mut self, rule: GlitchRule, rewriting: bool) {
        let Some(arena) = self.glitch_arena() else {
            return;
        };
        self.write_glitch_rule(rule, &arena);
        let now = self.simulation_tick;
        self.far_strange.glitch.rewriting = if rewriting {
            next_rule(rule).map(|next| (next, self.after_millis(now, GLITCH_REWRITE_MILLIS)))
        } else {
            None
        };
        if let Some((id, _, mut state)) = self.glitch_parts() {
            // Its health where that rule begins.
            let threshold = match rule.index() {
                0 => 1_000,
                index => GLITCH_THRESHOLDS[usize::from(index - 1).min(GLITCH_THRESHOLDS.len() - 1)],
            };
            state.health = (state.max_health * f32::from(threshold) / 1_000.0 - 1.0).max(1.0);
            if rewriting {
                state.mode = EnemyMode::Stunned;
                state.mode_until = self.after_millis(now, GLITCH_REWRITE_MILLIS);
            }
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
            });
        }
    }

    /// The Glitch sleeps: its body and shards go, the arena's rule is unwritten, it heals.
    fn put_glitch_to_sleep(&mut self) {
        let glitch = &mut self.far_strange.glitch;
        let entity = glitch.entity.take();
        let shards = std::mem::take(&mut glitch.shards);
        glitch.rewriting = None;
        glitch.blink_to = None;
        if let Some(entity) = entity {
            self.entities.despawn(EntityId(entity));
        }
        for shard in shards {
            self.entities.despawn(EntityId(shard));
        }
        if let Some(zone) = self.far_strange.glitch.zone.take() {
            let _ = self.fields.remove(zone);
        }
        self.far_strange.glitch.rule = GlitchRule::Baseline;
    }

    /// The Glitch fell: the arena's rules come back and every rig of the crew gets the Phase
    /// Drill.
    pub(in crate::world) fn glitch_fell(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        self.put_glitch_to_sleep();
        self.far_strange.glitch.beaten = true;
        output.push_event(super::super::far_strange::far_event(
            FarStrangeEvent::GlitchDefeated,
            by,
            tile,
        ));
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in players {
            if let Some(player) = self.players.get_mut(&player_id) {
                crate::economy::grant_rig_system(player, RigSystem::PhaseDrill);
            }
            output.push_event(super::super::far_strange::far_event(
                FarStrangeEvent::GateRewards,
                Some(player_id),
                tile,
            ));
        }
    }
}
