//! Descent VIII, the Heart (Wave 10, expansion plan §14): the last chapter of the Descent.
//!
//! The planet's core is a sleeping **Star Drive**, and the builders' cable runs from it to the
//! hanging stone we call the Moon.
//!
//! Pure data and math; the tick lives in `world/heart.rs`, the creatures' ways and the Heart's
//! fight in `world/enemies/heart.rs`, the generated places in
//! [`crate::worldgen::descent::heart`], the gravity wells' field in [`fields`] and the ending's
//! state machine in [`ending`]. Classic worlds never see any of it: every tile here is
//! generated only by V11 and every rule reads only those tiles or the V11 layout.
//!
//! | Stratum | Signature rule (introduced gently, then pushed) | Counter-play |
//! |---|---|---|
//! | Dark Matter Veins | **negative-mass ore**: every lump of dark matter in the bay makes the rig lighter ([`cargo_mass_permille`]); carry too much and it falls **up** onto the ceilings; ordinary ore weighs it back down | plan the load: mix dark matter with heavy ore, turn the rig over (free rotation once it is light) and thrust "down" |
//! | Singularity Fields | **gravity wells** pull rigs, drops and creatures in ([`fields`]) and bend the light on screen (lensing); the horizon at a well's heart crushes hulls ([`HORIZON_TILES`]) | read the lens, keep thrust in hand, slingshot round the rim |
//! | Core Ocean | a **liquid metal sea** under a rock sky, crossed on **magnetic rails** ([`MAG_RAIL`]): fast rides with junction switches (hold down at a switch for the low line), boosters, and a magnetic catch that grabs a rig landing on a rail | ride the rails, switch to the low line for the ore islands, never touch the metal |
//! | The Heart | the **sleeping Star Drive** in its chamber at the cable's root: the final fight ([`HeartPhase`]) and the ending; the Heart Key opens its seal | break the drive coils, dodge the beams, strike while it exhales |
//!
//! The Heart's reward is the **Star Drive** ([`Heart::star_drive`], per player, every rig of the
//! crew, later joiners too): the cable to the Moon lights up, space past the Moon opens
//! ([`Heart::space_unlocked`], read through [`SpaceAccess`] by Wave 11's space flight) and in
//! the sky the drive gives a big thrust boost ([`STAR_DRIVE_SKY_BOOST`]). The game keeps
//! going after the ending (expansion plan §18 decision 5).

pub mod ending;
pub mod fields;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    descent::{Descent, Stratum},
    ids::{PlayerId, SimulationTick},
    item::{BlockKind, ItemKind},
    terrain::{MineralKind, TilePosition},
    worldgen::{TileRect, WorldSettings},
};

// ---------------------------------------------------------------------------------------------
// Tiles.
// ---------------------------------------------------------------------------------------------

/// Kind-code range of [`crate::terrain::TileKind::Core`] tiles in
/// [`crate::terrain::Tile::code`] (`0x1A00 | CoreTile::code`), clear of every other family.
pub const CORE_TILE_CODE_BASE: u64 = 0x1A00;

/// A tile of the Heart's strata ([`crate::terrain::TileKind::Core`], V11 worlds only).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum CoreTile {
    // --- Dark Matter Veins ---
    /// Violet shale shot through with dark seams: the veins' rock.
    VeinShale,
    /// Black null stone: the veins' hard rock.
    NullStone,
    /// Dark matter: negative-mass ore (the stratum's layer resource). It hangs from ceilings.
    DarkMatter,
    // --- Singularity Fields ---
    /// Banded event shale, folded round the wells.
    EventShale,
    /// Horizon rock: dense, glassy and hard.
    HorizonRock,
    /// Event glass: rings of it crystallise at a well's rim (the layer resource).
    EventGlass,
    /// A singularity's point: the heart of a gravity well. Nothing breaks it.
    Singularity,
    // --- Core Ocean ---
    /// Iron-rich core basalt: the ocean's rock.
    CoreBasalt,
    /// Iron crust: the ocean's hard rock.
    IronCrust,
    /// Core metal ore on the ocean's islands (the layer resource).
    CoreMetal,
    /// A magnetic rail: a rig riding it floats along at speed ([`MAG_RAIL`]).
    MagRail,
    /// A magnetic rail at a junction: hold down to take the low line.
    MagSwitch,
    /// A booster rail: it throws a riding rig up to the rails' top speed.
    MagBooster,
    /// A rail pylon standing in the metal sea. Nothing breaks it.
    RailPylon,
    /// A plate of the builders' ferry terminal.
    TerminalPlate,
    // --- The Heart ---
    /// Star rock: bright, crystalline, warm.
    StarRock,
    /// Drive alloy: the builders' hard plating round the Heart.
    DriveAlloy,
    /// A star shard (the Heart's layer resource): a splinter of the drive's light.
    StarShard,
    /// The Heart chamber's shell. Nothing breaks it.
    HeartShell,
    /// The Heart's seal: only a rig with the Heart Key drills it.
    HeartSeal,
    /// The Star Drive's body in the middle of the chamber. Nothing breaks it.
    StarCore,
    /// The builders' cable, running from the Heart up toward the Moon. Nothing breaks it.
    Cable,
}

