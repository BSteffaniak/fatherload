//! Night-time gameplay (expansion plan WP-NIGHT, Part II R22): pure data and rules, no world
//! access. The behaviour lives in `world/night.rs`.
//!
//! * **Night monsters** ([`is_nocturnal`]): shadow hounds, dune stalkers, will-o'-wisps, moth
//!   swarms and the rare umbral alpha rise on the open surface at dusk and burrow or flee at
//!   dawn. The biome sets the mix ([`night_roster`]); the Moon sets the numbers
//!   ([`moon_density_percent`]: a full moon brings more) and new-moon nights bring the rare
//!   spawn ([`is_rare_spawn_night`]).
//! * **Light matters** ([`light_response`]): hounds, stalkers and the alpha shun the light of
//!   lamps and settlements (and run from flares); moths are drawn to every light (and circle a
//!   lamp instead of biting); wisps are lights themselves and do not care.
//! * **Safety**: towns, villages, wanderers' camps, homes (claims with a bed or a homestead)
//!   and fixture light of at least [`NIGHT_SAFE_LIGHT`] keep night monsters off; so does a claim
//!   fenced with palisades on both sides ([`crate::base::BuiltTile::Palisade`]). Night monsters
//!   stay on the open surface: tunnels and the high sky are not theirs.
//! * **Night economy**: night-only drops ([`NIGHT_MATERIALS`], valued by
//!   [`night_material_value`]) sell at the night market (village markets after dark, see
//!   [`crate::village::services::MarketTime`]); shallow nocturnal ores ([`glows_at_night`])
//!   glow and reveal themselves to anyone near them on the surface at night.
//! * **Hooks for story arcs** (T26): [`NightLedger`] (kills by kind, nights survived, the last
//!   rare night), [`NightHook`] and the night events (`NightFell`, `NightSurvived`,
//!   `RareNightSpawn`, `NightMonsterBurrowed`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    clock::{DayPhase, MoonPhase, WorldClock},
    enemy::EnemyKind,
    terrain::{MineralKind, StrategicResourceKind, SurfaceMaterial, TileKind},
    worldgen::biome::Biome,
};

