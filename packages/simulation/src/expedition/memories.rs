//! What people in the deep settlements remember of the Fathers' expedition.
//!
//! Plain data with **stable ids** for the settlement tracks and the integrator to wire into
//! their people's talk (Gravel Hollow in the Grey Stone, the Descent II camp, the Hive). A line
//! can wait for a log ([`SettlementMemory::after_log`]): people open up about what the crew has
//! already read, so the town's memories follow the trail instead of spoiling it. Ids are never
//! renamed or reused; new lines append.
//!
//! # Speakers (Wave 9)
//!
//! An id's middle part is its **speaker key** ([`SettlementMemory::speaker_key`]): the person
//! who says it. A settlement with resident people casts one resident per key, and that person
//! tells their own lines in their own voice ([`line_by_for_visit`]); anyone else there passes a
//! line on with its speaker named ([`SettlementMemory::told`]). Cathedral Station and the
//! Amber Court have their residents ([`crate::people::deep_folk`]).
//!
//! # Registering a settlement (Wave 9 seam)
//!
//! Merrow City (Descent IV), the jungle tribes' village (Descent V, the Sunfolk Kraal) and the
//! Prism Relay (Descent VI, Parallax Station) are built by their Descent tracks and registered
//! by their real names (Wave 9 integration); their generated people tell these lines. A settlement
//! whose population's name is one of [`Settlement::place_names`] remembers the Fathers
//! ([`Settlement::of_place_name`]); its people speak the lines whose speaker key
//! [`Settlement::speaker_for_role`] gives their trade. Wiring a new place takes one name in
//! `place_names` (or the place's real name there) and, if its trades differ, a row in
//! `speaker_for_role`.

use super::ExpeditionState;
use crate::people::Role;

/// A deep settlement that remembers the expedition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Settlement {
    /// The shallow village in the Grey Stone (Descent I).
    GravelHollow,
    /// The prospectors' camp at the Descent II outpost.
    OldDeepCamp,
    /// The insect city in the Living Dark (Descent III).
    TheHive,
    // --- Wave 9 (appended). -----------------------------------------------------------------
    /// The merrow's city in its air bubble (Descent IV), where Sefa stayed.
    MerrowCity,
    /// The jungle tribes' village under the inner sun (Descent V), where Bran stayed.
    JungleTribes,
    /// The Descent VI outpost on the edge of the Prism Desert.
    PrismRelay,
    // --- Wave 10 ancients-a (appended). -----------------------------------------------------
    /// The Archive Post, Descent VII's outpost in the Buried Megacity: the Lamplighters and the
    /// builders' patient machines.
    ArchivePost,
    /// The Descent VIII outpost, Heart's Edge, the last camp above the Heart (Wave 10 story).
    HeartCamp,
}

