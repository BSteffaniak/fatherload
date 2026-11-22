//! Culture-flavoured name generators for people and villages.
//!
//! Each [`Culture`] mixes a curated list of given names with a syllable generator (so names
//! never run out), builds surnames from compound parts, and has its own taste for nicknames.
//! The cultures are invented frontier flavours, not real-world peoples. Villages (WP-VILLAGE)
//! pick a culture for their building style and residents; [`village_name`] names them.

use serde::{Deserialize, Serialize};

use super::{PeopleRng, character::Pronouns, salt};

/// A naming and building culture. Villages and their residents share one.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Culture {
    /// Timber-and-tin boomtown folk: homespun names, lots of nicknames.
    Frontier,
    /// Stone villages up in the hills: old, sturdy names and clan-ish surnames.
    Highland,
    /// Adobe towns of the mesas: warm, rolling syllables.
    Mesa,
    /// Riverside timber villages of the plains and forests: soft nature surnames.
    Riverside,
    /// Scrap-metal camps: short punchy names and nicknames that stuck.
    Scrapper,
    /// The underground settlements: quiet, vowel-rich names and lamp epithets.
    Deepfolk,
    /// Sky culture (WP-SKY): the aeronauts of the airship docks and floating markets, brassy
    /// names and nicknames earned in the wind.
    Aeronaut,
    /// Sky culture: the cloud farmers and monks of the high isles, soft airy names and
    /// "of the" epithets for the winds they were born under.
    Cloudborn,
    /// The Moon's research colonists (Wave 6): crisp, clipped names from every corner of the
    /// world below, and call signs earned on shift. Not in [`Self::ALL`] (which samples the
    /// cultures villages are built in); see [`Self::EVERY`].
    Lunar,
    /// The merrow (Wave 9, Descent IV): the fish-folk of Merrow City in its air bubble under
    /// the sea, liquid, bubbling names and tide-and-shell epithets. Not in [`Self::ALL`]; see
    /// [`Self::EVERY`].
    Merrow,
}

impl Culture {
    pub const ALL: [Self; 8] = [
        Self::Frontier,
        Self::Highland,
        Self::Mesa,
        Self::Riverside,
        Self::Scrapper,
        Self::Deepfolk,
        Self::Aeronaut,
        Self::Cloudborn,
    ];

    /// Every culture, the Moon's colonists included.
    pub const EVERY: [Self; 10] = [
        Self::Frontier,
        Self::Highland,
        Self::Mesa,
        Self::Riverside,
        Self::Scrapper,
        Self::Deepfolk,
        Self::Aeronaut,
        Self::Cloudborn,
        Self::Lunar,
        Self::Merrow,
    ];

    /// Whether this is the Moon colonists' culture (Wave 6).
    #[must_use]
    pub const fn is_lunar(self) -> bool {
        matches!(self, Self::Lunar)
    }

    /// Whether this is a sky culture (the people of sky towns, WP-SKY).
    #[must_use]
    pub const fn is_sky(self) -> bool {
        matches!(self, Self::Aeronaut | Self::Cloudborn)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Frontier => "Frontier",
            Self::Highland => "Highland",
            Self::Mesa => "Mesa",
            Self::Riverside => "Riverside",
            Self::Scrapper => "Scrapper",
            Self::Deepfolk => "Deepfolk",
            Self::Aeronaut => "Aeronaut",
            Self::Cloudborn => "Cloudborn",
            Self::Lunar => "Lunar",
            Self::Merrow => "Merrow",
        }
    }

    /// The village building style this culture favours (WP-VILLAGE dressing).
    #[must_use]
    pub const fn building_style(self) -> &'static str {
        match self {
            Self::Frontier => "timber and tin",
            Self::Highland => "dry stone",
            Self::Mesa => "adobe",
            Self::Riverside => "timber",
            Self::Scrapper => "scrap metal",
            Self::Deepfolk => "carved rock",
            Self::Aeronaut => "canvas and brass",
            Self::Cloudborn => "white stone and blue tile",
            Self::Lunar => "pressed regolith and glass",
            Self::Merrow => "coral and shell",
        }
    }

    pub(crate) const fn table(self) -> &'static NameTable {
        match self {
            Self::Frontier => &FRONTIER,
            Self::Highland => &HIGHLAND,
            Self::Mesa => &MESA,
            Self::Riverside => &RIVERSIDE,
            Self::Scrapper => &SCRAPPER,
            Self::Deepfolk => &DEEPFOLK,
            Self::Aeronaut => &AERONAUT,
            Self::Cloudborn => &CLOUDBORN,
            Self::Lunar => &LUNAR,
            Self::Merrow => &MERROW,
        }
    }
}

