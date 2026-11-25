//! Descent VIII, the Heart's scenes (Wave 10, a Deep Descent world): every stratum with its
//! creatures, a rig falling up with too much dark matter, a gravity well bending the view
//! (lensing), the metal sea's rails with a switch and a booster, the Ferry Terminal, the
//! Heart's three phases, its awakening and the cable lighting up (from the Heart, the surface
//! and the Moon), the ending's cutaway, credits and epilogue, and the open stars over the Moon.

#![allow(
    clippy::suboptimal_flops,
    clippy::missing_const_for_fn,
    reason = "scene staging: plain arithmetic on tile positions"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::ProgressionRules,
    descent::Stratum,
    enemy::EnemyKind,
    heart::{FLOOR_BELOW_TILES, HeartPhase, WEIGHTLESS_LUMPS},
    item::ItemKind,
    terrain::{Terrain, TilePosition},
    worldgen::{
        GeneratorVersion, TileRect, WorldSettings,
        descent::{self, heart as places, landmarks},
    },
};

use super::{Scene, SceneStage, place_rig, reveal};

/// Seed of every Heart scene world (the Deep Descent scenes' seed).
const SCENE_SEED: u64 = 0x0D3E_9DE5;

const fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SCENE_SEED)
    }
}

fn deep_world(world: &mut WorldState) {
    *world.terrain_mut() = Terrain::generate(&settings());
    world.set_enemy_difficulty(fatherload_simulation::enemy::EnemyDifficulty::Normal);
    world.set_progression_rules(ProgressionRules::Descent);
    world.install_logistics_sites(&landmarks::logistics_sites(&settings()), false);
    world.grant_deep_claim();
}

/// A Descent VIII rig: every protection and every gate system (the Heart Key too).
fn fit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        fatherload_simulation::economy::max_out_upgrades(rig);
        rig.hazard_gear = fatherload_simulation::protection::HazardGear::MAXED;
        rig.hull = rig.max_hull();
        rig.fuel = rig.fuel_capacity * 0.8;
        rig.credits = 120_000;
    }
}

fn spawn(world: &mut WorldState, kind: EnemyKind, x: i32, y: i32) {
    let (px, py) = world.frame().tile_center(TilePosition { x, y });
    let _ = world.spawn_enemy(kind, px, py);
}

/// The rig at tile `(x, y)`, the ground round it explored.
fn stage_at(world: &mut WorldState, player: PlayerId, (x, y): (i32, i32)) {
    deep_world(world);
    fit(world, player);
    place_rig(world, player, x, y);
    world.stream_terrain_near_players();
    reveal(world, player, x - 64..x + 64, y - 40..y + 40);
}

/// The middle row of `stratum` on the scene world.
fn middle(stratum: Stratum) -> i32 {
    let (top, bottom) = descent::layout(&settings()).rows(stratum);
    top.midpoint(bottom)
}

/// A weightless hall near the cable, its middle and size.
fn a_hall() -> places::Hall {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(Stratum::DarkMatterVeins);
    let column = landmarks::lift_column(&settings);
    let area = TileRect {
        min: TilePosition {
            x: column - 400,
            y: top,
        },
        max: TilePosition {
            x: column + 400,
            y: bottom,
        },
    };
    places::halls_touching(&settings, &layout, &area)
        .into_iter()
        .max_by_key(|hall| hall.half_width)
        .unwrap_or_else(|| places::Hall {
            centre: TilePosition {
                x: column,
                y: middle(Stratum::DarkMatterVeins),
            },
            half_width: 14,
            half_height: 7,
        })
}

/// Dark Matter Veins: a weightless hall, dark matter on its roof, null moths, an upfall
/// beetle hanging, the rig carrying a little dark matter.
fn dark_matter_veins(stage: &mut SceneStage<'_>, falling_up: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let hall = a_hall();
        let floor = hall.centre.y + hall.half_height - 1;
        stage_at(world, player, (hall.centre.x - 4, floor - 1));
        if let Some(rig) = world.player_mut(player) {
            let lumps = if falling_up { WEIGHTLESS_LUMPS + 4 } else { 3 };
            rig.cargo_bay
                .add(ItemKind::Layer(Stratum::DarkMatterVeins), lumps);
        }
        if !falling_up {
            spawn(
                world,
                EnemyKind::NullMoth,
                hall.centre.x + 6,
                hall.centre.y - 2,
            );
            spawn(world, EnemyKind::NullMoth, hall.centre.x + 9, hall.centre.y);
            spawn(
                world,
                EnemyKind::UpfallBeetle,
                hall.centre.x - 9,
                hall.centre.y - hall.half_height + 2,
            );
        }
    });
    finish(stage, if falling_up { 50 } else { 90 });
}

