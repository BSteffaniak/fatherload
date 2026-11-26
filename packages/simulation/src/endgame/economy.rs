//! The space credit scale (Wave 13 endgame): one purse from the Heart to the stars.
//!
//! Credits are one purse. A rig reaches space after the Heart, and the Descent probe
//! ([`crate::balance::descent::run_descent`], [`late_purse`]) leaves it with about 6.7 million
//! credits and an everyday Heart trip worth about a million an hour. Wave 12's stations priced
//! space from nothing (a first ship-line tier about an hour of a first trip out, missions
//! 1,000-3,500), so a post-Heart purse bought every line at once and a bounty paid less than a
//! minute at the Heart.
//!
//! The decision (the Wave 12 stations review's first option): **every station figure is scaled
//! by one constant**, [`SPACE_CREDIT_SCALE`], matched to a late rig's income: ship lines, planet
//! gear, alien tech, flight gear, arms, trade goods, mission rewards, rounds at the bar,
//! salvage, wrecks and bounties, pirate demands, alien goods, lessons and home plots. Ratios
//! inside space are unchanged (the first engine tier is still about forty minutes of a first
//! trip out from an empty purse), but an hour of space work now earns what an hour at the
//! Heart does. Two things stay unscaled on purpose:
//!
//! * **Services by the unit** (fuel, hull points): the tank and plating they fill already grew
//!   with the rig.
//! * **Home ore sold at stations** pays its classic value at the station's ore rate, far under
//!   its Descent premium at home: hauling Heart ore to an alien station no longer out-earns the
//!   boards.
//!
//! Space materials ([`super::materials`]) are priced on this scale from the start. The late
//! goals ([`super::works`]) then ask for credits *and* materials, so a big purse is a head start,
//! never the finish: see [`super::curve`] for the two documented curves.

use std::sync::OnceLock;

/// The one constant every station figure is scaled by (see the module docs).
pub const SPACE_CREDIT_SCALE: u32 = 20;

/// `base` credits on the space scale.
#[must_use]
pub const fn space_credits(base: u32) -> u32 {
    base.saturating_mul(SPACE_CREDIT_SCALE)
}

/// A late rig arriving in space: the purse it carries and what its everyday work earned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatePurse {
    /// Credits when it leaves the Heart.
    pub purse: f64,
    /// Credits an hour of everyday mining at the Heart earns (after fuel).
    pub credits_per_hour: f64,
    /// Hours of play to get there.
    pub hours: f64,
}

/// The purse the Descent probe carries out of the Heart (memoised: pure in the registry).
///
/// The probe is the Wave 7-11 balance bot (no calibration through the tick, so it is cheap and
/// deterministic): its purse when it leaves the last stratum, and its last everyday trip's
/// credits over its minutes.
#[must_use]
pub fn late_purse() -> LatePurse {
    static LATE: OnceLock<LatePurse> = OnceLock::new();
    *LATE.get_or_init(|| {
        let config = crate::balance::descent::DescentConfig {
            strata_limit: None,
            calibrate: false,
            tick_rate_hz: crate::DEFAULT_TICK_RATE_HZ,
        };
        let run = crate::balance::descent::run_descent(
            &crate::balance::descent::RegistryStrata::default_world(),
            &config,
        );
        // The everyday income: Descent VIII's trips (credits over minutes, all its strata).
        let last = run.strata.last();
        let heart = last.map_or(0, |stratum| stratum.stratum.descent);
        let (credits, minutes) = run
            .strata
            .iter()
            .filter(|stratum| stratum.stratum.descent == heart && stratum.trip_minutes > 0.0)
            .fold((0.0, 0.0), |(credits, minutes), stratum| {
                (
                    credits + stratum.trip_credits,
                    minutes + stratum.trip_minutes,
                )
            });
        LatePurse {
            purse: last.map_or(0.0, |stratum| stratum.purse),
            credits_per_hour: if minutes > 0.0 {
                credits * 60.0 / minutes
            } else {
                0.0
            },
            hours: run.total_hours(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_rig_carries_millions_and_earns_about_a_million_an_hour() {
        let late = late_purse();
        assert!(
            (3_000_000.0..15_000_000.0).contains(&late.purse),
            "purse {}",
            late.purse
        );
        assert!(
            (400_000.0..2_500_000.0).contains(&late.credits_per_hour),
            "income {}",
            late.credits_per_hour
        );
        assert!(late.hours > 60.0);
        // The scale puts an hour of space work (Wave 12's 12,000-18,000 an hour once the
        // boards open up) near an hour at the Heart.
        let space_hour = f64::from(space_credits(15_000));
        assert!(space_hour * 4.0 > late.credits_per_hour * 0.5);
    }
}
