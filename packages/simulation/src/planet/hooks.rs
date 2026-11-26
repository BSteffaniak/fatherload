//! The planet theme framework (Wave 12 bodylife): what a theme track implements to make a
//! world.
//!
//! A theme is two things:
//!
//! * its **data** ([`PlanetTheme`] in [`super::theme::PLANET_THEMES`]: palette, crust, strata,
//!   core ore, caves, hills), registered by id, and
//! * its **hooks** ([`PlanetThemeHooks`] in [`THEME_HOOKS`]): the surface shaper, the strata
//!   and ore table, extra tiles (landmarks, ruins, secrets), creature spawns, the weather
//!   table, settlement sites, climate and hazards, and the music bed.
//!
//! Every hook has a default that reads the data, so a theme registered as data alone plays
//! (the generic hooks, [`DataTheme`]); a theme track that wants more implements the trait on
//! its own type and appends `&MyTheme` to [`THEME_HOOKS`]. Generation
//! ([`super::PlanetGround`]) goes through the hooks for every tile, so a theme's world is a pure
//! function of the planet's params whatever it overrides.
//!
//! The Wave 11 placeholders (barren, rocky, ice) are ported onto it ([`BarrenTheme`],
//! [`RockyTheme`], [`IceTheme`]): the same ground, plus their weather, climate and music.
//!
//! **One secret each.** Craters and small moons (radius up to [`SMALL_BODY_RADIUS`]) get one
//! secret from the default [`PlanetThemeHooks::secret`]: a sealed vault a few rows under the
//! crust, lined with hard rock, holding an artifact and a seam of the theme's core ore
//! ([`SecretSite`]). Themes may place their own instead (or none).

use crate::{
    bodylife::weather::{WeatherId, WeatherOdds, odds},
    terrain::{ArtifactKind, MineralKind, TileKind},
};

use super::{
    PlanetGround,
    theme::{PLANET_THEMES, PlanetStratum, PlanetTheme, PlanetThemeId, theme},
};

/// Bodies up to this radius (tiles) are "small": craters and small moons, one secret each.
pub const SMALL_BODY_RADIUS: u32 = 1_000;
/// A secret vault's size (columns, rows).
pub const SECRET_VAULT: (i32, i32) = (9, 5);
/// Rows of rock over a secret vault, under the crust.
pub const SECRET_COVER_ROWS: i32 = 6;

/// A creature a theme's worlds spawn.
///
/// The wild and frontier themes list theirs (the id is the creature's `EnemyKind` code); their
/// own spawners place them through the enemy body rules in the planet's local frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CreatureSpawn {
    /// The creature's id (prefixed by its track's range).
    pub creature: u16,
    /// Its weight against the theme's other spawns.
    pub weight: u16,
    /// Rows below the surface it lives between (negative: in the sky).
    pub min_depth: i32,
    pub max_depth: i32,
    /// It comes out only at night.
    pub night_only: bool,
}

/// The music a theme's worlds play (the game maps the name to its patches; a theme track
/// adds its own bed under its prefix).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MusicBed {
    /// The bed's name (`"bodylife_barren"`, `"planets_a_jungle"`, ...).
    pub name: &'static str,
    /// Tempo (beats a minute) and how bright the mood is (0 sombre ..= 10 bright): the
    /// game's generic bed reads them when it has no patch of the name.
    pub tempo: u16,
    pub brightness: u8,
}

/// A theme's climate on top of the body's own numbers.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ThemeClimate {
    /// °C added at the core (a volcanic world runs hotter below).
    pub core_bias_c: i16,
}

/// Hazards a theme's ground carries (read by the bodylife world rules).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ThemeHazards {
    /// Suit damage a second to a pilot on foot outside (thousandths: corrosive air).
    pub corrosive_air_milli: u16,
}

/// A secret's place in the strip (see the module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecretSite {
    /// The vault's first column and first (top) row.
    pub column: i32,
    pub row: i32,
    /// Its size (columns, rows), walls included.
    pub width: i32,
    pub height: i32,
    /// What lies inside.
    pub artifact: ArtifactKind,
    pub ore: TileKind,
}

