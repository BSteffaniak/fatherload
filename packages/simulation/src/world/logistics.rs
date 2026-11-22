//! Depth logistics in the tick (Part III F10, see [`crate::logistics`]): lift rides, stations,
//! ancient lift restoration, outposts and their counters, beacons and travel, dive charges,
//! the Descent gates and the rig systems' first effects.
//!
//! Every command goes through [`WorldState::check_logistics`] (the verdict and price the UI
//! shows) and then applies inside the tick, so it is host-authoritative and the same online,
//! in split-screen and by touch.

use crate::{
    base::BuiltTile,
    depth_progression::{
        BIO_FILTER_GAS_PERCENT, DescentLayout, GATE_DAMAGE_PER_SECOND, ProgressionRules, RigSystem,
        SONAR_SUITE_SCAN_BONUS_TILES, layout_for, missing_gate,
    },
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    logistics::{
        ANCIENT_LIFT_RESTORE_PRICE, BEACON_MIN_DEPTH_ROWS, BEACON_PRICE, BEACON_SPACING_TILES,
        DIVE_CHARGE_PACK, DIVE_CHARGE_ROWS, DIVE_KICK_SPEED, DIVE_MAX_MILLIS, DIVE_SETTLE_MILLIS,
        DepthBeacon, DiveState, LIFT_STATION_PRICE, LiftOrigin, LiftRide, LiftShaft, Logistics,
        LogisticsCommand, MAX_BEACONS_PER_PLAYER, MAX_BUILT_LIFTS_PER_PLAYER, MAX_DIVE_CHARGES,
        MAX_LIFT_STATIONS, OUTPOST_FUEL_UNIT, OUTPOST_REPAIR_UNIT, OUTPOST_SALE_PERCENT, Outpost,
        OutpostService, TOWN_TRAVEL_CEILING_ROWS, TOWN_TRAVEL_MARGIN, TravelNode, ride_millis,
        scaled_price, sites::LogisticsSites, travel_cost,
    },
    position::Frame,
    quantize::quantize_simulation_scalar,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Why a logistics command cannot go through (the UI shows [`Self::message`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogisticsRefusal {
    UnknownPlayer,
    /// The pilot is on foot: logistics work the rig.
    OnFoot,
    /// The rig is riding a lift.
    Riding,
    NotAtStation,
    NoSuchLift,
    NoSuchStation,
    /// An ancient lift that has not been restored yet.
    NotRestored,
    AlreadyRestored,
    /// Ancient lifts are restored from their lowest station.
    NotBottomStation,
    /// Something solid is in the shaft between the stations.
    ShaftBlocked,
    /// A station already stands within two rows (or an ancient rail runs here).
    StationHere,
    /// Stations and beacons go underground.
    TooShallow,
    TooCloseToBeacon,
    /// A per-player or per-shaft limit.
    TooMany,
    NotYours,
    NotAtStop,
    UnknownStop,
    SameStop,
    /// The destination is filled in.
    DestinationBlocked,
    NotAtOutpost,
    NotClaimed,
    AlreadyClaimed,
    /// A full tank, a sound hull, an empty bay, a home already set.
    NothingToDo,
    NoCharges,
    ChargesFull,
    /// Dive charges sell at outposts and in town.
    NoSeller,
    /// Nothing under the rig a charge can cut.
    NothingToCut,
    NotEnoughCredits {
        need: u32,
    },
    NotEnoughFuel {
        need: u32,
    },
}

impl LogisticsRefusal {
    /// The refusal in words.
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::UnknownPlayer => "No rig.".to_owned(),
            Self::OnFoot => "Board your rig first.".to_owned(),
            Self::Riding => "The cab is moving.".to_owned(),
            Self::NotAtStation => "Stand at a lift station.".to_owned(),
            Self::NoSuchLift | Self::NoSuchStation => "That stop is gone.".to_owned(),
            Self::NotRestored => {
                "The ancient lift is dead: restore it from its lowest station.".to_owned()
            }
            Self::AlreadyRestored => "The lift already runs.".to_owned(),
            Self::NotBottomStation => {
                "The lift's machinery is at its lowest station: restore it from there.".to_owned()
            }
            Self::ShaftBlocked => "Something blocks the shaft between the stations.".to_owned(),
            Self::StationHere => "A station already stands here.".to_owned(),
            Self::TooShallow => "Too close to the surface: dig deeper first.".to_owned(),
            Self::TooCloseToBeacon => "Another beacon is too close.".to_owned(),
            Self::TooMany => "You have as many as you can run.".to_owned(),
            Self::NotYours => "That belongs to someone else.".to_owned(),
            Self::NotAtStop => "Travel from a beacon, an outpost or the town.".to_owned(),
            Self::UnknownStop => "No such stop on the network.".to_owned(),
            Self::SameStop => "You are already there.".to_owned(),
            Self::DestinationBlocked => "The far end is buried.".to_owned(),
            Self::NotAtOutpost => "Step inside the outpost.".to_owned(),
            Self::NotClaimed => "Claim the outpost first.".to_owned(),
            Self::AlreadyClaimed => "The crew already holds it.".to_owned(),
            Self::NothingToDo => "Nothing to do.".to_owned(),
            Self::NoCharges => "No dive charges aboard.".to_owned(),
            Self::ChargesFull => "The charge rack is full.".to_owned(),
            Self::NoSeller => "Dive charges sell at outposts and in town.".to_owned(),
            Self::NothingToCut => "Nothing below to dive through.".to_owned(),
            Self::NotEnoughCredits { need } => format!("Needs {need} credits."),
            Self::NotEnoughFuel { need } => format!("Needs {need} fuel."),
        }
    }
}

/// What a logistics command costs or pays back.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogisticsQuote {
    pub credits: u32,
    pub fuel: u32,
    pub refund: u32,
}

