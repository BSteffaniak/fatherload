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
        }
    }

    /// Reads the floor once per floor (traps do not move; their reach is the open run in front
    /// of them).
    fn refresh(&mut self, world: &WorldState, run: &TombRun) {
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

    fn hits(seen: &Seen, millis: u64, body: &Bounds) -> bool {
        match seen.kind {
            TrapKind::Blade => body.touches_disc(blade_tip(seen.tile, millis), BLADE_RADIUS + 4.0),
            TrapKind::Crusher => {
                // A shaking crusher counts as down already: never step under one.
                let (stage, extension) = crusher_state(seen.tile, millis);
                let extension = if stage == CrusherStage::Shaking {
                    1.0
                } else {
                    extension
                };
                crusher_block(seen.tile, seen.reach, extension)
                    .is_some_and(|block| block.overlaps(body))
            }
            TrapKind::DartWall => dart_position(seen.tile, seen.facing, seen.reach, millis)
                .is_some_and(|at| dart_bounds(at).overlaps(body)),
            TrapKind::FogVent => fog_hurts(seen.tile, millis)
                .is_some_and(|radius| body.touches_disc(fog_centre(seen.tile), radius + 6.0)),
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
                let x = (seen.tile.x as f32 + 0.5) * TILE_SIZE;
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
            near.iter().any(|seen| Self::hits(seen, now + ahead, &body))
        })
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
        let under = TilePosition {
            x: (x / TILE_SIZE).floor() as i32,
            y: ((feet + 1.0) / TILE_SIZE).floor() as i32,
        };
        if matches!(
            world.terrain().tile(under).map(|found| found.kind),
            Some(TileKind::Built(BuiltTile::TombTrap { kind, .. }))
                if TrapKind::from_index(kind) == TrapKind::Crumble
        ) {
            return Advice::Go;
        }
        // A sluice basin ahead: wait on its edge for a long dry spell.
        let column = (x / TILE_SIZE).floor() as i32;
        let row = ((feet - 1.0) / TILE_SIZE).floor() as i32;
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
