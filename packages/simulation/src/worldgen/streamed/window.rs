//! A lazily evaluated rectangle of the cave layer: base rock with caverns, worm tunnels and
//! chambers carved out (no hazards, points of interest or deposits).
//!
//! A window's answers are a pure function of the world and its rectangle: it gathers every
//! worm, chamber and point of interest that can reach the rectangle from the region caches, so
//! two windows (of any two chunks or regions) agree on every tile they share.

use crate::{terrain::TileKind, worldgen::features::TileRect, worldgen::strata::Band};

use std::sync::Arc;

use super::{
    base::Ctx,
    cave_biomes::{BiomeCell, CaveRegion, caves_in},
    regions::{
        self, CHAMBER_REACH, Chamber, WORM_REACH, chamber_region, chamber_tile, poi_regions_in,
        worm_region,
    },
};

/// Margin around a point of interest's bounds in which no cave opens.
pub const POI_CAVE_SHIELD: i32 = 1;

pub struct Window<'c> {
    ctx: &'c Ctx,
    rect: TileRect,
    width: i32,
    /// Cached [`Self::class`] (0 unknown).
    classes: Vec<u8>,
    /// Raw cavern noise: 0 unknown, 1 closed, 2 open.
    raw: Vec<u8>,
    /// Cached band index + 1 (0 unknown).
    bands: Vec<u8>,
    /// Tiles carved by worm tunnels or chambers.
    carved: Vec<bool>,
    /// Bounds of every point of interest that reaches the window.
    pois: Vec<TileRect>,
    /// Every cave ecosystem whose footprint overlaps the window.
    caves: Vec<Arc<CaveRegion>>,
}

impl<'c> Window<'c> {
    /// A window over `rect` (rows clamped to the world).
    #[must_use]
    pub fn new(ctx: &'c Ctx, rect: &TileRect) -> Self {
        let rect = TileRect {
            min: crate::terrain::TilePosition {
                x: rect.min.x,
                y: rect.min.y.max(0),
            },
            max: crate::terrain::TilePosition {
                x: rect.max.x,
                y: rect.max.y.min(ctx.height - 1),
            },
        };
        let width = (rect.max.x - rect.min.x + 1).max(0);
        let height = (rect.max.y - rect.min.y + 1).max(0);
        let cells = usize::try_from(width * height).unwrap_or(0);
        let mut window = Self {
            ctx,
            rect,
            width,
            classes: vec![0; cells],
            raw: vec![0; cells],
            bands: vec![0; cells],
            carved: vec![false; cells],
            pois: Vec::new(),
            caves: Vec::new(),
        };
        if cells == 0 {
            return window;
        }
        let reach = rect.expanded(WORM_REACH);
        for ry in regions::span(reach.min.y, reach.max.y, regions::WORM_REGION.1) {
            for rx in regions::span(reach.min.x, reach.max.x, regions::WORM_REGION.0) {
                let worms = worm_region(ctx, rx, ry);
                if !worms.bounds.is_some_and(|bounds| bounds.intersects(&rect)) {
                    continue;
                }
                for &(cx, cy, radius) in &worms.discs {
                    if cx + radius < rect.min.x
                        || cx - radius > rect.max.x
                        || cy + radius < rect.min.y
                        || cy - radius > rect.max.y
                    {
                        continue;
                    }
                    window.carve_disc(cx, cy, radius);
                }
            }
        }
        let reach = rect.expanded(CHAMBER_REACH);
        for ry in regions::span(reach.min.y, reach.max.y, regions::CHAMBER_REGION.1) {
            for rx in regions::span(reach.min.x, reach.max.x, regions::CHAMBER_REGION.0) {
                if let Some(chamber) = chamber_region(ctx, rx, ry).chamber {
                    window.carve_chamber(&chamber);
                }
            }
        }
        window.pois = poi_regions_in(ctx, &rect.expanded(POI_CAVE_SHIELD + 4))
            .iter()
            .filter_map(|region| region.poi.as_ref().map(|poi| poi.feature.bounds))
            .collect();
        window.caves = caves_in(ctx, &rect);
        window
    }

    /// The cave-ecosystem cell at `(x, y)` (`Outside` beyond every cave).
    fn cave_cell(&self, x: i32, y: i32) -> BiomeCell {
        let position = crate::terrain::TilePosition { x, y };
        self.caves
            .iter()
            .filter_map(|region| region.biome.as_ref())
            .find(|biome| biome.bounds().contains(position))
            .map_or(BiomeCell::Outside, |biome| biome.cell(x, y))
    }

