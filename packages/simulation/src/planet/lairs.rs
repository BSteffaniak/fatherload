//! Theme-boss lairs of the Wave 12 worlds (Wave 14 finish, closing expansion plan §16 "theme
//! bosses" for the jungle, ocean archipelago, glacier, desert dunes, volcanic and crystal
//! worlds).
//!
//! The Wave 13 outland worlds dug a lair under each landmark ([`super::outlands::boss_lair`]);
//! the six Wave 12 worlds get theirs the same way, laid over their generated ground by
//! [`overlay`] (called from [`super::PlanetGround::chunk_tiles`], so every theme world and the
//! frontier's generic path carry it, a pure function of the body like the rest):
//!
//! | World | Boss | Lair (under the landmark) | Way in | Relic in the vault |
//! |---|---|---|---|---|
//! | jungle | the Bramble King | the root hall under the Elder Tree | a shaft beside the trunk | the Bramble Crown (a reactor bloom) |
//! | ocean archipelago | the Tidecaller | the dry hall under the Drowned Spire | a walled dry well down the spire, under its floor | the Tide Pearl (a void pearl) |
//! | glacier | the Hoarfrost Wyrm | the ice cave under the Frozen Titan | a shaft beside the Titan | the Frost Heart (a pressure glyph) |
//! | desert dunes | the Dune Tyrant | the buried hall under the Sleeping Sentinel | a shaft beside the Sentinel | the Sentinel's Eye (a warden's eye) |
//! | volcanic | the Magma Drake | the magma hall under the Caldera | a shaft down the cone's flank | the Drake Heart (a mantle heart) |
//! | crystal | the Geode Colossus | the geode hall under the Choir Spire | a shaft beside the spire | the Choir Core (a crystal prism) |
//!
//! A lair is the outland lair's shape ([`super::outlands::LAIR_HALF_WIDTH`] and friends): a hall
//! 53 wide and 16 tall, [`LAIR_DEPTH`] rows under the landmark's ground, lined in the theme's
//! stone and lit by builder lamps ([`LairTile::Lamp`]), the outland **seal**
//! ([`super::outlands::OutlandTile::BossSeal`]) on its floor's middle, and the **reward vault**
//! behind a two-tile vault door in its east wall, holding the boss's relic over a floor of the
//! theme's core ore. Small bodies (radius up to 1,000) have their secret instead.
//!
//! The fight is the outland bosses' (`world/enemies/outlands.rs`): a rig entering the hall
//! wakes the boss, a told volley, a brood called on a rhythm of world time; its fall spends
//! the seal, opens the vault, raises [`LairEvent::BossDefeated`] and records the boss on the
//! hunt list ([`crate::endgame::hunt`]).
//!
//! Tiles: the [`crate::terrain::TileKind::Lair`] family, codes `0x3A00 + LairTile::code` (the
//! Wave 14 finish range `0x3A00..=0x3BFF`).

use serde::{Deserialize, Serialize};

use super::{
    BodyGround, PlanetGround, PlanetThemeId,
    outlands::{
        LAIR_HALF_WIDTH, LAIR_HEIGHT, LAIR_SHAFT_HALF, OutlandTile, VAULT_HEIGHT, VAULT_WIDTH,
    },
    wilds::{WildTile, is_small_body, wrap_delta},
};
use crate::{
    enemy::EnemyKind,
    frontier::FrontierTile,
    item::ItemKind,
    terrain::{ArtifactKind, CHUNK_SIZE_TILES, ChunkPosition, Tile, TileKind, tile_durability},
};

/// Kind-code base of [`TileKind::Lair`] tiles (`0x3A00 + LairTile::code`, inside the Wave 14
/// finish range `0x3A00..=0x3BFF`).
pub const LAIR_TILE_CODE_BASE: u64 = 0x3A00;

/// Rows from the landmark's ground down to its lair's ceiling.
pub const LAIR_DEPTH: i32 = 52;
/// Columns west of the landmark's centre the shaft runs down (worlds whose way in is beside
/// the landmark).
pub const SHAFT_OFFSET: i64 = LAIR_HALF_WIDTH - 4;
/// Columns between the builder lamps along the hall's walls.
const LAMP_EVERY: i64 = 9;

