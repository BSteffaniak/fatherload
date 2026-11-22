//! The Living Dark in the world (Wave 8, Descent III; the rules live in
//! [`crate::living_dark`]).
//!
//! [`WorldState::advance_living_dark`] runs once per tick after the creatures move and before
//! the fluid pass (so the acid it pours moves the same tick). On worlds without Descents it
//! returns at once. In order:
//!
//! 1. **Openings** ([`WorldState::note_openings`]): every tile the tick opened (from the
//!    terrain journal, with the kind each write replaced) is read once: an opening in the Root
//!    Labyrinth or the Flesh Caverns is scheduled to grow back (unless scorched); a burst
//!    spore pod becomes a spore cloud; comb drilled in the Amber Court's hall costs standing;
//!    jelly cells broken in the matriarch's gallery count for the stolen brood.
//! 2. **Regrowth**: at most [`REGROW_BUDGET`] due tiles a tick grow back (roots, flesh); a
//!    tile with a body in it waits [`REGROW_RETRY_MILLIS`] more, so nothing is ever sealed
//!    into rock.
//! 3. **Spores**: rigs brushing a pod burst it; clouds clear when their time is up; a cloud
//!    stings a rig without the Bio-Filter.
//! 4. **Bounces** off cap crawlers' caps.
//! 5. **Tides** in the gullets near rigs (world-time steps, tick-rate safe).
//! 6. **The Mycelial Mind** (`world/enemies/living.rs`).

use crate::{
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::{Descent, Stratum},
    enemy::{EnemyKind, EnemyMode},
    entity::{EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    field::{FieldPreset, FieldSample, FieldZone},
    fluid::FluidKind,
    ids::{PlayerId, SimulationTick},
    living_dark::{
        COURT_AMENDS_STANDING, COURT_THEFT_STANDING, CourtService, Errand, GLOW_SEA_AMBIENT,
        GLOW_SEA_LAMP_SCALE, JELLY_PRICE, LivingDark, LivingDarkAction, LivingDarkEvent,
        LivingDarkRefusal, LivingSchedule, MAX_ROOT_AXES, MAX_SPORE_CLOUDS, REGROW_BUDGET,
        REGROW_RETRY_MILLIS, ROOT_AXE_PACK, ROOT_AXE_ROWS, ROOT_AXE_WIDTH, ROOT_BURN_RADIUS,
        Regrowth, SPORE_CLOUD_HALF, SPORE_CLOUD_MILLIS, SPORE_DAMAGE_PER_SECOND,
        SPORE_POD_BRUSH_PIXELS, SPORE_SHOT_CLOUD_MILLIS, SwarmState, TIDE_HIGH_PERMILLE,
        TIDE_REACH_TILES, TIDE_STEP_MILLIS, TidePhase, regrowth_of, tide_at,
    },
    logistics::scaled_price,
    pilot::PILOT_HALF_WIDTH,
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::{
            DescentLayout,
            living::{self, LivingStampKind},
        },
        noise::hash2,
    },
};

use super::{
    WorldState,
    bombs::Blast,
    movement::{AUTHORITATIVE_PLAYER_RADIUS, MUSHROOM_BOUNCE_KEEP, MUSHROOM_BOUNCE_SPEED},
};

/// Salt of the regrowth jitter.
const SALT_REGROW: u64 = 0x11D4_1001;
/// Slowest fall a cap crawler's cap bounces (px/s).
const CRAWLER_BOUNCE_MIN_IMPACT: f32 = 110.0;

