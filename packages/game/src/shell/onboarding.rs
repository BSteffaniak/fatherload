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
//!
//! Wave 9 adds the Drowned and Frozen's, the Hollow World's and the Strange Layers' firsts:
//! submarine water, deep cold, the inner sky, the inner sun's glare, turned gravity, mirrored
//! controls, slow time and the deep supplies. Their words name no key or button (the use and
//! thruster controls read the same on a keyboard, a pad and the touch screen).

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
    // Wave 9: Descents IV-VI.
    Submarine,
    DeepCold,
    InnerSky,
    Glare,
    TurnedGravity,
    Mirrored,
    TimeZone,
    Supplies,
}

impl Hint {
    /// Every hint, in the order they are checked (the most urgent first).
    pub const ALL: [Self; 19] = [
        Self::GateCrush,
        Self::Pressure,
        Self::SystemOnSale,
        Self::Outpost,
        Self::AncientLift,
        Self::LayerResource,
        // Wave 9: the deep rules before the general ones they refine.
        Self::DeepCold,
        Self::Glare,
        Self::Submarine,
        Self::InnerSky,
        Self::TurnedGravity,
        Self::Mirrored,
        Self::TimeZone,
        Self::Supplies,
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
        if let Some(text) = deep_hint(world, player, hint) {
            return Some((hint, text));
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
            // Wave 9: answered by `deep_hint` above.
            Hint::Submarine
            | Hint::DeepCold
            | Hint::InnerSky
            | Hint::Glare
            | Hint::TurnedGravity
            | Hint::Mirrored
            | Hint::TimeZone
            | Hint::Supplies => None,
        };
        if let Some(text) = text {
            return Some((hint, text));
        }
    }
    None
}

