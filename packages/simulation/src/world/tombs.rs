//! Tomb and crypt runs in the world (WP-TOMBS; data and rules in [`crate::tomb`]).
//!
//! * **Seals and entering** (the use button at a [`BuiltTile::TombDoor`]): a sealed door breaks
//!   for its [`SiteOpening`] (a tomb key in the pack or the rig, used up; a story step: the
//!   player has seen a story arc through; explosives: a blast beside the door,
//!   [`WorldState::blast_tomb_seals`]); [`WorldState::unseal_tomb_site`] is the entry point for
//!   story arcs that open a site. An open door starts a run for the user and every crewmate
//!   on foot within [`PARTY_REACH`]: the party is moved into a free run slot of the vault
//!   ([`crate::worldgen::tomb_sites::slot_origin`]), the rig waits outside (it takes no damage
//!   while its pilot is in a run).
//! * **Floors** ([`WorldState::write_tomb_floor`]): the plan for the site, its run counter and
//!   the floor is realized for the site's style and written into the slot (the last floor's
//!   cells put back to bedrock first); guards spawn at once, ambushes and the guardian when a
//!   member walks into their room. The pilot's view ([`PilotView`]) follows the style.
//! * **The run's tick** ([`WorldState::advance_tombs`]): ambushes, the guardian, traps, revives
//!   (a crewmate standing beside a downed member for [`REVIVE_MILLIS`]), a party wipe, and relic
//!   picks (when every member has picked, or after [`PICK_TIMEOUT_MILLIS`], the next floor).
//! * **Fixtures** (the use button): caches add unbanked loot (sometimes a tomb key or a lore
//!   page), a shrine banks everyone's loot, revives the downed, heals once and saves the run
//!   (the game saves on [`GameplayEventKind::TombShrineRested`]), the stairs end the floor (the
//!   last floor's only once the guardian is down, and then the run is won), and the first
//!   floor's way out ends the run with everything banked.
//! * **Death** ([`WorldState::down_tomb_member`]): a member at zero health is downed instead of
//!   dying; when every member is down the run ends and the unbanked loot is lost.

#![allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "world pixels and small spawn indices convert freely here"
)]

use crate::{
    base::BuiltTile,
    entity::{EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    ids::{PlayerId, SimulationTick},
    pilot::{PilotBoons, PilotMode, PilotView},
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        LORE_PAGES, RunMember, RunOutcome, RunPhase, RunSummary, SiteOpening, SiteStyle, TOMB_KEY,
        TombCommand, TombRun, TombSite,
        floor::plan_floor,
        realize::{
            RealizedFloor, RealizedRoom, SpawnRole, SpawnSlot, TrapKind, bestiary,
            boss_health_scale, realizer_for,
        },
        relic::{RelicEffects, offers},
    },
    worldgen::{features::TileRect, noise::hash2, tomb_sites},
};

use super::{WorldState, bombs::Blast};

/// Crewmates on foot within this distance of the door (px) go in with the opener.
pub const PARTY_REACH: f32 = TILE_SIZE * 8.0;
/// A crewmate this close to a downed member (px, middle to middle) revives them...
pub const REVIVE_REACH: f32 = TILE_SIZE * 1.8;
/// ...after standing there this long.
pub const REVIVE_MILLIS: u64 = 2_500;
/// Share of max health a revived member gets back.
pub const REVIVE_SHARE: f32 = 0.4;
/// Relic picks wait this long for everyone before passing for the slow.
pub const PICK_TIMEOUT_MILLIS: u64 = 90_000;
/// A blast this close to a sealed explosives door (px) cracks it.
pub const SEAL_BLAST_REACH: f32 = TILE_SIZE * 3.0;
/// Highest enemy tier a run spawns.
const MAX_ENEMY_TIER: u8 = 8;

/// The depth tier (health and damage scale) of a run's creatures: one below the site's tier,
/// plus one every two floors down. Tomb creatures fight pilots, not rigs, so they sit well
/// under the band tiers of the strata around them.
#[must_use]
pub fn enemy_tier(run: &TombRun) -> u8 {
    run.site
        .tier
        .saturating_sub(1)
        .saturating_add(run.floor / 2)
        .min(MAX_ENEMY_TIER)
}

const SALT_CACHE: u64 = 0x70B5_CAC4_000A;
const SALT_KEY: u64 = 0x70B5_CAC4_000B;

/// The loot value of a cache at `tile` on floor `floor` of a tier-`tier` run (before relics).
#[must_use]
pub fn cache_value(seed: u64, tier: u8, floor: u8, tile: TilePosition) -> u32 {
    let roll = u32::try_from(hash2(seed, SALT_CACHE, tile.x, tile.y) % 40).unwrap_or(0);
    let tier = u32::from(tier.max(1));
    (30 + 25 * u32::from(floor) + roll) * tier * tier
}

/// The guardian's hoard: paid to every member when the run is won.
#[must_use]
pub const fn guardian_hoard(tier: u8) -> u32 {
    let tier = tier as u32;
    300 * tier * tier
}

/// Whether a V10 ruin chest at `tile` also holds a tomb key (one in five).
#[must_use]
pub const fn chest_holds_tomb_key(seed: u64, tile: TilePosition) -> bool {
    hash2(seed, SALT_KEY, tile.x, tile.y).is_multiple_of(5)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "event positions are whole world pixels"
)]
const fn px(value: f32) -> i32 {
    value as i32
}

const fn offset(tile: TilePosition, by: TilePosition) -> TilePosition {
    TilePosition {
        x: tile.x + by.x,
        y: tile.y + by.y,
    }
}

const fn offset_rect(rect: TileRect, by: TilePosition) -> TileRect {
    TileRect {
        min: offset(rect.min, by),
        max: offset(rect.max, by),
    }
}

const fn view_of(style: SiteStyle) -> PilotView {
    match style {
        SiteStyle::SideView => PilotView::Side,
        SiteStyle::TopDown => PilotView::TopDown,
    }
}

impl WorldState {
    // --- Queries ----------------------------------------------------------------------------

    /// Every run, site counter, seal and unlock ([`crate::tomb::Tombs`]).
    #[must_use]
    pub const fn tombs(&self) -> &crate::tomb::Tombs {
        &self.tombs
    }

    /// The run `player` is in, if any.
    #[must_use]
    pub fn tomb_run_of(&self, player: PlayerId) -> Option<&TombRun> {
        self.tombs.run_of(player)
    }

