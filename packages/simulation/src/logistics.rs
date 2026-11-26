//! Depth logistics (expansion plan Part III, F10): the ways a 13,000-row dig stays a journey
//! instead of a commute.
//!
//! * **Lifts** ([`LiftShaft`]): a cab rides a vertical shaft between stations. *Ancient* lifts
//!   are placed by generators through [`sites`] (a rail of builder stone with station pockets)
//!   and must be restored once, from their lowest station, before they run; *built* lifts are
//!   stations a player places on a shaft column they dug. A ride is a fixed-speed move with a
//!   start tick, so it is deterministic and tick-rate safe; the rider is pinned to the cab.
//! * **Deep outposts** ([`Outpost`]): placed by generators; reaching one adds it to the crew's
//!   travel network. Fuel, repairs and selling cost a fee until the crew claims it; a claimed
//!   outpost also takes a respawn point.
//! * **Depth beacons** ([`DepthBeacon`]): placed by players; from any beacon, reached outpost
//!   or the town, a rig can travel to any other for fuel and credits ([`travel_cost`]).
//! * **Dive charges** (the momentum tool): a charge cuts a straight shaft under the rig and
//!   kicks it down it; the landing at the bottom of a dive does no crash damage.
//!
//! Everything is a [`LogisticsCommand`] resolved inside the tick (`world/logistics.rs`) and one
//! replicated section ([`crate::world::SectionKey::Logistics`]).

pub mod sites;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    depth_progression::ProgressionRules,
    ids::{PlayerId, SimulationTick},
    terrain::TilePosition,
};

/// Rows per second of a restored ancient lift's cab.
pub const ANCIENT_LIFT_ROWS_PER_SECOND: u32 = 45;
/// Rows per second of a built lift's cab.
pub const BUILT_LIFT_ROWS_PER_SECOND: u32 = 20;
/// Credits to restore an ancient lift (times [`crate::depth_progression::descent_price_percent`]).
pub const ANCIENT_LIFT_RESTORE_PRICE: u32 = 2_500;
/// Credits for one built lift station (times the Descent's price percent).
pub const LIFT_STATION_PRICE: u32 = 600;
/// Stations one shaft holds at most.
pub const MAX_LIFT_STATIONS: usize = 24;
/// Built lift shafts one player may own.
pub const MAX_BUILT_LIFTS_PER_PLAYER: usize = 6;
/// How far (in rows) a rig may stand from a station's row and still board.
pub const STATION_REACH_ROWS: i32 = 1;

/// Credits for one depth beacon (times the Descent's price percent).
pub const BEACON_PRICE: u32 = 400;
/// Beacons one player may own.
pub const MAX_BEACONS_PER_PLAYER: usize = 8;
/// Beacons keep at least this many tiles apart.
pub const BEACON_SPACING_TILES: i32 = 8;
/// A rig within this many tiles of a beacon may travel from it.
pub const BEACON_REACH_TILES: i32 = 2;
/// Beacons go at least this many rows under the local surface.
pub const BEACON_MIN_DEPTH_ROWS: i32 = 8;
/// Columns either side of the town's buildings that still count as the town for travel and
/// buying dive charges.
pub const TOWN_TRAVEL_MARGIN: i32 = 6;
/// Rows above the town's ground that still count as the town (a rig hovering over the
/// rooftops; not a sky isle or the Moon above it).
pub const TOWN_TRAVEL_CEILING_ROWS: i32 = 24;

