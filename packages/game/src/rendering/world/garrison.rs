//! Descent VII part B in world space (Wave 10 ancients-b: the Sleeping Garrison, the Seal Gate
//! and the Gatekeeper's hall). Presentation only: everything derives from the replicated
//! garrison state, the terrain (a pure function of the seed for the forts and the hall) and the
//! clocks.
//!
//! * **Forts**: the hold pylon's ring of light on the floor while a garrison is up; the bays a
//!   wave will climb out of pulse red through their tell; racks of sleeping machines'
//!   silhouettes along a fort's back wall.
//! * **Defences**: turrets on their tripods (a tracer to what they hit), shock traps arcing
//!   while armed, a damage bar on anything battered.
//! * **The Seal Gate**: six sockets on the door, lit for the keys the seat's rig carries (all
//!   of them, one by one, as it opens), a gold glow while it sinks.
//! * **The Colossus's hall**: the mark under a raised fist, the line a sweep will run, the
//!   cracks glowing and dust pouring while the hall collapses, dust over every stone about to
//!   fall (tremors too).
//!
//! Every flicker is slow and low contrast and scales with the effects setting (off: steady).

use raylib::prelude::*;

use fatherload_simulation::{
    garrison::{
        ColossusStage, DefenceKind, FistMoveKind, FortPhase, Garrison, SOCKET_MILLIS, SealKey,
        TRACER_MILLIS, sockets_lit,
    },
    terrain::{TILE_SIZE, Terrain, TilePosition},
    worldgen::{
        TileRect,
        descent::garrison::{self as sites, Fort},
    },
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::{sprite, sprites::garrison as art};

use super::{ART, fx::disc, snap};

/// What one view needs to draw Descent VII part B.
pub(super) struct GarrisonScene<'a> {
    pub terrain: &'a Terrain,
    pub visible: Rectangle,
    pub garrison: &'a Garrison,
    pub forts: &'a [Fort],
    pub hall: Option<TileRect>,
    pub door: Option<TileRect>,
    /// Which seal keys the view's rig carries (`None`: no local rig).
    pub keys: Option<[bool; 6]>,
    pub world_tick: u64,
    pub tick_rate_hz: u32,
    pub seconds: f32,
    /// The effects setting's share (0 turns flicker off).
    pub effects: f32,
}

impl GarrisonScene<'_> {
    #[allow(clippy::cast_precision_loss, reason = "a few seconds")]
    fn seconds_until(&self, tick: u64) -> f32 {
        tick.saturating_sub(self.world_tick) as f32 / self.tick_rate_hz.max(1) as f32
    }

    fn millis_since(&self, tick: u64) -> u64 {
        self.world_tick.saturating_sub(tick).saturating_mul(1_000)
            / u64::from(self.tick_rate_hz.max(1))
    }

    fn on_screen(&self, rect: &TileRect) -> bool {
        let frame = crate::view_frame::frame();
        let left = frame.rel_col(rect.min.x) * TILE_SIZE;
        let top = frame.rel_row(rect.min.y) * TILE_SIZE;
        #[allow(clippy::cast_precision_loss, reason = "a few dozen tiles")]
        let (width, height) = (
            rect.width() as f32 * TILE_SIZE,
            rect.height() as f32 * TILE_SIZE,
        );
        left < self.visible.x + self.visible.width
            && left + width > self.visible.x
            && top < self.visible.y + self.visible.height
            && top + height > self.visible.y
    }

    /// A slow pulse in `0..=1` (steady at the effects setting's off).
    fn pulse(&self, rate: f32) -> f32 {
        if self.effects <= 0.0 {
            return 0.75;
        }
        (self.seconds * rate).sin().mul_add(0.5, 0.5)
    }
}

/// The top-left of `tile` in world pixels.
fn tile_px(tile: TilePosition) -> (f32, f32) {
    let frame = crate::view_frame::frame();
    (
        frame.rel_col(tile.x) * TILE_SIZE,
        frame.rel_row(tile.y) * TILE_SIZE,
    )
}