/// A Living Dark event at `tile`.
pub(super) const fn living_dark_event(
    what: LivingDarkEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::LivingDark {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The Living Dark's shared state.
    #[must_use]
    pub const fn living_dark(&self) -> &LivingDark {
        &self.living_dark
    }

    /// The Living Dark's host bookkeeping (tests, tools).
    #[must_use]
    pub const fn living_schedule(&self) -> &LivingSchedule {
        &self.living_schedule
    }

    /// A Deep Descent world's stratum layout (cached), `None` for older worlds.
    #[must_use]
    pub fn descent_strata_layout(&self) -> Option<DescentLayout> {
        let settings = self.terrain.settings();
        settings
            .generator
            .has_descents()
            .then(|| crate::descent::fields::cached_layout(settings))
    }

    /// The stratum at `tile` on a Deep Descent world (`None` on older worlds).
    #[must_use]
    pub fn deep_stratum_at(&self, tile: TilePosition) -> Option<Stratum> {
        let layout = self.descent_strata_layout()?;
        Some(crate::worldgen::descent::stratum_in(
            self.terrain.settings(),
            &layout,
            tile.x,
            tile.y,
        ))
    }

    /// The Amber Court's hall (Deep Descent worlds).
    #[must_use]
    pub fn court_hall(&self) -> Option<TileRect> {
        living::court_hall(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// The Mycelial Mind's hollow (Deep Descent worlds).
    #[must_use]
    pub fn mind_hollow(&self) -> Option<TileRect> {
        living::mind_hollow(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// The brood matriarch's gallery (Deep Descent worlds).
    #[must_use]
    pub fn matriarch_gallery(&self) -> Option<TileRect> {
        living::matriarch_gallery(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// Whether `player_id`'s rig stands in the Amber Court's hall.
    #[must_use]
    pub fn at_court(&self, player_id: PlayerId) -> bool {
        let Some(hall) = self.court_hall() else {
            return false;
        };
        self.players
            .get(&player_id)
            .is_some_and(|player| hall.contains(player.tile_in(self.frame())))
    }

    /// The acid tide's phase and level (per mille of high water) now.
    #[must_use]
    pub fn tide_now(&self) -> (TidePhase, u32) {
        tide_at(self.millis_at(self.simulation_tick))
    }

    /// World time at `tick`, milliseconds (tick-rate safe).
    #[must_use]
    pub fn millis_at(&self, tick: SimulationTick) -> u64 {
        let millis = u128::from(tick.get()) * 1_000 / u128::from(self.tick_rate_hz().max(1));
        u64::try_from(millis).unwrap_or(u64::MAX)
    }

    /// The Flesh Caverns' heartbeat where `player_id`'s rig is (beats a minute), `None`
    /// outside them. The music's tempo follows it.
    #[must_use]
    pub fn heartbeat_for(&self, player_id: PlayerId) -> Option<u32> {
        let (x, y) = self.actor_position(player_id)?;
        let tile = self.frame().tile_at(x, y);
        if self.deep_stratum_at(tile)? != Stratum::FleshCaverns {
            return None;
        }
        let along = self.descent_strata_layout()?.along(tile.y);
        Some(crate::living_dark::heartbeat_bpm(along, self.tide_now().0))
    }

    /// How far `player_id`'s rig lamp reaches, as a multiple of its usual reach: Lamp II's
    /// wider beam, and in the Glow Sea a candle without it ([`GLOW_SEA_LAMP_SCALE`]).
    #[must_use]
    pub fn lamp_scale_for(&self, player_id: PlayerId) -> f32 {
        let Some(player) = self.players.get(&player_id) else {
            return 1.0;
        };
        let lamp_two = self.progression_rules()
            == crate::depth_progression::ProgressionRules::Descent
            && player.hazard_gear.has_system(RigSystem::LampII);
        let glow_sea = self.deep_stratum_at(player.tile_in(self.frame())) == Some(Stratum::GlowSea);
        let beam = match (glow_sea, lamp_two) {
            (true, false) => GLOW_SEA_LAMP_SCALE,
            (_, true) => crate::depth_progression::LAMP_II_LIGHT_SCALE,
            (false, false) => 1.0,
        };
        // The Headlamp line widens the beam a little per tier (Wave 8), but no Headlamp tier
        // lights the Glow Sea: only Lamp II does.
        if self.progression_rules().is_descent() && !(glow_sea && !lamp_two) {
            beam * crate::depth_progression::headlamp_light_scale(player.hazard_gear.headlamp)
        } else {
            beam
        }
    }

    /// Light at local pixel `(x, y)` in the Glow Sea (`0..=1`): its near-total darkness, every
    /// light-giving creature's pool and every rig's lamp (scaled as [`Self::lamp_scale_for`]).
    /// `None` outside the Glow Sea. The renderer draws the same lights.
    #[must_use]
    pub fn glow_light_at(&self, x: f32, y: f32) -> Option<f32> {
        let tile = self.frame().tile_at(x, y);
        if self.deep_stratum_at(tile)? != Stratum::GlowSea {
            return None;
        }
        let pool = |cx: f32, cy: f32, radius: f32, strength: f32| {
            let distance = (cx - x).hypot(cy - y);
            if distance >= radius {
                0.0
            } else {
                let falloff = 1.0 - distance / radius;
                strength * falloff * falloff
            }
        };
        let mut light = GLOW_SEA_AMBIENT;
        for (entity, state) in self.enemies() {
            let reach = state.kind.light_tiles();
            if reach > 0.0 {
                light += pool(entity.body.x, entity.body.y, reach * TILE_SIZE, 0.9);
            }
        }
        for (player_id, player) in &self.players {
            let reach = crate::living_dark::GLOW_SEA_RIG_LAMP_TILES
                * TILE_SIZE
                * self.lamp_scale_for(*player_id);
            light += pool(player.x, player.y, reach, 1.0);
        }
        Some(light.min(1.0))
    }

    /// The fields a seat centred on `(x, y)` sees for `player_id`: the world's fields, less
    /// the spore clouds when the rig has the Bio-Filter (it sees through spores).
    #[must_use]
    pub fn view_fields_for(&self, player_id: PlayerId, x: f32, y: f32) -> FieldSample {
        let filtered = self
            .players
            .get(&player_id)
            .is_some_and(|player| player.hazard_gear.has_system(RigSystem::BioFilter));
        if !filtered || self.living_dark.spores.is_empty() {
            return self.fields_at(x, y);
        }
        let mut zones = self.fields.clone();
        for id in self.living_dark.spores.keys() {
            let _ = zones.remove(*id);
        }
        crate::field::sample_in(&zones, self.terrain.settings(), self.terrain.frame(), x, y)
    }

    // --- Commands -----------------------------------------------------------------------------

    /// Whether `player_id` (its rig on `tile`) may do `action`, and its price in credits.
    pub(super) fn check_living_dark(
        &self,
        player_id: PlayerId,
        tile: TilePosition,
        action: LivingDarkAction,
    ) -> Result<u32, LivingDarkRefusal> {
        let descent_here = self.descent_of_row(tile.y);
        let at_court = self.court_hall().is_some_and(|hall| hall.contains(tile));
        let hive = &self.living_dark.hive;
        match action {
            LivingDarkAction::BuyRootAxes => {
                let seller =
                    at_court || self.logistics.outpost_at(tile).is_some() || self.in_town(tile);
                if !seller {
                    return Err(LivingDarkRefusal::NoAxeSeller);
                }
                if at_court && hive.court_hostile() {
                    return Err(LivingDarkRefusal::CourtHostile);
                }
                let (count, price) = ROOT_AXE_PACK;
                if self.living_dark.axes_of(player_id).saturating_add(count) > MAX_ROOT_AXES {
                    return Err(LivingDarkRefusal::AxesFull);
                }
                Ok(scaled_price(price, descent_here))
            }
            LivingDarkAction::FireRootAxe => {
                if self.living_dark.axes_of(player_id) == 0 {
                    return Err(LivingDarkRefusal::NoAxes);
                }
                if self.axe_cut(tile).is_empty() {
                    return Err(LivingDarkRefusal::NothingToCut);
                }
                Ok(0)
            }
            LivingDarkAction::Court(service) => {
                if !at_court {
                    return Err(LivingDarkRefusal::NotAtCourt);
                }
                if hive.court_hostile() && service != CourtService::MakeAmends {
                    return Err(LivingDarkRefusal::CourtHostile);
                }
                let price = |base: u32| scaled_price(base, descent_here);
                match service {
                    CourtService::RoyalJelly => {
                        let player = self
                            .players
                            .get(&player_id)
                            .ok_or(LivingDarkRefusal::NothingToMend)?;
                        if player.hull >= player.max_hull() && player.fuel >= player.fuel_capacity {
                            return Err(LivingDarkRefusal::NothingToMend);
                        }
                        let base = if hive.honoured() {
                            JELLY_PRICE / 2
                        } else {
                            JELLY_PRICE
                        };
                        Ok(price(base))
                    }
                    CourtService::TakeErrand => {
                        hive.next_errand().ok_or(LivingDarkRefusal::NoErrand)?;
                        Ok(0)
                    }
                    CourtService::TurnIn => {
                        if hive.errand.is_none() {
                            return Err(LivingDarkRefusal::NoErrand);
                        }
                        if !hive.errand_ready() {
                            return Err(LivingDarkRefusal::ErrandUnfinished);
                        }
                        Ok(0)
                    }
                    CourtService::BrokerTruce => {
                        if hive.errand != Some(Errand::TheMatriarch)
                            || hive.swarm != SwarmState::Raiding
                        {
                            return Err(LivingDarkRefusal::NoTruce);
                        }
                        Ok(price(crate::living_dark::TRUCE_PRICE))
                    }
                    CourtService::MakeAmends => {
                        if !hive.court_hostile() {
                            return Err(LivingDarkRefusal::NoAmendsNeeded);
                        }
                        Ok(price(crate::living_dark::AMENDS_PRICE))
                    }
                }
            }
        }
    }

    /// Resolves a Living Dark command inside the tick (already checked and charged).
    pub(super) fn apply_living_dark(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        action: LivingDarkAction,
        output: &mut TickOutput,
    ) {
        match action {
            LivingDarkAction::BuyRootAxes => {
                let count = self.living_dark.axes.entry(player_id).or_insert(0);
                *count = count.saturating_add(ROOT_AXE_PACK.0).min(MAX_ROOT_AXES);
                output.push_event(living_dark_event(
                    LivingDarkEvent::Bought,
                    Some(player_id),
                    tile,
                ));
            }
            LivingDarkAction::FireRootAxe => {
                let cut = self.axe_cut(tile);
                if let Some(count) = self.living_dark.axes.get_mut(&player_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.living_dark.axes.remove(&player_id);
                    }
                }
                let now = self.simulation_tick;
                for position in &cut {
                    self.living_schedule.scorch(*position, now);
                    let _ = self.terrain.set_kind(*position, TileKind::Air);
                }
                // The shaft's walls are scorched too: they never close in on it.
                for position in &cut {
                    for dx in [-1, 1] {
                        let side = TilePosition {
                            x: position.x + dx,
                            y: position.y,
                        };
                        if self
                            .terrain
                            .tile(side)
                            .is_some_and(|t| t.kind == TileKind::Air)
                        {
                            self.living_schedule.scorch(side, now);
                        }
                    }
                }
                output.push_event(living_dark_event(
                    LivingDarkEvent::RootAxeFired,
                    Some(player_id),
                    tile,
                ));
            }
            LivingDarkAction::Court(service) => self.apply_court(player_id, tile, service, output),
        }
    }

    fn apply_court(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        service: CourtService,
        output: &mut TickOutput,
    ) {
        let hive = &mut self.living_dark.hive;
        match service {
            CourtService::RoyalJelly => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.hull = player.max_hull();
                    player.fuel = quantize(
                        player
                            .fuel_capacity
                            .mul_add(0.25, player.fuel)
                            .min(player.fuel_capacity),
                    );
                }
                output.push_event(living_dark_event(
                    LivingDarkEvent::Bought,
                    Some(player_id),
                    tile,
                ));
            }
            CourtService::TakeErrand => {
                hive.errand = hive.next_errand();
                hive.progress = 0;
                output.push_event(living_dark_event(
                    LivingDarkEvent::CourtStanding,
                    Some(player_id),
                    tile,
                ));
            }
            CourtService::TurnIn => {
                if let Some(errand) = hive.errand.take() {
                    let (standing, credits) = errand.reward();
                    hive.done.insert(errand);
                    hive.progress = 0;
                    hive.shift(standing);
                    if let Some(player) = self.players.get_mut(&player_id) {
                        player.credits = player.credits.saturating_add(credits);
                    }
                    output.push_event(living_dark_event(
                        LivingDarkEvent::ErrandDone,
                        Some(player_id),
                        tile,
                    ));
                }
            }
            CourtService::BrokerTruce => {
                hive.swarm = SwarmState::Truce;
                hive.errand = None;
                hive.progress = 0;
                hive.done.insert(Errand::TheMatriarch);
                hive.shift(-crate::living_dark::TRUCE_STANDING);
                output.push_event(living_dark_event(
                    LivingDarkEvent::SwarmTruce,
                    Some(player_id),
                    tile,
                ));
            }
            CourtService::MakeAmends => {
                hive.standing = COURT_AMENDS_STANDING;
                output.push_event(living_dark_event(
                    LivingDarkEvent::CourtStanding,
                    Some(player_id),
                    tile,
                ));
            }
        }
    }

    /// The tiles a root axe fired from `tile` cuts: a shaft [`ROOT_AXE_WIDTH`] wide straight up
    /// for [`ROOT_AXE_ROWS`] rows through roots, charred roots, flesh and plain rock (stopping
    /// a column at anything else solid), and open tiles in it that would grow back.
    fn axe_cut(&self, tile: TilePosition) -> Vec<TilePosition> {
        let mut cut = Vec::new();
        let half = ROOT_AXE_WIDTH / 2;
        for dx in -half..=half {
            for rise in 1..=ROOT_AXE_ROWS {
                let position = TilePosition {
                    x: tile.x + dx,
                    y: tile.y - rise,
                };
                let Some(kind) = self.terrain.tile(position).map(|tile| tile.kind) else {
                    break;
                };
                match kind {
                    TileKind::Cave(CaveTile::Root | CaveTile::CharredRoot | CaveTile::Flesh)
                    | TileKind::Dirt
                    | TileKind::Clay
                    | TileKind::Stone
                    | TileKind::HardRock
                        if !self.terrain.is_indestructible_at(position) =>
                    {
                        cut.push(position);
                    }
                    TileKind::Air if self.living_schedule.is_waiting(position) => {
                        cut.push(position);
                    }
                    TileKind::Air => {}
                    _ if !self.terrain.is_solid_at(position) => {}
                    _ => break,
                }
            }
        }
        cut
    }

    // --- Staging (tests, scenes and dev tools) ----------------------------------------------

    /// Releases a spore cloud centred on `tile` before play (scenes, tests).
    pub fn release_spore_cloud(&mut self, tile: TilePosition) {
        let mut output = TickOutput::default();
        self.spawn_spore_cloud(tile, SPORE_CLOUD_MILLIS, None, &mut output);
    }

    /// Opens every tile of `rect` as if it had been dug this moment, scheduling the Root
    /// Labyrinth's and the Flesh Caverns' regrowth for it (scenes, tests).
    pub fn dig_living_tiles(&mut self, rect: TileRect) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        self.terrain.start_journal();
        for position in rect.positions() {
            let _ = self.terrain.set_kind(position, TileKind::Air);
        }
        let mut output = TickOutput::default();
        let now = self.simulation_tick;
        self.note_openings(&layout, now, &mut output);
        let _ = self.terrain.finish_journal();
    }

    /// Fills every gullet within reach of `tile` to the tide level `per_mille` of high water,
    /// as the tide would (scenes, tests).
    pub fn flood_gullets_near(&mut self, tile: TilePosition, per_mille: u32) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let settings = *self.terrain.settings();
        let area = TileRect::around(tile, TIDE_REACH_TILES, TIDE_REACH_TILES);
        for stamp in living::stamps_with(&settings, &layout, &area) {
            if stamp.kind == LivingStampKind::Gullet {
                self.set_gullet_tide(&stamp, per_mille);
            }
        }
    }

    // --- Hooks --------------------------------------------------------------------------------

    /// This tick's blasts in the Living Dark: fire burns roots away for good (the burn paths).
    pub(super) fn living_dark_blasts(&mut self, blasts: &[Blast], output: &mut TickOutput) {
        if self.descent_strata_layout().is_none() {
            return;
        }
        let now = self.simulation_tick;
        let frame = self.frame();
        for blast in blasts {
            if blast.kind != crate::explosive::ExplosiveKind::Incendiary {
                continue;
            }
            let centre = frame.tile_at(blast.x, blast.y);
            let mut burned = 0_u32;
            for dy in -ROOT_BURN_RADIUS..=ROOT_BURN_RADIUS {
                for dx in -ROOT_BURN_RADIUS..=ROOT_BURN_RADIUS {
                    if dx * dx + dy * dy > ROOT_BURN_RADIUS * ROOT_BURN_RADIUS + 1 {
                        continue;
                    }
                    let position = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    match self.terrain.tile(position).map(|tile| tile.kind) {
                        Some(TileKind::Cave(CaveTile::Root | CaveTile::CharredRoot)) => {
                            self.living_schedule.scorch(position, now);
                            let _ = self.terrain.set_kind(position, TileKind::Air);
                            burned += 1;
                        }
                        Some(TileKind::Air) if self.living_schedule.is_waiting(position) => {
                            self.living_schedule.scorch(position, now);
                            burned += 1;
                        }
                        _ => {}
                    }
                }
            }
            if burned > 0 {
                output.push_event(living_dark_event(
                    LivingDarkEvent::RootsBurned,
                    blast.owner,
                    centre,
                ));
            }
        }
    }

    /// A Living Dark creature died (kill credit `by`): the errands count it, the Court mourns
    /// its drones, the Swarm breaks with its matriarch, and the Mind's fight moves on.
    pub(super) fn living_dark_kill(
        &mut self,
        id: u64,
        kind: EnemyKind,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let hive = &mut self.living_dark.hive;
        match kind {
            EnemyKind::HiveSoldier | EnemyKind::AcidLancer => {
                if by.is_some() && hive.errand == Some(Errand::SoldiersAtTheGate) {
                    hive.progress = hive.progress.saturating_add(1);
                }
            }
            EnemyKind::HiveDrone if by.is_some() => {
                let was = hive.court_hostile();
                hive.shift(-5);
                output.push_event(living_dark_event(LivingDarkEvent::CourtStanding, by, tile));
                if !was && hive.court_hostile() {
                    output.push_event(living_dark_event(LivingDarkEvent::CourtHostile, by, tile));
                }
            }
            EnemyKind::SwarmMatriarch => {
                hive.swarm = SwarmState::Broken;
                if hive.errand == Some(Errand::TheMatriarch) {
                    hive.progress = hive.progress.saturating_add(1);
                }
                output.push_event(living_dark_event(LivingDarkEvent::SwarmBroken, by, tile));
            }
            EnemyKind::MindNode => {
                self.living_dark.mind.nodes.retain(|node| *node != id);
            }
            EnemyKind::MycelialMind => self.mind_fell(tile, by, output),
            _ => {}
        }
    }

    /// Bursts a spore shot that landed at local pixel `(x, y)` into a cloud.
    pub(super) fn burst_spore_shot(
        &mut self,
        shot: crate::enemy::ShotKind,
        x: f32,
        y: f32,
        output: &mut TickOutput,
    ) {
        if shot.bursts_into_spores() {
            let tile = self.frame().tile_at(x, y);
            self.spawn_spore_cloud(tile, SPORE_SHOT_CLOUD_MILLIS, None, output);
        }
    }

    /// A spore cloud centred on `tile` for `millis`.
    pub(crate) fn spawn_spore_cloud(
        &mut self,
        tile: TilePosition,
        millis: u64,
        player_id: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        while self.living_dark.spores.len() >= MAX_SPORE_CLOUDS {
            let Some(oldest) = self
                .living_dark
                .spores
                .iter()
                .min_by_key(|(id, until)| (**until, **id))
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.living_dark.spores.remove(&oldest);
            let _ = self.fields.remove(oldest);
        }
        let id = self.fields.insert(FieldZone::of_preset(
            FieldPreset::Spores,
            (tile.x, tile.y),
            SPORE_CLOUD_HALF,
        ));
        let until = SimulationTick::new(
            self.simulation_tick
                .get()
                .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
        );
        self.living_dark.spores.insert(id, until);
        output.push_event(living_dark_event(
            LivingDarkEvent::SporeBurst,
            player_id,
            tile,
        ));
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of the Living Dark (see the module docs).
    pub(super) fn advance_living_dark(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        self.note_openings(&layout, next_tick, output);
        self.regrow(next_tick, output);
        self.brush_spore_pods(output);
        self.advance_spores(next_tick);
        self.bounce_on_crawlers(output);
        self.advance_tides(&layout, next_tick, output);
        self.advance_mind(next_tick, output);
    }

    /// Reads the tiles this tick opened (see the module docs, step 1).
    fn note_openings(
        &mut self,
        layout: &DescentLayout,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let (first, last) = layout.descent_rows(Descent::LivingDark);
        let changes: Vec<(TilePosition, TileKind, TileKind)> = self
            .terrain
            .journal_with_before()
            .filter(|(change, before)| {
                change.position.y >= first - 24
                    && change.position.y <= last + 24
                    && change.tile.kind != *before
            })
            .map(|(change, before)| (change.position, before, change.tile.kind))
            .collect();
        if changes.is_empty() {
            return;
        }
        let settings = *self.terrain.settings();
        let court = living::court_hall(&settings, layout);
        let gallery = living::matriarch_gallery(&settings, layout);
        let mut theft = 0_i16;
        for (position, before, after) in changes {
            if after == TileKind::Air {
                let stratum =
                    crate::worldgen::descent::stratum_in(&settings, layout, position.x, position.y);
                if let Some((kind, millis)) = regrowth_of(stratum, layout.along(position.y))
                    && !self.living_schedule.scorched.contains_key(&position)
                    && self.logistics.outpost_at(position).is_none()
                    && !self.in_lift_shaft(position)
                {
                    // A little jitter so a tunnel closes tile by tile, not all at once.
                    let jitter = hash2(settings.seed, SALT_REGROW, position.x, position.y) % 400;
                    let millis = millis * (800 + jitter) / 1_000;
                    let due = SimulationTick::new(
                        now.get()
                            .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
                    );
                    self.living_schedule.schedule(position, due, kind);
                }
            }
            if before == TileKind::Cave(CaveTile::SporePod) {
                self.spawn_spore_cloud(position, SPORE_CLOUD_MILLIS, None, output);
            }
            let comb = matches!(before, TileKind::Cave(CaveTile::Comb | CaveTile::JellyCell));
            if comb && court.is_some_and(|hall| hall.contains(position)) {
                theft = theft.saturating_add(COURT_THEFT_STANDING);
            }
            if before == TileKind::Cave(CaveTile::JellyCell)
                && gallery.is_some_and(|hall| hall.contains(position))
                && self.living_dark.hive.errand == Some(Errand::TheStolenBrood)
            {
                let hive = &mut self.living_dark.hive;
                hive.progress = hive.progress.saturating_add(1);
            }
        }
        if theft > 0 {
            let hive = &mut self.living_dark.hive;
            let was = hive.court_hostile();
            hive.shift(-theft);
            let at = court.map_or(TilePosition { x: 0, y: 0 }, |hall| hall.center());
            output.push_event(living_dark_event(LivingDarkEvent::CourtStanding, None, at));
            if !was && hive.court_hostile() {
                output.push_event(living_dark_event(LivingDarkEvent::CourtHostile, None, at));
            }
        }
    }

    /// Whether `tile` lies in a lift's shaft (its column, from a row above its top station to
    /// a row below its bottom one): roots and flesh never close a lift's ride.
    fn in_lift_shaft(&self, tile: TilePosition) -> bool {
        self.logistics.lift_on_column(tile.x).is_some_and(|lift| {
            match (lift.stations.first(), lift.stations.last()) {
                (Some(top), Some(bottom)) => (top - 1..=bottom + 1).contains(&tile.y),
                _ => false,
            }
        })
    }

    /// Whether any body (a rig, a pilot, a creature) overlaps `tile`.
    fn tile_occupied(&self, tile: TilePosition) -> bool {
        let frame = self.frame();
        let left = frame.col_px(tile.x);
        let top = frame.row_px(tile.y);
        let overlaps = |x: f32, y: f32, half_w: f32, half_h: f32| {
            x + half_w > left
                && x - half_w < left + TILE_SIZE
                && y + half_h > top
                && y - half_h < top + TILE_SIZE
        };
        let rig = AUTHORITATIVE_PLAYER_RADIUS + 1.0;
        if self
            .players
            .values()
            .any(|player| overlaps(player.x, player.y, rig, rig))
        {
            return true;
        }
        if self.pilots().any(|(_, body, pilot)| {
            let (x, y) = pilot.middle(body);
            overlaps(x, y, PILOT_HALF_WIDTH, pilot.height() * 0.5)
        }) {
            return true;
        }
        self.enemies().any(|(entity, state)| {
            let radius = state.kind.stats().radius;
            overlaps(entity.body.x, entity.body.y, radius, radius)
        })
    }

    /// Grows back the due tiles (step 2).
    fn regrow(&mut self, now: SimulationTick, output: &mut TickOutput) {
        if self.living_schedule.is_idle() {
            return;
        }
        let due: Vec<((SimulationTick, TilePosition), Regrowth)> = self
            .living_schedule
            .due
            .iter()
            .take_while(|((tick, _), _)| *tick <= now)
            .take(REGROW_BUDGET)
            .map(|(key, kind)| (*key, *kind))
            .collect();
        let mut roots: Option<TilePosition> = None;
        let mut flesh: Option<TilePosition> = None;
        for ((tick, position), kind) in due {
            self.living_schedule.due.remove(&(tick, position));
            self.living_schedule.waiting.remove(&position);
            let open = self
                .terrain
                .tile(position)
                .is_some_and(|tile| tile.kind == TileKind::Air);
            if !open || self.in_lift_shaft(position) {
                // Filled some other way (a block placed, fluid flowed in), or a lift was built
                // through it since: nothing to grow.
                continue;
            }
            if self.tile_occupied(position) {
                let retry = SimulationTick::new(
                    now.get()
                        .saturating_add(self.timing.ticks_for_millis(REGROW_RETRY_MILLIS).max(1)),
                );
                self.living_schedule.schedule(position, retry, kind);
                continue;
            }
            let tile = match kind {
                Regrowth::Root => CaveTile::Root,
                Regrowth::Flesh => CaveTile::Flesh,
            };
            if self.terrain.set_kind(position, TileKind::Cave(tile)) {
                match kind {
                    Regrowth::Root => roots = roots.or(Some(position)),
                    Regrowth::Flesh => flesh = flesh.or(Some(position)),
                }
            }
        }
        if let Some(at) = roots {
            output.push_event(living_dark_event(LivingDarkEvent::RootsRegrew, None, at));
        }
        if let Some(at) = flesh {
            output.push_event(living_dark_event(LivingDarkEvent::FleshHealed, None, at));
        }
    }

    /// Rigs brushing a spore pod burst it (step 3).
    fn brush_spore_pods(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let mut burst: Vec<(TilePosition, PlayerId)> = Vec::new();
        for (player_id, player) in &self.players {
            let centre = player.tile_in(frame);
            if !matches!(
                self.deep_stratum_at(centre),
                Some(Stratum::FungalForest | Stratum::GlowSea)
            ) {
                continue;
            }
            let reach = AUTHORITATIVE_PLAYER_RADIUS + SPORE_POD_BRUSH_PIXELS;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let position = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    if self.terrain.tile(position).map(|tile| tile.kind)
                        != Some(TileKind::Cave(CaveTile::SporePod))
                    {
                        continue;
                    }
                    let (left, top) = (frame.col_px(position.x), frame.row_px(position.y));
                    let near_x = player.x.clamp(left, left + TILE_SIZE);
                    let near_y = player.y.clamp(top, top + TILE_SIZE);
                    if (player.x - near_x).hypot(player.y - near_y) <= reach {
                        burst.push((position, *player_id));
                    }
                }
            }
        }
        for (position, player_id) in burst {
            // Writing the pod away bursts it next tick's way: do it now so the cloud is here.
            if self.terrain.set_kind(position, TileKind::Air) {
                self.spawn_spore_cloud(position, SPORE_CLOUD_MILLIS, Some(player_id), output);
            }
        }
    }

    /// Clears spent spore clouds and stings rigs inside clouds (step 3).
    fn advance_spores(&mut self, now: SimulationTick) {
        if self.living_dark.spores.is_empty() {
            return;
        }
        let spent: Vec<u32> = self
            .living_dark
            .spores
            .iter()
            .filter(|(_, until)| **until <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in spent {
            self.living_dark.spores.remove(&id);
            let _ = self.fields.remove(id);
        }
        // Zones gone some other way (dev clears) leave the ledger too.
        let fields = &self.fields;
        self.living_dark
            .spores
            .retain(|id, _| fields.iter().any(|(zone, _)| zone == *id));
        if self.damage_immune() {
            return;
        }
        let frame = self.frame();
        let dt = self.timing.delta_seconds;
        let stung: Vec<(PlayerId, f32)> = self
            .players
            .iter()
            .filter(|(_, player)| !player.hazard_gear.has_system(RigSystem::BioFilter))
            .filter_map(|(player_id, player)| {
                let weight = self
                    .living_dark
                    .spores
                    .keys()
                    .filter_map(|id| self.fields.iter().find(|(zone, _)| zone == id))
                    .map(|(_, zone)| zone.weight_in(frame, player.x, player.y))
                    .fold(0.0_f32, f32::max);
                // Deeper spores sting as hard as the creatures there bite.
                let scale = crate::enemy::tier_scale(self.tier_at(player.tile_in(frame)));
                (weight > 0.0)
                    .then_some((*player_id, SPORE_DAMAGE_PER_SECOND * scale * weight * dt))
            })
            .collect();
        for (player_id, damage) in stung {
            let _ = self.damage_player(player_id, damage);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.hull = quantize(player.hull);
            }
        }
    }

    /// Rigs falling onto a cap crawler's cap bounce off it (step 4).
    fn bounce_on_crawlers(&mut self, output: &mut TickOutput) {
        let crawlers: Vec<(f32, f32, f32, f32)> = self
            .enemies()
            .filter(|(_, state)| state.kind == EnemyKind::CapCrawler)
            .map(|(entity, state)| {
                let (half_w, half_h) = state.kind.contact_half();
                (entity.body.x, entity.body.y, half_w, half_h)
            })
            .collect();
        if crawlers.is_empty() {
            return;
        }
        let frame = self.frame();
        let mut bounced = Vec::new();
        for (player_id, player) in &mut self.players {
            if player.velocity_y < CRAWLER_BOUNCE_MIN_IMPACT {
                continue;
            }
            let bottom = player.y + AUTHORITATIVE_PLAYER_RADIUS;
            let on_cap = crawlers.iter().any(|(x, y, half_w, half_h)| {
                (player.x - x).abs() <= half_w + AUTHORITATIVE_PLAYER_RADIUS * 0.5
                    && bottom >= y - half_h - 3.0
                    && bottom <= y - half_h * 0.2
            });
            if on_cap {
                player.velocity_y = quantize(
                    -(player.velocity_y * MUSHROOM_BOUNCE_KEEP).max(MUSHROOM_BOUNCE_SPEED),
                );
                bounced.push((*player_id, player.tile_in(frame)));
            }
        }
        for (player_id, tile) in bounced {
            output.push_event(living_dark_event(
                LivingDarkEvent::CapBounce,
                Some(player_id),
                tile,
            ));
        }
    }

    /// The acid tides of the gullets near rigs (step 5).
    fn advance_tides(
        &mut self,
        layout: &DescentLayout,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let step_ticks = self.timing.ticks_for_millis(TIDE_STEP_MILLIS).max(1);
        let step = now.get() / step_ticks;
        if step == self.living_schedule.tide_step {
            return;
        }
        let previous_step = self.living_schedule.tide_step;
        self.living_schedule.tide_step = step;
        let (phase, level) = tide_at(self.millis_at(now));
        let (before, _) = tide_at(self.millis_at(SimulationTick::new(
            previous_step.saturating_mul(step_ticks),
        )));
        let frame = self.frame();
        let rigs: Vec<TilePosition> = self
            .players
            .values()
            .map(|player| player.tile_in(frame))
            .filter(|tile| {
                let (top, bottom) = layout.rows(Stratum::FleshCaverns);
                tile.y >= top - TIDE_REACH_TILES && tile.y <= bottom + TIDE_REACH_TILES
            })
            .collect();
        if rigs.is_empty() {
            return;
        }
        let settings = *self.terrain.settings();
        let mut gullets = Vec::new();
        for rig in &rigs {
            let area = TileRect::around(*rig, TIDE_REACH_TILES, TIDE_REACH_TILES);
            for stamp in living::stamps_with(&settings, layout, &area) {
                if stamp.kind == LivingStampKind::Gullet && !gullets.contains(&stamp) {
                    gullets.push(stamp);
                }
            }
        }
        if gullets.is_empty() {
            return;
        }
        if before != phase && matches!(phase, TidePhase::Rising | TidePhase::Ebbing) {
            let what = if phase == TidePhase::Rising {
                LivingDarkEvent::TideRising
            } else {
                LivingDarkEvent::TideEbbing
            };
            output.push_event(living_dark_event(what, None, gullets[0].bounds.center()));
        }
        for stamp in gullets {
            self.set_gullet_tide(&stamp, level);
        }
    }

    /// Raises or lowers `stamp`'s (a gullet's) acid to `level` per mille of its high water:
    /// open air under the surface fills with acid, acid above it drains (the glands drink it).
    fn set_gullet_tide(&mut self, stamp: &living::LivingStamp, level: u32) {
        let bounds = stamp.bounds;
        // The basin: from the gullet's bottom up to its high water.
        let floor = bounds.max.y;
        let high = bounds.height() * TIDE_HIGH_PERMILLE / 1_000;
        #[allow(clippy::cast_possible_wrap, reason = "per mille")]
        let surface = floor - (high * level.min(1_000) as i32) / 1_000;
        let key = bounds.min;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "rows"
        )]
        let wanted = (floor - surface) as u16;
        if self.living_schedule.tides.get(&key).copied() == Some(wanted) {
            return;
        }
        self.living_schedule.tides.insert(key, wanted);
        let mut woke = false;
        for y in bounds.min.y..=floor {
            for x in bounds.min.x..=bounds.max.x {
                let position = TilePosition { x, y };
                let Some(tile) = self.terrain.tile(position) else {
                    continue;
                };
                if y > surface {
                    if tile.kind == TileKind::Air
                        && living::stamp_tile(stamp, x, y, TileKind::Stone) == Some(TileKind::Air)
                    {
                        let _ = self
                            .terrain
                            .set_kind(position, TileKind::Fluid(FluidKind::Acid));
                        woke = true;
                    }
                } else if tile.kind == TileKind::Fluid(FluidKind::Acid) {
                    let _ = self.terrain.set_kind(position, TileKind::Air);
                    woke = true;
                }
            }
        }
        if woke {
            self.wake_fluids(bounds.expanded(1));
        }
    }

    /// The Mind fell: the gate opens and every rig of the crew gets the Bio-Filter and Lamp II.
    pub(super) fn mind_fell(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        let nodes = std::mem::take(&mut self.living_dark.mind.nodes);
        for node in nodes {
            self.entities.despawn(crate::entity::EntityId(node));
        }
        self.release_possessed();
        self.clear_spore_clouds();
        let mind = &mut self.living_dark.mind;
        mind.beaten = true;
        mind.entity = None;
        mind.exposed_until = None;
        output.push_event(living_dark_event(LivingDarkEvent::MindDefeated, by, tile));
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in players {
            if let Some(player) = self.players.get_mut(&player_id) {
                crate::economy::grant_rig_system(player, RigSystem::BioFilter);
                crate::economy::grant_rig_system(player, RigSystem::LampII);
            }
            output.push_event(living_dark_event(
                LivingDarkEvent::GateRewards,
                Some(player_id),
                tile,
            ));
        }
    }

    /// Removes every spore cloud.
    pub(super) fn clear_spore_clouds(&mut self) {
        let ids: Vec<u32> = self.living_dark.spores.keys().copied().collect();
        for id in ids {
            let _ = self.fields.remove(id);
        }
        self.living_dark.spores.clear();
    }

    /// Frees every creature the Mind possessed.
    pub(super) fn release_possessed(&mut self) {
        let ids: Vec<crate::entity::EntityId> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| {
                matches!(entity.state, EntityState::Enemy(state)
                    if state.phase == crate::living_dark::POSSESSED_PHASE)
            })
            .map(|entity| entity.id)
            .collect();
        for id in ids {
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.phase = 0;
                    state.mode = EnemyMode::Return;
                    state.target = None;
                }
            });
        }
    }
}
