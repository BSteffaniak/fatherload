//! The point-evaluable base layer of a streamed world: the surface heightmap, the bedrock
//! floor, the town's foundations, each band's rock mass and the raw cavern noise.
//!
//! Everything here is a pure function of the settings and the tile coordinates, so any tile
//! can be evaluated on its own, in any order, by any chunk.

use crate::{
    surface::building_foundation_at,
    terrain::{SurfaceMaterial, TileKind},
    village::placement::{self, FLATTEN_FADE_COLUMNS},
    worldgen::{
        TOWN_CLEAN_TOPSOIL_MARGIN, WorldSettings, biome, caves, contrast_threshold,
        noise::{NOISE_MAX, fractal_noise, line_noise, value_noise},
        pois::TIER_ROWS,
        rock_texture,
        strata::{
            self, BOUNDARY_COUNT, Band, SKY_ROWS, band_at_with_bases, band_layout_rows,
            bedrock_top_row,
        },
        town_columns,
    },
};

/// First column of the flat ground around the town (the classic world's span plus a margin).
pub const TOWN_FLAT_FIRST_COLUMN: i32 = -16;
/// Last column of the flat ground around the town.
pub const TOWN_FLAT_LAST_COLUMN: i32 = 255;
/// How far the placeholder surface may dip below the classic surface row.
pub const MAX_SURFACE_DIP_ROWS: i32 = 2;
/// Columns over which the surface eases from flat town ground into the open heightmap.
const SURFACE_FADE_COLUMNS: i32 = 32;
const SALT_SURFACE: u64 = 0x5_5ACE_0001;

/// The first ground row of column `x` of a streamed world (the surface heightmap).
///
/// The first ground row of column `x` of a streamed world (the surface heightmap hook).
///
/// On biome worlds ([`GeneratorVersion::V7`](crate::worldgen::GeneratorVersion::V7)) the
/// biome heightmap ([`biome::surface_row`]), which already lays every village site
/// ([`placement::surface_site`]) on a flat shelf and eases its slopes around it.
///
/// On older worlds with villages ([`GeneratorVersion::V6`](crate::worldgen::GeneratorVersion::V6))
/// it is the raw heightmap ([`raw_surface_row`]) with village sites flattened: across a
/// village's span the ground sits at the raw row of the village's centre column, easing back to
/// the raw heightmap over [`FLATTEN_FADE_COLUMNS`] either side. The rest of the generator only
/// reads this function, so the town, spawn and villages stay put whatever the heightmap does.
#[must_use]
pub fn surface_row(settings: &WorldSettings, x: i32) -> i32 {
    if settings.generator.has_biomes() {
        return biome::surface_row(settings, x);
    }
    let raw = placeholder_surface_row(settings, x);
    if !settings.generator.has_villages() {
        return raw;
    }
    placement::surface_site_at(settings.seed, x, FLATTEN_FADE_COLUMNS).map_or(raw, |site| {
        let ground = village_ground_row(settings, site.center);
        let distance = site.distance(x);
        ground + (raw - ground) * distance / (FLATTEN_FADE_COLUMNS + 1)
    })
}

/// The ground row of a surface village centred on column `center`.
///
/// On biome worlds the row of the village's flat shelf; on older worlds the raw heightmap's
/// row there.
#[must_use]
pub fn village_ground_row(settings: &WorldSettings, center: i32) -> i32 {
    raw_surface_row(settings, center)
}

/// The surface heightmap of a streamed world before V6 villages flatten their sites: the biome
/// heightmap on biome worlds (its village shelves included), the placeholder swell otherwise.
#[must_use]
pub fn raw_surface_row(settings: &WorldSettings, x: i32) -> i32 {
    if settings.generator.has_biomes() {
        return biome::surface_row(settings, x);
    }
    placeholder_surface_row(settings, x)
}

/// The first row of a streamed world.
///
/// Sky worlds (V8) reach up to the Moon ([`crate::worldgen::sky::SkyBands::ceiling_rows`] of
/// the world's bands, [`crate::worldgen::sky::SKY_CEILING_ROWS`] in V8); biome worlds keep
/// [`biome::SKY_HEADROOM_ROWS`] above row 0 for their mountains; older streamed worlds start at
/// row 0.
#[must_use]
pub const fn top_row(settings: &WorldSettings) -> i32 {
    if let Some(bands) = settings.generator.sky_bands() {
        -bands.ceiling_rows()
    } else if settings.generator.has_biomes() {
        -biome::SKY_HEADROOM_ROWS
    } else {
        0
    }
}

