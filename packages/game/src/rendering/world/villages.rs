//! Villages in the world: each building as pixel art in its village's style (timber, stone,
//! adobe, scrap metal, rock-cut), the plaza centrepiece and street dressing, the landing pad,
//! signposts to the neighbouring villages and the residents standing about.
//!
//! Buildings stand on the village's ground row on lots [`BuildingKind::width`] tiles wide (16
//! art pixels a tile) and are drawn behind the terrain like the town; residents are drawn in
//! front of it. Underground settlements only show their residents once the hall is explored.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::match_same_arms,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    DiscoveredTiles,
    entity::{EntityId, EntityState, EntityStore},
    people::{self, Character, Culture},
    terrain::{TILE_SIZE, Terrain, TilePosition},
    village::{
        self, BuildingKind, BuildingStyle, Decor, DecorKind, SignTarget, Signpost, Village,
        VillageBuilding, VillageSetting,
    },
};

use crate::render_snapshot::PlayerPresentation;
use crate::rendering::people_art::{self, PersonPen};
#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{ART, Pen, text_art_width};

/// Art pixels per tile.
const TILE_ART: i32 = 16;

/// Columns either side of the view to look for villages in (a whole hall or street).
const LOOKAHEAD_COLUMNS: i32 = 90;

/// A building's palette.
#[derive(Clone, Copy)]
struct Look {
    wall: Color,
    wall_dark: Color,
    wall_light: Color,
    roof: Color,
    roof_light: Color,
    trim: Color,
    /// Paint on doors, shutters and awnings.
    accent: Color,
}

fn look(style: BuildingStyle, culture: Culture, variant: u32) -> Look {
    let pick = |options: &[Color]| options[(variant as usize) % options.len()];
    match style {
        BuildingStyle::Timber => {
            let riverside = culture == Culture::Riverside;
            Look {
                wall: if riverside { CLAY } else { LOAM },
                wall_dark: SOIL,
                wall_light: if riverside { SAND } else { CLAY },
                roof: pick(if riverside {
                    &[MOSS, PINE, TEAL_DARK]
                } else {
                    &[RED, MAROON, SLATE, RUST]
                }),
                roof_light: if riverside { LEAF } else { ROSE },
                trim: BONE,
                accent: pick(&[TEAL, OCEAN, RED, MOSS]),
            }
        }
        BuildingStyle::Stone => Look {
            wall: STONE_LIGHT,
            wall_dark: STONE,
            wall_light: MIST,
            roof: pick(&[SLATE_DARK, SLATE, STONE_DARK]),
            roof_light: SLATE_LIGHT,
            trim: STONE_DARK,
            accent: pick(&[OCEAN, MAROON, MOSS]),
        },
        BuildingStyle::Adobe => Look {
            wall: SAND,
            wall_dark: CLAY,
            wall_light: CREAM,
            roof: CLAY,
            roof_light: SAND,
            trim: SOIL_DARK,
            accent: pick(&[TEAL, ROSE, OCEAN, AMBER]),
        },
        BuildingStyle::Scrap => Look {
            wall: pick(&[STEEL, RUST, STEEL_DARK]),
            wall_dark: STEEL_DARK,
            wall_light: STEEL_LIGHT,
            roof: pick(&[RUST, STEEL_DARK, COPPER]),
            roof_light: COPPER,
            trim: INK,
            accent: pick(&[AMBER, LIME, ORANGE]),
        },
        BuildingStyle::Carved => Look {
            wall: STONE,
            wall_dark: STONE_DARK,
            wall_light: STONE_LIGHT,
            roof: SLATE_DARK,
            roof_light: SLATE,
            trim: NIGHT,
            accent: pick(&[MINT, LAVENDER, TEAL]),
        },
    }
}

/// Everything a building's art needs.
#[derive(Clone, Copy)]
struct Lot {
    pen: Pen,
    /// Lot width in art pixels.
    width: i32,
    /// Lot height in art pixels (the ground line is at the bottom).
    height: i32,
    style: BuildingStyle,
    look: Look,
    variant: u32,
    open: bool,
    seconds: f32,
}

/// Art height of a building kind's lot.
const fn lot_height(kind: BuildingKind) -> i32 {
    match kind {
        BuildingKind::Hall => 76,
        BuildingKind::House => 50,
        BuildingKind::Market | BuildingKind::Workshop | BuildingKind::Assay => 56,
        BuildingKind::Inn => 72,
        BuildingKind::Depot | BuildingKind::Smithy | BuildingKind::Tailor => 58,
        BuildingKind::MapRoom => 70,
        BuildingKind::Shrine => 72,
        BuildingKind::Tower => 92,
        BuildingKind::Farm => 46,
        BuildingKind::MineHead => 78,
    }
}

/// A short sign for a building's front (houses have none).
const fn sign_text(kind: BuildingKind) -> Option<&'static str> {
    Some(match kind {
        BuildingKind::Hall => "HALL",
        BuildingKind::Market => "TRADE",
        BuildingKind::Inn => "INN",
        BuildingKind::Workshop => "WORKS",
        BuildingKind::Depot => "FUEL",
        BuildingKind::Smithy => "SMITH",
        BuildingKind::Tailor => "PAINT",
        BuildingKind::Assay => "ASSAY",
        BuildingKind::MapRoom => "MAPS",
        BuildingKind::Shrine => "SHRINE",
        BuildingKind::House | BuildingKind::Tower | BuildingKind::Farm | BuildingKind::MineHead => {
            return None;
        }
    })
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "view rectangles are far inside i32 tiles"
)]
fn visible_villages(terrain: &Terrain, visible: Rectangle) -> Vec<std::sync::Arc<Village>> {
    let settings = terrain.settings();
    if !settings.generator.has_villages() {
        return Vec::new();
    }
    let first = (visible.x / TILE_SIZE).floor() as i32 - LOOKAHEAD_COLUMNS;
    let last = ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + LOOKAHEAD_COLUMNS;
    village::villages_between(settings, first, last)
        .into_iter()
        .filter(|found| {
            let ground = tile_px(found.ground_row);
            ground + 4.0 * TILE_SIZE >= visible.y
                && ground - 8.0 * TILE_SIZE <= visible.y + visible.height
        })
        .collect()
}

#[allow(
    clippy::cast_precision_loss,
    reason = "tile indices are far inside f32's range"
)]
fn tile_px(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE
}

/// Which pass of the world scene is drawing villages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VillagePass {
    /// Behind the terrain: surface villages (the sky shows through open air).
    Behind,
    /// After the terrain: underground settlements, whose hall the terrain paints as cave
    /// background; only the explored parts are drawn, so fog still hides them.
    Front,
}

