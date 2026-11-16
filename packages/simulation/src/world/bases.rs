//! The world side of base building (WP-BASE, see [`crate::base`]): validating and applying
//! [`BuildCommand`]s inside the fixed tick, fuel synthesis, lift ladders and home points.

use crate::{
    base::{
        BUILD_REACH, BuildCommand, BuildCost, BuildPayment, BuildRefusal, Buildable, BuiltTile,
        CLAIM_RADIUS, Claim, ClaimAccess, FUEL_STATION_CAPACITY, FUEL_STATION_STARTING_FUEL,
        FUEL_SYNTH_PER_PERIOD, FUEL_SYNTH_PERIOD_MILLIS, Facing, LIFT_CLIMB_SPEED,
        LIFT_SLIDE_SPEED, MAX_CLAIMS, MODULE_REFUND_PERCENT, ModuleKind, ModuleService,
        PlacedModule, PowerBudget, REFINE_PER_PERIOD, REFINERY_BONUS_PERCENT, SUPPLY_BASE_FEE,
        SUPPLY_UNIT_PRICE, StructureId, UPLINK_FEE_PERCENT, beacon_price, block_credit_price,
        claim_limit, footprint_tiles, origin_for, workshop_recipe,
    },
    economy::{REPAIR_UNIT_COST, repair_amount},
    ids::PlayerId,
    item::{BlockKind, ItemCategory, ItemContainer, ItemKind, ItemStack},
    player::Player,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::strata::band_reached,
};

use super::WorldState;

/// Rows below the top of the world under the town where no claim may reach (the town and its
/// approach stay free; deep under the town is fine).
pub const TOWN_NO_BUILD_ROWS: i32 = 24;
/// Columns either side of the town's lots that stay free of claims.
pub const TOWN_NO_BUILD_MARGIN: i32 = 2;

/// What a build action costs and refunds, once stock on hand is counted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BuildQuote {
    /// Taken from the player (blocks from the hopper, credits).
    pub cost: BuildPayment,
    /// Given back (deconstruction).
    pub refund: BuildPayment,
}

impl WorldState {
    /// Whether `command` from `player_id` would be accepted right now, and what it would cost
    /// or refund. The fixed tick applies exactly this verdict; build mode previews it.
    ///
    /// # Errors
    ///
    /// The [`BuildRefusal`] that stops the action.
    pub fn check_build(
        &self,
        player_id: PlayerId,
        command: &BuildCommand,
    ) -> Result<BuildQuote, BuildRefusal> {
        let player = self
            .players
            .get(&player_id)
            .ok_or(BuildRefusal::UnknownPlayer)?;
        if player.hull <= 0.0 {
            return Err(BuildRefusal::RigDisabled);
        }
        // On foot the pilot builds and uses modules from where it stands (Part II F5).
        let rig = self
            .actor_tile(player_id)
            .unwrap_or_else(|| player.tile_position(TILE_SIZE));
        match *command {
            BuildCommand::Place { item, at, .. } => {
                self.check_place(player_id, player, rig, item, at)
            }
            BuildCommand::Remove { at } => self.check_remove(player_id, rig, at),
            BuildCommand::Use { structure, service } => {
                self.check_use(player_id, player, rig, structure, service)
            }
        }
    }

    /// Power supply and demand of the claim `claim`.
    #[must_use]
    pub fn claim_power(&self, claim: crate::base::ClaimId) -> PowerBudget {
        self.bases.power(claim, |tile| self.terrain.band_at(tile))
    }

    /// Whether base module `structure` has power.
    #[must_use]
    pub fn base_module_powered(&self, structure: StructureId) -> bool {
        self.bases
            .powered(structure, |tile| self.terrain.band_at(tile))
    }

    /// A supply drop for fuel station `structure` as `(fuel units, credits)` for a player with
    /// `credits` to spend (the largest drop they can afford, up to a full reserve).
    #[must_use]
    pub fn supply_drop_offer(&self, structure: StructureId, credits: u32) -> (u32, u32) {
        let Some(module) = self.bases.module(structure) else {
            return (0, 0);
        };
        let missing = FUEL_STATION_CAPACITY.saturating_sub(module.fuel);
        let fee = self.supply_drop_fee(module);
        let affordable = credits.saturating_sub(fee) / SUPPLY_UNIT_PRICE;
        let units = missing.min(affordable);
        if units == 0 {
            return (
                0,
                fee.saturating_add(SUPPLY_UNIT_PRICE.saturating_mul(missing.min(1))),
            );
        }
        (units, fee + units * SUPPLY_UNIT_PRICE)
    }

