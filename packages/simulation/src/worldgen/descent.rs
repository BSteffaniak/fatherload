//! The stratum layout of a Deep Descent world ([`GeneratorVersion::V11`], expansion plan F8).
//!
//! A V11 world is about 13,000 rows from the ground line to the bedrock floor
//! ([`WorldSize::descent_height_tiles`]), split into the eight Descents of [`crate::descent`]
//! and their 41 strata:
//!
//! * each Descent draws its thickness per seed (0.9 to 1.1 times its target share of the
//!   world's rows), and each stratum its share of its Descent (0.7 to 1.3 times the registry's
//!   share, never under [`MIN_STRATUM_ROWS`]);
//! * every boundary between two strata is a ragged transition zone in the manner of V4
//!   (lenses of the stratum below thicken row by row until they take over), wider where one
//!   Descent hands over to the next;
//! * the bedrock floor closes the bottom as in every deep world.
//!
//! Every function here is a pure, integer function of the settings and the tile, so chunks
//! stay seamless and order-independent. Worlds before V11 answer [`stratum_at`] with the
//! stratum standing for their classic band ([`Stratum::of_band`]).
//!
//! Classic rules read a V11 world through its strata's host bands: [`super::band_at`] answers
//! the host band, and [`nominal_row`] maps each stratum's rows onto its host band's classic
//! nominal rows, so heat, hazard gates, ore curves and the rest behave in each placeholder
//! stratum as they do in the band it borrows from.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "const integer layout maths on small row counts, indices and reduced hashes"
)]

pub mod landmarks;

use crate::{
    descent::{Descent, STRATUM_COUNT, Stratum},
    terrain::{TileKind, TilePosition},
};

use super::{
    WORLD_HEIGHT_TILES, WorldSettings,
    noise::{NOISE_MAX, centred, hash2, line_noise, value_noise},
    strata::{BEDROCK_MIN_ROWS, BEDROCK_WAVE_ROWS, Band, SKY_ROWS, nominal_band_top},
};

/// Rows from the ground line to the bedrock floor of a mean Standard V11 world.
pub const DESCENT_DEPTH_ROWS: i32 = 13_000;
/// Fewest rows any stratum of a V11 world has (room for both of its transition zones and
/// something to explore between them).
pub const MIN_STRATUM_ROWS: i32 = 96;
/// Number of stratum boundaries.
pub const BOUNDARY_COUNT: usize = STRATUM_COUNT - 1;

const SALT_DESCENT: u64 = 0x5744_0001;
const SALT_STRATUM: u64 = 0x5744_0100;
const SALT_WAVE: u64 = 0x5744_0200;
const SALT_LENS: u64 = 0x5744_0300;
const SALT_VARIANT: u64 = 0x5744_0400;
const SALT_HOLLOW: u64 = 0x5744_0500;

/// A V11 world's strata rows.
///
/// The first row of every stratum (`tops[0]` is the ground line) and the floor row the last
/// stratum runs down to (before the bedrock's waves).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescentLayout {
    pub tops: [i32; STRATUM_COUNT],
    pub floor: i32,
}

impl DescentLayout {
    /// `(first, last)` rows of `stratum` before waves and lenses.
    #[must_use]
    pub const fn rows(&self, stratum: Stratum) -> (i32, i32) {
        let index = stratum.index();
        let bottom = if index + 1 < STRATUM_COUNT {
            self.tops[index + 1] - 1
        } else {
            self.floor - 1
        };
        (self.tops[index], bottom)
    }

    /// `(first, last)` rows of `descent` before waves and lenses.
    #[must_use]
    pub const fn descent_rows(&self, descent: Descent) -> (i32, i32) {
        (
            self.rows(descent.first_stratum()).0,
            self.rows(descent.last_stratum()).1,
        )
    }

    /// The stratum whose layout rows hold `row` (no waves or lenses; rows above the ground are
    /// the first stratum's, rows below the floor the last one's).
    #[must_use]
    pub const fn stratum_of_row(&self, row: i32) -> Stratum {
        let mut index = 0;
        while index + 1 < STRATUM_COUNT && row >= self.tops[index + 1] {
            index += 1;
        }
        Stratum::ALL[index]
    }

