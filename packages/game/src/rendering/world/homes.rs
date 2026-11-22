//! Contractor-built homes in the world (WP-HOME): "for sale" boards on the land office's sites,
//! staked plots, each construction stage (survey stakes and a theodolite, the foundation and
//! stacked materials, scaffolding and the timber frame, walls rising, the roof going on, the
//! finishing touches, with the supply wagon and its mule on site in working hours), the
//! finished houses in their blueprint's style (windows lit at night, chimney smoke, gardens on
//! large plots and estates, the garage door), and hard hats on the site crews.
//!
//! Houses stand on the plot's ground row behind the terrain like the town and the villages;
//! the crews are NPC entities drawn with the people, and get their hats in a pass after them.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use std::cell::RefCell;

use raylib::prelude::*;

use fatherload_simulation::{
    entity::{EntityState, EntityStore},
    home::{
        Blueprint, BuildStage, Construction, Home, HomeJob, Homes, Plot, PlotSetting, PlotSite,
        PlotSize, blueprint::ROOM_WIDTH_TILES, plot,
    },
    people,
    terrain::{TILE_SIZE, Terrain},
    worldgen::WorldSettings,
};

use crate::rendering::people_art;
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, Pen, text_art_width};

/// Art pixels per tile.
const TILE_ART: i32 = 16;
/// Columns either side of the view to look for plots.
const LOOKAHEAD_COLUMNS: i32 = 40;

/// What the homes pass reads.
pub struct HomesScene<'a> {
    pub homes: &'a Homes,
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    /// World time (construction progress).
    pub now: u64,
    /// How dark it is (0 by day): windows light up.
    pub darkness: f32,
    pub seconds: f32,
    /// Whether it is the builders' working hours (the supply wagon stands on site).
    pub working: bool,
}

/// Column buckets of listed sites kept (one per split-screen view, with room to spare).
const SITE_CACHE_BUCKETS: usize = 4;

/// Cached listed sites: the world settings, the column bucket and the sites around it.
type SiteBucket = (WorldSettings, i32, Vec<PlotSite>);

thread_local! {
    /// The land office's listed sites around the last few views, keyed by world settings and a
    /// coarse column bucket (they are a pure function of the settings, but slow to recompute).
    /// Several buckets are kept so split-screen views far apart do not evict each other every
    /// frame.
    static SITES: RefCell<Vec<SiteBucket>> = const { RefCell::new(Vec::new()) };
}

/// Listed sites overlapping `first..=last` (cached per 512-column bucket).
fn listed_sites(terrain: &Terrain, first: i32, last: i32) -> Vec<PlotSite> {
    let settings = *terrain.settings();
    let bucket = (first + last).div_euclid(2).div_euclid(512);
    SITES.with(|cache| {
        let mut cache = cache.borrow_mut();
        let found = cache
            .iter()
            .position(|(cached, at, _)| *cached == settings && *at == bucket);
        let index = found.unwrap_or_else(|| {
            let middle = bucket * 512 + 256;
            let mut sites = plot::listed_sites_between(&settings, middle - 900, middle + 900);
            // The sky towns' isle plots (Wave 6) hang over the same columns.
            for town in fatherload_simulation::worldgen::sky::towns::sky_towns_between(
                &settings,
                middle - 900,
                middle + 900,
            ) {
                sites.extend(plot::sky_sites_of(&settings, town.id));
            }
            if cache.len() >= SITE_CACHE_BUCKETS {
                cache.remove(0);
            }
            cache.push((settings, bucket, sites));
            cache.len() - 1
        });
        cache[index]
            .2
            .iter()
            .filter(|site| site.overlaps_columns(first, last))
            .copied()
            .collect()
    })
}

/// Draws every plot, construction site and home overlapping the view (behind the terrain).
pub fn draw_homes(draw: &mut impl RaylibDraw, scene: &HomesScene<'_>) {
    let first = crate::view_frame::frame().col(scene.visible.x) - LOOKAHEAD_COLUMNS;
    let last = crate::view_frame::frame().col_ceil(scene.visible.x + scene.visible.width)
        + LOOKAHEAD_COLUMNS;
    // "For sale" boards on listed sites nobody owns yet (ground-level only: in the view).
    let top = crate::view_frame::frame().legacy_y(scene.visible.y) / TILE_SIZE - 4.0;
    let bottom = crate::view_frame::frame().legacy_y(scene.visible.y + scene.visible.height)
        / TILE_SIZE
        + 4.0;
    for site in listed_sites(scene.terrain, first, last) {
        let row = site.ground_row as f32;
        if row < top || row > bottom || scene.homes.plot_of_site(site.key).is_some() {
            continue;
        }
        draw_sale_board(draw, &site);
    }
    for plot in scene.homes.plots_between(first, last) {
        let row = plot.site.ground_row as f32;
        if row < top - 20.0 || row > bottom + 4.0 {
            continue;
        }
        draw_plot(draw, plot, scene);
    }
}

/// A plot's corner posts, its grounds, then what stands on it.
fn draw_plot(draw: &mut impl RaylibDraw, plot: &Plot, scene: &HomesScene<'_>) {
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let left = crate::view_frame::frame().col_px(plot.site.left);
    let width = plot.site.width() * TILE_ART;
    let pen = Pen::at(left, ground);
    // Corner posts with the owner's flag.
    for x in [1, width - 3] {
        pen.rect(draw, x, -12, 2, 12, SOIL);
        pen.rect(draw, x, -12, 1, 12, LOAM);
        let wave = ((scene.seconds * 3.0 + x as f32).sin() * 1.5) as i32;
        pen.rect(draw, x + 2, -12, 5 + wave, 3, GOLD);
    }
    if plot.home.is_some() && plot.construction.is_none() {
        draw_grounds(draw, plot, scene.seconds);
    }
    let finished = plot.home.as_ref();
    match (&plot.construction, finished) {
        (Some(work), home) => draw_construction(draw, plot, work, home, scene),
        (None, Some(home)) => draw_house(draw, plot, home, scene, HouseDraw::Whole),
        (None, None) => draw_staked(draw, plot),
    }
}

