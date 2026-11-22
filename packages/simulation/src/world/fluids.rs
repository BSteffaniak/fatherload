//! Fluids, world side (Part III F9; the rules and data live in [`crate::fluid`]).
//!
//! * **The fluid pass** ([`WorldState::advance_fluids`], last in the tick before the terrain
//!   journal closes): every terrain write this tick wakes the fluid cells next to it (a drilled
//!   wall beside a sealed pocket breaches it: [`GameplayEventKind::FluidBreached`]); then the
//!   world runs the fluid steps that are due ([`crate::fluid::fluid_steps_by`], tick-rate safe).
//!   A step takes up to [`FLUID_STEP_BUDGET`] woken cells in position order (round robin from
//!   where the last step stopped), runs their reactions and density swaps in order, then moves
//!   fluid between them from one snapshot (fall, spread, climb; every move a transfer, so
//!   volume is conserved) and writes only the cells that changed. Changed cells wake their
//!   neighbours; a cell that neither moved nor changed goes back to sleep.
//! * **The schedule** ([`FluidSchedule`]: the woken cells and the round-robin cursor) is host
//!   bookkeeping. It saves with the world (so a save mid-flood resumes exactly) but is neither
//!   hashed nor replicated: a client's authority copy never ticks, and its predicted copy wakes
//!   fluid from its own predicted terrain writes (drilling into a pocket starts the flood at
//!   once); the host's tile writes correct it through the op log like any terrain change.
//! * **Bodies**: [`WorldState::submersion`] measures any body; rigs get buoyancy, drag, the
//!   submarine mode, pressure and acid/heat damage ([`WorldState::apply_fluid_to_rig`],
//!   [`WorldState::submersion_gear`] is the Descent IV upgrade seam), pilots swim in fluid and
//!   take its damage, and creatures that cannot swim turn back at its edge
//!   ([`WorldState::keep_out_of_fluid`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    entity::Body,
    event::{GameplayEventKind, TickOutput},
    fluid::{
        ACID_DISSOLVE_COST, ACID_DISSOLVE_ODDS, ACID_MIN_LEVEL, COOL_WATER_COST, COOLED_METAL_ORE,
        FLUID_FULL, FLUID_MAX, FLUID_STEP_BUDGET, FluidCell, FluidKind, FluidReaction,
        METAL_ORE_LEVEL, PRESSURE_DAMAGE_PER_ROW, PRESSURE_SCAN_ROWS, QUENCH_WATER_COST,
        SUBMARINE_DIVE_ACCELERATION, SUBMARINE_FUEL_PER_SECOND, SUBMARINE_TOP_SPEED, Submersion,
        SubmersionGear, acid_dissolves, fluid_steps_by, stable_lower,
    },
    ids::{PlayerId, SimulationTick},
    quantize::quantize_simulation_scalar as quantize,
    terrain::{TILE_SIZE, Tile, TileKind, TilePosition},
    worldgen::TileRect,
};

use super::{
    WorldState,
    movement::{AUTHORITATIVE_PLAYER_RADIUS, AUTHORITATIVE_RIG_GRAVITY},
};

/// The four neighbours, in a fixed order (up, down, left, right).
const NEIGHBOURS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
/// A body counts as in a cell's fluid at this share of a full tile or more (creatures, pilots).
const DEEP_ENOUGH: u8 = FLUID_FULL / 3;

/// Which fluid cells the next fluid step looks at (see the module docs).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FluidSchedule {
    /// Woken cells, in position order.
    #[serde(default)]
    awake: BTreeSet<TilePosition>,
    /// Where the next step starts when the last one ran out of budget.
    #[serde(default)]
    cursor: Option<TilePosition>,
}

