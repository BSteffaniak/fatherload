//! The outland worlds (Wave 13 planets-c, expansion plan §16 "planet themes"): the **fungal**,
//! **machine** and **dead** planets, each a world like the home ground level.
//!
//! Each theme is registered like the Wave 12 wild worlds ([`super::wilds`], whose generation
//! helpers it shares): as data in the planet theme registry ([`super::theme::PLANET_THEMES`]:
//! palette, crust, three strata with rock, accent and ores, the core ore), as hooks in
//! [`super::hooks::THEME_HOOKS`] ([`hooks`]: weather, climate, hazard tags, creatures, music,
//! the settlement site) and as a [`super::ThemeWorld`] (its surface, the features over its
//! strata, its depth, its landmark and its settlement site). The space generator's
//! `Theme::Fungal`, `Theme::MachineWorld` and `Theme::DeadWorld` land on them
//! ([`crate::place::planet_theme_of`]).
//!
//! | Theme | Surface | Strata (signature rule) | Landmark and boss |
//! |---|---|---|---|
//! | [`fungal`] | puffball mounds under giant mushrooms whose caps throw you back up | Mycelium Mat (it grows back into your tunnels), Glowcap Grottos (a drilled glowcap flashes and stuns the hunters), Sporeheart Deep (spore vents swell, then blast you upward) | the Mother Cap; the Sporemother in the hollow under it |
//! | [`machine`] | riveted plate terraces, pylons with beacons | Plate Crust (conveyor belts carry you), Gear Works (gears turn on a rhythm and grind), Reactor Shell (light three relays to open a power vault) | the Reactor Spire; the Reactor Warden in the core reactor's hall |
//! | [`dead`] | ash plains and petrified cities | Ash Beds (loose ash pours into your shaft), the Necropolis (break a tomb's seal and its wardens wake), Grave Mantle (ghost-light wisps drift through the rock and chill the hull) | the Hollow Throne; the Ashen Sovereign in the tomb beneath |
//!
//! **Depth**: the wild worlds' ([`super::wilds::core_floor_row`]): 800 rows at radius 1,500 to
//! 2,500 at 8,000 and up (1,715 at 5,000), down to the core chamber; a small moon or crater
//! rock (radius up to 1,000) is strata all the way to its true core.
//!
//! **Sites** (pure functions of the body): the settlement (level ground for the Tokkari or the
//! Siphra to build on, [`super::wilds::settlement_column`]), the landmark half a lap from it
//! with the theme boss's lair under it ([`boss_lair`]), a builders' outpost three quarters of
//! a lap round ([`builder_outposts`], for the galaxy story) and, on small bodies, a secret a
//! quarter lap round ([`secret_tile`]).

pub mod dead;
pub mod fungal;
pub mod hooks;
pub mod life;
pub mod machine;
#[cfg(test)]
mod tests;
pub mod tile;

pub use life::{OutlandRule, OutlandsEvent, outland_theme};
pub use tile::{OUTLAND_TILE_CODE_BASE, OutlandTile};

use super::{
    PlanetGround, PlanetThemeId, SurfaceSite, hash3,
    wilds::{Cell, SECRET_DEPTH, is_small_body, secret_column, settlement_column, wrap_delta},
};
use crate::{
    enemy::EnemyKind,
    terrain::{ArtifactKind, Tile, TileKind, tile_durability},
};

/// The three outland themes, in id order.
pub const OUTLAND_THEMES: [PlanetThemeId; 3] = [
    PlanetThemeId::FUNGAL,
    PlanetThemeId::MACHINE,
    PlanetThemeId::DEAD,
];

/// Whether `theme` is one of the outland worlds.
#[must_use]
pub const fn is_outland(theme: PlanetThemeId) -> bool {
    matches!(theme.0, 9..=11)
}

/// A hash of `(purpose, a, b)` for `ground`'s body (the outlands' own purposes, clear of the
/// wild worlds').
#[must_use]
pub const fn oroll(ground: &PlanetGround, purpose: u64, a: i64, b: i64) -> u64 {
    hash3(ground.params().seed, 0x0C7A_0000 + purpose, a, b)
}

