//! Rig reference sheets for screenshot review: the rig at every visual tier class and pose,
//! paint jobs with patterns, decals and attachments, and every drill tier in every style.

use fatherload_simulation::{
    PlayerId, RigCosmetics,
    cosmetics::{
        Decal, DrillStyle, FlameColor, FrontAttachment, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::ui_kit::{self, TextStyle};

use super::{DrillPose, RigLook, RigPose, RigTiers, Thrust, paint_rig};

/// Which rig sheet to draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RigSheet {
    /// Every tier class (columns) parked, climbing and drilling (rows).
    Tiers,
    /// Paint jobs, patterns, decals and attachments.
    Paints,
    /// Every drill tier (columns) in every drill style (rows).
    Drills,
    /// Every hardpoint weapon (columns) at each upgrade tier and aim (rows).
    Weapons,
    /// The pilot's sprite sheet (Part II F5): every pose and its frames, then crew paint jobs
    /// and suit tiers.
    Pilots,
    /// The milestone-reward cosmetics (WP-FEEL part 2) on rigs of several tiers.
    Rewards,
}

/// Tiers that start each visual class (plus the top tier).
const TIER_COLUMNS: [u8; 6] = [1, 3, 6, 10, 13, 15];

const fn look(cosmetics: RigCosmetics, tiers: RigTiers) -> RigLook {
    RigLook {
        player_id: PlayerId::new(1),
        cosmetics,
        tiers,
    }
}

/// Sample paint jobs for the paints sheet.
fn paint_jobs() -> Vec<(&'static str, RigCosmetics)> {
    let job = |primary, secondary, trim| RigCosmetics {
        primary,
        secondary,
        trim,
        ..RigCosmetics::default()
    };
    vec![
        (
            "Classic",
            RigCosmetics {
                pattern: HullPattern::Racing,
                ..RigCosmetics::default_for(PlayerId::new(1))
            },
        ),
        (
            "Cobalt racer",
            RigCosmetics {
                pattern: HullPattern::Racing,
                decal: Decal::Number,
                flame: FlameColor::Blue,
                rear: RearAttachment::Spoiler,
                roof: RoofAttachment::Antenna,
                ..job(PaintColor::Cobalt, PaintColor::Pearl, PaintColor::Crimson)
            },
        ),
        (
            "Hot rod",
            RigCosmetics {
                decal: Decal::Flames,
                pattern: HullPattern::Solid,
                flame: FlameColor::Gold,
                rear: RearAttachment::Stacks,
                front: FrontAttachment::BullBar,
                ..job(PaintColor::Crimson, PaintColor::Graphite, PaintColor::Gold)
            },
        ),
        (
            "Jungle",
            RigCosmetics {
                pattern: HullPattern::Camo,
                drill_style: DrillStyle::Serrated,
                flame: FlameColor::Green,
                roof: RoofAttachment::Dish,
                ..job(PaintColor::Forest, PaintColor::Sand, PaintColor::Lime)
            },
        ),
        (
            "Hazard",
            RigCosmetics {
                pattern: HullPattern::Hazard,
                decal: Decal::Bolt,
                roof: RoofAttachment::Siren,
                front: FrontAttachment::Floodlamp,
                ..job(
                    PaintColor::Tangerine,
                    PaintColor::Graphite,
                    PaintColor::Gold,
                )
            },
        ),
        (
            "Blossom",
            RigCosmetics {
                pattern: HullPattern::TwoTone,
                decal: Decal::Star,
                flame: FlameColor::Pink,
                rear: RearAttachment::Pennant,
                drill_style: DrillStyle::Auger,
                ..job(PaintColor::Blossom, PaintColor::Pearl, PaintColor::Violet)
            },
        ),
        (
            "Checker",
            RigCosmetics {
                pattern: HullPattern::Checker,
                decal: Decal::Picks,
                flame: FlameColor::White,
                roof: RoofAttachment::LightBar,
                drill_style: DrillStyle::Crusher,
                ..job(PaintColor::Pearl, PaintColor::Steel, PaintColor::Cobalt)
            },
        ),
        (
            "Unlocks",
            RigCosmetics {
                pattern: HullPattern::Circuit,
                decal: Decal::Crown,
                flame: FlameColor::Prism,
                roof: RoofAttachment::Halo,
                drill_style: DrillStyle::Spire,
                ..job(PaintColor::Stealth, PaintColor::Graphite, PaintColor::Mint)
            },
        ),
    ]
}

/// Draws a rig sheet over the whole UI canvas.
pub fn draw_rig_sheet(draw: &mut RaylibDrawHandle<'_>, sheet: RigSheet) {
    let canvas = ui_kit::canvas();
    draw.draw_rectangle(0, 0, canvas.width, canvas.height, SLATE_DARK);
    match sheet {
        RigSheet::Tiers => draw_tiers(draw, canvas.width),
        RigSheet::Paints => draw_paints(draw, canvas.width),
        RigSheet::Drills => draw_drills(draw, canvas.width),
        RigSheet::Weapons => draw_weapons(draw, canvas.width),
        RigSheet::Pilots => draw_pilots(draw, canvas.width),
        RigSheet::Rewards => draw_rewards(draw, canvas.width),
    }
}

/// The pilot's poses (rows of frames) and paint jobs, large.
fn draw_pilots(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    use crate::rendering::pilot_art::{PilotFrame, PilotLook, paint_pilot};
    use fatherload_simulation::pilot::{PilotGear, PilotPose};
    ui_kit::text(draw, "Pilot on foot", 8, 6, TextStyle::Bold, GOLD);
    let poses: [(&str, PilotPose, f32, bool); 13] = [
        ("Idle", PilotPose::Idle, 1.5, false),
        ("Walk", PilotPose::Walk, 8.0, false),
        ("Run", PilotPose::Run, 12.0, false),
        ("Jump", PilotPose::Jump, 1.0, false),
        ("Fall", PilotPose::Fall, 1.0, false),
        ("Climb", PilotPose::Climb, 6.0, false),
        ("Crouch", PilotPose::Crouch, 1.0, false),
        ("Crawl", PilotPose::Crawl, 6.0, false),
        ("Swim", PilotPose::Swim, 4.0, false),
        ("Hurt", PilotPose::Hurt, 1.0, false),
        ("Jetpack", PilotPose::Jetpack, 10.0, false),
        ("Hang", PilotPose::Hang, 1.0, false),
        ("Pick", PilotPose::Idle, 6.6, true),
    ];
    let scale = 2;
    let columns = 7;
    let column_width = (width - 16) / columns;
    let look = PilotLook::new(
        RigCosmetics::default_for(PlayerId::new(1)),
        PilotGear {
            jetpack: 1,
            suit: 2,
        },
    );
    for (index, (name, pose, rate, swinging)) in poses.into_iter().enumerate() {
        let column = index as i32 % columns;
        let row = index as i32 / columns;
        let cx = 8 + column * column_width + column_width / 2;
        let feet = 78 + row * 84;
        let label_width = ui_kit::text_width(name, TextStyle::Bold);
        ui_kit::text(
            draw,
            name,
            cx - label_width / 2,
            feet + 6,
            TextStyle::Bold,
            BONE,
        );
        // Two frames side by side (one cycle step apart).
        for frame in 0..2 {
            let seconds = frame as f32 / rate.max(1.0);
            let canvas = paint_pilot(
                &look,
                PilotFrame {
                    pose,
                    seconds,
                    swinging,
                },
            );
            let x = cx - 14 + frame * 28;
            canvas.draw(
                draw,
                Vector2::new(x as f32, feet as f32),
                scale as f32,
                false,
                None,
            );
        }
    }
    // Paint jobs and suit tiers.
    let jobs = paint_jobs();
    let top = 78 + 2 * 84 + 14;
    ui_kit::text(draw, "Suit paint and tiers", 8, top, TextStyle::Bold, GOLD);
    for (index, (name, cosmetics)) in jobs.into_iter().take(8).enumerate() {
        let cx = 8 + index as i32 * ((width - 16) / 8) + (width - 16) / 16;
        let look = PilotLook::new(
            cosmetics,
            PilotGear {
                jetpack: u8::from(index % 2 == 0),
                suit: (index % 4) as u8,
            },
        );
        let canvas = paint_pilot(
            &look,
            PilotFrame {
                pose: PilotPose::Walk,
                seconds: index as f32 * 0.13,
                swinging: false,
            },
        );
        canvas.draw(
            draw,
            Vector2::new(cx as f32, (top + 72) as f32),
            2.0,
            false,
            None,
        );
        let label_width = ui_kit::text_width(name, TextStyle::Body);
        ui_kit::text(
            draw,
            name,
            cx - label_width / 2,
            top + 76,
            TextStyle::Body,
            super::paint_accent(cosmetics.primary),
        );
    }
}

/// One cell: a rig drawn at `scale` with its body box centred on `(cx, top)`.
fn cell(
    draw: &mut RaylibDrawHandle<'_>,
    rig: &RigLook,
    pose: &RigPose,
    cx: i32,
    top: i32,
    scale: i32,
) {
    let painted = paint_rig(rig, pose);
    let x = cx - super::BODY_WIDTH * scale / 2;
    painted.draw(
        draw,
        Vector2::new(x as f32, top as f32),
        scale as f32,
        false,
    );
}

fn draw_tiers(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Rig tiers", 8, 6, TextStyle::Bold, GOLD);
    let poses = [
        ("Parked", RigPose::idle(41)),
        (
            "Climbing",
            RigPose {
                thrust: Thrust::Climb,
                drill: DrillPose::Nose { spinning: true },
                ..RigPose::idle(41)
            },
        ),
        (
            "Drilling",
            RigPose {
                drill: DrillPose::Down,
                ..RigPose::idle(41)
            },
        ),
    ];
    let column_width = (width - 60) / TIER_COLUMNS.len() as i32;
    let scale = 2;
    for (column, tier) in TIER_COLUMNS.into_iter().enumerate() {
        let cx = 60 + column as i32 * column_width + column_width / 2;
        let label = format!("Tier {tier}");
        let label_width = ui_kit::text_width(&label, TextStyle::Bold);
        ui_kit::text(
            draw,
            &label,
            cx - label_width / 2,
            20,
            TextStyle::Bold,
            BONE,
        );
        for (row, (name, pose)) in poses.iter().enumerate() {
            let top = 54 + row as i32 * 104;
            if column == 0 {
                ui_kit::text(draw, name, 6, top + 8, TextStyle::Body, MIST);
            }
            let rig = look(
                RigCosmetics::default_for(PlayerId::new(1)),
                RigTiers::all(tier),
            );
            cell(draw, &rig, pose, cx, top, scale);
        }
    }
}

/// The milestone-reward looks for the rewards sheet: every reward paint, flame, pattern, decal,
/// the corkscrew drill, the depth trophy and the trail lantern.
fn reward_jobs() -> Vec<(&'static str, RigCosmetics)> {
    let job = |primary, secondary, trim| RigCosmetics {
        primary,
        secondary,
        trim,
        ..RigCosmetics::default()
    };
    vec![
        (
            "Jade tiger",
            RigCosmetics {
                pattern: HullPattern::Tiger,
                drill_style: DrillStyle::Corkscrew,
                decal: Decal::Wrench,
                ..job(PaintColor::Jade, PaintColor::Graphite, PaintColor::Gold)
            },
        ),
        (
            "Glowcap lantern",
            RigCosmetics {
                decal: Decal::Gem,
                flame: FlameColor::Aether,
                rear: RearAttachment::Lantern,
                pattern: HullPattern::Racing,
                ..job(PaintColor::Glowcap, PaintColor::Forest, PaintColor::Mint)
            },
        ),
        (
            "Mirror chrome",
            RigCosmetics {
                decal: Decal::Wings,
                flame: FlameColor::Plasma,
                roof: RoofAttachment::Trophy,
                pattern: HullPattern::TwoTone,
                ..job(PaintColor::Chrome, PaintColor::Steel, PaintColor::Crimson)
            },
        ),
        (
            "Abyss starfield",
            RigCosmetics {
                pattern: HullPattern::Starfield,
                decal: Decal::Moon,
                flame: FlameColor::Aether,
                ..job(PaintColor::Abyss, PaintColor::Chrome, PaintColor::Sky)
            },
        ),
        (
            "Magma ember",
            RigCosmetics {
                decal: Decal::Fang,
                flame: FlameColor::Ember,
                pattern: HullPattern::Solid,
                rear: RearAttachment::Stacks,
                ..job(PaintColor::Magma, PaintColor::Stealth, PaintColor::Gold)
            },
        ),
        (
            "Claim flag",
            RigCosmetics {
                decal: Decal::Flag,
                flame: FlameColor::Plasma,
                drill_style: DrillStyle::Corkscrew,
                pattern: HullPattern::Racing,
                ..job(PaintColor::Sand, PaintColor::Copper, PaintColor::Crimson)
            },
        ),
        (
            "Trophy hunter",
            RigCosmetics {
                roof: RoofAttachment::Trophy,
                pattern: HullPattern::Tiger,
                decal: Decal::Gem,
                flame: FlameColor::Ember,
                ..job(PaintColor::Gold, PaintColor::Graphite, PaintColor::Magma)
            },
        ),
        (
            "Night trail",
            RigCosmetics {
                pattern: HullPattern::Starfield,
                rear: RearAttachment::Lantern,
                decal: Decal::Wings,
                flame: FlameColor::Aether,
                ..job(PaintColor::Violet, PaintColor::Abyss, PaintColor::Glowcap)
            },
        ),
    ]
}

fn draw_rewards(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Milestone rewards", 8, 6, TextStyle::Bold, GOLD);
    draw_jobs(draw, width, reward_jobs());
}

fn draw_paints(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Paint jobs", 8, 6, TextStyle::Bold, GOLD);
    draw_jobs(draw, width, paint_jobs());
}

/// Eight looks in a 4x2 grid at four times size.
fn draw_jobs(draw: &mut RaylibDrawHandle<'_>, width: i32, jobs: Vec<(&'static str, RigCosmetics)>) {
    let per_row = 4;
    let column_width = width / per_row;
    let scale = 4;
    for (index, (name, cosmetics)) in jobs.into_iter().enumerate() {
        let column = index as i32 % per_row;
        let row = index as i32 / per_row;
        let cx = column * column_width + column_width / 2;
        let top = 64 + row * 168;
        let label_width = ui_kit::text_width(name, TextStyle::Bold);
        ui_kit::text(
            draw,
            name,
            cx - label_width / 2,
            top - 48,
            TextStyle::Bold,
            super::paint_accent(cosmetics.primary),
        );
        let tiers = RigTiers::all([4, 7, 11, 14][index % 4]);
        let pose = RigPose {
            thrust: if index % 2 == 1 {
                Thrust::Climb
            } else {
                Thrust::Off
            },
            beacon: Some(GOLD),
            ..RigPose::idle(30 + index as u64 * 7)
        };
        cell(draw, &look(cosmetics, tiers), &pose, cx, top, scale);
    }
}

