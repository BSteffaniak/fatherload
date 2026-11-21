//! Tomb scenes (WP-TOMBS, T27, V10 worlds): a side-view tomb's pyramid entrance in a mesa, a
//! top-down crypt's mausoleum on the surface and another on the Moon, the run HUD inside a
//! placeholder floor of each style, the relic pick (keyboard and touch), the run summary, the
//! tomb codex and a split-screen party with a downed partner.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops,
    reason = "scene layouts mix small tile indices and world pixels"
)]

use fatherload_simulation::{
    EnemyKind, PlayerId, WorldState,
    pilot::PilotGear,
    terrain::{TILE_SIZE, TilePosition},
    tomb::{Relic, RunOutcome, SiteSetting, SiteStyle, TombSite},
    worldgen::{features::TileRect, tomb_sites},
};

use super::{Scene, reveal};
use crate::{input::devices::InputDevice, shell::ModalScreen};

/// The first site of `style` (and `setting`, if given) near town, ground settings first.
fn site(world: &WorldState, style: SiteStyle, setting: Option<SiteSetting>) -> Option<TombSite> {
    let settings = world.terrain().settings();
    let ground = TileRect {
        min: TilePosition { x: -9_000, y: -100 },
        max: TilePosition { x: 9_000, y: 900 },
    };
    let mut sites = tomb_sites::sites_touching(settings, &ground);
    if let Some(moon) = fatherload_simulation::worldgen::sky::moon(settings) {
        sites.extend(tomb_sites::sites_touching(settings, &moon.bounds()));
    }
    sites
        .into_iter()
        .filter(|site| setting.is_none_or(|setting| site.setting == setting))
        .find(|site| site.style == style)
}

/// A well kitted pilot (jetpack, suit, a rifle) so the art reads as late-game.
fn kit(world: &mut WorldState, player: PlayerId) {
    if let Some(rig) = world.player_mut(player) {
        rig.pilot_gear = PilotGear {
            jetpack: 1,
            suit: 2,
            headlamp: 2,
            ..PilotGear::default()
        };
        let _ = rig
            .pilot_arsenal
            .receive(fatherload_simulation::WeaponKind::Rifle);
        rig.pilot_arsenal.equipped = Some(fatherload_simulation::WeaponKind::Rifle);
        rig.credits = 4_200;
    }
}

/// The pilot out in front of `site`'s door, the rig parked beside it, everything revealed.
fn at_door(world: &mut WorldState, player: PlayerId, site: &TombSite) {
    kit(world, player);
    let (x, feet) = WorldState::outside_spot(site, 0);
    if let Some(rig) = world.player_mut(player) {
        let outward = if site.mirrored { 1.0 } else { -1.0 };
        rig.x = outward * TILE_SIZE * 4.0 + x;
        rig.y = feet - 11.5;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.x_remainder = 0.0;
    }
    let _ = world.place_pilot(player, x, feet);
    reveal(
        world,
        player,
        site.door.x - 50..site.door.x + 50,
        site.door.y - 30..site.door.y + 20,
    );
    world.stream_terrain_near_players();
}

/// Starts a run of `site` for `party` (the first member is at the door first).
fn enter(world: &mut WorldState, party: &[PlayerId], site: &TombSite) {
    for player in party {
        kit(world, *player);
    }
    at_door(world, party[0], site);
    let _ = world.scenario_enter_tomb(site, party);
    world.stream_terrain_near_players();
}

/// The run's pilot walked to `dx` tiles into room `room` of its floor.
fn into_room(world: &mut WorldState, player: PlayerId, room: usize, dx: i32) {
    let Some(run) = world.tomb_run_of(player).cloned() else {
        return;
    };
    let Some(room) = run.rooms.get(room) else {
        return;
    };
    let x = (room.interior.min.x + dx) as f32 * TILE_SIZE + 16.0;
    let feet = match run.site.style {
        SiteStyle::SideView => (room.interior.max.y + 1) as f32 * TILE_SIZE - 0.25,
        SiteStyle::TopDown => {
            (i32::midpoint(room.interior.min.y, room.interior.max.y) + 1) as f32 * TILE_SIZE - 4.0
        }
    };
    if let Some((id, _, _)) = world.pilot_of(player) {
        world.edit_pilot(id, |body, _| {
            body.x = x;
            body.y = feet;
            body.previous_x = x;
            body.previous_y = feet;
        });
    }
}

