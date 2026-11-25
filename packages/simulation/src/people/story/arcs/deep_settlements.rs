//! Batch: the deep settlements' own stories (Wave 10 story). Two or three arcs for each
//! settlement down the builders' cable: Gravel Hollow (I), Cathedral Station (II), the Amber
//! Court of the Hive (III), Merrow City (IV), the Sunfolk Kraal (V), Parallax Station (VI) and
//! the Megacity outpost (VII). Ids 1401-1469.
//!
//! Each arc is offered only by the people of its own settlement
//! ([`Trigger::Settlement`], matched by the population's registered name,
//! [`crate::expedition::memories::Settlement::of_place_name`]) and is bound to the places of
//! its own Descent: the nearest of the Fathers' expedition sites ([`StoryPlace::FathersSite`]),
//! a seam of rock in the settlement's own stratum ([`StoryPlace::DeepSeam`]) and the settlement
//! itself, whose people are the contacts. They tie the settlements to the Fathers they
//! remember, and pay in the deep economy's coin: credits, layer resources and furniture.

use super::{NO_THANKS, choice, ending, give_up, log, meet, place, step, voices};
use crate::{
    expedition::memories::Settlement,
    home::Furniture,
    people::{
        character::Role,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Trigger},
    },
    quest::{Objective, QuestItem, QuestReward, QuestTier, StoryPlace},
    terrain::StrategicResourceKind,
};

/// A quest reward of credits, materials and reputation.
const fn reward(
    credits: u32,
    materials: &'static [(StrategicResourceKind, u32)],
    reputation: u32,
) -> QuestReward {
    QuestReward {
        credits,
        materials,
        fitting: None,
        fragment: None,
        bombs: 0,
        kit: None,
        reputation,
    }
}

