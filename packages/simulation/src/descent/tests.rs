//! The stratum registry is complete: 41 strata in eight Descents per the content map, every
//! field filled, every reference resolving.

use std::collections::BTreeSet;

use super::*;
use crate::worldgen::descent::landmarks::{LandmarkGenerator, landmark_generator};

/// The content map (expansion plan §14), Descent by Descent.
const CONTENT_MAP: [(Descent, &[&str]); 8] = [
    (
        Descent::Crust,
        &[
            "Topsoil",
            "Clay Beds",
            "Grey Stone",
            "Slate Shelves",
            "Basalt Columns",
        ],
    ),
    (
        Descent::OldDeep,
        &[
            "Crystal Faults",
            "Fossil Oceans",
            "Pressure Cathedrals",
            "Radioactive Hollow",
            "Ancient Machine Layer",
            "Void Geode Fields",
            "Mantle Storm Zone",
        ],
    ),
    (
        Descent::LivingDark,
        &[
            "Fungal Forest",
            "Root Labyrinth",
            "The Hive",
            "Flesh Caverns",
            "Glow Sea",
        ],
    ),
    (
        Descent::DrownedAndFrozen,
        &[
            "The Underground Ocean",
            "Brine Glaciers",
            "Geyser Fields",
            "Merrow City",
            "Pearl Abyss",
        ],
    ),
    (
        Descent::HollowWorld,
        &[
            "Inner Sky",
            "Megafauna Jungle",
            "Upside-Down Isles",
            "The Sunforge",
        ],
    ),
    (
        Descent::StrangeLayers,
        &[
            "Mirror Strata",
            "Gravity Shear",
            "Pixel Rot",
            "Time Sludge",
            "Echo Caves",
            "Prism Desert",
        ],
    ),
    (
        Descent::Ancients,
        &[
            "Buried Megacity",
            "Engine Halls",
            "The Archive of Stars",
            "Sleeping Garrison",
            "The Seal Gate",
        ],
    ),
    (
        Descent::Heart,
        &[
            "Dark Matter Veins",
            "Singularity Fields",
            "Core Ocean",
            "The Heart",
        ],
    ),
];

#[test]
fn the_registry_holds_all_41_strata_of_the_content_map() {
    assert_eq!(STRATA.len(), 41);
    assert_eq!(Stratum::ALL.len(), 41);
    let mut names = BTreeSet::new();
    for (index, stratum) in Stratum::ALL.into_iter().enumerate() {
        let def = stratum.def();
        assert_eq!(def.stratum, stratum, "registry slot {index}");
        assert_eq!(stratum.index(), index);
        assert_eq!(Stratum::from_index(index), stratum);
        assert!(names.insert(def.name), "duplicate name {}", def.name);
        assert!(!def.signature.is_empty(), "{stratum:?} has no signature");
    }
    let mut expected = Vec::new();
    for (descent, strata) in CONTENT_MAP {
        let names: Vec<&str> = descent.strata().map(Stratum::name).collect();
        assert_eq!(names, strata, "{descent:?}");
        assert!((4..=7).contains(&names.len()) || descent == Descent::Crust);
        expected.extend(strata.iter().copied());
    }
    let all: Vec<&str> = Stratum::ALL.iter().map(|stratum| stratum.name()).collect();
    assert_eq!(
        all, expected,
        "strata run Descent by Descent, top to bottom"
    );
}

