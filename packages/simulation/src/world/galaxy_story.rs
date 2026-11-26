//! The galaxy story in the tick (Wave 13 story, see [`crate::galaxy_story`]).
//!
//! * **The world pass** ([`WorldState::advance_galaxy_story`], once a tick after space combat
//!   with every rig back from its store): the story begins once space is open; every
//!   [`galaxy_story::CHECK_MILLIS`] of world time the crew's whereabouts are read (each rig
//!   flying, docked, landed or home) and the goals met anywhere are done: systems reached,
//!   worlds scanned and landed on, species met, bosses beaten, the Harbour reached, the Warden
//!   called and calmed; new systems are visited (their leads picked up) and the leads' steps
//!   in flight advance.
//! * **A body's ground pass** ([`WorldState::advance_galaxy_on_ground`]): the goals and lead
//!   steps met on that world's ground (a relic found, a site held while raiders come, a ruin's
//!   mark reached), read from the actors standing there in the body's own frame.
//! * **Commands** ([`crate::galaxy_story::GalaxyCommand`]): set course, ask the archivist, the
//!   finale's choice.
//!
//! Everything is the crew's (one shared [`GalaxyStory`]), so whoever does a goal does it for
//! everyone; whoever finishes a chapter or a lead is paid.

use crate::{
    GameplayEventKind, PlayerId, TickOutput,
    flight::{FlightPlace, StarDrive, nav::NavTarget, space::hash64},
    galaxy_story::{
        self, CHAPTER_COUNT, GalaxyCommand, GalaxyEvent, GalaxyRefusal, GalaxyStory, Goal,
        LeadRecord, LeadStep, MAX_LANDED, MAX_LEADS, MAX_VISITED,
        finale::{self, FinaleChoice},
        journal::{self, RemarkTrigger},
        plan::{self, Harbour, RelicSite, StoryPlan},
    },
    hostiles::{EncounterKind, Faction, ShipClass, encounter},
    place::{BodyId, Place, StoreKey},
    planet::BodyGround,
    position::WorldPos,
    space::{SpaceBodyId, SpaceTile, SystemRef},
    summary::PlayerScopedCommandOutcome,
};

use super::WorldState;

/// Where a crew member is, for the story's checks.
#[derive(Clone, Copy, Debug)]
enum Whereabouts {
    /// On the home world (or the Moon).
    Home,
    /// Away in flight (free, approaching or docked).
    Flying {
        pos: WorldPos,
        system: Option<SystemRef>,
        docked: bool,
        /// In free flight (or an approach), not warping, intact: a fight can find it.
        free: bool,
    },
    /// On a world's ground.
    Landed(SpaceBodyId),
}

impl Whereabouts {
    const fn system(self) -> Option<SystemRef> {
        match self {
            Self::Home => None,
            Self::Flying { system, .. } => system,
            Self::Landed(body) => Some(body.system_ref()),
        }
    }
}

const fn galaxy_event(
    what: GalaxyEvent,
    player_id: Option<PlayerId>,
    detail: u32,
) -> GameplayEventKind {
    GameplayEventKind::Galaxy {
        what,
        player_id,
        detail,
    }
}

/// The column distance between two strip columns of a lap `width` wide.
const fn wrapped(a: i32, b: i32, width: i32) -> i32 {
    if width <= 0 {
        return (a - b).abs();
    }
    let d = (a - b).rem_euclid(width);
    if d > width / 2 { width - d } else { d }
}

impl WorldState {
    /// The crew's galaxy story.
    #[must_use]
    pub const fn galaxy_story(&self) -> &GalaxyStory {
        &self.galaxy_story
    }

    /// Replaces the galaxy story (tests, scenes and tools, before play).
    #[doc(hidden)]
    pub fn set_galaxy_story(&mut self, story: GalaxyStory) {
        self.galaxy_story = story;
    }

    /// The road the story follows, once it has begun (or would follow now, before it has).
    #[must_use]
    pub fn galaxy_plan(&self) -> Option<std::sync::Arc<StoryPlan>> {
        if !self.space_enabled() {
            return None;
        }
        let seed = self.world_seed();
        let anchor = self
            .galaxy_story
            .anchor
            .unwrap_or_else(|| plan::anchor_of(seed, self.star_charts()));
        Some(plan::plan_for(seed, anchor))
    }

    /// The Harbour, once the crew knows where it is (the lamp-key found: from the long dark on).
    #[must_use]
    pub fn galaxy_harbour(&self) -> Option<Harbour> {
        (self.galaxy_story.begun && self.galaxy_story.chapter >= HARBOUR_KNOWN_FROM)
            .then(|| self.galaxy_plan().map(|plan| plan.harbour))
            .flatten()
    }

    /// The names the current chapter's words are filled with.
    #[must_use]
    pub fn galaxy_words(&self, chapter: u8) -> galaxy_story::StopWords {
        self.galaxy_plan()
            .map_or_else(galaxy_story::StopWords::default, |plan| {
                galaxy_story::stop_words(self.world_seed(), &plan, chapter)
            })
    }

