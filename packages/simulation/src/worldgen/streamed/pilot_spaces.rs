//! Pilot-only spaces of [`GeneratorVersion::V9`] worlds (expansion plan WP-PILOT).
//!
//! Sealed ruins with lever gates, pressure plates, keycard doors and chests, a rig vault whose
//! gate only a pilot can open, and crawlway caves.
//!
//! Placement is regional like every other streamed feature: each [`REGION`] cell of the ground
//! layer may hold one space at a position rolled from the world seed, kept inside the cell (so a
//! chunk only consults the cells it overlaps), below the surface, off the town's columns and
//! clear of villages and underground halls. Every space is a fixed hand-authored layout
//! ([`PilotSpaceKind::layout`]) stamped last into the chunk, possibly mirrored; cells marked
//! `#` keep the generated rock, so a space sits in the strata like a fossil.
//!
//! Entrances are vent grates (rigs, drops and creatures stop at them; pilots crawl through) set
//! in unbreakable walls (ruin masonry, or bedrock hardpan around crawlway caves): a rig digs up
//! to the grate, its pilot climbs out and goes in on foot. The rig vault's chamber is big enough
//! for the rig, but its gate opens only from a lever up a crawlway: hop out, pull the lever,
//! fly through the gate.
//!
//! The mechanisms' rules are in [`crate::mechanism`]; access tags in
//! [`PilotSpaceKind::access`] ([`SpaceAccess`]).
//!
//! [`GeneratorVersion::V9`]: crate::worldgen::GeneratorVersion::V9

use crate::{
    base::BuiltTile,
    poi::SpaceAccess,
    terrain::{MineralKind, TileKind, TilePosition},
    worldgen::{WorldSettings, features::TileRect, noise::hash2},
};

use super::{TOWN_FLAT_FIRST_COLUMN, TOWN_FLAT_LAST_COLUMN, base::Ctx, cache::MemoCache};

/// Each placement cell's space, memoized per process (chunks ask for the same cells again and
/// again; a miss only costs time).
static SPACES: MemoCache<Option<PilotSpace>> = MemoCache::new(1_024);

/// Size of a placement cell (columns, rows).
pub const REGION: (i32, i32) = (176, 112);
/// Percent of cells that hold a space.
pub const SPACE_PERCENT: u64 = 55;
/// Spaces start at least this many rows below the deepest surface row.
pub const MIN_DEPTH_ROWS: i32 = 26;
/// Spaces stay this many rows above the world's floor.
pub const FLOOR_MARGIN_ROWS: i32 = 24;
/// Columns either side of the town kept free of spaces.
pub const TOWN_MARGIN_COLUMNS: i32 = 48;
/// Tiles kept between a space and a village or hall.
pub const VILLAGE_MARGIN: i32 = 10;

const SALT_SPACE: u64 = 0x9170_75AC_E000;

/// What a pilot space is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PilotSpaceKind {
    /// Three sealed rooms: a lever gate, a pressure-plate gate and a keycard vault (the key is
    /// in the middle room's chest). Pilot only.
    Ruin,
    /// A rig-sized ore vault behind a gate whose lever sits up a crawlway: pilot and rig.
    RigVault,
    /// A pocket cave in bedrock hardpan, reached only through a crawlway. Pilot only.
    CrawlwayCave,
}

/// The layout legend: `#` keeps generated rock, `.` air, `R` ruin masonry, `B` bedrock hardpan,
/// `V` vent grate, `G` gate bars, `0`/`1` gate lintels of channel 0/1, `L` lever (channel 0),
/// `P` pressure plate (channel 1), `K` keycard door, `C` chest holding the keycard, `T` chest,
/// `o` ore for the depth.
const RUIN: [&str; 7] = [
    "RRRRRRRRRRRRRRRRRRRRRRRRRRRRR",
    "R.......RR.......RR.....RR..R",
    "R.......0R.......1R.....RR..R",
    "R.......G........G......K...R",
    "L.......G........G......K...R",
    "V.......G......C.G......K.T.R",
    "RRRRRRRRRRRRPRRRRRRRRRRRRRRRR",
];

const RIG_VAULT: [&str; 8] = [
    "RRRRRRRRRRRRRRRR",
    "R..RRRRRRRRRRRRR",
    "V..LRRRRRRRRRRRR",
    "0RRRRRRRRRRRRRRR",
    "G.....oooo.....R",
    "G....oooooo....R",
    "G...oooooooo.T.R",
    "RRRRRRRRRRRRRRRR",
];

const CRAWLWAY_CAVE: [&str; 6] = [
    "#######BBBBBBBBBB####",
    "######BB.........BB##",
    "BBBBBBB...........B##",
    "V.....V......o....B##",
    "BBBBBBB.....ooo.T.B##",
    "######BBBBBBBBBBBBB##",
];