/// Half the width (in tiles, either side of its column) of an outpost's room.
pub const OUTPOST_HALF_WIDTH: i32 = 4;
/// Height in tiles of an outpost's room (above its floor row).
pub const OUTPOST_HEIGHT: i32 = 4;
/// Rows below a rig a supply stop may lie and still be the rig's own stop (Wave 11 carryover
/// review: a rig in an outpost's hall stands above its floor row; see
/// [`Logistics::supply_row_above`]).
pub const SUPPLY_STOP_SLACK_ROWS: i32 = OUTPOST_HEIGHT * 4;
/// Credits to claim an outpost (times the Descent's price percent).
pub const OUTPOST_CLAIM_PRICE: u32 = 5_000;
/// Credits per unit of fuel at an unclaimed / claimed outpost.
pub const OUTPOST_FUEL_UNIT: [u32; 2] = [3, 1];
/// Credits per hull point at an unclaimed / claimed outpost.
pub const OUTPOST_REPAIR_UNIT: [u32; 2] = [5, 2];
/// Percent of a sale an unclaimed / claimed outpost pays out (the rest is its cut).
pub const OUTPOST_SALE_PERCENT: [u32; 2] = [75, 95];

/// Dive charges in one pack, and its price.
pub const DIVE_CHARGE_PACK: (u8, u32) = (3, 240);
/// Dive charges a rig carries at most.
pub const MAX_DIVE_CHARGES: u8 = 9;
/// Rows a dive charge cuts under the rig.
pub const DIVE_CHARGE_ROWS: i32 = 24;
/// Downward speed (px/s) a dive charge kicks the rig to.
pub const DIVE_KICK_SPEED: f32 = 420.0;
/// A dive ends (and crash damage counts again) once the rig stands still this long after the
/// charge.
pub const DIVE_SETTLE_MILLIS: u64 = 250;
/// A dive ends on its own after this long.
pub const DIVE_MAX_MILLIS: u64 = 20_000;

/// Where a lift came from.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LiftOrigin {
    /// Placed by the world generator: a builders' lift that must be restored once.
    Ancient,
    /// Stations a player placed on a shaft they dug.
    Built { owner: PlayerId },
}

/// A lift shaft: a column with stations a cab rides between.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LiftShaft {
    pub id: u32,
    pub column: i32,
    /// Rows a rig stands on to board (each station's floor is the row under it).
    pub stations: BTreeSet<i32>,
    pub origin: LiftOrigin,
    /// Whether the cab runs (built lifts always do; ancient ones once restored).
    pub restored: bool,
    /// The station row the cab last stopped at (where it is drawn waiting).
    pub cab_row: i32,
}

impl LiftShaft {
    /// Rows per second of this lift's cab.
    #[must_use]
    pub const fn rows_per_second(&self) -> u32 {
        match self.origin {
            LiftOrigin::Ancient => ANCIENT_LIFT_ROWS_PER_SECOND,
            LiftOrigin::Built { .. } => BUILT_LIFT_ROWS_PER_SECOND,
        }
    }

    /// The station a rig on `tile` can board, if it stands at one.
    #[must_use]
    pub fn station_at(&self, tile: TilePosition) -> Option<i32> {
        if tile.x != self.column {
            return None;
        }
        self.stations
            .iter()
            .copied()
            .find(|row| (row - tile.y).abs() <= STATION_REACH_ROWS)
    }

    /// The lowest station (ancient lifts are restored from it).
    #[must_use]
    pub fn bottom_station(&self) -> Option<i32> {
        self.stations.iter().next_back().copied()
    }

    /// Whether this is an ancient (generated) lift.
    #[must_use]
    pub const fn is_ancient(&self) -> bool {
        matches!(self.origin, LiftOrigin::Ancient)
    }
}

/// A rig riding a lift cab.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LiftRide {
    pub shaft: u32,
    pub from_row: i32,
    pub to_row: i32,
    pub start_tick: SimulationTick,
    /// Ticks the ride takes ([`ride_ticks`]).
    pub ticks: u64,
}

impl LiftRide {
    /// How far along the ride is at `tick`, as `(done, total)` ticks.
    #[must_use]
    pub const fn progress(&self, tick: SimulationTick) -> (u64, u64) {
        let done = tick.get().saturating_sub(self.start_tick.get());
        let total = if self.ticks == 0 { 1 } else { self.ticks };
        (if done > total { total } else { done }, total)
    }

