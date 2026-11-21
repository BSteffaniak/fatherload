//! Side-view tomb cultures (WP-TOMBS, T28): which builders' tile set a side-view tomb is built
//! in, from where the site is.
//!
//! | Culture | Sites | Masonry set ([`crate::base::BuiltTile::TombWall`]) |
//! |---|---|---|
//! | [`TombCulture::Sandstone`] | mesa tombs (desert and badlands) | 0 (T27's sandstone, code 72) |
//! | [`TombCulture::Catacomb`] | the deep strata and villages' old quarters | 2 (code 120) |
//! | [`TombCulture::SkyTemple`] | sky continents | 3 (code 121) |
//! | [`TombCulture::LunarVault`] | the Moon | 4 (code 122) |
//!
//! The realizer writes the culture's masonry and pillars into every floor; the game paints
//! each set with its own art, back wall and torches, and the entrance outside (always
//! masonry set 0 on the V10 map, so the generator's output stays pinned) is painted in the
//! culture of the site it belongs to.

use serde::{Deserialize, Serialize};

use super::SiteSetting;

/// A side-view tomb's builders (see the module docs).
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum TombCulture {
    /// Sun-baked sandstone with carved glyph bands (mesa tombs).
    #[default]
    Sandstone,
    /// Grey bone-and-mortar catacombs with skull niches (deep strata, old quarters).
    Catacomb,
    /// Pale marble and gold of a sky temple (sky continents).
    SkyTemple,
    /// Dark alloy panels and cold light of a lunar vault (the Moon).
    LunarVault,
}

impl TombCulture {
    pub const ALL: [Self; 4] = [
        Self::Sandstone,
        Self::Catacomb,
        Self::SkyTemple,
        Self::LunarVault,
    ];

    /// The culture of a site in `setting`.
    #[must_use]
    pub const fn of_setting(setting: SiteSetting) -> Self {
        match setting {
            SiteSetting::Mesa => Self::Sandstone,
            SiteSetting::Deep | SiteSetting::OldQuarter => Self::Catacomb,
            SiteSetting::SkyContinent => Self::SkyTemple,
            SiteSetting::Moon => Self::LunarVault,
        }
    }

    /// The masonry set of [`crate::base::BuiltTile::TombWall`] and `TombPillar`.
    #[must_use]
    pub const fn masonry(self) -> u8 {
        match self {
            Self::Sandstone => 0,
            Self::Catacomb => 2,
            Self::SkyTemple => 3,
            Self::LunarVault => 4,
        }
    }

    /// The culture a masonry set shows (set 1, the crypts' stone, has none: `None`).
    #[must_use]
    pub const fn of_masonry(set: u8) -> Option<Self> {
        match set {
            0 => Some(Self::Sandstone),
            2 => Some(Self::Catacomb),
            3 => Some(Self::SkyTemple),
            4 => Some(Self::LunarVault),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sandstone => "Sandstone Tomb",
            Self::Catacomb => "Catacomb",
            Self::SkyTemple => "Sky Temple Vault",
            Self::LunarVault => "Lunar Vault",
        }
    }

    /// Whether the culture's sluices pour water rather than sand (the rising-flood trap).
    #[must_use]
    pub const fn floods_with_water(self) -> bool {
        matches!(self, Self::SkyTemple | Self::LunarVault)
    }
}

/// The offset of a T28 masonry set past the first two (codes 120 + offset).
#[must_use]
pub const fn masonry_extra(set: u8) -> u8 {
    let extra = set.saturating_sub(2);
    if extra > 2 { 2 } else { extra }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::BuiltTile;

    #[test]
    fn masonry_sets_round_trip_through_tile_codes() {
        for culture in TombCulture::ALL {
            let set = culture.masonry();
            assert_eq!(TombCulture::of_masonry(set), Some(culture));
            for tile in [BuiltTile::TombWall(set), BuiltTile::TombPillar(set)] {
                assert_eq!(BuiltTile::from_code(tile.code()), Some(tile), "{tile:?}");
            }
        }
        // T27's codes did not move.
        assert_eq!(BuiltTile::TombWall(0).code(), 72);
        assert_eq!(BuiltTile::TombWall(1).code(), 73);
        assert_eq!(BuiltTile::TombPillar(1).code(), 75);
        assert_eq!(TombCulture::of_masonry(1), None);
    }

    #[test]
    fn every_setting_has_a_culture() {
        assert_eq!(
            TombCulture::of_setting(SiteSetting::Mesa),
            TombCulture::Sandstone
        );
        assert_eq!(
            TombCulture::of_setting(SiteSetting::Moon),
            TombCulture::LunarVault
        );
        assert_eq!(
            TombCulture::of_setting(SiteSetting::SkyContinent),
            TombCulture::SkyTemple
        );
        assert_eq!(
            TombCulture::of_setting(SiteSetting::Deep),
            TombCulture::Catacomb
        );
    }
}
