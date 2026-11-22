//! Cave-ecosystem gameplay (WP-CAVES, [`crate::cave`]): discovering caves, falling
//! stalactites, opening cave caches and keeping each cave's critters around nearby rigs.
//!
//! Everything here runs inside the tick and only on worlds with cave ecosystems
//! ([`crate::worldgen::GeneratorVersion::has_cave_biomes`]); older worlds never get here.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    cave::{CaveTile, CritterKind, cache_loot},
    entity::{Body, CritterState, EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    hazard::FallingBoulder,
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{
        FeatureId, TileRect,
        noise::hash2,
        streamed::{BiomeCell, CaveBiome},
    },
};

use super::WorldState;

/// Rows above a rig a stalactite notices it from.
pub const STALACTITE_TRIGGER_ROWS: i32 = 7;
/// Warning before a loosened stalactite drops (it cracks first).
pub const STALACTITE_WARNING_MILLIS: u64 = 450;
/// Lifetime of a falling stalactite (it shatters on landing long before).
pub const STALACTITE_LIFE_MILLIS: u64 = 3_000;
/// Speed a stalactite starts falling at (px/s): faster than a loose boulder.
pub const STALACTITE_FALL_SPEED: f32 = 240.0;
/// How often each cave's critter population is refreshed.
pub const CRITTER_REFRESH_MILLIS: u64 = 500;
/// Caves within this many tiles (columns, rows) of a rig keep their critters.
pub const CRITTER_SPAWN_RANGE: (i32, i32) = (40, 28);
/// Critters whose home is farther than this many tiles from every rig leave.
pub const CRITTER_DESPAWN_RANGE: f32 = 64.0;
/// Most critters one cave holds.
pub const MAX_CRITTERS_PER_CAVE: usize = 8;

/// One stalactite in this many is loose enough to fall on a rig passing under it (a fixed roll
/// per tile); the rest are part of the cave's ceiling for good.
pub const STALACTITE_LOOSE_ODDS: u64 = 3;

const SALT_CRITTER: u64 = 0xC217_7E25;
const SALT_STALACTITE: u64 = 0x57A1_AC71;

/// Whether the stalactite at `tile` on `seed`'s world is a loose one (see
/// [`STALACTITE_LOOSE_ODDS`]).
#[must_use]
pub const fn stalactite_is_loose(seed: u64, tile: TilePosition) -> bool {
    hash2(seed, SALT_STALACTITE, tile.x, tile.y).is_multiple_of(STALACTITE_LOOSE_ODDS)
}

impl WorldState {
    /// Records caves rigs enter for the first time ("Found: Crystal Geode").
    pub(super) fn track_cave_discoveries(&mut self, output: &mut TickOutput) {
        let frame = self.frame();
        if !self.terrain.settings().generator.has_cave_biomes() {
            return;
        }
        for (player_id, player) in &mut self.players {
            let tile = player.tile_in(frame);
            let Some(sample) = self.terrain.cave_at(tile) else {
                continue;
            };
            if !sample.interior || player.journal.caves.contains_key(&sample.feature) {
                continue;
            }
            player.journal.caves.insert(sample.feature, sample.kind);
            output.push_event(GameplayEventKind::CaveDiscovered {
                player_id: *player_id,
                feature: sample.feature,
                cave: sample.kind,
            });
        }
    }

