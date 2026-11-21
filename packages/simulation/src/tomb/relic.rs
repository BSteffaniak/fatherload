//! Relics: run-only perks picked between floors (WP-TOMBS).
//!
//! The list is data ([`RELICS`]): each [`Relic`] has a name, a blurb, a [`RelicKind`] and its
//! [`RelicEffects`]. A member's effects are the sum of their relics' plus every
//! [`SYNERGIES`] pair they hold ([`effects_of`]). Effects reach gameplay through:
//!
//! * the pilot's body ([`crate::pilot::PilotBoons`] on the pilot entity: extra jumps, move and
//!   jump speed, the glider boost, no fall damage), refreshed whenever relics change;
//! * health and regeneration (`world/tombs.rs` scales the pilot's max health and regen);
//! * damage dealt and taken (`WorldState::damage_entity` scales a member's hits and heals the
//!   vampiric; `damage_pilot` and strikes take the armour and curses into account);
//! * projectiles (ricochet bounces off rock, `world/weapons.rs`);
//! * the light and the traps (trap sight shows hidden traps and widens the headlamp);
//! * loot (caches pay more).
//!
//! Between floors each member is offered three perks and one bargain (a blessing tied to a
//! curse: [`RelicKind::Bargain`]), rolled from the run seed, the floor and the member
//! ([`offers`]); taking nothing is allowed.

use serde::{Deserialize, Serialize};

use crate::worldgen::noise::RegionRng;

const SALT_OFFERS: u64 = 0x70B5_4E11_0003;

/// Whether a relic is a plain perk or a risk-reward bargain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum RelicKind {
    /// A plain run-only perk.
    Perk,
    /// A blessing bound to a curse: strong upside, a real cost.
    Bargain,
}

/// Every relic (append only: the index is the codex entry and the save/replication value).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Relic {
    /// Double jump.
    SpringHeels,
    /// Personal-weapon shots bounce off rock once.
    RicochetRounds,
    /// Shows hidden traps and widens the headlamp.
    TrapSightLantern,
    /// Hits on creatures heal the pilot.
    VampiricPick,
    /// The glider falls slower and flies faster.
    GliderBoost,
    /// More health.
    IronLungs,
    /// Faster on foot.
    Quickstep,
    /// No fall damage.
    Featherfall,
    /// Caches pay more.
    Gravedigger,
    /// Takes less damage.
    BoneCharm,
    /// Hits harder.
    HuntersMark,
    /// Heals faster after a hit.
    SecondWind,
    /// Bargain: a lot more loot, but enemies and traps hurt much more.
    PactOfGreed,
    /// Bargain: hit much harder, but less health.
    GlassCannon,
    /// Bargain: much faster with a triple jump, but no healing at all.
    RestlessSoul,
}

/// What a relic (or a set of them) does. Additive; zero is no effect.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RelicEffects {
    /// Extra jumps in the air.
    pub extra_jumps: u8,
    /// Walk / run speed, percent.
    pub move_pct: i16,
    /// Jump speed, percent.
    pub jump_pct: i16,
    /// Glider: slower fall and faster glide, percent.
    pub glide_pct: i16,
    /// Max health, percent.
    pub health_pct: i16,
    /// Damage dealt, percent.
    pub damage_pct: i16,
    /// Damage taken, percent (negative: less).
    pub taken_pct: i16,
    /// Health regained per point of damage dealt to creatures, percent.
    pub lifesteal_pct: u8,
    /// Personal-weapon shots bounce off rock this many times.
    pub ricochets: u8,
    /// Hidden traps show and the headlamp reaches further.
    pub trap_sight: bool,
    /// No fall damage.
    pub featherfall: bool,
    /// Health regeneration, percent (-100: none).
    pub regen_pct: i16,
    /// Cache loot, percent.
    pub loot_pct: i16,
}

impl RelicEffects {
    /// Adds `other` to these effects.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            extra_jumps: self.extra_jumps.saturating_add(other.extra_jumps),
            move_pct: self.move_pct.saturating_add(other.move_pct),
            jump_pct: self.jump_pct.saturating_add(other.jump_pct),
            glide_pct: self.glide_pct.saturating_add(other.glide_pct),
            health_pct: self.health_pct.saturating_add(other.health_pct),
            damage_pct: self.damage_pct.saturating_add(other.damage_pct),
            taken_pct: self.taken_pct.saturating_add(other.taken_pct),
            lifesteal_pct: self.lifesteal_pct.saturating_add(other.lifesteal_pct),
            ricochets: self.ricochets.saturating_add(other.ricochets),
            trap_sight: self.trap_sight || other.trap_sight,
            featherfall: self.featherfall || other.featherfall,
            regen_pct: self.regen_pct.saturating_add(other.regen_pct),
            loot_pct: self.loot_pct.saturating_add(other.loot_pct),
        }
    }

    /// `percent` as a scale factor (`1.0 + percent / 100`, never below 0.1).
    #[must_use]
    pub fn scale(percent: i16) -> f32 {
        (1.0 + f32::from(percent) / 100.0).max(0.1)
    }
}

