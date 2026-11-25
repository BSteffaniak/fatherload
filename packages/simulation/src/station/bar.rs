//! A station's bar: the patrons (the people engine's characters), what they say, and the
//! rumours they trade for a round.
//!
//! Every crewed station has a bar with [`PATRONS`] patrons, a pure function of the world seed
//! and the station. Human patrons are people-engine characters ([`generate_character`]: a name,
//! a look and a voice); at an alien-run station most patrons are its own kin. Each patron has a
//! line about their trade and, for most, a **rumour** pointing at a world near the station:
//! what it is like, what to bring (its hazard tags), what is said to be there. Buying the round
//! ([`super::StationAction::BuyRound`]) charts that world's system on the nav chart and puts a
//! waypoint on the world.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    reason = "hash picks and bounded shop figures"
)]
use super::{
    StationId,
    bounty::BoardContext,
    hazard::{PlanetHazard, hazards_of},
    profile::{StationOwner, StationProfile},
    station_hash,
};
use crate::{
    people::{Character, Culture, character::CharacterId, generate_character},
    space::{Space, SpaceBodyId, SystemRef},
};

/// Patrons at a bar.
pub const PATRONS: u8 = 3;
/// What a round costs at a human bar, and at an alien one.
pub const ROUND_PRICE: u32 = 30;
pub const ALIEN_ROUND_PRICE: u32 = 45;

const SALT_PATRON: u64 = 0x57A7_0301;

/// A rumour about a world.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rumour {
    pub body: SpaceBodyId,
    pub text: String,
    /// A rumour of a faction boss's lair instead (Wave 12 integration with the hostiles
    /// track): the round charts the lair's system and points the waypoint at it.
    pub lair: Option<crate::hostiles::Faction>,
}

/// One patron.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Patron {
    pub name: String,
    /// What they do ("ore hauler").
    pub trade: &'static str,
    /// One of the station's alien kin.
    pub alien: bool,
    /// A human patron (the people engine's character: name, look, voice), for the portrait.
    pub character: Option<Character>,
    /// A seed for an alien patron's portrait.
    pub look: u64,
    /// What they say on the first word.
    pub line: String,
    pub rumour: Option<Rumour>,
}

const TRADES: [&str; 8] = [
    "ore hauler",
    "dock hand",
    "survey pilot",
    "ship's medic",
    "salvager",
    "drive mechanic",
    "courier",
    "retired miner",
];
const ALIEN_TRADES: [&str; 4] = [
    "trade envoy",
    "hull-singer",
    "star reader",
    "keeper of tolls",
];

const LINES: [&str; 8] = [
    "Hauled ore out past the belt for twenty years. The trick is the way back.",
    "Mind the docking lights. Green means slow, not go.",
    "Charted three worlds last month. Named one after my cat.",
    "Bring air. Whatever they told you, bring more air.",
    "Wrecks out there still hum. Something in them never powered down.",
    "Your coils are singing flat. A tune-up would buy you a few sectors.",
    "Everything out here is a delivery. Even you.",
    "Dug the home world down to its heart once. Space is quieter.",
];
const ALIEN_LINES: [&str; 4] = [
    "Your kind lands so loudly. The ground remembers.",
    "We sell the light we keep. Ask the yard for our tech.",
    "The stars here are old. We read them like letters.",
    "Pay the toll, small ship, and the lanes are yours.",
];
const ALIEN_NAMES: [&str; 8] = [
    "Ish'vael", "Oru-Tenn", "Quessa", "Mmorrin", "Ah'lith", "Zhevvet", "Ullan", "Seirro",
];

/// The theme's flavour in a rumour.
const fn theme_hint(theme: crate::space::Theme) -> &'static str {
    use crate::space::Theme;
    match theme {
        Theme::Jungle => "green from pole to pole, and the green bites back",
        Theme::OceanArchipelago => "nothing but sea and little islands",
        Theme::Ice => "white all over; the ice goes down for ever",
        Theme::DesertDunes => "dunes the size of mountains, moving at night",
        Theme::Volcanic => "the ground glows at night",
        Theme::Crystal => "crystal that sings when you drill it",
        Theme::Fungal => "mushrooms taller than a rig",
        Theme::MachineWorld => "built over, every inch, and still running",
        Theme::GasGiant => "cities floating in the clouds",
        Theme::TidallyLocked => "one side burns, the other never sees the sun",
        Theme::RingWorld => "rings you can see from three systems off",
        Theme::DeadWorld => "ruins, and nobody left to ask about them",
        Theme::Barren | Theme::Home | Theme::Moon => "bare rock with something under it",
    }
}

const fn hazard_hint(hazard: PlanetHazard) -> &'static str {
    match hazard {
        PlanetHazard::Heat => "Bring heat shielding",
        PlanetHazard::Cold => "Insulate or freeze",
        PlanetHazard::ToxicAir => "Don't breathe it without filters",
        PlanetHazard::HighGravity => "Fit struts, it pulls hard",
    }
}

