//! The exotic worlds (Wave 13 planets-d, expansion plan §16 WP-SPACE "planet themes"): the
//! **gas giant with floating cities**, the **tidally locked world** and the **ring world**,
//! each a world like the home ground level.
//!
//! Every body the space generator tags [`crate::space::Theme::GasGiant`],
//! [`crate::space::Theme::TidallyLocked`] or [`crate::space::Theme::RingWorld`] lands on one of
//! the three ground themes registered here ([`GAS_GIANT`], [`LOCKED_WORLD`], [`RING_WORLD`] in
//! the planet theme registry, [`crate::planet::theme`]). Each is a theme world
//! ([`crate::planet::ThemeWorld`]) that builds its own chunks over the generic strata:
//!
//! | World | Surface | Strata (signature rule) | Landmark and boss |
//! |---|---|---|---|
//! | [`gas_giant`] | no ground: billowing cloud tops, three floating sky cities of the Siphra (docks, decks, towers, lift bladders) | Cloud Tops (jet streams), Storm Belt (lightning), Pressure Deep (crushing pressure, warned early), Diamond Rain (hailstones of diamond) | the Eye, a storm wall round a calm eye; the Thunderhead |
//! | [`locked`] | a scorched day side (hardpan, salt pans, mesas), a frozen night side (nitrogen plains, pressure ridges, ice spikes) and a living twilight ring (lichen hills, meltwater pools) | Divided Crust (sun and shadow), Geyser Fields (geysers), Stress Fractures (tidal quakes), Lodestone Core (the core's pull sunward) | the Gloam Gate on the morning terminator; the Gloamwarden |
//! | [`ring`] | ringfall plains, craters, fallen monoliths, and a band of floating ring debris (the Skyring) in its low sky | Ringfall Plains (meteor showers), Impact Breccia (venting ice), Tether Roots (live conduits), Dense Core (heavy pull) | the Tether, a builder column up to the Skyring; the Ring Warden |
//!
//! * **Depth**: the strata run 800-2,500 rows on a planet (scaled with its radius, the wild
//!   worlds' [`crate::planet::wilds::strata_rows`]) as concentric rings down to the core
//!   chamber; a moon or crater rock is strata all the way to its true core.
//! * **Halves that play apart** (the locked world): every stratum's materials, ores and
//!   creatures differ between the day side, the twilight ring and the night side
//!   ([`locked::Side`]), and the crust's rule bites with heat on one and cold on the other.
//! * **A boss each** ([`boss`]): a big readable creature at the theme's landmark, telegraphed,
//!   fair and winnable; beating it opens its seal into a trophy (the terrain keeps the win).
//! * **Sites** the other tracks build on: the settlement ([`settlement_site`]; the aliens' town
//!   on its deck or ground), the landmark ([`landmark_site`]), a builder outpost
//!   ([`builder_outpost`], the galaxy story's relic sites) and one secret on a small body ([`secret`]).
//!
//! Generation is a pure function of the planet's params (integer noise only). The rules run in
//! `world/exotic.rs`, the creatures in `world/enemies/exotic.rs`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::too_long_first_doc_paragraph,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::collapsible_match,
    clippy::too_many_lines,
    reason = "generation and rules arithmetic over bounded columns, rows and milliseconds; \
              doc paragraphs that read as one thought"
)]

pub mod boss;
pub mod gas_giant;
pub mod hooks;
pub mod locked;
pub mod ring;
pub mod rules;
#[cfg(test)]
mod tests;
pub mod tile;

use serde::{Deserialize, Serialize};

pub use tile::{EXOTIC_TILE_CODE_BASE, ExoticTile};

use crate::{
    enemy::EnemyKind,
    planet::{PlanetGround, PlanetThemeId, SurfaceSite, hash3},
    terrain::{Tile, TileKind, tile_durability},
};

/// The registry id of the gas giant theme (planets-d range 12-14; planets-c holds 9-11).
pub const GAS_GIANT: PlanetThemeId = PlanetThemeId(12);
/// The registry id of the tidally locked theme.
pub const LOCKED_WORLD: PlanetThemeId = PlanetThemeId(13);
/// The registry id of the ring world theme.
pub const RING_WORLD: PlanetThemeId = PlanetThemeId(14);

