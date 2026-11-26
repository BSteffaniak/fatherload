//! The character model and its deterministic generator.

use serde::{Deserialize, Serialize};

use super::{
    PeopleRng,
    dialogue::grammar::{self, Vars},
    names::{self, Culture, PersonName},
    salt,
    traits::{Fear, Personality, Quirk, Tone, TraitBias, Value, Want},
};

/// A village (or settlement) id. WP-VILLAGE assigns these when it places villages; until then
/// any stable number works (the report uses `0..n`).
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub struct VillageId(pub u32);

/// Stable identity of a generated character: their home village and their index in it.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub struct CharacterId {
    pub village: VillageId,
    pub index: u16,
}

impl CharacterId {
    #[must_use]
    pub const fn new(village: u32, index: u16) -> Self {
        Self {
            village: VillageId(village),
            index,
        }
    }
}

/// What a character does for a living.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Role {
    Prospector,
    Smith,
    Trader,
    Farmer,
    Innkeeper,
    Mechanic,
    Scholar,
    Cartographer,
    Healer,
    ShrineKeeper,
    Hermit,
    Courier,
    RetiredMiner,
    Brewer,
    Tinkerer,
    Warden,
    Baker,
    Storyteller,
    Apprentice,
    /// Sky occupation (WP-SKY): flies and fixes the airships of the sky towns.
    Aeronaut,
    /// Sky occupation: keeps the bells and the gardens of a sky monastery.
    Monk,
    /// Sky occupation: chases storms across the archipelago for the thrill and the readings.
    StormChaser,
    /// Sky occupation: runs goods between the islands without asking who owns them.
    Smuggler,
    /// Lunar occupation (Wave 6): grows the colony's greens under lamps.
    Hydroponist,
    /// Lunar occupation: keeps the domes airtight and the air scrubbers humming.
    DomeTech,
    /// Lunar occupation: maps the Moon crater by crater.
    Selenographer,
    /// Lunar occupation: keeps the colony's stores and signs for every crate.
    Quartermaster,
}

/// The kind of one-of-a-kind thing a character can offer (WP-PEOPLE fills in the details).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum OfferKind {
    /// A one-of-a-kind rig part variant.
    PartVariant,
    /// A recipe (consumables, refining, base modules).
    Recipe,
    /// A cosmetic for the rig.
    Cosmetic,
    /// A map secret: a hidden cave, vault or route.
    MapSecret,
    /// An apprenticeship: a small permanent perk.
    Apprenticeship,
    /// Rumours: hints about places and people.
    Rumours,
}

impl OfferKind {
    pub const ALL: [Self; 6] = [
        Self::PartVariant,
        Self::Recipe,
        Self::Cosmetic,
        Self::MapSecret,
        Self::Apprenticeship,
        Self::Rumours,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PartVariant => "a one-of-a-kind part",
            Self::Recipe => "a recipe",
            Self::Cosmetic => "a cosmetic",
            Self::MapSecret => "a map secret",
            Self::Apprenticeship => "an apprenticeship",
            Self::Rumours => "rumours",
        }
    }
}

impl Role {
    pub const ALL: [Self; 19] = [
        Self::Prospector,
        Self::Smith,
        Self::Trader,
        Self::Farmer,
        Self::Innkeeper,
        Self::Mechanic,
        Self::Scholar,
        Self::Cartographer,
        Self::Healer,
        Self::ShrineKeeper,
        Self::Hermit,
        Self::Courier,
        Self::RetiredMiner,
        Self::Brewer,
        Self::Tinkerer,
        Self::Warden,
        Self::Baker,
        Self::Storyteller,
        Self::Apprentice,
    ];

    /// The sky occupations (WP-SKY). They are not in [`Self::ALL`] (which every ground culture
    /// rolls from, so adding to it would reshuffle old worlds' people); sky cultures roll from
    /// [`Self::SKY_POOL`].
    pub const SKY: [Self; 4] = [
        Self::Aeronaut,
        Self::Monk,
        Self::StormChaser,
        Self::Smuggler,
    ];

    /// The lunar occupations (Wave 6), only rolled by the Moon's colonists
    /// ([`Self::LUNAR_POOL`]).
    pub const LUNAR: [Self; 4] = [
        Self::Hydroponist,
        Self::DomeTech,
        Self::Selenographer,
        Self::Quartermaster,
    ];

    /// The roles the Moon's colonists roll from: the lunar occupations and the trades a
    /// research station still needs.
    pub const LUNAR_POOL: [Self; 9] = [
        Self::Hydroponist,
        Self::DomeTech,
        Self::Selenographer,
        Self::Quartermaster,
        Self::Scholar,
        Self::Healer,
        Self::Mechanic,
        Self::Cartographer,
        Self::Innkeeper,
    ];

