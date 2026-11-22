//! The tombs bot's side-view sense (WP-TOMBS T28): reading the traps ahead.
//!
//! The side-view traps are pure functions of the world clock ([`crate::tomb::traps`]), so the
//! bot can look ahead exactly: before each step it predicts where its body will be over the
//! next [`LOOK_AHEAD_MILLIS`] walking on, standing still or backing off, tests every timed trap
//! near it at those moments, and takes the first safe choice (walking on if nothing is safe).
//! It never stops on a crumbling slab, waits at a sluice basin's edge until the sand has long
//! enough to stay down, and only jumps when no dart would cross the jump.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "tile coordinates and world pixels"
)]

use crate::{
    WorldState,
    base::BuiltTile,
    pilot::PILOT_HALF_WIDTH,
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        TombRun,
        side_view::dart_facing,
        traps::{
            BLADE_RADIUS, Bounds, CRUSHER_MAX_REACH, CrusherStage, DART_MAX_REACH, TrapKind,
            blade_tip, crusher_block, crusher_state, dart_bounds, dart_position, fog_centre,
            fog_hurts, sand_clear_for, timed, world_millis,
        },
    },
    worldgen::{features::TileRect, tomb_sites},
};

/// How far ahead the bot looks, milliseconds.
pub const LOOK_AHEAD_MILLIS: u64 = 900;
/// The bot's walking speed for its predictions, pixels per second.
const WALK: f32 = 175.0;
/// Time the bot wants with the sand down to cross a basin.
const BASIN_MILLIS: u64 = 4_500;

/// What the bot should do this tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Advice {
    /// Walk on (east).
    Go,
    /// Stand still: something ahead is about to sweep the path.
    Wait,
    /// Step back west: it is about to sweep where the bot stands.
    Back,
}

/// One timed trap of the floor and its reach.
#[derive(Clone, Copy, Debug)]
struct Seen {
    tile: TilePosition,
    kind: TrapKind,
    facing: i8,
    reach: i32,
}

/// The side-view half of [`super::tombs::TombBot`]: the floor's timed traps and sluice rooms.
#[derive(Clone, Debug, Default)]
pub struct SideBot {
    key: Option<(u32, u8)>,
    traps: Vec<Seen>,
    sluices: Vec<TileRect>,
    /// The world's frame at the last look (F7: trap positions are frame-relative).
    frame: crate::position::Frame,
}

fn blocks(world: &WorldState, tile: TilePosition) -> bool {
    crate::world::pilot::pilot_blocks(world.terrain(), tile)
}