impl SecretSite {
    /// The tile the vault puts at `(column, row)` (columns already wrapped into the lap), if
    /// any: a hard-rock shell, open inside, the artifact on the floor's middle and ore along
    /// the floor.
    #[must_use]
    pub const fn tile_at(&self, column: i32, row: i32) -> Option<TileKind> {
        let dx = column - self.column;
        let dy = row - self.row;
        if dx < 0 || dy < 0 || dx >= self.width || dy >= self.height {
            return None;
        }
        let wall = dx == 0 || dy == 0 || dx == self.width - 1 || dy == self.height - 1;
        if wall {
            return Some(TileKind::HardRock);
        }
        if dy == self.height - 2 {
            if dx == self.width / 2 {
                return Some(TileKind::Artifact(self.artifact));
            }
            if dx % 2 == 1 {
                return Some(self.ore);
            }
        }
        Some(TileKind::Air)
    }

    /// The vault's middle (strip tile).
    #[must_use]
    pub const fn centre(&self) -> (i32, i32) {
        (self.column + self.width / 2, self.row + self.height / 2)
    }
}

/// The artifacts a secret vault may hold.
const SECRET_ARTIFACTS: [ArtifactKind; 4] = [
    ArtifactKind::VoidPearl,
    ArtifactKind::MachineSigil,
    ArtifactKind::CrystalPrism,
    ArtifactKind::RadiantShell,
];

/// What a theme implements (see the module docs). Every method has a default.
pub trait PlanetThemeHooks: Sync {
    /// The theme's id (its data in [`PLANET_THEMES`]).
    fn id(&self) -> PlanetThemeId;

    /// The theme's data.
    fn data(&self) -> &'static PlanetTheme {
        theme(self.id())
    }

    /// The surface shaper: hill rows above the datum at strip `column` (columns wrap; keep it
    /// periodic in the lap and within [`PlanetGround::max_hill_rows`]).
    fn surface_rows(&self, ground: &PlanetGround, column: i64) -> i32 {
        ground.rolling_hills(column)
    }

    /// The strata, top down (shares sum to 1,000).
    fn strata(&self) -> &'static [PlanetStratum] {
        self.data().strata
    }

    /// The ore at a tile of `stratum` for a roll in `0..10_000` (`None`: the stratum's rock).
    fn ore_at(&self, stratum: &PlanetStratum, roll: u16) -> Option<TileKind> {
        let mut threshold = 0_u16;
        for (mineral, chance) in stratum.ores {
            threshold = threshold.saturating_add(*chance);
            if roll < threshold {
                return Some(TileKind::Ore(*mineral));
            }
        }
        None
    }

    /// An extra tile at strip `(column, row)` (`depth` rows under the surface; negative in the
    /// sky) before the strata: landmarks, ruins, surface growth, the secret. The default
    /// places the secret.
    fn feature_tile(
        &self,
        ground: &PlanetGround,
        column: i32,
        row: i32,
        depth: i32,
    ) -> Option<TileKind> {
        let _ = depth;
        ground
            .secret()
            .and_then(|secret| secret.tile_at(column, row))
    }

    /// The body's secret, if it has one (default: one on every small body).
    fn secret(&self, ground: &PlanetGround) -> Option<SecretSite> {
        default_secret(ground)
    }

    /// Creatures its worlds spawn.
    fn creature_spawns(&self) -> &'static [CreatureSpawn] {
        &[]
    }

    /// How often each weather kind blows.
    fn weather_table(&self) -> &'static [WeatherOdds] {
        &DEFAULT_WEATHER
    }

    /// Strip columns where settlements stand, `count` of them spread round the body (default:
    /// the flattest stretches, [`flattest_sites`]).
    fn settlement_sites(&self, ground: &PlanetGround, count: usize) -> Vec<i32> {
        flattest_sites(ground, count)
    }

    /// The theme's own settlement site, if it levels one: `(middle column, width)` of the
    /// stretch it keeps flat, dry and clear for a town (the aliens build their town there,
    /// [`crate::alien::town::sites`]). `None`: towns spread round the body on generic ground.
    fn settlement_site(&self, ground: &PlanetGround) -> Option<(i32, i32)> {
        let _ = ground;
        None
    }

    /// The theme's climate on top of the body's numbers.
    fn climate(&self) -> ThemeClimate {
        ThemeClimate::default()
    }

    /// Hazards of its ground and air.
    fn hazards(&self) -> ThemeHazards {
        ThemeHazards::default()
    }

    /// The theme's whole-world generator, if it builds its chunks itself (the wild worlds,
    /// [`super::world::ThemeWorld`]); `None` keeps the generic generator over these hooks.
    fn world(&self) -> Option<&'static dyn super::world::ThemeWorld> {
        None
    }

    /// The hazard tags its worlds carry on top of their physical ones (each mild 1 or severe
    /// 2): the stations' planet gear counters them, the bar's gossip and the landing warning
    /// name them ([`crate::station::hazard::hazards_of_body`]).
    fn hazard_tags(&self) -> &'static [(crate::station::hazard::PlanetHazard, u8)] {
        &[]
    }

    /// Whether the theme's own world rules apply its hazards zone by zone (reading the planet
    /// gear through [`crate::station::hazard::gear_tier`]): the stations' flat per-second
    /// drain then stands back on its ground, so nothing bites twice.
    fn rules_own_hazards(&self) -> bool {
        false
    }

    /// Its music.
    fn music_bed(&self) -> MusicBed {
        MusicBed {
            name: "bodylife_generic",
            tempo: 72,
            brightness: 5,
        }
    }
}

