//! Batch: crew arcs. Built around the quest engine's co-op primitives (two switches, a relay,
//! a hold-and-climb), each with a forgiving solo path. Ids 801-803.

use super::{NO_THANKS, choice, ending, feature, give_up, log, place, poi, step, voices};
use crate::{
    people::{
        character::Role,
        population::RelationshipKind,
        story::{ArcEffect, ArcId, ArcKind, ArcTemplate, Trigger, VillageProblem},
        traits::{Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{Objective, Prerequisite, QuestReward, QuestTier, StoryPlace},
    worldgen::FeatureKind,
};

pub const BATCH: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(801),
        title: "The Twin Bells",
        kind: ArcKind::Crew,
        contact_other: false,
        hook: "An old miners' signal: two bells in a cavern rung at the same moment call everyone home for the festival. Ring them together.",
        twist: "When the twin bells ring, every lamp in the village lights up on its own, an old trick of wires nobody remembered.",
        consequence: "The festival is back, and the bells ring every year.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(25)],
        triggers: &[
            Trigger::Value(Value::Tradition),
            Trigger::Value(Value::Community),
            Trigger::Role(Role::ShrineKeeper),
        ],
        roles: &[Role::ShrineKeeper, Role::Innkeeper, Role::RetiredMiner],
        slots: &[],
        targets: &[feature(FeatureKind::LargeCavern, 1, 3)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Simultaneous {
                target: 0,
                spots: 2,
                window_ms: 3_000,
                solo_window_ms: 26_000,
            }],
        ],
        beats: &["Find the bell cavern", "Ring both bells at once"],
        steps: &[
            step(
                "{Giver} remembers the twin bells in {dest}, {where}: ring them together and everyone comes home for the festival.",
                "Two bells, far apart. They have to ring together. That's the whole point. Together.",
            ),
            step(
                "Stand at both bells at the same moment. A crew makes it easy; alone, you'll have to be quick.",
                "Together! If they ring apart it just sounds like someone fell down the stairs.",
            ),
        ],
        climax: "The bells ring as one, a deep double note that rolls up through the rock, and far above every lamp in {village} flickers on at once, wire by wire, like stars waking up.",
        twists: &[],
        reward: QuestReward::credits(880, 13),
        outcomes: &[
            ending(
                "Come up for the festival",
                &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "Every lamp in the village! Lit by the bells! The old miners wired it all up and nobody remembered! The festival's BACK!",
                "The bell festival is back in {village}. Every year someone rings the twin bells, and every year the lamps light up together, and everyone cheers like it's the first time.",
            ),
            choice(
                "Keep ringing them",
                &[
                    ArcEffect::Reputation(6),
                    ArcEffect::Mood(10),
                    ArcEffect::Credits(90),
                ],
                "Again? And again? ...Every time you ring them the lamps blink. The whole village is dancing. Don't stop!",
                "You rang the bells until the lamps blinked in rhythm and {village} danced in the square till dawn. Nobody slept. Nobody minded.",
            ),
            give_up(
                &[],
                "The twin bells stay silent. The festival is a quiet one again.",
            ),
        ],
        pitch: voices(
            &[
                "Two bells in {dest}. Ring them together, the whole village comes home for the festival. Haven't heard them in years.",
            ],
            &[
                "Have you heard of the twin bells? Down in {dest}! Ring them at the same moment and it calls everyone home for the festival! Bring a friend!",
            ],
            &[
                "Deep in {dest} there are two bells that must speak with one voice. When they do, everyone comes home. Would you ring them for us?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "Two bells in a cavern, rung together, call everyone home for the festival.",
            "Reach the cavern and stand at both bells at the same moment.",
            "The bells rang together, and every lamp lit up.",
        ),
    },
    ArcTemplate {
        id: ArcId(802),
        title: "Hot Potato",
        kind: ArcKind::Crew,
        contact_other: false,
        hook: "A tinkerer's experimental heat core has to reach the deep cavern, but it overheats if carried too long by one rig. Pass it along.",
        twist: "Every time it changes hands, the core plays a different note. By the time it arrives, it's played a whole tune.",
        consequence: "The tinkerer's deep heater works, and the tunnel it warms is everybody's favourite place to rest.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(30)],
        triggers: &[
            Trigger::Role(Role::Tinkerer),
            Trigger::Want(Want::BuildInvention),
            Trigger::Quirk(Quirk::TalksToMachines),
        ],
        roles: &[Role::Tinkerer, Role::Mechanic, Role::Smith],
        slots: &[],
        targets: &[
            place(StoryPlace::Home),
            feature(FeatureKind::LargeCavern, 2, 4),
        ],
        stages: &[&[Objective::Relay {
            target: 1,
            leg_rows: 20,
            solo_cooldown_ms: 6_000,
        }]],
        beats: &["Relay the heat core down"],
        steps: &[step(
            "{Giver}'s heat core has to reach {dest2}, {where2}. It overheats if one rig holds it too long: pass it along, or let it cool and pick it up again.",
            "Don't hold it too long! It gets cranky. And then it gets HOT. And then it gets a bit explodey.",
        )],
        climax: "The core settles into the cavern floor, and as it warms up it plays every note it learned on the way down, one after another: a cheerful little tune that echoes through the deep.",
        twists: &[],
        reward: QuestReward::credits(860, 12),
        outcomes: &[
            ending(
                "Switch on the deep heater",
                &[
                    ArcEffect::Reputation(10),
                    ArcEffect::UnlockOffer,
                    ArcEffect::Remember,
                ],
                "It WORKS! A heater in the deep! And it hums a tune? It never did that on the bench. It must like you.",
                "The deep heater keeps the cavern toasty, and miners stop there to warm their hands and listen to it hum. {Giver} sells smaller ones now, for rigs.",
            ),
            choice(
                "Teach the core a song on the way",
                &[ArcEffect::Reputation(6), ArcEffect::Settlement(2)],
                "You taught it a SONG? Which song? ...Oh no. Not that song. It'll never stop now.",
                "The deep heater plays the same silly song, over and over, forever. Miners hum along whether they want to or not.",
            ),
            give_up(
                &[],
                "The heat core sits in {giver}'s workshop, warm and unused.",
            ),
        ],
        pitch: voices(
            &[
                "Heat core needs to get down to {dest2}. Overheats if one rig holds it too long. Pass it along. Or wait for it to cool.",
            ],
            &[
                "My heat core is ready for the deep! It just overheats if one rig carries it too long. Bring a crewmate and pass it like a hot potato!",
            ],
            &[
                "I built a little sun to warm the deep. It's too hot for one pair of hands. Would you carry it down to {dest2}, with friends?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "A tinkerer's heat core must reach the deep cavern, but overheats if held too long.",
            "Relay the core down, handing it over or letting it cool.",
            "The deep heater hums a tune.",
        ),
    },
    ArcTemplate {
        id: ArcId(803),
        title: "The Stuck Lift",
        kind: ArcKind::Crew,
        contact_other: false,
        hook: "The old lift down to a sealed vault only runs while someone holds the switch. Hold it, and climb.",
        twist: "The vault holds the village's old founding charter, and it says the village was founded by two rival families together.",
        consequence: "The village's founding is rewritten, and two families have to admit they're related.",
        tier: QuestTier::Mid,
        prerequisites: &[Prerequisite::Depth(45)],
        triggers: &[
            Trigger::Problem(VillageProblem::Feud),
            Trigger::Relationship(RelationshipKind::Rival),
            Trigger::Value(Value::Honesty),
        ],
        roles: &[Role::Warden, Role::Scholar, Role::Storyteller],
        slots: &[],
        targets: &[
            feature(FeatureKind::LargeCavern, 2, 4),
            poi(PoiKind::Vault, 3, 5),
        ],
        stages: &[
            &[Objective::HoldAndClimb {
                switch: 0,
                goal: 1,
                solo_latch_ms: 40_000,
            }],
            &[Objective::OpenPoi { target: 1 }],
        ],
        beats: &["Hold the switch and climb", "Open the vault"],
        steps: &[
            step(
                "The old lift to {dest2} only runs while someone holds the switch in {dest}. Hold it, and get a rig to the vault.",
                "Someone holds, someone climbs. If you're alone, the latch holds a while on its own. Not long. Be quick.",
            ),
            step(
                "You're at the vault. Open it.",
                "Open it! The old charter's in there, they say. The truth about how the village began.",
            ),
        ],
        climax: "The charter is brittle and yellow. At the bottom, two signatures, side by side: the founders of both feuding families, who started {village} together.",
        twists: &[],
        reward: QuestReward::credits(950, 13),
        outcomes: &[
            ending(
                "Read the charter aloud in the square",
                &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Settlement(4),
                    ArcEffect::Remember,
                ],
                "Both families. TOGETHER. All this time they've been fighting over who founded the place and they BOTH did. Oh, this is going to be a very interesting dinner.",
                "The founding charter hangs in {village}'s hall. Both families gather under it once a year, glaring at each other fondly. They've started calling each other cousin.",
            ),
            choice(
                "Give the charter to the giver privately",
                &[ArcEffect::Reputation(10), ArcEffect::Credits(140)],
                "Privately? So I can choose when to... yes. Yes. Timing is everything with this sort of thing.",
                "{Giver} is waiting for the right moment to reveal the charter. It's been a while. They say the moment is very nearly right.",
            ),
            give_up(
                &[],
                "The old lift stays stuck, and the charter stays sealed.",
            ),
        ],
        pitch: voices(
            &[
                "Old lift down to the vault in {dest2}. Only runs while someone holds the switch. Bring a friend, or be fast.",
            ],
            &[
                "There's an old lift down to a sealed vault, but it only runs while someone holds the switch! Bring a crewmate? The village charter's down there!",
            ],
            &[
                "The truth about how this village began is locked in {dest2}. The way down needs one hand on the switch and another to climb. Would you?",
            ],
        ),
        thanks: NO_THANKS,
        log: log(
            "An old lift to a sealed vault only runs while someone holds the switch.",
            "Hold the switch while a rig reaches the vault, then open it.",
            "Both feuding families founded the village together.",
        ),
    },
];
