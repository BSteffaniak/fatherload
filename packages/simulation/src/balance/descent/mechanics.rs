//! What the descent probe models of Descents IV-VI's movement rules (Wave 9 progression).
//!
//! Submarine travel in the drowned strata, flight across the inner sky, turned gravity, mirrored
//! controls, slow time and the dark, plus the deep supplies each stratum spends and the gate
//! bosses' fights.
//!
//! **Read from the registry, not from names.** Every factor comes from the stratum's
//! [`crate::descent::StratumDef`]: its drag and water pockets (submarine travel), a
//! [`LandmarkKind::HollowCavern`] (the open inner sky), its [`FieldRegions`] (turned gravity
//! and mirrored cells), its time scale and its light. When the Descent tracks author their
//! strata, the probe measures their real fields with no change here. Descents I-III keep their
//! Wave 8 model (their pockets are swum or drilled around, not crossed by submarine), so their
//! measured hours stand.

use crate::{
    depth_progression::RigSystem,
    descent::{DescentBoss, FieldRegions, LandmarkKind, Stratum},
    fluid::FluidKind,
    workshop::DeepSupply,
};

/// Share of normal speed a rig keeps under water in submarine mode (ballast, drag, currents).
pub const SUBMARINE_SPEED_SHARE: f64 = 0.55;
/// Share of the Inner Sky's rows that is open air (the cavern under the rock sky).
pub const INNER_SKY_OPEN_SHARE: f64 = 0.55;
/// Rows per second a rig flies across open sky against the wind (thrusting the whole way).
pub const SKY_FLIGHT_ROWS_PER_SECOND: f64 = 9.0;
/// How much longer moving and digging take in a turned-gravity cell (rotating the craft with
/// its thrusters, landing on walls and ceilings), and with the Gravity Anchor fitted.
pub const GRAVITY_TURN_SLOWDOWN: f64 = 0.8;
pub const GRAVITY_TURN_SLOWDOWN_ANCHORED: f64 = 0.3;
/// How much longer moving and digging take in a mirrored cell (relearning left and right).
pub const MIRROR_SLOWDOWN: f64 = 0.5;
/// How much longer digging takes in near-total dark, and with the Sonar Suite's pings.
pub const DARK_SLOWDOWN: f64 = 0.25;
pub const DARK_SLOWDOWN_SONAR: f64 = 0.08;
/// Light (per mille of daylight) at or below which a stratum counts as dark.
pub const DARK_LIGHT_PERMILLE: u16 = 200;
/// Light (per mille of daylight) at or above which the inner sun's glare sends a rig the long
/// way round, shadow to shadow (a shade canopy along).
///
/// The authored Sunforge shines at 1300 (Wave 9 integration), the Inner Sky at 1250.
pub const GLARE_LIGHT_PERMILLE: u16 = 1_300;
/// How much longer moving and digging take in the glare.
pub const GLARE_MOVE_SLOWDOWN: f64 = 0.25;
pub const GLARE_DIG_SLOWDOWN: f64 = 0.1;
/// How much longer each tile takes where the ore sits on floating stones in open sky (flying
/// from stone to stone between bites).
pub const STONE_HOP_SLOWDOWN: f64 = 0.8;

/// The movement rules of one stratum as the descent bot meets them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StratumMechanics {
    /// Share of a route crossed under water in submarine mode.
    pub water_share: f64,
    /// Share of the stratum's rows that is open sky, flown rather than dug.
    pub open_share: f64,
    /// Share of the cells whose gravity is turned.
    pub gravity_share: f64,
    /// Share of the cells whose controls and screen are mirrored.
    pub mirror_share: f64,
    /// The field's time scale (1.0 is normal; slow time makes everything take longer).
    pub time_scale: f64,
    /// Near-total darkness.
    pub dark: bool,
    /// The inner sun's glare.
    pub glare: bool,
    /// The deep supply its mechanic spends, if any.
    pub supply: Option<DeepSupply>,
}

impl Default for StratumMechanics {
    fn default() -> Self {
        Self {
            water_share: 0.0,
            open_share: 0.0,
            gravity_share: 0.0,
            mirror_share: 0.0,
            time_scale: 1.0,
            dark: false,
            glare: false,
            supply: None,
        }
    }
}

