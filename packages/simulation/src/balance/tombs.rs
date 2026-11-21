//! The tombs probe (Wave 5 T27, `--balance-report --only tombs`): run length, death rate and
//! loot value per tier and style, from a scripted bot playing whole runs through the real
//! tick on the placeholder realizers.
//!
//! The bot ([`TombBot`]) is the same one the world tests clear runs with: it walks east, jumps
//! obstacles and two-deep pits (side view) or slides around spike floors and walks to levers
//! (top down), uses whatever fixture is in reach (levers, shrines, caches, the stairs), takes the
//! first relic offered, and shoots the nearest creature with a rifle on auto-aim. It plays on
//! Normal enemies; a run it cannot finish in [`RUN_MINUTES`] counts as a timeout.

use super::{BalanceProbe, ProbeContext, ReportSection, Table, mean, run_parallel};
use crate::{
    ControlState, PlayerCommand, PlayerId, SimulationParams, TickInputs, WeaponControl, WeaponKind,
    WorldState,
    base::BuiltTile,
    enemy::EnemyDifficulty,
    event::GameplayEventKind,
    mechanism::Usable,
    pilot::{PILOT_HALF_WIDTH, PilotAction, PilotView},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{RunOutcome, RunPhase, SiteStyle, TombCommand, TombRun, TombSite},
    weapon::AmmoKind,
    worldgen::{GeneratorVersion, WorldSettings, features::TileRect, tomb_sites},
};

/// Game minutes a probe run may take before it counts as a timeout.
pub const RUN_MINUTES: u32 = 12;

/// A bot that plays a tomb run (see the module docs).
#[derive(Clone, Debug)]
pub struct TombBot {
    pub player: PlayerId,
    /// Shoot creatures (the probe); the world tests play on Peaceful without.
    pub armed: bool,
    best_x: f32,
    stall: u32,
    jump: u32,
    use_cooldown: u32,
    floor: u8,
}

impl TombBot {
    #[must_use]
    pub const fn new(player: PlayerId, armed: bool) -> Self {
        Self {
            player,
            armed,
            best_x: f32::MIN,
            stall: 0,
            jump: 0,
            use_cooldown: 0,
            floor: 0,
        }
    }

    fn solid(world: &WorldState, tile: TilePosition) -> bool {
        crate::world::pilot::pilot_blocks(world.terrain(), tile)
    }

    fn trap(world: &WorldState, tile: TilePosition) -> bool {
        matches!(
            world.terrain().tile(tile).map(|found| found.kind),
            Some(TileKind::Built(BuiltTile::TombTrap { .. }))
        )
    }

