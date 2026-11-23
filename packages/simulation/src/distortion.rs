//! The distortion strata (expansion plan §14, Descent VI part A; Wave 9): the first three
//! Strange Layers, where reality thins near the builders' machine.
//!
//! Pure data and math; the tick lives in `world/distortion.rs`, the creatures' behaviour in
//! `world/enemies/distortion.rs`, the generated places in
//! [`crate::worldgen::descent::distortion`] and the field-effect wiring in [`fields`].
//!
//! | Stratum | Signature rule (gentle, then pushed) | Natives | Landmark | Resource |
//! |---|---|---|---|---|
//! | Mirror Strata | **mirror halls** ([`mirror`]): symmetric halls where the screen and the controls mirror, framed by a shimmering silver boundary; few and narrow at the top, wide and close together at the bottom. **Mirror twins** step out of the glass and copy a rig's moves and shots mirrored; **mirror ore** sits in pairs, one real and one reflection | mirror twin, glassback (its mirrored face throws shots back), gazer (its gaze mirrors a rig's controls) | the Looking-Glass Hall; Parallax Station (the Descent VI outpost, a research station) | Mirror Ore |
//! | Gravity Shear | **shear halls** ([`shear`]): gravity turns 90 degrees (walls) at the top, 180 (ceilings) further down and every way at the bottom, with flow lines on screen; the rig turns with its rotation thrusters and walks the walls | tumbler (rolls downhill, whichever way that is), lodestar (pulls rigs in), shear mantis (hangs from a ceiling, drops along gravity) | the Shear Spire: a hollow tower where down is always the nearest wall | Shear Stone |
//! | Pixel Rot | **rot tiers** ([`rot`]): the deeper, the lower the resolution: five tiers of coarser pixels, fewer colours and glitching sprites, and the rock itself is quantized into ever bigger blocks; whole **data ore** blocks pay a data core | glitchling (teleports between pixels), bitworm (runs the grid in straight lines), dead pixel (pulses of rot) | the Dead Screen: a builder picture drawn in giant pixels | Data Ore |
//!
//! State: [`Distortion`] (shared, saved, replicated as the appended `Distortion` section and
//! hashed once anything happened): which rigs a gazer holds and until when, rigs caught in a
//! dead pixel's rot pulse, the mirror halls' twin cooldowns and the landmarks the crew found.

pub mod fields;
pub mod mirror;
pub mod rot;
pub mod shear;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    descent::{Descent, Stratum},
    ids::{PlayerId, SimulationTick},
    item::{BlockKind, ItemKind},
    worldgen::WorldSettings,
};

/// Kind-code range of [`crate::terrain::TileKind::Warped`] tiles in
/// [`crate::terrain::Tile::code`] (`0x1600 | WarpedTile::code`), clear of every other family.
pub const WARPED_TILE_CODE_BASE: u64 = 0x1600;

/// A tile of the distortion strata ([`crate::terrain::TileKind::Warped`], V11 worlds only).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum WarpedTile {
    // --- Mirror Strata ---
    /// Silvered glass: it lines the mirror halls and dots their frames in the rock.
    MirrorGlass,
    /// Mirror ore: the real one of a pair (drill it for the layer resource and mythril).
    MirrorOre,
    /// A reflection of mirror ore: it looks the same at a glance, but it is glass and
    /// shatters (a small hull nick) for nothing.
    MirrorGhost,
    // --- Gravity Shear ---
    /// Flow stone: it lines the shear halls' frames (the renderer draws the field's flow on it).
    ShearStone,
    /// A shear plate: a walkable shelf on a shear hall's turned floor.
    ShearPlate,
    // --- Pixel Rot ---
    /// Data ore: blocks of it lie on the rot grid; a whole block mined pays a data core.
    DataOre,
    /// A rotten block: rock quantized into a flat, glitching square.
    RotBlock,
    /// A lit pixel of the Dead Screen's builder picture.
    ScreenPixel,
}

