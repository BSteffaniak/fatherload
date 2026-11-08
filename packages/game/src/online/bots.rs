//! Scripted bot players for headless soak runs and tests: dig a shaft (scanning on the way), drop
//! a bomb and fly out, travel to the depot and sell, refuel when low, and walk back to dig again.
//!
//! A bot only ever produces the same held controls and one-shot actions a human seat would, so it
//! exercises exactly the production input path (including the host's building-zone validation of
//! service actions).

use drillgame_simulation::{
    ControlState, PlayerCommand, PlayerId, SessionServiceRequest, TILE_SIZE, WorldState,
    economy::SurfaceZone, surface::SURFACE_BUILDINGS,
};

/// Longest a bot digs before flying out.
const DIG_SECONDS: f32 = 6.0;
/// Longest a bot flies up before giving up and travelling anyway.
const FLY_SECONDS: f32 = 8.0;
/// Longest a bot travels before giving up and digging where it is.
const TRAVEL_SECONDS: f32 = 20.0;
/// Pause after a transaction before moving on.
const SERVICE_WAIT_SECONDS: f32 = 0.6;
/// Minimum time between rescue requests while stranded.
const RESCUE_RETRY_SECONDS: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BotPhase {
    Dig,
    Fly,
    Travel(SurfaceZone),
    Service(SurfaceZone),
    ToDigSite,
}

/// Counters describing what a bot has done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BotCounters {
    /// Completed dig/sell cycles.
    pub cycles: u64,
    /// Bombs requested.
    pub bombs: u64,
    /// Sales requested.
    pub sales: u64,
    /// Refuels requested.
    pub refuels: u64,
    /// Rescues requested.
    pub rescues: u64,
    /// Every one-shot action issued.
    pub actions: u64,
}

/// One scripted bot.
#[derive(Clone, Debug)]
pub struct BotBrain {
    phase: BotPhase,
    phase_seconds: f32,
    acted: bool,
    rescue_cooldown: f32,
    dig_offset_tiles: f32,
    counters: BotCounters,
}

impl BotBrain {
    /// A bot; `index` spreads bots over different dig columns.
    #[must_use]
    pub fn new(index: usize) -> Self {
        #[allow(
            clippy::cast_precision_loss,
            reason = "small bot index picks one of a few columns"
        )]
        let column = (index % 4) as f32;
        Self {
            phase: BotPhase::ToDigSite,
            phase_seconds: 0.0,
            acted: false,
            rescue_cooldown: 0.0,
            dig_offset_tiles: column.mul_add(-2.0, -2.5),
            counters: BotCounters::default(),
        }
    }

    /// What the bot has done so far.
    #[must_use]
    pub const fn counters(&self) -> BotCounters {
        self.counters
    }

    /// Controls and actions for the next `delta_seconds` of play.
    pub fn decide(
        &mut self,
        world: &WorldState,
        player: PlayerId,
        delta_seconds: f32,
    ) -> (ControlState, Vec<PlayerCommand>) {
        self.phase_seconds += delta_seconds;
        self.rescue_cooldown = (self.rescue_cooldown - delta_seconds).max(0.0);
        let mut actions = Vec::new();
        let Some(rig) = world.player(player) else {
            return (ControlState::default(), actions);
        };
        if world.failure_state(player).is_some() {
            if self.rescue_cooldown <= 0.0 {
                self.rescue_cooldown = RESCUE_RETRY_SECONDS;
                self.counters.rescues += 1;
                self.push(&mut actions, PlayerCommand::Rescue);
                self.enter(BotPhase::ToDigSite);
            }
            return (ControlState::default(), actions);
        }
        let surface_y = TILE_SIZE.mul_add(0.5, world.player_template().y);
        let controls = match self.phase {
            BotPhase::Dig => {
                if !self.acted && self.phase_seconds > 0.5 {
                    self.acted = true;
                    self.push(&mut actions, PlayerCommand::UseScanner);
                }
                if self.phase_seconds > DIG_SECONDS || !rig.has_cargo_space() {
                    self.enter(BotPhase::Fly);
                    if rig.bombs > 0 {
                        self.counters.bombs += 1;
                        self.push(&mut actions, PlayerCommand::PlaceBomb);
                    }
                }
                ControlState::new(0.0, false, true)
            }
            BotPhase::Fly => {
                if rig.y <= surface_y || self.phase_seconds > FLY_SECONDS {
                    self.enter(BotPhase::Travel(SurfaceZone::Depot));
                }
                ControlState::new(0.0, true, false)
            }
            BotPhase::Travel(zone) => {
                if world.player_surface_zone(player) == Some(zone)
                    && rig.velocity_x.abs() < TILE_SIZE
                {
                    self.enter(BotPhase::Service(zone));
                    ControlState::default()
                } else if self.phase_seconds > TRAVEL_SECONDS {
                    self.enter(BotPhase::Dig);
                    ControlState::default()
                } else {
                    steer(rig.x, rig.y, building_center_x(zone), surface_y)
                }
            }
            BotPhase::Service(zone) => {
                if !self.acted {
                    self.acted = true;
                    let request = if zone == SurfaceZone::Fuel {
                        self.counters.refuels += 1;
                        SessionServiceRequest::Refuel { menu_item: 2 }
                    } else {
                        self.counters.sales += 1;
                        SessionServiceRequest::SellCargo
                    };
                    self.push(&mut actions, PlayerCommand::Service(request));
                }
                if self.phase_seconds > SERVICE_WAIT_SECONDS {
                    if zone == SurfaceZone::Depot && rig.fuel < rig.fuel_capacity * 0.5 {
                        self.enter(BotPhase::Travel(SurfaceZone::Fuel));
                    } else {
                        self.counters.cycles += 1;
                        self.enter(BotPhase::ToDigSite);
                    }
                }
                ControlState::default()
            }
            BotPhase::ToDigSite => {
                let target = self.dig_site_x();
                // Centre over the column so the rig fits down the shaft it digs.
                let settled = (rig.x - target).abs() < TILE_SIZE * 0.1
                    && rig.velocity_x.abs() < TILE_SIZE * 0.2;
                if settled || self.phase_seconds > TRAVEL_SECONDS {
                    self.enter(BotPhase::Dig);
                    ControlState::default()
                } else {
                    steer(rig.x, rig.y, target, surface_y)
                }
            }
        };
        (controls, actions)
    }

    /// A dig column just left of the depot (its foundation cannot be dug).
    fn dig_site_x(&self) -> f32 {
        let depot = building_left_x(SurfaceZone::Depot);
        (depot / TILE_SIZE + self.dig_offset_tiles)
            .floor()
            .mul_add(TILE_SIZE, TILE_SIZE * 0.5)
    }

    const fn enter(&mut self, phase: BotPhase) {
        self.phase = phase;
        self.phase_seconds = 0.0;
        self.acted = false;
    }

    fn push(&mut self, actions: &mut Vec<PlayerCommand>, command: PlayerCommand) {
        self.counters.actions += 1;
        actions.push(command);
    }
}

