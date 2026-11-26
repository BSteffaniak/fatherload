//! The world rules of living bodies (Wave 12 bodylife, [`crate::bodylife`]).
//!
//! On a body's ground (a body store's ground pass, or a lone planet world):
//!
//! * **Climate replaces the home world's bands**: a rig's heat and cold, and a pilot's, read
//!   the body's temperature at its depth and hour ([`WorldState::body_heat_units_at`],
//!   [`WorldState::body_cold_at`]); the classic band heat and altitude cold never apply there.
//! * **Air**: a pilot breathes only where the body's air is breathable and the weather does not
//!   foul it; the rig's life support refills only there too ([`WorldState::body_breathable_at`]).
//! * **Weather** under the open sky: its wind carries an airborne rig and pilot (the rig
//!   through the same airborne-wind rule as the Inner Sky's, the pilot nudged where its box
//!   fits), and its hull and suit damage reach whatever stands outside
//!   ([`WorldState::body_weather_at`]). A cave, a dug shaft or an overhang shelters.
//! * **Events** (netcode 880-899): every seat on a body hears its weather turn
//!   (`BodyLifeEvent::WeatherTurned`) and the dawn and dusk (`Dawn`, `Dusk`) at its column.
//! * **Markets**: [`WorldState::sell_cargo_at_market`] sells a rig's minerals and artifacts at a
//!   body's local market (credits are universal; the home depot is refused away from home).
//!
//! A body's life is a pure function of the world seed and the body, memoized per process
//! ([`body_life_of`]): nothing here is saved or replicated beyond the events.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};

use crate::{
    bodylife::{BodyLife, BodyReading, LocalMarket, WeatherNow},
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    place::{SpaceBodyKey, StoreKey},
    planet::PlanetParams,
    summary::PlayerScopedCommandOutcome,
    terrain::TilePosition,
};

use super::WorldState;

/// What a body's life does that the game hears about (netcode 880-899).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum BodyLifeEvent {
    /// The weather at the player's place turned (detail: the new [`crate::bodylife::WeatherId`]).
    WeatherTurned,
    /// The sun rose at the player's column.
    Dawn,
    /// The sun set at the player's column.
    Dusk,
    /// The player sold at a body's market (detail: credits paid).
    Sold,
}

impl BodyLifeEvent {
    /// Wire code offset (netcode adds 880; 880-899 is this track's range).
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// Rows below the surface past which nothing of the sky reaches (deep ground is sheltered).
pub const SHELTER_DEPTH_ROWS: i32 = 40;
/// Most body lives memoized per process.
const LIFE_CACHE: usize = 64;

type LifeKey = (u64, Option<SpaceBodyKey>, PlanetParams);

static LIVES: Mutex<BTreeMap<LifeKey, Arc<BodyLife>>> = Mutex::new(BTreeMap::new());

/// The life of body `key` in the world of `seed` (or of the lone planet `params` when `key` is
/// `None`), memoized: a pure function of its inputs.
#[must_use]
pub fn body_life_of(
    seed: u64,
    key: Option<SpaceBodyKey>,
    params: PlanetParams,
) -> Option<Arc<BodyLife>> {
    let cache_key = (seed, key, params);
    let cached = LIVES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&cache_key)
        .cloned();
    if cached.is_some() {
        return cached;
    }
    let life = match key {
        Some(id) => {
            let body = crate::space::Space::shared(seed).body(id)?;
            BodyLife::of_space_body(id, &body, params)
        }
        None => BodyLife::of_planet(params),
    };
    let life = Arc::new(life);
    {
        let mut lives = LIVES.lock().unwrap_or_else(PoisonError::into_inner);
        if lives.len() >= LIFE_CACHE {
            lives.clear();
        }
        lives.insert(cache_key, Arc::clone(&life));
    }
    Some(life)
}

impl WorldState {
    /// The life of store `store`'s body (`None` for the home world, free flight and the flat
    /// test ground; a lone planet world's home store is its planet).
    #[must_use]
    pub fn body_life(&self, store: StoreKey) -> Option<Arc<BodyLife>> {
        match store {
            StoreKey::Body(key) => {
                let params = crate::place::planet_params_for(self.home_seed(), key)?;
                body_life_of(self.home_seed(), Some(key), params)
            }
            StoreKey::Home => {
                // A lone planet world: its terrain is the planet (outside any body pass).
                let terrain = self.store_terrain(StoreKey::Home)?;
                let params = *terrain.planet_ground()?.params();
                body_life_of(self.home_seed(), None, params)
            }
            StoreKey::Space => None,
        }
    }

