//! Each species' story arc (Wave 12 aliens track): five chapters given by the elders of its
//! towns, one after another, each gated by how well the player understands the tongue.
//!
//! | Chapter | Kind | Done when |
//! |---|---|---|
//! | 1 | fetch | the player hands over ores the species prizes |
//! | 2 | explore | the player's rig (or pilot) reaches a place on the body: deep under the town, or the far side of the world |
//! | 3 | escort | a resident rides along to the species' next town on the body (the far side on a body with one town) |
//! | 4 | build | ores and credits handed over raise the town's monument (it stands from then on, for everyone) |
//! | 5 | the builders' trail | the elder tells what the species remembers of the builders and gives a lead: a star system charted on the nav chart (the hook for Wave 13's galaxy story) |
//!
//! Lines are written in plain words and shown through the language filter
//! ([`super::language::speech_tokens`]): an elder's request reads only as well as the player
//! understands the tongue.
//!
//! TODO-SEAM(w13-story): the quest engine ([`crate::quest`]) and the story arcs
//! ([`crate::people::story`]) bind targets on the home world's feature index; once they are
//! body-aware these chapters can become `ArcTemplate`s. The trail leads
//! ([`super::state::TrailLead`]) are the builders' trail's first steps beyond the Moon.

use serde::{Deserialize, Serialize};

use crate::terrain::MineralKind;

use super::{AlienSpecies, language::Understanding};

/// Chapters in each species' arc.
pub const CHAPTERS: u8 = 5;

/// What a chapter asks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ChapterGoal {
    /// Hand over `count` of `mineral` at the elder.
    Deliver { mineral: MineralKind, count: u32 },
    /// Reach `rows` rows under the town's ground, within its span.
    ReachDepth { rows: i32 },
    /// Reach the surface on the far side of the world from the town.
    ReachFarSide,
    /// Carry a resident to the species' next town on the body (or the far side).
    Escort,
    /// Hand over `count` of `mineral` and `credits` to raise the town's monument.
    Raise {
        mineral: MineralKind,
        count: u32,
        credits: u32,
    },
    /// Listen to the elder: the builders' trail.
    Trail,
}

/// One chapter of a species' arc.
#[derive(Clone, Copy, Debug)]
pub struct ChapterDef {
    pub title: &'static str,
    pub goal: ChapterGoal,
    /// Understanding the elder waits for before asking.
    pub needs: Understanding,
    /// The elder's request.
    pub ask: &'static str,
    /// What the elder says while it is under way.
    pub nudge: &'static str,
    /// The elder's thanks.
    pub thanks: &'static str,
    /// Credits paid at the end.
    pub credits: u32,
}

/// Understanding each chapter waits for.
pub const CHAPTER_NEEDS: [Understanding; CHAPTERS as usize] = [0, 150, 300, 450, 600];

/// The resident slot of the escorted traveller (the species' far-walker, else the trader).
pub const TRAVELLER_SLOT: usize = 7;

/// The chapters of `species`' arc.
#[must_use]
pub const fn chapters(species: AlienSpecies) -> &'static [ChapterDef; CHAPTERS as usize] {
    match species {
        AlienSpecies::Vessi => &VESSI,
        AlienSpecies::Tokkari => &TOKKARI,
        AlienSpecies::Siphra => &SIPHRA,
    }
}

/// Chapter `chapter` (0-based) of `species`' arc.
#[must_use]
pub fn chapter(species: AlienSpecies, chapter: u8) -> Option<&'static ChapterDef> {
    chapters(species).get(usize::from(chapter))
}

static VESSI: [ChapterDef; 5] = [
    ChapterDef {
        title: "A Warm Welcome",
        goal: ChapterGoal::Deliver {
            mineral: MineralKind::Copper,
            count: 5,
        },
        needs: CHAPTER_NEEDS[0],
        ask: "You are cold, small one, and so are we. Bring us five copper, the warm metal, \
              and we will sing you into the town.",
        nudge: "Copper. Five. It holds the warmth of a song.",
        thanks: "Hoo! Warm in the hand. Now you are one of our verses.",
        credits: 180,
    },
    ChapterDef {
        title: "The Buried Song",
        goal: ChapterGoal::ReachDepth { rows: 40 },
        needs: CHAPTER_NEEDS[1],
        ask: "Under the town the ice remembers a song older than us. Dig down forty rows and \
              listen. Tell us if it still sings.",
        nudge: "Forty rows down, under the spire. Listen for it.",
        thanks: "It still sings? Then the ice is still ours. Thank you, quiet driller.",
        credits: 320,
    },
    ChapterDef {
        title: "The Long Walk",
        goal: ChapterGoal::Escort,
        needs: CHAPTER_NEEDS[2],
        ask: "Our far-walker is too slow for the cold between towns this year. Carry them in \
              your machine to our next town. Do not hurry; do not stop.",
        nudge: "The far-walker rides with you. The next town waits.",
        thanks: "They arrived singing. Your machine is warmer than it looks.",
        credits: 420,
    },
    ChapterDef {
        title: "Raise the Spire-bell",
        goal: ChapterGoal::Raise {
            mineral: MineralKind::Ruby,
            count: 4,
            credits: 300,
        },
        needs: CHAPTER_NEEDS[3],
        ask: "The spire has no bell since the long storm. Four rubies, fire in stone, and three \
              hundred of your credits for the glass. Then the whole town will ring.",
        nudge: "Four rubies and three hundred credits for the bell.",
        thanks: "Listen! The bell! Every dawn it will ring your name.",
        credits: 380,
    },
    ChapterDef {
        title: "Songs of the Builders",
        goal: ChapterGoal::Trail,
        needs: CHAPTER_NEEDS[4],
        ask: "When the ice was young, tall ships passed over us, singing a song with no warmth \
              in it. They went toward a cold star. We kept its place in a verse. Take it.",
        nudge: "Hear the verse of the tall ships.",
        thanks: "Follow the verse. If you find the singers, tell them we remember.",
        credits: 500,
    },
];

