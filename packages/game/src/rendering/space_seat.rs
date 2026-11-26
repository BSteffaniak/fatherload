#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "screen geometry: f64 strip coordinates become f32 points"
)]

//! Each seat's view of space and of round bodies (Wave 11 integration): the space renderer
//! ([`super::space`]) fed with the real galaxy ([`super::space::live`]) for a seat in free
//! flight, and with its body's round ground (the polar track's strip from the regions' body
//! store) for a seat on a generated body. The camera is locked to the craft: the world turns
//! by the craft's own orientation (relative to the local up on a body), the craft is drawn
//! upright, and the HUD stays upright over it.

use fatherload_simulation::{TILE_SIZE, flight::StarDrive, position::WorldPos, world::strip_point};
use raylib::prelude::*;

use super::space::{
    live::{LiveSpace, LiveStrip, body_view, ground_things},
    view::{
        BodyView, CraftView, FlightMotion, GroundThing, GroundView, SpaceCamera, SpaceRenderView,
    },
};
use crate::render_snapshot::{PlaceBody, PlaceScene, RenderSnapshot, ViewSnapshot};

/// Ambient light a body's night side keeps (its ground stays readable).
const NIGHT_LIGHT: f32 = 0.45;

/// The craft (and its look) at a seat's camera.
fn craft_of(view: &ViewSnapshot<'_>) -> CraftView {
    CraftView {
        thrusting: view.player.velocity_y < -1.0,
        look: Some(super::rig_art::RigLook::from_player(
            view.player_id,
            view.player,
        )),
    }
}

/// The Star Drive's motion for the speed and warp effects.
fn motion_of(snapshot: &RenderSnapshot<'_>, view: &ViewSnapshot<'_>) -> FlightMotion {
    let flight = view.player.flight;
    let heading = flight.velocity_y.atan2(flight.velocity_x);
    match flight.drive {
        StarDrive::Cruise { speed } => FlightMotion::Cruise {
            speed: speed / TILE_SIZE,
            heading,
        },
        StarDrive::Spool { since, .. } => FlightMotion::Warp {
            charge: (snapshot.space.millis.saturating_sub(since) as f32 / 2_000.0).clamp(0.0, 1.0)
                * 0.4,
            heading,
        },
        // A wormhole's tunnel (Wave 12 systems) reads like a short, full warp.
        StarDrive::Warp(_) | StarDrive::Transit(_) => FlightMotion::Warp {
            charge: 1.0,
            heading,
        },
        StarDrive::Off => FlightMotion::Coasting,
    }
}

/// The camera of a seat in free flight, in the flight HUD's frame (`roll`, `px_per_tile` in
/// the frame's physical pixels).
#[must_use]
pub const fn flight_camera(view: &ViewSnapshot<'_>, roll: f32, px_per_tile: f32) -> SpaceCamera {
    SpaceCamera {
        centre: view.player.flight.pos,
        roll,
        px_per_tile,
        drop: 0.0,
        zoom: 1.0,
    }
}

/// A seat in free flight: the real galaxy round its rig (stations are flight's own, with
/// their ports and docking guides).
#[must_use]
pub fn flight_view<'a>(
    live: &'a LiveSpace,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    camera: SpaceCamera,
    seconds: f32,
) -> SpaceRenderView<'a> {
    live.view(
        camera,
        motion_of(snapshot, view),
        None,
        None,
        seconds,
        snapshot.effects,
        false,
    )
}

/// Where a seat on a body stands on its strip (`(u, v)`: columns and rows), and the turn of
/// its actor relative to the local up there.
pub fn strip_actor(view: &ViewSnapshot<'_>, scene: &PlaceScene<'_>) -> ((f64, f64), f32) {
    let on_foot = scene
        .pilots
        .iter()
        .find(|pilot| pilot.player_id == view.player_id);
    let (x, y, roll) = on_foot.map_or(
        (
            view.player.x,
            view.player.y,
            view.player.attitude.orientation,
        ),
        // The pilot stands up against gravity: its feet, upright.
        |pilot| (pilot.x, pilot.y - TILE_SIZE * 0.5, 0.0),
    );
    (strip_point(scene.frame.world(x, y)), roll)
}

/// The ground things of a seat's body (everyone but the seat's own rig when it flies it).
#[must_use]
pub fn body_things(
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    scene: &PlaceScene<'_>,
    seconds: f32,
) -> Vec<GroundThing> {
    let on_foot = scene
        .pilots
        .iter()
        .any(|pilot| pilot.player_id == view.player_id);
    let millis = snapshot.tick.saturating_mul(1_000) / u64::from(snapshot.tick_rate.hz().max(1));
    let mut things = ground_things(scene, (!on_foot).then_some(view.player_id), seconds, millis);
    // The galaxy story's waymarks on this world (Wave 13 story).
    things.extend(super::galaxy_hud::waymark_things(snapshot.world, scene));
    things
}