    /// Breaks off a loose stalactite ([`stalactite_is_loose`]) hanging over a rig with nothing in
    /// between, on unsettled ground: it cracks, then falls (a falling boulder that shatters on
    /// the rig or the floor).
    pub(super) fn trigger_stalactites(&mut self, output: &mut TickOutput) {
        if !self.terrain.settings().generator.has_cave_biomes() {
            return;
        }
        let rigs: Vec<TilePosition> = self
            .players
            .values()
            .map(|player| player.tile_in(self.frame()))
            .collect();
        let seed = self.terrain.seed();
        let mut loosened = BTreeSet::new();
        for rig in rigs {
            for dx in -1..=1 {
                for rise in 1..=STALACTITE_TRIGGER_ROWS {
                    let tile = TilePosition {
                        x: rig.x + dx,
                        y: rig.y - rise,
                    };
                    let Some(found) = self.terrain.tile(tile) else {
                        break;
                    };
                    if found.kind == TileKind::Cave(CaveTile::Stalactite) {
                        if stalactite_is_loose(seed, tile) {
                            loosened.insert(tile);
                        }
                        break;
                    }
                    if found.kind != TileKind::Air {
                        break;
                    }
                }
            }
        }
        for tile in loosened {
            // Each break settles the ground around it for a while (`world/hazards.rs`), so a
            // ceiling full of stalactites drops one at a time, not in a shower.
            if !self.loosen_ground(tile) || !self.terrain.set_kind(tile, TileKind::Air) {
                continue;
            }
            #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
            let (x, y) = (
                (self.frame().rel_col(tile.x) + 0.5) * TILE_SIZE,
                (self.frame().rel_row(tile.y) + 0.5) * TILE_SIZE,
            );
            let rate = self.timing.rate;
            let warning = rate.ticks_for_millis(STALACTITE_WARNING_MILLIS);
            let life = rate.ticks_for_millis(STALACTITE_LIFE_MILLIS);
            let now = self.simulation_tick.get();
            self.spawn_boulder(
                FallingBoulder {
                    x,
                    y,
                    velocity_y: STALACTITE_FALL_SPEED,
                    warning_seconds: rate.seconds_for_ticks(warning),
                    life: rate.seconds_for_ticks(life),
                },
                SimulationTick::new(now.saturating_add(warning)),
                SimulationTick::new(now.saturating_add(life)),
            );
            output.push_event(GameplayEventKind::StalactiteFell {
                x: tile.x,
                y: tile.y,
            });
            output.push_event(GameplayEventKind::BouldersChanged);
        }
    }

    /// A player drilled a cave cache open: credits to them, the loot spilled as item drops
    /// (WP-DROPS collects them).
    pub(super) fn open_cave_cache(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        tile: CaveTile,
        output: &mut TickOutput,
    ) {
        let band = self.terrain.band_at(target);
        let loot = cache_loot(self.terrain.seed(), target, band, tile);
        if let Some(player) = self.players.get_mut(&player_id) {
            player.credits = player.credits.saturating_add(loot.credits);
        }
        #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
        let (x, y) = (
            (self.frame().rel_col(target.x) + 0.5) * TILE_SIZE,
            (self.frame().rel_row(target.y) + 0.5) * TILE_SIZE,
        );
        for (index, stack) in loot.items.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "a handful of stacks")]
            let spread = (index as f32 - 0.5) * 60.0;
            // Through the drop rules (WP-DROPS): the kind's lifetime and the per-cell cap.
            let _ = self.spawn_loot(x, y, (spread, -150.0), *stack, None);
        }
        // An antique piece of furniture for the home (WP-HOME), straight into the stock.
        let furniture = crate::home::decor::cache_find(
            self.terrain.seed(),
            target.x,
            target.y,
            tile == CaveTile::RelicCache,
        );
        if let Some(piece) = furniture {
            self.grant_furniture(player_id, piece, 1);
        }
        output.push_event(GameplayEventKind::CaveCacheOpened {
            player_id,
            tile,
            credits: loot.credits,
            furniture,
        });
    }

    /// Keeps critters in the caves near rigs and sends away those no rig is near (every
    /// [`CRITTER_REFRESH_MILLIS`]).
    pub(super) fn advance_critters(&mut self, next_tick: SimulationTick) {
        if !self.terrain.settings().generator.has_cave_biomes() {
            return;
        }
        let period = self.timing.ticks_for_millis(CRITTER_REFRESH_MILLIS).max(1);
        if !next_tick.get().is_multiple_of(period) {
            return;
        }
        let rigs: Vec<(f32, f32)> = self
            .players
            .values()
            .map(|player| (player.x, player.y))
            .collect();
        let reach = CRITTER_DESPAWN_RANGE * TILE_SIZE;
        let mut leaving: BTreeSet<EntityId> = BTreeSet::new();
        let mut present: BTreeSet<(FeatureId, u8)> = BTreeSet::new();
        for entity in self.entities.of_kind(EntityKind::Critter) {
            let EntityState::Critter(critter) = entity.state else {
                continue;
            };
            let near = rigs.iter().any(|(x, y)| {
                (entity.body.x - x).abs() <= reach && (entity.body.y - y).abs() <= reach
            });
            if near {
                present.insert((critter.cave, critter.slot));
            } else {
                leaving.insert(entity.id);
            }
        }
        if !leaving.is_empty() {
            self.entities.despawn_many(&leaving);
        }
        // Caves near any rig, in id order.
        let mut caves: BTreeMap<FeatureId, std::sync::Arc<crate::worldgen::streamed::CaveRegion>> =
            BTreeMap::new();
        for player in self.players.values() {
            let tile = player.tile_in(self.frame());
            let rect = TileRect::around(tile, CRITTER_SPAWN_RANGE.0, CRITTER_SPAWN_RANGE.1);
            for region in self.terrain.caves_touching(&rect) {
                if let Some(biome) = region.biome.as_ref() {
                    caves.insert(biome.feature.id, std::sync::Arc::clone(&region));
                }
            }
        }
        for region in caves.values() {
            let Some(biome) = region.biome.as_ref() else {
                continue;
            };
            let missing: Vec<u8> = (0..critter_count(biome))
                .filter(|slot| !present.contains(&(biome.feature.id, *slot)))
                .collect();
            if missing.is_empty() {
                continue;
            }
            let homes = CritterHomes::of(biome);
            for slot in missing {
                let kinds = biome.kind.critters();
                let kind = kinds[usize::from(slot) % kinds.len()];
                let roll = hash2(
                    self.terrain.seed(),
                    SALT_CRITTER,
                    i32::try_from(biome.feature.id.0 & 0x7FFF_FFFF).unwrap_or(0),
                    i32::from(slot),
                );
                let Some((home, roam)) = homes.pick(kind, roll) else {
                    continue;
                };
                if self
                    .terrain
                    .tile(home)
                    .is_none_or(|tile| tile.kind != TileKind::Air)
                {
                    continue;
                }
                #[allow(clippy::cast_precision_loss, reason = "tile coordinates are small")]
                let (x, y) = (
                    (self.frame().rel_col(home.x) + 0.5) * TILE_SIZE,
                    (self.frame().rel_row(home.y) + 0.5) * TILE_SIZE,
                );
                let state = CritterState {
                    kind,
                    cave: biome.feature.id,
                    slot,
                    roam,
                    phase: u16::try_from((roll >> 20) & 0xFFFF).unwrap_or(0),
                };
                self.spawn_entity(Body::at(x, y), EntityState::Critter(state), None);
            }
        }
    }

    /// The critters near `(x, y)` (world pixels) within `radius` pixels, with their homes
    /// (presentation and tests).
    #[must_use]
    pub fn critters_near(
        &self,
        x: f32,
        y: f32,
        radius: f32,
    ) -> Vec<(EntityId, f32, f32, CritterState)> {
        self.entities
            .ids_in_radius(x, y, radius)
            .into_iter()
            .filter_map(|id| {
                let entity = self.entities.get(id)?;
                match entity.state {
                    EntityState::Critter(critter) => {
                        Some((id, entity.body.x, entity.body.y, critter))
                    }
                    _ => None,
                }
            })
            .collect()
    }
}