/// Which of the Seal Gate's keys `player` carries (`deep_claim`: the crew's Deep Claim), in
/// [`SealKey::ALL`] order.
#[must_use]
pub fn seal_keys(player: &fatherload_simulation::player::Player, deep_claim: bool) -> [bool; 6] {
    let mut keys = [false; 6];
    for (slot, key) in keys.iter_mut().zip(SealKey::ALL) {
        *slot = if key == SealKey::DeepClaim {
            deep_claim
        } else {
            key.systems()
                .iter()
                .all(|system| player.hazard_gear.has_system(*system))
        };
    }
    keys
}

/// Draws what lies behind the creatures: fort lights and bays, the racks, the gate's sockets,
/// the hall's marks and collapse, dust over falling stones.
pub(super) fn draw_garrison_back(draw: &mut impl RaylibDraw, scene: &GarrisonScene<'_>) {
    for fort in scene.forts {
        if scene.on_screen(&fort.bounds) {
            draw_fort(draw, scene, fort);
        }
    }
    if let Some(door) = scene.door
        && scene.on_screen(&door.expanded(6))
    {
        draw_seal_door(draw, scene, &door);
    }
    if let Some(hall) = scene.hall
        && scene.on_screen(&hall)
    {
        draw_hall(draw, scene, &hall);
    }
    for drop in &scene.garrison.drops {
        let (x, y) = tile_px(TilePosition {
            x: drop.x,
            y: drop.y,
        });
        let left = scene.seconds_until(drop.lands.get());
        // Dust pours from the ceiling over the stone; a mark grows on the floor under it.
        for grain in 0..10 {
            #[allow(clippy::cast_precision_loss, reason = "ten grains")]
            let g = grain as f32;
            let fall = (scene.seconds * 1.6 + g * 0.17).fract();
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(x + TILE_SIZE * 0.5 + (g * 2.3).sin() * 6.0),
                    snap(y + fall * TILE_SIZE * 3.0),
                    ART * 2.0,
                    ART * 2.0,
                ),
                with_alpha(SAND, 200),
            );
        }
        let urgency = (1.0 - left / 1.4).clamp(0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, reason = "a few art pixels")]
        let radius = 5 + (urgency * 5.0) as i32;
        // The mark where it lands: on the floor under the dust.
        let floor = (0..14)
            .find(|dy| {
                scene.terrain.is_solid_at(TilePosition {
                    x: drop.x,
                    y: drop.y + dy + 1,
                })
            })
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss, reason = "a few rows")]
        let floor_y = (floor as f32).mul_add(TILE_SIZE, y);
        disc(
            draw,
            x + TILE_SIZE * 0.5,
            floor_y + TILE_SIZE * 0.85,
            radius,
            with_alpha(RED, 70 + (urgency * 80.0) as u8),
        );
    }
}

