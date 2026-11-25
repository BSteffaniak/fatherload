#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "fixture geometry: small whole tile counts"
)]

//! Staged space views for the screenshot scenes and the browser's timing page (TODO-SEAM: the
//! integrator builds the same [`SpaceRenderView`] from the real space generator and flight
//! state; these fixtures stand in until then).
//!
//! The systems sit a billion sectors from home, so every scene also proves the projection is
//! exact far out. Ground fixtures are real [`Terrain`]s laid out as polar strips: a barren
//! moon of radius 400 (its whole ring, down to the core, with an optional shaft) and a corner
//! of a jungle world of radius 5,000 (the home world's own generator, its surface columns
//! only).

use std::sync::OnceLock;

use fatherload_simulation::{
    moon::LunarTile,
    position::WorldPos,
    star_chart::StarClass,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::{
    hash64, unit,
    view::{
        AsteroidView, BeltView, BlackHoleView, BodyTheme, BodyView, CometView, CraftView,
        FlightMotion, GroundThing, GroundThingKind, GroundView, NebulaView, PolarGeometry,
        PolarStrip, SpaceCamera, SpaceRenderView, StarView, StructureKind, StructureView,
        WormholeView,
    },
};
use crate::{rendering::pixel_art::palette, screen_fx::EffectsIntensity};

/// The fixtures' sky seed (the default world seed's).
const SKY_SEED: u64 = 0x00F0_7E51_0AD5;
/// The fixture system's sector: far from home.
const FAR: i64 = 1_000_000_000;

/// A position `(x, y)` tiles from the fixture system's origin.
fn at(x: f64, y: f64) -> WorldPos {
    let tile = f64::from(TILE_SIZE);
    let (px, py) = (x * tile, y * tile);
    let sector = f64::from(fatherload_simulation::position::SECTOR_PX);
    let (sx, sy) = ((px / sector).floor(), (py / sector).floor());
    WorldPos::new(
        FAR + sx as i64,
        -3 + sy as i64,
        (px - sx * sector) as f32,
        (py - sy * sector) as f32,
    )
}

/// A ground strip held in a terrain (see [`PolarStrip`]).
pub struct FixtureStrip {
    geometry: PolarGeometry,
    terrain: Terrain,
    /// The surface column the terrain's column 0 holds (a window of a big world).
    base: i64,
    /// Whether the terrain holds the whole ring.
    whole: bool,
}

impl FixtureStrip {
    /// The first solid row of surface column `column` (where things stand).
    fn ground_row(&self, column: f64) -> i32 {
        let Some(tile) = self.tile_at(0, column.floor() as u32, 0) else {
            return self.geometry.surface_row;
        };
        (self.terrain.min_y()..self.terrain.height())
            .find(|&row| {
                self.terrain
                    .tile(TilePosition { x: tile.x, y: row })
                    .is_some_and(|found| !found.kind.is_open())
            })
            .unwrap_or(self.geometry.surface_row)
    }
}

impl PolarStrip for FixtureStrip {
    fn geometry(&self) -> &PolarGeometry {
        &self.geometry
    }

    fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    fn tile_at(&self, band: u32, column: u32, row: i32) -> Option<TilePosition> {
        let row = row.min(self.terrain.height() - 1);
        if self.whole {
            return Some(TilePosition {
                x: column as i32,
                y: row,
            });
        }
        if band > 0 {
            return None;
        }
        let x = i64::from(column) - self.base;
        (0..i64::from(self.terrain.width()))
            .contains(&x)
            .then_some(TilePosition { x: x as i32, y: row })
    }

    fn revision(&self) -> u64 {
        0
    }
}

/// The barren moon's ground material at `depth` rows below its surface row.
fn moon_tile(column: i32, row: i32, depth: i32) -> TileKind {
    let h = hash64(&[i64::from(column), i64::from(row), 7]);
    let roll = unit(h, 0);
    let lunar = |tile| TileKind::Lunar(tile);
    // Caves: a few rounded pockets in the rock.
    let cave = hash64(&[i64::from(column / 9), i64::from(row / 6), 3]);
    if depth > 14 && depth < 300 && unit(cave, 0) < 0.06 {
        let (cx, cy) = (column % 9, row % 6);
        if (1..8).contains(&cx) && (1..5).contains(&cy) {
            return TileKind::Air;
        }
    }
    match depth {
        d if d < 3 => {
            if (column as f32 * 0.043).sin() + (column as f32 * 0.11).sin() * 0.5 > 0.9 {
                lunar(LunarTile::MareDust)
            } else {
                lunar(LunarTile::Regolith)
            }
        }
        d if d < 60 => {
            if roll < 0.03 {
                lunar(LunarTile::Ice)
            } else if roll < 0.045 {
                lunar(LunarTile::Helium3)
            } else {
                lunar(LunarTile::Rock)
            }
        }
        d if d < 170 => {
            if roll < 0.02 {
                lunar(LunarTile::AlloyVein)
            } else if roll < 0.2 {
                lunar(LunarTile::Basalt)
            } else {
                lunar(LunarTile::DeepRock)
            }
        }
        d if d < 300 => {
            if roll < 0.03 {
                lunar(LunarTile::CrystalStalk)
            } else {
                lunar(LunarTile::Basalt)
            }
        }
        d if d < 350 => {
            if roll < 0.04 {
                TileKind::MagmaVent
            } else {
                TileKind::HardRock
            }
        }
        d if d < 372 => {
            if roll < 0.85 {
                TileKind::Lava
            } else {
                TileKind::HardRock
            }
        }
        _ => TileKind::HardRock,
    }
}

/// The surface column the moon scenes stand on.
const MOON_COLUMN: f64 = 1_200.5;

fn build_moon(shaft: bool) -> FixtureStrip {
    let probe = Terrain::new_seeded(64, 64, 41);
    let surface_row = probe.surface_row(MOON_COLUMN as i32);
    let mut geometry = PolarGeometry::standard(400.0, surface_row);
    let width = geometry.surface_columns as i32;
    let height = geometry.last_row() + 2;
    let mut terrain = Terrain::new_seeded(width, height, 41);
    for x in 0..width {
        // Gentle hills on top of the generator's line (only ever up: open air below that line
        // reads as cave to the tile art).
        let hill = (1.4 * (x as f32 * 0.047).sin() + 0.9 * (x as f32 * 0.131 + 1.0).sin() + 0.9)
            .round()
            .max(0.0) as i32;
        let ground = terrain.surface_row(x) - hill;
        for y in 0..height {
            let columns = geometry.columns_at(y) as i32;
            let kind = if y < ground || x >= columns {
                TileKind::Air
            } else {
                moon_tile(x, y, y - surface_row)
            };
            terrain.set_kind(TilePosition { x, y }, kind);
        }
    }
    if shaft {
        for y in 0..height {
            let band = geometry.band(y);
            let centre = (MOON_COLUMN as i64 >> band) as i32;
            let half = if band == 0 { 1 } else { 0 };
            for x in centre - half..=centre + half {
                terrain.set_kind(TilePosition { x, y }, TileKind::Air);
            }
        }
    }
    geometry.core_radius_tiles = geometry.core_radius_tiles.max(6.0);
    FixtureStrip {
        geometry,
        terrain,
        base: 0,
        whole: true,
    }
}

fn moon(shaft: bool) -> &'static FixtureStrip {
    static PLAIN: OnceLock<FixtureStrip> = OnceLock::new();
    static SHAFT: OnceLock<FixtureStrip> = OnceLock::new();
    if shaft {
        SHAFT.get_or_init(|| build_moon(true))
    } else {
        PLAIN.get_or_init(|| build_moon(false))
    }
}

