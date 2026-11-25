//! The ending sequence (Wave 10, Descent VIII): what each seat shows once the cable lights.
//!
//! The simulation records one fact, the world time the cable lit
//! ([`super::Heart::cable_lit_millis`], replicated and saved), so every peer starts the
//! sequence on the same tick. Each seat then plays it for itself from that moment: a pure
//! state machine of the time since the cable lit and the seat's own skips ([`EndingView`]), so
//! split-screen seats and online players can each skip their own view without touching anyone
//! else's. Afterwards play simply goes on (expansion plan §18 decision 5).
//!
//! | Phase | Time | What plays |
//! |---|---|---|
//! | [`EndingPhase::Awakening`] | 0-8 s | in the world: the Star Drive wakes, the camera pulls back, the beam leaves the Heart and climbs the cable |
//! | [`EndingPhase::Ascent`] | 8-22 s | the planet in cutaway: the light climbs through all eight Descents to the surface and up to the Moon, which lights up |
//! | [`EndingPhase::Credits`] | 22-84 s | the credits roll over the stars (skippable) |
//! | [`EndingPhase::Epilogue`] | then 16 s, or until dismissed | the epilogue card |
//! | [`EndingPhase::Done`] | after | back to play |

/// The awakening's length (in the world).
pub const AWAKENING_MILLIS: u64 = 8_000;
/// When the ascent ends (the planet cutaway).
pub const ASCENT_END_MILLIS: u64 = 22_000;
/// When the credits end.
pub const CREDITS_END_MILLIS: u64 = 84_000;
/// How long the epilogue card stays unless dismissed.
pub const EPILOGUE_MILLIS: u64 = 16_000;
/// How long rigs are shielded once the cable lights (the whole sequence, with a margin).
pub const ENDING_SHIELD_MILLIS: u64 = CREDITS_END_MILLIS + EPILOGUE_MILLIS + 6_000;
/// No input skips for this long after a phase starts (a held button does not skip by accident).
pub const SKIP_GUARD_MILLIS: u64 = 1_200;

/// Where a seat's ending is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EndingPhase {
    Awakening,
    Ascent,
    Credits,
    Epilogue,
    Done,
}

impl EndingPhase {
    /// Whether the seat shows a full-screen sequence (the cutaway, the credits, the card)
    /// instead of its world view.
    #[must_use]
    pub const fn covers_screen(self) -> bool {
        matches!(self, Self::Ascent | Self::Credits | Self::Epilogue)
    }

    /// Whether the sequence is still playing.
    #[must_use]
    pub const fn playing(self) -> bool {
        !matches!(self, Self::Done)
    }
}

/// One seat's view of the ending: its skips (presentation state, never simulated).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EndingView {
    /// When (ms since the cable lit) the seat skipped to the epilogue, if it did.
    pub epilogue_from: Option<u64>,
    /// Whether the seat dismissed the epilogue (or skipped past it).
    pub closed: bool,
}