/// A generated person's name.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct PersonName {
    pub given: String,
    /// A surname, or an epithet starting with "of " (Deepfolk).
    pub family: String,
    /// What friends call them, if it stuck.
    #[serde(default)]
    pub nickname: Option<String>,
}

impl PersonName {
    /// "Hollis \"Dusty\" Pike", or "Ilvo of the Third Lamp".
    #[must_use]
    pub fn full(&self) -> String {
        self.nickname.as_ref().map_or_else(
            || self.formal(),
            |nickname| format!("{} \"{nickname}\" {}", self.given, self.family),
        )
    }

    /// The name they go by day to day.
    #[must_use]
    pub fn short(&self) -> &str {
        self.nickname.as_deref().unwrap_or(&self.given)
    }

    /// Given name and surname without the nickname.
    #[must_use]
    pub fn formal(&self) -> String {
        format!("{} {}", self.given, self.family)
    }
}

/// A culture's naming material.
pub(crate) struct NameTable {
    pub given: &'static [&'static str],
    /// Names in `given` that read as feminine; only she/they characters get them.
    pub she: &'static [&'static str],
    /// Names in `given` that read as masculine; only he/they characters get them.
    pub he: &'static [&'static str],
    /// Syllable generator: onsets, vowels and endings.
    pub onsets: &'static [&'static str],
    pub vowels: &'static [&'static str],
    pub endings: &'static [&'static str],
    /// Percent of given names that come from the syllable generator.
    pub generated_percent: u32,
    pub surname_heads: &'static [&'static str],
    pub surname_tails: &'static [&'static str],
    /// Whole surnames used as they are.
    pub surnames: &'static [&'static str],
    /// Percent of people with a nickname.
    pub nickname_percent: u32,
    pub nicknames: &'static [&'static str],
    /// Village name parts.
    pub place_heads: &'static [&'static str],
    pub place_tails: &'static [&'static str],
}

const FRONTIER: NameTable = NameTable {
    given: &[
        "Hollis",
        "Juniper",
        "Otis",
        "Clementine",
        "Amos",
        "Wren",
        "Silas",
        "Della",
        "Rufus",
        "Opal",
        "Jebediah",
        "Mae",
        "Cyrus",
        "Hattie",
        "Ezra",
        "Josie",
        "Lyle",
        "Pearl",
        "Abner",
        "Etta",
        "Virgil",
        "Birdie",
        "Homer",
        "Loretta",
        "Gus",
        "Dottie",
        "Augustin",
        "Nell",
    ],
    she: &[
        "Juniper",
        "Clementine",
        "Della",
        "Opal",
        "Mae",
        "Hattie",
        "Josie",
        "Pearl",
        "Etta",
        "Birdie",
        "Loretta",
        "Dottie",
        "Nell",
    ],
    he: &[
        "Hollis", "Otis", "Amos", "Silas", "Rufus", "Jebediah", "Cyrus", "Ezra", "Lyle", "Abner",
        "Virgil", "Homer", "Gus", "Augustin",
    ],
    onsets: &[
        "B", "C", "D", "G", "H", "J", "L", "M", "N", "R", "T", "W", "Cl", "Fr",
    ],
    vowels: &["a", "e", "i", "o", "u", "ay", "ee"],
    endings: &[
        "dy", "ley", "son", "ton", "lie", "ett", "ard", "ford", "ie", "a",
    ],
    generated_percent: 25,
    surname_heads: &[
        "Ash", "Black", "Cole", "Dun", "Gold", "Hart", "Iron", "Long", "Mar", "Pike", "Red",
        "Stark", "Whit", "Hollo", "Brack",
    ],
    surname_tails: &[
        "ford", "well", "wood", "ridge", "man", "ley", "ston", "field", "barrow", "kettle",
    ],
    surnames: &[
        "Pike",
        "Marrow",
        "Fane",
        "Tumbleweed",
        "Calloway",
        "Buckle",
        "Quill",
        "Hatch",
        "Lark",
        "Sutter",
        "Bramble",
    ],
    nickname_percent: 30,
    nicknames: &[
        "Dusty",
        "Lucky",
        "Two-Shovels",
        "Sunny",
        "Biscuit",
        "Boots",
        "Lantern",
        "Patch",
        "Kettle",
        "Slim",
        "Doc",
        "Copper",
    ],
    place_heads: &[
        "Dry", "Gold", "Copper", "Lucky", "Rattle", "Dust", "Tin", "Coyote", "Pickaxe", "Lonesome",
    ],
    place_tails: &[
        " Gulch",
        " Creek",
        " Flats",
        "ville",
        " Junction",
        " Hollow",
        " Crossing",
        " Bend",
    ],
};