impl WarpedTile {
    /// Every warped tile, in code order.
    pub const ALL: [Self; 8] = [
        Self::MirrorGlass,
        Self::MirrorOre,
        Self::MirrorGhost,
        Self::ShearStone,
        Self::ShearPlate,
        Self::DataOre,
        Self::RotBlock,
        Self::ScreenPixel,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MirrorGlass => "mirror glass",
            // A reflection is named like the real thing: telling them apart is the game.
            Self::MirrorOre | Self::MirrorGhost => "mirror ore",
            Self::ShearStone => "flow stone",
            Self::ShearPlate => "shear plate",
            Self::DataOre => "data ore",
            Self::RotBlock => "rotten block",
            Self::ScreenPixel => "screen pixel",
        }
    }

    /// Stable code (part of [`crate::terrain::Tile::code`]). Never renumber.
    #[must_use]
    pub const fn code(self) -> u64 {
        self as u64
    }

    /// The tile with code `code`, if any.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        if code < Self::ALL.len() as u64 {
            #[allow(clippy::cast_possible_truncation, reason = "checked above")]
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// Drill strength needed (Descent VI wants the deep drills; glass gives easily).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        match self {
            Self::MirrorGhost => 2,
            Self::MirrorGlass | Self::DataOre => 4,
            Self::MirrorOre | Self::ShearPlate | Self::RotBlock | Self::ScreenPixel => 5,
            Self::ShearStone => 6,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    pub const fn durability(self) -> u8 {
        match self {
            Self::MirrorGhost => 1,
            Self::MirrorGlass => 6,
            Self::DataOre | Self::MirrorOre => 9,
            Self::ShearPlate | Self::ScreenPixel => 10,
            Self::RotBlock => 11,
            Self::ShearStone => 14,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::MirrorGhost => 0.05,
            Self::MirrorGlass => 0.12,
            Self::DataOre | Self::MirrorOre | Self::ScreenPixel => 0.16,
            Self::ShearPlate | Self::RotBlock => 0.18,
            Self::ShearStone => 0.22,
        }
    }

    /// The item a broken tile yields (the layer resources; glass and reflections give none).
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::MirrorOre => ItemKind::Layer(Stratum::MirrorStrata),
            Self::ShearStone | Self::ShearPlate => ItemKind::Layer(Stratum::GravityShear),
            Self::DataOre | Self::ScreenPixel => ItemKind::Layer(Stratum::PixelRot),
            Self::RotBlock => ItemKind::Block(BlockKind::HardRock),
            Self::MirrorGlass | Self::MirrorGhost => return None,
        })
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::ScreenPixel => 200,
            Self::DataOre => 120,
            Self::MirrorOre | Self::MirrorGhost => 90,
            Self::MirrorGlass => 50,
            Self::ShearStone | Self::ShearPlate | Self::RotBlock => 0,
        }
    }
}

// --- Mirror twins, gazers and rot pulses --------------------------------------------------------

/// A mirror hall spawns a rig's twin once the rig is this far into it (field weight).
pub const TWIN_SPAWN_WEIGHT: f32 = 0.6;
/// ... and at least this many tiles from the hall's axis (a twin needs room to stand).
pub const TWIN_AXIS_CLEARANCE_TILES: f32 = 3.0;
/// A hall whose twin fell rests this long before another steps out.
pub const TWIN_COOLDOWN_MILLIS: u64 = 45_000;
/// A twin follows its rig out of the hall no farther than this many tiles, then fades.
pub const TWIN_LEASH_TILES: f32 = 40.0;
/// A gazer's gaze holds a rig's controls mirrored this long past its last look.
pub const GAZE_HOLD_MILLIS: u64 = 450;
/// A dead pixel's rot pulse rots a rig's view this long.
pub const ROT_BURST_MILLIS: u64 = 2_500;
/// Most hall cooldowns remembered (the oldest drop first).
pub const MAX_TWIN_COOLDOWNS: usize = 64;

/// A mined data ore block pays this many extra layer resources...
pub const DATA_CORE_RESOURCES: u32 = 3;
/// ... and this many mythril.
pub const DATA_CORE_MYTHRIL: u32 = 1;
/// A real mirror ore pays this much mythril besides its layer resource.
pub const MIRROR_ORE_MYTHRIL: u32 = 1;
/// Hull a shattered reflection nicks (scaled by the depth's creature scale).
pub const GHOST_SHATTER_DAMAGE: f32 = 1.2;