    /// The cab's row at `tick` (fractional, for drawing and pinning the rider).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "ride lengths are thousands of ticks and rows"
    )]
    pub fn row_at(&self, tick: SimulationTick) -> f64 {
        let (done, total) = self.progress(tick);
        let share = done as f64 / total as f64;
        f64::from(self.to_row - self.from_row).mul_add(share, f64::from(self.from_row))
    }

    /// Whether the cab has arrived by `tick`.
    #[must_use]
    pub const fn arrived(&self, tick: SimulationTick) -> bool {
        let (done, total) = self.progress(tick);
        done >= total
    }
}

/// Milliseconds a cab takes for `rows` at `rows_per_second`.
#[must_use]
pub const fn ride_millis(rows: i32, rows_per_second: u32) -> u64 {
    let speed = if rows_per_second == 0 {
        1
    } else {
        rows_per_second as u64
    };
    (rows.unsigned_abs() as u64 * 1_000).div_ceil(speed)
}

/// A deep outpost: a camp in a Descent with fuel, repairs and a buyer, claimable as a base.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Outpost {
    pub id: u32,
    pub name: String,
    /// The room's middle column.
    pub column: i32,
    /// The room's floor row (rigs stand on the row above).
    pub floor_row: i32,
    /// The Descent it serves (prices scale with it).
    pub descent: u8,
    /// Who claimed it for the crew, if anyone.
    pub claimed_by: Option<PlayerId>,
    /// Whether anyone has reached it (reached outposts join the travel network).
    pub reached: bool,
}

impl Outpost {
    /// Whether `tile` is inside the outpost's room.
    #[must_use]
    pub const fn contains(&self, tile: TilePosition) -> bool {
        (tile.x - self.column).abs() <= OUTPOST_HALF_WIDTH
            && tile.y < self.floor_row
            && tile.y >= self.floor_row - OUTPOST_HEIGHT
    }

    /// Whether the crew has claimed it.
    #[must_use]
    pub const fn claimed(&self) -> bool {
        self.claimed_by.is_some()
    }

    /// The fee tier: 0 unclaimed, 1 claimed.
    #[must_use]
    pub const fn tier(&self) -> usize {
        if self.claimed() { 1 } else { 0 }
    }

    /// The tile a rig arrives on when it travels here.
    #[must_use]
    pub const fn landing(&self) -> TilePosition {
        TilePosition {
            x: self.column,
            y: self.floor_row - 1,
        }
    }

    /// Credits to claim it.
    #[must_use]
    pub const fn claim_price(&self) -> u32 {
        scaled_price(OUTPOST_CLAIM_PRICE, self.descent)
    }
}

/// A player's depth beacon.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DepthBeacon {
    pub id: u32,
    pub owner: PlayerId,
    pub at: TilePosition,
}

/// A stop on the travel network.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TravelNode {
    /// The town on the surface.
    Surface,
    Beacon(u32),
    Outpost(u32),
}

/// A service at an outpost's counter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum OutpostService {
    Refuel,
    Repair,
    SellCargo,
    BuyDiveCharges,
    /// Claim the outpost for the crew (cheaper services, a respawn point).
    Claim,
    /// Make this claimed outpost the player's rescue point.
    SetHome,
}

impl OutpostService {
    /// Every service, in counter order.
    pub const ALL: [Self; 6] = [
        Self::Refuel,
        Self::Repair,
        Self::SellCargo,
        Self::BuyDiveCharges,
        Self::Claim,
        Self::SetHome,
    ];

    /// The counter's label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Refuel => "Refuel",
            Self::Repair => "Repair",
            Self::SellCargo => "Sell cargo",
            Self::BuyDiveCharges => "Dive charges",
            Self::Claim => "Claim outpost",
            Self::SetHome => "Make it home",
        }
    }
}

