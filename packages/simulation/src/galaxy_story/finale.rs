//! The galaxy story's finale (Wave 13 story): the Harbour, its Warden, the choice and the
//! ending sequence.
//!
//! * **The Harbour** is a ring of ten builder world-ships moored round a dim star far down the
//!   road ([`super::plan::Harbour`]). The ring is presentation; its gate is where things
//!   happen.
//! * **The Warden** is the Harbour's door: a builder guardian machine, a boss of the space
//!   combat rules (`hostiles::ShipClass::Warden`: big, readable, every attack telegraphed,
//!   hit areas fitted to its drawn body, no countdown, the escape jump always works). It meets
//!   a rig that comes within [`WARDEN_REACH_TILES`] of the gate while its chapter is open and
//!   it is not calmed; it comes back if the rig flees and returns.
//! * **The choice** ([`FinaleChoice`]): ring the Homecoming Bell (wake the sleepers: the
//!   builders come home) or keep the lamp (let them sleep on; the crew keeps the road lit).
//!   Anyone in the crew makes it, for the crew, at the gate once the Warden is calm.
//! * **The ending** plays from the world time the choice was made
//!   (`super::GalaxyStory::finale_millis`, replicated and saved), so every peer starts it on
//!   the same tick, and each seat plays it for itself ([`FinaleView`]: its own skips), like
//!   the Descent's ending. Then play goes on: the stars are still out there.

use serde::{Deserialize, Serialize};

/// A rig within this of the gate meets the Warden (tiles).
pub const WARDEN_REACH_TILES: i64 = 40_000;
/// A rig must be within this of the gate to make the choice (tiles).
pub const CHOICE_REACH_TILES: i64 = 45_000;

/// The finale's choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FinaleChoice {
    /// Wake the sleepers: the builders come home.
    RingTheBell,
    /// Let them sleep on, and keep the road's lamp lit.
    KeepTheLamp,
}

impl FinaleChoice {
    pub const ALL: [Self; 2] = [Self::RingTheBell, Self::KeepTheLamp];

    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::RingTheBell => 0,
            Self::KeepTheLamp => 1,
        }
    }

    /// The choice of wire code `code`.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::RingTheBell),
            1 => Some(Self::KeepTheLamp),
            _ => None,
        }
    }

    /// What the choice is called on its button.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RingTheBell => "Ring the Homecoming Bell",
            Self::KeepTheLamp => "Keep the lamp",
        }
    }

    /// What the choice means, said before it is made.
    #[must_use]
    pub const fn promise(self) -> &'static str {
        match self {
            Self::RingTheBell => {
                "Wake the sleepers. They have waited an age for the key, and the key is you. \
                 They will be old, and astonished, and they will want to go home."
            }
            Self::KeepTheLamp => {
                "Let them sleep on, safe, as the fathers kept the children. Take up the \
                 Harbour's lamp and keep the road lit, for whoever wakes next."
            }
        }
    }
}

/// What the Harbour's heart looks like before the choice (the card's lead).
pub const HEART_TEXT: &str = "Behind the Warden the gate stands open, and in it hang two \
    things: a bell the size of a town, cast in the cable's stuff, and a lamp, unlit, the twin of \
    the one at the Heart. The archivist says nothing at all. This one is yours.";

/// One beat of the epilogue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Beat {
    pub place: &'static str,
    pub line: &'static str,
}

/// The ending's title for `choice`.
#[must_use]
pub const fn title(choice: FinaleChoice) -> &'static str {
    match choice {
        FinaleChoice::RingTheBell => "Homecoming",
        FinaleChoice::KeepTheLamp => "The Lamp Keepers",
    }
}

/// What the ending card says first.
#[must_use]
pub const fn lead(choice: FinaleChoice) -> &'static str {
    match choice {
        FinaleChoice::RingTheBell => {
            "The bell rings once, and the sound goes round the ring of ships like light. One by \
             one, for the first time in an age, their windows wake."
        }
        FinaleChoice::KeepTheLamp => {
            "You lift the lamp and light it from your drive. The ring of ships sleeps on, warm \
             now, and the road behind you lights waymark by waymark all the way home."
        }
    }
}

