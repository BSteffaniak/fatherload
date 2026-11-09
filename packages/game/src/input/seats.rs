//! Which devices drive which local seat.
//!
//! Policy (kept deliberately simple):
//!
//! - **Playing alone** (one seat): the keyboard (WASD *and* arrow keys) and *every* connected
//!   controller drive the rig; prompts follow whichever was used last.
//! - **Split-screen**: seat 1 is the WASD keyboard, seat 2 the arrow keys, seats 3+ have no
//!   keyboard. Each seat can also have one controller; while that controller is plugged in it
//!   drives the seat alone (the seat's keyboard keys are ignored until it is unplugged). Whenever the seat count changes, the
//!   connected controllers nobody owns fill the seats that have no controller, in this order:
//!   seats without a keyboard, then the arrow-keys seat, then the WASD seat. A controller that
//!   is still unassigned (connected later, or more controllers than seats) claims a seat by
//!   pressing **A** or **Start**: the first seat without a controller in the same order, or a new
//!   seat while fewer than the seat cap exist.
//! - A controller keeps its seat while unplugged, so reconnecting resumes control. Seats beyond
//!   the current count drop their controllers, and going back to one seat clears every claim.

use std::collections::{BTreeMap, BTreeSet};

use super::devices::KeyboardLayout;

/// Where a claiming controller goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Claim {
    /// Joined existing seat `0`-based index.
    Seat(usize),
    /// Every seat has a controller; a new seat would be needed.
    NewSeat,
    /// Every seat has a controller and the seat cap is reached.
    Full,
}

/// Seat <-> device assignment.
#[derive(Clone, Debug, Default)]
pub struct SeatRoster {
    seat_count: usize,
    /// Controller index -> seat index (split-screen only).
    pads: BTreeMap<u8, usize>,
}

impl SeatRoster {
    /// Current number of seats.
    #[cfg(test)]
    #[must_use]
    pub const fn seat_count(&self) -> usize {
        self.seat_count
    }

    /// Whether every controller drives seat 0 (playing alone).
    #[must_use]
    pub const fn solo(&self) -> bool {
        self.seat_count <= 1
    }

    /// The keyboard layout driving `seat`, if any.
    #[must_use]
    pub const fn keyboard(&self, seat: usize) -> Option<KeyboardLayout> {
        match (self.seat_count <= 1, seat) {
            (true, 0) => Some(KeyboardLayout::Full),
            (false, 0) => Some(KeyboardLayout::Wasd),
            (false, 1) => Some(KeyboardLayout::Arrows),
            _ => None,
        }
    }

    /// The controller claimed by `seat` (split-screen).
    #[must_use]
    pub fn pad(&self, seat: usize) -> Option<u8> {
        self.pads
            .iter()
            .find(|(_, owner)| **owner == seat)
            .map(|(pad, _)| *pad)
    }

    /// The seat `pad` drives: its claimed seat in split-screen, seat 0 when playing alone.
    #[must_use]
    pub fn seat_of(&self, pad: u8) -> Option<usize> {
        if self.solo() {
            Some(0)
        } else {
            self.pads.get(&pad).copied()
        }
    }

    /// Adapts to the game's seat count, then lets unassigned `connected` controllers fill
    /// seats that have none (see the module policy). Returns the new `(pad, seat)` claims.
    pub fn set_seat_count(&mut self, count: usize, connected: &BTreeSet<u8>) -> Vec<(u8, usize)> {
        if count == self.seat_count {
            return Vec::new();
        }
        self.seat_count = count;
        if count <= 1 {
            self.pads.clear();
            return Vec::new();
        }
        self.pads.retain(|_, seat| *seat < count);
        let mut claims = Vec::new();
        for pad in connected {
            if self.pads.contains_key(pad) {
                continue;
            }
            if let Some(seat) = self.free_seat() {
                self.pads.insert(*pad, seat);
                claims.push((*pad, seat));
            }
        }
        claims
    }

    /// An unassigned controller pressed A/Start: joins the first seat without a controller,
    /// or asks for a new seat (`max_seats` caps it). A controller that already has a seat (or
    /// playing alone) is not a claim.
    pub fn claim(&mut self, pad: u8, max_seats: usize) -> Option<Claim> {
        if self.solo() || self.pads.contains_key(&pad) {
            return None;
        }
        Some(match self.free_seat() {
            Some(seat) => {
                self.pads.insert(pad, seat);
                Claim::Seat(seat)
            }
            None if self.seat_count < max_seats => Claim::NewSeat,
            None => Claim::Full,
        })
    }

