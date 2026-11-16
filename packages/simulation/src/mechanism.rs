//! Ruin mechanisms for the pilot on foot (expansion plan WP-PILOT): lever doors, keycard doors,
//! pressure plates and treasure chests that need hands.
//!
//! Everything is a terrain tile ([`crate::base::BuiltTile`], unbreakable like every built
//! piece), so mechanisms stream, save, replicate and diff like any tile, and generators stamp
//! them like any other cell:
//!
//! * **Gates** are a column of [`BuiltTile::Gate`] bars hanging under a
//!   [`BuiltTile::GateLintel`]. The lintel names a *channel* (0..[`CHANNELS`]); an open gate is
//!   plain air under its lintel down to the floor, so closing it fills that air back in.
//! * **Levers** ([`BuiltTile::Lever`]) flip every gate of their channel within
//!   [`REACH_COLUMNS`] x [`REACH_ROWS`] when a pilot uses them ([`USE_REACH`]).
//! * **Pressure plates** ([`BuiltTile::PressurePlate`]) are floor tiles: a pilot's feet or a
//!   rig resting on one presses it and opens its channel's gates; they close again
//!   [`PLATE_HOLD_SECONDS`] after the last weight leaves (the release ticks live in
//!   [`PlateTimers`], part of the bases section). A rig parked on a plate holds a gate open
//!   for its pilot: one of the rig-and-pilot combos.
//! * **Keycard doors** ([`BuiltTile::KeycardDoor`]) open (for good) for a pilot carrying the
//!   keycard of their colour ([`keycard_item`], found in chests).
//! * **Chests** ([`BuiltTile::Chest`]) open only by hand and spill loot rolled from the world
//!   seed and the chest's tile ([`chest_loot`]), and sometimes a keycard.
//!
//! The world side (using, plates, gates, loot) lives in `world/mechanisms.rs`.

use serde::{Deserialize, Serialize};

use crate::{
    base::BuiltTile,
    ids::SimulationTick,
    item::{ItemKind, ItemStack, QuestItemId},
    terrain::{MineralKind, TILE_SIZE, TileKind, TilePosition},
};

/// Gate channels (a lintel's and a mechanism's channel is taken modulo this).
pub const CHANNELS: u8 = 4;
/// Keycard colours.
pub const KEYCARD_COLORS: u8 = 4;
/// A mechanism reaches gates this many columns either side of it.
pub const REACH_COLUMNS: i32 = 12;
/// A mechanism reaches gates this many rows above or below it.
pub const REACH_ROWS: i32 = 8;
/// Tallest gate under a lintel (tiles).
pub const MAX_GATE_HEIGHT: i32 = 5;
/// A pilot uses a lever, chest or keycard door within this distance of its middle (px, to the
/// tile's centre).
pub const USE_REACH: f32 = TILE_SIZE * 1.75;
/// A plate holds its gates open this long after the last weight leaves it.
pub const PLATE_HOLD_SECONDS: f32 = 3.5;
/// First quest item id of the keycards (`0xC100 + colour`; WP-STORY's quest items stay below).
pub const KEYCARD_ITEM_BASE: u16 = 0xC100;

/// A keycard colour's name.
#[must_use]
pub const fn keycard_name(color: u8) -> &'static str {
    match color % KEYCARD_COLORS {
        0 => "red",
        1 => "blue",
        2 => "green",
        _ => "gold",
    }
}

/// The item of the keycard of `color`.
#[must_use]
pub const fn keycard_item(color: u8) -> ItemKind {
    ItemKind::Quest(QuestItemId(
        KEYCARD_ITEM_BASE + (color % KEYCARD_COLORS) as u16,
    ))
}

/// Whether `item` rides the pilot's key ring rather than the pack: keycards and sky temple
/// keystones are always picked up, so a full pack never strands the key to a vault.
#[must_use]
pub fn rides_key_ring(item: ItemKind) -> bool {
    keycard_of(item).is_some() || item == crate::item::TEMPLE_KEYSTONE
}

