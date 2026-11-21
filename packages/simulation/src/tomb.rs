//! Tombs and crypts as roguelite runs (expansion plan WP-TOMBS, Wave 5 T27: the shared core).
//!
//! **Two kinds of place, never mixed.** Every tomb site is either a side-view tomb
//! ([`SiteStyle::SideView`], a platformer run on the pilot's own movement) or a top-down crypt
//! ([`SiteStyle::TopDown`], a dungeon crawl). The style is fixed by the world seed for each site
//! ([`TombSite::style`]), drawn on its entrance and its map marker, and is the same for every
//! floor of that place.
//!
//! The pieces, and who owns what:
//!
//! * **Sites** ([`TombSite`], placed by [`crate::worldgen::tomb_sites`] on
//!   [`GeneratorVersion::V10`](crate::worldgen::GeneratorVersion::V10) worlds): sealed entrances
//!   deep underground, in badland and desert mesas, under villages' old quarters, on sky
//!   continents and on the Moon. Depth or altitude sets the [`TombSite::tier`]; the
//!   [`SiteOpening`] says how the seal breaks (a found tomb key, a story step, or explosives).
//! * **Floors** ([`floor`]): a style-agnostic room graph ([`floor::FloorPlan`]: entry, trap
//!   halls, puzzles, ambushes, shrine checkpoints, the treasure vault and the boss room on the
//!   last floor) generated from the site seed, the site's run counter and the floor number, so
//!   every run is fresh but replays identically online.
//! * **Realizers** ([`realize`]): turn a plan into tiles for one style
//!   ([`realize::FloorRealizer`]; [`realize::realizer_for`]). The side-view realizer builds
//!   platformer rooms the pilot's existing movement plays; the top-down realizer builds a room
//!   grid walked in the pilot's top-down mode ([`crate::pilot::PilotView::TopDown`], a
//!   placeholder T29 replaces with the real movement, collision and camera).
//! * **Side-view content** (T28): [`side_view`] (the realizer: room templates, platforming,
//!   secrets), [`culture`] (tile sets per site: sandstone, catacomb, sky temple, lunar vault)
//!   and [`traps`] (blades, crushers, crumbling floors, dart walls, rising sand, cursed fog:
//!   pure functions of the world clock).
//! * **Relics** ([`relic`]): run-only perks picked between floors, with synergies, and
//!   blessing/curse bargains; their effects hook into the pilot's body
//!   ([`crate::pilot::PilotBoons`]), weapons and damage.
//! * **Runs and unlocks** ([`run`]): the world field [`run::Tombs`] (replicated as
//!   `SectionKey::Tombs`, saved, hashed once used): every live run (party, floor, relics,
//!   banked and unbanked loot, downed members), each site's run counter and seal, and each
//!   player's permanent unlocks (relic codex, tomb maps, trophies, lore pages).
//!
//! The world side (entering, the run's tick, banking, death and revives, relic picks, leaving)
//! lives in `world/tombs.rs`.

pub mod culture;
pub mod floor;
pub mod realize;
pub mod relic;
pub mod run;
pub mod side_view;
pub mod traps;

use serde::{Deserialize, Serialize};

use crate::terrain::TilePosition;

pub use floor::{FloorPlan, RoomKind, RoomNode};
pub use realize::{FloorRealizer, RealizedFloor, realizer_for};
pub use relic::{Relic, RelicEffects, RelicKind};
pub use run::{RunMember, RunOutcome, RunPhase, RunSummary, TombRun, TombUnlocks, Tombs};

/// A tomb run action ([`crate::PlayerCommand::Tomb`]), validated inside the tick.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TombCommand {
    /// Take relic offer `index` between floors (`None`: take nothing).
    PickRelic(Option<u8>),
}

/// How a site plays: one or the other for every floor of the place (user decision).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum SiteStyle {
    /// A side-view tomb: platformer rooms on the pilot's own movement.
    #[default]
    SideView,
    /// A top-down crypt: a room grid walked in the pilot's top-down mode.
    TopDown,
}