impl LogisticsQuote {
    const fn credits(credits: u32) -> Self {
        Self {
            credits,
            fuel: 0,
            refund: 0,
        }
    }
}

type Verdict = Result<LogisticsQuote, LogisticsRefusal>;

/// The resting centre `y` (local pixels of `frame`, F7) of a rig standing on row `row` (its
/// floor is the row below).
#[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
fn resting_y(frame: Frame, row: f64) -> f32 {
    let y = (row - frame.origin_row() as f64 + 1.0).mul_add(
        f64::from(TILE_SIZE),
        -f64::from(AUTHORITATIVE_PLAYER_RADIUS + 0.5),
    );
    #[allow(clippy::cast_possible_truncation, reason = "world pixels fit f32")]
    quantize_simulation_scalar(y as f32)
}

/// The centre `x` (local pixels of `frame`) of column `column`.
fn column_x(frame: Frame, column: i32) -> f32 {
    frame.col_px(column) + HALF_TILE
}

/// Half a tile (px).
const HALF_TILE: f32 = TILE_SIZE * 0.5;

impl WorldState {
    /// The world's depth logistics (lifts, outposts, beacons, rides, dives, rules).
    #[must_use]
    pub const fn logistics(&self) -> &Logistics {
        &self.logistics
    }

    /// Which upgrade ladder and depth rules this world plays.
    #[must_use]
    pub const fn progression_rules(&self) -> ProgressionRules {
        self.logistics.rules
    }

    /// Switches the world to `rules` before play (new Descent worlds, scenarios, tests).
    pub const fn set_progression_rules(&mut self, rules: ProgressionRules) {
        self.logistics.rules = rules;
    }

    /// The Descent layout this world's depth rules read.
    #[must_use]
    pub fn descent_layout(&self) -> crate::depth_progression::WorldLayout {
        layout_for(self.terrain.settings())
    }

    /// The Descent (0 = I) that `row` lies in.
    #[must_use]
    pub fn descent_of_row(&self, row: i32) -> u8 {
        self.descent_layout().descent_at(row)
    }

    /// Registers a generator's logistics sites (ancient lifts, deep outposts) before play.
    /// `stamp` also writes their tiles into the terrain, for worlds whose generator did not
    /// carve them (scenarios, tests); a generator that carved them passes `false`.
    pub fn install_logistics_sites(&mut self, sites: &LogisticsSites, stamp: bool) {
        if stamp {
            for (position, kind) in sites.carved_tiles() {
                let _ = self.terrain.set_kind(position, kind);
            }
        }
        for site in &sites.lifts {
            let id = self.logistics.allocate_id();
            self.logistics.lifts.insert(
                id,
                LiftShaft {
                    id,
                    column: site.column,
                    stations: site.stations.iter().copied().collect(),
                    origin: LiftOrigin::Ancient,
                    restored: false,
                    cab_row: site.bottom_row,
                },
            );
        }
        for site in &sites.outposts {
            let id = self.logistics.allocate_id();
            self.logistics.outposts.insert(
                id,
                Outpost {
                    id,
                    name: site.name.clone(),
                    column: site.column,
                    floor_row: site.floor_row,
                    descent: site.descent,
                    claimed_by: None,
                    reached: false,
                },
            );
        }
    }

