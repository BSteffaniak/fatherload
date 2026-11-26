//! The hunt list (Wave 13 endgame): every boss of the galaxy in one list.
//!
//! The three faction bosses (Wave 12 hostiles: the Gilded Maw, the Swarm Queen, the Arbiter,
//! each in a lair the nav chart can point at) and the theme worlds' bosses, registered in
//! [`THEME_BOSSES`] (Wave 13 integration: the fungal, machine and dead worlds' from
//! [`crate::planet::outlands::THEME_BOSSES`], the gas giant's, locked and ring worlds' from
//! [`crate::exotic::boss::THEME_BOSSES`]); each records itself through
//! [`crate::WorldState::record_theme_boss`] when it falls.
//! The list, the completion screen and the achievements read the same registry, so a theme
//! boss counts the moment it is registered.

use serde::{Deserialize, Serialize};

use crate::{hostiles::Faction, space::Theme};

/// A theme world's boss (registered by its theme track).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeBoss {
    /// Its planet theme's id ([`crate::planet::PlanetThemeId`]'s number).
    pub theme: u16,
    /// The space theme of the worlds it rules (for the list's where-to line).
    pub world: Theme,
    pub name: &'static str,
}

/// The theme bosses, by planet theme (append only: a theme track appends its boss here).
pub const THEME_BOSSES: &[ThemeBoss] = &[
    // Wave 13 planets-c: the outland worlds.
    ThemeBoss {
        theme: crate::planet::PlanetThemeId::FUNGAL.0,
        world: Theme::Fungal,
        name: "The Sporemother",
    },
    ThemeBoss {
        theme: crate::planet::PlanetThemeId::MACHINE.0,
        world: Theme::MachineWorld,
        name: "The Reactor Warden",
    },
    ThemeBoss {
        theme: crate::planet::PlanetThemeId::DEAD.0,
        world: Theme::DeadWorld,
        name: "The Ashen Sovereign",
    },
    // Wave 13 planets-d: the exotic worlds.
    ThemeBoss {
        theme: crate::exotic::GAS_GIANT.0,
        world: Theme::GasGiant,
        name: "The Thunderhead",
    },
    ThemeBoss {
        theme: crate::exotic::LOCKED_WORLD.0,
        world: Theme::TidallyLocked,
        name: "The Gloamwarden",
    },
    ThemeBoss {
        theme: crate::exotic::RING_WORLD.0,
        world: Theme::RingWorld,
        name: "The Ring Warden",
    },
];

/// The hunt list's theme boss of an enemy kind (the theme it rules), if it is one.
#[must_use]
pub fn theme_boss_of(kind: crate::enemy::EnemyKind) -> Option<u16> {
    crate::planet::outlands::THEME_BOSSES
        .iter()
        .find(|boss| boss.kind == kind)
        .map(|boss| boss.theme.0)
        .or_else(|| {
            crate::exotic::boss::THEME_BOSSES
                .iter()
                .find(|(_, boss, _)| *boss == kind)
                .map(|(style, ..)| style.theme().0)
        })
}

/// One entry of the hunt list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Quarry {
    Faction(Faction),
    /// A theme boss, by planet theme id.
    Theme(u16),
}

impl Quarry {
    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Faction(faction) => faction_boss_name(faction),
            Self::Theme(theme) => THEME_BOSSES
                .iter()
                .find(|boss| boss.theme == theme)
                .map_or("Unknown", |boss| boss.name),
        }
    }

    /// Where to look (a line for the list).
    #[must_use]
    pub fn whereabouts(self) -> String {
        match self {
            Self::Faction(faction) => format!("Lair of {}", faction.name()),
            Self::Theme(theme) => THEME_BOSSES
                .iter()
                .find(|boss| boss.theme == theme)
                .map_or_else(String::new, |boss| {
                    format!("On a {} world", boss.world.name())
                }),
        }
    }

    /// Wire code (events): factions 0-2, theme bosses 100 + theme id.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Faction(faction) => faction.code(),
            Self::Theme(theme) => 100 + theme as u32,
        }
    }
}

/// A faction boss's name.
#[must_use]
pub const fn faction_boss_name(faction: Faction) -> &'static str {
    match faction {
        Faction::Pirates => "The Gilded Maw",
        Faction::Swarm => "The Swarm Queen",
        Faction::Machine => "The Arbiter",
    }
}

/// How far round home (sectors, rings) [`nearest_theme_world`] looks.
pub const THEME_WORLD_SEARCH_RINGS: i32 = 8;

/// The nearest full planet of space theme `world` round home, in the world of `seed`.
///
/// Ring by ring, the biggest of a ring: a theme boss's lair is on every full planet of its
/// theme (small bodies have none). Memoized: the hunt list asks every frame it shows.
#[must_use]
pub fn nearest_theme_world(seed: u64, world: Theme) -> Option<crate::space::SpaceBodyId> {
    use std::{
        collections::BTreeMap,
        sync::{Mutex, PoisonError},
    };
    type Cache = Mutex<BTreeMap<(u64, Theme), Option<crate::space::SpaceBodyId>>>;
    static CACHE: Cache = Mutex::new(BTreeMap::new());
    if let Some(found) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(seed, world))
    {
        return *found;
    }
    let space = crate::space::Space::shared(seed);
    let home = crate::space::HOME_SECTOR;
    let mut found = None;
    'rings: for ring in 0..=THEME_WORLD_SEARCH_RINGS {
        let mut best: Option<(u32, crate::space::SpaceBodyId)> = None;
        for y in -ring..=ring {
            for x in -ring..=ring {
                if x.abs().max(y.abs()) != ring {
                    continue;
                }
                let sector = crate::space::SpaceSector::new(home.x + x, home.y + y);
                for system in &space.sector(sector).systems {
                    for body in &system.bodies {
                        let id = crate::space::SpaceBodyId {
                            sector,
                            system: system.index,
                            body: body.index,
                        };
                        if body.theme != world
                            || best.is_some_and(|(radius, _)| radius >= body.radius)
                        {
                            continue;
                        }
                        let Some(params) = crate::place::planet_params_for(seed, id) else {
                            continue;
                        };
                        if !crate::planet::wilds::is_small_body(&params) {
                            best = Some((body.radius, id));
                        }
                    }
                }
            }
        }
        if let Some((_, id)) = best {
            found = Some(id);
            break 'rings;
        }
    }
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert((seed, world), found);
    found
}

/// The whole hunt list, in order: the faction bosses, then the theme bosses.
#[must_use]
pub fn hunt_list() -> Vec<Quarry> {
    Faction::ALL
        .into_iter()
        .map(Quarry::Faction)
        .chain(THEME_BOSSES.iter().map(|boss| Quarry::Theme(boss.theme)))
        .collect()
}