/// How many critters live in `biome`: one per 30 interior tiles, 3 to [`MAX_CRITTERS_PER_CAVE`].
fn critter_count(biome: &CaveBiome) -> u8 {
    let count = usize::try_from(biome.feature.size / 30)
        .unwrap_or(0)
        .clamp(3, MAX_CRITTERS_PER_CAVE);
    u8::try_from(count).unwrap_or(3)
}

/// Where a cave's critters can live: open air tiles and water tiles, each with the open
/// radius around it.
struct CritterHomes {
    air: Vec<(TilePosition, u16)>,
    water: Vec<(TilePosition, u16)>,
}

impl CritterHomes {
    fn of(biome: &CaveBiome) -> Self {
        let mut homes = Self {
            air: Vec::new(),
            water: Vec::new(),
        };
        for (position, cell) in biome.cells() {
            let wanted = match cell {
                BiomeCell::Open => BiomeCell::Open,
                BiomeCell::Water => BiomeCell::Water,
                _ => continue,
            };
            // The open radius (in tiles, up to 3) of cells like this one around it.
            let mut radius = 0;
            for r in 1..=3 {
                let clear = (-r..=r).all(|dy| {
                    (-r..=r).all(|dx| biome.cell(position.x + dx, position.y + dy) == wanted)
                });
                if !clear {
                    break;
                }
                radius = r;
            }
            if radius == 0 {
                continue;
            }
            // About 0.8 of the clear radius, in pixels (the radius is at most 3 tiles).
            let roam = u16::try_from(radius * 25).unwrap_or(25);
            if wanted == BiomeCell::Water {
                homes.water.push((position, roam));
            } else {
                homes.air.push((position, roam));
            }
        }
        homes
    }

    fn pick(&self, kind: CritterKind, roll: u64) -> Option<(TilePosition, u16)> {
        let pool = if kind.aquatic() {
            &self.water
        } else {
            &self.air
        };
        if pool.is_empty() {
            return None;
        }
        let index = usize::try_from(roll % u64::try_from(pool.len()).unwrap_or(1)).unwrap_or(0);
        pool.get(index).copied()
    }
}
