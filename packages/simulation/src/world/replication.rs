//! Replication sections of [`WorldState`] (plan §4).
//!
//! Every field of the world belongs to exactly one section kind:
//!
//! | Kind | Contents | API |
//! |---|---|---|
//! | Per-tick | each rig's pose, velocity, fuel, hull, drill and scanner timers, deep-run pressure ([`PlayerTickState`]); live entities: gas clouds, falling boulders, bombs ([`LiveEntities`]); the tick itself | [`WorldState::player_tick_state`], [`WorldState::live_entities`] |
//! | Op log | terrain tiles | [`crate::TickOutput::terrain_ops`], [`WorldState::apply_tile_change`] |
//! | On-change | per player: inventory, credits, upgrades, rig parts, rescue/salvage, discovery ([`PlayerLoadout`]); shared: economy, contracts, progression, town, world events, infrastructure, map markers, RNG, timers, summary, session constants | [`WorldState::section`], [`WorldState::apply_section`] |
//!
//! Terrain dimensions, seed and the initial tiles only travel in a keyframe (the serialized
//! [`WorldState`] itself).
//!
//! Coverage is enforced by the compiler: [`WorldState::section`] and
//! [`WorldState::remove_player`] destructure the whole world, and [`split_player`] destructures
//! the whole [`Player`], without `..`. Adding a field fails to compile until it is assigned to a
//! section here.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    contract::ContractLog,
    economy::{DeepClaimStatus, TownDevelopment},
    hash::StateHasher,
    hazard::{FallingBoulder, HazardCloud, PlacedBomb, ScanMarker},
    ids::{PlayerId, SimulationTick},
    infrastructure::PlacedInfrastructure,
    player::Player,
    progression::{Expedition, LegendaryBlueprint, SideContract, WorldEventKind},
    rig::{DrillState, RigPartKind},
    rng::DeterministicRngState,
    service::PlayerServiceTransaction,
    summary::AuthoritativeWorldSummary,
    terrain::{
        ArtifactKind, MineralKind, StrategicResourceKind, TILE_SIZE, TileChange, TilePosition,
    },
};

use super::{ActiveDrillTimer, DeepRunState, LostCargoSite, TickTiming, WorldState};

/// Number of spawn columns new players are spread over.
const SPAWN_COLUMNS: u64 = 16;
/// Tiles between neighbouring spawn columns.
const SPAWN_COLUMN_SPACING_TILES: f32 = 2.0;

/// Per-tick replicated state of one rig.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PlayerTickState {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
    pub drill: Option<DrillState>,
    pub drill_timer: Option<ActiveDrillTimer>,
    pub scanner_cooldown: Option<f32>,
    pub scanner_deadline: Option<SimulationTick>,
    pub deep_run: Option<DeepRunState>,
}

/// The per-tick parts of a [`Player`].
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerRig {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub fuel: f32,
    pub hull: f32,
}

/// The on-change parts of a [`Player`]: inventory, credits, upgrades and kits.
#[allow(
    clippy::struct_excessive_bools,
    reason = "mirrors the player's independent inventory fields"
)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PlayerInventory {
    pub fuel_capacity: f32,
    pub cargo: BTreeMap<MineralKind, u32>,
    pub artifacts: BTreeMap<ArtifactKind, u32>,
    pub materials: BTreeMap<StrategicResourceKind, u32>,
    pub cargo_capacity: u32,
    pub fuel_tank_level: u8,
    pub cargo_bay_level: u8,
    pub credits: u32,
    pub drill_strength: u8,
    pub engine_level: u8,
    pub hull_level: u8,
    pub radiator_level: u8,
    pub scanner_level: u8,
    pub bombs: u32,
    pub loan_debt: u32,
    pub insured: bool,
    pub insurance_tier: u8,
    pub crafted_bulkheads: u8,
    pub crafted_sorters: u8,
    pub signal_relay_kits: u32,
    pub survey_drone_kits: u32,
    pub cargo_lift_kits: u32,
    pub tunnel_support_kits: u32,
    pub pump_station_kits: u32,
    pub ore_processor_kits: u32,
}

