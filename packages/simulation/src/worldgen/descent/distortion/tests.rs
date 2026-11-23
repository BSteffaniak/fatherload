//! The distortion strata's generated places: mirror halls are symmetric and hold ore pairs,
//! shear halls open turned floors, Pixel Rot's rock is quantized deeper, the three landmarks are
//! carved, and older worlds have none of it.

use super::*;
use crate::{
    distortion::{WarpedTile, mirror::MirrorZone},
    worldgen::{
        DEFAULT_WORLD_SEED, GeneratorVersion, WorldSettings,
        descent::{landmarks::outpost_column, layout},
        streamed::generated_tile,
    },
};

fn settings(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(seed)
    }
}

fn kind_at(settings: &WorldSettings, x: i32, y: i32) -> TileKind {
    generated_tile(settings, TilePosition { x, y }).kind
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::Warped(WarpedTile::MirrorGlass) => '+',
        TileKind::Warped(WarpedTile::MirrorOre) => 'M',
        TileKind::Warped(WarpedTile::MirrorGhost) => 'm',
        TileKind::Warped(WarpedTile::ShearStone) => '%',
        TileKind::Warped(WarpedTile::ShearPlate) => '=',
        TileKind::Warped(WarpedTile::DataOre) => 'D',
        TileKind::Warped(WarpedTile::RotBlock) => '#',
        TileKind::Warped(WarpedTile::ScreenPixel) => '@',
        TileKind::Ore(_) => '$',
        TileKind::Stone => '.',
        TileKind::HardRock => ':',
        TileKind::Fluid(_) => '~',
        TileKind::Built(_) => 'B',
        TileKind::Cave(crate::cave::CaveTile::TreasureCache) => 'T',
        _ => '?',
    }
}

#[test]
#[ignore = "prints an ASCII dump of each distortion stratum and landmark for review"]
fn dump_distortion_strata() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let dump = |label: &str, x0: i32, y0: i32, width: i32, height: i32| {
        println!("{label} at ({x0}, {y0})");
        for y in y0..y0 + height {
            let line: String = (x0..x0 + width)
                .map(|x| glyph(kind_at(&settings, x, y)))
                .collect();
            println!("{line}");
        }
    };
    for stratum in [
        Stratum::MirrorStrata,
        Stratum::GravityShear,
        Stratum::PixelRot,
    ] {
        let (top, bottom) = layout.rows(stratum);
        for along in [150, 500, 850] {
            let row = top + (bottom - top) * along / 1_000;
            dump(&format!("{stratum:?} {along}"), -40, row - 20, 200, 40);
        }
    }
    if let Some(hall) = mirror::looking_glass_hall(&settings, &layout) {
        let b = hall.bounds.expanded(3);
        dump(
            "Looking-Glass Hall",
            b.min.x,
            b.min.y,
            b.width(),
            b.height(),
        );
    }
    if let Some(spire) = shear::spire(&settings, &layout) {
        let b = spire.expanded(3);
        dump("Shear Spire", b.min.x, b.min.y, b.width(), b.height());
    }
    if let Some(screen) = rot::dead_screen(&settings, &layout) {
        let b = screen.expanded(3);
        dump("Dead Screen", b.min.x, b.min.y, b.width(), b.height());
    }
}

