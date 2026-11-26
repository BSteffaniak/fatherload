//! The galaxy story's words beyond the chapters (Wave 13 story).
//!
//! The space logs (the road's throughline, like the Descent's expedition logs), the builders'
//! fragments found by exploring, and the archivist's remarks on space finds.
//!
//! Every table is append-only: indices and ids are saved and sent, never renumbered.

/// Who a space log is in the voice of.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogVoice {
    /// A builder record: a waymark's script, a foundry's order, the Harbour's books.
    Builder,
    /// Ines Calloway's margin notes on the charts she copied in the Archive of Stars.
    Ines,
    /// The archivist, remembering.
    Archivist,
    /// The Concord's plate in the Arbiter's wreck.
    Concord,
    /// A builder's voice kept in crystal.
    Spire,
    /// A letter from home.
    Wren,
}

impl LogVoice {
    /// Who signs it.
    #[must_use]
    pub const fn signature(self) -> &'static str {
        match self {
            Self::Builder => "builder record",
            Self::Ines => "Ines Calloway, in the chart's margin",
            Self::Archivist => "the archivist",
            Self::Concord => "the Concord's plate",
            Self::Spire => "the memory spire",
            Self::Wren => "Wren Varga",
        }
    }
}

/// One space log.
#[derive(Clone, Copy, Debug)]
pub struct SpaceLog {
    /// Stable id (`galaxy.log.name`).
    pub id: &'static str,
    pub voice: LogVoice,
    pub title: &'static str,
    pub text: &'static str,
}

const fn log(
    id: &'static str,
    voice: LogVoice,
    title: &'static str,
    text: &'static str,
) -> SpaceLog {
    SpaceLog {
        id,
        voice,
        title,
        text,
    }
}