/// A seat on a generated body: its round ground under the craft, the galaxy above.
#[must_use]
#[allow(clippy::too_many_arguments, reason = "one seat's picture")]
pub fn body_view_of<'a>(
    live: &'a LiveSpace,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    scene: &PlaceScene<'_>,
    body: &PlaceBody,
    strip: &'a LiveStrip<'a>,
    things: &'a [GroundThing],
    px_per_tile: f32,
    seconds: f32,
) -> SpaceRenderView<'a> {
    let grid = body.ground.params.grid();
    let ((u, v), roll) = strip_actor(view, scene);
    let local = grid.local_frame(u);
    let on_foot = scene
        .pilots
        .iter()
        .any(|pilot| pilot.player_id == view.player_id);
    let camera = SpaceCamera {
        centre: grid.to_world(body.ground.centre, u, v),
        roll: roll + local.angle as f32,
        px_per_tile: px_per_tile * live.framing_zoom(),
        drop: live.drop,
        zoom: live.framing_zoom(),
    };
    let id = fatherload_simulation::place::SpaceBodyKey::mix(body.ground.id);
    // The body's own day (Wave 12 bodylife): the star where its clock puts it, its night as
    // dark as its sky keeps it (never unreadable).
    let ground = GroundView {
        body: BodyView {
            sun: live.sun_of(id),
            ..body_view(&body.body, id, false)
        },
        strip,
        column: u,
        row: v as f32,
        roll,
        night_light: body.life.as_ref().map_or(NIGHT_LIGHT, |life| {
            (life.night_light * 2.6).clamp(0.3, NIGHT_LIGHT)
        }),
        things,
    };
    live.view(
        camera,
        FlightMotion::Coasting,
        (!on_foot).then(|| craft_of(view)),
        Some(ground),
        seconds,
        snapshot.effects,
        true,
    )
}

/// Columns either side of a seat that its framing looks at for tall ground.
const HEADROOM_COLUMNS: i32 = 12;
/// Rows over the surface its framing looks up through.
const HEADROOM_ROWS: i32 = 72;
/// The most the craft drops below the view's middle (a share of the half height).
pub const MAX_DROP: f32 = 0.5;
/// The farthest the framing zooms out (tall ground past what a dropped craft shows).
pub const MIN_FRAMING_ZOOM: f32 = 0.6;

/// How far below the view's middle a seat on round ground should sit, and how far its view
/// zooms out, so the tall ground
/// round it (a jungle's giant trees and canopy, a crystal spire, a volcano's cone) shows
/// whole (Wave 13 art-audio): `(0, 1)` underground, in open country or when it already fits.
/// `half_rows` is how many rows the view shows above its middle.
#[must_use]
pub fn headroom_drop(scene: &PlaceScene<'_>, (u, v): (f64, f64), half_rows: f32) -> (f32, f32) {
    use fatherload_simulation::{planet::BodyGround, terrain::TileKind};
    let Some(planet) = scene.terrain.planet_ground() else {
        return (0.0, 1.0);
    };
    let width = i64::from(planet.strip_columns().max(1));
    let column = u.floor() as i64;
    let row = v.floor() as i32;
    let surface = planet.surface_row(column.rem_euclid(width) as i32);
    // Only on or over the open surface (underground the rock above is no sky to frame).
    if row > surface + 1 {
        return (0.0, 1.0);
    }
    let mut top = row;
    for dx in -HEADROOM_COLUMNS..=HEADROOM_COLUMNS {
        let x = (column + i64::from(dx)).rem_euclid(width) as i32;
        // From the seat's own row up: a spire or a tree counts however its column's ground
        // is measured.
        for y in (row - HEADROOM_ROWS)..row {
            let filled = scene
                .terrain
                .tile(fatherload_simulation::terrain::TilePosition { x, y })
                .is_some_and(|tile| tile.kind != TileKind::Air);
            if filled {
                top = top.min(y);
                break;
            }
        }
    }
    let tall = (row - top) as f32 + 2.0;
    let short = tall - half_rows;
    if short <= 0.0 {
        return (0.0, 1.0);
    }
    let drop = (short / half_rows.max(1.0)).clamp(0.0, MAX_DROP);
    // Still short with the craft dropped: widen the view (to two thirds at the most).
    let shown = half_rows * (1.0 + drop);
    let zoom = (shown / tall).clamp(MIN_FRAMING_ZOOM, 1.0);
    (drop, zoom)
}

/// The physical rectangle of a seat's viewport.
#[must_use]
pub fn physical_rect(view: &ViewSnapshot<'_>, density: f32) -> Rectangle {
    Rectangle::new(
        view.viewport.x as f32 * density,
        view.viewport.y as f32 * density,
        view.viewport.width as f32 * density,
        view.viewport.height as f32 * density,
    )
}

/// Points per tile at a seat's ground zoom (physical pixels).
#[must_use]
pub fn ground_px_per_tile(view: &ViewSnapshot<'_>, density: f32) -> f32 {
    TILE_SIZE * view.world_scale * density
}

/// The space position a body seat's camera looks at (for the galaxy's gathering).
#[must_use]
pub fn body_camera_centre(
    view: &ViewSnapshot<'_>,
    scene: &PlaceScene<'_>,
    body: &PlaceBody,
) -> WorldPos {
    let grid = body.ground.params.grid();
    let ((u, v), _) = strip_actor(view, scene);
    grid.to_world(body.ground.centre, u, v)
}
