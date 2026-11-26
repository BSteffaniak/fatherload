//! The space economy's long curve (Wave 13 endgame): a bot that plays a career in space,
//! from an empty purse and from a post-Heart purse, on the real prices, bills, boards and belts.
//!
//! The bot plays in [`STEP_MINUTES`] steps. Each step it buys what it can among the next few
//! goals of its list ([`goals`]: every ship line tier, planet gear, alien tech and its marks,
//! the faction bosses, a home on an alien world and every wing of its estate, the Halcyon
//! Gate's parts, the Starwright ship, the Heartline Drive and the outfitter's stock), then
//! works toward the first goal it could not buy:
//!
//! * a boss: flies to its lair and fights ([`BOSS_MINUTES`], the wreck's credits);
//! * missing materials: mines belts at the farthest ring it reaches, for the hour rates
//!   [`ring_rates`] measured by mining real belt rocks of that ring (a hop between rocks, the
//!   beam's unit a second, a dock to sell when the bay fills), selling what no goal needs;
//! * missing credits: the better of the ring's boards ([`ring_rates`] reads the real boards of
//!   stations in that ring and the first-hour model's minutes for each kind of mission) and
//!   mining to sell.
//!
//! The rig's reach grows with its Star Drive range (ring 1 plus a ring a tier, two more with the
//! Heartline Drive); a species' tongue opens its Mk II after [`CONVERSANT_HOURS`] and Mk III
//! after [`FLUENT_HOURS`] of play in space (lessons, trades and chapters, the aliens track's
//! pace). Each step records whether credits or materials held the bot back, so the tests can
//! say credits stay worth earning.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "a model of hours and credits, all small and positive (plain arithmetic)"
)]

use std::collections::BTreeMap;

use crate::{
    hostiles::{Faction, ShipClass},
    space::{HOME_SECTOR, Space, SpaceSector, StationId, asteroids_in_segment},
    station::{
        AlienTech, MissionKind, ShipLine, SpaceGear,
        balance::{
            BOUNTY_MINUTES, DELIVER_MINUTES, DOCKED_MINUTES, ELSEWHERE_MINUTES, MINE_MINUTES,
            SCAN_MINUTES,
        },
        bounty::{board_context, offer},
    },
    terrain::MineralKind,
};

use super::{
    economy::space_credits,
    materials::{SPACE_MATERIALS, belt_material},
    works::{
        ESTATE_WINGS, GATE_PARTS, HEARTLINE_BILL, HEARTLINE_PRICE, Materials, OUTFITTER,
        StarwrightPart, estate_bill, estate_price, gate_part_bill, gate_part_price, line_bill,
        mark_bill, mark_price,
    },
};

/// Minutes a step of the bot lasts.
pub const STEP_MINUTES: f64 = 15.0;
/// Most hours a run plays.
pub const MAX_HOURS: f64 = 200.0;
/// Minutes a boss takes: the flight to its lair, the fight, the wreck (the hostiles bots beat
/// each in about a minute of fighting; a player learns it over a few tries).
pub const BOSS_MINUTES: f64 = 45.0;
/// Hours of play in space until a species' tongue reads Conversant and Fluent.
pub const CONVERSANT_HOURS: f64 = 4.0;
pub const FLUENT_HOURS: f64 = 12.0;
/// The bot's mining: seconds between rocks, units a second (the Mining Beam; a Heart drill in
/// contact is a little faster), the bay it fills and the minutes a trip to sell takes.
pub const HOP_SECONDS: f64 = 20.0;
pub const MINE_UNITS_PER_SECOND: f64 = 1.0;
pub const LATE_BAY_SLOTS: f64 = 1_000.0;
pub const SELL_TRIP_MINUTES: f64 = 6.0;
/// Comet dust an hour of material work sweeps (two tails).
pub const COMET_DUST_PER_HOUR: f64 = 12.0;
/// The farthest ring the model samples.
pub const MAX_RING: u8 = 7;

/// What an hour of work earns at one ring of the galaxy.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RingRates {
    pub ring: u8,
    /// Credits an hour of board missions (the real boards of stations in the ring).
    pub missions_per_hour: f64,
    /// Units of each space material an hour of belt mining gives (in [`SPACE_MATERIALS`]
    /// order).
    pub mined_per_hour: [f64; 8],
}

