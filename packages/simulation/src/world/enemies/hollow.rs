//! The Hollow World's creatures (Wave 9, Descent V): what sets them apart from the generic
//! state machine in `world/enemies.rs`, and the Sun Serpent's fight.
//!
//! * **Wind-gliders** (sky mantas, gust bladders) ride the Inner Sky's wind bands instead of
//!   wandering home; mantas only fight back when struck, bladders burst into a gust that
//!   shoves rigs away ([`HollowEvent::GustBurst`]).
//! * **Stoop rocs** see only what passes below them and roost at night; their attack is a
//!   steep dive (a long, fast lunge).
//! * **Crestbacks** graze by day, sleep at night and flee fangrunners; a ridden one is carried
//!   by its pilot (`world/hollow.rs`). **Skynecks** wander slowly and stamp only if hurt.
//! * **Fangrunner packs** circle their prey and leap together: every pack in the world shares
//!   one rhythm of world time ([`PACK_CIRCLE_MILLIS`]), so the leap comes all at once. They see
//!   farther at night.
//! * **Hangclaws** cling to isles' undersides and drop on rigs below, then climb back and
//!   hang still again.
//! * **Lode mites** cling to a rig ([`CLINGING_PHASE`]), dragging it and nibbling its hull,
//!   until the pilot spins the rig hard enough to throw them off ([`SHAKE_SPIN`]).
//! * **Cinder drakes** run on sunlight: quick and fiery in it, sluggish (and quiet) in shade.
//!   **Shade stalkers** hunt only in shade and never leave it. **Flareflies** burst into heat
//!   on a rig (exposure as well as hull).
//! * **The Sun Serpent** ([`WorldState::advance_serpent`]): wakes when a rig enters the Sun
//!   Well and orbits the inner sun, its body following its head's path (every point a pure
//!   function of world time and the fight's replicated state, so hosts and clients agree).
//!   Its four glowing sun scales take hits (the other coils are armour); break all four and it
//!   falls from its orbit, its head bare for [`SERPENT_EXPOSED_MILLIS`], then the scales blaze
//!   back and it orbits faster. It lunges at a marked point after a tell, and the inner sun
//!   flares after a warning: rigs in its line of sight burn, the pillars' shade is safe. No
//!   countdown: with no rig in the well it sinks back and heals. Its death fits every rig with
//!   the Gravity Anchor.

use crate::{
    depth_progression::RigSystem,
    enemy::{DamageOutcome, EnemyKind, EnemyMode, EnemyState, tier_scale},
    entity::{Body, EntityId, EntityState},
    event::TickOutput,
    hollow::{
        EXPOSURE_SCORCH_AT, FLARE_BURN_MILLIS, FLARE_DAMAGE_PER_SECOND, FLARE_EVERY_MILLIS,
        FLARE_WARNING_MILLIS, HollowEvent, SERPENT_BREATH_MILLIS, SERPENT_COILS,
        SERPENT_DASH_MILLIS, SERPENT_EXPOSED_MILLIS, SERPENT_LUNGE_MILLIS, SERPENT_ORBIT_SPEED,
        SERPENT_ORBIT_TILES, SERPENT_SCALES, SERPENT_SLEEP_MILLIS, SERPENT_TELL_MILLIS,
        SerpentPhase, SerpentState, WELL_RADIUS,
    },
    ids::{PlayerId, SimulationTick},
    physics,
    terrain::{TILE_SIZE, TilePosition},
};

use super::{Intent, RIG_CONTACT_HALF, WorldState, direction};

/// A creature touching a rig: its id, kind, position and the rig's player.
type Touch = (EntityId, EnemyKind, (f32, f32), Option<PlayerId>);
/// A clinging mite: its id, its rig, its offset on the rig, when it lets go and its tier.
type Rider = (EntityId, PlayerId, (i32, i32), SimulationTick, u8);
use crate::world::hollow::hollow_event;

/// `EnemyState::phase` of a lode mite clinging to a rig.
pub const CLINGING_PHASE: u8 = 0xC1;
/// `EnemyState::phase` of a serpent coil carrying a whole sun scale.
pub const SCALE_PHASE: u8 = 0xB1;
/// ... and of a cracked one.
pub const CRACKED_PHASE: u8 = 0xB2;
/// A fangrunner pack circles this long, then leaps for as long (a world-time rhythm).
pub const PACK_CIRCLE_MILLIS: u64 = 2_600;
/// A rig spinning faster than this (rad/s) throws its lode mites off.
pub const SHAKE_SPIN: f32 = 2.6;
/// The most lode mites a rig carries at once.
pub const MAX_CLINGING: usize = 5;
/// How long a mite clings at most before it drops off on its own.
pub const CLING_MILLIS: u64 = 14_000;
/// Hull a second each clinging mite nibbles (scaled by tier).
pub const MITE_NIBBLE_PER_SECOND: f32 = 0.7;
/// How much of a rig's speed each clinging mite takes a second.
pub const MITE_DRAG_PER_SECOND: f32 = 0.45;
/// A bursting gust bladder's shove (px/s) and its reach (tiles).
pub const GUST_PUSH: f32 = 330.0;
pub const GUST_REACH_TILES: f32 = 4.5;
/// A bursting flarefly's heat (exposure, per mille) and hull (scaled by tier).
pub const FLAREFLY_HEAT: u16 = 320;
pub const FLAREFLY_DAMAGE: f32 = 3.0;
/// Milliseconds a coil trails the one ahead of it along the serpent's path.
pub const COIL_LAG_MILLIS: u64 = 120;
/// How long the serpent takes to fall from its orbit (and rise back).
const SERPENT_FALL_MILLIS: u64 = 1_500;

