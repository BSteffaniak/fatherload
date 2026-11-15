//! Contractors: people who build homes (the person-role half of WP-HOME, expansion plan §10).
//!
//! Wave 4's contractor-built homes hire a person from a village to build on a plot over in-game
//! days. This module is the people side of that, ready to hire later:
//!
//! - [`contractor_profile`] says whether a character takes building work and how they work:
//!   their [`ContractorStyle`] (fast and loose, slow and perfect, chatty, grumpy, steady), a
//!   speed (percent of the usual build time), a quality (1-5) and a price (percent of the usual
//!   price), all rolled from their personality, so a character is always the same contractor.
//! - [`ContractorProfile::quote`] turns a [`BuildJob`] (the blueprint's usual days and price)
//!   into a [`ContractQuote`].
//! - [`ContractorHire`] is the record WP-HOME keeps for a hired contractor (who, which job, the
//!   quote, when work started on the clock). Construction progresses in world time only
//!   (§11 decision 3); the clock is [`super::clock`].
//! - [`ContractorProfile::pitch`] is what they say when asked about building work (the talk
//!   panel's "Ask about building work" topic).
//!
//! Contractors are ordinary residents: they live, walk and talk like everyone else, and they
//! remember you (their [`super::CharacterState`]).

use serde::{Deserialize, Serialize};

use super::{Character, CharacterId, PeopleRng, Role, clock::DayTime};

const SALT_CONTRACTOR: u64 = 0x5045_4F50_4C45_0030;

/// How a contractor works.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum ContractorStyle {
    /// Quick and cheap, corners cut.
    FastAndLoose,
    /// Slow and dear, but it will outlast you.
    SlowAndPerfect,
    /// Talks the whole time; the work gets done eventually.
    Chatty,
    /// Grumbles, charges extra, builds well.
    Grumpy,
    /// No surprises either way.
    Steady,
}

impl ContractorStyle {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FastAndLoose => "fast and loose",
            Self::SlowAndPerfect => "slow and perfect",
            Self::Chatty => "chatty",
            Self::Grumpy => "grumpy",
            Self::Steady => "steady",
        }
    }
}

/// A character's terms as a contractor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ContractorProfile {
    pub character: CharacterId,
    pub style: ContractorStyle,
    /// Percent of a job's usual build time they take (lower is faster).
    pub speed_percent: u16,
    /// 1 (it stands) to 5 (a masterpiece).
    pub quality: u8,
    /// Percent of a job's usual price they charge.
    pub price_percent: u16,
}

/// A building job as a blueprint describes it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct BuildJob {
    /// The usual build time in tenths of an in-game day (a cabin: 10).
    pub tenth_days: u32,
    /// The usual price in credits.
    pub price: u32,
}

impl BuildJob {
    /// The plan's cabin: about a day.
    pub const CABIN: Self = Self {
        tenth_days: 10,
        price: 2_500,
    };
    /// The plan's house: about three days.
    pub const HOUSE: Self = Self {
        tenth_days: 30,
        price: 8_000,
    };
    /// The plan's manor: about a week.
    pub const MANOR: Self = Self {
        tenth_days: 70,
        price: 24_000,
    };
}

/// What a contractor would charge and take for a job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ContractQuote {
    pub tenth_days: u32,
    pub price: u32,
    pub quality: u8,
}

/// A hired contractor (WP-HOME keeps one per contracted build).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ContractorHire {
    pub contractor: ContractorProfile,
    pub job: BuildJob,
    pub quote: ContractQuote,
    /// When work started (day and minute on the world clock).
    pub started_day: u32,
    pub started_minute: u16,
}

impl ContractorHire {
    /// Hires `contractor` for `job` at `now`.
    #[must_use]
    pub const fn new(contractor: ContractorProfile, job: BuildJob, now: DayTime) -> Self {
        Self {
            contractor,
            job,
            quote: contractor.quote(job),
            started_day: now.day,
            started_minute: now.minute,
        }
    }

    /// Progress at `now` in thousandths (0 on the day work starts, 1000 when it is done).
    #[must_use]
    pub fn progress_per_mille(&self, now: DayTime) -> u32 {
        let start = u64::from(self.started_day) * 1_440 + u64::from(self.started_minute);
        let at = u64::from(now.day) * 1_440 + u64::from(now.minute);
        let elapsed = at.saturating_sub(start);
        let total = u64::from(self.quote.tenth_days) * 144;
        let total = if total == 0 { 1 } else { total };
        let done = elapsed * 1_000 / total;
        u32::try_from(done.min(1_000)).unwrap_or(1_000)
    }
}

impl ContractorProfile {
    /// Their quote for `job`.
    #[must_use]
    pub const fn quote(&self, job: BuildJob) -> ContractQuote {
        ContractQuote {
            tenth_days: (job.tenth_days * self.speed_percent as u32).div_ceil(100),
            price: (job.price * self.price_percent as u32).div_ceil(100),
            quality: self.quality,
        }
    }