/// A weapon on a rig drawn at `scale` (body box at `(x, top)`), pivoting on the hardpoint and
/// pointing along `aim`, with a gold notch per tier above Mk I (as in the world).
fn mounted_gun(
    draw: &mut RaylibDrawHandle<'_>,
    weapon: fatherload_simulation::WeaponKind,
    mk: u8,
    (x, top): (i32, i32),
    scale: f32,
    aim: (f32, f32),
) {
    use crate::rendering::pixel_art::{sprite, sprites::weapons};
    let (column, row) = super::HARDPOINT;
    let pivot = (
        (column as f32 + 0.5).mul_add(scale, x as f32),
        (row as f32 + 1.0).mul_add(scale, top as f32),
    );
    let gun = weapons::mounted(weapon);
    let reach = (gun.width() as f32).mul_add(0.5, -1.0) * scale;
    draw.draw_circle(
        pivot.0 as i32,
        (pivot.1 + scale) as i32,
        scale * 2.0,
        STEEL_DARK,
    );
    sprite::draw_sprite_rotated_flipped(
        draw,
        gun,
        0,
        Vector2::new(aim.0.mul_add(reach, pivot.0), aim.1.mul_add(reach, pivot.1)),
        scale,
        aim.1.atan2(aim.0).to_degrees(),
        aim.0 < 0.0,
    );
    for notch in 0..mk {
        let along = f32::from(notch).mul_add(2.0, 2.0) * scale;
        let nx = aim.0.mul_add(along, pivot.0);
        let ny = aim.1.mul_add(along, pivot.1);
        draw.draw_rectangle(nx as i32, ny as i32, scale as i32, scale as i32, GOLD);
    }
}