/// Draws every village building, decoration, landing pad and signpost overlapping `visible`
/// for `pass`.
pub fn draw_villages(
    draw: &mut impl RaylibDraw,
    scene: (&Terrain, Option<&DiscoveredTiles>),
    visible: Rectangle,
    local: Option<&PlayerPresentation>,
    seconds: f32,
    pass: VillagePass,
) {
    let (terrain, explored) = scene;
    for found in visible_villages(terrain, visible) {
        let underground = found.setting == VillageSetting::Underground;
        if underground != (pass == VillagePass::Front) {
            continue;
        }
        let seen = |column: i32| {
            !underground
                || explored.is_some_and(|tiles| {
                    tiles.contains(TilePosition {
                        x: column,
                        y: found.ground_row - 1,
                    })
                })
        };
        let ground = tile_px(found.ground_row);
        if underground {
            draw_hall_lamps(draw, &found, &seen, seconds);
        }
        for decor in found
            .decor
            .iter()
            .filter(|decor| is_backdrop(decor.kind) && seen(decor.column))
        {
            draw_decor(draw, &found, *decor, ground, seconds);
        }
        let door = local.and_then(|player| found.building_at_pixel(player.x, player.y));
        for (index, building) in found.buildings.iter().enumerate() {
            if !seen(building.first_column + building.width / 2) {
                continue;
            }
            let x = tile_px(building.first_column);
            let width = building.width * TILE_ART;
            if x + width as f32 * ART < visible.x || x > visible.x + visible.width {
                continue;
            }
            let height = lot_height(building.kind);
            let lot = Lot {
                pen: Pen::at(x, ground - height as f32 * ART),
                width,
                height,
                style: found.style,
                look: look(found.style, found.culture, building.variant),
                variant: building.variant,
                open: door == Some(index),
                seconds,
            };
            draw_building(draw, building, lot);
        }
        if let Some((first, last)) = found.landing_pad {
            draw_landing_pad(draw, first, last, ground, seconds);
        }
        for decor in found
            .decor
            .iter()
            .filter(|decor| !is_backdrop(decor.kind) && seen(decor.column))
        {
            draw_decor(draw, &found, *decor, ground, seconds);
        }
        for post in &found.signposts {
            draw_signpost(draw, terrain, &found, post);
        }
    }
}

/// Where surface villages' light comes from at night (the light map, `world/lighting.rs`):
/// each building's lit windows and every street lamp, as `(x, y, radius)` in world pixels.
/// Underground halls keep their own lamps in the cave darkness.
pub fn village_lights(terrain: &Terrain, visible: Rectangle) -> Vec<(f32, f32, f32)> {
    let mut lights = Vec::new();
    for found in visible_villages(terrain, visible) {
        if found.setting == VillageSetting::Underground {
            continue;
        }
        let ground = tile_px(found.ground_row);
        for building in &found.buildings {
            let height = lot_height(building.kind) as f32 * ART;
            let center = tile_px(building.first_column) + building.width as f32 * TILE_SIZE * 0.5;
            let reach = (building.width as f32).mul_add(0.45, 1.8) * TILE_SIZE;
            lights.push((center, ground - height * 0.5, reach));
        }
        for decor in &found.decor {
            if decor.kind == DecorKind::Lamp {
                lights.push((
                    tile_px(decor.column) + 16.0 * ART,
                    ground - 33.0 * ART,
                    TILE_SIZE * 4.5,
                ));
            }
        }
    }
    lights
}

/// The people (WP-PEOPLE NPC entities) on the streets overlapping `visible`, in front of the
/// terrain: interpolated between ticks with `alpha`, walking with a step cycle, facing the way
/// they go (or the player they talk to), with a name tag over whoever `near` names (the person
/// the view's rig would talk to). People indoors are drawn in their rooms instead. Underground
/// settlements only show their people once the hall is explored.
#[allow(
    clippy::too_many_arguments,
    reason = "one draw pass over the scene's parts"
)]
pub fn draw_village_people(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    entities: &EntityStore,
    visible: Rectangle,
    explored: Option<&DiscoveredTiles>,
    alpha: f32,
    near: Option<EntityId>,
    seconds: f32,
) {
    let settings = terrain.settings();
    if !settings.generator.has_villages() {
        return;
    }
    let margin = 2.0 * TILE_SIZE;
    for id in entities.ids_in_rect(
        visible.x - margin,
        visible.y - margin,
        visible.x + visible.width + margin,
        visible.y + visible.height + margin,
    ) {
        let Some(entity) = entities.get(id) else {
            continue;
        };
        let EntityState::Npc(state) = entity.state else {
            continue;
        };
        if state.inside.is_some() {
            continue;
        }
        let Some(settlement) = people::routine::settlement(settings, state.character.village)
        else {
            continue;
        };
        let Some(character) = settlement
            .population
            .characters
            .get(usize::from(state.character.index))
        else {
            continue;
        };
        let (x, y) = entity.body.lerp_position(alpha);
        if settlement.underground
            && !explored.is_some_and(|tiles| {
                tiles.contains(TilePosition {
                    x: (x / TILE_SIZE).floor() as i32,
                    y: settlement.ground_row - 1,
                })
            })
        {
            continue;
        }
        let walking = entity.body.velocity_x.abs() > 1.0;
        let pen = PersonPen {
            x: super::snap(x),
            y: super::snap(y),
            pixel: ART,
        };
        let pose = people_art::Pose {
            facing: i32::from(state.facing),
            stride: walking.then_some(x),
            talking: state.talking_to.is_some() && ((seconds * 0.7) as i32) % 2 == 0,
        };
        people_art::draw_person_posed(draw, pen, character, pose, seconds);
        if near == Some(id) {
            draw_name_tag(draw, pen, character);
        }
    }
}

/// A small name plate over a person's head (the one the view's rig can talk to).
fn draw_name_tag(draw: &mut impl RaylibDraw, pen: PersonPen, character: &Character) {
    let name = character.name.short();
    // Clear of a rig hovering beside them.
    let height = people_art::person_height(character) + 16;
    let width = text_art_width(name) + 6;
    let tag = Pen::at(
        pen.x - (width / 2) as f32 * ART,
        pen.y - height as f32 * ART,
    );
    tag.rect(draw, 0, 0, width, 9, with_alpha(INK, 210));
    tag.rect(draw, 0, 8, width, 1, GOLD);
    tag.text(draw, name, 3, 2, CREAM);
    // A little pointer down at their head.
    tag.rect(draw, width / 2 - 1, 9, 2, 1, GOLD);
}

const fn is_backdrop(kind: DecorKind) -> bool {
    matches!(kind, DecorKind::Tree | DecorKind::Banner)
}

/// Lamps hanging from the carved hall's ceiling and glow worms in the moss (explored parts).
fn draw_hall_lamps(
    draw: &mut impl RaylibDraw,
    found: &Village,
    seen: &impl Fn(i32) -> bool,
    seconds: f32,
) {
    let area = found.area();
    let pen = Pen::at(tile_px(area.min.x), tile_px(area.min.y));
    let width = (area.max.x - area.min.x + 1) * TILE_ART;
    let flicker = ((seconds * 7.0) as i32 % 3) == 0;
    for x in (24..width - 8).step_by(72) {
        if !seen(area.min.x + x / TILE_ART) {
            continue;
        }
        pen.rect(draw, x + 2, 0, 1, 12, STEEL_DARK);
        pen.boxed(draw, x, 12, 5, 5, AMBER);
        pen.px(draw, x + 2, 14, CREAM);
        pen.rect(
            draw,
            x - 8,
            9,
            21,
            12,
            with_alpha(GOLD, if flicker { 34 } else { 46 }),
        );
    }
    for step in 0..width / 23 {
        let x = step * 23 + (step * 7) % 11;
        if !seen(area.min.x + x / TILE_ART) {
            continue;
        }
        pen.rect(draw, x, 0, 2, 3 + step % 4, MOSS);
        if step % 3 == 0 {
            let lit = ((seconds * 1.5 + step as f32) as i32) % 4 != 0;
            pen.px(draw, x + 1, 3 + step % 4, if lit { MINT } else { TEAL });
        }
    }
}

