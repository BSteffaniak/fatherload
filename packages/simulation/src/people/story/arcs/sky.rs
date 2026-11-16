//! Batch: sky arcs (Act II). Kites, weather, lost mail and the great lands past the dead air.
//! Every target is a story place with a fallback (the highest ridge near the giver when a world
//! has no sky), so the text names places through `{dest}` and `{where}`. Ids 301-308.

use super::{NO_THANKS, choice, ending, give_up, log, meet, place, step, voices};
use crate::{
    cosmetics::{CosmeticItem, Decal, FlameColor, HullPattern},
    people::{
        character::Role,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Trigger},
        traits::{Fear, Quirk, Value, Want},
    },
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace, UpgradeStat},
    terrain::StrategicResourceKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(301),
        title: "Kite Strings",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A farmer's prize kite, loaded with seed packets for a contest, snapped its string and landed somewhere high. Fly up and fetch it.",
        twist: "The seed packets burst on landing. By the time you arrive, there's a tiny vegetable garden growing where the kite came down.",
        consequence: "The farmer wins the contest with a photograph of the highest garden in the world.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Role(Role::Farmer),
            Trigger::Value(Value::Wonder),
            Trigger::Want(Want::SeeTheWorld),
        ],
        roles: &[Role::Farmer, Role::Apprentice],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Fly up to the kite",
            "Take a reading of the garden",
            "Bring the news down",
        ],
        steps: &[
            step(
                "{Giver}'s prize kite snapped its string and came down on {dest}, {where}. Go and get it.",
                "It's yellow with a red tail. Very hard to miss. Unless it's in a cloud. Then very easy to miss.",
            ),
            step(
                "The kite's here, and so is something else: little green shoots, in neat rows, where the seed packets burst. Pulse the scanner so {giver} believes you.",
                "Shoots? GROWING? Up there? Take a reading! The judges will want proof!",
            ),
            step(
                "You have the kite and a scan of the highest garden in the world. Bring them down.",
                "Bring it down gently! The kite, I mean. And the garden. Well, the scan of the garden.",
            ),
        ],
        climax: "In the scanner's glow: twelve tiny rows of carrots and beans, sprouting on the edge of the sky, the kite draped over them like a tent.",
        twists: &[],
        reward: QuestReward::credits(1_100, 13)
            .materials(&[(StrategicResourceKind::CrystalLens, 1)]),
        outcomes: &[
            ending(
                "Show them the garden scan",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
                "Carrots! In the SKY! The judges will faint. I'm going to win. I'm going to win so hard.",
                "{Giver} won the contest with the scan of the sky garden. The judges asked for a carrot. Nobody has been brave enough to fly up and pick one.",
            ),
            choice(
                "Leave the kite to shelter the shoots",
                &[ArcEffect::Reputation(8), ArcEffect::Mood(20)],
                "Leave it as a tent for them? ...Yes. That's right. It's their kite now.",
                "The yellow kite still flaps over the sky garden, keeping the frost off. Passing rigs tip their lamps to it.",
            ),
            give_up(
                &[],
                "The kite is still up there somewhere, and so is a little garden nobody tends.",
            ),
        ],
        pitch: voices(
            &[
                "My contest kite snapped its string. It's up on {dest}, {where}. Fetch it. There's seed in it.",
            ],
            &[
                "My prize kite flew away! It's on {dest}, {where}, with all my contest seeds aboard! Could you fly up and fetch it? Please?",
            ],
            &[
                "My kite went up and didn't come down. It's resting on {dest}, {where}, with a season's worth of seed. Would you climb the sky for it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A farmer's contest kite came down somewhere high, full of seed packets.",
            "Reach the kite, scan what's growing there, and bring the news down.",
            "The kite's seeds sprouted. A garden grows in the sky.",
        ),
    },
    ArcTemplate {
        id: ArcId(302),
        title: "The Weather Up There",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A weather-watcher wants readings from somewhere high: hold still up there long enough for the instruments to settle.",
        twist: "The readings predict a storm on the giver's birthday, which they take personally.",
        consequence: "The giver's forecasts are the best in the valley, and they move their party indoors.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Quirk(Quirk::WeatherWatcher),
            Trigger::Value(Value::Knowledge),
            Trigger::Role(Role::Scholar),
        ],
        roles: &[Role::Scholar, Role::Cartographer],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 25,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Climb to the station",
            "Let the needles settle",
            "Bring the readings down",
        ],
        steps: &[
            step(
                "{Giver} needs weather readings from {dest}, {where}. Get up there.",
                "The higher the better. Weather starts up there and comes down here to annoy us.",
            ),
            step(
                "Hold still. The instruments need the wind to tell them its whole story.",
                "Twenty-five seconds. Don't fidget. The barometer can tell when you fidget.",
            ),
            step(
                "Readings taken. They don't look good for next week. Take them down to {giver}.",
                "Well? What does it say? No, don't read it on the radio. I want to see the needles.",
            ),
        ],
        climax: "The needles settle and the forecast prints itself out in neat purple ink: STORM, BIG ONE, NEXT SIXTHDAY. Next sixthday is {giver}'s birthday.",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Hand over the forecast",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "A storm. On my BIRTHDAY. Typical. The sky's had it in for me since I was six. ...Thank you, though. Truly.",
                "{Giver}'s forecasts are the best for a hundred leagues. Their birthday was moved indoors. The storm arrived exactly on time, and they said 'I told you so' to it personally.",
            ),
            choice(
                "Fudge the forecast to 'mild'",
                &[
                    ArcEffect::Reputation(-6),
                    ArcEffect::Mood(15),
                    ArcEffect::Credits(80),
                ],
                "Mild? Lovely! Garden party it is! ...Why are you making that face?",
                "The garden party was rained out spectacularly. {Giver} suspects you. {Giver} is right.",
            ),
            give_up(
                &[],
                "{Giver} goes back to licking a finger and holding it up. It's about as accurate.",
            ),
        ],
        pitch: voices(
            &[
                "Need weather readings from {dest}, {where}. Sit up there till the needles settle. Bring them down.",
            ],
            &[
                "I predict the weather and I need readings from way up at {dest}! Could you fly my instruments up and sit with them a bit? It's fun! Mostly windy!",
            ],
            &[
                "The weather is born up high, {where}. I'd like to hear it before it reaches us. Would you sit with my instruments on {dest}?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A weather-watcher wants readings from somewhere high.",
            "Reach the place, hold still while the needles settle, and bring the readings down.",
            "The forecast says storm. On their birthday.",
        ),
    },
    ArcTemplate {
        id: ArcId(303),
        title: "A View for Someone Who Can't",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "The giver is terrified of heights but has always wanted to see the world from up there. Go up, look properly, and come back to describe it.",
        twist: "Describing it doesn't cure their fear. It does make them start painting, and the paintings are wonderful.",
        consequence: "The giver's paintings of a place they've never been hang all over the village.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Fear(Fear::Heights),
            Trigger::Value(Value::Wonder),
            Trigger::Want(Want::SeeTheWorld),
        ],
        roles: &[Role::Storyteller, Role::ShrineKeeper],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 1 }],
        ],
        beats: &["Go up and look", "Take it all in", "Come back and tell it"],
        steps: &[
            step(
                "{Giver} has never been higher than a stepladder and never will. They've asked you to go to {dest}, {where}, and look.",
                "Look properly. Not glancing. Looking. Like you'll be tested on it. You will.",
            ),
            step(
                "Turn slowly. Take it in. Pulse the scanner so you remember the shape of everything.",
                "What colour is the ground from up there? Tell me later. Tell me everything later.",
            ),
            step(
                "You've seen it. Now carry it home to {giver} in {village}, all of it.",
                "Are you coming? I've got paper. I've got so much paper.",
            ),
        ],
        climax: "From {dest} the world is a quilt: the trail a thread, the villages buttons, the clouds below you like sheep that wandered off a cliff and decided to stay.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Describe every detail",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(30),
                    ArcEffect::Cosmetic(CosmeticItem::Decal(Decal::Wings)),
                    ArcEffect::Remember,
                ],
                "Sheep that decided to stay. Yes. YES. Hold still, I need to paint that before it leaves my head.",
                "{Giver}'s paintings of the view from {dest} hang in every house in {village}. They've never been up there. They never will. They got the clouds exactly right.",
            ),
            choice(
                "Offer to take them up, very slowly",
                &[
                    ArcEffect::Reputation(6),
                    ArcEffect::Mood(-5),
                    ArcEffect::FollowUp(ArcId(305)),
                ],
                "Me? Up THERE? ...Maybe. Maybe someday. Ask me again when I've painted it a hundred times.",
                "{Giver} hasn't said yes. They haven't said no, either. The paintings are getting higher.",
            ),
            give_up(
                &[],
                "{Giver} still wonders what the world looks like from up there, and paints it badly from guesswork.",
            ),
        ],
        pitch: voices(
            &[
                "Can't do heights. Never could. Go up to {dest}, look properly, come back and tell me. All of it.",
            ],
            &[
                "I've always wanted to see the world from {dest} but I'm scared stiff of heights! Would you go up and tell me what it's like? Every detail!",
            ],
            &[
                "I will never stand {where}. My knees won't let me. Would you go, and bring the view back for me? I'll know what to do with it.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone afraid of heights wants to know what the world looks like from up high.",
            "Reach the high place, scan it, and come back to describe it.",
            "The view came home and became paintings.",
        ),
    },
    ArcTemplate {
        id: ArcId(304),
        title: "Airmail",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A courier's mail balloon slipped its tether and drifted up. The mailbag is stuck somewhere high.",
        twist: "Among the letters is one addressed to you, in a hand you don't recognise. It just says 'Keep going up.'",
        consequence: "The mail gets delivered, late but intact, and you have a mystery in your pocket.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 4)],
        triggers: &[
            Trigger::Role(Role::Courier),
            Trigger::Quirk(Quirk::MistrustsBirds),
            Trigger::Value(Value::Honesty),
        ],
        roles: &[Role::Courier, Role::Trader],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland), place(StoryPlace::Village)],
        stages: &[
            &[Objective::PickUp {
                target: 0,
                parcel: "mailbag",
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Rescue the mailbag", "Deliver the mail"],
        steps: &[
            step(
                "{Giver}'s mail balloon drifted off and snagged on {dest}, {where}. Pick up the mailbag.",
                "The bag's red canvas with a brass clasp. If a bird's sitting on it, don't negotiate. Birds lie.",
            ),
            meet(
                "Half the bag is for {dest2}. Hand it to {contact} there, who sorts the post.",
                "Give it to {contact}. They sort the post and they never read anyone's mail. Much.",
                "The lost mailbag! Oh, there'll be dancing. Half the village has been waiting on letters. There's even... huh. There's one in here for you.",
            ),
        ],
        climax: "The letter addressed to you is thin, unsigned, the ink silvery. It says only: 'Keep going up. The Moon is listening.'",
        twists: &[],
        reward: QuestReward::credits(1_150, 12),
        outcomes: &[
            ending(
                "Deliver every letter",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Every letter delivered! And one for YOU? From who? ...The Moon is listening? Well, that's not ominous at all.",
                "The mail got through. In your pocket is a letter on silvery paper: 'Keep going up. The Moon is listening.' Nobody knows who wrote it.",
            ),
            choice(
                "Ask the courier who sent your letter",
                &[ArcEffect::Reputation(6), ArcEffect::FollowUp(ArcId(403))],
                "Who sent it? No idea. It wasn't in the bag when I packed it. ...Now that's a story. I'll ask around.",
                "{Giver} is asking every courier on the road about the silvery letter. They've promised to tell you the moment they hear anything.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "The mailbag swings from the sky, full of letters nobody will read.",
            ),
        ],
        pitch: voices(
            &[
                "Mail balloon got loose. Bag's stuck on {dest}, {where}. Fetch it. People are waiting on letters.",
            ],
            &[
                "My mail balloon flew off with a whole bag of post! It's snagged on {dest}, {where}. Could you rescue it? People are waiting!",
            ],
            &[
                "A whole village of letters is caught on {dest}, {where}, flapping in the wind. Every one of them is someone waiting. Would you bring them down?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A courier's mailbag drifted up and snagged somewhere high.",
            "Pick up the mailbag, then deliver it to the next village.",
            "The mail got through. One letter was for you.",
        ),
    },
    ArcTemplate {
        id: ArcId(305),
        title: "Two Islands, One Shadow",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A shrine keeper says two sky places line up at noon, and their shadows spell something on the ground. Visit both.",
        twist: "From the second one, you see the first's shadow falling across the ground below: it's the shape of a key.",
        consequence: "The shrine keeper paints the key over the shrine door, and wonders aloud what it opens.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 6)],
        triggers: &[
            Trigger::Role(Role::ShrineKeeper),
            Trigger::Quirk(Quirk::Superstitious),
            Trigger::Want(Want::UncoverDeepSecret),
        ],
        roles: &[Role::ShrineKeeper, Role::Hermit],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland), place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::Scan { target: 1 }],
        ],
        beats: &["The first island", "The second island", "Read the shadow"],
        steps: &[
            step(
                "{Giver} believes two sky places line up at noon. Start with {dest}, {where}.",
                "The first one is the 'lock'. Don't ask me why. The old songs say so.",
            ),
            step(
                "Now the second: {dest2}, {where2}. From there you should see the first one's shadow on the ground.",
                "The second one is the 'eye'. From the eye, you see what the lock is hiding.",
            ),
            step(
                "Look down. Pulse the scanner to fix the shadow's shape before the sun moves.",
                "Quick, before noon slides away! What shape is it?",
            ),
        ],
        climax: "Far below, the first island's shadow lies across the hills, and with the scanner's lines drawn over it the shape is unmistakable: a key, its teeth pointing straight down into the earth.",
        twists: &[],
        reward: QuestReward::credits(2_200, 16).materials(&[(StrategicResourceKind::VoidGlass, 2)]),
        outcomes: &[
            ending(
                "Tell them it's a key",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(402)),
                ],
                "A key. Pointing DOWN. The songs had it backwards. It's not the sky that's locked. It's the deep.",
                "A key is painted over {village}'s shrine door now, teeth pointing down. {Giver} has started asking the deep miners some very odd questions.",
            ),
            choice(
                "Say it looked more like a spoon",
                &[ArcEffect::Reputation(-4), ArcEffect::Credits(250)],
                "A spoon? ...A SPOON? All that, for a spoon? Well, I suppose the songs never said it would be useful.",
                "A spoon is painted over {village}'s shrine door. Visitors find it charming. You feel a little guilty every time you see it.",
            ),
            give_up(
                &[],
                "The islands keep lining up at noon with nobody watching. Whatever the shadow says, it says it to the hills.",
            ),
        ],
        pitch: voices(
            &[
                "Two sky places line up at noon. Their shadow spells something. Visit both. Read it. I'm too old for flying.",
            ],
            &[
                "The old songs say two sky islands line up at noon and their shadow spells a secret! Visit {dest} and then the other one? I'll make tea for when you're back!",
            ],
            &[
                "At noon, the old songs say, two islands become one sentence. Start at {dest}. Read me what the sky has been trying to say.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A shrine keeper says two sky places cast a shadow that spells something at noon.",
            "Reach both places and scan the shadow from the second.",
            "The shadow is a key, pointing down.",
        ),
    },
    ArcTemplate {
        id: ArcId(306),
        title: "The Flag on the Far Land",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A cartographer has dreamed all their life of mapping the great land past the dead air. Fly there, climb out and walk its ground.",
        twist: "Someone was there before: a weathered flag with the town's old crest, planted long before anyone thought it possible.",
        consequence: "The cartographer redraws the history of the town, and the first map of the far land bears your name.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 11)],
        triggers: &[
            Trigger::Role(Role::Cartographer),
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Value(Value::Adventure),
        ],
        roles: &[Role::Cartographer, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::SkyContinent)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Cross the dead air",
            "Walk the far land",
            "Bring the map home",
        ],
        steps: &[
            step(
                "{Giver} has dreamed of the far land all their life. It's {dest}, {where}. Cross the dead air and get there.",
                "Full tank. Spare thoughts. Bring a sandwich. People forget the sandwich.",
            ),
            step(
                "You made it. Climb out and walk the ground with your own boots: maps made from a cockpit lie.",
                "Walk it! Feel the ground! Is it springy? I've always imagined it springy.",
            ),
            step(
                "There's a flag up here, old and weathered, with the town's old crest. Bring the story down.",
                "A flag? Whose flag? WHOSE? Get down here this instant and tell me.",
            ),
        ],
        climax: "On a rise in the grass stands a pole with the ragged remains of a flag: the town's old crest, a pick crossed with a feather, faded almost white. Someone stood here before the town had a name.",
        twists: &[],
        reward: QuestReward::credits(3_000, 18).materials(&[(StrategicResourceKind::VoidGlass, 3)]),
        outcomes: &[
            ending(
                "Bring back the first map",
                &[
                    ArcEffect::Reputation(15),
                    ArcEffect::Cosmetic(CosmeticItem::Flame(FlameColor::Aether)),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "The old crest. Up THERE. Someone from here made it, long ago, and never told a soul. Every history book in town is wrong. Isn't it wonderful?",
                "The first map of the far land hangs in {village}'s hall with your name in the corner. Underneath, {giver} added a small drawing of an old flag and a question mark.",
            ),
            choice(
                "Plant a new flag beside the old one",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Cosmetic(CosmeticItem::Decal(Decal::Flag)),
                ],
                "A new flag beside the old? Two firsts, a lifetime apart. Yes. I like that very much.",
                "Two flags fly on the far land now, an old one and a new one. {Giver} has a painting of them both over the fireplace.",
            ),
            give_up(
                &[],
                "The far land stays unmapped. {Giver} keeps dreaming of it, and the old flag keeps fading.",
            ),
        ],
        pitch: voices(
            &[
                "The far land. Past the dead air. I've wanted it mapped since I was a child. You've got the tank. Go. Walk it. Tell me.",
            ],
            &[
                "You've got the fuel for it now! The great land past the dead air, {dest}! Fly there, walk it, and bring me the first real map! I've waited my whole life!",
            ],
            &[
                "Past the dead air there's a land nobody's walked. I've drawn it a thousand times from guesses. Would you go and make my guesses into a map?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A cartographer dreams of mapping the great land past the dead air.",
            "Reach the land, walk it on foot, and bring the map home.",
            "Someone planted a flag on the far land long ago.",
        ),
    },
    ArcTemplate {
        id: ArcId(307),
        title: "Thin Air",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A healer wants to study how the air thins up high, for their patients with weak lungs. Sit in the thin air near the great land and let the instruments breathe.",
        twist: "The thinnest air has a faint, sweet smell, like hay and rain. The healer bottles it for homesick patients.",
        consequence: "Little jars of 'sky air' sit by the beds of the sick, and somehow they help.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 10)],
        triggers: &[
            Trigger::Role(Role::Healer),
            Trigger::Value(Value::Knowledge),
            Trigger::Fear(Fear::LosingFamily),
        ],
        roles: &[Role::Healer, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::SkyContinent)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 30,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Breathe the thin air", "Bring the jars home"],
        steps: &[
            step(
                "{Giver} wants samples of the thinnest air, near {dest}, {where}. Sit in it with the jars open.",
                "Thirty seconds, jars open. Watch your hull. Watch your head. Thin air makes people giddy.",
            ),
            step(
                "The jars are sealed. They smell faintly of hay and rain. Bring them down to {giver}.",
                "Careful with the jars. Air's lighter than it looks, but jars aren't.",
            ),
        ],
        climax: "You crack one jar a hair to check the seal. It smells of hay and summer rain and something old and clean, like the inside of a bell.",
        twists: &[],
        reward: QuestReward::credits(2_400, 16),
        outcomes: &[
            ending(
                "Deliver the jars",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(4),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Hay and rain. Oh. That's what my patients keep asking for, the ones who've been inside too long. Sky air. I can give them sky air.",
                "A little jar of sky air sits beside every sickbed in {village}. Patients open them for a sniff when they're homesick. Nobody can explain why it helps. It helps.",
            ),
            choice(
                "Keep one jar for yourself",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(200)],
                "One for you? Of course. Everyone gets homesick. Even people who live in rigs.",
                "A jar of sky air rides in your rig. On long nights deep underground, you open it, just a crack.",
            ),
            give_up(
                &[],
                "{Giver} goes on guessing at the thin air from books, and opening windows for patients who miss the sky.",
            ),
        ],
        pitch: voices(
            &[
                "Need air samples from the thinnest air there is, near {dest}. For patients. Sit in it with jars. Bring them back.",
            ],
            &[
                "I want to bottle the thinnest air in the sky, up near {dest}! It might help my patients with weak lungs. Would you sit up there with my jars?",
            ],
            &[
                "Some of my patients have forgotten what open sky feels like. Far up, near {dest}, the air is thin and clean. Would you bottle some for them?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A healer wants samples of the thinnest air, high near the great land.",
            "Hold still in the thin air with the jars, then bring them down unbroken.",
            "Sky air sits by every sickbed now.",
        ),
    },
    ArcTemplate {
        id: ArcId(308),
        title: "The Smuggler's Cloud",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A smuggler hid a crate up high and can't get it back. They'd like it dug out, no questions asked.",
        twist: "The crate is full of seed potatoes and old toys: the smuggler has been running gifts to a struggling village for years.",
        consequence: "Either the gifts get through, or the warden gets a very confusing report.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 4)],
        triggers: &[
            Trigger::Role(Role::Courier),
            Trigger::Value(Value::Freedom),
            Trigger::Value(Value::Community),
        ],
        roles: &[Role::Courier, Role::Trader],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland), place(StoryPlace::Village)],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 6,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Dig out the crate", "Deliver it, quietly"],
        steps: &[
            step(
                "{Giver} stashed a crate on {dest}, {where}, and a rockslide buried it. Dig it out. Don't open it.",
                "Don't open it. I mean it. It's not dangerous. It's private.",
            ),
            meet(
                "The crate's out. {Giver} wants it taken to {contact} in {dest2}. Don't ask.",
                "Give it to {contact}. They'll know what to do. Say it's from 'a friend'.",
                "From 'a friend', eh? It's always from a friend. ...Seed potatoes and toys again. Every winter. Tell your friend the children say thank you.",
            ),
        ],
        climax: "{Contact} opens the crate: seed potatoes in straw, a wooden horse, a rag doll, three tin whistles. 'Every winter,' they say. 'Every winter for six years.'",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Keep the smuggler's secret",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Cosmetic(CosmeticItem::Pattern(HullPattern::Starfield)),
                    ArcEffect::Remember,
                ],
                "They said thank you? The children? ...Don't tell anyone. It ruins the reputation.",
                "{Giver} still runs crates through the sky every winter. You know what's in them now. You've started slipping a toy in yourself.",
            ),
            choice(
                "Tell the village who's been sending gifts",
                &[
                    ArcEffect::Reputation(-6),
                    ArcEffect::Settlement(5),
                    ArcEffect::Prosperity(1),
                ],
                "You TOLD them? ...Now they want to throw me a dinner. A dinner! Do you know what that does to a smuggler's standing?",
                "{Dest2} threw {giver} a dinner. {Giver} grumbled through every course and wiped their eyes during the speeches.",
            ),
            give_up(
                &[],
                "The crate stays buried on the island. Somewhere, a village has a lean winter.",
            ),
        ],
        pitch: voices(
            &[
                "I hid a crate on {dest}. Rockslide buried it. Dig it out, take it where I say, no questions.",
            ],
            &[
                "Bit of a delicate one! A crate of mine got buried on {dest}, {where}. Dig it out and deliver it? Don't open it! It's a surprise. For someone.",
            ],
            &[
                "There's a crate on {dest} that has to reach someone before winter. I can't say what's in it. Would you trust me, just this once?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A smuggler's crate is buried somewhere high.",
            "Dig out the crate and deliver it to the next village.",
            "The crate was full of gifts.",
        ),
    },
];