/// The land office's "for sale" board on a listed site.
fn draw_sale_board(draw: &mut impl RaylibDraw, site: &PlotSite) {
    let ground = crate::view_frame::frame().row_px(site.ground_row);
    let pen = Pen::at(crate::view_frame::frame().col_px(site.left + 1), ground);
    let title = "FOR SALE";
    let price = format!("{} cr", site.price);
    let width = text_art_width(title).max(text_art_width(&price)) + 8;
    let height = 24;
    pen.rect(draw, 4, -height - 12, 2, height + 12, SOIL);
    pen.rect(draw, width - 6, -height - 12, 2, height + 12, SOIL);
    pen.boxed(draw, 0, -height - 14, width, height, CREAM);
    pen.rect(draw, 1, -height - 13, width - 2, 2, RED);
    pen.text(draw, title, 4, -height - 10, RED);
    pen.text(draw, &price, 4, -height, INK);
    // Survey pegs along the plot.
    for x in (0..site.width() * TILE_ART).step_by(24) {
        pen.rect(draw, x, -4, 1, 4, CLAY);
        pen.px(draw, x, -5, ORANGE);
    }
}

/// A bought, bare plot: the deed's pegs and string, and a "SOLD" board.
fn draw_staked(draw: &mut impl RaylibDraw, plot: &Plot) {
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let pen = Pen::at(crate::view_frame::frame().col_px(plot.site.left), ground);
    let width = plot.site.width() * TILE_ART;
    pen.rect(draw, 4, -5, width - 8, 1, with_alpha(ORANGE, 200));
    for x in (4..width - 4).step_by(20) {
        pen.rect(draw, x, -6, 1, 6, CLAY);
    }
    let door = (plot.door_column() - plot.site.left) * TILE_ART;
    pen.rect(draw, door + 6, -22, 2, 22, SOIL);
    pen.boxed(draw, door - 4, -32, 24, 12, CREAM);
    pen.text(draw, "SOLD", door - 1, -29, MOSS);
}

/// Gardens, a path, hedges and trees on large plots and estates; a mown lawn on the rest.
fn draw_grounds(draw: &mut impl RaylibDraw, plot: &Plot, seconds: f32) {
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let pen = Pen::at(crate::view_frame::frame().col_px(plot.site.left), ground);
    let width = plot.site.width() * TILE_ART;
    match plot.site.setting {
        PlotSetting::Moon => {
            // Regolith and a flag.
            pen.rect(draw, 10, -26, 1, 26, STEEL_LIGHT);
            pen.rect(draw, 11, -26, 9, 6, RED);
            pen.rect(draw, 11, -24, 9, 1, BONE);
            return;
        }
        PlotSetting::SkyIsland => {
            for x in (6..width - 6).step_by(28) {
                pen.rect(draw, x, -6, 3, 6, MINT);
                pen.px(draw, x + 1, -7, BONE);
            }
        }
        _ => {}
    }
    // A picket fence along the plot.
    for x in (0..width).step_by(6) {
        pen.rect(draw, x, -9, 2, 9, BONE);
        pen.px(draw, x, -10, BONE);
    }
    pen.rect(draw, 0, -6, width, 1, BONE);
    // Flowers along the fence.
    for x in (3..width).step_by(11) {
        let color = [RED, GOLD, PINK, LAVENDER][((x / 11) % 4) as usize];
        pen.px(draw, x, -3, color);
        pen.px(draw, x, -2, MOSS);
    }
    if plot.site.size >= PlotSize::Large {
        // Trees at the ends and a hedge.
        for x in [10, width - 26] {
            pen.rect(draw, x + 6, -22, 3, 22, SOIL);
            let sway = ((seconds * 0.8 + x as f32).sin() * 1.0) as i32;
            pen.rect(draw, x + sway, -40, 16, 18, PINE);
            pen.rect(draw, x + 2 + sway, -44, 12, 6, MOSS);
            pen.rect(draw, x + 4 + sway, -38, 4, 3, LEAF);
        }
    }
    if plot.site.size == PlotSize::Estate {
        // A fountain off to one side.
        let fx = width - 60;
        pen.rect(draw, fx, -6, 20, 6, STONE_LIGHT);
        pen.rect(draw, fx + 8, -16, 4, 10, STONE);
        let spray = ((seconds * 6.0) as i32) % 3;
        pen.rect(draw, fx + 9, -20 - spray, 2, 4, ICE);
        pen.rect(draw, fx + 2, -5, 16, 2, OCEAN);
    }
}

/// How much of a house to draw.
#[derive(Clone, Copy, PartialEq)]
enum HouseDraw {
    Whole,
    /// Walls up to this share of their height, no roof.
    Walls(f32),
    /// Walls whole, roof this far on.
    Roof(f32),
    /// Whole, but dark (not lived in yet).
    Unlit,
}

/// A blueprint's palette.
#[derive(Clone, Copy)]
struct HouseLook {
    wall: Color,
    wall_dark: Color,
    wall_light: Color,
    roof: Color,
    roof_light: Color,
    trim: Color,
    door: Color,
}

const fn house_look(blueprint: Blueprint) -> HouseLook {
    match blueprint {
        Blueprint::Cabin => HouseLook {
            wall: LOAM,
            wall_dark: SOIL,
            wall_light: CLAY,
            roof: MAROON,
            roof_light: RUST,
            trim: SOIL_DARK,
            door: SOIL_DARK,
        },
        Blueprint::Cottage => HouseLook {
            wall: CREAM,
            wall_dark: SAND,
            wall_light: BONE,
            roof: GOLD,
            roof_light: AMBER,
            trim: LOAM,
            door: TEAL,
        },
        Blueprint::House => HouseLook {
            wall: RUST,
            wall_dark: MAROON,
            wall_light: CLAY,
            roof: SLATE_DARK,
            roof_light: SLATE,
            trim: BONE,
            door: OCEAN,
        },
        Blueprint::WorkshopHome => HouseLook {
            wall: STEEL,
            wall_dark: STEEL_DARK,
            wall_light: STEEL_LIGHT,
            roof: RUST,
            roof_light: COPPER,
            trim: INK,
            door: AMBER,
        },
        Blueprint::Manor => HouseLook {
            wall: STONE_LIGHT,
            wall_dark: STONE,
            wall_light: MIST,
            roof: SLATE_DARK,
            roof_light: SLATE,
            trim: BONE,
            door: MAROON,
        },
        Blueprint::SkyVilla => HouseLook {
            wall: BONE,
            wall_dark: MIST,
            wall_light: CREAM,
            roof: TEAL,
            roof_light: MINT,
            trim: GOLD,
            door: TEAL_DARK,
        },
        Blueprint::LunarHabitat => HouseLook {
            wall: STEEL_LIGHT,
            wall_dark: STEEL,
            wall_light: ICE,
            roof: STEEL,
            roof_light: STEEL_LIGHT,
            trim: STEEL_DARK,
            door: AMBER,
        },
        // Wave 6 plans.
        Blueprint::CloudCottage => HouseLook {
            wall: BONE,
            wall_dark: MIST,
            wall_light: CREAM,
            roof: SKY,
            roof_light: SKY_LIGHT,
            trim: OCEAN,
            door: DEEP_BLUE,
        },
        Blueprint::Aerie => HouseLook {
            wall: CREAM,
            wall_dark: SAND,
            wall_light: BONE,
            roof: COPPER,
            roof_light: AMBER,
            trim: LOAM,
            door: MAROON,
        },
        Blueprint::CraterDome => HouseLook {
            wall: STONE_LIGHT,
            wall_dark: STONE,
            wall_light: MIST,
            roof: STONE,
            roof_light: STONE_LIGHT,
            trim: STEEL_DARK,
            door: ORANGE,
        },
        Blueprint::Longhouse => HouseLook {
            wall: LOAM,
            wall_dark: SOIL,
            wall_light: CLAY,
            roof: MOSS,
            roof_light: LEAF,
            trim: SOIL_DARK,
            door: RED,
        },
    }
}

