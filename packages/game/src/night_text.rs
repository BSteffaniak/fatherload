//! Player-facing words for night-time gameplay (WP-NIGHT): what each Moon means for the night
//! ahead and the dawn summary. Pure text, no drawing.

use std::collections::BTreeSet;

use fatherload_simulation::{PlayerId, clock::MoonPhase};

/// The warning under the "NIGHT FALLS" banner: what the Moon brings tonight.
#[must_use]
pub const fn moon_warning(moon: MoonPhase) -> &'static str {
    match moon {
        MoonPhase::Full => "A full moon: more of them walk tonight. Keep to the light.",
        MoonPhase::WaxingGibbous | MoonPhase::WaningGibbous => {
            "A bright moon: the night is restless. Lamps and walls keep them off."
        }
        MoonPhase::New => "A new moon: the dark is deep, and something rare walks tonight.",
        MoonPhase::FirstQuarter
        | MoonPhase::LastQuarter
        | MoonPhase::WaxingCrescent
        | MoonPhase::WaningCrescent => {
            "Things stir on the open ground. Towns, camps and lit bases are safe."
        }
    }
}

/// The line under the "DAWN" banner.
#[must_use]
pub fn dawn_detail(kills: u32) -> String {
    match kills {
        0 => "The night things burrow and flee as the sun comes up.".to_owned(),
        1 => "One night monster fell. The rest burrow and flee.".to_owned(),
        count => format!("{count} night monsters fell. The rest burrow and flee."),
    }
}

/// Who should see tonight's "DAWN" banner ("You made it through the night"): nobody when the
/// night was slept away (skipped), and never a player who lay in bed through it (a player
/// who slept while a crewmate kept watch did not make it through anything).
///
/// Presentation only: the game watches who is in bed each frame while a night is on.
#[derive(Clone, Debug, Default)]
pub struct DawnWitnesses {
    slept: BTreeSet<PlayerId>,
}

impl DawnWitnesses {
    /// A night fell: nobody has slept in it yet.
    pub fn night_fell(&mut self) {
        self.slept.clear();
    }

    /// `in_bed` are in bed this frame.
    pub fn observe(&mut self, in_bed: impl IntoIterator<Item = PlayerId>) {
        self.slept.extend(in_bed);
    }

    /// Whether `player` sees the dawn banner (`night_skipped`: the night was slept away).
    #[must_use]
    pub fn sees_dawn(&self, player: PlayerId, night_skipped: bool) -> bool {
        !night_skipped && !self.slept.contains(&player)
    }

    /// The dawn came: the next night starts afresh.
    pub fn dawn(&mut self) {
        self.slept.clear();
    }
}

/// Seconds after a night skip during which a dawn counts as slept away (the fade back in).
pub const SKIPPED_DAWN_SECONDS: f32 = 5.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dawn_banner_skips_sleepers_and_skipped_nights() {
        let (watch, sleeper) = (PlayerId::new(0), PlayerId::new(1));
        let mut witnesses = DawnWitnesses::default();
        witnesses.night_fell();
        witnesses.observe([sleeper]);
        witnesses.observe([]);
        // The one who kept watch made it through; the one in bed slept through it.
        assert!(witnesses.sees_dawn(watch, false));
        assert!(!witnesses.sees_dawn(sleeper, false));
        // A night slept away shows nobody the banner.
        assert!(!witnesses.sees_dawn(watch, true));
        // The next night starts afresh.
        witnesses.dawn();
        witnesses.night_fell();
        assert!(witnesses.sees_dawn(sleeper, false));
    }

    #[test]
    fn every_moon_has_a_warning_and_dawn_counts_kills() {
        for moon in MoonPhase::ALL {
            assert!(!moon_warning(moon).is_empty());
        }
        assert!(dawn_detail(3).starts_with('3'));
        assert!(dawn_detail(0).contains("burrow"));
    }
}
