//! Fluids v1 (Part III F9): water, acid, brine and liquid metal as simple, deterministic
//! cellular fluids.
//!
//! **Storage.** A fluid cell is a terrain tile, [`TileKind::Fluid`], whose `durability` byte holds
//! the fluid's level: [`FLUID_FULL`] is one tile of fluid, and a cell deep in a column may hold a
//! little more ([`FLUID_MAX`], the "pressure" that lets fluid climb U-bends). Fluid tiles are open
//! ground for everything that moves (rigs, pilots, entities, drops; [`TileKind::is_open`]), and
//! because they are tiles, fluids save, hash, stream and replicate exactly like every other
//! terrain write: a fluid that settles stops writing tiles, so only the cells that actually change
//! cost anything on the wire (the terrain op log, and a stale chunk's refresh carries its fluid
//! tiles with its other diffs). Generated fluid (a V11 stratum's pockets and pools,
//! [`fluid_pockets_in_chunk`]) is part of generation, so an untouched pocket costs nothing at all.
//!
//! **Rules** (`world/fluids.rs` steps them; everything here is pure):
//! * Fluid falls first, then spreads sideways towards lower neighbours, then climbs when it is
//!   compressed past a full tile ([`stable_lower`], the classic compressible split). Levels are
//!   integers and every move is a transfer, so the total volume is conserved exactly; the only
//!   sinks are the reactions below and a solid tile written over a fluid cell.
//! * A denser fluid sinks through a lighter one (the two cells swap, [`FluidKind::density`]).
//! * Reactions ([`FluidReaction`]): water quenches lava into obsidian (some water boils away),
//!   water cools liquid metal into an iron ore crust (a lone thin splash becomes slag dust), and acid
//!   slowly eats soft tiles ([`acid_dissolves`]) and is spent doing it.
//! * The world steps fluids at [`FLUID_STEPS_PER_SECOND`] whatever the tick rate, and only cells
//!   woken by a change (their own or a neighbour's) are looked at, at most
//!   [`FLUID_STEP_BUDGET`] per step, so a settled lake costs nothing and a breached one is spread
//!   over a few steps.
//!
//! **Bodies in fluid.** [`Submersion`] measures how deep a body is; rigs get buoyancy, drag and a
//! submarine mode (`world/fluids.rs`), pilots swim (the pilot's swim mode reads fluid as water),
//! and creatures that cannot swim ([`crate::enemy::EnemyKind::can_swim`]) keep out.
//! [`SubmersionGear`] is the upgrade seam for Descent IV's Pressure Hull and Bio-Filter.

use serde::{Deserialize, Serialize};

use crate::terrain::{CHUNK_SIZE_TILES, ChunkPosition, MineralKind, Tile, TileKind, TilePosition};

/// Level of one full tile of fluid.
pub const FLUID_FULL: u8 = 100;
/// Most a (compressed) cell ever holds.
pub const FLUID_MAX: u8 = 250;
/// Extra level each row of depth lets a cell hold over the one above it (the compression that
/// carries pressure through a column; see [`stable_lower`]).
pub const FLUID_COMPRESSION: u16 = 4;
/// Fluid steps per second of world time (tick-rate independent).
pub const FLUID_STEPS_PER_SECOND: u64 = 20;
/// Most woken cells one fluid step looks at (the rest wait for the next step, in order).
pub const FLUID_STEP_BUDGET: usize = 2_048;
/// Most fluid cells a body's submersion counts above it when it reads the fluid's pressure.
pub const PRESSURE_SCAN_ROWS: i32 = 64;

/// A fluid's kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FluidKind {
    /// Plain water: drowns pilots, quenches lava, cools liquid metal.
    Water,
    /// Corrosive acid: eats hulls and soft ground.
    Acid,
    /// Heavy, salty brine: very buoyant (a rig bobs at its surface).
    Brine,
    /// Molten metal: very heavy, scalding, and it sets into ore where water cools it.
    LiquidMetal,
}

