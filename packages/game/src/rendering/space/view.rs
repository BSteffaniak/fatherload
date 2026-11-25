//! What the space renderer draws: a plain-data picture of the space around one seat, and the
//! polar strip of the body it stands on.
//!
//! TODO-SEAM (Wave 11 integration): everything here is this track's *local* interface. The
//! space generator (`simulation::space`) owns the real systems, bodies, belts and the rest; the
//! polar track (`simulation::polar` / `simulation::planet`) owns the ring geometry and the
//! planet's ground; the flight track (`simulation::flight`) owns the craft, cruise and warp;
//! the regions track (`simulation::place`) owns `BodyId` / `Place`. The integrator fills a
//! [`SpaceRenderView`] from the render snapshot (one per seat, positions as [`WorldPos`],
//! never `f32` absolutes) and implements [`PolarStrip`] for the body's ground; nothing in the
//! renderer reads the simulation beyond these types.

use fatherload_simulation::{
    position::WorldPos,
    star_chart::StarClass,
    terrain::{TILE_SIZE, Terrain, TilePosition},
};
use raylib::prelude::Color;

use crate::screen_fx::EffectsIntensity;

use super::super::pixel_art::palette;

/// Screen points per tile at the ground zoom (the home world's camera).
pub const GROUND_PX_PER_TILE: f32 = 32.0;
/// Screen points per tile far out in a star system.
pub const SYSTEM_PX_PER_TILE: f32 = 0.004;

/// Rows over the ground that keep the ground zoom.
const HOVER_ROWS: f32 = 6.0;

/// Where the seat's camera is and how it looks: always locked to the craft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceCamera {
    /// The craft (the camera's centre), an exact world position.
    pub centre: WorldPos,
    /// The craft's orientation in the space frame (radians, clockwise on screen). The screen
    /// turns only because the player turned the craft: the world is drawn turned by `-roll`.
    pub roll: f32,
    /// Zoom: screen points per tile.
    pub px_per_tile: f32,
}

impl SpaceCamera {
    /// Screen points per world pixel.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.px_per_tile / TILE_SIZE
    }

    /// The smooth approach zoom over a body of `radius_tiles`, `altitude_tiles` above its
    /// surface, in a view `view_height` points tall: the ground zoom at the surface, and from
    /// far away a zoom that keeps the whole body (centre and far side) inside the view below
    /// the craft, blended harmonically so it never jumps; never below the system zoom. The
    /// flight track picks the body (the nearest) and the altitude; this keeps every scene and
    /// the real flight on one curve.
    #[must_use]
    pub fn zoom_for_altitude(altitude_tiles: f32, radius_tiles: f32, view_height: f32) -> f32 {
        // The first few rows keep the ground zoom (taking off and landing).
        let altitude = (altitude_tiles - HOVER_ROWS).max(0.0);
        let radius = radius_tiles.max(1.0);
        // How much of the far side must fit: none at the surface, all of it far away.
        let reach = altitude + radius * altitude / (altitude + radius);
        let fit = 0.34 * view_height.max(1.0);
        (1.0 / (1.0 / GROUND_PX_PER_TILE + reach / fit)).max(SYSTEM_PX_PER_TILE)
    }
}

/// A sun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    pub class: StarClass,
    /// Seeds its flares and spikes.
    pub seed: u32,
}

/// The look of a round body (TODO-SEAM: the space generator's planet themes, Wave 12-13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyTheme {
    /// Grey dust and craters (small moons, the Moon).
    Barren,
    Ice,
    Desert,
    Jungle,
    Ocean,
    Volcanic,
    Crystal,
    Fungal,
    Machine,
    GasGiant,
    /// A dead world with ruins.
    Dead,
}

impl BodyTheme {
    pub const ALL: [Self; 11] = [
        Self::Barren,
        Self::Ice,
        Self::Desert,
        Self::Jungle,
        Self::Ocean,
        Self::Volcanic,
        Self::Crystal,
        Self::Fungal,
        Self::Machine,
        Self::GasGiant,
        Self::Dead,
    ];