impl SiteStyle {
    pub const ALL: [Self; 2] = [Self::SideView, Self::TopDown];

    /// A short name for prompts and the map.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SideView => "side-view tomb",
            Self::TopDown => "top-down crypt",
        }
    }

    /// The place's noun ("Tomb" or "Crypt").
    #[must_use]
    pub const fn noun(self) -> &'static str {
        match self {
            Self::SideView => "Tomb",
            Self::TopDown => "Crypt",
        }
    }

    /// The style's stable index (tile codes, hashing).
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::SideView => 0,
            Self::TopDown => 1,
        }
    }

    /// The style with [`Self::index`] `index` (anything but 1 is side view).
    #[must_use]
    pub const fn from_index(index: u8) -> Self {
        if index == 1 {
            Self::TopDown
        } else {
            Self::SideView
        }
    }
}

/// Where a site is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SiteSetting {
    /// Sealed in the deep strata; the tier follows the depth.
    Deep,
    /// Cut into a badland or desert mesa on the surface (Act I).
    Mesa,
    /// Under a village's old quarter.
    OldQuarter,
    /// On a sky continent.
    SkyContinent,
    /// On the Moon.
    Moon,
}

impl SiteSetting {
    pub const ALL: [Self; 5] = [
        Self::Deep,
        Self::Mesa,
        Self::OldQuarter,
        Self::SkyContinent,
        Self::Moon,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Deep => "Deep Strata",
            Self::Mesa => "Mesa",
            Self::OldQuarter => "Old Quarter",
            Self::SkyContinent => "Sky Continent",
            Self::Moon => "Moon",
        }
    }

    /// Whether sites of this setting stand on open ground (a mesa, a sky continent, the Moon)
    /// rather than in a pocket of rock.
    #[must_use]
    pub const fn on_the_surface(self) -> bool {
        matches!(self, Self::Mesa | Self::SkyContinent | Self::Moon)
    }

    /// The setting's stable index (site ids).
    #[must_use]
    pub const fn index(self) -> u64 {
        match self {
            Self::Deep => 0,
            Self::Mesa => 1,
            Self::OldQuarter => 2,
            Self::SkyContinent => 3,
            Self::Moon => 4,
        }
    }
}

/// How a site's seal breaks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SiteOpening {
    /// A tomb key ([`TOMB_KEY`], found in ruin chests and tomb vaults), used at the door.
    Key,
    /// A story step: the seal answers someone who has seen a story through
    /// (`WorldState::unseal_tomb_site` is the arc binding's entry point).
    Story,
    /// A blast against the door cracks it.
    Explosives,
}

impl SiteOpening {
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Key => "A keyhole shaped like the old builders' sigil.",
            Self::Story => {
                "The seal is carved with a tale; it answers those who have seen one through."
            }
            Self::Explosives => "Hairline cracks run through the seal. A blast might open it.",
        }
    }
}

/// A site's stable id: its setting and placement cell, packed (see
/// [`crate::worldgen::tomb_sites`]).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct SiteId(pub u64);

impl SiteId {
    /// The id of the site of `setting` in placement cell `(cx, cy)`.
    #[must_use]
    pub const fn new(setting: SiteSetting, cx: i32, cy: i32) -> Self {
        #[allow(clippy::cast_sign_loss, reason = "cells are packed as raw bits")]
        let (x, y) = (cx as u32 as u64, (cy as u32 as u64) & 0x00FF_FFFF);
        Self((setting.index() << 56) | (x << 24) | y)
    }
}

