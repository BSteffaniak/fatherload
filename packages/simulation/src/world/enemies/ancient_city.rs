//! The buried city's creatures (Wave 10 ancients-a, Descent VII strata 1-3): what sets them
//! apart from the generic state machine in `world/enemies.rs`.
//!
//! * **The city's watch** ([`WorldState::city_notice`]): maintenance drones mend the city and
//!   leave rigs alone, unless the city's alarm hunts a rig (then drones and street sentinels
//!   come for it from [`ALARM_HUNT_TILES`] away) or a rig shot them. A **street sentinel**
//!   sees only what its searchlight shows: a cone ahead of it, in line of sight.
//! * **Theft** ([`WorldState::city_strike`]): a scrap crawler's bite carries off the best ore
//!   in the bay ([`crate::ancient_city::crawler_pick`]); kill it and the ore drops where it
//!   dies. A spark wisp's jolt drains fuel.
//!
//! The rest (cog rollers' charges, rivet hounds' pounces, archive wardens' reading beams,
//! page moths' nibbles, index spiders' drops) are the generic lunges, contacts and shots with
//! their own stats and sprites; every kind keeps the body rules like any creature.

use crate::{
    ancient_city::{ALARM_HUNT_TILES, CityEvent, WISP_FUEL_DRAIN, crawler_pick},
    enemy::{EnemyKind, EnemyState},
    entity::Body,
    event::TickOutput,
    ids::PlayerId,
    item::ItemStack,
    physics,
    quantize::quantize_simulation_scalar as quantize,
    terrain::TILE_SIZE,
};

use super::super::ancient_city::city_event;
use super::WorldState;

impl WorldState {
    /// The buried city's say on who `state`'s creature notices: `Some(answer)` overrides the
    /// generic perception, `None` leaves it (see the module docs).
    #[allow(
        clippy::option_option,
        reason = "None leaves the generic perception; Some(None) notices nobody"
    )]
    pub(in crate::world) fn city_notice(
        &self,
        body: &Body,
        state: &EnemyState,
    ) -> Option<Option<PlayerId>> {
        if !matches!(
            state.kind,
            EnemyKind::MaintenanceDrone | EnemyKind::StreetSentinel
        ) {
            return None;
        }
        let now = self.millis_at(self.simulation_tick);
        let reach = ALARM_HUNT_TILES * TILE_SIZE;
        // A rig the city hunts, from far off.
        let hunted = self
            .players
            .iter()
            .filter(|(id, player)| player.hull > 0.0 && self.ancient_city.hunted(**id, now))
            .map(|(id, player)| ((player.x - body.x).hypot(player.y - body.y), *id))
            .filter(|(distance, _)| *distance <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id);
        if hunted.is_some() {
            return Some(hunted);
        }
        // Whoever shot it.
        let leash = state.kind.stats().leash_tiles * TILE_SIZE;
        if let Some(attacker) = state.last_attacker
            && self.players.get(&attacker).is_some_and(|player| {
                player.hull > 0.0 && (player.x - body.x).hypot(player.y - body.y) <= leash
            })
        {
            return Some(Some(attacker));
        }
        if state.kind == EnemyKind::MaintenanceDrone {
            return Some(None);
        }
        // A sentinel's searchlight: a cone ahead, in line of sight.
        let sight = state.kind.stats().sight_tiles * TILE_SIZE;
        let facing = if state.facing < 0 { -1.0 } else { 1.0 };
        let seen = self
            .players
            .iter()
            .filter(|(_, player)| player.hull > 0.0)
            .filter_map(|(id, player)| {
                let (dx, dy) = (player.x - body.x, player.y - body.y);
                let distance = dx.hypot(dy);
                let ahead = dx * facing > 0.0 && dy.abs() <= dx.abs().mul_add(0.55, TILE_SIZE);
                (ahead
                    && distance <= sight
                    && physics::sight_clear(&self.terrain, (body.x, body.y), (player.x, player.y)))
                .then_some((distance, *id))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id)| id);
        Some(seen)
    }

    /// What a buried city creature's landed hit on `player_id` adds (see the module docs).
    pub(in crate::world) fn city_strike(
        &mut self,
        player_id: PlayerId,
        kind: EnemyKind,
        from: (f32, f32),
        output: &mut TickOutput,
    ) {
        let frame = self.frame();
        match kind {
            EnemyKind::ScrapCrawler => {
                // The crawler that bit (the one standing where the hit came from).
                let Some(crawler) = self
                    .enemies()
                    .filter(|(_, state)| state.kind == EnemyKind::ScrapCrawler)
                    .map(|(entity, _)| {
                        (
                            (entity.body.x - from.0).hypot(entity.body.y - from.1),
                            entity.id,
                        )
                    })
                    .filter(|(distance, _)| *distance <= TILE_SIZE * 2.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id)
                else {
                    return;
                };
                if self.city_schedule.stolen.contains_key(&crawler.get()) {
                    return;
                }
                let Some(player) = self.players.get_mut(&player_id) else {
                    return;
                };
                let bay: Vec<ItemStack> = player.cargo_bay.stacks().collect();
                let Some(take) = crawler_pick(&bay) else {
                    return;
                };
                let taken = player.cargo_bay.remove(take.kind, take.count);
                if taken == 0 {
                    return;
                }
                let tile = frame.tile_at(player.x, player.y);
                self.city_schedule
                    .stolen
                    .insert(crawler.get(), ItemStack::new(take.kind, taken));
                #[allow(clippy::cast_possible_truncation, reason = "a couple of ore")]
                output.push_event(city_event(
                    CityEvent::CargoStolen,
                    Some(player_id),
                    tile,
                    taken as u16,
                ));
            }
            EnemyKind::SparkWisp => {
                let Some(player) = self.players.get_mut(&player_id) else {
                    return;
                };
                player.fuel = quantize((player.fuel - WISP_FUEL_DRAIN).max(0.0));
                let tile = frame.tile_at(player.x, player.y);
                output.push_event(city_event(CityEvent::FuelJolted, Some(player_id), tile, 0));
            }
            _ => {}
        }
    }

    /// A buried city creature died at `(x, y)`: a scrap crawler drops what it carried.
    pub(in crate::world) fn city_kill(&mut self, id: u64, x: f32, y: f32) {
        if let Some(stack) = self.city_schedule.stolen.remove(&id) {
            let _ = self.spawn_loot(x, y, (0.0, -120.0), stack, None);
        }
    }
}
