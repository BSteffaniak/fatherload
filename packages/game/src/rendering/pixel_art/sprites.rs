//! Every sprite definition, grouped by where it is used. Register new sprites in [`ALL`] so the
//! atlas packs them (the tests fail for a sprite that is drawn but missing here only at runtime,
//! so keep groups' lists complete).

pub mod crypt;
pub mod deep_gear;
pub mod drowned;
pub mod enemies;
pub mod explosives;
pub mod icons;
pub mod interior;
pub mod living;
pub mod menu_icons;
pub mod moon;
pub mod old_deep;
pub mod poi;
pub mod prompts;
pub mod rig;
pub mod terrain;
pub mod tomb;
pub mod ui;
pub mod weapons;
pub mod world;

use super::sprite::SpriteDef;

/// Sprites the atlas also packs as white silhouettes (Wave 6 art pass): small or dark enemies
/// that get a rim light and an outline so they read against a night or space sky.
pub const SILHOUETTED: &[&SpriteDef] = &[
    &enemies::SHADOW_HOUND,
    &enemies::DUNE_STALKER,
    &enemies::UMBRAL_ALPHA,
    &enemies::WILL_O_WISP,
    &enemies::WYVERN,
    &enemies::SKY_RAY,
    &enemies::HARPY_NESTER,
    &moon::WARDEN,
    &moon::REGOLITH_CRAWLER,
    &moon::CRYSTAL_MITE,
    &moon::DERELICT_AUTOMATON,
];

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
    &icons::ALTITUDE,
    &icons::WARNING,
    &icons::HEAT,
    &icons::BOMB,
    &icons::SCANNER,
    &icons::HEAT_SHIELD,
    &icons::LAVA_PLATING,
    &icons::GAS_SCRUBBER,
    &icons::PRESSURE_BRACING,
    &icons::BOULDER_DEFLECTOR,
    &icons::ALTITUDE_INTAKE,
    &icons::DE_ICER,
    &icons::AIR,
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
    &icons::SKYSTONE_ORE,
    &icons::CLOUD_SILVER_ORE,
    &icons::DIRT_BLOCK,
    &icons::CLAY_BLOCK,
    &icons::STONE_BLOCK,
    &icons::HARD_ROCK_BLOCK,
    &icons::MAGNET,
    &icons::MOON,
    &icons::SUN,
    &rig::RIG,
    &rig::RIG_DAMAGED,
    &rig::DRILL_DOWN,
    &rig::DRILL_UP,
    &rig::DRILL_SIDE,
    &rig::FLAME,
    &rig::FLAME_SMALL,
    &rig::ICON_SPRAY_CAN,
    &rig::ICON_FLAME,
    &rig::ICON_PATTERN,
    &rig::ICON_STAR,
    &rig::ICON_ANTENNA,
    &rig::ICON_LAMP,
    &rig::ICON_PENNANT,
    &rig::ICON_DICE,
    &rig::ICON_CREW,
    &world::BOULDER,
    &world::BOMB,
    &explosives::STICKY,
    &explosives::CLUSTER,
    &explosives::BOMBLET,
    &explosives::C4,
    &explosives::SHAPED,
    &explosives::MINING,
    &explosives::INCENDIARY,
    &explosives::EMP,
    &explosives::FLARE,
    &explosives::ICON_STICKY,
    &explosives::ICON_CLUSTER,
    &explosives::ICON_C4,
    &explosives::ICON_SHAPED,
    &explosives::ICON_MINING,
    &explosives::ICON_INCENDIARY,
    &explosives::ICON_EMP,
    &explosives::ICON_FLARE,
    &explosives::SMOKE,
    &explosives::SCARE,
    &explosives::ICON_SMOKE,
    &explosives::ICON_SCARE,
    &weapons::RIVET_GUN,
    &weapons::SCATTER_BLASTER,
    &weapons::PLASMA_CUTTER,
    &weapons::ROCKET_LAUNCHER,
    &weapons::HARPOON,
    &weapons::RAILGUN,
    &weapons::ARC_RIFLE,
    &weapons::GRAVITY_GUN,
    &weapons::TETHER_HARPOON,
    &weapons::FLAK_CANNON,
    &weapons::FLAK_SHELL,
    &weapons::ICON_ARC_RIFLE,
    &weapons::ICON_GRAVITY_GUN,
    &weapons::ICON_TETHER_HARPOON,
    &weapons::ICON_FLAK_CANNON,
    &weapons::RIVET,
    &weapons::PELLET,
    &weapons::ROCKET,
    &weapons::SPEAR,
    &weapons::TARGET,
    &weapons::ICON_RIVET_GUN,
    &weapons::ICON_SCATTER_BLASTER,
    &weapons::ICON_PLASMA_CUTTER,
    &weapons::ICON_ROCKET_LAUNCHER,
    &weapons::ICON_HARPOON,
    &weapons::ICON_RAILGUN,
    &weapons::ICON_BATTERY,
    &weapons::ICON_ROCKET_AMMO,
    &weapons::ICON_CARTRIDGE_AMMO,
    &weapons::ICON_SHELL_AMMO,
    &icons::PILOT_ARMOR,
    &icons::HEADLAMP,
    &icons::BOOTS,
    &icons::GLIDER,
    &icons::GRAPPLE,
    &icons::RIG_LINK,
    &weapons::PILOT_SIDEARM,
    &weapons::PILOT_RIFLE,
    &weapons::PILOT_SHOTGUN,
    &weapons::PILOT_WRENCH,
    &weapons::RIFLE_ROUND,
    &weapons::PISTOL_ROUND,
    &weapons::ICON_PILOT_SIDEARM,
    &weapons::ICON_PILOT_RIFLE,
    &weapons::ICON_PILOT_SHOTGUN,
    &weapons::ICON_PILOT_WRENCH,
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
    &terrain::ORE_MYTHRIL_GEODE,
    &terrain::ORE_MYTHRIL,
    &terrain::ORE_SKYSTONE,
    &terrain::ORE_CLOUD_SILVER,
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
    &terrain::WIND_ASTROLABE,
    &terrain::CLOUD_PEARL,
    &terrain::CLAY_POT,
    &world::MINE_CART,
    &terrain::HARPY_EGG,
    &terrain::SKY_RELIC,
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
    &interior::village_timber::WALL,
    &interior::village_timber::PLANK,
    &interior::village_timber::FLOOR,
    &interior::village_timber::COUNTER_SPRITE,
    &interior::village_timber::KEEPER,
    &interior::village_stone::WALL,
    &interior::village_stone::PLANK,
    &interior::village_stone::FLOOR,
    &interior::village_stone::COUNTER_SPRITE,
    &interior::village_stone::KEEPER,
    &interior::village_adobe::WALL,
    &interior::village_adobe::PLANK,
    &interior::village_adobe::FLOOR,
    &interior::village_adobe::COUNTER_SPRITE,
    &interior::village_adobe::KEEPER,
    &interior::village_scrap::WALL,
    &interior::village_scrap::PLANK,
    &interior::village_scrap::FLOOR,
    &interior::village_scrap::COUNTER_SPRITE,
    &interior::village_scrap::KEEPER,
    &interior::village_carved::WALL,
    &interior::village_carved::PLANK,
    &interior::village_carved::FLOOR,
    &interior::village_carved::COUNTER_SPRITE,
    &interior::village_carved::KEEPER,
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
    &menu_icons::GHOST,
    &menu_icons::MEDAL_BRONZE,
    &menu_icons::MEDAL_SILVER,
    &menu_icons::MEDAL_GOLD,
    &menu_icons::MEDAL_PLATINUM,
    &menu_icons::MEDAL_LOCKED,
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
    &enemies::CAVE_BAT,
    &enemies::BURROWER,
    &enemies::ACID_SPITTER,
    &enemies::ROCK_MIMIC,
    &enemies::SHELLBACK,
    &enemies::BURROW_MOLE,
    &enemies::MUD_LEECH,
    &enemies::POTTER_WASP,
    &enemies::MINE_BAT,
    &enemies::TIMBER_BEETLE,
    &enemies::SLATE_SKINK,
    &enemies::SHELF_CRAB,
    &enemies::TUBE_CRAWLER,
    &enemies::CINDER_MOTH,
    &enemies::MAGMA_WISP,
    &enemies::BROOD_NEST,
    &enemies::BROOD_LORD,
    &enemies::MINING_DRONE,
    &enemies::SENTRY_TURRET,
    &enemies::ROGUE_EXCAVATOR,
    &enemies::ANCIENT_GUARDIAN,
    &enemies::SHOT_ACID,
    &enemies::SHOT_FIREBALL,
    &enemies::SHOT_BOLT,
    &enemies::SKY_RAY,
    &enemies::WYVERN,
    &enemies::STORM_ELEMENTAL,
    &enemies::PIRATE_AIRSHIP,
    &enemies::AIRSHIP_TURRET,
    &enemies::SKY_PIRATE,
    &enemies::HARPY_NESTER,
    &enemies::SHOT_CANNONBALL,
    // The Moon (WP-MOON).
    &moon::REGOLITH_CRAWLER,
    &moon::CRYSTAL_MITE,
    &moon::DERELICT_AUTOMATON,
    &moon::WARDEN,
    &moon::PRESSURIZED_HULL,
    &moon::LIFE_SUPPORT,
    &moon::LUNAR_RELIC,
    &moon::WARDEN_EYE,
    // Descent II, the Old Deep (Wave 8).
    &old_deep::PRISM_MOTH,
    &old_deep::FOSSIL_EEL,
    &old_deep::JET_RAY,
    &old_deep::GLOW_GRUB,
    &old_deep::GLOWMOTHER,
    &old_deep::PISTON_WARDEN,
    &old_deep::VOID_JELLY,
    &old_deep::MAGMA_SALAMANDER,
    &moon::LUNAR_CUTTER,
    &moon::LOW_G_MORTAR,
    &moon::VOID_LANCE,
    &moon::ICON_LUNAR_CUTTER,
    &moon::ICON_LOW_G_MORTAR,
    &moon::ICON_VOID_LANCE,
    &moon::MORTAR_SHELL,
    &moon::MOON_ICON,
    &enemies::SHADOW_HOUND,
    &enemies::DUNE_STALKER,
    &enemies::WILL_O_WISP,
    &enemies::NIGHT_MOTH,
    &enemies::UMBRAL_ALPHA,
    &enemies::SHOT_WISPFIRE,
    &tomb::MUMMY,
    &tomb::CRYPT_BAT,
    &tomb::SKELETAL_ARCHER,
    &tomb::ANIMATED_STATUE,
    &tomb::SUN_COLOSSUS,
    &tomb::HOLLOW_MATRIARCH,
    &tomb::SHOT_ARROW,
    &tomb::SHOT_RUBBLE,
    &tomb::SHOT_HEX,
    &crypt::CRYPT_SKELETON,
    &crypt::SPECTRE,
    &crypt::CULTIST,
    &crypt::CRYPT_GOLEM,
    &crypt::CRYPT_LORD,
    &crypt::SHOT_HEX,
    &crypt::SHOT_ARROW,
    &crypt::SHOT_ARROW_UP,
    &crypt::SHOT_BOULDER,
    &deep_gear::PRESSURE_RATING,
    &deep_gear::HEADLAMP,
    &deep_gear::WORKSHOP,
    &deep_gear::LAYER_FIBRE,
    &deep_gear::LAYER_LUMP,
    &deep_gear::LAYER_GEM,
    &deep_gear::LAYER_RESIN,
    &deep_gear::LAYER_SHARD,
    &deep_gear::LAYER_RELIC,
    // The Living Dark (Wave 8).
    &living::SPORE_PUFFER,
    &living::CAP_CRAWLER,
    &living::GLOW_GNAT,
    &living::ROOT_SNAPPER,
    &living::BARK_BEETLE,
    &living::THORN_WISP,
    &living::HIVE_SOLDIER,
    &living::HIVE_DRONE,
    &living::ACID_LANCER,
    &living::SWARM_MATRIARCH,
    &living::MAW_WORM,
    &living::BILE_POLYP,
    &living::LEECHLING,
    &living::LURE_FISH,
    &living::GLOW_JELLY,
    &living::ANGLER,
    &living::MYCELIAL_MIND,
    &living::MIND_NODE,
    &living::SHOT_SPORE,
    &living::SHOT_THORN,
    &living::SHOT_MIND_SPORE,
    // The Drowned and Frozen (Wave 9).
    &drowned::REEF_SHARK,
    &drowned::INK_SQUID,
    &drowned::EEL_DRAGON,
    &drowned::FROST_MITE,
    &drowned::TUSK_SEAL,
    &drowned::SCALD_BEETLE,
    &drowned::SINTER_SNAIL,
    &drowned::SPINEPUFF,
    &drowned::LURE_ANGLER,
    &drowned::GULPER_EEL,
    &drowned::LEVIATHAN,
    &drowned::SHOT_PRESSURE_BUBBLE,
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
        let layout = AtlasLayout::pack_with(ALL, SILHOUETTED);
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
        for sprite in SILHOUETTED {
            assert!(ALL.contains(sprite), "{} is registered", sprite.name);
            for frame in 0..sprite.frames.len() {
                let rect = layout.silhouette(sprite.name, frame).expect("packed");
                assert!(rect.y + rect.height <= layout.height);
                rects.push(rect);
            }
        }
        // WebGL 1 (the browser build) guarantees textures up to 4096 pixels.
        assert!(layout.height <= 4_096, "atlas {} px tall", layout.height);
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
