//! Rescue, lost-cargo recovery, hazard damage, and bomb blasts.

use crate::{
    ids::PlayerId,
    infrastructure::InfrastructureKind,
    item::{ItemContainer, ItemKind},
    rig::RigPartKind,
    service::{PlayerServiceTransaction, PlayerTransactionKind},
    summary::{PlayerScopedCommandOutcome, PlayerSurvivalSnapshot},
    terrain::{TILE_SIZE, TilePosition},
};

use super::{LostCargoSite, WorldState, record_service_transaction};

impl WorldState {
    #[allow(
        clippy::too_many_lines,
        reason = "rescue transaction explicitly records exact cargo losses"
    )]
    pub(super) fn canonical_rescue_player(
        &mut self,
        player_id: PlayerId,
    ) -> PlayerScopedCommandOutcome {
        let relay_count = u32::try_from(
            self.infrastructure
                .iter()
                .filter(|item| item.kind == InfrastructureKind::SignalRelay)
                .count(),
        )
        .unwrap_or(u32::MAX);
        let parts = self
            .equipped_rig_parts
            .get(&player_id)
            .cloned()
            .unwrap_or_default();
        // A home point (WP-BASE bed or homestead) replaces the town drop.
        let frame = self.frame();
        let (home_x, home_y) = self
            .rescue_home(player_id)
            .unwrap_or_else(|| frame.from_legacy_px(12.0 * TILE_SIZE, 4.0 * TILE_SIZE));
        let Some(player) = self.players.get_mut(&player_id) else {
            return PlayerScopedCommandOutcome::UnknownPlayer;
        };
        let credits_before = player.credits;
        let cargo_before = player.cargo_used();
        let rescue_x = player.x;
        let rescue_y = player.y;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "nonnegative bounded world depth intentionally converts to rescue fee units"
        )]
        let depth = (frame.legacy_y(player.y) / TILE_SIZE).max(0.0) as u32;
        let base_fee = 50_u32.saturating_add(depth.saturating_mul(3));
        let discount_percent = relay_count.saturating_mul(10).min(50);
        let relayed_fee = base_fee.saturating_mul(100 - discount_percent) / 100;
        let fee_divisor = if player.insured {
            u32::from(player.insurance_tier).saturating_add(1)
        } else {
            1
        };
        player.credits -= (relayed_fee / fee_divisor).min(player.credits);
        let protected_cargo = player.insured && player.insurance_tier >= 2;
        let quarter_loss = player.insured || parts.contains(&RigPartKind::ArmoredCargoBay);
        let mut lost_count = 0_u32;
        // Ore and artifacts spill at the rescue site; materials and other items stay aboard.
        let mut lost_items = ItemContainer::new();
        if !protected_cargo {
            player.cargo_bay.update_counts(|kind, count| {
                if !matches!(kind, ItemKind::Mineral(_) | ItemKind::Artifact(_)) {
                    return count;
                }
                let lost = if quarter_loss {
                    count.div_ceil(4)
                } else {
                    count.div_ceil(2)
                };
                lost_items.add(kind, lost);
                lost_count = lost_count.saturating_add(lost);
                count - lost
            });
        }
        player.insured = false;
        player.x = home_x;
        player.x_remainder = 0.0;
        player.y = home_y;
        player.velocity_x = 0.0;
        player.velocity_y = 0.0;
        player.hull = player.max_hull() * 0.5;
        player.fuel = player.fuel_capacity * 0.5;
        self.active_drills.remove(&player_id);
        self.active_drill_timers.remove(&player_id);
        if lost_count > 0 {
            self.lost_cargo_sites.insert(
                player_id,
                LostCargoSite {
                    x: rescue_x,
                    y: rescue_y,
                    items: lost_items,
                },
            );
        }
        self.rescue_counts
            .entry(player_id)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        record_service_transaction(
            &mut self.service_transactions,
            PlayerServiceTransaction {
                player_id,
                kind: PlayerTransactionKind::Rescue,
                credits_before,
                credits_after: player.credits,
                cargo_before,
                cargo_after: player.cargo_used(),
            },
        );
        PlayerScopedCommandOutcome::Applied
    }

    pub(super) fn recover_lost_cargo_for_player(&mut self, player_id: PlayerId) -> bool {
        let Some(mut site) = self.lost_cargo_sites.get(&player_id).cloned() else {
            return false;
        };
        let Some(player) = self.players.get_mut(&player_id) else {
            return false;
        };
        let dx = player.x - site.x;
        let dy = player.y - site.y;
        let mut capacity = player.cargo_capacity.saturating_sub(player.cargo_used());
        if dx.mul_add(dx, dy * dy) > (TILE_SIZE * 0.9).powi(2) || capacity == 0 {
            return false;
        }
        // Minerals first, then artifacts (kind order), as long as the bay has room.
        site.items.update_counts(|kind, count| {
            let recovered = capacity.min(count);
            player.cargo_bay.add(kind, recovered);
            capacity -= recovered;
            count - recovered
        });
        if site.count() == 0 {
            self.lost_cargo_sites.remove(&player_id);
        } else {
            self.lost_cargo_sites.insert(player_id, site);
        }
        true
    }

    /// Whether rigs take no damage at all: a test-mode world with the invulnerable toggle on,
    /// or with no-clip flight on (a rig flying through lava, gas pockets or a hot layer's rock
    /// must not burn while the hull toggle is off).
    #[must_use]
    pub fn damage_immune(&self) -> bool {
        self.test_mode
            .is_some_and(|state| state.invulnerable || state.no_clip)
    }

    /// Takes `damage` off `player_id`'s hull, returning whether any was taken (nothing in an
    /// invulnerable test-mode world, or for zero damage after the rig's counters).
    pub(crate) fn damage_player(&mut self, player_id: PlayerId, damage: f32) -> bool {
        if damage <= 0.0 || self.damage_immune() || !self.players.contains_key(&player_id) {
            return false;
        }
        let _snapshot = self.apply_hazard_damage(player_id, damage);
        true
    }

    pub(crate) fn apply_hazard_damage(
        &mut self,
        player_id: PlayerId,
        damage: f32,
    ) -> Option<PlayerSurvivalSnapshot> {
        // A rig waiting outside a tomb while its pilot runs it is safe (WP-TOMBS).
        let immune = self.damage_immune() || self.tombs.run_of(player_id).is_some();
        let warded = self.player_blessing(player_id) == Some(crate::village::BlessingKind::Ward);
        let player = self.players.get_mut(&player_id)?;
        if !immune {
            // A shrine's Blessing of the Ward softens every hit.
            #[allow(clippy::cast_precision_loss, reason = "a small percentage")]
            let damage = if warded {
                damage * (crate::village::services::WARD_DAMAGE_PERCENT as f32 / 100.0)
            } else {
                damage
            };
            player.hull = (player.hull - damage.max(0.0)).max(0.0);
        }
        Some(PlayerSurvivalSnapshot {
            player_id,
            fuel: player.fuel,
            hull: player.hull,
            disabled: player.hull <= 0.0,
            stranded: player.fuel <= 0.0,
        })
    }

    #[must_use]
    pub fn failure_state(&self, player_id: PlayerId) -> Option<PlayerSurvivalSnapshot> {
        self.player_survival_snapshot(player_id)
            .filter(|snapshot| snapshot.disabled || snapshot.stranded)
    }

    pub(crate) fn detonate_bomb_at(
        &mut self,
        center: TilePosition,
        radius: i32,
    ) -> crate::terrain::BlastResult {
        self.terrain.blast_radius(center, radius)
    }
}
