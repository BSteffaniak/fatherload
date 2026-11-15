//! Rig cosmetics: a player's paint job, thruster flame, drill style, hull pattern, decal and
//! attachments, plus which cosmetic items they have unlocked.
//!
//! Cosmetics are visual only. They never change gameplay; the rig's upgrade tiers decide which
//! parts are drawn (drill head, thrusters, cargo pods, plating, radiator), and the cosmetics
//! decide how those parts look.
//!
//! A player's chosen [`RigCosmetics`] live on [`crate::Player::cosmetics`] (public: replicated
//! to everyone in the rig build section, and saved). `None` means the player never visited the
//! garage and wears their per-player default look ([`RigCosmetics::default_for`]), so co-op rigs
//! stay distinct out of the box. [`crate::Player::cosmetic_unlocks`] holds the items earned on
//! top of the generous starter set (private, saved).
//!
//! Rendering maps every enum here to palette colours and pixel art; the simulation only knows
//! the names.
//!
//! # Hooks for unlocks (Wave 3)
//!
//! Every [`CosmeticItem`] is either a starter item ([`CosmeticItem::is_starter`]) or must be
//! unlocked. Grant one inside the fixed tick with [`crate::Player::unlock_cosmetic`] (quest
//! rewards, milestones, shops). The garage shows locked items with a lock tag, and the
//! simulation refuses a paint job that uses an item the player has not unlocked (test-mode
//! worlds unlock everything).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ids::PlayerId;

/// Declares a cosmetic option enum with its `ALL` list, display names and starter flags.
macro_rules! cosmetic_enum {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($(#[$vmeta:meta])* $variant:ident => ($label:literal, $starter:literal)),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd,
            Serialize,
        )]
        pub enum $name {
            #[default]
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every option, in garage order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Display name.
            #[must_use]
            pub const fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            /// Whether every player owns this option from the start.
            #[must_use]
            pub const fn is_starter(self) -> bool {
                match self {
                    $(Self::$variant => $starter),+
                }
            }

            /// The option `step` places after this one in [`Self::ALL`] (wrapping; negative
            /// steps go back).
            #[must_use]
            pub fn cycled(self, step: i32) -> Self {
                let all = Self::ALL;
                let len = i32::try_from(all.len()).unwrap_or(i32::MAX);
                let index = all.iter().position(|item| *item == self).unwrap_or_default();
                let index = i32::try_from(index).unwrap_or_default();
                let next = usize::try_from((index + step).rem_euclid(len)).unwrap_or_default();
                all[next]
            }
        }
    };
}

cosmetic_enum! {
    /// A paint colour (a four-step ramp in the renderer's palette).
    PaintColor {
        Gold => ("Prospector Gold", true),
        Tangerine => ("Tangerine", true),
        Crimson => ("Crimson", true),
        Rose => ("Rose", true),
        Blossom => ("Blossom", true),
        Violet => ("Violet", true),
        Sky => ("Sky", true),
        Cobalt => ("Cobalt", true),
        Teal => ("Teal", true),
        Mint => ("Mint", true),
        Lime => ("Lime", true),
        Forest => ("Forest", true),
        Copper => ("Copper", true),
        Sand => ("Sand", true),
        Pearl => ("Pearl", true),
        Steel => ("Steel", true),
        Graphite => ("Graphite", true),
        /// Near-black stealth paint (an unlock).
        Stealth => ("Stealth", false),
    }
}

cosmetic_enum! {
    /// Thruster flame colour.
    FlameColor {
        Classic => ("Classic", true),
        Gold => ("Gold", true),
        Blue => ("Blue", true),
        Teal => ("Teal", true),
        Green => ("Green", true),
        Violet => ("Violet", true),
        Pink => ("Pink", true),
        White => ("White Hot", true),
        /// Cycles through the spectrum (an unlock).
        Prism => ("Prism", false),
    }
}

cosmetic_enum! {
    /// The drill head's shape (its material and size follow the drill tier).
    DrillStyle {
        Standard => ("Standard", true),
        Serrated => ("Serrated", true),
        Auger => ("Auger", true),
        Twin => ("Twin Bit", true),
        Crusher => ("Crusher", true),
        /// A long needle spire (an unlock).
        Spire => ("Spire", false),
    }
}

cosmetic_enum! {
    /// A pattern painted over the whole hull.
    HullPattern {
        Solid => ("Solid", true),
        TwoTone => ("Two-Tone", true),
        Racing => ("Racing Stripe", true),
        Checker => ("Checker Band", true),
        Camo => ("Camo", true),
        Hazard => ("Hazard Chevrons", true),
        /// Glowing circuit traces (an unlock).
        Circuit => ("Circuit", false),
    }
}

cosmetic_enum! {
    /// A decal on the hull's flank.
    Decal {
        None => ("None", true),
        Star => ("Star", true),
        Bolt => ("Lightning Bolt", true),
        Flames => ("Hot-Rod Flames", true),
        Picks => ("Crossed Picks", true),
        Number => ("Crew Number", true),
        /// A golden crown (an unlock).
        Crown => ("Crown", false),
    }
}

