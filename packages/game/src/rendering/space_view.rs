//! Space on screen (Wave 11 flight): a seat whose rig is away from home sees space instead of
//! the home world, with the flight HUD, the nav HUD and the nav chart and station dialogs.
//!
//! **Placeholder art** until the space-render track's (TODO-SEAM(w11-space-render): its polar
//! shader draws the round worlds and their ground; this view keeps the HUD and the flight
//! cues). Everything is drawn from the seat's rig outward:
//!
//! * **The camera is locked to the craft**: the rig is drawn upright in the middle of the view
//!   and space turns around it (the screen-relative accessibility scheme keeps the chart up
//!   instead and turns the rig). The HUD always stays upright.
//! * **The view zooms with the nearest surface**: far from everything the stars and worlds
//!   are a map; approaching a world (or a station's port) it closes in to one to one.
//! * A parallax starfield (streaking at speed), each body as a lit disc (stars with coronas,
//!   worlds with an atmosphere's glow, the home world with its Moon), stations with their
//!   port's light, the other flyers (streaks in warp), a warp's tunnel of light, re-entry
//!   plasma, and the scanner ping's ring.
//! * The HUD: where the rig is and its drive, hull, fuel, air and heat bars with early
//!   warnings, the docking guide, the button prompts and, with a nav suite, the nav panel
//!   (systems around, the waypoint, distance and ETA, the warp's alignment) and arrows to the
//!   waypoint and the crew at the view's edge.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "screen layout mixes whole pixels and float positions"
)]

use fatherload_simulation::{
    PlayerId,
    flight::{
        BodyKind, FlightBody, FlightPlace, RigFlight, SpaceQuery, StarDrive,
        nav::{self, NavTarget},
        physics::{WARP_ABORT_ANGLE, WARP_ALIGN, WARP_SPOOL_MILLIS},
        space::{self, nearest_surface},
    },
    position::WorldPos,
    terrain::TILE_SIZE,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::sprites::{icons, menu_icons},
    prompts::hint,
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::{Flyer, PlayerPresentation, RenderSnapshot, ViewSnapshot},
    shell::{
        ShellState,
        flight::{NavRow, StationRow, nav_rows_for, station_rows, system_name, target_name},
    },
};

/// Deep space's backdrop.
const SPACE: Color = Color::new(8, 7, 20, 255);
/// Where the drawn rig sits in the sky (high, in open air, so it draws flying).
const RIG_SKY_Y: f32 = -200_000.0;

/// How space maps to the seat's view this frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct SpaceFrame {
    /// The rig's space position (the view's middle).
    pub origin: WorldPos,
    /// The view's middle, physical pixels.
    pub centre: Vector2,
    /// Physical pixels per space pixel.
    pub scale: f32,
    /// The view's turn (the rig's orientation when the camera is locked to it, else 0).
    pub roll: f32,
    /// The physical viewport.
    pub rect: Rectangle,
}

impl SpaceFrame {
    /// `pos` on screen (physical pixels).
    pub fn project(&self, pos: WorldPos) -> Vector2 {
        let (dx, dy) = self.origin.delta(pos);
        self.offset_to_screen(dx, dy)
    }

    /// A space offset from the rig on screen.
    pub fn offset_to_screen(&self, dx: f32, dy: f32) -> Vector2 {
        let (x, y) = self.turn(dx, dy);
        Vector2::new(
            x.mul_add(self.scale, self.centre.x),
            y.mul_add(self.scale, self.centre.y),
        )
    }

    /// A space direction turned into the view's (unscaled).
    pub fn turn(&self, dx: f32, dy: f32) -> (f32, f32) {
        let (sin, cos) = (-self.roll).sin_cos();
        (dx.mul_add(cos, -dy * sin), dx.mul_add(sin, dy * cos))
    }

    fn visible(&self, at: Vector2, radius: f32) -> bool {
        at.x + radius >= self.rect.x
            && at.x - radius <= self.rect.x + self.rect.width
            && at.y + radius >= self.rect.y
            && at.y - radius <= self.rect.y + self.rect.height
    }
}

/// The view's zoom (a share of one to one) `gap` space pixels from the nearest surface, with
/// `visible` space pixels of view height at one to one.
#[must_use]
pub fn zoom_for(gap: f32, visible: f32) -> f32 {
    (visible * 0.32 / gap.max(1.0)).clamp(1.0 / 6_000.0, 1.0)
}

/// Distance from `at` to the nearest black hole's disc (space pixels), if one is near.
fn black_hole_gap(seed: u64, at: WorldPos) -> Option<f32> {
    let space = fatherload_simulation::space::Space::shared(seed);
    let tile = fatherload_simulation::space::SpaceTile::of_pos(at);
    space
        .sectors_near(tile, fatherload_simulation::space::SPACE_SECTOR_TILES / 2)
        .iter()
        .flat_map(|sector| sector.black_holes.clone())
        .map(|hole| {
            let distance = hole.centre.distance(tile) as f32;
            let disc = hole.horizon as f32 * 3.0;
            (distance - disc).max(1.0) * TILE_SIZE
        })
        .min_by(f32::total_cmp)
}

/// The space frame for `view` (see the module docs) in the world seeded `seed`.
pub(super) fn space_frame(
    shell: &ShellState,
    view: &ViewSnapshot<'_>,
    bodies: &[FlightBody],
    density: f32,
    seed: u64,
) -> SpaceFrame {
    let flight = view.player.flight;
    let rect = Rectangle::new(
        view.viewport.x as f32 * density,
        view.viewport.y as f32 * density,
        view.viewport.width as f32 * density,
        view.viewport.height as f32 * density,
    );
    let (gap, _) = nearest_surface(bodies, flight.pos);
    // A black hole's disc frames the view as a body's surface would (Wave 11 integration).
    let gap = black_hole_gap(seed, flight.pos).map_or(gap, |hole| gap.min(hole));
    let visible = view.viewport.height as f32 / view.world_scale.max(0.01);
    let zoom = zoom_for(gap, visible);
    SpaceFrame {
        origin: flight.pos,
        centre: Vector2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
        scale: view.world_scale * density * zoom,
        roll: if shell.flight.screen_relative {
            0.0
        } else {
            view.player.attitude.orientation
        },
        rect,
    }
}

/// Draws space for a seat whose rig is away (the world part, physical pixels), then returns
/// the frame for the HUD. With `backdrop` false the sky, the suns and the worlds are left to
/// the space renderer (drawn first, from the real galaxy: Wave 11 integration) and this draws
/// flight's own layer over it: stations with their ports, pings, crewmates, the warp tunnel
/// and the rig.
#[allow(clippy::too_many_arguments, reason = "one view's scene")]
pub(super) fn draw_space_world(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &fatherload_simulation::terrain::Terrain,
    facings: &super::world::RigFacings,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    density: f32,
    backdrop: bool,
) -> SpaceFrame {
    let galaxy = snapshot.space.galaxy();
    let flight = view.player.flight;
    let bodies = galaxy.bodies_near(flight.pos);
    let frame = space_frame(shell, view, &bodies, density, snapshot.space.seed);
    let strength = match snapshot.effects {
        crate::screen_fx::EffectsIntensity::Full => 1.0,
        crate::screen_fx::EffectsIntensity::Reduced => 0.5,
        crate::screen_fx::EffectsIntensity::Off => 0.25,
    };
    let ticks = shell.update_ticks;
    if backdrop {
        draw.draw_rectangle_rec(frame.rect, SPACE);
        draw_starfield(draw, &frame, &flight, ticks, strength);
    }
    // Biggest first: stars, then worlds, then stations on top.
    let mut order: Vec<&FlightBody> = bodies
        .iter()
        .filter(|body| backdrop || body.kind == BodyKind::Station)
        .collect();
    order.sort_by(|a, b| b.approach.total_cmp(&a.approach));
    for body in order {
        draw_body(draw, &frame, body, &bodies, ticks);
    }
    draw_ping(draw, &frame, snapshot, view.player_id);
    for flyer in &snapshot.space.flyers {
        if flyer.player_id == view.player_id {
            continue;
        }
        draw_flyer(draw, terrain, facings, &frame, snapshot, view, flyer, ticks);
    }
    if let StarDrive::Warp(run) = flight.drive {
        draw_warp_tunnel(draw, &frame, &flight, snapshot, run.target, ticks, strength);
    }
    draw_own_rig(draw, terrain, facings, &frame, snapshot, view, ticks);
    frame
}

