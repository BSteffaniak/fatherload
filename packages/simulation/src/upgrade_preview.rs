//! What an upgrade changes, measured from the real gameplay formulas (WP-FEEL part 2).
//!
//! The shop's before/after stat bars and its concrete "what this changes" lines ("Drills hard
//! rock 1.6x faster", "Climbs 61% higher on a full tank") come from here.
//!
//! Everything here is a pure function of a [`Player`] and the same formulas the fixed tick
//! uses (drill speed and bite, base chip seconds, tank sizes and burn efficiency, climb speed,
//! top speed, hull plating, cooling, hazard protection), so a preview can never promise more
//! than the purchase delivers. It ignores rig parts, cargo load and (until T16) thin air: the
//! numbers describe a stock rig with an empty bay in sea-level air, and say so where it matters.
//!
//! # API
//!
//! - [`upgrade_preview`]: the next tier of a line for a player (the shop showcase).
//! - [`preview_between`]: any two levels of a line (the purchase moment compares the tier just
//!   bought with the one before).
//! - The measurements: [`dig_seconds`], [`dig_speed_multiplier`], [`thrust_seconds`], [`climb_speed_tiles`],
//!   [`climb_ceiling_tiles`] (sea level), [`thin_air_ceiling_tiles`] (a sky with thin air: the
//!   measured climb table), [`cooled_band`].

use crate::{
    economy::{UpgradeKind, current_level},
    milestone::{crosses_dead_air, island_capable},
    player::{
        ENGINE_CLIMB_CAP, Player, cargo_capacity_for_level, drill_bite, drill_speed, engine_power,
        fuel_burn_efficiency, fuel_capacity_for_level, hull_capacity_for_level,
    },
    protection::{MAX_BAND_HEAT, band_heat_units, heat_cooling, protection_fraction},
    terrain::{SurfaceMaterial, TILE_SIZE, TileKind, tile_durability, tile_hardness},
    world::{
        AUTHORITATIVE_FUEL_BURN_PER_SECOND, AUTHORITATIVE_MAX_CLIMB_SPEED,
        MIN_DRILL_SECONDS_PER_CHIP, base_drill_seconds_per_chip,
        drops::block_hopper_capacity_for_level, engine_top_speed_tiles,
    },
    worldgen::{Band, sky::SkyBands},
};

/// One before/after bar: `now` and `next` on a scale whose full bar is `max` (the line's top
/// tier, so the bar also shows how far the line has to go).
#[derive(Clone, Debug, PartialEq)]
pub struct StatBar {
    pub label: &'static str,
    pub now: f32,
    pub next: f32,
    pub max: f32,
    /// Unit written after the value ("fuel", "tiles/s", "%").
    pub unit: &'static str,
    /// Decimals the value is written with.
    pub decimals: u8,
}

impl StatBar {
    /// `value` written with this bar's decimals and unit.
    #[must_use]
    pub fn format(&self, value: f32) -> String {
        let number = format!("{value:.*}", usize::from(self.decimals));
        if self.unit.is_empty() {
            number
        } else if self.unit == "%" || self.unit.starts_with('x') {
            format!("{number}{}", self.unit)
        } else {
            format!("{number} {}", self.unit)
        }
    }

    /// `now` and `next` as fractions of a full bar.
    #[must_use]
    pub fn fractions(&self) -> (f32, f32) {
        if self.max <= 0.0 {
            return (0.0, 0.0);
        }
        (
            (self.now / self.max).clamp(0.0, 1.0),
            (self.next / self.max).clamp(0.0, 1.0),
        )
    }
}

/// What going from `from_level` to `to_level` on a line changes.
#[derive(Clone, Debug, PartialEq)]
pub struct UpgradePreview {
    pub kind: UpgradeKind,
    pub from_level: u8,
    pub to_level: u8,
    /// Before/after bars, most important first.
    pub bars: Vec<StatBar>,
    /// Concrete, measured lines ("Drills hard rock 1.6x faster (0.79 s -> 0.50 s a tile)"),
    /// most important first.
    pub changes: Vec<String>,
}

impl UpgradePreview {
    /// The most important change line, for toasts.
    #[must_use]
    pub fn headline(&self) -> Option<&str> {
        self.changes.first().map(String::as_str)
    }
}

/// The preview of the next tier of `kind` for `player`, or `None` when the line is maxed (bomb
/// packs never max out). `sky` is the world's sky bands (see [`preview_between`]).
#[must_use]
pub fn upgrade_preview(
    player: &Player,
    kind: UpgradeKind,
    sky: Option<&SkyBands>,
) -> Option<UpgradePreview> {
    upgrade_preview_for(
        player,
        kind,
        sky,
        crate::depth_progression::ProgressionRules::Classic,
    )
}