/// The Upfall: the shaft whose roof is crusted with dark matter, the hauler stuck to it.
fn upfall(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let shaft = places::upfall(&settings, &layout);
        stage_at(world, player, (shaft.center().x + 5, shaft.min.y + 10));
        if let Some(rig) = world.player_mut(player) {
            // Weightless: it hangs under the roof to look.
            rig.cargo_bay
                .add(ItemKind::Layer(Stratum::DarkMatterVeins), WEIGHTLESS_LUMPS);
        }
    });
    finish(stage, 40);
}

/// A well near the cable on the scene world.
fn a_well() -> places::Well {
    let settings = settings();
    let layout = descent::layout(&settings);
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    let column = landmarks::lift_column(&settings);
    let area = TileRect {
        min: TilePosition {
            x: column - 300,
            y: top,
        },
        max: TilePosition {
            x: column + 300,
            y: bottom,
        },
    };
    places::wells_touching(&settings, &layout, &area)
        .into_iter()
        .find(|well| well.radius >= 9 && well.radius < places::LENS_RADIUS)
        .unwrap_or_else(|| places::lens(&settings, &layout))
}

/// The Singularity Fields: a gravity well bending the light, orbiters, motes, a lurker.
fn singularity_fields(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let well = a_well();
        let c = well.centre;
        stage_at(world, player, (c.x - well.radius + 3, c.y + 2));
        spawn(world, EnemyKind::Orbiter, c.x + well.radius - 3, c.y - 2);
        spawn(world, EnemyKind::AccretionMote, c.x + 3, c.y + 1);
        spawn(world, EnemyKind::AccretionMote, c.x - 3, c.y - 1);
        spawn(world, EnemyKind::AccretionMote, c.x + 1, c.y - 3);
    });
    finish(stage, 30);
}

/// The Lens: the greatest well.
fn lens(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let lens = places::lens(&settings, &layout);
        stage_at(
            world,
            player,
            (lens.centre.x + lens.radius - 4, lens.centre.y + 4),
        );
    });
    finish(stage, 30);
}

/// The Core Ocean: the rig riding the upper rail over the metal sea, an eel below, a leech on
/// the rail ahead, a skimmer over the metal.
fn core_ocean(stage: &mut SceneStage<'_>, at_switch: bool) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let (upper, _) = places::rail_rows(&settings, &layout);
        let basin = places::basin_at(&settings, landmarks::lift_column(&settings) + 400);
        let x = if at_switch {
            basin.junctions.0 - 6
        } else {
            basin.start + basin.pillar + 30
        };
        stage_at(world, player, (x, upper - 1));
        let (_, sea, _) = places::ocean_rows(&settings, &layout, x);
        spawn(world, EnemyKind::MercuryEel, x + 8, sea + 3);
        spawn(world, EnemyKind::RailLeech, x + 14, upper - 1);
        spawn(world, EnemyKind::SlagSkimmer, x - 10, sea - 3);
    });
    // Hold the stick along the line: the rig hooks onto the rail and rides.
    stage.hold(
        if at_switch { 20 } else { 40 },
        crate::input::PlayerInput {
            horizontal: 1.0,
            ..crate::input::PlayerInput::default()
        },
    );
    finish(stage, 4);
}

/// The Ferry Terminal: the builders' station over the metal sea at the cable.
fn ferry_terminal(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let terminal = places::ferry_terminal(&settings, &layout);
        stage_at(world, player, (terminal.center().x - 6, terminal.max.y - 2));
    });
    finish(stage, 30);
}

/// The Heart's chamber with its fight on `phase`, `elapsed` ms into it when captured.
fn heart_fight(stage: &mut SceneStage<'_>, phase: HeartPhase, elapsed: u64) {
    const FRAMES: usize = 30;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let centre = places::heart_centre(&settings, &layout);
        stage_at(
            world,
            player,
            (centre.x - 8, centre.y + FLOOR_BELOW_TILES - 1),
        );
        reveal(
            world,
            player,
            centre.x - 34..centre.x + 34,
            centre.y - 34..centre.y + 34,
        );
        let lead = FRAMES as u64 * 1_000 / 60;
        // The world's clock far enough on for the phase to have begun `elapsed` ago.
        let hz = u64::from(world.tick_rate_hz());
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
            (elapsed + 10_000) * hz / 1_000,
        ));
        world.stage_heart_phase(phase, elapsed.saturating_sub(lead));
        // The chamber's story card was read on the way in.
        let mut heart = world.heart().clone();
        heart.found = 0b1111;
        world.set_heart(heart);
        if phase == HeartPhase::Ignition {
            spawn(world, EnemyKind::HeartSpark, centre.x + 6, centre.y - 6);
        }
    });
    quiet_finish(stage, FRAMES);
}

