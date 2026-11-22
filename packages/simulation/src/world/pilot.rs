//! The pilot on foot, world side (Part II F5; the body and data live in [`crate::pilot`]).
//!
//! * **Exit and board** ([`crate::PlayerCommand::TogglePilot`]): a slow rig lets its pilot out
//!   beside it as a [`EntityKind::Pilot`] entity; standing within [`BOARD_REACH`] of the rig,
//!   the same button boards it (the personal pack empties into the rig, what fits). The parked
//!   rig keeps its cargo, stays solid to its pilot (who can stand on its roof) and can still be
//!   hurt by enemies.
//! * **Controls**: while a player is on foot their held controls drive the pilot (the rig gets
//!   neutral controls): move, up / jump, down, the jump button, and the trigger swings the hand
//!   pick toward the aim. Bomb buttons throw from the pilot's hands.
//! * **Survival**: health (suit tiers), oxygen in water, gas and thin air
//!   ([`WorldState::pilot_air_at`] is the seam for altitude physics and the Moon), lava, fall
//!   damage, enemy strikes ([`WorldState::strike_pilot_as`]) and blasts. At zero health the pilot
//!   drops part of its pack and respawns in the rig (at the home point if the rig is wrecked).
//! * **Persistence**: parking a player parks the pilot too ([`ParkedPilot`]).

use crate::{
    entity::{Body, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    explosive::{Deployment, ExplosiveKind, ThrowAim},
    ids::{PlayerId, SimulationTick},
    input::{ControlState, TickInputs},
    item::{ItemKind, ItemStack, Stowage},
    pilot::{
        BOARD_REACH, EXIT_MAX_SPEED, LAVA_DAMAGE_PER_SECOND, OXYGEN_REFILL_RATE,
        PICK_CLINK_SECONDS, PILOT_CROUCH_HEIGHT, PILOT_HALF_WIDTH, PILOT_PICKUP_RADIUS,
        PILOT_RECALL_FEE, PILOT_STAND_HEIGHT, PickProgress, PilotControls, PilotGear,
        PilotGearKind, PilotMode, PilotState, PilotTerrain, REGEN_AFTER_SECONDS, REGEN_PER_SECOND,
        STRIKE_GUARD_SECONDS, SUFFOCATION_DAMAGE_PER_SECOND, THIN_AIR_ROW, THROW_COOLDOWN_SECONDS,
        THROW_SPEED, box_blocked, fall_damage_in, pick_seconds, pilot_passable, step_pilot,
    },
    quantize::quantize_simulation_scalar as quantize,
    summary::PlayerScopedCommandOutcome,
    terrain::{TILE_SIZE, Terrain, TileKind, TilePosition},
};

use super::{WorldState, bombs::Blast};

/// Half-width of a parked rig's roof, the one-way platform pilots stand on (px).
const RIG_ROOF_HALF_WIDTH: f32 = 14.0;
/// Height of a rig's roof above its centre (px).
const RIG_ROOF_ABOVE_CENTRE: f32 = 13.0;
/// Reach samples of the hand pick (4 px apart, see [`PICK_REACH`]).
const PICK_SAMPLES: u8 = 10;
/// Seconds between pick swing sounds.
const PICK_SWING_SECONDS: f32 = 0.3;
/// Share of each carried stack (at most three stacks) a dying pilot drops.
const DEATH_DROP_STACKS: usize = 3;
/// A blast hurts a pilot this share of what it does to a practice target.
const PILOT_BLAST_SHARE: f32 = 0.6;
/// The oxygen warning fires once the air falls under this share.
const OXYGEN_WARNING_SHARE: f32 = 0.25;

/// What a pilot breathes at a spot ([`WorldState::pilot_air_at`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PilotAir {
    /// Normal air: oxygen refills.
    Breathable,
    /// Head under water.
    Water,
    /// Inside a gas cloud or beside a gas pocket.
    Gas,
    /// Too high up: the air is too thin ([`THIN_AIR_ROW`]; the Moon has none at all).
    Thin,
}

impl PilotAir {
    /// Whether the pilot uses up its own air here.
    #[must_use]
    pub const fn drains(self) -> bool {
        !matches!(self, Self::Breathable)
    }
}

/// A parked player's pilot (saved in [`crate::ParkedPlayer::pilot`] while they are away).
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct ParkedPilot {
    pub body: Body,
    pub state: PilotState,
}

/// The world as a pilot's body sees it: terrain (vent grates open), ladder and rope fixtures,
/// cave water and parked rigs' roofs.
struct PilotView<'a> {
    world: &'a WorldState,
    /// Rig roofs: `(centre x, roof y)`.
    roofs: Vec<(f32, f32)>,
}

impl PilotTerrain for PilotView<'_> {
    fn blocks(&self, tile: TilePosition) -> bool {
        pilot_blocks(&self.world.terrain, tile)
    }

    fn climbable(&self, tile: TilePosition) -> bool {
        matches!(
            self.world.bases.fixtures_at(tile).front,
            Some((crate::base::Fixture::Ladder | crate::base::Fixture::Rope, _))
        )
    }

    fn water(&self, tile: TilePosition) -> bool {
        self.world.terrain.water_at(tile)
    }

    fn platform_top(&self, left: f32, right: f32, from_y: f32, to_y: f32) -> Option<f32> {
        self.roofs
            .iter()
            .filter(|(x, top)| {
                right > x - RIG_ROOF_HALF_WIDTH
                    && left < x + RIG_ROOF_HALF_WIDTH
                    && from_y <= *top + 0.5
                    && to_y >= *top
            })
            .map(|(_, top)| *top)
            .min_by(f32::total_cmp)
    }
    fn gravity_at(&self, x: f32, y: f32) -> f32 {
        // Read at the feet: the fields are smooth over a pilot's height. A field (F9) turns
        // and scales gravity; the pilot stays upright and feels its downward share here (the
        // sideways share pushes it in `advance_pilots`).
        let pull = crate::environment::environment_in(
            self.world.terrain.settings(),
            self.world.frame(),
            x,
            y,
        )
        .body_gravity(crate::pilot::GRAVITY);
        let field = self.world.fields_at(x, y);
        if field.is_neutral_physics() {
            pull
        } else {
            pull * field.gravity_y
        }
    }

    fn frame(&self) -> crate::position::Frame {
        self.world.frame()
    }
}