/// Which exotic world a theme is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ExoticStyle {
    GasGiant,
    Locked,
    Ring,
}

impl ExoticStyle {
    pub const ALL: [Self; 3] = [Self::GasGiant, Self::Locked, Self::Ring];

    /// The theme's id in the planet theme registry.
    #[must_use]
    pub const fn theme(self) -> PlanetThemeId {
        match self {
            Self::GasGiant => GAS_GIANT,
            Self::Locked => LOCKED_WORLD,
            Self::Ring => RING_WORLD,
        }
    }

    /// The four strata's rules, top down.
    #[must_use]
    pub const fn rules(self) -> [ExoticRule; 4] {
        match self {
            Self::GasGiant => [
                ExoticRule::JetStreams,
                ExoticRule::Lightning,
                ExoticRule::CrushingPressure,
                ExoticRule::DiamondRain,
            ],
            Self::Locked => [
                ExoticRule::SunAndShadow,
                ExoticRule::Geysers,
                ExoticRule::TidalQuakes,
                ExoticRule::LodestoneDrift,
            ],
            Self::Ring => [
                ExoticRule::MeteorShower,
                ExoticRule::VentingIce,
                ExoticRule::LiveConduits,
                ExoticRule::DenseCore,
            ],
        }
    }

    /// The world's name (the body badge's theme line).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GasGiant => "Gas Giant",
            Self::Locked => "Locked World",
            Self::Ring => "Ring World",
        }
    }

    /// The landmark every body of the style carries.
    #[must_use]
    pub const fn landmark_name(self) -> &'static str {
        match self {
            Self::GasGiant => "the Eye",
            Self::Locked => "the Gloam Gate",
            Self::Ring => "the Tether",
        }
    }

    /// The secret a crater rock or small moon of the style hides.
    #[must_use]
    pub const fn secret_name(self) -> &'static str {
        match self {
            Self::GasGiant => "a drifting weather buoy's cache",
            Self::Locked => "a cache frozen in the terminator's shade",
            Self::Ring => "a cache in a fallen ring shard",
        }
    }

    /// The builder outpost the galaxy story can send the player to.
    #[must_use]
    pub const fn outpost_name(self) -> &'static str {
        match self {
            Self::GasGiant => "the Builders' Aerostat",
            Self::Locked => "the Dark Observatory",
            Self::Ring => "the Tether Station",
        }
    }

    /// The theme boss of the style.
    #[must_use]
    pub const fn boss(self) -> EnemyKind {
        match self {
            Self::GasGiant => EnemyKind::Thunderhead,
            Self::Locked => EnemyKind::Gloamwarden,
            Self::Ring => EnemyKind::RingWarden,
        }
    }

    /// The four native creatures (neutral fauna first).
    #[must_use]
    pub const fn creatures(self) -> [EnemyKind; 4] {
        match self {
            Self::GasGiant => [
                EnemyKind::DriftRay,
                EnemyKind::Gasbag,
                EnemyKind::StormEel,
                EnemyKind::VaporStinger,
            ],
            Self::Locked => [
                EnemyKind::DuskGrazer,
                EnemyKind::Sunbasker,
                EnemyKind::ScorchMite,
                EnemyKind::FrostStalker,
            ],
            Self::Ring => [
                EnemyKind::Ringhopper,
                EnemyKind::Shardback,
                EnemyKind::TetherDrone,
                EnemyKind::VoidMite,
            ],
        }
    }

    /// The style's world (its generator).
    #[must_use]
    pub fn world(self) -> &'static dyn crate::planet::ThemeWorld {
        match self {
            Self::GasGiant => &gas_giant::GAS_GIANT_WORLD,
            Self::Locked => &locked::LOCKED_WORLD_GEN,
            Self::Ring => &ring::RING_WORLD_GEN,
        }
    }
}

