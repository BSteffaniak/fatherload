//! Test-mode travel and stocking for the Wave 14 command palette (see
//! [`crate::test_mode::travel`] and [`crate::test_mode::stock`]).
//!
//! **TODO-SEAM(w14-index, w14-stock):** the console track's stub resolver and stub stock
//! handler. The integrator swaps [`WorldState::resolve_destination`]'s body for the index's
//! resolver (same answer type: name, detail, where it lands) and routes
//! [`StockRequest`] to w14-stock's actions; callers do not change.

use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    descent::{Descent, Stratum},
    ids::PlayerId,
    place::{BodyId, Place, SpaceBodyKey},
    position::WorldPos,
    space::{BodyKind, HOME_SECTOR, Space, SpaceSector, Theme},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    test_mode::{
        palette_stock::StockRequest,
        travel::{Arrival, ArrivalTarget, Destination},
    },
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Rings of sectors round the traveller searched for planets (7 x 7 sectors).
const PLANET_SEARCH_RINGS: i32 = 3;
/// Rows under a stratum's (or Descent's) top a travel pocket opens at.
const ROWS_UNDER_TOP: i32 = 6;
/// Rows above a Descent's floor a boss travel opens at (the gate's neighbourhood).
const ROWS_ABOVE_FLOOR: i32 = 40;
/// How far either side of the rig a travel looks for a column clear of fixtures.
const SEARCH_COLUMNS: i32 = 48;
/// Half-width in tiles of a travel pocket (3 tiles wide).
const POCKET_HALF_WIDTH: i32 = 1;
/// Height in tiles of a travel pocket.
const POCKET_HEIGHT: i32 = 2;

/// One landable planet near a sector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlanetCandidate {
    id: SpaceBodyKey,
    theme: Theme,
    ring: i32,
}

type PlanetCache = Mutex<
    Vec<(
        (crate::space::SpaceSeed, SpaceSector),
        Arc<Vec<PlanetCandidate>>,
    )>,
>;

/// The landable planets round `centre`, ring by ring outward, in a fixed order (memoized per
/// seed and sector: pure generation).
fn planets_near(seed: crate::space::SpaceSeed, centre: SpaceSector) -> Arc<Vec<PlanetCandidate>> {
    static CACHE: PlanetCache = Mutex::new(Vec::new());
    if let Some((_, found)) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(key, _)| *key == (seed, centre))
    {
        return Arc::clone(found);
    }
    let space = Space::shared(seed);
    let mut out = Vec::new();
    for ring in 0..=PLANET_SEARCH_RINGS {
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = SpaceSector::new(centre.x + x, centre.y + y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        if body.kind != BodyKind::Planet || !Theme::PLANET.contains(&body.theme) {
                            continue;
                        }
                        let id = SpaceBodyKey {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if crate::place::planet_params_for(seed, id).is_some() {
                            out.push(PlanetCandidate {
                                id,
                                theme: body.theme,
                                ring,
                            });
                        }
                    }
                }
            }
        }
    }
    let found = Arc::new(out);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 16 {
        cache.clear();
    }
    cache.push(((seed, centre), Arc::clone(&found)));
    found
}

