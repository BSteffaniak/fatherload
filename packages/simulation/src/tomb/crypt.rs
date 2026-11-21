//! Top-down crypts (WP-TOMBS, Wave 5 T29): layout, tile sets, traps and bestiary.
//!
//! The world side (traps going off, pits, the crypt creatures' AI) lives in
//! `world/crypts.rs` and `world/enemies/crypt.rs`; the pilot's top-down movement and roll in
//! [`crate::pilot::top_down`].
//!
//! * **Layout** ([`layout`], [`CryptRealizer`], swapped in by
//!   [`super::realize::realizer_for`]): the plan's rooms on a grid of [`GRID`] cells seen from
//!   above, the main path a winding walk from the entry, the vault off its parent through any
//!   free side, the crypt lord's hall two cells by two. Doorways are three tiles wide; a puzzle
//!   room's way on is a lever gate (always an east or west doorway, the gate hanging under its
//!   lintel). The realized cells are plain crypt masonry ([`BuiltTile::TombWall`] in the
//!   top-down style), open floor and the tomb fixtures; **traps are not tiles**: they are
//!   overlays on open floor ([`CryptTrap`], kept in the run's [`CryptState`]) so creatures walk,
//!   shots fly and sight lines pass over them exactly as over the floor.
//! * **Traps** ([`CryptTrapKind`]): floor spikes and flame jets on a telegraphed cycle
//!   ([`CryptTrap::stage`]: idle, a warning, then active), pressure-plate arrows (a click, then
//!   arrows from both walls along the plate's row), rolling boulders (a rumble, then a boulder
//!   rolls out of its chute across the room), and cracked floors that crumble into pits a beat
//!   after a pilot steps on them. A dodge roll passes over every one of them.
//! * **Tile sets** ([`CryptTileset`]): catacomb (the deep strata and village old quarters),
//!   sandstone (mesas), the sky vault (sky continents) and the lunar vault (the Moon). The
//!   tiles are the same; the game draws them per set.
//! * **Bestiary** ([`bestiary`]): skeleton packs, spectres that drift through walls, cultist
//!   casters and stone golems, and the crypt lord, a boss with three phases.

pub mod layout;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{enemy::EnemyKind, ids::SimulationTick, terrain::TilePosition};

use super::{SiteSetting, realize::SpawnRole};

pub use layout::{CryptFloor, CryptRealizer, GRID, layout};

/// A crypt trap: an overlay on open floor (see the module docs).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CryptTrapKind {
    /// Floor spikes that rise on a cycle (a rattle and a glint first).
    Spikes,
    /// A pressure plate: a click, then arrows from both walls along its row.
    ArrowPlate,
    /// A boulder chute against a wall: a rumble, then a boulder rolls across the room.
    BoulderChute,
    /// A flame vent: a glow, then a jet of fire on a cycle.
    FlameJet,
    /// Cracked flagstones: they crumble into a pit a beat after a pilot steps on them.
    CrackedFloor,
    /// An open pit: a pilot who walks in falls (hurt), climbing out at the nearest safe floor.
    Pit,
}

impl CryptTrapKind {
    pub const ALL: [Self; 6] = [
        Self::Spikes,
        Self::ArrowPlate,
        Self::BoulderChute,
        Self::FlameJet,
        Self::CrackedFloor,
        Self::Pit,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spikes => "Floor Spikes",
            Self::ArrowPlate => "Arrow Plate",
            Self::BoulderChute => "Boulder Chute",
            Self::FlameJet => "Flame Jet",
            Self::CrackedFloor => "Cracked Floor",
            Self::Pit => "Pit",
        }
    }

    /// The cycle (period, warning, active) in milliseconds of a cycling trap; `None` for traps a
    /// pilot sets off.
    #[must_use]
    pub const fn cycle(self) -> Option<(u64, u64, u64)> {
        match self {
            Self::Spikes => Some((2_400, 550, 750)),
            Self::FlameJet => Some((2_800, 650, 900)),
            // The rumble, then the roll (a boulder is spawned as the warning ends).
            Self::BoulderChute => Some((3_600, 1_000, 200)),
            Self::ArrowPlate | Self::CrackedFloor | Self::Pit => None,
        }
    }

    /// Damage per second to a pilot inside an active spike or flame tile (before tier,
    /// difficulty and relics).
    #[must_use]
    pub const fn damage_per_second(self) -> f32 {
        match self {
            Self::Spikes => 42.0,
            Self::FlameJet => 56.0,
            Self::ArrowPlate | Self::BoulderChute | Self::CrackedFloor | Self::Pit => 0.0,
        }
    }
}

/// A crypt creature's telegraphed attack ([`crate::GameplayEventKind::CryptTell`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CryptAttack {
    /// A golem raises its fists: a slam around it.
    Slam,
    /// The lord's fan of five hex bolts at its target.
    Fan,
    /// The lord's ring of twelve hex bolts.
    Ring,
    /// The lord's charge across the hall.
    Charge,
    /// The lord's spiral: three rotating waves of eight bolts.
    Spiral,
}