/// The bar's patrons at `station` (empty where nobody is aboard).
#[must_use]
pub fn patrons(space: &Space, station: &StationProfile, context: &BoardContext) -> Vec<Patron> {
    if !station.bar {
        return Vec::new();
    }
    let seed = space.seed();
    // Worlds worth a rumour: elsewhere first (that is what a rumour is for), then near.
    let mut worlds: Vec<SpaceBodyId> = context
        .bodies
        .iter()
        .filter(|body| body.elsewhere)
        .map(|body| body.id)
        .take(6)
        .collect();
    worlds.extend(
        context
            .bodies
            .iter()
            .filter(|body| !body.elsewhere)
            .map(|body| body.id)
            .take(4),
    );
    (0..PATRONS)
        .map(|index| patron(space, seed, station, index, &worlds))
        .collect()
}

fn patron(
    space: &Space,
    seed: u64,
    station: &StationProfile,
    index: u8,
    worlds: &[SpaceBodyId],
) -> Patron {
    let roll = station_hash(seed, station.id, SALT_PATRON ^ (u64::from(index) << 32));
    let alien = matches!(station.owner, StationOwner::Alien(_)) && index < 2;
    let pick = |n: usize, shift: u32| ((roll >> shift) % n as u64) as usize;
    let (name, trade, character, line) = if alien {
        (
            ALIEN_NAMES[pick(ALIEN_NAMES.len(), 3)].to_owned(),
            ALIEN_TRADES[pick(ALIEN_TRADES.len(), 9)],
            None,
            ALIEN_LINES[pick(ALIEN_LINES.len(), 15)].to_owned(),
        )
    } else {
        let culture = [
            Culture::Frontier,
            Culture::Scrapper,
            Culture::Lunar,
            Culture::Aeronaut,
        ][pick(4, 21)];
        let character = generate_character(
            seed,
            CharacterId::new(bar_village(station.id), u16::from(index)),
            culture,
        );
        (
            character.name.full(),
            TRADES[pick(TRADES.len(), 9)],
            Some(character),
            LINES[pick(LINES.len(), 15)].to_owned(),
        )
    };
    // Two in three have a rumour; a relay's bar hears everything. The last patron of a bar
    // out past the home system knows where a faction's boss holds court (the lairs are
    // otherwise only met by flying through them).
    let rumoured = station.kind == crate::space::StationKind::Relay || pick(3, 27) != 0;
    let far = station.id.sector != crate::space::HOME_SECTOR || station.id.system != 0;
    let lair = (far && index + 1 == PATRONS)
        .then(|| crate::hostiles::Faction::ALL[pick(crate::hostiles::Faction::ALL.len(), 39)])
        .and_then(|faction| lair_rumour(space, faction, worlds));
    let rumour = if lair.is_some() {
        lair
    } else if rumoured && !worlds.is_empty() {
        let body = worlds[(usize::from(index) + pick(worlds.len(), 33)) % worlds.len()];
        rumour_about(space, body)
    } else {
        None
    };
    Patron {
        name,
        trade,
        alien,
        character,
        look: roll,
        line,
        rumour,
    }
}

/// The village id a station's bar draws its people from (a namespace of its own).
#[must_use]
pub const fn bar_village(station: StationId) -> u32 {
    let packed = (station.sector.x as u32).wrapping_mul(0x9E37)
        ^ (station.sector.y as u32).wrapping_mul(0x7F4A)
        ^ ((station.system as u32) << 4)
        ^ station.index as u32;
    0x3E00_0000 | (packed & 0x00FF_FFFF)
}

/// A rumour about `body` (`None` if it is not a generated body).
#[must_use]
pub fn rumour_about(space: &Space, body: SpaceBodyId) -> Option<Rumour> {
    let system = space.system(SystemRef {
        sector: body.sector,
        system: body.system,
    })?;
    let found = system.bodies.get(usize::from(body.body))?;
    let name = system.body_name(body.body);
    let hazards = hazards_of(found);
    let warning = hazards
        .iter()
        .max_by_key(|(_, severity)| *severity)
        .map_or("Easy landing, they say", |(hazard, _)| hazard_hint(*hazard));
    Some(Rumour {
        body,
        text: format!(
            "{name}, out in {}: {}. {warning}.",
            system.name,
            theme_hint(found.theme),
        ),
        lair: None,
    })
}

/// A rumour of `faction`'s boss lair (`None` if the galaxy has none for it).
fn lair_rumour(
    space: &Space,
    faction: crate::hostiles::Faction,
    worlds: &[SpaceBodyId],
) -> Option<Rumour> {
    let lair = crate::hostiles::boss_lair(space.seed(), faction)?;
    let body = *worlds.first()?;
    let boss = faction.boss().name();
    Some(Rumour {
        body,
        text: format!(
            "Stay clear of the {} if you value your hull: {boss} holds court out there, on \
             the way in from home. Go armed, or go round.",
            faction.name()
        ),
        lair: (!lair.system.is_home()).then_some(faction),
    })
}
