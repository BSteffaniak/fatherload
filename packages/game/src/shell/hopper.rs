//! The block hopper rows of the inventory dialog (WP-DROPS): the "collect blocks" toggle and
//! dumping blocks, by keyboard, mouse-free menus and controller alike; then the pilot's
//! personal pack rows (Part II F5): move a stack from the pack into the rig, or from the rig's
//! bay or hopper into the pack (the manual swap; boarding empties the pack automatically).
//!
//! The rows are shared with the renderer (`rendering::counters::hopper_section`), so the
//! labels always describe what confirm does.

use fatherload_simulation::{
    Player, PlayerCommand,
    item::{BlockKind, ItemKind},
};

/// One selectable row of the inventory's hopper section, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HopperRow {
    /// Flip the rig's "collect blocks" toggle.
    ToggleCollect,
    /// Tip every block out.
    DumpAll,
    /// Tip one kind out.
    Dump(BlockKind),
    /// Move a whole stack between the pilot's pack and the rig (`to_rig`: pack to rig).
    Pack { item: ItemKind, to_rig: bool },
}

impl HopperRow {
    /// The command confirming this row submits for `player`.
    #[must_use]
    pub const fn command(self, player: &Player) -> PlayerCommand {
        match self {
            Self::ToggleCollect => PlayerCommand::SetCollectBlocks {
                enabled: !player.collect_blocks,
            },
            Self::DumpAll => PlayerCommand::DumpBlocks { block: None },
            Self::Dump(block) => PlayerCommand::DumpBlocks { block: Some(block) },
            // The simulation moves as much as fits (and as much as there is).
            Self::Pack { item, to_rig } => PlayerCommand::PilotPack {
                item,
                count: u32::MAX,
                to_rig,
            },
        }
    }
}

/// The hopper rows for `player`: the toggle, then (with blocks aboard) "dump all" and one row
/// per kind held.
#[must_use]
pub fn hopper_rows(player: &Player) -> Vec<HopperRow> {
    let mut rows = vec![HopperRow::ToggleCollect];
    if !player.block_hopper.is_empty() {
        rows.push(HopperRow::DumpAll);
        rows.extend(
            player
                .block_hopper
                .blocks()
                .map(|(block, _)| HopperRow::Dump(block)),
        );
    }
    rows.extend(pack_rows(player));
    rows
}

/// The pack rows: each stack in the pack (to the rig), then the rig's ore, relics and blocks
/// that could go into the pack.
#[must_use]
pub fn pack_rows(player: &Player) -> Vec<HopperRow> {
    let mut rows: Vec<HopperRow> = player
        .pilot_pack
        .iter()
        .map(|(item, _)| HopperRow::Pack { item, to_rig: true })
        .collect();
    let room = player
        .pilot_gear
        .pack_capacity()
        .saturating_sub(player.pilot_pack.total());
    if room > 0 {
        rows.extend(
            player
                .cargo_bay
                .iter()
                .filter(|(item, _)| matches!(item, ItemKind::Mineral(_) | ItemKind::Artifact(_)))
                .chain(player.block_hopper.iter())
                .map(|(item, _)| HopperRow::Pack {
                    item,
                    to_rig: false,
                }),
        );
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_hopper() {
        let mut player = Player::new(0.0, 0.0);
        assert_eq!(hopper_rows(&player), vec![HopperRow::ToggleCollect]);
        assert_eq!(
            HopperRow::ToggleCollect.command(&player),
            PlayerCommand::SetCollectBlocks { enabled: false }
        );
        player.block_hopper.add(BlockKind::Stone, 4);
        player.block_hopper.add(BlockKind::Dirt, 9);
        assert_eq!(
            hopper_rows(&player),
            vec![
                HopperRow::ToggleCollect,
                HopperRow::DumpAll,
                HopperRow::Dump(BlockKind::Dirt),
                HopperRow::Dump(BlockKind::Stone),
            ]
        );
    }
}