impl CryptAttack {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Slam => "Slam",
            Self::Fan => "Bone Fan",
            Self::Ring => "Hex Ring",
            Self::Charge => "Charge",
            Self::Spiral => "Spiral",
        }
    }
}

/// The crypt lord's attack pattern in `phase` (1..=3), cycled in order.
#[must_use]
pub const fn lord_patterns(phase: u8) -> &'static [CryptAttack] {
    match phase {
        0 | 1 => &[CryptAttack::Fan, CryptAttack::Ring],
        2 => &[
            CryptAttack::Fan,
            CryptAttack::Charge,
            CryptAttack::Ring,
            CryptAttack::Charge,
        ],
        _ => &[
            CryptAttack::Spiral,
            CryptAttack::Charge,
            CryptAttack::Fan,
            CryptAttack::Ring,
        ],
    }
}

/// The crypt lord's phase for its health share: 1 above two thirds, 2 above a third, then 3.
#[must_use]
pub fn lord_phase(health_fraction: f32) -> u8 {
    if health_fraction > 2.0 / 3.0 {
        1
    } else if health_fraction > 1.0 / 3.0 {
        2
    } else {
        3
    }
}

/// Where a cycling trap is in its cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TrapStage {
    /// Harmless.
    Idle,
    /// The telegraph: about to go off.
    Warning,
    /// Hurting (spikes up, flames out) or, for a chute, just released.
    Active,
}

/// One trap overlay of a crypt floor (absolute tile once in a run).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CryptTrap {
    pub at: TilePosition,
    pub kind: CryptTrapKind,
    /// Arrows fly and boulders roll this way (unit steps, +y down); zero for the rest.
    pub dir: (i8, i8),
    /// Offset into the cycle (milliseconds), so spike fields ripple and vents take turns.
    pub phase: u16,
    /// Shows only to trap sight (the Trap-Sight Lantern) until it goes off.
    pub hidden: bool,
    /// Index of its room in the floor's rooms.
    pub room: u8,
}

impl CryptTrap {
    /// The trap's stage `elapsed_millis` into its floor (cycling traps; the rest stay idle).
    #[must_use]
    pub const fn stage(&self, elapsed_millis: u64) -> TrapStage {
        let Some((period, warning, active)) = self.kind.cycle() else {
            return TrapStage::Idle;
        };
        let at = (elapsed_millis + self.phase as u64) % period;
        let fire = period - active;
        if at >= fire {
            TrapStage::Active
        } else if at >= fire - warning {
            TrapStage::Warning
        } else {
            TrapStage::Idle
        }
    }

    /// How far into the warning or active stage the trap is (0..=1; presentation).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "a few seconds in milliseconds")]
    pub fn progress(&self, elapsed_millis: u64) -> f32 {
        let Some((period, warning, active)) = self.kind.cycle() else {
            return 0.0;
        };
        let at = (elapsed_millis + u64::from(self.phase)) % period;
        let fire = period - active;
        if at >= fire {
            (at - fire) as f32 / active.max(1) as f32
        } else if at + warning >= fire {
            (at + warning - fire) as f32 / warning.max(1) as f32
        } else {
            0.0
        }
    }

    /// Whether a boulder chute releases a boulder between `before` and `after` milliseconds
    /// into the floor (the moment its warning ends).
    #[must_use]
    pub const fn releases(&self, before: u64, after: u64) -> bool {
        let Some((period, _, active)) = self.kind.cycle() else {
            return false;
        };
        if after <= before {
            return false;
        }
        let fire = period - active;
        let shift = period - fire;
        // Count cycle starts shifted so the release moment is a multiple of the period.
        (after + self.phase as u64 + shift) / period
            != (before + self.phase as u64 + shift) / period
    }
}

/// Run-time state of a crypt floor (the run's [`super::TombRun::crypt`]; empty on side-view
/// runs).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CryptState {
    /// The floor's traps (absolute tiles), in tile order.
    pub traps: Vec<CryptTrap>,
    /// When the floor was written: trap cycles count from here.
    pub started: SimulationTick,
    /// Cracked floors a pilot stepped on, and the tick they give way.
    pub crumbling: BTreeMap<TilePosition, SimulationTick>,
    /// Arrow plates pressed: the tick their arrows fly, and whether they have.
    pub plates: BTreeMap<TilePosition, (SimulationTick, bool)>,
}

impl CryptState {
    /// The trap overlay at `tile`, if any.
    #[must_use]
    pub fn trap_at(&self, tile: TilePosition) -> Option<&CryptTrap> {
        self.traps
            .binary_search_by(|trap| trap.at.cmp(&tile))
            .ok()
            .and_then(|index| self.traps.get(index))
    }
}

/// A crypt's tile set: the culture or era its builders were.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CryptTileset {
    /// Bone-niched catacomb stone (the deep strata and village old quarters).
    Catacomb,
    /// Wind-worn sandstone (mesas).
    Sandstone,
    /// Pale marble and brass under the sky continents.
    SkyVault,
    /// Black glassy stone and cold light (the Moon).
    LunarVault,
}

impl CryptTileset {
    pub const ALL: [Self; 4] = [
        Self::Catacomb,
        Self::Sandstone,
        Self::SkyVault,
        Self::LunarVault,
    ];

