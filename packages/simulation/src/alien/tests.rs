//! Unit tests of the alien species, towns and tongues (Wave 12 aliens track).

use super::{
    AlienSpecies, AlienTile,
    atlas::{first_home, folk_of_body},
    language::{
        FULL_UNDERSTANDING, Fluency, readable_permille, rendered_text, speech_tokens,
        word_difficulty,
    },
    quest::{CHAPTER_NEEDS, chapters},
    town::{self, AlienBuildingKind, is_alien_town, town_by_id, towns_of},
    trade::{AlienGood, good_price, ore_price},
};
use crate::{
    planet::{BodyGround, PlanetGround, PlanetParams, PlanetThemeId},
    space::Space,
    terrain::{MineralKind, TileKind},
    worldgen::DEFAULT_WORLD_SEED,
};

fn settled(seed: u64, radius: u32, species: AlienSpecies) -> PlanetParams {
    PlanetParams {
        folk: species.folk(),
        ..PlanetParams::new(seed, radius, PlanetThemeId::ROCKY)
    }
}

#[test]
fn every_species_has_a_first_home_near_home_on_a_planet_it_likes() {
    for seed in [DEFAULT_WORLD_SEED, 7, 0x00C0_FFEE] {
        let space = Space::shared(crate::space::SpaceSeed::from(seed));
        let mut homes = Vec::new();
        for species in AlienSpecies::ALL {
            let id = first_home(seed, species).expect("a first home");
            assert!(
                id.sector.chebyshev(crate::space::HOME_SECTOR) <= 1,
                "{species:?} lives near home"
            );
            let body = space.body(id).expect("a real body");
            assert_eq!(body.kind, crate::space::BodyKind::Planet);
            assert_eq!(folk_of_body(seed, id, &body), species.folk());
            assert!(!homes.contains(&id), "one species per first home");
            homes.push(id);
            // The body's params (what the store generates) name the species.
            let params = crate::place::planet_params_for(crate::space::SpaceSeed::from(seed), id)
                .expect("landable");
            assert_eq!(AlienSpecies::from_folk(params.folk), Some(species));
            // A wild world (its own whole ground) holds one town on its settlement site; every
            // other planet two or more.
            let wild = crate::planet::hooks::hooks(params.theme).world().is_some();
            assert!(
                towns_of(&params).len() >= if wild { 1 } else { 2 },
                "a first home has two towns or more"
            );
        }
        // Deterministic.
        assert_eq!(
            super::atlas::first_homes(seed),
            super::atlas::compute_first_homes_for_tests(seed)
        );
    }
}

#[test]
fn towns_are_level_paved_spread_round_the_ring_and_deterministic() {
    for species in AlienSpecies::ALL {
        let params = settled(0x7A11_0000 + u64::from(species.index()), 1_800, species);
        let ground = PlanetGround::new(params);
        let width = ground.strip_width();
        let towns = towns_of(&params);
        assert!(towns.len() >= 2);
        assert_eq!(towns_of(&params).len(), towns.len());
        let again = towns_of(&params);
        for (a, b) in towns.iter().zip(again.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.buildings, b.buildings);
            assert_eq!(a.population.characters.len(), b.population.characters.len());
        }
        let mut ids = Vec::new();
        for found in towns.iter() {
            assert!(is_alien_town(found.id));
            assert!(!ids.contains(&found.id));
            ids.push(found.id);
            assert_eq!(found.species, species);
            // Never across the seam.
            assert!(found.first_column() > 0 && found.last_column() < width);
            // Level and paved across the span.
            for column in found.first_column()..=found.last_column() {
                assert_eq!(ground.surface_row(column), found.ground_row());
                let tile = ground.generated_tile(column, found.ground_row());
                assert_eq!(tile.kind, TileKind::Alien(AlienTile::paving(species)));
                let above = ground.generated_tile(column, found.ground_row() - 1);
                assert!(above.kind.is_open(), "open sky over the town");
            }
            // Lexicon stones under it.
            for (column, row) in found.site.stones {
                assert_eq!(
                    ground.generated_tile(column, row).kind,
                    TileKind::Alien(AlienTile::LexiconStone(species))
                );
            }
            // Its own buildings, inside its span, not overlapping.
            for kind in [
                AlienBuildingKind::Landmark,
                AlienBuildingKind::TradeHall,
                AlienBuildingKind::ElderHouse,
                AlienBuildingKind::LexiconHouse,
                AlienBuildingKind::Dwelling,
                AlienBuildingKind::HomePlot,
            ] {
                assert!(found.building(kind).is_some(), "{species:?} {kind:?}");
            }
            for pair in found.buildings.windows(2) {
                assert!(pair[0].last_column() < pair[1].first_column);
            }
            for building in &found.buildings {
                assert!(building.first_column >= found.first_column());
                assert!(building.last_column() <= found.last_column());
            }
            // Residents of the species with the trades the rules ask for.
            assert!(found.population.characters.len() >= 6);
            for (index, character) in found.population.characters.iter().enumerate() {
                assert_eq!(character.culture, species.culture());
                assert_eq!(character.role, species.def().trades[index]);
                assert_eq!(character.id.village, found.id);
            }
            // Ids resolve back, and the routine view keeps everyone in town.
            assert_eq!(town_by_id(found.id).expect("registered").name, found.name);
            let settlement = town::settlement(found.id).expect("a settlement of the town");
            assert_eq!(
                settlement.residents.len(),
                found.population.characters.len()
            );
            for index in 0..settlement.residents.len() {
                let stop = settlement
                    .stop_at(index, crate::people::DayTime::at(2, 11, 0))
                    .expect("a stop");
                let column = settlement.place_column(stop.place);
                assert!((found.first_column() - 4..=found.last_column() + 4).contains(&column));
            }
        }
        // Spread round the ring: no two towns closer than a third of a lap.
        for pair in towns.windows(2) {
            assert!(pair[1].site.centre - pair[0].site.centre > width / 4);
        }
    }
    // An unsettled planet has none, and its ground is untouched.
    let bare = PlanetParams::new(0x7A11_0000, 1_800, PlanetThemeId::ROCKY);
    assert!(towns_of(&bare).is_empty());
    let settled_ground = PlanetGround::new(settled(0x7A11_0000, 1_800, AlienSpecies::Vessi));
    let bare_ground = PlanetGround::new(bare);
    let far = settled_ground.town_sites()[0].expect("a site").centre + 300;
    assert_eq!(
        bare_ground.surface_row(far),
        settled_ground.surface_row(far),
        "away from the towns the hills are the same"
    );
}