/// A full tile of `kind`.
#[must_use]
pub const fn tile(kind: TileKind) -> Tile {
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

/// An outland tile.
#[must_use]
pub const fn outland(kind: OutlandTile) -> Tile {
    tile(TileKind::Outland(kind))
}

/// The open air.
#[must_use]
pub const fn air() -> Tile {
    tile(TileKind::Air)
}

/// A value from `hash`'s bits at `shift`, in `0..n`.
#[must_use]
pub const fn pick(hash: u64, shift: u32, n: u64) -> i64 {
    ((hash >> shift) % n).cast_signed()
}

// --- The secret (small bodies) ---------------------------------------------------------------

/// The secret's tile at `cell`, if it is part of it (small bodies only): a marker on the
/// surface over a chamber [`SECRET_DEPTH`] rows down holding an [`OutlandTile::Cache`],
/// walled in `rock`.
#[must_use]
pub fn secret_tile(ground: &PlanetGround, cell: &Cell, rock: Tile, marker: Tile) -> Option<Tile> {
    if !is_small_body(ground.params()) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let delta = wrap_delta(cell.first, secret_column(ground), width) / cell.span.max(1);
    if !(-3..=3).contains(&delta) {
        return None;
    }
    if delta == 0 && (cell.row == cell.surface - 1 || cell.row == cell.surface - 2) {
        return Some(marker);
    }
    let top = cell.surface + SECRET_DEPTH;
    if cell.row < top - 1 || cell.row > top + 3 {
        return None;
    }
    if (-2..=2).contains(&delta) && (top..=top + 2).contains(&cell.row) {
        if delta == 0 && cell.row == top + 2 {
            return Some(outland(OutlandTile::Cache));
        }
        return Some(air());
    }
    Some(rock)
}

// --- Builders' outposts (the galaxy story's targets) ----------------------------------------

/// The level ground an outpost stands on (half width, columns).
pub const OUTPOST_HALF: i64 = 6;
/// Rows of the outpost's obelisk over its plinth.
pub const OUTPOST_HEIGHT: i32 = 10;
/// Rows from the surface down to the outpost's cache chamber floor.
pub const OUTPOST_CACHE_DEPTH: i32 = 6;

/// A builders' outpost on a body: a glyph-marked obelisk over a sealed cache.
///
/// TODO-SEAM w13-story: the galaxy story targets these ([`builder_outposts`]): a quest step
/// can name the body and the outpost's glyph, and the cache's old circuit is its clue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuilderOutpost {
    /// The obelisk's ground (its centre is the obelisk).
    pub site: SurfaceSite,
    /// The glowing glyph's tile (strip column, row).
    pub glyph: (i32, i32),
    /// The cache's tile.
    pub cache: (i32, i32),
}

/// The outpost's centre column: three quarters of a lap from the settlement.
#[must_use]
pub fn outpost_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (settlement_column(ground) + width * 3 / 4).rem_euclid(width)
}

/// The builders' outposts on an outland body (one each, a pure function of the body).
#[must_use]
pub fn builder_outposts(ground: &PlanetGround) -> Vec<BuilderOutpost> {
    if !is_outland(ground.params().theme) {
        return Vec::new();
    }
    let centre = outpost_column(ground);
    let floor = super::BodyGround::surface_row(ground, i32::try_from(centre).unwrap_or(0));
    let column = i32::try_from(centre).unwrap_or(0);
    vec![BuilderOutpost {
        site: SurfaceSite {
            first: column - i32::try_from(OUTPOST_HALF).unwrap_or(6),
            width: i32::try_from(OUTPOST_HALF * 2 + 1).unwrap_or(13),
            floor,
        },
        glyph: (column, floor - OUTPOST_HEIGHT + 2),
        cache: (column, floor + OUTPOST_CACHE_DEPTH - 1),
    }]
}

