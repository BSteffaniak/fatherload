//! Tile art for the exotic worlds (Wave 13 planets-d, `fatherload_simulation::exotic`): the gas
//! giant, the tidally locked world and the ring world, and their open ground's backs.
//!
//! * The gas giant: soft cream ammonia puffs, ochre storm cloud in swirls, blue haze in
//!   streaks, sooty carbon cloud; glowing gas pockets (cyan, violet, teal), diamond hail and
//!   silver metallic hydrogen; the sky cities' plated decks, riveted hulls, glowing lift
//!   bladders between ribs, lilac glass towers with lit windows and lanterns; the Eye's
//!   churning wall. Its open gas below the cloud tops is a haze that darkens band by band.
//! * The locked world: cracked hardpan, salt crust, sun glass, banded mesa stone; nitrogen
//!   ice, frozen regolith, ice spikes; lichen soil and glowmoss; vent rock streaked sulphur,
//!   frost-veined cryo rock and the vents' mouths; fractured rock, fault rubble and the dark
//!   magnetic lodestone.
//! * The ring world: lilac ring dust, breccia clasts, banded ring ice, ringstone, meteorite
//!   with its crystal lattice, the Tether's plate, live and dead conduits and their junctions,
//!   the dense core, monoliths and impact glass.
//! * Shared: builder plate, beacons, the bosses' seals and trophies, relics.

#![allow(
    clippy::match_same_arms,
    clippy::if_same_then_else,
    clippy::comparison_chain,
    clippy::bool_to_int_with_if,
    reason = "pixel art reads best one shade per case"
)]

use fatherload_simulation::{
    exotic::{
        ExoticStyle, ExoticTile, gas_giant,
        locked::{self, Side},
        rules::{Zone, zone_at},
        style_of_ground,
    },
    terrain::{Terrain, TileKind, TilePosition},
};

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use crate::rendering::pixel_art::palette::*;
use crate::rendering::pixel_art::shade::{PaletteIndex, bayer4, index_of};

use super::{
    Cell, Edges, Ramp, Strata, TILE_ART, TileCanvas, hash, noise, paint_edges, ramp, worley,
};

// The gas giant.
const CLOUD: Ramp = ramp([FOG, MIST, BONE, CREAM]);
const STORM: Ramp = ramp([RUST, COPPER, AMBER, CREAM]);
const HAZE: Ramp = ramp([DEEP_BLUE, OCEAN, SLATE_LIGHT, FOG]);
const SOOT: Ramp = ramp([INK, NIGHT, SLATE_DARK, SLATE]);
const AMMONIA: Ramp = ramp([SKY, SKY_LIGHT, ICE, BONE]);
const CHARGED: Ramp = ramp([PLUM, VIOLET, LAVENDER, BONE]);
const DEEP_GAS: Ramp = ramp([TEAL_DARK, TEAL, MINT, ICE]);
const METAL: Ramp = ramp([STEEL_DARK, STEEL, STEEL_LIGHT, BONE]);
const DECK: Ramp = ramp([SLATE, STEEL, STEEL_LIGHT, BONE]);
const HULL: Ramp = ramp([SLATE_DARK, SLATE, STEEL, STEEL_LIGHT]);
const BLADDER: Ramp = ramp([TEAL, MINT, ICE, BONE]);
const TOWER: Ramp = ramp([PLUM, VIOLET, LAVENDER, PINK]);
const LANTERN: Ramp = ramp([AMBER, GOLD, CREAM, BONE]);
const WALL: Ramp = ramp([SLATE_DARK, SLATE, FOG, MIST]);
// The locked world.
const HARDPAN: Ramp = ramp([SOIL, CLAY, SAND, CREAM]);
const SALT: Ramp = ramp([FOG, MIST, BONE, CREAM]);
const SUNGLASS: Ramp = ramp([RUST, ORANGE, AMBER, GOLD]);
const MESA: Ramp = ramp([MAROON, RUST, CLAY, SAND]);
const NITROGEN: Ramp = ramp([OCEAN, SKY_LIGHT, ICE, BONE]);
const REGOLITH: Ramp = ramp([SLATE_DARK, SLATE, FOG, MIST]);
const SPIKE: Ramp = ramp([SKY, SKY_LIGHT, ICE, BONE]);
const LICHEN: Ramp = ramp([SOIL_DARK, SOIL, MOSS, TEAL]);
const GLOWMOSS: Ramp = ramp([PINE, MOSS, MINT, ICE]);
const VENT: Ramp = ramp([STONE_DARK, STONE, AMBER, GOLD]);
const CRYO: Ramp = ramp([NIGHT, SLATE_DARK, SKY, ICE]);
const FRACTURE: Ramp = ramp([NIGHT, STONE_DARK, STONE, STONE_LIGHT]);
const RUBBLE: Ramp = ramp([STONE_DARK, STONE, STONE_LIGHT, FOG]);
const LODESTONE: Ramp = ramp([INK, STEEL_DARK, RUST, STEEL]);
// The ring world.
const DUST: Ramp = ramp([SLATE, FOG, MIST, BONE]);
const BRECCIA: Ramp = ramp([STONE_DARK, STONE, CLAY, STONE_LIGHT]);
const RING_ICE: Ramp = ramp([OCEAN, SKY_LIGHT, ICE, BONE]);
const RINGSTONE: Ramp = ramp([SLATE_DARK, SLATE, SLATE_LIGHT, LAVENDER]);
const METEORITE: Ramp = ramp([INK, STEEL_DARK, STEEL, STEEL_LIGHT]);
const TETHER: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, MINT]);
const CONDUIT: Ramp = ramp([NIGHT, STEEL_DARK, SKY, ICE]);
const DENSE: Ramp = ramp([INK, NIGHT, PLUM, VIOLET]);
const MONOLITH: Ramp = ramp([SLATE_DARK, SLATE, SLATE_LIGHT, ICE]);
const IMPACT: Ramp = ramp([NIGHT, TEAL_DARK, TEAL, MINT]);
// Shared.
const BUILDER: Ramp = ramp([NIGHT, STEEL_DARK, STEEL, STEEL_LIGHT]);
const SEAL: Ramp = ramp([PLUM, VIOLET, GOLD, CREAM]);

