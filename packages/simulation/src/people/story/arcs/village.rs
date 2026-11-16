//! Batch: village arcs. Letters, sweethearts, feuds, missing couriers and market days: stories
//! that send you between settlements to talk to real people. Ids 201-210.

use super::{NO_THANKS, choice, ending, feature, give_up, log, meet, place, poi, step, voices};
use crate::{
    people::{
        character::{AbsentFate, Role},
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger, VillageProblem},
        traits::{Fear, Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace},
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(201),
        title: "Letters Up the Trail",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The giver writes letters to a pen friend in the next village and the courier's laid up. Carry one over and bring the reply.",
        twist: "The reply is three pages long and almost entirely about turnips. The giver is delighted.",
        consequence: "The letters keep flowing, and the two villages start a turnip show.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::WritesLetters),
            Trigger::Value(Value::Community),
            Trigger::Role(Role::Courier),
        ],
        roles: &[Role::Courier, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::Village), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Reach { target: 1 }],
        ],
        beats: &["Deliver the letter", "Bring back the reply"],
        steps: &[
            meet(
                "{Giver} has a letter for their pen friend {contact} in {dest}, {where}. The courier's laid up. You aren't.",
                "Hand it to {contact} yourself. Not the inn. The inn reads everything.",
                "A letter! From {giver}? Oh, wonderful. Wait right there, I'll write back. It might take a moment. I have news.",
            ),
            step(
                "{Contact} wrote back, at length. Carry the reply home to {giver} in {village}.",
                "Is it a long one? Tell me it's a long one.",
            ),
        ],
        climax: "{Contact}'s reply is three pages, both sides. It opens with 'Dearest {giver}' and is then, as far as you can tell, entirely about turnips.",
        twists: &[],
        reward: QuestReward::credits(240, 8),
        outcomes: &[
            ending(
                "Hand over the reply",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Mood(20),
                    ArcEffect::Remember,
                ],
                "Three pages! About turnips! Oh, they've had a marvellous season. Listen to this bit...",
                "{Giver} read the whole letter aloud at the inn. Twice. {Village} and {dest} hold a turnip show every spring now, and the letters never stop.",
            ),
            choice(
                "Admit you peeked at the turnip part",
                &[ArcEffect::Reputation(4), ArcEffect::Settlement(2)],
                "You read it? Well, it IS a gripping account. The bit about the frost, honestly.",
                "Now you get letters too, from both of them, mostly about turnips. You've started to have opinions about turnips.",
            ),
            give_up(
                &[],
                "The letter sits in {giver}'s drawer, unsent. They write a new one every week, just in case.",
            ),
        ],
        pitch: voices(
            &[
                "Need a letter carried to {dest}. To {contact}. Bring the reply. Courier's sick, and I'm impatient.",
            ],
            &[
                "I write to a friend in {dest}, {contact}, and the courier's laid up! Would you carry my letter and bring back the reply? They always write back!",
            ],
            &[
                "I have a friend in {dest} I've never met, only written to. {Contact}. The courier is ill, and a letter waiting is a heavy thing. Would you?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager's letter needs carrying to their pen friend in the next settlement.",
            "Talk to the pen friend, then bring the reply home.",
            "The reply came home. It was mostly about turnips.",
        ),
    },
    ArcTemplate {
        id: ArcId(202),
        title: "Something Shiny",
        kind: ArcKind::Village,
        contact_other: true,
        hook: "The giver is sweet on a neighbour and wants to give them something that glows. A crystal from a formation would do.",
        twist: "When you bring it over, the neighbour has a crystal of their own for the giver, bought from a passing trader weeks ago, and no nerve to hand it over.",
        consequence: "Two crystals swap hands in the square. The village pretends not to watch. The village watches.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Relationship(RelationshipKind::Sweetheart),
            Trigger::Value(Value::Family),
            Trigger::Fear(Fear::BeingForgotten),
            Trigger::Quirk(Quirk::CollectsPebbles),
        ],
        roles: &[
            Role::Baker,
            Role::Farmer,
            Role::Brewer,
            Role::Smith,
            Role::Apprentice,
            Role::Courier,
        ],
        slots: &[Slot::Neighbour(RelationshipKind::Sweetheart)],
        targets: &[poi(PoiKind::Formation, 1, 4), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 3,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Chip a glowing crystal", "Deliver it, nervously"],
        steps: &[
            step(
                "{Giver} is sweet on {other} and wants to give them something that glows. Chip a crystal off {dest}, {where}.",
                "A pretty one. Not too big. Big is showing off. Medium. Medium and glowing.",
            ),
            meet(
                "You have a crystal that glows like a candle. {Giver} wants you to take it to {contact}. Right now. Before {giver} changes their mind.",
                "Go! Before I come to my senses. Tell them... no, don't tell them anything. Just give it.",
                "From {giver}? For me? ...Oh, wait. Wait right there. I've had this for weeks. I never dared. Would you take it back to them?",
            ),
        ],
        climax: "{Contact} presses a little crystal into your hand, blushing to the ears. It's the twin of the one you just gave them.",
        twists: &[],
        reward: QuestReward::credits(260, 10),
        outcomes: &[
            ending(
                "Hand back the second crystal",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Relationship(Some(RelationshipKind::Sweetheart)),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "They... had one for ME? For WEEKS? Oh, we're both hopeless. I'm going over there. Right now. Don't watch.",
                "{Giver} and {other} met in the square at dusk with their crystals. Everyone in {village} pretended to be busy. Everyone in {village} was not busy.",
            ),
            choice(
                "Tell the giver to deliver the second one themself",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(15)],
                "Me? Go over there myself? ...Yes. Yes, you're right. Hold my hat.",
                "{Giver} marched over to {other}'s door themself. It took three tries to knock. It was worth all three.",
            ),
            give_up(
                &[ArcEffect::Mood(-8)],
                "The crystal stays in {giver}'s pocket. They take it out sometimes and look at it.",
            ),
        ],
        pitch: voices(
            &[
                "I want to give {other} something that glows. Crystal from {dest}. Bring it, then take it to them. Don't make it a thing.",
            ],
            &[
                "Okay. So. I'm sweet on {other}. Don't laugh! I want to give them a glowing crystal from {dest}. Would you fetch one and, um, deliver it?",
            ],
            &[
                "There's someone. {Other}. I can't find the words, so I thought, a crystal. Something that glows in the dark the way they do. Would you help?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager wants to give their sweetheart a glowing crystal.",
            "Chip a crystal from the formation, then hand it to the sweetheart.",
            "Two crystals changed hands. Everyone pretended not to watch.",
        ),
    },
    ArcTemplate {
        id: ArcId(203),
        title: "The Feud Cake",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "Two families have been feuding for so long nobody remembers why. The giver wants to bake a peace cake, if they can get the spice.",
        twist: "The spice trader asks which family it's for. Both, it turns out: both grandmothers used the same recipe, and it was the recipe they fell out over.",
        consequence: "The cake is shared. The feud cools into an argument about whose grandmother made it first.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Problem(VillageProblem::Feud),
            Trigger::Quirk(Quirk::BakesForEveryone),
            Trigger::Role(Role::Baker),
        ],
        roles: &[Role::Baker, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Camp), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Fetch the spice", "Bring the families to the table"],
        steps: &[
            meet(
                "{Giver} needs a particular spice for a peace cake. The only one who sells it is {contact} at {dest}, {where}.",
                "It's a reddish powder that smells like a bonfire and a sneeze. Ask {contact}.",
                "The bonfire spice? Ha. Last one who bought a tin was a grandmother from your way, years back. She bought two. Said it was for a recipe she wasn't giving away.",
            ),
            meet(
                "Spice in hand. Now someone has to tell the other family about the cake. {Giver} nominates you. Find {contact} in {village}.",
                "Talk to {contact}. They're the least likely to throw a boot.",
                "A cake? With the bonfire spice? ...That's my grandmother's recipe. That's my grandmother's recipe. How does YOUR lot have it?",
            ),
        ],
        climax: "It all comes out at once: both grandmothers, one recipe, a long-ago bake sale and a single blue ribbon. That's the whole feud. A cake.",
        twists: &[],
        reward: QuestReward::credits(300, 9),
        outcomes: &[
            ending(
                "Share the cake with both families",
                &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "We fought for thirty years over a CAKE. Well. It is a very good cake. Pass the plate.",
                "The families share a table at the inn now and argue, loudly and happily, about whose grandmother invented the bonfire cake. {Village} has a bake-off every year. It always ends in a tie.",
            ),
            choice(
                "Enter the cake in the fair under the giver's name",
                &[ArcEffect::Reputation(10), ArcEffect::Prosperity(-1)],
                "First prize! Oh, that's... that's going to start something, isn't it. Oh dear.",
                "{Giver}'s cake took the blue ribbon. The feud is back, hotter than ever, and the whole thing smells faintly of bonfire.",
            ),
            give_up(
                &[],
                "No cake, no peace. The families still cross the square on opposite sides.",
            ),
        ],
        pitch: voices(
            &[
                "Families here have feuded since before I was born. I'm baking a peace cake. Need spice from {dest}. Then the hard part.",
            ],
            &[
                "I'm going to end this silly feud with CAKE! I just need a spice from {contact} at {dest}, and then someone brave to invite the other side. That's you!",
            ],
            &[
                "Two families, one street, thirty years of not speaking. I think a cake might do what words couldn't. I need a spice, and a messenger.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A baker wants to end an old feud with a peace cake.",
            "Talk to the spice seller, then invite the other family.",
            "The feud was about a cake. Now there is cake.",
        ),
    },
    ArcTemplate {
        id: ArcId(204),
        title: "The Courier Who Didn't Come",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The courier didn't arrive. Someone at the nearest camp saw them last; after that, a cave.",
        twist: "The courier wasn't lost: they were mapping a shortcut and got very excited about it. It is, to be fair, a great shortcut.",
        consequence: "The courier comes home with a new route, and the giver stops pacing.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Problem(VillageProblem::MissingPerson),
            Trigger::Fear(Fear::LosingFamily),
            Trigger::Role(Role::Courier),
        ],
        roles: &[Role::Courier, Role::Warden],
        slots: &[],
        targets: &[
            place(StoryPlace::Camp),
            feature(FeatureKind::LargeCavern, 1, 2),
        ],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Ask at the camp", "Search the cave", "Bring them home"],
        steps: &[
            meet(
                "The courier never arrived. {Giver} heard they stopped at {dest}, {where}. Ask {contact}.",
                "Ask at the camp. Couriers always stop for tea. Always.",
                "The courier? Came by two days back, very excited, talking about a 'shortcut under the hill'. Pointed at the old cave and off they went.",
            ),
            step(
                "The trail leads to {dest2}, {where2}. The courier went in after a shortcut.",
                "A shortcut? Under a hill? That fool. That wonderful fool. Find them.",
            ),
            step(
                "The courier is alive, filthy and thrilled, with a map. Bring them up.",
                "Bring them up! I'm going to hug them. Then I'm going to shout. Then hug again.",
            ),
        ],
        climax: "Your lamps find the courier sitting on a rock, sketching, beaming. 'You came through the east side? Then you've seen it! Half a day off the run! HALF A DAY!'",
        twists: &[],
        reward: QuestReward::credits(780, 13),
        outcomes: &[
            ending(
                "Bring the courier home",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(25),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Half a day! They scared me half to death for half a day! ...It is a good shortcut, though, isn't it.",
                "The courier's shortcut is on every map now, labelled in {giver}'s handwriting: 'Do NOT go exploring without telling someone.' The mail is faster than ever.",
            ),
            choice(
                "Keep the shortcut secret for yourself",
                &[ArcEffect::Reputation(4), ArcEffect::Credits(160)],
                "You want it kept quiet? ...I won't ask. The courier's home. That's what matters.",
                "Only you and the courier use the shortcut. You nod to each other in the dark sometimes, like conspirators.",
            ),
            give_up(
                &[ArcEffect::Mood(-15)],
                "The courier found their own way home three days later, surprised anyone was worried. {Giver} has not forgiven you.",
            ),
        ],
        pitch: voices(
            &[
                "Courier's two days late. Last seen at {dest}. Ask there, then look. Couriers don't get lost. Usually.",
            ],
            &[
                "Our courier never showed up! Someone at {dest} saw them last. Could you ask around and go looking? I'm sure they're fine. I'm sure. Mostly sure.",
            ],
            &[
                "The courier is late, and the road is quiet in a way I don't like. They stopped at {dest}. Would you follow their trail? Please?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A courier has gone missing between settlements.",
            "Ask at the camp, search the cave beyond, and bring the courier home.",
            "The courier was mapping a shortcut. A good one.",
        ),
    },
    ArcTemplate {
        id: ArcId(205),
        title: "The Long Way Round",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The giver's estranged family moved away years ago. Someone in the next village might know where they went.",
        twist: "They never left town at all: they left a letter with the innkeeper there, years ago, addressed to the giver. Nobody ever carried it.",
        consequence: "The letter reaches the giver at last, a door opens, and the giver starts writing back.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::MakeAmends),
            Trigger::Absent(AbsentFate::Estranged),
            Trigger::Absent(AbsentFate::MovedAway),
        ],
        roles: &[],
        slots: &[Slot::Absent(AbsentFate::Estranged)],
        targets: &[place(StoryPlace::Village), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Reach { target: 1 }],
        ],
        beats: &["Ask after them", "Bring the letter home"],
        steps: &[
            meet(
                "{Giver} hasn't spoken to {focus}, their {focus_relation}, in years. Someone in {dest}, {where}, might know where they went. Ask {contact}.",
                "Just ask if they're well. You don't have to say I asked. You can say I asked.",
                "{Focus}? Oh, I remember {focus}. Left a letter here before they moved on. For {giver}. It's been in the drawer behind the bar for years. Nobody ever came for it.",
            ),
            step(
                "A letter, yellow at the edges, addressed to {giver} in {focus}'s hand. Bring it home.",
                "A letter? From {focus}? Don't read it to me. Bring it. I need to hold it.",
            ),
        ],
        climax: "The envelope is soft with age. On the back, in faded ink: 'If you're reading this, I was too proud to come myself. I'm sorry. Write back.'",
        twists: &[],
        reward: QuestReward::credits(280, 11),
        outcomes: &[
            ending(
                "Give them the letter",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Mood(30),
                    ArcEffect::Remember,
                ],
                "Years. It was here for YEARS. And all that time I thought... Well. I'll write back. Tonight.",
                "{Giver} wrote back the same night, then again the next morning. The letters go back and forth with every courier now, and some of them are just recipes.",
            ),
            choice(
                "Offer to carry their reply too",
                &[ArcEffect::Reputation(8), ArcEffect::FollowUp(ArcId(201))],
                "You'd carry one back? Then I'd better write a good one. Give me a day. Maybe two.",
                "{Giver} is writing the most careful letter of their life. They've asked you to carry it when it's done.",
            ),
            give_up(
                &[],
                "{Focus}'s letter stays in the drawer behind the bar. Someday someone will carry it.",
            ),
        ],
        pitch: voices(
            &[
                "My {focus_relation}, {focus}. We fell out. Years ago. Somebody in {dest} might know where they went. Ask. That's all.",
            ],
            &[
                "I've been thinking about my {focus_relation}, {focus}. We fell out ages ago. Someone in {dest} might know where they are! Could you ask?",
            ],
            &[
                "Pride is a heavy thing to carry for years. My {focus_relation}, {focus}, went away after we quarrelled. Someone in {dest} might remember them.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager wants news of an estranged relative who moved away.",
            "Ask in the next village, then bring home what you find.",
            "A letter, years late, reached home at last.",
        ),
    },
    ArcTemplate {
        id: ArcId(206),
        title: "Where the Water Went",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The well ran dry overnight. The water went somewhere: find it in the caves below.",
        twist: "A hermit dug a lovely underground bath and accidentally drained the village's well into it. They're mortified.",
        consequence: "The water's back in the well, and the hermit's bath is now a public one, open Tuesdays.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Problem(VillageProblem::DryWell),
            Trigger::Role(Role::Farmer),
            Trigger::Value(Value::Community),
        ],
        roles: &[Role::Farmer, Role::Warden],
        slots: &[],
        targets: &[feature(FeatureKind::Cave, 1, 3), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 8,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Follow the water down",
            "Unblock the channel",
            "Tell the village",
        ],
        steps: &[
            step(
                "{Village}'s well dried up overnight. The water had to go somewhere: {dest}, {where}, by the sound of it.",
                "Listen for dripping. Water always gossips about where it's going.",
            ),
            step(
                "There's a fresh channel cut into the rock, running off toward a warm, steamy pool. Dig the old channel open again.",
                "Is that... a bath? Somebody's having a bath in our water? Unblock it!",
            ),
            meet(
                "The water's running home. Someone ought to tell the village what happened, gently. {Contact} will spread it fastest.",
                "Tell {contact}. They'll have it round the whole place by supper.",
                "A HERMIT? Running a BATH? With our WELL? ...Oh, that's the best news I've heard all year. Wait till I tell everyone.",
            ),
        ],
        climax: "The rock gives, the channel gurgles, and somewhere behind you a voice wails: 'My bath!' An embarrassed hermit in a towel waves an apology.",
        twists: &[],
        reward: QuestReward::credits(820, 12),
        outcomes: &[
            ending(
                "Bring the water home",
                &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "The well's full! And there's a BATH down there? A warm one? ...We should talk to this hermit.",
                "The well is full again. The hermit's bath is a public bath now, open Tuesdays, with a little sign: 'Please do not drain the village.'",
            ),
            choice(
                "Split the water: half well, half bath",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(120)],
                "Share it? With a bath? ...Fine. But I'm having the first soak.",
                "Half the water runs to the well and half to the bath. {Giver} goes down every evening. Their back has never been better.",
            ),
            give_up(
                &[ArcEffect::Prosperity(-1)],
                "The well stays dry. {Village} hauls water from the river, grumbling, and the hermit's bath stays very warm.",
            ),
        ],
        pitch: voices(
            &["Well's dry. Overnight. Water went down into {dest}. Find it, bring it back."],
            &[
                "Our well went dry overnight! The water's gone somewhere down in {dest}, {where}. Could you track it down? We're running out of tea!",
            ],
            &[
                "The well is silent this morning. Water doesn't just leave; it goes somewhere. {Dest}, I think. Would you follow it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A village well ran dry overnight. Find where the water went.",
            "Reach the cave, dig the old channel open, and tell the village.",
            "A hermit's bath had drained the well. The water is home.",
        ),
    },
    ArcTemplate {
        id: ArcId(207),
        title: "Market Day",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The giver wants to hold the biggest market day in years and needs word spread to two other settlements.",
        twist: "Everyone comes. EVERYONE. The giver forgot to arrange chairs.",
        consequence: "Market day becomes a yearly thing, and the giver becomes the person who knows where to get chairs.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Value(Value::Community),
            Trigger::Role(Role::Trader),
            Trigger::Quirk(Quirk::KnowsEveryonesBusiness),
        ],
        roles: &[Role::Trader, Role::Innkeeper, Role::Brewer],
        slots: &[],
        targets: &[place(StoryPlace::Village), place(StoryPlace::Camp)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Spread the word", "Spread it further"],
        steps: &[
            meet(
                "{Giver} is throwing a market day and wants everyone there. Start with {contact} in {dest}, {where}.",
                "Tell them there'll be stalls. And music. And pie. Mostly mention pie.",
                "A market day? With pie? Count us in. Count all of us in. I'll bring the fiddle.",
            ),
            meet(
                "One down. The travellers at {dest2}, {where2}, should hear too: they've the best goods. Find {contact} there.",
                "The camp folk have things you can't get anywhere else. Tell them the pitch fees are waived!",
                "Market day, eh? Free pitches? We'll be there with the wagon. Tell your friend to save us a spot near the pie.",
            ),
        ],
        climax: "By noon on market day the trail is a river of wagons, rigs and people carrying their own chairs, because somebody forgot to arrange chairs.",
        twists: &[],
        reward: QuestReward::credits(320, 9),
        outcomes: &[
            ending(
                "Help set up the stalls",
                &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(5),
                    ArcEffect::Remember,
                ],
                "Everyone came. EVERYONE. I didn't arrange chairs. Why did nobody tell me about chairs?",
                "Market day is a yearly fixture now. {Giver} owns forty chairs and lends them out with great authority. The pie sells out by ten.",
            ),
            choice(
                "Take a stall yourself",
                &[ArcEffect::Reputation(4), ArcEffect::Credits(140)],
                "A stall? Of course! Sell whatever it is you dig up. People love a bit of rock.",
                "You sold a whole crate of pretty rocks at market day. {Giver} takes a small commission, and a large amount of credit.",
            ),
            give_up(
                &[],
                "The market day was small and pleasant. Nobody came from far off. There were enough chairs.",
            ),
        ],
        pitch: voices(
            &[
                "Market day. Biggest in years. Need word sent to {dest} and the camp. You go places. Go to those places.",
            ],
            &[
                "I'm throwing the biggest market day in years! Could you spread the word to {dest} and the travellers' camp? There'll be pie!",
            ],
            &[
                "I'd like to fill this place with people again, the way it was when I was small. A market. Would you carry the invitation?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A trader is throwing a market day and wants the word spread.",
            "Tell someone in the next village, then someone at the camp.",
            "Everyone came to market day. Nobody arranged chairs.",
        ),
    },
    ArcTemplate {
        id: ArcId(208),
        title: "Tall Tale",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The local storyteller swears there's a crystal 'as big as a barn' down there. Nobody believes them. Prove it.",
        twist: "It isn't as big as a barn. It's bigger. The storyteller is, for once, speechless.",
        consequence: "The storyteller's tall tales get a new respect, and slightly taller.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(45)],
        triggers: &[
            Trigger::Quirk(Quirk::TallTales),
            Trigger::Role(Role::Storyteller),
            Trigger::Want(Want::ProveThemselves),
        ],
        roles: &[Role::Storyteller, Role::Hermit],
        slots: &[],
        targets: &[poi(PoiKind::Formation, 3, 5), place(StoryPlace::Camp)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 0 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Ping the barn-sized crystal",
            "See it with your own eyes",
            "Find a witness",
        ],
        steps: &[
            step(
                "{Giver} says there's a crystal as big as a barn at {dest}, {where}. Ping it and see.",
                "As big as a barn! A big barn! Two cows, minimum.",
            ),
            step(
                "The scanner nearly overloaded. Go and look at it properly, with your own lamps.",
                "The scanner went wild, didn't it. DIDN'T IT. Go look. Go look!",
            ),
            meet(
                "It's real, and it's enormous. Nobody will take {giver}'s word for it, but a stranger's... Tell {contact} at {dest2}.",
                "Tell someone who doesn't owe me money. Someone at the camp. They'll spread it.",
                "Bigger than a barn? You saw it? With your own eyes? Well, I'll be. I'll tell every trader on the road.",
            ),
        ],
        climax: "Your floodlamps sweep across a crystal face that goes up and up past the edge of the light. It isn't as big as a barn. A barn would fit inside it.",
        twists: &[],
        reward: QuestReward::credits(740, 12),
        outcomes: &[
            ending(
                "Back the tale",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Bigger than a barn? ...I'm going to have to make my other stories bigger now, you know. It's only fair.",
                "{Giver}'s tales are taken seriously now, which has made them much worse. Travellers ask after 'the barn crystal'. {Giver} charges them to hear the story.",
            ),
            choice(
                "Say it's only about as big as a shed",
                &[
                    ArcEffect::Reputation(-4),
                    ArcEffect::Mood(-10),
                    ArcEffect::Credits(100),
                ],
                "A SHED? A shed?! I'll never live this down. ...It was a big shed, though? Tell me it was a big shed.",
                "The crystal is 'the shed' now, and {giver} sulks about it at the inn. Secretly, you know better.",
            ),
            give_up(
                &[ArcEffect::Mood(-6)],
                "Nobody believes {giver}'s barn crystal. It's still down there, bigger than a barn, and smug about it.",
            ),
        ],
        pitch: voices(
            &[
                "There's a crystal down at {dest} as big as a barn. Nobody believes me. Go see. Then tell them.",
            ],
            &[
                "Nobody believes me but there's a crystal as BIG AS A BARN down at {dest}! Go and see it, and then tell everyone I was right!",
            ],
            &[
                "I've told a few tall tales, I admit it. But this one's true. A crystal like a cathedral, at {dest}. Would you be my witness?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A storyteller swears there's a crystal as big as a barn. Prove it.",
            "Scan the formation, reach it, and tell a witness at the camp.",
            "The crystal is bigger than a barn.",
        ),
    },
    ArcTemplate {
        id: ArcId(209),
        title: "Seed Swap",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "A farmer's seeds keep failing in this soil. A farmer at the next settlement grows the same crop beautifully. Ask them how.",
        twist: "The secret isn't the seed. It's the song: the other farmer sings to their rows, and it's genuinely, scientifically working (something about the vibration and the beetles).",
        consequence: "Two farmers become friends, and there's a lot more singing in the fields.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Farmer),
            Trigger::Value(Value::Craft),
            Trigger::Quirk(Quirk::HumsOffKey),
        ],
        roles: &[Role::Farmer, Role::Healer],
        slots: &[],
        targets: &[
            place(StoryPlace::Village),
            feature(FeatureKind::ArtifactBed, 1, 2),
        ],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Dig {
                target: 1,
                count: 5,
            }],
        ],
        beats: &["Ask the other farmer", "Dig some old soil for the rows"],
        steps: &[
            meet(
                "{Giver}'s crop keeps failing. {Contact} in {dest}, {where}, grows the same crop beautifully. Ask how.",
                "Ask what they feed it. Ask what they whisper to it. Ask everything.",
                "My secret? I sing to them. Don't laugh. The beetles hate it, the roots love it. Oh, and dig them a handful of old soil from the relic beds, for minerals.",
            ),
            step(
                "{Contact} swears by old soil from {dest2}, {where2}. Dig some out for {giver}'s rows.",
                "Old soil, and singing? I can do old soil. The singing I'll have to work on.",
            ),
        ],
        climax: "The soil from the relic beds is dark and rich and full of tiny fossil flecks. Somewhere above, very faintly, someone is practising scales.",
        twists: &[],
        reward: QuestReward::credits(270, 9),
        outcomes: &[
            ending(
                "Bring the soil and the secret",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
                "Singing? To the rows? ...I'll try anything. Don't stand too close, I'm told I'm flat.",
                "{Giver}'s rows are thriving and the beetles have moved next door. You can hear the singing from the road. It is very flat. It is working.",
            ),
            choice(
                "Suggest they hire a proper singer",
                &[ArcEffect::Reputation(4), ArcEffect::Settlement(2)],
                "A singer! For the turnips! ...Actually, the inn's lad has a lovely voice.",
                "The inn's lad sings to {giver}'s fields every morning now, for a turnip a day. It's become a bit of a tourist attraction.",
            ),
            give_up(
                &[],
                "{Giver}'s rows struggle on in silence. They never did find out the secret.",
            ),
        ],
        pitch: voices(
            &[
                "My crop keeps dying. {Contact} in {dest} grows it fine. Go ask how. I'd go but we don't get on.",
            ],
            &[
                "My seedlings keep wilting but {contact} over in {dest} grows a beautiful crop! Would you ask their secret? I'll owe you a basket!",
            ],
            &[
                "My rows wither, and theirs flourish, and the soil is the same soil. There's a secret in {dest}. Would you ask {contact} for it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A farmer's crop keeps failing; another farmer grows it beautifully.",
            "Ask the other farmer, then dig the soil they recommend.",
            "The secret was singing. It works.",
        ),
    },
    ArcTemplate {
        id: ArcId(210),
        title: "The Rival's Rig",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The giver's rival is stuck: their old rig broke down in a cavern. The giver wants it hauled out, and wants nobody to know they asked.",
        twist: "The rival's radio was working the whole time. They heard every word, including the bit where the giver said 'Don't tell them I sent you'.",
        consequence: "The rivals become friends, grudgingly, and then not grudgingly at all.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(20)],
        triggers: &[
            Trigger::Relationship(RelationshipKind::Rival),
            Trigger::Value(Value::Honesty),
            Trigger::Absent(AbsentFate::OldRival),
        ],
        roles: &[Role::Prospector, Role::Mechanic],
        slots: &[Slot::Neighbour(RelationshipKind::Rival)],
        targets: &[feature(FeatureKind::LargeCavern, 1, 3)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Escort {
                target: 0,
                step_ms: 650,
                crew_step_ms: 420,
            }],
        ],
        beats: &["Find the stuck rig", "Haul it up"],
        steps: &[
            step(
                "{Other}'s rig broke down in {dest}, {where}. {Giver} wants it hauled out, and wants their name kept out of it.",
                "Don't say I sent you. Say you were passing. Say you were lost. Say anything.",
            ),
            step(
                "{Other}'s old rig is on its last legs. Stay close and it'll limp up behind you.",
                "Slowly! That rig's older than both of us. Older than our quarrel, even.",
            ),
        ],
        climax: "As you break the surface, {other}'s radio crackles: 'Tell {giver} thanks. Yes, I heard. I heard all of it. Tell them the soup's on me.'",
        twists: &[],
        reward: QuestReward::credits(720, 12),
        outcomes: &[
            ending(
                "Tell them the radio was on",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Relationship(Some(RelationshipKind::Friend)),
                    ArcEffect::Mood(15),
                    ArcEffect::Remember,
                ],
                "They HEARD? All of it? ...The soup's on them, you say? Well. I suppose I'll have to go and eat it.",
                "{Giver} and {other} eat soup together at the inn on Fridays, bickering over whose rig is worse. Both rigs are terrible. Both of them are happy.",
            ),
            choice(
                "Keep the giver's secret, as asked",
                &[ArcEffect::Reputation(6)],
                "You didn't tell them? Good. ...Did they say anything? Anything about me? No? Good. Fine.",
                "{Other} never knew who sent you. Or so {giver} believes. {Other} leaves a jar of soup on {giver}'s step, unsigned, every winter.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "{Other} walked out on their own, two days later, muttering. {Giver} pretends not to have noticed.",
            ),
        ],
        pitch: voices(
            &["{Other}'s rig broke down in {dest}. Haul it out. Don't say I sent you. I mean it."],
            &[
                "So, my rival {other} got stuck in {dest} and I couldn't care less! But could you haul them out? And maybe not mention me? Thanks!",
            ],
            &[
                "{Other} and I haven't spoken in years, but their rig is dying in the dark and that's not a thing I can sleep through. Would you help them? Quietly?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager's rival is stuck in a cavern. Haul their rig out, quietly.",
            "Reach the cavern and escort the old rig up.",
            "The rival heard everything. The soup is on them.",
        ),
    },
];
