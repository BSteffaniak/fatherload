//! Upgrade reward moment scenes (WP-FEEL part 2): the shop showcase with before/after bars, the
//! purchase moment mid-transform (played through the real purchase flow), milestone banners
//! (including the Act I -> II unlock), the progression screen on keyboard, controller and in
//! split-screen, and fresh and locked milestone cosmetics in the paint bay.

use fatherload_simulation::{
    Player, PlayerId, RigCosmetics, WorldState,
    cosmetics::{
        Decal, DrillStyle, FlameColor, FrontAttachment, HullPattern, PaintColor, RearAttachment,
        RoofAttachment,
    },
    economy::{SurfaceZone, UpgradeKind},
    milestone::Milestone,
    terrain::{DeepStratum, TileKind, TilePosition},
    worldgen::Band,
};

use super::{
    ARROWS, Scene, SceneStage, WASD, XBOX, band_row, carve, park_at, park_rig, put_seat_inside,
    reveal, sink_rig,
};
use crate::{
    input::PlayerInput,
    shell::{GarageDraft, GarageRow, ModalScreen},
};

/// Fits every visible line of `rig` at `tier` (the tank too).
fn fit(rig: &mut Player, tier: u8) {
    rig.drill_strength = tier;
    rig.engine_level = tier;
    rig.cargo_bay_level = tier;
    rig.hull_level = tier;
    rig.radiator_level = tier;
    rig.fuel_tank_level = tier;
    rig.hull = rig.max_hull();
}

/// Records `milestones` as already earned (and the log primed, so new ones are announced).
fn earn(rig: &mut Player, milestones: &[Milestone]) {
    rig.milestones.primed = true;
    for milestone in milestones {
        rig.earn_milestone(*milestone, 1);
    }
}

/// Records every milestone the rig's tiers already meet (so only the scene's own moment is
/// announced).
fn earn_met(rig: &mut Player) {
    let met = Milestone::newly_met(
        rig,
        &fatherload_simulation::milestone::MilestoneFacts::default(),
    );
    earn(rig, &met);
}

/// The shop counter with the rig at `tier` (whatever it has met already earned), `row`
/// selected, for `seat` 0.
fn open_shop(stage: &mut SceneStage<'_>, tier: u8, row: usize, credits: u32) {
    stage.play(1, |world, seats| {
        let player = seats[0].player;
        park_at(world, player, SurfaceZone::Shop);
        if let Some(rig) = world.player_mut(player) {
            fit(rig, tier);
            rig.credits = credits;
            earn_met(rig);
        }
    });
    stage.settle(5);
    put_seat_inside(stage, 0, SurfaceZone::Shop, Some(ModalScreen::Shop));
    stage.seat(0).selected_menu_item = row;
}

/// Confirms the purchase receipt (the seat must be at [`ModalScreen::ShopConfirm`]) through the
/// real flow and lets the purchase moment play for `frames` frames.
fn buy_selected(stage: &mut SceneStage<'_>, frames: usize) {
    stage.seat(0).modal = Some(ModalScreen::ShopConfirm);
    stage.hold(
        1,
        PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        },
    );
    stage.settle(frames);
}

/// A trail-blazing explorer in milestone colours.
const EXPLORER: RigCosmetics = RigCosmetics {
    primary: PaintColor::Jade,
    secondary: PaintColor::Graphite,
    trim: PaintColor::Gold,
    flame: FlameColor::Aether,
    drill_style: DrillStyle::Corkscrew,
    pattern: HullPattern::Tiger,
    decal: Decal::Gem,
    roof: RoofAttachment::Trophy,
    front: FrontAttachment::Floodlamp,
    rear: RearAttachment::Lantern,
};

/// A deep diver in magma paint with an ember burn.
const MAGMA_DIVER: RigCosmetics = RigCosmetics {
    primary: PaintColor::Magma,
    secondary: PaintColor::Stealth,
    trim: PaintColor::Gold,
    flame: FlameColor::Ember,
    drill_style: DrillStyle::Standard,
    pattern: HullPattern::Racing,
    decal: Decal::Fang,
    roof: RoofAttachment::Antenna,
    front: FrontAttachment::BullBar,
    rear: RearAttachment::Stacks,
};

/// A sky captain in abyss blue with a starfield and a moon.
const SKY_CAPTAIN: RigCosmetics = RigCosmetics {
    primary: PaintColor::Abyss,
    secondary: PaintColor::Chrome,
    trim: PaintColor::Sky,
    flame: FlameColor::Plasma,
    drill_style: DrillStyle::Spire,
    pattern: HullPattern::Starfield,
    decal: Decal::Moon,
    roof: RoofAttachment::Halo,
    front: FrontAttachment::None,
    rear: RearAttachment::Spoiler,
};