fn draw_weapons(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    use fatherload_simulation::WeaponKind;
    ui_kit::text(draw, "Hardpoint weapons", 8, 6, TextStyle::Bold, GOLD);
    let rows: [(&str, u8, (f32, f32)); 3] = [
        ("Mk I, level", 0, (1.0, 0.0)),
        ("Mk III, up", 2, (0.7, -0.7)),
        ("Mk IV, left", 3, (-0.94, -0.34)),
    ];
    let column_width = (width - 70) / WeaponKind::ALL.len() as i32;
    let scale = 3;
    for (column, weapon) in WeaponKind::ALL.into_iter().enumerate() {
        let cx = 70 + column as i32 * column_width + column_width / 2;
        let label = weapon.name();
        let label_width = ui_kit::text_width(label, TextStyle::Bold);
        ui_kit::text(draw, label, cx - label_width / 2, 22, TextStyle::Bold, BONE);
        for (row, (name, mk, aim)) in rows.iter().enumerate() {
            let top = 70 + row as i32 * 80;
            if column == 0 {
                ui_kit::text(draw, name, 4, top + 12, TextStyle::Body, MIST);
            }
            let rig = look(
                RigCosmetics::default_for(PlayerId::new(1)),
                RigTiers::all([3, 9, 14][row]),
            );
            let x = cx - super::BODY_WIDTH * scale / 2;
            let facing_left = aim.0 < 0.0;
            paint_rig(&rig, &RigPose::idle(12)).draw(
                draw,
                Vector2::new(x as f32, top as f32),
                scale as f32,
                facing_left,
            );
            let pivot_box = if facing_left {
                // Mirror the hardpoint column for a rig facing left.
                let (column, _) = super::HARDPOINT;
                x + (super::BODY_WIDTH - 1 - 2 * column) * scale
            } else {
                x
            };
            mounted_gun(draw, weapon, *mk, (pivot_box, top), scale as f32, *aim);
        }
    }
}

