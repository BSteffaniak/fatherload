//! A docked station's hub (Wave 12): the hub page and its four counters (shipyard, market,
//! bounty board, bar), plus refuel, repair, salvage, save and undocking.
//!
//! Every page is a list of rows, so it works by keys, pad, mouse and touch alike (a tap selects
//! a row and confirms it). A counter's **Back** row (or the back button) returns to the hub; on
//! the hub it closes. Every purchase, trade and mission goes to the simulation as a
//! [`PlayerCommand::Flight`] for the seat's player, so it works the same in split screen (each
//! seat on its own page, at its own station) and online. Abandoning a mission takes a second
//! confirm on the same row.
//!
//! Wave 13 endgame: the **Works** counter (space materials, alien tech marks, the Starwright
//! ship, the Heartline Drive, the Halcyon Gate and its jump, the outfitter) and the **Records**
//! (the hunt list, whose rows point the waypoint at a boss's lair, and completion).

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    endgame::{
        WorkOrder,
        materials::station_stock,
        works::{GATE_PARTS, OUTFITTER, StarwrightPart, builds_starwright, has_outfitter},
    },
    flight::{
        FlightCommand, FlightGearKind, FlightPlace, StationService, SystemRef, nav::NavTarget,
    },
    station::{
        AlienTech, Good, ShipUpgrade, StationAction, StationProfile, bar::PATRONS,
        bounty::BOARD_SLOTS, market::goods_of,
    },
    terrain::MineralKind,
};

use super::{SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// A page of the hub.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StationPage {
    #[default]
    Hub,
    Shipyard,
    Market,
    Board,
    Bar,
    /// The Works (Wave 13 endgame): materials, marks, the Starwright ship, the gate, outfits.
    Works,
    /// The hunt list and completion (Wave 13 endgame).
    Records,
}

impl StationPage {
    /// The page's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Hub => "Concourse",
            Self::Shipyard => "Shipyard",
            Self::Market => "Market",
            Self::Board => "Bounty Board",
            Self::Bar => "The Bar",
            Self::Works => "The Works",
            Self::Records => "Records",
        }
    }
}

/// One row of a hub page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StationRow {
    /// Go to a counter.
    Open(StationPage),
    Refuel,
    Repair,
    /// Wave 11's flight gear (the shipyard page).
    Buy(FlightGearKind),
    /// A ship line, planet gear or alien tech.
    Fit(ShipUpgrade),
    SellOre,
    BuyGood(Good),
    SellGood(Good),
    /// A mission under way (a second confirm abandons it).
    Mission(u8),
    /// A board slot's mission.
    Offer(u8),
    /// Collect finished missions (and hand deliveries over here).
    TurnIn,
    /// A patron at the bar (confirm: buy a round for their rumour).
    Patron(u8),
    Salvage,
    /// Save the game (quick save).
    Save,
    Undock,
    /// Back to the hub.
    Back,
    Close,
    /// Space weapons and the deflector (Wave 12 hostiles, sold at the shipyard).
    Arm(fatherload_simulation::hostiles::ShipGearKind),
    /// Ten units of a common space material (Wave 13 endgame).
    Material(MineralKind),
    /// A Works order: a mark, a Starwright part, the Heartline, a gate part, an outfit.
    Order(WorkOrder),
    /// Ride the finished gate to a known system.
    GateJump(SystemRef),
    /// The hunt list's entry `index` (confirm: the waypoint to its lair).
    Quarry(u8),
    /// Completion's category `index` (a read-out).
    Record(u8),
}

/// How many units a Material row buys.
pub const MATERIAL_LOT: u16 = 10;

/// Whether `station` has anything at the Works counter.
#[must_use]
pub fn has_works(station: &StationProfile) -> bool {
    !station_stock(station).is_empty()
        || station.alien_tech
        || builds_starwright(station)
        || station.halcyon
        || has_outfitter(station)
}