    /// The flat delivery fee of a supply drop to `module` (deeper costs more).
    fn supply_drop_fee(&self, module: &PlacedModule) -> u32 {
        let anchor = module.anchor();
        let depth = (anchor.y - self.terrain.surface_row(anchor.x)).max(0);
        SUPPLY_BASE_FEE.saturating_add(u32::try_from(depth / 2).unwrap_or(0))
    }

    /// Where the rescue crew drops `player_id`: in front of their home point if it still
    /// stands, else `None` (town).
    #[must_use]
    pub fn rescue_home(&self, player_id: PlayerId) -> Option<(f32, f32)> {
        // A contractor-built home with a bed the player made theirs comes first (WP-HOME).
        if let Some(plot) = self.homes.home_point_of(player_id) {
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let x = (plot.door_column() as f32 + 0.5) * TILE_SIZE;
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let y = (plot.site.ground_row as f32 - 0.5) * TILE_SIZE;
            return Some((x, y));
        }
        let home = self.bases.home_of(player_id)?;
        let (width, _) = home.kind.footprint();
        let anchor = home.anchor();
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let x = (width as f32).mul_add(0.5, anchor.x as f32) * TILE_SIZE;
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let y = (anchor.y as f32 + 0.5) * TILE_SIZE;
        Some((x, y))
    }

    fn claim_for_build(
        &self,
        player_id: PlayerId,
        at: TilePosition,
    ) -> Result<&Claim, BuildRefusal> {
        let claim = self.bases.claim_at(at).ok_or(BuildRefusal::NoClaim)?;
        if !claim.access.may_build(claim.owner, player_id) {
            return Err(BuildRefusal::NotPermitted);
        }
        Ok(claim)
    }

    fn rig_overlaps(&self, tile: TilePosition) -> bool {
        let radius = super::movement::AUTHORITATIVE_PLAYER_RADIUS;
        self.players.values().any(|player| {
            #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
            let span = |center: f32| {
                (
                    ((center - radius) / TILE_SIZE).floor() as i32,
                    ((center + radius) / TILE_SIZE).floor() as i32,
                )
            };
            let (left, right) = span(player.x);
            let (top, bottom) = span(player.y);
            (left..=right).contains(&tile.x) && (top..=bottom).contains(&tile.y)
        }) || self.pilots().any(|(_, body, state)| {
            // Never wall a pilot in either.
            #[allow(clippy::cast_possible_truncation, reason = "bounded world pixels")]
            let cell = |value: f32| (value / TILE_SIZE).floor() as i32;
            let half = crate::pilot::PILOT_HALF_WIDTH;
            (cell(body.x - half)..=cell(body.x + half)).contains(&tile.x)
                && (cell(body.y - state.height())..=cell(body.y - 0.5)).contains(&tile.y)
        })
    }

