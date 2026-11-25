//! The buried city's generated places: avenues with tram lines and towers with lifts in the
//! Megacity, the Lantern Library and its core, engine halls whose machinery has room to move,
//! puzzle vaults that are well formed and carved, the Orrery's plinths, and older worlds
//! untouched.

use super::*;
use crate::{
    ancient_city::movers::{LIFT_CAB_TILES, PISTON_TILES, TRAM_CAR_ROWS, TRAM_CAR_TILES},
    mechanism::{REACH_COLUMNS, REACH_ROWS},
    worldgen::{
        DEFAULT_WORLD_SEED, GeneratorVersion, WorldSettings, descent::layout,
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

const fn solid(kind: TileKind) -> bool {
    !kind.is_open()
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => ' ',
        TileKind::City(tile) => match tile {
            CityTile::CityStone => '=',
            CityTile::TowerGlass => '|',
            CityTile::TramRail => '-',
            CityTile::Platform => '_',
            CityTile::StreetLamp => '*',
            CityTile::AlloyOre => 'a',
            CityTile::LiftRail => '!',
            CityTile::ArchivistCore => '@',
            CityTile::EngineSteel => '#',
            CityTile::ConveyorLeft => '<',
            CityTile::ConveyorRight => '>',
            CityTile::PistonHousing => 'H',
            CityTile::EngineCoreOre => 'e',
            CityTile::Pipe => '~',
            CityTile::ArchiveShelf => 'h',
            CityTile::ArchiveStone => '%',
            CityTile::StarGlass => 's',
            CityTile::ChartPlinth => 'P',
            CityTile::ChartPlinthRead => 'p',
        },
        TileKind::Cave(CaveTile::BuilderAlloy) => 'A',
        TileKind::Cave(CaveTile::PrismRed) => 'r',
        TileKind::Cave(CaveTile::PrismGreen) => 'g',
        TileKind::Cave(CaveTile::PrismBlue) => 'b',
        TileKind::Cave(CaveTile::TreasureCache | CaveTile::RelicCache) => 'T',
        TileKind::Built(BuiltTile::RuinWall) => 'R',
        TileKind::Built(BuiltTile::Gate) => 'G',
        TileKind::Built(BuiltTile::GateLintel(_)) => 'L',
        TileKind::Built(BuiltTile::Lever { .. }) => 'l',
        TileKind::Built(BuiltTile::PressurePlate { .. }) => 'o',
        TileKind::Built(BuiltTile::VentGrate) => 'V',
        TileKind::Built(_) => 'B',
        TileKind::Ore(_) => '$',
        TileKind::Stone | TileKind::HardRock | TileKind::Dirt | TileKind::Clay => '.',
        TileKind::Fluid(_) => 'w',
        _ => '?',
    }
}

fn dump(settings: &WorldSettings, label: &str, area: TileRect) {
    println!("{label} at {:?}", area.min);
    for y in area.min.y..=area.max.y {
        let line: String = (area.min.x..=area.max.x)
            .map(|x| glyph(kind_at(settings, x, y)))
            .collect();
        println!("{line}");
    }
}

#[test]
#[ignore = "prints an ASCII dump of the buried city's set pieces for review"]
fn dump_buried_city() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    for stratum in [
        Stratum::BuriedMegacity,
        Stratum::EngineHalls,
        Stratum::ArchiveOfStars,
    ] {
        let (top, bottom) = layout.rows(stratum);
        println!("{stratum:?}: rows {top}..{bottom}");
    }
    let levels = avenue_levels(&settings, &layout);
    println!("levels {levels:?}");
    if let Some(library) = lantern_library(&settings, &layout) {
        dump(&settings, "Lantern Library", library.bounds.expanded(6));
    }
    let probe = TileRect {
        min: TilePosition {
            x: -200,
            y: layout.rows(Stratum::BuriedMegacity).0,
        },
        max: TilePosition {
            x: 200,
            y: layout.rows(Stratum::ArchiveOfStars).1,
        },
    };
    let mut shown = std::collections::BTreeSet::new();
    for stamp in stamps_with(&settings, &layout, &probe) {
        let key = format!("{:?}", stamp.kind);
        if shown.insert(key.clone()) {
            dump(&settings, &key, stamp.bounds.expanded(2));
        }
    }
    if let Some(engine) = great_engine(&settings, &layout) {
        dump(&settings, "Great Engine", engine.bounds.expanded(2));
    }
    if let Some(orrery) = orrery(&settings, &layout) {
        dump(&settings, "Orrery", orrery.bounds.expanded(2));
    }
}

