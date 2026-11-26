//! Endless exploration (Wave 13 story): procedural discoveries and mini-arcs, generated per
//! star system, so every new system keeps giving reasons to visit after the story is done.
//!
//! [`leads_of`] is a pure function of the world seed and the system: every peer, save and
//! frontend agrees what a system holds without anything being sent. A system's leads are
//! **picked up** the first time a rig of the crew enters it (they go into the discovery log,
//! `super::GalaxyStory::leads`), then advance step by step as the crew does what each step
//! asks. Every lead pays when its last step is done and turns up one of the builders'
//! fragments ([`super::journal::FRAGMENTS`]).
//!
//! | Kind | Steps |
//! |---|---|
//! | Lost ship | fly to the transponder (a derelict's, if the system has one) |
//! | Distress call | fly to the beacon; survive the ambush there; bring the survivors to any station |
//! | Ruins | land on the world; dig to the ruin's mark |
//! | Survey contract | scan the system's worlds (up to three) |
//! | Species request | land on the settled world; meet its people |
//!
//! The home system has none. Danger rings out from home pay more.

use crate::{
    alien::AlienSpecies,
    flight::space::hash64,
    hostiles::Faction,
    space::{HOME_SECTOR, Space, SpaceBodyId, SpaceTile, StarSystem, SystemRef},
};

use super::plan::RelicSite;

/// A rig within this of a lead's point in flight has reached it (tiles).
pub const LEAD_POINT_REACH_TILES: i64 = 8_000;
/// Most leads one system holds.
pub const MAX_LEADS_PER_SYSTEM: u8 = 2;

const SALT_LEADS: u64 = 0x1EAD_0001;

/// What a lead is.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum LeadKind {
    LostShip,
    DistressCall,
    Ruins,
    Survey,
    SpeciesRequest,
}

impl LeadKind {
    pub const ALL: [Self; 5] = [
        Self::LostShip,
        Self::DistressCall,
        Self::Ruins,
        Self::Survey,
        Self::SpeciesRequest,
    ];

    /// Its index (stats and the wire).
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::LostShip => 0,
            Self::DistressCall => 1,
            Self::Ruins => 2,
            Self::Survey => 3,
            Self::SpeciesRequest => 4,
        }
    }

    /// What the discovery log calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LostShip => "Lost ship",
            Self::DistressCall => "Distress call",
            Self::Ruins => "Ruins",
            Self::Survey => "Survey contract",
            Self::SpeciesRequest => "A people's request",
        }
    }
}

/// One step of a lead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeadStep {
    /// Fly within [`LEAD_POINT_REACH_TILES`] of the point.
    ReachPoint(SpaceTile),
    /// Survive the ambush the point springs (`faction`'s ships, on the rig that reached it).
    Survive(Faction),
    /// Dock at any station.
    Dock,
    /// Set down on the world.
    Land(SpaceBodyId),
    /// Stand at the site on the world (on its surface, or dug down to it).
    FindSite(SpaceBodyId, RelicSite),
    /// Have this many of the system's worlds scanned.
    Scan(u8),
    /// Meet the people of the world (first contact with `species`, anyone in the crew).
    MeetFolk(SpaceBodyId, AlienSpecies),
}

impl LeadStep {
    /// The world the step is done on, if any.
    #[must_use]
    pub const fn body(self) -> Option<SpaceBodyId> {
        match self {
            Self::Land(body) | Self::FindSite(body, _) | Self::MeetFolk(body, _) => Some(body),
            _ => None,
        }
    }
}

/// One lead of a system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lead {
    pub system: SystemRef,
    /// Its index in the system (`0..MAX_LEADS_PER_SYSTEM`).
    pub index: u8,
    pub kind: LeadKind,
    pub title: String,
    /// What the log says about it.
    pub text: String,
    pub steps: Vec<LeadStep>,
    /// Credits paid when the last step is done.
    pub credits: u32,
    /// The builders' fragment it turns up.
    pub fragment: u8,
}

