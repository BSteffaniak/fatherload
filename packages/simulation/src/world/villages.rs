//! Villages inside the tick ([`crate::village`]): which village and building a rig is at,
//! first-visit discovery, every counter deal, and shrine blessings.

use std::sync::Arc;

use crate::{
    economy::{REFUEL_UNIT_COST, REPAIR_UNIT_COST},
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    poi::PoiKnowledge,
    service::{PlayerServiceTransaction, PlayerTransactionKind},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TilePosition},
    village::{
        self, Blessing, BlessingKind, Village, VillageDeal, VillageId, VillageService,
        services::{BLESSING_MILLIS, FORTUNE_BONUS_PERCENT},
    },
    worldgen::{FeatureKind, TileRect},
};

use super::{WorldState, record_service_transaction};

/// Columns either side of a village a cartographer charts points of interest in.
pub const CHART_RADIUS_COLUMNS: i32 = 420;
/// Points of interest one charting reveals at most.
pub const CHART_SITES: usize = 3;
/// Columns either side of an inn its regulars know rumours about.
pub const RUMOUR_RADIUS_COLUMNS: i32 = 320;
/// Diamond radius of map revealed around each charted site.
const CHART_REVEAL_RADIUS: i32 = 6;

impl WorldState {
    /// The village `player_id`'s rig is in, if any.
    #[must_use]
    pub fn player_village(&self, player_id: PlayerId) -> Option<Arc<Village>> {
        let player = self.players.get(&player_id)?;
        village::village_at(self.terrain.settings(), player.tile_position(TILE_SIZE))
    }

    /// The village building whose door `player_id`'s rig stands at: `(village, building)`.
    /// The single rule for "may this rig use that counter", shared by the shell and the tick.
    #[must_use]
    pub fn player_village_building(&self, player_id: PlayerId) -> Option<(Arc<Village>, usize)> {
        // On foot the pilot is the one at the door (Part II F5).
        let (x, y) = self.actor_position(player_id)?;
        village::building_at(self.terrain.settings(), x, y)
    }

    /// The blessing on `player_id`'s rig right now, if any.
    #[must_use]
    pub fn player_blessing(&self, player_id: PlayerId) -> Option<BlessingKind> {
        self.players
            .get(&player_id)?
            .villages
            .blessing_at(self.simulation_tick)
    }

    /// Records every rig's first visit to the village it is in (once per tick).
    pub(super) fn discover_villages(&mut self, output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_villages() {
            return;
        }
        let settings = *self.terrain.settings();
        for (player_id, player) in &mut self.players {
            let Some(found) = village::village_at(&settings, player.tile_position(TILE_SIZE))
            else {
                continue;
            };
            if player.villages.visited.insert(found.id) {
                output.push_event(GameplayEventKind::VillageDiscovered {
                    player_id: *player_id,
                    village: found.id,
                });
            }
        }
    }