/// A landmark of the distortion strata the crew can find (story beats).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum DistortionLandmark {
    /// The Looking-Glass Hall (Mirror Strata).
    LookingGlassHall,
    /// Parallax Station, the Descent VI outpost (Mirror Strata).
    ParallaxStation,
    /// The Shear Spire (Gravity Shear).
    ShearSpire,
    /// The Dead Screen (Pixel Rot).
    DeadScreen,
}

impl DistortionLandmark {
    pub const ALL: [Self; 4] = [
        Self::LookingGlassHall,
        Self::ParallaxStation,
        Self::ShearSpire,
        Self::DeadScreen,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LookingGlassHall => "The Looking-Glass Hall",
            Self::ParallaxStation => "Parallax Station",
            Self::ShearSpire => "The Shear Spire",
            Self::DeadScreen => "The Dead Screen",
        }
    }

    /// The stratum it stands in.
    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::LookingGlassHall | Self::ParallaxStation => Stratum::MirrorStrata,
            Self::ShearSpire => Stratum::GravityShear,
            Self::DeadScreen => Stratum::PixelRot,
        }
    }

    /// The story beat the place tells when the crew first finds it.
    #[must_use]
    pub const fn story(self) -> &'static str {
        match self {
            Self::LookingGlassHall => {
                "The hall's two halves are the same to the last grain. Scratched into the \
                 left wall, in the Fathers' hand: TWO OF US CAME IN. Scratched into the right \
                 wall, the same words, backwards."
            }
            Self::ParallaxStation => {
                "The station's researchers keep two logbooks of the Fathers' visit, one in \
                 each wing. They agree on everything but the last line: in one the Fathers \
                 went down, in the other they went back up."
            }
            Self::ShearSpire => {
                "Down is not a fact here. It is a setting, and something below keeps \
                 changing it: the Spire's walls are worn smooth on all four sides."
            }
            Self::DeadScreen => {
                "A builder picture, drawn in pixels the size of rooms: a cable from the hanging \
                 stone to a sleeping star. The deeper the rot, the fewer pixels it can spare. \
                 The machine is running out of power to draw the world."
            }
        }
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// Something of the distortion strata happened (netcode codes `560 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DistortionEvent {
    /// A rig crossed into a mirror hall (the screen and the controls flip).
    MirrorEntered,
    /// A rig left a mirror hall.
    MirrorLeft,
    /// A mirror twin stepped out of the glass.
    TwinAppeared,
    /// A mirror twin fired its rig's shot back.
    TwinFired,
    /// A mirror twin broke.
    TwinBroken,
    /// A mirror twin faded back into the glass.
    TwinFaded,
    /// Real mirror ore was mined.
    MirrorOreMined,
    /// A reflection shattered under the drill.
    GhostShattered,
    /// A glassback's face threw a shot back.
    ShotReflected,
    /// A gazer caught a rig in its gaze (its controls mirror).
    Gazed,
    /// A rig entered a turned shear field.
    ShearEntered,
    /// A lodestar began to pull a rig.
    LodestarPull,
    /// A shear mantis dropped.
    MantisDropped,
    /// A rig crossed into a deeper rot tier.
    RotDeepened,
    /// A glitchling teleported.
    GlitchTeleport,
    /// A whole data ore block was mined: a data core.
    DataCore,
    /// A dead pixel's pulse rotted a rig's view.
    RotBurst,
    /// The crew found a landmark (its story beat).
    LandmarkFound,
}