static TOKKARI: [ChapterDef; 5] = [
    ChapterDef {
        title: "An Honest Weight",
        goal: ChapterGoal::Deliver {
            mineral: MineralKind::Iron,
            count: 8,
        },
        needs: CHAPTER_NEEDS[0],
        ask: "A stranger is a knot nobody tied. Bring eight iron, true weight, and we tie you \
              into the cord of the hold.",
        nudge: "Eight iron. We weigh it twice.",
        thanks: "True weight! Tok! The knot is tied. You are in the cord now.",
        credits: 200,
    },
    ChapterDef {
        title: "The Old Vein",
        goal: ChapterGoal::ReachDepth { rows: 60 },
        needs: CHAPTER_NEEDS[1],
        ask: "Our grandmothers dug a vein sixty rows under the hold and sealed it in a hurry. \
              Go down and see why. Our claws are too slow for the deep now.",
        nudge: "Sixty rows under the hold. Mind the old seals.",
        thanks: "Nothing but rock and old claw marks? Good. Fear is a short weight.",
        credits: 340,
    },
    ChapterDef {
        title: "Knots to Carry",
        goal: ChapterGoal::Escort,
        needs: CHAPTER_NEEDS[2],
        ask: "Our door-guard owes the next hold a debt, and a debt must be carried by hand. \
              Carry the guard there in your machine.",
        nudge: "The guard rides with you, debt-cord and all.",
        thanks: "The debt is paid and the knot untied. Your machine carries well.",
        credits: 430,
    },
    ChapterDef {
        title: "A Forge for the Hold",
        goal: ChapterGoal::Raise {
            mineral: MineralKind::Silver,
            count: 5,
            credits: 300,
        },
        needs: CHAPTER_NEEDS[3],
        ask: "The old forge cracked. Five silver for the bellows and three hundred credits for \
              the stone, and we raise a new one where everyone can see it burn.",
        nudge: "Five silver and three hundred credits for the new forge.",
        thanks: "It burns! Every blade we make now carries your knot.",
        credits: 400,
    },
    ChapterDef {
        title: "The Cord of the Builders",
        goal: ChapterGoal::Trail,
        needs: CHAPTER_NEEDS[4],
        ask: "The oldest cord in the hold has a knot nobody ever untied: a debt to the builders, \
              who mined here before us and left toward another star. Here is where they went.",
        nudge: "Read the oldest knot with us.",
        thanks: "If you find the builders, tell them the hold still owes them. We pay our debts.",
        credits: 520,
    },
];

static SIPHRA: [ChapterDef; 5] = [
    ChapterDef {
        title: "Clear Light",
        goal: ChapterGoal::Deliver {
            mineral: MineralKind::Emerald,
            count: 3,
        },
        needs: CHAPTER_NEEDS[0],
        ask: "You are new, so we do not remember you yet. Bring three emeralds, clear light, \
              and we will keep you in our glass.",
        nudge: "Three emeralds. Clear ones.",
        thanks: "Now we remember you. We always will.",
        credits: 220,
    },
    ChapterDef {
        title: "The Far Side",
        goal: ChapterGoal::ReachFarSide,
        needs: CHAPTER_NEEDS[1],
        ask: "On the far side of this world the stars are different. We cannot drift so far. Go \
              there and stand under them, so we can remember them through you.",
        nudge: "The far side of the world, under its stars.",
        thanks: "Yes... we see them now, in you. Thank you for the stars.",
        credits: 360,
    },
    ChapterDef {
        title: "Carry a Memory",
        goal: ChapterGoal::Escort,
        needs: CHAPTER_NEEDS[2],
        ask: "One of us holds a memory the next town has lost. Carry them there in your machine. \
              Gently: memories spill.",
        nudge: "Carry the dreamer gently to the next town.",
        thanks: "The memory is home. The town remembers itself again.",
        credits: 440,
    },
    ChapterDef {
        title: "Light the Pylon",
        goal: ChapterGoal::Raise {
            mineral: MineralKind::Diamond,
            count: 3,
            credits: 300,
        },
        needs: CHAPTER_NEEDS[3],
        ask: "A pylon of memory needs a heart of diamond. Three diamonds and three hundred credits \
              for the growing, and it will hold every memory of this town.",
        nudge: "Three diamonds and three hundred credits for the pylon.",
        thanks: "It glows. Everything we were is safe in it now.",
        credits: 420,
    },
    ChapterDef {
        title: "What the Eldest Remember",
        goal: ChapterGoal::Trail,
        needs: CHAPTER_NEEDS[4],
        ask: "The eldest remember ships older than any of us, long and silent, the builders \
              going between the stars. One passed this moon and turned toward a far star. We \
              give you its place.",
        nudge: "Remember the builders' ship with us.",
        thanks: "Go where they went. Remember for us what we cannot reach.",
        credits: 540,
    },
];

/// The sector offset (from home) each species' trail lead points at, before the seed's
/// jitter: three directions, a long warp out.
#[must_use]
pub const fn trail_offset(species: AlienSpecies) -> (i32, i32) {
    match species {
        AlienSpecies::Vessi => (9, -4),
        AlienSpecies::Tokkari => (-7, 8),
        AlienSpecies::Siphra => (3, 12),
    }
}