impl PilotSpaceKind {
    pub const ALL: [Self; 3] = [Self::Ruin, Self::RigVault, Self::CrawlwayCave];

    /// The hand-authored layout (unmirrored, the entrance on the left).
    #[must_use]
    pub const fn layout(self) -> &'static [&'static str] {
        match self {
            Self::Ruin => &RUIN,
            Self::RigVault => &RIG_VAULT,
            Self::CrawlwayCave => &CRAWLWAY_CAVE,
        }
    }

    /// Width and height in tiles.
    #[must_use]
    pub fn size(self) -> (i32, i32) {
        let layout = self.layout();
        let width = layout.iter().map(|row| row.len()).max().unwrap_or(0);
        (
            i32::try_from(width).unwrap_or(0),
            i32::try_from(layout.len()).unwrap_or(0),
        )
    }

    /// Who gets in: ruins and crawlway caves are pilot-only; the rig vault needs both.
    #[must_use]
    pub const fn access(self) -> SpaceAccess {
        match self {
            Self::Ruin | Self::CrawlwayCave => SpaceAccess::RequiresPilot,
            Self::RigVault => SpaceAccess::Both,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ruin => "Sealed Ruin",
            Self::RigVault => "Rig Vault",
            Self::CrawlwayCave => "Crawlway Cave",
        }
    }
}

/// One placed pilot space.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PilotSpace {
    pub kind: PilotSpaceKind,
    /// The top-left tile of its layout box.
    pub origin: TilePosition,
    /// Mirrored left to right (the entrance on the right).
    pub mirrored: bool,
    /// The keycard colour of a ruin's vault.
    pub card: u8,
}

impl PilotSpace {
    /// The tiles its layout covers.
    #[must_use]
    pub fn bounds(&self) -> TileRect {
        let (width, height) = self.kind.size();
        TileRect {
            min: self.origin,
            max: TilePosition {
                x: self.origin.x + width - 1,
                y: self.origin.y + height - 1,
            },
        }
    }

    /// The vent grate it is entered by (the outside face of the layout).
    #[must_use]
    pub fn entrance(&self) -> TilePosition {
        self.cells()
            .into_iter()
            .find(|(_, kind)| *kind == TileKind::Built(BuiltTile::VentGrate))
            .map_or(self.origin, |(tile, _)| tile)
    }

    /// Every tile it writes (`#` cells are left out), in row order.
    #[must_use]
    pub fn cells(&self) -> Vec<(TilePosition, TileKind)> {
        let (width, _) = self.kind.size();
        let ore = TileKind::Ore(vault_ore(self.origin.y));
        let mut cells = Vec::new();
        for (row, line) in (0..).zip(self.kind.layout()) {
            for (column, cell) in (0..).zip(line.bytes()) {
                let x = if self.mirrored {
                    self.origin.x + width - 1 - column
                } else {
                    self.origin.x + column
                };
                let kind = match cell {
                    b'.' => TileKind::Air,
                    b'R' => TileKind::Built(BuiltTile::RuinWall),
                    b'B' => TileKind::Bedrock,
                    b'V' => TileKind::Built(BuiltTile::VentGrate),
                    b'G' => TileKind::Built(BuiltTile::Gate),
                    b'0' => TileKind::Built(BuiltTile::GateLintel(0)),
                    b'1' => TileKind::Built(BuiltTile::GateLintel(1)),
                    b'L' => TileKind::Built(BuiltTile::Lever {
                        channel: 0,
                        on: false,
                    }),
                    b'P' => TileKind::Built(BuiltTile::PressurePlate {
                        channel: 1,
                        down: false,
                    }),
                    b'K' => TileKind::Built(BuiltTile::KeycardDoor(self.card)),
                    b'C' => TileKind::Built(BuiltTile::Chest {
                        open: false,
                        card: Some(self.card),
                    }),
                    b'T' => TileKind::Built(BuiltTile::Chest {
                        open: false,
                        card: None,
                    }),
                    b'o' => ore,
                    _ => continue,
                };
                cells.push((
                    TilePosition {
                        x,
                        y: self.origin.y + row,
                    },
                    kind,
                ));
            }
        }
        cells
    }
}

/// The ore a vault at `row` holds: richer the deeper it lies.
#[must_use]
pub const fn vault_ore(row: i32) -> MineralKind {
    match row / 200 {
        i32::MIN..=0 => MineralKind::Silver,
        1 => MineralKind::Gold,
        2 => MineralKind::Platinum,
        3 => MineralKind::Emerald,
        4 => MineralKind::Ruby,
        _ => MineralKind::Diamond,
    }
}