cosmetic_enum! {
    /// What sits on the roof.
    RoofAttachment {
        None => ("None", true),
        Antenna => ("Whip Antenna", true),
        Dish => ("Radar Dish", true),
        Siren => ("Siren Lamp", true),
        LightBar => ("Light Bar", true),
        /// A floating ring (an unlock).
        Halo => ("Halo", false),
    }
}

cosmetic_enum! {
    /// What sits on the nose, above the drill.
    FrontAttachment {
        None => ("None", true),
        Floodlamp => ("Floodlamp", true),
        BullBar => ("Bull Bar", true),
    }
}

cosmetic_enum! {
    /// What sits on the tail.
    RearAttachment {
        None => ("None", true),
        Spoiler => ("Spoiler", true),
        Pennant => ("Pennant", true),
        Stacks => ("Chrome Stacks", true),
    }
}

/// A rig's full cosmetic loadout. Missing fields load as their defaults, so saves keep
/// loading when later waves add slots.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(default)]
pub struct RigCosmetics {
    /// Main hull paint (also the player's colour on name tags, the HUD and the minimap).
    pub primary: PaintColor,
    /// Lower hull, cargo pods and thruster pods.
    pub secondary: PaintColor,
    /// Stripes, rims and the drill collar.
    pub trim: PaintColor,
    pub flame: FlameColor,
    pub drill_style: DrillStyle,
    pub pattern: HullPattern,
    pub decal: Decal,
    pub roof: RoofAttachment,
    pub front: FrontAttachment,
    pub rear: RearAttachment,
}

/// The per-player default paint jobs `(primary, secondary, trim)`, by player id (player 1 is the
/// classic yellow rig).
pub const DEFAULT_PAINTS: [(PaintColor, PaintColor, PaintColor); 6] = [
    (PaintColor::Gold, PaintColor::Steel, PaintColor::Tangerine),
    (PaintColor::Teal, PaintColor::Steel, PaintColor::Mint),
    (PaintColor::Rose, PaintColor::Graphite, PaintColor::Blossom),
    (PaintColor::Lime, PaintColor::Steel, PaintColor::Forest),
    (
        PaintColor::Violet,
        PaintColor::Graphite,
        PaintColor::Blossom,
    ),
    (PaintColor::Sky, PaintColor::Steel, PaintColor::Cobalt),
];

impl RigCosmetics {
    /// The look of a player who never visited the garage: their per-player default paint
    /// (cycling every six players), a classic flame and stock parts.
    #[must_use]
    pub fn default_for(player_id: PlayerId) -> Self {
        let index = usize::try_from(player_id.get().saturating_sub(1) % 6).unwrap_or_default();
        let (primary, secondary, trim) = DEFAULT_PAINTS[index];
        Self {
            primary,
            secondary,
            trim,
            pattern: HullPattern::Racing,
            ..Self::default()
        }
    }

    /// `custom` if the player chose a look, otherwise their default.
    #[must_use]
    pub fn resolve(custom: Option<Self>, player_id: PlayerId) -> Self {
        custom.unwrap_or_else(|| Self::default_for(player_id))
    }

    /// Every item this loadout uses.
    #[must_use]
    pub const fn items(&self) -> [CosmeticItem; 10] {
        [
            CosmeticItem::Paint(self.primary),
            CosmeticItem::Paint(self.secondary),
            CosmeticItem::Paint(self.trim),
            CosmeticItem::Flame(self.flame),
            CosmeticItem::Drill(self.drill_style),
            CosmeticItem::Pattern(self.pattern),
            CosmeticItem::Decal(self.decal),
            CosmeticItem::Roof(self.roof),
            CosmeticItem::Front(self.front),
            CosmeticItem::Rear(self.rear),
        ]
    }
}

/// One unlockable cosmetic option.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CosmeticItem {
    Paint(PaintColor),
    Flame(FlameColor),
    Drill(DrillStyle),
    Pattern(HullPattern),
    Decal(Decal),
    Roof(RoofAttachment),
    Front(FrontAttachment),
    Rear(RearAttachment),
}

impl CosmeticItem {
    /// Whether every player owns this item from the start.
    #[must_use]
    pub const fn is_starter(self) -> bool {
        match self {
            Self::Paint(item) => item.is_starter(),
            Self::Flame(item) => item.is_starter(),
            Self::Drill(item) => item.is_starter(),
            Self::Pattern(item) => item.is_starter(),
            Self::Decal(item) => item.is_starter(),
            Self::Roof(item) => item.is_starter(),
            Self::Front(item) => item.is_starter(),
            Self::Rear(item) => item.is_starter(),
        }
    }