impl Lead {
    /// What step `step` asks, in words.
    #[must_use]
    pub fn step_text(&self, step: u8, system: &StarSystem) -> String {
        let Some(found) = self.steps.get(usize::from(step)) else {
            return "Done.".to_owned();
        };
        match *found {
            LeadStep::ReachPoint(_) => match self.kind {
                LeadKind::DistressCall => format!("Fly to the distress beacon in {}.", system.name),
                _ => format!("Fly to the transponder in {}.", system.name),
            },
            LeadStep::Survive(faction) => {
                format!(
                    "Drive off the {} lying in wait at the beacon.",
                    faction.name()
                )
            }
            LeadStep::Dock => "Bring the survivors to any station and dock.".to_owned(),
            LeadStep::Land(body) => format!("Land on {}.", system.body_name(body.body)),
            LeadStep::FindSite(body, site) => {
                if site.rows == 0 {
                    format!("Find the ruin's mark on {}.", system.body_name(body.body))
                } else {
                    format!(
                        "Dig {} rows under the ruin's mark on {}.",
                        site.rows,
                        system.body_name(body.body)
                    )
                }
            }
            LeadStep::Scan(count) => format!("Scan {count} of {}'s worlds.", system.name),
            LeadStep::MeetFolk(body, species) => format!(
                "Meet the {} on {}.",
                species.def().name,
                system.body_name(body.body)
            ),
        }
    }
}

/// The danger ring of a system (Chebyshev sectors from home).
#[must_use]
pub fn ring_of(system: SystemRef) -> u32 {
    let dx = (i64::from(system.sector.x) - i64::from(HOME_SECTOR.x)).unsigned_abs();
    let dy = (i64::from(system.sector.y) - i64::from(HOME_SECTOR.y)).unsigned_abs();
    u32::try_from(dx.max(dy)).unwrap_or(u32::MAX)
}

/// The landable worlds of a system (planets with ground), in body order.
fn worlds(seed: u64, reference: SystemRef, system: &StarSystem) -> Vec<SpaceBodyId> {
    system
        .bodies
        .iter()
        .filter(|body| !body.is_home())
        .map(|body| SpaceBodyId {
            sector: reference.sector,
            system: reference.system,
            body: body.index,
        })
        .filter(|id| crate::place::planet_params_for(seed, *id).is_some())
        .collect()
}

const SHIP_NAMES: [&str; 12] = [
    "Patient Gull",
    "Second Chance",
    "Lantern Moth",
    "Old Reliable",
    "Kettle Black",
    "Wren's Errand",
    "Long Way Round",
    "Copper Kite",
    "Quiet Harvest",
    "Burrow Home",
    "Tin Psalm",
    "Brightwater",
];

/// A system's leads (pure; none for home), memoized per seed and system (the discovery log
/// reads them every check and the renderer every frame).
#[must_use]
pub fn leads_cached(seed: u64, reference: SystemRef) -> std::sync::Arc<Vec<Lead>> {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex, PoisonError},
    };
    type LeadCache = Mutex<BTreeMap<(u64, SystemRef), Arc<Vec<Lead>>>>;
    static CACHE: LeadCache = Mutex::new(BTreeMap::new());
    if let Some(found) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(seed, reference))
    {
        return Arc::clone(found);
    }
    let leads = Arc::new(leads_of(seed, reference));
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 4_096 {
        cache.clear();
    }
    cache.insert((seed, reference), Arc::clone(&leads));
    leads
}

/// A system's leads (pure; none for home).
#[must_use]
pub fn leads_of(seed: u64, reference: SystemRef) -> Vec<Lead> {
    let space = Space::shared(seed);
    let Some(system) = space.system(reference) else {
        return Vec::new();
    };
    if system.home {
        return Vec::new();
    }
    let roll = |salt: u64| {
        hash64(
            seed,
            SALT_LEADS ^ salt,
            i64::from(reference.sector.x) * 8 + i64::from(reference.system),
            i64::from(reference.sector.y),
        )
    };
    let worlds = worlds(seed, reference, &system);
    let settled: Vec<(SpaceBodyId, AlienSpecies)> = worlds
        .iter()
        .filter_map(|id| {
            let params = crate::place::planet_params_for(seed, *id)?;
            AlienSpecies::from_folk(params.folk).map(|species| (*id, species))
        })
        .collect();
    let mut kinds: Vec<LeadKind> = vec![LeadKind::LostShip, LeadKind::DistressCall];
    if !worlds.is_empty() {
        kinds.push(LeadKind::Ruins);
        kinds.push(LeadKind::Survey);
    }
    if !settled.is_empty() {
        kinds.push(LeadKind::SpeciesRequest);
    }
    let count = 1 + u8::try_from(roll(1) % u64::from(MAX_LEADS_PER_SYSTEM)).unwrap_or(0);
    let ring = ring_of(reference);
    let mut out: Vec<Lead> = Vec::new();
    for index in 0..count {
        let pick = roll(10 + u64::from(index));
        // A people's request comes first where people live (and only once a system).
        let kind = if index == 0 && !settled.is_empty() && pick % 3 == 0 {
            LeadKind::SpeciesRequest
        } else {
            kinds[usize::try_from(pick % kinds.len() as u64).unwrap_or(0)]
        };
        if out.iter().any(|lead| lead.kind == kind) {
            continue;
        }
        let detail = roll(20 + u64::from(index));
        let lead = build(
            seed, reference, &system, &worlds, &settled, index, kind, detail, ring,
        );
        out.push(lead);
    }
    out
}

