//! Batch: frontier arcs (Act I). The ground and the shallow deep: lost tools, old wagers,
//! derelicts, caverns and the people who dig them. Ids 101-110.

use super::{
    NO_THANKS, choice, ending, feature, give_up, log, meet, place, poi, sealed, step, voices,
};
use crate::{
    people::{
        character::{AbsentFate, Role},
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Slot, Trigger, WorldFactKind},
        traits::{Fear, Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{BlueprintId, Objective, Prerequisite, QuestReward, QuestTier, StoryPlace},
    terrain::StrategicResourceKind,
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(101),
        title: "The Brass Canary",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "An old miner's brass gas canary, the one that saved their life twice, was lost in a gas pocket. Sit in the gas long enough for it to sing, then bring it home.",
        twist: "The canary still works. It works so well it now warns about everything: gas, damp, bad jokes, Tuesdays.",
        consequence: "The miner gets their old friend back, and the canary rides on their porch rail, judging the weather.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Role(Role::RetiredMiner),
            Trigger::Fear(Fear::Gas),
            Trigger::Quirk(Quirk::NamesTheirTools),
        ],
        roles: &[Role::RetiredMiner, Role::Mechanic],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::HazardPocket,
        }],
        targets: &[feature(FeatureKind::HazardCluster, 1, 3)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 18,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Find the gas pocket",
            "Wait for the song",
            "Bring it up singing",
        ],
        steps: &[
            step(
                "{Giver} lost a brass canary in {a place} {where}. It only wakes up in bad air, so that's where you're going.",
                "It's about so big. Brass. Answers to Biscuit. Well, it doesn't answer. It sings.",
            ),
            step(
                "Somewhere in the murk a little brass bird is waiting for enough gas to sing. Stay in it. Mind your hull.",
                "Stay in the gas till it sings. I know. I know. That's the job, I'm afraid.",
            ),
            step(
                "Biscuit is singing like a kettle. Get it up into fresh air before it sings itself hoarse.",
                "You found Biscuit? Bring it up! It hates being down there alone.",
            ),
        ],
        climax: "Somewhere in the gas, a thin brass voice starts up: tweet-tweet-TWEET. Biscuit is alive, outraged, and pointing straight at the worst of the fumes.",
        twists: &[],
        reward: QuestReward::credits(340, 9),
        outcomes: &[
            ending(
                "Hand Biscuit back",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "Biscuit! Oh, you daft brass thing. Look at you, singing at the clouds.",
                "Biscuit sits on {giver}'s porch rail now and sings at anything that smells odd. {Giver} swears it predicted rain twice. It also predicted rain eleven times it didn't rain.",
            ),
            choice(
                "Ask to keep Biscuit on your dashboard",
                &[ArcEffect::Reputation(4), ArcEffect::Credits(60)],
                "Keep it? ...It's taken a shine to you, hasn't it. Fine. Feed it a little gas on Sundays.",
                "Biscuit rides on your dashboard and sings whenever you drill into a pocket. {Giver} visits sometimes, just to hear it.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "Biscuit is still down there somewhere, waiting for bad air. {Giver} leaves the porch light on.",
            ),
        ],
        pitch: voices(
            &[
                "Lost my canary in {a place} {where}. Brass one. Wakes up in gas. Sit in the gas till it sings, bring it home. That's it.",
            ],
            &[
                "My brass canary, Biscuit! Lost in {a place} down near {depth}m. It only sings in bad air, so you'd have to sit in the gas a bit. I'll make it worth your while!",
            ],
            &[
                "There's a little brass bird in {a place}, {depth}m down. It saved my life twice. It's alone in the dark and I can't go back in there. Would you?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old miner's brass gas canary is lost in a hazard pocket. It sings only in gas.",
            "Reach the gas pocket, stay until the canary sings, and bring it up.",
            "The canary is home and singing at everything.",
        ),
    },
    ArcTemplate {
        id: ArcId(102),
        title: "The Wager",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "Two prospectors bet who can find the fatter seam. The giver wants you to dig theirs out first.",
        twist: "The bet was a ruse to keep the rival away from home all day while the village set up a birthday party for them.",
        consequence: "The rival gets the best birthday of their life and the giver's friendship back; the seam is a nice bonus.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::StrikeItRich),
            Trigger::Relationship(RelationshipKind::Rival),
            Trigger::Value(Value::Fortune),
        ],
        roles: &[Role::Prospector, Role::Trader],
        slots: &[
            Slot::Neighbour(RelationshipKind::Rival),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Motherlode,
            },
        ],
        targets: &[feature(FeatureKind::Motherlode, 1, 4)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 12,
            }],
        ],
        beats: &["Sniff out the seam", "Dig it out before sundown"],
        steps: &[
            step(
                "{Giver} bet {other} they could find the fatter seam. Rumour puts {a place} {where}. Pulse the scanner there.",
                "Scan it first. If {other} sees you digging blind they'll laugh for a year.",
            ),
            step(
                "The seam's there and it's a beauty. Dig out a good chunk of it, quick, while {other} is still out hunting.",
                "Dig! Keep {other} busy... I mean, beat {other}. Obviously. That's the bet.",
            ),
        ],
        climax: "As the last chunk comes loose, a flare goes up over {village}: red, gold, red. A party signal. Your radio crackles: 'Keep {other} out ten more minutes!'",
        twists: &[],
        reward: QuestReward::credits(380, 10),
        outcomes: &[
            ending(
                "Tell them the seam is theirs",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Relationship(Some(RelationshipKind::Friend)),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Ha! I won the bet AND {other} cried at their party. Best day this village has had in years.",
                "{Other} walked into a surprise party, a cake shaped like a pick, and a seam with {giver}'s name scratched out and theirs written in. They dig it together now.",
            ),
            choice(
                "Point out you did the actual digging",
                &[ArcEffect::Reputation(2), ArcEffect::Credits(90)],
                "Fair. Fair! Here, take a cut. Don't tell {other}, they think I'm generous.",
                "You took a cut of the seam. {Giver} grumbles about it at every party, which is how you know you're invited to every party.",
            ),
            give_up(
                &[ArcEffect::Mood(-6)],
                "{Other} came home early and found half a party. The cake was lovely anyway.",
            ),
        ],
        pitch: voices(
            &[
                "Bet {other} I'd find the fatter seam. There's {a place} {where}. Scan it, dig it, today. Don't ask why today.",
            ],
            &[
                "I've got a bet on with {other}! Whoever finds the fattest seam wins, and I know there's {a place} near {depth}m. Help me dig it out today? It has to be today!",
            ],
            &[
                "I made a silly bet with {other}. There's {a place} {where}. Could you dig it out for me, today? Only today. It matters. I'll explain after.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Two prospectors bet who finds the fatter seam. Dig out the giver's seam today.",
            "Scan the motherlode, then dig into it.",
            "The bet is won, and a surprise party went off without a hitch.",
        ),
    },
    ArcTemplate {
        id: ArcId(103),
        title: "The Echo Map",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A cartographer wants a cavern mapped by its echoes, then a cave beyond it charted by eye.",
        twist: "Laid out on paper, the echoes draw a face: a smiling face, as wide as the cavern, carved long before anyone came here.",
        consequence: "The giver's map hangs in the village hall, and people argue about who carved the smile.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(40)],
        triggers: &[
            Trigger::Role(Role::Cartographer),
            Trigger::Value(Value::Knowledge),
            Trigger::Want(Want::SeeTheWorld),
        ],
        roles: &[Role::Cartographer, Role::Scholar],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Cavern,
        }],
        targets: &[
            feature(FeatureKind::LargeCavern, 2, 4),
            feature(FeatureKind::Cave, 2, 5),
        ],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Ping the cavern",
            "Chart the cave beyond",
            "Bring the map home",
        ],
        steps: &[
            step(
                "{Giver} needs echoes from {a place} {where}. One good scanner pulse from inside should do it.",
                "One pulse. Hold still while it rings. The rock does the drawing.",
            ),
            step(
                "The echoes run on past the cavern into {dest2}, {where2}. Go and look at it with your own lamps.",
                "The echoes say there's a cave beyond. Echoes lie less than people, but they do lie.",
            ),
            step(
                "Map's done. Get it up before the paper gets damp and the face smudges.",
                "Bring it up flat! Don't fold it. If you fold it I'll know.",
            ),
        ],
        climax: "The scanner trace prints out on the way up, line by line: two eyes, a long nose of rock, a curve of tunnel. The cavern is smiling at you.",
        twists: &[],
        reward: QuestReward::credits(760, 12).materials(&[(StrategicResourceKind::CrystalLens, 1)]),
        outcomes: &[
            ending(
                "Unroll the map",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "It's a face. It's a FACE. I've mapped forty caverns and not one of them smiled at me.",
                "The Smiling Cavern map hangs in {village}'s hall. Children draw moustaches on the copies. {Giver} sells the originals, and finally lets you buy the good ones.",
            ),
            choice(
                "Keep the trace; give them a copy",
                &[ArcEffect::Reputation(3), ArcEffect::Credits(120)],
                "A copy? Hm. Well, the original is yours, I suppose. You did ring the bell.",
                "The original trace is pinned up in your rig. {Giver}'s copy is slightly crooked, and they mention it every time you visit.",
            ),
            give_up(
                &[ArcEffect::Reputation(-3)],
                "The cavern keeps its smile to itself. {Giver} maps somewhere else, a bit put out.",
            ),
        ],
        pitch: voices(
            &[
                "Need a cavern mapped by echo: {a place} {where}. Then the cave past it, by eye. Paying in coin and a lens.",
            ],
            &[
                "I'm making the best map this side of the slate and I need echoes from {a place} near {depth}m! One pulse, then a peek at the cave beyond. Easy!",
            ],
            &[
                "The rock in {a place} rings when you pulse it. {Depth}m down. If you bring me its echo, I can draw its shape. Things have shapes for a reason.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A cartographer wants a cavern mapped by its echoes and the cave beyond charted.",
            "Pulse the scanner in the cavern, reach the cave, and bring the map up.",
            "The cavern map is home. The echoes drew a smiling face.",
        ),
    },
    ArcTemplate {
        id: ArcId(104),
        title: "Cold Coffee",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "Someone's partner went deep years ago and always stashed a flask in their supply cache. Open it; the giver just wants to know if it's still there.",
        twist: "Taped to the flask: 'For whoever finds this. The coffee is terrible. Drink it anyway, and come home safe.'",
        consequence: "The giver laughs for the first time in a long while, and keeps the note in their hat.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Absent(AbsentFate::WentDeep),
            Trigger::Quirk(Quirk::AlwaysHungry),
            Trigger::Value(Value::Tradition),
        ],
        roles: &[Role::Innkeeper, Role::Baker, Role::Brewer],
        slots: &[
            Slot::Absent(AbsentFate::WentDeep),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::SupplyCache,
            },
        ],
        targets: &[sealed(PoiKind::SupplyCache, 1, 3)],
        stages: &[
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Open the cache", "Bring the flask home"],
        steps: &[
            step(
                "{Focus}, {giver}'s {focus_relation}, always stashed a flask in {a place} {where}. Open it and see if it's still there.",
                "Green flask, dented on one side. {Focus} dropped it off a ladder once and never let me forget it.",
            ),
            step(
                "There's a flask with a note taped to it. Take it up to {giver}, note and all.",
                "Is it there? Don't read me the note on the radio. Bring it. I want to read it myself.",
            ),
        ],
        climax: "The note is in {focus}'s handwriting: 'For whoever finds this. The coffee is terrible. Drink it anyway, and come home safe.'",
        twists: &[],
        reward: QuestReward::credits(300, 10),
        outcomes: &[
            ending(
                "Share the terrible coffee",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(30),
                    ArcEffect::Remember,
                ],
                "Oh, that's awful. That's truly awful. That's exactly how {focus} made it.",
                "You and {giver} drank the coffee on the step. It was dreadful. {Giver} keeps the note in their hat band now, and grins whenever someone complains about the coffee at the inn.",
            ),
            choice(
                "Leave the flask for the next one down",
                &[ArcEffect::Reputation(8), ArcEffect::Settlement(2)],
                "Put it back? ...Yes. {Focus} would like that. Someone else might need it more.",
                "The flask is back in the cache with a new note under the old one, in {giver}'s hand: 'Still terrible. Still come home.'",
            ),
            give_up(
                &[],
                "The flask stays sealed in the dark. {Giver} says they'd rather not know, and means it most days.",
            ),
        ],
        pitch: voices(
            &[
                "My {focus_relation} {focus} kept a flask in {a place} {where}. Probably nothing left. Go see. I'd like to know.",
            ],
            &[
                "This'll sound silly! My {focus_relation} {focus} always hid a flask of coffee in {a place}, about {depth}m down. Could you check if it's still there?",
            ],
            &[
                "{Focus} hid things in caches. A flask, always a flask. There's {a place} at {depth}m that might be theirs. I'd like to hold something of theirs again.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager's relative always kept a flask in a supply cache. See if it's still there.",
            "Open the cache and bring the flask up.",
            "The flask came home with a note. The coffee was terrible.",
        ),
    },
    ArcTemplate {
        id: ArcId(105),
        title: "The Restless Seam",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A seam keeps shuddering and dropping dust on the lower diggings. Set charges at the marked spots, in order, to settle it.",
        twist: "The last charge cracks open a geode the size of a wagon, which was what was creaking all along as it grew.",
        consequence: "The diggings go quiet, the village gets a crystal to show off, and the warden sleeps through the night.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(30)],
        triggers: &[
            Trigger::Fear(Fear::CaveIns),
            Trigger::Role(Role::Warden),
            Trigger::Want(Want::ProtectVillage),
        ],
        roles: &[Role::Warden, Role::RetiredMiner],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::HazardPocket,
        }],
        targets: &[feature(FeatureKind::HazardCluster, 2, 4)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Detonate {
                target: 0,
                spots: 3,
            }],
        ],
        beats: &["Find the shuddering seam", "Settle it, in order"],
        steps: &[
            step(
                "Something in {a place} {where} keeps shaking the lower diggings. {Giver} wants it settled before somebody gets buried.",
                "You'll feel it before you see it. The whole seam hums like a cart on cobbles.",
            ),
            step(
                "{Giver} marked three weak points. Bomb them in order: wrong order and the strain just shifts.",
                "In order! First, second, third. Rock remembers the order. Like a grudge.",
            ),
        ],
        climax: "The third charge goes off and the rumbling stops dead. In the settling dust, something glitters: a split geode the size of a wagon, purple inside, still creaking as it cools.",
        twists: &[],
        reward: QuestReward::credits(900, 12).bombs(4),
        outcomes: &[
            ending(
                "Report the seam settled",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
                "A geode? THAT was the noise? It was growing? I've been blaming the poor seam for a year.",
                "The lower diggings are quiet. A slab of purple geode sits outside the warden's door in {village}, and {giver} sleeps soundly for the first time in a year.",
            ),
            choice(
                "Sell the geode, split the take",
                &[ArcEffect::Reputation(4), ArcEffect::Credits(260)],
                "Sell it? ...Well, it was making an awful racket. Half each, then.",
                "The geode went to a collector in another town. {Village} spent its half on a new bell. You can hear it from the diggings.",
            ),
            give_up(
                &[ArcEffect::Prosperity(-1)],
                "The seam still shudders. Nobody digs the lower galleries anymore.",
            ),
        ],
        pitch: voices(
            &[
                "Seam in {a place} {where} won't stop shaking. Three charges, in order, should settle it. I marked the spots.",
            ],
            &[
                "The lower diggings keep shaking and it's {a place} at about {depth}m that's doing it! Three charges in the right order and it'll settle. Fancy some loud work?",
            ],
            &[
                "Can you hear it at night? The rock in {a place}, {depth}m down, shivering. Three charges, in order. Then maybe it can rest. Then maybe I can.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A shuddering seam keeps dropping dust on the lower diggings. Settle it with charges.",
            "Reach the seam, then bomb the three marks in order.",
            "The seam is quiet. It was a geode, growing.",
        ),
    },
    ArcTemplate {
        id: ArcId(106),
        title: "Three Pebbles",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A young apprentice collects pebbles: one from each layer of the earth. They're missing the deep ones.",
        twist: "The last pebble is warm, and pulses faintly. The apprentice names it Toast.",
        consequence: "The apprentice's pebble shelf is complete; Toast sits in the middle on a little cushion.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Quirk(Quirk::CollectsPebbles),
            Trigger::Role(Role::Apprentice),
            Trigger::Value(Value::Wonder),
        ],
        roles: &[Role::Apprentice],
        slots: &[],
        targets: &[
            feature(FeatureKind::ArtifactBed, 1, 2),
            poi(PoiKind::Formation, 3, 4),
        ],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 4,
            }],
            &[Objective::Dig {
                target: 1,
                count: 4,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "A pebble from the clay",
            "A pebble from the deep",
            "Home to the shelf",
        ],
        steps: &[
            step(
                "{Giver} collects pebbles. One from every layer. First stop, {dest}, {where}: pick a good one.",
                "A good one. Round, but not too round. You'll know. Pebbles know you know.",
            ),
            step(
                "Now something deeper: {dest2}, {where2}. That's the gap on {giver}'s shelf.",
                "The deep shelf's empty. It's been empty for AGES. It looks so sad.",
            ),
            step(
                "Two fine pebbles aboard, and the deep one is warm to the touch. Take them home.",
                "Is it a good one? It's a good one, isn't it. I can tell from your voice.",
            ),
        ],
        climax: "The pebble from the deep is warm in your hand, and every few seconds it pulses, like something sleeping.",
        twists: &[],
        reward: QuestReward::credits(260, 8),
        outcomes: &[
            ending(
                "Hand over the pebbles",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "It's WARM. I'm calling it Toast. Toast goes in the middle. Everyone else shuffle up.",
                "{Giver}'s shelf is full. Toast sits in the middle on a sock folded into a cushion, and the whole shelf is very slightly warmer than the rest of the room.",
            ),
            choice(
                "Keep the warm one, give the rest",
                &[ArcEffect::Reputation(-2), ArcEffect::Credits(40)],
                "You're keeping the warm one? ...That's okay. I suppose it picked you. Look after it.",
                "Toast lives in your rig's cup holder. On cold nights it's nice to have. {Giver} asks after it every visit.",
            ),
            give_up(
                &[ArcEffect::Mood(-8)],
                "The deep shelf is still empty. {Giver} dusts it every day anyway.",
            ),
        ],
        pitch: voices(
            &[
                "I collect pebbles. One per layer. Missing the deep ones. You go deep. You see where this is going.",
            ],
            &[
                "I've got pebbles from every layer except the deep ones! Could you bring me one from {dest}, and one from even further down? Please please please?",
            ],
            &[
                "Every layer has its own stone, and its own story. I'm missing the deep ones. Could you bring me two? I'll keep them safe. I keep everything safe.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An apprentice collects a pebble from every layer and is missing the deep ones.",
            "Dig a pebble from each place and bring them up.",
            "The pebble shelf is complete. The deep one is warm.",
        ),
    },
    ArcTemplate {
        id: ArcId(107),
        title: "Overdue",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A scholar lent a book to a miner forty-one years ago. The miner's rig is down there. So is the book.",
        twist: "The book is in perfect condition, with every page annotated in the margins: the miner read it a hundred times.",
        consequence: "The giver waives the late fee with great ceremony, and shelves the book under 'Well Loved'.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(50)],
        triggers: &[
            Trigger::Role(Role::Scholar),
            Trigger::Value(Value::Knowledge),
            Trigger::Quirk(Quirk::OverlyFormal),
        ],
        roles: &[Role::Scholar, Role::Storyteller],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Derelict,
        }],
        targets: &[sealed(PoiKind::DerelictRig, 2, 4)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::RecoverLog { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Find the borrower's rig",
            "Recover the book",
            "Return it, at last",
        ],
        steps: &[
            step(
                "Forty-one years ago, {giver} lent a book to a miner who drove {a place} {where}. Find the rig with the scanner.",
                "The borrower's name was on the slip. I keep slips. I keep every slip.",
            ),
            step(
                "The book should be in the salvage core with the log. Drill it out carefully: it's a first edition.",
                "Gently. The spine was cracked when I lent it. Any further cracking will be noted.",
            ),
            step(
                "You have the book. Its margins are full of tiny pencil notes. Bring it home.",
                "Do NOT read it on the way. It has a twist. Well, it had a twist forty-one years ago.",
            ),
        ],
        climax: "You open the cover on the way up. Every margin is crowded with pencil notes: 'Ha!', 'Told you so', 'Read this bit to the crew', 'Must return. Not yet. One more time.'",
        twists: &[],
        reward: QuestReward::credits(820, 12).fragment(BlueprintId::DampenerCoil),
        outcomes: &[
            ending(
                "Return the book",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Forty-one years overdue. The fine, by my reckoning, is roughly one village. I hereby waive it. Formally.",
                "The book has its own little shelf in {giver}'s study, labelled 'Well Loved'. {Giver} lends it out again, with a note asking for it back within a reasonable fifty years.",
            ),
            choice(
                "Ask to borrow it first",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(15)],
                "Borrow it? ...You'll bring it back? Within a reasonable time? Very well. I'll start a new slip.",
                "You have the book on loan. {Giver} has a new slip with your name on it, and checks on it more often than strictly necessary.",
            ),
            give_up(
                &[ArcEffect::Reputation(-4)],
                "The book stays overdue. {Giver} adds another line to the slip, in red.",
            ),
        ],
        pitch: voices(
            &[
                "Lent a book to a miner forty-one years ago. Their rig is {a place} {where}. I want my book back. Principle of the thing.",
            ],
            &[
                "I have a forty-one year overdue book down in {a place} at {depth}m! The borrower's rig, the book, the lot. Bring it back and I'll waive the fee. Probably!",
            ],
            &[
                "A book I lent long ago is sleeping in {a place}, {depth}m down, in the rig of the one who borrowed it. I would like to know if they ever finished it.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A scholar lent a book forty-one years ago. It's in the borrower's derelict rig.",
            "Scan the derelict, drill its salvage core, and bring the book up.",
            "The book is back. It was very well loved.",
        ),
    },
    ArcTemplate {
        id: ArcId(108),
        title: "The Wandering Mole",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A tinkerer's digging machine, Mole, wandered off into a cavern. Walk it back to the surface.",
        twist: "Mole wasn't lost: it was following a vein and has mapped half of it on its own little paper roll.",
        consequence: "Mole comes home a hero, and the tinkerer builds it a friend.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(25)],
        triggers: &[
            Trigger::Role(Role::Tinkerer),
            Trigger::Want(Want::BuildInvention),
            Trigger::Quirk(Quirk::TalksToMachines),
        ],
        roles: &[Role::Tinkerer, Role::Mechanic],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Cavern,
        }],
        targets: &[feature(FeatureKind::LargeCavern, 1, 3)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Escort {
                target: 0,
                step_ms: 700,
                crew_step_ms: 450,
            }],
        ],
        beats: &["Find Mole", "Walk Mole home"],
        steps: &[
            step(
                "{Giver}'s digging machine, Mole, wandered into {a place} {where}. Go and find it. It likes shiny things and loud noises.",
                "Call its name. It won't answer. It's a machine. But I like to think it listens.",
            ),
            step(
                "Mole's found, chuffing happily. It's slow: stay near it and it'll follow you up through the tunnels.",
                "Stay close! Mole gets distracted. It once followed a lantern for two days.",
            ),
        ],
        climax: "Mole trundles into daylight with a long paper roll trailing out behind it: a hand-punched map of a copper vein, perfect to the tile.",
        twists: &[],
        reward: QuestReward::credits(700, 12)
            .kit(crate::infrastructure::InfrastructureKind::SurveyDrone),
        outcomes: &[
            ending(
                "Return Mole",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "Mole! You mapped a VEIN? On your own? I'm building you a friend. A small one. You'll love her.",
                "Mole has a little sister now, Vole, who mostly follows Mole around. {Giver} rents them both out to prospectors and lets you have the first pick of whatever they build next.",
            ),
            choice(
                "Keep Mole's map for yourself",
                &[ArcEffect::Reputation(-2), ArcEffect::Credits(180)],
                "The map? I suppose... Mole would want it used. Just don't tell Mole.",
                "Mole's map is folded in your rig. Mole doesn't know. {Giver} suspects.",
            ),
            give_up(
                &[ArcEffect::Mood(-12)],
                "Mole is still out there, mapping. Every so often a strip of punched paper drifts up a mineshaft.",
            ),
        ],
        pitch: voices(
            &[
                "My digging machine wandered into {a place} {where}. It's slow. It follows rigs. Walk it home.",
            ],
            &[
                "My little digging machine Mole went exploring in {a place} around {depth}m and didn't come back! It follows anything with lights. Could you lead it home?",
            ],
            &[
                "Mole went into {a place}, {depth}m down, and hasn't come back. It's only a machine. I know. But it's my machine, and the dark is big.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A tinkerer's digging machine wandered into a cavern. Lead it home.",
            "Find the machine in the cavern and escort it up through open tunnel.",
            "Mole is home, with a map it made itself.",
        ),
    },
    ArcTemplate {
        id: ArcId(109),
        title: "The Fossil Choir",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "Deep down, the old fossil beds hum when the rock cools at night. A storyteller wants a recording.",
        twist: "Played back, the hum is a tune: the lullaby the storyteller's grandmother sang, note for note.",
        consequence: "The storyteller tells a new story at the inn, and nobody laughs at the ending.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Depth(120)],
        triggers: &[
            Trigger::Role(Role::Storyteller),
            Trigger::Value(Value::Wonder),
            Trigger::Quirk(Quirk::HumsOffKey),
        ],
        roles: &[Role::Storyteller, Role::ShrineKeeper],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::DeepChamber,
        }],
        targets: &[feature(FeatureKind::Chamber, 5, 7)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 30,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Descend to the choir",
            "Record the hum",
            "Play it at the inn",
        ],
        steps: &[
            step(
                "{Giver} swears the fossil beds in {a place} {where} hum. Go down and listen.",
                "It's very quiet at first. Then it isn't. Turn your engine down, if you can.",
            ),
            step(
                "Hold still in the chamber and let the scanner record. Thirty seconds of the deep, singing.",
                "Don't talk over it! The deep doesn't sing for chatterboxes.",
            ),
            step(
                "The recording is aboard. Take it up to {giver} without playing it. Not yet.",
                "Bring it up. We'll play it at the inn. Everyone should hear it at once.",
            ),
        ],
        climax: "The recorder's little light blinks. Under the drip and tick of the deep, the fossils are singing: slow, low, and oddly familiar, like a song from before you could talk.",
        twists: &[],
        reward: QuestReward::credits(1_900, 16)
            .materials(&[(StrategicResourceKind::RadiantFossil, 2)]),
        outcomes: &[
            ending(
                "Play it at the inn",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "That's my grandmother's lullaby. Note for note. How does the rock know my grandmother's lullaby?",
                "The fossil choir plays at {village}'s inn on quiet evenings. Babies sleep through it. Old miners pretend to have something in their eye. {Giver} tells the story differently every time.",
            ),
            choice(
                "Keep the recording to yourself",
                &[ArcEffect::Reputation(-4), ArcEffect::Credits(300)],
                "You want to keep it? ...Then play it sometimes. Somewhere quiet. It shouldn't just sit.",
                "You play the fossil choir in your rig when the drilling gets lonely. Sometimes, deep down, the rock seems to hum along.",
            ),
            give_up(
                &[ArcEffect::Mood(-8)],
                "The choir sings on with nobody to hear it. {Giver} hums the tune anyway, off key.",
            ),
        ],
        pitch: voices(
            &[
                "Fossil beds in {a place} {where} sing when the rock cools. I want it recorded. Stay down till it's done.",
            ],
            &[
                "Have you heard the fossils sing? Down in {a place} at {depth}m the old beds hum! Record it for me and I'll tell the best story this inn has ever heard!",
            ],
            &[
                "Deep in {band}, {depth}m down, the old bones sing in their sleep. Would you sit with them a while, and bring me their song?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A storyteller wants a recording of the deep fossil beds humming.",
            "Reach the deep chamber, hold still while it records, and bring it up in one piece.",
            "The fossil choir was recorded. It sang a lullaby.",
        ),
    },
    ArcTemplate {
        id: ArcId(110),
        title: "Debts in the Dark",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "The giver owes a neighbour a fortune and thinks an old vault's blueprint could square it.",
        twist: "The creditor never wanted the money: they wanted to be asked over for dinner once in a while.",
        consequence: "The debt is forgiven over a very long dinner, and the vault's prize is the giver's to keep or pass on.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(60)],
        triggers: &[
            Trigger::Want(Want::PayOffDebt),
            Trigger::Relationship(RelationshipKind::Owes),
            Trigger::Fear(Fear::GoingBroke),
        ],
        roles: &[Role::Trader, Role::Farmer],
        slots: &[
            Slot::Neighbour(RelationshipKind::Owes),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::SealedVault,
            },
        ],
        targets: &[sealed(PoiKind::Vault, 3, 5), place(StoryPlace::Home)],
        stages: &[
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::ReturnToSurface],
            &[Objective::Speak { target: 1 }],
        ],
        beats: &[
            "Crack the vault",
            "Bring the prize home",
            "Settle up with the creditor",
        ],
        steps: &[
            step(
                "{Giver} owes {other} more than they can pay. They're betting on {a place} {where}. Open it.",
                "I'm not proud of it. But a vault's a vault. Somebody's grandad was hiding something good.",
            ),
            step(
                "There's a blueprint in the vault, rolled tight. Bring it up.",
                "Is it valuable? Don't tell me. Yes, tell me. No, wait till you're here.",
            ),
            meet(
                "Now the awkward part: go and see {contact}, who knows exactly how {giver} and {other} stand.",
                "Talk to {contact} first. They'll know if {other} is in a forgiving mood.",
                "Money? Oh, {other} doesn't care about the money. They just miss {giver}. They used to have dinner every week. Then the debt, and the dinners stopped.",
            ),
        ],
        climax: "{Contact} hands you a covered dish. 'Take this to {giver}. Tell them it's from {other}. Tell them to set two places.'",
        twists: &[],
        reward: QuestReward::credits(960, 13).fragment(BlueprintId::TowWinch),
        outcomes: &[
            ending(
                "Tell them it's about dinner, not money",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Relationship(Some(RelationshipKind::Friend)),
                    ArcEffect::Mood(20),
                    ArcEffect::Remember,
                ],
                "Dinner. All this time it was dinner. I'll cook. I'll cook something enormous.",
                "{Giver} and {other} have dinner every week again. The debt is written on the back of a menu and crossed out. The blueprint hangs on the wall like a trophy.",
            ),
            choice(
                "Pay the debt with the blueprint anyway",
                &[
                    ArcEffect::Reputation(4),
                    ArcEffect::Credits(200),
                    ArcEffect::Mood(-5),
                ],
                "Squared away, then. Clean. ...Strange, I thought I'd feel lighter.",
                "The debt is paid with the blueprint. {Giver} and {other} nod politely in the street. The dinners never quite came back.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "The vault stays sealed and the debt stays owed. {Giver} crosses the street when {other} comes by.",
            ),
        ],
        pitch: voices(
            &[
                "I owe {other}. A lot. There's {a place} {where}. Open it, and I can pay. Don't make me say it twice.",
            ],
            &[
                "So, I owe {other} rather a lot, and there's {a place} at {depth}m nobody's opened! If there's anything good in it, I'm square. Help?",
            ],
            &[
                "I owe {other} more than I can ever pay, and it's been sitting between us like a stone. There's {a place} {where}. Maybe it holds a way out.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A villager owes a neighbour a fortune and hopes an old vault's prize can pay it.",
            "Open the vault, bring the prize up, and talk to someone who knows the creditor.",
            "The debt was never about money.",
        ),
    },
];
