//! The authored Crust's chunk pass (V11, Wave 8): writes the Crust's features
//! ([`crate::worldgen::descent::crust`]) into a chunk after its deposits, and the Sinkhole after
//! the landmarks.

use crate::{
    descent::{Descent, Stratum},
    terrain::{CHUNK_SIZE_TILES, TileKind},
    worldgen::descent::{self as layout_rules, DescentLayout, crust},
};

use super::{assemble::ChunkKinds, base::Ctx, window::Window};

/// Rows of the Crust's last stratum past its layout bottom that the pass still reaches (the
/// ragged boundary into Descent II).
const BOUNDARY_SLACK: i32 = 40;

/// Whether `kind` may be overwritten by the Crust's features (plain rock, open ground, ore,
/// hazards and the Crust's own tiles; never points of interest, cave decor or villages).
const fn writable(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Air
            | TileKind::Dirt
            | TileKind::Clay
            | TileKind::Stone
            | TileKind::HardRock
            | TileKind::Ore(_)
            | TileKind::Gas
            | TileKind::ExplosivePocket
            | TileKind::PressurePocket
            | TileKind::MagmaVent
            | TileKind::Lava
            | TileKind::Surface(_)
            | TileKind::Crust(_)
    )
}

/// Writes the Crust's features into `chunk` (a chunk of a V11 world).
pub(super) fn apply(
    ctx: &Ctx,
    layout: &DescentLayout,
    window: &mut Window<'_>,
    chunk: &mut ChunkKinds,
) {
    let rect = chunk.rect;
    let crust_end = layout.rows(Descent::Crust.last_stratum()).1 + BOUNDARY_SLACK;
    if rect.min.y > crust_end || rect.max.y < layout.tops[0] {
        return;
    }
    let settings = &ctx.settings;
    let works = crust::works_touching(settings, layout, &rect);
    let cabin = crate::crust::cabin(settings);
    for y in rect.min.y..=rect.max.y {
        for x in rect.min.x..=rect.max.x {
            let Some(slot) = chunk.slot(x, y) else {
                continue;
            };
            let current = chunk.kinds[slot];
            if !writable(current) {
                continue;
            }
            let stratum = layout_rules::stratum_in(settings, layout, x, y);
            if stratum.descent() != Descent::Crust {
                continue;
            }
            // The old mine works and the cabin first (whole structures).
            if let Some(kind) = works
                .iter()
                .find_map(|works| works.tile(x, y))
                .filter(|_| !window.near_poi(x, y, 1))
            {
                chunk.kinds[slot] = kind;
                continue;
            }
            if let Some(cabin) = &cabin
                && let Some(kind) = crust::cabin_tile(cabin, ctx.surface_row(x), x, y)
            {
                chunk.kinds[slot] = kind;
                continue;
            }
            let plain = super::base::is_plain_rock(current);
            let open = current.is_open();
            let needs_neighbours = matches!(stratum, Stratum::Topsoil | Stratum::SlateShelves);
            let (open_above, solid_above) = if needs_neighbours && (plain || open) {
                let above = if y > rect.min.y {
                    chunk.kinds[slot - usize::try_from(CHUNK_SIZE_TILES).unwrap_or(16)]
                } else {
                    window.kind(x, y - 1)
                };
                (
                    above == TileKind::Air,
                    super::base::is_plain_rock(above)
                        || matches!(above, TileKind::Crust(crate::crust::CrustTile::Slate)),
                )
            } else {
                (false, false)
            };
            let surface = if stratum == Stratum::Topsoil {
                ctx.surface_row(x)
            } else {
                0
            };
            if stratum == Stratum::Topsoil && !ctx.cave_allowed(x, y) {
                // The town's crust and the spawn shaft stay clean.
                continue;
            }
            if let Some(kind) = crust::base_tile(
                settings,
                layout,
                stratum,
                x,
                y,
                surface,
                plain,
                open,
                open_above,
                solid_above,
            ) {
                chunk.kinds[slot] = kind;
            }
        }
    }
}

/// Writes the Sinkhole's funnel, core and strongbox over the stamped gate chamber.
pub(super) fn apply_sinkhole(ctx: &Ctx, chunk: &mut ChunkKinds) {
    let rect = chunk.rect;
    let Some(site) = crust::sinkhole_touching(&ctx.settings, &rect) else {
        return;
    };
    for y in rect.min.y..=rect.max.y {
        for x in rect.min.x..=rect.max.x {
            if let Some(kind) = crust::sinkhole_tile(&site, x, y)
                && chunk.slot(x, y).is_some()
            {
                chunk.set(x, y, kind);
            }
        }
    }
}
