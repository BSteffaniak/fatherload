//! Player-facing reading of an accepted quest: objective lines for the quest log and HUD, and
//! the markers maps and the world draw.

use crate::{economy::SurfaceZone, player::Player, terrain::TilePosition, tick::TickRate};

use super::{ActiveQuest, Objective, ObjectiveProgress, QuestDef, QuestId, Twist};

/// What a quest marker shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuestMarkerKind {
    /// A place to go (the current objective's target).
    Target,
    /// A marked spot; `lit` once touched (simultaneous) or done (detonations), `next` for the
    /// spot to bomb next.
    Spot { lit: bool, next: bool },
    /// The escorted haul.
    Haul,
    /// The relay marker resting on the ground.
    Beacon,
    /// A hold-and-climb switch.
    Switch { held: bool },
}

/// One marker of an accepted quest, in tiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuestMarker {
    pub quest: QuestId,
    pub position: TilePosition,
    pub kind: QuestMarkerKind,
}

/// Display name of a town building.
#[must_use]
pub const fn zone_name(zone: SurfaceZone) -> &'static str {
    match zone {
        SurfaceZone::Fuel => "the fuel station",
        SurfaceZone::Repair => "the repair garage",
        SurfaceZone::Depot => "the depot",
        SurfaceZone::Headquarters => "HQ",
        SurfaceZone::Shop => "the shop",
        SurfaceZone::Bank => "the bank",
        SurfaceZone::Explosives => "the explosives shack",
        SurfaceZone::Salvage => "the salvage yard",
    }
}

/// The objectives of the quest's current stage with their progress (empty once ready).
fn current<'a>(
    def: &'a QuestDef,
    quest: &'a ActiveQuest,
) -> impl Iterator<Item = (Objective, &'a ObjectiveProgress)> + 'a {
    let stage = usize::from(quest.stage);
    let start = def.stage_start(stage);
    def.stages
        .get(stage)
        .copied()
        .unwrap_or(&[])
        .iter()
        .copied()
        .zip(quest.objectives.iter().skip(start))
}

/// Markers of the quest's current stage.
#[must_use]
pub fn quest_markers(quest: &ActiveQuest, tick: u64) -> Vec<QuestMarker> {
    let Some(def) = quest.def() else {
        return Vec::new();
    };
    if quest.ready {
        return Vec::new();
    }
    let mut markers = Vec::new();
    let mut push = |position: TilePosition, kind: QuestMarkerKind| {
        markers.push(QuestMarker {
            quest: quest.quest,
            position,
            kind,
        });
    };
    let target = |index: u8| {
        quest
            .targets
            .get(usize::from(index))
            .map(|target| target.center)
    };
    for (objective, progress) in current(def, quest) {
        if progress.done {
            continue;
        }
        match objective {
            Objective::Reach { target: index }
            | Objective::Dig { target: index, .. }
            | Objective::Scan { target: index }
            | Objective::OpenPoi { target: index }
            | Objective::Survive { target: index, .. }
            | Objective::PickUp { target: index, .. }
            | Objective::Build { target: index, .. }
            | Objective::RecoverLog { target: index } => {
                if let Some(center) = target(index) {
                    push(center, QuestMarkerKind::Target);
                }
            }
            Objective::Detonate { spots, .. } => {
                let done = usize::try_from(progress.count).unwrap_or(usize::MAX);
                for (index, spot) in progress.spots.iter().enumerate().take(usize::from(spots)) {
                    push(
                        *spot,
                        QuestMarkerKind::Spot {
                            lit: index < done,
                            next: index == done,
                        },
                    );
                }
            }
            Objective::Simultaneous { .. } => {
                for (index, spot) in progress.spots.iter().enumerate() {
                    let lit = progress.marks.get(index).is_some_and(|mark| *mark > 0);
                    push(*spot, QuestMarkerKind::Spot { lit, next: false });
                }
            }
            Objective::Relay { target: index, .. } => {
                if progress.carrier.is_none()
                    && let Some(position) = progress.position
                {
                    push(position, QuestMarkerKind::Beacon);
                }
                if let Some(center) = target(index) {
                    push(center, QuestMarkerKind::Target);
                }
            }
            Objective::HoldAndClimb { goal, .. } => {
                let held = progress.timer > tick;
                if let Some(switch) = progress.spots.first() {
                    push(*switch, QuestMarkerKind::Switch { held });
                }
                if let Some(center) = target(goal) {
                    push(center, QuestMarkerKind::Target);
                }
            }
            Objective::Escort { .. } => {
                if let Some(position) = progress.position {
                    push(position, QuestMarkerKind::Haul);
                }
            }
            Objective::Deliver { .. }
            | Objective::ReachDepth { .. }
            | Objective::ReturnToSurface
            | Objective::SplitDelivery { .. } => {}
        }
    }
    markers
}

/// The tracked "where to go next": the first marker of the current stage.
#[must_use]
pub fn primary_marker(quest: &ActiveQuest, tick: u64) -> Option<QuestMarker> {
    let markers = quest_markers(quest, tick);
    markers
        .iter()
        .find(|marker| {
            matches!(
                marker.kind,
                QuestMarkerKind::Spot { next: true, .. }
                    | QuestMarkerKind::Haul
                    | QuestMarkerKind::Beacon
            )
        })
        .or_else(|| markers.first())
        .copied()
}

fn noun(def: &QuestDef, index: u8) -> &'static str {
    def.targets
        .get(usize::from(index))
        .map_or("site", |spec| spec.noun())
}

