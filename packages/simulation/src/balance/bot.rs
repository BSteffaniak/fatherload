//! The economy bot: a scripted miner that plays the drill/sell/upgrade loop through the real
//! tick, the way a sensible new player would.
//!
//! A trip: fly to a shaft column in a gap between the town's buildings, drop down the shaft,
//! drill deeper, and at every new row tunnel sideways to the ore it can see within reach
//! ([`Strategy::Prospector`] and [`Strategy::Charges`]); the charges strategy also blasts rows
//! rich in ore with mining charges from a safe distance up the shaft (pressed at its own
//! surface workshop from the blocks it hauls). Hazards (lava, gas, pockets, vents) and rock the
//! drill cannot bite are side-stepped; a band hotter than the rig's cooling, bedrock or no way
//! around end the shaft and the next trip starts a fresh one. The
//! bot flies home when the bay is full, the tank holds only the fuel to climb out (measured on
//! earlier climbs) plus a margin, or the hull is low; in town it sells ore and blocks, buys
//! upgrades (cheapest weighted line first, keeping a reserve for fuel and repairs), arms itself
//! with a rivet gun (and the boulder deflector once boulders hurt), refuels and repairs. Enemies within reach get auto-aimed fire. A stranded
//! or wrecked rig calls the rescue, like a player would.
//!
//! The bot reads the terrain around its rig (a player sees it on screen and on the scanner)
//! and its own rig, never hidden state.

use crate::{
    ControlState, ExplosiveKind, GameplayEventKind, PlayerCommand, PlayerId, SessionServiceRequest,
    ThrowAim, TickOutput, WeaponControl, WeaponKind, WorldState,
    economy::{
        SurfaceZone, UpgradeKind, current_level, upgrade_locked_by, upgrade_maxed, upgrade_offers,
    },
    explosive::explosive_offer,
    player::Player,
    protection::heat_excess,
    surface::{SURFACE_BUILDINGS, surface_building_at_tile},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    world::DrillContact,
};

/// How the bot mines.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub enum Strategy {
    /// Straight shafts only: drill down, take what the shaft cuts through.
    Shaft,
    /// Shafts plus sideways tunnels to the ore visible within [`SPUR_REACH`] tiles of each row.
    Prospector,
    /// The prospector, plus mining charges on rows rich in ore (bought at the shack).
    Charges,
}

impl Strategy {
    /// Every strategy, in report order.
    pub const ALL: [Self; 3] = [Self::Shaft, Self::Prospector, Self::Charges];

    /// Lowercase name (report rows, `--strategies`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shaft => "shaft",
            Self::Prospector => "prospector",
            Self::Charges => "charges",
        }
    }

    /// Sideways reach of the ore tunnels.
    #[must_use]
    pub const fn spur_reach(self) -> i32 {
        match self {
            Self::Shaft => 0,
            Self::Prospector | Self::Charges => SPUR_REACH,
        }
    }
}

/// Tiles the prospector tunnels sideways for ore.
pub const SPUR_REACH: i32 = 7;
/// Shaft columns are this far apart, so tunnels of neighbouring shafts do not overlap.
const SHAFT_SPACING: i32 = 18;
/// The first shaft (between the Repair Garage and the depot).
const FIRST_SHAFT: i32 = 78;
/// Falling faster than this (px/s) makes the bot brake with thrust.
const BRAKE_SPEED: f32 = 230.0;
/// A tile that takes longer than this to drill means the drill is outclassed.
const SLOW_TILE_SECONDS: f32 = 9.0;
/// No progress for this long while travelling means the bot is stuck (it calls the rescue).
const STUCK_SECONDS: f32 = 8.0;
/// A trip never lasts longer than this.
const MAX_TRIP_SECONDS: f32 = 600.0;
/// Fly home below this fraction of the hull.
const HULL_RETURN_FRACTION: f32 = 0.4;
/// Enemies this close (tiles) draw fire.
const ENGAGE_TILES: f32 = 8.0;
/// Ore tiles inside a mining charge's reach that make a row worth a charge.
const CHARGE_MIN_ORE: usize = 4;
/// Rows the bot climbs above an armed mining charge before it blows.
const CHARGE_RETREAT_ROWS: i32 = 8;
/// Mining charges the charges strategy keeps on board.
const CHARGE_STOCK: u32 = 4;
/// Credits the bot always keeps besides fuel and repairs.
const CREDIT_FLOOR: u32 = 40;

/// One straight run of a shaft: column `col` from row `top` down to row `bottom`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Segment {
    col: i32,
    top: i32,
    bottom: i32,
}

/// What the bot is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Flying over town to the shaft's top.
    ToShaft,
    /// Following the shaft down to its bottom (segment index).
    Descend(usize),
    /// At the shaft bottom: plan the row, drill down, decide to go home.
    Mine,
    /// Tunnelling sideways to `target` on `row`, then back to the shaft (`back`).
    Spur { target: i32, row: i32, back: bool },
    /// Moving the shaft one column over around a hazard.
    Sidestep { target: i32, row: i32 },
    /// A mining charge is armed below: climb to `row`, wait until `until` seconds.
    Blast { row: i32, until: f32 },
    /// Climbing the shaft home (segment index).
    Ascend(usize),
    /// In town, working through the stops (index of the current stop).
    Town(usize),
}

/// A town errand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    /// The charges strategy's surface claim with a workshop (built once, then crafting).
    Outpost,
    /// Fuel before the other errands when the tank is low.
    LowFuel,
    Depot,
    Shop,
    Shack,
    Repair,
    Fuel,
}

impl Stop {
    const fn zone(self) -> SurfaceZone {
        match self {
            Self::Outpost | Self::Depot => SurfaceZone::Depot,
            Self::Shop => SurfaceZone::Shop,
            Self::Shack => SurfaceZone::Explosives,
            Self::Repair => SurfaceZone::Repair,
            Self::Fuel | Self::LowFuel => SurfaceZone::Fuel,
        }
    }
}

