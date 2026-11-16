//! Batch: home arcs. Settling down: homesteads, beds below, fuel stations at depth and the
//! builders who help. `Settle` beats need a module standing in a claim the player holds
//! (WP-BASE today; WP-HOME's contracted homes are claims too, so they count). Ids 501-506.

use super::{NO_THANKS, choice, ending, feature, give_up, log, meet, place, step, voices};
use crate::{
    base::ModuleKind,
    cosmetics::{CosmeticItem, RearAttachment, RoofAttachment},
    people::{
        character::Role,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger, WorldFactKind},
        traits::{Fear, Quirk, Value, Want},
    },
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace},
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(501),
        title: "Housewarming",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A builder thinks you've been living out of your rig too long. Put down roots: a homestead of your own. They'll throw the housewarming.",
        twist: "The whole village turns up, each carrying a chair, because the builder told everyone you didn't own any.",
        consequence: "You have a home, a village that knows where it is, and far too many chairs.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Smith),
            Trigger::Value(Value::Community),
            Trigger::Quirk(Quirk::BakesForEveryone),
        ],
        roles: &[Role::Smith, Role::Apprentice, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Settle {
                kind: ModuleKind::Homestead,
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &["Put up a homestead", "Tell the village"],
        steps: &[
            step(
                "{Giver} says you've lived out of your rig long enough. Stake a claim on the surface and put up a homestead.",
                "Somewhere with a view. A home should have a view. And a door that shuts properly.",
            ),
            meet(
                "Your homestead stands. Someone should spread the word for the housewarming: {contact} in {village} will.",
                "Tell {contact}! They'll have the whole village there by sundown.",
                "A housewarming? Tonight? Leave it to me. ...You don't own any chairs, do you. Don't worry. Leave that to me too.",
            ),
        ],
        climax: "At sundown the trail fills with lanterns. Everyone in {village} is coming up the path to your door, and every single one of them is carrying a chair.",
        twists: &[],
        reward: QuestReward::credits(400, 10),
        outcomes: &[
            ending(
                "Throw open the door",
                &[
                    ArcEffect::Settlement(5),
                    ArcEffect::Credits(120),
                    ArcEffect::Remember,
                ],
                "Welcome home! Look at it. A real roof, a real door, and... how many chairs is that? Forty? Well, you'll never be short of a seat.",
                "Your homestead has forty-one chairs, a door that shuts properly, and visitors from {village} most evenings. {Giver} still claims the best chair, by the window.",
            ),
            choice(
                "Keep it a quiet night in",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(-5)],
                "A quiet night? In a new home? ...Fine, fine. I'll bring a pie round tomorrow. Just one pie. Possibly two.",
                "Your first night home was quiet. The second night, {giver} turned up with two pies and a chair. Things escalated from there.",
            ),
            give_up(
                &[],
                "You're still living out of your rig. {Giver} shakes their head every time you drive past.",
            ),
        ],
        pitch: voices(
            &[
                "You live in that rig. It's sad. Stake a claim, put up a homestead. I'll throw the housewarming.",
            ],
            &[
                "You can't live in a rig forever! Put up a homestead somewhere nice and I'll throw you the best housewarming this side of the slate!",
            ],
            &[
                "Everyone needs somewhere to come home to, even people who spend their lives underground. Would you let us help you put down roots?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A builder wants you to put down roots: a homestead of your own.",
            "Have a homestead standing in your claim, then spread word of the housewarming.",
            "Everyone came to the housewarming, with chairs.",
        ),
    },
    ArcTemplate {
        id: ArcId(502),
        title: "A Bed Below",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "An old miner says nobody really knows the deep until they've slept in it. Set up a bed down there and spend the night.",
        twist: "Lying in the dark, you hear it: the deep's slow hum, like the world breathing in its sleep. The old miner just smiles.",
        consequence: "You have a bed below and a new respect from every old miner in the valley.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(40)],
        triggers: &[
            Trigger::Role(Role::RetiredMiner),
            Trigger::Value(Value::Tradition),
            Trigger::Fear(Fear::TheDark),
        ],
        roles: &[Role::RetiredMiner, Role::Warden],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Settle {
                kind: ModuleKind::Bed,
            }],
            &[Objective::Reach { target: 0 }],
        ],
        beats: &["Make a bed in the deep", "Come up and tell it"],
        steps: &[
            step(
                "{Giver} says you don't know the deep till you've slept in it. Set up a bed in a claim of yours below ground.",
                "Underground. Proper underground. A cot in a cellar doesn't count, I'll know.",
            ),
            step(
                "A bed below, and a night's sleep in the deep. Go back up to {village} and tell {giver} what you heard.",
                "Did you hear it? You heard it. Come and tell me. I'll know if you're fibbing.",
            ),
        ],
        climax: "In the dark, with the lamps off, the deep breathes: a slow, enormous hum, so low it's more a feeling than a sound, like lying on the chest of something asleep.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Tell them you heard the deep breathe",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Cosmetic(CosmeticItem::Rear(RearAttachment::Lantern)),
                    ArcEffect::Remember,
                ],
                "Breathing. Yes. That's it exactly. Forty years I've tried to describe it and you've done it in one word. Welcome to the deep, miner.",
                "Every old miner in {village} nods to you now, the way they nod to each other. {Giver} gave you their old trail lantern. 'For the walk to bed,' they said.",
            ),
            choice(
                "Admit you slept straight through",
                &[ArcEffect::Reputation(4), ArcEffect::Mood(10)],
                "Slept straight through? In the DEEP? Ha! Then you're either very brave or very tired. Both, probably.",
                "{Giver} tells everyone about the miner who slept through the deep's breathing. It's become a compliment, somehow.",
            ),
            give_up(
                &[],
                "No bed below. {Giver} says you'll understand one day, and leaves it there.",
            ),
        ],
        pitch: voices(
            &[
                "Nobody knows the deep till they've slept in it. Set up a bed down there. Spend a night. Then we'll talk.",
            ],
            &[
                "Here's a miner's secret: you don't really know the deep until you've slept in it! Set up a bed down there and tell me what you hear!",
            ],
            &[
                "When I was young, my teacher made me sleep a night underground. I've never forgotten what I heard. Would you? Then come and tell me.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old miner says nobody knows the deep until they've slept in it.",
            "Have a bed standing in your claim, then come back up and tell it.",
            "The deep breathes in its sleep.",
        ),
    },
    ArcTemplate {
        id: ArcId(503),
        title: "Fuel at the Bottom",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A courier is sick of flying all the way up to refuel. Put an underground fuel station near the big cavern and everybody wins.",
        twist: "The courier immediately starts a rival delivery service from your station, and names it after you.",
        consequence: "The deep gets a little busier, and your name is painted on a delivery cart.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(35)],
        triggers: &[
            Trigger::Role(Role::Courier),
            Trigger::Want(Want::StrikeItRich),
            Trigger::Value(Value::Freedom),
        ],
        roles: &[Role::Courier, Role::Trader, Role::Mechanic],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Cavern,
        }],
        targets: &[feature(FeatureKind::LargeCavern, 2, 4)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Settle {
                kind: ModuleKind::FuelStation,
            }],
        ],
        beats: &["Scout the cavern", "Build the fuel station"],
        steps: &[
            step(
                "{Giver} is sick of flying all the way up to refuel. Scout {a place} {where} for a station.",
                "Somewhere central. Somewhere roomy. Somewhere I can turn my rig around without scraping the paint.",
            ),
            step(
                "Good spot. Stake a claim nearby and build a fuel station.",
                "A fuel station down there would change everything. EVERYTHING. Well, my fuel bill.",
            ),
        ],
        climax: "The fuel station hums into life in the dark, its pump light glowing green. Within an hour, three rigs you've never seen before are queuing at it.",
        twists: &[],
        reward: QuestReward::credits(1_100, 13),
        outcomes: &[
            ending(
                "Open the station to everyone",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Open to everyone? Then I'm starting a delivery service from it. I'm calling it after you. No, you can't stop me.",
                "A cart with your name painted on it rattles through the deep tunnels now, running parcels from your fuel station. {Giver} is unbearably proud of it.",
            ),
            choice(
                "Charge the courier a fee",
                &[ArcEffect::Reputation(-4), ArcEffect::Credits(300)],
                "A fee? From ME? I gave you the idea! ...Fine. It's still cheaper than flying up. Barely.",
                "{Giver} pays your fee every week, with exact change and a pointed look.",
            ),
            give_up(
                &[],
                "{Giver} still flies all the way up to refuel, and complains to everyone about it.",
            ),
        ],
        pitch: voices(
            &[
                "I fly up to refuel twenty times a day. Build a fuel station down by {a place}. We all win.",
            ],
            &[
                "I'm SO tired of flying all the way up to refuel! Build a fuel station down near {a place}? Everyone in the deep will love you!",
            ],
            &[
                "Every trip up for fuel is an hour of my life I won't get back. A station down in {band}, near {a place}... imagine it.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A courier wants an underground fuel station near the big cavern.",
            "Scout the cavern, then have a fuel station standing in your claim.",
            "The fuel station opened. A delivery cart bears your name.",
        ),
    },
    ArcTemplate {
        id: ArcId(504),
        title: "The Workbench",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A smith's old master built a workbench they've always wanted to see in use again. Build yourself a workshop and they'll bring you the master's tools.",
        twist: "The tools come with a notebook of the master's designs, and a note: 'For whoever has a bench worth using.'",
        consequence: "Your workshop holds a master's tools, and the smith visits to watch you work.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Smith),
            Trigger::Value(Value::Craft),
            Trigger::Quirk(Quirk::NamesTheirTools),
        ],
        roles: &[Role::Smith, Role::Tinkerer],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Settle {
                kind: ModuleKind::Workshop,
            }],
            &[Objective::Reach { target: 0 }],
        ],
        beats: &["Build a workshop", "Collect the master's tools"],
        steps: &[
            step(
                "{Giver}'s old master left tools that deserve a proper bench. Build a workshop in a claim of yours.",
                "A proper workshop. Good light. A bench you could stand an anvil on.",
            ),
            step(
                "Your workshop is ready. Go and see {giver} in {village}: they have something for it.",
                "Come round. I've been keeping them oiled for years, waiting for a bench worth the trouble.",
            ),
        ],
        climax: "{Giver} unrolls a leather bundle: tools worn smooth by a lifetime of hands, each one with a name scratched on the handle, and a notebook tied with string.",
        twists: &[],
        reward: QuestReward::credits(380, 10),
        outcomes: &[
            ending(
                "Take the master's tools",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "They're named, you know. That's Ol' Bitey, that's Gerald, that's the Persuader. Treat them well. They've been lonely.",
                "The master's tools hang over your workbench, each by its name. {Giver} comes by to watch you work and corrects your grip, lovingly, every single time.",
            ),
            choice(
                "Ask them to work beside you instead",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Settlement(2),
                ],
                "Work beside you? At my age? ...I'll bring my apron. And Gerald. Gerald goes everywhere with me.",
                "{Giver} works at your bench two days a week now, humming. Their designs and yours are starting to look alike.",
            ),
            give_up(
                &[],
                "The master's tools stay in their bundle, oiled and waiting.",
            ),
        ],
        pitch: voices(
            &["My old master's tools need a real bench. Build a workshop. Then come see me."],
            &[
                "My master left me the most beautiful tools and nowhere to use them! Build yourself a workshop and they're yours. Well, on loan. Forever.",
            ],
            &[
                "Tools that aren't used forget what they're for. My master's have been waiting years. Build a bench worth them, and they're yours.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A smith will give you their master's tools once you have a workshop.",
            "Have a workshop standing in your claim, then collect the tools.",
            "The master's tools have a bench again.",
        ),
    },
    ArcTemplate {
        id: ArcId(505),
        title: "Somewhere to Keep It All",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A collector of pebbles, string and everything else wants to leave their collection to someone with proper storage.",
        twist: "Among the string and pebbles is a tin of old coins, a map with an X, and a single perfect pearl.",
        consequence: "You inherit a collection, a treasure, and a friend who visits to rearrange it.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::CollectsPebbles),
            Trigger::Quirk(Quirk::CountsThings),
            Trigger::Fear(Fear::BeingForgotten),
        ],
        roles: &[Role::Hermit, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Settle {
                kind: ModuleKind::Storage,
            }],
            &[Objective::Reach { target: 0 }],
        ],
        beats: &["Build proper storage", "Collect the collection"],
        steps: &[
            step(
                "{Giver} wants to leave their collection to someone with proper storage. Build a storage chest in a claim of yours.",
                "Dry. Lidded. Good hinges. Some of my pebbles are very old and they don't like drafts.",
            ),
            step(
                "Storage built. {Giver} in {village} is waiting with boxes. Many boxes.",
                "I've packed it all. Labelled. Cross-referenced. There may be a few boxes. Bring the rig.",
            ),
        ],
        climax: "Twelve boxes, all labelled. String (various). Pebbles (round). Pebbles (other). And at the bottom, a tin: old coins, a map with an X, and a single perfect pearl.",
        twists: &[],
        reward: QuestReward::credits(420, 10)
            .materials(&[(crate::terrain::StrategicResourceKind::PressurePearl, 1)]),
        outcomes: &[
            ending(
                "Take the whole collection",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(25),
                    ArcEffect::Cosmetic(CosmeticItem::Roof(RoofAttachment::Trophy)),
                    ArcEffect::Remember,
                ],
                "All of it! Even the string! Oh, I knew you'd understand. Visit them. Visit ME. I'll come and dust.",
                "{Giver}'s collection fills your storage chest: pebbles, string, buttons, a map with an X. {Giver} visits weekly to rearrange it, and you've stopped pretending you mind.",
            ),
            choice(
                "Take only the tin; let them keep the rest",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(100)],
                "Just the tin? ...You're right. I'm not ready to let the pebbles go. Not yet. Maybe next year.",
                "You took the tin. {Giver} kept the pebbles, and seems happier for it. The map with the X is pinned over your bed.",
            ),
            give_up(
                &[],
                "The collection stays in {giver}'s cottage, growing. Nobody is sure where they sleep anymore.",
            ),
        ],
        pitch: voices(
            &[
                "Got a collection. Big one. Want to leave it to someone with proper storage. Build some. Then we'll see.",
            ],
            &[
                "I've been collecting things my whole life and I want them to go to someone who'll look after them! Build some proper storage and they're yours!",
            ],
            &[
                "When I'm gone, I'd like my things to go somewhere they'll be kept and counted. Would you build somewhere for them? Somewhere dry?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A collector wants to leave their collection to someone with proper storage.",
            "Have a storage chest standing in your claim, then collect the boxes.",
            "The collection came with a treasure.",
        ),
    },
    ArcTemplate {
        id: ArcId(506),
        title: "A Light in the Window",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A worrier frets about you flying home in the dark. They want to know there's a light on somewhere waiting for you: power up a home of your own.",
        twist: "They walk all the way out to see it themselves, at night, and stand there smiling at the light like it's a sunrise.",
        consequence: "Somebody worries about you a little less, and you've got a light on at home.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(15)],
        triggers: &[
            Trigger::Fear(Fear::TheDark),
            Trigger::Fear(Fear::LosingFamily),
            Trigger::Value(Value::Family),
        ],
        roles: &[Role::Healer, Role::Baker, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Settle {
                kind: ModuleKind::Generator,
            }],
            &[Objective::Settle {
                kind: ModuleKind::Homestead,
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &["Power a claim", "Put a home on it", "Tell the worrier"],
        steps: &[
            step(
                "{Giver} worries about you flying home in the dark. Start with power: a generator in a claim of yours.",
                "Power first. You can't have a light without power. I asked someone. They explained. At length.",
            ),
            step(
                "Power's on. Now a homestead, so the light has a window to shine from.",
                "A window. With a light in it. That's all I want. Is that so much?",
            ),
            meet(
                "Your home is lit. Let {giver} know through {contact}, who'll make sure they believe it.",
                "Tell {contact}. They'll tell me properly, with details. I need details.",
                "A light in the window? I'll tell {giver} straight away. They'll want to see it. They'll walk out there tonight, you watch.",
            ),
        ],
        climax: "That night a small figure walks up the trail with a lantern and stops at a distance, looking at your window. It's {giver}. They stand there a long time, smiling at the light like it's a sunrise.",
        twists: &[],
        reward: QuestReward::credits(950, 13),
        outcomes: &[
            ending(
                "Leave the light on",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Mood(30),
                    ArcEffect::Remember,
                ],
                "I saw it. With my own eyes. A light, on, waiting for you. I can sleep now. I'll sleep tonight like a stone.",
                "You leave the light on at home. Some nights, flying back late, you see a second lantern far off on the trail: {giver}, out for a walk, checking.",
            ),
            choice(
                "Invite them in for tea",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Settlement(2),
                    ArcEffect::Credits(80),
                ],
                "In? For TEA? Oh, I couldn't. ...Well, maybe one cup. Do you have biscuits? You don't, do you. I'll bring biscuits.",
                "{Giver} comes for tea on Sundays, with biscuits, and checks your lamp oil before leaving. Every Sunday.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "{Giver} still worries about you flying home in the dark. They light a candle in their own window, just in case.",
            ),
        ],
        pitch: voices(
            &[
                "You fly home in the dark. Nobody waiting. Bothers me. Get yourself a home with a light in it.",
            ],
            &[
                "I worry about you flying home in the dark with nobody waiting! Would you set up a proper home with a light on? For my nerves?",
            ],
            &[
                "Every night I see your lamps go out over the hill and I think: who's waiting for them? Would you make a home, with a light in the window? For me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A worrier wants to know there's a light on somewhere waiting for you.",
            "Power a claim, put a homestead on it, and let them know.",
            "The light is on. Someone came to see it.",
        ),
    },
];