/// The road's logs, in the order the chapters open them.
pub static SPACE_LOGS: [SpaceLog; 22] = [
    // Chapter 1: the first road.
    log(
        "galaxy.log.waymark_one",
        LogVoice::Builder,
        "Waymark One",
        "Waymark one of the Long Road. The Harbour lies ahead, the dock behind. Keep the \
         cable warm. Whoever lights this lamp carries a drive: go on.",
    ),
    log(
        "galaxy.log.ines_first",
        LogVoice::Ines,
        "First Margin",
        "Copied the outermost chart tonight. Dov says I am drawing roads to nowhere. Dov has \
         never once looked at a map the right way up. If anyone ever follows these: the \
         builders wrote their roads from home outward. So read them backwards to find home.",
    ),
    // Chapter 2: the peoples of the near stars.
    log(
        "galaxy.log.elders",
        LogVoice::Archivist,
        "What the Elders Saw",
        "The ships they saw were ours. We flew them silent so as not to frighten anyone. It \
         seems we frightened them anyway, and they made songs about it. I would like to hear \
         the songs. Ask them to sing slowly.",
    ),
    log(
        "galaxy.log.ines_people",
        LogVoice::Ines,
        "Margin: People",
        "There are names on the outer charts that are not builder names. Little marks beside \
         some stars, like a host's sign on a door. Somebody lived out there. I hope they were \
         kind to them.",
    ),
    // Chapter 3: the foundry.
    log(
        "galaxy.log.foundry_order",
        LogVoice::Builder,
        "Foundry Nine: Standing Order",
        "Foundry Nine to all Wardens: hold the Harbour. Let nothing near the sleepers that \
         does not carry a drive key. This order stands until it is cancelled. (It was never \
         cancelled.)",
    ),
    log(
        "galaxy.log.cradles",
        LogVoice::Archivist,
        "The Cradles",
        "We built world-ships here. Every cradle is a ship that left. One did not leave: it \
         was too heavy with what it carried, and its drive would not wake. You know which. You \
         were born on it.",
    ),
    // Chapter 4: the quiet world.
    log(
        "galaxy.log.dying_star",
        LogVoice::Builder,
        "The Last Archive",
        "Our star is failing. We have hollowed eleven worlds and hung a drive in each. We will \
         carry everyone. We will call what we carry the Load, so that no one forgets it is \
         heavy, and no one forgets it is precious.",
    ),
    log(
        "galaxy.log.eleven",
        LogVoice::Builder,
        "Eleven Worlds",
        "Ten worlds woke and flew for the Harbour. The eleventh slept. We could not leave the \
         children on a cold world, and we could not wake it. We will think of something. We \
         are good at thinking of something.",
    ),
    log(
        "galaxy.log.ines_eleven",
        LogVoice::Ines,
        "Margin: Eleven",
        "The archivist counted the charts' circles for me tonight: eleven, and one of them \
         drawn again and again, darker each time, as if someone could not stop going over it. \
         Ours, I think. Somebody worried about us very hard for a very long time.",
    ),
    // Chapter 5: the Concord's captain.
    log(
        "galaxy.log.concord_plate",
        LogVoice::Concord,
        "The Concord's Plate",
        "HOLD THE ROAD. WAIT FOR THE KEY. HOLD THE ROAD. WAIT FOR THE KEY. (Repeated on the \
         plate eleven thousand times, and then, in a smaller hand, once: Where is everyone?)",
    ),
    log(
        "galaxy.log.pilots",
        LogVoice::Archivist,
        "Pilots Without Orders",
        "The Concord were the Harbour's pilots: good machines with one order and nobody left \
         to give them another. That is a sad thing to be. Do not hate them for shooting at \
         you. They have been very lonely, and they are bad at it.",
    ),
    // Chapter 6: the singing spires.
    log(
        "galaxy.log.spire_mother",
        LogVoice::Spire,
        "A Mother's Voice",
        "Look up, little one. When the lamp is lit and the cable hums, look up. We are going \
         to find a key, and then we are coming back for you. Be good for the fathers. Eat what \
         they give you. Look up.",
    ),
    log(
        "galaxy.log.wren_look_up",
        LogVoice::Archivist,
        "Look Up",
        "Amos told me his daughter said look up when he went down the cable. I did not tell \
         him that it was the oldest thing your people say, older than your town. I wanted him \
         to keep it as his.",
    ),
    // Chapter 7: the Fathers' load.
    log(
        "galaxy.log.nursery",
        LogVoice::Builder,
        "The Nursery Record",
        "Eleventh world, left sleeping. Aboard: the children of the Load, four hundred and \
         twelve. Staying with them: the fathers, eighty, by choice. A lamp is lit at the \
         heart and a cable runs to the dock. When the key is found, we come home.",
    ),
    log(
        "galaxy.log.archivist_sorry",
        LogVoice::Archivist,
        "One of Ours",
        "I told you the mark on the Fathers' sled meant one of ours. I should have said one of \
         our children. I was afraid that if you knew, you would think the road was ours to \
         walk and not yours. It is yours. It was always yours. I am sorry I waited.",
    ),
    log(
        "galaxy.log.sled",
        LogVoice::Ines,
        "Margin: The Sled",
        "Bran painted THE FATHERS' LOAD on the sled tonight and the archivist went very still \
         and asked her where she learned those words. She said she made them up. It said: \
         yes, that is how it always starts.",
    ),
    // Chapter 8: the gathering light.
    log(
        "galaxy.log.lamp_key",
        LogVoice::Builder,
        "The Lamp-key",
        "The lamp-key wakes a sleeping Harbour. Give it only to one who carries a drive from \
         a world that woke by itself. Such a one has come a long way, and will not have come \
         to steal.",
    ),
    log(
        "galaxy.log.wren_letter",
        LogVoice::Wren,
        "A Letter from the Porch",
        "The archivist reads me your reports over the cable now. I am too old to understand \
         the half of it. My father went down to find out what hums; you went up and found out \
         who. Come home when you are done. Bring them if they want to come.",
    ),
    // Chapter 9: the long dark.
    log(
        "galaxy.log.harbour_books",
        LogVoice::Builder,
        "The Harbour's Books",
        "Ten world-ships moored. Drives banked to wait for the eleventh. Sleepers in the long \
         cold until the key comes. The Warden keeps watch. If you are reading this and you are \
         not one of us: please go away. If you are: welcome home.",
    ),
    log(
        "galaxy.log.ines_backwards",
        LogVoice::Ines,
        "Margin: Backwards",
        "Read them backwards to find home, I wrote. I had it the wrong way round. The roads \
         run from their home to ours. They were always coming back. We just never sent anyone \
         to meet them.",
    ),
    // Chapter 10: the Harbour.
    log(
        "galaxy.log.warden",
        LogVoice::Archivist,
        "The Warden",
        "The Warden is not cruel. It is a door that has been told to stay shut, and it is very \
         good at being a door. Do not be angry with it. When it sees the key it will be the \
         happiest door that ever was.",
    ),
    log(
        "galaxy.log.road_walked",
        LogVoice::Archivist,
        "The Road Walked",
        "The road is walked. I have been an archive for a very long time, and this is the \
         first page I have wanted to read twice. Go on, Ines would say. So: go on. There are \
         more stars than roads.",
    ),
];