#[test]
fn a_moon_holds_one_town_and_ids_never_collide_with_other_settlements() {
    let params = settled(0x4D00_4E00, 400, AlienSpecies::Siphra);
    let towns = towns_of(&params);
    assert_eq!(towns.len(), 1);
    let id = towns[0].id;
    assert!(!crate::people::camps::is_camp(id));
    assert!(!crate::worldgen::sky::continent::is_sky_city(id));
    assert!(crate::village::placement::decode_sky_town(id).is_none());
    assert!(!crate::people::colony::is_colony(id));
    assert!(crate::people::deep_folk::DeepSettlement::of_id(id).is_none());
    assert!(!is_alien_town(crate::people::colony::LUNAR_COLONY));
    assert!(!is_alien_town(crate::people::deep_folk::HEARTS_EDGE));
    assert!(!is_alien_town(crate::village::placement::surface_id(-900)));
}

#[test]
fn speech_starts_as_glyphs_and_reads_as_understanding_grows() {
    let line = "Bring us five copper, the warm metal, and we will sing you into the town.";
    for species in AlienSpecies::ALL {
        let none = speech_tokens(line, species, 0);
        assert!(
            none.iter().all(|token| !token.known()),
            "{species:?}: all glyphs at first"
        );
        let all = speech_tokens(line, species, FULL_UNDERSTANDING);
        assert!(all.iter().all(super::language::SpeechToken::known));
        assert_eq!(rendered_text(line, species, FULL_UNDERSTANDING), line);
        // Monotonic: more understanding never hides a word.
        let mut last = 0;
        for understanding in (0..=1_000).step_by(50) {
            let readable = readable_permille(line, species, understanding);
            assert!(readable >= last);
            last = readable;
        }
        assert!(readable_permille(line, species, 300) > 0);
        assert!(readable_permille(line, species, 300) < 1_000);
        // Glyphs stay in the species' alphabet and are the same every time.
        for token in &none {
            if let super::language::SpeechToken::Glyphs { glyphs, .. } = token {
                assert!(glyphs.iter().all(|glyph| *glyph < species.def().glyphs));
            }
        }
        assert_eq!(speech_tokens(line, species, 0), none);
    }
    // Names and numbers read from the start; short words before long ones on average.
    assert_eq!(word_difficulty("Hollis", AlienSpecies::Vessi, false), 0);
    assert_eq!(word_difficulty("42", AlienSpecies::Vessi, true), 0);
    assert!(word_difficulty("us", AlienSpecies::Tokkari, true) < 300);
    assert!(word_difficulty("understanding", AlienSpecies::Tokkari, true) >= 480);
    // The same word reads differently hard in different tongues.
    let words = ["warm", "metal", "town", "song", "stone", "memory"];
    assert!(words.iter().any(|word| {
        word_difficulty(word, AlienSpecies::Vessi, true)
            != word_difficulty(word, AlienSpecies::Siphra, true)
    }));
    assert_eq!(Fluency::of(0), Fluency::Glyphs);
    assert_eq!(Fluency::of(CHAPTER_NEEDS[4]), Fluency::Conversant);
    assert_eq!(Fluency::of(1_000), Fluency::Native);
}

