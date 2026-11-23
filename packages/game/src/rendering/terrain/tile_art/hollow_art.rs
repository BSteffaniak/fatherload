//! Tile art for Descent V, the Hollow World (Wave 9, `fatherload_simulation::hollow`).
//!
//! Each stratum has its own materials: pale wind-scoured sky stone streaked with grain and
//! studded with honey sky amber, and the Fathers' weathered winch planks (Inner Sky); dark
//! loam threaded with rootlets, ferns curling over the hall floors, ridged bark, leafy crowns,
//! glossy black tar and ivory giant bones (Megafauna Jungle); warm layered isle rock with
//! grass growing *down* off its undersides, deep-blue lodestone with ice glints and braided
//! stalks (Upside-Down Isles); bright sun-baked rock with rust cracks, faceted sun glass and
//! ember-veined slag (the Sunforge); and the Sun Well's white-gold builder stone, its obsidian
//! pillars banded with gold, and the inner sun itself, a blazing swirl. The jungle tribes'
//! totems are carved faces with green eyes.
//!
//! Open air in the three great open caverns (the Inner Sky, the isle sea and the forge sky)
//! paints transparent, so the hollow backdrop (`rendering::world::hollow`) shows through: the
//! inner sky, its sun and its day. The jungle's halls show a back wall of their own.

#![allow(
    clippy::suboptimal_flops,
    clippy::while_float,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::bool_to_int_with_if,
    clippy::manual_range_contains,
    reason = "presentation code: art and sound math, layout passes read top to bottom"
)]

use fatherload_simulation::{
    descent::Stratum,
    hollow::HollowTile,
    terrain::{Terrain, TilePosition},
    worldgen::descent::{self as layout_rules, hollow as places},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp, worley,
};

const SKY_STONE: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, BONE]);
const AMBER_RAMP: Ramp = ramp([RUST, ORANGE, AMBER, GOLD]);
const TIMBER: Ramp = ramp([SOIL_DARK, SOIL, LOAM, SAND]);
const LOAM_RAMP: Ramp = ramp([INK, SOIL_DARK, SOIL, LOAM]);
const LEAVES: Ramp = ramp([PINE, MOSS, LEAF, LIME]);
const BARK: Ramp = ramp([SOIL_DARK, SOIL, CLAY, LOAM]);
const TAR: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const BONE_RAMP: Ramp = ramp([STONE_DARK, FOG, CREAM, BONE]);
const ISLE: Ramp = ramp([SOIL, CLAY, SAND, CREAM]);
const LODE: Ramp = ramp([NIGHT, DEEP_BLUE, OCEAN, SKY]);
const SUN_ROCK: Ramp = ramp([CLAY, SAND, AMBER, CREAM]);
const SUN_GLASS: Ramp = ramp([AMBER, GOLD, CREAM, BONE]);
const SLAG: Ramp = ramp([INK, MAROON, RUST, ORANGE]);
const WELL: Ramp = ramp([SAND, CREAM, BONE, GOLD]);
const PILLAR: Ramp = ramp([INK, NIGHT, PLUM, GOLD]);
const TOTEM: Ramp = ramp([SOIL_DARK, RUST, ORANGE, GOLD]);

/// Representative colour of a Hollow World tile (drill dust, debris, the map).
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "one colour per tile, grouped by look"
)]
pub const fn hollow_color(tile: HollowTile) -> raylib::prelude::Color {
    match tile {
        HollowTile::SkyStone => STONE_LIGHT,
        HollowTile::SkyAmber => AMBER,
        HollowTile::BreachTimber => LOAM,
        HollowTile::Loam => SOIL,
        HollowTile::Fern => MOSS,
        HollowTile::Trunk => CLAY,
        HollowTile::Canopy => LEAF,
        HollowTile::Tar => NIGHT,
        HollowTile::GiantBone => BONE,
        HollowTile::IsleRock => SAND,
        HollowTile::IsleGrass => LEAF,
        HollowTile::Lodestone => OCEAN,
        HollowTile::IsleStalk => CLAY,
        HollowTile::SunRock => SAND,
        HollowTile::SunGlass => GOLD,
        HollowTile::Slag => RUST,
        HollowTile::WellWall => CREAM,
        HollowTile::ShadePillar => PLUM,
        HollowTile::InnerSun => BONE,
        HollowTile::Totem => ORANGE,
    }
}