#[test]
fn every_descent_is_contiguous_and_its_shares_fill_it() {
    let mut previous = Descent::Crust;
    for stratum in Stratum::ALL {
        let descent = stratum.descent();
        assert!(
            descent == previous || descent.index() == previous.index() + 1,
            "{stratum:?} breaks the Descent order"
        );
        previous = descent;
    }
    for descent in Descent::ALL {
        let shares: u32 = descent
            .strata()
            .map(|stratum| u32::from(stratum.def().share_per_mille))
            .sum();
        assert_eq!(shares, 1_000, "{descent:?}");
        assert_eq!(descent.strata().next(), Some(descent.first_stratum()));
        assert_eq!(descent.strata().last(), Some(descent.last_stratum()));
        assert!(!descent.name().is_empty() && !descent.story_beat().is_empty());
    }
    // Target rows run back to back to 13,000 rows over 60-90 hours.
    let mut row = 0;
    let mut hours = 0;
    for descent in Descent::ALL {
        let (first, end) = descent.target_rows();
        assert_eq!(first, row);
        assert!(end > first);
        row = end;
        let (start, finish) = descent.target_hours();
        assert_eq!(start, hours);
        hours = finish;
    }
    assert_eq!(row, crate::worldgen::descent::DESCENT_DEPTH_ROWS);
    assert!((60..=90).contains(&hours));
}