/// `color` a quarter darker (shingle courses).
const fn darken(color: Color) -> Color {
    Color::new(
        (color.r as u16 * 3 / 4) as u8,
        (color.g as u16 * 3 / 4) as u8,
        (color.b as u16 * 3 / 4) as u8,
        color.a,
    )
}

// --- Shared building parts ---------------------------------------------------------------------

/// The style's wall texture over `(x, y, w, h)`.
fn walls(draw: &mut impl RaylibDraw, lot: Lot, x: i32, y: i32, w: i32, h: i32) {
    let pen = lot.pen;
    let look = lot.look;
    pen.rect(draw, x - 1, y - 1, w + 2, h + 1, INK);
    pen.rect(draw, x, y, w, h, look.wall);
    match lot.style {
        BuildingStyle::Timber => {
            let mut px = x + 3;
            while px < x + w {
                pen.rect(draw, px, y, 1, h, look.wall_dark);
                px += 5;
            }
            pen.rect(draw, x, y, w, 1, look.wall_light);
            // Corner posts.
            pen.rect(draw, x, y, 2, h, look.wall_dark);
            pen.rect(draw, x + w - 2, y, 2, h, look.wall_dark);
        }
        BuildingStyle::Stone => {
            for row in 0..h / 5 {
                let ry = y + row * 5;
                pen.rect(draw, x, ry + 4, w, 1, look.wall_dark);
                let offset = if row % 2 == 0 { 2 } else { 6 };
                let mut px = x + offset;
                while px < x + w {
                    pen.rect(draw, px, ry, 1, 4, look.wall_dark);
                    px += 8;
                }
                pen.rect(draw, x + offset + 2, ry, 2, 1, look.wall_light);
            }
        }
        BuildingStyle::Adobe => {
            pen.rect(draw, x, y, w, 2, look.wall_light);
            pen.rect(draw, x, y + h - 4, w, 4, look.wall_dark);
            // Rounded corners.
            pen.px(draw, x, y, INK);
            pen.px(draw, x + w - 1, y, INK);
            for step in 0..w / 9 {
                pen.px(
                    draw,
                    x + 4 + step * 9,
                    y + 6 + (step * 5) % (h.max(10) - 9),
                    look.wall_dark,
                );
            }
        }
        BuildingStyle::Scrap => {
            for px in x..x + w {
                let color = match (px - x) % 4 {
                    0 => look.wall_dark,
                    3 => look.wall_light,
                    _ => look.wall,
                };
                pen.rect(draw, px, y, 1, h, color);
            }
            // Patch panels in other metals.
            let patch_x = x + 4 + (lot.variant % 7) as i32 * 3;
            pen.boxed(draw, patch_x, y + 4, 12, 10, RUST);
            pen.boxed(draw, x + w - 16, y + h / 2, 10, 8, STEEL_LIGHT);
            for rivet in (x + 2..x + w - 2).step_by(6) {
                pen.px(draw, rivet, y + 1, INK);
            }
        }
        BuildingStyle::Carved => {
            pen.rect(draw, x, y, w, 2, look.wall_light);
            for row in 0..h / 7 {
                pen.rect(
                    draw,
                    x + (row * 13) % 9,
                    y + 3 + row * 7,
                    w / 3,
                    1,
                    look.wall_dark,
                );
            }
            pen.rect(draw, x, y, 2, h, look.wall_dark);
            pen.rect(draw, x + w - 2, y, 2, h, look.wall_dark);
        }
    }
}

/// The style's roof over `x..x + w` with its top at `y`, rising `rise` art pixels.
fn roof(draw: &mut impl RaylibDraw, lot: Lot, x: i32, y: i32, w: i32, rise: i32) {
    let pen = lot.pen;
    let look = lot.look;
    match lot.style {
        BuildingStyle::Timber | BuildingStyle::Stone => {
            for row in 0..rise {
                let inset = (rise - 1 - row) * 2;
                pen.rect(draw, x + inset - 1, y + row, w - inset * 2 + 2, 1, INK);
                let color = if row == 1 {
                    look.roof_light
                } else if row % 3 == 2 && lot.style == BuildingStyle::Timber {
                    darken(look.roof)
                } else {
                    look.roof
                };
                pen.rect(draw, x + inset, y + row, w - inset * 2, 1, color);
            }
            pen.rect(draw, x - 2, y + rise, w + 4, 2, INK);
            pen.rect(draw, x - 1, y + rise, w + 2, 1, look.roof_light);
        }
        BuildingStyle::Adobe => {
            // Flat roof, parapet and protruding vigas.
            pen.rect(draw, x - 1, y + rise - 4, w + 2, 5, INK);
            pen.rect(draw, x, y + rise - 3, w, 3, look.wall);
            pen.rect(draw, x, y + rise - 3, w, 1, look.wall_light);
            let mut vx = x + 4;
            while vx < x + w - 2 {
                pen.rect(draw, vx, y + rise + 1, 3, 2, look.trim);
                vx += 10;
            }
        }
        BuildingStyle::Scrap => {
            // A single sloped sheet.
            for row in 0..rise {
                let inset = rise - 1 - row;
                pen.rect(draw, x - 2, y + row, w + 4 - inset * 3, 1, INK);
                pen.rect(draw, x - 1, y + row, w + 2 - inset * 3, 1, look.roof);
            }
            pen.rect(draw, x - 2, y + rise, w + 4, 1, look.roof_light);
        }
        BuildingStyle::Carved => {
            // An arched lintel cut into the rock.
            let half = w / 2;
            for row in 0..rise {
                let t = (rise - row) as f32 / rise as f32;
                let span = ((1.0 - t * t).max(0.0).sqrt() * half as f32) as i32;
                pen.rect(draw, x + half - span - 1, y + row, span * 2 + 2, 1, INK);
                pen.rect(draw, x + half - span, y + row, span * 2, 1, look.roof);
            }
            pen.rect(draw, x - 1, y + rise, w + 2, 2, look.roof_light);
        }
    }
}

/// A window, lit warm on most buildings.
fn window(draw: &mut impl RaylibDraw, lot: Lot, x: i32, y: i32, w: i32, h: i32, lit: bool) {
    let pen = lot.pen;
    let frame = if lot.style == BuildingStyle::Adobe {
        lot.look.accent
    } else {
        lot.look.trim
    };
    pen.rect(draw, x - 1, y - 1, w + 2, h + 2, INK);
    pen.rect(draw, x, y, w, h, frame);
    let glass = if lit { GOLD } else { SKY };
    pen.rect(draw, x + 1, y + 1, w - 2, h - 2, glass);
    if lit {
        pen.rect(draw, x + 1, y + 1, w - 2, 1, CREAM);
    } else {
        pen.rect(draw, x + 1, y + h - 3, w - 2, 2, OCEAN);
    }
    pen.rect(draw, x + w / 2, y + 1, 1, h - 2, frame);
    if lot.style == BuildingStyle::Timber {
        // Shutters.
        pen.rect(draw, x - 3, y, 2, h, lot.look.accent);
        pen.rect(draw, x + w + 1, y, 2, h, lot.look.accent);
    }
}

