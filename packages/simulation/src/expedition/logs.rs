//! The words of the Fathers' expedition.
//!
//! The logs they left in cipher canisters at their camps, wrecks and cairns, and the radio echoes and ghost recordings Dov Halloran sent up the drop
//! line or left on his recorder beside the builders' cable.
//!
//! The throughline runs to about 80 logs (the design table is in the simulation README, "Wave 8
//! story"); Wave 8 writes Descents I to III (logs 1-30, echoes 1-12). Later waves append to
//! [`LOGS`] and [`ECHOES`] in depth order: placement and the journal read the tables only.

use crate::descent::{Descent, Stratum};

use super::{Member, SiteKind};

/// One expedition log: the entry an expedition member wrote and left in a cipher canister.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpeditionLog {
    /// The expedition's day the entry was written on.
    pub day: u16,
    pub title: &'static str,
    pub author: Member,
    /// The stratum the canister was left in (its site is placed there, below the previous
    /// log's).
    pub stratum: Stratum,
    /// What the canister was left at.
    pub site: SiteKind,
    pub text: &'static str,
}

impl ExpeditionLog {
    /// The Descent the log belongs to.
    #[must_use]
    pub const fn descent(&self) -> Descent {
        self.stratum.descent()
    }
}

/// How an echo reaches the player.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EchoKind {
    /// Dov's transmissions on the drop line, caught by the rig's radio while the line still
    /// reached the surface (Descent I and the top of Descent II).
    Radio,
    /// Dov's recorder, left on the cable after the storm cut the line: it plays as a rig
    /// passes.
    Ghost,
}

impl EchoKind {
    /// The card heading.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Radio => "A RADIO ECHO",
            Self::Ghost => "A GHOST RECORDING",
        }
    }
}

/// A radio echo or ghost recording: it plays once, to the whole crew, the first time a rig
/// reaches its depth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Echo {
    pub kind: EchoKind,
    pub speaker: Member,
    pub stratum: Stratum,
    /// How far through its stratum (per mille) the echo plays.
    pub along: u16,
    pub title: &'static str,
    pub text: &'static str,
}

/// A Descent's chapter of the expedition story: what the logs of that Descent reveal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Chapter {
    pub descent: Descent,
    pub title: &'static str,
    /// The revelation, shown in the journal once the chapter's last log is read.
    pub revelation: &'static str,
}

/// The chapters written so far (one per Descent; later waves append IV to VIII).
pub const CHAPTERS: [Chapter; 3] = [
    Chapter {
        descent: Descent::Crust,
        title: "The Shaped World",
        revelation: "The bands are laid like courses of brick, the shelves all lean to the \
                     cable and the columns are poured to one width. Nothing here is just rock: \
                     the world was shaped, and the Crust is a lid.",
    },
    Chapter {
        descent: Descent::OldDeep,
        title: "The Builders' Cellar",
        revelation: "Sleeping machines, steps for longer legs, bones laid out in rows and a \
                     glyph on every door: a ball on a cable. The builders were here long before \
                     the town, and the Star Core is only their lamp.",
    },
    Chapter {
        descent: Descent::LivingDark,
        title: "What Grows in the Dark",
        revelation: "Below the lamp, life learned to make its own light. The Hive remembers the \
                     Lamp-Bearers, the walls beat with the hum, and the queens' carving shows \
                     the cable running from a sleeping engine at the Heart up to the Moon.",
    },
];

/// The chapter of `descent`, if one is written.
#[must_use]
pub fn chapter(descent: Descent) -> Option<&'static Chapter> {
    CHAPTERS.iter().find(|chapter| chapter.descent == descent)
}