    /// Whether the tiles of a module footprint are open (air, no front fixture, no module) and
    /// it stands on solid ground.
    fn check_footprint(&self, origin: TilePosition, size: (i32, i32)) -> Result<(), BuildRefusal> {
        for tile in footprint_tiles(origin, size) {
            if self.terrain.is_solid_at(tile)
                || self.bases.fixtures_at(tile).front.is_some()
                || self.bases.module_at(tile).is_some()
            {
                return Err(BuildRefusal::Blocked);
            }
        }
        let floor = origin.y + size.1;
        if (origin.x..origin.x + size.0)
            .any(|x| !self.terrain.is_solid_at(TilePosition { x, y: floor }))
        {
            return Err(BuildRefusal::NeedsFloor);
        }
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per kind of buildable keeps the placement rules together"
    )]
    fn check_place(
        &self,
        player_id: PlayerId,
        player: &Player,
        rig: TilePosition,
        item: Buildable,
        at: TilePosition,
    ) -> Result<BuildQuote, BuildRefusal> {
        let size = item.footprint();
        let origin = origin_for(at, size);
        if footprint_tiles(origin, size).any(|tile| !self.terrain.contains(tile))
            || !self.terrain.contains(TilePosition {
                x: at.x,
                y: at.y + 1,
            })
        {
            return Err(BuildRefusal::OutsideWorld);
        }
        let near = footprint_tiles(origin, size).any(|tile| {
            (tile.x - rig.x).abs() <= BUILD_REACH && (tile.y - rig.y).abs() <= BUILD_REACH
        });
        if !near {
            return Err(BuildRefusal::OutOfReach);
        }
        let mut cost = item.base_cost();
        match item {
            Buildable::Module(ModuleKind::Beacon) => {
                let held = self.bases.claims_held(player_id);
                let limit = if self.is_test_mode() {
                    MAX_CLAIMS
                } else {
                    claim_limit(
                        self.terrain
                            .band_at(TilePosition {
                                x: at.x,
                                y: self.progression.deepest_tile_reached,
                            })
                            .index(),
                    )
                };
                if held >= limit {
                    return Err(BuildRefusal::TooManyClaims);
                }
                let (first, last) = crate::surface::town_span();
                let overlaps_town = at.x + CLAIM_RADIUS >= first - TOWN_NO_BUILD_MARGIN
                    && at.x - CLAIM_RADIUS <= last + TOWN_NO_BUILD_MARGIN
                    && at.y - CLAIM_RADIUS < TOWN_NO_BUILD_ROWS;
                if overlaps_town {
                    return Err(BuildRefusal::InTown);
                }
                // Villages (WP-VILLAGE) stay unclaimed: their area grown by the claim's reach.
                if crate::worldgen::streamed::in_village_area(
                    self.terrain.settings(),
                    at,
                    CLAIM_RADIUS,
                ) {
                    return Err(BuildRefusal::InVillage);
                }
                if self.bases.claims().any(|claim| {
                    (claim.center.x - at.x).abs() <= 2 * CLAIM_RADIUS
                        && (claim.center.y - at.y).abs() <= 2 * CLAIM_RADIUS
                }) {
                    return Err(BuildRefusal::OverlapsClaim);
                }
                // Home plots (WP-HOME) are claims of their own.
                let on_plot = self
                    .homes
                    .plots_between(at.x - CLAIM_RADIUS, at.x + CLAIM_RADIUS)
                    .any(|plot| {
                        plot.site.ground_row >= at.y - CLAIM_RADIUS - 3
                            && plot.site.ground_row <= at.y + CLAIM_RADIUS + 14
                    });
                if on_plot {
                    return Err(BuildRefusal::OnPlot);
                }
                self.check_footprint(origin, size)?;
                cost.credits = beacon_price(held);
            }
            Buildable::Module(kind) => {
                let claim = self.claim_for_build(player_id, at)?;
                if footprint_tiles(origin, size).any(|tile| !claim.contains(tile)) {
                    return Err(BuildRefusal::LeavesClaim);
                }
                if let Some(band) = kind.unlock_band()
                    && !self.is_test_mode()
                    && !band_reached(
                        self.terrain.settings(),
                        self.progression.deepest_tile_reached,
                        band,
                    )
                {
                    return Err(BuildRefusal::Locked(band));
                }
                if kind.surface_only() && !claim.surface {
                    return Err(BuildRefusal::SurfaceOnly);
                }
                if kind == ModuleKind::LunarUplink && !player.lunar.landed && !self.is_test_mode() {
                    return Err(BuildRefusal::NeedsMoonLanding);
                }
                self.check_footprint(origin, size)?;
            }
            Buildable::Tile(_) => {
                let _claim = self.claim_for_build(player_id, at)?;
                if self.terrain.is_solid_at(at)
                    || self.bases.module_at(at).is_some()
                    || self.bases.fixtures_at(at).front.is_some()
                    || self.rig_overlaps(at)
                {
                    return Err(BuildRefusal::Blocked);
                }
            }
            Buildable::BackWall(_) => {
                let _claim = self.claim_for_build(player_id, at)?;
                if self.terrain.is_solid_at(at) || self.bases.fixtures_at(at).back_wall.is_some() {
                    return Err(BuildRefusal::Blocked);
                }
            }
            Buildable::Fixture(fixture) => {
                let _claim = self.claim_for_build(player_id, at)?;
                if self.terrain.is_solid_at(at)
                    || self.bases.fixtures_at(at).front.is_some()
                    || self.bases.module_at(at).is_some()
                {
                    return Err(BuildRefusal::Blocked);
                }
                if fixture.needs_floor()
                    && !self.terrain.is_solid_at(TilePosition {
                        x: at.x,
                        y: at.y + 1,
                    })
                {
                    return Err(BuildRefusal::NeedsFloor);
                }
            }
        }
        let payment = payment_for(player, cost);
        if payment.credits > player.credits {
            return Err(BuildRefusal::CannotAfford);
        }
        Ok(BuildQuote {
            cost: payment,
            refund: BuildPayment::default(),
        })
    }

    fn check_remove(
        &self,
        player_id: PlayerId,
        rig: TilePosition,
        at: TilePosition,
    ) -> Result<BuildQuote, BuildRefusal> {
        if (at.x - rig.x).abs() > BUILD_REACH || (at.y - rig.y).abs() > BUILD_REACH {
            return Err(BuildRefusal::OutOfReach);
        }
        let Some(claim) = self.bases.claim_at(at) else {
            return Err(BuildRefusal::NothingThere);
        };
        if !claim.access.may_build(claim.owner, player_id) {
            return Err(BuildRefusal::NotPermitted);
        }
        let mut refund = BuildPayment::default();
        if let Some(module) = self.bases.module_at(at) {
            if !module.storage.is_empty() || module.refined > 0 {
                return Err(BuildRefusal::NotEmpty);
            }
            if module.kind == ModuleKind::Beacon && !self.claim_is_bare(claim) {
                return Err(BuildRefusal::NotEmpty);
            }
            refund.credits = module.paid_credits.saturating_mul(MODULE_REFUND_PERCENT) / 100;
            refund.blocks = module.paid_blocks.clone();
        } else if let Some(TileKind::Built(tile)) = self.terrain.tile(at).map(|tile| tile.kind) {
            if tile.is_ruin() {
                // Ruin pieces (WP-PILOT) belong to the ruin, not the claim.
                return Err(BuildRefusal::NotPermitted);
            }
            if self.supports_something(at) {
                return Err(BuildRefusal::Supporting);
            }
            match tile {
                BuiltTile::Wall(block) => refund.blocks.add(block, 1),
                other => {
                    refund.credits = Buildable::Tile(other).base_cost().credits / 2;
                }
            }
        } else {
            let fixtures = self.bases.fixtures_at(at);
            if let Some((fixture, _)) = fixtures.front {
                refund.credits = fixture.credits() / 2;
            } else if let Some(block) = fixtures.back_wall {
                refund.blocks.add(block, 1);
            } else {
                return Err(BuildRefusal::NothingThere);
            }
        }
        Ok(BuildQuote {
            cost: BuildPayment::default(),
            refund,
        })
    }

    /// Whether a module or a fixture that needs a floor stands on the tile at `at`.
    fn supports_something(&self, at: TilePosition) -> bool {
        let above = TilePosition {
            x: at.x,
            y: at.y - 1,
        };
        self.bases
            .module_at(above)
            .is_some_and(|module| module.anchor().y == above.y)
            || self
                .bases
                .fixtures_at(above)
                .front
                .is_some_and(|(fixture, _)| fixture.needs_floor())
    }

    /// Whether only the beacon is left in `claim` (no other module, piece or fixture).
    fn claim_is_bare(&self, claim: &Claim) -> bool {
        let (min, max) = claim.bounds();
        if self
            .bases
            .modules_in(claim.id)
            .any(|module| module.kind != ModuleKind::Beacon)
            || self.bases.fixtures_in(min, max).next().is_some()
        {
            return false;
        }
        !(min.y..=max.y).any(|y| {
            (min.x..=max.x).any(|x| {
                matches!(
                    self.terrain
                        .tile(TilePosition { x, y })
                        .map(|tile| tile.kind),
                    Some(TileKind::Built(_))
                )
            })
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per module service keeps the service rules together"
    )]
    fn check_use(
        &self,
        player_id: PlayerId,
        player: &Player,
        rig: TilePosition,
        structure: StructureId,
        service: ModuleService,
    ) -> Result<BuildQuote, BuildRefusal> {
        let module = self
            .bases
            .module(structure)
            .ok_or(BuildRefusal::NothingThere)?;
        if !module.kind.services().contains(&service) {
            return Err(BuildRefusal::NoSuchService);
        }
        if !module.reachable_from(rig) {
            return Err(BuildRefusal::NotAtModule);
        }
        let claim = self
            .bases
            .claim(module.claim)
            .ok_or(BuildRefusal::NothingThere)?;
        if !claim.access.may_use(claim.owner, player_id) {
            return Err(BuildRefusal::NotPermitted);
        }
        let mut quote = BuildQuote::default();
        let capacity = module.kind.storage_capacity();
        match service {
            ModuleService::Refuel => {
                if module.fuel == 0 || player.fuel_capacity - player.fuel < 1.0 {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::OrderSupply => {
                if module.fuel >= FUEL_STATION_CAPACITY {
                    return Err(BuildRefusal::NothingToDo);
                }
                let (units, credits) = self.supply_drop_offer(structure, player.credits);
                if units == 0 {
                    return Err(BuildRefusal::CannotAfford);
                }
                quote.cost.credits = credits;
            }
            ModuleService::Repair => {
                if !self.base_module_powered(structure) {
                    return Err(BuildRefusal::Unpowered);
                }
                if player.max_hull() - player.hull < 1.0 {
                    return Err(BuildRefusal::NothingToDo);
                }
                if player.credits < REPAIR_UNIT_COST {
                    return Err(BuildRefusal::CannotAfford);
                }
            }
            ModuleService::DepositCargo => {
                let any = player.cargo_bay.iter().any(|(kind, _)| {
                    is_cargo(kind) && module.storage.accepts(kind, 1, capacity) > 0
                });
                if !any {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::DepositBlocks => {
                let any = player
                    .block_hopper
                    .iter()
                    .any(|(kind, _)| module.storage.accepts(kind, 1, capacity) > 0);
                if !any {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::WithdrawAll => {
                if !module
                    .storage
                    .iter()
                    .any(|(kind, _)| player.room_for(kind, 1) > 0)
                {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::SellCargo => {
                if !self.base_module_powered(structure) {
                    return Err(BuildRefusal::Unpowered);
                }
                if !player.cargo_bay.has_minerals() && !player.cargo_bay.has_artifacts() {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::Sleep => {
                if !self.clock().phase().sleepable() {
                    return Err(BuildRefusal::NotTired);
                }
                if self.is_in_bed(player_id) {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::SetHome => {
                if self
                    .bases
                    .home_of(player_id)
                    .is_some_and(|home| home.id == structure)
                {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::CycleAccess => {
                if claim.owner != player_id {
                    return Err(BuildRefusal::NotPermitted);
                }
            }
            ModuleService::RefineOre => {
                let any = player.cargo_bay.iter().any(|(kind, _)| {
                    matches!(kind, ItemKind::Mineral(_))
                        && module.storage.accepts(kind, 1, capacity) > 0
                });
                if !any {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::CollectRefined => {
                if module.refined == 0 {
                    return Err(BuildRefusal::NothingToDo);
                }
            }
            ModuleService::Beam => {
                if self.is_on_foot(player_id) {
                    return Err(BuildRefusal::BoardFirst);
                }
                if !self.base_module_powered(structure) {
                    return Err(BuildRefusal::Unpowered);
                }
                if self.uplink_destination(player_id, structure).is_none() {
                    return Err(BuildRefusal::NoUplinkTarget);
                }
            }
            ModuleService::Craft(kind) => {
                let (blocks, credits) = workshop_recipe(kind).ok_or(BuildRefusal::NoSuchService)?;
                let on_hand: u32 = BlockKind::ALL
                    .into_iter()
                    .map(|block| player.block_hopper.count(block))
                    .sum();
                if on_hand < blocks {
                    return Err(BuildRefusal::NeedsBlocks(blocks));
                }
                if player.credits < credits {
                    return Err(BuildRefusal::CannotAfford);
                }
                quote.cost = BuildPayment {
                    blocks: blocks_from_hopper(player, blocks),
                    credits,
                };
            }
        }
        Ok(quote)
    }

    /// Applies one build action inside the fixed tick (validated by [`Self::check_build`]).
    pub(super) fn apply_build_command(
        &mut self,
        player_id: PlayerId,
        command: &BuildCommand,
    ) -> PlayerScopedCommandOutcome {
        let Ok(quote) = self.check_build(player_id, command) else {
            return if self.players.contains_key(&player_id) {
                PlayerScopedCommandOutcome::IgnoredUnavailable
            } else {
                PlayerScopedCommandOutcome::UnknownPlayer
            };
        };
        match *command {
            BuildCommand::Place { item, at, facing } => {
                self.apply_place(player_id, item, at, facing, quote.cost);
            }
            BuildCommand::Remove { at } => self.apply_remove(player_id, at, &quote.refund),
            BuildCommand::Use { structure, service } => {
                self.apply_use(player_id, structure, service, &quote.cost);
            }
        }
        PlayerScopedCommandOutcome::Applied
    }

    fn apply_place(
        &mut self,
        player_id: PlayerId,
        item: Buildable,
        at: TilePosition,
        facing: Facing,
        cost: BuildPayment,
    ) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        for (kind, count) in cost.blocks.iter() {
            player.block_hopper.remove(kind, count);
        }
        player.credits = player.credits.saturating_sub(cost.credits);
        let facing = facing.within(item.facings());
        let size = item.footprint();
        match item {
            Buildable::Module(kind) => {
                let module = PlacedModule {
                    id: StructureId::default(),
                    kind,
                    origin: origin_for(at, size),
                    facing,
                    claim: crate::base::ClaimId::default(),
                    owner: player_id,
                    paid_credits: cost.credits,
                    paid_blocks: cost.blocks,
                    fuel: if kind == ModuleKind::FuelStation {
                        FUEL_STATION_STARTING_FUEL
                    } else {
                        0
                    },
                    storage: ItemContainer::new(),
                    refined: 0,
                };
                if kind == ModuleKind::Beacon {
                    let surface = at.y < self.terrain.surface_row(at.x) + 4;
                    self.bases.stake_claim(player_id, at, surface, module);
                } else if let Some(claim) = self.bases.claim_at(at).map(|claim| claim.id) {
                    self.bases.insert_module(PlacedModule { claim, ..module });
                }
            }
            Buildable::Tile(tile) => {
                let _placed = self.terrain.set_kind(at, TileKind::Built(tile));
            }
            Buildable::BackWall(block) => {
                self.bases
                    .update_fixtures(at, |fixtures| fixtures.back_wall = Some(block));
            }
            Buildable::Fixture(fixture) => {
                self.bases
                    .update_fixtures(at, |fixtures| fixtures.front = Some((fixture, facing)));
            }
        }
    }

    fn apply_remove(&mut self, player_id: PlayerId, at: TilePosition, refund: &BuildPayment) {
        if let Some(id) = self.bases.module_at(at).map(|module| module.id) {
            let _removed = self.bases.remove_module(id);
        } else if matches!(
            self.terrain.tile(at).map(|tile| tile.kind),
            Some(TileKind::Built(_))
        ) {
            let _cleared = self.terrain.set_kind(at, TileKind::Air);
        } else {
            self.bases.update_fixtures(at, |fixtures| {
                if fixtures.front.is_some() {
                    fixtures.front = None;
                } else {
                    fixtures.back_wall = None;
                }
            });
        }
        self.give_refund(player_id, at, refund);
    }

    /// Pays a refund: credits, and blocks into the hopper (what does not fit drops at `at`).
    fn give_refund(&mut self, player_id: PlayerId, at: TilePosition, refund: &BuildPayment) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.credits = player.credits.saturating_add(refund.credits);
        let mut spilled = Vec::new();
        for (kind, count) in refund.blocks.iter() {
            let stowed = player.stow(kind, count);
            if stowed < count {
                spilled.push(ItemStack::new(kind, count - stowed));
            }
        }
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let (x, y) = (
            (at.x as f32 + 0.5) * TILE_SIZE,
            (at.y as f32 + 0.5) * TILE_SIZE,
        );
        for stack in spilled {
            let _drop = self.spawn_item_drop(x, y, (0.0, -60.0), stack, None);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per module service keeps the services together"
    )]
    fn apply_use(
        &mut self,
        player_id: PlayerId,
        structure: StructureId,
        service: ModuleService,
        cost: &BuildPayment,
    ) {
        let offer = self.supply_drop_offer(
            structure,
            self.players
                .get(&player_id)
                .map_or(0, |player| player.credits),
        );
        let claim = self.bases.module(structure).map(|module| module.claim);
        match service {
            ModuleService::SellCargo => {
                let before = self
                    .players
                    .get(&player_id)
                    .map_or(0, |player| player.credits);
                let _sold = self.sell_canonical_cargo(player_id);
                if let Some(player) = self.players.get_mut(&player_id) {
                    let payout = player.credits.saturating_sub(before);
                    let fee = payout.saturating_mul(UPLINK_FEE_PERCENT) / 100;
                    player.credits = player.credits.saturating_sub(fee);
                    self.progression.total_earnings =
                        self.progression.total_earnings.saturating_sub(fee);
                }
                return;
            }
            ModuleService::SetHome => {
                self.bases.set_home(player_id, structure);
                return;
            }
            ModuleService::Sleep => {
                let _outcome = self.lie_down(player_id, crate::clock::BedSpot::Module(structure));
                return;
            }
            ModuleService::Beam => {
                self.beam_through_uplink(player_id, structure);
                return;
            }
            ModuleService::Craft(kind) => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    for (block, count) in cost.blocks.iter() {
                        player.block_hopper.remove(block, count);
                    }
                    player.credits = player.credits.saturating_sub(cost.credits);
                    player.add_explosives(kind, 1);
                }
                return;
            }
            ModuleService::CycleAccess => {
                if let Some(claim) = claim {
                    let next = self
                        .bases
                        .claim(claim)
                        .map_or_else(ClaimAccess::default, |claim| claim.access.next());
                    self.bases.set_access(claim, next);
                }
                return;
            }
            _ => {}
        }
        let (Some(player), Some(module)) = (
            self.players.get_mut(&player_id),
            self.bases.module_mut(structure),
        ) else {
            return;
        };
        let capacity = module.kind.storage_capacity();
        match service {
            ModuleService::Refuel => {
                // An island base's station tops a rig up only as far as the sky allows
                // (`milestone::sky_refuel_level`: the dead-air gate stays honest).
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "altitudes are a few thousand rows"
                )]
                let altitude = (-player.y / TILE_SIZE).floor() as i32;
                let level = crate::milestone::sky_refuel_level(player, altitude);
                let missing = (level - player.fuel).floor().max(0.0);
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a non-negative tank's worth of fuel"
                )]
                let wanted = missing as u32;
                let pumped = wanted.min(module.fuel);
                module.fuel -= pumped;
                #[allow(clippy::cast_precision_loss, reason = "a reserve of a few hundred")]
                let added = pumped as f32;
                player.fuel = (player.fuel + added).min(player.fuel_capacity);
            }
            ModuleService::OrderSupply => {
                let (units, credits) = offer;
                player.credits = player.credits.saturating_sub(credits);
                module.fuel = (module.fuel + units).min(FUEL_STATION_CAPACITY);
            }
            ModuleService::Repair => {
                let _spent = repair_amount(player, 1.0);
            }
            ModuleService::DepositCargo => {
                player.cargo_bay.update_counts(|kind, count| {
                    if !is_cargo(kind) {
                        return count;
                    }
                    let stored = module.storage.accepts(kind, count, capacity);
                    module.storage.add(kind, stored);
                    count - stored
                });
            }
            ModuleService::DepositBlocks => {
                player.block_hopper.update_counts(|kind, count| {
                    let stored = module.storage.accepts(kind, count, capacity);
                    module.storage.add(kind, stored);
                    count - stored
                });
            }
            ModuleService::WithdrawAll => {
                module
                    .storage
                    .update_counts(|kind, count| count - player.stow(kind, count));
            }
            ModuleService::RefineOre => {
                player.cargo_bay.update_counts(|kind, count| {
                    if !matches!(kind, ItemKind::Mineral(_)) {
                        return count;
                    }
                    let stored = module.storage.accepts(kind, count, capacity);
                    module.storage.add(kind, stored);
                    count - stored
                });
            }
            ModuleService::CollectRefined => {
                let value = std::mem::take(&mut module.refined);
                player.credits = player.credits.saturating_add(value);
                self.progression.total_earnings =
                    self.progression.total_earnings.saturating_add(value);
            }
            ModuleService::SellCargo
            | ModuleService::Sleep
            | ModuleService::Beam
            | ModuleService::SetHome
            | ModuleService::CycleAccess
            | ModuleService::Craft(_) => {}
        }
    }

    /// Per-tick base upkeep, on a fixed schedule: powered fuel stations synthesize fuel and
    /// powered refineries refine their ore (the first kind in item order, a unit at a time).
    pub(super) fn advance_bases(&mut self, next_tick: crate::ids::SimulationTick) {
        if self.bases.is_empty()
            || !self
                .timing
                .rate
                .schedule_fires(next_tick, FUEL_SYNTH_PERIOD_MILLIS, 0)
        {
            return;
        }
        let filling: Vec<StructureId> = self
            .bases
            .modules()
            .filter(|module| {
                module.kind == ModuleKind::FuelStation && module.fuel < FUEL_STATION_CAPACITY
            })
            .map(|module| module.id)
            .filter(|id| self.base_module_powered(*id))
            .collect();
        for id in filling {
            if let Some(module) = self.bases.module_mut(id) {
                module.fuel = (module.fuel + FUEL_SYNTH_PER_PERIOD).min(FUEL_STATION_CAPACITY);
            }
        }
        let refining: Vec<StructureId> = self
            .bases
            .modules()
            .filter(|module| module.kind == ModuleKind::Refinery && !module.storage.is_empty())
            .map(|module| module.id)
            .filter(|id| self.base_module_powered(*id))
            .collect();
        for id in refining {
            for _ in 0..REFINE_PER_PERIOD {
                let Some(mineral) = self.bases.module(id).and_then(|module| {
                    module.storage.iter().find_map(|(kind, _)| match kind {
                        ItemKind::Mineral(mineral) => Some(mineral),
                        _ => None,
                    })
                }) else {
                    break;
                };
                let value = self
                    .canonical_mineral_market_value(mineral)
                    .saturating_mul(100 + REFINERY_BONUS_PERCENT)
                    / 100;
                if let Some(module) = self.bases.module_mut(id) {
                    module.storage.remove(ItemKind::Mineral(mineral), 1);
                    module.refined = module.refined.saturating_add(value);
                }
            }
        }
    }

    /// Lift ladders: a rig whose centre is on one burns no fuel, climbs while thrust is held
    /// (even with an empty tank) and never falls faster than a safe slide. Runs after the
    /// rig's movement step with the fuel it had before it.
    pub(super) fn apply_base_lift(&mut self, player_id: PlayerId, fuel_before: f32, thrust: bool) {
        let Some(tile) = self
            .players
            .get(&player_id)
            .map(|player| player.tile_position(TILE_SIZE))
        else {
            return;
        };
        if !self.bases.ladder_at(tile) {
            return;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.fuel = player.fuel.max(fuel_before.min(player.fuel_capacity));
        player.velocity_y = if thrust {
            player.velocity_y.min(-LIFT_CLIMB_SPEED)
        } else {
            player.velocity_y.min(LIFT_SLIDE_SPEED)
        };
    }
}

/// Whether a cargo-bay item goes to storage with "Store cargo" (ore, artifacts, materials).
const fn is_cargo(kind: ItemKind) -> bool {
    matches!(
        kind.category(),
        ItemCategory::Mineral | ItemCategory::Artifact | ItemCategory::Material
    )
}

/// What `cost` takes from `player`: blocks on hand first (a specific block, then any blocks
/// cheapest kind first), missing blocks bought at their credit price.
#[must_use]
pub fn payment_for(player: &Player, cost: BuildCost) -> BuildPayment {
    let mut payment = BuildPayment {
        blocks: ItemContainer::new(),
        credits: cost.credits,
    };
    if let Some(block) = cost.block {
        if player.block_hopper.count(block) > 0 {
            payment.blocks.add(block, 1);
        } else {
            payment.credits = payment.credits.saturating_add(block_credit_price(block));
        }
    }
    let needed = take_blocks(player, &mut payment.blocks, cost.any_blocks);
    payment.credits = payment
        .credits
        .saturating_add(needed.saturating_mul(block_credit_price(BlockKind::Dirt)));
    payment
}

/// Up to `count` blocks of any kind from `player`'s hopper, cheapest kind first.
fn blocks_from_hopper(player: &Player, count: u32) -> ItemContainer {
    let mut blocks = ItemContainer::new();
    let _missing = take_blocks(player, &mut blocks, count);
    blocks
}

/// Adds up to `count` blocks from `player`'s hopper (cheapest kind first, minus what `taken`
/// already holds) to `taken`; returns how many were missing.
fn take_blocks(player: &Player, taken: &mut ItemContainer, count: u32) -> u32 {
    let mut needed = count;
    for block in BlockKind::ALL {
        if needed == 0 {
            break;
        }
        let available = player
            .block_hopper
            .count(block)
            .saturating_sub(taken.count(block));
        let take = available.min(needed);
        if take > 0 {
            taken.add(block, take);
            needed -= take;
        }
    }
    needed
}
