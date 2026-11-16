//! The furnished islands of a V9 sky (WP-SKY, T20), drawn around their tiles:
//!
//! * **Behind the terrain:** jet streams as ribbons of racing wind streaks along archipelago
//!   chains, and the mint columns of island vents breathing up.
//! * **In front of it:** sky flora on the turf (grass tufts, flowers in each palette's colour,
//!   frost sprigs, little bushes), waterfalls pouring from springs over the island's rim into
//!   the void (a wobbling sheet, falling droplets, spray at the lip and mist where it thins
//!   out), fuel buoys riding over their pads (a striped float on a tether, a blinking beacon),
//!   the slow violet pulse of intact keystones, and dust and pebbles raining from an island
//!   whose keystone is broken.
//!
//! Everything is a pure function of the world settings, the terrain, the view and the time,
//! and is skipped on worlds without furnished isles ([`GeneratorVersion::has_sky_isles`]).
//!
//! [`GeneratorVersion::has_sky_isles`]: fatherload_simulation::worldgen::GeneratorVersion::has_sky_isles

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    sky::{SkyBiome, SkyTile},
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        sky::{self, SkyLand, winds},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::terrain::tile_art::hash;

use super::{ART, Pen, snap};

/// Rows a waterfall falls before it has thinned into mist.
const FALL_ROWS: i32 = 70;

/// The tile rect of `visible` (world pixels), grown by `margin` tiles.
fn tiles_of(visible: Rectangle, margin: i32) -> TileRect {
    TileRect {
        min: TilePosition {
            x: (visible.x / TILE_SIZE).floor() as i32 - margin,
            y: (visible.y / TILE_SIZE).floor() as i32 - margin,
        },
        max: TilePosition {
            x: ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + margin,
            y: ((visible.y + visible.height) / TILE_SIZE).ceil() as i32 + margin,
        },
    }
}

/// Whether the view reaches the furnished island bands.
fn in_isle_bands(settings: &WorldSettings, visible: Rectangle) -> bool {
    let Some(bands) = settings.generator.sky_bands() else {
        return false;
    };
    settings.generator.has_sky_isles()
        && visible.y < -((bands.low_isles - 40) as f32) * TILE_SIZE
        && visible.y + visible.height > -(bands.dead_air as f32) * TILE_SIZE
}

fn tile_is(terrain: &Terrain, position: TilePosition, kind: TileKind) -> bool {
    terrain.tile(position).is_some_and(|tile| tile.kind == kind)
}

fn is_air(terrain: &Terrain, position: TilePosition) -> bool {
    terrain
        .tile(position)
        .is_none_or(|tile| tile.kind == TileKind::Air)
}

/// Jet stream ribbons and vent columns, behind the terrain.
pub fn draw_isle_backdrop(
    draw: &mut impl RaylibDraw,
    settings: &WorldSettings,
    visible: Rectangle,
    seconds: f32,
) {
    if !in_isle_bands(settings, visible) {
        return;
    }
    let rect = tiles_of(visible, 2);
    for stream in sky::jet_streams_touching(settings, &rect) {
        draw_jet_stream(draw, &stream, visible, seconds);
    }
    for land in sky::lands_touching(settings, &tiles_of(visible, winds::VENT_RISE_ROWS)) {
        if let Some(vent) = land.content().and_then(|content| content.vent) {
            draw_vent_column(draw, vent, visible, seconds);
        }
    }
}

/// Flora, waterfalls, buoys, keystones and collapse dust, in front of the terrain.
pub fn draw_isle_front(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    seconds: f32,
) {
    let settings = terrain.settings();
    if !in_isle_bands(settings, visible) {
        return;
    }
    // Waterfalls start above the view and fall into it, so look a fall's length up.
    let lands = sky::lands_touching(settings, &tiles_of(visible, FALL_ROWS));
    for land in &lands {
        let Some(content) = land.content() else {
            continue;
        };
        draw_flora(draw, terrain, land, visible, seconds);
        if let Some(spring) = content.spring
            && tile_is(terrain, spring, TileKind::Sky(SkyTile::Spring))
        {
            draw_waterfall(draw, terrain, land, spring, visible, seconds);
        }
        if let Some(pad) = content.buoy
            && tile_is(terrain, pad, TileKind::Sky(SkyTile::BuoyPad))
        {
            draw_buoy(draw, pad, seconds);
        }
        if let Some(keystone) = content.keystone {
            if tile_is(terrain, keystone, TileKind::Sky(SkyTile::Keystone)) {
                draw_keystone_glow(draw, keystone, seconds);
            } else {
                draw_collapse_dust(draw, land, visible, seconds);
            }
        }
    }
}

