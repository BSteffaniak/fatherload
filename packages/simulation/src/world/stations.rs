//! Space stations in the world (Wave 12 stations, see [`crate::station`]): the hub's actions
//! at a docked station, missions moving on (scans, ore dug on a body, deliveries), a world's
//! hazards on its ground, and the ship's gear at work in flight.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use crate::{
    event::{GameplayEventKind, TickOutput},
    flight::{FlightPlace, StationService, nav::NavTarget},
    ids::PlayerId,
    item::ItemKind,
    place::StoreKey,
    position::WorldPos,
    space::{SpaceBodyId, SpaceTile},
    station::{
        AlienTech, Mission, MissionKind, ShipGear, ShipUpgrade, StationAction, StationEvent,
        StationId, StationLedger, StationProfile, StationRefusal,
        bar::{ALIEN_ROUND_PRICE, Patron, ROUND_PRICE, patrons},
        bounty::{BOARD_SLOTS, BoardContext, MAX_ACTIVE, board_context, offer},
        hazard::{drain, hazards_of},
        market::{Good, Quote, ore_price, price, trade_lot},
        profile::{StationOwner, profile_at, station_at},
        shipyard::{WEAVE_HULL_PER_SECOND, WEAVE_HULL_SHARE},
        station_hash,
    },
    summary::PlayerScopedCommandOutcome,
};

use super::WorldState;

/// A scan reaches bodies whose surface is within this many tiles of the ping (twice as far
/// with the xeno lens).
pub const SCAN_BODY_TILES: i64 = 8_000;

const SALT_SALVAGE: u64 = 0x57A7_0401;

const fn station_event(what: StationEvent, player_id: PlayerId, detail: u32) -> GameplayEventKind {
    GameplayEventKind::Station {
        what,
        player_id,
        detail,
    }
}