type Color = raylib::prelude::Color;

/// Representative colour of an exotic tile (drill dust, debris, the map).
#[must_use]
pub const fn exotic_color(tile: ExoticTile) -> Color {
    match tile {
        ExoticTile::CloudIce | ExoticTile::SaltCrust | ExoticTile::RingDust => MIST,
        ExoticTile::StormCloud => AMBER,
        ExoticTile::DeepHaze => OCEAN,
        ExoticTile::SootCloud | ExoticTile::DenseRock => NIGHT,
        ExoticTile::AmmoniaPocket | ExoticTile::IceSpike | ExoticTile::RingIce => SKY_LIGHT,
        ExoticTile::StormPocket | ExoticTile::CityTower | ExoticTile::BossSeal => VIOLET,
        ExoticTile::DeepPocket | ExoticTile::LiftBladder | ExoticTile::BuilderBeacon => MINT,
        ExoticTile::DiamondHail | ExoticTile::NitrogenIce => ICE,
        ExoticTile::MetallicHydrogen | ExoticTile::SkyDeck | ExoticTile::Meteorite => STEEL,
        ExoticTile::CityHull | ExoticTile::BuilderPlate | ExoticTile::TetherPlate => STEEL_DARK,
        ExoticTile::CityLantern | ExoticTile::StormTrophy | ExoticTile::GloamTrophy => GOLD,
        ExoticTile::EyeWall | ExoticTile::FrostRegolith | ExoticTile::Ringstone => SLATE,
        ExoticTile::Hardpan => SAND,
        ExoticTile::SunGlass => ORANGE,
        ExoticTile::MesaStone => RUST,
        ExoticTile::LichenSoil | ExoticTile::Glowmoss => MOSS,
        ExoticTile::VentRock => STONE,
        ExoticTile::CryoRock => SLATE_DARK,
        ExoticTile::GeyserVent | ExoticTile::FractureRock => STONE_DARK,
        ExoticTile::FaultRubble | ExoticTile::Breccia => STONE_LIGHT,
        ExoticTile::Lodestone => STEEL_DARK,
        ExoticTile::ConduitLive | ExoticTile::ConduitJunction => SKY,
        ExoticTile::ConduitDead => STEEL_DARK,
        ExoticTile::Monolith => SLATE_LIGHT,
        ExoticTile::ImpactGlass | ExoticTile::AnchorTrophy => TEAL,
        ExoticTile::RelicCache | ExoticTile::LandmarkRelic => AMBER,
    }
}

/// The light an exotic tile gives (halos, the light overlay's tint).
#[must_use]
#[allow(dead_code, reason = "the round ground renderer's halos")]
pub const fn exotic_glow(tile: ExoticTile) -> Option<Color> {
    match tile {
        ExoticTile::CityLantern | ExoticTile::StormTrophy | ExoticTile::GloamTrophy => Some(GOLD),
        ExoticTile::LiftBladder | ExoticTile::DeepPocket | ExoticTile::BuilderBeacon => Some(MINT),
        ExoticTile::AmmoniaPocket | ExoticTile::ConduitLive | ExoticTile::ConduitJunction => {
            Some(SKY_LIGHT)
        }
        ExoticTile::StormPocket | ExoticTile::BossSeal => Some(LAVENDER),
        ExoticTile::Glowmoss | ExoticTile::AnchorTrophy => Some(MINT),
        ExoticTile::DiamondHail => Some(ICE),
        _ => None,
    }
}

/// The exotic world and zone at `position` (`None` on any other ground).
#[must_use]
pub(super) fn exotic_place(
    terrain: &Terrain,
    position: TilePosition,
) -> Option<(ExoticStyle, Zone, Side)> {
    let ground = terrain.planet_ground()?;
    let style = style_of_ground(ground)?;
    let side = locked::side_of(ground, i64::from(position.x));
    Some((style, zone_at(ground, style, position), side))
}