impl FluidKind {
    /// Every kind, in code order.
    pub const ALL: [Self; 4] = [Self::Water, Self::Acid, Self::Brine, Self::LiquidMetal];

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Water => "water",
            Self::Acid => "acid",
            Self::Brine => "brine",
            Self::LiquidMetal => "liquid metal",
        }
    }

    /// Stable code (part of [`Tile::code`] and so of terrain digests).
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Water => 0,
            Self::Acid => 1,
            Self::Brine => 2,
            Self::LiquidMetal => 3,
        }
    }

    /// The kind with code `code`.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Water),
            1 => Some(Self::Acid),
            2 => Some(Self::Brine),
            3 => Some(Self::LiquidMetal),
            _ => None,
        }
    }

    /// Relative density: a denser fluid sinks through a lighter one.
    #[must_use]
    pub const fn density(self) -> u8 {
        match self {
            Self::Water => 10,
            Self::Acid => 12,
            Self::Brine => 13,
            Self::LiquidMetal => 70,
        }
    }

    /// Share of a fully submerged body's weight the fluid carries (over 1 floats it up).
    #[must_use]
    pub const fn buoyancy(self) -> f32 {
        match self {
            Self::Water => 0.82,
            Self::Acid => 0.86,
            Self::Brine => 1.12,
            Self::LiquidMetal => 1.6,
        }
    }

    /// Drag on a fully submerged body (per second; velocity divides by `1 + rate * dt`).
    #[must_use]
    pub const fn drag(self) -> f32 {
        match self {
            Self::Water => 3.0,
            Self::Acid => 3.4,
            Self::Brine => 3.6,
            Self::LiquidMetal => 9.0,
        }
    }

    /// Divisor of the sideways spread (higher is more viscous).
    #[must_use]
    pub const fn viscosity(self) -> u16 {
        match self {
            Self::Water | Self::Brine | Self::Acid => 3,
            Self::LiquidMetal => 6,
        }
    }

    /// Most of the fluid a cell pours into the cell below in one step.
    #[must_use]
    pub const fn fall_per_step(self) -> u16 {
        match self {
            Self::Water | Self::Brine | Self::Acid => 100,
            Self::LiquidMetal => 40,
        }
    }

    /// Hull lost per second by a fully submerged rig with no protection.
    #[must_use]
    pub const fn hull_damage_per_second(self) -> f32 {
        match self {
            Self::Water | Self::Brine => 0.0,
            Self::Acid => 6.0,
            Self::LiquidMetal => 16.0,
        }
    }

    /// Health lost per second by a pilot in it (on top of holding its breath).
    #[must_use]
    pub const fn pilot_damage_per_second(self) -> f32 {
        match self {
            Self::Water | Self::Brine => 0.0,
            Self::Acid => 9.0,
            Self::LiquidMetal => 40.0,
        }
    }

    /// Whether a pilot can swim in it (liquid metal holds a pilot up like a floor would, but
    /// it burns; every other fluid is swum).
    #[must_use]
    pub const fn swimmable(self) -> bool {
        !matches!(self, Self::LiquidMetal)
    }
}

/// One fluid cell: its kind and level (`1..=FLUID_MAX`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FluidCell {
    pub kind: FluidKind,
    pub level: u8,
}

impl FluidCell {
    /// The fluid in `tile`, if it is a fluid tile with something in it.
    #[must_use]
    pub const fn of(tile: Tile) -> Option<Self> {
        match tile.kind {
            TileKind::Fluid(kind) if tile.durability > 0 => Some(Self {
                kind,
                level: tile.durability,
            }),
            _ => None,
        }
    }

    /// The terrain tile holding this fluid.
    #[must_use]
    pub const fn tile(self) -> Tile {
        Tile {
            kind: TileKind::Fluid(self.kind),
            durability: self.level,
        }
    }

    /// A full tile of `kind`.
    #[must_use]
    pub const fn full(kind: FluidKind) -> Self {
        Self {
            kind,
            level: FLUID_FULL,
        }
    }