/// Horizontal steering toward `target_x`, hopping when below the surface line.
fn steer(x: f32, y: f32, target_x: f32, surface_y: f32) -> ControlState {
    let horizontal = ((target_x - x) / (TILE_SIZE * 2.0)).clamp(-1.0, 1.0);
    ControlState::new(horizontal, y > surface_y, false)
}

fn building_left_x(zone: SurfaceZone) -> f32 {
    SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == zone)
        .map_or(0.0, |building| {
            #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
            let left = building.tile_x as f32 * TILE_SIZE;
            left
        })
}

fn building_center_x(zone: SurfaceZone) -> f32 {
    SURFACE_BUILDINGS
        .iter()
        .find(|building| building.zone == zone)
        .map_or(0.0, |building| {
            #[allow(clippy::cast_precision_loss, reason = "small tile coordinate")]
            let center = (building.tile_x as f32 + building.tile_width as f32 / 2.0) * TILE_SIZE;
            center
        })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        local_game::new_world,
        runtime::{Runtime, RuntimeEventKind, SeatInput},
    };

    #[test]
    fn a_bot_digs_bombs_and_trades_at_the_depot_through_the_host() {
        let mut world = new_world();
        let rig = world.player_ids().next().expect("one rig");
        world.player_mut(rig).expect("rig").bombs = 3;
        let mut runtime = Runtime::local(world, 1);
        let seat = runtime.local_seats()[0];
        let mut bot = BotBrain::new(0);
        let mut deepest = 0.0_f32;
        let mut sales_resolved = 0;
        let mut bombs_applied = 0;
        let dt = 1.0 / 60.0;
        for _ in 0..60 * 90 {
            let (controls, actions) = bot.decide(runtime.world(), seat.player, dt);
            runtime.submit(seat.seat, SeatInput { controls, actions });
            let frame = runtime.advance(Duration::from_secs_f32(dt));
            for event in frame.events {
                if let RuntimeEventKind::ActionResolved { command, applied } = event.event {
                    match command {
                        PlayerCommand::Service(SessionServiceRequest::SellCargo) if applied => {
                            sales_resolved += 1;
                        }
                        PlayerCommand::PlaceBomb if applied => bombs_applied += 1,
                        _ => {}
                    }
                }
            }
            if let Some(rig) = runtime.world().player(seat.player) {
                deepest = deepest.max(rig.y);
                if std::env::var("BOT_TRACE").is_ok()
                    && runtime.world().simulation_tick().get().is_multiple_of(30)
                {
                    eprintln!(
                        "{:?} x={:.1} y={:.1} vx={:.1} fuel={:.0} cargo={}",
                        bot.phase,
                        rig.x / TILE_SIZE,
                        rig.y / TILE_SIZE,
                        rig.velocity_x,
                        rig.fuel,
                        rig.cargo_used()
                    );
                }
            }
            if sales_resolved > 0 && bombs_applied > 0 {
                break;
            }
        }
        let surface = runtime.world().player_template().y;
        assert!(
            deepest > TILE_SIZE.mul_add(2.0, surface),
            "bot dug: {deepest} vs {surface}"
        );
        assert!(bombs_applied > 0, "bot bombed: {:?}", bot.counters());
        assert!(
            sales_resolved > 0,
            "bot reached the depot and sold: {:?}",
            bot.counters()
        );
    }
}