    /// The surface's three band colours (dark, mid, light) and its atmosphere's colour.
    #[must_use]
    pub const fn palette(self) -> ([Color; 3], Color) {
        use palette::{
            AMBER, CLAY, COPPER, CREAM, DEEP_BLUE, FOG, GOLD, ICE, LAVENDER, LEAF, LIME, LOAM,
            MAROON, MINT, MIST, MOSS, OCEAN, ORANGE, PINE, PINK, PLUM, ROSE, RUST, SAND, SKY,
            SKY_LIGHT, SLATE, SLATE_LIGHT, SOIL, STEEL, STEEL_DARK, STEEL_LIGHT, STONE, STONE_DARK,
            STONE_LIGHT, TEAL, VIOLET,
        };
        match self {
            Self::Barren => ([STONE_DARK, STONE, STONE_LIGHT], FOG),
            Self::Ice => ([SLATE_LIGHT, SKY_LIGHT, ICE], SKY_LIGHT),
            Self::Desert => ([CLAY, SAND, CREAM], AMBER),
            Self::Jungle => ([PINE, MOSS, LEAF], SKY),
            Self::Ocean => ([DEEP_BLUE, OCEAN, LIME], SKY_LIGHT),
            Self::Volcanic => ([STONE_DARK, MAROON, ORANGE], RUST),
            Self::Crystal => ([PLUM, VIOLET, LAVENDER], PINK),
            Self::Fungal => ([PLUM, ROSE, MINT], LAVENDER),
            Self::Machine => ([STEEL_DARK, STEEL, STEEL_LIGHT], TEAL),
            Self::GasGiant => ([COPPER, GOLD, CREAM], AMBER),
            Self::Dead => ([SOIL, LOAM, SLATE], MIST),
        }
    }

    /// Whether the surface reads as stripes of weather (gas giants) instead of land.
    #[must_use]
    pub const fn banded(self) -> bool {
        matches!(self, Self::GasGiant)
    }
}

/// A round body seen from space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyView {
    /// TODO-SEAM: `simulation::place::BodyId` (Home, Moon, or a generated body).
    pub id: u64,
    pub centre: WorldPos,
    /// Surface radius (tiles). Landable bodies are 400 tiles or more.
    pub radius_tiles: f32,
    pub theme: BodyTheme,
    /// Atmosphere thickness as a share of the radius (0: airless).
    pub atmosphere: f32,
    /// The surface's turn (radians): where column 0 is. Turns the surface bands with it.
    pub spin: f32,
    /// Seeds its surface pattern.
    pub seed: u32,
}

/// An asteroid belt: a ring of rocks around `centre` (a star).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeltView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    pub width_tiles: f32,
    /// Rocks per cell, on average (0..=3).
    pub density: f32,
    pub seed: u32,
}

/// One loose asteroid (belts make their own; these are the ones the generator places).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AsteroidView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    pub seed: u32,
}

/// A coloured cloud.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NebulaView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    /// Its two colours (core, edge).
    pub colours: [Color; 2],
    pub seed: u32,
}

/// A comet: a nucleus with a tail pointing away from its star.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CometView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    /// The tail's direction in the space frame (radians, 0 = right, clockwise).
    pub tail_angle: f32,
    pub tail_tiles: f32,
    pub seed: u32,
}

/// What kind of built thing floats here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureKind {
    /// A broken hulk (placeholder art Wave 12 replaces).
    Derelict,
    /// A working station hub (placeholder art Wave 12 replaces).
    Station,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureView {
    pub kind: StructureKind,
    pub centre: WorldPos,
    /// Its span (tiles).
    pub size_tiles: f32,
    /// Its own turn in the space frame (radians).
    pub angle: f32,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlackHoleView {
    pub centre: WorldPos,
    /// The event horizon's radius (tiles).
    pub horizon_tiles: f32,
    /// The accretion disc's outer radius (tiles).
    pub disc_tiles: f32,
    /// The disc's tilt (radians in the space frame).
    pub tilt: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WormholeView {
    pub centre: WorldPos,
    pub radius_tiles: f32,
    /// The colour of the far side's light.
    pub tint: Color,
    pub seed: u32,
}

/// How the craft is moving, for the speed and warp effects (TODO-SEAM: `simulation::flight`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FlightMotion {
    /// Thrusters only (no streaks).
    #[default]
    Coasting,
    /// Star Drive cruise: `speed` tiles a second along `heading` (space frame, radians).
    Cruise { speed: f32, heading: f32 },
    /// Warp: `charge` 0..=1 (spool up, the streaks lengthen), along `heading`.
    Warp { charge: f32, heading: f32 },
}

/// The craft at the camera's centre, drawn upright (the camera is locked to it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CraftView {
    /// Thrusters burning (flames).
    pub thrusting: bool,
    /// The seat's own rig (`RigLook::from_player`: its paint job and tiers), so split-screen
    /// seats tell their craft apart; `None` draws the first player's default (the fixtures).
    pub look: Option<crate::rendering::rig_art::RigLook>,
}