/// A star field in three parallax layers; at speed the nearer stars streak along the motion
/// (in a warp the tunnel takes over and the field dims).
fn draw_starfield(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    flight: &RigFlight,
    ticks: u64,
    strength: f32,
) {
    let (ax, ay) = flight.pos.px_f64();
    let diagonal = frame.rect.width.hypot(frame.rect.height);
    let cell = (44.0 * (frame.rect.height / 720.0).max(1.0)).max(24.0);
    let reach = (diagonal * 0.5 / cell).ceil() as i64 + 1;
    let warping = matches!(flight.drive, StarDrive::Warp(_));
    let speed = if warping { 0.0 } else { flight.speed() };
    // Speed on screen: a streak's length (motion's direction turned into the view).
    let (vx, vy) = frame.turn(flight.velocity_x, flight.velocity_y);
    let (ux, uy) = if speed > 1.0 {
        (vx / speed, vy / speed)
    } else {
        (0.0, 0.0)
    };
    for (layer, parallax) in [0.000_05_f64, 0.000_2, 0.000_8].into_iter().enumerate() {
        let shift_x = ax * parallax;
        let shift_y = ay * parallax;
        let base_x = (shift_x / f64::from(cell)).floor() as i64;
        let base_y = (shift_y / f64::from(cell)).floor() as i64;
        // Only the nearer layers streak, and never far: a hint of speed, not rain.
        let longest = cell * (layer as f32 * 0.45);
        let streak = ((f64::from(speed) * parallax * 1.2) as f32).min(longest) * strength;
        for j in -reach..=reach {
            for i in -reach..=reach {
                let (cx, cy) = (base_x + i, base_y + j);
                let hash = space::hash64(0x57A2_F1E1, layer as u64, cx, cy);
                if !hash.is_multiple_of(2) {
                    continue;
                }
                let fx = ((hash >> 1) & 0xFFFF) as f32 / 65_536.0;
                let fy = ((hash >> 17) & 0xFFFF) as f32 / 65_536.0;
                let local_x = ((cx as f64 + f64::from(fx)) * f64::from(cell) - shift_x) as f32;
                let local_y = ((cy as f64 + f64::from(fy)) * f64::from(cell) - shift_y) as f32;
                let (x, y) = frame.turn(local_x, local_y);
                let at = Vector2::new(frame.centre.x + x, frame.centre.y + y);
                if !frame.visible(at, streak + 2.0) {
                    continue;
                }
                let bright = (hash >> 40) % 5;
                // A slow, gentle twinkle on a few (never a flash).
                let twinkle =
                    (hash >> 44).is_multiple_of(7) && (ticks / 40 + hash).is_multiple_of(4);
                let colour = match (layer, bright) {
                    (2, 0) => CREAM,
                    (2, _) => BONE,
                    (1, 0) => SKY_LIGHT,
                    (1, _) => MIST,
                    (_, 0) => LAVENDER,
                    _ => FOG,
                };
                let alpha = match (warping, twinkle) {
                    (true, _) => 90,
                    (false, true) => 150,
                    (false, false) => 255,
                };
                let size = if layer == 2 && bright == 0 { 2 } else { 1 };
                if streak > 2.0 {
                    draw.draw_line_ex(
                        at,
                        Vector2::new(at.x - ux * streak, at.y - uy * streak),
                        size as f32,
                        with_alpha(colour, 90),
                    );
                }
                draw.draw_rectangle(
                    at.x as i32,
                    at.y as i32,
                    size,
                    size,
                    with_alpha(colour, alpha),
                );
            }
        }
    }
}

/// The worlds' colour sets: (base, lit, detail, air).
const BODY_COLOURS: [(Color, Color, Color, Color); 6] = [
    (RUST, COPPER, SAND, ORANGE),
    (MOSS, LEAF, LIME, MINT),
    (TEAL_DARK, TEAL, MINT, ICE),
    (PLUM, VIOLET, LAVENDER, PINK),
    (STONE_DARK, STONE, STONE_LIGHT, MIST),
    (DEEP_BLUE, OCEAN, ICE, SKY_LIGHT),
];

/// A body's colours: (base, lit, detail, air).
fn body_colours(body: &FlightBody) -> (Color, Color, Color, Color) {
    if body.kind == BodyKind::Home {
        return (OCEAN, SKY, LEAF, SKY_LIGHT);
    }
    let roll = space::hash64(body.body.system.id, 0xC010, i64::from(body.body.index), 0);
    BODY_COLOURS[(roll % 6) as usize]
}

/// A filled ring that stays round however large (huge rings draw only the arc near the view).
fn fill_ring(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    at: Vector2,
    inner: f32,
    outer: f32,
    colour: Color,
) {
    if outer <= 0.5 || outer <= inner || !frame.visible(at, outer) {
        return;
    }
    let screen = frame.rect.width.hypot(frame.rect.height);
    // Wholly outside the view (the view inside the hole): nothing to draw.
    let to_view = (frame.centre.x - at.x).hypot(frame.centre.y - at.y);
    if to_view + screen < inner {
        return;
    }
    if outer < screen * 2.0 {
        let segments = ((outer * 0.6) as i32).clamp(18, 256);
        draw.draw_ring(at, inner.max(0.0), outer, 0.0, 360.0, segments, colour);
        return;
    }
    huge_band(draw, frame, at, inner, outer, colour);
}

/// A filled disc that stays round however large (huge discs draw only the arc near the view).
fn fill_disc(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    at: Vector2,
    radius: f32,
    colour: Color,
) {
    if radius <= 0.5 {
        return;
    }
    if !frame.visible(at, radius) {
        return;
    }
    let screen = frame.rect.width.hypot(frame.rect.height);
    if radius < screen * 2.0 {
        let segments = ((radius * 0.6) as i32).clamp(18, 256);
        draw.draw_circle_sector(at, radius, 0.0, 360.0, segments, colour);
        return;
    }
    huge_band(draw, frame, at, 0.0, radius, colour);
}

/// The part of a huge disc or ring (`inner` 0 for a disc) near the view, as a strip of quads
/// computed in double precision relative to the view, so its edge is a smooth curve however
/// far its centre is (a landed rig's horizon).
fn huge_band(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    at: Vector2,
    inner: f32,
    outer: f32,
    colour: Color,
) {
    const SEGMENTS: usize = 96;
    let screen = f64::from(frame.rect.width.hypot(frame.rect.height));
    let (cx, cy) = (f64::from(at.x), f64::from(at.y));
    let towards = (f64::from(frame.centre.y) - cy).atan2(f64::from(frame.centre.x) - cx);
    let outer = f64::from(outer);
    let half = (screen * 1.5 / outer).min(std::f64::consts::PI);
    // A disc's fill only needs to reach a little past the view (the view may lie deep inside
    // it: an atmosphere seen from its floor).
    let to_view = (f64::from(frame.centre.x) - cx).hypot(f64::from(frame.centre.y) - cy);
    let inner = if inner > 0.0 {
        f64::from(inner)
    } else {
        (to_view - screen).min(outer - screen * 2.0).max(0.0)
    };
    let point = |angle: f64, reach: f64| {
        Vector2::new(
            angle.cos().mul_add(reach, cx) as f32,
            angle.sin().mul_add(reach, cy) as f32,
        )
    };
    let mut previous: Option<(Vector2, Vector2)> = None;
    for step in 0..=SEGMENTS {
        let angle = towards - half + 2.0 * half * step as f64 / SEGMENTS as f64;
        let (edge, depth) = (point(angle, outer), point(angle, inner));
        if let Some((last_edge, last_depth)) = previous {
            // Both windings: raylib culls one.
            draw.draw_triangle(last_edge, last_depth, edge, colour);
            draw.draw_triangle(last_edge, edge, last_depth, colour);
            draw.draw_triangle(edge, last_depth, depth, colour);
            draw.draw_triangle(edge, depth, last_depth, colour);
        }
        previous = Some((edge, depth));
    }
}

