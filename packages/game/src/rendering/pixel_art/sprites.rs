//! Every sprite definition, grouped by where it is used. Register new sprites in [`ALL`] so the
//! atlas packs them (the tests fail for a sprite that is drawn but missing here only at runtime,
//! so keep groups' lists complete).

pub mod icons;
pub mod interior;
pub mod rig;
pub mod ui;

use super::sprite::SpriteDef;

/// Every sprite the atlas packs, in packing order.
pub const ALL: &[&SpriteDef] = &[
    &ui::PANEL_STANDARD_SPRITE,
    &ui::PANEL_ACCENT_SPRITE,
    &ui::PANEL_WARNING_SPRITE,
    &ui::WELL_SPRITE,
    &ui::BUTTON_IDLE_SPRITE,
    &ui::BUTTON_SELECTED_SPRITE,
    &ui::BUTTON_DISABLED_SPRITE,
    &ui::TAB_ACTIVE_SPRITE,
    &ui::TAB_IDLE_SPRITE,
    &ui::TOOLTIP_SPRITE,
    &ui::TOOLTIP_TAIL,
    &ui::CURSOR,
    &icons::FUEL,
    &icons::HULL,
    &icons::CARGO,
    &icons::CREDITS,
    &icons::DEPTH,
    &icons::WARNING,
    &icons::HEAT,
    &icons::BOMB,
    &icons::SCANNER,
    &icons::COPPER_ORE,
    &icons::IRON_ORE,
    &icons::SILVER_ORE,
    &icons::GOLD_ORE,
    &icons::EMERALD_ORE,
    &icons::RUBY_ORE,
    &icons::DIAMOND_ORE,
    &icons::PLATINUM_ORE,
    &icons::URANIUM_ORE,
    &icons::MYTHRIL_ORE,
    &rig::RIG,
    &rig::RIG_DAMAGED,
    &rig::DRILL_DOWN,
    &rig::DRILL_SIDE,
    &rig::FLAME,
    &interior::shop::WALL,
    &interior::shop::PLANK,
    &interior::shop::FLOOR,
    &interior::shop::COUNTER_SPRITE,
    &interior::shop::KEEPER,
    &interior::fuel::WALL,
    &interior::fuel::PLANK,
    &interior::fuel::FLOOR,
    &interior::fuel::COUNTER_SPRITE,
    &interior::fuel::KEEPER,
    &interior::repair::WALL,
    &interior::repair::PLANK,
    &interior::repair::FLOOR,
    &interior::repair::COUNTER_SPRITE,
    &interior::repair::KEEPER,
    &interior::depot::WALL,
    &interior::depot::PLANK,
    &interior::depot::FLOOR,
    &interior::depot::COUNTER_SPRITE,
    &interior::depot::KEEPER,
    &interior::hq::WALL,
    &interior::hq::PLANK,
    &interior::hq::FLOOR,
    &interior::hq::COUNTER_SPRITE,
    &interior::hq::KEEPER,
    &interior::bank::WALL,
    &interior::bank::PLANK,
    &interior::bank::FLOOR,
    &interior::bank::COUNTER_SPRITE,
    &interior::bank::KEEPER,
    &interior::explosives::WALL,
    &interior::explosives::PLANK,
    &interior::explosives::FLOOR,
    &interior::explosives::COUNTER_SPRITE,
    &interior::explosives::KEEPER,
    &interior::salvage::WALL,
    &interior::salvage::PLANK,
    &interior::salvage::FLOOR,
    &interior::salvage::COUNTER_SPRITE,
    &interior::salvage::KEEPER,
    &interior::DOOR_FRAME_SPRITE,
    &interior::EXIT_SIGN,
    &interior::LAMP,
    &interior::GEAR,
    &interior::PART_BOX,
    &interior::CAN,
    &interior::CRATE,
    &interior::BARREL,
    &interior::FUEL_PUMP,
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::rendering::pixel_art::{palette::is_palette_color, sprite::AtlasLayout};

    #[test]
    fn sprites_are_well_formed_and_use_the_palette() {
        let mut names = BTreeSet::new();
        for sprite in ALL {
            assert!(
                names.insert(sprite.name),
                "duplicate sprite name {}",
                sprite.name
            );
            assert!(!sprite.frames.is_empty(), "{} has no frames", sprite.name);
            let (width, height) = (sprite.width(), sprite.height());
            assert!(width > 0 && height > 0, "{} is empty", sprite.name);
            for (index, frame) in sprite.frames.iter().enumerate() {
                assert_eq!(
                    i32::try_from(frame.len()).unwrap(),
                    height,
                    "{} frame {index} height",
                    sprite.name
                );
                for row in *frame {
                    assert_eq!(
                        i32::try_from(row.chars().count()).unwrap(),
                        width,
                        "{} frame {index} row {row:?}",
                        sprite.name
                    );
                    for symbol in row.chars() {
                        assert!(
                            sprite.color_of(symbol).is_some(),
                            "{} uses {symbol:?} without a legend entry",
                            sprite.name
                        );
                    }
                }
            }
            for (symbol, color) in sprite.legend {
                assert!(
                    is_palette_color(*color),
                    "{} maps {symbol:?} to an off-palette colour",
                    sprite.name
                );
            }
        }
    }

    #[test]
    fn nine_slices_have_room_for_their_borders() {
        for slice in [
            &ui::PANEL_STANDARD,
            &ui::PANEL_ACCENT,
            &ui::PANEL_WARNING,
            &ui::WELL,
            &ui::BUTTON_IDLE,
            &ui::BUTTON_SELECTED,
            &ui::BUTTON_DISABLED,
            &ui::TAB_ACTIVE,
            &ui::TAB_IDLE,
            &ui::TOOLTIP,
            &interior::DOOR_FRAME,
            &interior::shop::COUNTER,
        ] {
            assert!(
                slice.sprite.width() > slice.border * 2,
                "{}",
                slice.sprite.name
            );
            assert!(
                slice.sprite.height() > slice.border * 2,
                "{}",
                slice.sprite.name
            );
            assert!(ALL.iter().any(|sprite| sprite.name == slice.sprite.name));
        }
    }

    #[test]
    fn atlas_packs_every_frame_without_overlap() {
        let layout = AtlasLayout::pack(ALL);
        let mut rects = Vec::new();
        for sprite in ALL {
            for frame in 0..sprite.frames.len() {
                let rect = layout.frame(sprite.name, frame).expect("packed");
                assert!(rect.x >= 0 && rect.y >= 0);
                assert!(rect.x + rect.width <= layout.width);
                assert!(rect.y + rect.height <= layout.height);
                rects.push(rect);
            }
        }
        for (index, a) in rects.iter().enumerate() {
            for b in &rects[index + 1..] {
                let disjoint = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                assert!(disjoint, "{a:?} overlaps {b:?}");
            }
        }
    }
}