/// Fixture light (permille, from lamps, settlements and campfires) at or above which night
/// monsters will not go: a lamp's pool of about four tiles.
pub const NIGHT_SAFE_LIGHT: u16 = 280;
/// Rows under the local ground below which night monsters never follow (tunnels are the deep
/// enemies' domain).
pub const NIGHT_SURFACE_DEPTH: i32 = 4;
/// Rows above the local ground night monsters reach (the high sky is not theirs).
pub const NIGHT_SKY_ROWS: i32 = 30;
/// Tiles round a wanderers' campfire that stay safe.
pub const CAMP_SAFE_TILES: i32 = 10;
/// Night monsters spawn this many columns from the actor they spawn for (out of view).
pub const NIGHT_SPAWN_COLUMNS: (i32, i32) = (24, 34);
/// Night monsters counted toward an actor's local cap within this many tiles.
pub const NIGHT_LOCAL_TILES: f32 = 36.0;
/// Night monsters around one actor before more stop rising (before the Moon and difficulty).
pub const NIGHT_LOCAL_CAP: usize = 2;
/// Base per-mille chance per spawn check (each second) that a monster rises near an exposed
/// actor, before difficulty, the world's hazards and the Moon.
pub const NIGHT_SPAWN_PER_MILLE: u32 = 70;
/// Per-mille chance per check that the rare spawn walks on a new-moon night (once a night).
pub const RARE_SPAWN_PER_MILLE: u32 = 35;
/// How far an idle night monster scents prey and prowls toward it, tiles.
pub const SCENT_TILES: f32 = 48.0;
/// How long a night monster takes to burrow or flee at dawn (plus up to [`RETREAT_STAGGER_MILLIS`]).
pub const RETREAT_MILLIS: u64 = 1_400;
/// Spread of retreat times so the night empties over a moment, not in one tick.
pub const RETREAT_STAGGER_MILLIS: u64 = 1_600;
/// How far a moth swarm sees a light to circle (lamps, flares), tiles.
pub const MOTH_LURE_TILES: f32 = 14.0;
/// How close a lit flare must be to send a light-shunning monster running, tiles.
pub const FLARE_SCARE_TILES: f32 = 6.0;
/// Rows under the local ground nocturnal ore glows at night.
pub const NOCTURNAL_GLOW_DEPTH: i32 = 24;
/// Tiles round an actor on the surface at night within which glowing ore reveals itself.
pub const NOCTURNAL_REVEAL_TILES: i32 = 10;
/// Light (permille) right at a glowing nocturnal ore tile at night.
pub const NOCTURNAL_ORE_LIGHT: u16 = 380;
/// Reach (tiles) of a nocturnal ore's glow.
pub const NOCTURNAL_ORE_LIGHT_TILES: i32 = 2;
/// Light (permille) right at a will-o'-wisp.
pub const WISP_LIGHT: u16 = 520;
/// Reach (tiles) of a will-o'-wisp's glow.
pub const WISP_LIGHT_TILES: i32 = 3;
/// Light (permille) at a wanderers' campfire.
pub const CAMPFIRE_LIGHT: u16 = 700;
/// Reach (tiles) of a campfire's light.
pub const CAMPFIRE_LIGHT_TILES: i32 = 7;
/// Light (permille) right at a pilot's helmet lamp.
pub const PILOT_LIGHT: u16 = 700;
/// Reach (tiles) of a pilot's helmet lamp.
pub const PILOT_LIGHT_TILES: i32 = 3;
/// Most night monsters alive at once (they share the enemy world cap too).
pub const MAX_NIGHT_MONSTERS: usize = 32;
/// Palisade tiles must stand within this many columns of a tile, inside the same claim, on
/// both sides for the claim to count as fenced there.
pub const PALISADE_REACH: i32 = 14;
/// Rows above and below a tile a fence post may stand at to guard it.
pub const PALISADE_ROWS: (i32, i32) = (3, 2);

/// Whether `kind` is a night monster (surface, dusk to dawn).
#[must_use]
pub const fn is_nocturnal(kind: EnemyKind) -> bool {
    matches!(
        kind,
        EnemyKind::ShadowHound
            | EnemyKind::DuneStalker
            | EnemyKind::WillOWisp
            | EnemyKind::MothSwarm
            | EnemyKind::UmbralAlpha
    )
}

/// How a night monster treats light.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LightResponse {
    /// Keeps out of lamp light and runs from flares.
    Shuns,
    /// Drawn to every light: circles lamps and flares, hunts whatever carries a lamp.
    Drawn,
    /// A light itself: does not care.
    Indifferent,
}

/// How `kind` treats light (deep enemies are [`LightResponse::Indifferent`]).
#[must_use]
pub const fn light_response(kind: EnemyKind) -> LightResponse {
    match kind {
        EnemyKind::ShadowHound | EnemyKind::DuneStalker | EnemyKind::UmbralAlpha => {
            LightResponse::Shuns
        }
        EnemyKind::MothSwarm => LightResponse::Drawn,
        _ => LightResponse::Indifferent,
    }
}

/// Whether `kind` feels footsteps through the ground (it hears anything walking or resting
/// on the surface within its hearing, not only drills).
#[must_use]
pub const fn feels_footsteps(kind: EnemyKind) -> bool {
    matches!(kind, EnemyKind::DuneStalker)
}

/// Ground a dune stalker swims through: dirt, clay and the soft surface crusts (sand,
/// sandstone, red rock, snow, mud), never built pieces, ore or hard rock.
#[must_use]
pub const fn stalker_ground(kind: TileKind) -> bool {
    matches!(
        kind,
        TileKind::Dirt
            | TileKind::Clay
            | TileKind::Surface(
                SurfaceMaterial::Sand
                    | SurfaceMaterial::Sandstone
                    | SurfaceMaterial::RedRock
                    | SurfaceMaterial::Snow
                    | SurfaceMaterial::Mud
            )
    )
}