    /// The rig's tile, if `player_id` is in its rig (not on foot).
    fn logistics_rig_tile(&self, player_id: PlayerId) -> Result<TilePosition, LogisticsRefusal> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(LogisticsRefusal::UnknownPlayer)?;
        if self.is_on_foot(player_id) {
            return Err(LogisticsRefusal::OnFoot);
        }
        if self.logistics.is_riding(player_id) {
            return Err(LogisticsRefusal::Riding);
        }
        Ok(player.tile_in(self.frame()))
    }

    fn credits_of(&self, player_id: PlayerId) -> u32 {
        self.players
            .get(&player_id)
            .map_or(0, |player| player.credits)
    }

    fn afford(&self, player_id: PlayerId, credits: u32) -> Result<(), LogisticsRefusal> {
        if self.credits_of(player_id) < credits {
            return Err(LogisticsRefusal::NotEnoughCredits { need: credits });
        }
        Ok(())
    }

    /// Whether a rig on `tile` is in the town for travel and shopping.
    fn in_town(&self, tile: TilePosition) -> bool {
        let (first, last) = crate::surface::town_span();
        let ground = self.terrain.surface_row(tile.x);
        (first - TOWN_TRAVEL_MARGIN..=last + TOWN_TRAVEL_MARGIN).contains(&tile.x)
            && (ground - TOWN_TRAVEL_CEILING_ROWS..=ground).contains(&tile.y)
    }

    /// The travel stop `player_id`'s rig is at, if any.
    #[must_use]
    pub fn travel_stop_of(&self, player_id: PlayerId) -> Option<TravelNode> {
        let tile = self.players.get(&player_id)?.tile_in(self.frame());
        if let Some(outpost) = self
            .logistics
            .outpost_at(tile)
            .filter(|outpost| outpost.reached)
        {
            return Some(TravelNode::Outpost(outpost.id));
        }
        if let Some(beacon) = self.logistics.beacon_near(tile) {
            return Some(TravelNode::Beacon(beacon.id));
        }
        self.in_town(tile).then_some(TravelNode::Surface)
    }

    /// Where a rig arrives at `node`: the tile and the rig's centre.
    #[must_use]
    pub fn travel_landing(&self, node: TravelNode) -> Option<(TilePosition, f32, f32)> {
        let frame = self.frame();
        match node {
            TravelNode::Surface => {
                let template = &self.player_template;
                Some((template.tile_in(frame), template.x, template.y))
            }
            TravelNode::Beacon(id) => {
                let beacon = self.logistics.beacons.get(&id)?;
                let y = frame.row_px(beacon.at.y) + HALF_TILE;
                Some((beacon.at, column_x(frame, beacon.at.x), y))
            }
            TravelNode::Outpost(id) => {
                let outpost = self.logistics.outposts.get(&id)?;
                let landing = outpost.landing();
                Some((
                    landing,
                    column_x(frame, landing.x),
                    resting_y(frame, f64::from(landing.y)),
                ))
            }
        }
    }

    /// A display name for travel stop `node`.
    #[must_use]
    pub fn travel_stop_name(&self, node: TravelNode) -> String {
        match node {
            TravelNode::Surface => "Town".to_owned(),
            TravelNode::Outpost(id) => self
                .logistics
                .outposts
                .get(&id)
                .map_or_else(|| "Outpost".to_owned(), |outpost| outpost.name.clone()),
            TravelNode::Beacon(id) => self.logistics.beacons.get(&id).map_or_else(
                || "Beacon".to_owned(),
                |beacon| format!("Beacon at row {}", beacon.at.y),
            ),
        }
    }

    fn outpost_for(&self, player_id: PlayerId, outpost: u32) -> Result<&Outpost, LogisticsRefusal> {
        let tile = self.logistics_rig_tile(player_id)?;
        self.logistics
            .outposts
            .get(&outpost)
            .filter(|found| found.contains(tile))
            .ok_or(LogisticsRefusal::NotAtOutpost)
    }

    /// Whether the column between `from` and `to` (exclusive of `from`) lets lift `lift`'s cab
    /// through: open tiles, the lift's own station floors, and an ancient lift's rail.
    fn shaft_clear(&self, lift: &LiftShaft, from: i32, to: i32) -> bool {
        let (step, mut row) = if to > from {
            (1, from + 1)
        } else {
            (-1, from - 1)
        };
        loop {
            let position = TilePosition {
                x: lift.column,
                y: row,
            };
            let kind = self.terrain.tile(position).map(|tile| tile.kind);
            let floor = lift.stations.contains(&(row - 1));
            let rail = lift.is_ancient()
                && matches!(
                    kind,
                    Some(TileKind::Built(BuiltTile::RuinWall | BuiltTile::Plating))
                );
            let open = kind.is_some() && !self.terrain.is_solid_at(position);
            if !(open || rail || (floor && row != to)) {
                return false;
            }
            if row == to {
                return true;
            }
            row += step;
        }
    }

    /// The verdict and price of `command` for `player_id` now (nothing changes).
    ///
    /// # Errors
    ///
    /// Returns why the command cannot go through.
    #[allow(clippy::too_many_lines, reason = "one arm per logistics command")]
    pub fn check_logistics(&self, player_id: PlayerId, command: &LogisticsCommand) -> Verdict {
        let tile = self.logistics_rig_tile(player_id)?;
        let descent_here = self.descent_of_row(tile.y);
        match *command {
            LogisticsCommand::RideLift { shaft, to_row } => {
                let lift = self
                    .logistics
                    .lifts
                    .get(&shaft)
                    .ok_or(LogisticsRefusal::NoSuchLift)?;
                let from = lift
                    .station_at(tile)
                    .ok_or(LogisticsRefusal::NotAtStation)?;
                if !lift.restored {
                    return Err(LogisticsRefusal::NotRestored);
                }
                if !lift.stations.contains(&to_row) {
                    return Err(LogisticsRefusal::NoSuchStation);
                }
                if to_row == from {
                    return Err(LogisticsRefusal::SameStop);
                }
                if !self.shaft_clear(lift, from, to_row) {
                    return Err(LogisticsRefusal::ShaftBlocked);
                }
                Ok(LogisticsQuote::default())
            }
            LogisticsCommand::RestoreLift { shaft } => {
                let lift = self
                    .logistics
                    .lifts
                    .get(&shaft)
                    .ok_or(LogisticsRefusal::NoSuchLift)?;
                let at = lift
                    .station_at(tile)
                    .ok_or(LogisticsRefusal::NotAtStation)?;
                if lift.restored {
                    return Err(LogisticsRefusal::AlreadyRestored);
                }
                if Some(at) != lift.bottom_station() {
                    return Err(LogisticsRefusal::NotBottomStation);
                }
                let price = scaled_price(ANCIENT_LIFT_RESTORE_PRICE, descent_here);
                self.afford(player_id, price)?;
                Ok(LogisticsQuote::credits(price))
            }
            LogisticsCommand::PlaceLiftStation => {
                if tile.y < self.terrain.surface_row(tile.x) {
                    return Err(LogisticsRefusal::TooShallow);
                }
                if self.terrain.is_solid_at(tile) {
                    return Err(LogisticsRefusal::DestinationBlocked);
                }
                if let Some(lift) = self.logistics.lift_on_column(tile.x) {
                    if lift.is_ancient() {
                        return Err(LogisticsRefusal::StationHere);
                    }
                    if lift.stations.iter().any(|row| (row - tile.y).abs() <= 2) {
                        return Err(LogisticsRefusal::StationHere);
                    }
                    if lift.stations.len() >= MAX_LIFT_STATIONS {
                        return Err(LogisticsRefusal::TooMany);
                    }
                } else {
                    let owned = self
                        .logistics
                        .lifts
                        .values()
                        .filter(|lift| lift.origin == LiftOrigin::Built { owner: player_id })
                        .count();
                    if owned >= MAX_BUILT_LIFTS_PER_PLAYER {
                        return Err(LogisticsRefusal::TooMany);
                    }
                }
                let floor = TilePosition {
                    x: tile.x,
                    y: tile.y + 1,
                };
                if self.terrain.tile(floor).is_none() {
                    return Err(LogisticsRefusal::DestinationBlocked);
                }
                let price = scaled_price(LIFT_STATION_PRICE, descent_here);
                self.afford(player_id, price)?;
                Ok(LogisticsQuote::credits(price))
            }
            LogisticsCommand::RemoveLiftStation { shaft, row } => {
                let lift = self
                    .logistics
                    .lifts
                    .get(&shaft)
                    .ok_or(LogisticsRefusal::NoSuchLift)?;
                match lift.origin {
                    LiftOrigin::Built { owner } if owner == player_id => {}
                    _ => return Err(LogisticsRefusal::NotYours),
                }
                if !lift.stations.contains(&row) {
                    return Err(LogisticsRefusal::NoSuchStation);
                }
                if self
                    .logistics
                    .rides
                    .values()
                    .any(|ride| ride.shaft == shaft)
                {
                    return Err(LogisticsRefusal::Riding);
                }
                Ok(LogisticsQuote {
                    refund: scaled_price(LIFT_STATION_PRICE, self.descent_of_row(row)) / 2,
                    ..LogisticsQuote::default()
                })
            }
            LogisticsCommand::PlaceBeacon => {
                if tile.y < self.terrain.surface_row(tile.x) + BEACON_MIN_DEPTH_ROWS {
                    return Err(LogisticsRefusal::TooShallow);
                }
                if self.logistics.beacons.values().any(|beacon| {
                    (beacon.at.x - tile.x).abs() < BEACON_SPACING_TILES
                        && (beacon.at.y - tile.y).abs() < BEACON_SPACING_TILES
                }) {
                    return Err(LogisticsRefusal::TooCloseToBeacon);
                }
                if self.logistics.beacons_of(player_id).count() >= MAX_BEACONS_PER_PLAYER {
                    return Err(LogisticsRefusal::TooMany);
                }
                let price = scaled_price(BEACON_PRICE, descent_here);
                self.afford(player_id, price)?;
                Ok(LogisticsQuote::credits(price))
            }
            LogisticsCommand::RemoveBeacon { beacon } => {
                let found = self
                    .logistics
                    .beacons
                    .get(&beacon)
                    .ok_or(LogisticsRefusal::UnknownStop)?;
                if found.owner != player_id {
                    return Err(LogisticsRefusal::NotYours);
                }
                Ok(LogisticsQuote {
                    refund: scaled_price(BEACON_PRICE, self.descent_of_row(found.at.y)) / 2,
                    ..LogisticsQuote::default()
                })
            }
            LogisticsCommand::Travel { to } => {
                let from = self
                    .travel_stop_of(player_id)
                    .ok_or(LogisticsRefusal::NotAtStop)?;
                if from == to {
                    return Err(LogisticsRefusal::SameStop);
                }
                let known = match to {
                    TravelNode::Surface => true,
                    TravelNode::Beacon(id) => self.logistics.beacons.contains_key(&id),
                    TravelNode::Outpost(id) => self
                        .logistics
                        .outposts
                        .get(&id)
                        .is_some_and(|outpost| outpost.reached),
                };
                if !known {
                    return Err(LogisticsRefusal::UnknownStop);
                }
                let (landing, _, _) = self
                    .travel_landing(to)
                    .ok_or(LogisticsRefusal::UnknownStop)?;
                if self.terrain.is_solid_at(landing) {
                    return Err(LogisticsRefusal::DestinationBlocked);
                }
                let (fuel, credits) = travel_cost(landing.y - tile.y);
                self.afford(player_id, credits)?;
                let fuel_on_board = self
                    .players
                    .get(&player_id)
                    .map_or(0.0, |player| player.fuel);
                #[allow(clippy::cast_precision_loss, reason = "fuel costs are small")]
                if fuel_on_board < fuel as f32 {
                    return Err(LogisticsRefusal::NotEnoughFuel { need: fuel });
                }
                Ok(LogisticsQuote {
                    credits,
                    fuel,
                    refund: 0,
                })
            }
            LogisticsCommand::Outpost { outpost, service } => {
                let found = self.outpost_for(player_id, outpost)?;
                self.check_outpost_service(player_id, found, service)
            }
            LogisticsCommand::BuyDiveCharges => {
                if self.logistics.outpost_at(tile).is_none() && !self.in_town(tile) {
                    return Err(LogisticsRefusal::NoSeller);
                }
                self.check_dive_purchase(player_id)
            }
            LogisticsCommand::FireDiveCharge => {
                if self.logistics.dive_charges_of(player_id) == 0 {
                    return Err(LogisticsRefusal::NoCharges);
                }
                if self.dive_cut(tile).is_empty() {
                    return Err(LogisticsRefusal::NothingToCut);
                }
                Ok(LogisticsQuote::default())
            }
        }
    }

    fn check_dive_purchase(&self, player_id: PlayerId) -> Verdict {
        let (count, price) = DIVE_CHARGE_PACK;
        if self
            .logistics
            .dive_charges_of(player_id)
            .saturating_add(count)
            > MAX_DIVE_CHARGES
        {
            return Err(LogisticsRefusal::ChargesFull);
        }
        self.afford(player_id, price)?;
        Ok(LogisticsQuote::credits(price))
    }

    fn check_outpost_service(
        &self,
        player_id: PlayerId,
        outpost: &Outpost,
        service: OutpostService,
    ) -> Verdict {
        let player = self
            .players
            .get(&player_id)
            .ok_or(LogisticsRefusal::UnknownPlayer)?;
        let tier = outpost.tier();
        match service {
            OutpostService::Refuel => {
                let missing = (player.fuel_capacity - player.fuel).ceil().max(0.0);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "fuel tanks hold some thousands of units"
                )]
                let missing = missing as u32;
                if missing == 0 {
                    return Err(LogisticsRefusal::NothingToDo);
                }
                let unit = OUTPOST_FUEL_UNIT[tier];
                if player.credits < unit {
                    return Err(LogisticsRefusal::NotEnoughCredits { need: unit });
                }
                Ok(LogisticsQuote::credits(
                    missing.min(player.credits / unit) * unit,
                ))
            }
            OutpostService::Repair => {
                let missing = (player.max_hull() - player.hull).ceil().max(0.0);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "hulls hold some thousands of points"
                )]
                let missing = missing as u32;
                if missing == 0 {
                    return Err(LogisticsRefusal::NothingToDo);
                }
                let unit = OUTPOST_REPAIR_UNIT[tier];
                if player.credits < unit {
                    return Err(LogisticsRefusal::NotEnoughCredits { need: unit });
                }
                Ok(LogisticsQuote::credits(
                    missing.min(player.credits / unit) * unit,
                ))
            }
            OutpostService::SellCargo => {
                let has_cargo = player.cargo_bay.minerals().next().is_some()
                    || player.cargo_bay.artifacts().next().is_some();
                if has_cargo {
                    Ok(LogisticsQuote::default())
                } else {
                    Err(LogisticsRefusal::NothingToDo)
                }
            }
            OutpostService::BuyDiveCharges => self.check_dive_purchase(player_id),
            OutpostService::Claim => {
                if outpost.claimed() {
                    return Err(LogisticsRefusal::AlreadyClaimed);
                }
                let price = outpost.claim_price();
                self.afford(player_id, price)?;
                Ok(LogisticsQuote::credits(price))
            }
            OutpostService::SetHome => {
                if !outpost.claimed() {
                    return Err(LogisticsRefusal::NotClaimed);
                }
                if self.logistics.homes.get(&player_id) == Some(&outpost.id) {
                    return Err(LogisticsRefusal::NothingToDo);
                }
                Ok(LogisticsQuote::default())
            }
        }
    }

    /// The tiles a dive charge fired from `tile` cuts: plain rock and ore straight down, up to
    /// [`DIVE_CHARGE_ROWS`] rows, stopping at anything else that is solid (hazards, relics,
    /// built pieces, bedrock).
    fn dive_cut(&self, tile: TilePosition) -> Vec<(TilePosition, TileKind)> {
        let mut cut = Vec::new();
        for depth in 1..=DIVE_CHARGE_ROWS {
            let position = TilePosition {
                x: tile.x,
                y: tile.y + depth,
            };
            let Some(kind) = self.terrain.tile(position).map(|tile| tile.kind) else {
                break;
            };
            match kind {
                TileKind::Dirt
                | TileKind::Clay
                | TileKind::Stone
                | TileKind::HardRock
                | TileKind::Ore(_)
                | TileKind::Surface(_)
                    if !self.terrain.is_indestructible_at(position) =>
                {
                    cut.push((position, kind));
                }
                TileKind::Air => {}
                _ if !self.terrain.is_solid_at(position) => {}
                _ => break,
            }
        }
        cut
    }

    /// Resolves one logistics command inside the tick (reports its own events).
    #[allow(clippy::too_many_lines, reason = "one arm per logistics command")]
    pub(super) fn apply_logistics_command(
        &mut self,
        player_id: PlayerId,
        command: LogisticsCommand,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Ok(quote) = self.check_logistics(player_id, &command) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(tile) = self
            .players
            .get(&player_id)
            .map(|player| player.tile_in(self.terrain.frame()))
        else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        self.charge_logistics(player_id, quote);
        match command {
            LogisticsCommand::RideLift { shaft, to_row } => {
                let Some(lift) = self.logistics.lifts.get(&shaft) else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                let from = lift.station_at(tile).unwrap_or(to_row);
                let millis = ride_millis(to_row - from, lift.rows_per_second());
                let ticks = self.timing.ticks_for_millis(millis).max(1);
                self.logistics.rides.insert(
                    player_id,
                    LiftRide {
                        shaft,
                        from_row: from,
                        to_row,
                        start_tick: self.simulation_tick,
                        ticks,
                    },
                );
                self.set_active_drill(player_id, None);
                self.pin_rider(player_id);
                output.push_event(GameplayEventKind::LiftRideStarted {
                    player_id,
                    shaft,
                    to_row,
                });
            }
            LogisticsCommand::RestoreLift { shaft } => {
                if let Some(lift) = self.logistics.lifts.get_mut(&shaft) {
                    lift.restored = true;
                }
                output.push_event(GameplayEventKind::LiftRestored { player_id, shaft });
            }
            LogisticsCommand::PlaceLiftStation => {
                let floor = TilePosition {
                    x: tile.x,
                    y: tile.y + 1,
                };
                if !self.terrain.is_solid_at(floor) {
                    let _ = self
                        .terrain
                        .set_kind(floor, TileKind::Built(BuiltTile::Plating));
                }
                if let Some(id) = self.logistics.lift_on_column(tile.x).map(|lift| lift.id) {
                    if let Some(lift) = self.logistics.lifts.get_mut(&id) {
                        lift.stations.insert(tile.y);
                    }
                } else {
                    let id = self.logistics.allocate_id();
                    self.logistics.lifts.insert(
                        id,
                        LiftShaft {
                            id,
                            column: tile.x,
                            stations: std::iter::once(tile.y).collect(),
                            origin: LiftOrigin::Built { owner: player_id },
                            restored: true,
                            cab_row: tile.y,
                        },
                    );
                }
                output.push_event(GameplayEventKind::LogisticsPlaced {
                    player_id,
                    x: tile.x,
                    y: tile.y,
                    beacon: false,
                });
            }
            LogisticsCommand::RemoveLiftStation { shaft, row } => {
                let mut empty = false;
                let mut column = 0;
                if let Some(lift) = self.logistics.lifts.get_mut(&shaft) {
                    lift.stations.remove(&row);
                    column = lift.column;
                    empty = lift.stations.is_empty();
                    if let Some(&nearest) = lift.stations.iter().min_by_key(|r| (*r - row).abs()) {
                        lift.cab_row = nearest;
                    }
                }
                let floor = TilePosition {
                    x: column,
                    y: row + 1,
                };
                if self.terrain.tile(floor).map(|tile| tile.kind)
                    == Some(TileKind::Built(BuiltTile::Plating))
                {
                    let _ = self.terrain.set_kind(floor, TileKind::Air);
                }
                if empty {
                    self.logistics.lifts.remove(&shaft);
                }
            }
            LogisticsCommand::PlaceBeacon => {
                let id = self.logistics.allocate_id();
                self.logistics.beacons.insert(
                    id,
                    DepthBeacon {
                        id,
                        owner: player_id,
                        at: tile,
                    },
                );
                output.push_event(GameplayEventKind::LogisticsPlaced {
                    player_id,
                    x: tile.x,
                    y: tile.y,
                    beacon: true,
                });
            }
            LogisticsCommand::RemoveBeacon { beacon } => {
                self.logistics.beacons.remove(&beacon);
            }
            LogisticsCommand::Travel { to } => {
                let Some((_, x, y)) = self.travel_landing(to) else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                self.set_active_drill(player_id, None);
                self.deep_run_states.remove(&player_id);
                self.logistics.dives.remove(&player_id);
                if let Some(player) = self.players.get_mut(&player_id) {
                    #[allow(clippy::cast_possible_truncation, reason = "world pixels")]
                    let (from_x, from_y) = (player.x as i32, player.y as i32);
                    player.x = x;
                    player.x_remainder = 0.0;
                    player.y = y;
                    player.velocity_x = 0.0;
                    player.velocity_y = 0.0;
                    #[allow(clippy::cast_possible_truncation, reason = "world pixels")]
                    output.push_event(GameplayEventKind::BeaconTravelled {
                        player_id,
                        from_x,
                        from_y,
                        to_x: x as i32,
                        to_y: y as i32,
                    });
                }
            }
            LogisticsCommand::Outpost { outpost, service } => {
                self.apply_outpost_service(player_id, outpost, service, quote);
                if service == OutpostService::Claim {
                    output.push_event(GameplayEventKind::OutpostClaimed { player_id, outpost });
                } else {
                    output.push_event(GameplayEventKind::OutpostServiced {
                        player_id,
                        outpost,
                        service,
                    });
                }
            }
            LogisticsCommand::BuyDiveCharges => {
                let count = self.logistics.dive_charges.entry(player_id).or_insert(0);
                *count = count
                    .saturating_add(DIVE_CHARGE_PACK.0)
                    .min(MAX_DIVE_CHARGES);
                output.push_event(GameplayEventKind::OutpostServiced {
                    player_id,
                    outpost: u32::MAX,
                    service: OutpostService::BuyDiveCharges,
                });
            }
            LogisticsCommand::FireDiveCharge => {
                let cut = self.dive_cut(tile);
                if let Some(count) = self.logistics.dive_charges.get_mut(&player_id) {
                    *count = count.saturating_sub(1);
                }
                for (position, kind) in &cut {
                    let _ = self.terrain.set_kind(*position, TileKind::Air);
                    if let (TileKind::Ore(mineral), Some(player)) =
                        (kind, self.players.get_mut(&player_id))
                    {
                        let _ = player.add_cargo(*mineral);
                    }
                }
                self.set_active_drill(player_id, None);
                self.logistics.dives.insert(
                    player_id,
                    DiveState {
                        fired_tick: self.simulation_tick,
                    },
                );
                let frame = self.frame();
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.x = column_x(frame, tile.x);
                    player.x_remainder = 0.0;
                    player.velocity_x = 0.0;
                    player.velocity_y = player.velocity_y.max(DIVE_KICK_SPEED);
                    #[allow(clippy::cast_possible_truncation, reason = "world pixels")]
                    output.push_event(GameplayEventKind::DiveChargeFired {
                        player_id,
                        x: player.x as i32,
                        y: player.y as i32,
                        rows: u8::try_from(cut.len()).unwrap_or(u8::MAX),
                    });
                }
            }
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn charge_logistics(&mut self, player_id: PlayerId, quote: LogisticsQuote) {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player
                .credits
                .saturating_sub(quote.credits)
                .saturating_add(quote.refund);
            #[allow(clippy::cast_precision_loss, reason = "fuel costs are small")]
            {
                player.fuel =
                    quantize_simulation_scalar((player.fuel - quote.fuel as f32).max(0.0));
            }
        }
    }

    fn apply_outpost_service(
        &mut self,
        player_id: PlayerId,
        outpost: u32,
        service: OutpostService,
        quote: LogisticsQuote,
    ) {
        let tier = self
            .logistics
            .outposts
            .get(&outpost)
            .map_or(0, Outpost::tier);
        match service {
            OutpostService::Refuel => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    #[allow(clippy::cast_precision_loss, reason = "fuel units are small")]
                    let added = (quote.credits / OUTPOST_FUEL_UNIT[tier]) as f32;
                    player.fuel = (player.fuel + added).min(player.fuel_capacity);
                }
            }
            OutpostService::Repair => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    #[allow(clippy::cast_precision_loss, reason = "hull points are small")]
                    let added = (quote.credits / OUTPOST_REPAIR_UNIT[tier]) as f32;
                    player.hull = (player.hull + added).min(player.max_hull());
                }
            }
            OutpostService::SellCargo => {
                let before = self.credits_of(player_id);
                let _sold = self.sell_canonical_cargo(player_id);
                if let Some(player) = self.players.get_mut(&player_id) {
                    let payout = player.credits.saturating_sub(before);
                    let cut = payout.saturating_mul(100 - OUTPOST_SALE_PERCENT[tier]) / 100;
                    player.credits = player.credits.saturating_sub(cut);
                    self.progression.total_earnings =
                        self.progression.total_earnings.saturating_sub(cut);
                }
            }
            OutpostService::BuyDiveCharges => {
                let count = self.logistics.dive_charges.entry(player_id).or_insert(0);
                *count = count
                    .saturating_add(DIVE_CHARGE_PACK.0)
                    .min(MAX_DIVE_CHARGES);
            }
            OutpostService::Claim => {
                if let Some(found) = self.logistics.outposts.get_mut(&outpost) {
                    found.claimed_by = Some(player_id);
                    found.reached = true;
                }
            }
            OutpostService::SetHome => {
                self.logistics.homes.insert(player_id, outpost);
            }
        }
    }

    /// Puts a riding rig where its cab is now.
    fn pin_rider(&mut self, player_id: PlayerId) {
        let Some(ride) = self.logistics.rides.get(&player_id).copied() else {
            return;
        };
        let Some(column) = self
            .logistics
            .lifts
            .get(&ride.shaft)
            .map(|lift| lift.column)
        else {
            return;
        };
        let row = ride.row_at(self.simulation_tick);
        let frame = self.frame();
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = column_x(frame, column);
            player.x_remainder = 0.0;
            player.y = resting_y(frame, row);
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
    }

    /// One tick of logistics: rides move and arrive, dives settle, outposts are reached, and on
    /// Descent worlds rigs past a gate they lack the system for are crushed.
    #[allow(
        clippy::too_many_lines,
        reason = "the logistics pass in tick order: rides, dives, outposts, gates"
    )]
    pub(super) fn advance_logistics(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        // Rides (a wrecked rig drops out of its cab; the rescue takes it from there).
        let riders: Vec<PlayerId> = self.logistics.rides.keys().copied().collect();
        for player_id in riders {
            let wrecked = self
                .players
                .get(&player_id)
                .is_none_or(|player| player.hull <= 0.0);
            if wrecked {
                self.logistics.rides.remove(&player_id);
                continue;
            }
            let Some(ride) = self.logistics.rides.get(&player_id).copied() else {
                continue;
            };
            let Some(column) = self
                .logistics
                .lifts
                .get(&ride.shaft)
                .map(|lift| lift.column)
            else {
                self.logistics.rides.remove(&player_id);
                continue;
            };
            let row = ride.row_at(next_tick);
            let frame = self.frame();
            if let Some(player) = self.players.get_mut(&player_id) {
                player.x = column_x(frame, column);
                player.x_remainder = 0.0;
                player.y = resting_y(frame, row);
                player.velocity_x = 0.0;
                player.velocity_y = 0.0;
            }
            if ride.arrived(next_tick) {
                self.logistics.rides.remove(&player_id);
                if let Some(lift) = self.logistics.lifts.get_mut(&ride.shaft) {
                    lift.cab_row = ride.to_row;
                }
                output.push_event(GameplayEventKind::LiftArrived {
                    player_id,
                    shaft: ride.shaft,
                });
            }
        }
        // Dives end once the rig has settled.
        let settle = self.timing.ticks_for_millis(DIVE_SETTLE_MILLIS);
        let longest = self.timing.ticks_for_millis(DIVE_MAX_MILLIS);
        let divers: Vec<(PlayerId, DiveState)> = self
            .logistics
            .dives
            .iter()
            .map(|(id, dive)| (*id, *dive))
            .collect();
        for (player_id, dive) in divers {
            let elapsed = next_tick.get().saturating_sub(dive.fired_tick.get());
            let still = self
                .players
                .get(&player_id)
                .is_none_or(|player| player.velocity_y.abs() < f32::EPSILON);
            if elapsed >= longest || (elapsed >= settle && still) {
                self.logistics.dives.remove(&player_id);
            }
        }
        // Outposts join the network when a rig first reaches them.
        let frame = self.frame();
        let tiles: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .collect();
        for (player_id, tile) in &tiles {
            let newly = self
                .logistics
                .outposts
                .values()
                .find(|outpost| !outpost.reached && outpost.contains(*tile))
                .map(|outpost| outpost.id);
            if let Some(outpost) = newly {
                if let Some(found) = self.logistics.outposts.get_mut(&outpost) {
                    found.reached = true;
                }
                output.push_event(GameplayEventKind::OutpostReached {
                    player_id: *player_id,
                    outpost,
                });
            }
        }
        // The Descent gates (Descent worlds only).
        if !self.logistics.rules.is_descent() {
            return;
        }
        let delta = self.timing.delta_seconds;
        for (player_id, tile) in tiles {
            let descent = self.descent_of_row(tile.y);
            let systems = self
                .players
                .get(&player_id)
                .map_or(0, |player| player.hazard_gear.rig_systems);
            let Some(missing) = missing_gate(systems, descent) else {
                continue;
            };
            let warned = self.logistics.gate_warned.get(&player_id).copied();
            if warned.is_none_or(|warned| warned < descent) {
                self.logistics.gate_warned.insert(player_id, descent);
                output.push_event(GameplayEventKind::DescentGateBlocked {
                    player_id,
                    descent,
                    system: missing,
                });
            }
            if self.damage_player(player_id, GATE_DAMAGE_PER_SECOND * delta) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
            }
        }
    }

    /// The claimed outpost `player_id` made home, as a rescue landing.
    pub(super) fn outpost_rescue_home(&self, player_id: PlayerId) -> Option<(f32, f32)> {
        let outpost = self.logistics.homes.get(&player_id)?;
        let outpost = self
            .logistics
            .outposts
            .get(outpost)
            .filter(|outpost| outpost.claimed())?;
        let landing = outpost.landing();
        let frame = self.frame();
        Some((
            column_x(frame, landing.x),
            resting_y(frame, f64::from(landing.y)),
        ))
    }

    /// Gas damage after the rig's Bio-Filter (Descent worlds).
    pub(super) fn bio_filtered(&self, player_id: PlayerId, damage: f32) -> f32 {
        let filtered = self.logistics.rules.is_descent()
            && self
                .players
                .get(&player_id)
                .is_some_and(|player| player.hazard_gear.has_system(RigSystem::BioFilter));
        if filtered {
            #[allow(clippy::cast_precision_loss, reason = "a percentage")]
            let keep = BIO_FILTER_GAS_PERCENT as f32 / 100.0;
            damage * keep
        } else {
            damage
        }
    }

    /// Extra scanner reach from the Sonar Suite (Descent worlds).
    pub(super) fn sonar_bonus(&self, player_id: PlayerId) -> i32 {
        let sonar = self.logistics.rules.is_descent()
            && self
                .players
                .get(&player_id)
                .is_some_and(|player| player.hazard_gear.has_system(RigSystem::SonarSuite));
        if sonar {
            SONAR_SUITE_SCAN_BONUS_TILES
        } else {
            0
        }
    }
}
