//! The distortion strata in the world (Wave 9, Descent VI part A; the rules are in
//! [`crate::distortion`]).
//!
//! [`WorldState::advance_distortion`] runs once per tick after the creatures move (beside the
//! Living Dark's pass):
//!
//! 1. Holds, pulses and hall cooldowns past their tick are dropped.
//! 2. For every rig near the distortion strata: crossing into or out of a mirror hall or a
//!    turned shear field (a cue each way), a deeper rot tier than it has seen, a landmark the
//!    crew has not found (its story beat), and a mirror twin stepping out of the glass when the
//!    rig is well inside a hall that is not resting.
//! 3. Every shot a rig fired this tick (`WeaponFired`) is copied by its twin, mirrored.
//!
//! Drilling hooks (`distortion_dig_extras`, from the drill's dig extras): real mirror ore pays
//! mythril, a reflection shatters and nicks the hull, and the last tile of a data ore block
//! recovers a data core. A dead pixel's pulse rots a rig's view ([`WorldState::rot_burst`]);
//! a gazer's gaze mirrors a rig's controls (`world/enemies/distortion.rs`, read by
//! `simulate_tick` through [`WorldState::distortion_gazed`]).

use crate::{
    descent::fields::cached_layout,
    distortion::{
        DATA_CORE_MYTHRIL, DATA_CORE_RESOURCES, Distortion, DistortionEvent, DistortionLandmark,
        GHOST_SHATTER_DAMAGE, MIRROR_ORE_MYTHRIL, ROT_BURST_MILLIS, TWIN_AXIS_CLEARANCE_TILES,
        TWIN_COOLDOWN_MILLIS, TWIN_SPAWN_WEIGHT, WarpedTile, fields::mirror_zone_weight, mirror,
        rot, shear,
    },
    enemy::{EnemyKind, EnemyMode},
    entity::{EntityId, EntityKind, EntityState},
    event::{GameplayEventKind, TickOutput},
    field::{FieldSample, ScreenEffect},
    ids::{PlayerId, SimulationTick},
    input::ControlState,
    item::{ItemKind, ItemStack},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
    worldgen::TileRect,
};

use super::{WorldState, enemies::distortion_event};

/// A rotted view's extra pixelation, palette collapse and glitch (levels, `0..=1`).
pub const ROT_BURST_SCREEN: (f32, f32, f32) = (0.55, 0.5, 0.6);
/// A gazed rig's screen mirrors with its controls (level).
pub const GAZE_SCREEN: f32 = 1.0;
/// A rig counts as inside a turned shear field past this weight (cues only).
const SHEAR_CUE_WEIGHT: f32 = 0.5;

impl WorldState {
    /// The distortion strata's shared state (Wave 9).
    #[must_use]
    pub const fn distortion(&self) -> &Distortion {
        &self.distortion
    }

    /// Marks every distortion landmark found before play (scenes and tools: their story cards
    /// then stay out of the picture).
    pub fn mark_distortion_landmarks_found(&mut self) {
        for landmark in DistortionLandmark::ALL {
            let _ = self.distortion.find(landmark);
        }
    }

    /// Whether a gazer holds `player_id`'s controls now.
    #[must_use]
    pub fn distortion_gazed(&self, player_id: PlayerId) -> bool {
        self.distortion
            .gazed_at(player_id, self.simulation_tick.next())
    }

    /// `controls` mirrored for `player_id`'s rig: the stick and the rotation thrusters swap,
    /// and so does its aim, since the seat's screen is mirrored too (a mouse or stick aimed at
    /// the screen's right points at the world's left).
    #[must_use]
    pub const fn mirror_controls(
        &self,
        player_id: PlayerId,
        controls: ControlState,
    ) -> ControlState {
        let _ = player_id;
        let mut mirrored = controls.mirrored();
        mirrored.weapon.aim_x = mirrored.weapon.aim_x.saturating_neg();
        mirrored
    }