/// The exotic style of planet theme `theme`, if it is one of the three.
#[must_use]
pub const fn style_of(theme: PlanetThemeId) -> Option<ExoticStyle> {
    match theme.0 {
        id if id == GAS_GIANT.0 => Some(ExoticStyle::GasGiant),
        id if id == LOCKED_WORLD.0 => Some(ExoticStyle::Locked),
        id if id == RING_WORLD.0 => Some(ExoticStyle::Ring),
        _ => None,
    }
}

/// The exotic style of `ground`, if it is an exotic world.
#[must_use]
pub const fn style_of_ground(ground: &PlanetGround) -> Option<ExoticStyle> {
    style_of(ground.params().theme)
}

/// A stratum's signature rule (what changes how it plays; `world/exotic.rs` runs them), and
/// the two zone rules above ground ([`Self::Buoyancy`], [`Self::Microgravity`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ExoticRule {
    /// Cloud Tops: lanes of wind 24 rows deep blow east and west in turn and carry whatever
    /// flies in them (a fast lane one way, a slog the other; drilling braces).
    JetStreams,
    /// Storm Belt: charged columns flicker, then a bolt strikes down them (move out of the
    /// flicker).
    Lightning,
    /// Pressure Deep: the gas crushes the hull harder the deeper you go, past early warnings
    /// (the high-gravity struts brace against it).
    CrushingPressure,
    /// Diamond Rain: hailstones of diamond fall under growing shadows, leaving diamond where
    /// they land (and a dent in what they hit).
    DiamondRain,
    /// Divided Crust: the day side's open ground and crust burn, the night side's freeze; the
    /// twilight ring is mild (heat or cold gear, or shade).
    SunAndShadow,
    /// Geyser Fields: vents hiss, then erupt: steam on the day side, nitrogen on the night;
    /// a plume throws whatever is over it upward (a lift if you ride it, a knock if not).
    Geysers,
    /// Stress Fractures: the tide flexes the world: a rumble, then rubble falls from the
    /// ceilings round a rig (move under solid rock).
    TidalQuakes,
    /// Lodestone Core: the magnetic core drags a rig toward the sunward meridian.
    LodestoneDrift,
    /// Ringfall Plains: ring debris rains down under growing shadows, leaving meteorite where
    /// it lands.
    MeteorShower,
    /// Impact Breccia: buried ring ice vents a blast of vapour when drilled, throwing the rig
    /// back.
    VentingIce,
    /// Tether Roots: the builders' conduits are live and shock a rig touching them; cut a
    /// junction to kill its run.
    LiveConduits,
    /// Dense Core: the heavy core pulls half again as hard (high-gravity gear).
    DenseCore,
    /// The gas giant everywhere: the thick gas holds a craft up (a gentle fall, a fast climb).
    Buoyancy,
    /// The Skyring: in the ring's low orbit the pull is a third of the ground's and the
    /// debris drifts east.
    Microgravity,
}

impl ExoticRule {
    /// What the player is told when entering it.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::JetStreams => "Jet streams blow east and west by turns: ride them or drill",
            Self::Lightning => "Charged clouds flicker before a bolt: move out of the flicker",
            Self::CrushingPressure => "The pressure crushes deeper down: brace the hull",
            Self::DiamondRain => "Diamond hail falls under growing shadows",
            Self::SunAndShadow => "The day side burns, the night side freezes: twilight is mild",
            Self::Geysers => "Vents hiss before they blow: ride a plume up, or stand clear",
            Self::TidalQuakes => "Tidal quakes shake rubble loose: shelter under solid rock",
            Self::LodestoneDrift => "The lodestone core drags you sunward",
            Self::MeteorShower => "Ring debris falls under growing shadows",
            Self::VentingIce => "Buried ring ice vents when drilled: brace for the blast",
            Self::LiveConduits => "Live conduits shock on touch: cut a junction to kill a run",
            Self::DenseCore => "The dense core pulls half again as hard",
            Self::Buoyancy => "The thick gas holds you up",
            Self::Microgravity => "Low orbit: a third of the pull, and the debris drifts",
        }
    }

    /// The planet-gear hazard the rule is, if it is one.
    #[must_use]
    pub const fn hazard(self) -> Option<crate::frontier::PlanetHazard> {
        use crate::frontier::PlanetHazard;
        match self {
            Self::CrushingPressure | Self::DenseCore => Some(PlanetHazard::HighGravity),
            Self::SunAndShadow => Some(PlanetHazard::Heat),
            _ => None,
        }
    }
}