const NONE: RelicEffects = RelicEffects {
    extra_jumps: 0,
    move_pct: 0,
    jump_pct: 0,
    glide_pct: 0,
    health_pct: 0,
    damage_pct: 0,
    taken_pct: 0,
    lifesteal_pct: 0,
    ricochets: 0,
    trap_sight: false,
    featherfall: false,
    regen_pct: 0,
    loot_pct: 0,
};

/// One relic's data.
#[derive(Clone, Copy, Debug)]
pub struct RelicDef {
    pub relic: Relic,
    pub name: &'static str,
    pub blurb: &'static str,
    pub kind: RelicKind,
    pub effects: RelicEffects,
}

/// Every relic's data, in [`Relic`] order.
pub const RELICS: [RelicDef; 15] = [
    RelicDef {
        relic: Relic::SpringHeels,
        name: "Spring Heels",
        blurb: "Jump again in mid-air.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            extra_jumps: 1,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::RicochetRounds,
        name: "Ricochet Rounds",
        blurb: "Your shots bounce off rock once.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            ricochets: 1,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::TrapSightLantern,
        name: "Trap-Sight Lantern",
        blurb: "Hidden traps glow; your lamp reaches further.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            trap_sight: true,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::VampiricPick,
        name: "Vampiric Pick",
        blurb: "Hits on creatures heal you a little.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            lifesteal_pct: 20,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::GliderBoost,
        name: "Glider Boost",
        blurb: "Glide slower and further (needs a glider).",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            glide_pct: 35,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::IronLungs,
        name: "Iron Lungs",
        blurb: "Thirty percent more health.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            health_pct: 30,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::Quickstep,
        name: "Quickstep",
        blurb: "Move twenty percent faster.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            move_pct: 20,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::Featherfall,
        name: "Featherfall",
        blurb: "Falls never hurt.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            featherfall: true,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::Gravedigger,
        name: "Gravedigger",
        blurb: "Caches pay forty percent more.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            loot_pct: 40,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::BoneCharm,
        name: "Bone Charm",
        blurb: "Take a quarter less damage.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            taken_pct: -25,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::HuntersMark,
        name: "Hunter's Mark",
        blurb: "Hit thirty percent harder.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            damage_pct: 30,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::SecondWind,
        name: "Second Wind",
        blurb: "Heal three times as fast.",
        kind: RelicKind::Perk,
        effects: RelicEffects {
            regen_pct: 200,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::PactOfGreed,
        name: "Pact of Greed",
        blurb: "Caches pay double, but everything hurts half again as much.",
        kind: RelicKind::Bargain,
        effects: RelicEffects {
            loot_pct: 100,
            taken_pct: 50,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::GlassCannon,
        name: "Glass Cannon",
        blurb: "Hit twice as hard with a third less health.",
        kind: RelicKind::Bargain,
        effects: RelicEffects {
            damage_pct: 100,
            health_pct: -35,
            ..NONE
        },
    },
    RelicDef {
        relic: Relic::RestlessSoul,
        name: "Restless Soul",
        blurb: "Faster, with two extra jumps, but you never heal on your own.",
        kind: RelicKind::Bargain,
        effects: RelicEffects {
            move_pct: 25,
            extra_jumps: 2,
            regen_pct: -100,
            ..NONE
        },
    },
];

/// A pair of relics that does more together.
#[derive(Clone, Copy, Debug)]
pub struct Synergy {
    pub pair: (Relic, Relic),
    pub name: &'static str,
    pub bonus: RelicEffects,
}

/// Relic pairs with a bonus on top.
pub const SYNERGIES: [Synergy; 3] = [
    Synergy {
        pair: (Relic::SpringHeels, Relic::GliderBoost),
        name: "Skywalker",
        bonus: RelicEffects {
            extra_jumps: 1,
            glide_pct: 20,
            ..NONE
        },
    },
    Synergy {
        pair: (Relic::RicochetRounds, Relic::HuntersMark),
        name: "Trick Shot",
        bonus: RelicEffects {
            ricochets: 1,
            damage_pct: 15,
            ..NONE
        },
    },
    Synergy {
        pair: (Relic::VampiricPick, Relic::BoneCharm),
        name: "Bloodbound",
        bonus: RelicEffects {
            lifesteal_pct: 15,
            taken_pct: -10,
            ..NONE
        },
    },
];

impl Relic {
    /// Every relic, in codex order.
    pub const ALL: [Self; 15] = [
        Self::SpringHeels,
        Self::RicochetRounds,
        Self::TrapSightLantern,
        Self::VampiricPick,
        Self::GliderBoost,
        Self::IronLungs,
        Self::Quickstep,
        Self::Featherfall,
        Self::Gravedigger,
        Self::BoneCharm,
        Self::HuntersMark,
        Self::SecondWind,
        Self::PactOfGreed,
        Self::GlassCannon,
        Self::RestlessSoul,
    ];

    /// The relic's codex index.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|relic| *relic == self)
            .unwrap_or(0)
    }

    /// The relic's data.
    #[must_use]
    pub fn def(self) -> &'static RelicDef {
        &RELICS[self.index()]
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        self.def().name
    }

    #[must_use]
    pub fn blurb(self) -> &'static str {
        self.def().blurb
    }

    #[must_use]
    pub fn kind(self) -> RelicKind {
        self.def().kind
    }
}