/// The finished house (or part of it, for construction stages).
fn draw_house(
    draw: &mut impl RaylibDraw,
    plot: &Plot,
    home: &Home,
    scene: &HomesScene<'_>,
    part: HouseDraw,
) {
    draw_house_shape(
        draw,
        plot,
        (home.blueprint, home.width, home.height, home.floors),
        Some(home),
        scene,
        part,
    );
}

/// Draws a house of `blueprint` `width` x `height` tiles with `floors` storeys.
fn draw_house_shape(
    draw: &mut impl RaylibDraw,
    plot: &Plot,
    (blueprint, width, height, floors): (Blueprint, i32, i32, u8),
    home: Option<&Home>,
    scene: &HomesScene<'_>,
    part: HouseDraw,
) {
    let look = house_look(blueprint);
    let left = plot.house_left(width);
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let w = width * TILE_ART;
    let h = height * TILE_ART;
    let pen = Pen::at(
        crate::view_frame::frame().col_px(left),
        ground - h as f32 * ART,
    );
    if blueprint.site() == fatherload_simulation::home::BlueprintSite::Moon {
        draw_dome(draw, pen, w, h, look, home, scene, part);
        return;
    }
    let roof_h = match blueprint {
        Blueprint::Manor => 40,
        Blueprint::Cottage | Blueprint::Cabin | Blueprint::CloudCottage => 26,
        Blueprint::SkyVilla => 22,
        Blueprint::WorkshopHome => 14,
        Blueprint::Aerie => 34,
        Blueprint::Longhouse => 24,
        _ => 28,
    }
    .min(h / 2);
    let wall_top = roof_h;
    let wall_h = h - wall_top;
    let (wall_share, roof_share, dark) = match part {
        HouseDraw::Whole => (1.0, 1.0, false),
        HouseDraw::Walls(share) => (share, 0.0, true),
        HouseDraw::Roof(share) => (1.0, share, true),
        HouseDraw::Unlit => (1.0, 1.0, true),
    };
    let built_h = (wall_h as f32 * wall_share).round() as i32;
    let built_top = wall_top + wall_h - built_h;
    // Walls.
    if built_h > 0 {
        pen.rect(draw, 0, built_top, w, built_h, INK);
        pen.rect(draw, 1, built_top + 1, w - 2, built_h - 1, look.wall);
        match blueprint {
            Blueprint::Cabin => {
                for y in (built_top + 2..wall_top + wall_h).step_by(4) {
                    pen.rect(draw, 1, y, w - 2, 1, look.wall_dark);
                    pen.rect(draw, 1, y + 1, w - 2, 1, look.wall_light);
                }
                // Log ends at the corners.
                for y in (built_top + 2..wall_top + wall_h).step_by(4) {
                    pen.rect(draw, -2, y, 3, 3, look.wall_light);
                    pen.rect(draw, w - 1, y, 3, 3, look.wall_light);
                }
            }
            Blueprint::House => {
                for (row, y) in (built_top + 2..wall_top + wall_h).step_by(4).enumerate() {
                    pen.rect(draw, 1, y, w - 2, 1, look.wall_dark);
                    let shift = if row % 2 == 0 { 0 } else { 4 };
                    for x in (1 + shift..w - 1).step_by(8) {
                        pen.rect(draw, x, y + 1, 1, 3, look.wall_dark);
                    }
                }
            }
            Blueprint::Manor => {
                for (row, y) in (built_top + 2..wall_top + wall_h).step_by(6).enumerate() {
                    pen.rect(draw, 1, y, w - 2, 1, look.wall_dark);
                    let shift = if row % 2 == 0 { 0 } else { 6 };
                    for x in (1 + shift..w - 1).step_by(12) {
                        pen.rect(draw, x, y + 1, 1, 5, look.wall_dark);
                    }
                }
            }
            Blueprint::WorkshopHome => {
                for x in (2..w - 1).step_by(3) {
                    pen.rect(draw, x, built_top + 1, 1, built_h - 1, look.wall_dark);
                }
            }
            Blueprint::Cottage | Blueprint::SkyVilla | Blueprint::CloudCottage => {
                // Timber beams on plaster.
                pen.rect(draw, 1, built_top + 1, 2, built_h - 1, look.trim);
                pen.rect(draw, w - 3, built_top + 1, 2, built_h - 1, look.trim);
            }
            Blueprint::Aerie => {
                // A tower of whitewashed stone with brass bands.
                for y in (built_top + 6..wall_top + wall_h).step_by(12) {
                    pen.rect(draw, 1, y, w - 2, 1, look.trim);
                }
                pen.rect(draw, 1, built_top + 1, 1, built_h - 1, look.wall_dark);
            }
            Blueprint::Longhouse => {
                // Upright planks under one long roof.
                for x in (3..w - 1).step_by(5) {
                    pen.rect(draw, x, built_top + 1, 1, built_h - 1, look.wall_dark);
                }
            }
            Blueprint::LunarHabitat | Blueprint::CraterDome => {}
        }
        // Floor bands between storeys.
        for floor in 1..floors {
            let y = wall_top + wall_h - i32::from(floor) * (wall_h / i32::from(floors.max(1)));
            if y > built_top {
                pen.rect(draw, 1, y, w - 2, 2, look.trim);
            }
        }
        // Windows (only once the walls are there).
        if wall_share >= 1.0 {
            draw_windows(
                draw,
                pen,
                plot,
                (w, wall_top, wall_h, floors),
                home,
                look,
                scene,
                dark,
            );
        }
    }
    // The roof.
    if roof_share > 0.0 {
        let laid = (roof_h as f32 * roof_share).round() as i32;
        match blueprint {
            Blueprint::WorkshopHome => {
                // Sawtooth roof with skylights.
                pen.rect(draw, -2, wall_top - 2, w + 4, 3, INK);
                let teeth = (w / 24).max(1);
                for tooth in 0..teeth {
                    let tx = tooth * 24;
                    for step in 0..roof_h.min(laid) {
                        let span = 24 - step * 24 / roof_h.max(1);
                        pen.rect(
                            draw,
                            tx,
                            wall_top - 2 - step,
                            span,
                            1,
                            if step % 3 == 0 {
                                look.roof_light
                            } else {
                                look.roof
                            },
                        );
                    }
                    pen.rect(
                        draw,
                        tx + 20,
                        wall_top - roof_h.min(laid),
                        2,
                        roof_h.min(laid),
                        with_alpha(SKY_LIGHT, 180),
                    );
                }
            }
            Blueprint::SkyVilla => {
                // A flat roof and two domes.
                pen.rect(draw, -3, wall_top - 3, w + 6, 4, look.trim);
                for cx in [w / 4, w * 3 / 4] {
                    for step in 0..laid.min(14) {
                        let half = ((14 * 14 - step * step) as f32).sqrt() as i32 * 10 / 14;
                        pen.rect(
                            draw,
                            cx - half,
                            wall_top - 3 - step,
                            half * 2,
                            1,
                            if step < 3 { look.roof_light } else { look.roof },
                        );
                    }
                    pen.rect(draw, cx, wall_top - 20, 1, 4, GOLD);
                }
            }
            _ => {
                // A gable roof, laid from the eaves up.
                for step in 0..roof_h {
                    if step >= laid {
                        break;
                    }
                    let inset = step * (w / 2 + 4) / roof_h;
                    let color = if step % 4 == 0 {
                        look.roof_light
                    } else {
                        look.roof
                    };
                    pen.rect(
                        draw,
                        -4 + inset,
                        wall_top - 1 - step,
                        w + 8 - inset * 2,
                        1,
                        color,
                    );
                }
                if laid >= roof_h {
                    pen.rect(draw, -5, wall_top - 1, w + 10, 2, INK);
                    // Dormers on a manor.
                    if blueprint == Blueprint::Manor {
                        for dx in [w / 3, w * 2 / 3] {
                            pen.boxed(draw, dx - 6, wall_top - 22, 12, 12, look.wall);
                            pen.rect(draw, dx - 7, wall_top - 24, 14, 3, look.roof_light);
                            let lit = home.is_some_and(Home::lit) && scene.darkness > 0.1 && !dark;
                            pen.rect(
                                draw,
                                dx - 3,
                                wall_top - 18,
                                6,
                                6,
                                if lit { GOLD } else { SLATE_DARK },
                            );
                        }
                    }
                    // The chimney and its smoke.
                    let chimney_x = w * 3 / 4;
                    pen.boxed(
                        draw,
                        chimney_x,
                        wall_top - roof_h * 3 / 4 - 6,
                        7,
                        roof_h * 3 / 4,
                        look.wall_dark,
                    );
                    if !dark && home.is_some() {
                        for puff in 0..3 {
                            let rise = (scene.seconds * 6.0 + puff as f32 * 7.0) % 21.0;
                            let drift = (rise * 0.4) as i32;
                            pen.rect(
                                draw,
                                chimney_x + 1 + drift,
                                wall_top - roof_h * 3 / 4 - 9 - rise as i32,
                                3 + puff,
                                3,
                                with_alpha(MIST, (180.0 - rise * 8.0).max(0.0) as u8),
                            );
                        }
                    }
                }
            }
        }
        // Manor towers at both ends.
        if blueprint == Blueprint::Manor && laid >= roof_h {
            for tx in [-10, w - 6] {
                pen.boxed(
                    draw,
                    tx,
                    wall_top - 16,
                    16,
                    h - wall_top + 16,
                    look.wall_dark,
                );
                for step in 0..14 {
                    pen.rect(
                        draw,
                        tx + step / 2,
                        wall_top - 17 - step,
                        16 - step,
                        1,
                        look.roof,
                    );
                }
                pen.rect(draw, tx + 7, wall_top - 34, 1, 4, GOLD);
                let lit = home.is_some_and(Home::lit) && scene.darkness > 0.1 && !dark;
                pen.rect(
                    draw,
                    tx + 5,
                    wall_top + 4,
                    6,
                    9,
                    if lit { GOLD } else { SLATE_DARK },
                );
            }
        }
    }
    // The front door (and the porch, columns, the garage door).
    if wall_share >= 1.0 {
        let door_x = (plot.door_column() - left) * TILE_ART;
        let door_h = 22;
        pen.rect(draw, door_x - 1, h - door_h - 1, 12, door_h + 1, INK);
        pen.rect(draw, door_x, h - door_h, 10, door_h, look.door);
        pen.rect(draw, door_x + 7, h - 11, 2, 2, GOLD);
        pen.rect(draw, door_x - 3, h - 2, 16, 2, STONE_LIGHT);
        match blueprint {
            Blueprint::Cottage | Blueprint::CloudCottage => {
                pen.rect(draw, door_x - 8, h - door_h - 6, 26, 3, look.roof);
                pen.rect(draw, door_x - 7, h - door_h - 3, 2, door_h + 3, look.trim);
                pen.rect(draw, door_x + 15, h - door_h - 3, 2, door_h + 3, look.trim);
            }
            Blueprint::Aerie => {
                // A mooring mast with a pennant up top.
                pen.rect(draw, w / 2, -12, 1, 12, LOAM);
                let flap = ((scene.seconds * 4.0) as i32) % 2;
                pen.rect(draw, w / 2 + 1, -12, 6, 3 + flap, RED);
            }
            Blueprint::Manor => {
                for cx in [door_x - 7, door_x + 15] {
                    pen.rect(draw, cx, h - door_h - 8, 3, door_h + 8, BONE);
                }
                pen.rect(draw, door_x - 9, h - door_h - 11, 28, 4, BONE);
            }
            _ => {}
        }
        if home.is_some_and(Home::has_garage) {
            let gx = 3;
            let gw = ROOM_WIDTH_TILES * TILE_ART - 6;
            pen.rect(draw, gx - 1, h - 30, gw + 2, 30, INK);
            for y in (h - 29..h).step_by(4) {
                pen.rect(draw, gx, y, gw, 3, STEEL);
                pen.rect(draw, gx, y, gw, 1, STEEL_LIGHT);
            }
            pen.rect(draw, gx + gw / 2 - 3, h - 4, 6, 1, STEEL_DARK);
        }
    }
}