impl StratumMechanics {
    /// The movement rules of `stratum` from its registry definition (Descents IV-VI and below;
    /// the neutral rules above).
    #[must_use]
    pub fn of(stratum: Stratum) -> Self {
        if stratum.descent().index() < 3 {
            return Self::default();
        }
        let def = stratum.def();
        let field = &def.field;
        let pockets: u32 = def
            .fluids
            .iter()
            .filter(|rule| matches!(rule.kind, FluidKind::Water | FluidKind::Brine))
            .map(|rule| u32::from(rule.per_mille))
            .sum();
        let water_share = if field.drag_permille > 1_000 {
            0.75
        } else {
            (f64::from(pockets) / 1_000.0).min(0.5)
        };
        let open_share = if def.landmarks.contains(&LandmarkKind::HollowCavern) {
            INNER_SKY_OPEN_SHARE
        } else {
            0.0
        };
        let (gravity_share, mirror_share) = match field.regions {
            FieldRegions::GravityTurns { per_mille, .. } => (f64::from(per_mille) / 1_000.0, 0.0),
            FieldRegions::Mirrored { per_mille } => (0.0, f64::from(per_mille) / 1_000.0),
            // The distortion strata's authored halls (Wave 9 integration): about as much of each
            // stratum as the placeholders' cells covered (mirror halls 40%, shear halls 45%); Pixel
            // Rot changes the view, not the movement.
            FieldRegions::Authored(crate::descent::AuthoredRegions::MirrorHalls) => (0.0, 0.4),
            FieldRegions::Authored(crate::descent::AuthoredRegions::ShearHalls) => (0.45, 0.0),
            FieldRegions::None
            | FieldRegions::Authored(crate::descent::AuthoredRegions::RotTiers) => (0.0, 0.0),
        };
        // Authored rules the registry's fields leave out (Wave 9 integration): the Underground
        // Ocean's basins and the isles' turned gravity.
        let water_share =
            water_share.max(f64::from(crate::drowned::submarine_share_permille(stratum)) / 1_000.0);
        let gravity_share = gravity_share
            .max(f64::from(crate::hollow::turned_gravity_share_permille(stratum)) / 1_000.0);
        let mirror_share = if field.mirror_controls {
            1.0
        } else {
            mirror_share
        };
        Self {
            water_share,
            open_share,
            gravity_share,
            mirror_share,
            time_scale: (f64::from(field.time_scale_permille) / 1_000.0).clamp(0.25, 4.0),
            dark: field.light_permille <= DARK_LIGHT_PERMILLE,
            glare: field.light_permille >= GLARE_LIGHT_PERMILLE,
            supply: DeepSupply::ALL
                .iter()
                .copied()
                .find(|supply| supply.stratum() == stratum),
        }
    }

