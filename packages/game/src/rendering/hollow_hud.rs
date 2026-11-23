//! The Hollow World's HUD (Wave 9, Descent V): the breakthrough's card, and a panel for the
//! inner sun, the Sunforge's light and shade, the Sun Serpent's fight, the Gravity Anchor and
//! a ride.
//!
//! * **The breakthrough** ([`draw_breakthrough`]): for a few seconds after a seat's rig breaks
//!   through the rock sky, a great card names the place ("THE PLANET IS HOLLOW") over the view
//!   while the camera eases back in from wide and the light floods the screen and fades
//!   (`render_snapshot`: [`HollowView::reveal_zoom`] and [`HollowView::flood`]).
//! * **The panel** ([`draw_hollow_hud`]): what the seat needs to read here, most urgent first.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "HUD layout maps small values to pixels; the panel's lines read top to bottom"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    depth_progression::RigSystem,
    descent::Stratum,
    hollow::{EXPOSURE_SCORCH_AT, INNER_DAY_MILLIS, InnerPhase, SerpentPhase, inner_sun_at},
    terrain::TilePosition,
};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::ViewSnapshot;

/// Seconds the breakthrough card shows.
pub const REVEAL_SECONDS: f32 = 7.0;
/// How wide the camera starts at the breakthrough (a fraction of its usual scale).
pub const REVEAL_WIDE: f32 = 0.55;
/// How wide the camera pulls back inside the Sun Well (a fraction of its usual scale), so the
/// inner sun, the pillars and the serpent's orbit fit the view together.
pub const ARENA_WIDE: f32 = 0.7;
/// Tiles past the Sun Well's wall over which the arena's wide view eases in.
pub const ARENA_EASE_TILES: f32 = 12.0;

/// The Gravity Anchor's state for the HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnchorView {
    Ready,
    Holding { seconds: f32 },
    Recharging { seconds: f32 },
}

/// What a seat reads of the Hollow World (computed with the world in the view snapshot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HollowView {
    pub stratum: Stratum,
    /// The rig's exposure, `0..=1` of the scorch line (more past it).
    pub exposure: f32,
    /// In the inner sun's light (the Sunforge).
    pub sunlit: bool,
    pub phase: InnerPhase,
    /// Seconds until the inner sun's phase turns.
    pub phase_seconds: f32,
    /// Seconds since this seat broke through (`None`: long ago or never).
    pub since_reveal: Option<f32>,
    pub anchor: Option<AnchorView>,
    pub riding: bool,
    /// In the Sun Well: the serpent's scales left, its phase, a flare coming or burning.
    pub serpent: Option<(usize, SerpentPhase, bool, bool)>,
    /// How far inside the Sun Well's wide view the seat is, `0..=1` (1 inside the well, easing
    /// to 0 over [`ARENA_EASE_TILES`] outside it).
    pub arena: f32,
}

impl HollowView {
    /// What `player_id` reads at `tile` (`None` outside the Hollow World and its fight).
    #[must_use]
    pub fn of(world: &WorldState, player_id: PlayerId, tile: TilePosition) -> Option<Self> {
        let stratum = world.hollow_stratum_at(tile)?;
        let hollow = world.hollow();
        let hz = f32::from(u16::try_from(world.tick_rate_hz()).unwrap_or(60).max(1));
        let now = world.simulation_tick().get();
        let millis = world.clock().world_millis();
        let sun = inner_sun_at(millis);
        let into = millis % INNER_DAY_MILLIS;
        let ends = [
            fatherload_simulation::hollow::INNER_DAWN_MILLIS,
            fatherload_simulation::hollow::INNER_DAWN_MILLIS
                + fatherload_simulation::hollow::INNER_NOON_MILLIS,
            fatherload_simulation::hollow::INNER_DAWN_MILLIS
                + fatherload_simulation::hollow::INNER_NOON_MILLIS
                + fatherload_simulation::hollow::INNER_DUSK_MILLIS,
            INNER_DAY_MILLIS,
        ];
        let next = ends
            .iter()
            .copied()
            .find(|end| *end > into)
            .unwrap_or(INNER_DAY_MILLIS);
        let since_reveal = hollow
            .revealed
            .get(&player_id)
            .filter(|at| at.get() > 0)
            .map(|at| now.saturating_sub(at.get()) as f32 / hz)
            .filter(|seconds| *seconds < REVEAL_SECONDS + 1.0);
        let fitted = world
            .player(player_id)
            .is_some_and(|rig| rig.hazard_gear.has_system(RigSystem::GravityAnchor));
        let anchor = fitted.then(|| {
            let state = hollow.anchors.get(&player_id).copied().unwrap_or_default();
            match state.until {
                Some(until) if until.get() > now => AnchorView::Holding {
                    seconds: (until.get() - now) as f32 / hz,
                },
                _ if state.ready.get() > now => AnchorView::Recharging {
                    seconds: (state.ready.get() - now) as f32 / hz,
                },
                _ => AnchorView::Ready,
            }
        });
        let arena = world.sun_well().map_or(0.0, |centre| {
            let distance = ((tile.x - centre.x) as f32).hypot((tile.y - centre.y) as f32);
            let outside = distance - fatherload_simulation::hollow::WELL_RADIUS as f32;
            let t = (1.0 - outside / ARENA_EASE_TILES).clamp(0.0, 1.0);
            t * t * 2.0f32.mul_add(-t, 3.0)
        });
        let serpent = world.sun_well().and_then(|centre| {
            let close = (tile.x - centre.x).pow(2) + (tile.y - centre.y).pow(2)
                <= (fatherload_simulation::hollow::WELL_RADIUS + 2).pow(2);
            let fight = &hollow.serpent;
            (close && fight.awake()).then(|| {
                (
                    fight.scales.iter().filter(|whole| **whole).count(),
                    fight.phase,
                    fight.flare_warned.is_some() && fight.flare_burns.is_none(),
                    fight.flaring(),
                )
            })
        });
        Some(Self {
            stratum,
            exposure: f32::from(hollow.exposure_of(player_id)) / f32::from(EXPOSURE_SCORCH_AT),
            sunlit: world.sunlit_at(tile),
            phase: sun.phase,
            phase_seconds: (next - into) as f32 / 1_000.0,
            since_reveal,
            anchor,
            riding: world.mount_of(player_id).is_some(),
            serpent,
            arena,
        })
    }

