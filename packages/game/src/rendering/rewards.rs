//! Upgrade reward moments on screen (WP-FEEL part 2): the shop's showcase card (the rig now and
//! with the next tier, before/after stat bars and measured "what this changes" lines), the
//! purchase moment in the shop room (the part bolting on: old part, flash, new part, sparks,
//! light and a camera punch), milestone banners, the progression screen with its trophy shelf,
//! and the medal icons they share.
//!
//! Everything reads the simulation's numbers ([`fatherload_simulation::upgrade_preview`],
//! [`fatherload_simulation::milestone`]) and the seat's reward state (`shell::rewards`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "UI art pixels are small integers drawn at float scales"
)]

use fatherload_simulation::{
    CosmeticItem, Player,
    economy::{UpgradeKind, current_level, upgrade_offers_for, upgrade_tier_name},
    milestone::{Medal, Milestone, MilestoneGroup},
    upgrade_preview::{StatBar, UpgradePreview, preview_between, upgrade_preview_for},
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    layout::{self, ItemEntry, ModalContent, Section, SectionItem, UiLayout},
    pixel_art::palette,
    pixel_art::{
        sprite::{SpriteDef, draw_sprite},
        sprites::{icons, menu_icons},
    },
    prompts::hint,
    rig_art::{self, DrillPose, RigCanvas, RigLook, RigPose, RigTiers, Thrust},
    screen::UiFrame,
    ui_kit::{self, PanelStyle, TextStyle},
};
use crate::{
    input::prompts::Prompt,
    shell::{
        SeatUi,
        rewards::{INSTALL_TICKS, InstallMoment},
    },
};

// ---------------------------------------------------------------------------------------------
// Medals
// ---------------------------------------------------------------------------------------------

/// The medal icon of `medal` (`None`: not earned yet).
#[must_use]
pub(super) const fn medal_icon(medal: Option<Medal>) -> &'static SpriteDef {
    match medal {
        None => &menu_icons::MEDAL_LOCKED,
        Some(Medal::Bronze) => &menu_icons::MEDAL_BRONZE,
        Some(Medal::Silver) => &menu_icons::MEDAL_SILVER,
        Some(Medal::Gold) => &menu_icons::MEDAL_GOLD,
        Some(Medal::Platinum) => &menu_icons::MEDAL_PLATINUM,
    }
}

/// A medal's text colour.
#[must_use]
pub(super) const fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Bronze => COPPER,
        Medal::Silver => STEEL_LIGHT,
        Medal::Gold => GOLD,
        Medal::Platinum => ICE,
    }
}

/// A medal drawn `scale` times its icon size with its top-left at `(x, y)`.
fn draw_medal(draw: &mut impl RaylibDraw, medal: Option<Medal>, x: i32, y: i32, scale: i32) {
    draw_sprite(
        draw,
        medal_icon(medal),
        0,
        Vector2::new(x as f32, y as f32),
        scale as f32,
        Color::WHITE,
    );
}

/// The garage slot a cosmetic goes in, for unlock lines ("Wrench decal").
#[must_use]
pub(super) const fn cosmetic_slot(item: CosmeticItem) -> &'static str {
    match item {
        CosmeticItem::Paint(_) => "paint",
        CosmeticItem::Flame(_) => "flame",
        CosmeticItem::Drill(_) => "drill style",
        CosmeticItem::Pattern(_) => "pattern",
        CosmeticItem::Decal(_) => "decal",
        CosmeticItem::Roof(_) => "roof piece",
        CosmeticItem::Front(_) => "nose piece",
        CosmeticItem::Rear(_) => "tail piece",
    }
}

