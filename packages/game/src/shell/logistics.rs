//! The depth logistics dialog at the seat (Part III F10, [`ModalScreen::Logistics`]).
//!
//! One dialog serves every stop of the logistics network: it opens with the use button at a
//! deep outpost, a lift station or a depth beacon, and from the inventory's "Depth logistics"
//! row anywhere else (so keyboard, gamepad and touch all reach it through inputs they already
//! have; rows are tapped like every menu's). Its rows are rebuilt from the world every frame
//! ([`logistics_rows`]): what the stop the rig is at offers (an outpost's counter, the lift's
//! other stations, restoring an ancient lift), every travel destination with its fuel and
//! credit cost, then the field kit (place a beacon or a lift station, buy or fire dive
//! charges). Each row carries the [`LogisticsCommand`] it sends and, when the simulation would
//! refuse it, the reason ([`WorldState::check_logistics`], the same rule the tick applies), so
//! the dialog works the same online, in split-screen and by touch.

use fatherload_simulation::{
    PlayerCommand, PlayerId, WorldState,
    logistics::{
        DIVE_CHARGE_PACK, LiftOrigin, LogisticsCommand, MAX_DIVE_CHARGES, OUTPOST_SALE_PERCENT,
        OutpostService, TravelNode,
    },
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::input::PlayerInput;

/// Which part of the dialog a row belongs to (its section and icon).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogisticsGroup {
    /// What the stop the rig is at offers: an outpost's counter, a lift's stations.
    Here,
    /// The travel network.
    Travel,
    /// Beacons, lift stations and dive charges, anywhere.
    FieldKit,
}

/// One row of the logistics dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogisticsRow {
    pub group: LogisticsGroup,
    pub label: String,
    pub detail: String,
    /// The price column ("240 cr", "12 fuel + 300 cr", "free").
    pub value: String,
    pub command: LogisticsCommand,
    /// Why the simulation would refuse it now, if it would.
    pub refusal: Option<String>,
}

/// A seat's logistics dialog: its heading and rows (rebuilt every frame while open).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatLogistics {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<LogisticsRow>,
}

impl SeatLogistics {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            rows: Vec::new(),
        }
    }
}

fn price_label(credits: u32, fuel: u32) -> String {
    match (credits, fuel) {
        (0, 0) => "free".to_owned(),
        (credits, 0) => format!("{credits} cr"),
        (0, fuel) => format!("{fuel} fuel"),
        (credits, fuel) => format!("{fuel} fuel + {credits} cr"),
    }
}

