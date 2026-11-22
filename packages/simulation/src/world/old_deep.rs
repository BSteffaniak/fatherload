//! Descent II, the Old Deep, in the tick (Wave 8; rules and placement in [`crate::old_deep`]).
//!
//! [`WorldState::advance_old_deep`] runs once a tick after the logistics pass and returns at
//! once in every world without Descents, so classic worlds never change. Everything it reads is
//! a pure function of the world time (cycles), the V11 layout (set pieces) or the terrain, so a
//! client's predicted copy runs the same rules.

use crate::{
    GameplayEventKind, PlayerId,
    descent::Stratum,
    enemy::{EnemyKind, EnemyShotState, ShotKind},
    entity::{Body, EntityId, EntityState},
    event::TickOutput,
    hazard::HazardCloud,
    ids::SimulationTick,
    old_deep::{
        self, ActiveMutation, CORE_EXPOSED_MILLIS, CORE_FLARE_DAMAGE, CORE_FLARE_MILLIS,
        CORE_MAX_SPARKS, CORE_RESET_MILLIS, CORE_SPARK_MILLIS, CORE_STRIKES, CorePhase, DeepTile,
        Dose, Mutation, OldDeep, StarCoreFight, StormPhase,
        refraction::{self, Refraction},
        sites::{self, ArenaSite, CathedralSite},
    },
    quantize::quantize_simulation_scalar as quantize,
    rig::RigPartKind,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{TileRect, descent::DescentLayout},
};