/// The builders' fragments: lines of script found by exploring (procedural finds), each a
/// sliver of the builders' life on the road.
pub static FRAGMENTS: [&str; 16] = [
    "A tally scratched into a hull plate: days on the road, then a line, then the word for \
     \"tired\", then more days.",
    "A recipe for something with nine kinds of moss. The builders were, it turns out, very \
     bad cooks.",
    "A child's drawing of a world with a lamp in it and a long line going up. Under it: ours.",
    "A rule from a pilot's school: never pass a waymark without touching it. It is lucky.",
    "A list of names of stars, each crossed out once it was passed, the last few not crossed.",
    "A builder joke, translated by the archivist with apologies. It is about a Warden and a \
     door. The archivist says it is funnier in the original.",
    "Instructions for keeping a lamp lit in the cold: talk to it. It does not help the lamp. \
     It helps.",
    "A complaint to the Harbourmaster about the noise of the Concord's patrols. Signed by \
     several hundred sleepers who were, at the time, still awake.",
    "A map of a world-ship's corridors with one marked: the long way round, for thinking.",
    "A lullaby with the words worn off. The tune survives in the metal's grain.",
    "A promise written over and over in a beginner's hand: we will come back with the key.",
    "A star chart corrected in three colours, by three builders who could not agree.",
    "A note on a cargo crate: handle like the Load. Inside: seeds, and a single worn shoe.",
    "The builders' word for \"home\", which is also their word for \"the people you are \
     travelling with\".",
    "A tide table for a sea on a world nobody here has ever seen.",
    "A pilot's last log: drive banked, all quiet, going to sleep now. Wake me for the key.",
];

/// What sets the archivist talking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemarkTrigger {
    /// A chapter began (its index).
    ChapterBegun(u8),
    /// A relic of a chapter was found (its index).
    RelicFound(u8),
    /// A procedural lead was picked up.
    LeadFound,
    /// A procedural lead was finished.
    LeadDone,
    /// Asked, with nothing new to say: the road so far.
    Asked,
    /// After the finale.
    AfterTheRoad,
}

/// One remark of the archivist on space.
#[derive(Clone, Copy, Debug)]
pub struct Remark {
    /// Stable id (`galaxy.archivist.name`).
    pub id: &'static str,
    pub trigger: RemarkTrigger,
    /// The remark waits until this many chapters are done.
    pub after_chapters: u8,
    pub line: &'static str,
}

const fn remark(
    id: &'static str,
    trigger: RemarkTrigger,
    after_chapters: u8,
    line: &'static str,
) -> Remark {
    Remark {
        id,
        trigger,
        after_chapters,
        line,
    }
}

