//! The Moon in the world (expansion plan WP-MOON, see [`crate::moon`] and
//! [`crate::worldgen::moon`]).
//!
//! * **Vacuum** ([`WorldState::in_vacuum`]): orbit and the Moon have no air. A rig without a
//!   Pressurized Hull buckles and one without Life Support struggles
//!   ([`crate::moon::vacuum_damage_per_second`], in the hazard pass);
//!   [`GameplayEventKind::MoonVacuumBreach`] fires as it first meets vacuum, so the HUD can say
//!   what is missing. Inside the colony's pressurized rooms there is air.
//! * **The landing** ([`GameplayEventKind::MoonLanded`]): the first time a rig rests on the
//!   Moon's top side (recorded in [`crate::moon::LunarProgress::landed`]); the game plays the
//!   sunrise set piece.
//! * **Breathing on foot** ([`WorldState::lunar_air`]): a pilot breathes in the colony's
//!   pressurized rooms and within its rig's life-support air line
//!   ([`crate::moon::life_support_reach_tiles`]); anywhere else in vacuum the suit's oxygen
//!   runs down.
//! * **The colony** ([`crate::moon::MoonCommand::Colony`]): its counter works for a rig or a
//!   pilot inside the colony's area ([`WorldState::in_colony`]); enemies leave the colony alone.
//! * **The facility**: the hand pick lights the puzzle hall's pylons (in the order their glyph
//!   marks show; a wrong one puts them all out) and the gate opens when all three glow; the
//!   Warden wakes when someone enters its arena, its two conduits knock its shield down when the
//!   pick breaks them, and when it falls the core seal opens, its killer's crew in the facility
//!   receive the Void Lance and the way down to the core is clear. On Peaceful the Warden never
//!   wakes and the seal opens for whoever reaches the arena.
//! * **The core** ([`GameplayEventKind::MoonCoreReached`]): a pilot who reaches the strange
//!   core (the story's endgame hook, recorded in [`crate::moon::LunarProgress::core_reached`]).
//! * **Lunar enemies**: crawlers, mites and automatons roam the Moon (never the colony) around
//!   rigs and pilots, on the director's schedule.
//! * **The uplink** ([`crate::base::ModuleKind::LunarUplink`]): see `world/bases.rs`.
//!
//! The facility's state lives in its terrain (lit pylons, the opened gate and seal, broken
//! conduits are ordinary tile writes), so it saves and replicates with no new section.

use crate::{
    TickOutput,
    enemy::{Damage, DamageKind, DamageSource, EnemyKind, LUNAR_TIER, Locomotion, lunar_roster},
    entity::{EntityKind, EntityState},
    event::GameplayEventKind,
    ids::{PlayerId, SimulationTick},
    moon::{
        COLONY_FUEL_UNIT_COST, COLONY_REPAIR_UNIT_COST, ColonyOffer, LunarTile, MOON_ICE_FUEL,
        MoonCommand, colony_weapon, haul_value, life_support_reach_tiles, vacuum_breach,
        vacuum_damage_per_second,
    },
    summary::PlayerScopedCommandOutcome,
    terrain::{StrategicResourceKind, TILE_SIZE, TileKind, TilePosition},
    weapon::WeaponKind,
    worldgen::{
        moon::{MoonLayout, layout},
        noise::hash2,
    },
};

use super::WorldState;

const SALT_LUNAR_SPAWN: u64 = 0x300B_0001;
/// How close (tiles) a pilot must come to the core's middle to reach it.
pub const CORE_REACH_TILES: i32 = 4;
/// Lunar enemies around one actor at most.
const LUNAR_LOCAL_CAP: usize = 5;
/// Spawn chance per actor per director check, per mille.
const LUNAR_SPAWN_PER_MILLE: u64 = 260;

