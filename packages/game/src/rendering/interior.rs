use raylib::prelude::*;

use super::{layout::UiLayout, render_scale_for_draw};
use drillgame_simulation::economy::SurfaceZone;

use crate::{
    render_snapshot::ViewSnapshot,
    shell::{ServiceAnimation, ShellState},
};

/// The room is authored on a fixed canvas (logical points) and scaled uniformly to fit the screen.
const CANVAS_WIDTH: f32 = 1280.0;
const CANVAS_HEIGHT: f32 = 720.0;
/// Canvas y where the floor starts; the floor colour extends to the screen edges below it.
const FLOOR_Y: f32 = 455.0;
/// Gap kept between UI panels and the screen edge.
const PANEL_MARGIN: f32 = 8.0;

/// Maps the fixed interior canvas onto a screen of any size: uniformly scaled to fit and centred,
/// so the room never stretches or spills off screen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct InteriorCanvas {
    /// Screen points per canvas point.
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    screen_width: f32,
    screen_height: f32,
}

impl InteriorCanvas {
    fn fit(screen_width: i32, screen_height: i32) -> Self {
        let screen_width = screen_width.max(1) as f32;
        let screen_height = screen_height.max(1) as f32;
        let scale = (screen_width / CANVAS_WIDTH).min(screen_height / CANVAS_HEIGHT);
        Self {
            scale,
            offset_x: CANVAS_WIDTH.mul_add(-scale, screen_width) * 0.5,
            offset_y: CANVAS_HEIGHT.mul_add(-scale, screen_height) * 0.5,
            screen_width,
            screen_height,
        }
    }

    fn to_screen(self, x: f32, y: f32) -> Vector2 {
        Vector2::new(
            x.mul_add(self.scale, self.offset_x),
            y.mul_add(self.scale, self.offset_y),
        )
    }

    /// The physical-pixel camera that draws canvas coordinates (see `render_scale_for_draw`).
    fn camera(self, density: f32) -> Camera2D {
        Camera2D {
            offset: Vector2::new(self.offset_x * density, self.offset_y * density),
            target: Vector2::zero(),
            rotation: 0.0,
            zoom: self.scale * density,
        }
    }