/// The rock ore and finds sit in at `position` of an exotic world.
#[must_use]
pub(super) const fn host(style: ExoticStyle, zone: Zone, side: Side) -> TileKind {
    let index = match zone {
        Zone::Surface | Zone::Skyring => 0,
        Zone::Stratum(index) => index,
        Zone::CoreChamber => 3,
    };
    TileKind::Exotic(match (style, index, side) {
        (ExoticStyle::GasGiant, 0, _) => ExoticTile::CloudIce,
        (ExoticStyle::GasGiant, 1, _) => ExoticTile::StormCloud,
        (ExoticStyle::GasGiant, 2, _) => ExoticTile::DeepHaze,
        (ExoticStyle::GasGiant, _, _) => ExoticTile::SootCloud,
        (ExoticStyle::Locked, 0, Side::Night) => ExoticTile::NitrogenIce,
        (ExoticStyle::Locked, 0, Side::Twilight) => ExoticTile::MesaStone,
        (ExoticStyle::Locked, 0, Side::Day) => ExoticTile::Hardpan,
        (ExoticStyle::Locked, 1, Side::Night) => ExoticTile::CryoRock,
        (ExoticStyle::Locked, 1, _) => ExoticTile::VentRock,
        (ExoticStyle::Locked, 2, _) => ExoticTile::FractureRock,
        (ExoticStyle::Locked, _, _) => ExoticTile::Lodestone,
        (ExoticStyle::Ring, 0, _) => ExoticTile::RingDust,
        (ExoticStyle::Ring, 1, _) => ExoticTile::Breccia,
        (ExoticStyle::Ring, 2, _) => ExoticTile::Ringstone,
        (ExoticStyle::Ring, _, _) => ExoticTile::DenseRock,
    })
}

/// The back wall's body and shadow behind an exotic world's open ground.
#[must_use]
pub(super) const fn back_pair(style: ExoticStyle, zone: Zone, side: Side) -> [PaletteIndex; 2] {
    let deep = matches!(zone, Zone::Stratum(2 | 3) | Zone::CoreChamber);
    match style {
        ExoticStyle::GasGiant => match zone {
            Zone::Surface | Zone::Skyring | Zone::Stratum(0) => [index_of(MIST), index_of(FOG)],
            Zone::Stratum(1) => [index_of(COPPER), index_of(RUST)],
            Zone::Stratum(2) => [index_of(OCEAN), index_of(DEEP_BLUE)],
            _ => [index_of(NIGHT), index_of(INK)],
        },
        ExoticStyle::Locked => match (side, deep) {
            (Side::Day, false) => [index_of(SOIL), index_of(SOIL_DARK)],
            (Side::Twilight, false) => [index_of(SOIL_DARK), index_of(PINE)],
            (Side::Night, false) => [index_of(SLATE_DARK), index_of(NIGHT)],
            (_, true) => [index_of(STONE_DARK), index_of(INK)],
        },
        ExoticStyle::Ring => {
            if deep {
                [index_of(PLUM), index_of(INK)]
            } else {
                [index_of(SLATE_DARK), index_of(NIGHT)]
            }
        }
    }
}

/// The highlight and the shade of an exotic tile's lips.
#[must_use]
pub(super) const fn lip_and_shade(tile: ExoticTile) -> (PaletteIndex, PaletteIndex) {
    let ramp = body_ramp(tile);
    (ramp[3], ramp[0])
}

const fn body_ramp(tile: ExoticTile) -> Ramp {
    match tile {
        ExoticTile::CloudIce => CLOUD,
        ExoticTile::StormCloud => STORM,
        ExoticTile::DeepHaze => HAZE,
        ExoticTile::SootCloud | ExoticTile::DiamondHail => SOOT,
        ExoticTile::AmmoniaPocket => AMMONIA,
        ExoticTile::StormPocket => CHARGED,
        ExoticTile::DeepPocket => DEEP_GAS,
        ExoticTile::MetallicHydrogen => METAL,
        ExoticTile::SkyDeck => DECK,
        ExoticTile::CityHull => HULL,
        ExoticTile::LiftBladder => BLADDER,
        ExoticTile::CityTower => TOWER,
        ExoticTile::CityLantern => LANTERN,
        ExoticTile::EyeWall => WALL,
        ExoticTile::Hardpan => HARDPAN,
        ExoticTile::SaltCrust => SALT,
        ExoticTile::SunGlass => SUNGLASS,
        ExoticTile::MesaStone => MESA,
        ExoticTile::NitrogenIce => NITROGEN,
        ExoticTile::FrostRegolith => REGOLITH,
        ExoticTile::IceSpike => SPIKE,
        ExoticTile::LichenSoil => LICHEN,
        ExoticTile::Glowmoss => GLOWMOSS,
        ExoticTile::VentRock | ExoticTile::GeyserVent => VENT,
        ExoticTile::CryoRock => CRYO,
        ExoticTile::FractureRock => FRACTURE,
        ExoticTile::FaultRubble => RUBBLE,
        ExoticTile::Lodestone => LODESTONE,
        ExoticTile::RingDust => DUST,
        ExoticTile::Breccia => BRECCIA,
        ExoticTile::RingIce => RING_ICE,
        ExoticTile::Ringstone => RINGSTONE,
        ExoticTile::Meteorite => METEORITE,
        ExoticTile::TetherPlate => TETHER,
        ExoticTile::ConduitLive | ExoticTile::ConduitDead | ExoticTile::ConduitJunction => CONDUIT,
        ExoticTile::DenseRock => DENSE,
        ExoticTile::Monolith => MONOLITH,
        ExoticTile::ImpactGlass => IMPACT,
        ExoticTile::BuilderPlate | ExoticTile::BuilderBeacon => BUILDER,
        ExoticTile::BossSeal
        | ExoticTile::StormTrophy
        | ExoticTile::GloamTrophy
        | ExoticTile::AnchorTrophy => SEAL,
        ExoticTile::RelicCache | ExoticTile::LandmarkRelic => LANTERN,
    }
}