impl FluidSchedule {
    /// Whether no cell is waiting to move.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.awake.is_empty()
    }

    /// How many cells are waiting.
    #[must_use]
    pub fn awake_count(&self) -> usize {
        self.awake.len()
    }

    fn wake(&mut self, position: TilePosition) -> bool {
        self.awake.insert(position)
    }

    /// Takes up to `budget` cells from the cursor on (wrapping), leaving the rest for later.
    fn take_batch(&mut self, budget: usize) -> Vec<TilePosition> {
        let start = self.cursor.take();
        let mut batch: Vec<TilePosition> = match start {
            Some(from) => self
                .awake
                .range(from..)
                .chain(self.awake.range(..from))
                .take(budget)
                .copied()
                .collect(),
            None => self.awake.iter().take(budget).copied().collect(),
        };
        for position in &batch {
            self.awake.remove(position);
        }
        if !self.awake.is_empty()
            && let Some(last) = batch.last()
        {
            self.cursor = self.awake.range(*last..).next().copied();
        }
        // Process in position order whatever the wrap (determinism does not need it, but the
        // flow rules read more naturally top-down within a column).
        batch.sort_unstable();
        batch
    }
}

/// A working cell during a step: the fluid kind (or open air) and the level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Working {
    kind: Option<FluidKind>,
    level: u16,
}

/// One proposed move of `amount` of `kind` from `from` to `to`.
#[derive(Clone, Copy, Debug)]
struct Flow {
    from: TilePosition,
    to: TilePosition,
    kind: FluidKind,
    amount: u16,
}

/// Open air.
const AIR: Tile = Tile {
    kind: TileKind::Air,
    durability: 0,
};

/// `cell` with `cost` spent: what is left of it, or open air.
const fn spend(cell: FluidCell, cost: u8) -> Tile {
    if cell.level > cost {
        FluidCell {
            level: cell.level - cost,
            ..cell
        }
        .tile()
    } else {
        AIR
    }
}

/// What a step's reactions wrote, and which reactions it has announced.
#[derive(Default)]
struct StepLog {
    changed: BTreeSet<TilePosition>,
    reacted: BTreeSet<FluidReaction>,
}

impl StepLog {
    fn write(&mut self, world: &mut WorldState, at: TilePosition, tile: Tile) {
        let _ = world
            .terrain
            .set_tile_from_network(at, tile.kind, tile.durability);
        let _ = self.changed.insert(at);
    }

    /// Announces `reaction` at `at`, once per reaction per step.
    fn react(&mut self, reaction: FluidReaction, at: TilePosition, output: &mut TickOutput) {
        if self.reacted.insert(reaction) {
            output.push_event(GameplayEventKind::FluidReacted {
                reaction,
                x: at.x,
                y: at.y,
            });
        }
    }
}

const fn offset(position: TilePosition, (dx, dy): (i32, i32)) -> TilePosition {
    TilePosition {
        x: position.x + dx,
        y: position.y + dy,
    }
}

