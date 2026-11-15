//! Combat probes.
//!
//! - **Duels** ([`duel`]): a rig with a weapon loadout in a sealed arena at a band's depth,
//!   one enemy of a kind at that band's tier, the rig aiming straight at it, holding the
//!   trigger (fresh pulls for semi-automatic guns) and closing in when the gun is out of reach. Measures time to kill, hull lost and
//!   shots. The arena is walled with bedrock past a stone shell and its lair chunks are marked
//!   cleared, so the director adds nobody.
//! - **Exposure** ([`exposure`]): a band-appropriate unarmed rig drilling down from the top of
//!   a band in a real world for a while; enemy damage by kind and hazard damage per minute.

use std::collections::BTreeMap;

use super::{
    BalanceProbe, ProbeContext, ReportSection, Table,
    economy::{BOT_PLAYER, inputs_for},
    mean, median, run_parallel,
};
use crate::{
    ControlState, EnemyDifficulty, EnemyKind, GameplayEventKind, SimulationParams, WeaponControl,
    WorldState,
    enemy::{Locomotion, band_roster, lair_occupant, tier_scale},
    entity::{EntityId, EntityState},
    player::hull_capacity_for_level,
    protection::HazardGear,
    terrain::{ChunkPosition, TILE_SIZE, TileKind, TilePosition},
    weapon::{AmmoKind, MountedWeapon, WeaponKind, battery_capacity, quantize_aim},
    world::{DrillContact, WorldImport},
    worldgen::{Band, WorldSettings},
};

/// Longest duel before it counts as a stalemate.
pub const DUEL_SECONDS: u32 = 150;
/// Seconds of drilling per exposure run.
pub const EXPOSURE_SECONDS: u32 = 150;
/// Column of the duel arena (east of town).
const ARENA_X: i32 = 300;
/// Arena half-width and height (tiles).
const ARENA_HALF_WIDTH: i32 = 12;
const ARENA_HEIGHT: i32 = 9;

/// A weapon fit: the gun, its upgrade tier and the capacitor level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Loadout {
    pub name: &'static str,
    pub weapon: WeaponKind,
    pub mk: u8,
    pub battery: u8,
}

/// The loadouts the duel table compares, roughly in purchase order.
pub const LOADOUTS: [Loadout; 5] = [
    Loadout {
        name: "Rivet I",
        weapon: WeaponKind::RivetGun,
        mk: 0,
        battery: 0,
    },
    Loadout {
        name: "Scatter II",
        weapon: WeaponKind::ScatterBlaster,
        mk: 1,
        battery: 1,
    },
    Loadout {
        name: "Cutter II",
        weapon: WeaponKind::PlasmaCutter,
        mk: 1,
        battery: 2,
    },
    Loadout {
        name: "Rockets III",
        weapon: WeaponKind::RocketLauncher,
        mk: 2,
        battery: 2,
    },
    Loadout {
        name: "Railgun III",
        weapon: WeaponKind::Railgun,
        mk: 2,
        battery: 4,
    },
];

/// One duel's outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuelResult {
    /// Seconds to the kill (`None`: still alive after [`DUEL_SECONDS`]).
    pub seconds: Option<f32>,
    /// Enemy health left at the end (fraction).
    pub health_left: f32,
    /// The enemy's starting health (and shield).
    pub health: f32,
    /// Hull the rig lost.
    pub hull_lost: f32,
    /// Shots fired.
    pub shots: u32,
}

/// The first row of `band` in column `x`, scanning down from the surface.
#[must_use]
pub fn band_top(world: &WorldState, x: i32, band: Band) -> Option<i32> {
    let terrain = world.terrain();
    let start = terrain.surface_row(x);
    let bottom = start + terrain.height().max(1200);
    (start..bottom).find(|&y| terrain.band_at(TilePosition { x, y }).index() >= band.index())
}

/// A sealed arena world at `band`'s depth, the rig parked on the floor: `(world, floor row,
/// arena centre column)`. Columns whose rock will not carve (vault walls) are skipped.
fn arena(band: Band, tick_rate_hz: u32) -> Option<(WorldState, i32, i32)> {
    (0..8).find_map(|attempt| arena_at(ARENA_X + attempt * 90, band, tick_rate_hz))
}

