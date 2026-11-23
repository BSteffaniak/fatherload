//! The Hollow World in the world (Wave 9, Descent V; the rules live in [`crate::hollow`]).
//!
//! [`WorldState::advance_hollow`] runs once per tick after the Living Dark's pass. On worlds
//! without Descents it returns at once. In order:
//!
//! 1. **The inner sun**: dawn and night are announced when the phase turns ([`inner_sun_at`],
//!    a pure function of world time).
//! 2. **The breakthrough**: a rig (or pilot) in the Hollow World's open air for the first time
//!    gets the reveal ([`HollowEvent::Breakthrough`]).
//! 3. **Exposure** in the Sunforge: on world-time steps ([`EXPOSURE_STEP_MILLIS`]) each actor in
//!    the inner sun's light ([`WorldState::sunlit_at`]) heats and each in shade cools; past the
//!    scorch line the hull (or the pilot) burns.
//! 4. **Isle landings**: a rig touching down under an isle (its gravity up) lands feet first
//!    (cheered) or on its roof (it costs hull).
//! 5. **The Gravity Anchor** lets go when its time is up.
//! 6. **Riding**: each mounted crestback is carried under its pilot; a pilot back in the rig,
//!    gone far or the crestback gone ends the ride.
//! 7. The creatures' Hollow ways and the Sun Serpent (`world/enemies/hollow.rs`).
//!
//! Wind and tar act inside the rig's movement step (`world/movement.rs` asks
//! [`WorldState::hollow_wind_at`] and [`WorldState::tar_under`]), the anchor's gravity too
//! ([`WorldState::anchor_quarter_for`]).

use crate::{
    depth_progression::RigSystem,
    descent::{Descent, Stratum},
    enemy::{EnemyKind, EnemyMode},
    entity::{EntityId, EntityState},
    event::{GameplayEventKind, TickOutput},
    field::{FieldSample, ScreenEffect},
    hollow::{
        ANCHOR_COOLDOWN_MILLIS, ANCHOR_MILLIS, EXPOSURE_COOL_PER_SECOND,
        EXPOSURE_DAMAGE_PER_SECOND, EXPOSURE_GAIN_PER_SECOND, EXPOSURE_SCORCH_AT,
        EXPOSURE_STEP_MILLIS, Hollow, HollowAction, HollowEvent, HollowRefusal, HollowTile,
        InnerPhase, InnerSun, MOUNT_JUMP_PCT, MOUNT_MOVE_PCT, MOUNT_REACH, ROOF_LANDING_DAMAGE,
        ROOF_LANDING_DEGREES, SHADE_SCAN_ROWS, anchor_quarter, inner_sun_at, wind_at,
    },
    ids::{PlayerId, SimulationTick},
    pilot::PilotBoons,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::descent::hollow as places,
};

use super::WorldState;

