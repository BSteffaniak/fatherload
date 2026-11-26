//! The destination index on the world (Wave 14 W14-B, see [`crate::destination`]): where a
//! player is, the candidates of a kind ordered outward from there, the selectors, and travel
//! with safe arrival, bookmarks and the back-stack.
//!
//! Travel is a test-mode action. Inside the tick it is queued and run after the tick's last
//! pass, with the home store swapped in and every rig back in its place (so it can carve the
//! home ground, swap a body's ground in, or move between stores at once).

use crate::{
    destination::{
        self, ArrivalOptions, Destination, DestinationFilter, DestinationId, DestinationKind,
        HERE_TILES, MAX_CANDIDATES, PlayerTravel, Selector, TravelCursor, TravelEvent, TravelLog,
        TravelRequest,
        home::HomeQuery,
        space::{ARRIVAL_POINT, SURFACE, SpaceQuery},
    },
    entity::{EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    place::{BodyId, Place, StoreKey},
    position::WorldPos,
    space::{SpaceBodyId, SpaceTile},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Half-width in tiles of an arrival pocket (3 tiles wide).
const POCKET_HALF_WIDTH: i32 = 1;
/// Height in tiles of an arrival pocket.
const POCKET_HEIGHT: i32 = 2;
/// Tiles round the pocket whose hazards (lava, gas, pockets, vents, fluids) turn to rock.
const HAZARD_MARGIN: i32 = 3;
/// Columns either side an arrival looks for a spot clear of fixtures.
const SITE_SEARCH_COLUMNS: i32 = 160;
/// Rows an arrival in open air drops to find a floor.
const SETTLE_ROWS: i32 = 24;
/// Tiles round an arrival cleared of (non-boss) enemies.
const CLEAR_ENEMY_TILES: f32 = 24.0;
/// Health from which an enemy counts as a boss (arrival never despawns one).
const BOSS_HEALTH: f32 = 1_500.0;
/// Space tiles a player destination stops short of the player it names.
const BESIDE_PLAYER_TILES: i64 = 40;

/// A travel order queued in the tick (run after its last pass).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TravelOrder {
    To(TravelRequest),
    Back,
    BookmarkHere,
    Bookmark(DestinationId),
    RemoveBookmark(u8),
}

/// The ordering key of a candidate seen from an origin: (another place?, squared distance).
type OrderKey = (u8, i128);

impl WorldState {
    // --- State --------------------------------------------------------------------------------

    /// Every player's bookmarks, back-stack and cycle.
    #[must_use]
    pub const fn travel_log(&self) -> &TravelLog {
        &self.travel
    }

    /// Replaces the travel log (saves, tests, scenes).
    pub fn set_travel_log(&mut self, log: TravelLog) {
        self.travel = log;
    }

    /// `player_id`'s bookmarks, back-stack and cycle.
    #[must_use]
    pub fn player_travel(&self, player_id: PlayerId) -> PlayerTravel {
        self.travel.of(player_id)
    }

    // --- Where a player is --------------------------------------------------------------------

    /// Where `player_id` stands, as a destination id of kind `Recent` (the back-stack's
    /// entries): the home world's tile, a body's strip tile, or a space tile in flight.
    #[must_use]
    pub fn travel_here(&self, player_id: PlayerId) -> Option<DestinationId> {
        let rig = self.players.get(&player_id)?;
        let kind = DestinationKind::Recent;
        if rig.flight.away() {
            let tile = SpaceTile::of_pos(rig.flight.pos);
            return Some(DestinationId::new(kind, Place::Space, tile.x, tile.y));
        }
        let store = self.store_of(player_id);
        let (x, y, _) = self.store_actor_position(player_id)?;
        let tile = self.store_frame(store)?.tile_at(x, y);
        let place = if store.is_home() {
            if self.place_pass_store_is_home() && self.on_moon(tile) {
                Place::OnBody(BodyId::Moon)
            } else {
                Place::HOME
            }
        } else {
            self.recorded_place(player_id)
        };
        Some(DestinationId::new(
            kind,
            place,
            i64::from(tile.x),
            i64::from(tile.y),
        ))
    }

    /// Where `id` lies in space (its own tile in flight, its body's centre otherwise).
    fn space_anchor(&self, id: &DestinationId) -> SpaceTile {
        match id.place {
            Place::Space => SpaceTile::new(id.x, id.y),
            Place::OnBody(BodyId::Home) => SpaceTile::of_pos(crate::flight::space::home_centre()),
            Place::OnBody(BodyId::Moon) => SpaceTile::of_pos(crate::flight::space::moon_centre()),
            Place::OnBody(BodyId::Space(key)) => self
                .space()
                .space()
                .body(key)
                .map_or_else(SpaceTile::default, |body| body.centre),
        }
    }

    /// The ordering key of `candidate` seen from `origin`: in the same place, its tile
    /// distance; elsewhere, the space distance between the places.
    fn order_key(&self, origin: &DestinationId, candidate: &DestinationId) -> OrderKey {
        let same = origin.place.store() == candidate.place.store()
            && (origin.place != Place::Space || candidate.place == Place::Space);
        if same {
            let x = if candidate.x == ARRIVAL_POINT && candidate.place != Place::Space {
                origin.x
            } else {
                candidate.x
            };
            let y = if candidate.y == SURFACE || candidate.x == ARRIVAL_POINT {
                origin.y
            } else {
                candidate.y
            };
            return (0, destination::distance2((origin.x, origin.y), (x, y)));
        }
        let (from, to) = (self.space_anchor(origin), self.space_anchor(candidate));
        (1, destination::distance2((from.x, from.y), (to.x, to.y)))
    }

    /// Whether `key` is "here" (the same place, within [`HERE_TILES`]).
    const fn is_here(key: OrderKey) -> bool {
        key.0 == 0 && key.1 <= (HERE_TILES * HERE_TILES) as i128
    }

    // --- Candidates ---------------------------------------------------------------------------

    /// Every candidate of `kind` (narrowed by `filter`) ordered outward from where `player_id`
    /// stands, nearest first, at most [`MAX_CANDIDATES`], each with its distance on its card.
    /// A pure read: the Atlas lists and previews these.
    #[must_use]
    pub fn destinations(
        &self,
        player_id: PlayerId,
        kind: DestinationKind,
        filter: DestinationFilter,
    ) -> Vec<Destination> {
        let Some(origin) = self.travel_here(player_id) else {
            return Vec::new();
        };
        self.destinations_from(player_id, kind, filter, &origin)
    }

    /// [`Self::destinations`] ordered from `origin` instead.
    fn destinations_from(
        &self,
        player_id: PlayerId,
        kind: DestinationKind,
        filter: DestinationFilter,
        origin: &DestinationId,
    ) -> Vec<Destination> {
        let found = self.gather_destinations(player_id, kind, filter, origin);
        let mut keyed: Vec<(OrderKey, Destination)> = found
            .into_iter()
            .map(|mut candidate| {
                let key = self.order_key(origin, &candidate.id);
                candidate.info.distance = Some(destination::isqrt(key.1));
                (key, candidate)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        keyed.dedup_by(|a, b| a.1.id == b.1.id);
        keyed
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|(_, candidate)| candidate)
            .collect()
    }

    fn gather_destinations(
        &self,
        player_id: PlayerId,
        kind: DestinationKind,
        filter: DestinationFilter,
        origin: &DestinationId,
    ) -> Vec<Destination> {
        match kind {
            DestinationKind::Bookmark | DestinationKind::Recent => {
                let travel = self.travel.of(player_id);
                let list = if kind == DestinationKind::Bookmark {
                    travel.bookmarks
                } else {
                    travel.recent.into_iter().rev().collect()
                };
                list.into_iter()
                    .enumerate()
                    .map(|(index, id)| {
                        let mut found = Destination::new(
                            id.as_kind(kind),
                            format!("{} {}", kind.name(), index + 1),
                        );
                        self.describe_place(&mut found);
                        found
                    })
                    .collect()
            }
            DestinationKind::Player => self
                .players
                .keys()
                .copied()
                .filter(|other| *other != player_id)
                .filter(|other| match filter {
                    DestinationFilter::Player(only) => only == *other,
                    _ => true,
                })
                .filter_map(|other| {
                    let mut id = self.travel_here(other)?.as_kind(kind);
                    if id.place == Place::Space {
                        id.y -= BESIDE_PLAYER_TILES;
                    }
                    let mut found = Destination::new(id, format!("Player {}", other.get()));
                    self.describe_place(&mut found);
                    Some(found)
                })
                .collect(),
            _ if kind.in_space() => {
                if !self.space_enabled() {
                    return Vec::new();
                }
                let seed = self.world_seed().seed;
                let space = crate::space::Space::shared(self.world_seed());
                let charts = self.archive_chart_systems();
                let query = SpaceQuery {
                    space: &space,
                    seed,
                    from: self.space_anchor(origin),
                    charts: &charts,
                };
                destination::space::candidates(&query, kind, filter)
            }
            _ => {
                let settings = self.home_settings();
                let Some(terrain) = self.store_terrain(StoreKey::Home) else {
                    return Vec::new();
                };
                let frame = terrain.frame();
                let spawn = frame.tile_at(self.player_template.x, self.player_template.y);
                let anchor = if origin.place.is_home_store() {
                    TilePosition {
                        x: i32::try_from(origin.x).unwrap_or(spawn.x),
                        y: i32::try_from(origin.y).unwrap_or(spawn.y),
                    }
                } else {
                    spawn
                };
                let query = HomeQuery {
                    settings: &settings,
                    anchor,
                    spawn,
                    columns: (terrain.min_x(), terrain.max_x()),
                };
                destination::home::candidates(&query, kind, filter)
            }
        }
    }

    /// The systems the Archive's charts lead to: every chart the crew holds, else the ones the
    /// Archive's first plinth and the orrery's would chart.
    fn archive_chart_systems(&self) -> Vec<crate::space::SystemRef> {
        let seed = self.world_seed().seed;
        let space = crate::space::Space::shared(self.world_seed());
        let mut charts: Vec<crate::star_chart::StarChart> = self.star_charts().to_vec();
        if charts.is_empty() {
            let zero = crate::ids::SimulationTick::new(0);
            charts.push(crate::star_chart::chart_at_site(seed, 0, 0, 0, zero));
            let settings = self.home_settings();
            if settings.generator.has_descents() {
                let layout = crate::descent::fields::cached_layout(&settings);
                if let Some(orrery) =
                    crate::worldgen::descent::ancient_city::orrery(&settings, &layout)
                {
                    for plinth in crate::worldgen::descent::ancient_city::orrery_plinths(&orrery) {
                        charts.push(crate::star_chart::chart_at_site(
                            seed,
                            plinth.x,
                            plinth.y,
                            orrery.along,
                            zero,
                        ));
                    }
                }
            }
        }
        let mut systems: Vec<crate::space::SystemRef> = Vec::new();
        for chart in &charts {
            let system = space.chart_system(chart);
            if !systems.contains(&system) {
                systems.push(system);
            }
        }
        systems
    }

    /// Fills a bookmark's, a recent place's or a player's card from where it is.
    fn describe_place(&self, found: &mut Destination) {
        match found.id.place {
            Place::OnBody(BodyId::Home | BodyId::Moon) => {
                let settings = self.home_settings();
                let Some(terrain) = self.store_terrain(StoreKey::Home) else {
                    return;
                };
                let frame = terrain.frame();
                let spawn = frame.tile_at(self.player_template.x, self.player_template.y);
                let query = HomeQuery {
                    settings: &settings,
                    anchor: spawn,
                    spawn,
                    columns: (terrain.min_x(), terrain.max_x()),
                };
                destination::home::describe(&query, found);
            }
            Place::OnBody(BodyId::Space(key)) => {
                let space = self.space();
                if let (Some(system), Some(body)) = (
                    space.space().system(key.system_ref()),
                    space.space().body(key),
                ) {
                    found.info.place = system.body_name(body.index);
                    found.info.system = Some(system.name);
                    found.info.theme = Some(body.theme);
                    found.info.gravity_milli_g = Some(u32::from(body.gravity_milli_g));
                }
            }
            Place::Space => "free space".clone_into(&mut found.info.place),
        }
    }

    /// Every destination of every kind whose name, kind, theme, Descent, stratum, boss or
    /// residents hold every word of `query` ("planet ice", "leviathan", "descent 5"), nearest
    /// first within each kind, at most `limit`. A leading kind word narrows to that kind.
    #[must_use]
    pub fn search_destinations(
        &self,
        player_id: PlayerId,
        query: &str,
        limit: usize,
    ) -> Vec<Destination> {
        let words: Vec<&str> = query.split_whitespace().collect();
        let (kinds, rest): (Vec<DestinationKind>, String) = words
            .first()
            .and_then(|word| DestinationKind::parse(word))
            .map_or_else(
                || (DestinationKind::ALL.to_vec(), query.to_owned()),
                |kind| (vec![kind], words[1..].join(" ")),
            );
        let mut found = Vec::new();
        for kind in kinds {
            for candidate in self.destinations(player_id, kind, DestinationFilter::Any) {
                if candidate.matches(&rest) {
                    found.push(candidate);
                    if found.len() >= limit {
                        return found;
                    }
                }
            }
        }
        found
    }

    // --- Selectors ----------------------------------------------------------------------------

    /// What `request` would pick for `player_id` now (a pure read: the Atlas's preview of a
    /// travel), and the next / previous cycle after it.
    #[must_use]
    pub fn resolve_travel(
        &self,
        player_id: PlayerId,
        request: &TravelRequest,
    ) -> Option<(Destination, Option<TravelCursor>)> {
        let here = self.travel_here(player_id)?;
        match request.selector {
            Selector::Exact(id) => {
                let listed = self
                    .destinations_from(player_id, id.kind, request.filter, &here)
                    .into_iter()
                    .find(|candidate| candidate.id == id);
                Some((
                    listed.unwrap_or_else(|| {
                        let mut found = Destination::new(id, id.kind.name());
                        self.describe_place(&mut found);
                        found
                    }),
                    None,
                ))
            }
            Selector::Nearest => {
                let list = self.destinations_from(player_id, request.kind, request.filter, &here);
                let pick = list
                    .iter()
                    .position(|candidate| !Self::is_here(self.order_key(&here, &candidate.id)))
                    .unwrap_or(0);
                list.into_iter().nth(pick).map(|found| (found, None))
            }
            Selector::Random(seed) => {
                let list = self.destinations_from(player_id, request.kind, request.filter, &here);
                if list.is_empty() {
                    return None;
                }
                // The player's travel count is mixed in, so asking for "a random planet" again
                // with the same seed (a console's `tp planet random`) picks afresh; the preview
                // and the travel it previews still agree (the count changes only on arrival).
                let travels = self
                    .travel
                    .players
                    .get(&player_id)
                    .map_or(0, |travel| travel.travels);
                let roll = crate::worldgen::noise::hash2(
                    self.world_seed().seed,
                    0x5714_B0B0 ^ (u64::from(travels) << 32),
                    i32::try_from(request.kind.code()).unwrap_or(0),
                    i32::from_ne_bytes(seed.to_ne_bytes()),
                );
                let index = usize::try_from(roll % list.len() as u64).unwrap_or(0);
                list.into_iter().nth(index).map(|found| (found, None))
            }
            Selector::Next | Selector::Previous => {
                let cursor = self
                    .travel
                    .players
                    .get(&player_id)
                    .and_then(|travel| travel.cursor)
                    .filter(|cursor| {
                        cursor.kind == request.kind && cursor.filter == request.filter
                    });
                let origin = cursor.map_or(here, |cursor| cursor.origin);
                let list = self.destinations_from(player_id, request.kind, request.filter, &origin);
                if list.is_empty() {
                    return None;
                }
                let len = list.len();
                let forward = request.selector == Selector::Next;
                let index = match cursor {
                    Some(cursor) => {
                        let at = usize::from(cursor.index) % len;
                        if forward {
                            (at + 1) % len
                        } else {
                            (at + len - 1) % len
                        }
                    }
                    None if forward => list
                        .iter()
                        .position(|candidate| {
                            !Self::is_here(self.order_key(&origin, &candidate.id))
                        })
                        .unwrap_or(0),
                    None => len - 1,
                };
                let next = TravelCursor {
                    kind: request.kind,
                    filter: request.filter,
                    origin,
                    index: u16::try_from(index).unwrap_or(0),
                };
                list.into_iter().nth(index).map(|found| (found, Some(next)))
            }
        }
    }

    // --- Actions ------------------------------------------------------------------------------

    /// Queues a travel order (test mode only; see the module docs).
    pub(super) fn order_travel(
        &mut self,
        player_id: PlayerId,
        order: TravelOrder,
    ) -> PlayerScopedCommandOutcome {
        if !self.is_test_mode() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        if !self.player_in_world(player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        self.queue_travel(player_id, order);
        PlayerScopedCommandOutcome::Applied
    }

    /// Runs the travel orders queued this tick (after its last pass).
    pub(super) fn run_travel_orders(&mut self, output: &mut TickOutput) {
        let orders = self.take_travel_orders();
        if orders.is_empty() {
            return;
        }
        // The pockets carved in the home ground travel with this tick's home tile ops.
        self.terrain.start_journal();
        self.run_orders(orders, output);
        let carved = self.terrain.finish_journal();
        self.wake_entities_near(&carved);
        output.terrain_ops.extend(carved);
    }

    fn run_orders(&mut self, orders: Vec<(PlayerId, TravelOrder)>, output: &mut TickOutput) {
        for (player_id, order) in orders {
            if !self.players.contains_key(&player_id) || !self.is_test_mode() {
                continue;
            }
            let from = self.place_of(player_id);
            let event = match order {
                TravelOrder::To(request) => Some(self.travel_to(player_id, &request)),
                TravelOrder::Back => Some(self.travel_back(player_id)),
                TravelOrder::BookmarkHere => self.travel_here(player_id).map(|here| {
                    self.travel.of_mut(player_id).bookmark(here);
                    (TravelEvent::Bookmarked, self.bookmark_count(player_id))
                }),
                TravelOrder::Bookmark(id) => {
                    self.travel.of_mut(player_id).bookmark(id);
                    Some((TravelEvent::Bookmarked, self.bookmark_count(player_id)))
                }
                TravelOrder::RemoveBookmark(index) => {
                    let travel = self.travel.of_mut(player_id);
                    let index = usize::from(index);
                    (index < travel.bookmarks.len()).then(|| {
                        travel.bookmarks.remove(index);
                        let kept = u32::try_from(travel.bookmarks.len()).unwrap_or(u32::MAX);
                        (TravelEvent::BookmarkRemoved, kept)
                    })
                }
            };
            let to = self.place_of(player_id);
            if from != to {
                output.push_event(GameplayEventKind::PlaceChanged {
                    player_id,
                    from,
                    to,
                });
                let at = output.events.len() - 1;
                output.stamp_stores(at, |_| to.store());
            }
            if let Some((what, detail)) = event {
                output.push_event(GameplayEventKind::Travel {
                    what,
                    player_id,
                    detail,
                });
                let at = output.events.len() - 1;
                output.stamp_stores(at, |_| to.store());
            }
        }
    }

    fn bookmark_count(&self, player_id: PlayerId) -> u32 {
        u32::try_from(self.travel.of(player_id).bookmarks.len()).unwrap_or(u32::MAX)
    }

    /// Travels now (outside the tick: tools, scenes, tests and `--start-at`). Returns where the
    /// rig arrived, `None` when nothing was found or the world is not in test mode.
    pub fn stage_travel(
        &mut self,
        player_id: PlayerId,
        request: &TravelRequest,
    ) -> Option<Destination> {
        if !self.is_test_mode() || self.place_pass_running() {
            return None;
        }
        let (found, _) = self.resolve_travel(player_id, request)?;
        (self.travel_to(player_id, request).0 == TravelEvent::Arrived).then_some(found)
    }

    fn travel_to(&mut self, player_id: PlayerId, request: &TravelRequest) -> (TravelEvent, u32) {
        let not_found = (TravelEvent::NotFound, request.kind.code());
        let Some(here) = self.travel_here(player_id) else {
            return not_found;
        };
        let Some((found, cursor)) = self.resolve_travel(player_id, request) else {
            return not_found;
        };
        if !self.arrive(player_id, &found, request.arrival) {
            return not_found;
        }
        let travel = self.travel.of_mut(player_id);
        travel.push_recent(here);
        travel.travels = travel.travels.wrapping_add(1);
        match request.selector {
            Selector::Next | Selector::Previous => travel.cursor = cursor,
            _ => travel.cursor = None,
        }
        (TravelEvent::Arrived, request.kind.code())
    }

    fn travel_back(&mut self, player_id: PlayerId) -> (TravelEvent, u32) {
        let Some(last) = self.travel.of_mut(player_id).recent.pop() else {
            return (TravelEvent::NotFound, DestinationKind::Recent.code());
        };
        let found = Destination::new(last, "back");
        if !self.arrive(player_id, &found, ArrivalOptions::default()) {
            return (TravelEvent::NotFound, DestinationKind::Recent.code());
        }
        let left = u32::try_from(self.travel.of(player_id).recent.len()).unwrap_or(0);
        (TravelEvent::WentBack, left)
    }

    // --- Safe arrival -------------------------------------------------------------------------

    /// Moves `player_id`'s rig to `found` with safe arrival. Returns whether it moved.
    fn arrive(
        &mut self,
        player_id: PlayerId,
        found: &Destination,
        options: ArrivalOptions,
    ) -> bool {
        let inside = if options.in_arena { found.inside } else { None };
        let (x, y) = inside.unwrap_or((found.id.x, found.id.y));
        let arrived = match found.id.place {
            Place::OnBody(BodyId::Home | BodyId::Moon) => {
                let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
                    return false;
                };
                self.arrive_home(player_id, x, y, options)
            }
            Place::OnBody(BodyId::Space(key)) => self.arrive_on_body(player_id, key, x, y, options),
            Place::Space => {
                self.warp_into_flight(player_id, SpaceTile::new(x, y).pos())
                    == PlayerScopedCommandOutcome::Applied
            }
        };
        if arrived && let Some(rig) = self.players.get_mut(&player_id) {
            // A ground arrival stands level and still; in flight the rig keeps its heading
            // (rotation is the player's) but stops spinning.
            if found.id.place == Place::Space {
                rig.attitude.angular_velocity = 0.0;
            } else {
                rig.attitude = crate::attitude::RigAttitude::LEVEL;
            }
            rig.velocity_x = 0.0;
            rig.velocity_y = 0.0;
            let air = self.air_capacity_of(player_id);
            if let Some(rig) = self.players.get_mut(&player_id) {
                rig.flight.air = rig.flight.air.max(air);
            }
        }
        arrived
    }

    /// Onto a body: its arrival point, or the surface (or a floor) at a strip column, or
    /// inside its lair.
    fn arrive_on_body(
        &mut self,
        player_id: PlayerId,
        body: SpaceBodyId,
        x: i64,
        y: i64,
        options: ArrivalOptions,
    ) -> bool {
        let place = Place::OnBody(BodyId::Space(body));
        let arrived = if x == ARRIVAL_POINT {
            self.warp_to_place(player_id, place) == PlayerScopedCommandOutcome::Applied
        } else {
            let base = (y != SURFACE).then(|| i32::try_from(y + 2).unwrap_or(0));
            self.warp_onto_body(player_id, body, u32::try_from(x.max(0)).unwrap_or(0), base)
                == PlayerScopedCommandOutcome::Applied
        };
        if arrived && !options.in_arena {
            let store = place.store();
            let _ = self.with_store_ground(store, |world| {
                if let Some((px, py)) = world.players.get(&player_id).map(|rig| (rig.x, rig.y)) {
                    world.clear_arrival_enemies(px, py);
                }
            });
        }
        arrived
    }

    /// Onto the home world (or the Moon) at tile `(x, y)`: settled onto a floor, in a freshly
    /// carved pocket clear of hazards, open water and enemies.
    fn arrive_home(
        &mut self,
        player_id: PlayerId,
        x: i32,
        y: i32,
        options: ArrivalOptions,
    ) -> bool {
        let away = self.store_of(player_id) != StoreKey::Home
            || self
                .players
                .get(&player_id)
                .is_some_and(|rig| rig.flight.away());
        if away && self.warp_to_place(player_id, Place::HOME) != PlayerScopedCommandOutcome::Applied
        {
            return false;
        }
        let column = x.clamp(self.terrain.min_x() + 4, self.terrain.max_x() - 4);
        let row = self.settle_row(column, y);
        let Some((column, top)) = self.arrival_site(column, row - (POCKET_HEIGHT - 1)) else {
            return false;
        };
        self.carve_arrival_pocket(column, top, options);
        let frame = self.frame();
        #[allow(
            clippy::cast_precision_loss,
            reason = "small tile coordinates convert to pixels"
        )]
        let local = (
            TILE_SIZE.mul_add(frame.rel_col(column), TILE_SIZE * 0.5),
            TILE_SIZE.mul_add(
                frame.rel_row(top + POCKET_HEIGHT),
                -(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
            ),
        );
        if !options.in_arena {
            self.clear_arrival_enemies(local.0, local.1);
        }
        let position: WorldPos = frame.world(local.0, local.1);
        self.set_active_drill(player_id, None);
        self.deep_run_states.remove(&player_id);
        if self
            .move_to_place(player_id, Place::HOME, position)
            .is_err()
        {
            return false;
        }
        if let Some(rig) = self.players.get_mut(&player_id) {
            rig.x_remainder = 0.0;
            rig.flight.place = crate::flight::FlightPlace::Home;
            rig.flight.velocity_x = 0.0;
            rig.flight.velocity_y = 0.0;
        }
        true
    }

    /// The row a rig arriving at `(column, row)` stands on: dropped through open air onto the
    /// first floor within [`SETTLE_ROWS`] (it stays where it is in rock or with no floor near).
    fn settle_row(&self, column: i32, row: i32) -> i32 {
        let open = |y: i32| {
            self.terrain
                .tile(TilePosition { x: column, y })
                .is_none_or(|tile| matches!(tile.kind, TileKind::Air | TileKind::Fluid(_)))
        };
        if !open(row) {
            return row;
        }
        (row..row + SETTLE_ROWS)
            .find(|y| !open(y + 1))
            .unwrap_or(row)
    }

    /// The column nearest `column` whose pocket at `top` (with a margin) holds no fixtures
    /// (points of interest, foundations, buildings, bedrock), and that top.
    fn arrival_site(&self, column: i32, top: i32) -> Option<(i32, i32)> {
        let margin = POCKET_HALF_WIDTH + 2;
        let (first, last) = (self.terrain.min_x() + margin, self.terrain.max_x() - margin);
        if last < first || top + POCKET_HEIGHT + 1 >= self.terrain.height() {
            return None;
        }
        let start = column.clamp(first, last);
        (0..=SITE_SEARCH_COLUMNS)
            .flat_map(|offset| [start - offset, start + offset])
            .filter(|candidate| (first..=last).contains(candidate))
            .find(|candidate| {
                Self::arrival_area(*candidate, top, 1).all(|position| {
                    !matches!(
                        self.terrain.tile(position).map(|tile| tile.kind),
                        Some(
                            TileKind::Poi(_)
                                | TileKind::Foundation
                                | TileKind::Bedrock
                                | TileKind::Built(_)
                        )
                    )
                })
            })
            .map(|candidate| (candidate, top))
    }

    /// The pocket at `(column, top)` with a `margin` all round.
    fn arrival_area(column: i32, top: i32, margin: i32) -> impl Iterator<Item = TilePosition> {
        let columns = column - POCKET_HALF_WIDTH - margin..=column + POCKET_HALF_WIDTH + margin;
        let rows = top - margin..=top + POCKET_HEIGHT + margin;
        rows.flat_map(move |y| columns.clone().map(move |x| TilePosition { x, y }))
    }

    /// Carves the arrival pocket: open air inside (open water kept "in the water"), a floor of
    /// the stratum's rock under it, and every hazard within [`HAZARD_MARGIN`] made rock.
    fn carve_arrival_pocket(&mut self, column: i32, top: i32, options: ArrivalOptions) {
        let floor = top + POCKET_HEIGHT;
        let rock = self.arrival_rock(floor);
        for position in Self::arrival_area(column, top, HAZARD_MARGIN) {
            let kind = self.terrain.tile(position).map(|tile| tile.kind);
            let in_columns = (position.x - column).abs() <= POCKET_HALF_WIDTH;
            let inside = in_columns && (top..floor).contains(&position.y);
            let fluid = matches!(kind, Some(TileKind::Fluid(_)));
            let replacement = if inside {
                (kind != Some(TileKind::Air) && !(fluid && options.in_water))
                    .then_some(TileKind::Air)
            } else if in_columns && position.y == floor {
                (!kind.is_some_and(|kind| {
                    matches!(
                        kind,
                        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
                    ) || kind == rock
                }))
                .then_some(rock)
            } else {
                kind.is_some_and(|kind| {
                    matches!(
                        kind,
                        TileKind::Lava
                            | TileKind::Gas
                            | TileKind::ExplosivePocket
                            | TileKind::PressurePocket
                            | TileKind::MagmaVent
                    ) || (matches!(kind, TileKind::Fluid(_)) && !options.in_water)
                })
                .then_some(rock)
            };
            if let Some(replacement) = replacement {
                let _written = self.terrain.set_kind(position, replacement);
            }
        }
    }

    /// The plain rock an arrival pocket's floor and plugs are made of at `row`.
    fn arrival_rock(&self, row: i32) -> TileKind {
        let settings = self.terrain.settings();
        if settings.generator.has_descents() && row > crate::worldgen::strata::SKY_ROWS {
            crate::descent::fields::cached_layout(settings)
                .stratum_of_row(row)
                .def()
                .soft
        } else {
            TileKind::Stone
        }
    }

    /// Despawns every enemy but a boss within [`CLEAR_ENEMY_TILES`] of local point `(x, y)`
    /// of the ground swapped in.
    fn clear_arrival_enemies(&mut self, x: f32, y: f32) {
        let reach = CLEAR_ENEMY_TILES * TILE_SIZE;
        let doomed: std::collections::BTreeSet<crate::entity::EntityId> = self
            .entities
            .of_kind(EntityKind::Enemy)
            .filter(|entity| {
                let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                let near = dx.mul_add(dx, dy * dy) <= reach * reach;
                let boss = matches!(&entity.state, EntityState::Enemy(enemy)
                    if enemy.kind.stats().health >= BOSS_HEALTH || enemy.max_health >= BOSS_HEALTH);
                near && !boss
            })
            .map(|entity| entity.id)
            .collect();
        if !doomed.is_empty() {
            let _gone = self.entities.despawn_many(&doomed);
        }
    }
}