/// The combined effects of `relics`, synergies included.
#[must_use]
pub fn effects_of(relics: &[Relic]) -> RelicEffects {
    let mut effects = relics
        .iter()
        .fold(NONE, |sum, relic| sum.plus(relic.def().effects));
    for synergy in &SYNERGIES {
        if relics.contains(&synergy.pair.0) && relics.contains(&synergy.pair.1) {
            effects = effects.plus(synergy.bonus);
        }
    }
    effects
}

/// The synergies `relics` complete.
#[must_use]
pub fn synergies_of(relics: &[Relic]) -> Vec<&'static Synergy> {
    SYNERGIES
        .iter()
        .filter(|synergy| relics.contains(&synergy.pair.0) && relics.contains(&synergy.pair.1))
        .collect()
}

/// Perks offered between floors (plus one bargain).
pub const PERK_OFFERS: usize = 3;

/// The relics offered to member `member` (its index in the party) after floor `floor` of a
/// run with seed `seed`, leaving out relics already `held`: [`PERK_OFFERS`] perks, then one
/// bargain when any is left. Pure.
#[must_use]
pub fn offers(seed: u64, floor: u8, member: u8, held: &[Relic]) -> Vec<Relic> {
    let mut rng = RegionRng::new(seed, SALT_OFFERS, i32::from(floor), i32::from(member));
    let mut perks: Vec<Relic> = Relic::ALL
        .iter()
        .copied()
        .filter(|relic| relic.kind() == RelicKind::Perk && !held.contains(relic))
        .collect();
    let mut chosen = Vec::new();
    while chosen.len() < PERK_OFFERS && !perks.is_empty() {
        let pick = usize::try_from(rng.next_u64() % perks.len() as u64).unwrap_or(0);
        chosen.push(perks.remove(pick));
    }
    let bargains: Vec<Relic> = Relic::ALL
        .iter()
        .copied()
        .filter(|relic| relic.kind() == RelicKind::Bargain && !held.contains(relic))
        .collect();
    if !bargains.is_empty() {
        let pick = usize::try_from(rng.next_u64() % bargains.len() as u64).unwrap_or(0);
        chosen.push(bargains[pick]);
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_the_enum() {
        for (index, relic) in Relic::ALL.iter().enumerate() {
            assert_eq!(RELICS[index].relic, *relic);
            assert_eq!(relic.index(), index);
        }
        assert!(
            Relic::ALL
                .iter()
                .filter(|relic| relic.kind() == RelicKind::Perk)
                .count()
                >= 12
        );
    }

    #[test]
    fn synergies_add_on_top() {
        let alone = effects_of(&[Relic::SpringHeels]);
        assert_eq!(alone.extra_jumps, 1);
        let both = effects_of(&[Relic::SpringHeels, Relic::GliderBoost]);
        assert_eq!(both.extra_jumps, 2);
        assert_eq!(both.glide_pct, 55);
        assert_eq!(
            synergies_of(&[Relic::SpringHeels, Relic::GliderBoost]).len(),
            1
        );
    }

    #[test]
    fn offers_are_pure_distinct_and_skip_held_relics() {
        let held = [Relic::Quickstep];
        let first = offers(42, 1, 0, &held);
        assert_eq!(first, offers(42, 1, 0, &held));
        assert_eq!(first.len(), PERK_OFFERS + 1);
        assert!(!first.contains(&Relic::Quickstep));
        assert_eq!(
            first.last().map(|relic| relic.kind()),
            Some(RelicKind::Bargain)
        );
        let mut sorted = first.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), first.len());
        // Members get their own picks.
        assert!((0..4).any(|member| offers(42, 1, member, &held) != first));
    }
}
