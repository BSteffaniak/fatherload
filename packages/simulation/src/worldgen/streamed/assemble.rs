//! Assembling one chunk of a streamed world from its layers.

use crate::{
    terrain::{
        CHUNK_SIZE_TILES, ChunkPosition, MineralKind, Tile, TileKind, TilePosition, tile_durability,
    },
    worldgen::{
        deposits::{self, mineral_fits},
        features::{HazardKind, TileRect},
        noise::{NOISE_MAX, centred, fractal_noise, hash2, ridge, value_noise},
        strata::{SKY_ROWS, scaled_row},
    },
};

use super::{
    base::{Ctx, is_plain_rock},
    regions::{
        BED_REACH, BED_REGION, CLUSTER_REACH, CLUSTER_REGION, HAZARD_REACH, HAZARD_REGION,
        MOTHERLODE_REACH, MOTHERLODE_REGION, STARTER_AREA, bed_region, cluster_region,
        hazard_region, motherlode_region, poi_regions_in, span, starter_field, stray_artifact,
    },
    window::Window,
};

/// Tiles per chunk.
#[allow(
    clippy::cast_sign_loss,
    reason = "the chunk side is a positive constant"
)]
pub const CHUNK_TILES: usize = (CHUNK_SIZE_TILES * CHUNK_SIZE_TILES) as usize;

/// The vein mineral at `(x, y)`, if a vein runs through it (the V4 vein fields).
fn vein_mineral(ctx: &Ctx, band: crate::worldgen::Band, x: i32, y: i32) -> Option<MineralKind> {
    if y < SKY_ROWS + 1 {
        return None;
    }
    let seed = ctx.seed();
    // The world's richness (V11's registry strata scale it by their ore density, Wave 8).
    let rich = ctx.ore_richness(x, y).min(220);
    let mask_threshold = NOISE_MAX * (100 - rich * 45 / 100) / 100;
    for family in 0..2_u64 {
        let (cell_x, cell_y) = if family == 0 { (26, 13) } else { (13, 22) };
        let warp_x = x + centred(value_noise(seed, deposits::SALT_VEIN_WARP, x, y, 5, 5), 3);
        let warp_y = y + centred(
            value_noise(seed, deposits::SALT_VEIN_WARP + 1, x, y, 5, 5),
            2,
        );
        let line = ridge(fractal_noise(
            seed,
            deposits::SALT_VEIN + family,
            warp_x,
            warp_y,
            cell_x,
            cell_y,
            2,
        ));
        if line < NOISE_MAX - 4_200 {
            continue;
        }
        if value_noise(seed, deposits::SALT_VEIN_MASK + family, x, y, 18, 10) < mask_threshold {
            continue;
        }
        let stretch = hash2(
            seed,
            deposits::SALT_VEIN_MINERAL + family,
            x.div_euclid(9),
            y.div_euclid(6),
        );
        if let Some(mineral) = ctx.mineral_from_hash(band, x, y, stretch) {
            return Some(mineral);
        }
    }
    None
}

pub(super) struct ChunkKinds {
    pub(super) rect: TileRect,
    top: i32,
    height: i32,
    pub(super) kinds: Vec<TileKind>,
}

impl ChunkKinds {
    pub(super) fn slot(&self, x: i32, y: i32) -> Option<usize> {
        (self.rect.contains(TilePosition { x, y }) && y >= self.top && y < self.height).then(|| {
            usize::try_from((y - self.rect.min.y) * CHUNK_SIZE_TILES + (x - self.rect.min.x))
                .unwrap_or(0)
        })
    }

    pub(super) fn set(&mut self, x: i32, y: i32, kind: TileKind) {
        if let Some(slot) = self.slot(x, y) {
            self.kinds[slot] = kind;
        }
    }

    /// Writes `kind` if `(x, y)` is inside the chunk and still plain rock.
    fn fill_rock(&mut self, x: i32, y: i32, kind: TileKind) {
        if let Some(slot) = self.slot(x, y)
            && is_plain_rock(self.kinds[slot])
        {
            self.kinds[slot] = kind;
        }
    }
}