/// "Wrench decal, Gemstone decal": what `milestone` unlocks.
#[must_use]
pub(super) fn reward_line(milestone: Milestone) -> String {
    milestone
        .rewards()
        .iter()
        .map(|item| format!("{} {}", item.label(), cosmetic_slot(*item)))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------------------------
// Banners
// ---------------------------------------------------------------------------------------------

/// The seat's milestone banner across the top of its view (below a quest fanfare showing at
/// the same time). The Act I -> II unlock gets a larger sky banner.
///
/// `avoid` is a docked counter panel (inside a building): the banner then sits in the free band
/// beside it, below the shop's showcase card, instead of over the panel.
#[allow(clippy::too_many_lines, reason = "one banner laid out top to bottom")]
pub(super) fn draw_reward_banner(
    draw: &mut RaylibDrawHandle<'_>,
    seat: &SeatUi,
    now: u64,
    avoid: Option<Rectangle>,
) {
    let Some((banner, age)) = seat.rewards.banner(now) else {
        return;
    };
    let milestone = banner.milestone;
    let act_two = milestone == Milestone::ActTwoUnlocked;
    let canvas = ui_kit::canvas();
    // The free band to centre in: the whole view, or beside a docked counter panel.
    let (band_left, band_width, beside) = match avoid {
        Some(panel) if canvas.width - (panel.x + panel.width) as i32 - 16 >= 220 => {
            let left = (panel.x + panel.width) as i32 + 8;
            (left, canvas.width - left, true)
        }
        _ => (0, canvas.width, false),
    };
    let width = (band_width - 24).min(if act_two { 340 } else { 300 });
    if width < 120 {
        return;
    }
    let medal = milestone.medal();
    let color = if act_two {
        SKY_LIGHT
    } else {
        medal_color(medal)
    };
    let heading = if act_two { "ACT II" } else { "MILESTONE" };
    let text_left = 30;
    let mut body = format!(
        "Unlocked: {}. Try it at the paint bay.",
        reward_line(milestone)
    );
    if act_two {
        body = format!("Your tank and intake can cross the dead-air gap. {body}");
    }
    let mut details = ui_kit::wrap(&body, width - text_left - 10, TextStyle::Body);
    details.truncate(if act_two { 4 } else { 3 });
    let heading_height = TextStyle::Title.line_height();
    let title_y = 6 + heading_height;
    let details_y = title_y + TextStyle::Bold.line_height() + 1;
    let height =
        details_y + 5 + i32::try_from(details.len()).unwrap_or(0) * TextStyle::Body.line_height();
    // Drops in, then lifts away over its last quarter second.
    let duration = banner.duration();
    let drop = i32::try_from(12_u64.saturating_sub(age)).unwrap_or(0) * 4;
    let lift = i32::try_from((age + 15).saturating_sub(duration)).unwrap_or(0) * 6;
    let fanfare_clear = seat
        .fanfare
        .as_ref()
        .is_some_and(|fanfare| fanfare.showing(now));
    let top = if beside {
        // Below the showcase card, above the toast.
        (canvas.height - height - 48).min(176)
    } else if let Some(panel) = avoid
        && panel.y as i32 > height + 50
    {
        // A counter panel docked along the bottom: just above it, clear of the top toast.
        panel.y as i32 - height - 4
    } else if fanfare_clear {
        112
    } else {
        46
    };
    let x = band_left + (band_width - width) / 2;
    let y = top - drop - lift;
    let rect = Rectangle::new(x as f32, y as f32, width as f32, height as f32);
    ui_kit::panel(draw, rect, PanelStyle::Accent);
    if act_two {
        draw_sky_fill(draw, rect, age);
    }
    // A diagonal glint sweeping across the panel now and then.
    let shine = ((age * 6) % (width as u64 + 120)) as i32 - 30;
    for row in 3..height - 3 {
        for dx in 0..4 {
            let sx = x + shine + dx - row / 2;
            if sx > x + 3 && sx < x + width - 3 {
                draw.draw_rectangle(sx, y + row, 1, 1, palette::with_alpha(BONE, 40));
            }
        }
    }
    let bob = i32::from((age / 20) % 2 == 0);
    draw_medal(draw, Some(medal), x + 6, y + (height - 18) / 2 - bob, 2);
    let centre = x + text_left + (width - text_left - 10) / 2;
    ui_kit::text_centred(draw, heading, centre, y + 6, TextStyle::Title, color);
    ui_kit::text_centred(
        draw,
        &ui_kit::truncate(
            &format!("{} ({} medal)", milestone.title(), medal.label()),
            width - text_left - 10,
            TextStyle::Bold,
        ),
        centre,
        y + title_y,
        TextStyle::Bold,
        BONE,
    );
    for (index, line) in details.iter().enumerate() {
        ui_kit::text_centred(
            draw,
            line,
            centre,
            y + details_y + i32::try_from(index).unwrap_or(0) * TextStyle::Body.line_height(),
            TextStyle::Body,
            if act_two && index == 0 { ICE } else { MIST },
        );
    }
    draw_sparkles(draw, rect, age, color);
}

/// The Act II banner's inner sky: night to dawn bands and a few stars.
fn draw_sky_fill(draw: &mut impl RaylibDraw, rect: Rectangle, age: u64) {
    let (x, y, w, h) = (
        rect.x as i32 + 3,
        rect.y as i32 + 3,
        rect.width as i32 - 6,
        rect.height as i32 - 6,
    );
    let bands = [NIGHT, DEEP_BLUE, OCEAN, DEEP_BLUE, NIGHT];
    for (index, color) in bands.iter().enumerate() {
        let top = y + h * index as i32 / bands.len() as i32;
        let bottom = y + h * (index as i32 + 1) / bands.len() as i32;
        draw.draw_rectangle(x, top, w, bottom - top, palette::with_alpha(*color, 200));
    }
    for star in 0..14_i32 {
        let sx = x + (star * 37 + 11) % w.max(1);
        let sy = y + (star * 23 + 5) % h.max(1);
        let lit = (star as u64 + age / 8).is_multiple_of(3);
        draw.draw_rectangle(sx, sy, 1, 1, if lit { BONE } else { SKY });
    }
}

/// Twinkling four-point sparkles around a banner's corners and edges.
fn draw_sparkles(draw: &mut impl RaylibDraw, rect: Rectangle, age: u64, color: Color) {
    let (x, y, w, h) = (
        rect.x as i32,
        rect.y as i32,
        rect.width as i32,
        rect.height as i32,
    );
    let spots = [
        (x + 4, y + 4),
        (x + w - 7, y + 4),
        (x + w - 7, y + h - 7),
        (x + w / 3, y - 2),
        (x + 2 * w / 3, y + h - 1),
    ];
    for (index, (sx, sy)) in spots.into_iter().enumerate() {
        let phase = (age / 5 + index as u64) % 4;
        let tint = if phase == 0 { CREAM } else { color };
        if phase == 3 {
            continue;
        }
        draw.draw_rectangle(sx, sy + 1, 3, 1, tint);
        draw.draw_rectangle(sx + 1, sy, 1, 3, tint);
    }
}

// ---------------------------------------------------------------------------------------------
// The shop showcase
// ---------------------------------------------------------------------------------------------

/// `tiers` with `kind`'s visible part at `level` (lines the rig art does not show leave it).
const fn tiers_with(mut tiers: RigTiers, kind: UpgradeKind, level: u8) -> RigTiers {
    match kind {
        UpgradeKind::Drill => tiers.drill = level,
        UpgradeKind::Engine => tiers.engine = level,
        UpgradeKind::CargoBay => tiers.cargo = level,
        UpgradeKind::Hull => tiers.hull = level,
        UpgradeKind::Radiator => tiers.radiator = level,
        UpgradeKind::DrillMount => tiers.mount = level,
        UpgradeKind::FuelTank
        | UpgradeKind::Scanner
        | UpgradeKind::BombPack
        | UpgradeKind::HeatShield
        | UpgradeKind::LavaPlating
        | UpgradeKind::GasScrubber
        | UpgradeKind::PressureBracing
        | UpgradeKind::BoulderDeflector
        | UpgradeKind::AltitudeIntake
        | UpgradeKind::DeIcer
        | UpgradeKind::PressurizedHull
        | UpgradeKind::LifeSupport
        | UpgradeKind::DescentPressureHull
        | UpgradeKind::BioFilter
        | UpgradeKind::LampII
        | UpgradeKind::SonarSuite
        | UpgradeKind::CryoHull
        | UpgradeKind::GravityAnchor
        | UpgradeKind::PhaseDrill
        | UpgradeKind::HeartKey => {}
    }
    tiers
}

/// Whether `kind` changes how the rig looks.
const fn visible_line(kind: UpgradeKind) -> bool {
    matches!(
        kind,
        UpgradeKind::Drill
            | UpgradeKind::Engine
            | UpgradeKind::CargoBay
            | UpgradeKind::Hull
            | UpgradeKind::Radiator
            | UpgradeKind::DrillMount
    )
}

/// The pose that shows off `kind`'s part.
fn showcase_pose(kind: UpgradeKind, ticks: u64) -> RigPose {
    let idle = RigPose::idle(ticks);
    match kind {
        UpgradeKind::Drill => RigPose {
            drill: DrillPose::Nose { spinning: true },
            ..idle
        },
        UpgradeKind::Engine | UpgradeKind::FuelTank => RigPose {
            thrust: Thrust::Climb,
            ..idle
        },
        UpgradeKind::DrillMount => RigPose {
            drill: DrillPose::Up,
            gimbal_live: true,
            ..idle
        },
        _ => RigPose {
            beacon: (!(ticks / 40).is_multiple_of(2)).then_some(GOLD),
            ..idle
        },
    }
}

/// The largest whole scale (2 or 3) at which `canvas` fits inside `well` with a margin.
fn fit_scale(canvas: &RigCanvas, well: Rectangle) -> i32 {
    let Some((min_x, min_y, max_x, max_y)) = canvas.bounds() else {
        return 2;
    };
    let (w, h) = (max_x - min_x + 1, max_y - min_y + 1);
    if w * 3 <= well.width as i32 - 8 && h * 3 <= well.height as i32 - 14 {
        3
    } else {
        2
    }
}

/// Where `canvas` drawn at `scale` sits centred in `well` (its body box's top-left).
fn centred_origin(canvas: &RigCanvas, well: Rectangle, scale: i32) -> Option<Vector2> {
    let (min_x, min_y, max_x, max_y) = canvas.bounds()?;
    let centre_x = well.x as i32 + well.width as i32 / 2;
    // A little below centre: the corner labels sit at the top.
    let centre_y = well.y as i32 + well.height as i32 / 2 + 3;
    Some(Vector2::new(
        (centre_x - (min_x + max_x + 1) * scale / 2) as f32,
        (centre_y - (min_y + max_y + 1) * scale / 2) as f32,
    ))
}

/// Draws `canvas` centred in `well` at `scale`.
fn draw_centred(draw: &mut impl RaylibDraw, canvas: &RigCanvas, well: Rectangle, scale: i32) {
    if let Some(origin) = centred_origin(canvas, well, scale) {
        canvas.draw(draw, origin, scale as f32, false);
    }
}

/// Draws every painted pixel of `canvas` in `color` (the transform flash), placed like
/// [`RigCanvas::draw`].
pub(super) fn draw_silhouette(
    draw: &mut impl RaylibDraw,
    canvas: &RigCanvas,
    origin: Vector2,
    scale: f32,
    flip: bool,
    color: Color,
) {
    let Some((min_x, min_y, max_x, max_y)) = canvas.bounds() else {
        return;
    };
    let mirror = |x: i32| {
        if flip { rig_art::BODY_WIDTH - 1 - x } else { x }
    };
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if canvas.get(x, y).is_some() {
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        mirror(x) as f32 * scale + origin.x,
                        y as f32 * scale + origin.y,
                        scale,
                        scale,
                    ),
                    color,
                );
            }
        }
    }
}