/// A Hollow World event at `tile`.
pub(super) const fn hollow_event(
    what: HollowEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Hollow {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

/// Whether sunlight passes a tile of `kind` (shade comes only from solid rock).
const fn lets_light(kind: TileKind) -> bool {
    match kind {
        TileKind::Air | TileKind::Fluid(_) | TileKind::Lava | TileKind::Gas => true,
        TileKind::Hollow(tile) => tile.lets_light_through(),
        _ => false,
    }
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The Hollow World's shared state.
    #[must_use]
    pub const fn hollow(&self) -> &Hollow {
        &self.hollow
    }

    /// The inner sun now (a pure function of the world clock's time, which saves, replicates and
    /// moves with a skipped night like the surface day).
    #[must_use]
    pub const fn inner_sun_now(&self) -> InnerSun {
        inner_sun_at(self.clock().world_millis())
    }

    /// The Hollow World stratum at `tile`, `None` outside it (and on older worlds).
    #[must_use]
    pub fn hollow_stratum_at(&self, tile: TilePosition) -> Option<Stratum> {
        self.deep_stratum_at(tile)
            .filter(|stratum| stratum.descent() == Descent::HollowWorld)
    }

    /// The Sun Well's centre (the Descent V gate arena; Deep Descent worlds).
    #[must_use]
    pub fn sun_well(&self) -> Option<TilePosition> {
        places::sun_well(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// Whether `tile` is open air of the Hollow World's caverns (the breakthrough's test: the
    /// Inner Sky's great cavern).
    #[must_use]
    pub fn in_hollow_sky(&self, tile: TilePosition) -> bool {
        let Some(layout) = self.descent_strata_layout() else {
            return false;
        };
        let (top, bottom) = layout.rows(Stratum::InnerSky);
        tile.y >= top
            && tile.y <= bottom
            && crate::worldgen::descent::hollow_open(
                self.terrain.settings(),
                &layout,
                tile.x,
                tile.y,
            )
            && self
                .terrain
                .tile(tile)
                .is_none_or(|t| t.kind == TileKind::Air)
    }

    /// Whether `tile` is in the inner sun's light: nothing solid between it and the sun below
    /// for [`SHADE_SCAN_ROWS`] rows, along the light's slant (shadows lean with the inner
    /// day). Only the Sunforge's light counts (its stratum and the Sun Well).
    #[must_use]
    pub fn sunlit_at(&self, tile: TilePosition) -> bool {
        if self.hollow_stratum_at(tile) != Some(Stratum::TheSunforge) {
            return false;
        }
        let slant = i32::from(self.inner_sun_now().slant);
        (1..=SHADE_SCAN_ROWS).all(|k| {
            let at = TilePosition {
                x: tile.x + k * slant / 1_000,
                y: tile.y + k,
            };
            self.terrain.tile(at).is_none_or(|t| lets_light(t.kind))
        })
    }

    /// The Inner Sky's wind at local pixel `(x, y)` (px/s, positive right), `None` where no
    /// wind blows (outside the great cavern's open air, older worlds).
    #[must_use]
    pub fn hollow_wind_at(&self, x: f32, y: f32) -> Option<f32> {
        let layout = self.descent_strata_layout()?;
        let frame = self.frame();
        let tile = frame.tile_at(x, y);
        let (top, bottom) = layout.rows(Stratum::InnerSky);
        if tile.y < top || tile.y > bottom {
            return None;
        }
        let settings = self.terrain.settings();
        if !crate::worldgen::descent::hollow_open(settings, &layout, tile.x, tile.y) {
            return None;
        }
        Some(wind_at(
            settings.seed,
            tile.y,
            layout.along(tile.y),
            self.millis_at(self.simulation_tick),
        ))
    }

    /// Whether the rig at local pixel `(x, y)` rests on tar (it grips: `TAR_GRIP_PER_SECOND`).
    #[must_use]
    pub fn tar_under(&self, x: f32, y: f32) -> bool {
        let below = self.frame().tile_at(x, TILE_SIZE.mul_add(0.75, y));
        self.terrain
            .tile(below)
            .is_some_and(|t| t.kind == TileKind::Hollow(HollowTile::Tar))
    }

    /// The quarter `player_id`'s Gravity Anchor pulls toward while it holds.
    #[must_use]
    pub fn anchor_quarter_for(&self, player_id: PlayerId) -> Option<u8> {
        self.hollow.anchored(player_id, self.simulation_tick)
    }

    /// The crestback `player_id`'s pilot rides, if any.
    #[must_use]
    pub fn mount_of(&self, player_id: PlayerId) -> Option<EntityId> {
        self.hollow.mounts.get(&player_id).copied().map(EntityId)
    }

    /// The Hollow World's say on what a seat sees (laid over [`Self::view_fields_for`]'s
    /// fields): a rig's exposure blooms and brightens its view, and the Sun Serpent's flares
    /// glare.
    #[must_use]
    pub fn hollow_view(&self, player_id: PlayerId, mut fields: FieldSample) -> FieldSample {
        let exposure = f32::from(self.hollow.exposure_of(player_id)) / 1_000.0;
        let flare = if self.hollow.serpent.flaring() {
            0.35
        } else {
            0.0
        };
        let glare = (exposure * 0.45 + flare).min(0.8);
        if glare > 0.0 {
            let index = ScreenEffect::Exposure.index();
            fields.screen[index] = fields.screen[index].max(glare);
            let bloom = ScreenEffect::Bloom.index();
            fields.screen[bloom] = fields.screen[bloom].max(glare * 0.8);
        }
        fields
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of the Hollow World (see the module docs).
    pub(super) fn advance_hollow(&mut self, now: SimulationTick, output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_descents() {
            return;
        }
        self.announce_inner_sun(now, output);
        self.note_breakthroughs(output);
        self.step_exposure(now, output);
        self.note_isle_landings(output);
        self.release_spent_anchors(now, output);
        self.carry_mounts(output);
        self.advance_hollow_creatures(now, output);
        self.advance_serpent(now, output);
    }

    /// Step 1: dawn and night, announced on the change (only once a rig has broken in).
    fn announce_inner_sun(&mut self, now: SimulationTick, output: &mut TickOutput) {
        if self.hollow.revealed.is_empty() {
            return;
        }
        let phase = inner_sun_at(self.clock_at(now).world_millis()).phase;
        let before = self.hollow.sun_phase.replace(phase);
        if before.is_some_and(|before| before != phase) {
            let what = match phase {
                InnerPhase::Dawn => Some(HollowEvent::InnerDawn),
                InnerPhase::Night => Some(HollowEvent::InnerNight),
                InnerPhase::Day | InnerPhase::Dusk => None,
            };
            if let Some(what) = what {
                let tile = self.sun_well().unwrap_or(TilePosition { x: 0, y: 0 });
                output.push_event(hollow_event(what, None, tile));
            }
        }
    }

    /// Step 2: the reveal for an actor in the Inner Sky's open air for the first time.
    fn note_breakthroughs(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let fresh: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .copied()
            .filter(|id| !self.hollow.revealed.contains_key(id))
            .filter_map(|id| {
                let (x, y) = self.actor_position(id)?;
                let tile = frame.tile_at(x, y);
                self.in_hollow_sky(tile).then_some((id, tile))
            })
            .collect();
        let now = self.simulation_tick.next();
        for (player, tile) in fresh {
            self.hollow.revealed.insert(player, now);
            output.push_event(hollow_event(HollowEvent::Breakthrough, Some(player), tile));
        }
    }

    /// Step 3: exposure in the Sunforge's light, on world-time steps.
    fn step_exposure(&mut self, now: SimulationTick, output: &mut TickOutput) {
        let before = self.millis_at(SimulationTick::new(now.get().saturating_sub(1)));
        let at = self.millis_at(now);
        let steps = at / EXPOSURE_STEP_MILLIS - before / EXPOSURE_STEP_MILLIS;
        if steps == 0 {
            return;
        }
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let sun = inner_sun_at(self.clock_at(now).world_millis());
        let frame = self.frame();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player in players {
            let Some((x, y)) = self.actor_position(player) else {
                continue;
            };
            let tile = frame.tile_at(x, y);
            let in_forge = self.hollow_stratum_at(tile) == Some(Stratum::TheSunforge);
            let old = self.hollow.exposure_of(player);
            if !in_forge && old == 0 {
                continue;
            }
            let lit = in_forge && self.sunlit_at(tile);
            let step_seconds_milli = u32::try_from(EXPOSURE_STEP_MILLIS * steps).unwrap_or(0);
            let change = if lit {
                // Brighter by day, twice as hot at the forge's bottom; radiators take the edge.
                let along = u32::from(layout.along(tile.y));
                let radiator = self
                    .players
                    .get(&player)
                    .map_or(0, |rig| u32::from(rig.radiator_level.min(22)));
                let gain = EXPOSURE_GAIN_PER_SECOND * u32::from(sun.light) / 1_000
                    * (1_000 + along)
                    / 1_000
                    * (60 - radiator)
                    / 60;
                i32::try_from(gain * step_seconds_milli / 1_000).unwrap_or(0)
            } else {
                -i32::try_from(EXPOSURE_COOL_PER_SECOND * step_seconds_milli / 1_000).unwrap_or(0)
            };
            let mut new = u16::try_from((i32::from(old) + change).clamp(0, 1_400)).unwrap_or(0);
            // A Shade Canopy in the kit (Wave 9 integration, the progression's supply) throws
            // its shade as the light starts to scorch: the rig cools back to nothing.
            if old < EXPOSURE_SCORCH_AT
                && new >= EXPOSURE_SCORCH_AT
                && self.take_supply(player, crate::workshop::DeepSupply::ShadeCanopy)
            {
                new = 0;
            }
            if new == 0 {
                self.hollow.exposure.remove(&player);
            } else {
                self.hollow.exposure.insert(player, new);
            }
            if old < EXPOSURE_SCORCH_AT && new >= EXPOSURE_SCORCH_AT {
                output.push_event(hollow_event(HollowEvent::Overexposed, Some(player), tile));
            }
            if new >= EXPOSURE_SCORCH_AT {
                #[allow(clippy::cast_precision_loss, reason = "a few hundred milliseconds")]
                let seconds = step_seconds_milli as f32 / 1_000.0;
                let scale = crate::enemy::tier_scale(self.tier_at(tile));
                let damage = EXPOSURE_DAMAGE_PER_SECOND * scale * seconds;
                if let Some((pilot, _, _)) = self.pilot_of(player) {
                    let _ = self.damage_pilot(pilot, damage * 0.5, output);
                } else {
                    let _ = self.damage_player(player, damage);
                    if let Some(rig) = self.players.get_mut(&player) {
                        rig.hull = crate::quantize::quantize_simulation_scalar(rig.hull);
                    }
                }
            }
        }
    }

    /// Step 4: rigs touching down under an isle land feet first or on their roof.
    fn note_isle_landings(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player in players {
            let Some(rig) = self.players.get(&player) else {
                continue;
            };
            let (x, y, vy) = (rig.x, rig.y, rig.velocity_y);
            let orientation = rig.attitude.orientation;
            let tile = frame.tile_at(x, y);
            let inverted = self.hollow_stratum_at(tile) == Some(Stratum::UpsideDownIsles)
                && self.fields_at(x, y).gravity_y < -0.5
                && !self.is_on_foot(player);
            if !inverted {
                self.hollow.grounded.remove(&player);
                continue;
            }
            // Resting against the isle overhead (gravity up): rock just above the rig.
            let above = frame.tile_at(x, TILE_SIZE.mul_add(-0.75, y));
            let resting = vy.abs() < 30.0 && self.terrain.is_solid_at(above);
            let was = self.hollow.grounded.get(&player).copied();
            if !resting {
                if was.is_some() {
                    self.hollow.grounded.remove(&player);
                }
                continue;
            }
            if was.is_some() {
                continue;
            }
            // Feet first means turned half way round (pi): how far it leans from that.
            let lean = crate::field::trig::wrap_angle(orientation - std::f32::consts::PI).abs();
            let roof = lean > ROOF_LANDING_DEGREES.to_radians();
            self.hollow.grounded.insert(player, roof);
            if roof {
                let scale = crate::enemy::tier_scale(self.tier_at(tile));
                let _ = self.damage_player(player, ROOF_LANDING_DAMAGE * scale);
                if let Some(rig) = self.players.get_mut(&player) {
                    rig.hull = crate::quantize::quantize_simulation_scalar(rig.hull);
                }
                output.push_event(hollow_event(HollowEvent::RoofLanding, Some(player), tile));
            } else {
                self.hollow.isle_landings = self.hollow.isle_landings.saturating_add(1);
                output.push_event(hollow_event(
                    HollowEvent::LandedUpsideDown,
                    Some(player),
                    tile,
                ));
            }
        }
    }

    /// Step 5: anchors whose time is up let go.
    fn release_spent_anchors(&mut self, now: SimulationTick, output: &mut TickOutput) {
        let frame = self.frame();
        let spent: Vec<PlayerId> = self
            .hollow
            .anchors
            .iter()
            .filter(|(_, anchor)| anchor.until.is_some_and(|until| now >= until))
            .map(|(player, _)| *player)
            .collect();
        for player in spent {
            let ready = self.after_millis(now, ANCHOR_COOLDOWN_MILLIS);
            if let Some(anchor) = self.hollow.anchors.get_mut(&player) {
                anchor.until = None;
                anchor.ready = ready;
            }
            let tile = self
                .players
                .get(&player)
                .map_or(TilePosition { x: 0, y: 0 }, |rig| rig.tile_in(frame));
            output.push_event(hollow_event(
                HollowEvent::AnchorReleased,
                Some(player),
                tile,
            ));
        }
    }

    /// Step 6: each crestback carried under its riding pilot.
    fn carry_mounts(&mut self, output: &mut TickOutput) {
        let rides: Vec<(PlayerId, EntityId)> = self
            .hollow
            .mounts
            .iter()
            .map(|(player, mount)| (*player, EntityId(*mount)))
            .collect();
        for (player, mount) in rides {
            let pilot = self
                .pilot_of(player)
                .map(|(id, body, state)| (id, *body, state.facing));
            let mount_body = self.entities.get(mount).map(|entity| entity.body);
            let (Some((pilot_id, body, facing)), Some(mount_body)) = (pilot, mount_body) else {
                self.end_ride(player, output);
                continue;
            };
            if (mount_body.x - body.x).hypot(mount_body.y - body.y) > TILE_SIZE * 4.0 {
                self.end_ride(player, output);
                continue;
            }
            let half = EnemyKind::Crestback.contact_half().1;
            self.entities.update(mount, |entity| {
                entity.body.previous_x = entity.body.x;
                entity.body.previous_y = entity.body.y;
                entity.body.x = body.x;
                entity.body.y = body.y - half;
                entity.body.velocity_x = body.velocity_x;
                entity.body.velocity_y = body.velocity_y;
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.facing = facing;
                    state.mode = EnemyMode::Idle;
                    state.target = None;
                }
            });
            // The ride's boons stay on while it lasts (a tomb run sets its own and ends it).
            self.edit_pilot(pilot_id, |_, state| {
                if state.boons == PilotBoons::default() {
                    state.boons = riding_boons();
                }
            });
        }
    }

    /// Ends `player`'s ride: the pilot's boons go, the crestback walks again.
    fn end_ride(&mut self, player: PlayerId, output: &mut TickOutput) {
        let Some(mount) = self.hollow.mounts.remove(&player) else {
            return;
        };
        let frame = self.frame();
        let mut tile = TilePosition { x: 0, y: 0 };
        if let Some((pilot, body, _)) = self.pilot_of(player).map(|(id, b, s)| (id, *b, *s)) {
            tile = frame.tile_at(body.x, body.y);
            self.edit_pilot(pilot, |_, state| {
                if state.boons == riding_boons() {
                    state.boons = PilotBoons::default();
                }
            });
        }
        self.entities.update(EntityId(mount), |entity| {
            entity.body.velocity_x = 0.0;
        });
        output.push_event(hollow_event(HollowEvent::Dismounted, Some(player), tile));
    }

    /// The pilot's use button among crestbacks: climb onto the nearest one in reach, or get
    /// down. `None`: nothing to ride here (the use button does its usual work).
    pub(super) fn hollow_use(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        if self.hollow.mounts.contains_key(&player_id) {
            self.end_ride(player_id, output);
            return Some(PlayerScopedCommandOutcome::Applied);
        }
        let (pilot, body) = self.pilot_of(player_id).map(|(id, body, _)| (id, *body))?;
        let taken: Vec<u64> = self.hollow.mounts.values().copied().collect();
        let half = EnemyKind::Crestback.contact_half().1;
        let mount = self
            .enemies()
            .filter(|(entity, state)| {
                state.kind == EnemyKind::Crestback && !taken.contains(&entity.id.get())
            })
            .map(|(entity, _)| {
                (
                    (entity.body.x - body.x).hypot(entity.body.y + half - body.y),
                    entity.id,
                )
            })
            .filter(|(distance, _)| *distance <= MOUNT_REACH + half)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id)?;
        self.hollow.mounts.insert(player_id, mount.get());
        self.edit_pilot(pilot, |_, state| state.boons = riding_boons());
        let tile = self.frame().tile_at(body.x, body.y);
        output.push_event(hollow_event(HollowEvent::Mounted, Some(player_id), tile));
        Some(PlayerScopedCommandOutcome::Applied)
    }

    /// Whether the Hollow World drives enemy `id` itself this tick (a ridden crestback, the
    /// Sun Serpent and its coils, a clinging lode mite): the generic state machine leaves it.
    pub(super) fn hollow_holds(&self, id: EntityId, kind: EnemyKind) -> bool {
        match kind {
            EnemyKind::Crestback => self.hollow.mounts.values().any(|mount| *mount == id.get()),
            EnemyKind::SunSerpent | EnemyKind::SerpentCoil => true,
            EnemyKind::LodeMite => self
                .enemy_parts(id)
                .is_some_and(|(_, state)| state.phase == super::enemies::hollow::CLINGING_PHASE),
            _ => false,
        }
    }

    // --- Commands -----------------------------------------------------------------------------

    /// Whether `player_id` may do `action`.
    pub(super) fn check_hollow(
        &self,
        player_id: PlayerId,
        action: HollowAction,
    ) -> Result<(), HollowRefusal> {
        let now = self.simulation_tick;
        let anchor = self
            .hollow
            .anchors
            .get(&player_id)
            .copied()
            .unwrap_or_default();
        let holding = anchor.until.is_some_and(|until| now < until);
        let action = match action {
            HollowAction::ToggleAnchor if holding => HollowAction::ReleaseAnchor,
            HollowAction::ToggleAnchor => HollowAction::EngageAnchor,
            other => other,
        };
        match action {
            HollowAction::EngageAnchor | HollowAction::ToggleAnchor => {
                let fitted = self
                    .players
                    .get(&player_id)
                    .is_some_and(|rig| rig.hazard_gear.has_system(RigSystem::GravityAnchor));
                if !fitted {
                    return Err(HollowRefusal::NoAnchor);
                }
                if self.is_on_foot(player_id) {
                    return Err(HollowRefusal::OnFoot);
                }
                if anchor.until.is_some_and(|until| now < until) || now < anchor.ready {
                    return Err(HollowRefusal::Recharging);
                }
                Ok(())
            }
            HollowAction::ReleaseAnchor => {
                if holding {
                    Ok(())
                } else {
                    Err(HollowRefusal::NotHolding)
                }
            }
        }
    }

    /// Does `action` (checked by [`Self::check_hollow`]).
    pub(super) fn apply_hollow(
        &mut self,
        player_id: PlayerId,
        action: HollowAction,
        output: &mut TickOutput,
    ) {
        let now = self.simulation_tick.next();
        let frame = self.frame();
        let Some((orientation, tile)) = self
            .players
            .get(&player_id)
            .map(|rig| (rig.attitude.orientation, rig.tile_in(frame)))
        else {
            return;
        };
        let holding = self
            .hollow
            .anchored(player_id, self.simulation_tick)
            .is_some();
        let action = match action {
            HollowAction::ToggleAnchor if holding => HollowAction::ReleaseAnchor,
            HollowAction::ToggleAnchor => HollowAction::EngageAnchor,
            other => other,
        };
        match action {
            HollowAction::EngageAnchor | HollowAction::ToggleAnchor => {
                let until = self.after_millis(now, ANCHOR_MILLIS);
                let anchor = self.hollow.anchors.entry(player_id).or_default();
                anchor.quarter = anchor_quarter(orientation);
                anchor.until = Some(until);
                output.push_event(hollow_event(
                    HollowEvent::AnchorEngaged,
                    Some(player_id),
                    tile,
                ));
            }
            HollowAction::ReleaseAnchor => {
                let ready = self.after_millis(now, ANCHOR_COOLDOWN_MILLIS);
                if let Some(anchor) = self.hollow.anchors.get_mut(&player_id) {
                    anchor.until = None;
                    anchor.ready = ready;
                }
                output.push_event(hollow_event(
                    HollowEvent::AnchorReleased,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    // --- Dev seams ----------------------------------------------------------------------------

    /// Sets `player_id`'s exposure (tests and scenes).
    pub fn set_hollow_exposure(&mut self, player_id: PlayerId, exposure: u16) {
        if exposure == 0 {
            self.hollow.exposure.remove(&player_id);
        } else {
            self.hollow.exposure.insert(player_id, exposure);
        }
    }

    /// Marks `player_id` as having broken through already (scenes that skip the reveal).
    pub fn mark_hollow_revealed(&mut self, player_id: PlayerId) {
        self.hollow
            .revealed
            .insert(player_id, SimulationTick::new(0));
    }

    /// The pilot's use button among crestbacks outside the tick (scenes): climb on or get
    /// down. Whether anything happened.
    pub fn scene_pilot_use(&mut self, player_id: PlayerId, output: &mut TickOutput) -> bool {
        self.hollow_use(player_id, output).is_some()
    }

    /// Engages `player_id`'s Gravity Anchor outside the tick (scenes): whether it took hold.
    pub fn scene_engage_anchor(&mut self, player_id: PlayerId) -> bool {
        if self
            .check_hollow(player_id, HollowAction::EngageAnchor)
            .is_err()
        {
            return false;
        }
        let mut output = TickOutput::default();
        self.apply_hollow(player_id, HollowAction::EngageAnchor, &mut output);
        true
    }
}

/// The boons of a pilot riding a crestback.
const fn riding_boons() -> PilotBoons {
    PilotBoons {
        extra_jumps: 0,
        move_pct: MOUNT_MOVE_PCT,
        jump_pct: MOUNT_JUMP_PCT,
        glide_pct: 0,
        featherfall: true,
        trap_sight: false,
    }
}