/// [`upgrade_preview`] under the world's `rules`: on Deep Descent worlds the core lines preview
/// their deep tiers past the classic top (Part III F10).
#[must_use]
pub fn upgrade_preview_for(
    player: &Player,
    kind: UpgradeKind,
    sky: Option<&SkyBands>,
    rules: crate::depth_progression::ProgressionRules,
) -> Option<UpgradePreview> {
    let level = current_level(player, kind);
    if kind != UpgradeKind::BombPack && level >= kind.max_level_for(rules) {
        return None;
    }
    let next = if kind == UpgradeKind::BombPack {
        level
    } else {
        level + 1
    };
    Some(preview_between(player, kind, level, next, sky))
}

/// The hardest grade of common rock (hard rock and the richest ores): a drill of this tier or
/// better digs every breakable tile.
pub const MAX_ROCK_HARDNESS: u8 = 4;

/// Core materials the drill preview measures, softest first, with their display names.
/// Mountain granite digs like stone (one hardness grade, a chip more durable), so the two share a
/// rung, measured on granite.
const DRILL_LADDER: [(TileKind, &str); 4] = [
    (TileKind::Dirt, "dirt"),
    (TileKind::Clay, "clay"),
    (
        TileKind::Surface(SurfaceMaterial::Granite),
        "granite and stone",
    ),
    (TileKind::HardRock, "hard rock"),
];

/// Seconds a stock-parted drill of `drill_level` takes to break a fresh `kind` tile drilling
/// down, or `None` if the tile is too hard for it.
#[must_use]
pub fn dig_seconds(kind: TileKind, drill_level: u8) -> Option<f32> {
    (tile_hardness(kind) <= drill_level).then(|| chip_through_seconds(kind, drill_level))
}

/// [`dig_seconds`] without the hardness check: how long the drill's chips (speed, chip-rate
/// floor and bite) take to wear a fresh `kind` tile away.
fn chip_through_seconds(kind: TileKind, drill_level: u8) -> f32 {
    let per_chip = (base_drill_seconds_per_chip(kind) / drill_speed(drill_level))
        .max(MIN_DRILL_SECONDS_PER_CHIP);
    let bite = drill_bite(drill_level).max(1);
    let chips = tile_durability(kind).max(1).div_ceil(bite);
    per_chip * f32::from(chips)
}

/// The drill's measured dig speed as a multiple of a stock drill's (the shop's "Dig speed" bar).
///
/// It is how much faster its chips wear through a hard rock tile: speed, the chip-rate floor and
/// bite all count, so tiers that change nothing on rock show no gain.
#[must_use]
pub fn dig_speed_multiplier(drill_level: u8) -> f32 {
    chip_through_seconds(TileKind::HardRock, 1)
        / chip_through_seconds(TileKind::HardRock, drill_level)
}

/// Seconds of continuous thrust a full tank at `fuel_tank_level` gives.
#[must_use]
pub fn thrust_seconds(fuel_tank_level: u8) -> f32 {
    fuel_capacity_for_level(fuel_tank_level)
        / (AUTHORITATIVE_FUEL_BURN_PER_SECOND * fuel_burn_efficiency(fuel_tank_level))
}

/// Top climb speed (tiles per second) of an engine at `engine_level`, empty bay.
#[must_use]
pub fn climb_speed_tiles(engine_level: u8) -> f32 {
    AUTHORITATIVE_MAX_CLIMB_SPEED * engine_power(engine_level).min(ENGINE_CLIMB_CAP) / TILE_SIZE
}

/// How many tiles a rig climbs straight up on one full tank (empty bay, sea-level air): the
/// tank's thrust time at the engine's top climb speed.
#[must_use]
pub fn climb_ceiling_tiles(fuel_tank_level: u8, engine_level: u8) -> f32 {
    thrust_seconds(fuel_tank_level) * climb_speed_tiles(engine_level)
}

/// How many tiles a rig climbs straight up on one full tank in a sky with thin air.
///
/// The measured climb table of the world's sky (`bands.climb`, [`crate::climb::reach_in`],
/// engine and intake rounded down to its tiers) above the harness's starting altitude. Without altitude intakes thin air caps every
/// climb, so this is far below [`climb_ceiling_tiles`] for big tanks.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "altitudes are some thousands of rows"
)]
pub fn thin_air_ceiling_tiles(
    bands: &SkyBands,
    fuel_tank_level: u8,
    engine_level: u8,
    intake_level: u8,
) -> f32 {
    (crate::climb::reach_in(bands.climb, fuel_tank_level, engine_level, intake_level)
        - crate::climb::START_ALTITUDE_ROWS)
        .max(0) as f32
}

/// The deepest band a rig with `cooling` heat units runs cool in.
#[must_use]
pub fn cooled_band(cooling: u8) -> Band {
    Band::ALL
        .into_iter()
        .rev()
        .find(|band| band_heat_units(*band) <= cooling)
        .unwrap_or(Band::Topsoil)
}