impl WorldState {
    /// Where `destination` would take `player_id`'s rig, or `None` when it has no answer in
    /// this world (no Descents on an old world, no planet of that theme nearby, a station
    /// before the index lands). Pure: the palette previews with it, the travel action lands
    /// with it.
    #[must_use]
    pub fn resolve_destination(
        &self,
        player_id: PlayerId,
        destination: Destination,
    ) -> Option<Arrival> {
        let home_row = |row: i32, name: String, detail: String| Arrival {
            name,
            detail,
            target: ArrivalTarget::HomeRow { row },
        };
        match destination {
            Destination::Descent(descent) => {
                let layout = self.travel_layout()?;
                let (top, _) = layout.descent_rows(descent);
                let row = top + ROWS_UNDER_TOP;
                Some(home_row(
                    row,
                    format!("Descent {}: {}", descent.numeral(), descent.name()),
                    format!(
                        "{} m down, {}",
                        self.metres_below_ground(row),
                        descent.first_stratum().name()
                    ),
                ))
            }
            Destination::Stratum(stratum) => {
                let layout = self.travel_layout()?;
                let row = layout.rows(stratum).0 + ROWS_UNDER_TOP;
                Some(home_row(
                    row,
                    stratum.name().to_owned(),
                    format!(
                        "{} m down, Descent {}",
                        self.metres_below_ground(row),
                        stratum.descent().numeral()
                    ),
                ))
            }
            Destination::Boss(boss) => {
                let descent = Descent::ALL
                    .into_iter()
                    .find(|descent| descent.gate().is_some_and(|gate| gate.boss == boss))?;
                let layout = self.travel_layout()?;
                let row = layout.descent_rows(descent).1 - ROWS_ABOVE_FLOOR;
                Some(home_row(
                    row,
                    boss.name().to_owned(),
                    format!(
                        "{} m down, the gate of Descent {}",
                        self.metres_below_ground(row),
                        descent.numeral()
                    ),
                ))
            }
            Destination::Depth(rows) => {
                let rows = rows.max(1);
                let column = self.travel_column(player_id);
                let ground = crate::worldgen::surface_row(&self.home_settings(), column) - 1;
                let row = ground + rows;
                let stratum = self
                    .travel_layout()
                    .map(|layout| layout.stratum_of_row(row).name().to_owned())
                    .unwrap_or_default();
                Some(home_row(row, format!("{rows} m down"), stratum))
            }
            Destination::Home => Some(Arrival {
                name: "Home".to_owned(),
                detail: "the spawn point".to_owned(),
                target: ArrivalTarget::Place(Place::HOME),
            }),
            Destination::Moon => Some(Arrival {
                name: "The Moon".to_owned(),
                detail: "its top".to_owned(),
                target: ArrivalTarget::Place(Place::OnBody(BodyId::Moon)),
            }),
            Destination::Planet { theme, pick } => self.resolve_planet(player_id, theme, pick),
            // TODO-SEAM(w14-index): stations (and every other space kind) come with the index.
            Destination::Station { .. } => None,
            Destination::Player(other) => {
                let (place, pos) = self.rig_world_pos(other)?;
                let (column, row) = pos.tile();
                Some(Arrival {
                    name: format!("Player {}", other.get()),
                    detail: format!("beside their rig ({column}, {row})"),
                    target: ArrivalTarget::Spot {
                        place,
                        column,
                        row: row - 2,
                    },
                })
            }
            Destination::Spot { place, column, row } => Some(Arrival {
                name: "A saved spot".to_owned(),
                detail: format!("({column}, {row})"),
                target: ArrivalTarget::Spot { place, column, row },
            }),
        }
    }

    /// A planet with `theme` (any when `None`) round the traveller's sector.
    fn resolve_planet(
        &self,
        player_id: PlayerId,
        theme: Option<Theme>,
        pick: crate::test_mode::travel::Pick,
    ) -> Option<Arrival> {
        if !self.space_enabled() {
            return None;
        }
        let centre = match self.place_of(player_id) {
            Place::OnBody(BodyId::Space(key)) => key.sector,
            _ => HOME_SECTOR,
        };
        let planets = planets_near(self.world_seed(), centre);
        let matching: Vec<&PlanetCandidate> = planets
            .iter()
            .filter(|planet| theme.is_none_or(|theme| planet.theme == theme))
            .collect();
        let index = pick.index(matching.len())?;
        let planet = matching[index];
        let system = Space::shared(self.world_seed())
            .sector(planet.id.sector)
            .systems
            .get(usize::from(planet.id.system))
            .map(|system| system.name.clone())
            .unwrap_or_default();
        Some(Arrival {
            name: format!(
                "{} planet {} of {system}",
                planet.theme.name(),
                planet.id.body
            ),
            detail: format!(
                "{} of {} ({} sectors out)",
                index + 1,
                matching.len(),
                planet.ring
            ),
            target: ArrivalTarget::Place(Place::OnBody(BodyId::Space(planet.id))),
        })
    }

    /// The home world's Descent layout (Deep Descent worlds only).
    pub(super) fn travel_layout(&self) -> Option<crate::worldgen::descent::DescentLayout> {
        let settings = self.home_settings();
        settings
            .generator
            .has_descents()
            .then(|| crate::descent::fields::cached_layout(&settings))
    }

    /// Metres (rows) `row` lies under the ground at the spawn column.
    fn metres_below_ground(&self, row: i32) -> i32 {
        let settings = self.home_settings();
        #[allow(clippy::cast_possible_truncation, reason = "tile columns are small")]
        let column = (self.player_template.x / TILE_SIZE).floor() as i32;
        row - (crate::worldgen::surface_row(&settings, column) - 1)
    }

    /// The column a home-world travel opens at: the rig's own while it is at home, else the
    /// spawn column.
    fn travel_column(&self, player_id: PlayerId) -> i32 {
        if self.travel_ready_at_home(player_id)
            && let Some(player) = self.players.get(&player_id)
        {
            return player.tile_in(self.frame()).x;
        }
        #[allow(clippy::cast_possible_truncation, reason = "tile columns are small")]
        let column = (self.player_template.x / TILE_SIZE).floor() as i32;
        column
    }

