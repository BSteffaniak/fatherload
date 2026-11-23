//! The distortion strata's pure rules: tile codes, the state's holds and cooldowns, the rot
//! tiers by depth and the halls' reflections.

use super::*;

#[test]
fn warped_tiles_round_trip_their_codes() {
    for tile in WarpedTile::ALL {
        assert_eq!(WarpedTile::from_code(tile.code()), Some(tile));
        assert!(tile.hardness() >= 2 && tile.durability() >= 1);
    }
    assert_eq!(WarpedTile::from_code(99), None);
    // A reflection reads as the real thing by name; only real ore pays.
    assert_eq!(WarpedTile::MirrorGhost.name(), WarpedTile::MirrorOre.name());
    assert!(WarpedTile::MirrorOre.item().is_some());
    assert!(WarpedTile::MirrorGhost.item().is_none());
}