/// Board contexts, memoised per world seed and station (pure data: the space round a station
/// never changes).
fn cached_context(world: &WorldState, station: StationId) -> Option<Arc<BoardContext>> {
    type Cache = Mutex<BTreeMap<(u64, StationId), Arc<BoardContext>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let seed = world.world_seed();
    let cache = CACHE.get_or_init(Mutex::default);
    let found = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(seed, station))
        .map(Arc::clone);
    if found.is_some() {
        return found;
    }
    let galaxy = world.space();
    let context = Arc::new(board_context(galaxy.space(), station)?);
    {
        let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= 256 {
            cache.clear();
        }
        cache.insert((seed, station), Arc::clone(&context));
    }
    Some(context)
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// The crew's station state.
    #[must_use]
    pub const fn station_ledger(&self) -> &StationLedger {
        &self.stations
    }

    /// Replaces the crew's station state (scenes, tests, tools).
    pub fn set_station_ledger(&mut self, ledger: StationLedger) {
        self.stations = ledger;
    }

    /// The profile of the station `player_id`'s rig is docked at.
    #[must_use]
    pub fn docked_station(&self, player_id: PlayerId) -> Option<StationProfile> {
        let FlightPlace::Docked(body) = self.players.get(&player_id)?.flight.place else {
            return None;
        };
        profile_at(&self.space(), body)
    }

    /// The board of `station`: its three slots' missions (`None`: nothing to offer).
    #[must_use]
    pub fn station_board(&self, station: StationId) -> Vec<Option<Mission>> {
        let Some(context) = cached_context(self, station) else {
            return Vec::new();
        };
        let serials = self.stations.serials(station);
        (0..BOARD_SLOTS)
            .map(|slot| {
                offer(
                    self.world_seed(),
                    &context,
                    slot,
                    serials[usize::from(slot)],
                )
            })
            .collect()
    }

    /// The patrons at `station`'s bar.
    #[must_use]
    pub fn station_patrons(&self, station: StationId) -> Vec<Patron> {
        type Cache = Mutex<BTreeMap<(u64, StationId), Arc<Vec<Patron>>>>;
        static CACHE: OnceLock<Cache> = OnceLock::new();
        let Some(context) = cached_context(self, station) else {
            return Vec::new();
        };
        let Some(profile) = context.station.as_ref() else {
            return Vec::new();
        };
        // The bar is drawn every frame: its patrons are memoised (pure data).
        let key = (self.world_seed(), station);
        let cache = CACHE.get_or_init(Mutex::default);
        let found = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
            .map(Arc::clone);
        if let Some(found) = found {
            return found.as_ref().clone();
        }
        let made = Arc::new(patrons(self.space().space(), profile, &context));
        {
            let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&made));
        }
        made.as_ref().clone()
    }

    /// `good`'s quote at `station` now.
    #[must_use]
    pub fn station_quote(&self, station: &StationProfile, good: Good) -> Option<Quote> {
        price(
            self.world_seed(),
            station,
            good,
            self.stations.shift(station.id, good),
            self.millis_at(self.simulation_tick),
        )
    }

    /// `player_id`'s ship gear.
    #[must_use]
    pub fn ship_gear(&self, player_id: PlayerId) -> ShipGear {
        self.players
            .get(&player_id)
            .map(|player| player.hazard_gear.ship)
            .unwrap_or_default()
    }

    /// Units `player_id`'s hold carries.
    #[must_use]
    pub fn hold_capacity_of(&self, player_id: PlayerId) -> u32 {
        self.ship_gear(player_id).hold_capacity()
    }

    /// What `player_id`'s cargo bay's ore and relics fetch at `station`.
    #[must_use]
    pub fn ore_offer(&self, player_id: PlayerId, station: &StationProfile) -> u32 {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let minerals: u32 = player
            .cargo_bay
            .minerals()
            .map(|(mineral, count)| ore_price(station, mineral.value()).saturating_mul(count))
            .sum();
        let artifacts: u32 = player
            .cargo_bay
            .artifacts()
            .map(|(artifact, count)| ore_price(station, artifact.value()).saturating_mul(count))
            .sum();
        minerals.saturating_add(artifacts)
    }

    // --- The hub ------------------------------------------------------------------------------

    fn station_refuse(
        player_id: PlayerId,
        refusal: StationRefusal,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        output.push_event(station_event(
            StationEvent::Refused,
            player_id,
            refusal.code(),
        ));
        PlayerScopedCommandOutcome::IgnoredUnavailable
    }

    /// Whether the docked station offers Wave 11's counter (refuel, repair, flight gear); a
    /// derelict refuses with an event. `Some` is the price of a unit of fuel and a point of
    /// hull there.
    pub(super) fn dock_counter(
        &self,
        player_id: PlayerId,
        service: StationService,
        output: &mut TickOutput,
    ) -> Option<(u32, u32)> {
        let profile = self.docked_station(player_id)?;
        if matches!(service, StationService::Hub(_)) || profile.dock_services {
            return Some((profile.fuel_price, profile.repair_price));
        }
        let _ = Self::station_refuse(player_id, StationRefusal::NotHere, output);
        None
    }

    /// A hub action at the station `player_id` is docked at (see [`StationAction`]).
    pub(super) fn station_hub(
        &mut self,
        player_id: PlayerId,
        action: StationAction,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        let Some(profile) = self.docked_station(player_id) else {
            return Self::station_refuse(player_id, StationRefusal::NotDocked, output);
        };
        let result = match action {
            StationAction::Fit(upgrade) => self.hub_fit(player_id, &profile, upgrade, output),
            StationAction::BuyGood(good, units) => {
                self.hub_buy(player_id, &profile, good, units, output)
            }
            StationAction::SellGood(good, units) => {
                self.hub_sell(player_id, &profile, good, units, output)
            }
            StationAction::SellOre => self.hub_sell_ore(player_id, &profile, output),
            StationAction::Accept(slot) => self.hub_accept(player_id, &profile, slot, output),
            StationAction::Abandon(index) => self.hub_abandon(player_id, index, output),
            StationAction::TurnIn => self.hub_turn_in(player_id, &profile, output),
            StationAction::BuyRound(patron) => self.hub_round(player_id, &profile, patron, output),
            StationAction::Salvage => self.hub_salvage(player_id, &profile, output),
        };
        match result {
            Ok(()) => PlayerScopedCommandOutcome::Applied,
            Err(refusal) => Self::station_refuse(player_id, refusal, output),
        }
    }

    fn hub_fit(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        upgrade: ShipUpgrade,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        let sold = upgrade.sold_to(profile);
        if sold == 0 {
            return Err(StationRefusal::NotHere);
        }
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(StationRefusal::NotDocked)?;
        let level = upgrade.level(&player.hazard_gear.ship);
        if level >= upgrade.max_level() || level >= sold {
            return Err(StationRefusal::Maxed);
        }
        let price = upgrade.price(level);
        if player.credits < price {
            return Err(StationRefusal::Credits);
        }
        player.credits -= price;
        player.hazard_gear.ship.fit(upgrade);
        let what = match upgrade {
            ShipUpgrade::Line(_) => StationEvent::ShipUpgraded,
            ShipUpgrade::Gear(_) => StationEvent::GearFitted,
            ShipUpgrade::Alien(_) => StationEvent::AlienTechFitted,
        };
        output.push_event(station_event(what, player_id, u32::from(level + 1)));
        output.push_event(GameplayEventKind::PurchaseCompleted { player_id });
        Ok(())
    }

    fn hub_buy(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        good: Good,
        units: u16,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.market {
            return Err(StationRefusal::NotHere);
        }
        if self.station_quote(profile, good).is_none() {
            return Err(StationRefusal::Nothing);
        }
        let seed = self.world_seed();
        let now = self.millis_at(self.simulation_tick);
        let shift_before = self.stations.shift(profile.id, good);
        let room = self
            .hold_capacity_of(player_id)
            .saturating_sub(self.stations.hold_used(player_id));
        if room == 0 {
            return Err(StationRefusal::HoldFull);
        }
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(StationRefusal::NotDocked)?;
        // Priced unit by unit: the lot pays its own price impact.
        let (lot, cost) = trade_lot(
            seed,
            profile,
            good,
            shift_before,
            now,
            u32::from(units).min(room),
            true,
            player.credits,
        )
        .ok_or(StationRefusal::Nothing)?;
        if lot == 0 {
            return Err(StationRefusal::Credits);
        }
        player.credits -= cost;
        self.stations
            .holds
            .entry(player_id)
            .or_default()
            .add(good, lot);
        let shift = self
            .stations
            .shift(profile.id, good)
            .traded(now, i32::try_from(lot).unwrap_or(i32::MAX));
        self.stations.market.insert((profile.id, good), shift);
        output.push_event(station_event(StationEvent::GoodsBought, player_id, cost));
        Ok(())
    }

    fn hub_sell(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        good: Good,
        units: u16,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.market {
            return Err(StationRefusal::NotHere);
        }
        if self.station_quote(profile, good).is_none() {
            return Err(StationRefusal::Nothing);
        }
        let now = self.millis_at(self.simulation_tick);
        let shift_before = self.stations.shift(profile.id, good);
        let lot = self
            .stations
            .holds
            .get_mut(&player_id)
            .map_or(0, |hold| hold.take(good, u32::from(units)));
        if lot == 0 {
            return Err(StationRefusal::Nothing);
        }
        if self
            .stations
            .holds
            .get(&player_id)
            .is_some_and(|hold| hold.goods.is_empty())
        {
            self.stations.holds.remove(&player_id);
        }
        // Priced unit by unit, like buying.
        let (_, paid) = trade_lot(
            self.world_seed(),
            profile,
            good,
            shift_before,
            now,
            lot,
            false,
            0,
        )
        .unwrap_or((lot, 0));
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_add(paid);
        }
        let shift = self
            .stations
            .shift(profile.id, good)
            .traded(now, -i32::try_from(lot).unwrap_or(i32::MAX));
        self.stations.market.insert((profile.id, good), shift);
        output.push_event(station_event(StationEvent::GoodsSold, player_id, paid));
        Ok(())
    }

    fn hub_sell_ore(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.market {
            return Err(StationRefusal::NotHere);
        }
        let paid = self.ore_offer(player_id, profile);
        if paid == 0 {
            return Err(StationRefusal::Nothing);
        }
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(StationRefusal::NotDocked)?;
        player.cargo_bay.clear_ore_and_artifacts();
        player.credits = player.credits.saturating_add(paid);
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(paid);
        output.push_event(station_event(StationEvent::OreSold, player_id, paid));
        Ok(())
    }

    fn hub_accept(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        slot: u8,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.bounties {
            return Err(StationRefusal::NotHere);
        }
        if self.stations.missions_of(player_id).len() >= MAX_ACTIVE {
            return Err(StationRefusal::TooManyMissions);
        }
        let mission = self
            .station_board(profile.id)
            .get(usize::from(slot))
            .copied()
            .flatten()
            .ok_or(StationRefusal::Nothing)?;
        let room = self
            .hold_capacity_of(player_id)
            .saturating_sub(self.stations.hold_used(player_id));
        if mission.kind.hold_units() > room {
            return Err(StationRefusal::HoldFull);
        }
        // A bounty is posted for the hunter (the ships come when the rig flies near).
        let bounty = crate::station::bounty::bounty_target_of(self.world_seed(), mission.kind);
        if let Some(target) = bounty.clone()
            && !self.post_bounty(player_id, target)
        {
            return Err(StationRefusal::Nothing);
        }
        let serials = self.stations.boards.entry(profile.id).or_default();
        serials[usize::from(slot)] = serials[usize::from(slot)].saturating_add(1);
        self.stations
            .missions
            .entry(player_id)
            .or_default()
            .push(mission);
        // The nav waypoint goes on the new job's target (and its system is charted), so the
        // player can fly it straight from the board.
        let galaxy = self.space();
        let target = match mission.kind {
            MissionKind::Scan { body } | MissionKind::Mine { body, .. } => {
                galaxy.body_ref(body).map(NavTarget::Body)
            }
            MissionKind::Deliver { to, .. } => {
                crate::station::profile::station_ref(&galaxy, to).map(NavTarget::Body)
            }
            // A bounty's waypoint is the system its target lurks in.
            MissionKind::Bounty { .. } => bounty.map(|target| NavTarget::System(target.system)),
        };
        if let Some(target) = target {
            if !target.system().is_home() {
                self.flight.found(target.system());
            }
            self.flight.waypoints.insert(player_id, target);
        }
        output.push_event(station_event(
            StationEvent::MissionAccepted,
            player_id,
            mission.reward,
        ));
        Ok(())
    }

    fn hub_abandon(
        &mut self,
        player_id: PlayerId,
        index: u8,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        let missions = self
            .stations
            .missions
            .get_mut(&player_id)
            .ok_or(StationRefusal::Nothing)?;
        if usize::from(index) >= missions.len() {
            return Err(StationRefusal::Nothing);
        }
        let gone = missions.remove(usize::from(index));
        if missions.is_empty() {
            self.stations.missions.remove(&player_id);
        }
        // An abandoned bounty is taken down (its ships stop hunting for this player).
        if let MissionKind::Bounty { target, .. } = gone.kind {
            self.hostiles
                .bounties
                .retain(|posted| !(posted.target.id == target && posted.hunter == player_id));
        }
        output.push_event(station_event(
            StationEvent::MissionAbandoned,
            player_id,
            u32::from(index),
        ));
        Ok(())
    }

    fn hub_turn_in(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.owner.crewed() {
            return Err(StationRefusal::NotHere);
        }
        let Some(missions) = self.stations.missions.get_mut(&player_id) else {
            return Err(StationRefusal::Nothing);
        };
        // Deliveries for this station are handed over here.
        for mission in missions.iter_mut() {
            if let MissionKind::Deliver { to, units, .. } = mission.kind
                && to == profile.id
            {
                mission.progress = units;
            }
        }
        let mut paid = 0_u32;
        let mut count = 0_u32;
        missions.retain(|mission| {
            if mission.ready() {
                paid = paid.saturating_add(mission.reward);
                count += 1;
                false
            } else {
                true
            }
        });
        if missions.is_empty() {
            self.stations.missions.remove(&player_id);
        }
        if count == 0 {
            return Err(StationRefusal::Nothing);
        }
        *self.stations.completed.entry(player_id).or_insert(0) += count;
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_add(paid);
        }
        self.progression.total_earnings = self.progression.total_earnings.saturating_add(paid);
        output.push_event(station_event(StationEvent::MissionPaid, player_id, paid));
        Ok(())
    }

    fn hub_round(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        patron: u8,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.bar {
            return Err(StationRefusal::NotHere);
        }
        let patrons = self.station_patrons(profile.id);
        let rumour = patrons
            .get(usize::from(patron))
            .and_then(|found| found.rumour.clone())
            .ok_or(StationRefusal::Nothing)?;
        let price = if matches!(profile.owner, StationOwner::Alien(_)) {
            ALIEN_ROUND_PRICE
        } else {
            ROUND_PRICE
        };
        let player = self
            .players
            .get_mut(&player_id)
            .ok_or(StationRefusal::NotDocked)?;
        if player.credits < price {
            return Err(StationRefusal::Credits);
        }
        player.credits -= price;
        self.stations.heard.insert((profile.id, patron));
        let galaxy = self.space();
        if let Some(lair) = rumour
            .lair
            .and_then(|faction| crate::hostiles::boss_lair(self.world_seed(), faction))
        {
            // A boss's lair: its system charted and the waypoint on it (Wave 12 hostiles).
            if !lair.system.is_home() {
                self.flight.found(lair.system);
            }
            self.flight
                .waypoints
                .insert(player_id, NavTarget::System(lair.system));
        } else if let Some(body) = galaxy.body_ref(rumour.body) {
            if !body.system.is_home() {
                self.flight.found(body.system);
            }
            self.flight
                .waypoints
                .insert(player_id, NavTarget::Body(body));
        }
        output.push_event(station_event(
            StationEvent::RumourHeard,
            player_id,
            u32::from(patron),
        ));
        Ok(())
    }

    fn hub_salvage(
        &mut self,
        player_id: PlayerId,
        profile: &StationProfile,
        output: &mut TickOutput,
    ) -> Result<(), StationRefusal> {
        if !profile.salvage {
            return Err(StationRefusal::NotHere);
        }
        if self.stations.salvaged.contains(&profile.id) {
            return Err(StationRefusal::Stripped);
        }
        let roll = station_hash(self.world_seed(), profile.id, SALT_SALVAGE);
        let credits =
            crate::endgame::economy::space_credits(400 + u32::try_from(roll % 600).unwrap_or(0));
        let room = self
            .hold_capacity_of(player_id)
            .saturating_sub(self.stations.hold_used(player_id));
        let alloy = room.min(3);
        let relic = room.saturating_sub(alloy).min(1);
        let hold = self.stations.holds.entry(player_id).or_default();
        hold.add(Good::SalvagedAlloy, alloy);
        hold.add(Good::BuilderRelic, relic);
        if hold.goods.is_empty() {
            self.stations.holds.remove(&player_id);
        }
        self.stations.salvaged.insert(profile.id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_add(credits);
        }
        output.push_event(station_event(StationEvent::Salvaged, player_id, credits));
        Ok(())
    }

    // --- Missions moving on -------------------------------------------------------------------

    /// Adds `units` to `player_id`'s missions that `matches`; raises progress and ready events.
    fn advance_missions(
        &mut self,
        player_id: PlayerId,
        units: u16,
        matches: impl Fn(&MissionKind) -> bool,
        output: &mut TickOutput,
    ) {
        let Some(missions) = self.stations.missions.get_mut(&player_id) else {
            return;
        };
        for mission in missions.iter_mut() {
            if mission.ready() || !matches(&mission.kind) {
                continue;
            }
            mission.progress = mission
                .progress
                .saturating_add(units)
                .min(mission.kind.goal());
            let what = if mission.ready() {
                StationEvent::MissionReady
            } else {
                StationEvent::MissionProgress
            };
            output.push_event(station_event(what, player_id, u32::from(mission.progress)));
        }
    }

    /// `player_id` scanned `body`: the crew's record and the player's scan missions.
    pub(super) fn note_body_scan(
        &mut self,
        player_id: PlayerId,
        body: SpaceBodyId,
        output: &mut TickOutput,
    ) {
        if self.stations.scanned.insert(body) {
            output.push_event(station_event(
                StationEvent::BodyScanned,
                player_id,
                u32::from(body.body),
            ));
        }
        self.advance_missions(
            player_id,
            1,
            |kind| matches!(kind, MissionKind::Scan { body: target } if *target == body),
            output,
        );
    }

    /// A scanner ping in space at `centre`: every body whose surface is in reach is scanned.
    pub(super) fn note_ping_scans(
        &mut self,
        player_id: PlayerId,
        centre: WorldPos,
        output: &mut TickOutput,
    ) {
        let lens = self.ship_gear(player_id).has(AlienTech::XenoLens);
        let reach = if lens {
            SCAN_BODY_TILES * 2
        } else {
            SCAN_BODY_TILES
        };
        let hits = self
            .space()
            .space()
            .bodies_near(SpaceTile::of_pos(centre), reach);
        for hit in hits {
            if matches!(
                hit.kind,
                crate::space::BodyKind::HomeWorld | crate::space::BodyKind::HomeMoon
            ) {
                continue;
            }
            self.note_body_scan(player_id, hit.id, output);
        }
    }

    /// The scanner used on a body's ground scans the body (then the ground scanner runs).
    pub(super) fn note_ground_scan(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        if let StoreKey::Body(body) = self.store_of(player_id) {
            self.note_body_scan(player_id, body, output);
        }
    }

    /// Ore collected in `store`'s ground pass counts toward quotas on that body.
    pub(super) fn credit_store_ore(&mut self, store: StoreKey, output: &mut TickOutput) {
        let StoreKey::Body(body) = store else {
            return;
        };
        if self.stations.missions.is_empty() {
            return;
        }
        let collected: Vec<(PlayerId, u32)> = output
            .events
            .iter()
            .filter_map(|event| match event.kind {
                GameplayEventKind::ItemsCollected {
                    player_id,
                    item: ItemKind::Mineral(_),
                    count,
                    ..
                } => Some((player_id, count)),
                _ => None,
            })
            .collect();
        for (player_id, count) in collected {
            let units = u16::try_from(count).unwrap_or(u16::MAX);
            self.advance_missions(
                player_id,
                units,
                |kind| matches!(kind, MissionKind::Mine { body: target, .. } if *target == body),
                output,
            );
        }
    }

    /// Asteroid ore mined from flight counts toward quotas on bodies of its system: the
    /// systems track's belt mining calls this with the rock's system (quotas name a body, so
    /// ore from a belt counts toward any quota in that system).
    pub fn credit_asteroid_ore(
        &mut self,
        player_id: PlayerId,
        system: crate::space::SystemRef,
        units: u16,
        output: &mut TickOutput,
    ) {
        self.advance_missions(
            player_id,
            units,
            |kind| {
                matches!(kind, MissionKind::Mine { body, .. }
                    if body.sector == system.sector && body.system == system.system)
            },
            output,
        );
    }

    /// A bounty `target` was claimed by `hunter` (the kill paid its reward, Wave 12 hostiles):
    /// the board's mission for it is done and closes.
    pub(super) fn bounty_mission_claimed(
        &mut self,
        hunter: PlayerId,
        target: u64,
        output: &mut TickOutput,
    ) {
        let Some(missions) = self.stations.missions.get_mut(&hunter) else {
            return;
        };
        let Some(index) = missions.iter().position(|mission| {
            matches!(mission.kind, MissionKind::Bounty { target: id, .. } if id == target)
        }) else {
            return;
        };
        let mission = missions.remove(index);
        if missions.is_empty() {
            self.stations.missions.remove(&hunter);
        }
        *self.stations.completed.entry(hunter).or_insert(0) += 1;
        output.push_event(station_event(
            StationEvent::MissionPaid,
            hunter,
            mission.reward,
        ));
    }

    // --- The ship at work ---------------------------------------------------------------------

    /// A world's hazards on a rig on its ground for one tick (in the body's ground pass).
    pub(super) fn body_hazards(
        &mut self,
        player_id: PlayerId,
        body: SpaceBodyId,
        seconds: f32,
        output: &mut TickOutput,
    ) {
        let Some(found) = self.space().space().body(body) else {
            return;
        };
        // A wild or frontier world applies its hazards zone by zone through its own rules
        // (reading the same planet gear) and says so where they bite: the landing's flat
        // drain and its warning stand back (a deep stratum's heat is no news on a cold
        // shore; Wave 12 integration).
        if crate::station::hazard::theme_rules_hazards(&found) {
            return;
        }
        let hazards = hazards_of(&found);
        if hazards.is_empty() {
            return;
        }
        let thrusting = self.far_schedule.thrusting.contains(&player_id);
        let infinite_fuel = self.test_mode.is_some_and(|state| state.infinite_fuel);
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let taken = drain(&hazards, &player.hazard_gear.ship, thrusting);
        // The hull and fuel drains are shares of the rig's own hull and tank.
        let hull_lost = taken.hull * player.max_hull() * seconds;
        if !infinite_fuel {
            let fuel_lost = taken.fuel * player.fuel_capacity * seconds;
            player.fuel = (player.fuel - fuel_lost).max(0.0);
        }
        player.flight.air = taken.air.mul_add(-seconds, player.flight.air).max(0.0);
        let warned = match self.stations.warned.get(&player_id) {
            Some((on, bits)) if *on == body => *bits,
            _ => 0,
        };
        let fresh = taken.biting & !warned;
        if fresh != 0 {
            self.stations
                .warned
                .insert(player_id, (body, warned | taken.biting));
            // Every hazard that bites for the first time is named (a hot toxic world warns
            // of both).
            for (hazard, _) in hazards
                .iter()
                .filter(|(hazard, _)| fresh & hazard.bit() != 0)
            {
                output.push_event(station_event(
                    StationEvent::HazardBiting,
                    player_id,
                    hazard.code(),
                ));
            }
        }
        if hull_lost > 0.0 && self.damage_player(player_id, hull_lost) {
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
        }
    }

    /// The ship's gear for one tick away in space (not on a body): the symbiont weave knits
    /// the hull.
    pub(super) fn ship_upkeep(&mut self, player_id: PlayerId, seconds: f32) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if !player.hazard_gear.ship.has(AlienTech::SymbiontWeave)
            || matches!(player.flight.place, FlightPlace::Landed(_))
            || player.hull <= 0.0
        {
            return;
        }
        let cap = player.max_hull() * WEAVE_HULL_SHARE;
        if player.hull < cap {
            player.hull = WEAVE_HULL_PER_SECOND.mul_add(seconds, player.hull).min(cap);
        }
    }
}

/// The station a flight body names, if it is one (for tools that hold only flight's ids).
#[must_use]
pub fn station_profile_at(
    world: &WorldState,
    body: crate::flight::BodyRef,
) -> Option<StationProfile> {
    station_at(&world.space(), body).map(|(system, station)| {
        crate::station::profile::profile_of(world.world_seed(), &system, &station)
    })
}
