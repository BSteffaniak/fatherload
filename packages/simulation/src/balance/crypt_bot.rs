//! The crypt half of the tombs probe's bot (Wave 5 T29): plays a top-down crypt floor.
//!
//! Each tick it picks a goal (a lever whose gate is shut, an unlit shrine, an unopened cache,
//! the crypt lord while it stands, then the stairs), finds a breadth-first path over the floor
//! that keeps off pits and cracked floors and, when it can, off plates and spikes or vents that
//! are warning or active, and walks it eight ways. It shoots the nearest creature in sight,
//! keeps its distance from the lord and from golems, rolls through lunges, slams and incoming
//! shots, and rolls out when it has been stuck a while. It is the same bot the world tests
//! clear crypts with (unarmed on Peaceful).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "the bot converts world pixels and tiles freely"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    ControlState, PlayerCommand, PlayerId, TickInputs, WeaponControl, WeaponKind, WorldState,
    base::BuiltTile,
    enemy::{EnemyKind, EnemyMode},
    entity::{EntityKind, EntityState},
    mechanism::Usable,
    pilot::{PilotAction, TOP_DOWN_FOOTPRINT},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    tomb::{
        TombRun,
        crypt::{CryptTrapKind, TrapStage},
    },
    weapon::AmmoKind,
};

/// Breadth-first search budget (tiles).
const SEARCH_NODES: usize = 4_000;

/// The crypt bot's memory between ticks.
#[derive(Clone, Debug, Default)]
pub struct CryptBot {
    use_cooldown: u32,
    jump_held: bool,
    stall: u32,
    last: (f32, f32),
    floor: u8,
    /// Fixtures it tried to use and could not (a lever that would not budge): skipped.
    given_up: BTreeSet<TilePosition>,
}

fn tile_at(x: f32, y: f32) -> TilePosition {
    TilePosition {
        x: (x / TILE_SIZE).floor() as i32,
        y: (y / TILE_SIZE).floor() as i32,
    }
}

fn centre(tile: TilePosition) -> (f32, f32) {
    (
        (tile.x as f32 + 0.5) * TILE_SIZE,
        (tile.y as f32 + 0.5) * TILE_SIZE,
    )
}

const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