fn dither(x: i32, y: i32) -> f32 {
    f32::from(bayer4(x, y)) / 40.0
}

/// A soft puff of cloud: bright tops, shaded bellies (worley blobs).
fn puff(ramp: Ramp, x: i32, y: i32, cell: i32, seed: u32) -> PaletteIndex {
    let (near, _, dx, dy) = worley(x, y, cell, seed, false);
    let lit = noise(x, y, 5, seed + 1) + dither(x, y) * 0.5;
    if near > cell as f32 * 0.62 && lit < 0.55 {
        ramp[0]
    } else if dy > 1 || (dx > 2 && dy > 0) {
        ramp[1]
    } else if dy < -2 && lit > 0.45 {
        ramp[3]
    } else {
        ramp[2]
    }
}

/// A glowing bubble of gas in its cloud.
fn bubble(ramp: Ramp, cloud: Ramp, px: i32, py: i32, x: i32, y: i32) -> PaletteIndex {
    let (dx, dy) = (px * 2 - 15, py * 2 - 15);
    let r2 = dx * dx + dy * dy;
    if r2 <= 20 {
        ramp[3]
    } else if r2 <= 70 {
        if dx + dy < -4 { ramp[3] } else { ramp[2] }
    } else if r2 <= 150 {
        ramp[1]
    } else if r2 <= 190 {
        ramp[0]
    } else {
        puff(cloud, x, y, 7, 3101)
    }
}

