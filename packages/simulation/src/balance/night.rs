//! The night probe (WP-NIGHT).
//!
//! A stock, unarmed rig parked out on open ground at night, on
//! Normal with real spawns and the real AI, under three Moons, against the same rig parked
//! under a lamp. Measures the hull a night costs per minute, how many monsters rose, the first
//! hit and the worst biter, so the night stays tense but fair and lamps stay worth building.

use std::collections::BTreeMap;

use super::{
    BalanceProbe, ProbeContext, ReportSection, Table, economy::inputs_for, mean, median,
    run_parallel,
};
use crate::{
    ControlState, EnemyKind, GameplayEventKind, PlayerCommand, PlayerId, SimulationParams,
    WorldState,
    base::{BuildCommand, Buildable, Facing, Fixture, ModuleKind},
    clock::{MoonPhase, WorldClock},
    terrain::{TILE_SIZE, TilePosition},
    worldgen::{WorldSettings, biome::Biome},
};

/// Seconds of night per run.
pub const NIGHT_SECONDS: u32 = 150;
/// The rig of every run.
const PLAYER: PlayerId = PlayerId::new(0);

/// What one night out cost.
#[derive(Clone, Debug, Default)]
pub struct NightResult {
    /// Hull lost to night monsters.
    pub hull_lost: f64,
    /// Night monsters that rose (most alive at once).
    pub most_alive: usize,
    /// Seconds to the first hit, if any.
    pub first_hit: Option<u32>,
    /// Hull lost by attacker.
    pub by_kind: BTreeMap<EnemyKind, f64>,
}

/// A column east of town on open ground (not mountains), level for a few tiles, with no safe
/// place within 40 columns.
#[must_use]
pub fn exposed_column(world: &WorldState) -> Option<i32> {
    let ground = |x: i32| world.terrain().surface_row(x);
    (300..8_000).find(|column| {
        let row = ground(*column);
        crate::worldgen::biome::biome_at(world.world_settings(), *column) != Biome::Mountains
            && (column - 3..=column + 3).all(|x| ground(x) == row)
            && (column - 40..=column + 40).step_by(2).all(|x| {
                !world.night_safe_zone(TilePosition {
                    x,
                    y: ground(x) - 1,
                })
            })
    })
}

/// World time of 21:00 on the first day whose Moon is `moon`.
fn night_time(world: &WorldState, moon: MoonPhase) -> u64 {
    let length = world.day_length();
    let seed = world.terrain().seed();
    let day = (0..8)
        .find(|day| {
            WorldClock::from_world_millis(day * length.millis(), length, seed).moon_phase() == moon
        })
        .unwrap_or(0);
    day * length.millis() + 84 * length.slice_millis()
}

/// One night out on seed `seed` under `moon` for `seconds`; `lamp` parks the rig under a lamp
/// in a claim instead of out in the open.
#[must_use]
pub fn night_exposure(
    seed: u64,
    moon: MoonPhase,
    lamp: bool,
    seconds: u32,
    hz: u32,
) -> NightResult {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings::new(seed),
        SimulationParams::new(hz),
    );
    let frame = world.frame();
    let Some(column) = exposed_column(&world) else {
        return NightResult::default();
    };
    let row = world.terrain().surface_row(column);
    let park = |world: &mut WorldState, x: i32| {
        if let Some(rig) = world.player_mut(PLAYER) {
            #[allow(clippy::cast_precision_loss, reason = "small tile indices")]
            {
                rig.x = (frame.rel_col(x) + 0.5) * TILE_SIZE;
                rig.y = (row as f32 - 0.5) * TILE_SIZE;
            }
            rig.velocity_x = 0.0;
            rig.velocity_y = 0.0;
        }
    };
    park(&mut world, column);
    world.stream_terrain_near_players();
    if lamp {
        if let Some(rig) = world.player_mut(PLAYER) {
            rig.credits = 100_000;
        }
        for (item, at) in [
            (Buildable::Module(ModuleKind::Beacon), (column - 2, row - 1)),
            (Buildable::Fixture(Fixture::Light), (column, row - 2)),
        ] {
            let inputs = inputs_for(
                PLAYER,
                ControlState::new(0.0, false, false),
                vec![PlayerCommand::Build(BuildCommand::Place {
                    item,
                    at: TilePosition { x: at.0, y: at.1 },
                    facing: Facing::Right,
                })],
            );
            let tick = world.simulation_tick();
            let _ = world.simulate_tick(tick, &inputs);
        }
        park(&mut world, column);
    }
    let time = night_time(&world, moon);
    world.set_world_time_millis(time);
    let mut result = NightResult::default();
    let idle = inputs_for(PLAYER, ControlState::new(0.0, false, false), Vec::new());
    for tick_index in 0..seconds * hz {
        let before = world.player(PLAYER).map_or(0.0, |rig| rig.hull);
        let tick = world.simulation_tick();
        let output = world.simulate_tick(tick, &idle);
        let striker = output.events.iter().find_map(|event| match event.kind {
            GameplayEventKind::RigStruck { kind, .. } => Some(kind),
            _ => None,
        });
        let after = world.player(PLAYER).map_or(0.0, |rig| rig.hull);
        if let Some(kind) = striker {
            let lost = f64::from((before - after).max(0.0));
            result.hull_lost += lost;
            *result.by_kind.entry(kind).or_default() += lost;
            result.first_hit.get_or_insert(tick_index / hz);
        }
        // Keep the rig alive and in place so the whole night is measured.
        if let Some(rig) = world.player_mut(PLAYER) {
            rig.hull = rig.max_hull();
        }
        result.most_alive = result.most_alive.max(world.night_monsters().count());
    }
    result
}