fn arena_at(centre_x: i32, band: Band, tick_rate_hz: u32) -> Option<(WorldState, i32, i32)> {
    let import = WorldImport::new_game(BOT_PLAYER);
    let mut world =
        WorldState::from_import_with_params(&import, SimulationParams::new(tick_rate_hz));
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let top = band_top(&world, centre_x, band)?;
    let floor = top + ARENA_HEIGHT + 6;
    let shell = 3;
    let outer = 34;
    let inside = |x: i32, y: i32, margin: i32| {
        (centre_x - ARENA_HALF_WIDTH - margin..=centre_x + ARENA_HALF_WIDTH + margin).contains(&x)
            && (floor - ARENA_HEIGHT - margin..=floor - 1 + margin).contains(&y)
    };
    let kind_at = |x: i32, y: i32| {
        if inside(x, y, 0) {
            TileKind::Air
        } else if inside(x, y, shell) {
            TileKind::Stone
        } else {
            TileKind::Bedrock
        }
    };
    // One write per tile: bedrock cannot be overwritten once placed.
    for y in floor - ARENA_HEIGHT - outer..=floor + outer {
        for x in centre_x - outer..=centre_x + outer {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, kind_at(x, y));
        }
    }
    let carved = (floor - ARENA_HEIGHT - shell..=floor + shell).all(|y| {
        (centre_x - ARENA_HALF_WIDTH - shell..=centre_x + ARENA_HALF_WIDTH + shell).all(|x| {
            world
                .terrain()
                .tile(TilePosition { x, y })
                .is_some_and(|tile| tile.kind == kind_at(x, y))
        })
    });
    if !carved {
        return None;
    }
    // Reaching a new depth tier puts a deep reward tile beside the rig: let that happen now,
    // then clear the arena again.
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.x = (centre_x as f32 - 4.5) * TILE_SIZE;
        rig.y = floor as f32 * TILE_SIZE - 11.0;
    }
    for _ in 0..30 {
        let _ = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, ControlState::default(), Vec::new()),
        );
    }
    for y in floor - ARENA_HEIGHT..floor {
        for x in centre_x - ARENA_HALF_WIDTH..=centre_x + ARENA_HALF_WIDTH {
            let _ = world
                .terrain_mut()
                .set_kind(TilePosition { x, y }, TileKind::Air);
        }
    }
    let centre = ChunkPosition::of(TilePosition {
        x: centre_x,
        y: floor,
    });
    for dy in -3..=3 {
        for dx in -3..=3 {
            world.mark_lair_cleared(ChunkPosition {
                x: centre.x + dx,
                y: centre.y + dy,
            });
        }
    }
    let rig = world.player_mut(BOT_PLAYER)?;
    rig.x = (centre_x as f32 - 4.5) * TILE_SIZE;
    rig.y = floor as f32 * TILE_SIZE - 11.0;
    rig.velocity_x = 0.0;
    rig.velocity_y = 0.0;
    rig.fuel_capacity = 100_000.0;
    rig.fuel = 100_000.0;
    // Only the enemy may hurt it: heat and hazards are someone else's report.
    rig.hazard_gear = HazardGear::MAXED;
    rig.radiator_level = 15;
    Some((world, floor, centre_x))
}

/// Hull a measuring rig is topped up to every tick (losses are read against it).
const MEASURE_HULL: f32 = 1_000_000.0;

/// Tops `world`'s rig's hull up to [`MEASURE_HULL`]; returns the hull lost since the last top-up
/// (ignoring the drop when a deep reward or repair clamps the hull to its maximum).
fn top_up_hull(world: &mut WorldState) -> f32 {
    let Some(rig) = world.player_mut(BOT_PLAYER) else {
        return 0.0;
    };
    let lost = MEASURE_HULL - rig.hull;
    rig.hull = MEASURE_HULL;
    if (0.0..10_000.0).contains(&lost) {
        lost
    } else {
        0.0
    }
}