    /// How far through its stratum `row` is, per mille (top 0, bottom 1000).
    #[must_use]
    pub const fn along(&self, row: i32) -> u16 {
        let (top, bottom) = self.rows(self.stratum_of_row(row));
        let span = bottom - top + 1;
        if span <= 0 || row <= top {
            return 0;
        }
        if row >= bottom {
            return 1_000;
        }
        ((row - top) * 1_000 / span) as u16
    }
}

/// The first row below a V11 world's strata (the bedrock floor starts here or a few rows
/// lower).
#[must_use]
pub const fn floor_row(settings: &WorldSettings) -> i32 {
    settings.height_tiles() - BEDROCK_MIN_ROWS - BEDROCK_WAVE_ROWS
}

/// This V11 world's layout (see the module docs). Meaningful only for worlds with Descents.
#[must_use]
pub const fn layout(settings: &WorldSettings) -> DescentLayout {
    let seed = settings.seed;
    let floor = floor_row(settings);
    let rows = floor - SKY_ROWS;
    // Each Descent's thickness: its target share times a per-seed 0.9-1.1 roll.
    let mut weights = [0_i64; 8];
    let mut total = 0_i64;
    let mut descent = 0;
    while descent < 8 {
        let roll = (hash2(seed, SALT_DESCENT, descent as i32, 0) % 21) as i64;
        weights[descent] = Descent::ALL[descent].target_thickness() as i64 * (90 + roll);
        total += weights[descent];
        descent += 1;
    }
    let mut thickness = [0_i32; 8];
    let mut handed = 0;
    descent = 0;
    while descent < 8 {
        thickness[descent] = (rows as i64 * weights[descent] / total) as i32;
        handed += thickness[descent];
        descent += 1;
    }
    thickness[7] += rows - handed;
    // Each stratum's share of its Descent.
    let mut tops = [0_i32; STRATUM_COUNT];
    let mut row = SKY_ROWS;
    let mut index = 0;
    descent = 0;
    while descent < 8 {
        let first = index;
        let mut count = 0;
        let mut share_total = 0_i64;
        let mut shares = [0_i64; 8];
        while index < STRATUM_COUNT && Stratum::ALL[index].def().descent as usize == descent {
            let roll = (hash2(seed, SALT_STRATUM, index as i32, 0) % 61) as i64;
            shares[count] = Stratum::ALL[index].def().share_per_mille as i64 * (70 + roll);
            share_total += shares[count];
            count += 1;
            index += 1;
        }
        let spare = thickness[descent] - count as i32 * MIN_STRATUM_ROWS;
        let mut handed = 0;
        let mut each = 0;
        while each < count {
            let extra = if share_total > 0 && spare > 0 {
                (spare as i64 * shares[each] / share_total) as i32
            } else {
                0
            };
            tops[first + each] = row;
            row += MIN_STRATUM_ROWS + extra;
            handed += MIN_STRATUM_ROWS + extra;
            each += 1;
        }
        // Rounding leftovers go to the Descent's last stratum (the gate's).
        row += thickness[descent] - handed;
        descent += 1;
    }
    DescentLayout { tops, floor }
}

/// `(wave amplitude, half height of the transition zone)` of boundary `index`.
///
/// The boundary lies between stratum `index` and the next; it is wider where a Descent ends.
#[must_use]
pub const fn boundary_shape(index: usize) -> (i32, i32) {
    if index == 0 {
        (4, 6)
    } else if Stratum::ALL[index].def().descent as usize
        != Stratum::ALL[index + 1].def().descent as usize
    {
        (18, 20)
    } else {
        (12, 14)
    }
}

/// Wave offset of boundary `index` in column `x`.
const fn boundary_wave(seed: u64, index: usize, x: i32) -> i32 {
    let salt = SALT_WAVE + index as u64;
    let wave = (line_noise(seed, salt, x, 71) * 2 + line_noise(seed, salt ^ 0xFF, x, 23)) / 3;
    centred(wave, boundary_shape(index).0)
}