#[test]
fn vault_layouts_are_rectangular_and_every_gate_has_its_mechanism_in_reach() {
    for kind in VaultKind::ALL {
        let layout = vault_layout(kind);
        let width = layout[0].len();
        assert!(layout.iter().all(|row| row.len() == width), "{kind:?}");
        let cells: Vec<(i32, i32, u8)> = layout
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.bytes().enumerate().map(move |(x, c)| {
                    (
                        i32::try_from(x).unwrap_or(0),
                        i32::try_from(y).unwrap_or(0),
                        c,
                    )
                })
            })
            .collect();
        assert_eq!(
            cells.iter().filter(|(_, _, c)| *c == b'*').count(),
            1,
            "{kind:?} has one plinth"
        );
        for &(lx, ly, c) in &cells {
            let channel = match c {
                b'0' => b'L',
                b'1' => b'P',
                b'2' => b'Q',
                _ => continue,
            };
            assert!(
                cells.iter().any(|&(mx, my, m)| m == channel
                    && (mx - lx).abs() <= REACH_COLUMNS
                    && (my - ly).abs() <= REACH_ROWS),
                "{kind:?}: the gate at ({lx}, {ly}) has no {} in reach",
                channel as char
            );
            // A gate column hangs under each lintel down to the floor.
            assert_eq!(
                cells
                    .iter()
                    .find(|&&(x, y, _)| x == lx && y == ly + 1)
                    .map(|cell| cell.2),
                Some(b'G')
            );
        }
    }
}

fn probe_area(layout: &DescentLayout, stratum: Stratum, x0: i32, x1: i32) -> TileRect {
    let (top, bottom) = layout.rows(stratum);
    TileRect {
        min: TilePosition { x: x0, y: top },
        max: TilePosition { x: x1, y: bottom },
    }
}

#[test]
fn the_megacity_has_avenues_trams_and_towers_whose_lifts_have_room() {
    for seed in [DEFAULT_WORLD_SEED, 0x5EED, 0xC17E] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let levels = avenue_levels(&settings, &layout);
        assert!(levels.len() >= 6, "seed {seed}: {} levels", levels.len());
        let area = probe_area(&layout, Stratum::BuriedMegacity, -400, 400);
        let stamps = stamps_with(&settings, &layout, &area);
        let avenues = stamps
            .iter()
            .filter(|stamp| stamp.kind == CityStampKind::Avenue)
            .count();
        let towers = stamps
            .iter()
            .filter(|stamp| stamp.kind == CityStampKind::Tower)
            .count();
        assert!(
            avenues >= 20 && towers >= 8,
            "seed {seed}: {avenues} avenues, {towers} towers"
        );
        let movers = movers_with(&settings, &layout, &area);
        let trams: Vec<&CityMover> = movers
            .iter()
            .filter(|mover| matches!(mover, CityMover::Tram { .. }))
            .collect();
        let lifts: Vec<&CityMover> = movers
            .iter()
            .filter(|mover| matches!(mover, CityMover::Lift { .. }))
            .collect();
        assert!(trams.len() >= 6 && lifts.len() >= 6, "seed {seed}");
        // A tram car's whole run is open air over its rail or platforms.
        for tram in trams.iter().take(6) {
            let CityMover::Tram {
                rail_row, track, ..
            } = tram
            else {
                continue;
            };
            let first = i32::try_from(track.stops[0] / MILLI).unwrap_or(0);
            let last = i32::try_from(track.stops[track.stops.len() - 1] / MILLI).unwrap_or(0);
            for x in first - TRAM_CAR_TILES / 2..=last + TRAM_CAR_TILES / 2 {
                let under = kind_at(&settings, x, *rail_row);
                assert!(
                    matches!(
                        under,
                        TileKind::City(CityTile::TramRail | CityTile::Platform)
                    ) || under == TileKind::Air,
                    "seed {seed}: no rail under the line at ({x}, {rail_row}): {under:?}"
                );
                for y in rail_row - TRAM_CAR_ROWS..*rail_row {
                    assert_eq!(
                        kind_at(&settings, x, y),
                        TileKind::Air,
                        "seed {seed}: the tram's way is blocked at ({x}, {y})"
                    );
                }
            }
        }
        // A lift's cab has open air its whole way, and every stop has a floor beside it.
        for lift in lifts.iter().take(6) {
            let CityMover::Lift { column, track, .. } = lift else {
                continue;
            };
            let top = i32::try_from(track.stops[0] / MILLI).unwrap_or(0);
            let bottom = i32::try_from(track.stops[track.stops.len() - 1] / MILLI).unwrap_or(0);
            for x in column - LIFT_CAB_TILES / 2..=column + LIFT_CAB_TILES / 2 {
                for y in top - 3..bottom {
                    assert_eq!(
                        kind_at(&settings, x, y),
                        TileKind::Air,
                        "seed {seed}: the lift shaft at ({x}, {y}) is blocked"
                    );
                }
            }
            for stop in &track.stops {
                let row = i32::try_from(stop / MILLI).unwrap_or(0);
                let beside = [column - 2, column + 2]
                    .iter()
                    .any(|x| solid(kind_at(&settings, *x, row)));
                assert!(beside, "seed {seed}: no floor beside the stop at row {row}");
            }
        }
    }
}