/// A door at art `x` (its left), `w` wide and `h` tall, standing on the ground line.
fn door(draw: &mut impl RaylibDraw, lot: Lot, x: i32, w: i32, h: i32) {
    let pen = lot.pen;
    let y = lot.height - h;
    pen.rect(draw, x - 1, y - 1, w + 2, h + 1, INK);
    if lot.open {
        pen.rect(draw, x, y, w, h, AMBER);
        pen.rect(draw, x, y, w, 2, ORANGE);
        pen.rect(draw, x + 1, y + h - 3, w - 2, 2, GOLD);
        pen.rect(draw, x - 2, lot.height - 1, w + 4, 1, with_alpha(GOLD, 150));
    } else {
        let body = if lot.style == BuildingStyle::Carved {
            NIGHT
        } else {
            lot.look.accent
        };
        pen.rect(draw, x, y, w, h, body);
        pen.rect(draw, x + 1, y + 1, w - 2, 1, with_alpha(BONE, 70));
        pen.rect(draw, x + w / 2, y + 2, 1, h - 3, with_alpha(INK, 120));
        pen.px(draw, x + w - 3, y + h / 2, GOLD);
    }
    if lot.style == BuildingStyle::Adobe || lot.style == BuildingStyle::Carved {
        // Arched top.
        pen.rect(draw, x, y - 2, w, 2, INK);
        pen.rect(draw, x + 1, y - 3, w - 2, 1, INK);
    }
}

/// A sign board centred on art `cx` with its top at `y`.
fn sign(draw: &mut impl RaylibDraw, lot: Lot, cx: i32, y: i32, text: &str) {
    let pen = lot.pen;
    let width = text_art_width(text) + 6;
    let x = cx - width / 2;
    let (board, ink) = match lot.style {
        BuildingStyle::Timber => (SOIL, CREAM),
        BuildingStyle::Stone => (SLATE_DARK, GOLD),
        BuildingStyle::Adobe => (lot.look.accent, CREAM),
        BuildingStyle::Scrap => (INK, lot.look.accent),
        BuildingStyle::Carved => (NIGHT, MINT),
    };
    pen.boxed(draw, x, y, width, 13, board);
    pen.rect(draw, x + 1, y + 1, width - 2, 1, with_alpha(BONE, 50));
    pen.text(draw, text, x + 3, y + 2, ink);
}

/// A chimney at art `x` from the roof line `y`, puffing smoke.
fn chimney(draw: &mut impl RaylibDraw, lot: Lot, x: i32, y: i32, glow: bool) {
    let pen = lot.pen;
    pen.boxed(draw, x, y - 10, 7, 12, lot.look.wall_dark);
    pen.rect(draw, x - 1, y - 11, 9, 2, INK);
    if glow {
        pen.rect(draw, x + 2, y - 12, 3, 1, ORANGE);
    }
    let phase = lot.seconds * 0.8 + (lot.variant % 10) as f32 * 0.3;
    for puff in 0..3 {
        let t = (phase + puff as f32 / 3.0).fract();
        let rise = (t * 22.0) as i32;
        let drift = (t * 7.0) as i32;
        let size = 2 + (t * 4.0) as i32;
        let alpha = (150.0 * (1.0 - t)) as u8;
        pen.rect(
            draw,
            x + 2 + drift,
            y - 14 - rise,
            size,
            size,
            with_alpha(MIST, alpha),
        );
    }
}

// --- Buildings ---------------------------------------------------------------------------------

fn draw_building(draw: &mut impl RaylibDraw, building: &VillageBuilding, lot: Lot) {
    match building.kind {
        BuildingKind::House => house(draw, lot),
        BuildingKind::Hall => hall(draw, lot),
        BuildingKind::Tower => tower(draw, lot),
        BuildingKind::Farm => farm(draw, lot),
        BuildingKind::MineHead => mine_head(draw, lot),
        kind => shopfront(draw, lot, kind),
    }
}

fn house(draw: &mut impl RaylibDraw, lot: Lot) {
    let w = lot.width;
    let body_top = 24;
    walls(draw, lot, 6, body_top, w - 12, lot.height - body_top);
    roof(draw, lot, 4, 8, w - 8, body_top - 8);
    chimney(draw, lot, w - 26, 14, false);
    let door_x = if lot.variant.is_multiple_of(2) {
        14
    } else {
        w - 28
    };
    door(draw, lot, door_x, 12, 20);
    let window_x = if lot.variant.is_multiple_of(2) {
        w - 34
    } else {
        16
    };
    window(
        draw,
        lot,
        window_x,
        body_top + 6,
        14,
        11,
        !lot.variant.is_multiple_of(3),
    );
    if lot.style == BuildingStyle::Timber || lot.style == BuildingStyle::Stone {
        // Window box with flowers.
        let pen = lot.pen;
        pen.rect(draw, window_x - 1, body_top + 18, 16, 3, lot.look.wall_dark);
        for flower in 0..4 {
            pen.px(
                draw,
                window_x + 1 + flower * 4,
                body_top + 17,
                [ROSE, GOLD, PINK, LAVENDER][flower as usize],
            );
        }
    }
}

fn hall(draw: &mut impl RaylibDraw, lot: Lot) {
    let pen = lot.pen;
    let w = lot.width;
    let body_top = 30;
    let mid = w / 2;
    // The flag pole stands on the roof (drawn first, so a pitched roof hides its foot).
    pen.rect(draw, mid, 0, 1, body_top - 2, STEEL_LIGHT);
    walls(draw, lot, 8, body_top, w - 16, lot.height - body_top);
    roof(draw, lot, 6, 10, w - 12, body_top - 10);
    let wave = (lot.seconds * 3.0) as i32 % 2;
    pen.rect(draw, mid + 1, 1, 10, 5, lot.look.accent);
    pen.rect(draw, mid + 1, 5 - wave, 10, 1 + wave, with_alpha(INK, 90));
    // Columns either side of a wide door, windows along the front.
    for x in [mid - 20, mid + 17] {
        pen.boxed(
            draw,
            x,
            body_top + 6,
            4,
            lot.height - body_top - 6,
            lot.look.wall_light,
        );
    }
    door(draw, lot, mid - 9, 18, 26);
    for x in [16, 36, w - 50, w - 30] {
        window(draw, lot, x, body_top + 12, 12, 14, true);
    }
    sign(draw, lot, mid, body_top + 1, "HALL");
    // Steps.
    pen.rect(draw, mid - 14, lot.height - 2, 28, 2, STONE_LIGHT);
    pen.rect(draw, mid - 14, lot.height - 2, 28, 1, BONE);
}

