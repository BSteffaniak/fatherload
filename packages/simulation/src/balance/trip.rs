//! The everyday trip: what a typical early run costs in fuel.
//!
//! A scripted rig plays the trip a new player makes over and over, through the real tick: it
//! stands on the surface beside town (the nearest column clear of buildings whose shaft its
//! drill can cut; the rig has the first drill upgrade, which clay needs, and is stock
//! otherwise), drills straight down
//! [`TRIP_DEPTH_ROWS`] rows, then mines for [`TRIP_MINING_SECONDS`]: tunnels
//! [`TUNNEL_TILES`] tiles out to one side and drives back, then the other side, drills two rows
//! deeper and repeats, with a short hop up the shaft now and then (a player hovering up to a
//! pocket of ore). Then it flies straight back up the shaft and lands on the surface. The probe
//! records the fuel burned (drilling and thrust), the trip's length and how much of each tank
//! it is.
//!
//! The fuel curve is tuned against it ([`crate::player::fuel_capacity_for_level`]): an
//! everyday trip burns well under half a stock tank, so fuel only bites on deep and long runs.

use super::{
    BalanceProbe, ProbeContext, ReportSection, Table,
    economy::{BOT_PLAYER, inputs_for, new_world},
};
use crate::{
    ControlState, EnemyDifficulty, WorldState,
    player::{Player, fuel_capacity_for_level},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    world::DrillContact,
};

/// Rows the trip drills down before mining.
pub const TRIP_DEPTH_ROWS: i32 = 50;
/// Seconds the trip mines at depth.
pub const TRIP_MINING_SECONDS: f32 = 120.0;
/// Tiles each side tunnel runs out from the shaft.
pub const TUNNEL_TILES: i32 = 8;
/// The column the trip's shaft is sunk in (a gap between the town's buildings).
const TRIP_COLUMN: i32 = 78;
/// Seconds between hops up the shaft while mining, and how long each hop thrusts.
const HOP_EVERY_SECONDS: f32 = 20.0;
const HOP_SECONDS: f32 = 1.0;
/// Columns beside the shaft the rig lands on.
const LANDING_OFFSET: i32 = 2;
/// No phase lasts longer than this (a stuck rig ends the trip where it is).
const PHASE_LIMIT_SECONDS: f32 = 150.0;
/// The trip rig's drill tier: the first upgrade (clay needs it).
pub const TRIP_DRILL: u8 = 2;
/// Fuel tank tiers the report shows the trip against.
pub const TRIP_TANKS: [u8; 4] = [1, 2, 3, 6];

/// One everyday trip's outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TripResult {
    /// Fuel tank tier flown.
    pub tank: u8,
    /// Its tank size.
    pub fuel_capacity: f32,
    /// Fuel burned from lift-off to landing.
    pub fuel_used: f32,
    /// Deepest row reached below the surface.
    pub depth_rows: i32,
    /// Tiles drilled out.
    pub tiles_mined: u32,
    /// Seconds from the first chip to landing.
    pub seconds: f32,
    /// The rig made it back onto the surface under its own power.
    pub landed: bool,
}

impl TripResult {
    /// The share of the tank the trip burned, in percent.
    #[must_use]
    pub fn tank_percent(&self) -> f32 {
        self.fuel_used / self.fuel_capacity.max(1.0) * 100.0
    }
}

/// What the scripted rig is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Drilling down the shaft.
    Dig,
    /// Tunnelling out (`dir` -1 left, 1 right).
    Out(i32),
    /// Driving back to the shaft from side `dir`.
    Back(i32),
    /// Drilling two rows deeper below the tunnels.
    Deeper(i32),
    /// Flying home up the shaft.
    Climb,
    /// Over the surface: settle onto the ground.
    Land,
}

fn centre(frame: crate::position::Frame, col: i32) -> f32 {
    (frame.rel_col(col) + 0.5) * TILE_SIZE
}