/// A depth logistics action (always the acting player's rig; never on foot).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LogisticsCommand {
    /// Ride lift `shaft` from the station the rig stands at to the station at `to_row`.
    RideLift { shaft: u32, to_row: i32 },
    /// Restore ancient lift `shaft` (from its lowest station).
    RestoreLift { shaft: u32 },
    /// Place a lift station on the rig's tile (extending the lift on its column, or starting
    /// one).
    PlaceLiftStation,
    /// Take down your own station at `row` on `shaft` (half the price back).
    RemoveLiftStation { shaft: u32, row: i32 },
    /// Place a depth beacon on the rig's tile.
    PlaceBeacon,
    /// Take down your own beacon (half the price back).
    RemoveBeacon { beacon: u32 },
    /// Travel from the stop the rig is at to `to`.
    Travel { to: TravelNode },
    /// A service at the outpost the rig is in.
    Outpost {
        outpost: u32,
        service: OutpostService,
    },
    /// Buy a pack of dive charges (at an outpost or in town).
    BuyDiveCharges,
    /// Fire a dive charge straight down.
    FireDiveCharge,
    /// Buy the next tier of shop line `offer` (an index of [`crate::economy::UpgradeKind::ALL`])
    /// at the counter of outpost `outpost` (Wave 8: the deep shops; the outpost must stock the
    /// tier, [`crate::depth_progression::outpost_stocks`]).
    OutpostUpgrade { outpost: u32, offer: u8 },
    /// Work recipe `recipe` ([`crate::workshop::RECIPES`]) at outpost `outpost`'s workshop
    /// (Wave 8).
    Craft { outpost: u32, recipe: u8 },
    /// A Living Dark command (Wave 8): root axes and the Amber Court's counter, served by the
    /// same dialog.
    LivingDark(crate::living_dark::LivingDarkAction),
    /// A Hollow World command (Wave 9): the Gravity Anchor.
    Hollow(crate::hollow::HollowAction),
    /// A far Strange Layers command (Wave 9 strange-b): lamp filters and time bubbles, served
    /// by the same dialog and the use button.
    FarStrange(crate::far_strange::FarStrangeAction),
    /// Buy one `supply` ([`crate::workshop::DeepSupply`]) at the counter of outpost `outpost`
    /// (Wave 9: the deep counters sell their Descent's supplies and every shallower one's,
    /// [`crate::workshop::outpost_sells_supply`]).
    BuySupply {
        outpost: u32,
        supply: crate::workshop::DeepSupply,
    },
    /// Ask the player's archivist a question (Wave 11 carryover,
    /// [`crate::ancient_city::questions`]): it answers at once, in the rig or on foot.
    AskArchivist(crate::ancient_city::questions::ArchivistQuestion),
}

/// A rig in a dive (from a dive charge): crash damage is off until it settles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiveState {
    pub fired_tick: SimulationTick,
}

