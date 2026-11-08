//! Menu auto-repeat: a held direction steps once immediately, then again after an initial delay
//! and at a steady interval while it stays held.

/// Seconds a direction must be held before it starts repeating.
pub const REPEAT_DELAY: f32 = 0.35;
/// Seconds between repeated steps once repeating.
pub const REPEAT_INTERVAL: f32 = 0.11;

/// Repeat timer for one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuRepeat {
    direction: i8,
    held_seconds: f32,
    next_step_seconds: f32,
}

impl MenuRepeat {
    /// Feeds this frame's direction (-1, 0 or 1) and returns the step to take (0 for none). A
    /// new direction steps on its first frame (edge detection); reversing restarts the delay.
    /// At most one step happens per frame, whatever the frame time.
    pub fn update(&mut self, direction: i8, delta_seconds: f32) -> i8 {
        if direction == 0 {
            *self = Self::default();
            return 0;
        }
        if direction != self.direction {
            *self = Self {
                direction,
                held_seconds: 0.0,
                next_step_seconds: REPEAT_DELAY,
            };
            return direction;
        }
        self.held_seconds += delta_seconds.max(0.0);
        if self.held_seconds >= self.next_step_seconds {
            self.next_step_seconds = (self.next_step_seconds + REPEAT_INTERVAL)
                .max(REPEAT_INTERVAL.mul_add(0.5, self.held_seconds));
            direction
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Times (at `dt` per frame) at which a direction held for `seconds` steps.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test frame counts are small"
    )]
    fn step_times(seconds: f32, dt: f32) -> Vec<f32> {
        let mut repeat = MenuRepeat::default();
        let frames = (seconds / dt).round() as usize;
        (0..=frames)
            .filter(|frame| repeat.update(1, if *frame == 0 { 0.0 } else { dt }) != 0)
            .map(|frame| frame as f32 * dt)
            .collect()
    }

    #[test]
    fn steps_immediately_then_after_the_delay_then_at_the_interval() {
        let dt = 1.0 / 120.0;
        let times = step_times(1.0, dt);
        assert!(
            times[0].abs() < f32::EPSILON,
            "first step on the press frame"
        );
        assert!((times[1] - REPEAT_DELAY).abs() <= dt * 1.5, "{times:?}");
        for pair in times[1..].windows(2) {
            assert!(
                ((pair[1] - pair[0]) - REPEAT_INTERVAL).abs() <= dt * 1.5,
                "{times:?}"
            );
        }
    }

    #[test]
    fn releasing_or_reversing_restarts_the_delay() {
        let mut repeat = MenuRepeat::default();
        assert_eq!(repeat.update(1, 0.016), 1);
        assert_eq!(repeat.update(1, 0.1), 0);
        assert_eq!(repeat.update(-1, 0.016), -1, "reversal steps at once");
        assert_eq!(repeat.update(-1, 0.1), 0, "and waits the full delay again");
        assert_eq!(repeat.update(0, 0.016), 0);
        assert_eq!(repeat.update(-1, 0.016), -1, "a fresh press steps at once");
    }

    #[test]
    fn a_long_frame_never_steps_more_than_once() {
        let mut repeat = MenuRepeat::default();
        assert_eq!(repeat.update(1, 0.0), 1);
        assert_eq!(repeat.update(1, 5.0), 1);
        assert_eq!(repeat.update(1, 0.0), 0, "no burst of queued repeats");
    }
}