impl RingRates {
    /// Credits an hour of mining earns when everything is sold.
    #[must_use]
    pub fn mining_sale_per_hour(&self) -> f64 {
        SPACE_MATERIALS
            .iter()
            .zip(self.mined_per_hour)
            .map(|(mineral, units)| f64::from(mineral.value()) * units)
            .sum()
    }
}

/// Sectors at Chebyshev distance `ring` from home (`ring` 0: home's own).
fn ring_sectors(ring: u8) -> Vec<SpaceSector> {
    let r = i32::from(ring);
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx.abs().max(dy.abs()) == r {
                out.push(SpaceSector::new(HOME_SECTOR.x + dx, HOME_SECTOR.y + dy));
            }
        }
    }
    out
}

/// Minutes a mission of `kind` takes (the first-hour model's, [`crate::station::balance`]).
fn mission_minutes(kind: MissionKind, elsewhere: bool) -> f64 {
    let work = match kind {
        MissionKind::Scan { .. } => SCAN_MINUTES,
        MissionKind::Mine { .. } => MINE_MINUTES,
        MissionKind::Deliver { .. } => DELIVER_MINUTES,
        MissionKind::Bounty { .. } => BOUNTY_MINUTES,
    };
    f64::from(work + if elsewhere { ELSEWHERE_MINUTES } else { 0.0 } + DOCKED_MINUTES)
}

/// The hour rates of ring `ring` in the galaxy of `seed`: the boards of up to six crewed
/// stations there (two boards each), and a mining bot over three segments of every belt of
/// up to six of its systems.
#[must_use]
pub fn ring_rates(seed: u64, ring: u8) -> RingRates {
    let space = Space::new(seed);
    let mut credits = 0.0;
    let mut minutes = 0.0;
    let mut stations = 0;
    let mut mined = [0.0_f64; 8];
    let mut seconds = 0.0_f64;
    let mut systems = 0;
    'sectors: for coords in ring_sectors(ring) {
        let sector = space.sector(coords);
        for system in &sector.systems {
            if stations < 6 {
                for station in &system.stations {
                    let id = StationId {
                        sector: coords,
                        system: system.index,
                        index: station.id.index,
                    };
                    let Some(context) = board_context(&space, id) else {
                        continue;
                    };
                    if !context
                        .station
                        .as_ref()
                        .is_some_and(|profile| profile.bounties)
                    {
                        continue;
                    }
                    stations += 1;
                    for serial in 1..=2 {
                        for slot in 0..3 {
                            let Some(mission) = offer(seed, &context, slot, serial) else {
                                continue;
                            };
                            let elsewhere = match mission.kind {
                                MissionKind::Scan { body } | MissionKind::Mine { body, .. } => {
                                    context
                                        .bodies
                                        .iter()
                                        .any(|found| found.id == body && found.elsewhere)
                                }
                                MissionKind::Deliver { to, .. } => context
                                    .stations
                                    .iter()
                                    .any(|found| found.0 == to && found.2),
                                MissionKind::Bounty { .. } => true,
                            };
                            credits += f64::from(mission.reward);
                            minutes += mission_minutes(mission.kind, elsewhere);
                        }
                    }
                }
            }
            if systems < 6 && !system.belts.is_empty() {
                systems += 1;
                let mut bay = 0.0;
                for belt in &system.belts {
                    for segment in 0..3 {
                        for rock in asteroids_in_segment(seed, system, coords, belt, segment) {
                            let units = f64::from(crate::flight::systems::rock_yield(&rock))
                                .min(LATE_BAY_SLOTS);
                            let index = SPACE_MATERIALS
                                .iter()
                                .position(|each| *each == belt_material(rock.ore))
                                .unwrap_or(0);
                            mined[index] += units;
                            seconds += HOP_SECONDS + units / MINE_UNITS_PER_SECOND;
                            bay += units;
                            if bay >= LATE_BAY_SLOTS {
                                seconds += SELL_TRIP_MINUTES * 60.0;
                                bay = 0.0;
                            }
                        }
                    }
                }
            }
            if stations >= 6 && systems >= 6 {
                break 'sectors;
            }
        }
    }
    // Comets: a rig out for materials sweeps about two tails an hour (six units each; nav
    // charts every comet of a system).
    let hours = (seconds / 3_600.0).max(1e-6);
    let mut per_hour = [0.0; 8];
    for (rate, units) in per_hour.iter_mut().zip(mined) {
        *rate = units / hours;
    }
    per_hour[7] = if ring > 0 { COMET_DUST_PER_HOUR } else { 0.0 };
    RingRates {
        ring,
        missions_per_hour: if minutes > 0.0 {
            credits * 60.0 / minutes
        } else {
            0.0
        },
        mined_per_hour: per_hour,
    }
}