impl CoreTile {
    /// Every Heart tile, in code order.
    pub const ALL: [Self; 22] = [
        Self::VeinShale,
        Self::NullStone,
        Self::DarkMatter,
        Self::EventShale,
        Self::HorizonRock,
        Self::EventGlass,
        Self::Singularity,
        Self::CoreBasalt,
        Self::IronCrust,
        Self::CoreMetal,
        Self::MagRail,
        Self::MagSwitch,
        Self::MagBooster,
        Self::RailPylon,
        Self::TerminalPlate,
        Self::StarRock,
        Self::DriveAlloy,
        Self::StarShard,
        Self::HeartShell,
        Self::HeartSeal,
        Self::StarCore,
        Self::Cable,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::VeinShale => "vein shale",
            Self::NullStone => "null stone",
            Self::DarkMatter => "dark matter",
            Self::EventShale => "event shale",
            Self::HorizonRock => "horizon rock",
            Self::EventGlass => "event glass",
            Self::Singularity => "a singularity",
            Self::CoreBasalt => "core basalt",
            Self::IronCrust => "iron crust",
            Self::CoreMetal => "core metal",
            Self::MagRail => "magnetic rail",
            Self::MagSwitch => "rail switch",
            Self::MagBooster => "booster rail",
            Self::RailPylon => "rail pylon",
            Self::TerminalPlate => "terminal plate",
            Self::StarRock => "star rock",
            Self::DriveAlloy => "drive alloy",
            Self::StarShard => "star shard",
            Self::HeartShell => "the Heart's shell",
            Self::HeartSeal => "the Heart's seal",
            Self::StarCore => "the Star Drive",
            Self::Cable => "the builders' cable",
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

    /// Whether no drill, blast or pick ever breaks it (the wells' points, the pylons, the
    /// Heart's shell and body, the cable).
    #[must_use]
    pub const fn indestructible(self) -> bool {
        matches!(
            self,
            Self::Singularity | Self::RailPylon | Self::HeartShell | Self::StarCore | Self::Cable
        )
    }

    /// Drill strength needed to dig it (Descent VIII wants the top drills).
    #[must_use]
    pub const fn hardness(self) -> u8 {
        if self.indestructible() {
            return crate::terrain::UNBREAKABLE_HARDNESS;
        }
        match self {
            Self::MagRail | Self::MagSwitch | Self::MagBooster | Self::DarkMatter => 3,
            Self::VeinShale
            | Self::EventShale
            | Self::CoreBasalt
            | Self::StarRock
            | Self::EventGlass
            | Self::CoreMetal
            | Self::StarShard
            | Self::TerminalPlate => 4,
            _ => 5,
        }
    }

    /// Chips a fresh tile takes.
    #[must_use]
    #[allow(clippy::match_same_arms, reason = "one row per toughness")]
    pub const fn durability(self) -> u8 {
        if self.indestructible() {
            return u8::MAX;
        }
        match self {
            Self::MagRail | Self::MagSwitch | Self::MagBooster => 6,
            Self::DarkMatter | Self::EventGlass => 8,
            Self::VeinShale | Self::EventShale | Self::CoreBasalt | Self::StarRock => 10,
            Self::CoreMetal | Self::StarShard | Self::TerminalPlate => 11,
            Self::NullStone | Self::HorizonRock | Self::IronCrust => 14,
            Self::DriveAlloy => 16,
            Self::HeartSeal => 18,
            _ => 12,
        }
    }

    /// Seconds per chip at the stock drill before drill bonuses.
    #[must_use]
    pub const fn base_chip_seconds(self) -> f32 {
        match self {
            Self::MagRail | Self::MagSwitch | Self::MagBooster | Self::DarkMatter => 0.12,
            Self::EventGlass | Self::StarShard | Self::CoreMetal => 0.16,
            Self::VeinShale | Self::EventShale | Self::CoreBasalt | Self::StarRock => 0.18,
            Self::HeartSeal | Self::DriveAlloy => 0.26,
            _ => 0.22,
        }
    }

    /// The item a broken tile yields: the layer resources and blocks.
    #[must_use]
    pub const fn item(self) -> Option<ItemKind> {
        Some(match self {
            Self::DarkMatter => ItemKind::Layer(Stratum::DarkMatterVeins),
            Self::EventGlass => ItemKind::Layer(Stratum::SingularityFields),
            Self::CoreMetal => ItemKind::Layer(Stratum::CoreOcean),
            Self::StarShard => ItemKind::Layer(Stratum::TheHeart),
            Self::VeinShale
            | Self::NullStone
            | Self::EventShale
            | Self::HorizonRock
            | Self::CoreBasalt
            | Self::IronCrust
            | Self::StarRock => ItemKind::Block(BlockKind::HardRock),
            _ => return None,
        })
    }

    /// Light it gives off (0-255, the renderer's light map).
    #[must_use]
    pub const fn glow(self) -> u8 {
        match self {
            Self::StarCore => 255,
            Self::StarShard | Self::Cable => 200,
            Self::MagBooster | Self::HeartSeal => 170,
            Self::DarkMatter | Self::EventGlass => 130,
            Self::MagRail | Self::MagSwitch | Self::CoreMetal => 90,
            Self::StarRock => 60,
            _ => 0,
        }
    }

    /// Whether it is plain rock to the deposit, cave and hazard passes.
    #[must_use]
    pub const fn plain_rock(self) -> bool {
        matches!(
            self,
            Self::VeinShale
                | Self::NullStone
                | Self::EventShale
                | Self::HorizonRock
                | Self::CoreBasalt
                | Self::IronCrust
                | Self::StarRock
                | Self::DriveAlloy
        )
    }

    /// Whether a mag-rail ride rolls on it.
    #[must_use]
    pub const fn is_mag_rail(self) -> bool {
        matches!(self, Self::MagRail | Self::MagSwitch | Self::MagBooster)
    }
}

// ---------------------------------------------------------------------------------------------
// Dark matter: inverted cargo physics.
// ---------------------------------------------------------------------------------------------

/// Weight (per mille of the rig's own) each lump of dark matter in the bay takes away.
pub const DARK_LIFT_PERMILLE: i32 = 125;
/// Weight each unit of ordinary cargo adds back once dark matter is aboard (a rig with no dark
/// matter keeps the classic physics exactly).
pub const CARGO_WEIGHT_PERMILLE: i32 = 25;
/// The lightest a rig gets: it falls up at most this hard (per mille of standard gravity).
pub const MASS_MIN_PERMILLE: i32 = -850;
/// The heaviest dark-matter-laden rig (lots of ore, a little dark matter).
pub const MASS_MAX_PERMILLE: i32 = 1_600;
/// Below this weight the rig turns freely (the player rolls it over with the thrusters to
/// thrust "down"; nothing turns it for them).
pub const FREE_TURN_BELOW_PERMILLE: i32 = 250;

/// The rig's weight, per mille of its own, carrying `dark` lumps of dark matter and `other`
/// units of ordinary cargo.
///
/// 1000 with no dark matter (the classic physics), lighter with every lump, negative (it falls
/// up) past about eight lumps with an empty bay.
#[must_use]
pub const fn cargo_mass_permille(dark: u32, other: u32) -> i32 {
    if dark == 0 {
        return 1_000;
    }
    let dark = (if dark > 64 { 64 } else { dark }).cast_signed();
    let other = (if other > 200 { 200 } else { other }).cast_signed();
    let mass = 1_000 - dark * DARK_LIFT_PERMILLE + other * CARGO_WEIGHT_PERMILLE;
    if mass < MASS_MIN_PERMILLE {
        MASS_MIN_PERMILLE
    } else if mass > MASS_MAX_PERMILLE {
        MASS_MAX_PERMILLE
    } else {
        mass
    }
}

/// Lumps of dark matter that make an otherwise empty rig weightless.
pub const WEIGHTLESS_LUMPS: u32 = (1_000 / DARK_LIFT_PERMILLE) as u32;

// ---------------------------------------------------------------------------------------------
// Gravity wells.
// ---------------------------------------------------------------------------------------------

/// A well's pull at its point, standard gravities (a top-tier engine climbs out of it; a stock
/// one would not).
pub const WELL_PULL_G: f32 = 1.85;
/// Tiles past a well's open cavern that its pull still reaches (faint at the edge).
pub const WELL_REACH_TILES: i32 = 9;
/// Within this many tiles of a well's point the horizon crushes a hull.
pub const HORIZON_TILES: f32 = 2.6;
/// Hull a second the horizon takes (scaled by the depth's creature scale like other hazards).
pub const HORIZON_DAMAGE_PER_SECOND: f32 = 14.0;
/// Pull share past which a rig counts as caught in a well (the cue and the hint).
pub const WELL_CAUGHT_SHARE: f32 = 0.45;

// ---------------------------------------------------------------------------------------------
// Magnetic rails.
// ---------------------------------------------------------------------------------------------

/// The mag rails' ride tuning (the Crust's carts with magnets: fast, little friction, little
/// pull on ramps).
pub const MAG_RAIL: crate::crust::CartTuning = crate::crust::CartTuning {
    top_speed: 19_000,
    push: 6_500,
    brake: 9_000,
    friction: 120,
    slope: 1_200,
};
/// Speed a booster rail throws a riding rig up to (thousandths of a tile a second).
pub const BOOSTER_SPEED: i32 = 17_500;
/// The magnetic catch: a rig moving at least this fast sideways (tiles a second) that touches
/// down on a mag rail is grabbed onto it.
pub const CATCH_SPEED_TILES: f32 = 2.5;

// ---------------------------------------------------------------------------------------------
// The Star Drive.
// ---------------------------------------------------------------------------------------------

/// The Star Drive's thrust boost in the sky (over the ground line): its engine's push and its
/// climb cap multiply by this.
pub const STAR_DRIVE_SKY_BOOST: f32 = 2.2;

/// What Wave 11's space flight reads (see [`crate::WorldState::space_access`]).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpaceAccess {
    /// Whether the Heart woke and space past the Moon is open (world-wide).
    pub unlocked: bool,
    /// Whether this rig carries the Star Drive.
    pub star_drive: bool,
    /// World time (ms) the cable lit, if it has.
    pub cable_lit_millis: Option<u64>,
}