/// splitmix64 over a position and a step (acid's bite roll; stable on every target).
const fn step_roll(position: TilePosition, step: u64) -> u64 {
    let key = ((position.x.cast_unsigned() as u64) << 32) | (position.y.cast_unsigned() as u64);
    let mut z = key
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(step.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl WorldState {
    // --- Queries ----------------------------------------------------------------------------

    /// The fluid schedule (woken cells; see [`FluidSchedule`]).
    #[must_use]
    pub const fn fluid_schedule(&self) -> &FluidSchedule {
        &self.fluids
    }

    /// How deep a body centred on world pixel `(x, y)`, `half_height` px above and below, is in
    /// fluid: five samples down its height against each tile's fluid surface.
    #[must_use]
    pub fn submersion(&self, x: f32, y: f32, half_height: f32) -> Submersion {
        let frame = self.frame();
        let mut wet = 0_u8;
        let mut kind = None;
        for sample in 0..5_u8 {
            let sy = (half_height * 2.0).mul_add(f32::from(sample) / 4.0, y - half_height);
            let tile = frame.tile_at(x, sy);
            let Some(cell) = self.terrain.fluid_at(tile) else {
                continue;
            };
            let bottom = frame.row_px(tile.y + 1);
            let surface = cell.fill().mul_add(-TILE_SIZE, bottom);
            if sy >= surface {
                wet += 1;
                kind = Some(cell.kind);
            }
        }
        let middle = frame.tile_at(x, y);
        let depth_rows = if wet > 0 {
            (1..=PRESSURE_SCAN_ROWS)
                .take_while(|dy| {
                    self.terrain
                        .fluid_at(TilePosition {
                            x: middle.x,
                            y: middle.y - dy,
                        })
                        .is_some()
                })
                .count()
        } else {
            0
        };
        Submersion {
            share: f32::from(wet) / 5.0,
            kind,
            depth_rows: i32::try_from(depth_rows).unwrap_or(PRESSURE_SCAN_ROWS),
        }
    }

    /// What `player_id`'s rig brings into fluid: the upgrade seam for Descent IV gear. Today
    /// pressure bracing tiers add rows of pressure rating and lava plating keeps out molten
    /// metal's heat (through [`crate::protection::HazardGear::lava_damage`]); the Pressure Hull,
    /// Bio-Filter and Cryo Hull plug in here.
    #[must_use]
    pub fn submersion_gear(&self, player_id: PlayerId) -> SubmersionGear {
        let mut gear = SubmersionGear::default();
        if let Some(player) = self.players.get(&player_id) {
            gear.pressure_rows += 8 * i32::from(player.hazard_gear.pressure_bracing);
            // The Pressure Rating's seals and the Pressure Hull take the water's weight too on
            // Descent worlds (Wave 9: the Drowned and Frozen's deep basins).
            if self.progression_rules().is_descent() {
                gear.pressure_rows += 12 * i32::from(player.hazard_gear.pressure_rating);
                if player
                    .hazard_gear
                    .has_system(crate::depth_progression::RigSystem::PressureHull)
                {
                    gear.pressure_rows += 16;
                }
            }
            // The Bio-Filter (the Mycelial Mind's reward, Wave 8) shrugs off acid.
            if player
                .hazard_gear
                .has_system(crate::depth_progression::RigSystem::BioFilter)
            {
                gear.acid_resistance = 1.0;
            }
        }
        gear
    }

    /// Whether `player_id`'s pilot breathes in `kind` (the Bio-Filter / rebreather seam; no
    /// gear does yet).
    #[must_use]
    pub const fn pilot_breathes_in(&self, player_id: PlayerId, kind: FluidKind) -> bool {
        let _ = (player_id, kind, &self.fluids);
        false
    }

    // --- Staging (tests, scenes and dev tools) ----------------------------------------------

    /// Wakes every fluid cell in `rect` (a staging seam next to [`Self::terrain_mut`]: fluid
    /// written straight into the terrain sits still until something wakes it).
    pub fn wake_fluids(&mut self, rect: TileRect) {
        for position in rect.positions() {
            if self.terrain.fluid_at(position).is_some() {
                let _ = self.fluids.wake(position);
            }
        }
    }

    /// Writes the [`crate::fluid::demo_basins`] beside `origin` and leaves them still (the
    /// test-mode "test pools" command and screenshot scenes).
    pub(crate) fn pour_demo_basins(&mut self, origin: TilePosition) -> bool {
        let mut any = false;
        for (position, tile) in crate::fluid::demo_basins(origin) {
            if self
                .terrain
                .tile(position)
                .is_some_and(|found| crate::terrain::is_indestructible(found.kind))
            {
                continue;
            }
            any |= self
                .terrain
                .set_tile_from_network(position, tile.kind, tile.durability);
        }
        any
    }

    // --- The fluid pass ---------------------------------------------------------------------

    /// Wakes fluid next to this tick's terrain writes, then runs the fluid steps due by
    /// `next_tick`.
    pub(super) fn advance_fluids(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let changed: BTreeSet<TilePosition> = self
            .terrain
            .journal()
            .iter()
            .map(|change| change.position)
            .collect();
        let mut breach: Option<(FluidKind, TilePosition)> = None;
        for position in changed {
            let opened = self
                .terrain
                .tile(position)
                .is_some_and(|tile| tile.kind == TileKind::Air);
            if self.terrain.fluid_at(position).is_some() {
                let _ = self.fluids.wake(position);
            }
            for step in NEIGHBOURS {
                let next = offset(position, step);
                if let Some(cell) = self.terrain.fluid_at(next)
                    && self.fluids.wake(next)
                    && opened
                    && breach.is_none()
                {
                    breach = Some((cell.kind, next));
                }
            }
        }
        if let Some((kind, at)) = breach {
            output.push_event(GameplayEventKind::FluidBreached {
                kind,
                x: at.x,
                y: at.y,
            });
        }
        let hz = self.timing.rate.hz();
        let now = next_tick.get();
        let before = fluid_steps_by(now.saturating_sub(1), hz);
        let due = fluid_steps_by(now, hz).saturating_sub(before);
        for step in 1..=due {
            if self.fluids.is_idle() {
                break;
            }
            self.fluid_step(before + step, output);
        }
    }

    /// One fluid step (see the module docs).
    fn fluid_step(&mut self, step: u64, output: &mut TickOutput) {
        let batch = self.fluids.take_batch(FLUID_STEP_BUDGET);
        let mut log = StepLog::default();
        // Reactions and density swaps, in order, straight on the terrain.
        for &position in &batch {
            self.fluid_react(position, step, &mut log, output);
        }
        let mut changed = log.changed;
        // Flows from one snapshot.
        let mut flows: Vec<Flow> = Vec::new();
        for &position in &batch {
            self.propose_flows(position, step, &mut flows);
        }
        let mut working: BTreeMap<TilePosition, Working> = BTreeMap::new();
        let mut original: BTreeMap<TilePosition, Working> = BTreeMap::new();
        let mut held: BTreeSet<TilePosition> = BTreeSet::new();
        for flow in flows {
            for at in [flow.from, flow.to] {
                if let std::collections::btree_map::Entry::Vacant(slot) = working.entry(at) {
                    let cell = self.working_cell(at);
                    let _ = slot.insert(cell);
                    let _ = original.insert(at, cell);
                }
            }
            let source = working[&flow.from];
            let target = working[&flow.to];
            let compatible = target.kind.is_none_or(|kind| kind == flow.kind);
            if source.kind != Some(flow.kind) || !compatible {
                let _ = held.insert(flow.from);
                continue;
            }
            let room = u16::from(FLUID_MAX).saturating_sub(target.level);
            let amount = flow.amount.min(room).min(source.level);
            if amount < flow.amount {
                let _ = held.insert(flow.from);
            }
            if amount == 0 {
                continue;
            }
            if let Some(cell) = working.get_mut(&flow.from) {
                cell.level -= amount;
                if cell.level == 0 {
                    cell.kind = None;
                }
            }
            if let Some(cell) = working.get_mut(&flow.to) {
                cell.level += amount;
                cell.kind = Some(flow.kind);
            }
        }
        for (position, cell) in &working {
            if original.get(position) == Some(cell) {
                continue;
            }
            let tile = match cell.kind {
                Some(kind) if cell.level > 0 => FluidCell {
                    kind,
                    level: u8::try_from(cell.level).unwrap_or(FLUID_MAX),
                }
                .tile(),
                _ => AIR,
            };
            let _ = self
                .terrain
                .set_tile_from_network(*position, tile.kind, tile.durability);
            let _ = changed.insert(*position);
        }
        // Wake what changed and its fluid neighbours; held-back cells try again.
        for position in held {
            if self.terrain.fluid_at(position).is_some() {
                let _ = self.fluids.wake(position);
            }
        }
        for position in changed {
            for at in std::iter::once(position).chain(NEIGHBOURS.map(|step| offset(position, step)))
            {
                if self.terrain.fluid_at(at).is_some() {
                    let _ = self.fluids.wake(at);
                }
            }
        }
    }

    fn working_cell(&self, position: TilePosition) -> Working {
        self.terrain.fluid_at(position).map_or(
            Working {
                kind: None,
                level: 0,
            },
            |cell| Working {
                kind: Some(cell.kind),
                level: u16::from(cell.level),
            },
        )
    }

    /// Whether fluid of `kind` may move into `position`: open air, or the same fluid.
    fn fluid_room(&self, position: TilePosition, kind: FluidKind) -> Option<u16> {
        let tile = self.terrain.tile(position)?;
        match tile.kind {
            TileKind::Air => Some(0),
            TileKind::Fluid(other) if other == kind => Some(u16::from(tile.durability)),
            _ => None,
        }
    }

    /// The moves `position`'s fluid wants this step, read from the terrain as it stands.
    fn propose_flows(&self, position: TilePosition, step: u64, flows: &mut Vec<Flow>) {
        let Some(cell) = self.terrain.fluid_at(position) else {
            return;
        };
        let kind = cell.kind;
        let level = u16::from(cell.level);
        let mut remaining = level;
        // Fall.
        let below = offset(position, (0, 1));
        if let Some(under) = self.fluid_room(below, kind) {
            let want = stable_lower(remaining + under).saturating_sub(under);
            let amount = want.min(remaining).min(kind.fall_per_step());
            if amount > 0 {
                flows.push(Flow {
                    from: position,
                    to: below,
                    kind,
                    amount,
                });
                remaining -= amount;
            }
        }
        // Spread towards lower neighbours (from the same base, so neither side is favoured).
        let base = remaining;
        let sides = if step.is_multiple_of(2) {
            [(-1, 0), (1, 0)]
        } else {
            [(1, 0), (-1, 0)]
        };
        for side in sides {
            let next = offset(position, side);
            let Some(there) = self.fluid_room(next, kind) else {
                continue;
            };
            if base > there + 1 {
                let amount = (base - there + 1) / kind.viscosity();
                let amount = amount.min(remaining);
                if amount > 0 {
                    flows.push(Flow {
                        from: position,
                        to: next,
                        kind,
                        amount,
                    });
                    remaining -= amount;
                }
            }
        }
        // Climb when compressed past a full tile.
        if remaining > u16::from(FLUID_FULL) {
            let above = offset(position, (0, -1));
            if let Some(over) = self.fluid_room(above, kind) {
                let keep = stable_lower(remaining + over);
                let amount = remaining.saturating_sub(keep);
                if amount > 0 {
                    flows.push(Flow {
                        from: position,
                        to: above,
                        kind,
                        amount,
                    });
                }
            }
        }
    }

    /// `position`'s reactions and density swap for this step, applied in place.
    fn fluid_react(
        &mut self,
        position: TilePosition,
        step: u64,
        log: &mut StepLog,
        output: &mut TickOutput,
    ) {
        let Some(cell) = self.terrain.fluid_at(position) else {
            return;
        };
        let left = match cell.kind {
            FluidKind::Water => self.quench_lava(position, cell, log, output),
            FluidKind::LiquidMetal => self.cool_metal(position, cell, log, output),
            FluidKind::Acid if cell.level >= ACID_MIN_LEVEL => {
                self.acid_bite(position, cell, step, log, output)
            }
            FluidKind::Acid | FluidKind::Brine => Some(cell),
        };
        let Some(cell) = left else {
            return;
        };
        // A denser fluid sinks through a lighter one under it (the cells swap whole).
        let below = offset(position, (0, 1));
        if let Some(under) = self.terrain.fluid_at(below)
            && under.kind != cell.kind
            && under.kind.density() < cell.kind.density()
        {
            log.write(self, below, cell.tile());
            log.write(self, position, under.tile());
        }
    }

    /// Water beside lava sets it into obsidian and boils some water off; what water is left.
    fn quench_lava(
        &mut self,
        position: TilePosition,
        mut cell: FluidCell,
        log: &mut StepLog,
        output: &mut TickOutput,
    ) -> Option<FluidCell> {
        for side in NEIGHBOURS {
            let next = offset(position, side);
            if !self.terrain.is_lava_at(next) {
                continue;
            }
            let obsidian = TileKind::Cave(crate::cave::CaveTile::Obsidian);
            log.write(
                self,
                next,
                Tile {
                    kind: obsidian,
                    durability: crate::terrain::tile_durability(obsidian),
                },
            );
            let left = spend(cell, QUENCH_WATER_COST);
            log.write(self, position, left);
            log.react(FluidReaction::Quenched, next, output);
            cell = FluidCell::of(left)?;
        }
        Some(cell)
    }

    /// Liquid metal beside water sets (into an ore crust, or slag dust for a lone thin splash)
    /// and boils some of the water off; `None` once it has set.
    fn cool_metal(
        &mut self,
        position: TilePosition,
        cell: FluidCell,
        log: &mut StepLog,
        output: &mut TickOutput,
    ) -> Option<FluidCell> {
        for side in NEIGHBOURS {
            let next = offset(position, side);
            let Some(water) = self
                .terrain
                .fluid_at(next)
                .filter(|found| found.kind == FluidKind::Water)
            else {
                continue;
            };
            // A full-ish cell, or a thin edge of a bigger body (beside more metal or the crust
            // it already grew), sets into ore; a lone thin splash is slag dust.
            let in_body = NEIGHBOURS.iter().any(|step| {
                self.terrain
                    .tile(offset(position, *step))
                    .is_some_and(|tile| {
                        tile.kind == TileKind::Fluid(FluidKind::LiquidMetal)
                            || tile.kind == COOLED_METAL_ORE
                    })
            });
            let set = if cell.level >= METAL_ORE_LEVEL || in_body {
                Tile {
                    kind: COOLED_METAL_ORE,
                    durability: crate::terrain::tile_durability(COOLED_METAL_ORE),
                }
            } else {
                AIR
            };
            log.write(self, position, set);
            log.write(self, next, spend(water, COOL_WATER_COST));
            log.react(FluidReaction::Solidified, position, output);
            return None;
        }
        Some(cell)
    }

    /// Now and then acid eats the soft tile it rests on (or beside it) and is spent; what acid
    /// is left.
    fn acid_bite(
        &mut self,
        position: TilePosition,
        cell: FluidCell,
        step: u64,
        log: &mut StepLog,
        output: &mut TickOutput,
    ) -> Option<FluidCell> {
        if !step_roll(position, step).is_multiple_of(ACID_DISSOLVE_ODDS) {
            return Some(cell);
        }
        // Down first, then the sides (never up: acid eats what it rests on).
        for side in [(0, 1), (-1, 0), (1, 0)] {
            let next = offset(position, side);
            if self
                .terrain
                .tile(next)
                .is_some_and(|tile| acid_dissolves(tile.kind))
            {
                log.write(self, next, AIR);
                let left = spend(cell, ACID_DISSOLVE_COST);
                log.write(self, position, left);
                log.react(FluidReaction::Dissolved, next, output);
                return FluidCell::of(left);
            }
        }
        Some(cell)
    }

    // --- Bodies in fluid --------------------------------------------------------------------

    /// Buoyancy, drag, the submarine mode, pressure and acid or heat damage for `player_id`'s
    /// rig after its movement step (`before`: where it was before the step). Fires
    /// [`GameplayEventKind::RigSubmerged`] / [`GameplayEventKind::RigSurfaced`] as it goes under
    /// and comes up.
    pub(super) fn apply_fluid_to_rig(
        &mut self,
        player_id: PlayerId,
        before: (f32, f32),
        controls: (f32, bool, bool),
        output: &mut TickOutput,
    ) {
        let (horizontal, thrust, dive) = controls;
        let Some((x, y)) = self
            .players
            .get(&player_id)
            .map(|player| (player.x, player.y))
        else {
            return;
        };
        let was = self.submersion(before.0, before.1, AUTHORITATIVE_PLAYER_RADIUS);
        let now = self.submersion(x, y, AUTHORITATIVE_PLAYER_RADIUS);
        match (
            was.is_submerged(),
            now.is_submerged(),
            now.kind.or(was.kind),
        ) {
            (false, true, Some(kind)) => {
                output.push_event(GameplayEventKind::RigSubmerged { player_id, kind });
            }
            (true, false, Some(kind)) => {
                output.push_event(GameplayEventKind::RigSurfaced { player_id, kind });
            }
            _ => {}
        }
        let Some(kind) = now.kind else {
            return;
        };
        let gear = self.submersion_gear(player_id);
        let drilling = self.active_drills.contains_key(&player_id);
        let dt = self.timing.delta_seconds;
        let immune = self.damage_immune();
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let share = now.share;
        // Buoyancy carries part of the rig's weight (none while its drill bites: the drill
        // anchors it, so a rig can drill a brine floor without bobbing off it), drag slows it
        // in every direction.
        let lift = if drilling {
            0.0
        } else {
            AUTHORITATIVE_RIG_GRAVITY * kind.buoyancy() * share
        };
        let drag = 1.0 / (kind.drag() * share).mul_add(dt, 1.0);
        let mut vx = player.velocity_x * drag;
        let mut vy = (-lift).mul_add(dt, player.velocity_y) * drag;
        if now.is_submerged() {
            // Submarine mode: the drill held off the ground dives, the pumps burn a little fuel
            // while the rig steers, and nothing moves faster than a slow cruise.
            if dive && !drilling && player.fuel > 0.0 {
                vy = (SUBMARINE_DIVE_ACCELERATION * gear.handling).mul_add(dt, vy);
            }
            let top = SUBMARINE_TOP_SPEED * gear.handling;
            vx = vx.clamp(-top, top);
            vy = vy.clamp(-top, top);
            if horizontal != 0.0 || thrust || dive {
                player.fuel = SUBMARINE_FUEL_PER_SECOND.mul_add(-dt, player.fuel).max(0.0);
                player.fuel = quantize(player.fuel);
            }
        }
        player.velocity_x = quantize(vx);
        player.velocity_y = quantize(vy);
        if immune {
            return;
        }
        // Pressure past the hull's rating, acid and molten metal.
        let over = (now.depth_rows - gear.pressure_rows).max(0);
        #[allow(clippy::cast_precision_loss, reason = "a few dozen rows")]
        let mut damage = over as f32 * PRESSURE_DAMAGE_PER_ROW * dt;
        let burn = kind.hull_damage_per_second() * share * dt;
        damage += match kind {
            FluidKind::Acid => burn * (1.0 - gear.acid_resistance.clamp(0.0, 1.0)),
            FluidKind::LiquidMetal => {
                player.hazard_gear.lava_damage(burn) * (1.0 - gear.heat_resistance.clamp(0.0, 1.0))
            }
            FluidKind::Water | FluidKind::Brine => 0.0,
        };
        if damage > 0.0 && self.damage_player(player_id, damage) {
            output.push_event(GameplayEventKind::PlayerDamaged { player_id });
        }
    }

    /// Health a pilot loses this tick to the fluid it is in (acid, molten metal).
    pub(super) fn pilot_fluid_damage(&self, body: &Body, height: f32, dt: f32) -> f32 {
        let wet = self.submersion(body.x, height.mul_add(-0.5, body.y), height * 0.5);
        wet.kind
            .map_or(0.0, |kind| kind.pilot_damage_per_second() * wet.share * dt)
    }

    /// The fluid a pilot's head tile holds, when it is deep enough to swim or drown in.
    pub(crate) fn swimmable_fluid_at(&self, tile: TilePosition) -> Option<FluidKind> {
        self.terrain
            .fluid_at(tile)
            .filter(|cell| cell.level >= FLUID_FULL / 2)
            .map(|cell| cell.kind)
    }

    /// Turns a creature that cannot swim back from fluid: if its step carried its middle into
    /// fluid it was not in before, it goes back to `before` and stops.
    pub(super) fn keep_out_of_fluid(
        &self,
        kind: crate::enemy::EnemyKind,
        before: (f32, f32),
        body: &mut Body,
    ) {
        if kind.can_swim() {
            return;
        }
        let deep = |x: f32, y: f32| {
            self.terrain
                .fluid_at(self.frame().tile_at(x, y))
                .is_some_and(|cell| cell.level >= DEEP_ENOUGH)
        };
        if deep(body.x, body.y) && !deep(before.0, before.1) {
            body.x = before.0;
            body.y = before.1;
            body.velocity_x = 0.0;
            body.velocity_y = body.velocity_y.min(0.0);
        }
    }
}