/// Windows along each storey: lit gold at night when a lamp burns inside, sky by day.
#[allow(clippy::too_many_arguments, reason = "one house's measures")]
fn draw_windows(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    plot: &Plot,
    (w, wall_top, wall_h, floors): (i32, i32, i32, u8),
    home: Option<&Home>,
    look: HouseLook,
    scene: &HomesScene<'_>,
    dark: bool,
) {
    let lit = !dark && home.is_some_and(Home::lit) && scene.darkness > 0.1;
    let round = matches!(
        home.map(|home| home.blueprint),
        Some(Blueprint::SkyVilla | Blueprint::CloudCottage | Blueprint::Aerie)
    );
    let storey = (wall_h / i32::from(floors.max(1))).max(16);
    let door_x = (plot.door_column() - plot.house_left(w / TILE_ART)) * TILE_ART;
    let garage = home.is_some_and(Home::has_garage);
    for floor in 0..floors {
        let base = wall_top + wall_h - i32::from(floor) * storey;
        let y = base - storey + (storey - 11) / 2;
        if y < wall_top + 2 {
            continue;
        }
        for x in (8..w - 12).step_by(22) {
            // Keep clear of the door and the garage.
            if floor == 0
                && ((x - door_x).abs() < 16 || (garage && x < ROOM_WIDTH_TILES * TILE_ART))
            {
                continue;
            }
            let glass = if lit {
                if ((x + i32::from(floor) * 7) / 22) % 3 == 0 {
                    AMBER
                } else {
                    GOLD
                }
            } else if scene.darkness > 0.3 {
                SLATE_DARK
            } else {
                SKY_LIGHT
            };
            if round {
                pen.rect(draw, x, y + 1, 10, 9, INK);
                pen.rect(draw, x + 1, y, 8, 11, INK);
                pen.rect(draw, x + 1, y + 2, 8, 7, glass);
                pen.rect(draw, x + 2, y + 1, 6, 9, glass);
            } else {
                pen.rect(draw, x - 1, y - 1, 12, 13, INK);
                pen.rect(draw, x, y, 10, 11, glass);
                pen.rect(draw, x + 5, y, 1, 11, look.trim);
                pen.rect(draw, x, y + 5, 10, 1, look.trim);
                pen.rect(draw, x - 2, y + 11, 14, 2, look.trim);
                if !lit && scene.darkness <= 0.3 {
                    pen.rect(draw, x + 1, y + 1, 2, 3, BONE);
                }
                // Flower boxes on a cottage.
                if home.is_some_and(|home| home.blueprint == Blueprint::Cottage) {
                    pen.rect(draw, x, y + 13, 10, 2, LOAM);
                    pen.px(draw, x + 2, y + 12, RED);
                    pen.px(draw, x + 6, y + 12, PINK);
                }
            }
        }
    }
}