/// Grass tufts, flowers, sprigs and bushes along an island's untouched turf.
fn draw_flora(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    land: &SkyLand,
    visible: Rectangle,
    seconds: f32,
) {
    let first = land.center_x - land.half_width;
    let last = land.center_x + land.half_width;
    let (tip, blade, flower, accent) = match land.biome {
        SkyBiome::Meadow => (LIME, LEAF, CREAM, GOLD),
        SkyBiome::Blossom => (LIME, LEAF, PINK, ROSE),
        SkyBiome::Frost => (BONE, ICE, SKY_LIGHT, BONE),
        SkyBiome::Amber => (GOLD, AMBER, CREAM, ORANGE),
    };
    for x in first..=last {
        let Some(top) = land.surface_row(x) else {
            continue;
        };
        let world_x = x as f32 * TILE_SIZE;
        if world_x + TILE_SIZE < visible.x || world_x > visible.x + visible.width {
            continue;
        }
        let turf = TilePosition { x, y: top };
        if !matches!(
            terrain.tile(turf).map(|tile| tile.kind),
            Some(TileKind::Sky(SkyTile::Turf(_)))
        ) || !is_air(terrain, TilePosition { x, y: top - 1 })
        {
            continue;
        }
        let h = hash(x, top, 701);
        let pen = Pen::at(world_x, top as f32 * TILE_SIZE - 16.0 * ART);
        let sway = ((seconds * 1.3 + x as f32 * 0.7).sin() * 1.2) as i32;
        match h % 9 {
            0..=2 => {
                // A grass tuft.
                for blade_x in [3, 6, 9, 12] {
                    let height = 3 + ((h >> (blade_x as u32)) % 4) as i32;
                    pen.rect(draw, blade_x + sway, 16 - height, 1, height, blade);
                    pen.px(draw, blade_x + sway, 16 - height, tip);
                }
            }
            3 | 4 => {
                // Flowers on stalks.
                for (fx, height) in [(4, 6), (10, 4), (13, 7)] {
                    pen.rect(draw, fx + sway, 16 - height, 1, height, blade);
                    pen.rect(draw, fx - 1 + sway, 15 - height, 3, 2, flower);
                    pen.px(draw, fx + sway, 15 - height, accent);
                }
            }
            5 => {
                // A small round bush.
                pen.rect(draw, 3, 10, 10, 6, INK);
                pen.rect(draw, 4, 9, 8, 7, blade);
                pen.rect(draw, 5, 9, 4, 2, tip);
                if land.biome == SkyBiome::Blossom || land.biome == SkyBiome::Meadow {
                    pen.px(draw, 9, 11, flower);
                    pen.px(draw, 6, 13, flower);
                }
            }
            6 if land.biome == SkyBiome::Frost => {
                // A frost sprig of crystal.
                pen.rect(draw, 7, 9, 2, 7, ICE);
                pen.rect(draw, 5, 11, 2, 2, BONE);
                pen.rect(draw, 9, 12, 2, 2, SKY_LIGHT);
            }
            6 => {
                // A sky mushroom.
                pen.rect(draw, 7, 12, 2, 4, CREAM);
                pen.rect(draw, 5, 10, 6, 2, accent);
                pen.px(draw, 6, 10, BONE);
            }
            _ => {}
        }
    }
}