    /// The roles sky-town residents roll from: the sky occupations and the trades a town in
    /// the clouds still needs.
    pub const SKY_POOL: [Self; 16] = [
        Self::Aeronaut,
        Self::Monk,
        Self::StormChaser,
        Self::Smuggler,
        Self::Trader,
        Self::Farmer,
        Self::Innkeeper,
        Self::Mechanic,
        Self::Scholar,
        Self::Cartographer,
        Self::Healer,
        Self::ShrineKeeper,
        Self::Courier,
        Self::Tinkerer,
        Self::Brewer,
        Self::Apprentice,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Prospector => "prospector",
            Self::Smith => "smith",
            Self::Trader => "trader",
            Self::Farmer => "farmer",
            Self::Innkeeper => "innkeeper",
            Self::Mechanic => "mechanic",
            Self::Scholar => "scholar",
            Self::Cartographer => "cartographer",
            Self::Healer => "healer",
            Self::ShrineKeeper => "shrine keeper",
            Self::Hermit => "hermit",
            Self::Courier => "courier",
            Self::RetiredMiner => "retired miner",
            Self::Brewer => "brewer",
            Self::Tinkerer => "tinkerer",
            Self::Warden => "warden",
            Self::Baker => "baker",
            Self::Storyteller => "storyteller",
            Self::Apprentice => "apprentice",
            Self::Aeronaut => "aeronaut",
            Self::Monk => "monk",
            Self::StormChaser => "storm-chaser",
            Self::Smuggler => "smuggler",
            Self::Hydroponist => "hydroponist",
            Self::DomeTech => "dome tech",
            Self::Selenographer => "selenographer",
            Self::Quartermaster => "quartermaster",
        }
    }

    /// How the role nudges rolled traits.
    #[must_use]
    pub const fn bias(self) -> TraitBias {
        match self {
            Self::Prospector => TraitBias::new(10, -5, -5, 0, 0),
            Self::Smith => TraitBias::new(-5, 20, -5, 0, -5),
            Self::Trader => TraitBias::new(0, 5, 20, -10, 0),
            Self::Farmer => TraitBias::new(-10, 10, 0, 10, 0),
            Self::Innkeeper => TraitBias::new(0, 0, 20, 10, 0),
            Self::Mechanic => TraitBias::new(5, 10, -5, -5, 0),
            Self::Scholar => TraitBias::new(25, 10, -10, 0, 5),
            Self::Cartographer => TraitBias::new(15, 15, -5, 0, 0),
            Self::Healer => TraitBias::new(0, 10, 0, 20, 0),
            Self::ShrineKeeper => TraitBias::new(15, 5, -10, 10, -5),
            Self::Hermit => TraitBias::new(10, 0, -30, -10, 0),
            Self::Courier => TraitBias::new(5, -10, 10, 0, -5),
            Self::RetiredMiner => TraitBias::new(-5, 5, 0, -5, -5),
            Self::Brewer => TraitBias::new(10, -5, 10, 5, 0),
            Self::Tinkerer => TraitBias::new(20, -15, 0, 0, 5),
            Self::Warden => TraitBias::new(-5, 15, 0, -10, -5),
            Self::Baker => TraitBias::new(0, 10, 5, 15, 0),
            Self::Storyteller => TraitBias::new(15, -5, 20, 5, 0),
            Self::Apprentice => TraitBias::new(5, -5, 0, 5, 15),
            Self::Aeronaut => TraitBias::new(15, 5, 10, 0, -10),
            Self::Monk => TraitBias::new(15, 15, -15, 15, -10),
            Self::StormChaser => TraitBias::new(20, -15, 15, -5, -15),
            Self::Smuggler => TraitBias::new(5, -10, 10, -15, 5),
            Self::Hydroponist => TraitBias::new(5, 15, -5, 15, 0),
            Self::DomeTech => TraitBias::new(0, 25, -10, 0, 5),
            Self::Selenographer => TraitBias::new(25, 15, -15, 0, 0),
            Self::Quartermaster => TraitBias::new(-5, 25, 5, -5, 0),
        }
    }

    /// The role's usual offer.
    #[must_use]
    pub const fn offer(self) -> OfferKind {
        match self {
            Self::Smith | Self::Mechanic | Self::Tinkerer | Self::Aeronaut | Self::DomeTech => {
                OfferKind::PartVariant
            }
            Self::StormChaser => OfferKind::Rumours,
            Self::Farmer | Self::Healer | Self::Brewer | Self::Baker | Self::Hydroponist => {
                OfferKind::Recipe
            }
            Self::Trader | Self::Smuggler | Self::Quartermaster => OfferKind::Cosmetic,
            Self::Prospector
            | Self::Scholar
            | Self::Cartographer
            | Self::Hermit
            | Self::Selenographer => OfferKind::MapSecret,
            Self::ShrineKeeper | Self::RetiredMiner | Self::Warden | Self::Monk => {
                OfferKind::Apprenticeship
            }
            Self::Innkeeper | Self::Courier | Self::Storyteller | Self::Apprentice => {
                OfferKind::Rumours
            }
        }
    }

    /// Youngest and oldest age for the role.
    #[must_use]
    pub const fn ages(self) -> (u8, u8) {
        match self {
            Self::Apprentice => (15, 23),
            Self::Courier => (17, 40),
            Self::RetiredMiner => (58, 86),
            Self::Hermit | Self::ShrineKeeper => (35, 84),
            Self::Scholar | Self::Healer => (24, 75),
            _ => (20, 68),
        }
    }

    /// Gerund phrases about the work ("Spent the morning {work}").
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "three phrases per role")]
    pub const fn work(self) -> &'static [&'static str] {
        match self {
            Self::Prospector => &[
                "panning the creek for colour",
                "staking a claim nobody else wants",
                "sniffing rock samples, which is a real technique",
            ],
            Self::Smith => &[
                "hammering out drill teeth",
                "re-tempering a bit somebody cooked",
                "arguing with a stubborn horseshoe",
            ],
            Self::Trader => &[
                "haggling over a crate of lamp oil",
                "counting stock that won't count right",
                "selling a hat to a man who already had two",
            ],
            Self::Farmer => &[
                "coaxing turnips out of rocky ground",
                "chasing a goat off the roof",
                "fixing the fence the goat broke",
            ],
            Self::Innkeeper => &[
                "sweeping up after the night shift",
                "brewing coffee strong enough to drill with",
                "pretending not to hear the card players cheat",
            ],
            Self::Mechanic => &[
                "rebuilding a gearbox out of three dead ones",
                "listening to engines cough",
                "fishing a bolt out of a fuel line",
            ],
            Self::Scholar => &[
                "copying old survey maps by candlelight",
                "arguing with a book",
                "cataloguing fossils nobody asked me to catalogue",
            ],
            Self::Cartographer => &[
                "mapping the gullies east of here",
                "redrawing a ridge that turned out to be a cloud",
                "measuring things twice and drawing them once",
            ],
            Self::Healer => &[
                "patching up drillers",
                "drying herbs for the winter",
                "picking gravel out of someone's elbow",
            ],
            Self::ShrineKeeper => &[
                "lighting lamps at the shrine",
                "listening to the stones",
                "polishing offerings left by nervous miners",
            ],
            Self::Hermit => &[
                "keeping out of everyone's way",
                "watching the hills",
                "talking to my kettle",
            ],
            Self::Courier => &[
                "running parcels up and down the valley",
                "outrunning a thunderstorm",
                "delivering a letter to the wrong house, twice",
            ],
            Self::RetiredMiner => &[
                "telling the young ones how it used to be",
                "missing the dark, just a little",
                "sorting forty years of rock samples",
            ],
            Self::Brewer => &[
                "babysitting a barrel of something fizzy",
                "naming a new ale",
                "mopping up after an over-enthusiastic barrel",
            ],
            Self::Tinkerer => &[
                "taking apart a clock to see why it ticks",
                "inventing a better spoon",
                "putting out a small, educational fire",
            ],
            Self::Warden => &[
                "walking the boundary",
                "keeping an eye on the old shaft",
                "telling kids to stop climbing the headframe",
            ],
            Self::Baker => &[
                "proofing dough at four in the morning",
                "burning only some of the bread",
                "inventing a pie that survives a mine shaft",
            ],
            Self::Storyteller => &[
                "collecting stories from passing drillers",
                "making up better endings",
                "rehearsing the ghost story for Saturday",
            ],
            Self::Aeronaut => &[
                "patching a gas envelope",
                "trimming ballast for a crosswind",
                "talking a nervous airship down onto a mooring",
            ],
            Self::Monk => &[
                "ringing the noon bell for nobody in particular",
                "weeding the cloud garden",
                "copying out the old wind charts",
            ],
            Self::StormChaser => &[
                "flying into a squall on purpose",
                "reading a barometer like a love letter",
                "drying out after the last storm",
            ],
            Self::Smuggler => &[
                "moving crates that are definitely full of wool",
                "forgetting where a shipment came from",
                "paying a customs man in compliments",
            ],
            Self::Hydroponist => &[
                "coaxing lettuce out of a nutrient tray",
                "arguing with the grow lamps about the time of day",
                "pollinating tomatoes with a very small brush",
            ],
            Self::DomeTech => &[
                "chasing a hiss along the dome seals",
                "swapping the scrubber filters",
                "patching a micrometeorite pinhole with chewing resin",
            ],
            Self::Selenographer => &[
                "naming a crater nobody asked me to name",
                "measuring shadows at the terminator",
                "redrawing the rille west of the pad",
            ],
            Self::Quartermaster => &[
                "counting oxygen candles twice",
                "signing for a crate of socks that took a month to arrive",
                "rationing the good coffee",
            ],
            Self::Apprentice => &[
                "learning the trade the hard way",
                "sweeping the workshop again",
                "breaking things in instructive ways",
            ],
        }
    }
}