/// A chrome show rig with wings and a claim flag crew mate.
const CHROME: RigCosmetics = RigCosmetics {
    primary: PaintColor::Chrome,
    secondary: PaintColor::Glowcap,
    trim: PaintColor::Crimson,
    flame: FlameColor::Prism,
    drill_style: DrillStyle::Crusher,
    pattern: HullPattern::TwoTone,
    decal: Decal::Wings,
    roof: RoofAttachment::Trophy,
    front: FrontAttachment::None,
    rear: RearAttachment::Pennant,
};

/// The milestones a mid-game rig has earned.
const MID_GAME: &[Milestone] = &[
    Milestone::FirstUpgrade,
    Milestone::DrillTier5,
    Milestone::EngineTier5,
    Milestone::ReachedStone,
    Milestone::FirstDeepLayer,
    Milestone::FirstArtifact,
    Milestone::FirstCave,
    Milestone::FirstVillage,
    Milestone::FirstKill,
    Milestone::HighFlyer,
    Milestone::IslandCapableTank,
];

/// The progression screen for seat `seat` with a mid-game record.
fn arrange_achievements(world: &mut WorldState, player: PlayerId, x: i32) {
    park_rig(world, player, x, 4);
    if let Some(rig) = world.player_mut(player) {
        fit(rig, 6);
        earn(rig, MID_GAME);
        rig.cosmetics = Some(EXPLORER);
        let records = &mut rig.milestones.records;
        records.deepest_row = 412;
        records.highest_altitude = 168;
        records.enemies_defeated = 37;
        records.artifacts_found = 9;
        records.upgrades_bought = 24;
    }
}