/// A waterfall pouring from `spring` over `land`'s rim, thinning into mist as it falls.
fn draw_waterfall(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    land: &SkyLand,
    spring: TilePosition,
    visible: Rectangle,
    seconds: f32,
) {
    let side = if spring.x >= land.center_x { 1 } else { -1 };
    // The sheet falls just outside the island's last column.
    let lip_x = if side > 0 {
        (spring.x + 1) as f32 * TILE_SIZE
    } else {
        spring.x as f32 * TILE_SIZE - 6.0 * ART
    };
    let lip_y = spring.y as f32 * TILE_SIZE + 8.0 * ART;
    let fall = FALL_ROWS as f32 * TILE_SIZE;
    if lip_y > visible.y + visible.height || lip_y + fall < visible.y {
        return;
    }
    // The water's first arc out of the spring, over the rim.
    let pen = Pen::at(lip_x, lip_y);
    for step in 0..4 {
        let dx = if side > 0 { step } else { 6 - step };
        pen.rect(draw, dx, step - 2, 6, 2, with_alpha(SKY_LIGHT, 230));
    }
    // The sheet: strips of water with a slow wobble, fading out down the fall.
    let rows = (fall / ART) as i32;
    let first = (((visible.y - lip_y) / ART).floor() as i32).max(0);
    let last = (((visible.y + visible.height - lip_y) / ART).ceil() as i32).min(rows);
    for row in first..last {
        let t = row as f32 / rows as f32;
        let alpha = (240.0 * (1.0 - t).powf(1.3)) as u8;
        if alpha < 8 {
            continue;
        }
        let y = lip_y + row as f32 * ART;
        // The sheet spreads a little as it falls.
        let width = 10.0 + t * 8.0;
        let wobble = (y * 0.05 + seconds * 2.0).sin() * t.mul_add(2.0, 1.0);
        let x = lip_x + wobble * ART - (width - 6.0) * 0.5 * ART;
        // Bright streaks scroll down the sheet.
        let streak = ((row as f32 * 0.7 - seconds * 22.0).rem_euclid(9.0)) < 2.0;
        let color = if streak { BONE } else { SKY_LIGHT };
        draw.draw_rectangle_rec(
            Rectangle::new(snap(x), snap(y), snap(width * ART), ART),
            with_alpha(color, alpha),
        );
        draw.draw_rectangle_rec(
            Rectangle::new(snap(x), snap(y), ART, ART),
            with_alpha(SKY, alpha),
        );
    }
    // Droplets peeling off the sheet.
    for drop in 0..14 {
        let h = hash(spring.x, drop, 703);
        let period = 1.4 + (h % 60) as f32 / 40.0;
        let t = ((seconds + (h >> 8) as f32 % 100.0 / 37.0) / period).fract();
        let y = lip_y + t * fall * 0.75;
        let drift = side as f32 * (2.0 + (h >> 16) as f32 % 10.0) * t * 8.0 * ART;
        let x = lip_x + 3.0 * ART + drift + ((h >> 20) % 7) as f32 * ART - 3.0 * ART;
        if y < visible.y || y > visible.y + visible.height {
            continue;
        }
        let alpha = (200.0 * (1.0 - t)) as u8;
        draw.draw_rectangle_rec(
            Rectangle::new(snap(x), snap(y), ART, ART * 2.0),
            with_alpha(BONE, alpha),
        );
    }
    // Spray where the water leaves the rim, and mist where the sheet thins out.
    for puff in 0..5 {
        let h = hash(spring.y, puff, 705);
        let t = ((seconds * 0.6 + puff as f32 / 5.0).fract()) * 1.0;
        let spray_x = lip_x + side as f32 * (t * 10.0) * ART + ((h % 6) as f32 - 2.0) * ART;
        let spray_y = lip_y - t * 6.0 * ART;
        draw.draw_rectangle_rec(
            Rectangle::new(snap(spray_x), snap(spray_y), ART * 2.0, ART * 2.0),
            with_alpha(BONE, (120.0 * (1.0 - t)) as u8),
        );
        let mist_y = lip_y + fall * (0.55 + 0.1 * puff as f32 / 5.0);
        let size = (5 + (h >> 8) % 6) as f32;
        let mist_x = lip_x + ((h >> 12) % 16) as f32 * ART - 6.0 * ART
            + (seconds * 3.0 + puff as f32).sin() * 3.0 * ART;
        draw.draw_rectangle_rec(
            Rectangle::new(snap(mist_x), snap(mist_y), size * ART, size * ART * 0.6),
            with_alpha(MIST, 40),
        );
    }
    let _ = terrain;
}