/// The lunar habitat: a ribbed dome with an airlock and an antenna.
#[allow(clippy::too_many_arguments, reason = "one house's measures")]
fn draw_dome(
    draw: &mut impl RaylibDraw,
    pen: Pen,
    w: i32,
    h: i32,
    look: HouseLook,
    home: Option<&Home>,
    scene: &HomesScene<'_>,
    part: HouseDraw,
) {
    let share = match part {
        HouseDraw::Walls(share) => share * 0.6,
        HouseDraw::Roof(share) => 0.6 + share * 0.4,
        HouseDraw::Whole | HouseDraw::Unlit => 1.0,
    };
    let rows = (h as f32 * share) as i32;
    for step in 0..rows {
        let y = h - 1 - step;
        let t = step as f32 / h as f32;
        let half = ((1.0 - t * t).max(0.0).sqrt() * w as f32 * 0.5) as i32;
        pen.rect(draw, w / 2 - half - 1, y, half * 2 + 2, 1, INK);
        pen.rect(
            draw,
            w / 2 - half,
            y,
            half * 2,
            1,
            if step % 6 == 0 {
                look.wall_dark
            } else {
                look.wall
            },
        );
    }
    if share >= 1.0 {
        pen.rect(draw, w / 2 - 12, h - 18, 24, 18, look.trim);
        pen.rect(draw, w / 2 - 9, h - 15, 18, 15, look.door);
        pen.rect(draw, w / 2 + 20, 2, 1, 14, STEEL_DARK);
        let blink = (scene.seconds * 2.0) as i32 % 2 == 0;
        pen.rect(draw, w / 2 + 19, 0, 3, 3, if blink { RED } else { MAROON });
        let lit = home.is_some_and(Home::lit) && scene.darkness > 0.1;
        for x in [w / 4, w * 3 / 4 - 8] {
            pen.rect(draw, x, h / 2, 8, 6, INK);
            pen.rect(draw, x + 1, h / 2 + 1, 6, 4, if lit { GOLD } else { ICE });
        }
    }
}