impl SpaceAccess {
    /// Whether this rig may fly past the Moon.
    #[must_use]
    pub const fn may_leave(&self) -> bool {
        self.unlocked && self.star_drive
    }
}

// ---------------------------------------------------------------------------------------------
// The Heart's fight.
// ---------------------------------------------------------------------------------------------

/// The Heart chamber's radius, tiles (its shell is the next [`SHELL_TILES`] rings).
pub const CHAMBER_RADIUS: i32 = 24;
/// The shell's thickness, tiles.
pub const SHELL_TILES: i32 = 2;
/// The Star Drive's body radius, tiles (its glowing core).
pub const CORE_RADIUS: i32 = 3;
/// Rows from the chamber's middle down to its floor (below it the chamber is a solid bowl).
pub const FLOOR_BELOW_TILES: i32 = 15;
/// The drive coils of the first phase.
pub const COIL_COUNT: usize = 6;
/// The coils' orbit radius, tiles.
pub const COIL_ORBIT_TILES: f32 = 10.0;
/// One turn of the coils' orbit.
pub const COIL_ORBIT_MILLIS: u64 = 26_000;
/// A coil fires a plasma bolt at the nearest rig this often (staggered round the ring).
pub const COIL_FIRE_MILLIS: u64 = 5_000;
/// The first gravity pulse comes this long into the first phase, then every
/// [`PULSE_PERIOD_MILLIS`].
pub const PULSE_FIRST_MILLIS: u64 = 6_000;
/// Time between gravity pulses.
pub const PULSE_PERIOD_MILLIS: u64 = 11_000;
/// A pulse's tell: the ring of light contracts toward the core.
pub const PULSE_TELL_MILLIS: u64 = 1_800;
/// How long a pulse pulls.
pub const PULSE_PULL_MILLIS: u64 = 2_200;
/// A pulse's pull, px/s² toward the core (a thrusting top-tier rig holds its ground).
pub const PULSE_PULL: f32 = 420.0;
/// The ignition phase's two beams turn once in this long.
pub const BEAM_TURN_MILLIS: u64 = 18_000;
/// The beams fade in over this long at the ignition (no damage yet: the tell).
pub const BEAM_TELL_MILLIS: u64 = 2_400;
/// A beam's half width, tiles.
pub const BEAM_HALF_WIDTH_TILES: f32 = 0.9;
/// Hull a second a beam burns (scaled by the depth's creature scale).
pub const BEAM_DAMAGE_PER_SECOND: f32 = 26.0;
/// Sparks the ignited drive throws out, at most at once.
pub const MAX_SPARKS: usize = 4;
/// Time between sparks.
pub const SPARK_MILLIS: u64 = 7_000;
/// Time between sparks while the core breathes (the collapse and the eclipse run long; Wave 11
/// carryover keeps them from swarming).
pub const BREATHING_SPARK_MILLIS: u64 = 10_000;
/// Sparks out at most at once while the core breathes.
pub const BREATHING_MAX_SPARKS: usize = 3;
/// The collapse's steady pull toward the core, px/s².
pub const COLLAPSE_PULL: f32 = 300.0;
/// The collapse's breath: armoured, then a tell, then the exhale (vulnerable, it pushes).
pub const BREATH_PERIOD_MILLIS: u64 = 9_500;
/// When in a breath the tell starts.
pub const EXHALE_TELL_AT_MILLIS: u64 = 4_000;
/// When in a breath the exhale starts (it lasts to the end of the breath).
pub const EXHALE_AT_MILLIS: u64 = 5_600;
/// The exhale's push, px/s² away from the core.
pub const EXHALE_PUSH: f32 = 260.0;
/// The core's health share (per mille) where the ignition gives way to the collapse.
pub const COLLAPSE_AT_PERMILLE: u16 = 520;
/// The core's health share (per mille) where the first ignition gives way to the reforge
/// (Wave 11 carryover: the finale's longer fight).
pub const REFORGE_AT_PERMILLE: u16 = 780;
/// The reforge's inner coils.
pub const INNER_COIL_COUNT: usize = 3;
/// The inner coils' orbit radius, tiles (inside the outer ring's, outside the core's touch).
pub const INNER_ORBIT_TILES: f32 = 6.5;
/// One turn of the inner coils' orbit (they turn the other way round).
pub const INNER_ORBIT_MILLIS: u64 = 16_000;
/// An inner coil fires this often (staggered round the ring).
pub const INNER_FIRE_MILLIS: u64 = 3_600;
/// The rekindled ignition's beams turn once in this long, the other way round.
pub const REKINDLED_BEAM_TURN_MILLIS: u64 = 15_000;
/// The core's health share (per mille) where the collapse deepens into the eclipse.
pub const ECLIPSE_AT_PERMILLE: u16 = 260;
/// The eclipse's single beam turns once in this long (it burns only while the core holds its
/// breath; the exhale is the safe window to strike).
pub const ECLIPSE_BEAM_TURN_MILLIS: u64 = 21_000;
/// With no rig in the chamber this long the Heart sinks back to sleep and heals.
pub const SLEEP_MILLIS: u64 = 20_000;
/// Touching the core burns this much hull a second.
pub const CORE_TOUCH_DAMAGE_PER_SECOND: f32 = 30.0;
/// Rigs are shielded from every harm for this long once the cable lights (the ending plays;
/// nobody dies during the credits).
pub const ENDING_SHIELD_MILLIS: u64 = ending::ENDING_SHIELD_MILLIS;