/// A fuel buoy riding over its pad: a striped float on a tether and a blinking beacon.
fn draw_buoy(draw: &mut impl RaylibDraw, pad: TilePosition, seconds: f32) {
    let bob = ((seconds * 1.6 + pad.x as f32).sin() * 2.0) as i32;
    let base_x = pad.x as f32 * TILE_SIZE;
    let base_y = pad.y as f32 * TILE_SIZE;
    let pen = Pen::at(base_x - 8.0 * ART, base_y - 64.0 * ART);
    // The mooring post and the tether up to the float.
    pen.rect(draw, 15, 50, 2, 14, STEEL_DARK);
    pen.rect(draw, 13, 62, 6, 2, INK);
    for step in 0..18 {
        let x = 16 + ((step as f32 * 0.6 + seconds * 2.0).sin() * 1.0) as i32;
        pen.px(draw, x, 32 + bob + step, LOAM);
    }
    // The float: a red-and-white striped drum with a brass cap.
    let top = 12 + bob;
    pen.rect(draw, 6, top, 20, 20, INK);
    for row in 0..18 {
        let color = if (row / 3) % 2 == 0 { RED } else { BONE };
        let inset = if row == 0 || row == 17 { 2 } else { 0 };
        pen.rect(draw, 7 + inset, top + 1 + row, 18 - inset * 2, 1, color);
    }
    pen.rect(draw, 8, top + 2, 2, 14, with_alpha(BONE, 120));
    pen.rect(draw, 12, top - 3, 8, 3, COPPER);
    pen.rect(draw, 15, top - 7, 2, 4, STEEL);
    // The beacon blinks amber; a fuel drop painted on the float.
    let blink = (seconds * 2.0) as i32 % 2 == 0;
    pen.rect(
        draw,
        14,
        top - 10,
        4,
        3,
        if blink { AMBER } else { with_alpha(AMBER, 90) },
    );
    if blink {
        pen.rect(draw, 12, top - 12, 8, 7, with_alpha(GOLD, 50));
    }
    pen.rect(draw, 14, top + 8, 4, 4, GOLD);
    pen.px(draw, 15, top + 7, GOLD);
    // A hose coiled at the post.
    pen.rect(draw, 18, 58, 6, 2, INK);
    pen.rect(draw, 18, 60, 2, 3, INK);
}

/// The slow violet pulse of an intact keystone, seen through the rock.
fn draw_keystone_glow(draw: &mut impl RaylibDraw, keystone: TilePosition, seconds: f32) {
    let pulse = (seconds * 1.7 + keystone.x as f32).sin() * 0.5 + 0.5;
    let cx = (keystone.x as f32 + 0.5) * TILE_SIZE;
    let cy = (keystone.y as f32 + 0.5) * TILE_SIZE;
    for ring in 0..3 {
        let radius = TILE_SIZE * (0.8 + ring as f32 * 0.5);
        let alpha = ((30.0 + 40.0 * pulse) / (ring as f32 + 1.0)) as u8;
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(cx - radius),
                snap(cy - radius),
                snap(radius * 2.0),
                snap(radius * 2.0),
            ),
            with_alpha(LAVENDER, alpha),
        );
    }
}

/// Dust and pebbles raining from the underside of an island whose keystone has broken.
fn draw_collapse_dust(
    draw: &mut impl RaylibDraw,
    land: &SkyLand,
    visible: Rectangle,
    seconds: f32,
) {
    let bounds = land.bounds();
    let bottom = (bounds.max.y + 1) as f32 * TILE_SIZE;
    let left = bounds.min.x as f32 * TILE_SIZE;
    let width = (bounds.max.x - bounds.min.x + 1) as f32 * TILE_SIZE;
    for mote in 0..24 {
        let h = hash(land.center_x, mote, 707);
        let period = 0.9 + (h % 50) as f32 / 40.0;
        let t = ((seconds + (h >> 8) as f32 % 50.0 / 13.0) / period).fract();
        let x = left + (h >> 12) as f32 % width.max(1.0);
        let y = bottom - TILE_SIZE * 4.0 + t * t * TILE_SIZE * 18.0;
        if y < visible.y || y > visible.y + visible.height {
            continue;
        }
        let (size, color) = if h.is_multiple_of(3) {
            (ART * 2.0, SLATE)
        } else {
            (ART, FOG)
        };
        draw.draw_rectangle_rec(
            Rectangle::new(snap(x), snap(y), size, size),
            with_alpha(color, (220.0 * (1.0 - t)) as u8),
        );
    }
}