/// A construction site at its current stage.
fn draw_construction(
    draw: &mut impl RaylibDraw,
    plot: &Plot,
    work: &Construction,
    home: Option<&Home>,
    scene: &HomesScene<'_>,
) {
    let stage = work.stage(scene.now);
    let progress = work.progress_per_mille(scene.now);
    let next = BuildStage::ALL
        .iter()
        .copied()
        .find(|later| *later > stage)
        .unwrap_or(BuildStage::Done);
    let span = (next.start() - stage.start()).max(1);
    let within = (progress.saturating_sub(stage.start())) as f32 / span as f32;
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let (blueprint, width, height, floors) = match work.job {
        HomeJob::Build(blueprint) => {
            let (w, h) = blueprint.footprint();
            (blueprint, w, h, blueprint.floors())
        }
        HomeJob::Expand(_) => {
            let (w, h) = plot.planned_footprint().unwrap_or((7, 4));
            let home = home.map_or((Blueprint::Cabin, 1), |home| (home.blueprint, home.floors));
            (home.0, w, h, home.1)
        }
    };
    let left = plot.house_left(width);
    let pen = Pen::at(crate::view_frame::frame().col_px(left), ground);
    let w = width * TILE_ART;
    let h = height * TILE_ART;
    let shape = (blueprint, width, height, floors);
    if let HomeJob::Expand(_) = work.job {
        // The house stands; the new part goes up under scaffolding at its side.
        if let Some(home) = home {
            draw_house(draw, plot, home, scene, HouseDraw::Whole);
        }
        draw_scaffold(
            draw,
            Pen::at(
                crate::view_frame::frame().col_px(left + width - ROOM_WIDTH_TILES),
                ground - h as f32 * ART,
            ),
            ROOM_WIDTH_TILES * TILE_ART,
            h,
        );
        draw_materials(draw, pen, w, scene.seconds);
        if scene.working && work.hold_since.is_none() {
            draw_wagon(draw, pen, w + 46, scene.seconds);
        }
        draw_site_sign(draw, pen, plot, work);
        return;
    }
    // The supply wagon stands by while the crew works (a held job sends it home).
    if scene.working
        && work.hold_since.is_none()
        && matches!(
            stage,
            BuildStage::Foundation | BuildStage::Frame | BuildStage::Walls | BuildStage::Roof
        )
    {
        draw_wagon(draw, pen, w + 46, scene.seconds);
    }
    match stage {
        BuildStage::Survey => {
            // Stakes at the corners, a string line, and the surveyor's tripod.
            for x in [0, w / 2 - 1, w - 2] {
                pen.rect(draw, x, -16, 2, 16, CLAY);
                pen.rect(draw, x, -16, 2, 3, ORANGE);
            }
            let line = ((w - 2) as f32 * (0.3 + within * 0.7)) as i32;
            pen.rect(draw, 1, -8, line, 1, ORANGE);
            pen.rect(draw, 1, -14, line, 1, with_alpha(ORANGE, 160));
            // The house's outline chalked on posts: corner poles with flags at full height.
            for x in [0, w - 2] {
                pen.rect(draw, x, -h, 1, h - 16, with_alpha(BONE, 120));
                pen.rect(draw, x + 1, -h, 4, 3, RED);
            }
            let tx = w / 2 + 20;
            pen.rect(draw, tx - 7, -18, 1, 18, SOIL);
            pen.rect(draw, tx + 6, -18, 1, 18, SOIL);
            pen.rect(draw, tx, -18, 1, 18, SOIL);
            pen.boxed(draw, tx - 5, -26, 11, 8, GOLD);
            pen.rect(draw, tx + 5, -24, 3, 3, STEEL_DARK);
            pen.px(draw, tx + 6, -23, SKY_LIGHT);
            // A plan table with the blueprint pinned down.
            pen.rect(draw, -30, -10, 22, 2, LOAM);
            pen.rect(draw, -28, -8, 2, 8, SOIL);
            pen.rect(draw, -12, -8, 2, 8, SOIL);
            pen.rect(draw, -28, -13, 18, 3, OCEAN);
            pen.rect(draw, -26, -12, 14, 1, SKY_LIGHT);
        }
        BuildStage::Foundation => {
            // The footing poured across the footprint, the rest of the site stacked up.
            let poured = (w as f32 * (0.4 + within * 0.6)) as i32;
            pen.rect(draw, 0, -4, poured, 4, STONE);
            pen.rect(draw, 0, -4, poured, 1, STONE_LIGHT);
            for x in (2..poured).step_by(10) {
                pen.rect(draw, x, -3, 1, 3, STONE_DARK);
            }
            draw_mixer(
                draw,
                Pen {
                    x: ((w + 6) as f32).mul_add(ART, pen.x),
                    y: pen.y,
                },
                scene.seconds,
            );
            draw_materials(draw, pen, w, scene.seconds);
        }
        BuildStage::Frame => {
            pen.rect(draw, 0, -3, w, 3, STONE);
            draw_frame(
                draw,
                Pen::at(
                    crate::view_frame::frame().col_px(left),
                    ground - h as f32 * ART,
                ),
                w,
                h,
                within,
            );
            draw_scaffold(
                draw,
                Pen::at(
                    crate::view_frame::frame().col_px(left),
                    ground - h as f32 * ART,
                ),
                w,
                h,
            );
            draw_materials(draw, pen, w, scene.seconds);
        }
        BuildStage::Walls => {
            draw_house_shape(
                draw,
                plot,
                shape,
                None,
                scene,
                HouseDraw::Walls(0.15 + within * 0.85),
            );
            draw_frame(
                draw,
                Pen::at(
                    crate::view_frame::frame().col_px(left),
                    ground - h as f32 * ART,
                ),
                w,
                h,
                1.0,
            );
            draw_scaffold(
                draw,
                Pen::at(
                    crate::view_frame::frame().col_px(left),
                    ground - h as f32 * ART,
                ),
                w,
                h,
            );
            draw_materials(draw, pen, w, scene.seconds);
        }
        BuildStage::Roof => {
            draw_house_shape(
                draw,
                plot,
                shape,
                None,
                scene,
                HouseDraw::Roof(0.1 + within * 0.9),
            );
            draw_scaffold(
                draw,
                Pen::at(
                    crate::view_frame::frame().col_px(left + width - 3),
                    ground - h as f32 * ART,
                ),
                3 * TILE_ART,
                h,
            );
            // A crane arm swinging tiles up.
            let swing = (scene.seconds * 0.6).sin() * 6.0;
            pen.rect(draw, w + 8, -h - 20, 2, h + 20, AMBER);
            pen.rect(draw, w + 8 - 40 + swing as i32, -h - 20, 44, 2, AMBER);
            pen.rect(draw, w - 30 + swing as i32, -h - 18, 1, 10, INK);
            pen.boxed(draw, w - 34 + swing as i32, -h - 8, 9, 5, BONE);
        }
        BuildStage::Finishing => {
            draw_house_shape(draw, plot, shape, None, scene, HouseDraw::Unlit);
            // A ladder, paint pots and a fresh coat going on.
            pen.rect(draw, w - 18, -h + 10, 1, h - 10, LOAM);
            pen.rect(draw, w - 10, -h + 10, 1, h - 10, LOAM);
            for y in (-h + 12..0).step_by(5) {
                pen.rect(draw, w - 18, y, 9, 1, CLAY);
            }
            pen.boxed(draw, w + 2, -6, 6, 6, OCEAN);
            pen.boxed(draw, w + 9, -5, 5, 5, RED);
            let wet = (within * w as f32) as i32;
            pen.rect(draw, 2, -h / 2, wet.min(w - 4), 1, with_alpha(BONE, 120));
        }
        BuildStage::Done => {
            draw_house_shape(draw, plot, shape, None, scene, HouseDraw::Unlit);
        }
    }
    draw_site_sign(draw, pen, plot, work);
}

