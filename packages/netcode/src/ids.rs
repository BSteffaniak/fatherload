//! Identifier newtypes.
//!
//! Every identifier is a `u32`/`u64` newtype. There is no fixed player cap and
//! no `u8` slot index anywhere on the wire (plan §15).

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl $name {
            /// Returns the raw value.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }

            /// Returns the next identifier in sequence.
            #[must_use]
            pub const fn next(self) -> Self {
                Self(self.0.wrapping_add(1))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_newtype!(
    /// A player in the simulation. Allocated by the host, stable for the session
    /// (and across reconnects through a [`ReconnectToken`]).
    PlayerId(u32)
);
id_newtype!(
    /// A transport-level connection, assigned by whatever owns the sockets.
    ClientId(u64)
);
id_newtype!(
    /// A local input seat on the host (single-player / split-screen).
    SeatId(u32)
);
id_newtype!(
    /// A simulation tick number. A world "at tick T" has simulated T ticks.
    Tick(u64)
);
id_newtype!(
    /// A one-shot action identity, unique per client session, assigned
    /// sequentially from 1. Used for exactly-once application on the host.
    ActionId(u64)
);
id_newtype!(
    /// A client input frame sequence number, strictly increasing per client.
    InputSeq(u64)
);
id_newtype!(
    /// A session epoch. Bumped whenever the host replaces the world (e.g. on
    /// load), so stale packets from the previous world are rejected.
    SessionEpoch(u64)
);
id_newtype!(
    /// Opaque token that lets a reconnecting client reclaim its [`PlayerId`].
    ReconnectToken(u64)
);

impl Tick {
    /// Returns this tick advanced by `n` ticks.
    #[must_use]
    pub const fn plus(self, n: u64) -> Self {
        Self(self.0.saturating_add(n))
    }

    /// Signed difference `self - other` in ticks.
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub const fn diff(self, other: Self) -> i64 {
        self.0.wrapping_sub(other.0) as i64
    }
}