/// [`finish`] with the stratum banner and the tips already read on the way down, so the
/// Heart and the ending fill the view.
fn quiet_finish(stage: &mut SceneStage<'_>, frames: usize) {
    let seat = stage.seat(0);
    seat.hints.seen = crate::shell::onboarding::Hint::ALL.into_iter().collect();
    seat.descent.banner = None;
    seat.fanfare = None;
    seat.message.clear();
    finish(stage, frames);
    let seat = stage.seat(0);
    seat.descent.banner = None;
    seat.message.clear();
    stage.settle(1);
}

/// The Heart a quiet moment before a rig wakes it: the corona wisps and a cable warden in
/// the star rock outside its shell, the cable running down into it.
fn heart_approach(stage: &mut SceneStage<'_>) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let centre = places::heart_centre(&settings, &layout);
        let top = centre.y - fatherload_simulation::heart::CHAMBER_RADIUS - 6;
        // A pocket dug over the top seal, beside the cable.
        for y in top - 4..top + 4 {
            for x in centre.x - 7..=centre.x + 7 {
                if x != centre.x {
                    let _ = world.terrain_mut().set_kind(
                        TilePosition { x, y },
                        fatherload_simulation::terrain::TileKind::Air,
                    );
                }
            }
        }
        stage_at(world, player, (centre.x - 4, top + 3));
        spawn(world, EnemyKind::CoronaWisp, centre.x + 4, top - 1);
        spawn(world, EnemyKind::CableWarden, centre.x + 5, top + 2);
    });
    finish(stage, 20);
}

/// Where a scene puts the rig, from the world's settings.
type RigPlace = fn(&WorldSettings) -> (i32, i32);

/// The ending at `elapsed` ms since the cable lit, with the rig at `at` (`None`: in the
/// Heart's chamber).
fn ending(stage: &mut SceneStage<'_>, elapsed: u64, at: Option<RigPlace>) {
    const FRAMES: usize = 20;
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        let settings = settings();
        let layout = descent::layout(&settings);
        let centre = places::heart_centre(&settings, &layout);
        let spot = at.map_or((centre.x - 8, centre.y + FLOOR_BELOW_TILES - 1), |at| {
            at(&settings)
        });
        stage_at(world, player, spot);
        // The world's clock far enough on for the cable to have lit `elapsed` ago.
        let hz = u64::from(world.tick_rate_hz());
        world.set_simulation_tick(fatherload_simulation::SimulationTick::new(
            (elapsed + 10_000) * hz / 1_000,
        ));
        world.stage_heart_phase(HeartPhase::Awakened, 0);
        let lead = FRAMES as u64 * 1_000 / 60;
        let mut heart = world.heart().clone();
        heart.found = 0b1111;
        let now = world.millis_at(world.simulation_tick());
        heart.cable_lit_millis = Some(now.saturating_sub(elapsed.saturating_sub(lead)));
        world.set_heart(heart);
    });
    quiet_finish(stage, FRAMES);
}

/// On the surface at the cable's column: the light rising out of the ground into the sky.
fn cable_surface_spot(settings: &WorldSettings) -> (i32, i32) {
    let column = landmarks::lift_column(settings);
    (column - 9, -3)
}

/// Just under the Moon, where the cable docks.
fn cable_moon_spot(settings: &WorldSettings) -> (i32, i32) {
    fatherload_simulation::worldgen::sky::moon(settings).map_or((0, -400), |moon| {
        (moon.center_x - 6, moon.top_row + moon.depth + 10)
    })
}

/// High over the Moon after the ending: the stars are open.
fn over_the_moon_spot(settings: &WorldSettings) -> (i32, i32) {
    fatherload_simulation::worldgen::sky::moon(settings)
        .map_or((0, -800), |moon| (moon.center_x + 40, moon.top_row - 60))
}

/// Settles `frames` frames, with everything round the seat's rig explored for the last two.
fn finish(stage: &mut SceneStage<'_>, frames: usize) {
    stage.settle(frames.saturating_sub(2));
    stage.play_on(|world| {
        let Some(player) = world.player_ids().next() else {
            return;
        };
        if let Some(tile) = world.actor_tile(player) {
            world.stream_terrain_near_players();
            reveal(
                world,
                player,
                tile.x - 64..tile.x + 64,
                tile.y - 40..tile.y + 40,
            );
        }
    });
    stage.settle(2);
}