pub const BATCH: &[ArcTemplate] = &[
    // --- Gravel Hollow (Descent I, the Grey Stone) --------------------------------------------
    ArcTemplate {
        id: ArcId(1401),
        title: "Where the Rails Go",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Old Tully's granddad found the Hollow's rails already laid. Tully wants to know, before he dies, where they lead.",
        twist: "The rails run on past every mine the Hollow ever dug, straight toward the builders' cable, in iron no smith of the Hollow could have forged.",
        consequence: "The Hollow learns its rails were the builders' first, and the smith hangs a new sign over his forge: Est. Before Us.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::GravelHollow)],
        roles: &[Role::Smith, Role::RetiredMiner],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Follow the old rails",
            "Read the rock where they end",
            "Tell the Hollow",
        ],
        steps: &[
            step(
                "{Giver} says the Hollow's oldest rails run on past the last mine. Dig after them to {dest}, {where}.",
                "My granddad walked them as far as his lamp went. Then he turned round. Don't turn round.",
            ),
            step(
                "The rails stop dead in the rock. Pulse the scanner and see what lies past the end of the line.",
                "Rails don't just stop. Somebody stopped them. Find out why.",
            ),
            meet(
                "The scanner shows the line running on, buried, straight to the builders' cable. Tell {contact} in {village}.",
                "Go on, tell them. They'll not believe me, but they'll believe a rig.",
                "Iron older than the Hollow, laid toward the cable? Then we didn't build this place. We just moved in. I need a sit down.",
            ),
        ],
        climax: "Under the scanner the buried line runs on, plumb and true, to the builders' cable, in iron that has not rusted in an age. The Hollow's rails were never the Hollow's.",
        twists: &[],
        reward: reward(420, &[(StrategicResourceKind::MachineRelic, 1)], 10),
        outcomes: &[
            ending(
                "Hang the new sign",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Furniture(Furniture::MountedPick),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Est. Before Us. That's what goes over the forge. Here, take my granddad's pick for your wall. He'd want it to see the bottom.",
                "A new sign hangs over the Hollow's forge: Est. Before Us. {Giver} tells the story to every rig that stops.",
            ),
            choice(
                "Keep it quiet",
                &[ArcEffect::Reputation(5), ArcEffect::Credits(120)],
                "Quiet, then. Some folk like to think their great-grandparents laid every sleeper. Let them.",
                "The Hollow still believes it laid its own rails. {Giver} knows better, and smiles about it in the forge.",
            ),
            give_up(
                &[],
                "The rails run on into the dark, unfollowed, for one more generation.",
            ),
        ],
        pitch: voices(
            &["The rails. Where do they go? I'm old. I'd like to know before I'm older."],
            &[
                "My granddad found the rails already laid when he came! Will you follow them to the end for me? I've wondered all my life!",
            ],
            &[
                "Every night I hear the old rails tick as the rock cools, as if they lead somewhere. Would you follow them for an old man?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Gravel Hollow's smith wants to know where the oldest rails lead.",
            "Dig after the rails to their end, scan the rock, then tell the Hollow.",
            "The rails were the builders', laid toward the cable.",
        ),
    },
    ArcTemplate {
        id: ArcId(1402),
        title: "It Worked",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "A Hollow child sold Teo Marsh a lucky stone a generation ago, for a whole credit. The family wants a new stone left on the Fathers' trail for whoever comes next.",
        twist: "At the Fathers' camp there is already a stone on the crate, scratched with three words in a young man's hand: IT WORKED. T.M.",
        consequence: "The stone family keeps Teo's answer in the window, and every rig that goes down from the Hollow now leaves a lucky stone at the Fathers' camp.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::GravelHollow)],
        roles: &[Role::Apprentice, Role::Prospector],
        slots: &[],
        targets: &[place(StoryPlace::FathersSite), place(StoryPlace::Home)],
        stages: &[
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Leave a stone at the camp", "Bring the answer back"],
        steps: &[
            step(
                "{Giver} has a new lucky stone for the Fathers' trail. Climb out at {dest}, {where}, and set it on the canister's crate by hand.",
                "It has to go by hand. A drill can't carry luck, everybody knows that.",
            ),
            meet(
                "There was already a stone on the crate, with IT WORKED. T.M. scratched in it. Take the news to {contact} in {village}.",
                "Well? Did you leave it? You've got a face like you found something.",
                "It worked? He wrote that it worked? Grandad will cry. Grandad cries at weddings and dogs. He'll cry for a week.",
            ),
        ],
        climax: "Teo's stone is still on the crate after all these years, three words scratched into it with a screwdriver: IT WORKED. T.M. You set the new stone beside it.",
        twists: &[],
        reward: reward(380, &[], 9),
        outcomes: &[
            ending(
                "Start a tradition",
                &[
                    ArcEffect::Reputation(9),
                    ArcEffect::Furniture(Furniture::Lantern),
                    ArcEffect::Settlement(2),
                    ArcEffect::Remember,
                ],
                "From now on every rig that goes down takes a stone. Take this lantern, so you can find the camp again in the dark.",
                "A pile of lucky stones grows on the Fathers' crate. Teo's sits on top, where everyone can read it.",
            ),
            choice(
                "Bring Teo's stone home",
                &[ArcEffect::Reputation(4), ArcEffect::Mood(-6)],
                "You brought it up? Oh. Oh, it's lovely. But it was his, down there. Well. It's ours now, I suppose.",
                "Teo's stone sits in the Hollow's window. The crate down below is bare, and it feels a little colder.",
            ),
            give_up(
                &[],
                "The new stone stays in a pocket in the Hollow, unlucky and unused.",
            ),
        ],
        pitch: voices(
            &[
                "Teo Marsh bought a lucky stone off my grandad. A new one should go down the trail. You're going down.",
            ],
            &[
                "My grandad sold a lucky stone to one of the Fathers! Will you take a new one down to their camp, for the next one who comes?",
            ],
            &[
                "Grandad says luck has to be passed on or it goes stale. Would you carry a stone down to the Fathers' camp for us?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A Hollow family wants a lucky stone left on the Fathers' trail.",
            "Leave the stone at the Fathers' camp on foot, then tell the family.",
            "Teo's old stone was still there: IT WORKED.",
        ),
    },
    ArcTemplate {
        id: ArcId(1403),
        title: "Brannigan's Stew",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Ruth Brannigan cooked a stew in the Hollow's inn that people still ask for, and promised to send the recipe up from the bottom. The innkeeper has waited a generation.",
        twist: "Bran left the recipe in a tin at the Fathers' camp. It reads, in full: Mostly onions. Don't tell Tully.",
        consequence: "The inn puts Brannigan's Stew back on the board, mostly onions, and nobody tells Tully.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::GravelHollow)],
        roles: &[Role::Innkeeper, Role::Baker],
        slots: &[],
        targets: &[place(StoryPlace::FathersSite), place(StoryPlace::Home)],
        stages: &[
            &[Objective::PickUp {
                target: 0,
                parcel: "Bran's recipe tin",
            }],
            &[Objective::HandDeliver {
                target: 1,
                parcel: "Bran's recipe tin",
            }],
        ],
        beats: &["Find Bran's tin", "Carry it to the inn"],
        steps: &[
            step(
                "{Giver} is sure Bran left the stew recipe on the trail. Look for a tin at {dest}, {where}.",
                "She said she'd send it from the bottom. The bottom's a long way. Maybe she left it on the way.",
            ),
            step(
                "There was a tin, labelled STEW. Climb out and carry it by hand to the inn in {village}.",
                "Don't open it! Don't you dare. I want to open it. I've waited a generation to open it.",
            ),
        ],
        climax: "Inside the tin, on a card spotted with old grease, in Ruth Brannigan's big round hand: MOSTLY ONIONS. DON'T TELL TULLY.",
        twists: &[],
        reward: reward(400, &[], 10),
        outcomes: &[
            ending(
                "Put it back on the board",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Furniture(Furniture::Stove),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Mostly onions. A generation of guessing, and it's onions. Take the old stove from the back, I'm buying a bigger pot.",
                "Brannigan's Stew is back on the inn's board. It is mostly onions. Nobody has told Tully.",
            ),
            choice(
                "Tell Tully",
                &[ArcEffect::Reputation(3), ArcEffect::Settlement(2)],
                "You told him? He laughed so hard he dropped his hammer. Well. Maybe that's what she wanted all along.",
                "Old Tully tells everyone the secret of Brannigan's Stew, and laughs every time.",
            ),
            give_up(&[], "The stew stays a mystery, and the inn keeps guessing."),
        ],
        pitch: voices(
            &["Brannigan promised me a recipe. A generation ago. I'd like it now."],
            &[
                "Ruth Brannigan's stew was the best thing ever cooked in my kitchen! She promised me the recipe. Will you find it?",
            ],
            &[
                "Folk still ask for a stew a woman cooked here once and never wrote down. Would you look for her recipe on the trail?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Gravel Hollow's innkeeper wants Ruth Brannigan's stew recipe.",
            "Find Bran's tin at the Fathers' camp, then carry it to the inn by hand.",
            "Mostly onions.",
        ),
    },
    // --- Cathedral Station (Descent II, the Old Deep) -----------------------------------------
    ArcTemplate {
        id: ArcId(1411),
        title: "Calloway's Benchmarks",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Cathedral Station's surveyor copies Ines Calloway's old marks and adds her own name under them. One mark has never been checked, and she wants it read with a modern scanner.",
        twist: "Calloway's mark is exact to the tile, and beside it, in chalk, she wrote the next mark's depth: one no rig of her day could reach.",
        consequence: "The station's charts carry Calloway's numbers to the Heart, and the surveyor stops adding her own name underneath.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::OldDeepCamp)],
        roles: &[Role::Cartographer],
        slots: &[],
        targets: &[
            place(StoryPlace::DeepSeam),
            place(StoryPlace::FathersSite),
            place(StoryPlace::Home),
        ],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::Speak { target: 2 }],
        ],
        beats: &[
            "Scan the old benchmark",
            "Check it against the camp",
            "Report to the station",
        ],
        steps: &[
            step(
                "{Giver} wants Calloway's unchecked benchmark read. Pulse the scanner at {dest}, {where}.",
                "Her marks are always plumb to the cable. Always. If this one isn't, I'll eat my theodolite.",
            ),
            step(
                "The benchmark reads true to the tile. Now ride to {dest2}, {where2}, where she would have taken it from.",
                "She measured everything twice from wherever she camped. Go and stand where she stood.",
            ),
            meet(
                "Beside the camp's canister a chalk line gives the next benchmark's depth, far below any rig of her day. Tell {contact} at {village}.",
                "Well? Was she right? Of course she was right. Tell me anyway.",
                "The next mark, down there? She was surveying a road she knew she'd never come back up. That's the bravest number I ever saw.",
            ),
        ],
        climax: "Calloway's benchmark reads true to the tile. At her camp, in fresh-looking chalk on the rock: NEXT MARK 4,200 DOWN. TRUST THE CABLE. I.C.",
        twists: &[],
        reward: reward(1_250, &[(StrategicResourceKind::CrystalLens, 2)], 13),
        outcomes: &[
            ending(
                "Carry her numbers down",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Furniture(Furniture::StarChart),
                    ArcEffect::Credits(600),
                    ArcEffect::Remember,
                ],
                "Her numbers go on every chart we sell, all the way down. Take her star chart. I copied it by hand, so mind the coffee ring.",
                "Cathedral Station's charts carry Ines Calloway's numbers to the bottom of the map, with no other names underneath.",
            ),
            choice(
                "Add your own name",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(300)],
                "Your name, under hers? Bold. Fair enough, you read it. Here's a surveyor's fee.",
                "The station's charts say Calloway, and under it, smaller, the name of the rig that checked her.",
            ),
            give_up(
                &[],
                "The benchmark stays unchecked, and the surveyor keeps signing under it.",
            ),
        ],
        pitch: voices(
            &["One of Calloway's marks was never checked. I want it read. Properly."],
            &[
                "Ines Calloway's survey marks are the best down here, and one has never been checked! Will you scan it for me?",
            ],
            &[
                "There is one mark of Calloway's I've never dared to check, in case she was wrong. Would you read it for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Cathedral Station's surveyor wants Calloway's last benchmark checked.",
            "Scan the benchmark, check it from the Fathers' camp, report back.",
            "Calloway was right, and left the next depth in chalk.",
        ),
    },
    ArcTemplate {
        id: ArcId(1412),
        title: "Halloran's Splice",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The quartermaster sold Dov Halloran his fuel drums and still talks about the neat double-wrapped splice he left in the old drop line. She wants a length of it for the station wall.",
        twist: "The splice holds a coil of tape inside the wrapping: Dov's spare recording of the town's hymn, kept dry all these years.",
        consequence: "The splice hangs over Cathedral Station's counter, and on rest days the station plays Dov's flat, happy hymn.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::OldDeepCamp)],
        roles: &[Role::Quartermaster, Role::Trader],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 25,
            }],
            &[Objective::PickUp {
                target: 0,
                parcel: "a length of Dov's drop line",
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Hold out at the old line",
            "Cut the splice free",
            "Bring it to the counter",
        ],
        steps: &[
            step(
                "The old drop line runs through {dest}, {where}. {Giver} says the rock there shifts: hold your rig steady in it while you find the splice.",
                "The rock round the line moves in its sleep. Wait it out. Halloran did.",
            ),
            step(
                "There it is, double wrapped, neat as a bandage. Cut it free and bring it along.",
                "Mind the wrapping. That man wrapped a splice like he was tucking in a baby.",
            ),
            meet(
                "Inside the wrapping is a coil of tape. Take the splice and the tape to {contact} at {village}.",
                "Got it? Bring it here. I've a nail picked out and everything.",
                "A tape, in the splice? He kept it dry for us. Of course he did. Put it on, put it on. Oh, he sang flat. He sang so flat.",
            ),
        ],
        climax: "You cut the splice free. Inside Dov's neat double wrapping is a coil of tape, dry as the day he hid it, labelled HYMN, SPARE, D.H.",
        twists: &[],
        reward: reward(1_100, &[(StrategicResourceKind::RadiantFossil, 1)], 12),
        outcomes: &[
            ending(
                "Hang it over the counter",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Furniture(Furniture::Gramophone),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Over the counter it goes. And the gramophone's yours, I've got his voice now and that's all I wanted it for.",
                "Dov Halloran's splice hangs over Cathedral Station's counter. On rest days the station sings his hymn, flat, on purpose.",
            ),
            choice(
                "Leave the splice in the line",
                &[ArcEffect::Reputation(5), ArcEffect::Mood(-5)],
                "You left it? Well. It's still holding, I suppose. It was always going to hold.",
                "The splice still holds the old drop line together. The quartermaster's nail stays empty.",
            ),
            give_up(
                &[],
                "The splice stays in the dark rock, double wrapped, holding nothing up.",
            ),
        ],
        pitch: voices(
            &["Halloran's splice is still in the old line. I want it on my wall."],
            &[
                "I sold Dov Halloran his drums, you know! His splice is still in the old drop line. Will you fetch me a length of it?",
            ],
            &[
                "There is a splice in the old line that a kind man made, and I'd like to keep it safe. Would you bring it to me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Cathedral Station's quartermaster wants Dov Halloran's old splice.",
            "Hold steady at the old line, cut the splice free and bring it back.",
            "The splice held a spare tape of Dov's hymn.",
        ),
    },
    ArcTemplate {
        id: ArcId(1413),
        title: "One a Day, Dov",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The station's medic keeps Ruth Brannigan's iodine tin on her shelf for luck. She has heard there was a lead tent at the Fathers' camp, and wants to see where Bran nursed Dov through the glow.",
        twist: "The camp's lead tent still holds a sickbed made of sled boards, and a note pinned over it: DOV, ONE A DAY. I MEAN IT. R.B.",
        consequence: "The medic writes her own notes the way Bran did, short and stern, and her patients take their pills.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::OldDeepCamp)],
        roles: &[Role::Healer],
        slots: &[],
        targets: &[place(StoryPlace::FathersSite), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Find the camp", "Look inside on foot", "Tell the medic"],
        steps: &[
            step(
                "{Giver} wants to know how Bran nursed Dov. Find {dest}, {where}.",
                "A lead tent, they say. In the glow. She must have been frightened out of her wits and never once shown it.",
            ),
            step(
                "The tent is too low for the rig. Climb out and look inside on foot.",
                "Mind the floor in there. Medics leave sharp things lying about. I should know.",
            ),
            meet(
                "A sled-board sickbed and a stern note pinned above it. Tell {contact} at {village} what you saw.",
                "Well? What did she leave? I'm a medic, you can tell me the worst.",
                "One a day, I mean it. That's how you do it. Short words, big letters, and you don't leave the room till they swallow.",
            ),
        ],
        climax: "Inside the low lead tent, a sickbed of sled boards, a cold lamp and a note in big round letters: DOV, ONE A DAY. I MEAN IT. R.B.",
        twists: &[],
        reward: reward(1_000, &[], 12),
        outcomes: &[
            ending(
                "Write notes like Bran",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Furniture(Furniture::WallLamp),
                    ArcEffect::Credits(500),
                    ArcEffect::Remember,
                ],
                "Short words, big letters. I'm doing that from today. Take the lamp from the sick bay, I'll be working in the light now.",
                "Cathedral Station's medic pins short, stern notes over every bed. Her patients take their pills.",
            ),
            choice(
                "Bring the note back",
                &[ArcEffect::Reputation(5), ArcEffect::Credits(250)],
                "You brought it? I'll frame it. But she'd want it read by a patient, not a medic. Ah well.",
                "Bran's note hangs framed in the station's sick bay. The sled-board bed below waits empty.",
            ),
            give_up(
                &[],
                "The lead tent stays closed, and the medic keeps guessing.",
            ),
        ],
        pitch: voices(
            &["Brannigan nursed Halloran in a lead tent at their camp. I want to know how."],
            &[
                "I keep Ruth Brannigan's iodine tin for luck! Will you find the tent where she nursed Dov and tell me what's there?",
            ],
            &[
                "I think of a medic in a lead tent, far from anywhere, keeping a man alive. Would you go and see where she did it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Cathedral Station's medic wants to see Bran's lead tent.",
            "Find the Fathers' camp, look inside on foot and tell the medic.",
            "A sickbed of sled boards and a stern note.",
        ),
    },
    // --- The Amber Court (Descent III, the Hive) ----------------------------------------------
    ArcTemplate {
        id: ArcId(1421),
        title: "The Wrench-One's Pumps",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The nursery's warm pumps that Teo Marsh kept singing have choked: a vein of rock has crept into the old builder pipe below the Court.",
        twist: "The pipe was choked with Teo's own spare washers, hidden there for the day the pumps would need them, wrapped in a note: FOR THE NEXT WRENCH.",
        consequence: "The pumps sing again, the nursery is warm, and the Court names its first wrench-wielding worker after you.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::TheHive)],
        roles: &[Role::Mechanic, Role::Healer],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 12,
            }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Clear the choked pipe",
            "Listen to the pumps",
            "Tell the Court",
        ],
        steps: &[
            step(
                "Click-hum. {Giver} says rock has crept into the old pipe at {dest}, {where}. Dig it clear.",
                "The brood is cold. Cold brood is sad brood. Dig, soft-shell. Dig.",
            ),
            step(
                "Washers spill from the cleared pipe. Pulse the scanner to hear whether the pumps sing again.",
                "Listen with your machine. The pumps sing low, lower than you hear.",
            ),
            meet(
                "The pumps sing. In the pipe, Teo's washers and a note: FOR THE NEXT WRENCH. Tell {contact} in the Court.",
                "Click. Do they sing? We feel the walls. We think they sing.",
                "Hm-hm-hm. The Wrench-One left us his little rings, for the next wrench. You are the next wrench. The brood will say your name wrong, to make you laugh.",
            ),
        ],
        climax: "The rock gives way and the pipe coughs out a handful of bright washers wrapped in oilcloth: FOR THE NEXT WRENCH. T. Then the pumps sing, low and warm, all through the comb.",
        twists: &[],
        reward: reward(1_200, &[(StrategicResourceKind::MachineRelic, 2)], 14),
        outcomes: &[
            ending(
                "Become the next wrench",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::Furniture(Furniture::GlowcapLamp),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "Click-hum. Take a lamp from the nursery, wrench. The brood made it. It glows the colour of warm.",
                "The nursery's pumps sing again. In the Court the young say the name of the next wrench wrong, on purpose, and laugh.",
            ),
            choice(
                "Leave the washers for later",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(400)],
                "Hm. You keep his rings in the pipe, for the next next wrench. That is a long thought. We like long thoughts.",
                "Teo's washers wait in the pipe again, for whoever comes after you.",
            ),
            give_up(
                &[],
                "The nursery stays cold, and the Court's workers huddle the brood close.",
            ),
        ],
        pitch: voices(
            &["Click. The Wrench-One's pumps are choked. You have a drill. Come."],
            &[
                "Hum-hum! The warm pumps the Wrench-One loved have stopped singing! Will you dig the old pipe clear for the brood?",
            ],
            &[
                "The walls of the nursery are cooling, and the brood cries a song with no warmth in it. Would you help the pumps sing?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Amber Court's nursery pumps have choked.",
            "Dig the old pipe clear, scan the pumps, tell the Court.",
            "Teo left washers for the next wrench.",
        ),
    },
    ArcTemplate {
        id: ArcId(1422),
        title: "The Humming Song",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The brood hums a song a frightened soft-shell woman taught it a generation ago. The nurse wants the song carried back to the place it was first sung, so the brood can hear its echo.",
        twist: "In the quiet of the Fathers' camp the rig's microphones catch the walls humming the same song back, faint and slow: the comb remembered it too.",
        consequence: "The Court sings Bran's song at every hatching, and the walls answer.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::TheHive)],
        roles: &[Role::Healer, Role::ShrineKeeper],
        slots: &[],
        targets: &[place(StoryPlace::FathersSite), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 30,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Wait in the quiet camp", "Bring back the echo"],
        steps: &[
            step(
                "Hm-hm. {Giver} wants the song carried to {dest}, {where}. Sit in the camp and listen a while.",
                "Be still in the soft-shells' camp. Songs come back to the still.",
            ),
            meet(
                "The walls hummed the song back to you. Tell {contact} in the Court what you heard.",
                "Did the walls answer? The nurse waits. The brood waits. Everyone waits, soft-shell.",
                "The walls know her song too. Then she sang it to all of us, the comb and the brood and the stone. Hm-hm-hm. Listen, they hum it louder now.",
            ),
        ],
        climax: "In the quiet of the camp the microphones pick up a sound under the hum of the cable: the walls, humming Bran's frightened little song back, slow and steady.",
        twists: &[],
        reward: reward(1_000, &[(StrategicResourceKind::RadiantFossil, 1)], 13),
        outcomes: &[
            ending(
                "Sing it at every hatching",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Furniture(Furniture::HangingPlant),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "At every hatching now, her song. Take a hanging garden from the nursery, soft-shell. It grows to the song.",
                "The Amber Court sings Bran's song at every hatching, and the walls of the comb hum it back.",
            ),
            choice(
                "Keep the echo to yourself",
                &[ArcEffect::Reputation(4), ArcEffect::Mood(-8)],
                "You keep it? Click. The nurse is sad. The brood does not know why, so it hums the sad part.",
                "The brood still hums Bran's song. Only you know the walls hum it too.",
            ),
            give_up(
                &[],
                "The brood hums its song alone, and the walls keep quiet.",
            ),
        ],
        pitch: voices(
            &["Click. The brood's song came from the soft-shells' camp. Take it back. Listen."],
            &[
                "The brood hums the song the singing soft-shell taught us! Will you take it back to her camp and hear if it echoes?",
            ],
            &[
                "A frightened woman once hummed to our young in the dark. Would you carry her song back to where she first sang it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Amber Court's nurse wants Bran's song carried back to her camp.",
            "Wait in the Fathers' camp, then bring back what you heard.",
            "The walls hummed the song back.",
        ),
    },
    // --- Merrow City (Descent IV) -------------------------------------------------------------
    ArcTemplate {
        id: ArcId(1431),
        title: "Sefa's Second Verse",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Sefa Rhee wrote the merrow's songs in a book, and the city's archivist swears there was a second verse to the oldest one that never made it into the archive.",
        twist: "The page in Sefa's camp is the second verse, and it is not the merrow's: it is about rain on a town far above, written for the merrow to sing back.",
        consequence: "Merrow City sings Sefa's second verse at the bubble's festival, a song about rain from people who have never seen any.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::MerrowCity)],
        roles: &[Role::Scholar, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::FathersSite), place(StoryPlace::Home)],
        stages: &[
            &[Objective::PickUp {
                target: 0,
                parcel: "a page of Sefa's songbook",
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Find Sefa's page", "Bring it to the singers"],
        steps: &[
            step(
                "{Giver} believes Sefa left the missing verse at her camp: {dest}, {where}. Find the page.",
                "Bubble-blub. A dry-air book, lined pages, her small careful hand. The verse is in it somewhere.",
            ),
            meet(
                "The page is a song about rain. Bring it to {contact} in {village} to sing.",
                "You have it? Do not let it get wet. We have been very careful about wet, for her sake.",
                "Rain? She wrote us a verse about rain? Water falling from the sky onto dry people? Oh, it is beautiful. It makes no sense. We will sing it forever.",
            ),
        ],
        climax: "The page is pressed in oilcloth under the canister: Verse Two, for the merrow to sing back up the cable. It is about rain on a roof in a town that hums.",
        twists: &[],
        reward: reward(2_800, &[(StrategicResourceKind::PressurePearl, 2)], 16),
        outcomes: &[
            ending(
                "Sing it at the festival",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Furniture(Furniture::CoralLamp),
                    ArcEffect::Credits(1_800),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "At the festival, the rain verse! Take a coral lamp, dry-air friend. It glows the colour of her singing voice, we think.",
                "Every festival, Merrow City sings about rain on a roof. None of them has felt rain. All of them cry at the last line.",
            ),
            choice(
                "Keep the page for the archive",
                &[ArcEffect::Reputation(8), ArcEffect::Credits(900)],
                "In the archive, then, under glass. Safe. Quiet. She would have wanted it sung, I think, but safe is good too.",
                "Sefa's rain verse rests under glass in the merrow archive, read in a whisper and never sung.",
            ),
            give_up(&[], "The rain verse stays under the canister, unsung."),
        ],
        pitch: voices(
            &["Sefa's book had a second verse. We do not. Find it."],
            &[
                "Sefa-of-the-dry-air wrote our songs in her book, and there was a second verse we never got! Will you find it for us?",
            ],
            &[
                "There is a hole in our oldest song where a verse should be. Sefa kept it. Would you look for it at her camp?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Merrow City's archivist wants Sefa's missing verse.",
            "Find the page at Sefa's camp and bring it to the singers.",
            "The verse was about rain.",
        ),
    },
    ArcTemplate {
        id: ArcId(1432),
        title: "Pearls for the Bubble",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Merrow City's air bubble is held by seals of pressed pearl, and one is weeping. The dock keeper needs fresh pearls from the beds below, and the beds are guarded by things with teeth.",
        twist: "The scan of the pearl beds shows the city's oldest seal was set by the builders, and it is the one holding fastest.",
        consequence: "The bubble holds, the seal is renewed, and the dock keeper carves the rig's name on the new pearl.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::MerrowCity)],
        roles: &[Role::Quartermaster, Role::Trader],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Deliver {
                item: QuestItem::Material(StrategicResourceKind::PressurePearl),
                count: 4,
            }],
        ],
        beats: &[
            "Dive to the pearl beds",
            "Read the old seals",
            "Bring four pearls",
        ],
        steps: &[
            step(
                "{Giver} says the best pearl beds lie at {dest}, {where}. Get down there.",
                "The beds are deep and the sharks are rude. Go quick, come back quicker.",
            ),
            step(
                "Pulse the scanner over the beds to read how the city's seals were made.",
                "Our seals are pearls pressed under the sea. Your machine can see how. We only feel it.",
            ),
            step(
                "The oldest seal is builder work and still sound. Bring four pressure pearls to renew the weeping one.",
                "Four pearls. Not three, three is unlucky. Not five, five is showing off.",
            ),
        ],
        climax: "Under the scanner the pearl beds glow, and among them the city's first seal shows clear: pressed by the Tall Ones themselves, and still holding the sea out.",
        twists: &[],
        reward: reward(3_000, &[(StrategicResourceKind::PressurePearl, 1)], 15),
        outcomes: &[
            ending(
                "Carve the rig's name on the seal",
                &[
                    ArcEffect::Reputation(15),
                    ArcEffect::Furniture(Furniture::PearlMirror),
                    ArcEffect::Credits(2_000),
                    ArcEffect::Settlement(5),
                    ArcEffect::Remember,
                ],
                "Your name, on the new seal, where the sea can read it. And this pearl mirror, so you can see who saved us.",
                "A new seal holds Merrow City's bubble, carved with the name of the rig that brought its pearls.",
            ),
            choice(
                "Carve the Fathers' name instead",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Credits(1_500),
                    ArcEffect::Settlement(6),
                ],
                "The Fathers' name? For Sefa. Yes. The whole city will want to touch it.",
                "The bubble's new seal reads THE FATHERS' LOAD. Merrow children touch it for luck on the way to the archive.",
            ),
            give_up(&[], "The seal keeps weeping, and the dock keeper mops."),
        ],
        pitch: voices(
            &["A seal is weeping. Pearls from the beds fix it. You have a hull. Go."],
            &[
                "One of the seals that keeps our air in is weeping! Will you dive to the pearl beds and bring us fresh pearls?",
            ],
            &[
                "Every night I listen to the weeping seal and count the drops. Would you bring pearls before I run out of numbers?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Merrow City's dock keeper needs pearls for a weeping seal.",
            "Dive to the pearl beds, scan the seals, bring four pressure pearls.",
            "The oldest seal was builder work.",
        ),
    },
    ArcTemplate {
        id: ArcId(1433),
        title: "A Jar of Rain",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "A merrow child once wore Amos Varga's hat, which smelled of smoke. Sefa told her the world above is mostly rain. She has asked every dry-air visitor since to bring her some.",
        twist: "Up top it is raining when the rig surfaces, as if the town knew.",
        consequence: "A jar of the town's rain stands on a shelf in Merrow City, and the child charges a shell to look at it.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::MerrowCity)],
        roles: &[Role::Apprentice],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::ReturnToSurface],
            &[Objective::HandDeliver {
                target: 0,
                parcel: "a jar of rain",
            }],
        ],
        beats: &["Go up for rain", "Carry it down by hand"],
        steps: &[
            step(
                "{Giver} wants a jar of real rain from the world above. Take the rig all the way up to the surface.",
                "Is it really falling up there? From the sky? On purpose? Go and look, go and look!",
            ),
            step(
                "You filled a jar with the town's rain. Carry it down to {village} and hand it over yourself.",
                "Don't spill it! If you spill it, it's just water. If you don't, it's rain.",
            ),
        ],
        climax: "Up top the sky is grey and it is raining, soft and steady, on the town's roofs. You hold the jar out until it is full.",
        twists: &[],
        reward: reward(2_400, &[], 14),
        outcomes: &[
            ending(
                "Put it on the shelf",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::Furniture(Furniture::KelpPlanter),
                    ArcEffect::Credits(1_200),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Rain! Real rain! It looks like sea that forgot how to be salty. Take my kelp bowl. I don't need it now, I have rain.",
                "A jar of the town's rain stands on a shelf in Merrow City. The child charges one shell to look at it and two to hold it.",
            ),
            choice(
                "Pour it out over the city",
                &[ArcEffect::Reputation(9), ArcEffect::Settlement(5)],
                "You poured it? From up high? It fell on everyone! It was raining in Merrow City! Do it again! No, don't, it's gone. Oh, it was perfect.",
                "For one minute it rained in Merrow City. The merrow still talk about it, and look up hopefully at festivals.",
            ),
            give_up(
                &[],
                "No rain comes down, and the child goes on asking every dry-air visitor.",
            ),
        ],
        pitch: voices(
            &["Sefa said up there is mostly rain. I want some. In a jar. Please."],
            &[
                "Amos let me wear his hat and Sefa said up top is mostly rain! Will you bring me a jar of real rain? Please, please?",
            ],
            &[
                "I dream about water falling from the sky, soft, on people who aren't wet already. Would you bring me a little?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A merrow child wants a jar of rain from the world above.",
            "Go up to the surface, then carry the jar down to Merrow City by hand.",
            "It was raining in town.",
        ),
    },
    // --- The Sunfolk Kraal (Descent V, the Hollow World) --------------------------------------
    ArcTemplate {
        id: ArcId(1441),
        title: "Bran's Garden",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Ruth Brannigan kept a garden of medicine plants by her camp, and the Kraal's healer wants seed from it to plant a new one under the inner sun.",
        twist: "Bran's old garden has run wild into a meadow, every plant labelled in her hand on a scrap of sled board: FEVER. COUGH. SAD.",
        consequence: "A new medicine garden grows by the Kraal, and the healer keeps Bran's labels, SAD included.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::JungleTribes)],
        roles: &[Role::Healer, Role::Farmer],
        slots: &[],
        targets: &[
            place(StoryPlace::FathersSite),
            place(StoryPlace::DeepSeam),
            place(StoryPlace::Home),
        ],
        stages: &[
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Dig {
                target: 1,
                count: 10,
            }],
            &[Objective::Speak { target: 2 }],
        ],
        beats: &[
            "Gather seed on foot",
            "Break new ground",
            "Plant with the healer",
        ],
        steps: &[
            step(
                "{Giver} wants seed from Bran's old garden by {dest}, {where}. Walk in and gather it.",
                "Bran said plants are lazy medicine. They do the work while you sleep.",
            ),
            step(
                "The garden ran wild, labelled in Bran's hand. Now break new ground for it at {dest2}, {where2}.",
                "Soft soil, deep roots. Dig like you mean it. Bran always dug like she meant it.",
            ),
            meet(
                "The ground is ready. Bring the seed and the labels to {contact} at the Kraal.",
                "The children are waiting with water pots. They have been waiting since sunrise.",
                "FEVER, COUGH, SAD. She had a plant for sad. Of course she did. We will plant that one in the middle, where everyone can reach it.",
            ),
        ],
        climax: "Bran's garden has run wild into a meadow by the camp, and every clump still wears its label on a scrap of sled board: FEVER. COUGH. BURNS. SAD.",
        twists: &[],
        reward: reward(2_600, &[(StrategicResourceKind::RadiantFossil, 2)], 16),
        outcomes: &[
            ending(
                "Plant SAD in the middle",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Furniture(Furniture::Planter),
                    ArcEffect::Credits(1_600),
                    ArcEffect::Settlement(5),
                    ArcEffect::Remember,
                ],
                "In the middle, where everyone can reach. Take a planter of our own, sky-friend, and grow something for sad at home.",
                "A new medicine garden grows by the Sunfolk Kraal. In the middle grows the plant Bran labelled SAD, and it is picked often.",
            ),
            choice(
                "Leave her garden wild",
                &[ArcEffect::Reputation(8), ArcEffect::Credits(800)],
                "Wild is how she grows now. Yes. We will walk out to it instead. It is a good walk.",
                "Bran's garden stays wild by the camp, and the Kraal's healer walks out to it every morning.",
            ),
            give_up(
                &[],
                "No new garden, and the healer walks a long way for every fever.",
            ),
        ],
        pitch: voices(
            &["Bran kept a garden of healing plants. I want its seed. You can fly there."],
            &[
                "Grandmother Bran grew medicine plants by her old camp! Will you bring me seed to plant a new garden at the Kraal?",
            ],
            &[
                "Bran's hands shook at the end, so I learned her plants by heart. Would you bring their seed home to us?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Sunfolk Kraal's healer wants seed from Bran's old garden.",
            "Gather seed on foot at the camp, break ground, plant with the healer.",
            "Every plant was labelled, even SAD.",
        ),
    },
    ArcTemplate {
        id: ArcId(1442),
        title: "The Other Half of the Board",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "THE FATHERS' LOAD hangs in the Kraal's long-house on half a sled board. The hunters want the other half, lost with the rig a beast stepped on.",
        twist: "Under the crushed rig's name plate is a second plank Bran painted on the sly: AND THE SUNFOLK. She meant to add them all along.",
        consequence: "The long-house board is whole again, and reads THE FATHERS' LOAD AND THE SUNFOLK.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::JungleTribes)],
        roles: &[Role::Warden, Role::Prospector],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::PickUp {
                target: 0,
                parcel: "the rest of the sled board",
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Find the crushed rig",
            "Pry the board loose",
            "Hang it in the long-house",
        ],
        steps: &[
            step(
                "The hunter {giver} says the crushed rig lies at {dest}, {where}. Find it.",
                "A beast as big as a hill stepped on it. Not angry. Just big. Look for a flat place.",
            ),
            step(
                "The rig is flat as a leaf. Pry the rest of the board loose from under the name plate.",
                "Gently. Bran painted it. Paint remembers who held the brush.",
            ),
            meet(
                "There is a second plank, painted in secret. Bring it all to {contact} at the Kraal.",
                "The long-house is waiting. The children have made a space on the wall with their hands.",
                "AND THE SUNFOLK. She was going to put us on the sled. She was going to take us with them. Hang it, hang it now, I can't see for crying.",
            ),
        ],
        climax: "Under the crushed name plate is a second plank, painted in Bran's careful letters and hidden face down: AND THE SUNFOLK.",
        twists: &[],
        reward: reward(2_900, &[(StrategicResourceKind::AncientAlloy, 2)], 16),
        outcomes: &[
            ending(
                "Hang the whole board",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Furniture(Furniture::Banner),
                    ArcEffect::Credits(1_700),
                    ArcEffect::Settlement(6),
                    ArcEffect::Remember,
                ],
                "Whole again. Take the old banner from the long-house, sky-friend. It was waiting for the board to come home too.",
                "In the Sunfolk Kraal's long-house the board reads THE FATHERS' LOAD AND THE SUNFOLK, and the children read it aloud.",
            ),
            choice(
                "Keep the secret plank for the chief",
                &[ArcEffect::Reputation(9), ArcEffect::Credits(1_200)],
                "For Ruth-Little only? Hm. She will keep it by her bed. That is a good place for a secret.",
                "The board in the long-house is whole. The second plank hangs over the chief's bed, where only she reads it.",
            ),
            give_up(
                &[],
                "Half a board hangs in the long-house, and the hunters still wonder.",
            ),
        ],
        pitch: voices(
            &[
                "Half the Fathers' board hangs in our long-house. The other half is under a flat rig. Get it.",
            ],
            &[
                "We have half of the Fathers' painted board! Will you find the crushed rig and bring the other half home?",
            ],
            &[
                "Half a name hangs on our wall, and half a name is a sad thing. Would you bring the rest of it home?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Sunfolk hunters want the rest of the Fathers' sled board.",
            "Find the crushed rig, pry the board loose, hang it in the long-house.",
            "Bran had added the Sunfolk in secret.",
        ),
    },
    // --- Parallax Station (Descent VI, the Strange Layers) ------------------------------------
    ArcTemplate {
        id: ArcId(1451),
        title: "Two Clocks",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Parallax Station's glass cutter keeps two clocks and believes neither. She wants a rig to sit in a pocket of loose time and bring back a true reading.",
        twist: "The reading says the pocket runs one heartbeat slow for every fast one: time down there is not broken, it is breathing.",
        consequence: "Parallax Station sets its clocks by the breathing, and the glass cutter finally keeps one.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::PrismRelay)],
        roles: &[Role::Tinkerer, Role::Smith],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 40,
            }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Sit in the loose time",
            "Take a true reading",
            "Set the station's clocks",
        ],
        steps: &[
            step(
                "{Giver} wants a reading from the loose time at {dest}, {where}. Sit there, rig still, and let it pass.",
                "Don't count. Counting makes it worse. Hum, if you must. Varga hummed.",
            ),
            step(
                "Now pulse the scanner and take the reading while you're still inside.",
                "Quick, before it notices you're measuring it.",
            ),
            meet(
                "Time in the pocket breathes: one slow beat, one fast. Tell {contact} at {village}.",
                "Well? Which clock? The left one? Tell me it's the left one.",
                "It breathes? Slow, fast, slow, fast, like a sleeper. Then it isn't broken. Then I can stop being afraid of it. I'll keep one clock now. The left one.",
            ),
        ],
        climax: "The scanner's trace rises and falls like breath: one beat slow, one beat fast, steady as a sleeper. The time down here is not broken. It is breathing.",
        twists: &[],
        reward: reward(3_000, &[(StrategicResourceKind::VoidGlass, 2)], 17),
        outcomes: &[
            ending(
                "Keep one clock",
                &[
                    ArcEffect::Reputation(17),
                    ArcEffect::Furniture(Furniture::Clock),
                    ArcEffect::Credits(2_200),
                    ArcEffect::Remember,
                ],
                "One clock! Take the other, it's wrong anyway, beautifully wrong. Hang it where you can laugh at it.",
                "Parallax Station keeps time by the breathing of the loose pockets. The glass cutter keeps one clock, and trusts it.",
            ),
            choice(
                "Keep both clocks",
                &[ArcEffect::Reputation(8), ArcEffect::Credits(1_400)],
                "Both? Ha. Wise. One for the breath in, one for the breath out.",
                "Parallax Station still keeps two clocks. Now everyone knows why.",
            ),
            give_up(&[], "The glass cutter goes on believing neither clock."),
        ],
        pitch: voices(
            &[
                "I keep two clocks. Neither is right. Sit in the loose time and tell me which is less wrong.",
            ],
            &[
                "I keep two clocks because time pools out there! Will you sit in a pocket of it and bring me a true reading?",
            ],
            &[
                "I lie awake listening to my two clocks disagree. Would you go where time is loose and ask it which is right?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Parallax Station's glass cutter wants a true reading of loose time.",
            "Sit in the time pocket, scan it, then report back.",
            "Time down there breathes.",
        ),
    },
    ArcTemplate {
        id: ArcId(1452),
        title: "The Real Canister",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "A drifter swears a second Fathers' canister has appeared beside the real one, a mirror twin that keys to nothing. The relay keeper wants it checked before someone opens the wrong one.",
        twist: "Read in order, the true canister answers; the twin flickers, writes the same log backwards, and is gone.",
        consequence: "The relay keeper chalks a rule by the door: read in order. The false one keys to nothing.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::PrismRelay)],
        roles: &[Role::Quartermaster, Role::Courier],
        slots: &[],
        targets: &[place(StoryPlace::Home), place(StoryPlace::FathersSite)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[
                Objective::Reach { target: 1 },
                Objective::Scan { target: 1 },
            ],
        ],
        beats: &["Hear the drifter out", "Check the canisters"],
        steps: &[
            meet(
                "{Giver} wants the twin canister looked into. First hear the story from {contact} at {village}.",
                "Ask the drifter. Drifters see everything twice. Usually they're drunk. This time I'm not sure.",
                "Two canisters, same crate, same scratch on the lid, one of them backwards. I didn't touch either. My mother didn't raise a fool, just a drifter.",
            ),
            step(
                "Ride to {dest2}, {where2}, and pulse the scanner over both canisters.",
                "Scan them both. The true one keys to the log before. The false one keys to nothing.",
            ),
        ],
        climax: "The scanner keys the true canister to the log before it. The twin beside it shivers, writes the same words backwards across its lid, and is gone.",
        twists: &[],
        reward: reward(2_800, &[(StrategicResourceKind::CrystalLens, 2)], 15),
        outcomes: &[
            ending(
                "Chalk the rule by the door",
                &[
                    ArcEffect::Reputation(15),
                    ArcEffect::Furniture(Furniture::Mirror),
                    ArcEffect::Credits(1_800),
                    ArcEffect::Remember,
                ],
                "Read in order. That's going by the door in big letters. And take this mirror, I can't look at it any more.",
                "By Parallax Station's door, in big chalk letters: READ IN ORDER. THE FALSE ONE KEYS TO NOTHING.",
            ),
            choice(
                "Tell the drifter it was nothing",
                &[ArcEffect::Reputation(7), ArcEffect::Credits(1_000)],
                "Nothing? Good. Good. I'll tell them it was the drink. It's usually the drink.",
                "The drifter believes it was the drink. The keeper knows better and keeps an eye on the crate.",
            ),
            give_up(
                &[],
                "Two canisters wait on the crate, and nobody dares to open either.",
            ),
        ],
        pitch: voices(
            &[
                "Drifter says there's a twin canister at the Fathers' camp. Check it before someone opens the wrong one.",
            ],
            &[
                "A drifter swears there are two Fathers' canisters now, one a mirror twin! Will you check which one is real?",
            ],
            &[
                "Something on the trail has copied itself, and copies are cruel down here. Would you find the true canister?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Parallax Station's keeper wants a twin canister checked.",
            "Hear the drifter, then scan both canisters at the Fathers' camp.",
            "The true one keyed to the log before; the twin vanished.",
        ),
    },
    // --- The Megacity outpost (Descent VII, the Ancients) -------------------------------------
    ArcTemplate {
        id: ArcId(1461),
        title: "The Last Tram",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The Megacity's trams still run, empty, on a timetable an age old. The tram driver rides the last one every night, and wants someone to wait at the stop where Ines Calloway once got on.",
        twist: "The last tram stops for the rig, opens its doors and announces, in the builders' tongue and then in Calloway's voice: Mind the gap. Go on.",
        consequence: "The tram driver keeps a seat on the last tram for the rig, and the archivist adds a new stop to the timetable.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::ArchivePost)],
        roles: &[Role::Courier, Role::Mechanic],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 30,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Find the old stop",
            "Wait for the last tram",
            "Tell the driver",
        ],
        steps: &[
            step(
                "{Giver} says Calloway's stop lies at {dest}, {where}, under an age of dust. Get there.",
                "The stop has a bench and a lamp. The lamp comes on when someone waits. Wait.",
            ),
            step(
                "This is the stop. Keep the rig still and wait for the last tram.",
                "It's never late. It's never early. It's the only thing down here that keeps time.",
            ),
            meet(
                "The tram stopped for you and spoke in Calloway's voice. Tell {contact} at {village}.",
                "Did it stop? It never stops for me. I ride it, but it never stops for me.",
                "Mind the gap, go on? In her voice? Then she's on the timetable now. I'll keep you a seat. Second from the front, by the window.",
            ),
        ],
        climax: "Out of the dark comes the last tram, lit and empty. It stops. Its doors slide open, and it speaks: a builder chime, then a woman's dry voice. Mind the gap. Go on.",
        twists: &[],
        reward: reward(3_100, &[(StrategicResourceKind::AncientAlloy, 3)], 18),
        outcomes: &[
            ending(
                "Keep a seat on the last tram",
                &[
                    ArcEffect::Reputation(18),
                    ArcEffect::Furniture(Furniture::Chandelier),
                    ArcEffect::Credits(3_000),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Your seat's kept. And take the lamp from the old stop's ceiling, it's been waiting for someone to notice it.",
                "On the Megacity's last tram one seat is always kept, second from the front, by the window.",
            ),
            choice(
                "Let the tram run on empty",
                &[ArcEffect::Reputation(9), ArcEffect::Credits(2_000)],
                "Empty, then. It likes empty, I think. It's had a long time to get used to it.",
                "The last tram runs on empty through the Megacity, on time, every night.",
            ),
            give_up(
                &[],
                "Nobody waits at the old stop, and the last tram passes it by.",
            ),
        ],
        pitch: voices(
            &["Calloway rode the last tram once. Wait at her stop. See if it stops."],
            &[
                "The trams still run on the old timetable! Will you wait at the stop where Ines Calloway got on and see if it stops?",
            ],
            &[
                "Every night I ride the last tram past a stop with a bench and a lamp, and nobody waiting. Would you wait there?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Megacity's tram driver wants someone to wait at Calloway's stop.",
            "Reach the old stop, wait for the last tram, tell the driver.",
            "The tram spoke in Calloway's voice.",
        ),
    },
    ArcTemplate {
        id: ArcId(1462),
        title: "I Looked Up Every Night",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "Years ago Wren Varga came as far as the Megacity asking after her father, and left a note at the outpost for him. The keeper wants it carried down the Fathers' trail and left where Amos will have passed.",
        twist: "In the camp's canister, under Amos's log, is a pressed flower from the town and a scrap of paper: FOR WREN, IF SHE COMES.",
        consequence: "Wren's note rests in her father's canister, and his flower goes up the cable to the town.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::ArchivePost)],
        roles: &[Role::Quartermaster, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Home), place(StoryPlace::FathersSite)],
        stages: &[
            &[Objective::PickUp {
                target: 0,
                parcel: "Wren's note",
            }],
            &[Objective::OnFoot { target: 1 }],
        ],
        beats: &["Take Wren's note", "Leave it on the trail"],
        steps: &[
            step(
                "{Giver} has kept Wren Varga's note for years. Take it from the outpost's shelf in {village}.",
                "Careful with it. It's been read by every keeper since. We all cried. Keepers are soft.",
            ),
            step(
                "Carry the note on foot into {dest2}, {where2}, and leave it with the canister.",
                "On foot. A daughter's note shouldn't arrive in a drill.",
            ),
        ],
        climax: "You lift the canister's lid to set Wren's note inside, and find, under Amos's log, a pressed yellow flower from the town and a scrap of paper: FOR WREN, IF SHE COMES.",
        twists: &[],
        reward: reward(3_000, &[], 18),
        outcomes: &[
            ending(
                "Send his flower up the cable",
                &[
                    ArcEffect::Reputation(18),
                    ArcEffect::Furniture(Furniture::Painting),
                    ArcEffect::Credits(2_800),
                    ArcEffect::Settlement(5),
                    ArcEffect::Remember,
                ],
                "His flower, going home. I'll send it up with the next lift. Take the painting of the town from my wall. She painted it, you know. Wren.",
                "Wren Varga's note rests in her father's canister. His pressed flower went up the cable to a porch in the town.",
            ),
            choice(
                "Leave both in the canister",
                &[ArcEffect::Reputation(10), ArcEffect::Credits(2_000)],
                "Together. A note and a flower in one tin. That's a family, that is.",
                "In a canister on the Fathers' trail a daughter's note and a father's flower lie side by side.",
            ),
            give_up(
                &[],
                "Wren's note stays on the outpost's shelf, read by every keeper.",
            ),
        ],
        pitch: voices(
            &["Wren Varga left a note for her father. Take it down the trail where he passed."],
            &[
                "Amos Varga's daughter came this far once and left him a note! Will you carry it down the trail for her?",
            ],
            &[
                "There is a note on my shelf that says I looked up every night, and it has waited long enough. Would you take it down?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Megacity's keeper wants Wren Varga's note carried down the trail.",
            "Take the note from the outpost and leave it at the Fathers' camp on foot.",
            "Amos had left a flower for Wren.",
        ),
    },
    ArcTemplate {
        id: ArcId(1463),
        title: "Coordinates for the Archivist",
        kind: ArcKind::Deep,
        contact_other: false,
        hook: "The Megacity's scholar has been asked by the archivist, very politely, for a fresh reading of the star chart vault below the outpost. It has waited an age and would like to check its sums.",
        twist: "The vault's chart has a new star marked on it since the archive was last read: the rig's own lamp, logged as the first ship in an age.",
        consequence: "The archivist logs the rig as a ship, and the scholar copies its first coordinates into the outpost's book.",
        tier: QuestTier::Deep,
        prerequisites: &[],
        triggers: &[Trigger::Settlement(Settlement::ArchivePost)],
        roles: &[Role::Scholar, Role::Cartographer],
        slots: &[],
        targets: &[place(StoryPlace::DeepSeam), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 14,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Read the chart vault",
            "Open the vault's face",
            "Bring the sums back",
        ],
        steps: &[
            step(
                "{Giver} has the archivist's request: a fresh reading of the chart vault at {dest}, {where}. Pulse the scanner over it.",
                "It asked so nicely. Nobody's asked me anything nicely in years.",
            ),
            step(
                "The vault's face is sealed in rubble. Dig it open so the chart can be read by eye.",
                "Mind the chart. The archivist says it is the only map of the roads between the suns. No pressure.",
            ),
            meet(
                "On the chart, a new star: the rig's lamp, logged as a ship. Tell {contact} at {village}.",
                "Well? Do its sums add up? It gets fretful when its sums don't add up.",
                "A ship? It logged your rig as a ship? Then the roads on that chart are for us now. I'll copy the first coordinates in the outpost book tonight.",
            ),
        ],
        climax: "Behind the rubble the chart vault glows: suns and roads between them, and near the bottom a new mark, bright and small, labelled in the builders' script. The archivist translates: FIRST SHIP IN AN AGE.",
        twists: &[],
        reward: reward(
            3_200,
            &[
                (StrategicResourceKind::AncientAlloy, 2),
                (StrategicResourceKind::CrystalLens, 2),
            ],
            18,
        ),
        outcomes: &[
            ending(
                "Log the rig as a ship",
                &[
                    ArcEffect::Reputation(18),
                    ArcEffect::Furniture(Furniture::StarMobile),
                    ArcEffect::Credits(3_200),
                    ArcEffect::Remember,
                ],
                "A ship! Take the star mobile from the reading room, it turns toward the first road on the chart. We checked.",
                "The archivist logs the rig as the first ship in an age, and the outpost's book holds its first coordinates.",
            ),
            choice(
                "Ask it to wait",
                &[ArcEffect::Reputation(9), ArcEffect::Credits(1_800)],
                "Wait? It's very good at waiting. It says thank you for asking. It says that a lot.",
                "The archivist keeps the new star unlogged, patiently, until the rig is ready to be a ship.",
            ),
            give_up(
                &[],
                "The chart vault stays sealed, and the archivist politely asks again.",
            ),
        ],
        pitch: voices(
            &["The archivist wants its star chart read again. It asked nicely. Go."],
            &[
                "The archivist asked me for a fresh reading of the star chart vault! Will you scan it and dig it open for us?",
            ],
            &[
                "A machine that has waited an age asked me, so politely, to check its sums. Would you read its star chart for it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Megacity's scholar has the archivist's request for a chart reading.",
            "Scan the chart vault, dig its face open, bring the sums back.",
            "The rig was logged as a ship.",
        ),
    },
];