impl WorldState {
    /// The Hollow World's say on who `state`'s creature notices: `Some(answer)` overrides the
    /// generic perception, `None` leaves it.
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn hollow_notice(
        &self,
        body: &Body,
        state: &EnemyState,
        generic: impl FnOnce() -> Option<PlayerId>,
    ) -> Option<Option<PlayerId>> {
        if !state.kind.is_hollow() {
            return None;
        }
        let night = !self.inner_sun_now().phase.sun_up();
        let frame = self.frame();
        let below = |player: PlayerId| {
            self.players
                .get(&player)
                .is_some_and(|rig| rig.y > TILE_SIZE.mul_add(2.0, body.y))
        };
        Some(match state.kind {
            // The peaceful only ever fight back (a hit sets their target directly).
            EnemyKind::SkyManta
            | EnemyKind::Skyneck
            | EnemyKind::Crestback
            | EnemyKind::GustBladder
            | EnemyKind::SunSerpent
            | EnemyKind::SerpentCoil => None,
            EnemyKind::StoopRoc => {
                if night {
                    None
                } else {
                    generic().filter(|player| below(*player))
                }
            }
            EnemyKind::Hangclaw => generic().filter(|player| below(*player)),
            EnemyKind::Fangrunner if night => {
                // Night hunters: farther sight, still needing a clear line.
                let reach = state.kind.stats().sight_tiles * TILE_SIZE * 1.6;
                self.players
                    .iter()
                    .filter(|(_, rig)| rig.hull > 0.0)
                    .map(|(id, rig)| ((rig.x - body.x).hypot(rig.y - body.y), *id, rig.x, rig.y))
                    .filter(|(distance, ..)| *distance <= reach)
                    .filter(|(_, _, x, y)| {
                        physics::line_clear(&self.terrain, body.x, body.y, *x, *y)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                    .map(|(_, id, ..)| id)
            }
            EnemyKind::ShadeStalker => generic().filter(|player| {
                self.players
                    .get(player)
                    .is_some_and(|rig| !self.sunlit_at(rig.tile_in(frame)))
            }),
            _ => generic(),
        })
    }

    /// Whether `state`'s creature is at peace with the crew now (auto-aim spares it): the
    /// grazing crestbacks and skynecks, sky mantas, until struck.
    pub(in crate::world) const fn hollow_peaceful(state: &EnemyState) -> bool {
        matches!(
            state.kind,
            EnemyKind::Crestback | EnemyKind::Skyneck | EnemyKind::SkyManta
        ) && state.last_attacker.is_none()
    }

    /// Whether the Hollow World lets `kind` roam where the director wants it (`None`: the
    /// generic habitat rules decide).
    pub(in crate::world) fn hollow_habitat(
        &self,
        kind: EnemyKind,
        tile: TilePosition,
    ) -> Option<bool> {
        if !kind.is_hollow() {
            return None;
        }
        // Nothing roams into the Sun Well: the fight is the serpent's.
        if self.sun_well().is_some_and(|centre| {
            crate::worldgen::descent::hollow::sun_well_rect(centre)
                .expanded(4)
                .contains(tile)
        }) {
            return Some(false);
        }
        let air = |at: TilePosition| !self.terrain.is_solid_at(at);
        match kind {
            // The serpent lives only in its well.
            EnemyKind::SunSerpent | EnemyKind::SerpentCoil => Some(false),
            // Under an isle: hanging from the rock overhead.
            EnemyKind::Hangclaw => Some(
                air(tile)
                    && self.terrain.is_solid_at(TilePosition {
                        x: tile.x,
                        y: tile.y - 1,
                    }),
            ),
            // In shade only.
            EnemyKind::ShadeStalker => Some(air(tile) && !self.sunlit_at(tile)),
            _ => None,
        }
    }

    /// The Hollow World's movement for `state`'s creature this tick, or `None` for the generic
    /// behaviour (see the module docs).
    #[allow(
        clippy::too_many_lines,
        clippy::cast_precision_loss,
        reason = "one arm per creature; small rhythm counters"
    )]
    pub(super) fn hollow_behave(
        &self,
        body: &Body,
        state: &mut EnemyState,
        now: SimulationTick,
    ) -> Option<Intent> {
        let spec = state.kind.stats();
        let frame = self.frame();
        let (home_x, home_y) = frame.centre_of(state.home.0, state.home.1);
        let millis = self.millis_at(now);
        let night = !self.inner_sun_now().phase.sun_up();
        match state.kind {
            EnemyKind::SkyManta | EnemyKind::GustBladder
                if matches!(state.mode, EnemyMode::Idle | EnemyMode::Return) =>
            {
                // Ride the wind, bobbing on a slow swell of its own.
                let wind = self.hollow_wind_at(body.x, body.y).unwrap_or(0.0);
                let phase = (millis + u64::from(state.seed) * 37) % 6_000;
                #[allow(clippy::cast_precision_loss, reason = "a few thousand milliseconds")]
                let swell = if phase < 3_000 {
                    phase as f32 / 3_000.0
                } else {
                    (6_000 - phase) as f32 / 3_000.0
                };
                let bob = (swell - 0.5) * TILE_SIZE * 3.0;
                // Never far from its band: it drifts back toward its home row.
                let drift_y = (home_y - body.y).clamp(-TILE_SIZE * 2.0, TILE_SIZE * 2.0);
                state.mode = EnemyMode::Idle;
                Some(Intent {
                    goal: Some((
                        f32::from(state.facing).mul_add(TILE_SIZE, wind.mul_add(1.5, body.x)),
                        body.y + drift_y * 0.5 + bob,
                    )),
                    speed: spec.speed.max(wind.abs()),
                })
            }
            EnemyKind::StoopRoc if night && state.mode == EnemyMode::Idle => Some(Intent {
                goal: Some((home_x, home_y)),
                speed: spec.speed * 0.5,
            }),
            EnemyKind::Crestback => {
                // Fangrunners about: run from them.
                let threat = self
                    .enemies()
                    .filter(|(_, other)| other.kind == EnemyKind::Fangrunner)
                    .map(|(entity, _)| (entity.body.x, entity.body.y))
                    .find(|(x, y)| (x - body.x).hypot(y - body.y) < TILE_SIZE * 7.0);
                if let Some((tx, ty)) = threat {
                    if state.mode == EnemyMode::Dormant {
                        state.mode = EnemyMode::Idle;
                    }
                    let ((nx, _), _) = direction((tx, ty), (body.x, body.y));
                    let away = if nx.abs() < 0.1 { 1.0 } else { nx.signum() };
                    return Some(Intent {
                        goal: Some((away.mul_add(TILE_SIZE * 6.0, body.x), body.y)),
                        speed: spec.speed * 1.8,
                    });
                }
                match (night, state.mode) {
                    (true, EnemyMode::Idle) => {
                        state.mode = EnemyMode::Dormant;
                        Some(Intent::default())
                    }
                    (false, EnemyMode::Dormant) => {
                        state.mode = EnemyMode::Idle;
                        None
                    }
                    _ => None,
                }
            }
            EnemyKind::Fangrunner if state.mode == EnemyMode::Chase => {
                // The pack circles until the shared rhythm turns, then leaps together.
                let leaping = (millis / PACK_CIRCLE_MILLIS).is_multiple_of(2);
                if leaping {
                    return None;
                }
                let (tx, ty) = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, rig.y))?;
                let side: f32 = if (state.seed & 1) == 0 { 1.0 } else { -1.0 };
                let swing = ((millis / 700 + u64::from(state.seed)) % 3) as f32 - 1.0;
                Some(Intent {
                    goal: Some((side.mul_add(TILE_SIZE * 4.5, tx) + swing * TILE_SIZE, ty)),
                    speed: spec.speed,
                })
            }
            EnemyKind::Hangclaw if state.mode == EnemyMode::Return => {
                if (body.x - home_x).hypot(body.y - home_y) < TILE_SIZE * 0.6 {
                    state.mode = EnemyMode::Dormant;
                    return Some(Intent::default());
                }
                None
            }
            EnemyKind::CinderDrake => {
                let tile = frame.tile_at(body.x, body.y);
                if self.sunlit_at(tile) {
                    return None;
                }
                // In shade: sluggish, and no fire breath.
                let goal = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .map(|rig| (rig.x, body.y));
                if state.mode == EnemyMode::Windup {
                    state.mode = EnemyMode::Chase;
                }
                state.attack_ready = state.attack_ready.max(now.next());
                Some(Intent {
                    goal: goal.or(Some((home_x, home_y))),
                    speed: spec.speed * 0.3,
                })
            }
            EnemyKind::ShadeStalker => {
                let here = frame.tile_at(body.x, body.y);
                let target_lit = state
                    .target
                    .and_then(|player| self.players.get(&player))
                    .is_some_and(|rig| self.sunlit_at(rig.tile_in(frame)));
                if self.sunlit_at(here) || (state.mode == EnemyMode::Chase && target_lit) {
                    // Back to the shadows it came from.
                    state.mode = EnemyMode::Return;
                    state.target = None;
                    return Some(Intent {
                        goal: Some((home_x, home_y)),
                        speed: spec.speed * 1.3,
                    });
                }
                None
            }
            _ => None,
        }
    }

    /// Whether the Hollow World lets `kind` spawn roaming (the serpent never roams).
    pub(in crate::world) const fn hollow_spawn_allowed(kind: EnemyKind) -> bool {
        !matches!(kind, EnemyKind::SunSerpent | EnemyKind::SerpentCoil)
    }

    /// The Hollow World's say on a hit to enemy `id` (after resistances): the serpent's armour
    /// and scales. `Some(outcome)` decides the hit, `None` leaves it to the generic damage.
    pub(in crate::world) fn hollow_damage(
        &mut self,
        id: EntityId,
        state: &EnemyState,
        amount: f32,
        output: &mut TickOutput,
    ) -> Option<DamageOutcome> {
        match state.kind {
            EnemyKind::SunSerpent if self.hollow.serpent.shielded() => Some(DamageOutcome::Ignored),
            EnemyKind::SerpentCoil => {
                if state.phase != SCALE_PHASE {
                    return Some(DamageOutcome::Ignored);
                }
                if amount < state.health {
                    return None;
                }
                // The scale cracks: the coil is armour again.
                let index = self
                    .hollow
                    .serpent
                    .coils
                    .iter()
                    .position(|coil| *coil == id.get())?;
                if let Some(scale) = SERPENT_SCALES.iter().position(|coil| *coil == index) {
                    self.hollow.serpent.scales[scale] = false;
                }
                let mut at = None;
                self.entities.update(id, |entity| {
                    if let EntityState::Enemy(coil) = &mut entity.state {
                        coil.phase = CRACKED_PHASE;
                        coil.health = coil.max_health;
                        at = Some((entity.body.x, entity.body.y));
                    }
                });
                if let Some((x, y)) = at {
                    output.push_event(hollow_event(
                        HollowEvent::ScaleBroken,
                        None,
                        self.frame().tile_at(x, y),
                    ));
                }
                Some(DamageOutcome::Hurt { dealt: amount })
            }
            _ => None,
        }
    }

    /// A Hollow World creature died: a bladder bursts, the serpent falls.
    pub(in crate::world) fn hollow_kill(
        &mut self,
        kind: EnemyKind,
        body: &Body,
        last_attacker: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        match kind {
            EnemyKind::GustBladder => self.burst_gust(body.x, body.y, output),
            EnemyKind::SunSerpent => self.serpent_fell(last_attacker, output),
            _ => {}
        }
    }

    /// A gust bladder bursts at `(x, y)`: rigs and pilots near it are shoved away.
    fn burst_gust(&mut self, x: f32, y: f32, output: &mut TickOutput) {
        let reach = GUST_REACH_TILES * TILE_SIZE;
        for rig in self.players.values_mut() {
            let (dx, dy) = (rig.x - x, rig.y - y);
            let distance = dx.hypot(dy);
            if distance > reach || distance < 0.01 {
                continue;
            }
            let push = GUST_PUSH * (distance / reach).mul_add(-0.5, 1.0);
            rig.velocity_x = crate::quantize::quantize_simulation_scalar(
                (dx / distance).mul_add(push, rig.velocity_x),
            );
            rig.velocity_y = crate::quantize::quantize_simulation_scalar(
                (dy / distance).mul_add(push, rig.velocity_y),
            );
        }
        output.push_event(hollow_event(
            HollowEvent::GustBurst,
            None,
            self.frame().tile_at(x, y),
        ));
    }

    /// The Hollow creatures' tick (see the module docs): bladders burst on contact, mites
    /// cling and are shaken off, flareflies burst into heat.
    #[allow(
        clippy::too_many_lines,
        reason = "the creatures' contact rules in order"
    )]
    pub(in crate::world) fn advance_hollow_creatures(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let dt = self.timing.delta_seconds;
        let frame = self.frame();
        // Bladders and flareflies touching a rig.
        let touched: Vec<Touch> = self
            .enemies()
            .filter(|(_, state)| matches!(state.kind, EnemyKind::GustBladder | EnemyKind::Flarefly))
            .filter_map(|(entity, state)| {
                let half = state.kind.contact_half();
                self.players
                    .iter()
                    .find(|(_, rig)| {
                        rig.hull > 0.0
                            && Self::touching(&entity.body, half, (rig.x, rig.y), RIG_CONTACT_HALF)
                    })
                    .map(|(player, _)| {
                        (
                            entity.id,
                            state.kind,
                            (entity.body.x, entity.body.y),
                            Some(*player),
                        )
                    })
            })
            .collect();
        for (id, kind, (x, y), player) in touched {
            if self.damage_immune() {
                break;
            }
            self.entities.despawn(id);
            if kind == EnemyKind::GustBladder {
                self.burst_gust(x, y, output);
            } else {
                {
                    let tile = frame.tile_at(x, y);
                    if let Some(player) = player {
                        let heat = self
                            .hollow
                            .exposure_of(player)
                            .saturating_add(FLAREFLY_HEAT);
                        self.hollow
                            .exposure
                            .insert(player, heat.min(EXPOSURE_SCORCH_AT + 200));
                        let damage = FLAREFLY_DAMAGE * tier_scale(self.tier_at(tile));
                        let _ = self.damage_player(player, damage);
                        if let Some(rig) = self.players.get_mut(&player) {
                            rig.hull = crate::quantize::quantize_simulation_scalar(rig.hull);
                        }
                    }
                    output.push_event(hollow_event(HollowEvent::FlareflyBurst, player, tile));
                }
            }
        }
        // Lode mites: chasing mites touching a rig cling to it.
        let mut clinging: std::collections::BTreeMap<PlayerId, usize> =
            std::collections::BTreeMap::new();
        for (_, state) in self.enemies() {
            if state.kind == EnemyKind::LodeMite
                && state.phase == CLINGING_PHASE
                && let Some(player) = state.target
            {
                *clinging.entry(player).or_insert(0) += 1;
            }
        }
        let latching: Vec<(EntityId, PlayerId, (i32, i32))> = self
            .enemies()
            .filter(|(_, state)| {
                state.kind == EnemyKind::LodeMite
                    && state.phase != CLINGING_PHASE
                    && state.mode == EnemyMode::Chase
            })
            .filter_map(|(entity, state)| {
                let half = state.kind.contact_half();
                self.players
                    .iter()
                    .find(|(_, rig)| {
                        Self::touching(&entity.body, half, (rig.x, rig.y), RIG_CONTACT_HALF)
                    })
                    .map(|(player, rig)| {
                        #[allow(clippy::cast_possible_truncation, reason = "a few pixels")]
                        let offset = (
                            (entity.body.x - rig.x).clamp(-14.0, 14.0) as i32,
                            (entity.body.y - rig.y).clamp(-12.0, 12.0) as i32,
                        );
                        (entity.id, *player, offset)
                    })
            })
            .collect();
        let until = self.after_millis(now, CLING_MILLIS);
        for (id, player, offset) in latching {
            let count = clinging.entry(player).or_insert(0);
            if *count >= MAX_CLINGING {
                continue;
            }
            *count += 1;
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.phase = CLINGING_PHASE;
                    state.target = Some(player);
                    state.home = offset;
                    state.mode_until = until;
                }
            });
            let tile = self
                .players
                .get(&player)
                .map_or(TilePosition { x: 0, y: 0 }, |rig| rig.tile_in(frame));
            output.push_event(hollow_event(HollowEvent::MiteLatched, Some(player), tile));
        }
        // Clinging mites ride their rig, drag it and nibble; a hard spin throws them off.
        let riders: Vec<Rider> = self
            .enemies()
            .filter(|(_, state)| state.kind == EnemyKind::LodeMite && state.phase == CLINGING_PHASE)
            .filter_map(|(entity, state)| {
                Some((
                    entity.id,
                    state.target?,
                    state.home,
                    state.mode_until,
                    state.tier,
                ))
            })
            .collect();
        for (id, player, offset, until, tier) in riders {
            let Some(rig) = self.players.get(&player) else {
                self.unclings(id, None);
                continue;
            };
            let (x, y, spin) = (rig.x, rig.y, rig.attitude.angular_velocity.abs());
            if spin > SHAKE_SPIN || now >= until || rig.hull <= 0.0 || self.is_on_foot(player) {
                self.unclings(id, Some((x, y)));
                continue;
            }
            #[allow(clippy::cast_precision_loss, reason = "a few pixels")]
            let (ox, oy) = (offset.0 as f32, offset.1 as f32);
            self.entities.update(id, |entity| {
                entity.body.previous_x = entity.body.x;
                entity.body.previous_y = entity.body.y;
                entity.body.x = x + ox;
                entity.body.y = y + oy;
                entity.body.velocity_x = 0.0;
                entity.body.velocity_y = 0.0;
            });
            let keep = crate::field::trig::exp_neg(MITE_DRAG_PER_SECOND * dt);
            if let Some(rig) = self.players.get_mut(&player) {
                rig.velocity_x = crate::quantize::quantize_simulation_scalar(rig.velocity_x * keep);
                rig.velocity_y = crate::quantize::quantize_simulation_scalar(rig.velocity_y * keep);
            }
            let _ = self.damage_player(player, MITE_NIBBLE_PER_SECOND * tier_scale(tier) * dt);
            if let Some(rig) = self.players.get_mut(&player) {
                rig.hull = crate::quantize::quantize_simulation_scalar(rig.hull);
            }
        }
    }

    /// A lode mite lets go (thrown off from `(x, y)` when given): it is stunned a moment and
    /// flung outward.
    fn unclings(&mut self, id: EntityId, from: Option<(f32, f32)>) {
        let frame = self.frame();
        let now = self.simulation_tick.next();
        let stun = self.after(now, 1_500);
        self.entities.update(id, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.phase = 0;
                state.mode = EnemyMode::Stunned;
                state.mode_until = stun;
                state.home = {
                    let tile = frame.tile_at(entity.body.x, entity.body.y);
                    (tile.x, tile.y)
                };
                if let Some((x, y)) = from {
                    let ((nx, ny), _) = direction((x, y), (entity.body.x, entity.body.y));
                    entity.body.velocity_x =
                        crate::quantize::quantize_simulation_scalar(nx * 260.0);
                    entity.body.velocity_y =
                        crate::quantize::quantize_simulation_scalar(ny * 260.0);
                }
            }
        });
    }

    // --- The Sun Serpent ------------------------------------------------------------------------

    /// The serpent's orbit speed (rad/s) after `cycles` broken sets of scales.
    fn serpent_speed(cycles: u8) -> f32 {
        SERPENT_ORBIT_SPEED * f32::from(cycles.min(3)).mul_add(0.18, 1.0)
    }

    /// Where on its path the serpent's head (or a coil `coil` behind it) is at world time
    /// `millis`, local pixels: a pure function of the fight's state.
    #[must_use]
    pub fn serpent_point(
        &self,
        state: &SerpentState,
        coil: usize,
        millis: u64,
    ) -> Option<(f32, f32)> {
        let centre = self.sun_well()?;
        let (cx, cy) = self.frame().tile_center(centre);
        let lag = COIL_LAG_MILLIS * u64::try_from(coil).unwrap_or(0);
        let t = millis.saturating_sub(lag);
        let orbit = |t: u64| -> (f32, f32) {
            #[allow(clippy::cast_precision_loss, reason = "world seconds of a fight")]
            let seconds = t.saturating_sub(state.orbit_millis) as f32 / 1_000.0;
            #[allow(clippy::cast_precision_loss, reason = "a fixed-point turn")]
            let start = state.angle as f32 / 65_536.0 * std::f32::consts::TAU;
            let angle = Self::serpent_speed(state.cycles).mul_add(seconds, start);
            let phase = t % SERPENT_BREATH_MILLIS;
            let half = SERPENT_BREATH_MILLIS / 2;
            #[allow(clippy::cast_precision_loss, reason = "a few thousand milliseconds")]
            let breath = if phase < half {
                phase as f32 / half as f32
            } else {
                (SERPENT_BREATH_MILLIS - phase) as f32 / half as f32
            };
            let (low, high) = SERPENT_ORBIT_TILES;
            let radius = (high - low).mul_add(breath, low) * TILE_SIZE;
            let (sin, cos) = crate::field::trig::sin_cos(crate::field::trig::wrap_angle(angle));
            (cos.mul_add(radius, cx), (sin * 0.82).mul_add(radius, cy))
        };
        let lerp = |a: (f32, f32), b: (f32, f32), f: f32| {
            (f.mul_add(b.0 - a.0, a.0), f.mul_add(b.1 - a.1, a.1))
        };
        // Fallen, it lies along an inner ring where it fell, the head first and the coils
        // trailing behind it round the ring (so a rig fighting near it is never far).
        let rest = |fell: u64| -> (f32, f32) {
            let (hx, hy) = orbit(fell);
            let angle = crate::field::trig::atan2(hy - cy, hx - cx);
            #[allow(clippy::cast_precision_loss, reason = "a coil index")]
            let trail = (coil as f32).mul_add(-0.11, angle);
            let (sin, cos) = crate::field::trig::sin_cos(crate::field::trig::wrap_angle(trail));
            let radius = 13.0 * TILE_SIZE;
            (cos.mul_add(radius, cx), (sin * 0.82).mul_add(radius, cy))
        };
        #[allow(clippy::cast_precision_loss, reason = "a fall of milliseconds")]
        let share = |from: u64, at: u64| -> f32 {
            (at.saturating_sub(from) as f32 / SERPENT_FALL_MILLIS as f32).clamp(0.0, 1.0)
        };
        // Fallen out of its orbit: down onto the well's floor, then back up.
        if let Some(fell) = state.exposed_millis
            && t >= fell
        {
            let rest = rest(fell);
            let down = lerp(orbit(fell), rest, share(fell, t));
            let rises = fell + SERPENT_EXPOSED_MILLIS;
            if t < rises {
                return Some(down);
            }
            return Some(lerp(rest, orbit(t), share(rises, t)));
        }
        let mut point = orbit(t);
        // A lunge: out to the mark and back.
        if let Some(lunged) = state.lunge_millis
            && t >= lunged
            && t < lunged + SERPENT_DASH_MILLIS
        {
            #[allow(clippy::cast_precision_loss, reason = "a dash of milliseconds")]
            let p = (t - lunged) as f32 / SERPENT_DASH_MILLIS as f32;
            let reach = 1.0 - 2.0f32.mul_add(p, -1.0).abs();
            #[allow(clippy::cast_precision_loss, reason = "local pixels")]
            let mark = (state.mark.0 as f32, state.mark.1 as f32);
            point = lerp(point, mark, reach);
        }
        Some(point)
    }

    /// The rigs (alive) inside the Sun Well and their positions.
    fn rigs_in_well(&self, centre: TilePosition) -> Vec<(PlayerId, f32, f32)> {
        let frame = self.frame();
        let (cx, cy) = frame.tile_center(centre);
        #[allow(clippy::cast_precision_loss, reason = "a radius in tiles")]
        let reach = (WELL_RADIUS as f32 + 0.5) * TILE_SIZE;
        self.players
            .iter()
            .filter(|(_, rig)| rig.hull > 0.0)
            .filter(|(_, rig)| (rig.x - cx).hypot(rig.y - cy) <= reach)
            .map(|(id, rig)| (*id, rig.x, rig.y))
            .collect()
    }

    /// Whether the inner sun in the well at `centre` sees local pixel `(x, y)` (no pillar or
    /// wall between).
    #[must_use]
    pub fn sun_sees(&self, centre: TilePosition, x: f32, y: f32) -> bool {
        let (cx, cy) = self.frame().tile_center(centre);
        let (dx, dy) = (x - cx, y - cy);
        let distance = dx.hypot(dy);
        // Start at the sun's rim.
        #[allow(clippy::cast_precision_loss, reason = "a small radius")]
        let rim = (crate::hollow::SUN_RADIUS as f32 + 1.0) * TILE_SIZE;
        if distance <= rim {
            return true;
        }
        let (sx, sy) = (
            (dx / distance).mul_add(rim, cx),
            (dy / distance).mul_add(rim, cy),
        );
        physics::line_clear(&self.terrain, sx, sy, x, y)
    }

    /// One tick of the Sun Serpent's fight (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the fight's beats in order")]
    pub(in crate::world) fn advance_serpent(
        &mut self,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.hollow.serpent.beaten {
            // The gate stays open for the whole crew: a rig that joins later is fitted too.
            for player in self.players.values_mut() {
                if !player.hazard_gear.has_system(RigSystem::GravityAnchor) {
                    crate::economy::grant_rig_system(player, RigSystem::GravityAnchor);
                }
            }
            return;
        }
        let Some(centre) = self.sun_well() else {
            return;
        };
        let inside = self.rigs_in_well(centre);
        let millis = self.millis_at(now);
        if !inside.is_empty() {
            self.hollow.serpent.last_visited = now;
        }
        // Its head gone some other way (despawned far from every rig): it sinks.
        if let Some(head) = self.hollow.serpent.head
            && self.entities.get(EntityId(head)).is_none()
        {
            self.put_serpent_to_sleep();
        }
        if !self.hollow.serpent.awake() {
            let Some((player, _, _)) = inside.first().copied() else {
                return;
            };
            // A peaceful world has no creatures: the well yields its gate to the first rig.
            if !self.enemies.difficulty.hostile() {
                self.serpent_fell(Some(player), output);
                return;
            }
            self.wake_serpent(centre, now, output, player);
            return;
        }
        let idle = now
            .get()
            .saturating_sub(self.hollow.serpent.last_visited.get());
        if inside.is_empty() && idle > self.timing.ticks_for_millis(SERPENT_SLEEP_MILLIS) {
            self.put_serpent_to_sleep();
            return;
        }
        let frame = self.frame();
        // The fight's beats.
        let serpent = &self.hollow.serpent;
        match serpent.phase {
            SerpentPhase::Orbit => {
                if !serpent.scales.iter().any(|whole| *whole) {
                    let until = self.after_millis(now, SERPENT_EXPOSED_MILLIS);
                    let serpent = &mut self.hollow.serpent;
                    serpent.phase = SerpentPhase::Exposed;
                    serpent.exposed_millis = Some(millis);
                    serpent.phase_until = until;
                    serpent.cycles = serpent.cycles.saturating_add(1);
                    output.push_event(hollow_event(HollowEvent::SerpentExposed, None, centre));
                } else if now >= serpent.next_lunge
                    && let Some((player, x, y)) = self.nearest_in(&inside, centre)
                {
                    let until = self.after_millis(now, SERPENT_TELL_MILLIS);
                    let serpent = &mut self.hollow.serpent;
                    serpent.phase = SerpentPhase::Tell;
                    #[allow(clippy::cast_possible_truncation, reason = "local pixels")]
                    {
                        serpent.mark = (x as i32, y as i32);
                    }
                    serpent.phase_until = until;
                    output.push_event(hollow_event(
                        HollowEvent::SerpentTell,
                        Some(player),
                        frame.tile_at(x, y),
                    ));
                }
            }
            SerpentPhase::Tell => {
                if now >= serpent.phase_until {
                    let until = self.after_millis(now, SERPENT_DASH_MILLIS);
                    let serpent = &mut self.hollow.serpent;
                    serpent.phase = SerpentPhase::Lunge;
                    serpent.lunge_millis = Some(millis);
                    serpent.phase_until = until;
                }
            }
            SerpentPhase::Lunge => {
                if now >= serpent.phase_until {
                    let wait = SERPENT_LUNGE_MILLIS
                        - SERPENT_LUNGE_MILLIS * u64::from(serpent.cycles.min(3)) / 8;
                    let next = self.after_millis(now, wait);
                    let serpent = &mut self.hollow.serpent;
                    serpent.phase = SerpentPhase::Orbit;
                    serpent.next_lunge = next;
                }
            }
            SerpentPhase::Exposed => {
                if now >= serpent.phase_until {
                    // The scales blaze back; it rises and orbits faster.
                    let rises = millis;
                    self.reignite_scales();
                    let next_lunge = self.after_millis(now, SERPENT_LUNGE_MILLIS);
                    let serpent = &mut self.hollow.serpent;
                    serpent.phase = SerpentPhase::Orbit;
                    serpent.next_lunge = next_lunge;
                    // Re-anchor the orbit where it rises (its path continues from there).
                    serpent.orbit_millis = rises;
                    output.push_event(hollow_event(HollowEvent::ScalesReignited, None, centre));
                }
            }
        }
        // The inner sun's flares: a warning, then the burn; shade behind a pillar is safe.
        let serpent = &self.hollow.serpent;
        match (serpent.flare_warned, serpent.flare_burns) {
            (None, None) if now >= serpent.next_flare && !inside.is_empty() => {
                self.hollow.serpent.flare_warned = Some(now);
                output.push_event(hollow_event(HollowEvent::FlareWarning, None, centre));
            }
            (Some(warned), None) => {
                let burns = self.after_millis(warned, FLARE_WARNING_MILLIS);
                if now >= burns {
                    self.hollow.serpent.flare_burns = Some(now);
                }
            }
            (_, Some(burned)) => {
                let ends = self.after_millis(burned, FLARE_BURN_MILLIS);
                if now >= ends {
                    let wait = FLARE_EVERY_MILLIS
                        - FLARE_EVERY_MILLIS * u64::from(self.hollow.serpent.cycles.min(3)) / 10;
                    let next = self.after_millis(now, wait);
                    let serpent = &mut self.hollow.serpent;
                    serpent.flare_warned = None;
                    serpent.flare_burns = None;
                    serpent.next_flare = next;
                } else {
                    let dt = self.timing.delta_seconds;
                    let scale = tier_scale(self.tier_at(centre));
                    let burned: Vec<PlayerId> = inside
                        .iter()
                        .filter(|(_, x, y)| self.sun_sees(centre, *x, *y))
                        .map(|(player, ..)| *player)
                        .collect();
                    for player in burned {
                        let _ = self.damage_player(player, FLARE_DAMAGE_PER_SECOND * scale * dt);
                        if let Some(rig) = self.players.get_mut(&player) {
                            rig.hull = crate::quantize::quantize_simulation_scalar(rig.hull);
                        }
                    }
                }
            }
            _ => {}
        }
        self.place_serpent(millis);
        self.serpent_strikes(now, output);
    }

    /// The nearest rig of `inside` to the well's centre's ring (the lunge's mark).
    fn nearest_in(
        &self,
        inside: &[(PlayerId, f32, f32)],
        centre: TilePosition,
    ) -> Option<(PlayerId, f32, f32)> {
        let head = self
            .hollow
            .serpent
            .head
            .and_then(|head| self.entities.get(EntityId(head)))
            .map_or_else(
                || self.frame().tile_center(centre),
                |entity| (entity.body.x, entity.body.y),
            );
        inside.iter().copied().min_by(|a, b| {
            (a.1 - head.0)
                .hypot(a.2 - head.1)
                .total_cmp(&(b.1 - head.0).hypot(b.2 - head.1))
                .then(a.0.cmp(&b.0))
        })
    }

    /// Wakes the serpent in the well at `centre`: its head and coils rise from the sun.
    fn wake_serpent(
        &mut self,
        centre: TilePosition,
        now: SimulationTick,
        output: &mut TickOutput,
        player: PlayerId,
    ) {
        let tier = self.tier_at(centre);
        let (x, y) = self.frame().tile_center(centre);
        let millis = self.millis_at(now);
        let head = self.spawn_enemy_with(EnemyKind::SunSerpent, (x, y), tier, None, None);
        let mut coils = Vec::with_capacity(SERPENT_COILS);
        for index in 0..SERPENT_COILS {
            let coil =
                self.spawn_enemy_with(EnemyKind::SerpentCoil, (x, y), tier, None, Some(head));
            if SERPENT_SCALES.contains(&index) {
                self.entities.update(coil, |entity| {
                    if let EntityState::Enemy(state) = &mut entity.state {
                        state.phase = SCALE_PHASE;
                    }
                });
            }
            coils.push(coil.get());
        }
        let next_lunge = self.after_millis(now, SERPENT_LUNGE_MILLIS / 2);
        let next_flare = self.after_millis(now, FLARE_EVERY_MILLIS / 2);
        let serpent = &mut self.hollow.serpent;
        serpent.head = Some(head.get());
        serpent.coils = coils;
        serpent.scales = [true; 4];
        serpent.phase = SerpentPhase::Orbit;
        serpent.cycles = 0;
        serpent.angle = 0;
        serpent.orbit_millis = millis;
        serpent.exposed_millis = None;
        serpent.lunge_millis = None;
        serpent.next_lunge = next_lunge;
        serpent.next_flare = next_flare;
        serpent.flare_warned = None;
        serpent.flare_burns = None;
        output.push_event(hollow_event(
            HollowEvent::SerpentAwakened,
            Some(player),
            centre,
        ));
    }

    /// The scales blaze back (after an exposure).
    fn reignite_scales(&mut self) {
        let coils = self.hollow.serpent.coils.clone();
        for scale in SERPENT_SCALES {
            if let Some(coil) = coils.get(scale) {
                self.entities.update(EntityId(*coil), |entity| {
                    if let EntityState::Enemy(state) = &mut entity.state {
                        state.phase = SCALE_PHASE;
                        state.health = state.max_health;
                    }
                });
            }
        }
        self.hollow.serpent.scales = [true; 4];
        self.hollow.serpent.exposed_millis = None;
    }

    /// Puts the head and coils where the path says they are at `millis`.
    fn place_serpent(&mut self, millis: u64) {
        let serpent = self.hollow.serpent.clone();
        let parts = serpent
            .head
            .into_iter()
            .chain(serpent.coils.iter().copied())
            .enumerate();
        for (index, id) in parts {
            // The head is part 0; coil `i` trails `i + 1` lags behind it.
            let Some((x, y)) = self.serpent_point(&serpent, index, millis) else {
                continue;
            };
            let ahead = self
                .serpent_point(&serpent, index, millis.saturating_sub(40))
                .unwrap_or((x, y));
            self.entities.update(EntityId(id), |entity| {
                entity.body.previous_x = entity.body.x;
                entity.body.previous_y = entity.body.y;
                entity.body.x = crate::quantize::quantize_simulation_scalar(x);
                entity.body.y = crate::quantize::quantize_simulation_scalar(y);
                entity.body.velocity_x =
                    crate::quantize::quantize_simulation_scalar((x - ahead.0) * 25.0);
                entity.body.velocity_y =
                    crate::quantize::quantize_simulation_scalar((y - ahead.1) * 25.0);
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.facing = if x >= ahead.0 { 1 } else { -1 };
                    state.mode = if serpent.phase == SerpentPhase::Tell && index == 0 {
                        EnemyMode::Windup
                    } else if serpent.phase == SerpentPhase::Exposed {
                        EnemyMode::Stunned
                    } else {
                        EnemyMode::Chase
                    };
                }
            });
        }
    }

    /// Rigs and pilots touching the serpent's head or coils are struck (the rig's i-frames
    /// space the hits).
    fn serpent_strikes(&mut self, now: SimulationTick, output: &mut TickOutput) {
        if self.hollow.serpent.phase == SerpentPhase::Exposed {
            return;
        }
        let parts: Vec<(Body, EnemyState)> = self
            .hollow
            .serpent
            .head
            .into_iter()
            .chain(self.hollow.serpent.coils.iter().copied())
            .filter_map(|id| self.enemy_parts(EntityId(id)))
            .collect();
        for (body, state) in parts {
            let spec = state.kind.stats();
            let half = state.kind.contact_half();
            let victims: Vec<PlayerId> = self
                .players
                .iter()
                .filter(|(_, rig)| Self::touching(&body, half, (rig.x, rig.y), RIG_CONTACT_HALF))
                .map(|(id, _)| *id)
                .collect();
            for player in victims {
                let damage = spec.contact_damage * tier_scale(state.tier);
                let _ = self.strike_rig(
                    player,
                    damage,
                    (body.x, body.y),
                    spec.knockback,
                    state.kind,
                    now,
                    output,
                );
            }
        }
    }

    /// The serpent sinks into the sun: its body goes, it heals.
    fn put_serpent_to_sleep(&mut self) {
        let serpent = std::mem::take(&mut self.hollow.serpent);
        if let Some(head) = serpent.head {
            self.entities.despawn(EntityId(head));
        }
        for coil in serpent.coils {
            self.entities.despawn(EntityId(coil));
        }
        // Only the fight goes; whether it was beaten stays (it was not, or we would not be
        // here).
        self.hollow.serpent.beaten = serpent.beaten;
    }

    /// The serpent fell: its body goes and every rig gets the Gravity Anchor.
    pub(in crate::world) fn serpent_fell(
        &mut self,
        player: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let centre = self.sun_well().unwrap_or(TilePosition { x: 0, y: 0 });
        let coils = std::mem::take(&mut self.hollow.serpent.coils);
        for coil in coils {
            self.entities.despawn(EntityId(coil));
        }
        if let Some(head) = self.hollow.serpent.head.take() {
            self.entities.despawn(EntityId(head));
        }
        self.hollow.serpent = SerpentState {
            beaten: true,
            ..SerpentState::default()
        };
        for rig in self.players.values_mut() {
            crate::economy::grant_rig_system(rig, RigSystem::GravityAnchor);
        }
        output.push_event(hollow_event(HollowEvent::SerpentDefeated, player, centre));
    }
}
