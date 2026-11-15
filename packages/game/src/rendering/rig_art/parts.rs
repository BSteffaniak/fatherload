//! The rig's hull and parts, painted into a [`RigCanvas`] in body coordinates (the 16x13 body box,
//! facing right: column 0 is the tail, 15 the nose).
//!
//! Upgrade lines group their 15 tiers into five visual classes ([`class`]): stock (tiers 1-2),
//! then 3-5, 6-9, 10-12 and 13-15, each a visibly bigger part.

use fatherload_simulation::cosmetics::{
    Decal, FrontAttachment, HullPattern, RearAttachment, RoofAttachment,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;

use super::{RigCanvas, RigLook, RigPose, Thrust, flame_ramp, paint_ramp};

/// Body box width in art pixels.
pub const BODY_WIDTH: i32 = 16;
/// Body box height in art pixels.
pub const BODY_HEIGHT: i32 = 13;
/// Where a hardpoint weapon mounts: the art pixel above the plate on the dome's roof (body
/// coordinates, facing right).
pub const HARDPOINT: (i32, i32) = (11, -1);

/// The visual class (0-4) of an upgrade tier.
pub const fn class(tier: u8) -> i32 {
    match tier {
        0..=2 => 0,
        3..=5 => 1,
        6..=9 => 2,
        10..=12 => 3,
        _ => 4,
    }
}

/// The hull, glass and tracks. `t` is the stripe row and `q` the secondary skirt band; both
/// are repainted by the hull pattern.
const HULL: [&str; 13] = [
    "................", //
    "........ooooo...", //
    "......oowwggGo..", //
    ".....owwggggGGo.", //
    ".oooooooooooooo.", //
    "ohhhhhhhhhhhhhyo", //
    "ohyySyyyyyyyyyll", //
    "ohttttttttttttao", //
    "oyyyyyyySyyyyyao", //
    "oqqqqqqqqqqqqqQo", //
    ".oooooooooooooo.", //
    "oLdLdLdLdLdLdLdo", //
    ".oooooooooooooo.", //
];

/// Scorch marks of a badly damaged hull: `x` soot, `r` rust, `k` a dead headlight.
const SCORCH: [&str; 10] = [
    "................", //
    "................", //
    ".........x......", //
    ".........x......", //
    "................", //
    ".....x......x...", //
    "...xx......xx.yk", //
    ".....x..........", //
    "....x....x......", //
    "...r....xx......", //
];

/// Camo blotches over rows 5-8 (`c` secondary, `C` secondary shade).
const CAMO: [&str; 4] = [
    "...cc.....CC...", //
    "..ccC...cc..C..", //
    "...C...ccC....c", //
    ".....cc..C..cc.", //
];

/// The rear deck: where cargo sits, the mast stands and tail attachments mount.
#[derive(Clone, Copy, Debug)]
struct Deck {
    /// Top row of the cargo (or of the stock stack).
    top: i32,
    /// Left (tail-side) column of the cargo, or of the hull without cargo.
    left: i32,
    /// The mast's column.
    mast_x: i32,
    /// The exhaust stack's column and top row.
    stack: (i32, i32),
}

const fn deck(look: &RigLook) -> Deck {
    match class(look.tiers.cargo) {
        0 => Deck {
            top: 3,
            left: 1,
            mast_x: 2,
            stack: (2, 3),
        },
        cargo => {
            let top = 2 - cargo;
            Deck {
                top,
                left: 1 - cargo,
                mast_x: 2 - cargo,
                stack: (3, top - 2),
            }
        }
    }
}

/// Where the exhaust puffs start (the top of the stack).
pub(super) const fn exhaust_origin(look: &RigLook) -> (i32, i32) {
    let deck = deck(look);
    if class(look.tiers.cargo) == 0 {
        (deck.stack.0, 1)
    } else {
        (deck.stack.0, deck.stack.1 - 1)
    }
}

/// A thruster nozzle on a pod: `(left column, width, bottom row)`.
type Nozzle = (i32, i32, i32);

/// The pod nozzles of an engine class (the belly flame is separate).
const fn nozzles(engine: i32) -> &'static [Nozzle] {
    match engine {
        0 => &[],
        1 => &[(-3, 4, 11)],
        2 => &[(-4, 5, 12)],
        3 => &[(-5, 2, 12), (-2, 2, 12)],
        _ => &[(-6, 3, 12), (-3, 3, 12)],
    }
}