impl Settlement {
    pub const ALL: [Self; 8] = [
        Self::GravelHollow,
        Self::OldDeepCamp,
        Self::TheHive,
        Self::MerrowCity,
        Self::JungleTribes,
        Self::PrismRelay,
        Self::ArchivePost,
        Self::HeartCamp,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GravelHollow => "Gravel Hollow",
            Self::OldDeepCamp => "the Old Deep camp",
            Self::TheHive => "the Hive",
            Self::MerrowCity => "Merrow City",
            Self::JungleTribes => "the jungle tribes",
            Self::PrismRelay => "the Prism Relay",
            Self::ArchivePost => "the Archive Post",
            Self::HeartCamp => "the Heart camp",
        }
    }

    /// The prefix of the settlement's memory ids.
    #[must_use]
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::GravelHollow => "gravel_hollow.",
            Self::OldDeepCamp => "old_deep_camp.",
            Self::TheHive => "hive.",
            Self::MerrowCity => "merrow_city.",
            Self::JungleTribes => "jungle_tribes.",
            Self::PrismRelay => "prism_relay.",
            Self::ArchivePost => "archive_post.",
            Self::HeartCamp => "heart_camp.",
        }
    }

    /// The names a settlement's population may go by (the registration seam, see the module
    /// docs). The Descent tracks' settlements answer to their plan names.
    #[must_use]
    pub const fn place_names(self) -> &'static [&'static str] {
        match self {
            Self::GravelHollow => &[crate::crust::GRAVEL_HOLLOW_NAME],
            Self::OldDeepCamp => &["Cathedral Station"],
            Self::TheHive => &["The Amber Court", "the Amber Court"],
            Self::MerrowCity => &[crate::drowned::MERROW_CITY_NAME, "Merrow Air Dock"],
            // The Hollow World's Sunfolk village (Wave 9 integration: its real name).
            Self::JungleTribes => &[
                crate::hollow::SUNFOLK_NAME,
                "the Jungle Tribes",
                "The Jungle Tribes",
                "Sunward Camp",
            ],
            // Parallax Station, the Descent VI outpost's researchers (Wave 9 integration).
            Self::PrismRelay => &[
                crate::distortion::PARALLAX_STATION_NAME,
                "Prism Relay",
                "the Prism Relay",
            ],
            // The Descent VII outpost's Lamplighters (Wave 10 ancients-a).
            Self::ArchivePost => ANCIENTS_OUTPOST_NAMES,
            // The Descent VIII outpost (Wave 10 story): Heart's Edge, by its logistics name.
            Self::HeartCamp => HEART_CAMP_NAMES,
        }
    }

    /// The settlement a population named `name` is, if it remembers the Fathers.
    #[must_use]
    pub fn of_place_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|settlement| settlement.place_names().contains(&name))
    }

    /// The speaker key a resident of this settlement working `role` speaks for, if any.
    ///
    /// The residents cast by [`crate::people::deep_folk`] carry their key themselves; this is
    /// the fallback for settlements whose people are generated (the Descent tracks' towns).
    #[must_use]
    pub const fn speaker_for_role(self, role: Role) -> Option<&'static str> {
        match (self, role) {
            (Self::GravelHollow, Role::Smith) => Some("smith"),
            (Self::GravelHollow, Role::Innkeeper) => Some("innkeeper"),
            (Self::GravelHollow | Self::MerrowCity | Self::JungleTribes, Role::Apprentice) => {
                Some("child")
            }
            (Self::GravelHollow, Role::Prospector | Role::RetiredMiner) => Some("miner"),
            (Self::OldDeepCamp, Role::Quartermaster) => Some("quartermaster"),
            (Self::OldDeepCamp, Role::Prospector) => Some("prospector"),
            (Self::OldDeepCamp, Role::Healer) => Some("medic"),
            (Self::OldDeepCamp, Role::Cartographer) => Some("surveyor"),
            (Self::OldDeepCamp, Role::Innkeeper | Role::Baker) => Some("cook"),
            (Self::TheHive, Role::Storyteller) => Some("elder"),
            (Self::TheHive, Role::Mechanic) => Some("pump_tender"),
            (Self::TheHive, Role::Healer) => Some("nurse"),
            (Self::TheHive, Role::Warden) => Some("soldier"),
            (Self::TheHive, Role::ShrineKeeper) => Some("queen"),
            (Self::MerrowCity, Role::Scholar | Role::Storyteller) => Some("archivist"),
            (Self::MerrowCity, Role::ShrineKeeper | Role::Brewer) => Some("singer"),
            (Self::MerrowCity, Role::Quartermaster | Role::Trader) => Some("dockkeeper"),
            (Self::JungleTribes, Role::Warden | Role::Hermit) => Some("chief"),
            (Self::JungleTribes, Role::Healer) => Some("healer"),
            (Self::JungleTribes, Role::Farmer | Role::Prospector) => Some("hunter"),
            (Self::PrismRelay, Role::Quartermaster | Role::Mechanic)
            | (Self::ArchivePost, Role::Quartermaster | Role::Trader)
            | (Self::HeartCamp, Role::Quartermaster | Role::Mechanic | Role::Trader) => {
                Some("keeper")
            }
            (Self::PrismRelay, Role::Tinkerer | Role::Smith) => Some("glass_cutter"),
            (
                Self::PrismRelay | Self::HeartCamp,
                Role::Courier | Role::Prospector | Role::Hermit,
            ) => Some("drifter"),
            (Self::ArchivePost, Role::Storyteller | Role::RetiredMiner) => Some("lamplighter"),
            (Self::ArchivePost, Role::Mechanic | Role::Tinkerer | Role::Smith) => Some("tinker"),
            (Self::ArchivePost, Role::Courier | Role::Apprentice) => Some("sweeper"),
            (Self::ArchivePost, Role::Scholar | Role::Cartographer) => Some("curator"),
            _ => None,
        }
    }
}

/// The names the Descent VII outpost's population goes by (Wave 10: the ancients track's
/// Archive Post in the Buried Megacity, and its logistics outpost's name).
pub const ANCIENTS_OUTPOST_NAMES: &[&str] = &[
    "The Archive Post",
    "the Archive Post",
    "Megacity Archive Post",
];

