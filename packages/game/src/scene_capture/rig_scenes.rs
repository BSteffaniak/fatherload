//! Rig customization scenes: rig reference sheets, rigs at many tiers and paint jobs in the
//! world, the garage (keyboard, controller, split-screen) and co-op crews in custom paint.

use fatherload_simulation::{
    Player, PlayerId, RigCosmetics, WorldState,
    cosmetics::{
        Decal, DrillStyle, FlameColor, FrontAttachment, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
    economy::SurfaceZone,
};

use super::{
    ARROWS, Scene, SceneStage, WASD, XBOX, park_at, park_rig, put_seat_inside, reveal, sink_rig,
};
use crate::{
    rendering::rig_art::sheet::RigSheet,
    shell::{GARAGE_COUNTER_ROW, GarageDraft, GarageRow, ModalScreen},
};

/// Sets every visible upgrade line of `rig` to `tier`.
fn fit_tiers(rig: &mut Player, tier: u8) {
    rig.drill_strength = tier;
    rig.engine_level = tier;
    rig.cargo_bay_level = tier;
    rig.hull_level = tier;
    rig.radiator_level = tier;
    rig.drill_mount_level = tier.saturating_sub(1).min(2);
}

const fn job(primary: PaintColor, secondary: PaintColor, trim: PaintColor) -> RigCosmetics {
    RigCosmetics {
        primary,
        secondary,
        trim,
        flame: FlameColor::Classic,
        drill_style: DrillStyle::Standard,
        pattern: HullPattern::Racing,
        decal: Decal::None,
        roof: RoofAttachment::None,
        front: FrontAttachment::None,
        rear: RearAttachment::None,
    }
}

/// A cobalt racer with a blue flame, antenna and spoiler.
const RACER: RigCosmetics = RigCosmetics {
    flame: FlameColor::Blue,
    decal: Decal::Number,
    roof: RoofAttachment::Antenna,
    rear: RearAttachment::Spoiler,
    ..job(PaintColor::Cobalt, PaintColor::Pearl, PaintColor::Crimson)
};

/// A crimson hot rod with flames, chrome stacks and a bull bar.
const HOT_ROD: RigCosmetics = RigCosmetics {
    flame: FlameColor::Gold,
    decal: Decal::Flames,
    pattern: HullPattern::Solid,
    front: FrontAttachment::BullBar,
    rear: RearAttachment::Stacks,
    ..job(PaintColor::Crimson, PaintColor::Graphite, PaintColor::Gold)
};

/// A camo surveyor with a radar dish and a serrated drill.
const JUNGLE: RigCosmetics = RigCosmetics {
    flame: FlameColor::Green,
    pattern: HullPattern::Camo,
    drill_style: DrillStyle::Serrated,
    roof: RoofAttachment::Dish,
    ..job(PaintColor::Forest, PaintColor::Sand, PaintColor::Lime)
};

/// A blossom two-tone with a star, pennant and auger.
const BLOSSOM: RigCosmetics = RigCosmetics {
    flame: FlameColor::Pink,
    pattern: HullPattern::TwoTone,
    decal: Decal::Star,
    drill_style: DrillStyle::Auger,
    rear: RearAttachment::Pennant,
    ..job(PaintColor::Blossom, PaintColor::Pearl, PaintColor::Violet)
};

/// A hazard-striped worker with a siren and floodlamp.
const HAZARD: RigCosmetics = RigCosmetics {
    pattern: HullPattern::Hazard,
    decal: Decal::Bolt,
    roof: RoofAttachment::Siren,
    front: FrontAttachment::Floodlamp,
    ..job(
        PaintColor::Tangerine,
        PaintColor::Graphite,
        PaintColor::Gold,
    )
};

/// A pearl checker rig with a light bar and crusher.
const CHECKER: RigCosmetics = RigCosmetics {
    flame: FlameColor::White,
    pattern: HullPattern::Checker,
    decal: Decal::Picks,
    drill_style: DrillStyle::Crusher,
    roof: RoofAttachment::LightBar,
    ..job(PaintColor::Pearl, PaintColor::Steel, PaintColor::Cobalt)
};

/// Fits `player`'s rig with every line at `tier` (hull topped up) and dresses it in `cosmetics`.
fn dress(world: &mut WorldState, player: PlayerId, tier: u8, cosmetics: Option<RigCosmetics>) {
    if let Some(rig) = world.player_mut(player) {
        fit_tiers(rig, tier);
        rig.hull = rig.max_hull();
        rig.cosmetics = cosmetics;
    }
}

/// Enters the Repair Garage at its counter with a rig of `tier`; returns its player.
fn enter_repair(stage: &mut SceneStage<'_>, tier: u8, look: Option<RigCosmetics>) -> PlayerId {
    let mut staged = PlayerId::new(1);
    stage.play(1, |world, seats| {
        staged = seats[0].player;
        park_rig(world, staged, 68, 4);
        dress(world, staged, tier, look);
        if let Some(rig) = world.player_mut(staged) {
            rig.credits = 4_850;
        }
    });
    stage.settle(5);
    staged
}

/// Opens seat `seat`'s garage inside the Repair Garage with `draft`, `row` selected.
fn open_garage(
    stage: &mut SceneStage<'_>,
    seat: usize,
    player: PlayerId,
    draft: RigCosmetics,
    row: GarageRow,
) {
    put_seat_inside(stage, seat, SurfaceZone::Repair, Some(ModalScreen::Garage));
    let seat_ui = stage.seat(seat);
    seat_ui.garage = Some(GarageDraft::of(player, Some(draft)));
    seat_ui.selected_menu_item = GarageRow::ALL
        .iter()
        .position(|candidate| *candidate == row)
        .unwrap_or_default();
}

pub const RIG_SCENES: &[Scene] = &[
    Scene {
        name: "rig_sheet_tiers",
        summary: "The modular rig at each visual tier class: parked, climbing and drilling",
        setup: |stage| stage.rig_sheet = Some(RigSheet::Tiers),
    },
    Scene {
        name: "rig_sheet_paints",
        summary: "Paint jobs, patterns, decals, flames and attachments on rigs of several tiers",
        setup: |stage| stage.rig_sheet = Some(RigSheet::Paints),
    },
    Scene {
        name: "rig_sheet_drills",
        summary: "Every drill tier's head in every drill style",
        setup: |stage| stage.rig_sheet = Some(RigSheet::Drills),
    },
    Scene {
        name: "rig_lineup",
        summary: "Four crew rigs from stock to top tier in different paint jobs, side by side in town",
        setup: |stage| {
            stage.play(4, |world, seats| {
                for ((seat, x), (tier, look)) in seats.iter().zip([93, 96, 99, 102]).zip([
                    (1, None),
                    (6, Some(HOT_ROD)),
                    (11, Some(BLOSSOM)),
                    (15, Some(CHECKER)),
                ]) {
                    park_rig(world, seat.player, x, 4);
                    dress(world, seat.player, tier, look);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "rig_flight_custom",
        summary: "Custom rigs climbing out of a shaft with coloured thruster flames",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let first = seats[0].player;
                sink_rig(world, first, 100, 14);
                dress(world, first, 12, Some(RACER));
                if let Some(second) = seats.get(1) {
                    sink_rig(world, second.player, 103, 14);
                    dress(world, second.player, 8, Some(BLOSSOM));
                    reveal(world, second.player, 92..112, 4..20);
                }
                reveal(world, first, 92..112, 4..20);
                super::carve(world, 99..105, 8..15);
                for (player, x) in seats.iter().map(|seat| seat.player).zip([100.5, 103.5]) {
                    if let Some(rig) = world.player_mut(player) {
                        rig.x = x * fatherload_simulation::TILE_SIZE;
                        rig.y = 10.5 * fatherload_simulation::TILE_SIZE;
                        rig.velocity_y = -160.0;
                    }
                }
            });
            stage.hold_seats(
                4,
                &[
                    crate::input::PlayerInput {
                        thrust: true,
                        ..crate::input::PlayerInput::default()
                    },
                    crate::input::PlayerInput {
                        thrust: true,
                        ..crate::input::PlayerInput::default()
                    },
                ],
            );
        },
    },
    Scene {
        name: "garage_counter",
        summary: "The repair counter with its new paint bay row selected",
        setup: |stage| {
            super::enter_interior(stage, SurfaceZone::Repair, Some(ModalScreen::Repair));
            stage.seat(0).selected_menu_item = GARAGE_COUNTER_ROW;
        },
    },
    Scene {
        name: "garage_paint_bay",
        summary: "The paint bay: a draft paint job with live preview on the rig and the preview card",
        setup: |stage| {
            let player = enter_repair(stage, 8, None);
            open_garage(stage, 0, player, HOT_ROD, GarageRow::Flame);
        },
    },
    Scene {
        name: "garage_paint_bay_pad",
        summary: "The paint bay on an Xbox controller, a locked unlock tried on",
        setup: |stage| {
            let player = enter_repair(stage, 13, Some(JUNGLE));
            let draft = RigCosmetics {
                primary: PaintColor::Stealth,
                ..JUNGLE
            };
            open_garage(stage, 0, player, draft, GarageRow::Primary);
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "split_garage",
        summary: "Split-screen: player 2 in the paint bay while player 1 flies outside",
        setup: |stage| {
            let mut player = PlayerId::new(2);
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 88, 4);
                dress(world, seats[0].player, 4, Some(HAZARD));
                if let Some(second) = seats.get(1) {
                    player = second.player;
                    park_at(world, second.player, SurfaceZone::Repair);
                    dress(world, second.player, 6, None);
                }
            });
            stage.settle(5);
            open_garage(stage, 1, player, BLOSSOM, GarageRow::Decal);
            stage.use_devices(&[WASD, ARROWS]);
        },
    },
    Scene {
        name: "coop_custom_paints",
        summary: "Two players side by side in custom paint: tags, HUD titles and minimap follow the paint",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 108, 4);
                dress(world, seats[0].player, 7, Some(JUNGLE));
                if let Some(second) = seats.get(1) {
                    park_rig(world, second.player, 111, 4);
                    dress(world, second.player, 10, Some(RACER));
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "coop_custom_underground",
        summary: "Four players in custom paint drilling underground in split-screen",
        setup: |stage| {
            stage.play(4, |world, seats| {
                let looks = [Some(HOT_ROD), Some(BLOSSOM), Some(CHECKER), None];
                for ((seat, x), (look, tier)) in seats
                    .iter()
                    .zip([98, 101, 104, 107])
                    .zip(looks.into_iter().zip([5, 9, 14, 2]))
                {
                    sink_rig(world, seat.player, x, 16);
                    dress(world, seat.player, tier, look);
                }
                for seat in seats {
                    reveal(world, seat.player, 92..114, 4..22);
                }
            });
            stage.hold_seats(
                12,
                &[crate::input::PlayerInput {
                    drill_down: true,
                    ..crate::input::PlayerInput::default()
                }; 4],
            );
        },
    },
];