/// A jet stream: a faint band of wind with long streaks racing along it.
fn draw_jet_stream(
    draw: &mut impl RaylibDraw,
    stream: &winds::JetStream,
    visible: Rectangle,
    seconds: f32,
) {
    let left = stream.rect.min.x as f32 * TILE_SIZE;
    let right = (stream.rect.max.x + 1) as f32 * TILE_SIZE;
    let top = stream.rect.min.y as f32 * TILE_SIZE;
    let bottom = (stream.rect.max.y + 1) as f32 * TILE_SIZE;
    if right < visible.x || left > visible.x + visible.width {
        return;
    }
    let band_left = left.max(visible.x - ART);
    let band_right = right.min(visible.x + visible.width + ART);
    // A soft band, strongest in the middle rows.
    let strips = 6;
    for strip in 0..strips {
        let t = (strip as f32 + 0.5) / strips as f32;
        let alpha = (22.0 * (1.0 - (t * 2.0 - 1.0).abs())) as u8;
        let y = top + (bottom - top) * strip as f32 / strips as f32;
        draw.draw_rectangle_rec(
            Rectangle::new(
                band_left,
                y,
                band_right - band_left,
                (bottom - top) / strips as f32,
            ),
            with_alpha(ICE, alpha),
        );
    }
    let speed = stream.velocity as f32 * 2.2;
    let length = right - left;
    let lanes = stream.rect.max.y - stream.rect.min.y + 1;
    for lane in 0..lanes * 2 {
        let y = top + lane as f32 * TILE_SIZE * 0.5 + ART;
        for streak in 0..((length / 90.0) as i32).max(2) {
            let h = hash(stream.rect.min.x + lane, streak, 709);
            let offset = (h % 997) as f32;
            let travel = (offset * 7.0 + seconds * speed).rem_euclid(length);
            let x = left + travel;
            if x < visible.x - 40.0 || x > visible.x + visible.width + 40.0 {
                continue;
            }
            // Fade in and out at the stream's ends (its taper).
            let from_end = (x - left).min(right - x) / (winds::JET_STREAM_TAPER as f32 * TILE_SIZE);
            let alpha = (170.0 * from_end.clamp(0.0, 1.0)) as u8;
            if alpha < 10 {
                continue;
            }
            let long = (8 + (h >> 12) % 14) as f32 * ART;
            let tail = if stream.velocity > 0 { -long } else { 0.0 };
            draw.draw_rectangle_rec(
                Rectangle::new(snap(x + tail), snap(y), snap(long), ART),
                with_alpha(BONE, alpha),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(snap(x + tail * 1.6), snap(y), snap(long * 0.6), ART),
                with_alpha(SKY_LIGHT, alpha / 3),
            );
        }
    }
}

/// The narrow column of rising air over an island vent.
fn draw_vent_column(
    draw: &mut impl RaylibDraw,
    vent: TilePosition,
    visible: Rectangle,
    seconds: f32,
) {
    let left = (vent.x - winds::VENT_HALF_WIDTH) as f32 * TILE_SIZE;
    let width = (winds::VENT_HALF_WIDTH * 2 + 1) as f32 * TILE_SIZE;
    let bottom = vent.y as f32 * TILE_SIZE;
    let height = winds::VENT_RISE_ROWS as f32 * TILE_SIZE;
    let top = bottom - height;
    if bottom < visible.y || top > visible.y + visible.height {
        return;
    }
    for band in 0..8 {
        let y = top + height * band as f32 / 8.0;
        draw.draw_rectangle_rec(
            Rectangle::new(left, y, width, height / 8.0),
            with_alpha(MINT, (4 + band * 3) as u8),
        );
    }
    for lane in 0..5 {
        let h = hash(vent.x, lane, 711);
        let speed = 160.0 + (h % 90) as f32;
        for mote in 0..5 {
            let phase = (seconds * speed + (h >> 8) as f32 % height + mote as f32 * height / 5.0)
                .rem_euclid(height);
            let y = bottom - phase;
            if y < visible.y || y > visible.y + visible.height {
                continue;
            }
            let swirl = (y * 0.04 + lane as f32).sin() * 3.0;
            let x = left + (lane as f32 + 0.5) * width / 5.0 + swirl * ART;
            let alpha = (60.0 + 140.0 * (phase / height).min(0.7)) as u8;
            draw.draw_rectangle_rec(
                Rectangle::new(snap(x), snap(y), ART, ART * 3.0),
                with_alpha(ICE, 255 - alpha),
            );
        }
    }
}