    /// `fields` (the seat's view fields at its rig) with the distortion strata's per-rig screen
    /// on top: a gazed rig's screen mirrors, a rotted rig's view pixelates, loses colours and
    /// glitches. Presentation (each seat's post-process stack).
    #[must_use]
    pub fn distortion_view_fields(&self, player_id: PlayerId, fields: FieldSample) -> FieldSample {
        let now = self.simulation_tick.next();
        let mut fields = fields;
        if self.distortion.gazed_at(player_id, now) {
            fields.screen[ScreenEffect::Mirror.index()] = GAZE_SCREEN;
            fields.mirror = 1.0;
        }
        if self.distortion.rotted_at(player_id, now) {
            let (pixelate, palette, glitch) = ROT_BURST_SCREEN;
            let raise = |level: &mut f32, by: f32| *level = (*level + by).min(1.0);
            raise(&mut fields.screen[ScreenEffect::Pixelate.index()], pixelate);
            raise(&mut fields.screen[ScreenEffect::Palette.index()], palette);
            raise(&mut fields.screen[ScreenEffect::Glitch.index()], glitch);
        }
        fields
    }

    /// The mirror hall `player_id`'s rig is in and how far in (field weight), if any.
    #[must_use]
    pub fn mirror_hall_of(&self, player_id: PlayerId) -> Option<(mirror::MirrorZone, f32)> {
        let (x, y) = self.actor_position(player_id)?;
        let (column, row) = self.fractional_tile(x, y);
        mirror_zone_weight(self.terrain.settings(), column, row)
    }

    /// The rot tier at world pixel row `y` (`None` outside Pixel Rot or on older worlds).
    #[must_use]
    pub fn rot_tier_at(&self, y: f32) -> Option<u8> {
        let settings = self.terrain.settings();
        if !crate::distortion::has_distortion(settings) {
            return None;
        }
        let (_, row) = self.fractional_tile(0.0, y);
        #[allow(clippy::cast_possible_truncation, reason = "tile rows fit i32")]
        rot::tier_at(&cached_layout(settings), row.floor() as i32)
    }

    /// Fractional absolute tile coordinates of local pixel `(x, y)`.
    fn fractional_tile(&self, x: f32, y: f32) -> (f32, f32) {
        let frame = self.frame();
        #[allow(clippy::cast_precision_loss, reason = "frame origins are small")]
        (
            frame.origin_col() as f32 + x / TILE_SIZE,
            frame.origin_row() as f32 + y / TILE_SIZE,
        )
    }

    /// A dead pixel's pulse hit `player_id`: its view rots for [`ROT_BURST_MILLIS`].
    pub(super) fn rot_burst(
        &mut self,
        player_id: PlayerId,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let until = self.after(now, ROT_BURST_MILLIS);
        self.distortion.rotted.insert(player_id, until);
        if let Some(tile) = self
            .players
            .get(&player_id)
            .map(|rig| rig.tile_in(self.frame()))
        {
            output.push_event(distortion_event(
                DistortionEvent::RotBurst,
                Some(player_id),
                tile,
            ));
        }
    }

    /// What drilling `mined` out of `target` adds in the distortion strata (see the module
    /// docs).
    pub(super) fn distortion_dig_extras(
        &mut self,
        player_id: PlayerId,
        target: TilePosition,
        mined: TileKind,
    ) {
        let TileKind::Warped(tile) = mined else {
            return;
        };
        let (x, y) = self.frame().tile_center(target);
        match tile {
            WarpedTile::MirrorOre => {
                let _ = self.spawn_loot(
                    x,
                    y,
                    (40.0, -80.0),
                    ItemStack::new(ItemKind::Mineral(MineralKind::Mythril), MIRROR_ORE_MYTHRIL),
                    None,
                );
            }
            WarpedTile::MirrorGhost => {
                let tier = self.tier_at(target);
                let damage = GHOST_SHATTER_DAMAGE * crate::enemy::tier_scale(tier);
                if !self.damage_immune() {
                    let _ = self.damage_player(player_id, damage);
                }
            }
            _ => {}
        }
    }