    /// The nav target of the current goal (the journal's and the HUD's story waypoint).
    #[must_use]
    pub fn galaxy_waypoint(&self) -> Option<NavTarget> {
        let story = &self.galaxy_story;
        let goal = story.current_goal()?;
        let plan = self.galaxy_plan()?;
        let stop = plan.stop(story.chapter)?;
        let space = self.space();
        let system_target = || {
            space.space().system(stop.system).map(|system| {
                NavTarget::System(crate::flight::space::GeneratedSpace::reference(
                    stop.system.sector,
                    &system,
                ))
            })
        };
        match goal.goal {
            Goal::ReachSystem
            | Goal::BeatBoss(_)
            | Goal::ReachHarbour
            | Goal::CalmWarden
            | Goal::Choose => system_target(),
            Goal::MeetSpecies | Goal::Scan | Goal::Land | Goal::FindRelic | Goal::Hold { .. } => {
                stop.body
                    .and_then(|body| space.body_ref(body))
                    .map(NavTarget::Body)
                    .or_else(system_target)
            }
            // Any people's chapter will do: no one place.
            Goal::SpeciesChapter => None,
        }
    }

    /// The point in flight the current goal is about (a lair, the Harbour's gate), if any.
    #[must_use]
    pub fn galaxy_point(&self) -> Option<WorldPos> {
        let story = &self.galaxy_story;
        let goal = story.current_goal()?;
        let plan = self.galaxy_plan()?;
        match goal.goal {
            Goal::BeatBoss(_) | Goal::ReachHarbour | Goal::CalmWarden | Goal::Choose => plan
                .stop(story.chapter)
                .and_then(|stop| stop.point)
                .map(SpaceTile::pos),
            _ => None,
        }
    }

    /// The relic the current goal looks for: its world, its site and what it is called.
    #[must_use]
    pub fn galaxy_relic(&self) -> Option<(SpaceBodyId, RelicSite, &'static str)> {
        let story = &self.galaxy_story;
        let goal = story.current_goal()?;
        if !matches!(goal.goal, Goal::FindRelic | Goal::Hold { .. } | Goal::Land) {
            return None;
        }
        let chapter = galaxy_story::chapter(story.chapter)?;
        let stop = *self.galaxy_plan()?.stop(story.chapter)?;
        Some((stop.body?, stop.relic?, chapter.relic))
    }

