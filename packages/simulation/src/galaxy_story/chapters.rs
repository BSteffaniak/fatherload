//! The galaxy story's chapters (Wave 13 story): ten chapters along the builders' road, from
//! the Archive's first chart to the Harbour.
//!
//! A chapter is a list of [`Goal`]s done **in order** by anyone in the crew. Its targets (the
//! system, the world, the relic's site) are not written here: they are the story plan's
//! stop for the chapter ([`super::plan::StoryPlan::stop`]), a pure function of the world seed
//! and the chart the road starts from, so every peer agrees where the road goes. Words name
//! the targets through placeholders the journal fills in: `{system}`, `{world}`, `{theme}`
//! and `{harbour}`.

use crate::{hostiles::Faction, space::Theme};

/// How many chapters the galaxy story has.
pub const CHAPTER_COUNT: u8 = 10;

/// What a goal asks of the crew. Each reads the chapter's stop for its target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Goal {
    /// Reach the stop's system (in flight inside its reach, or on one of its worlds).
    ReachSystem,
    /// The stop's world scanned (a scanner ping that reaches it; the stations' record).
    Scan,
    /// Set down on the stop's world.
    Land,
    /// Stand (rig or pilot) at the stop's relic: on the surface, or dug down to it.
    FindRelic,
    /// Keep someone at the relic for `seconds` (world time, in total) while `raiders` come.
    Hold { seconds: u16, raiders: Faction },
    /// Meet one of the friendly species (first contact, anyone in the crew).
    MeetSpecies,
    /// Finish a chapter of a species' own arc (anyone in the crew).
    SpeciesChapter,
    /// Beat a faction's boss (the hostiles' lairs).
    BeatBoss(Faction),
    /// Reach the Harbour's gate in flight.
    ReachHarbour,
    /// Calm the Harbour's Warden (the finale's fight).
    CalmWarden,
    /// Make the finale's choice at the Harbour's heart.
    Choose,
}

impl Goal {
    /// Whether the goal needs the stop's world.
    #[must_use]
    pub const fn needs_world(self) -> bool {
        matches!(
            self,
            Self::Scan | Self::Land | Self::FindRelic | Self::Hold { .. }
        )
    }

    /// Whether the goal is met on the stop's world's ground (checked in its ground pass).
    #[must_use]
    pub const fn on_ground(self) -> bool {
        matches!(self, Self::FindRelic | Self::Hold { .. })
    }
}

/// One goal of a chapter with its words.
#[derive(Clone, Copy, Debug)]
pub struct GoalDef {
    pub goal: Goal,
    /// The journal's line for it (placeholders filled in).
    pub text: &'static str,
}

/// What a chapter's stop is looked for as.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopKind {
    /// The system of the Archive's first chart (and its biggest world).
    Anchor,
    /// The nearest world of one of these themes along the road (any world if none).
    Theme(&'static [Theme]),
    /// A friendly species' first home (the nearest).
    Species,
    /// A faction boss's lair.
    Lair(Faction),
    /// The Harbour's system, far down the road.
    Harbour,
}

/// One chapter.
#[derive(Clone, Copy, Debug)]
pub struct ChapterDef {
    pub title: &'static str,
    /// What the chapter's first card and journal page say.
    pub intro: &'static str,
    pub stop: StopKind,
    /// How deep the relic lies, rows under the surface (0: it stands on it).
    pub relic_rows: u16,
    /// What the relic is called.
    pub relic: &'static str,
    pub goals: &'static [GoalDef],
    /// What the closing card says.
    pub outro: &'static str,
    /// Credits paid to whoever finished the last goal (shared purse: the crew's finisher).
    pub credits: u32,
    /// Space logs opened when the chapter closes (indices into [`super::journal::SPACE_LOGS`]).
    pub logs: &'static [u8],
}

const fn goal(goal: Goal, text: &'static str) -> GoalDef {
    GoalDef { goal, text }
}