pub const REWARD_SCENES: &[Scene] = &[
    Scene {
        name: "rig_sheet_rewards",
        summary: "Every milestone-reward cosmetic on rigs: new paints, flames, patterns, decals, the corkscrew drill, depth trophy and trail lantern",
        setup: |stage| stage.rig_sheet = Some(crate::rendering::rig_art::sheet::RigSheet::Rewards),
    },
    Scene {
        name: "reward_shop_showcase",
        summary: "Shop showcase: the rig now and with the next drill, before/after bars and measured change lines",
        setup: |stage| open_shop(stage, 3, 0, 9_400),
    },
    Scene {
        name: "reward_shop_showcase_tank",
        summary: "Shop showcase for a fuel tank that reaches the low sky isles (climb ceiling bars)",
        setup: |stage| open_shop(stage, 3, 1, 9_400),
    },
    Scene {
        name: "reward_shop_showcase_engine",
        summary: "Shop showcase for the next engine: bigger thrusters, top speed and climb bars",
        setup: |stage| open_shop(stage, 7, 3, 60_000),
    },
    Scene {
        name: "reward_purchase_moment",
        summary: "The purchase moment: the new drill bolting on with sparks and a light beam, bars growing, the toast and the first-upgrade milestone banner",
        setup: |stage| {
            open_shop(stage, 1, 0, 4_850);
            buy_selected(stage, 40);
        },
    },
    Scene {
        name: "reward_purchase_flash",
        summary: "The purchase moment's flash: the rig's old engine turning into the new one",
        setup: |stage| {
            open_shop(stage, 6, 3, 60_000);
            buy_selected(stage, 22);
        },
    },
    Scene {
        name: "reward_milestone_toast",
        summary: "A milestone banner outdoors: the first deep layer, unlocking the depth trophy",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let row = band_row(world, Band::Deep(DeepStratum::CrystalFaults), 6);
                sink_rig(world, player, 120, row);
                carve(world, 112..129, row..row + 1);
                reveal(world, player, 100..141, row - 10..row + 12);
                if let Some(rig) = world.player_mut(player) {
                    fit(rig, 4);
                    earn(rig, &[Milestone::FirstUpgrade, Milestone::ReachedStone]);
                }
            });
            stage.settle(24);
        },
    },
    Scene {
        name: "reward_first_use",
        summary: "The first-use toast: a freshly fitted diamond drill biting into hard rock it bounced off before",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let row = band_row(world, Band::Stone, 4);
                sink_rig(world, player, 120, row);
                for x in 116..125 {
                    for y in row + 1..row + 4 {
                        world
                            .terrain_mut()
                            .set_kind(TilePosition { x, y }, TileKind::HardRock);
                    }
                }
                reveal(world, player, 104..137, row - 8..row + 10);
                if let Some(rig) = world.player_mut(player) {
                    fit(rig, 4);
                    earn_met(rig);
                    earn(rig, &[Milestone::ReachedStone]);
                }
            });
            stage.settle(5);
            // The drill was just bought (tier 3 -> 4); the rig now digs down.
            stage.seat(0).rewards.installed(UpgradeKind::Drill, 4, 0);
            stage.seat(0).rewards.install = None;
            stage.hold(
                12,
                PlayerInput {
                    drill_down: true,
                    ..PlayerInput::default()
                },
            );
        },
    },
    Scene {
        name: "reward_act_two_unlock",
        summary: "The Act I to II unlock: buying the tank that crosses the dead-air gap raises the sky banner",
        setup: |stage| {
            open_shop(stage, 12, 1, 400_000);
            buy_selected(stage, 50);
        },
    },
    Scene {
        name: "reward_achievements",
        summary: "The progression screen: milestones by group with medals and unlocks, records and the trophy shelf",
        setup: |stage| {
            stage.play(1, |world, seats| {
                arrange_achievements(world, seats[0].player, 96);
            });
            stage.settle(5);
            stage.seat(0).modal = Some(ModalScreen::Achievements);
            stage.seat(0).selected_menu_item = 8;
        },
    },
    Scene {
        name: "reward_achievements_pad",
        summary: "The progression screen on an Xbox controller, a locked Act II milestone selected",
        setup: |stage| {
            stage.play(1, |world, seats| {
                arrange_achievements(world, seats[0].player, 96);
            });
            stage.settle(5);
            stage.seat(0).modal = Some(ModalScreen::Achievements);
            stage.seat(0).selected_menu_item = Milestone::ALL.len() - 1;
            stage.use_devices(&[XBOX]);
        },
    },
    Scene {
        name: "reward_achievements_split",
        summary: "Split-screen: player 2 reads their progression screen while player 1 flies",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 90, 4);
                if let Some(second) = seats.get(1) {
                    arrange_achievements(world, second.player, 100);
                }
            });
            stage.settle(5);
            stage.seat(1).modal = Some(ModalScreen::Achievements);
            stage.seat(1).selected_menu_item = 2;
            stage.use_devices(&[WASD, ARROWS]);
        },
    },
    Scene {
        name: "reward_garage_unlock",
        summary: "The paint bay with a fresh milestone unlock tagged NEW: the depth trophy on the roof",
        setup: |stage| {
            let mut staged = PlayerId::new(1);
            stage.play(1, |world, seats| {
                staged = seats[0].player;
                park_at(world, staged, SurfaceZone::Repair);
                if let Some(rig) = world.player_mut(staged) {
                    fit(rig, 8);
                    earn(rig, MID_GAME);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Repair, Some(ModalScreen::Garage));
            let look = RigCosmetics {
                roof: RoofAttachment::Trophy,
                ..RigCosmetics::default_for(staged)
            };
            let seat = stage.seat(0);
            seat.garage = Some(GarageDraft::of(staged, Some(look)));
            seat.rewards
                .fresh_unlocks
                .extend(Milestone::FirstDeepLayer.rewards().iter().copied());
            seat.selected_menu_item = GarageRow::ALL
                .iter()
                .position(|row| *row == GarageRow::Roof)
                .unwrap_or_default();
        },
    },
    Scene {
        name: "reward_garage_locked",
        summary: "The paint bay trying on a locked milestone paint: the footer says how to earn it",
        setup: |stage| {
            let mut staged = PlayerId::new(1);
            stage.play(1, |world, seats| {
                staged = seats[0].player;
                park_at(world, staged, SurfaceZone::Repair);
                if let Some(rig) = world.player_mut(staged) {
                    fit(rig, 8);
                    earn(rig, &[Milestone::FirstUpgrade]);
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 0, SurfaceZone::Repair, Some(ModalScreen::Garage));
            let look = RigCosmetics {
                primary: PaintColor::Magma,
                flame: FlameColor::Ember,
                ..RigCosmetics::default_for(staged)
            };
            let seat = stage.seat(0);
            seat.garage = Some(GarageDraft::of(staged, Some(look)));
            seat.selected_menu_item = 0;
        },
    },
    Scene {
        name: "reward_cosmetics_lineup",
        summary: "Four rigs in town wearing milestone cosmetics: new paints, flames, patterns, decals, the corkscrew drill, trophy and lantern",
        setup: |stage| {
            stage.play(4, |world, seats| {
                for ((seat, x), (tier, look)) in seats.iter().zip([93, 96, 99, 102]).zip([
                    (5, EXPLORER),
                    (9, MAGMA_DIVER),
                    (13, SKY_CAPTAIN),
                    (15, CHROME),
                ]) {
                    park_rig(world, seat.player, x, 4);
                    if let Some(rig) = world.player_mut(seat.player) {
                        fit(rig, tier);
                        rig.cosmetics = Some(look);
                    }
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "reward_split_purchase",
        summary: "Split-screen: player 2's purchase moment and banner while player 1 drives outside",
        setup: |stage| {
            stage.play(2, |world, seats| {
                park_rig(world, seats[0].player, 90, 4);
                if let Some(second) = seats.get(1) {
                    park_at(world, second.player, SurfaceZone::Shop);
                    if let Some(rig) = world.player_mut(second.player) {
                        rig.credits = 5_000;
                    }
                }
            });
            stage.settle(5);
            put_seat_inside(stage, 1, SurfaceZone::Shop, Some(ModalScreen::ShopConfirm));
            stage.seat(1).selected_menu_item = 2;
            let confirm = PlayerInput {
                confirm: true,
                ..PlayerInput::default()
            };
            stage.hold_seats(1, &[PlayerInput::default(), confirm]);
            stage.settle(40);
            stage.use_devices(&[WASD, ARROWS]);
        },
    },
];
