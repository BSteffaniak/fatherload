//! Living bodies (Wave 12 bodylife): clocks, tidal locking, weather, climate, prices and the
//! theme registry.

#![allow(
    clippy::float_cmp,
    reason = "pure functions compared bit for bit on purpose"
)]

use std::collections::BTreeSet;

use super::{
    BodyClimate, BodyClock, BodyLife, BodyPrices, LocalMarket, effective_gravity_milli,
    market::{COMMON_PERMILLE, MINERALS, SCARCE_PERMILLE},
    weather::{
        BODYLIFE_WEATHER, RAMP_MILLIS, SPELL_MILLIS, WeatherId, WeatherOdds, forecast,
        next_spell_of, odds, spell_kind, weather_kind, weather_kinds,
    },
};
use crate::{
    clock::DayPhase,
    planet::{
        BodyGround, PLANET_THEMES, PlanetGround, PlanetParams, PlanetThemeId,
        hooks::{self, PlanetThemeHooks, SMALL_BODY_RADIUS, THEME_HOOKS},
        theme,
    },
    space::Atmosphere,
    terrain::{MineralKind, TileKind},
};

const TABLE: [WeatherOdds; 4] = [
    odds(WeatherId::CLEAR, 6),
    odds(WeatherId::STORM, 4),
    odds(WeatherId::SNOW, 3),
    odds(WeatherId::AURORA, 2),
];

#[test]
fn each_body_keeps_its_own_clock() {
    let a = BodyClock::new(600, 2_560, 11);
    let b = BodyClock::new(1_800, 2_560, 12);
    assert!(!a.tidally_locked());
    // A day later, the same hour; half a day later, the other half of the day.
    for millis in [0, 12_345, 1_000_000] {
        assert_eq!(
            a.local_permille(millis, 40),
            a.local_permille(millis + 600_000, 40)
        );
        let half =
            (a.local_permille(millis + 300_000, 40) + 1_000 - a.local_permille(millis, 40)) % 1_000;
        assert!((499..=501).contains(&half), "half a day moves {half}");
    }
    // Two bodies seen at the same moment read different hours (their days and phases differ).
    let differs = (0..20).any(|step| {
        let millis = step * 97_000;
        a.local_permille(millis, 0) != b.local_permille(millis, 0)
    });
    assert!(differs);
    // Round the body the hour advances with the column: a quarter lap is a quarter day.
    let now = 77_000;
    let quarter = (a.local_permille(now, 640) + 1_000 - a.local_permille(now, 0)) % 1_000;
    assert_eq!(quarter, 250);
    // Columns wrap.
    assert_eq!(a.local_permille(now, 5), a.local_permille(now, 5 + 2_560));
    assert_eq!(a.local_permille(now, -5), a.local_permille(now, 2_555));
    // Noon is bright, midnight dark; phases follow the home clock's slices.
    let noon = a.column_at(now, 500);
    let midnight = a.column_at(now, 0);
    assert!(a.daylight(now, noon, 120) > 990);
    assert_eq!(a.daylight(now, midnight, 120), 120);
    assert_eq!(a.phase(now, noon), DayPhase::Day);
    assert_eq!(a.phase(now, midnight), DayPhase::Night);
    assert_eq!(a.hours_minutes(now, noon).0, 12);
    // The sun stands overhead at noon and a quarter turn round at the terminators.
    assert!(a.sun_angle(now, noon).abs() < 0.01);
    let dusk = a.column_at(now, 750);
    assert!((a.sun_angle(now, dusk) + std::f64::consts::FRAC_PI_2).abs() < 0.01);
    assert!(a.elevation(now, dusk).abs() < 0.01);
    // The moment a column reaches an hour.
    for (column, permille) in [(0, 250), (900, 0), (2_000, 777)] {
        let when = a.millis_when(123_456, column, permille);
        assert!((123_456..123_456 + 600_000).contains(&when));
        assert_eq!(a.local_permille(when, column), permille);
    }
    // The local day turns over at local midnight.
    let before = a.local_day(599_999 - a.phase_millis % 600_000, 0);
    assert!(a.local_day(1_200_000, 0) > before);
}