    /// Whether `tile` is inside the run vault (a run space: no cave water, critters, heat or
    /// cave-ins; enemies there are always hostile).
    #[must_use]
    pub const fn in_run_space(&self, tile: TilePosition) -> bool {
        tomb_sites::in_vault(self.terrain.settings(), tile)
    }

    /// The site whose door is at `tile`, if any.
    #[must_use]
    pub fn tomb_site_at_door(&self, tile: TilePosition) -> Option<TombSite> {
        tomb_sites::site_with_door(self.terrain.settings(), tile)
    }

    /// The tomb site whose door `player`'s pilot could use now (the prompt shows its style).
    #[must_use]
    pub fn tomb_door_in_reach(&self, player: PlayerId) -> Option<TombSite> {
        match self.pilot_usable(player)? {
            (tile, crate::mechanism::Usable::TombDoor { .. }) => self.tomb_site_at_door(tile),
            _ => None,
        }
    }

    /// The realized floor `run` is on (pure: recomputed from the site and counters).
    #[must_use]
    pub fn tomb_floor_of(run: &TombRun) -> RealizedFloor {
        realizer_for(run.site.style).realize(&plan_floor(&run.site, run.run, run.floor))
    }

    /// Damage taken scale for `player` (relics); `None` (no damage at all) while downed, and
    /// for the whole party while it picks relics on the stairs (the pick screen is up, and a
    /// partner may still be choosing).
    pub(super) fn tomb_damage_taken(&self, player: PlayerId) -> Option<f32> {
        let Some(run) = self.tombs.run_of(player) else {
            return Some(1.0);
        };
        match run.member(player) {
            _ if run.phase == RunPhase::Picking => None,
            Some(member) if member.is_downed() => None,
            Some(member) => Some(RelicEffects::scale(member.effects().taken_pct)),
            None => Some(1.0),
        }
    }

    /// Whether `player` is downed in a run.
    #[must_use]
    pub fn tomb_downed(&self, player: PlayerId) -> bool {
        self.tombs
            .member_of(player)
            .is_some_and(RunMember::is_downed)
    }

    // --- Commands ---------------------------------------------------------------------------

    /// [`crate::PlayerCommand::Tomb`]. `None`: not a tomb command.
    pub(super) fn tomb_command(
        &mut self,
        player: PlayerId,
        command: &crate::PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        let crate::PlayerCommand::Tomb(command) = command else {
            return None;
        };
        Some(match *command {
            TombCommand::PickRelic(choice) => self.pick_relic(player, choice, output),
        })
    }

    /// Commands a member in a run may not use: the radio pickup and the rig recall (the way
    /// out is the stairs or the first floor's exit).
    pub(super) fn tomb_blocks(&self, player: PlayerId, command: &crate::PlayerCommand) -> bool {
        if self.tombs.run_of(player).is_none() {
            return false;
        }
        matches!(
            command,
            crate::PlayerCommand::Rescue
                | crate::PlayerCommand::TogglePilot
                | crate::PlayerCommand::Pilot(
                    crate::pilot::PilotAction::RecallRig | crate::pilot::PilotAction::Rope { .. }
                )
        ) || (self.tomb_downed(player) && !matches!(command, crate::PlayerCommand::Tomb(_)))
    }

    /// The use button on a tomb fixture (`world/mechanisms.rs` dispatches here).
    pub(super) fn use_tomb_fixture(
        &mut self,
        player: PlayerId,
        tile: TilePosition,
        usable: crate::mechanism::Usable,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        use crate::mechanism::Usable;
        match usable {
            Usable::TombDoor { sealed: true, .. } => self.try_unseal(player, tile, output),
            Usable::TombDoor { sealed: false, .. } => self.enter_tomb(player, tile, output),
            Usable::TombShrine { lit } => self.rest_at_shrine(player, tile, lit, output),
            Usable::TombCache => self.open_tomb_cache(player, tile, output),
            Usable::TombStairs => self.take_tomb_stairs(player, output),
            Usable::TombExit => self.leave_tomb(player),
            _ => PlayerScopedCommandOutcome::IgnoredUnavailable,
        }
    }

    // --- Seals ------------------------------------------------------------------------------

    fn try_unseal(
        &mut self,
        player: PlayerId,
        door: TilePosition,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(site) = self.tomb_site_at_door(door) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let opened = match site.opening {
            SiteOpening::Key => self.players.get_mut(&player).is_some_and(|rig| {
                if rig.pilot_pack.count(TOMB_KEY) > 0 {
                    rig.pilot_pack.remove(TOMB_KEY, 1) == 1
                } else {
                    rig.cargo_bay.remove(TOMB_KEY, 1) == 1
                }
            }),
            SiteOpening::Story => self
                .players
                .get(&player)
                .is_some_and(|rig| !rig.quests.arcs.finished.is_empty()),
            SiteOpening::Explosives => false,
        };
        if opened {
            self.unseal_tomb_site(&site, Some(player), output);
        } else {
            output.push_event(GameplayEventKind::TombSeal {
                player_id: Some(player),
                x: door.x,
                y: door.y,
                opening: site.opening,
                opened: false,
            });
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// Breaks `site`'s seal (its door tiles open; the site is listed as unsealed). The entry
    /// point for story arcs that open a site, and for keys and blasts. Returns whether it was
    /// sealed.
    pub fn unseal_tomb_site(
        &mut self,
        site: &TombSite,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) -> bool {
        let mut changed = false;
        for (tile, kind) in tomb_sites::entrance_cells(site) {
            if let TileKind::Built(BuiltTile::TombDoor { style, .. }) = kind
                && self.terrain.tile(tile).map(|found| found.kind)
                    == Some(TileKind::Built(BuiltTile::TombDoor {
                        style,
                        sealed: true,
                    }))
            {
                changed |= self.terrain.set_kind_by_rule(
                    tile,
                    TileKind::Built(BuiltTile::TombDoor {
                        style,
                        sealed: false,
                    }),
                );
            }
        }
        let _ = self.tombs.unsealed.insert(site.id);
        if changed {
            output.push_event(GameplayEventKind::TombSeal {
                player_id: by,
                x: site.door.x,
                y: site.door.y,
                opening: site.opening,
                opened: true,
            });
        }
        changed
    }

    /// This tick's blasts crack the sealed explosives doors beside them.
    pub(super) fn blast_tomb_seals(&mut self, blasts: &[Blast], output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_tombs() || blasts.is_empty() {
            return;
        }
        for blast in blasts {
            let centre = self.frame().tile_at(blast.x, blast.y);
            let reach = 3;
            let mut doors = Vec::new();
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let tile = TilePosition {
                        x: centre.x + dx,
                        y: centre.y + dy,
                    };
                    let (tx, ty) = self.frame().tile_center(tile);
                    if (tx - blast.x).hypot(ty - blast.y) > SEAL_BLAST_REACH {
                        continue;
                    }
                    if matches!(
                        self.terrain.tile(tile).map(|found| found.kind),
                        Some(TileKind::Built(BuiltTile::TombDoor { sealed: true, .. }))
                    ) {
                        doors.push(tile);
                    }
                }
            }
            for door in doors {
                if let Some(site) = self.tomb_site_at_door(door)
                    && site.opening == SiteOpening::Explosives
                {
                    let _ = self.unseal_tomb_site(&site, blast.owner, output);
                }
            }
        }
    }

