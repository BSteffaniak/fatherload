//! First-time hints for the Deep Descent's mechanics (Wave 8 onboarding).
//!
//! Each seat follows its own rig and, the first time it meets one of the Descent's systems,
//! shows a one-line tip in its own toast: a layer resource in the bay, a deep outpost, an
//! ancient lift, seals rated too shallow, a gate's crush, a rig system going on sale, the dark,
//! weak gravity, slick ground or fluid, and the depth beacons. Every hint shows once per seat
//! (split-screen seats learn separately), never while a dialog is open, never over a fresh
//! message, and at most one every [`HINT_SPACING_TICKS`] frames. Hints only run on Deep Descent
//! worlds, and a hint whose lesson the rig already shows (a Pressure Rating bought, a beacon
//! planted) is skipped. Everything reads the replicated world, so hints work the same online.

use std::collections::BTreeSet;

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::{
        PRESSURE_FUEL_PERCENT_PER_DEFICIT, RigSystem, descent_index_at_row, missing_gate,
        pressure_deficit,
    },
    item::ItemKind,
};

use super::{SeatContext, SeatUi};

/// Frames between two hints of one seat.
pub const HINT_SPACING_TICKS: u64 = 600;
/// Frames a message must have shown before a hint may replace it.
const MESSAGE_QUIET_TICKS: u64 = 300;
/// Rows below the ground line before the depth beacon hint.
const BEACON_HINT_ROWS: i32 = 160;

/// One first-time hint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Hint {
    LayerResource,
    Outpost,
    AncientLift,
    Pressure,
    GateCrush,
    SystemOnSale,
    Dark,
    LowGravity,
    Slippery,
    Fluid,
    Beacon,
}

impl Hint {
    /// Every hint, in the order they are checked (the most urgent first).
    pub const ALL: [Self; 11] = [
        Self::GateCrush,
        Self::Pressure,
        Self::SystemOnSale,
        Self::Outpost,
        Self::AncientLift,
        Self::LayerResource,
        Self::Fluid,
        Self::LowGravity,
        Self::Slippery,
        Self::Dark,
        Self::Beacon,
    ];
}

/// A seat's hints: the ones it has shown and when the last went up.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeatHints {
    pub seen: BTreeSet<Hint>,
    last_tick: Option<u64>,
}

impl SeatHints {
    /// None shown yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seen: BTreeSet::new(),
            last_tick: None,
        }
    }
}

/// The hint `player` in `world` should see now, if any (skipping `seen` ones), with its text.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one check per hint")]
pub fn due_hint(
    world: &WorldState,
    player: PlayerId,
    seen: &BTreeSet<Hint>,
) -> Option<(Hint, String)> {
    let settings = world.world_settings();
    if !settings.generator.has_descents() || !world.progression_rules().is_descent() {
        return None;
    }
    if world.is_on_foot(player) || world.logistics().is_riding(player) {
        return None;
    }
    let rig = world.player(player)?;
    let frame = world.frame();
    let tile = rig.tile_in(frame);
    let ground = fatherload_simulation::worldgen::strata::SKY_ROWS;
    if tile.y < ground + 2 {
        return None;
    }
    let descent = descent_index_at_row(settings, tile.y);
    let logistics = world.logistics();
    for hint in Hint::ALL {
        if seen.contains(&hint) {
            continue;
        }
        let text = match hint {
            Hint::GateCrush => missing_gate(rig.hazard_gear.rig_systems, descent).map(|system| {
                format!(
                    "Tip: the hull groans. This Descent needs the {}: climb back, it is on sale \
                     now at the shop and the outposts.",
                    system.name()
                )
            }),
            Hint::Pressure => {
                let deficit = pressure_deficit(rig.hazard_gear.pressure_rating, descent);
                (deficit > 0).then(|| {
                    format!(
                        "Tip: your seals are rated for Descent {}. Down here the thrusters burn \
                         {}% more fuel: outposts sell Pressure Rating tiers.",
                        fatherload_simulation::depth_progression::numeral(
                            rig.hazard_gear.pressure_rating
                        ),
                        PRESSURE_FUEL_PERCENT_PER_DEFICIT * u32::from(deficit)
                    )
                })
            }
            Hint::SystemOnSale => RigSystem::ALL
                .into_iter()
                .find(|system| {
                    logistics.felt_systems & system.bit() != 0
                        && !rig.hazard_gear.has_system(*system)
                        && world.progression().deepest_tile_reached
                            >= system.sold_from_row(&world.descent_layout())
                })
                .map(|system| {
                    format!(
                        "Tip: the {} is on sale now, in town and at the outposts.",
                        system.name()
                    )
                }),
            Hint::Outpost => logistics.outpost_at(tile).map(|outpost| {
                format!(
                    "Tip: {} refuels, repairs, buys your haul, sells deep upgrade tiers and \
                     crafts in its workshop. Press use to trade.",
                    outpost.name
                )
            }),
            Hint::AncientLift => logistics
                .station_at(tile)
                .and_then(|(shaft, _)| logistics.lifts.get(&shaft))
                .filter(|lift| lift.is_ancient())
                .map(|_| {
                    "Tip: a builders' lift. Restore it from its lowest station, then ride it \
                     between stations for no fuel."
                        .to_owned()
                }),
            Hint::LayerResource => rig.cargo_bay.iter().find_map(|(kind, _)| match kind {
                ItemKind::Layer(stratum) => Some(format!(
                    "Tip: you dug out {}, a layer resource. Outpost workshops turn layer \
                     resources into parts and supplies.",
                    stratum.def().layer_resource.name
                )),
                _ => None,
            }),
            Hint::Fluid => world.terrain().fluid_at(tile).map(|_| {
                "Tip: fluid pours into open tunnels. Climb out of it, or dig a sump below to \
                 drain it."
                    .to_owned()
            }),
            Hint::LowGravity => {
                let fields = world.fields_at(rig.x, rig.y);
                let pull = fields.gravity_x.hypot(fields.gravity_y);
                (pull < 0.7 || fields.gravity_y < 0.5).then(|| {
                    "Tip: gravity is strange here. Feather the thrusters and turn the rig to \
                     land."
                        .to_owned()
                })
            }
            Hint::Slippery => (world.fields_at(rig.x, rig.y).friction < 0.6)
                .then(|| "Tip: slick ground. Brake early: the rig slides a long way.".to_owned()),
            Hint::Dark => (world.fields_at(rig.x, rig.y).light < 0.6
                && rig.hazard_gear.headlamp == 0)
                .then(|| {
                    "Tip: the dark closes in. Headlamp tiers (and later Lamp II) light \
                         further."
                        .to_owned()
                }),
            Hint::Beacon => (tile.y - ground > BEACON_HINT_ROWS
                && logistics.beacons_of(player).next().is_none()
                && logistics.outpost_at(tile).is_none())
            .then(|| {
                "Tip: plant a depth beacon (inventory, Depth logistics) to jump back down here \
                 later."
                    .to_owned()
            }),
        };
        if let Some(text) = text {
            return Some((hint, text));
        }
    }
    None
}