const BELLY_CLIMB: [[&str; 6]; 3] = [
    ["agccga", "ragcar", ".rgcr.", ".rgar.", "..ra..", "..r..."],
    ["agccga", "rgccgr", ".agca.", "..gar.", "..rr..", "...r.."],
    ["agccga", "ragcgr", ".rgcar", ".rag..", ".rr...", "..r..."],
];
const BELLY_CLIMB_BIG: [[&str; 9]; 3] = [
    [
        "agcccga.", "ragccgar", "ragccgar", ".rgccar.", ".ragcar.", "..rgar..", "..rga...",
        "...r....", "...r....",
    ],
    [
        ".agcccga", "ragcccgr", "ragcccar", ".agcgar.", "..gcar..", "..rgar..", "...rar..",
        "....r...", "........",
    ],
    [
        "agcccga.", "rgccgcar", "ragccgr.", ".rgccar.", ".rgcar..", ".rgar...", "..rar...",
        "..r.....", "..r.....",
    ],
];
const BELLY_HOVER: [[&str; 3]; 2] = [
    ["agccga", ".rgar.", "..r..."],
    ["agccga", ".ragr.", "...r.."],
];
const POD_CLIMB: [[&str; 6]; 2] = [
    ["gccg", "agca", "rgar", ".ar.", ".r..", "...."],
    ["gccg", "acga", "rgcr", ".ga.", "..r.", "..r."],
];
const POD_HOVER: [[&str; 2]; 2] = [["gccg", ".ar."], ["gccg", ".ra."]];

/// Paints the thruster flames (behind everything else).
pub(super) fn paint_flames(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    if pose.thrust == Thrust::Off {
        return;
    }
    let ramp = flame_ramp(look.cosmetics.flame, (pose.ticks / 5) as usize);
    let legend = |ch: char| match ch {
        'r' => Some(ramp.deep),
        'a' => Some(ramp.mid),
        'g' => Some(ramp.bright),
        'c' => Some(ramp.core),
        _ => None,
    };
    let engine = class(look.tiers.engine);
    let climbing = pose.thrust == Thrust::Climb;
    let frame = pose.spin;
    // The belly lift jet, a pixel toward the tail.
    if climbing && engine >= 3 {
        canvas.stamp(&BELLY_CLIMB_BIG[frame % 3], 3, 12, legend);
    } else if climbing {
        canvas.stamp(&BELLY_CLIMB[frame % 3], 4, 12, legend);
    } else {
        canvas.stamp(&BELLY_HOVER[frame % 2], 4, 12, legend);
    }
    for (index, &(left, width, bottom)) in nozzles(engine).iter().enumerate() {
        let frame = frame + index;
        let rows: &[&str] = if climbing {
            &POD_CLIMB[frame % 2]
        } else {
            &POD_HOVER[frame % 2]
        };
        // Narrow nozzles use the middle of the 4-wide flame.
        let skip = usize::try_from((4 - width).max(0) / 2).unwrap_or_default();
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().skip(skip).take(width as usize).enumerate() {
                if let Some(color) = legend(ch) {
                    canvas.set(left + dx as i32, bottom + 1 + dy as i32, color);
                }
            }
        }
        if width > 4 {
            // Wide nozzles: the outer columns get a short edge flame.
            canvas.set(left, bottom + 1, ramp.deep);
            canvas.set(left + width - 1, bottom + 1, ramp.deep);
        }
    }
}