/// Every building with a counter: walls, roof, door, windows and sign, plus its trade dressing.
fn shopfront(draw: &mut impl RaylibDraw, lot: Lot, kind: BuildingKind) {
    let pen = lot.pen;
    let w = lot.width;
    let body_top = match kind {
        BuildingKind::Inn => 26,
        BuildingKind::MapRoom | BuildingKind::Shrine => 34,
        _ => 22,
    };
    walls(draw, lot, 6, body_top, w - 12, lot.height - body_top);
    if kind == BuildingKind::Shrine {
        shrine_roof(draw, lot, body_top);
    } else {
        roof(draw, lot, 4, body_top - 14, w - 8, 14);
    }
    let door_x = w / 2 - 7;
    door(draw, lot, door_x, 14, 22);
    window(draw, lot, 14, body_top + 8, 13, 12, true);
    window(
        draw,
        lot,
        w - 27,
        body_top + 8,
        13,
        12,
        lot.variant.is_multiple_of(2),
    );
    if let Some(text) = sign_text(kind) {
        sign(
            draw,
            lot,
            w / 2,
            body_top - 2 + i32::from(kind == BuildingKind::Inn) * 2,
            text,
        );
    }
    match kind {
        BuildingKind::Market => {
            // A striped awning and crates of goods.
            let y = body_top + 22;
            pen.rect(draw, 8, y, w - 16, 6, INK);
            for stripe in 0..(w - 18) / 8 {
                let color = if stripe % 2 == 0 {
                    lot.look.accent
                } else {
                    BONE
                };
                pen.rect(draw, 9 + stripe * 8, y + 1, 8, 4, color);
            }
            for (x, color) in [(10, LOAM), (w - 22, CLAY)] {
                pen.boxed(draw, x, lot.height - 10, 11, 10, color);
                pen.rect(draw, x + 2, lot.height - 12, 3, 3, GOLD);
                pen.rect(draw, x + 6, lot.height - 12, 3, 3, LIME);
            }
        }
        BuildingKind::Inn => {
            // Second storey windows and a hanging mug sign.
            window(draw, lot, 20, body_top + 30, 10, 9, true);
            window(draw, lot, w - 30, body_top + 30, 10, 9, true);
            pen.rect(draw, w - 18, body_top + 14, 10, 1, INK);
            pen.rect(draw, w - 10, body_top + 14, 1, 4, INK);
            pen.boxed(draw, w - 14, body_top + 18, 8, 8, AMBER);
            pen.rect(draw, w - 12, body_top + 19, 4, 2, CREAM);
            chimney(draw, lot, 16, body_top - 12, false);
        }
        BuildingKind::Workshop => {
            // A gear plaque and a stack of tyres.
            pen.boxed(draw, w / 2 - 6, body_top + 12, 12, 9, lot.look.trim);
            pen.rect(draw, w / 2 - 3, body_top + 14, 6, 5, STEEL);
            pen.px(draw, w / 2, body_top + 16, INK);
            for (index, y) in [lot.height - 6, lot.height - 12].into_iter().enumerate() {
                pen.boxed(draw, w - 20 + index as i32, y, 12, 6, SLATE_DARK);
                pen.rect(draw, w - 16 + index as i32, y + 2, 4, 2, NIGHT);
            }
        }
        BuildingKind::Depot => {
            // A fuel tank on legs and a pump.
            pen.boxed(draw, w - 30, body_top - 6, 24, 16, RED);
            pen.rect(draw, w - 29, body_top - 5, 22, 2, ROSE);
            pen.rect(
                draw,
                w - 26,
                body_top + 10,
                2,
                lot.height - body_top - 10,
                STEEL_DARK,
            );
            pen.rect(
                draw,
                w - 12,
                body_top + 10,
                2,
                lot.height - body_top - 10,
                STEEL_DARK,
            );
            pen.boxed(draw, 9, lot.height - 18, 9, 18, RED);
            pen.boxed(draw, 10, lot.height - 15, 7, 4, NIGHT);
            let blink = (lot.seconds * 2.0) as i32 % 2 == 0;
            pen.rect(
                draw,
                11,
                lot.height - 14,
                5,
                1,
                if blink { LIME } else { MOSS },
            );
        }
        BuildingKind::Smithy => {
            chimney(draw, lot, w - 22, body_top - 14, true);
            // Anvil and a glowing forge mouth.
            pen.boxed(draw, 10, lot.height - 8, 14, 4, STEEL_DARK);
            pen.rect(draw, 14, lot.height - 4, 6, 4, STEEL_DARK);
            let glow = 120 + ((lot.seconds * 6.0).sin() * 60.0) as i32;
            pen.boxed(draw, w - 26, lot.height - 14, 14, 10, NIGHT);
            pen.rect(
                draw,
                w - 25,
                lot.height - 11,
                12,
                6,
                with_alpha(ORANGE, glow.clamp(40, 200) as u8),
            );
        }
        BuildingKind::Tailor => {
            // Bolts of cloth and paint pots.
            for (index, color) in [ROSE, TEAL, GOLD, VIOLET].into_iter().enumerate() {
                let x = 10 + index as i32 * 5;
                pen.boxed(draw, x, lot.height - 14, 5, 14, color);
            }
            for (index, color) in [RED, SKY, LIME].into_iter().enumerate() {
                let x = w - 26 + index as i32 * 6;
                pen.boxed(draw, x, lot.height - 6, 5, 6, color);
            }
        }
        BuildingKind::Assay => {
            // A balance-scale plaque and an ore cart.
            let cx = w / 2;
            pen.rect(draw, cx - 7, body_top + 13, 14, 1, GOLD);
            pen.rect(draw, cx, body_top + 13, 1, 6, GOLD);
            pen.rect(draw, cx - 8, body_top + 14, 3, 2, GOLD);
            pen.rect(draw, cx + 5, body_top + 14, 3, 2, GOLD);
            pen.boxed(draw, w - 28, lot.height - 10, 18, 7, RUST);
            for (index, color) in [GOLD, LIME, BONE, COPPER].into_iter().enumerate() {
                pen.rect(
                    draw,
                    w - 26 + index as i32 * 4,
                    lot.height - 12,
                    3,
                    3,
                    color,
                );
            }
        }
        BuildingKind::MapRoom => {
            // A domed observatory with a telescope.
            let cx = w / 2;
            for row in 0..10 {
                let span = 12 - (10 - row) * 12 / 11;
                pen.rect(
                    draw,
                    cx - span - 1,
                    body_top - 24 + row,
                    span * 2 + 2,
                    1,
                    INK,
                );
                pen.rect(
                    draw,
                    cx - span,
                    body_top - 24 + row,
                    span * 2,
                    1,
                    STEEL_LIGHT,
                );
            }
            pen.rect(draw, cx + 4, body_top - 26, 12, 3, STEEL_DARK);
            pen.rect(draw, cx + 14, body_top - 27, 3, 5, INK);
            pen.rect(draw, 8, body_top - 14, 2, 10, LOAM);
            pen.rect(draw, 10, body_top - 14, 8, 6, BONE);
            pen.rect(draw, 11, body_top - 12, 5, 1, RED);
        }
        _ => {}
    }
}

fn shrine_roof(draw: &mut impl RaylibDraw, lot: Lot, body_top: i32) {
    let pen = lot.pen;
    let w = lot.width;
    // Two tiers of upturned eaves over a drum wall, a bell and prayer flags.
    pen.rect(draw, 17, body_top - 15, w - 34, 12, INK);
    pen.rect(draw, 18, body_top - 14, w - 36, 11, lot.look.wall);
    pen.rect(draw, w / 2 - 4, body_top - 12, 8, 6, GOLD);
    for (tier, (y, inset)) in [(body_top - 12, 0), (body_top - 26, 14)]
        .into_iter()
        .enumerate()
    {
        pen.rect(draw, 2 + inset, y + 8, w - 4 - inset * 2, 4, INK);
        pen.rect(draw, 3 + inset, y + 9, w - 6 - inset * 2, 2, lot.look.roof);
        pen.rect(draw, inset, y + 6, 4, 3, lot.look.roof_light);
        pen.rect(draw, w - 4 - inset, y + 6, 4, 3, lot.look.roof_light);
        if tier == 0 {
            pen.rect(draw, 10, y + 12, w - 20, 2, lot.look.accent);
        }
    }
    let cx = w / 2;
    pen.rect(draw, cx, body_top - 36, 1, 10, GOLD);
    let swing = ((lot.seconds * 2.0).sin() * 1.5) as i32;
    pen.boxed(draw, cx - 3 + swing, body_top - 30, 7, 6, GOLD);
    for step in 0..8 {
        let color = [RED, GOLD, TEAL, ROSE, LIME][step % 5];
        let x = 6 + step as i32 * ((w - 12) / 8);
        pen.rect(draw, x, body_top - 2 + (step as i32 % 2), 4, 4, color);
    }
    // A soft glow at the door.
    let pulse = 40 + ((lot.seconds * 1.5).sin() * 25.0) as i32;
    pen.rect(
        draw,
        cx - 12,
        lot.height - 26,
        24,
        26,
        with_alpha(GOLD, pulse.clamp(10, 90) as u8),
    );
}