/// The world's depth logistics: lifts, outposts, beacons, rides, dives and the progression rules
/// (one replicated, saved section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Logistics {
    /// Which upgrade ladder and depth rules the world plays.
    #[serde(default)]
    pub rules: ProgressionRules,
    #[serde(default)]
    pub lifts: BTreeMap<u32, LiftShaft>,
    #[serde(default)]
    pub outposts: BTreeMap<u32, Outpost>,
    #[serde(default)]
    pub beacons: BTreeMap<u32, DepthBeacon>,
    #[serde(default)]
    pub rides: BTreeMap<PlayerId, LiftRide>,
    #[serde(default)]
    pub dives: BTreeMap<PlayerId, DiveState>,
    #[serde(default)]
    pub dive_charges: BTreeMap<PlayerId, u8>,
    /// Claimed outposts players made their rescue point.
    #[serde(default)]
    pub homes: BTreeMap<PlayerId, u32>,
    /// The deepest Descent each player has been warned about lacking a gate system for.
    #[serde(default)]
    pub gate_warned: BTreeMap<PlayerId, u8>,
    /// Rig systems whose need the crew has felt (Wave 8, bits of
    /// [`crate::depth_progression::RigSystem::bit`]): a gate's crush, the Glow Sea's dark. The
    /// shop sells a rig system only once it is felt ("felt before you can buy it").
    #[serde(default)]
    pub felt_systems: u8,
    /// The deepest Descent each player's haul has come from since their last sale (Wave 8):
    /// buyers pay that Descent's premium on ore
    /// ([`crate::depth_progression::descent_value_percent`]).
    #[serde(default)]
    pub haul_descent: BTreeMap<PlayerId, u8>,
    /// The next id for a lift, outpost or beacon.
    #[serde(default)]
    pub next_id: u32,
    /// Each player's deep supply kit (Wave 9 progression): how many of each
    /// [`crate::workshop::DeepSupply`] the rig carries, by [`crate::workshop::DeepSupply::index`]
    /// (up to [`crate::workshop::MAX_SUPPLY_STACK`] each). Bought at the deep counters
    /// ([`LogisticsCommand::BuySupply`]) or crafted; the Descent mechanics spend them
    /// ([`crate::WorldState::take_supply`]). An emptied kit is dropped.
    #[serde(default)]
    pub supplies: BTreeMap<PlayerId, Vec<u8>>,
    /// What the spent supplies are still doing (Wave 10 polish): blowing ballast, burning
    /// flares, lying bait, fitted lamp filter glass.
    #[serde(default)]
    pub supply_effects: crate::workshop::SupplyEffects,
}

impl Logistics {
    /// The deepest supply stop at or above `row` (Wave 11 carryover, the supply line): an
    /// outpost, a restored lift's station or a depth beacon (anyone's: the crew shares the
    /// network), else the surface (row 0).
    ///
    /// A stop up to [`SUPPLY_STOP_SLACK_ROWS`] below `row` counts too: a rig standing in an
    /// outpost's hall, or at a station's cab, is a row or more above the stop's floor row and
    /// is at that stop, not far below the one before it.
    #[must_use]
    pub fn supply_row_above(&self, row: i32) -> i32 {
        let outposts = self.outposts.values().map(|outpost| outpost.floor_row);
        let beacons = self.beacons.values().map(|beacon| beacon.at.y);
        let stations = self
            .lifts
            .values()
            .filter(|lift| lift.restored)
            .flat_map(|lift| lift.stations.iter().copied());
        outposts
            .chain(beacons)
            .chain(stations)
            .filter(|stop| *stop <= row.saturating_add(SUPPLY_STOP_SLACK_ROWS))
            .max()
            .unwrap_or(0)
            .max(0)
    }

    /// The supply line's strain at `row` on a world with `settings` (Deep Descent rules only;
    /// [`crate::depth_progression::supply_strain`]).
    #[must_use]
    pub fn supply_strain_at(&self, settings: &crate::worldgen::WorldSettings, row: i32) -> f32 {
        if !self.rules.is_descent() || row <= 0 {
            return 0.0;
        }
        let descent = crate::depth_progression::descent_index_at_row(settings, row);
        let below = (row - self.supply_row_above(row)).max(0);
        crate::depth_progression::supply_strain(below, descent)
    }

    /// Percent of the ore price buyers pay `player_id` for the haul aboard (Wave 8, Descent
    /// worlds: the deepest Descent it came from,
    /// [`crate::depth_progression::descent_value_percent`]; 100 elsewhere). The sale counters
    /// read it too, so the payout they show is the payout the sale makes.
    #[must_use]
    pub fn haul_premium_percent(&self, player_id: PlayerId) -> u32 {
        if !self.rules.is_descent() {
            return 100;
        }
        crate::depth_progression::descent_value_percent(
            self.haul_descent.get(&player_id).copied().unwrap_or(0),
        )
    }

    /// Whether nothing logistic exists (a classic world that never used it: not hashed, so
    /// older worlds keep their hashes).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Takes the next id.
    pub const fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// The lift on `column`, if any.
    #[must_use]
    pub fn lift_on_column(&self, column: i32) -> Option<&LiftShaft> {
        self.lifts.values().find(|lift| lift.column == column)
    }