/// The space placed in cell `(cx, cy)`, if any (pure: seed, settings and the cell).
fn space_in_cell(ctx: &Ctx, cx: i32, cy: i32) -> Option<PilotSpace> {
    let seed = ctx.settings.seed;
    let roll = hash2(seed, SALT_SPACE, cx, cy);
    if roll % 100 >= SPACE_PERCENT {
        return None;
    }
    let kind = match (roll >> 8) % 10 {
        0..=3 => PilotSpaceKind::Ruin,
        4..=6 => PilotSpaceKind::CrawlwayCave,
        _ => PilotSpaceKind::RigVault,
    };
    let (width, height) = kind.size();
    let (left, top) = (cx * REGION.0, cy * REGION.1);
    let first_row = top.max(ctx.deepest_surface + MIN_DEPTH_ROWS);
    let last_row = (top + REGION.1 - 1).min(ctx.height - FLOOR_MARGIN_ROWS) - height;
    if last_row < first_row {
        return None;
    }
    let span_x = u64::try_from(REGION.0 - width - 4).ok()?.max(1);
    let span_y = u64::try_from(last_row - first_row + 1).ok()?.max(1);
    let x = left + 2 + i32::try_from((roll >> 16) % span_x).ok()?;
    let y = first_row + i32::try_from((roll >> 32) % span_y).ok()?;
    let space = PilotSpace {
        kind,
        origin: TilePosition { x, y },
        mirrored: (roll >> 40) & 1 == 1,
        card: u8::try_from((roll >> 44) % u64::from(crate::mechanism::KEYCARD_COLORS)).ok()?,
    };
    let bounds = space.bounds();
    if bounds.max.x >= TOWN_FLAT_FIRST_COLUMN - TOWN_MARGIN_COLUMNS
        && bounds.min.x <= TOWN_FLAT_LAST_COLUMN + TOWN_MARGIN_COLUMNS
    {
        return None;
    }
    let corners = [
        bounds.min,
        bounds.max,
        TilePosition {
            x: bounds.min.x,
            y: bounds.max.y,
        },
        TilePosition {
            x: bounds.max.x,
            y: bounds.min.y,
        },
        TilePosition {
            x: bounds.min.x.midpoint(bounds.max.x),
            y: bounds.min.y.midpoint(bounds.max.y),
        },
    ];
    if corners
        .iter()
        .any(|corner| super::in_village_area(&ctx.settings, *corner, VILLAGE_MARGIN))
    {
        return None;
    }
    Some(space)
}

/// Every pilot space whose box touches `rect` (none before V9), in cell order.
#[must_use]
pub fn spaces_touching(settings: &WorldSettings, rect: &TileRect) -> Vec<PilotSpace> {
    if !settings.generator.has_pilot_spaces() {
        return Vec::new();
    }
    let ctx = super::ctx(settings);
    let mut found = Vec::new();
    for cy in rect.min.y.div_euclid(REGION.1)..=rect.max.y.div_euclid(REGION.1) {
        if cy < 0 {
            continue;
        }
        for cx in rect.min.x.div_euclid(REGION.0)..=rect.max.x.div_euclid(REGION.0) {
            let space = SPACES.get_or_compute((*settings, cx, cy), || space_in_cell(&ctx, cx, cy));
            if let Some(space) = *space
                && space.bounds().intersects(rect)
            {
                found.push(space);
            }
        }
    }
    found
}

