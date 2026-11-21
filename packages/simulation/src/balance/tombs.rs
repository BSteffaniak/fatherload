//! The tombs probe (Wave 5 T27, `--balance-report --only tombs`).
//!
//! Run length, death rate and loot value per tier and style, from a scripted bot playing
//! whole runs through the real tick (T28's side-view realizer, T29's crypt realizer).
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
    /// T28: what the bot knows of a side-view floor's traps.
    side: super::tomb_side::SideBot,
    /// The top-down crypt half (T29, [`super::crypt_bot`]).
    crypt: super::crypt_bot::CryptBot,
}

impl TombBot {
    #[must_use]
    pub fn new(player: PlayerId, armed: bool) -> Self {
        Self {
            player,
            armed,
            best_x: f32::MIN,
            stall: 0,
            jump: 0,
            use_cooldown: 0,
            floor: 0,
            side: super::tomb_side::SideBot::new(),
            crypt: super::crypt_bot::CryptBot::default(),
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
    #[allow(
        clippy::too_many_lines,
        reason = "the bot's whole decision in reading order"
    )]
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
        // Crypts play top-down (T29).
        if run.site.style == SiteStyle::TopDown {
            return self.crypt.inputs(world, run, player, self.armed);
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
            Self::top_down(world, run, x, feet, column)
        } else if let Some(lever) = super::tomb_side::SideBot::lever_behind(world, run, (x, feet))
            && (lever - x).abs() > TILE_SIZE * 0.9
        {
            // T28: the gate ahead is shut and its lever is behind: go back up for it.
            let side = (lever - x).signum();
            let feet_row = ((feet - 4.0) / TILE_SIZE).floor() as i32;
            let ahead = ((x + side * (PILOT_HALF_WIDTH + 6.0)) / TILE_SIZE).floor() as i32;
            let blocked = Self::solid(
                world,
                TilePosition {
                    x: ahead,
                    y: feet_row,
                },
            );
            if blocked && self.jump == 0 && state.grounded {
                self.jump = 14;
            }
            let jumping = self.jump > 0;
            self.jump = self.jump.saturating_sub(1);
            self.stall = 0;
            ControlState::new(side, false, false).with_jump(jumping)
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
            // T28: a pressure plate down a pit is for stepping on, not jumping over.
            let plate_at = |column: i32, depth: i32| {
                matches!(
                    world
                        .terrain()
                        .tile(TilePosition {
                            x: column,
                            y: feet_row + depth,
                        })
                        .map(|found| found.kind),
                    Some(TileKind::Built(BuiltTile::PressurePlate {
                        down: false,
                        ..
                    }))
                )
            };
            // Stop once the whole body hangs over the plate pit (it drops onto a plate).
            let over_plate = [x - PILOT_HALF_WIDTH + 1.0, x + PILOT_HALF_WIDTH - 1.0]
                .into_iter()
                .map(|edge| (edge / TILE_SIZE).floor() as i32)
                .all(|here| {
                    !Self::solid(
                        world,
                        TilePosition {
                            x: here,
                            y: feet_row + 1,
                        },
                    ) && plate_at(here, 2)
                });
            let pit = pit && !plate_at(column + 1, 2) && !plate_at(column + 1, 3);
            let wants_jump = (blocked || pit) && self.jump == 0 && state.grounded;
            // T28: read the traps ahead (blades, crushers, darts, fog, sand) before stepping.
            let advice = self.side.advise(
                world,
                run,
                (x, feet),
                state.height(),
                wants_jump || self.jump > 0,
            );
            if advice != super::tomb_side::Advice::Go {
                self.stall = 0;
            }
            if wants_jump && advice == super::tomb_side::Advice::Go {
                self.jump = 14;
            }
            let jumping = self.jump > 0;
            self.jump = self.jump.saturating_sub(1);
            // Long stuck: back off a little to come at it again.
            let backing = self.stall > 150 && self.stall % 120 < 20;
            let horizontal = match advice {
                _ if jumping => 1.0,
                _ if over_plate => 0.0,
                super::tomb_side::Advice::Go if !backing => 1.0,
                super::tomb_side::Advice::Wait => 0.0,
                _ => -1.0,
            };
            ControlState::new(horizontal, false, false).with_jump(jumping)
        };
        if self.armed {
            // Aim straight at the nearest creature in sight within fourteen tiles (past the
            // guardian's twelve-tile bolts) and fire.
            let (mx, my) = state.middle(body);
            let target = world
                .enemies()
                .map(|(entity, _)| (entity.body.x - mx, entity.body.y - my))
                .filter(|(dx, dy)| {
                    dx.hypot(*dy) <= TILE_SIZE * 14.0
                        && crate::physics::line_clear(world.terrain(), mx, my, mx + dx, my + dy)
                })
                .min_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)));
            // The rifle dry: the sidearm never runs out.
            let dry = world.player(player).is_some_and(|rig| {
                rig.pilot_arsenal.equipped == Some(WeaponKind::Rifle)
                    && rig.pilot_arsenal.ammo(AmmoKind::Cartridge) == 0
                    && rig.pilot_arsenal.owned.contains(&WeaponKind::Sidearm)
            });
            if dry && self.use_cooldown == 0 {
                inputs.act_as(
                    player,
                    PlayerCommand::Pilot(PilotAction::Equip(Some(WeaponKind::Sidearm))),
                );
                self.use_cooldown = 10;
            }
            let guardian = run
                .boss
                .and_then(|boss| world.entities().get(crate::entity::EntityId(boss)))
                .map(|entity| (entity.body.x, entity.body.y));
            let guardian_near =
                guardian.is_some_and(|(gx, gy)| (gx - mx).hypot(gy - my) <= TILE_SIZE * 11.0);
            let guardian_behind = guardian.is_some_and(|(gx, _)| gx < mx - TILE_SIZE * 2.0);
            if let Some((dx, dy)) = target {
                let length = dx.hypot(dy).max(1.0);
                controls = controls.with_weapon(WeaponControl {
                    fire: true,
                    auto_aim: false,
                    aim_x: (dx / length * 127.0) as i8,
                    aim_y: (dy / length * 127.0) as i8,
                });
                // Hold ground against a guardian in sight: the stairs wait until it falls.
                if run.on_last_floor() && !run.boss_defeated && guardian_near {
                    controls = ControlState::new(0.0, false, false)
                        .with_jump(controls.jump)
                        .with_weapon(controls.weapon);
                }
            }
            // T28: a guardian that hung back in its crypt is walked back to.
            if run.on_last_floor() && !run.boss_defeated && !guardian_near && guardian_behind {
                controls = ControlState::new(-1.0, false, false)
                    .with_jump(controls.jump)
                    .with_weapon(controls.weapon);
            }
        }
        inputs.hold_as(player, controls)
    }

    fn top_down(world: &WorldState, run: &TombRun, x: f32, feet: f32, column: i32) -> ControlState {
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
    /// Health the pilot lost over the run.
    pub hurt: u32,
    /// Creatures the bot killed.
    pub kills: u32,
    /// Shots and swings the bot made.
    pub shots: u32,
    /// Where the bot went down for the last time (floor, room), if it did.
    pub downed_in: Option<(u8, crate::tomb::RoomKind)>,
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
        // Kit that fits the tier: deeper and higher tombs are entered by better-equipped
        // pilots (suit and armour tier one below the site's, a jetpack).
        let kit = site.tier.saturating_sub(1).min(3);
        rig.pilot_gear.suit = kit;
        rig.pilot_gear.armor = kit;
        rig.pilot_gear.jetpack = 1;
        let _ = rig.pilot_arsenal.receive(WeaponKind::Rifle);
        let _ = rig.pilot_arsenal.receive(WeaponKind::Sidearm);
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
        hurt: 0,
        kills: 0,
        shots: 0,
        downed_in: None,
    };
    for _ in 0..hz * 60 * RUN_MINUTES {
        if world.tomb_run_of(player).is_none() {
            break;
        }
        let inputs = bot.inputs(&world);
        let tick = world.simulation_tick();
        let output = world.simulate_tick(tick, &inputs);
        for event in output.events {
            match event.kind {
                GameplayEventKind::PilotHurt { amount, .. } => {
                    result.hurt += u32::from(amount);
                }
                GameplayEventKind::EnemyKilled { .. } => result.kills += 1,
                GameplayEventKind::PilotFired { .. } => result.shots += 1,
                GameplayEventKind::TombMemberDowned { revived: false, .. } => {
                    result.downed_in = world.tomb_run_of(player).and_then(|run| {
                        let (_, body, state) = world.pilot_of(player)?;
                        let (x, y) = state.middle(body);
                        let tile = TilePosition {
                            x: (x / TILE_SIZE).floor() as i32,
                            y: (y / TILE_SIZE).floor() as i32,
                        };
                        let room = run.rooms.iter().find(|room| room.interior.contains(tile))?;
                        Some((run.floor, room.kind))
                    });
                }
                _ => {}
            }
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

/// Loot on offer in a whole run of `site` (run 0): every cache of every floor at its value,
/// plus the guardian's hoard, and how many caches that is.
#[must_use]
pub fn loot_on_offer(site: &TombSite) -> (u32, u32) {
    let mut total = crate::world::guardian_hoard(site.tier);
    let mut caches = 0;
    for floor in 0..site.floors() {
        let plan = crate::tomb::floor::plan_floor(site, 0, floor);
        let realized = crate::tomb::realizer_for(site.style).realize(&plan);
        for (tile, kind) in &realized.cells {
            if *kind == TileKind::Built(BuiltTile::TombCache { open: false }) {
                caches += 1;
                total += crate::world::cache_value(site.seed, site.tier, floor, *tile);
            }
        }
    }
    (total, caches)
}

/// Loot on offer per tier and style (pure arithmetic over the floor plans, every seed).
fn loot_table(seeds: &[u64], section: &mut ReportSection) -> Table {
    let mut table = Table::new(
        "Loot on offer in a whole run (every cache plus the guardian's hoard, run 0, mean over \
         seeds):",
        &["Tier", "Style", "Floors", "Caches", "Credits"],
    );
    for tier in 1..=crate::tomb::MAX_TIER {
        for style in SiteStyle::ALL {
            let offers: Vec<(u8, u32, u32)> = seeds
                .iter()
                .filter_map(|seed| {
                    let settings = WorldSettings {
                        generator: GeneratorVersion::V10,
                        ..WorldSettings::new(*seed)
                    };
                    find_site(&settings, style, tier).map(|site| {
                        let (credits, caches) = loot_on_offer(&site);
                        (site.floors(), caches, credits)
                    })
                })
                .collect();
            if offers.is_empty() {
                continue;
            }
            let credits = mean(
                &offers
                    .iter()
                    .map(|(_, _, credits)| f64::from(*credits))
                    .collect::<Vec<_>>(),
            );
            let caches = mean(
                &offers
                    .iter()
                    .map(|(_, caches, _)| f64::from(*caches))
                    .collect::<Vec<_>>(),
            );
            section.metric(
                format!(
                    "tombs.t{tier}.{}.loot_on_offer",
                    match style {
                        SiteStyle::SideView => "side",
                        SiteStyle::TopDown => "crypt",
                    }
                ),
                credits,
            );
            table.row(vec![
                tier.to_string(),
                style.noun().to_owned(),
                offers[0].0.to_string(),
                format!("{caches:.1}"),
                super::grouped(credits),
            ]);
        }
    }
    table
}

/// The tombs section of the balance report.
pub struct TombsProbe;

impl BalanceProbe for TombsProbe {
    fn name(&self) -> &'static str {
        "tombs"
    }

    #[allow(clippy::too_many_lines, reason = "one table per section")]
    fn run(&self, context: &mut ProbeContext) -> ReportSection {
        let config = context.config.clone();
        let mut section = ReportSection::new(
            "Tombs and crypts: run length, deaths and loot",
            "A scripted bot (rifle on auto-aim, first relic each floor) plays one run per seed, \
             tier and style on V10 worlds, Normal enemies, through the real tick on the \
             side-view realizer (T28) and the crypt realizer (T29, its own bot). Banked is the run's \
             credits including the guardian's \
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
        let loot = loot_table(&seeds, &mut section);
        section.tables.push(loot);
        let total = runs.len().max(1) as f64;
        section.metric(
            "tombs.death_rate",
            runs.iter()
                .filter(|run| run.outcome == Some(RunOutcome::Wiped))
                .count() as f64
                / total,
        );
        let mut wipes: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        // Crypts (T29) apart: their rooms tell a different story.
        let mut crypt_wipes: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for run in runs
            .iter()
            .filter(|run| run.outcome == Some(RunOutcome::Wiped))
        {
            let place = run
                .downed_in
                .map_or("between rooms (a pit)", |(_, room)| room.name());
            *wipes.entry(place).or_default() += 1;
            if run.style == SiteStyle::TopDown {
                *crypt_wipes.entry(place).or_default() += 1;
            }
        }
        if !crypt_wipes.is_empty() {
            section.notes.push(format!(
                "Crypt wipes by room: {}.",
                crypt_wipes
                    .iter()
                    .map(|(place, count)| format!("{place} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !wipes.is_empty() {
            section.notes.push(format!(
                "Where the bot's wipes happened: {}.",
                wipes
                    .iter()
                    .map(|(place, count)| format!("{place} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        section.notes.push(format!(
            "{} runs; cache value per tier is (30 + 25 x floor + d40) x tier^2 credits, the \
             guardian's hoard 300 x tier^2, the guardian's health 0.25 + 0.25 x tier of its \
             stock.",
            runs.len()
        ));
        section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "debug: follows the crypt bot and prints where it stalls"]
    #[allow(
        clippy::too_many_lines,
        reason = "a debug trace, printed in reading order"
    )]
    fn trace_crypt_bot() {
        let seed: u64 = std::env::var("SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(11);
        let tier: u8 = std::env::var("TIER")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        let settings = WorldSettings {
            generator: GeneratorVersion::V10,
            ..WorldSettings::new(seed)
        };
        let site = find_site(&settings, SiteStyle::TopDown, tier).expect("site");
        let player = PlayerId::new(0);
        let mut world =
            WorldState::new_game_with_settings(player, &settings, SimulationParams::new(60));
        world.set_enemy_difficulty(EnemyDifficulty::Normal);
        if let Some(rig) = world.player_mut(player) {
            let kit = site.tier.saturating_sub(1).min(3);
            rig.pilot_gear.suit = kit;
            rig.pilot_gear.armor = kit;
            rig.pilot_gear.jetpack = 1;
            let _ = rig.pilot_arsenal.receive(WeaponKind::Rifle);
            let _ = rig.pilot_arsenal.receive(WeaponKind::Sidearm);
            rig.pilot_arsenal.equipped = Some(WeaponKind::Rifle);
            let _ = rig
                .pilot_arsenal
                .add_ammo(AmmoKind::Cartridge, crate::pilot::arsenal::MAX_PILOT_AMMO);
        }
        let _ = world.place_pilot(player, 0.0, 0.0);
        let _ = world.scenario_enter_tomb(&site, &[player]);
        let mut bot = TombBot::new(player, true);
        let mut still = 0;
        let mut last = (0.0, 0.0);
        let mut hurts: Vec<String> = Vec::new();
        for tick in 0..60 * 60 * 6 {
            let Some(run) = world.tomb_run_of(player).cloned() else {
                println!(
                    "run over at {tick}: {:?}",
                    world.tombs().summaries.get(&player)
                );
                return;
            };
            let inputs = bot.inputs(&world);
            let now = world.simulation_tick();
            let output = world.simulate_tick(now, &inputs);
            for event in &output.events {
                match event.kind {
                    GameplayEventKind::CryptPitFall { x, y, .. }
                        if std::env::var("PITS").is_ok() =>
                    {
                        println!(
                            "pit fall at tick {tick} into ({x}, {y}), previous {last:?} inputs {:?}\n{}",
                            inputs.players.get(&player),
                            super::super::crypt_bot::floor_text(
                                &world,
                                &run,
                                Some(TilePosition { x, y })
                            )
                        );
                    }
                    GameplayEventKind::PilotHurt { amount, .. } => {
                        let near: Vec<String> = world
                            .enemies()
                            .filter(|(entity, _)| {
                                world.pilot_of(player).is_some_and(|(_, body, _)| {
                                    (entity.body.x - body.x).hypot(entity.body.y - body.y)
                                        < TILE_SIZE * 4.0
                                })
                            })
                            .map(|(_, state)| format!("{:?}/{:?}", state.kind, state.mode))
                            .collect();
                        hurts.push(format!("t{tick} -{amount} near {near:?}"));
                    }
                    GameplayEventKind::TombMemberDowned { revived: false, .. } => {
                        let tile = world.pilot_of(player).map(|(_, body, _)| TilePosition {
                            x: (body.x / TILE_SIZE).floor() as i32,
                            y: ((body.y - 12.5) / TILE_SIZE).floor() as i32,
                        });
                        println!(
                            "downed at tick {tick} floor {}\nlast hurts: {:?}\n{}",
                            run.floor,
                            &hurts[hurts.len().saturating_sub(12)..],
                            super::super::crypt_bot::floor_text(&world, &run, tile)
                        );
                    }
                    _ => {}
                }
            }
            let Some((_, body, _)) = world.pilot_of(player) else {
                continue;
            };
            if (body.x - last.0).hypot(body.y - last.1) < 0.5 {
                still += 1;
            } else {
                still = 0;
            }
            last = (body.x, body.y);
            if still == 300 {
                let tile = TilePosition {
                    x: (body.x / TILE_SIZE).floor() as i32,
                    y: ((body.y - 12.5) / TILE_SIZE).floor() as i32,
                };
                println!(
                    "stalled at tick {tick} floor {} phase {:?} boss {:?} defeated {}\n{}",
                    run.floor,
                    run.phase,
                    run.boss,
                    run.boss_defeated,
                    super::super::crypt_bot::floor_text(&world, &run, Some(tile))
                );
                return;
            }
        }
        println!("still running");
    }

    #[test]
    #[ignore = "debug: prints a few probe runs"]
    fn print_probe_runs() {
        for seed in [11_u64, 4_242, 90_001] {
            let settings = WorldSettings {
                generator: GeneratorVersion::V10,
                ..WorldSettings::new(seed)
            };
            for tier in [1, 2, 3] {
                for style in SiteStyle::ALL {
                    if let Some(site) = find_site(&settings, style, tier) {
                        println!(
                            "{seed} t{tier} {style:?}: {:?}",
                            play_run(&settings, &site, 60)
                        );
                    }
                }
            }
        }
    }
}