/// The Heart's fight, phase by phase.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum HeartPhase {
    /// Dormant: it wakes when a rig enters its chamber.
    #[default]
    Asleep,
    /// The drive coils orbit the sealed core: break all six while dodging its gravity pulses.
    Coils,
    /// The core ignites: two beams sweep the chamber and sparks fly; strike the open core.
    Ignition,
    /// The core collapses into a singularity: it pulls steadily and is armoured except while
    /// it exhales.
    Collapse,
    /// Awake: the cable lit, the Star Drive is the crew's.
    Awakened,
    /// The core reseals (Wave 11 carryover): three inner coils orbit close the other way round
    /// and the gravity pulses return. Break them to rekindle the core (the second ignition's
    /// beams turn back the other way, faster).
    Reforge,
    /// The singularity's last breath (Wave 11 carryover): the collapse's breathing, and one beam
    /// sweeping the chamber while the core holds its breath. Strike it as it exhales.
    Eclipse,
}

impl HeartPhase {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Asleep => "sleeping",
            Self::Coils => "the drive coils",
            Self::Ignition => "ignition",
            Self::Collapse => "collapse",
            Self::Awakened => "awake",
            Self::Reforge => "the reforge",
            Self::Eclipse => "the eclipse",
        }
    }

    /// The phase's number in the fight (1-5: coils, ignition, reforge, collapse, eclipse; the
    /// rekindled ignition counts as the second again), 0 outside it.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Coils => 1,
            Self::Ignition => 2,
            Self::Reforge => 3,
            Self::Collapse => 4,
            Self::Eclipse => 5,
            Self::Asleep | Self::Awakened => 0,
        }
    }

    /// Whether the fight is on.
    #[must_use]
    pub const fn fighting(self) -> bool {
        matches!(
            self,
            Self::Coils | Self::Ignition | Self::Reforge | Self::Collapse | Self::Eclipse
        )
    }

    /// Whether the core is sealed (it ignores every hit): while coils orbit it.
    #[must_use]
    pub const fn sealed(self) -> bool {
        matches!(self, Self::Coils | Self::Reforge)
    }

    /// Whether the core breathes (the collapse and the eclipse).
    #[must_use]
    pub const fn breathing(self) -> bool {
        matches!(self, Self::Collapse | Self::Eclipse)
    }
}

