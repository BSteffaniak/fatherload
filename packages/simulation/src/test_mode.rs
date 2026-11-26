//! Test mode ("god mode"): a world setting for exploring the layers quickly.
//!
//! A test-mode world starts every rig with every upgrade maxed (the drill mount included), every
//! rig part in the shared parts locker and a large credit balance. It is tuned to feel like the
//! game is beaten: rigs drive and climb well above the top engine tier (see
//! [`crate::world::DriveTuning`]) and drill anything but bedrock instantly. While it is on,
//! any player may toggle an invulnerable hull, infinite fuel and no-clip flight for the whole
//! crew, and warp their own rig to the top of any band. Depth records and deep-reward milestones
//! do not count in a test-mode world.
//!
//! The setting is chosen when the world is created (the host's choice online) and is part of the
//! world: it replicates as its own shared section and is saved with the world.
//!
//! # The god-mode toolkit (Wave 14, W14-A)
//!
//! A test-mode world is also **fully stocked** ([`stock`]): every consumable at its stack,
//! every rig part, pilot gear and cosmetic, every boss-won gate reward, the Star Drive and every
//! space line, alien tech and star chart. The crew's **infinite supplies** switch (on by
//! default) keeps the consumables topped up; **progress switches** set each Descent gate and
//! the Heart's ending, the home clock and the bodies' weather; and the **enemy tools** spawn
//! any creature next to a rig, clear the ones nearby and freeze their AI. Every one is a
//! [`TestModeAction`] (a replicated player action the host's world decides), refused outside a
//! test-mode world.

pub mod stock;

use serde::{Deserialize, Serialize};

use crate::{bodylife::weather::WeatherId, enemy::EnemyKind, worldgen::Band};

pub use stock::{STOCK_REVISION, StockItem, TEST_STACK, TestStockEvent};

pub mod palette_stock;
pub mod travel;

/// Credits each rig starts a test-mode world with.
pub const TEST_MODE_CREDITS: u32 = 1_000_000;

/// The test-mode toggles of a world (present only in a test-mode world).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches of the pause menu, each saved on its own"
)]
pub struct TestModeState {
    /// Hulls never lose integrity (refilled every tick).
    pub invulnerable: bool,
    /// Fuel tanks never run dry (refilled every tick).
    pub infinite_fuel: bool,
    /// Rigs fly freely through solid rock: no gravity, no collision, no drilling (thrust flies
    /// up, drill flies down). Turning it off frees any rig left inside rock.
    #[serde(default)]
    pub no_clip: bool,
    /// Consumables never run out: every rig's stocked items are topped back up to their stack
    /// at the end of every tick (Wave 14; on by default, and for test-mode saves from before
    /// the toggle existed).
    #[serde(default = "infinite_supplies_by_default")]
    pub infinite_supplies: bool,
    /// The story and the codex are unlocked: every expedition log, echo, diary page, tablet,
    /// inscription and relic entry read (Wave 14; off by default so the story can be tested
    /// fresh, and switching it off resets them).
    #[serde(default)]
    pub story_unlocked: bool,
    /// Creatures stand still: no enemy or enemy shot thinks or moves while it is on (Wave 14).
    #[serde(default)]
    pub ai_frozen: bool,
    /// The weather forced on every body (Wave 14; `None`: each body's own forecast).
    #[serde(default)]
    pub weather: Option<WeatherId>,
    /// The stocking revision the world's rigs were stocked to ([`STOCK_REVISION`]): a world
    /// saved before a revision is restocked on its next tick, so older test-mode saves pick up
    /// everything added since.
    #[serde(default)]
    pub stock_revision: u8,
}

const fn infinite_supplies_by_default() -> bool {
    true
}

impl TestModeState {
    /// A new test-mode world's toggles: invulnerable, infinite fuel and infinite supplies on,
    /// the story untouched, stocked to the current revision.
    #[must_use]
    pub const fn new_world() -> Self {
        Self {
            invulnerable: true,
            infinite_fuel: true,
            no_clip: false,
            infinite_supplies: true,
            story_unlocked: false,
            ai_frozen: false,
            weather: None,
            stock_revision: STOCK_REVISION,
        }
    }
}