fn steer(dx: f32) -> f32 {
    if dx.abs() < 1.5 {
        0.0
    } else {
        (dx / (TILE_SIZE * 0.6)).clamp(-1.0, 1.0)
    }
}

/// Whether `rig` can drill out the tile at `at` safely (air counts; hazards and rock too hard
/// for its drill do not).
fn workable(world: &WorldState, rig: &Player, at: TilePosition) -> bool {
    let terrain = world.terrain();
    match terrain.tile(at) {
        None => false,
        Some(tile) if tile.kind == TileKind::Air => true,
        Some(tile) => {
            !matches!(
                tile.kind,
                TileKind::Foundation
                    | TileKind::Lava
                    | TileKind::Gas
                    | TileKind::ExplosivePocket
                    | TileKind::PressurePocket
                    | TileKind::MagmaVent
            ) && !terrain.is_indestructible_at(at)
                && terrain
                    .hardness_at(at)
                    .is_none_or(|hardness| hardness <= rig.drill_strength)
        }
    }
}

/// The column nearest [`TRIP_COLUMN`] (clear of the town's buildings) whose shaft the rig can
/// drill straight down past [`TRIP_DEPTH_ROWS`] (plus room for the deeper passes).
fn shaft_column(world: &mut WorldState) -> Option<i32> {
    let centre_tile = TilePosition {
        x: TRIP_COLUMN,
        y: world.terrain().surface_row(TRIP_COLUMN) + TRIP_DEPTH_ROWS / 2,
    };
    let _ = world.terrain_mut().stream_around(&[centre_tile]);
    let mut rig = world.player(BOT_PLAYER)?.clone();
    rig.drill_strength = TRIP_DRILL;
    (0..120)
        .flat_map(|step| [TRIP_COLUMN + step, TRIP_COLUMN - step - 1])
        .find(|col| {
            let surface = world.terrain().surface_row(*col);
            // Solid all the way down: no cave for the rig to drop into and lose its shaft.
            crate::surface::surface_building_at_tile(*col).is_none()
                && (surface..=surface + TRIP_DEPTH_ROWS + 12).all(|y| {
                    let at = TilePosition { x: *col, y };
                    world.terrain().is_solid_at(at) && workable(world, &rig, at)
                })
        })
}

