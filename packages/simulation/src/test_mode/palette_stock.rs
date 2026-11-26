//! Test-mode stocking and progress switches the command palette calls (Wave 14 console track).
//!
//! **TODO-SEAM(w14-stock):** the console track's thin stand-in for w14-stock's actions
//! (expansion plan §19 W14-A). The palette's `give`, `gate`, `ending`, `spawn`, `clear`,
//! `freeze`, `time` and `weather` commands submit [`crate::TestModeAction::PaletteStock`] with one of
//! these; the integrator maps each onto w14-stock's own action (one line each in the console's
//! dispatcher) and deletes this enum and its stub handler in `world/travel.rs`. The stub
//! applies what the world can already do (the loadout, cargo, the Star Drive's debug grant and
//! a creature spawned beside the rig) and refuses the rest.

use serde::{Deserialize, Serialize};

use crate::{descent::DescentBoss, enemy::EnemyKind, item::ItemKind, workshop::DeepSupply};

/// Weather the palette can ask for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum WeatherSwitch {
    /// The place's own schedule again.
    Auto,
    /// Clear skies.
    Clear,
    /// The place's storm at full strength.
    Storm,
}

impl WeatherSwitch {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Clear, Self::Storm];

    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Clear => "clear",
            Self::Storm => "storm",
        }
    }
}

/// One stocking or progress request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StockRequest {
    /// Everything at once (the loadout again; w14-stock: every registry's `ALL`).
    GiveAll,
    /// `count` of one item into the rig's cargo bay.
    Give { item: ItemKind, count: u32 },
    /// `count` of one deep supply into the rig's kit.
    GiveSupply { supply: DeepSupply, count: u8 },
    /// Every Descent gate cleared.
    GateAll,
    /// One gate cleared (its boss beaten, its rewards held).
    Gate(DescentBoss),
    /// The Heart ending done: space open (the Star Drive fitted).
    EndingDone,
    /// A creature (or boss) beside the rig.
    Spawn(EnemyKind),
    /// Every hostile near the rig gone.
    ClearEnemies,
    /// Creature AI frozen (or thawed).
    FreezeAi(bool),
    /// The clock set to this minute of the day (0..1440).
    SetTime { minute_of_day: u16 },
    /// The weather where the rig is.
    SetWeather(WeatherSwitch),
}
