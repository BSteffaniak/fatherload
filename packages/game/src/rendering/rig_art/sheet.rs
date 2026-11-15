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

fn draw_paints(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Paint jobs", 8, 6, TextStyle::Bold, GOLD);
    let jobs = paint_jobs();
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

fn draw_drills(draw: &mut RaylibDrawHandle<'_>, width: i32) {
    ui_kit::text(draw, "Drill heads", 8, 6, TextStyle::Bold, GOLD);
    let column_width = (width - 70) / 15;
    let scale = 3;
    for (row, style) in DrillStyle::ALL.iter().enumerate() {
        let top = 30 + row as i32 * 55;
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
