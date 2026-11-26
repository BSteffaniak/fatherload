//! Launch flags for testing (Wave 14): `--god` starts a new test-mode world straight away, and
//! `--start-at <target>` starts it already in place (`descent:5`, `planet:random:ice`,
//! `boss:colossus`, `station:nearest`, `depth:4200m`, `stratum:crystal_faults`, `moon`).
//! Targets are the palette's `tp` words joined by `:` and go through the same travel action
//! (the destination index's `TravelTo`), so screenshot scenes and bots reuse them
//! ([`start_actions`]).

use fatherload_simulation::{PlayerCommand, PlayerId, TestModeAction, WorldState};

use super::{
    dispatch::{DispatchContext, SeatTravel, Trip, trip},
    parse::{TpTarget, parse_start_at},
};

/// The seed a `--god` world uses unless `--seed` names one (the same world every launch).
pub const DEFAULT_TEST_SEED: u64 = 0x00C0_FFEE;

/// What the testing launch flags ask for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchFlags {
    /// Where the rig starts (`None`: the spawn point).
    pub start_at: Option<TpTarget>,
    /// The world seed.
    pub seed: u64,
}

/// Reads `--god`, `--start-at <target>` (which implies `--god`) and `--seed <n>`. `None` when
/// neither `--god` nor `--start-at` is given.
///
/// # Errors
///
/// A target or seed that does not parse.
pub fn parse_launch_flags(args: &[String]) -> Result<Option<LaunchFlags>, String> {
    let value_of = |flag: &str| -> Result<Option<&String>, String> {
        args.iter()
            .position(|arg| arg == flag)
            .map_or(Ok(None), |at| {
                args.get(at + 1)
                    .filter(|value| !value.starts_with("--"))
                    .map(Some)
                    .ok_or_else(|| format!("{flag} needs a value"))
            })
    };
    let god = args.iter().any(|arg| arg == "--god");
    let start_at = value_of("--start-at")?
        .map(|text| parse_start_at(text))
        .transpose()?;
    if !god && start_at.is_none() {
        return Ok(None);
    }
    let seed = match value_of("--seed")? {
        None => DEFAULT_TEST_SEED,
        Some(text) => parse_seed(text).ok_or_else(|| format!("--seed: not a number: {text}"))?,
    };
    Ok(Some(LaunchFlags { start_at, seed }))
}

fn parse_seed(text: &str) -> Option<u64> {
    text.strip_prefix("0x").map_or_else(
        || text.parse().ok(),
        |hex| u64::from_str_radix(hex, 16).ok(),
    )
}

/// The actions that put `player`'s rig at `target` in a fresh test-mode world: the
/// destination index's travel (with the ending marked done first when space is not open yet,
/// for a space target).
///
/// # Errors
///
/// When the target names nothing in this world.
pub fn start_actions(
    world: &WorldState,
    player: PlayerId,
    target: &TpTarget,
) -> Result<Vec<PlayerCommand>, String> {
    let ctx = DispatchContext {
        world,
        player,
        online: false,
    };
    let trip = trip(target, ctx, &mut SeatTravel::seeded())?;
    let Trip::To(request) = trip else {
        return Err("--start-at cannot go back (no earlier spot)".to_owned());
    };
    let mut actions = Vec::new();
    let space_closed = !world.space_enabled();
    if space_closed && request.kind.in_space() {
        // A space target in a world whose Heart has not woken: the ending opens space first.
        actions.push(PlayerCommand::TestMode(TestModeAction::SetEndingDone(true)));
    } else if world.resolve_travel(player, &request).is_none() {
        return Err(format!(
            "--start-at: nowhere to land for {target:?} in this world"
        ));
    }
    actions.push(PlayerCommand::TestMode(TestModeAction::TravelTo(request)));
    Ok(actions)
}
