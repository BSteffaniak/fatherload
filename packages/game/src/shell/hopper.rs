//! The block hopper rows of the inventory dialog (WP-DROPS): the "collect blocks" toggle and
//! dumping blocks, by keyboard, mouse-free menus and controller alike.
//!
//! The rows are shared with the renderer (`rendering::counters::hopper_section`), so the
//! labels always describe what confirm does.

use fatherload_simulation::{Player, PlayerCommand, item::BlockKind};

/// One selectable row of the inventory's hopper section, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HopperRow {
    /// Flip the rig's "collect blocks" toggle.
    ToggleCollect,
    /// Tip every block out.
    DumpAll,
    /// Tip one kind out.
    Dump(BlockKind),
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