/// First row of the stratum below boundary `index` in column `x`, before lenses.
#[must_use]
pub const fn boundary_column_depth(
    settings: &WorldSettings,
    layout: &DescentLayout,
    index: usize,
    x: i32,
) -> i32 {
    layout.tops[index + 1] + boundary_wave(settings.seed, index, x)
}

/// Rows boundary `index` may occupy anywhere: `(shallowest, deepest)`.
#[must_use]
pub const fn boundary_depth_range(layout: &DescentLayout, index: usize) -> (i32, i32) {
    let (wave, half) = boundary_shape(index);
    let base = layout.tops[index + 1];
    (base - wave - half, base + wave + half)
}

/// Whether tile `(x, y)` lies below boundary `index` (the V4 lens model).
const fn below_boundary(
    settings: &WorldSettings,
    layout: &DescentLayout,
    index: usize,
    x: i32,
    y: i32,
) -> bool {
    let (wave, half) = boundary_shape(index);
    let base = layout.tops[index + 1];
    if y >= base + wave + half {
        return true;
    }
    if y < base - wave - half {
        return false;
    }
    let line = base + boundary_wave(settings.seed, index, x);
    let top = line - half;
    if y < top {
        return false;
    }
    if y >= line + half {
        return true;
    }
    let along = (y - top) * NOISE_MAX / (half * 2);
    let salt = SALT_LENS + index as u64;
    let lens = value_noise(settings.seed, salt, x, y, 11, 5);
    let fringe = value_noise(settings.seed, salt ^ 0xFF, x, y, 3, 2);
    let mixed = (lens * 3 + fringe) / 4;
    let spread = 32_768 + (mixed - 32_768) * 2;
    spread < along
}

/// The stratum containing tile `(x, y)` of any world: V11 worlds read their own layout, older
/// worlds the stratum standing for their classic band.
#[must_use]
pub const fn stratum_at(settings: &WorldSettings, x: i32, y: i32) -> Stratum {
    if settings.generator.has_descents() {
        stratum_in(settings, &layout(settings), x, y)
    } else {
        Stratum::of_band(super::strata::band_at(settings, x, y))
    }
}

/// [`stratum_at`] for a V11 world whose layout is already known (bulk passes).
#[must_use]
pub const fn stratum_in(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
) -> Stratum {
    let mut index = 0;
    while index < BOUNDARY_COUNT && below_boundary(settings, layout, index, x, y) {
        index += 1;
    }
    Stratum::ALL[index]
}

/// The Descent containing tile `(x, y)` (see [`stratum_at`]).
#[must_use]
pub const fn descent_at(settings: &WorldSettings, x: i32, y: i32) -> Descent {
    stratum_at(settings, x, y).descent()
}

/// The first row of column `x` inside `stratum` whatever the lenses do there.
#[must_use]
pub const fn stratum_solid_top_row(
    settings: &WorldSettings,
    layout: &DescentLayout,
    stratum: Stratum,
    x: i32,
) -> i32 {
    match stratum.index() {
        0 => SKY_ROWS + 1,
        index => {
            boundary_column_depth(settings, layout, index - 1, x) + boundary_shape(index - 1).1 + 1
        }
    }
}

/// The shallowest row `stratum` reaches anywhere in a V11 world.
#[must_use]
pub const fn stratum_shallowest_row(layout: &DescentLayout, stratum: Stratum) -> i32 {
    match stratum.index() {
        0 => SKY_ROWS + 1,
        index => boundary_depth_range(layout, index - 1).0,
    }
}

/// The variant (an index into [`crate::descent::StratumDef::variants`]) this world uses for
/// `stratum`.
#[must_use]
pub const fn variant(settings: &WorldSettings, stratum: Stratum) -> usize {
    let count = stratum.def().variants.len();
    if count == 0 {
        return 0;
    }
    (hash2(settings.seed, SALT_VARIANT, stratum.index() as i32, 0) % count as u64) as usize
}

// ---------------------------------------------------------------------------------------------
// Classic rules on a V11 world: host bands and nominal rows.

/// The classic nominal rows `(first, end)` of `band` (the ~220-row stack).
const fn nominal_span(band: Band) -> (i32, i32) {
    let index = band.index();
    let end = if index + 1 < Band::ALL.len() {
        nominal_band_top(Band::ALL[index + 1])
    } else {
        WORLD_HEIGHT_TILES
    };
    (nominal_band_top(band), end)
}