/// Specks of ground under a surface seen up close (a landed rig's view): pixel grit in the
/// body's colours, fixed to the body so they do not swim as the rig moves.
fn draw_ground_grit(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    body: &FlightBody,
    at: Vector2,
    radius: f32,
    colours: (Color, Color, Color),
) {
    let screen = frame.rect.width.hypot(frame.rect.height);
    if radius < screen * 2.0 {
        return;
    }
    let (cx, cy) = (f64::from(at.x), f64::from(at.y));
    let towards = (f64::from(frame.centre.y) - cy).atan2(f64::from(frame.centre.x) - cx);
    let pixel = f64::from((frame.scale * 4.0).max(2.0));
    let radius = f64::from(radius);
    let step = pixel * 2.0 / radius;
    let span = f64::from(screen) * 0.6 / radius;
    let first = ((towards - span) / step).floor() as i64;
    let last = ((towards + span) / step).ceil() as i64;
    let size = pixel as f32;
    for column in first..=last {
        for layer in 1..14_i64 {
            let roll = space::hash64(
                body.body.system.id ^ u64::from(body.body.index),
                0x6A17,
                column,
                layer,
            );
            if !roll.is_multiple_of(3) {
                continue;
            }
            let angle = column as f64 * step + ((roll >> 8) & 0xFF) as f64 / 255.0 * step;
            let reach =
                radius - (layer as f64 + ((roll >> 16) & 0xFF) as f64 / 255.0) * pixel * 2.5;
            let spot = Vector2::new(
                angle.cos().mul_add(reach, cx) as f32,
                angle.sin().mul_add(reach, cy) as f32,
            );
            if !frame.visible(spot, size) {
                continue;
            }
            let colour = match (roll >> 24) % 3 {
                0 => colours.0,
                1 => colours.1,
                _ => colours.2,
            };
            let fade = (255 - layer * 14).clamp(60, 255) as u8;
            draw.draw_rectangle_v(spot, Vector2::new(size, size), with_alpha(colour, fade));
        }
    }
}

/// One body.
#[allow(
    clippy::too_many_lines,
    reason = "each kind of body's look in one place"
)]
fn draw_body(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    body: &FlightBody,
    bodies: &[FlightBody],
    ticks: u64,
) {
    let at = frame.project(body.centre);
    let radius = body.radius * frame.scale;
    match body.kind {
        BodyKind::Star => {
            let [r, g, b] = body.class.colour();
            let colour = Color::new(r, g, b, 255);
            let glow = radius.max(2.0);
            fill_disc(draw, frame, at, glow * 3.0, with_alpha(colour, 14));
            fill_disc(draw, frame, at, glow * 2.0, with_alpha(colour, 30));
            fill_disc(draw, frame, at, glow * 1.4, with_alpha(colour, 70));
            fill_disc(draw, frame, at, radius.max(1.5), colour);
            fill_disc(draw, frame, at, radius * 0.72, with_alpha(CREAM, 200));
        }
        BodyKind::Planet | BodyKind::Home => {
            let (base, lit, detail, air) = body_colours(body);
            if body.atmosphere > 0.0 {
                let top = (body.radius + body.atmosphere) * frame.scale;
                fill_disc(draw, frame, at, top, with_alpha(air, 26));
                fill_disc(
                    draw,
                    frame,
                    at,
                    radius + (top - radius) * 0.35,
                    with_alpha(air, 60),
                );
            }
            if radius < 1.5 {
                draw.draw_rectangle(at.x as i32, at.y as i32, 2, 2, lit);
                return;
            }
            fill_disc(draw, frame, at, radius, base);
            // Lit toward the system's star.
            let star = bodies.iter().find(|other| other.kind == BodyKind::Star);
            let (lx, ly) = star.map_or((0.0, -1.0), |star| {
                let (dx, dy) = body.centre.delta(star.centre);
                let length = dx.hypot(dy).max(1.0);
                frame.turn(dx / length, dy / length)
            });
            let lit_at = Vector2::new(
                lx.mul_add(radius * 0.22, at.x),
                ly.mul_add(radius * 0.22, at.y),
            );
            fill_disc(draw, frame, lit_at, radius * 0.78, lit);
            // Bands of ground, like strata seen from above, and a few lands and seas.
            for (share, alpha) in [(0.62, 60_u8), (0.4, 50), (0.2, 40)] {
                fill_ring(
                    draw,
                    frame,
                    lit_at,
                    radius * share,
                    radius * (share + 0.06),
                    with_alpha(base, alpha),
                );
            }
            for patch in 0..6_u64 {
                let roll = space::hash64(
                    body.body.system.id,
                    0xDE7A,
                    i64::from(body.body.index),
                    patch as i64,
                );
                let angle = (roll & 0xFFFF) as f32 / 65_536.0 * std::f32::consts::TAU - frame.roll;
                let reach = ((roll >> 16) & 0xFF) as f32 / 255.0 * 0.55 * radius;
                let size = (((roll >> 24) & 0xFF) as f32 / 255.0).mul_add(0.12, 0.1) * radius;
                let spot = Vector2::new(
                    angle.cos().mul_add(reach, at.x),
                    angle.sin().mul_add(reach, at.y),
                );
                fill_disc(draw, frame, spot, size, with_alpha(detail, 150));
                fill_disc(draw, frame, spot, size * 0.55, with_alpha(lit, 120));
            }
            // The crust at the limb: a bright rim and a darker band under it (the ground a
            // landed rig stands on reads as a surface, not a flat fill).
            let rim = (radius * 0.012).clamp(1.0, 10.0 * frame.scale.max(0.2));
            draw_ground_grit(draw, frame, body, at, radius, (detail, lit, base));
            fill_ring(
                draw,
                frame,
                at,
                radius - rim,
                radius,
                with_alpha(detail, 230),
            );
            if body.atmosphere > 0.0 {
                fill_ring(
                    draw,
                    frame,
                    at,
                    radius,
                    radius + rim * 1.5,
                    with_alpha(air, 150),
                );
            }
            if body.kind == BodyKind::Home {
                // The Moon stays as it is (part of the home sky); seen from space it hangs by
                // the home world.
                let moon = Vector2::new(at.x + radius * 0.95, at.y - radius * 0.62);
                fill_disc(draw, frame, moon, (radius * 0.12).max(1.0), STEEL_LIGHT);
                fill_disc(
                    draw,
                    frame,
                    Vector2::new(moon.x + radius * 0.02, moon.y + radius * 0.02),
                    (radius * 0.08).max(0.8),
                    STEEL,
                );
            }
        }
        BodyKind::Station => draw_station(draw, frame, body, at, radius, ticks),
    }
}

/// A station: solar wings, a spar, a ring with lit windows, a hub, and the docking port's
/// light with its approach guide.
fn draw_station(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    body: &FlightBody,
    at: Vector2,
    radius: f32,
    ticks: u64,
) {
    if !frame.visible(at, radius * 4.0 + 8.0) {
        return;
    }
    if radius < 3.0 {
        draw.draw_rectangle(at.x as i32 - 1, at.y as i32 - 1, 3, 3, STEEL_LIGHT);
        draw.draw_rectangle(at.x as i32, at.y as i32, 1, 1, MINT);
        return;
    }
    let rotation = (body.port_angle - frame.roll).to_degrees();
    let unit = radius / 5.0;
    // The solar wings (outlined, with cell lines), the spar.
    for side in [-1.0_f32, 1.0] {
        let (sin, cos) = (body.port_angle - frame.roll).sin_cos();
        let centre = Vector2::new(
            at.x + cos * side * unit * 7.5,
            at.y + sin * side * unit * 7.5,
        );
        let frame_rect = Rectangle::new(centre.x, centre.y, unit * 5.4, unit * 3.0);
        draw.draw_rectangle_pro(
            frame_rect,
            Vector2::new(unit * 2.7, unit * 1.5),
            rotation,
            INK,
        );
        let wing = Rectangle::new(centre.x, centre.y, unit * 5.0, unit * 2.6);
        draw.draw_rectangle_pro(
            wing,
            Vector2::new(unit * 2.5, unit * 1.3),
            rotation,
            DEEP_BLUE,
        );
        for column in 0..4 {
            let offset = (column as f32 - 1.5) * unit * 1.2;
            let cell = Vector2::new(centre.x + cos * offset, centre.y + sin * offset);
            let rect = Rectangle::new(cell.x, cell.y, unit * 1.0, unit * 2.2);
            draw.draw_rectangle_pro(rect, Vector2::new(unit * 0.5, unit * 1.1), rotation, OCEAN);
        }
    }
    let spar = Rectangle::new(at.x, at.y, unit * 13.0, unit * 0.8);
    draw.draw_rectangle_pro(
        spar,
        Vector2::new(unit * 6.5, unit * 0.4),
        rotation,
        STEEL_DARK,
    );
    draw.draw_ring(at, radius * 0.85, radius * 1.2, 0.0, 360.0, 48, INK);
    draw.draw_ring(at, radius * 0.9, radius * 1.15, 0.0, 360.0, 48, STEEL);
    // Lit windows round the ring (steady).
    for window in 0..12 {
        let angle = window as f32 / 12.0 * std::f32::consts::TAU + body.port_angle - frame.roll;
        let spot = Vector2::new(
            angle.cos().mul_add(radius * 1.02, at.x),
            angle.sin().mul_add(radius * 1.02, at.y),
        );
        let size = (unit * 0.35).max(1.0);
        draw.draw_rectangle_v(
            Vector2::new(spot.x - size * 0.5, spot.y - size * 0.5),
            Vector2::new(size, size),
            if window % 3 == 0 { GOLD } else { CREAM },
        );
    }
    fill_disc(draw, frame, at, radius * 0.72, INK);
    fill_disc(draw, frame, at, radius * 0.66, STEEL_LIGHT);
    fill_disc(draw, frame, at, radius * 0.42, STEEL_DARK);
    // The port: a collar and a green light (steady; it brightens with a slow pulse).
    if let Some(port) = body.port() {
        let port_at = frame.project(port);
        let pulse = 160 + ((ticks % 120) as i32 - 60).unsigned_abs() as u8;
        draw.draw_line_ex(at, port_at, unit * 1.2, STEEL);
        let size = (unit * 0.6).max(2.0);
        draw.draw_rectangle_v(
            Vector2::new(port_at.x - size, port_at.y - size),
            Vector2::new(size * 2.0, size * 2.0),
            INK,
        );
        draw.draw_rectangle_v(
            Vector2::new(port_at.x - size * 0.6, port_at.y - size * 0.6),
            Vector2::new(size * 1.2, size * 1.2),
            with_alpha(MINT, pulse),
        );
        // The approach guide: a dashed line out of the port.
        let (dx, dy) = (port_at.x - at.x, port_at.y - at.y);
        let length = dx.hypot(dy).max(1.0);
        for dash in 1..8 {
            let from = dash as f32 * unit * 2.0;
            let start = Vector2::new(
                port_at.x + dx / length * from,
                port_at.y + dy / length * from,
            );
            let end = Vector2::new(
                port_at.x + dx / length * (from + unit),
                port_at.y + dy / length * (from + unit),
            );
            draw.draw_line_ex(start, end, (unit * 0.3).max(1.0), with_alpha(MINT, 110));
        }
    }
}