/// The night monsters of `biome`, with relative spawn weights.
#[must_use]
pub const fn night_roster(biome: Biome) -> &'static [(EnemyKind, u32)] {
    use EnemyKind::{DuneStalker, MothSwarm, ShadowHound, WillOWisp};
    match biome {
        Biome::Plains => &[(ShadowHound, 4), (MothSwarm, 3), (WillOWisp, 1)],
        Biome::Forest => &[(MothSwarm, 4), (WillOWisp, 3), (ShadowHound, 3)],
        Biome::Desert => &[(DuneStalker, 5), (MothSwarm, 2), (ShadowHound, 1)],
        Biome::Badlands => &[(ShadowHound, 4), (DuneStalker, 3), (WillOWisp, 1)],
        Biome::Tundra => &[(ShadowHound, 5), (WillOWisp, 2)],
        Biome::Mountains => &[(ShadowHound, 3), (WillOWisp, 3), (MothSwarm, 1)],
        Biome::Wetlands => &[(WillOWisp, 5), (MothSwarm, 4)],
    }
}

/// How many of `kind` rise together (hounds hunt in packs).
#[must_use]
pub const fn pack_size(kind: EnemyKind, roll: u64) -> usize {
    match kind {
        EnemyKind::ShadowHound => {
            if roll.is_multiple_of(3) {
                3
            } else {
                2
            }
        }
        _ => 1,
    }
}

/// Spawn density of a night under `moon`, in percent: a full moon brings more monsters, a new
/// moon fewer (but the rare spawn).
#[must_use]
pub const fn moon_density_percent(moon: MoonPhase) -> u32 {
    match moon {
        MoonPhase::Full => 160,
        MoonPhase::WaxingGibbous | MoonPhase::WaningGibbous => 125,
        MoonPhase::FirstQuarter | MoonPhase::LastQuarter => 100,
        MoonPhase::WaxingCrescent | MoonPhase::WaningCrescent => 90,
        MoonPhase::New => 80,
    }
}

/// Extra night monsters allowed round each actor under `moon`.
#[must_use]
pub const fn moon_extra_cap(moon: MoonPhase) -> usize {
    match moon {
        MoonPhase::Full => 2,
        MoonPhase::WaxingGibbous | MoonPhase::WaningGibbous => 1,
        _ => 0,
    }
}

/// Whether a night under `moon` brings the rare spawn (the umbral alpha and its pack).
#[must_use]
pub const fn is_rare_spawn_night(moon: MoonPhase) -> bool {
    matches!(moon, MoonPhase::New)
}

/// Whether night monsters rise at `clock`: through the night and the second half of dusk.
#[must_use]
pub const fn monsters_rise(clock: WorldClock) -> bool {
    match clock.phase() {
        DayPhase::Night => true,
        DayPhase::Dusk => clock.phase_progress() >= 500,
        DayPhase::Dawn | DayPhase::Day => false,
    }
}

/// The night `clock` falls in, named by the day its dusk belongs to (`None` by day). A night
/// spans midnight, so after midnight it is still the previous day's night.
#[must_use]
pub const fn night_of(clock: WorldClock) -> Option<u32> {
    if !monsters_rise(clock) {
        return None;
    }
    if clock.slice() >= crate::clock::DUSK_START_SLICE {
        Some(clock.day())
    } else {
        Some(clock.day().saturating_sub(1))
    }
}

/// The depth tier of a night monster rising `distance` columns from town: farther out, tougher.
#[must_use]
pub const fn night_tier(distance: i32) -> u8 {
    match distance.unsigned_abs() / 1_200 {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        _ => 6,
    }
}

/// The night-only creature drops.
pub const NIGHT_MATERIALS: [StrategicResourceKind; 5] = [
    StrategicResourceKind::MothSilk,
    StrategicResourceKind::ShadowPelt,
    StrategicResourceKind::StalkerFang,
    StrategicResourceKind::WispEssence,
    StrategicResourceKind::UmbralHeart,
];