/// The outpost's tile at `cell`, if any (`floor` is the outpost's level ground row).
#[must_use]
pub fn outpost_tile(ground: &PlanetGround, cell: &Cell, floor: i32) -> Option<Tile> {
    if cell.span != 1 {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let dx = wrap_delta(cell.first, outpost_column(ground), width);
    if dx.abs() > OUTPOST_HALF {
        return None;
    }
    let row = cell.row;
    // The obelisk: three wide, a glyph near its top and one lower, a capstone.
    if dx.abs() <= 1 && (floor - OUTPOST_HEIGHT..floor).contains(&row) {
        if dx == 0 && (row == floor - OUTPOST_HEIGHT + 2 || row == floor - 5) {
            return Some(outland(OutlandTile::BuilderGlyph));
        }
        if row == floor - OUTPOST_HEIGHT && dx != 0 {
            return Some(air());
        }
        return Some(outland(OutlandTile::BuilderStone));
    }
    // A plinth of dressed stone, and the sealed cache under it.
    if row == floor && dx.abs() < OUTPOST_HALF {
        return Some(outland(OutlandTile::BuilderStone));
    }
    let cache_floor = floor + OUTPOST_CACHE_DEPTH;
    if (cache_floor - 3..=cache_floor).contains(&row) && dx.abs() <= 3 {
        if row == cache_floor || dx.abs() == 3 || row == cache_floor - 3 {
            return Some(outland(OutlandTile::BuilderStone));
        }
        if dx == 0 && row == cache_floor - 1 {
            return Some(outland(OutlandTile::BuilderCache));
        }
        if dx.abs() == 2 && row == cache_floor - 1 {
            return Some(outland(OutlandTile::BuilderGlyph));
        }
        return Some(air());
    }
    None
}

// --- Theme bosses -------------------------------------------------------------------------------

/// Rows from the landmark's ground down to its lair's ceiling.
pub const LAIR_DEPTH: i32 = 52;
/// The lair's half width and height (open rows).
pub const LAIR_HALF_WIDTH: i64 = 26;
pub const LAIR_HEIGHT: i32 = 16;
/// The lair's shaft (half width) from the landmark down into it.
pub const LAIR_SHAFT_HALF: i64 = 2;
/// The reward vault beside the lair: inner width and height.
pub const VAULT_WIDTH: i64 = 6;
pub const VAULT_HEIGHT: i32 = 4;

/// A theme boss: who fights where for what (see [`THEME_BOSSES`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeBoss {
    pub theme: PlanetThemeId,
    /// The boss creature.
    pub kind: EnemyKind,
    pub name: &'static str,
    /// Where it waits.
    pub lair: &'static str,
    /// The relic in its reward vault (the boss-won reward, with its loot).
    pub reward: ArtifactKind,
    /// The relic tile in the vault.
    pub relic: OutlandTile,
}

/// Every outland theme's boss.
///
/// TODO-SEAM w13-endgame: the hunt list reads this registry (theme, boss kind, name, lair,
/// reward); a kill raises [`OutlandsEvent::BossDefeated`] with the body's store, and
/// `WorldState::outland_boss_beaten_here` reads a loaded body's spent seal.
pub const THEME_BOSSES: [ThemeBoss; 3] = [
    ThemeBoss {
        theme: PlanetThemeId::FUNGAL,
        kind: EnemyKind::Sporemother,
        name: "the Sporemother",
        lair: "the hollow under the Mother Cap",
        reward: ArtifactKind::ReactorBloom,
        relic: OutlandTile::SporeCrown,
    },
    ThemeBoss {
        theme: PlanetThemeId::MACHINE,
        kind: EnemyKind::ReactorWarden,
        name: "the Reactor Warden",
        lair: "the core reactor's hall under the Reactor Spire",
        reward: ArtifactKind::MachineSigil,
        relic: OutlandTile::ReactorHeart,
    },
    ThemeBoss {
        theme: PlanetThemeId::DEAD,
        kind: EnemyKind::AshenSovereign,
        name: "the Ashen Sovereign",
        lair: "the tomb beneath the Hollow Throne",
        reward: ArtifactKind::StarCore,
        relic: OutlandTile::ThroneRelic,
    },
];

/// The boss of `theme`, if it is an outland theme.
#[must_use]
pub fn theme_boss(theme: PlanetThemeId) -> Option<&'static ThemeBoss> {
    THEME_BOSSES.iter().find(|boss| boss.theme == theme)
}

/// A theme boss's lair on one body (strip tiles; see [`boss_lair`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossLair {
    pub boss: &'static ThemeBoss,
    /// The lair's centre column (the landmark's).
    pub centre: i64,
    /// Its first open row (the ceiling's underside) and its floor row (the first solid row).
    pub top: i32,
    pub floor: i32,
    /// The seal on the lair's floor: while it stands unspent, the boss waits here.
    pub seal: (i32, i32),
    /// The reward vault's door tiles (two, in the lair's east wall) and its relic.
    pub door: [(i32, i32); 2],
    pub relic: (i32, i32),
}

impl BossLair {
    /// Whether strip tile `(column, row)` lies inside the open lair (`width`: the strip's).
    #[must_use]
    pub const fn contains(&self, column: i64, row: i32, width: i64) -> bool {
        wrap_delta(column, self.centre, width).abs() <= LAIR_HALF_WIDTH
            && row >= self.top
            && row < self.floor
    }