/// The scanner ping: a ring sweeping out from where it went out to the view's edge, for a
/// moment after it went out (a nebula's fog tints it).
fn draw_ping(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    snapshot: &RenderSnapshot<'_>,
    player: PlayerId,
) {
    let Some(ping) = snapshot.space.shared.pings.get(&player) else {
        return;
    };
    let age = snapshot.space.millis.saturating_sub(ping.at);
    if age > 1_600 {
        return;
    }
    let share = age as f32 / 1_600.0;
    let at = frame.project(ping.centre);
    let edge = frame.rect.width.hypot(frame.rect.height) * 0.6;
    let reach = edge * share.sqrt();
    let alpha = (200.0 * (1.0 - share)) as u8;
    let colour = if ping.fogged { LAVENDER } else { MINT };
    let width = (frame.rect.height / 240.0).max(2.0);
    draw.draw_ring(
        at,
        (reach - width).max(0.0),
        reach,
        0.0,
        360.0,
        96,
        with_alpha(colour, alpha),
    );
    draw.draw_ring(
        at,
        (reach * 0.8 - width * 0.5).max(0.0),
        reach * 0.8,
        0.0,
        360.0,
        96,
        with_alpha(colour, alpha / 3),
    );
}

/// The rig sprite drawn at `at`, turned `turn` radians on screen, at the view's rig scale.
#[allow(clippy::too_many_arguments, reason = "one rig at one place")]
fn draw_rig_at(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &fatherload_simulation::terrain::Terrain,
    facings: &super::world::RigFacings,
    presentation: &PlayerPresentation,
    at: Vector2,
    turn: f32,
    zoom: f32,
    ticks: u64,
) {
    let mut rig = presentation.clone();
    rig.x = 0.0;
    rig.y = RIG_SKY_Y;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.active_drill = None;
    let camera = Camera2D {
        offset: at,
        target: Vector2::new(0.0, RIG_SKY_Y + super::world::RIG_BODY_CENTRE_Y),
        rotation: turn.to_degrees(),
        zoom,
    };
    let mut scene = draw.begin_mode2D(camera);
    super::world::draw_rig(&mut scene, terrain, &rig, facings, ticks);
}

/// Another flyer: its rig (and a streak while it warps or cruises fast).
#[allow(clippy::too_many_arguments, reason = "one flyer in one view")]
fn draw_flyer(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &fatherload_simulation::terrain::Terrain,
    facings: &super::world::RigFacings,
    frame: &SpaceFrame,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    flyer: &Flyer,
    ticks: u64,
) {
    let at = frame.project(flyer.flight.pos);
    let speed = flyer.flight.speed();
    if speed > 2_000.0 && frame.visible(at, 400.0) {
        let (vx, vy) = frame.turn(flyer.flight.velocity_x, flyer.flight.velocity_y);
        let length = (speed * frame.scale * 0.1).clamp(30.0, 600.0);
        let colour = if matches!(flyer.flight.drive, StarDrive::Warp(_)) {
            ICE
        } else {
            SKY_LIGHT
        };
        for (width, alpha) in [(6.0, 40), (3.0, 110), (1.0, 230)] {
            draw.draw_line_ex(
                at,
                Vector2::new(at.x - vx / speed * length, at.y - vy / speed * length),
                width,
                with_alpha(colour, alpha),
            );
        }
    }
    if !frame.visible(at, 40.0) {
        return;
    }
    let Some(presentation) = snapshot
        .players
        .iter()
        .find(|player| player.player_id == flyer.player_id)
    else {
        return;
    };
    let density = frame.rect.height / view.viewport.height.max(1) as f32;
    let roll = frame.roll;
    draw_rig_at(
        draw,
        terrain,
        facings,
        presentation,
        at,
        flyer.orientation - roll,
        view.world_scale * density,
        ticks,
    );
}

/// The seat's own rig in the middle, upright (or turned, chart-up), with its drive's glow and
/// re-entry plasma.
#[allow(
    clippy::too_many_lines,
    reason = "the rig and its drive's and heat's glows"
)]
fn draw_own_rig(
    draw: &mut RaylibDrawHandle<'_>,
    terrain: &fatherload_simulation::terrain::Terrain,
    facings: &super::world::RigFacings,
    frame: &SpaceFrame,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    ticks: u64,
) {
    let density = frame.rect.height / view.viewport.height.max(1) as f32;
    let zoom = view.world_scale * density;
    let flight = view.player.flight;
    let turn = view.player.attitude.orientation - frame.roll;
    // The drive's glow behind the rig (cruise) and its spool's ring.
    let (sin, cos) = turn.sin_cos();
    let back = Vector2::new(-sin, cos);
    match flight.drive {
        StarDrive::Cruise { speed } => {
            let length = (speed / 2_000.0).clamp(4.0, 24.0) * zoom;
            for (width, alpha) in [(8.0, 50), (4.0, 120), (2.0, 230)] {
                draw.draw_line_ex(
                    Vector2::new(
                        frame.centre.x + back.x * 7.0 * zoom,
                        frame.centre.y + back.y * 7.0 * zoom,
                    ),
                    Vector2::new(
                        frame.centre.x + back.x * (7.0 * zoom + length),
                        frame.centre.y + back.y * (7.0 * zoom + length),
                    ),
                    width * zoom * 0.5,
                    with_alpha(SKY_LIGHT, alpha),
                );
            }
        }
        StarDrive::Spool { since, .. } => {
            let share = (snapshot.space.millis.saturating_sub(since) as f32
                / WARP_SPOOL_MILLIS as f32)
                .clamp(0.0, 1.0);
            let reach = (1.0 - share).mul_add(60.0, 18.0) * zoom;
            draw.draw_ring(
                frame.centre,
                reach - 2.0 * zoom,
                reach,
                0.0,
                360.0,
                64,
                with_alpha(ICE, (80.0 + 150.0 * share) as u8),
            );
        }
        StarDrive::Off | StarDrive::Warp(_) => {}
    }
    if let Some(presentation) = snapshot
        .players
        .iter()
        .find(|player| player.player_id == view.player_id)
    {
        draw_rig_at(
            draw,
            terrain,
            facings,
            presentation,
            frame.centre,
            turn,
            zoom,
            ticks,
        );
    }
    if flight.heat > 0.15 {
        // A bow of plasma on the leading face, brighter as the hull heats, trailing sparks.
        let speed = flight.speed().max(1.0);
        let (vx, vy) = frame.turn(flight.velocity_x / speed, flight.velocity_y / speed);
        let glow = (flight.heat / 1.5).clamp(0.2, 1.0);
        let heading = vy.atan2(vx).to_degrees();
        let lead = Vector2::new(
            frame.centre.x + vx * 6.0 * zoom,
            frame.centre.y + vy * 6.0 * zoom,
        );
        fill_disc(
            draw,
            frame,
            lead,
            30.0 * zoom,
            with_alpha(ORANGE, (50.0 * glow) as u8),
        );
        for (inner, outer, colour, alpha) in [
            (20.0, 32.0, RED, 100.0),
            (16.0, 24.0, ORANGE, 180.0),
            (13.0, 18.0, AMBER, 220.0),
            (11.5, 13.5, CREAM, 240.0),
        ] {
            draw.draw_ring(
                lead,
                inner * zoom,
                outer * zoom,
                heading - 75.0,
                heading + 75.0,
                24,
                with_alpha(colour, (alpha * glow) as u8),
            );
        }
        for spark in 0..10_u64 {
            let roll = space::hash64(0x9A57, spark, (ticks / 3) as i64, 0);
            let lateral = ((roll & 0xFF) as f32 / 255.0 - 0.5) * 22.0 * zoom;
            let back_by = ((roll >> 8) & 0xFF) as f32 / 255.0 * 30.0 * zoom + 8.0 * zoom;
            let at = Vector2::new(
                frame.centre.x - vx * back_by - vy * lateral,
                frame.centre.y - vy * back_by + vx * lateral,
            );
            let size = (zoom * 0.8).max(1.0);
            draw.draw_rectangle_v(
                at,
                Vector2::new(size, size),
                with_alpha(
                    if spark % 2 == 0 { ORANGE } else { AMBER },
                    (200.0 * glow) as u8,
                ),
            );
        }
    }
}