/// The body pattern of `tile` at art pixel `(x, y)` (absolute art coordinates).
#[allow(
    clippy::too_many_lines,
    reason = "one pattern per Hollow World material"
)]
fn body_pixel(tile: HollowTile, x: i32, y: i32) -> PaletteIndex {
    match tile {
        HollowTile::SkyStone | HollowTile::SkyAmber => {
            // Wind-scoured stone: long horizontal grain streaks over soft mottling.
            let streak = (y + (noise(x, y, 18, 901) * 3.0) as i32).rem_euclid(5);
            let mottle = noise(x, y, 6, 903);
            SKY_STONE[if streak == 0 {
                3
            } else if mottle > 0.62 {
                2
            } else if mottle < 0.25 {
                0
            } else {
                1
            }]
        }
        HollowTile::BreachTimber => {
            // Weathered planks: grain along the board, a dark seam every few pixels.
            let seam = x.rem_euclid(8) == 0;
            if seam {
                TIMBER[0]
            } else if hash(x, y, 905).is_multiple_of(13) {
                TIMBER[3]
            } else {
                TIMBER[1 + usize::from((y + x / 8).rem_euclid(3) == 0)]
            }
        }
        HollowTile::Loam | HollowTile::Fern | HollowTile::Tar | HollowTile::GiantBone => {
            // Dark crumbly loam with pale rootlets wandering through it.
            let root = (x + (noise(x, y, 9, 907) * 6.0) as i32).rem_euclid(11) == 0
                && noise(x, y, 5, 909) > 0.45;
            if root {
                index_of(LOAM)
            } else if hash(x, y, 911).is_multiple_of(23) {
                LOAM_RAMP[0]
            } else {
                LOAM_RAMP[1 + usize::from(noise(x, y, 4, 913) > 0.55)]
            }
        }
        HollowTile::Trunk => {
            // Ridged bark: vertical ridges with deep furrows and moss patches.
            let ridge = (x + (noise(x, y, 12, 915) * 2.0) as i32).rem_euclid(5);
            if noise(x, y, 7, 917) > 0.72 {
                LEAVES[1]
            } else {
                BARK[match ridge {
                    0 => 0,
                    1 | 4 => 1,
                    2 => 3,
                    _ => 2,
                }]
            }
        }
        HollowTile::Canopy => {
            // Leaf clumps: round clusters lit from the top-left, dark between them.
            let (near, second, dx, dy) = worley(x, y, 5, 919, false);
            if second - near < 0.8 {
                LEAVES[0]
            } else if dx + dy <= -2 {
                LEAVES[3]
            } else if dx + dy <= 1 {
                LEAVES[2]
            } else {
                LEAVES[1]
            }
        }
        HollowTile::IsleRock | HollowTile::IsleGrass | HollowTile::Lodestone => {
            // Warm sandstone strata, gently curving.
            let band = (y + (noise(x, y, 16, 921) * 4.0) as i32).rem_euclid(6);
            if hash(x, y, 923).is_multiple_of(31) {
                ISLE[3]
            } else {
                ISLE[match band {
                    0 => 0,
                    1 | 2 => 1,
                    _ => 2,
                }]
            }
        }
        HollowTile::IsleStalk => {
            // Braided stone: two strands twisting round each other.
            let twist = (x + y / 2).rem_euclid(6);
            ISLE[if twist < 2 {
                0
            } else if twist < 4 {
                1
            } else {
                2
            }]
        }
        HollowTile::SunRock => {
            // Bright baked rock crazed with rust cracks.
            let (near, second, _, _) = worley(x, y, 7, 925, true);
            if second - near < 0.7 {
                index_of(RUST)
            } else {
                SUN_ROCK[if noise(x, y, 5, 927) > 0.6 {
                    3
                } else {
                    1 + usize::from(bayer4(x, y) > 9)
                }]
            }
        }
        HollowTile::SunGlass => {
            let (near, second, dx, dy) = worley(x, y, 5, 929, false);
            if second - near < 0.8 {
                SUN_GLASS[0]
            } else if dx + dy <= -3 {
                SUN_GLASS[3]
            } else if dx + dy <= 0 {
                SUN_GLASS[2]
            } else {
                SUN_GLASS[1]
            }
        }
        HollowTile::Slag => {
            // A dark crust split by glowing cracks.
            let (near, second, _, _) = worley(x, y, 6, 931, true);
            if second - near < 0.6 {
                index_of(if hash(x, y, 933).is_multiple_of(3) {
                    GOLD
                } else {
                    ORANGE
                })
            } else {
                SLAG[usize::from(noise(x, y, 4, 935) > 0.5)]
            }
        }
        HollowTile::WellWall => {
            // White-gold builder ashlar: big blocks, thin seams.
            let seam = x.rem_euclid(16) == 0 || y.rem_euclid(8) == 0;
            if seam {
                WELL[0]
            } else {
                WELL[1 + usize::from(noise(x, y, 6, 937) > 0.55)]
            }
        }
        HollowTile::ShadePillar => {
            // Polished obsidian, a gold band every so often.
            if y.rem_euclid(12) < 2 {
                PILLAR[3]
            } else if (x.rem_euclid(TILE_ART)) < 3 {
                PILLAR[2]
            } else {
                PILLAR[usize::from(hash(x, y, 939).is_multiple_of(17))]
            }
        }
        HollowTile::InnerSun => {
            // A blazing swirl of white and gold.
            let swirl = noise(x + y / 2, y - x / 3, 6, 941);
            if swirl > 0.68 {
                index_of(BONE)
            } else if swirl > 0.4 {
                index_of(CREAM)
            } else if swirl > 0.2 {
                index_of(GOLD)
            } else {
                index_of(AMBER)
            }
        }
        HollowTile::Totem => TOTEM[1 + usize::from(y.rem_euclid(6) < 3)],
    }
}