/// The surface column the big world's scenes stand on, and the window its terrain holds.
const BIG_COLUMN: f64 = 12_000.5;
const BIG_WINDOW: i32 = 2_048;

fn big_planet() -> &'static FixtureStrip {
    static BIG: OnceLock<FixtureStrip> = OnceLock::new();
    BIG.get_or_init(|| {
        let base = BIG_COLUMN as i64 - i64::from(BIG_WINDOW / 2);
        let terrain = Terrain::new_seeded(BIG_WINDOW, 160, 77);
        let surface_row = terrain.surface_row(BIG_WINDOW / 2);
        FixtureStrip {
            geometry: PolarGeometry::standard(5_000.0, surface_row),
            terrain,
            base,
            whole: false,
        }
    })
}

/// A staged space view (owns everything a [`SpaceRenderView`] borrows).
pub struct SpaceFixture {
    pub camera: SpaceCamera,
    pub stars: Vec<StarView>,
    pub bodies: Vec<BodyView>,
    pub belts: Vec<BeltView>,
    pub asteroids: Vec<AsteroidView>,
    pub nebulae: Vec<NebulaView>,
    pub comets: Vec<CometView>,
    pub structures: Vec<StructureView>,
    pub black_holes: Vec<BlackHoleView>,
    pub wormholes: Vec<WormholeView>,
    pub motion: FlightMotion,
    pub craft: Option<CraftView>,
    pub ground: Option<FixtureGround>,
    pub seconds: f32,
    pub effects: EffectsIntensity,
    /// Draw the polar ground as curved quads (the fallback's own scene).
    pub force_quads: bool,
}