#[test]
fn a_tidally_locked_body_keeps_an_eternal_day_and_night_side() {
    let clock = BodyClock::new(0, 4_096, 99);
    assert!(clock.tidally_locked());
    for millis in [0, 5_000, 86_400_000, 1_234_567_890] {
        // Column 0 faces the star forever; half a lap round is midnight forever.
        assert_eq!(clock.local_permille(millis, 0), 500);
        assert_eq!(clock.local_permille(millis, 2_048), 0);
        assert!(clock.daylight(millis, 0, 140) >= 999);
        assert_eq!(clock.daylight(millis, 2_048, 140), 140);
        // The terminators lie a quarter lap either way.
        assert!(clock.elevation(millis, 1_024).abs() < 1e-9);
        assert!(clock.elevation(millis, 3_072).abs() < 1e-9);
        assert_eq!(clock.day(millis), 0);
    }
    // Its sides swing further: the day side bakes and the night side freezes.
    let climate = BodyClimate {
        mean_c: 10,
        atmosphere: Atmosphere::Thin,
        gravity_milli: 900,
        depth_rows: 2_000,
        tidally_locked: true,
        core_bias_c: 0,
    };
    let spinning = BodyClimate {
        tidally_locked: false,
        ..climate
    };
    let day = climate.temperature_at(0, clock.elevation(0, 0), 0);
    let night = climate.temperature_at(0, clock.elevation(0, 2_048), 0);
    assert!(day - night > spinning.temperature_at(0, 1.0, 0) - spinning.temperature_at(0, -1.0, 0));
}

#[test]
fn weather_is_a_pure_function_of_seed_table_air_and_time() {
    let seed = 0xBEEF;
    let mut kinds = BTreeSet::new();
    for spell in 0..400 {
        let millis = spell * SPELL_MILLIS + SPELL_MILLIS / 2;
        let a = forecast(seed, &TABLE, Atmosphere::Breathable, millis, true);
        let b = forecast(seed, &TABLE, Atmosphere::Breathable, millis, true);
        assert_eq!(a, b);
        kinds.insert(a.kind);
    }
    // Every kind of the table comes up over a few hundred spells.
    assert_eq!(kinds.len(), TABLE.len());
    // Another seed tells another story.
    let differs = (0..50).any(|spell| {
        spell_kind(seed, &TABLE, Atmosphere::Breathable, spell)
            != spell_kind(seed + 1, &TABLE, Atmosphere::Breathable, spell)
    });
    assert!(differs);
    // Nothing that needs air comes to an airless body.
    for spell in 0..200 {
        assert_eq!(
            spell_kind(seed, &TABLE, Atmosphere::Vacuum, spell),
            WeatherId::CLEAR
        );
    }
    // An aurora shows only in the dark.
    let aurora = next_spell_of(seed, &TABLE, Atmosphere::Thin, WeatherId::AURORA, 0, 500)
        .expect("an aurora");
    let middle = aurora - aurora % SPELL_MILLIS + SPELL_MILLIS / 2;
    assert_eq!(
        forecast(seed, &TABLE, Atmosphere::Thin, middle, true).kind,
        WeatherId::AURORA
    );
    assert!(forecast(seed, &TABLE, Atmosphere::Thin, middle, false).is_clear());
    // The registry: ids unique, the generic kinds registered, unknown ids clear.
    let ids: BTreeSet<_> = weather_kinds().map(|kind| kind.id).collect();
    assert_eq!(ids.len(), weather_kinds().count());
    assert_eq!(BODYLIFE_WEATHER.len(), 4);
    assert_eq!(weather_kind(WeatherId(60_000)).id, WeatherId::CLEAR);
    for kind in weather_kinds() {
        assert!(kind.id.0 >= 256 || kind.track == "bodylife" || kind.id.0 >= 64);
        assert_eq!(weather_kind(kind.id), kind);
    }
}