#[test]
fn the_lantern_library_holds_the_archivist_core_and_its_lift_runs() {
    for seed in [DEFAULT_WORLD_SEED, 0x5EED, 0xC17E] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let library = lantern_library(&settings, &layout).expect("a library");
        let core = library_core(&library);
        assert_eq!(
            kind_at(&settings, core.x, core.y),
            TileKind::City(CityTile::ArchivistCore),
            "seed {seed}"
        );
        // It stands on a floor.
        assert!(solid(kind_at(&settings, core.x, core.y + 1)));
        let movers = movers_with(&settings, &layout, &library.bounds);
        assert!(
            movers
                .iter()
                .any(|mover| matches!(mover, CityMover::Lift { .. })),
            "seed {seed}: the library has its lift"
        );
        // And the Archive Post has the other core in its hall's ceiling.
        let post = post_core(&settings, &layout).expect("the post");
        assert_eq!(
            kind_at(&settings, post.x, post.y),
            TileKind::City(CityTile::ArchivistCore),
            "seed {seed}: the Archive Post's core"
        );
        assert_eq!(archivist_cores(&settings, &layout).len(), 2);
        assert!(layout.stratum_of_row(post.y) == Stratum::BuriedMegacity);
    }
}

#[test]
fn engine_machinery_has_room_to_move_and_is_on_the_beat() {
    for seed in [DEFAULT_WORLD_SEED, 0x5EED, 0xC17E] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let area = probe_area(&layout, Stratum::EngineHalls, -500, 500);
        let stamps = stamps_with(&settings, &layout, &area);
        let kinds: std::collections::BTreeSet<CityStampKind> =
            stamps.iter().map(|stamp| stamp.kind).collect();
        for kind in [
            CityStampKind::EngineHall,
            CityStampKind::PistonShaft,
            CityStampKind::CrusherLine,
        ] {
            assert!(kinds.contains(&kind), "seed {seed}: no {kind:?}");
        }
        let movers = movers_with(&settings, &layout, &area);
        let mut pistons = 0;
        let mut crushers = 0;
        for mover in &movers {
            match mover {
                CityMover::Piston {
                    column,
                    floor_row,
                    cycle,
                    ..
                } => {
                    pistons += 1;
                    // The head's housing at rest, and open air over the head's whole stroke
                    // plus a rig's height.
                    for x in column - PISTON_TILES / 2..=column + PISTON_TILES / 2 {
                        assert_eq!(
                            kind_at(&settings, x, *floor_row),
                            TileKind::City(CityTile::PistonHousing),
                            "seed {seed}"
                        );
                        for y in floor_row - cycle.travel - 2..*floor_row {
                            assert!(
                                kind_at(&settings, x, y).is_open()
                                    || matches!(
                                        kind_at(&settings, x, y),
                                        TileKind::Cave(_) | TileKind::Ore(_)
                                    ),
                                "seed {seed}: a piston at ({column}, {floor_row}) is blocked \
                                 at ({x}, {y}): {:?}",
                                kind_at(&settings, x, y)
                            );
                        }
                    }
                    assert!(cycle.period_millis >= 3_000 && cycle.period_millis <= 4_400);
                }
                CityMover::Crusher { .. } => crushers += 1,
                _ => {}
            }
        }
        assert!(
            pistons >= 10 && crushers >= 6,
            "seed {seed}: {pistons} pistons, {crushers} crushers"
        );
        assert!(great_engine(&settings, &layout).is_some());
    }
}

