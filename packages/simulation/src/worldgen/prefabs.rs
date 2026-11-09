//! Prefab rooms: hand-authored points of interest as char grids, like the game's sprite
//! definitions.
//!
//! Each [`Prefab`] is a list of equal-length rows (at most [`MAX_PREFAB_WIDTH`] x
//! [`MAX_PREFAB_HEIGHT`] tiles) read through one shared legend:
//!
//! | Char | Cell |
//! |---|---|
//! | `,` or space | keep whatever generation put there |
//! | `.` | open air |
//! | `r` | host rock: the band's common rock (seals the room off from caves and hazards) |
//! | `c` | supply crate |
//! | `C` | supply crate or air (random fill) |
//! | `w` | wreckage |
//! | `%` | wreckage or air (random fill) |
//! | `S` | salvage core |
//! | `#` | vault wall |
//! | `D` | vault door |
//! | `X` | pressure seal |
//! | `1` `2` `3` | vault switch slots (each placement shuffles which slot is which order) |
//! | `R` | vault relay |
//! | `V` | vault cache |
//! | `f` | fossil shale |
//! | `g` | geode shell |
//! | `a` | an artifact of the band's kind |
//! | `o` | ore picked from the band's depth curves |
//! | `?` | ore or the formation's matrix (fossil shale or geode shell, random fill) |
//! | `h` | a guardian hazard in the band's hazard style |
//! | `p` | outpost plating |
//! | `$` | outpost locker |
//!
//! Variants come from mirroring (each placement may flip a prefab left to right), the random
//! fills above, the shuffled switch order, and the band (host rock, ore, artifacts, guardian
//! hazards and the renderer's stratum palette all follow the band the prefab lands in).

use crate::poi::{FormationStyle, PoiKind, PoiTile, VaultPuzzle};

/// Widest prefab, in tiles.
pub const MAX_PREFAB_WIDTH: usize = 17;
/// Tallest prefab, in tiles.
pub const MAX_PREFAB_HEIGHT: usize = 11;

/// What one legend character stamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cell {
    Keep,
    Air,
    HostRock,
    Fixture(PoiTile),
    CrateOrAir,
    WreckOrAir,
    /// A vault switch slot (0-based); the placement maps slots to orders.
    Switch(u8),
    Artifact,
    Ore,
    OreOrMatrix,
    Hazard,
}

impl Cell {
    /// The cell a legend character stands for.
    #[must_use]
    pub const fn from_char(ch: char) -> Option<Self> {
        Some(match ch {
            ',' | ' ' => Self::Keep,
            '.' => Self::Air,
            'r' => Self::HostRock,
            'c' => Self::Fixture(PoiTile::SupplyCrate),
            'C' => Self::CrateOrAir,
            'w' => Self::Fixture(PoiTile::Wreckage),
            '%' => Self::WreckOrAir,
            'S' => Self::Fixture(PoiTile::SalvageCore),
            '#' => Self::Fixture(PoiTile::VaultWall),
            'D' => Self::Fixture(PoiTile::VaultDoor),
            'X' => Self::Fixture(PoiTile::VaultSeal),
            '1' => Self::Switch(0),
            '2' => Self::Switch(1),
            '3' => Self::Switch(2),
            'R' => Self::Fixture(PoiTile::VaultRelay { lit: false }),
            'V' => Self::Fixture(PoiTile::VaultCache),
            'f' => Self::Fixture(PoiTile::FossilShale),
            'g' => Self::Fixture(PoiTile::GeodeShell),
            'a' => Self::Artifact,
            'o' => Self::Ore,
            '?' => Self::OreOrMatrix,
            'h' => Self::Hazard,
            'p' => Self::Fixture(PoiTile::OutpostPlating),
            '$' => Self::Fixture(PoiTile::OutpostSalvage),
            _ => return None,
        })
    }
}

/// What a prefab builds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefabRole {
    SupplyCache,
    DerelictRig,
    Vault(VaultPuzzle),
    Formation(FormationStyle),
    RivalClaim,
}

impl PrefabRole {
    #[must_use]
    pub const fn kind(self) -> PoiKind {
        match self {
            Self::SupplyCache => PoiKind::SupplyCache,
            Self::DerelictRig => PoiKind::DerelictRig,
            Self::Vault(_) => PoiKind::Vault,
            Self::Formation(_) => PoiKind::Formation,
            Self::RivalClaim => PoiKind::RivalClaim,
        }
    }
}