/// Everything one seat sees of space this frame.
#[derive(Clone, Copy, Debug)]
pub struct SpaceRenderView<'a> {
    pub camera: SpaceCamera,
    /// Seeds the deep starfield (the world seed): the same sky for every peer.
    pub sky_seed: u64,
    /// Presentation clock (seconds): twinkles, spins, tails, streaks.
    pub seconds: f32,
    pub stars: &'a [StarView],
    pub bodies: &'a [BodyView],
    pub belts: &'a [BeltView],
    pub asteroids: &'a [AsteroidView],
    pub nebulae: &'a [NebulaView],
    pub comets: &'a [CometView],
    pub structures: &'a [StructureView],
    pub black_holes: &'a [BlackHoleView],
    pub wormholes: &'a [WormholeView],
    pub motion: FlightMotion,
    pub craft: Option<CraftView>,
    /// The seat's ground, when a body is near enough to draw its tiles (on or over it).
    pub ground: Option<GroundView<'a>>,
    /// The screen effects setting (lensing and streaks follow it).
    pub effects: EffectsIntensity,
}

impl SpaceRenderView<'_> {
    /// The star lighting `point` (the nearest one), as a unit direction toward it in the space
    /// frame, or straight up-left when the sky has no star.
    #[must_use]
    pub fn light_toward(&self, point: WorldPos) -> (f32, f32) {
        self.stars
            .iter()
            .map(|star| point.delta_f64(star.centre))
            .min_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
            .map_or((-0.6, -0.8), |(dx, dy)| {
                let length = dx.hypot(dy).max(1e-9);
                #[allow(clippy::cast_possible_truncation, reason = "a unit vector")]
                ((dx / length) as f32, (dy / length) as f32)
            })
    }
}

/// The ring layout of a body's ground (TODO-SEAM: `simulation::polar`'s geometry).
///
/// Row `surface_row` is the first ground row: its top edge sits at `radius_tiles` from the
/// centre, and each row below is one tile closer. Columns are angles: the surface ring has
/// `surface_columns`, and below each row in `halvings` the ring has half as many (so tiles
/// stay between about 0.7x and 1.4x square); a tile of band `k` is column `c` of
/// `surface_columns >> k`, stored at terrain column `c`.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarGeometry {
    pub radius_tiles: f32,
    pub surface_row: i32,
    pub surface_columns: u32,
    /// Rows (ascending) from which the columns halve again. At most [`MAX_HALVINGS`].
    pub halvings: Vec<i32>,
    /// The core chamber's radius (tiles): no tiles inside it.
    pub core_radius_tiles: f32,
}

/// The polar shader takes this many halvings.
pub const MAX_HALVINGS: usize = 8;

