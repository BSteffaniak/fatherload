//! Descent II news for local seats (Wave 8): what the Old Deep's rules did to a seat's rig, the
//! set pieces' moments and the story logs (the Fathers' expedition, the builder tablets and the
//! Star Core's revelation).

use fatherload_simulation::{
    GameplayEventKind, PlayerId,
    descent::Stratum,
    old_deep::{CorePhase, StormPhase},
};

use crate::{runtime::RuntimeEventKind, shell::FanfareKind};

use super::LocalGame;

/// A story log's title and text (`fatherload_simulation::old_deep::LOG_*`). The tablets and
/// the core's revelation are the codex's ([`fatherload_simulation::expedition::codex`], Wave
/// 9), which keeps them in the journal's Builder Tablets page once read.
#[must_use]
pub fn deep_log(log: u8) -> (String, &'static str) {
    use fatherload_simulation::{
        expedition::codex::{BUILDER_TABLETS, CORE_REVELATION},
        old_deep::{LOG_CORE_REVEAL, LOG_TABLET_FIRST},
    };
    if log < LOG_TABLET_FIRST {
        // The Fathers' camps hold the expedition's own logs (Wave 8 integration): the
        // journal's card tells them (`expedition_log_of_camp`), so this is only a fallback.
        return (
            "A Fathers' camp".to_owned(),
            "A cache of the Fathers' expedition, its canister handed on to the journal.",
        );
    }
    if log >= LOG_CORE_REVEAL {
        return (CORE_REVELATION.0.to_owned(), CORE_REVELATION.1);
    }
    let (title, text) = BUILDER_TABLETS[usize::from(log - LOG_TABLET_FIRST).min(3)];
    (format!("Builder tablet: {}", title.to_lowercase()), text)
}

