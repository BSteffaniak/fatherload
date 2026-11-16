//! Air you can ride in a V9 sky (expansion plan WP-SKY "wind-riding, updraft vents").
//!
//! * **Jet streams** ([`JetStream`]): every archipelago chain of two or more islands carries a
//!   ribbon of fast wind a few rows above its turf, from its westmost island to its eastmost,
//!   blowing one way along it. A rig (or a pilot, a drop, a bomb) that slips into the ribbon
//!   is carried along at up to [`JET_STREAM_SPEED`]: island hopping without the fuel.
//! * **Updraft vents** ([`vent_updraft_at`]): an island's vent ([`super::IsleContent::vent`])
//!   breathes a narrow column of rising air [`VENT_RISE_ROWS`] tall: a lift to the isle above.
//!
//! Both are pure functions of the cached island cells and feed the environment
//! ([`crate::environment::environment_at`]: `wind_x` and `updraft`) on V9 worlds only.

use crate::{
    terrain::TilePosition,
    worldgen::{TileRect, WorldSettings},
};

use super::{
    ARCHIPELAGO_CELL, ID_ARCHIPELAGO, ID_LOW, LOW_ISLE_CELL, SkyLand, band_cells, bands_of,
    cell_lands, cells,
};

/// Top speed of the air in a jet stream (px/s): a quick ride between islands.
pub const JET_STREAM_SPEED: i32 = 480;
/// Rows of a jet stream ribbon.
pub const JET_STREAM_ROWS: i32 = 10;
/// How hard a jet stream pulls an airborne rig toward its speed (per second): it reaches most
/// of the stream's speed within a second (`world/movement.rs`).
pub const JET_STREAM_GRIP: f32 = 2.2;
/// Columns over which a jet stream eases in and out at its ends.
pub const JET_STREAM_TAPER: i32 = 14;
/// Rows a vent's updraft rises above its island.
pub const VENT_RISE_ROWS: i32 = 60;
/// Upward acceleration of a vent's updraft (px/s², above a rig's gravity).
pub const VENT_STRENGTH: i32 = 860;
/// Columns either side of a vent its updraft covers.
pub const VENT_HALF_WIDTH: i32 = 1;

/// A ribbon of fast wind along an archipelago chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JetStream {
    pub rect: TileRect,
    /// Air velocity at full strength (px/s; positive blows east).
    pub velocity: i32,
}

impl JetStream {
    /// The air velocity (px/s) at tile `(x, y)` (0 outside), easing in and out at the ends.
    #[must_use]
    pub const fn velocity_at(&self, x: i32, y: i32) -> i32 {
        if !self.rect.contains(TilePosition { x, y }) {
            return 0;
        }
        let from_end = {
            let west = x - self.rect.min.x;
            let east = self.rect.max.x - x;
            if west < east { west } else { east }
        };
        if from_end < JET_STREAM_TAPER {
            self.velocity * (from_end + 1) / (JET_STREAM_TAPER + 1)
        } else {
            self.velocity
        }
    }
}

/// The jet stream of an archipelago cell's chain (`None` for fewer than two islands).
fn stream_of(lands: &[SkyLand]) -> Option<JetStream> {
    if lands.len() < 2 {
        return None;
    }
    let west = lands.iter().min_by_key(|land| land.center_x)?;
    let east = lands.iter().max_by_key(|land| land.center_x)?;
    // A few rows above the highest turf of the chain.
    let top = lands.iter().map(|land| land.top_row).min()?;
    let bottom = top - 3;
    // East or west from the shape of the westmost island.
    let velocity = if west.shape & 0x100 == 0 {
        JET_STREAM_SPEED
    } else {
        -JET_STREAM_SPEED
    };
    Some(JetStream {
        rect: TileRect {
            min: TilePosition {
                x: west.center_x,
                y: bottom - JET_STREAM_ROWS + 1,
            },
            max: TilePosition {
                x: east.center_x,
                y: bottom,
            },
        },
        velocity,
    })
}

/// Every jet stream overlapping `rect` (empty on generators without sky isles).
#[must_use]
pub fn jet_streams_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<JetStream> {
    let mut found = Vec::new();
    let Some(bands) = bands_of(settings).filter(|_| settings.generator.has_sky_isles()) else {
        return found;
    };
    for ry in band_cells(&bands, ID_ARCHIPELAGO, ARCHIPELAGO_CELL.1, rect) {
        for rx in cells(rect.min.x, rect.max.x, ARCHIPELAGO_CELL.0) {
            if let Some(stream) = stream_of(&cell_lands(settings, ID_ARCHIPELAGO, rx, ry))
                && stream.rect.intersects(rect)
            {
                found.push(stream);
            }
        }
    }
    found
}

/// The jet stream wind (px/s, positive east) at tile `(x, y)`: 0 outside every stream and on
/// generators without sky isles.
#[must_use]
pub fn jet_stream_at(settings: &WorldSettings, x: i32, y: i32) -> i32 {
    let rect = TileRect {
        min: TilePosition { x, y },
        max: TilePosition { x, y },
    };
    jet_streams_touching(settings, &rect)
        .iter()
        .map(|stream| stream.velocity_at(x, y))
        .sum()
}

/// The vent updraft (px/s² upward) at tile `(x, y)`: 0 away from vents and on generators
/// without sky isles.
#[must_use]
pub fn vent_updraft_at(settings: &WorldSettings, x: i32, y: i32) -> i32 {
    let Some(bands) = bands_of(settings).filter(|_| settings.generator.has_sky_isles()) else {
        return 0;
    };
    // Vents lie at most VENT_RISE_ROWS below the point.
    let rect = TileRect {
        min: TilePosition {
            x: x - VENT_HALF_WIDTH,
            y,
        },
        max: TilePosition {
            x: x + VENT_HALF_WIDTH,
            y: y + VENT_RISE_ROWS,
        },
    };
    let mut strength = 0;
    for (id_band, cell) in [(ID_LOW, LOW_ISLE_CELL), (ID_ARCHIPELAGO, ARCHIPELAGO_CELL)] {
        for ry in band_cells(&bands, id_band, cell.1, &rect) {
            for rx in cells(rect.min.x, rect.max.x, cell.0) {
                for land in cell_lands(settings, id_band, rx, ry).iter() {
                    let Some(vent) = land.content.and_then(|content| content.vent) else {
                        continue;
                    };
                    if (x - vent.x).abs() > VENT_HALF_WIDTH || y >= vent.y {
                        continue;
                    }
                    let above = vent.y - y;
                    if above > VENT_RISE_ROWS {
                        continue;
                    }
                    // Full strength low down, fading over the top third.
                    let fade = VENT_RISE_ROWS - above;
                    let taper = VENT_RISE_ROWS / 3;
                    strength = strength.max(if fade < taper {
                        VENT_STRENGTH * (fade + 1) / (taper + 1)
                    } else {
                        VENT_STRENGTH
                    });
                }
            }
        }
    }
    strength
}
