//! Batch: the deep mystery. Blue lights below the slate, tidy tunnels nobody sweeps, a hum
//! that answers from far down, and the Moon that hums back. A chain: "Blue Lights Below"
//! leads to "The Lamplighters", then "Two Hums", then "The Heart of the Moon". Moon targets
//! fall back to the deepest chamber near the giver (the deep is the other half of the
//! mystery), so the text names them through `{dest}` and `{where}`. Ids 401-408.

use super::{NO_THANKS, choice, ending, feature, give_up, log, meet, place, sealed, step, voices};
use crate::{
    cosmetics::{CosmeticItem, Decal, PaintColor, RoofAttachment},
    people::{
        character::{AbsentFate, Role},
        story::{
            ArcEffect, ArcId, ArcKind, ArcTemplate, HistoryKind, Slot, Trigger, WorldFactKind,
        },
        traits::{Fear, Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{
        BlueprintId, Objective, Prerequisite, QuestReward, QuestTier, StoryPlace, UpgradeStat,
    },
    terrain::StrategicResourceKind,
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(401),
        title: "Blue Lights Below",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "Someone who once saw blue lights deep below the slate wants proof they weren't dreaming. Keep watch down there, through the night.",
        twist: "The lights come, and they blink: long, short, long. It's the town's founding year, in the old miners' tap code.",
        consequence: "The giver is vindicated, and now everyone wants to know who down there knows the founding year.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(70)],
        triggers: &[
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Fear(Fear::DeepThings),
        ],
        roles: &[Role::RetiredMiner, Role::Hermit],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Cavern,
        }],
        targets: &[feature(FeatureKind::LargeCavern, 3, 5)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Vigil {
                target: 0,
                seconds: 40,
            }],
        ],
        beats: &["Go down past the slate", "Watch through the night"],
        steps: &[
            step(
                "{Giver} swears they once saw blue lights in {a place} {where}. Go down there.",
                "They come at night. Don't ask me how a cave knows it's night. It knows.",
            ),
            step(
                "Kill your lamps and wait for night to fall up top. Watch the dark.",
                "Patient. Lamps off. They're shy. They're shy the way deer are shy, not the way people are.",
            ),
        ],
        climax: "Out of the black, the lights come: soft blue, drifting, then blinking in a slow rhythm. Long, short, long, long. Old miners' tap code. It spells the year the town was founded.",
        twists: &[],
        reward: QuestReward::credits(1_050, 14)
            .materials(&[(StrategicResourceKind::CrystalLens, 1)]),
        outcomes: &[
            ending(
                "Tell them what the lights said",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(30),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(402)),
                ],
                "The founding year? In tap code? I KNEW it. Forty years they called me a dreamer. ...So who down there taught them tap code?",
                "{Giver} tells the story of the blue lights at the inn and nobody laughs anymore. Some of the young miners have started tapping messages back into the rock.",
            ),
            choice(
                "Tap a reply into the rock",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Cosmetic(CosmeticItem::Paint(PaintColor::Abyss)),
                    ArcEffect::FollowUp(ArcId(402)),
                ],
                "You answered them? What did you say? ...'Hello'? You said HELLO to the deep? Well. I suppose that's polite.",
                "You tapped 'hello' into the rock and the lights blinked twice before they faded. {Giver} goes very quiet whenever you mention it.",
            ),
            give_up(
                &[ArcEffect::Mood(-10)],
                "{Giver} still says they saw lights. People still smile politely.",
            ),
        ],
        pitch: voices(
            &[
                "Saw blue lights below the slate once. Years back. Nobody believes me. Go down to {a place} at night. Watch.",
            ],
            &[
                "I saw lights down below the slate once, blue ones, moving! Everyone says I dreamed it. Would you keep watch in {a place} at {depth}m one night? Please!",
            ],
            &[
                "Deep in {band}, years ago, I saw lights that weren't lamps. I've never stopped thinking about them. Would you sit in the dark and see if they come back?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone once saw blue lights deep below the slate. Watch for them through the night.",
            "Reach the deep cavern and keep watch while it's night up top.",
            "The lights came, and blinked the town's founding year.",
        ),
    },
    ArcTemplate {
        id: ArcId(402),
        title: "The Lamplighters",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "Someone keeps the old tunnels tidy, lamps trimmed. The deep settlers might know who. Ask them, then follow the lamps.",
        twist: "Nobody has ever seen the lamplighters. But the deep settlers leave out oil every night, and every morning the cans are empty and polished.",
        consequence: "The giver starts leaving out oil too, and the lights in the deep get a little brighter.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Depth(90)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Value(Value::Tradition),
            Trigger::History(HistoryKind::ReachedDepth),
        ],
        roles: &[Role::ShrineKeeper, Role::RetiredMiner, Role::Scholar],
        slots: &[],
        targets: &[place(StoryPlace::Hall), feature(FeatureKind::Chamber, 5, 7)],
        stages: &[
            &[Objective::Speak { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::Scan { target: 1 }],
        ],
        beats: &[
            "Ask the deep settlers",
            "Follow the trimmed lamps",
            "Read the chamber",
        ],
        steps: &[
            meet(
                "{Giver} wants to know who keeps the old tunnels tidy. If anyone knows, it's the folk of {dest}, {where}. Ask {contact}.",
                "Ask the deep folk. They live closest. They must have seen something.",
                "The lamplighters? Never seen them. Nobody has. We leave oil out every night, by the old door. Every morning the cans are empty. Polished, too. Follow the lamps down, if you've the nerve.",
            ),
            step(
                "Somebody trims the lamps that lead down to {dest2}, {where2}. Follow them.",
                "Count the lamps as you go. If one's out, come back. I mean it.",
            ),
            step(
                "The chamber is spotless. Swept. A bench, a little shelf. Pulse the scanner and see what's here.",
                "A shelf? A BENCH? Somebody LIVES down there? Scan it. Scan everything.",
            ),
        ],
        climax: "The scanner paints the chamber's walls: hundreds of tiny tap-code carvings, row after row, all saying the same thing. 'Still here. Still keeping the lights.'",
        twists: &[],
        reward: QuestReward::credits(2_300, 17).fragment(BlueprintId::RefineryFrame),
        outcomes: &[
            ending(
                "Tell them what the walls said",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(403)),
                ],
                "'Still keeping the lights.' Someone, down in the dark, all this time, keeping lights for miners who never knew. I'm going to leave out oil. Tonight.",
                "{Giver} leaves a can of oil by the mine head every night. Every morning it's empty and polished. Down in the deep, the lamps burn a little brighter.",
            ),
            choice(
                "Leave a can of oil in the chamber",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Cosmetic(CosmeticItem::Roof(RoofAttachment::Halo)),
                    ArcEffect::FollowUp(ArcId(403)),
                ],
                "You left them oil? Good. Good. You can't keep lights on kindness alone.",
                "You left a can of oil on the little shelf. The next time you passed, the can was polished, and someone had tapped 'thank you' into the bench.",
            ),
            give_up(
                &[],
                "The lamps go on burning with nobody asking who trims them.",
            ),
        ],
        pitch: voices(
            &[
                "Somebody keeps the deep tunnels tidy. Lamps trimmed. Nobody knows who. The deep settlers might. Go ask. Then follow the lamps.",
            ],
            &[
                "Have you noticed the old tunnels are always SWEPT? Someone trims the lamps down there! Ask the deep folk at {dest} who it is, then follow the lamps!",
            ],
            &[
                "Down in the deep, someone sweeps the tunnels and trims the lamps, and has done for longer than anyone remembers. I'd like to thank them. First we have to find them.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Someone keeps the old deep tunnels tidy. Find out who.",
            "Ask at the underground settlement, follow the lamps to the deep chamber, and scan it.",
            "Someone is still down there, keeping the lights.",
        ),
    },
    ArcTemplate {
        id: ArcId(403),
        title: "Two Hums",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "The deep hums. A scholar thinks something far away hums back. Pulse the scanner at the deepest place you can reach, then at the farthest.",
        twist: "The two hums are the same note, a perfect octave apart, as if one is the echo of the other.",
        consequence: "The scholar writes a paper nobody believes, titled 'The World Is Talking To Itself'.",
        tier: QuestTier::Deep,
        prerequisites: &[
            Prerequisite::Depth(110),
            Prerequisite::Upgrade(UpgradeStat::FuelTank, 8),
        ],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Role(Role::Scholar),
            Trigger::Value(Value::Wonder),
        ],
        roles: &[Role::Scholar, Role::ShrineKeeper],
        slots: &[],
        targets: &[feature(FeatureKind::Chamber, 6, 8), place(StoryPlace::Moon)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Scan { target: 1 }],
        ],
        beats: &["Record the deep hum", "Record its answer"],
        steps: &[
            step(
                "{Giver} wants the deep's hum recorded at {dest}, {where}. One clean pulse.",
                "Pulse once, then listen. The note comes after, like thunder after lightning.",
            ),
            step(
                "Now the other half: {dest2}, {where2}. {Giver} thinks it hums back.",
                "Pulse there too. If I'm right it's the same note. If I'm wrong I'll eat my notes.",
            ),
        ],
        climax: "Side by side on the scanner screen, the two traces are the same shape. The same note, an octave apart, as if one is calling and the other is answering.",
        twists: &[],
        reward: QuestReward::credits(2_600, 17).materials(&[(StrategicResourceKind::CoreShard, 2)]),
        outcomes: &[
            ending(
                "Show them the matching traces",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(404)),
                ],
                "An octave apart. Call and answer. The deep and {dest2} are TALKING. I'm writing this up tonight. Nobody will believe it. That's never stopped me before.",
                "{Giver}'s paper, 'The World Is Talking To Itself', was laughed out of three academies. {Giver} framed the rejection letters and hung them beside the traces.",
            ),
            choice(
                "Play the two notes together",
                &[
                    ArcEffect::Reputation(8),
                    ArcEffect::Cosmetic(CosmeticItem::Decal(Decal::Moon)),
                    ArcEffect::FollowUp(ArcId(404)),
                ],
                "Both at once? ...Oh. Oh, that's beautiful. That's a chord. The world is singing a chord.",
                "You played the two hums together in {giver}'s study. It made a chord so low you felt it in your teeth. The lamps flickered. Neither of you said anything for a long time.",
            ),
            give_up(
                &[],
                "The two hums go on calling to each other, with nobody writing it down.",
            ),
        ],
        pitch: voices(
            &[
                "Deep hums. I think something far off hums back. Pulse the deepest chamber, then pulse {dest2}. Bring both traces.",
            ],
            &[
                "The deep has a hum, right? I think something FAR away hums back! Scan the deep, then scan {dest2}, and let's compare! This could be huge!",
            ],
            &[
                "Listen long enough to the deep and you'll hear it hum. Listen longer and you'll swear something answers. Would you bring me both voices?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A scholar thinks the deep's hum is answered from far away.",
            "Pulse the scanner in the deepest chamber, then at the far place.",
            "The two hums are one note, an octave apart.",
        ),
    },
    ArcTemplate {
        id: ArcId(404),
        title: "The Heart of the Moon",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "The scholar's last request: go to the place that answers, climb out, and put your hand on it.",
        twist: "It's warm, and it ticks, slow and steady, like a clock counting down to something, or a heart.",
        consequence: "The giver has the answer they wanted, and a much bigger question.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 13)],
        triggers: &[
            Trigger::Want(Want::UncoverDeepSecret),
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Value(Value::Adventure),
        ],
        roles: &[Role::Scholar, Role::ShrineKeeper, Role::Hermit],
        slots: &[],
        targets: &[place(StoryPlace::Moon)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::OnFoot { target: 0 }],
            &[Objective::Survive {
                target: 0,
                seconds: 20,
            }],
        ],
        beats: &["Go to the heart", "Step out", "Listen"],
        steps: &[
            step(
                "Everything points to {dest}, {where}. {Giver} has one last request: go there.",
                "All the way. Don't stop at the edge. The edge is where everyone stops.",
            ),
            step(
                "You're here. Climb out of the rig. Some things you have to touch with your own hand.",
                "Out of the rig. Glove off, if you can manage it. Well, perhaps not the glove.",
            ),
            step(
                "Stand still. Press your hand to the ground. Listen.",
                "Twenty seconds. Count them. It'll feel longer.",
            ),
        ],
        climax: "Under your glove the ground is warm, and it ticks. Slow. Steady. Once every few seconds, deep inside, like a clock counting down to something. Or a heart.",
        twists: &[],
        reward: QuestReward::credits(3_200, 20).materials(&[
            (StrategicResourceKind::CoreShard, 3),
            (StrategicResourceKind::AncientAlloy, 2),
        ]),
        outcomes: &[
            ending(
                "Tell the village what you felt",
                &[
                    ArcEffect::Reputation(16),
                    ArcEffect::Settlement(5),
                    ArcEffect::Cosmetic(CosmeticItem::Decal(Decal::Moon)),
                    ArcEffect::Remember,
                ],
                "It ticks. It's WARM, and it ticks. Then whatever it is, it's not dead. It's waiting. ...I need to sit down. Will you sit with me?",
                "The whole of {village} knows now: far away, something warm is ticking. Some are scared. Most go outside on clear nights and look up, and down, and wonder what it's counting to.",
            ),
            choice(
                "Keep it between you and the giver",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(20),
                    ArcEffect::Cosmetic(CosmeticItem::Paint(PaintColor::Glowcap)),
                ],
                "Just us? ...Yes. Until we know what it's counting to. Some secrets should be kept until they're ready.",
                "Only you and {giver} know that something warm is ticking. Sometimes you catch each other's eye across the inn, and look away before anyone notices.",
            ),
            give_up(
                &[],
                "Whatever is ticking goes on ticking. {Giver} listens at night for it, and sometimes thinks they hear it.",
            ),
        ],
        pitch: voices(
            &[
                "Last thing I'll ask. Go to {dest}. Get out. Put your hand on it. Tell me what you feel.",
            ],
            &[
                "We're so close! Everything points to {dest}, {where}! Go there, climb out, and touch it. I need to know what it is. I NEED to!",
            ],
            &[
                "We've followed the lights and the hum all this way. The answer is {where}. Go and put your hand on it, and tell me if it's alive.",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "The mystery leads to one last place. Go there, step out and listen.",
            "Reach the place, climb out on foot, and hold still for twenty seconds.",
            "It's warm, and it ticks.",
        ),
    },
    ArcTemplate {
        id: ArcId(405),
        title: "The Battery Shaped Like a Heart",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "A mechanic who distrusts old machines heard a vault humming like an engine. Open it before whatever's inside wakes up on its own.",
        twist: "Inside is a battery shaped like a heart, still charged after centuries, and when you lift it, every lamp in the vault flickers on.",
        consequence: "The mechanic studies the heart-battery for months and learns to trust one old machine, at least.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(80)],
        triggers: &[
            Trigger::Fear(Fear::OldMachines),
            Trigger::Role(Role::Mechanic),
            Trigger::Quirk(Quirk::TalksToMachines),
        ],
        roles: &[Role::Mechanic, Role::Tinkerer, Role::Smith],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::SealedVault,
        }],
        targets: &[sealed(PoiKind::Vault, 4, 6)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Listen to the vault", "Open it", "Bring out what hums"],
        steps: &[
            step(
                "{Giver} heard {a place} {where} humming like an engine. Pulse it and listen.",
                "If it hums in time with your scanner, back off. If it hums in its own time, that's worse.",
            ),
            step(
                "Something inside hums with a steady beat. Open the vault.",
                "Open it before it opens itself. Machines that wait this long always have a reason.",
            ),
            step(
                "There's a battery inside, shaped like a heart, still warm. Carry it up gently.",
                "Gently! Don't drop it. Don't talk to it. Well. Maybe talk to it a little.",
            ),
        ],
        climax: "You lift the heart-shaped battery from its cradle and every lamp in the vault blinks on at once, row after row into the dark, as if the place has been waiting to see you.",
        twists: &[],
        reward: QuestReward::credits(1_200, 14)
            .materials(&[(StrategicResourceKind::MachineRelic, 2)]),
        outcomes: &[
            ending(
                "Hand over the heart-battery",
                &[
                    ArcEffect::Reputation(12),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "It's beating. Not humming. BEATING. ...I've hated old machines all my life. This one I think I'm going to like.",
                "The heart-battery powers {giver}'s workshop now, and {giver} talks to it every morning. The lights there never flicker. {Giver} has started selling things they'd never have dared build before.",
            ),
            choice(
                "Put it back and seal the vault",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(10)],
                "Put it back? ...You're right. It was waiting for something. Probably not us.",
                "The heart-battery sleeps in its cradle again, the vault resealed. Sometimes, near it, your scanner picks up a slow beat.",
            ),
            give_up(
                &[ArcEffect::Mood(-6)],
                "The vault goes on humming. {Giver} won't go anywhere near that part of the mine.",
            ),
        ],
        pitch: voices(
            &[
                "There's a vault in {a place} humming like an engine. I don't like old machines. Open it before it opens itself.",
            ],
            &[
                "I heard {a place} at {depth}m humming like a motor! Old machines give me the shivers, but I HAVE to know. Open it for me?",
            ],
            &[
                "Something in {a place} hums like it's breathing. Old machines frighten me; they remember things we've forgotten. Would you open it, carefully?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A mechanic heard a vault humming like an engine.",
            "Scan the vault, open it, and bring up what hums.",
            "It was a battery shaped like a heart.",
        ),
    },
    ArcTemplate {
        id: ArcId(406),
        title: "What the Deep Remembers",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "A family's ancestor went deep generations ago. Their wreck lies at the very bottom, log intact. The family wants to hear it.",
        twist: "The log ends mid-sentence: 'The lights are guiding us to a door. It's warm. It's ticking. I think it's been waiting for -'",
        consequence: "The family knows their ancestor's last day, and the deep mystery has one more witness.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Depth(130)],
        triggers: &[
            Trigger::Absent(AbsentFate::WentDeep),
            Trigger::Value(Value::Family),
            Trigger::Fear(Fear::BeingForgotten),
        ],
        roles: &[Role::Storyteller, Role::Healer],
        slots: &[
            Slot::Absent(AbsentFate::WentDeep),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Derelict,
            },
        ],
        targets: &[sealed(PoiKind::DerelictRig, 5, 7)],
        stages: &[
            &[Objective::RecoverLog { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Recover the oldest log", "Bring it to the family"],
        steps: &[
            step(
                "{Focus}, {giver}'s {focus_relation}, went deep generations ago. The wreck is {a place} {where}. Drill out its log.",
                "The family's told the story for generations. Every version ends differently. I'd like the real one.",
            ),
            step(
                "The log is ancient, but it plays. Bring it up for {giver} and the whole family to hear.",
                "Bring it up. We'll gather everyone. Nobody should hear it alone.",
            ),
        ],
        climax: "The log crackles: '...the lights are guiding us to a door. It's warm. It's ticking. I think it's been waiting for -' Then nothing.",
        twists: &[],
        reward: QuestReward::credits(2_500, 18)
            .materials(&[(StrategicResourceKind::RadiantFossil, 2)]),
        outcomes: &[
            ending(
                "Play the log for the family",
                &[
                    ArcEffect::Reputation(14),
                    ArcEffect::Mood(25),
                    ArcEffect::Settlement(3),
                    ArcEffect::Remember,
                ],
                "Waiting for... who? For {focus}? For us? ...Generations we've told this story. I think it's only just started.",
                "{Giver}'s whole family gathered to hear {focus}'s last words. They tell the story with a new ending now, and it isn't an ending at all. It's a door, and it's ticking.",
            ),
            choice(
                "Give them the log; keep the coordinates",
                &[ArcEffect::Reputation(8), ArcEffect::Credits(400)],
                "You'll follow it? Where {focus} went? ...Then come back and tell us what was waiting. Promise.",
                "You kept the coordinates of the last transmission. Somewhere below them, something warm is ticking, and you mean to find it.",
            ),
            give_up(
                &[],
                "{Focus}'s log stays in the deep. The family goes on telling the story every way but the true one.",
            ),
        ],
        pitch: voices(
            &[
                "My {focus_relation}, {focus}. Went deep generations ago. Wreck's {a place} at {depth}m. Log's intact, they say. Bring it up.",
            ],
            &[
                "My family's always told stories about {focus}, who went deeper than anyone! The wreck's {a place} at {depth}m. Would you bring us the real story?",
            ],
            &[
                "Generations ago {focus} went down into {band} and never came back. We've only ever had guesses. There's a log down there. Would you bring us the truth?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A family's ancestor went deep generations ago. Their log lies in the wreck.",
            "Drill the derelict's log at the bottom and bring it up in one piece.",
            "The log ends at a warm, ticking door.",
        ),
    },
    ArcTemplate {
        id: ArcId(407),
        title: "The Moon Is Falling",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "A superstitious neighbour is convinced the Moon is falling. Keep watch from somewhere high tonight and prove them wrong.",
        twist: "It isn't falling. But at the top of the night, it seems to dip, ever so slightly, like a nod.",
        consequence: "The neighbour sleeps again, mostly. They wave at the Moon before bed now. Just in case.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::FuelTank, 3)],
        triggers: &[
            Trigger::Quirk(Quirk::Superstitious),
            Trigger::Fear(Fear::TheDark),
            Trigger::Quirk(Quirk::WeatherWatcher),
        ],
        roles: &[Role::ShrineKeeper, Role::Farmer],
        slots: &[],
        targets: &[place(StoryPlace::SkyIsland)],
        stages: &[&[Objective::Vigil {
            target: 0,
            seconds: 45,
        }]],
        beats: &["Watch the Moon all night"],
        steps: &[step(
            "{Giver} is sure the Moon is falling. Keep watch from {dest}, {where}, through the night, and see.",
            "Watch its edge against the stars. If the stars move behind it, it's moving. If they don't, I'll stop worrying. Probably.",
        )],
        climax: "All night the Moon hangs still against the stars. Then, at the very top of the night, it seems to dip, just slightly, the way someone nods when they've heard you.",
        twists: &[],
        reward: QuestReward::credits(700, 11),
        outcomes: &[
            ending(
                "Swear it isn't falling",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
                "Not falling. Not falling! Oh, thank the stones. ...It NODDED? What do you mean it nodded?",
                "{Giver} sleeps through the night again. Before bed they step outside and wave at the Moon, just in case it's watching. Lately you've caught yourself doing it too.",
            ),
            choice(
                "Mention the nod",
                &[
                    ArcEffect::Reputation(6),
                    ArcEffect::Mood(-5),
                    ArcEffect::FollowUp(ArcId(403)),
                ],
                "It NODDED. At you. ...That's not falling, I'll give you that. That's worse. That's listening.",
                "{Giver} no longer thinks the Moon is falling. They think it's listening, which, they say, is a whole other kettle of fish.",
            ),
            give_up(
                &[ArcEffect::Mood(-8)],
                "{Giver} still watches the Moon every night, waiting for it to drop.",
            ),
        ],
        pitch: voices(
            &[
                "The Moon's falling. I'm sure of it. Sit up on {dest} tonight and watch. Prove me wrong. Please.",
            ],
            &[
                "Don't laugh, but I think the Moon is FALLING. Would you watch it all night from {dest}, {where}? If it's fine, I'll bake you something!",
            ],
            &[
                "Every night it looks a little bigger to me. A little closer. Would you sit up high tonight and watch it, so I can sleep?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A superstitious neighbour thinks the Moon is falling.",
            "Keep watch from somewhere high through the night.",
            "The Moon isn't falling. It nodded.",
        ),
    },
    ArcTemplate {
        id: ArcId(408),
        title: "Core Sample",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "A scholar wants a sample of the deepest rock you can reach. Dig a good chunk and bring it up.",
        twist: "The sample is warm, and once a minute, regular as breath, it pulses.",
        consequence: "The scholar keeps the sample under a glass dome, and times their tea by its pulse.",
        tier: QuestTier::Deep,
        prerequisites: &[Prerequisite::Depth(140)],
        triggers: &[
            Trigger::Role(Role::Scholar),
            Trigger::Value(Value::Knowledge),
            Trigger::History(HistoryKind::FoundArtifact),
        ],
        roles: &[Role::Scholar, Role::Tinkerer],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::DeepChamber,
        }],
        targets: &[feature(FeatureKind::Chamber, 6, 9)],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 14,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Dig the deepest rock", "Bring the sample up"],
        steps: &[
            step(
                "{Giver} wants a sample of the deepest rock there is: {a place} {where}. Dig a good chunk.",
                "Deeper is better. Hotter is better. Stranger is best.",
            ),
            step(
                "The sample is aboard, warm as bread. Bring it up.",
                "Keep it away from the cargo heater. Actually, it might BE a heater. Just bring it.",
            ),
        ],
        climax: "On the way up, the sample in your cargo bay pulses once. Then, a minute later, again. Regular as breathing.",
        twists: &[],
        reward: QuestReward::credits(2_400, 17).fragment(BlueprintId::PressureWeave),
        outcomes: &[
            ending(
                "Hand over the sample",
                &[
                    ArcEffect::Reputation(13),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "It pulses. Once a minute. Rock does NOT do that. I've devoted my life to rock and rock does not do that.",
                "The sample sits under a glass dome in {giver}'s study, pulsing once a minute. {Giver} times their tea by it. The tea has never been better.",
            ),
            choice(
                "Put it back where it came from",
                &[ArcEffect::Reputation(6), ArcEffect::Mood(-5)],
                "Back? It's the most important rock in the world and you want to put it BACK? ...Oh, all right. Perhaps it was comfortable.",
                "The warm sample is back in the deep where you found it. Your scanner still picks up its pulse whenever you pass.",
            ),
            give_up(
                &[],
                "The deepest rock stays where it is, pulsing for no one.",
            ),
        ],
        pitch: voices(
            &[
                "Need a sample of the deepest rock. {A place}, {depth}m. Dig a big chunk. Bring it up. That's science.",
            ],
            &[
                "I need the DEEPEST rock you can find! {A place} at {depth}m should do! Dig me a big chunk? I've cleared a whole shelf for it!",
            ],
            &[
                "What is the world made of, at the very bottom? I've wondered all my life. Would you bring me a piece of {band}, from {depth}m down?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A scholar wants a sample of the deepest rock you can reach.",
            "Dig into the deepest chamber and bring the sample up.",
            "The sample is warm, and it pulses.",
        ),
    },
];