#[test]
fn weather_fades_in_and_out_and_its_effects_follow_its_strength() {
    let seed = 7;
    let storm = next_spell_of(
        seed,
        &TABLE,
        Atmosphere::Breathable,
        WeatherId::STORM,
        0,
        500,
    )
    .expect("a storm");
    let spell = storm / SPELL_MILLIS;
    // Fading: the strength never jumps more than a tick's share of the ramp.
    let mut last: Option<u16> = None;
    let start = spell * SPELL_MILLIS;
    for millis in (start.saturating_sub(SPELL_MILLIS)..start + 2 * SPELL_MILLIS).step_by(500) {
        let now = forecast(seed, &TABLE, Atmosphere::Breathable, millis, false);
        let strength = if now.kind == WeatherId::STORM {
            now.intensity_permille
        } else {
            0
        };
        if let Some(before) = last {
            let step = u64::from(before.abs_diff(strength));
            assert!(
                step <= 500 * 1_000 / RAMP_MILLIS + 1,
                "jump {before} -> {strength}"
            );
        }
        last = Some(strength);
    }
    // At full strength a storm blows hard, hides the view, darkens and stings.
    let full = (start..start + SPELL_MILLIS)
        .step_by(1_000)
        .map(|millis| forecast(seed, &TABLE, Atmosphere::Breathable, millis, false))
        .find(|now| now.intensity_permille == 1_000)
        .expect("full strength");
    let info = weather_kind(WeatherId::STORM);
    assert!(full.wind.abs() >= f32::from(info.wind) - 0.5);
    assert!(full.wind.abs() <= f32::from(info.wind + info.gust) + 0.5);
    assert_eq!(full.visibility_permille, info.visibility_permille);
    assert!(full.light_permille < 0);
    assert!(full.hull_damage > 0.0);
    // Thin air pushes less, thick air more.
    let thin = forecast(
        seed,
        &TABLE,
        Atmosphere::Thin,
        storm + SPELL_MILLIS / 2,
        false,
    );
    let thick = forecast(
        seed,
        &TABLE,
        Atmosphere::Thick,
        storm + SPELL_MILLIS / 2,
        false,
    );
    if thin.kind == WeatherId::STORM && thick.kind == WeatherId::STORM {
        assert!(thin.wind.abs() < thick.wind.abs());
    }
}