use super::{WorldState, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// How close (in tiles, each axis) to the Glowmother's den a rig must be for her vanishing to
/// count as her fall: well inside the director's despawn reach, so a despawn never counts.
const GLOWMOTHER_WITNESS_TILES: i32 = 30;

/// A rig as the Old Deep's rules see it this tick.
#[derive(Clone, Copy)]
struct Rig {
    id: PlayerId,
    x: f32,
    y: f32,
    tile: TilePosition,
    stratum: Stratum,
}

impl WorldState {
    /// Descent II's live state (dose meters, mutations, digs, the Cathedral Lift, the Star
    /// Core fight).
    #[must_use]
    pub const fn old_deep(&self) -> &OldDeep {
        &self.old_deep
    }

    /// The V11 layout of this world, if it has Descents.
    fn old_deep_layout(&self) -> Option<DescentLayout> {
        let settings = self.terrain.settings();
        settings
            .generator
            .has_descents()
            .then(|| crate::descent::fields::cached_layout(settings))
    }

    /// The Cathedral Lift's nave, if this world has one.
    #[must_use]
    pub fn cathedral_site(&self) -> Option<CathedralSite> {
        sites::cathedral(self.terrain.settings(), &self.old_deep_layout()?)
    }

    /// The Star Core arena, if this world has one.
    #[must_use]
    pub fn star_core_arena(&self) -> Option<ArenaSite> {
        sites::star_core_arena(self.terrain.settings(), &self.old_deep_layout()?)
    }

    /// What `player`'s headlamp lights through the prisms around it (the renderer draws the
    /// beams and gems; a Lumen mutation lights nearby gems on its own).
    #[must_use]
    pub fn refraction_for(&self, player: PlayerId) -> Refraction {
        let Some(rig) = self.players.get(&player) else {
            return Refraction::default();
        };
        if !self.terrain.settings().generator.has_descents() {
            return Refraction::default();
        }
        let eye = self.frame().tile_at(rig.x, rig.y);
        let tile_at = |at: TilePosition| self.terrain.tile(at).map(|tile| tile.kind);
        let mut refraction = refraction::refract(tile_at, eye, refraction::LAMP_REACH_TILES);
        if self.old_deep.mutation(player, self.simulation_tick) == Some(Mutation::Lumen) {
            for gem in refraction::glow_reveals(tile_at, eye, 4) {
                if !refraction.gems.contains(&gem) {
                    refraction.gems.push(gem);
                }
            }
        }
        refraction
    }

    fn deep_kind(&self, at: TilePosition) -> Option<DeepTile> {
        match self.terrain.tile(at).map(|tile| tile.kind) {
            Some(TileKind::Deep(tile)) => Some(tile),
            _ => None,
        }
    }

    fn old_deep_rigs(&self, layout: &DescentLayout) -> Vec<Rig> {
        let frame = self.frame();
        let settings = self.terrain.settings();
        self.players
            .iter()
            .filter(|(id, _)| !self.logistics.is_riding(**id))
            .map(|(&id, player)| {
                let tile = frame.tile_at(player.x, player.y);
                Rig {
                    id,
                    x: player.x,
                    y: player.y,
                    tile,
                    stratum: crate::worldgen::descent::stratum_in(settings, layout, tile.x, tile.y),
                }
            })
            .collect()
    }

    /// The Old Deep's pass of the tick (after the logistics pass).
    pub(super) fn advance_old_deep(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(layout) = self.old_deep_layout() else {
            return;
        };
        let (top, bottom) = sites::carve_rows(&layout);
        let rigs = self.old_deep_rigs(&layout);
        let any_near = rigs
            .iter()
            .any(|rig| rig.tile.y >= top - 40 && rig.tile.y <= bottom + 40);
        let hz = self.tick_rate_hz();
        let now = self.simulation_tick;
        let second = old_deep::second_boundary(now, hz);
        // Doses drain everywhere (a rig back in town recovers), so the meters run even when
        // nobody is near.
        let in_hollow = rigs
            .iter()
            .any(|rig| rig.stratum == Stratum::RadioactiveHollow);
        if second && (in_hollow || !self.old_deep.doses.is_empty()) {
            self.advance_doses(&rigs, output);
        }
        if second {
            self.advance_mutations(&rigs, next_tick);
        }
        if !any_near {
            return;
        }
        let millis = old_deep::world_millis(now, hz);
        let next_millis = old_deep::world_millis(next_tick, hz);
        for rig in &rigs {
            self.apply_amber(rig, output);
        }
        self.advance_machines(&layout, &rigs, millis, next_millis, output);
        self.advance_jets(&rigs, millis);
        self.advance_cathedral(&rigs, next_tick, output);
        self.advance_storms(&rigs, millis, next_millis, second, output);
        self.advance_gas_tides(&rigs, millis, next_millis, second, output);
        self.advance_star_core(&layout, &rigs, next_tick, output);
        self.advance_glowmother(&layout, &rigs, output);
    }

    // --- The dose meter and mutations -------------------------------------------------------

    fn dose_gain(&self, rig: &Rig) -> u16 {
        if rig.stratum != Stratum::RadioactiveHollow {
            return 0;
        }
        let mut glow = 0u16;
        let r = old_deep::DOSE_GLOW_RADIUS;
        for dy in -r..=r {
            for dx in -r..=r {
                let at = TilePosition {
                    x: rig.tile.x + dx,
                    y: rig.tile.y + dy,
                };
                glow += match self.deep_kind(at) {
                    Some(DeepTile::GlowRock) => old_deep::DOSE_PER_GLOW,
                    Some(DeepTile::IsotopeCluster | DeepTile::MutationRelic) => {
                        old_deep::DOSE_PER_GLOW * 2
                    }
                    _ => 0,
                };
            }
        }
        let raw = old_deep::DOSE_HOLLOW_PER_SECOND + glow.min(old_deep::DOSE_GLOW_CAP);
        let gear = self.players.get(&rig.id).map(|player| player.hazard_gear);
        let pressure_hull = self
            .equipped_rig_parts
            .get(&rig.id)
            .is_some_and(|parts| parts.contains(&RigPartKind::PressureHull));
        let lead = self.old_deep.mutation(rig.id, self.simulation_tick) == Some(Mutation::LeadSkin);
        let shield = old_deep::dose_shield_percent(
            gear.map_or(0, |gear| gear.gas_scrubber),
            pressure_hull,
            lead,
        );
        u16::try_from(u32::from(raw) * (100 - shield) / 100).unwrap_or(raw)
    }

    fn advance_doses(&mut self, rigs: &[Rig], output: &mut TickOutput) {
        for rig in rigs {
            let gain = self.dose_gain(rig);
            let current = self.old_deep.dose(rig.id);
            if gain == 0 && current.level == 0 {
                continue;
            }
            let mut dose = current;
            if gain > 0 {
                dose.level = (dose.level + gain).min(old_deep::DOSE_MAX);
            } else {
                let near_outpost = self.logistics.outposts.values().any(|outpost| {
                    (outpost.column - rig.tile.x).abs() <= 20
                        && (outpost.floor_row - rig.tile.y).abs() <= 12
                });
                let decay = if near_outpost {
                    old_deep::DOSE_DECAY_PER_SECOND * 3
                } else {
                    old_deep::DOSE_DECAY_PER_SECOND
                };
                dose.level = dose.level.saturating_sub(decay);
            }
            let step = dose.step();
            if step > dose.warned {
                dose.warned = step;
                output.push_event(GameplayEventKind::DoseWarning {
                    player_id: rig.id,
                    level: step,
                });
            } else if step < dose.warned {
                dose.warned = step;
            }
            if dose.level >= old_deep::DOSE_MAX
                && self.damage_player(rig.id, old_deep::DOSE_HULL_PER_SECOND)
            {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id: rig.id });
            }
            // The sickness costs fuel: the penalty share of a second's idle burn.
            let penalty = dose.fuel_penalty();
            if penalty > 0.0
                && let Some(player) = self.players.get_mut(&rig.id)
            {
                player.fuel = quantize((player.fuel - penalty).max(0.0));
            }
            if dose.level == 0 {
                self.old_deep.doses.remove(&rig.id);
            } else {
                self.old_deep.doses.insert(rig.id, dose);
            }
        }
        // Players who left the world keep nothing (a rig riding a lift is still here: it
        // keeps its dose for when it steps off).
        let players = &self.players;
        self.old_deep.doses.retain(|id, _| players.contains_key(id));
    }

    fn advance_mutations(&mut self, rigs: &[Rig], next_tick: SimulationTick) {
        if self.old_deep.mutations.is_empty() {
            return;
        }
        self.old_deep
            .mutations
            .retain(|_, active| active.until > next_tick);
        for rig in rigs {
            match self.old_deep.mutation(rig.id, next_tick) {
                Some(Mutation::Regrowth) => {
                    if let Some(player) = self.players.get_mut(&rig.id) {
                        let max = player.max_hull();
                        player.hull =
                            quantize((player.hull + old_deep::REGROWTH_PER_SECOND).min(max));
                    }
                }
                Some(Mutation::FuelGland) => {
                    if let Some(player) = self.players.get_mut(&rig.id) {
                        player.fuel = quantize(
                            (player.fuel + old_deep::FUEL_GLAND_PER_SECOND)
                                .min(player.fuel_capacity),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // --- Amber ------------------------------------------------------------------------------

    fn apply_amber(&mut self, rig: &Rig, output: &mut TickOutput) {
        if rig.stratum != Stratum::FossilOceans {
            if self.old_deep.stuck.remove(&rig.id) {
                // Left the layer: forget the touch.
            }
            return;
        }
        let frame = self.frame();
        // Pressed against it, or nearly (a rig in a one-tile gap touches both walls).
        let reach = AUTHORITATIVE_PLAYER_RADIUS + 6.0;
        let touching = [(reach, 0.0), (-reach, 0.0), (0.0, reach), (0.0, -reach)]
            .into_iter()
            .any(|(dx, dy)| {
                self.deep_kind(frame.tile_at(rig.x + dx, rig.y + dy)) == Some(DeepTile::Amber)
            });
        if !touching {
            self.old_deep.stuck.remove(&rig.id);
            return;
        }
        if let Some(player) = self.players.get_mut(&rig.id) {
            let cap = old_deep::AMBER_SPEED_CAP;
            player.velocity_x = quantize(player.velocity_x.clamp(-cap, cap));
            player.velocity_y = quantize(player.velocity_y.clamp(-cap, cap));
        }
        if self.old_deep.stuck.insert(rig.id) {
            output.push_event(GameplayEventKind::AmberStuck { player_id: rig.id });
        }
    }

    // --- Machine halls: waking, conveyors and pistons --------------------------------------

    fn advance_machines(
        &mut self,
        layout: &DescentLayout,
        rigs: &[Rig],
        millis: u64,
        next_millis: u64,
        output: &mut TickOutput,
    ) {
        let settings = *self.terrain.settings();
        let dt = self.timing.delta_seconds;
        for rig in rigs {
            if !matches!(
                rig.stratum,
                Stratum::AncientMachineLayer
                    | Stratum::RadioactiveHollow
                    | Stratum::VoidGeodeFields
            ) {
                continue;
            }
            let area = TileRect {
                min: rig.tile,
                max: rig.tile,
            };
            let halls =
                sites::machine_halls_near(&settings, layout, &area, old_deep::MACHINE_WAKE_TILES);
            for hall in halls {
                if self.old_deep.woken_halls.insert(hall.region) {
                    output.push_event(GameplayEventKind::MachineryWoke {
                        x: hall.bounds.min.x,
                        y: hall.bounds.min.y,
                    });
                }
                self.push_by_hall(rig, &hall, millis, next_millis, dt, output);
            }
        }
    }

    fn push_by_hall(
        &mut self,
        rig: &Rig,
        hall: &sites::MachineHall,
        millis: u64,
        next_millis: u64,
        dt: f32,
        output: &mut TickOutput,
    ) {
        let frame = self.frame();
        let radius = AUTHORITATIVE_PLAYER_RADIUS;
        // Conveyors: a rig resting on a belt rides it.
        let under = frame.tile_at(rig.x, rig.y + radius + 1.0);
        let belt = match self.deep_kind(under) {
            Some(DeepTile::ConveyorLeft) => Some(-old_deep::CONVEYOR_SPEED),
            Some(DeepTile::ConveyorRight) => Some(old_deep::CONVEYOR_SPEED),
            _ => None,
        };
        if let Some(target) = belt
            && hall.bounds.contains(under)
            && let Some(player) = self.players.get_mut(&rig.id)
        {
            let step = 900.0 * dt;
            let vx = player.velocity_x;
            let pushed = if vx < target {
                (vx + step).min(target)
            } else {
                (vx - step).max(target)
            };
            player.velocity_x = quantize(pushed);
        }
        // Pistons: a ram slamming down onto a rig strikes it once per slam and shoves it down.
        let ceiling = hall.ceiling();
        let top_px = rig.y - radius;
        for x in hall.bounds.min.x + 1..hall.bounds.max.x {
            let housing = TilePosition { x, y: ceiling };
            if self.deep_kind(housing) != Some(DeepTile::PistonHousing) {
                continue;
            }
            let column_px = frame.tile_center(housing).0;
            if (rig.x - column_px).abs() > TILE_SIZE.mul_add(0.5, radius) {
                continue;
            }
            let ram_bottom = |ext: f32| ext.mul_add(TILE_SIZE, frame.row_px(ceiling + 1));
            let before = ram_bottom(old_deep::piston_extension(housing, millis));
            let after = ram_bottom(old_deep::piston_extension(housing, next_millis));
            if before <= top_px && after > top_px && rig.y > frame.row_px(ceiling + 1) {
                if self.damage_player(rig.id, old_deep::PISTON_DAMAGE) {
                    output.push_event(GameplayEventKind::PlayerDamaged { player_id: rig.id });
                }
                if let Some(player) = self.players.get_mut(&rig.id) {
                    player.velocity_y = quantize(player.velocity_y.max(320.0));
                }
                output.push_event(GameplayEventKind::PistonStruck { player_id: rig.id });
            }
        }
    }

    // --- Pressure jets and the Cathedral Lift ----------------------------------------------

    fn valves_held(&self) -> bool {
        !self.old_deep.cathedral.holds.is_empty()
    }

    fn advance_jets(&mut self, rigs: &[Rig], millis: u64) {
        let frame = self.frame();
        let dt = self.timing.delta_seconds;
        let site = self.cathedral_site();
        let calmed = self.valves_held();
        for rig in rigs {
            if !matches!(
                rig.stratum,
                Stratum::PressureCathedrals | Stratum::FossilOceans | Stratum::RadioactiveHollow
            ) {
                continue;
            }
            let mut push = false;
            'search: for dx in -1..=1 {
                for dy in 0..=old_deep::JET_REACH_ROWS {
                    let vent = TilePosition {
                        x: rig.tile.x + dx,
                        y: rig.tile.y + dy,
                    };
                    if self.deep_kind(vent) == Some(DeepTile::JetVent) {
                    } else {
                        // Rock between the rig and any vent below blocks the jet.
                        if dy > 0
                            && self
                                .terrain
                                .tile(vent)
                                .is_some_and(|tile| !tile.kind.is_open())
                        {
                            break;
                        }
                        continue;
                    }
                    let column_px = frame.tile_center(vent).0;
                    if (rig.x - column_px).abs()
                        > AUTHORITATIVE_PLAYER_RADIUS.mul_add(0.5, TILE_SIZE * 0.5)
                    {
                        break;
                    }
                    let calm =
                        calmed && site.as_ref().is_some_and(|site| site.bounds.contains(vent));
                    if !calm && old_deep::jet_active(vent, millis) {
                        push = true;
                        break 'search;
                    }
                    break;
                }
            }
            if push && let Some(player) = self.players.get_mut(&rig.id) {
                let vy = (old_deep::JET_ACCELERATION.mul_add(-dt, player.velocity_y))
                    .max(-old_deep::JET_TOP_SPEED);
                player.velocity_y = quantize(vy);
            }
        }
    }

    fn advance_cathedral(
        &mut self,
        rigs: &[Rig],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if self.old_deep.cathedral.restored && self.old_deep.cathedral.holds.is_empty() {
            return;
        }
        let Some(site) = self.cathedral_site() else {
            return;
        };
        let inside: Vec<Rig> = rigs
            .iter()
            .copied()
            .filter(|rig| site.bounds.contains(rig.tile))
            .collect();
        if inside.is_empty() && self.old_deep.cathedral.holds.is_empty() {
            return;
        }
        let frame = self.frame();
        let hold_ticks = self
            .timing
            .ticks_for_millis(old_deep::VALVE_HOLD_MILLIS)
            .max(1);
        for (index, valve) in site.valves.iter().enumerate() {
            let bit = 1u8 << index;
            let valve_index = u8::try_from(index).unwrap_or(0);
            if self.old_deep.cathedral.opened & bit != 0 {
                continue;
            }
            let (vx, vy) = frame.tile_center(*valve);
            let near = |rig: &Rig| {
                (rig.x - vx).hypot(rig.y - vy) <= TILE_SIZE.mul_add(0.5, old_deep::VALVE_REACH_PX)
            };
            let holder = self.old_deep.cathedral.holds.get(&valve_index).copied();
            match holder {
                Some((player, start)) => {
                    let still = inside.iter().any(|rig| rig.id == player && near(rig));
                    if !still {
                        self.old_deep.cathedral.holds.remove(&valve_index);
                    } else if next_tick.get().saturating_sub(start.get()) >= hold_ticks {
                        self.old_deep.cathedral.holds.remove(&valve_index);
                        self.old_deep.cathedral.opened |= bit;
                        let _ = self
                            .terrain
                            .set_kind_by_rule(*valve, TileKind::Deep(DeepTile::LiftValveOpen));
                        let opened =
                            u8::try_from(self.old_deep.cathedral.opened.count_ones()).unwrap_or(0);
                        output.push_event(GameplayEventKind::CathedralValveOpened {
                            player_id: player,
                            valve: valve_index,
                            opened,
                        });
                        if opened >= 3 {
                            self.restore_cathedral_lift(&site, player, output);
                        }
                    }
                }
                None => {
                    if let Some(rig) = inside.iter().find(|rig| near(rig)) {
                        self.old_deep
                            .cathedral
                            .holds
                            .insert(valve_index, (rig.id, self.simulation_tick));
                    }
                }
            }
        }
    }

    fn restore_cathedral_lift(
        &mut self,
        site: &CathedralSite,
        player: PlayerId,
        output: &mut TickOutput,
    ) {
        self.old_deep.cathedral.restored = true;
        self.old_deep.cathedral.holds.clear();
        if let Some(lift) = self
            .logistics
            .lifts
            .values_mut()
            .find(|lift| lift.column == site.lift_column)
        {
            lift.restored = true;
        }
        output.push_event(GameplayEventKind::CathedralLiftRestored { player_id: player });
    }

    // --- Heat storms ------------------------------------------------------------------------

    /// Whether a rig at `tile` stands in open cave air the storm can reach.
    fn storm_exposed(&self, tile: TilePosition) -> bool {
        let mut open = 0;
        for dy in -3..=3 {
            for dx in -3..=3 {
                if self
                    .terrain
                    .tile(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    })
                    .is_some_and(|t| t.kind.is_open())
                {
                    open += 1;
                }
            }
        }
        open >= old_deep::STORM_EXPOSED_OPEN_TILES
    }

    fn advance_storms(
        &mut self,
        rigs: &[Rig],
        millis: u64,
        next_millis: u64,
        second: bool,
        output: &mut TickOutput,
    ) {
        let in_zone: Vec<Rig> = rigs
            .iter()
            .copied()
            .filter(|rig| rig.stratum == Stratum::MantleStormZone)
            .collect();
        if in_zone.is_empty() {
            return;
        }
        let phase = old_deep::storm_phase(next_millis);
        if old_deep::storm_phase(millis) != phase {
            output.push_event(GameplayEventKind::HeatStormChanged { phase });
        }
        if phase != StormPhase::Raging {
            return;
        }
        let dt = self.timing.delta_seconds;
        for rig in &in_zone {
            if !self.storm_exposed(rig.tile) {
                continue;
            }
            let thermal = self
                .equipped_rig_parts
                .get(&rig.id)
                .is_some_and(|parts| parts.contains(&RigPartKind::ThermalHull));
            let shield = self
                .players
                .get(&rig.id)
                .map_or(0, |player| player.hazard_gear.heat_shield);
            let scale =
                (if thermal { 0.5 } else { 1.0 }) * 0.12f32.mul_add(-f32::from(shield.min(5)), 1.0);
            if self.damage_player(rig.id, old_deep::STORM_DAMAGE_PER_SECOND * scale * dt) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id: rig.id });
            }
        }
        if !second {
            return;
        }
        // Storm vents near the rigs pour molten metal (a few cells each storm).
        let storm = old_deep::storm_index(next_millis);
        if self.old_deep.pours.0 != storm {
            self.old_deep.pours = (storm, std::collections::BTreeMap::new());
        }
        let mut poured = Vec::new();
        for rig in &in_zone {
            for dy in -16..=16 {
                for dx in -20..=20 {
                    let vent = TilePosition {
                        x: rig.tile.x + dx,
                        y: rig.tile.y + dy,
                    };
                    if self.deep_kind(vent) == Some(DeepTile::StormVent) && !poured.contains(&vent)
                    {
                        poured.push(vent);
                    }
                }
            }
        }
        for vent in poured {
            let count = self
                .old_deep
                .pours
                .1
                .get(&(vent.x, vent.y))
                .copied()
                .unwrap_or(0);
            if count >= old_deep::STORM_VENT_POURS {
                continue;
            }
            let mouth = TilePosition {
                x: vent.x,
                y: vent.y - 1,
            };
            if self.terrain.tile(mouth).map(|tile| tile.kind) != Some(TileKind::Air) {
                continue;
            }
            let _ = self
                .terrain
                .set_kind(mouth, TileKind::Fluid(crate::fluid::FluidKind::LiquidMetal));
            self.wake_fluids(TileRect {
                min: TilePosition {
                    x: mouth.x - 1,
                    y: mouth.y - 1,
                },
                max: TilePosition {
                    x: mouth.x + 1,
                    y: mouth.y + 1,
                },
            });
            self.old_deep.pours.1.insert((vent.x, vent.y), count + 1);
            output.push_event(GameplayEventKind::LavaPoured {
                x: mouth.x,
                y: mouth.y,
            });
        }
    }

    // --- Gas tides --------------------------------------------------------------------------

    fn advance_gas_tides(
        &mut self,
        rigs: &[Rig],
        millis: u64,
        next_millis: u64,
        second: bool,
        output: &mut TickOutput,
    ) {
        let in_oceans: Vec<Rig> = rigs
            .iter()
            .copied()
            .filter(|rig| rig.stratum == Stratum::FossilOceans)
            .collect();
        if in_oceans.is_empty() {
            return;
        }
        let high = old_deep::gas_tide_high(next_millis);
        if old_deep::gas_tide_high(millis) != high {
            output.push_event(GameplayEventKind::GasTideTurned { high });
        }
        if !high || !second {
            return;
        }
        let tide = next_millis / old_deep::GAS_TIDE_MILLIS;
        if self.old_deep.breathed.0 != tide {
            self.old_deep.breathed = (tide, std::collections::BTreeSet::new());
        }
        let frame = self.frame();
        for rig in &in_oceans {
            let mut breaths = 0;
            for dy in -10..=10 {
                for dx in -12..=12 {
                    if breaths >= 2 {
                        break;
                    }
                    let pocket = TilePosition {
                        x: rig.tile.x + dx,
                        y: rig.tile.y + dy,
                    };
                    let above = TilePosition {
                        x: pocket.x,
                        y: pocket.y - 1,
                    };
                    if self.terrain.tile(pocket).map(|t| t.kind) != Some(TileKind::Gas)
                        || self.terrain.tile(above).map(|t| t.kind) != Some(TileKind::Air)
                        || self.old_deep.breathed.1.contains(&(pocket.x, pocket.y))
                    {
                        continue;
                    }
                    self.old_deep.breathed.1.insert((pocket.x, pocket.y));
                    let (x, y) = frame.tile_center(above);
                    self.spawn_gas_cloud(HazardCloud {
                        x,
                        y,
                        life: 7.0,
                        radius: 14.0,
                    });
                    breaths += 1;
                }
            }
            if breaths > 0 {
                output.push_event(GameplayEventKind::HazardChanged);
            }
        }
    }

    // --- The Star Core ----------------------------------------------------------------------

    fn restore_pylons(&mut self, arena: &ArenaSite) {
        for pylon in arena.pylons {
            if self.deep_kind(pylon) != Some(DeepTile::CorePylon) {
                let _ = self
                    .terrain
                    .set_kind_by_rule(pylon, TileKind::Deep(DeepTile::CorePylon));
            }
        }
    }

    fn pylons_standing(&self, arena: &ArenaSite) -> usize {
        arena
            .pylons
            .iter()
            .filter(|&&pylon| self.deep_kind(pylon) == Some(DeepTile::CorePylon))
            .count()
    }

    fn after_millis(&self, from: SimulationTick, millis: u64) -> SimulationTick {
        SimulationTick::new(
            from.get()
                .saturating_add(self.timing.ticks_for_millis(millis).max(1)),
        )
    }

    fn fire_core_flares(
        &mut self,
        arena: &ArenaSite,
        target: (f32, f32),
        count: u8,
        next_tick: SimulationTick,
    ) {
        let (cx, cy) = self.frame().tile_center(arena.core);
        let (dx, dy) = (target.0 - cx, target.1 - cy);
        let distance = dx.hypot(dy).max(1.0);
        let speed = ShotKind::Fireball.speed();
        let lifetime = self
            .timing
            .ticks_for_millis(ShotKind::Fireball.life_millis())
            .max(1);
        let expires = SimulationTick::new(next_tick.get().saturating_add(lifetime));
        // A fan: straight at the rig, then pairs either side (about 14 degrees apart).
        let spreads: &[f32] = match count {
            0 | 1 => &[0.0],
            2 => &[-0.12, 0.12],
            _ => &[-0.25, 0.0, 0.25],
        };
        for spread in spreads {
            let (nx, ny) = (dx / distance, dy / distance);
            let (rx, ry) = (nx - ny * spread, ny + nx * spread);
            let norm = rx.hypot(ry).max(1e-3);
            let (vx, vy) = (rx / norm * speed, ry / norm * speed);
            let muzzle = TILE_SIZE * 0.9;
            let _ = self.spawn_entity(
                Body::at(
                    quantize((rx / norm).mul_add(muzzle, cx)),
                    quantize((ry / norm).mul_add(muzzle, cy)),
                )
                .moving(quantize(vx), quantize(vy)),
                EntityState::EnemyShot(EnemyShotState {
                    kind: ShotKind::Fireball,
                    damage: CORE_FLARE_DAMAGE,
                    shooter: EnemyKind::MagmaWisp,
                }),
                Some(expires),
            );
        }
    }

    #[allow(clippy::too_many_lines, reason = "the fight's phases, one block each")]
    fn advance_star_core(
        &mut self,
        layout: &DescentLayout,
        rigs: &[Rig],
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let phase = self.old_deep.core.phase;
        if phase == CorePhase::Beaten {
            return;
        }
        let Some(arena) = sites::star_core_arena(self.terrain.settings(), layout) else {
            return;
        };
        if phase == CorePhase::Dormant
            && !rigs
                .iter()
                .any(|rig| (rig.tile.y - arena.floor()).abs() < 40)
        {
            return;
        }
        let inside: Vec<Rig> = rigs
            .iter()
            .copied()
            .filter(|rig| arena.inside(rig.tile))
            .collect();
        let now = self.simulation_tick;
        match phase {
            CorePhase::Dormant => {
                if inside.is_empty() {
                    return;
                }
                self.restore_pylons(&arena);
                self.old_deep.core = StarCoreFight {
                    phase: CorePhase::Shielded,
                    strikes_left: CORE_STRIKES,
                    exposed_until: now,
                    next_flare: self.after_millis(now, 2_000),
                    next_spark: self.after_millis(now, 5_000),
                    last_seen: now,
                    beaten_by: None,
                };
                output.push_event(GameplayEventKind::StarCoreAwoke);
                output.push_event(GameplayEventKind::StarCorePhase {
                    phase: CorePhase::Shielded,
                    strikes_left: CORE_STRIKES,
                });
                return;
            }
            CorePhase::Beaten => return,
            CorePhase::Shielded | CorePhase::Exposed => {}
        }
        if inside.is_empty() {
            let idle = self.timing.ticks_for_millis(CORE_RESET_MILLIS);
            if next_tick
                .get()
                .saturating_sub(self.old_deep.core.last_seen.get())
                >= idle
            {
                // Nobody stayed: the core goes back to sleep, whole (a fair retry).
                self.restore_pylons(&arena);
                let _ = self
                    .terrain
                    .set_kind_by_rule(arena.core, TileKind::Deep(DeepTile::FirstCoreShielded));
                self.old_deep.core = StarCoreFight::default();
                output.push_event(GameplayEventKind::StarCorePhase {
                    phase: CorePhase::Dormant,
                    strikes_left: CORE_STRIKES,
                });
            }
            return;
        }
        self.old_deep.core.last_seen = now;
        let (cx, cy) = self.frame().tile_center(arena.core);
        // Flares at the nearest rig inside, faster and wider as the core weakens.
        if now >= self.old_deep.core.next_flare {
            let target = inside
                .iter()
                .min_by(|a, b| {
                    let da = (a.x - cx).hypot(a.y - cy);
                    let db = (b.x - cx).hypot(b.y - cy);
                    da.total_cmp(&db).then(a.id.cmp(&b.id))
                })
                .map(|rig| (rig.x, rig.y));
            let strikes_left = self.old_deep.core.strikes_left.max(1);
            let count = 1 + CORE_STRIKES.saturating_sub(strikes_left);
            if let Some(target) = target {
                self.fire_core_flares(&arena, target, count, next_tick);
            }
            let millis =
                CORE_FLARE_MILLIS * u64::from(strikes_left + 2) / u64::from(CORE_STRIKES + 2);
            self.old_deep.core.next_flare = self.after_millis(now, millis);
        }
        match self.old_deep.core.phase {
            CorePhase::Shielded => {
                if now >= self.old_deep.core.next_spark {
                    let sparks = self
                        .enemies()
                        .filter(|(entity, state)| {
                            state.kind == EnemyKind::MagmaWisp
                                && (entity.body.x - cx).hypot(entity.body.y - cy) < TILE_SIZE * 30.0
                        })
                        .count();
                    if sparks < CORE_MAX_SPARKS {
                        let tier = Stratum::MantleStormZone.enemy_tier();
                        let _ = self.spawn_enemy_with(
                            EnemyKind::MagmaWisp,
                            (cx, TILE_SIZE.mul_add(-2.0, cy)),
                            tier,
                            None,
                            None,
                        );
                    }
                    self.old_deep.core.next_spark = self.after_millis(now, CORE_SPARK_MILLIS);
                }
                if self.pylons_standing(&arena) == 0 {
                    let _ = self
                        .terrain
                        .set_kind_by_rule(arena.core, TileKind::Deep(DeepTile::FirstCore));
                    self.old_deep.core.phase = CorePhase::Exposed;
                    self.old_deep.core.exposed_until = self.after_millis(now, CORE_EXPOSED_MILLIS);
                    output.push_event(GameplayEventKind::StarCorePhase {
                        phase: CorePhase::Exposed,
                        strikes_left: self.old_deep.core.strikes_left,
                    });
                }
            }
            CorePhase::Exposed if now >= self.old_deep.core.exposed_until => {
                self.shield_core(&arena, output);
            }
            _ => {}
        }
    }

    fn shield_core(&mut self, arena: &ArenaSite, output: &mut TickOutput) {
        let _ = self
            .terrain
            .set_kind_by_rule(arena.core, TileKind::Deep(DeepTile::FirstCoreShielded));
        self.restore_pylons(arena);
        self.old_deep.core.phase = CorePhase::Shielded;
        output.push_event(GameplayEventKind::StarCorePhase {
            phase: CorePhase::Shielded,
            strikes_left: self.old_deep.core.strikes_left,
        });
    }

    /// A drill struck the open core (it broke the `FirstCore` tile at `target`).
    fn strike_core(&mut self, player_id: PlayerId, target: TilePosition, output: &mut TickOutput) {
        let Some(arena) = self.star_core_arena() else {
            return;
        };
        if target != arena.core || self.old_deep.core.phase != CorePhase::Exposed {
            return;
        }
        let left = self.old_deep.core.strikes_left.saturating_sub(1);
        self.old_deep.core.strikes_left = left;
        if left > 0 {
            self.shield_core(&arena, output);
            return;
        }
        // Spent: the first core goes dark and the floor opens on the way below.
        let _ = self
            .terrain
            .set_kind_by_rule(arena.core, TileKind::Deep(DeepTile::FirstCoreSpent));
        for tile in arena.breach() {
            let _ = self
                .terrain
                .set_kind_by_rule(tile, TileKind::Deep(DeepTile::CinderRock));
        }
        self.old_deep.core.phase = CorePhase::Beaten;
        self.old_deep.core.beaten_by = Some(player_id);
        // The Pressure Hull is the crew's: the builders' seal fits every rig.
        for player in self.players.values_mut() {
            crate::economy::grant_rig_system(
                player,
                crate::depth_progression::RigSystem::PressureHull,
            );
        }
        self.old_deep.logs_read.insert(old_deep::LOG_CORE_REVEAL);
        output.push_event(GameplayEventKind::StarCorePhase {
            phase: CorePhase::Beaten,
            strikes_left: 0,
        });
        output.push_event(GameplayEventKind::StarCoreBeaten { player_id });
        output.push_event(GameplayEventKind::DeepLogRead {
            player_id,
            log: old_deep::LOG_CORE_REVEAL,
        });
    }

    // --- The Glowmother ---------------------------------------------------------------------

    fn advance_glowmother(
        &mut self,
        layout: &DescentLayout,
        rigs: &[Rig],
        output: &mut TickOutput,
    ) {
        if self.old_deep.glowmother.defeated {
            return;
        }
        let Some((centre, (half_w, half_h))) =
            sites::glowmother_den(self.terrain.settings(), layout)
        else {
            return;
        };
        let near: Vec<&Rig> = rigs
            .iter()
            .filter(|rig| {
                (rig.tile.x - centre.x).abs() <= half_w + 6
                    && (rig.tile.y - centre.y).abs() <= half_h + 6
            })
            .collect();
        if let Some(id) = self.old_deep.glowmother.entity {
            let alive = self.entities.get(EntityId(id)).is_some_and(|entity| {
                matches!(entity.state, EntityState::Enemy(state) if state.kind == EnemyKind::Glowmother)
            });
            if alive {
                return;
            }
            self.old_deep.glowmother.entity = None;
            // Gone with a rig still well inside the despawn reach: she fell (a bomb or a long
            // chase can finish her from beyond the den's mouth).
            let witness = rigs.iter().find(|rig| {
                (rig.tile.x - centre.x).abs() <= GLOWMOTHER_WITNESS_TILES
                    && (rig.tile.y - centre.y).abs() <= GLOWMOTHER_WITNESS_TILES
            });
            if let Some(rig) = witness {
                self.old_deep.glowmother.defeated = true;
                output.push_event(GameplayEventKind::GlowmotherDefeated { player_id: rig.id });
            }
        } else {
            if near.is_empty() || self.enemies.difficulty == crate::enemy::EnemyDifficulty::Peaceful
            {
                return;
            }
            let (x, y) = self.frame().tile_center(TilePosition {
                x: centre.x,
                y: centre.y + half_h - 3,
            });
            let tier = Stratum::RadioactiveHollow.enemy_tier();
            let id = self.spawn_enemy_with(EnemyKind::Glowmother, (x, y), tier, None, None);
            self.old_deep.glowmother.entity = Some(id.get());
            output.push_event(GameplayEventKind::GlowmotherAwoke {
                x: centre.x,
                y: centre.y,
            });
        }
    }

    // --- Mined tiles ------------------------------------------------------------------------

    /// Whether the drill drop of a just-broken `kind` at `target` should spawn (a crumbled
    /// fossil bone leaves nothing).
    pub(super) fn deep_drop_allowed(&self, target: TilePosition, kind: TileKind) -> bool {
        match kind {
            TileKind::Deep(DeepTile::FossilBone) => self.bone_whole(target),
            _ => true,
        }
    }

    /// Whether the bone just taken from `target` came out whole: at most one of its sides still
    /// touched rock (bones of its skeleton do not count).
    fn bone_whole(&self, target: TilePosition) -> bool {
        let solid = [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .into_iter()
            .filter(|(dx, dy)| {
                self.terrain
                    .tile(TilePosition {
                        x: target.x + dx,
                        y: target.y + dy,
                    })
                    .is_some_and(|tile| {
                        !tile.kind.is_open() && tile.kind != TileKind::Deep(DeepTile::FossilBone)
                    })
            })
            .count();
        old_deep::bone_exposed(solid)
    }

    /// A drill broke an Old Deep tile.
    pub(super) fn mined_deep_tile(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: DeepTile,
        output: &mut TickOutput,
    ) {
        match tile {
            DeepTile::ResonantCrystal => self.ring_crystal(player_id, target, output),
            DeepTile::FossilBone => self.free_bone(player_id, target, output),
            DeepTile::MutationRelic => {
                let mutation = Mutation::of_relic(self.terrain.seed(), target);
                let until = self.after_millis(self.simulation_tick, old_deep::MUTATION_MILLIS);
                self.old_deep
                    .mutations
                    .insert(player_id, ActiveMutation { mutation, until });
                let mut dose = self.old_deep.dose(player_id);
                if mutation != Mutation::LeadSkin {
                    dose.level = (dose.level + old_deep::MUTATION_DOSE).min(old_deep::DOSE_MAX);
                    self.old_deep.doses.insert(player_id, dose);
                }
                output.push_event(GameplayEventKind::MutationGained {
                    player_id,
                    mutation,
                });
            }
            DeepTile::BuilderTablet => {
                let pick =
                    crate::worldgen::noise::hash2(self.terrain.seed(), 0x7AB1, target.x, target.y);
                let log = old_deep::LOG_TABLET_FIRST
                    + u8::try_from(pick % u64::from(old_deep::LOG_TABLETS)).unwrap_or(0);
                self.read_log(player_id, log, output);
            }
            DeepTile::ExpeditionLog => {
                if let Some(layout) = self.old_deep_layout() {
                    let stratum = layout.stratum_of_row(target.y);
                    let log = sites::camp_log(stratum);
                    self.read_log(player_id, log, output);
                }
            }
            DeepTile::FirstCore => self.strike_core(player_id, target, output),
            _ => {}
        }
    }

    fn read_log(&mut self, player_id: PlayerId, log: u8, output: &mut TickOutput) {
        self.old_deep.logs_read.insert(log);
        output.push_event(GameplayEventKind::DeepLogRead { player_id, log });
    }

    fn free_bone(&mut self, player_id: PlayerId, target: TilePosition, output: &mut TickOutput) {
        let Some(layout) = self.old_deep_layout() else {
            return;
        };
        let intact = self.bone_whole(target);
        output.push_event(GameplayEventKind::FossilBoneFreed {
            player_id,
            x: target.x,
            y: target.y,
            intact,
        });
        let Some(skeleton) = sites::skeleton_of(self.terrain.settings(), &layout, target) else {
            return;
        };
        let dig = self.old_deep.skeletons.entry(skeleton.region).or_default();
        if intact {
            dig.freed = dig.freed.saturating_add(1);
        } else {
            dig.broken = dig.broken.saturating_add(1);
        }
        let bones = u8::try_from(skeleton.bones.len()).unwrap_or(u8::MAX);
        if !dig.completed && dig.broken == 0 && dig.freed >= bones {
            dig.completed = true;
            let bounty = old_deep::SKELETON_BOUNTY_PER_BONE * u32::from(bones);
            if let Some(player) = self.players.get_mut(&player_id) {
                player.credits = player.credits.saturating_add(bounty);
            }
            output.push_event(GameplayEventKind::SkeletonCompleted {
                player_id,
                bones,
                bounty,
            });
        }
    }

    /// Resonance: the crystal around a drilled crystal rings and shatters, as far as the
    /// drill's speed carries it. A Resonance Drill catches the shards; without one a big chain
    /// shakes the ceiling.
    fn ring_crystal(&mut self, player_id: PlayerId, target: TilePosition, output: &mut TickOutput) {
        let (strength, harvest) = self.players.get(&player_id).map_or((0, false), |player| {
            (
                player.drill_strength,
                self.equipped_rig_parts
                    .get(&player_id)
                    .is_some_and(|parts| parts.contains(&RigPartKind::ResonanceDrill)),
            )
        });
        let reach = old_deep::resonance_reach(strength);
        let mut shattered = Vec::new();
        let mut frontier = vec![target];
        while let Some(at) = frontier.pop() {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let next = TilePosition {
                        x: at.x + dx,
                        y: at.y + dy,
                    };
                    if (next.x - target.x).abs() > reach
                        || (next.y - target.y).abs() > reach
                        || shattered.contains(&next)
                        || shattered.len() >= old_deep::RESONANCE_MAX_SHATTER
                    {
                        continue;
                    }
                    if self.deep_kind(next) == Some(DeepTile::ResonantCrystal) {
                        shattered.push(next);
                        frontier.push(next);
                    }
                }
            }
        }
        if shattered.is_empty() {
            return;
        }
        for &at in &shattered {
            let _ = self.terrain.set_kind(at, TileKind::Air);
            if harvest {
                let _ =
                    self.spawn_tile_drop(at, TileKind::Deep(DeepTile::ResonantCrystal), (0.0, 0.0));
            }
        }
        if !harvest && shattered.len() >= old_deep::RESONANCE_CAVE_IN_AT {
            let (x, y) = self.frame().tile_center(target);
            if self.try_cave_in(x, y) {
                output.push_event(GameplayEventKind::BouldersChanged);
            }
        }
        output.push_event(GameplayEventKind::ResonanceChain {
            player_id,
            x: target.x,
            y: target.y,
            shattered: u8::try_from(shattered.len()).unwrap_or(u8::MAX),
            harvested: harvest,
        });
    }

    /// A glow grub's bite: it raises the bitten rig's dose (Radioactive Hollow).
    pub(super) fn glow_grub_bite(&mut self, player: PlayerId) {
        if !self.terrain.settings().generator.has_descents() {
            return;
        }
        let mut dose = self.old_deep.dose(player);
        dose.level = (dose.level + old_deep::GLOW_GRUB_BITE_DOSE).min(old_deep::DOSE_MAX);
        self.old_deep.doses.insert(player, dose);
    }

    /// Staging for tests and scenes: `player`'s dose meter.
    #[doc(hidden)]
    pub fn set_dose_for_tests(&mut self, player: PlayerId, level: u16) {
        self.old_deep.doses.insert(
            player,
            Dose {
                level,
                warned: Dose { level, warned: 0 }.step(),
            },
        );
    }

    /// Staging for tests and scenes: a mutation on `player`.
    #[doc(hidden)]
    pub fn set_mutation_for_tests(&mut self, player: PlayerId, mutation: Mutation) {
        let until = self.after_millis(self.simulation_tick, old_deep::MUTATION_MILLIS);
        self.old_deep
            .mutations
            .insert(player, ActiveMutation { mutation, until });
    }
}