/// One goal of the bot's list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Goal {
    Line(ShipLine, u8),
    Gear(SpaceGear, u8),
    Tech(AlienTech),
    Mark(AlienTech, u8),
    Boss(Faction),
    AlienHome,
    Wing(u8),
    GatePart(u8),
    Starwright(StarwrightPart),
    Heartline,
    Outfit(u8),
}

impl Goal {
    /// The goal as a line of the report.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Line(line, tier) => format!("{} tier {tier}", line.name()),
            Self::Gear(gear, tier) => format!("{} tier {tier}", gear.name()),
            Self::Tech(tech) => tech.name().to_owned(),
            Self::Mark(tech, mark) => format!("{} Mk {}", tech.name(), roman(mark)),
            Self::Boss(faction) => super::hunt::faction_boss_name(faction).to_owned(),
            Self::AlienHome => "a home on an alien world".to_owned(),
            Self::Wing(wing) => format!("estate wing {}", wing + 1),
            Self::GatePart(index) => super::works::gate_part_name(index).to_owned(),
            Self::Starwright(part) => part.name().to_owned(),
            Self::Heartline => "the Heartline Drive".to_owned(),
            Self::Outfit(index) => OUTFITTER
                .get(usize::from(index))
                .map_or_else(String::new, |(item, _)| {
                    format!("outfitter: {}", item.label())
                }),
        }
    }

    /// The goal the game wants done first, if any.
    #[must_use]
    pub const fn requires(self) -> Option<Self> {
        match self {
            Self::Line(line, tier) if tier > 1 => Some(Self::Line(line, tier - 1)),
            Self::Gear(gear, tier) if tier > 1 => Some(Self::Gear(gear, tier - 1)),
            Self::Mark(tech, mark) if mark > 2 => Some(Self::Mark(tech, mark - 1)),
            Self::Mark(tech, _) => Some(Self::Tech(tech)),
            Self::Wing(0) => Some(Self::AlienHome),
            Self::Wing(wing) => Some(Self::Wing(wing - 1)),
            _ => None,
        }
    }

    /// Its credits and materials.
    #[must_use]
    pub fn bill(self) -> (f64, Materials) {
        let (credits, materials) = match self {
            Self::Line(line, tier) => (line.price(tier), line_bill(line, tier)),
            Self::Gear(gear, tier) => (gear.price(tier), &[][..]),
            Self::Tech(tech) => (tech.price(), &[][..]),
            Self::Mark(tech, mark) => (mark_price(tech, mark), mark_bill(tech, mark)),
            Self::Boss(_) => (0, &[][..]),
            Self::AlienHome => (
                crate::world::alien_home_price(crate::alien::AlienSpecies::Tokkari),
                &[][..],
            ),
            Self::Wing(wing) => {
                let expansion = ESTATE_WINGS[usize::from(wing)];
                (
                    estate_price(expansion, usize::from(wing)),
                    estate_bill(expansion),
                )
            }
            Self::GatePart(index) => (gate_part_price(index), gate_part_bill(index)),
            Self::Starwright(part) => (part.price(), part.bill()),
            Self::Heartline => (HEARTLINE_PRICE, HEARTLINE_BILL),
            Self::Outfit(index) => (
                OUTFITTER
                    .get(usize::from(index))
                    .map_or(0, |(_, price)| *price),
                &[][..],
            ),
        };
        (f64::from(credits), materials)
    }
}

