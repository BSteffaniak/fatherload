//! Batch: tombs, crypts and ruins (Wave 6). Stories bound to the sealed sites of V10 worlds
//! (side-view tombs and top-down crypts, [`StoryPlace::Tomb`] / [`StoryPlace::Crypt`]) and the
//! lever ruins of the pilot spaces ([`StoryPlace::Ruin`]), each with a fallback (a sealed vault,
//! a derelict, a cavern) on older worlds. Some endings break a seal
//! ([`ArcEffect::UnsealTomb`]): the story step a story-sealed site waits for. Ids 901-908.

use super::{NO_THANKS, choice, ending, give_up, log, meet, place, step, voices};
use crate::{
    home::Furniture,
    people::{
        character::{AbsentFate, Role},
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger},
        traits::{Fear, Quirk, Value, Want},
    },
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace},
    terrain::StrategicResourceKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(901),
        title: "The Tale on the Seal",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "A scholar has spent years on a rubbing of a tomb seal and can't read the last line. Go and read it off the stone itself.",
        twist: "The last line isn't a curse or a riddle. It's a lullaby, the same one the scholar's grandmother used to sing.",
        consequence: "The seal answers the song, and a tomb that stayed shut for a thousand years opens for whoever comes next.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(20)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Role(Role::Storyteller),
            Trigger::Value(Value::Knowledge),
        ],
        roles: &[Role::Scholar, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::Tomb), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Find the sealed door",
            "Read the seal's last line",
            "Bring the reading home",
        ],
        steps: &[
            step(
                "{Giver} has a rubbing of a tomb seal with one line they can't make out. The tomb is {dest}, {where}.",
                "The rubbing smudged right at the end. Of course it did. The best part always smudges.",
            ),
            step(
                "You're at the seal. Pulse the scanner over the carving so the worn last line shows up.",
                "Slowly over the bottom edge, please. The carvers always crammed the last line in.",
            ),
            meet(
                "You have the whole seal. {Giver} wants {contact} to hear it first: they knew the old songs.",
                "Take it to {contact}. If anyone in {village} can tell what that line means, it's them.",
                "Read it again? ...Oh. Oh, I know this. 'Hush now, the lamps are low, the stone will keep you.' My mother sang it. Her mother sang it. It was carved on a tomb?",
            ),
        ],
        climax: "Under the scanner's glow the worn line surfaces, letter by letter, and it isn't a warning at all. It's a lullaby, four lines long, about lamps and stone and sleep.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Sing the seal its song",
                &[
                    ArcEffect::UnsealTomb,
                    ArcEffect::Reputation(12),
                    ArcEffect::Furniture(Furniture::StarChart),
                    ArcEffect::Remember,
                ],
                "A lullaby. All those years I was looking for a curse. Sing it to the seal, then. Gently. And take my old star chart: you've earned the sky as well as the stone.",
                "They say the tomb's seal opened to a lullaby. {Giver} has added a verse to the rubbing on their wall, in their own handwriting, so it never smudges again.",
            ),
            choice(
                "Keep the song between you",
                &[ArcEffect::Reputation(8), ArcEffect::Mood(15)],
                "You're right. Some songs aren't for opening doors. Let it sleep.",
                "The tomb stays sealed. {Giver} hums the lullaby while they work, and won't say where they learned it.",
            ),
            give_up(&[], "The last line stays smudged. {Giver} keeps guessing."),
        ],
        pitch: voices(
            &["Tomb seal. One line I can't read. Go read it off the stone and bring it back."],
            &[
                "Years I've worked on this seal rubbing and the last line beats me every time! Would you go read it off the real stone? Please?",
            ],
            &[
                "There's a seal carved with a tale, and the tale's last line has worn away on my rubbing. I lie awake wondering how it ends. Would you find out?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A scholar can't read the last line of a tomb seal.",
            "Read the seal with the scanner and take the reading home.",
            "The seal's last line was a lullaby.",
        ),
    },
    ArcTemplate {
        id: ArcId(902),
        title: "A Lamp for the Builders",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "An old custom says a lamp left burning at a crypt's door all night keeps the builders' rest easy. Nobody has done it in a generation.",
        twist: "When the night is darkest, the crypt's own carvings glow back at the lamp, as if someone inside lit one in return.",
        consequence: "The custom comes back, and a lamp burns at the crypt door on the longest nights.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(25)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Role(Role::ShrineKeeper),
            Trigger::Quirk(Quirk::Superstitious),
        ],
        roles: &[Role::ShrineKeeper, Role::Hermit],
        slots: &[],
        targets: &[place(StoryPlace::Crypt)],
        stages: &[
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Vigil {
                target: 0,
                seconds: 30,
            }],
        ],
        beats: &[
            "Carry the lamp to the door",
            "Keep it burning through the dark",
        ],
        steps: &[
            step(
                "Climb out and carry {giver}'s lamp to the crypt door yourself: {dest}, {where}. It has to be set down by hand.",
                "By hand, mind. Not from a rig window. The builders were people, they'd want to see a face.",
            ),
            step(
                "The lamp is lit at the door. Keep watch beside it after dark so it doesn't go out.",
                "Trim the wick at midnight. And don't talk to anything that answers. Probably nothing will.",
            ),
        ],
        climax: "At the darkest hour the carvings around the crypt door begin to glow, faint and amber, line by line, as if a lamp had been lit on the other side to answer yours.",
        twists: &[],
        reward: QuestReward::credits(800, 11),
        outcomes: &[
            ending(
                "Leave the lamp burning",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Settlement(3),
                    ArcEffect::Furniture(Furniture::GlowcapLamp),
                    ArcEffect::Remember,
                ],
                "They answered. A generation of nobody bothering, and they still answered. Here: a lamp for your own door, so you never come home to the dark.",
                "Every year now, on the longest night, someone from {village} carries a lamp to the crypt door. The carvings always glow back.",
            ),
            choice(
                "Open the shutters and look inside",
                &[ArcEffect::UnsealTomb, ArcEffect::Reputation(4)],
                "You looked? Well. If they lit a lamp, I suppose they were expecting a visitor.",
                "The crypt's seal gave way the night of the lamp. Some say the builders were waiting for someone polite enough to knock.",
            ),
            give_up(
                &[],
                "The lamp went home unlit. The custom waits another generation.",
            ),
        ],
        pitch: voices(
            &[
                "Old custom. A lamp at the crypt door, all night, for the builders. Nobody's done it in years. You'll do it.",
            ],
            &[
                "There's a lovely old custom nobody keeps anymore: a lamp at the crypt door all night, for the people who built it! Will you carry one out with me?",
            ],
            &[
                "My grandmother left a lamp at the crypt door every long night, for the builders' sleep. Since she went, no one has. Would you take it for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old custom asks for a lamp at a crypt door all night.",
            "Carry the lamp on foot to the crypt door, then keep watch after dark.",
            "The crypt's carvings glowed back.",
        ),
    },
    ArcTemplate {
        id: ArcId(903),
        title: "Grave Goods",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "A carved urn was taken from a tomb's doorway generations ago and passed round the valley as a lucky charm. Its luck has run out. Put it back.",
        twist: "The urn's maker's name is still legible on the base, and their descendants live one village over.",
        consequence: "The urn goes home to the family, who carry it back to the tomb themselves.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(15)],
        triggers: &[
            Trigger::Want(Want::StrikeItRich),
            Trigger::Value(Value::Honesty),
            Trigger::Fear(Fear::GoingBroke),
        ],
        roles: &[Role::Trader, Role::RetiredMiner],
        slots: &[],
        targets: &[place(StoryPlace::Tomb), place(StoryPlace::Village)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::PickUp {
                target: 0,
                parcel: "carved urn",
            }],
            &[Objective::HandDeliver {
                target: 1,
                parcel: "carved urn",
            }],
        ],
        beats: &[
            "Take the urn to the tomb",
            "Find the maker's mark",
            "Carry it to the maker's family",
        ],
        steps: &[
            step(
                "{Giver} has the tomb's lucky urn and wants it back where it came from: {dest}, {where}.",
                "Bad luck since the spring. Bad luck all summer. It's the urn. I'm almost sure it's the urn.",
            ),
            step(
                "At the doorway the urn's base catches the light: a maker's mark. Pick it up again, carefully. It isn't the tomb's to keep after all.",
                "A name? On the bottom? Fifty years it's been on my mantel and I never turned it over.",
            ),
            step(
                "The mark is a family name still found in {dest2}, {where2}. Climb out and carry the urn to their door by hand.",
                "Don't post it through the window. Knock. Families like a knock.",
            ),
        ],
        climax: "The old woman at the door turns the urn over, reads the mark, and laughs until she cries. 'Great-great-grandfather's,' she says. 'He always said it'd find its way home.'",
        twists: &[],
        reward: QuestReward::credits(850, 12),
        outcomes: &[
            ending(
                "Let the family take it back",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
                "Home to the family that made it. That's better than any tomb, isn't it. I feel lucky already. Lighter, anyway.",
                "The family carried the urn back to the tomb door themselves, singing. {Giver}'s luck turned the next week, or so they insist.",
            ),
            choice(
                "Ask for a little of the luck back",
                &[ArcEffect::Credits(250), ArcEffect::Reputation(3)],
                "They paid you? For bringing back their own urn? ...Well, it was a long trip. I suppose.",
                "The family pressed a purse on you at the door. {Giver} tells the story with a slight frown at that part.",
            ),
            give_up(
                &[],
                "The urn sits on {giver}'s mantel, turned to face the wall.",
            ),
        ],
        pitch: voices(
            &[
                "Took an urn from a tomb door, years back. Lucky charm. It's not lucky now. Put it back.",
            ],
            &[
                "I've got a confession: the lucky urn on my mantel came off a tomb doorway! My luck's gone sour. Would you take it home for me?",
            ],
            &[
                "There's an urn on my mantel that was never mine. It came from a tomb, long ago. Lately I feel it watching. Would you carry it back?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A lucky urn taken from a tomb has run out of luck.",
            "Take the urn to the tomb, then carry it by hand to its maker's family.",
            "The urn went home to the family that made it.",
        ),
    },
    ArcTemplate {
        id: ArcId(904),
        title: "Mapping the Crypt",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "A cartographer wants the first proper map of a crypt's outer halls, and the readings only come clear if someone stays inside long enough.",
        twist: "The readings show the crypt's halls are laid out as a map themselves: of the valley, as it was a thousand years ago.",
        consequence: "The cartographer publishes two maps at once, and the old one is better.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(30)],
        triggers: &[
            Trigger::Role(Role::Cartographer),
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Quirk(Quirk::NamesTheirTools),
        ],
        roles: &[Role::Cartographer, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::Crypt)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 25,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Take a first reading",
            "Hold still while it resolves",
            "Bring the map up",
        ],
        steps: &[
            step(
                "{Giver} wants a map of the crypt's outer halls: {dest}, {where}. Start with a scanner pulse at the entrance.",
                "One pulse to start. The crypt will tell you how big it is. Crypts love telling you that.",
            ),
            step(
                "The reading is muddy. Stay put by the crypt while it resolves, however long it takes.",
                "Don't fidget. Cartography is ninety percent standing still and ten percent being right.",
            ),
            step(
                "The map has resolved, and it's stranger than {giver} expected. Bring it up to the surface.",
                "Strange how? Don't tell me on the radio. Bring it up. I want to see your face when you say it.",
            ),
        ],
        climax: "Line by line the crypt's halls appear on the display, and the shape is familiar: the river's old course, the hills, a lake where the town stands now. The crypt is a map of the valley, a thousand years old.",
        twists: &[],
        reward: QuestReward::credits(950, 12).materials(&[(StrategicResourceKind::CrystalLens, 1)]),
        outcomes: &[
            ending(
                "Publish both maps",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "A map in the shape of a map. They drew the valley in stone and buried it. Both go in the atlas, side by side. And my good charts are yours to buy, any time.",
                "{Giver}'s atlas now opens with the crypt's map. Most readers prefer the old valley. {Giver} pretends to be hurt.",
            ),
            choice(
                "Keep the old map secret",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(200)],
                "Secret? ...I suppose some places should stay where only we can find them. Here's for your trouble.",
                "Only you and {giver} know what the crypt's halls really draw. Sometimes you catch them smiling at the old lake bed.",
            ),
            give_up(
                &[],
                "The crypt stays unmapped. {Giver} draws a question mark on the atlas.",
            ),
        ],
        pitch: voices(
            &["Need a map of a crypt. Readings only settle if you sit there. You sit, I draw."],
            &[
                "I want to make the first real map of a crypt! The scanner needs someone to stay put inside while it reads. Interested?",
            ],
            &[
                "There's a crypt nobody has ever charted. The halls only show themselves to someone patient. Would you be patient for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A cartographer wants the first map of a crypt.",
            "Scan the crypt, hold out while the reading resolves, then surface.",
            "The crypt is laid out as a map of the old valley.",
        ),
    },
    ArcTemplate {
        id: ArcId(905),
        title: "What the Prospector Saw",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "A wandering prospector swears they watched a tomb's door glow from the inside. The giver wants someone sober to go and look.",
        twist: "The prospector was right, and the glow is a story carved for travellers: a welcome, not a warning.",
        consequence: "The tomb's seal yields to the first traveller who reads its welcome aloud.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(15)],
        triggers: &[
            Trigger::Quirk(Quirk::TallTales),
            Trigger::Role(Role::Prospector),
            Trigger::Fear(Fear::DeepThings),
        ],
        roles: &[Role::Prospector, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Tomb), place(StoryPlace::Camp)],
        stages: &[
            &[Objective::Speak { target: 1 }],
            &[Objective::OnFoot { target: 0 }],
        ],
        beats: &["Hear the prospector out", "Walk up to the door yourself"],
        steps: &[
            meet(
                "First hear the tale from the source: {contact}, out at {dest2}, {where2}.",
                "They tell it different every time. Get the version before the second drink.",
                "Glowing, I tell you. From the INSIDE. Like a hearth behind the door. I didn't go closer. I'm not a fool. Well. Not that kind.",
            ),
            step(
                "Climb out and walk right up to the tomb door, {dest}, {where}. See for yourself.",
                "On foot, so you can see the carvings close. And so you can run, if it comes to that.",
            ),
        ],
        climax: "Up close, the glow is in the carving itself: a figure at a door with a lamp, and a line of old script around it. Your helmet lamp catches it and it reads, as near as you can tell, 'Come in from the cold.'",
        twists: &[],
        reward: QuestReward::credits(700, 10),
        outcomes: &[
            ending(
                "Read the welcome aloud",
                &[
                    ArcEffect::UnsealTomb,
                    ArcEffect::Reputation(10),
                    ArcEffect::Remember,
                ],
                "A welcome! All this time we were scared of a tomb that was inviting us in. Well, that's one tall tale that turned out shorter than the truth.",
                "The tomb opened to a welcome read aloud. The prospector tells the story at every campfire now, and for once nobody calls it a tall tale.",
            ),
            choice(
                "Tell them it was swamp gas",
                &[ArcEffect::Reputation(-2), ArcEffect::Credits(60)],
                "Swamp gas? On a mesa? ...If you say so. Here, for your trouble.",
                "Officially, it was swamp gas. The prospector, offended, moved their camp two valleys over.",
            ),
            give_up(
                &[],
                "Nobody went to look. The glowing door stays a campfire story.",
            ),
        ],
        pitch: voices(
            &["A prospector says a tomb door glows from inside. Probably drink. Go see."],
            &[
                "A wandering prospector swears a tomb door was glowing from the inside! Will you go and check? I'm dying to know if it's true!",
            ],
            &[
                "Someone out on the trail saw light behind a tomb door. Light, where no one has been for a thousand years. Would you look, so I needn't?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A prospector swears a tomb door glows from inside.",
            "Hear the prospector out, then walk to the tomb door on foot.",
            "The glow was a carved welcome for travellers.",
        ),
    },
    ArcTemplate {
        id: ArcId(906),
        title: "The Guardian's Name",
        kind: ArcKind::Tomb,
        contact_other: false,
        hook: "A deep scholar believes every tomb guardian once had a name, carved somewhere near the door, and that saying it calms them.",
        twist: "The name is scratched under the threshold in a child's hand: the builders' children named their guardian like a pet.",
        consequence: "The guardian has a name again, and the scholar has the warmest footnote in the history of tombs.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Depth(60)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Role(Role::Hermit),
        ],
        roles: &[Role::Hermit, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::Tomb), place(StoryPlace::Hall)],
        stages: &[
            &[Objective::Speak { target: 1 }],
            &[Objective::Dig {
                target: 0,
                count: 10,
            }],
            &[Objective::Vigil {
                target: 0,
                seconds: 25,
            }],
        ],
        beats: &[
            "Ask the deepfolk",
            "Clear the threshold",
            "Say the name after dark",
        ],
        steps: &[
            meet(
                "{Giver} says the deepfolk know where guardians' names were carved. Ask {contact} at {dest2}, {where2}.",
                "The deepfolk remember things the surface forgot on purpose. Ask nicely. Bring a lamp.",
                "Names? Under the threshold, always. Low, where the builders' children could reach. Look low, surface friend.",
            ),
            step(
                "Clear the rubble from around the tomb's threshold, {dest}, {where}, and look low.",
                "Gently with the drill. A name scratched by a child won't survive enthusiasm.",
            ),
            step(
                "There it is, scratched in a child's hand. Wait by the door after dark and say it once.",
                "After dark, when the tomb is listening. Say it like you'd call a dog in from the rain.",
            ),
        ],
        climax: "You say the name, softly, the way a child would. Deep behind the door something heavy shifts, settles, and is still, like an old dog lying back down by a fire.",
        twists: &[],
        reward: QuestReward::credits(2_200, 16)
            .materials(&[(StrategicResourceKind::CrystalLens, 2)]),
        outcomes: &[
            ending(
                "Keep the name safe",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Mood(20),
                    ArcEffect::Furniture(Furniture::Moonflower),
                    ArcEffect::Remember,
                ],
                "A pet's name. The builders' children named the thing that guards their parents. I'll write it down and keep it safe. And this moonflower is for you: it only opens in the dark, like the best secrets.",
                "{Giver}'s history of tombs has one warm footnote: the guardian's name, and the drawing of a child's scratch beside it.",
            ),
            choice(
                "Tell every tomb-runner the name",
                &[ArcEffect::Reputation(8), ArcEffect::Settlement(4)],
                "Everyone? Well. If it makes the guardian gentler with them, the builders' children would approve.",
                "Tomb-runners whisper the name at the door now, for luck. Some swear it works.",
            ),
            give_up(&[], "The guardian's name stays under the rubble, unspoken."),
        ],
        pitch: voices(
            &["Every tomb guardian had a name once. Carved near the door. Find it. Say it."],
            &[
                "I have a theory: every tomb guardian had a name, and saying it calms them! Want to help me prove it?",
            ],
            &[
                "Somewhere near each tomb's door, the builders carved their guardian's name. Imagine being guarded by someone nobody remembers. Would you find it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A deep scholar believes tomb guardians had names.",
            "Ask the deepfolk, clear the threshold, then say the name after dark.",
            "The guardian's name was scratched by a child.",
        ),
    },
    ArcTemplate {
        id: ArcId(907),
        title: "The Lever Room",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "A tinkerer is obsessed with the old ruins' lever gates and wants someone small enough to crawl in and read the mechanism.",
        twist: "The levers aren't locks at all. They're a musical instrument, and the right order plays a tune.",
        consequence: "The tinkerer builds a copy, and the village dances to a thousand-year-old song.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(30)],
        triggers: &[
            Trigger::Want(Want::BuildInvention),
            Trigger::Role(Role::Tinkerer),
            Trigger::Quirk(Quirk::TalksToMachines),
        ],
        roles: &[Role::Tinkerer, Role::Mechanic],
        slots: &[],
        targets: &[place(StoryPlace::Ruin)],
        stages: &[
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Crawl into the ruin",
            "Read the lever mechanism",
            "Bring the drawings up",
        ],
        steps: &[
            step(
                "Climb out and crawl through the ruin's grate on foot: {dest}, {where}. The rig won't fit.",
                "Elbows in, chin down. Ruins hate rigs. Ruins quite like pilots.",
            ),
            step(
                "You're inside. Pulse the scanner across the lever gate so {giver} can see how it's built.",
                "All of it. The linkages, the counterweights, the little bits you think don't matter. They matter.",
            ),
            step(
                "The scan is strange: tuned rods behind every lever. Bring it up to the surface.",
                "Tuned? Like a fork? Come up, come up, I need to see this before I lose my mind.",
            ),
        ],
        climax: "On the scan, behind every lever, hangs a tuned metal rod. Pull them in the gate's order and they'd ring out a tune. The ruin's lock is a song.",
        twists: &[],
        reward: QuestReward::credits(1_000, 13),
        outcomes: &[
            ending(
                "Build the lever organ",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Settlement(3),
                    ArcEffect::Furniture(Furniture::Gramophone),
                    ArcEffect::Remember,
                ],
                "A lock that's a song! I've built a copy with fence posts. It's terrible. Everyone loves it. Here, take my gramophone: I won't need it, I have an ORGAN now.",
                "{Village} has a lever organ in the square, built from fence posts and stubbornness. It plays one thousand-year-old song, badly, every evening.",
            ),
            choice(
                "Sell the drawings to the guild",
                &[ArcEffect::Credits(300), ArcEffect::Reputation(4)],
                "The guild? They'll use it for a doorbell. ...Fine. It's your scan.",
                "The guild's doorbell plays a thousand-year-old song now. {Giver} winces every time they visit.",
            ),
            give_up(
                &[],
                "The levers stay a puzzle. {Giver} keeps sketching guesses.",
            ),
        ],
        pitch: voices(
            &["Lever gates in the old ruins. Need the mechanism read. Rig won't fit. You will."],
            &[
                "The old ruins' lever gates are the cleverest machines I've ever seen and I can't fit inside! Would you crawl in and scan one for me?",
            ],
            &[
                "Deep in the old ruins there are levers no one living understands. Machines talk, if you listen. Would you listen for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A tinkerer wants the old ruins' lever gates read.",
            "Crawl into the ruin on foot, scan the levers and surface.",
            "The ruin's lock is a song.",
        ),
    },
    ArcTemplate {
        id: ArcId(908),
        title: "Rubbings from the Ruin",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A relative went into the old ruins years ago and came out with a sketchbook they never showed anyone. The giver wants the walls they drew.",
        twist: "They didn't draw the walls. They drew their family, over and over, on every page, so they wouldn't forget them in the dark.",
        consequence: "The giver finally understands why their relative came back quieter, and frames the drawings.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(25)],
        triggers: &[
            Trigger::Absent(AbsentFate::WentDeep),
            Trigger::Want(Want::FindRelative),
            Trigger::Value(Value::Family),
        ],
        roles: &[Role::Farmer, Role::Baker],
        slots: &[Slot::Absent(AbsentFate::WentDeep)],
        targets: &[place(StoryPlace::Ruin), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 8,
            }],
            &[Objective::HandDeliver {
                target: 1,
                parcel: "wall rubbings",
            }],
        ],
        beats: &[
            "Find the ruin",
            "Uncover the walls",
            "Carry the rubbings home",
        ],
        steps: &[
            step(
                "{Focus}, {giver}'s {focus_relation}, came back from the old ruin with a sketchbook nobody was allowed to open. Find the ruin: {dest}, {where}.",
                "Went down cheerful, came up quiet. I want to see what was down there.",
            ),
            step(
                "Clear the rubble around the ruin's outer wall so the carvings show, and take rubbings.",
                "Whatever's carved there, I want it. Every line. Something down there got drawn.",
            ),
            step(
                "The walls are bare. Whatever {focus} drew, it wasn't these. Carry the rubbings home to {giver} by hand anyway.",
                "Bare? Then what did he... bring them anyway. Bring them to the door.",
            ),
        ],
        climax: "The ruin's walls are blank. Whatever {focus} sat in the dark drawing, it wasn't carvings. {Giver} goes very quiet, then fetches the sketchbook from under the bed.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Open the sketchbook together",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "Faces. Page after page of us. {Focus} drew us so as not to forget us down there. ...I never knew. Thank you for making me look.",
                "The sketchbook's pages hang framed in {giver}'s hallway now: the whole family, drawn by lamplight in a ruin by someone who missed them.",
            ),
            choice(
                "Leave the sketchbook closed",
                &[ArcEffect::Reputation(6)],
                "You're right. If {focus} wanted it shown, it would have been shown. Some books stay shut.",
                "The sketchbook stays under {giver}'s bed. Now and then they rest a hand on its cover, and don't open it.",
            ),
            give_up(&[], "{Giver} never learned what {focus} drew in the dark."),
        ],
        pitch: voices(
            &[
                "My {focus_relation} {focus} drew something in the old ruins. Never showed a soul. Go see the walls.",
            ],
            &[
                "My {focus_relation} {focus} came back from the old ruins with a sketchbook and kept it shut for years! Would you find out what was in it?",
            ],
            &[
                "{Focus} went into the old ruins laughing and came out silent, with a sketchbook held shut. I need to know what he saw.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A relative drew something in the old ruins and never showed anyone.",
            "Find the ruin, uncover its walls and carry the rubbings home by hand.",
            "They drew their family, so as not to forget them.",
        ),
    },
];
