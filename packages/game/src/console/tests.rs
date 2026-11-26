//! The palette: parsing, completion, dispatch (every command sends the right action) and the
//! launch flags.

use fatherload_simulation::{
    EnemyDifficulty, EnemyKind, PlayerCommand, PlayerId, SimulationParams, TestModeAction,
    WorldState,
    bodylife::weather::WeatherId,
    descent::{Descent, DescentBoss, Stratum},
    destination::{DestinationFilter, DestinationKind, Selector, TravelRequest},
    explosive::ExplosiveKind,
    hostiles::Faction,
    space::Theme,
    test_mode::StockItem,
    workshop::DeepSupply,
    worldgen::{GeneratorVersion, WorldSettings},
};

use super::{
    Console, ConsoleContext,
    complete::{common_prefix, complete},
    dispatch::Effect,
    names::{self, BossName},
    parse::{Command, Give, Step, TpTarget, parse, parse_depth, parse_start_at, parse_time},
    start_at::{DEFAULT_TEST_SEED, parse_launch_flags, start_actions},
};
use crate::{
    debug_views::{DebugView, TimeScale},
    input::PlayerInput,
};

const PLAYER: PlayerId = PlayerId::new(1);
const MATE: PlayerId = PlayerId::new(2);

fn test_world() -> WorldState {
    let mut world = WorldState::new_game_with_settings(
        PLAYER,
        &WorldSettings {
            generator: GeneratorVersion::DEEP_DESCENT,
            ..WorldSettings::new(DEFAULT_TEST_SEED)
        },
        SimulationParams::new(30),
    );
    world.enable_test_mode();
    world.add_player(MATE);
    world
}

fn tp(target: TpTarget) -> Command {
    Command::Tp {
        target,
        preview: false,
    }
}

fn parsed(line: &str) -> Command {
    parse(line)
        .unwrap_or_else(|error| panic!("{line}: {error}"))
        .unwrap_or_else(|| panic!("{line}: empty"))
}

#[test]
#[allow(clippy::too_many_lines, reason = "one case per travel form")]
fn travel_commands_parse() {
    assert_eq!(
        parsed("tp planet next"),
        tp(TpTarget::Planet {
            step: Step::Next,
            theme: None
        })
    );
    assert_eq!(
        parsed("tp planet random ice"),
        tp(TpTarget::Planet {
            step: Step::Random,
            theme: Some(Theme::Ice)
        })
    );
    assert_eq!(
        parsed("TP Planet gas"),
        tp(TpTarget::Planet {
            step: Step::Nearest,
            theme: Some(Theme::GasGiant)
        })
    );
    assert_eq!(
        parsed("tp planet prev ocean"),
        tp(TpTarget::Planet {
            step: Step::Previous,
            theme: Some(Theme::OceanArchipelago)
        })
    );
    assert_eq!(
        parsed("tp descent 5"),
        tp(TpTarget::Descent(Descent::HollowWorld))
    );
    assert_eq!(
        parsed("tp descent v"),
        tp(TpTarget::Descent(Descent::HollowWorld))
    );
    assert_eq!(
        parsed("tp descent heart"),
        tp(TpTarget::Descent(Descent::Heart))
    );
    assert_eq!(parsed("tp depth 4200m"), tp(TpTarget::Depth(4_200)));
    assert_eq!(parsed("tp depth 300"), tp(TpTarget::Depth(300)));
    assert_eq!(parsed("tp depth 1.5km"), tp(TpTarget::Depth(1_500)));
    assert_eq!(parsed("tp 4200m"), tp(TpTarget::Depth(4_200)));
    assert_eq!(
        parsed("tp stratum crystal faults"),
        tp(TpTarget::Stratum(Stratum::CrystalFaults))
    );
    assert_eq!(
        parsed("tp boss leviathan"),
        tp(TpTarget::Boss(BossName::Gate(DescentBoss::Leviathan)))
    );
    assert_eq!(
        parsed("tp boss colossus"),
        tp(TpTarget::Boss(BossName::Gate(
            DescentBoss::GatekeeperColossus
        )))
    );
    assert_eq!(
        parsed("tp leviathan"),
        tp(TpTarget::Boss(BossName::Gate(DescentBoss::Leviathan)))
    );
    assert_eq!(
        parsed("tp boss sporemother"),
        tp(TpTarget::Boss(BossName::Theme(0)))
    );
    assert_eq!(
        parsed("tp boss pirates"),
        tp(TpTarget::Boss(BossName::Flagship(Faction::Pirates)))
    );
    assert_eq!(
        parsed("tp station nearest"),
        tp(TpTarget::Kind {
            kind: DestinationKind::Station,
            step: Step::Nearest,
            name: None
        })
    );
    assert_eq!(
        parsed("tp village next"),
        tp(TpTarget::Kind {
            kind: DestinationKind::Village,
            step: Step::Next,
            name: None
        })
    );
    assert_eq!(
        parsed("tp lair random"),
        tp(TpTarget::Kind {
            kind: DestinationKind::ThemeBossLair,
            step: Step::Random,
            name: None
        })
    );
    assert_eq!(parsed("tp player 2"), tp(TpTarget::Player(2)));
    assert_eq!(
        parsed("tp bookmark camp"),
        tp(TpTarget::Bookmark("camp".to_owned()))
    );
    assert_eq!(parsed("tp back"), tp(TpTarget::Back));
    assert_eq!(parsed("tp home"), tp(TpTarget::Home));
    assert_eq!(parsed("tp moon"), tp(TpTarget::Moon));
    assert_eq!(
        parsed("tp descent 5 ?"),
        Command::Tp {
            target: TpTarget::Descent(Descent::HollowWorld),
            preview: true
        }
    );
    assert_eq!(
        parsed("tp boss leviathan?"),
        Command::Tp {
            target: TpTarget::Boss(BossName::Gate(DescentBoss::Leviathan)),
            preview: true
        }
    );
    assert_eq!(parsed("go"), Command::Go);
}