/// A point inside the system's reach (between its orbits), from `detail`.
fn point_in(system: &StarSystem, detail: u64) -> SpaceTile {
    // A derelict's, if the system has one.
    if let Some(derelict) = system.derelicts.first()
        && detail.is_multiple_of(2)
    {
        return derelict.centre;
    }
    let reach = i64::from(system.extent.max(80_000)) * 3 / 5;
    let angle = u32::try_from(detail % u64::from(crate::space::TURN)).unwrap_or(0);
    let (cos, sin) = crate::space::cos_sin(angle);
    let one = crate::space::TRIG_ONE;
    system.centre.offset(cos * reach / one, sin * reach / one)
}

#[allow(clippy::too_many_arguments, reason = "a lead's whole make-up")]
fn build(
    seed: u64,
    reference: SystemRef,
    system: &StarSystem,
    worlds: &[SpaceBodyId],
    settled: &[(SpaceBodyId, AlienSpecies)],
    index: u8,
    kind: LeadKind,
    detail: u64,
    ring: u32,
) -> Lead {
    let pay = |base: u32| base.saturating_mul(2 + ring.min(30)) / 2;
    let fragment = u8::try_from(detail % super::journal::FRAGMENTS.len() as u64).unwrap_or(0);
    let ship = SHIP_NAMES[usize::try_from((detail >> 8) % 12).unwrap_or(0)];
    let world = |offset: u64| {
        worlds[usize::try_from((detail >> 16).wrapping_add(offset) % worlds.len().max(1) as u64)
            .unwrap_or(0)]
    };
    let (title, text, steps, credits) = match kind {
        LeadKind::LostShip => (
            format!("The {ship}"),
            format!(
                "A transponder ticks in {}: the {ship}, a prospector's hauler, overdue for a \
                 generation. Somebody should find out what happened.",
                system.name
            ),
            vec![LeadStep::ReachPoint(point_in(system, detail))],
            pay(900),
        ),
        LeadKind::DistressCall => {
            let faction = match (detail >> 12) % 3 {
                0 => Faction::Swarm,
                _ => Faction::Pirates,
            };
            (
                format!("Mayday from the {ship}"),
                format!(
                    "A distress beacon in {}: the {ship}, crippled, crew alive and hiding. \
                     Something is circling them.",
                    system.name
                ),
                vec![
                    LeadStep::ReachPoint(point_in(system, detail | 1)),
                    LeadStep::Survive(faction),
                    LeadStep::Dock,
                ],
                pay(1_600),
            )
        }
        LeadKind::Ruins => {
            let body = world(0);
            let columns = Space::shared(seed)
                .body(body)
                .map_or(1, |found| found.surface_columns);
            let site = RelicSite {
                column: u32::try_from((detail >> 24) % u64::from(columns.max(1))).unwrap_or(0),
                rows: u16::try_from((detail >> 40) % 3).unwrap_or(0) * 10,
            };
            (
                format!("Ruins on {}", system.body_name(body.body)),
                format!(
                    "Old walls show on {} from orbit, laid in courses the way the builders \
                     laid the Crust. A mark among them still glows.",
                    system.body_name(body.body)
                ),
                vec![LeadStep::Land(body), LeadStep::FindSite(body, site)],
                pay(1_200),
            )
        }
        LeadKind::Survey => {
            let count = u8::try_from(worlds.len().min(3)).unwrap_or(1);
            (
                format!("Survey of {}", system.name),
                format!(
                    "The stations' cartographers pay for fresh scans of {}: {count} of its \
                     worlds, from orbit.",
                    system.name
                ),
                vec![LeadStep::Scan(count)],
                pay(700) * u32::from(count),
            )
        }
        LeadKind::SpeciesRequest => {
            let (body, species) =
                settled[usize::try_from(detail % settled.len().max(1) as u64).unwrap_or(0)];
            (
                format!("A song-stone for {}", system.body_name(body.body)),
                format!(
                    "A courier of the {} asks you to carry a song-stone to their town on {}. \
                     They will not say what it sings.",
                    species.def().name,
                    system.body_name(body.body)
                ),
                vec![LeadStep::Land(body), LeadStep::MeetFolk(body, species)],
                pay(1_000),
            )
        }
    };
    Lead {
        system: reference,
        index,
        kind,
        title,
        text,
        steps,
        credits,
        fragment,
    }
}