/// Every hazard of `style` (surface, weather and strata), for the codex and the planet gear's
/// shop text (the hooks' tags name them for the stations).
#[must_use]
pub fn hazards(style: ExoticStyle) -> Vec<crate::frontier::PlanetHazard> {
    use crate::frontier::PlanetHazard;
    let mut all: Vec<PlanetHazard> = match style {
        ExoticStyle::GasGiant => vec![PlanetHazard::HighGravity, PlanetHazard::Cold],
        ExoticStyle::Locked => vec![PlanetHazard::Heat, PlanetHazard::Cold],
        ExoticStyle::Ring => vec![PlanetHazard::HighGravity],
    };
    all.extend(style.rules().iter().filter_map(|rule| rule.hazard()));
    all.sort();
    all.dedup();
    all
}

/// Something that happened on an exotic world (netcode codes 1020-1039, see
/// [`crate::event::GameplayEventKind::Exotic`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ExoticEvent {
    /// A charged column began to flicker (a bolt follows).
    LightningTell,
    /// A bolt struck down a column.
    LightningStrike,
    /// A rig went deep enough for the pressure to bite (and every few seconds while it does).
    PressureWarning,
    /// A diamond hailstone landed.
    DiamondHail,
    /// A geyser vent began to hiss.
    GeyserHiss,
    /// A geyser erupted.
    GeyserErupt,
    /// A tidal quake began to rumble.
    QuakeRumble,
    /// The quake shook rubble loose.
    QuakeShock,
    /// Ring debris is falling (its shadow grows).
    MeteorIncoming,
    /// A meteorite landed.
    MeteorImpact,
    /// Buried ring ice vented at the drill.
    IceVented,
    /// A live conduit shocked a rig.
    ConduitShock,
    /// A conduit junction was cut and its run went dark.
    ConduitCut,
    /// A planet hazard bit a rig without the gear for it (heat, cold, the pull).
    HazardBite,
    /// The world's storm is rising.
    StormRising,
    /// The theme boss woke.
    BossAwoke,
    /// The boss is winding up an attack (its tell; the marks show where).
    BossTell,
    /// The boss fell: its seal opened into its trophy.
    BossDefeated,
    /// A landmark's relic, a secret cache or a builder outpost's beacon was found.
    RelicFound,
    /// A rig set down on a sky city's deck (or a ring anchor's platform).
    CityDocked,
}

impl ExoticEvent {
    /// Every event, in code order.
    pub const ALL: [Self; 20] = [
        Self::LightningTell,
        Self::LightningStrike,
        Self::PressureWarning,
        Self::DiamondHail,
        Self::GeyserHiss,
        Self::GeyserErupt,
        Self::QuakeRumble,
        Self::QuakeShock,
        Self::MeteorIncoming,
        Self::MeteorImpact,
        Self::IceVented,
        Self::ConduitShock,
        Self::ConduitCut,
        Self::HazardBite,
        Self::StormRising,
        Self::BossAwoke,
        Self::BossTell,
        Self::BossDefeated,
        Self::RelicFound,
        Self::CityDocked,
    ];

