//! Drill heads: one procedural head per drill tier (material ramp, size, tip accent, rings),
//! shaped by the cosmetic drill style, painted in drill space and mapped onto the rig for each
//! pose.
//!
//! Drill space: `u` runs from the base (0, just past the two collar rows `-2` and `-1`) to the
//! tip; `l` runs across the head, `-half..half` pixels around its axis.

use fatherload_simulation::cosmetics::DrillStyle;
use raylib::prelude::Color;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{DrillPose, RigCanvas, RigLook, RigPose, paint_ramp};

/// One drill tier's head.
#[derive(Clone, Copy, Debug)]
struct DrillSpec {
    /// Cone rows (after the collar).
    length: i32,
    /// Half width at the base (pixels each side of the axis, outline included).
    half: i32,
    light: Color,
    mid: Color,
    dark: Color,
    /// The spinning glint band.
    glint: Color,
    /// The last rows' colours (bright, dim): diamond tips, glowing plasma, white-hot points.
    tip: Option<(Color, Color)>,
    /// Ring colour every third row.
    band: Option<Color>,
}

const fn spec(
    length: i32,
    half: i32,
    [light, mid, dark, glint]: [Color; 4],
    tip: Option<(Color, Color)>,
    band: Option<Color>,
) -> DrillSpec {
    DrillSpec {
        length,
        half,
        light,
        mid,
        dark,
        glint,
        tip,
        band,
    }
}

/// The head of drill tier `tier` (Steel, Carbide, Tempered, Diamond, Plasma, Twin-Bit, Sonic,
/// Thermite, Graphene, Resonant, Fusion, Quantum, Singularity, Nova, Star): longer every few
/// tiers, wider at the top, each in its own material.
const fn drill_spec(tier: u8) -> DrillSpec {
    const STEEL_RAMP: [Color; 4] = [STEEL_LIGHT, STEEL, STEEL_DARK, BONE];
    match tier {
        0 | 1 => spec(5, 5, STEEL_RAMP, None, None),
        2 => spec(
            5,
            5,
            [FOG, SLATE_LIGHT, SLATE, MIST],
            Some((STEEL_LIGHT, STEEL)),
            None,
        ),
        3 => spec(
            5,
            5,
            [SAND, COPPER, RUST, CREAM],
            Some((SKY_LIGHT, SKY)),
            None,
        ),
        4 => spec(6, 5, STEEL_RAMP, Some((ICE, SKY_LIGHT)), None),
        5 => spec(
            6,
            5,
            [LAVENDER, VIOLET, PLUM, PINK],
            Some((PINK, ROSE)),
            None,
        ),
        6 => spec(6, 5, STEEL_RAMP, None, Some(GOLD)),
        7 => spec(6, 5, [MINT, TEAL, TEAL_DARK, ICE], None, Some(ICE)),
        8 => spec(7, 5, [GOLD, ORANGE, RED, CREAM], Some((CREAM, GOLD)), None),
        9 => spec(
            7,
            5,
            [SLATE_LIGHT, SLATE, SLATE_DARK, MINT],
            None,
            Some(TEAL),
        ),
        10 => spec(7, 5, [CREAM, GOLD, AMBER, BONE], None, Some(RUST)),
        11 => spec(7, 5, [BONE, MIST, FOG, CREAM], Some((ICE, MINT)), None),
        12 => spec(8, 6, [MINT, TEAL, VIOLET, LAVENDER], None, Some(LAVENDER)),
        13 => spec(
            8,
            6,
            [LAVENDER, PLUM, NIGHT, PINK],
            Some((PINK, LAVENDER)),
            None,
        ),
        14 => spec(
            8,
            6,
            [CREAM, GOLD, ORANGE, BONE],
            Some((BONE, CREAM)),
            Some(ORANGE),
        ),
        _ => spec(
            8,
            6,
            [BONE, GOLD, AMBER, CREAM],
            Some((BONE, ICE)),
            Some(SKY_LIGHT),
        ),
    }
}

/// Linear taper from `from` at `u = 0` to `to` at `u = length - 1`, rounding up.
const fn taper(from: i32, to: i32, u: i32, length: i32) -> i32 {
    if length <= 1 {
        return to;
    }
    let span = (from - to) * (length - 1 - u);
    to + (span + length - 2) / (length - 1)
}

