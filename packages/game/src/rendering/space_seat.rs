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
    view::{CraftView, FlightMotion, GroundThing, GroundView, SpaceCamera, SpaceRenderView},
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
        StarDrive::Warp(_) => FlightMotion::Warp {
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
fn strip_actor(view: &ViewSnapshot<'_>, scene: &PlaceScene<'_>) -> ((f64, f64), f32) {
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
pub fn body_things(view: &ViewSnapshot<'_>, scene: &PlaceScene<'_>) -> Vec<GroundThing> {
    let on_foot = scene
        .pilots
        .iter()
        .any(|pilot| pilot.player_id == view.player_id);
    ground_things(scene, (!on_foot).then_some(view.player_id))
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
        px_per_tile,
    };
    let id = fatherload_simulation::place::SpaceBodyKey::mix(body.ground.id);
    let ground = GroundView {
        body: body_view(&body.body, id),
        strip,
        column: u,
        row: v as f32,
        roll,
        night_light: NIGHT_LIGHT,
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