    /// This tick's inputs for the bot's player.
    #[must_use]
    pub fn inputs(&mut self, world: &WorldState) -> TickInputs {
        let player = self.player;
        let mut inputs = TickInputs::default();
        let Some(run) = world.tomb_run_of(player) else {
            return inputs;
        };
        if run.floor != self.floor {
            self.floor = run.floor;
            self.best_x = f32::MIN;
            self.stall = 0;
        }
        if run.phase == RunPhase::Picking {
            if run.member(player).is_some_and(|member| !member.picked) {
                inputs.act_as(player, PlayerCommand::Tomb(TombCommand::PickRelic(Some(0))));
            }
            return inputs;
        }
        self.use_cooldown = self.use_cooldown.saturating_sub(1);
        if self.use_cooldown == 0
            && let Some((_, usable)) = world.pilot_usable(player)
        {
            let wanted = match usable {
                Usable::Lever { on, .. } => !on,
                Usable::TombShrine { lit } => !lit,
                Usable::TombStairs | Usable::TombCache => true,
                _ => false,
            };
            if wanted {
                inputs.act_as(player, PlayerCommand::Pilot(PilotAction::Use));
                self.use_cooldown = 20;
            }
        }
        let Some((_, body, state)) = world.pilot_of(player) else {
            return inputs;
        };
        let (x, feet) = (body.x, body.y);
        if x > self.best_x + 2.0 {
            self.best_x = x;
            self.stall = 0;
        } else {
            self.stall += 1;
        }
        let column = ((x + PILOT_HALF_WIDTH + 6.0) / TILE_SIZE).floor() as i32;
        let mut controls = if state.view == PilotView::TopDown {
            self.top_down(world, run, x, feet, column)
        } else {
            let feet_row = ((feet - 4.0) / TILE_SIZE).floor() as i32;
            let blocked = Self::solid(
                world,
                TilePosition {
                    x: column,
                    y: feet_row,
                },
            );
            // A pit is exactly two deep (a ledge's or a step's edge is not).
            let open_below = |depth: i32| {
                !Self::solid(
                    world,
                    TilePosition {
                        x: column + 1,
                        y: feet_row + depth,
                    },
                )
            };
            let pit = open_below(1) && open_below(2) && !open_below(3);
            if (blocked || pit) && self.jump == 0 && state.grounded {
                self.jump = 14;
            }
            let jumping = self.jump > 0;
            self.jump = self.jump.saturating_sub(1);
            // Long stuck: back off a little to come at it again.
            let backing = self.stall > 150 && self.stall % 120 < 20;
            ControlState::new(if backing { -1.0 } else { 1.0 }, false, false).with_jump(jumping)
        };
        if self.armed {
            let (mx, my) = state.middle(body);
            let near = !world.enemies_near(mx, my, TILE_SIZE * 9.0).is_empty();
            controls = controls.with_weapon(WeaponControl {
                fire: near,
                auto_aim: true,
                aim_x: 127,
                aim_y: 0,
            });
        }
        inputs.hold_as(player, controls)
    }

    fn top_down(
        &mut self,
        world: &WorldState,
        run: &TombRun,
        x: f32,
        feet: f32,
        column: i32,
    ) -> ControlState {
        let head_row = ((feet - 49.95) / TILE_SIZE).floor() as i32;
        let feet_row = ((feet - 0.05) / TILE_SIZE).floor() as i32;
        let here = TilePosition {
            x: (x / TILE_SIZE).floor() as i32,
            y: feet_row,
        };
        let room = run.rooms.iter().find(|room| room.interior.contains(here));
        let mid = room.map_or(feet_row, |room| {
            i32::midpoint(room.interior.min.y, room.interior.max.y)
        });
        let target_feet = |row: i32| (row as f32 + 1.0) * TILE_SIZE - 6.0;
        // A closed gate on the room's east doorway: go to the room's lever.
        if let Some(room) = room
            && matches!(
                world
                    .terrain()
                    .tile(TilePosition {
                        x: room.interior.max.x + 1,
                        y: mid
                    })
                    .map(|found| found.kind),
                Some(TileKind::Built(BuiltTile::Gate))
            )
        {
            let lever = (room.interior.min.y..=room.interior.max.y)
                .flat_map(|y| (room.interior.min.x..=room.interior.max.x).map(move |x| (x, y)))
                .map(|(x, y)| TilePosition { x, y })
                .find(|tile| {
                    matches!(
                        world.terrain().tile(*tile).map(|found| found.kind),
                        Some(TileKind::Built(BuiltTile::Lever { on: false, .. }))
                    )
                });
            if let Some(lever) = lever {
                let target_x = (lever.x as f32 - 0.5) * TILE_SIZE;
                let dx = target_x - x;
                let dy = target_feet(lever.y) - feet;
                return ControlState::new(
                    if dx.abs() > 4.0 { dx.signum() } else { 0.0 },
                    dy < -3.0,
                    dy > 3.0,
                );
            }
        }
        let open = |column: i32, rows: std::ops::RangeInclusive<i32>| {
            rows.into_iter().all(|y| {
                let tile = TilePosition { x: column, y };
                !Self::solid(world, tile) && !Self::trap(world, tile)
            })
        };
        if open(column, head_row..=feet_row) {
            // Clear ahead: walk on, drifting back toward the doorway rows.
            let dy = target_feet(mid) - feet;
            let drift_ok = open(column, (mid - 1).min(head_row)..=mid.max(feet_row));
            return ControlState::new(1.0, drift_ok && dy < -3.0, drift_ok && dy > 3.0);
        }
        // Blocked: slide up or down to the nearest two open rows in the column ahead.
        let window = (0..8)
            .flat_map(|step| [feet_row - step, feet_row + step])
            .find(|row| open(column, row - 1..=*row));
        let dy = window.map_or(0.0, |row| target_feet(row) - feet);
        ControlState::new(if dy.abs() <= 3.0 { 1.0 } else { 0.0 }, dy < -3.0, dy > 3.0)
    }
}