fn draw_drills(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Drill heads", 8, 6, TextStyle::Bold, GOLD);
    let column_width = (width - 70) / 15;
    // Rows share the canvas height (more drill styles, tighter rows).
    let row_height = ((ui_kit::canvas().height - 34) / DrillStyle::ALL.len() as i32).min(55);
    let scale = if row_height >= 50 { 3 } else { 2 };
    for (row, style) in DrillStyle::ALL.iter().enumerate() {
        let top = 30 + row as i32 * row_height;
        ui_kit::text(draw, style.label(), 6, top + 14, TextStyle::Body, MIST);
        for tier in 1..=15_u8 {
            let cx = 70 + (i32::from(tier) - 1) * column_width + column_width / 2;
            if row == 0 {
                let label = tier.to_string();
                let label_width = ui_kit::text_width(&label, TextStyle::Body);
                ui_kit::text(
                    draw,
                    &label,
                    cx - label_width / 2,
                    18,
                    TextStyle::Body,
                    BONE,
                );
            }
            let rig = look(
                RigCosmetics {
                    drill_style: *style,
                    ..RigCosmetics::default_for(PlayerId::new(1))
                },
                RigTiers {
                    drill: tier,
                    ..RigTiers::STOCK
                },
            );
            let painted = paint_rig(
                &rig,
                &RigPose {
                    drill: DrillPose::Down,
                    ..RigPose::idle(0)
                },
            );
            // Only the drill: rows from the collar down, shifted up under the label.
            let x = cx - super::BODY_WIDTH * scale / 2;
            let mut only_drill = super::RigCanvas::new();
            for y in 12..30 {
                for dx in -2..18 {
                    if let Some(color) = painted.get(dx, y) {
                        only_drill.set(dx, y, color);
                    }
                }
            }
            only_drill.draw(
                draw,
                Vector2::new(x as f32, (top - 12 * scale + 2) as f32),
                scale as f32,
                false,
            );
        }
    }
}
