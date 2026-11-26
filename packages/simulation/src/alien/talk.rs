//! What the species say (Wave 12 aliens track): their everyday talk, in plain words that the
//! language filter ([`super::language::speech_tokens`]) shows as glyphs until learned.
//!
//! A resident greets in their own tongue (the species' greeting, which no translation
//! touches), then says one of their species' lines picked by day and visit, then now and then a
//! line about home (the people engine's local colour for their culture).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "hashed rolls reduced into small bounded ranges"
)]

use crate::people::Character;

use super::AlienSpecies;

/// The local colour's village and player variables (`people::dialogue::local`).
const VILLAGE_VAR: &str = concat!("{", "village", "}");
const PLAYER_VAR: &str = concat!("{", "player", "}");

/// Everyday lines per species.
const fn lines(species: AlienSpecies) -> &'static [&'static str] {
    match species {
        AlienSpecies::Vessi => &[
            "You walk so fast. Does your song not get left behind?",
            "The cold is a guest here. We give it the best seat and it stays polite.",
            "Your machine hums a single note. It must be lonely.",
            "Sit with me until the next verse. There is no hurry anywhere.",
            "We heard you coming three songs ago. Engines carry on the ice.",
            "If you bring warmth, bring it slowly. Fast warmth cracks the vaults.",
            "My lantern is low today. A sad song does that.",
            "The eldest voice says you have a good silence. That is high praise.",
        ],
        AlienSpecies::Tokkari => &[
            "Your hull plates are not fused. Who weighed them? A cheat, I think.",
            "Tok! Mind the doorway. Round doors are for round heads.",
            "Every knot on my cord is a promise. Most of them are paid.",
            "We dig toward the warm stone at the middle. One day we arrive.",
            "Your drill sings wrong. Too fast, too proud. Let it bite.",
            "Silver sings, iron holds, platinum sleeps. Remember that.",
            "The forge never sleeps, so the smiths take turns not sleeping.",
            "A guest who pays in true weight is a cousin by the second visit.",
        ],
        AlienSpecies::Siphra => &[
            "We remember a hundred travellers. None of them landed so loudly.",
            "Starlight here is thin. We drink slowly, to make it last.",
            "I hold a memory of rain. I have never seen rain.",
            "Do not stand in our light, please. It is lunch.",
            "The pylons hum when they remember something. Listen.",
            "Your face is new. We will keep it in the glass.",
            "Long ago, silent ships passed this moon. We remember their shadows.",
            "Forgetting is the only death we fear. Tell us your name again.",
        ],
    }
}

/// A splitmix step.
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// What `character` (of `species`) says on a chat: the greeting unless the talk is under way,
/// then a line of theirs, and every third visit a line about home with `village` named.
#[must_use]
pub fn chat_lines(
    character: &Character,
    species: AlienSpecies,
    day: u32,
    visit: u32,
    continuing: bool,
    village: &str,
    player: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let def = species.def();
    if !continuing {
        out.push(format!("{}!", def.greeting.0));
    }
    let pool = lines(species);
    let roll = mix(u64::from(character.id.index)
        ^ u64::from(character.id.village.0) << 20
        ^ u64::from(day) << 40
        ^ u64::from(visit));
    out.push(pool[(roll % pool.len() as u64) as usize].to_owned());
    if visit % 3 == 2 {
        let local = crate::people::dialogue::local::local_lines(species.culture());
        let line = local[((roll >> 16) % local.len() as u64) as usize];
        out.push(
            line.replace(VILLAGE_VAR, village)
                .replace(PLAYER_VAR, player),
        );
    }
    out
}

/// What a species' resident says when a player first meets their kind.
#[must_use]
pub const fn first_contact_line(species: AlienSpecies) -> &'static str {
    match species {
        AlienSpecies::Vessi => {
            "Hoo-veh, ssii. The tall ones lean down and their throats glow: a long, warm chord."
        }
        AlienSpecies::Tokkari => {
            "Tok! Kaa-tok. Six claws click on the stone and two eye-stalks look you up and down."
        }
        AlienSpecies::Siphra => {
            "Sii... ea, phra. A bell of living glass drifts close and chimes, very softly."
        }
    }
}
