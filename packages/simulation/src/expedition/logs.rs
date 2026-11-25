//! The words of the Fathers' expedition.
//!
//! The logs they left in cipher canisters at their camps, wrecks and cairns, and the radio echoes and ghost recordings Dov Halloran sent up the drop
//! line or left on his recorder beside the builders' cable.
//!
//! The throughline runs to 80 logs (the design table is in the simulation README, "Wave 8
//! story"); Wave 8 writes Descents I to III (logs 1-30, echoes 1-12), Wave 9 Descents IV to
//! VI (logs 31-60, echoes 13-26, where the builders' own voices begin) and Wave 10 Descents VII
//! and VIII (logs 61-80, echoes 27-34: the builders' city, Ines in the Archive, and Amos alone
//! to the Heart on Dov's recorder). Placement and the journal read the tables only.

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
    /// The builders' own voices, bleeding through where the world wears thin below the inner
    /// sun (Wave 9). They speak no language anyone knows: the card gives Ines's translation
    /// from the margins of her book, and the echo's `speaker` is who wrote it down.
    Builder,
}

impl EchoKind {
    /// The card heading.
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Radio => "A RADIO ECHO",
            Self::Ghost => "A GHOST RECORDING",
            Self::Builder => "A BUILDER ECHO",
        }
    }
}