    /// The camera's scale at the breakthrough: wide, easing back in over the card.
    #[must_use]
    pub fn reveal_zoom(&self) -> f32 {
        self.since_reveal.map_or(1.0, |seconds| {
            let t = (seconds / (REVEAL_SECONDS - 1.0)).clamp(0.0, 1.0);
            let eased = t * t * 2.0f32.mul_add(-t, 3.0);
            (1.0 - REVEAL_WIDE).mul_add(eased, REVEAL_WIDE)
        })
    }

    /// The camera's scale in and near the Sun Well: wide enough to read the serpent's orbit and
    /// the sun's flares (it eases in with distance, so it never jumps).
    #[must_use]
    pub fn arena_zoom(&self) -> f32 {
        (ARENA_WIDE - 1.0).mul_add(self.arena, 1.0)
    }

    /// The camera's scale for this seat: the breakthrough's or the arena's, whichever is wider.
    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.reveal_zoom().min(self.arena_zoom())
    }

    /// How much light floods the screen at the breakthrough (`0..=1`, one smooth fade).
    #[must_use]
    pub fn flood(&self) -> f32 {
        self.since_reveal.map_or(0.0, |seconds| {
            (1.0 - seconds / 2.8).clamp(0.0, 1.0).powi(2) * 0.85
        })
    }
}

/// The breakthrough's card (see the module docs).
pub(super) fn draw_breakthrough(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    let Some(seconds) = view.hollow.as_ref().and_then(|hollow| hollow.since_reveal) else {
        return;
    };
    if seconds > REVEAL_SECONDS {
        return;
    }
    let fade_in = (seconds / 0.6).clamp(0.0, 1.0);
    let fade_out = ((REVEAL_SECONDS - seconds) / 0.8).clamp(0.0, 1.0);
    let alpha = (fade_in * fade_out * 255.0) as u8;
    let width = (rect.width as i32 - 40).min(360);
    let height = 66;
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let y = rect.y as i32 + (rect.height as i32) / 2 + 36;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, height as f32),
        PanelStyle::Accent,
    );
    let centre = x + width / 2;
    // A little sun over the title.
    draw.draw_circle(centre, y - 4, 9.0, with_alpha(GOLD, alpha));
    draw.draw_circle(centre, y - 4, 6.0, with_alpha(CREAM, alpha));
    ui_kit::text_centred(
        draw,
        "DESCENT V: THE HOLLOW WORLD",
        centre,
        y + 8,
        TextStyle::Body,
        with_alpha(GOLD, alpha),
    );
    ui_kit::text_centred(
        draw,
        "The planet is hollow",
        centre,
        y + 21,
        TextStyle::Title,
        with_alpha(CREAM, alpha),
    );
    ui_kit::text_centred(
        draw,
        "An inner sun burns below. The Fathers came this way.",
        centre,
        y + 46,
        TextStyle::Body,
        with_alpha(SKY_LIGHT, alpha),
    );
}