fn depth(quest: &ActiveQuest, index: u8) -> String {
    quest
        .targets
        .get(usize::from(index))
        .map_or_else(String::new, |target| format!(" ({}m)", target.center.y))
}

/// The world row a depth objective asks for: the row bound at acceptance, or the authored row in
/// quests bound before deep worlds.
#[must_use]
pub const fn depth_row(row: i32, progress: &ObjectiveProgress) -> i32 {
    if progress.anchor > 0 {
        progress.anchor
    } else {
        row
    }
}

/// One line describing an objective and its progress.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one line per primitive keeps the wording in one place"
)]
pub fn objective_line(
    def: &QuestDef,
    quest: &ActiveQuest,
    objective: Objective,
    progress: &ObjectiveProgress,
    owner: Option<&Player>,
    rate: TickRate,
) -> String {
    let seconds = |ticks: u64| whole_seconds(rate, ticks);
    match objective {
        Objective::Deliver { item, count } => {
            let held = owner.map_or(0, |player| item.held(player)).min(count);
            format!("Bring {count} {} ({held}/{count})", item.name())
        }
        Objective::Reach { target } => {
            format!("Reach the {}{}", noun(def, target), depth(quest, target))
        }
        Objective::ReachDepth { row } => format!("Dive to {}m", depth_row(row, progress)),
        Objective::Dig { target, count } => format!(
            "Dig at the {}{} ({}/{count})",
            noun(def, target),
            depth(quest, target),
            progress.count.min(u64::from(count))
        ),
        Objective::Scan { target } => format!(
            "Pulse the scanner near the {}{}",
            noun(def, target),
            depth(quest, target)
        ),
        Objective::OpenPoi { target } => {
            format!("Open the {}{}", noun(def, target), depth(quest, target))
        }
        Objective::Survive {
            target,
            seconds: needed,
        } => format!(
            "Hold out in the {}{} ({}/{needed}s)",
            noun(def, target),
            depth(quest, target),
            seconds(progress.count).min(u64::from(needed))
        ),
        Objective::PickUp { target, parcel } => format!(
            "Pick up the {parcel} at the {}{}",
            noun(def, target),
            depth(quest, target)
        ),
        Objective::ReturnToSurface => {
            if quest.carrying {
                "Bring the parcel up to the surface".to_owned()
            } else {
                "Return to the surface".to_owned()
            }
        }
        Objective::Build { target, kind } => format!(
            "Place a {} at the {}{}",
            kind.name(),
            noun(def, target),
            depth(quest, target)
        ),
        Objective::Detonate { target, spots } => format!(
            "Bomb the marks at the {} in order ({}/{spots})",
            noun(def, target),
            progress.count.min(u64::from(spots))
        ),
        Objective::RecoverLog { target } => format!(
            "Drill the {}'s core{}",
            noun(def, target),
            depth(quest, target)
        ),
        Objective::Simultaneous {
            spots,
            window_ms,
            solo_window_ms,
            ..
        } => {
            let window = if progress.window_ms > 0 {
                progress.window_ms
            } else if quest.crew {
                window_ms
            } else {
                solo_window_ms
            };
            let lit = progress.marks.iter().filter(|mark| **mark > 0).count();
            let marks = if spots == 2 {
                "both marks".to_owned()
            } else {
                format!("all {spots} marks")
            };
            format!(
                "Light {marks} within {}s ({lit} lit)",
                window.div_ceil(1_000)
            )
        }
        Objective::Relay { target, .. } => {
            if progress.carrier.is_some() {
                format!(
                    "Carry the marker to the {}{}",
                    noun(def, target),
                    depth(quest, target)
                )
            } else {
                let at = progress.position.map_or(0, |position| position.y);
                format!("Pick up the marker ({at}m) and carry it on")
            }
        }
        Objective::HoldAndClimb { goal, .. } => format!(
            "Hold the switch while a rig reaches the {}{}",
            noun(def, goal),
            depth(quest, goal)
        ),
        Objective::Escort { .. } => {
            let at = progress.position.map_or(0, |position| position.y);
            format!("Walk the haul up to the surface ({at}m to go)")
        }
        Objective::SplitDelivery {
            item, count, zones, ..
        } => {
            let done = |zone: usize| progress.counts.get(zone).copied().unwrap_or(0).min(count);
            format!(
                "Drop {count} {} at {} ({}/{count}) and {} ({}/{count})",
                item.name(),
                zone_name(zones[0]),
                done(0),
                zone_name(zones[1]),
                done(1)
            )
        }
    }
}

