use raylib::prelude::*;

use super::{SCREEN_WIDTH, layout::UiLayout};
use drillgame_simulation::economy::SurfaceZone;

use crate::{
    render_snapshot::ViewSnapshot,
    shell::{ServiceAnimation, ShellState},
};

pub(super) fn draw_interior(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    view: &ViewSnapshot<'_>,
) {
    let zone = shell.interior_zone.unwrap_or(SurfaceZone::Depot);
    let (wall, trim, title) = interior_theme(zone);
    draw.clear_background(wall);
    draw.draw_rectangle(0, 455, SCREEN_WIDTH, 265, Color::new(38, 32, 28, 255));
    draw.draw_rectangle(35, 130, 1210, 380, Color::new(18, 18, 24, 220));
    draw.draw_rectangle_lines(35, 130, 1210, 380, trim);

    draw.draw_rectangle(58, 338, 48, 118, Color::new(55, 32, 20, 255));
    draw.draw_rectangle_lines(58, 338, 48, 118, Color::GOLD);

    draw_interior_props(draw, zone);
    draw_service_animation(draw, shell, zone);
    let service_x = interior_screen_x(interior_service_x_render(zone));

    let player_x = interior_screen_x(shell.interior_x);
    draw.draw_rectangle((player_x - 11.0) as i32, 402, 22, 38, Color::GOLD);
    draw.draw_circle(player_x as i32, 392, 10.0, Color::SKYBLUE);
    let visor_offset = if shell.interior_facing >= 0.0 { 5 } else { -13 };
    draw.draw_rectangle(player_x as i32 + visor_offset, 389, 8, 5, Color::DARKBLUE);

    let mut ui = UiLayout::screen(draw);
    ui.anchored_panel(
        Rectangle {
            x: 55.0,
            y: 145.0,
            width: 610.0,
            height: 100.0,
        },
        title,
        Some(npc_line(zone, view)),
        trim,
    );

    ui.anchored_panel(
        Rectangle {
            x: 350.0,
            y: 620.0,
            width: 580.0,
            height: 74.0,
        },
        "Controls",
        Some("A/D walk | E use counter/door | Esc exits"),
        Color::LIGHTGRAY,
    );

    ui.anchored_panel(
        Rectangle {
            x: service_x - 78.0,
            y: 258.0,
            width: 156.0,
            height: 64.0,
        },
        "Press E",
        None,
        Color::GOLD,
    );
}