/// The index of the first room of `kind` on the run's floor.
fn room_of(
    world: &WorldState,
    player: PlayerId,
    kind: fatherload_simulation::tomb::RoomKind,
) -> usize {
    world
        .tomb_run_of(player)
        .and_then(|run| run.rooms.iter().position(|room| room.kind == kind))
        .unwrap_or(0)
}

pub const TOMB_SCENES: &[Scene] = &[
    Scene {
        name: "tomb_entrance_side_view",
        summary: "A side-view tomb's sandstone pyramid in a mesa: the sealed door in its alcove, the entrance banner naming its style, tier and seal",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let site = site(world, SiteStyle::SideView, Some(SiteSetting::Mesa))
                    .or_else(|| site(world, SiteStyle::SideView, None));
                if let Some(site) = site {
                    at_door(world, player, &site);
                }
            });
            stage.settle(8);
        },
    },
    Scene {
        name: "tomb_entrance_top_down",
        summary: "A top-down crypt's mausoleum on the surface: crypt-stone roof on carved pillars over a sealed floor hatch",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let site = site(world, SiteStyle::TopDown, Some(SiteSetting::Mesa))
                    .or_else(|| site(world, SiteStyle::TopDown, Some(SiteSetting::OldQuarter)))
                    .or_else(|| site(world, SiteStyle::TopDown, None));
                if let Some(site) = site {
                    at_door(world, player, &site);
                }
            });
            stage.settle(8);
        },
    },
    Scene {
        name: "tomb_entrance_moon",
        summary: "A crypt or tomb entrance on the Moon's regolith (tier 5), the pilot in front of it",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                let site = site(world, SiteStyle::TopDown, Some(SiteSetting::Moon))
                    .or_else(|| site(world, SiteStyle::SideView, Some(SiteSetting::Moon)));
                if let Some(site) = site {
                    if let Some(rig) = world.player_mut(player) {
                        rig.lunar.landed = true;
                    }
                    at_door(world, player, &site);
                }
            });
            stage.settle(8);
        },
    },
    Scene {
        name: "tomb_run_side_view",
        summary: "The run HUD in a side-view tomb's placeholder floor: a lever puzzle room, the gate barred, a guard ahead",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::SideView, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics = vec![Relic::SpringHeels, Relic::TrapSightLantern];
                        member.unbanked = 340;
                        member.banked = 120;
                    });
                    let room =
                        room_of(world, player, fatherload_simulation::tomb::RoomKind::Puzzle);
                    into_room(world, player, room, 3);
                }
            });
            stage.settle(20);
        },
    },
    Scene {
        name: "tomb_run_side_view_traps",
        summary: "A side-view trap hall: spike pits, one hidden pit glowing under the trap-sight lantern",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::SideView, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics = vec![Relic::TrapSightLantern, Relic::Featherfall];
                        member.unbanked = 90;
                    });
                    let room = room_of(
                        world,
                        player,
                        fatherload_simulation::tomb::RoomKind::TrapHall,
                    );
                    into_room(world, player, room, 1);
                }
            });
            stage.settle(12);
        },
    },
    Scene {
        name: "tomb_run_top_down",
        summary: "The run HUD in a top-down crypt's placeholder floor: flagstone rooms seen from above, a spike floor with a safe lane, wisps guarding",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::TopDown, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics = vec![Relic::Quickstep, Relic::VampiricPick];
                        member.unbanked = 610;
                    });
                    let room = room_of(
                        world,
                        player,
                        fatherload_simulation::tomb::RoomKind::TrapHall,
                    );
                    into_room(world, player, room, 0);
                    if let Some(run) = world.tomb_run_of(player).cloned()
                        && let Some(hall) = run.rooms.get(room)
                    {
                        let (x, y) = (
                            (hall.interior.max.x - 1) as f32 * TILE_SIZE,
                            (hall.interior.min.y + 2) as f32 * TILE_SIZE,
                        );
                        let _ = world.spawn_enemy(EnemyKind::MagmaWisp, x, y);
                    }
                }
            });
            stage.settle(10);
        },
    },
    Scene {
        name: "tomb_relic_pick",
        summary: "Between floors: the relic pick, three perks and a bargain, a synergy called out",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::SideView, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics = vec![Relic::RicochetRounds];
                        member.unbanked = 220;
                    });
                    world.scenario_tomb_stairs(player);
                }
            });
            stage.settle(4);
            stage.seat(0).selected_menu_item = 1;
            stage.settle(4);
        },
    },
    Scene {
        name: "tomb_relic_pick_touch",
        summary: "The relic pick on a phone: rows to tap, the touch controls hidden behind the dialog",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::TopDown, None) {
                    enter(world, &[player], &site);
                    world.scenario_tomb_stairs(player);
                }
            });
            stage.use_devices(&[InputDevice::Touch]);
            stage.settle(6);
        },
    },
    Scene {
        name: "tomb_summary",
        summary: "The run summary after clearing a tomb: floors, banked loot, relics carried and a lore page",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::SideView, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics =
                            vec![Relic::SpringHeels, Relic::GliderBoost, Relic::PactOfGreed];
                        member.unbanked = 1_260;
                        member.banked = 400;
                        member.lore = Some(2);
                    });
                    if let Some(id) = world.tomb_run_of(player).map(|run| run.id) {
                        world.scenario_end_tomb_run(id, RunOutcome::Finished);
                    }
                }
            });
            stage.settle(3);
            stage.seat(0).modal = Some(ModalScreen::TombSummary);
            stage.settle(3);
        },
    },
    Scene {
        name: "tomb_party_wiped",
        summary: "The death screen: the party lost below, the unbanked loot gone",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                if let Some(site) = site(world, SiteStyle::TopDown, None) {
                    enter(world, &[player], &site);
                    world.scenario_edit_tomb_member(player, |member| {
                        member.relics = vec![Relic::GlassCannon];
                        member.unbanked = 880;
                        member.banked = 150;
                    });
                    if let Some(id) = world.tomb_run_of(player).map(|run| run.id) {
                        world.scenario_end_tomb_run(id, RunOutcome::Wiped);
                    }
                }
            });
            stage.settle(3);
            stage.seat(0).modal = Some(ModalScreen::TombSummary);
            stage.settle(3);
        },
    },
    Scene {
        name: "tomb_codex",
        summary: "The tomb codex page: records, relics found (the rest still unknown) and lore pages",
        setup: |stage| {
            stage.play(1, |world, seats| {
                let player = seats[0].player;
                world.scenario_tomb_unlocks(player, |unlocks| {
                    unlocks.codex.extend([
                        Relic::SpringHeels,
                        Relic::RicochetRounds,
                        Relic::Quickstep,
                        Relic::PactOfGreed,
                    ]);
                    unlocks.lore.extend([0, 2, 5]);
                    unlocks.trophies = 2;
                    unlocks.runs = 7;
                    unlocks.finished = 3;
                    unlocks.best_haul = 3_840;
                });
            });
            stage.settle(3);
            stage.seat(0).modal = Some(ModalScreen::TombCodex);
            stage.settle(3);
        },
    },
    Scene {
        name: "tomb_coop_downed",
        summary: "Split-screen party in a side-view tomb: one partner downed, the other standing beside them to revive",
        setup: |stage| {
            stage.play(2, |world, seats| {
                let (a, b) = (seats[0].player, seats[1].player);
                if let Some(site) = site(world, SiteStyle::SideView, None) {
                    enter(world, &[a, b], &site);
                    let room = room_of(world, a, fatherload_simulation::tomb::RoomKind::Ambush);
                    into_room(world, a, room, 4);
                    into_room(world, b, room, 5);
                    world.scenario_down_tomb_member(b);
                }
            });
            stage.settle(10);
        },
    },
];
