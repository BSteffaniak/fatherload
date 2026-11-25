//! Hostile factions and space combat in the world (Wave 12 hostiles, see
//! [`crate::hostiles`]): the encounter rolls, lairs, patrols and bounties that put ships near
//! rigs in free flight; the fixed combat steps (the rigs' guns, the ships' AI, the bosses'
//! telegraphed attacks, shots and hits, wrecks and salvage); the fair rules (deflector, the
//! escape jump, the cargo pod a wrecked rig leaves); combat commands; and ground raids on
//! bodies.
//!
//! Space combat runs in the home world pass (rigs under the Star Drive live in the home store,
//! their exact space positions in [`crate::flight::RigFlight::pos`]); raids run in a body's
//! ground pass (its troopers are creatures of the body's store).

#![allow(
    clippy::suboptimal_flops,
    clippy::imprecise_flops,
    reason = "plain IEEE arithmetic: the same bits on every platform (no fused multiply-add or \
              platform hypot)"
)]
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "steps, ids and hashes fold into hash inputs and small counts"
)]

use std::collections::BTreeMap;

use crate::{
    PlayerCommand, TickInputs,
    event::{GameplayEventKind, TickOutput},
    flight::{
        FlightCommand, FlightPlace, StarDrive,
        physics::impulse_top_speed,
        space::{advance, hash64, nearest_surface},
    },
    hostiles::{
        Attack, BossTech, BountyStatus, COMBAT_STEP_SECONDS, CombatCommand, ESCAPE_DAMAGE_SHARE,
        ESCAPE_FUEL, ESCAPE_JUMP_TILES, ESCAPE_SPOOL_STEPS, Encounter, EncounterKind,
        EncounterState, Faction, HostileEvent, HostileShip, MAX_SHIPS, MAX_SHOTS,
        MISSILE_RELOAD_STEPS, PostedBounty, RIG_HIT_RADIUS, Raid, RigCombat, SHIELD_REGEN,
        SHIELD_REGEN_DELAY_STEPS, ShipClass, ShipGearKind, ShipMode, Shooter, Shot, ShotKind,
        Telegraph, WRECK_LIFE_STEPS, WRECK_PICKUP_RANGE, Wreck,
        ai::{self, HAIL_START, PATROL_SIGHT, RigView},
        boss::{self, Unleash},
        combat::{
            self, MISSILE_ACCEL, MISSILE_CONE, MISSILE_LOCK_RANGE, MISSILE_TOP_SPEED, MISSILE_TURN,
            Order, bearing, damage_ship, hits_ship, length, nose, regrow_shield, turn_toward,
        },
        encounter::{
            self, BOUNTY_REACH_TILES, BountyTarget, DEMAND_STEPS, LAIR_REACH_TILES, REST_STEPS,
            ROLL_STEPS, danger_ring,
        },
        missile_capacity, raid, shield_capacity, step_at_tick, tiles,
    },
    ids::{PlayerId, SimulationTick},
    item::ItemStack,
    position::WorldPos,
    summary::PlayerScopedCommandOutcome,
};

use super::WorldState;

/// Where a ship spawns from its rig (px): just off screen, so it flies in.
const SPAWN_RANGE: f32 = 1_250.0;
/// Pirates paid off (or hurt) vanish this far from their rig (px).
const LEAVE_RANGE: f32 = 2_400.0;
/// Pirates open fire if their rig runs this far while they hail (px).
const HAIL_BREAK_RANGE: f32 = 1_800.0;
/// Non-pirate encounters start fighting once a ship is this near (px).
const ENGAGE_RANGE: f32 = 950.0;
/// Steps between a ship's contact hits on one rig.
const CONTACT_STEPS: u64 = 60;
/// Steps between the lair, bounty and patrol checks.
const PLACE_CHECK_STEPS: u64 = 60;

const fn hostile_event(
    what: HostileEvent,
    player_id: Option<PlayerId>,
    detail: u32,
) -> GameplayEventKind {
    GameplayEventKind::Hostile {
        what,
        player_id,
        detail,
    }
}

/// One rig as the combat step sees it this tick.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts about one rig"
)]
struct RigSnap {
    id: PlayerId,
    /// Its space position at the end of the tick (flight already moved it).
    pos: WorldPos,
    vx: f32,
    vy: f32,
    angle: f32,
    /// Ships may shoot it (free flight or approach; not docked, warping or home).
    targetable: bool,
    /// It may roll for encounters (free flight, not on the drive's warp or spool).
    rolls: bool,
    firing: bool,
    aim: (i8, i8),
    screen_relative: bool,
    gear: crate::hostiles::ShipGear,
}

/// Per-tick tallies (events go out once a tick, not once a step).
#[derive(Default)]
struct Tally {
    fired: BTreeMap<PlayerId, ShotKind>,
    ship_hits: BTreeMap<PlayerId, bool>,
    rig_damage: BTreeMap<PlayerId, f32>,
    shield_down: Vec<PlayerId>,
    destroyed: Vec<HostileShip>,
    telegraphs: Vec<(Option<PlayerId>, Attack)>,
    hostile_fired: Vec<(Option<PlayerId>, ShotKind)>,
}

impl WorldState {
    // --- Queries ------------------------------------------------------------------------------

    /// Everything hostile in space: ships, shots, wrecks, encounters, each rig's guns and
    /// deflector, bosses beaten, bounties and raids (Wave 12).
    #[must_use]
    pub const fn hostiles(&self) -> &crate::hostiles::HostileSpace {
        &self.hostiles
    }

    /// Replaces the hostile state (scenes, tests, tools).
    pub fn set_hostiles(&mut self, hostiles: crate::hostiles::HostileSpace) {
        self.hostiles = hostiles;
    }

    /// `player_id`'s live combat state (guns, rack, deflector, escape jump).
    #[must_use]
    pub fn rig_combat(&self, player_id: PlayerId) -> RigCombat {
        self.hostiles
            .rigs
            .get(&player_id)
            .copied()
            .unwrap_or_else(|| self.fresh_combat(player_id))
    }

    fn fresh_combat(&self, player_id: PlayerId) -> RigCombat {
        let gear = self
            .players
            .get(&player_id)
            .map(|player| player.hazard_gear.arms)
            .unwrap_or_default();
        RigCombat {
            missiles: missile_capacity(gear, self.hostiles.has_tech(BossTech::LanceMissiles)),
            shield: shield_capacity(gear),
            ..RigCombat::default()
        }
    }

    /// Where `faction`'s boss waits in this world (none on worlds without space).
    #[must_use]
    pub fn boss_lair(&self, faction: Faction) -> Option<crate::hostiles::Lair> {
        self.space_enabled()
            .then(|| crate::hostiles::boss_lair(self.world_seed(), faction))
            .flatten()
    }

    /// The bounties a station in `system` offers on `day` (the stations track's board reads
    /// these; see [`crate::hostiles::bounty_targets`]).
    #[must_use]
    pub fn bounty_board(&self, system: crate::flight::SystemRef, day: u32) -> Vec<BountyTarget> {
        crate::hostiles::bounty_targets(self.world_seed(), system, day)
            .into_iter()
            .filter(|target| self.bounty_status(target.id) == BountyStatus::Unknown)
            .collect()
    }

    /// Posts `target` for `hunter` (the stations track's board calls this when a player
    /// takes a bounty). The target then lurks at its point until a rig comes near; destroying
    /// it pays `hunter` the reward. Returns whether it was posted (not already posted or
    /// claimed).
    pub fn post_bounty(&mut self, hunter: PlayerId, target: BountyTarget) -> bool {
        if self.bounty_status(target.id) != BountyStatus::Unknown {
            return false;
        }
        self.hostiles.bounties.push(PostedBounty {
            target,
            hunter,
            met: false,
        });
        true
    }

    /// How the bounty `id` stands.
    #[must_use]
    pub fn bounty_status(&self, id: u64) -> BountyStatus {
        if self.hostiles.bounties_claimed.contains(&id) {
            BountyStatus::Claimed
        } else if self
            .hostiles
            .bounties
            .iter()
            .any(|bounty| bounty.target.id == id)
        {
            BountyStatus::Posted
        } else {
            BountyStatus::Unknown
        }
    }

    // --- Commands -----------------------------------------------------------------------------

    /// A command space combat answers: every [`PlayerCommand::Combat`], the warp button in a
    /// fight (the escape jump) and the use button under a pirate demand (pay). `None` lets
    /// the command through.
    pub(super) fn combat_command(
        &mut self,
        player_id: PlayerId,
        command: &PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        match command {
            PlayerCommand::Combat(command) => {
                Some(self.apply_combat_command(player_id, *command, output))
            }
            PlayerCommand::Flight(FlightCommand::Warp)
                if self.hostiles.engaged(player_id).is_some() && self.free_flying(player_id) =>
            {
                Some(self.apply_combat_command(player_id, CombatCommand::EscapeJump, output))
            }
            PlayerCommand::Flight(FlightCommand::Use)
                if self.hostiles.demand_for(player_id).is_some() =>
            {
                Some(self.apply_combat_command(player_id, CombatCommand::PayTribute, output))
            }
            _ => None,
        }
    }

