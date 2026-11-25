//! The first arc templates: one per classic story shape, to prove the data model. WP-STORY
//! authors the bulk (60-100) in batches.
//!
//! Ids are stable (saved in memories). Bands: 1 clay, 2 grey stone, 3 slate, 4 basalt,
//! 5 Crystal Faults, 6 Fossil Oceans.

use super::{
    ArcEffect, ArcId, ArcKind, ArcOutcome, ArcStep, ArcTemplate, HistoryKind, OutcomeWhen, Slot,
    Trigger, VillageProblem, WorldFactKind,
};
use crate::{
    people::{
        character::{AbsentFate, Role},
        dialogue::lines::ToneLines,
        population::RelationshipKind,
        traits::{Fear, Quirk, Value, Want},
    },
    poi::PoiKind,
    quest::{
        BlueprintId, Objective, QuestItem, QuestReward, QuestText, QuestTier, StoryPlace,
        TargetSpec, Twist,
    },
    terrain::MineralKind,
    worldgen::FeatureKind,
};

const fn poi(kind: PoiKind, first: u8, last: u8) -> TargetSpec {
    TargetSpec::Poi {
        kind,
        bands: (first, last),
        unopened: false,
    }
}

const fn sealed(kind: PoiKind, first: u8, last: u8) -> TargetSpec {
    TargetSpec::Poi {
        kind,
        bands: (first, last),
        unopened: true,
    }
}

const fn feature(kind: FeatureKind, first: u8, last: u8) -> TargetSpec {
    TargetSpec::Feature {
        kind,
        bands: (first, last),
    }
}

const fn log(offer: &'static str, progress: &'static str, complete: &'static str) -> QuestText {
    QuestText {
        offer,
        progress,
        complete,
    }
}

// The authoring kit the batches share.

/// Pitch lines in three voices: `terse` for gruff and dry people, `warm` for cheerful and
/// earnest ones, `soft` for nervous and poetic ones (the dialogue tone filter colours the rest).
const fn voices(
    terse: &'static [&'static str],
    warm: &'static [&'static str],
    soft: &'static [&'static str],
) -> ToneLines {
    ToneLines {
        gruff: terse,
        cheerful: warm,
        nervous: soft,
        poetic: soft,
        dry: terse,
        earnest: warm,
    }
}

/// No arc-specific thanks: the giver's own opener (by tone) leads into the ending's line.
const NO_THANKS: ToneLines = ToneLines {
    gruff: &[],
    cheerful: &[],
    nervous: &[],
    poetic: &[],
    dry: &[],
    earnest: &[],
};

/// A beat: the journal's text and the giver's nudge.
const fn step(journal: &'static str, nudge: &'static str) -> ArcStep {
    ArcStep {
        journal,
        nudge,
        contact: "",
    }
}

/// A beat that sends you to a contact ([`Objective::Speak`]), with what they say.
const fn meet(journal: &'static str, nudge: &'static str, contact: &'static str) -> ArcStep {
    ArcStep {
        journal,
        nudge,
        contact,
    }
}

/// The default ending.
const fn ending(
    label: &'static str,
    effects: &'static [ArcEffect],
    line: &'static str,
    epilogue: &'static str,
) -> ArcOutcome {
    ArcOutcome {
        label,
        when: OutcomeWhen::Completed,
        effects,
        line,
        epilogue,
    }
}

/// An ending the player may choose instead at turn-in.
const fn choice(
    label: &'static str,
    effects: &'static [ArcEffect],
    line: &'static str,
    epilogue: &'static str,
) -> ArcOutcome {
    ArcOutcome {
        label,
        when: OutcomeWhen::Choice,
        effects,
        line,
        epilogue,
    }
}

/// What giving the story up does.
const fn give_up(effects: &'static [ArcEffect], epilogue: &'static str) -> ArcOutcome {
    ArcOutcome {
        label: "Give it up",
        when: OutcomeWhen::Abandoned,
        effects,
        line: "",
        epilogue,
    }
}

/// A story place target.
const fn place(place: StoryPlace) -> TargetSpec {
    TargetSpec::Story(place)
}