impl CryptBot {
    /// This tick's inputs for `player` in crypt run `run` (`armed`: shoot creatures).
    #[allow(
        clippy::too_many_lines,
        reason = "the bot's whole decision in reading order"
    )]
    pub fn inputs(
        &mut self,
        world: &WorldState,
        run: &TombRun,
        player: PlayerId,
        armed: bool,
    ) -> TickInputs {
        let mut inputs = TickInputs::default();
        if run.floor != self.floor {
            self.floor = run.floor;
            self.given_up.clear();
            self.stall = 0;
        }
        let Some((_, body, state)) = world.pilot_of(player) else {
            return inputs;
        };
        let (x, feet) = (body.x, body.y);
        let middle = (x, feet - TOP_DOWN_FOOTPRINT * 0.5);
        let here = tile_at(middle.0, middle.1);
        self.use_cooldown = self.use_cooldown.saturating_sub(1);
        if (x - self.last.0).hypot(feet - self.last.1) < 0.5 {
            self.stall += 1;
        } else {
            self.stall = 0;
        }
        self.last = (x, feet);

        // Use what is in reach and wanted.
        if self.use_cooldown == 0
            && let Some((tile, usable)) = world.pilot_usable(player)
        {
            let wanted = match usable {
                Usable::Lever { on, .. } => !on,
                Usable::TombShrine { lit } => !lit,
                Usable::TombCache => true,
                Usable::TombStairs => !run.on_last_floor() || run.boss_defeated,
                _ => false,
            };
            if wanted && !self.given_up.contains(&tile) {
                inputs.act_as(player, PlayerCommand::Pilot(PilotAction::Use));
                self.use_cooldown = 20;
                if matches!(usable, Usable::Lever { .. }) {
                    // Count a lever as done once pulled (a second try would close the gate).
                    self.given_up.insert(tile);
                }
            }
        }

        let millis = world.crypt_floor_millis(run.id);
        let terrain = world.terrain();
        let blocked = |tile: TilePosition| crate::world::pilot::pilot_blocks(terrain, tile);
        let trap = |tile: TilePosition| run.crypt.trap_at(tile).copied();
        let deadly = |tile: TilePosition| {
            trap(tile).is_some_and(|trap| {
                matches!(trap.kind, CryptTrapKind::Pit | CryptTrapKind::CrackedFloor)
            })
        };
        let risky = |tile: TilePosition| {
            trap(tile).is_some_and(|trap| trap.kind == CryptTrapKind::ArrowPlate)
        };
        // A cycling trap that is biting, or about to.
        let hot = |tile: TilePosition| {
            trap(tile).is_some_and(|trap| {
                matches!(trap.kind, CryptTrapKind::Spikes | CryptTrapKind::FlameJet)
                    && (trap.stage(millis) != TrapStage::Idle
                        || trap.stage(millis + 300) != TrapStage::Idle)
            })
        };

        // Goals: fixtures to stand beside, by priority.
        let mut levers = Vec::new();
        let mut shrines = Vec::new();
        let mut caches = Vec::new();
        let mut stairs = Vec::new();
        for room in &run.rooms {
            for y in room.interior.min.y - 1..=room.interior.max.y + 1 {
                for tx in room.interior.min.x - 1..=room.interior.max.x + 1 {
                    let tile = TilePosition { x: tx, y };
                    if self.given_up.contains(&tile) {
                        continue;
                    }
                    match terrain.tile(tile).map(|found| found.kind) {
                        Some(TileKind::Built(BuiltTile::Lever { on: false, .. })) => {
                            levers.push(tile);
                        }
                        Some(TileKind::Built(BuiltTile::TombShrine { lit: false })) => {
                            shrines.push(tile);
                        }
                        Some(TileKind::Built(BuiltTile::TombCache { open: false })) => {
                            caches.push(tile);
                        }
                        Some(TileKind::Built(BuiltTile::TombStairs)) => stairs.push(tile),
                        _ => {}
                    }
                }
            }
        }
        let boss = run.boss.and_then(|boss| {
            world
                .entities()
                .get(crate::entity::EntityId(boss))
                .map(|entity| (entity.body.x, entity.body.y))
        });
        let enemies: Vec<(f32, f32, EnemyKind, EnemyMode)> = world
            .enemies()
            .map(|(entity, enemy)| (entity.body.x, entity.body.y, enemy.kind, enemy.mode))
            .collect();
        let fighting_boss = run.on_last_floor() && !run.boss_defeated;

        // Two searches: carefully, then just alive.
        let path = [true, false].into_iter().find_map(|careful| {
            let avoid = |tile: TilePosition| deadly(tile) || (careful && risky(tile));
            let field = search(here, |tile| {
                !blocked(tile) && (tile == here || !avoid(tile))
            });
            let goal = |tiles: &[TilePosition]| {
                tiles
                    .iter()
                    .filter_map(|target| {
                        SIDES
                            .iter()
                            .map(|(dx, dy)| TilePosition {
                                x: target.x + dx,
                                y: target.y + dy,
                            })
                            .filter_map(|stand| field.get(&stand).map(|cost| (*cost, stand)))
                            .min()
                    })
                    .min()
                    .map(|(_, stand)| stand)
            };
            let target = goal(&levers)
                .or_else(|| goal(&shrines))
                .or_else(|| goal(&caches))
                .or_else(|| {
                    if fighting_boss {
                        boss.map(|(bx, by)| (tile_at(bx, by), (bx, by))).and_then(
                            |(tile, (bx, by))| {
                                // A spot about six tiles off the lord with a clear shot.
                                field
                                    .iter()
                                    .filter(|(stand, _)| {
                                        let (sx, sy) = centre(**stand);
                                        crate::physics::line_clear(terrain, sx, sy, bx, by)
                                    })
                                    .min_by_key(|(stand, cost)| {
                                        let (sx, sy) = centre(**stand);
                                        let off =
                                            ((sx - bx).hypot(sy - by) / TILE_SIZE - 6.0).abs();
                                        ((off * 2.0) as u32, **cost)
                                    })
                                    .map(|(stand, _)| *stand)
                                    .or(Some(tile))
                            },
                        )
                    } else {
                        None
                    }
                })
                .or_else(|| goal(&stairs))?;
            trace(&field, here, target, |tile| !blocked(tile))
        });

        let mut horizontal = 0.0;
        let (mut up, mut down) = (false, false);
        if let Some(path) = &path {
            // Wait on safe ground for the spikes or flames ahead to drop.
            let waiting = !hot(here) && path.iter().skip(1).take(2).any(|tile| hot(*tile));
            // Waiting: stand square on the safe tile (the footprint off the trap beside it).
            let next = if waiting {
                Some(&here)
            } else {
                path.get(1).or_else(|| path.first())
            };
            if let Some(next) = next {
                let (tx, ty) = centre(*next);
                let dx = tx - middle.0;
                let dy = ty - middle.1;
                if dx.abs() > 3.0 {
                    horizontal = dx.signum();
                }
                up = dy < -3.0;
                down = dy > 3.0;
            }
        } else if fighting_boss && let Some((bx, by)) = boss {
            // Keep a fighting distance from the lord.
            let (dx, dy) = (middle.0 - bx, middle.1 - by);
            if dx.hypot(dy) < TILE_SIZE * 4.5 {
                horizontal = dx.signum();
                up = dy < 0.0;
                down = dy > 0.0;
            }
        }

        // Rolls: through lunges, slams, the lord's charge and incoming shots, and out when
        // stuck; never into a pit.
        let mut roll = false;
        for (ex, ey, kind, mode) in &enemies {
            let distance = (ex - middle.0).hypot(ey - middle.1);
            let close = match kind {
                EnemyKind::CryptGolem => TILE_SIZE * 2.6,
                // The lord's charge runs the length of the hall: get off its line.
                EnemyKind::CryptLord => TILE_SIZE * 9.0,
                _ => TILE_SIZE * 2.2,
            };
            if *mode == EnemyMode::Windup && distance < close {
                roll = true;
                if *kind == EnemyKind::CryptLord {
                    // Sideways to the lord.
                    let (dx, dy) = (middle.0 - ex, middle.1 - ey);
                    horizontal = if dy.abs() > dx.abs() { 1.0 } else { 0.0 };
                    up = dx.abs() >= dy.abs();
                    down = false;
                } else {
                    // Away from it.
                    horizontal = (middle.0 - ex).signum();
                    up = middle.1 < *ey;
                    down = !up;
                }
            }
        }
        for shot in world.entities().of_kind(EntityKind::EnemyShot) {
            if let EntityState::EnemyShot(_) = shot.state {
                let (sx, sy) = (shot.body.x, shot.body.y);
                let (vx, vy) = (shot.body.velocity_x, shot.body.velocity_y);
                let (dx, dy) = (middle.0 - sx, middle.1 - sy);
                let distance = dx.hypot(dy);
                let speed = vx.hypot(vy).max(1.0);
                let towards = (dx * vx + dy * vy) / (distance.max(1.0) * speed);
                if distance < TILE_SIZE * 2.2 && towards > 0.85 {
                    roll = true;
                    // Sideways to the shot.
                    horizontal = if vy.abs() > vx.abs() { 1.0 } else { 0.0 };
                    up = vx.abs() >= vy.abs();
                    down = false;
                }
            }
        }
        if self.stall > 90 && self.stall.is_multiple_of(45) {
            roll = true;
            horizontal = if (self.stall / 45).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
        }
        if roll {
            // Only where the roll lands on safe floor (else the other way, else not at all).
            let vertical = f32::from(i8::from(down) - i8::from(up));
            let lands_safe = |dx: f32, dy: f32| {
                let length = dx.hypot(dy);
                if length < 0.1 {
                    return false;
                }
                let (ux, uy) = (dx / length, dy / length);
                let mut landing = here;
                for step in 1..=10 {
                    let reach = step as f32 * TILE_SIZE * 0.5;
                    let tile = tile_at(middle.0 + ux * reach, middle.1 + uy * reach);
                    if blocked(tile) {
                        break;
                    }
                    landing = tile;
                }
                !deadly(landing)
            };
            if !lands_safe(horizontal, vertical) {
                if lands_safe(-horizontal, -vertical) {
                    horizontal = -horizontal;
                    up = vertical > 0.0;
                    down = vertical < 0.0;
                } else {
                    roll = false;
                }
            }
        }
        let jump = roll && !self.jump_held;
        self.jump_held = jump;
        let mut controls = ControlState::new(horizontal, up, down).with_jump(jump);

        if armed {
            let target = enemies
                .iter()
                .map(|(ex, ey, ..)| (ex - middle.0, ey - middle.1))
                .filter(|(dx, dy)| {
                    dx.hypot(*dy) <= TILE_SIZE * 12.0
                        && crate::physics::line_clear(
                            terrain,
                            middle.0,
                            middle.1,
                            middle.0 + dx,
                            middle.1 + dy,
                        )
                })
                .min_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)));
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
            if let Some((dx, dy)) = target {
                let length = dx.hypot(dy).max(1.0);
                controls = controls.with_weapon(WeaponControl {
                    fire: true,
                    auto_aim: false,
                    aim_x: (dx / length * 127.0) as i8,
                    aim_y: (dy / length * 127.0) as i8,
                });
            }
        }
        let _ = state;
        inputs.hold_as(player, controls)
    }
}