/// Whether `tile` stops a pilot: solid terrain except vent grates; outside the world's
/// columns and below its floor is solid, the sky above is open.
#[must_use]
pub fn pilot_blocks(terrain: &Terrain, tile: TilePosition) -> bool {
    if !terrain.contains_column(tile.x) || tile.y >= terrain.height() {
        return true;
    }
    terrain
        .tile(tile)
        .is_some_and(|found| found.kind != TileKind::Air && !pilot_passable(found.kind))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "event positions are whole world pixels"
)]
const fn px(value: f32) -> i32 {
    value as i32
}

impl WorldState {
    // --- Queries ----------------------------------------------------------------------------

    /// Every pilot on foot, in entity id order: `(entity, body, state)`.
    pub fn pilots(&self) -> impl Iterator<Item = (EntityId, &Body, &PilotState)> + '_ {
        self.entities
            .of_kind(EntityKind::Pilot)
            .filter_map(|entity| match &entity.state {
                EntityState::Pilot(state) => Some((entity.id, &entity.body, state)),
                _ => None,
            })
    }

    /// `player_id`'s pilot, if it is out of its rig.
    #[must_use]
    pub fn pilot_of(&self, player_id: PlayerId) -> Option<(EntityId, &Body, &PilotState)> {
        self.pilots().find(|(_, _, state)| state.owner == player_id)
    }

    /// Whether `player_id` is on foot (its pilot is out of the rig).
    #[must_use]
    pub fn is_on_foot(&self, player_id: PlayerId) -> bool {
        self.pilot_of(player_id).is_some()
    }

    /// Where `player_id` is acting from: the pilot's middle while on foot, else the rig's
    /// centre. Building counters, village doors and the camera use it.
    #[must_use]
    pub fn actor_position(&self, player_id: PlayerId) -> Option<(f32, f32)> {
        if let Some((_, body, state)) = self.pilot_of(player_id) {
            return Some(state.middle(body));
        }
        self.players
            .get(&player_id)
            .map(|player| (player.x, player.y))
    }

    /// The tile [`Self::actor_position`] is in: where build mode, module menus and counters
    /// reach from.
    #[must_use]
    pub fn actor_tile(&self, player_id: PlayerId) -> Option<TilePosition> {
        self.actor_position(player_id)
            .map(|(x, y)| self.frame().tile_at(x, y))
    }

    /// Whether `player_id`'s rig would let its pilot out now (slow enough, intact).
    #[must_use]
    pub fn can_exit_rig(&self, player_id: PlayerId) -> bool {
        !self.is_on_foot(player_id) && self.exit_spot(player_id).is_some()
    }

    /// Whether `player_id`'s pilot stands close enough to board its rig.
    #[must_use]
    pub fn can_board_rig(&self, player_id: PlayerId) -> bool {
        let (Some((_, body, state)), Some(player)) =
            (self.pilot_of(player_id), self.players.get(&player_id))
        else {
            return false;
        };
        let (mx, my) = state.middle(body);
        player.hull > 0.0 && (player.x - mx).hypot(player.y - my) <= BOARD_REACH
    }

    /// What a pilot breathes at world pixel `(x, y)` (see [`PilotAir`]).
    ///
    /// Altitude physics (F6) and the Moon (WP-MOON): on a V8 sky the air is thin where its
    /// density falls under [`crate::pilot::PILOT_THIN_AIR_DENSITY`] (the upper archipelago and
    /// above, and the vacuum of orbit and the Moon); older worlds use the fixed
    /// [`THIN_AIR_ROW`].
    #[must_use]
    pub fn pilot_air_at(&self, x: f32, y: f32) -> PilotAir {
        let tile = self.frame().tile_at(x, y);
        if self.terrain.water_at(tile) {
            return PilotAir::Water;
        }
        let in_cloud = self.entities.of_kind(EntityKind::GasCloud).any(|entity| {
            matches!(entity.state, EntityState::GasCloud(cloud)
                if cloud.life < 6.0
                    && (entity.body.x - x).hypot(entity.body.y - y) <= cloud.radius)
        });
        let beside_pocket = (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                self.terrain
                    .tile(TilePosition {
                        x: tile.x + dx,
                        y: tile.y + dy,
                    })
                    .is_some_and(|found| found.kind == TileKind::Gas)
            })
        });
        if in_cloud || beside_pocket {
            return PilotAir::Gas;
        }
        // A sky decides by its breathable air (band-relative: thin from the lower archipelago,
        // vacuum in orbit and on the Moon); older worlds keep the fixed thin-air row.
        let settings = self.terrain.settings();
        let thin = if settings.generator.has_sky() {
            crate::environment::environment_in(settings, self.frame(), x, y).breathable
                < crate::pilot::PILOT_THIN_AIR_DENSITY
        } else {
            tile.y < THIN_AIR_ROW
        };
        if thin {
            return PilotAir::Thin;
        }
        PilotAir::Breathable
    }

    // --- Exit, board, pack ------------------------------------------------------------------

    /// Where a pilot leaving `player_id`'s rig would stand: `(x, feet y, crouched)`.
    fn exit_spot(&self, player_id: PlayerId) -> Option<(f32, f32, bool)> {
        let player = self.players.get(&player_id)?;
        if player.hull <= 0.0 || player.velocity_x.hypot(player.velocity_y) > EXIT_MAX_SPEED {
            return None;
        }
        let side = if player.velocity_x < 0.0 { -1.0 } else { 1.0 };
        let feet = player.y + super::movement::AUTHORITATIVE_PLAYER_RADIUS - 0.5;
        let view = self.pilot_view(None);
        // Clear of the rig's roof (half-width 14) on either side, else on top of it.
        for dx in [side * 30.0, -side * 30.0, 0.0] {
            let x = player.x + dx;
            for (height, crouched) in [(PILOT_STAND_HEIGHT, false), (PILOT_CROUCH_HEIGHT, true)] {
                if box_blocked(&view, x, feet, height) {
                    continue;
                }
                // Settle onto the floor just below (the rig hovers a hair above it).
                let floor = ((feet + 4.0) / TILE_SIZE).floor().mul_add(TILE_SIZE, -0.25);
                let feet = if floor > feet
                    && box_blocked(&view, x, feet + 4.0, height)
                    && !box_blocked(&view, x, floor, height)
                {
                    floor
                } else {
                    feet
                };
                return Some((quantize(x), quantize(feet), crouched));
            }
        }
        None
    }

    /// The world as `owner`'s pilot sees it. Only its own rig is solid to it (a roof to stand
    /// on): other players' rigs move under their own (unpredicted) controls, so treating them
    /// as solid would make every client mispredict its pilot near a crewmate's rig.
    fn pilot_view(&self, owner: Option<PlayerId>) -> PilotView<'_> {
        let roofs = owner
            .and_then(|owner| self.players.get(&owner))
            .filter(|player| player.hull > 0.0)
            .map(|player| vec![(player.x, player.y - RIG_ROOF_ABOVE_CENTRE)])
            .unwrap_or_default();
        PilotView { world: self, roofs }
    }

    /// [`crate::PlayerCommand::TogglePilot`]: exit the rig, or board it.
    pub(super) fn toggle_pilot(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        if self.is_on_foot(player_id) {
            if !self.can_board_rig(player_id) {
                return PlayerScopedCommandOutcome::IgnoredUnavailable;
            }
            self.board_rig(player_id, output);
            return PlayerScopedCommandOutcome::Applied;
        }
        let Some((x, feet, crouched)) = self.exit_spot(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let facing = if x < player.x { -1 } else { 1 };
        let mut state = PilotState::new(player_id, player.pilot_gear, facing);
        state.crouching = crouched;
        state.weapon = player.pilot_arsenal.equipped;
        let body = Body::at(x, feet).moving(
            quantize(player.velocity_x * 0.5),
            quantize(player.velocity_y.min(0.0)),
        );
        self.set_active_drill(player_id, None);
        let _ = self.spawn_entity(body, EntityState::Pilot(state), None);
        output.push_event(GameplayEventKind::PilotExited {
            player_id,
            x: px(x),
            y: px(feet),
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// The pilot climbs back in: the entity goes and the personal pack empties into the rig.
    fn board_rig(&mut self, player_id: PlayerId, output: &mut TickOutput) {
        let Some((id, _, _)) = self.pilot_of(player_id) else {
            return;
        };
        let _ = self.entities.despawn(id);
        let transferred = self.empty_pack_into_rig(player_id);
        output.push_event(GameplayEventKind::PilotBoarded {
            player_id,
            transferred,
        });
    }

    /// [`crate::PlayerCommand::Rescue`] on foot: the pilot radios for a pickup and is hauled
    /// back aboard its rig for [`PILOT_RECALL_FEE`] credits (what they have, if less). The way
    /// out of a pit the hand pick cannot dig, and back to a rig wrecked or stranded out of
    /// reach (whose own rescue then follows from the rig as usual).
    fn recall_pilot(
        &mut self,
        player_id: PlayerId,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        let Some((id, _, _)) = self.pilot_of(player_id) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits -= PILOT_RECALL_FEE.min(player.credits);
        }
        let _ = self.entities.despawn(id);
        let transferred = self.empty_pack_into_rig(player_id);
        output.push_event(GameplayEventKind::PilotBoarded {
            player_id,
            transferred,
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// Moves everything from the personal pack that fits into the rig; returns how many items.
    fn empty_pack_into_rig(&mut self, player_id: PlayerId) -> u32 {
        let Some(player) = self.players.get_mut(&player_id) else {
            return 0;
        };
        let stacks: Vec<(ItemKind, u32)> = player.pilot_pack.iter().collect();
        let mut moved = 0;
        for (kind, count) in stacks {
            let taken = player.stow(kind, count);
            player.pilot_pack.remove(kind, taken);
            moved += taken;
        }
        moved
    }

    /// [`crate::PlayerCommand::PilotPack`]: a manual swap between pack and rig (in the rig, or
    /// on foot within boarding reach of it).
    pub(super) fn transfer_pilot_pack(
        &mut self,
        player_id: PlayerId,
        item: ItemKind,
        count: u32,
        to_rig: bool,
        output: &mut TickOutput,
    ) -> PlayerScopedCommandOutcome {
        if !self.players.contains_key(&player_id) {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        }
        if self.is_on_foot(player_id) && !self.can_board_rig(player_id) {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let moved = if to_rig {
            let wanted = count.min(player.pilot_pack.count(item));
            let taken = player.stow(item, wanted);
            player.pilot_pack.remove(item, taken);
            taken
        } else {
            let room = player
                .pilot_gear
                .pack_capacity()
                .saturating_sub(player.pilot_pack.total());
            let wanted = count.min(player.carried(item)).min(room);
            let taken = match item.stowage() {
                Stowage::CargoBay => player.cargo_bay.remove(item, wanted),
                Stowage::BlockHopper => player.block_hopper.remove(item, wanted),
            };
            player.pilot_pack.add(item, taken);
            taken
        };
        if moved == 0 {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        output.push_event(GameplayEventKind::PackTransferred {
            player_id,
            count: moved,
            to_rig,
        });
        PlayerScopedCommandOutcome::Applied
    }

    /// [`crate::SessionServiceRequest::BuyPilotGear`] at the Explosives Shack.
    pub(super) fn buy_pilot_gear(
        &mut self,
        player_id: PlayerId,
        gear: PilotGearKind,
    ) -> PlayerScopedCommandOutcome {
        let skyless = !self.terrain.settings().generator.has_sky();
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if gear.needs_sky() && skyless {
            // No high cold on this world: the lining would be money thrown away.
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let owned = gear.tier(player.pilot_gear);
        let Some(price) = gear.price(owned) else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        if player.credits < price {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        player.credits -= price;
        player.pilot_gear = gear.upgraded(player.pilot_gear);
        let new_gear = player.pilot_gear;
        // A pilot already out (buying on foot at the counter) gets the new gear at once.
        if let Some((id, _, _)) = self.pilot_of(player_id) {
            self.entities.update(id, |entity| {
                if let EntityState::Pilot(state) = &mut entity.state {
                    refit(state, new_gear);
                }
            });
        }
        PlayerScopedCommandOutcome::Applied
    }

    /// The pilot's commands, and what the rig's commands do while its player is on foot:
    /// exit / board, pack swaps, throws from the pilot's hands; the rig-only scanner and
    /// infrastructure kits are refused on foot. `None`: not a pilot matter.
    pub(super) fn pilot_command(
        &mut self,
        player_id: PlayerId,
        command: &crate::PlayerCommand,
        output: &mut TickOutput,
    ) -> Option<PlayerScopedCommandOutcome> {
        use crate::PlayerCommand;
        if self.tomb_blocks(player_id, command) {
            // In a tomb run (WP-TOMBS): no radio pickup, rig recall, rope or boarding, and
            // the downed do nothing but wait.
            return Some(PlayerScopedCommandOutcome::IgnoredUnavailable);
        }
        match *command {
            PlayerCommand::TogglePilot => Some(self.toggle_pilot(player_id, output)),
            // Swapping the weapon in hand also works from the rig (for the next climb out).
            PlayerCommand::Pilot(
                action @ (crate::pilot::PilotAction::Equip(_)
                | crate::pilot::PilotAction::CycleWeapon),
            ) => Some(self.equip_pilot_weapon(player_id, action, output)),
            PlayerCommand::Pilot(_) if !self.is_on_foot(player_id) => {
                Some(PlayerScopedCommandOutcome::IgnoredUnavailable)
            }
            PlayerCommand::Pilot(crate::pilot::PilotAction::Use) => {
                Some(self.pilot_use(player_id, output))
            }
            PlayerCommand::Pilot(action) => self.pilot_gear_action(player_id, action, output),
            PlayerCommand::PilotPack {
                item,
                count,
                to_rig,
            } => Some(self.transfer_pilot_pack(player_id, item, count, to_rig, output)),
            _ if !self.is_on_foot(player_id) => None,
            PlayerCommand::PlaceBomb => {
                Some(self.pilot_throw(player_id, ExplosiveKind::Bomb, None))
            }
            PlayerCommand::DeployExplosive { kind, aim } => {
                Some(self.pilot_throw(player_id, kind, Some(aim)))
            }
            PlayerCommand::Rescue => Some(self.recall_pilot(player_id, output)),
            PlayerCommand::UseScanner | PlayerCommand::PlaceInfrastructure { .. } => {
                Some(PlayerScopedCommandOutcome::IgnoredUnavailable)
            }
            _ => None,
        }
    }

    // --- Throws -----------------------------------------------------------------------------

    /// A bomb button on foot: the pilot throws (or places) one carried explosive of `kind`
    /// from its hands along `aim` (plain bombs lob forward and up).
    pub(super) fn pilot_throw(
        &mut self,
        player_id: PlayerId,
        kind: ExplosiveKind,
        aim: Option<ThrowAim>,
    ) -> PlayerScopedCommandOutcome {
        let Some((id, body, state)) = self.pilot_of(player_id).map(|(id, b, s)| (id, *b, *s))
        else {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        };
        let Some(player) = self.players.get(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        if kind == ExplosiveKind::Bomblet
            || state.throw_cooldown > 0.0
            || player.explosive_count(kind) == 0
            || self.bomb_count() >= super::bombs::MAX_ACTIVE_BOMBS
        {
            return PlayerScopedCommandOutcome::IgnoredUnavailable;
        }
        let mut aim = aim.unwrap_or_else(|| ThrowAim::from_digital(state.facing, -1, state.facing));
        if kind.deployment() != Deployment::Place {
            // A level throw by hand gets a little loft so it carries.
            let (x, y) = aim.vector();
            if y.abs() < 0.1 {
                aim = ThrowAim::from_vector(x, -0.45).unwrap_or(aim);
            }
        }
        let (aim_x, aim_y) = aim.vector();
        let (hand_x, hand_y) = (
            f32::from(state.facing).mul_add(8.0, body.x),
            state.height().mul_add(-0.7, body.y),
        );
        let speed = match kind.deployment() {
            Deployment::Place => kind.launch_speed(),
            Deployment::Drop | Deployment::Throw => THROW_SPEED,
        };
        let pose = (
            hand_x,
            hand_y,
            aim_x.mul_add(speed, body.velocity_x * 0.5),
            aim_y.mul_add(speed, body.velocity_y * 0.5),
        );
        let bomb = self.launch_from(player_id, kind, aim, pose);
        if let Some(player) = self.players.get_mut(&player_id) {
            let _taken = player.take_explosive(kind);
        }
        let _ = self.spawn_charge(&bomb);
        self.next_bomb_id = self.next_bomb_id.saturating_add(1);
        self.entities.update(id, |entity| {
            if let EntityState::Pilot(state) = &mut entity.state {
                state.throw_cooldown = THROW_COOLDOWN_SECONDS;
            }
        });
        PlayerScopedCommandOutcome::Applied
    }

    // --- The pilot pass ---------------------------------------------------------------------

    /// Moves every pilot one tick with its owner's held controls, then applies breathing,
    /// hazards, the hand pick and deaths. Runs right after the tick's inputs.
    #[allow(clippy::too_many_lines, reason = "one pilot's tick in order")]
    pub(super) fn advance_pilots(
        &mut self,
        inputs: &TickInputs,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let ids: Vec<EntityId> = self.entities.ids_of(EntityKind::Pilot).collect();
        if ids.is_empty() {
            return;
        }
        let dt = self.timing.delta_seconds;
        for id in ids {
            let Some((mut body, mut state)) =
                self.entities.get(id).and_then(|entity| match entity.state {
                    EntityState::Pilot(state) => Some((entity.body, state)),
                    _ => None,
                })
            else {
                continue;
            };
            let owner = state.owner;
            if !self.players.contains_key(&owner) {
                // An orphaned pilot (its player left): nothing drives it.
                let _ = self.entities.despawn(id);
                continue;
            }
            // A downed tomb member (WP-TOMBS) lies still.
            let downed = self.tomb_downed(owner);
            let controls = if downed {
                ControlState::default()
            } else {
                inputs
                    .players
                    .get(&owner)
                    .and_then(|input| input.controls)
                    .unwrap_or_default()
            };
            let anchor = self.refresh_rope(&mut state, owner, output);
            // Field effects (F9): mirrored steering, the time scale and sideways gravity.
            let field = self.fields_at(body.x, body.y);
            let controls = if field.mirrored() {
                controls.mirrored()
            } else {
                controls
            };
            let foot = PilotControls {
                horizontal: controls.horizontal(),
                up: controls.thrust,
                down: controls.drill_down,
                jump: controls.jump,
                anchor,
            };
            let report = {
                let view = self.pilot_view(Some(owner));
                step_pilot(&mut body, &mut state, &view, foot, dt * field.time_scale)
            };
            if field.gravity_x != 0.0 && !state.grounded {
                body.velocity_x = crate::quantize::quantize_simulation_scalar(
                    (field.gravity_x * crate::pilot::GRAVITY)
                        .mul_add(dt * field.time_scale, body.velocity_x),
                );
            }
            state.pick_cooldown = (state.pick_cooldown - dt).max(0.0);
            state.throw_cooldown = (state.throw_cooldown - dt).max(0.0);
            state.calm += dt;
            if report.jumped {
                output.push_event(GameplayEventKind::PilotJumped { player_id: owner });
            }
            if report.jet_started {
                output.push_event(GameplayEventKind::JetpackIgnited { player_id: owner });
            }
            if report.jet_empty {
                output.push_event(GameplayEventKind::JetpackEmpty { player_id: owner });
            }
            if report.grabbed_ledge {
                output.push_event(GameplayEventKind::PilotLedgeGrab { player_id: owner });
            }
            if report.splashed {
                output.push_event(GameplayEventKind::PilotSplashed { player_id: owner });
            }
            if report.hover_started || report.glide_started {
                output.push_event(GameplayEventKind::PilotGlide {
                    player_id: owner,
                    hover: report.hover_started,
                });
            }
            if report.wall_jumped {
                output.push_event(GameplayEventKind::PilotWallJumped { player_id: owner });
            }
            if report.rolled {
                output.push_event(GameplayEventKind::CryptRoll {
                    player_id: owner,
                    x: px(body.x),
                    y: px(body.y),
                });
            }
            if report.rope_released {
                output.push_event(GameplayEventKind::PilotRope {
                    player_id: owner,
                    action: crate::pilot::RopeEvent::Released,
                    x: px(body.x),
                    y: px(body.y),
                });
            }
            let mut damage = 0.0;
            if report.landed && report.landing_speed > 300.0 {
                // Featherfall (WP-TOMBS relics): no fall hurts.
                let hurt = if state.boons.featherfall {
                    0.0
                } else {
                    fall_damage_in(report.landing_speed, state.gear)
                };
                output.push_event(GameplayEventKind::PilotLanded {
                    player_id: owner,
                    hard: hurt > 0.0,
                });
                damage += hurt;
            }
            damage += self.breathe(&body, &mut state, dt, owner, output);
            damage += self.pilot_exposure(&body, &mut state, dt, owner, output);
            if self.touches_lava(&body, &state) {
                damage = LAVA_DAMAGE_PER_SECOND.mul_add(dt, damage);
            }
            if state.calm >= REGEN_AFTER_SECONDS && damage <= 0.0 && !downed {
                // Second Wind and Restless Soul (WP-TOMBS relics) change the pace.
                let regen =
                    crate::tomb::RelicEffects::scale(self.tombs.effects_of(owner).regen_pct)
                        .max(0.0);
                let regen = if self.tombs.effects_of(owner).regen_pct <= -100 {
                    0.0
                } else {
                    regen
                };
                state.health = (REGEN_PER_SECOND * regen)
                    .mul_add(dt, state.health)
                    .min(state.max_health);
            }
            if !downed {
                self.use_pilot_hands(&body, &mut state, controls, owner, next_tick, dt, output);
            }
            state.health = quantize(state.health);
            state.oxygen = quantize(state.oxygen);
            state.jetpack_fuel = quantize(state.jetpack_fuel);
            self.entities.update(id, |entity| {
                entity.body = body;
                entity.state = EntityState::Pilot(state);
            });
            if damage > 0.0 {
                let _ = self.damage_pilot(id, damage, output);
            }
        }
    }

    /// Oxygen for one tick; returns suffocation damage.
    fn breathe(
        &self,
        body: &Body,
        state: &mut PilotState,
        dt: f32,
        owner: PlayerId,
        output: &mut TickOutput,
    ) -> f32 {
        let (hx, hy) = state.head(body);
        let mut air = self.pilot_air_at(hx, hy);
        if air == PilotAir::Thin && self.lunar_air(owner, hx, hy) {
            // WP-MOON: the colony's rooms and the rig's life-support air line.
            air = PilotAir::Breathable;
        }
        let before = state.oxygen;
        if air.drains() && !self.damage_immune() {
            state.oxygen = (state.oxygen - dt).max(0.0);
        } else {
            state.oxygen = dt
                .mul_add(OXYGEN_REFILL_RATE, state.oxygen)
                .min(state.max_oxygen);
        }
        let warning = state.max_oxygen * OXYGEN_WARNING_SHARE;
        if before > warning && state.oxygen <= warning {
            output.push_event(GameplayEventKind::PilotOxygenLow {
                player_id: owner,
                empty: false,
            });
        }
        if before > 0.0 && state.oxygen <= 0.0 {
            output.push_event(GameplayEventKind::PilotOxygenLow {
                player_id: owner,
                empty: true,
            });
        }
        if state.oxygen <= 0.0 && air.drains() {
            SUFFOCATION_DAMAGE_PER_SECOND * dt
        } else {
            0.0
        }
    }

    fn touches_lava(&self, body: &Body, state: &PilotState) -> bool {
        let left = self.frame().tile_at(body.x - PILOT_HALF_WIDTH, body.y).x;
        let right = self.frame().tile_at(body.x + PILOT_HALF_WIDTH, body.y).x;
        let top = self.frame().tile_at(body.x, body.y - state.height()).y;
        let bottom = self.frame().tile_at(body.x, body.y + 1.0).y;
        (top..=bottom)
            .any(|y| (left..=right).any(|x| self.terrain.is_lava_at(TilePosition { x, y })))
    }

    /// The tile the hand pick would hit for `controls`: the first tile within
    /// [`PICK_REACH`] along the aim (down while crouching, up while holding up, else the free
    /// aim or the facing). `None` if only air is in reach.
    #[must_use]
    pub fn pick_target(
        &self,
        body: &Body,
        state: &PilotState,
        controls: ControlState,
    ) -> Option<TilePosition> {
        let (cx, cy) = state.middle(body);
        let weapon = controls.weapon;
        let (mut ax, mut ay) = if controls.drill_down {
            (0.0, 1.0)
        } else if controls.thrust && controls.horizontal().abs() < 0.2 {
            (0.0, -1.0)
        } else if !weapon.auto_aim && (weapon.aim_x != 0 || weapon.aim_y != 0) {
            (f32::from(weapon.aim_x), f32::from(weapon.aim_y))
        } else {
            (f32::from(state.facing), 0.0)
        };
        let length = ax.hypot(ay).max(f32::EPSILON);
        ax /= length;
        ay /= length;
        // Samples every 4 px out to the reach.
        (1..=PICK_SAMPLES).find_map(|step| {
            let travelled = f32::from(step) * 4.0;
            let tile = self
                .frame()
                .tile_at(ax.mul_add(travelled, cx), ay.mul_add(travelled, cy));
            self.terrain.is_solid_at(tile).then_some(tile)
        })
    }

    pub(super) fn swing_pick(
        &mut self,
        body: &Body,
        state: &mut PilotState,
        controls: ControlState,
        owner: PlayerId,
        dt: f32,
        output: &mut TickOutput,
    ) {
        if !controls.weapon.fire || state.mode == PilotMode::Ledge {
            state.pick = None;
            return;
        }
        let Some(target) = self.pick_target(body, state, controls) else {
            state.pick = None;
            return;
        };
        let kind = self.terrain.tile(target).map(|tile| tile.kind);
        // Sky temple fittings (T21, `world/sky_temples.rs`): a swing works a lever, prises a
        // keystone, opens a keystone gate or breaks a relic chest.
        if let Some(TileKind::Sky(fitting)) = kind
            && fitting.is_temple_fitting()
        {
            state.pick = None;
            if state.pick_cooldown <= 0.0 {
                state.pick_cooldown = PICK_CLINK_SECONDS;
                if !self.work_temple_fitting(owner, target, fitting, output) {
                    output.push_event(GameplayEventKind::PickClinked { player_id: owner });
                }
            }
            return;
        }
        // The alien facility's pylons and conduits answer the pick (WP-MOON).
        if let Some(found) = kind
            && matches!(found, TileKind::Lunar(_))
            && state.pick_cooldown <= 0.0
            && self.pick_facility(owner, target, found, output)
        {
            state.pick = None;
            state.pick_cooldown = PICK_CLINK_SECONDS;
            output.push_event(GameplayEventKind::PickStruck {
                player_id: owner,
                x: target.x,
                y: target.y,
                broke: false,
            });
            return;
        }
        let Some(needed) = kind.and_then(pick_seconds) else {
            state.pick = None;
            if state.pick_cooldown <= 0.0 {
                state.pick_cooldown = PICK_CLINK_SECONDS;
                output.push_event(GameplayEventKind::PickClinked { player_id: owner });
            }
            return;
        };
        let before = state
            .pick
            .filter(|pick| pick.target == target)
            .map_or(0.0, |pick| pick.seconds);
        let after = before + dt;
        let swing = |seconds: f32| (seconds / PICK_SWING_SECONDS).floor();
        if swing(before) < swing(after) && after < needed {
            output.push_event(GameplayEventKind::PickStruck {
                player_id: owner,
                x: target.x,
                y: target.y,
                broke: false,
            });
        }
        if after < needed {
            state.pick = Some(PickProgress {
                target,
                seconds: quantize(after),
            });
            return;
        }
        state.pick = None;
        let Some(kind) = kind else {
            return;
        };
        if self.terrain.set_kind(target, TileKind::Air) {
            let (mx, my) = state.middle(body);
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let (tx, ty) = (
                (self.frame().rel_col(target.x) + 0.5) * TILE_SIZE,
                (self.frame().rel_row(target.y) + 0.5) * TILE_SIZE,
            );
            let distance = (mx - tx).hypot(my - ty).max(1.0);
            let velocity = (
                quantize((mx - tx) / distance * 40.0),
                quantize(((my - ty) / distance).mul_add(40.0, -40.0)),
            );
            let _ = self.spawn_tile_drop(target, kind, velocity);
            output.push_event(GameplayEventKind::PickStruck {
                player_id: owner,
                x: target.x,
                y: target.y,
                broke: true,
            });
        }
    }

    // --- Damage and death -------------------------------------------------------------------

    /// Takes `amount` health off pilot `id` (no guard: hazards, falls, suffocation, blasts).
    /// A pilot at zero health dies ([`GameplayEventKind::PilotDied`]). Returns whether any
    /// damage was taken (none in god mode).
    pub fn damage_pilot(&mut self, id: EntityId, amount: f32, output: &mut TickOutput) -> bool {
        if amount <= 0.0 || self.damage_immune() {
            return false;
        }
        // Tomb runs (WP-TOMBS): the downed take nothing, relics scale the rest.
        let owner = match self.entities.get(id).map(|entity| entity.state) {
            Some(EntityState::Pilot(state)) => state.owner,
            _ => return false,
        };
        let Some(scale) = self.tomb_damage_taken(owner) else {
            return false;
        };
        // A crypt pilot's dodge roll (WP-TOMBS T29): nothing lands mid-roll.
        if matches!(
            self.entities.get(id).map(|entity| entity.state),
            Some(EntityState::Pilot(state)) if state.roll_guarded()
        ) {
            return false;
        }
        let amount = amount * scale;
        let Some((owner, health, x, y)) = self
            .entities
            .update(id, |entity| {
                let (x, y) = (entity.body.x, entity.body.y);
                match &mut entity.state {
                    EntityState::Pilot(state) => {
                        state.health = quantize((state.health - amount).max(0.0));
                        state.calm = 0.0;
                        Some((state.owner, state.health, x, y))
                    }
                    _ => None,
                }
            })
            .flatten()
        else {
            return false;
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "hit sizes are small positive numbers"
        )]
        let shown = amount.round().clamp(0.0, f32::from(u16::MAX)) as u16;
        if shown > 0 {
            output.push_event(GameplayEventKind::PilotHurt {
                player_id: owner,
                amount: shown,
                x: px(x),
                y: px(y),
            });
        }
        if health <= 0.0 {
            self.kill_pilot(id, output);
        }
        true
    }

    /// An enemy's hit on a pilot from `from`: no hit in safe zones, in god mode or inside the
    /// pilot's strike guard; otherwise knockback, the guard starts and the (difficulty-scaled)
    /// damage lands. Returns whether the hit landed.
    #[cfg(test)]
    pub(super) fn strike_pilot(
        &mut self,
        id: EntityId,
        damage: f32,
        from: (f32, f32),
        knockback: f32,
        output: &mut TickOutput,
    ) -> bool {
        self.strike_pilot_as(None, id, damage, from, knockback, output)
    }

    /// [`Self::strike_pilot`] by an enemy of kind `attacker`: night monsters (WP-NIGHT) reach
    /// pilots on the open surface, where the deep enemies' safe zones do not apply.
    pub(super) fn strike_pilot_as(
        &mut self,
        attacker: Option<crate::enemy::EnemyKind>,
        id: EntityId,
        damage: f32,
        from: (f32, f32),
        knockback: f32,
        output: &mut TickOutput,
    ) -> bool {
        if self.damage_immune() {
            return false;
        }
        let Some((body, state)) = self.entities.get(id).and_then(|entity| match entity.state {
            EntityState::Pilot(state) => Some((entity.body, state)),
            _ => None,
        }) else {
            return false;
        };
        let (mx, my) = state.middle(&body);
        if state.hurt > 0.0
            || state.roll_guarded()
            || self.hostile_safe_zone(attacker, self.frame().tile_at(mx, my))
        {
            return false;
        }
        // Seen from above (a crypt, WP-TOMBS T29) a knock has no "up": straight away.
        let lift = if state.view == crate::pilot::PilotView::TopDown {
            0.0
        } else {
            -0.35
        };
        let along_y = if state.view == crate::pilot::PilotView::TopDown {
            1.0
        } else {
            0.4
        };
        let (dx, dy) = (mx - from.0, my - from.1);
        let distance = dx.hypot(dy).max(1.0);
        let push = knockback.min(260.0);
        self.entities.update(id, |entity| {
            entity.body.velocity_x =
                quantize((dx / distance).mul_add(push, entity.body.velocity_x));
            entity.body.velocity_y = quantize(push.mul_add(
                lift,
                (dy / distance).mul_add(push * along_y, entity.body.velocity_y),
            ));
            if let EntityState::Pilot(state) = &mut entity.state {
                state.hurt = STRIKE_GUARD_SECONDS;
                if state.mode == PilotMode::Ledge || state.mode == PilotMode::Climb {
                    state.mode = PilotMode::Free;
                }
            }
        });
        let scaled = damage * self.enemy_difficulty().damage() * state.gear.armor_factor();
        let _ = self.damage_pilot(id, scaled, output);
        true
    }

    /// Pilots within reach of `(x, y)` (their middle), nearest first: `(entity, middle)`.
    pub(super) fn pilots_near(&self, x: f32, y: f32, reach: f32) -> Vec<(EntityId, (f32, f32))> {
        let mut found: Vec<(f32, EntityId, (f32, f32))> = self
            .pilots()
            .filter_map(|(id, body, state)| {
                let middle = state.middle(body);
                let distance = (middle.0 - x).hypot(middle.1 - y);
                (distance <= reach).then_some((distance, id, middle))
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        found
            .into_iter()
            .map(|(_, id, middle)| (id, middle))
            .collect()
    }

    /// This tick's blasts push and hurt pilots (their own bombs too: stand back).
    pub(super) fn pilots_feel_blasts(&mut self, blasts: &[Blast], output: &mut TickOutput) {
        if self.entities.count_of(EntityKind::Pilot) == 0 {
            return;
        }
        for blast in blasts {
            let reach = crate::combat::explosive_damage_reach(blast.kind);
            let peak = crate::combat::explosive_damage(blast.kind, false) * PILOT_BLAST_SHARE;
            let push = crate::physics::Blast::explosive(blast.kind, blast.x, blast.y);
            for (id, (mx, my)) in self.pilots_near(blast.x, blast.y, reach) {
                if !crate::physics::line_clear(&self.terrain, blast.x, blast.y, mx, my) {
                    continue;
                }
                if let Some(push) = push {
                    let terrain = &self.terrain;
                    self.entities.update(id, |entity| {
                        let mut probe = entity.body;
                        probe.y = my;
                        let _ =
                            crate::physics::apply_blast(&mut probe, &push, terrain, mx < blast.x);
                        entity.body.velocity_x = quantize(probe.velocity_x.clamp(-420.0, 420.0));
                        entity.body.velocity_y = quantize(probe.velocity_y.clamp(-520.0, 520.0));
                        if let EntityState::Pilot(state) = &mut entity.state
                            && state.mode != PilotMode::Swim
                        {
                            state.mode = PilotMode::Free;
                        }
                    });
                }
                let distance = (mx - blast.x).hypot(my - blast.y);
                let armor = self
                    .entities
                    .get(id)
                    .and_then(|entity| match entity.state {
                        EntityState::Pilot(state) => Some(state.gear.armor_factor()),
                        _ => None,
                    })
                    .unwrap_or(1.0);
                let amount = peak * (1.0 - distance / reach) * armor;
                if amount > 0.0 {
                    let _ = self.damage_pilot(id, amount, output);
                }
            }
        }
    }

    /// A pilot at zero health: drops part of its pack, then respawns in the rig (or, if the
    /// rig is wrecked, on foot at the player's home point when there is one).
    fn kill_pilot(&mut self, id: EntityId, output: &mut TickOutput) {
        // In a tomb run (WP-TOMBS) a member is downed, not killed.
        if let Some(EntityState::Pilot(state)) = self.entities.get(id).map(|entity| entity.state)
            && self.down_tomb_member(state.owner, id, output)
        {
            return;
        }
        let Some(entity) = self.entities.despawn(id) else {
            return;
        };
        let EntityState::Pilot(state) = entity.state else {
            return;
        };
        let owner = state.owner;
        let (x, y) = state.middle(&entity.body);
        output.push_event(GameplayEventKind::PilotDied {
            player_id: owner,
            x: px(x),
            y: px(y),
        });
        let mut dropped = Vec::new();
        if let Some(player) = self.players.get_mut(&owner) {
            let stacks: Vec<(ItemKind, u32)> =
                player.pilot_pack.iter().take(DEATH_DROP_STACKS).collect();
            for (kind, count) in stacks {
                let lost = count.div_ceil(2);
                let taken = player.pilot_pack.remove(kind, lost);
                dropped.push(ItemStack::new(kind, taken));
            }
        }
        for (index, stack) in dropped.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a few stacks")]
            let spread = (index as f32 - 1.0) * 60.0;
            let _ = self.spawn_loot(x, y, (spread, -120.0), stack, None);
        }
        let rig_wrecked = self
            .players
            .get(&owner)
            .is_none_or(|player| player.hull <= 0.0);
        if !rig_wrecked {
            // Back in the rig: what the pack still holds goes aboard, as when boarding.
            let _moved = self.empty_pack_into_rig(owner);
        }
        if rig_wrecked && let Some((home_x, home_y)) = self.rescue_home(owner) {
            let gear = self
                .players
                .get(&owner)
                .map(|p| p.pilot_gear)
                .unwrap_or_default();
            let body = Body::at(
                quantize(home_x),
                quantize(TILE_SIZE.mul_add(0.5, home_y) - 0.5),
            );
            let mut state = PilotState::new(owner, gear, 1);
            state.weapon = self
                .players
                .get(&owner)
                .and_then(|player| player.pilot_arsenal.equipped);
            let _ = self.spawn_entity(body, EntityState::Pilot(state), None);
        }
    }

    // --- Pickups ----------------------------------------------------------------------------

    /// Pilots pick up loose drops they touch into the personal pack (no magnet). Runs after
    /// the rigs' pickup pass.
    pub(super) fn advance_pilot_pickups(&mut self, output: &mut TickOutput) {
        let pilots: Vec<(PlayerId, f32, f32)> = self
            .pilots()
            .map(|(_, body, state)| {
                let (x, y) = state.middle(body);
                (state.owner, x, y)
            })
            .collect();
        if pilots.is_empty() || self.entities.count_of(EntityKind::Drop) == 0 {
            return;
        }
        let mut collected = std::collections::BTreeSet::new();
        for (owner, x, y) in pilots {
            for id in self.entities.ids_in_radius(x, y, PILOT_PICKUP_RADIUS) {
                if collected.contains(&id) {
                    continue;
                }
                let Some((stack, ignored, dx, dy)) =
                    self.entities.get(id).and_then(|entity| match entity.state {
                        EntityState::Drop(drop) => {
                            Some((drop.stack, drop.ignored_by, entity.body.x, entity.body.y))
                        }
                        _ => None,
                    })
                else {
                    continue;
                };
                if ignored == Some(owner) || (dx - x).hypot(dy - y) > PILOT_PICKUP_RADIUS {
                    continue;
                }
                let Some(player) = self.players.get_mut(&owner) else {
                    continue;
                };
                // Keycards and temple keystones ride on the key ring, not in the pack: a
                // chest's loot filling the pack must never strand the key that opens the vault.
                let room = if crate::mechanism::rides_key_ring(stack.kind) {
                    stack.count
                } else {
                    player
                        .pilot_gear
                        .pack_capacity()
                        .saturating_sub(player.pilot_pack.total())
                };
                let taken = stack.count.min(room);
                if taken == 0 {
                    continue;
                }
                player.pilot_pack.add(stack.kind, taken);
                output.push_event(GameplayEventKind::ItemsCollected {
                    player_id: owner,
                    item: stack.kind,
                    count: taken,
                    x: px(dx),
                    y: px(dy),
                });
                if taken >= stack.count {
                    collected.insert(id);
                } else {
                    self.entities.update(id, |entity| {
                        if let EntityState::Drop(drop) = &mut entity.state {
                            drop.stack.count -= taken;
                        }
                    });
                }
            }
        }
        self.entities.despawn_many(&collected);
    }

    // --- Persistence ------------------------------------------------------------------------

    /// Takes `player_id`'s pilot out of the world for parking (WP-PERSIST).
    pub(super) fn take_pilot_for_parking(&mut self, player_id: PlayerId) -> Option<ParkedPilot> {
        let (id, _, _) = self.pilot_of(player_id)?;
        let entity = self.entities.despawn(id)?;
        match entity.state {
            EntityState::Pilot(state) => Some(ParkedPilot {
                body: Body {
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                    ..entity.body
                },
                state,
            }),
            _ => None,
        }
    }

    /// Puts a returning player's pilot back where it was (or in its rig if that spot has
    /// since been filled in).
    pub(super) fn restore_parked_pilot(&mut self, parked: ParkedPilot) {
        let view = self.pilot_view(None);
        if box_blocked(&view, parked.body.x, parked.body.y, parked.state.height()) {
            return;
        }
        let mut body = parked.body;
        body.previous_x = body.x;
        body.previous_y = body.y;
        let _ = self.spawn_entity(body, EntityState::Pilot(parked.state), None);
    }

    // --- Scenarios --------------------------------------------------------------------------

    /// Spawns `player_id`'s pilot out of its rig at feet position `(x, y)` (tests, screenshot
    /// scenes and scripted set pieces). Replaces a pilot already out. Returns its entity.
    pub fn place_pilot(&mut self, player_id: PlayerId, x: f32, y: f32) -> Option<EntityId> {
        let player = self.players.get(&player_id)?;
        let gear = player.pilot_gear;
        let mut state = PilotState::new(player_id, gear, 1);
        state.weapon = player.pilot_arsenal.equipped;
        if let Some((id, _, _)) = self.pilot_of(player_id) {
            let _ = self.entities.despawn(id);
        }
        Some(self.spawn_entity(
            Body::at(quantize(x), quantize(y)),
            EntityState::Pilot(state),
            None,
        ))
    }

    /// Rewrites pilot `id`'s state with `change` (tests and scenes: poses, health, oxygen).
    pub fn edit_pilot(&mut self, id: EntityId, change: impl FnOnce(&mut Body, &mut PilotState)) {
        self.entities.update(id, |entity| {
            let body = &mut entity.body;
            if let EntityState::Pilot(state) = &mut entity.state {
                change(body, state);
            }
        });
    }

    /// Carves a crawlway: a 1-tile-high tunnel `length` tiles long going right from `start`,
    /// capped with a vent grate at each end, so only a pilot on foot gets through (rigs, drops
    /// and enemies are stopped by the grates). The pilot-only space type of F5; generators can
    /// stamp the same shape ([`crate::poi::SpaceAccess::RequiresPilot`]). Returns the tiles
    /// written.
    pub fn carve_crawlway(&mut self, start: TilePosition, length: i32) -> Vec<TilePosition> {
        let mut written = Vec::new();
        for dx in 0..length.max(2) {
            let tile = TilePosition {
                x: start.x + dx,
                y: start.y,
            };
            let kind = if dx == 0 || dx == length.max(2) - 1 {
                TileKind::Built(crate::base::BuiltTile::VentGrate)
            } else {
                TileKind::Air
            };
            if self.terrain.set_kind(tile, kind) {
                written.push(tile);
            }
        }
        written
    }
}

/// Puts `gear`'s capacities on a live pilot (health and air keep their share).
fn refit(state: &mut PilotState, gear: PilotGear) {
    let health_share = state.health / state.max_health.max(1.0);
    let air_share = state.oxygen / state.max_oxygen.max(1.0);
    state.gear = gear;
    state.max_health = gear.max_health();
    state.health = quantize(state.max_health * health_share);
    state.max_oxygen = gear.max_oxygen();
    state.oxygen = quantize(state.max_oxygen * air_share);
    state.jetpack_capacity = gear.jetpack_seconds();
    state.jetpack_fuel = state.jetpack_capacity;
}