impl LocalGame {
    /// Descent II's news for each local seat.
    #[allow(clippy::too_many_lines, reason = "one arm per Old Deep event")]
    pub(super) fn announce_old_deep(&mut self, players: &[PlayerId]) {
        let world = self.runtime.world();
        let seat_of = |player: PlayerId| players.iter().position(|other| *other == player);
        let stratum_of = |player: PlayerId| {
            world
                .player(player)
                .map(|rig| world.terrain().stratum_at(rig.tile_in(world.frame())))
        };
        let seats_in = |strata: &[Stratum]| -> Vec<usize> {
            players
                .iter()
                .enumerate()
                .filter(|(_, player)| stratum_of(**player).is_some_and(|s| strata.contains(&s)))
                .map(|(seat, _)| seat)
                .collect()
        };
        let all_seats: Vec<usize> = (0..players.len()).collect();
        let mut fanfares: Vec<(usize, String, String)> = Vec::new();
        let mut messages: Vec<(usize, String)> = Vec::new();
        for event in &self.frame_events {
            let RuntimeEventKind::Gameplay(gameplay) = &event.event else {
                continue;
            };
            match gameplay.kind {
                GameplayEventKind::ResonanceChain {
                    player_id,
                    shattered,
                    harvested,
                    ..
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        let text = if harvested {
                            format!(
                                "The fault rings: {shattered} crystals ring loose into the bay."
                            )
                        } else {
                            format!(
                                "The fault rings and {shattered} crystals shatter. Drill slower, or fit a Resonance Drill."
                            )
                        };
                        messages.push((seat, text));
                    }
                }
                GameplayEventKind::FossilBoneFreed {
                    player_id, intact, ..
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((
                            seat,
                            if intact {
                                "A fossil bone comes away whole.".to_owned()
                            } else {
                                "The bone crumbles. Dig around a bone before you lift it."
                                    .to_owned()
                            },
                        ));
                    }
                }
                GameplayEventKind::SkeletonCompleted {
                    player_id,
                    bones,
                    bounty,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        fanfares.push((
                            seat,
                            "A whole skeleton".to_owned(),
                            format!(
                                "All {bones} bones, unbroken. The museum pays {bounty} credits."
                            ),
                        ));
                    }
                }
                GameplayEventKind::GasTideTurned { high } => {
                    for seat in seats_in(&[Stratum::FossilOceans]) {
                        messages.push((
                            seat,
                            if high {
                                "The gas tide is in: the pockets breathe. Keep off the basins."
                                    .to_owned()
                            } else {
                                "The gas tide ebbs.".to_owned()
                            },
                        ));
                    }
                }
                GameplayEventKind::AmberStuck { player_id } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((seat, "Stuck in amber! Drill it away.".to_owned()));
                    }
                }
                GameplayEventKind::CathedralValveOpened { opened, .. } => {
                    for &seat in &seats_in(&[Stratum::PressureCathedrals]) {
                        messages.push((seat, format!("A lift valve groans open: {opened} of 3.")));
                    }
                }
                GameplayEventKind::CathedralLiftRestored { .. } => {
                    for &seat in &all_seats {
                        fanfares.push((
                            seat,
                            "The Cathedral Lift runs".to_owned(),
                            "Cathedral Station and the great nave are a ride apart now.".to_owned(),
                        ));
                    }
                }
                GameplayEventKind::DoseWarning { player_id, level } => {
                    if let Some(seat) = seat_of(player_id) {
                        messages.push((
                            seat,
                            match level {
                                1 => "Dose rising: the crew is sick, the rig burns more fuel.",
                                2 => "Heavy dose: the scanner is blind. Get out of the hollow.",
                                _ => "Lethal dose! The hull is failing. Climb now.",
                            }
                            .to_owned(),
                        ));
                    }
                }
                GameplayEventKind::MutationGained {
                    player_id,
                    mutation,
                } => {
                    if let Some(seat) = seat_of(player_id) {
                        let detail = match mutation {
                            fatherload_simulation::old_deep::Mutation::Lumen => {
                                "Your rig glows: hidden gems show near you."
                            }
                            fatherload_simulation::old_deep::Mutation::LeadSkin => {
                                "A grey skin creeps over the hull: the dose barely touches you."
                            }
                            fatherload_simulation::old_deep::Mutation::Regrowth => {
                                "The plates knit themselves: the hull mends."
                            }
                            fatherload_simulation::old_deep::Mutation::FuelGland => {
                                "Something in the tank is alive: it slowly refills."
                            }
                        };
                        fanfares.push((
                            seat,
                            format!("Mutation: {}", mutation.name()),
                            detail.to_owned(),
                        ));
                    }
                }
                GameplayEventKind::MachineryWoke { .. } => {
                    for seat in seats_in(&[Stratum::AncientMachineLayer]) {
                        messages.push((
                            seat,
                            "Machinery wakes: belts roll, pistons begin to slam.".to_owned(),
                        ));
                    }
                }
                GameplayEventKind::HeatStormChanged { phase } => {
                    for seat in seats_in(&[Stratum::MantleStormZone]) {
                        messages.push((
                            seat,
                            match phase {
                                StormPhase::Gathering => "A heat storm is gathering. Dig in!",
                                StormPhase::Raging => "The storm breaks: open caves burn.",
                                StormPhase::Calm => "The storm passes. A safe window.",
                            }
                            .to_owned(),
                        ));
                    }
                }
                GameplayEventKind::DeepLogRead { log, .. }
                    if fatherload_simulation::old_deep::expedition_log_of_camp(log).is_some() =>
                {
                    // A Fathers' camp: the expedition's card tells its log (Wave 8 integration).
                }
                GameplayEventKind::DeepLogRead { player_id, log } => {
                    let (title, text) = deep_log(log);
                    let seats: Vec<usize> =
                        if log == fatherload_simulation::old_deep::LOG_CORE_REVEAL {
                            all_seats.clone()
                        } else {
                            seat_of(player_id).into_iter().collect()
                        };
                    for seat in seats {
                        fanfares.push((seat, title.clone(), text.to_owned()));
                        messages.push((seat, text.to_owned()));
                    }
                }
                GameplayEventKind::StarCoreAwoke => {
                    for &seat in &all_seats {
                        fanfares.push((
                            seat,
                            "The Star Core wakes".to_owned(),
                            "Break its four pylons to drop the shield, then strike the core."
                                .to_owned(),
                        ));
                    }
                }
                GameplayEventKind::StarCorePhase {
                    phase,
                    strikes_left,
                } => {
                    let text = match phase {
                        CorePhase::Exposed => {
                            "The shield falls! Strike the core before it closes.".to_owned()
                        }
                        CorePhase::Shielded
                            if strikes_left < fatherload_simulation::old_deep::CORE_STRIKES =>
                        {
                            format!(
                                "The core shudders. {strikes_left} more strikes. The pylons relight."
                            )
                        }
                        CorePhase::Shielded => "The pylons hum. The shield holds.".to_owned(),
                        CorePhase::Dormant => "The Star Core sleeps again, whole.".to_owned(),
                        CorePhase::Beaten => continue,
                    };
                    for seat in seats_in(&[Stratum::MantleStormZone]) {
                        messages.push((seat, text.clone()));
                    }
                }
                GameplayEventKind::GlowmotherAwoke { .. } => {
                    for seat in seats_in(&[Stratum::RadioactiveHollow]) {
                        fanfares.push((
                            seat,
                            "The Glowmother".to_owned(),
                            "Something vast stirs in the green. Her brood pours out.".to_owned(),
                        ));
                    }
                }
                GameplayEventKind::GlowmotherDefeated { .. } => {
                    for seat in seats_in(&[Stratum::RadioactiveHollow]) {
                        fanfares.push((
                            seat,
                            "The Glowmother falls".to_owned(),
                            "The hollow dims. Her heart is a reactor bloom.".to_owned(),
                        ));
                    }
                }
                _ => {}
            }
        }
        for (seat, title, detail) in fanfares {
            self.shell
                .quest_fanfare(seat, FanfareKind::OldDeep, title, detail);
        }
        for (seat, message) in messages {
            self.shell.seat_mut(seat).message = message;
        }
    }
}