/// `next` over `now` as a whole percentage change (`+61`).
fn percent(now: f32, next: f32) -> i32 {
    if now <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, reason = "a small percentage")]
    let change = ((next / now - 1.0) * 100.0).round() as i32;
    change
}

const fn bar(
    label: &'static str,
    now: f32,
    next: f32,
    max: f32,
    unit: &'static str,
    decimals: u8,
) -> StatBar {
    StatBar {
        label,
        now,
        next,
        max,
        unit,
        decimals,
    }
}

/// The preview of going from `from_level` to `to_level` on `kind` with the rest of `player`'s
/// rig as it is (the radiator and shielding cool together, the engine climbs with the tank).
///
/// `sky` is the bands of the world's sky (`GeneratorVersion::sky_bands`): the tank, engine and
/// intake previews announce the sky gates against them, and say nothing of the sky without one.
#[must_use]
pub fn preview_between(
    player: &Player,
    kind: UpgradeKind,
    from_level: u8,
    to_level: u8,
    sky: Option<&SkyBands>,
) -> UpgradePreview {
    let mut preview = preview_lines(player, kind, from_level, to_level, sky);
    if sky.is_none() {
        // Without a sky no line promises islands or the dead-air gap, and the sky gear says it
        // has nothing to do (T20: sky-aware previews).
        preview.changes.retain(|line| !mentions_sky(line));
        if crate::economy::needs_sky(kind) {
            preview.changes.insert(
                0,
                "This world has no sky to climb into: nothing for it to do".to_owned(),
            );
        }
    }
    preview
}

/// Whether a preview line promises something only a sky has.
fn mentions_sky(line: &str) -> bool {
    line.contains("sky isles")
        || line.contains("dead-air gap")
        || line.contains("sky lands")
        || line.contains("thin air")
        || line.contains("cold of orbit")
        || line.contains("ice off the hull")
}

