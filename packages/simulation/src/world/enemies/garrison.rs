//! Descent VII part B's creatures (Wave 10 ancients-b): what sets them apart from the generic
//! state machine in `world/enemies.rs`, and the Gatekeeper Colossus's fight.
//!
//! * **The garrison knows** ([`WorldState::garrison_notice`]): a fort's machines know every rig
//!   holding their fort (no line of sight needed: the alarm is raised); the Colossus and its
//!   parts know every rig in its hall.
//! * **Parts** ([`WorldState::garrison_damage`]): the Colossus's head, fists and legs are hit
//!   areas drawn where they are; a hit on one passes to the core (the head is the weak point,
//!   the fists armour except while they rest after a strike).
//! * **The Gatekeeper Colossus** ([`WorldState::advance_colossus`]): wakes when a rig is in its
//!   hall. Stage one it guards the far end and slams its fists (each raised over a marked
//!   column first); at two thirds of its health the hall's cracked floor gives way (telegraphed
//!   for [`COLLAPSE_TELL_MILLIS`], immune, never onto a body) and stage two it stands in the
//!   crypt, sweeping its fists along the floor while the ceiling sheds stones (each marked by
//!   dust first); at a third the vault comes down, leaving rubble platforms, its legs fall away
//!   and stage three its core hovers, slamming faster and calling two war sentries. It sleeps
//!   and heals to its stage's top after [`COLOSSUS_SLEEP_MILLIS`] with nobody in its hall; the
//!   hall keeps its collapses. Its death gives every rig the Heart Key and opens its floor on
//!   the way down.

use crate::{
    depth_progression::RigSystem,
    enemy::{Damage, DamageOutcome, EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityId, EntityState},
    event::TickOutput,
    garrison::{
        COLLAPSE_TELL_MILLIS, COLOSSUS_ADDS, COLOSSUS_SLEEP_MILLIS, COLOSSUS_STANDING_DROP,
        ColossusPart, ColossusStage, FIST_PERIOD_MILLIS, FIST_REACH_TILES, FIST_REST_MILLIS,
        FIST_STRIKE_MILLIS, FIST_TELL_MILLIS, FistMove, FistMoveKind, FortPhase, GarrisonEvent,
        RUBBLE_TELL_MILLIS, RUBBLE_VOLLEY, SLAM_WAVE, SWEEP_DAMAGE, UNBOUND_CORE_SHARE,
    },
    ids::{PlayerId, SimulationTick},
    terrain::{TILE_SIZE, TileKind, TilePosition},
    worldgen::{TileRect, descent::garrison as sites},
};

use super::super::garrison::garrison_event;
use super::WorldState;

/// How far up the crypt's ceiling is looked for over a rig, tiles.
const CRYPT_CEILING_TILES: i32 = 34;

/// The enemy kind of `part`.
#[must_use]
pub const fn part_kind(part: ColossusPart) -> EnemyKind {
    match part {
        ColossusPart::Head => EnemyKind::ColossusHead,
        ColossusPart::LeftFist | ColossusPart::RightFist => EnemyKind::ColossusFist,
        ColossusPart::Legs => EnemyKind::ColossusLegs,
    }
}

/// Linear interpolation from `a` to `b` by `t` in `0..=1`.
fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    ((b.0 - a.0).mul_add(t, a.0), (b.1 - a.1).mul_add(t, a.1))
}

