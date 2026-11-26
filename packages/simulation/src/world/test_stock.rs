//! The god-mode toolkit on the world (Wave 14, W14-A).
//!
//! See [`crate::test_mode`] and [`crate::test_mode::stock`]: stocking every rig with
//! everything, the infinite-supplies refill, the stock audit, the progress switches (Descent
//! gates, the Heart's ending, the story and codex, the home clock, the bodies' weather) and
//! the enemy tools.
//!
//! Every tool is a [`TestModeAction`] applied inside the tick (so it is replicated, the host's
//! world decides online, and each seat acts on its own rig). Outside a test-mode world they are
//! refused (`WorldState::apply_test_mode_action` checks first).

use std::collections::BTreeSet;

use crate::{
    PlayerId, SimulationTick,
    cosmetics::CosmeticItem,
    crust::{CRUST_LOGS, DRILL_II_LEVEL},
    depth_progression::RigSystem,
    descent::{DescentBoss, DescentKey, Stratum},
    economy::{UpgradeKind, current_level, max_out_upgrades},
    endgame::works::{MARK_MAX, StarwrightPart},
    enemy::EnemyKind,
    entity::{EntityId, EntityKind},
    expedition::{ECHOES, LOGS, anomaly::AnomalyKind, progress::boss_bit},
    flight::FlightGearKind,
    heart::{ENDING_SHIELD_MILLIS, HeartPhase},
    hostiles::{BossTech, ShipGearKind},
    old_deep::{CorePhase, LOG_CORE_REVEAL},
    pilot::{PilotGearKind, arsenal::PILOT_WEAPONS},
    player::Player,
    rig::RigPartKind,
    service::RecipeKind,
    star_chart::{MAX_CHARTS, chart_at_site},
    station::{AlienTech, ShipUpgrade},
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    test_mode::{StockItem, TEST_MODE_CREDITS, TestModeAction},
    tomb::{LORE_PAGES, relic::Relic},
    weapon::{MAX_BATTERY_LEVEL, MAX_WEAPON_MK, WeaponKind},
    workshop::DeepSupply,
    worldgen::{
        TileRect,
        descent::ancient_city::{orrery, orrery_plinths, stamps_with, vault_plinth},
    },
};