/// Paints the hull and every part in front of the flames and drill.
pub(super) fn paint_body(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    paint_hull(canvas, look, pose);
    paint_pattern(canvas, look, pose);
    paint_decal(canvas, look);
    if pose.damaged {
        paint_scorch(canvas);
    }
    paint_radiator(canvas, look, pose);
    paint_armour(canvas, look);
    paint_mount(canvas, look, pose);
    paint_hardpoint(canvas);
    let deck = deck(look);
    paint_cargo(canvas, look, deck);
    paint_pods(canvas, look, pose);
    paint_rear(canvas, look, pose, deck);
    paint_mast(canvas, look, pose, deck);
    paint_front(canvas, look, pose);
}

fn paint_hull(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    let primary = paint_ramp(look.cosmetics.primary);
    let secondary = paint_ramp(look.cosmetics.secondary);
    let tracks = if pose.track_frame.is_multiple_of(2) {
        "oLdLdLdLdLdLdLdo"
    } else {
        "odLdLdLdLdLdLdLo"
    };
    let mut rows = HULL;
    rows[11] = tracks;
    canvas.stamp(&rows, 0, 0, |ch| match ch {
        'o' => Some(INK),
        'w' => Some(ICE),
        'g' => Some(SKY),
        'G' => Some(OCEAN),
        'h' => Some(primary.light),
        'y' | 't' => Some(primary.body),
        'a' => Some(primary.shade),
        'q' => Some(secondary.body),
        'Q' => Some(secondary.shade),
        'l' => Some(CREAM),
        'L' => Some(STEEL_LIGHT),
        'd' | 'S' => Some(STEEL_DARK),
        _ => None,
    });
}

fn paint_pattern(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    let primary = paint_ramp(look.cosmetics.primary);
    let secondary = paint_ramp(look.cosmetics.secondary);
    let trim = paint_ramp(look.cosmetics.trim);
    match look.cosmetics.pattern {
        HullPattern::Solid => {}
        HullPattern::Racing => {
            canvas.rect(2, 7, 12, 1, trim.body);
            canvas.set(1, 7, trim.light);
        }
        HullPattern::TwoTone => {
            canvas.rect(1, 7, 14, 1, trim.body);
            canvas.rect(1, 8, 13, 1, secondary.light);
            canvas.set(14, 8, secondary.body);
        }
        HullPattern::Checker => {
            for y in 7..=8 {
                for x in 1..15 {
                    let color = if (x + y) % 2 == 0 {
                        trim.light
                    } else {
                        primary.deep
                    };
                    canvas.set(x, y, color);
                }
            }
        }
        HullPattern::Camo => {
            canvas.stamp(&CAMO, 0, 5, |ch| match ch {
                'c' => Some(secondary.body),
                'C' => Some(secondary.shade),
                _ => None,
            });
        }
        HullPattern::Hazard => {
            for y in 7..=8 {
                for x in 1..15 {
                    let color = if ((x + y) >> 1) % 2 == 0 {
                        trim.body
                    } else {
                        INK
                    };
                    canvas.set(x, y, color);
                }
            }
        }
        HullPattern::Circuit => {
            let glow = if (pose.ticks / 20).is_multiple_of(3) {
                BONE
            } else {
                trim.light
            };
            canvas.rect(2, 6, 4, 1, trim.body);
            canvas.rect(5, 7, 1, 2, trim.body);
            canvas.rect(5, 8, 6, 1, trim.body);
            canvas.rect(10, 6, 1, 2, trim.body);
            canvas.rect(10, 6, 3, 1, trim.body);
            canvas.set(2, 6, glow);
            canvas.set(12, 6, glow);
            canvas.set(8, 8, glow);
        }
    }
}