impl SeatUi {
    /// Shows the seat's next first-time Deep Descent hint in its toast, when one is due and
    /// the seat is free to read it.
    pub(super) fn track_hints(&mut self, ctx: SeatContext<'_>, tick: u64) {
        let Some(player) = ctx.player else {
            return;
        };
        if self.modal.is_some() {
            return;
        }
        if self
            .hints
            .last_tick
            .is_some_and(|last| tick.saturating_sub(last) < HINT_SPACING_TICKS)
        {
            return;
        }
        if !self.message.is_empty() && tick.saturating_sub(self.message_tick) < MESSAGE_QUIET_TICKS
        {
            return;
        }
        let Some((hint, text)) = due_hint(ctx.world, player, &self.hints.seen) else {
            return;
        };
        self.hints.seen.insert(hint);
        self.hints.last_tick = Some(tick);
        self.message = text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fatherload_simulation::{
        depth_progression::{DescentLayout, ProgressionRules},
        terrain::{TILE_SIZE, TileKind, TilePosition},
        worldgen::{GeneratorVersion, WorldSettings},
    };

    fn deep_world() -> (WorldState, PlayerId) {
        let player = PlayerId::new(0);
        let settings = WorldSettings {
            generator: GeneratorVersion::V11,
            ..WorldSettings::new(0x0B0A_4D)
        };
        let world = WorldState::new_game_with_settings(
            player,
            &settings,
            fatherload_simulation::SimulationParams::new(60),
        );
        (world, player)
    }

    fn park(world: &mut WorldState, player: PlayerId, x: i32, y: i32) {
        world.terrain_mut().ensure_depth(y + 4);
        for row in y - 1..=y + 1 {
            for column in x - 1..=x + 1 {
                let kind = if row == y + 1 {
                    TileKind::Stone
                } else {
                    TileKind::Air
                };
                let _ = world
                    .terrain_mut()
                    .set_kind(TilePosition { x: column, y: row }, kind);
            }
        }
        let frame = world.frame();
        let rig = world.player_mut(player).expect("rig");
        rig.x = frame.col_px(x) + TILE_SIZE * 0.5;
        rig.y = frame.row_px(y) + TILE_SIZE * 0.5;
    }

    #[test]
    fn hints_need_a_deep_descent_world_and_show_once() {
        let player = PlayerId::new(0);
        let classic = WorldState::new_game(player);
        assert_eq!(due_hint(&classic, player, &BTreeSet::new()), None);

        let (mut world, player) = deep_world();
        assert_eq!(world.progression_rules(), ProgressionRules::Descent);
        // A layer resource in the bay, a little way down.
        let layout = world.descent_layout();
        let topsoil = layout.strata()[0];
        park(&mut world, player, 40, topsoil.top + 12);
        world.player_mut(player).expect("rig").cargo_bay.add(
            ItemKind::Layer(fatherload_simulation::descent::Stratum::Topsoil),
            1,
        );
        let (hint, text) = due_hint(&world, player, &BTreeSet::new()).expect("a hint");
        assert_eq!(hint, Hint::LayerResource);
        assert!(text.contains("Root Fibre"), "{text}");
        let seen: BTreeSet<Hint> = [Hint::LayerResource].into_iter().collect();
        assert_ne!(
            due_hint(&world, player, &seen).map(|(hint, _)| hint),
            Some(hint)
        );
    }

    #[test]
    fn deeper_than_the_seals_explains_the_fuel() {
        let (mut world, player) = deep_world();
        let layout = world.descent_layout();
        let old_deep = layout.descent(1).expect("Descent II");
        park(&mut world, player, 40, old_deep.top + 30);
        let (hint, text) = due_hint(&world, player, &BTreeSet::new()).expect("a hint");
        assert_eq!(hint, Hint::Pressure);
        assert!(text.contains("35%"), "{text}");
        world
            .player_mut(player)
            .expect("rig")
            .hazard_gear
            .pressure_rating = 1;
        assert_ne!(
            due_hint(&world, player, &BTreeSet::new()).map(|(hint, _)| hint),
            Some(Hint::Pressure)
        );
    }
}