#[test]
fn stocking_and_switch_commands_parse() {
    assert_eq!(parsed("give all"), Command::Give(Give::All));
    assert_eq!(
        parsed("give dynamite 99"),
        Command::Give(Give::One {
            what: StockItem::Explosive(ExplosiveKind::Mining),
            count: 99
        })
    );
    assert_eq!(
        parsed("give c4"),
        Command::Give(Give::One {
            what: StockItem::Explosive(ExplosiveKind::C4),
            count: 1
        })
    );
    assert_eq!(
        parsed("give ballast tank 5"),
        Command::Give(Give::One {
            what: StockItem::Supply(DeepSupply::BallastTank),
            count: 5
        })
    );
    assert_eq!(parsed("gate all"), Command::Gate(None));
    assert_eq!(
        parsed("gate star core"),
        Command::Gate(Some(DescentBoss::StarCore))
    );
    assert_eq!(parsed("ending done"), Command::EndingDone);
    let matriarch = EnemyKind::ALL
        .into_iter()
        .find(|kind| kind.name() == "Hollow Matriarch")
        .expect("the Hollow Matriarch");
    assert_eq!(parsed("spawn hollow_matriarch"), Command::Spawn(matriarch));
    assert_eq!(parsed("clear"), Command::Clear);
    assert_eq!(parsed("freeze"), Command::Freeze(None));
    assert_eq!(parsed("freeze off"), Command::Freeze(Some(false)));
    assert_eq!(parsed("time 22:00"), Command::Time(22 * 60));
    assert_eq!(parsed("weather auto"), Command::Weather(None));
    let Command::Weather(Some(storm)) = parsed("weather storm") else {
        panic!("weather storm names a kind");
    };
    assert_eq!(
        fatherload_simulation::bodylife::weather::weather_kind(storm).name,
        "Storm"
    );
    assert_eq!(parsed("restock"), Command::Restock);
    assert_eq!(parsed("supplies off"), Command::Supplies(Some(false)));
    assert_eq!(parsed("difficulty"), Command::Difficulty(None));
    assert_eq!(
        parsed("difficulty hard"),
        Command::Difficulty(Some(EnemyDifficulty::Hard))
    );
    assert_eq!(
        parsed("view hitboxes"),
        Command::View(DebugView::HitBoxes, None)
    );
    assert_eq!(
        parsed("view fps on"),
        Command::View(DebugView::FrameStats, Some(true))
    );
    assert_eq!(parsed("view off"), Command::ViewsOff);
    assert_eq!(parsed("speed 4"), Command::Speed(TimeScale::Quadruple));
    assert_eq!(parsed("speed 0.5"), Command::Speed(TimeScale::Half));
    assert_eq!(parsed("pause"), Command::Speed(TimeScale::Paused));
    assert_eq!(parsed("help"), Command::Help(None));
    assert_eq!(parsed("help tp"), Command::Help(Some("tp".to_owned())));
}