#[test]
fn mirror_halls_are_symmetric_and_hold_ore_pairs() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::MirrorStrata);
    let area = TileRect {
        min: TilePosition { x: -900, y: top },
        max: TilePosition { x: 900, y: bottom },
    };
    let zones = mirror::zones_touching(&settings, &layout, &area);
    assert!(zones.len() >= 8, "{} halls", zones.len());
    let mut real = 0;
    let mut ghosts = 0;
    for zone in zones.iter().take(10) {
        let b = zone.bounds;
        for y in b.min.y..=b.max.y {
            for x in b.min.x..=b.max.x {
                let here = kind_at(&settings, x, y);
                let there = kind_at(&settings, zone.reflect_column(x), y);
                let open = |kind: TileKind| kind == TileKind::Air;
                assert_eq!(
                    open(here),
                    open(there),
                    "hall {b:?} at ({x}, {y}): {here:?} / {there:?}"
                );
                match (here, there) {
                    (
                        TileKind::Warped(WarpedTile::MirrorOre),
                        TileKind::Warped(WarpedTile::MirrorGhost),
                    ) => real += 1,
                    (
                        TileKind::Warped(WarpedTile::MirrorGhost),
                        TileKind::Warped(WarpedTile::MirrorOre),
                    ) => ghosts += 1,
                    (TileKind::Warped(WarpedTile::MirrorOre), other)
                    | (other, TileKind::Warped(WarpedTile::MirrorOre))
                        if other != TileKind::Warped(WarpedTile::MirrorGhost) =>
                    {
                        // A Descent landmark or the expedition can cover one side.
                        let _ = other;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(real > 4, "{real} real ore with their reflections");
    assert_eq!(real, ghosts, "every pair is one real, one reflection");
}

#[test]
fn halls_widen_and_crowd_toward_the_bottom() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::MirrorStrata);
    let third = (bottom - top) / 3;
    let band = |from: i32, to: i32| {
        let zones = mirror::zones_touching(
            &settings,
            &layout,
            &TileRect {
                min: TilePosition { x: -1_600, y: from },
                max: TilePosition { x: 1_600, y: to },
            },
        );
        let cells: Vec<MirrorZone> = zones.into_iter().filter(|zone| !zone.landmark).collect();
        let area: i32 = cells
            .iter()
            .map(|zone| zone.bounds.width() * zone.bounds.height())
            .sum();
        (cells.len(), area)
    };
    let (shallow, shallow_area) = band(top, top + third);
    let (deep, deep_area) = band(bottom - third, bottom);
    assert!(
        deep > shallow,
        "{shallow} halls at the top, {deep} at the bottom"
    );
    assert!(deep_area > shallow_area * 3 / 2);
    // Parallax Station's column stays plain.
    let column = outpost_column(&settings);
    let all = mirror::zones_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition {
                x: column - mirror::STATION_CLEARANCE,
                y: top,
            },
            max: TilePosition {
                x: column + mirror::STATION_CLEARANCE,
                y: bottom,
            },
        },
    );
    assert!(all.is_empty(), "{all:?}");
}

#[test]
fn the_looking_glass_hall_has_a_strongbox_and_its_reflection() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let hall = mirror::looking_glass_hall(&settings, &layout).expect("the hall");
    let b = hall.bounds;
    let floor = b.max.y - 2;
    let row: Vec<TileKind> = (b.min.x..=b.max.x)
        .map(|x| kind_at(&settings, x, floor))
        .collect();
    let caches = row
        .iter()
        .filter(|kind| **kind == TileKind::Cave(crate::cave::CaveTile::TreasureCache))
        .count();
    let ghosts = row
        .iter()
        .filter(|kind| **kind == TileKind::Warped(WarpedTile::MirrorGhost))
        .count();
    assert_eq!((caches, ghosts), (1, 1), "{row:?}");
    let open = (b.min.y..=b.max.y)
        .flat_map(|y| (b.min.x..=b.max.x).map(move |x| (x, y)))
        .filter(|(x, y)| kind_at(&settings, *x, *y) == TileKind::Air)
        .count();
    assert!(open > 600, "a great hall, {open} open tiles");
}

#[test]
fn shear_halls_open_turned_floors_with_plates() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let (top, bottom) = layout.rows(Stratum::GravityShear);
    let regions = shear::regions_touching(
        &settings,
        &layout,
        &TileRect {
            min: TilePosition { x: -900, y: top },
            max: TilePosition { x: 900, y: bottom },
        },
    );
    assert!(regions.len() >= 8);
    // Walls only near the top, ceilings further down.
    let shallow: Vec<u16> = regions
        .iter()
        .filter(|region| region.along < 340)
        .map(|region| region.turn)
        .collect();
    assert!(!shallow.is_empty() && shallow.iter().all(|turn| *turn == 90 || *turn == 270));
    assert!(regions.iter().any(|region| region.turn == 180));
    for region in regions.iter().take(6) {
        let b = region.bounds;
        let plates = (b.min.y..=b.max.y)
            .flat_map(|y| (b.min.x..=b.max.x).map(move |x| (x, y)))
            .filter(|(x, y)| kind_at(&settings, *x, *y) == TileKind::Warped(WarpedTile::ShearPlate))
            .count();
        assert!(plates >= 6, "{region:?}: {plates} plates");
        // The plates line the side gravity pulls toward.
        let (dx, dy) = region.down();
        let probe = match (dx, dy) {
            (-1, 0) => (b.min.x + 2, b.center().y),
            (1, 0) => (b.max.x - 2, b.center().y),
            (0, -1) => (b.center().x, b.min.y + 2),
            _ => (b.center().x, b.max.y - 2),
        };
        let floor = kind_at(&settings, probe.0, probe.1);
        assert!(
            matches!(floor, TileKind::Warped(WarpedTile::ShearPlate)) || floor == TileKind::Air,
            "{region:?} floor {floor:?}"
        );
    }
}