/// Plays one everyday trip on `seed`'s world with a rig fitted with fuel tank `tank` and the
/// first drill upgrade ([`TRIP_DRILL`]), stock otherwise.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one scripted trip, phase by phase")]
pub fn everyday_trip(seed: u64, tank: u8, tick_rate_hz: u32) -> Option<TripResult> {
    let mut world: WorldState = new_world(seed, tick_rate_hz);
    let frame = world.frame();
    world.set_enemy_difficulty(EnemyDifficulty::Peaceful);
    let col = shaft_column(&mut world)?;
    let surface = world.terrain().surface_row(col);
    let capacity = fuel_capacity_for_level(tank);
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.x = centre(frame, col);
        rig.y = surface as f32 * TILE_SIZE - 11.0;
        rig.velocity_x = 0.0;
        rig.velocity_y = 0.0;
        rig.fuel_tank_level = tank;
        rig.fuel_capacity = capacity;
        rig.fuel = capacity;
        // The first drill upgrade (minutes into a new game): the clay beds start above
        // `TRIP_DEPTH_ROWS`, and a stock bit cannot cut clay.
        rig.drill_strength = TRIP_DRILL;
        // Stow no blocks: the trip measures fuel, not a full hopper turning it home early.
        rig.collect_blocks = false;
    }
    let hz = tick_rate_hz.max(1) as f32;
    let start_fuel = world.player(BOT_PLAYER)?.fuel;
    let mut step = Step::Dig;
    let mut clock = 0.0_f32;
    let mut step_started = 0.0_f32;
    let mut mining_started = None;
    let mut next_hop = HOP_EVERY_SECONDS;
    let mut hop_until = 0.0_f32;
    let mut bottom = surface;
    let mut depth = 0;
    let mut tiles_mined = 0_u32;
    let mut landed = false;
    // The highest row reached on the climb home, and when.
    let mut climb_progress = (i32::MAX, 0.0_f32);
    loop {
        let rig = world.player(BOT_PLAYER)?;
        let tile = rig.tile_in(world.frame());
        let grounded = DrillContact::of(world.terrain(), rig).grounded;
        let below = TilePosition {
            x: tile.x,
            y: tile.y + 1,
        };
        depth = depth.max(tile.y - surface);
        let mining_over = mining_started.is_some_and(|start| clock - start >= TRIP_MINING_SECONDS);
        let stale = clock - step_started > PHASE_LIMIT_SECONDS;
        let blocked_below = grounded && !workable(&world, rig, below);
        let next = match step {
            Step::Dig if tile.y - surface >= TRIP_DEPTH_ROWS || blocked_below || stale => {
                bottom = tile.y;
                mining_started = Some(clock);
                Some(Step::Out(-1))
            }
            Step::Out(dir)
                if tile.x == col + dir * TUNNEL_TILES
                    || stale
                    || !workable(
                        &world,
                        rig,
                        TilePosition {
                            x: tile.x + dir,
                            y: tile.y,
                        },
                    )
                    || !world.terrain().is_solid_at(TilePosition {
                        x: tile.x + dir,
                        y: tile.y + 1,
                    }) =>
            {
                // The tunnel's end, rock too hard or a hazard ahead, or a drop: drive back.
                Some(Step::Back(dir))
            }
            Step::Back(dir)
                if (tile.x == col && (rig.x - centre(frame, col)).abs() < 3.0) || stale =>
            {
                Some(if dir < 0 {
                    Step::Out(1)
                } else {
                    Step::Deeper(bottom + 2)
                })
            }
            Step::Deeper(row) if (tile.y >= row && grounded) || blocked_below || stale => {
                bottom = tile.y;
                Some(Step::Out(-1))
            }
            Step::Climb if tile.y < surface - 1 => Some(Step::Land),
            // Wedged under a ledge: the trip failed (landed stays false).
            Step::Climb if clock - climb_progress.1 > 5.0 => break,
            Step::Land if grounded && rig.velocity_y.abs() < 1.0 && tile.y < surface => {
                landed = true;
                break;
            }
            Step::Climb | Step::Land if stale => break,
            _ => None,
        };
        if let Some(next) = next {
            step = next;
            step_started = clock;
        }
        if step != Step::Climb || tile.y < climb_progress.0 {
            climb_progress = (tile.y, clock);
        }
        if mining_over && matches!(step, Step::Out(_) | Step::Back(_) | Step::Deeper(_)) {
            // Time's up: finish at the shaft, then fly home.
            if tile.x == col {
                step = Step::Climb;
                step_started = clock;
            } else if let Step::Out(dir) = step {
                step = Step::Back(dir);
                step_started = clock;
            }
        }
        if mining_started.is_some() && !mining_over && tile.x == col && clock >= next_hop {
            next_hop = clock + HOP_EVERY_SECONDS;
            hop_until = clock + HOP_SECONDS;
        }
        let hopping = clock < hop_until;
        if std::env::var_os("BALANCE_TRACE_TRIP").is_some()
            && ((clock * hz) as u32).is_multiple_of(hz as u32)
        {
            eprintln!(
                "{clock:6.1} {step:?} tile={tile:?} x={:.1} fuel={:.1} vy={:.1} grounded={grounded}",
                rig.x, rig.fuel, rig.velocity_y
            );
        }
        let to_shaft = steer(centre(frame, col) - rig.x);
        let controls = match step {
            Step::Dig | Step::Deeper(_) => ControlState::new(to_shaft * 0.6, hopping, !hopping),
            Step::Out(dir) => ControlState::new(dir as f32, hopping, false),
            Step::Back(_) => ControlState::new(to_shaft, hopping, false),
            Step::Climb => ControlState::new(to_shaft * 0.6, true, false),
            Step::Land => {
                // Hover over beside the shaft's mouth, then settle onto the ground there.
                let pad = centre(frame, col + LANDING_OFFSET);
                let over = (rig.x - pad).abs() < TILE_SIZE * 0.3;
                let hold =
                    rig.y > world.frame().rel_row(surface - 2) * TILE_SIZE || rig.velocity_y > 40.0;
                ControlState::new(
                    steer(pad - rig.x),
                    if over { rig.velocity_y > 120.0 } else { hold },
                    false,
                )
            }
        };
        let output = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, controls, Vec::new()),
        );
        tiles_mined += u32::try_from(
            output
                .terrain_ops
                .iter()
                .filter(|change| change.tile.kind == TileKind::Air)
                .count(),
        )
        .unwrap_or(0);
        clock += 1.0 / hz;
    }
    let rig = world.player(BOT_PLAYER)?;
    Some(TripResult {
        tank,
        fuel_capacity: capacity,
        fuel_used: (start_fuel - rig.fuel).max(0.0),
        depth_rows: depth,
        tiles_mined,
        seconds: clock,
        landed,
    })
}