/// One art pixel of `tile`'s body at absolute art pixel `(x, y)` (tile pixel `(px, py)`).
#[allow(clippy::too_many_lines, reason = "one arm per material")]
#[allow(
    clippy::cast_possible_truncation,
    reason = "noise offsets are a few pixels"
)]
fn body(tile: ExoticTile, x: i32, y: i32, px: i32, py: i32) -> PaletteIndex {
    match tile {
        // --- The gas giant ---------------------------------------------------------------
        ExoticTile::CloudIce => puff(CLOUD, x, y, 7, 3001),
        ExoticTile::StormCloud => {
            // Ochre swirls: bands bent by eddies.
            let swirl = (noise(x, y, 18, 3011) * 9.0) as i32;
            let band = (y * 2 + x / 3 + swirl).rem_euclid(11);
            if band == 0 {
                STORM[0]
            } else if band < 3 {
                STORM[1]
            } else if band == 7 && noise(x, y, 6, 3012) > 0.5 {
                STORM[3]
            } else {
                STORM[2]
            }
        }
        ExoticTile::DeepHaze => {
            // Long horizontal streaks of packed haze.
            let wave = (noise(x, y, 24, 3021) * 4.0) as i32;
            let streak = (y + wave).rem_euclid(5);
            if streak == 0 {
                HAZE[0]
            } else if streak == 2 && noise(x, y, 9, 3022) > 0.5 {
                HAZE[3]
            } else if noise(x, y, 7, 3023) + dither(x, y) > 0.7 {
                HAZE[2]
            } else {
                HAZE[1]
            }
        }
        ExoticTile::SootCloud => {
            if hash(x, y, 3031).is_multiple_of(53) {
                index_of(STEEL_LIGHT)
            } else {
                puff(SOOT, x, y, 6, 3032)
            }
        }
        ExoticTile::AmmoniaPocket => bubble(AMMONIA, CLOUD, px, py, x, y),
        ExoticTile::StormPocket => {
            // A violet bubble with a crackle across it.
            if (px - py).abs() <= 0 && (3..13).contains(&px) && !hash(px, py, 3041).is_multiple_of(3) {
                index_of(BONE)
            } else {
                bubble(CHARGED, STORM, px, py, x, y)
            }
        }
        ExoticTile::DeepPocket => bubble(DEEP_GAS, HAZE, px, py, x, y),
        ExoticTile::DiamondHail => {
            // A cut stone on soot: facets in ice and white.
            let (dx, dy) = ((px - 8).abs(), py - 8);
            let inside = dx + dy.abs() <= 6;
            if !inside {
                puff(SOOT, x, y, 6, 3051)
            } else if dy < -2 {
                index_of(BONE)
            } else if dx + dy.abs() == 6 {
                index_of(SKY)
            } else if (px + py).rem_euclid(3) == 0 {
                index_of(SKY_LIGHT)
            } else {
                index_of(ICE)
            }
        }
        ExoticTile::MetallicHydrogen => {
            // Liquid silver: smooth ripples with a bright sheen.
            let ripple = ((noise(x, y, 12, 3061) * 10.0) as i32 + y).rem_euclid(6);
            match ripple {
                0 => METAL[0],
                1 | 5 => METAL[1],
                3 => METAL[3],
                _ => METAL[2],
            }
        }
        ExoticTile::SkyDeck => {
            // Plates with rivets and a bright top lip.
            if py == 0 {
                DECK[3]
            } else if py == TILE_ART - 1 || px.rem_euclid(8) == 7 {
                DECK[0]
            } else if (px.rem_euclid(8) == 1 || px.rem_euclid(8) == 5) && (py == 2 || py == 12) {
                DECK[3]
            } else if py < 4 {
                DECK[2]
            } else {
                DECK[1]
            }
        }
        ExoticTile::CityHull => {
            let (gx, gy) = (x.rem_euclid(12), y.rem_euclid(8));
            if gy == 0 || gx == 0 {
                HULL[0]
            } else if gy == 1 && gx % 3 == 1 {
                HULL[3]
            } else if noise(x, y, 8, 3071) + dither(x, y) > 0.75 {
                HULL[2]
            } else {
                HULL[1]
            }
        }
        ExoticTile::LiftBladder => {
            // A glowing gas cell: membrane veins over a bright core.
            let vein = (x + (noise(x, y, 10, 3081) * 6.0) as i32).rem_euclid(9) == 0;
            let glow = noise(x, y, 6, 3082);
            if vein {
                BLADDER[0]
            } else if glow > 0.7 {
                BLADDER[3]
            } else if glow > 0.4 {
                BLADDER[2]
            } else {
                BLADDER[1]
            }
        }
        ExoticTile::CityTower => {
            // Lilac glass with a grid of lit windows.
            let (gx, gy) = (px.rem_euclid(5), py.rem_euclid(6));
            if gx == 0 || gy == 0 {
                TOWER[0]
            } else if (2..=3).contains(&gx) && (2..=4).contains(&gy) {
                if hash(x.div_euclid(5), y.div_euclid(6), 3091).is_multiple_of(3) {
                    index_of(GOLD)
                } else {
                    TOWER[3]
                }
            } else if gx == 1 {
                TOWER[2]
            } else {
                TOWER[1]
            }
        }
        ExoticTile::CityLantern => {
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            if r2 <= 30 {
                LANTERN[3]
            } else if r2 <= 80 {
                LANTERN[2]
            } else if r2 <= 150 {
                LANTERN[1]
            } else {
                TOWER[0]
            }
        }
        ExoticTile::EyeWall => {
            // Churning storm: spiral streaks of dark and light cloud.
            let swirl = (noise(x, y, 16, 3101) * 12.0) as i32;
            let band = (x * 2 - y + swirl).rem_euclid(10);
            if band < 2 {
                WALL[0]
            } else if band == 5 {
                WALL[3]
            } else if band < 5 {
                WALL[1]
            } else {
                WALL[2]
            }
        }
        // --- The locked world ------------------------------------------------------------
        ExoticTile::Hardpan => {
            // Polygons of baked clay split by dark cracks.
            let (near, second, dx, dy) = worley(x, y, 7, 3201, false);
            if second - near < 1.0 {
                HARDPAN[0]
            } else if dy < -1 && dx < 1 {
                HARDPAN[3]
            } else if noise(x, y, 5, 3202) + dither(x, y) > 0.8 {
                HARDPAN[1]
            } else {
                HARDPAN[2]
            }
        }
        ExoticTile::SaltCrust => {
            let (gx, gy) = (x.rem_euclid(6), y.rem_euclid(4));
            if (gx == 5 && gy < 3) || hash(x, y, 3211).is_multiple_of(41) {
                SALT[0]
            } else if gy == 0 {
                SALT[3]
            } else if noise(x, y, 6, 3212) > 0.6 {
                SALT[2]
            } else {
                SALT[1]
            }
        }
        ExoticTile::SunGlass => {
            let (near, second, dx, dy) = worley(x, y, 6, 3221, false);
            if second - near < 0.9 {
                SUNGLASS[3]
            } else if dx + dy < -2 {
                SUNGLASS[2]
            } else if dx + dy > 2 {
                SUNGLASS[0]
            } else {
                SUNGLASS[1]
            }
        }
        ExoticTile::MesaStone => {
            let fold = (noise(x, y, 30, 3231) * 4.0) as i32;
            let band = (y + fold).div_euclid(3);
            match hash(band, 0, 3232) % 4 {
                0 => MESA[1],
                1 => MESA[3],
                2 if hash(x, y, 3233).is_multiple_of(4) => MESA[0],
                _ => MESA[2],
            }
        }
        ExoticTile::NitrogenIce => {
            // Smooth pale ice with long cracks and a pink blush.
            let crack = (x + y / 2 + (noise(x, y, 20, 3241) * 8.0) as i32).rem_euclid(17) == 0;
            if crack {
                NITROGEN[0]
            } else if noise(x, y, 14, 3242) > 0.7 {
                index_of(PINK)
            } else if noise(x, y, 6, 3243) + dither(x, y) > 0.75 {
                NITROGEN[3]
            } else {
                NITROGEN[2]
            }
        }
        ExoticTile::FrostRegolith => {
            let (near, _, dx, dy) = worley(x, y, 4, 3251, true);
            if near > 2.6 {
                REGOLITH[0]
            } else if dx + dy < -1 {
                REGOLITH[3]
            } else if hash(x, y, 3252).is_multiple_of(9) {
                index_of(ICE)
            } else {
                REGOLITH[1 + usize::from(dx < 0)]
            }
        }
        ExoticTile::IceSpike => {
            if px < 5 {
                SPIKE[3]
            } else if px > 11 {
                SPIKE[0]
            } else if (py + px).rem_euclid(7) == 0 {
                SPIKE[3]
            } else {
                SPIKE[2 - usize::from(px > 8)]
            }
        }
        ExoticTile::LichenSoil => {
            if noise(x, y, 4, 3261) + dither(x, y) > 0.85 {
                LICHEN[3]
            } else if noise(x, y, 7, 3262) > 0.62 {
                LICHEN[2]
            } else if hash(x, y, 3263).is_multiple_of(11) {
                LICHEN[0]
            } else {
                LICHEN[1]
            }
        }
        ExoticTile::Glowmoss => {
            if hash(x, y, 3271).is_multiple_of(13) {
                GLOWMOSS[3]
            } else if py < 3 || noise(x, y, 4, 3272) > 0.6 {
                GLOWMOSS[2]
            } else if py < 8 {
                GLOWMOSS[1]
            } else {
                LICHEN[1]
            }
        }
        ExoticTile::VentRock => {
            // Dark rock streaked with sulphur.
            let streak = (x - y * 2 + (noise(x, y, 14, 3281) * 6.0) as i32).rem_euclid(13);
            if streak == 0 {
                VENT[3]
            } else if streak == 1 {
                VENT[2]
            } else if noise(x, y, 5, 3282) + dither(x, y) > 0.75 {
                VENT[1]
            } else {
                VENT[0]
            }
        }
        ExoticTile::CryoRock => {
            let vein = (x + y + (noise(x, y, 12, 3291) * 8.0) as i32).rem_euclid(15) == 0;
            if vein {
                CRYO[3]
            } else if noise(x, y, 9, 3292) > 0.72 {
                CRYO[2]
            } else if noise(x, y, 5, 3293) + dither(x, y) > 0.7 {
                CRYO[1]
            } else {
                CRYO[0]
            }
        }
        ExoticTile::GeyserVent => {
            // A mouth of crusted rock round a dark throat.
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            if r2 <= 36 {
                index_of(INK)
            } else if r2 <= 80 {
                VENT[2]
            } else if r2 <= 140 && (px + py).rem_euclid(2) == 0 {
                VENT[3]
            } else {
                VENT[1]
            }
        }
        ExoticTile::FractureRock => {
            let crack = (x * 3 + y + (noise(x, y, 16, 3301) * 10.0) as i32).rem_euclid(23) == 0;
            if crack {
                FRACTURE[3]
            } else if noise(x, y, 6, 3302) + dither(x, y) > 0.75 {
                FRACTURE[2]
            } else {
                FRACTURE[1]
            }
        }
        ExoticTile::FaultRubble => {
            let (near, second, dx, dy) = worley(x, y, 5, 3311, false);
            if second - near < 0.8 {
                index_of(INK)
            } else if dy < -1 {
                RUBBLE[3]
            } else if dx > 1 {
                RUBBLE[0]
            } else {
                RUBBLE[1 + usize::from(dy < 1)]
            }
        }
        ExoticTile::Lodestone => {
            // Dark metal with rust and field lines.
            let line = (y * 2 + (noise(x, y, 18, 3321) * 10.0) as i32).rem_euclid(9) == 0;
            if line {
                LODESTONE[3]
            } else if noise(x, y, 7, 3322) > 0.7 {
                LODESTONE[2]
            } else if noise(x, y, 4, 3323) + dither(x, y) > 0.8 {
                LODESTONE[1]
            } else {
                LODESTONE[0]
            }
        }
        // --- The ring world --------------------------------------------------------------
        ExoticTile::RingDust => {
            if hash(x, y, 3401).is_multiple_of(29) {
                DUST[3]
            } else if noise(x, y, 8, 3402) + dither(x, y) > 0.8 {
                DUST[2]
            } else if noise(x, y, 5, 3403) < 0.25 {
                DUST[0]
            } else {
                DUST[1]
            }
        }
        ExoticTile::Breccia => {
            // Angular clasts of rose and grey in a dark matrix.
            let (near, second, dx, _) = worley(x, y, 5, 3411, true);
            if second - near < 1.0 {
                BRECCIA[0]
            } else if hash(x.div_euclid(5), y.div_euclid(5), 3412).is_multiple_of(3) {
                BRECCIA[2]
            } else if dx < 0 {
                BRECCIA[3]
            } else {
                BRECCIA[1]
            }
        }
        ExoticTile::RingIce => {
            let band = (y + (noise(x, y, 22, 3421) * 5.0) as i32).rem_euclid(7);
            if band == 0 {
                index_of(SLATE_LIGHT)
            } else if band == 3 {
                RING_ICE[3]
            } else if noise(x, y, 6, 3422) + dither(x, y) > 0.8 {
                RING_ICE[1]
            } else {
                RING_ICE[2]
            }
        }
        ExoticTile::Ringstone => {
            let (near, _, dx, dy) = worley(x, y, 6, 3431, false);
            if near > 4.2 {
                RINGSTONE[0]
            } else if dx + dy < -2 {
                RINGSTONE[3]
            } else if dx + dy > 2 {
                RINGSTONE[1]
            } else {
                RINGSTONE[2]
            }
        }
        ExoticTile::Meteorite => {
            // Iron with the crossing lattice of a cut meteorite.
            let lattice = (x + y).rem_euclid(6) == 0 || (x - y).rem_euclid(6) == 0;
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            if dx * dx + dy * dy > 210 {
                RINGSTONE[1]
            } else if lattice {
                METEORITE[3]
            } else if dx + dy < -6 {
                METEORITE[2]
            } else {
                METEORITE[1]
            }
        }
        ExoticTile::TetherPlate => {
            // Builder plate: panels with a glowing seam down the middle.
            if px == 7 || px == 8 {
                if (y.rem_euclid(8)) < 6 {
                    TETHER[3]
                } else {
                    TETHER[0]
                }
            } else if py.rem_euclid(8) == 0 || px == 0 || px == 15 {
                TETHER[0]
            } else if px < 4 {
                TETHER[2]
            } else {
                TETHER[1]
            }
        }
        ExoticTile::ConduitLive | ExoticTile::ConduitDead | ExoticTile::ConduitJunction => {
            // A cable in its casing; live, its core glows (a junction is a box with a node).
            let live = tile != ExoticTile::ConduitDead;
            let junction = tile == ExoticTile::ConduitJunction;
            if junction && (2..14).contains(&px) && (2..14).contains(&py) {
                let (dx, dy) = (px * 2 - 15, py * 2 - 15);
                if dx * dx + dy * dy <= 40 {
                    index_of(BONE)
                } else if px == 2 || px == 13 || py == 2 || py == 13 {
                    index_of(STEEL_LIGHT)
                } else {
                    index_of(STEEL)
                }
            } else if (6..=9).contains(&py) {
                if live && (py == 7 || py == 8) {
                    if (x.rem_euclid(6)) < 2 {
                        index_of(ICE)
                    } else {
                        index_of(SKY)
                    }
                } else {
                    index_of(STEEL_DARK)
                }
            } else if py == 5 || py == 10 {
                index_of(STEEL)
            } else {
                RINGSTONE[usize::from(noise(x, y, 5, 3441) > 0.6)]
            }
        }
        ExoticTile::DenseRock => {
            if hash(x, y, 3451).is_multiple_of(37) {
                DENSE[3]
            } else if noise(x, y, 6, 3452) + dither(x, y) > 0.78 {
                DENSE[2]
            } else if noise(x, y, 9, 3453) > 0.5 {
                DENSE[1]
            } else {
                DENSE[0]
            }
        }
        ExoticTile::Monolith => {
            if px == 0 || px == 15 {
                MONOLITH[0]
            } else if px < 4 {
                MONOLITH[3]
            } else if (y.rem_euclid(13)) == 0 {
                MONOLITH[1]
            } else {
                MONOLITH[2]
            }
        }
        ExoticTile::ImpactGlass => {
            let (near, second, dx, dy) = worley(x, y, 5, 3461, false);
            if second - near < 0.8 {
                IMPACT[3]
            } else if dx + dy < -1 {
                IMPACT[2]
            } else if near > 3.0 {
                IMPACT[0]
            } else {
                IMPACT[1]
            }
        }
        // --- Shared ----------------------------------------------------------------------
        ExoticTile::BuilderPlate => {
            if py.rem_euclid(8) == 0 || px.rem_euclid(8) == 0 {
                BUILDER[0]
            } else if py.rem_euclid(8) == 4 && px.rem_euclid(8) > 2 {
                index_of(TEAL)
            } else if px.rem_euclid(8) == 1 {
                BUILDER[3]
            } else {
                BUILDER[2]
            }
        }
        ExoticTile::BuilderBeacon => {
            let (dx, dy) = (
                (px - 7).abs() + (px - 8).abs(),
                (py - 7).abs() + (py - 8).abs(),
            );
            let d = dx / 2 + dy / 2;
            if d <= 2 {
                index_of(BONE)
            } else if d <= 4 {
                index_of(MINT)
            } else if d <= 6 {
                index_of(TEAL)
            } else {
                BUILDER[1]
            }
        }
        ExoticTile::BossSeal => {
            // A ring of glyphs round a glowing eye.
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            if r2 <= 16 {
                SEAL[3]
            } else if r2 <= 50 {
                SEAL[2]
            } else if r2 <= 120 {
                if (px + py * 3).rem_euclid(5) == 0 {
                    SEAL[3]
                } else {
                    SEAL[1]
                }
            } else if r2 <= 200 {
                SEAL[0]
            } else {
                index_of(INK)
            }
        }
        ExoticTile::StormTrophy | ExoticTile::GloamTrophy | ExoticTile::AnchorTrophy => {
            let (dx, dy) = (px * 2 - 15, py * 2 - 15);
            let r2 = dx * dx + dy * dy;
            let (core, rim) = match tile {
                ExoticTile::StormTrophy => (SKY_LIGHT, GOLD),
                ExoticTile::GloamTrophy => {
                    if px < 8 {
                        (GOLD, ORANGE)
                    } else {
                        (ICE, SKY)
                    }
                }
                _ => (MINT, STEEL_LIGHT),
            };
            if r2 <= 24 {
                index_of(BONE)
            } else if r2 <= 90 {
                index_of(core)
            } else if r2 <= 150 {
                index_of(rim)
            } else {
                index_of(NIGHT)
            }
        }
        ExoticTile::RelicCache | ExoticTile::LandmarkRelic => {
            // A little chest of builder make, a gold catch (a relic: a gold idol on a plinth).
            if tile == ExoticTile::LandmarkRelic {
                let (dx, dy) = ((px - 8).abs(), py);
                if dy >= 12 {
                    index_of(STEEL_DARK)
                } else if dx <= 2 && dy >= 3 || dx <= 4 && (6..=9).contains(&dy) {
                    if dx == 0 {
                        index_of(CREAM)
                    } else {
                        index_of(GOLD)
                    }
                } else {
                    index_of(NIGHT)
                }
            } else if !(3..13).contains(&py) || !(2..14).contains(&px) {
                index_of(NIGHT)
            } else if py == 7 && (6..10).contains(&px) {
                index_of(GOLD)
            } else if py == 3 || px == 2 || px == 13 {
                index_of(STEEL_LIGHT)
            } else {
                index_of(TEAL)
            }
        }
    }
}

