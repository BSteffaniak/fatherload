//! The living surface of a biome world (WP-SURFACE, presentation only): vegetation and props
//! standing on the ground, decorative lake water and per-biome weather.
//!
//! What grows where is simulation data ([`biome::flora_at`]), so every client and every seat
//! sees the same trees; this module only draws it. A plant is drawn while the ground it stands
//! on is still the generated surface (dig it out and it is gone). Flora sprites are rasterized
//! once at startup into small textures ([`SurfaceArt`]).
//!
//! Weather is a field of particles anchored to the world (flying moves through it), chosen by
//! the biome under the view and faded out underground and high in the sky: snow over tundra
//! and high mountain slopes, dust over deserts and badlands, leaves in forests, rain over
//! wetlands and drifting seeds over the plains.

use raylib::prelude::*;

use fatherload_simulation::{
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
    worldgen::biome::{self, Biome, Flora, LAKE_ROW},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::{pixel_art::sprite::blank_image, terrain::tile_art::hash};

use super::{ART, snap};

/// Variants rasterized per flora kind.
const VARIANTS: usize = 4;
/// Columns beyond the view's sides still checked for plants (wide crowns reach in).
const FLORA_MARGIN_COLUMNS: i32 = 3;

/// Every flora kind, in sprite order.
const FLORA: [Flora; 11] = [
    Flora::GrassTuft,
    Flora::Flowers,
    Flora::Bush,
    Flora::Oak,
    Flora::Pine,
    Flora::SnowPine,
    Flora::Cactus,
    Flora::DeadBush,
    Flora::Boulder,
    Flora::Reeds,
    Flora::LilyPad,
];

/// Startup-rasterized flora sprites: `VARIANTS` per kind, in [`FLORA`] order.
pub struct SurfaceArt {
    sprites: Vec<Option<Texture2D>>,
}

impl SurfaceArt {
    pub fn load(texture: &mut impl FnMut(Image) -> Option<Texture2D>) -> Self {
        let mut sprites = Vec::with_capacity(FLORA.len() * VARIANTS);
        for flora in FLORA {
            for variant in 0..VARIANTS {
                sprites.push(texture(flora_image(flora, variant)));
            }
        }
        Self { sprites }
    }

    fn sprite(&self, flora: Flora, variant: usize) -> Option<&Texture2D> {
        let kind = FLORA.iter().position(|f| *f == flora)?;
        self.sprites
            .get(kind * VARIANTS + variant % VARIANTS)?
            .as_ref()
    }
}

// ---------------------------------------------------------------------------------------------
// Sprites.

/// A small paint buffer in art pixels.
struct Canvas {
    image: Image,
    width: i32,
    height: i32,
}

impl Canvas {
    fn new(width: i32, height: i32) -> Self {
        Self {
            image: blank_image(width, height),
            width,
            height,
        }
    }

    fn px(&mut self, x: i32, y: i32, color: Color) {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.image.draw_pixel(x, y, color);
        }
    }

    /// A filled disc with a lit top-left and shaded bottom-right.
    fn blob(&mut self, cx: i32, cy: i32, r: i32, colors: [Color; 3], salt: u32) {
        for dy in -r..=r {
            for dx in -r..=r {
                let d = dx * dx + dy * dy;
                if d > r * r + r / 2 {
                    continue;
                }
                let lit = dx + dy < -r / 2;
                let dark = dx + dy > r / 2 || d > (r - 1) * (r - 1);
                let speck = hash(cx + dx, cy + dy, salt).is_multiple_of(7);
                let color = if lit && !speck {
                    colors[0]
                } else if dark || speck {
                    colors[2]
                } else {
                    colors[1]
                };
                self.px(cx + dx, cy + dy, color);
            }
        }
    }

    /// An `INK` outline around every painted pixel (reads crisply against any backdrop).
    fn outline(mut self) -> Image {
        let width = self.width;
        let height = self.height;
        let painted: Vec<bool> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| self.image.get_color(x, y).a > 0)
            .collect();
        let at = |x: i32, y: i32| {
            (0..width).contains(&x) && (0..height).contains(&y) && painted[(y * width + x) as usize]
        };
        for y in 0..height {
            for x in 0..width {
                if !at(x, y) && (at(x - 1, y) || at(x + 1, y) || at(x, y - 1) || at(x, y + 1)) {
                    self.image.draw_pixel(x, y, INK);
                }
            }
        }
        self.image
    }
}