/// The everyday trip section: fuel burned against each tank.
pub struct TripProbe;

impl BalanceProbe for TripProbe {
    fn name(&self) -> &'static str {
        "trip"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let seeds = context.config.seeds.clone();
        let mut section = ReportSection::new(
            "Everyday trip",
            format!(
                "A scripted rig with the first drill upgrade drills straight down \
                 {TRIP_DEPTH_ROWS} rows from the surface beside town, mines for {TRIP_MINING_SECONDS:.0} s ({TUNNEL_TILES}-tile \
                 tunnels to each side, two rows deeper, again; a one-second hop up the shaft every \
                 {HOP_EVERY_SECONDS:.0} s), then flies home and lands (real tick, Peaceful). Fuel \
                 is everything burned from the first chip to landing; medians over seeds."
            ),
        );
        let mut table = Table::new(
            "Fuel per everyday trip, by fuel tank:",
            &[
                "Tank (fuel)",
                "Fuel burned",
                "% of tank",
                "Tiles mined",
                "Depth (rows)",
                "Trip (min)",
                "Landed",
            ],
        );
        for tank in TRIP_TANKS {
            let jobs: Vec<_> = seeds
                .iter()
                .map(|seed| {
                    let seed = *seed;
                    move || everyday_trip(seed, tank, hz)
                })
                .collect();
            let results: Vec<TripResult> = super::run_parallel(context.config.threads, jobs)
                .into_iter()
                .flatten()
                .collect();
            if results.is_empty() {
                continue;
            }
            let collect =
                |f: &dyn Fn(&TripResult) -> f64| results.iter().map(f).collect::<Vec<_>>();
            let percent = super::median(&collect(&|r| f64::from(r.tank_percent())));
            section.metric(format!("trip.tank{tank}.tank_percent"), percent);
            table.row(vec![
                format!("Tier {tank} ({:.0})", fuel_capacity_for_level(tank)),
                format!(
                    "{:.0}",
                    super::median(&collect(&|r| f64::from(r.fuel_used)))
                ),
                format!("{percent:.0} %"),
                format!(
                    "{:.0}",
                    super::median(&collect(&|r| f64::from(r.tiles_mined)))
                ),
                format!(
                    "{:.0}",
                    super::median(&collect(&|r| f64::from(r.depth_rows)))
                ),
                format!(
                    "{:.1}",
                    super::median(&collect(&|r| f64::from(r.seconds) / 60.0))
                ),
                format!(
                    "{}/{}",
                    results.iter().filter(|r| r.landed).count(),
                    results.len()
                ),
            ]);
        }
        section.tables.push(table);
        section
    }
}