/// Pronouns, for UI and third-person text.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Pronouns {
    She,
    He,
    They,
}

impl Pronouns {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::She => "she/her",
            Self::He => "he/him",
            Self::They => "they/them",
        }
    }
}

/// A seed for procedural sprite assembly plus the coarse picks it decodes to.
///
/// WP-PEOPLE assembles the sprite from `seed`; the picks give the report (and any text UI)
/// words that match it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Appearance {
    pub seed: u64,
    /// Index into [`Appearance::BUILDS`].
    pub build: u8,
    /// Index into [`Appearance::HAIR`].
    pub hair: u8,
    /// Index into [`Appearance::ACCESSORIES`].
    pub accessory: u8,
    /// Palette index for clothes (WP-PEOPLE's palette).
    pub outfit: u8,
}

impl Appearance {
    pub const BUILDS: [&'static str; 6] = ["slight", "wiry", "stocky", "broad", "lanky", "round"];
    pub const HAIR: [&'static str; 9] = [
        "coal-black",
        "copper",
        "straw-blond",
        "silver",
        "chestnut",
        "snow-white",
        "rust-red",
        "dark brown",
        "no",
    ];
    pub const ACCESSORIES: [&'static str; 10] = [
        "brass goggles",
        "a battered hat",
        "a red neckerchief",
        "a lamp helmet",
        "wire spectacles",
        "a tool belt",
        "a patched scarf",
        "washer earrings",
        "a feathered cap",
        "a leather apron",
    ];

    fn roll(seed: u64, age: u8) -> Self {
        let mut rng = PeopleRng::new(seed, 0xA9, 0, 0);
        let rolled = u8::try_from(rng.below(Self::HAIR.len())).unwrap_or(0);
        // Older folk tend to go silver or white.
        let hair = match (age >= 60 && rng.chance(60), rng.chance(50)) {
            (true, true) => 3,
            (true, false) => 5,
            (false, _) => rolled,
        };
        Self {
            seed: rng.next_u64(),
            build: u8::try_from(rng.below(Self::BUILDS.len())).unwrap_or(0),
            hair,
            accessory: u8::try_from(rng.below(Self::ACCESSORIES.len())).unwrap_or(0),
            outfit: u8::try_from(rng.below(16)).unwrap_or(0),
        }
    }

    /// "wiry, copper hair, brass goggles".
    #[must_use]
    pub fn describe(self) -> String {
        let build = Self::BUILDS[usize::from(self.build) % Self::BUILDS.len()];
        let hair = Self::HAIR[usize::from(self.hair) % Self::HAIR.len()];
        let accessory = Self::ACCESSORIES[usize::from(self.accessory) % Self::ACCESSORIES.len()];
        format!("{build}, {hair} hair, {accessory}")
    }
}

/// What happened to someone a character talks about who is not around.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum AbsentFate {
    /// Went down and never came back (a mystery, not a tragedy: their rig is somewhere).
    WentDeep,
    /// Moved away to another village.
    MovedAway,
    /// An old rival from up the valley.
    OldRival,
    /// Someone they owe money to.
    Creditor,
    /// Someone they fell out with.
    Estranged,
}