fn tower(draw: &mut impl RaylibDraw, lot: Lot) {
    let pen = lot.pen;
    let w = lot.width;
    // A lookout: legs, a ladder, a railed platform and a roof.
    pen.rect(draw, 10, 30, 3, lot.height - 30, lot.look.wall_dark);
    pen.rect(draw, w - 13, 30, 3, lot.height - 30, lot.look.wall_dark);
    for y in (34..lot.height).step_by(10) {
        pen.rect(draw, 10, y, w - 20, 1, lot.look.wall_dark);
    }
    for y in (32..lot.height).step_by(4) {
        pen.rect(draw, w / 2 - 2, y, 5, 1, LOAM);
    }
    walls(draw, lot, 6, 16, w - 12, 14);
    roof(draw, lot, 4, 4, w - 8, 12);
    window(draw, lot, w / 2 - 6, 19, 12, 8, true);
}

fn farm(draw: &mut impl RaylibDraw, lot: Lot) {
    let pen = lot.pen;
    let w = lot.width;
    // Fields in rows.
    for row in 0..4 {
        let y = lot.height - 3 - row * 4;
        for x in (w / 2..w - 4).step_by(4) {
            let sway = ((lot.seconds * 1.7 + x as f32 * 0.3).sin() * 1.0) as i32;
            pen.rect(
                draw,
                x + sway,
                y - 3,
                1,
                3,
                if row % 2 == 0 { LEAF } else { LIME },
            );
            pen.px(draw, x + sway, y - 4, GOLD);
        }
    }
    // A barn.
    let barn = w / 2 - 6;
    let body_top = 22;
    let barn_look = Lot {
        look: Look {
            wall: RED,
            wall_dark: MAROON,
            wall_light: ROSE,
            ..lot.look
        },
        style: BuildingStyle::Timber,
        ..lot
    };
    walls(draw, barn_look, 4, body_top, barn, lot.height - body_top);
    roof(draw, barn_look, 2, 8, barn + 4, body_top - 8);
    pen.boxed(draw, barn / 2 - 6, lot.height - 18, 14, 18, BONE);
    pen.rect(draw, barn / 2 - 5, lot.height - 17, 12, 1, MAROON);
    pen.rect(draw, barn / 2 + 1, lot.height - 17, 1, 17, MAROON);
    // A fence along the field.
    for x in (w / 2 - 2..w).step_by(6) {
        pen.rect(draw, x, lot.height - 8, 2, 8, LOAM);
    }
    pen.rect(draw, w / 2 - 2, lot.height - 6, w / 2, 1, CLAY);
}

fn mine_head(draw: &mut impl RaylibDraw, lot: Lot) {
    let pen = lot.pen;
    let w = lot.width;
    // A timber headframe with its wheel turning slowly, rails and a cart.
    let cx = w / 2;
    for (x0, x1) in [(10, cx - 2), (w - 10, cx + 2)] {
        for step in 0..lot.height - 14 {
            let x = x0 + (x1 - x0) * step / (lot.height - 14);
            pen.rect(draw, x, lot.height - step, 2, 1, LOAM);
        }
    }
    pen.rect(draw, 14, lot.height - 30, w - 28, 2, SOIL);
    let angle = lot.seconds * 1.2;
    pen.rect(draw, cx - 6, 8, 12, 12, INK);
    pen.rect(draw, cx - 5, 9, 10, 10, STEEL_DARK);
    for spoke in 0..4 {
        let a = angle + spoke as f32 * std::f32::consts::FRAC_PI_4;
        let (dx, dy) = ((a.cos() * 4.0) as i32, (a.sin() * 4.0) as i32);
        pen.px(draw, cx + dx, 14 + dy, STEEL_LIGHT);
        pen.px(draw, cx - dx, 14 - dy, STEEL_LIGHT);
    }
    pen.rect(draw, cx, 20, 1, lot.height - 26, STEEL);
    pen.rect(draw, 2, lot.height - 2, w - 4, 1, STEEL_DARK);
    pen.boxed(draw, 6, lot.height - 10, 14, 8, RUST);
    pen.rect(draw, 8, lot.height - 12, 3, 3, STONE_LIGHT);
    pen.rect(draw, 12, lot.height - 12, 3, 3, GOLD);
}

// --- Decorations, pad and signposts ------------------------------------------------------------