pub const HEART_SCENES: &[Scene] = &[
    Scene {
        name: "heart_dark_matter_veins",
        summary: "Descent VIII, Dark Matter Veins: a weightless hall, dark matter hanging from its roof, null moths and an upfall beetle; the HUD's weight meter",
        setup: |stage| dark_matter_veins(stage, false),
    },
    Scene {
        name: "heart_falling_up",
        summary: "Too much dark matter: the rig falls up toward the hall's roof (the weight meter in pink)",
        setup: |stage| dark_matter_veins(stage, true),
    },
    Scene {
        name: "heart_upfall",
        summary: "The Upfall: a shaft whose roof is crusted with dark matter, a builder hauler stuck to it wheels up",
        setup: upfall,
    },
    Scene {
        name: "heart_singularity_fields",
        summary: "The Singularity Fields: a gravity well, its horizon and accretion disc, lensing bending the view round it, orbiters and motes",
        setup: singularity_fields,
    },
    Scene {
        name: "heart_lens",
        summary: "The Lens: the greatest well, event glass round its rim",
        setup: lens,
    },
    Scene {
        name: "heart_core_ocean",
        summary: "The Core Ocean: a rig riding the magnetic rail over the liquid metal sea, an eel, a rail leech and a slag skimmer",
        setup: |stage| core_ocean(stage, false),
    },
    Scene {
        name: "heart_rail_switch",
        summary: "A rail switch: the low line ramps down from the upper line toward the jump",
        setup: |stage| core_ocean(stage, true),
    },
    Scene {
        name: "heart_ferry_terminal",
        summary: "The Ferry Terminal: the builders' station over the metal sea at the cable",
        setup: ferry_terminal,
    },
    Scene {
        name: "heart_approach",
        summary: "The Heart's rock: star shards, corona wisps and a cable warden by the cable over the Heart's sealed top door",
        setup: heart_approach,
    },
    Scene {
        name: "heart_phase_coils",
        summary: "The Heart, phase 1: the drive coils orbit the sealed core; the gravity pulse's ring contracts (its tell)",
        setup: |stage| heart_fight(stage, HeartPhase::Coils, 6_700),
    },
    Scene {
        name: "heart_phase_ignition",
        summary: "The Heart, phase 2: ignition, the open core and its two turning beams, a spark",
        setup: |stage| heart_fight(stage, HeartPhase::Ignition, 4_000),
    },
    Scene {
        name: "heart_phase_collapse",
        summary: "The Heart, phase 3: the collapse, the dark core breathing out a ring of light (struck while it exhales)",
        setup: |stage| heart_fight(stage, HeartPhase::Collapse, 6_400),
    },
    Scene {
        name: "heart_awakening",
        summary: "The Heart wakes: the cable's light leaving the chamber up the cable, the camera pulled back, the title",
        setup: |stage| ending(stage, 4_500, None),
    },
    Scene {
        name: "heart_cable_surface",
        summary: "After the ending, on the surface: the cable's light rising out of the ground and across the sky to the Moon",
        setup: |stage| ending(stage, 300_000, Some(cable_surface_spot)),
    },
    Scene {
        name: "heart_cable_moon",
        summary: "After the ending, under the Moon: the cable's light docking at the Moon's underside",
        setup: |stage| ending(stage, 300_000, Some(cable_moon_spot)),
    },
    Scene {
        name: "heart_ending_ascent",
        summary: "The ending's cutaway: the planet's eight Descents, the light climbing the cable toward the Moon",
        setup: |stage| ending(stage, 15_500, None),
    },
    Scene {
        name: "heart_ending_moon",
        summary: "The ending's cutaway as the light reaches the Moon and it lights up",
        setup: |stage| ending(stage, 21_800, None),
    },
    Scene {
        name: "heart_credits",
        summary: "The credits rolling over the stars, the lit cable rising to the Moon beside them",
        setup: |stage| ending(stage, 40_000, None),
    },
    Scene {
        name: "heart_epilogue",
        summary: "The epilogue card after the credits",
        setup: |stage| ending(stage, 88_000, None),
    },
    Scene {
        name: "heart_stars_open",
        summary: "After the ending, high over the Moon with the Star Drive: the stars are open",
        setup: |stage| ending(stage, 300_000, Some(over_the_moon_spot)),
    },
];
