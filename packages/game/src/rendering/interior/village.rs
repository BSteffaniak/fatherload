//! Village building interiors: the shared room structure (walls, wainscot, floor, exit door,
//! lamps) in the village's building style, the counter with its keeper (a resident, drawn by
//! [`crate::rendering::people_art`]), residents at home, and per-building dressing.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::too_many_lines,
    reason = "procedural art mixes small hashed integers and pixel coordinates"
)]

use raylib::prelude::*;

use fatherload_simulation::{
    village::{BuildingKind, BuildingStyle, Village},
    worldgen::WorldSettings,
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::super::pixel_art::palette::*;
use super::{
    CEILING_Y, COUNTER_HEIGHT, COUNTER_WIDTH, FLOOR_Y, RoomArea, RoomPlacement, WAINSCOT_Y,
    art::{self, InteriorSet},
    draw_controls_hint, draw_counter_body, draw_door, draw_key_prompt, draw_name_plate, draw_rig,
    room_pixel_x,
};
use crate::{
    input::prompts::Prompt,
    render_snapshot::ViewSnapshot,
    rendering::{
        people_art::{self, PersonPen},
        prompts::hint,
        rig_art,
        ui_kit::{self, UiCanvasScope},
    },
    shell::{self, SeatUi, VILLAGE_COUNTER_X, VillageInterior},
};

const fn village_set(style: BuildingStyle) -> &'static InteriorSet {
    match style {
        BuildingStyle::Timber => &art::village_timber::SET,
        BuildingStyle::Stone => &art::village_stone::SET,
        BuildingStyle::Adobe => &art::village_adobe::SET,
        BuildingStyle::Scrap => &art::village_scrap::SET,
        BuildingStyle::Carved => &art::village_carved::SET,
    }
}

/// Draws `seat`'s village building interior into `area`, then its name plate, prompts and the
/// keeper's greeting.
pub(in crate::rendering) fn draw_village_interior(
    draw: &mut RaylibDrawHandle<'_>,
    seat: &SeatUi,
    view: &ViewSnapshot<'_>,
    settings: &WorldSettings,
    area: RoomArea,
    player: Option<usize>,
    interior: VillageInterior,
) {
    let Some((found, building)) = shell::interior_village(settings, interior) else {
        return;
    };
    let lot = &found.buildings[building];
    let room_set = village_set(found.style);
    let placement = RoomPlacement::fit_area(area);
    let seconds = ui_kit::seconds();
    match area.viewport {
        None => draw.clear_background(room_set.shadow),
        Some(viewport) => draw.draw_rectangle(
            viewport.x,
            viewport.y,
            viewport.width,
            viewport.height,
            room_set.shadow,
        ),
    }
    let counter_x = room_pixel_x(VILLAGE_COUNTER_X) - COUNTER_WIDTH / 2;
    let keeper = lot
        .keeper
        .and_then(|index| found.population.characters.get(usize::from(index)));
    {
        let mut room = draw.begin_mode2D(placement.camera());
        super::draw_room_shell(&mut room, room_set, placement);
        draw_door(&mut room);
        draw_dressing(&mut room, &found, lot.kind, counter_x, seconds);
        draw_village_lamps(&mut room, counter_x, keeper.is_some());
        if let Some(keeper) = keeper {
            let keeper_x = counter_x + COUNTER_WIDTH / 2;
            people_art::draw_person(
                &mut room,
                PersonPen {
                    x: keeper_x as f32,
                    y: (FLOOR_Y - COUNTER_HEIGHT + 2) as f32,
                    pixel: 1.0,
                },
                keeper,
                -1,
                seconds,
            );
            draw_counter_body(&mut room, room_set, counter_x);
            draw_counter_goods(&mut room, lot.kind, counter_x, seconds);
        }
        for (resident, x) in shell::room_residents(&found, building) {
            let Some(character) = found.population.characters.get(usize::from(resident)) else {
                continue;
            };
            let facing = if room_pixel_x(seat.interior_x) < room_pixel_x(x) {
                -1
            } else {
                1
            };
            people_art::draw_person(
                &mut room,
                PersonPen {
                    x: room_pixel_x(x) as f32,
                    y: FLOOR_Y as f32,
                    pixel: 1.0,
                },
                character,
                facing,
                seconds,
            );
        }
        draw_rig(&mut room, seat, view, seconds);
    }

    let _canvas = UiCanvasScope::begin(draw);
    let canvas = ui_kit::canvas();
    let accent = rig_art::player_accent(view.player_id, view.player.cosmetics);
    let title = shell::building_title(&found, building);
    if seat.modal.is_some() {
        if area.align_top && player.is_some() {
            draw_name_plate(draw, &title, player, accent);
        }
        return;
    }
    draw_name_plate(draw, &title, player, accent);
    draw_controls_hint(draw, canvas);
    let at_door = seat.interior_x < 74.0;
    let near_counter =
        keeper.is_some() && (seat.interior_x - VILLAGE_COUNTER_X).abs() <= shell::VILLAGE_REACH;
    let near_resident = shell::room_residents(&found, building)
        .into_iter()
        .find(|(_, x)| (seat.interior_x - x).abs() <= shell::VILLAGE_REACH * 0.7);
    if let Some(keeper) = keeper {
        let keeper_x = counter_x + COUNTER_WIDTH / 2;
        let (bubble_x, bubble_y) =
            placement.to_canvas(canvas, keeper_x, FLOOR_Y - COUNTER_HEIGHT - 26);
        let line = greeting(&found, lot.kind, keeper.name.short());
        ui_kit::tooltip(draw, &line, bubble_x, bubble_y, 180, canvas.rect());
    }
    let prompt = if at_door {
        Some((28, "Leave"))
    } else if near_counter {
        Some((counter_x + COUNTER_WIDTH / 2, "Use counter"))
    } else {
        near_resident.map(|(_, x)| (room_pixel_x(x), "Talk"))
    };
    if let Some((target_x, label)) = prompt {
        let (prompt_x, prompt_y) =
            placement.to_canvas(canvas, target_x, FLOOR_Y - rig_art::BODY_HEIGHT - 8);
        draw_key_prompt(draw, prompt_x, prompt_y, hint(Prompt::Use, label));
    }
}