    /// The lift station a rig on `tile` stands at, as `(shaft, row)`.
    #[must_use]
    pub fn station_at(&self, tile: TilePosition) -> Option<(u32, i32)> {
        self.lifts
            .values()
            .find_map(|lift| lift.station_at(tile).map(|row| (lift.id, row)))
    }

    /// Every lift station a rig on `tile` stands at, as `(shaft, row)`, by shaft id. Chained
    /// lifts share a station (one lift's bottom is the next one's top, as in
    /// [`sites::LogisticsSites::plan`]), so a stop can serve more than one shaft.
    #[must_use]
    pub fn stations_at(&self, tile: TilePosition) -> Vec<(u32, i32)> {
        self.lifts
            .values()
            .filter_map(|lift| lift.station_at(tile).map(|row| (lift.id, row)))
            .collect()
    }

    /// The outpost whose room holds `tile`.
    #[must_use]
    pub fn outpost_at(&self, tile: TilePosition) -> Option<&Outpost> {
        self.outposts
            .values()
            .find(|outpost| outpost.contains(tile))
    }

    /// The beacon a rig on `tile` is in reach of.
    #[must_use]
    pub fn beacon_near(&self, tile: TilePosition) -> Option<&DepthBeacon> {
        self.beacons.values().find(|beacon| {
            (beacon.at.x - tile.x).abs() <= BEACON_REACH_TILES
                && (beacon.at.y - tile.y).abs() <= BEACON_REACH_TILES
        })
    }

    /// `player`'s beacons.
    pub fn beacons_of(&self, player: PlayerId) -> impl Iterator<Item = &DepthBeacon> {
        self.beacons
            .values()
            .filter(move |beacon| beacon.owner == player)
    }

    /// How many of `supply` `player`'s kit holds (Wave 9).
    #[must_use]
    pub fn supplies_of(&self, player: PlayerId, supply: crate::workshop::DeepSupply) -> u8 {
        self.supplies
            .get(&player)
            .and_then(|kit| kit.get(supply.index()))
            .copied()
            .unwrap_or(0)
    }

    /// Adds `count` of `supply` to `player`'s kit (up to the stack limit); returns how many
    /// went in.
    pub fn add_supplies(
        &mut self,
        player: PlayerId,
        supply: crate::workshop::DeepSupply,
        count: u8,
    ) -> u8 {
        let kit = self.supplies.entry(player).or_default();
        if kit.len() <= supply.index() {
            kit.resize(supply.index() + 1, 0);
        }
        let slot = &mut kit[supply.index()];
        let added = count.min(crate::workshop::MAX_SUPPLY_STACK.saturating_sub(*slot));
        *slot += added;
        added
    }

    /// Takes one `supply` out of `player`'s kit; whether there was one. An emptied kit is
    /// dropped, so the section stays as it was before the player ever bought one.
    pub fn take_supply(&mut self, player: PlayerId, supply: crate::workshop::DeepSupply) -> bool {
        let Some(kit) = self.supplies.get_mut(&player) else {
            return false;
        };
        let Some(slot) = kit.get_mut(supply.index()).filter(|slot| **slot > 0) else {
            return false;
        };
        *slot -= 1;
        if kit.iter().all(|count| *count == 0) {
            self.supplies.remove(&player);
        }
        true
    }

    /// Dive charges `player` carries.
    #[must_use]
    pub fn dive_charges_of(&self, player: PlayerId) -> u8 {
        self.dive_charges.get(&player).copied().unwrap_or(0)
    }

    /// Whether `player`'s rig is riding a lift.
    #[must_use]
    pub fn is_riding(&self, player: PlayerId) -> bool {
        self.rides.contains_key(&player)
    }

    /// Whether `player`'s rig is in a dive (its landing does no crash damage).
    #[must_use]
    pub fn is_diving(&self, player: PlayerId) -> bool {
        self.dives.contains_key(&player)
    }

