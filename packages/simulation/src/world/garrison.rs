//! Descent VII part B in the world (Wave 10 ancients-b: the Sleeping Garrison, the Seal Gate
//! and the Gatekeeper's hall; the rules live in [`crate::garrison`]).
//!
//! [`WorldState::advance_garrison`] runs once per tick after the far Strange Layers' pass. On
//! worlds without Descents it returns at once. In order:
//!
//! 1. **Forts**: a rig walking into a fort stirs it (the build window); then wave after wave:
//!    the bays that will open glow ([`WAVE_TELL_MILLIS`]), the machines climb out, and once a
//!    wave is beaten a breather; the last wave beaten, the garrison sleeps and its armoury pays
//!    every rig that held the fort. A fort nobody holds for [`ABANDON_MILLIS`] stands down.
//! 2. **Defences**: turrets shoot the nearest hostile they see, shock traps zap and stun what
//!    walks over them, garrison machines batter the defences they reach (siege shells break
//!    them where they land, `garrison_shell_burst`).
//! 3. **Tremors** in the Seal Gate and **falling stones** (tremors' and the Colossus's crypt's),
//!    each telegraphed by dust.
//! 4. **The Seal Gate**: a rig carrying the keys of every Descent at the door lights its six
//!    sockets one by one and the door sinks into the floor; a rig missing one is told which.
//! 5. **Landmarks found** and **the Colossus's fight** (`world/enemies/garrison.rs`).
//!
//! Hooks elsewhere: the drill refuses the Heart Seal without the Heart Key
//! ([`WorldState::heart_seal_refuses`]); the build kit places and takes down defences
//! (`world/bases.rs` through [`WorldState::check_defence_place`]).

use crate::{
    base::{BuildCost, BuildPayment, BuildRefusal},
    cave::CaveTile,
    depth_progression::RigSystem,
    descent::Stratum,
    economy::DeepClaimStatus,
    enemy::{Damage, DamageKind, DamageSource, EnemyKind, EnemyMode, EnemyShotState, ShotKind},
    entity::{Body, EntityId, EntityState},
    event::{GameplayEventKind, TickOutput},
    garrison::{
        ABANDON_MILLIS, BATTER_MILLIS, BATTER_REACH_TILES, BREATHER_MILLIS, Defence, DefenceKind,
        FortPhase, FortState, Garrison, GarrisonEvent, GarrisonLandmark, GarrisonSchedule,
        RubbleDrop, SEAL_READ_TILES, SHELL_BLAST, STIR_MILLIS, SealKey, ShotTrace, TRAP_DAMAGE,
        TRAP_REARM_MILLIS, TREMOR_CEILING_TILES, TREMOR_STONE_DAMAGE, TREMOR_TELL_MILLIS,
        TURRET_DAMAGE, TURRET_PERIOD_MILLIS, TURRET_RANGE_TILES, WAVE_TELL_MILLIS, batter_damage,
        door_rows_open, sockets_lit, tremor_period, tremor_stones, wave_roster,
    },
    ids::{PlayerId, SimulationTick},
    item::ItemKind,
    logistics::scaled_price,
    player::Player,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        TileRect,
        descent::garrison::{self as sites, Fort},
        noise::hash2,
    },
};

use super::{WorldState, bases::BuildQuote, movement::AUTHORITATIVE_PLAYER_RADIUS};

/// Most stones waiting to fall at once.
const MAX_DROPS: usize = 32;
/// How far round a rig forts are looked for, tiles.
const FORT_LOOK_TILES: i32 = 48;
const SALT_BAYS: u64 = 0xA4C_1001;
const SALT_TREMOR: u64 = 0xA4C_1002;

