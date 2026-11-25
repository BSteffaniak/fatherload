//! The end of the Descent (Wave 10, expansion plan §13 and §18 decision 5).
//!
//! The Heart wakes, light runs up the builders' cable from the Star Drive to the Moon, and every
//! place where one of the Fathers stayed sees it pass.
//!
//! The ending is a story moment, not a game over: the Descent is the first big story and the
//! sky past the Moon is the second, so the world keeps going after the last card. The crew's
//! [`super::ExpeditionState::cable_lit`] records that it played (saved and shared, so it plays
//! once per world, for every seat and partner). What wakes the Heart is the Heart track's
//! fight; this module only says what everyone sees when it does.

/// One beat of the epilogue: a place on the cable and what happens there as the light passes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EpilogueBeat {
    pub place: &'static str,
    pub line: &'static str,
}

/// The ending card's title.
pub const ENDING_TITLE: &str = "The Cable Lights";

/// What the ending card says first, before the beats.
pub const ENDING_LEAD: &str = "The Star Drive opens its eyes. Light runs out of the Heart and up \
                               the builders' cable, through every Descent you dug, toward the \
                               Moon.";

/// The epilogue, bottom to top: the light passes each of the Fathers on its way up.
pub const EPILOGUE: [EpilogueBeat; 8] = [
    EpilogueBeat {
        place: "The Heart",
        line: "The light runs over a chair of a sled board and a crutch by the cable root, where \
               Amos Varga kept the drive company. The board's old paint, THE FATHERS' LOAD, \
               shines like the day Bran painted it.",
    },
    EpilogueBeat {
        place: "The Archive of Stars",
        line: "The archivist stops mid-sentence and finishes the page in Ines Calloway's voice: \
               go on. Every star chart on every wall lights at once.",
    },
    EpilogueBeat {
        place: "The Sunforge",
        line: "The light passes a grave in the inner sun. The headset hung on the stone \
               crackles, and for one bar, flat and happy, someone sings.",
    },
    EpilogueBeat {
        place: "The Sunfolk Kraal",
        line: "Chief Ruth-Little carries the children out of the long-house. Everyone look up, \
               she says, as she was told.",
    },
    EpilogueBeat {
        place: "Merrow City",
        line: "The bubble rings like a bell. The merrow sing Sefa's verse about a town far \
               above that hums, and for once the town hums back.",
    },
    EpilogueBeat {
        place: "The Amber Court",
        line: "The comb sings from top to bottom, and the old pumps Teo kept keep time with it.",
    },
    EpilogueBeat {
        place: "The Town",
        line: "On a porch, an old woman named Wren Varga watches the cable light from the ground \
               to the Moon. She has looked up every night. Tonight something looks back.",
    },
    EpilogueBeat {
        place: "The Moon",
        line: "The builders' dock wakes. The Star Drive is your rig's engine now, and the sky \
               past the Moon is open. The Descent is over. Look up.",
    },
];

/// The whole ending as one text (the journal keeps it once the cable is lit).
#[must_use]
pub fn ending_text() -> String {
    let mut text = ENDING_LEAD.to_owned();
    for beat in &EPILOGUE {
        text.push_str("\n\n");
        text.push_str(beat.place);
        text.push_str(": ");
        text.push_str(beat.line);
    }
    text
}