    /// Whether anything here changes how the bot moves.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self == &Self::default()
            || (self.water_share == 0.0
                && self.open_share == 0.0
                && self.gravity_share == 0.0
                && self.mirror_share == 0.0
                && (self.time_scale - 1.0).abs() < f64::EPSILON
                && !self.dark
                && !self.glare)
    }

    fn handling(&self, systems: u8) -> f64 {
        let gravity = if systems & RigSystem::GravityAnchor.bit() != 0 {
            GRAVITY_TURN_SLOWDOWN_ANCHORED
        } else {
            GRAVITY_TURN_SLOWDOWN
        };
        self.gravity_share.mul_add(gravity, 1.0) * self.mirror_share.mul_add(MIRROR_SLOWDOWN, 1.0)
            / self.time_scale
    }

    /// Multiplier on the time a trip spends travelling, for a rig with `systems` fitted.
    #[must_use]
    pub fn move_factor(&self, systems: u8) -> f64 {
        let water = self.water_share / SUBMARINE_SPEED_SHARE + (1.0 - self.water_share);
        let glare = if self.glare { GLARE_MOVE_SLOWDOWN } else { 0.0 };
        water * (1.0 + glare) * self.handling(systems)
    }

    /// Multiplier on the time each tile takes to dig, for a rig with `systems` fitted.
    #[must_use]
    pub fn dig_factor(&self, systems: u8) -> f64 {
        let dark = if !self.dark {
            0.0
        } else if systems & RigSystem::SonarSuite.bit() != 0 {
            DARK_SLOWDOWN_SONAR
        } else {
            DARK_SLOWDOWN
        };
        let glare = if self.glare { GLARE_DIG_SLOWDOWN } else { 0.0 };
        self.water_share.mul_add(0.3, 1.0)
            * self.open_share.mul_add(STONE_HOP_SLOWDOWN, 1.0)
            * (1.0 + dark + glare)
            * self.handling(systems)
    }

    /// Rows of a stratum of `rows` that are open sky (flown, not dug).
    #[must_use]
    pub fn open_rows(&self, rows: f64) -> f64 {
        rows * self.open_share
    }

    /// Seconds of thrusting flight across the open sky of a stratum of `rows`.
    #[must_use]
    pub fn flight_seconds(&self, rows: f64) -> f64 {
        self.open_rows(rows) / SKY_FLIGHT_ROWS_PER_SECOND
    }

    /// Minutes of learning the first time a rule is met (a first dive in submarine mode, the
    /// first crossing of the inner sky, the first turned room...), and the rules' names.
    #[must_use]
    pub fn firsts(&self) -> Vec<(&'static str, f64)> {
        let mut firsts = Vec::new();
        if self.water_share >= 0.5 {
            firsts.push(("submarine travel", 10.0));
        }
        if self.open_share > 0.0 {
            firsts.push(("inner-sky flight", 12.0));
        }
        if self.gravity_share > 0.0 {
            firsts.push(("turned gravity", 10.0));
        }
        if self.mirror_share > 0.0 {
            firsts.push(("mirrored controls", 8.0));
        }
        if (self.time_scale - 1.0).abs() > f64::EPSILON {
            firsts.push(("time zones", 8.0));
        }
        if self.dark {
            firsts.push(("total dark", 6.0));
        }
        if self.glare {
            firsts.push(("the inner sun's glare", 6.0));
        }
        firsts
    }

    /// Deep supplies the bot spends per trip here, and for crossing the stratum.
    #[must_use]
    pub const fn supplies_per_trip(&self) -> (u32, u32) {
        if self.supply.is_some() {
            (1, 2)
        } else {
            (0, 0)
        }
    }
}

/// One gate boss's fight as the bot plays it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BossFight {
    /// Minutes from the first attempt to the win (attempts and the walk back included).
    pub minutes: f64,
    /// Attempts that ended in a wreck (each a death and a rescue).
    pub deaths: u32,
}

/// One gate boss's fight as its scripted bot test wins it (Wave 10 polish: measured, not
/// guessed; `world::*_tests`' bot fights hold their rows with `record_bot_fight`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BotFight {
    /// World seconds from the boss waking to its fall.
    pub seconds: f64,
    /// The share of its hull the bot's rig kept (a tight fight costs a player more tries).
    pub hull_left: f64,
}

/// The measured bot fights.
///
/// The rigs are fitted as the probe's rig arrives: Descent kit, the hull tier the probe reaches
/// each gate with. `None`: the Sinkhole (an escape, not a fight).
#[must_use]
pub const fn bot_fight(boss: DescentBoss) -> Option<BotFight> {
    match boss {
        // Measured on this branch (30 Hz bot fights, Wave 10 polish).
        DescentBoss::StarCore => Some(BotFight {
            seconds: 95.0,
            hull_left: 1.0,
        }),
        DescentBoss::MycelialMind => Some(BotFight {
            // Re-measured once shots hit a walker's drawn body over its feet (Wave 10 review).
            seconds: 118.0,
            hull_left: 0.64,
        }),
        DescentBoss::Leviathan => Some(BotFight {
            seconds: 64.0,
            hull_left: 0.67,
        }),
        // The serpent's orbit is long: most of the fight is waiting for the scales' passes.
        DescentBoss::SunSerpent => Some(BotFight {
            seconds: 298.0,
            hull_left: 0.76,
        }),
        DescentBoss::TheGlitch => Some(BotFight {
            seconds: 96.0,
            hull_left: 0.43,
        }),
        // Measured at the integration (30 Hz bot fights, Wave 10).
        DescentBoss::GatekeeperColossus => Some(BotFight {
            seconds: 133.0,
            hull_left: 0.71,
        }),
        // The bot fights the Heart with the top railgun, as the probe's rig arrives.
        DescentBoss::TheHeart => Some(BotFight {
            seconds: 69.0,
            hull_left: 0.63,
        }),
        DescentBoss::TheSinkhole => None,
    }
}