/// A tile of a theme-boss lair ([`TileKind::Lair`]; append only, the code is the index in
/// [`LairTile::ALL`]).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LairTile {
    /// The Bramble King's crown of flowering thorn (the jungle lair's relic).
    BrambleCrown,
    /// The Tidecaller's lantern pearl (the ocean lair's relic).
    TidePearl,
    /// The Hoarfrost Wyrm's frozen heart (the glacier lair's relic).
    FrostHeart,
    /// The Sleeping Sentinel's lost eye, kept by the Dune Tyrant (the dune lair's relic).
    SentinelEye,
    /// The Magma Drake's ember heart (the volcanic lair's relic).
    DrakeHeart,
    /// The Choir Spire's singing core, kept by the Geode Colossus (the crystal lair's relic).
    ChoirCore,
    /// A builder lamp set in a lair's wall: it lights the hall.
    Lamp,
}

impl LairTile {
    /// Every lair tile, in code order.
    pub const ALL: [Self; 7] = [
        Self::BrambleCrown,
        Self::TidePearl,
        Self::FrostHeart,
        Self::SentinelEye,
        Self::DrakeHeart,
        Self::ChoirCore,
        Self::Lamp,
    ];

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BrambleCrown => "the Bramble Crown",
            Self::TidePearl => "the Tide Pearl",
            Self::FrostHeart => "the Frost Heart",
            Self::SentinelEye => "the Sentinel's Eye",
            Self::DrakeHeart => "the Drake Heart",
            Self::ChoirCore => "the Choir Core",
            Self::Lamp => "Lair Lamp",
        }
    }

    /// Its code (its index in [`Self::ALL`]).
    #[must_use]
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// The tile with code `code`.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        if code < Self::ALL.len() as u64 {
            #[allow(clippy::cast_possible_truncation, reason = "checked above")]
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// Whether no drill digs it (none: the vault's walls and door are the outland ones).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        false
    }

    /// Drill strength needed (relics and lamps are stone-hard).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        3
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::Lamp => 6,
            _ => 8,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        0.15
    }

    /// Whether it is a relic (a boss's reward).
    #[must_use]
    pub const fn is_relic(self) -> bool {
        !matches!(self, Self::Lamp)
    }

    /// What a broken tile yields: a relic its artifact, a lamp nothing.
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        match self.reward() {
            Some(artifact) => Some(ItemKind::Artifact(artifact)),
            None => None,
        }
    }

    /// The artifact a relic is.
    #[must_use]
    pub const fn reward(self) -> Option<ArtifactKind> {
        Some(match self {
            Self::BrambleCrown => ArtifactKind::ReactorBloom,
            Self::TidePearl => ArtifactKind::VoidPearl,
            Self::FrostHeart => ArtifactKind::PressureGlyph,
            Self::SentinelEye => ArtifactKind::WardenEye,
            Self::DrakeHeart => ArtifactKind::MantleHeart,
            Self::ChoirCore => ArtifactKind::CrystalPrism,
            Self::Lamp => return None,
        })
    }

    /// How much light it gives off in the dark (0-255; the renderer's glow).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::Lamp => 230,
            _ => 170,
        }
    }
}

/// Something a Wave 12 world's theme boss or its lair did (the
/// [`crate::event::GameplayEventKind::Lair`] events, netcode codes 1200-1219).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LairEvent {
    /// The boss woke: a rig entered its hall.
    BossWoke,
    /// The boss is calling its brood (they come next).
    BossSummons,
    /// The boss fell (or a Peaceful lair yielded): its seal is spent and its vault open.
    BossDefeated,
    /// The boss's relic was dug out of its vault.
    RelicFound,
}