impl PolarGeometry {
    /// The standard ring layout for a body of `radius_tiles` (the plan's rule): the surface
    /// ring has `round(2 pi R)` columns, and a band halves once its tiles would be narrower
    /// than 0.7 of a tile; the core chamber is the last 12 tiles.
    #[must_use]
    pub fn standard(radius_tiles: f32, surface_row: i32) -> Self {
        let radius = radius_tiles.max(1.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a ring of at most a few hundred thousand columns"
        )]
        let surface_columns = (std::f32::consts::TAU * radius).round().max(8.0) as u32;
        let mut halvings = Vec::new();
        let mut width = 1.0_f32;
        // The core chamber: 12 tiles, or more on a giant whose eight halvings would run out
        // before it (the shader takes eight).
        let core = 12.0_f32.min(radius * 0.25).max(radius * 0.7 / 256.0 * 1.05);
        let mut depth = 0;
        while halvings.len() < MAX_HALVINGS {
            // The tile width at the bottom of row `depth` is width * (R - depth - 1) / R.
            #[allow(clippy::cast_precision_loss, reason = "rows are small")]
            let bottom = radius - depth as f32 - 1.0;
            if bottom <= core {
                break;
            }
            if width * bottom / radius < 0.7 {
                halvings.push(surface_row + depth);
                width *= 2.0;
            }
            depth += 1;
        }
        Self {
            radius_tiles: radius,
            surface_row,
            surface_columns,
            halvings,
            core_radius_tiles: core,
        }
    }

    /// How many times the columns have halved at `row`.
    #[must_use]
    pub fn band(&self, row: i32) -> u32 {
        #[allow(clippy::cast_possible_truncation, reason = "at most eight halvings")]
        let band = self.halvings.iter().filter(|start| row >= **start).count() as u32;
        band
    }

    /// Columns in the ring at `row`.
    #[must_use]
    pub fn columns_at(&self, row: i32) -> u32 {
        self.columns_in_band(self.band(row))
    }

    /// Columns in a ring of band `band` (`surface_columns >> band`: an odd ring rounds down,
    /// so a band's columns are not always exactly half the band above's).
    #[must_use]
    pub fn columns_in_band(&self, band: u32) -> u32 {
        (self.surface_columns >> band.min(31)).max(1)
    }

    /// Band `band`'s continuous column at the angle of surface column `column`.
    #[must_use]
    pub fn band_column(&self, band: u32, column: f64) -> f64 {
        column * f64::from(self.columns_in_band(band)) / f64::from(self.surface_columns.max(1))
    }

    /// The surface column at the angle of band `band`'s continuous column `band_column`.
    #[must_use]
    pub fn surface_column(&self, band: u32, band_column: f64) -> f64 {
        band_column * f64::from(self.surface_columns.max(1)) / f64::from(self.columns_in_band(band))
    }

    /// Radius (tiles) of the top edge of `row`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "rows are small")]
    pub fn radius_of_row(&self, row: f32) -> f32 {
        self.radius_tiles - (row - self.surface_row as f32)
    }

    /// The row (continuous) at `radius` tiles from the centre.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "rows are small")]
    pub fn row_at_radius(&self, radius: f32) -> f32 {
        self.surface_row as f32 + (self.radius_tiles - radius)
    }

    /// The deepest row with tiles (the row whose bottom touches the core chamber).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "rows are small")]
    pub fn last_row(&self) -> i32 {
        self.row_at_radius(self.core_radius_tiles).ceil() as i32 - 1
    }
}

/// A body's ground as the renderer reads it (TODO-SEAM: the polar track's planet strip).
pub trait PolarStrip {
    fn geometry(&self) -> &PolarGeometry;
    /// The unrolled ground: band `k`'s column `c` at terrain column `c` (see
    /// [`PolarGeometry`]). Painted with the home world's tile art.
    fn terrain(&self) -> &Terrain;
    /// Where band `band`'s ring column `column` (already wrapped into the ring) of `row` is
    /// kept in [`Self::terrain`]; `None` for ground the strip does not hold.
    fn tile_at(&self, band: u32, column: u32, row: i32) -> Option<TilePosition> {
        let _ = band;
        Some(TilePosition {
            x: i32::try_from(column).ok()?,
            y: row,
        })
    }
    /// A number that changes whenever a tile changes (the renderer repaints then).
    fn revision(&self) -> u64;
}

/// Something standing or lying on the ground, drawn in the local up frame of where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundThing {
    /// Surface column (continuous; wraps).
    pub column: f64,
    /// Row (continuous; its feet).
    pub row: f32,
    pub kind: GroundThingKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundThingKind {
    /// The pilot on foot (standing against gravity).
    Pilot,
    /// A creature (one of the existing sprites, by index into the renderer's list).
    Creature(u8),
    /// A dropped ore lump.
    Drop,
    /// A parked rig.
    ParkedRig,
}

/// The seat's view of a body's ground.
#[derive(Clone, Copy)]
pub struct GroundView<'a> {
    pub body: BodyView,
    pub strip: &'a dyn PolarStrip,
    /// The camera (the craft) in strip coordinates: surface column (continuous, f64 so a
    /// 60,000-column world is still exact) and row (continuous; above the surface is
    /// negative of it).
    pub column: f64,
    pub row: f32,
    /// The craft's orientation relative to the local up at the camera (radians, clockwise).
    pub roll: f32,
    /// Ambient light the body's night side keeps (0..1).
    pub night_light: f32,
    pub things: &'a [GroundThing],
}