/// One art pixel of an exotic tile's body (ore and finds sit in it as their host rock).
#[must_use]
pub(super) fn base_pixel(tile: ExoticTile, x: i32, y: i32) -> PaletteIndex {
    body(tile, x, y, x.rem_euclid(TILE_ART), y.rem_euclid(TILE_ART))
}

/// Paints exotic tile `tile`.
pub(super) fn paint(canvas: &mut TileCanvas, tile: ExoticTile, strata: &Strata, edges: Edges) {
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            canvas.set(px, py, body(tile, gx, gy, px, py));
        }
    }
    let (lip, shade) = lip_and_shade(tile);
    let square = matches!(
        tile,
        ExoticTile::SkyDeck
            | ExoticTile::CityHull
            | ExoticTile::CityTower
            | ExoticTile::TetherPlate
            | ExoticTile::BuilderPlate
            | ExoticTile::BuilderBeacon
            | ExoticTile::BossSeal
            | ExoticTile::ConduitLive
            | ExoticTile::ConduitDead
            | ExoticTile::ConduitJunction
            | ExoticTile::Monolith
    );
    paint_edges(canvas, strata, edges, lip, shade, square);
}

/// Paints an open tile of an exotic world below its surface: a gas giant's haze (it darkens
/// band by band, streaked by the wind), or a tunnel's back wall shadowed by the rock round it.
pub(super) fn paint_back(
    canvas: &mut TileCanvas,
    terrain: &Terrain,
    position: TilePosition,
    (style, zone, side): (ExoticStyle, Zone, Side),
    (up, left, right): (Cell, Cell, Cell),
) {
    if style == ExoticStyle::GasGiant {
        paint_haze(canvas, terrain, position, zone);
        return;
    }
    let shade = index_of(INK);
    let [body, dark] = back_pair(style, zone, side);
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let shadowed = (up == Cell::Solid && (py < 2 || (py < 4 && bayer4(gx, gy) < 6)))
                || (left == Cell::Solid && (px == 0 || (px == 1 && bayer4(gx, gy) < 8)))
                || (right == Cell::Solid
                    && (px == TILE_ART - 1 || (px == TILE_ART - 2 && bayer4(gx, gy) < 8)));
            let index = if shadowed {
                shade
            } else if noise(gx, gy, 11, 3501) + dither(gx, gy) > 0.8
                || (gy + gx / 4).rem_euclid(13) == 0
            {
                body
            } else {
                dark
            };
            canvas.set(px, py, index);
        }
    }
}

