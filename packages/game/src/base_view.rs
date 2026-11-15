//! Presentation data for base building (WP-BASE), computed from the world once per frame so the
//! renderer never touches the simulation: the build-mode preview of a seat and the rows of an
//! open module menu.

use fatherload_simulation::{
    PlayerId, WorldState,
    base::{
        BuildCommand, BuildRefusal, Buildable, Claim, FUEL_STATION_CAPACITY, ModuleKind,
        ModuleService, PlacedModule, PowerBudget, StructureId, UPLINK_FEE_PERCENT, origin_for,
        workshop_recipe,
    },
    economy::REPAIR_UNIT_COST,
    terrain::{TILE_SIZE, TilePosition},
    world::BuildQuote,
};

use crate::shell::BuildDraft;

/// What build mode shows for one seat this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildPreview {
    /// Index of the selected piece in [`Buildable::PALETTE`].
    pub selected: usize,
    pub item: Buildable,
    pub facing: fatherload_simulation::base::Facing,
    /// The cursor tile (the footprint's bottom-left).
    pub target: TilePosition,
    /// The footprint's top-left and size.
    pub origin: TilePosition,
    pub size: (i32, i32),
    /// The rig's tile (the reach square is around it).
    pub rig: TilePosition,
    /// Whether placing here works, and what it takes.
    pub place: Result<BuildQuote, BuildRefusal>,
    /// Whether deconstructing the cursor tile works, and what it gives back.
    pub remove: Result<BuildQuote, BuildRefusal>,
    /// The claim under the cursor.
    pub claim: Option<Claim>,
    /// Blocks of the selected piece's own kind in the hopper.
    pub blocks_on_hand: u32,
    /// Building blocks of every kind in the hopper.
    pub hopper_total: u32,
    pub hopper_capacity: u32,
}

impl BuildPreview {
    /// The preview of `draft` for `player_id`'s rig.
    #[must_use]
    pub fn for_draft(world: &WorldState, player_id: PlayerId, draft: &BuildDraft) -> Option<Self> {
        let player = world.player(player_id)?;
        let rig = player.tile_position(TILE_SIZE);
        let target = draft.target(rig);
        let item = draft.item();
        let size = item.footprint();
        let place = world.check_build(player_id, &draft.place_command(target));
        let remove = world.check_build(player_id, &BuildCommand::Remove { at: target });
        let blocks_on_hand = item
            .base_cost()
            .block
            .map_or(0, |block| player.block_hopper.count(block));
        Some(Self {
            selected: draft.selected,
            item,
            facing: draft.facing.within(item.facings()),
            target,
            origin: origin_for(target, size),
            size,
            rig,
            place,
            remove,
            claim: world.bases().claim_at(target).copied(),
            blocks_on_hand,
            hopper_total: player.block_hopper.total(),
            hopper_capacity: player.block_hopper_capacity,
        })
    }
}

/// One row of a module menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRow {
    pub service: ModuleService,
    pub label: String,
    /// What it costs or does right now ("Free", "120 c", "Home", ...).
    pub value: String,
    /// Why it is unavailable, if it is.
    pub blocked: Option<BuildRefusal>,
}

/// An open module menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseModuleView {
    pub module: PlacedModule,
    pub claim: Claim,
    pub powered: bool,
    pub power: PowerBudget,
    pub rows: Vec<ModuleRow>,
    /// The viewer owns the claim.
    pub owned: bool,
    /// The viewer's home point is this module.
    pub is_home: bool,
}

impl BaseModuleView {
    /// The menu of module `id` as `player_id` sees it.
    #[must_use]
    pub fn for_module(world: &WorldState, player_id: PlayerId, id: StructureId) -> Option<Self> {
        let module = world.bases().module(id)?.clone();
        let claim = *world.bases().claim(module.claim)?;
        let player = world.player(player_id)?;
        let is_home = world
            .bases()
            .home_of(player_id)
            .is_some_and(|home| home.id == id);
        let rows = module
            .kind
            .services()
            .iter()
            .map(|service| {
                let blocked = world
                    .check_build(
                        player_id,
                        &BuildCommand::Use {
                            structure: id,
                            service: *service,
                        },
                    )
                    .err();
                let value = match service {
                    ModuleService::Refuel => format!("{} in reserve, free", module.fuel),
                    ModuleService::OrderSupply => {
                        let (units, credits) = world.supply_drop_offer(id, player.credits);
                        if module.fuel >= FUEL_STATION_CAPACITY {
                            "Full".to_owned()
                        } else {
                            format!("{units} fuel for {credits} c")
                        }
                    }
                    ModuleService::Repair => format!("{REPAIR_UNIT_COST} c per point"),
                    ModuleService::DepositCargo => format!("{} in the bay", player.cargo_used()),
                    ModuleService::DepositBlocks => {
                        format!("{} in the hopper", player.block_hopper.total())
                    }
                    ModuleService::WithdrawAll => format!(
                        "{} / {} stored",
                        module.storage.slots_used(),
                        module.kind.storage_capacity()
                    ),
                    ModuleService::SellCargo => {
                        let value: u32 = player
                            .cargo_bay
                            .minerals()
                            .map(|(mineral, count)| world.mineral_market_value(mineral) * count)
                            .chain(
                                player
                                    .cargo_bay
                                    .artifacts()
                                    .map(|(artifact, count)| artifact.value() * count),
                            )
                            .sum();
                        format!("~{} c after fee", value - value * UPLINK_FEE_PERCENT / 100)
                    }
                    ModuleService::SetHome => {
                        if is_home {
                            "Home".to_owned()
                        } else {
                            "Respawn here".to_owned()
                        }
                    }
                    ModuleService::CycleAccess => claim.access.name().to_owned(),
                    ModuleService::RefineOre => format!(
                        "{} / {} waiting",
                        module.storage.total(),
                        module.kind.storage_capacity()
                    ),
                    ModuleService::CollectRefined => format!("{} c ready", module.refined),
                    ModuleService::Craft(kind) => {
                        workshop_recipe(*kind).map_or_else(String::new, |(blocks, credits)| {
                            format!(
                                "{blocks} blocks + {credits} c (have {})",
                                player.explosive_count(*kind)
                            )
                        })
                    }
                };
                ModuleRow {
                    service: *service,
                    label: service.name().to_owned(),
                    value,
                    blocked,
                }
            })
            .collect();
        Some(Self {
            powered: world.base_module_powered(id),
            power: world.claim_power(claim.id),
            owned: claim.owner == player_id,
            is_home,
            rows,
            claim,
            module,
        })
    }

    /// Whether the module needs power to work.
    #[must_use]
    pub const fn needs_power(&self) -> bool {
        self.module.kind.power_draw() > 0
    }

    /// The module's kind.
    #[must_use]
    pub const fn kind(&self) -> ModuleKind {
        self.module.kind
    }
}