fn paint_amber(canvas: &mut TileCanvas, pick: u32) {
    // Two or three honey nodules with a bright highlight.
    for i in 0..3 {
        if i == 2 && pick.is_multiple_of(2) {
            break;
        }
        let cx = 3 + ((pick >> (i * 4)) % 10) as i32;
        let cy = 3 + ((pick >> (i * 4 + 12)) % 10) as i32;
        for py in -2..=2 {
            for px in -2..=2 {
                let d = px * px + py * py;
                if d <= 4 {
                    canvas.set(cx + px, cy + py, AMBER_RAMP[if d <= 1 { 2 } else { 1 }]);
                }
            }
        }
        canvas.set(cx - 1, cy - 1, AMBER_RAMP[3]);
    }
}

fn paint_fern(canvas: &mut TileCanvas, pick: u32) {
    // A turf of fern fronds over the loam: arching stems with leaflets.
    for py in 0..4 {
        for px in 0..TILE_ART {
            canvas.set(px, py, LEAVES[if py == 0 { 2 } else { 1 }]);
        }
    }
    for frond in 0..3 {
        let base = 2 + frond * 5 + ((pick >> (frond * 3)) % 3) as i32;
        for step in 0..5 {
            let px = base + step / 2;
            let py = 3 - step;
            canvas.set(px, py, LEAVES[2]);
            canvas.set(px - 1, py, LEAVES[if step % 2 == 0 { 3 } else { 1 }]);
            canvas.set(px + 1, py + 1, LEAVES[1]);
        }
    }
}

fn paint_tar(canvas: &mut TileCanvas, pick: u32, origin: (i32, i32)) {
    // Glossy black tar with a sheen and a slow bubble or two.
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (origin.0 + px, origin.1 + py);
            let sheen = (gx - gy).rem_euclid(13) == 0 && noise(gx, gy, 6, 943) > 0.5;
            canvas.set(
                px,
                py,
                TAR[if sheen {
                    3
                } else {
                    usize::from(bayer4(gx, gy) > 11)
                }],
            );
        }
    }
    let (bx, by) = (3 + (pick % 9) as i32, 2 + ((pick >> 5) % 6) as i32);
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        canvas.set(bx + dx, by + dy, TAR[2]);
    }
    canvas.set(bx, by, index_of(SLATE_LIGHT));
}

fn paint_bone(canvas: &mut TileCanvas, pick: u32) {
    // A rib of a giant: a long curved bone with knobbed ends.
    let flip = pick.is_multiple_of(2);
    for px in 1_i32..15 {
        let py = 7 + (px - 8).abs() / 3;
        let py = if flip { 15 - py } else { py };
        canvas.set(px, py, BONE_RAMP[2]);
        canvas.set(px, py + 1, BONE_RAMP[1]);
        canvas.set(px, py - 1, BONE_RAMP[3]);
    }
    for (cx, cy) in [(1, 7), (14, 7)] {
        let cy = if flip { 15 - cy } else { cy };
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0)] {
            canvas.set(cx + dx, cy + dy, BONE_RAMP[2]);
        }
    }
}