/// A crypt floor as text around the run's rooms (`#` masonry, traps by initial, `@` the
/// pilot), for debugging the bot and the layout.
#[must_use]
pub fn floor_text(world: &WorldState, run: &TombRun, pilot: Option<TilePosition>) -> String {
    let (mut min, mut max) = (
        TilePosition {
            x: i32::MAX,
            y: i32::MAX,
        },
        TilePosition {
            x: i32::MIN,
            y: i32::MIN,
        },
    );
    for room in &run.rooms {
        min.x = min.x.min(room.interior.min.x - 1);
        min.y = min.y.min(room.interior.min.y - 1);
        max.x = max.x.max(room.interior.max.x + 1);
        max.y = max.y.max(room.interior.max.y + 1);
    }
    let millis = world.crypt_floor_millis(run.id);
    let mut text = String::new();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            let tile = TilePosition { x, y };
            if pilot == Some(tile) {
                text.push('@');
                continue;
            }
            if let Some(trap) = run.crypt.trap_at(tile) {
                let active = trap.stage(millis) == TrapStage::Active;
                text.push(match trap.kind {
                    CryptTrapKind::Spikes => {
                        if active {
                            'X'
                        } else {
                            'x'
                        }
                    }
                    CryptTrapKind::FlameJet => {
                        if active {
                            'F'
                        } else {
                            'f'
                        }
                    }
                    CryptTrapKind::ArrowPlate => 'a',
                    CryptTrapKind::BoulderChute => 'b',
                    CryptTrapKind::CrackedFloor => 'c',
                    CryptTrapKind::Pit => 'O',
                });
                continue;
            }
            text.push(match world.terrain().tile(tile).map(|found| found.kind) {
                Some(TileKind::Air) => '.',
                Some(TileKind::Built(BuiltTile::TombWall(_))) => '#',
                Some(TileKind::Built(BuiltTile::Gate)) => '|',
                Some(TileKind::Built(BuiltTile::GateLintel(_))) => '=',
                Some(TileKind::Built(BuiltTile::Lever { on, .. })) => {
                    if on {
                        'l'
                    } else {
                        'L'
                    }
                }
                Some(TileKind::Built(BuiltTile::TombShrine { .. })) => 'S',
                Some(TileKind::Built(BuiltTile::TombStairs)) => '>',
                Some(TileKind::Built(BuiltTile::TombExit)) => '<',
                Some(TileKind::Built(BuiltTile::TombCache { open })) => {
                    if open {
                        'o'
                    } else {
                        '$'
                    }
                }
                _ => ' ',
            });
        }
        text.push('\n');
    }
    text
}