    /// Reads the tiles this tick opened: mirror ore and reflections broken (cues), data ore
    /// blocks mined out (a data core each, whoever or whatever broke the last tile).
    fn note_warped_openings(&mut self, output: &mut TickOutput) {
        let opened: Vec<(TilePosition, WarpedTile)> = self
            .terrain
            .journal_with_before()
            .filter_map(|(change, before)| match before {
                TileKind::Warped(tile) if change.tile.kind.is_open() => {
                    Some((change.position, tile))
                }
                _ => None,
            })
            .collect();
        for (position, tile) in opened {
            let who = self.nearest_rig(position);
            match tile {
                WarpedTile::MirrorOre => output.push_event(distortion_event(
                    DistortionEvent::MirrorOreMined,
                    who,
                    position,
                )),
                WarpedTile::MirrorGhost => output.push_event(distortion_event(
                    DistortionEvent::GhostShattered,
                    who,
                    position,
                )),
                WarpedTile::DataOre if self.data_block_complete(position) => {
                    let (x, y) = self.frame().tile_center(position);
                    let _ = self.spawn_loot(
                        x,
                        y,
                        (-40.0, -100.0),
                        ItemStack::new(
                            ItemKind::Layer(crate::descent::Stratum::PixelRot),
                            DATA_CORE_RESOURCES,
                        ),
                        None,
                    );
                    let _ = self.spawn_loot(
                        x,
                        y,
                        (40.0, -100.0),
                        ItemStack::new(ItemKind::Mineral(MineralKind::Mythril), DATA_CORE_MYTHRIL),
                        None,
                    );
                    output.push_event(distortion_event(DistortionEvent::DataCore, who, position));
                }
                _ => {}
            }
        }
    }

