//! The point-evaluable base layer of a streamed world: the surface heightmap, the bedrock
//! floor, the town's foundations, each band's rock mass and the raw cavern noise.
//!
//! Everything here is a pure function of the settings and the tile coordinates, so any tile
//! can be evaluated on its own, in any order, by any chunk.

use crate::{
    surface::building_foundation_at,
    terrain::TileKind,
    worldgen::{
        TOWN_CLEAN_TOPSOIL_MARGIN, WorldSettings, caves, contrast_threshold,
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

/// The first ground row of column `x` of a streamed world (the surface heightmap hook).
///
/// Flat at [`SKY_ROWS`] across the town ([`TOWN_FLAT_FIRST_COLUMN`] to
/// [`TOWN_FLAT_LAST_COLUMN`]); elsewhere a gentle placeholder swell up to
/// [`MAX_SURFACE_DIP_ROWS`] lower. Surface biomes (wave 2) replace the swell; the rest of the
/// generator only reads this function, so the town and spawn stay fixed whatever it returns.
#[must_use]
pub const fn surface_row(settings: &WorldSettings, x: i32) -> i32 {
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

    pub const fn surface_row(&self, x: i32) -> i32 {
        surface_row(&self.settings, x)
    }

    /// The base tile at `(x, y)`: sky, bedrock, foundations or the band's rock mass (no caves,
    /// hazards, points of interest or deposits yet). `None` outside the world's rows.
    pub fn base_kind(&self, x: i32, y: i32) -> Option<TileKind> {
        if y < 0 || y >= self.height {
            return None;
        }
        Some(self.base_kind_in(x, y, self.band(x, y)))
    }

    /// The sky, bedrock or foundation tile at `(x, y)`, or `None` where the base is the band's
    /// rock (which [`Self::rock_kind`] then picks without the cost of the rock-mass noise).
    pub fn fixed_kind(&self, x: i32, y: i32) -> Option<TileKind> {
        if y < self.surface_row(x) {
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

    /// Whether caves may open at `(x, y)`: below the crust (measured from the local surface),
    /// outside the town's crust and the spawn shaft.
    pub const fn cave_allowed(&self, x: i32, y: i32) -> bool {
        y >= self.surface_row(x) + (caves::CRUST_ROWS - SKY_ROWS) && caves::cave_allowed(x, y)
    }
}

/// Whether `kind` is plain rock that caves, hazards and deposits may replace.
pub const fn is_plain_rock(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock
    )
}