/// The names the Descent VIII outpost's population may go by (Wave 10 registration seam).
pub const HEART_CAMP_NAMES: &[&str] = &[
    "Heart's Edge",
    "Heartfall Camp",
    "the Heart Camp",
    "The Heart Camp",
    "Heart Outpost",
];

/// One remembered line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementMemory {
    /// Stable id (`settlement.speaker.topic`): what a settlement's people key their talk on.
    pub id: &'static str,
    pub settlement: Settlement,
    /// Who says it (a role the settlement track casts).
    pub speaker: &'static str,
    /// The line waits until the crew has read this log (1-based), if set.
    pub after_log: Option<u8>,
    pub line: &'static str,
}

impl SettlementMemory {
    /// Whether a crew with `state` has read far enough to hear it.
    #[must_use]
    pub fn open(&self, state: &ExpeditionState) -> bool {
        self.after_log.is_none_or(|log| state.found >= log)
    }

    /// The speaker key: the middle part of the id (`hive.pump_tender.wrench_one` ->
    /// `pump_tender`).
    #[must_use]
    pub fn speaker_key(&self) -> &'static str {
        self.id.split('.').nth(1).unwrap_or("")
    }
}

/// Every remembered line.
pub const MEMORIES: [SettlementMemory; 66] = [
    SettlementMemory {
        id: "gravel_hollow.smith.rails",
        settlement: Settlement::GravelHollow,
        speaker: "Old Tully, the smith",
        after_log: Some(5),
        line: "The Fathers? Slept in my loft, all six of them. Amos asked about the rails and I \
               told him what my granddad told me: they were here before the Hollow was. He just \
               nodded, like that was what he'd come all this way to hear.",
    },
    SettlementMemory {
        id: "gravel_hollow.smith.lucky_stone",
        settlement: Settlement::GravelHollow,
        speaker: "Old Tully, the smith",
        after_log: Some(6),
        line: "Young Teo bought a lucky stone off my grandson. A whole credit, for a rock. If \
               you find him down there, tell him it worked. He got further than anyone.",
    },
    SettlementMemory {
        id: "gravel_hollow.innkeeper.stew",
        settlement: Settlement::GravelHollow,
        speaker: "the innkeeper",
        after_log: None,
        line: "Ruth Brannigan cooked a stew in my kitchen that folk still ask after. Wouldn't \
               give me the recipe. Said she'd send it up the line from the bottom.",
    },
    SettlementMemory {
        id: "gravel_hollow.child.lights",
        settlement: Settlement::GravelHollow,
        speaker: "a child",
        after_log: Some(1),
        line: "Gran says the Fathers' lamps went down the cable one by one, and the last one \
               blinked twice. Like goodbye. Or like: come on.",
    },
    SettlementMemory {
        id: "gravel_hollow.miner.hum",
        settlement: Settlement::GravelHollow,
        speaker: "a miner",
        after_log: None,
        line: "The hum went quiet the year they went down. Then it came back louder. Some say \
               they woke something. Some say they're still digging.",
    },
    SettlementMemory {
        id: "old_deep_camp.quartermaster.splice",
        settlement: Settlement::OldDeepCamp,
        speaker: "the quartermaster",
        after_log: Some(20),
        line: "There's a splice in the old drop line just above the storm zone. Halloran's \
               work, you can tell: neat, double wrapped. The cut below it isn't neat at all.",
    },
    SettlementMemory {
        id: "old_deep_camp.prospector.valve_key",
        settlement: Settlement::OldDeepCamp,
        speaker: "a prospector",
        after_log: Some(15),
        line: "Somebody turned one of the Cathedral's valves long before us, and left the key in \
               a cairn. Saved us a week and probably a leg.",
    },
    SettlementMemory {
        id: "old_deep_camp.medic.iodine",
        settlement: Settlement::OldDeepCamp,
        speaker: "the medic",
        after_log: None,
        line: "Found a tin of iodine in a lead tent down in the glow. The label says \
               \"R. Brannigan. ONE A DAY, DOV. I MEAN IT.\" I keep it on my shelf for luck.",
    },
    SettlementMemory {
        id: "old_deep_camp.surveyor.marks",
        settlement: Settlement::OldDeepCamp,
        speaker: "the surveyor",
        after_log: Some(18),
        line: "Calloway's survey marks are still the best down here. Plumb to the cable, every \
               one. We copy her numbers and add our names underneath.",
    },
    SettlementMemory {
        id: "old_deep_camp.cook.trespass",
        settlement: Settlement::OldDeepCamp,
        speaker: "the cook",
        after_log: Some(21),
        line: "The stories said the Star Core was the bottom. The Fathers went past it. We go \
               past it every week now, and every week it still feels like trespassing.",
    },
    SettlementMemory {
        id: "hive.elder.soft_shells",
        settlement: Settlement::TheHive,
        speaker: "a worker elder",
        after_log: None,
        line: "Click-hum. Soft-shells came before you, carrying small lights. Like the \
               Lamp-Bearers, but little and afraid. We liked them.",
    },
    SettlementMemory {
        id: "hive.pump_tender.wrench_one",
        settlement: Settlement::TheHive,
        speaker: "a pump tender",
        after_log: Some(27),
        line: "The Wrench-One kept the warm pumps singing for many broods. He taught the young \
               his name, Teo, and they said it wrong to make him laugh. His tools still hang in \
               the nursery. We polish them.",
    },
    SettlementMemory {
        id: "hive.nurse.humming",
        settlement: Settlement::TheHive,
        speaker: "a nurse",
        after_log: Some(26),
        line: "The singing soft-shell hummed to the brood when she was frightened. The brood \
               still hums it. Listen.",
    },
    SettlementMemory {
        id: "hive.soldier.warning",
        settlement: Settlement::TheHive,
        speaker: "a soldier",
        after_log: None,
        line: "Soft-shell. The last of your kind who went down did not come back up. Turn \
               around. (It does not move aside.)",
    },
    SettlementMemory {
        id: "hive.queen.engine",
        settlement: Settlement::TheHive,
        speaker: "the queen's voice",
        after_log: Some(30),
        line: "The light below is not a stone. It sleeps, and it dreams of the sky. The \
               Lamp-Bearers will come back to wake it. Or you will.",
    },
    // --- Wave 9: Cathedral Station and the Hive hear of the deeper Descents. ----------------
    SettlementMemory {
        id: "old_deep_camp.quartermaster.drums",
        settlement: Settlement::OldDeepCamp,
        speaker: "the quartermaster",
        after_log: Some(31),
        line: "A sea, they say, down past the Living Dark, and the Fathers rafted it. I sold \
               Halloran those fuel drums, you know. Thought he wanted them for fuel.",
    },
    SettlementMemory {
        id: "old_deep_camp.medic.fingers",
        settlement: Settlement::OldDeepCamp,
        speaker: "the medic",
        after_log: Some(33),
        line: "Brannigan's notes say she took two of Halloran's fingers in the ice and he joked \
               all the way through it. That's a good patient. The worst kind, and the best.",
    },
    SettlementMemory {
        id: "old_deep_camp.surveyor.hollow",
        settlement: Settlement::OldDeepCamp,
        speaker: "the surveyor",
        after_log: Some(41),
        line: "Calloway measured the inside of the world twice and wrote: hollow. I believed \
               her numbers before I'd believe my own eyes. Still do.",
    },
    SettlementMemory {
        id: "old_deep_camp.cook.grave",
        settlement: Settlement::OldDeepCamp,
        speaker: "the cook",
        after_log: Some(48),
        line: "If you pass Halloran's grave, leave him a song. Any song. He hated a quiet room \
               and he sang flat, so you can't do worse.",
    },
    SettlementMemory {
        id: "old_deep_camp.prospector.twin",
        settlement: Settlement::OldDeepCamp,
        speaker: "a prospector",
        after_log: Some(51),
        line: "Fellow came up the lift swearing he'd met himself down there. We laughed. Then \
               he showed me his twin's tally marks, in his own hand, counting backwards.",
    },
    SettlementMemory {
        id: "hive.elder.wet_singers",
        settlement: Settlement::TheHive,
        speaker: "a worker elder",
        after_log: Some(36),
        line: "Click. The Wet-Singers under the water remember the Lamp-Bearers too. They say \
               Tall Ones. Tall. Yes. Taller than the comb, and gentle with it.",
    },
    SettlementMemory {
        id: "hive.pump_tender.still_singing",
        settlement: Settlement::TheHive,
        speaker: "a pump tender",
        after_log: Some(40),
        line: "Wrench-One said: if my soft-shells come after me, show them the pumps still \
               sing. Listen. They sing. Tell his friends, when you find them.",
    },
    SettlementMemory {
        id: "hive.nurse.mother_twice",
        settlement: Settlement::TheHive,
        speaker: "a nurse",
        after_log: Some(45),
        line: "The humming soft-shell stayed with the giants' people? Hm-hm. Then she is a \
               mother twice. The brood is glad. They hum her song louder today.",
    },
    SettlementMemory {
        id: "hive.queen.warm_brood",
        settlement: Settlement::TheHive,
        speaker: "the queen's voice",
        after_log: Some(49),
        line: "The Lamp-Bearers hung a little sun inside the world so their sleep would be \
               warm. We felt it once, long ago: the walls were warm for a whole brood.",
    },
    // --- Wave 9: Merrow City (Descent IV), where Sefa stayed. --------------------------------
    SettlementMemory {
        id: "merrow_city.dockkeeper.raft",
        settlement: Settlement::MerrowCity,
        speaker: "the dock keeper",
        after_log: Some(31),
        line: "Your Fathers came to the bubble on a raft of drums. We thought it was a whale \
               that had swallowed a stove.",
    },
    SettlementMemory {
        id: "merrow_city.singer.lullaby",
        settlement: Settlement::MerrowCity,
        speaker: "a singer",
        after_log: None,
        line: "We sing the Tall Ones' song to make the little ones sleep. A dry-air woman \
               taught us a new verse, about a town far above that hums. We sing that too.",
    },
    SettlementMemory {
        id: "merrow_city.archivist.sefa",
        settlement: Settlement::MerrowCity,
        speaker: "the archivist",
        after_log: Some(36),
        line: "Sefa-of-the-dry-air sat in our archive for nine of your years. She wrote our \
               songs in a book with lines in it, and cried at all the right parts.",
    },
    SettlementMemory {
        id: "merrow_city.child.hat",
        settlement: Settlement::MerrowCity,
        speaker: "a child",
        after_log: Some(37),
        line: "The big one, Amos, let me wear his hat. It smelled of smoke. Is it all smoke, \
               up there? Sefa said no. Sefa said: mostly rain.",
    },
    SettlementMemory {
        id: "merrow_city.archivist.thread",
        settlement: Settlement::MerrowCity,
        speaker: "the archivist",
        after_log: Some(38),
        line: "The oldest song says the Tall Ones came from the stars on a thread and tied it \
               to the fire at the world's heart. Sefa said the thread is your cable. She was \
               right about most things.",
    },
    SettlementMemory {
        id: "merrow_city.archivist.flat_song",
        settlement: Settlement::MerrowCity,
        speaker: "the archivist",
        after_log: Some(48),
        line: "Sefa woke one night and said the radio man was gone. She sang his town song, \
               very flat, the way he did. We all sang it flat with her.",
    },
    // --- Wave 9: the jungle tribes (Descent V), where Bran stayed. ---------------------------
    SettlementMemory {
        id: "jungle_tribes.child.cheese",
        settlement: Settlement::JungleTribes,
        speaker: "a child",
        after_log: None,
        line: "Grandmother Bran taught us a song about cheese. We don't know what cheese is. \
               It's a very good song.",
    },
    SettlementMemory {
        id: "jungle_tribes.chief.fever",
        settlement: Settlement::JungleTribes,
        speaker: "the chief",
        after_log: Some(43),
        line: "I burned with fever as a girl and the sky-woman Bran sat with me three nights. \
               I am chief now. She named me, after: Ruth-Little.",
    },
    SettlementMemory {
        id: "jungle_tribes.hunter.sled_board",
        settlement: Settlement::JungleTribes,
        speaker: "a hunter",
        after_log: Some(44),
        line: "We lent your Fathers our gentlest grazer. It came home alone with a painted \
               board on its back: THE FATHERS' LOAD. The board hangs in the long-house.",
    },
    SettlementMemory {
        id: "jungle_tribes.healer.soup",
        settlement: Settlement::JungleTribes,
        speaker: "the healer",
        after_log: Some(45),
        line: "I was Bran's hands when hers shook, at the end. Willow bark, cold cloth and \
               soup. Soup is half of medicine, she said, and nagging is the other half.",
    },
    SettlementMemory {
        id: "jungle_tribes.chief.slow_hum",
        settlement: Settlement::JungleTribes,
        speaker: "the chief",
        after_log: Some(47),
        line: "The radio man knew he was dying. He told my father: when the hum goes slower, \
               don't be afraid. It isn't me. It's the big heart, resting.",
    },
    // --- Wave 9: the Prism Relay (Descent VI), the outpost on the glass desert's edge. -------
    SettlementMemory {
        id: "prism_relay.drifter.hello",
        settlement: Settlement::PrismRelay,
        speaker: "a drifter",
        after_log: None,
        line: "Sometimes the relay picks up a voice: a radio man saying hello. He's been dead \
               for years. Out here, that's just a Tuesday.",
    },
    SettlementMemory {
        id: "prism_relay.keeper.read_in_order",
        settlement: Settlement::PrismRelay,
        speaker: "the relay keeper",
        after_log: Some(52),
        line: "Twice a week someone comes in shaking because they met themselves. I tell them \
               what Calloway told the first of us: read in order. The false one keys to \
               nothing.",
    },
    SettlementMemory {
        id: "prism_relay.glass_cutter.two_clocks",
        settlement: Settlement::PrismRelay,
        speaker: "the glass cutter",
        after_log: Some(56),
        line: "Varga came through with a grey beard he swore was black that morning. Time \
               pools out there. I keep two clocks and believe neither of them.",
    },
    SettlementMemory {
        id: "prism_relay.keeper.filters",
        settlement: Settlement::PrismRelay,
        speaker: "the relay keeper",
        after_log: Some(59),
        line: "Calloway left a set of lamp filters on that hook, red, gold and blue, with a \
               note: for the doors. Best thing anyone ever left down here.",
    },
    SettlementMemory {
        id: "prism_relay.drifter.streets",
        settlement: Settlement::PrismRelay,
        speaker: "a drifter",
        after_log: Some(60),
        line: "Past the glass the walls turn into streets. Varga and Calloway went down there, \
               and the relay's needle has pointed after them ever since.",
    },
    // --- Wave 10 ancients-a: the Archive Post (Descent VII), the Lamplighters' hall. --------
    SettlementMemory {
        id: "archive_post.keeper.register",
        settlement: Settlement::ArchivePost,
        speaker: "Keeper-Unit Nine",
        after_log: None,
        line: "Nine visitors in four hundred years, and six of them in one week. The Fathers \
               signed my register in a hurry. I keep the page dusted.",
    },
    SettlementMemory {
        id: "archive_post.lamplighter.lamps",
        settlement: Settlement::ArchivePost,
        speaker: "Ada Lantry, the lamplighter",
        after_log: None,
        line: "My grandmother came down after a crew that never went home. She found the street \
               lamps still burning and could not bear to let them go out. Neither can I.",
    },
    SettlementMemory {
        id: "archive_post.tinker.runner",
        settlement: Settlement::ArchivePost,
        speaker: "Pell Corran, the tinker",
        after_log: None,
        line: "Varga's sled came in on a cracked runner. I welded it on that bench there. He \
               paid me in chocolate. I still have the wrapper.",
    },
    SettlementMemory {
        id: "archive_post.sweeper.boots",
        settlement: Settlement::ArchivePost,
        speaker: "Sweep",
        after_log: None,
        line: "Sweep-sweep. Muddy boots. Six pairs. The tall one's were muddiest. Sweep was very \
               busy that week. Sweep did not mind.",
    },
    SettlementMemory {
        id: "archive_post.curator.lifetime",
        settlement: Settlement::ArchivePost,
        speaker: "Curator Thessaly",
        after_log: None,
        line: "The builders charted every star they passed. Calloway asked to see them all. I \
               said it would take a lifetime. She said: then I will stay a lifetime.",
    },
    SettlementMemory {
        id: "archive_post.keeper.late",
        settlement: Settlement::ArchivePost,
        speaker: "Keeper-Unit Nine",
        after_log: Some(55),
        line: "The archivist will say you are late. It said the same to them. It means it \
               kindly. It has been waiting a very long time.",
    },
    SettlementMemory {
        id: "archive_post.lamplighter.song",
        settlement: Settlement::ArchivePost,
        speaker: "Ada Lantry, the lamplighter",
        after_log: Some(58),
        line: "Brannigan's song came down ahead of them, somehow. A load and six fools. We sing \
               it when we light the far streets, so the dark knows someone's coming.",
    },
    SettlementMemory {
        id: "archive_post.curator.soup",
        settlement: Settlement::ArchivePost,
        speaker: "Curator Thessaly",
        after_log: Some(60),
        line: "Calloway went down to the Archive and did not come back for supper. I sent Sweep \
               with soup. Sweep came back with an empty bowl and a note: thank you, still \
               reading.",
    },
    SettlementMemory {
        id: "archive_post.tinker.pipe",
        settlement: Settlement::ArchivePost,
        speaker: "Pell Corran, the tinker",
        after_log: Some(60),
        line: "If you go through the Engine Halls, count the beat. Folk who don't come back \
               with a leg set in a length of my pipe. Ask Varga.",
    },
    SettlementMemory {
        id: "archive_post.keeper.stars",
        settlement: Settlement::ArchivePost,
        speaker: "Keeper-Unit Nine",
        after_log: Some(60),
        line: "Varga asked me which way was home. I showed him up. He laughed and said: no, \
               the other home. I showed him up again, further.",
    },
    // --- Wave 10: the older settlements hear of the city, the Archive and the Heart. --------
    SettlementMemory {
        id: "old_deep_camp.surveyor.city",
        settlement: Settlement::OldDeepCamp,
        speaker: "the surveyor",
        after_log: Some(61),
        line: "A city. Calloway's numbers say a whole city lies under the thin places, plumb to \
               the cable. I'd give my good knee to see her survey of it.",
    },
    SettlementMemory {
        id: "hive.queen.waking",
        settlement: Settlement::TheHive,
        speaker: "the queen's voice",
        after_log: Some(80),
        line: "The light below turns in its sleep. Soon. Soft-shell, when you wake it, the comb \
               will sing all the way up, and the Wrench-One's pumps with it.",
    },
    SettlementMemory {
        id: "merrow_city.archivist.charts",
        settlement: Settlement::MerrowCity,
        speaker: "the archivist",
        after_log: Some(66),
        line: "Sefa always said the Tall Ones' song had a second verse, about roads between the \
               stars. The dry-air woman in the builders' archive found the map of them, they say.",
    },
    SettlementMemory {
        id: "jungle_tribes.chief.look_up",
        settlement: Settlement::JungleTribes,
        speaker: "the chief",
        after_log: Some(80),
        line: "My father said the big man walked down to the heart of the world to sit with it. \
               When the cable lights, he said, everyone look up.",
    },
    SettlementMemory {
        id: "prism_relay.keeper.needle",
        settlement: Settlement::PrismRelay,
        speaker: "the relay keeper",
        after_log: Some(72),
        line: "The relay's needle swung all the way round one night and settled pointing down, \
               steady as a compass. Something down there let somebody through.",
    },
    // --- Wave 10 story: the Archive Post (Descent VII) remembers the Fathers too. -----------
    SettlementMemory {
        id: "archive_post.keeper.lamp",
        settlement: Settlement::ArchivePost,
        speaker: "the outpost keeper",
        after_log: None,
        line: "The lamp over the gate came on by itself the day I got here. The archivist says it \
               does that for everyone it expects. I asked how long it had expected me. A while.",
    },
    SettlementMemory {
        id: "archive_post.lamplighter.ticket",
        settlement: Settlement::ArchivePost,
        speaker: "a lamplighter",
        after_log: Some(61),
        line: "The trams still run, empty, on the old timetable. I ride the last one every night \
               for company. Calloway rode it once: there's a ticket in the seat in her hand.",
    },
    SettlementMemory {
        id: "archive_post.curator.parked",
        settlement: Settlement::ArchivePost,
        speaker: "the scholar",
        after_log: Some(63),
        line: "We argued for years whether the builders lived here or only worked here. Then the \
               archivist lit the plan on the wall. They didn't live here. They parked here.",
    },
    SettlementMemory {
        id: "archive_post.tinker.splint",
        settlement: Settlement::ArchivePost,
        speaker: "a salvager",
        after_log: Some(64),
        line: "Found a tram-rail splint in the Engine Halls, wound in a medic's bandage, R.B. on \
               the clasp. Somebody broke a leg down there, and somebody had cared.",
    },
    SettlementMemory {
        id: "archive_post.curator.ines",
        settlement: Settlement::ArchivePost,
        speaker: "the scholar",
        after_log: Some(67),
        line: "Ines Calloway read in the Archive until she couldn't see the pages, then had the \
               archivist read them to her. It talks like her now. Hear it say: go on.",
    },
    SettlementMemory {
        id: "archive_post.keeper.wren",
        settlement: Settlement::ArchivePost,
        speaker: "the outpost keeper",
        after_log: Some(71),
        line: "A woman came down years back asking after Amos Varga. Grey hair, porch-lamp face. \
               Wren, she said. She got to the gold wall, went home to wait, and left a note: \
               I looked up every night.",
    },
    SettlementMemory {
        id: "archive_post.sweeper.mark",
        settlement: Settlement::ArchivePost,
        speaker: "the sweeper's boy",
        after_log: Some(72),
        line: "The big guard at the gold wall lets people with the mark through. Draw a ball on \
               a string on your hand! It doesn't work. It's still fun.",
    },
    // --- Wave 10: the Heart camp (Descent VIII), the last camp above the drive. -------------
    SettlementMemory {
        id: "heart_camp.drifter.skip",
        settlement: Settlement::HeartCamp,
        speaker: "a drifter",
        after_log: None,
        line: "The hum's so loud here you stop hearing it. Then one day it skips a beat, and \
               every one of us sits up in the dark like a dog.",
    },
    SettlementMemory {
        id: "heart_camp.keeper.board",
        settlement: Settlement::HeartCamp,
        speaker: "the camp keeper",
        after_log: Some(76),
        line: "There's a sled folded flat in the first well past camp, painted letters on what's \
               left: THE FATHERS'. Nobody moves it. It feels like a grave.",
    },
    SettlementMemory {
        id: "heart_camp.keeper.letters",
        settlement: Settlement::HeartCamp,
        speaker: "the camp keeper",
        after_log: Some(78),
        line: "There are letters in the cairns down here, all in one big careful hand, all to \
               the same five names. We read them out loud on rest days.",
    },
    SettlementMemory {
        id: "heart_camp.drifter.chair",
        settlement: Settlement::HeartCamp,
        speaker: "a drifter",
        after_log: Some(79),
        line: "I went as far as the gold light once. There's a chair down there made of a sled \
               board and a crutch, by the cable root, facing the drive. I didn't sit in it.",
    },
];