/// Decal glyphs (5 wide, 4-5 tall) for the flank: `x` the decal colour, `o` its outline,
/// `w` a highlight.
const fn decal_glyph(decal: Decal) -> &'static [&'static str] {
    match decal {
        Decal::None | Decal::Number => &[],
        Decal::Star => &["..w..", "xxwxx", ".xxx.", ".x.x."],
        Decal::Bolt => &["..xw", ".xx.", "xxxx", ".xx.", "xx.."],
        Decal::Flames => &[".....xx", "...xxww", ".xxwwww", "...xxww"],
        Decal::Picks => &["w...w", ".x.x.", "..x..", ".x.x.", "x...x"],
        Decal::Crown => &["w.w.w", "xxxxx", "xwxwx", "xxxxx"],
    }
}

/// 3x5 digits for the crew number decal.
const DIGITS: [[&str; 5]; 10] = [
    ["xxx", "x.x", "x.x", "x.x", "xxx"],
    [".x.", "xx.", ".x.", ".x.", "xxx"],
    ["xxx", "..x", "xxx", "x..", "xxx"],
    ["xxx", "..x", ".xx", "..x", "xxx"],
    ["x.x", "x.x", "xxx", "..x", "..x"],
    ["xxx", "x..", "xxx", "..x", "xxx"],
    ["xxx", "x..", "xxx", "x.x", "xxx"],
    ["xxx", "..x", ".x.", ".x.", ".x."],
    ["xxx", "x.x", "xxx", "x.x", "xxx"],
    ["xxx", "x.x", "xxx", "..x", "xxx"],
];

fn paint_decal(canvas: &mut RigCanvas, look: &RigLook) {
    let trim = paint_ramp(look.cosmetics.trim);
    let decal = look.cosmetics.decal;
    let (color, highlight) = match decal {
        Decal::Crown => (GOLD, CREAM),
        Decal::Flames => (trim.body, trim.light),
        _ => (trim.light, BONE),
    };
    let legend = |ch: char| match ch {
        'x' => Some(color),
        'w' => Some(highlight),
        _ => None,
    };
    match decal {
        Decal::None => {}
        Decal::Number => {
            let digit = (look.player_id.get() % 10) as usize;
            canvas.stamp(&DIGITS[digit], 5, 5, legend);
        }
        Decal::Flames => canvas.stamp(decal_glyph(decal), 3, 5, legend),
        Decal::Bolt => canvas.stamp(decal_glyph(decal), 5, 5, legend),
        _ => canvas.stamp(decal_glyph(decal), 4, 5, legend),
    }
}

fn paint_scorch(canvas: &mut RigCanvas) {
    let primary_body = canvas.get(6, 6);
    canvas.stamp(&SCORCH, 0, 0, |ch| match ch {
        'x' => Some(STONE_DARK),
        'r' => Some(RUST),
        'k' => Some(STEEL_DARK),
        'y' => primary_body,
        _ => None,
    });
}

fn paint_radiator(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    let radiator = class(look.tiers.radiator);
    if look.tiers.radiator <= 1 {
        return;
    }
    let metal = match radiator {
        0 | 1 => COPPER,
        2 => TEAL,
        3 => ICE,
        _ if (pose.ticks / 15).is_multiple_of(2) => MINT,
        _ => ICE,
    };
    let gap = paint_ramp(look.cosmetics.primary).deep;
    // Cooling fins low on the nose; bigger radiators add fins and a second row.
    let fins = (radiator + 1).min(3);
    let rows = if radiator >= 3 { 2 } else { 1 };
    for fin in 0..fins {
        let x = 13 - fin * 2;
        canvas.rect(x, 9 - rows, 1, rows, metal);
        canvas.rect(x - 1, 9 - rows, 1, rows, gap);
    }
}