/// What a bot has done (read by the economy probe).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotStats {
    /// Completed trips (back in town).
    pub trips: u32,
    /// Rescues called (stranded, wrecked or stuck).
    pub rescues: u32,
    /// Credits from selling ore and artifacts.
    pub ore_income: u64,
    /// Credits from selling building blocks.
    pub block_income: u64,
    /// Credits spent on upgrades.
    pub upgrade_spend: u64,
    /// Credits spent on fuel, repairs, the rescue and other services.
    pub upkeep_spend: u64,
    /// Credits spent on explosives and weapons.
    pub arms_spend: u64,
    /// Mining charges set off.
    pub charges_used: u32,
    /// Hull lost to enemies.
    pub enemy_damage: f64,
    /// Hull lost to each enemy kind.
    pub enemy_damage_by_kind: std::collections::BTreeMap<crate::EnemyKind, f64>,
    /// Hull lost to everything else (heat, lava, gas, falls, boulders, blasts).
    pub hazard_damage: f64,
    /// Enemies killed.
    pub kills: u32,
    /// Shafts started.
    pub shafts: u32,
    /// Sideways ore tunnels dug.
    pub spurs: u32,
    /// Deepest row the rig reached.
    pub deepest_row: i32,
    /// Upgrades bought, in order: (seconds into the run, line, new level).
    pub purchases: Vec<(f32, UpgradeKind, u8)>,
    /// Why each rescue was called: `(seconds, what the bot was doing, wrecked/stranded/stuck)`.
    pub rescue_log: Vec<(f32, String, &'static str)>,
}

/// The scripted miner of one rig.
#[derive(Clone, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent wishes and latches of a scripted player"
)]
pub struct MinerBot {
    player: PlayerId,
    strategy: Strategy,
    phase: Phase,
    /// Seconds since the run began.
    clock: f32,
    phase_started: f32,
    trip_started: f32,
    /// Shafts dug so far (index into the shaft column sequence).
    shaft_index: i32,
    route: Vec<Segment>,
    /// The current shaft cannot go deeper (bedrock, heat, hardness).
    shaft_done: bool,
    /// Rows whose ore tunnels were already planned.
    planned_row: Option<i32>,
    spur_queue: Vec<i32>,
    stops: Vec<Stop>,
    stop_acted: bool,
    /// Progress watch: the last tile and when it changed.
    last_tile: TilePosition,
    last_move: f32,
    /// Tile under the drill and when drilling it started.
    drill_tile: Option<(TilePosition, f32)>,
    /// Measured fuel per row climbed (the worst seen).
    fuel_per_row: f32,
    climb_from: Option<(f32, i32)>,
    wants_drill: bool,
    wants_cooling: bool,
    fire_toggle: bool,
    hull_before: f32,
    credits_before: u32,
    /// The charges strategy's workshop (and the column it stands on).
    workshop: Option<(crate::base::StructureId, i32)>,
    /// Where the charges strategy stakes its claim.
    outpost_site: Option<i32>,
    pending_sale: bool,
    stats: BotStats,
}

impl MinerBot {
    /// A bot for `player` mining with `strategy`.
    #[must_use]
    pub fn new(player: PlayerId, strategy: Strategy) -> Self {
        Self {
            player,
            strategy,
            phase: Phase::ToShaft,
            clock: 0.0,
            phase_started: 0.0,
            trip_started: 0.0,
            shaft_index: 0,
            route: Vec::new(),
            shaft_done: false,
            planned_row: None,
            spur_queue: Vec::new(),
            stops: Vec::new(),
            stop_acted: false,
            last_tile: TilePosition { x: 0, y: 0 },
            last_move: 0.0,
            drill_tile: None,
            fuel_per_row: 0.9,
            climb_from: None,
            wants_drill: false,
            wants_cooling: false,
            fire_toggle: false,
            hull_before: 0.0,
            credits_before: 0,
            workshop: None,
            outpost_site: None,
            pending_sale: false,
            stats: BotStats::default(),
        }
    }

    /// What the bot has done so far.
    #[must_use]
    pub const fn stats(&self) -> &BotStats {
        &self.stats
    }

    /// The bot's strategy.
    #[must_use]
    pub const fn strategy(&self) -> Strategy {
        self.strategy
    }

    /// Seconds of play so far.
    #[must_use]
    pub const fn clock(&self) -> f32 {
        self.clock
    }