impl LairEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 4] = [
        Self::BossWoke,
        Self::BossSummons,
        Self::BossDefeated,
        Self::RelicFound,
    ];

    /// Offset of the event's netcode code from 1200.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// How a lair is reached from the surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WayIn {
    /// A shaft from the surface [`SHAFT_OFFSET`] columns west of the landmark into the hall's
    /// west end.
    Beside,
    /// A walled dry well straight down from row `top` (under a dry room's floor) through the
    /// sea and the seabed into the hall's middle (the Drowned Spire's).
    Well { top: i32 },
}

/// A Wave 12 world's theme boss: who fights where for what (see [`LAIR_BOSSES`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LairBoss {
    pub theme: PlanetThemeId,
    /// The boss creature.
    pub kind: EnemyKind,
    pub name: &'static str,
    /// Where it waits.
    pub lair: &'static str,
    /// The relic tile in its reward vault.
    pub relic: LairTile,
    /// The stone its hall is lined with.
    pub wall: TileKind,
    pub way_in: WayIn,
}

/// Every Wave 12 world's theme boss, in theme order (jungle .. crystal).
pub const LAIR_BOSSES: [LairBoss; 6] = [
    LairBoss {
        theme: PlanetThemeId::JUNGLE,
        kind: EnemyKind::BrambleKing,
        name: "the Bramble King",
        lair: "the root hall under the Elder Tree",
        relic: LairTile::BrambleCrown,
        wall: TileKind::Wild(WildTile::ElderBark),
        way_in: WayIn::Beside,
    },
    LairBoss {
        theme: PlanetThemeId::OCEAN,
        kind: EnemyKind::Tidecaller,
        name: "the Tidecaller",
        lair: "the dry hall under the Drowned Spire",
        relic: LairTile::TidePearl,
        wall: TileKind::Wild(WildTile::RuinStone),
        way_in: WayIn::Well {
            top: super::wilds::ocean::SEA_LEVEL + 1,
        },
    },
    LairBoss {
        theme: PlanetThemeId::GLACIER,
        kind: EnemyKind::HoarfrostWyrm,
        name: "the Hoarfrost Wyrm",
        lair: "the ice cave under the Frozen Titan",
        relic: LairTile::FrostHeart,
        wall: TileKind::Wild(WildTile::TitanBone),
        way_in: WayIn::Beside,
    },
    LairBoss {
        theme: crate::frontier::DUNE_WORLD,
        kind: EnemyKind::DuneTyrant,
        name: "the Dune Tyrant",
        lair: "the buried hall under the Sleeping Sentinel",
        relic: LairTile::SentinelEye,
        wall: TileKind::Frontier(FrontierTile::RuinBrick),
        way_in: WayIn::Beside,
    },
    LairBoss {
        theme: crate::frontier::VOLCANIC_WORLD,
        kind: EnemyKind::MagmaDrake,
        name: "the Magma Drake",
        lair: "the magma hall under the Caldera",
        relic: LairTile::DrakeHeart,
        wall: TileKind::Frontier(FrontierTile::Basalt),
        way_in: WayIn::Beside,
    },
    LairBoss {
        theme: crate::frontier::CRYSTAL_WORLD,
        kind: EnemyKind::GeodeColossus,
        name: "the Geode Colossus",
        lair: "the geode hall under the Choir Spire",
        relic: LairTile::ChoirCore,
        wall: TileKind::Frontier(FrontierTile::PrismaticStone),
        way_in: WayIn::Beside,
    },
];

/// The boss of `theme`, if it is one of the six Wave 12 worlds.
#[must_use]
pub fn lair_boss(theme: PlanetThemeId) -> Option<&'static LairBoss> {
    LAIR_BOSSES.iter().find(|boss| boss.theme == theme)
}

/// The boss whose creature is `kind`.
#[must_use]
pub fn lair_boss_of(kind: EnemyKind) -> Option<&'static LairBoss> {
    LAIR_BOSSES.iter().find(|boss| boss.kind == kind)
}