#[test]
fn the_archive_has_every_kind_of_vault_carved_whole_and_the_orrery_its_plinths() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in [DEFAULT_WORLD_SEED, 0x5EED, 0xC17E] {
        let settings = settings(seed);
        let layout = layout(&settings);
        let area = probe_area(&layout, Stratum::ArchiveOfStars, -700, 700);
        let stamps = stamps_with(&settings, &layout, &area);
        let mut vaults = 0;
        for stamp in &stamps {
            let CityStampKind::Vault(kind) = stamp.kind else {
                continue;
            };
            vaults += 1;
            seen.insert(kind);
            // Carved as authored: every masonry cell of the layout is in the world.
            let plinth = vault_plinth(stamp).expect("a plinth");
            assert_eq!(
                kind_at(&settings, plinth.x, plinth.y),
                TileKind::City(CityTile::ChartPlinth),
                "seed {seed}: {kind:?} at {:?}",
                stamp.bounds.min
            );
        }
        assert!(vaults >= 6, "seed {seed}: {vaults} vaults");
        let orrery = orrery(&settings, &layout).expect("an orrery");
        for plinth in orrery_plinths(&orrery) {
            assert_eq!(
                kind_at(&settings, plinth.x, plinth.y),
                TileKind::City(CityTile::ChartPlinth),
                "seed {seed}"
            );
            assert!(solid(kind_at(&settings, plinth.x, plinth.y + 1)));
        }
    }
    assert_eq!(seen.len(), VaultKind::ALL.len(), "{seen:?}");
}

#[test]
fn older_worlds_have_none_of_it() {
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..WorldSettings::new(DEFAULT_WORLD_SEED)
    };
    let v11 = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&v11);
    let area = probe_area(&layout, Stratum::BuriedMegacity, -100, 100);
    assert!(stamps_with(&classic, &layout, &area).is_empty());
    assert!(lantern_library(&classic, &layout).is_none());
    assert!(great_engine(&classic, &layout).is_none());
    assert!(orrery(&classic, &layout).is_none());
}

#[test]
fn alloy_portals_frame_the_way_in_under_the_seal() {
    let settings = settings(DEFAULT_WORLD_SEED);
    let layout = layout(&settings);
    let (low, high) = clear_columns(&settings);
    let mut portals = 0;
    for cell in -11..11 {
        let centre = cell * PORTAL_SPACING + PORTAL_SPACING / 2;
        if centre >= low - 6 && centre <= high + 6 {
            continue;
        }
        let pillars = (layout.rows(Stratum::PrismDesert).1 - 40
            ..layout.rows(Stratum::BuriedMegacity).0 + 60)
            .filter(|y| {
                kind_at(&settings, centre - 3, *y) == TileKind::Cave(CaveTile::BuilderAlloy)
                    && kind_at(&settings, centre + 3, *y) == TileKind::Cave(CaveTile::BuilderAlloy)
                    && kind_at(&settings, centre, *y) == TileKind::Air
            })
            .count();
        if pillars >= 5 {
            portals += 1;
        }
    }
    assert!(portals >= 15, "{portals} portals");
}
