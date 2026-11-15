//! WP-DROPS economy recheck: drilling versus bombing income with drops, measured by playing
//! the real tick (a rig drilling straight down, or bomb-diving with plain bombs or mining
//! charges and hoovering the loot with its magnet) on several seeds and depths.
//!
//! `cargo test --release -p fatherload_simulation drop_economy_report -- --ignored --nocapture`
//! prints the table; `bombing_stays_balanced_against_drilling` pins the balance.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "credit tallies and tile coordinates are small and exact in f32"
)]

use super::WorldImport;
use crate::{
    ControlState, ExplosiveKind, PlayerCommand, PlayerId, SimulationParams, ThrowAim, TickInputs,
    WorldState,
    explosive::explosive_offer,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    world::BOMB_BUNDLES,
    worldgen::WorldSettings,
};

const PLAYER: PlayerId = PlayerId::new(0);
/// Seconds of play per run.
const RUN_SECONDS: u32 = 90;

/// How a run digs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Method {
    Drill,
    Bombs,
    MiningCharges,
}

impl Method {
    /// Credits one charge costs (plain bombs at the big bundle's price).
    fn charge_cost(self) -> f32 {
        match self {
            Self::Drill => 0.0,
            Self::Bombs => {
                let (count, cost) = BOMB_BUNDLES[1];
                cost as f32 / count as f32
            }
            Self::MiningCharges => explosive_offer(ExplosiveKind::Mining)
                .map_or(80.0, |offer| offer.cost as f32 / offer.count as f32),
        }
    }

    /// Seconds between charges (a fuse plus a moment to hoover).
    const fn period_seconds(self) -> u32 {
        match self {
            Self::Drill => 0,
            Self::Bombs => 3,
            Self::MiningCharges => 4,
        }
    }
}

/// One run's result.
#[derive(Clone, Copy, Debug, Default)]
struct Run {
    /// Base value of the ore and artifacts collected.
    gross: f32,
    /// Charges used.
    charges: u32,
    /// Tiles cleared.
    tiles: usize,
}

impl Run {
    fn net(self, method: Method) -> f32 {
        method
            .charge_cost()
            .mul_add(-(self.charges as f32), self.gross)
    }
}

/// A rig with drill tier `drill` resting at nominal row `nominal` in column 40 of seed
/// `seed`'s world, with room for everything and fuel and hull to spare.
fn rig_at(seed: u64, nominal: i32, drill: u8) -> WorldState {
    let mut import = WorldImport::new_game(PLAYER);
    let settings = WorldSettings {
        seed,
        ..*import.terrain.settings()
    };
    import.terrain = crate::terrain::Terrain::generate(&settings);
    let mut world = WorldState::from_import_with_params(&import, SimulationParams::new(60));
    let row = world.terrain().scaled_row(nominal);
    let column = 40;
    for y in row - 1..=row {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x: column, y }, TileKind::Air);
    }
    let below = TilePosition {
        x: column,
        y: row + 1,
    };
    if !world.terrain().is_solid_at(below) {
        let _ = world.terrain_mut().set_kind(below, TileKind::Stone);
    }
    let player = world.player_mut(PLAYER).expect("player");
    player.x = (column as f32 + 0.5) * TILE_SIZE;
    player.y = (row + 1) as f32 * TILE_SIZE - 11.0;
    player.drill_strength = drill;
    player.cargo_capacity = 100_000;
    player.block_hopper_capacity = 100_000;
    player.fuel_capacity = 100_000.0;
    player.fuel = 100_000.0;
    player.hull = 1_000_000.0;
    player.hazard_gear = crate::protection::HazardGear::MAXED;
    player.bombs = 1_000;
    player.add_explosives(ExplosiveKind::Mining, 1_000);
    world
}

fn collected_value(world: &WorldState) -> f32 {
    let player = world.player(PLAYER).expect("player");
    player
        .cargo_bay
        .iter()
        .map(|(kind, count)| kind.base_value() as f32 * count as f32)
        .sum()
}