const HIGHLAND: NameTable = NameTable {
    given: &[
        "Bram", "Isla", "Torin", "Maeve", "Euan", "Ailsa", "Corran", "Brisa", "Fergal", "Wynn",
        "Hamish", "Orla", "Duncan", "Elspet", "Callum", "Morven", "Struan", "Kenna", "Iver",
        "Senga",
    ],
    she: &[
        "Isla", "Maeve", "Ailsa", "Brisa", "Orla", "Elspet", "Morven", "Kenna", "Senga",
    ],
    he: &[
        "Bram", "Torin", "Euan", "Corran", "Fergal", "Hamish", "Duncan", "Callum", "Struan", "Iver",
    ],
    onsets: &[
        "B", "C", "D", "F", "G", "K", "M", "R", "S", "T", "Br", "Dr", "Gr",
    ],
    vowels: &["a", "ai", "e", "ea", "o", "u", "i"],
    endings: &["n", "ra", "ric", "dan", "ach", "na", "wen", "ock", "is"],
    generated_percent: 35,
    surname_heads: &[
        "Stone", "Craig", "Glen", "Hale", "Mac", "Ben", "Fell", "Moor", "Thistle", "Cairn",
        "Heather",
    ],
    surname_tails: &[
        "breck", "mor", "lach", "dale", "haven", "ward", "more", "loch", "rigg", "shaw",
    ],
    surnames: &[
        "Halloway",
        "Brackenridge",
        "Ironcliff",
        "Dunmore",
        "Kettering",
        "Ashcairn",
    ],
    nickname_percent: 15,
    nicknames: &[
        "Big",
        "Wee",
        "Old Granite",
        "Piper",
        "Ironside",
        "Bonnie",
        "the Elder",
    ],
    place_heads: &[
        "Cairn", "Glen", "Thistle", "Crag", "Fell", "Heather", "Granite", "Ben", "Pine", "Kestrel",
    ],
    place_tails: &[
        "more", "brae", " Rest", "holm", "wick", " Tor", "shiel", "dale",
    ],
};