/// How many times longer a player takes than the scripted bot over the same fight.
///
/// The bot knows every tell, never misses its line and never stops to look (players watched on
/// the Wave 8-9 bosses take three to five times as long).
pub const PLAYER_FIGHT_PACE: f64 = 4.0;
/// Minutes around a fight the bot does not count: finding the arena, reading it, the walk out
/// with the reward.
pub const FIGHT_SETUP_MINUTES: f64 = 6.0;
/// A bot that kept less than this share of its hull won a tight fight: a player fights it
/// [`TIGHT_FIGHT_CARE`] times as long.
pub const TIGHT_FIGHT_HULL: f64 = 0.4;
/// How much longer a tight fight's attempts run.
pub const TIGHT_FIGHT_CARE: f64 = 1.35;

/// Minutes of `boss`'s winning attempt, and the attempts a player learning its patterns
/// spends before it (every gate boss is fair and has no countdown; the deep ones take a try or
/// two to read).
///
/// Wave 10 polish: a boss with a bot fight is modelled from it (its measured seconds at
/// [`PLAYER_FIGHT_PACE`] plus [`FIGHT_SETUP_MINUTES`], a tight fight's at
/// [`TIGHT_FIGHT_CARE`]; one learning try); the rest keep Wave 9's modelled minutes.
#[must_use]
pub fn boss_pattern(boss: DescentBoss) -> (f64, u32) {
    if let Some(fight) = bot_fight(boss) {
        // A tight fight is fought more carefully: its attempts run a third longer.
        let care = if fight.hull_left < TIGHT_FIGHT_HULL {
            TIGHT_FIGHT_CARE
        } else {
            1.0
        };
        let minutes = (fight.seconds / 60.0 * care).mul_add(PLAYER_FIGHT_PACE, FIGHT_SETUP_MINUTES);
        return (minutes, 1);
    }
    match boss {
        DescentBoss::TheSinkhole => (14.0, 0),
        DescentBoss::StarCore => (16.0, 1),
        DescentBoss::MycelialMind => (18.0, 1),
        DescentBoss::Leviathan => (22.0, 1),
        DescentBoss::SunSerpent => (24.0, 1),
        DescentBoss::TheGlitch => (30.0, 1),
        DescentBoss::GatekeeperColossus => (32.0, 1),
        DescentBoss::TheHeart => (34.0, 1),
    }
}

/// Hull tiers below which a rig is under-armoured for `boss` (each tier short is one more
/// wrecked attempt, at most two).
#[must_use]
pub const fn boss_ready_hull(boss: DescentBoss) -> u8 {
    let descent = match boss {
        DescentBoss::TheSinkhole => 0,
        DescentBoss::StarCore => 1,
        DescentBoss::MycelialMind => 2,
        DescentBoss::Leviathan => 3,
        DescentBoss::SunSerpent => 4,
        DescentBoss::TheGlitch => 5,
        DescentBoss::GatekeeperColossus => 6,
        DescentBoss::TheHeart => 7,
    };
    crate::depth_progression::core_tier_cap(descent).saturating_sub(4)
}

/// `boss`'s fight for a rig of hull tier `hull`.
///
/// The winning attempt, the learning attempts and one more for each tier of armour short (at
/// most two), each failed attempt costing 40 % of a fight and two minutes back from the rescue
/// point.
#[must_use]
pub fn boss_fight(boss: DescentBoss, hull: u8) -> BossFight {
    let (minutes, learning) = boss_pattern(boss);
    let short = u32::from(boss_ready_hull(boss).saturating_sub(hull)).min(2);
    let deaths = learning + short;
    BossFight {
        minutes: f64::from(deaths).mul_add(minutes.mul_add(0.4, 2.0), minutes),
        deaths,
    }
}