/// A person a character talks about who is not in their village.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct AbsentPerson {
    pub name: PersonName,
    /// How they relate to the character ("brother", "old rival", ...).
    pub relation: String,
    pub fate: AbsentFate,
}

/// Who a character's [`Want`] is about.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize,
)]
pub enum WantFocus {
    #[default]
    Nobody,
    /// An entry of [`Character::absent`].
    Absent(u8),
    /// A neighbour in the same population.
    Neighbour(CharacterId),
}

/// The part of a life a history beat covers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum LifeStage {
    Origin,
    TurningPoint,
    Recently,
}

/// One authored beat of a character's past, already filled in.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct HistoryBeat {
    pub stage: LifeStage,
    pub text: String,
}

/// A generated character. Rebuilt from `(seed, id, culture)` by [`generate_character`] (and
/// adjusted by [`super::generate_population`]); never needs saving itself.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Character {
    pub id: CharacterId,
    pub culture: Culture,
    pub name: PersonName,
    pub pronouns: Pronouns,
    pub age: u8,
    pub appearance: Appearance,
    pub role: Role,
    pub personality: Personality,
    /// The tone bucket they mostly speak in.
    pub tone: Tone,
    /// The tone that colours the rest of their lines.
    pub secondary_tone: Tone,
    pub quirks: Vec<Quirk>,
    pub values: Vec<Value>,
    pub want: Want,
    pub want_focus: WantFocus,
    pub fear: Fear,
    pub history: Vec<HistoryBeat>,
    pub absent: Vec<AbsentPerson>,
    pub home: VillageId,
    pub offer: OfferKind,
    /// Resting mood, `-30..=30` (see [`super::memory`]).
    pub base_mood: i8,
    /// A per-character seed for dialogue rolls.
    pub voice_seed: u64,
}

impl Character {
    /// The name of whoever their want is about, if anyone.
    #[must_use]
    pub fn focus_name<'a>(&'a self, population: Option<&'a super::Population>) -> Option<String> {
        match self.want_focus {
            WantFocus::Nobody => None,
            WantFocus::Absent(index) => self
                .absent
                .get(usize::from(index))
                .map(|person| person.name.short().to_owned()),
            WantFocus::Neighbour(id) => population
                .and_then(|population| population.get(id))
                .map(|other| other.name.short().to_owned()),
        }
    }

    /// How the want's focus relates to them ("brother", "old rival", "neighbour").
    #[must_use]
    pub fn focus_relation(&self, population: Option<&super::Population>) -> Option<String> {
        match self.want_focus {
            WantFocus::Nobody => None,
            WantFocus::Absent(index) => self
                .absent
                .get(usize::from(index))
                .map(|person| person.relation.clone()),
            WantFocus::Neighbour(id) => Some(
                population
                    .and_then(|population| population.relationship(self.id, id))
                    .map_or("neighbour", |relationship| relationship.kind.noun())
                    .to_owned(),
            ),
        }
    }
}