/// Where an enemy of `kind` starts in the arena with floor row `floor` and centre `centre_x`.
fn spawn_point(kind: EnemyKind, floor: i32, centre_x: i32) -> (f32, f32) {
    let stats = kind.stats();
    let x = (centre_x as f32 + 4.5) * TILE_SIZE;
    let floor_y = floor as f32 * TILE_SIZE;
    match stats.locomotion {
        Locomotion::Flying => (x, floor_y - 4.0 * TILE_SIZE),
        Locomotion::Burrowing => (x, floor_y + 1.5 * TILE_SIZE),
        Locomotion::Walking | Locomotion::Stationary => (x, floor_y - stats.radius - 1.0),
    }
}

/// Duels `loadout` against `kind` at `band`'s depth.
#[must_use]
pub fn duel(
    kind: EnemyKind,
    band: Band,
    loadout: Loadout,
    tick_rate_hz: u32,
) -> Option<DuelResult> {
    let (mut world, floor, centre_x) = arena(band, tick_rate_hz)?;
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.weapon_rack.owned.insert(loadout.weapon, loadout.mk);
        rig.weapon_rack.battery_level = loadout.battery;
        rig.mounted_weapon = Some(MountedWeapon {
            kind: loadout.weapon,
            mk: loadout.mk,
        });
        rig.weapon.energy = battery_capacity(loadout.battery);
        if loadout.weapon == WeaponKind::RocketLauncher {
            let _ = rig.stow(AmmoKind::Rocket.item(), 400);
        }
    }
    let (x, y) = spawn_point(kind, floor, centre_x);
    let id = world.spawn_enemy(kind, x, y);
    let health = enemy_health(&world, id).map_or(0.0, |(health, shield, _)| health + shield);
    let _ = top_up_hull(&mut world);
    let stats = loadout.weapon.stats(loadout.mk);
    let hz = world.tick_rate_hz();
    let mut shots = 0;
    let mut hull_lost = 0.0;
    let mut toggle = false;
    for tick in 0..DUEL_SECONDS * hz {
        let Some((_, _, (ex, ey))) = enemy_health(&world, id) else {
            return Some(DuelResult {
                seconds: Some(tick as f32 / hz as f32),
                health_left: 0.0,
                health,
                hull_lost,
                shots,
            });
        };
        let rig = world.player(BOT_PLAYER)?;
        let (mx, my) = (rig.x, rig.y - crate::weapon::MUZZLE_RISE);
        let (dx, mut dy) = (ex - mx, ey - my);
        if stats.gravity > 0.0 && stats.speed > 0.0 {
            // Lob: aim above by the drop over the flight time.
            let t = dx.hypot(dy) / stats.speed;
            dy -= 0.5 * stats.gravity * t * t;
        }
        toggle = !toggle;
        let (aim_x, aim_y) = quantize_aim(dx, dy).unwrap_or((127, 0));
        let weapon = WeaponControl {
            fire: loadout.weapon.automatic() || toggle,
            auto_aim: false,
            aim_x,
            aim_y,
        };
        // Close in when out of reach (short-range guns), like a player would.
        let reach = stats.range * 0.75;
        let horizontal = if dx.hypot(dy) > reach && dx.abs() > TILE_SIZE * 1.5 {
            dx.signum()
        } else {
            0.0
        };
        let controls = ControlState::new(horizontal, false, false).with_weapon(weapon);
        let output = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, controls, Vec::new()),
        );
        hull_lost += top_up_hull(&mut world);
        shots += output
            .events
            .iter()
            .filter(|event| matches!(event.kind, GameplayEventKind::WeaponFired { .. }))
            .count() as u32;
    }
    let left = enemy_health(&world, id).map_or(0.0, |(h, s, _)| h + s);
    Some(DuelResult {
        seconds: None,
        health_left: if health > 0.0 { left / health } else { 0.0 },
        health,
        hull_lost,
        shots,
    })
}

/// `(health, shield, position)` of enemy `id`, if it lives.
fn enemy_health(world: &WorldState, id: EntityId) -> Option<(f32, f32, (f32, f32))> {
    let entity = world.entities().get(id)?;
    match &entity.state {
        EntityState::Enemy(state) if state.health > 0.0 => {
            Some((state.health, state.shield, (entity.body.x, entity.body.y)))
        }
        _ => None,
    }
}