    /// Height of the fluid's surface inside its tile, as a share of the tile (0..=1).
    #[must_use]
    pub fn fill(self) -> f32 {
        (f32::from(self.level) / f32::from(FLUID_FULL)).min(1.0)
    }
}

/// How much of a vertical pair holding `total` belongs in the lower cell at rest.
///
/// Everything up to a full tile, then the lower cell is compressed by [`FLUID_COMPRESSION`]
/// over the upper one (so pressure grows with depth and fluid climbs U-bends).
#[must_use]
#[allow(
    clippy::suspicious_operation_groupings,
    reason = "the classic compressible split: (full^2 + total * k) / (full + k)"
)]
pub const fn stable_lower(total: u16) -> u16 {
    let full = FLUID_FULL as u16;
    if total <= full {
        total
    } else if total < 2 * full + FLUID_COMPRESSION {
        (full * full + total * FLUID_COMPRESSION) / (full + FLUID_COMPRESSION)
    } else {
        total.div_ceil(2) + FLUID_COMPRESSION / 2
    }
}

/// Which soft tiles acid eats ([`acid_dissolves`]).
#[must_use]
pub const fn acid_dissolves(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt
            | TileKind::Clay
            | TileKind::Surface(
                crate::terrain::SurfaceMaterial::Sand
                    | crate::terrain::SurfaceMaterial::Mud
                    | crate::terrain::SurfaceMaterial::Snow
            )
    )
}

/// Fluid steps due by `tick` at `hz` (the world steps fluids when this grows, so fluids move at
/// [`FLUID_STEPS_PER_SECOND`] of world time at every tick rate).
#[must_use]
pub fn fluid_steps_by(tick: u64, hz: u32) -> u64 {
    let steps = u128::from(tick) * u128::from(FLUID_STEPS_PER_SECOND) / u128::from(hz.max(1));
    u64::try_from(steps).unwrap_or(u64::MAX)
}

/// Fluid reactions (each spends fluid: these are the volume's only sinks besides a solid tile
/// written over a fluid cell).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FluidReaction {
    /// Water met lava: the lava set into obsidian and some water boiled off.
    Quenched,
    /// Water cooled liquid metal: it set into ore (or slag dust) and some water boiled off.
    Solidified,
    /// Acid ate a soft tile and was spent.
    Dissolved,
}

/// Water boiled off by quenching a lava tile.
pub const QUENCH_WATER_COST: u8 = 30;
/// Water boiled off by cooling a liquid metal cell.
pub const COOL_WATER_COST: u8 = 25;
/// Liquid metal level that sets into an ore tile (thinner cells crumble to slag dust unless
/// they are the edge of a bigger body: beside more metal or its ore crust).
pub const METAL_ORE_LEVEL: u8 = 50;
/// What cooled liquid metal sets into.
pub const COOLED_METAL_ORE: TileKind = TileKind::Ore(MineralKind::Iron);
/// Acid spent dissolving one soft tile.
pub const ACID_DISSOLVE_COST: u8 = 20;
/// Least acid in a cell that eats ground.
pub const ACID_MIN_LEVEL: u8 = 20;
/// Acid eats a touching soft tile on about one fluid step in this many (about every 2 s).
pub const ACID_DISSOLVE_ODDS: u64 = 40;

/// How deep a body is in fluid: the share of its height under the surface and the fluid it
/// is mostly in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Submersion {
    /// Share of the body under fluid, 0 (dry) to 1 (fully under).
    pub share: f32,
    /// The fluid at its deepest wet sample.
    pub kind: Option<FluidKind>,
    /// Fluid cells stacked over the body's middle (its "pressure depth", capped at
    /// [`PRESSURE_SCAN_ROWS`]).
    pub depth_rows: i32,
}

impl Submersion {
    /// Whether the body is in fluid at all.
    #[must_use]
    pub fn is_wet(self) -> bool {
        self.share > 0.0
    }

