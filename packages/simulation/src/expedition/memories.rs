//! What people in the deep settlements remember of the Fathers' expedition.
//!
//! Plain data with **stable ids** for the settlement tracks and the integrator to wire into
//! their people's talk (Gravel Hollow in the Grey Stone, the Descent II camp, the Hive). A line
//! can wait for a log ([`SettlementMemory::after_log`]): people open up about what the crew has
//! already read, so the town's memories follow the trail instead of spoiling it. Ids are never
//! renamed or reused; new lines append.

use super::ExpeditionState;

/// A deep settlement that remembers the expedition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Settlement {
    /// The shallow village in the Grey Stone (Descent I).
    GravelHollow,
    /// The prospectors' camp at the Descent II outpost.
    OldDeepCamp,
    /// The insect city in the Living Dark (Descent III).
    TheHive,
}

impl Settlement {
    pub const ALL: [Self; 3] = [Self::GravelHollow, Self::OldDeepCamp, Self::TheHive];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GravelHollow => "Gravel Hollow",
            Self::OldDeepCamp => "the Old Deep camp",
            Self::TheHive => "the Hive",
        }
    }
}

/// One remembered line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementMemory {
    /// Stable id (`settlement.speaker.topic`): what a settlement's people key their talk on.
    pub id: &'static str,
    pub settlement: Settlement,
    /// Who says it (a role the settlement track casts).
    pub speaker: &'static str,
    /// The line waits until the crew has read this log (1-based), if set.
    pub after_log: Option<u8>,
    pub line: &'static str,
}

impl SettlementMemory {
    /// Whether a crew with `state` has read far enough to hear it.
    #[must_use]
    pub fn open(&self, state: &ExpeditionState) -> bool {
        self.after_log.is_none_or(|log| state.found >= log)
    }
}

/// Every remembered line.
pub const MEMORIES: [SettlementMemory; 15] = [
    SettlementMemory {
        id: "gravel_hollow.smith.rails",
        settlement: Settlement::GravelHollow,
        speaker: "Old Tully, the smith",
        after_log: Some(5),
        line: "The Fathers? Slept in my loft, all six of them. Amos asked about the rails and I \
               told him what my granddad told me: they were here before the Hollow was. He just \
               nodded, like that was what he'd come all this way to hear.",
    },
    SettlementMemory {
        id: "gravel_hollow.smith.lucky_stone",
        settlement: Settlement::GravelHollow,
        speaker: "Old Tully, the smith",
        after_log: Some(6),
        line: "Young Teo bought a lucky stone off my grandson. A whole credit, for a rock. If \
               you find him down there, tell him it worked. He got further than anyone.",
    },
    SettlementMemory {
        id: "gravel_hollow.innkeeper.stew",
        settlement: Settlement::GravelHollow,
        speaker: "the innkeeper",
        after_log: None,
        line: "Ruth Brannigan cooked a stew in my kitchen that folk still ask after. Wouldn't \
               give me the recipe. Said she'd send it up the line from the bottom.",
    },
    SettlementMemory {
        id: "gravel_hollow.child.lights",
        settlement: Settlement::GravelHollow,
        speaker: "a child",
        after_log: Some(1),
        line: "Gran says the Fathers' lamps went down the cable one by one, and the last one \
               blinked twice. Like goodbye. Or like: come on.",
    },
    SettlementMemory {
        id: "gravel_hollow.miner.hum",
        settlement: Settlement::GravelHollow,
        speaker: "a miner",
        after_log: None,
        line: "The hum went quiet the year they went down. Then it came back louder. Some say \
               they woke something. Some say they're still digging.",
    },
    SettlementMemory {
        id: "old_deep_camp.quartermaster.splice",
        settlement: Settlement::OldDeepCamp,
        speaker: "the quartermaster",
        after_log: Some(20),
        line: "There's a splice in the old drop line just above the storm zone. Halloran's \
               work, you can tell: neat, double wrapped. The cut below it isn't neat at all.",
    },
    SettlementMemory {
        id: "old_deep_camp.prospector.valve_key",
        settlement: Settlement::OldDeepCamp,
        speaker: "a prospector",
        after_log: Some(15),
        line: "Somebody turned one of the Cathedral's valves long before us, and left the key in \
               a cairn. Saved us a week and probably a leg.",
    },
    SettlementMemory {
        id: "old_deep_camp.medic.iodine",
        settlement: Settlement::OldDeepCamp,
        speaker: "the medic",
        after_log: None,
        line: "Found a tin of iodine in a lead tent down in the glow. The label says \
               \"R. Brannigan. ONE A DAY, DOV. I MEAN IT.\" I keep it on my shelf for luck.",
    },
    SettlementMemory {
        id: "old_deep_camp.surveyor.marks",
        settlement: Settlement::OldDeepCamp,
        speaker: "the surveyor",
        after_log: Some(18),
        line: "Calloway's survey marks are still the best down here. Plumb to the cable, every \
               one. We copy her numbers and add our names underneath.",
    },
    SettlementMemory {
        id: "old_deep_camp.cook.trespass",
        settlement: Settlement::OldDeepCamp,
        speaker: "the cook",
        after_log: Some(21),
        line: "The stories said the Star Core was the bottom. The Fathers went past it. We go \
               past it every week now, and every week it still feels like trespassing.",
    },
    SettlementMemory {
        id: "hive.elder.soft_shells",
        settlement: Settlement::TheHive,
        speaker: "a worker elder",
        after_log: None,
        line: "Click-hum. Soft-shells came before you, carrying small lights. Like the \
               Lamp-Bearers, but little and afraid. We liked them.",
    },
    SettlementMemory {
        id: "hive.pump_tender.wrench_one",
        settlement: Settlement::TheHive,
        speaker: "a pump tender",
        after_log: Some(27),
        line: "The Wrench-One kept the warm pumps singing for many broods. He taught the young \
               his name, Teo, and they said it wrong to make him laugh. His tools still hang in \
               the nursery. We polish them.",
    },
    SettlementMemory {
        id: "hive.nurse.humming",
        settlement: Settlement::TheHive,
        speaker: "a nurse",
        after_log: Some(26),
        line: "The singing soft-shell hummed to the brood when she was frightened. The brood \
               still hums it. Listen.",
    },
    SettlementMemory {
        id: "hive.soldier.warning",
        settlement: Settlement::TheHive,
        speaker: "a soldier",
        after_log: None,
        line: "Soft-shell. The last of your kind who went down did not come back up. Turn \
               around. (It does not move aside.)",
    },
    SettlementMemory {
        id: "hive.queen.engine",
        settlement: Settlement::TheHive,
        speaker: "the queen's voice",
        after_log: Some(30),
        line: "The light below is not a stone. It sleeps, and it dreams of the sky. The \
               Lamp-Bearers will come back to wake it. Or you will.",
    },
];

/// The lines `settlement`'s people may say to a crew with `state`, in order.
pub fn lines_for(
    settlement: Settlement,
    state: &ExpeditionState,
) -> impl Iterator<Item = &'static SettlementMemory> + '_ {
    MEMORIES
        .iter()
        .filter(move |memory| memory.settlement == settlement && memory.open(state))
}

/// The line with stable id `id`.
#[must_use]
pub fn memory(id: &str) -> Option<&'static SettlementMemory> {
    MEMORIES.iter().find(|memory| memory.id == id)
}