/// Every remark, append-only.
pub static REMARKS: [Remark; 30] = [
    remark(
        "galaxy.archivist.begin",
        RemarkTrigger::ChapterBegun(0),
        0,
        "A road, a drive and a crew. That is everything the builders ever had, and they \
         crossed the sky with it.",
    ),
    remark(
        "galaxy.archivist.species",
        RemarkTrigger::ChapterBegun(1),
        1,
        "Be polite. We were not always polite. We were in a hurry.",
    ),
    remark(
        "galaxy.archivist.foundry",
        RemarkTrigger::ChapterBegun(2),
        2,
        "Foundries are loud when they wake. Wardens are louder. Keep your shields up.",
    ),
    remark(
        "galaxy.archivist.quiet",
        RemarkTrigger::ChapterBegun(3),
        3,
        "Quiet worlds keep their secrets well. Dig gently.",
    ),
    remark(
        "galaxy.archivist.arbiter",
        RemarkTrigger::ChapterBegun(4),
        4,
        "The Arbiter is the oldest pilot of the Concord. It will not listen. It has not \
         listened to anything since your Moon was young.",
    ),
    remark(
        "galaxy.archivist.spires",
        RemarkTrigger::ChapterBegun(5),
        5,
        "If the spire sings, let it finish. It has been waiting a long time to be heard.",
    ),
    remark(
        "galaxy.archivist.nursery",
        RemarkTrigger::ChapterBegun(6),
        6,
        "I would like to say something here. I will say it after. Please go.",
    ),
    remark(
        "galaxy.archivist.lamp",
        RemarkTrigger::ChapterBegun(7),
        7,
        "A lamp-key is only a lamp that knows where it is going. So are you, now.",
    ),
    remark(
        "galaxy.archivist.long_dark",
        RemarkTrigger::ChapterBegun(8),
        8,
        "Fill the tanks. Fill the bay with air. Then fill it again. The long dark is long.",
    ),
    remark(
        "galaxy.archivist.harbour",
        RemarkTrigger::ChapterBegun(9),
        9,
        "Ten ships, and every sleeper in them dreaming of an eleventh. Let us not keep them \
         waiting.",
    ),
    remark(
        "galaxy.archivist.waymark",
        RemarkTrigger::RelicFound(0),
        0,
        "A waymark! Touch it. It is lucky. That is not engineering, it is tradition.",
    ),
    remark(
        "galaxy.archivist.door",
        RemarkTrigger::RelicFound(2),
        2,
        "The door is reading you. It will take a while. Doors are thorough.",
    ),
    remark(
        "galaxy.archivist.archive",
        RemarkTrigger::RelicFound(3),
        3,
        "An archive smaller than me. How cosy. Let me read it to you.",
    ),
    remark(
        "galaxy.archivist.spire_found",
        RemarkTrigger::RelicFound(5),
        5,
        "I know that voice. I do not know from where.",
    ),
    remark(
        "galaxy.archivist.nursery_found",
        RemarkTrigger::RelicFound(6),
        6,
        "There are your names. The first ones. Read them slowly.",
    ),
    remark(
        "galaxy.archivist.key_found",
        RemarkTrigger::RelicFound(7),
        7,
        "Hold it up. Let them see. Let everything see.",
    ),
    remark(
        "galaxy.archivist.lead_found_a",
        RemarkTrigger::LeadFound,
        0,
        "Something out here wants finding. Most things do.",
    ),
    remark(
        "galaxy.archivist.lead_found_b",
        RemarkTrigger::LeadFound,
        1,
        "Another voice on the scanner. The sky is not as empty as your people thought.",
    ),
    remark(
        "galaxy.archivist.lead_found_c",
        RemarkTrigger::LeadFound,
        4,
        "I have added it to the discovery log. I like the discovery log. It is a kind of \
         archive that grows.",
    ),
    remark(
        "galaxy.archivist.lead_done_a",
        RemarkTrigger::LeadDone,
        0,
        "Done. Somebody out here will remember that you came.",
    ),
    remark(
        "galaxy.archivist.lead_done_b",
        RemarkTrigger::LeadDone,
        2,
        "That is the way the builders travelled too: one small kindness per star.",
    ),
    remark(
        "galaxy.archivist.lead_done_c",
        RemarkTrigger::LeadDone,
        6,
        "Every star you help is a waymark of your own. Somebody will follow them one day.",
    ),
    remark(
        "galaxy.archivist.asked_a",
        RemarkTrigger::Asked,
        0,
        "The charts are roads, not maps. The road starts at the Archive's first chart. The \
         journal keeps where it goes next.",
    ),
    remark(
        "galaxy.archivist.asked_b",
        RemarkTrigger::Asked,
        3,
        "Eleven worlds hollowed, ten that flew. I keep counting them. The number does not \
         change.",
    ),
    remark(
        "galaxy.archivist.asked_c",
        RemarkTrigger::Asked,
        7,
        "You are the Load. That is not a burden. It means you were the thing worth carrying.",
    ),
    remark(
        "galaxy.archivist.asked_d",
        RemarkTrigger::Asked,
        9,
        "The Warden is a good door. Be kind to it after.",
    ),
    remark(
        "galaxy.archivist.after_a",
        RemarkTrigger::AfterTheRoad,
        10,
        "The road is walked. The sky is not. Shall we see what else is out here?",
    ),
    remark(
        "galaxy.archivist.after_b",
        RemarkTrigger::AfterTheRoad,
        10,
        "Every system has something to say. I would like to hear all of it. We have time now.",
    ),
    remark(
        "galaxy.archivist.after_c",
        RemarkTrigger::AfterTheRoad,
        10,
        "Wren asked for a postcard from every star. I have started a list. It is long.",
    ),
    remark(
        "galaxy.archivist.after_d",
        RemarkTrigger::AfterTheRoad,
        10,
        "Go on, Ines would say. She always said it as if it were the easiest thing in the \
         world. It is not. You are doing it anyway.",
    ),
];

/// The remark for `trigger` with `chapters_done` chapters finished, on the `turn`-th remark
/// (the open lines of a trigger take turns). `Asked` falls back to the road's state.
#[must_use]
pub fn remark_for(trigger: RemarkTrigger, chapters_done: u8, turn: u32) -> Option<u8> {
    let open: Vec<u8> = REMARKS
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trigger == trigger && line.after_chapters <= chapters_done)
        .filter_map(|(index, _)| u8::try_from(index).ok())
        .collect();
    let count = u32::try_from(open.len()).ok().filter(|count| *count > 0)?;
    // Newest-opened first, then the older ones in turn.
    let back = usize::try_from(turn % count).ok()?;
    open.get(open.len() - 1 - back).copied()
}