#[test]
fn bad_lines_say_what_is_wrong() {
    assert_eq!(parse("   "), Ok(None));
    for (line, wanted) in [
        ("fly", "unknown command"),
        ("tp descent 9", "no Descent"),
        ("tp planet next lava", "no planet theme"),
        ("tp nowhere", "tp where?"),
        ("tp player 0", "player N"),
        ("give", "give all"),
        ("give unobtainium 4", "no item"),
        ("spawn", "spawn <creature>"),
        ("time 25:00", "time HH:MM"),
        ("view sparkles", "no debug view"),
        ("speed 3", "no speed"),
        ("freeze maybe", "on or off"),
    ] {
        let error = parse(line).expect_err(line);
        assert!(error.contains(wanted), "{line}: {error}");
    }
    // An ambiguous name lists what it could be.
    let error = parse("tp boss s").expect_err("ambiguous");
    assert!(
        error.contains("could be") || error.contains("which"),
        "{error}"
    );
}

#[test]
fn times_and_depths_parse_their_forms() {
    assert_eq!(parse_time("06:30"), Ok(390));
    assert_eq!(parse_time("6"), Ok(360));
    assert_eq!(parse_time("2215"), Ok(22 * 60 + 15));
    assert_eq!(parse_time("24:00"), Ok(0));
    assert!(parse_time("12:60").is_err());
    assert_eq!(parse_depth("4200m"), Ok(4_200));
    assert_eq!(parse_depth("12000rows"), Ok(12_000));
    assert!(parse_depth("-4").is_err());
}

#[test]
fn completion_offers_what_the_parser_accepts() {
    assert_eq!(complete("tp plan", &[]), vec!["tp planet".to_owned()]);
    assert_eq!(complete("tp pl", &[]).len(), 2, "planet and player");
    let after_planet = complete("tp planet ", &[]);
    for word in ["next", "prev", "nearest", "random", "ice", "gas_giant"] {
        assert!(
            after_planet.contains(&format!("tp planet {word}")),
            "{word} in {after_planet:?}"
        );
    }
    assert_eq!(
        complete("tp planet n", &[]),
        vec!["tp planet nearest".to_owned(), "tp planet next".to_owned()]
    );
    assert_eq!(
        complete("spawn hollow_m", &[]),
        vec!["spawn hollow_matriarch".to_owned()]
    );
    assert!(complete("tp boss colo", &[]).contains(&"tp boss gatekeeper_colossus".to_owned()));
    assert_eq!(
        complete("tp bookmark c", &["camp".to_owned()]),
        vec!["tp bookmark camp".to_owned()]
    );
    assert!(complete("", &[]).contains(&"tp".to_owned()));
    assert_eq!(complete("give dyn", &[]), vec!["give dynamite".to_owned()]);
    assert_eq!(
        complete("view f", &[]),
        vec![
            "view fields".to_owned(),
            "view fps".to_owned(),
            "view freecam".to_owned()
        ]
    );
    // Every completion of every command word parses (or needs only its argument).
    for line in complete("tp descent ", &[]) {
        assert!(parse(&line).is_ok(), "{line}");
    }
    for line in complete("tp stratum ", &[]) {
        assert!(parse(&line).is_ok(), "{line}");
    }
    for line in complete("spawn ", &[]) {
        assert!(parse(&line).is_ok(), "{line}");
    }
    for line in complete("give ", &[]) {
        assert!(parse(&line).is_ok(), "{line}");
    }
    assert_eq!(
        common_prefix(&["tp planet next".to_owned(), "tp planet nearest".to_owned()]),
        "tp planet ne"
    );
}

