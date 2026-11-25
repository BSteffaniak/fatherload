//! Space's things on screen (Wave 12 systems, see `fatherload_simulation::flight::systems`):
//! a star's glare and flare wash, a nebula's fog, a black hole's point-of-no-return ring, the
//! wormhole tunnel, the mining beam and the drill's sparks in the world layer; the time
//! dilation readout, the flare forecast, the well's warnings, the mining and comet gauges and
//! the parking hold in the HUD.
//!
//! Every wash is smooth (no flashing), capped well under full white, and scaled by the screen
//! effects setting (`EffectsIntensity`): the readouts carry the information either way.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "screen layout mixes whole pixels and float positions"
)]

use fatherload_simulation::{
    field::ScreenEffect,
    flight::{
        BodyKind, FlightBody, RigFlight, SpaceQuery, StarDrive,
        physics::MARK_HOLDING,
        systems::{
            self, BodyExtra, COMET_HARVEST_CAP, FlarePhase, HoleWarning, beam_target,
            drill_contact, flare_heat, hole_warning, in_comet_tail,
        },
    },
    position::WorldPos,
    space::Space,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    space_view::SpaceFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// Brightest a star's glare or a flare washes the view (alpha 0-1, before the setting).
const MAX_GLARE: f32 = 0.42;
/// Thickest a nebula tints the view (alpha 0-1, before the setting).
const MAX_FOG_TINT: f32 = 0.34;

/// What the HUD and the world layer read of space's things round one seat's rig.
pub(super) struct SystemsReading {
    pub bodies: Vec<FlightBody>,
    pub glare: f32,
    pub glare_tint: Color,
    pub flare: Option<systems::FlareNear>,
    pub fog: u16,
    pub fog_tint: Color,
    pub hole: Option<(HoleWarning, usize)>,
    pub time_scale: f32,
    pub drill: Option<usize>,
    pub beam: Option<usize>,
    pub comet: Option<usize>,
    pub thrust: f32,
}

/// Reads space's things round `view`'s rig (pure functions of the snapshot).
pub(super) fn read(snapshot: &RenderSnapshot<'_>, view: &ViewSnapshot<'_>) -> SystemsReading {
    let galaxy = snapshot.space.galaxy();
    let flight = view.player.flight;
    let mut bodies = galaxy.bodies_near(flight.pos);
    bodies.extend(galaxy.rocks_near(flight.pos, 600, snapshot.space.systems));
    let now = snapshot.space.millis;
    let glare = systems::star_glare(&bodies, flight.pos, now);
    let glare_tint = bodies
        .iter()
        .filter(|body| body.kind == BodyKind::Star)
        .min_by(|a, b| {
            a.centre
                .span(flight.pos)
                .total_cmp(&b.centre.span(flight.pos))
        })
        .map_or(CREAM, |star| {
            let [r, g, b] = star.class.colour();
            Color::new(
                u16::midpoint(u16::from(r), 255) as u8,
                u16::midpoint(u16::from(g), 255) as u8,
                u16::midpoint(u16::from(b), 255) as u8,
                255,
            )
        });
    let (_, flare) = flare_heat(&bodies, flight.pos, now);
    let space: &Space = galaxy.space();
    let (fog, fog_tint) = space
        .nebula_at(flight.pos)
        .map_or((0, LAVENDER), |(fog, [r, g, b])| {
            (fog, Color::new(r, g, b, 255))
        });
    let thrust = fatherload_simulation::flight::SPACE_THRUST
        * fatherload_simulation::player::engine_power(view.player.engine_level);
    let orientation = view.player.attitude.orientation;
    let free = !flight.drive.engaged();
    SystemsReading {
        hole: hole_warning(&bodies, flight.pos, thrust),
        time_scale: space.time_scale_at(flight.pos),
        drill: free
            .then(|| {
                drill_contact(
                    &bodies,
                    flight.pos,
                    orientation,
                    fatherload_simulation::flight::RIG_CONTACT,
                )
            })
            .flatten(),
        beam: (free && view.player.hazard_gear.flight.mining_beam > 0)
            .then(|| beam_target(&bodies, flight.pos, orientation))
            .flatten(),
        comet: in_comet_tail(&bodies, flight.pos),
        bodies,
        glare,
        glare_tint,
        flare,
        fog,
        fog_tint,
        thrust,
    }
}

// --- The world layer (physical pixels) -----------------------------------------------------

/// Space's things over the galaxy and under the HUD (see the module docs).
pub(super) fn draw_world(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    reading: &SystemsReading,
    ticks: u64,
) {
    let flight = view.player.flight;
    let exposure = snapshot.effects.scale(ScreenEffect::Exposure);
    let fog_scale = snapshot.effects.scale(ScreenEffect::Fog);
    // The point of no return round a black hole: a dashed red ring, and the lensing rim.
    for body in &reading.bodies {
        if let BodyExtra::Hole(hole) = body.extra {
            let at = frame.project(body.centre);
            let no_return = systems::no_return_radius(body, reading.thrust) * frame.scale;
            dashed_ring(draw, at, no_return, with_alpha(RED, 170), ticks);
            dashed_ring(
                draw,
                at,
                hole.lensing * frame.scale,
                with_alpha(VIOLET, 70),
                0,
            );
        }
    }
    // Mining: sparks where the drill bites, or the beam from the nose to the rock.
    if flight.mining > 0.0 || reading.drill.is_some() {
        if let Some(index) = reading.drill {
            drill_sparks(draw, frame, &flight, &reading.bodies[index], ticks);
        } else if let Some(index) = reading.beam {
            mining_beam(draw, frame, view, &reading.bodies[index], ticks);
        }
    }
    // A comet's tail: dust motes drifting past the rig while it gathers.
    if reading.comet.is_some() {
        dust_motes(draw, frame, ticks);
    }
    if let StarDrive::Transit(transit) = flight.drive {
        let share = snapshot.space.millis.saturating_sub(transit.run.start) as f32
            / transit.run.duration_millis.max(1) as f32;
        transit_tunnel(
            draw,
            frame,
            share.clamp(0.0, 1.0),
            ticks,
            snapshot.effects.scale(ScreenEffect::Wobble).max(0.25),
        );
    }
    // A nebula tints everything (thicker toward the edges), a star's light washes it out.
    if reading.fog > 0 {
        let fog = f32::from(reading.fog.min(1_000)) / 1_000.0;
        let alpha = fog * MAX_FOG_TINT * fog_scale.max(0.3);
        vignette_wash(draw, frame.rect, reading.fog_tint, alpha);
    }
    let flare = reading.flare.map_or(0.0, |near| {
        if near.sheltered {
            0.0
        } else {
            near.phase.strength()
        }
    });
    let glare = (reading.glare + flare * 0.6).min(1.0) * MAX_GLARE * exposure;
    if glare > 0.004 {
        let tint = if flare > 0.0 {
            AMBER
        } else {
            reading.glare_tint
        };
        draw.draw_rectangle_rec(frame.rect, with_alpha(tint, (glare * 255.0) as u8));
    }
}

/// A ring of dashes turning slowly.
fn dashed_ring(
    draw: &mut RaylibDrawHandle<'_>,
    at: Vector2,
    radius: f32,
    colour: Color,
    ticks: u64,
) {
    if !(2.0..200_000.0).contains(&radius) {
        return;
    }
    let dashes = 48;
    let turn = (ticks % 3_600) as f32 * 0.05;
    for dash in 0..dashes {
        let start = (dash as f32 / dashes as f32).mul_add(360.0, turn);
        draw.draw_ring(
            at,
            radius - 1.5,
            radius + 1.5,
            start,
            start + 4.0,
            4,
            colour,
        );
    }
}

/// Sparks spraying off where the rig's underside grinds the rock.
fn drill_sparks(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    flight: &RigFlight,
    rock: &FlightBody,
    ticks: u64,
) {
    let (dx, dy) = flight.pos.delta(rock.centre);
    let distance = fatherload_simulation::field::trig::hypot(dx, dy).max(1.0);
    let contact = flight.pos.offset(
        dx / distance * (distance - rock.radius),
        dy / distance * (distance - rock.radius),
    );
    let at = frame.project(contact);
    let colour = ore_glint(rock);
    for spark in 0..10_u64 {
        let h = fatherload_simulation::flight::space::hash64(0x5_9A2C, spark, ticks as i64 / 2, 0);
        let angle = (h & 0xFFFF) as f32 / 65_536.0 * std::f32::consts::TAU;
        let reach = ((h >> 16) & 0xFF) as f32 / 255.0 * 14.0 * frame.scale.max(0.3) + 2.0;
        let (sin, cos) = angle.sin_cos();
        let end = Vector2::new(at.x + cos * reach, at.y + sin * reach);
        draw.draw_line_ex(at, end, 1.5, if spark % 3 == 0 { colour } else { GOLD });
    }
    draw.draw_circle_v(at, 3.0, with_alpha(CREAM, 200));
}

/// The Mining Beam from the rig's nose to the rock's surface.
fn mining_beam(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    view: &ViewSnapshot<'_>,
    rock: &FlightBody,
    ticks: u64,
) {
    let flight = view.player.flight;
    let (up, _) = fatherload_simulation::flight::physics::axes(view.player.attitude.orientation);
    let nose = flight.pos.offset(up.0 * 14.0, up.1 * 14.0);
    let (dx, dy) = nose.delta(rock.centre);
    let distance = fatherload_simulation::field::trig::hypot(dx, dy).max(1.0);
    let reach = (distance - rock.radius).max(0.0);
    let hit = nose.offset(dx / distance * reach, dy / distance * reach);
    let (a, b) = (frame.project(nose), frame.project(hit));
    let pulse = 0.75 + 0.25 * ((ticks % 20) as f32 / 20.0 * std::f32::consts::TAU).sin();
    let colour = ore_glint(rock);
    draw.draw_line_ex(a, b, 6.0 * pulse, with_alpha(colour, 70));
    draw.draw_line_ex(a, b, 2.5, with_alpha(colour, 200));
    draw.draw_line_ex(a, b, 1.0, CREAM);
    draw.draw_circle_v(b, 5.0 * pulse, with_alpha(colour, 120));
    draw.draw_circle_v(b, 2.0, CREAM);
}

/// The glint of a rock's ore.
const fn ore_glint(rock: &FlightBody) -> Color {
    match rock.extra {
        BodyExtra::Rock(facts) => super::space::live::ore_colour(facts.ore),
        _ => GOLD,
    }
}

/// Dust motes streaming past while the rig gathers in a comet's tail.
fn dust_motes(draw: &mut RaylibDrawHandle<'_>, frame: &SpaceFrame, ticks: u64) {
    for mote in 0..40_u64 {
        let h = fatherload_simulation::flight::space::hash64(0xC0_3E7, mote, 0, 0);
        let x = (h & 0xFFFF) as f32 / 65_536.0;
        let speed = 0.004 + ((h >> 16) & 0xFF) as f32 / 255.0 * 0.006;
        let y = (((h >> 24) & 0xFFFF) as f32 / 65_536.0 + ticks as f32 * speed) % 1.0;
        let at = Vector2::new(
            frame.rect.x + x * frame.rect.width,
            frame.rect.y + y * frame.rect.height,
        );
        let colour = if mote % 4 == 0 { ICE } else { BONE };
        draw.draw_rectangle_v(at, Vector2::new(2.0, 2.0), with_alpha(colour, 150));
    }
}

/// The wormhole's tunnel: rings rushing out from the middle, violet to teal, with a slow
/// twist (the effects setting thins it; nothing flashes).
fn transit_tunnel(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    share: f32,
    ticks: u64,
    strength: f32,
) {
    let span = frame.rect.width.hypot(frame.rect.height) * 0.6;
    // The tunnel's swell: in over the first fifth, out over the last.
    let swell = (share / 0.2).min((1.0 - share) / 0.2).clamp(0.0, 1.0);
    draw.draw_rectangle_rec(
        frame.rect,
        with_alpha(Color::new(20, 8, 40, 255), (swell * 150.0) as u8),
    );
    let rings = (10.0 + 14.0 * strength) as u64;
    for ring in 0..rings {
        let t = ((ticks as f32 * 0.02 + ring as f32 / rings as f32) % 1.0).powi(2);
        let radius = t * span + 4.0;
        let colour = if ring % 2 == 0 { VIOLET } else { TEAL };
        let alpha = (swell * (40.0 + 150.0 * t) * strength.max(0.4)) as u8;
        let twist = ticks as f32 * 0.6 + ring as f32 * 17.0;
        for arc in 0..6 {
            let start = (arc as f32).mul_add(60.0, twist);
            draw.draw_ring(
                frame.centre,
                radius,
                radius + 1.0 + 5.0 * t,
                start,
                start + 38.0,
                12,
                with_alpha(colour, alpha),
            );
        }
    }
    draw.draw_circle_v(
        frame.centre,
        18.0 * swell,
        with_alpha(LAVENDER, (swell * 120.0) as u8),
    );
}

/// A tint thicker toward the view's edges.
fn vignette_wash(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, tint: Color, alpha: f32) {
    let alpha = alpha.clamp(0.0, 0.6);
    draw.draw_rectangle_rec(rect, with_alpha(tint, (alpha * 0.6 * 255.0) as u8));
    let band = rect.width.min(rect.height) * 0.18;
    let edge = with_alpha(tint, (alpha * 0.5 * 255.0) as u8);
    let clear = with_alpha(tint, 0);
    draw.draw_rectangle_gradient_v(
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        band as i32,
        edge,
        clear,
    );
    draw.draw_rectangle_gradient_v(
        rect.x as i32,
        (rect.y + rect.height - band) as i32,
        rect.width as i32,
        band as i32,
        clear,
        edge,
    );
    draw.draw_rectangle_gradient_h(
        rect.x as i32,
        rect.y as i32,
        band as i32,
        rect.height as i32,
        edge,
        clear,
    );
    draw.draw_rectangle_gradient_h(
        (rect.x + rect.width - band) as i32,
        rect.y as i32,
        band as i32,
        rect.height as i32,
        clear,
        edge,
    );
}

// --- The HUD (UI pixels) -------------------------------------------------------------------

/// The share nav markers show at in the rig's fog (1 clear).
pub(super) fn marker_alpha(reading: &SystemsReading) -> f32 {
    systems::marker_visibility(reading.fog)
}

/// Space's readouts under the HUD's plate, from `top` down; returns the next free row.
#[allow(clippy::too_many_lines, reason = "one readout per kind of space thing")]
pub(super) fn draw_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    reading: &SystemsReading,
    rect: Rectangle,
    top: f32,
) -> f32 {
    let flight = view.player.flight;
    // Long lines only where they clear the meters and the nav panel.
    let narrow = rect.width < 820.0;
    let mut top = top;
    let banner = |draw: &mut RaylibDrawHandle<'_>, top: &mut f32, text: &str, colour: Color| {
        let width = ui_kit::text_width(text, TextStyle::Bold) + 12;
        let plate = Rectangle::new(plate_x(rect, width as f32), *top, width as f32, 15.0);
        ui_kit::panel(draw, plate, PanelStyle::Standard);
        ui_kit::text_centred(
            draw,
            text,
            (plate.x + plate.width * 0.5) as i32,
            plate.y as i32 + 4,
            TextStyle::Bold,
            colour,
        );
        *top = plate.y + plate.height + 3.0;
    };
    // A black hole's well: the deepest warning, plainly.
    if let Some((level, _)) = reading.hole {
        let (text, colour) = match level {
            HoleWarning::Well => (
                if narrow {
                    "GRAVITY WELL"
                } else {
                    "GRAVITY WELL: time runs slow here"
                },
                GOLD,
            ),
            HoleWarning::Strain => (
                if narrow {
                    "ESCAPE THRUST FAILING"
                } else {
                    "ESCAPE THRUST FAILING: burn away from the hole now"
                },
                ORANGE,
            ),
            HoleWarning::NoReturn => (
                if narrow {
                    "NO RETURN: BEACON"
                } else {
                    "PAST THE POINT OF NO RETURN: call the beacon (nav chart)"
                },
                RED,
            ),
        };
        banner(draw, &mut top, text, colour);
    }
    // The flare forecast.
    if let Some(near) = reading.flare {
        let text = match near.phase {
            FlarePhase::Warning { in_millis } => Some((
                if narrow {
                    format!("FLARE IN {}s", in_millis.div_ceil(1_000))
                } else {
                    format!(
                        "SOLAR FLARE IN {}s: get behind a world or out of reach",
                        in_millis.div_ceil(1_000)
                    )
                },
                ORANGE,
            )),
            FlarePhase::Burst { .. } if near.sheltered => {
                Some(("SOLAR FLARE: SHELTERED".to_owned(), MINT))
            }
            FlarePhase::Burst { .. } => Some(("SOLAR FLARE".to_owned(), RED)),
            FlarePhase::Quiet { next_millis } if next_millis < 120_000 => Some((
                format!(
                    "next flare in {}",
                    fatherload_simulation::flight::nav::format_eta(next_millis as f32 / 1_000.0)
                ),
                AMBER,
            )),
            FlarePhase::Quiet { .. } => None,
        };
        if let Some((text, colour)) = text {
            banner(draw, &mut top, &text, colour);
        }
    }
    // Time dilation: how fast the rig's clock runs and how far it has fallen behind.
    if reading.time_scale < 0.999 || flight.lag_seconds >= 0.5 {
        let text = format!(
            "TIME x{:.2}   CLOCK -{}",
            reading.time_scale,
            fatherload_simulation::flight::nav::format_eta(flight.lag_seconds)
        );
        let colour = if reading.time_scale < 0.999 {
            LAVENDER
        } else {
            FOG
        };
        banner(draw, &mut top, &text, colour);
    }
    // Nebula fog.
    if reading.fog >= 60 {
        let text = format!(
            "SENSORS FOGGED {}%",
            (u32::from(reading.fog.min(1_000)) + 5) / 10
        );
        banner(draw, &mut top, &text, reading.fog_tint);
    }
    // Mining and comet dust: a gauge each.
    let gauge =
        |draw: &mut RaylibDrawHandle<'_>, top: &mut f32, label: &str, ratio: f32, colour: Color| {
            let width = 170.0_f32.min(rect.width - 12.0);
            let plate = Rectangle::new(plate_x(rect, width), *top, width, 22.0);
            ui_kit::panel(draw, plate, PanelStyle::Standard);
            ui_kit::text(
                draw,
                label,
                plate.x as i32 + 5,
                plate.y as i32 + 3,
                TextStyle::Bold,
                colour,
            );
            ui_kit::bar(
                draw,
                Rectangle::new(plate.x + 5.0, plate.y + 13.0, plate.width - 10.0, 6.0),
                ratio,
                ui_kit::BarColors::FUEL,
                0.0,
            );
            *top = plate.y + plate.height + 3.0;
        };
    let rock = reading
        .drill
        .or(reading.beam)
        .map(|index| &reading.bodies[index]);
    if let Some(rock) = rock
        && let BodyExtra::Rock(facts) = rock.extra
    {
        let left = facts
            .yield_units
            .saturating_sub(snapshot.space.systems.mined_from(facts.id));
        let label = format!(
            "{} {}  {left} LEFT",
            if reading.drill.is_some() {
                "DRILLING"
            } else {
                "BEAM"
            },
            facts.ore.name().to_uppercase()
        );
        gauge(
            draw,
            &mut top,
            &label,
            flight.mining,
            super::space::live::ore_colour(facts.ore),
        );
    }
    if let Some(index) = reading.comet
        && let BodyExtra::Comet(comet) = reading.bodies[index].extra
    {
        let taken = snapshot
            .space
            .systems
            .comets
            .get(&comet.key)
            .copied()
            .unwrap_or(0);
        let label = if taken >= COMET_HARVEST_CAP {
            "COMET TAIL: DUST GATHERED".to_owned()
        } else {
            format!("COMET DUST  {taken}/{COMET_HARVEST_CAP}")
        };
        gauge(draw, &mut top, &label, flight.gathering, ICE);
    }
    if flight.marks & MARK_HOLDING != 0 {
        banner(draw, &mut top, "PARKING HOLD", SKY_LIGHT);
    }
    top
}