/// The dialog's heading and rows for `player` in `world` now.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one block per kind of stop and row")]
pub fn logistics_rows(world: &WorldState, player: PlayerId) -> SeatLogistics {
    let Some(rig) = world.player(player) else {
        return SeatLogistics::default();
    };
    let tile = rig.tile_in(world.frame());
    let logistics = world.logistics();
    let mut rows = Vec::new();
    let mut row = |group, label: String, detail: String, command: LogisticsCommand| {
        let verdict = world.check_logistics(player, &command);
        let sale = match command {
            LogisticsCommand::Outpost {
                outpost,
                service: OutpostService::SellCargo,
            } => logistics
                .outposts
                .get(&outpost)
                .map(|found| format!("{}% paid", OUTPOST_SALE_PERCENT[found.tier()])),
            _ => None,
        };
        let (value, refusal) = match verdict {
            Ok(_) if sale.is_some() => (sale.unwrap_or_default(), None),
            Ok(quote) if quote.refund > 0 => (format!("+{} cr", quote.refund), None),
            Ok(quote) => (price_label(quote.credits, quote.fuel), None),
            Err(refusal) => ("-".to_owned(), Some(refusal.message())),
        };
        rows.push(LogisticsRow {
            group,
            label,
            detail,
            value,
            command,
            refusal,
        });
    };
    let descent = world.descent_of_row(tile.y);
    let mut heading = "Depth Logistics".to_owned();
    let mut subtitle = format!(
        "Row {} of Descent {}. Lifts, beacons and dive charges keep the long dig moving.",
        tile.y,
        fatherload_simulation::depth_progression::numeral(descent)
    );
    // An outpost's counter.
    if let Some(outpost) = logistics.outpost_at(tile) {
        heading.clone_from(&outpost.name);
        subtitle = if outpost.claimed() {
            "A deep outpost the crew holds: fair prices and a bed to wake up in.".to_owned()
        } else {
            "A deep outpost: fuel, repairs and a buyer, at a fee until the crew claims it."
                .to_owned()
        };
        for service in OutpostService::ALL {
            let detail = match service {
                OutpostService::Refuel => "Top up the tank".to_owned(),
                OutpostService::Repair => "Patch the hull".to_owned(),
                OutpostService::SellCargo => {
                    if outpost.claimed() {
                        "The crew's buyer takes a small cut".to_owned()
                    } else {
                        "The buyer keeps a quarter".to_owned()
                    }
                }
                OutpostService::BuyDiveCharges => format!(
                    "{} charges ({} aboard, {MAX_DIVE_CHARGES} max)",
                    DIVE_CHARGE_PACK.0,
                    logistics.dive_charges_of(player)
                ),
                OutpostService::Claim => match outpost.claimed_by {
                    None => "Cheaper services and a respawn point for the crew".to_owned(),
                    Some(owner) if owner == player => "You claimed it for the crew".to_owned(),
                    Some(_) => "A crewmate claimed it".to_owned(),
                },
                OutpostService::SetHome => "Rescues bring the rig back here".to_owned(),
            };
            row(
                LogisticsGroup::Here,
                service.label().to_owned(),
                detail,
                LogisticsCommand::Outpost {
                    outpost: outpost.id,
                    service,
                },
            );
        }
    }
    // A lift station.
    if let Some((shaft, at)) = logistics.station_at(tile)
        && let Some(lift) = logistics.lifts.get(&shaft)
    {
        if logistics.outpost_at(tile).is_none() {
            heading = if lift.is_ancient() {
                "Ancient Lift".to_owned()
            } else {
                "Lift Station".to_owned()
            };
            subtitle = if lift.is_ancient() && !lift.restored {
                "A builders' lift, dead for an age. Its machinery waits at the lowest station."
                    .to_owned()
            } else {
                format!("Station at row {at}. The cab carries the rig; no fuel burns.")
            };
        }
        if lift.is_ancient() && !lift.restored {
            row(
                LogisticsGroup::Here,
                "Restore the lift".to_owned(),
                "Wake the cab: every station runs from then on".to_owned(),
                LogisticsCommand::RestoreLift { shaft },
            );
        }
        for &to_row in &lift.stations {
            if to_row == at {
                continue;
            }
            let direction = if to_row < at { "Up" } else { "Down" };
            row(
                LogisticsGroup::Here,
                format!("{direction} to row {to_row}"),
                format!("{} rows", (to_row - at).abs()),
                LogisticsCommand::RideLift { shaft, to_row },
            );
        }
        if lift.origin == (LiftOrigin::Built { owner: player }) {
            row(
                LogisticsGroup::Here,
                "Take down this station".to_owned(),
                "Half its price back".to_owned(),
                LogisticsCommand::RemoveLiftStation { shaft, row: at },
            );
        }
    }
    // A beacon of the player's own: take it down.
    if let Some(beacon) = logistics.beacon_near(tile)
        && beacon.owner == player
    {
        if logistics.outpost_at(tile).is_none() && logistics.station_at(tile).is_none() {
            "Depth Beacon".clone_into(&mut heading);
        }
        row(
            LogisticsGroup::Here,
            "Take down this beacon".to_owned(),
            "Half its price back".to_owned(),
            LogisticsCommand::RemoveBeacon { beacon: beacon.id },
        );
    }
    // The travel network.
    let here = world.travel_stop_of(player);
    if here.is_some() {
        for node in logistics.travel_nodes() {
            if Some(node) == here {
                continue;
            }
            let Some((landing, _, _)) = world.travel_landing(node) else {
                continue;
            };
            let rows = landing.y - tile.y;
            let detail = match (node, rows) {
                (TravelNode::Surface, _) => "Back to the surface".to_owned(),
                (_, 0) => "At this depth".to_owned(),
                (_, rows) if rows < 0 => format!("{} rows up", -rows),
                (_, rows) => format!("{rows} rows down"),
            };
            row(
                LogisticsGroup::Travel,
                format!("Travel to {}", world.travel_stop_name(node)),
                detail,
                LogisticsCommand::Travel { to: node },
            );
        }
    }
    // The field kit.
    row(
        LogisticsGroup::FieldKit,
        "Place a depth beacon".to_owned(),
        format!(
            "A stop on the travel network ({} placed)",
            logistics.beacons_of(player).count()
        ),
        LogisticsCommand::PlaceBeacon,
    );
    row(
        LogisticsGroup::FieldKit,
        "Place a lift station".to_owned(),
        "Stations on one dug shaft make a lift".to_owned(),
        LogisticsCommand::PlaceLiftStation,
    );
    row(
        LogisticsGroup::FieldKit,
        "Fire a dive charge".to_owned(),
        format!(
            "Cuts a shaft straight down; a soft landing ({} aboard)",
            logistics.dive_charges_of(player)
        ),
        LogisticsCommand::FireDiveCharge,
    );
    if logistics.outpost_at(tile).is_none() {
        row(
            LogisticsGroup::FieldKit,
            "Buy dive charges".to_owned(),
            format!("{} charges, in town and at outposts", DIVE_CHARGE_PACK.0),
            LogisticsCommand::BuyDiveCharges,
        );
    }
    SeatLogistics {
        title: heading,
        subtitle,
        rows,
    }
}