/// The gas giant's open gas: a haze of the band's colours in long wind streaks, darker deeper.
#[allow(
    clippy::cast_possible_truncation,
    reason = "noise offsets are a few pixels"
)]
fn paint_haze(canvas: &mut TileCanvas, terrain: &Terrain, position: TilePosition, zone: Zone) {
    let ground = terrain.planet_ground();
    let band = match zone {
        Zone::Stratum(index) => index,
        Zone::CoreChamber => 4,
        _ => 0,
    };
    // How far into its band the tile is (0-1023), to blend toward the next band's colour.
    let into = ground.map_or(0, |ground| {
        let top = gas_giant::band_top_row(ground, usize::from(band.min(3)));
        let next = gas_giant::band_top_row(ground, usize::from(band.min(3)) + 1).max(top + 1);
        let depth = position.y - gas_giant::cloud_top(ground, i64::from(position.x));
        i64::from((depth - top).clamp(0, next - top)) * 1_024 / i64::from(next - top)
    });
    let (light, mid, dark) = match band {
        0 => (BONE, MIST, FOG),
        1 => (AMBER, COPPER, RUST),
        2 => (SLATE_LIGHT, OCEAN, DEEP_BLUE),
        3 => (SLATE, NIGHT, INK),
        _ => (STEEL, NIGHT, INK),
    };
    for py in 0..TILE_ART {
        for px in 0..TILE_ART {
            let (gx, gy) = (canvas.origin_x + px, canvas.origin_y + py);
            let wave = (noise(gx, gy, 40, 3601) * 6.0) as i32;
            let streak = (gy + wave).rem_euclid(9);
            let deeper =
                (noise(gx, gy, 23, 3602) * 1_024.0) as i64 + i64::from(bayer4(gx, gy)) * 12 < into;
            let index = if streak == 0 && !deeper {
                index_of(light)
            } else if deeper || streak > 6 {
                index_of(dark)
            } else {
                index_of(mid)
            };
            canvas.set(px, py, index);
        }
    }
}