fn paint_armour(canvas: &mut RigCanvas, look: &RigLook) {
    let armour = class(look.tiers.hull);
    if armour == 0 {
        return;
    }
    let rivet = if armour >= 4 { GOLD } else { STEEL_LIGHT };
    // A bolted skirt over the top of the tracks.
    canvas.rect(1, 10, 14, 1, STEEL);
    for x in (2..15).step_by(4) {
        canvas.set(x, 10, rivet);
    }
    if armour >= 2 {
        // A nose plate under the headlight.
        canvas.rect(14, 7, 1, 3, STEEL_LIGHT);
        canvas.set(14, 8, rivet);
    }
    if armour >= 3 {
        // A cage over the dome.
        canvas.rect(9, 1, 1, 3, STEEL_DARK);
        canvas.rect(12, 2, 1, 2, STEEL_DARK);
        canvas.rect(8, 1, 5, 1, STEEL);
    }
    if armour >= 4 {
        // Heavy plate along the roof line.
        canvas.rect(1, 4, 14, 1, STEEL_LIGHT);
        for x in (3..15).step_by(4) {
            canvas.set(x, 4, rivet);
        }
    }
}

fn paint_mount(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    if look.tiers.mount == 0 {
        return;
    }
    let ring = if pose.gimbal_live { CREAM } else { GOLD };
    canvas.set(15, 6, ring);
    canvas.set(15, 9, ring);
    canvas.set(14, 7, AMBER);
    canvas.set(14, 8, AMBER);
    if look.tiers.mount >= 2 && pose.drill != super::DrillPose::Up {
        // Crown bore collar on the roof, the bit stowed inside.
        canvas.rect(6, 1, 3, 1, STEEL_DARK);
        canvas.set(6, 0, STEEL_LIGHT);
        canvas.set(7, 0, STEEL);
    }
}

fn paint_hardpoint(canvas: &mut RigCanvas) {
    // An empty weapon mount on the dome's roof: a bolted plate (WP-WEAPONS fills it).
    let (x, y) = HARDPOINT;
    canvas.set(x - 1, y + 1, INK);
    canvas.set(x, y + 1, STEEL_DARK);
    canvas.set(x + 1, y + 1, INK);
}

fn paint_cargo(canvas: &mut RigCanvas, look: &RigLook, deck: Deck) {
    let cargo = class(look.tiers.cargo);
    if cargo == 0 {
        return;
    }
    let secondary = paint_ramp(look.cosmetics.secondary);
    let trim = paint_ramp(look.cosmetics.trim);
    let (left, top, right, bottom) = (deck.left, deck.top, 4, 3);
    canvas.rect(left, top, right - left + 1, bottom - top + 1, INK);
    for y in top + 1..=bottom {
        for x in left + 1..right {
            let color = if y == top + 1 {
                secondary.light
            } else if cargo >= 3 && (x - left) % 2 == 0 {
                // Container ribs.
                secondary.shade
            } else {
                secondary.body
            };
            canvas.set(x, y, color);
        }
    }
    match cargo {
        1 => canvas.rect(2, top + 1, 1, bottom - top, trim.body),
        2 => {
            canvas.rect(left + 2, top + 1, 1, bottom - top, trim.body);
            canvas.set(right - 1, top + 2, secondary.deep);
        }
        _ => {
            // A trim band (hazard striped on the biggest holds).
            for x in left + 1..right {
                let color = if cargo >= 4 && x % 2 == 0 {
                    INK
                } else {
                    trim.body
                };
                canvas.set(x, bottom - 1, color);
            }
        }
    }
    if cargo >= 4 {
        // Lifting eyes on the roof of the hold.
        canvas.set(left + 2, top - 1, STEEL_LIGHT);
        canvas.set(right - 2, top - 1, STEEL_LIGHT);
    }
}

