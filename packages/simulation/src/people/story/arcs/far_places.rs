//! Batch: homesteads, sky towns and the Moon's colony (Wave 6). Stories on the land office's
//! empty plots ([`StoryPlace::Homestead`]), in the archipelago's sky towns
//! ([`StoryPlace::SkyTown`], whose folk are contacts) and at the lunar colony (the Moon's story
//! place, whose colonists are contacts). Furniture rewards furnish the homes these stories are
//! about. Ids 1101-1103, 1201-1204, 1301-1304.

use super::{NO_THANKS, choice, ending, give_up, log, meet, place, step, voices};
use crate::{
    base::ModuleKind,
    home::Furniture,
    infrastructure::InfrastructureKind,
    people::{
        character::{AbsentFate, Role},
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger},
        traits::{Fear, Quirk, Value, Want},
    },
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace, UpgradeStat},
    terrain::StrategicResourceKind,
};

pub const BATCH: &[ArcTemplate] = &[
    // --- Homesteads -------------------------------------------------------------------------
    ArcTemplate {
        id: ArcId(1101),
        title: "Breaking Ground",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A farmer has had an eye on an empty plot for years and wants someone to look it over and prove a homestead can stand out there.",
        twist: "The plot's soil reading shows an old garden under the grass, laid out in neat rows by someone long gone.",
        consequence: "A homestead stands where the old garden was, and the farmer plants the first row again.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Role(Role::Farmer),
            Trigger::Value(Value::Freedom),
        ],
        roles: &[Role::Farmer, Role::Smith],
        slots: &[],
        targets: &[place(StoryPlace::Homestead)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Settle {
                kind: ModuleKind::Homestead,
            }],
        ],
        beats: &[
            "Walk the empty plot",
            "Read the ground",
            "Raise a homestead",
        ],
        steps: &[
            step(
                "{Giver} has had an eye on {dest} for years: {where}. Go and look it over.",
                "Flat, sunny, nobody on it. I've walked past it a hundred times. I never stopped.",
            ),
            step(
                "Pulse the scanner over the plot to see what the ground is made of.",
                "Soil's what matters. Rock you can move. Bad soil you live with.",
            ),
            step(
                "The ground is good. Prove it by raising a homestead module in a claim of your own.",
                "One homestead standing, and I'll believe a person can live out there.",
            ),
        ],
        climax: "Under the scanner the plot's soil shows old furrows, ruler-straight, a garden planted long ago and left to the grass. Someone loved this ground once.",
        twists: &[],
        reward: QuestReward::credits(420, 11),
        outcomes: &[
            ending(
                "Plant the first row again",
                &[
                    ArcEffect::Reputation(11),
                    ArcEffect::Furniture(Furniture::RockingChair),
                    ArcEffect::Remember,
                ],
                "An old garden under the grass! Then it's not new ground at all, it's waiting ground. Take this rocking chair for your porch. Every homestead needs one.",
                "Cabbages grow in neat rows on the old plot again. {Giver} visits on rest days, just to look.",
            ),
            choice(
                "Keep the garden secret and build",
                &[ArcEffect::Reputation(5), ArcEffect::Credits(80)],
                "Build first, garden later. Practical. I can respect practical. Here, for the lumber.",
                "The homestead went up fast. Every spring, something green comes up through the yard in neat rows anyway.",
            ),
            give_up(&[], "The plot stays empty. {Giver} keeps walking past it."),
        ],
        pitch: voices(
            &["Empty plot out past the village. Look it over. See if a homestead stands there."],
            &[
                "There's a lovely empty plot I've dreamed about for years! Will you look it over and see if a homestead could stand there?",
            ],
            &[
                "I walk past an empty plot every day and imagine a house on it, a lamp in the window. Would you see whether the dream could stand?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A farmer has dreamed of homesteading an empty plot.",
            "Walk the plot, scan its ground, then raise a homestead in a claim of yours.",
            "An old garden lay under the grass.",
        ),
    },
    ArcTemplate {
        id: ArcId(1102),
        title: "A Housewarming",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The village wants to welcome whoever builds on the empty plot, so the giver is organising a housewarming in advance: a lamp post, a guest list, a cake.",
        twist: "The cake has been ready for three weeks. There is still nobody living on the plot.",
        consequence: "The village throws the housewarming anyway, on an empty plot, for nobody in particular, and it's the best party in years.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::BakesForEveryone),
            Trigger::Value(Value::Community),
            Trigger::Role(Role::Baker),
        ],
        roles: &[Role::Baker, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Homestead), place(StoryPlace::Village)],
        stages: &[
            &[Objective::Speak { target: 1 }],
            &[Objective::Reach { target: 0 }],
            &[Objective::Build {
                target: 0,
                kind: InfrastructureKind::SignalRelay,
            }],
        ],
        beats: &[
            "Invite the neighbours",
            "Visit the empty plot",
            "Put up a beacon for the party",
        ],
        steps: &[
            meet(
                "{Giver} wants the next village over invited too. Ask {contact} at {dest2}, {where2}.",
                "Everyone's invited. Everyone. Even the people we don't like. Especially them, it annoys them.",
                "A housewarming for a house nobody lives in yet? ...That's the most {village} thing I ever heard. We'll bring a pie.",
            ),
            step(
                "Go out to {dest}, {where}, where the party will be.",
                "Check the ground's flat for the tables. And for dancing. Mostly dancing.",
            ),
            step(
                "Set up a signal relay on the plot so the guests can find it after dark.",
                "Something with a light on top. People find parties by the light.",
            ),
        ],
        climax: "The relay's light blinks on over the empty plot, and from both villages, lanterns start bobbing across the fields toward it: guests, cakes and a fiddle, for a house that isn't there.",
        twists: &[],
        reward: QuestReward::credits(900, 12),
        outcomes: &[
            ending(
                "Dance on the empty plot",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(4),
                    ArcEffect::Furniture(Furniture::Gramophone),
                    ArcEffect::Remember,
                ],
                "Best housewarming I've ever thrown, and there wasn't even a house! Here, take the gramophone home. Somebody's house should have the music.",
                "The empty plot's housewarming is now an annual event in {village}. Whoever finally builds there has very big shoes to fill.",
            ),
            choice(
                "Suggest they wait for a real neighbour",
                &[ArcEffect::Reputation(3), ArcEffect::Mood(-10)],
                "Wait? The cake won't wait. ...Fine. We'll freeze it. Can you freeze a cake? We'll find out.",
                "The housewarming is postponed until someone moves in. The cake, three weeks old, is in a cellar, waiting.",
            ),
            give_up(
                &[],
                "No party. The cake went to the pigs, who were delighted.",
            ),
        ],
        pitch: voices(
            &[
                "Housewarming for the empty plot. Nobody lives there yet. Doesn't matter. Help me set it up.",
            ],
            &[
                "We're throwing a housewarming for whoever builds on the empty plot! Nobody has yet, but the cake's ready! Help me set it up?",
            ],
            &[
                "I keep thinking of whoever will live on the empty plot, arriving to nobody. Let's welcome them early. Would you help?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager is throwing a housewarming for an empty plot.",
            "Invite the next village, visit the plot and put up a signal relay.",
            "The party happened anyway.",
        ),
    },
    ArcTemplate {
        id: ArcId(1103),
        title: "The Chair Goes Home",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "A relative moved away and left their rocking chair behind, and it has had nowhere to go since the old house came down. The giver wants it carried out to an empty plot and a bed made beside it, so the chair has a home again.",
        twist: "Carried out to the plot, the chair rocks on its own in the breeze, facing the exact view the old house had.",
        consequence: "The chair has a home, and the plot has its first piece of furniture.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Value(Value::Family),
            Trigger::Role(Role::Storyteller),
            Trigger::Quirk(Quirk::TallTales),
        ],
        roles: &[Role::Storyteller, Role::Healer],
        slots: &[Slot::Absent(AbsentFate::MovedAway)],
        targets: &[place(StoryPlace::Homestead)],
        stages: &[
            &[Objective::HandDeliver {
                target: 0,
                parcel: "rocking chair",
            }],
            &[Objective::Settle {
                kind: ModuleKind::Bed,
            }],
        ],
        beats: &["Carry the chair out", "Make a bed for its new home"],
        steps: &[
            step(
                "Carry {focus}'s rocking chair by hand out to {dest}, {where}.",
                "Carefully. It creaks on the left. It always creaked on the left. That's how you know it's the right chair.",
            ),
            step(
                "The chair is out on the plot. Set up a bed in a claim of yours, so the chair has a house to belong to.",
                "A chair needs a house. A house needs a bed. That's just how it goes.",
            ),
        ],
        climax: "Set down on the plot, the old chair turns a little in the breeze and settles facing the hills, and rocks, slow and steady, the way it used to when {focus} sat in it.",
        twists: &[],
        reward: QuestReward::credits(800, 12),
        outcomes: &[
            ending(
                "Let the chair stay",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(20),
                    ArcEffect::Furniture(Furniture::RockingChair),
                    ArcEffect::Remember,
                ],
                "It faced the hills? Same as always. Then it's home. Keep it for your own house: it'll know where to look.",
                "{Focus}'s chair rocks on your porch now, facing whatever hills are nearest. {Giver} says it looks content.",
            ),
            choice(
                "Bring the chair back to them",
                &[ArcEffect::Reputation(8)],
                "Back here? ...You're right. I wasn't ready. It can rock by my window a while longer.",
                "The chair rocks by {giver}'s window again. Some evenings it faces the hills all on its own.",
            ),
            give_up(&[], "The chair stays in {giver}'s shed, under a sheet."),
        ],
        pitch: voices(
            &["{Focus}'s rocking chair. No house for it now. Take it somewhere it can be home."],
            &[
                "My {focus_relation} {focus} rocked in this chair every evening! It deserves a home with a view. Would you carry it out to an empty plot?",
            ],
            &[
                "Since the old house came down, {focus}'s chair has had nowhere to face. Would you find it a view, and a home to go with it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A relative's old rocking chair needs a home.",
            "Carry the chair by hand to an empty plot, then make a bed in a claim of yours.",
            "The chair faced the hills on its own.",
        ),
    },
    // --- The Moon's colony ------------------------------------------------------------------
    ArcTemplate {
        id: ArcId(1201),
        title: "Letters to the Colony",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "The colony's mail comes up twice a year. The giver's cousin works up there and has a birthday next week. Carry the letters by hand.",
        twist: "The whole colony has been writing to the village too, and never found anyone mad enough to carry the letters down.",
        consequence: "Mail runs both ways between the village and the Moon, by whoever's flying.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 12)],
        triggers: &[
            Trigger::Quirk(Quirk::WritesLetters),
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Role(Role::Courier),
        ],
        roles: &[Role::Courier, Role::Innkeeper],
        slots: &[],
        targets: &[place(StoryPlace::Moon)],
        stages: &[
            &[Objective::HandDeliver {
                target: 0,
                parcel: "bundle of letters",
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &["Carry the letters up", "Find the birthday colonist"],
        steps: &[
            step(
                "Fly {giver}'s letters up to {dest}, {where}, and carry them in by hand.",
                "By hand, at the door. Letters from home should arrive with a knock.",
            ),
            meet(
                "The letters are in. Find {contact} at the colony and hand over the birthday card.",
                "Ask for {contact}. Everyone up there knows everyone. There's not much else to do.",
                "Letters? From the ground? ...Oh, look at this, a card. Happy birthday to me. Wait right there, we've got a whole sack going the other way.",
            ),
        ],
        climax: "The colonist reads the card, laughs, and drags out a dusty sack from behind the airlock: two years of letters to the village below, written and never sent.",
        twists: &[],
        reward: QuestReward::credits(2_600, 16).materials(&[(StrategicResourceKind::MoonIce, 2)]),
        outcomes: &[
            ending(
                "Carry the colony's sack home",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Settlement(4),
                    ArcEffect::Furniture(Furniture::MoonGlobe),
                    ArcEffect::Remember,
                ],
                "Two years of letters from the Moon! Half the village has family up there and didn't know if they were even reading ours. The colony sent this globe with them, for you.",
                "The Moon's mail runs both ways now, carried by whoever's flying. A globe of the Moon sits in your house, the colony's thank-you.",
            ),
            choice(
                "Charge postage both ways",
                &[ArcEffect::Credits(500), ArcEffect::Reputation(4)],
                "Postage? From the MOON? ...It is a long way. Fine. Fine.",
                "You run the only mail service to the Moon, and you charge for it. The letters are shorter now. More to the point.",
            ),
            give_up(
                &[],
                "The letters wait for the colony's twice-yearly mail run.",
            ),
        ],
        pitch: voices(
            &[
                "Cousin works at the Moon colony. Birthday next week. Mail's slow. Carry the letters up.",
            ],
            &[
                "My cousin's birthday is next week and they work at the Moon colony! The mail takes months. Would you carry the letters up by hand?",
            ],
            &[
                "Someone I love lives up on the Moon, and letters take half a year to climb that far. Would you carry mine, so they arrive in time?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A cousin at the Moon colony has a birthday coming.",
            "Carry the letters by hand to the colony, then find the birthday colonist.",
            "The colony had two years of letters going the other way.",
        ),
    },
    ArcTemplate {
        id: ArcId(1202),
        title: "The Lettuce Question",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "The colony's hydroponist has written down asking why the village's lettuce tastes of something and theirs tastes of nothing. The giver wants to answer in person.",
        twist: "The difference is the soil. The colony grows in water and light alone, and nobody up there remembers what dirt smells like.",
        consequence: "The colony gets a sack of good valley soil, and the first lettuce on the Moon that tastes of anything.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 12)],
        triggers: &[
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Role(Role::Farmer),
            Trigger::Quirk(Quirk::AlwaysHungry),
        ],
        roles: &[Role::Farmer, Role::Baker],
        slots: &[],
        targets: &[place(StoryPlace::Moon), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Speak { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Fly up to the colony",
            "Talk lettuce with the hydroponist",
            "Bring the answer home",
        ],
        steps: &[
            step(
                "Fly up to {dest}, {where}, with {giver}'s questions about lettuce.",
                "Write down what they feed it. Exactly. Ounce by ounce. Lettuce is serious.",
            ),
            meet(
                "Find {contact}, who grows the colony's greens, and compare notes.",
                "Ask them to taste something from the valley. Their face will tell you everything.",
                "Water, light and a nutrient mix I've tuned for four years. Tastes of NOTHING. ...What's that smell on your boots? Is that soil? Real soil? Can I keep some?",
            ),
            step(
                "Bring {giver} the answer: the colony has never had dirt. Fly back down.",
                "No dirt? None at all? Oh, the poor things. Come home, I'm packing a sack.",
            ),
        ],
        climax: "{Contact} kneels down and sniffs the dust off your boots like it's perfume. Nobody on the Moon has smelled earth in years.",
        twists: &[],
        reward: QuestReward::credits(2_400, 15).materials(&[(StrategicResourceKind::Helium3, 2)]),
        outcomes: &[
            ending(
                "Send up a sack of valley soil",
                &[
                    ArcEffect::Reputation(15),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Furniture(Furniture::HydroponicRack),
                    ArcEffect::Remember,
                ],
                "A sack of our soil, growing lettuce on the Moon! They sent down one of their grow racks in thanks. Put it by a window. Or don't: it brings its own sun.",
                "The colony's lettuce tastes of the valley now. {Giver} insists it's the best on the Moon, which is true, being the only kind.",
            ),
            choice(
                "Sell them the soil by the ounce",
                &[ArcEffect::Credits(450), ArcEffect::Reputation(2)],
                "By the OUNCE? It's dirt! ...It is very far away, I suppose.",
                "Valley soil is the colony's dearest import, at your prices. The lettuce tastes of money.",
            ),
            give_up(&[], "The colony's lettuce goes on tasting of nothing."),
        ],
        pitch: voices(
            &[
                "Colony's lettuce tastes of nothing. Ours tastes of something. Go up and find out why.",
            ],
            &[
                "The Moon colony wrote asking why our lettuce tastes better than theirs! I want to know too! Fly up and ask their grower?",
            ],
            &[
                "Up on the Moon they grow greens under lamps and wonder why they taste of nothing. I think I know. Would you go up and see?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The Moon colony wants to know why its lettuce tastes of nothing.",
            "Fly to the colony, talk to its grower, then come back down.",
            "The colony has never grown anything in soil.",
        ),
    },
    ArcTemplate {
        id: ArcId(1203),
        title: "Earthrise",
        kind: ArcKind::Night,
        contact_other: false,
        hook: "A stargazer has always wondered what the world looks like from the Moon at night. Keep watch from the colony and describe it to someone who's seen it a thousand times.",
        twist: "The colonist has seen earthrise a thousand times and still gets up for every single one.",
        consequence: "The stargazer gets a description, and a colonist gets a reason to say out loud why they stayed.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 12)],
        triggers: &[
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Value(Value::Freedom),
            Trigger::Fear(Fear::Heights),
        ],
        roles: &[Role::Scholar, Role::Hermit],
        slots: &[],
        targets: &[place(StoryPlace::Moon)],
        stages: &[
            &[Objective::Vigil {
                target: 0,
                seconds: 30,
            }],
            &[Objective::Speak { target: 0 }],
        ],
        beats: &[
            "Watch the night from the Moon",
            "Ask a colonist what they see",
        ],
        steps: &[
            step(
                "Keep watch at {dest}, {where}, through the night, and remember everything you see.",
                "Every light, every cloud. I'll never get up there. You're my eyes.",
            ),
            meet(
                "Ask {contact}, who has watched a thousand earthrises, what they see that you missed.",
                "Ask them which part never gets old. There's always a part that never gets old.",
                "The thin blue edge. Right at the rim of the world, before the rest comes up. A thousand times, and I still get up for that blue edge. Every time.",
            ),
        ],
        climax: "The world below turns slow and enormous over the colony's dome, blue at the rim, the whole of every journey you ever made there at once, small enough to cover with a thumb.",
        twists: &[],
        reward: QuestReward::credits(2_500, 16),
        outcomes: &[
            ending(
                "Describe the blue edge",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Mood(25),
                    ArcEffect::Furniture(Furniture::StarChart),
                    ArcEffect::Remember,
                ],
                "A thin blue edge, before the rest comes up. I can see it now. I'll never see it, and I can see it. Here: my chart of the sky. You've seen more of it than I ever will.",
                "{Giver} has painted a thin blue line along the top of their window frame. Visitors ask about it. {Giver} just smiles.",
            ),
            choice(
                "Say it's indescribable",
                &[ArcEffect::Reputation(6)],
                "Indescribable? ...That's a description too, I suppose. A good one.",
                "{Giver} tells everyone the view from the Moon is indescribable, with great satisfaction, as if it proved something.",
            ),
            give_up(&[], "{Giver} goes on imagining the world from the Moon."),
        ],
        pitch: voices(
            &[
                "Always wondered what the world looks like from the Moon. At night. Go look. Tell me.",
            ],
            &[
                "I have always, ALWAYS wanted to know what our world looks like from the Moon at night! Will you go and watch, and tell me everything?",
            ],
            &[
                "I'll never stand on the Moon. But you could, and you could watch the world rise over it, and tell me. Would you be my eyes?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A stargazer wants to know what the world looks like from the Moon.",
            "Keep watch at the colony through the night, then ask a colonist.",
            "The blue edge never gets old.",
        ),
    },
    ArcTemplate {
        id: ArcId(1204),
        title: "The Selenographer's Survey",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "The colony's selenographer has mapped every crater but one, which won't hold still on the instruments. They need a second pair of eyes and a scanner.",
        twist: "The crater isn't moving. The ground under it hums, and the hum shakes the instruments: the same hum the deepfolk hear under the world.",
        consequence: "The Moon's maps gain one honest blank, marked 'Hum. Return.'",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 12)],
        triggers: &[
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Value(Value::Knowledge),
            Trigger::Role(Role::Cartographer),
        ],
        roles: &[Role::Cartographer, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::Moon)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Scan { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 20,
            }],
        ],
        beats: &[
            "Meet the selenographer",
            "Scan the restless crater",
            "Hold still and listen",
        ],
        steps: &[
            meet(
                "{Giver} has a friend at the colony who maps the Moon. Find {contact} at {dest}, {where}.",
                "They write me every year about the crater that won't sit still. Go and see it for me.",
                "Every crater on the Moon, mapped to the metre. Except that one. It jumps on the instruments. Here: scan it from the colony's rim and tell me what your scanner says.",
            ),
            step(
                "Pulse the scanner toward the restless crater from the colony.",
                "Watch the readout. Not the crater. Craters lie. Readouts only exaggerate.",
            ),
            step(
                "The reading trembles. Stay put and let it settle.",
                "Hold still. If it's shaking, I want to know what's doing the shaking.",
            ),
        ],
        climax: "As you hold still, the readout settles into a slow, steady wave: not the crater moving, but the ground beneath it humming, low and patient, exactly like the hum the deepfolk hear under the world.",
        twists: &[],
        reward: QuestReward::credits(2_800, 17)
            .materials(&[(StrategicResourceKind::AlienAlloy, 1)]),
        outcomes: &[
            ending(
                "Mark the crater honestly",
                &[
                    ArcEffect::Reputation(17),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "A hum under the Moon, the same as the one under us. My friend's marked it on the map: 'Hum. Return.' I think we will. And my best charts are open to you now.",
                "The Moon's map has one honest blank on it, labelled in two hands: 'Hum. Return.' {Giver} keeps a copy over the fireplace.",
            ),
            choice(
                "Call it instrument drift",
                &[ArcEffect::Credits(200), ArcEffect::Reputation(4)],
                "Drift. Yes. That's... a perfectly good answer. Thank you for the scan.",
                "The crater is mapped at last, slightly wrong. The hum goes on underneath it, unbothered.",
            ),
            give_up(&[], "The crater keeps jumping on the colony's instruments."),
        ],
        pitch: voices(
            &["Friend at the Moon colony maps craters. One won't hold still. Go scan it."],
            &[
                "My friend at the Moon colony has mapped every crater except ONE, and it keeps jumping around! Want to help solve it?",
            ],
            &[
                "There's a crater on the Moon that won't hold still for the instruments, and it troubles my friend's sleep. Would you go and listen to it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A crater on the Moon won't hold still on the instruments.",
            "Meet the colony's selenographer, scan the crater and hold out while it settles.",
            "The ground under the crater hums.",
        ),
    },
    // --- Sky towns --------------------------------------------------------------------------
    ArcTemplate {
        id: ArcId(1301),
        title: "A Plot in the Clouds",
        kind: ArcKind::Home,
        contact_other: false,
        hook: "The giver has always wanted to retire to the sky. Visit a sky town, ask its land office about the isles, and prove a person can make a home up there.",
        twist: "The sky town's land officer has a waiting list three pages long, and every name on it is someone from the ground.",
        consequence: "The giver is on the list, and you've proved a home in the clouds is more than a daydream.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 6)],
        triggers: &[
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Fear(Fear::Heights),
            Trigger::Quirk(Quirk::MiningMetaphors),
        ],
        roles: &[Role::RetiredMiner, Role::Farmer],
        slots: &[],
        targets: &[place(StoryPlace::SkyTown)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Speak { target: 0 }],
            &[Objective::Settle {
                kind: ModuleKind::Storage,
            }],
        ],
        beats: &[
            "Fly up to the sky town",
            "Ask about the isles",
            "Put down roots",
        ],
        steps: &[
            step(
                "Fly up to {dest}, {where}. {Giver} wants to know what a sky town is really like.",
                "Is it windy? It looks windy. I'm told it's windy. Go and see.",
            ),
            meet(
                "Ask {contact} about the isle plots the town's land office sells.",
                "Ask what it costs. Ask what the neighbours are like. Ask if it's windy.",
                "Isle plots? We've a list. Three pages. Nearly all ground folk wanting to live where the air is thin and the view goes on forever. I'll add a name, if you like.",
            ),
            step(
                "Prove a home can be made up high: have a storage module standing in a claim of yours.",
                "Somewhere to keep things. That's the start of every home. Somewhere to keep things.",
            ),
        ],
        climax: "{Contact} unrolls the waiting list for the isle plots across the counter: three pages of names, nearly every one from the ground, all wanting the same view.",
        twists: &[],
        reward: QuestReward::credits(1_100, 13),
        outcomes: &[
            ending(
                "Sign them up for the list",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Mood(20),
                    ArcEffect::Furniture(Furniture::CloudHammock),
                    ArcEffect::Remember,
                ],
                "On the list! Third page, near the bottom, but ON it. Here: I bought a hammock for the sky. You'd better use it until my plot comes up.",
                "{Giver}'s name is on a sky town's waiting list, and their bags are half packed. Your cloud hammock is the envy of the valley.",
            ),
            choice(
                "Tell them the wind is terrible",
                &[ArcEffect::Reputation(5), ArcEffect::Mood(-5)],
                "Terrible? Truly? ...Well. My knees thank you, even if my heart doesn't.",
                "{Giver} decided to stay on the ground. They still look up a lot.",
            ),
            give_up(&[], "{Giver} keeps dreaming of the sky from the porch."),
        ],
        pitch: voices(
            &["Want to retire to the sky. Go to a sky town. Ask about the isles. Tell me."],
            &[
                "I'm going to retire to the clouds! Would you visit a sky town and find out about those isle plots for me?",
            ],
            &[
                "All my life I've dug down. When I'm done, I'd like to live up, where the air is thin and clean. Would you ask the sky town for me?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager wants to retire to the sky.",
            "Visit a sky town, ask about isle plots, then have storage standing in your claim.",
            "Half the ground wants to live in the clouds.",
        ),
    },
    ArcTemplate {
        id: ArcId(1302),
        title: "Bells over the Archipelago",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A sky monastery's bell keeps ringing at night with nobody pulling the rope. The giver's friend up there wants someone from the ground to keep watch.",
        twist: "The wind through the archipelago plays the bell, and only on nights when the isles drift into one particular line.",
        consequence: "The monks write a new chant timed to the wind's own bell, and invite the ground to hear it.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 6)],
        triggers: &[
            Trigger::Want(Want::SeeTheWorld),
            Trigger::Role(Role::ShrineKeeper),
            Trigger::Quirk(Quirk::HumsOffKey),
        ],
        roles: &[Role::ShrineKeeper, Role::Brewer],
        slots: &[],
        targets: &[place(StoryPlace::SkyTown), place(StoryPlace::SkyIsland)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::Vigil {
                target: 1,
                seconds: 25,
            }],
        ],
        beats: &[
            "Meet the monks",
            "Fly out to the isles",
            "Keep watch for the bell",
        ],
        steps: &[
            meet(
                "Fly up to {dest}, {where}, and ask {contact} about the bell that rings itself.",
                "My friend up there is a sensible soul. If they say the bell rings itself, it does.",
                "Every few nights, past midnight, the great bell rings once. Nobody at the rope. We've checked. We've checked a great deal. Watch from the isles: the sound comes from there.",
            ),
            step(
                "Fly out to {dest2}, {where2}, where the sound seems to come from.",
                "Follow the sound, they said. Sound is tricky in the sky. Follow it anyway.",
            ),
            step(
                "Keep watch from the isle after dark and listen for the bell.",
                "When it rings, look at the wind. Not the bell. The wind.",
            ),
        ],
        climax: "Past midnight the isles drift into a line, the wind pours through the gap between them with a long low note, and far off the monastery bell answers it once, all by itself.",
        twists: &[],
        reward: QuestReward::credits(1_150, 13),
        outcomes: &[
            ending(
                "Tell the monks about the wind",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::Settlement(3),
                    ArcEffect::Furniture(Furniture::WindChime),
                    ArcEffect::Remember,
                ],
                "The wind rings it! On the nights the isles line up! They're writing a chant for it already. And they sent these chimes for your door, tuned to the bell.",
                "On the nights the isles line up, the monks chant along with the wind's bell. Your wind chimes ring the same note.",
            ),
            choice(
                "Let them keep the mystery",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(10)],
                "A bell that rings itself is a better story than a bell rung by wind. ...You're kinder than you look.",
                "The monks still don't know why the bell rings. They've decided it's a blessing. Perhaps it is.",
            ),
            give_up(&[], "The bell goes on ringing itself, unexplained."),
        ],
        pitch: voices(
            &[
                "Sky monastery's bell rings by itself at night. Friend up there wants a watcher. Go.",
            ],
            &[
                "A bell at a sky monastery rings all by itself at night! My friend up there is baffled. Will you go and keep watch?",
            ],
            &[
                "Far up among the isles a bell rings at midnight with no hand on the rope. My friend there is frightened and fascinated. Would you listen for it?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A sky monastery's bell rings by itself at night.",
            "Meet the monks, fly out to the isles and keep watch after dark.",
            "The wind rings the bell when the isles line up.",
        ),
    },
    ArcTemplate {
        id: ArcId(1303),
        title: "The Smuggler's Parcel",
        kind: ArcKind::Sky,
        contact_other: false,
        hook: "A smuggler in a sky town owes the giver a favour and a parcel. Collect it, no questions, and bring it down.",
        twist: "The parcel is a jar of the giver's grandmother's jam, smuggled because the sky town's customs bans anything that sticky.",
        consequence: "The giver gets their jam and the smuggler's debt is paid, in full and in jam.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 6)],
        triggers: &[
            Trigger::Relationship(RelationshipKind::Owes),
            Trigger::Want(Want::StrikeItRich),
            Trigger::Quirk(Quirk::AlwaysHungry),
        ],
        roles: &[Role::Trader, Role::Courier],
        slots: &[],
        targets: &[place(StoryPlace::SkyTown)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::PickUp {
                target: 0,
                parcel: "sticky parcel",
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Find the smuggler",
            "Collect the parcel",
            "Bring it down without questions",
        ],
        steps: &[
            meet(
                "Fly up to {dest}, {where}, and find {contact}, who owes {giver} a parcel.",
                "Don't mention my name loudly. Don't mention it quietly either. Just nod.",
                "From the ground, are you? Then you'll be wanting this. Don't shake it. Don't open it. And don't, whatever you do, let customs see it. They're very particular about sticky things.",
            ),
            step(
                "Pick up the parcel from the sky town without drawing attention.",
                "Casual. Like you collect mysterious parcels every day. Which, frankly, you do.",
            ),
            step(
                "Bring the parcel down to the surface for {giver}.",
                "Gently. It's... fragile. And delicious. I mean fragile.",
            ),
        ],
        climax: "Halfway down, curiosity wins and you peek: inside the smuggler's parcel, wrapped in four layers of oilcloth, is a jar of jam with a handwritten label.",
        twists: &[],
        reward: QuestReward::credits(1_000, 12),
        outcomes: &[
            ending(
                "Hand over the jam",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(15),
                    ArcEffect::Remember,
                ],
                "Grandmother's damson jam. They don't allow it up there, too sticky for the wind. Debt paid. Here, have a spoonful. Don't tell customs.",
                "{Giver} rations the smuggled jam a spoonful a week. The smuggler sends another jar every spring, by whoever's flying.",
            ),
            choice(
                "Admit you peeked",
                &[ArcEffect::Reputation(6), ArcEffect::Credits(120)],
                "You PEEKED? ...Well, now you know my secret: I'd smuggle anything for that jam. Here, keep quiet, and have this.",
                "You know about the jam. {Giver} knows you know. Every so often a jar arrives at your door, which seems to be a bribe.",
            ),
            give_up(&[], "The parcel waits in the sky town, getting stickier."),
        ],
        pitch: voices(
            &["Smuggler in a sky town owes me a parcel. Collect it. No questions. Bring it down."],
            &[
                "A friend of a friend up in a sky town has a parcel for me! Bit hush-hush. Would you fetch it? No peeking!",
            ],
            &[
                "There is a parcel waiting for me in the clouds, held by someone who owes me. It means more than it should. Would you bring it down?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A sky town smuggler owes a villager a parcel.",
            "Find the smuggler, collect the parcel and bring it down to the surface.",
            "The parcel was a jar of jam.",
        ),
    },
    ArcTemplate {
        id: ArcId(1304),
        title: "Chimes for the Sky",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "A whittler has carved wind chimes for a friend in a sky town who sends a letter every season saying how quiet it is up there. Carry them up by hand.",
        twist: "The friend hasn't been lonely at all; they've been learning to carve, and send back a set of their own.",
        consequence: "Chimes ring in a sky town and a village, each carved by the other.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 6)],
        triggers: &[
            Trigger::Quirk(Quirk::Whittles),
            Trigger::Role(Role::Tinkerer),
            Trigger::Value(Value::Craft),
        ],
        roles: &[Role::Tinkerer, Role::Apprentice],
        slots: &[],
        targets: &[place(StoryPlace::SkyTown), place(StoryPlace::Home)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::HandDeliver {
                target: 0,
                parcel: "carved wind chimes",
            }],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Fly up to the sky town",
            "Hang the chimes at their door",
            "Bring news home",
        ],
        steps: &[
            step(
                "Fly {giver}'s carved chimes up to {dest}, {where}.",
                "Wrap them in something. Not wool, they'll catch. Paper. Lots of paper.",
            ),
            step(
                "Climb out and carry the chimes to a door in the sky town by hand.",
                "Hang them where the wind's steadiest. My friend says it's always steady up there.",
            ),
            meet(
                "The friend sent something back. Take it home and tell {contact} in {village} first, they introduced the two of them.",
                "Tell {contact} first. They'll want to know it worked out.",
                "They sent chimes BACK? Carved them themselves? ...Oh, these are dreadful. Wonderful. Dreadful and wonderful. Hang them up, quick.",
            ),
        ],
        climax: "At the door in the sky town, the friend unwraps the chimes, laughs, and hands you a lumpy parcel of their own: a set of chimes, carved badly and with great love.",
        twists: &[],
        reward: QuestReward::credits(950, 12),
        outcomes: &[
            ending(
                "Hang both sets of chimes",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(3),
                    ArcEffect::Furniture(Furniture::ModelAirship),
                    ArcEffect::Remember,
                ],
                "Chimes up there, chimes down here, each carved by the other. And look, they slipped in a little airship model for the courier. That's you.",
                "Two sets of chimes ring in two places, one carved on the ground and one in the clouds. A model airship hangs from your ceiling.",
            ),
            choice(
                "Keep the friend's chimes for yourself",
                &[ArcEffect::Reputation(-2), ArcEffect::Credits(90)],
                "You KEPT them? ...They were for me, you know. Never mind. Never mind.",
                "{Giver} never got the sky friend's chimes. They hang on your rig instead, rattling.",
            ),
            give_up(&[], "The chimes stay wrapped in paper on {giver}'s shelf."),
        ],
        pitch: voices(
            &[
                "Carved chimes for a friend in a sky town. Says it's too quiet up there. Carry them up.",
            ],
            &[
                "I carved wind chimes for my friend in a sky town! They say it's terribly quiet up there. Will you take them up for me?",
            ],
            &[
                "Every season a letter comes down from the clouds saying how still and quiet it is. I carved something to fill the quiet. Would you carry it up?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A whittler carved wind chimes for a friend in a sky town.",
            "Fly to the sky town, hang the chimes by hand, then bring the news home.",
            "The friend carved a set back.",
        ),
    },
];