/// [`super::strata::nominal_row`] of a V11 world: the same relative position inside the
/// stratum's host band on the classic stack.
#[must_use]
pub const fn nominal_row(layout: &DescentLayout, row: i32) -> i32 {
    if row < SKY_ROWS {
        return row;
    }
    if row >= layout.floor {
        return WORLD_HEIGHT_TILES + (row - layout.floor);
    }
    let stratum = layout.stratum_of_row(row);
    let (top, bottom) = layout.rows(stratum);
    let (first, end) = nominal_span(stratum.host_band());
    let span = bottom - top + 1;
    if span <= 0 {
        return first;
    }
    first + (row - top) * (end - first) / span
}

/// [`super::strata::nominal_depth`] of a V11 world (continuous rows).
#[must_use]
pub fn nominal_depth(layout: &DescentLayout, rows: f32) -> f32 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "rows are far below f32's integer range"
    )]
    let to_f32 = |value: i32| value as f32;
    if rows < to_f32(SKY_ROWS) {
        return rows;
    }
    if rows >= to_f32(layout.floor) {
        return to_f32(WORLD_HEIGHT_TILES) + (rows - to_f32(layout.floor));
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "rows are inside the world here"
    )]
    let stratum = layout.stratum_of_row(rows.floor() as i32);
    let (top, bottom) = layout.rows(stratum);
    let (first, end) = nominal_span(stratum.host_band());
    let span = to_f32(bottom - top + 1).max(1.0);
    (rows - to_f32(top)).mul_add(to_f32(end - first) / span, to_f32(first))
}

/// [`super::strata::scaled_row`] of a V11 world.
///
/// The first row of Descents I and II (where the strata are the classic bands, in order) that maps at or past the classic nominal row.
/// Nominal rows past the classic stack land at the end of Descent II.
#[must_use]
pub const fn scaled_row(layout: &DescentLayout, nominal: i32) -> i32 {
    if nominal <= SKY_ROWS {
        return nominal;
    }
    let end = layout.tops[Band::ALL.len()];
    if nominal >= WORLD_HEIGHT_TILES {
        return end;
    }
    // The mapping is monotone over Descents I and II, where stratum `index` maps its rows
    // linearly onto classic band `index`: invert it in the stratum whose span holds `nominal`.
    // (Exactly the first row a binary search over [`nominal_row`] finds, without its dozen
    // probes; `scaled_row_matches_a_search_of_nominal_rows` checks every value.)
    let mut index = 0;
    while index + 1 < Band::ALL.len() && nominal >= nominal_span(Band::ALL[index + 1]).0 {
        index += 1;
    }
    let (first, end_nominal) = nominal_span(Band::ALL[index]);
    let top = layout.tops[index];
    let span = layout.tops[index + 1] - top;
    if nominal <= first || span <= 0 {
        return top;
    }
    // The least `r` with `(r - top) * (end - first) / span >= nominal - first`.
    let length = end_nominal - first;
    let steps = ((nominal - first) as i64 * span as i64 + length as i64 - 1) / length as i64;
    top + steps as i32
}

