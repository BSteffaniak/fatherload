use super::*;
use crate::{
    terrain::{CHUNK_SIZE_TILES, ChunkPosition, Terrain},
    worldgen::{GeneratorVersion, WorldSettings, sky, streamed},
};

fn v9(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V9,
        ..WorldSettings::new(seed)
    }
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "compared against `tile_at`'s answers"
)]
const fn lunar(tile: LunarTile) -> Option<TileKind> {
    Some(TileKind::Lunar(tile))
}

fn glyph(kind: Option<TileKind>) -> char {
    match kind {
        None => ' ',
        Some(TileKind::Lunar(tile)) => match tile {
            LunarTile::Regolith => '.',
            LunarTile::MareDust => ',',
            LunarTile::Rock => '#',
            LunarTile::DeepRock => '%',
            LunarTile::Basalt => 'B',
            LunarTile::Ice => 'i',
            LunarTile::Helium3 => 'h',
            LunarTile::AlloyVein => 'A',
            LunarTile::RelicCache => 'R',
            LunarTile::CrystalStalk => '|',
            LunarTile::CrystalBloom => '*',
            LunarTile::ColonyHull => 'H',
            LunarTile::ColonyGlass => 'o',
            LunarTile::ColonyPad => '=',
            LunarTile::HabitatPanel => 'P',
            LunarTile::FacilityWall => 'F',
            LunarTile::FacilityGlyph => 'g',
            LunarTile::Veil => 'v',
            LunarTile::Pylon => 'Y',
            LunarTile::PylonLit => 'y',
            LunarTile::Gate => 'G',
            LunarTile::Conduit => 'C',
            LunarTile::CoreSeal => 'S',
            LunarTile::CoreShell => '@',
            LunarTile::CoreHeart => 'X',
        },
        Some(_) => '?',
    }
}

/// Prints a window of the Moon (`cargo test -p fatherload_simulation moon_ascii -- --ignored
/// --nocapture`).
#[test]
#[ignore = "a picture for a human"]
fn moon_ascii() {
    let layout = layout(&v9(7)).expect("a filled moon");
    let plan = layout.facility;
    for (label, x0, x1, y0, y1) in [
        (
            "colony",
            layout.colony.center_x - 45,
            layout.colony.center_x + 45,
            layout.colony.ground_row - 25,
            layout.colony.ground_row + 6,
        ),
        (
            "facility",
            plan.bounds().min.x - 2,
            plan.bounds().max.x + 2,
            plan.temple.ground_row - 10,
            layout.core.y + 16,
        ),
        (
            "tube",
            layout.tubes[0].first - 5,
            layout.tubes[0].first + 90,
            layout.moon.top_row - 20,
            layout.tubes[0].row + 8,
        ),
    ] {
        println!("--- {label}");
        for y in y0..=y1 {
            let line: String = (x0..=x1).map(|x| glyph(layout.tile_at(x, y))).collect();
            println!("{line}");
        }
    }
}

#[test]
fn only_v9_fills_the_moon_and_v8_keeps_its_placeholder() {
    let v8 = WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(5)
    };
    assert!(layout(&v8).is_none());
    let placeholder = sky::moon(&v8).expect("v8 has a moon");
    assert!(!placeholder.is_lunar());
    let filled = sky::moon(&v9(5)).expect("v9 has a moon");
    assert!(filled.is_lunar());
    // Same footprint: the Moon hangs in the same place over the same column.
    assert_eq!(filled.center_x, placeholder.center_x);
    assert_eq!(filled.half_width, placeholder.half_width);
    assert_eq!(filled.top_row, placeholder.top_row);
    let top = placeholder
        .surface_row(placeholder.center_x + 500)
        .expect("covered");
    assert!(matches!(
        placeholder.tile_at(placeholder.center_x + 500, top),
        Some(TileKind::Sky(_))
    ));
}