/// How one probe run went.
#[derive(Clone, Debug, PartialEq)]
pub struct ProbeRun {
    pub style: SiteStyle,
    pub tier: u8,
    pub outcome: Option<RunOutcome>,
    pub minutes: f64,
    pub banked: u32,
    pub lost: u32,
    pub floors: u8,
}

/// The first site of `style` and `tier` on `settings`' world, if any (ground, then sky).
#[must_use]
pub fn find_site(settings: &WorldSettings, style: SiteStyle, tier: u8) -> Option<TombSite> {
    let ground = TileRect {
        min: TilePosition {
            x: -12_000,
            y: -100,
        },
        max: TilePosition { x: 12_000, y: 900 },
    };
    let sky = TileRect {
        min: TilePosition {
            x: -24_000,
            y: -16_400,
        },
        max: TilePosition {
            x: 24_000,
            y: -9_000,
        },
    };
    [ground, sky].into_iter().find_map(|rect| {
        tomb_sites::sites_touching(settings, &rect)
            .into_iter()
            .find(|site| site.style == style && site.tier == tier)
    })
}

/// Plays one run of `site` with an armed bot on Normal enemies.
#[must_use]
pub fn play_run(settings: &WorldSettings, site: &TombSite, hz: u32) -> ProbeRun {
    let player = PlayerId::new(0);
    let mut world = WorldState::new_game_with_settings(player, settings, SimulationParams::new(hz));
    world.set_enemy_difficulty(EnemyDifficulty::Normal);
    if let Some(rig) = world.player_mut(player) {
        let _ = rig.pilot_arsenal.receive(WeaponKind::Rifle);
        rig.pilot_arsenal.equipped = Some(WeaponKind::Rifle);
        let _ = rig
            .pilot_arsenal
            .add_ammo(AmmoKind::Cartridge, crate::pilot::arsenal::MAX_PILOT_AMMO);
    }
    let _ = world.place_pilot(player, 0.0, 0.0);
    let started = world.simulation_tick();
    let _ = world.scenario_enter_tomb(site, &[player]);
    let mut bot = TombBot::new(player, true);
    let mut result = ProbeRun {
        style: site.style,
        tier: site.tier,
        outcome: None,
        minutes: f64::from(RUN_MINUTES),
        banked: 0,
        lost: 0,
        floors: 0,
    };
    for _ in 0..hz * 60 * RUN_MINUTES {
        if world.tomb_run_of(player).is_none() {
            break;
        }
        let inputs = bot.inputs(&world);
        let tick = world.simulation_tick();
        let output = world.simulate_tick(tick, &inputs);
        for event in output.events {
            if let GameplayEventKind::TombRunEnded {
                outcome,
                banked,
                lost,
                ..
            } = event.kind
            {
                result.outcome = Some(outcome);
                result.banked = banked;
                result.lost = lost;
                result.minutes = f64::from(
                    u32::try_from(world.simulation_tick().get() - started.get()).unwrap_or(0),
                ) / f64::from(hz * 60);
            }
        }
        if let Some(run) = world.tomb_run_of(player) {
            result.floors = run.floor;
        }
    }
    if let Some(summary) = world.tombs().summaries.get(&player) {
        result.floors = summary.floors;
    }
    result
}