/// One hand-authored room.
#[derive(Clone, Copy, Debug)]
pub struct Prefab {
    pub name: &'static str,
    pub role: PrefabRole,
    pub rows: &'static [&'static str],
}

impl Prefab {
    #[must_use]
    pub const fn width(&self) -> i32 {
        if self.rows.is_empty() {
            0
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let width = self.rows[0].len() as i32;
            width
        }
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let height = self.rows.len() as i32;
        height
    }

    /// The cell at `(x, y)` of the unflipped grid (`Keep` outside it or on unknown chars).
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> Cell {
        let (Ok(column), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
            return Cell::Keep;
        };
        self.rows
            .get(row)
            .and_then(|line| line.as_bytes().get(column))
            .and_then(|byte| Cell::from_char(char::from(*byte)))
            .unwrap_or(Cell::Keep)
    }
}

/// Every prefab, grouped by kind.
pub const PREFABS: &[Prefab] = &[
    Prefab {
        name: "cache_hollow",
        role: PrefabRole::SupplyCache,
        rows: &[
            "rrrrrrrrr", //
            "r.......r", //
            "r..c.C..r", //
            "r.ccC.c.r", //
            "rrrrrrrrr", //
        ],
    },
    Prefab {
        name: "cache_niche",
        role: PrefabRole::SupplyCache,
        rows: &[
            ",rrrrr,", //
            "rr...rr", //
            "r..C..r", //
            "r.ccc.r", //
            "rrrrrrr", //
        ],
    },
    Prefab {
        name: "derelict_crash",
        role: PrefabRole::DerelictRig,
        rows: &[
            ",rrrrrrrrrrr,", //
            "rr...%.....rr", //
            "r...wwwww...r", //
            "r..wSwwwww%.r", //
            "r.%wwwwwwww.r", //
            "rr..w..w.C.rr", //
            ",rrrrrrrrrrr,", //
        ],
    },
    Prefab {
        name: "derelict_nose",
        role: PrefabRole::DerelictRig,
        rows: &[
            ",,rrrrrrr,,", //
            ",rr..%..rr,", //
            "rr..www..rr", //
            "r..wwSww..r", //
            "r..wwwww..r", //
            "r...www.%.r", //
            "rr.%.w..Crr", //
            ",rrrrrrrrr,", //
        ],
    },
    Prefab {
        name: "vault_sequence",
        role: PrefabRole::Vault(VaultPuzzle::Sequence),
        rows: &[
            "rrrrrrrrrrrrrrr", //
            "r#############r", //
            "r#...........#r", //
            "r#.....V.....#r", //
            "r#....###....#r", //
            "rD...........Dr", //
            "r#############r", //
            "rr...........rr", //
            "rr...........rr", //
            "rrr1rrr2rrr3rrr", //
            "rrrrrrrrrrrrrrr", //
        ],
    },
    Prefab {
        name: "vault_seals",
        role: PrefabRole::Vault(VaultPuzzle::Seals),
        rows: &[
            "rrrrrrrrrrrrr", //
            "r#####X#####r", //
            "r#.........#r", //
            "rX....V....Xr", //
            "r#...###...#r", //
            "rD.........Dr", //
            "r###########r", //
            "rrrrrrrrrrrrr", //
        ],
    },
    Prefab {
        name: "vault_relay",
        role: PrefabRole::Vault(VaultPuzzle::Relay),
        rows: &[
            "rrrrrrrrrrrrrrr", //
            "r.............r", //
            "r.###########.r", //
            "r.#.........#.r", //
            "r.#....V....#.r", //
            "r.D...###...D.r", //
            "rR###########Rr", //
            "rrrrrrrrrrrrrrr", //
        ],
    },
    Prefab {
        name: "fossil_bed",
        role: PrefabRole::Formation(FormationStyle::FossilBed),
        rows: &[
            ",,fffffffff,,", //
            "hffaff?ffaffh", //
            "hfffoffafffoh", //
            ",ff?fafff?ff,", //
            ",,fffffffff,,", //
        ],
    },
    Prefab {
        name: "fossil_shelf",
        role: PrefabRole::Formation(FormationStyle::FossilBed),
        rows: &[
            ",fffffffff,", //
            "ffaffoffaff", //
            "fo?ffaff?of", //
            ",fffhhfffff", //
        ],
    },
    Prefab {
        name: "geode_round",
        role: PrefabRole::Formation(FormationStyle::Geode),
        rows: &[
            "hh,ggggg,hh", //
            ",,ggo?ogg,,", //
            ",ggo...ogg,", //
            "ggo.....ogg", //
            "g?.......?g", //
            "ggo.....ogg", //
            ",ggo...ogg,", //
            ",,ggo?ogg,,", //
            "hh,ggggg,hh", //
        ],
    },
    Prefab {
        name: "geode_pocket",
        role: PrefabRole::Formation(FormationStyle::Geode),
        rows: &[
            ",,ggggg,,", //
            ",gg?o?gg,", //
            "gg.....gg", //
            "go.....og", //
            "gg.....gg", //
            "hgg?o?ggh", //
            "h,ggggg,h", //
        ],
    },
    Prefab {
        name: "outpost_hut",
        role: PrefabRole::RivalClaim,
        rows: &[
            ",rrrrrrrrrrrr,", //
            "rr...pppppp.rr", //
            "r....p....p..r", //
            "r.p..p$..$p.Cr", //
            "rppppppppppppr", //
            ",rrrrrrrrrrrr,", //
        ],
    },
    Prefab {
        name: "outpost_derrick",
        role: PrefabRole::RivalClaim,
        rows: &[
            ",rrrrrrrrrr,", //
            "rr...p....rr", //
            "r...ppp....r", //
            "r....p.....r", //
            "r..$.p.$.C.r", //
            "rppppppppppr", //
            ",rrrrrrrrrr,", //
        ],
    },
];