#[allow(
    clippy::cast_possible_truncation,
    reason = "bounded world pixels address terrain tiles"
)]
fn tile_of(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

#[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
fn centre(tile: TilePosition) -> (f32, f32) {
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}

impl WorldState {
    // --- Queries ----------------------------------------------------------------------------

    /// The layout of this world's filled Moon (V9 worlds; `None` elsewhere).
    #[must_use]
    pub fn moon_layout(&self) -> Option<MoonLayout> {
        layout(self.terrain.settings())
    }

    /// Whether world pixel `(x, y)` is in vacuum (orbit and the Moon; never inside the
    /// colony's pressurized rooms).
    #[must_use]
    pub fn in_vacuum(&self, x: f32, y: f32) -> bool {
        crate::moon::vacuum_at(self.terrain.settings(), x, y)
    }

    /// Whether `tile` is inside the lunar colony's area (its counter works and enemies stay
    /// away).
    #[must_use]
    pub fn in_colony(&self, tile: TilePosition) -> bool {
        self.moon_layout()
            .is_some_and(|layout| layout.colony_area().contains(tile))
    }

    /// Whether `tile` is on or in the Moon (its footprint, from the space above its top down
    /// to its underside).
    #[must_use]
    pub fn on_moon(&self, tile: TilePosition) -> bool {
        self.moon_layout()
            .is_some_and(|layout| layout.moon.bounds().expanded(24).contains(tile))
    }

    /// Whether `player_id` is acting from inside the colony (its rig, or its pilot on foot).
    #[must_use]
    pub fn player_in_colony(&self, player_id: PlayerId) -> bool {
        self.actor_tile(player_id)
            .is_some_and(|tile| self.in_colony(tile))
    }

    /// Whether the use button opens the colony's counter for `player_id`: its pilot anywhere in
    /// the colony, or its rig over the landing pad. Elsewhere in the colony's (wide) area the
    /// rig's use button stays free to let the pilot out (to walk into the dome).
    #[must_use]
    pub fn at_colony_counter(&self, player_id: PlayerId) -> bool {
        if !self.player_in_colony(player_id) {
            return false;
        }
        if self.is_on_foot(player_id) {
            return true;
        }
        let (Some(layout), Some(player)) = (self.moon_layout(), self.players.get(&player_id))
        else {
            return false;
        };
        let (first, last, _) = layout.pad();
        (first..=last).contains(&tile_of(player.x, player.y).x)
    }

    /// Whether a pilot of `owner` at world pixel `(x, y)` has air on the Moon (or anywhere the
    /// air is thin): in the colony's pressurized rooms, behind the alien facility's veil, or
    /// within its rig's life-support air line.
    #[must_use]
    pub fn lunar_air(&self, owner: PlayerId, x: f32, y: f32) -> bool {
        let tile = tile_of(x, y);
        if self
            .moon_layout()
            .is_some_and(|layout| layout.pressurized(tile.x, tile.y))
        {
            return true;
        }
        let Some(player) = self.players.get(&owner) else {
            return false;
        };
        let reach = life_support_reach_tiles(player.hazard_gear.life_support) * TILE_SIZE;
        player.hull > 0.0 && reach > 0.0 && (player.x - x).hypot(player.y - y) <= reach
    }

    /// Whether the Warden is awake (alive) somewhere.
    #[must_use]
    pub fn warden_awake(&self) -> bool {
        self.enemies()
            .any(|(_, state)| state.kind == EnemyKind::Warden)
    }

    /// Whether the core seal still stands (the Warden has not fallen yet).
    #[must_use]
    pub fn core_sealed(&self) -> bool {
        self.moon_layout().is_some_and(|layout| {
            layout.facility.seal.positions().any(|tile| {
                self.terrain
                    .tile(tile)
                    .is_some_and(|found| found.kind == TileKind::Lunar(LunarTile::CoreSeal))
            })
        })
    }

    // --- The hazard pass and edges ----------------------------------------------------------

    /// Hull damage `player_id`'s rig takes this tick from vacuum (0 with the gear, outside
    /// vacuum, or in the colony).
    pub(super) fn vacuum_damage(&self, player_id: PlayerId, dt: f32) -> f32 {
        let Some(player) = self.players.get(&player_id) else {
            return 0.0;
        };
        if player.hull <= 0.0 || !self.in_vacuum(player.x, player.y) {
            return 0.0;
        }
        vacuum_damage_per_second(player) * dt
    }

    /// The Moon's edges for one rig's move from `before`: the vacuum breach warning and the
    /// first landing.
    pub(super) fn push_moon_events(
        &mut self,
        player_id: PlayerId,
        before: (f32, f32),
        output: &mut TickOutput,
    ) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let (x, y, vy) = (player.x, player.y, player.velocity_y);
        if !self.terrain.settings().generator.has_sky() || y > 0.0 {
            return;
        }
        if let Some(breach) = vacuum_breach(player)
            && self.in_vacuum(x, y)
            && !self.in_vacuum(before.0, before.1)
        {
            output.push_event(GameplayEventKind::MoonVacuumBreach { player_id, breach });
        }
        if player.lunar.landed || vy.abs() > 1.0 {
            return;
        }
        let Some(layout) = self.moon_layout() else {
            return;
        };
        let tile = tile_of(x, y);
        let Some(surface) = layout.surface_row(tile.x) else {
            return;
        };
        let below = tile_of(
            x,
            TILE_SIZE.mul_add(0.25, y + super::movement::AUTHORITATIVE_PLAYER_RADIUS),
        );
        // Resting on the top side (the ground, the pad or a tree), not in a tube or below.
        let resting = self.terrain.is_solid_at(below)
            && matches!(
                self.terrain.tile(below).map(|t| t.kind),
                Some(TileKind::Lunar(_))
            );
        if resting && tile.y <= surface && tile.y >= surface - 30 {
            if let Some(player) = self.players.get_mut(&player_id) {
                player.lunar.landed = true;
            }
            output.push_event(GameplayEventKind::MoonLanded { player_id });
        }
    }

    // --- The colony -------------------------------------------------------------------------

    /// [`crate::PlayerCommand::Moon`].
    pub(super) fn apply_moon_command(
        &mut self,
        player_id: PlayerId,
        command: MoonCommand,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        match command {
            MoonCommand::Colony(offer) => {
                let outcome = self.apply_colony_offer(player_id, offer);
                if outcome == PlayerScopedCommandOutcome::Applied {
                    output.push_event(GameplayEventKind::ColonyServiceUsed { player_id, offer });
                }
                outcome
            }
        }
    }

    /// Whether `player_id` may take colony offer `offer` now (inside the colony, the offer
    /// available and affordable). The shell asks before sending; the tick asks again.
    #[must_use]
    pub fn colony_offer_ready(&self, player_id: PlayerId, offer: ColonyOffer) -> bool {
        if !self.player_in_colony(player_id) || !self.rig_within_service_reach(player_id) {
            return false;
        }
        let Some(player) = self.players.get(&player_id) else {
            return false;
        };
        let Some(price) = offer.price(player) else {
            return false;
        };
        match offer {
            ColonyOffer::Refuel => player.credits >= COLONY_FUEL_UNIT_COST,
            ColonyOffer::Repair => player.credits >= COLONY_REPAIR_UNIT_COST,
            _ => price.affordable(player),
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "fuel and hull amounts are small and non-negative"
    )]
    fn apply_colony_offer(
        &mut self,
        player_id: PlayerId,
        offer: ColonyOffer,
    ) -> PlayerScopedCommandOutcome {
        if !self.colony_offer_ready(player_id, offer) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let Some(price) = offer.price(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        match offer {
            ColonyOffer::Refuel => {
                let missing = (player.fuel_capacity - player.fuel).floor().max(0.0) as u32;
                let units = missing.min(player.credits / COLONY_FUEL_UNIT_COST);
                player.credits -= units * COLONY_FUEL_UNIT_COST;
                player.fuel = (player.fuel + units as f32).min(player.fuel_capacity);
            }
            ColonyOffer::Repair => {
                let missing = (player.max_hull() - player.hull).floor().max(0.0) as u32;
                let units = missing.min(player.credits / COLONY_REPAIR_UNIT_COST);
                player.credits -= units * COLONY_REPAIR_UNIT_COST;
                player.hull = (player.hull + units as f32).min(player.max_hull());
            }
            ColonyOffer::MeltIce => {
                let ice = player.cargo_bay.material(StrategicResourceKind::MoonIce);
                let room = (player.fuel_capacity - player.fuel).max(0.0);
                let melted = ice.min((room / MOON_ICE_FUEL).ceil() as u32);
                player.cargo_bay.remove(
                    crate::item::ItemKind::Material(StrategicResourceKind::MoonIce),
                    melted,
                );
                player.fuel = (melted as f32)
                    .mul_add(MOON_ICE_FUEL, player.fuel)
                    .min(player.fuel_capacity);
            }
            ColonyOffer::SellHaul => {
                let tank_full = player.fuel >= player.fuel_capacity;
                let value = haul_value(&player.cargo_bay, tank_full);
                player.cargo_bay.update_counts(|kind, count| match kind {
                    crate::item::ItemKind::Material(StrategicResourceKind::Helium3) => 0,
                    crate::item::ItemKind::Material(StrategicResourceKind::MoonIce)
                        if tank_full =>
                    {
                        0
                    }
                    crate::item::ItemKind::Artifact(artifact) if artifact.is_lunar() => 0,
                    _ => count,
                });
                player.credits = player.credits.saturating_add(value);
                self.progression.total_earnings =
                    self.progression.total_earnings.saturating_add(value);
            }
            ColonyOffer::Reactor | ColonyOffer::AlloyDrill => {
                pay(player, price);
                if offer == ColonyOffer::Reactor {
                    player.lunar.reactor = true;
                } else {
                    player.lunar.alloy_drill = true;
                }
            }
            ColonyOffer::BuyWeapon(weapon) => {
                if !colony_weapon(weapon) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                pay(player, price);
                let _ = player.receive_weapon(weapon);
            }
            ColonyOffer::UpgradeWeapon(weapon) => {
                let Some(mk) = player.weapon_rack.mk(weapon) else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                pay(player, price);
                player.weapon_rack.owned.insert(weapon, mk + 1);
                if let Some(mounted) = player.mounted_weapon.as_mut()
                    && mounted.kind == weapon
                {
                    mounted.mk = mk + 1;
                }
            }
        }
        PlayerScopedCommandOutcome::Applied
    }

    // --- The facility -----------------------------------------------------------------------

    /// The hand pick struck a facility piece at `target` (a pylon or a conduit). Returns
    /// whether it was one (the pick does nothing else to it).
    pub(super) fn pick_facility(
        &mut self,
        owner: PlayerId,
        target: TilePosition,
        kind: TileKind,
        output: &mut TickOutput,
    ) -> bool {
        let Some(layout) = self.moon_layout() else {
            return false;
        };
        let plan = layout.facility;
        match kind {
            TileKind::Lunar(LunarTile::Pylon) => {
                let Some(index) = plan.pylons.iter().position(|pylon| *pylon == target) else {
                    return true;
                };
                let lit: Vec<usize> = (0..3)
                    .filter(|i| {
                        self.terrain.tile(plan.pylons[*i]).map(|t| t.kind)
                            == Some(TileKind::Lunar(LunarTile::PylonLit))
                    })
                    .collect();
                let next = u8::try_from(lit.len()).unwrap_or(0);
                if plan.order[index] == next {
                    let _ = self
                        .terrain
                        .set_kind_by_rule(target, TileKind::Lunar(LunarTile::PylonLit));
                    output.push_event(GameplayEventKind::FacilityPylon {
                        player_id: owner,
                        lit: next + 1,
                        reset: false,
                    });
                    if next + 1 == 3 {
                        for tile in plan.gate.positions() {
                            let _ = self.terrain.set_kind_by_rule(tile, TileKind::Air);
                        }
                        output
                            .push_event(GameplayEventKind::FacilityGateOpened { player_id: owner });
                    }
                } else {
                    for i in lit {
                        let _ = self
                            .terrain
                            .set_kind_by_rule(plan.pylons[i], TileKind::Lunar(LunarTile::Pylon));
                    }
                    output.push_event(GameplayEventKind::FacilityPylon {
                        player_id: owner,
                        lit: 0,
                        reset: true,
                    });
                }
                true
            }
            TileKind::Lunar(LunarTile::PylonLit) => true,
            TileKind::Lunar(LunarTile::Conduit) => {
                // The conduit bursts: its surge knocks the Warden's shield down.
                let _ = self
                    .terrain
                    .set_kind_by_rule(target, TileKind::Lunar(LunarTile::FacilityWall));
                let wardens: Vec<_> = self
                    .enemies()
                    .filter(|(_, state)| state.kind == EnemyKind::Warden)
                    .map(|(entity, _)| entity.id)
                    .collect();
                for id in wardens {
                    let _ = self.damage_enemy(
                        id,
                        Damage::new(40.0, DamageKind::Emp, DamageSource::Player(owner)),
                        output,
                    );
                    output.push_event(GameplayEventKind::WardenStaggered { entity: id.get() });
                }
                true
            }
            _ => false,
        }
    }

    /// Strikes the facility piece at `target` as `player_id`'s hand pick would (a pylon or a
    /// conduit; scenes and scripted set pieces). Returns the events it raised.
    pub fn strike_facility(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
    ) -> Vec<GameplayEventKind> {
        let mut output = TickOutput::default();
        if let Some(kind) = self.terrain.tile(target).map(|tile| tile.kind) {
            let _ = self.pick_facility(player_id, target, kind, &mut output);
        }
        output.events.into_iter().map(|event| event.kind).collect()
    }

    /// The Warden fell: the seal opens, its crew in the facility get the Void Lance.
    pub(super) fn warden_fell(&mut self, by: Option<PlayerId>, output: &mut TickOutput) {
        let Some(layout) = self.moon_layout() else {
            return;
        };
        let plan = layout.facility;
        for tile in plan.seal.positions() {
            let _ = self.terrain.set_kind_by_rule(tile, TileKind::Air);
        }
        let area = plan.bounds().expanded(8);
        let crew: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|id| {
                Some(*id) == by || self.actor_tile(*id).is_some_and(|tile| area.contains(tile))
            })
            .collect();
        for id in crew {
            if let Some(player) = self.players.get_mut(&id) {
                player.lunar.warden_defeated = true;
                let _ = player.receive_weapon(WeaponKind::VoidLance);
            }
        }
        output.push_event(GameplayEventKind::WardenDefeated { by });
    }

    /// The Moon's per-tick pass: the Warden wakes (or, on Peaceful, the seal opens) when
    /// someone enters the arena, and pilots reach the core.
    pub(super) fn advance_moon(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.moon_layout() else {
            return;
        };
        let plan = layout.facility;
        let actors: Vec<(PlayerId, TilePosition, bool)> = self
            .players
            .keys()
            .filter_map(|id| {
                self.actor_tile(*id)
                    .map(|tile| (*id, tile, self.is_on_foot(*id)))
            })
            .collect();
        let in_arena = actors
            .iter()
            .find(|(_, tile, _)| plan.arena.contains(*tile))
            .map(|(id, ..)| *id);
        if let Some(first) = in_arena
            && self.core_sealed()
            && !self.warden_awake()
        {
            if self.enemies.difficulty.hostile() {
                let (x, y) = centre(plan.arena_center());
                let id = self.spawn_enemy_with(EnemyKind::Warden, (x, y), 0, None, None);
                // Its shield runs on the conduits still standing.
                let standing = plan
                    .conduits
                    .iter()
                    .filter(|conduit| {
                        self.terrain.tile(**conduit).map(|t| t.kind)
                            == Some(TileKind::Lunar(LunarTile::Conduit))
                    })
                    .count();
                if standing == 0 {
                    self.entities.update(id, |entity| {
                        if let EntityState::Enemy(state) = &mut entity.state {
                            state.shield = 0.0;
                        }
                    });
                }
                output.push_event(GameplayEventKind::WardenAwakened { entity: id.get() });
            } else {
                self.warden_fell(Some(first), output);
            }
        }
        for (player_id, tile, on_foot) in actors {
            if !on_foot
                || (tile.x - layout.core.x).abs() > CORE_REACH_TILES
                || (tile.y - layout.core.y).abs() > CORE_REACH_TILES
            {
                continue;
            }
            if let Some(player) = self.players.get_mut(&player_id)
                && !player.lunar.core_reached
            {
                player.lunar.core_reached = true;
                output.push_event(GameplayEventKind::MoonCoreReached { player_id });
            }
        }
    }

    // --- The uplink -------------------------------------------------------------------------

    /// Where uplink `from` beams `player_id`'s rig (world pixels of the rig's centre): from the
    /// Moon to the first uplink below that the player may use (else the town); from anywhere
    /// else to the first lunar uplink the player may use (`None` if there is none).
    #[must_use]
    pub fn uplink_destination(
        &self,
        player_id: PlayerId,
        from: crate::base::StructureId,
    ) -> Option<(f32, f32)> {
        let source = self.bases.module(from)?;
        let up_there = self.on_moon(source.anchor());
        let target = self
            .bases
            .modules()
            .filter(|module| {
                module.id != from
                    && module.kind == crate::base::ModuleKind::LunarUplink
                    && self.on_moon(module.anchor()) != up_there
                    && self
                        .bases
                        .claim(module.claim)
                        .is_some_and(|claim| claim.access.may_use(claim.owner, player_id))
            })
            .min_by_key(|module| (module.owner != player_id, module.id));
        match target {
            Some(module) => {
                let (width, _) = module.kind.footprint();
                let anchor = module.anchor();
                #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
                Some((
                    (width as f32).mul_add(0.5, anchor.x as f32) * TILE_SIZE,
                    (anchor.y as f32 + 0.5) * TILE_SIZE,
                ))
            }
            // Home from the Moon: the town's landing spot (the rescue crew's drop).
            None if up_there => Some((12.0 * TILE_SIZE, 4.0 * TILE_SIZE)),
            None => None,
        }
    }

    /// Beams `player_id`'s rig through uplink `from` (validated by `check_build`).
    pub(super) fn beam_through_uplink(
        &mut self,
        player_id: PlayerId,
        from: crate::base::StructureId,
    ) {
        let Some((x, y)) = self.uplink_destination(player_id, from) else {
            return;
        };
        self.active_drills.remove(&player_id);
        self.active_drill_timers.remove(&player_id);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.x = crate::quantize::quantize_simulation_scalar(x);
            player.y = crate::quantize::quantize_simulation_scalar(y);
            player.x_remainder = 0.0;
            player.velocity_x = 0.0;
            player.velocity_y = 0.0;
        }
    }

    /// The beam event of an applied uplink command (after it moved the rig).
    pub(super) fn uplink_event(
        &self,
        player_id: PlayerId,
        command: &crate::PlayerCommand,
    ) -> Option<GameplayEventKind> {
        let crate::PlayerCommand::Build(crate::base::BuildCommand::Use {
            service: crate::base::ModuleService::Beam,
            ..
        }) = command
        else {
            return None;
        };
        let player = self.players.get(&player_id)?;
        Some(GameplayEventKind::UplinkBeamed {
            player_id,
            to_moon: self.on_moon(tile_of(player.x, player.y)),
        })
    }

    // --- Lunar enemies ----------------------------------------------------------------------

    /// Whether enemies leave `tile` alone on the Moon: `Some(true)` in the colony,
    /// `Some(false)` anywhere else on or in the Moon, `None` off the Moon (the usual rules
    /// apply).
    #[must_use]
    pub fn lunar_safe_zone(&self, tile: TilePosition) -> Option<bool> {
        let layout = self.moon_layout()?;
        if !layout.moon.bounds().expanded(24).contains(tile) {
            return None;
        }
        Some(layout.colony_area().expanded(6).contains(tile))
    }

    /// Spawns Moon enemies around rigs and pilots on the Moon (the director's check).
    pub(super) fn spawn_lunar_enemies(&mut self, now: SimulationTick) {
        let Some(layout) = self.moon_layout() else {
            return;
        };
        let actors: Vec<(PlayerId, TilePosition)> = self
            .players
            .keys()
            .filter_map(|id| self.actor_tile(*id).map(|tile| (*id, tile)))
            .filter(|(_, tile)| self.lunar_safe_zone(*tile) == Some(false))
            .collect();
        // The world's hazard setting scales Moon creatures like every roaming spawn.
        let density = self.enemies.difficulty.density()
            * crate::enemy::hazard_density(self.terrain.settings().hazards);
        for (index, (player_id, tile)) in actors.into_iter().enumerate() {
            if self.entities.count_of(EntityKind::Enemy) >= super::enemies::MAX_ENEMIES {
                break;
            }
            let (cx, cy) = centre(tile);
            let near = self
                .enemies()
                .filter(|(entity, state)| {
                    state.kind.is_lunar()
                        && (entity.body.x - cx).abs() < 30.0 * TILE_SIZE
                        && (entity.body.y - cy).abs() < 20.0 * TILE_SIZE
                })
                .count();
            if near >= LUNAR_LOCAL_CAP {
                continue;
            }
            let roll = hash2(
                self.terrain.seed() ^ now.get().wrapping_mul(0x9E37_79B9_7F4A_7C15),
                SALT_LUNAR_SPAWN,
                i32::try_from(player_id.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(index).unwrap_or(0),
            );
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "a per-mille chance"
            )]
            let chance = (LUNAR_SPAWN_PER_MILLE as f32 * density) as u64;
            if roll % 1000 >= chance {
                continue;
            }
            let in_forest = layout.forests.iter().any(|f| f.contains(tile.x));
            let near_habitat = layout
                .habitats
                .iter()
                .any(|h| (h.center_x - tile.x).abs() < 50);
            let roster = lunar_roster(in_forest, near_habitat);
            let total: u32 = roster.iter().map(|(_, weight)| weight).sum();
            let mut pick = u32::try_from((roll >> 12) % u64::from(total.max(1))).unwrap_or(0);
            let mut kind = roster[0].0;
            for (candidate, weight) in roster {
                if pick < *weight {
                    kind = *candidate;
                    break;
                }
                pick -= weight;
            }
            let Some(site) = self.lunar_spawn_site(&layout, kind, tile, roll) else {
                continue;
            };
            let count = if kind == EnemyKind::CrystalMite {
                3 + usize::try_from((roll >> 30) % 3).unwrap_or(0)
            } else {
                1
            };
            for offset in 0..count {
                let spot = TilePosition {
                    x: site.x + i32::try_from(offset % 2).unwrap_or(0),
                    y: site.y - i32::try_from(offset / 2).unwrap_or(0),
                };
                let point = Self::spawn_point(kind, spot);
                let _ = self.spawn_enemy_with(kind, point, LUNAR_TIER, None, None);
            }
        }
    }

    /// A spot 14-24 tiles to one side of `around` for an enemy of `kind`: on the surface, or
    /// along the lava tube the actor is in.
    fn lunar_spawn_site(
        &self,
        layout: &MoonLayout,
        kind: EnemyKind,
        around: TilePosition,
        roll: u64,
    ) -> Option<TilePosition> {
        let side = if (roll >> 20) & 1 == 0 { 1 } else { -1 };
        let distance = 14 + i32::try_from((roll >> 21) % 11).unwrap_or(0);
        let x = around.x + side * distance;
        let tube = layout.tube_at(around.x, around.y);
        let y = if let Some(tube) = tube {
            if x < tube.first || x > tube.last {
                return None;
            }
            tube.center_row(x) + 1
        } else {
            layout.surface_row(x)? - 1
        };
        let flying = matches!(kind.stats().locomotion, Locomotion::Flying);
        let tile = TilePosition {
            x,
            y: if flying && tube.is_none() { y - 3 } else { y },
        };
        if self.lunar_safe_zone(tile) != Some(false) {
            return None;
        }
        // Settle onto the floor below (walkers).
        let mut spot = tile;
        if !flying {
            for _ in 0..6 {
                let below = TilePosition {
                    x: spot.x,
                    y: spot.y + 1,
                };
                if self.terrain.is_solid_at(below) {
                    break;
                }
                spot = below;
            }
        }
        (!self.terrain.is_solid_at(spot)
            && !self.terrain.is_solid_at(TilePosition {
                x: spot.x,
                y: spot.y - 1,
            }))
        .then_some(spot)
    }
}

/// Takes `price` from `player` (credits and materials).
fn pay(player: &mut crate::Player, price: crate::moon::ColonyPrice) {
    player.credits = player.credits.saturating_sub(price.credits);
    player.cargo_bay.remove(
        crate::item::ItemKind::Material(StrategicResourceKind::Helium3),
        price.helium,
    );
    player.cargo_bay.remove(
        crate::item::ItemKind::Material(StrategicResourceKind::AlienAlloy),
        price.alloy,
    );
}