    /// Whether `player_id`'s rig flies free in space (free flight or approach, off the drive's
    /// warp).
    fn free_flying(&self, player_id: PlayerId) -> bool {
        self.players.get(&player_id).is_some_and(|player| {
            matches!(
                player.flight.place,
                FlightPlace::Space | FlightPlace::Approach(_)
            ) && !matches!(player.flight.drive, StarDrive::Warp(_))
        })
    }

    const fn combat_step_now(&self) -> u64 {
        step_at_tick(self.simulation_tick.get(), self.tick_rate_hz())
    }

    fn apply_combat_command(
        &mut self,
        player_id: PlayerId,
        command: CombatCommand,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let step = self.combat_step_now();
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        match command {
            CombatCommand::FireMissile => {
                if !self.free_flying(player_id) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let mut combat = self.rig_combat(player_id);
                if combat.missiles == 0 {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let Some(player) = self.players.get(&player_id) else {
                    return PlayerScopedCommandOutcome::UnknownPlayer;
                };
                let pos = player.flight.pos;
                let angle = player.attitude.orientation;
                let (vx, vy) = (player.flight.velocity_x, player.flight.velocity_y);
                // The lock: the ship nearest the nose inside the cone, in range.
                let lock = self
                    .hostiles
                    .ships
                    .iter()
                    .filter_map(|ship| {
                        let (dx, dy) = pos.delta(ship.pos);
                        let distance = length(dx, dy);
                        let off = crate::field::trig::wrap_angle(bearing(dx, dy) - angle).abs();
                        (distance <= MISSILE_LOCK_RANGE && off <= MISSILE_CONE)
                            .then_some((off, ship.id))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                    .map(|(_, id)| id);
                let (nx, ny) = nose(angle);
                let id = self.hostiles.take_id();
                let lance = self.hostiles.has_tech(BossTech::LanceMissiles);
                self.hostiles.shots.push(Shot {
                    id,
                    kind: ShotKind::Missile,
                    from: Shooter::Rig(player_id),
                    pos: pos.offset(nx * 20.0, ny * 20.0),
                    vx: vx + nx * ShotKind::Missile.speed(),
                    vy: vy + ny * ShotKind::Missile.speed(),
                    expires: step + ShotKind::Missile.life_steps(),
                    lock,
                    damage: combat::missile_damage(lance),
                });
                combat.missiles -= 1;
                if combat.reload_at <= step {
                    combat.reload_at = step + MISSILE_RELOAD_STEPS;
                }
                self.hostiles.rigs.insert(player_id, combat);
                output.push_event(hostile_event(
                    HostileEvent::Fired,
                    Some(player_id),
                    ShotKind::Missile.code(),
                ));
                PlayerScopedCommandOutcome::Applied
            }
            CombatCommand::PayTribute => self.pay_tribute(player_id, output),
            CombatCommand::EscapeJump => {
                if !self.free_flying(player_id) {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                let mut combat = self.rig_combat(player_id);
                if combat.escape.is_some() {
                    return PlayerScopedCommandOutcome::IgnoredUnavailable;
                }
                combat.escape = Some(step);
                self.hostiles.rigs.insert(player_id, combat);
                // The drive's own cruise or spool gives way to the jump.
                if let Some(player) = self.players.get_mut(&player_id)
                    && !matches!(player.flight.drive, StarDrive::Warp(_))
                {
                    player.flight.drive = StarDrive::Off;
                }
                output.push_event(hostile_event(
                    HostileEvent::EscapeSpooling,
                    Some(player_id),
                    0,
                ));
                PlayerScopedCommandOutcome::Applied
            }
            CombatCommand::BuyGear(kind) => self.buy_ship_gear(player_id, kind, output),
        }
    }

    fn buy_ship_gear(
        &mut self,
        player_id: PlayerId,
        kind: ShipGearKind,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if !matches!(player.flight.place, FlightPlace::Docked(_)) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let gear = &mut player.hazard_gear.arms;
        let level = kind.level(*gear);
        if level >= kind.max_level() {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let price = kind.price(level);
        if player.credits < price {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= price;
        match kind {
            ShipGearKind::Cannon => gear.cannon += 1,
            ShipGearKind::Turret => gear.turret += 1,
            ShipGearKind::Missiles => gear.missiles += 1,
            ShipGearKind::Shield => gear.shield += 1,
        }
        // A new rack or deflector comes charged.
        let fresh = self.fresh_combat(player_id);
        let combat = self.hostiles.rigs.entry(player_id).or_default();
        combat.missiles = fresh.missiles;
        combat.shield = fresh.shield;
        output.push_event(hostile_event(
            HostileEvent::GearBought,
            Some(player_id),
            kind.code(),
        ));
        output.push_event(GameplayEventKind::CargoChanged { player_id });
        PlayerScopedCommandOutcome::Applied
    }

    /// Pays the pirates' demand: credits first, then the most valuable cargo for the rest.
    fn pay_tribute(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some(index) = self.hostiles.encounters.iter().position(|encounter| {
            encounter.target == player_id
                && matches!(encounter.state, EncounterState::Demand { .. })
        }) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let EncounterState::Demand { credits, .. } = self.hostiles.encounters[index].state else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let paid = player.credits.min(credits);
        player.credits -= paid;
        let mut owed = credits - paid;
        // The rest in cargo, the dearest first.
        let mut stacks: Vec<ItemStack> = player.cargo_bay.stacks().collect();
        stacks.sort_by(|a, b| {
            b.kind
                .base_value()
                .cmp(&a.kind.base_value())
                .then(a.kind.cmp(&b.kind))
        });
        for stack in stacks {
            if owed == 0 {
                break;
            }
            let value = stack.kind.base_value().max(1);
            let take = stack.count.min(owed.div_ceil(value));
            let taken = player.cargo_bay.remove(stack.kind, take);
            owed = owed.saturating_sub(taken.saturating_mul(value));
        }
        let encounter = &mut self.hostiles.encounters[index];
        encounter.state = EncounterState::Paid;
        let id = encounter.id;
        for ship in &mut self.hostiles.ships {
            if ship.encounter == id {
                ship.mode = ShipMode::Leaving;
            }
        }
        output.push_event(hostile_event(
            HostileEvent::TributePaid,
            Some(player_id),
            credits,
        ));
        output.push_event(GameplayEventKind::CargoChanged { player_id });
        PlayerScopedCommandOutcome::Applied
    }

    /// The pirates' demand of `player_id`: a quarter of the cargo's worth plus a twentieth of
    /// the credits, 150-5,000.
    fn demand_of(&self, player_id: PlayerId) -> u32 {
        let Some(player) = self.players.get(&player_id) else {
            return 150;
        };
        let cargo: u32 = player
            .cargo_bay
            .stacks()
            .map(|stack| stack.kind.base_value().saturating_mul(stack.count))
            .fold(0, u32::saturating_add);
        (cargo / 4 + player.credits / 20).clamp(150, 5_000)
    }

    // --- Test mode ----------------------------------------------------------------------------

    /// Test mode: call `faction`'s encounter (or its boss) on `player_id`'s rig in space.
    pub(super) fn test_spawn_hostiles(
        &mut self,
        player_id: PlayerId,
        faction: Faction,
        boss: bool,
    ) -> PlayerScopedCommandOutcome {
        if !self.free_flying(player_id) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let step = self.combat_step_now();
        let kind = if boss {
            EncounterKind::Boss
        } else {
            EncounterKind::Roaming
        };
        let ships = if boss {
            vec![faction.boss()]
        } else {
            encounter::compose(
                faction,
                8,
                hash64(self.world_seed(), 0x7E57, step as i64, 0),
            )
        };
        let mut output = TickOutput::default();
        self.start_encounter(player_id, faction, kind, &ships, 3, step, &mut output);
        PlayerScopedCommandOutcome::Applied
    }

    // --- Encounters ---------------------------------------------------------------------------

    /// Starts an encounter of `ships` against `player_id`'s rig, just off its screen (ahead
    /// and to one side). A cruising rig is pulled out of cruise.
    #[allow(clippy::too_many_arguments, reason = "an encounter's whole make-up")]
    fn start_encounter(
        &mut self,
        player_id: PlayerId,
        faction: Faction,
        kind: EncounterKind,
        ships: &[ShipClass],
        danger: u8,
        step: u64,
        output: &mut TickOutput,
    ) -> Option<u32> {
        let player = self.players.get(&player_id)?;
        let pos = player.flight.pos;
        let heading = player.attitude.orientation;
        let cruising = matches!(player.flight.drive, StarDrive::Cruise { .. });
        let top = impulse_top_speed(crate::player::engine_power(player.engine_level));
        if self.hostiles.ships.len() + ships.len() > MAX_SHIPS {
            return None;
        }
        let roll = hash64(
            self.world_seed(),
            0xE4C0,
            step as i64,
            i64::from(player_id.get() as u32),
        );
        #[allow(clippy::cast_precision_loss, reason = "a hash share")]
        let side = ((roll & 0xFFFF) as f32 / 65_536.0 - 0.5) * 2.4;
        let from_angle = heading + side;
        let (fx, fy) = nose(from_angle);
        let id = self.hostiles.take_id();
        let encounter = Encounter {
            id,
            faction,
            kind,
            target: player_id,
            state: EncounterState::Closing,
            danger: danger.max(1),
            origin: pos,
            started: step,
        };
        self.hostiles.encounters.push(encounter);
        let boss_range = if kind == EncounterKind::Boss {
            1.5
        } else {
            1.0
        };
        let centre = pos.offset(fx * SPAWN_RANGE * boss_range, fy * SPAWN_RANGE * boss_range);
        let (vx, vy) = if cruising {
            (0.0, 0.0)
        } else {
            (player.flight.velocity_x, player.flight.velocity_y)
        };
        let face = bearing(-fx, -fy);
        for (index, class) in ships.iter().enumerate() {
            let slot = u8::try_from(index).unwrap_or(u8::MAX);
            // A loose cluster facing the rig (a formation lines up behind its leader).
            let (ox, oy) = if *class == ShipClass::Lancer || index == 0 {
                (0.0, 0.0)
            } else {
                let (sx, sy) = ai::slot_offset(slot.saturating_sub(1));
                combat::from_ship_frame(face, sx, sy)
            };
            let ship_id = self.hostiles.take_id();
            let mut ship = HostileShip::new(ship_id, *class, centre.offset(ox, oy), face, id);
            ship.vx = vx;
            ship.vy = vy;
            ship.target = Some(player_id);
            ship.slot = slot.saturating_sub(1);
            ship.mode = if kind == EncounterKind::Patrol {
                ShipMode::Patrolling
            } else {
                ShipMode::Closing
            };
            ship.next_attack = step + crate::hostiles::steps(1.5);
            self.hostiles.ships.push(ship);
        }
        if cruising && let Some(player) = self.players.get_mut(&player_id) {
            player.flight.drive = StarDrive::Off;
            let speed = player.flight.speed();
            if speed > top && speed > 0.0 {
                player.flight.velocity_x *= top / speed;
                player.flight.velocity_y *= top / speed;
            }
            output.push_event(hostile_event(HostileEvent::Interdicted, Some(player_id), 0));
        }
        output.push_event(hostile_event(
            HostileEvent::EncounterStarted,
            Some(player_id),
            faction.code(),
        ));
        if kind == EncounterKind::Boss {
            output.push_event(hostile_event(
                HostileEvent::BossAppeared,
                Some(player_id),
                faction.boss().code(),
            ));
        }
        Some(id)
    }

    /// The rigs combat sees this tick.
    fn rig_snaps(&self, inputs: &TickInputs) -> Vec<RigSnap> {
        self.players
            .iter()
            .filter(|(_, player)| player.flight.away())
            .map(|(id, player)| {
                let flight = &player.flight;
                let free = matches!(flight.place, FlightPlace::Space | FlightPlace::Approach(_));
                let warping = matches!(flight.drive, StarDrive::Warp(_));
                let controls = inputs
                    .players
                    .get(id)
                    .and_then(|input| input.controls)
                    .unwrap_or_default();
                RigSnap {
                    id: *id,
                    pos: flight.pos,
                    vx: flight.velocity_x,
                    vy: flight.velocity_y,
                    angle: player.attitude.orientation,
                    targetable: free && !warping && player.hull > 0.0,
                    rolls: free
                        && !warping
                        && !matches!(flight.drive, StarDrive::Spool { .. })
                        && player.hull > 0.0,
                    firing: controls.weapon.fire && free && !warping,
                    aim: (controls.weapon.aim_x, controls.weapon.aim_y),
                    screen_relative: controls.flight().screen_relative,
                    gear: player.hazard_gear.arms,
                }
            })
            .collect()
    }

    /// Space combat's world pass (after the rigs flew): new encounters, the combat steps of
    /// this tick, then each encounter's course.
    pub(super) fn advance_hostiles(
        &mut self,
        inputs: &TickInputs,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        if !self.space_enabled() {
            return;
        }
        let rigs = self.rig_snaps(inputs);
        if rigs.is_empty() && self.hostiles.ships.is_empty() && self.hostiles.shots.is_empty() {
            return;
        }
        let hz = self.tick_rate_hz();
        let first = step_at_tick(next_tick.get().saturating_sub(1), hz);
        let last = step_at_tick(next_tick.get(), hz);
        // Rigs that left space are out of every fight (their combat state stays).
        self.find_encounters(&rigs, first, last, output);
        let mut tally = Tally::default();
        for step in first..last {
            let remaining = (last - step - 1) as f32 * COMBAT_STEP_SECONDS;
            let at: Vec<(RigSnap, WorldPos)> = rigs
                .iter()
                .map(|rig| {
                    (
                        *rig,
                        rig.pos.offset(-rig.vx * remaining, -rig.vy * remaining),
                    )
                })
                .collect();
            self.rig_guns(&at, step, &mut tally);
            self.fly_ships(&at, step, &mut tally);
            self.fly_shots(&at, step, &mut tally);
            self.drift_wrecks(&at, step, output);
            self.rig_upkeep(&rigs, step);
        }
        self.settle_hits(&rigs, &mut tally, last, output);
        self.run_escapes(&rigs, last, output);
        self.resolve_encounters(&rigs, last, output);
    }

    /// Rolls for new encounters, and meets lairs, bounties and patrols.
    #[allow(clippy::too_many_lines, reason = "lairs, bounties, patrols and rolls in order")]
    fn find_encounters(
        &mut self,
        rigs: &[RigSnap],
        first: u64,
        last: u64,
        output: &mut TickOutput,
    ) {
        // Checks fall on fixed steps (multiples of their period), so every tick rate makes
        // the same rolls at the same steps.
        let on = |period: u64| {
            let next = first.div_ceil(period) * period;
            (next < last).then_some(next)
        };
        let place_step = on(PLACE_CHECK_STEPS);
        let roll_step = on(ROLL_STEPS);
        if place_step.is_none() && roll_step.is_none() {
            return;
        }
        let space = self.space();
        let seed = self.world_seed();
        for rig in rigs {
            if !rig.rolls || self.hostiles.engaged(rig.id).is_some() {
                continue;
            }
            let ring = danger_ring(rig.pos);
            if ring == 0 {
                continue;
            }
            let combat = self.rig_combat(rig.id);
            if let Some(step) = place_step {
                // Bosses in their lairs (not beaten yet).
                let mut met = false;
                for faction in Faction::ALL {
                    if met
                        || self.hostiles.bosses_beaten.contains(&faction)
                        || i64::from(ring) < encounter::lair_ring(faction)
                    {
                        continue;
                    }
                    let Some(lair) = crate::hostiles::boss_lair(seed, faction) else {
                        continue;
                    };
                    if rig.pos.distance(lair.pos) < tiles(LAIR_REACH_TILES) {
                        let escorts: Vec<ShipClass> = std::iter::once(faction.boss()).collect();
                        met = self
                            .start_encounter(
                                rig.id,
                                faction,
                                EncounterKind::Boss,
                                &escorts,
                                ring,
                                step,
                                output,
                            )
                            .is_some();
                    }
                }
                if met {
                    continue;
                }
                // Bounty targets (anyone may meet one; the hunter is paid).
                if let Some(index) = self.hostiles.bounties.iter().position(|bounty| {
                    !bounty.met && rig.pos.distance(bounty.target.pos) < tiles(BOUNTY_REACH_TILES)
                }) {
                    let target = self.hostiles.bounties[index].target.clone();
                    let mut ships = vec![target.class];
                    let escort = match target.faction {
                        Faction::Pirates => ShipClass::Cutter,
                        Faction::Swarm => ShipClass::Mite,
                        Faction::Machine => ShipClass::Drone,
                    };
                    ships.extend(std::iter::repeat_n(escort, usize::from(target.escorts)));
                    if let Some(id) = self.start_encounter(
                        rig.id,
                        target.faction,
                        EncounterKind::Bounty(target.id),
                        &ships,
                        ring,
                        step,
                        output,
                    ) {
                        self.hostiles.bounties[index].met = true;
                        if let Some(ship) = self
                            .hostiles
                            .ships
                            .iter_mut()
                            .find(|ship| ship.encounter == id && ship.class == target.class)
                        {
                            ship.bounty = Some(target.id);
                        }
                    }
                    continue;
                }
                // Machine patrols on their routes.
                if !encounter::calm_at(&space, rig.pos)
                    && let Some(patrol) =
                        encounter::patrols_near(&space, rig.pos)
                            .into_iter()
                            .find(|patrol| {
                                encounter::meets_patrol(patrol, rig.pos)
                                    && !self
                                        .hostiles
                                        .patrols_met
                                        .contains(&encounter::patrol_key(patrol.id, step))
                            })
                {
                    let ships = encounter::compose(
                        Faction::Machine,
                        2 + patrol.size,
                        hash64(seed, patrol.id, step as i64, 0),
                    );
                    if self
                        .start_encounter(
                            rig.id,
                            Faction::Machine,
                            EncounterKind::Patrol,
                            &ships,
                            ring,
                            step,
                            output,
                        )
                        .is_some()
                    {
                        self.hostiles
                            .patrols_met
                            .insert(encounter::patrol_key(patrol.id, step));
                    }
                    continue;
                }
            }
            let Some(step) = roll_step else {
                continue;
            };
            // Resting after a fight.
            if step < combat.next_roll {
                continue;
            }
            if encounter::calm_at(&space, rig.pos) {
                continue;
            }
            let roll = hash64(seed, 0xE11C, step as i64, i64::from(rig.id.get() as u32));
            let tier = self.pacing_tier(rig.id);
            if roll % 1_000 >= u64::from(encounter::chance_permille(ring, tier)) {
                continue;
            }
            let faction = encounter::choose_faction(&space, rig.pos, roll >> 12);
            let ships = encounter::compose(faction, encounter::budget(ring, tier), roll >> 24);
            self.start_encounter(
                rig.id,
                faction,
                EncounterKind::Roaming,
                &ships,
                ring,
                step,
                output,
            );
        }
    }

    /// A rig's tier for pacing: its ship gear plus a share of its hull and engine upgrades.
    fn pacing_tier(&self, player_id: PlayerId) -> u8 {
        self.players.get(&player_id).map_or(0, |player| {
            player
                .hazard_gear
                .arms
                .tier()
                .saturating_add(player.hull_level / 3)
                .saturating_add(player.engine_level / 3)
                .min(12)
        })
    }

    /// The rigs' guns for one step: the cannon down the nose, the turret along the seat's aim
    /// (relative to the craft). No auto-aim.
    fn rig_guns(&mut self, at: &[(RigSnap, WorldPos)], step: u64, tally: &mut Tally) {
        let twin = self.hostiles.has_tech(BossTech::CorsairTurret);
        for (rig, pos) in at {
            if !rig.firing || !rig.targetable {
                if let Some(combat) = self.hostiles.rigs.get_mut(&rig.id) {
                    combat.firing = false;
                }
                continue;
            }
            let mut combat = self.rig_combat(rig.id);
            combat.firing = true;
            let (nx, ny) = nose(rig.angle);
            if step >= combat.cannon_at && self.hostiles.shots.len() < MAX_SHOTS {
                let (damage, every) = combat::cannon(rig.gear);
                let id = self.hostiles.take_id();
                self.hostiles.shots.push(Shot {
                    id,
                    kind: ShotKind::Cannon,
                    from: Shooter::Rig(rig.id),
                    pos: pos.offset(nx * 18.0, ny * 18.0),
                    vx: rig.vx + nx * ShotKind::Cannon.speed(),
                    vy: rig.vy + ny * ShotKind::Cannon.speed(),
                    expires: step + ShotKind::Cannon.life_steps(),
                    lock: None,
                    damage,
                });
                combat.cannon_at = step + every;
                tally.fired.entry(rig.id).or_insert(ShotKind::Cannon);
            }
            if let Some((damage, every, barrels)) = combat::turret(rig.gear, twin)
                && step >= combat.turret_at
                && self.hostiles.shots.len() + usize::from(barrels) <= MAX_SHOTS
            {
                // The seat's aim, relative to the craft (the camera is locked to it), or along
                // the chart in the screen-relative scheme; with no aim, forward.
                let angle = if rig.aim == (0, 0) {
                    rig.angle
                } else {
                    let local = bearing(f32::from(rig.aim.0), f32::from(rig.aim.1));
                    if rig.screen_relative {
                        local
                    } else {
                        rig.angle + local
                    }
                };
                let (tx, ty) = nose(angle);
                for barrel in 0..barrels {
                    let side = if barrels == 1 {
                        0.0
                    } else if barrel == 0 {
                        -5.0
                    } else {
                        5.0
                    };
                    let id = self.hostiles.take_id();
                    self.hostiles.shots.push(Shot {
                        id,
                        kind: ShotKind::Turret,
                        from: Shooter::Rig(rig.id),
                        pos: pos.offset(tx * 10.0 - ty * side, ty * 10.0 + tx * side),
                        vx: rig.vx + tx * ShotKind::Turret.speed(),
                        vy: rig.vy + ty * ShotKind::Turret.speed(),
                        expires: step + ShotKind::Turret.life_steps(),
                        lock: None,
                        damage,
                    });
                }
                combat.turret_at = step + every;
                tally.fired.entry(rig.id).or_insert(ShotKind::Turret);
            }
            self.hostiles.rigs.insert(rig.id, combat);
        }
    }

    /// One step of every ship: its order (faction AI, boss patterns, patrol routes), its
    /// telegraphs, its motion, its gun and its body's contact.
    #[allow(clippy::too_many_lines, reason = "one ship's step in order")]
    fn fly_ships(&mut self, at: &[(RigSnap, WorldPos)], step: u64, tally: &mut Tally) {
        let seed = self.world_seed();
        let count = self.hostiles.ships.len();
        let mut launches: Vec<(u32, ShipClass, u8, usize, WorldPos, f32)> = Vec::new();
        for index in 0..count {
            let ship = self.hostiles.ships[index];
            let Some(encounter) = self.hostiles.encounter(ship.encounter).copied() else {
                continue;
            };
            let rig = ship
                .target
                .and_then(|target| at.iter().find(|(rig, _)| rig.id == target))
                .filter(|(rig, _)| rig.targetable)
                .map(|(rig, pos)| RigView {
                    player: rig.id,
                    pos: *pos,
                    vx: rig.vx,
                    vy: rig.vy,
                });
            let mates: Vec<HostileShip> = self
                .hostiles
                .ships
                .iter()
                .filter(|other| other.encounter == ship.encounter)
                .copied()
                .collect();
            let leader = mates
                .iter()
                .filter(|other| other.class == ShipClass::Lancer)
                .min_by_key(|other| other.id)
                .copied();
            let mut ship = ship;
            let charging = match ship.mode {
                ShipMode::Charging { until } if step < until => true,
                ShipMode::Charging { .. } => {
                    ship.mode = ShipMode::Attacking;
                    false
                }
                _ => false,
            };
            let mut order = if ship.class.is_boss() {
                rig.as_ref()
                    .map_or_else(Order::default, |rig| boss::order(&ship, rig))
            } else if ship.mode == ShipMode::Patrolling
                && leader.is_none_or(|leader| leader.id == ship.id)
            {
                patrol_order(&ship, &encounter)
            } else {
                ai::order(
                    &ship,
                    &encounter,
                    rig.as_ref(),
                    &mates,
                    leader.as_ref(),
                    step,
                    seed,
                )
            };
            if order.face == 0.0 && order.ax == 0.0 && order.ay == 0.0 && rig.is_none() {
                order.face = ship.angle;
            }
            // A machine patrol notices a rig in sight.
            if ship.mode == ShipMode::Patrolling
                && let Some(rig) = &rig
            {
                let fog = self.space().space().fog_at(rig.pos);
                let sight = if fog > 0 {
                    PATROL_SIGHT * 0.5
                } else {
                    PATROL_SIGHT
                };
                if ship.pos.distance(rig.pos) < sight {
                    let id = ship.encounter;
                    for other in &mut self.hostiles.ships {
                        if other.encounter == id {
                            other.mode = ShipMode::Attacking;
                        }
                    }
                    ship.mode = ShipMode::Attacking;
                    if let Some(found) = self.hostiles.encounters.iter_mut().find(|e| e.id == id) {
                        found.state = EncounterState::Fighting;
                    }
                }
            }
            // Telegraphs: a lancer's lance; a boss's cycle.
            let mut charge_speed = None;
            if let Some(rig) = &rig {
                if ship.class.is_boss() && !charging {
                    if ship.telegraph.is_none()
                        && step >= ship.next_attack
                        && encounter.state == EncounterState::Fighting
                    {
                        let telegraph = boss::begin(&ship, rig, step);
                        tally.telegraphs.push((Some(rig.player), telegraph.attack));
                        ship.telegraph = Some(telegraph);
                    }
                    if let Some(telegraph) = ship.telegraph
                        && step >= telegraph.fire_at
                    {
                        match boss::unleash(&ship, &telegraph, rig, seed) {
                            Unleash::Shots(volleys) => {
                                let scale = encounter.damage_scale();
                                if let Some(first) = volleys.first() {
                                    tally.hostile_fired.push((Some(rig.player), first.kind));
                                }
                                for volley in volleys {
                                    self.spawn_hostile_shot(
                                        &ship,
                                        volley.kind,
                                        volley.from,
                                        volley.angle,
                                        scale,
                                        step,
                                    );
                                }
                            }
                            Unleash::Launch(class, many, most) => {
                                launches.push((
                                    ship.encounter,
                                    class,
                                    many,
                                    most,
                                    ship.pos,
                                    ship.angle,
                                ));
                            }
                            Unleash::Charge { speed, steps } => {
                                if telegraph.attack == Attack::Lunge {
                                    let (dx, dy) = ship.pos.delta(telegraph.mark);
                                    ship.angle = bearing(dx, dy);
                                }
                                ship.mode = ShipMode::Charging {
                                    until: step + steps,
                                };
                                charge_speed = Some(speed);
                            }
                        }
                        ship.telegraph = None;
                        ship.cycle = ship.cycle.wrapping_add(1);
                        ship.phase = boss::phase_of(&ship);
                        ship.next_attack = step + boss::recovery(ship.class, ship.phase);
                    }
                } else if ship.class == ShipClass::Lancer {
                    if let Some(telegraph) = ship.telegraph {
                        // Holds its line while the lance charges.
                        order.face = telegraph.angle;
                        if step >= telegraph.fire_at {
                            let from = combat::from_ship_frame(ship.angle, 0.0, -44.0);
                            self.spawn_hostile_shot(
                                &ship,
                                ShotKind::Lance,
                                from,
                                telegraph.angle,
                                encounter.damage_scale(),
                                step,
                            );
                            tally
                                .hostile_fired
                                .push((Some(rig.player), ShotKind::Lance));
                            ship.telegraph = None;
                            ship.ready_at = step + ship.class.stats().fire_steps;
                        }
                    } else if order.fire && step >= ship.ready_at {
                        ship.telegraph = Some(Telegraph {
                            attack: Attack::Lance,
                            since: step,
                            fire_at: step + ai::lance_warning(),
                            angle: ship.angle,
                            mark: rig.pos,
                        });
                    }
                    order.fire = false;
                }
            }
            if charging || charge_speed.is_some() {
                let charge = charge_speed.unwrap_or_else(|| {
                    if ship.class == ShipClass::SwarmQueen {
                        1_250.0
                    } else {
                        1_350.0
                    }
                });
                combat::move_ship(&mut ship, &order, Some(charge));
            } else {
                combat::move_ship(&mut ship, &order, None);
            }
            regrow_shield(&mut ship, step);
            // Mites dive in turns.
            if ship.class == ShipClass::Mite
                && let Some(rig) = &rig
            {
                if ship.phase == 0 && step >= ship.next_attack && ship.pos.distance(rig.pos) < 520.0
                {
                    ship.phase = 1;
                    ship.next_attack = step + ai::DIVE_STEPS;
                } else if ship.phase == 1 && step >= ship.next_attack {
                    ship.phase = 0;
                    ship.next_attack =
                        step + 200 + hash64(seed, 0xD1FE, i64::from(ship.id), step as i64) % 260;
                }
            }
            // The gun.
            if order.fire
                && step >= ship.ready_at
                && let Some(gun) = ship.class.stats().gun
                && !ship.class.is_boss()
                && ship.class != ShipClass::Lancer
                && encounter.state == EncounterState::Fighting
            {
                let stats = ship.class.stats();
                let from = combat::from_ship_frame(ship.angle, 0.0, -stats.size.1 * 0.5);
                self.spawn_hostile_shot(
                    &ship,
                    gun,
                    from,
                    ship.angle,
                    encounter.damage_scale(),
                    step,
                );
                tally.hostile_fired.push((ship.target, gun));
                if stats.burst > 1 {
                    if ship.burst_left == 0 {
                        ship.burst_left = stats.burst - 1;
                        ship.ready_at = step + 14;
                    } else {
                        ship.burst_left -= 1;
                        ship.ready_at = if ship.burst_left == 0 {
                            step + stats.fire_steps
                        } else {
                            step + 14
                        };
                    }
                } else {
                    ship.ready_at = step + stats.fire_steps;
                }
            }
            // Its body on contact (bites, rams), every rig in reach.
            let contact = ship.class.stats().contact_damage * encounter.damage_scale();
            if encounter.state == EncounterState::Fighting {
                for (other, pos) in at {
                    if !other.targetable {
                        continue;
                    }
                    let (dx, dy) = ship.pos.delta(*pos);
                    if combat::body_touches(ship.class, ship.angle, (dx, dy), RIG_HIT_RADIUS) {
                        let mut combat = self.rig_combat(other.id);
                        if step >= combat.hit_at.max(1) + CONTACT_STEPS || combat.hit_at == 0 {
                            combat.hit_at = step;
                            self.hostiles.rigs.insert(other.id, combat);
                            self.hurt_rig(other.id, contact, step, tally);
                            if ship.class == ShipClass::Mite {
                                ship.phase = 0;
                                ship.next_attack = step + 240;
                            }
                            if matches!(ship.mode, ShipMode::Charging { .. }) {
                                ship.mode = ShipMode::Attacking;
                            }
                        }
                    }
                }
            }
            self.hostiles.ships[index] = ship;
        }
        // Launches (escorts, broods, drones), at most `most` alive of the class.
        for (encounter, class, many, most, pos, angle) in launches {
            let alive = self
                .hostiles
                .ships
                .iter()
                .filter(|ship| ship.encounter == encounter && ship.class == class)
                .count();
            let target = self.hostiles.encounter(encounter).map(|e| e.target);
            for index in 0..usize::from(many).min(most.saturating_sub(alive)) {
                if self.hostiles.ships.len() >= MAX_SHIPS {
                    break;
                }
                let side = if index % 2 == 0 { -1.0 } else { 1.0 };
                let (ox, oy) =
                    combat::from_ship_frame(angle, side * 90.0, 40.0 + 30.0 * index as f32);
                let id = self.hostiles.take_id();
                let mut ship = HostileShip::new(id, class, pos.offset(ox, oy), angle, encounter);
                ship.target = target;
                ship.mode = ShipMode::Attacking;
                ship.slot = u8::try_from(index).unwrap_or(0);
                ship.next_attack = step + 120;
                self.hostiles.ships.push(ship);
            }
        }
        // The dead.
        let mut index = 0;
        while index < self.hostiles.ships.len() {
            if self.hostiles.ships[index].hull <= 0.0 {
                let ship = self.hostiles.ships.remove(index);
                tally.destroyed.push(ship);
                self.wreck_of(&ship, step);
            } else {
                index += 1;
            }
        }
    }

    fn spawn_hostile_shot(
        &mut self,
        ship: &HostileShip,
        kind: ShotKind,
        from: (f32, f32),
        angle: f32,
        scale: f32,
        step: u64,
    ) {
        if self.hostiles.shots.len() >= MAX_SHOTS {
            return;
        }
        let (nx, ny) = nose(angle);
        let id = self.hostiles.take_id();
        // Bosses' slow shells keep the ship's drift; fast bolts fly true.
        let inherit = if kind.speed() < 700.0 { 0.5 } else { 1.0 };
        self.hostiles.shots.push(Shot {
            id,
            kind,
            from: Shooter::Ship(ship.id),
            pos: ship.pos.offset(from.0, from.1),
            vx: ship.vx * inherit + nx * kind.speed(),
            vy: ship.vy * inherit + ny * kind.speed(),
            expires: step + kind.life_steps(),
            lock: None,
            damage: kind.damage() * scale,
        });
    }

    /// Damage `amount` to `player_id`'s rig: the deflector first, then the hull (settled once
    /// a tick); an escape jump spooling halves it.
    fn hurt_rig(&mut self, player_id: PlayerId, amount: f32, step: u64, tally: &mut Tally) {
        let mut combat = self.rig_combat(player_id);
        let amount = if combat.escape.is_some() {
            amount * ESCAPE_DAMAGE_SHARE
        } else {
            amount
        };
        combat.hit_at = step;
        let had_shield = combat.shield > 0.0;
        let soaked = combat.shield.min(amount);
        combat.shield -= soaked;
        if had_shield && combat.shield <= 0.0 {
            tally.shield_down.push(player_id);
        }
        self.hostiles.rigs.insert(player_id, combat);
        let rest = amount - soaked;
        if rest > 0.0 {
            *tally.rig_damage.entry(player_id).or_default() += rest;
        }
    }

    /// One step of every shot: missiles steer, everything flies, hits land on the drawn bodies
    /// (rig shots on ships, ships' shots on rigs: no friendly fire), the spent burn out.
    fn fly_shots(&mut self, at: &[(RigSnap, WorldPos)], step: u64, tally: &mut Tally) {
        let dt = COMBAT_STEP_SECONDS;
        let mut index = 0;
        while index < self.hostiles.shots.len() {
            let mut shot = self.hostiles.shots[index];
            if step >= shot.expires {
                self.hostiles.shots.remove(index);
                continue;
            }
            // A missile turns toward its lock.
            if shot.kind == ShotKind::Missile
                && let Some(lock) = shot.lock
                && let Some(target) = self.hostiles.ships.iter().find(|ship| ship.id == lock)
            {
                let (dx, dy) = shot.pos.delta(target.pos);
                let heading = bearing(shot.vx, shot.vy);
                let goal = bearing(dx, dy);
                let turned = turn_toward(heading, goal, MISSILE_TURN, dt);
                let speed = (length(shot.vx, shot.vy) + MISSILE_ACCEL * dt).min(MISSILE_TOP_SPEED);
                let (nx, ny) = nose(turned);
                shot.vx = nx * speed;
                shot.vy = ny * speed;
            }
            let from = shot.pos;
            let to = shot.pos.offset(shot.vx * dt, shot.vy * dt);
            shot.pos = to;
            let mut spent = false;
            match shot.from {
                Shooter::Rig(owner) => {
                    for ship in &mut self.hostiles.ships {
                        if ship.hull <= 0.0 {
                            continue;
                        }
                        let a = ship.pos.delta(from);
                        let b = ship.pos.delta(to);
                        if hits_ship(ship.class, ship.angle, a, b, shot.kind.radius()) {
                            let shielded = damage_ship(ship, shot.damage, step);
                            ship.hit_by = Some(owner);
                            let entry = tally.ship_hits.entry(owner).or_insert(shielded);
                            *entry &= shielded;
                            spent = true;
                            break;
                        }
                    }
                }
                Shooter::Ship(_) => {
                    for (rig, pos) in at {
                        if !rig.targetable {
                            continue;
                        }
                        let a = pos.delta(from);
                        let b = pos.delta(to);
                        if combat::segment_hits(
                            a,
                            b,
                            (0.0, 0.0),
                            RIG_HIT_RADIUS + shot.kind.radius(),
                        ) {
                            self.hurt_rig(rig.id, shot.damage, step, tally);
                            spent = true;
                            break;
                        }
                    }
                }
            }
            if spent {
                self.hostiles.shots.remove(index);
            } else {
                self.hostiles.shots[index] = shot;
                index += 1;
            }
        }
    }

    /// A destroyed ship's wreck: its credits and ore (a boss's tech too).
    fn wreck_of(&mut self, ship: &HostileShip, step: u64) {
        let stats = ship.class.stats();
        let ring = self
            .hostiles
            .encounter(ship.encounter)
            .map_or(1, |encounter| encounter.danger);
        let roll = hash64(self.world_seed(), 0x3EC4, i64::from(ship.id), step as i64);
        let mineral = crate::enemy::tier_mineral(4 + ring.min(5) + u8::from(ship.class.is_boss()));
        let mut cargo = Vec::new();
        let count = stats.cargo + u32::try_from(roll % 3).unwrap_or(0);
        if stats.cargo > 0 {
            cargo.push(ItemStack::new(
                crate::item::ItemKind::Mineral(mineral),
                count,
            ));
        }
        let id = self.hostiles.take_id();
        self.hostiles.wrecks.push(Wreck {
            id,
            class: ship.class,
            pos: ship.pos,
            vx: ship.vx * 0.3,
            vy: ship.vy * 0.3,
            credits: stats.credits + stats.credits * u32::from(ring.saturating_sub(1)) / 4,
            cargo,
            expires: step + WRECK_LIFE_STEPS,
            tech: ship.class.is_boss().then(|| ship.class.faction().tech()),
            owner: None,
        });
    }

    /// One step of wrecks drifting (slowing), breaking up, and being collected.
    fn drift_wrecks(&mut self, at: &[(RigSnap, WorldPos)], step: u64, output: &mut TickOutput) {
        let dt = COMBAT_STEP_SECONDS;
        let mut index = 0;
        while index < self.hostiles.wrecks.len() {
            let wreck = &mut self.hostiles.wrecks[index];
            if step >= wreck.expires {
                self.hostiles.wrecks.remove(index);
                continue;
            }
            wreck.pos = wreck.pos.offset(wreck.vx * dt, wreck.vy * dt);
            wreck.vx *= 1.0 - 0.6 * dt;
            wreck.vy *= 1.0 - 0.6 * dt;
            let reach = WRECK_PICKUP_RANGE
                + if wreck.class.is_boss() {
                    wreck.class.stats().size.0 * 0.4
                } else {
                    0.0
                };
            let collector = at
                .iter()
                .filter(|(rig, pos)| {
                    rig.targetable
                        && wreck.owner.is_none_or(|owner| owner == rig.id)
                        && pos.distance(wreck.pos) <= reach
                })
                .map(|(rig, _)| rig.id)
                .min();
            if let Some(player_id) = collector {
                let wreck = self.hostiles.wrecks.remove(index);
                self.collect_wreck(player_id, &wreck, output);
            } else {
                index += 1;
            }
        }
    }

    fn collect_wreck(&mut self, player_id: PlayerId, wreck: &Wreck, output: &mut TickOutput) {
        if let Some(tech) = wreck.tech {
            self.hostiles.tech.insert(tech);
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        player.credits = player.credits.saturating_add(wreck.credits);
        let capacity = player.cargo_capacity;
        for stack in &wreck.cargo {
            let fits = player.cargo_bay.accepts(stack.kind, stack.count, capacity);
            player.cargo_bay.add(stack.kind, fits);
        }
        output.push_event(hostile_event(
            HostileEvent::WreckLooted,
            Some(player_id),
            wreck.credits,
        ));
        output.push_event(GameplayEventKind::CargoChanged { player_id });
    }

    /// One step of every rig's deflector regrowing and its missile rack reloading.
    fn rig_upkeep(&mut self, rigs: &[RigSnap], step: u64) {
        let living = self.hostiles.has_tech(BossTech::LivingShield);
        let lance = self.hostiles.has_tech(BossTech::LanceMissiles);
        for rig in rigs {
            let Some(combat) = self.hostiles.rigs.get_mut(&rig.id) else {
                continue;
            };
            let capacity = shield_capacity(rig.gear);
            if step >= combat.hit_at + SHIELD_REGEN_DELAY_STEPS || combat.hit_at == 0 {
                let regen = if living {
                    SHIELD_REGEN * 2.0
                } else {
                    SHIELD_REGEN
                };
                combat.shield = (combat.shield + regen * COMBAT_STEP_SECONDS).min(capacity);
            }
            combat.shield = combat.shield.min(capacity);
            let rack = missile_capacity(rig.gear, lance);
            if combat.missiles < rack && step >= combat.reload_at {
                if combat.reload_at > 0 {
                    combat.missiles += 1;
                }
                combat.reload_at = step + MISSILE_RELOAD_STEPS;
            }
            combat.missiles = combat.missiles.min(rack);
        }
    }

    /// The tick's hits on rigs (hull damage through the usual mitigation) and its events; the
    /// destroyed ships' kills, bosses and bounties; a wrecked rig's cargo pod.
    fn settle_hits(
        &mut self,
        rigs: &[RigSnap],
        tally: &mut Tally,
        step: u64,
        output: &mut TickOutput,
    ) {
        for (player_id, kind) in std::mem::take(&mut tally.fired) {
            output.push_event(hostile_event(
                HostileEvent::Fired,
                Some(player_id),
                kind.code(),
            ));
        }
        for (player_id, shielded) in std::mem::take(&mut tally.ship_hits) {
            output.push_event(hostile_event(
                HostileEvent::ShipHit,
                Some(player_id),
                u32::from(shielded),
            ));
        }
        for (player_id, kind) in std::mem::take(&mut tally.hostile_fired).into_iter().take(4) {
            output.push_event(hostile_event(HostileEvent::Fired, player_id, kind.code()));
        }
        for (player_id, attack) in std::mem::take(&mut tally.telegraphs) {
            output.push_event(hostile_event(
                HostileEvent::BossTelegraph,
                player_id,
                attack.code(),
            ));
        }
        for player_id in std::mem::take(&mut tally.shield_down) {
            output.push_event(hostile_event(HostileEvent::ShieldDown, Some(player_id), 0));
        }
        for (player_id, damage) in std::mem::take(&mut tally.rig_damage) {
            let was_alive = self.players.get(&player_id).is_some_and(|p| p.hull > 0.0);
            if self.damage_player(player_id, damage) {
                output.push_event(GameplayEventKind::PlayerDamaged { player_id });
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "damage"
                )]
                output.push_event(hostile_event(
                    HostileEvent::RigHit,
                    Some(player_id),
                    damage.round() as u32,
                ));
            }
            let wrecked = self.players.get(&player_id).is_some_and(|p| p.hull <= 0.0);
            if was_alive && wrecked {
                self.drop_cargo_pod(player_id, step, output);
            }
        }
        for ship in std::mem::take(&mut tally.destroyed) {
            let faction = ship.class.faction();
            *self.hostiles.kills.entry(faction).or_default() += 1;
            output.push_event(hostile_event(
                HostileEvent::ShipDestroyed,
                ship.hit_by,
                ship.class.code(),
            ));
            if ship.class.is_boss() {
                self.hostiles.bosses_beaten.insert(faction);
                output.push_event(hostile_event(
                    HostileEvent::BossDefeated,
                    ship.hit_by,
                    faction.code(),
                ));
            }
            if let Some(bounty) = ship.bounty
                && let Some(index) = self
                    .hostiles
                    .bounties
                    .iter()
                    .position(|posted| posted.target.id == bounty)
            {
                let posted = self.hostiles.bounties.remove(index);
                self.hostiles.bounties_claimed.insert(bounty);
                if let Some(hunter) = self.players.get_mut(&posted.hunter) {
                    hunter.credits = hunter.credits.saturating_add(posted.target.reward);
                }
                output.push_event(hostile_event(
                    HostileEvent::BountyClaimed,
                    Some(posted.hunter),
                    posted.target.reward,
                ));
                output.push_event(GameplayEventKind::CargoChanged {
                    player_id: posted.hunter,
                });
            }
        }
        let _ = rigs;
    }

    /// A rig wrecked in a fight leaves half its cargo in a pod where it fell (only it can
    /// collect the pod); the beacon tows it home as anywhere in space.
    fn drop_cargo_pod(&mut self, player_id: PlayerId, step: u64, output: &mut TickOutput) {
        let Some(player) = self.players.get_mut(&player_id) else {
            return;
        };
        let pos = player.flight.pos;
        let stacks: Vec<ItemStack> = player.cargo_bay.stacks().collect();
        let mut pod = Vec::new();
        for stack in stacks {
            let half = stack.count / 2;
            if half == 0 {
                continue;
            }
            let taken = player.cargo_bay.remove(stack.kind, half);
            pod.push(ItemStack::new(stack.kind, taken));
        }
        if !pod.is_empty() {
            let id = self.hostiles.take_id();
            self.hostiles.wrecks.push(Wreck {
                id,
                class: ShipClass::Cutter,
                pos,
                vx: 0.0,
                vy: 0.0,
                credits: 0,
                cargo: pod,
                expires: step + WRECK_LIFE_STEPS * 4,
                tech: None,
                owner: Some(player_id),
            });
            output.push_event(GameplayEventKind::CargoChanged { player_id });
        }
        self.end_encounters_of(player_id, step, output);
    }

    /// Escape jumps whose spool is done: the rig is far away along its heading, its fights
    /// over. A jump that would end inside a body's reach turns aside (always somewhere).
    fn run_escapes(&mut self, rigs: &[RigSnap], step: u64, output: &mut TickOutput) {
        for rig in rigs {
            let Some(mut combat) = self.hostiles.rigs.get(&rig.id).copied() else {
                continue;
            };
            let Some(since) = combat.escape else {
                continue;
            };
            if !self.free_flying(rig.id) {
                combat.escape = None;
                self.hostiles.rigs.insert(rig.id, combat);
                continue;
            }
            if step < since + ESCAPE_SPOOL_STEPS {
                continue;
            }
            let length = f64::from(tiles(ESCAPE_JUMP_TILES));
            let mut destination = None;
            for turn in [
                0.0_f32,
                std::f32::consts::FRAC_PI_2,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
            ] {
                let (nx, ny) = nose(rig.angle + turn);
                let at = advance(rig.pos, f64::from(nx) * length, f64::from(ny) * length);
                let bodies = self.space_bodies_near(at);
                let (gap, _) = nearest_surface(&bodies, at);
                let clear = bodies
                    .iter()
                    .all(|body| at.distance(body.centre) > body.approach)
                    && gap > tiles(2_000.0);
                if clear {
                    destination = Some(at);
                    break;
                }
            }
            let destination = destination.unwrap_or_else(|| {
                let (nx, ny) = nose(rig.angle);
                advance(
                    rig.pos,
                    f64::from(nx) * length * 0.1,
                    f64::from(ny) * length * 0.1,
                )
            });
            combat.escape = None;
            combat.next_roll = step + REST_STEPS;
            self.hostiles.rigs.insert(rig.id, combat);
            let infinite = self.test_mode.is_some_and(|state| state.infinite_fuel);
            if let Some(player) = self.players.get_mut(&rig.id) {
                player.flight.pos = destination;
                player.flight.drive = StarDrive::Off;
                if !infinite {
                    player.fuel = (player.fuel - ESCAPE_FUEL).max(0.0);
                }
            }
            output.push_event(hostile_event(HostileEvent::Escaped, Some(rig.id), 0));
            self.end_encounters_of(rig.id, step, output);
        }
    }

    /// Ends every encounter after `player_id` (its ships vanish).
    fn end_encounters_of(&mut self, player_id: PlayerId, step: u64, output: &mut TickOutput) {
        let ended: Vec<u32> = self
            .hostiles
            .encounters
            .iter()
            .filter(|encounter| encounter.target == player_id)
            .map(|encounter| encounter.id)
            .collect();
        for id in ended {
            self.end_encounter(id, step, output);
        }
    }

    fn end_encounter(&mut self, id: u32, step: u64, output: &mut TickOutput) {
        let Some(index) = self.hostiles.encounters.iter().position(|e| e.id == id) else {
            return;
        };
        let encounter = self.hostiles.encounters.remove(index);
        // A boss that got away waits in its lair again; a bounty target too.
        if let EncounterKind::Bounty(bounty) = encounter.kind
            && let Some(posted) = self
                .hostiles
                .bounties
                .iter_mut()
                .find(|posted| posted.target.id == bounty)
        {
            posted.met = false;
        }
        self.hostiles.ships.retain(|ship| ship.encounter != id);
        let combat = self.hostiles.rigs.entry(encounter.target).or_default();
        combat.next_roll = combat.next_roll.max(step + REST_STEPS);
        output.push_event(hostile_event(
            HostileEvent::EncounterOver,
            Some(encounter.target),
            encounter.faction.code(),
        ));
    }

    /// Each encounter's course: closing, the pirates' demand and its answer, fighting, the
    /// paid and the wounded leaving, and the end (beaten, lost, or its rig gone).
    #[allow(clippy::too_many_lines, reason = "each state's answer in one place")]
    fn resolve_encounters(&mut self, rigs: &[RigSnap], step: u64, output: &mut TickOutput) {
        let encounters = self.hostiles.encounters.clone();
        for encounter in encounters {
            let rig = rigs
                .iter()
                .find(|rig| rig.id == encounter.target && rig.targetable);
            let ships: Vec<HostileShip> = self.hostiles.ships_of(encounter.id).copied().collect();
            let Some(rig) = rig else {
                // Its rig docked, landed, warped away or went home: the ships lose it.
                self.end_encounter(encounter.id, step, output);
                continue;
            };
            if ships.is_empty() {
                self.end_encounter(encounter.id, step, output);
                continue;
            }
            let nearest = ships
                .iter()
                .map(|ship| ship.pos.distance(rig.pos))
                .fold(f32::MAX, f32::min);
            if nearest > crate::hostiles::DISENGAGE_RANGE {
                self.end_encounter(encounter.id, step, output);
                continue;
            }
            let shot_at = ships.iter().any(|ship| ship.hit_by.is_some());
            let state = match encounter.state {
                EncounterState::Closing => {
                    if encounter.faction == Faction::Pirates
                        && !matches!(
                            encounter.kind,
                            EncounterKind::Boss | EncounterKind::Bounty(_)
                        )
                    {
                        if shot_at {
                            output.push_event(hostile_event(
                                HostileEvent::TurnedHostile,
                                Some(rig.id),
                                0,
                            ));
                            EncounterState::Fighting
                        } else if nearest < HAIL_START {
                            let credits = self.demand_of(rig.id);
                            output.push_event(hostile_event(
                                HostileEvent::Hailed,
                                Some(rig.id),
                                credits,
                            ));
                            EncounterState::Demand {
                                credits,
                                until: step + DEMAND_STEPS,
                            }
                        } else {
                            EncounterState::Closing
                        }
                    } else if encounter.kind == EncounterKind::Patrol
                        && ships.iter().all(|ship| ship.mode == ShipMode::Patrolling)
                        && !shot_at
                    {
                        EncounterState::Closing
                    } else if nearest < ENGAGE_RANGE || shot_at {
                        EncounterState::Fighting
                    } else {
                        EncounterState::Closing
                    }
                }
                EncounterState::Demand { until, .. } => {
                    if shot_at || step >= until || nearest > HAIL_BREAK_RANGE {
                        output.push_event(hostile_event(
                            HostileEvent::TurnedHostile,
                            Some(rig.id),
                            0,
                        ));
                        EncounterState::Fighting
                    } else {
                        encounter.state
                    }
                }
                EncounterState::Fighting => EncounterState::Fighting,
                EncounterState::Paid => {
                    if nearest > LEAVE_RANGE
                        || step >= encounter.started + crate::hostiles::steps(60.0)
                    {
                        self.end_encounter(encounter.id, step, output);
                        continue;
                    }
                    EncounterState::Paid
                }
            };
            if let Some(found) = self
                .hostiles
                .encounters
                .iter_mut()
                .find(|e| e.id == encounter.id)
            {
                found.state = state;
            }
            // Ships follow the encounter's course: hailers hold, fighters fight, the badly
            // hurt cutters run, and those that ran far enough vanish.
            let mut gone = Vec::new();
            for ship in self
                .hostiles
                .ships
                .iter_mut()
                .filter(|ship| ship.encounter == encounter.id)
            {
                match state {
                    EncounterState::Demand { .. } if ship.mode == ShipMode::Closing => {
                        ship.mode = ShipMode::Hailing;
                    }
                    EncounterState::Fighting
                        if matches!(
                            ship.mode,
                            ShipMode::Closing | ShipMode::Hailing | ShipMode::Patrolling
                        ) =>
                    {
                        ship.mode = ShipMode::Attacking;
                    }
                    _ => {}
                }
                if ship.class == ShipClass::Cutter
                    && ship.mode == ShipMode::Attacking
                    && ship.hull_share() < ai::CUTTER_FLEE_SHARE
                {
                    ship.mode = ShipMode::Leaving;
                }
                if ship.mode == ShipMode::Leaving && ship.pos.distance(rig.pos) > LEAVE_RANGE {
                    gone.push(ship.id);
                }
            }
            self.hostiles.ships.retain(|ship| !gone.contains(&ship.id));
        }
    }

    // --- Ground raids -------------------------------------------------------------------------

    /// Ground raids in a body's ground pass: rolls on an occupied body, troopers landing near a
    /// rig (TODO-SEAM w12-aliens: alien towns; w12-bodylife: bases), and raids ending when
    /// their troopers are gone.
    pub(super) fn advance_raids(&mut self, next_tick: SimulationTick, output: &mut TickOutput) {
        let store = self.active_store();
        let crate::place::StoreKey::Body(key) = store else {
            return;
        };
        let step = step_at_tick(next_tick.get(), self.tick_rate_hz());
        // Raids whose troopers are all gone are over.
        let entities = &self.entities;
        self.hostiles.raids.retain(|raid| {
            raid.store != store
                || raid
                    .troopers
                    .iter()
                    .any(|id| entities.get(crate::entity::EntityId(*id)).is_some())
        });
        if self.hostiles.raids.iter().any(|raid| raid.store == store) {
            return;
        }
        let code = store.code();
        let ready = self.hostiles.raid_ready.get(&code).copied().unwrap_or(0);
        if step < ready {
            return;
        }
        self.hostiles
            .raid_ready
            .insert(code, step + raid::RAID_ROLL_STEPS);
        let space = self.space();
        let Some(body) = space.space().body(key) else {
            return;
        };
        let ring = danger_ring(body.centre.pos());
        let roll = hash64(
            self.world_seed(),
            0x4A1D,
            step as i64,
            i64::try_from(code >> 2).unwrap_or(0),
        );
        if roll % 1_000 >= u64::from(raid::raid_chance_permille(ring)) {
            return;
        }
        let faction = raid::raiders_of(body.theme, roll >> 16);
        self.land_raid(faction, ring, roll, step, output);
    }

    /// Test mode: land `faction`'s raid by `player_id`'s rig (on a body's ground only).
    pub(super) fn test_spawn_raid(
        &mut self,
        player_id: PlayerId,
        faction: Faction,
    ) -> PlayerScopedCommandOutcome {
        if !matches!(self.active_store(), crate::place::StoreKey::Body(_))
            || !self.players.contains_key(&player_id)
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let step = self.combat_step_now();
        let mut output = TickOutput::default();
        if self.land_raid(
            faction,
            4,
            hash64(self.world_seed(), 0x7A1D, step as i64, 0),
            step,
            &mut output,
        ) {
            PlayerScopedCommandOutcome::Applied
        } else {
            PlayerScopedCommandOutcome::IgnoredUnavailable
        }
    }

    /// Lands a raid of `faction` beside the first rig on this ground. Returns whether it
    /// landed.
    fn land_raid(
        &mut self,
        faction: Faction,
        ring: u8,
        roll: u64,
        step: u64,
        output: &mut TickOutput,
    ) -> bool {
        let store = self.active_store();
        let frame = self.frame();
        let Some(site) = self.raid_sites().into_iter().next() else {
            return false;
        };
        let way = if roll & 1 == 0 { 1 } else { -1 };
        let column = site.column + way * raid::LANDING_OFFSET;
        // The ground under the landing column: the first solid tile below the site's row.
        let mut ground = site.row;
        for row in site.row - 30..site.row + 60 {
            if self
                .terrain
                .is_solid_at(crate::terrain::TilePosition { x: column, y: row })
            {
                ground = row;
                break;
            }
        }
        let kind = raid::trooper(faction);
        let count = raid::trooper_count(ring);
        let mut troopers = Vec::new();
        for index in 0..count {
            let offset = i32::try_from(index).unwrap_or(0) * 2 - i32::try_from(count).unwrap_or(0);
            let (x, y) = frame.centre_of(column + offset, ground - raid::DROP_ROWS);
            let id = self.spawn_enemy(kind, x, y);
            troopers.push(id.get());
        }
        self.hostiles.raids.push(Raid {
            store,
            faction,
            troopers,
            column,
            row: ground,
            landed: step,
        });
        self.hostiles
            .raid_ready
            .insert(store.code(), step + raid::RAID_REST_STEPS);
        output.push_event(hostile_event(
            HostileEvent::RaidLanded,
            None,
            faction.code(),
        ));
        true
    }

    /// Where raiders come for on this ground: every rig on it (TODO-SEAM w12-aliens: alien
    /// towns; w12-bodylife / bases: player bases on bodies).
    fn raid_sites(&self) -> Vec<raid::RaidSite> {
        let frame = self.frame();
        self.players
            .values()
            .filter(|player| player.hull > 0.0)
            .map(|player| {
                let tile = frame.tile_at(player.x, player.y);
                raid::RaidSite {
                    column: tile.x,
                    row: tile.y,
                }
            })
            .collect()
    }
}

/// A patrol leader's order: along its route (a circle round the encounter's start, the way
/// round it faces).
fn patrol_order(ship: &HostileShip, encounter: &Encounter) -> Order {
    let stats = ship.class.stats();
    // Fly a wide circle round where the patrol was met: tangent plus a pull back to the ring.
    let (dx, dy) = ship.pos.delta(encounter.origin);
    let distance = length(dx, dy).max(1.0);
    let radius = 900.0;
    let (tx, ty) = (dy / distance, -dx / distance);
    let radial = ((distance - radius) * 1.5).clamp(-stats.top_speed, stats.top_speed);
    let speed = stats.top_speed * 0.5;
    let wx = tx * speed + dx / distance * radial;
    let wy = ty * speed + dy / distance * radial;
    let (ax, ay) = combat::match_velocity(ship.vx, ship.vy, wx, wy);
    Order {
        ax,
        ay,
        face: bearing(wx, wy),
        fire: false,
    }
}