impl DistortionEvent {
    /// Offset of the event's netcode code from 560.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// The distortion strata's shared state (see the module docs).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Distortion {
    /// Rigs a gazer holds, until the tick their controls stop mirroring.
    #[serde(default)]
    pub gazed: BTreeMap<PlayerId, SimulationTick>,
    /// Rigs caught in a dead pixel's rot pulse, until the tick their view clears.
    #[serde(default)]
    pub rotted: BTreeMap<PlayerId, SimulationTick>,
    /// Mirror halls (keyed by their bounds' top-left tile) resting after a twin fell, until a
    /// tick. Saved as pairs (JSON keys must be strings).
    #[serde(default, with = "crate::pair_list")]
    pub twin_cooldowns: BTreeMap<(i32, i32), SimulationTick>,
    /// Landmarks the crew found ([`DistortionLandmark`] bits).
    #[serde(default)]
    pub found: u8,
    /// The deepest rot tier each rig has seen, plus one (for the "deeper" cue; absent above
    /// Pixel Rot).
    #[serde(default)]
    pub rot_tiers: BTreeMap<PlayerId, u8>,
    /// Rigs inside a mirror hall now (for the cues in and out).
    #[serde(default)]
    pub in_mirror: BTreeSet<PlayerId>,
    /// Rigs inside a turned shear field now (for the cue in).
    #[serde(default)]
    pub in_shear: BTreeSet<PlayerId>,
}

impl Distortion {
    /// Whether nothing happened yet (older worlds keep their hashes).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gazed.is_empty()
            && self.rotted.is_empty()
            && self.twin_cooldowns.is_empty()
            && self.found == 0
            && self.rot_tiers.is_empty()
            && self.in_mirror.is_empty()
            && self.in_shear.is_empty()
    }

    /// Whether the crew found `landmark`.
    #[must_use]
    pub const fn has_found(&self, landmark: DistortionLandmark) -> bool {
        self.found & landmark.bit() != 0
    }

    /// Marks `landmark` found; whether it was new.
    pub const fn find(&mut self, landmark: DistortionLandmark) -> bool {
        let new = !self.has_found(landmark);
        self.found |= landmark.bit();
        new
    }

    /// Whether a gazer holds `player`'s controls at `now`.
    #[must_use]
    pub fn gazed_at(&self, player: PlayerId, now: SimulationTick) -> bool {
        self.gazed.get(&player).is_some_and(|until| *until > now)
    }

    /// Whether `player`'s view is rotted by a pulse at `now`.
    #[must_use]
    pub fn rotted_at(&self, player: PlayerId, now: SimulationTick) -> bool {
        self.rotted.get(&player).is_some_and(|until| *until > now)
    }

    /// Whether the hall keyed `key` rests at `now`.
    #[must_use]
    pub fn hall_resting(&self, key: (i32, i32), now: SimulationTick) -> bool {
        self.twin_cooldowns
            .get(&key)
            .is_some_and(|until| *until > now)
    }

    /// Forgets `player` (it left the world).
    pub fn forget(&mut self, player: PlayerId) {
        self.gazed.remove(&player);
        self.rotted.remove(&player);
        self.rot_tiers.remove(&player);
        self.in_mirror.remove(&player);
        self.in_shear.remove(&player);
    }

    /// Drops every expired hold, pulse and cooldown.
    pub fn prune(&mut self, now: SimulationTick) {
        self.gazed.retain(|_, until| *until > now);
        self.rotted.retain(|_, until| *until > now);
        self.twin_cooldowns.retain(|_, until| *until > now);
        while self.twin_cooldowns.len() > MAX_TWIN_COOLDOWNS {
            let _ = self.twin_cooldowns.pop_first();
        }
    }
}

/// Whether the world has the distortion strata (a Deep Descent world).
#[must_use]
pub const fn has_distortion(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// Whether `stratum` is one of the distortion strata.
#[must_use]
pub const fn is_distortion(stratum: Stratum) -> bool {
    matches!(
        stratum,
        Stratum::MirrorStrata | Stratum::GravityShear | Stratum::PixelRot
    )
}

/// The rows (layout, `(first, last)`) the distortion strata span.
#[must_use]
pub const fn distortion_rows(layout: &crate::worldgen::descent::DescentLayout) -> (i32, i32) {
    (
        layout.rows(Stratum::MirrorStrata).0,
        layout.rows(Stratum::PixelRot).1,
    )
}

/// The Descent the distortion strata belong to.
pub const DESCENT: Descent = Descent::StrangeLayers;

/// The authored content of a distortion stratum the descent probe counts, `(mechanics, set
/// pieces)` (like `old_deep::authored_content`).
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8) {
    match stratum {
        // Mirror halls, twins and ore pairs; the Looking-Glass Hall and Parallax Station.
        Stratum::MirrorStrata => (3, 2),
        // Shear halls and the lodestars' pull; the Shear Spire. Rot tiers and data cores; the
        // Dead Screen.
        Stratum::GravityShear | Stratum::PixelRot => (2, 1),
        _ => (0, 0),
    }
}