/// The epilogue of `choice`.
#[must_use]
pub const fn epilogue(choice: FinaleChoice) -> &'static [Beat; 6] {
    match choice {
        FinaleChoice::RingTheBell => &BELL,
        FinaleChoice::KeepTheLamp => &LAMP,
    }
}

static BELL: [Beat; 6] = [
    Beat {
        place: "The Harbour",
        line: "The sleepers wake slowly, as old people do. The first one out stands at a \
               window and looks at your little rig for a long time, and then puts a hand flat \
               on the glass. The Warden, very quietly, opens every door it has.",
    },
    Beat {
        place: "The Concord",
        line: "Across the sky the Concord's ships stop mid-patrol. New orders: come home. \
               Some of them, the archivist swears, do a little turn of joy first.",
    },
    Beat {
        place: "The near stars",
        line: "The Vessi make a new verse about the tall ships, and for the first time it is \
               a warm one. The Tokkari untie the oldest knot in the cord. The Siphra remember.",
    },
    Beat {
        place: "The Archive of Stars",
        line: "The archivist reads the Harbour's books aloud down the cable, all of them, in \
               Ines Calloway's voice. It takes a week. Nobody asks it to stop.",
    },
    Beat {
        place: "The Town",
        line: "On the porch, Wren Varga hears the news and goes inside and puts the kettle \
               on, because if the builders are coming home somebody ought to be ready for \
               company.",
    },
    Beat {
        place: "The Moon",
        line: "Ten world-ships turn toward the dock. They are slow; it will take years. \
               The road is open, and the sky is still out there. Go on.",
    },
];

static LAMP: [Beat; 6] = [
    Beat {
        place: "The Harbour",
        line: "The sleepers turn over in their long cold, warm for the first time in an age, \
               and dream of a lamp. The Warden settles in front of the gate again, and this \
               time it leaves the door ajar.",
    },
    Beat {
        place: "The Concord",
        line: "The Concord's ships see the road light up behind you and fall in along it, \
               waymark by waymark: pilots with an order again. Hold the road. It is lit.",
    },
    Beat {
        place: "The near stars",
        line: "Every people of the near stars sees the waymarks light. The Vessi sing a verse \
               for the lamp keepers. The Tokkari tie a new knot. The Siphra remember you.",
    },
    Beat {
        place: "The Archive of Stars",
        line: "The archivist writes the last page of the road, and then a new heading under \
               it: Keepers. It underlines it twice, which it has never done before.",
    },
    Beat {
        place: "The Town",
        line: "On the porch, Wren Varga sees a new light at the top of the cable, steady, \
               like a lamp in a window. That is what fathers do, she says, to nobody, and \
               smiles.",
    },
    Beat {
        place: "The Moon",
        line: "The dock keeps a lamp lit now. Whoever wakes, whenever, the road is there. And \
               so are you: the sky is still out there. Go on.",
    },
];

/// One line of the credits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditLine {
    Heading(&'static str),
    Name(&'static str),
    Gap,
}

/// The galaxy story's credits.
pub static CREDITS: [CreditLine; 28] = [
    CreditLine::Heading("FATHERLOAD"),
    CreditLine::Name("The Builders' Road"),
    CreditLine::Gap,
    CreditLine::Heading("The crew of the road"),
    CreditLine::Name("You, the Load"),
    CreditLine::Name("The archivist of the Buried Megacity"),
    CreditLine::Gap,
    CreditLine::Heading("Who went before"),
    CreditLine::Name("Amos Varga, who kept the drive company"),
    CreditLine::Name("Ines Calloway, who copied the roads"),
    CreditLine::Name("Dov Halloran, who sang badly"),
    CreditLine::Name("Ruth Brannigan, who made people eat"),
    CreditLine::Name("Teo Marsh, who listened to pumps"),
    CreditLine::Name("Sefa Rhee, who wrote down the rain verse"),
    CreditLine::Gap,
    CreditLine::Heading("Met on the road"),
    CreditLine::Name("The Vessi, the Tokkari and the Siphra"),
    CreditLine::Name("The Concord, who held the road"),
    CreditLine::Name("The Warden, a very good door"),
    CreditLine::Gap,
    CreditLine::Heading("Waiting at home"),
    CreditLine::Name("Wren Varga, on her porch"),
    CreditLine::Gap,
    CreditLine::Heading("And the builders"),
    CreditLine::Name("Who promised to come back with the key"),
    CreditLine::Gap,
    CreditLine::Gap,
    CreditLine::Name("Look up."),
];

/// The ending's text (the journal keeps it once played).
#[must_use]
pub fn ending_text(choice: FinaleChoice) -> String {
    let mut text = lead(choice).to_owned();
    for beat in epilogue(choice) {
        text.push_str("\n\n");
        text.push_str(beat.place);
        text.push_str(": ");
        text.push_str(beat.line);
    }
    text
}

/// The ending's lead card's length.
pub const LEAD_MILLIS: u64 = 10_000;
/// When the credits end.
pub const CREDITS_END_MILLIS: u64 = 64_000;
/// How long the epilogue stays unless dismissed.
pub const EPILOGUE_MILLIS: u64 = 30_000;
/// No input skips for this long after a phase starts.
pub const SKIP_GUARD_MILLIS: u64 = 1_200;

/// Where a seat's ending is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FinalePhase {
    /// The choice's moment over the Harbour (in the world).
    Lead,
    /// The credits over the road's lit waymarks.
    Credits,
    /// The epilogue card.
    Epilogue,
    Done,
}

