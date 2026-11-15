//! Upgrade reward moments in the shell (WP-FEEL part 2), per seat and presentation only.
//!
//! - **Purchase moment** ([`InstallMoment`]): set when the world reports an
//!   `UpgradeInstalled` for the seat's rig. The shop room plays the rig's transform (old part,
//!   flash, new part, sparks, camera punch) and the showcase card stamps "INSTALLED" with its bars
//!   growing from the old tier to the new one, for [`INSTALL_TICKS`].
//! - **Milestone banners** ([`RewardBanner`]): one per `MilestoneEarned`, queued and shown one
//!   at a time across the top of the seat's view for [`BANNER_TICKS`] (the Act I -> II unlock
//!   gets a bigger, longer one).
//! - **Fresh unlocks**: cosmetics a milestone just unlocked, tagged NEW in the paint bay until
//!   the seat closes it.
//! - The progression screen's selected row ([`SeatUi::selected_menu_item`]) needs nothing
//!   here: it is an ordinary seat dialog ([`super::ModalScreen::Achievements`]).
//!
//! Nothing here is saved or replicated: the simulation owns milestones and unlocks
//! ([`fatherload_simulation::milestone`]); this only decides what each local view shows.

use std::collections::{BTreeSet, VecDeque};

use fatherload_simulation::{
    CosmeticItem, Player, economy::UpgradeKind, milestone::Milestone,
    upgrade_preview::preview_between,
};

use super::{SeatUi, ShellState};

/// How long the purchase moment plays, in shell ticks (60 per second).
pub const INSTALL_TICKS: u64 = 150;
/// How long a milestone banner stays up, in shell ticks.
pub const BANNER_TICKS: u64 = 300;
/// How long the Act I -> II banner stays up, in shell ticks.
pub const ACT_TWO_BANNER_TICKS: u64 = 480;
/// Most banners waiting their turn (a burst of milestones keeps the newest).
const MAX_QUEUED_BANNERS: usize = 6;

/// The purchase moment of one upgrade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallMoment {
    pub upgrade: UpgradeKind,
    /// The tier before the purchase.
    pub from_level: u8,
    /// The tier now fitted.
    pub to_level: u8,
    /// `ShellState::update_ticks` when it started.
    pub tick: u64,
}

impl InstallMoment {
    /// Shell ticks since it started.
    #[must_use]
    pub const fn age(&self, now: u64) -> u64 {
        now.saturating_sub(self.tick)
    }

    /// Whether it is still playing at `now`.
    #[must_use]
    pub const fn playing(&self, now: u64) -> bool {
        self.age(now) < INSTALL_TICKS
    }

    /// How far through the moment `now` is (0 to 1).
    #[must_use]
    pub fn progress(&self, now: u64) -> f32 {
        #[allow(clippy::cast_precision_loss, reason = "a few hundred ticks")]
        let progress = self.age(now) as f32 / INSTALL_TICKS as f32;
        progress.clamp(0.0, 1.0)
    }
}

/// A milestone banner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RewardBanner {
    pub milestone: Milestone,
    /// When it went up (`None` while it waits behind another banner).
    pub shown_at: Option<u64>,
}

impl RewardBanner {
    /// How long this banner stays up.
    #[must_use]
    pub fn duration(&self) -> u64 {
        if self.milestone == Milestone::ActTwoUnlocked {
            ACT_TWO_BANNER_TICKS
        } else {
            BANNER_TICKS
        }
    }
}

/// One seat's reward moments.
#[derive(Clone, Debug, Default)]
pub struct SeatRewards {
    pub install: Option<InstallMoment>,
    /// The banner showing (front) and those waiting their turn.
    pub banners: VecDeque<RewardBanner>,
    /// Cosmetics unlocked since the paint bay was last closed (tagged NEW there).
    pub fresh_unlocks: BTreeSet<CosmeticItem>,
    /// `ShellState::update_ticks` at the last [`Self::advance`] (the clock the room's purchase
    /// moment plays on).
    pub clock: u64,
}

impl SeatRewards {
    /// An empty set of moments.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            install: None,
            banners: VecDeque::new(),
            fresh_unlocks: BTreeSet::new(),
            clock: 0,
        }
    }

    /// Starts the purchase moment for `upgrade`, now fitted at `level`.
    pub fn installed(&mut self, upgrade: UpgradeKind, level: u8, now: u64) {
        self.install = Some(InstallMoment {
            upgrade,
            from_level: level
                .saturating_sub(1)
                .max(upgrade.starting_level().min(level)),
            to_level: level,
            tick: now,
        });
    }

    /// Queues `milestone`'s banner and tags its rewards NEW.
    pub fn earned(&mut self, milestone: Milestone) {
        self.fresh_unlocks
            .extend(milestone.rewards().iter().copied());
        if self.banners.len() >= MAX_QUEUED_BANNERS {
            // Keep the one showing; drop the oldest waiting.
            if self.banners.len() > 1 {
                self.banners.remove(1);
            } else {
                self.banners.clear();
            }
        }
        self.banners.push_back(RewardBanner {
            milestone,
            shown_at: None,
        });
    }

    /// Retires finished moments and raises the next banner (once a frame).
    pub fn advance(&mut self, now: u64) {
        self.clock = now;
        if self
            .install
            .as_ref()
            .is_some_and(|moment| !moment.playing(now))
        {
            self.install = None;
        }
        if let Some(front) = self.banners.front()
            && let Some(shown_at) = front.shown_at
            && now.saturating_sub(shown_at) >= front.duration()
        {
            self.banners.pop_front();
        }
        if let Some(front) = self.banners.front_mut()
            && front.shown_at.is_none()
        {
            front.shown_at = Some(now);
        }
    }

    /// The banner showing at `now` and its age in shell ticks.
    #[must_use]
    pub fn banner(&self, now: u64) -> Option<(RewardBanner, u64)> {
        let front = *self.banners.front()?;
        let age = now.saturating_sub(front.shown_at?);
        (age < front.duration()).then_some((front, age))
    }

    /// The purchase moment playing at `now`.
    #[must_use]
    pub fn install_at(&self, now: u64) -> Option<&InstallMoment> {
        self.install.as_ref().filter(|moment| moment.playing(now))
    }
}