    /// The life of the body whose ground is swapped in (inside a body store's ground pass, or
    /// a lone planet world).
    #[must_use]
    pub fn body_life_here(&self) -> Option<Arc<BodyLife>> {
        match self.active_store() {
            StoreKey::Body(key) => {
                let params = *self.terrain.planet_ground()?.params();
                body_life_of(self.home_seed(), Some(key), params)
            }
            StoreKey::Home => {
                let params = *self.terrain.planet_ground()?.params();
                body_life_of(self.home_seed(), None, params)
            }
            StoreKey::Space => None,
        }
    }

    /// The life of the body `player_id` is on, if any.
    #[must_use]
    pub fn body_life_of_player(&self, player_id: PlayerId) -> Option<Arc<BodyLife>> {
        self.body_life(self.store_of(player_id))
    }

    /// Whether the open sky is over strip tile `tile` of the ground swapped in: nothing solid
    /// between it and the top of the strip, and not deep underground.
    #[must_use]
    pub fn open_sky_at(&self, tile: TilePosition) -> bool {
        self.open_sky_in(&self.terrain, tile)
    }

    fn open_sky_in(&self, terrain: &crate::terrain::Terrain, tile: TilePosition) -> bool {
        let _ = self;
        let Some(planet) = terrain.planet_ground() else {
            return false;
        };
        let surface = crate::planet::BodyGround::surface_row(planet, tile.x);
        if tile.y - surface > SHELTER_DEPTH_ROWS {
            return false;
        }
        let top = planet.strip_top();
        (top..tile.y).all(|y| {
            terrain
                .tile(TilePosition { x: tile.x, y })
                .is_none_or(|found| found.kind.is_open())
        })
    }

    /// The body's reading (hour, light, weather, climate) at strip pixel `(x, y)` of the ground
    /// swapped in, if it is a body.
    #[must_use]
    pub fn body_reading_here(&self, x: f32, y: f32) -> Option<BodyReading> {
        let life = self.body_life_here()?;
        Some(self.reading_in(&life, &self.terrain, x, y))
    }

    fn reading_in(
        &self,
        life: &BodyLife,
        terrain: &crate::terrain::Terrain,
        x: f32,
        y: f32,
    ) -> BodyReading {
        let tile = terrain.frame().tile_at(x, y);
        let surface = terrain.surface_row(tile.x);
        let exposed = self.open_sky_in(terrain, tile);
        life.reading(
            self.world_millis(),
            i64::from(tile.x),
            tile.y,
            surface,
            exposed,
        )
    }

    /// The body reading where `player_id` is (its pilot on foot, else its rig), from any pass
    /// or outside the tick (the HUD's).
    #[must_use]
    pub fn body_reading_of(&self, player_id: PlayerId) -> Option<BodyReading> {
        let store = self.store_of(player_id);
        let life = self.body_life(store)?;
        let terrain = if store == self.active_store() {
            &self.terrain
        } else {
            self.store_terrain(store)?
        };
        let (x, y, _) = self.store_actor_position(player_id).or_else(|| {
            self.players
                .get(&player_id)
                .map(|player| (player.x, player.y, false))
        })?;
        Some(self.reading_in(&life, terrain, x, y))
    }

    /// The weather reaching strip pixel `(x, y)` of the ground swapped in (clear under cover
    /// or off a body).
    #[must_use]
    pub fn body_weather_at(&self, x: f32, y: f32) -> WeatherNow {
        self.body_reading_here(x, y)
            .map_or(WeatherNow::CLEAR, |reading| reading.weather)
    }

    /// The weather's wind for an airborne rig or pilot at `(x, y)` (`None` off a body, under
    /// cover or in still air).
    #[must_use]
    pub fn body_wind_at(&self, x: f32, y: f32) -> Option<f32> {
        let weather = self.body_weather_at(x, y);
        (weather.wind != 0.0).then_some(weather.wind)
    }

    /// Heat units at strip tile `tile` of the body swapped in (`None` off a body: the classic
    /// band heat applies).
    #[must_use]
    pub fn body_heat_units_at(&self, tile: TilePosition) -> Option<u8> {
        let (x, y) = self.frame().centre_of(tile.x, tile.y);
        self.body_reading_here(x, y)
            .map(|reading| reading.climate.heat_units)
    }