/// The keycard colour `item` is, if it is one.
#[must_use]
pub const fn keycard_of(item: ItemKind) -> Option<u8> {
    match item {
        ItemKind::Quest(QuestItemId(id))
            if id >= KEYCARD_ITEM_BASE && id < KEYCARD_ITEM_BASE + KEYCARD_COLORS as u16 =>
        {
            #[allow(clippy::cast_possible_truncation, reason = "below KEYCARD_COLORS")]
            Some((id - KEYCARD_ITEM_BASE) as u8)
        }
        _ => None,
    }
}

/// What a pilot can use on a tile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Usable {
    /// A lever of `channel`, currently `on` or off.
    Lever { channel: u8, on: bool },
    /// A closed chest.
    Chest,
    /// A keycard door of `color`.
    KeycardDoor { color: u8 },
    /// A sky temple fitting (T21: a temple lever, the keystone on its plinth, the vault's
    /// keystone gate or a relic chest). The use button works it like a ruin mechanism, and a
    /// pick swing still does too (`WorldState::work_temple_fitting` is the one entry point).
    TempleFitting(crate::sky::SkyTile),
}

impl Usable {
    /// What can be used on a tile of `kind`.
    #[must_use]
    pub const fn of(kind: TileKind) -> Option<Self> {
        match kind {
            TileKind::Built(BuiltTile::Lever { channel, on }) => Some(Self::Lever { channel, on }),
            TileKind::Built(BuiltTile::Chest { open: false, .. }) => Some(Self::Chest),
            TileKind::Built(BuiltTile::KeycardDoor(color)) => Some(Self::KeycardDoor { color }),
            TileKind::Sky(fitting) if fitting.is_temple_fitting() => {
                Some(Self::TempleFitting(fitting))
            }
            _ => None,
        }
    }

    /// A short verb for the prompt.
    #[must_use]
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Lever { .. } | Self::TempleFitting(crate::sky::SkyTile::TempleLever) => {
                "Pull the lever"
            }
            Self::Chest => "Open the chest",
            Self::KeycardDoor { .. } => "Swipe a keycard",
            Self::TempleFitting(crate::sky::SkyTile::TempleKeystone) => "Prise the keystone",
            Self::TempleFitting(crate::sky::SkyTile::KeystoneDoor) => "Set the keystone",
            Self::TempleFitting(_) => "Open the relic chest",
        }
    }
}

/// What happened at a mechanism ([`crate::GameplayEventKind::MechanismUsed`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum MechanismEvent {
    /// A lever was flipped (`on`: now on).
    LeverPulled { on: bool },
    /// A plate went down under a weight.
    PlatePressed,
    /// A plate came back up.
    PlateReleased,
    /// A gate column opened.
    GateOpened,
    /// A gate column closed.
    GateClosed,
    /// A keycard door slid open.
    KeycardAccepted { color: u8 },
    /// A keycard door refused a pilot without its card.
    Locked { color: u8 },
}

/// Release ticks of the pressure plates being held down (part of the bases section, so it
/// saves and replicates with the claims). Stored as a sorted list: JSON keys must be strings.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlateTimers {
    #[serde(default)]
    pub(crate) held: Vec<(TilePosition, SimulationTick)>,
}

impl PlateTimers {
    /// When the plate at `tile` releases, if it is held.
    #[must_use]
    pub fn release_at(&self, tile: TilePosition) -> Option<SimulationTick> {
        self.held
            .binary_search_by(|(position, _)| position.cmp(&tile))
            .ok()
            .map(|index| self.held[index].1)
    }

    /// Holds the plate at `tile` until `until`.
    pub fn hold(&mut self, tile: TilePosition, until: SimulationTick) {
        match self
            .held
            .binary_search_by(|(position, _)| position.cmp(&tile))
        {
            Ok(index) => self.held[index].1 = until,
            Err(index) => self.held.insert(index, (tile, until)),
        }
    }

    /// Takes every plate whose release tick is at or before `now`, in tile order.
    pub fn take_due(&mut self, now: SimulationTick) -> Vec<TilePosition> {
        let due: Vec<TilePosition> = self
            .held
            .iter()
            .filter(|(_, until)| *until <= now)
            .map(|(tile, _)| *tile)
            .collect();
        self.held.retain(|(_, until)| *until > now);
        due
    }