/// The night probe (see the module docs).
pub struct NightProbe;

impl BalanceProbe for NightProbe {
    fn name(&self) -> &'static str {
        "night"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let seeds = context.config.seeds.clone();
        let mut section = ReportSection::new(
            "Night",
            format!(
                "A stock, unarmed rig parked on open ground east of town for {NIGHT_SECONDS} s \
                 of night on Normal (real spawns and AI), per seed and Moon; its hull is topped up \
                 every tick so the whole night is measured. \"Under a lamp\" parks it in a claim \
                 beside a lamp instead: night monsters must not touch it."
            ),
        );
        let cases = [
            ("First quarter", MoonPhase::FirstQuarter, false),
            ("Full moon", MoonPhase::Full, false),
            ("New moon", MoonPhase::New, false),
            (
                "Under a lamp (first quarter)",
                MoonPhase::FirstQuarter,
                true,
            ),
        ];
        let jobs: Vec<_> = cases
            .iter()
            .flat_map(|(_, moon, lamp)| {
                seeds
                    .iter()
                    .map(move |seed| move || night_exposure(*seed, *moon, *lamp, NIGHT_SECONDS, hz))
            })
            .collect();
        let results = run_parallel(context.config.threads, jobs);
        let mut table = Table::new(
            "Hull lost per minute at night:",
            &[
                "Night",
                "Loss / min",
                "Most monsters at once",
                "First hit (median s)",
                "Worst biter",
            ],
        );
        let minutes = f64::from(NIGHT_SECONDS) / 60.0;
        for (index, (name, _, lamp)) in cases.iter().enumerate() {
            let runs = &results[index * seeds.len()..(index + 1) * seeds.len()];
            let loss = mean(&runs.iter().map(|run| run.hull_lost).collect::<Vec<_>>()) / minutes;
            #[allow(clippy::cast_precision_loss, reason = "a handful of monsters")]
            let alive = mean(
                &runs
                    .iter()
                    .map(|run| run.most_alive as f64)
                    .collect::<Vec<_>>(),
            );
            let hits: Vec<f64> = runs
                .iter()
                .filter_map(|run| run.first_hit.map(f64::from))
                .collect();
            let mut by_kind: BTreeMap<EnemyKind, f64> = BTreeMap::new();
            for run in runs {
                for (kind, lost) in &run.by_kind {
                    *by_kind.entry(*kind).or_default() += lost;
                }
            }
            let worst = by_kind
                .iter()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or_else(|| "-".to_owned(), |(kind, _)| kind.name().to_owned());
            table.row(vec![
                (*name).to_owned(),
                format!("{loss:.1}"),
                format!("{alive:.1}"),
                if hits.is_empty() {
                    "-".to_owned()
                } else {
                    format!("{:.0}", median(&hits))
                },
                worst,
            ]);
            let key = if *lamp {
                "night.lamp_loss_per_min".to_owned()
            } else {
                format!(
                    "night.loss_per_min.{}",
                    name.to_lowercase().replace(' ', "_")
                )
            };
            section.metric(key, loss);
        }
        section.tables.push(table);
        section
    }
}