/// A lair on one body (strip tiles; see [`lair_of`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lair {
    pub boss: &'static LairBoss,
    /// The lair's centre column (the landmark's).
    pub centre: i64,
    /// Its first open row (the ceiling's underside) and its floor row (the first solid row).
    pub top: i32,
    pub floor: i32,
    /// The shaft's centre column and its first open row.
    pub shaft: i64,
    pub shaft_top: i32,
    /// The seal on the hall's floor: while it stands unspent, the boss waits here.
    pub seal: (i32, i32),
    /// The reward vault's door tiles (two, in the hall's east wall) and its relic.
    pub door: [(i32, i32); 2],
    pub relic: (i32, i32),
}

impl Lair {
    /// Whether strip tile `(column, row)` lies inside the open hall (`width`: the strip's).
    #[must_use]
    pub const fn contains(&self, column: i64, row: i32, width: i64) -> bool {
        wrap_delta(column, self.centre, width).abs() <= LAIR_HALF_WIDTH
            && row >= self.top
            && row < self.floor
    }

    /// The hall's middle (strip tile).
    #[must_use]
    pub const fn middle(&self) -> (i64, i32) {
        (self.centre, self.top + LAIR_HEIGHT / 2)
    }
}

/// The landmark's centre column of a Wave 12 world (the wild worlds' half-lap rule, the
/// frontier's own landmark).
#[must_use]
pub fn landmark_column(ground: &PlanetGround) -> Option<i64> {
    let theme = ground.params().theme;
    if crate::frontier::style_of(theme).is_some() {
        return crate::frontier::landmark(ground).map(|landmark| i64::from(landmark.column));
    }
    super::wilds::wild_theme(theme).map(|_| super::wilds::landmark_column(ground))
}

/// The lair of a Wave 12 world's body, under its landmark: planets only (a small moon or
/// crater rock keeps its secret instead).
#[must_use]
pub fn lair_of(ground: &PlanetGround) -> Option<Lair> {
    let boss = lair_boss(ground.params().theme)?;
    if is_small_body(ground.params()) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let centre = landmark_column(ground)?.rem_euclid(width);
    let ground_row = ground.surface_row(i32::try_from(centre).unwrap_or(0));
    let top = ground_row + LAIR_DEPTH;
    let floor = top + LAIR_HEIGHT;
    let column = |offset: i64| i32::try_from((centre + offset).rem_euclid(width)).unwrap_or(0);
    let door_column = column(LAIR_HALF_WIDTH + 1);
    let (shaft, shaft_top) = match boss.way_in {
        WayIn::Beside => {
            let shaft = (centre - SHAFT_OFFSET).rem_euclid(width);
            (shaft, ground.surface_row(i32::try_from(shaft).unwrap_or(0)))
        }
        WayIn::Well { top } => (centre, top),
    };
    Some(Lair {
        boss,
        centre,
        top,
        floor,
        shaft,
        shaft_top,
        seal: (column(0), floor - 1),
        door: [(door_column, floor - 1), (door_column, floor - 2)],
        relic: (column(LAIR_HALF_WIDTH + 2 + VAULT_WIDTH / 2), floor - 1),
    })
}