/// A warp's tunnel: streaks of light rushing out from the point ahead, the target star's glow
/// at its heart (steady, never flashing; fewer and dimmer with reduced effects).
#[allow(clippy::too_many_arguments, reason = "one view's tunnel")]
fn draw_warp_tunnel(
    draw: &mut RaylibDrawHandle<'_>,
    frame: &SpaceFrame,
    flight: &RigFlight,
    snapshot: &RenderSnapshot<'_>,
    target: fatherload_simulation::flight::SystemRef,
    ticks: u64,
    strength: f32,
) {
    let speed = flight.speed();
    let galaxy = snapshot.space.galaxy();
    let tint = galaxy.system(target).map_or(ICE, |system| {
        let [r, g, b] = system.class.colour();
        Color::new(r, g, b, 255)
    });
    // The point ahead: along the motion (the heading while aligned).
    let (ux, uy) = if speed > 1.0 {
        frame.turn(flight.velocity_x / speed, flight.velocity_y / speed)
    } else {
        (0.0, -1.0)
    };
    let span = frame.rect.width.hypot(frame.rect.height);
    let ahead = Vector2::new(
        frame.centre.x + ux * frame.rect.height * 0.32,
        frame.centre.y + uy * frame.rect.height * 0.32,
    );
    // The star ahead's glow.
    for (reach, alpha) in [(0.32, 22), (0.18, 40), (0.08, 90), (0.03, 200)] {
        fill_disc(
            draw,
            frame,
            ahead,
            frame.rect.height * reach,
            with_alpha(tint, alpha),
        );
    }
    let count = (110.0 * strength) as u64 + 20;
    let pace = 0.012 * (0.6 + strength);
    for line in 0..count {
        let roll = space::hash64(0x3A_7B, line, 0, 0);
        let angle = (roll & 0xFFFF) as f32 / 65_536.0 * std::f32::consts::TAU;
        let phase = ((roll >> 16) & 0xFFFF) as f32 / 65_536.0;
        // Out from the point ahead, accelerating (the streaks lengthen as they pass).
        let t = (ticks as f32).mul_add(pace, phase) % 1.0;
        let near = t * t * span * 0.75 + frame.rect.height * 0.04;
        let length = (t * t).mul_add(span * 0.25, 4.0);
        let (sin, cos) = angle.sin_cos();
        let start = Vector2::new(ahead.x + cos * near, ahead.y + sin * near);
        let end = Vector2::new(
            ahead.x + cos * (near + length),
            ahead.y + sin * (near + length),
        );
        let colour = if (roll >> 40).is_multiple_of(3) {
            tint
        } else {
            BONE
        };
        let alpha = (60.0 + 170.0 * t) * strength.max(0.5);
        draw.draw_line_ex(start, end, 1.0 + 2.0 * t, with_alpha(colour, alpha as u8));
    }
}

// --- The HUD ----------------------------------------------------------------------------------

/// Where the rig is, in words.
fn place_line(snapshot: &RenderSnapshot<'_>, flight: &RigFlight) -> String {
    let charts = snapshot.space.charts;
    let galaxy = snapshot.space.galaxy();
    let system = galaxy
        .system_at(space::galaxy_sector_of(flight.pos))
        .map_or_else(
            || "Deep space".to_owned(),
            |system| system_name(charts, system.system),
        );
    match flight.place {
        FlightPlace::Home | FlightPlace::Space => format!("OPEN SPACE  {}", system.to_uppercase()),
        FlightPlace::Approach(body) => format!(
            "APPROACH  {}",
            target_name(charts, NavTarget::Body(body)).to_uppercase()
        ),
        FlightPlace::Landed(body) => format!(
            "LANDED  {}",
            target_name(charts, NavTarget::Body(body)).to_uppercase()
        ),
        FlightPlace::Docked(body) => format!(
            "DOCKED  {}",
            target_name(charts, NavTarget::Body(body)).to_uppercase()
        ),
    }
}

/// The drive's mode and speed, in words.
fn drive_line(snapshot: &RenderSnapshot<'_>, flight: &RigFlight) -> (String, Color) {
    let tiles = flight.speed() / TILE_SIZE;
    match flight.drive {
        StarDrive::Off => (format!("IMPULSE  {tiles:.0} t/s"), MIST),
        StarDrive::Cruise { .. } => (format!("CRUISE  {} t/s", thousands(tiles)), SKY_LIGHT),
        StarDrive::Spool { since, .. } => {
            let left =
                WARP_SPOOL_MILLIS.saturating_sub(snapshot.space.millis.saturating_sub(since));
            (format!("WARP SPOOLING  {:.1}s", left as f32 / 1_000.0), ICE)
        }
        StarDrive::Warp(run) => {
            let share = snapshot.space.millis.saturating_sub(run.start) as f32
                / run.duration_millis.max(1) as f32;
            (format!("WARP  {:.0}%", (share * 100.0).min(100.0)), ICE)
        }
    }
}

