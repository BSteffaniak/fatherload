//! Batch: pilot arcs. Only the pilot on foot can finish them: squeezing into a crevice the rig
//! can't fit, carrying something in by hand, walking ground a rig would crush. Ids 701-707.

use super::{NO_THANKS, choice, ending, feature, give_up, log, meet, place, poi, step, voices};
use crate::{
    cosmetics::{CosmeticItem, Decal, FrontAttachment},
    people::{
        character::Role,
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Trigger, VillageProblem},
        traits::{Fear, Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace, UpgradeStat},
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(701),
        title: "Stuck",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "An apprentice crawled into a crevice after a shiny rock and got wedged. No rig fits. Climb out and go in on foot.",
        twist: "They're stuck because their pack is full of snacks. They refuse to leave the snacks.",
        consequence: "The apprentice is home, the snacks are safe, and the master has some words about crevices.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Apprentice),
            Trigger::Fear(Fear::LosingFamily),
            Trigger::Problem(VillageProblem::MissingPerson),
        ],
        roles: &[Role::Smith, Role::Tinkerer, Role::Healer],
        slots: &[],
        targets: &[feature(FeatureKind::Cave, 1, 2)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Find the crevice", "Squeeze in on foot", "Get them home"],
        steps: &[
            step(
                "{Giver}'s apprentice crawled into a crevice in {dest}, {where}, and got stuck. Get the rig as close as it'll go.",
                "Follow the swearing. They swear like a sailor when they're stuck. I've no idea where they learned it.",
            ),
            step(
                "The crevice is too narrow for the rig. Climb out and squeeze in on foot.",
                "On foot. Mind your head. And theirs. Mostly theirs, it's emptier.",
            ),
            step(
                "The apprentice is free, and clutching an enormous pack. Get them back up top.",
                "Is that the pack? Is it the snack pack? Of course it's the snack pack.",
            ),
        ],
        climax: "Your lamp finds the apprentice wedged sideways in the rock, perfectly cheerful, hugging a pack the size of a barrel. 'I can get out,' they say. 'I just can't get out with the snacks.'",
        twists: &[],
        reward: QuestReward::credits(320, 10),
        outcomes: &[
            ending(
                "Bring them home, snacks and all",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(20),
                    ArcEffect::Remember,
                ],
                "Stuck. Because of SNACKS. ...Thank you. They're an idiot, but they're my idiot.",
                "The apprentice is home, grounded, and sharing the snacks as an apology. {Giver} has hung a sign over the workshop: 'Measure the pack before the crevice.'",
            ),
            choice(
                "Leave the snacks for the cave mice",
                &[ArcEffect::Reputation(6), ArcEffect::Settlement(1)],
                "You left the snacks? For MICE? ...The apprentice will never forgive you. I think it's wonderful.",
                "The cave mice in that crevice are the fattest in the valley. The apprentice glares at you whenever you pass.",
            ),
            give_up(
                &[ArcEffect::Mood(-12)],
                "The apprentice wriggled out on their own, eventually, minus one pack. {Giver} is still upset you didn't go.",
            ),
        ],
        pitch: voices(
            &[
                "My apprentice is stuck in a crevice in {dest}. Rig won't fit. You'll have to go in on foot. Hurry.",
            ],
            &[
                "My apprentice crawled into a crevice in {dest} chasing a pretty rock and now they're stuck! No rig fits! Could you go in on foot? Please hurry!",
            ],
            &[
                "Somewhere in {dest}, my apprentice is wedged in the dark, too far for a rig. Would you climb out and go to them? Two hands are all it needs.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An apprentice is stuck in a crevice no rig can fit.",
            "Reach the cave, go in on foot, and bring them back up.",
            "They were stuck because of the snacks.",
        ),
    },
    ArcTemplate {
        id: ArcId(702),
        title: "Signed, Sealed, Hand-Delivered",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "A wedding invitation has to be delivered by hand: the recipient refuses anything dropped off by a rig. 'Rigs have no manners.'",
        twist: "The recipient is the giver's old flame, and the invitation asks them to be best person.",
        consequence: "An old friendship is rekindled, and the wedding has one more guest crying happily.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Value(Value::Family),
            Trigger::Relationship(RelationshipKind::Sweetheart),
            Trigger::Quirk(Quirk::OverlyFormal),
        ],
        roles: &[Role::Innkeeper, Role::Baker, Role::Courier],
        slots: &[],
        targets: &[place(StoryPlace::Village)],
        stages: &[&[Objective::HandDeliver {
            target: 0,
            parcel: "wedding invitation",
        }]],
        beats: &["Walk the invitation to the door"],
        steps: &[step(
            "{Giver} has a wedding invitation for someone in {dest}, {where}, who refuses anything handed over from a rig. Park, climb out, and walk it to a door.",
            "By hand. On foot. They're very particular. They once sent back a parcel because the rig 'looked smug'.",
        )],
        climax: "The door opens before you can knock. The person inside reads the invitation once, twice, then sits down on their own step. 'Best person,' they whisper. 'After everything. Best person.'",
        twists: &[],
        reward: QuestReward::credits(280, 10),
        outcomes: &[
            ending(
                "Tell them it was received with tears",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Settlement(2),
                    ArcEffect::Remember,
                ],
                "Tears? Good tears? ...Then they'll come. Oh, I'm so glad. I was so afraid they'd still be angry.",
                "The wedding was in {village}'s square. The best person gave a speech that made everyone cry, including two rigs' worth of passing miners. You were invited. Obviously.",
            ),
            choice(
                "Offer to carry the reply too",
                &[ArcEffect::Reputation(8), ArcEffect::FollowUp(ArcId(201))],
                "You'd carry the reply? On foot? You're a treasure. A slightly muddy treasure.",
                "You've become the trail's unofficial hand-delivery service. Some people wait specially for you to bring their letters.",
            ),
            give_up(
                &[],
                "The invitation stays on {giver}'s mantelpiece, unsent. The wedding goes on without a best person.",
            ),
        ],
        pitch: voices(
            &[
                "Wedding invitation for someone in {dest}. Won't take anything from a rig. Walk it to their door. On foot.",
            ],
            &[
                "I'm getting married! And I need this invitation delivered to {dest}, by hand, on foot. The recipient is very fussy about rigs. You'll see!",
            ],
            &[
                "Some things shouldn't arrive by machine. This invitation is one of them. Would you walk it, by hand, into {dest}?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A wedding invitation must be delivered by hand, on foot.",
            "Walk the invitation to a door in the next village.",
            "The invitation asked them to be best person.",
        ),
    },
    ArcTemplate {
        id: ArcId(703),
        title: "A Flag on the Edge",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "A child wants their homemade flag planted somewhere high, by hand, 'properly, like explorers'.",
        twist: "When you plant it, the wind catches it and you can see it from the village: a tiny bright square on the edge of the sky.",
        consequence: "The child can see their flag from their window every morning.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Want(Want::ProveThemselves),
            Trigger::Value(Value::Adventure),
            Trigger::Role(Role::Apprentice),
        ],
        roles: &[Role::Apprentice, Role::Baker, Role::Farmer],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
        ],
        beats: &["Fly up high", "Plant it by hand"],
        steps: &[
            step(
                "{Giver} has a homemade flag that needs planting on {dest}, {where}. Fly up.",
                "It has to be high. Higher than the church. Higher than anything. Explorers always pick the highest bit.",
            ),
            step(
                "You're here. Explorers plant flags by hand, so climb out and do it properly.",
                "Out of the rig! Explorers don't plant flags from rigs. It's in all the books.",
            ),
        ],
        climax: "You push the pole into the turf and the wind snaps the flag open: a wobbly hand-painted star on a blue square. Far below, a tiny figure is jumping up and down in a window.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Tell them it's flying",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Cosmetic(CosmeticItem::Decal(Decal::Star)),
                    ArcEffect::Remember,
                ],
                "I SAW it! From my window! It's flying! It's the highest flag in the world and it's MINE!",
                "Every morning {giver} checks the flag from the window. It's faded a little. They've asked you to take up a new one when it's worn out. You've said yes.",
            ),
            choice(
                "Plant your own flag beside it",
                &[ArcEffect::Reputation(8), ArcEffect::Settlement(2)],
                "Two flags! Mine and yours! Like a proper expedition! Is yours as good as mine? It's not, is it. That's okay.",
                "Two flags fly up there now, side by side. {Giver} insists theirs is bigger. It is not. You let them have it.",
            ),
            give_up(
                &[],
                "The flag is still folded on {giver}'s windowsill, waiting for an explorer.",
            ),
        ],
        pitch: voices(
            &["Made a flag. Wants planting somewhere high. By hand. Properly. Like explorers."],
            &[
                "I made a flag! Would you plant it way up high on {dest}? By hand, properly, like real explorers do? Please please please?",
            ],
            &[
                "I painted a flag for somewhere I'll never stand. Would you carry it up {where}, and plant it with your own two hands?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A child wants their homemade flag planted somewhere high, by hand.",
            "Reach the high place and plant the flag on foot.",
            "The flag flies where they can see it every morning.",
        ),
    },
    ArcTemplate {
        id: ArcId(704),
        title: "The Recipe Book",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "An innkeeper's prized recipe book was borrowed by the inn in the next village. It has to come home by hand: it's too precious for a cargo bay.",
        twist: "The other innkeeper has added a page: a recipe of their own, labelled 'For when we're both old and friends again.'",
        consequence: "Two rival innkeepers start swapping recipes, and both inns get better.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Innkeeper),
            Trigger::Quirk(Quirk::AlwaysHungry),
            Trigger::Relationship(RelationshipKind::Rival),
        ],
        roles: &[Role::Innkeeper, Role::Baker, Role::Brewer],
        slots: &[],
        targets: &[place(StoryPlace::Village), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::HandDeliver {
                target: 1,
                parcel: "recipe book",
            }],
        ],
        beats: &["Collect the book", "Carry it home by hand"],
        steps: &[
            meet(
                "{Giver}'s recipe book is at the inn in {dest}, {where}. Ask {contact} for it.",
                "Ask {contact}. Nicely. Then carry it home in your arms. Not the cargo bay. The cargo bay smells.",
                "The book? Here. ...I added a page. Don't read it. Well, read it, but don't tell them you read it.",
            ),
            step(
                "Carry the book into {village} in your own arms and hand it over at the door.",
                "In your arms! The cargo bay is for ore and regrets.",
            ),
        ],
        climax: "Tucked at the back of the book, in an unfamiliar hand: a new recipe, 'Two-Inn Stew', and underneath, 'For when we're both old and friends again.'",
        twists: &[],
        reward: QuestReward::credits(260, 9),
        outcomes: &[
            ending(
                "Hand over the book",
                &[
                    ArcEffect::Reputation(9),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "A new page? 'Two-Inn Stew'? ...The cheek. I'm going to have to make it now. And it'll be better than theirs. And then I'll write back.",
                "{Giver} and {contact} trade recipes by post now, each one slightly more competitive than the last. Both inns have never served better food.",
            ),
            choice(
                "Point out the new page first",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(15)],
                "They wrote IN my book? ...'Friends again'? Oh. Oh, that's... give me a moment. Something in my eye.",
                "{Giver} read the new page standing in the doorway, and went inside without a word. That night, the inn smelled of Two-Inn Stew.",
            ),
            give_up(
                &[],
                "The recipe book stays in the other inn, and both kitchens are poorer for it.",
            ),
        ],
        pitch: voices(
            &[
                "My recipe book's at the inn in {dest}. Fetch it. Carry it home in your hands. Not your cargo bay. Hands.",
            ],
            &[
                "My precious recipe book is over at the inn in {dest}! Could you fetch it and carry it home by hand? The cargo bay would ruin it!",
            ],
            &[
                "Every recipe I know is in one book, and the book is in {dest}. Would you carry it home in your own hands, like the treasure it is?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An innkeeper's recipe book must come home by hand.",
            "Collect the book in the next village and carry it home on foot.",
            "There's a new page in the book.",
        ),
    },
    ArcTemplate {
        id: ArcId(705),
        title: "Footprints in the Stone",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "Someone saw footprints pressed into the rock at an old formation, too small for any rig. Walk in and look at them yourself.",
        twist: "The footprints are pressed into solid stone, centuries old, and they're exactly the size of your boots.",
        consequence: "The giver measures your boots twice and goes very quiet.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(40)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Quirk(Quirk::TallTales),
            Trigger::Value(Value::Knowledge),
        ],
        roles: &[Role::Scholar, Role::Hermit, Role::Storyteller],
        slots: &[],
        targets: &[poi(PoiKind::Formation, 2, 5)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Reach the formation",
            "Walk in and see",
            "Bring the measurements home",
        ],
        steps: &[
            step(
                "{Giver} heard of footprints pressed into the stone at {dest}, {where}. Get there.",
                "Footprints. In stone. Small ones. No rig made them, that's certain.",
            ),
            step(
                "The prints are in a gap too narrow for the rig. Climb out and look at them with your own lamp.",
                "Measure them. Width, length, depth. And look for which way they're walking.",
            ),
            step(
                "You've measured them. You don't like what the measurements say. Take them up.",
                "Well? How big? Don't tell me on the radio. I want to see your face.",
            ),
        ],
        climax: "The prints march into the rock and stop at a blank wall. You set your boot beside one. Same length. Same width. Same worn heel.",
        twists: &[],
        reward: QuestReward::credits(820, 12),
        outcomes: &[
            ending(
                "Show them the measurements",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(402)),
                ],
                "Your size. Exactly your size. Centuries old. ...I'm going to make us both a very strong cup of tea, and we are not going to talk about it.",
                "{Giver} keeps a plaster cast of one footprint on their mantelpiece, beside a cast of your boot. They look at both a great deal and say nothing.",
            ),
            choice(
                "Say they were bigger than yours",
                &[
                    ArcEffect::Reputation(2),
                    ArcEffect::Mood(10),
                    ArcEffect::Credits(120),
                ],
                "Bigger? Oh, good. Good. For a moment there I thought... never mind what I thought.",
                "{Giver} believes the footprints were someone else's. You know better. Sometimes, deep down, you walk very carefully.",
            ),
            give_up(&[], "The footprints walk on into the stone, unmeasured."),
        ],
        pitch: voices(
            &[
                "Footprints in the stone at {dest}. Small ones. No rig fits in there. Go on foot. Measure them.",
            ],
            &[
                "Someone saw FOOTPRINTS pressed into solid stone at {dest}! Too small for a rig to reach! Would you walk in and measure them?",
            ],
            &[
                "There are footsteps in the rock at {dest}, walking somewhere, centuries old. Would you go in on foot and follow them as far as they go?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone saw small footprints pressed into stone at an old formation.",
            "Reach the formation, walk in on foot, and bring the measurements up.",
            "The footprints are your size.",
        ),
    },
    ArcTemplate {
        id: ArcId(706),
        title: "House Call",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "A healer can't leave their patients, but someone in the next village needs medicine carried in by hand, kept warm, not jostled in a cargo bay.",
        twist: "The 'patient' over there is the healer's own grandmother, who has been pretending to be sick so the healer will visit.",
        consequence: "The healer visits more often, and their grandmother makes a miraculous recovery every time.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Healer),
            Trigger::Value(Value::Family),
            Trigger::Fear(Fear::LosingFamily),
        ],
        roles: &[Role::Healer, Role::Brewer],
        slots: &[],
        targets: &[place(StoryPlace::Village)],
        stages: &[&[Objective::HandDeliver {
            target: 0,
            parcel: "warm medicine",
        }]],
        beats: &["Carry the medicine in by hand"],
        steps: &[step(
            "{Giver} has warm medicine for someone in {dest}, {where}. It mustn't be jostled: park, climb out and carry it to a door.",
            "Inside your coat, to keep it warm. Walk, don't run. Running's for emergencies and this is merely urgent.",
        )],
        climax: "The 'patient' answers the door with a spring in her step, takes the medicine, and says, 'Tell my grandchild I'm at death's door. Very poorly. They'd better come and see me.'",
        twists: &[],
        reward: QuestReward::credits(300, 10),
        outcomes: &[
            ending(
                "Tell them she's 'very poorly'",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Mood(15),
                    ArcEffect::Remember,
                ],
                "Very poorly? Oh no. I'll go tomorrow. ...She was faking it, wasn't she. She always fakes it. I'll go anyway.",
                "{Giver} visits their grandmother every week now. She makes a miraculous recovery every time, and relapses the day after they leave.",
            ),
            choice(
                "Tell them the truth",
                &[ArcEffect::Reputation(6), ArcEffect::Settlement(2)],
                "Faking it? To get me to visit? ...She could have just ASKED. Oh, I'm a terrible grandchild. I'm going right now.",
                "{Giver} turned up at their grandmother's door that very evening. They argued, they laughed, they had dinner. Now they have dinner every week.",
            ),
            give_up(
                &[],
                "The medicine went cold on the shelf. {Giver}'s grandmother sent a very pointed letter.",
            ),
        ],
        pitch: voices(
            &[
                "Medicine for someone in {dest}. Warm. Can't be jostled. Carry it in by hand. I can't leave my patients.",
            ],
            &[
                "I have medicine that needs to go to {dest}, {where}, warm and gentle! Could you carry it in by hand? Cargo bays rattle so!",
            ],
            &[
                "Some medicine works better carried in someone's hands than in a machine's belly. Would you walk this into {dest} for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A healer needs warm medicine carried into the next village by hand.",
            "Walk the medicine to a door in the next village.",
            "The patient was faking it, for visits.",
        ),
    },
    ArcTemplate {
        id: ArcId(707),
        title: "The Long Walk",
        kind: ArcKind::Pilot,
        contact_other: false,
        hook: "An old prospector walked the trail to the next settlement every year of their youth, on foot. Their knees won't let them anymore. Walk it for them.",
        twist: "Along the trail, carved into a boundary stone, are the prospector's initials and a date for every year they walked it, and room for more.",
        consequence: "The prospector's tradition goes on, with a new set of initials on the stone.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::RetiredMiner),
            Trigger::Value(Value::Tradition),
            Trigger::Want(Want::SeeTheWorld),
        ],
        roles: &[Role::RetiredMiner, Role::Prospector, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::Camp)],
        stages: &[
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &["Walk the old trail", "Say hello for them"],
        steps: &[
            step(
                "{Giver} walked the trail to {dest} every year of their youth. Their knees won't manage it now. Walk the last stretch for them, on foot.",
                "On foot. A rig sees the trail. Boots feel it. There's a difference.",
            ),
            meet(
                "You walked it. Now say hello at the camp for {giver}: {contact} will remember them.",
                "Say hello to whoever's at the fire. Say {giver} sends their best. They'll know the name.",
                "{Giver}? Still alive, the old goat? They used to walk in every summer singing at the top of their lungs. Tell them the stone's still there. Tell them there's room.",
            ),
        ],
        climax: "By the trail stands a boundary stone carved with {giver}'s initials and a date, again and again, forty times, one for every summer. Below the last one, there's room for more.",
        twists: &[],
        reward: QuestReward::credits(260, 9),
        outcomes: &[
            ending(
                "Carve your initials on the stone",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Cosmetic(CosmeticItem::Front(FrontAttachment::Floodlamp)),
                    ArcEffect::Remember,
                ],
                "You carved yours under mine? ...Then it keeps going. Forty-one. Do it again next year, would you? Someone should.",
                "Your initials are on the boundary stone now, under forty of {giver}'s. Every summer you walk the trail again, and {giver} asks how the stone is looking.",
            ),
            choice(
                "Carve their initials for them, one more time",
                &[ArcEffect::Reputation(12), ArcEffect::Mood(30)],
                "Mine? One more? Forty-one summers. ...Oh, that's a good number. That's a very good number.",
                "There are forty-one sets of {giver}'s initials on the stone now. The last one is slightly wobbly. {Giver} says it's the best one.",
            ),
            give_up(&[], "Nobody walked the trail this summer. The stone waits."),
        ],
        pitch: voices(
            &[
                "Walked that trail every summer for forty years. Knees won't do it now. Walk it for me. On foot.",
            ],
            &[
                "I walked the trail to {dest} every summer when I was young! My knees say no now, but would you walk it for me? On foot, the proper way?",
            ],
            &[
                "Forty summers I walked to {dest} on my own two feet. My knees have retired. Would you walk it once more, for an old prospector?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old prospector walked the trail every summer and can't anymore.",
            "Walk the trail on foot to the camp and say hello for them.",
            "Their initials are on a stone, forty times over.",
        ),
    },
];
