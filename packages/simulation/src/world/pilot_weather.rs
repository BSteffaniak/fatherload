//! The deep's forces on a pilot on foot (Wave 10 polish): Wave 9's currents, geysers, frost,
//! wind and the Gravity Anchor acted on rigs only. A pilot climbing out now feels them too,
//! scaled to a body on foot (the pilot stays upright; only its position and speed change):
//!
//! * **Currents** (the Underground Ocean's gyres, the Leviathan's maelstrom) carry a swimming
//!   pilot at [`PILOT_CURRENT_SHARE`] of the rig's drift (a swimmer is lighter than a sub but
//!   swims against it).
//! * **Geysers** throw a pilot standing in an erupting jet up the shaft at
//!   [`PILOT_GEYSER_SHARE`] of the rig's lift, and scald one at the throat.
//! * **Frost**: in the Brine Glaciers the cold stiffens a pilot's suit: its walk slows by up to
//!   [`PILOT_CHILL_MAX`] deeper in the ice, not at all by a thermal vent, in an outpost or the
//!   city, or for the crew of a rig with the Cryo Hull (its heater warms the suit).
//! * **Wind**: the Inner Sky's wind carries an airborne pilot (a jump, a glide, a fall) at
//!   [`PILOT_WIND_SHARE`] of its speed.
//! * **The Gravity Anchor** works on foot: engaged out of the rig it plants the pilot's boots
//!   for its hold (plain downward gravity under any turned field, and no current, jet or wind
//!   moves it).
//!
//! Every push moves the pilot only where its box fits, from per-second rates and world time, so
//! every tick rate agrees.

use super::WorldState;
use crate::{
    PlayerId,
    depth_progression::RigSystem,
    descent::Stratum,
    drowned::{
        CURRENT_DRIFT, GEYSER_LIFT_SPEED, GEYSER_SCALD_DAMAGE_PER_SECOND, GEYSER_SCALD_ROWS,
        GeyserPhase, frost_rate,
    },
    entity::Body,
    pilot::PilotState,
    quantize::quantize_simulation_scalar as quantize,
};

/// A swimming pilot's share of the rig's current drift.
pub const PILOT_CURRENT_SHARE: f32 = 0.7;
/// A pilot's share of a geyser's lift.
pub const PILOT_GEYSER_SHARE: f32 = 0.8;
/// The most the glaciers' cold slows a pilot's walk (a share of its speed).
pub const PILOT_CHILL_MAX: f32 = 0.45;
/// An airborne pilot's share of the Inner Sky's wind.
pub const PILOT_WIND_SHARE: f32 = 0.6;

impl WorldState {
    /// Whether `player_id`'s pilot is on foot with the Gravity Anchor holding its boots.
    #[must_use]
    pub fn pilot_anchored(&self, player_id: PlayerId) -> bool {
        self.is_on_foot(player_id) && self.anchor_quarter_for(player_id).is_some()
    }

    /// How much the glaciers' cold slows `player_id`'s pilot at `(x, y)` (feet): a share of
    /// its walk, from nothing up to [`PILOT_CHILL_MAX`].
    #[must_use]
    pub fn pilot_chill(&self, player_id: PlayerId, x: f32, y: f32) -> f32 {
        let tile = self.frame().tile_at(x, y - 4.0);
        if self.deep_stratum_at(tile) != Some(Stratum::BrineGlaciers) {
            return 0.0;
        }
        let warm = self
            .players
            .get(&player_id)
            .is_some_and(|rig| rig.hazard_gear.has_system(RigSystem::CryoHull))
            || self.logistics.outpost_at(tile).is_some()
            || self.merrow_city().is_some_and(|site| site.in_city(tile))
            || self.near_vent(tile);
        if warm {
            return 0.0;
        }
        let Some(layout) = self.descent_strata_layout() else {
            return 0.0;
        };
        let rate = frost_rate(layout.along(tile.y));
        let top = frost_rate(1_000).max(1.0);
        (rate / top * PILOT_CHILL_MAX).clamp(0.0, PILOT_CHILL_MAX)
    }

    /// The deep's forces on a pilot after its step (see the module docs); returns the scald
    /// damage it took.
    pub(super) fn pilot_weather(
        &self,
        body: &mut Body,
        state: &PilotState,
        owner: PlayerId,
        dt: f32,
    ) -> f32 {
        if self.pilot_anchored(owner) {
            return 0.0;
        }
        let (mx, my) = state.middle(body);
        // A body's weather under the open sky (Wave 12 bodylife): its wind carries a pilot in
        // the air, and its hail (or a theme's acid and ash) stings the suit.
        let weather = self.body_weather_at(mx, my);
        if weather.kind != crate::bodylife::WeatherId::CLEAR {
            let mut damage = weather.suit_damage * dt;
            if !state.grounded && weather.wind != 0.0 {
                let push = ((weather.wind * PILOT_WIND_SHARE) * dt, 0.0);
                self.nudge_pilot(body, state, owner, push);
            }
            if !self.terrain.settings().generator.has_descents() {
                damage = crate::quantize::quantize_simulation_scalar(damage);
                return damage;
            }
        }
        if !self.terrain.settings().generator.has_descents() {
            return 0.0;
        }
        let mut push = (0.0_f32, 0.0_f32);
        let mut damage = 0.0;
        // Currents carry a swimmer.
        let wet = self.submersion(mx, my, state.height() * 0.5);
        if wet.share >= 0.5 {
            let (cx, cy) = self.current_at(mx, my);
            let drift = CURRENT_DRIFT * PILOT_CURRENT_SHARE * wet.share * dt;
            push.0 = cx.mul_add(drift, push.0);
            push.1 = cy.mul_add(drift, push.1);
        }
        // The Inner Sky's wind carries a pilot in the air.
        if !state.grounded
            && let Some(wind) = self.hollow_wind_at(mx, my)
        {
            push.0 = (wind * PILOT_WIND_SHARE).mul_add(dt, push.0);
        }
        // An erupting geyser throws the pilot up, or scalds it at the throat.
        if let Some(jet) = self
            .geyser_jet_at(body.x, body.y - 2.0)
            .filter(|jet| jet.phase == GeyserPhase::Erupting)
        {
            if jet.rows_above <= GEYSER_SCALD_ROWS {
                damage = GEYSER_SCALD_DAMAGE_PER_SECOND.mul_add(dt, damage);
            }
            let lift = if jet.great {
                GEYSER_LIFT_SPEED * 1.3
            } else {
                GEYSER_LIFT_SPEED
            } * PILOT_GEYSER_SHARE;
            body.velocity_y = quantize(body.velocity_y.min(-lift));
        }
        if push != (0.0, 0.0) {
            self.nudge_pilot(body, state, owner, push);
        }
        damage
    }

    /// Moves a pilot's body by `(dx, dy)` where its box fits (else along one axis, else not at
    /// all).
    fn nudge_pilot(&self, body: &mut Body, state: &PilotState, owner: PlayerId, delta: (f32, f32)) {
        let view = self.pilot_view(Some(owner));
        let height = state.height();
        for (dx, dy) in [delta, (delta.0, 0.0), (0.0, delta.1)] {
            if dx == 0.0 && dy == 0.0 {
                continue;
            }
            let (x, y) = (quantize(body.x + dx), quantize(body.y + dy));
            // A pilot standing on a floor touches it: judge a sideways push a pixel up.
            let probe = if dy == 0.0 { y - 1.0 } else { y };
            if !crate::pilot::box_blocked(&view, x, probe, height) {
                body.x = x;
                body.y = y;
                return;
            }
        }
    }
}