    /// The sites of the crew's open leads on `body` (ruins' marks, for the ground renderer).
    #[must_use]
    pub fn galaxy_lead_sites(&self, body: SpaceBodyId) -> Vec<RelicSite> {
        let seed = self.world_seed();
        self.galaxy_story
            .leads
            .iter()
            .filter(|record| !record.done && record.system == body.system_ref())
            .filter_map(|record| GalaxyStory::lead_of_seed(seed, record))
            .flat_map(|lead| {
                lead.steps
                    .into_iter()
                    .filter_map(|step| match step {
                        LeadStep::FindSite(on, site) if on == body => Some(site),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether the Harbour's space is calm at `pos` (no roaming fights near its gate once the
    /// crew is on its way there).
    #[must_use]
    pub(super) fn galaxy_calm_at(&self, pos: WorldPos) -> bool {
        self.galaxy_harbour().is_some_and(|harbour| {
            harbour.near_gate(SpaceTile::of_pos(pos), plan::HARBOUR_CALM_TILES)
        })
    }

    /// The Warden is beaten (space combat's hook, inside the tick).
    pub(super) fn note_warden_calmed(&mut self, by: Option<PlayerId>, output: &mut TickOutput) {
        if self.galaxy_story.warden_calmed {
            return;
        }
        self.galaxy_story.warden_calmed = true;
        self.galaxy_story.warden_on = None;
        output.push_event(galaxy_event(GalaxyEvent::WardenCalmed, by, 0));
    }

    // --- Commands ----------------------------------------------------------------------------

    /// A command the galaxy story answers.
    pub(super) fn galaxy_command(
        &mut self,
        player_id: PlayerId,
        command: &crate::PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        let crate::PlayerCommand::Galaxy(command) = command else {
            return None;
        };
        Some(self.apply_galaxy_command(player_id, *command, output))
    }

    fn galaxy_refuse(
        player_id: PlayerId,
        why: GalaxyRefusal,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        output.push_event(galaxy_event(
            GalaxyEvent::Refused,
            Some(player_id),
            why.code(),
        ));
        PlayerScopedCommandOutcome::IgnoredUnavailable
    }

    fn apply_galaxy_command(
        &mut self,
        player_id: PlayerId,
        command: GalaxyCommand,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        if !self.galaxy_story.begun {
            return Self::galaxy_refuse(player_id, GalaxyRefusal::NotBegun, output);
        }
        match command {
            GalaxyCommand::SetCourse => {
                let Some(target) = self.galaxy_waypoint() else {
                    return Self::galaxy_refuse(player_id, GalaxyRefusal::NoTarget, output);
                };
                self.flight.waypoints.insert(player_id, target);
                output.push_event(galaxy_event(GalaxyEvent::CourseSet, Some(player_id), 0));
            }
            GalaxyCommand::SetLeadCourse { lead } => {
                let Some(target) = self.lead_waypoint(usize::from(lead)) else {
                    return Self::galaxy_refuse(player_id, GalaxyRefusal::NoTarget, output);
                };
                self.flight.waypoints.insert(player_id, target);
                output.push_event(galaxy_event(GalaxyEvent::CourseSet, Some(player_id), 1));
            }
            GalaxyCommand::AskArchivist => {
                let trigger = if self.galaxy_story.finished() {
                    RemarkTrigger::AfterTheRoad
                } else {
                    RemarkTrigger::Asked
                };
                self.galaxy_remark(trigger, Some(player_id), true, output);
            }
            GalaxyCommand::Choose(choice) => {
                if self.galaxy_story.current_goal().map(|goal| goal.goal) != Some(Goal::Choose) {
                    return Self::galaxy_refuse(player_id, GalaxyRefusal::NotYet, output);
                }
                let at_gate = self.galaxy_plan().is_some_and(|plan| {
                    self.players.get(&player_id).is_some_and(|player| {
                        player.flight.away()
                            && plan.harbour.near_gate(
                                SpaceTile::of_pos(player.flight.pos),
                                finale::CHOICE_REACH_TILES,
                            )
                    })
                });
                if !at_gate {
                    return Self::galaxy_refuse(player_id, GalaxyRefusal::NotAtGate, output);
                }
                self.make_choice(player_id, choice, output);
            }
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// The nav target of lead `index` of the discovery log: its current step's world or
    /// system.
    fn lead_waypoint(&self, index: usize) -> Option<NavTarget> {
        let record = *self.galaxy_story.leads.get(index)?;
        if record.done {
            return None;
        }
        let lead = GalaxyStory::lead_of_seed(self.world_seed(), &record)?;
        let space = self.space();
        let step = lead.steps.get(usize::from(record.step))?;
        if let Some(body) = step.body()
            && let Some(found) = space.body_ref(body)
        {
            return Some(NavTarget::Body(found));
        }
        let system = space.space().system(record.system)?;
        Some(NavTarget::System(
            crate::flight::space::GeneratedSpace::reference(record.system.sector, &system),
        ))
    }

    fn make_choice(&mut self, player_id: PlayerId, choice: FinaleChoice, output: &mut TickOutput) {
        let now = self.world_millis();
        self.galaxy_story.choice = Some(choice);
        self.galaxy_story.finale_millis = Some(now);
        output.push_event(galaxy_event(
            GalaxyEvent::Chosen,
            Some(player_id),
            choice.code(),
        ));
        self.complete_goal(Some(player_id), output);
    }

    // --- The world pass ------------------------------------------------------------------------

    /// The galaxy story's world pass (see the module docs).
    pub(super) fn advance_galaxy_story(&mut self, output: &mut TickOutput) {
        if !self.space_enabled() {
            return;
        }
        let now = self.world_millis();
        if !self.galaxy_story.begun {
            if !self.heart.space_unlocked {
                return;
            }
            self.begin_galaxy_story(now, output);
        }
        if now < self.galaxy_story.next_check_millis {
            return;
        }
        self.galaxy_story.next_check_millis =
            (now / galaxy_story::CHECK_MILLIS + 1) * galaxy_story::CHECK_MILLIS;
        let crew = self.crew_whereabouts();
        self.visit_systems(&crew, now, output);
        self.advance_story_goals(&crew, output);
        self.advance_leads_in_flight(&crew, output);
    }

    fn begin_galaxy_story(&mut self, now: u64, output: &mut TickOutput) {
        let anchor = plan::anchor_of(self.world_seed(), self.star_charts());
        self.galaxy_story.begun = true;
        self.galaxy_story.anchor = Some(anchor);
        self.galaxy_story.chapter = 0;
        self.galaxy_story.step = 0;
        self.galaxy_story.next_check_millis = now;
        output.push_event(galaxy_event(GalaxyEvent::Begun, None, 0));
        output.push_event(galaxy_event(GalaxyEvent::ChapterBegun, None, 0));
        self.galaxy_remark(RemarkTrigger::ChapterBegun(0), None, true, output);
        if self.flight.crew_waypoint.is_none() {
            self.flight.crew_waypoint = self.galaxy_waypoint();
        }
    }

    /// Where every crew member is.
    fn crew_whereabouts(&self) -> Vec<(PlayerId, Whereabouts)> {
        let space = self.space();
        self.players
            .iter()
            .map(|(id, player)| {
                let place = self.place_of(*id);
                let where_ = match place {
                    Place::OnBody(BodyId::Space(body)) => Whereabouts::Landed(body),
                    _ if player.flight.away() => {
                        let tile = SpaceTile::of_pos(player.flight.pos);
                        let system = space
                            .space()
                            .systems_near(tile, 0)
                            .first()
                            .map(|(system, _)| *system);
                        Whereabouts::Flying {
                            pos: player.flight.pos,
                            system,
                            docked: matches!(player.flight.place, FlightPlace::Docked(_)),
                            free: matches!(
                                player.flight.place,
                                FlightPlace::Space | FlightPlace::Approach(_)
                            ) && !matches!(player.flight.drive, StarDrive::Warp(_))
                                && player.hull > 0.0,
                        }
                    }
                    _ => Whereabouts::Home,
                };
                (*id, where_)
            })
            .collect()
    }

    /// Systems the crew enters for the first time (their leads picked up) and worlds set down
    /// on.
    fn visit_systems(
        &mut self,
        crew: &[(PlayerId, Whereabouts)],
        now: u64,
        output: &mut TickOutput,
    ) {
        let seed = self.world_seed();
        for (player, where_) in crew {
            if let Whereabouts::Landed(body) = where_
                && self.galaxy_story.landed.len() < MAX_LANDED
            {
                self.galaxy_story.landed.insert(*body);
            }
            let Some(system) = where_.system() else {
                continue;
            };
            if self.galaxy_story.visited.contains(&system)
                || self.galaxy_story.visited.len() >= MAX_VISITED
            {
                continue;
            }
            self.galaxy_story.visited.insert(system);
            self.galaxy_story.crew.entry(*player).or_default().systems += 1;
            let mut found_any = false;
            for lead in galaxy_story::leads_of(seed, system) {
                if self.galaxy_story.leads.len() >= MAX_LEADS {
                    break;
                }
                self.galaxy_story.leads.push(LeadRecord {
                    system,
                    index: lead.index,
                    step: 0,
                    done: false,
                    found_millis: now,
                    ambushed: None,
                });
                let index = u32::try_from(self.galaxy_story.leads.len() - 1).unwrap_or(u32::MAX);
                output.push_event(galaxy_event(GalaxyEvent::LeadFound, Some(*player), index));
                found_any = true;
            }
            if found_any {
                self.galaxy_remark(RemarkTrigger::LeadFound, Some(*player), false, output);
            }
        }
    }

    /// The story goal under way, met anywhere but a world's ground.
    fn advance_story_goals(&mut self, crew: &[(PlayerId, Whereabouts)], output: &mut TickOutput) {
        // A few goals can fall at once (a crew already there); never more than the story has.
        for _ in 0..8 {
            let Some(goal) = self.galaxy_story.current_goal().map(|goal| goal.goal) else {
                return;
            };
            let Some(plan) = self.galaxy_plan() else {
                return;
            };
            let Some(stop) = plan.stop(self.galaxy_story.chapter).copied() else {
                return;
            };
            let by: Option<Option<PlayerId>> = match goal {
                Goal::ReachSystem => crew
                    .iter()
                    .find(|(_, where_)| where_.system() == Some(stop.system))
                    .map(|(id, _)| Some(*id)),
                Goal::Scan => stop
                    .body
                    .filter(|body| self.stations.scanned.contains(body))
                    .map(|_| None),
                Goal::Land => crew
                    .iter()
                    .find(|(_, where_)| {
                        matches!(where_, Whereabouts::Landed(body) if Some(*body) == stop.body)
                    })
                    .map(|(id, _)| Some(*id)),
                Goal::MeetSpecies => self
                    .aliens
                    .players
                    .iter()
                    .find(|(_, log)| {
                        log.species
                            .values()
                            .any(|species| species.met_millis.is_some())
                    })
                    .map(|(id, _)| Some(*id)),
                Goal::SpeciesChapter => self
                    .aliens
                    .players
                    .iter()
                    .find(|(_, log)| log.species.values().any(|species| species.chapters_done > 0))
                    .map(|(id, _)| Some(*id)),
                Goal::BeatBoss(faction) => self
                    .hostiles
                    .bosses_beaten
                    .contains(&faction)
                    .then_some(None),
                Goal::ReachHarbour => crew
                    .iter()
                    .find(|(_, where_)| {
                        matches!(where_, Whereabouts::Flying { pos, .. }
                            if plan.harbour.near_gate(
                                SpaceTile::of_pos(*pos),
                                plan::HARBOUR_REACH_TILES,
                            ))
                    })
                    .map(|(id, _)| Some(*id)),
                Goal::CalmWarden => self.advance_warden(&plan.harbour, crew, output),
                // On a world's ground, or by command.
                Goal::FindRelic | Goal::Hold { .. } | Goal::Choose => None,
            };
            let Some(by) = by else {
                return;
            };
            self.complete_goal(by, output);
        }
    }

    /// The Warden: called on a rig near the gate, again after a flight; calmed by the fight
    /// (space combat's hook). A peaceful world's Warden knows the key at once.
    #[allow(
        clippy::option_option,
        reason = "None: not calm yet; Some(None): calm, by nobody in particular"
    )]
    fn advance_warden(
        &mut self,
        harbour: &Harbour,
        crew: &[(PlayerId, Whereabouts)],
        output: &mut TickOutput,
    ) -> Option<Option<PlayerId>> {
        if self.galaxy_story.warden_calmed {
            return Some(self.galaxy_story.warden_on.take());
        }
        let near = crew.iter().find(|(_, where_)| {
            matches!(where_, Whereabouts::Flying { pos, free: true, .. }
                if harbour.near_gate(SpaceTile::of_pos(*pos), finale::WARDEN_REACH_TILES))
        });
        if !self.enemy_difficulty().hostile() {
            // No fights on a peaceful world: the Warden sees the key and stands aside.
            return near.map(|(id, _)| {
                self.galaxy_story.warden_calmed = true;
                output.push_event(galaxy_event(GalaxyEvent::WardenCalmed, Some(*id), 0));
                Some(*id)
            });
        }
        let present = self
            .hostiles
            .ships
            .iter()
            .any(|ship| ship.class == ShipClass::Warden);
        if present {
            return None;
        }
        // Not here (never called, or the rig fled or was towed home and its fight ended).
        self.galaxy_story.warden_on = None;
        let (player, _) = near?;
        // Not on a rig already fighting, nor one resting after a fight or an escape jump (the
        // jump is shorter than the gate's reach: it must not land back in the fight).
        let step = self.combat_step_now();
        if self.hostiles.engaged(*player).is_some() || step < self.rig_combat(*player).next_roll {
            return None;
        }
        let ring = self
            .players
            .get(player)
            .map_or(1, |rig| encounter::danger_ring(rig.flight.pos));
        if self
            .start_encounter(
                *player,
                Faction::Machine,
                EncounterKind::Boss,
                &[ShipClass::Warden],
                ring.min(8),
                step,
                output,
            )
            .is_some()
        {
            self.galaxy_story.warden_on = Some(*player);
            output.push_event(galaxy_event(GalaxyEvent::WardenAppeared, Some(*player), 0));
        }
        None
    }

    /// Marks the current goal done by `by` (the crew's), closing the chapter after its last.
    fn complete_goal(&mut self, by: Option<PlayerId>, output: &mut TickOutput) {
        let Some(chapter) = galaxy_story::chapter(self.galaxy_story.chapter) else {
            return;
        };
        let old_target = self.galaxy_waypoint();
        let story = &mut self.galaxy_story;
        let index = story.chapter;
        output.push_event(galaxy_event(
            GalaxyEvent::GoalDone,
            by,
            u32::from(index) * 16 + u32::from(story.step),
        ));
        if let Some(player) = by {
            story.crew.entry(player).or_default().goals += 1;
        }
        story.step += 1;
        story.held_millis = 0;
        story.held_seen = None;
        story.raid_landed = false;
        if usize::from(story.step) >= chapter.goals.len() {
            story.step = 0;
            story.chapter += 1;
            let now = self.world_millis();
            self.galaxy_story.chapter_millis.push(now);
            if let Some(player) = by.or_else(|| self.players.keys().next().copied())
                && let Some(rig) = self.players.get_mut(&player)
            {
                rig.credits = rig.credits.saturating_add(chapter.credits);
            }
            for log in chapter.logs {
                if self.galaxy_story.logs.insert(*log) {
                    output.push_event(galaxy_event(GalaxyEvent::LogFound, by, u32::from(*log)));
                }
            }
            output.push_event(galaxy_event(GalaxyEvent::ChapterDone, by, u32::from(index)));
            let next = self.galaxy_story.chapter;
            if next < CHAPTER_COUNT {
                output.push_event(galaxy_event(GalaxyEvent::ChapterBegun, by, u32::from(next)));
                self.galaxy_remark(RemarkTrigger::ChapterBegun(next), by, true, output);
            } else {
                self.galaxy_remark(RemarkTrigger::AfterTheRoad, by, true, output);
            }
        }
        // The crew's waypoint follows the story while it pointed at the story.
        let new_target = self.galaxy_waypoint();
        if self.flight.crew_waypoint.is_none() || self.flight.crew_waypoint == old_target {
            self.flight.crew_waypoint = new_target;
        }
    }

    /// The archivist remarks on `trigger` (at most every [`galaxy_story::REMARK_GAP_MILLIS`]
    /// unless `always`).
    fn galaxy_remark(
        &mut self,
        trigger: RemarkTrigger,
        player: Option<PlayerId>,
        always: bool,
        output: &mut TickOutput,
    ) {
        let now = self.world_millis();
        let story = &mut self.galaxy_story;
        if !always
            && story.remarks > 0
            && now < story.last_remark_millis + galaxy_story::REMARK_GAP_MILLIS
        {
            return;
        }
        let Some(remark) = journal::remark_for(trigger, story.chapters_done(), story.remarks)
        else {
            return;
        };
        story.remarks = story.remarks.wrapping_add(1);
        story.last_remark_millis = now;
        output.push_event(galaxy_event(
            GalaxyEvent::ArchivistRemark,
            player,
            u32::from(remark),
        ));
    }

    // --- Leads -------------------------------------------------------------------------------

    /// Lead steps met in flight or anywhere (not on a world's ground).
    fn advance_leads_in_flight(
        &mut self,
        crew: &[(PlayerId, Whereabouts)],
        output: &mut TickOutput,
    ) {
        let seed = self.world_seed();
        for index in 0..self.galaxy_story.leads.len() {
            let record = self.galaxy_story.leads[index];
            if record.done {
                continue;
            }
            // Only leads of systems someone is in (or a dock anywhere) can move.
            let Some(lead) = GalaxyStory::lead_of_seed(seed, &record) else {
                continue;
            };
            let Some(step) = lead.steps.get(usize::from(record.step)).copied() else {
                continue;
            };
            let in_system = |where_: &Whereabouts| where_.system() == Some(record.system);
            let by: Option<PlayerId> = match step {
                LeadStep::ReachPoint(point) => crew
                    .iter()
                    .find(|(_, where_)| {
                        matches!(where_, Whereabouts::Flying { pos, .. }
                        if point.within(
                            SpaceTile::of_pos(*pos),
                            galaxy_story::discovery::LEAD_POINT_REACH_TILES,
                        ))
                    })
                    .map(|(id, _)| *id),
                LeadStep::Survive(_) => self.survive_step(index, record),
                LeadStep::Dock => crew
                    .iter()
                    .find(|(_, where_)| matches!(where_, Whereabouts::Flying { docked: true, .. }))
                    .map(|(id, _)| *id),
                LeadStep::Land(body) => crew
                    .iter()
                    .find(|(_, where_)| matches!(where_, Whereabouts::Landed(on) if *on == body))
                    .map(|(id, _)| *id),
                LeadStep::Scan(count) => {
                    let scanned = self
                        .stations
                        .scanned
                        .iter()
                        .filter(|body| body.system_ref() == record.system)
                        .count();
                    // Paid to whoever is there (else the first of the crew).
                    (scanned >= usize::from(count))
                        .then(|| {
                            crew.iter()
                                .find(|(_, where_)| in_system(where_))
                                .or_else(|| crew.first())
                        })
                        .flatten()
                        .map(|(id, _)| *id)
                }
                LeadStep::MeetFolk(_, species) => self
                    .aliens
                    .players
                    .iter()
                    .find(|(_, log)| log.of(species).met_millis.is_some())
                    .map(|(id, _)| *id),
                // On the world's ground.
                LeadStep::FindSite(..) => None,
            };
            if let Some(player) = by {
                self.advance_lead(index, player, crew, output);
            }
        }
    }

    /// A distress call's ambush: done once its fight is over with the rig still flying; begun
    /// again (back to the beacon) if the rig was lost.
    fn survive_step(&mut self, index: usize, record: LeadRecord) -> Option<PlayerId> {
        let player = record.ambushed?;
        if self.hostiles.engaged(player).is_some() {
            return None;
        }
        let alive = self
            .players
            .get(&player)
            .is_some_and(|rig| rig.flight.away() && rig.hull > 0.0);
        if alive {
            return Some(player);
        }
        // Lost: the beacon waits for another try.
        let record = &mut self.galaxy_story.leads[index];
        record.step = record.step.saturating_sub(1);
        record.ambushed = None;
        None
    }

    /// Advances lead `index` past its step, done by `player`.
    fn advance_lead(
        &mut self,
        index: usize,
        player: PlayerId,
        crew: &[(PlayerId, Whereabouts)],
        output: &mut TickOutput,
    ) {
        let seed = self.world_seed();
        let record = self.galaxy_story.leads[index];
        let Some(lead) = GalaxyStory::lead_of_seed(seed, &record) else {
            return;
        };
        let detail = u32::try_from(index).unwrap_or(u32::MAX);
        let next = record.step + 1;
        self.galaxy_story.leads[index].step = next;
        self.galaxy_story.leads[index].ambushed = None;
        if usize::from(next) >= lead.steps.len() {
            self.galaxy_story.leads[index].done = true;
            self.galaxy_story.crew.entry(player).or_default().leads += 1;
            if let Some(rig) = self.players.get_mut(&player) {
                rig.credits = rig.credits.saturating_add(lead.credits);
            }
            if self.galaxy_story.fragments.insert(lead.fragment) {
                output.push_event(galaxy_event(
                    GalaxyEvent::FragmentFound,
                    Some(player),
                    u32::from(lead.fragment),
                ));
            }
            output.push_event(galaxy_event(GalaxyEvent::LeadDone, Some(player), detail));
            self.galaxy_remark(RemarkTrigger::LeadDone, Some(player), false, output);
            return;
        }
        output.push_event(galaxy_event(GalaxyEvent::LeadStep, Some(player), detail));
        // An ambush springs on the rig that reached the beacon (none on a peaceful world).
        if let Some(LeadStep::Survive(faction)) = lead.steps.get(usize::from(next)).copied() {
            let flying = crew.iter().any(|(id, where_)| {
                *id == player && matches!(where_, Whereabouts::Flying { free: true, .. })
            });
            if !self.enemy_difficulty().hostile() || !flying {
                // Nothing comes (or nobody to meet it): the crew it was circling slips away.
                self.advance_lead(index, player, crew, output);
                return;
            }
            let ring = self
                .players
                .get(&player)
                .map_or(1, |rig| encounter::danger_ring(rig.flight.pos));
            let step = self.combat_step_now();
            let tier = self.pacing_tier(player);
            let roll = hash64(
                seed,
                0x6A1A_D157,
                i64::from(record.index),
                step.cast_signed(),
            );
            let ships = encounter::compose(faction, encounter::budget(ring, tier), roll);
            if self
                .start_encounter(
                    player,
                    faction,
                    EncounterKind::Roaming,
                    &ships,
                    ring,
                    step,
                    output,
                )
                .is_some()
            {
                self.galaxy_story.leads[index].ambushed = Some(player);
            } else {
                self.advance_lead(index, player, crew, output);
            }
        }
    }

    // --- A world's ground ----------------------------------------------------------------------

    /// The goals and lead steps met on the ground of the body whose pass is running.
    pub(super) fn advance_galaxy_on_ground(&mut self, output: &mut TickOutput) {
        if !self.galaxy_story.begun {
            return;
        }
        let StoreKey::Body(body) = self.active_store() else {
            return;
        };
        self.advance_relic_goal(body, output);
        self.advance_lead_sites(body, output);
    }

    /// The actors on this ground and their strip tiles.
    fn actors_here(&self) -> Vec<(PlayerId, i32, i32)> {
        self.players
            .keys()
            .filter_map(|id| self.actor_tile(*id).map(|tile| (*id, tile.x, tile.y)))
            .collect()
    }

    /// The row and lap width of `site` on this ground.
    fn site_row(&self, site: RelicSite) -> Option<(i32, i32, i32)> {
        let ground = self.terrain.planet_ground()?;
        let column = i32::try_from(site.column).ok()?;
        let surface = ground.surface_row(column);
        Some((column, surface + i32::from(site.rows), ground.strip_width()))
    }

    /// Who stands at `site` (within reach of its column and row).
    fn at_site(&self, site: RelicSite, columns: i32) -> Option<PlayerId> {
        let (column, row, width) = self.site_row(site)?;
        let (above, below) = galaxy_story::RELIC_REACH_ROWS;
        self.actors_here()
            .into_iter()
            .find(|(_, x, y)| {
                wrapped(*x, column, width) <= columns && *y >= row - above && *y <= row + below
            })
            .map(|(id, ..)| id)
    }

    fn advance_relic_goal(&mut self, body: SpaceBodyId, output: &mut TickOutput) {
        let Some(goal) = self.galaxy_story.current_goal().map(|goal| goal.goal) else {
            return;
        };
        if !goal.on_ground() {
            return;
        }
        let Some((relic_body, site, _)) = self.galaxy_relic() else {
            return;
        };
        if relic_body != body {
            return;
        }
        let chapter = self.galaxy_story.chapter;
        match goal {
            Goal::FindRelic => {
                if let Some(player) = self.at_site(site, galaxy_story::RELIC_REACH_COLUMNS) {
                    output.push_event(galaxy_event(
                        GalaxyEvent::RelicFound,
                        Some(player),
                        u32::from(chapter),
                    ));
                    self.galaxy_remark(
                        RemarkTrigger::RelicFound(chapter),
                        Some(player),
                        true,
                        output,
                    );
                    self.complete_goal(Some(player), output);
                }
            }
            Goal::Hold { seconds, raiders } => {
                let now = self.world_millis();
                let Some(player) = self.at_site(site, galaxy_story::HOLD_REACH_COLUMNS) else {
                    self.galaxy_story.held_seen = None;
                    return;
                };
                if !self.galaxy_story.raid_landed {
                    self.galaxy_story.raid_landed = true;
                    output.push_event(galaxy_event(
                        GalaxyEvent::HoldBegun,
                        Some(player),
                        raiders.code(),
                    ));
                    self.land_hold_raid(body, site, raiders, output);
                }
                if let Some(seen) = self.galaxy_story.held_seen {
                    let gained = now.saturating_sub(seen).min(500);
                    self.galaxy_story.held_millis = self
                        .galaxy_story
                        .held_millis
                        .saturating_add(u32::try_from(gained).unwrap_or(0));
                }
                self.galaxy_story.held_seen = Some(now);
                if self.galaxy_story.held_millis >= u32::from(seconds) * 1_000 {
                    self.complete_goal(Some(player), output);
                }
            }
            _ => {}
        }
    }

    /// The raiders that come for a held site (none on a peaceful world).
    fn land_hold_raid(
        &mut self,
        body: SpaceBodyId,
        site: RelicSite,
        faction: Faction,
        output: &mut TickOutput,
    ) {
        if !self.enemy_difficulty().hostile() {
            return;
        }
        let Some((column, row, _)) = self.site_row(site) else {
            return;
        };
        let ring = self
            .space()
            .space()
            .body(body)
            .map_or(2, |found| encounter::danger_ring(found.centre.pos()));
        let step = self.combat_step_now();
        let roll = hash64(
            self.world_seed(),
            0x6A1A_401D,
            i64::from(column),
            step.cast_signed(),
        );
        self.land_raid(
            StoreKey::Body(body),
            crate::hostiles::raid::RaidSite { column, row },
            faction,
            ring.max(3),
            roll,
            step,
            output,
        );
    }

    fn advance_lead_sites(&mut self, body: SpaceBodyId, output: &mut TickOutput) {
        let seed = self.world_seed();
        for index in 0..self.galaxy_story.leads.len() {
            let record = self.galaxy_story.leads[index];
            if record.done || record.system != body.system_ref() {
                continue;
            }
            let Some(lead) = GalaxyStory::lead_of_seed(seed, &record) else {
                continue;
            };
            let Some(LeadStep::FindSite(on, site)) =
                lead.steps.get(usize::from(record.step)).copied()
            else {
                continue;
            };
            if on != body {
                continue;
            }
            if let Some(player) = self.at_site(site, galaxy_story::RELIC_REACH_COLUMNS) {
                self.advance_lead(index, player, &[], output);
            }
        }
    }

    // --- Test mode -----------------------------------------------------------------------------

    /// Test mode: moves `player_id` to where the current story goal is done (beside the stop's
    /// world or its star, over the world's relic, the Harbour's gate).
    pub(super) fn warp_to_story(&mut self, player_id: PlayerId) -> PlayerScopedCommandOutcome {
        let Some(goal) = self.galaxy_story.current_goal().map(|goal| goal.goal) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(plan) = self.galaxy_plan() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(stop) = plan.stop(self.galaxy_story.chapter).copied() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let space = self.space();
        if let (Goal::Land | Goal::FindRelic | Goal::Hold { .. }, Some(body)) = (goal, stop.body) {
            let column = stop.relic.map_or(0, |site| site.column);
            let rows = stop.relic.map_or(0, |site| site.rows);
            self.warp_onto_body(player_id, body, column, rows)
        } else {
            {
                let point = match goal {
                    Goal::ReachHarbour | Goal::CalmWarden | Goal::Choose => {
                        Some(plan.harbour.gate.offset(0, 6_000))
                    }
                    _ => stop.point,
                }
                .or_else(|| {
                    // Beside the stop's world, else between the star and its first orbit.
                    stop.body
                        .and_then(|body| space.space().body(body))
                        .map(|found| found.centre.offset(0, -i64::from(found.radius) - 3_000))
                        .or_else(|| {
                            space
                                .space()
                                .system(stop.system)
                                .map(|system| system.centre.offset(0, -60_000))
                        })
                });
                let Some(point) = point else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                self.warp_into_flight(player_id, point.pos())
            }
        }
    }

    /// Test mode: `player_id`'s rig into free flight at `pos`, still.
    pub(super) fn warp_into_flight(
        &mut self,
        player_id: PlayerId,
        pos: WorldPos,
    ) -> PlayerScopedCommandOutcome {
        let outcome = self.warp_to_place(player_id, Place::Space);
        if outcome != PlayerScopedCommandOutcome::Applied {
            return outcome;
        }
        if let Some(rig) = self.players.get_mut(&player_id) {
            rig.flight.place = FlightPlace::Space;
            rig.flight.pos = pos;
            rig.flight.velocity_x = 0.0;
            rig.flight.velocity_y = 0.0;
            rig.flight.drive = StarDrive::Off;
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Test mode: `player_id` set down on `body` over strip `column`, `rows` under its surface
    /// (in a pocket dug for it: the warp does not dig, it stands the rig on the surface above).
    fn warp_onto_body(
        &mut self,
        player_id: PlayerId,
        body: SpaceBodyId,
        column: u32,
        rows: u16,
    ) -> PlayerScopedCommandOutcome {
        let Some(params) = crate::place::planet_params_for(self.world_seed(), body) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let ground = crate::planet::PlanetGround::new(params);
        let column = i32::try_from(column).unwrap_or(0);
        let surface = ground.surface_row(column);
        let _ = rows;
        let place = Place::OnBody(BodyId::Space(body));
        let outcome = self.warp_to_place(player_id, place);
        if outcome != PlayerScopedCommandOutcome::Applied {
            return outcome;
        }
        let position = super::strip_position(f64::from(column) + 0.5, f64::from(surface - 2));
        match self.move_to_place(player_id, place, position) {
            Ok(()) => PlayerScopedCommandOutcome::Applied,
            Err(_) => PlayerScopedCommandOutcome::IgnoredUnavailable,
        }
    }
}

/// From this chapter on (the long dark) the crew knows where the Harbour is.
pub const HARBOUR_KNOWN_FROM: u8 = 7;