const FAMILY_WORDS_YOUNG: &[&str] = &[
    "brother", "sister", "mother", "father", "cousin", "uncle", "aunt",
];
const FAMILY_WORDS_OLD: &[&str] = &[
    "brother", "sister", "son", "daughter", "cousin", "nephew", "niece", "partner",
];

/// Roles each culture leans towards (picked twice as often).
const fn culture_roles(culture: Culture) -> &'static [Role] {
    match culture {
        Culture::Frontier => &[Role::Prospector, Role::Innkeeper, Role::Trader],
        Culture::Highland => &[Role::Smith, Role::Warden, Role::Farmer],
        Culture::Mesa => &[Role::Cartographer, Role::Healer, Role::Brewer],
        Culture::Riverside => &[Role::Farmer, Role::Baker, Role::Storyteller],
        Culture::Scrapper => &[Role::Mechanic, Role::Tinkerer, Role::Trader],
        Culture::Deepfolk => &[Role::ShrineKeeper, Role::Hermit, Role::Scholar],
        Culture::Aeronaut => &[Role::Aeronaut, Role::Smuggler, Role::StormChaser],
        Culture::Cloudborn => &[Role::Monk, Role::Farmer, Role::StormChaser],
        Culture::Lunar => &[Role::Hydroponist, Role::DomeTech, Role::Selenographer],
        // The merrow (Wave 9): pearl traders, song-keepers and archivists of the sea.
        Culture::Merrow => &[Role::Trader, Role::Storyteller, Role::Scholar],
        // The friendly aliens (Wave 12): their towns set every resident's trade anyway.
        Culture::Vessi => &[Role::Storyteller, Role::Healer, Role::Farmer],
        Culture::Tokkari => &[Role::Smith, Role::Prospector, Role::Trader],
        Culture::Siphra => &[Role::Scholar, Role::Cartographer, Role::ShrineKeeper],
    }
}

/// Generates the character `id` of a `culture` village under world `seed`.
///
/// Characters generated alone have no neighbours; [`super::generate_population`] adds families
/// and relationships on top.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one roll after another, in a fixed order"
)]
pub fn generate_character(seed: u64, id: CharacterId, culture: Culture) -> Character {
    let mut rng = PeopleRng::new(
        seed,
        salt::CHARACTER,
        u64::from(id.village.0),
        u64::from(id.index),
    );
    // Sky towns roll from their own pool (ground cultures keep the classic list exactly).
    let pool: &[Role] = if culture.is_lunar() {
        &Role::LUNAR_POOL
    } else if culture.is_sky() {
        &Role::SKY_POOL
    } else {
        &Role::ALL
    };
    let mut roles: Vec<(Role, u32)> = pool.iter().map(|role| (*role, 2)).collect();
    for favoured in culture_roles(culture) {
        if let Some(entry) = roles.iter_mut().find(|(role, _)| role == favoured) {
            entry.1 = 4;
        }
    }
    let role = *rng.weighted(&roles);
    let (youngest, oldest) = role.ages();
    let age = u8::try_from(rng.range(i32::from(youngest), i32::from(oldest))).unwrap_or(30);
    let personality = Personality::roll(&mut rng, role.bias());
    let (tone, secondary_tone) = personality.tones();
    let pronouns = *rng.weighted(&[(Pronouns::She, 46), (Pronouns::He, 46), (Pronouns::They, 8)]);
    let name = PersonName {
        given: names::given_name(culture, Some(pronouns), &mut rng),
        family: names::surname(culture, &mut rng),
        nickname: names::nickname(culture, &mut rng),
    };
    let appearance = Appearance::roll(rng.next_u64(), age);

    let mut quirks = vec![*rng.pick(&Quirk::ALL)];
    if rng.chance(35) {
        let second = *rng.pick(&Quirk::ALL);
        if second != quirks[0] {
            quirks.push(second);
        }
    }
    let mut values = vec![*rng.pick(&Value::ALL)];
    let second = *rng.pick(&Value::ALL);
    if second != values[0] {
        values.push(second);
    }
    let want = pick_want(role, &values, &mut rng);
    let fear = *rng.pick(&Fear::ALL);

    let mut absent = Vec::new();
    let mut want_focus = WantFocus::Nobody;
    let words = if age < 40 {
        FAMILY_WORDS_YOUNG
    } else {
        FAMILY_WORDS_OLD
    };
    let focus = match want {
        Want::FindRelative => Some(((*rng.pick(words)).to_owned(), AbsentFate::WentDeep)),
        Want::BeatRival => Some(("old rival".to_owned(), AbsentFate::OldRival)),
        Want::PayOffDebt => Some((
            (*rng.pick(&["old partner", "creditor", "former boss"])).to_owned(),
            AbsentFate::Creditor,
        )),
        Want::MakeAmends => Some((
            (*rng.pick(&["old friend", "sister", "brother", "former partner"])).to_owned(),
            AbsentFate::Estranged,
        )),
        _ => None,
    };
    if let Some((relation, fate)) = focus {
        absent.push(absent_person(culture, relation, fate, &name, &mut rng));
        want_focus = WantFocus::Absent(0);
    }
    if rng.chance(30) {
        let relation = (*rng.pick(words)).to_owned();
        absent.push(absent_person(
            culture,
            relation,
            AbsentFate::MovedAway,
            &name,
            &mut rng,
        ));
    }

    let offer = if rng.chance(20) {
        *rng.pick(&OfferKind::ALL)
    } else {
        role.offer()
    };
    let e = i32::from(personality.extraversion);
    let n = i32::from(personality.neuroticism);
    let a = i32::from(personality.agreeableness);
    let base_mood = i8::try_from(((e - n) / 4 + (a - 50) / 6).clamp(-30, 30)).unwrap_or(0);

    let mut character = Character {
        id,
        culture,
        name,
        pronouns,
        age,
        appearance,
        role,
        personality,
        tone,
        secondary_tone,
        quirks,
        values,
        want,
        want_focus,
        fear,
        history: Vec::new(),
        absent,
        home: id.village,
        offer,
        base_mood,
        voice_seed: rng.next_u64(),
    };
    character.history = history(seed, &character);
    character
}