impl EndingView {
    /// A seat that has not skipped anything.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epilogue_from: None,
            closed: false,
        }
    }

    /// The phase at `elapsed` ms since the cable lit.
    #[must_use]
    pub const fn phase(&self, elapsed: u64) -> EndingPhase {
        if self.closed {
            return EndingPhase::Done;
        }
        let epilogue_start = match self.epilogue_from {
            Some(from) if from < CREDITS_END_MILLIS => from,
            _ => CREDITS_END_MILLIS,
        };
        if elapsed >= epilogue_start {
            return if elapsed - epilogue_start < EPILOGUE_MILLIS {
                EndingPhase::Epilogue
            } else {
                EndingPhase::Done
            };
        }
        if elapsed < AWAKENING_MILLIS {
            EndingPhase::Awakening
        } else if elapsed < ASCENT_END_MILLIS {
            EndingPhase::Ascent
        } else {
            EndingPhase::Credits
        }
    }

    /// When (ms since the cable lit) the current phase began.
    #[must_use]
    pub const fn phase_start(&self, elapsed: u64) -> u64 {
        match self.phase(elapsed) {
            EndingPhase::Awakening => 0,
            EndingPhase::Ascent => AWAKENING_MILLIS,
            EndingPhase::Credits => ASCENT_END_MILLIS,
            EndingPhase::Epilogue | EndingPhase::Done => match self.epilogue_from {
                Some(from) if from < CREDITS_END_MILLIS => from,
                _ => CREDITS_END_MILLIS,
            },
        }
    }

    /// The seat's skip button at `elapsed`: before the epilogue it skips to it, on the
    /// epilogue it closes the sequence. Ignored just after a phase starts
    /// ([`SKIP_GUARD_MILLIS`]). Returns whether it did anything.
    pub const fn skip(&mut self, elapsed: u64) -> bool {
        let phase = self.phase(elapsed);
        if elapsed < self.phase_start(elapsed) + SKIP_GUARD_MILLIS {
            return false;
        }
        match phase {
            EndingPhase::Awakening | EndingPhase::Ascent | EndingPhase::Credits => {
                self.epilogue_from = Some(elapsed);
                true
            }
            EndingPhase::Epilogue => {
                self.closed = true;
                true
            }
            EndingPhase::Done => false,
        }
    }

    /// How far through the credits roll the seat is at `elapsed`, `0..=1`.
    #[must_use]
    pub fn credits_progress(&self, elapsed: u64) -> f32 {
        let span = CREDITS_END_MILLIS - ASCENT_END_MILLIS;
        #[allow(clippy::cast_precision_loss, reason = "a minute of milliseconds")]
        let share = elapsed.saturating_sub(ASCENT_END_MILLIS) as f32 / span as f32;
        share.clamp(0.0, 1.0)
    }

    /// How far through the ascent cutaway the seat is at `elapsed`, `0..=1`.
    #[must_use]
    pub fn ascent_progress(&self, elapsed: u64) -> f32 {
        let span = ASCENT_END_MILLIS - AWAKENING_MILLIS;
        #[allow(clippy::cast_precision_loss, reason = "seconds of milliseconds")]
        let share = elapsed.saturating_sub(AWAKENING_MILLIS) as f32 / span as f32;
        share.clamp(0.0, 1.0)
    }
}