/// A fort: its pylon's ring while the garrison is up, its lit bays, its racks.
fn draw_fort(draw: &mut impl RaylibDraw, scene: &GarrisonScene<'_>, fort: &Fort) {
    let state = scene.garrison.forts.get(&fort.pylon());
    let inside = fort.inside();
    // Racks of sleeping machines' silhouettes along the back wall (dim, every eighth column).
    let (left, top) = tile_px(inside.min);
    #[allow(clippy::cast_precision_loss, reason = "a few dozen tiles")]
    let width = inside.width() as f32 * TILE_SIZE;
    let floor_y = tile_px(TilePosition {
        x: inside.min.x,
        y: inside.max.y + 1,
    })
    .1;
    #[allow(clippy::cast_possible_truncation, reason = "a fort's width in tiles")]
    let racks = ((width - TILE_SIZE * 4.0) / (TILE_SIZE * 4.0)).max(0.0) as i32;
    let asleep = state.is_none_or(|fort| fort.phase == FortPhase::Asleep);
    for rack in 0..racks {
        #[allow(clippy::cast_precision_loss, reason = "a few racks")]
        let column = (rack as f32).mul_add(TILE_SIZE * 4.0, left + TILE_SIZE * 2.0);
        let rack_top = floor_y - TILE_SIZE * 2.4;
        draw.draw_rectangle_rec(
            Rectangle::new(snap(column), snap(rack_top), ART * 8.0, TILE_SIZE * 2.4),
            with_alpha(INK, 120),
        );
        // The machine's shoulders and head in the rack.
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(column + ART),
                snap(rack_top + ART * 2.0),
                ART * 6.0,
                ART * 5.0,
            ),
            with_alpha(PINE, 150),
        );
        // A dormant machine's lamp: green asleep, red while the garrison is up.
        let lamp = if asleep { MOSS } else { RED };
        draw.draw_rectangle_rec(
            Rectangle::new(
                snap(column + ART * 3.0),
                snap(rack_top + ART * 3.0),
                ART * 2.0,
                ART,
            ),
            with_alpha(lamp, 200),
        );
    }
    let _ = top;
    let Some(state) = state else {
        return;
    };
    let pylon = fort.pylon();
    let (px, py) = tile_px(pylon);
    if state.awake() {
        // The hold zone: a band of amber light along the floor round the pylon.
        let glow = scene.pulse(2.0);
        #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
        let alpha = (70.0 + 60.0 * glow) as u8;
        draw.draw_rectangle_rec(
            Rectangle::new(left, floor_y - ART * 3.0, width, ART * 3.0),
            with_alpha(AMBER, alpha),
        );
        draw.draw_rectangle_rec(
            Rectangle::new(
                left,
                floor_y - TILE_SIZE * 1.5,
                width,
                TILE_SIZE * 1.5 - ART * 3.0,
            ),
            with_alpha(AMBER, alpha / 6),
        );
        disc(
            draw,
            px + TILE_SIZE,
            py - TILE_SIZE,
            16,
            with_alpha(AMBER, alpha / 3),
        );
    }
    if state.phase == FortPhase::Telegraph {
        // The bays the wave climbs out of pulse red, faster as it nears.
        let left_s = scene.seconds_until(state.due.get());
        let rate = 4.0 + 8.0 * (1.0 - (left_s / 5.0).clamp(0.0, 1.0));
        let glow = scene.pulse(rate);
        for bay in &state.lit_bays {
            let (bx, by) = tile_px(*bay);
            #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
            let alpha = (70.0 + 120.0 * glow) as u8;
            disc(
                draw,
                bx + TILE_SIZE * 0.5,
                by + TILE_SIZE * 0.5,
                9,
                with_alpha(RED, alpha / 2),
            );
            disc(
                draw,
                bx + TILE_SIZE * 0.5,
                by + TILE_SIZE * 0.5,
                4,
                with_alpha(PINK, alpha),
            );
        }
    }
}

/// The Seal Gate: six sockets on the door, lit for the keys carried (all, one by one, while
/// it opens), and a gold glow while it sinks.
fn draw_seal_door(draw: &mut impl RaylibDraw, scene: &GarrisonScene<'_>, door: &TileRect) {
    let side = sites::door_side(scene.terrain.settings());
    let (left, top) = tile_px(door.min);
    // The sockets sit on the door's face, two columns of three.
    let face_x = left + TILE_SIZE;
    let _ = side;
    let opening = scene.garrison.seal.opening;
    let lit = opening.map(|(start, _)| sockets_lit(scene.millis_since(start.get())));
    if scene.garrison.seal.open {
        return;
    }
    for (index, key) in SealKey::ALL.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "six sockets")]
        let (column, row) = ((index % 2) as f32, (index / 2) as f32);
        let x = face_x + (column - 0.5) * ART * 14.0;
        let y = top + TILE_SIZE * 1.6 + row * TILE_SIZE * 2.2;
        let held = lit.map_or_else(
            || scene.keys.is_some_and(|keys| keys[index]),
            |count| index < count,
        );
        let [r, g, b] = key.colour();
        let colour = Color::new(r, g, b, 255);
        disc(draw, x, y, 5, with_alpha(INK, 220));
        if held {
            let glow = scene.pulse(1.5);
            #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
            let alpha = (150.0 + 80.0 * glow) as u8;
            disc(draw, x, y, 9, with_alpha(colour, 50));
            disc(draw, x, y, 4, with_alpha(colour, alpha));
            disc(draw, x, y, 1, with_alpha(CREAM, 230));
        } else {
            disc(draw, x, y, 4, with_alpha(SOIL_DARK, 255));
            disc(draw, x, y, 2, with_alpha(colour, 60));
        }
    }
    if let Some((start, _)) = opening {
        // A gold glow along the door's edge while the sockets light and it sinks.
        let since = scene.millis_since(start.get());
        #[allow(clippy::cast_precision_loss, reason = "a few seconds")]
        let strength = (since as f32 / (SOCKET_MILLIS * 6) as f32).min(1.0);
        #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
        let alpha = (60.0 + 80.0 * strength) as u8;
        #[allow(clippy::cast_precision_loss, reason = "a door's rows")]
        let height = door.height() as f32 * TILE_SIZE;
        draw.draw_rectangle_rec(
            Rectangle::new(left - ART * 2.0, top, TILE_SIZE * 2.0 + ART * 4.0, height),
            with_alpha(GOLD, alpha / 2),
        );
    }
}