/// A test-mode action submitted as [`crate::PlayerCommand::TestMode`]. Ignored outside a
/// test-mode world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TestModeAction {
    /// Turn the crew's invulnerable hulls on or off.
    SetInvulnerable(bool),
    /// Turn the crew's infinite fuel on or off.
    SetInfiniteFuel(bool),
    /// Move the submitting rig into a freshly carved, safe pocket at the top of `Band`.
    WarpToBand(Band),
    /// Turn the crew's no-clip flight on or off.
    SetNoClip(bool),
    /// Place a field zone of the preset around the submitting rig or pilot (F9 dev zones).
    PlaceFieldZone(crate::field::FieldPreset),
    /// Remove every field zone.
    ClearFieldZones,
    /// Carve a row of test pools, one per fluid kind, beside the submitting rig (Part III F9:
    /// water, acid, brine and liquid metal to swim, dive and react in).
    PourTestPools,
    /// Move the submitting rig to another place (F11): over a space body's arrival point,
    /// into free flight, onto the Moon or back to the spawn point at home.
    WarpToPlace(crate::place::Place),
    /// Fit the submitting rig with the Star Drive and open space (Wave 11's debug grant; on
    /// Deep Descent worlds only).
    GrantStarDrive,
    /// Call a `Faction`'s encounter on the submitting rig in space (Wave 12 hostiles).
    SpawnHostiles(crate::hostiles::Faction),
    /// Call a `Faction`'s boss on the submitting rig in space (Wave 12 hostiles).
    SpawnBoss(crate::hostiles::Faction),
    /// Land a `Faction`'s raid by the submitting rig on a body's ground (Wave 12 hostiles).
    SpawnRaid(crate::hostiles::Faction),
    /// Move the submitting rig to where the galaxy story's current goal is done (Wave 13
    /// story): the stop's system, its world's relic, the Harbour's gate.
    WarpToStory,
    // --- The god-mode toolkit (Wave 14, W14-A; `world/test_stock.rs`) -----------------------
    /// Turn the crew's infinite supplies on or off.
    SetInfiniteSupplies(bool),
    /// Refill the submitting rig's consumables to their stacks once (and fit anything missing).
    Restock,
    /// Stock the submitting rig with everything (the console's `give all`).
    StockAll,
    /// Set the submitting rig's count of one stocked item (capped at its stack; the console's
    /// `give dynamite 99`).
    Give(StockItem, u32),
    /// Unlock the story and codex (or switch it off: the story resets to fresh).
    SetStoryUnlocked(bool),
    /// Mark Descent `descent`'s gate (0 = the Crust's Sinkhole .. 7 = the Heart) cleared or
    /// not: its boss beaten (and the Sinkhole's Deep Claim held). Deep Descent worlds only.
    SetGateCleared { descent: u8, cleared: bool },
    /// Mark the Heart's ending done (the Heart woke, the cable lit, space open, every rig with
    /// the Star Drive) or not. Deep Descent worlds only.
    SetEndingDone(bool),
    /// Move the home world's clock forward to `minutes` past midnight (0-1439; a held clock
    /// holds there instead).
    SetTimeOfDay { minutes: u16 },
    /// Hold the home world's time of day where it is (the day/night lock), or let it run.
    SetDayLock(bool),
    /// Force a weather kind on every body (`None`: back to each body's forecast).
    SetWeather(Option<WeatherId>),
    /// Spawn a creature (or boss) of `EnemyKind` at a safe offset beside the submitting rig.
    SpawnCreature(EnemyKind),
    /// Remove every creature and creature shot near the submitting rig.
    ClearNearbyEnemies,
    /// Freeze every creature's AI (or let them move again).
    SetAiFrozen(bool),
    /// Travel to a destination of the index (Wave 14, [`crate::destination`]): "the next
    /// planet", "Descent V", a bookmark, with safe arrival. Run after the tick's last pass.
    TravelTo(crate::destination::TravelRequest),
    /// Travel back to the last place on the submitting player's back-stack (Wave 14).
    TravelBack,
    /// Bookmark where the submitting player stands (Wave 14).
    BookmarkHere,
    /// Bookmark a destination (Wave 14; the Atlas's "Bookmark this").
    Bookmark(crate::destination::DestinationId),
    /// Remove the submitting player's bookmark at this index (Wave 14).
    RemoveBookmark(u8),
    /// Move the submitting rig to a destination (Wave 14 console: the palette's `tp`,
    /// `--start-at` and the bots; TODO-SEAM(w14-index): the index's own travel action).
    PaletteTravel(travel::Destination),
    /// A stocking or progress switch for the submitting rig or the world (Wave 14 console:
    /// the palette's `give`, `gate`, `spawn`, `time`...; TODO-SEAM(w14-stock)).
    PaletteStock(palette_stock::StockRequest),
}

impl TestModeAction {
    /// The toolkit event an applied action raises, with its detail (`None`: the action changes
    /// a toggle, reported as `GameplayEventKind::TestModeChanged`).
    #[must_use]
    pub const fn stock_event(self) -> Option<(TestStockEvent, u32)> {
        Some(match self {
            Self::StockAll => (TestStockEvent::Stocked, 0),
            Self::Restock => (TestStockEvent::Restocked, 0),
            Self::Give(_, count) => (TestStockEvent::Restocked, count),
            Self::SpawnCreature(kind) => (TestStockEvent::CreatureSpawned, kind as u32),
            Self::ClearNearbyEnemies => (TestStockEvent::EnemiesCleared, 0),
            Self::SetGateCleared { descent, cleared } => (
                TestStockEvent::ProgressSet,
                descent as u32 * 2 + cleared as u32,
            ),
            Self::SetEndingDone(done) => (TestStockEvent::ProgressSet, 16 + done as u32),
            Self::SetStoryUnlocked(on) => (TestStockEvent::StorySet, on as u32),
            Self::SetTimeOfDay { minutes } => (TestStockEvent::SkySet, minutes as u32),
            Self::SetWeather(weather) => (
                TestStockEvent::SkySet,
                match weather {
                    Some(kind) => kind.0 as u32,
                    None => u32::MAX,
                },
            ),
            Self::SetInvulnerable(_)
            | Self::SetInfiniteFuel(_)
            | Self::WarpToBand(_)
            | Self::SetNoClip(_)
            | Self::PlaceFieldZone(_)
            | Self::ClearFieldZones
            | Self::PourTestPools
            | Self::WarpToPlace(_)
            | Self::GrantStarDrive
            | Self::SpawnHostiles(_)
            | Self::SpawnBoss(_)
            | Self::SpawnRaid(_)
            | Self::WarpToStory
            | Self::SetInfiniteSupplies(_)
            | Self::SetDayLock(_)
            | Self::SetAiFrozen(_)
            | Self::TravelTo(_)
            | Self::TravelBack
            | Self::BookmarkHere
            | Self::Bookmark(_)
            | Self::RemoveBookmark(_)
            | Self::PaletteTravel(_)
            | Self::PaletteStock(_) => return None,
        })
    }
}