/// Weather on a world whose theme gives no table: mostly clear, some storms, an aurora.
pub const DEFAULT_WEATHER: [WeatherOdds; 3] = [
    odds(WeatherId::CLEAR, 10),
    odds(WeatherId::STORM, 3),
    odds(WeatherId::AURORA, 2),
];

/// The default secret: on a small body, a vault under the crust at a seeded column.
#[must_use]
pub fn default_secret(ground: &PlanetGround) -> Option<SecretSite> {
    let params = ground.params();
    if params.radius > SMALL_BODY_RADIUS {
        return None;
    }
    let roll = super::hash3(params.seed, 77, 0, 0);
    let (vault_w, vault_h) = SECRET_VAULT;
    // Inside one lap (the vault never straddles the seam).
    let span = (ground.strip_width() - vault_w).max(1);
    #[allow(clippy::cast_possible_truncation, reason = "a column of the lap")]
    let column = (roll % u64::from(span.cast_unsigned())) as i32;
    // Its roof sits under the crust and some cover, under the lowest ground over it.
    let highest = (column..column + vault_w)
        .map(|x| super::BodyGround::surface_row(ground, x))
        .max()
        .unwrap_or(0);
    let row = highest + ground.data_theme().crust_rows + SECRET_COVER_ROWS;
    #[allow(clippy::cast_possible_truncation, reason = "an index under 4")]
    let artifact = SECRET_ARTIFACTS[((roll >> 32) % SECRET_ARTIFACTS.len() as u64) as usize];
    Some(SecretSite {
        column,
        row,
        width: vault_w,
        height: vault_h,
        artifact,
        ore: ground.data_theme().core_ore,
    })
}

/// `count` strip columns spread round the body, each the flattest 24-column stretch of its
/// share of the lap.
#[must_use]
pub fn flattest_sites(ground: &PlanetGround, count: usize) -> Vec<i32> {
    const STRETCH: i32 = 24;
    if count == 0 {
        return Vec::new();
    }
    let width = ground.strip_width();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "few sites"
    )]
    let share = (width / count as i32).max(STRETCH);
    (0..count)
        .map(|index| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_possible_wrap,
                reason = "few sites"
            )]
            let start = index as i32 * share;
            let mut best = (i32::MAX, start);
            let mut column = start;
            while column + STRETCH <= start + share {
                let rows: Vec<i32> = (column..column + STRETCH)
                    .map(|x| super::BodyGround::surface_row(ground, x))
                    .collect();
                let spread = rows.iter().max().unwrap_or(&0) - rows.iter().min().unwrap_or(&0);
                if spread < best.0 {
                    best = (spread, column + STRETCH / 2);
                }
                column += 8;
            }
            best.1.rem_euclid(width)
        })
        .collect()
}

/// The generic hooks of a theme registered as data alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataTheme(pub PlanetThemeId);

impl PlanetThemeHooks for DataTheme {
    fn id(&self) -> PlanetThemeId {
        self.0
    }
}

/// The barren placeholder (Wave 11), ported: airless grey moons and crater rocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarrenTheme;

impl PlanetThemeHooks for BarrenTheme {
    fn id(&self) -> PlanetThemeId {
        PlanetThemeId::BARREN
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        // Airless worlds have no weather (thin-aired ones see the odd aurora).
        const TABLE: [WeatherOdds; 2] = [odds(WeatherId::CLEAR, 12), odds(WeatherId::AURORA, 2)];
        &TABLE
    }

    fn music_bed(&self) -> MusicBed {
        MusicBed {
            name: "bodylife_barren",
            tempo: 56,
            brightness: 2,
        }
    }
}

/// The rocky placeholder (Wave 11), ported: dusty brown worlds with storms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RockyTheme;