/// The nearest pilot space to `from` (by its entrance) within `cells` placement cells, if any:
/// quests and rumours bind to it.
#[must_use]
pub fn nearest_space(
    settings: &WorldSettings,
    from: TilePosition,
    cells: i32,
) -> Option<PilotSpace> {
    let reach = TileRect {
        min: TilePosition {
            x: from.x - cells * REGION.0,
            y: (from.y - cells * REGION.1).max(0),
        },
        max: TilePosition {
            x: from.x + cells * REGION.0,
            y: from.y + cells * REGION.1,
        },
    };
    spaces_touching(settings, &reach)
        .into_iter()
        .min_by_key(|space| {
            let entrance = space.entrance();
            let (dx, dy) = (
                i64::from(entrance.x - from.x),
                i64::from(entrance.y - from.y),
            );
            (dx * dx + dy * dy, *space)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::GeneratorVersion;

    fn v9() -> WorldSettings {
        WorldSettings {
            generator: GeneratorVersion::V9,
            ..WorldSettings::default()
        }
    }

    #[test]
    fn layouts_are_rectangular_and_mechanisms_reach_their_gates() {
        for kind in PilotSpaceKind::ALL {
            let (width, _) = kind.size();
            for row in kind.layout() {
                assert!(
                    i32::try_from(row.len()).unwrap_or(0) <= width,
                    "{kind:?} row too wide"
                );
            }
            let space = PilotSpace {
                kind,
                origin: TilePosition { x: 1_000, y: 300 },
                mirrored: false,
                card: 2,
            };
            let cells = space.cells();
            let grates = cells
                .iter()
                .filter(|(_, kind)| *kind == TileKind::Built(BuiltTile::VentGrate))
                .count();
            assert!(grates >= 1, "{kind:?} needs a crawl entrance");
            // Every mechanism reaches a lintel of its channel.
            for (tile, cell) in &cells {
                let channel = match cell {
                    TileKind::Built(
                        BuiltTile::Lever { channel, .. } | BuiltTile::PressurePlate { channel, .. },
                    ) => *channel,
                    _ => continue,
                };
                assert!(
                    cells.iter().any(|(lintel, kind)| {
                        *kind == TileKind::Built(BuiltTile::GateLintel(channel))
                            && crate::mechanism::in_reach(*tile, *lintel)
                    }),
                    "{kind:?} mechanism at {tile:?} reaches no gate"
                );
            }
            // Mirroring keeps the cell count and flips the entrance side.
            let mirrored = PilotSpace {
                mirrored: true,
                ..space
            };
            assert_eq!(mirrored.cells().len(), cells.len());
            assert_ne!(mirrored.entrance().x, space.entrance().x);
        }
        assert_eq!(PilotSpaceKind::Ruin.access(), SpaceAccess::RequiresPilot);
        assert_eq!(PilotSpaceKind::RigVault.access(), SpaceAccess::Both);
    }

    #[test]
    fn spaces_are_placed_in_v9_only_underground_and_away_from_town() {
        let rect = TileRect {
            min: TilePosition { x: -4_000, y: 0 },
            max: TilePosition { x: 4_000, y: 1_400 },
        };
        let spaces = spaces_touching(&v9(), &rect);
        assert!(spaces.len() >= 20, "only {} spaces", spaces.len());
        for kind in PilotSpaceKind::ALL {
            assert!(
                spaces.iter().any(|space| space.kind == kind),
                "no {kind:?} placed"
            );
        }
        let ctx = super::super::ctx(&v9());
        for space in &spaces {
            let bounds = space.bounds();
            assert!(bounds.min.y >= ctx.deepest_surface + MIN_DEPTH_ROWS);
            assert!(bounds.max.y < ctx.height);
            assert!(
                bounds.max.x < TOWN_FLAT_FIRST_COLUMN - TOWN_MARGIN_COLUMNS
                    || bounds.min.x > TOWN_FLAT_LAST_COLUMN + TOWN_MARGIN_COLUMNS
            );
        }
        let v8 = WorldSettings {
            generator: GeneratorVersion::V8,
            ..WorldSettings::default()
        };
        assert!(spaces_touching(&v8, &rect).is_empty());
        let near = nearest_space(&v9(), TilePosition { x: 600, y: 200 }, 3).expect("a space");
        assert!(spaces.contains(&near) || near.origin.y > 1_400);
    }

    /// V9 generation is pinned: the chunks around the first few pilot spaces near town and a
    /// plain ground chunk. Change it only with a new generator version.
    #[test]
    fn v9_generation_is_pinned() {
        let settings = v9();
        let rect = TileRect {
            min: TilePosition { x: -2_000, y: 0 },
            max: TilePosition { x: 2_000, y: 900 },
        };
        let mut chunks: Vec<crate::terrain::ChunkPosition> = spaces_touching(&settings, &rect)
            .into_iter()
            .take(6)
            .map(|space| crate::terrain::ChunkPosition::of(space.entrance()))
            .collect();
        chunks.push(crate::terrain::ChunkPosition { x: 3, y: 2 });
        let mut combined = 0_u64;
        for chunk in chunks {
            for (index, tile) in super::super::generated_chunk(&settings, chunk)
                .tiles()
                .iter()
                .enumerate()
            {
                combined = combined
                    .rotate_left(7)
                    .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
            }
        }
        assert_eq!(
            combined, GOLDEN_V9_DIGEST,
            "V9 generation changed: {combined:#018x}"
        );
    }

    const GOLDEN_V9_DIGEST: u64 = 0x6c3b_090c_955a_b9f9;

    #[test]
    fn a_space_reads_the_same_from_every_chunk_it_touches() {
        let settings = v9();
        let rect = TileRect {
            min: TilePosition { x: 300, y: 0 },
            max: TilePosition { x: 3_000, y: 900 },
        };
        let space = spaces_touching(&settings, &rect)
            .into_iter()
            .next()
            .expect("a space");
        for (tile, kind) in space.cells() {
            let chunk = crate::terrain::ChunkPosition::of(tile);
            let generated = super::super::generated_chunk(&settings, chunk);
            assert_eq!(
                generated
                    .tile(crate::terrain::ChunkPosition::local_index(tile))
                    .kind,
                kind,
                "{tile:?} of {space:?}"
            );
        }
    }
}