/// The prefabs that build `kind`.
pub fn prefabs_of(kind: PoiKind) -> impl Iterator<Item = &'static Prefab> {
    PREFABS
        .iter()
        .filter(move |prefab| prefab.role.kind() == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefabs_are_rectangular_within_limits_and_use_the_legend() {
        let mut names = std::collections::BTreeSet::new();
        for prefab in PREFABS {
            assert!(names.insert(prefab.name), "duplicate {}", prefab.name);
            assert!(!prefab.rows.is_empty(), "{}", prefab.name);
            assert!(prefab.rows.len() <= MAX_PREFAB_HEIGHT, "{}", prefab.name);
            let width = prefab.rows[0].len();
            assert!(width <= MAX_PREFAB_WIDTH, "{}", prefab.name);
            for row in prefab.rows {
                assert_eq!(row.len(), width, "{} row {row:?}", prefab.name);
                for ch in row.chars() {
                    assert!(Cell::from_char(ch).is_some(), "{}: {ch:?}", prefab.name);
                }
            }
        }
    }

    #[test]
    fn every_kind_and_puzzle_has_a_prefab() {
        for kind in PoiKind::ALL {
            assert!(prefabs_of(kind).count() >= 2, "{kind:?}");
        }
        for puzzle in VaultPuzzle::ALL {
            assert!(
                PREFABS
                    .iter()
                    .any(|prefab| prefab.role == PrefabRole::Vault(puzzle)),
                "{puzzle:?}"
            );
        }
    }

    #[test]
    fn vault_prefabs_hold_a_cache_and_their_lock() {
        for prefab in prefabs_of(PoiKind::Vault) {
            let cells: Vec<Cell> = (0..prefab.height())
                .flat_map(|y| (0..prefab.width()).map(move |x| prefab.cell(x, y)))
                .collect();
            let count = |cell: Cell| cells.iter().filter(|other| **other == cell).count();
            assert!(
                count(Cell::Fixture(PoiTile::VaultCache)) >= 1,
                "{}",
                prefab.name
            );
            assert!(
                count(Cell::Fixture(PoiTile::VaultDoor)) >= 1,
                "{}",
                prefab.name
            );
            match prefab.role {
                PrefabRole::Vault(VaultPuzzle::Sequence) => {
                    for slot in 0..3 {
                        assert_eq!(count(Cell::Switch(slot)), 1, "{}", prefab.name);
                    }
                }
                PrefabRole::Vault(VaultPuzzle::Seals) => {
                    assert!(
                        count(Cell::Fixture(PoiTile::VaultSeal)) >= 2,
                        "{}",
                        prefab.name
                    );
                }
                PrefabRole::Vault(VaultPuzzle::Relay) => {
                    assert_eq!(
                        count(Cell::Fixture(PoiTile::VaultRelay { lit: false })),
                        2,
                        "{}",
                        prefab.name
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}
