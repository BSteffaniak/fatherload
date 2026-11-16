//! The filled Moon of [`GeneratorVersion::V9`] worlds (expansion plan WP-MOON).
//!
//! V8 placed the Moon's footprint ([`super::sky::moon`]): one landmass 2,000-2,900 tiles wide
//! hanging above a fixed column, regolith over moon rock with craters. V9 fills it in. Like the
//! rest of the sky everything here is a pure integer function of the settings and the tile
//! coordinates, so chunks stay seamless and identical on every platform.
//!
//! **Layout** ([`MoonLayout`], [`layout`]): from one seeded roll per world,
//!
//! * **the lunar research colony** ([`MoonLayout::colony`]): a landing pad, a glass dome with
//!   its doors at the base, a hab module with windows and a comms mast, on ground flattened
//!   for it, 220-380 columns out from the Moon's centre;
//! * **the alien facility** ([`FacilityPlan`]) on the far side of the centre: a stepped temple
//!   whose door is an energy veil (pilots pass, rigs do not), a ledged shaft down to the puzzle
//!   hall (three dark pylons lit in the order their glyph marks show open the sealed gate),
//!   the Warden's arena (two power conduits that knock its shield down), the core seal that
//!   falls with the Warden, and a long ledged shaft down to
//! * **the core** ([`MoonLayout::core`]): a chamber at the Moon's heart inside an unbreakable
//!   shell, with the strange core floating in it;
//! * **abandoned habitats** (three broken huts with a relic cache), **lava tubes** (four long
//!   basalt-lined tunnels a rig or a pilot can explore, open to the sky through skylights at
//!   both ends), **ice fields**, **crystal forests** (alien crystal trees with glowing blooms)
//!   and **maria** (dark dust plains), kept clear of each other.
//!
//! **The body**: bright regolith (dark in the maria) over lunar rock, deep rock along the
//! underside (a long dig from below), helium-3 pockets in the dust, ice under the ice fields
//! and along the cold underside, alien alloy veins deep down, rare relic caches, and crystal
//! stalks with blooms hanging under it. Craters are V8's; blast craters stay forever.
//!
//! [`GeneratorVersion::V9`]: super::GeneratorVersion::V9

use std::cell::Cell;

use crate::{
    moon::LunarTile,
    terrain::{TileKind, TilePosition},
    worldgen::{
        TileRect, WorldSettings,
        noise::{NOISE_MAX, RegionRng, centred, fractal_noise, hash2, line_noise},
        sky::{SkyLand, moon},
    },
};

const SALT_LAYOUT: u64 = 0x300A_0001;
const SALT_BODY: u64 = 0x300A_0010;
const SALT_TUBE: u64 = 0x300A_0020;
const SALT_TREE: u64 = 0x300A_0030;
const SALT_HAB: u64 = 0x300A_0040;

/// Rows the colony's comms mast, the temple and the crystal trees may rise above the Moon's
/// highest hill (part of the Moon's bounds on V9).
pub const LUNAR_STRUCTURE_ROWS: i32 = 26;
/// Rows from the Moon's centre surface row down to the middle of the core chamber.
pub const CORE_DEPTH_ROWS: i32 = 118;
/// Radius of the air inside the core chamber.
pub const CORE_CHAMBER_RADIUS: i32 = 11;
/// Rows above the core's middle of the core shaft's lowest ledge (in the shell's veil).
pub const CORE_FOOT_LEDGE_ROWS: i32 = CORE_CHAMBER_RADIUS + 1;
/// Rows above the core's middle of the plinth under the chamber's veil (columns -1..=1).
pub const CORE_PLINTH_ROWS: i32 = 8;
/// Rows above the core's middle of the side plinth (columns +2..=+4).
pub const CORE_SIDE_PLINTH_ROWS: i32 = 6;
/// Rows under the temple's ground of the plinth beneath the temple shaft, in the puzzle hall.
pub const HALL_PLINTH_ROWS: i32 = 23;
/// Rows under the temple's ground of the hall's side plinth (2-4 columns toward the core).
pub const HALL_SIDE_PLINTH_ROWS: i32 = 26;
/// Rows below the core's middle where the chamber's floor starts (the heart floats above it).
pub const CORE_FLOOR_ROWS: i32 = 5;
/// Outer radius of the core's shell.
pub const CORE_SHELL_RADIUS: i32 = 14;
/// Half-width of the colony's flattened ground.
pub const COLONY_HALF_WIDTH: i32 = 40;
/// Radius of the colony's dome.
pub const DOME_RADIUS: i32 = 13;
/// Half-width of an abandoned habitat's site.
pub const HABITAT_HALF_WIDTH: i32 = 8;
/// Half-width of the facility temple's site.
pub const TEMPLE_HALF_WIDTH: i32 = 12;
/// Height of the comms mast.
const MAST_ROWS: i32 = 22;
/// Columns over which flattened ground eases back into the natural surface.
const FLATTEN_FADE: i32 = 14;
/// Rows of deep rock along the underside.
const DEEP_ROCK_ROWS: i32 = 28;

