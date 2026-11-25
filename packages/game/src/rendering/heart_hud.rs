//! Descent VIII's HUD (Wave 10, the Heart): a panel for the rig's weight with dark matter
//! aboard, a gravity well's pull, a magnetic rail ride, the Heart's fight, and over the Moon
//! after the ending, the open stars.
//!
//! * **The view** ([`HeartView::of`]): what the seat needs to read here, computed with the
//!   world in the view snapshot. Its [`HeartView::zoom`] pulls the camera back in and near the
//!   Heart's chamber (the whole fight in sight) and during the ending's awakening.
//! * **The panel** ([`draw_heart_hud`]): most urgent first.
//! * **The stars** ([`draw_stars_open`]): a card over the view the first minutes a rig flies
//!   above the Moon after the ending.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "HUD layout maps small values to pixels; the panel's lines read top to bottom"
)]

use fatherload_simulation::{
    PlayerId, WorldState,
    descent::Stratum,
    heart::{
        BreathState, CHAMBER_RADIUS, COIL_COUNT, HeartPhase, PulseState, WEIGHTLESS_LUMPS,
        beam_angle, beam_strength, breath_at, ending::EndingView, pulse_at,
    },
    item::ItemKind,
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

/// How wide the camera pulls back in the Heart's chamber (a fraction of its usual scale).
pub const ARENA_WIDE: f32 = 0.62;
/// Tiles past the chamber's shell over which the wide view eases in.
pub const ARENA_EASE_TILES: f32 = 14.0;
/// How wide the camera pulls back while the Heart wakes (the ending's first moments).
pub const AWAKENING_WIDE: f32 = 0.5;

/// The Heart's fight as the HUD reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightView {
    pub phase: HeartPhase,
    pub coils_left: usize,
    /// The core's health, `0..=1`.
    pub health: f32,
    pub pulse: PulseState,
    pub breath: BreathState,
    /// The ignition's beam angle (radians) and strength (`0..=1`).
    pub beams: Option<(f32, f32)>,
}

/// What a seat reads of Descent VIII (`None` elsewhere, except over the Moon after the ending).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeartView {
    pub stratum: Option<Stratum>,
    /// The rig's weight, per mille of its own (1000 with no dark matter).
    pub mass: i32,
    pub dark: u32,
    /// How hard a gravity well pulls the rig, `0..=1`.
    pub well: f32,
    /// On a magnetic rail, at this many tiles a second.
    pub mag_ride: Option<f32>,
    pub fight: Option<FightView>,
    /// How far inside the chamber's wide view the seat is, `0..=1`.
    pub arena: f32,
    /// Milliseconds since the cable lit (the ending's clock), while it is recent.
    pub ending_elapsed: Option<u64>,
    /// Above the Moon with space open and the Star Drive fitted: the stars are open.
    pub stars_open: bool,
}