/// The HUD toast for a fitted upgrade: its name and the headline of what it changes.
#[must_use]
pub fn install_message(player: &Player, upgrade: UpgradeKind, level: u8) -> String {
    let name = fatherload_simulation::economy::upgrade_tier_name(upgrade, level.saturating_sub(1));
    if upgrade == UpgradeKind::BombPack {
        return format!("Bought a {name}. You now carry {} bombs.", player.bombs);
    }
    let preview = preview_between(player, upgrade, level.saturating_sub(1), level);
    preview.headline().map_or_else(
        || format!("Bought the {name}!"),
        |headline| {
            let stop = if headline.ends_with(['!', '.', '?']) {
                ""
            } else {
                "."
            };
            format!("Bought the {name}! {headline}{stop}")
        },
    )
}

impl ShellState {
    /// Plays the purchase moment of `upgrade` (now at `level`) in seat `seat`'s view, with a
    /// HUD toast saying what it changes.
    pub fn upgrade_installed(
        &mut self,
        seat: usize,
        player: &Player,
        upgrade: UpgradeKind,
        level: u8,
    ) {
        let now = self.update_ticks;
        if let Some(state) = self.seats.get_mut(seat) {
            state.rewards.installed(upgrade, level, now);
            state.message = install_message(player, upgrade, level);
        }
    }

    /// Raises `milestone`'s banner in seat `seat`'s view and tags its unlocks NEW.
    pub fn milestone_earned(&mut self, seat: usize, milestone: Milestone) {
        if let Some(state) = self.seats.get_mut(seat) {
            state.rewards.earned(milestone);
        }
    }

    /// Retires finished reward moments and raises queued banners in every seat (once a frame).
    pub fn advance_rewards(&mut self) {
        let now = self.update_ticks;
        for seat in &mut self.seats {
            seat.rewards.advance(now);
        }
    }
}

impl SeatUi {
    /// Forgets the NEW tags (the paint bay closed).
    pub fn clear_fresh_unlocks(&mut self) {
        self.rewards.fresh_unlocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banners_show_one_at_a_time_in_order() {
        let mut rewards = SeatRewards::new();
        rewards.earned(Milestone::FirstUpgrade);
        rewards.earned(Milestone::DrillTier5);
        rewards.advance(10);
        assert_eq!(
            rewards.banner(10).map(|(b, _)| b.milestone),
            Some(Milestone::FirstUpgrade)
        );
        rewards.advance(10 + BANNER_TICKS);
        rewards.advance(10 + BANNER_TICKS);
        assert_eq!(
            rewards.banner(11 + BANNER_TICKS).map(|(b, _)| b.milestone),
            Some(Milestone::DrillTier5)
        );
        assert!(
            rewards
                .fresh_unlocks
                .contains(&Milestone::DrillTier5.rewards()[0])
        );
        rewards.advance(20 + 2 * BANNER_TICKS);
        assert!(rewards.banner(20 + 2 * BANNER_TICKS).is_none());
    }

    #[test]
    fn the_act_two_banner_stays_up_longer() {
        let mut rewards = SeatRewards::new();
        rewards.earned(Milestone::ActTwoUnlocked);
        rewards.advance(0);
        assert!(rewards.banner(BANNER_TICKS + 10).is_some());
        assert!(rewards.banner(ACT_TWO_BANNER_TICKS).is_none());
    }

    #[test]
    fn the_install_moment_plays_then_retires() {
        let mut rewards = SeatRewards::new();
        rewards.installed(UpgradeKind::Drill, 4, 100);
        let moment = rewards.install_at(100).expect("playing");
        assert_eq!((moment.from_level, moment.to_level), (3, 4));
        rewards.advance(100 + INSTALL_TICKS);
        assert!(rewards.install.is_none());
        // Lines that start uninstalled go from 0 to 1.
        rewards.installed(UpgradeKind::Scanner, 1, 0);
        assert_eq!(rewards.install.as_ref().map(|m| m.from_level), Some(0));
    }

    #[test]
    fn install_toasts_name_the_part_and_what_it_changes() {
        let mut player = Player::new(0.0, 0.0);
        player.fuel_tank_level = 2;
        let message = install_message(&player, UpgradeKind::FuelTank, 2);
        assert!(
            message.starts_with("Bought the Twin Fuel Tank! Climbs "),
            "{message}"
        );
        assert!(message.contains("% higher on a full tank"), "{message}");
    }
}