/// `value` with thousands separators.
fn thousands(value: f32) -> String {
    let whole = value.max(0.0).round() as u64;
    let digits = whole.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Draws the flight HUD for `view` (UI canvas, after the space world): where the rig is and
/// the drive (top middle), its meters (top left, like the home HUD's stats), the nav panel
/// (top right), warnings, the docking guide, edge markers and the prompts.
#[allow(clippy::too_many_lines, reason = "the flight HUD's panels in order")]
pub(super) fn draw_space_hud(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    frame: &SpaceFrame,
) {
    let rect = super::screen::view_rect(view);
    let flight = view.player.flight;
    let player = view.player;
    let ui_per_physical = rect.width / frame.rect.width.max(1.0);
    let touch = view.seat_index == 0 && super::touch_controls::active();
    let narrow = rect.width < 420.0;
    // The meters, top left.
    let capacity = fatherload_simulation::flight::air_capacity(player.hazard_gear.life_support);
    let meters = Rectangle::new(rect.x + 4.0, rect.y + 4.0, 150.0, 60.0);
    ui_kit::panel(draw, meters, PanelStyle::Standard);
    let (x, mut y) = (meters.x as i32 + 5, meters.y as i32 + 5);
    let row_width = meters.width as i32 - 10;
    y += ui_kit::stat_bar(
        draw,
        x,
        y,
        row_width,
        &icons::HULL,
        "HULL",
        player.hull,
        player.max_hull(),
        ui_kit::BarColors::HULL,
        0.3,
    );
    y += ui_kit::stat_bar(
        draw,
        x,
        y,
        row_width,
        &icons::FUEL,
        "FUEL",
        player.fuel,
        player.fuel_capacity,
        ui_kit::BarColors::FUEL,
        0.2,
    );
    y += ui_kit::stat_bar(
        draw,
        x,
        y,
        row_width,
        &icons::AIR,
        "AIR",
        flight.air,
        capacity,
        ui_kit::BarColors::AIR,
        0.25,
    );
    let _ = ui_kit::stat_bar(
        draw,
        x,
        y,
        row_width,
        &icons::HEAT,
        "HEAT",
        flight.heat * 100.0,
        150.0,
        ui_kit::BarColors::DANGER,
        0.0,
    );
    // Where the rig is and the drive, top middle (below the touch top row on a touch screen).
    let where_line = place_line(snapshot, &flight);
    let (drive, drive_colour) = drive_line(snapshot, &flight);
    let plate_width = (ui_kit::text_width(&where_line, TextStyle::Bold)
        .max(ui_kit::text_width(&drive, TextStyle::Body))
        + 16) as f32;
    let plate_x = if narrow {
        meters.x + meters.width + 4.0
    } else {
        rect.x + (rect.width - plate_width) * 0.5
    };
    // A split-screen seat too narrow for both: the plate goes under the meters instead of
    // over them.
    let beside = plate_x.min(rect.x + rect.width - plate_width - 4.0);
    let clears = beside >= meters.x + meters.width + 4.0 || !narrow;
    let plate = Rectangle::new(
        if clears { beside } else { meters.x },
        match (touch, clears) {
            (_, false) => meters.y + meters.height + 4.0,
            (true, true) => rect.y + 50.0,
            (false, true) => rect.y + 4.0,
        },
        plate_width,
        28.0,
    );
    ui_kit::panel(draw, plate, PanelStyle::Standard);
    let middle = (plate.x + plate.width * 0.5) as i32;
    ui_kit::text_centred(
        draw,
        &where_line,
        middle,
        plate.y as i32 + 4,
        TextStyle::Bold,
        GOLD,
    );
    ui_kit::text_centred(
        draw,
        &drive,
        middle,
        plate.y as i32 + 15,
        TextStyle::Body,
        drive_colour,
    );
    // Warnings, early and plain (short in a narrow view).
    let air_share = flight.air / capacity.max(1.0);
    let warning = if flight.heat > fatherload_simulation::flight::HEAT_WARNING {
        Some(if narrow {
            "HULL HEATING"
        } else {
            "HULL HEATING: level out and slow down"
        })
    } else if air_share <= 0.25 {
        Some(if narrow {
            "AIR LOW: dock or beacon"
        } else {
            "AIR LOW: dock, land somewhere breathable, or call the beacon"
        })
    } else if player.fuel <= 0.0 {
        Some(if narrow {
            "OUT OF FUEL"
        } else {
            "OUT OF FUEL: the emergency beacon is on the nav chart"
        })
    } else {
        None
    };
    let mut top = plate.y + plate.height + 4.0;
    if let Some(text) = warning {
        let width = ui_kit::text_width(text, TextStyle::Bold) + 12;
        let banner = Rectangle::new(
            rect.x + (rect.width - width as f32) * 0.5,
            top,
            width as f32,
            15.0,
        );
        ui_kit::panel(draw, banner, PanelStyle::Standard);
        ui_kit::text_centred(
            draw,
            text,
            (banner.x + banner.width * 0.5) as i32,
            banner.y as i32 + 4,
            TextStyle::Bold,
            ORANGE,
        );
        top = banner.y + banner.height + 4.0;
    }
    let galaxy = snapshot.space.galaxy();
    let bodies = galaxy.bodies_near(flight.pos);
    draw_docking_guide(draw, rect, meters, view, &bodies);
    let nav = player.hazard_gear.flight.nav_suite > 0;
    let waypoint = snapshot
        .space
        .shared
        .waypoints
        .get(&view.player_id)
        .copied()
        .or(snapshot.space.shared.crew_waypoint);
    // Arrows stay clear of the top panels and the bottom prompts.
    let lanes = Rectangle::new(
        rect.x + 6.0,
        top + 4.0,
        rect.width - 12.0,
        rect.height - (top - rect.y) - 30.0,
    );
    if nav {
        if !narrow {
            draw_nav_panel(draw, rect, snapshot, view, frame, waypoint);
        }
        if let Some(target) = waypoint
            && let Some(pos) = target.position(&galaxy)
        {
            let name = target_name(snapshot.space.charts, target);
            edge_marker(
                draw,
                rect,
                lanes,
                frame,
                flight.pos,
                pos,
                &name,
                GOLD,
                ui_per_physical,
            );
        }
        for flyer in &snapshot.space.flyers {
            if flyer.player_id == view.player_id {
                continue;
            }
            let name = format!("P{}", flyer.player_id.get());
            edge_marker(
                draw,
                rect,
                lanes,
                frame,
                flight.pos,
                flyer.flight.pos,
                &name,
                MINT,
                ui_per_physical,
            );
        }
    } else {
        ui_kit::text(
            draw,
            "No nav suite: dock at a station to fit one",
            meters.x as i32,
            (meters.y + meters.height) as i32 + 3,
            TextStyle::Body,
            FOG,
        );
    }
    // The buttons that mean something out here.
    let mut hints = Vec::new();
    match flight.drive {
        StarDrive::Off => hints.push(hint(Prompt::Anchor, "cruise")),
        StarDrive::Cruise { .. } | StarDrive::Spool { .. } => {
            hints.push(hint(Prompt::Anchor, "drop cruise"));
        }
        StarDrive::Warp(_) => {}
    }
    hints.extend([hint(Prompt::Map, "nav chart"), hint(Prompt::Scan, "ping")]);
    if nav && matches!(waypoint, Some(NavTarget::System(_))) && !flight.drive.engaged() {
        hints.push(hint(Prompt::Bomb, "warp"));
    }
    if matches!(flight.place, FlightPlace::Docked(_)) {
        hints.push(hint(Prompt::Use, "undock"));
    }
    let device = shell.prompt_device(view.seat_index);
    if !touch {
        let rows = super::prompts::wrap_bar(&hints, device, rect.width as i32 - 12);
        let mut row_y =
            (rect.y + rect.height - 16.0) as i32 - (rows.len().saturating_sub(1) as i32) * 14;
        for row in rows {
            let width = super::prompts::bar_width(&row, device);
            super::prompts::draw_bar(
                draw,
                (rect.x + rect.width - width as f32 - 6.0) as i32,
                row_y,
                &row,
                device,
                MIST,
            );
            row_y += 14;
        }
    }
}

/// The docking guide near a station's port: range, speed and alignment, each ticked off
/// (under the meters).
fn draw_docking_guide(
    draw: &mut RaylibDrawHandle<'_>,
    rect: Rectangle,
    meters: Rectangle,
    view: &ViewSnapshot<'_>,
    bodies: &[FlightBody],
) {
    use fatherload_simulation::flight::{DOCK_ALIGN, DOCK_RANGE, DOCK_SPEED};
    let flight = view.player.flight;
    if matches!(flight.place, FlightPlace::Docked(_)) {
        return;
    }
    let Some((station, port)) = bodies
        .iter()
        .filter_map(|body| Some((body, body.port()?)))
        .find(|(_, port)| flight.pos.distance(*port) < 40.0 * TILE_SIZE)
    else {
        return;
    };
    let range = flight.pos.distance(port);
    let speed = flight.speed();
    let lean = fatherload_simulation::field::trig::wrap_angle(
        view.player.attitude.orientation - station.dock_orientation(),
    );
    let checks = [
        (
            format!("RANGE {:.1} t", range / TILE_SIZE),
            range <= DOCK_RANGE,
        ),
        (format!("SPEED {speed:.0}"), speed <= DOCK_SPEED),
        (
            format!("ALIGN {:+.0} deg", lean.to_degrees()),
            lean.abs() <= DOCK_ALIGN,
        ),
    ];
    let ready = checks.iter().all(|(_, ok)| *ok);
    let panel = Rectangle::new(
        meters.x,
        meters.y + meters.height + 16.0,
        meters.width,
        if ready { 62.0 } else { 50.0 },
    );
    if panel.y + panel.height > rect.y + rect.height - 20.0 {
        return;
    }
    ui_kit::panel(draw, panel, PanelStyle::Standard);
    ui_kit::text(
        draw,
        "DOCKING",
        panel.x as i32 + 6,
        panel.y as i32 + 4,
        TextStyle::Bold,
        MINT,
    );
    for (index, (text, ok)) in checks.iter().enumerate() {
        let y = panel.y as i32 + 15 + index as i32 * 10;
        ui_kit::text(
            draw,
            if *ok { "+" } else { "-" },
            panel.x as i32 + 6,
            y,
            TextStyle::Bold,
            if *ok { LIME } else { RED },
        );
        ui_kit::text(
            draw,
            text,
            panel.x as i32 + 16,
            y,
            TextStyle::Body,
            if *ok { BONE } else { FOG },
        );
    }
    if ready {
        let hints = [hint(Prompt::Use, "dock")];
        super::prompts::draw_bar(
            draw,
            panel.x as i32 + 6,
            panel.y as i32 + 46,
            &hints,
            super::prompts::menu_device(),
            LIME,
        );
    }
}

/// An arrow at the edge of `lanes` toward `to` (or a ring on it when it is in view), with a
/// name and distance beside it, inside the view.
#[allow(clippy::too_many_arguments, reason = "one marker")]
fn edge_marker(
    draw: &mut RaylibDrawHandle<'_>,
    rect: Rectangle,
    lanes: Rectangle,
    frame: &SpaceFrame,
    from: WorldPos,
    to: WorldPos,
    name: &str,
    colour: Color,
    ui_per_physical: f32,
) {
    let (dx, dy) = from.delta_f64(to);
    let distance = dx.hypot(dy);
    if !distance.is_finite() || distance < 1.0 {
        return;
    }
    let at = frame.project(to);
    let ui = Vector2::new(
        rect.x + (at.x - frame.rect.x) * ui_per_physical,
        rect.y + (at.y - frame.rect.y) * ui_per_physical,
    );
    let label = format!("{name} {}", nav::format_distance(distance));
    let label_width = ui_kit::text_width(&label, TextStyle::Body) as f32;
    let inside = ui.x > lanes.x + 12.0
        && ui.x < lanes.x + lanes.width - 12.0
        && ui.y > lanes.y + 12.0
        && ui.y < lanes.y + lanes.height - 12.0;
    if inside {
        draw.draw_ring(ui, 5.0, 6.5, 0.0, 360.0, 24, colour);
        let text_x = (ui.x + 9.0).min(lanes.x + lanes.width - label_width);
        ui_kit::text(
            draw,
            &label,
            text_x as i32,
            ui.y as i32 - 3,
            TextStyle::Body,
            colour,
        );
        return;
    }
    let (tx, ty) = frame.turn(dx as f32, dy as f32);
    let length = tx.hypot(ty).max(1.0);
    let (ux, uy) = (tx / length, ty / length);
    let centre = Vector2::new(lanes.x + lanes.width * 0.5, lanes.y + lanes.height * 0.5);
    let half_w = lanes.width * 0.5 - 8.0;
    let half_h = lanes.height * 0.5 - 8.0;
    let reach = (half_w / ux.abs().max(1e-3)).min(half_h / uy.abs().max(1e-3));
    let tip = Vector2::new(centre.x + ux * reach, centre.y + uy * reach);
    let left = Vector2::new(tip.x - ux * 9.0 - uy * 5.0, tip.y - uy * 9.0 + ux * 5.0);
    let right = Vector2::new(tip.x - ux * 9.0 + uy * 5.0, tip.y - uy * 9.0 - ux * 5.0);
    draw.draw_triangle(tip, right, left, colour);
    draw.draw_triangle(tip, left, right, colour);
    // The label inside the arrow, clear of it.
    let back = 14.0 + label_width * 0.5 * ux.abs() + 4.0 * uy.abs();
    let label_centre = Vector2::new(tip.x - ux * back, tip.y - uy * (back + 2.0));
    let text_x =
        (label_centre.x - label_width * 0.5).clamp(lanes.x, lanes.x + lanes.width - label_width);
    ui_kit::text(
        draw,
        &label,
        text_x as i32,
        (label_centre.y - 4.0) as i32,
        TextStyle::Body,
        colour,
    );
}

/// The nav panel (top right): the systems around on a little chart, the waypoint with its
/// distance and ETA, the warp's alignment.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "the nav panel's chart and readouts in order"
)]
fn draw_nav_panel(
    draw: &mut RaylibDrawHandle<'_>,
    rect: Rectangle,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    frame: &SpaceFrame,
    waypoint: Option<NavTarget>,
) {
    let flight = view.player.flight;
    let size = 92.0;
    let width = 136.0;
    let panel = Rectangle::new(
        rect.x + rect.width - width - 4.0,
        rect.y + 4.0,
        width,
        size + 40.0,
    );
    ui_kit::panel(draw, panel, PanelStyle::Standard);
    let chart = Rectangle::new(panel.x + (width - size) * 0.5, panel.y + 4.0, size, size);
    draw.draw_rectangle_rec(chart, SPACE);
    let centre = Vector2::new(chart.x + size * 0.5, chart.y + size * 0.5);
    // Three galactic sectors each way fill the chart.
    let reach = (3.5 * space::GALAXY_SECTOR_TILES as f32) * TILE_SIZE;
    let scale = size * 0.5 / reach;
    for ring in 1..=3 {
        draw.draw_circle_lines(
            centre.x as i32,
            centre.y as i32,
            size * 0.5 * ring as f32 / 3.5,
            with_alpha(SLATE, 160),
        );
    }
    let galaxy = snapshot.space.galaxy();
    let known = &snapshot.space.known;
    for system in galaxy.systems_near(flight.pos, 3) {
        let (dx, dy) = flight.pos.delta(system.star);
        let (x, y) = frame.turn(dx * scale, dy * scale);
        if x.abs() > size * 0.5 - 2.0 || y.abs() > size * 0.5 - 2.0 {
            continue;
        }
        let [r, g, b] = system.class.colour();
        let colour = if known.contains(&system.system) {
            Color::new(r, g, b, 255)
        } else {
            with_alpha(SLATE_LIGHT, 200)
        };
        let point = Vector2::new(centre.x + x, centre.y + y);
        draw.draw_rectangle(point.x as i32 - 1, point.y as i32 - 1, 2, 2, colour);
        if waypoint.is_some_and(|target| target.system() == system.system) {
            draw.draw_ring(point, 3.0, 4.0, 0.0, 360.0, 16, GOLD);
        }
    }
    // The rig, pointing up the panel when the camera is locked to it.
    let turn = view.player.attitude.orientation - frame.roll;
    let (sin, cos) = turn.sin_cos();
    let tip = Vector2::new(centre.x + sin * 4.0, centre.y - cos * 4.0);
    draw.draw_line_ex(centre, tip, 2.0, MINT);
    draw.draw_rectangle(centre.x as i32 - 1, centre.y as i32 - 1, 2, 2, BONE);
    let text_x = panel.x as i32 + 5;
    let text_width = width as i32 - 10;
    let mut y = (chart.y + size) as i32 + 3;
    let Some(target) = waypoint else {
        ui_kit::text(draw, "No waypoint", text_x, y, TextStyle::Bold, FOG);
        ui_kit::text(
            draw,
            "pick one on the chart",
            text_x,
            y + 11,
            TextStyle::Body,
            FOG,
        );
        return;
    };
    let name = ui_kit::truncate(
        &target_name(snapshot.space.charts, target),
        text_width,
        TextStyle::Bold,
    );
    ui_kit::text(draw, &name, text_x, y, TextStyle::Bold, GOLD);
    y += 11;
    let Some(pos) = target.position(&galaxy) else {
        return;
    };
    let (dx, dy) = flight.pos.delta_f64(pos);
    let distance = dx.hypot(dy);
    let far = matches!(target, NavTarget::System(system) if space::galaxy_sector_of(flight.pos) != system.sector);
    let eta = match flight.drive {
        // Mid-warp: what is left of the jump, then the cruise in from the system's edge.
        StarDrive::Warp(run) => {
            let left = (run.start + u64::from(run.duration_millis))
                .saturating_sub(snapshot.space.millis) as f32
                / 1_000.0;
            left + nav::eta_seconds(
                f64::from(fatherload_simulation::flight::physics::WARP_ARRIVAL),
                false,
            )
        }
        _ => nav::eta_seconds(distance, far),
    };
    let line = ui_kit::truncate(
        &format!(
            "{}  ETA {}",
            nav::format_distance(distance),
            nav::format_eta(eta)
        ),
        text_width,
        TextStyle::Body,
    );
    ui_kit::text(draw, &line, text_x, y, TextStyle::Body, BONE);
    y += 10;
    if far {
        let off = fatherload_simulation::field::trig::wrap_angle(
            view.player.attitude.orientation - nav::bearing(flight.pos, pos),
        );
        let (text, colour) = if matches!(flight.drive, StarDrive::Warp(_)) {
            ("IN WARP: hold the heading".to_owned(), SKY_LIGHT)
        } else if matches!(flight.drive, StarDrive::Spool { .. }) {
            ("SPOOLING: hold the heading".to_owned(), LIME)
        } else if off.abs() <= WARP_ALIGN {
            ("ALIGNED: warp ready".to_owned(), LIME)
        } else if off.abs() <= WARP_ABORT_ANGLE {
            (format!("ALIGN {:+.0} deg", off.to_degrees()), AMBER)
        } else {
            (format!("TURN {:+.0} deg", off.to_degrees()), FOG)
        };
        ui_kit::text(draw, &text, text_x, y, TextStyle::Body, colour);
    }
}