    /// Offset of the event's netcode code from 1020.
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

// ---------------------------------------------------------------------------------------------
// Generation helpers shared by the three worlds.
// ---------------------------------------------------------------------------------------------

/// A hash of `(purpose, a, b)` for `ground`'s body (the planets-d salt).
#[must_use]
pub const fn roll(ground: &PlanetGround, purpose: u64, a: i64, b: i64) -> u64 {
    hash3(ground.params().seed, 0xD13E_0000 + purpose, a, b)
}

/// A full tile of `kind`.
#[must_use]
pub const fn tile(kind: TileKind) -> Tile {
    Tile {
        kind,
        durability: tile_durability(kind),
    }
}

/// An exotic tile.
#[must_use]
pub const fn exotic(tile_kind: ExoticTile) -> Tile {
    tile(TileKind::Exotic(tile_kind))
}

/// The open air.
#[must_use]
pub const fn air() -> Tile {
    tile(TileKind::Air)
}

/// `column - centre` folded into `-width/2..width/2` (the short way round the strip).
#[must_use]
pub const fn wrap_delta(column: i64, centre: i64, width: i64) -> i64 {
    crate::planet::wilds::wrap_delta(column, centre, width)
}

/// A column `fraction_permille` of a lap past `from`, wrapped into the strip.
#[must_use]
pub fn lap_column(ground: &PlanetGround, from: i64, fraction_permille: i64) -> i64 {
    let width = i64::from(ground.strip_width());
    (from + width * fraction_permille / 1_000).rem_euclid(width)
}

/// The settlement site of `ground`, if it is an exotic world (the aliens build their town on
/// it: a sky city's deck, a twilight meadow, a ringfall plain).
#[must_use]
pub fn settlement_site(ground: &PlanetGround) -> Option<SurfaceSite> {
    let style = style_of_ground(ground)?;
    style.world().settlement_site(ground)
}

/// The landmark site of `ground` (its boss's arena), if it is an exotic world.
#[must_use]
pub fn landmark_site(ground: &PlanetGround) -> Option<SurfaceSite> {
    let style = style_of_ground(ground)?;
    style.world().landmark_site(ground)
}

/// A builder outpost the galaxy story can target: where it stands and its beacon.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BuilderOutpost {
    pub style: ExoticStyle,
    /// Its middle column.
    pub column: i32,
    /// Its floor row (the first solid row under its room).
    pub floor: i32,
    /// The beacon tile inside it (a glowing builder relic: the story's marker).
    pub beacon: crate::terrain::TilePosition,
}

/// The builder outpost of `ground`, if it is an exotic world (the galaxy story's relics stand
/// at its beacon: `galaxy_story::plan`).
#[must_use]
pub fn builder_outpost(ground: &PlanetGround) -> Option<BuilderOutpost> {
    let style = style_of_ground(ground)?;
    Some(match style {
        ExoticStyle::GasGiant => gas_giant::outpost(ground),
        ExoticStyle::Locked => locked::outpost(ground),
        ExoticStyle::Ring => ring::outpost(ground),
    })
}

/// A small body's one secret: its prize tile and the marker over it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Secret {
    pub style: ExoticStyle,
    pub prize: crate::terrain::TilePosition,
    pub column: i32,
}

/// The secret of `ground`, if it is a small exotic body (crater rocks and moons up to
/// radius 1,000).
#[must_use]
pub fn secret(ground: &PlanetGround) -> Option<Secret> {
    let style = style_of_ground(ground)?;
    if !crate::planet::wilds::is_small_body(ground.params()) {
        return None;
    }
    let column = secret_column(ground);
    let surface = style.world().surface_row(ground, column);
    Some(Secret {
        style,
        prize: crate::terrain::TilePosition {
            x: i32::try_from(column).unwrap_or(0),
            y: surface + SECRET_DEPTH + 2,
        },
        column: i32::try_from(column).unwrap_or(0),
    })
}

/// Rows from the surface to the top of a secret's chamber.
pub const SECRET_DEPTH: i32 = 16;

/// The secret's column on a small body (three eighths of a lap from the settlement: clear of
/// the sky cities, the landmark and the outposts).
#[must_use]
pub fn secret_column(ground: &PlanetGround) -> i64 {
    let settlement = style_of_ground(ground)
        .and_then(|style| style.world().settlement_site(ground))
        .map_or(0, |site| i64::from(site.centre()));
    lap_column(ground, settlement, 375)
}