fn play(seed: u64, nominal: i32, drill: u8, method: Method) -> Run {
    let mut world = rig_at(seed, nominal, drill);
    let hz = world.tick_rate_hz();
    let air_before = air_count(&world);
    let mut charges = 0;
    for step in 0..RUN_SECONDS * hz {
        let mut inputs = match method {
            Method::Drill => TickInputs::held(PLAYER, ControlState::new(0.0, false, true)),
            _ => TickInputs::held(PLAYER, ControlState::new(0.0, false, false)),
        };
        let period = method.period_seconds() * hz;
        if period > 0 && step % period == 0 {
            charges += 1;
            inputs = inputs.act(
                PLAYER,
                match method {
                    Method::Bombs => PlayerCommand::PlaceBomb,
                    _ => PlayerCommand::DeployExplosive {
                        kind: ExplosiveKind::Mining,
                        aim: ThrowAim::DOWN,
                    },
                },
            );
        }
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &inputs);
    }
    // Hoover what is still lying in reach of the last blast.
    for _ in 0..hz * 2 {
        let tick = world.simulation_tick();
        let _ = world.simulate_tick(tick, &TickInputs::default());
    }
    Run {
        gross: collected_value(&world),
        charges,
        tiles: air_count(&world).saturating_sub(air_before),
    }
}

fn air_count(world: &WorldState) -> usize {
    world
        .terrain()
        .generation_diffs()
        .iter()
        .filter(|change| change.tile.kind == TileKind::Air)
        .count()
}

/// Net credits per minute of `method` at `nominal` with drill tier `drill`, averaged over
/// `seeds`.
fn per_minute(seeds: &[u64], nominal: i32, drill: u8, method: Method) -> (f32, f32, f32) {
    let mut net = 0.0;
    let mut gross = 0.0;
    let mut tiles = 0.0;
    for seed in seeds {
        let run = play(*seed, nominal, drill, method);
        net += run.net(method);
        gross += run.gross;
        tiles += run.tiles as f32;
    }
    let scale = 60.0 / RUN_SECONDS as f32 / seeds.len() as f32;
    (net * scale, gross * scale, tiles * scale)
}

#[test]
#[ignore = "economy report: run with --ignored --nocapture (release recommended)"]
fn drop_economy_report() {
    let seeds = [11, 4_242, 90_001, 777_777];
    println!(
        "| Depth (nominal row) | Drill tier | Method | Tiles / min | Gross / min | Net / min |"
    );
    println!("|---|---|---|---|---|---|");
    for (nominal, drill) in [(14, 3), (60, 5), (60, 8), (120, 10), (120, 14)] {
        for method in [Method::Drill, Method::Bombs, Method::MiningCharges] {
            let (net, gross, tiles) = per_minute(&seeds, nominal, drill, method);
            println!("| {nominal} | {drill} | {method:?} | {tiles:.0} | {gross:.0} | {net:.0} |");
        }
    }
}

#[test]
fn bombing_stays_balanced_against_drilling() {
    let seeds = [4_242];
    // A mid-game rig (drill tier 8) in the middle strata: plain bombs and mining charges earn
    // their keep but never dwarf drilling (drops made blasts lucrative; ore shatter and charge
    // prices hold them in line). Near the surface bombs are for tunnels, not income.
    let (drill, ..) = per_minute(&seeds, 60, 8, Method::Drill);
    let (bombs, ..) = per_minute(&seeds, 60, 8, Method::Bombs);
    let (mining, ..) = per_minute(&seeds, 60, 8, Method::MiningCharges);
    assert!(drill > 0.0, "drilling pays: {drill}");
    assert!(
        bombs > 0.0 && bombs < drill * 1.5,
        "bombs pay but stay in line: {bombs} vs drilling {drill}"
    );
    assert!(
        mining > 0.0 && mining < drill * 2.0,
        "mining charges pay but stay in line: {mining} vs drilling {drill}"
    );
    let (shallow_drill, ..) = per_minute(&seeds, 14, 3, Method::Drill);
    let (shallow_bombs, ..) = per_minute(&seeds, 14, 3, Method::Bombs);
    assert!(
        shallow_bombs < shallow_drill,
        "shallow bombing does not out-earn the drill: {shallow_bombs} vs {shallow_drill}"
    );
}