/// Where a fixture stands on a body.
pub struct FixtureGround {
    pub body: BodyView,
    pub strip: &'static FixtureStrip,
    pub column: f64,
    pub row: f32,
    pub roll: f32,
    pub things: Vec<GroundThing>,
}

impl SpaceFixture {
    fn empty(camera: SpaceCamera) -> Self {
        Self {
            camera,
            stars: Vec::new(),
            bodies: Vec::new(),
            belts: Vec::new(),
            asteroids: Vec::new(),
            nebulae: Vec::new(),
            comets: Vec::new(),
            structures: Vec::new(),
            black_holes: Vec::new(),
            wormholes: Vec::new(),
            motion: FlightMotion::Coasting,
            craft: Some(CraftView { thrusting: false }),
            ground: None,
            seconds: 12.0,
            effects: EffectsIntensity::Full,
            force_quads: false,
        }
    }

    /// The view the renderer draws.
    #[must_use]
    pub fn view(&self) -> SpaceRenderView<'_> {
        SpaceRenderView {
            camera: self.camera,
            sky_seed: SKY_SEED,
            seconds: self.seconds,
            stars: &self.stars,
            bodies: &self.bodies,
            belts: &self.belts,
            asteroids: &self.asteroids,
            nebulae: &self.nebulae,
            comets: &self.comets,
            structures: &self.structures,
            black_holes: &self.black_holes,
            wormholes: &self.wormholes,
            motion: self.motion,
            craft: self.craft,
            ground: self.ground.as_ref().map(|ground| GroundView {
                body: ground.body,
                strip: ground.strip,
                column: ground.column,
                row: ground.row,
                roll: ground.roll,
                night_light: 0.12,
                things: &ground.things,
            }),
            effects: self.effects,
        }
    }

    /// Advances the presentation clock (the browser's timing page animates).
    pub fn advance(&mut self, seconds: f32) {
        self.seconds += seconds;
    }

    /// Stands the camera on `body` at `column`, `row` (rolled `roll` against the local up),
    /// at `px_per_tile`: the space camera follows from the ground's place.
    fn stand_on(
        &mut self,
        body: BodyView,
        strip: &'static FixtureStrip,
        (column, row, roll): (f64, f32, f32),
        px_per_tile: f32,
        things: Vec<GroundThing>,
    ) {
        let geometry = strip.geometry();
        let angle = (column / f64::from(geometry.surface_columns) * std::f64::consts::TAU)
            + f64::from(body.spin);
        let radius = f64::from(geometry.radius_of_row(row)) * f64::from(TILE_SIZE);
        let centre = body.centre.offset(
            (angle.sin() * radius) as f32,
            (-angle.cos() * radius) as f32,
        );
        self.camera = SpaceCamera {
            centre,
            roll: angle as f32 + roll,
            px_per_tile,
        };
        self.ground = Some(FixtureGround {
            body,
            strip,
            column,
            row,
            roll,
            things,
        });
    }
}