fn pick_want(role: Role, values: &[Value], rng: &mut PeopleRng) -> Want {
    let mut weights: Vec<(Want, u32)> = Want::ALL.iter().map(|want| (*want, 3)).collect();
    let mut boost = |want: Want, by: u32| {
        if let Some(entry) = weights.iter_mut().find(|(candidate, _)| *candidate == want) {
            entry.1 += by;
        }
    };
    match role {
        Role::Tinkerer | Role::Mechanic => boost(Want::BuildInvention, 6),
        Role::Scholar | Role::ShrineKeeper | Role::Hermit => boost(Want::UncoverDeepSecret, 5),
        Role::Prospector | Role::Trader | Role::Smuggler => boost(Want::StrikeItRich, 4),
        Role::Warden | Role::Healer => boost(Want::ProtectVillage, 4),
        Role::Cartographer | Role::Courier | Role::Aeronaut | Role::StormChaser => {
            boost(Want::SeeTheWorld, 5);
        }
        Role::Monk | Role::Selenographer => boost(Want::UncoverDeepSecret, 4),
        Role::DomeTech => boost(Want::BuildInvention, 4),
        Role::Hydroponist | Role::Quartermaster => boost(Want::ProtectVillage, 3),
        Role::Apprentice => boost(Want::ProveThemselves, 5),
        _ => {}
    }
    for value in values {
        match value {
            Value::Family => boost(Want::FindRelative, 3),
            Value::Fortune => boost(Want::StrikeItRich, 3),
            Value::Knowledge | Value::Wonder => boost(Want::UncoverDeepSecret, 3),
            Value::Tradition => boost(Want::RecoverHeirloom, 4),
            Value::Craft => boost(Want::BuildInvention, 2),
            Value::Community => boost(Want::ProtectVillage, 3),
            Value::Adventure | Value::Freedom => boost(Want::SeeTheWorld, 2),
            Value::Honesty => boost(Want::PayOffDebt, 2),
        }
    }
    *rng.weighted(&weights)
}

/// The pronouns a family word implies ("brother" is a he), if any.
#[must_use]
pub fn relation_pronouns(relation: &str) -> Option<Pronouns> {
    match relation {
        "brother" | "father" | "son" | "uncle" | "nephew" | "husband" => Some(Pronouns::He),
        "sister" | "mother" | "daughter" | "aunt" | "niece" | "wife" => Some(Pronouns::She),
        _ => None,
    }
}

fn absent_person(
    culture: Culture,
    relation: String,
    fate: AbsentFate,
    of: &PersonName,
    rng: &mut PeopleRng,
) -> AbsentPerson {
    let blood = matches!(
        relation.as_str(),
        "brother"
            | "sister"
            | "mother"
            | "father"
            | "son"
            | "daughter"
            | "uncle"
            | "aunt"
            | "nephew"
            | "niece"
    );
    let pronouns = relation_pronouns(&relation);
    let mut given = names::given_name(culture, pronouns, rng);
    if given == of.given {
        given = names::given_name(culture, pronouns, rng);
    }
    let family = if blood {
        of.family.clone()
    } else {
        names::surname(culture, rng)
    };
    AbsentPerson {
        name: PersonName {
            given,
            family,
            nickname: None,
        },
        relation,
        fate,
    }
}

/// An authored history beat. `{absent}` beats need an absent person; `cultures` and `roles`
/// limit who may have it (empty: anyone).
pub(crate) struct BeatDef {
    pub stage: LifeStage,
    pub text: &'static str,
    pub cultures: &'static [Culture],
    pub roles: &'static [Role],
}