/// A flattened site on the surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Site {
    pub center_x: i32,
    pub half_width: i32,
    /// The surface row of the flattened ground.
    pub ground_row: i32,
}

impl Site {
    #[must_use]
    pub const fn covers(&self, x: i32) -> bool {
        (x - self.center_x).abs() <= self.half_width
    }

    /// Columns of the site plus the fade on both sides.
    #[must_use]
    pub const fn reach(&self) -> (i32, i32) {
        (
            self.center_x - self.half_width - FLATTEN_FADE,
            self.center_x + self.half_width + FLATTEN_FADE,
        )
    }
}

/// A span of columns (inclusive).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub first: i32,
    pub last: i32,
}

impl Span {
    #[must_use]
    pub const fn contains(&self, x: i32) -> bool {
        x >= self.first && x <= self.last
    }
}

/// A lava tube: a basalt-lined tunnel winding along `first..=last` around `row`, with
/// skylights to the surface at both ends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LavaTube {
    pub first: i32,
    pub last: i32,
    /// The tube's mean centre row.
    pub row: i32,
    pub seed: u64,
}

impl LavaTube {
    /// The centre row of the tube at column `x`.
    #[must_use]
    pub const fn center_row(&self, x: i32) -> i32 {
        self.row + centred(line_noise(self.seed, SALT_TUBE, x, 26), 4)
    }

    /// Whether `(x, y)` is open air inside the tube (3-4 rows tall).
    #[must_use]
    pub const fn is_open(&self, x: i32, y: i32) -> bool {
        if x < self.first || x > self.last {
            return false;
        }
        let dy = y - self.center_row(x);
        let tall = hash2(self.seed, SALT_TUBE + 1, x.div_euclid(5), 0).is_multiple_of(3);
        dy >= -1 && dy <= if tall { 2 } else { 1 }
    }

    /// Columns of the skylights (3 wide, at both ends).
    #[must_use]
    pub const fn skylight(&self, x: i32) -> bool {
        self.last > self.first
            && ((x >= self.first && x <= self.first + 2) || (x >= self.last - 2 && x <= self.last))
    }

    /// Whether the tube was placed (a crowded Moon may leave one out).
    #[must_use]
    pub const fn placed(&self) -> bool {
        self.last > self.first
    }
}

/// The alien facility: every room as a rectangle of open air (walls are the facility stone
/// around them).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FacilityPlan {
    /// The temple on the surface (its site).
    pub temple: Site,
    /// Which way the facility runs under the surface (toward the Moon's centre): 1 or -1.
    pub direction: i32,
    /// The outer tile of the veil doorway into the temple ([`Self::in_door`]).
    pub door: TilePosition,
    /// The temple's inner room.
    pub temple_room: TileRect,
    /// The shaft from the temple floor down to the hall.
    pub shaft: TileRect,
    /// The puzzle hall.
    pub hall: TileRect,
    /// The three pylons on the hall floor, in hall order.
    pub pylons: [TilePosition; 3],
    /// The lighting order: `order[i]` is pylon `i`'s place in the sequence (0 first). Its
    /// glyph mark shows `order[i] + 1` dots.
    pub order: [u8; 3],
    /// The sealed gate between the hall and the arena.
    pub gate: TileRect,
    /// The Warden's arena.
    pub arena: TileRect,
    /// The arena's two power conduits.
    pub conduits: [TilePosition; 2],
    /// The core seal at the arena's far end.
    pub seal: TileRect,
    /// The corridor from the seal to the core shaft.
    pub corridor: TileRect,
    /// The shaft from the corridor down to the core chamber.
    pub core_shaft: TileRect,
}

impl FacilityPlan {
    /// Every room (open air), the temple room first.
    #[must_use]
    pub const fn rooms(&self) -> [TileRect; 6] {
        [
            self.temple_room,
            self.shaft,
            self.hall,
            self.arena,
            self.corridor,
            self.core_shaft,
        ]
    }

    /// Everything the facility occupies, walls included.
    #[must_use]
    pub fn bounds(&self) -> TileRect {
        let mut bounds = self.temple_room;
        for room in self.rooms() {
            bounds = union(bounds, room);
        }
        union(
            bounds.expanded(2),
            TileRect::around(
                TilePosition {
                    x: self.temple.center_x,
                    y: self.temple.ground_row - 4,
                },
                TEMPLE_HALF_WIDTH,
                5,
            ),
        )
    }

    /// Whether `position` is part of the veil doorway (through the temple's stepped wall to
    /// its room, two rows tall).
    #[must_use]
    pub const fn in_door(&self, position: TilePosition) -> bool {
        let into = (position.x - self.door.x) * self.direction;
        into >= 0 && into <= 4 && position.y >= self.door.y - 1 && position.y <= self.door.y
    }