/// The horizontal spans `[start, end)` (in `l`) a style fills on cone row `u`.
fn row_spans(style: DrillStyle, u: i32, length: i32, half: i32, spin: usize) -> Vec<(i32, i32)> {
    let last = length - 1;
    match style {
        DrillStyle::Standard => {
            let w = taper(half, 1, u, length);
            vec![(-w, w)]
        }
        DrillStyle::Serrated => {
            let w = taper(half, 1, u, length);
            // Teeth stick out on alternate rows (shifting as the head spins).
            let tooth = i32::from(u < last && (u + spin as i32 / 2) % 2 == 0);
            vec![(-w - tooth, w + tooth)]
        }
        DrillStyle::Auger => {
            // A slim screw: the flighting winds around a tapering core.
            let core = taper(half - 2, 1, u, length).max(1);
            let left = (u + spin as i32) % 2 == 0;
            let flight = i32::from(u < last);
            if left {
                vec![(-core - flight, core)]
            } else {
                vec![(-core, core + flight)]
            }
        }
        DrillStyle::Twin => {
            let small = (half + 1) / 2;
            let w = taper(small, 1, u, length);
            vec![
                ((-small - w).max(-half - 1), -small + w),
                (small - w, (small + w).min(half + 1)),
            ]
        }
        DrillStyle::Crusher => {
            match u.cmp(&(last - 1)) {
                std::cmp::Ordering::Less => vec![(-half, half)],
                std::cmp::Ordering::Equal => vec![(-half + 1, half - 1)],
                std::cmp::Ordering::Greater => {
                    // Studs across the blunt face.
                    (-half + 1..half - 1)
                        .step_by(2)
                        .map(|l| (l, l + 1))
                        .collect()
                }
            }
        }
        DrillStyle::Spire => {
            // A short cone narrowing into a long needle.
            let w = if u == last { 1 } else { (half - 1 - u).max(2) };
            vec![(-w, w)]
        }
    }
}

/// Cone rows a style uses for a tier's `length`.
const fn style_length(style: DrillStyle, length: i32) -> i32 {
    match style {
        DrillStyle::Crusher => length - 1,
        DrillStyle::Spire => length + 3,
        _ => length,
    }
}

/// Paints the drill head for the pose.
pub(super) fn paint_drill(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    let drill = drill_spec(look.tiers.drill);
    let style = look.cosmetics.drill_style;
    let (spinning, map): (bool, &dyn Fn(i32, i32) -> (i32, i32)) = match pose.drill {
        DrillPose::Down => (true, &|u, l| (8 + l, 14 + u)),
        DrillPose::Up => (true, &|u, l| (8 + l, -1 - u)),
        DrillPose::Nose { spinning } => {
            let shake = pose.shake;
            (spinning, &move |u, l| (17 + u + shake, 8 + l))
        }
    };
    let half = match pose.drill {
        DrillPose::Nose { .. } => drill.half - 1,
        _ => drill.half,
    };
    let spin = if spinning { pose.spin } else { 0 };
    let trim = paint_ramp(look.cosmetics.trim);
    // Collar: an outline row and a band in the trim colour.
    for l in -half..half {
        let (x, y) = map(-2, l);
        canvas.set(x, y, INK);
        let (x, y) = map(-1, l);
        let edge = l == -half || l == half - 1;
        let lit = !edge && (l + spin as i32).rem_euclid(3) == 0;
        canvas.set(
            x,
            y,
            if edge {
                INK
            } else if lit {
                trim.body
            } else {
                trim.shade
            },
        );
    }
    let length = style_length(style, drill.length);
    for u in 0..length {
        for (start, end) in row_spans(style, u, length, half, spin) {
            for l in start..end {
                let (x, y) = map(u, l);
                let color = if l == start || l == end - 1 || u == length - 1 {
                    INK
                } else {
                    head_color(&drill, u, l, start, end, length, spin)
                };
                canvas.set(x, y, color);
            }
        }
    }
}

/// The colour of an inner pixel of the head.
fn head_color(
    drill: &DrillSpec,
    u: i32,
    l: i32,
    start: i32,
    end: i32,
    length: i32,
    spin: usize,
) -> Color {
    let width = (end - start - 2).max(1);
    let across = l - start - 1;
    let glint = (l - u + spin as i32).rem_euclid(4) == 0;
    if let Some((bright, dim)) = drill.tip
        && u >= length - 3
    {
        return if glint || across * 3 < width {
            bright
        } else {
            dim
        };
    }
    if glint {
        return drill.glint;
    }
    if let Some(band) = drill.band
        && u % 3 == 1
    {
        return band;
    }
    if across * 3 < width {
        drill.light
    } else if across * 3 < width * 2 {
        drill.mid
    } else {
        drill.dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapers_run_from_the_base_to_a_point() {
        assert_eq!(taper(5, 1, 0, 5), 5);
        assert_eq!(taper(5, 1, 4, 5), 1);
        for u in 1..5 {
            assert!(taper(5, 1, u, 5) <= taper(5, 1, u - 1, 5));
        }
    }

    #[test]
    fn every_style_fits_inside_its_collar_plus_teeth() {
        for style in DrillStyle::ALL {
            for tier in 1..=15 {
                let drill = drill_spec(tier);
                let length = style_length(*style, drill.length);
                for u in 0..length {
                    for (start, end) in row_spans(*style, u, length, drill.half, 3) {
                        assert!(start < end);
                        assert!(start >= -drill.half - 1 && end <= drill.half + 1);
                    }
                }
            }
        }
    }
}