/// A bot fight test's measured win (Wave 10 polish): prints it and holds it to the probe's
/// [`BOT_FIGHTS`] row, so the probe's fight minutes follow the real fight. A boss rework that
/// moves its fight by more than the tolerance fails here until the row is re-measured.
#[cfg(test)]
pub(crate) fn record_bot_fight(boss: DescentBoss, seconds: f64, hull_left: f64) {
    println!(
        "BOT FIGHT {boss:?}: won in {seconds:.0} s with {:.0} % hull left",
        hull_left * 100.0
    );
    let Some(row) = bot_fight(boss) else {
        panic!("{boss:?} has no measured bot fight row");
    };
    assert!(
        seconds >= row.seconds * 0.6 && seconds <= row.seconds * 1.5,
        "{boss:?}: the bot won in {seconds:.0} s, the probe models {:.0} s: re-measure BOT_FIGHTS",
        row.seconds
    );
    assert!(
        (hull_left - row.hull_left).abs() <= 0.25,
        "{boss:?}: the bot kept {:.0} % hull, the probe models {:.0} %: re-measure BOT_FIGHTS",
        hull_left * 100.0,
        row.hull_left * 100.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descent::Descent;

    #[test]
    fn the_registry_fields_become_movement_rules() {
        let ocean = StratumMechanics::of(Stratum::UndergroundOcean);
        assert!(
            ocean.water_share >= 0.5,
            "the ocean is crossed by submarine"
        );
        assert!(ocean.move_factor(0) > 1.5);
        let sky = StratumMechanics::of(Stratum::InnerSky);
        assert!(sky.open_share > 0.0);
        assert!(sky.flight_seconds(600.0) > 0.0);
        let isles = StratumMechanics::of(Stratum::UpsideDownIsles);
        assert!(isles.gravity_share > 0.0);
        let shear = StratumMechanics::of(Stratum::GravityShear);
        assert!(shear.move_factor(RigSystem::GravityAnchor.bit()) < shear.move_factor(0));
        let mirror = StratumMechanics::of(Stratum::MirrorStrata);
        assert!(mirror.mirror_share > 0.0);
        let sludge = StratumMechanics::of(Stratum::TimeSludge);
        assert!(sludge.time_scale < 1.0 && sludge.dig_factor(0) > 1.0);
        let forge = StratumMechanics::of(Stratum::TheSunforge);
        assert!(forge.glare && forge.move_factor(0) > 1.0);
        let echo = StratumMechanics::of(Stratum::EchoCaves);
        assert!(echo.dark);
        assert!(echo.dig_factor(RigSystem::SonarSuite.bit()) < echo.dig_factor(0));
        // Descents I-III keep the Wave 8 model.
        for stratum in Stratum::ALL.iter().take(17) {
            assert!(StratumMechanics::of(*stratum).is_neutral(), "{stratum:?}");
        }
        // Every deep supply belongs to a stratum that spends it.
        for supply in DeepSupply::ALL {
            assert_eq!(StratumMechanics::of(supply.stratum()).supply, Some(supply));
        }
    }

    #[test]
    fn boss_fights_are_winnable_and_cost_more_when_underarmoured() {
        for descent in Descent::ALL.iter().skip(1) {
            let Some(gate) = descent.last_stratum().def().gate else {
                continue;
            };
            let ready = boss_fight(gate.boss, boss_ready_hull(gate.boss));
            let short = boss_fight(gate.boss, 0);
            assert!(ready.deaths <= 1, "{:?}", gate.boss);
            assert!(short.deaths <= 3 && short.minutes > ready.minutes);
            assert!(ready.minutes >= FIGHT_SETUP_MINUTES && short.minutes < 90.0);
            // Wave 10 polish: a boss with a bot fight is modelled from its measured win.
            if let Some(fight) = bot_fight(gate.boss) {
                let (minutes, _) = boss_pattern(gate.boss);
                assert!(
                    minutes >= (fight.seconds / 60.0).mul_add(PLAYER_FIGHT_PACE, 0.0),
                    "{:?}",
                    gate.boss
                );
            }
        }
    }
}