/// A shelter-less scaffold: poles and plank decks over `w` x `h` art pixels.
fn draw_scaffold(draw: &mut impl RaylibDraw, pen: Pen, w: i32, h: i32) {
    for x in (-4..=w + 2).step_by(20) {
        pen.rect(draw, x, -4, 1, h + 4, STEEL);
    }
    pen.rect(draw, w + 2, -4, 1, h + 4, STEEL);
    for y in (8..h).step_by(18) {
        pen.rect(draw, -6, y, w + 12, 2, LOAM);
        pen.rect(draw, -6, y, w + 12, 1, CLAY);
    }
    // Cross braces.
    for x in (-4..w).step_by(40) {
        for step in 0..18 {
            pen.px(draw, x + step, 10 + step, STEEL_DARK);
        }
    }
}

/// The timber frame, rising with `share`: posts, beams and rafters.
fn draw_frame(draw: &mut impl RaylibDraw, pen: Pen, w: i32, h: i32, share: f32) {
    let roof = 24.min(h / 2);
    let posts_h = ((h - roof) as f32 * share.min(1.0)) as i32;
    for x in (0..w).step_by(16) {
        pen.rect(draw, x, h - posts_h, 2, posts_h, CLAY);
        pen.rect(draw, x, h - posts_h, 1, posts_h, SAND);
    }
    pen.rect(draw, w - 2, h - posts_h, 2, posts_h, CLAY);
    if share > 0.5 {
        pen.rect(draw, 0, roof, w, 2, CLAY);
        pen.rect(draw, 0, i32::midpoint(h, roof), w, 2, CLAY);
    }
    if share > 0.8 {
        for step in 0..roof {
            let inset = step * (w / 2) / roof;
            pen.px(draw, inset, roof - step, CLAY);
            pen.px(draw, w - 1 - inset, roof - step, CLAY);
        }
        pen.rect(draw, w / 2, 0, 1, roof, CLAY);
    }
}

/// Stacked lumber, a brick pallet and a wheelbarrow beside the footprint.
fn draw_materials(draw: &mut impl RaylibDraw, pen: Pen, w: i32, seconds: f32) {
    let x = -34;
    for layer in 0..4 {
        pen.rect(
            draw,
            x + layer % 2,
            -3 - layer * 3,
            26,
            3,
            if layer % 2 == 0 { CLAY } else { LOAM },
        );
        pen.px(draw, x + 1, -2 - layer * 3, SOIL);
    }
    pen.rect(draw, x + 4, -16, 18, 2, SOIL);
    // Brick pallet.
    for row in 0..3 {
        for brick in 0..3 {
            pen.boxed(
                draw,
                x - 18 + brick * 5 + (row % 2) * 2,
                -4 - row * 3,
                5,
                3,
                RUST,
            );
        }
    }
    // A wheelbarrow, its wheel turning.
    pen.rect(draw, w + 22, -8, 12, 5, STEEL);
    let spoke = ((seconds * 3.0) as i32).rem_euclid(2);
    pen.px(draw, w + 25 + spoke, -2, STEEL_LIGHT);
    pen.rect(draw, w + 24, -3, 3, 3, INK);
    pen.rect(draw, w + 34, -10, 6, 1, LOAM);
}

/// The builders' supply wagon at art x `x`: a flatbed of planks and stone blocks, and the
/// mule that pulls it flicking its tail.
fn draw_wagon(draw: &mut impl RaylibDraw, pen: Pen, x: i32, seconds: f32) {
    // Wheels with spokes.
    for wheel in [x + 2, x + 21] {
        pen.rect(draw, wheel, -8, 7, 7, INK);
        pen.rect(draw, wheel + 1, -7, 5, 5, LOAM);
        pen.rect(draw, wheel + 3, -7, 1, 5, SOIL);
        pen.rect(draw, wheel + 1, -5, 5, 1, SOIL);
        pen.px(draw, wheel + 3, -5, STEEL_LIGHT);
    }
    // The bed and its side boards.
    pen.rect(draw, x, -12, 30, 4, CLAY);
    pen.rect(draw, x, -12, 30, 1, SAND);
    pen.rect(draw, x, -16, 1, 4, LOAM);
    pen.rect(draw, x + 29, -16, 1, 4, LOAM);
    // The load: planks and a couple of dressed stones.
    pen.rect(draw, x + 2, -14, 17, 2, SAND);
    pen.rect(draw, x + 3, -16, 15, 2, CLAY);
    pen.px(draw, x + 3, -13, SOIL);
    pen.boxed(draw, x + 20, -17, 4, 5, STONE);
    pen.boxed(draw, x + 24, -16, 4, 4, STONE_LIGHT);
    // Shafts to the mule.
    pen.rect(draw, x + 30, -10, 10, 1, SOIL);
    draw_mule(draw, pen, x + 40, seconds);
}