impl SideBot {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            key: None,
            traps: Vec::new(),
            sluices: Vec::new(),
            frame: crate::position::Frame::FIXED,
        }
    }

    /// Reads the floor once per floor (traps do not move; their reach is the open run in front
    /// of them).
    fn refresh(&mut self, world: &WorldState, run: &TombRun) {
        self.frame = world.frame();
        let key = (run.id, run.floor);
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.traps.clear();
        self.sluices.clear();
        let origin = tomb_sites::slot_origin(run.slot);
        for y in origin.y..origin.y + tomb_sites::SLOT_SIZE.1 {
            for x in origin.x..origin.x + tomb_sites::SLOT_SIZE.0 {
                let tile = TilePosition { x, y };
                let Some(TileKind::Built(BuiltTile::TombTrap { kind, hidden })) =
                    world.terrain().tile(tile).map(|found| found.kind)
                else {
                    continue;
                };
                let kind = TrapKind::from_index(kind);
                if kind == TrapKind::SandSluice {
                    if let Some(room) = run.rooms.iter().find(|room| {
                        room.interior.max.y + 1 == y
                            && (room.interior.min.x..=room.interior.max.x).contains(&x)
                    }) && !self.sluices.contains(&room.interior)
                    {
                        self.sluices.push(room.interior);
                    }
                    continue;
                }
                if !timed(kind) {
                    continue;
                }
                let facing = dart_facing(hidden);
                let reach = match kind {
                    TrapKind::Crusher => (1..=CRUSHER_MAX_REACH)
                        .take_while(|dy| !blocks(world, TilePosition { x, y: y + dy }))
                        .count() as i32,
                    TrapKind::DartWall => (1..=DART_MAX_REACH)
                        .take_while(|dx| {
                            !blocks(
                                world,
                                TilePosition {
                                    x: x + dx * i32::from(facing),
                                    y,
                                },
                            )
                        })
                        .count() as i32,
                    _ => 0,
                };
                self.traps.push(Seen {
                    tile,
                    kind,
                    facing,
                    reach,
                });
            }
        }
    }

    fn hits(frame: crate::position::Frame, seen: &Seen, millis: u64, body: &Bounds) -> bool {
        match seen.kind {
            TrapKind::Blade => {
                body.touches_disc(blade_tip(frame, seen.tile, millis), BLADE_RADIUS + 4.0)
            }
            TrapKind::Crusher => {
                // A shaking crusher counts as down already: never step under one.
                let (stage, extension) = crusher_state(seen.tile, millis);
                let extension = if stage == CrusherStage::Shaking {
                    1.0
                } else {
                    extension
                };
                crusher_block(frame, seen.tile, seen.reach, extension)
                    .is_some_and(|block| block.overlaps(body))
            }
            TrapKind::DartWall => dart_position(frame, seen.tile, seen.facing, seen.reach, millis)
                .is_some_and(|at| dart_bounds(at).overlaps(body)),
            TrapKind::FogVent => fog_hurts(seen.tile, millis).is_some_and(|radius| {
                body.touches_disc(fog_centre(frame, seen.tile), radius + 6.0)
            }),
            _ => false,
        }
    }

    /// Whether a body that starts as `start` and moves `speed` px/s sideways is hurt by a trap
    /// near it in the next [`LOOK_AHEAD_MILLIS`].
    fn path_hurts(&self, start: Bounds, speed: f32, now: u64) -> bool {
        let near: Vec<&Seen> = self
            .traps
            .iter()
            .filter(|seen| {
                let x = (self.frame.rel_col(seen.tile.x) + 0.5) * TILE_SIZE;
                (x - (start.left + start.right) * 0.5).abs() < TILE_SIZE * 18.0
            })
            .collect();
        if near.is_empty() {
            return false;
        }
        (0..=LOOK_AHEAD_MILLIS).step_by(50).any(|ahead| {
            let shift = speed * ahead as f32 / 1_000.0;
            let body = Bounds::new(
                start.left + shift,
                start.top,
                start.right + shift,
                start.bottom,
            );
            near.iter()
                .any(|seen| Self::hits(self.frame, seen, now + ahead, &body))
        })
    }

    /// The middle of the lever the bot walked past, when the gate ahead in its room is still
    /// shut (it goes back for it).
    #[must_use]
    pub fn lever_behind(world: &WorldState, run: &TombRun, (x, feet): (f32, f32)) -> Option<f32> {
        let here = world.frame().tile_at(x, feet - 4.0);
        let room = run.rooms.iter().find(|room| {
            let mut reach = room.interior;
            reach.max.x += 1;
            reach.contains(here)
        })?;
        let door = room.interior.max.x + 1;
        let shut = (room.interior.min.y..=room.interior.max.y + 1).any(|y| {
            matches!(
                world
                    .terrain()
                    .tile(TilePosition { x: door, y })
                    .map(|found| found.kind),
                Some(TileKind::Built(BuiltTile::Gate))
            )
        });
        if !shut {
            return None;
        }
        (room.interior.min.y..=room.interior.max.y)
            .flat_map(|y| (room.interior.min.x..=room.interior.max.x).map(move |x| (x, y)))
            .find(|(x, y)| {
                matches!(
                    world
                        .terrain()
                        .tile(TilePosition { x: *x, y: *y })
                        .map(|found| found.kind),
                    Some(TileKind::Built(BuiltTile::Lever { on: false, .. }))
                )
            })
            .map(|(x, _)| (world.frame().rel_col(x) + 0.5) * TILE_SIZE)
    }

    /// What to do this tick, for a pilot with feet at `(x, feet)` and body `height` tall that
    /// means to jump (`jumping`).
    #[must_use]
    pub fn advise(
        &mut self,
        world: &WorldState,
        run: &TombRun,
        (x, feet): (f32, f32),
        height: f32,
        jumping: bool,
    ) -> Advice {
        self.refresh(world, run);
        let now = world_millis(world.simulation_tick().get(), world.tick_rate_hz());
        // Never linger on a crumbling slab.
        let under = world.frame().tile_at(x, feet + 1.0);
        if matches!(
            world.terrain().tile(under).map(|found| found.kind),
            Some(TileKind::Built(BuiltTile::TombTrap { kind, .. }))
                if TrapKind::from_index(kind) == TrapKind::Crumble
        ) {
            return Advice::Go;
        }
        // A sluice basin ahead: wait on its edge for a long dry spell.
        let column = world.frame().col(x);
        let row = world.frame().row(feet - 1.0);
        if let Some(room) = self
            .sluices
            .iter()
            .find(|room| (room.min.x..=room.max.x).contains(&column))
        {
            let on_edge = row < room.max.y - 1 && column < room.min.x + 3;
            if on_edge && sand_clear_for(*room, now) < BASIN_MILLIS {
                return Advice::Wait;
            }
        }
        let reach_up = if jumping { TILE_SIZE * 2.6 } else { 0.0 };
        let body = Bounds::new(
            x - PILOT_HALF_WIDTH,
            feet - height - reach_up,
            x + PILOT_HALF_WIDTH,
            feet,
        );
        if !self.path_hurts(body, WALK, now) {
            return Advice::Go;
        }
        let standing = Bounds::new(body.left, feet - height, body.right, feet);
        if !self.path_hurts(standing, 0.0, now) {
            return Advice::Wait;
        }
        if !self.path_hurts(standing, -WALK, now) {
            return Advice::Back;
        }
        Advice::Go
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PlayerId, SimulationParams,
        enemy::EnemyDifficulty,
        tomb::SiteStyle,
        worldgen::{GeneratorVersion, WorldSettings},
    };

    /// The run's floor around the pilot as text (debugging the bot).
    fn around(world: &WorldState, run: &TombRun) -> String {
        let origin = tomb_sites::slot_origin(run.slot);
        let (x, y) = world
            .pilot_of(run.members[0].player)
            .map_or((0, 0), |(_, body, _)| {
                ((body.x / TILE_SIZE) as i32, (body.y / TILE_SIZE) as i32)
            });
        let mut text = format!("pilot at {:?}\n", (x - origin.x, y - origin.y));
        for row in y - 12..y + 5 {
            for column in x - 30..x + 30 {
                let tile = TilePosition { x: column, y: row };
                let glyph = if column == x && row == y - 1 {
                    '@'
                } else {
                    match world.terrain().tile(tile).map(|found| found.kind) {
                        Some(TileKind::Air) => '.',
                        Some(TileKind::Built(BuiltTile::TombTrap { kind, .. })) => {
                            ['^', 'B', 'C', 'K', 'D', 'S', 'F', 'W'][usize::from(kind % 8)]
                        }
                        Some(TileKind::Built(BuiltTile::Gate)) => 'G',
                        Some(TileKind::Built(BuiltTile::Lever { .. })) => '/',
                        Some(TileKind::Built(BuiltTile::PressurePlate { .. })) => '_',
                        Some(TileKind::Built(BuiltTile::TombCache { .. })) => '$',
                        Some(TileKind::Built(BuiltTile::TombStairs)) => '>',
                        Some(TileKind::Built(BuiltTile::TombShrine { .. })) => 'A',
                        Some(TileKind::Built(BuiltTile::TombPillar(_))) => '|',
                        _ => '#',
                    }
                };
                text.push(glyph);
            }
            text.push('\n');
        }
        text
    }

    #[test]
    #[ignore = "debug: shows where the tombs bot sticks in side-view runs"]
    fn print_where_the_bot_sticks() {
        for (seed, tier, armed) in [
            (777_777_u64, 1_u8, true),
            (777_777, 1, false),
            (11, 2, false),
            (4_242, 2, false),
            (4_242, 3, false),
            (90_001, 3, false),
            (11, 3, false),
            (90_001, 4, true),
            (90_001, 5, true),
            (11, 4, false),
            (11, 5, false),
            (4_242, 5, false),
        ] {
            let settings = WorldSettings {
                generator: GeneratorVersion::V10,
                ..WorldSettings::new(seed)
            };
            let Some(site) = super::super::tombs::find_site(&settings, SiteStyle::SideView, tier)
            else {
                continue;
            };
            let player = PlayerId::new(0);
            let mut world =
                WorldState::new_game_with_settings(player, &settings, SimulationParams::new(60));
            world.set_enemy_difficulty(if armed {
                EnemyDifficulty::Normal
            } else {
                EnemyDifficulty::Peaceful
            });
            if let Some(rig) = world.player_mut(player) {
                rig.pilot_gear.suit = 3;
                rig.pilot_gear.armor = 3;
                rig.pilot_gear.jetpack = 1;
                let _ = rig.pilot_arsenal.receive(crate::WeaponKind::Rifle);
                rig.pilot_arsenal.equipped = Some(crate::WeaponKind::Rifle);
                let _ = rig.pilot_arsenal.add_ammo(
                    crate::weapon::AmmoKind::Cartridge,
                    crate::pilot::arsenal::MAX_PILOT_AMMO,
                );
            }
            let _ = world.place_pilot(player, 0.0, 0.0);
            let _ = world.scenario_enter_tomb(&site, &[player]);
            let mut bot = super::super::tombs::TombBot::new(player, armed);
            let mut best = (0_u8, f32::MIN, 0_u32);
            for tick in 0..60 * 60 * 4 {
                let Some(run) = world.tomb_run_of(player) else {
                    println!("{seed} t{tier}: run over at tick {tick}");
                    break;
                };
                if let Some((_, body, _)) = world.pilot_of(player)
                    && (run.floor > best.0 || body.x > best.1 + 8.0)
                {
                    best = (run.floor, body.x, tick);
                }
                if tick - best.2 > 60 * 40 {
                    println!(
                        "{seed} t{tier} stuck on floor {}:\n{}",
                        run.floor,
                        around(&world, run)
                    );
                    break;
                }
                let inputs = bot.inputs(&world);
                let now = world.simulation_tick();
                let output = world.simulate_tick(now, &inputs);
                for event in output.events {
                    if let crate::GameplayEventKind::MechanismUsed {
                        x, y, mechanism, ..
                    } = event.kind
                    {
                        let origin = tomb_sites::slot_origin(0);
                        println!(
                            "  tick {tick}: {mechanism:?} at {:?}",
                            (x - origin.x, y - origin.y)
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "debug: where an armed bot loses health in side-view runs (Normal enemies)"]
    fn print_what_hurts_the_bot() {
        for seed in [11_u64, 4_242, 90_001] {
            for tier in 1..=5_u8 {
                let settings = WorldSettings {
                    generator: GeneratorVersion::V10,
                    ..WorldSettings::new(seed)
                };
                let Some(site) =
                    super::super::tombs::find_site(&settings, SiteStyle::SideView, tier)
                else {
                    continue;
                };
                let player = PlayerId::new(0);
                let mut world = WorldState::new_game_with_settings(
                    player,
                    &settings,
                    SimulationParams::new(60),
                );
                world.set_enemy_difficulty(EnemyDifficulty::Normal);
                if let Some(rig) = world.player_mut(player) {
                    let kit = tier.saturating_sub(1).min(3);
                    rig.pilot_gear.suit = kit;
                    rig.pilot_gear.armor = kit;
                    rig.pilot_gear.jetpack = 1;
                    let _ = rig.pilot_arsenal.receive(crate::WeaponKind::Rifle);
                    rig.pilot_arsenal.equipped = Some(crate::WeaponKind::Rifle);
                    let _ = rig.pilot_arsenal.add_ammo(
                        crate::weapon::AmmoKind::Cartridge,
                        crate::pilot::arsenal::MAX_PILOT_AMMO,
                    );
                }
                let _ = world.place_pilot(player, 0.0, 0.0);
                let _ = world.scenario_enter_tomb(&site, &[player]);
                let mut bot = super::super::tombs::TombBot::new(player, true);
                let mut hurt: std::collections::BTreeMap<String, f32> =
                    std::collections::BTreeMap::new();
                let mut last = None;
                let mut end = String::from("timeout");
                for _ in 0..60 * 60 * 8 {
                    let Some(run) = world.tomb_run_of(player) else {
                        break;
                    };
                    let floor = run.floor;
                    let room = world.pilot_of(player).and_then(|(_, body, state)| {
                        let (x, y) = state.middle(body);
                        let tile = world.frame().tile_at(x, y);
                        run.rooms
                            .iter()
                            .find(|room| room.interior.contains(tile))
                            .map(|room| room.kind)
                    });
                    let near_enemy = world.pilot_of(player).is_some_and(|(_, body, _)| {
                        !world
                            .enemies_near(body.x, body.y - 20.0, TILE_SIZE * 3.0)
                            .is_empty()
                    });
                    let inputs = bot.inputs(&world);
                    let now = world.simulation_tick();
                    let output = world.simulate_tick(now, &inputs);
                    for event in &output.events {
                        if let crate::GameplayEventKind::TombRunEnded { outcome, .. } = event.kind {
                            end = format!("{outcome:?} on floor {floor} in {room:?}");
                        }
                    }
                    let health = world.pilot_of(player).map(|(_, _, state)| state.health);
                    if let (Some(before), Some(after)) = (last, health)
                        && after < before
                    {
                        let cause = if near_enemy { "enemy" } else { "trap/shot" };
                        *hurt.entry(format!("{cause} in {room:?}")).or_default() += before - after;
                    }
                    last = health;
                }
                println!("{seed} t{tier}: {end}; hurt {hurt:?}");
            }
        }
    }
}