/// [`scaled_row`] by binary search over [`nominal_row`] (the reference the closed form must
/// match).
#[cfg(test)]
pub(crate) const fn scaled_row_by_search(layout: &DescentLayout, nominal: i32) -> i32 {
    if nominal <= SKY_ROWS {
        return nominal;
    }
    let end = layout.tops[Band::ALL.len()];
    if nominal >= WORLD_HEIGHT_TILES {
        return end;
    }
    let (mut low, mut high) = (SKY_ROWS, end);
    while low < high {
        let middle = low + (high - low) / 2;
        if nominal_row(layout, middle) >= nominal {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// [`super::strata::scaled_span`] of a V11 world: classic rows stretched as Descents I and II
/// stretch the classic stack.
#[must_use]
pub const fn scaled_span(layout: &DescentLayout, nominal_rows: i32) -> i32 {
    let world_rows = layout.tops[Band::ALL.len()] - SKY_ROWS;
    let classic_rows = WORLD_HEIGHT_TILES - SKY_ROWS;
    nominal_rows * world_rows / classic_rows
}

// ---------------------------------------------------------------------------------------------
// The Hollow World cavern.

/// Rows the Hollow World cavern keeps below the Inner Sky's top (its rock sky).
const HOLLOW_CEILING_ROWS: i32 = 40;
/// Rows the cavern keeps above the Inner Sky's bottom (its ground).
const HOLLOW_FLOOR_ROWS: i32 = 48;
/// How far the cavern's ceiling and floor wave.
const HOLLOW_WAVE_ROWS: i32 = 14;

/// `(ceiling, floor)`: the first and last open rows of the Hollow World cavern in column `x`.
#[must_use]
pub const fn hollow_rows(settings: &WorldSettings, layout: &DescentLayout, x: i32) -> (i32, i32) {
    let (top, bottom) = layout.rows(Stratum::InnerSky);
    let seed = settings.seed;
    let ceiling_wave = centred(
        (line_noise(seed, SALT_HOLLOW, x, 47) * 2 + line_noise(seed, SALT_HOLLOW ^ 1, x, 13)) / 3,
        HOLLOW_WAVE_ROWS,
    );
    let floor_wave = centred(
        (line_noise(seed, SALT_HOLLOW ^ 2, x, 61) * 2 + line_noise(seed, SALT_HOLLOW ^ 3, x, 17))
            / 3,
        HOLLOW_WAVE_ROWS,
    );
    (
        top + HOLLOW_CEILING_ROWS + HOLLOW_WAVE_ROWS + ceiling_wave,
        bottom - HOLLOW_FLOOR_ROWS - HOLLOW_WAVE_ROWS + floor_wave,
    )
}

/// Whether `(x, y)` is open air of the Hollow World cavern.
///
/// Inside its ceiling and floor and not part of a floating stone (smooth noise blobs, stretched sideways, thinning towards the
/// cavern's middle so the space stays open).
#[must_use]
pub const fn hollow_open(settings: &WorldSettings, layout: &DescentLayout, x: i32, y: i32) -> bool {
    let (top, bottom) = layout.rows(Stratum::InnerSky);
    if y < top || y > bottom {
        return false;
    }
    let (ceiling, floor) = hollow_rows(settings, layout, x);
    if y < ceiling || y > floor {
        return false;
    }
    let seed = settings.seed;
    // Stalactites from the rock sky and spires from the ground.
    let fringe = value_noise(seed, SALT_HOLLOW ^ 4, x, 0, 5, 1) * 12 / NOISE_MAX;
    if y < ceiling + fringe / 2 || y > floor - fringe / 3 {
        return false;
    }
    // Floating stones: big blobs, rarer near the middle of the cavern.
    let blob = (value_noise(seed, SALT_HOLLOW ^ 5, x, y, 34, 14) * 3
        + value_noise(seed, SALT_HOLLOW ^ 6, x, y, 9, 5))
        / 4;
    blob < NOISE_MAX * 81 / 100
}

/// The tile a V11 world's base layer has at `(x, y)` before caves where the Descents differ
/// from the classic stack (the Hollow World's open air), or `None` to keep the base rock.
#[must_use]
pub const fn fixed_kind(
    settings: &WorldSettings,
    layout: &DescentLayout,
    x: i32,
    y: i32,
) -> Option<TileKind> {
    if hollow_open(settings, layout, x, y) {
        Some(TileKind::Air)
    } else {
        None
    }
}

/// Whether `settings` describes a world with Descents (V11 and later).
#[must_use]
pub const fn has_descents(settings: &WorldSettings) -> bool {
    settings.generator.has_descents()
}

/// The tile position at the middle of `stratum`'s layout rows above column `x` (scenes,
/// tests and tools that want "somewhere in that stratum").
#[must_use]
pub const fn stratum_middle(layout: &DescentLayout, stratum: Stratum, x: i32) -> TilePosition {
    let (top, bottom) = layout.rows(stratum);
    TilePosition {
        x,
        y: i32::midpoint(top, bottom),
    }
}

#[cfg(test)]
mod tests;
