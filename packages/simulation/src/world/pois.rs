//! Point-of-interest gameplay: opening crates, salvage cores, vault caches and outpost lockers,
//! vault locks (sequence switches, pressure seals, twin relays), first visits and scanner
//! contacts.
//!
//! A POI's state lives in its tiles (the terrain op log replicates and saves it), plus the
//! twin-relay deadlines in the timers section and each player's journal in their loadout.

use crate::{
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    infrastructure::InfrastructureKind,
    poi::{
        PoiKind, PoiKnowledge, PoiReward, PoiTile, PoiVariant, RELAY_WINDOW_MILLIS, RigFitting,
        VaultPuzzle, crate_reward, in_scanner_range, outpost_reward, salvage_reward,
        scanner_poi_knowledge, scanner_poi_range, vault_reward,
    },
    quantize::quantize_simulation_scalar,
    rig::RigPartKind,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{FeatureId, WorldFeature},
};

use super::WorldState;

/// Margin (tiles) around a POI's bounds that counts as being inside it.
const VISIT_MARGIN: i32 = 1;

impl WorldState {
    /// The point of interest whose bounds contain `position`.
    fn poi_containing(&self, position: TilePosition) -> Option<WorldFeature> {
        if !self.terrain.settings().generator.has_points_of_interest() {
            return None;
        }
        self.terrain.features().poi_at(position).copied()
    }

    /// A player drilled through a POI tile: pays out what it held.
    pub(super) fn open_poi_tile(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: PoiTile,
        output: &mut TickOutput,
    ) {
        let band = self.terrain.band_at(target);
        let seed = self.terrain.seed();
        let feature = self.poi_containing(target);
        let (poi, reward) = match tile {
            PoiTile::SupplyCrate => (PoiKind::SupplyCache, crate_reward(seed, target, band)),
            PoiTile::SalvageCore => {
                let log = match feature.and_then(|feature| feature.poi()) {
                    Some(PoiVariant::DerelictRig { log }) => log,
                    _ => crate::poi::LoreId(u16::try_from(target.x.rem_euclid(12)).unwrap_or(0)),
                };
                (
                    PoiKind::DerelictRig,
                    salvage_reward(
                        seed,
                        target,
                        band,
                        log,
                        &self.progression.rig_part_inventory,
                    ),
                )
            }
            PoiTile::VaultCache => (
                PoiKind::Vault,
                vault_reward(seed, target, band, &self.progression.rig_part_inventory),
            ),
            PoiTile::OutpostSalvage => (PoiKind::RivalClaim, outpost_reward(seed, target, band)),
            _ => return,
        };
        self.grant_poi_reward(player_id, &reward);
        if let Some(feature) = feature
            && let Some(player) = self.players.get_mut(&player_id)
        {
            player.journal.learn(feature.id, PoiKnowledge::Visited);
        }
        output.push_event(GameplayEventKind::PoiOpened {
            player_id,
            poi,
            reward,
        });
    }