#[test]
fn each_species_buys_its_ores_and_sells_its_goods_better_to_good_speakers() {
    for species in AlienSpecies::ALL {
        let def = species.def();
        for (ore, _) in def.valued_ores {
            let stranger = ore_price(species, *ore, 100, 1_000, Fluency::Glyphs).expect("buys");
            let fluent = ore_price(species, *ore, 100, 1_000, Fluency::Fluent).expect("buys");
            assert!(fluent > stranger);
            assert!(stranger >= 100, "a prized ore pays at least the market");
        }
        for good in def.goods {
            assert!(
                good_price(*good, 1_000, Fluency::Native)
                    < good_price(*good, 1_000, Fluency::Glyphs)
            );
        }
        // A primer of their own tongue is always on sale.
        assert!(def.goods.contains(&AlienGood::Primer(species)));
        // Chapters ask for ores they prize or plain ones, and gate on understanding.
        let arc = chapters(species);
        for (index, chapter) in arc.iter().enumerate() {
            assert_eq!(chapter.needs, CHAPTER_NEEDS[index]);
            assert!(!chapter.ask.is_empty() && !chapter.thanks.is_empty());
        }
    }
    // Each species buys what the others do not lead with.
    assert!(
        ore_price(
            AlienSpecies::Vessi,
            MineralKind::Iron,
            100,
            1_000,
            Fluency::Native
        )
        .is_none()
    );
    assert!(
        ore_price(
            AlienSpecies::Tokkari,
            MineralKind::Iron,
            100,
            1_000,
            Fluency::Native
        )
        .is_some()
    );
}

/// A settled wild world holds several towns (Wave 13 QA): a planet two or three, spread round
/// the lap on its own level, paved ground (the jungle's clearings, the archipelago's islands,
/// the glacier's shelves), clear of the landmark half a lap away; a moon still one. The
/// unsettled worlds' ground (the golden digests) does not move.
#[test]
fn a_settled_wild_planet_holds_several_level_towns_spread_round_the_lap() {
    use crate::planet::wilds::{landmark_column, settlement_centres, wrap_delta};
    let mut seen_three = false;
    for theme in [
        PlanetThemeId::JUNGLE,
        PlanetThemeId::OCEAN,
        PlanetThemeId::GLACIER,
    ] {
        for (salt, species) in AlienSpecies::ALL.into_iter().enumerate() {
            for radius in [1_600_u32, 2_400, 4_000] {
                let params = PlanetParams {
                    folk: species.folk(),
                    ..PlanetParams::new(
                        0x7A11_1300 + u64::from(radius) * 8 + salt as u64,
                        radius,
                        theme,
                    )
                };
                let ground = PlanetGround::new(params);
                let width = i64::from(ground.strip_width());
                let towns = towns_of(&params);
                let count = usize::from(town::town_count(&params));
                assert!(count >= 2, "a planet holds two towns or more");
                assert_eq!(
                    towns.len(),
                    count,
                    "{theme:?} R {radius}: every town stands"
                );
                seen_three |= count == 3;
                assert_eq!(settlement_centres(&ground).len(), count);
                let landmark = landmark_column(&ground);
                for (index, found) in towns.iter().enumerate() {
                    assert_eq!(usize::from(found.site.index), index);
                    // Level, paved, open sky over it.
                    for column in found.first_column()..=found.last_column() {
                        assert_eq!(
                            ground.surface_row(column),
                            found.ground_row(),
                            "{theme:?} R {radius} town {index} is level at column {column}"
                        );
                        let tile = ground.generated_tile(column, found.ground_row());
                        assert_eq!(tile.kind, TileKind::Alien(AlienTile::paving(species)));
                        let above = ground.generated_tile(column, found.ground_row() - 1);
                        assert!(above.kind.is_open(), "open sky over the town");
                    }
                    // On the ocean, an island over the sea.
                    if theme == PlanetThemeId::OCEAN {
                        assert!(found.ground_row() < crate::planet::wilds::ocean::SEA_LEVEL);
                    }
                    // Clear of the landmark.
                    let off = wrap_delta(i64::from(found.site.centre), landmark, width).abs();
                    assert!(off > width / 16, "town {index} keeps off the landmark");
                    // Its lexicon stones under it.
                    for (column, row) in found.site.stones {
                        assert_eq!(
                            ground.generated_tile(column, row).kind,
                            TileKind::Alien(AlienTile::LexiconStone(species)),
                            "{theme:?} R {radius} town {index} stone at {column},{row}"
                        );
                    }
                }
                // Spread round the lap.
                for (a, first) in towns.iter().enumerate() {
                    for second in towns.iter().skip(a + 1) {
                        let apart = wrap_delta(
                            i64::from(first.site.centre),
                            i64::from(second.site.centre),
                            width,
                        )
                        .abs();
                        assert!(apart >= width / 8, "towns spread round the lap");
                    }
                }
            }
        }
        // A settled wild moon keeps its one town.
        let moon = PlanetParams {
            folk: AlienSpecies::ALL[0].folk(),
            ..PlanetParams::new(0x7A11_1399, 700, theme)
        };
        assert_eq!(towns_of(&moon).len(), 1);
    }
    assert!(seen_three, "some planet holds three towns");
}
