//! Every sprite definition, grouped by where it is used. Register new sprites in [`ALL`] so the
//! atlas packs them (the tests fail for a sprite that is drawn but missing here only at runtime,
//! so keep groups' lists complete).

pub mod icons;
pub mod interior;
pub mod menu_icons;
pub mod poi;
pub mod prompts;
pub mod rig;
pub mod terrain;
pub mod ui;
pub mod world;

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
    &rig::DRILL_UP,
    &rig::DRILL_SIDE,
    &rig::FLAME,
    &rig::FLAME_SMALL,
    &rig::RIG_TEAL,
    &rig::RIG_ROSE,
    &rig::RIG_LIME,
    &rig::RIG_VIOLET,
    &rig::RIG_SKY,
    &world::BOULDER,
    &world::BOMB,
    &world::CARGO_CRATE,
    &world::MARKER_ARROW,
    &world::SIGNAL_RELAY,
    &world::SURVEY_DRONE,
    &world::CARGO_LIFT,
    &world::TUNNEL_SUPPORT,
    &world::PUMP_STATION,
    &world::ORE_PROCESSOR,
    &world::BEDROCK,
    &terrain::ORE_COPPER,
    &terrain::ORE_IRON,
    &terrain::ORE_SILVER,
    &terrain::ORE_GOLD,
    &terrain::ORE_EMERALD,
    &terrain::ORE_RUBY,
    &terrain::ORE_DIAMOND,
    &terrain::ORE_PLATINUM,
    &terrain::ORE_URANIUM,
    &terrain::ORE_MYTHRIL,
    &terrain::VOLATILE_CRYSTAL,
    &terrain::FOSSIL,
    &terrain::OLD_CIRCUIT,
    &terrain::BURIED_IDOL,
    &terrain::STAR_CORE,
    &terrain::CRYSTAL_PRISM,
    &terrain::RADIANT_SHELL,
    &terrain::PRESSURE_GLYPH,
    &terrain::REACTOR_BLOOM,
    &terrain::MACHINE_SIGIL,
    &terrain::VOID_PEARL,
    &terrain::MANTLE_HEART,
    &terrain::CRACKS,
    &terrain::CRACKS_LIT,
    &poi::CRATE,
    &poi::SALVAGE_CORE,
    &poi::VAULT_DOOR,
    &poi::VAULT_SEAL,
    &poi::VAULT_SEAL_VENTED,
    &poi::SWITCH_1,
    &poi::SWITCH_2,
    &poi::SWITCH_3,
    &poi::SWITCH_1_LIT,
    &poi::SWITCH_2_LIT,
    &poi::SWITCH_3_LIT,
    &poi::RELAY,
    &poi::RELAY_LIT,
    &poi::VAULT_CACHE,
    &poi::LOCKER,
    &poi::SHELL_IMPRINT,
    &poi::BONE_IMPRINT,
    &poi::ICON_CACHE,
    &poi::ICON_DERELICT,
    &poi::ICON_VAULT,
    &poi::ICON_FORMATION,
    &poi::ICON_OUTPOST,
    &poi::ICON_ECHO,
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
    &menu_icons::HAMMER,
    &menu_icons::CONTRACT,
    &menu_icons::RECEIPT,
    &menu_icons::SORT,
    &menu_icons::BOOK,
    &menu_icons::LETTER,
    &menu_icons::BANK,
    &menu_icons::INSURANCE,
    &menu_icons::ROCKET,
    &menu_icons::GIFT,
    &menu_icons::DRONE,
    &menu_icons::SCRAP,
    &menu_icons::COLLAPSE,
    &menu_icons::HOUSE,
    &menu_icons::FLAG,
    &menu_icons::ANVIL,
    &menu_icons::DISK,
    &menu_icons::FOLDER,
    &menu_icons::PERSON,
    &menu_icons::READY,
    &menu_icons::WAITING,
    &menu_icons::OFFLINE,
    &menu_icons::CROWN,
    &menu_icons::SIGNAL,
    &menu_icons::KEY,
    &menu_icons::SPEAKER,
    &menu_icons::SCREEN,
    &menu_icons::PLAY,
    &menu_icons::DOOR,
    &menu_icons::BACK,
    &menu_icons::GLOBE,
    &menu_icons::SPLIT,
    &menu_icons::DICE,
    &menu_icons::GEAR,
    &prompts::KEYCAP_SPRITE,
    &prompts::XBOX_A,
    &prompts::XBOX_B,
    &prompts::XBOX_X,
    &prompts::XBOX_Y,
    &prompts::NINTENDO_A,
    &prompts::NINTENDO_B,
    &prompts::NINTENDO_X,
    &prompts::NINTENDO_Y,
    &prompts::PS_CROSS,
    &prompts::PS_CIRCLE,
    &prompts::PS_SQUARE,
    &prompts::PS_TRIANGLE,
    &prompts::BUMPER,
    &prompts::TRIGGER,
    &prompts::MENU,
    &prompts::VIEW,
    &prompts::DPAD_VERTICAL,
    &prompts::DPAD_HORIZONTAL,
    &prompts::DPAD_DOWN,
    &prompts::LEFT_STICK,
    &prompts::RIGHT_STICK,
    &prompts::MOUSE_WHEEL,
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
            &prompts::KEYCAP,
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
    fn every_prompt_glyph_is_registered_and_prompt_height() {
        for glyph in prompts::ALL {
            assert!(
                ALL.iter().any(|sprite| sprite.name == glyph.name),
                "{}",
                glyph.name
            );
            assert_eq!(glyph.height(), prompts::GLYPH_HEIGHT, "{}", glyph.name);
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