    fn grant_poi_reward(&mut self, player_id: PlayerId, reward: &PoiReward) {
        if let Some(part) = reward.part {
            self.progression.rig_part_inventory.insert(part);
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.credits = player.credits.saturating_add(reward.credits);
        if reward.fuel > 0 {
            player.fuel = quantize_simulation_scalar(
                (player.fuel + f32::from(reward.fuel)).min(player.fuel_capacity),
            );
        }
        player.bombs = player.bombs.saturating_add(u32::from(reward.bombs));
        match reward.fitting {
            Some(RigFitting::Bulkhead) => {
                player.crafted_bulkheads = player.crafted_bulkheads.saturating_add(1);
                player.hull =
                    quantize_simulation_scalar((player.hull + 15.0).min(player.max_hull()));
            }
            Some(RigFitting::AuxiliaryTank) => {
                player.fuel_capacity = quantize_simulation_scalar(player.fuel_capacity + 15.0);
                player.fuel =
                    quantize_simulation_scalar((player.fuel + 15.0).min(player.fuel_capacity));
            }
            Some(RigFitting::CargoRack) => {
                player.cargo_capacity = player.cargo_capacity.saturating_add(2);
            }
            None => {}
        }
        if let Some((material, count)) = reward.material {
            player.add_material(material, u32::from(count));
        }
        if let Some(kit) = reward.kit {
            let count = match kit {
                InfrastructureKind::SignalRelay => &mut player.signal_relay_kits,
                InfrastructureKind::SurveyDrone => &mut player.survey_drone_kits,
                InfrastructureKind::CargoLift => &mut player.cargo_lift_kits,
                InfrastructureKind::TunnelSupport => &mut player.tunnel_support_kits,
                InfrastructureKind::PumpStation => &mut player.pump_station_kits,
                InfrastructureKind::OreProcessor => &mut player.ore_processor_kits,
            };
            *count = count.saturating_add(1);
        }
        if let Some(log) = reward.log {
            player.journal.logs.insert(log);
        }
    }

    /// The lock tiles (switches, relays, seals) and doors of a vault, by position.
    fn vault_tiles(&self, vault: &WorldFeature) -> Vec<(TilePosition, PoiTile)> {
        vault
            .bounds
            .positions()
            .filter_map(|position| match self.terrain.tile(position)?.kind {
                TileKind::Poi(tile) => Some((position, tile)),
                _ => None,
            })
            .collect()
    }

    /// Opens a vault: its doors vanish.
    fn open_vault(&mut self, vault: &WorldFeature, output: &mut TickOutput) {
        let doors: Vec<TilePosition> = self
            .vault_tiles(vault)
            .into_iter()
            .filter(|(_, tile)| *tile == PoiTile::VaultDoor)
            .map(|(position, _)| position)
            .collect();
        if doors.is_empty() {
            return;
        }
        for door in doors {
            let _ = self.terrain.set_kind(door, TileKind::Air);
        }
        self.progression.vault_relay_deadlines.remove(&vault.id);
        output.push_event(GameplayEventKind::VaultOpened { feature: vault.id });
    }

    /// Unlights every switch and relay of a vault.
    fn reset_vault_lock(&mut self, vault: &WorldFeature, output: &mut TickOutput) {
        for (position, tile) in self.vault_tiles(vault) {
            let unlit = match tile {
                PoiTile::VaultSwitch { order, lit: true } => {
                    PoiTile::VaultSwitch { order, lit: false }
                }
                PoiTile::VaultRelay { lit: true } => PoiTile::VaultRelay { lit: false },
                _ => continue,
            };
            let _ = self.terrain.set_kind(position, TileKind::Poi(unlit));
        }
        self.progression.vault_relay_deadlines.remove(&vault.id);
        output.push_event(GameplayEventKind::VaultLockReset { feature: vault.id });
    }

    /// A player drilled a vault switch or relay through and it lit up.
    pub(super) fn activate_vault_lock(
        &mut self,
        target: TilePosition,
        lit: PoiTile,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let Some(vault) = self.poi_containing(target) else {
            return;
        };
        let tiles = self.vault_tiles(&vault);
        match lit {
            PoiTile::VaultSwitch { order, .. } => {
                let in_order = tiles.iter().all(|(_, tile)| match tile {
                    PoiTile::VaultSwitch {
                        order: other,
                        lit: other_lit,
                    } => *other >= order || *other_lit,
                    _ => true,
                });
                if !in_order {
                    self.reset_vault_lock(&vault, output);
                    return;
                }
                let all_lit = tiles
                    .iter()
                    .all(|(_, tile)| !matches!(tile, PoiTile::VaultSwitch { lit: false, .. }));
                if all_lit {
                    self.open_vault(&vault, output);
                }
            }
            PoiTile::VaultRelay { .. } => {
                let all_lit = tiles
                    .iter()
                    .all(|(_, tile)| *tile != PoiTile::VaultRelay { lit: false });
                if all_lit {
                    self.open_vault(&vault, output);
                } else {
                    self.progression
                        .vault_relay_deadlines
                        .entry(vault.id)
                        .or_insert_with(|| {
                            SimulationTick::new(
                                next_tick.get().saturating_add(
                                    self.timing.ticks_for_millis(RELAY_WINDOW_MILLIS),
                                ),
                            )
                        });
                }
            }
            _ => {}
        }
    }

    /// After a blast: opens every pressure-seal vault whose seals are now all vented.
    pub(super) fn vent_vault_seals(&mut self, changed: &[TilePosition], output: &mut TickOutput) {
        if changed.is_empty() || !self.terrain.settings().generator.has_points_of_interest() {
            return;
        }
        let vaults: Vec<WorldFeature> = self
            .terrain
            .features()
            .pois()
            .filter(|feature| feature.poi() == Some(PoiVariant::Vault(VaultPuzzle::Seals)))
            .filter(|feature| changed.iter().any(|tile| feature.bounds.contains(*tile)))
            .copied()
            .collect();
        for vault in vaults {
            let tiles = self.vault_tiles(&vault);
            let sealed = tiles.iter().any(|(_, tile)| *tile == PoiTile::VaultSeal);
            if !sealed {
                self.open_vault(&vault, output);
            }
        }
    }

    /// Resets twin-relay locks whose window ran out.
    pub(super) fn advance_vault_relays(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let expired: Vec<FeatureId> = self
            .progression
            .vault_relay_deadlines
            .iter()
            .filter(|(_, deadline)| next_tick >= **deadline)
            .map(|(feature, _)| *feature)
            .collect();
        for feature in expired {
            let vault = self.terrain.features().get(feature).copied();
            match vault {
                Some(vault) => self.reset_vault_lock(&vault, output),
                None => {
                    self.progression.vault_relay_deadlines.remove(&feature);
                }
            }
        }
    }

    /// Marks POIs each rig is inside as visited.
    pub(super) fn track_poi_visits(&mut self, output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_points_of_interest() {
            return;
        }
        let Self {
            players, terrain, ..
        } = self;
        let features = terrain.features();
        for (player_id, player) in players.iter_mut() {
            let tile = player.tile_position(TILE_SIZE);
            for feature in features.pois() {
                if !feature.bounds.expanded(VISIT_MARGIN).contains(tile) {
                    continue;
                }
                if player.journal.knowledge(feature.id) != Some(PoiKnowledge::Visited)
                    && player.journal.learn(feature.id, PoiKnowledge::Visited)
                    && let Some(poi) = feature.poi_kind()
                {
                    output.push_event(GameplayEventKind::PoiDiscovered {
                        player_id: *player_id,
                        feature: feature.id,
                        poi,
                    });
                }
            }
        }
    }

    /// A scanner pulse: records the POIs in the scanner's range as echoes or identified
    /// silhouettes, depending on its tier. Returns how many contacts were new or clearer.
    pub(super) fn ping_pois(&mut self, player_id: PlayerId, center: TilePosition) -> usize {
        if !self.terrain.settings().generator.has_points_of_interest() {
            return 0;
        }
        let relic = self
            .equipped_rig_parts
            .get(&player_id)
            .is_some_and(|parts| parts.contains(&RigPartKind::RelicScanner));
        let Some(player) = self.players.get(&player_id) else {
            return 0;
        };
        let level = player.scanner_level;
        let Some(knowledge) = scanner_poi_knowledge(level, relic) else {
            return 0;
        };
        let range = scanner_poi_range(level, relic);
        let Self {
            players, terrain, ..
        } = self;
        let Some(player) = players.get_mut(&player_id) else {
            return 0;
        };
        terrain
            .features()
            .pois()
            .filter(|feature| in_scanner_range(&feature.bounds, center, range))
            .filter(|feature| player.journal.learn(feature.id, knowledge))
            .count()
    }
}