fn paint_pods(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    let engine = class(look.tiers.engine);
    if engine == 0 {
        return;
    }
    let secondary = paint_ramp(look.cosmetics.secondary);
    let trim = paint_ramp(look.cosmetics.trim);
    let flame = flame_ramp(look.cosmetics.flame, (pose.ticks / 5) as usize);
    let (left, top) = match engine {
        1 => (-3, 5),
        2 => (-4, 4),
        3 => (-5, 4),
        _ => (-6, 4),
    };
    let bottom = 10;
    // The nacelle: outlined, rounded at the top tail corner, lit on its tail side.
    canvas.rect(left, top + 1, 1 - left, bottom - top, INK);
    canvas.rect(left + 1, top, -left, 1, INK);
    for y in top + 1..bottom {
        for x in left + 1..0 {
            let color = if y == top + 1 || x == left + 1 {
                secondary.light
            } else if y == bottom - 1 || x == -1 {
                secondary.shade
            } else {
                secondary.body
            };
            canvas.set(x, y, color);
        }
    }
    let burning = pose.thrust != Thrust::Off;
    if engine >= 2 {
        // Intake ring (glowing while the thrusters burn) and a trim band.
        let glow = if burning { flame.bright } else { flame.deep };
        canvas.rect(left + 1, top + 2, -1 - left, 1, glow);
        canvas.rect(left + 1, top + 4, -1 - left, 1, trim.body);
    }
    if engine >= 4 {
        let glow = if burning { flame.core } else { flame.mid };
        canvas.rect(left + 2, top + 6, -3 - left, 1, glow);
    }
    if engine >= 3 {
        // A swept tail fin on top of the nacelle.
        canvas.stamp(&["o...", "oso.", "osso"], left, top - 2, |ch| match ch {
            'o' => Some(INK),
            's' => Some(secondary.shade),
            _ => None,
        });
    }
    for &(nozzle_left, width, nozzle_bottom) in nozzles(engine) {
        for y in bottom + 1..=nozzle_bottom {
            for x in nozzle_left..nozzle_left + width {
                let edge = x == nozzle_left || x == nozzle_left + width - 1;
                let color = if edge && width > 2 {
                    INK
                } else if y == nozzle_bottom && burning {
                    flame.bright
                } else if y == nozzle_bottom {
                    STEEL_DARK
                } else {
                    STEEL
                };
                canvas.set(x, y, color);
            }
        }
    }
}

fn paint_mast(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose, deck: Deck) {
    let trim = paint_ramp(look.cosmetics.trim);
    let x = deck.mast_x;
    let base = deck.top;
    // The exhaust stack.
    if class(look.tiers.cargo) == 0 {
        if look.cosmetics.rear != RearAttachment::Stacks {
            canvas.set(1, 3, INK);
            canvas.set(2, 3, STEEL_DARK);
            canvas.set(3, 3, INK);
        }
    } else if look.cosmetics.rear != RearAttachment::Stacks {
        let (stack_x, stack_top) = deck.stack;
        canvas.rect(stack_x, stack_top, 1, base - stack_top, STEEL_DARK);
        canvas.set(stack_x, stack_top - 1, INK);
    }
    let beacon = pose.beacon.unwrap_or(RED);
    match look.cosmetics.roof {
        RoofAttachment::None | RoofAttachment::Halo => {
            canvas.rect(x, base - 2, 1, 2, STEEL_LIGHT);
            canvas.set(x, base - 3, beacon);
            if look.cosmetics.roof == RoofAttachment::Halo {
                let bob = i32::from((pose.ticks / 30).is_multiple_of(2));
                let y = base - 7 + bob;
                canvas.rect(x - 1, y, 3, 1, CREAM);
                canvas.set(x - 2, y + 1, GOLD);
                canvas.set(x + 2, y + 1, GOLD);
                canvas.rect(x - 1, y + 2, 3, 1, AMBER);
            }
        }
        RoofAttachment::Antenna => {
            canvas.rect(x, base - 6, 1, 6, STEEL_LIGHT);
            canvas.rect(x - 1, base - 3, 3, 1, STEEL);
            canvas.set(x, base - 7, beacon);
            canvas.set(x, base - 8, trim.light);
        }
        RoofAttachment::Dish => {
            canvas.rect(x, base - 2, 1, 2, STEEL_LIGHT);
            canvas.stamp(
                &["oo..", "olmo", "olm.", "olmo", "oo.."],
                x - 1,
                base - 7,
                |ch| match ch {
                    'o' => Some(INK),
                    'l' => Some(MIST),
                    'm' => Some(STEEL),
                    _ => None,
                },
            );
            canvas.set(x + 2, base - 5, beacon);
        }
        RoofAttachment::Siren => {
            canvas.rect(x, base - 2, 1, 2, STEEL_LIGHT);
            let phase = (pose.ticks / 6).is_multiple_of(2);
            let (left, right) = if phase { (RED, MAROON) } else { (MAROON, RED) };
            canvas.rect(x - 2, base - 5, 5, 3, INK);
            canvas.set(x - 1, base - 4, left);
            canvas.set(x, base - 4, BONE);
            canvas.set(x + 1, base - 4, right);
        }
        RoofAttachment::LightBar => {
            canvas.rect(x, base - 2, 1, 2, STEEL_LIGHT);
            canvas.rect(x - 2, base - 4, 5, 2, INK);
            canvas.rect(x - 1, base - 4, 3, 1, CREAM);
            canvas.set(x, base - 4, BONE);
            canvas.set(x, base - 3, beacon);
        }
    }
}