/// The Colossus's hall: the mark under a raised fist, a sweep's line, the collapse's cracks
/// and dust.
#[allow(
    clippy::too_many_lines,
    reason = "the fist marks and the collapse in one pass"
)]
fn draw_hall(draw: &mut impl RaylibDraw, scene: &GarrisonScene<'_>, hall: &TileRect) {
    let colossus = &scene.garrison.colossus;
    if let Some(fist) = colossus.fist
        && scene.world_tick < fist.lands.get()
    {
        let (fx, fy) = tile_px(fist.from);
        let (tx, ty) = tile_px(fist.to);
        let tell = scene.world_tick < fist.strikes.get();
        let urgency = if tell {
            1.0 - (scene.seconds_until(fist.strikes.get()) / 1.4).clamp(0.0, 1.0)
        } else {
            1.0
        };
        match fist.kind {
            FistMoveKind::Slam => {
                // A red mark on the floor under the raised fist, wider than the shockwave.
                let floor = ty + TILE_SIZE;
                let half = TILE_SIZE * fatherload_simulation::garrison::SLAM_WAVE.0;
                #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
                let alpha = (130.0 + 110.0 * urgency) as u8;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        tx + TILE_SIZE * 0.5 - half,
                        floor - ART * 2.0,
                        half * 2.0,
                        ART * 2.0,
                    ),
                    with_alpha(RED, alpha),
                );
                // A column of red light up from the mark to the fist.
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        tx + TILE_SIZE * 0.5 - half,
                        floor - TILE_SIZE * 3.0,
                        half * 2.0,
                        TILE_SIZE * 3.0 - ART * 2.0,
                    ),
                    with_alpha(RED, alpha / 3),
                );
                for chevron in 0..3 {
                    #[allow(clippy::cast_precision_loss, reason = "three chevrons")]
                    let lift = (chevron as f32)
                        .mul_add(ART * 4.0, (scene.seconds * 3.0).fract() * ART * 4.0);
                    draw.draw_rectangle_rec(
                        Rectangle::new(
                            snap(tx + TILE_SIZE * 0.5 - ART * 2.0),
                            snap(floor - ART * 4.0 - lift),
                            ART * 4.0,
                            ART,
                        ),
                        with_alpha(PINK, alpha / 2),
                    );
                }
            }
            FistMoveKind::Sweep => {
                // The line the sweep will run, glowing along the crypt floor.
                let (from, to) = if fx < tx { (fx, tx) } else { (tx, fx) };
                #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
                let alpha = (60.0 + 140.0 * urgency) as u8;
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        from,
                        fy + TILE_SIZE - ART * 2.0,
                        to - from + TILE_SIZE,
                        ART * 2.0,
                    ),
                    with_alpha(ORANGE, alpha),
                );
                draw.draw_rectangle_rec(
                    Rectangle::new(
                        from,
                        fy - TILE_SIZE * 0.5,
                        to - from + TILE_SIZE,
                        TILE_SIZE * 1.5,
                    ),
                    with_alpha(RED, alpha / 5),
                );
            }
        }
    }
    if let Some((next, lands)) = colossus.collapsing {
        let urgency = 1.0 - (scene.seconds_until(lands.get()) / 4.0).clamp(0.0, 1.0);
        let area = match next {
            ColossusStage::Fallen => sites::cracked_floor(hall),
            _ => sites::cracked_vault(hall),
        };
        let (left, top) = tile_px(area.min);
        #[allow(clippy::cast_precision_loss, reason = "a few dozen tiles")]
        let (width, height) = (
            area.width() as f32 * TILE_SIZE,
            area.height() as f32 * TILE_SIZE,
        );
        let glow = scene.pulse(3.0 + urgency * 6.0);
        #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
        let alpha = (50.0 + 110.0 * urgency * glow) as u8;
        draw.draw_rectangle_rec(
            Rectangle::new(left, top, width, height),
            with_alpha(ORANGE, alpha),
        );
        // Dust pouring from the vault all along the hall.
        let (hall_left, hall_top) = tile_px(sites::upper_hall(hall).min);
        #[allow(clippy::cast_precision_loss, reason = "a hall's width")]
        let hall_width = sites::upper_hall(hall).width() as f32 * TILE_SIZE;
        for grain in 0..40 {
            #[allow(clippy::cast_precision_loss, reason = "forty grains")]
            let g = grain as f32;
            let x = hall_left + (g * 0.618).fract() * hall_width;
            let fall = (scene.seconds * (0.8 + (g * 0.37).fract()) + g * 0.13).fract();
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(x),
                    snap(hall_top + fall * TILE_SIZE * 12.0),
                    ART,
                    ART * 2.0,
                ),
                with_alpha(SAND, 150),
            );
        }
    }
}

