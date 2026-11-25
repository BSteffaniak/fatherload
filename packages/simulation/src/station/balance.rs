//! The space economy's first hour: a model of a first trip out from Halcyon Dock, built from
//! the real board, the real prices and the flight times the Wave 11 journey measured.
//!
//! A rig reaches space after the Heart (the home world's credits long spent on the home
//! world's upgrades), so the model starts from nothing. Its first hour: take Halcyon Dock's
//! starter board (a scan, a quota, a delivery), fly them in turn, sell the ore dug beyond the
//! quota, fill the hold with what Halcyon makes for the delivery's station, then take the
//! board's next three missions. [`first_trip`] lists every step with its minutes and credits
//! and when the first ship-line tier ([`super::ShipLine::Engine`], the dearest first tier) is
//! affordable; the tests hold it within an hour on several seeds.
//!
//! Flight minutes come from the Wave 11 measurements (`world::space_journey_tests`: a warp
//! about a minute, an approach and landing two and a half, docking about three) plus time to
//! work: a scan is a short hop and a ping, a quota is a landing, a dig and a climb out.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use super::{
    StationLedger,
    bounty::{BoardContext, MissionKind, board_context, offer},
    market::{Good, Role, goods_of, ore_price, price},
    profile::StationProfile,
    shipyard::ShipLine,
};
use crate::space::{HOME_SECTOR, Space, StationId};

/// Minutes to fly to a body in the station's system, scan it and come back.
pub const SCAN_MINUTES: f32 = 4.0;
/// Minutes to fly to a body, land, dig a quota, climb out and come back.
pub const MINE_MINUTES: f32 = 10.0;
/// Minutes to carry a delivery to another station in the system and dock.
pub const DELIVER_MINUTES: f32 = 6.0;
/// A bounty hunt: the flight out to its system and the fight.
pub const BOUNTY_MINUTES: f32 = 9.0;
/// Extra minutes when the target is in another system (a warp each way, the cruise in).
pub const ELSEWHERE_MINUTES: f32 = 4.0;
/// Minutes at a station's counters per visit.
pub const DOCKED_MINUTES: f32 = 1.5;
/// Ore dug beyond a quota (the quota again), and what a unit of near-surface ore is worth at
/// home (iron and silver, the planet themes' shallow ores).
pub const EXTRA_ORE_SHARE: f32 = 1.0;
pub const SURFACE_ORE_VALUE: u32 = 20;

/// One step of the model.
#[derive(Clone, Debug, PartialEq)]
pub struct TripStep {
    pub label: String,
    pub minutes: f32,
    pub credits: u32,
}

/// The first trip out (see the module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TripModel {
    pub steps: Vec<TripStep>,
    /// Minutes until the first Ship Engine tier is affordable (`None`: not in the model).
    pub first_line_minutes: Option<f32>,
}

impl TripModel {
    /// Credits earned over the whole model.
    #[must_use]
    pub fn credits(&self) -> u32 {
        self.steps.iter().map(|step| step.credits).sum()
    }

    /// Minutes the whole model takes.
    #[must_use]
    pub fn minutes(&self) -> f32 {
        self.steps.iter().map(|step| step.minutes).sum()
    }
}

/// Minutes a mission takes in the model.
fn minutes_for(kind: MissionKind, context: &BoardContext) -> f32 {
    let elsewhere = match kind {
        MissionKind::Scan { body } | MissionKind::Mine { body, .. } => context
            .bodies
            .iter()
            .any(|found| found.id == body && found.elsewhere),
        MissionKind::Deliver { to, .. } => context
            .stations
            .iter()
            .any(|found| found.0 == to && found.2),
        MissionKind::Bounty { .. } => true,
    };
    let work = match kind {
        MissionKind::Scan { .. } => SCAN_MINUTES,
        MissionKind::Mine { .. } => MINE_MINUTES,
        MissionKind::Deliver { .. } => DELIVER_MINUTES,
        MissionKind::Bounty { .. } => BOUNTY_MINUTES,
    };
    work + if elsewhere { ELSEWHERE_MINUTES } else { 0.0 } + DOCKED_MINUTES
}

/// The trade run's profit a unit: buy what `from` makes, sell where `to` pays best.
fn best_trade(seed: u64, from: &StationProfile, to: &StationProfile) -> Option<(Good, u32)> {
    goods_of(from, seed)
        .into_iter()
        .filter(|(_, role)| *role == Role::Makes)
        .filter_map(|(good, _)| {
            let buy = price(seed, from, good, super::market::MarketShift::default(), 0)?.buy;
            let sell = price(seed, to, good, super::market::MarketShift::default(), 0)?.sell;
            (sell > buy).then(|| (good, sell - buy))
        })
        .max_by_key(|(good, profit)| (*profit, std::cmp::Reverse(*good)))
}

/// The first trip out from Halcyon Dock on the world seeded `seed`.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn first_trip(seed: u64) -> TripModel {
    let space = Space::new(seed);
    let halcyon = StationId {
        sector: HOME_SECTOR,
        system: 0,
        index: 0,
    };
    let Some(context) = board_context(&space, halcyon) else {
        return TripModel::default();
    };
    let Some(station) = context.station.clone() else {
        return TripModel::default();
    };
    let mut model = TripModel::default();
    let ledger = StationLedger::default();
    let mut serials = ledger.serials(halcyon);
    // Two boards' worth: the starter set, then the next roll of each slot.
    for round in 0..2 {
        for slot in 0..3_u8 {
            let Some(mission) = offer(seed, &context, slot, serials[usize::from(slot)]) else {
                continue;
            };
            serials[usize::from(slot)] += 1;
            let minutes = minutes_for(mission.kind, &context);
            model.steps.push(TripStep {
                label: format!("board {} slot {slot}: {:?}", round + 1, mission.kind),
                minutes,
                credits: mission.reward,
            });
            match mission.kind {
                MissionKind::Mine { units, .. } => {
                    let extra = (f32::from(units) * EXTRA_ORE_SHARE) as u32 + u32::from(units);
                    model.steps.push(TripStep {
                        label: format!("sell {extra} ore at Halcyon"),
                        minutes: 0.0,
                        credits: extra * ore_price(&station, SURFACE_ORE_VALUE),
                    });
                }
                MissionKind::Deliver { to, units, .. } => {
                    let Some(target) = super::profile::profile_of_id(
                        &crate::flight::GeneratedSpace::new(seed),
                        to,
                    ) else {
                        continue;
                    };
                    let room = super::shipyard::hold_capacity(0).saturating_sub(u32::from(units));
                    if let Some((good, profit)) = best_trade(seed, &station, &target) {
                        model.steps.push(TripStep {
                            label: format!("carry {room} {} along", good.name()),
                            minutes: 0.5,
                            credits: room * profit,
                        });
                    }
                }
                MissionKind::Scan { .. } | MissionKind::Bounty { .. } => {}
            }
        }
    }
    let target = ShipLine::Engine.price(1);
    let mut minutes = 0.0;
    let mut credits = 0;
    for step in &model.steps {
        minutes += step.minutes;
        credits += step.credits;
        if credits >= target {
            model.first_line_minutes = Some(minutes);
            break;
        }
    }
    model
}