    /// Resolves offer `offer` of building `building` of village `village` for `player_id`
    /// (the rig's position was validated by [`Self::command_allowed_here`]).
    pub(crate) fn apply_village_deal(
        &mut self,
        player_id: PlayerId,
        village_id: VillageId,
        building: u8,
        offer: u8,
        service: VillageService,
    ) -> PlayerScopedCommandOutcome {
        let Some(found) = village::village(self.terrain.settings(), village_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let building = usize::from(building);
        let Some(lot) = found.buildings.get(building) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if VillageService::of(lot.kind) != Some(service) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(deal) = found
            .offers_at(building, self.market_time())
            .get(usize::from(offer))
            .copied()
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        if matches!(
            deal.deal,
            VillageDeal::SellCargo { .. }
                | VillageDeal::Refuel { .. }
                | VillageDeal::Repair { .. }
                | VillageDeal::SkyCraft { .. }
        ) && !self.rig_within_service_reach(player_id)
        {
            // On foot, the rig itself must be parked outside (Part II F5).
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        // An apprenticed haggler (WP-PEOPLE) pays less at every counter.
        let haggler = self
            .people
            .has_perk(player_id, crate::people::offers::Perk::Haggler);
        let price = if haggler {
            (deal.price * crate::people::offers::HAGGLER_PERCENT).div_ceil(100)
        } else {
            deal.price
        };
        let metered = |percent: u32| {
            if haggler {
                percent * crate::people::offers::HAGGLER_PERCENT / 100
            } else {
                percent
            }
        };
        match deal.deal {
            VillageDeal::SellCargo { percent, only } => self.village_sell(player_id, percent, only),
            VillageDeal::Refuel { percent } => self.village_refuel(player_id, metered(percent)),
            VillageDeal::Repair { percent } => self.village_repair(player_id, metered(percent)),
            VillageDeal::ChartRegion => self.village_chart(player_id, &found, price),
            VillageDeal::Rumour => self.village_rumour(player_id, &found, price),
            VillageDeal::SellNightGoods { percent } => self.sell_night_goods(player_id, percent),
            VillageDeal::Room => {
                if !self.clock().phase().sleepable()
                    || self.is_in_bed(player_id)
                    || !self.charge(player_id, price)
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                self.lie_down(player_id, crate::clock::BedSpot::Inn(village_id))
            }
            goods @ (VillageDeal::Part(_)
            | VillageDeal::Cosmetic(_)
            | VillageDeal::Explosives { .. }
            | VillageDeal::Blessing(_)
            | VillageDeal::PilotArms(_)) => self.apply_goods(player_id, goods, price),
            VillageDeal::SkyCraft { recipe, tier } => self.apply_sky_craft(player_id, recipe, tier),
        }
    }

    /// Resolves a deal that needs no particular counter (a part, a cosmetic, a pack of charges,
    /// a blessing, or metered fuel and repairs at `percent`) for `price` credits: village
    /// counters and people's trade lists (WP-PEOPLE) share it.
    pub(super) fn apply_goods(
        &mut self,
        player_id: PlayerId,
        deal: VillageDeal,
        price: u32,
    ) -> PlayerScopedCommandOutcome {
        match deal {
            VillageDeal::Refuel { percent } => self.village_refuel(player_id, percent),
            VillageDeal::Repair { percent } => self.village_repair(player_id, percent),
            VillageDeal::Part(part) => {
                if self.progression.rig_part_inventory.contains(&part)
                    || !self.charge(player_id, price)
                {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                self.progression.rig_part_inventory.insert(part);
                PlayerScopedCommandOutcome::Applied
            }
            VillageDeal::Cosmetic(item) => {
                let owned = self
                    .players
                    .get(&player_id)
                    .is_some_and(|player| player.cosmetic_unlocks.has(item));
                if owned || !self.charge(player_id, price) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.unlock_cosmetic(item);
                }
                PlayerScopedCommandOutcome::Applied
            }
            VillageDeal::Explosives { kind, count } => {
                if !self.charge(player_id, price) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.add_explosives(kind, count);
                }
                PlayerScopedCommandOutcome::Applied
            }
            VillageDeal::Blessing(kind) => {
                if !self.charge(player_id, price) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let until = crate::ids::SimulationTick::new(
                    self.simulation_tick
                        .get()
                        .saturating_add(self.timing.ticks_for_millis(BLESSING_MILLIS)),
                );
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.villages.blessing = Some(Blessing { kind, until });
                }
                PlayerScopedCommandOutcome::Applied
            }
            VillageDeal::PilotArms(purchase) => {
                let Some(player) = self.players.get(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                let room = match purchase {
                    crate::pilot::arsenal::PilotArmsPurchase::Weapon(weapon) => {
                        crate::pilot::arsenal::PILOT_WEAPONS.contains(&weapon)
                            && !player.pilot_arsenal.owned.contains(&weapon)
                    }
                    crate::pilot::arsenal::PilotArmsPurchase::Ammo(ammo) => {
                        crate::weapon::AmmoKind::PILOT.contains(&ammo)
                            && player.pilot_arsenal.ammo(ammo)
                                < crate::pilot::arsenal::MAX_PILOT_AMMO
                    }
                };
                if !room || !self.charge(player_id, price) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                if let Some(player) = self.players.get_mut(&player_id) {
                    match purchase {
                        crate::pilot::arsenal::PilotArmsPurchase::Weapon(weapon) => {
                            let _ = player.pilot_arsenal.receive(weapon);
                        }
                        crate::pilot::arsenal::PilotArmsPurchase::Ammo(ammo) => {
                            let _ = player.pilot_arsenal.add_ammo(ammo, ammo.pack().0);
                        }
                    }
                }
                let held = self
                    .players
                    .get(&player_id)
                    .and_then(|player| player.pilot_arsenal.equipped);
                if let Some((id, _, _)) = self.pilot_of(player_id) {
                    self.edit_pilot(id, |_, state| state.weapon = held);
                }
                PlayerScopedCommandOutcome::Applied
            }
            // A room is the inn's own counter (`apply_village_deal`), never a trade-list row.
            VillageDeal::SellCargo { .. }
            | VillageDeal::ChartRegion
            | VillageDeal::Rumour
            | VillageDeal::Room
            | VillageDeal::SkyCraft { .. }
            | VillageDeal::SellNightGoods { .. } => PlayerScopedCommandOutcome::IgnoredUnavailable,
        }
    }

    /// The village counters' trading hours now (WP-NIGHT: the night market keeps the night
    /// monsters' hours).
    #[must_use]
    pub const fn market_time(&self) -> crate::village::services::MarketTime {
        crate::village::services::MarketTime::of(self.clock())
    }

    /// Sells every night-only material in the bay at `percent` of its night value (the night
    /// market, WP-NIGHT).
    fn sell_night_goods(
        &mut self,
        player_id: PlayerId,
        percent: u32,
    ) -> PlayerScopedCommandOutcome {
        let percent = percent + self.sale_bonus_percent(player_id);
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let mut payout = 0_u64;
        for material in crate::night::NIGHT_MATERIALS {
            let count = player.cargo_bay.take_all(material);
            payout += u64::from(count) * u64::from(crate::night::night_material_value(material));
        }
        let payout = u32::try_from(payout * u64::from(percent) / 100).unwrap_or(u32::MAX);
        if payout == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits = player.credits.saturating_add(payout);
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(payout);
        PlayerScopedCommandOutcome::Applied
    }

    /// Takes `price` credits from `player_id` if they have them.
    pub(super) fn charge(&mut self, player_id: PlayerId, price: u32) -> bool {
        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        if player.credits < price {
            return false;
        }
        player.credits -= price;
        true
    }

    /// The percent a sale pays on top of its deal (a Fortune blessing).
    pub(super) fn sale_bonus_percent(&self, player_id: PlayerId) -> u32 {
        if self.player_blessing(player_id) == Some(BlessingKind::Fortune) {
            FORTUNE_BONUS_PERCENT
        } else {
            0
        }
    }

    fn village_sell(
        &mut self,
        player_id: PlayerId,
        percent: u32,
        only: Option<crate::terrain::MineralKind>,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let minerals: u32 = player
            .cargo_bay
            .minerals()
            .filter(|(mineral, _)| only.is_none_or(|wanted| wanted == *mineral))
            .map(|(mineral, count)| {
                self.canonical_mineral_market_value(mineral)
                    .saturating_mul(count)
            })
            .sum();
        let artifacts: u32 = if only.is_some() {
            0
        } else {
            player
                .cargo_bay
                .artifacts()
                .map(|(artifact, count)| artifact.value().saturating_mul(count))
                .sum()
        };
        let percent = percent + self.sale_bonus_percent(player_id);
        let payout =
            u32::try_from(u64::from(minerals.saturating_add(artifacts)) * u64::from(percent) / 100)
                .unwrap_or(u32::MAX);
        if payout == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        match only {
            Some(mineral) => {
                let _ = player.cargo_bay.take_all(mineral);
            }
            None => player.cargo_bay.clear_ore_and_artifacts(),
        }
        player.credits = player.credits.saturating_add(payout);
        let cargo_after = player.cargo_used();
        let credits_after = player.credits;
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(payout);
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: PlayerTransactionKind::SellCargo,
                credits_before,
                credits_after,
                cargo_before,
                cargo_after,
            },
        );
        PlayerScopedCommandOutcome::Applied
    }

    fn village_refuel(&mut self, player_id: PlayerId, percent: u32) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        // Up in the sky a rig that cannot cross the dead-air gap is only topped up to an
        // honest level (sky towns and their traders; `milestone::sky_refuel_level`).
        #[allow(
            clippy::cast_possible_truncation,
            reason = "altitudes are a few thousand rows"
        )]
        let altitude = (-player.y / TILE_SIZE).floor() as i32;
        let level = crate::milestone::sky_refuel_level(player, altitude);
        let missing = (level - player.fuel).ceil().max(0.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a tank holds a few thousand units at most"
        )]
        let units = metered_units(missing as u32, REFUEL_UNIT_COST * percent, player.credits);
        if units == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let credits_before = player.credits;
        player.credits -= metered_cost(units, REFUEL_UNIT_COST * percent);
        #[allow(clippy::cast_precision_loss, reason = "a few thousand units")]
        let added = units as f32;
        player.fuel = (player.fuel + added).min(player.fuel_capacity);
        let entry = PlayerServiceTransaction {
            player_id,
            kind: PlayerTransactionKind::Refuel,
            credits_before,
            credits_after: player.credits,
            cargo_before: player.cargo_used(),
            cargo_after: player.cargo_used(),
        };
        record_service_transaction(&mut self.service_transactions, entry);
        PlayerScopedCommandOutcome::Applied
    }

    fn village_repair(&mut self, player_id: PlayerId, percent: u32) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let missing = (player.max_hull() - player.hull).ceil().max(0.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a hull holds a few thousand points at most"
        )]
        let units = metered_units(missing as u32, REPAIR_UNIT_COST * percent, player.credits);
        if units == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let credits_before = player.credits;
        player.credits -= metered_cost(units, REPAIR_UNIT_COST * percent);
        #[allow(clippy::cast_precision_loss, reason = "a few thousand points")]
        let added = units as f32;
        player.hull = (player.hull + added).min(player.max_hull());
        let entry = PlayerServiceTransaction {
            player_id,
            kind: PlayerTransactionKind::Repair,
            credits_before,
            credits_after: player.credits,
            cargo_before: player.cargo_used(),
            cargo_after: player.cargo_used(),
        };
        record_service_transaction(&mut self.service_transactions, entry);
        PlayerScopedCommandOutcome::Applied
    }

    /// The points of interest near `around` (within `radius` columns, any depth) that
    /// `player_id` does not know yet, nearest first.
    pub(super) fn unknown_sites_near(
        &self,
        player_id: PlayerId,
        around: TilePosition,
        radius: i32,
    ) -> Vec<crate::worldgen::WorldFeature> {
        let Some(player) = self.players.get(&player_id) else {
            return Vec::new();
        };
        let rect = TileRect {
            min: TilePosition {
                x: around.x - radius,
                y: 0,
            },
            max: TilePosition {
                x: around.x + radius,
                y: self.terrain.height() - 1,
            },
        };
        let mut sites: Vec<_> = self
            .terrain
            .features_in(&rect, Some(&[FeatureKind::PointOfInterest]))
            .into_iter()
            .filter(|feature| !player.journal.known.contains_key(&feature.id))
            .collect();
        sites.sort_by_key(|feature| {
            let dx = i64::from(feature.center.x - around.x);
            let dy = i64::from(feature.center.y - around.y);
            (dx * dx + dy * dy, feature.id)
        });
        sites
    }

    fn village_chart(
        &mut self,
        player_id: PlayerId,
        found: &Village,
        price: u32,
    ) -> PlayerScopedCommandOutcome {
        let around = TilePosition {
            x: found.center_column(),
            y: found.ground_row,
        };
        let sites: Vec<_> = self
            .unknown_sites_near(player_id, around, CHART_RADIUS_COLUMNS)
            .into_iter()
            .take(CHART_SITES)
            .collect();
        let settings = *self.terrain.settings();
        let neighbours: Vec<VillageId> = (found.site - 2..=found.site + 2)
            .flat_map(|site| {
                [
                    village::placement::surface_id(site),
                    village::placement::underground_id(site),
                ]
            })
            .filter(|id| *id != found.id && village::village(&settings, *id).is_some())
            .filter(|id| {
                self.players
                    .get(&player_id)
                    .is_some_and(|player| !player.villages.knows(*id))
            })
            .collect();
        if (sites.is_empty() && neighbours.is_empty()) || !self.charge(player_id, price) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        for site in &sites {
            let _ = self.reveal_scanner_area(player_id, site.center, CHART_REVEAL_RADIUS);
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            for site in &sites {
                player.journal.learn(site.id, PoiKnowledge::Identified);
            }
            player.villages.charted.extend(neighbours);
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn village_rumour(
        &mut self,
        player_id: PlayerId,
        found: &Village,
        price: u32,
    ) -> PlayerScopedCommandOutcome {
        let around = TilePosition {
            x: found.center_column(),
            y: found.ground_row,
        };
        let Some(site) = self
            .unknown_sites_near(player_id, around, RUMOUR_RADIUS_COLUMNS)
            .into_iter()
            .next()
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if !self.charge(player_id, price) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        if let Some(player) = self.players.get_mut(&player_id) {
            player.journal.learn(site.id, PoiKnowledge::Echo);
        }
        PlayerScopedCommandOutcome::Applied
    }
}

/// Cost of `units` at `per_hundred` credits per hundred units (rounded up).
const fn metered_cost(units: u32, per_hundred: u32) -> u32 {
    (units * per_hundred).div_ceil(100)
}

/// How many of `missing` units `credits` pays for at `per_hundred` credits per hundred units.
const fn metered_units(missing: u32, per_hundred: u32, credits: u32) -> u32 {
    if per_hundred == 0 {
        return missing;
    }
    let mut units = credits.saturating_mul(100) / per_hundred;
    if units > missing {
        units = missing;
    }
    while units > 0 && metered_cost(units, per_hundred) > credits {
        units -= 1;
    }
    units
}
