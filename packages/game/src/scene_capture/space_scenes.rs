//! Wave 11 space-render: space and round bodies, drawn from staged views
//! (`rendering::space::fixtures`) until the integrator wires the real space and flight state.

use super::{Scene, SceneStage};
use crate::rendering::{GameRenderer, space::fixtures};

impl SceneStage<'_> {
    /// Shows the staged space view `name` instead of the game.
    fn space_view(&mut self, name: &str) {
        self.space = fixtures::build(name);
        debug_assert!(self.space.is_some(), "unknown space fixture {name}");
    }
}

/// Draws `frames` more frames of a space scene (the clock running) and prints their CPU time.
pub(super) fn time_space_frames(
    (raylib, thread): (&mut raylib::RaylibHandle, &raylib::RaylibThread),
    renderer: &mut GameRenderer,
    fixture: &mut fixtures::SpaceFixture,
    name: &str,
    frames: usize,
) {
    let mut times = Vec::with_capacity(frames);
    for _ in 0..frames {
        let started = std::time::Instant::now();
        fixture.advance(super::FRAME_SECONDS);
        renderer.sync_space(raylib, thread, &fixture.view(), fixture.force_quads);
        let mut draw = raylib.begin_drawing(thread);
        renderer.render_space(&mut draw, &fixture.view());
        // SAFETY: flushes the batch so the timing includes the draw calls' submission.
        unsafe { raylib::ffi::rlDrawRenderBatchActive() };
        times.push(started.elapsed().as_secs_f64() * 1_000.0);
        drop(draw);
    }
    times.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frame counts and percentiles"
    )]
    let at = |fraction: f64| times[((times.len() - 1) as f64 * fraction).round() as usize];
    #[allow(clippy::cast_precision_loss, reason = "frame counts")]
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "timing {name:<28} mean {mean:6.2} ms  median {:6.2}  p95 {:6.2}  max {:6.2}",
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

pub const SPACE_SCENES: &[Scene] = &[
    Scene {
        name: "space_deep_starfield",
        summary: "Wave 11: the deep starfield in three parallax layers, the galaxy's band and haze, far galaxies",
        setup: |stage| stage.space_view("space_deep_starfield"),
    },
    Scene {
        name: "space_star_system",
        summary: "Wave 11: a star system overview: a yellow sun, six worlds and a moon, the belt, a comet",
        setup: |stage| stage.space_view("space_star_system"),
    },
    Scene {
        name: "space_planet_gallery",
        summary: "Wave 11: every body theme seen from space (lit sphere, bands, terminator, atmosphere)",
        setup: |stage| stage.space_view("space_planet_gallery"),
    },
    Scene {
        name: "space_orbit_themes",
        summary: "Wave 13: every theme's face from orbit: seas, clouds, caps, lava and crystal glow, gas bands, rings",
        setup: |stage| stage.space_view("space_orbit_themes"),
    },
    Scene {
        name: "space_orbit_night",
        summary: "Wave 13: night sides from orbit: a settled jungle world's town lights, a volcanic world's lava, home",
        setup: |stage| stage.space_view("space_orbit_night"),
    },
    Scene {
        name: "space_approach_moon",
        summary: "Wave 11: approaching a small moon (radius 400) from 600 tiles up",
        setup: |stage| stage.space_view("space_approach_moon"),
    },
    Scene {
        name: "space_approach_planet",
        summary: "Wave 11: approaching a big jungle world (radius 5,000) from 2,000 tiles up",
        setup: |stage| stage.space_view("space_approach_planet"),
    },
    Scene {
        name: "space_moon_hover",
        summary: "Wave 11: hovering 36 tiles over the small moon: its curve, the polar ground shader",
        setup: |stage| stage.space_view("space_moon_hover"),
    },
    Scene {
        name: "space_moon_surface",
        summary: "Wave 11: on the small moon's surface at the ground zoom: the curved horizon, things upright in their local frame",
        setup: |stage| stage.space_view("space_moon_surface"),
    },
    Scene {
        name: "space_moon_night",
        summary: "Wave 11: the small moon's night side",
        setup: |stage| stage.space_view("space_moon_night"),
    },
    Scene {
        name: "space_moon_rolled",
        summary: "Wave 11: the craft turned by its thrusters over the moon: the world turns, the craft and HUD stay upright",
        setup: |stage| stage.space_view("space_moon_rolled"),
    },
    Scene {
        name: "space_planet_surface",
        summary: "Wave 11: on the big jungle world (radius 5,000): the horizon almost flat, the atmosphere's sky",
        setup: |stage| stage.space_view("space_planet_surface"),
    },
    Scene {
        name: "space_core_shaft",
        summary: "Wave 11: a dig shaft down to the small moon's core: strata as rings, halved columns, the core chamber",
        setup: |stage| stage.space_view("space_core_shaft"),
    },
    Scene {
        name: "space_polar_quads",
        summary: "Wave 11: the polar ground's CPU fallback (curved quads), the hover scene's view",
        setup: |stage| stage.space_view("space_polar_quads"),
    },
    Scene {
        name: "space_nebula",
        summary: "Wave 11: two nebulae and a star born inside one",
        setup: |stage| stage.space_view("space_nebula"),
    },
    Scene {
        name: "space_black_hole",
        summary: "Wave 11: a black hole: accretion disc, the lens bending the sky, the horizon's shadow",
        setup: |stage| stage.space_view("space_black_hole"),
    },
    Scene {
        name: "space_asteroid_belt",
        summary: "Wave 11: inside an asteroid belt round a red sun",
        setup: |stage| stage.space_view("space_asteroid_belt"),
    },
    Scene {
        name: "space_comet",
        summary: "Wave 11: a comet: nucleus, coma, the curved dust tail and the straight ion tail",
        setup: |stage| stage.space_view("space_comet"),
    },
    Scene {
        name: "space_station_derelict",
        summary: "Wave 11: a station and a derelict (placeholder art for Wave 12)",
        setup: |stage| stage.space_view("space_station_derelict"),
    },
    Scene {
        name: "space_wormhole",
        summary: "Wave 11: a wormhole showing the far side's stars",
        setup: |stage| stage.space_view("space_wormhole"),
    },
    Scene {
        name: "space_warp",
        summary: "Wave 11: Star Drive warp: streaks and the tunnel",
        setup: |stage| stage.space_view("space_warp"),
    },
    Scene {
        name: "space_moon_flyover",
        summary: "Wave 11: skimming the small moon's hills (timed with --frame-timing: the ground pans 40 columns a second, its art repainted a little at a time)",
        setup: |stage| stage.space_view("space_moon_flyover"),
    },
    Scene {
        name: "space_cruise",
        summary: "Wave 11: Star Drive cruise past a moon: speed lines",
        setup: |stage| stage.space_view("space_cruise"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_has_a_scene() {
        for name in fixtures::NAMES {
            assert!(
                SPACE_SCENES.iter().any(|scene| scene.name == name),
                "{name}"
            );
        }
    }
}