#[test]
fn every_creature_stratum_and_boss_name_is_unique_enough_to_type() {
    for (kind, name) in names::creatures() {
        assert_eq!(
            names::pick(&name, "creature", &names::creatures()).map(EnemyKind::name),
            Ok(kind.name()),
            "{name}"
        );
    }
    for (stratum, name) in names::strata() {
        assert_eq!(
            names::pick(&name, "stratum", &names::strata()),
            Ok(stratum),
            "{name}"
        );
    }
}

/// The palette run on `world` for seat 0 (`PLAYER`); its effects.
fn run(console: &mut Console, world: &WorldState, line: &str) -> Vec<Effect> {
    let players = [PLAYER, MATE];
    console
        .run(
            line,
            ConsoleContext {
                world,
                players: &players,
                online: false,
            },
        )
        .into_iter()
        .map(|(seat, effect)| {
            assert_eq!(seat, 0);
            effect
        })
        .collect()
}

fn travel(request: TravelRequest) -> Effect {
    test(TestModeAction::TravelTo(request))
}

fn test(action: TestModeAction) -> Effect {
    Effect::Act(PlayerCommand::TestMode(action))
}

fn planet(selector: Selector, theme: Theme) -> TravelRequest {
    TravelRequest::new(DestinationKind::Planet, selector).filtered(DestinationFilter::Theme(theme))
}

