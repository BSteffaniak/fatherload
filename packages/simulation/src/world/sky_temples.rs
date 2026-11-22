//! Sky temple fittings (Wave 4 T21): the lever and keystone gates of the pilot-only temples
//! on V9 sky continents ([`crate::worldgen::sky::continent::TEMPLE`]).
//!
//! A pilot works a fitting with a swing of the hand pick (the same `weapon.fire` control and
//! touch button as chipping dirt), so no new input is needed on any device:
//!
//! * **Lever** ([`SkyTile::TempleLever`]): opens every temple gate ([`SkyTile::TempleDoor`])
//!   within [`LEVER_REACH`] (`TempleLeverPulled`).
//! * **Keystone** ([`SkyTile::TempleKeystone`]): prised off its plinth, it drops as
//!   [`crate::item::TEMPLE_KEYSTONE`] and the pilot picks it up.
//! * **Keystone gate** ([`SkyTile::KeystoneDoor`]): a pilot with a keystone in its pack spends
//!   it to open the gate's tiles ([`GATE_REACH`]) (`TempleGateOpened`); without one the pick
//!   clinks.
//! * **Relic chest** ([`SkyTile::TempleCache`]): breaks open and spills [`relic_loot`] through
//!   the drop rules (`RelicChestOpened`).
//!
//! Gates and fittings are indestructible to drills, weapons and blasts, so the temple is a
//! pilot puzzle. **Seam for T23** (WP-PILOT's lever doors): [`WorldState::work_temple_fitting`]
//! is the one entry point; a pilot "interact" action can call it for the tile in front of the
//! pilot, and ruins can reuse the tiles.

use crate::{
    event::{GameplayEventKind, TickOutput},
    ids::PlayerId,
    item::{ItemKind, ItemStack, TEMPLE_KEYSTONE},
    sky::SkyTile,
    terrain::{ArtifactKind, MineralKind, StrategicResourceKind, TileKind, TilePosition},
    worldgen::noise::hash2,
};

use super::WorldState;

/// Columns either side (and rows above and below, a third of it) a pulled lever opens gates.
pub const LEVER_REACH: i32 = 24;
/// How far a keystone gate's tiles reach from the tile struck.
pub const GATE_REACH: i32 = 3;

const SALT_RELIC: u64 = 0x5C1E_7E01;

/// What a temple's relic chest at `tile` holds (a sky relic, aether crystal, storm glass and
/// rich ore; `seed` is the world seed).
#[must_use]
pub fn relic_loot(seed: u64, tile: TilePosition) -> Vec<ItemStack> {
    let roll = hash2(seed, SALT_RELIC, tile.x, tile.y);
    let extra = u32::try_from(roll % 3).unwrap_or(0);
    vec![
        ItemStack::new(ItemKind::Artifact(ArtifactKind::SkyRelic), 1),
        ItemStack::new(
            ItemKind::Material(StrategicResourceKind::AetherCrystal),
            2 + extra,
        ),
        ItemStack::new(
            ItemKind::Material(StrategicResourceKind::StormGlass),
            1 + extra,
        ),
        ItemStack::new(
            ItemKind::Mineral(if roll.is_multiple_of(4) {
                MineralKind::Mythril
            } else {
                MineralKind::Platinum
            }),
            2 + extra,
        ),
    ]
}

impl WorldState {
    /// Works the temple fitting `fitting` at `tile` for `player`'s pilot (a pick swing; see the
    /// module docs). Returns whether anything happened (otherwise the pick clinks).
    pub fn work_temple_fitting(
        &mut self,
        player: PlayerId,
        tile: TilePosition,
        fitting: SkyTile,
        output: &mut TickOutput,
    ) -> bool {
        match fitting {
            SkyTile::TempleLever => {
                let opened = self.open_tiles_near(
                    tile,
                    (LEVER_REACH, LEVER_REACH / 3),
                    TileKind::Sky(SkyTile::TempleDoor),
                );
                output.push_event(GameplayEventKind::TempleLeverPulled {
                    player_id: player,
                    x: tile.x,
                    y: tile.y,
                    opened: u16::try_from(opened).unwrap_or(u16::MAX),
                });
                true
            }
            SkyTile::TempleKeystone => {
                let kind = TileKind::Sky(SkyTile::TempleKeystone);
                if !self.terrain.set_kind(tile, TileKind::Air) {
                    return false;
                }
                let _drop = self.spawn_tile_drop(tile, kind, (0.0, -60.0));
                output.push_event(GameplayEventKind::PickStruck {
                    player_id: player,
                    x: tile.x,
                    y: tile.y,
                    broke: true,
                });
                true
            }
            SkyTile::KeystoneDoor => {
                // The keystone in the pilot's pack, or stowed in the rig's hold (boarding
                // empties the pack into the rig, and there is one keystone per temple, so a
                // stowed key must not seal the vault for good).
                let Some(rig) = self.players.get_mut(&player) else {
                    return false;
                };
                let spent = if rig.pilot_pack.count(TEMPLE_KEYSTONE) > 0 {
                    rig.pilot_pack.remove(TEMPLE_KEYSTONE, 1)
                } else {
                    rig.cargo_bay.remove(TEMPLE_KEYSTONE, 1)
                };
                if spent == 0 {
                    return false;
                }
                let _opened = self.open_tiles_near(
                    tile,
                    (GATE_REACH, GATE_REACH),
                    TileKind::Sky(SkyTile::KeystoneDoor),
                );
                output.push_event(GameplayEventKind::TempleGateOpened {
                    player_id: player,
                    x: tile.x,
                    y: tile.y,
                });
                true
            }
            SkyTile::TempleCache => {
                if !self.terrain.set_kind(tile, TileKind::Air) {
                    return false;
                }
                let (x, y) = self.frame().centre_of(tile.x, tile.y);
                for (index, stack) in (0_u8..).zip(relic_loot(self.terrain.seed(), tile)) {
                    let spread = (f32::from(index % 4) - 1.5) * 40.0;
                    let _drop = self.spawn_loot(x, y, (spread, -140.0), stack, None);
                }
                output.push_event(GameplayEventKind::RelicChestOpened {
                    player_id: player,
                    x: tile.x,
                    y: tile.y,
                });
                true
            }
            _ => false,
        }
    }

    /// Turns every tile of `kind` within `reach` (columns, rows) of `center` into air.
    /// Returns how many opened.
    fn open_tiles_near(
        &mut self,
        center: TilePosition,
        reach: (i32, i32),
        kind: TileKind,
    ) -> usize {
        let mut opened = 0;
        for y in center.y - reach.1..=center.y + reach.1 {
            for x in center.x - reach.0..=center.x + reach.0 {
                let position = TilePosition { x, y };
                if self
                    .terrain
                    .tile(position)
                    .is_some_and(|tile| tile.kind == kind)
                    && self.terrain.set_kind(position, TileKind::Air)
                {
                    opened += 1;
                }
            }
        }
        opened
    }
}