/// The Works rows for `player` at `station`.
fn works_rows(world: &WorldState, player: PlayerId, station: &StationProfile) -> Vec<StationRow> {
    let mut rows: Vec<StationRow> = station_stock(station)
        .into_iter()
        .map(|(mineral, _)| StationRow::Material(mineral))
        .collect();
    if station.alien_tech {
        let gear = world.ship_gear(player);
        rows.extend(
            AlienTech::ALL
                .into_iter()
                .filter(|tech| gear.has(*tech))
                .map(|tech| StationRow::Order(WorkOrder::AlienMark(tech))),
        );
    }
    if builds_starwright(station) {
        rows.extend(
            StarwrightPart::ALL
                .into_iter()
                .map(|part| StationRow::Order(WorkOrder::Starwright(part))),
        );
        rows.push(StationRow::Order(WorkOrder::Heartline));
    }
    if station.halcyon {
        let endgame = world.endgame();
        rows.extend(
            (0..GATE_PARTS)
                .filter(|part| !endgame.gate_part(*part))
                .map(|part| StationRow::Order(WorkOrder::GatePart(part))),
        );
        if endgame.gate_open() {
            // The farthest known systems first (where a ride saves the most flight; review:
            // the first eight in chart order left a late crew's far finds off the list).
            let home = fatherload_simulation::flight::space::HOME_SYSTEM;
            let mut known: Vec<SystemRef> = world
                .known_systems()
                .into_iter()
                .filter(|system| *system != home)
                .collect();
            let reach = |system: &SystemRef| {
                (system.sector.0 - home.sector.0)
                    .abs()
                    .max((system.sector.1 - home.sector.1).abs())
            };
            known.sort_by_key(|system| (std::cmp::Reverse(reach(system)), system.id));
            rows.extend(known.into_iter().take(8).map(StationRow::GateJump));
        }
    }
    if has_outfitter(station) {
        rows.extend(
            OUTFITTER
                .iter()
                .map(|(item, _)| StationRow::Order(WorkOrder::Cosmetic(*item))),
        );
    }
    rows
}

/// The seat's hub state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SeatStation {
    pub page: StationPage,
    /// The mission row confirmed once (a second confirm abandons it).
    pub abandon_armed: Option<u8>,
}

impl SeatStation {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            page: StationPage::Hub,
            abandon_armed: None,
        }
    }
}

/// The station `player` is docked at.
#[must_use]
pub fn docked_profile(world: &WorldState, player: PlayerId) -> Option<StationProfile> {
    world.docked_station(player)
}