const fn beat(stage: LifeStage, text: &'static str) -> BeatDef {
    BeatDef {
        stage,
        text,
        cultures: &[],
        roles: &[],
    }
}

/// Variables history beats may use.
#[cfg(test)]
pub(crate) const BEAT_VARS: &[&str] = &[
    "village",
    "elsewhere",
    "years",
    "band",
    "absent",
    "absent_relation",
    "formerly",
];

pub(crate) const BEATS: &[BeatDef] = &[
    beat(
        LifeStage::Origin,
        "Born in {village}, in the house with the crooked chimney.",
    ),
    beat(
        LifeStage::Origin,
        "Came out to {village} {years} ago with a shovel, a mule and a cough. Only the shovel's left.",
    ),
    beat(
        LifeStage::Origin,
        "Grew up in {elsewhere}, the youngest of [nine|eleven|far too many].",
    ),
    beat(
        LifeStage::Origin,
        "Arrived on the back of a supply wagon {years} ago and simply never left.",
    ),
    beat(
        LifeStage::Origin,
        "Raised by an aunt who ran the assay office in {elsewhere}.",
    ),
    beat(
        LifeStage::Origin,
        "Was {a formerly} in {elsewhere} until the work dried up, then followed the drills here.",
    ),
    beat(
        LifeStage::Origin,
        "Turned up in {village} one winter with no luggage and a very good story. Nobody believes it.",
    ),
    BeatDef {
        stage: LifeStage::Origin,
        text: "Born under the rock in a hall lit by glow-moss; saw the open sky for the first time at twelve.",
        cultures: &[Culture::Deepfolk],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::Origin,
        text: "Built their first engine out of a bedframe and a kettle, aged nine.",
        cultures: &[Culture::Scrapper],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::Origin,
        text: "Grew up herding goats on the high slopes above {elsewhere}.",
        cultures: &[Culture::Highland, Culture::Mesa],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::Origin,
        text: "Grew up on a river barge, and still sways a little when standing still.",
        cultures: &[Culture::Riverside],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::Recently,
        text: "Went up top for a market day and came back sunburnt and amazed by clouds.",
        cultures: &[Culture::Deepfolk],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::TurningPoint,
        text: "Spent a whole year alone in the far tunnels, listening, and came back with a new name.",
        cultures: &[Culture::Deepfolk],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::TurningPoint,
        text: "Won {a formerly}'s entire workshop in a card game, and has been learning the trade ever since.",
        cultures: &[Culture::Frontier, Culture::Scrapper],
        roles: &[],
    },
    BeatDef {
        stage: LifeStage::Recently,
        text: "Has been painting the adobe walls a new colour every season, to the neighbours' delight.",
        cultures: &[Culture::Mesa],
        roles: &[],
    },
    beat(
        LifeStage::TurningPoint,
        "Lost a whole rig in {band} and walked out with only the lamp, which still works.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Won the {elsewhere} drilling contest three years running, then retired undefeated.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Was trapped two days by a cave-in in {band} and came out talking about blue lights.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Spent one glorious season rich, and one long winter spending it.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Apprenticed under a famous {formerly} in {elsewhere} who never once said 'well done'.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Watched {absent} take a rig down past {band} {years} ago. The rig never came back up.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Fell out with {absent} over a claim neither of them could have dug anyway.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Found an artifact nobody could explain, and sold it for a song. Still regrets it.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Helped build half a bridge over the {elsewhere} gorge before the money ran out.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Nursed half of {elsewhere} through the gas fever and never caught it once.",
    ),
    beat(
        LifeStage::TurningPoint,
        "Once dug a well so deep it hit something warm, and filled it back in, quickly.",
    ),
    BeatDef {
        stage: LifeStage::TurningPoint,
        text: "Mapped a cave so big it had its own weather; nobody has found the entrance since.",
        cultures: &[],
        roles: &[Role::Cartographer, Role::Prospector, Role::Hermit],
    },
    BeatDef {
        stage: LifeStage::TurningPoint,
        text: "Blew the roof off the old workshop testing an engine. Kept the engine.",
        cultures: &[],
        roles: &[Role::Tinkerer, Role::Mechanic, Role::Smith],
    },
    beat(
        LifeStage::Recently,
        "Lately has been saving every coin in a tin under a loose floorboard.",
    ),
    beat(
        LifeStage::Recently,
        "Has started taking long walks near the old shaft at night.",
    ),
    beat(
        LifeStage::Recently,
        "Recently took up the [fiddle|accordion|bagpipes], to the village's quiet dismay.",
    ),
    beat(
        LifeStage::Recently,
        "Just got back from {elsewhere} with a sunburn and a secret.",
    ),
    beat(
        LifeStage::Recently,
        "Has been writing to {absent} again, and tearing the letters up.",
    ),
    beat(
        LifeStage::Recently,
        "Is teaching a stray dog to fetch ore samples. Progress is slow.",
    ),
    beat(
        LifeStage::Recently,
        "Has been seen copying strange symbols off a derelict's hull into a notebook.",
    ),
    beat(
        LifeStage::Recently,
        "Planted a garden. Mostly rocks so far, but tidy rocks.",
    ),
    beat(
        LifeStage::Recently,
        "Keeps a lamp burning in the window, every night, for no reason they'll admit.",
    ),
];