    /// What they say when asked about building work.
    #[must_use]
    pub fn pitch(&self, character: &Character) -> String {
        let quote = self.quote(BuildJob::CABIN);
        let days = days_phrase(quote.tenth_days);
        let price = quote.price;
        match self.style {
            ContractorStyle::FastAndLoose => format!(
                "A cabin? {days}, {price} credits, and I don't dawdle. Measure twice? I measure once and hope."
            ),
            ContractorStyle::SlowAndPerfect => format!(
                "I could raise you a cabin in {days}, for {price} credits. Every joint true. Rushing is how roofs come down."
            ),
            ContractorStyle::Chatty => format!(
                "Building work! Oh, I love building work. A cabin, {days}, {price} credits, and I'll tell you about my {} the whole time.",
                character.role.name()
            ),
            ContractorStyle::Grumpy => {
                format!("Hmph. Cabin. {days}. {price} credits. Don't stand over me while I work.")
            }
            ContractorStyle::Steady => format!(
                "I do take building work. A cabin runs {days} and {price} credits. No surprises, that's my promise."
            ),
        }
    }
}

/// "about a day", "two days", "half a day".
fn days_phrase(tenth_days: u32) -> String {
    match tenth_days {
        0..=6 => "half a day".to_owned(),
        7..=9 => "under a day".to_owned(),
        10..=12 => "about a day".to_owned(),
        13..=17 => "a day and a half".to_owned(),
        _ => format!("{} days", tenth_days.div_ceil(10)),
    }
}

/// Whether `character` takes building work, and on what terms.
#[must_use]
pub fn contractor_profile(character: &Character) -> Option<ContractorProfile> {
    if !matches!(
        character.role,
        Role::Smith
            | Role::Mechanic
            | Role::Tinkerer
            | Role::Farmer
            | Role::Apprentice
            | Role::RetiredMiner
            | Role::Warden
    ) {
        return None;
    }
    let p = character.personality;
    let mut rng = PeopleRng::new(character.voice_seed, SALT_CONTRACTOR, 0, 0);
    let style = if p.conscientiousness >= 70 {
        ContractorStyle::SlowAndPerfect
    } else if p.conscientiousness <= 32 {
        ContractorStyle::FastAndLoose
    } else if p.agreeableness <= 35 {
        ContractorStyle::Grumpy
    } else if p.extraversion >= 66 {
        ContractorStyle::Chatty
    } else {
        ContractorStyle::Steady
    };
    let (speed, quality, price): (i32, i32, i32) = match style {
        ContractorStyle::FastAndLoose => (70, 2, 85),
        ContractorStyle::SlowAndPerfect => (140, 5, 125),
        ContractorStyle::Chatty => (115, 3, 95),
        ContractorStyle::Grumpy => (100, 4, 115),
        ContractorStyle::Steady => (100, 3, 100),
    };
    // Apprentices are cheap and green; old hands are good.
    let (quality, price) = match character.role {
        Role::Apprentice => (quality - 1, price - 15),
        Role::RetiredMiner | Role::Smith => (quality + 1, price + 5),
        _ => (quality, price),
    };
    Some(ContractorProfile {
        character: character.id,
        style,
        speed_percent: u16::try_from((speed + rng.range(-8, 8)).clamp(55, 160)).unwrap_or(100),
        quality: u8::try_from(quality.clamp(1, 5)).unwrap_or(3),
        price_percent: u16::try_from((price + rng.range(-6, 6)).clamp(60, 150)).unwrap_or(100),
    })
}

/// The contractors among `characters`, in resident order.
#[must_use]
pub fn contractors_among(characters: &[Character]) -> Vec<ContractorProfile> {
    characters.iter().filter_map(contractor_profile).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{BuildJob, ContractorHire, contractor_profile, contractors_among};
    use crate::people::{
        CharacterId, VillageId, clock::DayTime, generate_population, names::Culture,
    };

    #[test]
    fn contractors_differ_and_quotes_follow_their_style() {
        let mut styles = BTreeSet::new();
        for village in 1..30 {
            let population = generate_population(4, VillageId(village), Culture::Highland, 10);
            for profile in contractors_among(&population.characters) {
                styles.insert(profile.style);
                let quote = profile.quote(BuildJob::HOUSE);
                assert!(quote.tenth_days > 0 && quote.price > 0);
                assert!((1..=5).contains(&quote.quality));
                let character = population.get(profile.character).expect("resident");
                assert!(!profile.pitch(character).contains('{'));
                assert_eq!(contractor_profile(character), Some(profile));
            }
        }
        assert!(styles.len() >= 4, "{styles:?}");
        let _ = CharacterId::default();
    }

    #[test]
    fn hired_work_progresses_in_world_time() {
        let population = generate_population(4, VillageId(3), Culture::Frontier, 12);
        let profile = contractors_among(&population.characters)[0];
        let hire = ContractorHire::new(profile, BuildJob::CABIN, DayTime::at(2, 9, 0));
        assert_eq!(hire.progress_per_mille(DayTime::at(2, 9, 0)), 0);
        assert_eq!(hire.progress_per_mille(DayTime::at(30, 9, 0)), 1_000);
        let midway = hire.progress_per_mille(DayTime::at(2, 15, 0));
        assert!(midway > 0 && midway < 1_000);
    }
}