#[test]
fn layouts_are_deterministic_and_keep_their_places_apart() {
    for seed in 0..24_u64 {
        let settings = v9(seed);
        let first = layout(&settings).expect("filled");
        // A fresh build (not the memo) agrees.
        let again = build(&first.moon);
        assert_eq!(first, again, "seed {seed}");
        let moon = first.moon;
        let reach = |site: &Site| site.reach();
        let mut spans = vec![
            reach(&first.colony),
            (first.facility.bounds().min.x, first.facility.bounds().max.x),
        ];
        spans.extend(first.habitats.iter().map(reach));
        spans.extend(
            first
                .tubes
                .iter()
                .filter(|tube| tube.placed())
                .map(|tube| (tube.first, tube.last)),
        );
        for (index, (a0, a1)) in spans.iter().enumerate() {
            assert!(a0 <= a1, "seed {seed}");
            assert!(
                (a0 - moon.center_x).abs() <= moon.half_width
                    && (a1 - moon.center_x).abs() <= moon.half_width,
                "seed {seed}: inside the Moon"
            );
            for (b0, b1) in spans.iter().skip(index + 1) {
                assert!(a1 < b0 || b1 < a0, "seed {seed}: {a0}..{a1} vs {b0}..{b1}");
            }
        }
        // Every forest, ice field and tube found room.
        assert!(
            first
                .forests
                .iter()
                .chain(&first.ice_fields)
                .all(|span| span.last > span.first),
            "seed {seed}: {:?} {:?}",
            first.forests,
            first.ice_fields
        );
        assert!(first.tubes.iter().all(LavaTube::placed), "seed {seed}");
        // The colony and the facility are on opposite sides of the centre.
        assert!(
            (first.colony.center_x - moon.center_x).signum()
                != (first.facility.temple.center_x - moon.center_x).signum(),
            "seed {seed}"
        );
    }
}

#[test]
fn the_colony_stands_on_flat_ground_with_a_pad_a_dome_and_air_inside() {
    for seed in [1_u64, 2, 3, 99] {
        let layout = layout(&v9(seed)).expect("filled");
        let g = layout.colony.ground_row;
        let cx = layout.colony.center_x;
        for x in cx - COLONY_HALF_WIDTH..=cx + COLONY_HALF_WIDTH {
            assert_eq!(layout.surface_row(x), Some(g), "seed {seed}: flat at {x}");
        }
        let (pad_first, pad_last, pad_row) = layout.pad();
        for x in pad_first..=pad_last {
            assert_eq!(layout.tile_at(x, pad_row), lunar(LunarTile::ColonyPad));
        }
        // Room to land above the pad.
        for x in pad_first + 1..pad_last {
            for y in pad_row - 4..pad_row {
                assert_eq!(layout.tile_at(x, y), None, "seed {seed}: ({x}, {y})");
            }
        }
        // The dome: glass up top, open doors at its base, air inside, pressurized.
        assert_eq!(
            layout.tile_at(cx, g - DOME_RADIUS + 1),
            lunar(LunarTile::ColonyGlass)
        );
        assert_eq!(layout.tile_at(cx - DOME_RADIUS + 1, g - 1), None, "a door");
        assert_eq!(layout.tile_at(cx, g - 3), None);
        assert!(layout.colony_pressurized(cx, g - 3));
        assert!(!layout.colony_pressurized(cx, g - DOME_RADIUS - 2));
        assert!(
            layout
                .colony_area()
                .contains(TilePosition { x: cx, y: g - 3 })
        );
    }
}