/// Where a gravity pulse is in its cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PulseState {
    Calm,
    /// The ring contracts: a pull is coming.
    Tell,
    /// It pulls.
    Pull,
}

/// The pulse at `elapsed` milliseconds into the coils phase (a pure function).
#[must_use]
pub const fn pulse_at(elapsed: u64) -> PulseState {
    if elapsed < PULSE_FIRST_MILLIS {
        return PulseState::Calm;
    }
    let t = (elapsed - PULSE_FIRST_MILLIS) % PULSE_PERIOD_MILLIS;
    if t < PULSE_TELL_MILLIS {
        PulseState::Tell
    } else if t < PULSE_TELL_MILLIS + PULSE_PULL_MILLIS {
        PulseState::Pull
    } else {
        PulseState::Calm
    }
}

/// Where the collapse's breath is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreathState {
    /// Armoured, pulling.
    Held,
    /// The core brightens: an exhale is coming.
    Tell,
    /// Open: it pushes, and it can be hurt.
    Exhale,
}

/// The breath at `elapsed` milliseconds into the collapse (a pure function).
#[must_use]
pub const fn breath_at(elapsed: u64) -> BreathState {
    let t = elapsed % BREATH_PERIOD_MILLIS;
    if t < EXHALE_TELL_AT_MILLIS {
        BreathState::Held
    } else if t < EXHALE_AT_MILLIS {
        BreathState::Tell
    } else {
        BreathState::Exhale
    }
}

/// The ignition's first beam angle at `elapsed` ms into the phase, radians (the second is
/// opposite it).
#[must_use]
pub fn beam_angle(elapsed: u64) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a turn's milliseconds")]
    let turn = (elapsed % BEAM_TURN_MILLIS) as f32 / BEAM_TURN_MILLIS as f32;
    crate::field::trig::wrap_angle(turn * std::f32::consts::TAU)
}

/// The rekindled ignition's first beam angle at `elapsed` ms into it (Wave 11 carryover): the
/// beams turn the other way round, faster.
#[must_use]
pub fn rekindled_beam_angle(elapsed: u64) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a turn's milliseconds")]
    let turn = (elapsed % REKINDLED_BEAM_TURN_MILLIS) as f32 / REKINDLED_BEAM_TURN_MILLIS as f32;
    crate::field::trig::wrap_angle(-turn * std::f32::consts::TAU)
}

/// The eclipse's beam angle at `elapsed` ms into it (Wave 11 carryover), and whether it burns:
/// only while the core holds its breath (never in the exhale, the window to strike).
#[must_use]
pub fn eclipse_beam(elapsed: u64) -> (f32, bool) {
    #[allow(clippy::cast_precision_loss, reason = "a turn's milliseconds")]
    let turn = (elapsed % ECLIPSE_BEAM_TURN_MILLIS) as f32 / ECLIPSE_BEAM_TURN_MILLIS as f32;
    let angle = crate::field::trig::wrap_angle(turn * std::f32::consts::TAU);
    let burning = elapsed >= BEAM_TELL_MILLIS && breath_at(elapsed) != BreathState::Exhale;
    (angle, burning)
}