fn paint_front(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose) {
    match look.cosmetics.front {
        FrontAttachment::None => {}
        FrontAttachment::Floodlamp => {
            canvas.stamp(&["ooo", "osl", "ooo"], 14, 2, |ch| match ch {
                'o' => Some(INK),
                's' => Some(STEEL),
                'l' => Some(if pose.damaged { STEEL_DARK } else { CREAM }),
                _ => None,
            });
        }
        FrontAttachment::BullBar => {
            // A steel bumper across the front of the tracks.
            canvas.stamp(&["oooo", "olllo", ".oooo"], 13, 9, |ch| match ch {
                'o' => Some(INK),
                'l' => Some(STEEL_LIGHT),
                _ => None,
            });
        }
    }
}

fn paint_rear(canvas: &mut RigCanvas, look: &RigLook, pose: &RigPose, deck: Deck) {
    let trim = paint_ramp(look.cosmetics.trim);
    match look.cosmetics.rear {
        RearAttachment::None => {}
        RearAttachment::Spoiler => {
            let y = deck.top - 2;
            let right = deck.left + 1;
            canvas.rect(right - 4, y, 5, 2, INK);
            canvas.rect(right - 3, y, 4, 1, trim.light);
            canvas.rect(right - 3, y + 1, 4, 1, trim.shade);
            canvas.rect(right - 1, y + 2, 1, deck.top - y - 1, INK);
        }
        RearAttachment::Pennant => {
            let x = deck.left;
            let top = deck.top - 7;
            canvas.rect(x, top, 1, 7, STEEL_LIGHT);
            let wave = (pose.ticks / 12).is_multiple_of(2);
            let rows: [&str; 3] = if wave {
                ["lll.", "bbbb", "bb.."]
            } else {
                [".lll", "bbbb", "..bb"]
            };
            canvas.stamp(&rows, x - 4, top, |ch| match ch {
                'l' => Some(trim.light),
                'b' => Some(trim.body),
                _ => None,
            });
        }
        RearAttachment::Stacks => {
            let (x, top) = if class(look.tiers.cargo) == 0 {
                (4, 1)
            } else {
                (deck.stack.0, deck.stack.1)
            };
            let base = if class(look.tiers.cargo) == 0 {
                4
            } else {
                deck.top
            };
            for pipe in [x - 1, x] {
                canvas.rect(pipe, top, 1, base - top, MIST);
                canvas.set(pipe, top - 1, INK);
            }
            canvas.rect(x - 1, top + 1, 2, 1, STEEL_LIGHT);
        }
    }
}