#[test]
fn the_shear_spire_is_hollow_with_a_cache_on_its_plinth() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let spire = shear::spire(&settings, &layout).expect("the spire");
    let centre = spire.center();
    let cache = kind_at(&settings, centre.x, centre.y - shear::SPIRE_PLINTH_HALF.1);
    assert_eq!(cache, TileKind::Cave(crate::cave::CaveTile::TreasureCache));
    assert_eq!(
        kind_at(&settings, spire.min.x + 6, spire.min.y + 6),
        TileKind::Air
    );
    // Down is the nearest wall.
    let (cx, cy) = (f64::from(centre.x), f64::from(centre.y));
    #[allow(clippy::cast_possible_truncation, reason = "tile coordinates")]
    let at = |x: f64, y: f64| shear::spire_down(&spire, x as f32, y as f32);
    let (left, _) = at(f64::from(spire.min.x) + 3.5, cy);
    assert!(left < 0.0);
    let (_, up) = at(cx, f64::from(spire.min.y) + 3.5);
    assert!(up < 0.0);
}

#[test]
fn pixel_rot_quantizes_its_rock_deeper() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let tops = rot::tier_tops(&layout);
    // In the deepest tier every block is one kind all through.
    let tier = rot::ROT_TIERS - 1;
    let size = rot::BLOCK_SIZES[tier];
    let mut checked = 0;
    let mut data = 0;
    for by in 1..6 {
        for bx in -30..30 {
            let x0 = bx * size;
            let y0 = tops[tier] + by * size;
            let Some(cell) = rot::block_at(&layout, x0, y0) else {
                continue;
            };
            if stratum_in(&settings, &layout, x0, y0) != Stratum::PixelRot
                || stratum_in(&settings, &layout, x0 + size - 1, y0 + size - 1) != Stratum::PixelRot
            {
                continue;
            }
            let kinds: Vec<TileKind> = cell
                .bounds
                .positions()
                .map(|at| kind_at(&settings, at.x, at.y))
                .collect();
            let first = kinds[0];
            if matches!(first, TileKind::Ore(_))
                || kinds.iter().any(|kind| matches!(kind, TileKind::Ore(_)))
            {
                continue;
            }
            if kinds.iter().all(|kind| *kind == first) {
                checked += 1;
                if first == TileKind::Warped(WarpedTile::DataOre) {
                    data += 1;
                }
            }
        }
    }
    assert!(checked > 200, "{checked} uniform blocks");
    assert!(data > 3, "{data} data blocks");
    // The top tier keeps the rock above (few warped tiles).
    let (top, _) = layout.rows(Stratum::PixelRot);
    assert_eq!(rot::tier_at(&layout, top), Some(0));
    assert!(rot::block_size(4) > rot::block_size(1));
}

#[test]
fn the_dead_screen_shows_its_picture() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let screen = rot::dead_screen(&settings, &layout).expect("the screen");
    let lit = screen
        .positions()
        .filter(|at| kind_at(&settings, at.x, at.y) == TileKind::Warped(WarpedTile::ScreenPixel))
        .count();
    assert!(lit > 60, "{lit} lit pixels");
    let gallery = kind_at(&settings, screen.center().x, screen.max.y - 4);
    assert_eq!(gallery, TileKind::Air);
}

#[test]
fn older_worlds_have_no_distortion() {
    let classic = WorldSettings::new(DEFAULT_WORLD_SEED);
    let layout = layout(&settings(DEFAULT_WORLD_SEED));
    let area = TileRect {
        min: TilePosition { x: 0, y: 0 },
        max: TilePosition { x: 100, y: 100 },
    };
    assert!(!touches_rows(&classic, &layout, &area));
    assert!(plan_for(&classic, &layout, &area).mirrors.is_empty());
}