impl WorldState {
    /// Descent VII part B's say on who `state`'s creature notices: `Some(answer)` overrides the
    /// generic perception, `None` leaves it (see the module docs).
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn garrison_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        let frame = self.frame();
        let nearest_in = |area: TileRect| {
            self.players
                .iter()
                .filter(|(_, player)| player.hull > 0.0)
                .filter(|(_, player)| area.contains(player.tile_in(frame)))
                .map(|(id, player)| ((player.x - body.x).hypot(player.y - body.y), *id))
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                .map(|(_, id)| id)
        };
        match state.kind {
            EnemyKind::WarSentry | EnemyKind::SiegeWalker | EnemyKind::LancerDrone => {
                let home = TilePosition {
                    x: state.home.0,
                    y: state.home.1,
                };
                let fort = self
                    .garrison
                    .forts
                    .values()
                    .find(|fort| fort.awake() && fort.bounds.contains(home))?;
                Some(nearest_in(
                    fort.bounds.expanded(crate::garrison::FORT_REACH_TILES),
                ))
            }
            EnemyKind::GatekeeperColossus
            | EnemyKind::ColossusHead
            | EnemyKind::ColossusFist
            | EnemyKind::ColossusLegs => {
                let hall = self.colossus_hall()?;
                Some(nearest_in(sites::hall_inside(&hall).expanded(1)))
            }
            _ => None,
        }
    }

    /// Whether Descent VII part B lets `kind` spawn roaming (the Colossus and its parts only
    /// appear in its hall).
    pub(in crate::world) const fn garrison_spawn_allowed(kind: EnemyKind) -> bool {
        !matches!(
            kind,
            EnemyKind::GatekeeperColossus
                | EnemyKind::ColossusHead
                | EnemyKind::ColossusFist
                | EnemyKind::ColossusLegs
        )
    }

    /// Whether `state`'s creature shrugs off every hit now (the Colossus while its hall
    /// collapses).
    pub(in crate::world) const fn garrison_ignores_damage(&self, state: &EnemyState) -> bool {
        matches!(state.kind, EnemyKind::GatekeeperColossus) && self.garrison.colossus.collapsing()
    }

    /// The damage multiplier on `kind` (the unbound core's armour is gone).
    pub(in crate::world) fn garrison_damage_factor(&self, kind: EnemyKind) -> f32 {
        if kind == EnemyKind::GatekeeperColossus
            && self.garrison.colossus.stage == ColossusStage::Unbound
        {
            UNBOUND_CORE_SHARE
        } else {
            1.0
        }
    }

    /// A hit on one of the Colossus's parts passes to its core (scaled by the part's share).
    /// `Some(outcome)` decides the hit; `None` leaves it to the generic damage.
    pub(in crate::world) fn garrison_damage(
        &mut self,
        id: EntityId,
        state: &EnemyState,
        damage: Damage,
        output: &mut TickOutput,
    ) -> Option<DamageOutcome> {
        if !state.kind.is_colossus_part() {
            return None;
        }
        let colossus = &self.garrison.colossus;
        let Some(core) = colossus.entity else {
            return Some(DamageOutcome::Ignored);
        };
        let part = colossus
            .parts
            .iter()
            .find(|(_, entity)| *entity == id.get())
            .map_or(ColossusPart::Head, |(part, _)| *part);
        let now = self.simulation_tick;
        let resting = part.hand().is_some_and(|hand| {
            colossus
                .fist
                .is_some_and(|fist| fist.hand == hand && now >= fist.lands && now < fist.rests)
        });
        let share = part.damage_share(resting);
        let outcome = self.damage_enemy(
            EntityId(core),
            Damage {
                amount: damage.amount * share,
                ..damage
            },
            output,
        );
        Some(match outcome {
            DamageOutcome::Killed { dealt } => DamageOutcome::Hurt { dealt },
            other => other,
        })
    }

    /// A Descent VII part B creature died (kill credit `by`).
    pub(in crate::world) fn garrison_kill(
        &mut self,
        id: u64,
        kind: EnemyKind,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        for fort in self.garrison.forts.values_mut() {
            fort.machines.retain(|machine| *machine != id);
        }
        self.garrison.colossus.adds.retain(|add| *add != id);
        if kind == EnemyKind::GatekeeperColossus {
            self.colossus_fell(tile, by, output);
        }
    }

    // --- The Gatekeeper Colossus --------------------------------------------------------------

    /// The core's body and state while it is awake.
    fn colossus_parts(&self) -> Option<(EntityId, Body, EnemyState)> {
        let id = EntityId(self.garrison.colossus.entity?);
        let (body, state) = self.enemy_parts(id)?;
        Some((id, body, state))
    }

    /// Where the core's centre stands in `stage` before its sway (local pixels).
    fn colossus_home(&self, hall: &TileRect, stage: ColossusStage) -> (f32, f32) {
        let post = sites::colossus_post(self.terrain.settings(), hall, stage);
        let frame = self.frame();
        let x = frame.tile_center(post).0;
        let y = match stage {
            ColossusStage::Guard | ColossusStage::Fallen => {
                frame.row_px(post.y) - COLOSSUS_STANDING_DROP
            }
            ColossusStage::Unbound => frame.tile_center(post).1,
        };
        (x, y)
    }

    /// The living rigs in the hall: `(id, x, y, tile)`.
    fn rigs_in_hall(&self, hall: &TileRect) -> Vec<(PlayerId, f32, f32, TilePosition)> {
        let inside = sites::hall_inside(hall);
        let frame = self.frame();
        self.players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.x, player.y, player.tile_in(frame)))
            .filter(|(_, _, _, tile)| inside.contains(*tile))
            .collect()
    }

    /// Wakes the Colossus at the hall's present shape (its health that stage's top).
    fn wake_colossus(&mut self, hall: &TileRect, player: PlayerId, output: &mut TickOutput) {
        let now = self.simulation_tick;
        let stage = self.garrison.colossus.arena;
        let tier = self.tier_at(hall.center());
        let (x, y) = self.colossus_home(hall, stage);
        let core = self.spawn_enemy_with(EnemyKind::GatekeeperColossus, (x, y), tier, None, None);
        self.entities.update(core, |entity| {
            if let EntityState::Enemy(state) = &mut entity.state {
                state.health = (state.max_health * f32::from(stage.starts_at()) / 1_000.0 - 1.0)
                    .max(1.0)
                    .min(state.max_health);
                state.mode = EnemyMode::Chase;
                state.target = Some(player);
                state.last_seen = now;
                state.attack_ready = SimulationTick::new(
                    now.get()
                        .saturating_add(self.timing.ticks_for_millis(1_500).max(1)),
                );
            }
        });
        let mut parts = Vec::new();
        for part in ColossusPart::ALL {
            if part == ColossusPart::Legs && stage == ColossusStage::Unbound {
                continue;
            }
            let (dx, dy) = part.rest_offset();
            let id =
                self.spawn_enemy_with(part_kind(part), (x + dx, y + dy), tier, None, Some(core));
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.pattern = part.index();
                    state.mode = EnemyMode::Chase;
                    state.target = Some(player);
                }
            });
            parts.push((part, id.get()));
        }
        let colossus = &mut self.garrison.colossus;
        colossus.entity = Some(core.get());
        colossus.parts = parts;
        colossus.stage = stage;
        colossus.collapsing = None;
        colossus.fist = None;
        colossus.adds.clear();
        let first = self.after_millis(now, 2_000);
        self.garrison.colossus.next_fist = first;
        self.garrison.colossus.next_rubble = first;
        self.garrison.colossus.next_adds = self.after_millis(now, COLOSSUS_ADDS.1 / 2);
        output.push_event(garrison_event(
            GarrisonEvent::ColossusAwoke,
            Some(player),
            hall.center(),
        ));
    }

    /// One tick of the Colossus's fight (see the module docs).
    #[allow(clippy::too_many_lines, reason = "the fight's beats in order")]
    pub(in crate::world) fn advance_colossus(&mut self, output: &mut TickOutput) {
        if self.garrison.colossus.beaten {
            // The gate stays open for the whole crew: a rig that joins later gets the key too.
            for player in self.players.values_mut() {
                if !player.hazard_gear.has_system(RigSystem::HeartKey) {
                    crate::economy::grant_rig_system(player, RigSystem::HeartKey);
                }
            }
            return;
        }
        let Some(hall) = self.colossus_hall() else {
            return;
        };
        let now = self.simulation_tick;
        let rigs = self.rigs_in_hall(&hall);
        if !rigs.is_empty() {
            self.garrison.colossus.last_visited = now;
        }
        if self.garrison.colossus.entity.is_some() && self.colossus_parts().is_none() {
            self.put_colossus_to_sleep();
        }
        if !self.garrison.colossus.awake() {
            let Some((player, ..)) = rigs.first().copied() else {
                return;
            };
            // A peaceful world has no creatures: the hall yields its gate to the first rig.
            if !self.enemies.difficulty.hostile() {
                self.colossus_fell(hall.center(), Some(player), output);
                return;
            }
            self.wake_colossus(&hall, player, output);
            return;
        }
        let idle = now
            .get()
            .saturating_sub(self.garrison.colossus.last_visited.get());
        if rigs.is_empty() && idle > self.timing.ticks_for_millis(COLOSSUS_SLEEP_MILLIS) {
            self.put_colossus_to_sleep();
            return;
        }
        let Some((core, mut body, mut state)) = self.colossus_parts() else {
            return;
        };
        let centre = hall.center();
        // The hall's collapses: at each stage's end, telegraphed, then the hall gives way.
        if let Some((next, lands)) = self.garrison.colossus.collapsing {
            if now >= lands {
                self.collapse_hall(&hall, next);
                self.garrison.colossus.collapsing = None;
                self.garrison.colossus.stage = next;
                self.garrison.colossus.arena = next;
                self.garrison.colossus.next_fist = self.after_millis(now, 1_500);
                self.garrison.colossus.next_rubble = self.after_millis(now, 2_500);
                self.garrison.colossus.next_adds = self.after_millis(now, 2_500);
                if next == ColossusStage::Unbound {
                    let legs: Vec<u64> = self
                        .garrison
                        .colossus
                        .parts
                        .iter()
                        .filter(|(part, _)| *part == ColossusPart::Legs)
                        .map(|(_, id)| *id)
                        .collect();
                    for id in legs {
                        self.entities.despawn(EntityId(id));
                    }
                    self.garrison
                        .colossus
                        .parts
                        .retain(|(part, _)| *part != ColossusPart::Legs);
                }
                state.mode = EnemyMode::Chase;
                state.attack_ready = self.after_millis(now, 1_200);
                output.push_event(garrison_event(GarrisonEvent::HallCollapsed, None, centre));
            } else {
                state.mode = EnemyMode::Stunned;
                state.mode_until = lands;
            }
        } else if let Some(next) = self.garrison.colossus.stage.next() {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a health share in per mille"
            )]
            let health = (state.health / state.max_health.max(1.0) * 1_000.0) as u16;
            if health < self.garrison.colossus.stage.ends_below() {
                let lands = self.after_millis(now, COLLAPSE_TELL_MILLIS);
                self.garrison.colossus.collapsing = Some((next, lands));
                self.garrison.colossus.fist = None;
                state.mode = EnemyMode::Stunned;
                state.mode_until = lands;
                output.push_event(garrison_event(GarrisonEvent::HallCollapsing, None, centre));
            }
        }
        // The core's place: it guards (swaying), stands in the crypt, or hovers after its prey.
        let current = self.garrison.colossus.stage;
        let (home_x, home_y) = self.colossus_home(&hall, current);
        let millis = self.millis_at(now);
        let (x, y) = match current {
            ColossusStage::Guard => {
                // A slow sway towards the hall's middle and back, four tiles.
                let toward = if sites::door_side(self.terrain.settings()) < 0 {
                    -1.0
                } else {
                    1.0
                };
                #[allow(clippy::cast_precision_loss, reason = "a sixteen-second cycle")]
                let phase = (millis % 16_000) as f32 / 16_000.0;
                let tri = 1.0 - (phase * 2.0 - 1.0).abs();
                ((toward * tri * 4.0).mul_add(TILE_SIZE, home_x), home_y)
            }
            ColossusStage::Fallen => (home_x, home_y),
            ColossusStage::Unbound => {
                let target_x = rigs
                    .iter()
                    .min_by(|a, b| {
                        (a.1 - body.x)
                            .abs()
                            .total_cmp(&(b.1 - body.x).abs())
                            .then(a.0.cmp(&b.0))
                    })
                    .map_or(home_x, |rig| rig.1);
                let limit =
                    TILE_SIZE * (f32::from(i16::try_from(hall.width() / 2 - 10).unwrap_or(10)));
                let goal = target_x.clamp(home_x - limit, home_x + limit);
                let step = 40.0 * self.timing.delta_seconds;
                let next_x = body.x + (goal - body.x).clamp(-step, step);
                #[allow(clippy::cast_precision_loss, reason = "a three-second bob")]
                let bob = ((millis % 3_000) as f32 / 3_000.0 * std::f32::consts::TAU).sin() * 6.0;
                (next_x, home_y + bob)
            }
        };
        body.x = crate::quantize::quantize_simulation_scalar(x);
        body.y = crate::quantize::quantize_simulation_scalar(y);
        body.previous_x = body.x;
        body.previous_y = body.y;
        body.velocity_x = 0.0;
        body.velocity_y = 0.0;
        if let Some(target) = rigs.first().map(|rig| rig.0)
            && state.target.is_none()
        {
            state.target = Some(target);
        }
        self.entities.update(core, |entity| {
            entity.body = body;
            entity.state = EntityState::Enemy(state);
        });
        if !self.garrison.colossus.collapsing() {
            self.advance_fists(&hall, (body.x, body.y), &rigs, output);
            if current == ColossusStage::Fallen && now >= self.garrison.colossus.next_rubble {
                self.garrison.colossus.next_rubble = self.after_millis(now, RUBBLE_VOLLEY.1);
                let lands = self.after_millis(now, RUBBLE_TELL_MILLIS);
                let mut columns: Vec<(i32, i32)> = Vec::new();
                for (index, (_, _, _, tile)) in rigs.iter().enumerate() {
                    for offset in [0, -3, 3] {
                        if columns.len() >= RUBBLE_VOLLEY.0 * (index + 1) {
                            break;
                        }
                        columns.push((tile.x + offset, tile.y));
                    }
                }
                for (column, row) in columns {
                    if let Some(drop) =
                        self.ceiling_drop_within(column, row, lands, CRYPT_CEILING_TILES)
                        && !self.garrison.colossus_drop_known(&drop)
                    {
                        self.garrison.drops.push(drop);
                    }
                }
            }
            if current == ColossusStage::Unbound && now >= self.garrison.colossus.next_adds {
                self.garrison.colossus.next_adds = self.after_millis(now, COLOSSUS_ADDS.1);
                let entities = &self.entities;
                self.garrison
                    .colossus
                    .adds
                    .retain(|add| entities.get(EntityId(*add)).is_some());
                let tier = self.tier_at(centre);
                let floor = sites::upper_floor_row(&hall);
                for side in [hall.min.x + 6, hall.max.x - 6] {
                    if self.garrison.colossus.adds.len() >= COLOSSUS_ADDS.0 {
                        break;
                    }
                    let (sx, sy) = self.frame().tile_center(TilePosition {
                        x: side,
                        y: floor - 2,
                    });
                    let add = self.spawn_enemy_with(
                        EnemyKind::WarSentry,
                        (sx, sy),
                        tier,
                        None,
                        Some(core),
                    );
                    let target = rigs.first().map(|rig| rig.0);
                    self.entities.update(add, |entity| {
                        if let EntityState::Enemy(sentry) = &mut entity.state {
                            sentry.mode = EnemyMode::Chase;
                            sentry.target = target;
                            sentry.last_seen = now;
                        }
                    });
                    self.garrison.colossus.adds.push(add.get());
                }
            }
        }
        self.place_colossus_parts((body.x, body.y));
    }

    /// The fists' attacks: a slam over a marked column (stages one and three), a sweep along
    /// the crypt's floor (stage two).
    #[allow(clippy::too_many_lines, reason = "both attacks' beats in order")]
    fn advance_fists(
        &mut self,
        hall: &TileRect,
        core: (f32, f32),
        rigs: &[(PlayerId, f32, f32, TilePosition)],
        output: &mut TickOutput,
    ) {
        let now = self.simulation_tick;
        let stage = self.garrison.colossus.stage;
        let stage_index = usize::from(stage.index());
        if let Some(fist) = self.garrison.colossus.fist {
            match fist.kind {
                FistMoveKind::Slam if !fist.landed && now >= fist.lands => {
                    if let Some(started) = self.garrison.colossus.fist.as_mut() {
                        started.landed = true;
                    }
                    let (ix, iy) = self.frame().tile_center(fist.to);
                    let floor_y = TILE_SIZE.mul_add(0.5, iy);
                    let (half_w, height, damage) = SLAM_WAVE;
                    for (player_id, x, y, _) in rigs {
                        if (x - ix).abs() <= half_w * TILE_SIZE
                            && *y <= TILE_SIZE.mul_add(0.5, floor_y)
                            && *y >= height.mul_add(-TILE_SIZE, floor_y)
                        {
                            let _landed = self.strike_rig(
                                *player_id,
                                damage,
                                (ix, floor_y),
                                260.0,
                                EnemyKind::GatekeeperColossus,
                                now,
                                output,
                            );
                        }
                    }
                    for (pilot, _) in self.pilots_near(ix, floor_y - TILE_SIZE, half_w * TILE_SIZE)
                    {
                        let _landed = self.strike_pilot_as(
                            Some(EnemyKind::GatekeeperColossus),
                            pilot,
                            damage,
                            (ix, floor_y),
                            220.0,
                            output,
                        );
                    }
                    output.push_event(garrison_event(GarrisonEvent::FistSlam, None, fist.to));
                }
                FistMoveKind::Sweep if now >= fist.strikes && now < fist.lands => {
                    let at = self.fist_position(core, &fist);
                    let reach = ColossusPart::LeftFist.radius() + Self::rig_radius();
                    for (player_id, x, y, _) in rigs {
                        if (x - at.0).hypot(y - at.1) <= reach {
                            let _landed = self.strike_rig(
                                *player_id,
                                SWEEP_DAMAGE,
                                at,
                                240.0,
                                EnemyKind::GatekeeperColossus,
                                now,
                                output,
                            );
                        }
                    }
                }
                _ => {}
            }
            if now >= fist.rests {
                self.garrison.colossus.fist = None;
                self.garrison.colossus.next_fist =
                    self.after_millis(now, FIST_PERIOD_MILLIS[stage_index] / 2);
            }
            return;
        }
        if now < self.garrison.colossus.next_fist {
            return;
        }
        let hand = 1 - self.garrison.colossus.last_hand.min(1);
        let core_tile = self.frame().tile_at(core.0, core.1);
        let kind = match stage {
            ColossusStage::Fallen => FistMoveKind::Sweep,
            ColossusStage::Guard | ColossusStage::Unbound => FistMoveKind::Slam,
        };
        let (from, to) = match kind {
            FistMoveKind::Slam => {
                // Over the nearest rig's column (within reach), onto the floor under it.
                let Some(target) = rigs
                    .iter()
                    .min_by_key(|rig| ((rig.3.x - core_tile.x).abs(), rig.0))
                    .map(|rig| rig.3)
                else {
                    return;
                };
                let column = target.x.clamp(
                    core_tile.x - FIST_REACH_TILES,
                    core_tile.x + FIST_REACH_TILES,
                );
                let Some(floor) = (target.y..target.y + 24).find(|row| {
                    self.terrain.is_solid_at(TilePosition {
                        x: column,
                        y: row + 1,
                    })
                }) else {
                    return;
                };
                let at = TilePosition {
                    x: column,
                    y: floor,
                };
                (at, at)
            }
            FistMoveKind::Sweep => {
                let crypt = sites::hall_crypt(hall);
                let row = crypt.max.y;
                let (start, end) = if hand == 0 {
                    (crypt.min.x + 2, crypt.max.x - 2)
                } else {
                    (crypt.max.x - 2, crypt.min.x + 2)
                };
                (
                    TilePosition { x: start, y: row },
                    TilePosition { x: end, y: row },
                )
            }
        };
        let strikes = self.after_millis(now, FIST_TELL_MILLIS[stage_index]);
        let lands = self.after_millis(strikes, FIST_STRIKE_MILLIS[stage_index]);
        let rests = self.after_millis(lands, FIST_REST_MILLIS);
        self.garrison.colossus.last_hand = hand;
        self.garrison.colossus.fist = Some(FistMove {
            kind,
            hand,
            from,
            to,
            strikes,
            lands,
            rests,
            landed: false,
        });
        self.garrison.colossus.next_fist = self.after_millis(now, FIST_PERIOD_MILLIS[stage_index]);
    }

    /// Where a fist is during `fist` (local pixels).
    fn fist_position(&self, core: (f32, f32), fist: &FistMove) -> (f32, f32) {
        let now = self.simulation_tick;
        let part = if fist.hand == 0 {
            ColossusPart::LeftFist
        } else {
            ColossusPart::RightFist
        };
        let (rx, ry) = part.rest_offset();
        let rest = (core.0 + rx, core.1 + ry);
        let frame = self.frame();
        let (fx, fy) = frame.tile_center(fist.from);
        let (tx, ty) = frame.tile_center(fist.to);
        let ticks = |a: SimulationTick, b: SimulationTick| b.get().saturating_sub(a.get()).max(1);
        #[allow(clippy::cast_precision_loss, reason = "a few hundred ticks")]
        let share = |from: SimulationTick, to: SimulationTick| {
            now.get().saturating_sub(from.get()) as f32 / ticks(from, to) as f32
        };
        match fist.kind {
            FistMoveKind::Slam => {
                let raised = (fx, TILE_SIZE.mul_add(-4.0, fy));
                if now < fist.strikes {
                    // Up over the marked column (it rises in the first part of the tell).
                    let tell_ticks = self.timing.ticks_for_millis(
                        FIST_TELL_MILLIS[usize::from(self.garrison.colossus.stage.index())],
                    );
                    let began = SimulationTick::new(fist.strikes.get().saturating_sub(tell_ticks));
                    lerp(rest, raised, share(began, fist.strikes) * 2.5)
                } else if now < fist.lands {
                    lerp(raised, (tx, ty), share(fist.strikes, fist.lands))
                } else {
                    (tx, ty)
                }
            }
            FistMoveKind::Sweep => {
                if now < fist.strikes {
                    let tell_ticks = self.timing.ticks_for_millis(
                        FIST_TELL_MILLIS[usize::from(self.garrison.colossus.stage.index())],
                    );
                    let began = SimulationTick::new(fist.strikes.get().saturating_sub(tell_ticks));
                    lerp(rest, (fx, fy), share(began, fist.strikes) * 1.8)
                } else if now < fist.lands {
                    lerp((fx, fy), (tx, ty), share(fist.strikes, fist.lands))
                } else {
                    (tx, ty)
                }
            }
        }
    }

    /// Puts the parts where they are drawn: the head and legs round the core, the fists at rest
    /// or where their attack has them.
    fn place_colossus_parts(&mut self, core: (f32, f32)) {
        let fist = self.garrison.colossus.fist;
        let parts = self.garrison.colossus.parts.clone();
        for (part, id) in parts {
            let (dx, dy) = part.rest_offset();
            let at = match (part.hand(), fist) {
                (Some(hand), Some(fist)) if fist.hand == hand => self.fist_position(core, &fist),
                _ => (core.0 + dx, core.1 + dy),
            };
            let (x, y) = (
                crate::quantize::quantize_simulation_scalar(at.0),
                crate::quantize::quantize_simulation_scalar(at.1),
            );
            self.entities.update(EntityId(id), |entity| {
                entity.body.x = x;
                entity.body.y = y;
                entity.body.previous_x = x;
                entity.body.previous_y = y;
                entity.body.velocity_x = 0.0;
                entity.body.velocity_y = 0.0;
            });
        }
    }

    /// The hall gives way into `stage`'s shape (no solid tile onto a body).
    fn collapse_hall(&mut self, hall: &TileRect, stage: ColossusStage) {
        for (at, kind) in sites::collapse_writes(hall, stage) {
            let here = self.terrain.tile(at).map(|tile| tile.kind);
            if kind == TileKind::Air {
                if here.is_some_and(|kind| !kind.is_open()) {
                    let _ = self.terrain.set_kind_by_rule(at, kind);
                }
                continue;
            }
            if here.is_some_and(TileKind::is_open) && !self.tile_occupied(at) {
                let _ = self.terrain.set_kind_by_rule(at, kind);
            }
        }
    }

    /// A stone falling at `lands` from the first ceiling within `rise` tiles above `row`.
    fn ceiling_drop_within(
        &self,
        column: i32,
        row: i32,
        lands: SimulationTick,
        rise: i32,
    ) -> Option<crate::garrison::RubbleDrop> {
        let mut open_below = None;
        for up in 0..=rise {
            let at = TilePosition {
                x: column,
                y: row - up,
            };
            if self.terrain.is_solid_at(at) {
                return open_below.map(|(x, y)| crate::garrison::RubbleDrop { lands, x, y });
            }
            open_below = Some((at.x, at.y));
        }
        None
    }

    /// Stages the Colossus's fight on `stage` (scenes, tests): the hall takes the stage's shape
    /// and, if it is awake, the core's health that stage's top; `collapsing` starts the tell of
    /// the collapse after it.
    pub fn stage_colossus(&mut self, stage: ColossusStage, collapsing: bool) {
        let Some(hall) = self.colossus_hall() else {
            return;
        };
        for shape in ColossusStage::ALL {
            if shape.index() > self.garrison.colossus.arena.index()
                && shape.index() <= stage.index()
            {
                self.collapse_hall(&hall, shape);
            }
        }
        self.garrison.colossus.arena = stage;
        self.garrison.colossus.stage = stage;
        self.garrison.colossus.fist = None;
        let now = self.simulation_tick;
        self.garrison.colossus.collapsing = if collapsing {
            stage
                .next()
                .map(|next| (next, self.after_millis(now, COLLAPSE_TELL_MILLIS)))
        } else {
            None
        };
        if stage == ColossusStage::Unbound {
            let legs: Vec<u64> = self
                .garrison
                .colossus
                .parts
                .iter()
                .filter(|(part, _)| *part == ColossusPart::Legs)
                .map(|(_, id)| *id)
                .collect();
            for id in legs {
                self.entities.despawn(EntityId(id));
            }
            self.garrison
                .colossus
                .parts
                .retain(|(part, _)| *part != ColossusPart::Legs);
        }
        let home = self.colossus_home(&hall, stage);
        if let Some((id, _, mut state)) = self.colossus_parts() {
            state.health =
                (state.max_health * f32::from(stage.starts_at()) / 1_000.0 - 1.0).max(1.0);
            if collapsing {
                state.mode = EnemyMode::Stunned;
                state.mode_until = self.after_millis(now, COLLAPSE_TELL_MILLIS);
            }
            self.entities.update(id, |entity| {
                entity.state = EntityState::Enemy(state);
                entity.body.x = home.0;
                entity.body.y = home.1;
            });
            self.place_colossus_parts(home);
        }
    }

    /// Stages a fist attack of the Colossus's present stage at `at` (scenes): its tell.
    pub fn stage_colossus_fist(&mut self, hand: u8, at: TilePosition) {
        let Some(hall) = self.colossus_hall() else {
            return;
        };
        let now = self.simulation_tick;
        let stage = self.garrison.colossus.stage;
        let index = usize::from(stage.index());
        let (kind, from, to) = if stage == ColossusStage::Fallen {
            let crypt = sites::hall_crypt(&hall);
            let row = crypt.max.y;
            let (start, end) = if hand == 0 {
                (crypt.min.x + 2, crypt.max.x - 2)
            } else {
                (crypt.max.x - 2, crypt.min.x + 2)
            };
            (
                FistMoveKind::Sweep,
                TilePosition { x: start, y: row },
                TilePosition { x: end, y: row },
            )
        } else {
            (FistMoveKind::Slam, at, at)
        };
        let strikes = self.after_millis(now, FIST_TELL_MILLIS[index]);
        let lands = self.after_millis(strikes, FIST_STRIKE_MILLIS[index]);
        self.garrison.colossus.fist = Some(FistMove {
            kind,
            hand,
            from,
            to,
            strikes,
            lands,
            rests: self.after_millis(lands, FIST_REST_MILLIS),
            landed: false,
        });
        if let Some((_, body, _)) = self.colossus_parts() {
            self.place_colossus_parts((body.x, body.y));
        }
    }

    /// The Colossus sleeps: its body, parts and adds go; it heals to its stage's top.
    fn put_colossus_to_sleep(&mut self) {
        let colossus = &mut self.garrison.colossus;
        let entity = colossus.entity.take();
        let parts = std::mem::take(&mut colossus.parts);
        let adds = std::mem::take(&mut colossus.adds);
        colossus.collapsing = None;
        colossus.fist = None;
        if let Some(entity) = entity {
            self.entities.despawn(EntityId(entity));
        }
        for (_, id) in parts {
            self.entities.despawn(EntityId(id));
        }
        for id in adds {
            self.entities.despawn(EntityId(id));
        }
    }

    /// The Colossus fell: every rig of the crew gets the Heart Key, and the crypt's floor opens
    /// on the way down.
    pub(in crate::world) fn colossus_fell(
        &mut self,
        tile: TilePosition,
        by: Option<PlayerId>,
        output: &mut TickOutput,
    ) {
        self.put_colossus_to_sleep();
        self.garrison.colossus.beaten = true;
        let Some(hall) = self.colossus_hall() else {
            return;
        };
        if self.garrison.colossus.arena == ColossusStage::Guard {
            self.collapse_hall(&hall, ColossusStage::Fallen);
            self.garrison.colossus.arena = ColossusStage::Fallen;
        }
        for at in sites::victory_shaft(&hall).positions() {
            if self
                .terrain
                .tile(at)
                .is_some_and(|found| !found.kind.is_open())
            {
                let _ = self.terrain.set_kind_by_rule(at, TileKind::Air);
            }
        }
        output.push_event(garrison_event(GarrisonEvent::ColossusDefeated, by, tile));
        let players: Vec<PlayerId> = self.players.keys().copied().collect();
        for player_id in players {
            if let Some(player) = self.players.get_mut(&player_id) {
                crate::economy::grant_rig_system(player, RigSystem::HeartKey);
            }
            output.push_event(garrison_event(
                GarrisonEvent::GateRewards,
                Some(player_id),
                tile,
            ));
        }
    }

    /// Whether `pylon`'s fort is awake (tests, the game).
    #[must_use]
    pub fn fort_awake(&self, pylon: TilePosition) -> bool {
        self.garrison
            .forts
            .get(&pylon)
            .is_some_and(|fort| fort.phase != FortPhase::Dormant && fort.phase != FortPhase::Asleep)
    }
}

impl crate::garrison::Garrison {
    /// Whether a stone already falls at `drop`'s tile.
    fn colossus_drop_known(&self, drop: &crate::garrison::RubbleDrop) -> bool {
        self.drops
            .iter()
            .any(|known| known.x == drop.x && known.y == drop.y)
    }
}