    /// Display name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Paint(item) => item.label(),
            Self::Flame(item) => item.label(),
            Self::Drill(item) => item.label(),
            Self::Pattern(item) => item.label(),
            Self::Decal(item) => item.label(),
            Self::Roof(item) => item.label(),
            Self::Front(item) => item.label(),
            Self::Rear(item) => item.label(),
        }
    }

    /// Every cosmetic item (starter and unlockable).
    #[must_use]
    pub fn all() -> Vec<Self> {
        PaintColor::ALL
            .iter()
            .map(|item| Self::Paint(*item))
            .chain(FlameColor::ALL.iter().map(|item| Self::Flame(*item)))
            .chain(DrillStyle::ALL.iter().map(|item| Self::Drill(*item)))
            .chain(HullPattern::ALL.iter().map(|item| Self::Pattern(*item)))
            .chain(Decal::ALL.iter().map(|item| Self::Decal(*item)))
            .chain(RoofAttachment::ALL.iter().map(|item| Self::Roof(*item)))
            .chain(FrontAttachment::ALL.iter().map(|item| Self::Front(*item)))
            .chain(RearAttachment::ALL.iter().map(|item| Self::Rear(*item)))
            .collect()
    }
}

/// The cosmetic items a player earned on top of the starter set.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct CosmeticUnlocks {
    pub items: BTreeSet<CosmeticItem>,
}

impl CosmeticUnlocks {
    /// Whether the player may use `item` (a starter item or one they unlocked).
    #[must_use]
    pub fn has(&self, item: CosmeticItem) -> bool {
        item.is_starter() || self.items.contains(&item)
    }

    /// Whether every item of `cosmetics` is available.
    #[must_use]
    pub fn allows(&self, cosmetics: &RigCosmetics) -> bool {
        cosmetics.items().into_iter().all(|item| self.has(item))
    }

    /// Unlocks `item`. Returns `true` if it was newly unlocked (starter items never are).
    pub fn unlock(&mut self, item: CosmeticItem) -> bool {
        !item.is_starter() && self.items.insert(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_looks_are_distinct_for_the_first_six_players() {
        let primaries: BTreeSet<PaintColor> = (1..=6)
            .map(|id| RigCosmetics::default_for(PlayerId::new(id)).primary)
            .collect();
        assert_eq!(primaries.len(), 6);
        assert_eq!(
            RigCosmetics::default_for(PlayerId::new(7)),
            RigCosmetics::default_for(PlayerId::new(1)),
            "defaults cycle"
        );
        assert_eq!(
            RigCosmetics::default_for(PlayerId::new(1)).primary,
            PaintColor::Gold
        );
    }

    #[test]
    fn defaults_use_only_starter_items() {
        let unlocks = CosmeticUnlocks::default();
        for id in 1..=6 {
            assert!(unlocks.allows(&RigCosmetics::default_for(PlayerId::new(id))));
        }
    }

    #[test]
    fn the_starter_set_is_generous_but_leaves_unlocks() {
        let all = CosmeticItem::all();
        let starter = all.iter().filter(|item| item.is_starter()).count();
        assert!(starter * 10 >= all.len() * 8, "most items are starters");
        assert!(starter < all.len(), "some items are left to unlock");
    }

    #[test]
    fn unlocking_makes_an_item_available_once() {
        let mut unlocks = CosmeticUnlocks::default();
        let stealth = CosmeticItem::Paint(PaintColor::Stealth);
        assert!(!unlocks.has(stealth));
        assert!(unlocks.unlock(stealth));
        assert!(!unlocks.unlock(stealth));
        assert!(unlocks.has(stealth));
        assert!(!unlocks.unlock(CosmeticItem::Paint(PaintColor::Gold)));
        assert!(unlocks.items.len() == 1);
    }

    #[test]
    fn options_cycle_both_ways() {
        assert_eq!(PaintColor::Gold.cycled(1), PaintColor::Tangerine);
        assert_eq!(PaintColor::Gold.cycled(-1), PaintColor::Stealth);
        assert_eq!(PaintColor::Stealth.cycled(1), PaintColor::Gold);
        assert_eq!(RearAttachment::None.cycled(-1), RearAttachment::Stacks);
    }

    #[test]
    fn cosmetics_round_trip_through_serde() {
        let look = RigCosmetics {
            primary: PaintColor::Cobalt,
            decal: Decal::Bolt,
            roof: RoofAttachment::Dish,
            ..RigCosmetics::default()
        };
        let json = serde_json::to_string(&look).unwrap();
        assert_eq!(serde_json::from_str::<RigCosmetics>(&json).unwrap(), look);
    }

    #[test]
    fn looks_saved_with_fewer_slots_still_load() {
        let partial = r#"{"primary":"Cobalt","decal":"Bolt"}"#;
        let look: RigCosmetics = serde_json::from_str(partial).unwrap();
        assert_eq!(look.primary, PaintColor::Cobalt);
        assert_eq!(look.decal, Decal::Bolt);
        assert_eq!(look.roof, RoofAttachment::None);
        let unlocks: CosmeticUnlocks = serde_json::from_str("{}").unwrap();
        assert!(unlocks.items.is_empty());
    }
}