/// The fight's beams at `elapsed` ms into `phase` (Wave 11 carryover).
///
/// Each beam's angle from the core and how strong it is (`0..=1`, burning only at 1). Two opposed beams in the
/// ignition (turning back the other way once `rekindled`), one in the eclipse while the core
/// holds its breath, none otherwise. The tick and the renderer both read this.
#[must_use]
pub fn fight_beams(phase: HeartPhase, rekindled: bool, elapsed: u64) -> Vec<(f32, f32)> {
    match phase {
        HeartPhase::Ignition => {
            let angle = if rekindled {
                rekindled_beam_angle(elapsed)
            } else {
                beam_angle(elapsed)
            };
            let strength = beam_strength(elapsed);
            vec![
                (angle, strength),
                (
                    crate::field::trig::wrap_angle(angle + std::f32::consts::PI),
                    strength,
                ),
            ]
        }
        HeartPhase::Eclipse => {
            let (angle, burning) = eclipse_beam(elapsed);
            let strength = if burning {
                1.0
            } else if breath_at(elapsed) == BreathState::Exhale {
                0.0
            } else {
                beam_strength(elapsed) * 0.99
            };
            vec![(angle, strength)]
        }
        _ => Vec::new(),
    }
}

/// Inner coil `index`'s angle on its orbit at `elapsed` ms into the reforge, radians (they
/// turn the other way round from the outer ring).
#[must_use]
pub fn inner_coil_angle(index: usize, elapsed: u64) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a coil index and a turn's milliseconds"
    )]
    let turn = index as f32 / INNER_COIL_COUNT as f32
        - (elapsed % INNER_ORBIT_MILLIS) as f32 / INNER_ORBIT_MILLIS as f32;
    crate::field::trig::wrap_angle(turn * std::f32::consts::TAU)
}

/// How strong the ignition's beams are at `elapsed` ms into the phase, `0..=1` (fading in
/// during the tell; they burn only at 1).
#[must_use]
pub fn beam_strength(elapsed: u64) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a tell's milliseconds")]
    let share = elapsed as f32 / BEAM_TELL_MILLIS as f32;
    share.clamp(0.0, 1.0)
}

/// Coil `index`'s angle on its orbit at `elapsed` ms into the coils phase, radians.
#[must_use]
pub fn coil_angle(index: usize, elapsed: u64) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "a coil index and a turn's milliseconds"
    )]
    let turn = index as f32 / COIL_COUNT as f32
        + (elapsed % COIL_ORBIT_MILLIS) as f32 / COIL_ORBIT_MILLIS as f32;
    crate::field::trig::wrap_angle(turn * std::f32::consts::TAU)
}

/// The Heart's fight state (shared, replicated, saved).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeartFight {
    #[serde(default)]
    pub phase: HeartPhase,
    /// The Star Drive's core while it is awake (entity id).
    #[serde(default)]
    pub entity: Option<u64>,
    /// The drive coils still orbiting (entity ids, by ring slot; `None` once broken).
    #[serde(default)]
    pub coils: Vec<Option<u64>>,
    /// Sparks out (entity ids).
    #[serde(default)]
    pub sparks: Vec<u64>,
    /// World time (ms) the current phase began.
    #[serde(default)]
    pub phase_millis: u64,
    /// The last tick a living rig was inside the chamber.
    #[serde(default)]
    pub last_visited: SimulationTick,
    /// World time (ms) the last spark came.
    #[serde(default)]
    pub spark_millis: u64,
    /// Whether the Heart has been woken for good.
    #[serde(default)]
    pub beaten: bool,
    /// The reforge's inner coils still orbiting (Wave 11 carryover; entity ids by slot).
    #[serde(default)]
    pub inner: Vec<Option<u64>>,
    /// The core has been reforged and rekindled: its second ignition (Wave 11 carryover).
    #[serde(default)]
    pub rekindled: bool,
}

impl HeartFight {
    /// Whether nothing has happened yet.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Coils still orbiting (the outer ring's, or the reforge's inner ones in the reforge).
    #[must_use]
    pub fn coils_left(&self) -> usize {
        if self.phase == HeartPhase::Reforge {
            return self.inner.iter().flatten().count();
        }
        self.coils.iter().flatten().count()
    }
}

// ---------------------------------------------------------------------------------------------
// Landmarks, events and the shared state.
// ---------------------------------------------------------------------------------------------

/// A landmark of Descent VIII the crew can find (story beats).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum HeartLandmark {
    /// The Upfall: a shaft where dark matter rains upward (Dark Matter Veins).
    Upfall,
    /// The Lens: the greatest of the wells (Singularity Fields).
    Lens,
    /// The Ferry Terminal: the builders' rail station over the metal sea (Core Ocean).
    FerryTerminal,
    /// The Heart's chamber (The Heart).
    HeartChamber,
}