/// The new things a distortion stratum shows a player beyond its registry entry, in the order
/// they come (the descent probe's markers): `(what, is it a place)`.
#[must_use]
pub const fn novelties(stratum: Stratum) -> &'static [(&'static str, bool)] {
    match stratum {
        Stratum::MirrorStrata => &[
            ("mirror halls flip the controls", false),
            ("mirror ore and its reflection", false),
            ("Parallax Station", true),
            ("mirror twins", false),
            ("the Looking-Glass Hall", true),
        ],
        Stratum::GravityShear => &[
            ("walls become floors", false),
            ("ceilings become floors", false),
            ("the Shear Spire", true),
        ],
        Stratum::PixelRot => &[
            ("the rock quantizes", false),
            ("data cores", false),
            ("the Dead Screen", true),
            ("the rot deepens", false),
        ],
        _ => &[],
    }
}

/// The name of the Descent VI outpost and the research station around it.
pub const PARALLAX_STATION_NAME: &str = "Parallax Station";
/// The village site Parallax Station's settlement is filed under: past every site a world
/// places ([`crate::village::placement::MAX_SITE`]), so no generated village shares it.
pub const PARALLAX_SITE: i32 = crate::village::placement::MAX_SITE + 6;

/// Parallax Station's settlement hall: its centre column and first air row.
///
/// A research station of the Strange Layers (the people who study the mirror halls) beside the
/// Descent VI outpost on the spawn side, sharing its floor, like Gravel Hollow beside the
/// first. `None` before V11.
#[must_use]
pub fn parallax_station(settings: &WorldSettings) -> Option<(i32, i32)> {
    use crate::village::placement::{HALL_AIR_ROWS, HALL_WIDTH};
    use crate::worldgen::descent::landmarks::{OUTPOST_SIZE, outpost, outpost_column};
    if !has_distortion(settings) {
        return None;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    let hall = outpost(settings, &layout, DESCENT)?;
    let floor = hall.bounds.max.y;
    let column = outpost_column(settings);
    let side = if column >= crate::worldgen::SPAWN_TILE_X {
        -1
    } else {
        1
    };
    let center = column + side * (OUTPOST_SIZE.0 / 2 + 4 + HALL_WIDTH / 2);
    Some((center, floor - HALL_AIR_ROWS))
}

/// Whether the generator's distortion passes may carve over `kind` (built pieces, points of
/// interest, bedrock, fluid and the Descent landmarks' builder stone stay).
#[must_use]
pub const fn carvable(kind: crate::terrain::TileKind) -> bool {
    use crate::{cave::CaveTile, terrain::TileKind};
    !matches!(
        kind,
        TileKind::Built(_)
            | TileKind::Poi(_)
            | TileKind::Bedrock
            | TileKind::Fluid(_)
            | TileKind::Cave(
                CaveTile::Crystal
                    | CaveTile::RuinBrick
                    | CaveTile::RuinPillar
                    | CaveTile::RuinGlyph
                    | CaveTile::TreasureCache
                    | CaveTile::RelicCache
            )
    )
}

/// Whether `kind` is rock (plain rock or ore) a frame's dots may replace.
#[must_use]
pub const fn is_rock(kind: crate::terrain::TileKind) -> bool {
    use crate::terrain::TileKind;
    matches!(
        kind,
        TileKind::Dirt | TileKind::Clay | TileKind::Stone | TileKind::HardRock | TileKind::Ore(_)
    )
}

#[cfg(test)]
mod tests;