struct Line {
    text: String,
    color: Color,
}

/// The Hollow World's panel (see the module docs), below `top` on the view's right.
pub(super) fn draw_hollow_hud(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
    anchor_key: &str,
) {
    let Some(hollow) = view.hollow.as_ref() else {
        return;
    };
    if hollow
        .since_reveal
        .is_some_and(|seconds| seconds < REVEAL_SECONDS)
    {
        return;
    }
    let mut title: Option<(&str, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    let mut bar: Option<(f32, Color)> = None;
    if let Some((scales, phase, warning, burning)) = hollow.serpent {
        title = Some(("THE SUN SERPENT", GOLD));
        if burning {
            lines.push(Line {
                text: "THE SUN FLARES: stay in a pillar's shadow".to_owned(),
                color: ORANGE,
            });
        } else if warning {
            lines.push(Line {
                text: "The sun swells: get behind a pillar!".to_owned(),
                color: GOLD,
            });
        }
        lines.push(match phase {
            SerpentPhase::Exposed => Line {
                text: "FALLEN: strike its head!".to_owned(),
                color: CREAM,
            },
            SerpentPhase::Tell => Line {
                text: "Its head glows: it lunges!".to_owned(),
                color: ORANGE,
            },
            _ => Line {
                text: format!("Sun scales {scales}/4: shoot the glowing coils"),
                color: AMBER,
            },
        });
    }
    if hollow.stratum == Stratum::TheSunforge || hollow.exposure > 0.0 {
        title.get_or_insert(("SUNLIGHT", GOLD));
        let scorching = hollow.exposure >= 1.0;
        let color = if scorching {
            RED
        } else if hollow.exposure > 0.6 {
            ORANGE
        } else {
            GOLD
        };
        bar = Some((hollow.exposure, color));
        lines.push(Line {
            text: if scorching {
                "SCORCHING: get into shade".to_owned()
            } else if hollow.sunlit {
                "In the light: heating".to_owned()
            } else {
                "In shade: cooling".to_owned()
            },
            color,
        });
    }
    if hollow.riding {
        title.get_or_insert(("RIDING", LEAF));
        lines.push(Line {
            text: "On a crestback (use: get down)".to_owned(),
            color: LEAF,
        });
    }
    if let Some(anchor) = hollow.anchor {
        let (text, color) = match anchor {
            AnchorView::Ready => (format!("Gravity Anchor ready [{anchor_key}]"), MINT),
            AnchorView::Holding { seconds } => (format!("ANCHORED {seconds:.0} s"), SKY_LIGHT),
            AnchorView::Recharging { seconds } => {
                (format!("Anchor recharging {seconds:.0} s"), FOG)
            }
        };
        lines.push(Line { text, color });
    }
    // The inner sun's day, everywhere in the Hollow World.
    let minutes = (hollow.phase_seconds / 60.0).floor() as u32;
    let seconds = (hollow.phase_seconds % 60.0).floor() as u32;
    let (next, color) = match hollow.phase {
        InnerPhase::Dawn => ("full day", CREAM),
        InnerPhase::Day => ("dusk", GOLD),
        InnerPhase::Dusk => ("night", ORANGE),
        InnerPhase::Night => ("dawn", LAVENDER),
    };
    lines.push(Line {
        text: format!(
            "{}: {next} in {minutes}:{seconds:02}",
            capitalised(hollow.phase.name())
        ),
        color,
    });
    let (heading, heading_color) = title.unwrap_or(("INNER SUN", GOLD));
    let width = 210;
    let bar_height = if bar.is_some() { 7 } else { 0 };
    let height = 18 + bar_height + lines.len() as i32 * 11;
    let x = (rect.x + rect.width) as i32 - 16 - width;
    if x <= rect.x as i32 + 120 {
        return;
    }
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, top as f32, width as f32, height as f32),
        PanelStyle::Standard,
    );
    let inner = x + ui_kit::PANEL_PADDING;
    ui_kit::text(
        draw,
        heading,
        inner,
        top + 5,
        TextStyle::Bold,
        heading_color,
    );
    let mut y = top + 17;
    if let Some((fill, color)) = bar {
        let bar_width = width - ui_kit::PANEL_PADDING * 2;
        draw.draw_rectangle(inner, y, bar_width, 5, INK);
        draw.draw_rectangle(
            inner + 1,
            y + 1,
            ((bar_width - 2) as f32 * fill.clamp(0.0, 1.0)) as i32,
            3,
            color,
        );
        y += bar_height;
    }
    for line in lines {
        let text = ui_kit::truncate(&line.text, width - 10, TextStyle::Body);
        ui_kit::text(draw, &text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