    // --- Entering ---------------------------------------------------------------------------

    fn enter_tomb(
        &mut self,
        player: PlayerId,
        door: TilePosition,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(site) = self.tomb_site_at_door(door) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if self.tombs.run_of(player).is_some() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(slot) = self.tombs.free_slot() else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        // The party: the opener first, then crewmates on foot near the door, in id order.
        let (dx, dy) = self.frame().tile_center(door);
        let mut party = vec![player];
        let others: Vec<PlayerId> = self
            .pilots()
            .filter(|(_, body, state)| {
                let (mx, my) = state.middle(body);
                state.owner != player && (mx - dx).hypot(my - dy) <= PARTY_REACH
            })
            .map(|(_, _, state)| state.owner)
            .filter(|owner| self.tombs.run_of(*owner).is_none())
            .collect();
        party.extend(others);
        let counter = self.tombs.site_runs.entry(site.id).or_insert(0);
        let run_number = *counter;
        *counter = counter.saturating_add(1);
        let id = self.tombs.next_run;
        self.tombs.next_run = self.tombs.next_run.wrapping_add(1);
        let now = self.simulation_tick;
        let run = TombRun {
            id,
            site,
            run: run_number,
            slot,
            floor: 0,
            phase: RunPhase::Exploring,
            members: party.iter().copied().map(RunMember::new).collect(),
            rooms: Vec::new(),
            spawns: Vec::new(),
            sprung: Vec::new(),
            start: TilePosition { x: 0, y: 0 },
            boss: None,
            boss_defeated: false,
            checkpoint: None,
            started: now,
            picking_since: None,
            leaving: false,
            crumbling: Vec::new(),
            boss_phase: 0,
            crypt: crate::tomb::crypt::CryptState::default(),
        };
        let _ = self.tombs.runs.insert(id, run);
        for member in &party {
            let unlocks = self.tombs.unlocks.entry(*member).or_default();
            let _ = unlocks.maps.insert(site.id);
            unlocks.runs = unlocks.runs.saturating_add(1);
        }
        self.write_tomb_floor(id, None);
        output.push_event(GameplayEventKind::TombEntered {
            player_id: player,
            style: site.style,
            tier: site.tier,
            party: u8::try_from(party.len()).unwrap_or(u8::MAX),
        });
        PlayerScopedCommandOutcome::Applied
    }

    // --- Floors -----------------------------------------------------------------------------

