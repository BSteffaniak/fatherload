//! Player-facing words for night-time gameplay (WP-NIGHT): what each Moon means for the night
//! ahead and the dawn summary. Pure text, no drawing.

use fatherload_simulation::clock::MoonPhase;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_moon_has_a_warning_and_dawn_counts_kills() {
        for moon in MoonPhase::ALL {
            assert!(!moon_warning(moon).is_empty());
        }
        assert!(dawn_detail(3).starts_with('3'));
        assert!(dawn_detail(0).contains("burrow"));
    }
}