#[test]
fn the_facility_runs_from_its_veiled_temple_to_the_core() {
    for seed in [1_u64, 4, 17, 0xBEEF] {
        let layout = layout(&v9(seed)).expect("filled");
        let plan = layout.facility;
        // A veil doorway (pilots only) into the temple room.
        assert_eq!(
            layout.tile_at(plan.door.x, plan.door.y),
            lunar(LunarTile::Veil)
        );
        assert_eq!(
            layout.tile_at(plan.door.x + 4 * plan.direction, plan.door.y - 1),
            lunar(LunarTile::Veil)
        );
        // The door leads into the open temple room.
        assert_eq!(
            layout.tile_at(plan.door.x + 5 * plan.direction, plan.door.y - 1),
            None
        );
        // Every room is open air except its puzzle pieces and ledges.
        for room in plan.rooms() {
            let open = room
                .positions()
                .filter(|p| layout.tile_at(p.x, p.y).is_none())
                .count();
            assert!(
                open * 10 >= room.positions().count() * 7,
                "seed {seed}: {room:?}"
            );
        }
        for pylon in plan.pylons {
            assert_eq!(layout.tile_at(pylon.x, pylon.y), lunar(LunarTile::Pylon));
            // On the hall floor.
            assert!(matches!(
                layout.tile_at(pylon.x, pylon.y + 1),
                Some(TileKind::Lunar(
                    LunarTile::FacilityWall | LunarTile::FacilityGlyph
                ))
            ));
        }
        let mut order = plan.order;
        order.sort_unstable();
        assert_eq!(order, [0, 1, 2]);
        assert_ne!(plan.order, [0, 1, 2]);
        for position in plan.gate.positions() {
            assert_eq!(
                layout.tile_at(position.x, position.y),
                lunar(LunarTile::Gate)
            );
        }
        for position in plan.seal.positions() {
            assert_eq!(
                layout.tile_at(position.x, position.y),
                lunar(LunarTile::CoreSeal)
            );
        }
        for conduit in plan.conduits {
            assert_eq!(
                layout.tile_at(conduit.x, conduit.y),
                lunar(LunarTile::Conduit)
            );
        }
        // The rooms lie under the surface (the facility is buried), the temple above it.
        assert!(
            plan.hall.min.y
                > layout
                    .surface(plan.hall.min.x)
                    .max(layout.surface(plan.hall.max.x))
        );
        // The core: a shell with the heart floating in an air chamber, entered from above.
        let core = layout.core;
        assert_eq!(layout.tile_at(core.x, core.y), lunar(LunarTile::CoreHeart));
        assert_eq!(layout.tile_at(core.x + 5, core.y), None);
        assert_eq!(
            layout.tile_at(core.x + CORE_SHELL_RADIUS - 1, core.y),
            lunar(LunarTile::CoreShell)
        );
        assert_eq!(
            layout.tile_at(core.x, core.y - CORE_CHAMBER_RADIUS - 2),
            lunar(LunarTile::Veil)
        );
        assert!(plan.core_shaft.contains(TilePosition {
            x: core.x,
            y: core.y - CORE_CHAMBER_RADIUS - 4
        }));
        // The corridor reaches the core shaft.
        assert!(plan.corridor.contains(TilePosition {
            x: core.x,
            y: plan.corridor.max.y
        }));
    }
}

#[test]
fn lava_tubes_open_to_the_sky_and_the_body_holds_lunar_riches() {
    let layout = layout(&v9(12)).expect("filled");
    for tube in layout.tubes {
        assert!(tube.last > tube.first, "every tube placed");
        let middle = i32::midpoint(tube.first, tube.last);
        assert_eq!(layout.tile_at(middle, tube.center_row(middle)), None);
        // A skylight from the surface down into it.
        let x = tube.first + 1;
        for y in layout.surface(x)..tube.center_row(x) {
            assert_eq!(layout.tile_at(x, y), None, "skylight at ({x}, {y})");
        }
        // Lined with basalt (or ice frozen on it).
        let below = layout.tile_at(middle, tube.center_row(middle) + 3);
        assert!(
            matches!(
                below,
                Some(TileKind::Lunar(
                    LunarTile::Basalt | LunarTile::Ice | LunarTile::RelicCache
                ))
            ),
            "{below:?}"
        );
    }
    let mut counts = std::collections::BTreeMap::new();
    let moon = layout.moon;
    for x in (moon.center_x - moon.half_width..moon.center_x + moon.half_width).step_by(3) {
        let Some(top) = layout.surface_row(x) else {
            continue;
        };
        for y in top..top + moon.depth + 20 {
            if let Some(TileKind::Lunar(tile)) = layout.tile_at(x, y) {
                *counts.entry(tile).or_insert(0_u32) += 1;
            }
        }
    }
    for tile in [
        LunarTile::Regolith,
        LunarTile::MareDust,
        LunarTile::Rock,
        LunarTile::DeepRock,
        LunarTile::Basalt,
        LunarTile::Ice,
        LunarTile::Helium3,
        LunarTile::AlloyVein,
        LunarTile::RelicCache,
    ] {
        assert!(
            counts.get(&tile).copied().unwrap_or(0) > 0,
            "{tile:?}: {counts:?}"
        );
    }
    let rock = counts[&LunarTile::Rock];
    assert!(counts[&LunarTile::Helium3] * 1_000 > rock, "{counts:?}");
    assert!(counts[&LunarTile::AlloyVein] * 2_000 > rock, "{counts:?}");
    assert!(counts[&LunarTile::AlloyVein] * 20 < rock, "{counts:?}");
    // Crystal forests grow trees with glowing blooms; habitats keep a relic cache.
    let mut blooms = 0;
    for forest in layout.forests {
        for x in forest.first..=forest.last {
            let top = layout.surface(x);
            for y in top - 12..top {
                if layout.tile_at(x, y) == lunar(LunarTile::CrystalBloom) {
                    blooms += 1;
                }
            }
        }
    }
    assert!(blooms > 10, "{blooms}");
    for habitat in layout.habitats {
        assert_eq!(
            layout.tile_at(habitat.center_x - 3, habitat.ground_row - 1),
            lunar(LunarTile::RelicCache)
        );
    }
}