#[test]
fn the_classic_strata_are_the_classic_bands() {
    for (index, band) in Band::ALL.into_iter().enumerate() {
        let stratum = Stratum::of_band(band);
        assert_eq!(stratum.index(), index);
        assert!(stratum.is_classic());
        let def = stratum.def();
        assert_eq!(def.host_band, band);
        assert_eq!(def.soft, band.soft_material(), "{stratum:?}");
        assert_eq!(def.hard, band.hard_material(), "{stratum:?}");
        // The authored Crust and Old Deep (Wave 8) have creatures of their own.
        if stratum.descent() == Descent::OldDeep {
            assert!(
                def.creatures
                    .iter()
                    .any(|(kind, _)| *kind as usize >= crate::enemy::EnemyKind::PrismMoth as usize),
                "{stratum:?} has no creature of its own"
            );
        } else if stratum.descent() != Descent::Crust {
            assert_eq!(
                def.creatures,
                crate::enemy::band_roster(band),
                "{stratum:?}"
            );
        }
        assert_eq!(stratum.enemy_tier(), u8::try_from(index).unwrap_or(0));
    }
    for stratum in Stratum::ALL.into_iter().skip(Band::ALL.len()) {
        assert!(!stratum.is_classic());
        assert!(matches!(stratum.host_band(), Band::Deep(_)), "{stratum:?}");
        assert_eq!(stratum.enemy_tier(), 11);
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one check per field, each authored Descent's music listed"
)]
fn every_field_of_every_stratum_is_filled_and_resolves() {
    for stratum in Stratum::ALL {
        let def = stratum.def();
        assert!(!def.ores.is_empty(), "{stratum:?} has no ores");
        for ore in def.ores {
            assert!(
                crate::worldgen::deposits::MINERALS.contains(&ore.mineral),
                "{stratum:?}: {:?} is not an underground mineral",
                ore.mineral
            );
            assert!(ore.peak_per_mille <= 1_000 && ore.spread_per_mille > 0);
            assert!(ore.weight > 0);
            // The ore is diggable with the host band's drill, so it shows up.
            assert!(
                crate::terrain::tile_hardness(TileKind::Ore(ore.mineral))
                    <= def.host_band.required_drill_strength(),
                "{stratum:?}: {:?} is too hard",
                ore.mineral
            );
        }
        assert!(def.ore_weights(500).iter().any(|weight| *weight > 0));
        assert!(!def.layer_resource.name.is_empty(), "{stratum:?}");
        assert!(def.layer_resource.stands_in.is_some(), "{stratum:?}");
        if stratum != Stratum::Topsoil {
            assert!(!def.creatures.is_empty(), "{stratum:?} has no creatures");
            assert!(!def.hazards.is_empty(), "{stratum:?} has no hazards");
        }
        for (kind, weight) in def.creatures {
            assert!(*weight > 0);
            if stratum.descent() == Descent::LivingDark {
                // Authored (Wave 8): the Living Dark's own creatures, never its bosses.
                assert!(kind.is_living_dark(), "{stratum:?}: {kind:?}");
                assert!(!kind.stats().elite, "{stratum:?}: {kind:?} roams");
            } else {
                // Underground kinds only (no sky, Moon, night, tomb or crypt families): the classic
                // ones and the authored strata's own (Wave 8 on).
                assert!(
                    !kind.is_sky()
                        && !kind.is_lunar()
                        && !crate::night::is_nocturnal(*kind)
                        && !kind.is_tomb()
                        && !kind.is_crypt(),
                    "{stratum:?}: {kind:?}"
                );
            }
        }
        assert!(def.variants.len() >= 2, "{stratum:?} needs seed variants");
        assert!(!def.landmarks.is_empty(), "{stratum:?}");
        for kind in def.landmarks {
            // Every landmark kind has a generator behind it.
            let generator = landmark_generator(*kind);
            assert!(matches!(
                generator,
                LandmarkGenerator::DescentStamp
                    | LandmarkGenerator::HollowCavern
                    | LandmarkGenerator::PointsOfInterest
                    | LandmarkGenerator::CaveEcosystems
                    | LandmarkGenerator::Villages
                    | LandmarkGenerator::TombSites
                    | LandmarkGenerator::LivingDark
                    | LandmarkGenerator::Drowned
                    | LandmarkGenerator::HollowWorld
            ));
        }
        let field = def.field;
        assert!(field.time_scale_permille > 0 && field.friction_permille > 0);
        for level in field.screen {
            assert!(level.intensity_permille <= 1_000, "{stratum:?}");
        }
        // Music follows the Descent; the authored Crust (Wave 8) has a piece per stratum.
        if def.descent == Descent::Crust {
            assert!(
                matches!(
                    def.music,
                    MusicCue::Topsoil
                        | MusicCue::ClayBeds
                        | MusicCue::GreyStone
                        | MusicCue::SlateShelves
                        | MusicCue::BasaltColumns
                ),
                "{stratum:?}"
            );
        } else if def.descent == Descent::DrownedAndFrozen {
            // So do the Drowned and Frozen's (Wave 9).
            assert!(
                matches!(
                    def.music,
                    MusicCue::UndergroundOcean
                        | MusicCue::BrineGlaciers
                        | MusicCue::GeyserFields
                        | MusicCue::MerrowCity
                        | MusicCue::PearlAbyss
                ),
                "{stratum:?}"
            );
        } else if def.descent == Descent::LivingDark {
            // The Living Dark's strata have pieces of their own too (Wave 8).
            assert!(
                matches!(
                    def.music,
                    MusicCue::FungalForest
                        | MusicCue::RootLabyrinth
                        | MusicCue::Hive
                        | MusicCue::FleshCaverns
                        | MusicCue::GlowSea
                ),
                "{stratum:?}"
            );
        } else if def.descent == Descent::HollowWorld {
            // The Hollow World's strata have pieces of their own (Wave 9).
            assert!(
                matches!(
                    def.music,
                    MusicCue::InnerSky
                        | MusicCue::MegafaunaJungle
                        | MusicCue::UpsideDownIsles
                        | MusicCue::Sunforge
                ),
                "{stratum:?}"
            );
        } else {
            assert_eq!(def.music as usize, def.descent as usize, "{stratum:?}");
        }
    }
    assert!(FieldProfile::NEUTRAL.is_neutral());
    assert!(FieldProfile::default().is_neutral());
    // Descents I and II keep neutral fields but for the Radioactive Hollow's green grade.
    assert!(Stratum::Topsoil.def().field.is_neutral());
    assert!(!Stratum::PixelRot.def().field.is_neutral());
}