const fn roman(mark: u8) -> &'static str {
    match mark {
        1 => "I",
        2 => "II",
        _ => "III",
    }
}

/// The bot's list, in the order a player works through it.
#[must_use]
pub fn goals() -> Vec<Goal> {
    let mut list = Vec::new();
    let lines = |tier: u8| ShipLine::ALL.map(|line| Goal::Line(line, tier));
    list.extend(lines(1));
    list.extend(SpaceGear::ALL.map(|gear| Goal::Gear(gear, 1)));
    list.extend(lines(2));
    list.extend(AlienTech::ALL.map(Goal::Tech));
    list.extend(SpaceGear::ALL.map(|gear| Goal::Gear(gear, 2)));
    list.push(Goal::AlienHome);
    list.extend(lines(3));
    list.push(Goal::Boss(Faction::Pirates));
    list.extend([Goal::Wing(0), Goal::Wing(1)]);
    list.extend(lines(4));
    list.extend(AlienTech::ALL.map(|tech| Goal::Mark(tech, 2)));
    list.push(Goal::Boss(Faction::Swarm));
    list.extend([Goal::Wing(2), Goal::Wing(3)]);
    list.extend(lines(5));
    list.push(Goal::Boss(Faction::Machine));
    list.extend(StarwrightPart::ALL.map(Goal::Starwright));
    list.push(Goal::Heartline);
    list.extend((0..6).map(Goal::GatePart));
    list.extend([Goal::Wing(4), Goal::Wing(5), Goal::Wing(6)]);
    list.extend(AlienTech::ALL.map(|tech| Goal::Mark(tech, 3)));
    list.extend([Goal::GatePart(6), Goal::GatePart(GATE_PARTS - 1)]);
    list.extend((0..u8::try_from(OUTFITTER.len()).unwrap_or(0)).map(Goal::Outfit));
    list
}

/// How the bot's run went.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CareerRun {
    /// The purse it started with.
    pub start_purse: f64,
    /// Every goal reached, with the hour.
    pub reached: Vec<(f64, Goal)>,
    /// Hours until the last goal (`None`: not within [`MAX_HOURS`]).
    pub done_hours: Option<f64>,
    /// Each step: the hour it ended, the purse, and what held the bot back (`None`: a boss).
    pub steps: Vec<(f64, f64, Option<Limit>)>,
}

/// What held the bot back in a step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    Credits,
    Materials,
    /// A tongue not yet learned (time in space alone).
    Tongue,
}

impl CareerRun {
    /// The hour `goal` was reached.
    #[must_use]
    pub fn hour_of(&self, goal: Goal) -> Option<f64> {
        self.reached
            .iter()
            .find(|(_, each)| *each == goal)
            .map(|(hour, _)| *hour)
    }

    /// The share of steps before `hours` that credits held back (0-1).
    #[must_use]
    pub fn credit_limited_share(&self, hours: f64) -> f64 {
        let steps: Vec<_> = self.steps.iter().filter(|step| step.0 <= hours).collect();
        if steps.is_empty() {
            return 0.0;
        }
        steps
            .iter()
            .filter(|step| step.2 == Some(Limit::Credits))
            .count() as f64
            / steps.len() as f64
    }

    /// The purse at `hours`.
    #[must_use]
    pub fn purse_at(&self, hours: f64) -> f64 {
        self.steps
            .iter()
            .take_while(|step| step.0 <= hours)
            .last()
            .map_or(self.start_purse, |step| step.1)
    }

    /// A report of the run (the README's tables).
    #[must_use]
    pub fn report(&self) -> String {
        use std::fmt::Write as _;
        let mut out = format!("start purse {:.0}\n", self.start_purse);
        for (hour, goal) in &self.reached {
            let _ = writeln!(out, "{hour:>6.2} h  {}", goal.label());
        }
        let _ = writeln!(
            out,
            "done {:?} h; credits held it back {:.0}% of hours 0-20, {:.0}% overall",
            self.done_hours,
            self.credit_limited_share(20.0) * 100.0,
            self.credit_limited_share(MAX_HOURS) * 100.0
        );
        out
    }
}