/// Batch 0: the first arcs, one per classic story shape.
pub const BATCH_FIRST: &[ArcTemplate] = &[
    ArcTemplate {
        id: ArcId(1),
        title: "The Long Way Down",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "Someone's relative took a rig down and never came back. Find the wreck and bring home its last log.",
        twist: "The log says they went deeper on purpose, following blue lights, and left a message for home.",
        consequence: "The giver finally knows. They remember who found their family, and the lights become a lead deeper down.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Absent(AbsentFate::WentDeep),
            Trigger::Want(Want::FindRelative),
            Trigger::Problem(VillageProblem::MissingPerson),
        ],
        roles: &[],
        slots: &[
            Slot::Absent(AbsentFate::WentDeep),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Derelict,
            },
        ],
        targets: &[sealed(PoiKind::DerelictRig, 2, 5)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::RecoverLog { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &[
            "Ping the wreck",
            "Recover the last log",
            "Bring the story home",
        ],
        steps: &[
            ArcStep {
                journal: "{Giver} thinks {a place} {where} might be the rig {focus} drove down in. Ping it with the scanner and see what answers.",
                nudge: "Did the scanner hear anything? Even a click?",
                contact: "",
            },
            ArcStep {
                journal: "The wreck answered with a weak, old ping. Its salvage core should still hold the last log. Drill it out gently.",
                nudge: "If the core's still there, the log will be too. {Focus} always kept a log.",
                contact: "",
            },
            ArcStep {
                journal: "The log is aboard, crackling. Get it up into daylight before it fades.",
                nudge: "Bring it up. Please don't play it down there.",
                contact: "",
            },
        ],
        climax: "The log plays on the way up: {focus}'s voice, calm and delighted. 'Blue lights, below the slate. They move like they're waiting for someone. Tell {giver} I'm fine. Tell {giver} to look down sometimes.'",
        twists: &[],
        reward: QuestReward::credits(820, 14),
        outcomes: &[
            ArcOutcome {
                label: "Bring the log home",
                line: "",
                epilogue: "{Giver} plays {focus}'s log every evening now, and says goodnight to the floor. Somewhere below, the blue lights keep their own time.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Reputation(20),
                    ArcEffect::Mood(30),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(5)),
                ],
            },
            ArcOutcome {
                label: "Keep the log's coordinates to yourself",
                line: "You're keeping the coordinates? ...Fine. Maybe someone should follow {focus}. Just come back, will you?",
                epilogue: "You kept the wreck's last coordinates to yourself. {Giver} pretends not to mind, and sells you things a little cheaper, as if paying in advance for news.",
                when: OutcomeWhen::Choice,
                effects: &[ArcEffect::Reputation(4), ArcEffect::UnlockOffer],
            },
            ArcOutcome {
                label: "Give up the search",
                line: "",
                epilogue: "You stopped looking. {Giver} still asks every drifter who passes whether they've seen an old rig down deep.",
                when: OutcomeWhen::Abandoned,
                effects: &[ArcEffect::Reputation(-8), ArcEffect::Mood(-15)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "My {focus_relation}, {focus}. Went down past {band}. There's {a place} near {depth}m. Might be theirs. Find out. Please.",
            ],
            cheerful: &[
                "Big favour! My {focus_relation} {focus} went exploring years ago and someone spotted {a place} around {depth}m in {band}! Could you check if it's theirs? I just know it is!",
            ],
            nervous: &[
                "There's {a place} down in {band}, about {depth}m. People say... they say it might be my {focus_relation}'s. {Focus}'s. Could you look? I can't. I've tried.",
            ],
            poetic: &[
                "The deep took my {focus_relation}, {focus}, years ago. Now there's word of {a place} sleeping in {band}, {depth}m down. Bring me back their last words, if the rock will let you.",
            ],
            dry: &[
                "My {focus_relation} {focus} said 'back by supper' and drove a rig into the ground. There's {a place} at {depth}m. If it's {focus}'s, I'd like a word. Or the log, at least.",
            ],
            earnest: &[
                "My {focus_relation}, {focus}, went down and never came back. There's {a place} near {depth}m in {band}. If it's theirs, would you bring back the log? I just need to know.",
            ],
        },
        thanks: ToneLines {
            gruff: &["So that's where {focus} went. Following lights. Of course. ...Thank you."],
            cheerful: &[
                "{Focus} left me a message?! They were chasing lights! That's so {focus}! Thank you thank you!",
            ],
            nervous: &[
                "{Focus} went deeper on purpose. Following something. I... I think I can sleep now. Thank you.",
            ],
            poetic: &[
                "{Focus} followed the lights down into the dark, and wrote my name on the way. You've given me back a story.",
            ],
            dry: &[
                "Typical {focus}. Gone chasing lights, and still finds time to leave a note. Thank you, {player}.",
            ],
            earnest: &[
                "You brought {focus} home to me, in the only way you could. I'll never forget it.",
            ],
        },
        log: log(
            "A villager's relative went down years ago. A derelict might be theirs: scan it and recover its log.",
            "Ping the wreck, drill its salvage core, and bring the log to the surface.",
            "The log is home. The relative went deeper on purpose, following lights.",
        ),
    },
    ArcTemplate {
        id: ArcId(2),
        title: "Claim and Counterclaim",
        kind: ArcKind::Village,
        contact_other: false,
        hook: "Two neighbours both swear an old claim is theirs. Go see who's right.",
        twist: "Both names are scratched into the claim post, side by side: they staked it together, back when they were partners.",
        consequence: "Settle it and the rivals become partners again (or the giver wins and the rival sulks for a season).",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::BeatRival),
            Trigger::Relationship(RelationshipKind::Rival),
            Trigger::Absent(AbsentFate::OldRival),
        ],
        roles: &[Role::Prospector, Role::Trader],
        slots: &[
            Slot::Neighbour(RelationshipKind::Rival),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Cavern,
            },
        ],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 10,
            }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Find the claim", "Dig out the claim post", "Report back"],
        steps: &[
            ArcStep {
                journal: "{Giver} and {other} both swear the old claim in {a place} {where} is theirs. Find it.",
                nudge: "Found it yet? It's the one with the crooked post. Mine's the straight half.",
                contact: "",
            },
            ArcStep {
                journal: "The claim post is buried in loose fill. Dig it out and read what's carved on it.",
                nudge: "Read the post. It'll say {giver}. Probably. Mostly.",
                contact: "",
            },
            ArcStep {
                journal: "Two names on one post, carved side by side years ago. Bring the news home.",
                nudge: "Well? Whose name is it? Don't make that face.",
                contact: "",
            },
        ],
        climax: "Scratched into the post, side by side: '{giver} and {other}, partners, the summer of the big seam.' Somebody carved a little pick between the names.",
        twists: &[Twist::NoBombs],
        reward: QuestReward::credits(380, 10),
        outcomes: &[
            ArcOutcome {
                label: "Show them both the post",
                line: "",
                epilogue: "{Giver} and {other} stood over the post for a long time. They work the claim together now, and argue about everything except whose it is.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Relationship(Some(RelationshipKind::Friend)),
                    ArcEffect::Reputation(10),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Tell the giver it's theirs alone",
                line: "Mine alone? Ha! I knew it. ...Don't tell {other} I said thank you.",
                epilogue: "You told {giver} the claim was theirs alone. {Other} sulks at the inn, and the post sits in {giver}'s shed, face to the wall.",
                when: OutcomeWhen::Choice,
                effects: &[ArcEffect::Reputation(14), ArcEffect::Mood(20)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "{Other} says the claim by {a place} is theirs. It's mine. Go dig out the post and prove it. No bombs; I want it in one piece.",
            ],
            cheerful: &[
                "Okay, so {other} thinks the claim down by {a place} is theirs, which is hilarious, because it's mine! Dig up the post and settle it! Gently!",
            ],
            nervous: &[
                "{Other} keeps saying the claim near {a place} belongs to them. It doesn't. I think. Could you dig out the old post? Carefully? I'd rather know.",
            ],
            poetic: &[
                "There's a claim post buried by {a place}, and two names fighting over it: mine, and {other}'s. Dig gently. Let the post speak.",
            ],
            dry: &[
                "{Other} and I disagree about who owns a hole in the ground by {a place}. Riveting, I know. Dig up the post. No explosions, it's evidence.",
            ],
            earnest: &[
                "{Other} and I have argued about the claim near {a place} for years. Would you dig out the old post, carefully, so we can finally settle it?",
            ],
        },
        thanks: ToneLines {
            gruff: &["Both our names. On the same post. ...Hm. Maybe I'll buy {other} a drink."],
            cheerful: &[
                "We staked it together?! I forgot! Oh, {other}'s going to laugh so hard! Thank you!",
            ],
            nervous: &[
                "Both names? Oh no. Oh, that's... that's actually lovely. I should talk to {other}.",
            ],
            poetic: &[
                "Two names on one post, weathered side by side. We were friends once, {other} and I. Perhaps again.",
            ],
            dry: &[
                "So we're both right. Worst possible outcome. I'll have to be nice to {other} now.",
            ],
            earnest: &[
                "We were partners. I'd forgotten that. Thank you for reminding me, {player}.",
            ],
        },
        log: log(
            "Two neighbours argue over an old claim. Find it, dig out the claim post by hand and report back.",
            "Reach the claim, dig out its post (no bombs), then head back up.",
            "The post bears both names: they staked it together.",
        ),
    },
    ArcTemplate {
        id: ArcId(3),
        title: "The Prototype",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "An inventor needs their machine field-tested in a real hazard pocket, by someone brave or foolish.",
        twist: "It works too well: it starts sniffing out ore on its own and won't switch off.",
        consequence: "The inventor's one-of-a-kind part goes on offer, and the village hears about it for weeks.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::BuildInvention),
            Trigger::Value(Value::Craft),
        ],
        roles: &[Role::Tinkerer, Role::Mechanic, Role::Smith],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::HazardPocket,
        }],
        targets: &[feature(FeatureKind::HazardCluster, 1, 4)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 20,
            }],
            &[Objective::ReturnToSurface],
            &[Objective::Deliver {
                item: QuestItem::Mineral(MineralKind::Copper),
                count: 3,
            }],
        ],
        beats: &[
            "Run the test in the hazard",
            "Get it back in one piece",
            "Bring copper for repairs",
        ],
        steps: &[
            ArcStep {
                journal: "{Giver}'s prototype needs a real test: hold it inside {a place} {where} until the needles stop twitching.",
                nudge: "Keep it in the muck till the dial goes green. Or orange. Green's better.",
                contact: "",
            },
            ArcStep {
                journal: "The test's done and the machine is ticking oddly. Get it to the surface.",
                nudge: "Is it ticking? Ticking's fine. Whirring's fine. Singing would be new.",
                contact: "",
            },
            ArcStep {
                journal: "It's sniffing for ore all by itself now. Bring {giver} what it asked for.",
                nudge: "Bring the samples! It's never done that on the bench.",
                contact: "",
            },
        ],
        climax: "The prototype gives a happy little chirp and points straight at the nearest seam. It will not stop pointing. It has opinions now.",
        twists: &[Twist::Fragile { max_damage: 30 }],
        reward: QuestReward::credits(420, 10),
        outcomes: &[
            ArcOutcome {
                label: "Hand back the prototype",
                line: "",
                epilogue: "{Giver}'s prototype sits in the workshop window, pointing at the ground. Tamer copies sell well, and {giver} tells everyone who tested the first.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::UnlockOffer,
                    ArcEffect::Reputation(10),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Abandon the test",
                line: "",
                epilogue: "The test never finished. {Giver} put the prototype under a sheet and stopped mentioning it.",
                when: OutcomeWhen::Abandoned,
                effects: &[ArcEffect::Mood(-10)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Built a thing. Needs testing in {a place}, {band}. Sit in it twenty seconds. Don't break it. Bring copper after.",
            ],
            cheerful: &[
                "It's finished! Well, nearly! I need someone to take it into {a place} down in {band} for twenty seconds! What could go wrong? Lots! But probably not!",
            ],
            nervous: &[
                "My machine needs a real test. In {a place}. In {band}. I can't watch. Twenty seconds, then bring it back, gently. And some copper, in case.",
            ],
            poetic: &[
                "Six years of evenings live inside this little machine. Carry it into {a place} in {band}, and let it breathe the dark for twenty heartbeats.",
            ],
            dry: &[
                "My prototype needs field testing. 'Field' meaning {a place} near {depth}m. Twenty seconds. If it explodes, that's also data.",
            ],
            earnest: &[
                "I've put everything into this machine. Would you test it in {a place} in {band}? Twenty seconds, then bring it home, and a little copper for repairs.",
            ],
        },
        thanks: ToneLines {
            gruff: &["It's still running. Won't stop. ...I'm keeping it."],
            cheerful: &["It works! It works too much! It's sniffing the floorboards! I love it!"],
            nervous: &[
                "It won't turn off. Is that good? It's pointing at the ground. I think it found ore. Under my bed.",
            ],
            poetic: &[
                "Listen: it hums, searching. My machine has learned to want something. Thank you, {player}.",
            ],
            dry: &["It works perfectly and I can't turn it off. Like most of my relationships."],
            earnest: &[
                "It actually works. You don't know what this means to me, {player}. Thank you.",
            ],
        },
        log: log(
            "An inventor's prototype needs a field test inside a hazard pocket.",
            "Survive inside the hazard without wrecking the rig, bring it up, then deliver copper.",
            "The prototype works, slightly too well.",
        ),
    },
    ArcTemplate {
        id: ArcId(4),
        title: "The Stranger's Map",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A map traded for a hot meal points at a humming formation and a vault beyond it.",
        twist: "The map was drawn decades ago by the giver's own grandparent, who never told anyone.",
        consequence: "A family secret comes to light; the giver shares the rest of the old maps.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Value(Value::Adventure),
            Trigger::Value(Value::Knowledge),
            Trigger::Want(Want::SeeTheWorld),
            Trigger::History(HistoryKind::FoundArtifact),
        ],
        roles: &[Role::Cartographer, Role::Scholar, Role::Innkeeper],
        slots: &[
            Slot::Place {
                target: 1,
                kind: WorldFactKind::SealedVault,
            },
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Formation,
            },
        ],
        targets: &[poi(PoiKind::Formation, 2, 4), sealed(PoiKind::Vault, 4, 6)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::OpenPoi { target: 1 }],
        ],
        beats: &["Read the formation", "Open the vault"],
        steps: &[
            ArcStep {
                journal: "A stranger's map leads to {dest}, {where}. Pulse the scanner near it and see if the marks line up.",
                nudge: "The map says 'listen to the stone'. I assume that means the scanner.",
                contact: "",
            },
            ArcStep {
                journal: "The formation's echo points at a sealed vault {where2}. Open it.",
                nudge: "A vault! The map has a little drawing of a key. It's no help at all.",
                contact: "",
            },
        ],
        climax: "Inside the vault door, scratched in an old hand: the same initials as the corner of the map. {Giver}'s grandparent's initials.",
        twists: &[],
        reward: QuestReward::credits(1_050, 12).fragment(BlueprintId::RefineryFrame),
        outcomes: &[
            ArcOutcome {
                label: "Bring back what the vault held",
                line: "",
                epilogue: "{Giver} found the rest of the old maps in an attic trunk and pins one up every week. The family had a cartographer all along.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::UnlockOffer,
                    ArcEffect::Reputation(12),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Abandon the map",
                line: "",
                epilogue: "The map went back in a drawer. {Giver} still takes it out on long evenings and traces the line with a finger.",
                when: OutcomeWhen::Abandoned,
                effects: &[ArcEffect::Reputation(-4)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Stranger paid for supper with a map. Marks {a place} in {band}, {depth}m. Probably nonsense. Go check. Scan the rock first.",
            ],
            cheerful: &[
                "Look what a stranger gave me for a bowl of stew: a treasure map! It's got {a place} on it, {depth}m down! Adventure! Go! Tell me everything!",
            ],
            nervous: &[
                "A stranger left this map. It shows {a place} at {depth}m. I keep thinking it's a trap. Or it isn't, and it's real. Which is worse?",
            ],
            poetic: &[
                "A traveller left me a map, inked in a careful hand: {a place}, {depth}m beneath us, in {band}. Maps are just stories that point.",
            ],
            dry: &[
                "Someone traded me a 'treasure map' for soup. It points to {a place} in {band}. I've been robbed, or I'm rich. Find out which.",
            ],
            earnest: &[
                "A traveller left me this old map. It marks {a place}, about {depth}m down in {band}. Would you see if it's real? I'd love to know.",
            ],
        },
        thanks: ToneLines {
            gruff: &["That's my gran's handwriting. On the map. ...She never said a word."],
            cheerful: &[
                "Wait, that's Grandma's writing! Grandma drew a treasure map?! Grandma was an adventurer!",
            ],
            nervous: &[
                "That signature... that's my grandparent's. Why didn't they ever tell us? What else didn't they tell us?",
            ],
            poetic: &[
                "The ink was my grandmother's all along. The map came home, the long way round.",
            ],
            dry: &[
                "The map's signed. By my grandfather. So I paid soup for my own inheritance. Marvellous.",
            ],
            earnest: &[
                "My grandparent drew this. They walked those tunnels. Thank you for bringing that back to me.",
            ],
        },
        log: log(
            "A stranger's map marks a formation and a sealed vault beyond it.",
            "Scan the formation, then work the vault's lock and get inside.",
            "The map was drawn by the giver's grandparent.",
        ),
    },
    ArcTemplate {
        id: ArcId(5),
        title: "The Humming Stone",
        kind: ArcKind::Mystery,
        contact_other: false,
        hook: "Something in a relic bed hums at night and keeps half the village awake. Dig it out and bury it deeper.",
        twist: "Sealed deep, the hum doesn't stop. It changes key, as if answering something further down.",
        consequence: "The village sleeps again, but the giver starts listening at night, and the deep has noticed.",
        tier: QuestTier::Mid,
        prerequisites: &[],
        triggers: &[
            Trigger::Fear(Fear::DeepThings),
            Trigger::Quirk(Quirk::SawTheDeepLights),
            Trigger::Quirk(Quirk::Superstitious),
            Trigger::Want(Want::UncoverDeepSecret),
        ],
        roles: &[Role::ShrineKeeper, Role::Hermit, Role::Scholar],
        slots: &[
            Slot::Place {
                target: 0,
                kind: WorldFactKind::ArtifactBed,
            },
            Slot::Place {
                target: 1,
                kind: WorldFactKind::Cavern,
            },
        ],
        targets: &[
            feature(FeatureKind::ArtifactBed, 1, 4),
            feature(FeatureKind::LargeCavern, 2, 4),
        ],
        stages: &[
            &[Objective::Reach { target: 0 }],
            &[Objective::PickUp {
                target: 0,
                parcel: "humming stone",
            }],
            &[Objective::Detonate {
                target: 1,
                spots: 2,
            }],
        ],
        beats: &["Find the hum", "Take the stone", "Bury it with the cave"],
        steps: &[
            ArcStep {
                journal: "Something in {a place} {where} hums all night. Find it.",
                nudge: "Can you hear it from here? I can. I always can now.",
                contact: "",
            },
            ArcStep {
                journal: "It's a stone, warm and buzzing like a hive. Take it.",
                nudge: "Don't hold it too long. It gets into your teeth.",
                contact: "",
            },
            ArcStep {
                journal: "Bury it deeper: bomb the marked rock and bring the cave down over it.",
                nudge: "Bury it. Deep. And please don't hum back at it.",
                contact: "",
            },
        ],
        climax: "The cave folds shut over the stone. For a moment there's silence. Then, far below, the hum starts again in a lower key, as if something down there just answered.",
        twists: &[],
        reward: QuestReward::credits(900, 12).bombs(3),
        outcomes: &[
            ArcOutcome {
                label: "Seal the stone away",
                line: "",
                epilogue: "{Village} sleeps again. Only {giver} sits up some nights, ear to the floorboards, listening to a hum nobody else hears anymore.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Prosperity(1),
                    ArcEffect::Reputation(10),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Keep the stone",
                line: "You're keeping it? Then keep it far from me. And if it starts singing words, tell me which ones.",
                epilogue: "The humming stone rides in your cargo bay and purrs when you drill. {Giver} won't stand within ten paces of your rig.",
                when: OutcomeWhen::Choice,
                effects: &[ArcEffect::Reputation(-6), ArcEffect::FollowUp(ArcId(1))],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Something in {a place} hums all night. {Depth}m down and I still hear it. Dig it out, take it deeper, bring the roof down on it.",
            ],
            cheerful: &[
                "Fun mystery! Something in {a place} hums every night! Nobody can sleep! Could you dig it out and bury it somewhere deeper? With bombs? Bombs are fun!",
            ],
            nervous: &[
                "Can you hear it? That hum? It's coming from {a place}, {depth}m under us. Every night. Please take it away. Far away. Deep.",
            ],
            poetic: &[
                "At night the ground sings, one low note from {a place} in {band}. Lift the singer out, and lay it to rest where the deep can hold it.",
            ],
            dry: &[
                "Something in {a place} hums. All night. In B flat. Take it deeper and drop a cave on it. I'd like to sleep before I'm ninety.",
            ],
            earnest: &[
                "Something down in {a place} has been humming every night, and people are frightened. Would you dig it out and seal it away somewhere deeper?",
            ],
        },
        thanks: ToneLines {
            gruff: &["Quiet now. Mostly. ...If it starts again, you didn't hear it from me."],
            cheerful: &[
                "It's quiet! I slept twelve hours! Although... was that a hum just now? No! Probably not!",
            ],
            nervous: &[
                "It stopped. It stopped! Wait. Listen. It's different now. Like it's... answering something.",
            ],
            poetic: &[
                "The song has changed key. Something deeper heard it, and is singing back. Thank you, I think.",
            ],
            dry: &["The hum's stopped. Well, it's changed. It's in C now. Progress."],
            earnest: &[
                "The village can sleep again. Thank you, {player}. Truly. I'll keep an ear out, just in case.",
            ],
        },
        log: log(
            "Something in a relic bed hums at night. Dig it out and seal it in a deeper cave.",
            "Reach the relic bed, pick up the humming stone, then blow the marks to bury it.",
            "The stone is sealed away. The hum changed key.",
        ),
    },
    ArcTemplate {
        id: ArcId(6),
        title: "Bad Blood",
        kind: ArcKind::Crew,
        contact_other: false,
        hook: "A feud over an old boundary: two markers in a cave have to be struck at the same moment to split it fairly.",
        twist: "Both sides asked you separately, each hoping you'd quietly favour them.",
        consequence: "Strike it fair and the feud cools to grumbling; the whole village breathes out.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Problem(VillageProblem::Feud),
            Trigger::Relationship(RelationshipKind::Rival),
            Trigger::Want(Want::MakeAmends),
        ],
        roles: &[Role::Warden, Role::Innkeeper],
        slots: &[
            Slot::Neighbour(RelationshipKind::Rival),
            Slot::Place {
                target: 0,
                kind: WorldFactKind::Cavern,
            },
        ],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[
            &[Objective::Simultaneous {
                target: 0,
                spots: 2,
                window_ms: 3_000,
                solo_window_ms: 25_000,
            }],
            &[Objective::Dig {
                target: 0,
                count: 8,
            }],
        ],
        beats: &["Strike both markers at once", "Cut the new boundary"],
        steps: &[
            ArcStep {
                journal: "The feud's boundary markers sit in {a place} {where}. Strike both at the same moment, or nobody will believe it was fair.",
                nudge: "Both at once. Not one and then the other. Both.",
                contact: "",
            },
            ArcStep {
                journal: "Fair strike. Now cut the new boundary through the rock.",
                nudge: "Cut it straight. Everyone will be measuring.",
                contact: "",
            },
        ],
        climax: "The boundary runs dead straight through the rock. Both families check it twice, then shake hands, loudly, as if it hurts.",
        twists: &[],
        reward: QuestReward::credits(400, 10),
        outcomes: &[
            ArcOutcome {
                label: "Split it fairly",
                line: "",
                epilogue: "The feud cooled to grumbling. {Giver} and {other} still don't wave, but they've stopped counting each other's buckets.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Relationship(None),
                    ArcEffect::Prosperity(1),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Quietly favour the giver",
                line: "A little to my side, you say? Well. I didn't ask. But I won't complain.",
                epilogue: "The boundary leans a hand's width toward {giver}. Nobody can prove it. Everybody suspects it.",
                when: OutcomeWhen::Choice,
                effects: &[ArcEffect::Reputation(12), ArcEffect::Prosperity(-1)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Me and {other}. Old boundary in {a place}. Two markers, struck at once, or it doesn't count. Bring a mate or be quick.",
            ],
            cheerful: &[
                "Let's end this silly feud with {other}! There are two old markers in {a place}: hit them both at once and the boundary's settled! Teamwork!",
            ],
            nervous: &[
                "The feud with {other} is getting worse. The old way to settle it is two markers in {a place}, struck together. Could you? Please be fair.",
            ],
            poetic: &[
                "{Other} and I have quarrelled long enough. In {a place} stand two old markers. Strike them in one breath, and let the rock divide us fairly.",
            ],
            dry: &[
                "{Other} and I are feuding. Traditional remedy: hit two rocks in {a place} at the same time. Our ancestors were very strange people.",
            ],
            earnest: &[
                "I'm tired of fighting with {other}. Two markers in {a place} settle the old boundary if they're struck together. Would you help us end it?",
            ],
        },
        thanks: ToneLines {
            gruff: &["Fair split. {Other} grumbled. I grumbled. That's peace, round here."],
            cheerful: &["Done! Fair and square! {Other} even smiled! A tiny smile! I saw it!"],
            nervous: &[
                "{Other} asked you too? Of course they did. And you were fair anyway. Thank you.",
            ],
            poetic: &[
                "The line is drawn, and drawn true. Perhaps now {other} and I can stop drawing lines.",
            ],
            dry: &[
                "So {other} asked you as well. And you were fair to both of us. How irritating. Thank you.",
            ],
            earnest: &[
                "You were fair to both of us, even though we both asked you not to be. Thank you, {player}.",
            ],
        },
        log: log(
            "Settle an old boundary feud: two markers in a cavern must be struck together.",
            "Stand on both markers at once (alone, be quick), then cut the new boundary.",
            "The boundary is split fairly. The feud cools.",
        ),
    },
    ArcTemplate {
        id: ArcId(7),
        title: "Heirloom Below",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "A family keepsake was cached below generations ago. Open the cache and bring it home without a scratch.",
        twist: "The heirloom is, objectively, hideous. They love it more than anything.",
        consequence: "It takes pride of place in the giver's home; their whole family warms to you.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Want(Want::RecoverHeirloom),
            Trigger::Value(Value::Tradition),
            Trigger::Value(Value::Family),
        ],
        roles: &[Role::RetiredMiner, Role::Farmer, Role::Baker],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::SupplyCache,
        }],
        targets: &[sealed(PoiKind::SupplyCache, 1, 3)],
        stages: &[
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        beats: &["Open the old cache", "Carry it home gently"],
        steps: &[
            ArcStep {
                journal: "{Giver}'s family cached their keepsake in {a place} {where}, generations back. Open the cache.",
                nudge: "It's wrapped in a blue shawl. Or it was. Shawls don't last.",
                contact: "",
            },
            ArcStep {
                journal: "You have it. It is very ugly. Carry it home without a scratch.",
                nudge: "Gently! It's survived three floods and an uncle.",
                contact: "",
            },
        ],
        climax: "Unwrapped in daylight, the heirloom is a brass frog with three legs and an expression of deep disappointment. It is perfect.",
        twists: &[Twist::Fragile { max_damage: 20 }],
        reward: QuestReward::credits(300, 9),
        outcomes: &[
            ArcOutcome {
                label: "Bring the heirloom home",
                line: "",
                epilogue: "The frog sits in {giver}'s window. Children make faces back at it. {Giver}'s whole family nods to you in the street now.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Reputation(12),
                    ArcEffect::Mood(25),
                    ArcEffect::Remember,
                ],
            },
            ArcOutcome {
                label: "Abandon the search",
                line: "",
                epilogue: "The cache stayed closed. {Giver} says the frog is safer down there anyway. Their voice says otherwise.",
                when: OutcomeWhen::Abandoned,
                effects: &[ArcEffect::Mood(-10)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Family keepsake. Great-grandad cached it in {a place}, {depth}m. Get it out. Don't scratch it.",
            ],
            cheerful: &[
                "My family's greatest treasure is in {a place} down in {band}! Great-grandma stashed it there! Bring it home, and don't bump it!",
            ],
            nervous: &[
                "There's a family heirloom in {a place}, {depth}m down. It's very old and very fragile and please, please don't drop it.",
            ],
            poetic: &[
                "Three generations ago, my family laid a treasure to rest in {a place}, in {band}. It's time it came home. Carry it as you'd carry a sleeping child.",
            ],
            dry: &[
                "My family hid an heirloom in {a place} at {depth}m. Nobody remembers what it is. Bring it up undamaged so we can be disappointed properly.",
            ],
            earnest: &[
                "My family's keepsake has been in {a place} for three generations. Would you bring it home safely? It would mean the world to us.",
            ],
        },
        thanks: ToneLines {
            gruff: &["...It's uglier than I remembered. It's perfect. Don't say a word."],
            cheerful: &[
                "Oh, it's HIDEOUS! I love it! Look at its little face! Pride of place, right on the mantel!",
            ],
            nervous: &[
                "It's... very... It's ours. It's ours and it's home. Thank you. Please don't tell anyone what it looks like.",
            ],
            poetic: &[
                "Beauty is in the keeping, not the thing. And oh, what a gloriously ugly thing to keep.",
            ],
            dry: &[
                "Generations of longing, for this. It looks like a potato in a hat. I'm weeping. Thank you.",
            ],
            earnest: &[
                "It's not much to look at, but it's my family's, and you brought it home. Thank you, {player}.",
            ],
        },
        log: log(
            "A family heirloom lies in an old sealed cache. Open it and bring it home intact.",
            "Open the cache, then fly it home without taking much damage.",
            "The heirloom is home. It is hideous. It is beloved.",
        ),
    },
    ArcTemplate {
        id: ArcId(8),
        title: "The Sealed Mine",
        kind: ArcKind::Frontier,
        contact_other: false,
        hook: "The village's old mine caved in and took its living with it. Blast the rubble clear and open the way.",
        twist: "Behind the rubble, somebody has been keeping the old tunnels tidy, lamps trimmed and all.",
        consequence: "The mine reopens: the village prospers again, and someone in the deep has a new neighbour.",
        tier: QuestTier::Shallow,
        prerequisites: &[],
        triggers: &[
            Trigger::Problem(VillageProblem::SealedMine),
            Trigger::Want(Want::ProtectVillage),
            Trigger::Value(Value::Community),
            Trigger::Fear(Fear::CaveIns),
        ],
        roles: &[Role::Warden, Role::RetiredMiner, Role::Smith],
        slots: &[Slot::Place {
            target: 0,
            kind: WorldFactKind::Cavern,
        }],
        targets: &[feature(FeatureKind::LargeCavern, 1, 3)],
        stages: &[
            &[Objective::Detonate {
                target: 0,
                spots: 3,
            }],
            &[Objective::Dig {
                target: 0,
                count: 12,
            }],
        ],
        beats: &["Blast the rubble", "Clear the way through"],
        steps: &[
            ArcStep {
                journal: "The old mine in {a place} {where} caved in. Bomb the marked rubble in order.",
                nudge: "Mind the order. The old foreman always said: left, right, then pray.",
                contact: "",
            },
            ArcStep {
                journal: "The rubble's down. Dig on through to the old tunnels.",
                nudge: "Can you see the old tunnels yet? Rails? Lamps?",
                contact: "",
            },
        ],
        climax: "Beyond the rubble the tunnels are swept clean. The lamps are trimmed. A tin mug sits on a crate, still warm.",
        twists: &[],
        reward: QuestReward::credits(450, 12).bombs(4),
        outcomes: &[
            ArcOutcome {
                label: "Reopen the mine",
                line: "",
                epilogue: "The mine is working again and {village} has money for paint. The miners leave a mug of tea at the far end each night. By morning it's always empty.",
                when: OutcomeWhen::Completed,
                effects: &[
                    ArcEffect::Prosperity(2),
                    ArcEffect::Reputation(12),
                    ArcEffect::Remember,
                    ArcEffect::FollowUp(ArcId(5)),
                ],
            },
            ArcOutcome {
                label: "Leave it sealed",
                line: "",
                epilogue: "The mine stayed shut. {Village} tightened its belt, and some nights people swear they hear tidy footsteps under the square.",
                when: OutcomeWhen::Abandoned,
                effects: &[ArcEffect::Prosperity(-1)],
            },
        ],
        pitch: ToneLines {
            gruff: &[
                "Old mine came down. Village needs it open. Three charges at the marks by {a place}, in order, then dig through. Here's powder.",
            ],
            cheerful: &[
                "Let's reopen the old mine! Three big booms at the marks near {a place}, then dig, dig, dig! {Village} will throw you a parade! I'll organise it!",
            ],
            nervous: &[
                "The old mine caved in. Everyone's been so worried since. Three charges near {a place}, in order, then clear it? Please be careful. Ceilings are... ceilings.",
            ],
            poetic: &[
                "Our mine fell silent the day the roof came down. Wake it, {player}: three charges by {a place}, then cut a path back into the dark.",
            ],
            dry: &[
                "The mine's sealed. The village is sad. You have bombs. I feel like this solves itself. Three marks by {a place}, in order.",
            ],
            earnest: &[
                "Since the mine caved in, {village} has been struggling. Would you blast the rubble near {a place} and clear the way? It would save us.",
            ],
        },
        thanks: ToneLines {
            gruff: &[
                "Open again. And swept? Somebody's been down there. ...Leave them be. Good work.",
            ],
            cheerful: &[
                "It's open! And someone's been dusting?! We have a mystery neighbour! I'm baking them a cake!",
            ],
            nervous: &[
                "The lamps were already lit? Somebody lives down there? That's... that's fine. That's fine. Thank you.",
            ],
            poetic: &[
                "The mine breathes again. And in the dark, someone had kept the lamps trimmed, waiting for us.",
            ],
            dry: &[
                "The mine's open, and someone's been tidying it. Better housekeeping than mine. Thank you, {player}.",
            ],
            earnest: &[
                "You've given {village} its future back, {player}. Whoever's down there, I hope they're friendly.",
            ],
        },
        log: log(
            "The village's old mine caved in. Blast the rubble and clear a way through.",
            "Blow the three marks in order, then dig through.",
            "The mine is open. Somebody had been keeping its lamps lit.",
        ),
    },
];

mod crew;
mod deep_settlements;
mod far_places;
mod frontier;
mod home;
mod mystery;
mod night;
mod night_market;
mod pilot;
mod sky;
mod tombs;
mod village;

/// Every batch of arcs, in authoring order ([`super::ARC_TEMPLATES`] chains them).
pub const BATCHES: &[&[ArcTemplate]] = &[
    BATCH_FIRST,
    frontier::BATCH,
    village::BATCH,
    sky::BATCH,
    mystery::BATCH,
    home::BATCH,
    night::BATCH,
    pilot::BATCH,
    crew::BATCH,
    // Wave 6 content.
    tombs::BATCH,
    night_market::BATCH,
    far_places::BATCH,
    // Wave 10: the deep settlements' own stories.
    deep_settlements::BATCH,
];