    /// Records that the game added `seat` for `pad` after a [`Claim::NewSeat`].
    pub fn seat_added(&mut self, pad: u8, seat: usize) {
        self.seat_count = self.seat_count.max(seat + 1);
        self.pads.insert(pad, seat);
    }

    /// The first seat without a controller: keyboardless seats, then arrows, then WASD.
    fn free_seat(&self) -> Option<usize> {
        let free = |seat: &usize| self.pad(*seat).is_none();
        (2..self.seat_count)
            .chain([1, 0])
            .filter(|seat| *seat < self.seat_count)
            .find(free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pads(indices: &[u8]) -> BTreeSet<u8> {
        indices.iter().copied().collect()
    }

    #[test]
    fn playing_alone_every_device_drives_seat_zero() {
        let mut roster = SeatRoster::default();
        assert!(roster.set_seat_count(1, &pads(&[0, 1])).is_empty());
        assert_eq!(roster.keyboard(0), Some(KeyboardLayout::Full));
        assert_eq!(roster.seat_of(0), Some(0));
        assert_eq!(roster.seat_of(3), Some(0));
        assert_eq!(roster.claim(1, 4), None, "no claims when playing alone");
    }

    #[test]
    fn split_screen_gives_the_first_controller_to_the_arrow_keys_seat() {
        let mut roster = SeatRoster::default();
        roster.set_seat_count(1, &pads(&[0]));
        assert_eq!(roster.set_seat_count(2, &pads(&[0])), vec![(0, 1)]);
        assert_eq!(roster.keyboard(0), Some(KeyboardLayout::Wasd));
        assert_eq!(roster.keyboard(1), Some(KeyboardLayout::Arrows));
        assert_eq!(roster.pad(1), Some(0));
        assert_eq!(roster.seat_of(0), Some(1));
    }

    #[test]
    fn controllers_fill_keyboardless_seats_first_then_arrows_then_wasd() {
        let mut roster = SeatRoster::default();
        assert_eq!(
            roster.set_seat_count(4, &pads(&[0, 1, 2, 3, 4])),
            vec![(0, 2), (1, 3), (2, 1), (3, 0)]
        );
        assert_eq!(roster.keyboard(2), None);
        assert_eq!(roster.seat_of(4), None, "more controllers than seats");
    }

    #[test]
    fn an_unassigned_controller_claims_a_free_seat_or_a_new_one() {
        let mut roster = SeatRoster::default();
        roster.set_seat_count(2, &pads(&[]));
        assert_eq!(roster.claim(5, 4), Some(Claim::Seat(1)));
        assert_eq!(roster.claim(5, 4), None, "already seated");
        assert_eq!(roster.claim(6, 4), Some(Claim::Seat(0)));
        assert_eq!(roster.claim(7, 4), Some(Claim::NewSeat));
        roster.seat_added(7, 2);
        assert_eq!(roster.seat_count(), 2 + 1);
        assert_eq!(roster.seat_of(7), Some(2));
        roster.set_seat_count(4, &pads(&[5, 6, 7]));
        assert_eq!(roster.claim(8, 4), Some(Claim::Seat(3)));
        assert_eq!(roster.claim(9, 4), Some(Claim::Full));
    }

    #[test]
    fn shrinking_drops_claims_beyond_the_seats_and_solo_clears_them() {
        let mut roster = SeatRoster::default();
        roster.set_seat_count(3, &pads(&[0, 1]));
        assert_eq!(roster.seat_of(0), Some(2));
        roster.set_seat_count(2, &pads(&[0, 1]));
        assert_eq!(
            roster.seat_of(0),
            Some(0),
            "re-seated in the remaining seats"
        );
        assert_eq!(roster.seat_of(1), Some(1));
        roster.set_seat_count(1, &pads(&[0, 1]));
        roster.set_seat_count(2, &pads(&[]));
        assert_eq!(roster.pad(0), None);
        assert_eq!(roster.pad(1), None, "an unplugged pad is not auto-seated");
    }
}