#[test]
fn temperature_air_and_pressure_follow_the_star_the_air_and_the_depth() {
    let climate = BodyClimate {
        mean_c: -20,
        atmosphere: Atmosphere::Vacuum,
        gravity_milli: 400,
        depth_rows: 360,
        tidally_locked: false,
        core_bias_c: 0,
    };
    let thick = BodyClimate {
        atmosphere: Atmosphere::Thick,
        ..climate
    };
    // Day is warmer than night, far more so on bare rock than under a thick sky.
    let swing = |c: &BodyClimate| c.temperature_at(0, 1.0, 0) - c.temperature_at(0, -1.0, 0);
    assert!(swing(&climate) > 100);
    assert!(swing(&thick) < 20);
    // Underground the swing fades and the heat rises toward the core.
    assert_eq!(
        climate.temperature_at(30, 1.0, 0),
        climate.temperature_at(30, -1.0, 0)
    );
    let mut last = climate.temperature_at(30, 0.0, 0);
    for depth in (40..=360).step_by(20) {
        let now = climate.temperature_at(depth, 0.0, 0);
        assert!(now >= last, "warmer with depth");
        last = now;
    }
    assert_eq!(climate.temperature_at(360, 0.0, 0), climate.core_c());
    // Heat and cold levels.
    assert_eq!(BodyClimate::heat_units(20), 0);
    assert_eq!(BodyClimate::heat_units(70 + 45), 3);
    assert_eq!(BodyClimate::cold_levels(-30), 0);
    assert_eq!(BodyClimate::cold_levels(-41), 1);
    assert!(BodyClimate::cold_levels(-200) <= crate::environment::MAX_COLD);
    // Air: breathable only under a breathable sky, and not while the weather fouls it.
    assert!(!climate.reading(0, 0.0, 0, false).breathable);
    let earthlike = BodyClimate {
        atmosphere: Atmosphere::Breathable,
        ..climate
    };
    assert!(earthlike.reading(0, 0.0, 0, false).breathable);
    assert!(!earthlike.reading(0, 0.0, 0, true).breathable);
    // Pressure: none in vacuum, rising with depth and gravity under air.
    assert_eq!(climate.pressure_at(100), 0);
    assert!(earthlike.pressure_at(100) > earthlike.pressure_at(0));
    let heavy = BodyClimate {
        gravity_milli: 2_000,
        ..earthlike
    };
    assert!(heavy.pressure_at(0) > earthlike.pressure_at(0));
    // The weather's cold reaches the surface, not the deep.
    assert!(earthlike.temperature_at(0, 0.0, -28) < earthlike.temperature_at(0, 0.0, 0));
    assert_eq!(
        earthlike.temperature_at(200, 0.0, -28),
        earthlike.temperature_at(200, 0.0, 0)
    );
}

#[test]
fn every_body_has_its_own_prices_in_universal_credits() {
    let common = [MineralKind::Iron, MineralKind::Copper];
    let near = BodyPrices::new(42, &common, 0);
    let far = BodyPrices::new(42, &common, 6);
    // Its own ore is cheap there, what it lacks is dear, dearer far from home.
    assert!(near.mineral_permille(MineralKind::Iron) < 1_000);
    assert!(near.mineral_permille(MineralKind::Iron) <= COMMON_PERMILLE * 108 / 100);
    assert!(near.mineral_permille(MineralKind::Gold) >= SCARCE_PERMILLE * 92 / 100);
    assert!(far.mineral_permille(MineralKind::Gold) > near.mineral_permille(MineralKind::Gold));
    assert_eq!(
        far.mineral_permille(MineralKind::Iron),
        near.mineral_permille(MineralKind::Iron)
    );
    assert_eq!(BodyPrices::new(42, &common, 0), near);
    assert_ne!(BodyPrices::new(43, &common, 0), near);
    assert!(near.mineral_price(MineralKind::Gold) > MineralKind::Gold.value());
    // A settlement's market: two wanted goods a local day, a glut that is never wanted.
    let mut days = BTreeSet::new();
    for day in 0..20 {
        let market = LocalMarket::new(near.clone(), 7, day);
        let wanted = market.wanted();
        assert_eq!(wanted.len(), 2);
        assert_ne!(wanted[0], wanted[1]);
        assert!(!wanted.contains(&market.glut()));
        days.insert(wanted.clone());
        for mineral in &wanted {
            assert!(market.mineral_permille(*mineral) > near.mineral_permille(*mineral));
        }
        assert!(market.mineral_permille(market.glut()) < near.mineral_permille(market.glut()));
        assert_eq!(market, LocalMarket::new(near.clone(), 7, day));
    }
    assert!(days.len() > 5, "the boards change from day to day");
    // Two settlements on one body disagree.
    let a = LocalMarket::new(near.clone(), 1, 3);
    let b = LocalMarket::new(near, 2, 3);
    assert!(
        MINERALS
            .iter()
            .any(|mineral| a.mineral_price(*mineral) != b.mineral_price(*mineral))
    );
}