/// The deepest surface row any column of a streamed world can have (rows below it are below
/// every column's surface).
#[must_use]
pub const fn deepest_surface_row(settings: &WorldSettings) -> i32 {
    if settings.generator.has_biomes() {
        SKY_ROWS + biome::MAX_DIP_ROWS
    } else {
        SKY_ROWS + MAX_SURFACE_DIP_ROWS
    }
}

/// The placeholder surface of V5 and V6 worlds: flat at [`SKY_ROWS`] across the town
/// ([`TOWN_FLAT_FIRST_COLUMN`] to [`TOWN_FLAT_LAST_COLUMN`]); elsewhere a gentle swell up to
/// [`MAX_SURFACE_DIP_ROWS`] lower.
const fn placeholder_surface_row(settings: &WorldSettings, x: i32) -> i32 {
    let distance = if x < TOWN_FLAT_FIRST_COLUMN {
        TOWN_FLAT_FIRST_COLUMN - x
    } else if x > TOWN_FLAT_LAST_COLUMN {
        x - TOWN_FLAT_LAST_COLUMN
    } else {
        return SKY_ROWS;
    };
    let wave = (line_noise(settings.seed, SALT_SURFACE, x, 41) * 2
        + line_noise(settings.seed, SALT_SURFACE ^ 0xFF, x, 13))
        / 3;
    let dip = wave * (MAX_SURFACE_DIP_ROWS + 1) / (NOISE_MAX + 1);
    let fade = if distance < SURFACE_FADE_COLUMNS {
        distance
    } else {
        SURFACE_FADE_COLUMNS
    };
    SKY_ROWS + dip * fade / SURFACE_FADE_COLUMNS
}

/// One tier of point-of-interest placement: a band (or a slice of a thick band) and its rows.
#[derive(Clone, Copy, Debug)]
pub struct Tier {
    pub band: Band,
    /// Region key row: `band index * 64 + tier ordinal` (as V4 keys its tiers).
    pub key: i32,
    /// Which slice of the band this is, and how many slices the band has.
    pub ordinal: usize,
    pub count: usize,
    pub top: i32,
    pub bottom: i32,
}

/// Generation context: the settings plus values derived from them once.
pub struct Ctx {
    pub settings: WorldSettings,
    pub bases: [i32; BOUNDARY_COUNT],
    pub height: i32,
    /// The world's first row ([`top_row`]).
    pub top: i32,
    /// The first row of the ground layer (mountains): [`top_row`] without the sky. Rows above
    /// it are the sky generator's ([`crate::worldgen::sky`]) or open air.
    pub ground_top: i32,
    /// Whether the world has a sky (V8).
    pub sky: bool,
    /// Whether the world has surface biomes.
    pub biomes: bool,
    /// [`deepest_surface_row`].
    pub deepest_surface: i32,
    /// Point-of-interest tiers, top to bottom.
    pub tiers: Vec<Tier>,
}

impl Ctx {
    #[must_use]
    pub fn new(settings: &WorldSettings) -> Self {
        let mut tiers = Vec::new();
        for band in Band::ALL {
            let (top, bottom) = band_layout_rows(settings, band);
            let count = ((bottom - top + 1) / TIER_ROWS).max(1);
            let rows = (bottom - top + 1) / count;
            for ordinal in 0..count {
                let first = top + ordinal * rows;
                let last = if ordinal + 1 == count {
                    bottom
                } else {
                    first + rows - 1
                };
                tiers.push(Tier {
                    band,
                    key: i32::try_from(band.index() * 64).unwrap_or(0) + ordinal,
                    ordinal: usize::try_from(ordinal).unwrap_or(0),
                    count: usize::try_from(count).unwrap_or(1),
                    top: first,
                    bottom: last,
                });
            }
        }
        Self {
            settings: *settings,
            bases: strata::boundary_bases(settings),
            height: settings.height_tiles(),
            top: top_row(settings),
            ground_top: if settings.generator.has_biomes() {
                -biome::SKY_HEADROOM_ROWS
            } else {
                0
            },
            sky: settings.generator.has_sky(),
            biomes: settings.generator.has_biomes(),
            deepest_surface: deepest_surface_row(settings),
            tiers,
        }
    }

    pub const fn seed(&self) -> u64 {
        self.settings.seed
    }

    pub const fn band(&self, x: i32, y: i32) -> Band {
        band_at_with_bases(&self.settings, &self.bases, x, y)
    }

    /// The first bedrock row of column `x`.
    pub const fn floor_row(&self, x: i32) -> i32 {
        match bedrock_top_row(&self.settings, x) {
            Some(row) => row,
            None => self.height,
        }
    }