/// The logs, in the order they were written and must be read (Wave 8: Descents I to III).
pub const LOGS: [ExpeditionLog; 30] = [
    // --- Descent I: The Crust. The world was shaped. -----------------------------------------
    ExpeditionLog {
        day: 1,
        title: "The Fathers' Load",
        author: Member::Amos,
        stratum: Stratum::Topsoil,
        site: SiteKind::Camp,
        text: "Six of us, one rig, one sled and a drop line to Dov's radio shack. The town calls \
               us the Fathers. Bran says two of us are mothers and Teo is barely shaving, but \
               she painted it on the sled anyway: THE FATHERS' LOAD. We go down until we find \
               out what hums.",
    },
    ExpeditionLog {
        day: 2,
        title: "Roots Going Down",
        author: Member::Teo,
        stratum: Stratum::Topsoil,
        site: SiteKind::Marker,
        text: "Roots this deep shouldn't be alive. These are, and every one grows the same way: \
               down, like it knows something warm is waiting at the bottom. Built a cairn so \
               the next fool knows we came through.",
    },
    ExpeditionLog {
        day: 5,
        title: "Wet Clay",
        author: Member::Bran,
        stratum: Stratum::ClayBeds,
        site: SiteKind::Camp,
        text: "Teo cracked an aquifer and we spent the morning bailing out the tent. Lost the \
               salt and half the coffee. Morale fine. The clay comes up in flat sheets, smooth \
               as dinner plates. Ines has gone very quiet about that.",
    },
    ExpeditionLog {
        day: 7,
        title: "Courses",
        author: Member::Ines,
        stratum: Stratum::ClayBeds,
        site: SiteKind::Marker,
        text: "The clay is laid in courses, a hand thick, the same for a hundred rows. Water \
               doesn't lay clay like that. Masons do. I've put a cipher on these canisters: \
               each one opens with a word from the one before. If you can read this, you've \
               been reading us in order. Good.",
    },
    ExpeditionLog {
        day: 11,
        title: "Gravel Hollow",
        author: Member::Amos,
        stratum: Stratum::GreyStone,
        site: SiteKind::Camp,
        text: "Gravel Hollow put us up for two nights. Old Tully at the forge says his grandad \
               found the rails we're following already laid, and nobody in the Hollow laid \
               them. Bought lamp oil. Teo bought a lucky stone off a kid. It was a rock.",
    },
    ExpeditionLog {
        day: 13,
        title: "The First Wreck",
        author: Member::Teo,
        stratum: Stratum::GreyStone,
        site: SiteKind::Rig,
        text: "A rig older than any yard model. No maker's mark, no rivets: the hull is one \
               piece, like it was poured. Its drill points straight down and its cab faces the \
               cable. Whoever drove it was following the same thing we are.",
    },
    ExpeditionLog {
        day: 18,
        title: "Slide",
        author: Member::Bran,
        stratum: Stratum::SlateShelves,
        site: SiteKind::Camp,
        text: "A slate shelf slid out from under the sled and took the stove with it. Amos dug \
               it out swearing in two languages. Ines measured shelves while he swore: every \
               one tilts the same few degrees, toward the cable.",
    },
    ExpeditionLog {
        day: 20,
        title: "The Cable",
        author: Member::Ines,
        stratum: Stratum::SlateShelves,
        site: SiteKind::Marker,
        text: "We've called it the cable since the Hollow: a rail of stone no drill scratches, \
               running dead plumb for a thousand rows. A plumb line that long is not geology. \
               It is the spine of a building, and we are climbing down its stairwell.",
    },
    ExpeditionLog {
        day: 24,
        title: "Columns",
        author: Member::Sefa,
        stratum: Stratum::BasaltColumns,
        site: SiteKind::Camp,
        text: "The basalt cools into six-sided columns, and every column is the same width to \
               the thumb. Lava makes columns. It does not make identical ones. Ines has a word \
               for it now and she says it every night by the stove: shaped. The world was \
               shaped.",
    },
    ExpeditionLog {
        day: 27,
        title: "The Lid",
        author: Member::Amos,
        stratum: Stratum::BasaltColumns,
        site: SiteKind::Rig,
        text: "The floor gave way under the camp. We lost the second sled and nearly lost Dov's \
               line. Ines says the Crust is a lid set over the deep like a cork, and we just \
               pulled it. Whoever made it made it to keep something in, or something out. \
               Going down anyway.",
    },
    // --- Descent II: The Old Deep. The builders were here long ago. ------------------------
    ExpeditionLog {
        day: 31,
        title: "Singing Glass",
        author: Member::Sefa,
        stratum: Stratum::CrystalFaults,
        site: SiteKind::Camp,
        text: "The crystal rings when the drill touches it, and the ring travels. Teo set off a \
               chain and the whole gallery sang one long note. Dov swears it came back up his \
               line from the surface a day later.",
    },
    ExpeditionLog {
        day: 36,
        title: "Sea Floor",
        author: Member::Sefa,
        stratum: Stratum::FossilOceans,
        site: SiteKind::Camp,
        text: "Shells. Fish. A whole sea floor standing on end, this far down. And the gas \
               breathes in and out of the basins like a tide. Bran won't light the stove below \
               the line Ines chalked on the wall.",
    },
    ExpeditionLog {
        day: 38,
        title: "Bones in Rows",
        author: Member::Sefa,
        stratum: Stratum::FossilOceans,
        site: SiteKind::Rig,
        text: "The skeletons lie in rows. Not a herd that died together: rows, sorted by size, \
               every skull pointing at the cable. Something laid them out. Something kept a \
               collection.",
    },
    ExpeditionLog {
        day: 43,
        title: "The Cathedral",
        author: Member::Ines,
        stratum: Stratum::PressureCathedrals,
        site: SiteKind::Camp,
        text: "A chamber so big the lamp never finds the far wall. Steps cut into it, for legs \
               longer than ours. At its heart a dead lift shaft with the cable through it like \
               thread through a needle. Someone came and went from here, often.",
    },
    ExpeditionLog {
        day: 45,
        title: "Valves",
        author: Member::Teo,
        stratum: Stratum::PressureCathedrals,
        site: SiteKind::Marker,
        text: "Got one of the lift's valves to turn. Jets everywhere, the whole hall shaking. \
               Amos held it open while I climbed. Next time it wants three people and a better \
               plan. Left the valve key in the cairn for whoever's next.",
    },
    ExpeditionLog {
        day: 51,
        title: "The Glow",
        author: Member::Bran,
        stratum: Stratum::RadioactiveHollow,
        site: SiteKind::Camp,
        text: "Everyone's counters are ticking. Four hours down, two back in the lead tent. \
               Dov's hands shake and he says it's the cold. It isn't cold. I'm rationing the \
               iodine and I am not taking questions.",
    },
    ExpeditionLog {
        day: 56,
        title: "Doors",
        author: Member::Amos,
        stratum: Stratum::AncientMachineLayer,
        site: SiteKind::Rig,
        text: "Doors. Real doors in the rock that slide open when you dig near them. Pistons, \
               conveyors, a hall of machines asleep under a hand of dust. They aren't ruins. \
               Ruins are broken. These are waiting.",
    },
    ExpeditionLog {
        day: 58,
        title: "The Builders",
        author: Member::Ines,
        stratum: Stratum::AncientMachineLayer,
        site: SiteKind::Camp,
        text: "One glyph over every door: a circle on a line. A ball on a cable. The builders \
               were here long before the town, long before the Hollow, and they laid the Crust \
               over all of this like a floor over a cellar. We're walking around their cellar \
               with lamps.",
    },
    ExpeditionLog {
        day: 63,
        title: "Weightless",
        author: Member::Teo,
        stratum: Stratum::VoidGeodeFields,
        site: SiteKind::Marker,
        text: "Inside the big geodes you weigh nothing. I did a somersault holding a full \
               toolbox. Sefa threw up. Ines says gravity in here is being held back on purpose, \
               like a door somebody is holding open.",
    },
    ExpeditionLog {
        day: 69,
        title: "Storm on the Line",
        author: Member::Dov,
        stratum: Stratum::MantleStormZone,
        site: SiteKind::Camp,
        text: "A heat storm took the drop line. Last thing I sent up: we're alive, we're going \
               on. I don't know if it got through. Without the line I'm just a man with a \
               radio, so I'll keep recording and leave the tapes on the cable. Somebody might \
               come down after us.",
    },
    ExpeditionLog {
        day: 74,
        title: "Not the Bottom",
        author: Member::Amos,
        stratum: Stratum::MantleStormZone,
        site: SiteKind::Rig,
        text: "We reached the core. Every story in town ends here, at the Star Core, the bottom \
               of the world. It isn't. The cable runs on under it. The core is a lamp the \
               builders hung in the shaft to light the way down, and Sefa thinks something \
               lives below the light.",
    },
    // --- Descent III: The Living Dark. Life adapted to the dark. ---------------------------
    ExpeditionLog {
        day: 80,
        title: "Spores",
        author: Member::Sefa,
        stratum: Stratum::FungalForest,
        site: SiteKind::Camp,
        text: "Mushrooms taller than the Hollow's chapel; their caps throw you if you land on \
               them. The spores make the lamps swim with colour. Everything here eats the dark \
               and grows toward the cable: toward the warmth, toward the hum.",
    },
    ExpeditionLog {
        day: 82,
        title: "Bran's Rule",
        author: Member::Bran,
        stratum: Stratum::FungalForest,
        site: SiteKind::Marker,
        text: "New rule: nobody licks anything. I am looking at you, Teo. Left a jar of spore \
               paste in the cairn: rub it on the visor when the colours get bad. Or don't, and \
               have a lovely time.",
    },
    ExpeditionLog {
        day: 87,
        title: "The Roots Close",
        author: Member::Amos,
        stratum: Stratum::RootLabyrinth,
        site: SiteKind::Camp,
        text: "The roots grow back behind us. We cut a tunnel at noon and by supper it was gone. \
               Now we burn a path and paint it. The roots never touch the cable, the one place \
               they won't grow. Even the roots know whose house this is.",
    },
    ExpeditionLog {
        day: 90,
        title: "Lost",
        author: Member::Ines,
        stratum: Stratum::RootLabyrinth,
        site: SiteKind::Rig,
        text: "Two days lost in the roots; found our own rig by walking in a circle. Maps are \
               useless where the walls move, so I've stopped drawing walls. I draw the cable, \
               and how far from it we are. It's the only thing down here that stays put.",
    },
    ExpeditionLog {
        day: 96,
        title: "The Hive",
        author: Member::Sefa,
        stratum: Stratum::TheHive,
        site: SiteKind::Camp,
        text: "Insects the size of the rig, and they talk: clicks, hums and dances. The workers \
               traded jelly for lamp oil; the soldiers did not trade. They call the builders \
               the Lamp-Bearers, and say the Lamp-Bearers went down and never came back up \
               either.",
    },
    ExpeditionLog {
        day: 99,
        title: "Teo Stays",
        author: Member::Amos,
        stratum: Stratum::TheHive,
        site: SiteKind::Marker,
        text: "Teo is staying with the Hive. The old builder pumps that warm their nursery are \
               failing and he's the only one of us who can fix them. I told him his mother \
               would kill me. He said: tell her I found my job. Left him the good wrench.",
    },
    ExpeditionLog {
        day: 104,
        title: "Heartbeat",
        author: Member::Bran,
        stratum: Stratum::FleshCaverns,
        site: SiteKind::Camp,
        text: "The walls are warm and soft, they heal where you cut them, and they beat. Slow. \
               I counted it against my watch: the same beat as the hum up top. We have heard \
               this all our lives and called it the ground.",
    },
    ExpeditionLog {
        day: 110,
        title: "Living Lights",
        author: Member::Sefa,
        stratum: Stratum::GlowSea,
        site: SiteKind::Camp,
        text: "No light here but what the animals make: jellies, lure fish, and something big \
               that dangles a lamp to bring you close. Life learned to see without a sun, then \
               learned to make its own. I could study this sea for the rest of my life. I may \
               have to.",
    },
    ExpeditionLog {
        day: 116,
        title: "The Engine Below",
        author: Member::Ines,
        stratum: Stratum::GlowSea,
        site: SiteKind::Rig,
        text: "The Hive's oldest queen showed us a carving: the cable running down to a ball of \
               light, and up to the Moon. Like the old tomb page: a cable from the deep core to \
               the hanging stone. It is no core, she said. It is an engine, and it sleeps. Five \
               of us left. Going down.",
    },
];