/// What the night market pays for one `material` at full price (0: not a night material).
#[must_use]
pub const fn night_material_value(material: StrategicResourceKind) -> u32 {
    match material {
        StrategicResourceKind::MothSilk => 45,
        StrategicResourceKind::ShadowPelt => 90,
        StrategicResourceKind::StalkerFang => 130,
        StrategicResourceKind::WispEssence => 150,
        StrategicResourceKind::UmbralHeart => 1_200,
        _ => 0,
    }
}

/// Whether `mineral` glows at night when it lies shallow under the open sky ("nocturnal ore").
#[must_use]
pub const fn glows_at_night(mineral: MineralKind) -> bool {
    matches!(
        mineral,
        MineralKind::Silver | MineralKind::Emerald | MineralKind::Diamond | MineralKind::Platinum
    )
}

/// Night facts the world keeps (saved, hashed and replicated inside the enemy director).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NightLedger {
    /// The night under way (named by its dusk's day, see [`night_of`]), while night monsters
    /// are about.
    #[serde(default)]
    pub active_night: Option<u32>,
    /// Night monsters killed during the active night.
    #[serde(default)]
    pub night_kills: u32,
    /// Night monsters killed in this world, by kind.
    #[serde(default)]
    pub kills_by_kind: BTreeMap<EnemyKind, u32>,
    /// Nights that came and went while monsters were about.
    #[serde(default)]
    pub nights_survived: u32,
    /// The last night the rare spawn walked (so it walks at most once a night).
    #[serde(default)]
    pub rare_night: Option<u32>,
}

impl NightLedger {
    /// Counts a night monster's death.
    pub fn record_kill(&mut self, kind: EnemyKind) {
        *self.kills_by_kind.entry(kind).or_insert(0) += 1;
        if self.active_night.is_some() {
            self.night_kills = self.night_kills.saturating_add(1);
        }
    }

    /// Night monsters of `kind` killed in this world.
    #[must_use]
    pub fn kills_of(&self, kind: EnemyKind) -> u32 {
        self.kills_by_kind.get(&kind).copied().unwrap_or(0)
    }
}

/// Story hooks a night offers (for T26's arc grammar): what an arc can ask of a player after
/// dark. Built from the ledger and events; [`night_hooks`] lists the ones a place offers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NightHook {
    /// Hunt `count` night monsters of a kind (count kills with [`NightLedger::kills_of`] or
    /// the `EnemyKilled` events of nocturnal kinds).
    Hunt { kind: EnemyKind, count: u32 },
    /// Bring back night-only materials (sold or handed in at a night market).
    Gather {
        material: StrategicResourceKind,
        count: u32,
    },
    /// Stay out on the open surface through a whole night (the `NightSurvived` event).
    SurviveTheNight,
    /// Bring down the rare spawn (new-moon nights, the `RareNightSpawn` event).
    SlayTheAlpha,
}

/// Hooks a night in `biome` under `moon` offers (the biome's commonest monster and its drop,
/// the night itself, and the alpha on a new moon).
#[must_use]
pub fn night_hooks(biome: Biome, moon: MoonPhase) -> Vec<NightHook> {
    let roster = night_roster(biome);
    let mut hooks = Vec::new();
    if let Some((kind, _)) = roster.iter().max_by_key(|(_, weight)| *weight) {
        hooks.push(NightHook::Hunt {
            kind: *kind,
            count: 3,
        });
        if let Some(material) = signature_drop(*kind) {
            hooks.push(NightHook::Gather { material, count: 2 });
        }
    }
    hooks.push(NightHook::SurviveTheNight);
    if is_rare_spawn_night(moon) {
        hooks.push(NightHook::SlayTheAlpha);
    }
    hooks
}