impl PlanetThemeHooks for RockyTheme {
    fn id(&self) -> PlanetThemeId {
        PlanetThemeId::ROCKY
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        const TABLE: [WeatherOdds; 3] = [
            odds(WeatherId::CLEAR, 9),
            odds(WeatherId::STORM, 4),
            odds(WeatherId::AURORA, 1),
        ];
        &TABLE
    }

    fn climate(&self) -> ThemeClimate {
        ThemeClimate { core_bias_c: 40 }
    }

    fn music_bed(&self) -> MusicBed {
        MusicBed {
            name: "bodylife_rocky",
            tempo: 76,
            brightness: 5,
        }
    }
}

/// The ice placeholder (Wave 11), ported: snowfall, storms and bright auroras.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IceTheme;

impl PlanetThemeHooks for IceTheme {
    fn id(&self) -> PlanetThemeId {
        PlanetThemeId::ICE
    }

    fn weather_table(&self) -> &'static [WeatherOdds] {
        const TABLE: [WeatherOdds; 4] = [
            odds(WeatherId::CLEAR, 7),
            odds(WeatherId::SNOW, 5),
            odds(WeatherId::STORM, 2),
            odds(WeatherId::AURORA, 3),
        ];
        &TABLE
    }

    fn climate(&self) -> ThemeClimate {
        ThemeClimate { core_bias_c: -60 }
    }

    fn music_bed(&self) -> MusicBed {
        MusicBed {
            name: "bodylife_ice",
            tempo: 64,
            brightness: 7,
        }
    }
}

/// Themes with hooks of their own (append `&YourTheme` at the end; a theme missing here
/// plays with the generic hooks).
pub static THEME_HOOKS: &[&dyn PlanetThemeHooks] = &[
    &BarrenTheme,
    &RockyTheme,
    &IceTheme,
    // Wave 12 planets-a: the wild worlds.
    &super::wilds::hooks::JUNGLE_HOOKS,
    &super::wilds::hooks::OCEAN_HOOKS,
    &super::wilds::hooks::GLACIER_HOOKS,
    // Wave 12 planets-b: the frontier worlds.
    &crate::frontier::hooks::DUNE_HOOKS,
    &crate::frontier::hooks::VOLCANIC_HOOKS,
    &crate::frontier::hooks::CRYSTAL_HOOKS,
    // Wave 13 planets-c: the outland worlds.
    &super::outlands::hooks::FUNGAL_HOOKS,
    &super::outlands::hooks::MACHINE_HOOKS,
    &super::outlands::hooks::DEAD_HOOKS,
    // Wave 13 planets-d: the exotic worlds.
    &crate::exotic::hooks::GAS_GIANT_HOOKS,
    &crate::exotic::hooks::LOCKED_HOOKS,
    &crate::exotic::hooks::RING_HOOKS,
];

const fn data_hooks<const N: usize>() -> [DataTheme; N] {
    let mut out = [DataTheme(PlanetThemeId(0)); N];
    let mut index = 0;
    while index < N {
        #[allow(clippy::cast_possible_truncation, reason = "a few dozen themes")]
        {
            out[index] = DataTheme(PlanetThemeId(index as u16));
        }
        index += 1;
    }
    out
}

/// The generic hooks of every registered theme (by id).
static DATA_HOOKS: [DataTheme; PLANET_THEMES.len()] = data_hooks();

/// The hooks of theme `id`: its own if registered in [`THEME_HOOKS`], else the generic ones
/// (an unknown id reads as the first theme, like [`theme`]).
#[must_use]
pub fn hooks(id: PlanetThemeId) -> &'static dyn PlanetThemeHooks {
    if let Some(found) = THEME_HOOKS.iter().find(|hooks| hooks.id() == id) {
        return *found;
    }
    DATA_HOOKS
        .get(usize::from(id.0))
        .map_or(THEME_HOOKS[0], |data| data as &dyn PlanetThemeHooks)
}

/// The minerals a theme's ground is rich in (every stratum's ores and its core ore): what its
/// markets have plenty of ([`crate::bodylife::market::BodyPrices`]).
#[must_use]
pub fn common_minerals(id: PlanetThemeId) -> Vec<MineralKind> {
    let hooks = hooks(id);
    let mut out: Vec<MineralKind> = hooks
        .strata()
        .iter()
        .flat_map(|stratum| stratum.ores.iter().map(|(mineral, _)| *mineral))
        .collect();
    if let TileKind::Ore(mineral) = hooks.data().core_ore {
        out.push(mineral);
    }
    out.sort();
    out.dedup();
    out
}