/// Radio echoes and ghost recordings, in depth order (Wave 8: Descents I to III).
pub const ECHOES: [Echo; 12] = [
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::Topsoil,
        along: 600,
        title: "Testing, Testing",
        text: "...Halloran on the drop line, testing. Amos, if you can hear this, your daughter \
               says don't forget to look up. I don't know what that means either. Over.",
    },
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::GreyStone,
        along: 300,
        title: "Good to the Hollow",
        text: "...signal's good to the Hollow. Tully says the rails go deeper than anyone ever \
               followed them. Bran says bring back cheese. Over.",
    },
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::BasaltColumns,
        along: 500,
        title: "Breathing",
        text: "...hum's louder on the line tonight. Slow, like something breathing. Ines says \
               it's the cable singing in the heat. Ines says a lot of things. Over.",
    },
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::CrystalFaults,
        along: 250,
        title: "A Bell",
        text: "...the glass is singing back up the line, the same note, a day late. Like the \
               whole world's a bell and we just struck it...",
    },
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::PressureCathedrals,
        along: 500,
        title: "A Church for Giants",
        text: "...Halloran. We're standing in a church built for giants. If anyone up top keeps \
               a list of things I never believed in, add this one. Over.",
    },
    Echo {
        kind: EchoKind::Radio,
        speaker: Member::Dov,
        stratum: Stratum::MantleStormZone,
        along: 350,
        title: "The Last Transmission",
        text: "...storm on the line... alive, all six... going on. If this reaches the town, \
               tell them the bottom isn't the bottom. Tell them we'll...",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::MantleStormZone,
        along: 900,
        title: "Recording, Day 75",
        text: "Recording. No line now, so I'll leave these where the cable passes. If you can \
               hear this, you came down after us. Hello. Sorry about the mess.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::FungalForest,
        along: 300,
        title: "Recording, Day 81",
        text: "Recording. The colours are in my head now, not just my eyes. Bran hums when she's \
               scared. She's humming the same note as the walls.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::RootLabyrinth,
        along: 600,
        title: "Recording, Day 91",
        text: "Recording. Ines walked us out of the roots counting steps from the cable. She \
               doesn't sleep. She draws the line over and over, with a little ball at the \
               bottom.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::TheHive,
        along: 700,
        title: "Recording, Day 98",
        text: "Recording. (Teo, far off: \"Tell Mum the pumps work!\") He's happy. First time \
               I've seen anyone happy down here. We leave him tomorrow.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::FleshCaverns,
        along: 500,
        title: "Recording, Day 105",
        text: "Recording. You can hear it, can't you? Under my voice. Thum. Thum. That isn't \
               the tape.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::GlowSea,
        along: 800,
        title: "Recording, Day 113",
        text: "Recording. Amos says the Moon isn't a moon. It's a dock, the cable's a mooring \
               line and the thing at the bottom is the ship's engine. I said, who parks a ship \
               in a planet? He said: someone who means to come back.",
    },
];
