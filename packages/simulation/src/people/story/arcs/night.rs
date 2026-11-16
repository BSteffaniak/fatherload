//! Batch: night arcs. Watches, lanterns, sleepwalkers and stargazing: beats that count only
//! while it is night (Part II F4's clock). Ids 601-607.

use super::{NO_THANKS, choice, ending, feature, give_up, log, meet, place, step, voices};
use crate::{
    cosmetics::{CosmeticItem, FlameColor, RearAttachment},
    people::{
        character::{AbsentFate, Role},
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger, VillageProblem},
        traits::{Fear, Quirk, Value},
    },
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace, UpgradeStat},
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(601),
        title: "Night Watch",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "The warden keeps hearing something prowl around the village at night. Keep watch from dusk and find out what.",
        twist: "It's the baker's cat, who has been stealing one bread roll a night from every doorstep for a year.",
        consequence: "The mystery is solved, the cat is unrepentant, and the village leaves out a roll for it on purpose now.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::Warden),
            Trigger::Want(crate::people::traits::Want::ProtectVillage),
            Trigger::Problem(VillageProblem::CaveSwarm),
        ],
        roles: &[Role::Warden, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 40,
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &["Keep watch after dark", "Report what you saw"],
        steps: &[
            step(
                "{Giver} hears something prowling round {village} every night. Keep watch here after dark.",
                "It comes after the inn shuts. Soft footsteps. Rustling. Every night. Every single night.",
            ),
            meet(
                "You've seen the culprit. Tell {contact} first: they'll want to hear before the warden does.",
                "Who was it? No, tell {contact} first. They've had rolls go missing too.",
                "The baker's CAT? Stealing a roll a night? For a YEAR? ...That cat has more nerve than the whole village put together.",
            ),
        ],
        climax: "Just past midnight, a fat ginger shadow slips along the doorsteps, takes one bread roll from each, and trots off with its tail high. The baker's cat.",
        twists: &[],
        reward: QuestReward::credits(300, 9),
        outcomes: &[
            ending(
                "Reveal the culprit",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "A CAT. A whole year of dread and it was a CAT. ...I'm leaving it a roll. Out of respect.",
                "{Village} leaves a bread roll on the warden's step every night now, for the cat. The cat is fatter and smugger than ever. The warden sleeps fine.",
            ),
            choice(
                "Cover for the cat",
                &[ArcEffect::Reputation(-2), ArcEffect::Credits(40)],
                "Nothing? You saw nothing all night? ...Well, I suppose it must have been the wind. The wind that eats bread.",
                "You never told. The cat sits on your rig's bonnet whenever you visit, purring, like an accomplice.",
            ),
            give_up(
                &[],
                "The prowler goes on prowling. The rolls go on vanishing. Nobody suspects the cat.",
            ),
        ],
        pitch: voices(
            &[
                "Something prowls the village after dark. Every night. Keep watch. Tell me what it is.",
            ],
            &[
                "Something sneaks around the village every night and I can't catch it! Would you keep watch after dark? It's probably nothing! Probably!",
            ],
            &[
                "Every night, after the lamps go out, something walks our street. I lie awake listening. Would you watch, so I don't have to?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Something prowls the village at night. Keep watch and find out what.",
            "Keep watch in the village after dark, then report what you saw.",
            "It was the baker's cat.",
        ),
    },
    ArcTemplate {
        id: ArcId(602),
        title: "Lanterns for the Lost",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "On the anniversary of their loved one going deep, the giver keeps a vigil with lanterns at the mine head. This year they'd like company.",
        twist: "Far below, as the lanterns burn, a faint blue light flickers back up the shaft, just once.",
        consequence: "The giver feels heard, and lights a lantern every year with a little less sorrow.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Absent(AbsentFate::WentDeep),
            Trigger::Value(Value::Tradition),
            Trigger::Fear(Fear::BeingForgotten),
        ],
        roles: &[Role::ShrineKeeper, Role::Healer],
        slots: &[Slot::Absent(AbsentFate::WentDeep)],
        targets: &[place(StoryPlace::Home)],
        stages: &[&[Objective::Vigil {
            target: 0,
            seconds: 45,
        }]],
        beats: &["Keep the lantern vigil"],
        steps: &[step(
            "Tonight is the anniversary of {focus}, {giver}'s {focus_relation}, going deep. Keep the lantern vigil with {giver} after dark.",
            "Just sit with me. You don't have to say anything. The lanterns do the talking.",
        )],
        climax: "Near the end of the night, deep down the old shaft, a faint blue light flickers once, then is gone. {Giver} squeezes your arm and says nothing at all.",
        twists: &[],
        reward: QuestReward::credits(260, 11),
        outcomes: &[
            ending(
                "Stay until dawn",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Mood(25),
                    ArcEffect::Cosmetic(CosmeticItem::Rear(RearAttachment::Lantern)),
                    ArcEffect::Remember,
                ],
                "You saw it too, didn't you. The blue. ...Thank you for staying. Take a lantern. For your own long nights.",
                "{Giver} lights a lantern at the mine head every year on {focus}'s night. They say it feels less like mourning now and more like saying hello.",
            ),
            choice(
                "Leave a lantern burning down the shaft",
                &[ArcEffect::Reputation(9), ArcEffect::Settlement(3)],
                "Down the shaft? So {focus} can see it? ...Yes. Let's.",
                "A lantern burns at the bottom of the old shaft now, kept lit by {village} in turns. Every so often, something down there seems to trim its wick.",
            ),
            give_up(&[], "{Giver} kept the vigil alone this year, as always."),
        ],
        pitch: voices(
            &[
                "Tonight's the night {focus} went down. I keep a lantern vigil. Most years alone. You could sit with me.",
            ],
            &[
                "Every year I light lanterns for my {focus_relation} {focus}, who went deep. Would you keep me company tonight? It's nicer with two!",
            ],
            &[
                "The night {focus} went down, I light lanterns at the mine head and wait for morning. Would you sit with me this year?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager keeps a lantern vigil for a relative who went deep.",
            "Keep the vigil in the village through the night.",
            "Something deep down flickered back.",
        ),
    },
    ArcTemplate {
        id: ArcId(603),
        title: "The Stargazer's Ledge",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A scholar charting the stars wants observations from somewhere high, above the haze, through a whole night.",
        twist: "One 'star' moves against all the others: a steady lamp, far up past the dead air. Someone, or something, lives up there.",
        consequence: "The scholar's star chart has a new mark labelled 'Lamp?', and a very long list of questions.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Role(Role::Scholar),
            Trigger::Value(Value::Wonder),
            Trigger::Quirk(Quirk::WeatherWatcher),
        ],
        roles: &[Role::Scholar, Role::Cartographer],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Vigil {
                target: 0,
                seconds: 50,
            }],
        ],
        beats: &["Climb above the haze", "Watch the stars all night"],
        steps: &[
            step(
                "{Giver} wants star observations from above the haze: {dest}, {where}.",
                "Higher than the smoke from the inn chimney. Much higher. The inn makes a lot of smoke.",
            ),
            step(
                "Settle in and watch the stars wheel overhead till the night is done.",
                "Watch for anything that moves wrong. Stars are reliable. Anything unreliable is interesting.",
            ),
        ],
        climax: "Hours in, one star refuses to wheel with the others. It holds steady, warm and yellow, far up past the dead air. Not a star. A lamp.",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Show them the moving light",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "A lamp? Up THERE? Past the dead air? ...Then someone lit it. I've charted those stars for thirty years and nobody ever told me someone lives in them.",
                "{Giver}'s star chart has a new mark, circled three times: 'Lamp?' Underneath, a growing list of questions. You're mentioned in most of them.",
            ),
            choice(
                "Signal back with your floodlamps",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Cosmetic(CosmeticItem::Flame(FlameColor::Prism)),
                ],
                "You SIGNALLED it? Did it answer? ...It blinked? Twice? I'm going to need a much bigger notebook.",
                "You flashed your lamps at the far light. It blinked twice. {Giver} has been up every clear night since, flashing a lantern at the sky.",
            ),
            give_up(
                &[],
                "{Giver} charts the stars from the inn roof, through the smoke, and never sees the lamp.",
            ),
        ],
        pitch: voices(
            &[
                "Need star observations from above the haze. All night. Up at {dest}. Take a notebook.",
            ],
            &[
                "I'm charting every star in the sky and I need clear observations from way up at {dest}! Would you spend a night up there for me?",
            ],
            &[
                "From down here the stars are smudged by chimney smoke and lamplight. From {dest} they must be sharp as needles. Would you watch them for me, one whole night?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A scholar wants star observations from above the haze, all night.",
            "Reach the high place and keep watch through the night.",
            "One of the stars is a lamp.",
        ),
    },
    ArcTemplate {
        id: ArcId(604),
        title: "The Sleepwalker",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "Someone in the village sleepwalks to the old diggings every night. The healer wants to know why, without waking them.",
        twist: "The sleepwalker goes to the cave mouth and listens. Down below, the blue lights come up to meet them, and they talk.",
        consequence: "The sleepwalker's family keeps the cave mouth lit, and the sleepwalker's dreams are, they say, wonderful.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(20)],
        triggers: &[
            Trigger::Role(Role::Healer),
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Fear(Fear::DeepThings),
        ],
        roles: &[Role::Healer, Role::ShrineKeeper],
        slots: &[],
        targets: &[feature(FeatureKind::Cave, 1, 2), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 40,
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &["Follow the sleepwalker", "Tell the family"],
        steps: &[
            step(
                "Someone in {village} sleepwalks to {dest}, {where}, every night. {Giver} wants you to watch, quietly.",
                "Don't wake them! Waking a sleepwalker is bad luck. Also they bite. This one bites.",
            ),
            meet(
                "You saw where the sleepwalker goes, and what met them there. Tell the family: {contact} will know how to break it gently.",
                "Tell {contact} first. They're family. They'll know what to do.",
                "Talking? To LIGHTS? ...You know, Gran always said our family had 'deep ears'. I thought she meant we were nosy.",
            ),
        ],
        climax: "At the cave mouth the sleepwalker kneels and speaks softly into the dark. From far below, blue lights rise, flickering in time with the words, like someone nodding along.",
        twists: &[],
        reward: QuestReward::credits(850, 12),
        outcomes: &[
            ending(
                "Tell them it's harmless",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Mood(15),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(401)),
                ],
                "Harmless, and talking to the deep. Well. I've seen stranger in my clinic. Not much stranger. But some.",
                "The sleepwalker's family keeps a lamp at the cave mouth now, and a blanket. The sleepwalker says their dreams are full of blue light and kind voices.",
            ),
            choice(
                "Suggest they lock the door at night",
                &[ArcEffect::Reputation(4), ArcEffect::Mood(-10)],
                "Lock them in? It would be safer. ...It would also be very sad, wouldn't it. I'll think about it.",
                "The sleepwalker's door is locked at night now. They sleep badly, and the lights at the cave mouth come up and wait, and wait.",
            ),
            give_up(
                &[],
                "The sleepwalker keeps walking. Nobody follows to see where.",
            ),
        ],
        pitch: voices(
            &[
                "Patient of mine sleepwalks to {dest} every night. Watch where they go. Don't wake them.",
            ],
            &[
                "One of my patients sleepwalks to the old diggings every night! Would you follow them, quietly, and see what they're doing? It's a medical mystery!",
            ],
            &[
                "Every night someone in this village rises in their sleep and walks to {dest}, as if called. Would you follow, and see who's calling?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone sleepwalks to the old diggings every night.",
            "Keep watch at the cave through the night, then tell the family.",
            "The sleepwalker talks to the lights.",
        ),
    },
    ArcTemplate {
        id: ArcId(605),
        title: "The Birds Are Up to Something",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "The giver is certain the birds are plotting. They gather on the roof every night. Watch them and find out.",
        twist: "They're building a nest on the giver's chimney, a huge, careful, beautiful nest, with a scrap of the giver's old scarf woven in.",
        consequence: "The giver distrusts birds a little less, and has a lodger with feathers.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::MistrustsBirds),
            Trigger::Quirk(Quirk::Superstitious),
            Trigger::Relationship(RelationshipKind::Rival),
        ],
        roles: &[Role::Farmer, Role::Baker],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[&[Objective::Vigil {
            target: 0,
            seconds: 35,
        }]],
        beats: &["Watch the roof all night"],
        steps: &[step(
            "{Giver} is convinced the birds of {village} are plotting. They gather on the roof every night. Watch them.",
            "Look at them. LOOK at them. Sitting there. Planning. Don't let them see you looking.",
        )],
        climax: "In the moonlight the birds work in shifts: one brings a twig, another tucks it in, a third adds a scrap of red wool. It's a nest, enormous and careful, right on {giver}'s chimney.",
        twists: &[],
        reward: QuestReward::credits(240, 8),
        outcomes: &[
            ending(
                "Tell them it's a nest",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Mood(20),
                    ArcEffect::Remember,
                ],
                "A NEST? On MY chimney? With my old scarf in it? ...The cheek. The absolute cheek. ...It's a very nice nest, though, isn't it.",
                "{Giver} has a family of birds on the chimney now and pretends to be furious about it. You've seen them leaving out crumbs.",
            ),
            choice(
                "Confirm the plot",
                &[
                    ArcEffect::Reputation(4),
                    ArcEffect::Mood(-5),
                    ArcEffect::Credits(40),
                ],
                "I KNEW it. I knew they were up to something. Did you see the ringleader? Big one? Smug?",
                "{Giver} is more convinced than ever that the birds are plotting. Now they have a witness. The birds, meanwhile, have chicks.",
            ),
            give_up(
                &[],
                "The birds finish their nest unobserved. {Giver} is still suspicious.",
            ),
        ],
        pitch: voices(
            &[
                "Birds gather on my roof every night. They're planning something. Watch them. Tell me what.",
            ],
            &[
                "The birds have been gathering on my roof every night and I KNOW they're up to something! Would you keep watch one night? I'll make cocoa!",
            ],
            &[
                "Every night they come, the birds, and sit in rows on my roof, whispering. I'm not mad. Would you watch with me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone is sure the birds are plotting on their roof at night.",
            "Keep watch over the village through the night.",
            "The birds were building a nest.",
        ),
    },
    ArcTemplate {
        id: ArcId(606),
        title: "Dark Road Home",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A traveller at the camp is frightened of the dark road. Keep watch over the camp tonight so they can sleep, then walk them to a village in the morning.",
        twist: "They're not scared of the dark at all. They're scared of being alone in it. They just needed someone to sit up with.",
        consequence: "The traveller arrives safe and finds the nerve to keep travelling.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Fear(Fear::TheDark),
            Trigger::Role(Role::Trader),
            Trigger::Value(Value::Freedom),
        ],
        roles: &[Role::Trader, Role::Courier, Role::Apprentice],
        slots: &[],
        targets: &[place(StoryPlace::Home), place(StoryPlace::Village)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 35,
            }],
            &[Objective::Reach { target: 1 }],
        ],
        beats: &[
            "Sit up through the night",
            "See them safe to the next village",
        ],
        steps: &[
            step(
                "{Giver} is frightened of the dark road. Keep watch over {village} tonight so they can sleep.",
                "Just be awake. That's all. Somebody awake. I'll be fine if somebody's awake.",
            ),
            step(
                "Morning. {Giver} wants to reach {dest2}, {where2}. Go on ahead and make sure the road's clear.",
                "Is the road clear? It's light now. It's always clearer in the light.",
            ),
        ],
        climax: "Somewhere in the small hours, {giver} stops pretending to be asleep and sits up beside you. 'It's not the dark,' they say. 'It's being the only one awake in it.'",
        twists: &[],
        reward: QuestReward::credits(280, 9),
        outcomes: &[
            ending(
                "See them safely on",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "We made it. And I slept! Actually slept! ...Maybe the next road won't be so bad.",
                "{Giver} travels on from {dest2} with a lighter step. Every so often a postcard arrives for you: 'Slept fine. Thinking of you, awake.'",
            ),
            choice(
                "Teach them a song for the dark",
                &[ArcEffect::Reputation(8), ArcEffect::Settlement(2)],
                "A song? For walking at night? ...Hum it again. Slower. I'll learn it.",
                "{Giver} hums your song on every night road now. They've taught it to half the travellers on the trail.",
            ),
            give_up(
                &[],
                "{Giver} waits at the camp for someone else to sit up with them.",
            ),
        ],
        pitch: voices(
            &[
                "Dark road scares me. Sit up tonight while I sleep. Then see me to the next village. I'll pay.",
            ],
            &[
                "I know, I know, a grown traveller scared of the dark! Would you keep watch tonight, then see me safe to the next village? I'll pay well!",
            ],
            &[
                "The dark out here is so very big, and I'm so very small in it. Would you sit up tonight, so I'm not the only one awake?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A frightened traveller needs someone to keep watch through the night.",
            "Keep watch through the night, then see the road to the next village is clear.",
            "They just needed someone awake.",
        ),
    },
    ArcTemplate {
        id: ArcId(607),
        title: "Glowcap Night",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "Once a year the glowcaps in the shallow caves bloom, but only at night. A brewer wants them picked at their brightest.",
        twist: "When the glowcaps bloom, the cave fills with light so bright you can read by it, and the walls are covered in old miners' initials and hearts.",
        consequence: "Glowcap ale returns to the inn, and couples start carving their initials in the cave again.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(15)],
        triggers: &[
            Trigger::Role(Role::Brewer),
            Trigger::Value(Value::Tradition),
            Trigger::Relationship(RelationshipKind::Sweetheart),
        ],
        roles: &[Role::Brewer, Role::Innkeeper, Role::Healer],
        slots: &[],
        targets: &[feature(FeatureKind::Cave, 1, 3)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 30,
            }],
            &[Objective::Dig {
                target: 0,
                count: 5,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Wait for the bloom", "Pick the glowcaps", "Bring them up"],
        steps: &[
            step(
                "{Giver}'s glowcap ale needs glowcaps picked at their brightest, at night, in {dest}, {where}. Wait for the bloom.",
                "They bloom after midnight. Not before. Not after dawn. Glowcaps keep strict hours.",
            ),
            step(
                "The cave is blazing with light. Pick the glowcaps, gently, before they fade.",
                "Only the bright ones. The dim ones make the ale taste of socks.",
            ),
            step(
                "A bay full of glowing caps. Bring them up before they dim.",
                "Quick! They dim by sunrise. Glowcap ale from dim caps is just ale. Sad ale.",
            ),
        ],
        climax: "At midnight the glowcaps open all at once and the cave blazes gold. On the walls, lit for the first time in years: initials, hearts, dates, a hundred years of miners' couples.",
        twists: &[],
        reward: QuestReward::credits(800, 12),
        outcomes: &[
            ending(
                "Deliver the glowcaps",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Settlement(4),
                    ArcEffect::Cosmetic(CosmeticItem::Paint(crate::cosmetics::PaintColor::Glowcap)),
                    ArcEffect::Remember,
                ],
                "Bright as lamps! The ale will be the best in years. ...You saw the walls, didn't you? The hearts? My grandparents are on there somewhere.",
                "Glowcap ale is back at {village}'s inn. On bloom night, young couples go down to the cave to carve their initials beside a hundred years of others.",
            ),
            choice(
                "Carve your own initials in the cave",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(15)],
                "You carved yours? With whose? ...Nobody's? Just yours? Well, there's room for another set beside it. When you're ready.",
                "Your initials are in the glowcap cave now, by themselves, with a lot of space beside them. {Giver} winks every time it comes up.",
            ),
            give_up(
                &[],
                "The glowcaps bloomed and faded with no one to see. There's no ale this year.",
            ),
        ],
        pitch: voices(
            &[
                "Glowcaps bloom tonight in {dest}. Only at night. Pick them at their brightest. Bring them up before dawn.",
            ],
            &[
                "It's glowcap night! The caps in {dest} only bloom after midnight, once a year! Would you pick me a bay full? The ale will be legendary!",
            ],
            &[
                "Once a year, the shallow caves remember how to shine. Tonight is the night. Would you go down to {dest} and bring me some of the light?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Glowcaps bloom once a year, only at night. A brewer wants them picked.",
            "Wait for the bloom in the cave at night, pick the glowcaps, and bring them up.",
            "The cave blazed with light and old initials.",
        ),
    },
];