const MESA: NameTable = NameTable {
    given: &[
        "Solene", "Amaru", "Ines", "Tavio", "Luzia", "Rafa", "Itzel", "Benito", "Marisol", "Cosme",
        "Paloma", "Teo", "Araceli", "Joaco", "Nayeli", "Dario", "Ofelia", "Ramiro",
    ],
    she: &[
        "Solene", "Ines", "Luzia", "Itzel", "Marisol", "Paloma", "Araceli", "Nayeli", "Ofelia",
    ],
    he: &[
        "Amaru", "Tavio", "Rafa", "Benito", "Cosme", "Teo", "Joaco", "Dario", "Ramiro",
    ],
    onsets: &[
        "S", "M", "L", "R", "T", "N", "C", "V", "Al", "Es", "Ar", "Ix",
    ],
    vowels: &["a", "e", "i", "o", "u", "ia", "io"],
    endings: &["na", "ro", "lo", "sa", "ra", "to", "el", "ita", "an", "ez"],
    generated_percent: 40,
    surname_heads: &[
        "Sol", "Arro", "Mesa", "Lumi", "Corra", "Vela", "Rio", "Alta", "Piedra",
    ],
    surname_tails: &["verde", "ndo", "res", "luz", "mar", "llo", "nte", "seca"],
    surnames: &[
        "Aguilar", "Montero", "Calavera", "Del Sol", "Arroyo", "Benavide",
    ],
    nickname_percent: 20,
    nicknames: &[
        "Chispa", "Lobo", "Cactus", "Sombra", "Flaco", "Canela", "Sunbird",
    ],
    place_heads: &[
        "Agua", "Sol", "Piedra", "Mesa", "Arroyo", "Cielo", "Luz", "Canyon", "Vista", "Coral",
    ],
    place_tails: &[
        " Roja", " Alta", " Seca", "rito", " Verde", " Blanca", " Dorada", " del Sur",
    ],
};

const RIVERSIDE: NameTable = NameTable {
    given: &[
        "Rowan", "Tamsin", "Fenn", "Briony", "Hob", "Linnet", "Barnaby", "Sorrel", "Ottilie",
        "Perrin", "Hazel", "Edmund", "Clover", "Jory", "Mabyn", "Tobin", "Willa", "Alder",
    ],
    she: &[
        "Tamsin", "Briony", "Linnet", "Ottilie", "Hazel", "Clover", "Willa",
    ],
    he: &["Hob", "Barnaby", "Perrin", "Edmund", "Jory", "Tobin"],
    onsets: &["B", "F", "H", "L", "M", "P", "R", "T", "W", "Th", "El"],
    vowels: &["a", "e", "i", "o", "y", "ie"],
    endings: &["wen", "lin", "ny", "ra", "son", "beth", "ric", "let", "ow"],
    generated_percent: 30,
    surname_heads: &[
        "Mill", "Fair", "Reed", "Bright", "Wil", "Brook", "Fern", "Thorn", "Oak", "Sparrow",
        "Holly",
    ],
    surname_tails: &[
        "brook", "weather", "water", "low", "bank", "hollow", "ford", "wick", "more", "field",
    ],
    surnames: &[
        "Fairweather",
        "Goodbody",
        "Pennywhistle",
        "Ottery",
        "Wimble",
        "Dabney",
    ],
    nickname_percent: 15,
    nicknames: &["Duck", "Bramble", "Pudding", "Mossy", "Button", "Old Oak"],
    place_heads: &[
        "Willow", "Reed", "Otter", "Mill", "Alder", "Heron", "Fern", "Bramble",
    ],
    place_tails: &[
        "mere", " Ford", "brook", "ton", " Green", "stead", " Landing", "bury",
    ],
};

const SCRAPPER: NameTable = NameTable {
    given: &[
        "Dex", "Kit", "Moss", "Tam", "Jinx", "Rook", "Vee", "Cass", "Zed", "Nim", "Bo", "Lux",
        "Pax", "Remy", "Juno", "Fitz",
    ],
    she: &["Juno"],
    he: &["Dex", "Fitz"],
    onsets: &["D", "J", "K", "R", "V", "Z", "Br", "Sk", "Tr", "Qu"],
    vowels: &["a", "e", "i", "o", "u"],
    endings: &["x", "z", "ck", "nn", "ss", "tt", "do", "ka"],
    generated_percent: 35,
    surname_heads: &[
        "Bolt", "Gear", "Wrench", "Tin", "Chrome", "Spark", "Rust", "Clank",
    ],
    surname_tails: &["er", "worth", "smith", "well", "ley", "son", "box", "head"],
    surnames: &[
        "Sprocket",
        "Ironside",
        "Fusewire",
        "Twelvebolt",
        "Junkett",
        "Hasp",
    ],
    nickname_percent: 55,
    nicknames: &[
        "Rivet", "Sprocket", "Fuse", "Grease", "Spanner", "Zap", "Cog", "Torque", "Magnet",
        "Scrap", "Gasket", "Static",
    ],
    place_heads: &[
        "Rust", "Bolt", "Chrome", "Scrap", "Gear", "Tin", "Clank", "Wrench",
    ],
    place_tails: &[
        " Yard", "town", " Heap", " Works", " Camp", " Pile", "ton", " Depot",
    ],
};

