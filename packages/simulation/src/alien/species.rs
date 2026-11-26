//! The species registry: who the friendly aliens are (Wave 12 aliens track).
//!
//! Each [`SpeciesDef`] is the design of one species as data: body plan, culture, voice,
//! building style, what it values and dislikes, where it settles, what it trades, its
//! residents' trades and how its towns are named. Presentation (the art, the voice patches,
//! the glyph shapes) reads the same definitions.

use crate::{people::Role, space::Theme, terrain::MineralKind};

use super::{AlienSpecies, trade::AlienGood};

/// One species, as data.
#[derive(Clone, Copy, Debug)]
pub struct SpeciesDef {
    pub species: AlienSpecies,
    /// What people call them ("Vessi").
    pub name: &'static str,
    /// One of them ("a Vessi").
    pub singular: &'static str,
    /// What they call themselves in their own tongue.
    pub endonym: &'static str,
    /// Their body plan, as a player would describe them.
    pub body: &'static str,
    /// How they live.
    pub culture: &'static str,
    /// What they value (a trait list; quests and dialogue read it).
    pub values: &'static [&'static str],
    /// What offends them.
    pub dislikes: &'static str,
    /// How they sound (the voice patch reads it).
    pub voice: &'static str,
    /// How they build.
    pub building_style: &'static str,
    /// The space themes their towns prefer (a body of another theme can still be a first
    /// home: they settle where they must).
    pub habitats: &'static [Theme],
    /// Ores they prize, with the share of the home market's price they pay (per mille).
    pub valued_ores: &'static [(MineralKind, u16)],
    /// What their trade halls sell.
    pub goods: &'static [AlienGood],
    /// Their greeting, in their tongue and as it means.
    pub greeting: (&'static str, &'static str),
    /// Their farewell, in their tongue and as it means.
    pub farewell: (&'static str, &'static str),
    /// Town names: a first part and a second part, rolled per town.
    pub town_first: &'static [&'static str],
    pub town_second: &'static [&'static str],
    /// The trades their residents take (in order: the trader, the lexicon keeper, the
    /// elder, the builder, then the rest).
    pub trades: &'static [Role],
    /// Their own words for those trades (same order as [`Self::trades`]).
    pub trade_titles: &'static [&'static str],
    /// Glyphs in their writing (the renderer draws that many shapes).
    pub glyphs: u8,
    /// Height of one of them, sixteenths of a tile (art and contact boxes).
    pub height_px: u8,
    /// How they move: walk, scuttle or drift (the renderer's gait).
    pub gait: Gait,
}

/// How a species moves (the renderer's gait).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gait {
    /// Long, slow strides on stilt legs.
    Stride,
    /// Quick, low scuttling on six limbs.
    Scuttle,
    /// Drifting above the ground.
    Drift,
}

/// The three species, in index order.
pub static SPECIES: [SpeciesDef; 3] = [VESSI, TOKKARI, SIPHRA];

/// The definition of `species`.
#[must_use]
pub const fn species(species: AlienSpecies) -> &'static SpeciesDef {
    &SPECIES[species.index() as usize]
}

const VESSI: SpeciesDef = SpeciesDef {
    species: AlienSpecies::Vessi,
    name: "Vessi",
    singular: "a Vessi",
    endonym: "Veh-ssii-ul",
    body: "Tall as two rigs, on four thin stilt legs, with a bell-shaped body under white \
           frost-fur, a long neck and a lantern-sac at the throat that glows when they sing.",
    culture: "Patient singers of the ice worlds. They keep time in songs, settle every quarrel \
              in a duet, and never hurry. A Vessi town sings the dawn up together.",
    values: &["warmth", "patience", "harmony", "remembered songs"],
    dislikes: "haste, and engines left running",
    voice: "low hoots that bend into two-note chords",
    building_style: "spiral snow-vaults of packed ice, lit warm from inside, with song spires \
                     of frozen glass",
    habitats: &[Theme::Ice, Theme::TidallyLocked, Theme::OceanArchipelago],
    valued_ores: &[
        (MineralKind::Ruby, 1_700),
        (MineralKind::Copper, 1_450),
        (MineralKind::Uranium, 1_500),
        (MineralKind::Gold, 1_250),
    ],
    goods: &[
        AlienGood::Primer(AlienSpecies::Vessi),
        AlienGood::SongLantern,
        AlienGood::FrostDown,
        AlienGood::GlowPearls,
    ],
    greeting: ("Hoo-veh, ssii", "warm be your song"),
    farewell: ("Ul-ul, vehn", "the song goes with you"),
    town_first: &["Hoo", "Vel", "Ssei", "Ulu", "Nev", "Aum"],
    town_second: &["-vahl", "-ssuun", "-hoven", "-ilu", " Spire", " Hollow"],
    trades: &[
        Role::Trader,
        Role::Scholar,
        Role::Storyteller,
        Role::Smith,
        Role::Farmer,
        Role::Healer,
        Role::ShrineKeeper,
        Role::Courier,
    ],
    trade_titles: &[
        "warmth-trader",
        "song-teacher",
        "eldest voice",
        "vault-builder",
        "moss-keeper",
        "breath-mender",
        "spire-tender",
        "far-walker",
    ],
    glyphs: 12,
    height_px: 40,
    gait: Gait::Stride,
};