/// The rows of `page` for `player` docked at `station`.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one arm per row reads best together")]
pub fn station_rows(
    world: &WorldState,
    player: PlayerId,
    station: &StationProfile,
    page: StationPage,
) -> Vec<StationRow> {
    let mut rows = Vec::new();
    let ledger = world.station_ledger();
    match page {
        StationPage::Hub => {
            if station.yard_tier > 0
                || station.gear_tier > 0
                || station.alien_tech
                || station.dock_services
            {
                rows.push(StationRow::Open(StationPage::Shipyard));
            }
            if station.market {
                rows.push(StationRow::Open(StationPage::Market));
            }
            if station.bounties || !ledger.missions_of(player).is_empty() {
                rows.push(StationRow::Open(StationPage::Board));
            }
            if station.bar {
                rows.push(StationRow::Open(StationPage::Bar));
            }
            // Wave 13 endgame: the Works and the records.
            if has_works(station) {
                rows.push(StationRow::Open(StationPage::Works));
            }
            if station.owner.crewed() {
                rows.push(StationRow::Open(StationPage::Records));
            }
            if station.salvage {
                rows.push(StationRow::Salvage);
            }
            if station.dock_services {
                rows.extend([StationRow::Refuel, StationRow::Repair]);
            }
            rows.extend([StationRow::Save, StationRow::Undock, StationRow::Close]);
        }
        StationPage::Shipyard => {
            rows.extend(
                ShipUpgrade::all()
                    .into_iter()
                    .filter(|upgrade| upgrade.sold_to(station) > 0)
                    .map(StationRow::Fit),
            );
            if station.dock_services {
                rows.extend(FlightGearKind::ALL.into_iter().map(StationRow::Buy));
            }
            // The hostile factions' answer: cannon, turret, missiles and deflector at any yard
            // that fits ship lines (Wave 12 integration).
            if station.yard_tier > 0 {
                rows.extend(
                    fatherload_simulation::hostiles::ShipGearKind::ALL
                        .into_iter()
                        .map(StationRow::Arm),
                );
            }
            rows.push(StationRow::Back);
        }
        StationPage::Market => {
            rows.push(StationRow::SellOre);
            let goods = goods_of(station, world.world_seed());
            rows.extend(goods.iter().map(|(good, _)| StationRow::BuyGood(*good)));
            if let Some(hold) = ledger.hold(player) {
                rows.extend(
                    goods
                        .iter()
                        .filter(|(good, _)| hold.count(*good) > 0)
                        .map(|(good, _)| StationRow::SellGood(*good)),
                );
            }
            rows.push(StationRow::Back);
        }
        StationPage::Board => {
            if station.owner.crewed() && !ledger.missions_of(player).is_empty() {
                rows.push(StationRow::TurnIn);
            }
            let active = u8::try_from(ledger.missions_of(player).len()).unwrap_or(u8::MAX);
            rows.extend((0..active).map(StationRow::Mission));
            if station.bounties {
                let board = world.station_board(station.id);
                rows.extend(
                    (0..BOARD_SLOTS)
                        .filter(|slot| board.get(usize::from(*slot)).is_some_and(Option::is_some))
                        .map(StationRow::Offer),
                );
            }
            rows.push(StationRow::Back);
        }
        StationPage::Bar => {
            let patrons = u8::try_from(world.station_patrons(station.id).len()).unwrap_or(PATRONS);
            rows.extend((0..patrons).map(StationRow::Patron));
            rows.push(StationRow::Back);
        }
        StationPage::Works => {
            rows.extend(works_rows(world, player, station));
            rows.push(StationRow::Back);
        }
        StationPage::Records => {
            let quarries = u8::try_from(world.hunt().len()).unwrap_or(u8::MAX);
            rows.extend((0..quarries).map(StationRow::Quarry));
            let categories = u8::try_from(world.completion(player).categories.len()).unwrap_or(0);
            rows.extend((0..categories).map(StationRow::Record));
            rows.push(StationRow::Back);
        }
    }
    rows
}