#[test]
fn every_descent_has_one_outpost_and_ends_at_a_gate() {
    let mut bosses = BTreeSet::new();
    let mut rewards = BTreeSet::new();
    for descent in Descent::ALL {
        let outposts = descent
            .strata()
            .filter(|stratum| stratum.def().landmarks.contains(&LandmarkKind::Outpost))
            .count();
        assert_eq!(outposts, 1, "{descent:?}");
        let last = descent.last_stratum();
        assert!(
            last.def().landmarks.contains(&LandmarkKind::GateChamber),
            "{descent:?}"
        );
        let gate = descent.gate().expect("a gate");
        assert!(bosses.insert(gate.boss), "{descent:?} reuses a boss");
        assert_eq!(gate.boss as usize, descent.index());
        assert!(
            gate.requires.contains(&GateRequirement::Boss(gate.boss)),
            "{descent:?}"
        );
        for reward in gate.rewards {
            assert!(rewards.insert(*reward), "{reward:?} rewarded twice");
        }
        for stratum in descent.strata().filter(|stratum| *stratum != last) {
            assert!(stratum.def().gate.is_none(), "{stratum:?}");
        }
        // Every key a gate asks for is handed out by an earlier gate.
        for requirement in gate.requires {
            if let GateRequirement::Key(key) = requirement {
                let earlier = Descent::ALL[..descent.index()]
                    .iter()
                    .filter_map(|earlier| earlier.gate())
                    .any(|earlier| earlier.rewards.contains(key));
                assert!(earlier, "{descent:?} needs {key:?} before it is given");
            }
        }
    }
    assert_eq!(
        rewards.len(),
        DescentKey::ALL.len(),
        "every key is a reward"
    );
    // The Hollow World holds the cavern, the Strange Layers the anomaly rooms.
    assert!(
        Stratum::InnerSky
            .def()
            .landmarks
            .contains(&LandmarkKind::HollowCavern)
    );
    assert!(
        Descent::StrangeLayers
            .strata()
            .all(|stratum| stratum.def().landmarks.contains(&LandmarkKind::AnomalyRoom))
    );
}

#[test]
fn gates_check_their_requirements() {
    let star_core = Descent::OldDeep.gate().expect("a gate");
    let mut progress = GateProgress::default();
    assert!(!gate_passable(star_core, &progress));
    progress.keys |= 1 << DescentKey::DeepClaim as u32;
    progress.bosses |= 1 << DescentBoss::StarCore as u32;
    assert!(gate_passable(star_core, &progress));
    let seal = Descent::Ancients.gate().expect("a gate");
    progress.keys |= 1 << DescentKey::PhaseDrill as u32;
    progress.bosses |= 1 << DescentBoss::GatekeeperColossus as u32;
    assert!(
        !gate_passable(seal, &progress),
        "every earlier key is needed"
    );
    progress.keys = u32::MAX;
    assert!(gate_passable(seal, &progress));
}

#[test]
fn every_descent_looks_different() {
    // No stratum's rock looks like any stratum's of another Descent (sum of channel
    // differences), so every Descent reads as its own place.
    let distance = |a: Rgb, b: Rgb| {
        u32::from(a.0.abs_diff(b.0)) + u32::from(a.1.abs_diff(b.1)) + u32::from(a.2.abs_diff(b.2))
    };
    for first in Stratum::ALL {
        for second in Stratum::ALL {
            if first.descent() == second.descent() {
                continue;
            }
            let (a, b) = (first.def().palette.rock, second.def().palette.rock);
            assert!(
                distance(a, b) >= 32,
                "{first:?} and {second:?} look alike: {a:?} {b:?}"
            );
        }
    }
    // Neighbouring strata differ too.
    for pair in Stratum::ALL.windows(2) {
        assert_ne!(pair[0].def().palette, pair[1].def().palette, "{pair:?}");
    }
}

#[test]
fn ore_curves_peak_where_they_say() {
    let curve = OreCurve {
        mineral: MineralKind::Diamond,
        peak_per_mille: 500,
        spread_per_mille: 400,
        weight: 100,
    };
    assert_eq!(curve.weight_at(500), 100);
    assert!(curve.weight_at(300) < 100 && curve.weight_at(300) > curve.weight_at(100));
    assert_eq!(curve.weight_at(0), 10);
    assert_eq!(curve.weight_at(1_000), 10);
}