impl FinalePhase {
    /// Whether the seat shows a full-screen sequence instead of its world view.
    #[must_use]
    pub const fn covers_screen(self) -> bool {
        matches!(self, Self::Credits | Self::Epilogue)
    }

    /// Whether the sequence is still playing.
    #[must_use]
    pub const fn playing(self) -> bool {
        !matches!(self, Self::Done)
    }
}

/// One seat's view of the ending (presentation state: its skips, never simulated).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FinaleView {
    /// When (ms since the choice) the seat skipped to the epilogue, if it did.
    pub epilogue_from: Option<u64>,
    /// Whether the seat dismissed the epilogue.
    pub closed: bool,
}

impl FinaleView {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            epilogue_from: None,
            closed: false,
        }
    }

    /// The phase at `elapsed` ms since the choice.
    #[must_use]
    pub const fn phase(&self, elapsed: u64) -> FinalePhase {
        if self.closed {
            return FinalePhase::Done;
        }
        if let Some(from) = self.epilogue_from {
            return if elapsed >= from + EPILOGUE_MILLIS {
                FinalePhase::Done
            } else {
                FinalePhase::Epilogue
            };
        }
        if elapsed < LEAD_MILLIS {
            FinalePhase::Lead
        } else if elapsed < CREDITS_END_MILLIS {
            FinalePhase::Credits
        } else if elapsed < CREDITS_END_MILLIS + EPILOGUE_MILLIS {
            FinalePhase::Epilogue
        } else {
            FinalePhase::Done
        }
    }

    /// When the current phase started (ms since the choice).
    #[must_use]
    pub const fn phase_start(&self, elapsed: u64) -> u64 {
        match self.phase(elapsed) {
            FinalePhase::Lead => 0,
            FinalePhase::Credits => LEAD_MILLIS,
            FinalePhase::Epilogue => match self.epilogue_from {
                Some(from) => from,
                None => CREDITS_END_MILLIS,
            },
            FinalePhase::Done => elapsed,
        }
    }

    /// Skips the current phase (the credits to the epilogue, the epilogue away). Returns
    /// whether anything changed (a press right after a phase starts does nothing).
    pub const fn skip(&mut self, elapsed: u64) -> bool {
        if elapsed < self.phase_start(elapsed) + SKIP_GUARD_MILLIS {
            return false;
        }
        match self.phase(elapsed) {
            FinalePhase::Lead | FinalePhase::Credits => {
                self.epilogue_from = Some(elapsed);
                true
            }
            FinalePhase::Epilogue => {
                self.closed = true;
                true
            }
            FinalePhase::Done => false,
        }
    }

    /// How far the credits have rolled (0..=1).
    #[must_use]
    pub fn credits_progress(&self, elapsed: u64) -> f32 {
        if elapsed <= LEAD_MILLIS {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss, reason = "a share of a minute")]
        let share = (elapsed - LEAD_MILLIS) as f32 / (CREDITS_END_MILLIS - LEAD_MILLIS) as f32;
        share.clamp(0.0, 1.0)
    }
}