    /// What the bot is doing (debug traces).
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{:?}", self.phase)
    }

    /// Controls and actions for the next tick of `world`.
    pub fn decide(&mut self, world: &WorldState) -> (ControlState, Vec<PlayerCommand>) {
        let mut actions = Vec::new();
        let Some(rig) = world.player(self.player) else {
            return (ControlState::default(), actions);
        };
        self.hull_before = rig.hull;
        self.credits_before = rig.credits;
        if let Some(failure) = world.failure_state(self.player) {
            if self.clock - self.phase_started > 1.0 {
                let cause = if failure.disabled {
                    "wrecked"
                } else {
                    "stranded"
                };
                self.rescue(&mut actions, cause);
            }
            return (ControlState::default(), actions);
        }
        let tile = rig.tile_position(TILE_SIZE);
        if tile != self.last_tile {
            self.last_tile = tile;
            self.last_move = self.clock;
        }
        let stuck = self.clock - self.last_move > STUCK_SECONDS
            && !matches!(
                self.phase,
                Phase::Town(_) | Phase::Blast { .. } | Phase::Mine
            );
        if stuck {
            self.rescue(&mut actions, "stuck");
            return (ControlState::default(), actions);
        }
        let controls = match self.phase {
            Phase::ToShaft => self.head_to_shaft(world, rig),
            Phase::Descend(index) => self.descend(world, rig, index),
            Phase::Mine => self.mine(world, rig, &mut actions),
            Phase::Spur { target, row, back } => self.spur(world, rig, target, row, back),
            Phase::Sidestep { target, row } => self.sidestep(rig, target, row),
            Phase::Blast { row, until } => self.blast(world, rig, row, until),
            Phase::Ascend(index) => self.ascend(world, rig, index),
            Phase::Town(index) => self.town(world, rig, index, &mut actions),
        };
        let controls = controls.with_weapon(self.weapon_control(world, rig));
        (controls, actions)
    }

    /// Reads what the tick did (damage, kills, sales) and advances the bot's clock by one tick.
    pub fn observe(&mut self, world: &WorldState, output: &TickOutput) {
        self.clock += 1.0 / world.tick_rate_hz() as f32;
        let Some(rig) = world.player(self.player) else {
            return;
        };
        let mut struck = None;
        for event in &output.events {
            match event.kind {
                GameplayEventKind::RigStruck { player_id, kind } if player_id == self.player => {
                    struck = Some(kind);
                }
                GameplayEventKind::EnemyKilled { by, .. } if by == Some(self.player) => {
                    self.stats.kills += 1;
                }
                _ => {}
            }
        }
        let lost = f64::from((self.hull_before - rig.hull).max(0.0));
        if lost > 0.0 {
            if let Some(kind) = struck {
                self.stats.enemy_damage += lost;
                *self.stats.enemy_damage_by_kind.entry(kind).or_default() += lost;
            } else {
                self.stats.hazard_damage += lost;
                if std::env::var_os("BALANCE_TRACE_HAZARD").is_some() {
                    eprintln!(
                        "hazard {lost:.1} at {:?} {:?}: {:?}",
                        rig.tile_position(TILE_SIZE),
                        self.phase,
                        output.events.iter().map(|e| &e.kind).collect::<Vec<_>>()
                    );
                }
            }
        }
        if self.pending_sale {
            self.pending_sale = false;
        }
        let tile = rig.tile_position(TILE_SIZE);
        self.stats.deepest_row = self.stats.deepest_row.max(tile.y);
        self.find_workshop(world);
    }

    const fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.phase_started = self.clock;
        self.last_move = self.clock;
    }

    fn rescue(&mut self, actions: &mut Vec<PlayerCommand>, cause: &'static str) {
        actions.push(PlayerCommand::Rescue);
        self.stats.rescues += 1;
        self.stats
            .rescue_log
            .push((self.clock, format!("{:?}", self.phase), cause));
        // The rescue lands the rig in town (or at its home point): start the errands, then a
        // fresh shaft (the old route may be what stranded it).
        self.route.clear();
        self.shaft_done = true;
        self.climb_from = None;
        self.plan_town_stops_after_rescue();
        self.enter(Phase::Town(0));
    }

    fn plan_town_stops_after_rescue(&mut self) {
        self.stops = Self::town_stops();
        self.stop_acted = false;
    }

    /// The `index`th shaft column: alternating either side of the first, in the gaps between
    /// buildings (and beyond the town on endless worlds).
    fn shaft_column(world: &WorldState, index: i32) -> i32 {
        let step = (index + 1) / 2;
        let side = if index % 2 == 1 { 1 } else { -1 };
        let mut col = FIRST_SHAFT + side * step * SHAFT_SPACING;
        // Never under a building's foundation.
        while surface_building_at_tile(col).is_some() {
            col += side;
        }
        if world.terrain().contains_column(col) {
            col
        } else {
            FIRST_SHAFT
        }
    }

    fn surface_y(world: &WorldState, col: i32) -> f32 {
        world.terrain().surface_row(col) as f32 * TILE_SIZE
    }

    fn new_shaft(&mut self, world: &WorldState) {
        if self.shaft_done || self.route.is_empty() {
            if self.shaft_done {
                self.shaft_index += 1;
            }
            let col = Self::shaft_column(world, self.shaft_index);
            let top = world.terrain().surface_row(col) - 1;
            self.route = vec![Segment {
                col,
                top,
                bottom: top,
            }];
            self.shaft_done = false;
            self.stats.shafts += 1;
        }
    }

    /// Fuel to reach the current shaft's bottom, dig a little and climb back out.
    fn shaft_fuel_need(&self, world: &WorldState) -> f32 {
        let Some(last) = self.route.last() else {
            return 0.0;
        };
        let rows = (last.bottom - world.terrain().surface_row(last.col)).max(0) as f32
            + self.route.len() as f32 * 2.0;
        rows * self.fuel_per_row * 1.35 + 30.0
    }

    fn head_to_shaft(&mut self, world: &WorldState, rig: &Player) -> ControlState {
        let deep = self
            .route
            .last()
            .is_some_and(|last| last.bottom > self.route[0].top + 3);
        if deep && rig.fuel < self.shaft_fuel_need(world) {
            // Not enough fuel to work this shaft: start a shallow one.
            self.shaft_done = true;
        }
        self.new_shaft(world);
        let col = self.route[0].col;
        let target_x = centre(col);
        let dx = target_x - rig.x;
        if dx.abs() < TILE_SIZE * 0.12 && rig.velocity_x.abs() < TILE_SIZE * 0.3 {
            self.trip_started = self.clock;
            self.enter(Phase::Descend(0));
            return ControlState::new(0.0, false, false);
        }
        if dx.abs() < TILE_SIZE * 1.2 {
            // Ease over the shaft's mouth and drop in.
            return ControlState::new(steer(dx) * 0.6, rig.velocity_y > BRAKE_SPEED, false);
        }
        travel(world, rig, target_x)
    }

    fn descend(&mut self, world: &WorldState, rig: &Player, index: usize) -> ControlState {
        let Some(segment) = self.route.get(index).copied() else {
            self.enter(Phase::Mine);
            return ControlState::default();
        };
        let tile = rig.tile_position(TILE_SIZE);
        let last = index + 1 == self.route.len();
        let grounded = DrillContact::of(world.terrain(), rig).grounded;
        if last {
            // Fall to the bottom (or wherever the shaft's floor now is).
            if grounded && tile.y >= segment.top {
                self.enter(Phase::Mine);
                return ControlState::default();
            }
            return fall(rig, segment.col);
        }
        if tile.y < segment.bottom {
            return fall(rig, segment.col);
        }
        // At the knee: shift to the next segment's column on this row.
        let next = self.route[index + 1];
        if tile.x == next.col && (rig.x - centre(next.col)).abs() < TILE_SIZE * 0.2 {
            self.enter(Phase::Descend(index + 1));
        }
        hover_to(rig, centre(next.col), row_centre(segment.bottom))
    }

    fn mine(
        &mut self,
        world: &WorldState,
        rig: &Player,
        actions: &mut Vec<PlayerCommand>,
    ) -> ControlState {
        let terrain = world.terrain();
        let tile = rig.tile_position(TILE_SIZE);
        let Some(segment) = self.route.last().copied() else {
            self.enter(Phase::Ascend(0));
            return ControlState::default();
        };
        let contact = DrillContact::of(terrain, rig);
        if !contact.grounded {
            // Falling (into a cave, or after a blast): keep over the column.
            return fall(rig, segment.col);
        }
        if let Some(last) = self.route.last_mut() {
            last.bottom = last.bottom.max(tile.y);
        }
        if self.should_go_home(world, rig, tile) {
            return self.start_ascent(rig);
        }
        if tile.x != segment.col {
            // Knocked off the column: walk back.
            return ControlState::new(steer(centre(segment.col) - rig.x), false, false);
        }
        if self.planned_row != Some(tile.y) {
            self.planned_row = Some(tile.y);
            self.drill_tile = None;
            if self.strategy == Strategy::Charges
                && let Some(phase) = self.maybe_charge(world, rig, tile, actions)
            {
                self.enter(phase);
                return ControlState::default();
            }
            self.spur_queue = self.plan_spurs(world, rig, tile);
        }
        if let Some(target) = self.spur_queue.pop() {
            self.stats.spurs += 1;
            self.enter(Phase::Spur {
                target,
                row: tile.y,
                back: false,
            });
            return ControlState::new(steer(centre(target) - rig.x), false, false);
        }
        let below = TilePosition {
            x: tile.x,
            y: tile.y + 1,
        };
        let Some(below_tile) = terrain.tile(below) else {
            self.shaft_done = true;
            return self.start_ascent(rig);
        };
        if terrain.is_indestructible_at(below) {
            self.shaft_done = true;
            return self.start_ascent(rig);
        }
        let band = terrain.band_at(below);
        if heat_excess(band, rig.radiator_level, rig.hazard_gear.heat_shield) > 0 {
            self.wants_cooling = true;
            self.shaft_done = true;
            return self.start_ascent(rig);
        }
        let too_hard = !is_hazard(below_tile.kind) && !diggable(world, rig, below);
        if too_hard {
            // Too hard for this drill: upgrade it next time; meanwhile try around it.
            self.wants_drill = true;
        }
        if is_hazard(below_tile.kind) || too_hard {
            if let Some(side) = Self::sidestep_side(world, rig, tile) {
                if let Some(last) = self.route.last_mut() {
                    last.bottom = tile.y;
                }
                self.route.push(Segment {
                    col: side,
                    top: tile.y,
                    bottom: tile.y,
                });
                self.enter(Phase::Sidestep {
                    target: side,
                    row: tile.y,
                });
                return ControlState::new(steer(centre(side) - rig.x), false, false);
            }
            self.shaft_done = true;
            return self.start_ascent(rig);
        }
        // Drill down; watch for rock the drill cannot keep up with.
        match self.drill_tile {
            Some((at, since)) if at == below => {
                if self.clock - since > SLOW_TILE_SECONDS {
                    self.wants_drill = true;
                    self.shaft_done = true;
                    return self.start_ascent(rig);
                }
            }
            _ => self.drill_tile = Some((below, self.clock)),
        }
        let horizontal = steer(centre(segment.col) - rig.x) * 0.3;
        ControlState::new(horizontal, false, true)
    }

    fn should_go_home(&self, world: &WorldState, rig: &Player, tile: TilePosition) -> bool {
        if !rig.has_cargo_space() {
            return true;
        }
        if rig.hull < rig.max_hull() * HULL_RETURN_FRACTION {
            return true;
        }
        if self.clock - self.trip_started > MAX_TRIP_SECONDS {
            return true;
        }
        let rows = (tile.y - world.terrain().surface_row(tile.x)).max(0) as f32
            + self.route.len() as f32 * 2.0;
        rig.fuel < rows * self.fuel_per_row * 1.35 + 12.0
    }

    fn start_ascent(&mut self, rig: &Player) -> ControlState {
        let tile = rig.tile_position(TILE_SIZE);
        self.climb_from = Some((rig.fuel, tile.y));
        let last = self.route.len().saturating_sub(1);
        self.enter(Phase::Ascend(last));
        ControlState::new(0.0, true, false)
    }

    /// Ore tunnels worth digging on `tile`'s row: for each side, the farthest ore tile within
    /// reach along a run of safe, floored tiles (the pop order digs the nearer side first).
    fn plan_spurs(&self, world: &WorldState, rig: &Player, tile: TilePosition) -> Vec<i32> {
        let reach = self.strategy.spur_reach();
        let terrain = world.terrain();
        let mut targets: Vec<(i32, i32)> = Vec::new();
        for dir in [-1, 1] {
            let mut farthest = None;
            for step in 1..=reach {
                let at = TilePosition {
                    x: tile.x + dir * step,
                    y: tile.y,
                };
                let floor = TilePosition {
                    x: at.x,
                    y: at.y + 1,
                };
                let Some(here) = terrain.tile(at) else { break };
                let Some(under) = terrain.tile(floor) else {
                    break;
                };
                let safe = diggable(world, rig, at)
                    && !is_hazard(here.kind)
                    && under.kind != TileKind::Air
                    && !is_hazard(under.kind)
                    && !terrain.water_at(at);
                if !safe {
                    break;
                }
                if matches!(here.kind, TileKind::Ore(_) | TileKind::Artifact(_)) {
                    farthest = Some(step);
                }
            }
            if let Some(step) = farthest {
                targets.push((step, tile.x + dir * step));
            }
        }
        // Farther tunnels first in the queue so the nearer one pops first.
        targets.sort_by_key(|(step, _)| -step);
        targets.into_iter().map(|(_, target)| target).collect()
    }

    fn spur(
        &mut self,
        world: &WorldState,
        rig: &Player,
        target: i32,
        row: i32,
        back: bool,
    ) -> ControlState {
        let tile = rig.tile_position(TILE_SIZE);
        let col = self.route.last().map_or(target, |segment| segment.col);
        if tile.y != row {
            // Fell or was knocked out of the tunnel: give up on it, go home from here if the
            // shaft is not right above.
            if tile.x == col {
                self.enter(Phase::Mine);
                return ControlState::default();
            }
            return hover_to(rig, centre(col), row_centre(row));
        }
        if !rig.has_cargo_space() && !back {
            self.enter(Phase::Spur {
                target,
                row,
                back: true,
            });
        }
        if back {
            if tile.x == col && (rig.x - centre(col)).abs() < TILE_SIZE * 0.15 {
                self.enter(Phase::Mine);
                return ControlState::default();
            }
            return ControlState::new(steer(centre(col) - rig.x), false, false);
        }
        let reached = tile.x == target
            || world
                .terrain()
                .tile(TilePosition { x: target, y: row })
                .is_none_or(|here| here.kind == TileKind::Air)
                && (rig.x - centre(target)).abs() < TILE_SIZE * 0.6;
        if reached {
            self.enter(Phase::Spur {
                target,
                row,
                back: true,
            });
        }
        ControlState::new(steer(centre(target) - rig.x).signum(), false, false)
    }

    fn sidestep(&mut self, rig: &Player, target: i32, row: i32) -> ControlState {
        let tile = rig.tile_position(TILE_SIZE);
        if tile.x == target && (rig.x - centre(target)).abs() < TILE_SIZE * 0.15 {
            self.planned_row = None;
            self.enter(Phase::Mine);
            return ControlState::default();
        }
        if tile.y != row {
            self.enter(Phase::Mine);
            return ControlState::default();
        }
        ControlState::new(steer(centre(target) - rig.x), false, false)
    }

    /// The column beside `tile` the shaft can move to around a hazard below.
    fn sidestep_side(world: &WorldState, rig: &Player, tile: TilePosition) -> Option<i32> {
        let terrain = world.terrain();
        [1, -1].into_iter().map(|dir| tile.x + dir).find(|&x| {
            let beside = TilePosition { x, y: tile.y };
            let under = TilePosition { x, y: tile.y + 1 };
            diggable(world, rig, beside)
                && terrain
                    .tile(beside)
                    .is_some_and(|here| !is_hazard(here.kind))
                && terrain.tile(under).is_some_and(|here| {
                    here.kind != TileKind::Air
                        && !is_hazard(here.kind)
                        && diggable(world, rig, under)
                })
                && surface_building_at_tile(x).is_none_or(|_| tile.y > 8)
        })
    }

    /// Arms a mining charge here if the row is rich enough and the shaft above is clear for
    /// the retreat.
    fn maybe_charge(
        &mut self,
        world: &WorldState,
        rig: &Player,
        tile: TilePosition,
        actions: &mut Vec<PlayerCommand>,
    ) -> Option<Phase> {
        if rig.explosive_count(ExplosiveKind::Mining) == 0 {
            return None;
        }
        let terrain = world.terrain();
        let surface = terrain.surface_row(tile.x);
        if tile.y - surface < CHARGE_RETREAT_ROWS + 4 {
            return None;
        }
        let clear_above = (1..=CHARGE_RETREAT_ROWS).all(|dy| {
            !terrain.is_solid_at(TilePosition {
                x: tile.x,
                y: tile.y - dy,
            })
        });
        if !clear_above {
            return None;
        }
        let mut ore = 0;
        for dy in -4..=4 {
            for dx in -4..=4_i32 {
                if dx * dx + dy * dy > 16 {
                    continue;
                }
                let at = TilePosition {
                    x: tile.x + dx,
                    y: tile.y + dy,
                };
                if terrain
                    .tile(at)
                    .is_some_and(|here| matches!(here.kind, TileKind::Ore(_)))
                {
                    ore += 1;
                }
            }
        }
        if ore < CHARGE_MIN_ORE {
            return None;
        }
        actions.push(PlayerCommand::DeployExplosive {
            kind: ExplosiveKind::Mining,
            aim: ThrowAim::DOWN,
        });
        self.stats.charges_used += 1;
        let fuse = ExplosiveKind::Mining.fuse_millis() as f32 / 1000.0;
        Some(Phase::Blast {
            row: tile.y - CHARGE_RETREAT_ROWS,
            until: self.clock + fuse + 0.8,
        })
    }

    fn blast(&mut self, world: &WorldState, rig: &Player, row: i32, until: f32) -> ControlState {
        let col = self.route.last().map_or(0, |segment| segment.col);
        if self.clock >= until {
            // Back down into the crater; the magnet hoovers the loot on the way.
            self.planned_row = None;
            self.enter(Phase::Mine);
            return fall(rig, col);
        }
        let _ = world;
        hover_to(rig, centre(col), row_centre(row))
    }

    fn ascend(&mut self, world: &WorldState, rig: &Player, index: usize) -> ControlState {
        let tile = rig.tile_position(TILE_SIZE);
        let Some(segment) = self.route.get(index).copied() else {
            return self.arrive_in_town(world, rig);
        };
        if index == 0 {
            let surface = Self::surface_y(world, segment.col);
            if rig.y < surface - TILE_SIZE * 0.9 {
                return self.arrive_in_town(world, rig);
            }
            return climb(rig, segment.col);
        }
        if tile.y > segment.top {
            return climb(rig, segment.col);
        }
        let previous = self.route[index - 1];
        if tile.x == previous.col && (rig.x - centre(previous.col)).abs() < TILE_SIZE * 0.2 {
            self.enter(Phase::Ascend(index - 1));
        }
        hover_to(rig, centre(previous.col), row_centre(segment.top))
    }

    fn arrive_in_town(&mut self, world: &WorldState, rig: &Player) -> ControlState {
        if let Some((fuel, row)) = self.climb_from.take() {
            let surface = world.terrain().surface_row(rig.tile_position(TILE_SIZE).x);
            let rows = (row - surface).max(1) as f32;
            if rows >= 8.0 {
                let per_row = ((fuel - rig.fuel) / rows).max(0.0);
                self.fuel_per_row = self.fuel_per_row.max(per_row);
            }
        }
        self.stats.trips += 1;
        self.stops = Self::town_stops();
        self.stop_acted = false;
        self.enter(Phase::Town(0));
        ControlState::new(0.0, true, false)
    }

    fn town(
        &mut self,
        world: &WorldState,
        rig: &Player,
        index: usize,
        actions: &mut Vec<PlayerCommand>,
    ) -> ControlState {
        let Some(stop) = self.stops.get(index).copied() else {
            self.enter(Phase::ToShaft);
            return ControlState::default();
        };
        if !self.stop_needed(world, rig, stop) {
            self.stop_acted = false;
            self.enter(Phase::Town(index + 1));
            return ControlState::default();
        }
        let target_x = if stop == Stop::Outpost {
            let Some(col) = self.workshop.map(|(_, col)| col).or(self.outpost_site) else {
                self.enter(Phase::Town(index + 1));
                return ControlState::default();
            };
            centre(col)
        } else {
            let Some(building) = SURFACE_BUILDINGS.iter().find(|b| b.zone == stop.zone()) else {
                self.enter(Phase::Town(index + 1));
                return ControlState::default();
            };
            (building.tile_x as f32 + building.tile_width as f32 / 2.0) * TILE_SIZE
        };
        let dx = target_x - rig.x;
        let grounded = DrillContact::of(world.terrain(), rig).grounded;
        let at_door = if stop == Stop::Outpost {
            dx.abs() < TILE_SIZE * 0.4 && grounded && rig.velocity_x.abs() < TILE_SIZE
        } else {
            world.player_surface_zone(self.player) == Some(stop.zone())
                && dx.abs() < TILE_SIZE * 2.5
                && grounded
        };
        if !at_door {
            if self.clock - self.phase_started > 60.0 {
                // Something is in the way: skip the errand.
                self.enter(Phase::Town(index + 1));
                return ControlState::default();
            }
            return travel(world, rig, target_x);
        }
        if !self.stop_acted {
            self.stop_acted = true;
            self.act_at(world, rig, stop, actions);
            return ControlState::default();
        }
        if self.clock - self.phase_started > 0.25 {
            self.stop_acted = false;
            self.enter(Phase::Town(index + 1));
        }
        ControlState::default()
    }

    /// The errands of a town visit, in order: sell first (the money pays for the rest); a
    /// low tank is filled before the trip out to the outpost.
    fn town_stops() -> Vec<Stop> {
        vec![
            Stop::Depot,
            Stop::LowFuel,
            Stop::Outpost,
            Stop::Shop,
            Stop::Shack,
            Stop::Repair,
            Stop::Fuel,
        ]
    }

    /// Blocks in the hopper.
    fn hopper_blocks(rig: &Player) -> u32 {
        rig.block_hopper
            .iter()
            .filter(|(kind, _)| matches!(kind, crate::item::ItemKind::Block(_)))
            .map(|(_, count)| count)
            .sum()
    }

    /// Whether the charges strategy should visit its outpost: to build it once it can afford
    /// a claim and a workshop, then to press mining charges from the blocks it hauls.
    fn outpost_wanted(&mut self, world: &WorldState, rig: &Player) -> bool {
        if self.strategy != Strategy::Charges {
            return false;
        }
        let (blocks, credits) =
            crate::base::workshop_recipe(ExplosiveKind::Mining).unwrap_or((8, 30));
        if self.workshop.is_some() {
            return Self::hopper_blocks(rig) >= blocks
                && Self::spendable(rig) >= credits
                && rig.explosive_count(ExplosiveKind::Mining) < CHARGE_STOCK * 2;
        }
        if !self.saving_for_outpost(rig) || Self::spendable(rig) < Self::outpost_price() {
            return false;
        }
        if self.outpost_site.is_none() {
            let (town_west, _) = crate::surface::town_span();
            self.outpost_site = Self::find_outpost_site(
                world,
                town_west - crate::base::CLAIM_RADIUS - crate::world::TOWN_NO_BUILD_MARGIN - 5,
            );
        }
        self.outpost_site.is_some()
    }

    /// Credits the outpost takes (claim, workshop and blocks it may have to buy).
    const fn outpost_price() -> u32 {
        crate::base::ModuleKind::Beacon.credits()
            + crate::base::ModuleKind::Workshop.credits()
            + 120
    }

    /// The charges strategy puts the outpost before further upgrades once its drill reaches
    /// the clay beds.
    fn saving_for_outpost(&self, rig: &Player) -> bool {
        self.strategy == Strategy::Charges && self.workshop.is_none() && rig.drill_strength >= 2
    }

    /// A flat stretch of surface at or west of column `start` for a workshop and a claim beacon
    /// side by side.
    fn find_outpost_site(world: &WorldState, start: i32) -> Option<i32> {
        let terrain = world.terrain();
        (0..160).map(|step| start - step).find(|&x| {
            let ground = terrain.surface_row(x);
            (x..x + 4).all(|col| {
                terrain.surface_row(col) == ground
                    && terrain.is_solid_at(TilePosition { x: col, y: ground })
                    && !terrain.is_solid_at(TilePosition {
                        x: col,
                        y: ground - 1,
                    })
                    && !terrain.is_solid_at(TilePosition {
                        x: col,
                        y: ground - 2,
                    })
            })
        })
    }

    fn act_at_outpost(
        &mut self,
        world: &WorldState,
        rig: &Player,
        actions: &mut Vec<PlayerCommand>,
    ) {
        use crate::base::{BuildCommand, Buildable, Facing, ModuleKind, ModuleService};
        if let Some((structure, _)) = self.workshop {
            let (blocks, credits) =
                crate::base::workshop_recipe(ExplosiveKind::Mining).unwrap_or((8, 30));
            let room =
                (CHARGE_STOCK * 2).saturating_sub(rig.explosive_count(ExplosiveKind::Mining));
            let count = (Self::hopper_blocks(rig) / blocks.max(1))
                .min(Self::spendable(rig) / credits.max(1))
                .min(room);
            for _ in 0..count {
                actions.push(PlayerCommand::Build(BuildCommand::Use {
                    structure,
                    service: ModuleService::Craft(ExplosiveKind::Mining),
                }));
                self.stats.arms_spend += u64::from(credits);
            }
            return;
        }
        // Build the claim and the workshop where the rig stands.
        let Some(site) = self.outpost_site else {
            return;
        };
        let ground = world.terrain().surface_row(site);
        let place = |item, x| {
            PlayerCommand::Build(BuildCommand::Place {
                item: Buildable::Module(item),
                at: TilePosition { x, y: ground - 1 },
                facing: Facing::Right,
            })
        };
        let beacon = place(ModuleKind::Beacon, site + 2);
        if let PlayerCommand::Build(command) = &beacon
            && let Err(refusal) = world.check_build(self.player, command)
        {
            if std::env::var_os("BALANCE_TRACE").is_some() {
                eprintln!("outpost at {site} refused: {refusal:?}");
            }
            // Not buildable here after all: look further west next time.
            self.outpost_site = Self::find_outpost_site(world, site - 6);
            return;
        }
        actions.push(beacon);
        actions.push(place(ModuleKind::Workshop, site));
        self.stats.arms_spend +=
            u64::from(ModuleKind::Beacon.credits() + ModuleKind::Workshop.credits());
        let _ = rig;
    }

    /// Picks up the workshop the outpost visit built.
    fn find_workshop(&mut self, world: &WorldState) {
        if self.workshop.is_some() || self.strategy != Strategy::Charges {
            return;
        }
        self.workshop = world
            .bases()
            .modules()
            .find(|module| {
                module.owner == self.player && module.kind == crate::base::ModuleKind::Workshop
            })
            .map(|module| (module.id, module.origin.x));
    }

    fn reserve(rig: &Player) -> u32 {
        let fuel = (rig.fuel_capacity - rig.fuel).max(0.0).ceil() as u32;
        let repair = ((rig.max_hull() - rig.hull).max(0.0) * 2.0).ceil() as u32;
        fuel + repair + CREDIT_FLOOR
    }

    fn spendable(rig: &Player) -> u32 {
        rig.credits.saturating_sub(Self::reserve(rig))
    }

    fn stop_needed(&mut self, world: &WorldState, rig: &Player, stop: Stop) -> bool {
        match stop {
            Stop::Outpost => self.outpost_wanted(world, rig),
            Stop::Depot => rig.cargo_used() > 0 || rig.block_hopper.iter().next().is_some(),
            Stop::Shop => self.next_upgrade(world, rig).is_some(),
            Stop::Shack => self.shack_wish(world, rig).is_some(),
            Stop::Repair => rig.hull < rig.max_hull() * 0.75 && rig.credits > 0,
            Stop::LowFuel => rig.fuel < rig.fuel_capacity * 0.4 && rig.credits > 0,
            Stop::Fuel => {
                (rig.fuel < rig.fuel_capacity * 0.7 || rig.fuel < self.shaft_fuel_need(world))
                    && rig.credits > 0
            }
        }
    }

    /// The upgrade the bot would buy next: the affordable line with the lowest weighted price.
    fn next_upgrade(&self, world: &WorldState, rig: &Player) -> Option<usize> {
        let mut budget = Self::spendable(rig);
        if self.saving_for_outpost(rig) {
            budget = budget.saturating_sub(Self::outpost_price());
        }
        let deepest = world.progression().deepest_tile_reached;
        upgrade_offers(rig)
            .iter()
            .enumerate()
            .filter(|(_, offer)| {
                !upgrade_maxed(offer)
                    && offer.cost <= budget
                    && upgrade_locked_by(offer, world.terrain().settings(), deepest).is_none()
            })
            .filter_map(|(index, offer)| {
                self.weight(offer.kind, rig)
                    .map(|weight| (index, offer.cost as f32 * weight))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }

    /// How reluctant the bot is to buy `kind` (multiplies its price), or `None` if it never
    /// buys it.
    fn weight(&self, kind: UpgradeKind, rig: &Player) -> Option<f32> {
        Some(match kind {
            UpgradeKind::Drill => {
                if self.wants_drill {
                    0.5
                } else {
                    1.0
                }
            }
            UpgradeKind::CargoBay => 1.1,
            UpgradeKind::FuelTank => 1.3,
            UpgradeKind::Hull => 1.6,
            UpgradeKind::Engine => 1.8,
            UpgradeKind::Radiator => {
                if self.wants_cooling {
                    0.6
                } else {
                    4.0
                }
            }
            UpgradeKind::HeatShield if self.wants_cooling => 1.2,
            // Boulders shaken loose by mining are most of the early hazard damage.
            UpgradeKind::BoulderDeflector if self.stats.hazard_damage > 150.0 => 1.4,
            UpgradeKind::BoulderDeflector | UpgradeKind::PressureBracing
                if current_level(rig, UpgradeKind::Drill) >= 6 =>
            {
                5.0
            }
            _ => return None,
        })
    }

    /// What the bot wants from the Explosives Shack: a rivet gun once it has a few upgrades,
    /// then (charges strategy) mining charges.
    fn shack_wish(&self, world: &WorldState, rig: &Player) -> Option<SessionServiceRequest> {
        let shack_level = world.progression().town_development.explosives_shack_level;
        let budget = Self::spendable(rig);
        if rig.weapon_rack.owned.is_empty()
            && (rig.drill_strength >= 3 || self.stats.enemy_damage > 100.0)
            && budget >= WeaponKind::RivetGun.cost() + 300
        {
            return Some(SessionServiceRequest::BuyWeapon {
                weapon: WeaponKind::RivetGun,
            });
        }
        if self.strategy == Strategy::Charges
            && rig.explosive_count(ExplosiveKind::Mining) < CHARGE_STOCK
            && self.workshop.is_none()
            && let Some(offer) = explosive_offer(ExplosiveKind::Mining)
            && budget >= offer.cost
            && shack_level >= offer.shack_level
        {
            return Some(SessionServiceRequest::BuyExplosives {
                kind: ExplosiveKind::Mining,
            });
        }
        None
    }

    fn act_at(
        &mut self,
        world: &WorldState,
        rig: &Player,
        stop: Stop,
        actions: &mut Vec<PlayerCommand>,
    ) {
        match stop {
            Stop::Outpost => self.act_at_outpost(world, rig, actions),
            Stop::Depot => {
                let ore: u64 = rig
                    .cargo_bay
                    .iter()
                    .map(|(kind, count)| {
                        u64::from(
                            match kind {
                                crate::item::ItemKind::Mineral(mineral) => {
                                    world.mineral_market_value(mineral)
                                }
                                other => other.base_value(),
                            }
                            .saturating_mul(count),
                        )
                    })
                    .sum();
                let blocks: u64 = rig
                    .block_hopper
                    .iter()
                    .map(|(kind, count)| match kind {
                        crate::item::ItemKind::Block(block) => {
                            u64::from(crate::world::drops::block_sale_value(block))
                                * u64::from(count)
                        }
                        _ => 0,
                    })
                    .sum();
                self.stats.ore_income += ore;
                self.stats.block_income += blocks;
                actions.push(PlayerCommand::SellCargo);
                if self.workshop.is_none() {
                    actions.push(PlayerCommand::Service(SessionServiceRequest::SellBlocks));
                } else {
                    self.stats.block_income = self.stats.block_income.saturating_sub(blocks);
                }
                self.pending_sale = true;
            }
            Stop::Shop => {
                // Buy everything the policy wants and the budget allows, in this visit.
                let mut shadow = rig.clone();
                let mut bought = 0;
                while let Some(index) = self.next_upgrade(world, &shadow) {
                    let Ok(offer) = crate::economy::buy_upgrade(&mut shadow, index) else {
                        break;
                    };
                    actions.push(PlayerCommand::BuyUpgrade { index });
                    self.stats.upgrade_spend += u64::from(offer.cost);
                    self.stats
                        .purchases
                        .push((self.clock, offer.kind, offer.level + 1));
                    if offer.kind == UpgradeKind::Drill {
                        self.wants_drill = false;
                    }
                    if matches!(offer.kind, UpgradeKind::Radiator | UpgradeKind::HeatShield) {
                        self.wants_cooling = false;
                    }
                    bought += 1;
                    if bought >= 8 {
                        break;
                    }
                }
            }
            Stop::Shack => {
                if let Some(request) = self.shack_wish(world, rig) {
                    let cost = match request {
                        SessionServiceRequest::BuyWeapon { weapon } => weapon.cost(),
                        _ => explosive_offer(ExplosiveKind::Mining).map_or(0, |offer| offer.cost),
                    };
                    self.stats.arms_spend += u64::from(cost);
                    actions.push(PlayerCommand::Service(request));
                }
            }
            Stop::Repair => {
                let cost = ((rig.max_hull() - rig.hull).max(0.0) * 2.0).ceil() as u64;
                self.stats.upkeep_spend += cost.min(u64::from(rig.credits));
                actions.push(PlayerCommand::Repair);
            }
            Stop::Fuel | Stop::LowFuel => {
                let cost = (rig.fuel_capacity - rig.fuel).max(0.0).ceil() as u64;
                self.stats.upkeep_spend += cost.min(u64::from(rig.credits));
                actions.push(PlayerCommand::Refuel);
            }
        }
    }

    fn weapon_control(&mut self, world: &WorldState, rig: &Player) -> WeaponControl {
        let Some(mounted) = rig.mounted_weapon else {
            return WeaponControl::default();
        };
        if matches!(self.phase, Phase::Town(_) | Phase::ToShaft) {
            return WeaponControl::default();
        }
        let near = world.enemies_near(rig.x, rig.y, ENGAGE_TILES * TILE_SIZE);
        if near.is_empty() {
            return WeaponControl::default();
        }
        // Semi-automatic weapons need a fresh pull per shot.
        self.fire_toggle = !self.fire_toggle;
        WeaponControl {
            fire: mounted.kind.automatic() || self.fire_toggle,
            auto_aim: true,
            aim_x: 0,
            aim_y: 0,
        }
    }
}

/// Whether `rig`'s drill can dig the tile at `at` (air counts as diggable).
fn diggable(world: &WorldState, rig: &Player, at: TilePosition) -> bool {
    let terrain = world.terrain();
    match terrain.tile(at) {
        None => false,
        Some(tile) if tile.kind == TileKind::Air => true,
        // Building foundations grind the drill (and hurt).
        Some(tile) if tile.kind == TileKind::Foundation => false,
        Some(_) => {
            !terrain.is_indestructible_at(at)
                && terrain
                    .hardness_at(at)
                    .is_none_or(|hardness| hardness <= rig.drill_strength)
        }
    }
}

/// Whether drilling `kind` hurts (the bot steers around these).
const fn is_hazard(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Lava
            | TileKind::Gas
            | TileKind::ExplosivePocket
            | TileKind::PressurePocket
            | TileKind::MagmaVent
    )
}