const YEARS: &[&str] = &[
    "two winters",
    "six years",
    "a dozen years",
    "twenty-odd years",
    "nine years",
    "three summers",
];
const SHALLOW_BANDS: &[&str] = &[
    "the Clay Beds",
    "the Grey Stone",
    "the Slate Shelves",
    "the Basalt Deeps",
];

/// Renders a character's history beats (again, after a population renames people).
pub(crate) fn history(seed: u64, character: &Character) -> Vec<HistoryBeat> {
    let mut rng = PeopleRng::new(
        seed,
        salt::CHARACTER ^ 0xB0,
        u64::from(character.id.village.0),
        u64::from(character.id.index),
    );
    let rng = &mut rng;
    let mut vars = Vars::new();
    vars.set(
        "village",
        names::village_name(seed, character.home.0, character.culture),
    );
    let elsewhere_id = u32::try_from(rng.below(10_000)).unwrap_or(0) + 50_000;
    vars.set(
        "elsewhere",
        names::village_name(seed, elsewhere_id, character.culture),
    );
    vars.set("years", *rng.pick(YEARS));
    vars.set("band", *rng.pick(SHALLOW_BANDS));
    let formerly = loop {
        let role = *rng.pick(&Role::ALL);
        if role != character.role && role != Role::Apprentice {
            break role;
        }
    };
    vars.set("formerly", formerly.name());
    let absent = character
        .absent
        .iter()
        .find(|person| person.fate != AbsentFate::MovedAway)
        .or_else(|| character.absent.first());
    if let Some(person) = absent {
        vars.set(
            "absent",
            format!("their {} {}", person.relation, person.name.given),
        );
        vars.set("absent_relation", person.relation.clone());
    }

    let stages: &[LifeStage] = if character.age < 25 && character.want != Want::FindRelative {
        &[LifeStage::Origin, LifeStage::Recently]
    } else {
        &[
            LifeStage::Origin,
            LifeStage::TurningPoint,
            LifeStage::Recently,
        ]
    };
    let mut beats = Vec::new();
    for stage in stages {
        let options: Vec<&BeatDef> = BEATS
            .iter()
            .filter(|beat| beat.stage == *stage)
            .filter(|beat| beat.cultures.is_empty() || beat.cultures.contains(&character.culture))
            .filter(|beat| beat.roles.is_empty() || beat.roles.contains(&character.role))
            .filter(|beat| absent.is_some() || !beat.text.contains("{absent"))
            .filter(|beat| {
                // A missing relative's story always comes up as their turning point.
                *stage != LifeStage::TurningPoint
                    || character.want != Want::FindRelative
                    || beat.text.contains("{absent} take a rig")
            })
            .collect();
        if options.is_empty() {
            continue;
        }
        // Culture and trade beats are rarer in the table, so give them a fair chance.
        let specific: Vec<&BeatDef> = options
            .iter()
            .copied()
            .filter(|beat| !beat.cultures.is_empty() || !beat.roles.is_empty())
            .collect();
        let def = if !specific.is_empty() && rng.chance(40) {
            *rng.pick(&specific)
        } else {
            *rng.pick(&options)
        };
        beats.push(HistoryBeat {
            stage: *stage,
            text: grammar::expand(def.text, &vars, rng),
        });
    }
    beats
}

#[cfg(test)]
mod tests {
    use super::{AbsentFate, CharacterId, WantFocus, generate_character};
    use crate::people::{names::Culture, traits::Want};

    #[test]
    fn generation_is_deterministic() {
        for index in 0..50 {
            let id = CharacterId::new(3, index);
            assert_eq!(
                generate_character(42, id, Culture::Frontier),
                generate_character(42, id, Culture::Frontier)
            );
        }
        assert_ne!(
            generate_character(42, CharacterId::new(3, 1), Culture::Frontier),
            generate_character(43, CharacterId::new(3, 1), Culture::Frontier)
        );
    }

    #[test]
    fn wants_that_need_a_person_have_one() {
        for index in 0..400 {
            let character = generate_character(9, CharacterId::new(1, index), Culture::Mesa);
            let needs = matches!(
                character.want,
                Want::FindRelative | Want::BeatRival | Want::PayOffDebt | Want::MakeAmends
            );
            assert_eq!(
                needs,
                character.want_focus != WantFocus::Nobody,
                "{character:?}"
            );
            if character.want == Want::FindRelative {
                assert_eq!(character.absent[0].fate, AbsentFate::WentDeep);
                assert!(
                    character
                        .history
                        .iter()
                        .any(|beat| beat.text.contains("never came back")),
                    "{:?}",
                    character.history
                );
            }
            let (youngest, oldest) = character.role.ages();
            assert!((youngest..=oldest).contains(&character.age));
            assert!(character.history.len() >= 2);
            for beat in &character.history {
                assert!(
                    !beat.text.contains('{') && !beat.text.contains('['),
                    "{beat:?}"
                );
            }
        }
    }
}