const DEEPFOLK: NameTable = NameTable {
    given: &[
        "Ilvo", "Thessaly", "Oru", "Ysmay", "Lio", "Vaela", "Ossery", "Nuin", "Aule", "Mirelle",
        "Thren", "Eluan", "Saphi", "Ondo",
    ],
    she: &["Thessaly", "Ysmay", "Mirelle", "Vaela"],
    he: &[],
    onsets: &["Th", "L", "V", "N", "S", "Y", "O", "E", "A", "Ul", "Il"],
    vowels: &["a", "e", "i", "o", "u", "ae", "ui"],
    endings: &["n", "l", "ra", "ve", "th", "sa", "lo", "rin"],
    generated_percent: 55,
    surname_heads: &[],
    surname_tails: &[],
    surnames: &[
        "of the Third Lamp",
        "of the Low Hall",
        "of the Quiet Seam",
        "of the Glass Steps",
        "of the Long Dark",
        "of the Warm Vent",
        "of the Hollow Bell",
        "of the Ninth Stair",
        "of the Still Water",
        "of the Blue Vein",
        "of the Old Lantern",
        "of the Humming Wall",
    ],
    nickname_percent: 10,
    nicknames: &["Lamplighter", "Echo", "Glowworm", "Stillwater"],
    place_heads: &[
        "Lantern", "Hollow", "Glass", "Echo", "Glow", "Deep", "Still", "Vein",
    ],
    place_tails: &[
        " Hall", " Hearth", "deep", " Grotto", " Lamp", " Stair", "well", " Reach",
    ],
};

/// The merrow's names (Wave 9): liquid, bubbling, sung more than said.
const MERROW: NameTable = NameTable {
    given: &[
        "Ulla", "Mirro", "Pelagi", "Sooli", "Wavren", "Nerilla", "Coralyn", "Bubbo", "Thalassa",
        "Oolo", "Merrin", "Spindle", "Lumi", "Kelpie",
    ],
    she: &["Ulla", "Pelagi", "Nerilla", "Coralyn", "Thalassa"],
    he: &["Mirro", "Wavren", "Bubbo"],
    onsets: &["M", "P", "S", "W", "N", "L", "Th", "Bl", "Oo", "U"],
    vowels: &["a", "e", "i", "oo", "ui", "aa"],
    endings: &["la", "rro", "li", "ssa", "mi", "plo", "rin"],
    generated_percent: 45,
    surname_heads: &[],
    surname_tails: &[],
    surnames: &[
        "of the Slack Tide",
        "of the Pearl Bed",
        "of the Warm Current",
        "of the Glass Dome",
        "of the Kelp Hall",
        "of the Ninth Bubble",
        "of the Long Swim",
        "of the Singing Shell",
        "of the Low Reef",
        "of the Lantern Fish",
    ],
    nickname_percent: 25,
    nicknames: &[
        "Bubbles",
        "Finny",
        "Gills",
        "Driftwood",
        "Snapper",
        "Minnow",
        "Shellback",
    ],
    place_heads: &[
        "Pearl", "Coral", "Kelp", "Tide", "Shell", "Bubble", "Reef", "Brine",
    ],
    place_tails: &[
        " Dome", " Reach", " Hollow", "mere", " Shoal", " Spire", "fall", " Sound",
    ],
};