// --- The dialogs -------------------------------------------------------------------------------

/// The nav chart (the map button in space).
#[allow(clippy::too_many_lines, reason = "one dialog's rows")]
pub(super) fn draw_nav_chart(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let space = &ui.snapshot.space;
    let galaxy = space.galaxy();
    let player = ui.player;
    let waypoint = space
        .shared
        .waypoints
        .get(&ui.view.player_id)
        .copied()
        .or(space.shared.crew_waypoint);
    let rows = nav_rows_for(player, &space.known, waypoint, &galaxy);
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let flight = player.flight;
    let mut systems = Vec::new();
    let mut bodies = Vec::new();
    let mut actions = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let picked = index == selected;
        let entry = match row {
            NavRow::Target(target) => {
                let name = target_name(space.charts, *target);
                let pos = target.position(&galaxy);
                let distance = pos.map_or(f64::INFINITY, |pos| {
                    let (dx, dy) = flight.pos.delta_f64(pos);
                    dx.hypot(dy)
                });
                let detail = match target {
                    NavTarget::System(system) => {
                        galaxy.system(*system).map_or_else(String::new, |found| {
                            format!("{}, {} worlds", found.class.name(), found.worlds)
                        })
                    }
                    NavTarget::Body(_) => String::new(),
                };
                let is_waypoint = waypoint == Some(*target);
                ItemEntry {
                    icon: Some(match target {
                        NavTarget::System(_) => &menu_icons::GLOBE,
                        NavTarget::Body(_) => &menu_icons::FLAG,
                    }),
                    label: if is_waypoint {
                        format!("{name}  (waypoint)")
                    } else {
                        name
                    },
                    detail,
                    value: if distance.is_finite() {
                        nav::format_distance(distance)
                    } else {
                        String::new()
                    },
                    value_color: if is_waypoint { GOLD } else { MIST },
                    coin: false,
                    selected: picked,
                    disabled: false,
                }
            }
            NavRow::Warp => action_entry(
                &menu_icons::ROCKET,
                "Warp to the waypoint",
                "Face it, clear of gravity wells",
                picked,
            ),
            NavRow::Share => action_entry(
                &menu_icons::SIGNAL,
                "Share the waypoint with the crew",
                "",
                picked,
            ),
            NavRow::Clear => action_entry(&menu_icons::BACK, "Clear the waypoint", "", picked),
            NavRow::Station => action_entry(
                &menu_icons::DOOR,
                "Station services",
                "Fuel, repairs, flight gear",
                picked,
            ),
            NavRow::Beacon => {
                let fee = fatherload_simulation::flight::beacon_fee(player.credits);
                action_entry(
                    &menu_icons::INSURANCE,
                    "Emergency beacon",
                    &format!("Towed home for {fee} credits"),
                    picked,
                )
            }
            NavRow::Close => action_entry(&menu_icons::BACK, "Close", "", picked),
        };
        match row {
            NavRow::Target(NavTarget::System(_)) => systems.push(SectionItem::item(entry)),
            NavRow::Target(NavTarget::Body(_)) => bodies.push(SectionItem::item(entry)),
            _ => actions.push(SectionItem::item(entry)),
        }
    }
    let mut sections = Vec::new();
    if !systems.is_empty() {
        sections.push(Section::new(
            format!("Star systems ({})", systems.len()),
            GOLD,
            systems,
        ));
    }
    if !bodies.is_empty() {
        sections.push(Section::new("Nearby", SKY_LIGHT, bodies));
    }
    sections.push(Section::new("", FOG, actions));
    let footer = if player.hazard_gear.flight.nav_suite == 0 {
        "No nav suite fitted: only the way home is charted. Stations sell one.".to_owned()
    } else {
        "Pick a waypoint: the HUD points to it with its distance and ETA. Ping (the scanner) to \
         find more stars."
            .to_owned()
    };
    UiLayout::screen(draw).modal_with_render_plan(
        "Nav Chart",
        "Where to next?",
        &ModalContent::new(sections)
            .footer(Some(footer))
            .compact()
            .prompts(&[
                super::prompts::hint(Prompt::Navigate, "pick"),
                super::prompts::hint(Prompt::Confirm, "set"),
                super::prompts::hint(Prompt::Back, "close"),
            ]),
    );
}