    const fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < self.rect.min.x || x > self.rect.max.x || y < self.rect.min.y || y > self.rect.max.y
        {
            return None;
        }
        #[allow(clippy::cast_sign_loss, reason = "the offset is inside the window")]
        Some(((y - self.rect.min.y) * self.width + (x - self.rect.min.x)) as usize)
    }

    fn carve_disc(&mut self, cx: i32, cy: i32, radius: i32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius + radius / 2
                    && let Some(index) = self.index(cx + dx, cy + dy)
                {
                    self.carved[index] = true;
                }
            }
        }
    }

    fn carve_chamber(&mut self, chamber: &Chamber) {
        let (hw, hh) = (chamber.half_width, chamber.half_height);
        for dy in -hh - 1..=hh + 1 {
            for dx in -hw - 1..=hw + 1 {
                let (x, y) = (chamber.x + dx, chamber.y + dy);
                if let Some(index) = self.index(x, y)
                    && chamber_tile(self.ctx, chamber, x, y)
                {
                    self.carved[index] = true;
                }
            }
        }
    }

    /// Whether `(x, y)` lies within `margin` tiles of a point of interest.
    pub fn near_poi(&self, x: i32, y: i32, margin: i32) -> bool {
        let position = crate::terrain::TilePosition { x, y };
        self.pois
            .iter()
            .any(|bounds| bounds.expanded(margin).contains(position))
    }

    pub fn band(&mut self, x: i32, y: i32) -> Band {
        let Some(index) = self.index(x, y) else {
            return self.ctx.band(x, y);
        };
        if self.bands[index] == 0 {
            #[allow(clippy::cast_possible_truncation, reason = "there are twelve bands")]
            let code = self.ctx.band(x, y).index() as u8 + 1;
            self.bands[index] = code;
        }
        Band::from_index(usize::from(self.bands[index] - 1))
    }

    fn raw(&mut self, x: i32, y: i32) -> bool {
        let Some(index) = self.index(x, y) else {
            let band = self.ctx.band(x, y);
            return self.ctx.raw_open(x, y, band);
        };
        if self.raw[index] == 0 {
            let band = self.band(x, y);
            self.raw[index] = if self.ctx.raw_open(x, y, band) { 2 } else { 1 };
        }
        self.raw[index] == 2
    }

    /// The cavern mask after one smoothing pass: a cell opens with 5+ open neighbours and stays
    /// open with 4+.
    fn cavern_open(&mut self, x: i32, y: i32) -> bool {
        let mut neighbours = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) != (0, 0) && self.raw(x + dx, y + dy) {
                    neighbours += 1;
                }
            }
        }
        if self.raw(x, y) {
            neighbours >= 4
        } else {
            neighbours >= 5
        }
    }

    /// Whether `(x, y)` is open (sky or cave), solid rock, or a fixed tile: 1 fixed or outside,
    /// 2 open, 3 plain rock, 4 a cave ecosystem's decor (see [`BiomeCell`]). Cheaper than
    /// [`Self::kind`] (no rock-mass noise).
    fn class(&mut self, x: i32, y: i32) -> u8 {
        let Some(index) = self.index(x, y) else {
            return 1;
        };
        if self.classes[index] != 0 {
            return self.classes[index];
        }
        let class = match self.ctx.fixed_kind(x, y) {
            Some(TileKind::Air) => 2,
            Some(_) => 1,
            None => {
                let cave = self.cave_cell(x, y);
                match cave {
                    BiomeCell::Open | BiomeCell::Water => 2,
                    BiomeCell::Solid(_) => 4,
                    BiomeCell::Outside | BiomeCell::Wall | BiomeCell::Shell(_) => {
                        if self.ctx.cave_allowed(x, y)
                            && !self.near_poi(x, y, POI_CAVE_SHIELD)
                            && (self.carved[index] || self.cavern_open(x, y))
                        {
                            2
                        } else if matches!(cave, BiomeCell::Shell(_)) {
                            4
                        } else {
                            3
                        }
                    }
                }
            }
        };
        self.classes[index] = class;
        class
    }

    /// A quick reading of whether `(x, y)` is open: the cave layer before smoothing (one noise
    /// sample instead of nine). Large-cavern detection traces areas with it.
    pub fn is_roughly_open(&mut self, x: i32, y: i32) -> bool {
        let Some(index) = self.index(x, y) else {
            return false;
        };
        if self.classes[index] != 0 {
            return self.classes[index] == 2;
        }
        match self.ctx.fixed_kind(x, y) {
            Some(TileKind::Air) => true,
            Some(_) => false,
            None => match self.cave_cell(x, y) {
                BiomeCell::Open | BiomeCell::Water => true,
                BiomeCell::Solid(_) => false,
                BiomeCell::Outside | BiomeCell::Wall | BiomeCell::Shell(_) => {
                    self.ctx.cave_allowed(x, y)
                        && !self.near_poi(x, y, POI_CAVE_SHIELD)
                        && (self.carved[index] || self.raw(x, y))
                }
            },
        }
    }

    /// The cave-layer tile at `(x, y)`. Outside the window (or the world) it reads as bedrock,
    /// which nothing replaces.
    pub fn kind(&mut self, x: i32, y: i32) -> TileKind {
        match self.class(x, y) {
            2 => TileKind::Air,
            3 => {
                let band = self.band(x, y);
                self.ctx.rock_kind(x, y, band)
            }
            4 => match self.cave_cell(x, y) {
                BiomeCell::Solid(kind) | BiomeCell::Shell(kind) => kind,
                _ => {
                    let band = self.band(x, y);
                    self.ctx.rock_kind(x, y, band)
                }
            },
            _ => self
                .index(x, y)
                .and_then(|_| self.ctx.fixed_kind(x, y))
                .unwrap_or(TileKind::Bedrock),
        }
    }

    pub fn is_plain_rock(&mut self, x: i32, y: i32) -> bool {
        self.class(x, y) == 3
    }
}