/// The lines `settlement`'s people may say to a crew with `state`, in order.
pub fn lines_for(
    settlement: Settlement,
    state: &ExpeditionState,
) -> impl Iterator<Item = &'static SettlementMemory> + '_ {
    MEMORIES
        .iter()
        .filter(move |memory| memory.settlement == settlement && memory.open(state))
}

/// The lines `speaker` (a speaker key) of `settlement` may tell a crew with `state`, in the
/// order they open.
pub fn lines_by<'a>(
    settlement: Settlement,
    speaker: &'a str,
    state: &'a ExpeditionState,
) -> impl Iterator<Item = &'static SettlementMemory> + 'a {
    lines_for(settlement, state).filter(move |memory| memory.speaker_key() == speaker)
}

/// The line `speaker` of `settlement` tells a crew with `state` on its `visit`-th talk: their
/// own open lines take turns, newest first (Wave 9: residents speak for themselves).
#[must_use]
pub fn line_by_for_visit(
    settlement: Settlement,
    speaker: &str,
    state: &ExpeditionState,
    visit: u32,
) -> Option<&'static SettlementMemory> {
    let mut open: Vec<&'static SettlementMemory> = lines_by(settlement, speaker, state).collect();
    open.sort_by_key(|memory| memory.after_log.unwrap_or(0));
    let count = u32::try_from(open.len()).ok().filter(|count| *count > 0)?;
    let back = usize::try_from(visit % count).ok()?;
    open.get(open.len() - 1 - back).copied()
}