const AERONAUT: NameTable = NameTable {
    given: &[
        "Amelia", "Barnaby", "Wren", "Cyrus", "Ottoline", "Jasper", "Marlowe", "Hesper",
        "Ignatius", "Rosalind", "Fletcher", "Isadora", "Lark", "Bastian",
    ],
    she: &["Amelia", "Ottoline", "Rosalind", "Isadora", "Hesper"],
    he: &["Barnaby", "Cyrus", "Jasper", "Ignatius", "Bastian"],
    onsets: &["Al", "B", "C", "F", "H", "J", "M", "R", "W", "Z"],
    vowels: &["a", "e", "i", "o", "ay"],
    endings: &["ton", "by", "ra", "lin", "sey", "mer"],
    generated_percent: 25,
    surname_heads: &[
        "Gale", "Brass", "Cloud", "High", "Wind", "Spar", "Gas", "Sail",
    ],
    surname_tails: &[
        "wright", "worth", "ley", "more", "field", "hook", "ford", "line",
    ],
    surnames: &[
        "Montgolfier-Smythe",
        "Ballastier",
        "Updraft",
        "Fairweather",
        "Ropewalker",
        "Quickfall",
    ],
    nickname_percent: 45,
    nicknames: &[
        "Gusty", "Ballast", "Skipper", "Sparrow", "Ace", "Lofty", "Ripcord", "Breezy", "Captain",
        "Goggles", "Flapjack", "Kite",
    ],
    place_heads: &[
        "Gale", "Brass", "Gull", "Sky", "Zephyr", "Updraft", "Cirrus", "Halyard",
    ],
    place_tails: &[
        " Moorings",
        " Port",
        " Roost",
        " Wharf",
        " Landing",
        "haven",
        " Mast",
        " Dock",
    ],
};

const CLOUDBORN: NameTable = NameTable {
    given: &[
        "Aerin", "Soli", "Neve", "Ilia", "Caelo", "Mirai", "Sefa", "Oriel", "Tavi", "Luma",
        "Aeris", "Nimbo", "Eira", "Vashti",
    ],
    she: &["Neve", "Ilia", "Mirai", "Eira", "Vashti"],
    he: &["Caelo", "Nimbo"],
    onsets: &["Ae", "C", "L", "M", "N", "S", "T", "V", "Ei", "Or"],
    vowels: &["a", "e", "i", "o", "ae", "ia"],
    endings: &["l", "n", "ra", "ri", "sa", "lo", "ne"],
    generated_percent: 55,
    surname_heads: &[],
    surname_tails: &[],
    surnames: &[
        "of the East Wind",
        "of the Silver Fog",
        "of the Morning Bell",
        "of the High Cloister",
        "of the Long Rain",
        "of the Still Air",
        "of the White Terrace",
        "of the Seventh Gust",
        "of the Pale Dawn",
        "of the Drifting Field",
        "of the Hollow Gale",
        "of the Bright Squall",
        "of the Lantern Cloud",
        "of the Slow Thaw",
    ],
    nickname_percent: 15,
    nicknames: &["Feather", "Mist", "Bellringer", "Dewdrop", "Cloudcutter"],
    place_heads: &[
        "Cirrus", "Nimbus", "Halcyon", "Vapor", "Silver", "Bell", "Dew", "Lumen",
    ],
    place_tails: &[
        " Terrace",
        " Cloister",
        "fold",
        " Rest",
        " Garden",
        " Steps",
        "mere",
        " Fields",
    ],
};

const LUNAR: NameTable = NameTable {
    given: &[
        "Anouk", "Tycho", "Selene", "Idris", "Kepler", "Noor", "Halvard", "Juno", "Ravi", "Ingrid",
        "Oskar", "Mireille", "Tomasz", "Yusra", "Callisto", "Bram",
    ],
    she: &[
        "Anouk", "Selene", "Noor", "Juno", "Ingrid", "Mireille", "Yusra", "Callisto",
    ],
    he: &[
        "Tycho", "Idris", "Kepler", "Halvard", "Ravi", "Oskar", "Tomasz", "Bram",
    ],
    onsets: &["K", "T", "V", "Z", "L", "N", "Ar", "Is"],
    vowels: &["a", "e", "i", "o", "u"],
    endings: &["k", "n", "x", "ra", "ko", "sen"],
    generated_percent: 20,
    surname_heads: &["Crater", "Mare", "Orbit", "Apo", "Peri", "Tran"],
    surname_tails: &["vik", "sen", "berg", "ova", "ski", "lund"],
    surnames: &[
        "Okonkwo-Hale",
        "Vasquez",
        "Lindqvist",
        "Achterberg",
        "Moreau",
        "Tanaka-Byrne",
        "Abernathy",
        "Kowalczyk",
    ],
    nickname_percent: 50,
    nicknames: &[
        "Dusty", "Doc", "Airlock", "Lowgee", "Tether", "Crater", "Ping", "Halo", "Sixty",
        "Regolith", "Quiet", "Sparks",
    ],
    place_heads: &[
        "Tranquility",
        "Serenity",
        "Copernicus",
        "Tycho",
        "Endurance",
        "Halcyon",
    ],
    place_tails: &[" Station", " Colony", " Base", " Outpost"],
};

