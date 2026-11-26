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
        /// Milestone reward (reach the Grey Stone): deep green stone polish.
        Jade => ("Jade", false),
        /// Milestone reward (find a cave): a fungal glow.
        Glowcap => ("Glowcap", false),
        /// Milestone reward (max an upgrade line): mirror-bright chrome.
        Chrome => ("Mirror Chrome", false),
        /// Milestone reward (reach the Ancient Machine Layer): a deep abyssal blue.
        Abyss => ("Abyss Blue", false),
        /// Milestone reward (reach the Mantle Storm Zone): molten red and orange.
        Magma => ("Magma", false),
        /// Milestone reward (read the first expedition log): the Fathers' faded field khaki.
        Khaki => ("Expedition Khaki", false),
        /// Milestone reward (read the Crust's logs): Ines Calloway's survey-chalk ochre.
        Ochre => ("Survey Ochre", false),
        /// Milestone reward (read the Old Deep's logs): the builders' dark bronze.
        Bronze => ("Builder Bronze", false),
        /// Milestone reward (read the Living Dark's logs): a glow-sea bioluminescent teal.
        Lumen => ("Lumen", false),
        /// Milestone reward (hear a ghost recording): pale tape-hiss white.
        Phantom => ("Phantom", false),
        /// Milestone reward (enter an anomaly room): a deep anomaly violet.
        Anomaly => ("Anomaly Violet", false),
        /// Milestone reward (enter every kind of anomaly room): a corrupted magenta-green.
        Glitch => ("Glitch", false),
        /// Milestone reward (reach the Old Deep; Wave 10 story).
        Quartz => ("Fault Quartz", false),
        /// Milestone reward (reach the Living Dark; Wave 10 story).
        Spore => ("Spore Green", false),
        /// Milestone reward (reach the Drowned and Frozen; Wave 10 story).
        Brine => ("Brine Blue", false),
        /// Milestone reward (reach the Hollow World; Wave 10 story).
        InnerSun => ("Inner Sun", false),
        /// Milestone reward (reach the Strange Layers; Wave 10 story).
        Prism => ("Prism", false),
        /// Milestone reward (reach the Ancients; Wave 10 story).
        Alloy => ("Builder Alloy", false),
        /// Milestone reward (reach the Heart; Wave 10 story).
        Heartlight => ("Heartlight", false),
        /// Milestone reward (beat the Sinkhole; Wave 10 story).
        Rubble => ("Sinkhole Grey", false),
        /// Milestone reward (beat the Star Core; Wave 10 story).
        Corona => ("Core Corona", false),
        /// Milestone reward (beat the Mycelial Mind; Wave 10 story).
        Mycelium => ("Mycelium", false),
        /// Milestone reward (beat the Leviathan; Wave 10 story).
        DeepScale => ("Deep Scale", false),
        /// Milestone reward (beat the Sun Serpent; Wave 10 story).
        SerpentGold => ("Serpent Gold", false),
        /// Milestone reward (beat the Glitch; Wave 10 story).
        DeadPixel => ("Dead Pixel", false),
        /// Milestone reward (beat the Gatekeeper Colossus; Wave 10 story).
        GateGold => ("Gate Gold", false),
        /// Milestone reward (read the Drowned and Frozen's logs; Wave 10 story).
        Nacre => ("Nacre", false),
        /// Milestone reward (read the Hollow World's logs; Wave 10 story).
        Jungle => ("Jungle", false),
        /// Milestone reward (read the Strange Layers' logs; Wave 10 story).
        Mirrorglass => ("Mirrorglass", false),
        /// Milestone reward (read the Ancients' logs; Wave 10 story).
        Lamplight => ("Lamplight", false),
        /// Milestone reward (read the Heart's logs; Wave 10 story).
        DriveWhite => ("Drive White", false),
        /// Milestone reward (read every log of the expedition; Wave 10 story).
        FathersLoad => ("Fathers' Load", false),
        /// Milestone reward (wake the Heart: the ending; Wave 10 story).
        Starward => ("Starward", false),
        // Wave 13 endgame: the space achievements' rewards.
        /// Milestone reward (mine a space material): pitted meteorite rust.
        Meteorite => ("Meteorite", false),
        /// Milestone reward (land on ten worlds): nebula magenta and blue.
        Nebula => ("Nebula", false),
        /// Milestone reward (land on every kind of world): chart-paper blues.
        Atlas => ("Atlas", false),
        /// Milestone reward (speak a tongue fluently): xeno teal.
        Xeno => ("Xeno Teal", false),
        /// Milestone reward (speak every tongue): many-tongued violet.
        Babel => ("Babel", false),
        /// Milestone reward (beat the Gilded Maw): the Pirate King's gilt.
        GildedMaw => ("Gilded Maw", false),
        /// Milestone reward (beat the Swarm Queen): her chitin green.
        Chitin => ("Queen's Chitin", false),
        /// Milestone reward (beat the Arbiter): Concord white and red.
        Concord => ("Concord", false),
        /// Milestone reward (every boss on the hunt list): hunter's ochre.
        Huntsman => ("Huntsman", false),
        /// Milestone reward (a ship line at tier 5): yard steel blue.
        Shipwright => ("Shipwright", false),
        /// Milestone reward (the Starwright ship): starwright white gold.
        Starwright => ("Starwright", false),
        /// Milestone reward (the Halcyon Gate finished): gatestone grey-gold.
        Gatestone => ("Gatestone", false),
        /// Milestone reward (a home on an alien world): hearth red.
        Hearthstone => ("Hearthstone", false),
        /// Milestone reward (an estate with every wing): manor green.
        Manor => ("Manor Green", false),
        // Wave 13 endgame: the outfitters' stock (space stations only, never village vendors).
        /// Outfitter stock: aurora green and violet.
        Aurora => ("Aurora", false),
        /// Outfitter stock: a sunlit yellow-white.
        Solar => ("Solar", false),
        /// Outfitter stock: corsair red over black.
        Corsair => ("Corsair", false),
        /// Outfitter stock: voidglass black with a violet sheen.
        Voidblack => ("Voidblack", false),
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
        /// Milestone reward (engine tier 5): a smouldering deep-red burn.
        Ember => ("Ember", false),
        /// Milestone reward (engine tier 10): a violet-white plasma jet.
        Plasma => ("Plasma", false),
        /// Milestone reward (an island-capable tank): a pale sky-blue aether flame.
        Aether => ("Aether", false),
        /// Milestone reward (gather comet dust; Wave 13 endgame): a comet's icy blue-green.
        Comet => ("Comet Tail", false),
        /// Milestone reward (the Heartline Drive; Wave 13 endgame): the Heart's red-gold.
        Heartline => ("Heartline", false),
        /// Outfitter stock (Wave 13 endgame): a cold ion blue.
        Ion => ("Ion Blue", false),
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
        /// Milestone reward (drill tier 5): a spiral corkscrew bit.
        Corkscrew => ("Corkscrew", false),
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
        /// Milestone reward (drill tier 10): bold tiger stripes.
        Tiger => ("Tiger Stripes", false),
        /// Milestone reward (every core line maxed): a field of stars.
        Starfield => ("Starfield", false),
        /// Milestone reward (everything done; Wave 13 endgame): stars joined into figures.
        Constellation => ("Constellation", false),
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
        /// Milestone reward (first upgrade): a wrench.
        Wrench => ("Wrench", false),
        /// Milestone reward (first artifact): a cut gemstone.
        Gem => ("Gemstone", false),
        /// Milestone reward (first enemy defeated): a fang.
        Fang => ("Fang", false),
        /// Milestone reward (first claim staked): a claim flag.
        Flag => ("Claim Flag", false),
        /// Milestone reward (fly high above town): a pair of wings.
        Wings => ("Wings", false),
        /// Milestone reward (the Act II unlock): a crescent moon.
        Moon => ("Crescent Moon", false),
        /// Milestone reward (land on a world beyond the Moon; Wave 13 endgame): a ringed planet.
        Planet => ("Ringed Planet", false),
        /// Milestone reward (meet an alien species; Wave 13 endgame): an alien glyph.
        Glyph => ("Alien Glyph", false),
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
        /// Milestone reward (reach the first deep stratum): a depth-record trophy cup.
        Trophy => ("Depth Trophy", false),
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
        /// Milestone reward (visit a village): a swinging trail lantern.
        Lantern => ("Trail Lantern", false),
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

    /// Whether village cosmetic vendors stock this item: every unlockable that is not a
    /// milestone reward ([`crate::milestone::Milestone::reward_for`]). Milestone rewards are
    /// earned, never sold, so the vendor list is exactly the original wave-2 unlock set (and
    /// village offers are unchanged by later cosmetics).
    #[must_use]
    pub fn is_vendor_stock(self) -> bool {
        !self.is_starter()
            && crate::milestone::Milestone::reward_for(self).is_none()
            // Space's outfitters keep their own stock (Wave 13 endgame).
            && !crate::endgame::works::is_outfitter_stock(self)
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
        // At least a third are starters (Wave 10's finale milestones brought the earned paints
        // to about as many as the starter set; Wave 13's space achievements and outfitters
        // added more to earn).
        assert!(
            starter * 3 >= all.len(),
            "a third of the items are starters"
        );
        assert!(starter < all.len(), "some items are left to unlock");
    }

    #[test]
    fn vendors_stock_exactly_the_wave_two_unlocks() {
        let stock: Vec<CosmeticItem> = CosmeticItem::all()
            .into_iter()
            .filter(|item| item.is_vendor_stock())
            .collect();
        assert_eq!(
            stock,
            vec![
                CosmeticItem::Paint(PaintColor::Stealth),
                CosmeticItem::Flame(FlameColor::Prism),
                CosmeticItem::Drill(DrillStyle::Spire),
                CosmeticItem::Pattern(HullPattern::Circuit),
                CosmeticItem::Decal(Decal::Crown),
                CosmeticItem::Roof(RoofAttachment::Halo),
            ],
            "village offers must not change when milestone rewards are added"
        );
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
        assert_eq!(PaintColor::Gold.cycled(-1), PaintColor::Voidblack);
        assert_eq!(PaintColor::Voidblack.cycled(1), PaintColor::Gold);
        assert_eq!(PaintColor::Stealth.cycled(1), PaintColor::Jade);
        assert_eq!(RearAttachment::None.cycled(-1), RearAttachment::Lantern);
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