    /// The tile set of a site in `setting`.
    #[must_use]
    pub const fn of(setting: SiteSetting) -> Self {
        match setting {
            SiteSetting::Deep | SiteSetting::OldQuarter => Self::Catacomb,
            SiteSetting::Mesa => Self::Sandstone,
            SiteSetting::SkyContinent => Self::SkyVault,
            SiteSetting::Moon => Self::LunarVault,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Catacomb => "Catacomb",
            Self::Sandstone => "Sandstone",
            Self::SkyVault => "Sky Vault",
            Self::LunarVault => "Lunar Vault",
        }
    }
}

/// The crypt creature for a spawn slot (`variant` is a free roll).
///
/// Ambushes are mostly skeleton packs with spectres, cultists and now and then a golem; guards
/// are cultists, skeletons and golems; the boss is the crypt lord.
#[must_use]
pub const fn bestiary(role: SpawnRole, variant: u32) -> EnemyKind {
    let pick = variant % 20;
    match role {
        SpawnRole::Boss => EnemyKind::CryptLord,
        SpawnRole::Ambusher => match pick {
            0..=10 => EnemyKind::CryptSkeleton,
            11..=14 => EnemyKind::Spectre,
            15..=17 => EnemyKind::Cultist,
            _ => EnemyKind::CryptGolem,
        },
        SpawnRole::Guard => match pick {
            0..=7 => EnemyKind::CryptSkeleton,
            8..=15 => EnemyKind::Cultist,
            _ => EnemyKind::CryptGolem,
        },
    }
}

/// Trap damage scale for a tier-`tier` site (traps bite harder deeper down).
#[must_use]
pub fn trap_tier_scale(tier: u8) -> f32 {
    f32::from(tier.saturating_sub(1)).mul_add(0.15, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trap(kind: CryptTrapKind, phase: u16) -> CryptTrap {
        CryptTrap {
            at: TilePosition { x: 0, y: 0 },
            kind,
            dir: (0, 0),
            phase,
            hidden: false,
            room: 0,
        }
    }

    #[test]
    fn cycling_traps_warn_before_they_bite() {
        for kind in [CryptTrapKind::Spikes, CryptTrapKind::FlameJet] {
            let (period, warning, active) = kind.cycle().expect("cycles");
            let trap = trap(kind, 0);
            let mut last = TrapStage::Idle;
            for millis in 0..period * 2 {
                let stage = trap.stage(millis);
                if stage == TrapStage::Active && last != TrapStage::Active {
                    // Every activation comes straight out of a full warning.
                    assert_eq!(last, TrapStage::Warning, "{kind:?} at {millis}");
                    for back in 1..=warning {
                        assert_eq!(trap.stage(millis - back), TrapStage::Warning);
                    }
                }
                last = stage;
            }
            let active_millis = (0..period)
                .filter(|millis| trap.stage(*millis) == TrapStage::Active)
                .count();
            assert_eq!(active_millis as u64, active);
            // A phase shifts the cycle.
            let shifted = super::CryptTrap { phase: 500, ..trap };
            assert_eq!(shifted.stage(0), trap.stage(500));
        }
    }

    #[test]
    fn a_chute_releases_once_a_cycle_as_its_warning_ends() {
        let chute = trap(CryptTrapKind::BoulderChute, 300);
        let (period, _, _) = chute.kind.cycle().expect("cycles");
        let releases: Vec<u64> = (0..period * 3)
            .filter(|millis| chute.releases(*millis, millis + 1))
            .collect();
        assert_eq!(releases.len(), 3, "{releases:?}");
        for at in releases {
            assert_eq!(chute.stage(at), TrapStage::Warning);
            assert_eq!(chute.stage(at + 1), TrapStage::Active);
        }
        // Coarse ticks see the same releases.
        let coarse = (0..period * 3 / 33)
            .filter(|tick| chute.releases(tick * 33, (tick + 1) * 33))
            .count();
        assert_eq!(coarse, 3);
    }

    #[test]
    fn every_setting_has_a_tileset_and_bestiary_rolls_cover_the_crypt_kinds() {
        let sets: std::collections::BTreeSet<CryptTileset> = [
            SiteSetting::Deep,
            SiteSetting::Mesa,
            SiteSetting::OldQuarter,
            SiteSetting::SkyContinent,
            SiteSetting::Moon,
        ]
        .into_iter()
        .map(CryptTileset::of)
        .collect();
        assert_eq!(sets.len(), CryptTileset::ALL.len());
        let kinds: std::collections::BTreeSet<EnemyKind> = (0..40)
            .flat_map(|roll| {
                [
                    bestiary(SpawnRole::Ambusher, roll),
                    bestiary(SpawnRole::Guard, roll),
                ]
            })
            .collect();
        for kind in [
            EnemyKind::CryptSkeleton,
            EnemyKind::Spectre,
            EnemyKind::Cultist,
            EnemyKind::CryptGolem,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
            assert!(kind.is_crypt());
        }
        assert_eq!(bestiary(SpawnRole::Boss, 7), EnemyKind::CryptLord);
    }
}