    /// Whether `player_id`'s rig stands in the home store on the ground (not in free flight)
    /// while the home store's pass runs: a home-world pocket can be carved for it.
    fn travel_ready_at_home(&self, player_id: PlayerId) -> bool {
        self.active_store().is_home()
            && self.store_of(player_id).is_home()
            && self
                .players
                .get(&player_id)
                .is_some_and(|player| !player.flight.away())
    }

    /// The test-mode travel (`TestModeAction::PaletteTravel`).
    pub(super) fn palette_travel_to(
        &mut self,
        player_id: PlayerId,
        destination: Destination,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        let Some(arrival) = self.resolve_destination(player_id, destination) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match arrival.target {
            ArrivalTarget::HomeRow { row } => {
                if !self.travel_ready_at_home(player_id) {
                    // TODO-SEAM(w14-index): the index's safe arrival lands from anywhere; the
                    // palette brings the rig home first and travels again.
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                self.travel_to_home_row(player_id, row)
            }
            ArrivalTarget::Place(place) => self.warp_to_place(player_id, place),
            ArrivalTarget::Spot { place, column, row } => {
                if place == Place::Space {
                    return self.warp_to_place(player_id, place);
                }
                self.arrive_at(player_id, place, WorldPos::tile_center(column, row))
            }
        }
    }

    /// Carves a safe pocket at the top `row` near the rig's column and puts the rig there.
    fn travel_to_home_row(&mut self, player_id: PlayerId, row: i32) -> PlayerScopedCommandOutcome {
        let column = self.travel_column(player_id);
        let Some(column) = self.row_pocket_column(row, column) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let band = self
            .travel_layout()
            .map_or(crate::worldgen::Band::Stone, |layout| {
                Stratum::host_band(layout.stratum_of_row(row))
            });
        self.carve_warp_pocket(band, column, row);
        #[allow(
            clippy::cast_precision_loss,
            reason = "small tile coordinates convert to pixels"
        )]
        let (x, y) = (
            TILE_SIZE.mul_add(self.frame().rel_col(column), TILE_SIZE * 0.5),
            TILE_SIZE.mul_add(
                self.frame().rel_row(row + POCKET_HEIGHT),
                -(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
            ),
        );
        self.set_active_drill(player_id, None);
        self.deep_run_states.remove(&player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.x_remainder = 0.0;
            player.y = y;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// The column nearest `column` where a pocket with its top at `row` holds no fixtures.
    fn row_pocket_column(&self, row: i32, column: i32) -> Option<i32> {
        let margin = POCKET_HALF_WIDTH + 2;
        let first = self.terrain.min_x() + margin;
        let last = self.terrain.max_x() - margin;
        if last < first || row < 1 || row + POCKET_HEIGHT + 1 >= self.terrain.height() {
            return None;
        }
        let start = column.clamp(first, last);
        (0..=SEARCH_COLUMNS)
            .flat_map(|offset| [start - offset, start + offset])
            .filter(|candidate| (first..=last).contains(candidate))
            .find(|candidate| {
                Self::pocket_area(*candidate, row).all(|position: TilePosition| {
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
    }

    /// The palette's stocking and progress switches (`TestModeAction::PaletteStock`).
    ///
    /// TODO-SEAM(w14-stock): w14-stock's actions replace this stub.
    pub(super) fn apply_stock_request(
        &mut self,
        player_id: PlayerId,
        request: StockRequest,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        match request {
            StockRequest::GiveAll => {
                self.stock_player(player_id);
                self.progression
                    .rig_part_inventory
                    .extend(crate::rig::RigPartKind::ALL);
                PlayerScopedCommandOutcome::Applied
            }
            StockRequest::Give { item, count } => {
                if item.stowage() != crate::item::Stowage::CargoBay {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.cargo_bay.add(item, count);
                }
                PlayerScopedCommandOutcome::Applied
            }
            StockRequest::EndingDone => self.grant_star_drive(player_id),
            StockRequest::Spawn(kind) => {
                let Some(player) = self.players.get(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                let (x, y) = (TILE_SIZE.mul_add(4.0, player.x), player.y);
                let _spawned = self.spawn_enemy(kind, x, y);
                PlayerScopedCommandOutcome::Applied
            }
            StockRequest::GiveSupply { .. }
            | StockRequest::GateAll
            | StockRequest::Gate(_)
            | StockRequest::ClearEnemies
            | StockRequest::FreezeAi(_)
            | StockRequest::SetTime { .. }
            | StockRequest::SetWeather(_) => PlayerScopedCommandOutcome::IgnoredUnavailable,
        }
    }
}