fn npc_line(zone: SurfaceZone, view: &ViewSnapshot<'_>) -> &'static str {
    match zone {
        SurfaceZone::Fuel if view.player.fuel < view.player.fuel_capacity * 0.25 => {
            "Pip: You came in on fumes. Try leaving with more than courage."
        }
        SurfaceZone::Fuel => "Pip: Fuel sale rumors start as soon as miners stop exploding.",
        SurfaceZone::Repair if view.rescue_count > 0 => {
            "Iona: I patched your rescue dents. The drill keeps receipts."
        }
        SurfaceZone::Repair => "Iona: Slow landings are cheaper than heroic ones.",
        SurfaceZone::Depot => "Kade: Market's twitchy. Sell jackpots before the buyers blink.",
        SurfaceZone::Headquarters => "Director Vale: Bring contracts, not ghost stories.",
        SurfaceZone::Shop => "Bolt: If it still rattles, upgrade the part making the noise.",
        SurfaceZone::Bank if view.player.loan_debt > 0 => {
            "Ledger: Debt compounds faster than tunnels collapse."
        }
        SurfaceZone::Bank => "Ledger: Credit is just fuel with paperwork.",
        SurfaceZone::Explosives => "Nix: Set it, run, then brag from a safer zip code.",
        SurfaceZone::Salvage => "Mara: Everything lost underground becomes inventory eventually.",
    }
}

fn draw_service_animation(draw: &mut RaylibDrawHandle<'_>, shell: &ShellState, zone: SurfaceZone) {
    let Some(animation) = shell.service_animation else {
        return;
    };
    let pulse = (shell.service_animation_seconds * 18.0) as i32;
    match animation {
        ServiceAnimation::Fuel if zone == SurfaceZone::Fuel => {
            draw.draw_line_ex(
                Vector2::new(820.0, 372.0),
                Vector2::new(620.0, 420.0),
                5.0,
                Color::YELLOW,
            );
            draw.draw_circle(620, 420, 10.0 + (pulse.rem_euclid(6)) as f32, Color::GOLD);
        }
        ServiceAnimation::Repair if zone == SurfaceZone::Repair => {
            draw.draw_rectangle(672, 392 - pulse.rem_euclid(12), 235, 8, Color::ORANGE);
        }
        _ => {}
    }
}

fn draw_interior_props(draw: &mut RaylibDrawHandle<'_>, zone: SurfaceZone) {
    match zone {
        SurfaceZone::Fuel => {
            draw.draw_rectangle(760, 330, 70, 120, Color::DARKBLUE);
            draw.draw_circle(795, 350, 18.0, Color::GOLD);
            draw.draw_line(830, 370, 900, 420, Color::BLACK);
        }
        SurfaceZone::Repair => {
            draw.draw_rectangle(690, 418, 190, 18, Color::MAROON);
            draw.draw_rectangle(725, 350, 18, 82, Color::GRAY);
            draw.draw_rectangle(825, 350, 18, 82, Color::GRAY);
        }
        SurfaceZone::Depot => {
            draw.draw_rectangle(800, 385, 125, 55, Color::BROWN);
            draw.draw_rectangle_lines(800, 385, 125, 55, Color::GOLD);
            draw.draw_rectangle(690, 345, 95, 95, Color::DARKGREEN);
        }
        SurfaceZone::Headquarters => {
            draw.draw_rectangle(690, 310, 300, 90, Color::new(18, 24, 42, 255));
            draw.draw_rectangle_lines(690, 310, 300, 90, Color::SKYBLUE);
            draw.draw_circle(735, 355, 26.0, Color::DARKBLUE);
        }
        SurfaceZone::Shop => {
            draw.draw_rectangle(675, 300, 320, 35, Color::PURPLE);
            for index in 0..6 {
                draw.draw_rectangle(695 + index * 48, 352, 28, 70, Color::DARKPURPLE);
            }
        }
        SurfaceZone::Bank => {
            draw.draw_rectangle(690, 315, 260, 95, Color::new(20, 70, 45, 255));
            draw.draw_rectangle_lines(690, 315, 260, 95, Color::GOLD);
        }
        SurfaceZone::Explosives => {
            draw.draw_rectangle(690, 330, 280, 85, Color::MAROON);
            for index in 0..4 {
                draw.draw_circle(725 + index * 55, 372, 16.0, Color::BLACK);
            }
        }
        SurfaceZone::Salvage => {
            draw.draw_rectangle(675, 390, 320, 25, Color::GRAY);
            draw.draw_rectangle(735, 325, 120, 70, Color::BROWN);
        }
    }
}

const fn interior_screen_x(room_x: f32) -> f32 {
    55.0 + room_x * 1.85
}

const fn interior_service_x_render(zone: SurfaceZone) -> f32 {
    match zone {
        SurfaceZone::Fuel => 430.0,
        SurfaceZone::Repair => 405.0,
        SurfaceZone::Depot => 455.0,
        SurfaceZone::Headquarters => 390.0,
        SurfaceZone::Shop => 450.0,
        SurfaceZone::Bank => 380.0,
        SurfaceZone::Explosives => 431.0,
        SurfaceZone::Salvage => 410.0,
    }
}

const fn interior_theme(zone: SurfaceZone) -> (Color, Color, &'static str) {
    match zone {
        SurfaceZone::Fuel => (
            Color::new(18, 30, 48, 255),
            Color::GOLD,
            "Fuel Station Interior",
        ),
        SurfaceZone::Repair => (
            Color::new(42, 22, 22, 255),
            Color::ORANGE,
            "Repair Garage Interior",
        ),
        SurfaceZone::Depot => (
            Color::new(18, 36, 25, 255),
            Color::GREEN,
            "Ore Depot Interior",
        ),
        SurfaceZone::Headquarters => (
            Color::new(22, 20, 44, 255),
            Color::SKYBLUE,
            "Borealis HQ Interior",
        ),
        SurfaceZone::Shop => (
            Color::new(34, 20, 42, 255),
            Color::MAGENTA,
            "Upgrade Shop Interior",
        ),
        SurfaceZone::Bank => (Color::new(16, 38, 29, 255), Color::GOLD, "Iron Ledger Bank"),
        SurfaceZone::Explosives => (
            Color::new(48, 22, 18, 255),
            Color::RED,
            "Nix's Explosive Shack",
        ),
        SurfaceZone::Salvage => (
            Color::new(25, 32, 28, 255),
            Color::LIME,
            "Mara's Salvage Yard",
        ),
    }
}