    /// Every stop on the crew's travel network: the town, every beacon and every reached
    /// outpost.
    #[must_use]
    pub fn travel_nodes(&self) -> Vec<TravelNode> {
        let mut nodes = vec![TravelNode::Surface];
        nodes.extend(
            self.outposts
                .values()
                .filter(|outpost| outpost.reached)
                .map(|outpost| TravelNode::Outpost(outpost.id)),
        );
        nodes.extend(self.beacons.keys().map(|id| TravelNode::Beacon(*id)));
        nodes
    }

    /// Forgets a departed player's rides, dives and warnings (their beacons, built lifts and
    /// charges stay theirs).
    pub fn forget_transient(&mut self, player: PlayerId) {
        self.rides.remove(&player);
        self.dives.remove(&player);
        self.gate_warned.remove(&player);
        self.supply_effects.forget(player);
    }
}

/// `price` scaled for Descent `descent` ([`crate::depth_progression::descent_price_percent`]).
#[must_use]
pub const fn scaled_price(price: u32, descent: u8) -> u32 {
    price.saturating_mul(crate::depth_progression::descent_price_percent(descent)) / 100
}

/// What travelling `rows` between two stops costs: `(fuel, credits)`.
///
/// Cheap enough for everyday trips (a 350-row jump is 8 fuel and 27 credits, a fraction of a
/// Crust haul); across the whole 13,000-row dig it is about 170 fuel and 660 credits, next to
/// the climb's thousands of fuel it replaces.
#[must_use]
pub const fn travel_cost(rows: i32) -> (u32, u32) {
    let rows = rows.unsigned_abs();
    (4 + rows / 80, 10 + rows / 20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rides_are_tick_exact_and_rate_independent() {
        let ride = LiftRide {
            shaft: 0,
            from_row: 100,
            to_row: 400,
            start_tick: SimulationTick::new(10),
            ticks: 300,
        };
        assert!((ride.row_at(SimulationTick::new(10)) - 100.0).abs() < 1e-9);
        assert!((ride.row_at(SimulationTick::new(160)) - 250.0).abs() < 1e-9);
        assert!((ride.row_at(SimulationTick::new(10_000)) - 400.0).abs() < 1e-9);
        assert!(!ride.arrived(SimulationTick::new(309)));
        assert!(ride.arrived(SimulationTick::new(310)));
        assert_eq!(ride_millis(300, 20), 15_000);
        assert_eq!(ride_millis(-45, 45), 1_000);
    }

    #[test]
    fn chained_lifts_share_their_meeting_station() {
        let mut logistics = Logistics::default();
        for (top, bottom) in [(12, 549), (549, 1_139)] {
            let id = logistics.allocate_id();
            logistics.lifts.insert(
                id,
                LiftShaft {
                    id,
                    column: 40,
                    stations: [top, bottom].into_iter().collect(),
                    origin: LiftOrigin::Ancient,
                    restored: true,
                    cab_row: bottom,
                },
            );
        }
        let meeting = TilePosition { x: 40, y: 549 };
        assert_eq!(logistics.stations_at(meeting), vec![(0, 549), (1, 549)]);
        assert_eq!(
            logistics.stations_at(TilePosition { x: 40, y: 12 }),
            vec![(0, 12)]
        );
        assert_eq!(logistics.stations_at(TilePosition { x: 41, y: 549 }), []);
    }

    #[test]
    fn travel_costs_grow_with_distance() {
        let mut last = travel_cost(0);
        for rows in [60, 600, 1_100, 4_000, 13_000] {
            let cost = travel_cost(rows);
            assert!(cost.0 >= last.0 && cost.1 > last.1, "{rows}");
            last = cost;
        }
        assert_eq!(travel_cost(-600), travel_cost(600));
        let (fuel, credits) = travel_cost(13_000);
        assert!(fuel < 300 && credits < 4_000);
    }
}