const TOKKARI: SpeciesDef = SpeciesDef {
    species: AlienSpecies::Tokkari,
    name: "Tokkari",
    singular: "a Tokkari",
    endonym: "Tok-kaa-ri",
    body: "Low and broad, on four short legs with two great digging claws, under a domed \
           carapace studded with grown-in gems; eyes on two stalks that never stop looking.",
    culture: "Burrowing smiths of the rocky worlds. Every debt is a knot in a cord, every \
              guild has its forge, and an honest weight is holy. They build down as much as up.",
    values: &["craft", "honest weight", "paid debts", "good iron"],
    dislikes: "short weights and broken promises",
    voice: "quick clicks and clacks with a percussive rattle",
    building_style: "terraced mound-halls of fused stone, half sunk into the ground, with round \
                     doors and forge chimneys glowing orange",
    habitats: &[
        Theme::DesertDunes,
        Theme::Volcanic,
        Theme::Crystal,
        Theme::Jungle,
        Theme::Fungal,
        Theme::MachineWorld,
    ],
    valued_ores: &[
        (MineralKind::Iron, 1_900),
        (MineralKind::Platinum, 1_400),
        (MineralKind::Mythril, 1_350),
        (MineralKind::Silver, 1_300),
    ],
    goods: &[
        AlienGood::Primer(AlienSpecies::Tokkari),
        AlienGood::GeodeLamp,
        AlienGood::HullPlates,
        AlienGood::VeinChart,
    ],
    greeting: ("Tok! Kaa-tok", "your weight is true"),
    farewell: ("Kri-kri, tok", "the knot is tied"),
    town_first: &["Krak", "Tor", "Bokka", "Dur", "Gakk", "Murr"],
    town_second: &["-hold", "-kar", " Deep", "-tokk", " Forge", "-mound"],
    trades: &[
        Role::Trader,
        Role::Scholar,
        Role::RetiredMiner,
        Role::Smith,
        Role::Prospector,
        Role::Mechanic,
        Role::Brewer,
        Role::Warden,
    ],
    trade_titles: &[
        "weigh-master",
        "knot-reader",
        "eldest claw",
        "mound-builder",
        "vein-sniffer",
        "forge-hand",
        "brew-keeper",
        "door-guard",
    ],
    glyphs: 10,
    height_px: 20,
    gait: Gait::Scuttle,
};

const SIPHRA: SpeciesDef = SpeciesDef {
    species: AlienSpecies::Siphra,
    name: "Siphra",
    singular: "a Siphra",
    endonym: "Sii-phra-ea",
    body: "A drifting bell of living glass the size of a pilot, trailing long tendrils of \
           light; they feed on starlight and never touch the ground.",
    culture: "Memory keepers of the airless moons. Each Siphra carries the memories of the \
              ones before it, and their eldest remember ships older than any rig: the builders \
              passing between the stars.",
    values: &["memory", "starlight", "stories", "clear crystal"],
    dislikes: "forgetting, and the dark between stars",
    voice: "glassy chimes and ringing harmonics",
    building_style: "lantern-pods of grown glass hung from crystal pylons, tethered over the \
                     ground and glowing like slow stars",
    habitats: &[
        Theme::Barren,
        Theme::DeadWorld,
        Theme::RingWorld,
        Theme::GasGiant,
    ],
    valued_ores: &[
        (MineralKind::Diamond, 1_600),
        (MineralKind::Emerald, 1_550),
        (MineralKind::Skystone, 1_800),
        (MineralKind::CloudSilver, 1_500),
    ],
    goods: &[
        AlienGood::Primer(AlienSpecies::Siphra),
        AlienGood::MemoryGlass,
        AlienGood::AirBloom,
        AlienGood::StarMemory,
    ],
    greeting: ("Sii... ea, phra", "we remember you now"),
    farewell: ("Ea-sii", "you are remembered"),
    town_first: &["Sii", "Ea", "Phrae", "Lume", "Oss", "Thae"],
    town_second: &["-shine", "-mere", " Pylons", "-ea", " Drift", "-whisper"],
    trades: &[
        Role::Trader,
        Role::Scholar,
        Role::Storyteller,
        Role::Tinkerer,
        Role::Cartographer,
        Role::ShrineKeeper,
        Role::Healer,
        Role::Hermit,
    ],
    trade_titles: &[
        "light-trader",
        "memory-teacher",
        "eldest memory",
        "pod-grower",
        "star-reader",
        "pylon-singer",
        "glass-mender",
        "far-dreamer",
    ],
    glyphs: 14,
    height_px: 28,
    gait: Gait::Drift,
};

impl SpeciesDef {
    /// Their word for `role` (or the people engine's when they have none).
    #[must_use]
    pub fn trade_title(&self, role: Role) -> &'static str {
        self.trades
            .iter()
            .position(|trade| *trade == role)
            .and_then(|index| self.trade_titles.get(index))
            .copied()
            .unwrap_or_else(|| role.name())
    }

    /// The share of the market price (per mille) they pay for `mineral`, if they buy it.
    #[must_use]
    pub fn values_ore(&self, mineral: MineralKind) -> Option<u16> {
        self.valued_ores
            .iter()
            .find(|(ore, _)| *ore == mineral)
            .map(|(_, permille)| *permille)
    }

    /// Whether they settle bodies of `theme` by choice.
    #[must_use]
    pub fn likes(&self, theme: Theme) -> bool {
        self.habitats.contains(&theme)
    }
}