    /// Plates currently held.
    pub fn held(&self) -> impl Iterator<Item = TilePosition> + '_ {
        self.held.iter().map(|(tile, _)| *tile)
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

/// Whether gate lintel `lintel` is within reach of mechanism tile `at`.
#[must_use]
pub const fn in_reach(at: TilePosition, lintel: TilePosition) -> bool {
    (lintel.x - at.x).abs() <= REACH_COLUMNS && (lintel.y - at.y).abs() <= REACH_ROWS
}

/// A tiny deterministic mix for chest rolls.
const fn mix(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}

/// What a chest at `tile` of a world with `seed` spills: a few stacks of minerals for its depth
/// and, deeper down, an artifact-grade haul. Deterministic, so host and clients agree.
#[must_use]
pub fn chest_loot(seed: u64, tile: TilePosition) -> Vec<ItemStack> {
    #[allow(clippy::cast_sign_loss, reason = "hashing the coordinates' bits")]
    let roll = mix(seed ^ mix((u64::from(tile.x as u32) << 32) | u64::from(tile.y as u32)));
    // Deeper chests hold richer ore.
    let tier = (tile.y.max(0) / 160).min(5);
    let table: &[MineralKind] = match tier {
        0 => &[MineralKind::Copper, MineralKind::Iron, MineralKind::Silver],
        1 => &[MineralKind::Silver, MineralKind::Gold, MineralKind::Emerald],
        2 => &[
            MineralKind::Silver,
            MineralKind::Gold,
            MineralKind::Platinum,
        ],
        3 => &[
            MineralKind::Gold,
            MineralKind::Platinum,
            MineralKind::Emerald,
        ],
        4 => &[
            MineralKind::Platinum,
            MineralKind::Emerald,
            MineralKind::Ruby,
        ],
        _ => &[
            MineralKind::Emerald,
            MineralKind::Ruby,
            MineralKind::Diamond,
        ],
    };
    let stacks = 2 + (roll % 2) as usize;
    (0..stacks)
        .map(|index| {
            let pick = mix(roll.wrapping_add(index as u64 * 0x9E37_79B9));
            let mineral = table[usize::try_from(pick % table.len() as u64).unwrap_or(0)];
            let count = 2 + u32::try_from((pick >> 8) % 4).unwrap_or(0);
            ItemStack::new(ItemKind::Mineral(mineral), count)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycards_round_trip_and_leave_other_quest_items_alone() {
        for color in 0..KEYCARD_COLORS {
            assert_eq!(keycard_of(keycard_item(color)), Some(color));
        }
        assert_eq!(keycard_of(ItemKind::Quest(QuestItemId(3))), None);
        assert_eq!(keycard_of(ItemKind::Mineral(MineralKind::Copper)), None);
    }

    #[test]
    fn plate_timers_stay_sorted_and_release_on_time() {
        let mut timers = PlateTimers::default();
        let a = TilePosition { x: 5, y: 9 };
        let b = TilePosition { x: -3, y: 9 };
        timers.hold(a, SimulationTick::new(10));
        timers.hold(b, SimulationTick::new(20));
        timers.hold(a, SimulationTick::new(30));
        assert_eq!(timers.release_at(a), Some(SimulationTick::new(30)));
        assert_eq!(timers.take_due(SimulationTick::new(25)), vec![b]);
        assert_eq!(timers.held().collect::<Vec<_>>(), vec![a]);
        assert!(timers.take_due(SimulationTick::new(29)).is_empty());
        assert_eq!(timers.take_due(SimulationTick::new(30)), vec![a]);
        assert!(timers.is_empty());
    }

    #[test]
    fn chest_loot_is_deterministic_and_richer_deeper() {
        let shallow = chest_loot(7, TilePosition { x: 40, y: 60 });
        assert_eq!(shallow, chest_loot(7, TilePosition { x: 40, y: 60 }));
        assert!((2..=3).contains(&shallow.len()));
        let value = |loot: &[ItemStack]| -> u32 {
            loot.iter()
                .map(|stack| stack.kind.base_value() * stack.count)
                .sum()
        };
        let deep = chest_loot(7, TilePosition { x: 40, y: 900 });
        assert!(value(&deep) > value(&shallow));
        assert!(
            Usable::of(TileKind::Built(BuiltTile::Chest {
                open: true,
                card: None
            }))
            .is_none()
        );
    }
}