/// The bands `kind` lives in (roaming or as a lair occupant), shallowest first.
#[must_use]
pub fn habitat(kind: EnemyKind) -> Vec<Band> {
    Band::ALL
        .into_iter()
        .filter(|band| {
            band_roster(*band).iter().any(|(roamer, _)| *roamer == kind)
                || (0..4).any(|roll| lair_occupant(*band, roll) == Some(kind))
        })
        .collect()
}

fn format_duel(result: Option<DuelResult>) -> String {
    match result {
        None => "n/a".to_owned(),
        Some(DuelResult {
            seconds: Some(seconds),
            ..
        }) => format!("{seconds:.1} s"),
        Some(result) => format!(
            ">{DUEL_SECONDS} s ({:.0} % left)",
            result.health_left * 100.0
        ),
    }
}

/// Duels and the elite table.
pub struct CombatProbe;

impl BalanceProbe for CombatProbe {
    fn name(&self) -> &'static str {
        "combat"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let mut section = ReportSection::new(
            "Combat: time to kill",
            format!(
                "Duels in a sealed arena at the depth of each enemy's shallowest and deepest \
                 band (its tier there scales health and damage by `tier_scale`): one enemy, a \
                 rig with the loadout aiming straight at it and holding the trigger, Normal \
                 difficulty, one rig (no crew scaling). Stalemates stop after {DUEL_SECONDS} s. \
                 Hull lost is per duel."
            ),
        );
        let mut cases: Vec<(EnemyKind, Band)> = Vec::new();
        for kind in EnemyKind::ALL {
            let bands = habitat(kind);
            if let Some(first) = bands.first() {
                cases.push((kind, *first));
            }
            if let Some(last) = bands.last()
                && bands.len() > 1
            {
                cases.push((kind, *last));
            }
        }
        let jobs: Vec<_> = cases
            .iter()
            .flat_map(|(kind, band)| {
                LOADOUTS
                    .iter()
                    .map(move |loadout| move || duel(*kind, *band, *loadout, hz))
            })
            .collect();
        let results = run_parallel(context.config.threads, jobs);
        let mut headers = vec!["Enemy @ band (health)".to_owned()];
        headers.extend(LOADOUTS.iter().map(|l| l.name.to_owned()));
        headers.push("Hull lost (Rivet I)".to_owned());
        let mut table = Table {
            caption: "Time to kill per loadout:".to_owned(),
            headers,
            rows: Vec::new(),
        };
        for (index, (kind, band)) in cases.iter().enumerate() {
            let row_results = &results[index * LOADOUTS.len()..(index + 1) * LOADOUTS.len()];
            let health = row_results
                .iter()
                .flatten()
                .map(|result| result.health)
                .next()
                .unwrap_or(0.0);
            let mut row = vec![format!("{} @ {} ({health:.0})", kind.name(), band.name())];
            row.extend(row_results.iter().map(|result| format_duel(*result)));
            row.push(row_results[0].map_or_else(
                || "n/a".to_owned(),
                |result| format!("{:.0}", result.hull_lost),
            ));
            for (loadout, result) in LOADOUTS.iter().zip(row_results) {
                if let Some(DuelResult {
                    seconds: Some(seconds),
                    ..
                }) = result
                {
                    section.metric(
                        format!(
                            "combat.ttk.{}.{}.{}",
                            kind.name().to_lowercase().replace(' ', "_"),
                            band.index(),
                            loadout.name.to_lowercase().replace(' ', "_")
                        ),
                        f64::from(*seconds),
                    );
                }
            }
            table.row(row);
        }
        section.tables.push(table);
        let mut scale = Table::new(
            "Depth tier scaling (`tier_scale`: enemy health and damage multiplier by band):",
            &["Band", "Tier", "Scale"],
        );
        for band in Band::ALL {
            let tier = u8::try_from(band.index()).unwrap_or(u8::MAX);
            scale.row(vec![
                band.name().to_owned(),
                tier.to_string(),
                format!("{:.2}", tier_scale(tier)),
            ]);
        }
        section.tables.push(scale);
        section
    }
}

/// One exposure run's tallies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExposureResult {
    /// Hull lost to each enemy kind.
    pub by_kind: BTreeMap<EnemyKind, f64>,
    /// Hull lost to everything else.
    pub hazard: f64,
    /// Seconds to the first enemy hit.
    pub first_hit: Option<f32>,
    /// Rows drilled.
    pub rows: i32,
}