/// The wagon's mule at art x `m` (Wave 6 art pass): a stocky grey-brown barrel on short, thick
/// legs with dark hooves, the head carried low and forward on a short neck, a pale muzzle and
/// belly, a dark mane strip, the long upright ears that make it a mule (not a deer), a collar
/// with the harness and a tufted tail flicking now and then.
fn draw_mule(draw: &mut impl RaylibDraw, pen: Pen, m: i32, seconds: f32) {
    const COAT: Color = STONE;
    const COAT_DARK: Color = STONE_DARK;
    // Ink outline first, so the coat reads against snow, sand and night alike.
    pen.rect(draw, m - 1, -15, 17, 9, INK);
    pen.rect(draw, m + 13, -19, 6, 6, INK);
    pen.rect(draw, m + 16, -17, 7, 5, INK);
    // Short, thick legs (the far pair darker) with hooves.
    for (leg, color) in [(1, COAT_DARK), (4, COAT), (10, COAT_DARK), (13, COAT)] {
        pen.rect(draw, leg + m - 1, -7, 4, 7, INK);
        pen.rect(draw, leg + m, -7, 2, 6, color);
        pen.rect(draw, leg + m, -1, 2, 1, SOIL_DARK);
    }
    // The barrel: deep and round-backed, pale under the belly.
    pen.rect(draw, m, -14, 15, 7, COAT);
    pen.rect(draw, m + 1, -15, 12, 1, COAT);
    pen.rect(draw, m, -14, 15, 1, STONE_LIGHT);
    pen.rect(draw, m + 2, -8, 11, 1, MIST);
    // The harness: a red collar at the shoulder, a breeching strap behind.
    pen.rect(draw, m + 12, -15, 2, 8, RED);
    pen.rect(draw, m + 2, -12, 10, 1, SOIL);
    // A short neck reaching forward, the head low and long with a pale muzzle.
    pen.rect(draw, m + 14, -18, 4, 5, COAT);
    pen.rect(draw, m + 14, -18, 4, 1, SOIL_DARK);
    pen.rect(draw, m + 17, -16, 5, 3, COAT);
    pen.rect(draw, m + 20, -15, 2, 2, MIST);
    pen.px(draw, m + 21, -15, INK);
    pen.px(draw, m + 18, -16, INK);
    // The long ears, upright and a little splayed.
    pen.rect(draw, m + 14, -24, 3, 6, INK);
    pen.rect(draw, m + 16, -23, 3, 5, INK);
    pen.rect(draw, m + 15, -23, 1, 5, COAT);
    pen.rect(draw, m + 17, -22, 1, 4, COAT_DARK);
    pen.px(draw, m + 15, -21, PINK);
    // The tail: a thin switch with a dark tuft, flicking now and then.
    let flick = i32::from((seconds * 1.3).sin() > 0.7);
    pen.rect(draw, m - 1, -14, 1, 4, COAT_DARK);
    pen.rect(draw, m - 2 - flick, -10 - flick, 2, 3, SOIL_DARK);
}

/// A cement mixer drum turning.
fn draw_mixer(draw: &mut impl RaylibDraw, pen: Pen, seconds: f32) {
    pen.rect(draw, 0, -4, 18, 4, STEEL_DARK);
    pen.rect(draw, 2, -1, 3, 3, INK);
    pen.rect(draw, 13, -1, 3, 3, INK);
    pen.boxed(draw, 3, -16, 13, 12, ORANGE);
    let stripe = ((seconds * 4.0) as i32).rem_euclid(10);
    pen.rect(draw, 4 + stripe.min(10), -15, 2, 10, AMBER);
}

/// The contractor's board at the plot's left edge with the stage's progress bar.
fn draw_site_sign(draw: &mut impl RaylibDraw, _pen: Pen, plot: &Plot, work: &Construction) {
    let ground = crate::view_frame::frame().row_px(plot.site.ground_row);
    let pen = Pen::at(
        crate::view_frame::frame().col_px(plot.site.left + 1),
        ground,
    );
    pen.rect(draw, 6, -20, 2, 20, SOIL);
    pen.rect(draw, 20, -20, 2, 20, SOIL);
    pen.boxed(draw, 0, -34, 28, 16, GOLD);
    // A hard hat on the board.
    pen.rect(draw, 9, -32, 10, 4, AMBER);
    pen.rect(draw, 7, -28, 14, 1, AMBER);
    if work.hold_since.is_some() {
        pen.rect(draw, 3, -24, 22, 3, RED);
    }
}

/// Hard hats (and a swinging hammer) on site crews, after the people are drawn.
pub fn draw_crew_hats(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    entities: &EntityStore,
    visible: Rectangle,
    (tick, alpha): (u64, f32),
    seconds: f32,
) {
    let settings = terrain.settings();
    let previous = fatherload_simulation::SimulationTick::new(tick.saturating_sub(1));
    let margin = 48.0 * TILE_SIZE;
    for id in entities.ids_in_rect(
        visible.x - margin,
        visible.y - 2.0 * TILE_SIZE,
        visible.x + visible.width + margin,
        visible.y + visible.height + 2.0 * TILE_SIZE,
    ) {
        let Some(entity) = entities.get(id) else {
            continue;
        };
        let EntityState::Npc(state) = entity.state else {
            continue;
        };
        if state.site.is_none() || state.inside.is_some() {
            continue;
        }
        let Some(character) = people::routine::character(settings, state.character) else {
            continue;
        };
        let x = state.x_at(previous, alpha);
        let height = people_art::person_height(&character);
        let pen = Pen::at(x, entity.body.y);
        pen.rect(draw, -4, -height - 1, 9, 3, AMBER);
        pen.rect(draw, -5, -height + 1, 11, 1, GOLD);
        if !state.walking_at(previous) && state.talking_to.is_none() {
            // Hammering: the arm swings.
            let swing = ((seconds * 7.0 + f32::from(state.character.index)).sin() * 3.0) as i32;
            let facing = i32::from(state.facing);
            pen.rect(draw, facing * 5, -height / 2 + swing, 1, 6, LOAM);
            pen.rect(draw, facing * 5 - 1, -height / 2 + swing - 1, 3, 2, STEEL);
        }
    }
}

/// Night light from homes with a lamp burning, as `(x, y, radius)` in world pixels.
pub fn home_lights(homes: &Homes, visible: Rectangle) -> Vec<(f32, f32, f32)> {
    let first = crate::view_frame::frame().col(visible.x) - LOOKAHEAD_COLUMNS;
    let last = crate::view_frame::frame().col_ceil(visible.x + visible.width) + LOOKAHEAD_COLUMNS;
    homes
        .plots_between(first, last)
        .filter_map(|plot| {
            let home = plot.home.as_ref()?;
            if !home.lit() {
                return None;
            }
            let left = plot.house_left(home.width);
            // Tiles are absolute; the light is drawn in the frame's local pixels (F7).
            let frame = crate::view_frame::frame();
            let center = (frame.rel_col(left) + home.width as f32 * 0.5) * TILE_SIZE;
            let y = (frame.rel_row(plot.site.ground_row) - home.height as f32 * 0.5) * TILE_SIZE;
            Some((center, y, (home.width as f32 * 0.55 + 2.0) * TILE_SIZE))
        })
        .collect()
}