impl Echo {
    /// Whose voice the echo is, as the card names it.
    #[must_use]
    pub const fn voice(&self) -> &'static str {
        match self.kind {
            EchoKind::Radio | EchoKind::Ghost => self.speaker.name(),
            EchoKind::Builder => "Builder voices",
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

/// The chapters, one per Descent (Wave 10 wrote VII and VIII).
pub const CHAPTERS: [Chapter; 8] = [
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
    Chapter {
        descent: Descent::DrownedAndFrozen,
        title: "The People Below",
        revelation: "A sea under the rock, builder figures laid to rest in the ice, and a whole \
                     city of merrow in a bubble of air. The builders were not alone down here: \
                     whole peoples live below, and their oldest songs say the Tall Ones came \
                     down from the stars on a thread.",
    },
    Chapter {
        descent: Descent::HollowWorld,
        title: "The Hollow World",
        revelation: "The planet is hollow, with forests, tribes and beasts under a rock sky, and \
                     a small sun hung on the cable. The sun is a builder lamp, and the Star Core \
                     was a lamp for it: they lit their way down in stages. Dov Halloran lies in \
                     its light.",
    },
    Chapter {
        descent: Descent::StrangeLayers,
        title: "Where the World Wears Thin",
        revelation: "Near the builders' machine the world stops holding still: twins, turned \
                     gravity, slow time and rotting pixels. It is drawn from a model, and the \
                     drawing wears thin. The builders kept the plans of everything, in a city \
                     below.",
    },
    Chapter {
        descent: Descent::Ancients,
        title: "The Builders' City",
        revelation: "Below the thin places the builders' own city waits, swept and lit, with a \
                     thinking archivist that kept its lamps for an age. The planet is the \
                     housing of an engine and the Moon its dock. The Engine Halls hold that \
                     engine at an idle we hear as the hum, and the Archive charts roads between \
                     the stars.",
    },
    Chapter {
        descent: Descent::Heart,
        title: "The Star Drive",
        revelation: "At the centre of the world sleeps a Star Drive, the builders' engine: a star \
                     in a cage. Amos Varga reached it alone, with no rig and no key, and kept it \
                     company to the end. It wanted a rig to sit in and a key to turn. Wake it, \
                     and the cable to the Moon lights: you dug down to go up.",
    },
];

/// The chapter of `descent`, if one is written.
#[must_use]
pub fn chapter(descent: Descent) -> Option<&'static Chapter> {
    CHAPTERS.iter().find(|chapter| chapter.descent == descent)
}

/// The logs, in the order they were written and must be read (Wave 8: Descents I to III;
/// Wave 9: Descents IV to VI).
pub const LOGS: [ExpeditionLog; 80] = [
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
    // --- Descent IV: The Drowned and Frozen. Whole civilisations below. ----------------------
    ExpeditionLog {
        day: 121,
        title: "The Raft",
        author: Member::Amos,
        stratum: Stratum::UndergroundOcean,
        site: SiteKind::Camp,
        text: "The floor ran out and there was water, black and warm and flat as a table, going \
               on past the lamps. A sea, under the rock. We lashed the sled to empty fuel drums \
               and called it a raft. Ines paddles in a straight line for the cable. Of course \
               she does.",
    },
    ExpeditionLog {
        day: 126,
        title: "The Drowned Sub",
        author: Member::Sefa,
        stratum: Stratum::UndergroundOcean,
        site: SiteKind::Rig,
        text: "A builder submarine on the sea floor, hatch open, seats too long for us. Something \
               passed under the raft while we looked: a body like a river, scales the colour of \
               lamp oil, longer than the light. It did not want us. It was going home.",
    },
    ExpeditionLog {
        day: 133,
        title: "Two Fingers",
        author: Member::Bran,
        stratum: Stratum::BrineGlaciers,
        site: SiteKind::Camp,
        text: "Salt ice, and the sled slides wherever it likes. Dov lost two fingers to the \
               frost. I took them off with Teo's old snips and he joked the whole time: fewer \
               fingers, fewer dials to fiddle. I made him soup and he cried into it, after.",
    },
    ExpeditionLog {
        day: 137,
        title: "Figures in the Ice",
        author: Member::Ines,
        stratum: Stratum::BrineGlaciers,
        site: SiteKind::Marker,
        text: "Figures in the clear ice around the cable, standing in a ring, facing it. Tall, \
               too many joints in the arm. Not drowned: placed, the way we lay out our dead. \
               The builders had people, and the people had funerals.",
    },
    ExpeditionLog {
        day: 142,
        title: "Riding the Geyser",
        author: Member::Dov,
        stratum: Stratum::GeyserFields,
        site: SiteKind::Camp,
        text: "Steam vents that breathe on a count of ninety. Amos timed one, sat us on the sled \
               over it and up we went, two hundred feet, screaming. Soaked every tape. One \
               survived: the town's hymn. Typical. Writing this one in pencil.",
    },
    ExpeditionLog {
        day: 148,
        title: "The Bubble City",
        author: Member::Sefa,
        stratum: Stratum::MerrowCity,
        site: SiteKind::Camp,
        text: "A city in a bubble of air on the sea floor, lit green, and its people have gills \
               and patience: the merrow. They knew the Hive's Lamp-Bearers by another name, the \
               Tall Ones, and keep an archive of songs about them. I cannot leave this.",
    },
    ExpeditionLog {
        day: 155,
        title: "Sefa Stays",
        author: Member::Amos,
        stratum: Stratum::MerrowCity,
        site: SiteKind::Marker,
        text: "Sefa is staying with the merrow, to learn their songs and write them down. Four \
               of us now. She gave me her field book to carry to the bottom and back. I said I'd \
               bring it back. She said: bring yourself back, the book is a copy.",
    },
    ExpeditionLog {
        day: 158,
        title: "The Merrow Songs",
        author: Member::Ines,
        stratum: Stratum::MerrowCity,
        site: SiteKind::Camp,
        text: "Sefa sang us the oldest song before we left: the Tall Ones came down from the \
               stars on a thread, tied the thread to the fire at the world's heart, and slept. \
               The merrow sing it to make their children sleep. It works on me too.",
    },
    ExpeditionLog {
        day: 166,
        title: "Pressure",
        author: Member::Bran,
        stratum: Stratum::PearlAbyss,
        site: SiteKind::Camp,
        text: "The water leans on the hull like a hand and the colours go. Dov's heart is \
               failing: it skips, it races, his lips go blue at the bottom of every dive. I've \
               told no one. Amos would turn back, and Dov would never forgive us.",
    },
    ExpeditionLog {
        day: 171,
        title: "Through the Floor",
        author: Member::Amos,
        stratum: Stratum::PearlAbyss,
        site: SiteKind::Rig,
        text: "The Leviathan came up out of the pearl beds and broke the sub in half. We held on \
               to the sled. The sea floor cracked, the water went down through the crack, and \
               we went with it. We fell through the floor of the world, into light.",
    },
    // --- Descent V: The Hollow World. The planet is hollow. ----------------------------------
    ExpeditionLog {
        day: 172,
        title: "A Sun Inside",
        author: Member::Ines,
        stratum: Stratum::InnerSky,
        site: SiteKind::Camp,
        text: "We came down on a floating stone. Above us, rock. Below, forest, rivers, clouds, \
               and in the middle of it all a small sun hanging on the cable. It dims for night \
               and brightens for day. The planet is hollow. I measured it twice.",
    },
    ExpeditionLog {
        day: 175,
        title: "Inner Sunrise",
        author: Member::Dov,
        stratum: Stratum::InnerSky,
        site: SiteKind::Marker,
        text: "Taped the inner sunrise from the edge of the stone: forty minutes of birds you've \
               never heard and wind with nothing in its way. My last full tape. I'm leaving it \
               in this cairn. If you play it, mind the bit where I sing.",
    },
    ExpeditionLog {
        day: 181,
        title: "The Chief's Child",
        author: Member::Bran,
        stratum: Stratum::MegafaunaJungle,
        site: SiteKind::Camp,
        text: "People in the jungle: painted, wary, kind. The chief's little girl had a fever \
               nobody could break. I broke it, three nights of cold cloths and willow bark. Now \
               they bring me every scraped knee in the valley, and I am happy.",
    },
    ExpeditionLog {
        day: 186,
        title: "The Rig Is Gone",
        author: Member::Amos,
        stratum: Stratum::MegafaunaJungle,
        site: SiteKind::Rig,
        text: "A beast the size of the church at home stepped on the rig. Not angry: it never \
               saw us. The tribe laughed and lent us a grazer. We ride on its back now, the sled \
               slung under its belly. Ines has named it Survey Two.",
    },
    ExpeditionLog {
        day: 190,
        title: "Bran Stays",
        author: Member::Bran,
        stratum: Stratum::MegafaunaJungle,
        site: SiteKind::Camp,
        text: "I'm staying with the tribe. They have no healer and four babies due before the \
               rains. Someone has to be a mother down here. Amos, you great lump, mind Dov's \
               heart and make him take the pills. Three of you now. Go on.",
    },
    ExpeditionLog {
        day: 197,
        title: "Upside Down",
        author: Member::Ines,
        stratum: Stratum::UpsideDownIsles,
        site: SiteKind::Camp,
        text: "Islands hang from the rock sky, and on them up is down. We walked out onto one \
               upside down, the sun under our feet. From here you can see it plain: the cable \
               runs straight through the inner sun and on down out the other side.",
    },
    ExpeditionLog {
        day: 201,
        title: "Last Recording",
        author: Member::Dov,
        stratum: Stratum::UpsideDownIsles,
        site: SiteKind::Marker,
        text: "My heart's going. Bran told Amos, Amos told me, and I told them both I knew. My \
               last tape's in the cairn. Hello, town. Hello, whoever came down after us. It was \
               worth it, all of it, even the cold. Halloran, out.",
    },
    ExpeditionLog {
        day: 203,
        title: "Dov's Grave",
        author: Member::Amos,
        stratum: Stratum::TheSunforge,
        site: SiteKind::Grave,
        text: "Buried Dov Halloran where the sun's light never goes out. Ines cut his name into \
               a sunstone and I hung his headset on it, so he can hear the cable hum. He'd have \
               made a joke here. I can't think of one. Two of us.",
    },
    ExpeditionLog {
        day: 207,
        title: "A Lamp for the Lamp",
        author: Member::Ines,
        stratum: Stratum::TheSunforge,
        site: SiteKind::Marker,
        text: "The inner sun is machinery: rings of builder metal round a held flame, cooled by \
               the cable. A lamp, and the Star Core above was a lamp for this lamp. They lit \
               their way down in stages. Someone meant to walk all of this.",
    },
    ExpeditionLog {
        day: 212,
        title: "The Serpent's Locker",
        author: Member::Amos,
        stratum: Stratum::TheSunforge,
        site: SiteKind::Rig,
        text: "Something long and burning circles the sun, and it came for us. We hid in the \
               shadows until it passed, then found a builder locker with an anchor that holds \
               you to any wall. Below the sun the ground stops being honest.",
    },
    // --- Descent VI: The Strange Layers. Reality thins near the machine. ---------------------
    ExpeditionLog {
        day: 219,
        title: "The Other Ines",
        author: Member::Ines,
        stratum: Stratum::MirrorStrata,
        site: SiteKind::Camp,
        text: "I met myself today, walking the other way along the cable, writing in a book. \
               She raised her left hand when I raised my right. Amos says she was writing this \
               same log. I hope she keeps better notes than I do.",
    },
    ExpeditionLog {
        day: 222,
        title: "Two of Everything",
        author: Member::Amos,
        stratum: Stratum::MirrorStrata,
        site: SiteKind::Marker,
        text: "Two of every canister down here, two cairns, two of me. Which is real? Ines \
               laughed: the false one keys to nothing. Read them in order and the cipher tells \
               you. So that's what the cipher was for, all along.",
    },
    ExpeditionLog {
        day: 226,
        title: "The Only Up",
        author: Member::Ines,
        stratum: Stratum::GravityShear,
        site: SiteKind::Camp,
        text: "Down turns sideways here, then over. We walked a wall for a day and Amos was sick \
               twice. Only the cable keeps its direction, so we keep it on our left and trust \
               it. The builders made a road for people who didn't fall.",
    },
    ExpeditionLog {
        day: 231,
        title: "Low Resolution",
        author: Member::Amos,
        stratum: Stratum::PixelRot,
        site: SiteKind::Rig,
        text: "The rock goes to blocks. My hand goes to blocks if I look too long. Ines's maps \
               come out coarse, a picture of a picture. Found an old rig drawn in eight colours, \
               empty and quite real. I knocked on it. It knocked back, once.",
    },
    ExpeditionLog {
        day: 233,
        title: "The Plans",
        author: Member::Ines,
        stratum: Stratum::PixelRot,
        site: SiteKind::Camp,
        text: "I understand it now. The world is drawn from a model, and near the machine the \
               drawing wears thin and the model shows through. The builders kept the plans of \
               everything. Somewhere below is a copy of the world. I mean to read it.",
    },
    ExpeditionLog {
        day: 241,
        title: "A Day in an Hour",
        author: Member::Amos,
        stratum: Stratum::TimeSludge,
        site: SiteKind::Camp,
        text: "Time pools here like syrup in some places and runs like water in others. We slept \
               an hour by my watch and my beard came out grey. Ines says the canisters are \
               landing out of order in time. She was ready for that too.",
    },
    ExpeditionLog {
        day: 244,
        title: "Why the Cipher",
        author: Member::Ines,
        stratum: Stratum::TimeSludge,
        site: SiteKind::Marker,
        text: "For whoever follows: I keyed every canister to the one before because I knew \
               time would not hold near the machine. A log can arrive before its yesterday; the \
               key cannot. Read us in order and we stay a story.",
    },
    ExpeditionLog {
        day: 249,
        title: "By Echo",
        author: Member::Amos,
        stratum: Stratum::EchoCaves,
        site: SiteKind::Camp,
        text: "No light at all, not even the lamp's, as if the dark eats it. Things hunt by \
               sound. We clap, we listen, we move, and Ines draws the cave from the echoes. I \
               hold her hand. I haven't held a hand since my girl was small.",
    },
    ExpeditionLog {
        day: 255,
        title: "Coloured Doors",
        author: Member::Ines,
        stratum: Stratum::PrismDesert,
        site: SiteKind::Camp,
        text: "Sand of broken glass, light coming apart into colours. The builder doors here \
               open only to light of their own colour, and my lamp filters, the ones everyone \
               laughed at, open every one. Red, then gold, then a blue that hums.",
    },
    ExpeditionLog {
        day: 260,
        title: "The Glitch",
        author: Member::Amos,
        stratum: Stratum::PrismDesert,
        site: SiteKind::Rig,
        text: "The room rewrote itself round us: floor to wall, slow to fast, left to right, \
               something in the walls doing it on purpose. We came out with a builder drill \
               that goes through their metal like steam. Below is their city.",
    },
    // --- Wave 10: Descent VII, the Ancients. The builders' city and archive. ------------------
    ExpeditionLog {
        day: 266,
        title: "Streets",
        author: Member::Ines,
        stratum: Stratum::BuriedMegacity,
        site: SiteKind::Camp,
        text: "The rock opened onto a street. Lamps on poles, a tram on its rails, towers going \
               up into the dark like a forest of chimneys. Nothing ruined: swept, and waiting. \
               Then a lamp came on over our heads, and a voice asked our names.",
    },
    ExpeditionLog {
        day: 268,
        title: "You Are Late",
        author: Member::Amos,
        stratum: Stratum::BuriedMegacity,
        site: SiteKind::Marker,
        text: "The voice is the city's archivist, a thinking machine that kept the lamps for an \
               age. It knew the ball-on-a-cable mark on the sled. It said: you are late. Late \
               for what, I asked. It said: for waking. I didn't like how kind it sounded.",
    },
    ExpeditionLog {
        day: 271,
        title: "The Engine Housing",
        author: Member::Ines,
        stratum: Stratum::BuriedMegacity,
        site: SiteKind::Camp,
        text: "The archivist lit the plan on a wall. The planet is the housing of an engine, \
               the Moon is its dock and the cable is the mooring line. The builders came on a \
               ship, parked it in the world and went to sleep inside it. We live on the lid.",
    },
    ExpeditionLog {
        day: 277,
        title: "The Broken Leg",
        author: Member::Amos,
        stratum: Stratum::EngineHalls,
        site: SiteKind::Rig,
        text: "Halls of crushers keeping time, a beat you can dance to if you want to live. I \
               missed one. My leg is broken below the knee. Ines set it with a tram rail and \
               told me to stop being brave at her. We ride the pistons now. I swear a lot.",
    },
    ExpeditionLog {
        day: 284,
        title: "The Governors",
        author: Member::Ines,
        stratum: Stratum::EngineHalls,
        site: SiteKind::Camp,
        text: "The halls are the drive's governors: every crusher and piston is a weight on a \
               spring, holding the engine at an idle so low it hums instead of roars. That is \
               the hum the town has heard all our lives. Something has minded it for an age.",
    },
    ExpeditionLog {
        day: 291,
        title: "Star Charts",
        author: Member::Ines,
        stratum: Stratum::ArchiveOfStars,
        site: SiteKind::Camp,
        text: "An orrery as big as the Cathedral, and on every wall a sky, but not ours: charts \
               of suns past the Moon, with roads drawn between them. I copied the first \
               coordinates into the back of this book. Whoever reads this: they are yours.",
    },
    ExpeditionLog {
        day: 296,
        title: "Ines Stays",
        author: Member::Ines,
        stratum: Stratum::ArchiveOfStars,
        site: SiteKind::Marker,
        text: "Amos, I'm staying. There is a copy of the world in these stacks and I can read \
               perhaps a page a day. Don't argue: you can't argue well on that leg. Go on. The \
               archivist will teach me to wait, and I will teach it to read aloud. Ines.",
    },
    ExpeditionLog {
        day: 303,
        title: "Dear Wren",
        author: Member::Amos,
        stratum: Stratum::SleepingGarrison,
        site: SiteKind::Camp,
        text: "Wren, love. I'm on my own now, among sleeping war machines taller than our house. \
               You'd be eighteen. I keep seeing you on the porch the night I left, saying don't \
               forget to look up. I haven't. There's nothing up but rock. I look anyway.",
    },
    ExpeditionLog {
        day: 305,
        title: "The Garrison Wakes",
        author: Member::Amos,
        stratum: Stratum::SleepingGarrison,
        site: SiteKind::Rig,
        text: "My crutch clanged on a hull and the garrison woke in waves, eyes lighting down \
               the rows. I ran on one leg, which I don't recommend. They stopped at the line of \
               the old wall. They don't chase; they guard. I'm the wrong shape for a threat.",
    },
    ExpeditionLog {
        day: 309,
        title: "A Key from Every Descent",
        author: Member::Amos,
        stratum: Stratum::TheSealGate,
        site: SiteKind::Camp,
        text: "The Seal Gate: a wall of gold with a hollow for a key from every Descent. A core \
               stone, a spore heart, a pearl, a sunstone. I have none of them. I have a crutch, \
               a book and a sled with our name on it. I sat here a long time.",
    },
    ExpeditionLog {
        day: 312,
        title: "A Letter Home",
        author: Member::Amos,
        stratum: Stratum::TheSealGate,
        site: SiteKind::Marker,
        text: "To whoever comes next: there is food in the sled and a spare lamp. To the town: \
               we were right. It hums, and it is something. Tell Wren her father is fine and \
               fat and very far down. I leave this where the next crew will pass. Amos Varga.",
    },
    ExpeditionLog {
        day: 314,
        title: "Let Through",
        author: Member::Amos,
        stratum: Stratum::TheSealGate,
        site: SiteKind::Rig,
        text: "The Gatekeeper stood up, bigger than the gate and older than the hills. It looked \
               a long time at the cable mark Bran painted on the sled, and then it stepped \
               aside. Not for me: for the mark. Whatever it means, I'm carrying it down.",
    },
    // --- Wave 10: Descent VIII, the Heart. The planet's core is a sleeping Star Drive. --------
    ExpeditionLog {
        day: 318,
        title: "Lighter",
        author: Member::Amos,
        stratum: Stratum::DarkMatterVeins,
        site: SiteKind::Camp,
        text: "Black ore that weighs less than nothing. I filled the sled with it for a joke and \
               it tried to float off. The more I carry, the lighter I get. My leg likes it. Ines \
               would have had a theory. I laughed out loud, alone, for the first time.",
    },
    ExpeditionLog {
        day: 321,
        title: "The Idle",
        author: Member::Amos,
        stratum: Stratum::DarkMatterVeins,
        site: SiteKind::Marker,
        text: "The hum is loud enough here to feel in my teeth: slow, steady, patient. Dov said \
               it was slower than his heart and he found that a comfort. It's the drive, \
               idling. A heart beating for an engine asleep since before people.",
    },
    ExpeditionLog {
        day: 325,
        title: "The Moon Through the Rock",
        author: Member::Amos,
        stratum: Stratum::SingularityFields,
        site: SiteKind::Camp,
        text: "Gravity pools here like water and bends the light round it. In one well the rock \
               went clear as glass and I saw, straight up the cable, a pale round thing. The \
               Moon, Wren. Through the whole world. I looked up. You were right.",
    },
    ExpeditionLog {
        day: 327,
        title: "The Load Gives Out",
        author: Member::Amos,
        stratum: Stratum::SingularityFields,
        site: SiteKind::Rig,
        text: "The sled went into a well and folded like paper, Bran's paint and all. I cut the \
               board off, THE FATHERS' LOAD, and strapped it to my back with Dov's recorder. On \
               foot from here, on a crutch, with our name on my back like a fool. Good.",
    },
    ExpeditionLog {
        day: 330,
        title: "The Rails",
        author: Member::Amos,
        stratum: Stratum::CoreOcean,
        site: SiteKind::Camp,
        text: "A sea of liquid metal, bright as a mirror and hot as a forge, and over it the \
               builders' rails, humming. I rode a rail car across lying on my back and watched \
               my face go by underneath. It looked old. It looked like my father.",
    },
    ExpeditionLog {
        day: 333,
        title: "To the Five",
        author: Member::Amos,
        stratum: Stratum::CoreOcean,
        site: SiteKind::Marker,
        text: "Teo, the pumps. Sefa, the songs. Bran, the babies. Dov, the tapes. Ines, the \
               stacks. You each found the thing worth staying for, and I kept walking. I know \
               now what mine is. It's at the bottom. Thank you for carrying me this far.",
    },
    ExpeditionLog {
        day: 336,
        title: "The Star Drive",
        author: Member::Amos,
        stratum: Stratum::TheHeart,
        site: SiteKind::Camp,
        text: "The Heart: a chamber of white gold the size of a sky, and in it the drive, \
               asleep, turning so slowly it hums. A star in a cage. It wants a rig to sit in and \
               a key to turn, and I have a crutch and a board. So I'll sit with it, and keep it.",
    },
    ExpeditionLog {
        day: 349,
        title: "Wake It",
        author: Member::Amos,
        stratum: Stratum::TheHeart,
        site: SiteKind::Rig,
        text: "My lamp is low and so am I. I tied our board to the cable root, mark up, so it \
               knows us. You came down after us. Of course you did. Wake it. Go up. Wren, if \
               it's you or yours: look up. I'll be the light on the cable. Your father.",
    },
];

/// Radio echoes, ghost recordings and builder echoes, in depth order (Wave 8: Descents I to
/// III; Wave 9: Descents IV to VI).
pub const ECHOES: [Echo; 34] = [
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
    // --- Wave 9: Descents IV to VI. Dov's tapes run to his death in the Upside-Down Isles;
    // below his grave the builders' own voices take over (and once, out of time, his). ------
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::UndergroundOcean,
        along: 400,
        title: "Recording, Day 123",
        text: "Recording. Water everywhere, warm as a bath. Amos rows in his sleep. Something \
               big sang under the raft last night, one long low note, and the cable sang back.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::BrineGlaciers,
        along: 500,
        title: "Recording, Day 134",
        text: "Recording. Eight fingers. Plenty for the dials. Bran says one more joke about it \
               and she takes a toe. (Laughter.) Cold. So cold the tape's gone stiff.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::GeyserFields,
        along: 600,
        title: "Recording, Day 143",
        text: "Recording. (The town's hymn, warped and waterlogged, and under it Dov humming \
               along, off key.) The only tape that survived the geyser. Figures.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::MerrowCity,
        along: 500,
        title: "Recording, Day 156",
        text: "Recording. Sefa is singing with the merrow. You should hear it. No, really: I'm \
               leaving the recorder running. (A long, bubbling chorus, and one human voice.)",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::PearlAbyss,
        along: 700,
        title: "Recording, Day 168",
        text: "Recording. Quiet, so Bran won't hear. The heart's doing a little dance. If this \
               is the last tape: the dials were never the problem. It was always the hands.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::InnerSky,
        along: 300,
        title: "Recording, Day 172",
        text: "Recording. Light. Real light, warm on my face. I'm not going to describe it. I'm \
               just going to hold the microphone up. (Wind. Birds. Someone laughing: Ines.)",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::MegafaunaJungle,
        along: 500,
        title: "Recording, Day 188",
        text: "Recording. Riding a beast the size of a barn. Amos calls it the Fathers' Load, \
               Mark Two. The tribe's children run after us, singing Bran's name.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::UpsideDownIsles,
        along: 600,
        title: "Recording, Day 200",
        text: "Recording. Upside down, looking up at a sun under my boots. The hum is very loud \
               here. It's slower than my heart. I find that a comfort.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::TheSunforge,
        along: 600,
        title: "The Lamp-Lighters",
        text: "(Many voices, low and slow, counting in time with the cable's hum. Ines wrote \
               the meaning in the margin:) KEEP THE LAMPS LIT. WE ARE GOING DOWN TO SLEEP.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::MirrorStrata,
        along: 500,
        title: "Two Voices",
        text: "(One builder voice twice, half a beat apart, arguing with its own echo. Ines's \
               margin:) ONE OF US STAYS ABOVE. ONE OF US GOES DOWN. WHICH OF US IS THIS?",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::PixelRot,
        along: 500,
        title: "The Plan",
        text: "(A voice reading numbers, thousands of them, flat and patient. Ines's margin:) \
               THE PLAN OF THE WORLD, LINE BY LINE, SO THAT IT CAN BE MADE AGAIN.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Dov,
        stratum: Stratum::TimeSludge,
        along: 400,
        title: "Recording, Day ???",
        text: "Recording. I don't know what day it is. I don't know why I'm here: Amos buried \
               me three canisters back. Time's loose down here. If you hear this, wave. I'll \
               wave back.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::EchoCaves,
        along: 600,
        title: "Listen",
        text: "(Builder voices in the dark, clicking in patterns. Ines's margin:) WE SEE BY \
               LISTENING. THE MACHINE IS CLOSE NOW. IT IS DREAMING OF THE SKY.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::PrismDesert,
        along: 700,
        title: "The City Below",
        text: "(A clear voice over something like bells. Ines's margin:) BELOW: OUR CITY, OUR \
               ARCHIVE, OUR ENGINE. WHOEVER YOU ARE, YOU ARE LATE.",
    },
    // --- Wave 10: Descents VII and VIII. The city's own recordings (the archivist translates
    // them, from Ines's stacks), Ines reading aloud in the Archive, and Amos on Dov's
    // recorder, carried all the way to the Heart. ---------------------------------------------
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::BuriedMegacity,
        along: 600,
        title: "The Last Tram",
        text: "(A station voice, patient, repeating over a chime. The archivist translates:) \
               LAST TRAM TO THE SLEEPING HALLS. MIND THE GAP. WAKE US WHEN THE KEYS COME HOME.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::EngineHalls,
        along: 500,
        title: "The Governors' Song",
        text: "(Workers singing in time with the crushers. The archivist translates:) HOLD IT \
               LOW, HOLD IT SLOW. LET IT DREAM AND NOT GO.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Ines,
        stratum: Stratum::ArchiveOfStars,
        along: 700,
        title: "Page Four Thousand",
        text: "(Ines, older, reading aloud to a patient machine:) ...and the roads between the \
               suns are kept open by... No. Again. Slowly. You'll have it by the time they come.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::SleepingGarrison,
        along: 500,
        title: "Stand Guard",
        text: "(A commander's voice over rows of humming engines. The archivist translates:) \
               GUARD THE GATE. NOT AGAINST THEM. FOR THEM. THEY WILL BE SMALL, AND LATE.",
    },
    Echo {
        kind: EchoKind::Builder,
        speaker: Member::Ines,
        stratum: Stratum::TheSealGate,
        along: 600,
        title: "The Keys",
        text: "(Voices naming places, one after another. The archivist translates:) THE CRUST. \
               THE CELLAR. THE DARK. THE SEA. THE HOLLOW. THE THIN PLACES. BRING ONE FROM EACH.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Amos,
        stratum: Stratum::DarkMatterVeins,
        along: 500,
        title: "Recording, Day 319",
        text: "Recording. Dov, your machine still works. I can't sing, so I'll let it hear the \
               hum. (A long, slow hum, and a man breathing.) That one's for you.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Amos,
        stratum: Stratum::CoreOcean,
        along: 600,
        title: "Recording, Day 331",
        text: "Recording. Halfway over the metal sea, talking to keep the dark off. Wren: Bran's \
               stew, the one she'd never give the recipe for. It was mostly onions. Now you know.",
    },
    Echo {
        kind: EchoKind::Ghost,
        speaker: Member::Amos,
        stratum: Stratum::TheHeart,
        along: 900,
        title: "Recording, Day 350",
        text: "Recording. It's warm here. The drive turns in its sleep and the hum goes up and \
               down like breathing. I'm going to rest now. When you wake it, I'll hear. (The \
               hum. The tape runs out.)",
    },
];