#[test]
#[allow(clippy::too_many_lines, reason = "one case per command")]
fn every_command_dispatches_its_action() {
    let world = test_world();
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    let storm = fatherload_simulation::bodylife::weather::weather_kinds()
        .find(|kind| kind.name == "Storm")
        .map(|kind| kind.id);
    let leviathan = world
        .resolve_travel(PLAYER, &TravelRequest::nearest(DestinationKind::BossArena))
        .map(|_| ())
        .and_then(|()| {
            world
                .destinations(PLAYER, DestinationKind::BossArena, DestinationFilter::Any)
                .into_iter()
                .find(|arena| arena.info.boss.as_deref() == Some(DescentBoss::Leviathan.name()))
        })
        .expect("the Leviathan's arena");
    let gates: Vec<Effect> = (0..8)
        .map(|descent| {
            test(TestModeAction::SetGateCleared {
                descent,
                cleared: true,
            })
        })
        .collect();
    let cases: Vec<(&str, Vec<Effect>)> = vec![
        (
            "tp descent 5",
            vec![travel(TravelRequest::descent(Descent::HollowWorld))],
        ),
        (
            "tp depth 4200m",
            vec![travel(TravelRequest::depth_metres(4_200))],
        ),
        (
            "tp stratum crystal_faults",
            vec![travel(
                TravelRequest::nearest(DestinationKind::Stratum)
                    .filtered(DestinationFilter::Stratum(Stratum::CrystalFaults)),
            )],
        ),
        (
            "tp boss leviathan",
            vec![travel(TravelRequest::exact(leviathan.id))],
        ),
        (
            "tp planet ice",
            vec![travel(planet(Selector::Nearest, Theme::Ice))],
        ),
        (
            "tp planet next ice",
            vec![travel(planet(Selector::Next, Theme::Ice))],
        ),
        (
            "tp planet prev ice",
            vec![travel(planet(Selector::Previous, Theme::Ice))],
        ),
        (
            "tp village",
            vec![travel(TravelRequest::nearest(DestinationKind::Village))],
        ),
        (
            "tp player 2",
            vec![travel(
                TravelRequest::nearest(DestinationKind::Player)
                    .filtered(DestinationFilter::Player(MATE)),
            )],
        ),
        (
            "tp home",
            vec![travel(TravelRequest::nearest(DestinationKind::HomeTown))],
        ),
        (
            "tp moon",
            vec![travel(TravelRequest::nearest(DestinationKind::Moon))],
        ),
        ("give all", vec![test(TestModeAction::StockAll)]),
        (
            "give dynamite 99",
            vec![test(TestModeAction::Give(
                StockItem::Explosive(ExplosiveKind::Mining),
                99,
            ))],
        ),
        (
            "give flare buoy 3",
            vec![test(TestModeAction::Give(
                StockItem::Supply(DeepSupply::FlareBuoy),
                3,
            ))],
        ),
        ("restock", vec![test(TestModeAction::Restock)]),
        (
            "supplies off",
            vec![test(TestModeAction::SetInfiniteSupplies(false))],
        ),
        ("gate all", gates),
        (
            "gate leviathan",
            vec![test(TestModeAction::SetGateCleared {
                descent: 3,
                cleared: true,
            })],
        ),
        (
            "ending done",
            vec![test(TestModeAction::SetEndingDone(true))],
        ),
        (
            "spawn hollow_matriarch",
            vec![test(TestModeAction::SpawnCreature(
                names::pick("hollow_matriarch", "creature", &names::creatures()).expect("creature"),
            ))],
        ),
        ("clear", vec![test(TestModeAction::ClearNearbyEnemies)]),
        ("freeze", vec![test(TestModeAction::SetAiFrozen(true))]),
        ("freeze off", vec![test(TestModeAction::SetAiFrozen(false))]),
        (
            "time 22:00",
            vec![test(TestModeAction::SetTimeOfDay { minutes: 22 * 60 })],
        ),
        (
            "weather storm",
            vec![test(TestModeAction::SetWeather(storm))],
        ),
        ("weather auto", vec![test(TestModeAction::SetWeather(None))]),
        (
            "difficulty peaceful",
            vec![Effect::Act(PlayerCommand::SetEnemyDifficulty(
                EnemyDifficulty::Peaceful,
            ))],
        ),
        ("difficulty", vec![]),
        ("view grid", vec![Effect::View(DebugView::ChunkGrid, None)]),
        (
            "view reveal on",
            vec![Effect::View(DebugView::MapReveal, Some(true))],
        ),
        ("view off", vec![Effect::ViewsOff]),
        ("speed 2", vec![Effect::Speed(TimeScale::Double)]),
        ("pause", vec![Effect::Speed(TimeScale::Paused)]),
        ("help", vec![]),
        // A preview sends nothing; `go` then travels there.
        ("tp descent 3 ?", vec![]),
        (
            "go",
            vec![travel(TravelRequest::descent(Descent::LivingDark))],
        ),
        // The bookmark is the world's; the palette keeps its name.
        ("bookmark camp", vec![test(TestModeAction::BookmarkHere)]),
    ];
    for (line, wanted) in cases {
        assert_eq!(run(&mut console, &world, line), wanted, "{line}");
    }
    let here = world.travel_here(PLAYER).expect("rig");
    assert_eq!(
        run(&mut console, &world, "tp bookmark camp"),
        vec![travel(TravelRequest::exact(
            here.as_kind(DestinationKind::Bookmark)
        ))]
    );
    // Nothing travelled yet in this world: there is no back-stack to go down.
    assert!(run(&mut console, &world, "tp back").is_empty());
    // A preview prints a card with the destination's name.
    let _ = run(&mut console, &world, "tp descent 5 ?");
    assert!(
        console
            .lines
            .iter()
            .any(|line| line.contains("Hollow World")),
        "{:?}",
        console.lines
    );
    // Errors print and send nothing.
    assert!(run(&mut console, &world, "tp player 1").is_empty());
    assert!(
        console
            .lines
            .last()
            .is_some_and(|line| line.contains("you"))
    );
    assert!(run(&mut console, &world, "tp bookmark nowhere").is_empty());
}

#[test]
fn online_sessions_keep_real_time() {
    let world = test_world();
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    let players = [PLAYER];
    let effects = console.run(
        "speed 4",
        ConsoleContext {
            world: &world,
            players: &players,
            online: true,
        },
    );
    assert!(effects.is_empty());
}