fn action_entry(
    icon: &'static super::pixel_art::sprite::SpriteDef,
    label: &str,
    detail: &str,
    selected: bool,
) -> ItemEntry {
    ItemEntry {
        icon: Some(icon),
        label: label.to_owned(),
        detail: detail.to_owned(),
        value: String::new(),
        value_color: GOLD,
        coin: false,
        selected,
        disabled: false,
    }
}

/// A docked station's counter (the stub Wave 12 fills).
pub(super) fn draw_station_counter(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    use fatherload_simulation::flight::{STATION_FUEL_PRICE, STATION_REPAIR_PRICE};
    let player = ui.player;
    let rows = station_rows();
    let selected = ui.seat.selected_menu_item.min(rows.len().saturating_sub(1));
    let mut services = Vec::new();
    let mut gear = Vec::new();
    let mut leave = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let picked = index == selected;
        match row {
            StationRow::Refuel => {
                let missing = (player.fuel_capacity - player.fuel).max(0.0).ceil() as u32;
                services.push(SectionItem::item(ItemEntry {
                    icon: Some(&icons::FUEL),
                    label: "Refuel".to_owned(),
                    detail: format!("{missing} units"),
                    value: (missing * STATION_FUEL_PRICE).to_string(),
                    value_color: GOLD,
                    coin: true,
                    selected: picked,
                    disabled: missing == 0,
                }));
            }
            StationRow::Repair => {
                let missing = (player.max_hull() - player.hull).max(0.0).ceil() as u32;
                services.push(SectionItem::item(ItemEntry {
                    icon: Some(&icons::HULL),
                    label: "Repair".to_owned(),
                    detail: format!("{missing} hull"),
                    value: (missing * STATION_REPAIR_PRICE).to_string(),
                    value_color: GOLD,
                    coin: true,
                    selected: picked,
                    disabled: missing == 0,
                }));
            }
            StationRow::Buy(kind) => {
                let level = kind.level(&player.hazard_gear);
                let maxed = level >= kind.max_level();
                gear.push(SectionItem::item(ItemEntry {
                    icon: Some(&menu_icons::GEAR),
                    label: kind.name().to_owned(),
                    detail: if kind.max_level() > 1 {
                        format!("tier {level}/{}", kind.max_level())
                    } else if maxed {
                        "fitted (switch it on in Options)".to_owned()
                    } else {
                        "not fitted".to_owned()
                    },
                    value: if maxed {
                        "MAX".to_owned()
                    } else {
                        kind.price(level).to_string()
                    },
                    value_color: GOLD,
                    coin: !maxed,
                    selected: picked,
                    disabled: maxed || player.credits < kind.price(level),
                }));
            }
            StationRow::Save => leave.push(SectionItem::item(action_entry(
                &menu_icons::DISK,
                "Save the game",
                "",
                picked,
            ))),
            StationRow::Undock => leave.push(SectionItem::item(action_entry(
                &menu_icons::ROCKET,
                "Undock",
                "",
                picked,
            ))),
            StationRow::Close => leave.push(SectionItem::item(action_entry(
                &menu_icons::BACK,
                "Close",
                "",
                picked,
            ))),
        }
    }
    let name = match player.flight.place {
        FlightPlace::Docked(body) => target_name(ui.snapshot.space.charts, NavTarget::Body(body)),
        _ => "Station".to_owned(),
    };
    UiLayout::screen(draw).modal_with_render_plan(
        &name,
        "Docked. The air is topped up.",
        &ModalContent::new(vec![
            Section::new("Services", SKY_LIGHT, services),
            Section::new("Flight gear", GOLD, gear),
            Section::new("", FOG, leave),
        ])
        .footer(Some(format!("Credits: {}", player.credits)))
        .compact()
        .prompts(&[
            super::prompts::hint(Prompt::Navigate, "pick"),
            super::prompts::hint(Prompt::Confirm, "buy"),
            super::prompts::hint(Prompt::Back, "close"),
        ]),
    );
}