impl SeatUi {
    /// The docked station's hub.
    #[allow(clippy::too_many_lines, reason = "one arm per row reads best together")]
    pub(super) fn handle_station(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let docked = ctx
            .player
            .and_then(|player| ctx.world.rig_flight(player))
            .is_some_and(|flight| matches!(flight.place, FlightPlace::Docked(_)));
        let profile = ctx
            .player
            .and_then(|player| docked_profile(ctx.world, player));
        let (Some(player), Some(profile)) = (ctx.player, profile.filter(|_| docked)) else {
            self.modal = None;
            self.station = SeatStation::new();
            cues.push(SoundCue::UiBack);
            return;
        };
        if input.backs_out() {
            self.station_back(cues);
            return;
        }
        let rows = station_rows(ctx.world, player, &profile, self.station.page);
        let before = self.selected_menu_item;
        self.move_selection(input, rows.len().saturating_sub(1));
        self.selected_menu_item = self.selected_menu_item.min(rows.len().saturating_sub(1));
        if self.selected_menu_item != before {
            self.station.abandon_armed = None;
        }
        if !input.confirm {
            return;
        }
        let action = match rows.get(self.selected_menu_item).copied() {
            Some(StationRow::Open(page)) => {
                self.station.page = page;
                self.station.abandon_armed = None;
                self.selected_menu_item = 0;
                cues.push(SoundCue::Ui);
                return;
            }
            Some(StationRow::Back) => {
                self.station_back(cues);
                return;
            }
            Some(StationRow::Close) | None => {
                self.modal = None;
                self.station = SeatStation::new();
                cues.push(SoundCue::UiBack);
                return;
            }
            Some(StationRow::Save) => {
                requests.push(ShellRequest::QuickSave);
                cues.push(SoundCue::Ui);
                return;
            }
            Some(StationRow::Undock) => {
                self.modal = None;
                self.station = SeatStation::new();
                FlightCommand::Use
            }
            Some(StationRow::Refuel) => FlightCommand::Station(StationService::Refuel),
            Some(StationRow::Repair) => FlightCommand::Station(StationService::Repair),
            Some(StationRow::Buy(kind)) => FlightCommand::Station(StationService::Buy(kind)),
            Some(StationRow::Fit(upgrade)) => hub(StationAction::Fit(upgrade)),
            Some(StationRow::SellOre) => hub(StationAction::SellOre),
            Some(StationRow::BuyGood(good)) => hub(StationAction::BuyGood(good, 1)),
            Some(StationRow::SellGood(good)) => hub(StationAction::SellGood(good, 1)),
            Some(StationRow::TurnIn) => hub(StationAction::TurnIn),
            Some(StationRow::Offer(slot)) => hub(StationAction::Accept(slot)),
            Some(StationRow::Patron(index)) => hub(StationAction::BuyRound(index)),
            Some(StationRow::Salvage) => hub(StationAction::Salvage),
            Some(StationRow::Material(mineral)) => hub(StationAction::Works(
                WorkOrder::BuyMaterial(mineral, MATERIAL_LOT),
            )),
            Some(StationRow::Order(order)) => hub(StationAction::Works(order)),
            Some(StationRow::GateJump(system)) => {
                // The ride closes the hub: the rig is under way.
                self.modal = None;
                self.station = SeatStation::new();
                hub(StationAction::Works(WorkOrder::GateJump(system)))
            }
            Some(StationRow::Quarry(index)) => {
                let lair = ctx
                    .world
                    .hunt()
                    .get(usize::from(index))
                    .and_then(|(_, beaten, lair)| (!beaten).then_some(*lair).flatten());
                let Some(system) = lair else {
                    cues.push(SoundCue::UiMove);
                    return;
                };
                "Waypoint set on the lair.".clone_into(&mut self.message);
                FlightCommand::SetWaypoint(Some(NavTarget::System(system)))
            }
            Some(StationRow::Record(_)) => {
                cues.push(SoundCue::UiMove);
                return;
            }
            Some(StationRow::Arm(kind)) => {
                cues.push(SoundCue::Ui);
                requests.push(ShellRequest::Action {
                    seat: ctx.seat,
                    command: PlayerCommand::Combat(
                        fatherload_simulation::hostiles::CombatCommand::BuyGear(kind),
                    ),
                });
                return;
            }
            Some(StationRow::Mission(index)) => {
                if self.station.abandon_armed == Some(index) {
                    self.station.abandon_armed = None;
                    hub(StationAction::Abandon(index))
                } else {
                    self.station.abandon_armed = Some(index);
                    "Confirm again to abandon this mission.".clone_into(&mut self.message);
                    cues.push(SoundCue::UiMove);
                    return;
                }
            }
        };
        cues.push(SoundCue::Ui);
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Flight(action),
        });
    }

    /// Back from a counter to the hub, or out of the hub.
    fn station_back(&mut self, cues: &mut Vec<SoundCue>) {
        if self.station.page == StationPage::Hub {
            self.modal = None;
            self.station = SeatStation::new();
        } else {
            self.station.page = StationPage::Hub;
            self.selected_menu_item = 0;
        }
        self.station.abandon_armed = None;
        cues.push(SoundCue::UiBack);
    }
}

const fn hub(action: StationAction) -> FlightCommand {
    FlightCommand::Station(StationService::Hub(action))
}