impl ExposureResult {
    /// Hull lost to enemies.
    #[must_use]
    pub fn enemy(&self) -> f64 {
        self.by_kind.values().sum()
    }
}

/// The gear a player typically has on reaching `band` (drill and hull tiers, cooling enough
/// for the band's heat).
#[must_use]
pub fn band_gear(band: Band) -> (u8, u8) {
    let index = u8::try_from(band.index()).unwrap_or(0);
    // A tier past the band's own rock, so the lenses of the next band do not stop it.
    let drill = (index + 2).min(15);
    // The clay beds open on the stock hull; a hull tier per band after that.
    let hull = index.clamp(1, 15);
    (drill, hull)
}

/// A column near town clear of villages (they are safe zones).
fn quiet_column(world: &WorldState) -> i32 {
    let settings = *world.terrain().settings();
    let mut x = 252;
    while crate::worldgen::streamed::in_village_area(
        &settings,
        TilePosition {
            x,
            y: world.terrain().surface_row(x),
        },
        40,
    ) && x < 4_000
    {
        x += 37;
    }
    x
}

/// A rig of `band`'s typical gear drills down from the top of `band` on `seed`'s world for
/// `seconds`.
#[must_use]
pub fn exposure(seed: u64, band: Band, seconds: u32, tick_rate_hz: u32) -> Option<ExposureResult> {
    let mut world = WorldState::new_game_with_settings(
        BOT_PLAYER,
        &WorldSettings::new(seed),
        SimulationParams::new(tick_rate_hz),
    );
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    let x = quiet_column(&world);
    let top = band_top(&world, x, band)? + 1;
    for y in top - 1..=top {
        let _ = world
            .terrain_mut()
            .set_kind(TilePosition { x, y }, TileKind::Air);
    }
    let floor = TilePosition { x, y: top + 1 };
    if !world.terrain().is_solid_at(floor) {
        let _ = world.terrain_mut().set_kind(floor, TileKind::Stone);
    }
    let (drill, hull_level) = band_gear(band);
    {
        let rig = world.player_mut(BOT_PLAYER)?;
        rig.x = (x as f32 + 0.5) * TILE_SIZE;
        rig.y = (top + 1) as f32 * TILE_SIZE - 11.0;
        rig.fuel_capacity = 100_000.0;
        rig.fuel = 100_000.0;
        rig.cargo_capacity = 10_000;
        rig.block_hopper_capacity = 10_000;
        // Only enemies (and falls) may hurt it: hazards have their own counters.
        rig.hazard_gear = HazardGear::MAXED;
        rig.radiator_level = 15;
        rig.drill_strength = drill;
        rig.hull_level = hull_level;
    }
    let hz = world.tick_rate_hz();
    let mut result = ExposureResult::default();
    let _ = top_up_hull(&mut world);
    for tick in 0..seconds * hz {
        let Some(rig) = world.player(BOT_PLAYER) else {
            break;
        };
        let column_x = (x as f32 + 0.5) * TILE_SIZE;
        let grounded = DrillContact::of(world.terrain(), rig).grounded;
        let horizontal = ((column_x - rig.x) / (TILE_SIZE * 0.6)).clamp(-1.0, 1.0) * 0.4;
        let controls = ControlState::new(horizontal, rig.velocity_y > 230.0, grounded);
        let output = world.simulate_tick(
            world.simulation_tick(),
            &inputs_for(BOT_PLAYER, controls, Vec::new()),
        );
        let lost = f64::from(top_up_hull(&mut world));
        let Some(rig) = world.player(BOT_PLAYER) else {
            break;
        };
        let struck = output.events.iter().find_map(|event| match event.kind {
            GameplayEventKind::RigStruck { player_id, kind } if player_id == BOT_PLAYER => {
                Some(kind)
            }
            _ => None,
        });
        if let Some(kind) = struck {
            result.first_hit.get_or_insert(tick as f32 / hz as f32);
            *result.by_kind.entry(kind).or_default() += lost;
        } else {
            result.hazard += lost;
        }
        result.rows = rig.tile_position(TILE_SIZE).y - top;
    }
    Some(result)
}