fn paint_hanging_grass(canvas: &mut TileCanvas, pick: u32) {
    // Grass growing down off an isle's underside (its gravity points up).
    for px in 0..TILE_ART {
        canvas.set(px, TILE_ART - 1, LEAVES[1]);
        canvas.set(px, TILE_ART - 2, LEAVES[2]);
        let blade = 1 + ((pick >> (px % 16)) % 4) as i32;
        if (px + (pick % 3) as i32).rem_euclid(2) == 0 {
            for step in 0..blade {
                canvas.set(
                    px,
                    TILE_ART - 3 - step,
                    LEAVES[if step == blade - 1 { 3 } else { 2 }],
                );
            }
        }
    }
}

fn paint_lodestone(canvas: &mut TileCanvas, pick: u32, origin: (i32, i32)) {
    for py in 2..14 {
        for px in 2..14 {
            let (gx, gy) = (origin.0 + px, origin.1 + py);
            let d = (px - 8) * (px - 8) + (py - 8) * (py - 8);
            if d < 30 + (pick % 12) as i32 {
                canvas.set(
                    px,
                    py,
                    LODE[if noise(gx, gy, 3, 945) > 0.6 {
                        3
                    } else if d < 10 {
                        2
                    } else {
                        1
                    }],
                );
            }
        }
    }
    canvas.set(6, 6, index_of(ICE));
    canvas.set(10, 9, index_of(ICE));
}

fn paint_sun_glyph(canvas: &mut TileCanvas, pick: u32) {
    // A sun disc with rays carved into a few well stones.
    if !pick.is_multiple_of(5) {
        return;
    }
    for py in 5..11 {
        for px in 5..11 {
            let d = (px * 2 - 15) * (px * 2 - 15) + (py * 2 - 15) * (py * 2 - 15);
            if d <= 26 {
                canvas.set(px, py, WELL[3]);
            }
        }
    }
    for (dx, dy) in [
        (0, -5),
        (0, 5),
        (-5, 0),
        (5, 0),
        (-4, -4),
        (4, 4),
        (-4, 4),
        (4, -4),
    ] {
        canvas.set(8 + dx, 8 + dy, WELL[3]);
    }
}

fn paint_totem(canvas: &mut TileCanvas) {
    // A carved face: a brow, two green eyes and a wide mouth.
    let ink = index_of(INK);
    for px in 2..14 {
        canvas.set(px, 3, TOTEM[3]);
        canvas.set(px, 12, ink);
    }
    for (ex, ey) in [(5, 6), (10, 6)] {
        canvas.set(ex, ey, index_of(LIME));
        canvas.set(ex + 1, ey, index_of(LIME));
        canvas.set(ex, ey + 1, ink);
        canvas.set(ex + 1, ey + 1, ink);
    }
    for px in 4..12 {
        canvas.set(px, 10, ink);
    }
    for px in [4, 11] {
        canvas.set(px, 9, ink);
    }
}

pub(super) fn paint(
    canvas: &mut TileCanvas,
    position: TilePosition,
    tile: HollowTile,
    strata: &Strata,
    edges: Edges,
) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body_pixel(tile, gx, gy));
        }
    }
    let pick = hash(position.x, position.y, 951);
    let origin = (canvas.origin_x, canvas.origin_y);
    match tile {
        HollowTile::SkyAmber => paint_amber(canvas, pick),
        HollowTile::Fern if edges.up.is_open() => paint_fern(canvas, pick),
        HollowTile::Tar => paint_tar(canvas, pick, origin),
        HollowTile::GiantBone => paint_bone(canvas, pick),
        HollowTile::IsleGrass if edges.down.is_open() => paint_hanging_grass(canvas, pick),
        HollowTile::Lodestone => paint_lodestone(canvas, pick, origin),
        HollowTile::WellWall => paint_sun_glyph(canvas, pick),
        HollowTile::Totem => paint_totem(canvas),
        _ => {}
    }
    let (lip, shade) = match tile {
        HollowTile::SkyStone | HollowTile::SkyAmber => (SKY_STONE[3], SKY_STONE[0]),
        HollowTile::BreachTimber => (TIMBER[3], TIMBER[0]),
        HollowTile::Loam | HollowTile::Fern | HollowTile::GiantBone => (LOAM_RAMP[3], LOAM_RAMP[0]),
        HollowTile::Trunk => (BARK[3], BARK[0]),
        HollowTile::Canopy | HollowTile::IsleGrass => (LEAVES[3], LEAVES[0]),
        HollowTile::Tar => (TAR[3], TAR[0]),
        HollowTile::IsleRock | HollowTile::IsleStalk | HollowTile::Lodestone => (ISLE[3], ISLE[0]),
        HollowTile::SunRock => (SUN_ROCK[3], SUN_ROCK[0]),
        HollowTile::SunGlass => (SUN_GLASS[3], SUN_GLASS[0]),
        HollowTile::Slag => (SLAG[3], SLAG[0]),
        HollowTile::WellWall => (WELL[3], WELL[0]),
        HollowTile::ShadePillar => (PILLAR[3], PILLAR[0]),
        HollowTile::Totem => (TOTEM[3], TOTEM[0]),
        HollowTile::InnerSun => (SUN_GLASS[3], SUN_GLASS[1]),
    };
    let square = matches!(
        tile,
        HollowTile::BreachTimber
            | HollowTile::WellWall
            | HollowTile::ShadePillar
            | HollowTile::Totem
            | HollowTile::Trunk
    );
    // The sun blazes edge to edge; the fern turf and the hanging grass keep their own faces.
    let skip = matches!(tile, HollowTile::InnerSun)
        || (tile == HollowTile::Fern && edges.up.is_open())
        || (tile == HollowTile::IsleGrass && edges.down.is_open());
    if !skip {
        paint_edges(canvas, strata, edges, lip, shade, square);
    }
}