/// A given name from `culture` that suits `pronouns` (`None` or `They`: any name).
///
/// Curated names listed in a culture's `she` / `he` tables only go to matching (or they/them)
/// people, and generated names with a strongly gendered ending are re-rolled, so a village
/// never introduces "Edmund (she/her)" next to "Ottilie (he/him)".
pub(crate) fn given_name(
    culture: Culture,
    pronouns: Option<Pronouns>,
    rng: &mut PeopleRng,
) -> String {
    let table = culture.table();
    if rng.chance(table.generated_percent) {
        for _ in 0..8 {
            let name = syllable_name(table, rng);
            if pronounceable(&name) && generated_suits(&name, pronouns) {
                return name;
            }
        }
    }
    let options: Vec<&str> = table
        .given
        .iter()
        .copied()
        .filter(|name| name_suits(table, name, pronouns))
        .collect();
    if options.is_empty() {
        (*rng.pick(table.given)).to_owned()
    } else {
        (*rng.pick(&options)).to_owned()
    }
}

/// Whether curated `name` may go to someone with `pronouns`.
fn name_suits(table: &NameTable, name: &str, pronouns: Option<Pronouns>) -> bool {
    match pronouns {
        Some(Pronouns::She) => !table.he.contains(&name),
        Some(Pronouns::He) => !table.she.contains(&name),
        Some(Pronouns::They) | None => true,
    }
}

/// Whether a generated name's ending suits `pronouns` (endings like "-beth" or "-ita" read as
/// feminine, "-son" or "-ric" as masculine).
fn generated_suits(name: &str, pronouns: Option<Pronouns>) -> bool {
    const FEMININE: &[&str] = &["a", "ie", "beth", "ee", "lie", "wen"];
    const MASCULINE: &[&str] = &["o", "son", "ric", "ard", "ton", "ford", "ock", "dan"];
    let lower = name.to_ascii_lowercase();
    match pronouns {
        Some(Pronouns::He) => !FEMININE.iter().any(|ending| lower.ends_with(ending)),
        Some(Pronouns::She) => !MASCULINE.iter().any(|ending| lower.ends_with(ending)),
        Some(Pronouns::They) | None => true,
    }
}

fn syllable_name(table: &NameTable, rng: &mut PeopleRng) -> String {
    let mut name = String::new();
    name.push_str(rng.pick(table.onsets));
    name.push_str(rng.pick(table.vowels));
    if rng.chance(30) {
        let middle = rng.pick(table.onsets).to_lowercase();
        name.push_str(&middle);
        name.push_str(rng.pick(table.vowels));
    }
    name.push_str(rng.pick(table.endings));
    name
}

/// Rejects names that are too short, too long, or have three of a letter in a row or four
/// vowels running.
fn pronounceable(name: &str) -> bool {
    let chars: Vec<char> = name.to_lowercase().chars().collect();
    if !(4..=8).contains(&chars.len()) {
        return false;
    }
    let lower = name.to_lowercase();
    if lower.starts_with("ee")
        || ["aa", "ii", "uu", "yi", "iy"]
            .iter()
            .any(|pair| lower.contains(pair))
    {
        return false;
    }
    let is_vowel = |c: char| "aeiouy".contains(c);
    let mut vowel_run = 0;
    let mut consonant_run = 0;
    for (index, c) in chars.iter().enumerate() {
        if index >= 2 && chars[index - 1] == *c && chars[index - 2] == *c {
            return false;
        }
        if is_vowel(*c) {
            vowel_run += 1;
            consonant_run = 0;
        } else {
            consonant_run += 1;
            vowel_run = 0;
        }
        if vowel_run >= 3 || consonant_run >= 4 {
            return false;
        }
    }
    true
}