/// Exposure per band and the early burrower threat.
pub struct ExposureProbe;

impl BalanceProbe for ExposureProbe {
    fn name(&self) -> &'static str {
        "exposure"
    }

    #[allow(clippy::too_many_lines, reason = "one report section, table by table")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let hz = context.config.tick_rate_hz;
        let seeds = context.config.seeds.clone();
        let mut section = ReportSection::new(
            "Combat: damage taken while drilling",
            format!(
                "An unarmed rig drills straight down from the top of each band for \
                 {EXPOSURE_SECONDS} s on Normal (real spawns, real AI), per seed. Gear: the \
                 drill and hull tiers a player typically has there (`band_gear`), every hazard \
                 counter and radiator maxed (so \"other\" is falls and crashes). Hull loss is per minute; \"wrecks\" is \
                 the share of runs whose enemy damage alone would have wrecked that band's \
                 typical hull within the run. The clay beds row (stock hull) is the early \
                 burrower threat."
            ),
        );
        let cases: Vec<Band> = Band::ALL.iter().skip(1).copied().collect();
        let jobs: Vec<_> = cases
            .iter()
            .flat_map(|band| {
                seeds
                    .iter()
                    .map(move |seed| move || exposure(*seed, *band, EXPOSURE_SECONDS, hz))
            })
            .collect();
        let results = run_parallel(context.config.threads, jobs);
        let mut table = Table::new(
            "Hull lost per minute while drilling:",
            &[
                "Band",
                "Typical hull",
                "Enemy loss / min",
                "Other loss / min",
                "Worst enemy",
                "First hit (median s)",
                "Wrecks",
                "Rows drilled",
            ],
        );
        let minutes = f64::from(EXPOSURE_SECONDS) / 60.0;
        for (index, band) in cases.iter().enumerate() {
            let runs: Vec<&ExposureResult> = results
                [index * seeds.len()..(index + 1) * seeds.len()]
                .iter()
                .flatten()
                .collect();
            if runs.is_empty() {
                continue;
            }
            let hull = hull_capacity_for_level(band_gear(*band).1);
            let enemy = mean(&runs.iter().map(|run| run.enemy()).collect::<Vec<_>>()) / minutes;
            let hazard = mean(&runs.iter().map(|run| run.hazard).collect::<Vec<_>>()) / minutes;
            let mut by_kind: BTreeMap<EnemyKind, f64> = BTreeMap::new();
            for run in &runs {
                for (kind, lost) in &run.by_kind {
                    *by_kind.entry(*kind).or_default() += lost;
                }
            }
            let worst = by_kind
                .iter()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map_or_else(
                    || "-".to_owned(),
                    |(kind, lost)| {
                        format!(
                            "{} ({:.0}/min)",
                            kind.name(),
                            lost / runs.len() as f64 / minutes
                        )
                    },
                );
            let hits: Vec<f64> = runs
                .iter()
                .filter_map(|run| run.first_hit.map(f64::from))
                .collect();
            let wrecks = runs
                .iter()
                .filter(|run| run.enemy() >= f64::from(hull))
                .count();
            table.row(vec![
                band.name().to_owned(),
                format!("{hull:.0}"),
                format!("{enemy:.1}"),
                format!("{hazard:.1}"),
                worst,
                if hits.is_empty() {
                    "-".to_owned()
                } else {
                    format!("{:.0} ({}/{})", median(&hits), hits.len(), runs.len())
                },
                format!("{}/{}", wrecks, runs.len()),
                format!(
                    "{:.0}",
                    mean(
                        &runs
                            .iter()
                            .map(|run| f64::from(run.rows))
                            .collect::<Vec<_>>()
                    )
                ),
            ]);
            let key = band.index();
            section.metric(format!("exposure.{key}.enemy_per_min"), enemy);
            section.metric(
                format!("exposure.{key}.enemy_share_of_hull_per_min"),
                enemy / f64::from(hull),
            );
            let burrower = by_kind.get(&EnemyKind::Burrower).copied().unwrap_or(0.0)
                / runs.len() as f64
                / minutes;
            section.metric(format!("exposure.{key}.burrower_per_min"), burrower);
        }
        section.tables.push(table);
        section
    }
}