impl std::fmt::Debug for GroundView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroundView")
            .field("body", &self.body.id)
            .field("column", &self.column)
            .field("row", &self.row)
            .field("roll", &self.roll)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zoom_eases_from_the_ground_to_the_system() {
        let zoom =
            |altitude: f32, radius: f32| SpaceCamera::zoom_for_altitude(altitude, radius, 720.0);
        assert!((zoom(0.0, 400.0) - GROUND_PX_PER_TILE).abs() < 1e-4);
        assert!((zoom(4.0, 5_000.0) - GROUND_PX_PER_TILE).abs() < 1e-4);
        assert!((zoom(1e9, 400.0) - SYSTEM_PX_PER_TILE).abs() < 1e-6);
        // Coming in, the body's centre stays in the view below the craft (the camera is locked
        // to the craft); far out, the whole body does.
        for (altitude, radius) in [(600.0, 400.0), (2_000.0, 5_000.0), (40_000.0, 9_000.0)] {
            let z = zoom(altitude, radius);
            assert!(
                altitude * z < 300.0,
                "{altitude} {radius}: surface off screen"
            );
            if altitude >= 1.5 * radius {
                let far_side = (altitude + 2.0 * radius) * z;
                assert!(far_side < 720.0, "{altitude} {radius}: {far_side}");
            }
            assert!(radius * z > 20.0, "{altitude} {radius}: too small");
        }
        // Monotonic and never jumping by more than a few percent per small step.
        let mut last = GROUND_PX_PER_TILE;
        for step in 0..320 {
            let altitude = 1.05_f32.powi(step);
            let z = zoom(altitude, 1_500.0);
            assert!(z <= last + 1e-6);
            assert!(z >= last * 0.9, "{altitude}: {last} -> {z}");
            last = z;
        }
    }

    #[test]
    fn standard_rings_keep_tiles_near_square_down_to_the_core() {
        for radius in [400.0_f32, 1_500.0, 5_000.0] {
            let geometry = PolarGeometry::standard(radius, 10);
            assert_eq!(
                geometry.surface_columns,
                (std::f32::consts::TAU * radius).round() as u32
            );
            assert!(!geometry.halvings.is_empty());
            for row in geometry.surface_row..=geometry.last_row() {
                let columns = geometry.columns_at(row) as f32;
                let top = geometry.radius_of_row(row as f32);
                let bottom = top - 1.0;
                let top_width = std::f32::consts::TAU * top / columns;
                let bottom_width = std::f32::consts::TAU * bottom / columns;
                // Rows are whole tiles, so the last rows by the core run a little wide.
                assert!(top_width <= 1.5, "R {radius} row {row}: {top_width}");
                assert!(bottom_width >= 0.65, "R {radius} row {row}: {bottom_width}");
            }
            assert!(
                geometry.radius_of_row(geometry.last_row() as f32 + 1.0)
                    <= geometry.core_radius_tiles + 0.5
            );
        }
    }

    #[test]
    fn big_worlds_reach_the_eighth_band_and_odd_rings_keep_their_angles() {
        // R = 5,000 halves eight times before its core: the polar shader must draw band 8
        // (its offsets and ring come in `uBand8`, beside the two vec4s of bands 0..7).
        let big = PolarGeometry::standard(5_000.0, 0);
        assert_eq!(big.halvings.len(), MAX_HALVINGS);
        assert_eq!(big.band(big.last_row()), 8);
        // The moon's ring is odd (2,513): band 5 has 78 columns, not 2,513 / 32 = 78.53, so
        // the same angle is a band column in proportion to the band's own ring.
        let moon = PolarGeometry::standard(400.0, 0);
        assert_eq!(moon.columns_in_band(5), 78);
        let half_way = f64::from(moon.surface_columns) * 0.5;
        assert!((moon.band_column(5, half_way) - 39.0).abs() < 1e-9);
        assert!((moon.surface_column(5, 39.0) - half_way).abs() < 1e-9);
    }

    #[test]
    fn rows_and_radii_round_trip() {
        let geometry = PolarGeometry::standard(400.0, 30);
        assert!((geometry.radius_of_row(30.0) - 400.0).abs() < 1e-4);
        assert!((geometry.row_at_radius(390.0) - 40.0).abs() < 1e-4);
        assert_eq!(geometry.band(29), 0);
        assert_eq!(geometry.columns_at(30), geometry.surface_columns);
    }
}