    /// Whether the body is under enough to move like a submarine (two thirds under).
    #[must_use]
    pub fn is_submerged(self) -> bool {
        self.share >= SUBMARINE_SHARE
    }
}

/// Share of a rig under fluid at which it handles as a submarine.
pub const SUBMARINE_SHARE: f32 = 0.6;
/// Thrust a submerged rig's engine still gives (its lift is mostly ballast).
pub const SUBMARINE_THRUST_SHARE: f32 = 0.55;
/// Downward push of a submerged rig diving (drill held off the ground, px/s²).
pub const SUBMARINE_DIVE_ACCELERATION: f32 = 520.0;
/// Fastest a submerged rig moves in any direction (px/s).
pub const SUBMARINE_TOP_SPEED: f32 = 150.0;
/// Fuel a submerged rig's ballast pumps burn per second while it steers (on top of thrust).
pub const SUBMARINE_FUEL_PER_SECOND: f32 = 0.12;
/// Rows of fluid a stock hull takes before pressure starts to crush it.
pub const STOCK_PRESSURE_ROWS: i32 = 24;
/// Hull lost per second per row of fluid past the hull's pressure rating.
pub const PRESSURE_DAMAGE_PER_ROW: f32 = 0.5;

/// What a rig brings into fluid: the upgrade seam for Descent IV gear.
///
/// The Pressure Hull raises [`Self::pressure_rows`], the Bio-Filter [`Self::acid_resistance`],
/// the Cryo Hull or Lava Plating [`Self::heat_resistance`]. Read by
/// `WorldState::submersion_gear`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubmersionGear {
    /// Rows of fluid overhead the hull takes without damage.
    pub pressure_rows: i32,
    /// Share of acid damage kept out (0..=1).
    pub acid_resistance: f32,
    /// Share of liquid-metal heat kept out (0..=1).
    pub heat_resistance: f32,
    /// Multiplier on submarine handling (top speed and thrust; 1 is stock).
    pub handling: f32,
}

impl Default for SubmersionGear {
    fn default() -> Self {
        Self {
            pressure_rows: STOCK_PRESSURE_ROWS,
            acid_resistance: 0.0,
            heat_resistance: 0.0,
            handling: 1.0,
        }
    }
}

/// The shape of a generated fluid body ([`FluidPocketRule`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FluidPocketShape {
    /// A sealed reservoir inside the rock (an aquifer): it floods whatever tunnel breaches it.
    Sealed,
    /// Still pools on the floors of open caves.
    CavePool,
}

/// One kind of fluid body a stratum generates ([`fluid_pockets_in_chunk`]): the V11 stratum
/// registry lists these per stratum (the seam F8 wires).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FluidPocketRule {
    pub kind: FluidKind,
    pub shape: FluidPocketShape,
    /// Chance per 16x16 chunk, in thousandths.
    pub per_mille: u16,
    /// Half-width range of a sealed pocket (tiles; clamped to fit inside the chunk).
    pub min_radius: u8,
    pub max_radius: u8,
}

impl FluidPocketRule {
    /// A sealed reservoir of `kind`.
    #[must_use]
    pub const fn sealed(kind: FluidKind, per_mille: u16) -> Self {
        Self {
            kind,
            shape: FluidPocketShape::Sealed,
            per_mille,
            min_radius: 2,
            max_radius: 5,
        }
    }

    /// Cave-floor pools of `kind`.
    #[must_use]
    pub const fn cave_pools(kind: FluidKind, per_mille: u16) -> Self {
        Self {
            kind,
            shape: FluidPocketShape::CavePool,
            per_mille,
            min_radius: 1,
            max_radius: 3,
        }
    }
}

