//! Tomb runs in the world (WP-TOMBS, T27): the top-down crypt floor's placeholder look (worn
//! flagstones behind every open tile of a crypt run's slot, so the room grid reads as seen from
//! above), the trap-sight glow over hidden traps, a halo over the run's torches, and the
//! downed marker over fallen members.
//!
//! **T29** replaces the top-down floor with the crypt tile set and camera; **T28** adds side
//! view tile sets per culture. Both hook here (see `draw_tomb_floors`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "world drawing mixes tile indices and float pixels"
)]

use fatherload_simulation::{
    PlayerId,
    base::BuiltTile,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    tomb::{SiteStyle, Tombs},
    worldgen::tomb_sites,
};
use raylib::prelude::*;

use crate::render_snapshot::PilotPresentation;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

/// The tiles of `visible` (world pixels), clamped to `rect`.
fn tiles_in(visible: Rectangle) -> (i32, i32, i32, i32) {
    (
        (visible.x / TILE_SIZE).floor() as i32 - 1,
        (visible.y / TILE_SIZE).floor() as i32 - 1,
        ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 1,
        ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + 1,
    )
}

/// Crypt floors: flagstones over every open tile of a top-down run's slot (drawn after the
/// terrain, so the dark back wall of open tiles becomes a floor seen from above).
pub(super) fn draw_tomb_floors(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    tombs: &Tombs,
    explored: Option<&fatherload_simulation::DiscoveredTiles>,
    visible: Rectangle,
) {
    if tombs.runs.is_empty() {
        return;
    }
    let (left, top, right, bottom) = tiles_in(visible);
    for run in tombs.runs.values() {
        if run.site.style != SiteStyle::TopDown {
            continue;
        }
        let origin = tomb_sites::slot_origin(run.slot);
        let (x0, y0) = (origin.x.max(left), origin.y.max(top));
        let (x1, y1) = (
            (origin.x + tomb_sites::SLOT_SIZE.0).min(right),
            (origin.y + tomb_sites::SLOT_SIZE.1).min(bottom),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let tile = TilePosition { x, y };
                // Open floor, and hidden traps (they look like floor until trap sight shows them).
                let open = matches!(
                    terrain.tile(tile).map(|found| found.kind),
                    Some(TileKind::Air | TileKind::Built(BuiltTile::TombTrap { hidden: true, .. }))
                );
                if !open || explored.is_some_and(|explored| !explored.contains(tile)) {
                    continue;
                }
                let (px, py) = (x as f32 * TILE_SIZE, y as f32 * TILE_SIZE);
                let checker = (x + y).rem_euclid(2) == 0;
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(TILE_SIZE, TILE_SIZE),
                    if checker {
                        Color::new(72, 64, 86, 255)
                    } else {
                        Color::new(62, 55, 76, 255)
                    },
                );
                // Grout lines.
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(TILE_SIZE, 2.0),
                    Color::new(40, 34, 52, 255),
                );
                draw.draw_rectangle_v(
                    Vector2::new(px, py),
                    Vector2::new(2.0, TILE_SIZE),
                    Color::new(40, 34, 52, 255),
                );
            }
        }
    }
}

/// Whether the view of `seat`'s player shows hidden traps: only that player's own pilot
/// carrying the Trap-Sight Lantern lights them (per seat: in split screen a crewmate's lantern
/// does not reveal them on the other screen).
pub(super) fn seat_trap_sight<'a>(
    pilots: impl IntoIterator<Item = (PlayerId, &'a fatherload_simulation::pilot::PilotState)>,
    seat: Option<PlayerId>,
) -> bool {
    seat.is_some_and(|seat| {
        pilots
            .into_iter()
            .any(|(owner, state)| owner == seat && state.boons.trap_sight)
    })
}

/// [`seat_trap_sight`] over drawn pilots.
pub(super) fn seat_trap_sight_of(pilots: &[PilotPresentation], seat: Option<PlayerId>) -> bool {
    seat_trap_sight(
        pilots.iter().map(|pilot| (pilot.player_id, &pilot.state)),
        seat,
    )
}