#[test]
fn moon_chunks_match_the_layout_across_chunk_borders() {
    let settings = v9(31);
    let layout = layout(&settings).expect("filled");
    let terrain = Terrain::generate(&settings);
    let plan = layout.facility;
    let mut checked = 0;
    for area in [
        layout.colony_area(),
        plan.hall.expanded(3),
        plan.arena.expanded(3),
        TileRect::around(layout.core, CORE_SHELL_RADIUS + 2, CORE_SHELL_RADIUS + 2),
    ] {
        for position in area.positions() {
            let expected = layout
                .tile_at(position.x, position.y)
                .unwrap_or(TileKind::Air);
            assert_eq!(
                terrain.tile(position).map(|tile| tile.kind),
                Some(expected),
                "{position:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 3_000);
}

#[test]
fn v9_ground_is_exactly_v8() {
    // (V9 also furnishes the islands and continents, see `worldgen::sky::isle_tests` and
    // `worldgen::sky::continent::tests`; here the ground, also under the Moon's column.)
    let v8 = WorldSettings {
        generator: GeneratorVersion::V8,
        ..WorldSettings::new(0x5EED)
    };
    let v9 = v9(0x5EED);
    let moon = sky::moon(&v8).expect("a moon");
    let rows = [0, -3, 2];
    for cx in [
        -300,
        -1,
        0,
        7,
        250,
        moon.center_x.div_euclid(CHUNK_SIZE_TILES),
    ] {
        for cy in rows {
            let chunk = ChunkPosition { x: cx, y: cy };
            assert_eq!(
                streamed::generated_chunk(&v8, chunk).tiles(),
                streamed::generated_chunk(&v9, chunk).tiles(),
                "{chunk:?}"
            );
        }
    }
}

#[test]
fn chunk_digests_of_v9_moons_are_pinned() {
    // V9 = V8 + the filled Moon. Pinned once it ships: changing what V9 generates breaks V9
    // saves, so ship changes as a new generator version.
    let mut combined = 0_u64;
    for seed in [1_u64, 0xD1_11_6A_4E] {
        let settings = v9(seed);
        let layout = layout(&settings).expect("filled");
        let plan = layout.facility;
        let points = [
            TilePosition {
                x: layout.colony.center_x,
                y: layout.colony.ground_row - 4,
            },
            plan.hall.center(),
            plan.arena.center(),
            plan.shaft.center(),
            plan.core_shaft.center(),
            layout.core,
            TilePosition {
                x: layout.tubes[0].first,
                y: layout.tubes[0].row,
            },
            TilePosition {
                x: layout.moon.center_x + 600,
                y: layout.moon.top_row + 200,
            },
        ];
        for point in points {
            let chunk = ChunkPosition::of(point);
            for (index, tile) in streamed::generated_chunk(&settings, chunk)
                .tiles()
                .iter()
                .enumerate()
            {
                combined = combined
                    .rotate_left(7)
                    .wrapping_add(tile.code() ^ (index as u64).wrapping_mul(0x9E37));
            }
        }
    }
    assert_eq!(
        combined, GOLDEN_V9_DIGEST,
        "V9 generation changed: {combined:#018x}"
    );
}

const GOLDEN_V9_DIGEST: u64 = 0xabc9_4aa9_21c4_08e2;
