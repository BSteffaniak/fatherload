//! Descent VIII's generated places: veins and halls, wells, the metal sea and its rails, the
//! cable and the Heart's chamber.

use super::*;
use crate::worldgen::{GeneratorVersion, WorldSettings, descent::layout, streamed::generated_tile};

const SEED: u64 = 0x4EA7_7E57;

fn settings() -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::DEEP_DESCENT,
        ..WorldSettings::new(SEED)
    }
}

/// The generated world (tiles come from the generator, chunk by chunk).
struct Gen(WorldSettings);

fn terrain() -> Gen {
    Gen(settings())
}

fn kind_at(terrain: &Gen, x: i32, y: i32) -> TileKind {
    generated_tile(&terrain.0, TilePosition { x, y }).kind
}

fn glyph(kind: TileKind) -> char {
    match kind {
        TileKind::Air => '.',
        TileKind::Fluid(_) => '~',
        TileKind::Core(CoreTile::MagRail) => '=',
        TileKind::Core(CoreTile::MagSwitch) => 'S',
        TileKind::Core(CoreTile::MagBooster) => '>',
        TileKind::Core(CoreTile::RailPylon) => '|',
        TileKind::Core(CoreTile::HeartShell) => '#',
        TileKind::Core(CoreTile::HeartSeal) => '+',
        TileKind::Core(CoreTile::Cable) => 'C',
        TileKind::Core(CoreTile::Singularity) => '@',
        TileKind::Core(CoreTile::EventGlass) => 'o',
        TileKind::Core(CoreTile::DarkMatter) => 'd',
        TileKind::Core(CoreTile::CoreMetal) => 'm',
        TileKind::Core(CoreTile::StarShard) => '*',
        _ => ' ',
    }
}

/// Prints a map of `rect` (for looking at the places while tuning; `--nocapture`).
#[allow(dead_code)]
fn print_map(terrain: &Gen, rect: TileRect) {
    for y in rect.min.y..=rect.max.y {
        let row: String = (rect.min.x..=rect.max.x)
            .map(|x| glyph(kind_at(terrain, x, y)))
            .collect();
        println!("{y:6} {row}");
    }
}

#[test]
fn the_metal_sea_lies_under_rails_that_cross_it_with_switches_and_a_jump() {
    let settings = settings();
    let layout = layout(&settings);
    let terrain = terrain();
    let (upper, lower) = rail_rows(&settings, &layout);
    let basin = basin_at(&settings, 600);
    // The upper line runs the whole basin and through the next pillar.
    for x in basin.start..basin.start + BASIN_PERIOD + 30 {
        let kind = kind_at(&terrain, x, upper);
        assert!(
            matches!(kind, TileKind::Core(tile) if tile.is_mag_rail())
                || x == landmarks::lift_column(&settings),
            "a rail at {x}: {kind:?}"
        );
        // Head room over it.
        assert!(
            kind_at(&terrain, x, upper - 1).is_open(),
            "head room at {x}"
        );
    }
    let (west, east) = basin.junctions;
    assert_eq!(
        kind_at(&terrain, west, upper),
        TileKind::Core(CoreTile::MagSwitch)
    );
    assert_eq!(
        kind_at(&terrain, east, upper),
        TileKind::Core(CoreTile::MagSwitch)
    );
    // The low line: down the ramp, along, the jump, along one row lower, up the ramp.
    assert!(
        matches!(kind_at(&terrain, west + 1, upper + 1), TileKind::Core(tile) if tile.is_mag_rail())
    );
    assert!(matches!(
        kind_at(&terrain, basin.gap - 1, lower),
        TileKind::Core(CoreTile::MagBooster)
    ));
    for x in basin.gap..basin.gap + 3 {
        assert!(kind_at(&terrain, x, lower).is_open() && kind_at(&terrain, x, lower + 1).is_open());
    }
    assert!(matches!(
        kind_at(&terrain, basin.gap + 3, lower + 1),
        TileKind::Core(CoreTile::MagRail)
    ));
    // Liquid metal under the sea's surface, open air over it.
    let (_, sea, floor) = ocean_rows(&settings, &layout, west + 40);
    assert!(matches!(
        kind_at(&terrain, west + 40, sea + 2),
        TileKind::Fluid(crate::fluid::FluidKind::LiquidMetal)
    ));
    assert!(matches!(
        kind_at(&terrain, west + 40, floor - 1),
        TileKind::Fluid(_)
    ));
    assert!(kind_at(&terrain, west + 40, sea - 1).is_open());
    // Pylons stand in the metal.
    let pylons = (basin.start..basin.start + BASIN_PERIOD)
        .filter(|x| kind_at(&terrain, *x, sea + 4) == TileKind::Core(CoreTile::RailPylon))
        .count();
    assert!(pylons >= 8, "{pylons} pylons");
}