/// The tombs section of the balance report.
pub struct TombsProbe;

impl BalanceProbe for TombsProbe {
    fn name(&self) -> &'static str {
        "tombs"
    }

    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let config = context.config.clone();
        let mut section = ReportSection::new(
            "Tombs and crypts: run length, deaths and loot",
            "A scripted bot (rifle on auto-aim, first relic each floor) plays one run per seed, \
             tier and style on V10 worlds, Normal enemies, through the real tick on the \
             placeholder realizers. Banked is the run's credits including the guardian's \
             hoard; a run the bot cannot finish in the time limit counts as a timeout.",
        );
        let seeds: Vec<u64> = config.seeds.iter().copied().take(4).collect();
        let mut jobs = Vec::new();
        for &seed in &seeds {
            let settings = WorldSettings {
                generator: GeneratorVersion::V10,
                ..WorldSettings::new(seed)
            };
            for tier in 1..=crate::tomb::MAX_TIER {
                for style in SiteStyle::ALL {
                    let hz = config.tick_rate_hz;
                    jobs.push(move || {
                        find_site(&settings, style, tier).map(|site| play_run(&settings, &site, hz))
                    });
                }
            }
        }
        let runs: Vec<ProbeRun> = run_parallel(config.threads, jobs)
            .into_iter()
            .flatten()
            .collect();
        let mut table = Table::new(
            "Per tier and style (mean over seeds):",
            &[
                "Tier",
                "Style",
                "Runs",
                "Cleared",
                "Wiped",
                "Timeouts",
                "Minutes",
                "Banked cr",
                "Lost cr",
            ],
        );
        for tier in 1..=crate::tomb::MAX_TIER {
            for style in SiteStyle::ALL {
                let these: Vec<&ProbeRun> = runs
                    .iter()
                    .filter(|run| run.tier == tier && run.style == style)
                    .collect();
                if these.is_empty() {
                    continue;
                }
                let count = these.len();
                let cleared = these
                    .iter()
                    .filter(|run| run.outcome == Some(RunOutcome::Finished))
                    .count();
                let wiped = these
                    .iter()
                    .filter(|run| run.outcome == Some(RunOutcome::Wiped))
                    .count();
                let timeouts = these.iter().filter(|run| run.outcome.is_none()).count();
                let minutes = mean(&these.iter().map(|run| run.minutes).collect::<Vec<_>>());
                let banked = mean(
                    &these
                        .iter()
                        .map(|run| f64::from(run.banked))
                        .collect::<Vec<_>>(),
                );
                let lost = mean(
                    &these
                        .iter()
                        .map(|run| f64::from(run.lost))
                        .collect::<Vec<_>>(),
                );
                let key = format!(
                    "tombs.t{tier}.{}",
                    match style {
                        SiteStyle::SideView => "side",
                        SiteStyle::TopDown => "crypt",
                    }
                );
                section.metric(format!("{key}.death_rate"), wiped as f64 / count as f64);
                section.metric(format!("{key}.minutes"), minutes);
                section.metric(format!("{key}.banked"), banked);
                table.row(vec![
                    tier.to_string(),
                    style.noun().to_owned(),
                    count.to_string(),
                    cleared.to_string(),
                    wiped.to_string(),
                    timeouts.to_string(),
                    format!("{minutes:.1}"),
                    super::grouped(banked),
                    super::grouped(lost),
                ]);
            }
        }
        section.tables.push(table);
        let total = runs.len().max(1) as f64;
        section.metric(
            "tombs.death_rate",
            runs.iter()
                .filter(|run| run.outcome == Some(RunOutcome::Wiped))
                .count() as f64
                / total,
        );
        section.notes.push(format!(
            "{} runs; cache value per tier is (30 + 25 x floor + d40) x tier^2 credits, the \
             guardian's hoard 300 x tier^2.",
            runs.len()
        ));
        section
    }
}