impl HeartView {
    /// What `player_id` reads at `tile`.
    #[must_use]
    pub fn of(world: &WorldState, player_id: PlayerId, tile: TilePosition) -> Option<Self> {
        if !world.world_settings().generator.has_descents() {
            return None;
        }
        let stratum = world
            .deep_stratum_at(tile)
            .filter(|stratum| fatherload_simulation::heart::is_heart(*stratum));
        let ending_elapsed = world
            .ending_elapsed()
            .filter(|elapsed| *elapsed < fatherload_simulation::heart::ENDING_SHIELD_MILLIS);
        let stars_open = world.space_unlocked()
            && world.has_star_drive(player_id)
            && world
                .cable_path()
                .and_then(|path| path.moon_top)
                .is_some_and(|top| tile.y < top - 20);
        if stratum.is_none() && ending_elapsed.is_none() && !stars_open {
            return None;
        }
        let rig = world.player(player_id)?;
        let dark = rig
            .cargo_bay
            .count(ItemKind::Layer(Stratum::DarkMatterVeins));
        let well = world
            .well_at(rig.x, rig.y)
            .map_or(0.0, |(_, weight)| weight);
        let mag_ride = rig.cart.filter(|ride| {
            world.terrain().tile(ride.rail).is_some_and(|tile| {
                matches!(tile.kind, fatherload_simulation::terrain::TileKind::Core(core) if core.is_mag_rail())
            })
        });
        let centre = world.heart_centre();
        let arena = centre.map_or(0.0, |centre| {
            let distance = ((tile.x - centre.x) as f32).hypot((tile.y - centre.y) as f32);
            let outside = distance - CHAMBER_RADIUS as f32;
            let t = (1.0 - outside / ARENA_EASE_TILES).clamp(0.0, 1.0);
            t * t * 2.0f32.mul_add(-t, 3.0)
        });
        let fight = world.heart().fight.clone();
        let elapsed = sim_millis(world).saturating_sub(fight.phase_millis);
        let fight_view = (fight.phase.fighting() && arena > 0.5).then(|| {
            let health = fight
                .entity
                .and_then(|id| {
                    world
                        .entities()
                        .get(fatherload_simulation::entity::EntityId(id))
                })
                .and_then(|entity| match entity.state {
                    fatherload_simulation::entity::EntityState::Enemy(state) => {
                        Some(state.health / state.max_health.max(1.0))
                    }
                    _ => None,
                })
                .unwrap_or(1.0);
            FightView {
                phase: fight.phase,
                coils_left: fight.coils_left(),
                health,
                pulse: if fight.phase == HeartPhase::Coils {
                    pulse_at(elapsed)
                } else {
                    PulseState::Calm
                },
                breath: if fight.phase == HeartPhase::Collapse {
                    breath_at(elapsed)
                } else {
                    BreathState::Held
                },
                beams: (fight.phase == HeartPhase::Ignition)
                    .then(|| (beam_angle(elapsed), beam_strength(elapsed))),
            }
        });
        Some(Self {
            stratum,
            mass: world.cargo_mass_of(player_id),
            dark,
            well,
            mag_ride: mag_ride.map(|ride| ride.speed as f32 / 1_000.0),
            fight: fight_view,
            arena,
            ending_elapsed,
            stars_open,
        })
    }

    /// The camera's scale for this seat: wide in the chamber and while the Heart wakes.
    #[must_use]
    pub fn zoom(&self) -> f32 {
        let arena = (ARENA_WIDE - 1.0).mul_add(self.arena, 1.0);
        let waking = self.ending_elapsed.map_or(1.0, |elapsed| {
            let t = (elapsed as f32 / 2_500.0).clamp(0.0, 1.0);
            let eased = t * t * 2.0f32.mul_add(-t, 3.0);
            (AWAKENING_WIDE - 1.0).mul_add(eased, 1.0)
        });
        arena.min(waking)
    }
}

/// World time at the world's tick (the fight's clock), milliseconds.
fn sim_millis(world: &WorldState) -> u64 {
    world.millis_at(world.simulation_tick())
}

struct Line {
    text: String,
    color: Color,
}