/// Smooth 0..1 ease.
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One before/after bar row: label, the bar (fitted part, then the gain in lime, animated while
/// `grow` < 1) and "now -> next".
fn draw_stat_row(
    draw: &mut impl RaylibDraw,
    bar: &StatBar,
    (x, y, width): (i32, i32, i32),
    grow: f32,
    seconds: f32,
) {
    let label_width = 76;
    ui_kit::text(
        draw,
        &ui_kit::truncate(bar.label, label_width - 2, TextStyle::Bold),
        x,
        y,
        TextStyle::Bold,
        MIST,
    );
    let values = if (bar.next - bar.now).abs() < f32::EPSILON {
        bar.format(bar.now)
    } else {
        // The unit once, after the new value.
        let number = |value: f32| format!("{value:.*}", usize::from(bar.decimals));
        format!("{} -> {}", number(bar.now), bar.format(bar.next))
    };
    let values_width = ui_kit::text_width(&values, TextStyle::Body);
    ui_kit::text_right(draw, &values, x + width, y, TextStyle::Body, BONE);
    let bar_x = x + label_width;
    let bar_width = (width - label_width - values_width - 6).max(16);
    let (now, next) = bar.fractions();
    let shown = now + (next - now) * grow;
    let inner = bar_width - 2;
    draw.draw_rectangle(bar_x, y, bar_width, 8, INK);
    draw.draw_rectangle(bar_x + 1, y + 1, inner, 6, NIGHT);
    let fitted = ((inner as f32) * now).round() as i32;
    let reach = ((inner as f32) * shown).round() as i32;
    if fitted > 0 {
        draw.draw_rectangle(bar_x + 1, y + 1, fitted, 6, STEEL);
        draw.draw_rectangle(bar_x + 1, y + 1, fitted, 1, STEEL_LIGHT);
    }
    if reach > fitted {
        let pulse = (seconds * 3.0).sin() * 0.5 + 0.5;
        let body = if pulse > 0.5 { LIME } else { LEAF };
        draw.draw_rectangle(bar_x + 1 + fitted, y + 1, reach - fitted, 6, body);
        draw.draw_rectangle(bar_x + 1 + fitted, y + 1, reach - fitted, 1, CREAM);
        draw.draw_rectangle(bar_x + reach, y, 1, 8, BONE);
    }
}

/// The showcase card beside the shop's counter panel: the rig with the selected line's next
/// tier (or the tier just installed, while the purchase moment plays), its before/after bars
/// and what it changes. Skipped when the view has no room beside the panel.
#[allow(clippy::too_many_lines, reason = "one card laid out top to bottom")]
pub(super) fn draw_shop_showcase(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(panel) = layout::peek_last_dialog_rect() else {
        return;
    };
    let canvas = ui_kit::canvas();
    let left = (panel.x + panel.width) as i32 + 8;
    let width = (canvas.width - 8 - left).min(300);
    if width < 210 || canvas.height < 220 {
        return;
    }
    let player = ui.player;
    let rules = ui.snapshot.logistics.rules;
    let offers = upgrade_offers_for(player, rules);
    let index = ui.seat.selected_menu_item.min(offers.len() - 1);
    let kind = offers[index].kind;
    // The line's top under the world's rules (the deep tiers on Deep Descent worlds).
    let top_level = offers[index].max_level;
    let now = ui.shell.update_ticks;
    let install = ui
        .seat
        .rewards
        .install_at(now)
        .filter(|moment| moment.upgrade == kind)
        .cloned();
    let sky = ui.snapshot.terrain.settings().generator.sky_bands();
    let preview: Option<UpgradePreview> = install.as_ref().map_or_else(
        || upgrade_preview_for(player, kind, sky.as_ref(), rules),
        |moment| {
            Some(preview_between(
                player,
                kind,
                moment.from_level,
                moment.to_level,
                sky.as_ref(),
            ))
        },
    );
    let seconds = ui_kit::seconds();
    let ticks = (seconds * 60.0) as u64;
    let inner = width - 16;
    let lines: Vec<String> = preview
        .as_ref()
        .map(|preview| {
            preview
                .changes
                .iter()
                .flat_map(|change| ui_kit::wrap(change, inner - 10, TextStyle::Body))
                .take(4)
                .collect()
        })
        .unwrap_or_default();
    let bar_count = preview
        .as_ref()
        .map_or(0, |preview| preview.bars.len().min(3)) as i32;
    let well_height = 70;
    let header = 18;
    let height = header
        + well_height
        + 6
        + bar_count * 12
        + 4
        + lines.len() as i32 * TextStyle::Body.line_height()
        + 8;
    let x = left;
    let y = 8;
    let rect = Rectangle::new(x as f32, y as f32, width as f32, height as f32);
    ui_kit::panel(draw, rect, PanelStyle::Accent);

    // Header: what is on show.
    let progress = install
        .as_ref()
        .map_or(1.0, |moment: &InstallMoment| moment.progress(now));
    let (title, title_color) = match (&install, &preview) {
        (Some(moment), _) => (
            if kind == UpgradeKind::BombPack {
                "PACKED!".to_owned()
            } else {
                format!("INSTALLED: TIER {}", moment.to_level)
            },
            if (ticks / 8).is_multiple_of(2) {
                LIME
            } else {
                CREAM
            },
        ),
        (None, Some(preview)) if kind != UpgradeKind::BombPack => {
            (format!("NEXT: TIER {}", preview.to_level), GOLD)
        }
        (None, Some(_)) => ("NEXT".to_owned(), GOLD),
        (None, None) => ("TOP OF THE LINE".to_owned(), LIME),
    };
    ui_kit::text(draw, &title, x + 8, y + 6, TextStyle::Bold, title_color);
    // A maxed line names its top tier (a classic world's maxed core line must not name the
    // first deep tier, which only Deep Descent worlds sell).
    let level = current_level(player, kind).min(top_level.saturating_sub(1));
    let part = install.as_ref().map_or_else(
        || upgrade_tier_name(kind, level),
        |moment| upgrade_tier_name(kind, moment.to_level.saturating_sub(1)),
    );
    let part = ui_kit::truncate(
        part,
        width - 24 - ui_kit::text_width(&title, TextStyle::Bold),
        TextStyle::Body,
    );
    ui_kit::text_right(draw, &part, x + width - 8, y + 6, TextStyle::Body, BONE);

    // The rig now and with the part.
    let base = RigLook::from_player(ui.view.player_id, player);
    let well_y = y + header;
    let pose = showcase_pose(kind, ticks);
    let (from, to) = preview.as_ref().map_or((level, level), |preview| {
        (preview.from_level, preview.to_level)
    });
    if visible_line(kind) && from != to {
        let half = (inner - 14) / 2;
        let now_well = Rectangle::new(
            (x + 8) as f32,
            well_y as f32,
            half as f32,
            well_height as f32,
        );
        let next_well = Rectangle::new(
            (x + 8 + half + 14) as f32,
            well_y as f32,
            half as f32,
            well_height as f32,
        );
        ui_kit::well(draw, now_well);
        ui_kit::well(draw, next_well);
        let before = RigLook {
            tiers: tiers_with(base.tiers, kind, from),
            ..base
        };
        let after = RigLook {
            tiers: tiers_with(base.tiers, kind, to),
            ..base
        };
        let (painted_before, painted_after) = (
            rig_art::paint_rig(&before, &pose),
            rig_art::paint_rig(&after, &pose),
        );
        // One scale for both, so the bigger part reads as bigger.
        let scale = fit_scale(&painted_before, now_well).min(fit_scale(&painted_after, next_well));
        draw_centred(draw, &painted_before, now_well, scale);
        if install.is_some() {
            draw_install_glow(draw, next_well, progress, ticks);
        }
        draw_centred(draw, &painted_after, next_well, scale);
        if install.is_some()
            && progress < 0.3
            && let Some(origin) = centred_origin(&painted_after, next_well, scale)
        {
            // The flash of the part going on.
            let alpha = ((0.3 - progress) / 0.3 * 230.0) as u8;
            draw_silhouette(
                draw,
                &painted_after,
                origin,
                scale as f32,
                false,
                palette::with_alpha(BONE, alpha),
            );
        }
        ui_kit::text(draw, "NOW", x + 11, well_y + 3, TextStyle::Body, FOG);
        let next_label = if install.is_some() { "FITTED" } else { "NEXT" };
        ui_kit::text(
            draw,
            next_label,
            next_well.x as i32 + 3,
            well_y + 3,
            TextStyle::Body,
            if install.is_some() { LIME } else { GOLD },
        );
        // The arrow between the wells.
        let arrow_x = x + 8 + half + 4;
        let arrow_y = well_y + well_height / 2 - 3;
        let nudge = i32::from((ticks / 15).is_multiple_of(2));
        for row in 0..7_i32 {
            let span = 3 - (row - 3).abs();
            draw.draw_rectangle(arrow_x + nudge, arrow_y + row, span + 1, 1, GOLD);
        }
    } else {
        let well = Rectangle::new(
            (x + 8) as f32,
            well_y as f32,
            inner as f32,
            well_height as f32,
        );
        ui_kit::well(draw, well);
        if install.is_some() {
            draw_install_glow(draw, well, progress, ticks);
        }
        let painted = rig_art::paint_rig(&base, &pose);
        draw_centred(draw, &painted, well, fit_scale(&painted, well));
        // The part's own icon in the corner (tanks, scanners and counters do not show on the
        // rig).
        draw_sprite(
            draw,
            super::counters::upgrade_icon(kind),
            0,
            Vector2::new((x + inner - 14) as f32, (well_y + 4) as f32),
            2.0,
            Color::WHITE,
        );
    }

    // Before/after bars.
    let mut row_y = well_y + well_height + 6;
    if let Some(preview) = &preview {
        let grow = if install.is_some() {
            ease((progress - 0.15) / 0.55)
        } else {
            1.0
        };
        for bar in preview.bars.iter().take(3) {
            draw_stat_row(draw, bar, (x + 8, row_y, inner), grow, seconds);
            row_y += 12;
        }
    }
    // What it changes.
    row_y += 4;
    for (index, line) in lines.iter().enumerate() {
        let first_of_change = preview.as_ref().is_some_and(|preview| {
            preview
                .changes
                .iter()
                .any(|change| change.starts_with(line.as_str()))
        });
        if first_of_change {
            ui_kit::text(draw, "+", x + 8, row_y, TextStyle::Bold, LIME);
        }
        ui_kit::text(
            draw,
            line,
            x + 18,
            row_y,
            TextStyle::Body,
            if index == 0 { CREAM } else { MIST },
        );
        row_y += TextStyle::Body.line_height();
    }
    if install.is_some() {
        draw_sparkles(draw, rect, ticks, LIME);
    }
}

/// A burst of light behind a freshly fitted rig: rays turning slowly, fading out.
fn draw_install_glow(draw: &mut impl RaylibDraw, well: Rectangle, progress: f32, ticks: u64) {
    let fade = (1.0 - progress).clamp(0.0, 1.0);
    let centre_x = well.x + well.width / 2.0;
    let centre_y = well.y + well.height / 2.0;
    let radius = well.height * (0.35 + 0.25 * ease(progress * 2.0));
    for ray in 0..10 {
        let angle = ray as f32 * std::f32::consts::TAU / 10.0 + ticks as f32 * 0.02;
        let end = Vector2::new(
            centre_x + angle.cos() * radius,
            centre_y + angle.sin() * radius,
        );
        draw.draw_line_ex(
            Vector2::new(centre_x, centre_y),
            end,
            3.0,
            palette::with_alpha(if ray % 2 == 0 { GOLD } else { CREAM }, (90.0 * fade) as u8),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The purchase moment in the shop room
// ---------------------------------------------------------------------------------------------

/// How the room's rig looks during the purchase moment: the look to paint, a hop (room pixels
/// up) and whether this frame flashes white.
#[must_use]
pub(super) fn install_look(look: RigLook, seat: &SeatUi) -> (RigLook, i32, bool) {
    let now = seat.rewards.clock;
    let Some(moment) = seat.rewards.install_at(now) else {
        return (look, 0, false);
    };
    let t = moment.progress(now) * INSTALL_TICKS as f32 / 60.0;
    let fitted = RigLook {
        tiers: tiers_with(look.tiers, moment.upgrade, moment.to_level),
        ..look
    };
    let old = RigLook {
        tiers: tiers_with(look.tiers, moment.upgrade, moment.from_level),
        ..look
    };
    if t < 0.3 {
        return (old, 0, false);
    }
    if t < 0.55 {
        // Flash between the old part, white and the new part.
        let frame = ((t - 0.3) / 0.05) as i32;
        return match frame % 3 {
            0 => (old, 0, true),
            1 => (fitted, 0, false),
            _ => (fitted, 0, true),
        };
    }
    let hop = if t < 0.8 {
        (3.0 * ((t - 0.55) / 0.25 * std::f32::consts::PI).sin()).round() as i32
    } else {
        0
    };
    (fitted, hop, false)
}

/// Camera punch for the shop room during the purchase moment (room pixels down).
#[must_use]
pub(super) fn install_punch(seat: &SeatUi) -> f32 {
    let now = seat.rewards.clock;
    let Some(moment) = seat.rewards.install_at(now) else {
        return 0.0;
    };
    let t = moment.progress(now) * INSTALL_TICKS as f32 / 60.0;
    if !(0.5..1.1).contains(&t) {
        return 0.0;
    }
    let local = (t - 0.5) / 0.6;
    (2.5 * (1.0 - local) * (local * std::f32::consts::TAU * 2.0).sin()).round()
}

/// The purchase moment's room effects around the rig whose centre is `(rig_x, centre_y)`: a
/// beam of light, rising motes before the part goes on, then a ring of sparks and stars.
pub(super) fn draw_install_fx(
    draw: &mut impl RaylibDraw,
    seat: &SeatUi,
    rig_x: i32,
    centre_y: i32,
    floor_y: i32,
) {
    let now = seat.rewards.clock;
    let Some(moment) = seat.rewards.install_at(now) else {
        return;
    };
    let t = moment.progress(now) * INSTALL_TICKS as f32 / 60.0;
    let step = moment.age(now) as i32;
    // The beam.
    let beam = if t < 0.55 {
        ease(t / 0.55)
    } else {
        (1.0 - (t - 0.55) / 1.95).clamp(0.0, 1.0)
    };
    if beam > 0.0 {
        let alpha = (70.0 * beam) as u8;
        draw.draw_rectangle(
            rig_x - 9,
            0,
            18,
            floor_y,
            palette::with_alpha(CREAM, alpha / 2),
        );
        draw.draw_rectangle(rig_x - 5, 0, 10, floor_y, palette::with_alpha(BONE, alpha));
    }
    // Motes rising into the part before it goes on.
    if t < 0.55 {
        for mote in 0..10 {
            let dx = (mote * 7 + 3) % 22 - 11;
            let rise = (step * 2 + mote * 9) % 26;
            let color = if mote % 2 == 0 { GOLD } else { CREAM };
            let size = 1 + mote % 2;
            draw.draw_rectangle(rig_x + dx, floor_y - 2 - rise, size, size, color);
        }
    }
    // The ring of sparks and stars when it does.
    if (0.45..1.6).contains(&t) {
        let burst = (t - 0.45) / 1.15;
        let radius = 4.0 + 30.0 * ease(burst);
        for spark in 0..16 {
            let angle = spark as f32 * std::f32::consts::TAU / 16.0;
            let reach = radius * if spark % 2 == 0 { 1.0 } else { 0.7 };
            let sx = rig_x + (angle.cos() * reach).round() as i32;
            let sy = centre_y + (angle.sin() * reach * 0.8).round() as i32;
            if sy > floor_y {
                continue;
            }
            let color = match spark % 4 {
                0 => BONE,
                1 => GOLD,
                2 => LIME,
                _ => CREAM,
            };
            if burst < 0.7 || (step / 3 + spark) % 2 == 0 {
                draw.draw_rectangle(sx, sy, 2, 2, color);
                if spark % 2 == 0 && burst < 0.75 {
                    // A four-point star.
                    draw.draw_rectangle(sx - 2, sy, 6, 2, color);
                    draw.draw_rectangle(sx, sy - 2, 2, 6, color);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The progression screen
// ---------------------------------------------------------------------------------------------

/// A milestone group's row icon.
const fn group_icon(group: MilestoneGroup) -> &'static SpriteDef {
    match group {
        MilestoneGroup::Upgrades => &menu_icons::GEAR,
        MilestoneGroup::Depths => &icons::DEPTH,
        MilestoneGroup::Discoveries => &icons::SCANNER,
        MilestoneGroup::Sky => &icons::ALTITUDE,
    }
}

/// Every cosmetic a milestone can unlock, and how many of them `player` has.
fn reward_counts(player: &Player) -> (usize, usize) {
    let all: Vec<CosmeticItem> = Milestone::ALL
        .iter()
        .flat_map(|milestone| milestone.rewards().iter().copied())
        .collect();
    let owned = all
        .iter()
        .filter(|item| player.cosmetic_unlocks.has(**item))
        .count();
    (owned, all.len())
}

/// The seat's progression screen: milestones by group with their medals and rewards, the
/// cosmetics unlocked and the player's records, plus a trophy shelf card beside it when the view
/// has room. Works the same in every seat (selection is the seat's own).
#[allow(
    clippy::too_many_lines,
    reason = "one screen: milestone groups, records, shelf"
)]
pub(super) fn draw_achievements(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let player = ui.player;
    let log = &player.milestones;
    let selected = ui.seat.selected_menu_item;
    let test_mode = ui.snapshot.test_mode.is_some();
    let mut sections = vec![Section::new(
        "Trophy Shelf",
        GOLD,
        [Medal::Platinum, Medal::Gold, Medal::Silver, Medal::Bronze]
            .into_iter()
            .map(|medal| {
                let of_kind = || Milestone::ALL.iter().filter(move |m| m.medal() == medal);
                let earned = of_kind().filter(|m| log.has(**m)).count();
                SectionItem::icon_stat(
                    medal_icon(Some(medal)),
                    format!("{} medals", medal.label()),
                    format!("{earned}/{}", of_kind().count()),
                    medal_color(medal),
                )
            })
            .collect(),
    )];
    let mut about = None;
    for group in MilestoneGroup::ALL {
        let mut items = Vec::new();
        for (index, milestone) in Milestone::ALL.into_iter().enumerate() {
            if milestone.group() != group {
                continue;
            }
            let earned = log.has(milestone);
            let medal = milestone.medal();
            if index == selected {
                about = Some(if earned {
                    format!(
                        "{} medal. {} Unlocked: {} (at the paint bay).",
                        medal.label(),
                        milestone.description(),
                        reward_line(milestone)
                    )
                } else {
                    format!(
                        "{} {} medal; unlocks the {}.",
                        milestone.description(),
                        medal.label(),
                        reward_line(milestone)
                    )
                });
            }
            items.push(SectionItem::item(ItemEntry {
                icon: Some(if earned {
                    medal_icon(Some(medal))
                } else {
                    group_icon(group)
                }),
                label: milestone.title().to_owned(),
                detail: if earned {
                    format!("Unlocked: {}", reward_line(milestone))
                } else {
                    milestone.description().to_owned()
                },
                value: if earned {
                    medal.label().to_uppercase()
                } else {
                    "LOCKED".to_owned()
                },
                value_color: if earned { medal_color(medal) } else { FOG },
                coin: false,
                selected: index == selected,
                disabled: !earned,
            }));
        }
        sections.push(Section::new(group.label(), group_color(group), items));
    }
    let records = log.records;
    let (owned, total) = reward_counts(player);
    sections.push(Section::new(
        "Records",
        SKY_LIGHT,
        vec![
            SectionItem::icon_stat(
                &icons::DEPTH,
                "Deepest dive",
                format!("{} m", records.deepest_row.max(0)),
                BONE,
            ),
            SectionItem::icon_stat(
                &icons::ALTITUDE,
                "Highest flight",
                format!("{} m above ground", records.highest_altitude.max(0)),
                BONE,
            ),
            SectionItem::stat(
                "Enemies defeated",
                records.enemies_defeated.to_string(),
                BONE,
            ),
            SectionItem::stat("Artifacts found", records.artifacts_found.to_string(), BONE),
            SectionItem::stat("Upgrades bought", records.upgrades_bought.to_string(), BONE),
            SectionItem::stat("Rewards unlocked", format!("{owned}/{total}"), LIME),
        ],
    ));
    if test_mode {
        about = Some("Test mode: milestones and records do not count in this world.".to_owned());
    }
    UiLayout::screen(draw).modal_with_render_plan(
        "Milestones",
        &format!(
            "{}/{} earned; each unlocks a look for your rig.",
            log.count(),
            Milestone::ALL.len()
        ),
        &ModalContent::new(sections).footer(about).prompts(&[
            hint(Prompt::Navigate, "select"),
            hint(Prompt::Confirm, "map"),
            hint(Prompt::Back, "close"),
        ]),
    );
    draw_trophy_shelf(draw, ui);
}

const fn group_color(group: MilestoneGroup) -> Color {
    match group {
        MilestoneGroup::Upgrades => GOLD,
        MilestoneGroup::Depths => ORANGE,
        MilestoneGroup::Discoveries => MINT,
        MilestoneGroup::Sky => SKY_LIGHT,
    }
}

/// The trophy shelf beside the progression screen: every medal on wooden shelves (earned ones
/// shine, the rest wait as dark outlines), the medal tally and the rig in its current look.
fn draw_trophy_shelf(draw: &mut RaylibDrawHandle<'_>, ui: &UiFrame<'_, '_>) {
    let Some(panel) = layout::peek_last_dialog_rect() else {
        return;
    };
    let canvas = ui_kit::canvas();
    let left = (panel.x + panel.width) as i32 + 8;
    let width = (canvas.width - 8 - left).min(260);
    let per_row = 5;
    let rows = Milestone::ALL.len().div_ceil(per_row) as i32;
    let shelf_height = 26;
    let height = 22 + rows * shelf_height + 14 + 74;
    if width < 150 || height + 16 > canvas.height {
        return;
    }
    let (x, y) = (left, 8);
    let rect = Rectangle::new(x as f32, y as f32, width as f32, height as f32);
    ui_kit::panel(draw, rect, PanelStyle::Accent);
    ui_kit::text(draw, "Trophy Shelf", x + 8, y + 6, TextStyle::Bold, GOLD);
    let log = &ui.player.milestones;
    let tally = format!("{}/{}", log.count(), Milestone::ALL.len());
    ui_kit::text_right(draw, &tally, x + width - 8, y + 6, TextStyle::Body, BONE);
    let ticks = (ui_kit::seconds() * 60.0) as u64;
    let spacing = (width - 16) / per_row as i32;
    for (index, milestone) in Milestone::ALL.into_iter().enumerate() {
        let column = (index % per_row) as i32;
        let row = (index / per_row) as i32;
        let shelf_y = y + 22 + row * shelf_height;
        if column == 0 {
            // The shelf plank.
            draw.draw_rectangle(x + 6, shelf_y + 21, width - 12, 3, LOAM);
            draw.draw_rectangle(x + 6, shelf_y + 21, width - 12, 1, CLAY);
            draw.draw_rectangle(x + 6, shelf_y + 24, width - 12, 1, SOIL_DARK);
        }
        let earned = log.has(milestone);
        let mx = x + 8 + column * spacing + (spacing - 18) / 2;
        let selected = ui.seat.selected_menu_item == index;
        let raise = i32::from(selected && (ticks / 20).is_multiple_of(2));
        draw_medal(
            draw,
            earned.then_some(milestone.medal()),
            mx,
            shelf_y + 3 - raise,
            2,
        );
        if selected {
            ui_kit::cursor(draw, mx - 6, shelf_y + 8);
        }
        if earned && (ticks / 7 + index as u64).is_multiple_of(23) {
            draw.draw_rectangle(mx + 4, shelf_y + 9, 2, 2, BONE);
        }
    }
    // Medal tally by kind.
    let tally_y = y + 22 + rows * shelf_height + 2;
    let mut tx = x + 8;
    for medal in [Medal::Bronze, Medal::Silver, Medal::Gold, Medal::Platinum] {
        let count = Milestone::ALL
            .iter()
            .filter(|milestone| milestone.medal() == medal && log.has(**milestone))
            .count();
        ui_kit::icon(draw, medal_icon(Some(medal)), tx, tally_y);
        let label = count.to_string();
        ui_kit::text(
            draw,
            &label,
            tx + 11,
            tally_y + 1,
            TextStyle::Body,
            medal_color(medal),
        );
        tx += 14 + ui_kit::text_width(&label, TextStyle::Body) + 8;
    }
    // The rig wearing its look.
    let well = Rectangle::new(
        (x + 8) as f32,
        (tally_y + 14) as f32,
        (width - 16) as f32,
        68.0,
    );
    ui_kit::well(draw, well);
    let look = RigLook::from_player(ui.view.player_id, ui.player);
    let pose = RigPose {
        beacon: (!(ticks / 40).is_multiple_of(2)).then_some(GOLD),
        ..RigPose::idle(ticks)
    };
    draw_centred(draw, &rig_art::paint_rig(&look, &pose), well, 2);
}