#[allow(clippy::too_many_lines, reason = "one arm per decoration")]
fn draw_decor(
    draw: &mut impl RaylibDraw,
    found: &Village,
    decor: Decor,
    ground: f32,
    seconds: f32,
) {
    let x = tile_px(decor.column);
    let height = 48;
    let pen = Pen::at(x - 8.0 * ART, ground - height as f32 * ART);
    let base = height;
    let culture_color = match found.culture {
        Culture::Frontier => RED,
        Culture::Highland => OCEAN,
        Culture::Mesa => TEAL,
        Culture::Riverside => MOSS,
        Culture::Scrapper => AMBER,
        Culture::Deepfolk => LAVENDER,
    };
    match decor.kind {
        DecorKind::Well => {
            pen.boxed(draw, 4, base - 10, 24, 10, STONE_LIGHT);
            pen.rect(draw, 5, base - 9, 22, 2, MIST);
            pen.rect(draw, 6, base - 26, 2, 16, LOAM);
            pen.rect(draw, 24, base - 26, 2, 16, LOAM);
            for row in 0..6 {
                let inset = 5 - row;
                pen.rect(draw, 2 + inset * 2, base - 32 + row, 28 - inset * 4, 1, RED);
            }
            pen.rect(draw, 15, base - 24, 1, 8, STEEL_DARK);
            pen.boxed(draw, 13, base - 17, 5, 4, LOAM);
        }
        DecorKind::Fountain => {
            pen.boxed(draw, 0, base - 8, 32, 8, SAND);
            pen.rect(draw, 1, base - 7, 30, 3, OCEAN);
            pen.boxed(draw, 13, base - 22, 6, 14, CLAY);
            for jet in 0..6 {
                let t = (seconds * 1.4 + jet as f32 / 6.0).fract();
                let arc = (t * std::f32::consts::PI).sin();
                let dx = ((t - 0.5) * 2.0 * 12.0) as i32;
                let dy = (arc * 10.0) as i32;
                pen.rect(
                    draw,
                    15 + dx,
                    base - 22 - dy,
                    2,
                    2,
                    if jet % 2 == 0 { SKY_LIGHT } else { ICE },
                );
            }
        }
        DecorKind::Statue => {
            pen.boxed(draw, 6, base - 10, 20, 10, STONE);
            pen.rect(draw, 7, base - 9, 18, 1, STONE_LIGHT);
            // A prospector with a pick, in weathered stone.
            pen.boxed(draw, 12, base - 26, 8, 16, STONE_LIGHT);
            pen.boxed(draw, 13, base - 33, 6, 7, STONE_LIGHT);
            pen.rect(draw, 20, base - 30, 1, 14, STONE_DARK);
            pen.rect(draw, 18, base - 31, 6, 2, STONE_DARK);
            pen.rect(draw, 13, base - 22, 6, 1, MOSS);
        }
        DecorKind::Bonfire => {
            pen.rect(draw, 6, base - 3, 20, 3, SOIL_DARK);
            pen.rect(draw, 8, base - 5, 16, 2, LOAM);
            for flame in 0..5 {
                let flicker = ((seconds * 9.0 + flame as f32 * 1.7).sin() * 3.0) as i32;
                let h = 8 + (flame % 3) * 3 + flicker;
                let fx = 9 + flame * 3;
                pen.rect(draw, fx, base - 5 - h, 3, h, ORANGE);
                pen.rect(draw, fx + 1, base - 5 - h / 2, 1, h / 2, GOLD);
            }
            pen.rect(draw, 0, base - 18, 32, 18, with_alpha(AMBER, 30));
        }
        DecorKind::Totem => {
            pen.boxed(draw, 12, base - 40, 8, 40, LOAM);
            for (index, color) in [culture_color, GOLD, RED, TEAL].into_iter().enumerate() {
                let y = base - 38 + index as i32 * 9;
                pen.rect(draw, 13, y, 6, 3, color);
                pen.px(draw, 14, y + 4, INK);
                pen.px(draw, 17, y + 4, INK);
            }
            pen.rect(draw, 6, base - 36, 6, 3, LOAM);
            pen.rect(draw, 20, base - 36, 6, 3, LOAM);
        }
        DecorKind::CrystalCluster => {
            let glow = 40 + ((seconds * 2.0).sin() * 20.0) as i32;
            pen.rect(
                draw,
                4,
                base - 20,
                24,
                20,
                with_alpha(MINT, glow.clamp(10, 70) as u8),
            );
            pen.rect(
                draw,
                8,
                base - 28,
                16,
                8,
                with_alpha(MINT, (glow / 2).clamp(5, 40) as u8),
            );
            for (offset, h, color) in [
                (8, 18, MINT),
                (14, 24, LAVENDER),
                (20, 14, MINT),
                (25, 10, ICE),
            ] {
                pen.rect(draw, offset - 1, base - h - 1, 5, h + 1, INK);
                pen.rect(draw, offset, base - h, 3, h, color);
                pen.px(draw, offset, base - h, BONE);
            }
        }
        DecorKind::Lamp => {
            pen.rect(draw, 15, base - 30, 2, 30, STEEL_DARK);
            pen.boxed(draw, 12, base - 36, 8, 7, AMBER);
            pen.rect(draw, 13, base - 35, 6, 1, CREAM);
            pen.rect(draw, 6, base - 38, 20, 14, with_alpha(GOLD, 35));
        }
        DecorKind::Tree => {
            pen.rect(draw, 14, base - 20, 4, 20, SOIL);
            let sway = ((seconds * 0.8 + decor.column as f32).sin() * 1.2) as i32;
            for (dx, dy, r) in [(0, 0, 11), (-6, 5, 8), (7, 4, 8)] {
                let cx = 16 + dx + sway;
                let cy = base - 30 + dy;
                pen.rect(draw, cx - r - 1, cy - r + 2, r * 2 + 2, r * 2 - 2, INK);
                pen.rect(draw, cx - r, cy - r + 2, r * 2, r * 2 - 3, MOSS);
                pen.rect(draw, cx - r + 2, cy - r + 3, r, 2, LEAF);
            }
        }
        DecorKind::Cart => {
            pen.boxed(draw, 4, base - 14, 22, 8, LOAM);
            pen.rect(draw, 5, base - 13, 20, 1, CLAY);
            for wx in [7, 20] {
                pen.boxed(draw, wx, base - 7, 7, 7, SOIL_DARK);
                pen.px(draw, wx + 3, base - 4, CLAY);
            }
            pen.rect(draw, 26, base - 11, 6, 1, LOAM);
            pen.rect(draw, 7, base - 17, 4, 3, GOLD);
            pen.rect(draw, 12, base - 17, 4, 3, LIME);
        }
        DecorKind::Crates => {
            for (cx, cy) in [(4, 10), (16, 10), (10, 20)] {
                pen.boxed(draw, cx, base - cy, 11, 10, LOAM);
                pen.rect(draw, cx + 1, base - cy + 1, 9, 1, CLAY);
                pen.rect(draw, cx + 5, base - cy + 1, 1, 8, SOIL);
            }
        }
        DecorKind::Fence => {
            for fx in (0..32).step_by(6) {
                pen.rect(draw, fx, base - 10, 2, 10, LOAM);
            }
            pen.rect(draw, 0, base - 8, 32, 1, CLAY);
            pen.rect(draw, 0, base - 4, 32, 1, CLAY);
        }
        DecorKind::Banner => {
            pen.rect(draw, 15, base - 44, 2, 44, STEEL_DARK);
            let wave = (seconds * 3.0 + decor.column as f32) as i32 % 3;
            for row in 0..16 {
                let off = (row + wave) % 3 - 1;
                pen.rect(draw, 17 + off.max(0), base - 42 + row, 11, 1, culture_color);
            }
            pen.rect(draw, 18, base - 36, 9, 2, GOLD);
        }
        DecorKind::Mushrooms => {
            for (mx, h, color) in [(6, 8, LAVENDER), (14, 12, MINT), (22, 6, ROSE)] {
                pen.rect(draw, mx + 2, base - h, 2, h, BONE);
                pen.rect(draw, mx - 1, base - h - 3, 8, 4, INK);
                pen.rect(draw, mx, base - h - 2, 6, 2, color);
            }
        }
        DecorKind::Cactus => {
            pen.boxed(draw, 13, base - 26, 6, 26, MOSS);
            pen.boxed(draw, 6, base - 18, 5, 8, MOSS);
            pen.boxed(draw, 21, base - 22, 5, 9, MOSS);
            pen.rect(draw, 10, base - 12, 4, 3, MOSS);
            pen.rect(draw, 18, base - 15, 4, 3, MOSS);
            pen.px(draw, 15, base - 27, ROSE);
        }
    }
}

fn draw_landing_pad(draw: &mut impl RaylibDraw, first: i32, last: i32, ground: f32, seconds: f32) {
    let width = (last - first + 1) * TILE_ART;
    let pen = Pen::at(tile_px(first), ground - 24.0 * ART);
    let base = 24;
    // Painted deck markings on the paving and beacon posts at both ends.
    pen.rect(draw, 2, base - 2, width - 4, 2, SLATE_DARK);
    let mut x = 4;
    while x < width - 6 {
        pen.rect(draw, x, base - 2, 4, 1, GOLD);
        x += 8;
    }
    let mid = width / 2;
    pen.rect(draw, mid - 8, base - 9, 2, 7, BONE);
    pen.rect(draw, mid + 6, base - 9, 2, 7, BONE);
    pen.rect(draw, mid - 6, base - 6, 12, 1, BONE);
    let blink = (seconds * 2.0) as i32 % 2 == 0;
    for px in [1, width - 4] {
        pen.rect(draw, px + 1, base - 16, 1, 14, STEEL);
        pen.boxed(draw, px, base - 19, 4, 4, if blink { LIME } else { MOSS });
        if blink {
            pen.rect(draw, px - 3, base - 22, 10, 10, with_alpha(LIME, 40));
        }
    }
}