/// How far up the cable the light has climbed at `elapsed` ms since it lit, `0..=1`.
///
/// From the Heart to the Moon; the world beam and the cutaway both read it.
#[must_use]
pub fn beam_climb(elapsed: u64) -> f32 {
    // Slow at first out of the Heart, then racing up the cable.
    #[allow(clippy::cast_precision_loss, reason = "seconds of milliseconds")]
    let t = (elapsed as f32 / ASCENT_END_MILLIS as f32).clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

/// One line of the credits: a heading or a name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditLine {
    Title(&'static str),
    Heading(&'static str),
    Name(&'static str),
    Gap,
}

/// The credits, top to bottom (original names only).
pub const CREDITS: &[CreditLine] = &[
    CreditLine::Title("FATHERLOAD"),
    CreditLine::Gap,
    CreditLine::Heading("A game by"),
    CreditLine::Name("Braden Steffaniak"),
    CreditLine::Gap,
    CreditLine::Heading("Design, code, pixel art and sound"),
    CreditLine::Name("Braden Steffaniak"),
    CreditLine::Name("with Claude"),
    CreditLine::Gap,
    CreditLine::Heading("The Fathers' expedition"),
    CreditLine::Name("Amos Varga, lead driller"),
    CreditLine::Name("Ines Calloway, surveyor"),
    CreditLine::Name("Dov Halloran, radio"),
    CreditLine::Name("Ruth \"Bran\" Brannigan, medic and cook"),
    CreditLine::Name("Teo Marsh, mechanic"),
    CreditLine::Name("Sefa Rhee, naturalist"),
    CreditLine::Gap,
    CreditLine::Heading("Who remembered them"),
    CreditLine::Name("Gravel Hollow"),
    CreditLine::Name("Cathedral Station"),
    CreditLine::Name("The Hive"),
    CreditLine::Name("Merrow City"),
    CreditLine::Name("The Sunfolk Kraal"),
    CreditLine::Name("Parallax Station"),
    CreditLine::Name("The Megacity's archivist"),
    CreditLine::Name("Heart's Edge"),
    CreditLine::Gap,
    CreditLine::Heading("The builders"),
    CreditLine::Name("who laid the clay in courses"),
    CreditLine::Name("hung a lamp in the deep"),
    CreditLine::Name("lit a sun inside the world"),
    CreditLine::Name("and left the cable for whoever came next"),
    CreditLine::Gap,
    CreditLine::Heading("Built with"),
    CreditLine::Name("Rust and raylib"),
    CreditLine::Gap,
    CreditLine::Heading("And you"),
    CreditLine::Name("for digging all the way down"),
    CreditLine::Gap,
    CreditLine::Title("The stars are open."),
];

/// The epilogue card's title.
pub const EPILOGUE_TITLE: &str = "Epilogue: Up";

/// The epilogue card's text.
pub const EPILOGUE: &str = "The cable hums all the way up now. In town people stop in the \
street to watch the light climb into the sky, and the hanging stone shines as if it had been \
waiting. The Star Drive turns over in your rig like a second heart. Somewhere past the Moon \
the builders' road begins, and their star charts point the way. Nothing is over: dig, trade, \
build, and when you are ready, fly higher than the Moon.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sequence_plays_through_its_phases_in_order() {
        let view = EndingView::default();
        assert_eq!(view.phase(0), EndingPhase::Awakening);
        assert_eq!(view.phase(AWAKENING_MILLIS), EndingPhase::Ascent);
        assert_eq!(view.phase(ASCENT_END_MILLIS), EndingPhase::Credits);
        assert_eq!(view.phase(CREDITS_END_MILLIS), EndingPhase::Epilogue);
        assert_eq!(
            view.phase(CREDITS_END_MILLIS + EPILOGUE_MILLIS),
            EndingPhase::Done
        );
        let mut last = EndingPhase::Awakening;
        for elapsed in (0..CREDITS_END_MILLIS + EPILOGUE_MILLIS + 1_000).step_by(250) {
            let phase = view.phase(elapsed);
            assert!(phase >= last, "phases only move forward");
            last = phase;
        }
    }

    #[test]
    fn a_skip_jumps_to_the_epilogue_and_a_second_closes_it() {
        let mut view = EndingView::default();
        // Not straight away (a held button).
        assert!(!view.skip(200));
        assert_eq!(view.phase(200), EndingPhase::Awakening);
        assert!(view.skip(30_000));
        assert_eq!(view.phase(30_000), EndingPhase::Epilogue);
        assert_eq!(view.phase(30_000 + EPILOGUE_MILLIS), EndingPhase::Done);
        // The card's own guard, then a close.
        assert!(!view.skip(30_100));
        assert!(view.skip(32_000));
        assert_eq!(view.phase(32_000), EndingPhase::Done);
        assert!(!view.skip(40_000));
    }

    #[test]
    fn each_seat_skips_only_its_own_view() {
        let mut first = EndingView::default();
        let second = EndingView::default();
        assert!(first.skip(10_000));
        assert_eq!(first.phase(10_000), EndingPhase::Epilogue);
        assert_eq!(second.phase(10_000), EndingPhase::Ascent);
    }

    #[test]
    fn the_light_climbs_from_the_heart_to_the_moon() {
        assert!(beam_climb(0).abs() < 1e-6);
        assert!((beam_climb(ASCENT_END_MILLIS) - 1.0).abs() < 1e-6);
        let mut last = 0.0;
        for elapsed in (0..=ASCENT_END_MILLIS).step_by(500) {
            let climb = beam_climb(elapsed);
            assert!(climb >= last);
            last = climb;
        }
        assert!(matches!(CREDITS.first(), Some(CreditLine::Title(_))));
    }
}