/// Hidden traps glow for the seat whose pilot carries the Trap-Sight Lantern (`trap_sight`,
/// [`seat_trap_sight`]); open shrines and caches get a soft halo so the run's landmarks read
/// in the dark.
pub(super) fn draw_tomb_marks(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    tombs: &Tombs,
    trap_sight: bool,
    visible: Rectangle,
    seconds: f32,
) {
    if tombs.runs.is_empty() {
        return;
    }
    let (left, top, right, bottom) = tiles_in(visible);
    let pulse = (seconds * 3.0).sin().mul_add(0.25, 0.75);
    for y in top..bottom {
        for x in left..right {
            let tile = TilePosition { x, y };
            if !tomb_sites::in_vault(terrain.settings(), tile) {
                continue;
            }
            let (cx, cy) = ((x as f32 + 0.5) * TILE_SIZE, (y as f32 + 0.5) * TILE_SIZE);
            match terrain.tile(tile).map(|found| found.kind) {
                // (A dart thrower's hidden bit is its facing, T28: it is never hidden.)
                Some(TileKind::Built(BuiltTile::TombTrap { hidden: true, kind }))
                    if trap_sight
                        && fatherload_simulation::tomb::realize::TrapKind::from_index(kind)
                            != fatherload_simulation::tomb::realize::TrapKind::DartWall =>
                {
                    draw.draw_rectangle_lines_ex(
                        Rectangle::new(
                            x as f32 * TILE_SIZE + 2.0,
                            y as f32 * TILE_SIZE + 2.0,
                            TILE_SIZE - 4.0,
                            TILE_SIZE - 4.0,
                        ),
                        2.0,
                        Color::new(255, 120, 90, (200.0 * pulse) as u8),
                    );
                }
                Some(TileKind::Built(
                    BuiltTile::TombShrine { .. }
                    | BuiltTile::TombStairs
                    | BuiltTile::TombCache { open: false },
                )) => {
                    draw.draw_circle_v(
                        Vector2::new(cx, cy),
                        TILE_SIZE * 1.1,
                        Color::new(255, 200, 110, (34.0 * pulse) as u8),
                    );
                }
                _ => {}
            }
        }
    }
}

/// A red "DOWN" ring over every downed member's pilot.
pub(super) fn draw_downed(
    draw: &mut impl RaylibDraw,
    tombs: &Tombs,
    pilots: &[PilotPresentation],
    seconds: f32,
) {
    for run in tombs.runs.values() {
        for member in run.members.iter().filter(|member| member.is_downed()) {
            let Some(pilot) = pilots.iter().find(|pilot| pilot.player_id == member.player) else {
                continue;
            };
            let (x, y) = pilot.middle();
            let pulse = (seconds * 4.0).sin().mul_add(4.0, 22.0);
            draw.draw_ring(
                Vector2::new(x, y),
                pulse - 3.0,
                pulse,
                0.0,
                360.0,
                24,
                Color::new(230, 60, 60, 210),
            );
            draw.draw_rectangle_v(
                Vector2::new(x - 2.0, y - 40.0),
                Vector2::new(4.0, 10.0),
                RED,
            );
            draw.draw_rectangle_v(Vector2::new(x - 2.0, y - 27.0), Vector2::new(4.0, 4.0), RED);
        }
    }
}

#[cfg(test)]
mod tests {
    use fatherload_simulation::pilot::{PilotGear, PilotState};

    use super::*;

    #[test]
    fn trap_sight_is_per_seat() {
        let (lantern, plain) = (PlayerId::new(1), PlayerId::new(2));
        let mut lit = PilotState::new(lantern, PilotGear::default(), 1);
        lit.boons.trap_sight = true;
        let dark = PilotState::new(plain, PilotGear::default(), 1);
        let pilots = [(lantern, &lit), (plain, &dark)];
        // The lantern carrier's screen shows hidden traps; the crewmate's (split screen) does
        // not, and neither does a view with no player.
        assert!(seat_trap_sight(pilots, Some(lantern)));
        assert!(!seat_trap_sight(pilots, Some(plain)));
        assert!(!seat_trap_sight(pilots, None));
    }
}