/// Lines for the current stage, each with whether it is done. A ready quest reads "Turn in".
#[must_use]
pub fn current_lines(
    quest: &ActiveQuest,
    owner: Option<&Player>,
    rate: TickRate,
) -> Vec<(String, bool)> {
    let Some(def) = quest.def() else {
        return Vec::new();
    };
    let mut lines: Vec<(String, bool)> = current(def, quest)
        .map(|(objective, progress)| {
            (
                objective_line(def, quest, objective, progress, owner, rate),
                progress.done,
            )
        })
        .collect();
    if quest.ready {
        let last = def.stages.len().saturating_sub(1);
        let start = def.stage_start(last);
        lines = def
            .stages
            .get(last)
            .copied()
            .unwrap_or(&[])
            .iter()
            .copied()
            .zip(quest.objectives.iter().skip(start))
            .filter(|(objective, _)| matches!(objective, Objective::Deliver { .. }))
            .map(|(objective, progress)| {
                (
                    objective_line(def, quest, objective, progress, owner, rate),
                    false,
                )
            })
            .collect();
        lines.push((format!("Turn in to {}", def.giver.name()), false));
    }
    lines
}

/// Seconds left on a timed quest.
#[must_use]
pub fn seconds_left(quest: &ActiveQuest, tick: u64, rate: TickRate) -> Option<u64> {
    quest
        .deadline
        .map(|deadline| whole_seconds(rate, deadline.saturating_sub(tick)))
}

/// The twists in force, described.
#[must_use]
pub fn twist_lines(quest: &ActiveQuest) -> Vec<String> {
    quest
        .def()
        .map(|def| {
            def.active_twists(quest.variant)
                .into_iter()
                .map(Twist::describe)
                .collect()
        })
        .unwrap_or_default()
}

/// `ticks` at `rate`, in whole seconds (rounded up).
#[must_use]
pub fn whole_seconds(rate: TickRate, ticks: u64) -> u64 {
    ticks.div_ceil(u64::from(rate.hz()).max(1))
}