    /// The point a Warden hovers around: the arena's middle.
    #[must_use]
    pub const fn arena_center(&self) -> TilePosition {
        self.arena.center()
    }
}

/// Where everything on the Moon is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoonLayout {
    pub moon: SkyLand,
    pub colony: Site,
    pub habitats: [Site; 3],
    pub facility: FacilityPlan,
    /// The middle of the core chamber.
    pub core: TilePosition,
    pub tubes: [LavaTube; 4],
    pub ice_fields: [Span; 3],
    pub forests: [Span; 3],
    pub maria: [Span; 2],
}

const fn rect(x0: i32, x1: i32, y0: i32, y1: i32) -> TileRect {
    let (min_x, max_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (min_y, max_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    TileRect {
        min: TilePosition { x: min_x, y: min_y },
        max: TilePosition { x: max_x, y: max_y },
    }
}

const fn union(a: TileRect, b: TileRect) -> TileRect {
    rect(
        if a.min.x < b.min.x { a.min.x } else { b.min.x },
        if a.max.x > b.max.x { a.max.x } else { b.max.x },
        if a.min.y < b.min.y { a.min.y } else { b.min.y },
        if a.max.y > b.max.y { a.max.y } else { b.max.y },
    )
}

thread_local! {
    /// The last layout built (one Moon per world; a miss only costs time).
    static LAST: Cell<Option<MoonLayout>> = const { Cell::new(None) };
}

/// The layout of this world's filled Moon (`None` unless the generator fills it, see
/// [`super::GeneratorVersion::has_moon_content`]).
#[must_use]
pub fn layout(settings: &WorldSettings) -> Option<MoonLayout> {
    let land = moon(settings)?;
    land.is_lunar().then(|| layout_of(&land))
}

/// The layout of filled Moon `land` (memoized per thread).
#[must_use]
pub fn layout_of(land: &SkyLand) -> MoonLayout {
    if let Some(cached) = LAST.with(Cell::get)
        && cached.moon == *land
    {
        return cached;
    }
    let built = build(land);
    LAST.with(|last| last.set(Some(built)));
    built
}

/// The natural surface of the Moon at `x` before any site flattening (V8's shape).
fn natural_surface(land: &SkyLand, x: i32) -> i32 {
    land.raw_top(x)
}

/// The ground row a site centred at `x` is flattened to: the hills there, ignoring craters.
const fn site_ground(land: &SkyLand, x: i32) -> i32 {
    land.top_row - land.moon_hills(x)
}

const fn site(land: &SkyLand, center_x: i32, half_width: i32) -> Site {
    Site {
        center_x,
        half_width,
        ground_row: site_ground(land, center_x),
    }
}

#[allow(clippy::too_many_lines, reason = "one roll lays out the whole Moon")]
fn build(land: &SkyLand) -> MoonLayout {
    let mut rng = RegionRng::new(land.shape_seed(), SALT_LAYOUT, 0, 0);
    let c = land.center_x;
    let hw = land.half_width;
    let facility_side = if rng.chance(500) { 1 } else { -1 };
    // The facility on one side of the centre, running toward it; the colony on the other.
    let temple_x = c + facility_side * rng.range(70, 115);
    let colony_x = c - facility_side * rng.range(220, 380);
    let colony = site(land, colony_x, COLONY_HALF_WIDTH);
    let temple = site(land, temple_x, TEMPLE_HALF_WIDTH);
    let core = TilePosition {
        x: c,
        y: land.top_row + CORE_DEPTH_ROWS,
    };
    let facility = plan_facility(&mut rng, temple, -facility_side, core);

    let mut taken: Vec<(i32, i32)> = vec![
        colony.reach(),
        temple.reach(),
        (facility.bounds().min.x - 4, facility.bounds().max.x + 4),
    ];
    let usable = (hw * 3 / 4).max(200);
    let free = |taken: &Vec<(i32, i32)>, first: i32, last: i32, margin: i32| {
        taken
            .iter()
            .all(|(a, b)| last + margin < *a || first - margin > *b)
    };
    // Habitats: spread over both halves.
    let mut habitats = [colony; 3];
    for (index, habitat) in habitats.iter_mut().enumerate() {
        let mut placed = None;
        for _ in 0..24 {
            let side = if (index + usize::from(rng.chance(300))) % 2 == 0 {
                1
            } else {
                -1
            };
            let x = c + side * rng.range(160, usable);
            let (first, last) = (x - HABITAT_HALF_WIDTH, x + HABITAT_HALF_WIDTH);
            if free(&taken, first, last, 30) {
                placed = Some(site(land, x, HABITAT_HALF_WIDTH));
                break;
            }
        }
        // A cramped Moon (never in practice): stack it beside the colony.
        let fallback = site(
            land,
            colony_x
                + facility_side * (COLONY_HALF_WIDTH + 30 + 30 * i32::try_from(index).unwrap_or(0)),
            HABITAT_HALF_WIDTH,
        );
        *habitat = placed.unwrap_or(fallback);
        taken.push(habitat.reach());
    }
    // Lava tubes: long tunnels just under the surface, kept off the sites (they may run under
    // the forests and ice fields, which only cover the top).
    let sites = taken.clone();
    let mut tubes = [LavaTube {
        first: 0,
        last: -1,
        row: 0,
        seed: 0,
    }; 4];
    let mut tube_spans: Vec<(i32, i32)> = Vec::new();
    for tube in &mut tubes {
        for attempt in 0..120 {
            // Shorter tubes once the long ones stop fitting.
            let length = rng.range(70, (200 - attempt).max(80));
            let first = c + rng.range(-usable, usable - length);
            let last = first + length;
            if free(&sites, first, last, 12) && free(&tube_spans, first, last, 20) {
                // Deep enough under the lowest point of the surface along it (craters
                // included) that its roof holds.
                let lowest = (first..=last)
                    .step_by(2)
                    .map(|x| natural_surface(land, x))
                    .max()
                    .unwrap_or(land.top_row);
                *tube = LavaTube {
                    first,
                    last,
                    row: lowest + rng.range(8, 12),
                    seed: rng.next_u64(),
                };
                tube_spans.push((first, last));
                break;
            }
        }
    }
    let mut spans = |count: usize, low: i32, high: i32| {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let mut found = Span { first: 0, last: -1 };
            for _ in 0..48 {
                let width = rng.range(low, high);
                let first = c + rng.range(-usable, usable - width);
                let span = Span {
                    first,
                    last: first + width,
                };
                if free(&taken, span.first, span.last, 6) {
                    found = span;
                    break;
                }
            }
            if found.last >= found.first {
                taken.push((found.first, found.last));
            }
            out.push(found);
        }
        out
    };
    let forest = spans(3, 50, 110);
    let ice = spans(3, 40, 90);
    // Maria may lie under anything (they only darken the dust).
    let mut maria = [Span { first: 0, last: -1 }; 2];
    for mare in &mut maria {
        let width = rng.range(150, 400);
        let first = c + rng.range(-hw + 200, hw - 200 - width);
        *mare = Span {
            first,
            last: first + width,
        };
    }
    MoonLayout {
        moon: *land,
        colony,
        habitats,
        facility,
        core,
        tubes,
        ice_fields: [ice[0], ice[1], ice[2]],
        forests: [forest[0], forest[1], forest[2]],
        maria,
    }
}

/// Lays the facility out from its temple, running in `direction` toward the core.
fn plan_facility(
    rng: &mut RegionRng,
    temple: Site,
    direction: i32,
    core: TilePosition,
) -> FacilityPlan {
    let g = temple.ground_row;
    let fx = temple.center_x;
    let d = direction;
    // The temple room and its veil door on the side away from the core.
    let temple_room = rect(fx - 6, fx + 6, g - 5, g - 1);
    let door = TilePosition {
        x: fx - d * (TEMPLE_HALF_WIDTH - 1),
        y: g - 1,
    };
    // (The door runs from its outer tile `fx - d * 11` in to `fx - d * 7`, beside the room.)
    let shaft = rect(fx - 1, fx + 1, g, g + 20);
    let hall = rect(fx - d * 3, fx + d * 43, g + 21, g + 29);
    let pylons = [10, 22, 34].map(|step| TilePosition {
        x: fx + d * step,
        y: g + 29,
    });
    // A shuffled lighting order.
    let mut order = [0_u8, 1, 2];
    for index in (1..3).rev() {
        let pick = usize::try_from(rng.range(0, i32::try_from(index).unwrap_or(0))).unwrap_or(0);
        order.swap(index, pick);
    }
    if order == [0, 1, 2] {
        // Never left to right: the marks matter.
        order = [1, 2, 0];
    }
    let gate = rect(fx + d * 44, fx + d * 45, g + 21, g + 29);
    let arena = rect(fx + d * 46, fx + d * 80, g + 17, g + 33);
    let conduits = [
        TilePosition {
            x: fx + d * 52,
            y: g + 33,
        },
        TilePosition {
            x: fx + d * 74,
            y: g + 33,
        },
    ];
    let seal = rect(fx + d * 81, fx + d * 82, g + 27, g + 33);
    // From the seal to past the core shaft, whichever side of the centre the arena ends.
    let corridor = union(
        rect(fx + d * 83, core.x, g + 30, g + 33),
        rect(core.x - 2, core.x + 2, g + 30, g + 33),
    );
    let core_shaft = rect(core.x - 1, core.x + 1, g + 34, core.y - CORE_CHAMBER_RADIUS);
    FacilityPlan {
        temple,
        direction: d,
        door,
        temple_room,
        shaft,
        hall,
        pylons,
        order,
        gate,
        arena,
        conduits,
        seal,
        corridor,
        core_shaft,
    }
}

impl MoonLayout {
    /// The Moon's surface row at `x` (flattened under the sites), `None` past its limbs.
    #[must_use]
    pub fn surface_row(&self, x: i32) -> Option<i32> {
        if (x - self.moon.center_x).abs() > self.moon.half_width {
            return None;
        }
        Some(self.surface(x))
    }

    fn surface(&self, x: i32) -> i32 {
        let raw = natural_surface(&self.moon, x);
        let sites = [
            self.colony,
            self.facility.temple,
            self.habitats[0],
            self.habitats[1],
            self.habitats[2],
        ];
        for site in sites {
            let (first, last) = site.reach();
            if x < first || x > last {
                continue;
            }
            if site.covers(x) {
                return site.ground_row;
            }
            let outside = (x - site.center_x).abs() - site.half_width;
            return site.ground_row + (raw - site.ground_row) * outside / FLATTEN_FADE;
        }
        raw
    }

    /// The rectangle the colony occupies (its services and safety reach this far).
    #[must_use]
    pub const fn colony_area(&self) -> TileRect {
        rect(
            self.colony.center_x - COLONY_HALF_WIDTH,
            self.colony.center_x + COLONY_HALF_WIDTH,
            self.colony.ground_row - MAST_ROWS - 2,
            self.colony.ground_row + 2,
        )
    }

    /// Whether `(x, y)` is inside the colony's pressurized rooms (the dome or the hab
    /// module): a pilot breathes there.
    #[must_use]
    pub const fn colony_pressurized(&self, x: i32, y: i32) -> bool {
        let g = self.colony.ground_row;
        let cx = self.colony.center_x;
        if y >= g {
            return false;
        }
        let (dx, dy) = (x - cx, y - g);
        if dx * dx + dy * dy < (DOME_RADIUS - 1) * (DOME_RADIUS - 1) {
            return true;
        }
        x > cx + 16 && x < cx + 30 && y > g - 6
    }

    /// Whether `(x, y)` holds the alien facility's ancient air: its rooms behind the veil
    /// (which keeps the air in and lets a pilot through) and the core chamber. The dungeon is
    /// long and on foot, far past the rig's air line, so the pilot breathes inside.
    #[must_use]
    pub fn facility_air(&self, x: i32, y: i32) -> bool {
        let position = TilePosition { x, y };
        self.in_core_chamber(x, y)
            || self.facility.in_door(position)
            || self
                .facility
                .rooms()
                .iter()
                .any(|room| room.contains(position))
    }

    /// Whether `(x, y)` has air on the Moon: the colony's pressurized rooms
    /// ([`Self::colony_pressurized`]) or the facility ([`Self::facility_air`]).
    #[must_use]
    pub fn pressurized(&self, x: i32, y: i32) -> bool {
        self.colony_pressurized(x, y) || self.facility_air(x, y)
    }

    /// The landing pad's columns and row.
    #[must_use]
    pub const fn pad(&self) -> (i32, i32, i32) {
        let cx = self.colony.center_x;
        (cx - 37, cx - 19, self.colony.ground_row)
    }

    /// The lava tube whose skylight or tunnel holds `(x, y)`, if any.
    #[must_use]
    pub fn tube_at(&self, x: i32, y: i32) -> Option<&LavaTube> {
        self.tubes.iter().find(|tube| tube.is_open(x, y))
    }

    /// Whether `(x, y)` is inside the core chamber (air or the core).
    #[must_use]
    pub const fn in_core_chamber(&self, x: i32, y: i32) -> bool {
        let (dx, dy) = (x - self.core.x, y - self.core.y);
        dx * dx + dy * dy <= CORE_CHAMBER_RADIUS * CORE_CHAMBER_RADIUS
    }

    /// The tile at `(x, y)` (`None`: open air).
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "one layer after another, top to bottom"
    )]
    pub fn tile_at(&self, x: i32, y: i32) -> Option<TileKind> {
        let land = &self.moon;
        let dx = x - land.center_x;
        if dx.abs() > land.half_width {
            return None;
        }
        let lunar = |tile| Some(TileKind::Lunar(tile));
        // The core: a chamber inside an unbreakable shell, the core floating in the middle.
        let (cdx, cdy) = (x - self.core.x, y - self.core.y);
        let core_d2 = cdx * cdx + cdy * cdy;
        if core_d2 <= CORE_SHELL_RADIUS * CORE_SHELL_RADIUS {
            if cdx.abs() + cdy.abs() <= 2 {
                return lunar(LunarTile::CoreHeart);
            }
            if core_d2 <= CORE_CHAMBER_RADIUS * CORE_CHAMBER_RADIUS {
                // A floor under the floating heart, and two glowing plinths that step back up
                // to the shaft's lowest ledge for a pilot without a jetpack: heart's crown ->
                // the side plinth -> the plinth under the veil -> the ledge.
                if cdy >= CORE_FLOOR_ROWS {
                    return lunar(LunarTile::FacilityWall);
                }
                if (cdy == -CORE_SIDE_PLINTH_ROWS && (2..=4).contains(&cdx))
                    || (cdy == -CORE_PLINTH_ROWS && cdx.abs() <= 1)
                {
                    return lunar(LunarTile::FacilityGlyph);
                }
                return None;
            }
            // The shaft from the facility comes in through a veil at the top, over the
            // shaft's lowest ledge (its right-hand two columns, just inside the shell).
            if cdy < 0 && cdx.abs() <= 1 {
                if cdy == -CORE_FOOT_LEDGE_ROWS && cdx >= 0 {
                    return lunar(LunarTile::FacilityWall);
                }
                return lunar(LunarTile::Veil);
            }
            return lunar(LunarTile::CoreShell);
        }
        if let Some(tile) = self.facility_tile(x, y) {
            return tile;
        }
        let surface = self.surface(x);
        if let Some(tile) = self.colony_tile(x, y, surface) {
            return tile;
        }
        for habitat in &self.habitats {
            if let Some(tile) = habitat_tile(habitat, land.shape_seed(), x, y) {
                return tile;
            }
        }
        if y < surface {
            return self.tree_tile(x, y);
        }
        // Lava tubes and their skylights.
        for tube in &self.tubes {
            if tube.is_open(x, y) {
                return None;
            }
            if tube.skylight(x) && y < tube.center_row(x) {
                return None;
            }
        }
        let bottom = land.raw_bottom(x, surface);
        if y > bottom {
            return self.dangling(x, y - bottom);
        }
        lunar(self.body_tile(x, y, surface, bottom))
    }

    /// Facility stone, rooms, the puzzle pieces and the veil (`None`: not the facility;
    /// `Some(None)`: a room's open air).
    #[allow(
        clippy::option_option,
        reason = "outside / claimed air / a tile are three different answers"
    )]
    fn facility_tile(&self, x: i32, y: i32) -> Option<Option<TileKind>> {
        let plan = &self.facility;
        let position = TilePosition { x, y };
        if !plan.bounds().contains(position) {
            return None;
        }
        let lunar = |tile| Some(Some(TileKind::Lunar(tile)));
        // The puzzle pieces first: they sit inside the rooms.
        if let Some(index) = plan.pylons.iter().position(|p| *p == position) {
            let _ = index;
            return lunar(LunarTile::Pylon);
        }
        if plan.conduits.contains(&position) {
            return lunar(LunarTile::Conduit);
        }
        if plan.gate.contains(position) {
            return lunar(LunarTile::Gate);
        }
        if plan.seal.contains(position) {
            return lunar(LunarTile::CoreSeal);
        }
        if plan.in_door(position) {
            return lunar(LunarTile::Veil);
        }
        // Ledges in the shafts: every fourth row up from the shaft's foot, alternating sides,
        // two of the three columns wide so the middle never drops straight through (a free
        // fall down the core shaft in low gravity would kill an unarmoured pilot). Each foot
        // ledge is the right-hand one; plinths below lead back up to it (the hall's here, the
        // core chamber's in `tile_at`).
        for (shaft, foot) in [
            (plan.shaft, plan.shaft.max.y - 1),
            (plan.core_shaft, self.core.y - CORE_FOOT_LEDGE_ROWS),
        ] {
            if shaft.contains(position) {
                let step = foot - y;
                if y - shaft.min.y > 2 && step >= 0 && step % 4 == 0 {
                    let left = (step / 4) % 2 == 1;
                    if (left && x < shaft.max.x) || (!left && x > shaft.min.x) {
                        return lunar(LunarTile::FacilityWall);
                    }
                }
                return Some(None);
            }
        }
        // Two glowing plinths in the hall under the temple shaft: the hall floor -> the side
        // plinth -> the plinth under the shaft -> its foot ledge.
        let (sx, sy) = (
            (x - plan.temple.center_x) * plan.direction,
            y - plan.temple.ground_row,
        );
        if (sy == HALL_PLINTH_ROWS && sx.abs() <= 1)
            || (sy == HALL_SIDE_PLINTH_ROWS && (2..=4).contains(&sx))
        {
            return lunar(LunarTile::FacilityGlyph);
        }
        if plan.rooms().iter().any(|room| room.contains(position)) {
            return Some(None);
        }
        // Walls: two tiles around every buried room (the temple room has the temple).
        if plan
            .rooms()
            .iter()
            .skip(1)
            .any(|room| room.expanded(2).contains(position))
        {
            let glyph = hash2(self.moon.shape_seed(), SALT_BODY + 7, x, y).is_multiple_of(9);
            return lunar(if glyph {
                LunarTile::FacilityGlyph
            } else {
                LunarTile::FacilityWall
            });
        }
        // The stepped temple above the ground.
        let t = &plan.temple;
        let height = t.ground_row - y;
        if (1..=8).contains(&height) {
            let half = TEMPLE_HALF_WIDTH - height;
            if (x - t.center_x).abs() <= half {
                let glyph = (height == 3 || height == 7) && (x - t.center_x).abs() % 3 == 0;
                return lunar(if glyph {
                    LunarTile::FacilityGlyph
                } else {
                    LunarTile::FacilityWall
                });
            }
        }
        None
    }

    /// The colony's pad, dome, hab module and mast (`None`: not the colony; `Some(None)`: its
    /// open air).
    #[allow(
        clippy::option_option,
        reason = "outside / claimed air / a tile are three different answers"
    )]
    fn colony_tile(&self, x: i32, y: i32, surface: i32) -> Option<Option<TileKind>> {
        let cx = self.colony.center_x;
        let g = self.colony.ground_row;
        if (x - cx).abs() > COLONY_HALF_WIDTH || y < g - MAST_ROWS - 1 || y > g {
            return None;
        }
        let lunar = |tile| Some(Some(TileKind::Lunar(tile)));
        let (pad_first, pad_last, _) = self.pad();
        if y == g && ((pad_first..=pad_last).contains(&x) || (x - cx).abs() < DOME_RADIUS) {
            return lunar(LunarTile::ColonyPad);
        }
        if y == g - 1 && (x == pad_first || x == pad_last) {
            // Pad beacons.
            return lunar(LunarTile::ColonyHull);
        }
        // The dome: a ring, glass up high, hull low down, doors at both sides of its base.
        let (dx, dy) = (x - cx, y - g);
        let d2 = dx * dx + dy * dy;
        if dy < 0 && d2 < DOME_RADIUS * DOME_RADIUS {
            if d2 >= (DOME_RADIUS - 1) * (DOME_RADIUS - 1) {
                if dy >= -3 {
                    // Doors.
                    return Some(None);
                }
                return lunar(if dy < -5 {
                    LunarTile::ColonyGlass
                } else {
                    LunarTile::ColonyHull
                });
            }
            return Some(None);
        }
        // The hab module: hull walls with windows, a door on the dome side.
        if x >= cx + 16 && x <= cx + 30 && y >= g - 6 && y < g {
            let wall = x == cx + 16 || x == cx + 30 || y == g - 6;
            if !wall {
                return Some(None);
            }
            if x == cx + 16 && y >= g - 3 {
                return Some(None);
            }
            let window = (x == cx + 30 && (y == g - 4 || y == g - 3))
                || (y == g - 6 && (x - cx) % 4 == 0 && x != cx + 16 && x != cx + 30);
            return lunar(if window {
                LunarTile::ColonyGlass
            } else {
                LunarTile::ColonyHull
            });
        }
        if y == g && x > cx + 16 && x < cx + 30 {
            return lunar(LunarTile::ColonyPad);
        }
        // The comms mast.
        if x == cx + 35 && y < g && y >= g - MAST_ROWS {
            return lunar(LunarTile::ColonyHull);
        }
        if y < surface {
            return Some(None);
        }
        None
    }

    /// Alien crystal trees in the crystal forests (and their blooms).
    fn tree_tile(&self, x: i32, y: i32) -> Option<TileKind> {
        if !self.forests.iter().any(|forest| {
            forest.last >= forest.first && x >= forest.first - 1 && x <= forest.last + 1
        }) {
            return None;
        }
        for tx in x - 1..=x + 1 {
            if !self.forests.iter().any(|forest| forest.contains(tx)) {
                continue;
            }
            let roll = hash2(self.moon.shape_seed(), SALT_TREE, tx, 0);
            // About one column in five holds a tree, never two side by side.
            if !roll.is_multiple_of(5)
                || hash2(self.moon.shape_seed(), SALT_TREE, tx - 1, 0).is_multiple_of(5)
            {
                continue;
            }
            let ground = self.surface(tx);
            let height = 4 + i32::try_from((roll >> 8) % 6).unwrap_or(0);
            let top = ground - height;
            if tx == x {
                if y >= top && y < ground {
                    return Some(TileKind::Lunar(LunarTile::CrystalStalk));
                }
                if y == top - 1 {
                    return Some(TileKind::Lunar(LunarTile::CrystalBloom));
                }
            } else if height >= 5 && (y == top + 1 || (height >= 7 && y == top + 3)) {
                // Branches tipped with blooms.
                let side_ok = ((roll >> 16) & 1 == 0) == (x < tx) || height >= 7;
                if side_ok {
                    return Some(TileKind::Lunar(LunarTile::CrystalBloom));
                }
            }
        }
        None
    }

    /// Crystal stalks hanging `below` rows under the underside.
    fn dangling(&self, x: i32, below: i32) -> Option<TileKind> {
        let roll = hash2(self.moon.shape_seed(), SALT_BODY + 4, x, 0);
        if !roll.is_multiple_of(6) {
            return None;
        }
        let length = 1 + i32::try_from((roll >> 8) % 9).unwrap_or(0);
        match below.cmp(&length) {
            std::cmp::Ordering::Less => Some(TileKind::Lunar(LunarTile::CrystalStalk)),
            std::cmp::Ordering::Equal => Some(TileKind::Lunar(LunarTile::CrystalBloom)),
            std::cmp::Ordering::Greater => None,
        }
    }

    /// The Moon's body at `(x, y)`.
    fn body_tile(&self, x: i32, y: i32, surface: i32, bottom: i32) -> LunarTile {
        let seed = self.moon.shape_seed();
        let layer = y - surface;
        let above_bottom = bottom - y;
        let roll = hash2(seed, SALT_BODY, x, y);
        let icy = self.ice_fields.iter().any(|field| field.contains(x));
        let tube_near = self
            .tubes
            .iter()
            .any(|tube| x >= tube.first && x <= tube.last && (y - tube.center_row(x)).abs() <= 3);
        if tube_near {
            // Glassy basalt lines the tubes, with ice frozen on the floor here and there.
            if y > 0 && roll.is_multiple_of(9) {
                return LunarTile::Ice;
            }
            if roll % 211 == 5 {
                return LunarTile::RelicCache;
            }
            return LunarTile::Basalt;
        }
        if layer <= 4 {
            if icy && (layer <= 1 || roll.is_multiple_of(3)) {
                return LunarTile::Ice;
            }
            if layer >= 1 && self.helium_pocket(x, y) {
                return LunarTile::Helium3;
            }
            let mare = self.maria.iter().any(|mare| mare.contains(x));
            return if mare {
                LunarTile::MareDust
            } else {
                LunarTile::Regolith
            };
        }
        if layer <= 7 && self.helium_pocket(x, y) {
            return LunarTile::Helium3;
        }
        if icy && layer <= 9 && roll.is_multiple_of(4) {
            return LunarTile::Ice;
        }
        if above_bottom < 10 && roll.is_multiple_of(19) {
            // Ice on the cold, dark underside.
            return LunarTile::Ice;
        }
        if layer > 40 && self.alloy_vein(x, y) {
            return LunarTile::AlloyVein;
        }
        if layer > 10 && roll % 1_301 == 7 {
            return LunarTile::RelicCache;
        }
        if above_bottom < DEEP_ROCK_ROWS {
            LunarTile::DeepRock
        } else {
            LunarTile::Rock
        }
    }

    /// Helium-3 pockets: clumps in the dust.
    const fn helium_pocket(&self, x: i32, y: i32) -> bool {
        let seed = self.moon.shape_seed();
        let cell = hash2(seed, SALT_BODY + 1, x.div_euclid(5), y.div_euclid(3));
        cell.is_multiple_of(13) && !hash2(seed, SALT_BODY + 2, x, y).is_multiple_of(3)
    }

    /// Alien alloy: thin veins in clustered cells deep in the rock.
    const fn alloy_vein(&self, x: i32, y: i32) -> bool {
        let seed = self.moon.shape_seed();
        let cell = hash2(seed, SALT_BODY + 3, x.div_euclid(9), y.div_euclid(9));
        if !cell.is_multiple_of(13) {
            return false;
        }
        let vein = fractal_noise(seed, SALT_BODY + 5, x, y, 6, 6, 2);
        (vein - NOISE_MAX / 2).abs() < NOISE_MAX / 22
    }
}

/// An abandoned habitat: a broken hut of panels with a relic cache inside. `None`: not part of
/// it; `Some(None)`: open air it claims; `Some(Some(tile))`: its tile.
#[allow(
    clippy::option_option,
    reason = "outside / claimed air / a tile are three different answers"
)]
const fn habitat_tile(site: &Site, seed: u64, x: i32, y: i32) -> Option<Option<TileKind>> {
    let (cx, g) = (site.center_x, site.ground_row);
    if (x - cx).abs() > 6 || y < g - 5 || y >= g {
        return None;
    }
    let wall = (x - cx).abs() == 6 || y == g - 5;
    if !wall {
        if x == cx - 3 && y == g - 1 {
            return Some(Some(TileKind::Lunar(LunarTile::RelicCache)));
        }
        return Some(None);
    }
    // The door, and panels torn away by years of micrometeorites (never the corners).
    let corner = (x - cx).abs() == 6 && y == g - 5;
    let door = x == cx + 6 && y >= g - 2;
    let torn = !corner && hash2(seed, SALT_HAB, x, y).is_multiple_of(4);
    if door || torn {
        return Some(None);
    }
    Some(Some(TileKind::Lunar(LunarTile::HabitatPanel)))
}

#[cfg(test)]
mod tests;