/// The fixture star system: a yellow sun with six worlds, a belt and a comet.
fn system(fixture: &mut SpaceFixture) {
    fixture.stars.push(StarView {
        centre: at(0.0, 0.0),
        radius_tiles: 1_600.0,
        class: StarClass::Yellow,
        seed: 3,
    });
    let worlds = [
        (21_000.0, -0.6, 1_500.0, BodyTheme::Desert, 0.0),
        (36_000.0, 2.2, 5_000.0, BodyTheme::Jungle, 0.08),
        (52_000.0, 0.9, 2_600.0, BodyTheme::Ocean, 0.1),
        (74_000.0, -2.4, 9_000.0, BodyTheme::GasGiant, 0.06),
        (96_000.0, 1.6, 2_500.0, BodyTheme::Ice, 0.05),
        (110_000.0, -1.2, 1_800.0, BodyTheme::Volcanic, 0.07),
    ];
    for (index, (distance, angle, radius, theme, atmosphere)) in worlds.into_iter().enumerate() {
        let (sin, cos) = f64::sin_cos(angle);
        fixture.bodies.push(BodyView {
            id: 10 + index as u64,
            centre: at(cos * distance, sin * distance),
            radius_tiles: radius,
            theme,
            atmosphere,
            spin: angle as f32,
            seed: 100 + index as u32,
        });
    }
    // The jungle world's small moon.
    let (sin, cos) = f64::sin_cos(2.2);
    fixture.bodies.push(BodyView {
        id: 20,
        centre: at(cos * 36_000.0 + 9_000.0, sin * 36_000.0 - 4_000.0),
        radius_tiles: 400.0,
        theme: BodyTheme::Barren,
        atmosphere: 0.0,
        spin: 0.4,
        seed: 20,
    });
    fixture.belts.push(BeltView {
        centre: at(0.0, 0.0),
        radius_tiles: 63_000.0,
        width_tiles: 4_000.0,
        density: 1.6,
        seed: 5,
    });
    fixture.comets.push(CometView {
        centre: at(-30_000.0, -26_000.0),
        radius_tiles: 6.0,
        tail_angle: f64::atan2(-26_000.0, -30_000.0) as f32,
        tail_tiles: 7_000.0,
        seed: 9,
    });
}

/// A moon of radius 400 hanging in the fixture system (its body and its strip).
fn moon_body() -> BodyView {
    BodyView {
        id: 20,
        centre: at(-12_000.0, 30_000.0),
        radius_tiles: 400.0,
        theme: BodyTheme::Barren,
        atmosphere: 0.0,
        // Column 1,200 faces the star in the afternoon.
        spin: -3.7,
        seed: 20,
    }
}

/// The jungle world of radius 5,000.
fn jungle_body() -> BodyView {
    BodyView {
        id: 11,
        centre: at(-21_000.0, 29_000.0),
        radius_tiles: 5_000.0,
        theme: BodyTheme::Jungle,
        atmosphere: 0.08,
        // Column 12,000 faces the star.
        spin: -3.0,
        seed: 101,
    }
}

/// A star off to the side for the surface scenes' light, and the moon's parent world in the
/// sky.
fn moon_sky(fixture: &mut SpaceFixture) {
    fixture.stars.push(StarView {
        centre: at(-200_000.0, -40_000.0),
        radius_tiles: 1_600.0,
        class: StarClass::Yellow,
        seed: 3,
    });
    fixture.bodies.push(moon_body());
    fixture.bodies.push(BodyView {
        centre: at(-26_000.0, 18_000.0),
        ..jungle_body()
    });
}

/// A pilot, a parked rig, a creature and a drop on the ground round `column`.
fn surface_things(strip: &FixtureStrip, column: f64) -> Vec<GroundThing> {
    [
        (4.0, GroundThingKind::Pilot),
        (-7.0, GroundThingKind::ParkedRig),
        (11.0, GroundThingKind::Creature(0)),
        (7.5, GroundThingKind::Drop),
    ]
    .into_iter()
    .map(|(offset, kind)| GroundThing {
        column: column + offset,
        row: strip.ground_row(column + offset) as f32,
        kind,
    })
    .collect()
}