/// The region rows (of size `size`) whose cells cover the chunk's rows grown by `reach`, and
/// the columns likewise.
fn grid(
    ctx: &Ctx,
    rect: &TileRect,
    reach: i32,
    size: (i32, i32),
) -> impl Iterator<Item = (i32, i32)> + use<> {
    let grown = rect.expanded(reach);
    let (top, bottom) = (grown.min.y.max(0), grown.max.y.min(ctx.height - 1));
    let (left, right) = (grown.min.x, grown.max.x);
    span(top, bottom, size.1).flat_map(move |ry| span(left, right, size.0).map(move |rx| (rx, ry)))
}

/// Generates every tile of `chunk` (rows below the world read as bedrock, rows above it as
/// air).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "every generation layer in its order, one block each"
)]
pub fn assemble(ctx: &Ctx, chunk: ChunkPosition) -> Vec<Tile> {
    let origin = chunk.origin();
    let rect = TileRect {
        min: origin,
        max: TilePosition {
            x: origin.x + CHUNK_SIZE_TILES - 1,
            y: origin.y + CHUNK_SIZE_TILES - 1,
        },
    };
    let mut window = Window::new(ctx, &rect);
    let sky_lands = if ctx.sky && rect.min.y < ctx.ground_top {
        crate::worldgen::sky::lands_touching(&ctx.settings, &rect)
    } else {
        Vec::new()
    };
    let mut chunk_kinds = ChunkKinds {
        rect,
        top: ctx.top,
        height: ctx.height,
        kinds: vec![TileKind::Bedrock; CHUNK_TILES],
    };
    for position in rect.positions() {
        let local = usize::try_from(
            (position.y - rect.min.y) * CHUNK_SIZE_TILES + (position.x - rect.min.x),
        )
        .unwrap_or(0);
        if position.y < ctx.top {
            // Above a biome world: open sky (V5 kept bedrock there, never read).
            if ctx.biomes {
                chunk_kinds.kinds[local] = TileKind::Air;
            }
        } else if position.y < ctx.ground_top {
            // A sky world's sky (V8): the sky lands, else open air.
            chunk_kinds.kinds[local] = sky_lands
                .iter()
                .find_map(|land| land.tile_at(position.x, position.y))
                .unwrap_or(TileKind::Air);
        } else if position.y < 0 {
            // Above row 0 (a biome world's mountains): the base layer alone, no cave layer.
            if let Some(kind) = ctx.base_kind(position.x, position.y) {
                chunk_kinds.kinds[local] = kind;
            }
        } else if position.y < ctx.height {
            chunk_kinds.kinds[local] = window.kind(position.x, position.y);
        }
    }
    if rect.max.y >= 0 && rect.min.y < ctx.height {
        // Hazard clusters (validated against the cave layer when placed).
        for (rx, ry) in grid(ctx, &rect, HAZARD_REACH, HAZARD_REGION) {
            for cluster in &hazard_region(ctx, rx, ry).clusters {
                let tile = cluster.kind.tile();
                for &(x, y) in &cluster.tiles {
                    chunk_kinds.set(x, y, tile);
                }
            }
        }
        // Points of interest.
        for region in poi_regions_in(ctx, &rect) {
            if let Some(poi) = &region.poi {
                for &(x, y, kind) in &poi.cells {
                    chunk_kinds.set(x, y, kind);
                }
            }
        }
        place_deposits(ctx, &rect, &mut window, &mut chunk_kinds);
        // The authored Crust (V11, Wave 8): roots, mud, burrows, clay pots, the old mine
        // works, slate shelves, stalactites, basalt columns, lava tubes and the cabin.
        if let Some(layout) = &ctx.descent {
            super::crust_pass::apply(ctx, layout, &mut window, &mut chunk_kinds);
        }
    }
    if rect.max.y >= ctx.top && rect.min.y < ctx.height {
        // Villages last (a biome world's mountain shelves hold villages above row 0): hazards
        // near them turn back into rock, then their foundations, paving and halls go in.
        for stamp in super::villages::stamps_touching(ctx, &rect) {
            for clear in &stamp.clear {
                if !clear.intersects(&rect) {
                    continue;
                }
                for position in rect.positions() {
                    if !clear.contains(position) {
                        continue;
                    }
                    let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                        continue;
                    };
                    let kind = chunk_kinds.kinds[slot];
                    if HazardKind::of_tile(kind).is_some() || matches!(kind, TileKind::Poi(_)) {
                        chunk_kinds.kinds[slot] =
                            window.band(position.x, position.y).soft_material();
                    }
                }
            }
            for &(x, y, kind) in &stamp.cells {
                chunk_kinds.set(x, y, kind);
            }
        }
    }
    if rect.max.y >= 0 && rect.min.y < ctx.height {
        // Pilot-only spaces (V9, WP-PILOT) go in last, over whatever the strata held.
        for space in super::pilot_spaces::spaces_touching(&ctx.settings, &rect) {
            for (tile, kind) in space.cells() {
                chunk_kinds.set(tile.x, tile.y, kind);
            }
        }
    }
    if let Some(layout) = &ctx.descent {
        // Deep Descent landmarks (V11): outposts, gate chambers and anomaly rooms.
        for landmark in
            crate::worldgen::descent::landmarks::landmarks_with(&ctx.settings, layout, &rect)
        {
            for position in rect.positions() {
                if let Some(kind) = crate::worldgen::descent::landmarks::landmark_tile(
                    &landmark, position.x, position.y,
                ) {
                    chunk_kinds.set(position.x, position.y, kind);
                }
            }
        }
        // The Living Dark (Wave 8, Descent III): its set pieces and living rock, over the
        // landmarks (the Mind's hollow grows through Descent III's gate chamber).
        let (first, last) = layout.descent_rows(crate::descent::Descent::LivingDark);
        if rect.max.y >= first - 40 && rect.min.y <= last + 40 {
            use crate::worldgen::descent::living;
            let stamps = living::stamps_with(&ctx.settings, layout, &rect);
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                let base = chunk_kinds.kinds[slot];
                if let Some(kind) = living::living_tile(
                    &ctx.settings,
                    layout,
                    &stamps,
                    position.x,
                    position.y,
                    base,
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    if let Some(layout) = &ctx.descent {
        // The Drowned and Frozen (Wave 9, Descent IV): its basins, ice, geysers, Merrow City's
        // bubble, pearl beds and the Leviathan's arena, over the landmarks (the city keeps the
        // outpost and its settlement; the cable still carves over everything).
        let (first, last) = layout.descent_rows(crate::descent::Descent::DrownedAndFrozen);
        if rect.max.y >= first - 48 && rect.min.y <= last + 48 {
            use crate::worldgen::descent::drowned;
            let stamps = drowned::stamps_with(&ctx.settings, layout, &rect);
            let city = drowned::merrow_city(&ctx.settings);
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                let base = chunk_kinds.kinds[slot];
                if let Some(kind) = drowned::drowned_tile(
                    &ctx.settings,
                    layout,
                    city.as_ref(),
                    &stamps,
                    position.x,
                    position.y,
                    base,
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    if let Some(layout) = &ctx.descent {
        // The Hollow World (Wave 9, Descent V): its jungle halls, isles, forge sky, the Breach
        // and the Sun Well, over the landmarks (the Sun Well grows round Descent V's gate
        // chamber).
        use crate::worldgen::descent::hollow;
        let (first, last) = hollow::pass_rows(layout);
        if rect.max.y >= first && rect.min.y <= last {
            let context = hollow::chunk_context(&ctx.settings, layout, &rect);
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                if let Some(kind) = hollow::hollow_tile(
                    &ctx.settings,
                    layout,
                    &context,
                    position.x,
                    position.y,
                    chunk_kinds.kinds[slot],
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    if let Some(layout) = &ctx.descent {
        // The distortion strata (Wave 9, Descent VI part A): mirror halls, shear halls, the
        // rot grid and their landmarks, over the Descent landmarks (which they keep clear of).
        use crate::worldgen::descent::distortion;
        if distortion::touches_rows(&ctx.settings, layout, &rect) {
            let plan = distortion::plan_for(&ctx.settings, layout, &rect);
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                if let Some(kind) = distortion::distortion_tile(
                    &ctx.settings,
                    layout,
                    &plan,
                    position.x,
                    position.y,
                    chunk_kinds.kinds[slot],
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    if ctx.descent.is_some() {
        // The Sinkhole (V11, Wave 8): the core and the funnel over Descent I's gate chamber.
        super::crust_pass::apply_sinkhole(ctx, &mut chunk_kinds);
    }
    if let Some(layout) = &ctx.descent {
        // The Old Deep (V11, Wave 8): Descent II's features and set pieces over the landmarks
        // (the cable and outposts still carve over them).
        let (top, bottom) = crate::old_deep::sites::carve_rows(layout);
        if rect.max.y >= top && rect.min.y <= bottom {
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                if let Some(kind) = crate::old_deep::sites::carve_tile(
                    &ctx.settings,
                    layout,
                    position.x,
                    position.y,
                    chunk_kinds.kinds[slot],
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    if let Some(layout) = &ctx.descent {
        // The far Strange Layers (V11, Wave 9 strange-b): time pockets, the Stopped Clock,
        // echo hollows and the Choir, keyed curtains and spectrum vaults, the Rewrite Hall over
        // Descent VI's gate chamber, and the alloy seal over Descent VII.
        use crate::worldgen::descent::far_strange;
        let (top, bottom) = far_strange::carve_rows(layout);
        if rect.max.y >= top && rect.min.y <= bottom {
            let stamps = far_strange::stamps_with(&ctx.settings, layout, &rect);
            for position in rect.positions() {
                let Some(slot) = chunk_kinds.slot(position.x, position.y) else {
                    continue;
                };
                if let Some(kind) = far_strange::far_tile(
                    &ctx.settings,
                    layout,
                    &stamps,
                    position.x,
                    position.y,
                    chunk_kinds.kinds[slot],
                ) {
                    chunk_kinds.kinds[slot] = kind;
                }
            }
        }
    }
    // The Fathers' expedition (V11, Wave 8 story): its log sites and the anomaly rooms, a pass
    // of their own after the landmarks and under the builders' cable.
    let story = crate::expedition::sites::story_stamps(&ctx.settings, &rect);
    for position in rect.positions() {
        if let Some(kind) = story.tile_at(position) {
            chunk_kinds.set(position.x, position.y, kind);
        }
    }
    if !ctx.logistics.lifts.is_empty() || !ctx.logistics.outposts.is_empty() {
        // Depth logistics (V11, Wave 7 integration): the outposts' floors and the builders'
        // cable of ancient lifts, over the landmarks.
        for position in rect.positions() {
            if let Some(kind) = ctx.logistics.tile_at(position) {
                chunk_kinds.set(position.x, position.y, kind);
            }
        }
    }
    if ctx.settings.generator.has_tombs() {
        // Tomb and crypt entrances (V10, WP-TOMBS) after everything, in the ground and on
        // the sky lands alike.
        for site in super::tomb_sites::sites_touching(&ctx.settings, &rect) {
            for (tile, kind) in super::tomb_sites::entrance_cells(&site) {
                chunk_kinds.set(tile.x, tile.y, kind);
            }
        }
        // The run vault: sealed bedrock the run slots are realized in.
        let vault = super::tomb_sites::VAULT;
        if vault.intersects(&rect) {
            for position in rect.positions() {
                if vault.contains(position) {
                    chunk_kinds.set(position.x, position.y, TileKind::Bedrock);
                }
            }
        }
    }
    // Fluid bodies (V11, Wave 7 integration) last, over the finished chunk: sealed aquifers
    // keep a rock rim and cave pools sit on floors, judged against everything stamped above.
    let fluids = match ctx.stratum(
        rect.min.x + CHUNK_SIZE_TILES / 2,
        rect.min.y + CHUNK_SIZE_TILES / 2,
    ) {
        Some(stratum) if !stratum.def().fluids.is_empty() => crate::fluid::fluid_pockets_in_chunk(
            ctx.settings.seed,
            chunk,
            stratum.def().fluids,
            |at| {
                chunk_kinds
                    .slot(at.x, at.y)
                    .map_or(TileKind::Bedrock, |slot| chunk_kinds.kinds[slot])
            },
        ),
        _ => Vec::new(),
    };
    let mut tiles: Vec<Tile> = chunk_kinds
        .kinds
        .iter()
        .map(|&kind| Tile {
            kind,
            durability: tile_durability(kind),
        })
        .collect();
    let city = if fluids.is_empty() {
        None
    } else {
        crate::worldgen::descent::drowned::merrow_city(&ctx.settings)
    };
    for (at, tile) in fluids {
        // The expedition's rooms stay dry (their floors are basins to the pool rule).
        if story.covers(at) {
            continue;
        }
        // So do Merrow City's streets (Wave 9): the bubble keeps the sea out.
        if city.is_some_and(|site| site.bounds().contains(at)) {
            continue;
        }
        if let Some(slot) = chunk_kinds.slot(at.x, at.y) {
            tiles[slot] = tile;
        }
    }
    tiles
}

/// Ore and artifacts, in the V4 order: motherlodes, clusters, veins, the starter field, artifact
/// beds and strays, each only where plain rock is left.
fn place_deposits(ctx: &Ctx, rect: &TileRect, window: &mut Window<'_>, chunk: &mut ChunkKinds) {
    for (rx, ry) in grid(ctx, rect, MOTHERLODE_REACH, MOTHERLODE_REGION) {
        for &(x, y, mineral) in &motherlode_region(ctx, rx, ry).tiles {
            chunk.fill_rock(x, y, TileKind::Ore(mineral));
        }
    }
    for (rx, ry) in grid(ctx, rect, CLUSTER_REACH, CLUSTER_REGION) {
        for &(x, y, mineral) in &cluster_region(ctx, rx, ry).tiles {
            chunk.fill_rock(x, y, TileKind::Ore(mineral));
        }
    }
    for position in rect.positions() {
        let Some(slot) = chunk.slot(position.x, position.y) else {
            continue;
        };
        if !is_plain_rock(chunk.kinds[slot]) {
            continue;
        }
        let band = window.band(position.x, position.y);
        if let Some(mineral) = vein_mineral(ctx, band, position.x, position.y)
            && mineral_fits(band, mineral)
        {
            chunk.kinds[slot] = TileKind::Ore(mineral);
        }
    }
    if rect.intersects(&STARTER_AREA) {
        for &(x, y, mineral) in &starter_field(ctx).tiles {
            chunk.fill_rock(x, y, TileKind::Ore(mineral));
        }
    }
    for (rx, ry) in grid(ctx, rect, BED_REACH, BED_REGION) {
        for &(x, y, artifact) in &bed_region(ctx, rx, ry).tiles {
            chunk.fill_rock(x, y, TileKind::Artifact(artifact));
        }
    }
    let first_stray_row = scaled_row(&ctx.settings, 20);
    for position in rect.positions() {
        if position.y < first_stray_row {
            continue;
        }
        let Some(slot) = chunk.slot(position.x, position.y) else {
            continue;
        };
        if !is_plain_rock(chunk.kinds[slot]) {
            continue;
        }
        let band = window.band(position.x, position.y);
        if let Some(artifact) = stray_artifact(ctx, band, position.x, position.y) {
            chunk.kinds[slot] = TileKind::Artifact(artifact);
        }
    }
}