/// The night material `kind` is hunted for.
#[must_use]
pub const fn signature_drop(kind: EnemyKind) -> Option<StrategicResourceKind> {
    Some(match kind {
        EnemyKind::ShadowHound => StrategicResourceKind::ShadowPelt,
        EnemyKind::DuneStalker => StrategicResourceKind::StalkerFang,
        EnemyKind::WillOWisp => StrategicResourceKind::WispEssence,
        EnemyKind::MothSwarm => StrategicResourceKind::MothSilk,
        EnemyKind::UmbralAlpha => StrategicResourceKind::UmbralHeart,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{DayLength, WorldClock};

    #[test]
    fn every_biome_has_night_monsters_and_only_nocturnal_ones() {
        for biome in Biome::ALL {
            let roster = night_roster(biome);
            assert!(!roster.is_empty(), "{biome:?}");
            assert!(
                roster
                    .iter()
                    .all(|(kind, weight)| is_nocturnal(*kind) && *weight > 0)
            );
        }
        assert!(
            night_roster(Biome::Desert)
                .iter()
                .any(|(kind, _)| *kind == EnemyKind::DuneStalker)
        );
        assert!(
            night_roster(Biome::Wetlands)
                .iter()
                .all(|(kind, _)| *kind != EnemyKind::DuneStalker)
        );
        for kind in EnemyKind::ALL {
            assert_eq!(
                signature_drop(kind).is_some(),
                is_nocturnal(kind),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_moon_sets_the_numbers() {
        assert!(moon_density_percent(MoonPhase::Full) > moon_density_percent(MoonPhase::New));
        assert!(is_rare_spawn_night(MoonPhase::New));
        assert!(!is_rare_spawn_night(MoonPhase::Full));
        assert!(moon_extra_cap(MoonPhase::Full) > moon_extra_cap(MoonPhase::FirstQuarter));
    }

    #[test]
    fn nights_are_named_by_their_dusk() {
        let length = DayLength::Standard;
        let slice = length.slice_millis();
        // Day 1 at 21:00 (slice 84) and day 2 at 02:00 (slice 8) are both night 1.
        let evening = WorldClock::from_world_millis(84 * slice, length, 7);
        let small_hours = WorldClock::from_world_millis(length.millis() + 8 * slice, length, 7);
        let noon = WorldClock::from_world_millis(48 * slice, length, 7);
        assert_eq!(night_of(evening), Some(evening.day()));
        assert_eq!(night_of(small_hours), Some(evening.day()));
        assert_eq!(night_of(noon), None);
        // Early dusk is still quiet, late dusk stirs.
        let early_dusk = WorldClock::from_world_millis(73 * slice + 1, length, 7);
        let late_dusk = WorldClock::from_world_millis(77 * slice, length, 7);
        assert!(!monsters_rise(early_dusk));
        assert!(monsters_rise(late_dusk));
    }

    #[test]
    fn night_drops_are_valued_and_ores_glow() {
        for material in NIGHT_MATERIALS {
            assert!(night_material_value(material) > 0, "{material:?}");
        }
        assert_eq!(
            night_material_value(StrategicResourceKind::CoreShard),
            0,
            "deep materials are not night goods"
        );
        assert!(glows_at_night(MineralKind::Silver));
        assert!(!glows_at_night(MineralKind::Copper));
        assert_eq!(night_tier(0), 0);
        assert_eq!(night_tier(-100_000), 6);
    }

    #[test]
    fn the_ledger_counts_kills_and_round_trips() {
        let mut ledger = NightLedger::default();
        ledger.record_kill(EnemyKind::ShadowHound);
        assert_eq!(ledger.night_kills, 0, "no night under way");
        ledger.active_night = Some(3);
        ledger.record_kill(EnemyKind::ShadowHound);
        assert_eq!(ledger.kills_of(EnemyKind::ShadowHound), 2);
        assert_eq!(ledger.night_kills, 1);
        let json = serde_json::to_string(&ledger).expect("serializes");
        let back: NightLedger = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, ledger);
        let hooks = night_hooks(Biome::Desert, MoonPhase::New);
        assert!(hooks.contains(&NightHook::SlayTheAlpha));
        assert!(hooks.iter().any(|hook| matches!(
            hook,
            NightHook::Hunt {
                kind: EnemyKind::DuneStalker,
                ..
            }
        )));
    }
}
