//! Speed-scaled camera lead (presentation only, WP-FEEL).
//!
//! A slow rig stays dead centre, exactly as before. Once a rig moves faster than a few tiles a
//! second, its seat's camera eases ahead of it in the direction of travel, so a fast rig sees
//! what it is about to hit (and a falling one sees the floor coming). The lead follows the
//! rig's velocity through a smoothing filter, so a wall impact or a sudden turn glides the view
//! back instead of snapping it. Nothing here feeds back into the simulation.

use std::collections::BTreeMap;

use fatherload_simulation::{PlayerId, TILE_SIZE, WorldState};

/// Horizontal speed (px/s) below which the camera does not lead: every stock-to-mid-tier rig.
pub const LEAD_DEAD_ZONE_X: f32 = 4.0 * TILE_SIZE;
/// Vertical speed (px/s) below which the camera does not lead.
pub const LEAD_DEAD_ZONE_Y: f32 = 6.0 * TILE_SIZE;
/// Seconds of travel the camera looks ahead per px/s over the dead zone (horizontal).
const LEAD_SECONDS_X: f32 = 0.28;
/// Seconds of travel the camera looks ahead per px/s over the dead zone (vertical).
const LEAD_SECONDS_Y: f32 = 0.2;
/// Largest lead as a fraction of the visible world's width / height.
pub const MAX_LEAD_FRACTION_X: f32 = 0.28;
pub const MAX_LEAD_FRACTION_Y: f32 = 0.22;
/// How quickly the lead follows its target (per second).
const LEAD_FOLLOW_RATE: f32 = 3.5;

/// The lead (world px) the camera of a rig moving at `(velocity_x, velocity_y)` eases toward.
#[must_use]
pub fn lead_target(velocity_x: f32, velocity_y: f32) -> (f32, f32) {
    let axis = |velocity: f32, dead_zone: f32, seconds: f32| {
        velocity.signum() * (velocity.abs() - dead_zone).max(0.0) * seconds
    };
    (
        axis(velocity_x, LEAD_DEAD_ZONE_X, LEAD_SECONDS_X),
        axis(velocity_y, LEAD_DEAD_ZONE_Y, LEAD_SECONDS_Y),
    )
}

/// Each local rig's smoothed camera lead.
#[derive(Clone, Debug, Default)]
pub struct CameraLeads {
    leads: BTreeMap<PlayerId, (f32, f32)>,
}

impl CameraLeads {
    /// Eases each of `players`' leads toward its rig's current velocity over `delta_seconds`.
    pub fn update(&mut self, delta_seconds: f32, world: &WorldState, players: &[PlayerId]) {
        self.leads.retain(|player, _| players.contains(player));
        let follow = 1.0 - (-LEAD_FOLLOW_RATE * delta_seconds.max(0.0)).exp();
        for &player_id in players {
            let Some(player) = world.player(player_id) else {
                self.leads.remove(&player_id);
                continue;
            };
            let (target_x, target_y) = lead_target(player.velocity_x, player.velocity_y);
            let lead = self.leads.entry(player_id).or_insert((0.0, 0.0));
            lead.0 = (target_x - lead.0).mul_add(follow, lead.0);
            lead.1 = (target_y - lead.1).mul_add(follow, lead.1);
        }
    }

    /// The camera offset (world px) for `player_id`'s view showing `visible` world px (width,
    /// height): the smoothed lead, capped to a fraction of the view.
    #[must_use]
    pub fn offset(&self, player_id: PlayerId, visible: (f32, f32)) -> (f32, f32) {
        let Some(&(x, y)) = self.leads.get(&player_id) else {
            return (0.0, 0.0);
        };
        let cap_x = visible.0 * MAX_LEAD_FRACTION_X;
        let cap_y = visible.1 * MAX_LEAD_FRACTION_Y;
        (x.clamp(-cap_x, cap_x), y.clamp(-cap_y, cap_y))
    }

    /// Forgets every lead (new world, teleport).
    pub fn clear(&mut self) {
        self.leads.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER: PlayerId = PlayerId::new(1);

    #[test]
    fn slow_rigs_stay_centred_and_fast_rigs_look_ahead() {
        assert_eq!(lead_target(LEAD_DEAD_ZONE_X, LEAD_DEAD_ZONE_Y), (0.0, 0.0));
        assert_eq!(lead_target(-60.0, 100.0), (0.0, 0.0));
        let (right, down) = lead_target(800.0, 560.0);
        assert!(right > 4.0 * TILE_SIZE, "{right}");
        assert!(down > TILE_SIZE, "{down}");
        let (left, up) = lead_target(-800.0, -560.0);
        assert!((left + right).abs() < f32::EPSILON && (up + down).abs() < f32::EPSILON);
    }

    #[test]
    fn the_lead_eases_in_and_out_and_stays_inside_the_view() {
        let mut world = WorldState::new_game(PLAYER);
        world.player_mut(PLAYER).expect("player").velocity_x = 800.0;
        let mut leads = CameraLeads::default();
        leads.update(1.0 / 60.0, &world, &[PLAYER]);
        let (first, _) = leads.offset(PLAYER, (10_000.0, 10_000.0));
        let target = lead_target(800.0, 0.0).0;
        assert!(first > 0.0 && first < target * 0.1, "eases in: {first}");
        for _ in 0..240 {
            leads.update(1.0 / 60.0, &world, &[PLAYER]);
        }
        let (settled, _) = leads.offset(PLAYER, (10_000.0, 10_000.0));
        assert!((settled - target).abs() < 1.0, "{settled} vs {target}");
        // Capped in a small (split-screen) view.
        let (capped, _) = leads.offset(PLAYER, (320.0, 180.0));
        assert!(320.0f32.mul_add(-MAX_LEAD_FRACTION_X, capped).abs() < f32::EPSILON);
        // A sudden stop glides back rather than snapping.
        world.player_mut(PLAYER).expect("player").velocity_x = 0.0;
        leads.update(1.0 / 60.0, &world, &[PLAYER]);
        let (gliding, _) = leads.offset(PLAYER, (10_000.0, 10_000.0));
        assert!(gliding > settled * 0.9, "{gliding}");
        // Seats that leave lose their lead.
        leads.update(1.0 / 60.0, &world, &[]);
        assert_eq!(leads.offset(PLAYER, (10_000.0, 10_000.0)), (0.0, 0.0));
    }
}