/// A Descent VII part B event at `tile`.
pub(super) const fn garrison_event(
    what: GarrisonEvent,
    player_id: Option<PlayerId>,
    tile: TilePosition,
) -> GameplayEventKind {
    GameplayEventKind::Garrison {
        what,
        player_id,
        x: tile.x,
        y: tile.y,
    }
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// Descent VII part B's shared state.
    #[must_use]
    pub const fn garrison(&self) -> &Garrison {
        &self.garrison
    }

    /// Descent VII part B's host bookkeeping (tests, tools).
    #[must_use]
    pub const fn garrison_schedule(&self) -> &GarrisonSchedule {
        &self.garrison_schedule
    }

    /// The Gatekeeper's hall (Deep Descent worlds).
    #[must_use]
    pub fn colossus_hall(&self) -> Option<TileRect> {
        sites::colossus_hall(self.terrain.settings(), &self.descent_strata_layout()?)
    }

    /// The Seal Gate's door (Deep Descent worlds).
    #[must_use]
    pub fn seal_door(&self) -> Option<TileRect> {
        let hall = self.colossus_hall()?;
        Some(sites::seal_door(self.terrain.settings(), &hall))
    }

    /// The Seal Court before the door (Deep Descent worlds).
    #[must_use]
    pub fn seal_court(&self) -> Option<TileRect> {
        let hall = self.colossus_hall()?;
        Some(sites::seal_court(self.terrain.settings(), &hall))
    }

    /// Which of the Seal Gate's keys `player_id` carries, in [`SealKey::ALL`] order (the Deep
    /// Claim is the world's).
    #[must_use]
    pub fn seal_keys_of(&self, player_id: PlayerId) -> [bool; 6] {
        let mut held = [false; 6];
        let Some(player) = self.players.get(&player_id) else {
            return held;
        };
        for (slot, key) in held.iter_mut().zip(SealKey::ALL) {
            *slot = match key {
                // The Sinkhole's Deep Claim (a classic world's story unlocks it too).
                SealKey::DeepClaim => {
                    self.crust.deep_claim
                        || self.progression.deep_claim_status == DeepClaimStatus::Unlocked
                }
                _ => key
                    .systems()
                    .iter()
                    .all(|system| player.hazard_gear.has_system(*system)),
            };
        }
        held
    }

    /// Whether `player_id` carries the keys of every Descent (the Seal Gate opens to it).
    #[must_use]
    pub fn holds_every_seal_key(&self, player_id: PlayerId) -> bool {
        self.seal_keys_of(player_id).iter().all(|held| *held)
    }

    /// The forts within `reach` tiles of `tile` (Deep Descent worlds).
    #[must_use]
    pub fn forts_near(&self, tile: TilePosition, reach: i32) -> Vec<Fort> {
        let Some(layout) = self.descent_strata_layout() else {
            return Vec::new();
        };
        sites::forts_with(
            self.terrain.settings(),
            &layout,
            &TileRect::around(tile, reach, reach),
        )
    }

    /// The fort whose walls hold `tile`, if any (field works go up inside without a claim).
    #[must_use]
    pub fn fort_at(&self, tile: TilePosition) -> Option<Fort> {
        self.forts_near(tile, 2)
            .into_iter()
            .find(|fort| fort.bounds.contains(tile))
    }

    /// The fort state of the fort whose walls hold `tile`, if it has stirred.
    #[must_use]
    pub fn fort_state_at(&self, tile: TilePosition) -> Option<&FortState> {
        self.garrison
            .forts
            .values()
            .find(|fort| fort.bounds.expanded(1).contains(tile))
    }

    /// The landmark holding `tile`, if any (the game names a `LandmarkFound` event's find).
    #[must_use]
    pub fn garrison_landmark_at(&self, tile: TilePosition) -> Option<GarrisonLandmark> {
        let layout = self.descent_strata_layout()?;
        let settings = self.terrain.settings();
        if let Some(bastion) = sites::grand_bastion(settings, &layout)
            && bastion.bounds.expanded(1).contains(tile)
        {
            return Some(GarrisonLandmark::GrandBastion);
        }
        let hall = sites::colossus_hall(settings, &layout)?;
        if sites::hall_inside(&hall).contains(tile) {
            return Some(GarrisonLandmark::ColossusHall);
        }
        sites::seal_court(settings, &hall)
            .contains(tile)
            .then_some(GarrisonLandmark::SealCourt)
    }

    /// Whether the build kit's defences are unlocked: a crew reached the Sleeping Garrison
    /// (test mode: always).
    #[must_use]
    pub fn defences_unlocked(&self) -> bool {
        if self.is_test_mode() {
            return true;
        }
        self.descent_strata_layout().is_some_and(|layout| {
            self.progression.deepest_tile_reached >= layout.rows(Stratum::SleepingGarrison).0
        })
    }

    /// What a defence of `kind` costs at `at` before stock on hand (credits scaled by the
    /// Descent's price percent).
    #[must_use]
    pub fn defence_cost(&self, kind: DefenceKind, at: TilePosition) -> BuildCost {
        BuildCost {
            block: None,
            any_blocks: kind.blocks(),
            credits: scaled_price(kind.base_credits(), self.descent_of_row(at.y)),
        }
    }

    // --- Hooks --------------------------------------------------------------------------------

    /// Whether the Heart Seal refuses `player_id`'s drill on a `kind` tile (it yields only to a
    /// rig carrying the Heart Key, the Colossus's reward).
    #[must_use]
    pub(super) fn heart_seal_refuses(&self, player_id: PlayerId, kind: TileKind) -> bool {
        kind == TileKind::Cave(CaveTile::HeartSeal)
            && !self
                .players
                .get(&player_id)
                .is_some_and(|player| player.hazard_gear.has_system(RigSystem::HeartKey))
    }

    /// A siege shell burst at local point `(x, y)`: it breaks the defences it lands among.
    pub(super) fn garrison_shell_burst(
        &mut self,
        kind: ShotKind,
        x: f32,
        y: f32,
        output: &mut TickOutput,
    ) {
        if kind != ShotKind::SiegeShell || self.garrison.defences.is_empty() {
            return;
        }
        let (radius, damage) = SHELL_BLAST;
        let reach = radius * TILE_SIZE;
        let frame = self.frame();
        let hit: Vec<u32> = self
            .garrison
            .defences
            .iter()
            .filter(|(_, defence)| {
                let (cx, cy) = frame.tile_center(defence.anchor);
                (cx - x).hypot(cy - y) <= TILE_SIZE.mul_add(0.5, reach)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in hit {
            self.hurt_defence(id, damage, output);
        }
    }

    // --- The build kit's defences ---------------------------------------------------------

    /// Whether a defence may go up at `at`: in a claim `player_id` may build in, or inside a
    /// garrison fort's walls (field works need no claim).
    fn defence_ground(&self, player_id: PlayerId, at: TilePosition) -> Result<(), BuildRefusal> {
        if self.fort_at(at).is_some() {
            return Ok(());
        }
        let claim = self.bases.claim_at(at).ok_or(BuildRefusal::NoClaim)?;
        if !claim.access.may_build(claim.owner, player_id) {
            return Err(BuildRefusal::NotPermitted);
        }
        Ok(())
    }

    /// Checks placing a defence of `kind` with its bottom-left tile at `at` (the build kit,
    /// from [`WorldState::check_build`]).
    pub(super) fn check_defence_place(
        &self,
        player_id: PlayerId,
        player: &Player,
        kind: DefenceKind,
        at: TilePosition,
    ) -> Result<BuildQuote, BuildRefusal> {
        if !self.defences_unlocked() {
            return Err(BuildRefusal::NeedsGarrison);
        }
        self.defence_ground(player_id, at)?;
        let (_, height) = kind.footprint();
        for dy in 0..height {
            let tile = TilePosition {
                x: at.x,
                y: at.y - dy,
            };
            if self.terrain.is_solid_at(tile)
                || self.bases.module_at(tile).is_some()
                || self.bases.fixtures_at(tile).front.is_some()
                || self.garrison.defence_at(tile).is_some()
            {
                return Err(BuildRefusal::Blocked);
            }
        }
        if kind == DefenceKind::Barricade && self.tile_occupied(at) {
            return Err(BuildRefusal::Blocked);
        }
        if kind.needs_floor()
            && !self.terrain.is_solid_at(TilePosition {
                x: at.x,
                y: at.y + 1,
            })
        {
            return Err(BuildRefusal::NeedsFloor);
        }
        let payment = super::bases::payment_for(player, self.defence_cost(kind, at));
        if payment.credits > player.credits {
            return Err(BuildRefusal::CannotAfford);
        }
        Ok(BuildQuote {
            cost: payment,
            refund: BuildPayment::default(),
        })
    }

    /// Builds a defence (already checked and charged).
    pub(super) fn apply_defence_place(
        &mut self,
        player_id: PlayerId,
        kind: DefenceKind,
        at: TilePosition,
        paid: u32,
    ) {
        let id = self.garrison.next_defence;
        self.garrison.next_defence = id.wrapping_add(1);
        if kind == DefenceKind::Barricade {
            let _ = self
                .terrain
                .set_kind_by_rule(at, TileKind::Cave(CaveTile::WarBarricade));
        }
        self.garrison.defences.insert(
            id,
            Defence {
                kind,
                anchor: at,
                owner: player_id,
                health: kind.health(),
                ready: self.simulation_tick,
                fired: None,
                paid,
            },
        );
    }

    /// Checks taking down the defence at `at`, if there is one (`None`: no defence there).
    pub(super) fn check_defence_remove(
        &self,
        player_id: PlayerId,
        at: TilePosition,
    ) -> Option<Result<BuildQuote, BuildRefusal>> {
        let (_, defence) = self.garrison.defence_at(at)?;
        let allowed = defence.owner == player_id
            || self.fort_at(defence.anchor).is_some()
            || self
                .bases
                .claim_at(defence.anchor)
                .is_some_and(|claim| claim.access.may_build(claim.owner, player_id));
        if !allowed {
            return Some(Err(BuildRefusal::NotPermitted));
        }
        Some(Ok(BuildQuote {
            cost: BuildPayment::default(),
            refund: BuildPayment {
                credits: defence.paid / 2,
                ..BuildPayment::default()
            },
        }))
    }

    /// Takes down the defence at `at` (already checked); returns whether there was one.
    pub(super) fn apply_defence_remove(&mut self, at: TilePosition) -> bool {
        let Some((id, _)) = self.garrison.defence_at(at) else {
            return false;
        };
        self.remove_defence(id);
        true
    }

    /// Removes defence `id` (a barricade's block goes too).
    fn remove_defence(&mut self, id: u32) {
        if let Some(defence) = self.garrison.defences.remove(&id)
            && defence.kind == DefenceKind::Barricade
            && self.terrain.tile(defence.anchor).map(|tile| tile.kind)
                == Some(TileKind::Cave(CaveTile::WarBarricade))
        {
            let _ = self.terrain.set_kind_by_rule(defence.anchor, TileKind::Air);
        }
    }

    /// `damage` to defence `id`; at nothing left it is destroyed.
    fn hurt_defence(&mut self, id: u32, damage: u16, output: &mut TickOutput) {
        let Some(defence) = self.garrison.defences.get_mut(&id) else {
            return;
        };
        defence.health = defence.health.saturating_sub(damage);
        if defence.health == 0 {
            let at = defence.anchor;
            let owner = defence.owner;
            self.remove_defence(id);
            output.push_event(garrison_event(
                GarrisonEvent::DefenceDestroyed,
                Some(owner),
                at,
            ));
        }
    }

    // --- Staging (tests, scenes and dev tools) ----------------------------------------------

    /// Replaces Descent VII part B's shared state (scenes, tests, tools).
    pub fn set_garrison(&mut self, state: Garrison) {
        self.garrison = state;
    }

    /// Builds a defence of `kind` at `at` for `owner` without a check or a charge (scenes,
    /// tests).
    pub fn stage_defence(&mut self, owner: PlayerId, kind: DefenceKind, at: TilePosition) {
        self.apply_defence_place(owner, kind, at, 0);
    }

    /// Stirs the fort with pylon `pylon` straight into wave `wave`'s tell (scenes, tests).
    pub fn stage_fort_wave(&mut self, pylon: TilePosition, wave: u8) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let Some(fort) = sites::fort_of_pylon(self.terrain.settings(), &layout, pylon) else {
            return;
        };
        let now = self.simulation_tick;
        let state = self
            .garrison
            .forts
            .entry(pylon)
            .or_insert_with(|| FortState::new(fort.kind, fort.bounds));
        state.last_held = now;
        state.wave = wave.saturating_sub(1);
        self.telegraph_wave(&fort, &mut TickOutput::default());
    }

    // --- The tick -----------------------------------------------------------------------------

    /// One tick of Descent VII part B (see the module docs).
    pub(super) fn advance_garrison(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let (top, bottom) = sites::carve_rows(&layout);
        let anyone_near = self.players.values().any(|player| {
            let row = player.tile_in(self.frame()).y;
            row >= top && row <= bottom
        });
        if anyone_near || !self.garrison.forts.is_empty() {
            self.advance_forts(output);
        }
        if !self.garrison.defences.is_empty() {
            self.advance_defences(next_tick, output);
        }
        if anyone_near {
            self.advance_tremors(output);
        }
        if !self.garrison.drops.is_empty() {
            self.advance_drops();
        }
        if anyone_near {
            self.advance_seal_gate(output);
            self.note_garrison_landmarks(output);
        }
        self.advance_colossus(output);
    }

    /// Living rigs and their tiles.
    fn living_rigs(&self) -> Vec<(PlayerId, TilePosition)> {
        let frame = self.frame();
        self.players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .collect()
    }

    /// Forts stir, wave, breathe, sleep or stand down (step 1).
    fn advance_forts(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let settings = *self.terrain.settings();
        let now = self.simulation_tick;
        let rigs = self.living_rigs();
        // Rigs walking into a dormant fort stir it.
        let (top, bottom) = layout.rows(Stratum::SleepingGarrison);
        for (player_id, tile) in &rigs {
            if tile.y < top - 30 || tile.y > bottom + 30 {
                continue;
            }
            let forts = sites::forts_with(
                &settings,
                &layout,
                &TileRect::around(*tile, FORT_LOOK_TILES, FORT_LOOK_TILES),
            );
            for fort in forts {
                if !fort.inside().contains(*tile) {
                    continue;
                }
                let pylon = fort.pylon();
                let dormant = self
                    .garrison
                    .forts
                    .get(&pylon)
                    .is_none_or(|state| state.phase == FortPhase::Dormant);
                if !dormant {
                    continue;
                }
                let mut state = FortState::new(fort.kind, fort.bounds);
                state.last_held = now;
                if self.enemies.difficulty.hostile() {
                    state.phase = FortPhase::Stirring;
                    state.due = self.after_millis(now, STIR_MILLIS);
                    self.garrison.forts.insert(pylon, state);
                    output.push_event(garrison_event(
                        GarrisonEvent::FortStirred,
                        Some(*player_id),
                        pylon,
                    ));
                } else {
                    // A peaceful world: the armoury stands open to the first rig.
                    state.phase = FortPhase::Asleep;
                    state.wave = fort.kind.waves();
                    self.garrison.forts.insert(pylon, state);
                    self.pay_armoury(&fort, output);
                }
            }
        }
        let awake: Vec<TilePosition> = self
            .garrison
            .forts
            .iter()
            .filter(|(_, state)| state.awake())
            .map(|(pylon, _)| *pylon)
            .collect();
        for pylon in awake {
            let Some(fort) = sites::fort_of_pylon(&settings, &layout, pylon) else {
                self.garrison.forts.remove(&pylon);
                continue;
            };
            self.advance_fort(&fort, &rigs, output);
        }
    }

    /// One awake fort's tick.
    fn advance_fort(
        &mut self,
        fort: &Fort,
        rigs: &[(PlayerId, TilePosition)],
        output: &mut TickOutput,
    ) {
        let now = self.simulation_tick;
        let pylon = fort.pylon();
        let reach = fort.bounds.expanded(crate::garrison::FORT_REACH_TILES);
        let held = rigs.iter().any(|(_, tile)| reach.contains(*tile));
        let breather = self.after_millis(now, BREATHER_MILLIS);
        let abandon = self.timing.ticks_for_millis(ABANDON_MILLIS);
        let Some(state) = self.garrison.forts.get_mut(&pylon) else {
            return;
        };
        if held {
            state.last_held = now;
        }
        let idle = now.get().saturating_sub(state.last_held.get());
        if idle > abandon {
            self.stand_down(pylon, output);
            return;
        }
        // The wave's machines that are gone (killed, or wandered off and despawned).
        let entities = &self.entities;
        state
            .machines
            .retain(|id| entities.get(EntityId(*id)).is_some());
        match state.phase {
            FortPhase::Stirring | FortPhase::Breather if now >= state.due => {
                self.telegraph_wave(fort, output);
            }
            FortPhase::Telegraph if now >= state.due => self.release_wave(fort, output),
            FortPhase::Wave if state.machines.is_empty() => {
                if state.wave >= state.kind.waves() {
                    state.phase = FortPhase::Asleep;
                    state.lit_bays.clear();
                    self.pay_armoury(fort, output);
                } else {
                    state.phase = FortPhase::Breather;
                    state.due = breather;
                    output.push_event(garrison_event(GarrisonEvent::WaveCleared, None, pylon));
                }
            }
            _ => {}
        }
    }

    /// The next wave's bays light (its tell).
    fn telegraph_wave(&mut self, fort: &Fort, output: &mut TickOutput) {
        let now = self.simulation_tick;
        let pylon = fort.pylon();
        let due = self.after_millis(now, WAVE_TELL_MILLIS);
        let rigs = self.rigs_holding(fort);
        let seed = self.terrain.seed();
        let Some(state) = self.garrison.forts.get_mut(&pylon) else {
            return;
        };
        state.wave = state.wave.saturating_add(1);
        let roster = wave_roster(fort.kind, state.wave, rigs, fort.along);
        let bays = fort.bay_spawns();
        let start = usize::try_from(
            hash2(
                seed,
                SALT_BAYS,
                pylon.x,
                pylon.y.wrapping_add(i32::from(state.wave)),
            ) % bays.len().max(1) as u64,
        )
        .unwrap_or(0);
        let count = roster.len().min(bays.len()).max(1);
        // Spread round the fort from the start, so a wave comes from both sides.
        let step = (bays.len() / count).max(1);
        state.lit_bays = (0..count)
            .map(|index| bays[(start + index * step) % bays.len()])
            .collect();
        state.lit_bays.sort();
        state.lit_bays.dedup();
        state.phase = FortPhase::Telegraph;
        state.due = due;
        output.push_event(garrison_event(GarrisonEvent::WaveTelegraphed, None, pylon));
    }

    /// Rigs holding `fort` now (at least one).
    fn rigs_holding(&self, fort: &Fort) -> usize {
        let hold = fort.bounds.expanded(crate::garrison::FORT_REACH_TILES);
        self.living_rigs()
            .iter()
            .filter(|(_, tile)| hold.contains(*tile))
            .count()
            .max(1)
    }

    /// The wave climbs out of its lit bays.
    fn release_wave(&mut self, fort: &Fort, output: &mut TickOutput) {
        let pylon = fort.pylon();
        let rigs = self.rigs_holding(fort);
        let Some(state) = self.garrison.forts.get(&pylon) else {
            return;
        };
        let roster = wave_roster(fort.kind, state.wave, rigs, fort.along);
        let bays = if state.lit_bays.is_empty() {
            fort.bay_spawns()
        } else {
            state.lit_bays.clone()
        };
        let tier = self.tier_at(pylon);
        let frame = self.frame();
        let target = self.nearest_rig_to(pylon, fort);
        let mut machines = Vec::new();
        for (index, kind) in roster.into_iter().enumerate() {
            let bay = bays[index % bays.len()];
            let (x, y) = frame.tile_center(bay);
            #[allow(clippy::cast_precision_loss, reason = "a handful of machines")]
            let nudge = ((index / bays.len()) as f32 - 0.5) * TILE_SIZE * 0.8;
            let id = self.spawn_enemy_with(kind, (x + nudge, y), tier, None, None);
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(machine) = &mut entity.state {
                    machine.mode = EnemyMode::Chase;
                    machine.target = target;
                    machine.last_seen = self.simulation_tick;
                }
            });
            machines.push(id.get());
        }
        if let Some(state) = self.garrison.forts.get_mut(&pylon) {
            state.machines = machines;
            state.phase = FortPhase::Wave;
        }
        output.push_event(garrison_event(GarrisonEvent::WaveWoke, None, pylon));
    }

    /// The living rig nearest `tile` holding `fort`.
    fn nearest_rig_to(&self, tile: TilePosition, fort: &Fort) -> Option<PlayerId> {
        let hold = fort.bounds.expanded(crate::garrison::FORT_REACH_TILES);
        self.living_rigs()
            .into_iter()
            .filter(|(_, at)| hold.contains(*at))
            .min_by_key(|(id, at)| ((at.x - tile.x).abs() + (at.y - tile.y).abs(), *id))
            .map(|(id, _)| id)
    }

    /// Nobody held the fort: its machines go back into their bays and the waves start over.
    fn stand_down(&mut self, pylon: TilePosition, output: &mut TickOutput) {
        let Some(state) = self.garrison.forts.remove(&pylon) else {
            return;
        };
        for id in state.machines {
            self.entities.despawn(EntityId(id));
        }
        output.push_event(garrison_event(GarrisonEvent::StoodDown, None, pylon));
    }

    /// The garrison sleeps: every rig holding the fort is paid from its armoury.
    fn pay_armoury(&mut self, fort: &Fort, output: &mut TickOutput) {
        let pylon = fort.pylon();
        let hold = fort.bounds.expanded(crate::garrison::FORT_REACH_TILES);
        let (credits, plate, alloy) = fort.kind.armoury();
        let credits = scaled_price(credits, self.descent_of_row(pylon.y));
        let holders: Vec<PlayerId> = self
            .living_rigs()
            .into_iter()
            .filter(|(_, tile)| hold.contains(*tile))
            .map(|(id, _)| id)
            .collect();
        let (x, y) = self.frame().tile_center(TilePosition {
            x: pylon.x,
            y: pylon.y - 3,
        });
        for player_id in holders {
            let mut spilled = Vec::new();
            if let Some(player) = self.players.get_mut(&player_id) {
                player.credits = player.credits.saturating_add(credits);
                for (kind, count) in [
                    (ItemKind::Layer(Stratum::SleepingGarrison), plate),
                    (
                        ItemKind::Material(crate::terrain::StrategicResourceKind::AncientAlloy),
                        alloy,
                    ),
                ] {
                    let stowed = player.stow(kind, count);
                    if stowed < count {
                        spilled.push(crate::item::ItemStack::new(kind, count - stowed));
                    }
                }
            }
            for stack in spilled {
                let _drop = self.spawn_loot(x, y, (0.0, -120.0), stack, None);
            }
            output.push_event(garrison_event(
                GarrisonEvent::GarrisonSlept,
                Some(player_id),
                pylon,
            ));
        }
    }

    /// Turrets shoot, traps zap, machines batter (step 2).
    #[allow(clippy::too_many_lines, reason = "each defence's turn in one pass")]
    fn advance_defences(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let now = self.simulation_tick;
        let frame = self.frame();
        let ids: Vec<u32> = self.garrison.defences.keys().copied().collect();
        for id in ids {
            let Some(defence) = self.garrison.defences.get(&id).cloned() else {
                continue;
            };
            match defence.kind {
                DefenceKind::Turret if defence.ready <= now => {
                    let (hx, hy) = frame.tile_center(defence.top());
                    let reach = TURRET_RANGE_TILES * TILE_SIZE;
                    let target = self
                        .enemies()
                        .filter(|(_, state)| {
                            !self.living_peaceful(state) && !Self::hollow_peaceful(state)
                        })
                        .filter(|(entity, _)| {
                            (entity.body.x - hx).hypot(entity.body.y - hy) <= reach
                        })
                        .filter(|(entity, _)| {
                            crate::physics::sight_clear(
                                &self.terrain,
                                (hx, hy),
                                (entity.body.x, entity.body.y),
                            )
                        })
                        .min_by(|(a, _), (b, _)| {
                            let da = (a.body.x - hx).hypot(a.body.y - hy);
                            let db = (b.body.x - hx).hypot(b.body.y - hy);
                            da.total_cmp(&db).then(a.id.cmp(&b.id))
                        })
                        .map(|(entity, _)| (entity.id, entity.body.x, entity.body.y));
                    let Some((target, tx, ty)) = target else {
                        continue;
                    };
                    let (dx, dy) = (tx - hx, ty - hy);
                    let distance = dx.hypot(dy).max(1.0);
                    let _outcome = self.damage_enemy(
                        target,
                        Damage {
                            amount: TURRET_DAMAGE,
                            kind: DamageKind::Kinetic,
                            source: DamageSource::Player(defence.owner),
                            impulse: (dx / distance * 60.0, dy / distance * 60.0),
                        },
                        output,
                    );
                    let ready = self.after_millis(now, TURRET_PERIOD_MILLIS);
                    if let Some(turret) = self.garrison.defences.get_mut(&id) {
                        turret.ready = ready;
                        turret.fired = Some(ShotTrace {
                            tick: now,
                            to: frame.tile_at(tx, ty),
                        });
                    }
                    output.push_event(garrison_event(
                        GarrisonEvent::TurretFired,
                        Some(defence.owner),
                        defence.top(),
                    ));
                }
                DefenceKind::ShockTrap if defence.armed(now) => {
                    let left = frame.col_px(defence.anchor.x);
                    let top = frame.row_px(defence.anchor.y);
                    let caught = self
                        .enemies()
                        .filter(|(entity, state)| {
                            let (half_w, half_h) = state.kind.contact_half();
                            let radius = state.kind.stats().radius;
                            let half_h = half_h.max(radius);
                            entity.body.x + half_w > left
                                && entity.body.x - half_w < left + TILE_SIZE
                                && entity.body.y + half_h > top
                                && entity.body.y - half_h < top + TILE_SIZE
                                && !state.kind.is_colossus_part()
                        })
                        .map(|(entity, state)| (entity.id, state.kind.is_machine()))
                        .collect::<Vec<_>>();
                    if caught.is_empty() {
                        continue;
                    }
                    for (target, machine) in caught {
                        let _outcome = self.damage_enemy(
                            target,
                            Damage {
                                amount: TRAP_DAMAGE,
                                kind: if machine {
                                    DamageKind::Emp
                                } else {
                                    DamageKind::Kinetic
                                },
                                source: DamageSource::Player(defence.owner),
                                impulse: (0.0, -120.0),
                            },
                            output,
                        );
                    }
                    let ready = self.after_millis(now, TRAP_REARM_MILLIS);
                    if let Some(trap) = self.garrison.defences.get_mut(&id) {
                        trap.ready = ready;
                    }
                    output.push_event(garrison_event(
                        GarrisonEvent::TrapSprung,
                        Some(defence.owner),
                        defence.anchor,
                    ));
                }
                _ => {}
            }
        }
        if self.timing.rate.schedule_fires(next_tick, BATTER_MILLIS, 0) {
            self.batter_defences(output);
        }
    }

    /// Garrison machines batter the defences they reach.
    fn batter_defences(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        let blows: Vec<(u32, u16)> = self
            .enemies()
            .filter(|(_, state)| batter_damage(state.kind) > 0 && state.mode.hostile())
            .flat_map(|(entity, state)| {
                let (half_w, half_h) = state.kind.contact_half();
                let reach = BATTER_REACH_TILES * TILE_SIZE;
                let (x, y) = (entity.body.x, entity.body.y);
                self.garrison
                    .defences
                    .iter()
                    .filter(move |(_, defence)| {
                        let (_, height) = defence.kind.footprint();
                        let left = frame.col_px(defence.anchor.x);
                        #[allow(clippy::cast_precision_loss, reason = "a tile or two")]
                        let top = frame.row_px(defence.anchor.y - height + 1);
                        let right = left + TILE_SIZE;
                        let bottom = frame.row_px(defence.anchor.y) + TILE_SIZE;
                        let gap_x = (left - (x + half_w)).max((x - half_w) - right).max(0.0);
                        let gap_y = (top - (y + half_h)).max((y - half_h) - bottom).max(0.0);
                        gap_x <= reach && gap_y <= reach
                    })
                    .map(move |(id, _)| (*id, batter_damage(state.kind)))
                    .take(1)
            })
            .collect();
        for (id, damage) in blows {
            self.hurt_defence(id, damage, output);
        }
    }

    /// The Seal Gate shakes: stones gather dust over the rigs in it (step 3).
    fn advance_tremors(&mut self, output: &mut TickOutput) {
        let Some(layout) = self.descent_strata_layout() else {
            return;
        };
        let now = self.simulation_tick;
        let hall = self.colossus_hall();
        let shaken: Vec<(PlayerId, TilePosition)> = self
            .living_rigs()
            .into_iter()
            .filter(|(_, tile)| self.deep_stratum_at(*tile) == Some(Stratum::TheSealGate))
            .filter(|(_, tile)| !hall.is_some_and(|hall| hall.contains(*tile)))
            .collect();
        let Some((_, first)) = shaken.first().copied() else {
            return;
        };
        if self.garrison.next_tremor == SimulationTick::new(0) {
            self.garrison.next_tremor =
                self.after_millis(now, tremor_period(layout.along(first.y)));
            return;
        }
        if now < self.garrison.next_tremor {
            return;
        }
        self.garrison.last_tremor = Some(now);
        self.garrison.next_tremor = self.after_millis(now, tremor_period(layout.along(first.y)));
        output.push_event(garrison_event(GarrisonEvent::Tremor, None, first));
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let lands = self.after_millis(now, TREMOR_TELL_MILLIS);
        let seed = self.terrain.seed();
        for (player_id, tile) in shaken {
            let stones = tremor_stones(layout.along(tile.y));
            let roll = hash2(
                seed,
                SALT_TREMOR,
                i32::try_from(now.get() & 0x7FFF_FFFF).unwrap_or(0),
                i32::try_from(player_id.get()).unwrap_or(0),
            );
            for index in 0..stones {
                // Spread round the rig, one stone right over it.
                let offsets = [0, -2, 2, -4, 4];
                let jitter = i32::try_from((roll >> (index * 3)) % 3).unwrap_or(0) - 1;
                let column =
                    tile.x + offsets[index % offsets.len()] + jitter * i32::from(index > 0);
                if let Some(drop) = self.ceiling_drop(column, tile.y, lands)
                    && self.garrison.drops.len() < MAX_DROPS
                    && !self.garrison.drops.contains(&drop)
                {
                    self.garrison.drops.push(drop);
                }
            }
        }
    }

    /// A stone falling at `lands` from the first ceiling above row `row` in `column` (within
    /// [`TREMOR_CEILING_TILES`]), if there is one.
    pub(super) fn ceiling_drop(
        &self,
        column: i32,
        row: i32,
        lands: SimulationTick,
    ) -> Option<RubbleDrop> {
        let mut open_below = None;
        for rise in 0..=TREMOR_CEILING_TILES {
            let at = TilePosition {
                x: column,
                y: row - rise,
            };
            if self.terrain.is_solid_at(at) {
                return open_below.map(|(x, y)| RubbleDrop { lands, x, y });
            }
            open_below = Some((at.x, at.y));
        }
        None
    }

    /// Stones whose dust has gathered fall (step 3).
    fn advance_drops(&mut self) {
        let now = self.simulation_tick;
        let (due, waiting): (Vec<RubbleDrop>, Vec<RubbleDrop>) = self
            .garrison
            .drops
            .iter()
            .partition(|drop| drop.lands <= now);
        if due.is_empty() {
            return;
        }
        self.garrison.drops = waiting;
        if !self.enemies.difficulty.hostile() {
            return;
        }
        let frame = self.frame();
        let expires = self.after_millis(now, ShotKind::FallingStone.life_millis());
        for drop in due {
            let (x, y) = frame.tile_center(TilePosition {
                x: drop.x,
                y: drop.y,
            });
            let in_hall = self.colossus_hall().is_some_and(|hall| {
                hall.contains(TilePosition {
                    x: drop.x,
                    y: drop.y,
                })
            });
            let damage = if in_hall {
                crate::garrison::RUBBLE_DAMAGE
            } else {
                TREMOR_STONE_DAMAGE
            };
            let _stone = self.spawn_entity(
                Body::at(x, TILE_SIZE.mul_add(-0.3, y)).moving(0.0, ShotKind::FallingStone.speed()),
                EntityState::EnemyShot(EnemyShotState {
                    kind: ShotKind::FallingStone,
                    damage,
                    shooter: EnemyKind::GatekeeperColossus,
                }),
                Some(expires),
            );
        }
    }

    /// The Seal Gate reads the keys at its door, then opens (step 4).
    fn advance_seal_gate(&mut self, output: &mut TickOutput) {
        if self.garrison.seal.open {
            return;
        }
        let Some(door) = self.seal_door() else {
            return;
        };
        let now = self.simulation_tick;
        if let Some((start, opener)) = self.garrison.seal.opening {
            let since = self.millis_at(now).saturating_sub(self.millis_at(start));
            let before = self
                .millis_at(SimulationTick::new(now.get().saturating_sub(1)))
                .saturating_sub(self.millis_at(start));
            let lit = sockets_lit(since);
            // The first tick after the gate began reads its first socket too.
            let was = if now.get() <= start.get() + 1 {
                0
            } else {
                sockets_lit(before)
            };
            for socket in was..lit {
                let row = door.min.y + 1 + i32::try_from(socket / 2).unwrap_or(0) * 2;
                output.push_event(garrison_event(
                    GarrisonEvent::SocketLit,
                    Some(opener),
                    TilePosition {
                        x: door.min.x,
                        y: row,
                    },
                ));
            }
            let rows = door_rows_open(since, door.height());
            for row in 0..rows {
                for x in door.min.x..=door.max.x {
                    let at = TilePosition {
                        x,
                        y: door.min.y + row,
                    };
                    if self.terrain.tile(at).map(|tile| tile.kind)
                        == Some(TileKind::Cave(CaveTile::SealDoor))
                    {
                        let _ = self.terrain.set_kind_by_rule(at, TileKind::Air);
                    }
                }
            }
            if rows >= door.height() {
                self.garrison.seal.open = true;
            }
            return;
        }
        let reach = SEAL_READ_TILES;
        let near: Vec<PlayerId> = self
            .living_rigs()
            .into_iter()
            .filter(|(_, tile)| {
                tile.y >= door.min.y - 2
                    && tile.y <= door.max.y + 1
                    && tile.x >= door.min.x - reach
                    && tile.x <= door.max.x + reach
            })
            .map(|(id, _)| id)
            .collect();
        self.garrison_schedule
            .refused
            .retain(|player| near.contains(player));
        for player_id in near {
            if self.holds_every_seal_key(player_id) {
                self.garrison.seal.opening = Some((now, player_id));
                self.garrison_schedule.refused.clear();
                output.push_event(garrison_event(
                    GarrisonEvent::SealOpening,
                    Some(player_id),
                    door.center(),
                ));
                return;
            }
            if self.garrison_schedule.refused.insert(player_id) {
                output.push_event(garrison_event(
                    GarrisonEvent::SealRefused,
                    Some(player_id),
                    door.center(),
                ));
            }
        }
    }

    /// The first key `player_id` lacks at the Seal Gate, if any (the refusal says it).
    #[must_use]
    pub fn missing_seal_key(&self, player_id: PlayerId) -> Option<SealKey> {
        self.seal_keys_of(player_id)
            .iter()
            .zip(SealKey::ALL)
            .find(|(held, _)| !**held)
            .map(|(_, key)| key)
    }

    /// A rig reaching a landmark finds it: its story beat is told once (step 5).
    fn note_garrison_landmarks(&mut self, output: &mut TickOutput) {
        let mut finds: Vec<(GarrisonLandmark, PlayerId, TilePosition)> = Vec::new();
        for (player_id, tile) in self.living_rigs() {
            if let Some(landmark) = self.garrison_landmark_at(tile)
                && !self.garrison.found.contains(&landmark)
                && !finds.iter().any(|(found, ..)| *found == landmark)
            {
                finds.push((landmark, player_id, tile));
            }
        }
        for (landmark, player_id, tile) in finds {
            self.garrison.found.insert(landmark);
            output.push_event(garrison_event(
                GarrisonEvent::LandmarkFound,
                Some(player_id),
                tile,
            ));
        }
    }

    /// A rig's body radius (the fists' and stones' reach adds it).
    pub(super) const fn rig_radius() -> f32 {
        AUTHORITATIVE_PLAYER_RADIUS
    }
}