/// A surname from `culture`.
pub(crate) fn surname(culture: Culture, rng: &mut PeopleRng) -> String {
    let table = culture.table();
    if table.surname_heads.is_empty() || rng.chance(30) {
        return (*rng.pick(table.surnames)).to_owned();
    }
    let head = rng.pick(table.surname_heads);
    let tail = rng.pick(table.surname_tails);
    format!("{head}{tail}")
}

/// A culture nickname, if this person has one.
pub(crate) fn nickname(culture: Culture, rng: &mut PeopleRng) -> Option<String> {
    let table = culture.table();
    rng.chance(table.nickname_percent)
        .then(|| (*rng.pick(table.nicknames)).to_owned())
}

/// A deterministic, culture-flavoured village name for `village` under `seed` (WP-VILLAGE).
#[must_use]
pub fn village_name(seed: u64, village: u32, culture: Culture) -> String {
    let mut rng = PeopleRng::new(seed, salt::VILLAGE_NAME, u64::from(village), 0);
    let table = culture.table();
    let head = rng.pick(table.place_heads);
    let tail = rng.pick(table.place_tails);
    format!("{head}{tail}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Culture, PeopleRng, Pronouns, given_name, pronounceable, surname, village_name};

    #[test]
    fn every_culture_names_plenty_of_distinct_people() {
        for culture in Culture::ALL {
            let mut rng = PeopleRng::new(11, 0, culture as u64, 0);
            let given: BTreeSet<String> = (0..300)
                .map(|_| given_name(culture, None, &mut rng))
                .collect();
            let families: BTreeSet<String> = (0..300).map(|_| surname(culture, &mut rng)).collect();
            assert!(
                given.len() >= 40,
                "{culture:?} only made {} names",
                given.len()
            );
            assert!(
                families.len() >= 12,
                "{culture:?}: {} surnames",
                families.len()
            );
            for name in &given {
                assert!(
                    name.chars().next().is_some_and(char::is_uppercase),
                    "{name}"
                );
            }
        }
    }

    #[test]
    fn names_suit_pronouns() {
        for culture in Culture::ALL {
            let table = culture.table();
            for name in table.she.iter().chain(table.he) {
                assert!(
                    table.given.contains(name),
                    "{culture:?}: {name} not in given"
                );
            }
            assert!(table.she.iter().all(|name| !table.he.contains(name)));
            let mut rng = PeopleRng::new(3, 0, culture as u64, 0);
            for _ in 0..300 {
                let she = given_name(culture, Some(Pronouns::She), &mut rng);
                let he = given_name(culture, Some(Pronouns::He), &mut rng);
                assert!(!table.he.contains(&she.as_str()), "{she} for she/her");
                assert!(!table.she.contains(&he.as_str()), "{he} for he/him");
                let curated = table.given.contains(&he.as_str());
                assert!(curated || !he.ends_with("beth"), "{he} for he/him");
            }
        }
    }

    #[test]
    fn pronounceable_rejects_noise() {
        assert!(pronounceable("Hollis"));
        assert!(!pronounceable("Tasiita"));
        assert!(!pronounceable("Een"));
        assert!(!pronounceable("Aaaron"));
        assert!(!pronounceable("Brrkst"));
        assert!(!pronounceable("Al"));
    }

    #[test]
    fn village_names_are_stable_and_varied() {
        assert_eq!(
            village_name(5, 3, Culture::Mesa),
            village_name(5, 3, Culture::Mesa)
        );
        let names: BTreeSet<String> = (0..40)
            .map(|village| village_name(5, village, Culture::Frontier))
            .collect();
        assert!(names.len() >= 25);
    }
}
