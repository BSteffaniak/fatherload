//! Theme worlds (Wave 12): the planet themes that are whole worlds of their own, each "a world
//! like the home ground level" (expansion plan §16), registered by theme id.
//!
//! A theme in [`super::theme::PLANET_THEMES`] is plain data (palette, crust, strata, ores) that
//! the generic generator ([`super::PlanetGround`]) lays out as hills, ragged concentric strata,
//! caves and a core chamber. A **theme world** builds on that: its own surface shape, its own
//! features over the strata, its own strata depth, and the sites the other tracks build on.
//! [`world_of`] reads the theme framework's registry ([`super::hooks::THEME_HOOKS`]): a theme's
//! hooks name its world (Wave 12 planets-a's jungle, ocean archipelago and glacier worlds in
//! [`super::wilds`]).

use super::{PlanetGround, PlanetParams, PlanetThemeId};
use crate::{
    polar::PolarGrid,
    terrain::{ChunkPosition, Tile},
};

/// A spot on a body's surface another system builds on: strip columns `first..first + width`
/// (wrapping), level ground at row `floor` (the first ground row under it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceSite {
    pub first: i32,
    pub width: i32,
    pub floor: i32,
}

impl SurfaceSite {
    /// The site's middle column.
    #[must_use]
    pub const fn centre(&self) -> i32 {
        self.first + self.width / 2
    }
}

/// A theme that is a whole world (see the module docs).
pub trait ThemeWorld: Sync {
    /// The theme this world generates.
    fn theme(&self) -> PlanetThemeId;

    /// The first row of the indestructible core floor (the core chamber sits on it): the
    /// bottom of the theme's strata. `None` keeps the polar grid's (the body's true core).
    fn core_floor_row(&self, params: &PlanetParams, grid: &PolarGrid) -> Option<i32>;

    /// Extra open rows over the highest hill (tall trees, spires, towers).
    fn extra_sky_rows(&self) -> i32 {
        0
    }

    /// The first ground row of strip column `column` (columns wrap; `ground`'s generic hills
    /// are [`PlanetGround::hill`]).
    fn surface_row(&self, ground: &PlanetGround, column: i64) -> i32;

    /// The generated tiles of `chunk` (row-major, columns wrapped into the strip).
    fn chunk_tiles(&self, ground: &PlanetGround, chunk: ChunkPosition) -> Vec<Tile>;

    /// Where a friendly settlement stands: level, dry and clear (the aliens track builds on it).
    fn settlement_site(&self, ground: &PlanetGround) -> Option<SurfaceSite>;

    /// Every settlement site, [`Self::settlement_site`] first (Wave 13 QA: a settled world
    /// holds several alien towns, each on its own level site round the lap).
    fn settlement_sites(&self, ground: &PlanetGround) -> Vec<SurfaceSite> {
        self.settlement_site(ground).into_iter().collect()
    }

    /// The theme's landmark (its centre site).
    fn landmark_site(&self, ground: &PlanetGround) -> Option<SurfaceSite>;
}

/// The theme world of `theme`, if it is one: the theme's hooks say
/// ([`super::hooks::PlanetThemeHooks::world`]; one registry, Wave 12 integration).
#[must_use]
pub fn world_of(theme: PlanetThemeId) -> Option<&'static dyn ThemeWorld> {
    super::hooks::hooks(theme).world()
}