    /// Cold levels at strip tile `tile` of the body swapped in (`None` off a body).
    #[must_use]
    pub fn body_cold_at(&self, tile: TilePosition) -> Option<u8> {
        let (x, y) = self.frame().centre_of(tile.x, tile.y);
        self.body_reading_here(x, y)
            .map(|reading| reading.climate.cold)
    }

    /// Whether a pilot breathes at strip pixel `(x, y)` of the body swapped in (`None` off a
    /// body).
    #[must_use]
    pub fn body_breathable_at(&self, x: f32, y: f32) -> Option<bool> {
        self.body_reading_here(x, y)
            .map(|reading| reading.climate.breathable)
    }

    /// The body's gravity where `player_id` is (milli-g; `None` off a body): what the HUD
    /// shows and every falling thing on the ground feels.
    #[must_use]
    pub fn body_gravity_of(&self, player_id: PlayerId) -> Option<u16> {
        self.body_life_of_player(player_id)
            .map(|life| life.gravity_milli())
    }

    /// Weather damage to `player_id`'s rig standing under the open sky this tick (hull
    /// points, before the rig's mitigation).
    pub(super) fn body_weather_damage(&self, player_id: PlayerId, seconds: f32) -> f32 {
        if self.is_on_foot(player_id) {
            return 0.0;
        }
        let Some(player) = self.players.get(&player_id) else {
            return 0.0;
        };
        self.body_weather_at(player.x, player.y).hull_damage * seconds
    }

    /// The body's dawns, dusks and turning weather for the players on its ground (after the
    /// ground's other systems; edges between the tick's start and end time, so every tick
    /// rate hears each once).
    pub(super) fn advance_body_life(&self, output: &mut TickOutput) {
        let Some(life) = self.body_life_here() else {
            return;
        };
        // The edge between this tick's time and the next's.
        let before = self.world_millis();
        let now = self.clock_at(self.simulation_tick.next()).world_millis();
        for (&player_id, player) in &self.players {
            let (x, y) = self
                .store_actor_position(player_id)
                .map_or((player.x, player.y), |(x, y, _)| (x, y));
            let column = i64::from(self.frame().tile_at(x, y).x);
            let was = life.weather(before, column);
            let is = life.weather(now, column);
            if was.kind != is.kind {
                output.push_event(GameplayEventKind::BodyLife {
                    what: BodyLifeEvent::WeatherTurned,
                    player_id,
                    detail: u32::from(is.kind.0),
                });
            }
            let day = |millis: u64| life.clock.elevation(millis, column) > 0.0;
            match (day(before), day(now)) {
                (false, true) => output.push_event(GameplayEventKind::BodyLife {
                    what: BodyLifeEvent::Dawn,
                    player_id,
                    detail: 0,
                }),
                (true, false) => output.push_event(GameplayEventKind::BodyLife {
                    what: BodyLifeEvent::Dusk,
                    player_id,
                    detail: 0,
                }),
                _ => {}
            }
        }
    }

    /// The local market of settlement `settlement` at strip column `column` of `player_id`'s
    /// body now (`None` off a body).
    #[must_use]
    pub fn body_market(
        &self,
        player_id: PlayerId,
        settlement: u64,
        column: i32,
    ) -> Option<LocalMarket> {
        let life = self.body_life_of_player(player_id)?;
        Some(life.market(settlement, self.world_millis(), i64::from(column)))
    }

    /// Sells `player_id`'s minerals and artifacts at `market` (a settlement's or a station's
    /// counter on a body: TODO-SEAM w12-aliens / w12-stations call it from their counters).
    /// Credits are universal; the home world's premiums and rebates do not apply.
    pub fn sell_cargo_at_market(
        &mut self,
        player_id: PlayerId,
        market: &LocalMarket,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let minerals: u64 = player
            .cargo_bay
            .minerals()
            .map(|(mineral, count)| u64::from(market.mineral_price(mineral)) * u64::from(count))
            .sum();
        let artifacts: u64 = player
            .cargo_bay
            .artifacts()
            .map(|(artifact, count)| u64::from(market.artifact_price(artifact)) * u64::from(count))
            .sum();
        let payout = u32::try_from(minerals + artifacts).unwrap_or(u32::MAX);
        if payout == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.cargo_bay.clear_ore_and_artifacts();
        player.credits = player.credits.saturating_add(payout);
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(payout);
        output.push_event(GameplayEventKind::BodyLife {
            what: BodyLifeEvent::Sold,
            player_id,
            detail: payout,
        });
        PlayerScopedCommandOutcome::Applied
    }
}