#[test]
fn the_theme_registry_round_trips_and_every_theme_has_hooks() {
    for data in &PLANET_THEMES {
        let found = hooks::hooks(data.id);
        assert_eq!(found.id(), data.id);
        assert_eq!(found.data(), theme(data.id));
        let shares: u32 = found
            .strata()
            .iter()
            .map(|stratum| u32::from(stratum.share_permille))
            .sum();
        assert_eq!(shares, 1_000);
        assert!(!found.weather_table().is_empty());
        assert!(!found.music_bed().name.is_empty());
        assert!(!hooks::common_minerals(data.id).is_empty());
    }
    // The ported placeholders have hooks of their own; ids are unique.
    let ids: BTreeSet<_> = THEME_HOOKS.iter().map(|hooks| hooks.id()).collect();
    assert_eq!(ids.len(), THEME_HOOKS.len());
    assert!(ids.contains(&PlanetThemeId::BARREN));
    assert!(ids.contains(&PlanetThemeId::ROCKY));
    assert!(ids.contains(&PlanetThemeId::ICE));
    assert_eq!(hooks::hooks(PlanetThemeId(900)).id(), PlanetThemeId::BARREN);
    // A theme registered as data alone plays with the generic hooks.
    let generic = hooks::DataTheme(PlanetThemeId::ROCKY);
    assert_eq!(generic.strata(), theme(PlanetThemeId::ROCKY).strata);
    assert_eq!(generic.weather_table(), &hooks::DEFAULT_WEATHER);
}

#[test]
fn craters_and_small_moons_hold_one_secret_each() {
    let small = PlanetGround::new(PlanetParams::new(0x005E_C2E7, 480, PlanetThemeId::BARREN));
    let secret = small.secret().expect("a small body keeps a secret");
    // The vault is under the crust, whole, holding its artifact and ore.
    let mut artifacts = 0;
    let mut ores = 0;
    for dy in 0..secret.height {
        for dx in 0..secret.width {
            let tile = small
                .generated_tile(secret.column + dx, secret.row + dy)
                .kind;
            assert_eq!(
                Some(tile),
                secret.tile_at(secret.column + dx, secret.row + dy)
            );
            match tile {
                TileKind::Artifact(found) => {
                    assert_eq!(found, secret.artifact);
                    artifacts += 1;
                }
                kind if kind == secret.ore => ores += 1,
                _ => {}
            }
        }
    }
    assert_eq!(artifacts, 1);
    assert!(ores >= 2);
    for dx in 0..secret.width {
        let surface = small.surface_row(secret.column + dx);
        assert!(secret.row > surface + small.theme().crust_rows);
    }
    // Generation stays a pure function of the params.
    let again = PlanetGround::new(*small.params());
    assert_eq!(again.secret(), small.secret());
    // A planet keeps none (its theme places its own landmarks).
    let big = PlanetGround::new(PlanetParams::new(
        0x005E_C2E7,
        SMALL_BODY_RADIUS + 600,
        PlanetThemeId::ROCKY,
    ));
    assert!(big.secret().is_none());
    // Settlement sites: spread round the lap, each on flat ground.
    let sites = small.hooks().settlement_sites(&small, 4);
    assert_eq!(sites.len(), 4);
    let unique: BTreeSet<_> = sites.iter().collect();
    assert_eq!(unique.len(), 4);
    for site in sites {
        assert!((0..small.strip_width()).contains(&site));
    }
}

#[test]
fn gravity_is_one_number_per_body() {
    assert_eq!(effective_gravity_milli(120), 250);
    assert_eq!(effective_gravity_milli(900), 900);
    assert_eq!(effective_gravity_milli(3_000), 2_500);
    let params = PlanetParams {
        gravity_milli: 140,
        ..PlanetParams::new(1, 600, PlanetThemeId::BARREN)
    };
    let life = BodyLife::of_planet(params);
    // The life's gravity is the ground's (the field zone's strength).
    assert_eq!(f32::from(life.gravity_milli()) / 1_000.0, params.gravity());
}