/// The Hollow World stratum at `position` (`None` outside it or on older worlds).
#[must_use]
pub(super) fn hollow_stratum(terrain: &Terrain, position: TilePosition) -> Option<Stratum> {
    let settings = terrain.settings();
    if !settings.generator.has_descents() {
        return None;
    }
    let stratum = terrain.stratum_at(position);
    (stratum.descent() == fatherload_simulation::descent::Descent::HollowWorld).then_some(stratum)
}

/// Whether open `position` is part of one of the three great open caverns (the Inner Sky, the
/// isle sea, the forge sky), which paint transparent over the hollow backdrop.
#[must_use]
pub fn in_open_hollow(terrain: &Terrain, position: TilePosition) -> bool {
    let Some(stratum) = hollow_stratum(terrain, position) else {
        return false;
    };
    let settings = terrain.settings();
    let layout = fatherload_simulation::descent::fields::cached_layout(settings);
    let (x, y) = (position.x, position.y);
    match stratum {
        Stratum::InnerSky => layout_rules::hollow_open(settings, &layout, x, y),
        Stratum::UpsideDownIsles => {
            let (ceiling, floor) = places::isle_sea_rows(settings, &layout, x);
            y >= ceiling - 4 && y <= floor + 4
        }
        Stratum::TheSunforge => {
            let (ceiling, floor) = places::forge_rows(settings, &layout, x);
            y >= ceiling - 4 && y <= floor + 4
        }
        _ => false,
    }
}

/// One art pixel of the jungle halls' back wall at absolute art pixel `(x, y)`: deep green
/// murk with the silhouettes of far trunks and hanging vines.
fn jungle_back(x: i32, y: i32) -> PaletteIndex {
    let trunk = (x + (noise(x, y, 30, 961) * 10.0) as i32).rem_euclid(37) < 4;
    let vine_sway = (noise(x, y, 12, 963) * 4.0) as i32;
    let vine = (x + vine_sway).rem_euclid(19) == 0 && noise(x, y / 3, 6, 965) > 0.45;
    if hash(x, y, 967).is_multiple_of(211) {
        index_of(LIME)
    } else if trunk {
        index_of(SOIL_DARK)
    } else if vine {
        index_of(MOSS)
    } else if noise(x, y, 10, 969) + f32::from(bayer4(x, y)) / 50.0 > 0.62 {
        index_of(PINE)
    } else {
        index_of(NIGHT)
    }
}

/// Paints open `position` of the Hollow World: transparent in the great caverns (the backdrop
/// shows), the jungle's back wall in its halls, the stratum's tunnel wall elsewhere. Returns
/// whether it painted.
pub(super) fn paint_hollow_air(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    up: Cell,
    left: Cell,
    right: Cell,
) -> bool {
    let Some(stratum) = hollow_stratum(terrain, position) else {
        return false;
    };
    if in_open_hollow(terrain, position) {
        // Left transparent: the hollow backdrop draws the sky behind.
        return true;
    }
    if stratum != Stratum::MegafaunaJungle {
        return false;
    }
    let shade = index_of(INK);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && px == 0)
                || (right == Cell::Solid && px == TILE_ART - 1);
            canvas.set(px, py, if shadowed { shade } else { jungle_back(gx, gy) });
        }
    }
    true
}