/// Splits a player into its per-tick and on-change parts (exhaustive: every field goes to one).
#[must_use]
pub fn split_player(player: &Player) -> (PlayerRig, PlayerInventory) {
    let Player {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        fuel_capacity,
        hull,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_bulkheads,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
    } = player.clone();
    (
        PlayerRig {
            x,
            y,
            velocity_x,
            velocity_y,
            fuel,
            hull,
        },
        PlayerInventory {
            fuel_capacity,
            cargo,
            artifacts,
            materials,
            cargo_capacity,
            fuel_tank_level,
            cargo_bay_level,
            credits,
            drill_strength,
            engine_level,
            hull_level,
            radiator_level,
            scanner_level,
            bombs,
            loan_debt,
            insured,
            insurance_tier,
            crafted_bulkheads,
            crafted_sorters,
            signal_relay_kits,
            survey_drone_kits,
            cargo_lift_kits,
            tunnel_support_kits,
            pump_station_kits,
            ore_processor_kits,
        },
    )
}

/// Reassembles a player from its two parts.
#[must_use]
pub fn join_player(rig: PlayerRig, inventory: PlayerInventory) -> Player {
    let PlayerRig {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        hull,
    } = rig;
    let PlayerInventory {
        fuel_capacity,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_bulkheads,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
    } = inventory;
    Player {
        x,
        y,
        velocity_x,
        velocity_y,
        fuel,
        fuel_capacity,
        hull,
        cargo,
        artifacts,
        materials,
        cargo_capacity,
        fuel_tank_level,
        cargo_bay_level,
        credits,
        drill_strength,
        engine_level,
        hull_level,
        radiator_level,
        scanner_level,
        bombs,
        loan_debt,
        insured,
        insurance_tier,
        crafted_bulkheads,
        crafted_sorters,
        signal_relay_kits,
        survey_drone_kits,
        cargo_lift_kits,
        tunnel_support_kits,
        pump_station_kits,
        ore_processor_kits,
    }
}

/// Per-tick live entities. Parallel vectors stay index-aligned.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LiveEntities {
    pub hazards: Vec<HazardCloud>,
    pub falling_boulders: Vec<FallingBoulder>,
    pub boulder_warning_deadlines: Vec<SimulationTick>,
    pub boulder_expiry_ticks: Vec<SimulationTick>,
    pub bombs: Vec<PlacedBomb>,
    pub bomb_detonation_ticks: Vec<SimulationTick>,
}

/// One player's on-change section.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlayerLoadout {
    pub inventory: PlayerInventory,
    pub equipped_rig_parts: Option<BTreeSet<RigPartKind>>,
    pub mechanic_level: Option<u8>,
    pub lost_cargo_site: Option<LostCargoSite>,
    pub rescue_count: Option<u32>,
    pub discovered_tiles: Option<BTreeSet<TilePosition>>,
    pub deep_reward_milestone: Option<i32>,
}

/// Market and transaction state.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EconomySection {
    pub town_event_day: u32,
    pub market_salt: u32,
    pub market_history: Vec<u32>,
    pub mineral_market_history: BTreeMap<MineralKind, Vec<u32>>,
    pub total_earnings: u32,
    pub service_transactions: Vec<PlayerServiceTransaction>,
}

/// Story contracts, side contracts and expeditions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractsSection {
    pub contracts: ContractLog,
    pub deep_claim_status: DeepClaimStatus,
    pub active_side_contracts: Vec<SideContract>,
    pub expedition_offers: Vec<Expedition>,
    pub active_expeditions: Vec<Expedition>,
    pub expeditions_completed: u32,
}

/// Shared progression unlocks.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProgressionSection {
    pub legendary_blueprints: BTreeSet<LegendaryBlueprint>,
    pub rig_part_inventory: BTreeSet<RigPartKind>,
    pub deepest_tile_reached: i32,
}

/// Active world events.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorldEventsSection {
    pub severity: BTreeMap<WorldEventKind, u32>,
    pub days_remaining: BTreeMap<WorldEventKind, u32>,
}

/// Scanner markers and pending collapse warnings.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkersSection {
    pub scan_markers: Vec<ScanMarker>,
    pub collapse_warnings: Vec<TilePosition>,
}

/// World-level deadlines.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct TimersSection {
    pub next_day_tick: SimulationTick,
    pub escape_deadline: Option<SimulationTick>,
}

/// Session constants: fixed for the lifetime of a session.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionSection {
    pub timing: TickTiming,
    pub player_template: Player,
}

/// Key of an on-change section.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SectionKey {
    /// One player's inventory, credits, upgrades and per-player progress.
    Loadout(PlayerId),
    Economy,
    Contracts,
    Progression,
    Town,
    WorldEvents,
    Infrastructure,
    Markers,
    Rng,
    Timers,
    /// The derived world summary (its tick always follows the world tick).
    Summary,
    Session,
}