/// One placed tomb site (pure: settings and its cell).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TombSite {
    pub id: SiteId,
    pub setting: SiteSetting,
    pub style: SiteStyle,
    /// Difficulty tier 1..=[`MAX_TIER`] (depth or altitude).
    pub tier: u8,
    pub opening: SiteOpening,
    /// The top-left tile of the entrance's layout box.
    pub origin: TilePosition,
    /// The door tile (the one the pilot uses).
    pub door: TilePosition,
    /// The entrance is mirrored (opens to the right).
    pub mirrored: bool,
    /// The site's seed (floors roll from it and the run counter).
    pub seed: u64,
}

impl TombSite {
    /// Floors a run of this site goes down.
    #[must_use]
    pub const fn floors(&self) -> u8 {
        floors_for_tier(self.tier)
    }

    /// The place's name for prompts ("Sealed Tomb of the Mesa").
    #[must_use]
    pub fn title(&self) -> String {
        format!(
            "{} of the {} (tier {})",
            self.style.noun(),
            self.setting.name(),
            self.tier
        )
    }
}

/// The ambient light level of a run space (torchlight; see `WorldState::light_at`).
pub const RUN_SPACE_LIGHT: u16 = 520;

/// The highest site tier.
pub const MAX_TIER: u8 = 5;

/// Floors of a tier: 3 to 6.
#[must_use]
pub const fn floors_for_tier(tier: u8) -> u8 {
    match tier {
        0..=2 => 3,
        3 => 4,
        4 => 5,
        _ => 6,
    }
}

/// The tomb key: a quest item that rides the key ring (always picked up) and opens one sealed
/// [`SiteOpening::Key`] door (it is used up).
pub const TOMB_KEY: crate::item::ItemKind =
    crate::item::ItemKind::Quest(crate::item::QuestItemId(crate::item::TOMB_KEY_ID));

/// The relic codex entry for an item, if it is a tomb item (keys).
#[must_use]
pub fn is_tomb_key(item: crate::item::ItemKind) -> bool {
    item == TOMB_KEY
}

/// Lore pages found in tombs: short texts tying the tombs to the builders of the Moon and the
/// deep core (the main mystery; T26 arcs can point at them). Indexed by [`run::TombUnlocks`].
pub const LORE_PAGES: [(&str, &str); 8] = [
    (
        "The First Sealing",
        "We sealed the stair below the mesa so no rig could find it. The core hums louder each year.",
    ),
    (
        "Of Two Doors",
        "Some we built to be walked, some to be crawled from above. Both keep the same secret.",
    ),
    (
        "The Builders' Lift",
        "A cable ran from the deep core to the hanging stone. We called the stone the Moon.",
    ),
    (
        "Wardens",
        "Each tomb has a keeper. Wake it only if you mean to carry its weight out into the light.",
    ),
    (
        "Shrines",
        "Rest at the lamps. What you set down at a shrine, the tomb cannot take back.",
    ),
    (
        "A Miner's Note",
        "Dad said the old stone sings when the drill bites it. He never came back up from the third stair.",
    ),
    (
        "The Sky Vault",
        "We buried our dead high, nearer the stone above, so they could watch the cable sway.",
    ),
    (
        "The Last Page",
        "When the core wakes, the Moon will answer. Every tomb is a word in that answer.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floors_run_three_to_six_by_tier() {
        assert_eq!(floors_for_tier(1), 3);
        assert_eq!(floors_for_tier(2), 3);
        assert_eq!(floors_for_tier(3), 4);
        assert_eq!(floors_for_tier(4), 5);
        assert_eq!(floors_for_tier(5), 6);
    }

    #[test]
    fn site_ids_pack_setting_and_cell() {
        let a = SiteId::new(SiteSetting::Deep, -3, 7);
        let b = SiteId::new(SiteSetting::Mesa, -3, 7);
        let c = SiteId::new(SiteSetting::Deep, 3, 7);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(a, SiteId::new(SiteSetting::Deep, -3, 7));
        assert_eq!(
            SiteStyle::from_index(SiteStyle::TopDown.index()),
            SiteStyle::TopDown
        );
    }
}
