//! The Living Dark's creatures (Wave 8, Descent III): what sets them apart from the generic
//! state machine in `world/enemies.rs`, and the Mycelial Mind's fight.
//!
//! * **Who hunts** ([`WorldState::living_notice`]): lure fish and nerve nodes never do; the
//!   Amber Court's drones only once the Court is hostile; the Swarm's soldiers, lancers and
//!   matriarch not under a truce. A creature the Mind possesses knows where every rig in reach
//!   is, through rock, and never gives up the chase.
//! * **Brood** ([`brood_of`]): the matriarch breeds Swarm soldiers, bile polyps bud leechlings
//!   (cave nests still hatch bats).
//! * **Digging**: bark beetles bore through living root, maw worms through flesh (rarely, like
//!   every digger: `BITE_MILLIS`).
//! * **Spawning**: the Swarm breeds no new soldiers once broken; the matriarch never roams
//!   (her gallery holds her), nor do the Mind and its nodes.
//! * **The Mycelial Mind** ([`WorldState::advance_mind`]): wakes when a rig enters its hollow,
//!   with [`MIND_NODES`] nerve nodes that shield it. It lobs spore orbs (its ranged attack),
//!   calls spore storms onto the rigs in its hollow and possesses the nearest creature (or
//!   raises spore thralls when none is near). Kill the nodes and it is exposed for
//!   [`MIND_EXPOSED_MILLIS`], then they grow back and it storms faster. Its death opens the
//!   gate (`WorldState::living_dark_kill`). With no rig in its hollow for
//!   [`MIND_SLEEP_MILLIS`] it sleeps again and heals.

use crate::{
    enemy::{EnemyKind, EnemyMode, EnemyState},
    entity::{Body, EntityId, EntityState},
    event::TickOutput,
    ids::{PlayerId, SimulationTick},
    living_dark::{
        LivingDarkEvent, MIND_EXPOSED_MILLIS, MIND_NODES, MIND_POSSESS_MILLIS, MIND_POSSESS_TILES,
        MIND_SLEEP_MILLIS, MIND_STORM_MILLIS, POSSESSED_PHASE, SPORE_SHOT_CLOUD_MILLIS, SwarmState,
    },
    terrain::{TILE_SIZE, TilePosition},
};

use super::WorldState;

/// What a spawner of `kind` hatches.
#[must_use]
pub const fn brood_of(kind: EnemyKind) -> EnemyKind {
    match kind {
        EnemyKind::SwarmMatriarch => EnemyKind::HiveSoldier,
        EnemyKind::BilePolyp => EnemyKind::Leechling,
        // The Old Deep's Glowmother (Wave 8) births glow grubs.
        EnemyKind::Glowmother => EnemyKind::GlowGrub,
        _ => EnemyKind::CaveBat,
    }
}

/// How far a possessed creature senses rigs, tiles.
const POSSESSED_SENSE_TILES: f32 = 40.0;
/// Columns either side of the hollow's centre the nodes hang at.
const NODE_SPREAD: [i32; MIND_NODES] = [-15, 0, 15];