impl SectionKey {
    /// Every shared (not per-player) section.
    pub const SHARED: [Self; 11] = [
        Self::Economy,
        Self::Contracts,
        Self::Progression,
        Self::Town,
        Self::WorldEvents,
        Self::Infrastructure,
        Self::Markers,
        Self::Rng,
        Self::Timers,
        Self::Summary,
        Self::Session,
    ];
}

/// Payload of an on-change section.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SectionPayload {
    Loadout(Box<PlayerLoadout>),
    Economy(Box<EconomySection>),
    Contracts(Box<ContractsSection>),
    Progression(ProgressionSection),
    Town(TownDevelopment),
    WorldEvents(WorldEventsSection),
    Infrastructure(Vec<PlacedInfrastructure>),
    Markers(MarkersSection),
    Rng(DeterministicRngState),
    Timers(TimersSection),
    Summary(AuthoritativeWorldSummary),
    Session(Box<SessionSection>),
}

/// Stable hash of any serializable value (used for section hashes).
#[must_use]
pub fn hash_serialized<T: Serialize + ?Sized>(value: &T) -> u64 {
    let mut hasher = StateHasher::new();
    hasher.write_serialized(value);
    hasher.finish().get()
}

impl WorldState {
    /// Adds a player built from the player template, spread over spawn columns by id. Does
    /// nothing if the player exists.
    pub fn add_player(&mut self, player_id: PlayerId) {
        if self.players.contains_key(&player_id) {
            return;
        }
        let mut player = self.player_template.clone();
        #[allow(
            clippy::cast_precision_loss,
            reason = "small spawn column index becomes a world offset"
        )]
        let column = (player_id.get().saturating_sub(1) % SPAWN_COLUMNS) as f32;
        player.x = (column * SPAWN_COLUMN_SPACING_TILES).mul_add(TILE_SIZE, player.x);
        player.x = super::movement::clamp_authoritative_player_x(&self.terrain, player.x);
        self.insert_player(player_id, player);
    }

    /// Removes a player and every piece of per-player state.
    pub fn remove_player(&mut self, player_id: PlayerId) {
        let Self {
            timing: _,
            player_template: _,
            simulation_tick: _,
            authoritative_summary,
            players,
            equipped_rig_parts,
            mechanic_levels,
            world_event_severity: _,
            world_event_days_remaining: _,
            deep_run_states,
            lost_cargo_sites,
            rescue_counts,
            progression,
            scan_markers: _,
            collapse_warnings: _,
            hazards: _,
            falling_boulders: _,
            boulder_warning_deadlines: _,
            boulder_expiry_ticks: _,
            terrain: _,
            bombs: _,
            infrastructure: _,
            active_drills,
            active_drill_timers,
            scanner_cooldowns,
            scanner_cooldown_deadlines,
            bomb_detonation_ticks: _,
            rng: _,
            discovered_tiles,
            service_transactions: _,
        } = self;
        players.remove(&player_id);
        equipped_rig_parts.remove(&player_id);
        mechanic_levels.remove(&player_id);
        deep_run_states.remove(&player_id);
        lost_cargo_sites.remove(&player_id);
        rescue_counts.remove(&player_id);
        progression.deep_reward_milestones.remove(&player_id);
        active_drills.remove(&player_id);
        active_drill_timers.remove(&player_id);
        scanner_cooldowns.remove(&player_id);
        scanner_cooldown_deadlines.remove(&player_id);
        discovered_tiles.remove(&player_id);
        authoritative_summary.player_count = players.len();
    }

    /// Per-tick replicated state of one rig.
    #[must_use]
    pub fn player_tick_state(&self, player_id: PlayerId) -> Option<PlayerTickState> {
        let player = self.players.get(&player_id)?;
        let (rig, _) = split_player(player);
        Some(PlayerTickState {
            x: rig.x,
            y: rig.y,
            velocity_x: rig.velocity_x,
            velocity_y: rig.velocity_y,
            fuel: rig.fuel,
            hull: rig.hull,
            drill: self.active_drills.get(&player_id).copied(),
            drill_timer: self.active_drill_timers.get(&player_id).copied(),
            scanner_cooldown: self.scanner_cooldowns.get(&player_id).copied(),
            scanner_deadline: self.scanner_cooldown_deadlines.get(&player_id).copied(),
            deep_run: self.deep_run_states.get(&player_id).copied(),
        })
    }

    /// Overwrites one rig's per-tick state (adding the player from the template if needed).
    pub fn apply_player_tick_state(&mut self, player_id: PlayerId, state: PlayerTickState) {
        self.add_player(player_id);
        let PlayerTickState {
            x,
            y,
            velocity_x,
            velocity_y,
            fuel,
            hull,
            drill,
            drill_timer,
            scanner_cooldown,
            scanner_deadline,
            deep_run,
        } = state;
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = x;
            player.y = y;
            player.velocity_x = velocity_x;
            player.velocity_y = velocity_y;
            player.fuel = fuel;
            player.hull = hull;
        }
        set_or_remove(&mut self.active_drills, player_id, drill);
        set_or_remove(&mut self.active_drill_timers, player_id, drill_timer);
        set_or_remove(&mut self.scanner_cooldowns, player_id, scanner_cooldown);
        set_or_remove(
            &mut self.scanner_cooldown_deadlines,
            player_id,
            scanner_deadline,
        );
        set_or_remove(&mut self.deep_run_states, player_id, deep_run);
    }

    /// All live entities.
    #[must_use]
    pub fn live_entities(&self) -> LiveEntities {
        self.live_entities_where(|_, _| true)
    }

    /// Live entities whose world position satisfies `relevant(x, y)` (interest management).
    #[must_use]
    pub fn live_entities_where(&self, relevant: impl Fn(f32, f32) -> bool) -> LiveEntities {
        let mut entities = LiveEntities {
            hazards: self
                .hazards
                .iter()
                .filter(|cloud| relevant(cloud.x, cloud.y))
                .copied()
                .collect(),
            ..LiveEntities::default()
        };
        for (index, boulder) in self.falling_boulders.iter().enumerate() {
            if relevant(boulder.x, boulder.y) {
                entities.falling_boulders.push(*boulder);
                entities
                    .boulder_warning_deadlines
                    .extend(self.boulder_warning_deadlines.get(index).copied());
                entities
                    .boulder_expiry_ticks
                    .extend(self.boulder_expiry_ticks.get(index).copied());
            }
        }
        for (index, bomb) in self.bombs.iter().enumerate() {
            if relevant(bomb.x, bomb.y) {
                entities.bombs.push(*bomb);
                entities
                    .bomb_detonation_ticks
                    .extend(self.bomb_detonation_ticks.get(index).copied());
            }
        }
        entities
    }

    /// Replaces the live entities with a replicated copy.
    pub fn apply_live_entities(&mut self, entities: LiveEntities) {
        let LiveEntities {
            hazards,
            falling_boulders,
            boulder_warning_deadlines,
            boulder_expiry_ticks,
            bombs,
            bomb_detonation_ticks,
        } = entities;
        self.hazards = hazards;
        self.falling_boulders = falling_boulders;
        self.boulder_warning_deadlines = boulder_warning_deadlines;
        self.boulder_expiry_ticks = boulder_expiry_ticks;
        self.bombs = bombs;
        self.bomb_detonation_ticks = bomb_detonation_ticks;
    }

    /// Applies one op-log entry.
    pub fn apply_tile_change(&mut self, change: &TileChange) -> bool {
        self.terrain.apply_tile_change(change)
    }

    /// Keys of every on-change section that currently exists.
    #[must_use]
    pub fn section_keys(&self) -> Vec<SectionKey> {
        self.players
            .keys()
            .map(|player_id| SectionKey::Loadout(*player_id))
            .chain(SectionKey::SHARED)
            .collect()
    }

    /// Copy of one on-change section.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "the exhaustive destructure is the section coverage table"
    )]
    pub fn section(&self, key: SectionKey) -> Option<SectionPayload> {
        // Coverage table: every field is named here. `_` fields are replicated per tick or
        // through the op log (see the module docs).
        let Self {
            timing,
            player_template,
            simulation_tick: _,
            authoritative_summary,
            players,
            equipped_rig_parts,
            mechanic_levels,
            world_event_severity,
            world_event_days_remaining,
            deep_run_states: _,
            lost_cargo_sites,
            rescue_counts,
            progression,
            scan_markers,
            collapse_warnings,
            hazards: _,
            falling_boulders: _,
            boulder_warning_deadlines: _,
            boulder_expiry_ticks: _,
            terrain: _,
            bombs: _,
            infrastructure,
            active_drills: _,
            active_drill_timers: _,
            scanner_cooldowns: _,
            scanner_cooldown_deadlines: _,
            bomb_detonation_ticks: _,
            rng,
            discovered_tiles,
            service_transactions,
        } = self;
        let super::CanonicalProgressionState {
            contracts,
            deep_claim_status,
            escape_deadline,
            deep_reward_milestones,
            town_development,
            active_side_contracts,
            expedition_offers,
            active_expeditions,
            legendary_blueprints,
            rig_part_inventory,
            town_event_day,
            market_salt,
            market_history,
            mineral_market_history,
            deepest_tile_reached,
            total_earnings,
            expeditions_completed,
            next_day_tick,
        } = progression;
        Some(match key {
            SectionKey::Loadout(player_id) => {
                let (_, inventory) = split_player(players.get(&player_id)?);
                SectionPayload::Loadout(Box::new(PlayerLoadout {
                    inventory,
                    equipped_rig_parts: equipped_rig_parts.get(&player_id).cloned(),
                    mechanic_level: mechanic_levels.get(&player_id).copied(),
                    lost_cargo_site: lost_cargo_sites.get(&player_id).cloned(),
                    rescue_count: rescue_counts.get(&player_id).copied(),
                    discovered_tiles: discovered_tiles.get(&player_id).cloned(),
                    deep_reward_milestone: deep_reward_milestones.get(&player_id).copied(),
                }))
            }
            SectionKey::Economy => SectionPayload::Economy(Box::new(EconomySection {
                town_event_day: *town_event_day,
                market_salt: *market_salt,
                market_history: market_history.clone(),
                mineral_market_history: mineral_market_history.clone(),
                total_earnings: *total_earnings,
                service_transactions: service_transactions.clone(),
            })),
            SectionKey::Contracts => SectionPayload::Contracts(Box::new(ContractsSection {
                contracts: contracts.clone(),
                deep_claim_status: *deep_claim_status,
                active_side_contracts: active_side_contracts.clone(),
                expedition_offers: expedition_offers.clone(),
                active_expeditions: active_expeditions.clone(),
                expeditions_completed: *expeditions_completed,
            })),
            SectionKey::Progression => SectionPayload::Progression(ProgressionSection {
                legendary_blueprints: legendary_blueprints.clone(),
                rig_part_inventory: rig_part_inventory.clone(),
                deepest_tile_reached: *deepest_tile_reached,
            }),
            SectionKey::Town => SectionPayload::Town(town_development.clone()),
            SectionKey::WorldEvents => SectionPayload::WorldEvents(WorldEventsSection {
                severity: world_event_severity.clone(),
                days_remaining: world_event_days_remaining.clone(),
            }),
            SectionKey::Infrastructure => SectionPayload::Infrastructure(infrastructure.clone()),
            SectionKey::Markers => SectionPayload::Markers(MarkersSection {
                scan_markers: scan_markers.clone(),
                collapse_warnings: collapse_warnings.clone(),
            }),
            SectionKey::Rng => SectionPayload::Rng(*rng),
            SectionKey::Timers => SectionPayload::Timers(TimersSection {
                next_day_tick: *next_day_tick,
                escape_deadline: *escape_deadline,
            }),
            SectionKey::Summary => {
                // The summary's tick mirrors the world tick, which is replicated per tick.
                let mut summary = authoritative_summary.clone();
                summary.tick = SimulationTick::default();
                SectionPayload::Summary(summary)
            }
            SectionKey::Session => SectionPayload::Session(Box::new(SessionSection {
                timing: *timing,
                player_template: player_template.clone(),
            })),
        })
    }

    /// Stable hash of one on-change section.
    #[must_use]
    pub fn section_hash(&self, key: SectionKey) -> Option<u64> {
        self.section(key).map(|section| hash_serialized(&section))
    }

    /// Replaces one on-change section with a replicated copy. A payload that does not match
    /// the key is ignored.
    #[allow(
        clippy::too_many_lines,
        reason = "applies every section kind explicitly"
    )]
    pub fn apply_section(&mut self, key: SectionKey, payload: SectionPayload) {
        match (key, payload) {
            (SectionKey::Loadout(player_id), SectionPayload::Loadout(loadout)) => {
                self.add_player(player_id);
                let PlayerLoadout {
                    inventory,
                    equipped_rig_parts,
                    mechanic_level,
                    lost_cargo_site,
                    rescue_count,
                    discovered_tiles,
                    deep_reward_milestone,
                } = *loadout;
                if let Some(player) = self.players.get_mut(&player_id) {
                    let (rig, _) = split_player(player);
                    *player = join_player(rig, inventory);
                }
                set_or_remove(&mut self.equipped_rig_parts, player_id, equipped_rig_parts);
                set_or_remove(&mut self.mechanic_levels, player_id, mechanic_level);
                set_or_remove(&mut self.lost_cargo_sites, player_id, lost_cargo_site);
                set_or_remove(&mut self.rescue_counts, player_id, rescue_count);
                set_or_remove(&mut self.discovered_tiles, player_id, discovered_tiles);
                set_or_remove(
                    &mut self.progression.deep_reward_milestones,
                    player_id,
                    deep_reward_milestone,
                );
            }
            (SectionKey::Economy, SectionPayload::Economy(economy)) => {
                let EconomySection {
                    town_event_day,
                    market_salt,
                    market_history,
                    mineral_market_history,
                    total_earnings,
                    service_transactions,
                } = *economy;
                self.progression.town_event_day = town_event_day;
                self.progression.market_salt = market_salt;
                self.progression.market_history = market_history;
                self.progression.mineral_market_history = mineral_market_history;
                self.progression.total_earnings = total_earnings;
                self.service_transactions = service_transactions;
            }
            (SectionKey::Contracts, SectionPayload::Contracts(contracts)) => {
                let ContractsSection {
                    contracts,
                    deep_claim_status,
                    active_side_contracts,
                    expedition_offers,
                    active_expeditions,
                    expeditions_completed,
                } = *contracts;
                self.progression.contracts = contracts;
                self.progression.deep_claim_status = deep_claim_status;
                self.progression.active_side_contracts = active_side_contracts;
                self.progression.expedition_offers = expedition_offers;
                self.progression.active_expeditions = active_expeditions;
                self.progression.expeditions_completed = expeditions_completed;
            }
            (SectionKey::Progression, SectionPayload::Progression(progression)) => {
                let ProgressionSection {
                    legendary_blueprints,
                    rig_part_inventory,
                    deepest_tile_reached,
                } = progression;
                self.progression.legendary_blueprints = legendary_blueprints;
                self.progression.rig_part_inventory = rig_part_inventory;
                self.progression.deepest_tile_reached = deepest_tile_reached;
            }
            (SectionKey::Town, SectionPayload::Town(town)) => {
                self.progression.town_development = town;
            }
            (SectionKey::WorldEvents, SectionPayload::WorldEvents(events)) => {
                let WorldEventsSection {
                    severity,
                    days_remaining,
                } = events;
                self.world_event_severity = severity;
                self.world_event_days_remaining = days_remaining;
            }
            (SectionKey::Infrastructure, SectionPayload::Infrastructure(infrastructure)) => {
                self.infrastructure = infrastructure;
            }
            (SectionKey::Markers, SectionPayload::Markers(markers)) => {
                let MarkersSection {
                    scan_markers,
                    collapse_warnings,
                } = markers;
                self.scan_markers = scan_markers;
                self.collapse_warnings = collapse_warnings;
            }
            (SectionKey::Rng, SectionPayload::Rng(rng)) => self.rng = rng,
            (SectionKey::Timers, SectionPayload::Timers(timers)) => {
                let TimersSection {
                    next_day_tick,
                    escape_deadline,
                } = timers;
                self.progression.next_day_tick = next_day_tick;
                self.progression.escape_deadline = escape_deadline;
            }
            (SectionKey::Summary, SectionPayload::Summary(mut summary)) => {
                summary.tick = self.simulation_tick;
                self.authoritative_summary = summary;
            }
            (SectionKey::Session, SectionPayload::Session(session)) => {
                let SessionSection {
                    timing,
                    player_template,
                } = *session;
                self.timing = timing;
                self.player_template = player_template;
            }
            _ => {}
        }
    }

    /// Removes a section that no longer exists on the authority (a departed player's loadout).
    pub fn remove_section(&mut self, key: SectionKey) {
        if let SectionKey::Loadout(player_id) = key {
            self.remove_player(player_id);
        }
    }
}

fn set_or_remove<V>(map: &mut BTreeMap<PlayerId, V>, player_id: PlayerId, value: Option<V>) {
    match value {
        Some(value) => {
            map.insert(player_id, value);
        }
        None => {
            map.remove(&player_id);
        }
    }
}