    /// Writes run `id`'s current floor into its slot (clearing `previous`'s cells first),
    /// spawns its guards and stands the party at its start.
    pub(super) fn write_tomb_floor(&mut self, id: u32, previous: Option<RealizedFloor>) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let origin = tomb_sites::slot_origin(run.slot);
        let realized = Self::tomb_floor_of(run);
        let plan_last = run.on_last_floor();
        let tier = enemy_tier(run);
        let style = run.site.style;
        let seed = run.site.seed;
        self.clear_slot_entities(run.slot);
        if let Some(previous) = previous {
            for (tile, _) in &previous.cells {
                let _ = self
                    .terrain
                    .set_kind_by_rule(offset(*tile, origin), TileKind::Bedrock);
            }
        }
        for (tile, kind) in &realized.cells {
            let _ = self.terrain.set_kind_by_rule(offset(*tile, origin), *kind);
        }
        let rooms: Vec<RealizedRoom> = realized
            .rooms
            .iter()
            .map(|room| RealizedRoom {
                interior: offset_rect(room.interior, origin),
                ..*room
            })
            .collect();
        let spawns: Vec<SpawnSlot> = realized
            .spawns
            .iter()
            .map(|spawn| SpawnSlot {
                at: offset(spawn.at, origin),
                ..*spawn
            })
            .collect();
        let start = offset(realized.start, origin);
        let guards: Vec<SpawnSlot> = spawns
            .iter()
            .copied()
            .filter(|spawn| spawn.role == SpawnRole::Guard)
            .collect();
        let members: Vec<PlayerId> = {
            let Some(run) = self.tombs.runs.get_mut(&id) else {
                return;
            };
            run.sprung = vec![false; rooms.len()];
            run.rooms = rooms;
            run.spawns = spawns;
            run.start = start;
            run.boss = None;
            run.boss_phase = 0;
            run.crumbling.clear();
            run.boss_defeated = !plan_last;
            run.checkpoint = None;
            run.phase = RunPhase::Exploring;
            for member in &mut run.members {
                member.offers.clear();
                member.picked = false;
            }
            run.members.iter().map(|member| member.player).collect()
        };
        for (index, guard) in guards.iter().enumerate() {
            let kind = bestiary(
                style,
                guard.role,
                u32::try_from(hash2(seed, SALT_CACHE, guard.at.x, index as i32) & 0xFFFF)
                    .unwrap_or(0),
            );
            let _ =
                self.spawn_enemy_with(kind, self.frame().tile_center(guard.at), tier, None, None);
        }
        for (index, member) in members.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a handful of members")]
            let shift = (index as f32) * 18.0;
            let (x, _) = self.frame().tile_center(start);
            #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
            let feet = self.frame().rel_row(start.y + 1) * TILE_SIZE - 0.25;
            self.move_pilot_to(*member, x + shift, feet);
            self.revive_if_downed(*member, None);
        }
        for member in members {
            self.refresh_tomb_pilot(member);
        }
        // A crypt floor's trap overlays (T29, `world/crypts.rs`).
        self.prepare_crypt_floor(id);
    }

    /// Despawns every non-pilot entity inside run slot `slot` (creatures, shots, drops).
    fn clear_slot_entities(&mut self, slot: u8) {
        let origin = tomb_sites::slot_origin(slot);
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let (left, top) = (
            self.frame().rel_col(origin.x) * TILE_SIZE,
            self.frame().rel_row(origin.y) * TILE_SIZE,
        );
        #[allow(clippy::cast_precision_loss, reason = "slot sizes are small")]
        let (right, bottom) = (
            left + tomb_sites::SLOT_SIZE.0 as f32 * TILE_SIZE,
            top + tomb_sites::SLOT_SIZE.1 as f32 * TILE_SIZE,
        );
        let ids: std::collections::BTreeSet<EntityId> = self
            .entities
            .ids_in_rect(left, top, right, bottom)
            .into_iter()
            .filter(|id| {
                self.entities
                    .get(*id)
                    .is_some_and(|entity| entity.kind() != EntityKind::Pilot)
            })
            .collect();
        if !ids.is_empty() {
            let _ = self.entities.despawn_many(&ids);
        }
    }

    /// Puts `player`'s pilot (out of its rig if need be) with its feet at `(x, feet)`.
    fn move_pilot_to(&mut self, player: PlayerId, x: f32, feet: f32) {
        let (x, feet) = (quantize(x), quantize(feet));
        if let Some((id, _, _)) = self.pilot_of(player) {
            self.entities.update(id, |entity| {
                entity.body.x = x;
                entity.body.y = feet;
                entity.body.previous_x = x;
                entity.body.previous_y = feet;
                entity.body.velocity_x = 0.0;
                entity.body.velocity_y = 0.0;
                if let EntityState::Pilot(state) = &mut entity.state {
                    state.mode = PilotMode::Free;
                    state.tether = None;
                    state.recalling = false;
                    state.pick = None;
                }
            });
        } else {
            let _ = self.place_pilot(player, x, feet);
        }
    }

    /// Puts the run's view, relic boons and relic health on `player`'s pilot (or takes them
    /// off outside a run).
    pub(super) fn refresh_tomb_pilot(&mut self, player: PlayerId) {
        let (view, effects, downed) = self.tombs.run_of(player).map_or_else(
            || (PilotView::Side, RelicEffects::default(), false),
            |run| {
                let member = run.member(player);
                (
                    view_of(run.site.style),
                    member.map(RunMember::effects).unwrap_or_default(),
                    member.is_some_and(RunMember::is_downed),
                )
            },
        );
        let Some(gear) = self.players.get(&player).map(|rig| rig.pilot_gear) else {
            return;
        };
        let Some((id, _, _)) = self.pilot_of(player) else {
            return;
        };
        self.edit_pilot(id, |_, state| {
            state.view = view;
            state.boons = PilotBoons::of(effects);
            let max = quantize(gear.max_health() * RelicEffects::scale(effects.health_pct));
            if !downed {
                let share = (state.health / state.max_health.max(1.0)).clamp(0.0, 1.0);
                state.health = quantize((max * share).max(1.0));
            }
            state.max_health = max;
        });
    }

    // --- Fixtures ---------------------------------------------------------------------------

    fn rest_at_shrine(
        &mut self,
        player: PlayerId,
        tile: TilePosition,
        lit: bool,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(id) = self.tombs.run_id_of(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if !lit {
            let _ = self
                .terrain
                .set_kind_by_rule(tile, TileKind::Built(BuiltTile::TombShrine { lit: true }));
        }
        let mut banked = 0;
        let mut payouts = Vec::new();
        let mut downed = Vec::new();
        let members: Vec<PlayerId> = if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.checkpoint = Some(tile);
            for member in &mut run.members {
                banked += member.unbanked;
                member.banked = member.banked.saturating_add(member.unbanked);
                payouts.push((member.player, member.unbanked));
                member.unbanked = 0;
                if member.is_downed() {
                    downed.push(member.player);
                }
            }
            run.members.iter().map(|member| member.player).collect()
        } else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        for (member, amount) in payouts {
            if let Some(rig) = self.players.get_mut(&member) {
                rig.credits = rig.credits.saturating_add(amount);
            }
        }
        for member in downed {
            self.revive_if_downed(member, Some(output));
        }
        if !lit {
            // The first rest heals everyone.
            for member in members {
                if let Some((pilot, _, _)) = self.pilot_of(member) {
                    self.edit_pilot(pilot, |_, state| state.health = state.max_health);
                }
            }
        }
        output.push_event(GameplayEventKind::TombShrineRested {
            player_id: player,
            banked,
        });
        PlayerScopedCommandOutcome::Applied
    }

    fn open_tomb_cache(
        &mut self,
        player: PlayerId,
        tile: TilePosition,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(id) = self.tombs.run_id_of(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(run) = self.tombs.runs.get(&id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let (seed, tier, floor) = (run.site.seed ^ u64::from(run.run), run.site.tier, run.floor);
        let effects = self.tombs.effects_of(player);
        let base = cache_value(seed, tier, floor, tile);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "cache values are small"
        )]
        let value = (base as f32 * RelicEffects::scale(effects.loot_pct)).round() as u32;
        let roll = hash2(seed, SALT_KEY, tile.x, tile.y);
        let key = roll.is_multiple_of(6);
        let lore = (roll % 4 == 1)
            .then(|| u8::try_from((roll >> 8) % LORE_PAGES.len() as u64).unwrap_or(0));
        let _ = self
            .terrain
            .set_kind_by_rule(tile, TileKind::Built(BuiltTile::TombCache { open: true }));
        if let Some(member) = self
            .tombs
            .runs
            .get_mut(&id)
            .and_then(|run| run.member_mut(player))
        {
            member.unbanked = member.unbanked.saturating_add(value);
            member.caches = member.caches.saturating_add(1);
            if lore.is_some() {
                member.lore = lore;
            }
        }
        if key && let Some(rig) = self.players.get_mut(&player) {
            rig.pilot_pack.add(TOMB_KEY, 1);
        }
        if let Some(page) = lore {
            let _ = self
                .tombs
                .unlocks
                .entry(player)
                .or_default()
                .lore
                .insert(page);
        }
        output.push_event(GameplayEventKind::TombCacheOpened {
            player_id: player,
            value,
            key,
            lore,
        });
        PlayerScopedCommandOutcome::Applied
    }

    fn take_tomb_stairs(
        &mut self,
        player: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(id) = self.tombs.run_id_of(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let now = self.simulation_tick;
        let Some(run) = self.tombs.runs.get_mut(&id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if run.phase != RunPhase::Exploring {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        if run.on_last_floor() {
            if !run.boss_defeated {
                // The guardian still stands between the party and the way out.
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            }
            let floor = run.floor;
            output.push_event(GameplayEventKind::TombFloorCleared {
                player_id: player,
                floor,
            });
            self.end_tomb_run(id, RunOutcome::Finished, output);
            return PlayerScopedCommandOutcome::Applied;
        }
        let seed = run.site.seed ^ u64::from(run.run);
        let floor = run.floor;
        let style = run.site.style;
        run.phase = RunPhase::Picking;
        run.checkpoint = None;
        run.picking_since = Some(now);
        for (index, member) in run.members.iter_mut().enumerate() {
            member.offers = offers(
                seed,
                floor,
                u8::try_from(index).unwrap_or(u8::MAX),
                &member.relics,
                style,
            );
            member.picked = false;
        }
        output.push_event(GameplayEventKind::TombFloorCleared {
            player_id: player,
            floor,
        });
        PlayerScopedCommandOutcome::Applied
    }

    fn pick_relic(
        &mut self,
        player: PlayerId,
        choice: Option<u8>,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(id) = self.tombs.run_id_of(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(run) = self.tombs.runs.get_mut(&id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if run.phase != RunPhase::Picking {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(member) = run.member_mut(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if member.picked {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let relic = match choice {
            Some(index) => {
                let Some(relic) = member.offers.get(usize::from(index)).copied() else {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                };
                member.relics.push(relic);
                Some(relic)
            }
            None => None,
        };
        member.offers.clear();
        member.picked = true;
        if let Some(relic) = relic {
            let _ = self
                .tombs
                .unlocks
                .entry(player)
                .or_default()
                .codex
                .insert(relic);
        }
        output.push_event(GameplayEventKind::TombRelicPicked {
            player_id: player,
            relic,
        });
        self.refresh_tomb_pilot(player);
        PlayerScopedCommandOutcome::Applied
    }

    fn leave_tomb(&mut self, player: PlayerId) -> PlayerScopedCommandOutcome {
        let Some(id) = self.tombs.run_id_of(player) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if self.tombs.runs.get(&id).is_none_or(|run| run.floor != 0) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        // The run ends in the tombs pass after the tick's commands.
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.leaving = true;
        }
        PlayerScopedCommandOutcome::Applied
    }

    // --- Death and revives ------------------------------------------------------------------

    /// A member's pilot at zero health: downed instead of dead (it lies where it fell until a
    /// crewmate or a shrine revives it). Returns whether `player` was in a run.
    pub(super) fn down_tomb_member(
        &mut self,
        player: PlayerId,
        pilot: EntityId,
        output: &mut TickOutput,
    ) -> bool {
        let now = self.simulation_tick;
        let Some(member) = self
            .tombs
            .runs
            .values_mut()
            .find_map(|run| run.member_mut(player))
        else {
            return false;
        };
        if member.downed.is_none() {
            member.downed = Some(now);
            member.revive_since = None;
            output.push_event(GameplayEventKind::TombMemberDowned {
                player_id: player,
                revived: false,
            });
        }
        self.entities.update(pilot, |entity| {
            entity.body.velocity_x = 0.0;
            if let EntityState::Pilot(state) = &mut entity.state {
                state.health = 0.0;
                state.mode = PilotMode::Free;
                state.tether = None;
                state.pick = None;
            }
        });
        true
    }

    /// Stands a downed member back up with [`REVIVE_SHARE`] of their health.
    fn revive_if_downed(&mut self, player: PlayerId, output: Option<&mut TickOutput>) {
        let Some(member) = self
            .tombs
            .runs
            .values_mut()
            .find_map(|run| run.member_mut(player))
        else {
            return;
        };
        if member.downed.take().is_none() {
            return;
        }
        member.revive_since = None;
        if let Some((pilot, _, _)) = self.pilot_of(player) {
            self.edit_pilot(pilot, |_, state| {
                state.health = quantize(state.max_health * REVIVE_SHARE);
                state.hurt = crate::pilot::STRIKE_GUARD_SECONDS * 2.0;
            });
        }
        if let Some(output) = output {
            output.push_event(GameplayEventKind::TombMemberDowned {
                player_id: player,
                revived: true,
            });
        }
    }

    // --- The run's tick ---------------------------------------------------------------------

    /// One tick of every run (see the module docs). Runs after the pilots move.
    pub(super) fn advance_tombs(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        if self.tombs.runs.is_empty() {
            return;
        }
        let ids: Vec<u32> = self.tombs.runs.keys().copied().collect();
        for id in ids {
            if self.tombs.runs.get(&id).is_some_and(|run| run.leaving) {
                self.end_tomb_run(id, RunOutcome::Retreated, output);
                continue;
            }
            self.drop_departed_members(id, output);
            if !self.tombs.runs.contains_key(&id) {
                continue;
            }
            self.reveal_tomb_rooms(id);
            self.spring_ambushes(id, output);
            self.track_guardian(id, output);
            self.advance_traps(id, output);
            // T28: the side-view traps that move on the clock, crumbling floors and the
            // guardian's phases (`world/tomb_traps.rs`).
            self.advance_side_view_run(id, next_tick, output);
            self.advance_crypt(id, next_tick, output);
            self.advance_revives(id, next_tick, output);
            if self.tombs.runs.get(&id).is_some_and(TombRun::wiped) {
                self.end_tomb_run(id, RunOutcome::Wiped, output);
                continue;
            }
            self.advance_picks(id, next_tick, output);
        }
    }

    /// Members whose player left (or whose pilot vanished) leave the run.
    fn drop_departed_members(&mut self, id: u32, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let gone: Vec<PlayerId> = run
            .members
            .iter()
            .map(|member| member.player)
            .filter(|player| !self.players.contains_key(player) || self.pilot_of(*player).is_none())
            .collect();
        for player in gone {
            self.abandon_tomb_run(player, output);
        }
    }

    /// Every room a standing member is in is revealed for the whole party (rooms light up as
    /// the party walks in; the rest of the floor stays dark).
    fn reveal_tomb_rooms(&mut self, id: u32) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let mut rooms: Vec<TileRect> = Vec::new();
        for member in run.members.iter().filter(|member| !member.is_downed()) {
            let Some((_, body, state)) = self.pilot_of(member.player) else {
                continue;
            };
            let (x, y) = state.middle(body);
            let tile = self.frame().tile_at(x, y);
            if let Some(room) = run.rooms.iter().find(|room| room.interior.contains(tile)) {
                // The walls around, and the pits under the floor.
                let mut rect = room.interior.expanded(1);
                rect.max.y += 2;
                if !rooms.contains(&rect) {
                    rooms.push(rect);
                }
            }
        }
        let members: Vec<PlayerId> = run.members.iter().map(|member| member.player).collect();
        for rect in rooms {
            for member in &members {
                self.reveal_rect(*member, rect);
            }
        }
    }

    fn spring_ambushes(&mut self, id: u32, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        if run.phase != RunPhase::Exploring {
            return;
        }
        let standing: Vec<TilePosition> = run
            .members
            .iter()
            .filter(|member| !member.is_downed())
            .filter_map(|member| self.pilot_of(member.player))
            .map(|(_, body, state)| {
                let (x, y) = state.middle(body);
                self.frame().tile_at(x, y)
            })
            .collect();
        let mut sprung = Vec::new();
        for (index, room) in run.rooms.iter().enumerate() {
            if run.sprung.get(index).copied().unwrap_or(true) {
                continue;
            }
            let waiting = run.spawns.iter().any(|spawn| {
                spawn.room == index && matches!(spawn.role, SpawnRole::Ambusher | SpawnRole::Boss)
            });
            if waiting && standing.iter().any(|tile| room.interior.contains(*tile)) {
                sprung.push(index);
            }
        }
        if sprung.is_empty() {
            return;
        }
        let style = run.site.style;
        let seed = run.site.seed;
        let tier = enemy_tier(run);
        let boss_scale = boss_health_scale(run.site.tier);
        let spawns: Vec<SpawnSlot> = run
            .spawns
            .iter()
            .copied()
            .filter(|spawn| sprung.contains(&spawn.room) && spawn.role != SpawnRole::Guard)
            .collect();
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            for index in &sprung {
                if let Some(flag) = run.sprung.get_mut(*index) {
                    *flag = true;
                }
            }
        }
        let mut boss = None;
        for (index, spawn) in spawns.iter().enumerate() {
            let kind = bestiary(
                style,
                spawn.role,
                u32::try_from(hash2(seed, SALT_KEY, spawn.at.x, index as i32) & 0xFFFF)
                    .unwrap_or(0),
            );
            let (x, y) = self.frame().tile_center(spawn.at);
            let enemy = self.spawn_enemy_with(kind, (x, y), tier, None, None);
            if spawn.role == SpawnRole::Boss {
                self.entities.update(enemy, |entity| {
                    if let EntityState::Enemy(state) = &mut entity.state {
                        state.health = quantize(state.health * boss_scale);
                        state.max_health = quantize(state.max_health * boss_scale);
                    }
                });
                boss = Some(enemy.get());
            }
            output.push_event(GameplayEventKind::TombAmbush {
                x: px(x),
                y: px(y),
                boss: spawn.role == SpawnRole::Boss,
            });
        }
        if let Some(boss) = boss
            && let Some(run) = self.tombs.runs.get_mut(&id)
        {
            run.boss = Some(boss);
        }
    }

    fn track_guardian(&mut self, id: u32, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let Some(boss) = run.boss else {
            return;
        };
        if run.boss_defeated || self.entities.contains(EntityId(boss)) {
            return;
        }
        let members: Vec<PlayerId> = run.members.iter().map(|member| member.player).collect();
        let by = members.first().copied();
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.boss_defeated = true;
        }
        for member in members {
            let unlocks = self.tombs.unlocks.entry(member).or_default();
            unlocks.trophies = unlocks.trophies.saturating_add(1);
            // A trophy case for the home (WP-HOME's displays), the guardian's mask for its
            // wall (Wave 6) and the guardian's crown.
            self.grant_furniture(member, crate::home::decor::Furniture::TrophyCase, 1);
            self.grant_furniture(member, crate::home::decor::Furniture::GuardianMask, 1);
            if let Some(rig) = self.players.get_mut(&member) {
                let _ = rig
                    .cosmetic_unlocks
                    .unlock(crate::CosmeticItem::Decal(crate::cosmetics::Decal::Crown));
            }
        }
        output.push_event(GameplayEventKind::TombGuardianDefeated { by });
    }

    /// Pilots standing in trap tiles take the trap's damage.
    fn advance_traps(&mut self, id: u32, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let dt = self.timing.delta_seconds;
        let mut hurt = Vec::new();
        for member in run.members.iter().filter(|member| !member.is_downed()) {
            let Some((pilot, body, state)) = self.pilot_of(member.player) else {
                continue;
            };
            let left = self
                .frame()
                .tile_at(body.x - crate::pilot::PILOT_HALF_WIDTH + 1.0, body.y)
                .x;
            let right = self
                .frame()
                .tile_at(body.x + crate::pilot::PILOT_HALF_WIDTH - 1.0, body.y)
                .x;
            let top = self
                .frame()
                .tile_at(body.x, body.y - state.height() + 1.0)
                .y;
            let bottom = self.frame().tile_at(body.x, body.y - 1.0).y;
            let mut worst = 0.0_f32;
            for y in top..=bottom {
                for x in left..=right {
                    if let Some(TileKind::Built(BuiltTile::TombTrap { kind, .. })) = self
                        .terrain
                        .tile(TilePosition { x, y })
                        .map(|found| found.kind)
                    {
                        worst = worst.max(TrapKind::from_index(kind).damage_per_second());
                    }
                }
            }
            if worst > 0.0 {
                // T28: Sandwalker sandals and their synergies blunt every trap.
                let traps = RelicEffects::scale(member.effects().trap_pct);
                hurt.push((pilot, worst * dt * self.enemy_difficulty().damage() * traps));
            }
        }
        for (pilot, amount) in hurt {
            let _ = self.damage_pilot(pilot, amount, output);
        }
    }

    fn advance_revives(&mut self, id: u32, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        let middles: Vec<(PlayerId, bool, (f32, f32))> = run
            .members
            .iter()
            .filter_map(|member| {
                self.pilot_of(member.player)
                    .map(|(_, body, state)| (member.player, member.is_downed(), state.middle(body)))
            })
            .collect();
        let need = self.timing.ticks_for_millis(REVIVE_MILLIS).max(1);
        let mut revived = Vec::new();
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            for member in &mut run.members {
                if !member.is_downed() {
                    continue;
                }
                let Some((_, _, at)) = middles.iter().find(|(player, ..)| *player == member.player)
                else {
                    continue;
                };
                let helped = middles.iter().any(|(player, downed, middle)| {
                    *player != member.player
                        && !*downed
                        && (middle.0 - at.0).hypot(middle.1 - at.1) <= REVIVE_REACH
                });
                if !helped {
                    member.revive_since = None;
                    continue;
                }
                let since = *member.revive_since.get_or_insert(next_tick);
                if next_tick.get().saturating_sub(since.get()) >= need {
                    revived.push(member.player);
                }
            }
        }
        for player in revived {
            self.revive_if_downed(player, Some(output));
        }
    }

    fn advance_picks(&mut self, id: u32, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        if run.phase != RunPhase::Picking {
            return;
        }
        let since = run.picking_since.unwrap_or(next_tick);
        let timed_out = next_tick.get().saturating_sub(since.get())
            >= self.timing.ticks_for_millis(PICK_TIMEOUT_MILLIS);
        let slow: Vec<PlayerId> = run
            .members
            .iter()
            .filter(|member| !member.picked)
            .map(|member| member.player)
            .collect();
        if !slow.is_empty() && !timed_out {
            return;
        }
        for player in slow {
            let _ = self.pick_relic(player, None, output);
        }
        let previous = self.tombs.runs.get(&id).map(Self::tomb_floor_of);
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.floor += 1;
            run.picking_since = None;
        }
        self.write_tomb_floor(id, previous);
    }

    // --- Ending -----------------------------------------------------------------------------

    /// `player` leaves their run for good (left the session or was parked): their unbanked
    /// loot is lost and their pilot goes back outside. The run ends when no one is left.
    pub(super) fn abandon_tomb_run(&mut self, player: PlayerId, output: &mut TickOutput) {
        let Some(id) = self.tombs.run_id_of(player) else {
            return;
        };
        let Some(run) = self.tombs.runs.get(&id) else {
            return;
        };
        if run.members.len() <= 1 {
            self.end_tomb_run(id, RunOutcome::Abandoned, output);
            return;
        }
        let site = run.site;
        let summary_member = run.member(player).cloned();
        if let Some(run) = self.tombs.runs.get_mut(&id) {
            run.members.retain(|member| member.player != player);
        }
        if let Some(member) = summary_member {
            self.finish_member(id, &site, &member, RunOutcome::Abandoned, 0, output);
        }
    }

    /// Ends run `id`: banks or loses each member's loot, records summaries and unlocks, puts
    /// the party back outside the site and the slot back to bedrock.
    pub(super) fn end_tomb_run(&mut self, id: u32, outcome: RunOutcome, output: &mut TickOutput) {
        let Some(run) = self.tombs.runs.get(&id).cloned() else {
            return;
        };
        let realized = Self::tomb_floor_of(&run);
        let origin = tomb_sites::slot_origin(run.slot);
        for (index, member) in run.members.iter().enumerate() {
            self.finish_member(id, &run.site, member, outcome, index, output);
        }
        let _ = self.tombs.runs.remove(&id);
        self.clear_slot_entities(run.slot);
        for (tile, _) in &realized.cells {
            let _ = self
                .terrain
                .set_kind_by_rule(offset(*tile, origin), TileKind::Bedrock);
        }
    }

    /// One member's end of a run (they are already out of, or leaving, the run's member list
    /// for `outcome`).
    fn finish_member(
        &mut self,
        id: u32,
        site: &TombSite,
        member: &RunMember,
        outcome: RunOutcome,
        index: usize,
        output: &mut TickOutput,
    ) {
        let player = member.player;
        let floor = self.tombs.runs.get(&id).map_or(0, |run| run.floor);
        let started = self
            .tombs
            .runs
            .get(&id)
            .map_or(self.simulation_tick, |run| run.started);
        let hoard = if outcome == RunOutcome::Finished {
            guardian_hoard(site.tier)
        } else {
            0
        };
        let (kept, lost) = if outcome.banks() {
            (member.unbanked.saturating_add(hoard), 0)
        } else {
            (0, member.unbanked)
        };
        if let Some(rig) = self.players.get_mut(&player) {
            rig.credits = rig.credits.saturating_add(kept);
        }
        let banked = member.banked.saturating_add(kept);
        let lore = member.lore;
        let first_finish = {
            let unlocks = self.tombs.unlocks.entry(player).or_default();
            unlocks.codex.extend(member.relics.iter().copied());
            unlocks.best_haul = unlocks.best_haul.max(banked);
            if outcome == RunOutcome::Finished {
                unlocks.finished = unlocks.finished.saturating_add(1);
                let _ = unlocks.cleared.insert(site.id);
            }
            outcome == RunOutcome::Finished && unlocks.finished == 1
        };
        if first_finish {
            // The first tomb seen through to the end earns a plinth to show a relic on
            // (Wave 6 trophies for the home).
            self.grant_furniture(player, crate::home::decor::Furniture::RelicPlinth, 1);
        }
        let serial = self.tombs.next_summary;
        self.tombs.next_summary = serial.wrapping_add(1);
        let floors_cleared = if outcome == RunOutcome::Finished {
            site.floors()
        } else {
            floor
        };
        let _ = self.tombs.summaries.insert(
            player,
            RunSummary {
                serial,
                site: site.id,
                style: site.style,
                tier: site.tier,
                outcome,
                floors: floors_cleared,
                floors_total: site.floors(),
                banked,
                lost,
                relics: member.relics.clone(),
                ticks: self.simulation_tick.get().saturating_sub(started.get()),
                lore,
            },
        );
        // Back outside, in front of the door.
        let (x, feet) = Self::outside_spot(self.frame(), site, index);
        if self.players.contains_key(&player) {
            self.move_pilot_to(player, x, feet);
            if let Some((pilot, _, _)) = self.pilot_of(player) {
                let gear = self.players.get(&player).map(|rig| rig.pilot_gear);
                self.edit_pilot(pilot, |_, state| {
                    state.view = PilotView::Side;
                    state.boons = PilotBoons::default();
                    if let Some(gear) = gear {
                        state.max_health = gear.max_health();
                    }
                    if member.is_downed() || state.health <= 0.0 {
                        state.health = quantize(state.max_health * REVIVE_SHARE);
                    }
                    state.health = state.health.min(state.max_health);
                });
            }
        }
        output.push_event(GameplayEventKind::TombRunEnded {
            player_id: player,
            outcome,
            banked,
            lost,
        });
    }

    /// Where member `index` stands outside `site` after a run: `(x, feet)`.
    #[must_use]
    pub fn outside_spot(
        frame: crate::position::Frame,
        site: &TombSite,
        index: usize,
    ) -> (f32, f32) {
        let (door_x, _) = frame.tile_center(site.door);
        #[allow(clippy::cast_precision_loss, reason = "a handful of members")]
        let step = index as f32 * 20.0;
        match site.style {
            SiteStyle::SideView => {
                let outward: f32 = if site.mirrored { 1.0 } else { -1.0 };
                #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
                let feet = frame.rel_row(site.door.y + 2) * TILE_SIZE - 0.25;
                (outward.mul_add(TILE_SIZE * 1.3 + step, door_x), feet)
            }
            SiteStyle::TopDown => {
                #[allow(clippy::cast_precision_loss, reason = "tile rows are small")]
                let feet = frame.rel_row(site.door.y) * TILE_SIZE - 0.25;
                let outward: f32 = if index.is_multiple_of(2) { 1.0 } else { -1.0 };
                (outward.mul_add(step, door_x), feet)
            }
        }
    }

    // --- Relics in combat --------------------------------------------------------------------

    /// Ricochet Rounds: a personal-weapon shot of a member with bounces left reflects off the
    /// rock it just entered (back to `last`, the velocity mirrored on the axis it crossed).
    /// Returns whether it bounced.
    pub(super) fn ricochet(
        &self,
        state: &mut crate::entity::ProjectileState,
        body: &mut crate::entity::Body,
        last: (f32, f32),
    ) -> bool {
        if !state.weapon.is_personal() || self.tombs.runs.is_empty() {
            return false;
        }
        let bounces = self.tombs.effects_of(state.owner).ricochets;
        if state.bounces >= bounces {
            return false;
        }
        let (from, to) = (
            self.frame().tile_at(last.0, last.1),
            self.frame().tile_at(body.x, body.y),
        );
        let crossed_x = from.x != to.x
            && self
                .terrain
                .is_solid_at(TilePosition { x: to.x, y: from.y });
        let crossed_y = from.y != to.y
            && self
                .terrain
                .is_solid_at(TilePosition { x: from.x, y: to.y });
        if crossed_x || !crossed_y {
            body.velocity_x = -body.velocity_x;
        }
        if crossed_y || !crossed_x {
            body.velocity_y = -body.velocity_y;
        }
        body.x = last.0;
        body.y = last.1;
        state.bounces += 1;
        true
    }

    /// A hit by `damage`'s player on a creature, scaled by their relics; with the player's
    /// lifesteal share and pilot (Vampiric Pick) when they have one.
    pub(super) fn tomb_scaled_hit(
        &self,
        mut damage: crate::combat::Damage,
    ) -> (crate::combat::Damage, Option<(EntityId, u8)>) {
        if self.tombs.runs.is_empty() {
            return (damage, None);
        }
        let Some(player) = damage.source.player() else {
            return (damage, None);
        };
        let effects = self.tombs.effects_of(player);
        if effects == RelicEffects::default() {
            return (damage, None);
        }
        damage.amount = quantize(damage.amount * RelicEffects::scale(effects.damage_pct));
        let thief = (effects.lifesteal_pct > 0)
            .then(|| {
                self.pilot_of(player)
                    .map(|(id, _, _)| (id, effects.lifesteal_pct))
            })
            .flatten();
        (damage, thief)
    }

    // --- Scenarios --------------------------------------------------------------------------

    /// Starts a run at `site` for `party` (tests, screenshot scenes and the balance harness):
    /// the seal is broken, the party's pilots are put out by the door and walked in. Returns the
    /// run id.
    pub fn scenario_enter_tomb(&mut self, site: &TombSite, party: &[PlayerId]) -> Option<u32> {
        let mut output = TickOutput::default();
        let _ = self.unseal_tomb_site(site, None, &mut output);
        for (index, player) in party.iter().enumerate() {
            let (x, feet) = Self::outside_spot(self.frame(), site, index);
            self.move_pilot_to(*player, x, feet);
        }
        let first = *party.first()?;
        let _ = self.enter_tomb(first, site.door, &mut output);
        self.tombs.run_id_of(first)
    }

    /// Edits `player`'s member record (tests and scenes: relics, loot).
    pub fn scenario_edit_tomb_member(
        &mut self,
        player: PlayerId,
        change: impl FnOnce(&mut RunMember),
    ) {
        if let Some(member) = self
            .tombs
            .runs
            .values_mut()
            .find_map(|run| run.member_mut(player))
        {
            change(member);
        }
        self.refresh_tomb_pilot(player);
    }

    /// Ends run `id` with `outcome` (tests and scenes: the summary screens).
    pub fn scenario_end_tomb_run(&mut self, id: u32, outcome: RunOutcome) {
        let mut output = TickOutput::default();
        self.end_tomb_run(id, outcome, &mut output);
    }

    /// Edits `player`'s permanent tomb unlocks (tests and scenes: the codex).
    pub fn scenario_tomb_unlocks(
        &mut self,
        player: PlayerId,
        change: impl FnOnce(&mut crate::tomb::TombUnlocks),
    ) {
        change(self.tombs.unlocks.entry(player).or_default());
    }

    /// Downs `player`'s pilot in its run (tests and scenes: revives).
    pub fn scenario_down_tomb_member(&mut self, player: PlayerId) {
        let mut output = TickOutput::default();
        if let Some((id, _, _)) = self.pilot_of(player) {
            let _ = self.down_tomb_member(player, id, &mut output);
        }
    }

    /// Puts run `id` into its relic picks as if the stairs were taken (tests and scenes).
    pub fn scenario_tomb_stairs(&mut self, player: PlayerId) {
        let mut output = TickOutput::default();
        let _ = self.take_tomb_stairs(player, &mut output);
    }
}