    /// A UI panel authored at `rect` on the canvas: same size in screen points (so text stays
    /// readable), centred where the canvas rect's centre lands, and kept on screen.
    fn panel(self, rect: Rectangle) -> Rectangle {
        let centre = self.to_screen(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let max_x = (self.screen_width - rect.width - PANEL_MARGIN).max(PANEL_MARGIN);
        let max_y = (self.screen_height - rect.height - PANEL_MARGIN).max(PANEL_MARGIN);
        Rectangle {
            x: rect
                .width
                .mul_add(-0.5, centre.x)
                .clamp(PANEL_MARGIN, max_x),
            y: rect
                .height
                .mul_add(-0.5, centre.y)
                .clamp(PANEL_MARGIN, max_y),
            width: rect.width,
            height: rect.height,
        }
    }
}

pub(super) fn draw_interior(
    draw: &mut RaylibDrawHandle<'_>,
    shell: &ShellState,
    view: &ViewSnapshot<'_>,
) {
    let zone = shell.interior_zone.unwrap_or(SurfaceZone::Depot);
    let (wall, trim, title) = interior_theme(zone);
    let (screen_width, screen_height) = (draw.get_screen_width(), draw.get_screen_height());
    let canvas = InteriorCanvas::fit(screen_width, screen_height);
    draw.clear_background(wall);
    let floor_y = canvas.to_screen(0.0, FLOOR_Y).y as i32;
    draw.draw_rectangle(
        0,
        floor_y,
        screen_width,
        screen_height - floor_y,
        Color::new(38, 32, 28, 255),
    );

    let service_x = interior_screen_x(interior_service_x_render(zone));
    let player_x = interior_screen_x(shell.interior_x);
    {
        let density = render_scale_for_draw(draw);
        let mut scene = draw.begin_mode2D(canvas.camera(density));
        scene.draw_rectangle(35, 130, 1210, 380, Color::new(18, 18, 24, 220));
        scene.draw_rectangle_lines(35, 130, 1210, 380, trim);

        scene.draw_rectangle(58, 338, 48, 118, Color::new(55, 32, 20, 255));
        scene.draw_rectangle_lines(58, 338, 48, 118, Color::GOLD);

        draw_interior_props(&mut scene, zone);
        draw_service_animation(&mut scene, shell, zone);

        scene.draw_rectangle((player_x - 11.0) as i32, 402, 22, 38, Color::GOLD);
        scene.draw_circle(player_x as i32, 392, 10.0, Color::SKYBLUE);
        let visor_offset = if shell.interior_facing >= 0.0 { 5 } else { -13 };
        scene.draw_rectangle(player_x as i32 + visor_offset, 389, 8, 5, Color::DARKBLUE);
    }

    let mut ui = UiLayout::screen(draw);
    ui.anchored_panel(
        canvas.panel(Rectangle {
            x: 55.0,
            y: 145.0,
            width: 610.0,
            height: 100.0,
        }),
        title,
        Some(npc_line(zone, view)),
        trim,
    );

    ui.anchored_panel(
        canvas.panel(Rectangle {
            x: 350.0,
            y: 620.0,
            width: 580.0,
            height: 74.0,
        }),
        "Controls",
        Some("A/D walk | E use counter/door | Esc exits"),
        Color::LIGHTGRAY,
    );

    ui.anchored_panel(
        canvas.panel(Rectangle {
            x: service_x - 78.0,
            y: 258.0,
            width: 156.0,
            height: 64.0,
        }),
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

fn draw_service_animation(draw: &mut impl RaylibDraw, shell: &ShellState, zone: SurfaceZone) {
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

fn draw_interior_props(draw: &mut impl RaylibDraw, zone: SurfaceZone) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
    }

    fn assert_inside(canvas: InteriorCanvas, rect: Rectangle) {
        assert!(rect.x >= 0.0 && rect.y >= 0.0, "{rect:?}");
        assert!(rect.x + rect.width <= canvas.screen_width, "{rect:?}");
        assert!(rect.y + rect.height <= canvas.screen_height, "{rect:?}");
    }

    #[test]
    fn canvas_fits_and_centres_at_any_size() {
        let reference = InteriorCanvas::fit(1280, 720);
        assert_near(reference.scale, 1.0);
        assert_near(reference.offset_x, 0.0);
        assert_near(reference.offset_y, 0.0);

        let ultrawide = InteriorCanvas::fit(2560, 1080);
        assert_near(ultrawide.scale, 1.5);
        assert_near(ultrawide.offset_x, (2560.0 - 1920.0) * 0.5);
        assert_near(ultrawide.offset_y, 0.0);

        let tall = InteriorCanvas::fit(1000, 1000);
        assert_near(tall.scale, 1000.0 / 1280.0);
        assert_near(tall.offset_x, 0.0);
        assert_near(tall.to_screen(0.0, 360.0).y, 500.0);
    }

    #[test]
    fn panels_keep_their_size_and_stay_on_screen() {
        let panels = [
            Rectangle::new(55.0, 145.0, 610.0, 100.0),
            Rectangle::new(350.0, 620.0, 580.0, 74.0),
            Rectangle::new(interior_screen_x(0.0) - 78.0, 258.0, 156.0, 64.0),
            Rectangle::new(interior_screen_x(640.0) - 78.0, 258.0, 156.0, 64.0),
        ];
        for (width, height) in [(960, 540), (1280, 720), (1920, 1080), (2560, 1080)] {
            let canvas = InteriorCanvas::fit(width, height);
            for authored in panels {
                let placed = canvas.panel(authored);
                assert_near(placed.width, authored.width);
                assert_near(placed.height, authored.height);
                assert_inside(canvas, placed);
            }
        }
        let reference = InteriorCanvas::fit(1280, 720).panel(panels[0]);
        assert_near(reference.x, panels[0].x);
        assert_near(reference.y, panels[0].y);
    }
}