/// The secret's tile at `(first, row)` (`surface` the ground there), if part of it: a marker
/// of `marker` on the surface over a small chamber lined with `rock` holding a
/// [`ExoticTile::RelicCache`].
#[must_use]
pub fn secret_tile(
    ground: &PlanetGround,
    first: i64,
    row: i32,
    surface: i32,
    rock: Tile,
    marker: Tile,
) -> Option<Tile> {
    if !crate::planet::wilds::is_small_body(ground.params()) {
        return None;
    }
    let width = i64::from(ground.strip_width());
    let delta = wrap_delta(first, secret_column(ground), width);
    if !(-3..=3).contains(&delta) {
        return None;
    }
    if delta == 0 && (row == surface - 1 || row == surface - 2) {
        return Some(marker);
    }
    let top = surface + SECRET_DEPTH;
    if row < top - 1 || row > top + 3 {
        return None;
    }
    if (-2..=2).contains(&delta) && (top..=top + 2).contains(&row) {
        if delta == 0 && row == top + 2 {
            return Some(exotic(ExoticTile::RelicCache));
        }
        return Some(air());
    }
    Some(rock)
}

/// The settlement's half width: `base`, or wide enough for the alien town that stands on it
/// when a species settled the body (its span and the fades either side).
#[must_use]
pub fn settlement_half(ground: &PlanetGround, base: i32) -> i32 {
    crate::alien::AlienSpecies::from_folk(ground.params().folk).map_or(base, |species| {
        base.max(crate::alien::town::span_half(species) + crate::alien::town::FADE_COLUMNS + 2)
    })
}

/// The settlement's middle column (a pure function of the body).
#[must_use]
pub fn settlement_column(ground: &PlanetGround) -> i64 {
    let width = i64::from(ground.strip_width());
    (roll(ground, 1, 0, 0) % width.cast_unsigned()).cast_signed()
}

/// Value noise in two dimensions (`0..=1023`) over cells `cell_w` columns by `cell_h` rows,
/// periodic round the lap (`cell_w` must divide the strip's width: every width is a multiple
/// of 32, so 8, 16 and 32 do).
#[must_use]
pub fn value_noise2(
    ground: &PlanetGround,
    purpose: u64,
    column: i64,
    row: i32,
    cell_w: i64,
    cell_h: i32,
) -> i64 {
    let width = i64::from(ground.strip_width());
    let cells = (width / cell_w).max(1);
    let column = column.rem_euclid(width);
    let (cx, tx) = (column / cell_w, column % cell_w);
    let (cy, ty) = (row.div_euclid(cell_h), row.rem_euclid(cell_h));
    let value = |x: i64, y: i32| -> i64 {
        i64::try_from(roll(ground, purpose, x.rem_euclid(cells), i64::from(y)) >> 54).unwrap_or(0)
    };
    let top = value(cx, cy) * (cell_w - tx) + value(cx + 1, cy) * tx;
    let bottom = value(cx, cy + 1) * (cell_w - tx) + value(cx + 1, cy + 1) * tx;
    let ty = i64::from(ty);
    let h = i64::from(cell_h);
    (top * (h - ty) + bottom * ty) / (cell_w * h)
}

/// The tile of a builder outpost room centred at `centre` with its floor at `floor`
/// (`dx` columns from the centre, `row`), if part of it: a 15 x 7 shell of builder plate
/// with a door either side and the beacon on its floor.
#[must_use]
pub fn outpost_room_tile(dx: i64, row: i32, floor: i32) -> Option<Tile> {
    const HALF: i64 = 7;
    const HEIGHT: i32 = 7;
    if dx.abs() > HALF || row >= floor || row < floor - HEIGHT {
        return None;
    }
    let wall = dx.abs() == HALF;
    let roof = row == floor - HEIGHT;
    // Doors two rows high at the foot of each wall.
    if wall && row >= floor - 3 {
        return Some(air());
    }
    if wall || roof {
        return Some(exotic(ExoticTile::BuilderPlate));
    }
    if dx == 0 && row == floor - 1 {
        return Some(exotic(ExoticTile::BuilderBeacon));
    }
    Some(air())
}