/// The keeper's greeting over the counter.
fn greeting(found: &Village, kind: BuildingKind, name: &str) -> String {
    match kind {
        BuildingKind::Hall => format!("{name}: Welcome to {}. Sit, tell us the news.", found.name),
        BuildingKind::Market => format!("{name}: Ore, relics, fuel. Fair-ish prices."),
        BuildingKind::Inn => format!("{name}: Warm in here. Buy a round, hear a rumour."),
        BuildingKind::Workshop => format!("{name}: Dents, holes, rattles. I fix all three."),
        BuildingKind::Depot => format!("{name}: Cheapest fuel this side of the hills."),
        BuildingKind::Smithy => format!("{name}: Charges packed fresh. Mind the sparks."),
        BuildingKind::Tailor => format!("{name}: Your rig deserves better than crew colours."),
        BuildingKind::Assay => format!("{name}: Bring me the good stuff. I pay for it."),
        BuildingKind::MapRoom => format!("{name}: Every hill and hole, if you've the coin."),
        BuildingKind::Shrine => format!("{name}: Rest a moment. The deep can wait."),
        BuildingKind::House | BuildingKind::Tower | BuildingKind::Farm | BuildingKind::MineHead => {
            format!("{name}: Make yourself at home.")
        }
    }
}

/// One lamp over the counter (when there is one) and one over the room.
fn draw_village_lamps(draw: &mut impl RaylibDraw, counter_x: i32, counter: bool) {
    let lamp = &art::LAMP;
    let mut positions = vec![150];
    if counter {
        positions.push(counter_x + COUNTER_WIDTH / 2 - lamp.width() / 2);
    }
    for x in positions {
        draw.draw_rectangle(x + 3, CEILING_Y, 1, 8, INK);
        super::ui_icon(draw, lamp, x, CEILING_Y + 6);
        let tip = Vector2::new(x as f32 + 3.5, (CEILING_Y + 15) as f32);
        draw.draw_triangle(
            tip,
            Vector2::new(tip.x - 34.0, FLOOR_Y as f32),
            Vector2::new(tip.x + 34.0, FLOOR_Y as f32),
            with_alpha(CREAM, 22),
        );
    }
}