impl WorldState {
    /// The Living Dark's say on who `state`'s creature notices: `Some(answer)` overrides the
    /// generic perception (see the module docs), `None` leaves it.
    pub(in crate::world) fn living_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        if !state.kind.is_living_dark() && state.phase != POSSESSED_PHASE {
            return None;
        }
        if state.phase == POSSESSED_PHASE {
            let reach = POSSESSED_SENSE_TILES * TILE_SIZE;
            return Some(
                self.players
                    .iter()
                    .filter(|(_, player)| player.hull > 0.0)
                    .map(|(id, player)| ((player.x - body.x).hypot(player.y - body.y), *id))
                    .filter(|(distance, _)| *distance <= reach)
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                    .map(|(_, id)| id),
            );
        }
        match state.kind {
            EnemyKind::LureFish | EnemyKind::MindNode => Some(None),
            kind if !self.living_dark.hive.hostile_kind(kind) => Some(None),
            _ => None,
        }
    }

    /// Whether the Mind's nerve nodes shield it from `kind`'s damage.
    pub(super) fn mind_shields(&self, kind: EnemyKind) -> bool {
        kind == EnemyKind::MycelialMind && self.living_dark.mind.shielded()
    }

    /// Whether the Living Dark lets `kind` spawn roaming (the Swarm breeds no more once broken;
    /// bosses never roam).
    pub(super) fn living_spawn_allowed(&self, kind: EnemyKind) -> bool {
        match kind {
            EnemyKind::HiveSoldier | EnemyKind::AcidLancer => {
                self.living_dark.hive.swarm != SwarmState::Broken
            }
            EnemyKind::SwarmMatriarch | EnemyKind::MycelialMind | EnemyKind::MindNode => false,
            _ => true,
        }
    }

    /// The brood matriarch in her gallery: she appears when a rig comes near it (once the Swarm
    /// is broken she is gone for good).
    pub(super) fn advance_matriarch(&mut self) {
        if self.living_dark.hive.swarm == SwarmState::Broken {
            return;
        }
        let Some(gallery) = self.matriarch_gallery() else {
            return;
        };
        let frame = self.frame();
        let near = self
            .players
            .values()
            .any(|player| gallery.expanded(24).contains(player.tile_in(frame)));
        if !near
            || self
                .enemies()
                .any(|(_, state)| state.kind == EnemyKind::SwarmMatriarch)
        {
            return;
        }
        let centre = gallery.center();
        let floor = TilePosition {
            x: centre.x,
            y: gallery.max.y - 3,
        };
        let (x, y) = frame.tile_center(floor);
        let tier = self.tier_at(floor);
        let _ = self.spawn_enemy_with(EnemyKind::SwarmMatriarch, (x, y), tier, None, None);
    }

    /// Spawns the Mind's nerve nodes round the hollow's centre.
    fn grow_nodes(&mut self, hollow: crate::worldgen::TileRect) {
        let frame = self.frame();
        let centre = hollow.center();
        let tier = self.tier_at(centre);
        let mut nodes = Vec::new();
        for spread in NODE_SPREAD {
            let tile = TilePosition {
                x: centre.x + spread,
                y: centre.y + if spread == 0 { -5 } else { 1 },
            };
            let (x, y) = frame.tile_center(tile);
            let id = self.spawn_enemy_with(EnemyKind::MindNode, (x, y), tier, None, None);
            nodes.push(id.get());
        }
        self.living_dark.mind.nodes = nodes;
    }

    /// One tick of the Mycelial Mind's fight (see the module docs).
    pub(in crate::world) fn advance_mind(&mut self, now: SimulationTick, output: &mut TickOutput) {
        self.advance_matriarch();
        if self.living_dark.mind.beaten {
            return;
        }
        let Some(hollow) = self.mind_hollow() else {
            return;
        };
        let frame = self.frame();
        let inside: Vec<(PlayerId, TilePosition)> = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .map(|(id, player)| (*id, player.tile_in(frame)))
            .filter(|(_, tile)| hollow.expanded(2).contains(*tile))
            .collect();
        if !inside.is_empty() {
            self.living_dark.mind.last_visited = now;
        }
        let centre = hollow.center();
        // Its body gone some other way (despawned far from every rig): it sleeps.
        if let Some(entity) = self.living_dark.mind.entity
            && self.entities.get(EntityId(entity)).is_none()
        {
            self.put_mind_to_sleep();
        }
        if !self.living_dark.mind.awake() {
            if let Some((player, _)) = inside.first().copied() {
                let tier = self.tier_at(centre);
                let (x, y) = frame.tile_center(TilePosition {
                    x: centre.x,
                    y: centre.y - 2,
                });
                let id = self.spawn_enemy_with(EnemyKind::MycelialMind, (x, y), tier, None, None);
                self.living_dark.mind.entity = Some(id.get());
                self.living_dark.mind.cycles = 0;
                self.living_dark.mind.exposed_until = None;
                self.living_dark.mind.next_storm = self.after_millis(now, MIND_STORM_MILLIS / 2);
                self.living_dark.mind.next_possession =
                    self.after_millis(now, MIND_POSSESS_MILLIS / 2);
                self.grow_nodes(hollow);
                output.push_event(super::super::living_dark::living_dark_event(
                    LivingDarkEvent::MindAwakened,
                    Some(player),
                    centre,
                ));
            }
            return;
        }
        let idle_ticks = now
            .get()
            .saturating_sub(self.living_dark.mind.last_visited.get());
        if inside.is_empty() && idle_ticks > self.timing.ticks_for_millis(MIND_SLEEP_MILLIS) {
            self.put_mind_to_sleep();
            return;
        }
        // Nodes that died leave the list (the kill hook removes them too).
        let entities = &self.entities;
        self.living_dark
            .mind
            .nodes
            .retain(|node| entities.get(EntityId(*node)).is_some());
        let mind = &self.living_dark.mind;
        if mind.nodes.is_empty() && mind.exposed_until.is_none() {
            let until = self.after_millis(now, MIND_EXPOSED_MILLIS);
            let mind = &mut self.living_dark.mind;
            mind.exposed_until = Some(until);
            mind.cycles = mind.cycles.saturating_add(1);
            output.push_event(super::super::living_dark::living_dark_event(
                LivingDarkEvent::MindExposed,
                None,
                centre,
            ));
        } else if let Some(until) = mind.exposed_until
            && now >= until
        {
            self.living_dark.mind.exposed_until = None;
            self.grow_nodes(hollow);
            output.push_event(super::super::living_dark::living_dark_event(
                LivingDarkEvent::MindRegrew,
                None,
                centre,
            ));
        }
        let cycles = u64::from(self.living_dark.mind.cycles.min(3));
        if now >= self.living_dark.mind.next_storm && !inside.is_empty() {
            // A spore storm: a cloud on every rig in the hollow, and one more each cycle.
            let storms: Vec<TilePosition> = inside
                .iter()
                .enumerate()
                .flat_map(|(index, (_, tile))| {
                    let spread = i32::try_from(index).unwrap_or(0) * 3;
                    (0..=cycles).map(move |extra| {
                        let side = if extra % 2 == 0 { 1 } else { -1 };
                        TilePosition {
                            x: tile.x + side * (spread + i32::try_from(extra).unwrap_or(0) * 5),
                            y: tile.y - 1,
                        }
                    })
                })
                .collect();
            for tile in storms {
                self.spawn_spore_cloud(tile, SPORE_SHOT_CLOUD_MILLIS, None, output);
            }
            let wait = MIND_STORM_MILLIS - MIND_STORM_MILLIS * cycles / 6;
            self.living_dark.mind.next_storm = self.after_millis(now, wait);
        }
        if now >= self.living_dark.mind.next_possession {
            self.possess(hollow, output);
            self.living_dark.mind.next_possession = self.after_millis(now, MIND_POSSESS_MILLIS);
        }
    }

    /// The Mind reaches for the nearest free creature within [`MIND_POSSESS_TILES`] of its
    /// hollow's centre; with none near it raises two spore thralls from the hollow's floor.
    fn possess(&mut self, hollow: crate::worldgen::TileRect, output: &mut TickOutput) {
        let frame = self.frame();
        let centre = hollow.center();
        let (cx, cy) = frame.tile_center(centre);
        let reach = MIND_POSSESS_TILES * TILE_SIZE;
        let target = self
            .enemies()
            .filter(|(_, state)| {
                state.phase != POSSESSED_PHASE
                    && !matches!(state.kind, EnemyKind::MycelialMind | EnemyKind::MindNode)
                    && !state.kind.is_crypt()
                    && !state.kind.is_tomb()
            })
            .map(|(entity, _)| ((entity.body.x - cx).hypot(entity.body.y - cy), entity.id))
            .filter(|(distance, _)| *distance <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id);
        let possessed = if let Some(id) = target {
            vec![id]
        } else {
            let tier = self.tier_at(centre);
            [(EnemyKind::CapCrawler, -9), (EnemyKind::SporePuffer, 9)]
                .into_iter()
                .map(|(kind, dx)| {
                    let (x, y) = frame.tile_center(TilePosition {
                        x: centre.x + dx,
                        y: hollow.max.y - 4,
                    });
                    self.spawn_enemy_with(kind, (x, y), tier, None, None)
                })
                .collect()
        };
        let now = self.simulation_tick;
        for id in possessed {
            let mut at = None;
            self.entities.update(id, |entity| {
                if let EntityState::Enemy(state) = &mut entity.state {
                    state.phase = POSSESSED_PHASE;
                    state.mode = EnemyMode::Chase;
                    state.attack_ready = now;
                    state.health = state.max_health;
                    at = Some((entity.body.x, entity.body.y));
                }
            });
            if let Some((x, y)) = at {
                output.push_event(super::super::living_dark::living_dark_event(
                    LivingDarkEvent::MindPossessed,
                    None,
                    frame.tile_at(x, y),
                ));
            }
        }
    }

    /// The Mind sleeps: its body and nodes go, it heals, its thralls go free.
    fn put_mind_to_sleep(&mut self) {
        let mind = &mut self.living_dark.mind;
        let entity = mind.entity.take();
        let nodes = std::mem::take(&mut mind.nodes);
        mind.exposed_until = None;
        mind.cycles = 0;
        if let Some(entity) = entity {
            self.entities.despawn(EntityId(entity));
        }
        for node in nodes {
            self.entities.despawn(EntityId(node));
        }
        self.release_possessed();
        self.clear_spore_clouds();
    }
}
