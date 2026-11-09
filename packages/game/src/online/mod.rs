//! Online play: the net bridge between `fatherload_netcode` sessions and `quic_transport`, the
//! host and client transport adapters, LAN discovery, scripted bots and the headless modes.
//!
//! The app uses exactly the same pieces as the headless binary modes and the in-process
//! integration test (plan §11 rule): [`host::HostNet`] beside a `HostSession`, and
//! [`client::OnlineClient`] around a `ClientSession`.

pub mod bots;
pub mod bridge;
pub mod client;
pub mod headless;
pub mod host;
pub mod lan;

#[cfg(test)]
mod tests;

use fatherload_quic_transport::NetConfig;

/// Default UDP port for hosting.
pub const DEFAULT_PORT: u16 = fatherload_quic_transport::lan::DEFAULT_GAME_PORT;

/// Transport configuration: defaults plus the `FATHERLOAD_NET_SIM` link conditioner when set.
#[must_use]
pub fn net_config() -> NetConfig {
    NetConfig::from_env().unwrap_or_else(|error| {
        eprintln!(
            "Ignoring {}: {error}",
            fatherload_quic_transport::NET_SIM_ENV_VAR
        );
        NetConfig::default()
    })
}

/// A reconnect-token seed from OS randomness (reconnect tokens are the only credential for
/// reclaiming a reserved player, so they must not be guessable).
#[must_use]
pub fn random_token_seed() -> u64 {
    getrandom::u64().unwrap_or_else(|error| {
        eprintln!("OS randomness unavailable ({error}); falling back to a time-based token seed");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        #[allow(
            clippy::cast_possible_truncation,
            reason = "only the low bits of the clock matter for a fallback seed"
        )]
        let seed = (nanos as u64) ^ u64::from(std::process::id()).rotate_left(32);
        seed
    })
}

/// The display name used when hosting or joining.
#[must_use]
pub fn player_name() -> String {
    std::env::var("FATHERLOAD_PLAYER_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(fatherload_quic_transport::lan::local_machine_name)
}
