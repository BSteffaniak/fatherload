//! Descent VII part B on screen (Wave 10 ancients-b): one small panel per seat with what the
//! garrison, the gate or the Colossus is doing right now.
//!
//! * A fort the rig holds: the wave out of all of them, the machines left, the build window or
//!   a wave's tell counting down (a tell, never a fail countdown), and a reminder that walking
//!   away lets it stand down.
//! * At the Seal Gate: the six keys, each held or missing.
//! * In the Gatekeeper's hall: the Colossus's stage and health, and the collapse's warning.
//! * Elsewhere in the Seal Gate: the next tremor's rumble.

use fatherload_simulation::garrison::{ColossusStage, FortPhase, SealKey};
use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::ui_kit::{self, PanelStyle, TextStyle};
use crate::render_snapshot::{RenderSnapshot, ViewSnapshot};

/// One line of the panel.
struct Line {
    text: String,
    color: Color,
}

/// Seconds from `now` to `tick` at `hz`.
#[allow(clippy::cast_precision_loss, reason = "a few seconds")]
fn seconds_until(now: u64, tick: u64, hz: u32) -> f32 {
    tick.saturating_sub(now) as f32 / hz.max(1) as f32
}

/// Draws the seat's Descent VII part B panel at the right of its view, from `top` down.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "a few lines of text; one block per place shown"
)]
pub(super) fn draw_garrison_hud(
    draw: &mut RaylibDrawHandle<'_>,
    snapshot: &RenderSnapshot<'_>,
    view: &ViewSnapshot<'_>,
    rect: Rectangle,
    top: i32,
) {
    let terrain = snapshot.terrain;
    if !terrain.settings().generator.has_descents() {
        return;
    }
    let frame = crate::view_frame::frame();
    let rig_tile = frame.tile_at(view.player.x, view.player.y);
    let garrison = snapshot.garrison;
    let hz = snapshot.tick_rate.hz();
    let now = snapshot.tick;
    let mut title: Option<(String, Color)> = None;
    let mut lines: Vec<Line> = Vec::new();
    // The boss panel sits at the top middle, clear of the fight it describes.
    let mut centred = false;
    let mut seal = false;
    // The Colossus's hall first: the fight is what matters there.
    if let Some(hall) = snapshot.colossus_hall
        && hall.contains(rig_tile)
        && garrison.colossus.awake()
    {
        let colossus = &garrison.colossus;
        centred = true;
        title = Some((
            format!("THE GATEKEEPER: {}", colossus.stage.name().to_uppercase()),
            GOLD,
        ));
        let health = colossus
            .entity
            .and_then(|id| snapshot.enemies.iter().find(|entity| entity.id.get() == id))
            .and_then(|entity| match entity.state {
                fatherload_simulation::entity::EntityState::Enemy(state) => {
                    Some(state.health_fraction())
                }
                _ => None,
            })
            .unwrap_or(1.0);
        lines.push(Line {
            text: format!("Health {:.0} %", health * 100.0),
            color: if health > 0.33 { CREAM } else { ORANGE },
        });
        if let Some((next, lands)) = colossus.collapsing {
            let left = seconds_until(now, lands.get(), hz);
            lines.push(Line {
                text: match next {
                    ColossusStage::Fallen => format!("THE FLOOR GIVES WAY ({left:.1} s)"),
                    _ => format!("THE VAULT COMES DOWN ({left:.1} s)"),
                },
                color: RED,
            });
        } else {
            lines.push(Line {
                text: match colossus.stage {
                    ColossusStage::Guard => "Step off the red marks; strike the head".to_owned(),
                    ColossusStage::Fallen => {
                        "Rise over the sweeping fist; dust means stones".to_owned()
                    }
                    ColossusStage::Unbound => {
                        "Its core is bare: hit it, mind the sentries".to_owned()
                    }
                },
                color: FOG,
            });
        }
    } else if let Some(fort) = snapshot
        .forts
        .iter()
        .find(|fort| fort.bounds.expanded(6).contains(rig_tile))
        && let Some(state) = garrison.forts.get(&fort.pylon())
        && state.phase != FortPhase::Dormant
    {
        let waves = state.kind.waves();
        let name = match state.kind {
            fatherload_simulation::garrison::FortKind::Barracks => "GARRISON FORT",
            fatherload_simulation::garrison::FortKind::Bastion => "THE GRAND BASTION",
        };
        title = Some((name.to_owned(), RED));
        match state.phase {
            FortPhase::Stirring => {
                lines.push(Line {
                    text: format!(
                        "Build your defences: wave 1 in {:.0} s",
                        seconds_until(now, state.due.get(), hz)
                    ),
                    color: GOLD,
                });
            }
            FortPhase::Telegraph => {
                lines.push(Line {
                    text: format!(
                        "Wave {} of {waves}: bays opening ({:.1} s)",
                        state.wave,
                        seconds_until(now, state.due.get(), hz)
                    ),
                    color: RED,
                });
            }
            FortPhase::Wave => {
                lines.push(Line {
                    text: format!(
                        "Wave {} of {waves}: {} machines left",
                        state.wave,
                        state.machines.len()
                    ),
                    color: ORANGE,
                });
            }
            FortPhase::Breather => {
                lines.push(Line {
                    text: format!(
                        "Wave {} beaten. Next in {:.0} s",
                        state.wave,
                        seconds_until(now, state.due.get(), hz)
                    ),
                    color: LEAF,
                });
            }
            FortPhase::Asleep => {
                lines.push(Line {
                    text: "The garrison sleeps. Its armoury paid out.".to_owned(),
                    color: MINT,
                });
            }
            FortPhase::Dormant => {}
        }
        if state.awake() {
            lines.push(Line {
                text: "Hold the fort: leave and it stands down".to_owned(),
                color: FOG,
            });
        }
    } else if let Some(door) = snapshot.seal_door
        && !garrison.seal.open
        && garrison.seal.opening.is_none()
        && door.expanded(10).contains(rig_tile)
    {
        // Off the right side too: the door and its sockets stand there.
        centred = true;
        seal = true;
        title = Some(("THE SEAL GATE".to_owned(), GOLD));
        let keys = super::world::garrison::seal_keys(view.player, snapshot.crust.deep_claim);
        for (key, held) in SealKey::ALL.into_iter().zip(keys) {
            let [r, g, b] = key.colour();
            lines.push(Line {
                text: format!("{} {}", if held { "[x]" } else { "[ ]" }, key.name()),
                color: if held { Color::new(r, g, b, 255) } else { FOG },
            });
        }
    }
    let Some((heading, heading_color)) = title else {
        return;
    };
    let width = 236;
    let height = 18 + lines.len() as i32 * 11;
    let (x, top) = if centred {
        if seal {
            // At the view's left, under the stats: the door stands to the rig's side.
            (rect.x as i32 + 16, top + 40)
        } else {
            (
                (rect.x + rect.width * 0.5) as i32 - width / 2,
                rect.y as i32 + 8,
            )
        }
    } else {
        ((rect.x + rect.width) as i32 - 16 - width, top)
    };
    if x <= rect.x as i32 + 120 && !centred {
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
        &heading,
        inner,
        top + 5,
        TextStyle::Bold,
        heading_color,
    );
    let mut y = top + 17;
    for line in lines {
        ui_kit::text(draw, &line.text, inner, y, TextStyle::Body, line.color);
        y += 11;
    }
}