/// Wall and floor dressing for each kind of building.
#[allow(clippy::too_many_lines, reason = "one arm per building kind")]
fn draw_dressing(
    draw: &mut impl RaylibDraw,
    found: &Village,
    kind: BuildingKind,
    counter_x: i32,
    seconds: f32,
) {
    // A window on the back wall, with the village outside (sky, or lamplit rock deep down).
    let underground = found.setting == fatherload_simulation::village::VillageSetting::Underground;
    let (wx, wy) = (82, CEILING_Y + 20);
    draw.draw_rectangle(wx - 2, wy - 2, 40, 30, INK);
    draw.draw_rectangle(wx, wy, 36, 26, if underground { NIGHT } else { SKY });
    if underground {
        draw.draw_rectangle(wx + 6, wy + 8, 3, 3, AMBER);
        draw.draw_rectangle(wx + 24, wy + 14, 2, 2, MINT);
    } else {
        draw.draw_rectangle(wx, wy + 18, 36, 8, LEAF);
        draw.draw_rectangle(wx + 4, wy + 4, 8, 3, BONE);
    }
    draw.draw_rectangle(wx + 17, wy, 2, 26, INK);
    draw.draw_rectangle(wx, wy + 12, 36, 2, INK);
    draw.draw_rectangle(wx - 3, wy + 26, 42, 3, LOAM);
    match kind {
        BuildingKind::House | BuildingKind::Hall => {
            // A hearth with a fire, a rug and a table with stools.
            let hearth = 196;
            draw.draw_rectangle(
                hearth - 2,
                WAINSCOT_Y - 18,
                48,
                FLOOR_Y - WAINSCOT_Y + 18,
                INK,
            );
            draw.draw_rectangle(
                hearth,
                WAINSCOT_Y - 16,
                44,
                FLOOR_Y - WAINSCOT_Y + 16,
                STONE,
            );
            draw.draw_rectangle(hearth + 8, FLOOR_Y - 18, 28, 18, NIGHT);
            for flame in 0..4 {
                let flicker = ((seconds * 8.0 + flame as f32 * 1.3).sin() * 2.5) as i32;
                let h = 8 + flame % 2 * 4 + flicker;
                draw.draw_rectangle(hearth + 11 + flame * 6, FLOOR_Y - 2 - h, 4, h, ORANGE);
                draw.draw_rectangle(hearth + 12 + flame * 6, FLOOR_Y - 2 - h / 2, 2, h / 2, GOLD);
            }
            draw.draw_rectangle(hearth - 6, WAINSCOT_Y - 20, 56, 3, LOAM);
            draw.draw_rectangle(hearth + 4, WAINSCOT_Y - 26, 6, 6, CLAY);
            draw.draw_rectangle(hearth + 30, WAINSCOT_Y - 25, 8, 5, GOLD);
            // Rug.
            draw.draw_rectangle(120, FLOOR_Y + 2, 70, 5, MAROON);
            draw.draw_rectangle(122, FLOOR_Y + 3, 66, 3, RED);
            // Table.
            draw.draw_rectangle(270, FLOOR_Y - 14, 34, 3, LOAM);
            draw.draw_rectangle(273, FLOOR_Y - 11, 2, 11, SOIL);
            draw.draw_rectangle(299, FLOOR_Y - 11, 2, 11, SOIL);
            draw.draw_rectangle(280, FLOOR_Y - 18, 4, 4, CREAM);
            if kind == BuildingKind::Hall {
                // The village banner and a notice board.
                draw.draw_rectangle(252, CEILING_Y + 10, 2, 50, STEEL_DARK);
                draw.draw_rectangle(254, CEILING_Y + 12, 22, 30, OCEAN);
                draw.draw_rectangle(258, CEILING_Y + 20, 14, 4, GOLD);
                draw.draw_rectangle(130, CEILING_Y + 26, 40, 28, SOIL);
                for (index, color) in [BONE, CREAM, SAND, BONE].into_iter().enumerate() {
                    let i = index as i32;
                    draw.draw_rectangle(
                        133 + (i % 2) * 18,
                        CEILING_Y + 29 + (i / 2) * 12,
                        15,
                        10,
                        color,
                    );
                }
            }
        }
        BuildingKind::Market | BuildingKind::Assay => {
            // Shelves of goods and sacks.
            for (row, y) in [56, 80].into_iter().enumerate() {
                draw.draw_rectangle(170, y, 70, 3, LOAM);
                for item in 0..6 {
                    let color = [GOLD, COPPER, LIME, BONE, RUST, SKY][(item + row) % 6];
                    draw.draw_rectangle(174 + item as i32 * 11, y - 7, 8, 7, color);
                    draw.draw_rectangle(174 + item as i32 * 11, y - 7, 8, 1, with_alpha(BONE, 90));
                }
            }
            for (x, color) in [(184, SAND), (200, CLAY), (216, SAND)] {
                draw.draw_rectangle(x, FLOOR_Y - 14, 14, 14, INK);
                draw.draw_rectangle(x + 1, FLOOR_Y - 13, 12, 13, color);
            }
        }
        BuildingKind::Inn => {
            // Barrels, a dartboard and tables.
            for x in [176, 196] {
                draw.draw_rectangle(x, FLOOR_Y - 20, 16, 20, INK);
                draw.draw_rectangle(x + 1, FLOOR_Y - 19, 14, 19, LOAM);
                draw.draw_rectangle(x + 1, FLOOR_Y - 14, 14, 2, STEEL_DARK);
                draw.draw_rectangle(x + 1, FLOOR_Y - 6, 14, 2, STEEL_DARK);
            }
            draw.draw_circle(250, CEILING_Y + 40, 10.0, INK);
            draw.draw_circle(250, CEILING_Y + 40, 9.0, BONE);
            draw.draw_circle(250, CEILING_Y + 40, 6.0, RED);
            draw.draw_circle(250, CEILING_Y + 40, 2.0, GOLD);
        }
        BuildingKind::Workshop | BuildingKind::Depot | BuildingKind::Smithy => {
            // A tool board and a workbench; the smithy's forge glows.
            draw.draw_rectangle(170, CEILING_Y + 22, 60, 34, SOIL);
            for (index, color) in [STEEL_LIGHT, STEEL, COPPER, STEEL_LIGHT]
                .into_iter()
                .enumerate()
            {
                let i = index as i32;
                draw.draw_rectangle(176 + i * 13, CEILING_Y + 28, 3, 20, color);
            }
            draw.draw_rectangle(170, FLOOR_Y - 16, 60, 4, LOAM);
            draw.draw_rectangle(174, FLOOR_Y - 12, 3, 12, SOIL);
            draw.draw_rectangle(223, FLOOR_Y - 12, 3, 12, SOIL);
            if kind == BuildingKind::Smithy {
                let glow = (150.0 + (seconds * 5.0).sin() * 60.0) as u8;
                draw.draw_rectangle(240, FLOOR_Y - 30, 30, 30, INK);
                draw.draw_rectangle(242, FLOOR_Y - 20, 26, 18, with_alpha(ORANGE, glow));
            }
            if kind == BuildingKind::Depot {
                draw.draw_rectangle(240, FLOOR_Y - 34, 20, 34, INK);
                draw.draw_rectangle(241, FLOOR_Y - 33, 18, 33, RED);
                draw.draw_rectangle(244, FLOOR_Y - 28, 12, 6, NIGHT);
            }
        }
        BuildingKind::Tailor => {
            // Bolts of cloth and swatches on the wall.
            for (index, color) in [ROSE, TEAL, GOLD, VIOLET, LIME, SKY]
                .into_iter()
                .enumerate()
            {
                let i = index as i32;
                draw.draw_rectangle(172 + i * 11, FLOOR_Y - 28, 9, 28, INK);
                draw.draw_rectangle(173 + i * 11, FLOOR_Y - 27, 7, 27, color);
                draw.draw_rectangle(174 + i * 10, CEILING_Y + 30, 8, 8, color);
            }
        }
        BuildingKind::MapRoom => {
            // Maps pinned on the wall and a globe.
            for (index, (x, y)) in [(168, 40), (200, 34), (232, 44)].into_iter().enumerate() {
                draw.draw_rectangle(x, y, 28, 22, INK);
                draw.draw_rectangle(x + 1, y + 1, 26, 20, CREAM);
                draw.draw_rectangle(x + 4, y + 6, 12, 2, [OCEAN, MOSS, CLAY][index]);
                draw.draw_rectangle(x + 8, y + 12, 14, 2, [CLAY, OCEAN, MOSS][index]);
                draw.draw_rectangle(x + 18, y + 4, 2, 2, RED);
            }
            draw.draw_circle(280, FLOOR_Y - 22, 10.0, INK);
            draw.draw_circle(280, FLOOR_Y - 22, 9.0, OCEAN);
            draw.draw_rectangle(276, FLOOR_Y - 26, 6, 4, MOSS);
            draw.draw_rectangle(278, FLOOR_Y - 12, 4, 12, LOAM);
        }
        BuildingKind::Shrine => {
            // An altar with candles and a hanging bell.
            draw.draw_rectangle(178, FLOOR_Y - 18, 56, 18, INK);
            draw.draw_rectangle(179, FLOOR_Y - 17, 54, 17, MAROON);
            for candle in 0..5 {
                let x = 184 + candle * 10;
                draw.draw_rectangle(x, FLOOR_Y - 26, 3, 8, CREAM);
                let flicker = ((seconds * 9.0 + candle as f32).sin() > 0.0) as i32;
                draw.draw_rectangle(x, FLOOR_Y - 29 - flicker, 3, 3, GOLD);
            }
            let swing = ((seconds * 1.5).sin() * 2.0) as i32;
            draw.draw_rectangle(205, CEILING_Y, 1, 22, INK);
            draw.draw_rectangle(200 + swing, CEILING_Y + 22, 12, 10, GOLD);
            draw.draw_rectangle(
                178,
                CEILING_Y,
                56,
                FLOOR_Y - CEILING_Y,
                with_alpha(GOLD, 18),
            );
        }
        BuildingKind::Tower | BuildingKind::Farm | BuildingKind::MineHead => {}
    }
    let _ = counter_x;
}