#[allow(clippy::too_many_lines, reason = "one arm per upgrade line")]
fn preview_lines(
    player: &Player,
    kind: UpgradeKind,
    from_level: u8,
    to_level: u8,
    sky: Option<&SkyBands>,
) -> UpgradePreview {
    let (from, to) = (from_level, to_level);
    // The sky gates, against the world's own bands (none without a sky).
    let island = |rig: &Player| sky.is_some_and(|bands| island_capable(rig, bands));
    let crosses = |rig: &Player| sky.is_some_and(|bands| crosses_dead_air(rig, bands));
    // A deep tier (Deep Descent worlds, Part III F10) measures against the deep top tier.
    let top = if kind.is_core() && to > kind.max_level() {
        crate::depth_progression::DESCENT_MAX_UPGRADE_LEVEL
    } else {
        kind.max_level()
    };
    let mut bars = Vec::new();
    let mut changes = Vec::new();
    match kind {
        UpgradeKind::Drill => {
            let rate = dig_speed_multiplier;
            bars.push(bar("Dig speed", rate(from), rate(to), rate(top), "x", 1));
            let hardness = |level: u8| f32::from(level.min(MAX_ROCK_HARDNESS));
            bars.push(bar(
                "Rock grade",
                hardness(from),
                hardness(to),
                f32::from(MAX_ROCK_HARDNESS),
                "",
                0,
            ));
            let hardest = |level: u8| {
                DRILL_LADDER
                    .iter()
                    .rev()
                    .find(|(tile, _)| tile_hardness(*tile) <= level)
                    .copied()
            };
            if let Some((tile, name)) = hardest(to)
                && dig_seconds(tile, from).is_none()
            {
                changes.push(format!("Now cuts {name}: no more flying around it"));
            }
            if let Some((tile, name)) = hardest(from)
                && let (Some(now), Some(next)) = (dig_seconds(tile, from), dig_seconds(tile, to))
                && next < now
            {
                changes.push(format!(
                    "Drills {name} {:.1}x faster ({now:.2} s -> {next:.2} s a tile)",
                    now / next
                ));
            }
            if let (Some(now), Some(next)) = (
                dig_seconds(TileKind::Dirt, from),
                dig_seconds(TileKind::Dirt, to),
            ) && next < now
                && hardest(from).is_some_and(|(tile, _)| tile != TileKind::Dirt)
            {
                changes.push(format!(
                    "Chews through dirt in {next:.2} s (was {now:.2} s)"
                ));
            }
            if drill_bite(to) > drill_bite(from) {
                changes.push(format!(
                    "Bites {} layers per chip (was {})",
                    drill_bite(to),
                    drill_bite(from)
                ));
            }
            if changes.is_empty() {
                // Common rock already falls at the chip-rate limit: say so honestly.
                changes.push(format!(
                    "Spins faster (x{:.1} -> x{:.1}), but common rock is already at the chip-rate \
                     limit",
                    drill_speed(from),
                    drill_speed(to)
                ));
            }
        }
        UpgradeKind::FuelTank => {
            let engine = player.engine_level;
            let intake = player.hazard_gear.altitude_intake;
            // In a sky with thin air the measured table, at sea level the plain formula.
            let climb_ceiling_tiles = |tank: u8, engine: u8| {
                sky.map_or_else(
                    || climb_ceiling_tiles(tank, engine),
                    |bands| thin_air_ceiling_tiles(bands, tank, engine, intake),
                )
            };
            let (fuel_now, fuel_next) =
                (fuel_capacity_for_level(from), fuel_capacity_for_level(to));
            let (ceiling_now, ceiling_next) = (
                climb_ceiling_tiles(from, engine),
                climb_ceiling_tiles(to, engine),
            );
            bars.push(bar(
                "Fuel",
                fuel_now,
                fuel_next,
                fuel_capacity_for_level(top),
                "fuel",
                0,
            ));
            bars.push(bar(
                "Climb ceiling",
                ceiling_now,
                ceiling_next,
                climb_ceiling_tiles(top, engine),
                "tiles",
                0,
            ));
            if percent(ceiling_now, ceiling_next) > 0 {
                changes.push(format!(
                    "Climbs {}% higher on a full tank ({ceiling_now:.0} -> {ceiling_next:.0} tiles)",
                    percent(ceiling_now, ceiling_next)
                ));
            } else {
                // Thin air, not fuel, stops this rig: say so instead of a 0 % gain.
                changes.push(format!(
                    "Thin air caps the climb at {ceiling_now:.0} tiles: altitude intakes and a \
                     stronger engine lift it"
                ));
            }
            changes.push(format!(
                "{:.0} s of thrust per tank (was {:.0} s)",
                thrust_seconds(to),
                thrust_seconds(from)
            ));
            let (burn_now, burn_next) = (fuel_burn_efficiency(from), fuel_burn_efficiency(to));
            if burn_next < burn_now {
                changes.push(format!(
                    "Burns {}% less fuel per second",
                    -percent(burn_now, burn_next)
                ));
            }
            let mut upgraded = player.clone();
            upgraded.fuel_tank_level = to;
            let mut current = player.clone();
            current.fuel_tank_level = from;
            if island(&upgraded) && !island(&current) {
                changes.insert(0, "Reaches the low sky isles!".to_owned());
            }
            if crosses(&upgraded) && !crosses(&current) {
                changes.insert(
                    0,
                    "Crosses the dead-air gap: the sky lands open!".to_owned(),
                );
            } else if !crosses(&upgraded) {
                // Would this tank carry a top engine and intake over? Say so the first time.
                let mut kitted = upgraded;
                kitted.engine_level = crate::economy::MAX_UPGRADE_LEVEL;
                kitted.hazard_gear.altitude_intake = crate::environment::ALTITUDE_INTAKE_MAX_LEVEL;
                let mut kitted_now = kitted.clone();
                kitted_now.fuel_tank_level = from;
                if crosses(&kitted) && !crosses(&kitted_now) {
                    changes.push(
                        "Big enough for the dead-air gap, with a strong engine and altitude \
                         intakes"
                            .to_owned(),
                    );
                }
            }
        }
        UpgradeKind::CargoBay => {
            let (now, next) = (cargo_capacity_for_level(from), cargo_capacity_for_level(to));
            #[allow(clippy::cast_precision_loss, reason = "a few hundred slots")]
            bars.push(bar(
                "Cargo slots",
                now as f32,
                next as f32,
                cargo_capacity_for_level(top) as f32,
                "slots",
                0,
            ));
            let (hopper_now, hopper_next) = (
                block_hopper_capacity_for_level(from),
                block_hopper_capacity_for_level(to),
            );
            #[allow(clippy::cast_precision_loss, reason = "a few hundred blocks")]
            bars.push(bar(
                "Block hopper",
                hopper_now as f32,
                hopper_next as f32,
                block_hopper_capacity_for_level(top) as f32,
                "blocks",
                0,
            ));
            #[allow(clippy::cast_precision_loss, reason = "a few hundred slots")]
            changes.push(format!(
                "Hauls {} more ore per trip ({now} -> {next} slots, +{}%)",
                next - now,
                percent(now as f32, next as f32)
            ));
            if hopper_next > hopper_now {
                changes.push(format!("Block hopper {hopper_now} -> {hopper_next} blocks"));
            }
        }
        UpgradeKind::Engine => {
            let (speed_now, speed_next) =
                (engine_top_speed_tiles(from), engine_top_speed_tiles(to));
            bars.push(bar(
                "Top speed",
                speed_now,
                speed_next,
                engine_top_speed_tiles(top),
                "tiles/s",
                1,
            ));
            let (climb_now, climb_next) = (climb_speed_tiles(from), climb_speed_tiles(to));
            bars.push(bar(
                "Climb speed",
                climb_now,
                climb_next,
                climb_speed_tiles(top),
                "tiles/s",
                1,
            ));
            bars.push(bar(
                "Thrust",
                engine_power(from),
                engine_power(to),
                engine_power(top),
                "x",
                2,
            ));
            changes.push(format!(
                "Top speed +{}% ({speed_now:.1} -> {speed_next:.1} tiles/s)",
                percent(speed_now, speed_next)
            ));
            changes.push(format!(
                "Crosses 100 tiles in about {:.0} s (was {:.0} s)",
                100.0 / speed_next,
                100.0 / speed_now
            ));
            if climb_next > climb_now {
                changes.push(format!("Climbs {}% faster", percent(climb_now, climb_next)));
            } else {
                changes.push(format!(
                    "Lifts {}% harder with a full bay (climb speed is already at its cap)",
                    percent(engine_power(from), engine_power(to))
                ));
            }
            let mut upgraded = player.clone();
            upgraded.engine_level = to;
            let mut current = player.clone();
            current.engine_level = from;
            if crosses(&upgraded) && !crosses(&current) {
                changes.insert(
                    0,
                    "Crosses the dead-air gap: the sky lands open!".to_owned(),
                );
            }
        }
        UpgradeKind::Hull => {
            let bulkheads = f32::from(player.crafted_bulkheads) * 15.0;
            let (now, next) = (
                hull_capacity_for_level(from) + bulkheads,
                hull_capacity_for_level(to) + bulkheads,
            );
            bars.push(bar(
                "Hull",
                now,
                next,
                hull_capacity_for_level(top) + bulkheads,
                "hull",
                0,
            ));
            changes.push(format!(
                "Takes {}% more punishment ({now:.0} -> {next:.0} hull)",
                percent(now, next)
            ));
            changes.push("Repaired to full on fitting".to_owned());
        }
        UpgradeKind::Radiator | UpgradeKind::HeatShield => {
            let cooling = |level: u8| {
                if kind == UpgradeKind::Radiator {
                    heat_cooling(level, player.hazard_gear.heat_shield)
                } else {
                    heat_cooling(player.radiator_level, level)
                }
            };
            let full = f32::from(MAX_BAND_HEAT) * 3.0;
            bars.push(bar(
                "Cooling",
                f32::from(cooling(from)).min(full) / 3.0,
                f32::from(cooling(to)).min(full) / 3.0,
                f32::from(MAX_BAND_HEAT),
                "heat",
                1,
            ));
            let (band_now, band_next) = (cooled_band(cooling(from)), cooled_band(cooling(to)));
            if band_next == band_now {
                changes.push(format!(
                    "More headroom in the {} (cooling {:.1} -> {:.1})",
                    band_now.name(),
                    f32::from(cooling(from)) / 3.0,
                    f32::from(cooling(to)) / 3.0
                ));
            } else {
                changes.push(format!(
                    "Runs cool down to the {} (was the {})",
                    band_next.name(),
                    band_now.name()
                ));
            }
        }
        UpgradeKind::LavaPlating
        | UpgradeKind::GasScrubber
        | UpgradeKind::PressureBracing
        | UpgradeKind::BoulderDeflector => {
            let (now, next) = (protection_fraction(from, top), protection_fraction(to, top));
            bars.push(bar("Protection", now * 100.0, next * 100.0, 100.0, "%", 0));
            let hazard = match kind {
                UpgradeKind::LavaPlating => "lava burns",
                UpgradeKind::GasScrubber => "gas damage",
                UpgradeKind::PressureBracing => "pressure and vent blasts",
                _ => "rockfall damage",
            };
            if to >= top {
                changes.push(format!("Immune to {hazard}!"));
            } else {
                #[allow(clippy::cast_possible_truncation, reason = "a percentage")]
                changes.push(format!(
                    "Stops {}% of {hazard} (was {}%)",
                    (next * 100.0).round() as i32,
                    (now * 100.0).round() as i32
                ));
            }
        }
        // The sky lines (T16, F6): what thin air and cold cost a rig that has them.
        UpgradeKind::AltitudeIntake => {
            let (now, next) = (
                crate::environment::intake_recovery(from),
                crate::environment::intake_recovery(to),
            );
            bars.push(bar(
                "Thin-air thrust",
                now * 100.0,
                next * 100.0,
                crate::environment::intake_recovery(top) * 100.0,
                "%",
                0,
            ));
            #[allow(clippy::cast_possible_truncation, reason = "a percentage")]
            changes.push(format!(
                "Recovers {}% of the thrust thin air steals (was {}%)",
                (next * 100.0).round() as i32,
                (now * 100.0).round() as i32
            ));
            let mut upgraded = player.clone();
            upgraded.hazard_gear.altitude_intake = to;
            let mut current = player.clone();
            current.hazard_gear.altitude_intake = from;
            if crosses(&upgraded) && !crosses(&current) {
                changes.insert(
                    0,
                    "Crosses the dead-air gap: the sky lands open!".to_owned(),
                );
            }
        }
        UpgradeKind::DeIcer => {
            let (now, next) = (
                crate::environment::cold_cover(from),
                crate::environment::cold_cover(to),
            );
            bars.push(bar(
                "Cold cover",
                f32::from(now),
                f32::from(next),
                f32::from(crate::environment::MAX_COLD),
                "levels",
                0,
            ));
            changes.push(if next >= crate::environment::MAX_COLD {
                "Shrugs off the cold of orbit and the Moon!".to_owned()
            } else {
                format!("Keeps ice off the hull up to cold {next} (was {now})")
            });
        }
        // The Moon lines (T22, WP-MOON).
        UpgradeKind::PressurizedHull => {
            bars.push(bar(
                "Hull seal",
                f32::from(from),
                f32::from(to),
                f32::from(top),
                "",
                0,
            ));
            changes.push(match to.cmp(&crate::moon::VACUUM_SEAL_TIER) {
                std::cmp::Ordering::Less => {
                    "A sealed cabin: the high sky's cold bites one level less".to_owned()
                }
                std::cmp::Ordering::Equal => {
                    "Seals the rig against vacuum: orbit and the Moon stop crushing the frame"
                        .to_owned()
                }
                std::cmp::Ordering::Greater => {
                    "Void-rated plating: Moon creatures bite a fifth softer".to_owned()
                }
            });
        }
        UpgradeKind::LifeSupport => {
            let (now, next) = (
                crate::moon::life_support_reach_tiles(from),
                crate::moon::life_support_reach_tiles(to),
            );
            bars.push(bar(
                "Pilot air line",
                now,
                next,
                crate::moon::life_support_reach_tiles(top),
                "tiles",
                0,
            ));
            changes.push(if from == 0 {
                "Air for the crew in vacuum, and for the pilot near the rig".to_owned()
            } else {
                format!("The pilot breathes up to {next:.0} tiles from the rig (was {now:.0})")
            });
        }
        // The Deep Descent's rig systems (Part III F10): one tier, one promise.
        UpgradeKind::DescentPressureHull
        | UpgradeKind::BioFilter
        | UpgradeKind::LampII
        | UpgradeKind::SonarSuite
        | UpgradeKind::CryoHull
        | UpgradeKind::GravityAnchor
        | UpgradeKind::PhaseDrill
        | UpgradeKind::HeartKey => {
            // The shop appends the line's description, so these say what it unlocks.
            bars.push(bar(
                "Fitted",
                f32::from(from),
                f32::from(to),
                f32::from(top),
                "",
                0,
            ));
            if let Some(system) = kind.rig_system() {
                if let Some(descent) = system.gates_descent() {
                    changes.push(format!(
                        "Opens Descent {}: without it the depths there crush the rig",
                        crate::depth_progression::numeral(descent)
                    ));
                }
                match system {
                    crate::depth_progression::RigSystem::LampII => changes.push(format!(
                        "The headlamp lights {:.0}% further",
                        (crate::depth_progression::LAMP_II_LIGHT_SCALE - 1.0) * 100.0
                    )),
                    crate::depth_progression::RigSystem::SonarSuite => changes.push(format!(
                        "Scanner pings reach {} tiles further",
                        crate::depth_progression::SONAR_SUITE_SCAN_BONUS_TILES
                    )),
                    crate::depth_progression::RigSystem::BioFilter => changes.push(format!(
                        "Gas clouds do {}% of their damage",
                        crate::depth_progression::BIO_FILTER_GAS_PERCENT
                    )),
                    _ => {}
                }
            }
        }
        UpgradeKind::Scanner => {
            bars.push(bar(
                "Scanner",
                f32::from(from),
                f32::from(to),
                f32::from(top),
                "",
                0,
            ));
            changes.push(if from == 0 {
                "Pings nearby ore and hazards through the rock".to_owned()
            } else {
                format!("Scanner tier {to}: reaches further and reveals more")
            });
        }
        UpgradeKind::BombPack => {
            #[allow(clippy::cast_precision_loss, reason = "a few dozen bombs")]
            let carried = player.bombs as f32;
            bars.push(bar(
                "Bombs",
                carried,
                carried + 3.0,
                (carried + 3.0).max(12.0),
                "",
                0,
            ));
            changes.push(format!(
                "+3 blasting charges ({} -> {})",
                player.bombs,
                player.bombs + 3
            ));
        }
        UpgradeKind::DrillMount => {
            bars.push(bar(
                "Mount",
                f32::from(from),
                f32::from(to),
                f32::from(top),
                "",
                0,
            ));
            changes.push(if to <= 1 {
                "Drill sideways while flying".to_owned()
            } else {
                "Thrust into a ceiling to drill straight up".to_owned()
            });
        }
    }
    UpgradePreview {
        kind,
        from_level,
        to_level,
        bars,
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sky calibrated on the current fuel curve (the next generator's bands).
    const SKY: Option<&SkyBands> = Some(&crate::worldgen::sky::NEXT_SKY_BANDS);

    fn rig() -> Player {
        Player::new(0.0, 0.0)
    }

    #[test]
    fn dig_seconds_follow_the_drill_formulas() {
        // Stock drill, dirt: 2 chips of 0.09 s.
        let dirt = dig_seconds(TileKind::Dirt, 1).expect("diggable");
        assert!((dirt - 0.18).abs() < 1e-5, "{dirt}");
        // Too hard for a stock drill.
        assert_eq!(dig_seconds(TileKind::HardRock, 1), None);
        // Tier 8: speed 2.6 + 0.5 * 3 = 4.1, bite 2 -> hard rock (11) in 6 chips.
        let rock = dig_seconds(TileKind::HardRock, 8).expect("diggable");
        let expected = (0.19 / 4.1_f32).max(MIN_DRILL_SECONDS_PER_CHIP) * 6.0;
        assert!((rock - expected).abs() < 1e-5, "{rock} vs {expected}");
        // Every tier is at least as fast as the one before on everything it can dig.
        for (tile, _) in DRILL_LADDER {
            for level in 1..15 {
                if let (Some(now), Some(next)) =
                    (dig_seconds(tile, level), dig_seconds(tile, level + 1))
                {
                    assert!(next <= now, "{tile:?} tier {level}");
                }
            }
        }
    }

    #[test]
    fn the_dig_speed_bar_is_the_measured_rock_speed_up() {
        assert!((dig_speed_multiplier(1) - 1.0).abs() < 1e-6);
        for level in 1..15 {
            let (now, next) = (dig_speed_multiplier(level), dig_speed_multiplier(level + 1));
            assert!(next >= now, "tier {level}");
            // Tiers that dig hard rock show exactly its dig-time ratio.
            if let (Some(a), Some(b)) = (
                dig_seconds(TileKind::HardRock, level),
                dig_seconds(TileKind::HardRock, level + 1),
            ) {
                assert!((next / now - a / b).abs() < 1e-4, "tier {level}");
            }
        }
        // Past the chip-rate floor a tier that adds no bite adds no speed: the bar stays put.
        assert!((dig_speed_multiplier(11) - dig_speed_multiplier(10)).abs() < 1e-6);
    }

    #[test]
    fn the_drill_preview_names_the_new_rock_and_the_speedup() {
        let mut player = rig();
        player.drill_strength = 3;
        let preview = upgrade_preview(&player, UpgradeKind::Drill, None).expect("not maxed");
        assert_eq!((preview.from_level, preview.to_level), (3, 4));
        assert!(
            preview.changes[0].contains("Now cuts hard rock"),
            "{:?}",
            preview.changes
        );
        let granite = TileKind::Surface(SurfaceMaterial::Granite);
        let ratio = dig_seconds(granite, 3).unwrap() / dig_seconds(granite, 4).unwrap();
        let line = format!("Drills granite and stone {ratio:.1}x faster");
        assert!(
            preview
                .changes
                .iter()
                .any(|change| change.starts_with(&line)),
            "{line} in {:?}",
            preview.changes
        );
    }

    #[test]
    fn the_tank_preview_reads_the_climb_table_under_a_sky() {
        let player = rig();
        let preview = upgrade_preview(&player, UpgradeKind::FuelTank, SKY).expect("not maxed");
        let ceiling = preview
            .bars
            .iter()
            .find(|bar| bar.label == "Climb ceiling")
            .expect("ceiling bar");
        let climb = SKY.expect("a sky").climb;
        let table = |tank| {
            f32::from(
                u16::try_from(crate::climb::ceiling_in(climb, tank, 1, 0) - 100).expect("rows"),
            )
        };
        assert!((ceiling.now - table(1)).abs() < 1e-3, "{ceiling:?}");
        assert!((ceiling.next - table(2)).abs() < 1e-3, "{ceiling:?}");
        assert!((ceiling.max - table(15)).abs() < 1e-3, "{ceiling:?}");
    }

    #[test]
    fn the_tank_preview_measures_the_climb() {
        // Sea level (a world without a sky): the tank's thrust time at the climb speed.
        let player = rig();
        let preview = upgrade_preview(&player, UpgradeKind::FuelTank, None).expect("not maxed");
        let ceiling = preview
            .bars
            .iter()
            .find(|bar| bar.label == "Climb ceiling")
            .expect("ceiling bar");
        // Stock: 450 fuel at 5/s = 90 s; tier 2: 700 fuel at 4.75/s.
        let speed = climb_speed_tiles(1);
        assert!(90.0f32.mul_add(-speed, ceiling.now).abs() < 1e-2);
        assert!((700.0f32 / 4.75).mul_add(-speed, ceiling.next).abs() < 1e-2);
        let expected = percent(ceiling.now, ceiling.next);
        assert!(
            preview
                .changes
                .iter()
                .any(|change| change.starts_with(&format!("Climbs {expected}% higher"))),
            "{:?}",
            preview.changes
        );
    }

    #[test]
    fn the_tank_preview_announces_the_sky_gates() {
        let mut player = rig();
        // The first tank upgrade lifts a stock rig into the low isles (the climb table).
        player.fuel_tank_level = 1;
        let preview = upgrade_preview(&player, UpgradeKind::FuelTank, SKY).expect("not maxed");
        assert_eq!(preview.headline(), Some("Reaches the low sky isles!"));
        // A top engine with tier 6 intakes crosses the dead air from tank 13 (tank 12 cannot).
        player.fuel_tank_level = 12;
        player.engine_level = 15;
        player.hazard_gear.altitude_intake = 6;
        let preview = upgrade_preview(&player, UpgradeKind::FuelTank, SKY).expect("not maxed");
        assert_eq!(
            preview.headline(),
            Some("Crosses the dead-air gap: the sky lands open!")
        );
        // ... and so does buying the intake tier that a big tank and engine lack (3 -> 4).
        player.fuel_tank_level = 15;
        player.hazard_gear.altitude_intake = 3;
        let preview =
            upgrade_preview(&player, UpgradeKind::AltitudeIntake, SKY).expect("not maxed");
        assert_eq!(
            preview.headline(),
            Some("Crosses the dead-air gap: the sky lands open!")
        );
        // A world without a sky hears nothing of it.
        let preview =
            upgrade_preview(&player, UpgradeKind::AltitudeIntake, None).expect("not maxed");
        assert!(
            preview
                .changes
                .iter()
                .all(|change| !change.contains("dead-air")),
            "{:?}",
            preview.changes
        );
        player.fuel_tank_level = 1;
        player.engine_level = 1;
        let preview = upgrade_preview(&player, UpgradeKind::FuelTank, None).expect("not maxed");
        assert!(
            preview.changes.iter().all(|change| !change.contains("sky")),
            "{:?}",
            preview.changes
        );
    }

    #[test]
    fn engine_cargo_hull_and_cooling_deltas_match_their_tables() {
        let player = rig();
        let engine = upgrade_preview(&player, UpgradeKind::Engine, None).expect("not maxed");
        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(engine.bars[0].now, engine_top_speed_tiles(1)));
        assert!(close(engine.bars[0].next, engine_top_speed_tiles(2)));
        assert!(close(engine.bars[0].max, engine_top_speed_tiles(15)));
        let cargo = upgrade_preview(&player, UpgradeKind::CargoBay, None).expect("not maxed");
        assert!(cargo.changes[0].starts_with("Hauls 10 more ore per trip (12 -> 22 slots"));
        let hull = upgrade_preview(&player, UpgradeKind::Hull, None).expect("not maxed");
        assert!(
            hull.changes[0].contains("(100 -> 140 hull)"),
            "{:?}",
            hull.changes
        );
        let radiator = upgrade_preview(&player, UpgradeKind::Radiator, None).expect("not maxed");
        assert!(
            radiator.changes[0].contains(cooled_band(heat_cooling(2, 0)).name()),
            "{:?}",
            radiator.changes
        );
    }

    #[test]
    fn maxed_lines_have_no_preview_but_bomb_packs_always_do() {
        let mut player = rig();
        crate::economy::max_out_upgrades(&mut player);
        for kind in UpgradeKind::ALL {
            let preview = upgrade_preview(&player, kind, SKY);
            assert_eq!(preview.is_some(), kind == UpgradeKind::BombPack, "{kind:?}");
        }
    }

    #[test]
    fn every_line_has_a_bar_and_a_change_at_every_tier() {
        let player = rig();
        for kind in UpgradeKind::ALL {
            for level in kind.starting_level()..kind.max_level() {
                let preview = preview_between(&player, kind, level, level + 1, SKY);
                assert!(!preview.bars.is_empty(), "{kind:?} {level}");
                assert!(!preview.changes.is_empty(), "{kind:?} {level}");
                for bar in &preview.bars {
                    let (now, next) = bar.fractions();
                    assert!(next >= now, "{kind:?} {level} {}", bar.label);
                }
            }
        }
    }
}