/// Every fixture's scene name, in order.
pub const NAMES: [&str; 19] = [
    "space_deep_starfield",
    "space_star_system",
    "space_planet_gallery",
    "space_approach_moon",
    "space_approach_planet",
    "space_moon_hover",
    "space_moon_surface",
    "space_moon_night",
    "space_moon_rolled",
    "space_planet_surface",
    "space_core_shaft",
    "space_polar_quads",
    "space_nebula",
    "space_black_hole",
    "space_asteroid_belt",
    "space_comet",
    "space_station_derelict",
    "space_wormhole",
    "space_warp",
];

/// The fixture named `name` (see [`NAMES`]; also `space_cruise`).
#[must_use]
#[allow(clippy::too_many_lines, reason = "one staged view per scene, in one table")]
pub fn build(name: &str) -> Option<SpaceFixture> {
    let camera = |x: f64, y: f64, roll: f32, px_per_tile: f32| SpaceCamera {
        centre: at(x, y),
        roll,
        px_per_tile,
    };
    let mut fixture = SpaceFixture::empty(camera(-140_000.0, 160_000.0, 0.0, 1.0));
    match name {
        "space_deep_starfield" => {
            fixture.stars.push(StarView {
                centre: at(-200_000.0, 120_000.0),
                radius_tiles: 1_600.0,
                class: StarClass::Blue,
                seed: 1,
            });
        }
        "space_star_system" => {
            system(&mut fixture);
            fixture.camera = camera(-6_000.0, 8_000.0, 0.0, 0.0055);
        }
        "space_planet_gallery" => {
            fixture.stars.push(StarView {
                centre: at(-400_000.0, -20_000.0),
                radius_tiles: 1_600.0,
                class: StarClass::White,
                seed: 2,
            });
            for (index, theme) in BodyTheme::ALL.into_iter().enumerate() {
                let (column, row) = ((index % 6) as f64, (index / 6) as f64);
                fixture.bodies.push(BodyView {
                    id: 200 + index as u64,
                    centre: at(-4_400.0 + column * 1_800.0, -900.0 + row * 2_000.0),
                    radius_tiles: 650.0,
                    theme,
                    atmosphere: if theme == BodyTheme::Barren { 0.0 } else { 0.12 },
                    spin: index as f32 * 0.7,
                    seed: 300 + index as u32,
                });
            }
            fixture.camera = camera(0.0, 0.0, 0.0, 0.12);
            fixture.craft = None;
        }
        "space_approach_moon" | "space_moon_hover" | "space_polar_quads" => {
            moon_sky(&mut fixture);
            let strip = moon(false);
            let ground_row = strip.geometry().surface_row as f32;
            let (altitude, zoom) = match name {
                "space_approach_moon" => (600.0, SpaceCamera::zoom_for_altitude(600.0, 400.0, 720.0)),
                _ => (36.0, 7.0),
            };
            fixture.stand_on(
                moon_body(),
                strip,
                (MOON_COLUMN, ground_row - altitude, 0.0),
                zoom,
                surface_things(strip, MOON_COLUMN),
            );
            fixture.force_quads = name == "space_polar_quads";
            fixture.craft = Some(CraftView { thrusting: true });
        }
        "space_moon_surface" | "space_moon_night" | "space_moon_rolled" => {
            moon_sky(&mut fixture);
            let strip = moon(false);
            let (column, roll) = match name {
                // Round the far side, where the star has set.
                "space_moon_night" => (MOON_COLUMN + 1_200.0, 0.0),
                // The craft turned two thirds of a half turn by the player.
                "space_moon_rolled" => (MOON_COLUMN, 2.1),
                _ => (MOON_COLUMN, 0.0),
            };
            let ground = strip.ground_row(column) as f32;
            let row = if name == "space_moon_rolled" {
                ground - 9.0
            } else {
                ground - 0.45
            };
            let things = surface_things(strip, column);
            fixture.stand_on(moon_body(), strip, (column, row, roll), 32.0, things);
            fixture.craft = Some(CraftView {
                thrusting: name == "space_moon_rolled",
            });
        }
        "space_planet_surface" | "space_approach_planet" => {
            fixture.stars.push(StarView {
                centre: at(-200_000.0, -60_000.0),
                radius_tiles: 1_600.0,
                class: StarClass::Yellow,
                seed: 3,
            });
            fixture.bodies.push(jungle_body());
            fixture.bodies.push(BodyView {
                centre: at(-13_000.0, 17_000.0),
                ..moon_body()
            });
            let strip = big_planet();
            let ground = strip.ground_row(BIG_COLUMN) as f32;
            if name == "space_planet_surface" {
                fixture.stand_on(
                    jungle_body(),
                    strip,
                    (BIG_COLUMN, ground - 0.45, 0.0),
                    32.0,
                    surface_things(strip, BIG_COLUMN),
                );
            } else {
                fixture.stand_on(
                    jungle_body(),
                    strip,
                    (BIG_COLUMN, ground - 2_000.0, 0.0),
                    SpaceCamera::zoom_for_altitude(2_000.0, 5_000.0, 720.0),
                    Vec::new(),
                );
                fixture.craft = Some(CraftView { thrusting: true });
            }
        }
        "space_core_shaft" => {
            moon_sky(&mut fixture);
            let strip = moon(true);
            let row = (strip.geometry().last_row() - 26) as f32;
            fixture.stand_on(moon_body(), strip, (MOON_COLUMN, row, 0.0), 9.0, Vec::new());
        }
        "space_nebula" => {
            fixture.nebulae.push(NebulaView {
                centre: at(-150_000.0, 158_000.0),
                radius_tiles: 9_000.0,
                colours: [palette::ROSE, palette::VIOLET],
                seed: 4,
            });
            fixture.nebulae.push(NebulaView {
                centre: at(-131_000.0, 166_000.0),
                radius_tiles: 6_000.0,
                colours: [palette::MINT, palette::TEAL],
                seed: 8,
            });
            fixture.stars.push(StarView {
                centre: at(-146_000.0, 157_000.0),
                radius_tiles: 300.0,
                class: StarClass::Nebula,
                seed: 6,
            });
            fixture.camera.px_per_tile = 0.045;
        }
        "space_black_hole" => {
            fixture.black_holes.push(BlackHoleView {
                centre: at(-138_500.0, 159_700.0),
                horizon_tiles: 260.0,
                disc_tiles: 1_300.0,
                tilt: -0.18,
            });
            fixture.camera.px_per_tile = 0.14;
        }
        "space_asteroid_belt" => {
            fixture.stars.push(StarView {
                centre: at(0.0, 0.0),
                radius_tiles: 1_600.0,
                class: StarClass::Red,
                seed: 3,
            });
            fixture.belts.push(BeltView {
                centre: at(0.0, 0.0),
                radius_tiles: 63_000.0,
                width_tiles: 4_000.0,
                density: 2.4,
                seed: 5,
            });
            let angle = 0.8_f64;
            fixture.camera = camera(angle.cos() * 63_200.0, angle.sin() * 63_200.0, 0.0, 1.6);
            fixture.asteroids = loose_rocks(fixture.camera.centre, 5);
            fixture.craft = Some(CraftView { thrusting: true });
        }
        "space_comet" => {
            fixture.stars.push(StarView {
                centre: at(-160_000.0, 150_000.0),
                radius_tiles: 1_600.0,
                class: StarClass::Yellow,
                seed: 3,
            });
            fixture.comets.push(CometView {
                centre: at(-140_250.0, 160_080.0),
                radius_tiles: 9.0,
                tail_angle: f64::atan2(10_080.0, 19_750.0) as f32,
                tail_tiles: 1_100.0,
                seed: 9,
            });
            fixture.camera.px_per_tile = 0.6;
        }
        "space_station_derelict" => {
            fixture.structures.push(StructureView {
                kind: StructureKind::Station,
                centre: at(-140_090.0, 159_985.0),
                size_tiles: 150.0,
                angle: 0.15,
                seed: 1,
            });
            fixture.structures.push(StructureView {
                kind: StructureKind::Derelict,
                centre: at(-139_880.0, 160_060.0),
                size_tiles: 120.0,
                angle: -0.5,
                seed: 2,
            });
            fixture.stars.push(StarView {
                centre: at(-60_000.0, 120_000.0),
                radius_tiles: 1_600.0,
                class: StarClass::Red,
                seed: 3,
            });
            fixture.camera.px_per_tile = 2.0;
            fixture.camera.roll = 0.25;
        }
        "space_wormhole" => {
            fixture.wormholes.push(WormholeView {
                centre: at(-139_820.0, 159_930.0),
                radius_tiles: 90.0,
                tint: palette::LAVENDER,
                seed: 4,
            });
            fixture.camera.px_per_tile = 0.6;
        }
        "space_warp" | "space_cruise" => {
            fixture.motion = if name == "space_warp" {
                FlightMotion::Warp {
                    charge: 0.85,
                    heading: -0.35,
                }
            } else {
                FlightMotion::Cruise {
                    speed: 520.0,
                    heading: -0.35,
                }
            };
            fixture.camera.roll = 0.35 + std::f32::consts::FRAC_PI_2;
            fixture.craft = Some(CraftView { thrusting: true });
            if name == "space_cruise" {
                fixture.bodies.push(BodyView {
                    centre: at(-141_400.0, 160_700.0),
                    ..moon_body()
                });
                fixture.stars.push(StarView {
                    centre: at(-200_000.0, 120_000.0),
                    radius_tiles: 1_600.0,
                    class: StarClass::Yellow,
                    seed: 1,
                });
                fixture.camera.px_per_tile = 0.5;
            }
        }
        _ => return None,
    }
    Some(fixture)
}