#[test]
fn travel_from_another_place_is_one_trip_and_back_goes_down_the_worlds_back_stack() {
    let mut world = test_world();
    let tick = world.simulation_tick();
    let mut inputs = fatherload_simulation::TickInputs::default();
    inputs.act_as(
        PLAYER,
        PlayerCommand::TestMode(TestModeAction::TravelTo(planet(
            Selector::Nearest,
            Theme::Jungle,
        ))),
    );
    let _ = world.simulate_tick(tick, &inputs);
    assert!(
        !world.place_of(PLAYER).is_home_store(),
        "on the jungle world"
    );
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    // The index crosses places itself: a Descent from a planet is one action.
    let effects = run(&mut console, &world, "tp descent 2");
    assert_eq!(
        effects,
        vec![travel(TravelRequest::descent(Descent::OldDeep))]
    );
    // `tp back` sends the index's own back (the world keeps the back-stack).
    assert_eq!(
        run(&mut console, &world, "tp back"),
        vec![test(TestModeAction::TravelBack)]
    );
}

#[test]
fn typing_tab_history_and_closing() {
    let world = test_world();
    let players = [PLAYER, MATE];
    let ctx = ConsoleContext {
        world: &world,
        players: &players,
        online: false,
    };
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    let key = |character| PlayerInput {
        text_input: Some(character),
        ..PlayerInput::default()
    };
    for character in "tp de".chars() {
        let _ = console.handle(key(character), ctx);
    }
    // `e` typed is a letter, not Enter.
    assert!(console.open);
    let _ = console.handle(
        PlayerInput {
            inventory: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(
        console.text, "tp depth",
        "Tab cycles the candidates: depth first"
    );
    let _ = console.handle(
        PlayerInput {
            inventory: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(console.text, "tp descent");
    let _ = console.handle(key(' '), ctx);
    let _ = console.handle(key('5'), ctx);
    let effects = console.handle(
        PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(effects.len(), 1);
    assert!(console.text.is_empty());
    // Up brings the line back.
    let _ = console.handle(
        PlayerInput {
            menu_up: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(console.text, "tp descent 5");
    // Backspace deletes; Esc (cancel without backspace) closes.
    let _ = console.handle(
        PlayerInput {
            text_backspace: true,
            cancel: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert!(console.open);
    assert_eq!(console.text, "tp descent ");
    let _ = console.handle(
        PlayerInput {
            cancel: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert!(!console.open);
    // The backtick toggles it shut too, and is never typed.
    console.open_for(0);
    let _ = console.handle(key('`'), ctx);
    assert!(console.text.is_empty());
    let _ = console.handle(
        PlayerInput {
            console: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert!(!console.open);
    // Never opens outside a test-mode world.
    let mut closed = Console::default();
    closed.open_for(0);
    assert!(!closed.open);
}

#[test]
fn the_touch_menu_buttons_complete_and_recall() {
    let world = test_world();
    let players = [PLAYER, MATE];
    let ctx = ConsoleContext {
        world: &world,
        players: &players,
        online: false,
    };
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    for character in "tp mo".chars() {
        let _ = console.handle(
            PlayerInput {
                text_input: Some(character),
                ..PlayerInput::default()
            },
            ctx,
        );
    }
    // The touch menu's `>` (Right) completes: a phone keyboard has no Tab.
    let right = PlayerInput {
        menu_right: true,
        ..PlayerInput::default()
    };
    let _ = console.handle(right, ctx);
    assert_eq!(console.text, "tp moon ");
    let effects = console.handle(
        PlayerInput {
            confirm: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(effects.len(), 1);
    // Its `<` (Left) brings the last line back.
    let _ = console.handle(
        PlayerInput {
            menu_left: true,
            ..PlayerInput::default()
        },
        ctx,
    );
    assert_eq!(console.text, "tp moon");
}

#[test]
fn seat_switches_whose_rig_the_palette_acts_for() {
    let world = test_world();
    let mut console = Console {
        available: true,
        ..Console::default()
    };
    console.open_for(0);
    let players = [PLAYER, MATE];
    let ctx = ConsoleContext {
        world: &world,
        players: &players,
        online: false,
    };
    assert!(console.run("seat 2", ctx).is_empty());
    assert_eq!(console.acting_seat(), 1);
    assert_eq!(console.seat, 0, "the keyboard seat still types");
    let effects = console.run("tp player 1", ctx);
    assert_eq!(
        effects,
        vec![(
            1,
            travel(
                TravelRequest::nearest(DestinationKind::Player)
                    .filtered(DestinationFilter::Player(PLAYER))
            )
        )]
    );
    // No such seat: nothing changes.
    let _ = console.run("seat 3", ctx);
    assert_eq!(console.acting_seat(), 1);
    let _ = console.run("seat 1", ctx);
    assert_eq!(console.acting_seat(), 0);
    assert_eq!(console.acting, None);
}

#[test]
fn start_at_targets_parse() {
    assert_eq!(
        parse_start_at("descent:5"),
        Ok(TpTarget::Descent(Descent::HollowWorld))
    );
    assert_eq!(
        parse_start_at("planet:random:ice"),
        Ok(TpTarget::Planet {
            step: Step::Random,
            theme: Some(Theme::Ice)
        })
    );
    assert_eq!(
        parse_start_at("boss:colossus"),
        Ok(TpTarget::Boss(BossName::Gate(
            DescentBoss::GatekeeperColossus
        )))
    );
    assert_eq!(
        parse_start_at("station:nearest"),
        Ok(TpTarget::Kind {
            kind: DestinationKind::Station,
            step: Step::Nearest,
            name: None
        })
    );
    assert_eq!(parse_start_at("depth:4200m"), Ok(TpTarget::Depth(4_200)));
    assert_eq!(
        parse_start_at("stratum:crystal_faults"),
        Ok(TpTarget::Stratum(Stratum::CrystalFaults))
    );
    assert!(parse_start_at("back").is_err());
    assert!(parse_start_at("descent:12").is_err());

    let args = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(parse_launch_flags(&args("--fullscreen")), Ok(None));
    let god = parse_launch_flags(&args("--god"))
        .expect("parses")
        .expect("god");
    assert_eq!(god.start_at, None);
    assert_eq!(god.seed, DEFAULT_TEST_SEED);
    let placed = parse_launch_flags(&args("--god --start-at descent:5 --seed 0x2a"))
        .expect("parses")
        .expect("god");
    assert_eq!(
        placed.start_at,
        Some(TpTarget::Descent(Descent::HollowWorld))
    );
    assert_eq!(placed.seed, 42);
    assert!(parse_launch_flags(&args("--start-at")).is_err());
    assert!(parse_launch_flags(&args("--start-at descent:99")).is_err());

    // The targets turn into the palette's own actions.
    let world = test_world();
    let actions =
        start_actions(&world, PLAYER, &TpTarget::Descent(Descent::HollowWorld)).expect("lands");
    assert_eq!(
        actions,
        vec![PlayerCommand::TestMode(TestModeAction::TravelTo(
            TravelRequest::descent(Descent::HollowWorld)
        ))]
    );
    let actions = start_actions(
        &world,
        PLAYER,
        &TpTarget::Planet {
            step: Step::Random,
            theme: Some(Theme::Ice),
        },
    )
    .expect("lands");
    let Some(PlayerCommand::TestMode(TestModeAction::TravelTo(request))) = actions.last() else {
        panic!("a travel: {actions:?}");
    };
    assert_eq!(request.kind, DestinationKind::Planet);
    assert_eq!(request.filter, DestinationFilter::Theme(Theme::Ice));
    assert!(matches!(request.selector, Selector::Random(_)));
    // A space target opens space first when the world has not (the ending), else goes
    // straight there.
    if !world.space_enabled() {
        assert_eq!(
            actions[0],
            PlayerCommand::TestMode(TestModeAction::SetEndingDone(true))
        );
    }
    let _ = WeatherId::CLEAR;
}