/// Breadth-first distances from `from` over tiles `open` allows.
fn search(from: TilePosition, open: impl Fn(TilePosition) -> bool) -> BTreeMap<TilePosition, u32> {
    let mut seen = BTreeMap::from([(from, 0_u32)]);
    let mut queue = VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        if seen.len() >= SEARCH_NODES {
            break;
        }
        let cost = seen[&at];
        for (dx, dy) in SIDES {
            let next = TilePosition {
                x: at.x + dx,
                y: at.y + dy,
            };
            if !seen.contains_key(&next) && open(next) {
                seen.insert(next, cost + 1);
                queue.push_back(next);
            }
        }
    }
    seen
}

/// The path from `from` to `to` down the distance field (`from` first).
fn trace(
    field: &BTreeMap<TilePosition, u32>,
    from: TilePosition,
    to: TilePosition,
    open: impl Fn(TilePosition) -> bool,
) -> Option<Vec<TilePosition>> {
    let mut path = vec![to];
    let mut at = to;
    let mut cost = *field.get(&to)?;
    while at != from {
        let previous = SIDES
            .iter()
            .map(|(dx, dy)| TilePosition {
                x: at.x + dx,
                y: at.y + dy,
            })
            .find(|tile| open(*tile) && field.get(tile) == Some(&(cost.saturating_sub(1))))?;
        at = previous;
        cost = cost.saturating_sub(1);
        path.push(at);
    }
    path.reverse();
    Some(path)
}