impl HeartLandmark {
    pub const ALL: [Self; 4] = [
        Self::Upfall,
        Self::Lens,
        Self::FerryTerminal,
        Self::HeartChamber,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Upfall => "The Upfall",
            Self::Lens => "The Lens",
            Self::FerryTerminal => "The Ferry Terminal",
            Self::HeartChamber => "The Heart",
        }
    }

    /// The stratum it stands in.
    #[must_use]
    pub const fn stratum(self) -> Stratum {
        match self {
            Self::Upfall => Stratum::DarkMatterVeins,
            Self::Lens => Stratum::SingularityFields,
            Self::FerryTerminal => Stratum::CoreOcean,
            Self::HeartChamber => Stratum::TheHeart,
        }
    }

    /// The story beat the place tells when the crew first finds it.
    #[must_use]
    pub const fn story(self) -> &'static str {
        match self {
            Self::Upfall => {
                "A builder hauler hangs from the roof of the shaft, wheels up, its bay split \
                 open. Dark matter drifts out of it and falls toward the ceiling like slow snow. \
                 Chalked on a ledge below, in Amos's hand: SHE GOT LIGHTER WITH EVERY LUMP. \
                 CARRY ORE WITH IT, OR YOU GO UP AND NEVER DOWN."
            }
            Self::Lens => {
                "Light bends round the great well until the far wall shows twice, and through \
                 the rock, faint and upside down, hangs the Moon. One word is scratched into the \
                 glass ring at its rim, over and over, by the same tired hand: UP."
            }
            Self::FerryTerminal => {
                "The builders crossed the metal sea on rails that still hum. A ticket board \
                 lists two stops: HERE and HEART. Chalked under it, the Fathers' camp mark, an \
                 arrow down, and three words in Amos's hand: IT IS BREATHING."
            }
            Self::HeartChamber => {
                "The cable ends here, in a light too slow to be fire. The Star Drive sleeps in \
                 its cradle, as big as a town, and every few seconds it breathes. Somebody sat \
                 here a long time: a worn patch on the floor facing it, a cold stove, one cup. \
                 He never woke it. He was waiting for someone with the key."
            }
        }
    }

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// Something of Descent VIII happened (netcode codes `670 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HeartEvent {
    /// A lump of dark matter was mined.
    DarkMatterMined,
    /// A rig's dark matter outweighs it: it falls up.
    RigFloating,
    /// A floating rig is heavy again.
    RigSettled,
    /// A rig was caught in a gravity well's strong pull.
    WellCaught,
    /// A well's horizon crushed a hull.
    HorizonCrush,
    /// A booster rail threw a rig to top speed.
    RailBoost,
    /// The magnetic catch grabbed a rig onto a rail.
    RailCatch,
    /// The crew found a landmark (its story beat).
    LandmarkFound,
    /// The Heart stirred: the fight began.
    HeartStirred,
    /// A drive coil broke.
    CoilBroken,
    /// A gravity pulse's tell (the ring contracts).
    PulseTell,
    /// A gravity pulse pulls.
    Pulse,
    /// The core ignited (the second phase).
    Ignition,
    /// A spark flew from the core.
    SparkBorn,
    /// The core collapsed into a singularity (the third phase).
    Collapse,
    /// The exhale's tell (the core brightens).
    ExhaleTell,
    /// The core exhales (open to harm).
    Exhale,
    /// Nobody stayed: the Heart sank back to sleep.
    HeartSlept,
    /// The Heart woke for good.
    HeartAwakened,
    /// The cable to the Moon lit up (the ending begins).
    CableLit,
    /// A rig was fitted with the Star Drive.
    StarDriveFitted,
    /// Space past the Moon opened.
    SpaceUnlocked,
    /// A rig flew above the Moon after the ending: the stars are open.
    StarsOpen,
    /// A mercury eel leapt out of the metal.
    EelLeapt,
    /// A null moth drank a lump of dark matter from a rig's bay.
    MothDrank,
    /// The core resealed and called its inner coils (Wave 11 carryover; netcode 695).
    Reforged,
    /// The collapse deepened into the eclipse (Wave 11 carryover; netcode 696).
    Eclipse,
}

impl HeartEvent {
    /// Offset of the event's netcode code from 670.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

/// Descent VIII's shared state (world field, appended shared `SectionKey::Heart`, saved with
/// serde defaults, hashed once anything happened).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Heart {
    /// Rigs whose dark matter outweighs them now (they fall up).
    #[serde(default)]
    pub floating: BTreeSet<PlayerId>,
    /// Rigs caught in a gravity well now.
    #[serde(default)]
    pub in_well: BTreeSet<PlayerId>,
    /// Landmarks the crew found ([`HeartLandmark`] bits).
    #[serde(default)]
    pub found: u8,
    /// The Heart's fight.
    #[serde(default)]
    pub fight: HeartFight,
    /// Whether the Heart woke and space past the Moon is open: the world flag Wave 11's space
    /// flight reads ([`crate::WorldState::space_unlocked`]).
    #[serde(default)]
    pub space_unlocked: bool,
    /// Rigs fitted with the Star Drive (every rig of the crew once the Heart woke; later
    /// joiners are fitted as they arrive).
    #[serde(default)]
    pub star_drive: BTreeSet<PlayerId>,
    /// World time (ms) the cable lit: the ending began then (every seat plays it from there).
    #[serde(default)]
    pub cable_lit_millis: Option<u64>,
    /// Rigs that have flown above the Moon since the ending (the "stars are open" message
    /// shows once each).
    #[serde(default)]
    pub stars_seen: BTreeSet<PlayerId>,
}