/// The rig's reach (the farthest ring it works in) with Star Drive range `tier`.
#[must_use]
pub const fn reach(drive_range: u8, heartline: bool) -> u8 {
    let ring = 1 + drive_range + if heartline { 2 } else { 0 };
    if ring > MAX_RING { MAX_RING } else { ring }
}

/// The ring a faction's boss waits in, and the hull and engine tiers a well-kept rig brings.
const fn boss_needs(faction: Faction) -> (u8, u8) {
    match faction {
        Faction::Pirates => (2, 2),
        Faction::Swarm => (3, 3),
        Faction::Machine => (4, 4),
    }
}

/// Credits a boss's wreck gives (the hostiles' wreck at its lair's ring).
fn boss_wreck(faction: Faction) -> f64 {
    let class = match faction {
        Faction::Pirates => ShipClass::PirateKing,
        Faction::Swarm => ShipClass::SwarmQueen,
        Faction::Machine => ShipClass::Arbiter,
    };
    let (ring, _) = boss_needs(faction);
    let credits = class.stats().credits;
    f64::from(space_credits(
        credits + credits * u32::from(ring.saturating_sub(1)) / 4,
    ))
}

/// Plays a career on the world seeded `seed` from a purse of `purse` credits.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the bot's whole loop in order")]
pub fn run_career(seed: u64, purse: f64) -> CareerRun {
    let rates: Vec<RingRates> = (0..=MAX_RING).map(|ring| ring_rates(seed, ring)).collect();
    let list = goals();
    let mut done = vec![false; list.len()];
    let mut credits = purse;
    let mut stock: BTreeMap<MineralKind, f64> = BTreeMap::new();
    let mut lines = [0_u8; 5];
    let mut beaten: Vec<Faction> = Vec::new();
    let mut heartline = false;
    let mut boss_progress = 0.0;
    let mut run = CareerRun {
        start_purse: purse,
        ..CareerRun::default()
    };
    let mut hour = 0.0;
    let max_steps = (MAX_HOURS * 60.0 / STEP_MINUTES) as u32;
    let line_index = |line: ShipLine| ShipLine::ALL.iter().position(|each| *each == line);
    let has = |stock: &BTreeMap<MineralKind, f64>, materials: Materials| {
        materials
            .iter()
            .all(|(mineral, units)| stock.get(mineral).copied().unwrap_or(0.0) >= f64::from(*units))
    };
    for _ in 0..max_steps {
        // Buy what the purse and the bay allow, in the list's order.
        for (index, goal) in list.iter().enumerate() {
            if done[index] {
                continue;
            }
            let ready = match goal {
                Goal::Mark(_, mark) => {
                    hour >= if *mark >= 3 {
                        FLUENT_HOURS
                    } else {
                        CONVERSANT_HOURS
                    }
                }
                Goal::Boss(_) => false,
                Goal::Starwright(part) => {
                    line_index(part.line()).is_some_and(|line| lines[line] >= 5)
                }
                Goal::Heartline => {
                    Faction::ALL.iter().all(|faction| beaten.contains(faction))
                        && list.iter().enumerate().all(|(other, each)| {
                            !matches!(each, Goal::Starwright(_)) || done[other]
                        })
                }
                Goal::GatePart(part) if *part == GATE_PARTS - 1 => {
                    list.iter().enumerate().all(|(other, each)| {
                        !matches!(each, Goal::GatePart(_)) || other == index || done[other]
                    })
                }
                _ => true,
            };
            // The goal before it in the game's own order must be in (a line's or a gear's
            // tier before, Mk I before a mark, the estate's wing before).
            let ready = ready
                && goal.requires().is_none_or(|before| {
                    list.iter()
                        .position(|each| *each == before)
                        .is_none_or(|other| done[other])
                });
            let (price, materials) = goal.bill();
            if ready && credits >= price && has(&stock, materials) {
                credits -= price;
                for (mineral, units) in materials {
                    *stock.entry(*mineral).or_default() -= f64::from(*units);
                }
                if let Goal::Line(line, tier) = goal
                    && let Some(line) = line_index(*line)
                {
                    lines[line] = lines[line].max(*tier);
                }
                if *goal == Goal::Heartline {
                    heartline = true;
                }
                done[index] = true;
                run.reached.push((hour, *goal));
            }
        }
        let Some(next) = list
            .iter()
            .enumerate()
            .find(|(index, _)| !done[*index])
            .map(|(index, goal)| (index, *goal))
        else {
            run.done_hours = Some(hour);
            break;
        };
        let ring = reach(lines[2], heartline);
        let rate = &rates[usize::from(ring)];
        let step_hours = STEP_MINUTES / 60.0;
        // Work toward the next goal.
        let limit = match next.1 {
            Goal::Boss(faction) => {
                let (lair, tier) = boss_needs(faction);
                if ring >= lair && lines[0] >= tier && lines[1] >= tier {
                    boss_progress += STEP_MINUTES;
                    if boss_progress >= BOSS_MINUTES {
                        boss_progress = 0.0;
                        beaten.push(faction);
                        credits += boss_wreck(faction);
                        done[next.0] = true;
                        run.reached.push((hour + step_hours, next.1));
                    }
                    None
                } else {
                    // Not ready for it yet: earn toward the lines it needs.
                    credits += rate.missions_per_hour.max(rate.mining_sale_per_hour()) * step_hours;
                    Some(Limit::Credits)
                }
            }
            Goal::Mark(_, mark)
                if hour
                    < if mark >= 3 {
                        FLUENT_HOURS
                    } else {
                        CONVERSANT_HOURS
                    } =>
            {
                credits += rate.missions_per_hour.max(rate.mining_sale_per_hour()) * step_hours;
                Some(Limit::Tongue)
            }
            goal => {
                let (price, materials) = goal.bill();
                if has(&stock, materials) {
                    // Credits: the better of the boards and mining to sell.
                    let _ = price;
                    credits += rate.missions_per_hour.max(rate.mining_sale_per_hour()) * step_hours;
                    Some(Limit::Credits)
                } else {
                    // Mine for the bill at the ring (within reach) that gives its scarcest
                    // material fastest, selling what no goal needs.
                    let scarcest = materials
                        .iter()
                        .filter(|(mineral, units)| {
                            stock.get(mineral).copied().unwrap_or(0.0) < f64::from(*units)
                        })
                        .map(|(mineral, _)| *mineral)
                        .min_by_key(|mineral| std::cmp::Reverse(mineral.value()));
                    let slot = scarcest
                        .and_then(|mineral| {
                            SPACE_MATERIALS.iter().position(|each| *each == mineral)
                        })
                        .unwrap_or(0);
                    let rate = rates[1..=usize::from(ring)]
                        .iter()
                        .max_by(|a, b| a.mined_per_hour[slot].total_cmp(&b.mined_per_hour[slot]))
                        .unwrap_or(rate);
                    let needs: BTreeMap<MineralKind, f64> = list
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| !done[*index])
                        .flat_map(|(_, each)| each.bill().1.iter())
                        .fold(BTreeMap::new(), |mut needs, (mineral, units)| {
                            *needs.entry(*mineral).or_default() += f64::from(*units);
                            needs
                        });
                    for (mineral, per_hour) in SPACE_MATERIALS.iter().zip(rate.mined_per_hour) {
                        let units = per_hour * step_hours;
                        let held = stock.entry(*mineral).or_default();
                        let wanted = (needs.get(mineral).copied().unwrap_or(0.0) - *held).max(0.0);
                        let kept = units.min(wanted);
                        *held += kept;
                        credits += (units - kept) * f64::from(mineral.value());
                    }
                    // Commons the stations sell are bought with spare credits.
                    for (mineral, units) in materials {
                        let held = stock.get(mineral).copied().unwrap_or(0.0);
                        let short = (f64::from(*units) - held).max(0.0);
                        let unit_price = f64::from(super::materials::material_price(*mineral));
                        let buyable = matches!(
                            mineral,
                            MineralKind::MeteoricIron
                                | MineralKind::Kamacite
                                | MineralKind::VolatileIce
                                | MineralKind::CobaltBloom
                        );
                        if buyable && short > 0.0 && credits >= price + short * unit_price {
                            credits -= short * unit_price;
                            *stock.entry(*mineral).or_default() += short;
                        }
                    }
                    Some(Limit::Materials)
                }
            }
        };
        hour += step_hours;
        run.steps.push((hour, credits, limit));
    }
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x00C0_FFEE;

    #[test]
    fn rings_pay_more_and_give_rarer_ore_farther_out() {
        let near = ring_rates(SEED, 1);
        let far = ring_rates(SEED, 4);
        assert!(near.missions_per_hour > 100_000.0, "{near:?}");
        assert!(
            far.missions_per_hour > near.missions_per_hour * 0.8,
            "{far:?}"
        );
        // Every ring's belts give the commons; the rare ores are there to be found.
        for rates in [&near, &far] {
            assert!(rates.mined_per_hour[0] > 100.0, "{rates:?}");
            assert!(
                rates.mined_per_hour[4..7].iter().sum::<f64>() > 0.0,
                "{rates:?}"
            );
        }
    }

    #[test]
    fn from_an_empty_purse_the_first_line_is_within_the_hour_and_the_end_is_far() {
        let run = run_career(SEED, 0.0);
        // The first ship-line tier within the hour, as the first-hour model says
        // ([`crate::station::balance`]), and all five within three.
        let first = run
            .reached
            .iter()
            .find(|(_, goal)| matches!(goal, Goal::Line(_, 1)))
            .map(|(hour, _)| *hour)
            .expect("the first line");
        assert!(first <= 1.0, "{}", run.report());
        for line in ShipLine::ALL {
            assert!(
                run.hour_of(Goal::Line(line, 1))
                    .is_some_and(|hour| hour <= 3.0),
                "{}",
                run.report()
            );
        }
        let done = run.done_hours.expect("the bot finishes");
        assert!((40.0..MAX_HOURS).contains(&done), "{}", run.report());
        // Credits matter most of the way.
        assert!(
            run.credit_limited_share(MAX_HOURS) > 0.3,
            "{}",
            run.report()
        );
    }

    #[test]
    fn from_a_post_heart_purse_credits_stay_worth_earning_past_twenty_hours() {
        let late = super::super::economy::late_purse();
        let run = run_career(SEED, late.purse);
        // The purse buys the early lines at once...
        assert_eq!(run.hour_of(Goal::Line(ShipLine::Engine, 1)), Some(0.0));
        assert_eq!(run.hour_of(Goal::Line(ShipLine::Engine, 2)), Some(0.0));
        // ...but not the end: twenty hours and more of goals, and credits hold the bot back
        // for a good share of the first twenty.
        let done = run.done_hours.expect("the bot finishes");
        assert!(done >= 20.0, "{}", run.report());
        assert!(run.credit_limited_share(20.0) >= 0.2, "{}", run.report());
        assert!(run.purse_at(10.0) < late.purse, "{}", run.report());
        // And it is a head start: the late rig finishes sooner than one from nothing.
        let empty = run_career(SEED, 0.0);
        assert!(done < empty.done_hours.unwrap_or(MAX_HOURS));
    }

    #[test]
    #[ignore = "prints the two curves (cargo test --release -p fatherload_simulation career_report -- --ignored --nocapture)"]
    fn career_report() {
        for seed in [SEED, 1, 7] {
            for rates in (0..=MAX_RING).map(|ring| ring_rates(seed, ring)) {
                println!(
                    "seed {seed:#x} ring {}: missions {:.0}/h, mining sale {:.0}/h, {:?}",
                    rates.ring,
                    rates.missions_per_hour,
                    rates.mining_sale_per_hour(),
                    rates.mined_per_hour.map(f64::round)
                );
            }
            println!(
                "--- empty purse, seed {seed:#x}\n{}",
                run_career(seed, 0.0).report()
            );
            let late = super::super::economy::late_purse();
            println!(
                "--- post-Heart purse {:.0} (income at the Heart {:.0}/h), seed {seed:#x}\n{}",
                late.purse,
                late.credits_per_hour,
                run_career(seed, late.purse).report()
            );
        }
    }
}