#[test]
fn wells_are_round_caverns_round_a_singularity_ringed_with_event_glass() {
    let settings = settings();
    let layout = layout(&settings);
    let terrain = terrain();
    let (top, bottom) = layout.rows(Stratum::SingularityFields);
    let area = TileRect {
        min: TilePosition { x: -400, y: top },
        max: TilePosition { x: 400, y: bottom },
    };
    let wells = wells_touching(&settings, &layout, &area);
    assert!(wells.len() >= 20, "{} wells", wells.len());
    for well in wells.iter().take(6) {
        let c = well.centre;
        assert_eq!(
            kind_at(&terrain, c.x, c.y),
            TileKind::Core(CoreTile::Singularity)
        );
        assert!(kind_at(&terrain, c.x + well.radius - 1, c.y).is_open());
        assert!(kind_at(&terrain, c.x, c.y - well.radius + 1).is_open());
        let glass = (-well.radius - 2..=well.radius + 2)
            .filter(|dx| {
                kind_at(&terrain, c.x + dx, c.y - well.radius - 1)
                    == TileKind::Core(CoreTile::EventGlass)
            })
            .count();
        let ring = (0..8)
            .map(|k| {
                let (x, y) = [
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                    (-1, -1),
                ][k];
                (c.x + x * (well.radius + 1), c.y + y * (well.radius + 1))
            })
            .filter(|(x, y)| kind_at(&terrain, *x, *y) == TileKind::Core(CoreTile::EventGlass))
            .count();
        assert!(glass + ring > 0, "event glass round the well at {c:?}");
    }
    // The Lens is the biggest.
    let lens = lens(&settings, &layout);
    assert!(wells.iter().all(|well| well.radius <= lens.radius));
}

#[test]
fn the_heart_chamber_stands_at_the_cables_root_sealed_by_the_heart_key() {
    let settings = settings();
    let layout = layout(&settings);
    let terrain = terrain();
    let centre = heart_centre(&settings, &layout);
    assert_eq!(centre.x, landmarks::lift_column(&settings));
    // Open round the drive (the core is drawn and fought, not rock), a bowl floor, the shell.
    assert!(kind_at(&terrain, centre.x, centre.y).is_open());
    assert!(kind_at(&terrain, centre.x + 10, centre.y).is_open());
    assert_eq!(
        kind_at(&terrain, centre.x, centre.y + FLOOR_BELOW_TILES + 1),
        TileKind::Core(CoreTile::HeartShell)
    );
    assert_eq!(
        kind_at(&terrain, centre.x - CHAMBER_RADIUS - 1, centre.y - 6),
        TileKind::Core(CoreTile::HeartShell)
    );
    // The seals: over the top and in both sides at the floor.
    assert_eq!(
        kind_at(&terrain, centre.x, centre.y - CHAMBER_RADIUS - 1),
        TileKind::Core(CoreTile::HeartSeal)
    );
    let side = (CHAMBER_RADIUS - 6..=CHAMBER_RADIUS + SHELL_TILES + 1).any(|dx| {
        kind_at(&terrain, centre.x + dx, centre.y + FLOOR_BELOW_TILES - 2)
            == TileKind::Core(CoreTile::HeartSeal)
    });
    assert!(side, "a side seal");
    // The cable runs down to it from Heart's Edge through the rock.
    let (top, bottom) = cable_rows(&settings, &layout);
    assert!(bottom > top + 600, "{top}..{bottom}");
    let cable = (top..=bottom)
        .filter(|y| kind_at(&terrain, centre.x, *y) == TileKind::Core(CoreTile::Cable))
        .count();
    let rows = usize::try_from(bottom - top).unwrap_or(0);
    assert!(
        cable > rows / 2,
        "the cable is mostly there ({cable} of {rows})"
    );
    // Star shards round it.
    let shards = (centre.x - 40..centre.x + 40)
        .flat_map(|x| (centre.y - 40..centre.y - 27).map(move |y| (x, y)))
        .filter(|(x, y)| kind_at(&terrain, *x, *y) == TileKind::Core(CoreTile::StarShard))
        .count();
    assert!(shards > 10, "{shards} star shards");
}