impl Heart {
    /// Whether nothing happened yet (older worlds keep their hashes).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.floating.is_empty()
            && self.in_well.is_empty()
            && self.found == 0
            && self.fight.is_default()
            && !self.space_unlocked
            && self.star_drive.is_empty()
            && self.cable_lit_millis.is_none()
            && self.stars_seen.is_empty()
    }

    /// Whether the crew found `landmark`.
    #[must_use]
    pub const fn has_found(&self, landmark: HeartLandmark) -> bool {
        self.found & landmark.bit() != 0
    }

    /// Marks `landmark` found; whether it was new.
    pub const fn find(&mut self, landmark: HeartLandmark) -> bool {
        let new = !self.has_found(landmark);
        self.found |= landmark.bit();
        new
    }

    /// Whether `player`'s rig carries the Star Drive.
    #[must_use]
    pub fn has_star_drive(&self, player: PlayerId) -> bool {
        self.star_drive.contains(&player)
    }

    /// What `player` may do past the Moon.
    #[must_use]
    pub fn space_access(&self, player: PlayerId) -> SpaceAccess {
        SpaceAccess {
            unlocked: self.space_unlocked,
            star_drive: self.has_star_drive(player),
            cable_lit_millis: self.cable_lit_millis,
        }
    }

    /// Whether every rig is shielded at world time `now_millis` (the ending plays).
    #[must_use]
    pub const fn ending_shields(&self, now_millis: u64) -> bool {
        match self.cable_lit_millis {
            Some(lit) => now_millis >= lit && now_millis - lit < ENDING_SHIELD_MILLIS,
            None => false,
        }
    }

    /// Forgets a leaving player's transient state (the Star Drive stays: it is the crew's).
    pub fn forget(&mut self, player: PlayerId) {
        self.floating.remove(&player);
        self.in_well.remove(&player);
    }
}

// ---------------------------------------------------------------------------------------------
// Queries over the world layout.
// ---------------------------------------------------------------------------------------------

/// Whether the world has Descent VIII (a Deep Descent world).
#[must_use]
pub const fn has_heart(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// Whether `stratum` is one of the Heart's strata.
#[must_use]
pub const fn is_heart(stratum: Stratum) -> bool {
    matches!(
        stratum,
        Stratum::DarkMatterVeins
            | Stratum::SingularityFields
            | Stratum::CoreOcean
            | Stratum::TheHeart
    )
}

/// The Descent the Heart's strata belong to.
pub const DESCENT: Descent = Descent::Heart;

/// Whether `area` crosses one of Descent VIII's set pieces (the story's camps and rooms keep
/// off them; appended to `expedition::sites::DEEPER_SET_PIECES`).
#[must_use]
pub fn crosses_set_pieces(settings: &WorldSettings, area: &TileRect) -> bool {
    if !has_heart(settings) {
        return false;
    }
    let layout = crate::descent::fields::cached_layout(settings);
    crate::worldgen::descent::heart::crosses_set_pieces(settings, &layout, area)
}

/// The authored content of a Heart stratum the descent probe counts, `(mechanics, set pieces)`.
#[must_use]
pub const fn authored_content(stratum: Stratum) -> (u8, u8) {
    match stratum {
        // Inverted cargo physics, ceiling dark matter; the Upfall and the weightless halls.
        Stratum::DarkMatterVeins => (2, 2),
        // Wells and their horizons, lensing; the Lens.
        Stratum::SingularityFields => (2, 1),
        // The metal sea, the rails, switches and boosters; the Ferry Terminal.
        Stratum::CoreOcean => (3, 1),
        // The three-phase fight and the ending; the chamber.
        Stratum::TheHeart => (3, 2),
        _ => (0, 0),
    }
}

/// The new things a Heart stratum shows a player beyond its registry entry, in the order they
/// come (the descent probe's markers): `(what, is it a place)`.
#[must_use]
pub const fn novelties(stratum: Stratum) -> &'static [(&'static str, bool)] {
    match stratum {
        Stratum::DarkMatterVeins => &[
            ("dark matter lightens the rig", false),
            ("Heart's Edge, the last outpost", true),
            ("falling up onto the ceilings", false),
            ("the weightless halls", true),
            ("the Upfall", true),
        ],
        Stratum::SingularityFields => &[
            ("gravity wells pull everything in", false),
            ("light bends: lensing", false),
            ("slingshots round the rims", false),
            ("the Lens", true),
        ],
        Stratum::CoreOcean => &[
            ("a sea of liquid metal", false),
            ("magnetic rails", false),
            ("rail switches and the low line", false),
            ("the Ferry Terminal", true),
            ("boosters and the magnetic catch", false),
        ],
        Stratum::TheHeart => &[
            ("the builders' cable to the Heart", true),
            ("the Heart Key's seal", false),
            ("the Star Drive wakes", true),
            ("the cable lights: the ending", false),
            ("the Star Drive is your engine", false),
        ],
        _ => &[],
    }
}

/// The minerals the Heart's own tiles stand in for in reports (the layer resources).
#[must_use]
pub const fn layer_mineral(stratum: Stratum) -> Option<MineralKind> {
    match stratum {
        Stratum::DarkMatterVeins | Stratum::SingularityFields => Some(MineralKind::Mythril),
        Stratum::CoreOcean => Some(MineralKind::Platinum),
        Stratum::TheHeart => Some(MineralKind::Diamond),
        _ => None,
    }
}

/// The Heart chamber's middle on a Deep Descent world: at the builders' cable's root, near the
/// bottom of the world.
#[must_use]
pub fn heart_centre(settings: &WorldSettings) -> Option<TilePosition> {
    has_heart(settings).then(|| {
        let layout = crate::descent::fields::cached_layout(settings);
        crate::worldgen::descent::heart::heart_centre(settings, &layout)
    })
}

#[cfg(test)]
mod tests;