/// What the counter displays for each building.
fn draw_counter_goods(
    draw: &mut impl RaylibDraw,
    kind: BuildingKind,
    counter_x: i32,
    seconds: f32,
) {
    let top = FLOOR_Y - COUNTER_HEIGHT;
    match kind {
        BuildingKind::Market | BuildingKind::Assay => {
            // Scales.
            draw.draw_rectangle(counter_x + 10, top - 10, 1, 10, GOLD);
            draw.draw_rectangle(counter_x + 4, top - 10, 13, 1, GOLD);
            draw.draw_rectangle(counter_x + 3, top - 9, 4, 2, GOLD);
            draw.draw_rectangle(counter_x + 14, top - 9, 4, 2, GOLD);
            draw.draw_rectangle(counter_x + 56, top - 6, 6, 6, COPPER);
        }
        BuildingKind::Inn => {
            for x in [counter_x + 8, counter_x + 58] {
                draw.draw_rectangle(x, top - 8, 6, 8, AMBER);
                draw.draw_rectangle(x, top - 8, 6, 2, CREAM);
            }
        }
        BuildingKind::Shrine => {
            let glow = (100.0 + (seconds * 2.0).sin() * 50.0) as u8;
            draw.draw_rectangle(counter_x + 32, top - 10, 10, 10, with_alpha(GOLD, glow));
        }
        BuildingKind::Smithy => {
            draw.draw_rectangle(counter_x + 8, top - 5, 14, 5, STEEL_DARK);
        }
        BuildingKind::MapRoom => {
            draw.draw_rectangle(counter_x + 6, top - 2, 30, 2, CREAM);
        }
        _ => {}
    }
}
