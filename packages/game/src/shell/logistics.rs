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
    far_strange::{FarStrangeAction, LampFilter, TIME_BUBBLE_PACK},
    living_dark::{CourtService, LivingDark, LivingDarkAction, ROOT_AXE_PACK, SwarmState},
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
    /// An outpost's upgrade counter (Wave 8 deep shops: the tiers its Descent needs).
    Shop,
    /// An outpost's workshop (Wave 8: layer resources into consumables and parts).
    Workshop,
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
    /// A longer word on the row for the footer (Wave 8 shop and workshop rows), if any.
    pub about: Option<String>,
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
            about: None,
        });
    };
    let mut abouts: Vec<(LogisticsCommand, String)> = Vec::new();
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
        // Wave 8: the deep shop and the workshop.
        for (label, detail, command, group, about) in outpost_trade_rows(world, rig, outpost) {
            row(group, label, detail, command);
            abouts.push((command, about));
        }
    }
    // A lift station (chained lifts share one, so every shaft stopping here is listed).
    for (shaft, at) in logistics.stations_at(tile) {
        let Some(lift) = logistics.lifts.get(&shaft) else {
            continue;
        };
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
    // The Amber Court's counter in its hall (Wave 8, Deep Descent worlds).
    if world.at_court(player) {
        let living = world.living_dark();
        "The Amber Court".clone_into(&mut heading);
        subtitle = court_subtitle(living);
        for service in CourtService::ALL {
            let detail = court_detail(living, service);
            row(
                LogisticsGroup::Here,
                service.label().to_owned(),
                detail,
                LogisticsCommand::LivingDark(LivingDarkAction::Court(service)),
            );
        }
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
    // The Living Dark's root axes (Wave 8, Deep Descent worlds).
    if world.descent_strata_layout().is_some() {
        let living = world.living_dark();
        row(
            LogisticsGroup::FieldKit,
            "Fire a root axe".to_owned(),
            format!(
                "Cuts a shaft up that roots never close ({} aboard)",
                living.axes_of(player)
            ),
            LogisticsCommand::LivingDark(LivingDarkAction::FireRootAxe),
        );
        row(
            LogisticsGroup::FieldKit,
            "Buy root axes".to_owned(),
            format!(
                "{} axes, at outposts, in town and at the Court",
                ROOT_AXE_PACK.0
            ),
            LogisticsCommand::LivingDark(LivingDarkAction::BuyRootAxes),
        );
    }
    // The far Strange Layers (Wave 9): lamp filters from Descent VI on, time bubbles to buy.
    if world.descent_strata_layout().is_some() && descent >= 5 {
        let filter = world.lamp_filter_of(player);
        for choice in LampFilter::ALL {
            let label = if choice == filter {
                format!("Lamp filter: {} (fitted)", choice.name())
            } else {
                format!("Lamp filter: {}", choice.name())
            };
            row(
                LogisticsGroup::FieldKit,
                label,
                "Keyed walls open only to light of their colour".to_owned(),
                LogisticsCommand::FarStrange(FarStrangeAction::SetFilter(choice)),
            );
        }
        row(
            LogisticsGroup::FieldKit,
            "Buy time bubbles".to_owned(),
            format!(
                "{} bubbles of slow time for creatures, at outposts and in town",
                TIME_BUBBLE_PACK.0
            ),
            LogisticsCommand::FarStrange(FarStrangeAction::BuyTimeBubbles),
        );
    }
    for (command, about) in abouts {
        if let Some(found) = rows.iter_mut().find(|row| row.command == command) {
            found.about = Some(about);
        }
    }
    SeatLogistics {
        title: heading,
        subtitle,
        rows,
    }
}

/// One deep shop or workshop row: label, detail, command, group and the footer's word.
type TradeRow = (String, String, LogisticsCommand, LogisticsGroup, String);

/// An outpost's deep shop and workshop rows (Wave 8): every line whose next tier the outpost
/// stocks (locked ones too, so the counter says why, but not rig systems nobody has reached),
/// then the recipes its workshop knows.
fn outpost_trade_rows(
    world: &WorldState,
    rig: &fatherload_simulation::Player,
    outpost: &fatherload_simulation::logistics::Outpost,
) -> Vec<TradeRow> {
    use fatherload_simulation::{
        depth_progression::outpost_stocks,
        economy::{
            UpgradeKind, UpgradeLock, shop_line_count, upgrade_effect, upgrade_lock_for,
            upgrade_maxed, upgrade_offers_for, upgrade_tier_name,
        },
        item::ItemKind,
        workshop::{CraftOutput, RECIPES, outpost_crafts},
    };
    let mut rows = Vec::new();
    let rules = world.progression_rules();
    if !rules.is_descent() {
        return rows;
    }
    for (index, offer) in upgrade_offers_for(rig, rules)
        .iter()
        .enumerate()
        .take(shop_line_count(rules))
    {
        let next = offer.level.saturating_add(1);
        if upgrade_maxed(offer) || !outpost_stocks(outpost.descent, offer.kind, next) {
            continue;
        }
        // A rig system nobody has reached yet stays off the counter.
        if matches!(
            upgrade_lock_for(
                offer,
                world.world_settings(),
                world.progression().deepest_tile_reached,
                rules,
                world.logistics().felt_systems,
            ),
            Some(UpgradeLock::Depth { .. })
        ) {
            continue;
        }
        let detail = if offer.kind == UpgradeKind::BombPack {
            upgrade_tier_name(offer.kind, offer.level).to_owned()
        } else {
            format!(
                "Tier {next}/{} {}",
                offer.max_level,
                upgrade_tier_name(offer.kind, offer.level)
            )
        };
        let about = upgrade_effect(rig, offer.kind).map_or_else(
            || offer.description.to_owned(),
            |effect| format!("{effect}. {}.", offer.description),
        );
        rows.push((
            offer.name.to_owned(),
            detail,
            LogisticsCommand::OutpostUpgrade {
                outpost: outpost.id,
                offer: u8::try_from(index).unwrap_or(u8::MAX),
            },
            LogisticsGroup::Shop,
            about,
        ));
    }
    for (index, recipe) in RECIPES.iter().enumerate() {
        if !outpost_crafts(outpost.descent, recipe) {
            continue;
        }
        let needs: Vec<String> = recipe
            .inputs
            .iter()
            .map(|(stratum, count)| {
                format!(
                    "{count} {} ({} aboard)",
                    stratum.def().layer_resource.name,
                    rig.carried(ItemKind::Layer(*stratum))
                )
            })
            .collect();
        let makes = match recipe.output {
            CraftOutput::Part(kind) => format!("the next {} tier", kind.name()),
            CraftOutput::Repair { percent } => format!("patches {percent}% of the hull"),
            CraftOutput::Refuel { percent } => format!("fills {percent}% of the tank"),
            CraftOutput::DiveCharges(count) => format!("{count} dive charges"),
            CraftOutput::Bombs(count) => format!("{count} blasting charges"),
        };
        rows.push((
            recipe.name.to_owned(),
            format!("{}; {makes}", needs.join(" + ")),
            LogisticsCommand::Craft {
                outpost: outpost.id,
                recipe: u8::try_from(index).unwrap_or(u8::MAX),
            },
            LogisticsGroup::Workshop,
            format!("{}.", recipe.about),
        ));
    }
    rows
}

/// The Amber Court's mood toward the crew, in its steward's words.
fn court_subtitle(living: &LivingDark) -> String {
    let hive = &living.hive;
    let mood = if hive.court_hostile() {
        "Steward Ixa clicks her mandibles: \"Thieves. Pay for what you took.\""
    } else if hive.honoured() {
        "Steward Ixa bows her antennae: \"The Court honours you, digger.\""
    } else if hive.standing > 0 {
        "Steward Ixa hums: \"Welcome back to the Court, digger.\""
    } else {
        "Steward Ixa watches you: \"The Court does not know you yet.\""
    };
    let swarm = match hive.swarm {
        SwarmState::Raiding => "The Swarm raids the Hive.",
        SwarmState::Truce => "The Swarm keeps its bought peace.",
        SwarmState::Broken => "The Swarm is broken.",
    };
    format!("{mood} Standing {}. {swarm}", hive.standing)
}

/// What a Court service row says.
fn court_detail(living: &LivingDark, service: CourtService) -> String {
    let hive = &living.hive;
    match service {
        CourtService::RoyalJelly => "Mends the hull, a quarter tank".to_owned(),
        CourtService::TakeErrand => hive.next_errand().map_or_else(
            || {
                hive.errand.map_or_else(
                    || "No errand now".to_owned(),
                    |errand| format!("Under way: {}", errand.title()),
                )
            },
            |errand| format!("{}: {}", errand.title(), errand.brief()),
        ),
        CourtService::TurnIn => hive.errand.map_or_else(
            || "No errand under way".to_owned(),
            |errand| {
                format!(
                    "{} ({}/{})",
                    errand.title(),
                    hive.progress.min(errand.goal()),
                    errand.goal()
                )
            },
        ),
        CourtService::BrokerTruce => {
            "Pay the Swarm to stand down instead of slaying the matriarch".to_owned()
        }
        CourtService::MakeAmends => "Pay for what was taken".to_owned(),
    }
}

/// The use prompt at a logistics stop (an outpost, a lift station, one's own beacon), if the
/// rig stands at one: what the use button opens there.
#[must_use]
pub fn stop_prompt(world: &WorldState, player: PlayerId) -> Option<String> {
    if world.is_on_foot(player) || world.logistics().is_riding(player) {
        return None;
    }
    if world.at_court(player) {
        return Some("Visit the Amber Court".to_owned());
    }
    // Beside a keyed wall in the Prism Desert, the use button swaps the lamp filter (Wave 9).
    if world.filter_swap_here(player) && !at_logistics_stop(world, player) {
        let next = world.lamp_filter_of(player).next();
        return Some(format!("Swap the lamp to {}", next.name()));
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
        LogisticsCommand::OutpostUpgrade { offer, .. } => {
            fatherload_simulation::economy::UpgradeKind::ALL
                .get(usize::from(offer))
                .map_or_else(
                    || "The outpost's mechanic gets to work.".to_owned(),
                    |kind| format!("The outpost's mechanic fits the new {}.", kind.name()),
                )
        }
        LogisticsCommand::Craft { recipe, .. } => fatherload_simulation::workshop::recipe(recipe)
            .map_or_else(
                || "The workshop hums.".to_owned(),
                |found| format!("The workshop turns out {}.", found.name),
            ),
        LogisticsCommand::Hollow(action) => match action {
            fatherload_simulation::hollow::HollowAction::EngageAnchor => {
                "Gravity Anchor engaged: down is where the rig's feet are.".to_owned()
            }
            fatherload_simulation::hollow::HollowAction::ReleaseAnchor => {
                "Gravity Anchor released.".to_owned()
            }
            fatherload_simulation::hollow::HollowAction::ToggleAnchor => {
                "Gravity Anchor.".to_owned()
            }
        },
        LogisticsCommand::FarStrange(action) => match action {
            FarStrangeAction::CycleFilter => "The lamp's filter turns.".to_owned(),
            FarStrangeAction::SetFilter(filter) => format!("Lamp filter: {}.", filter.name()),
            FarStrangeAction::BuyTimeBubbles => "Time bubbles racked.".to_owned(),
        },
        LogisticsCommand::LivingDark(action) => match action {
            LivingDarkAction::BuyRootAxes => "Root axes racked.".to_owned(),
            LivingDarkAction::FireRootAxe => {
                "The axe bites up through the roots: that shaft stays open.".to_owned()
            }
            LivingDarkAction::Court(service) => match service {
                CourtService::RoyalJelly => {
                    "Royal jelly: the hull knits and the tank fills a little.".to_owned()
                }
                CourtService::TakeErrand => world.living_dark().hive.next_errand().map_or_else(
                    || "The steward has a task for you.".to_owned(),
                    |errand| format!("Errand taken: {}.", errand.title()),
                ),
                CourtService::TurnIn => "The Court rewards the crew.".to_owned(),
                CourtService::BrokerTruce => {
                    "Tribute paid. The Swarm stands down; the Court looks away.".to_owned()
                }
                CourtService::MakeAmends => "The Court accepts the crew's amends.".to_owned(),
            },
        },
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
            || ctx.world.at_court(player)
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

    /// Beside a keyed wall in the Prism Desert, the use button swaps the rig's lamp filter
    /// (Wave 9). Returns whether it did.
    pub(super) fn try_swap_filter(
        &mut self,
        ctx: SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) -> bool {
        let Some(player) = ctx.player else {
            return false;
        };
        if !ctx.world.filter_swap_here(player) {
            return false;
        }
        let next = ctx.world.lamp_filter_of(player).next();
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command: PlayerCommand::Logistics(LogisticsCommand::FarStrange(
                FarStrangeAction::CycleFilter,
            )),
        });
        self.message = format!("Lamp filter: {}.", next.name());
        cues.push(SoundCue::Ui);
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

/// Whether `player`'s rig stands at a stop the use button opens the dialog at (an outpost, a
/// lift station, one of its own beacons).
fn at_logistics_stop(world: &WorldState, player: PlayerId) -> bool {
    let Some(rig) = world.player(player) else {
        return false;
    };
    let tile = rig.tile_in(world.frame());
    let logistics = world.logistics();
    logistics.outpost_at(tile).is_some()
        || logistics.station_at(tile).is_some()
        || logistics
            .beacon_near(tile)
            .is_some_and(|beacon| beacon.owner == player)
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