    /// The rig nearest `tile` (within a dozen tiles), if any.
    fn nearest_rig(&self, tile: TilePosition) -> Option<PlayerId> {
        let (x, y) = self.frame().tile_center(tile);
        self.players
            .iter()
            .map(|(id, rig)| ((rig.x - x).hypot(rig.y - y), *id))
            .filter(|(distance, _)| *distance <= TILE_SIZE * 12.0)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id)
    }

    /// Whether the data ore block holding `target` is now mined out (every tile of it open).
    /// Single-tile specks of the top tier are not blocks.
    fn data_block_complete(&self, target: TilePosition) -> bool {
        let settings = *self.terrain.settings();
        let layout = cached_layout(&settings);
        let Some(cell) = rot::block_at(&layout, target.x, target.y) else {
            return false;
        };
        if cell.tier == 0 || !rot::is_data_block(&settings, &cell) {
            return false;
        }
        cell.bounds.positions().all(|at| {
            self.terrain
                .tile(at)
                .is_some_and(|tile| tile.kind.is_open())
        })
    }

    /// Kills of the distortion strata's creatures: a twin's hall rests.
    pub(super) fn distortion_kill(
        &mut self,
        kind: EnemyKind,
        home: (i32, i32),
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        if kind != EnemyKind::MirrorTwin {
            return;
        }
        let until = self.after(self.simulation_tick.next(), TWIN_COOLDOWN_MILLIS);
        self.distortion.twin_cooldowns.insert(home, until);
        output.push_event(distortion_event(DistortionEvent::TwinBroken, by, tile));
    }

    // --- The tick -------------------------------------------------------------------------------

    /// One tick of the distortion strata (see the module docs).
    pub(super) fn advance_distortion(
        &mut self,
        next_tick: SimulationTick,
        output: &mut TickOutput,
    ) {
        let settings = *self.terrain.settings();
        if !crate::distortion::has_distortion(&settings) {
            return;
        }
        self.note_warped_openings(output);
        if !self.distortion.is_empty() {
            self.distortion.prune(next_tick);
        }
        let layout = cached_layout(&settings);
        let (first, last) = crate::distortion::distortion_rows(&layout);
        let frame = self.frame();
        let rigs: Vec<(PlayerId, f32, f32, TilePosition)> = self
            .players
            .iter()
            .filter(|(id, rig)| rig.hull > 0.0 && !self.is_on_foot(**id))
            .map(|(id, rig)| (*id, rig.x, rig.y, rig.tile_in(frame)))
            .filter(|(.., tile)| tile.y >= first - 12 && tile.y <= last + 12)
            .collect();
        for (player_id, x, y, tile) in rigs {
            self.note_mirror(player_id, x, y, tile, next_tick, output);
            self.note_shear(player_id, x, y, tile, output);
            self.note_rot(player_id, tile, &layout, output);
            self.note_landmarks(player_id, tile, &layout, output);
        }
        self.copy_shots_to_twins(output);
    }

    /// Mirror halls: the cues in and out, and a twin stepping out of the glass.
    fn note_mirror(
        &mut self,
        player_id: PlayerId,
        x: f32,
        y: f32,
        tile: TilePosition,
        now: SimulationTick,
        output: &mut TickOutput,
    ) {
        let (column, row) = self.fractional_tile(x, y);
        let hall = mirror_zone_weight(self.terrain.settings(), column, row);
        let inside = hall.is_some_and(|(_, weight)| weight > 0.5);
        let was_inside = self.distortion.in_mirror.contains(&player_id);
        if inside != was_inside {
            if inside {
                self.distortion.in_mirror.insert(player_id);
            } else {
                self.distortion.in_mirror.remove(&player_id);
            }
            let what = if inside {
                DistortionEvent::MirrorEntered
            } else {
                DistortionEvent::MirrorLeft
            };
            output.push_event(distortion_event(what, Some(player_id), tile));
        }
        let Some((zone, weight)) = hall else {
            return;
        };
        if weight < TWIN_SPAWN_WEIGHT
            || !self.enemies.difficulty.hostile()
            || self.distortion.hall_resting(zone.key(), now)
            || (column - zone.axis()).abs() < TWIN_AXIS_CLEARANCE_TILES
            || self.twin_of(player_id).is_some()
            || self.entities.count_of(EntityKind::Enemy) >= super::enemies::MAX_ENEMIES
        {
            return;
        }
        let frame = self.frame();
        let axis_px = (frame.col_px(zone.bounds.min.x) + frame.col_px(zone.bounds.max.x + 1)) * 0.5;
        let reflection = (2.0f32.mul_add(axis_px, -x), y);
        let radius = EnemyKind::MirrorTwin.stats().radius;
        if crate::physics::collides(&self.terrain, reflection.0, reflection.1, radius) {
            return;
        }
        let tier = self.tier_at(tile);
        let twin = self.spawn_enemy_with(EnemyKind::MirrorTwin, reflection, tier, None, None);
        self.entities.update(twin, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.home = zone.key();
                state.target = Some(player_id);
                state.mode = EnemyMode::Chase;
                state.last_seen = now;
            }
        });
        output.push_event(distortion_event(
            DistortionEvent::TwinAppeared,
            Some(player_id),
            frame.tile_at(reflection.0, reflection.1),
        ));
    }

    /// The twin copying `player_id`'s rig, if any.
    #[must_use]
    pub fn twin_of(&self, player_id: PlayerId) -> Option<EntityId> {
        self.enemies()
            .find(|(_, state)| {
                state.kind == EnemyKind::MirrorTwin && state.target == Some(player_id)
            })
            .map(|(entity, _)| entity.id)
    }

    /// Shear halls: a cue on crossing into a turned field.
    fn note_shear(
        &mut self,
        player_id: PlayerId,
        x: f32,
        y: f32,
        tile: TilePosition,
        output: &mut TickOutput,
    ) {
        let field = self.fields_at(x, y);
        let turned = field.gravity_y < 1.0 - SHEAR_CUE_WEIGHT
            && self.terrain.stratum_at(tile) == crate::descent::Stratum::GravityShear;
        let was = self.distortion.in_shear.contains(&player_id);
        if turned == was {
            return;
        }
        if turned {
            self.distortion.in_shear.insert(player_id);
            output.push_event(distortion_event(
                DistortionEvent::ShearEntered,
                Some(player_id),
                tile,
            ));
        } else {
            self.distortion.in_shear.remove(&player_id);
        }
    }

    /// Pixel Rot: a cue the first time a rig reaches each deeper tier.
    fn note_rot(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        layout: &crate::worldgen::descent::DescentLayout,
        output: &mut TickOutput,
    ) {
        let Some(tier) = rot::tier_at(layout, tile.y) else {
            return;
        };
        let seen = self.distortion.rot_tiers.get(&player_id).copied();
        if seen.is_some_and(|seen| seen > tier) {
            return;
        }
        // Stored one above the tier, so 0 means none seen.
        self.distortion.rot_tiers.insert(player_id, tier + 1);
        if tier > 0 {
            output.push_event(distortion_event(
                DistortionEvent::RotDeepened,
                Some(player_id),
                tile,
            ));
        }
    }

    /// The landmarks: the first rig inside each one finds it for the crew.
    fn note_landmarks(
        &mut self,
        player_id: PlayerId,
        tile: TilePosition,
        layout: &crate::worldgen::descent::DescentLayout,
        output: &mut TickOutput,
    ) {
        for landmark in DistortionLandmark::ALL {
            if self.distortion.has_found(landmark) {
                continue;
            }
            let Some(bounds) =
                distortion_landmark_bounds(self.terrain.settings(), layout, landmark)
            else {
                continue;
            };
            if bounds.contains(tile) && self.distortion.find(landmark) {
                output.push_event(GameplayEventKind::DistortionLandmarkFound {
                    landmark,
                    player_id,
                });
            }
        }
    }

    /// Every shot a rig fired this tick is copied by its twin, mirrored.
    fn copy_shots_to_twins(&mut self, output: &mut TickOutput) {
        let fired: Vec<(PlayerId, (f32, f32))> = output
            .events
            .iter()
            .filter_map(|event| match event.kind {
                GameplayEventKind::WeaponFired {
                    player_id,
                    x,
                    y,
                    end_x,
                    end_y,
                    ..
                } => {
                    #[allow(clippy::cast_precision_loss, reason = "bounded world pixels")]
                    let (dx, dy) = ((end_x - x) as f32, (end_y - y) as f32);
                    let length = dx.hypot(dy);
                    (length > 0.5).then_some((player_id, (dx / length, dy / length)))
                }
                _ => None,
            })
            .collect();
        for (player_id, direction) in fired {
            if let Some(twin) = self.twin_of(player_id) {
                self.twin_fires(twin, direction, output);
            }
        }
    }
}

/// The tiles of `landmark` (a rig inside them finds it), or `None` on worlds without it.
#[must_use]
pub fn distortion_landmark_bounds(
    settings: &crate::worldgen::WorldSettings,
    layout: &crate::worldgen::descent::DescentLayout,
    landmark: DistortionLandmark,
) -> Option<TileRect> {
    match landmark {
        DistortionLandmark::LookingGlassHall => {
            mirror::looking_glass_hall(settings, layout).map(|hall| hall.bounds)
        }
        DistortionLandmark::ParallaxStation => crate::worldgen::descent::landmarks::outpost(
            settings,
            layout,
            crate::descent::Descent::StrangeLayers,
        )
        .map(|hall| hall.bounds.expanded(2)),
        DistortionLandmark::ShearSpire => shear::spire(settings, layout),
        DistortionLandmark::DeadScreen => rot::dead_screen(settings, layout),
    }
}