/// Where a readout plate `width` wide sits in `rect`: centred, clear of the meters on the
/// left where the view is wide enough, and never past the view's right edge (a split-screen
/// seat's view is narrow: a plate pushed right of the meters was cut off there).
fn plate_x(rect: Rectangle, width: f32) -> f32 {
    let centred = (rect.width - width).mul_add(0.5, rect.x);
    let clear = centred.max(rect.x + 158.0);
    clear
        .min(rect.x + rect.width - width - 2.0)
        .max(rect.x + 2.0)
}

/// The wormhole mouths round the rig the nav HUD points at: every mouth within two galactic
/// sectors, with the far end's sector once the crew has flown it.
pub(super) fn wormhole_marks(
    snapshot: &RenderSnapshot<'_>,
    at: WorldPos,
) -> Vec<(WorldPos, String)> {
    let galaxy = snapshot.space.galaxy();
    let (sx, sy) = fatherload_simulation::flight::space::galaxy_sector_of(at);
    let mut marks = Vec::new();
    for gy in sy - 2..=sy + 2 {
        for gx in sx - 2..=sx + 2 {
            let (Ok(x), Ok(y)) = (i32::try_from(gx), i32::try_from(gy)) else {
                continue;
            };
            let sector = galaxy
                .space()
                .sector(fatherload_simulation::space::SpaceSector::new(x, y));
            for mouth in &sector.wormholes {
                let label = snapshot.space.systems.wormhole_from((gx, gy)).map_or_else(
                    || "WORMHOLE ?".to_owned(),
                    |(_, (ex, ey))| format!("WORMHOLE > {ex},{ey}"),
                );
                marks.push((mouth.centre.pos(), label));
            }
        }
    }
    marks
}
