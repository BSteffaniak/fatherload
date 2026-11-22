//! The Old Deep's placement: every stratum gets its features, the set pieces stand where the
//! tick looks for them, and older worlds get none of it.

use std::collections::BTreeMap;

use crate::{
    descent::Stratum,
    old_deep::DeepTile,
    terrain::{ChunkPosition, TileKind, TilePosition},
    worldgen::{GeneratorVersion, WorldSettings, descent::layout, streamed::generated_chunk},
};

use super::*;

fn v11(seed: u64) -> WorldSettings {
    WorldSettings {
        generator: GeneratorVersion::V11,
        ..WorldSettings::new(seed)
    }
}

/// Every Old Deep tile kind generated in the middle third of `stratum` over 192 columns.
fn deep_tiles(settings: &WorldSettings, stratum: Stratum) -> BTreeMap<DeepTile, usize> {
    let (top, bottom) = layout(settings).rows(stratum);
    let third = (bottom - top) / 3;
    let mut found = BTreeMap::new();
    for cy in (top + third).div_euclid(16)..=(bottom - third).div_euclid(16) {
        for cx in -6..6 {
            for tile in generated_chunk(settings, ChunkPosition { x: cx, y: cy }).tiles() {
                if let TileKind::Deep(deep) = tile.kind {
                    *found.entry(deep).or_insert(0) += 1;
                }
            }
        }
    }
    found
}

#[test]
fn every_old_deep_stratum_generates_its_own_materials() {
    let settings = v11(0x01D_DEE9);
    let expect: [(Stratum, &[DeepTile]); 7] = [
        (
            Stratum::CrystalFaults,
            &[
                DeepTile::ResonantCrystal,
                DeepTile::Prism,
                DeepTile::LatentGem,
            ],
        ),
        (
            Stratum::FossilOceans,
            &[DeepTile::Shale, DeepTile::FossilBone],
        ),
        (
            Stratum::PressureCathedrals,
            &[DeepTile::NaveStone, DeepTile::JetVent],
        ),
        (
            Stratum::RadioactiveHollow,
            &[DeepTile::GlowRock, DeepTile::IsotopeCluster],
        ),
        (
            Stratum::AncientMachineLayer,
            &[DeepTile::MachineCasing, DeepTile::PistonHousing],
        ),
        (Stratum::VoidGeodeFields, &[DeepTile::GeodeShell]),
        (
            Stratum::MantleStormZone,
            &[DeepTile::CinderRock, DeepTile::StormVent],
        ),
    ];
    for (stratum, tiles) in expect {
        let found = deep_tiles(&settings, stratum);
        for tile in tiles {
            assert!(
                found.contains_key(tile),
                "{stratum:?} has no {tile:?}: {found:?}"
            );
        }
    }
    // Nothing of the Old Deep in Descent I or III.
    assert!(deep_tiles(&settings, Stratum::GreyStone).is_empty());
    assert!(deep_tiles(&settings, Stratum::RootLabyrinth).is_empty());
}

fn tile_of(settings: &WorldSettings, at: TilePosition) -> TileKind {
    let chunk = ChunkPosition {
        x: at.x.div_euclid(16),
        y: at.y.div_euclid(16),
    };
    let index = usize::try_from(at.y.rem_euclid(16) * 16 + at.x.rem_euclid(16)).unwrap_or(0);
    generated_chunk(settings, chunk).tiles()[index].kind
}

#[test]
fn the_set_pieces_stand_where_the_tick_finds_them() {
    let settings = v11(0x0CA7_ED2A);
    let layout = layout(&settings);
    let site = cathedral(&settings, &layout).expect("a cathedral");
    for valve in site.valves {
        assert_eq!(
            tile_of(&settings, valve),
            TileKind::Deep(DeepTile::LiftValve),
            "{valve:?}"
        );
    }
    assert!(site.bottom_station > site.top_station + 200);
    let arena = star_core_arena(&settings, &layout).expect("an arena");
    assert_eq!(
        tile_of(&settings, arena.core),
        TileKind::Deep(DeepTile::FirstCoreShielded)
    );
    for pylon in arena.pylons {
        assert_eq!(
            tile_of(&settings, pylon),
            TileKind::Deep(DeepTile::CorePylon)
        );
    }
    for stratum in Stratum::CrystalFaults.descent().strata() {
        let cache = expedition_camp(&settings, &layout, stratum).expect("a camp");
        assert_eq!(
            tile_of(&settings, cache),
            TileKind::Deep(DeepTile::ExpeditionLog),
            "{stratum:?}"
        );
    }
    // Older worlds: none of it.
    let classic = WorldSettings {
        generator: GeneratorVersion::V10,
        ..settings
    };
    assert!(cathedral(&classic, &layout).is_none());
    assert!(star_core_arena(&classic, &layout).is_none());
}