fn flora_image(flora: Flora, variant: usize) -> Image {
    let v = variant as i32;
    match flora {
        Flora::GrassTuft => {
            // One tuft per colour family: lush, dry, frosty, alpine.
            let colors = match variant {
                0 => (LIME, LEAF),
                1 => (CREAM, AMBER),
                2 => (ICE, MIST),
                _ => (LEAF, MOSS),
            };
            let mut canvas = Canvas::new(9, 6);
            for (x, tall) in [(1, 2), (2, 4), (3, 3), (4, 5), (5, 3), (6, 4), (7, 2)] {
                for row in 0..tall {
                    canvas.px(
                        x,
                        5 - row,
                        if row == tall - 1 { colors.0 } else { colors.1 },
                    );
                }
            }
            canvas.image
        }
        Flora::Flowers => {
            let petal = [ROSE, GOLD, LAVENDER, BONE][variant % 4];
            let mut canvas = Canvas::new(9, 7);
            for (x, tall) in [(1, 3), (4, 5), (7, 4)] {
                for row in 0..tall {
                    canvas.px(x, 6 - row, MOSS);
                }
                let top = 6 - tall;
                canvas.px(x, top, petal);
                canvas.px(x - 1, top + 1, petal);
                canvas.px(x + 1, top + 1, petal);
                canvas.px(x, top + 1, GOLD);
            }
            canvas.px(2, 6, LEAF);
            canvas.px(5, 6, LEAF);
            canvas.image
        }
        Flora::Bush => {
            let mut canvas = Canvas::new(16, 10);
            canvas.blob(5, 6, 4, [LEAF, MOSS, PINE], 3 + variant as u32);
            canvas.blob(10, 5, 5, [LIME, LEAF, MOSS], 5 + variant as u32);
            if variant % 2 == 0 {
                canvas.px(9, 3, ROSE);
                canvas.px(12, 6, ROSE);
            }
            canvas.outline()
        }
        Flora::Oak => {
            let (w, h) = (26, 34 + v * 3);
            let mut canvas = Canvas::new(w, h);
            let trunk_top = h - 14;
            for y in trunk_top..h {
                for x in 11..15 {
                    canvas.px(x, y, if x == 11 { LOAM } else { SOIL });
                }
            }
            canvas.px(10, h - 1, SOIL);
            canvas.px(15, h - 1, SOIL);
            let crown = [LIME, LEAF, MOSS];
            canvas.blob(13, 11 + v, 9 + v % 2, [LEAF, MOSS, PINE], 11);
            canvas.blob(8, 15 + v, 6, crown, 12);
            canvas.blob(18, 14 + v, 6, crown, 13);
            canvas.blob(13, 8 + v, 6, crown, 14);
            canvas.outline()
        }
        Flora::Pine | Flora::SnowPine => {
            let snowy = flora == Flora::SnowPine;
            let (w, h) = (20, 34 + v * 4);
            let mut canvas = Canvas::new(w, h);
            for y in h - 5..h {
                canvas.px(9, y, SOIL);
                canvas.px(10, y, LOAM);
            }
            let tiers = 4 + v / 2;
            let tier_height = (h - 6) / tiers;
            for tier in 0..tiers {
                let top = 1 + tier * tier_height;
                let bottom = (top + tier_height + 3).min(h - 4);
                for y in top..bottom {
                    let half = ((y - top) * (3 + tier) / (tier_height + 3)).max(0) + 1;
                    for dx in -half..=half {
                        let x = 10 + dx;
                        let color = if snowy && y - top < 2 + (dx.abs() % 2) {
                            if dx < 0 { BONE } else { ICE }
                        } else if dx <= -half + 1 {
                            MOSS
                        } else if dx > half / 2 {
                            PINE
                        } else if hash(x, y, 21).is_multiple_of(9) {
                            MOSS
                        } else {
                            PINE
                        };
                        canvas.px(x, y, color);
                    }
                }
            }
            canvas.px(10, 0, if snowy { BONE } else { MOSS });
            canvas.outline()
        }
        Flora::Cactus => {
            let (w, h) = (14, 18 + v * 3);
            let mut canvas = Canvas::new(w, h);
            let column = |canvas: &mut Canvas, x: i32, top: i32, bottom: i32| {
                for y in top..bottom {
                    canvas.px(x, y, LIME);
                    canvas.px(x + 1, y, LEAF);
                    canvas.px(x + 2, y, MOSS);
                    if (y + x).rem_euclid(3) == 0 {
                        canvas.px(x + 1, y, LIME);
                    }
                }
            };
            column(&mut canvas, 5, 1, h);
            // Arms.
            let arm_y = h / 2 - 1 + v % 2;
            for x in 2..5 {
                canvas.px(x, arm_y + 2, LEAF);
                canvas.px(x, arm_y + 3, MOSS);
            }
            column(&mut canvas, 1, arm_y - 3, arm_y + 3);
            if variant % 2 == 1 {
                for x in 8..11 {
                    canvas.px(x, arm_y + 4, LEAF);
                    canvas.px(x, arm_y + 5, MOSS);
                }
                column(&mut canvas, 9, arm_y - 1, arm_y + 5);
            }
            if variant == 2 {
                canvas.px(6, 0, ROSE);
            }
            canvas.outline()
        }
        Flora::DeadBush => {
            let mut canvas = Canvas::new(12, 8);
            for (x0, lean) in [(5, -1), (6, 0), (7, 1)] {
                let mut x = x0;
                for y in (1..8).rev() {
                    canvas.px(x, y, if y % 2 == 0 { LOAM } else { SOIL });
                    if y % 2 == 0 {
                        x += lean;
                    }
                    if hash(x, y, 31 + variant as u32).is_multiple_of(3) {
                        canvas.px(x + lean * 2, y - 1, LOAM);
                    }
                }
            }
            canvas.image
        }
        Flora::Boulder => {
            let mut canvas = Canvas::new(14, 9);
            let colors = if variant % 2 == 0 {
                [STONE_LIGHT, STONE, STONE_DARK]
            } else {
                [SLATE_LIGHT, SLATE, SLATE_DARK]
            };
            canvas.blob(6, 6, 4, colors, 41);
            canvas.blob(9, 7, 3, colors, 42);
            if variant >= 2 {
                canvas.px(5, 3, MOSS);
                canvas.px(6, 3, LEAF);
            }
            canvas.outline()
        }
        Flora::Reeds => {
            let mut canvas = Canvas::new(10, 14);
            for (x, tall, cattail) in [
                (1, 8, false),
                (3, 12, true),
                (5, 9, false),
                (7, 11, true),
                (8, 6, false),
            ] {
                for row in 0..tall {
                    canvas.px(x, 13 - row, if row % 3 == 0 { LEAF } else { MOSS });
                }
                if cattail && (x + v) % 2 == 1 {
                    canvas.px(x, 13 - tall, SOIL);
                    canvas.px(x, 14 - tall, SOIL);
                    canvas.px(x, 12 - tall, LOAM);
                }
            }
            canvas.image
        }
        Flora::LilyPad => {
            let mut canvas = Canvas::new(8, 3);
            for x in 1..7 {
                canvas.px(x, 1, if x == 3 { MOSS } else { LEAF });
                canvas.px(x, 2, MOSS);
            }
            if variant % 2 == 0 {
                canvas.px(5, 0, PINK);
                canvas.px(4, 0, ROSE);
            }
            canvas.image
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing.

/// The visible columns (plus a margin).
fn columns(visible: Rectangle, margin: i32) -> std::ops::RangeInclusive<i32> {
    let first = (visible.x / TILE_SIZE).floor() as i32 - margin;
    let last = ((visible.x + visible.width) / TILE_SIZE).ceil() as i32 + margin;
    first..=last
}

/// Whether column `x`'s generated ground is still there: its surface tile solid and the tile
/// above it open.
fn ground_intact(terrain: &Terrain, x: i32, row: i32) -> bool {
    terrain.is_solid_at(TilePosition { x, y: row })
        && terrain
            .tile(TilePosition { x, y: row - 1 })
            .is_none_or(|tile| tile.kind == TileKind::Air)
}

/// Draws the plants and props standing on the visible surface.
pub fn draw_flora(
    draw: &mut impl RaylibDraw,
    art: &SurfaceArt,
    terrain: &Terrain,
    visible: Rectangle,
) {
    let settings = terrain.settings();
    if !settings.generator.has_biomes() {
        return;
    }
    for x in columns(visible, FLORA_MARGIN_COLUMNS) {
        let Some(spot) = biome::flora_at(settings, x) else {
            continue;
        };
        let row = terrain.surface_row(x);
        let ground_y = row as f32 * TILE_SIZE;
        if spot.flora == Flora::LilyPad {
            let water_y = LAKE_ROW as f32 * TILE_SIZE;
            if water_y < visible.y - TILE_SIZE || water_y > visible.y + visible.height + TILE_SIZE {
                continue;
            }
        } else if ground_y < visible.y - 2.0 * TILE_SIZE
            || ground_y - 3.0 * TILE_SIZE > visible.y + visible.height
            || !ground_intact(terrain, x, row)
        {
            continue;
        }
        let variant = match spot.flora {
            Flora::GrassTuft => match biome::biome_at(settings, x) {
                Biome::Plains | Biome::Forest | Biome::Wetlands => 0,
                Biome::Desert | Biome::Badlands => 1,
                Biome::Tundra => 2,
                Biome::Mountains => 3,
            },
            _ => (spot.variant % VARIANTS as u32) as usize,
        };
        let Some(texture) = art.sprite(spot.flora, variant) else {
            continue;
        };
        let (width, height) = (texture.width() as f32 * ART, texture.height() as f32 * ART);
        let jitter = ((spot.variant >> 8) % 9) as f32 - 4.0;
        let center = (x as f32).mul_add(TILE_SIZE, TILE_SIZE * 0.5) + jitter * ART;
        let bottom = if spot.flora == Flora::LilyPad {
            LAKE_ROW as f32 * TILE_SIZE + ART
        } else {
            // Sink the base a pixel into the grass.
            ground_y + ART
        };
        let flip = (spot.variant >> 4) % 2 == 1;
        let source_width = if flip {
            -(texture.width() as f32)
        } else {
            texture.width() as f32
        };
        draw.draw_texture_pro(
            texture,
            Rectangle::new(0.0, 0.0, source_width, texture.height() as f32),
            Rectangle::new(
                snap(center - width * 0.5),
                snap(bottom - height),
                width,
                height,
            ),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );
    }
}

/// Draws decorative lake water over the basins in view, with a lapping, glinting surface line.
pub fn draw_water(draw: &mut impl RaylibDraw, terrain: &Terrain, visible: Rectangle, ticks: u64) {
    let settings = terrain.settings();
    if !settings.generator.has_biomes() {
        return;
    }
    let top = LAKE_ROW as f32 * TILE_SIZE;
    if top > visible.y + visible.height || top + 8.0 * TILE_SIZE < visible.y {
        return;
    }
    let body = with_alpha(OCEAN, 150);
    let deep = with_alpha(DEEP_BLUE, 110);
    for x in columns(visible, 1) {
        let Some(lake) = biome::lake_row(settings, x) else {
            continue;
        };
        let bed = terrain.surface_row(x);
        let left = x as f32 * TILE_SIZE;
        let water_top = lake as f32 * TILE_SIZE + ART * 2.0;
        let depth = bed as f32 * TILE_SIZE - water_top;
        draw.draw_rectangle_rec(Rectangle::new(left, water_top, TILE_SIZE, depth), body);
        if depth > TILE_SIZE * 2.0 {
            draw.draw_rectangle_rec(
                Rectangle::new(
                    left,
                    water_top + TILE_SIZE * 1.5,
                    TILE_SIZE,
                    depth - TILE_SIZE * 1.5,
                ),
                deep,
            );
        }
        // The surface: a light line that laps up and down a pixel, with moving glints.
        let art_columns = (TILE_SIZE / ART) as i32;
        for column in 0..art_columns {
            let gx = x * art_columns + column;
            let phase = (gx as u64 + ticks / 9) % 12;
            let lift = if phase < 6 { 0.0 } else { ART };
            let color = if hash(gx, (ticks / 20) as i32, 77).is_multiple_of(11) {
                BONE
            } else if phase % 4 == 0 {
                ICE
            } else {
                SKY_LIGHT
            };
            draw.draw_rectangle_rec(
                Rectangle::new(
                    (column as f32).mul_add(ART, left),
                    water_top - lift,
                    ART,
                    ART,
                ),
                color,
            );
        }
    }
}

/// One biome's weather particles.
#[derive(Clone, Copy)]
struct Weather {
    count: u32,
    /// World pixels per second, right and down.
    wind: f32,
    fall: f32,
    /// Sideways sway amplitude (world pixels).
    sway: f32,
    /// Art pixels wide and tall per particle.
    size: (f32, f32),
    colors: [Color; 2],
}

fn weather_for(biome: Biome, high: bool) -> Option<Weather> {
    match biome {
        Biome::Tundra => Some(Weather {
            count: 70,
            wind: 18.0,
            fall: 46.0,
            sway: 10.0,
            size: (2.0, 2.0),
            colors: [BONE, ICE],
        }),
        Biome::Mountains if high => Some(Weather {
            count: 90,
            wind: 70.0,
            fall: 60.0,
            sway: 6.0,
            size: (2.0, 2.0),
            colors: [BONE, ICE],
        }),
        Biome::Desert => Some(Weather {
            count: 50,
            wind: 150.0,
            fall: 8.0,
            sway: 4.0,
            size: (3.0, 1.0),
            colors: [with_alpha(CREAM, 170), with_alpha(SAND, 150)],
        }),
        Biome::Badlands => Some(Weather {
            count: 34,
            wind: 120.0,
            fall: 6.0,
            sway: 4.0,
            size: (3.0, 1.0),
            colors: [with_alpha(AMBER, 160), with_alpha(CLAY, 140)],
        }),
        Biome::Forest => Some(Weather {
            count: 16,
            wind: 14.0,
            fall: 26.0,
            sway: 18.0,
            size: (2.0, 1.0),
            colors: [AMBER, LEAF],
        }),
        Biome::Wetlands => Some(Weather {
            count: 80,
            wind: -40.0,
            fall: 330.0,
            sway: 0.0,
            size: (1.0, 3.0),
            colors: [with_alpha(SKY_LIGHT, 170), with_alpha(ICE, 140)],
        }),
        Biome::Plains => Some(Weather {
            count: 6,
            wind: 22.0,
            fall: -4.0,
            sway: 12.0,
            size: (1.0, 1.0),
            colors: [with_alpha(CREAM, 200), with_alpha(LIME, 200)],
        }),
        Biome::Mountains => None,
    }
}

/// Draws the weather of the biome under the view, if the view is out in the open near the
/// surface.
pub fn draw_weather(draw: &mut impl RaylibDraw, terrain: &Terrain, visible: Rectangle, ticks: u64) {
    const PERIOD: (f32, f32) = (720.0, 540.0);
    let settings = terrain.settings();
    if !settings.generator.has_biomes() {
        return;
    }
    let center_x = ((visible.x + visible.width * 0.5) / TILE_SIZE).floor() as i32;
    let center_row = ((visible.y + visible.height * 0.5) / TILE_SIZE).floor() as i32;
    let surface = terrain.surface_row(center_x);
    let biome = biome::biome_at(settings, center_x);
    // Fully out within a few rows of the ground and up to 40 rows above it; fades over 12
    // rows beyond (none underground, none high in the sky).
    let below = (center_row - surface - 4).max(0) as f32;
    let above = (surface - center_row - 40).max(0) as f32;
    let strength = (1.0 - below / 6.0).min(1.0 - above / 12.0).clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let high = center_row < fatherload_simulation::worldgen::strata::SKY_ROWS - 26;
    let Some(weather) = weather_for(biome, high) else {
        return;
    };
    let seconds = ticks as f32 / 60.0;
    let count = (weather.count as f32 * strength).round() as u32;
    let first_x = (visible.x / PERIOD.0).floor() as i32;
    let last_x = ((visible.x + visible.width) / PERIOD.0).floor() as i32;
    let first_y = (visible.y / PERIOD.1).floor() as i32;
    let last_y = ((visible.y + visible.height) / PERIOD.1).floor() as i32;
    for tile_y in first_y..=last_y {
        for tile_x in first_x..=last_x {
            for index in 0..count {
                let h = hash(index as i32, 3, 97);
                let speed = 0.7 + ((h >> 20) % 60) as f32 / 100.0;
                let base_x = (h % 1000) as f32 / 1000.0 * PERIOD.0;
                let base_y = ((h >> 10) % 1000) as f32 / 1000.0 * PERIOD.1;
                let sway = (seconds * 1.3 + index as f32).sin() * weather.sway;
                let x = (base_x + weather.wind * speed * seconds + sway).rem_euclid(PERIOD.0)
                    + tile_x as f32 * PERIOD.0;
                let y = (base_y + weather.fall * speed * seconds).rem_euclid(PERIOD.1)
                    + tile_y as f32 * PERIOD.1;
                if x < visible.x
                    || x > visible.x + visible.width
                    || y < visible.y
                    || y > visible.y + visible.height
                {
                    continue;
                }
                // Not inside the ground.
                let tile = TilePosition {
                    x: (x / TILE_SIZE).floor() as i32,
                    y: (y / TILE_SIZE).floor() as i32,
                };
                if terrain.is_solid_at(tile) {
                    continue;
                }
                let color = weather.colors[(h >> 30) as usize % 2];
                draw.draw_rectangle_rec(
                    Rectangle::new(snap(x), snap(y), weather.size.0 * ART, weather.size.1 * ART),
                    color,
                );
            }
        }
    }
}