/// splitmix64 over a chunk, a rule index and the seed (stable on every target).
const fn pocket_hash(seed: u64, chunk: ChunkPosition, salt: u64) -> u64 {
    let key = ((chunk.x.cast_unsigned() as u64) << 32) | (chunk.y.cast_unsigned() as u64);
    let mut z = key
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(seed.wrapping_mul(0xD1B5_4A32_D192_ED03))
        .wrapping_add(salt.wrapping_mul(0xA24B_AED4_963E_E407));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `roll % span` for a small positive span, as an `i32`.
fn roll_below(roll: u64, span: i32) -> i32 {
    let span = u64::try_from(span.max(1)).unwrap_or(1);
    i32::try_from(roll % span).unwrap_or(0)
}

/// What the chunk holds at `at` once `placed` is written over `tile_at`.
fn kind_with(
    placed: &[(TilePosition, Tile)],
    tile_at: &impl Fn(TilePosition) -> TileKind,
    at: TilePosition,
) -> TileKind {
    placed
        .iter()
        .find(|(position, _)| *position == at)
        .map_or_else(|| tile_at(at), |(_, tile)| tile.kind)
}

/// Diggable rock a sealed pocket may replace or be walled by.
const fn pocket_rock(kind: TileKind) -> bool {
    !kind.is_open()
        && !crate::terrain::is_indestructible(kind)
        && !matches!(
            kind,
            TileKind::Lava
                | TileKind::Gas
                | TileKind::ExplosivePocket
                | TileKind::PressurePocket
                | TileKind::MagmaVent
                | TileKind::Poi(_)
                | TileKind::Built(_)
                | TileKind::Foundation
        )
}

/// The fluid generation hook: the fluid tiles `rules` put in `chunk` of a world with `seed`,
/// given what the chunk holds so far (`tile_at`, called only for tiles inside the chunk).
///
/// Pure and chunk-local (every tile it returns and every tile it reads is inside `chunk`), so a
/// streamed generator can call it per chunk, after carving caves, for the stratum the chunk is in.
/// Sealed pockets only replace solid, diggable rock whose whole rim stays rock (so the pocket is
/// enclosed and sits still until something breaches it); cave pools fill the bottom rows of open
/// cave basins (runs of open tiles walled on both sides with ground under every tile). Both are
/// full tiles, so generated fluid never moves until a neighbour changes. The V1-V10 generators
/// never call it.
#[must_use]
pub fn fluid_pockets_in_chunk(
    seed: u64,
    chunk: ChunkPosition,
    rules: &[FluidPocketRule],
    tile_at: impl Fn(TilePosition) -> TileKind,
) -> Vec<(TilePosition, Tile)> {
    let mut placed: Vec<(TilePosition, Tile)> = Vec::new();
    for (index, rule) in (1_u64..).zip(rules) {
        let roll = pocket_hash(seed, chunk, index);
        if roll % 1_000 >= u64::from(rule.per_mille) {
            continue;
        }
        match rule.shape {
            FluidPocketShape::Sealed => sealed_pocket(chunk, *rule, roll, &tile_at, &mut placed),
            FluidPocketShape::CavePool => cave_pools(chunk, *rule, &tile_at, &mut placed),
        }
    }
    placed
}

/// One sealed, elliptical pocket inside `chunk` (with a rock rim inside the chunk too).
fn sealed_pocket(
    chunk: ChunkPosition,
    rule: FluidPocketRule,
    roll: u64,
    tile_at: &impl Fn(TilePosition) -> TileKind,
    placed: &mut Vec<(TilePosition, Tile)>,
) {
    let origin = chunk.origin();
    let size = CHUNK_SIZE_TILES;
    let span = i32::from(rule.max_radius.saturating_sub(rule.min_radius)) + 1;
    // Fits inside the chunk with a rock rim: at most (size - 4) / 2 wide each way.
    let max_fit = (size - 4) / 2;
    let rx = (i32::from(rule.min_radius) + roll_below(roll >> 12, span)).clamp(1, max_fit);
    let ry = (rx * 2 / 3).max(1);
    let cx = origin.x + 2 + rx + roll_below(roll >> 20, size - 4 - 2 * rx + 1);
    let cy = origin.y + 2 + ry + roll_below(roll >> 28, size - 4 - 2 * ry + 1);
    let inside = |at: TilePosition| {
        let (dx, dy) = (at.x - cx, at.y - cy);
        dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry
    };
    let cells: Vec<TilePosition> = (cy - ry..=cy + ry)
        .flat_map(|y| (cx - rx..=cx + rx).map(move |x| TilePosition { x, y }))
        .filter(|at| inside(*at))
        .collect();
    let sealed = cells.iter().all(|at| {
        pocket_rock(kind_with(placed, tile_at, *at))
            && [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().all(|(dx, dy)| {
                let next = TilePosition {
                    x: at.x + dx,
                    y: at.y + dy,
                };
                inside(next) || pocket_rock(kind_with(placed, tile_at, next))
            })
    });
    if sealed {
        placed.extend(
            cells
                .into_iter()
                .map(|at| (at, FluidCell::full(rule.kind).tile())),
        );
    }
}

/// Fills the bottom rows (at most `rule.max_radius` deep) of every walled, floored basin of
/// open tiles in `chunk`.
fn cave_pools(
    chunk: ChunkPosition,
    rule: FluidPocketRule,
    tile_at: &impl Fn(TilePosition) -> TileKind,
    placed: &mut Vec<(TilePosition, Tile)>,
) {
    let origin = chunk.origin();
    let last = origin.x + CHUNK_SIZE_TILES - 1;
    let depth = usize::from(rule.max_radius.max(1));
    let solid = |placed: &[(TilePosition, Tile)], at: TilePosition| {
        !kind_with(placed, tile_at, at).is_open()
    };
    let wet = |placed: &[(TilePosition, Tile)], at: TilePosition| {
        matches!(kind_with(placed, tile_at, at), TileKind::Fluid(_))
    };
    let open = |placed: &[(TilePosition, Tile)], at: TilePosition| {
        tile_at(at) == TileKind::Air && !placed.iter().any(|(position, _)| *position == at)
    };
    // Bottom-up, so a pool's lower rows exist before the rows over them.
    for y in (origin.y + 1..origin.y + CHUNK_SIZE_TILES - 1).rev() {
        let mut x = origin.x + 1;
        while x < last {
            if !open(placed, TilePosition { x, y }) || !solid(placed, TilePosition { x: x - 1, y })
            {
                x += 1;
                continue;
            }
            // A run of open tiles from x, walled at both ends, floored throughout.
            let mut end = x;
            while end < last && open(placed, TilePosition { x: end, y }) {
                end += 1;
            }
            let walled = end < last && solid(placed, TilePosition { x: end, y });
            let floored = (x..end).all(|fx| {
                let below = TilePosition { x: fx, y: y + 1 };
                solid(placed, below) || wet(placed, below)
            });
            // Never deeper than the rule allows: count the pool rows already under.
            let under = (1..=i32::try_from(depth).unwrap_or(1))
                .take_while(|dy| wet(placed, TilePosition { x, y: y + dy }))
                .count();
            if walled && floored && under < depth {
                for fx in x..end {
                    placed.push((TilePosition { x: fx, y }, FluidCell::full(rule.kind).tile()));
                }
            }
            x = end + 1;
        }
    }
}

/// A demonstration set of basins, one per fluid kind left to right.
///
/// For the test-mode "test pools" command, tests and screenshot scenes: tile writes with their
/// top left at `origin` (13 rows tall, 4 basins of 7 columns with rock walls between them).
///
/// Each basin is a carved room with a rock floor and walls, two thirds full; the room above
/// the fluid is open so a rig can fly over and dive in.
#[must_use]
pub fn demo_basins(origin: TilePosition) -> Vec<(TilePosition, Tile)> {
    const WIDTH: i32 = 7;
    const ROWS: i32 = 12;
    const FILLED: i32 = 7;
    let rock = Tile {
        kind: TileKind::Stone,
        durability: crate::terrain::tile_durability(TileKind::Stone),
    };
    let air = Tile {
        kind: TileKind::Air,
        durability: 0,
    };
    let mut writes = Vec::new();
    let basins = i32::try_from(FluidKind::ALL.len()).unwrap_or(4);
    let span = basins * (WIDTH + 1) + 1;
    for dy in 0..=ROWS {
        for dx in 0..span {
            let at = TilePosition {
                x: origin.x + dx,
                y: origin.y + dy,
            };
            let wall = dx % (WIDTH + 1) == 0;
            let tile = if dy == ROWS || (wall && dy >= ROWS - FILLED - 1) {
                rock
            } else if dy >= ROWS - FILLED && !wall {
                let basin = usize::try_from(dx / (WIDTH + 1)).unwrap_or(0);
                FluidCell::full(FluidKind::ALL[basin.min(FluidKind::ALL.len() - 1)]).tile()
            } else {
                air
            };
            writes.push((at, tile));
        }
    }
    writes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_split_keeps_everything_and_compresses_downwards() {
        for total in 0..=500_u16 {
            let lower = stable_lower(total);
            assert!(lower <= total, "{total}");
            assert!(total - lower <= lower, "upper never exceeds lower: {total}");
        }
        assert_eq!(stable_lower(80), 80);
        assert_eq!(stable_lower(200), 103);
        assert!(stable_lower(400) > 200);
    }

    #[test]
    fn steps_follow_world_time_at_every_rate() {
        for hz in [20, 30, 60, 120] {
            assert_eq!(fluid_steps_by(u64::from(hz) * 10, hz), 200, "{hz} Hz");
        }
        assert_eq!(fluid_steps_by(2, 60), 0);
        assert_eq!(fluid_steps_by(3, 60), 1);
    }

    #[test]
    fn kinds_round_trip_their_codes() {
        for kind in FluidKind::ALL {
            assert_eq!(FluidKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(FluidKind::from_code(9), None);
    }

    #[test]
    fn sealed_pockets_stay_inside_their_chunk_and_inside_rock() {
        let rule = FluidPocketRule::sealed(FluidKind::Water, 1_000);
        let mut found = 0;
        for cx in -6..6 {
            let chunk = ChunkPosition { x: cx, y: 3 };
            let tiles = fluid_pockets_in_chunk(7, chunk, &[rule], |_| TileKind::Stone);
            assert!(!tiles.is_empty(), "a sure rule in solid rock always places");
            for (at, tile) in &tiles {
                assert_eq!(ChunkPosition::of(*at), chunk);
                assert_eq!(
                    FluidCell::of(*tile),
                    Some(FluidCell::full(FluidKind::Water))
                );
            }
            // Deterministic.
            assert_eq!(
                tiles,
                fluid_pockets_in_chunk(7, chunk, &[rule], |_| TileKind::Stone)
            );
            found += tiles.len();
        }
        assert!(found > 50);
        // Never next to open ground.
        let none =
            fluid_pockets_in_chunk(7, ChunkPosition { x: 0, y: 3 }, &[rule], |_| TileKind::Air);
        assert!(none.is_empty());
    }

    #[test]
    fn cave_pools_fill_walled_basins_only() {
        let rule = FluidPocketRule::cave_pools(FluidKind::Brine, 1_000);
        let chunk = ChunkPosition { x: 0, y: 0 };
        // A basin: open rows 4..=10 between columns 3 and 9, rock everywhere else.
        let basin = |at: TilePosition| {
            if (3..=9).contains(&at.x) && (4..=10).contains(&at.y) {
                TileKind::Air
            } else {
                TileKind::Stone
            }
        };
        let tiles = fluid_pockets_in_chunk(1, chunk, &[rule], basin);
        assert_eq!(tiles.len(), 7 * 3, "three rows of a seven-wide basin");
        assert!(tiles.iter().all(|(at, _)| (8..=10).contains(&at.y)));
    }

    #[test]
    fn demo_basins_hold_every_kind() {
        let writes = demo_basins(TilePosition { x: 0, y: 0 });
        for kind in FluidKind::ALL {
            assert!(
                writes
                    .iter()
                    .any(|(_, tile)| tile.kind == TileKind::Fluid(kind))
            );
        }
    }
}