/// Pixel x of column `col`'s centre.
fn centre(col: i32) -> f32 {
    (col as f32 + 0.5) * TILE_SIZE
}

/// Pixel y of a rig resting on the floor of row `row`.
fn row_centre(row: i32) -> f32 {
    (row as f32 + 0.5) * TILE_SIZE
}

/// Proportional horizontal steering toward `dx` pixels away.
fn steer(dx: f32) -> f32 {
    if dx.abs() < 1.5 {
        0.0
    } else {
        (dx / (TILE_SIZE * 0.6)).clamp(-1.0, 1.0)
    }
}

/// Driving over the surface toward `target_x`, hopping over open shafts (and out of one if
/// the rig slipped in).
fn travel(world: &WorldState, rig: &Player, target_x: f32) -> ControlState {
    let terrain = world.terrain();
    let dx = target_x - rig.x;
    let dir = if dx < 0.0 { -1 } else { 1 };
    let col = (rig.x / TILE_SIZE).floor() as i32;
    let ground = |x: i32| terrain.surface_row(x);
    let hole = (0..=2).any(|step| {
        let x = col + dir * step;
        !terrain.is_solid_at(TilePosition { x, y: ground(x) })
    });
    let sunk = rig.y > (ground(col) as f32 + 0.1) * TILE_SIZE;
    let climbing = rig.y > (ground(col + dir) as f32 - 0.4) * TILE_SIZE
        && terrain.is_solid_at(TilePosition {
            x: col + dir,
            y: (rig.y / TILE_SIZE).floor() as i32,
        });
    let thrust = hole || sunk || climbing;
    ControlState::new(steer(dx), thrust || rig.velocity_y > BRAKE_SPEED, false)
}

/// Falling down column `col`, braking when too fast.
fn fall(rig: &Player, col: i32) -> ControlState {
    ControlState::new(
        steer(centre(col) - rig.x) * 0.6,
        rig.velocity_y > BRAKE_SPEED,
        false,
    )
}

/// Climbing column `col`.
fn climb(rig: &Player, col: i32) -> ControlState {
    ControlState::new(steer(centre(col) - rig.x) * 0.6, true, false)
}

/// Holding height at `y` while moving to `x` (shaft knees, retreats).
fn hover_to(rig: &Player, x: f32, y: f32) -> ControlState {
    let dy = rig.y - y;
    let thrust = dy > 1.0 || (dy > -4.0 && rig.velocity_y > 20.0);
    let dx = x - rig.x;
    // Only move sideways once roughly level with the tunnel.
    let horizontal = if dy.abs() < 6.0 {
        steer(dx)
    } else {
        steer(dx) * 0.2
    };
    ControlState::new(horizontal, thrust, false)
}