/// The use prompt at a logistics stop (an outpost, a lift station, one's own beacon), if the
/// rig stands at one: what the use button opens there.
#[must_use]
pub fn stop_prompt(world: &WorldState, player: PlayerId) -> Option<String> {
    if world.is_on_foot(player) || world.logistics().is_riding(player) {
        return None;
    }
    let tile = world.player(player)?.tile_in(world.frame());
    let logistics = world.logistics();
    if let Some(outpost) = logistics.outpost_at(tile) {
        return Some(format!("Trade at {}", outpost.name));
    }
    if let Some((shaft, _)) = logistics.station_at(tile) {
        let restored = logistics
            .lifts
            .get(&shaft)
            .is_some_and(|lift| lift.restored);
        return Some(if restored {
            "Call the lift".to_owned()
        } else {
            "Inspect the ancient lift".to_owned()
        });
    }
    logistics
        .beacon_near(tile)
        .filter(|beacon| beacon.owner == player)
        .map(|_| "Use the depth beacon".to_owned())
}

/// The toast after `command` is sent.
fn sent_message(world: &WorldState, command: LogisticsCommand) -> String {
    match command {
        LogisticsCommand::RideLift { to_row, .. } => {
            format!("The cab shudders and sets off for row {to_row}.")
        }
        LogisticsCommand::RestoreLift { .. } => {
            "The builders' machinery wakes with a groan. The lift runs.".to_owned()
        }
        LogisticsCommand::PlaceLiftStation => {
            "Lift station bolted to the shaft. Two make a lift.".to_owned()
        }
        LogisticsCommand::RemoveLiftStation { .. } => "Station taken down.".to_owned(),
        LogisticsCommand::PlaceBeacon => "Depth beacon planted: it joins the network.".to_owned(),
        LogisticsCommand::RemoveBeacon { .. } => "Beacon taken down.".to_owned(),
        LogisticsCommand::Travel { to } => {
            format!("Beacon jump to {}.", world.travel_stop_name(to))
        }
        LogisticsCommand::Outpost { service, .. } => match service {
            OutpostService::Refuel => "Fuel lines hiss.".to_owned(),
            OutpostService::Repair => "The outpost's welders patch the hull.".to_owned(),
            OutpostService::SellCargo => "The buyer weighs the haul and pays up.".to_owned(),
            OutpostService::BuyDiveCharges | OutpostService::Claim => match service {
                OutpostService::Claim => "The crew's flag goes up over the outpost.".to_owned(),
                _ => "Dive charges racked.".to_owned(),
            },
            OutpostService::SetHome => "Rescues will bring you back here.".to_owned(),
        },
        LogisticsCommand::BuyDiveCharges => "Dive charges racked.".to_owned(),
        LogisticsCommand::FireDiveCharge => "Charge away: dive!".to_owned(),
    }
}