/// The nearest sky town (or unused fuel buoy) to a rig, for the sky gauge's navigation line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NearestSkyPlace {
    /// The town's name, or `None` for a buoy.
    pub town: Option<String>,
    /// Columns and rows from the rig (positive: east, down).
    pub dx: i32,
    pub dy: i32,
}

impl NearestSkyPlace {
    #[must_use]
    pub const fn is_town(&self) -> bool {
        self.town.is_some()
    }

    /// "Gale Moorings 140 m NE" / "Buoy 60 m W".
    #[must_use]
    pub fn line(&self) -> String {
        let distance = f64::from(self.dx).hypot(f64::from(self.dy)).round() as i64;
        let vertical = if self.dy < -4 {
            "N"
        } else if self.dy > 4 {
            "S"
        } else {
            ""
        };
        let horizontal = if self.dx > 4 {
            "E"
        } else if self.dx < -4 {
            "W"
        } else {
            ""
        };
        let name = self.town.as_deref().unwrap_or("Fuel buoy");
        format!("{name} {distance} m {vertical}{horizontal}")
    }
}

/// The nearest sky town within a long hop of world pixel `(x, y)` (else the nearest fuel
/// buoy in the low isles), `None` away from the island bands or on worlds without them.
#[must_use]
pub fn nearest_sky_place(
    settings: &WorldSettings,
    (x, y): (f32, f32),
    drained: &std::collections::BTreeSet<fatherload_simulation::worldgen::FeatureId>,
) -> Option<NearestSkyPlace> {
    if !settings.generator.has_sky_isles() {
        return None;
    }
    let bands = settings.generator.sky_bands()?;
    let rig = TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    };
    let altitude = -rig.y;
    if !(bands.low_isles - 80..=bands.dead_air).contains(&altitude) {
        return None;
    }
    let reach = TileRect::around(rig, 700, 420);
    let towns = sky::sky_towns_touching(settings, &reach);
    let distance = |px: i32, py: i32| {
        let (dx, dy) = (i64::from(px - rig.x), i64::from(py - rig.y));
        dx * dx + dy * dy
    };
    if let Some(site) = towns
        .iter()
        .min_by_key(|site| distance(site.center(), site.ground_row))
    {
        let name = fatherload_simulation::village::village_identity(settings, site.id)
            .map_or_else(|| site.kind.name().to_owned(), |(name, _)| name);
        return Some(NearestSkyPlace {
            town: Some(name),
            dx: site.center() - rig.x,
            dy: site.ground_row - rig.y,
        });
    }
    let buoys = sky::lands_touching(settings, &TileRect::around(rig, 400, 160));
    buoys
        .iter()
        .filter_map(sky::isles::buoy_of)
        .filter(|buoy| !drained.contains(&buoy.isle))
        .min_by_key(|buoy| distance(buoy.pad.x, buoy.pad.y))
        .map(|buoy| NearestSkyPlace {
            town: None,
            dx: buoy.pad.x - rig.x,
            dy: buoy.pad.y - rig.y,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_place_line_reads_like_a_signpost() {
        let place = NearestSkyPlace {
            town: Some("Gale Moorings".to_owned()),
            dx: 120,
            dy: -60,
        };
        assert_eq!(place.line(), "Gale Moorings 134 m NE");
        let buoy = NearestSkyPlace {
            town: None,
            dx: -50,
            dy: 2,
        };
        assert_eq!(buoy.line(), "Fuel buoy 50 m W");
    }

    #[test]
    fn a_rig_among_the_islands_hears_of_a_sky_town_or_a_buoy() {
        let settings = WorldSettings::new(0x5C1E);
        assert!(settings.generator.has_sky_isles());
        let site = sky::sky_town_site(&settings, 0, 1).expect("a town");
        let x = (site.center() + 200) as f32 * TILE_SIZE;
        let y = (site.ground_row - 40) as f32 * TILE_SIZE;
        let place = nearest_sky_place(&settings, (x, y), &std::collections::BTreeSet::new())
            .expect("a place");
        assert!(place.is_town());
        assert!(place.dx < 0, "{place:?}");
        assert!(
            nearest_sky_place(&settings, (0.0, -100.0), &std::collections::BTreeSet::new())
                .is_none()
        );
    }
}