use super::{WorldState, expedition::BOSS_NOTED_SHIFT, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// How far round a rig "clear nearby enemies" reaches, in tiles.
pub const CLEAR_RADIUS_TILES: f32 = 48.0;
/// Columns either side of the world's middle whose Archive vaults' plinths a stocked world
/// has charted (with the Orrery's three): far more stars than a crew reads by hand.
pub const CHART_STOCK_COLUMNS: i32 = 1_200;
/// Minutes in a day on the clock face.
const MINUTES_PER_DAY: u64 = 24 * 60;

/// Fits `player` with every per-rig piece of gear: upgrades, weapons, the ship's lines and
/// alien tech, pilot gear and arms, cosmetics and the colony's one-offs (no consumables; see
/// [`top_up_rig`]).
pub(super) fn fit_everything(player: &mut Player) {
    max_out_upgrades(player);
    player.credits = player.credits.max(TEST_MODE_CREDITS);
    // Every weapon at its top tier and a full capacitor.
    for weapon in WeaponKind::ALL {
        let _ = player.receive_weapon(weapon);
        player.weapon_rack.owned.insert(weapon, MAX_WEAPON_MK);
    }
    player.weapon_rack.battery_level = MAX_BATTERY_LEVEL;
    if let Some(mounted) = player.mounted_weapon.as_mut() {
        mounted.mk = MAX_WEAPON_MK;
    }
    player.weapon.energy = player.battery_capacity();
    // The ship (Wave 12 stations): every line, planet gear and alien tech at its top.
    let ship = &mut player.hazard_gear.ship;
    for upgrade in ShipUpgrade::all() {
        while upgrade.level(ship) < upgrade.max_level() {
            ship.fit(upgrade);
        }
    }
    // The pilot's own gear and arms.
    for kind in PilotGearKind::ALL {
        while kind.tier(player.pilot_gear) < kind.max_tier() {
            player.pilot_gear = kind.upgraded(player.pilot_gear);
        }
    }
    for weapon in PILOT_WEAPONS {
        let _ = player.pilot_arsenal.receive(weapon);
    }
    for item in CosmeticItem::all() {
        let _ = player.cosmetic_unlocks.unlock(item);
    }
    // The lunar colony's one-off upgrades (WP-MOON).
    player.lunar.reactor = true;
    player.lunar.alloy_drill = true;
    player.fuel = player.fuel_capacity;
    player.hull = player.max_hull();
}

/// The count field of a carried infrastructure kit.
const fn kit_count(player: &mut Player, recipe: RecipeKind) -> Option<&mut u32> {
    Some(match recipe {
        RecipeKind::SignalRelayKit => &mut player.signal_relay_kits,
        RecipeKind::SurveyDroneKit => &mut player.survey_drone_kits,
        RecipeKind::CargoLiftKit => &mut player.cargo_lift_kits,
        RecipeKind::TunnelSupportKit => &mut player.tunnel_support_kits,
        RecipeKind::PumpStationKit => &mut player.pump_station_kits,
        RecipeKind::OreProcessorKit => &mut player.ore_processor_kits,
        RecipeKind::ReinforcedBulkhead | RecipeKind::AuxiliaryTank | RecipeKind::ExpandedSorter => {
            return None;
        }
    })
}

/// `player`'s count of a stocked item kept on the rig itself (`None` for the kit and rack
/// counts the world keeps).
fn rig_count(player: &Player, item: StockItem) -> Option<u32> {
    Some(match item {
        StockItem::Bombs => player.bombs,
        StockItem::Explosive(kind) => player.explosive_count(kind),
        StockItem::RigAmmo(ammo) => player.cargo_bay.count(ammo.item()),
        StockItem::PilotAmmo(ammo) => player.pilot_arsenal.ammo(ammo),
        StockItem::Kit(recipe) => match recipe {
            RecipeKind::SignalRelayKit => player.signal_relay_kits,
            RecipeKind::SurveyDroneKit => player.survey_drone_kits,
            RecipeKind::CargoLiftKit => player.cargo_lift_kits,
            RecipeKind::TunnelSupportKit => player.tunnel_support_kits,
            RecipeKind::PumpStationKit => player.pump_station_kits,
            RecipeKind::OreProcessorKit => player.ore_processor_kits,
            RecipeKind::ReinforcedBulkhead
            | RecipeKind::AuxiliaryTank
            | RecipeKind::ExpandedSorter => return None,
        },
        StockItem::Supply(_) | StockItem::DiveCharges | StockItem::ShipMissiles => return None,
    })
}

/// Sets `player`'s count of a rig-kept item to `count`; `false` for the world-kept ones.
fn set_rig_count(player: &mut Player, item: StockItem, count: u32) -> bool {
    match item {
        StockItem::Bombs => player.bombs = count,
        StockItem::Explosive(kind) => {
            let Some(carried) = kind.item() else {
                return false;
            };
            let _removed = player.cargo_bay.remove(carried, u32::MAX);
            player.cargo_bay.add(carried, count);
        }
        StockItem::RigAmmo(ammo) => {
            let _removed = player.cargo_bay.remove(ammo.item(), u32::MAX);
            player.cargo_bay.add(ammo.item(), count);
        }
        StockItem::PilotAmmo(ammo) => {
            player.pilot_arsenal.ammo.remove(&ammo);
            let _added = player.pilot_arsenal.add_ammo(ammo, count);
        }
        StockItem::Kit(recipe) => {
            let Some(slot) = kit_count(player, recipe) else {
                return false;
            };
            *slot = count;
        }
        StockItem::Supply(_) | StockItem::DiveCharges | StockItem::ShipMissiles => return false,
    }
    true
}

/// Tops `player`'s rig-kept consumables up to their stacks (never lowers a count above one).
pub(super) fn top_up_rig(player: &mut Player) {
    for item in StockItem::all() {
        if let Some(have) = rig_count(player, item)
            && have < item.stack()
        {
            let _set = set_rig_count(player, item, item.stack());
        }
    }
}

/// The rig system a boss-won gate key stands for (`None` for the world-held Deep Claim, the
/// Drill II tier and the Star Drive). Exhaustive, so a new key must say how it is held.
const fn key_system(key: DescentKey) -> Option<RigSystem> {
    Some(match key {
        DescentKey::PressureHull => RigSystem::PressureHull,
        DescentKey::BioFilter => RigSystem::BioFilter,
        DescentKey::LampII => RigSystem::LampII,
        DescentKey::SonarSuite => RigSystem::SonarSuite,
        DescentKey::CryoHull => RigSystem::CryoHull,
        DescentKey::GravityAnchor => RigSystem::GravityAnchor,
        DescentKey::PhaseDrill => RigSystem::PhaseDrill,
        DescentKey::HeartKey => RigSystem::HeartKey,
        DescentKey::DeepClaim | DescentKey::DrillII | DescentKey::StarDrive => return None,
    })
}

impl WorldState {
    /// The forced body weather of the test-mode switch (Wave 14), if any.
    #[must_use]
    pub fn forced_weather(&self) -> Option<crate::bodylife::WeatherId> {
        self.test_mode.and_then(|state| state.weather)
    }

    // --- Stocking --------------------------------------------------------------------------

    /// Stocks the player template, every rig and the world with everything (a new test-mode
    /// world, and a test-mode save from before the current stocking revision).
    pub(super) fn stock_everything(&mut self) {
        fit_everything(&mut self.player_template);
        top_up_rig(&mut self.player_template);
        self.stock_world();
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in players {
            self.stock_player(player_id);
        }
    }

    /// Stocks one rig with everything: its gear, its consumables and what the world keeps for
    /// it (the deep-supply kit, dive charges, the Star Drive, the endgame's marks).
    pub(super) fn stock_player(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        fit_everything(player);
        self.stock_joiner(player_id);
    }

    /// Stocks a rig arriving in a test-mode world: its consumables and what the world keeps for
    /// it. Its gear is the player template's (fitted when test mode was switched on, or as a
    /// scenario set it since).
    pub(super) fn stock_joiner(&mut self, player_id: PlayerId) {
        self.top_up_supplies(player_id);
        // The ship's side (the Star Drive and the endgame's ship work: every alien mark, the
        // Starwright ship and the Heartline Drive) goes with a rig whose ship lines are all
        // fitted: a scenario that sets the template's ship back to stock, as the flight bots'
        // and the online soak's do, keeps a stock ship, drive and career too.
        let fitted_ship = self.players.get(&player_id).is_some_and(|player| {
            ShipUpgrade::all()
                .into_iter()
                .all(|upgrade| upgrade.level(&player.hazard_gear.ship) >= upgrade.max_level())
        });
        if fitted_ship {
            if self.space_enabled() {
                self.heart.space_unlocked = true;
                self.heart.star_drive.insert(player_id);
            }
            let career = self.endgame.careers.entry(player_id).or_default();
            career.marks = [MARK_MAX - 1; 4];
            career.starwright = StarwrightPart::ALL
                .iter()
                .fold(0, |bits, part| bits | part.bit());
            career.heartline = true;
        }
        if self.test_mode.is_some_and(|state| state.story_unlocked) {
            self.unlock_tomb_codex(player_id);
        }
    }

    /// The crew's side of the stock: every rig part in the locker, the Deep Claim, every boss
    /// tech, space open and every star chart of the Archive.
    fn stock_world(&mut self) {
        self.progression.rig_part_inventory.extend(RigPartKind::ALL);
        self.hostiles.tech.extend(BossTech::ALL);
        let settings = self.home_settings();
        if settings.generator.has_descents() {
            self.crust.deep_claim = true;
            self.crust.drill_ii.extend(self.players.keys().copied());
        }
        if self.space_enabled() {
            self.heart.space_unlocked = true;
        }
        let sites = self.chart_stock_sites();
        if !sites.is_empty() && self.ancient_city.charts.is_empty() {
            // The galaxy story's road starts from the crew's first chart (or, with none, from
            // the one the Archive's first plinth would chart): that chart goes first, so a
            // stocked world's road starts where a fresh world's does.
            let seed = self.home_settings().seed;
            self.ancient_city
                .charts
                .push(chart_at_site(seed, 0, 0, 0, SimulationTick::new(0)));
        }
        for site in sites {
            self.chart_site(site);
        }
    }

    /// The plinths a stocked world has read: the Orrery's three and every vault's within
    /// [`CHART_STOCK_COLUMNS`] (none off Deep Descent worlds).
    #[must_use]
    pub fn chart_stock_sites(&self) -> Vec<TilePosition> {
        let settings = self.home_settings();
        if !settings.generator.has_descents() {
            return Vec::new();
        }
        let layout = crate::descent::fields::cached_layout(&settings);
        let mut sites = Vec::new();
        if let Some(hall) = orrery(&settings, &layout) {
            sites.extend(orrery_plinths(&hall));
        }
        let (top, bottom) = layout.rows(Stratum::ArchiveOfStars);
        let area = TileRect {
            min: TilePosition {
                x: -CHART_STOCK_COLUMNS,
                y: top,
            },
            max: TilePosition {
                x: CHART_STOCK_COLUMNS,
                y: bottom,
            },
        };
        sites.extend(
            stamps_with(&settings, &layout, &area)
                .iter()
                .filter_map(vault_plinth),
        );
        sites
    }

    /// Adds the chart of the plinth at `site` (home world tiles) to the crew's list.
    fn chart_site(&mut self, site: TilePosition) {
        let settings = self.home_settings();
        let layout = crate::descent::fields::cached_layout(&settings);
        let chart = chart_at_site(
            settings.seed,
            site.x,
            site.y,
            layout.along(site.y),
            self.simulation_tick,
        );
        if !self.star_chart_known(chart.system) && self.ancient_city.charts.len() < MAX_CHARTS {
            self.ancient_city.charts.push(chart);
        }
    }

    /// Tops `player_id`'s consumables up to their stacks: the rig's own, the deep-supply kit,
    /// the dive charges and the ship's missile rack (never lowers a count).
    pub(super) fn top_up_supplies(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        top_up_rig(player);
        for item in StockItem::all() {
            if self.stock_count(player_id, item) < item.stack() {
                self.set_stock(player_id, item, item.stack());
            }
        }
    }

    /// How many of `item` `player_id` carries (the ship's rack: missiles loaded, or a full
    /// rack before the rig's first fight).
    #[must_use]
    pub fn stock_count(&self, player_id: PlayerId, item: StockItem) -> u32 {
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        if let Some(count) = rig_count(player, item) {
            return count;
        }
        match item {
            StockItem::Supply(supply) => u32::from(self.supplies_of(player_id, supply)),
            StockItem::DiveCharges => u32::from(self.logistics.dive_charges_of(player_id)),
            StockItem::ShipMissiles => u32::from(self.rig_combat(player_id).missiles),
            _ => 0,
        }
    }

    /// The most of `item` `player_id`'s rig holds (the ship's rack by its fitted gear).
    #[must_use]
    pub fn stock_limit(&self, player_id: PlayerId, item: StockItem) -> u32 {
        if item == StockItem::ShipMissiles {
            return u32::from(self.fresh_combat(player_id).missiles);
        }
        item.stack()
    }

    /// Sets `player_id`'s count of `item` (capped at its limit).
    fn set_stock(&mut self, player_id: PlayerId, item: StockItem, count: u32) {
        let count = count.min(self.stock_limit(player_id, item));
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        if set_rig_count(player, item, count) {
            return;
        }
        let small = u8::try_from(count).unwrap_or(u8::MAX);
        match item {
            StockItem::Supply(supply) => {
                let kit = self.logistics.supplies.entry(player_id).or_default();
                if kit.len() < DeepSupply::ALL.len() {
                    kit.resize(DeepSupply::ALL.len(), 0);
                }
                kit[supply.index()] = small;
            }
            StockItem::DiveCharges => {
                self.logistics.dive_charges.insert(player_id, small);
            }
            StockItem::ShipMissiles => {
                // A rack in use is loaded; one never used loads full on its first fight.
                if let Some(combat) = self.hostiles.rigs.get_mut(&player_id) {
                    combat.missiles = small;
                }
            }
            _ => {}
        }
    }

    /// Brings an older test-mode world up to the current stocking revision (top of the tick,
    /// with every rig at hand).
    pub(super) fn upgrade_test_stock(&mut self) {
        let Some(state) = self.test_mode else {
            return;
        };
        if state.stock_revision >= crate::test_mode::STOCK_REVISION {
            return;
        }
        self.stock_everything();
        if let Some(state) = self.test_mode.as_mut() {
            state.stock_revision = crate::test_mode::STOCK_REVISION;
        }
    }

    /// What `player_id` lacks of a full stock, one line each (empty when fully stocked). Walks
    /// every registry's `ALL` list, so the audit grows with the game (see
    /// [`crate::test_mode::stock`]).
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one check per registry")]
    pub fn test_stock_gaps(&self, player_id: PlayerId) -> Vec<String> {
        let Some(player) = self.players.get(&player_id) else {
            return vec!["no such rig".to_owned()];
        };
        let mut gaps = Vec::new();
        for item in StockItem::all() {
            let limit = self.stock_limit(player_id, item);
            let have = self.stock_count(player_id, item);
            if have < limit {
                gaps.push(format!("{}: {have} of {limit}", item.name()));
            }
        }
        for kind in UpgradeKind::ALL {
            if kind != UpgradeKind::BombPack && current_level(player, kind) < kind.max_level() {
                gaps.push(format!("upgrade {kind:?}"));
            }
        }
        for weapon in WeaponKind::ALL {
            if player.weapon_rack.owned.get(&weapon) != Some(&MAX_WEAPON_MK) {
                gaps.push(format!("weapon {weapon:?}"));
            }
        }
        for weapon in PILOT_WEAPONS {
            if !player.pilot_arsenal.owned.contains(&weapon) {
                gaps.push(format!("pilot arm {weapon:?}"));
            }
        }
        for part in RigPartKind::ALL {
            if !self.progression.rig_part_inventory.contains(&part) {
                gaps.push(format!("rig part {part:?}"));
            }
        }
        for kind in PilotGearKind::ALL {
            if kind.tier(player.pilot_gear) < kind.max_tier() {
                gaps.push(format!("pilot gear {kind:?}"));
            }
        }
        for item in CosmeticItem::all() {
            if !player.cosmetic_unlocks.has(item) {
                gaps.push(format!("cosmetic {item:?}"));
            }
        }
        for upgrade in ShipUpgrade::all() {
            if upgrade.level(&player.hazard_gear.ship) < upgrade.max_level() {
                gaps.push(format!("ship {upgrade:?}"));
            }
        }
        for kind in FlightGearKind::ALL {
            if kind.level(&player.hazard_gear) < kind.max_level() {
                gaps.push(format!("flight gear {kind:?}"));
            }
        }
        if player.hazard_gear.attitude != crate::attitude::AttitudeGear::MAXED {
            gaps.push("attitude gear (landing legs, assists)".to_owned());
        }
        for kind in ShipGearKind::ALL {
            if kind.level(player.hazard_gear.arms) < kind.max_level() {
                gaps.push(format!("ship arms {kind:?}"));
            }
        }
        for tech in BossTech::ALL {
            if !self.hostiles.has_tech(tech) {
                gaps.push(format!("boss tech {tech:?}"));
            }
        }
        if !(player.lunar.reactor && player.lunar.alloy_drill) {
            gaps.push("lunar colony one-offs".to_owned());
        }
        let career = self.endgame.careers.get(&player_id);
        for tech in AlienTech::ALL {
            if career.is_none_or(|career| career.mark(tech) < MARK_MAX) {
                gaps.push(format!("alien mark {tech:?}"));
            }
        }
        for part in StarwrightPart::ALL {
            if career.is_none_or(|career| !career.has_starwright(part)) {
                gaps.push(format!("starwright {part:?}"));
            }
        }
        if career.is_none_or(|career| !career.heartline) {
            gaps.push("heartline drive".to_owned());
        }
        let descents = self.home_settings().generator.has_descents();
        for key in DescentKey::ALL {
            let held = match key {
                DescentKey::DeepClaim => !descents || self.crust.deep_claim,
                DescentKey::DrillII => player.drill_strength >= DRILL_II_LEVEL,
                DescentKey::StarDrive => {
                    !self.space_enabled()
                        || (self.heart.space_unlocked && self.heart.has_star_drive(player_id))
                }
                _ => key_system(key).is_none_or(|system| player.hazard_gear.has_system(system)),
            };
            if !held {
                gaps.push(format!("gate reward {key:?}"));
            }
        }
        let settings = self.home_settings();
        let layout = descents.then(|| crate::descent::fields::cached_layout(&settings));
        for site in self.chart_stock_sites() {
            let along = layout.map_or(500, |layout| layout.along(site.y));
            let chart = chart_at_site(settings.seed, site.x, site.y, along, self.simulation_tick);
            if !self.star_chart_known(chart.system) {
                gaps.push(format!("star chart {}", chart.name));
            }
        }
        gaps
    }

    // --- The toolkit's actions -------------------------------------------------------------

    /// Applies a W14-A toolkit action (`None`: not one of the toolkit's).
    pub(super) fn apply_toolkit_action(
        &mut self,
        player_id: PlayerId,
        action: TestModeAction,
    ) -> Option<PlayerScopedCommandOutcome> {
        if !self.players.contains_key(&player_id) {
            return Some(PlayerScopedCommandOutcome::UnknownPlayer);
        }
        let state = self.test_mode.as_mut()?;
        Some(match action {
            TestModeAction::SetInfiniteSupplies(on) => {
                state.infinite_supplies = on;
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::SetAiFrozen(on) => {
                state.ai_frozen = on;
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::SetWeather(weather) => {
                state.weather = weather;
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::Restock => {
                self.top_up_supplies(player_id);
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::StockAll => {
                self.stock_world();
                self.stock_player(player_id);
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::Give(item, count) => {
                self.set_stock(player_id, item, count);
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::SetStoryUnlocked(on) => {
                state.story_unlocked = on;
                self.set_story_unlocked(on);
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::SetGateCleared { descent, cleared } => {
                self.set_gate_cleared(descent, cleared)
            }
            TestModeAction::SetEndingDone(done) => self.set_ending_done(done),
            TestModeAction::SetTimeOfDay { minutes } => self.set_time_of_day(minutes),
            TestModeAction::SetDayLock(on) => {
                self.set_day_lock(on);
                PlayerScopedCommandOutcome::Applied
            }
            TestModeAction::SpawnCreature(kind) => self.spawn_beside(player_id, kind),
            TestModeAction::ClearNearbyEnemies => self.clear_enemies_near(player_id),
            _ => return None,
        })
    }

    // --- Progress switches -----------------------------------------------------------------

    /// Whether Descent `descent`'s gate (0-7) counts as cleared.
    #[must_use]
    pub fn gate_cleared(&self, descent: u8) -> bool {
        DescentBoss::ALL
            .get(usize::from(descent))
            .is_some_and(|boss| self.descent_boss_beaten(*boss))
    }

    /// Marks Descent `descent`'s gate cleared or not (its boss's own fight state and the
    /// expedition's record).
    fn set_gate_cleared(&mut self, descent: u8, cleared: bool) -> PlayerScopedCommandOutcome {
        if !self.home_settings().generator.has_descents() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(boss) = DescentBoss::ALL.get(usize::from(descent)).copied() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match boss {
            DescentBoss::TheHeart => return self.set_ending_done(cleared),
            DescentBoss::TheSinkhole => {
                self.crust.deep_claim = cleared;
                if cleared {
                    self.crust.drill_ii.extend(self.players.keys().copied());
                }
            }
            DescentBoss::StarCore => {
                if cleared {
                    self.old_deep.core.phase = CorePhase::Beaten;
                } else if self.old_deep.core.phase == CorePhase::Beaten {
                    self.old_deep.core.phase = CorePhase::Dormant;
                }
            }
            DescentBoss::MycelialMind => self.living_dark.mind.beaten = cleared,
            DescentBoss::Leviathan => self.drowned.leviathan.beaten = cleared,
            DescentBoss::SunSerpent => self.hollow.serpent.beaten = cleared,
            DescentBoss::TheGlitch => self.far_strange.glitch.beaten = cleared,
            DescentBoss::GatekeeperColossus => self.garrison.colossus.beaten = cleared,
        }
        self.note_gate(boss, cleared);
        PlayerScopedCommandOutcome::Applied
    }

    /// Notes `boss` beaten in the expedition's record, or forgets it (both the noted and the
    /// reported bit, so a gate cleared again is reported again).
    const fn note_gate(&mut self, boss: DescentBoss, cleared: bool) {
        let noted = boss_bit(boss) << BOSS_NOTED_SHIFT;
        if cleared {
            self.expedition.bosses |= noted;
        } else {
            self.expedition.bosses &= !(noted | boss_bit(boss));
        }
    }

    /// Whether the Heart's ending is done (the Heart woke and the cable lit).
    #[must_use]
    pub const fn ending_done(&self) -> bool {
        self.heart.fight.beaten && self.expedition.cable_lit
    }

    /// Marks the Heart's ending done or not. Done: the Heart wakes (space opens and every rig
    /// gets the Star Drive) with its ending already played, so no sequence starts. Not done:
    /// the Heart sleeps again and the cable is dark (a stocked test world's space stays open).
    fn set_ending_done(&mut self, done: bool) -> PlayerScopedCommandOutcome {
        if !self.home_settings().generator.has_descents() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        // The fight's live parts are the home world's entities.
        if !self.active_store().is_home() && self.heart.fight.entity.is_some() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        if done {
            if !self.heart.fight.beaten {
                self.put_heart_to_sleep();
                let now = self.millis_at(self.simulation_tick);
                let fight = &mut self.heart.fight;
                fight.phase = HeartPhase::Awakened;
                fight.phase_millis = now;
                fight.beaten = true;
                // The ending counts as played: its sequence never starts (a world too young
                // for a lit time that long ago keeps the cable's time unset).
                self.heart.cable_lit_millis = now.checked_sub(ENDING_SHIELD_MILLIS + 1_000);
            }
            self.heart.space_unlocked = true;
            let players: Vec<PlayerId> = self.players.keys().copied().collect();
            self.heart.star_drive.extend(players);
            self.expedition.cable_lit = true;
        } else {
            self.put_heart_to_sleep();
            self.heart.cable_lit_millis = None;
            self.expedition.cable_lit = false;
        }
        self.note_gate(DescentBoss::TheHeart, done);
        PlayerScopedCommandOutcome::Applied
    }

    /// Unlocks every story log, echo and codex entry, or resets them to a fresh story.
    fn set_story_unlocked(&mut self, on: bool) {
        let expedition = &mut self.expedition;
        if on {
            expedition.found = u8::try_from(LOGS.len()).unwrap_or(u8::MAX);
            expedition.echoes = u8::try_from(ECHOES.len()).unwrap_or(u8::MAX);
            expedition.sealed.clear();
            expedition.anomaly_kinds = AnomalyKind::ALL
                .iter()
                .fold(0, |bits, kind| bits | kind.bit());
            self.crust.logs = (0..u8::try_from(CRUST_LOGS.len()).unwrap_or(u8::MAX)).collect();
            self.old_deep.logs_read = (0..=LOG_CORE_REVEAL).collect();
            let players: Vec<PlayerId> = self.players.keys().copied().collect();
            for player_id in players {
                self.unlock_tomb_codex(player_id);
            }
        } else {
            expedition.found = 0;
            expedition.echoes = 0;
            expedition.sealed.clear();
            expedition.anomaly_kinds = 0;
            self.crust.logs.clear();
            self.old_deep.logs_read.clear();
            for unlocks in self.tombs.unlocks.values_mut() {
                unlocks.codex.clear();
                unlocks.lore.clear();
            }
        }
    }

    /// Every relic and lore page in `player_id`'s tomb codex.
    fn unlock_tomb_codex(&mut self, player_id: PlayerId) {
        let unlocks = self.tombs.unlocks.entry(player_id).or_default();
        unlocks.codex.extend(Relic::ALL);
        unlocks
            .lore
            .extend(0..u8::try_from(LORE_PAGES.len()).unwrap_or(u8::MAX));
    }

    /// Sets the home clock to `minutes` past midnight: forward to its next such time, or
    /// there at once while the day/night lock holds the time.
    fn set_time_of_day(&mut self, minutes: u16) -> PlayerScopedCommandOutcome {
        if u64::from(minutes) >= MINUTES_PER_DAY {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let day = self.clock.day_length.millis();
        let target = u64::from(minutes) * day / MINUTES_PER_DAY;
        if let Some(held) = self.clock.held_millis.as_mut() {
            *held = target;
        } else {
            let now = self.clock().millis_into_day();
            self.clock.offset_millis = self
                .clock
                .offset_millis
                .saturating_add((target + day - now) % day);
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Holds the home clock's time of day where it is, or lets it run on from the held time.
    const fn set_day_lock(&mut self, on: bool) {
        match (on, self.clock.held_millis) {
            (true, None) => self.clock.held_millis = Some(self.clock().millis_into_day()),
            (false, Some(held)) => {
                self.clock.held_millis = None;
                let day = self.clock.day_length.millis();
                let now = self.clock().millis_into_day();
                self.clock.offset_millis = self
                    .clock
                    .offset_millis
                    .saturating_add((held + day - now) % day);
            }
            _ => {}
        }
    }

    // --- Enemy tools -----------------------------------------------------------------------

    /// Spawns `kind` at a safe offset beside `player_id`'s rig: the first open spot to either
    /// side or above it, else a pocket carved for it. Refused on a world whose creatures are
    /// switched off (Peaceful clears them every tick).
    fn spawn_beside(&mut self, player_id: PlayerId, kind: EnemyKind) -> PlayerScopedCommandOutcome {
        if !self.enemies.difficulty.hostile() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some((x, y)) = self
            .players
            .get(&player_id)
            .map(|player| (player.x, player.y))
        else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let radius = kind.stats().radius;
        let gap = 2.0f32.mul_add(TILE_SIZE, AUTHORITATIVE_PLAYER_RADIUS + radius);
        let offsets = [
            (gap, 0.0),
            (-gap, 0.0),
            (0.0, -gap),
            (gap * 1.75, 0.0),
            (-gap * 1.75, 0.0),
            (gap, -gap),
            (-gap, -gap),
        ];
        let spot = offsets.iter().find_map(|(dx, dy)| {
            crate::physics::free_spot_near(&self.terrain, x + dx, y + dy, radius, TILE_SIZE)
        });
        let (sx, sy) = spot.unwrap_or_else(|| {
            let at = (x + gap, y);
            self.carve_spawn_pocket(at, radius);
            at
        });
        let _ = self.spawn_enemy(kind, sx, sy);
        PlayerScopedCommandOutcome::Applied
    }

    /// Opens a pocket of air for a creature of `radius` at `(x, y)` (bedrock, fixtures and
    /// built tiles stay).
    fn carve_spawn_pocket(&mut self, (x, y): (f32, f32), radius: f32) {
        let centre = self.frame().tile_at(x, y);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a creature's radius is a few tiles"
        )]
        let reach = (radius / TILE_SIZE).ceil() as i32 + 1;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let position = TilePosition {
                    x: centre.x + dx,
                    y: centre.y + dy,
                };
                let keep = self.terrain.tile(position).is_none_or(|tile| {
                    matches!(
                        tile.kind,
                        TileKind::Air
                            | TileKind::Bedrock
                            | TileKind::Poi(_)
                            | TileKind::Foundation
                            | TileKind::Built(_)
                    )
                });
                if !keep {
                    let _written = self.terrain.set_kind(position, TileKind::Air);
                }
            }
        }
    }

    /// Removes every creature and creature shot within [`CLEAR_RADIUS_TILES`] of
    /// `player_id`'s rig.
    fn clear_enemies_near(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        let Some((x, y)) = self
            .players
            .get(&player_id)
            .map(|player| (player.x, player.y))
        else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let reach = CLEAR_RADIUS_TILES * TILE_SIZE;
        let near: BTreeSet<EntityId> = self
            .entities
            .ids_of(EntityKind::Enemy)
            .chain(self.entities.ids_of(EntityKind::EnemyShot))
            .filter(|id| {
                self.entities.get(*id).is_some_and(|entity| {
                    let (dx, dy) = (entity.body.x - x, entity.body.y - y);
                    dx.mul_add(dx, dy * dy) <= reach * reach
                })
            })
            .collect();
        if !near.is_empty() {
            let _gone = self.entities.despawn_many(&near);
        }
        PlayerScopedCommandOutcome::Applied
    }
}