/// Descent VIII's panel (see the module docs), below `top` on the view's right.
pub(super) fn draw_heart_hud(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    let Some(heart) = view.heart.as_ref() else {
        return;
    };
    // The ending covers the screen or shows its own words.
    if view.ending.is_some_and(|ending| ending.phase.playing()) {
        return;
    }
    let mut title: Option<(&str, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    let mut bar: Option<(f32, Color)> = None;
    if let Some(fight) = heart.fight {
        title = Some(("THE HEART", GOLD));
        bar = Some((fight.health, GOLD));
        match fight.phase {
            HeartPhase::Coils => {
                lines.push(Line {
                    text: format!(
                        "Drive coils {}/{COIL_COUNT}: break them all",
                        fight.coils_left
                    ),
                    color: AMBER,
                });
                match fight.pulse {
                    PulseState::Tell => lines.push(Line {
                        text: "The ring tightens: a pull is coming!".to_owned(),
                        color: ORANGE,
                    }),
                    PulseState::Pull => lines.push(Line {
                        text: "IT PULLS: thrust away from the core".to_owned(),
                        color: RED,
                    }),
                    PulseState::Calm => lines.push(Line {
                        text: "The core is sealed while its coils turn".to_owned(),
                        color: FOG,
                    }),
                }
            }
            HeartPhase::Ignition => {
                lines.push(Line {
                    text: "IGNITION: strike the open core".to_owned(),
                    color: CREAM,
                });
                lines.push(Line {
                    text: if fight.beams.is_some_and(|(_, strength)| strength < 1.0) {
                        "The beams are waking: find the gap".to_owned()
                    } else {
                        "Keep out of the turning beams".to_owned()
                    },
                    color: ORANGE,
                });
            }
            HeartPhase::Collapse => {
                lines.push(match fight.breath {
                    BreathState::Exhale => Line {
                        text: "IT EXHALES: strike the core now!".to_owned(),
                        color: CREAM,
                    },
                    BreathState::Tell => Line {
                        text: "The core brightens: get ready".to_owned(),
                        color: GOLD,
                    },
                    BreathState::Held => Line {
                        text: "Collapse: it pulls and is armoured".to_owned(),
                        color: LAVENDER,
                    },
                });
            }
            HeartPhase::Asleep | HeartPhase::Awakened => {}
        }
    }
    if heart.dark > 0 || heart.mass != 1_000 {
        title.get_or_insert(("DARK MATTER", LAVENDER));
        let floating = heart.mass < 0;
        let color = if floating {
            PINK
        } else if heart.mass < 400 {
            LAVENDER
        } else {
            SKY_LIGHT
        };
        // The bar shows how light the rig is: empty at its own weight, full at weightless.
        if bar.is_none() {
            bar = Some(((1_000 - heart.mass).clamp(0, 1_000) as f32 / 1_000.0, color));
        }
        lines.push(Line {
            text: format!(
                "Weight {}%  ({} lumps; {WEIGHTLESS_LUMPS} lift an empty rig)",
                heart.mass / 10,
                heart.dark
            ),
            color,
        });
        lines.push(Line {
            text: if floating {
                "FALLING UP: roll over and thrust, or load ore".to_owned()
            } else if heart.mass < 250 {
                "Nearly weightless: the rig turns freely".to_owned()
            } else {
                "Ore weighs the rig back down".to_owned()
            },
            color,
        });
    }
    if heart.well > 0.05 {
        title.get_or_insert(("GRAVITY WELL", PINK));
        let color = if heart.well > 0.75 {
            RED
        } else if heart.well > 0.45 {
            ORANGE
        } else {
            PINK
        };
        lines.push(Line {
            text: if heart.well > 0.75 {
                "THE HORIZON: thrust out!".to_owned()
            } else {
                format!("Pull {:.0}%: the well drags you in", heart.well * 100.0)
            },
            color,
        });
    }
    if let Some(speed) = heart.mag_ride {
        title.get_or_insert(("MAGNETIC RAIL", TEAL));
        lines.push(Line {
            text: format!("Riding {speed:.0} tiles/s  (down at a switch: low line)"),
            color: MINT,
        });
    }
    if lines.is_empty() {
        return;
    }
    let (heading, heading_color) = title.unwrap_or(("THE HEART", GOLD));
    let width = 230;
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
        draw.draw_rectangle(inner, y, bar_width, 4, with_alpha(INK, 200));
        draw.draw_rectangle(
            inner,
            y,
            (bar_width as f32 * fill.clamp(0.0, 1.0)) as i32,
            4,
            color,
        );
        y += bar_height;
    }
    for line in lines {
        ui_kit::text(draw, &line.text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}

/// The open stars: over the Moon after the ending, a card for the seat's first minute up
/// there (the world remembers it was seen; the card follows the view while it is high).
pub(super) fn draw_stars_open(
    draw: &mut RaylibDrawHandle<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
) {
    if !view.heart.is_some_and(|heart| heart.stars_open) {
        return;
    }
    let pulse = (ui_kit::seconds() * 0.8).sin().mul_add(0.15, 0.85);
    let alpha = (pulse * 255.0) as u8;
    let width = (rect.width as i32 - 40).min(340);
    let x = rect.x as i32 + (rect.width as i32 - width) / 2;
    let y = rect.y as i32 + 40;
    ui_kit::panel(
        draw,
        Rectangle::new(x as f32, y as f32, width as f32, 44.0),
        PanelStyle::Accent,
    );
    let centre = x + width / 2;
    ui_kit::text_centred(
        draw,
        "THE STARS ARE OPEN",
        centre,
        y + 6,
        TextStyle::Title,
        with_alpha(CREAM, alpha),
    );
    ui_kit::text_centred(
        draw,
        "The Star Drive burns: the builders' road lies past the Moon",
        centre,
        y + 29,
        TextStyle::Body,
        with_alpha(SKY_LIGHT, alpha),
    );
}

/// The ending of the seat: its phase, the time since the cable lit and its skips (set by the
/// game loop from the seat's [`EndingView`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndingSeat {
    pub phase: fatherload_simulation::heart::ending::EndingPhase,
    pub elapsed: u64,
    pub view: EndingView,
}