/// A signpost: a post with an arrow board naming the next place along the surface and how far
/// it is, and the village's own name on a board above.
fn draw_signpost(draw: &mut impl RaylibDraw, terrain: &Terrain, found: &Village, post: &Signpost) {
    let settings = terrain.settings();
    let ground = tile_px(terrain.surface_row(post.column));
    let target = match post.target {
        SignTarget::Town => "TOWN".to_owned(),
        SignTarget::Village(id) => village::village_identity(settings, id)
            .map_or_else(|| "?".to_owned(), |(name, _)| name.to_uppercase()),
    };
    let distance = format!("{} m", post.distance);
    let pen = Pen::at(tile_px(post.column) + TILE_SIZE * 0.5, ground - 48.0 * ART);
    let base = 48;
    pen.rect(draw, -1, base - 34, 3, 34, INK);
    pen.rect(draw, 0, base - 34, 1, 34, LOAM);
    let name = found.name.to_uppercase();
    let name_width = text_art_width(&name) + 6;
    pen.boxed(draw, -name_width / 2, base - 48, name_width, 13, SOIL);
    pen.text(draw, &name, -name_width / 2 + 3, base - 46, CREAM);
    let label = format!("{target} {distance}");
    let width = text_art_width(&label) + 10;
    let left = if post.direction < 0 { -width + 4 } else { -4 };
    pen.boxed(draw, left, base - 32, width, 13, CLAY);
    pen.text(
        draw,
        &label,
        left + if post.direction < 0 { 7 } else { 3 },
        base - 30,
        INK,
    );
    // The arrow tip.
    let tip_x = if post.direction < 0 {
        left - 3
    } else {
        left + width
    };
    for row in 0..5 {
        let span = 3 - (row - 2_i32).abs();
        let x = if post.direction < 0 {
            tip_x + 3 - span
        } else {
            tip_x
        };
        pen.rect(draw, x, base - 30 + row * 2, span, 2, CLAY);
    }
}

/// Whether column `x` carries the trail between the town and the villages: open surface,
/// outside every village's flat span and the town's lots.
fn on_trail(terrain: &Terrain, x: i32) -> bool {
    let settings = terrain.settings();
    let (town_first, town_last) = fatherload_simulation::surface::town_span();
    !(town_first - 2..=town_last + 2).contains(&x)
        && village::placement::surface_site_at(settings.seed, x, 1).is_none()
}

/// The trails joining the town and the surface villages: a trodden dirt path along the top
/// of the untouched ground ([`VillagePass::Front`], over the grass) and a milestone every
/// [`village::layout::MILESTONE_SPACING`] columns naming the nearest place and how far it is
/// ([`VillagePass::Behind`], standing in the open air). Drilled-out ground loses its path.
pub fn draw_trails(
    draw: &mut impl RaylibDraw,
    terrain: &Terrain,
    visible: Rectangle,
    pass: VillagePass,
) {
    let settings = terrain.settings();
    if !settings.generator.has_villages() {
        return;
    }
    let first = (visible.x / TILE_SIZE).floor() as i32 - 2;
    let last = ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + 2;
    // The surface sits within a few rows of the classic row; skip deep views outright.
    let lowest_surface = tile_px(terrain.surface_row(first).max(terrain.surface_row(last)) + 4);
    if visible.y > lowest_surface {
        return;
    }
    match pass {
        VillagePass::Front => {
            for x in first..=last {
                let row = terrain.surface_row(x);
                let intact = terrain.is_solid_at(TilePosition { x, y: row })
                    && !terrain.is_solid_at(TilePosition { x, y: row - 1 });
                if !intact || !on_trail(terrain, x) {
                    continue;
                }
                draw_trail_tile(draw, x, row, terrain.surface_row(x + 1) - row);
            }
        }
        VillagePass::Behind => {
            let spacing = village::layout::MILESTONE_SPACING;
            // Start a spacing early: a milestone's board reaches a few tiles past its post.
            let mut x = first
                - (first - village::placement::TOWN_CENTER_COLUMN).rem_euclid(spacing)
                - spacing;
            while x <= last + spacing {
                if let Some(post) = village::milestone_at(settings, x) {
                    let row = terrain.surface_row(x);
                    if terrain.is_solid_at(TilePosition { x, y: row }) {
                        draw_milestone(draw, terrain, &post, row);
                    }
                }
                x += spacing;
            }
        }
    }
}

/// One tile of trodden path on top of surface tile `(x, row)`; `step` is how many rows the
/// next column's surface lies lower (the path bends down or up to meet it).
fn draw_trail_tile(draw: &mut impl RaylibDraw, x: i32, row: i32, step: i32) {
    let pen = Pen::at(tile_px(x), tile_px(row));
    let hash = (x as u32).wrapping_mul(0x2C9B_7F13) >> 7;
    // Packed earth over the grass, ruts darker, a few pale pebbles.
    pen.rect(draw, 0, 0, TILE_ART, 3, LOAM);
    pen.rect(draw, 0, 3, TILE_ART, 1, SOIL);
    pen.rect(draw, 2 + (hash % 3) as i32, 1, 4, 1, SOIL);
    pen.rect(draw, 9 + ((hash >> 2) % 3) as i32, 1, 4, 1, SOIL);
    if hash.is_multiple_of(4) {
        pen.px(draw, ((hash >> 4) % 14) as i32 + 1, 0, SAND);
    }
    if hash % 5 == 1 {
        pen.px(draw, ((hash >> 6) % 14) as i32 + 1, 2, CLAY);
    }
    if step != 0 {
        // A worn edge where the ground steps to the next column's height.
        let edge = if step > 0 { TILE_ART - 2 } else { 0 };
        pen.rect(draw, edge, 0, 2, 4, SOIL);
    }
}

/// A trail milestone: a squat painted stone and a stake carrying a little arrow board that
/// names the nearest place along the surface and the distance to its edge.
fn draw_milestone(draw: &mut impl RaylibDraw, terrain: &Terrain, post: &Signpost, row: i32) {
    let target = match post.target {
        SignTarget::Town => "TOWN".to_owned(),
        SignTarget::Village(id) => village::village_identity(terrain.settings(), id)
            .map_or_else(|| "?".to_owned(), |(name, _)| name.to_uppercase()),
    };
    let base = 30;
    let pen = Pen::at(
        tile_px(post.column) + TILE_SIZE * 0.5,
        tile_px(row) - base as f32 * ART,
    );
    pen.rect(draw, -1, base - 26, 3, 20, INK);
    pen.rect(draw, 0, base - 26, 1, 20, LOAM);
    pen.boxed(draw, -5, base - 9, 11, 9, STONE_LIGHT);
    pen.rect(draw, -4, base - 8, 9, 1, MIST);
    pen.rect(draw, -4, base - 5, 9, 2, RED);
    pen.rect(draw, -4, base - 2, 9, 1, STONE);
    let label = format!("{target} {} m", post.distance);
    let width = text_art_width(&label) + 10;
    let left = if post.direction < 0 { -width + 3 } else { -3 };
    pen.boxed(draw, left, base - 27, width, 12, CLAY);
    pen.text(
        draw,
        &label,
        left + if post.direction < 0 { 7 } else { 3 },
        base - 26,
        INK,
    );
    let tip_x = if post.direction < 0 {
        left - 3
    } else {
        left + width
    };
    for step in 0..5 {
        let span = 3 - (step - 2_i32).abs();
        let x = if post.direction < 0 {
            tip_x + 3 - span
        } else {
            tip_x
        };
        pen.rect(draw, x, base - 26 + step * 2, span, 2, CLAY);
    }
}
