//! The first quest content batch: five town keepers' shallow and mid-tier chains plus the HQ
//! board's templates.
//!
//! Every quest states its hook. Ids are stable (saved in quest logs): Kade 1xx, Mara 2xx,
//! Nix 3xx, Iona 4xx, Pip 5xx, board templates 9xx. Bands: 1 clay, 2 grey stone, 3 slate,
//! 4 basalt, 5 Crystal Faults, 6 Fossil Oceans.

use crate::{
    economy::SurfaceZone,
    infrastructure::InfrastructureKind,
    poi::{PoiKind, RigFitting},
    terrain::{MineralKind, StrategicResourceKind},
    worldgen::FeatureKind,
};

use super::{
    BlueprintId, Objective, Prerequisite, QuestDef, QuestGiver, QuestId, QuestItem, QuestReward,
    QuestText, QuestTier, TargetSpec, Twist, UpgradeStat,
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

const fn text(offer: &'static str, progress: &'static str, complete: &'static str) -> QuestText {
    QuestText {
        offer,
        progress,
        complete,
    }
}

const ALLOY: StrategicResourceKind = StrategicResourceKind::AncientAlloy;

/// The authored quests.
pub const QUESTS: &[QuestDef] = &[
    // ---------------------------------------------------------------- Kade (depot) ----------
    QuestDef {
        id: QuestId(101),
        title: "Scales Don't Lie",
        giver: QuestGiver::Kade,
        tier: QuestTier::Shallow,
        hook: "A first delivery against the market clock: plan one tight dive instead of a lazy one.",
        prerequisites: &[],
        targets: &[],
        stages: &[&[Objective::Deliver {
            item: QuestItem::Mineral(MineralKind::Iron),
            count: 5,
        }]],
        twists: &[Twist::TimeLimit { seconds: 420 }],
        reward: QuestReward::credits(170, 6),
        text: text(
            "Rival buyer up the road swears his scales are honest. Bring me five iron before the market closes and we'll weigh it on mine.",
            "Clock's running, rookie. Iron. Five. Today.",
            "Five iron, weighed twice. His scales were off by a third. You just made me a believer.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(102),
        title: "Blind Assay",
        giver: QuestGiver::Kade,
        tier: QuestTier::Shallow,
        hook: "Read the rock by eye: find and work a relic bed without the scanner's help.",
        prerequisites: &[Prerequisite::Quest(QuestId(101))],
        targets: &[feature(FeatureKind::ArtifactBed, 1, 3)],
        stages: &[
            &[Objective::Dig {
                target: 0,
                count: 5,
            }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::NoScanner],
        reward: QuestReward::credits(230, 7),
        text: text(
            "There's a relic bed under town some collector wants appraised. I want an honest sample, the old way. No scanner. Eyes and nerve.",
            "No pinging. Look for the odd shapes in the rock.",
            "Good grain, good color. You've got an assayer's eye under all that grease.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(103),
        title: "Glass Jaw",
        giver: QuestGiver::Kade,
        tier: QuestTier::Shallow,
        hook: "A fragile parcel turns every landing and rockfall into a decision.",
        prerequisites: &[Prerequisite::Depth(20)],
        targets: &[poi(PoiKind::SupplyCache, 1, 3)],
        stages: &[
            &[Objective::PickUp {
                target: 0,
                parcel: "crate of assay glass",
            }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::Fragile { max_damage: 15 }],
        reward: QuestReward::credits(280, 8),
        text: text(
            "Someone stashed my assay glassware in a supply hollow. It's thinner than your excuses. Bring it up without a scratch.",
            "Gentle on the thrusters. Every bump is coming out of your pay.",
            "Clean as the day it was blown. I'd hug you if you weren't covered in clay.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(104),
        title: "Two Counters",
        giver: QuestGiver::Kade,
        tier: QuestTier::Mid,
        hook: "Two buyers must never compare notes: split the silver and drop both halves at once.",
        prerequisites: &[
            Prerequisite::Reputation(QuestGiver::Kade, 10),
            Prerequisite::Depth(30),
        ],
        targets: &[],
        stages: &[&[Objective::SplitDelivery {
            item: QuestItem::Mineral(MineralKind::Silver),
            count: 2,
            zones: [SurfaceZone::Depot, SurfaceZone::Salvage],
            window_ms: 8_000,
            solo_window_ms: 120_000,
        }]],
        twists: &[],
        reward: QuestReward::credits(460, 10).materials(&[(ALLOY, 1)]),
        text: text(
            "Two silver at my counter, two at Mara's, close enough together that neither buyer hears about the other. Bring a partner or bring speed.",
            "Drop both halves before the first buyer gets chatty.",
            "Both buyers paid top rate and neither knows. This is why we're friends.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(105),
        title: "Heavy Haul",
        giver: QuestGiver::Kade,
        tier: QuestTier::Mid,
        hook: "Dig a road for a crawling ore sled and shepherd it home; two rigs push it twice as fast.",
        prerequisites: &[
            Prerequisite::Reputation(QuestGiver::Kade, 20),
            Prerequisite::Depth(50),
        ],
        targets: &[poi(PoiKind::RivalClaim, 2, 4)],
        stages: &[&[Objective::Escort {
            target: 0,
            step_ms: 1_300,
            crew_step_ms: 650,
        }]],
        twists: &[],
        reward: QuestReward::credits(900, 12)
            .materials(&[(ALLOY, 2)])
            .fragment(BlueprintId::HaulerFrame),
        text: text(
            "The old rival outpost left an ore sled full of concentrate. It crawls on its own if someone walks beside it. It only climbs open tunnel, so dig it a road.",
            "Stay close to the sled and keep the tunnel open up to town.",
            "That sled's worth more than the outpost was. Here's your cut and a frame drawing I've been saving.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(106),
        title: "The Motherlode Ledger",
        giver: QuestGiver::Kade,
        tier: QuestTier::Mid,
        hook: "Commit to one deep seam: no surfacing until it's surveyed and worked.",
        prerequisites: &[Prerequisite::Quest(QuestId(102)), Prerequisite::Depth(60)],
        targets: &[feature(FeatureKind::Motherlode, 2, 8)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Dig {
                target: 0,
                count: 10,
            }],
        ],
        twists: &[Twist::StayDown],
        reward: QuestReward::credits(720, 10)
            .materials(&[(ALLOY, 1)])
            .fragment(BlueprintId::HaulerFrame),
        text: text(
            "There's a seam down past the slate worth a ledger page of its own. Survey it, work it, and don't come up for air until it's done.",
            "Stay down. The seam doesn't care that you're tired.",
            "That's a full ledger page. Kade's Rule One: the deep seams pay the rent.",
        ),
        repeatable: false,
    },
    // ---------------------------------------------------------------- Mara (salvage) --------
    QuestDef {
        id: QuestId(201),
        title: "Echoes in the Stone",
        giver: QuestGiver::Mara,
        tier: QuestTier::Shallow,
        hook: "Learn to hunt with the scanner: ping a wreck's echo, then follow it in.",
        prerequisites: &[Prerequisite::Upgrade(UpgradeStat::Scanner, 1)],
        targets: &[poi(PoiKind::DerelictRig, 2, 3)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 0 }],
        ],
        twists: &[],
        reward: QuestReward::credits(190, 6),
        text: text(
            "A prospector rig went quiet under the grey stone. Your scanner will hear it before your eyes see it. Ping it, then go say hello.",
            "Pulse the scanner near the wreck, then climb inside.",
            "So that's where old Tamsin ended up. Thanks for finding her.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(202),
        title: "Last Words",
        giver: QuestGiver::Mara,
        tier: QuestTier::Shallow,
        hook: "Recover a black box with a light touch: bombs would wipe the recording.",
        prerequisites: &[Prerequisite::Depth(28)],
        targets: &[sealed(PoiKind::DerelictRig, 2, 4)],
        stages: &[&[Objective::RecoverLog { target: 0 }]],
        twists: &[Twist::NoBombs],
        reward: QuestReward::credits(270, 8).materials(&[(ALLOY, 1)]),
        text: text(
            "Every wreck keeps a log in its core. I collect last words. Drill the core out by hand. A blast scrambles the tape.",
            "Drill, don't blast. The recording is fragile.",
            "Listen to that. Still talking about the strike she never made. I'll keep her company.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(203),
        title: "Crate Expectations",
        giver: QuestGiver::Mara,
        tier: QuestTier::Shallow,
        hook: "Beat the scavengers to a stash: a short, sharp race through the clay.",
        prerequisites: &[],
        targets: &[sealed(PoiKind::SupplyCache, 1, 2)],
        stages: &[
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::TimeLimit { seconds: 300 }],
        reward: QuestReward::credits(160, 6).bombs(1),
        text: text(
            "Word on the radio is somebody's stash hollow is up for grabs. Get there first, crack a crate, and get back before the vultures do.",
            "Crack a crate and get home. The vultures are circling.",
            "First on the scene, first to profit. You're learning the salvage trade.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(204),
        title: "Salvage Rights",
        giver: QuestGiver::Mara,
        tier: QuestTier::Mid,
        hook: "Raid an abandoned outpost, then stake it with your own infrastructure.",
        prerequisites: &[Prerequisite::Quest(QuestId(202)), Prerequisite::Depth(45)],
        targets: &[sealed(PoiKind::RivalClaim, 2, 5)],
        stages: &[
            &[Objective::OpenPoi { target: 0 }],
            &[Objective::Build {
                target: 0,
                kind: InfrastructureKind::TunnelSupport,
            }],
        ],
        twists: &[],
        reward: QuestReward::credits(620, 10).kit(InfrastructureKind::SignalRelay),
        text: text(
            "An outpost nobody's paying rent on is an outpost waiting for a new owner. Crack a locker, then plant a support so the claim office knows it's ours.",
            "Open a locker, then brace the outpost with a tunnel support.",
            "Filed and stamped. That outpost has our name on it now.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(205),
        title: "Two-Key Recorder",
        giver: QuestGiver::Mara,
        tier: QuestTier::Mid,
        hook: "A wreck's recorder needs two keys turned at once: a natural two-rig job.",
        prerequisites: &[
            Prerequisite::Logs(1),
            Prerequisite::Reputation(QuestGiver::Mara, 10),
        ],
        targets: &[sealed(PoiKind::DerelictRig, 2, 5)],
        stages: &[
            &[Objective::Simultaneous {
                target: 0,
                spots: 2,
                window_ms: 3_000,
                solo_window_ms: 25_000,
            }],
            &[Objective::RecoverLog { target: 0 }],
        ],
        twists: &[],
        reward: QuestReward::credits(760, 12).fragment(BlueprintId::TowWinch),
        text: text(
            "Survey rigs had two-key recorders: two crew at two consoles. Stand on both marks together and the core unlocks. Alone? You'll have to be quick.",
            "Both marks lit together, then drill the core.",
            "Both keys turned. That's a crew log, and it mentions a winch design. Take a copy.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(206),
        title: "Ghost Signal",
        giver: QuestGiver::Mara,
        tier: QuestTier::Mid,
        hook: "Follow a distress signal through a gas pocket to a wreck deeper down.",
        prerequisites: &[Prerequisite::Quest(QuestId(202)), Prerequisite::Depth(65)],
        targets: &[
            sealed(PoiKind::DerelictRig, 4, 6),
            feature(FeatureKind::HazardCluster, 3, 5),
        ],
        stages: &[
            &[Objective::Survive {
                target: 1,
                seconds: 15,
            }],
            &[Objective::Scan { target: 0 }],
            &[Objective::RecoverLog { target: 0 }],
        ],
        twists: &[],
        reward: QuestReward::credits(980, 13)
            .materials(&[(ALLOY, 2)])
            .fragment(BlueprintId::BalloonRigging),
        text: text(
            "A distress loop keeps bleeding through a gas pocket. Sit in the haze long enough to triangulate, then chase it to the wreck it came from.",
            "Hold position in the hazard, then follow the signal down.",
            "Rescue beacon from a balloon rig. They tried to float out. Their rigging plans are yours.",
        ),
        repeatable: false,
    },
    // ---------------------------------------------------------------- Nix (explosives) ------
    QuestDef {
        id: QuestId(301),
        title: "Demolition 101",
        giver: QuestGiver::Nix,
        tier: QuestTier::Shallow,
        hook: "Bombs with a plan: two marked charges, in the right order.",
        prerequisites: &[],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[&[Objective::Detonate {
            target: 0,
            spots: 2,
        }]],
        twists: &[],
        reward: QuestReward::credits(180, 6).bombs(3),
        text: text(
            "Class is in session. I painted two marks by that cave. Blow the first, then the second. Wrong order and we start over, and I laugh.",
            "First mark, then second mark. Run between.",
            "Textbook! Here, a few charges for homework.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(302),
        title: "Choreography",
        giver: QuestGiver::Nix,
        tier: QuestTier::Shallow,
        hook: "A three-charge dance around a hazard pocket, on the clock.",
        prerequisites: &[Prerequisite::Quest(QuestId(301))],
        targets: &[feature(FeatureKind::HazardCluster, 1, 3)],
        stages: &[&[Objective::Detonate {
            target: 0,
            spots: 3,
        }]],
        twists: &[Twist::TimeLimit { seconds: 420 }],
        reward: QuestReward::credits(310, 8).bombs(2),
        text: text(
            "Three marks around a nasty pocket. One, two, three, boom-boom-boom. You have one song's length. Make it pretty.",
            "Three marks, in order, before the music stops.",
            "Bravo! The pocket never knew what hit it. Encore pay attached.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(303),
        title: "Quiet Hands",
        giver: QuestGiver::Nix,
        tier: QuestTier::Shallow,
        hook: "The bomb lady wants restraint: dig out an artifact bed with zero explosions.",
        prerequisites: &[Prerequisite::Quest(QuestId(301))],
        targets: &[feature(FeatureKind::ArtifactBed, 2, 4)],
        stages: &[&[Objective::Dig {
            target: 0,
            count: 8,
        }]],
        twists: &[Twist::NoBombs],
        reward: QuestReward::credits(250, 7),
        text: text(
            "A real demolitionist knows when not to. There's an artifact bed down there. Dig it out gently. If I hear a boom, I'll know.",
            "Drill only. Not one charge.",
            "Zero booms. I'm almost disappointed. Almost.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(304),
        title: "Seal Breaker",
        giver: QuestGiver::Nix,
        tier: QuestTier::Mid,
        hook: "Crack a sealed vault's lock: every vault is a little puzzle of drill, bomb and timing.",
        prerequisites: &[
            Prerequisite::Reputation(QuestGiver::Nix, 10),
            Prerequisite::Depth(80),
        ],
        targets: &[sealed(PoiKind::Vault, 4, 6)],
        stages: &[&[Objective::OpenPoi { target: 0 }]],
        twists: &[],
        reward: QuestReward::credits(920, 12).fragment(BlueprintId::DampenerCoil),
        text: text(
            "Old-world vault, walls my charges can't scratch. The lock's the weak point: switches, seals or relays. Figure out which and pop it.",
            "Read the lock. Work it. Get inside.",
            "Open! Inside was a drawing of a coil that hums. Nix wants one. Nix gets one. Here's a copy.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(305),
        title: "Twin Fuses",
        giver: QuestGiver::Nix,
        tier: QuestTier::Mid,
        hook: "Arm two fuse boxes at the same moment, then set the charges: a co-op countdown.",
        prerequisites: &[Prerequisite::Quest(QuestId(302)), Prerequisite::Depth(40)],
        targets: &[feature(FeatureKind::LargeCavern, 2, 4)],
        stages: &[
            &[Objective::Simultaneous {
                target: 0,
                spots: 2,
                window_ms: 2_500,
                solo_window_ms: 30_000,
            }],
            &[Objective::Detonate {
                target: 0,
                spots: 2,
            }],
        ],
        twists: &[],
        reward: QuestReward::credits(680, 11).bombs(4),
        text: text(
            "Two fuse boxes, one cavern. Arm them together or the safety trips. Then blow the marks. Bring a friend. Or very good legs.",
            "Stand on both fuse boxes together, then fire the marks in order.",
            "Ka-boom in stereo. That's what I call teamwork.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(306),
        title: "Dead Man's Switch",
        giver: QuestGiver::Nix,
        tier: QuestTier::Mid,
        hook: "One rig holds the dead man's switch while the other dives for the formation.",
        prerequisites: &[Prerequisite::Quest(QuestId(302)), Prerequisite::Depth(50)],
        targets: &[
            feature(FeatureKind::LargeCavern, 2, 4),
            poi(PoiKind::Formation, 3, 5),
        ],
        stages: &[&[Objective::HoldAndClimb {
            switch: 0,
            goal: 1,
            solo_latch_ms: 30_000,
        }]],
        twists: &[],
        reward: QuestReward::credits(720, 11).materials(&[(ALLOY, 2)]),
        text: text(
            "I rigged a formation to come loose, and the trigger's a dead man's switch. Someone sits on it while someone else gets to the formation. Solo? The switch latches for half a minute. Run.",
            "Keep the switch held while a rig reaches the formation.",
            "Formation freed, nobody squashed. My favorite kind of Tuesday.",
        ),
        repeatable: false,
    },
    // ---------------------------------------------------------------- Iona (repair) ---------
    QuestDef {
        id: QuestId(401),
        title: "Shakedown",
        giver: QuestGiver::Iona,
        tier: QuestTier::Shallow,
        hook: "Fly clean: a test dive where every crash landing counts against you.",
        prerequisites: &[],
        targets: &[],
        stages: &[
            &[Objective::ReachDepth { row: 26 }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::Fragile { max_damage: 20 }],
        reward: QuestReward::credits(150, 6),
        text: text(
            "Fresh welds need a shakedown. Take her down past the clay and back, gentle. If I see new dents, we do it again.",
            "Down past the clay, back up, no crash landings.",
            "Welds held, pilot held. You fly smoother than you talk.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(402),
        title: "Breathe Easy",
        giver: QuestGiver::Iona,
        tier: QuestTier::Shallow,
        hook: "Sit inside a hazard pocket on purpose and learn what your hull can take.",
        prerequisites: &[Prerequisite::Quest(QuestId(401))],
        targets: &[feature(FeatureKind::HazardCluster, 1, 3)],
        stages: &[&[Objective::Survive {
            target: 0,
            seconds: 12,
        }]],
        twists: &[],
        reward: QuestReward::credits(210, 7)
            .materials(&[(ALLOY, 1)])
            .kit(InfrastructureKind::TunnelSupport),
        text: text(
            "I need field data on my new seals. Park inside a hazard pocket for a while and let the sensors listen. I'll fix whatever melts.",
            "Stay in the pocket until the sensors have enough.",
            "Great data. Your seals held at a temperature I won't repeat. Take this support kit, you'll want it.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(403),
        title: "Pressure Test",
        giver: QuestGiver::Iona,
        tier: QuestTier::Mid,
        hook: "A long soak deep down with no bailing out to the surface halfway.",
        prerequisites: &[Prerequisite::Quest(QuestId(402)), Prerequisite::Depth(45)],
        targets: &[feature(FeatureKind::HazardCluster, 3, 4)],
        stages: &[&[Objective::Survive {
            target: 0,
            seconds: 25,
        }]],
        twists: &[Twist::StayDown],
        reward: QuestReward::credits(540, 10),
        text: text(
            "Next test is longer and deeper. Once you're down, you stay down until the soak is done. Bring fuel. Bring patience.",
            "Soak in the pocket. Don't surface until it's logged.",
            "Twenty-five seconds in that soup and you're grinning. Rig's rated now.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(404),
        title: "Beacon Relay",
        giver: QuestGiver::Iona,
        tier: QuestTier::Mid,
        hook: "Pass a hot diagnostic beacon rig to rig down to a hazard pocket.",
        prerequisites: &[Prerequisite::Quest(QuestId(402)), Prerequisite::Depth(40)],
        targets: &[feature(FeatureKind::HazardCluster, 2, 4)],
        stages: &[&[Objective::Relay {
            target: 0,
            leg_rows: 22,
            solo_cooldown_ms: 12_000,
        }]],
        twists: &[],
        reward: QuestReward::credits(700, 11).fragment(BlueprintId::TowWinch),
        text: text(
            "This beacon overheats if one rig carries it too far. Hand it off rig to rig down to the pocket. Alone, you'll have to set it down and let it cool.",
            "Carry the beacon down, handing it off before it overheats.",
            "Beacon's planted and still beeping. Iona approves. Here's a winch sketch for next time someone gets stuck.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(405),
        title: "Heat Soak",
        giver: QuestGiver::Iona,
        tier: QuestTier::Mid,
        hook: "Bake in a deep hazard pocket, then nurse the sensor home without a scratch.",
        prerequisites: &[Prerequisite::Quest(QuestId(403))],
        targets: &[feature(FeatureKind::HazardCluster, 4, 5)],
        stages: &[
            &[Objective::Survive {
                target: 0,
                seconds: 20,
            }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::Fragile { max_damage: 60 }],
        reward: QuestReward::credits(860, 13).fitting(RigFitting::Bulkhead),
        text: text(
            "Deep heat this time. Soak, then bring the sensor home in one piece. Big dents spoil the readings.",
            "Soak in the deep pocket, then fly the sensor home gently.",
            "Perfect readings. You've earned a bulkhead from my private stock.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(406),
        title: "Shore It Up",
        giver: QuestGiver::Iona,
        tier: QuestTier::Shallow,
        hook: "First taste of infrastructure: brace a cavern, then widen it safely.",
        prerequisites: &[Prerequisite::Quest(QuestId(402))],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[
            &[Objective::Build {
                target: 0,
                kind: InfrastructureKind::TunnelSupport,
            }],
            &[Objective::Dig {
                target: 0,
                count: 6,
            }],
        ],
        twists: &[],
        reward: QuestReward::credits(300, 8),
        text: text(
            "That cave is one sneeze from collapsing. Set the support kit I gave you, then widen the cave behind it.",
            "Place a tunnel support at the cave, then dig it wider.",
            "Solid as my coffee. That's how you build a mine that lasts.",
        ),
        repeatable: false,
    },
    // ---------------------------------------------------------------- Pip (fuel) ------------
    QuestDef {
        id: QuestId(501),
        title: "Fumes and Glory",
        giver: QuestGiver::Pip,
        tier: QuestTier::Shallow,
        hook: "A pure speed run: down past the clay and back before Pip's stopwatch beeps.",
        prerequisites: &[],
        targets: &[],
        stages: &[
            &[Objective::ReachDepth { row: 22 }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::TimeLimit { seconds: 150 }],
        reward: QuestReward::credits(140, 6),
        text: text(
            "I bet you can't touch the bottom of the clay and get back before my stopwatch beeps. Loser buys fuel. Winner gets paid.",
            "Down past the clay and back! Go go go!",
            "New record! For you. Mine's better. But still!",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(502),
        title: "Hot Delivery",
        giver: QuestGiver::Pip,
        tier: QuestTier::Shallow,
        hook: "Two drop-offs on one tank and one clock: route planning under pressure.",
        prerequisites: &[Prerequisite::Quest(QuestId(501))],
        targets: &[
            poi(PoiKind::SupplyCache, 1, 3),
            feature(FeatureKind::LargeCavern, 1, 4),
        ],
        stages: &[&[
            Objective::Reach { target: 0 },
            Objective::Reach { target: 1 },
        ]],
        twists: &[Twist::TimeLimit { seconds: 360 }],
        reward: QuestReward::credits(240, 7),
        text: text(
            "Two drop-offs, one tank: the supply hollow and that big cave. Any order you like, but the clock doesn't care about your feelings.",
            "Hit both drop-offs before time runs out.",
            "Both delivered and still hot! You're almost as fast as me.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(503),
        title: "Relay Race",
        giver: QuestGiver::Pip,
        tier: QuestTier::Shallow,
        hook: "Pass Pip's baton down the shaft rig to rig; alone you rest it and race on.",
        prerequisites: &[Prerequisite::Quest(QuestId(501))],
        targets: &[poi(PoiKind::SupplyCache, 1, 3)],
        stages: &[&[Objective::Relay {
            target: 0,
            leg_rows: 14,
            solo_cooldown_ms: 8_000,
        }]],
        twists: &[],
        reward: QuestReward::credits(330, 10),
        text: text(
            "Fuel-cell baton, from my pump to that supply hollow. It gets too hot to hold after a few lengths, so pass it! No partner? Set it down, count to eight, grab it again.",
            "Carry the baton down. Hand it off before it overheats.",
            "Baton delivered! I timed it. You don't want to know. Kidding, it was great.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(504),
        title: "Scenic Route",
        giver: QuestGiver::Pip,
        tier: QuestTier::Mid,
        hook: "A sightseeing loop through the mid layers, with no shortcuts through the rock.",
        prerequisites: &[Prerequisite::Quest(QuestId(502)), Prerequisite::Depth(50)],
        targets: &[
            poi(PoiKind::Formation, 3, 5),
            poi(PoiKind::DerelictRig, 3, 5),
        ],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 1 }],
            &[Objective::ReturnToSurface],
        ],
        twists: &[Twist::NoBombs],
        reward: QuestReward::credits(660, 10).materials(&[(ALLOY, 1)]),
        text: text(
            "Postcard run! Scan the pretty formation, visit the spooky wreck, come home. No blasting shortcuts. Postcards are about the journey.",
            "Scan the formation, visit the wreck, then home. No bombs.",
            "Best postcard yet. Mara says the wreck waved. Probably the wind.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(505),
        title: "Synchronized Fueling",
        giver: QuestGiver::Pip,
        tier: QuestTier::Mid,
        hook: "Three valves around a cavern, all open at once: grab your crew or sprint.",
        prerequisites: &[Prerequisite::Quest(QuestId(503)), Prerequisite::Depth(45)],
        targets: &[feature(FeatureKind::LargeCavern, 2, 4)],
        stages: &[&[Objective::Simultaneous {
            target: 0,
            spots: 3,
            window_ms: 4_000,
            solo_window_ms: 45_000,
        }]],
        twists: &[],
        reward: QuestReward::credits(800, 12).fitting(RigFitting::AuxiliaryTank),
        text: text(
            "There's a fuel seep in that cave with three valves. Open them together and the pressure balances. Open them slowly and, well, you'll see.",
            "All three valves held together.",
            "Pressure balanced! Have a tank I definitely didn't take off a customer.",
        ),
        repeatable: false,
    },
    QuestDef {
        id: QuestId(506),
        title: "Express Lane",
        giver: QuestGiver::Pip,
        tier: QuestTier::Mid,
        hook: "Clear a road and walk a slow fuel crawler home before its tank runs dry.",
        prerequisites: &[Prerequisite::Quest(QuestId(503)), Prerequisite::Depth(50)],
        targets: &[poi(PoiKind::SupplyCache, 3, 4)],
        stages: &[&[Objective::Escort {
            target: 0,
            step_ms: 1_100,
            crew_step_ms: 600,
        }]],
        twists: &[Twist::TimeLimit { seconds: 900 }],
        reward: QuestReward::credits(880, 12).fragment(BlueprintId::BalloonRigging),
        text: text(
            "My fuel crawler is stuck at a supply hollow way down. It follows rigs through open tunnel. Clear it a lane and walk it home before its tank dies.",
            "Keep the crawler moving and its road clear.",
            "The crawler made it! Here, balloon plans from its glovebox. Don't ask.",
        ),
        repeatable: false,
    },
];

/// The HQ board's templates: posted with fresh places (a binding salt) and one twist each day.
pub const BOARD_TEMPLATES: &[QuestDef] = &[
    QuestDef {
        id: QuestId(901),
        title: "Survey Contract",
        giver: QuestGiver::Vale,
        tier: QuestTier::Shallow,
        hook: "HQ wants eyes on a cavern; each posting adds a different complication.",
        prerequisites: &[],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[
            &[Objective::Scan { target: 0 }],
            &[Objective::Reach { target: 0 }],
        ],
        twists: &[
            Twist::TimeLimit { seconds: 360 },
            Twist::NoBombs,
            Twist::StayDown,
        ],
        reward: QuestReward::credits(200, 3),
        text: text(
            "HQ needs a fresh survey of a cavern. Scan it, then put a rig inside.",
            "Scan the cavern, then go in.",
            "Survey filed. HQ thanks you for your diligence.",
        ),
        repeatable: true,
    },
    QuestDef {
        id: QuestId(902),
        title: "Stash Recovery",
        giver: QuestGiver::Vale,
        tier: QuestTier::Shallow,
        hook: "Crack a supply stash for HQ under whatever condition the posting sets.",
        prerequisites: &[Prerequisite::Depth(15)],
        targets: &[sealed(PoiKind::SupplyCache, 1, 4)],
        stages: &[&[Objective::OpenPoi { target: 0 }]],
        twists: &[
            Twist::TimeLimit { seconds: 300 },
            Twist::Fragile { max_damage: 25 },
            Twist::NoScanner,
        ],
        reward: QuestReward::credits(180, 3).bombs(1),
        text: text(
            "An unclaimed supply stash has turned up on our maps. Crack a crate and HQ will match its value.",
            "Open a crate in the stash.",
            "Stash logged. Matching payment issued.",
        ),
        repeatable: true,
    },
    QuestDef {
        id: QuestId(903),
        title: "Blast Permit",
        giver: QuestGiver::Vale,
        tier: QuestTier::Shallow,
        hook: "Sanctioned demolition around a hazard pocket, with a rotating safety rule.",
        prerequisites: &[Prerequisite::Quest(QuestId(301))],
        targets: &[feature(FeatureKind::HazardCluster, 1, 4)],
        stages: &[&[Objective::Detonate {
            target: 0,
            spots: 2,
        }]],
        twists: &[
            Twist::TimeLimit { seconds: 360 },
            Twist::Fragile { max_damage: 30 },
        ],
        reward: QuestReward::credits(240, 3).bombs(2),
        text: text(
            "Permit granted to vent a hazard pocket: two marked charges, in order.",
            "Fire the marked charges in order.",
            "Pocket vented. Permit closed.",
        ),
        repeatable: true,
    },
    QuestDef {
        id: QuestId(904),
        title: "Core Assay",
        giver: QuestGiver::Vale,
        tier: QuestTier::Shallow,
        hook: "Dig out a relic bed for HQ's assayers, under a different constraint each time.",
        prerequisites: &[Prerequisite::Depth(20)],
        targets: &[feature(FeatureKind::ArtifactBed, 1, 4)],
        stages: &[&[Objective::Dig {
            target: 0,
            count: 8,
        }]],
        twists: &[
            Twist::NoScanner,
            Twist::StayDown,
            Twist::TimeLimit { seconds: 420 },
        ],
        reward: QuestReward::credits(220, 3),
        text: text(
            "HQ's assayers want a fresh cut from a relic bed. Dig into it.",
            "Dig into the relic bed.",
            "Samples received. Assay pending. Payment isn't.",
        ),
        repeatable: true,
    },
    QuestDef {
        id: QuestId(905),
        title: "Joint Venture",
        giver: QuestGiver::Vale,
        tier: QuestTier::Shallow,
        hook: "A paired survey: two marks lit together (or one fast pilot), with a rotating catch.",
        prerequisites: &[Prerequisite::Depth(15)],
        targets: &[feature(FeatureKind::LargeCavern, 1, 4)],
        stages: &[&[Objective::Simultaneous {
            target: 0,
            spots: 2,
            window_ms: 3_000,
            solo_window_ms: 30_000,
        }]],
        twists: &[Twist::TimeLimit { seconds: 480 }, Twist::NoBombs],
        reward: QuestReward::credits(260, 4),
        text: text(
            "Paired survey markers at a cavern need to be read together. Crews welcome.",
            "Light both markers together.",
            "Paired readings filed. Good coordination.",
        ),
        repeatable: true,
    },
];