impl SeatUi {
    /// Opens the logistics dialog if the seat's rig stands at an outpost, a lift station or one
    /// of its own beacons. Returns whether it opened.
    pub(super) fn open_logistics_here(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        if ctx.world.is_on_foot(player) {
            return false;
        }
        let Some(rig) = ctx.world.player(player) else {
            return false;
        };
        let tile = rig.tile_in(ctx.world.frame());
        let logistics = ctx.world.logistics();
        let at_stop = logistics.outpost_at(tile).is_some()
            || logistics.station_at(tile).is_some()
            || logistics
                .beacon_near(tile)
                .is_some_and(|beacon| beacon.owner == player);
        if !at_stop {
            return false;
        }
        self.open_logistics(ctx, cues);
        true
    }

    /// Opens the logistics dialog wherever the rig is (the inventory's row).
    pub(super) fn open_logistics(&mut self, ctx: SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        let Some(player) = ctx.player else {
            return;
        };
        self.logistics = logistics_rows(ctx.world, player);
        self.modal = Some(ModalScreen::Logistics);
        self.selected_menu_item = 0;
        cues.push(SoundCue::Ui);
    }

    /// One frame of the logistics dialog.
    pub(super) fn handle_logistics(
        &mut self,
        input: PlayerInput,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(player) = ctx.player else {
            self.modal = None;
            return;
        };
        if input.backs_out() || ctx.world.logistics().is_riding(player) {
            self.modal = None;
            cues.push(SoundCue::UiBack);
            return;
        }
        if ctx.world.is_on_foot(player) {
            self.modal = None;
            "Board your rig first.".clone_into(&mut self.message);
            return;
        }
        self.logistics = logistics_rows(ctx.world, player);
        let count = self.logistics.rows.len();
        if count == 0 {
            self.modal = None;
            return;
        }
        self.move_selection(input, count - 1);
        self.selected_menu_item = self.selected_menu_item.min(count - 1);
        if !input.confirm && !input.interact {
            return;
        }
        let row = self.logistics.rows[self.selected_menu_item].clone();
        if let Some(refusal) = row.refusal {
            self.message = refusal;
            cues.push(SoundCue::UiError);
            return;
        }
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Logistics(row.command),
        });
        self.message = sent_message(ctx.world, row.command);
        cues.push(SoundCue::Ui);
        // The rig moves away (a ride, a jump, a dive): the dialog closes.
        if matches!(
            row.command,
            LogisticsCommand::RideLift { .. }
                | LogisticsCommand::Travel { .. }
                | LogisticsCommand::FireDiveCharge
        ) {
            self.modal = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::terrain::{TILE_SIZE, TileKind, TilePosition};

    #[test]
    fn the_rows_offer_the_field_kit_anywhere_and_travel_at_a_stop() {
        let player = PlayerId::new(0);
        let mut world = WorldState::new_game(player);
        world.player_mut(player).expect("rig").credits = 50_000;
        // Deep in the rock, in a pocket: the field kit, no travel.
        for y in 59..=61 {
            for x in 29..=31 {
                let kind = if y == 61 {
                    TileKind::Stone
                } else {
                    TileKind::Air
                };
                let _ = world.terrain_mut().set_kind(TilePosition { x, y }, kind);
            }
        }
        {
            let rig = world.player_mut(player).expect("rig");
            rig.x = 30.5 * TILE_SIZE;
            rig.y = 60.5 * TILE_SIZE;
        }
        let menu = logistics_rows(&world, player);
        assert!(
            menu.rows
                .iter()
                .all(|row| row.group != LogisticsGroup::Travel)
        );
        let beacon = menu
            .rows
            .iter()
            .find(|row| row.command == LogisticsCommand::PlaceBeacon)
            .expect("beacon row");
        assert_eq!(beacon.refusal, None);
        assert!(beacon.value.ends_with("cr"));
        // In town: the network's travel rows appear (none yet but the town itself).
        let spawn = world.player_template().clone();
        {
            let rig = world.player_mut(player).expect("rig");
            rig.x = spawn.x;
            rig.y = spawn.y;
        }
        let menu = logistics_rows(&world, player);
        let buy = menu
            .rows
            .iter()
            .find(|row| row.command == LogisticsCommand::BuyDiveCharges)
            .expect("buy row");
        assert_eq!(buy.refusal, None);
        assert!(
            menu.rows
                .iter()
                .any(|row| row.command == LogisticsCommand::PlaceBeacon && row.refusal.is_some()),
            "too shallow for a beacon in town"
        );
    }
}
