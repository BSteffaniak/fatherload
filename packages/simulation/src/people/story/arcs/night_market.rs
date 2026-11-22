//! Batch: the night market and the night's hooks (Wave 6). Stories at a village's trading post
//! after dusk ([`StoryPlace::NightMarket`]) and the night hooks WP-NIGHT offers: hunting night
//! monsters ([`Objective::NightHunt`]), seeing a whole night through to dawn
//! ([`Objective::Dawn`]) and bringing back night-only materials. Ids 1001-1006.

use super::{NO_THANKS, choice, ending, give_up, log, meet, place, step, voices};
use crate::{
    home::Furniture,
    people::{
        character::Role,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Trigger},
        traits::{Fear, Quirk, Value, Want},
    },
    quest::{Objective, QuestItem, QuestReward, QuestTier, StoryPlace},
    terrain::StrategicResourceKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(1001),
        title: "Silk for the Night Market",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A tailor at the night market will pay well for moth silk, and the moths only come out for lamplight. Hunt a few, watch the stalls, bring the silk.",
        twist: "The tailor isn't making clothes. She's weaving a lantern shade fine enough that the moths will circle it instead of the market's lamps.",
        consequence: "The night market gets a silk lantern of its own, and the stallholders stop swatting moths out of the soup.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::StrikeItRich),
            Trigger::Role(Role::Trader),
            Trigger::Value(Value::Craft),
        ],
        roles: &[Role::Trader, Role::Smuggler],
        slots: &[],
        targets: &[place(StoryPlace::NightMarket)],
        stages: &[
            &[Objective::NightHunt { count: 3 }],
            &[Objective::Vigil {
                target: 0,
                seconds: 20,
            }],
            &[Objective::Deliver {
                item: QuestItem::Material(StrategicResourceKind::MothSilk),
                count: 2,
            }],
        ],
        beats: &[
            "Hunt what comes for the lamps",
            "Watch over the night market",
            "Bring the moth silk",
        ],
        steps: &[
            step(
                "{Giver} needs moth silk for the night market's tailor. The swarms come out after dusk: bring a few down.",
                "Moths, hounds, anything that prowls. Night things all leave something worth having.",
            ),
            step(
                "Now keep watch over {dest}, {where}, while the stalls are lit and the moths come calling.",
                "Stand by the lamps. Moths always come to the lamps. So do customers, funnily enough.",
            ),
            step(
                "Bring {giver} two lengths of moth silk.",
                "Two will do. She weaves tight, that tailor. Tighter than my purse.",
            ),
        ],
        climax: "As the night market glows, the moths pour in from the dark toward the lamps, and the tailor watches them with a weaver's eye, measuring the silk in their wings.",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Hand over the silk",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Furniture(Furniture::Moonflower),
                    ArcEffect::Remember,
                ],
                "A silk shade for the market lamp, and not a moth in the soup all week. The tailor sends this moonflower with her thanks. It opens at night, she says, like the market.",
                "The night market's silk lantern draws every moth for a mile, and the stalls are calmer for it. A moonflower blooms on your windowsill every dusk.",
            ),
            choice(
                "Sell the silk to the highest bidder",
                &[ArcEffect::Credits(260), ArcEffect::Reputation(2)],
                "Sold it on? Well, it was your hunt. The tailor will find silk somewhere. Slower.",
                "The silk went to a buyer from two valleys over. The night market's moths still land in the soup.",
            ),
            give_up(
                &[],
                "No silk came. The market's lamps stay crowded with moths.",
            ),
        ],
        pitch: voices(
            &[
                "Night market tailor wants moth silk. Moths come out after dark. Hunt them. Bring silk.",
            ],
            &[
                "The night market's tailor pays a fortune for moth silk! Hunt a few swarms after dark and we'll both do well. Deal?",
            ],
            &[
                "When the market lamps are lit, the moths come silver out of the dark. The tailor needs their silk. Would you go out among them?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The night market's tailor needs moth silk.",
            "Hunt night creatures, watch the night market, then bring two lengths of moth silk.",
            "The tailor wove a silk lantern for the market.",
        ),
    },
    ArcTemplate {
        id: ArcId(1002),
        title: "Paper Lanterns",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "The giver has folded a hundred paper lanterns for the night market's festival, and someone has to carry them over by hand before the stalls open.",
        twist: "The lanterns are folded from the giver's old love letters, and the person they were written to is running a stall.",
        consequence: "The festival is lit by old love letters, and one stallholder recognises the handwriting.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::WritesLetters),
            Trigger::Want(Want::StrikeItRich),
            Trigger::Role(Role::Storyteller),
        ],
        roles: &[Role::Storyteller, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::NightMarket)],
        stages: &[
            &[Objective::HandDeliver {
                target: 0,
                parcel: "paper lanterns",
            }],
            &[Objective::Vigil {
                target: 0,
                seconds: 30,
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &[
            "Carry the lanterns over",
            "Stay for the festival",
            "Ask the stallholder who smiled",
        ],
        steps: &[
            step(
                "Carry {giver}'s paper lanterns by hand to {dest}, {where}. They're too delicate for a cargo bay.",
                "Don't crush them. Don't let them get wet. Don't read them. I mean, don't look too closely.",
            ),
            step(
                "The lanterns are hung. Stay through the evening while the festival glows.",
                "Watch who stops at the lanterns. Not for any reason. Just watch.",
            ),
            meet(
                "One stallholder stopped at a lantern and smiled. Go and ask {contact} what made them smile.",
                "Ask them. No, don't. Yes. Ask them. Casually. Very casually.",
                "Made me smile? The handwriting. I'd know {giver}'s hand anywhere. We wrote to each other one summer, years ago. I kept every letter. I always wondered why the replies stopped.",
            ),
        ],
        climax: "The lanterns glow along the market, and in their light the folded paper shows faint lines of handwriting. At one stall, {contact} stops dead, reads a line on the nearest lantern, and smiles.",
        twists: &[],
        reward: QuestReward::credits(350, 10),
        outcomes: &[
            ending(
                "Tell them who smiled",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "{Contact} smiled? At MY lantern? ...I wrote those letters twenty years ago and never sent the last one. Maybe I'll fold one more. A new one.",
                "{Giver} and {contact} were seen walking the night market together, arm in arm, under a hundred paper lanterns made of letters never sent.",
            ),
            choice(
                "Keep what you saw to yourself",
                &[ArcEffect::Reputation(5), ArcEffect::Mood(10)],
                "Nobody noticed? Oh. Well. They were pretty lanterns, at least. Weren't they?",
                "The lanterns were the talk of the festival. {Giver} never learned who smiled, and the letters stay unsent.",
            ),
            give_up(
                &[],
                "The lanterns stayed in their box, and the festival stayed dim.",
            ),
        ],
        pitch: voices(
            &[
                "Paper lanterns for the night market festival. Delicate. Carry them over by hand. Don't ask.",
            ],
            &[
                "I folded a hundred paper lanterns for the night market festival! They're far too delicate for a rig. Would you carry them over for me?",
            ],
            &[
                "Every lantern I folded holds a little of something I never said. Would you carry them to the night market, and tell me who stops to look?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager folded paper lanterns for the night market festival.",
            "Carry the lanterns by hand to the night market, then stay for the festival.",
            "The lanterns were folded from old love letters.",
        ),
    },
    ArcTemplate {
        id: ArcId(1003),
        title: "Until the Sun Comes Up",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A warden is too old to keep the dawn watch and too proud to say so. Keep watch from dusk and stay out until the sun is up.",
        twist: "The warden was out there too, all night, in the dark by the well, keeping an eye on you keeping an eye on the village.",
        consequence: "The warden passes the dawn watch on, and finally sleeps through a night.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::ProtectVillage),
            Trigger::Fear(Fear::CaveIns),
            Trigger::Quirk(Quirk::MadeUpProverbs),
        ],
        roles: &[Role::Warden, Role::RetiredMiner],
        slots: &[],
        targets: &[place(StoryPlace::Home)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 20,
            }],
            &[Objective::Dawn { target: 0 }],
        ],
        beats: &["Take the dusk watch", "See the night through"],
        steps: &[
            step(
                "{Giver} wants someone to keep the watch over {village} from dusk. Stand watch after dark.",
                "Walk the edge, then stand still. Things move when you stand still. You'll see them.",
            ),
            step(
                "Stay out in {village} until the night turns to dawn. Don't go to bed.",
                "Until the sky goes grey. Not before. The last hour is the one that counts.",
            ),
        ],
        climax: "As the sky greys over {village}, a figure stands up from the dark beside the well, stiff and yawning: {giver}, who stayed out all night watching you watch the village.",
        twists: &[],
        reward: QuestReward::credits(380, 11),
        outcomes: &[
            ending(
                "Take over the dawn watch",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Settlement(3),
                    ArcEffect::Furniture(Furniture::StarChart),
                    ArcEffect::Remember,
                ],
                "You stayed till the grey. Good. The watch is yours when you're here. Take my star chart: it's how I passed the long hours. I won't need it. I intend to SLEEP.",
                "{Giver} sleeps through the night now, mostly. When you're in {village} after dark, the warden's lamp is yours to carry.",
            ),
            choice(
                "Tell them they should rest",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(15)],
                "Rest? ...You sound like my daughter. Fine. FINE. One night a week.",
                "{Giver} rests one night a week now, under protest, and the village takes turns at the watch.",
            ),
            give_up(&[], "{Giver} keeps the dawn watch alone, yawning."),
        ],
        pitch: voices(
            &["Need someone on the night watch. Dusk to dawn. Don't fall asleep."],
            &[
                "The night watch needs fresh eyes! Keep watch from dusk and stay up to see the sunrise with me. It's lovely, honestly!",
            ],
            &[
                "The last hour before dawn is the longest one, and lately I can't keep my eyes open for it. Would you keep the watch, all the way to morning?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old warden needs someone to keep the dawn watch.",
            "Keep watch in the village after dark, then stay out until dawn.",
            "The warden watched the watcher all night.",
        ),
    },
    ArcTemplate {
        id: ArcId(1004),
        title: "The Hound at the Market Gate",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "Something big has been prowling the edge of the night market and scaring the customers off. Watch for it, bring it down, and tell the stallholders.",
        twist: "The hounds come because a stallholder has been feeding them scraps, and has named every one of them.",
        consequence: "The market gets a fence and a strict no-feeding rule, and one stallholder sulks.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Fear(Fear::DeepThings),
            Trigger::Role(Role::Courier),
            Trigger::Value(Value::Community),
        ],
        roles: &[Role::Warden, Role::Courier],
        slots: &[],
        targets: &[place(StoryPlace::Home), place(StoryPlace::NightMarket)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 20,
            }],
            &[Objective::NightHunt { count: 2 }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Watch the edge of town",
            "Bring the prowlers down",
            "Tell the night market",
        ],
        steps: &[
            step(
                "Watch the dark edge of {village} after dusk. The prowlers come from there.",
                "They're big. They're quiet. They know exactly where the lamps stop.",
            ),
            step(
                "There they are. Bring down two of the night prowlers before they reach the stalls.",
                "Two should send the rest packing. Night things aren't brave, just hungry.",
            ),
            meet(
                "Tell {contact} at {dest2}, {where2}, that the prowlers are dealt with.",
                "The stallholders will want to hear it from you. They don't believe a word I say.",
                "Dealt with? ...Oh no. Not Biscuit. Tell me it wasn't Biscuit. I may have been feeding them. A little. Every night.",
            ),
        ],
        climax: "The last prowler slinks off toward the night market's back alley, where a stallholder is waiting with a plate of scraps and a name for every one of them.",
        twists: &[],
        reward: QuestReward::credits(1_100, 13),
        outcomes: &[
            ending(
                "Insist on a fence and no feeding",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
                "A fence and a rule. The customers are back and the stallholder is sulking. That's a good night's work.",
                "The night market has a palisade now, and a hand-painted sign: DO NOT FEED THE HOUNDS. Underneath, smaller: ESPECIALLY BISCUIT.",
            ),
            choice(
                "Let the stallholder keep feeding them",
                &[ArcEffect::Reputation(-3), ArcEffect::Settlement(-2)],
                "You told them it was FINE? The customers won't come back now. ...I suppose the hounds will be happy.",
                "The hounds are regulars at the night market now. So are fewer customers. The stallholder has never been happier.",
            ),
            give_up(
                &[],
                "The prowlers keep circling the market, and the customers stay home.",
            ),
        ],
        pitch: voices(
            &[
                "Something prowls the night market. Scares the customers. Watch for it. Deal with it.",
            ],
            &[
                "Big shapes have been prowling round the night market and nobody dares shop after dark! Will you watch for them and sort it out?",
            ],
            &[
                "Every night, at the edge of the lamps, something large waits and watches the market. The stalls empty early now. Would you face it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Something prowls the edge of the night market.",
            "Keep watch after dark, bring down two night creatures, then tell the market.",
            "A stallholder had been feeding the hounds.",
        ),
    },
    ArcTemplate {
        id: ArcId(1005),
        title: "A Pelt for the Tanner",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A wanderer at a camp wants to prove to their family that they can live off the open land. Hunt with them through a night and bring back a pelt.",
        twist: "The wanderer is terrified of the dark and has never once stayed out past dusk; you're the bravest thing about the whole plan.",
        consequence: "The wanderer goes home with a pelt, a story and a new respect for the night.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::StrikeItRich),
            Trigger::Value(Value::Adventure),
            Trigger::Fear(Fear::LosingFamily),
        ],
        roles: &[Role::Apprentice, Role::Prospector],
        slots: &[],
        targets: &[place(StoryPlace::Camp)],
        stages: &[
            &[Objective::NightHunt { count: 2 }],
            &[Objective::Dawn { target: 0 }],
            &[Objective::Deliver {
                item: QuestItem::Material(StrategicResourceKind::ShadowPelt),
                count: 1,
            }],
        ],
        beats: &[
            "Hunt through the dark",
            "Make it back to camp by dawn",
            "Hand over the pelt",
        ],
        steps: &[
            step(
                "{Giver} wants to hunt the open land through a night. Bring down two night prowlers.",
                "I'll be right behind you. Well behind you. At a strategic distance.",
            ),
            step(
                "Get back to {dest}, {where}, and be there, awake, when the sun comes up.",
                "Dawn at the campfire. That's how the old stories end. I checked.",
            ),
            step(
                "Give {giver} a shadow pelt to take home as proof.",
                "Just the one. My family doesn't need to know you did most of it.",
            ),
        ],
        climax: "The sun comes up over the camp and {giver}, who spent most of the night with a blanket over their head, lets out a whoop like they wrestled every hound themselves.",
        twists: &[],
        reward: QuestReward::credits(1_050, 12),
        outcomes: &[
            ending(
                "Let them tell it their way",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(20),
                    ArcEffect::Remember,
                ],
                "A pelt and a dawn! My family will never believe it. Which is fine, because I barely do. Thank you for not mentioning the blanket.",
                "In {giver}'s telling, they fought six hounds bare-handed under a blood moon. You come into it near the end, holding a lamp. You don't mind.",
            ),
            choice(
                "Tell them the night is nothing to be ashamed of",
                &[ArcEffect::Reputation(9), ArcEffect::Mood(10)],
                "Scared and out there anyway? ...Huh. When you put it like that, it sounds brave.",
                "{Giver} tells it true: scared stiff, out all night, home at dawn. People like that version better.",
            ),
            give_up(&[], "{Giver} went home without a pelt or a story."),
        ],
        pitch: voices(
            &[
                "Want to show my family I can live rough. Hunt with me through a night. Bring back a pelt.",
            ],
            &[
                "I'm going to prove I can live off the open land! A night's hunting, a pelt to show for it! Will you come? Say you'll come.",
            ],
            &[
                "My family thinks I'd never last a night out there. They're probably right. Would you hunt with me until dawn, so they're wrong?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A wanderer wants to prove they can live off the open land.",
            "Hunt through a night, be at the camp at dawn, then hand over a pelt.",
            "The wanderer was afraid of the dark all along.",
        ),
    },
    ArcTemplate {
        id: ArcId(1006),
        title: "Whispers at the Night Market",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A broker at the night market sells secrets that come true by morning. The giver wants to know how.",
        twist: "The broker doesn't predict anything; they just stay up all night and watch, and sell what they saw at dawn.",
        consequence: "The secret's out: the best way to know what the night brings is to stay up for it.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::KnowsEveryonesBusiness),
            Trigger::Value(Value::Honesty),
            Trigger::Role(Role::Cartographer),
        ],
        roles: &[Role::Cartographer, Role::Storyteller],
        slots: &[],
        targets: &[place(StoryPlace::NightMarket)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Vigil {
                target: 0,
                seconds: 25,
            }],
            &[Objective::Dawn { target: 0 }],
        ],
        beats: &[
            "Buy a secret",
            "Watch the market after dark",
            "See whether it comes true",
        ],
        steps: &[
            meet(
                "Find the secret broker, {contact}, at {dest}, {where}, and buy tonight's secret.",
                "Ask what happens tonight. They'll tell you, for a price. Then we'll see.",
                "Tonight's secret? A shooting star over the west stall, and a cart that loses a wheel. True by morning, guaranteed. That'll be a smile, for you.",
            ),
            step(
                "Keep watch over the night market after dark and see what happens.",
                "Watch the broker, not the sky. Watch their hands. Watch their eyes.",
            ),
            step(
                "Stay at the market until dawn breaks and see whether the secret came true.",
                "Till dawn. That's when the broker sells the next one. I'll bet that's no coincidence.",
            ),
        ],
        climax: "Through the night the broker never sleeps: they sit at the stall's edge watching every cart, every star, every face, and at dawn they write it all down for tomorrow's customers.",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Reveal the broker's method",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(2),
                    ArcEffect::Remember,
                ],
                "They just stay UP? That's the whole trick? ...I'm almost disappointed. Almost. I'm going to try it.",
                "Half of {village} stays up late at the night market now, watching for tomorrow's secrets. The broker has had to lower prices.",
            ),
            choice(
                "Go into business with the broker",
                &[ArcEffect::Credits(300), ArcEffect::Reputation(-2)],
                "You're SELLING them now? With the broker? ...Save me a good one.",
                "There are two secret brokers at the night market now. One of them looks remarkably like you, and yawns a lot.",
            ),
            give_up(&[], "The broker keeps the trick, and keeps selling."),
        ],
        pitch: voices(
            &["Broker at the night market sells secrets that come true by morning. Find out how."],
            &[
                "There's a broker at the night market whose secrets ALWAYS come true by morning! I have to know the trick. Will you find out?",
            ],
            &[
                "They say at the night market there's someone who sells tomorrow, a secret at a time. It frightens me a little. Would you learn how it's done?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A night market broker sells secrets that come true by morning.",
            "Buy a secret, watch the night market, then stay until dawn.",
            "The broker just stays up and watches.",
        ),
    },
];