/// Seeds a few loose rocks round the camera (the belt scene's foreground).
#[must_use]
pub fn loose_rocks(around: WorldPos, count: u32) -> Vec<AsteroidView> {
    (0..count)
        .map(|index| {
            let h = hash64(&[i64::from(index), 77]);
            let (dx, dy) = (
                (unit(h, 0) - 0.5) * 900.0 * TILE_SIZE,
                (unit(h, 16) - 0.5) * 500.0 * TILE_SIZE,
            );
            AsteroidView {
                centre: around.offset(dx, dy),
                radius_tiles: 4.0 + 24.0 * unit(h, 32).powi(2),
                seed: index * 31 + 5,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_builds_and_stands_where_it_says() {
        for name in NAMES {
            let fixture = build(name).unwrap_or_else(|| panic!("{name}"));
            let view = fixture.view();
            assert!(view.camera.px_per_tile > 0.0, "{name}");
            if let Some(ground) = view.ground {
                let geometry = ground.strip.geometry();
                // The camera stands over the body's centre at its row's radius.
                let distance = view.camera.centre.distance(ground.body.centre) / TILE_SIZE;
                let expected = geometry.radius_of_row(ground.row);
                assert!((distance - expected).abs() < 0.05, "{name}: {distance} vs {expected}");
            }
        }
        assert!(build("space_cruise").is_some());
        assert!(build("nope").is_none());
    }

    #[test]
    fn the_moon_strip_is_concentric_and_reaches_the_core() {
        let strip = moon(true);
        let geometry = strip.geometry();
        assert_eq!(geometry.surface_columns, 2_513);
        let terrain = strip.terrain();
        // The shaft is open from the surface to the core.
        for row in terrain.surface_row(MOON_COLUMN as i32)..=geometry.last_row() {
            let column = (MOON_COLUMN as i64 >> geometry.band(row)) as i32;
            let tile = terrain.tile(TilePosition { x: column, y: row });
            assert!(tile.is_some_and(|tile| tile.kind.is_open()), "row {row}");
        }
    }
}