    /// The lair's middle (strip tile).
    #[must_use]
    pub const fn middle(&self) -> (i64, i32) {
        (self.centre, self.top + LAIR_HEIGHT / 2)
    }
}

/// The boss lair of an outland body: under its landmark ([`landmark_column`]). Planets only
/// (a small moon or crater rock has its secret instead).
#[must_use]
pub fn boss_lair(ground: &PlanetGround) -> Option<BossLair> {
    let boss = theme_boss(ground.params().theme)?;
    if is_small_body(ground.params()) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let centre = landmark_column(ground);
    let ground_row = landmark_floor(ground);
    let top = ground_row + LAIR_DEPTH;
    let floor = top + LAIR_HEIGHT;
    let column = |offset: i64| i32::try_from((centre + offset).rem_euclid(width)).unwrap_or(0);
    let door_column = column(LAIR_HALF_WIDTH + 1);
    Some(BossLair {
        boss,
        centre,
        top,
        floor,
        seal: (column(0), floor - 1),
        door: [(door_column, floor - 1), (door_column, floor - 2)],
        relic: (column(LAIR_HALF_WIDTH + 2 + VAULT_WIDTH / 2), floor - 1),
    })
}

/// The landmark's centre column: half a lap from the settlement (the wild worlds' rule).
#[must_use]
pub fn landmark_column(ground: &PlanetGround) -> i64 {
    super::wilds::landmark_column(ground)
}

/// The landmark's level ground row (the theme's surface at its centre).
#[must_use]
pub fn landmark_floor(ground: &PlanetGround) -> i32 {
    super::BodyGround::surface_row(ground, i32::try_from(landmark_column(ground)).unwrap_or(0))
}

/// The lair's tile at `cell`, if any: the open hall (its seal on the floor), a wall of `wall`
/// round it, the shaft up to the landmark, and the sealed reward vault in the east wall.
#[must_use]
pub fn lair_tile(ground: &PlanetGround, lair: &BossLair, cell: &Cell, wall: Tile) -> Option<Tile> {
    if cell.span != 1 {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let dx = wrap_delta(cell.first, lair.centre, width);
    let row = cell.row;
    if dx.abs() > LAIR_HALF_WIDTH + VAULT_WIDTH + 4 || row < cell.surface || row > lair.floor + 3 {
        return None;
    }
    // The shaft from the landmark's floor down into the hall.
    if dx.abs() <= LAIR_SHAFT_HALF && row < lair.top {
        return Some(air());
    }
    if dx.abs() <= LAIR_SHAFT_HALF + 1 && row < lair.top {
        return Some(wall);
    }
    // The hall: a rounded box (its upper corners cut).
    let corner = (dx.abs() - (LAIR_HALF_WIDTH - 5)).max(0);
    let ceiling = lair.top + i32::try_from(corner).unwrap_or(0) / 2;
    if dx.abs() <= LAIR_HALF_WIDTH && row >= ceiling && row < lair.floor {
        if (i64::from(lair.seal.0) == cell.first) && row == lair.seal.1 {
            return Some(outland(OutlandTile::BossSeal));
        }
        return Some(air());
    }
    // The reward vault, east of the hall, behind its door.
    let vault_first = LAIR_HALF_WIDTH + 2;
    let in_vault_box = (vault_first - 1..=vault_first + VAULT_WIDTH).contains(&dx)
        && (lair.floor - VAULT_HEIGHT - 1..=lair.floor).contains(&row);
    if in_vault_box {
        let inside = (vault_first..vault_first + VAULT_WIDTH).contains(&dx)
            && (lair.floor - VAULT_HEIGHT..lair.floor).contains(&row);
        if inside {
            if i64::from(lair.relic.0) == cell.first && row == lair.relic.1 {
                return Some(outland(lair.boss.relic));
            }
            if row == lair.floor - 1 && dx % 2 == 0 {
                return Some(tile(ground.data_theme().core_ore));
            }
            return Some(air());
        }
        if dx == vault_first - 1 && (row == lair.floor - 1 || row == lair.floor - 2) {
            return Some(outland(OutlandTile::VaultDoor));
        }
        return Some(outland(OutlandTile::VaultWall));
    }
    // The hall's wall: two tiles of the theme's lining round the open space.
    if dx.abs() <= LAIR_HALF_WIDTH + 2 && row >= lair.top - 2 && row <= lair.floor + 1 {
        return Some(wall);
    }
    None
}