/// The line with stable id `id`.
#[must_use]
pub fn memory(id: &str) -> Option<&'static SettlementMemory> {
    MEMORIES.iter().find(|memory| memory.id == id)
}

/// The line `settlement`'s people bring up for a crew with `state` on its `visit`-th visit.
///
/// The open lines take turns, starting from the newest the crew has unlocked (Wave 8
/// integration: the settlements' people and counters speak them).
#[must_use]
pub fn line_for_visit(
    settlement: Settlement,
    state: &ExpeditionState,
    visit: u32,
) -> Option<&'static SettlementMemory> {
    let mut open: Vec<&'static SettlementMemory> = lines_for(settlement, state).collect();
    // In the order they open (lines from the start first, then by the log they wait for).
    open.sort_by_key(|memory| memory.after_log.unwrap_or(0));
    let count = u32::try_from(open.len()).ok().filter(|count| *count > 0)?;
    // Newest first: the last line opened is the first one told.
    let back = usize::try_from(visit % count).ok()?;
    open.get(open.len() - 1 - back).copied()
}

impl SettlementMemory {
    /// The line as a settlement's people pass it on: who says it, and what.
    #[must_use]
    pub fn told(&self) -> String {
        let mut speaker = self.speaker.to_owned();
        if let Some(first) = speaker.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        format!("{speaker}: \"{}\"", self.line)
    }
}