/// Draws the build kit's defences (turrets, traps; barricades are tiles) with their tracers
/// and damage bars.
pub(super) fn draw_defences(draw: &mut impl RaylibDraw, scene: &GarrisonScene<'_>) {
    for defence in scene.garrison.defences.values() {
        let (x, y) = tile_px(defence.top());
        if x > scene.visible.x + scene.visible.width + TILE_SIZE
            || x + TILE_SIZE < scene.visible.x - TILE_SIZE
            || y > scene.visible.y + scene.visible.height + TILE_SIZE * 2.0
            || y + TILE_SIZE * 2.0 < scene.visible.y - TILE_SIZE
        {
            continue;
        }
        match defence.kind {
            DefenceKind::Turret => {
                let firing = defence
                    .fired
                    .is_some_and(|shot| scene.millis_since(shot.tick.get()) < TRACER_MILLIS);
                let flip = defence
                    .fired
                    .is_some_and(|shot| shot.to.x < defence.anchor.x);
                sprite::draw_sprite_flipped(
                    draw,
                    &art::DEFENCE_TURRET,
                    usize::from(firing),
                    Vector2::new(snap(x), snap(y)),
                    ART,
                    flip,
                    Color::WHITE,
                );
                if let Some(shot) = defence.fired
                    && firing
                {
                    let (tx, ty) = tile_px(shot.to);
                    let muzzle =
                        Vector2::new(x + if flip { 0.0 } else { TILE_SIZE }, y + ART * 14.5);
                    let target = Vector2::new(tx + TILE_SIZE * 0.5, ty + TILE_SIZE * 0.5);
                    draw.draw_line_ex(muzzle, target, ART, with_alpha(CREAM, 220));
                    draw.draw_line_ex(muzzle, target, ART * 2.5, with_alpha(GOLD, 70));
                    disc(draw, target.x, target.y, 3, with_alpha(GOLD, 160));
                }
            }
            DefenceKind::ShockTrap => {
                let armed = defence.ready.get() <= scene.world_tick;
                sprite::draw_sprite_flipped(
                    draw,
                    &art::DEFENCE_TRAP,
                    usize::from(!armed),
                    Vector2::new(snap(x), snap(y + TILE_SIZE - ART * 5.0)),
                    ART,
                    false,
                    Color::WHITE,
                );
                if armed {
                    let glow = scene.pulse(5.0);
                    #[allow(clippy::cast_possible_truncation, reason = "alpha in 0..255")]
                    disc(
                        draw,
                        x + TILE_SIZE * 0.5,
                        y + TILE_SIZE - ART * 3.0,
                        6,
                        with_alpha(SKY, (30.0 + 40.0 * glow) as u8),
                    );
                }
            }
            DefenceKind::Barricade => {}
        }
        // A damage bar on anything battered.
        let most = defence.kind.health();
        if defence.health < most {
            let share = f32::from(defence.health) / f32::from(most.max(1));
            let bar_y = y - ART * 3.0;
            draw.draw_rectangle_rec(
                Rectangle::new(snap(x + ART), snap(bar_y), TILE_SIZE - ART * 2.0, ART * 1.5),
                with_alpha(INK, 200),
            );
            draw.draw_rectangle_rec(
                Rectangle::new(
                    snap(x + ART),
                    snap(bar_y),
                    (TILE_SIZE - ART * 2.0) * share,
                    ART * 1.5,
                ),
                if share > 0.4 { LEAF } else { ORANGE },
            );
        }
    }
}