/// Every chapter, in order.
pub static CHAPTERS: [ChapterDef; CHAPTER_COUNT as usize] = [
    ChapterDef {
        title: "The First Road",
        intro: "The archivist unrolls the first chart Ines Calloway copied in the Archive of \
                Stars. \"Roads, not maps,\" it says. \"This one leaves the Moon's dock and runs to \
                a star the builders called {system}. Let us see where it bends.\"",
        stop: StopKind::Anchor,
        relic_rows: 0,
        relic: "the builders' waymark",
        goals: &[
            goal(
                Goal::ReachSystem,
                "Set course for {system}, the first star on the Archive's chart, and warp there.",
            ),
            goal(
                Goal::Scan,
                "Ping {world} with the scanner from orbit: the chart marks it.",
            ),
            goal(
                Goal::FindRelic,
                "Land on {world} and find the builders' waymark standing on its ground.",
            ),
        ],
        outro: "The waymark is a pylon of the same unscratchable stuff as the cable, humming the \
                way the town hums. Its face holds a line of builder script and a lamp that \
                lights when your Star Drive comes near. The road goes on.",
        credits: 3_000,
        logs: &[0, 1],
    },
    ChapterDef {
        title: "Old Friends of the Road",
        intro: "\"Waymarks are for travellers,\" the archivist says, \"and travellers are \
                remembered.\" The near stars have people. If anyone saw the builders pass, it \
                was them.",
        stop: StopKind::Species,
        relic_rows: 0,
        relic: "",
        goals: &[
            goal(
                Goal::MeetSpecies,
                "Find one of the peoples of the near stars and meet them ({world} is closest).",
            ),
            goal(
                Goal::SpeciesChapter,
                "Earn their trust: do what an elder asks of you, one chapter of their story.",
            ),
        ],
        outro: "Over the work the elder talks of tall, silent ships that passed when their \
                grandmothers were young, singing a song with no warmth in it, going outward. \
                \"They stopped,\" the elder says, \"wherever the machines sleep.\"",
        credits: 3_500,
        logs: &[2, 3],
    },
    ChapterDef {
        title: "The Silent Foundry",
        intro: "The waymark's script names a foundry on a {theme}: {world}, in {system}. \
                \"Where the machines sleep,\" the archivist repeats. \"Bring a lamp. And your \
                guns.\"",
        stop: StopKind::Theme(&[Theme::MachineWorld, Theme::DeadWorld, Theme::Crystal]),
        relic_rows: 30,
        relic: "the foundry door",
        goals: &[
            goal(Goal::ReachSystem, "Warp to {system}."),
            goal(
                Goal::Land,
                "Land on {world}, the {theme} the waymark named.",
            ),
            goal(
                Goal::FindRelic,
                "Dig down to the foundry's door, thirty rows under the beacon's column.",
            ),
            goal(
                Goal::Hold {
                    seconds: 60,
                    raiders: Faction::Machine,
                },
                "The door is reading your drive. Hold it for a minute: the Concord has heard it \
                 too.",
            ),
        ],
        outro: "The foundry door opens on a hall of empty cradles, each the shape of a ship the \
                size of a mountain. Only one cradle still holds anything: a log, and an order \
                that was never cancelled.",
        credits: 4_500,
        logs: &[4, 5],
    },
    ChapterDef {
        title: "The Quiet World",
        intro: "The foundry's order points at {world} in {system}, a world that went quiet \
                long before anyone on it could write. \"An archive,\" the archivist says, and \
                for once it sounds hungry.",
        stop: StopKind::Theme(&[Theme::DeadWorld, Theme::MachineWorld, Theme::Ice]),
        relic_rows: 15,
        relic: "the ruin archive",
        goals: &[
            goal(Goal::ReachSystem, "Warp to {system}."),
            goal(Goal::Scan, "Scan {world} to find the ruins under its dust."),
            goal(
                Goal::FindRelic,
                "Land and dig fifteen rows under the ruin's mark to its archive.",
            ),
        ],
        outro: "The archive is one room and one voice. The builders came from a star that was \
                dying. They hollowed worlds and hung Star Drives in their hearts and carried \
                their people out in them. They called what they carried the Load.",
        credits: 5_000,
        logs: &[6, 7, 8],
    },
    ChapterDef {
        title: "The Concord's Captain",
        intro: "The archive names the Concord: the Harbour's pilots, whose captain carries one \
                of its keys. The Concord's capital ship, the Arbiter, waits in its lair in \
                {system}.",
        stop: StopKind::Lair(Faction::Machine),
        relic_rows: 0,
        relic: "",
        goals: &[goal(
            Goal::BeatBoss(Faction::Machine),
            "Find the Arbiter in its lair in {system} and beat it. Its wreck carries the key.",
        )],
        outro: "In the Arbiter's wreck, under its lance, a builder plate still holds a course: \
                the Harbour's bearing, and a line the Concord had been repeating to itself for an \
                age. Hold the road. Wait for the key.",
        credits: 6_000,
        logs: &[9, 10],
    },
    ChapterDef {
        title: "The Singing Spires",
        intro: "\"Crystal remembers,\" the archivist says. \"We wrote our voices into it.\" The \
                Arbiter's plate names a spire on {world} in {system}.",
        stop: StopKind::Theme(&[Theme::Crystal, Theme::Ice, Theme::Fungal]),
        relic_rows: 0,
        relic: "the memory spire",
        goals: &[
            goal(Goal::ReachSystem, "Warp to {system}."),
            goal(Goal::Land, "Land on {world}."),
            goal(
                Goal::FindRelic,
                "Find the memory spire on {world}'s ground and stand in its light.",
            ),
        ],
        outro: "The spire sings in a voice you almost know. It is a builder mother, speaking to \
                a child she would not see again: Look up. We will come back with the key.",
        credits: 6_500,
        logs: &[11, 12],
    },
    ChapterDef {
        title: "The Fathers' Load",
        intro: "One more waymark before the long dark: a nursery beacon on {world}, a world in \
                {system}. The archivist is quiet for a long time, then says only: \"Go. I will \
                explain after. I should have explained before.\"",
        stop: StopKind::Theme(&[Theme::TidallyLocked, Theme::RingWorld, Theme::DesertDunes]),
        relic_rows: 0,
        relic: "the nursery beacon",
        goals: &[
            goal(Goal::ReachSystem, "Warp to {system}."),
            goal(Goal::Land, "Land on {world}."),
            goal(
                Goal::FindRelic,
                "Find the nursery beacon and read its record.",
            ),
        ],
        outro: "The record is a list of names, in builder script and, under it, in the town's \
                own letters: the first families of the town. The builders did not leave home \
                empty. They left their children aboard, with a lamp lit and a cable to climb, \
                and those who stayed to raise them were called the fathers. You are the Load.",
        credits: 7_000,
        logs: &[13, 14, 15],
    },
    ChapterDef {
        title: "The Gathering Light",
        intro: "The Harbour's lamp-key was left where the road turns for the last time: {world} \
                in {system}. Something else wants it: the Bloom smells a lamp from a sector off.",
        stop: StopKind::Theme(&[
            Theme::GasGiant,
            Theme::RingWorld,
            Theme::Fungal,
            Theme::Jungle,
        ]),
        relic_rows: 0,
        relic: "the lamp-key",
        goals: &[
            goal(Goal::ReachSystem, "Warp to {system}."),
            goal(Goal::Scan, "Scan {world} for the lamp-key's glow."),
            goal(Goal::FindRelic, "Land on {world} and take up the lamp-key."),
            goal(
                Goal::Hold {
                    seconds: 45,
                    raiders: Faction::Swarm,
                },
                "The lamp is waking in your hands. Keep it lit while the Bloom comes for it.",
            ),
        ],
        outro: "The lamp-key settles into your Star Drive as if it had always belonged there, \
                and the nav chart draws a road you could not see before: long, straight and dark, \
                to {harbour}.",
        credits: 8_000,
        logs: &[16, 17],
    },
    ChapterDef {
        title: "The Long Dark",
        intro: "The road runs out of charted space. Past the last waymark the stars thin, and \
                then there is only the lamp-key's bearing and a dim red star called {harbour}.",
        stop: StopKind::Harbour,
        relic_rows: 0,
        relic: "",
        goals: &[
            goal(
                Goal::ReachSystem,
                "Cross the long dark to {harbour}. Fit out first: it is a long way from any dock.",
            ),
            goal(
                Goal::ReachHarbour,
                "Find the Harbour: a ring of builder ships round the red star.",
            ),
        ],
        outro: "The Harbour is a ring of ships round a red star, thousands of them, moored hull \
                to hull and dark, each the size of a world. In the middle of the ring, where the \
                lamps should be, something enormous turns to look at you.",
        credits: 9_000,
        logs: &[18, 19],
    },
    ChapterDef {
        title: "The Harbour",
        intro: "The Harbour's Warden has kept the sleepers for longer than your world has had a \
                name. It does not know you. It knows only that nothing without a key may come \
                near.",
        stop: StopKind::Harbour,
        relic_rows: 0,
        relic: "",
        goals: &[
            goal(
                Goal::CalmWarden,
                "Calm the Warden: break its lamps until it sees the key in your drive.",
            ),
            goal(
                Goal::Choose,
                "At the Harbour's heart: ring the Homecoming Bell, or keep the lamp.",
            ),
        ],
        outro: "The road is walked. Whatever you chose, the builders know now that their \
                children came looking. The stars are still out there, and so are you.",
        credits: 15_000,
        logs: &[20, 21],
    },
];

/// Chapter `chapter` (0-based).
#[must_use]
pub fn chapter(chapter: u8) -> Option<&'static ChapterDef> {
    CHAPTERS.get(usize::from(chapter))
}

/// Every goal of the story, in order: `(chapter, step)`.
pub fn all_goals() -> impl Iterator<Item = (u8, u8)> {
    CHAPTERS.iter().enumerate().flat_map(|(index, chapter)| {
        let index = u8::try_from(index).unwrap_or(u8::MAX);
        (0..chapter.goals.len()).map(move |step| (index, u8::try_from(step).unwrap_or(u8::MAX)))
    })
}