const fn full(kind: TileKind) -> Tile {
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

/// The lair's tile at strip column `column`, row `row`, if any.
///
/// The open hall (its seal on the floor, lamps in its walls), its lining round it, the shaft or
/// well, and the sealed reward vault in the east wall.
#[must_use]
pub fn lair_tile(ground: &PlanetGround, lair: &Lair, column: i64, row: i32) -> Option<Tile> {
    let width = i64::from(ground.strip_width());
    let wall = full(lair.boss.wall);
    // The way in.
    let sdx = wrap_delta(column, lair.shaft, width);
    if row >= lair.shaft_top && row < lair.top + 1 && sdx.abs() <= LAIR_SHAFT_HALF + 1 {
        return Some(if sdx.abs() <= LAIR_SHAFT_HALF {
            full(TileKind::Air)
        } else {
            wall
        });
    }
    let dx = wrap_delta(column, lair.centre, width);
    if dx.abs() > LAIR_HALF_WIDTH + VAULT_WIDTH + 4 || row < lair.top - 2 || row > lair.floor + 3 {
        return None;
    }
    // The hall: a rounded box (its upper corners cut).
    let corner = (dx.abs() - (LAIR_HALF_WIDTH - 5)).max(0);
    let ceiling = lair.top + i32::try_from(corner).unwrap_or(0) / 2;
    if dx.abs() <= LAIR_HALF_WIDTH && row >= ceiling && row < lair.floor {
        if i64::from(lair.seal.0) == column && row == lair.seal.1 {
            return Some(full(TileKind::Outland(OutlandTile::BossSeal)));
        }
        return Some(full(TileKind::Air));
    }
    // The reward vault, east of the hall, behind its door.
    let vault_first = LAIR_HALF_WIDTH + 2;
    let in_vault_box = (vault_first - 1..=vault_first + VAULT_WIDTH).contains(&dx)
        && (lair.floor - VAULT_HEIGHT - 1..=lair.floor).contains(&row);
    if in_vault_box {
        let inside = (vault_first..vault_first + VAULT_WIDTH).contains(&dx)
            && (lair.floor - VAULT_HEIGHT..lair.floor).contains(&row);
        if inside {
            if i64::from(lair.relic.0) == column && row == lair.relic.1 {
                return Some(full(TileKind::Lair(lair.boss.relic)));
            }
            if row == lair.floor - 1 && dx % 2 == 0 {
                return Some(full(ground.data_theme().core_ore));
            }
            return Some(full(TileKind::Air));
        }
        if dx == vault_first - 1 && (row == lair.floor - 1 || row == lair.floor - 2) {
            return Some(full(TileKind::Outland(OutlandTile::VaultDoor)));
        }
        return Some(full(TileKind::Outland(OutlandTile::VaultWall)));
    }
    // The hall's lining: two tiles round the open space, builder lamps along its sides and
    // ceiling.
    if dx.abs() <= LAIR_HALF_WIDTH + 2 && row >= lair.top - 2 && row <= lair.floor + 1 {
        let lamp_on_side =
            dx.abs() == LAIR_HALF_WIDTH + 1 && (row == lair.top + 4 || row == lair.floor - 8);
        let lamp_overhead =
            row == lair.top - 1 && dx.abs() < LAIR_HALF_WIDTH - 5 && dx.rem_euclid(LAMP_EVERY) == 4;
        if lamp_on_side || lamp_overhead {
            return Some(full(TileKind::Lair(LairTile::Lamp)));
        }
        return Some(wall);
    }
    None
}

/// Lays `ground`'s lair (if it has one) over the generated tiles of `chunk` (row-major,
/// [`ChunkPosition::local_index`] order; columns wrap).
pub fn overlay(ground: &PlanetGround, chunk: ChunkPosition, tiles: &mut [Tile]) {
    let Some(lair) = lair_of(ground) else {
        return;
    };
    let origin = chunk.origin();
    let (low, high) = (lair.shaft_top.min(lair.top) - 2, lair.floor + 4);
    if origin.y + CHUNK_SIZE_TILES <= low || origin.y > high {
        return;
    }
    let width = i64::from(ground.strip_width());
    let reach = LAIR_HALF_WIDTH + VAULT_WIDTH + 4;
    let near = |column: i64| {
        wrap_delta(column, lair.centre, width).abs() <= reach + i64::from(CHUNK_SIZE_TILES)
            || wrap_delta(column, lair.shaft, width).abs()
                <= LAIR_SHAFT_HALF + 1 + i64::from(CHUNK_SIZE_TILES)
    };
    if !near(i64::from(origin.x) + i64::from(CHUNK_SIZE_TILES / 2)) {
        return;
    }
    let grid = ground.grid();
    for y in origin.y..origin.y + CHUNK_SIZE_TILES {
        if y < low || y > high || grid.span_at(y) != 1 {
            continue;
        }
        for x in origin.x..origin.x + CHUNK_SIZE_TILES {
            let column = i64::from(x).rem_euclid(width);
            if let Some(tile) = lair_tile(ground, &lair, column, y) {
                let index = usize::try_from((y - origin.y) * CHUNK_SIZE_TILES + (x - origin.x))
                    .unwrap_or(0);
                if let Some(slot) = tiles.get_mut(index) {
                    *slot = tile;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