    pub fn surface_row(&self, x: i32) -> i32 {
        surface_row(&self.settings, x)
    }

    /// Whether `(x, y)` is above column `x`'s surface (skipping the heightmap for rows below
    /// every surface).
    fn above_surface(&self, x: i32, y: i32) -> bool {
        y <= self.deepest_surface && y < self.surface_row(x)
    }

    /// The base tile at `(x, y)`: sky, bedrock, foundations or the band's rock mass (no caves,
    /// hazards, points of interest or deposits yet). `None` outside the world's rows.
    pub fn base_kind(&self, x: i32, y: i32) -> Option<TileKind> {
        if y < self.top || y >= self.height {
            return None;
        }
        Some(self.base_kind_in(x, y, self.band(x, y)))
    }

    /// The sky, bedrock or foundation tile at `(x, y)`, or `None` where the base is the band's
    /// rock (which [`Self::rock_kind`] then picks without the cost of the rock-mass noise).
    pub fn fixed_kind(&self, x: i32, y: i32) -> Option<TileKind> {
        if self.above_surface(x, y) {
            return Some(TileKind::Air);
        }
        if self.biomes && biome::mountain_cave(&self.settings, x, y) {
            return Some(TileKind::Air);
        }
        if y >= self.floor_row(x) {
            return Some(TileKind::Bedrock);
        }
        if building_foundation_at(x, y) {
            return Some(TileKind::Foundation);
        }
        None
    }

    /// [`Self::base_kind`] with the band already known.
    pub fn base_kind_in(&self, x: i32, y: i32, band: Band) -> TileKind {
        self.fixed_kind(x, y)
            .unwrap_or_else(|| self.rock_kind(x, y, band))
    }

    /// The band's soft or hard rock at `(x, y)` (where [`Self::fixed_kind`] is `None`).
    pub fn rock_kind(&self, x: i32, y: i32, band: Band) -> TileKind {
        if self.biomes
            && y <= self.deepest_surface + 16
            && let Some(kind) = biome::ground_kind(&self.settings, x, y)
        {
            return kind;
        }
        let (cell_x, cell_y, contrast_per_mille) = rock_texture(band);
        let mass = fractal_noise(self.seed(), 0x0C0C_0000, x, y, cell_x, cell_y, 3);
        let (town_first, town_last) = town_columns();
        let starter_ground = band == Band::Topsoil
            && x >= town_first - TOWN_CLEAN_TOPSOIL_MARGIN
            && x <= town_last + TOWN_CLEAN_TOPSOIL_MARGIN;
        if !starter_ground && mass > contrast_threshold(contrast_per_mille) {
            band.hard_material()
        } else {
            band.soft_material()
        }
    }

    /// Whether the cavern noise opens `(x, y)` before smoothing.
    pub const fn raw_open(&self, x: i32, y: i32, band: Band) -> bool {
        if y < caves::CRUST_ROWS || y < 0 || y >= self.height {
            return false;
        }
        let (cell_x, cell_y) = caves::cavern_cells(band, true);
        let base = fractal_noise(self.seed(), caves::SALT_CAVERN, x, y, cell_x, cell_y, 3);
        let detail = value_noise(self.seed(), caves::SALT_CAVERN_DETAIL, x, y, 3, 3);
        base + (detail - 32_768) / 10 > contrast_threshold(caves::cavern_share(band))
    }

    /// Whether caves may open at `(x, y)`: below the crust (measured from the local surface;
    /// in biome worlds from the classic surface where the ground rises above it, since
    /// mountains carve their own caves), outside the town's crust and the spawn shaft. Biome
    /// worlds also keep every cave (classic caverns and cave ecosystems) clear of village
    /// ground and underground halls ([`super::villages::keeps_caves_out`]).
    pub fn cave_allowed(&self, x: i32, y: i32) -> bool {
        if !caves::cave_allowed(x, y) {
            return false;
        }
        if self.biomes && super::villages::keeps_caves_out(self, x, y) {
            return false;
        }
        let crust = caves::CRUST_ROWS - SKY_ROWS;
        if y >= self.deepest_surface + crust {
            return true;
        }
        let surface = self.surface_row(x);
        let surface = if self.biomes {
            surface.max(SKY_ROWS)
        } else {
            surface
        };
        y >= surface + crust
    }
}

/// Whether `kind` is plain rock that caves, hazards and deposits may replace.
pub const fn is_plain_rock(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt
            | TileKind::Clay
            | TileKind::Stone
            | TileKind::HardRock
            | TileKind::Surface(
                SurfaceMaterial::Sandstone | SurfaceMaterial::RedRock | SurfaceMaterial::Granite
            )
    )
}