/// The text of Wave 9's `hint` (Descents IV-VI) for `player` in `world` now, if it is due
/// (`None` for the earlier hints, which [`due_hint`] answers itself).
#[must_use]
pub fn deep_hint(world: &WorldState, player: PlayerId, hint: Hint) -> Option<String> {
    use fatherload_simulation::{
        depth_progression::deep_climate, descent::Stratum, fluid::FluidKind, workshop::DeepSupply,
    };
    let rig = world.player(player)?;
    let tile = rig.tile_in(world.frame());
    let stratum = world.deep_stratum_at(tile)?;
    let fields = world.fields_at(rig.x, rig.y);
    match hint {
        Hint::Submarine => (stratum.descent().index() >= 3
            && matches!(
                world.terrain().fluid_at(tile).map(|cell| cell.kind),
                Some(FluidKind::Water | FluidKind::Brine)
            ))
        .then(|| {
            "Tip: under water the rig runs as a submarine. Thrust to swim; ballast tanks from the \
             deep counters sink or surface it at once."
                .to_owned()
        }),
        Hint::DeepCold => deep_climate(stratum)
            .filter(|climate| climate.cold_units > 0)
            .filter(|_| !rig.hazard_gear.has_system(RigSystem::CryoHull))
            .map(|_| {
                format!(
                    "Tip: hull frost in {}. Light a frost heater (deep counters and workshops) \
                     or warm up by a thermal vent.",
                    super::logistics::the_stratum(stratum)
                )
            }),
        Hint::InnerSky => (stratum == Stratum::InnerSky && !world.terrain().is_solid_at(tile))
            .then(|| {
                "Tip: the inner sky. Fly from stone to stone, ride the wind and keep fuel for the \
                 way back up."
                    .to_owned()
            }),
        // The sampled light stops at daylight (the glare is a screen exposure), so the inner
        // sun's glare is read from the stratum's own light, as the descent probe reads it.
        Hint::Glare => (stratum.descent().index() == 4
            && stratum.def().field.light_permille
                >= fatherload_simulation::balance::descent::mechanics::GLARE_LIGHT_PERMILLE)
            .then(|| {
            "Tip: the inner sun's glare and heat. Hop between shadows; shade canopies and Heat \
             Shielding keep the rig cool."
                .to_owned()
        }),
        Hint::TurnedGravity => (fields.gravity_y < -0.2 || fields.gravity_x.abs() > 0.5).then(|| {
            "Tip: down points another way here. Turn the rig with its rotation thrusters to land \
             on walls and ceilings."
                .to_owned()
        }),
        Hint::Mirrored => (fields.mirror > 0.5).then(|| {
            "Tip: a mirror zone. Left is right in here: the controls and the screen flip back when \
             you leave it."
                .to_owned()
        }),
        Hint::TimeZone => ((fields.time_scale - 1.0).abs() > 0.1).then(|| {
            if fields.time_scale < 1.0 {
                "Tip: time runs slow here. Everything, you too, moves slower; time-bubble bombs \
                 make pockets of it."
                    .to_owned()
            } else {
                "Tip: time runs fast here. Creatures and hazards come quicker: keep moving."
                    .to_owned()
            }
        }),
        Hint::Supplies => DeepSupply::ALL
            .into_iter()
            .find(|supply| {
                supply.stratum() == stratum && world.supplies_of(player, *supply) == 0
            })
            .map(|supply| {
                let name = supply.name();
                let article = if name.starts_with(['A', 'E', 'I', 'O', 'U']) {
                    "an"
                } else {
                    "a"
                };
                format!(
                    "Tip: {} asks for {article} {name}. Deep counters sell them; workshops make \
                     them from {}.",
                    super::logistics::the_stratum(stratum),
                    stratum.def().layer_resource.name
                )
            }),
        _ => None,
    }
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
            ..WorldSettings::new(0x00B0_A40D)
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
        rig.x = TILE_SIZE.mul_add(0.5, frame.col_px(x));
        rig.y = TILE_SIZE.mul_add(0.5, frame.row_px(y));
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
        let seen: BTreeSet<Hint> = std::iter::once(Hint::LayerResource).collect();
        assert_ne!(
            due_hint(&world, player, &seen).map(|(hint, _)| hint),
            Some(hint)
        );
    }

    #[test]
    fn descents_four_to_six_explain_their_supplies_and_cold() {
        use fatherload_simulation::descent::Stratum;
        let (mut world, player) = deep_world();
        let layout = world.descent_layout();
        let brine = layout.strata()[Stratum::BrineGlaciers.index()];
        park(&mut world, player, 40, brine.top + 40);
        {
            let rig = world.player_mut(player).expect("rig");
            rig.hazard_gear.rig_systems = !RigSystem::CryoHull.bit();
            rig.hazard_gear.pressure_rating = 7;
            rig.hazard_gear.headlamp = 7;
        }
        // Every earlier hint is learned: the frost comes first, then the heaters.
        let mut seen: BTreeSet<Hint> = Hint::ALL
            .into_iter()
            .filter(|hint| {
                !matches!(
                    hint,
                    Hint::DeepCold | Hint::Supplies | Hint::Submarine | Hint::TurnedGravity
                )
            })
            .collect();
        let (hint, text) = due_hint(&world, player, &seen).expect("a hint");
        assert_eq!(hint, Hint::DeepCold, "{text}");
        assert!(text.contains("frost heater"), "{text}");
        seen.insert(Hint::DeepCold);
        let (hint, text) = due_hint(&world, player, &seen).expect("a hint");
        assert_eq!(hint, Hint::Supplies, "{text}");
        assert!(
            text.contains("Frost Heater") && text.contains("Brine Ice"),
            "{text}"
        );
        // With the Cryo Hull the frost is no news.
        world
            .player_mut(player)
            .expect("rig")
            .hazard_gear
            .rig_systems = u8::MAX;
        assert_eq!(deep_hint(&world, player, Hint::DeepCold), None);
        // The words never name a key: they read the same by touch.
        for hint in Hint::ALL {
            if let Some(text) = deep_hint(&world, player, hint) {
                assert!(!text.contains("press") && !text.contains("click"), "{text}");
            }
        }
    }

    #[test]
    fn the_sunforge_warns_of_its_glare_and_names_supplies_with_the_right_article() {
        use fatherload_simulation::descent::Stratum;
        let (mut world, player) = deep_world();
        let layout = world.descent_layout();
        // The sampled light stops at daylight, so the glare reads the stratum's own light.
        let forge = layout.strata()[Stratum::TheSunforge.index()];
        park(&mut world, player, 40, forge.top + 40);
        let glare = deep_hint(&world, player, Hint::Glare).expect("the Sunforge's glare");
        assert!(glare.contains("glare"), "{glare}");
        let sky = layout.strata()[Stratum::InnerSky.index()];
        park(&mut world, player, 40, sky.top + 40);
        assert_eq!(deep_hint(&world, player, Hint::Glare), None);
        // "an Echo Charge", never "a Echo Charge".
        let echo = layout.strata()[Stratum::EchoCaves.index()];
        park(&mut world, player, 40, echo.top + 40);
        let supplies = deep_hint(&world, player, Hint::Supplies).expect("echo charges");
        assert!(supplies.contains("an Echo Charge"), "{supplies}");
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