#[test]
fn dark_matter_runs_in_veins_and_hangs_from_the_weightless_halls_roofs() {
    let settings = settings();
    let layout = layout(&settings);
    let terrain = terrain();
    let (top, bottom) = layout.rows(Stratum::DarkMatterVeins);
    let mut dark = 0;
    let mut rock = 0;
    for y in (top + 40..bottom - 40).step_by(7) {
        for x in -150..150 {
            match kind_at(&terrain, x, y) {
                TileKind::Core(CoreTile::DarkMatter) => dark += 1,
                TileKind::Core(CoreTile::VeinShale | CoreTile::NullStone) => rock += 1,
                _ => {}
            }
        }
    }
    assert!(
        dark > 100 && rock > 1_000,
        "{dark} dark matter, {rock} rock"
    );
    let area = TileRect {
        min: TilePosition { x: -600, y: top },
        max: TilePosition { x: 600, y: bottom },
    };
    let halls = halls_touching(&settings, &layout, &area);
    assert!(halls.len() >= 4, "{} halls", halls.len());
    // Most halls hang dark matter from their roofs (one whose roof a cave or another set
    // piece opened has none; Wave 10 integration: the first hall is no longer always whole).
    let roofed = halls
        .iter()
        .filter(|hall| {
            assert!(kind_at(&terrain, hall.centre.x, hall.centre.y).is_open());
            let roof = (hall.centre.x - 4..=hall.centre.x + 4)
                .filter(|x| {
                    (1..=3).any(|up| {
                        kind_at(&terrain, *x, hall.centre.y - hall.half_height - up)
                            == TileKind::Core(CoreTile::DarkMatter)
                    })
                })
                .count();
            roof >= 2
        })
        .count();
    assert!(
        roofed * 2 >= halls.len(),
        "dark matter on {roofed} of {} halls' roofs",
        halls.len()
    );
    // The Upfall.
    let shaft = upfall(&settings, &layout);
    assert!(kind_at(&terrain, shaft.center().x - 6, shaft.center().y).is_open());
}

#[test]
fn story_rooms_keep_off_the_set_pieces() {
    let settings = settings();
    let layout = layout(&settings);
    assert!(crosses_set_pieces(
        &settings,
        &layout,
        &heart_chamber(&settings, &layout)
    ));
    assert!(crosses_set_pieces(
        &settings,
        &layout,
        &upfall(&settings, &layout)
    ));
    let (ceiling, _, _) = ocean_rows(&settings, &layout, 0);
    let in_sea = TileRect::around(
        TilePosition {
            x: 0,
            y: ceiling + 8,
        },
        4,
        3,
    );
    assert!(crosses_set_pieces(&settings, &layout, &in_sea));
    // Older worlds have none of it.
    let classic = WorldSettings::new(SEED);
    assert!(!crate::heart::crosses_set_pieces(&classic, &in_sea));
}
