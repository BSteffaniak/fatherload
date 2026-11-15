//! Personality: Big-Five-style traits, the dialogue [`Tone`] buckets they map to, quirks,
//! values, wants and fears.
//!
//! The phrase tables here are template fragments (they may use dialogue variables such as
//! `{absent}`), stitched into tone-specific lines by the dialogue system.

use serde::{Deserialize, Serialize};

use super::PeopleRng;

/// Big-Five-style personality traits, each `0..=100` (50 is average).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Personality {
    /// Curious, imaginative, dreamy (high) versus practical, conventional (low).
    pub openness: u8,
    /// Careful, organised, dutiful (high) versus loose, spontaneous (low).
    pub conscientiousness: u8,
    /// Talkative, sociable, loud (high) versus quiet, reserved (low).
    pub extraversion: u8,
    /// Kind, trusting, helpful (high) versus prickly, suspicious (low).
    pub agreeableness: u8,
    /// Anxious, moody, jumpy (high) versus calm, steady (low).
    pub neuroticism: u8,
}

/// Per-trait nudges a role or culture applies to rolled traits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraitBias {
    pub openness: i8,
    pub conscientiousness: i8,
    pub extraversion: i8,
    pub agreeableness: i8,
    pub neuroticism: i8,
}

impl TraitBias {
    pub const NONE: Self = Self {
        openness: 0,
        conscientiousness: 0,
        extraversion: 0,
        agreeableness: 0,
        neuroticism: 0,
    };

    #[must_use]
    pub const fn new(o: i8, c: i8, e: i8, a: i8, n: i8) -> Self {
        Self {
            openness: o,
            conscientiousness: c,
            extraversion: e,
            agreeableness: a,
            neuroticism: n,
        }
    }
}

impl Personality {
    pub const AVERAGE: Self = Self {
        openness: 50,
        conscientiousness: 50,
        extraversion: 50,
        agreeableness: 50,
        neuroticism: 50,
    };

    /// Rolls traits (a bell-ish average of two rolls) nudged by `bias`.
    pub(crate) fn roll(rng: &mut PeopleRng, bias: TraitBias) -> Self {
        let mut roll = |nudge: i8| {
            let value = (rng.range(0, 100) + rng.range(0, 100)) / 2 + i32::from(nudge);
            u8::try_from(value.clamp(0, 100)).unwrap_or(50)
        };
        Self {
            openness: roll(bias.openness),
            conscientiousness: roll(bias.conscientiousness),
            extraversion: roll(bias.extraversion),
            agreeableness: roll(bias.agreeableness),
            neuroticism: roll(bias.neuroticism),
        }
    }

    /// How strongly each tone fits these traits (higher fits better).
    #[must_use]
    pub fn tone_scores(self) -> [(Tone, i32); 6] {
        let o = i32::from(self.openness);
        let c = i32::from(self.conscientiousness);
        let e = i32::from(self.extraversion);
        let a = i32::from(self.agreeableness);
        let n = i32::from(self.neuroticism);
        [
            (Tone::Gruff, 2 * (50 - a) + (50 - e) / 2 + (c - 50) / 4),
            (Tone::Cheerful, 2 * (e - 50) + (a - 50) / 2 + (50 - n) / 4),
            (Tone::Nervous, 2 * (n - 50) + (50 - e) / 3),
            (Tone::Poetic, 2 * (o - 50) + (50 - c) / 3 + (50 - e) / 4),
            (Tone::Dry, (c - 50) + (50 - e) + (o - 50) / 2 + (50 - n) / 3),
            (Tone::Earnest, 2 * (a - 50) + (c - 50) / 2),
        ]
    }

    /// The tone bucket a character mostly speaks in, then the one that colours the rest.
    #[must_use]
    pub fn tones(self) -> (Tone, Tone) {
        let mut scores = self.tone_scores();
        // Stable: ties keep the table order.
        scores.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
        (scores[0].0, scores[1].0)
    }

    /// Short words for the report ("curious, sociable, prickly").
    #[must_use]
    pub fn adjectives(self) -> Vec<&'static str> {
        let mut words = Vec::new();
        let pick = |value: u8, high: &'static str, low: &'static str| match value {
            70..=u8::MAX => Some(high),
            0..=30 => Some(low),
            _ => None,
        };
        words.extend(pick(self.openness, "curious", "down-to-earth"));
        words.extend(pick(self.conscientiousness, "meticulous", "scatterbrained"));
        words.extend(pick(self.extraversion, "chatty", "quiet"));
        words.extend(pick(self.agreeableness, "kind-hearted", "prickly"));
        words.extend(pick(self.neuroticism, "jumpy", "unflappable"));
        if words.is_empty() {
            words.push("even-keeled");
        }
        words
    }
}

/// The voice a line is written in. Every dialogue line set has variants for every tone.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Tone {
    /// Short, blunt, secretly soft.
    Gruff,
    /// Bright, loud, exclamation-prone.
    Cheerful,
    /// Jumpy, apologetic, imagines the worst.
    Nervous,
    /// Dreamy, figurative, a little grand.
    Poetic,
    /// Deadpan and wry.
    Dry,
    /// Warm, sincere, means every word.
    Earnest,
}

impl Tone {
    pub const ALL: [Self; 6] = [
        Self::Gruff,
        Self::Cheerful,
        Self::Nervous,
        Self::Poetic,
        Self::Dry,
        Self::Earnest,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gruff => "gruff",
            Self::Cheerful => "cheerful",
            Self::Nervous => "nervous",
            Self::Poetic => "poetic",
            Self::Dry => "dry",
            Self::Earnest => "earnest",
        }
    }
}

/// A small personal oddity that colours dialogue with asides.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Quirk {
    HumsOffKey,
    NamesTheirTools,
    CollectsPebbles,
    Superstitious,
    AlwaysHungry,
    TerribleJokes,
    CountsThings,
    TalksToMachines,
    MadeUpProverbs,
    Whittles,
    KnowsEveryonesBusiness,
    WeatherWatcher,
    ForgetsNames,
    TallTales,
    SawTheDeepLights,
    MiningMetaphors,
    BakesForEveryone,
    MistrustsBirds,
    WritesLetters,
    OverlyFormal,
}

impl Quirk {
    pub const ALL: [Self; 20] = [
        Self::HumsOffKey,
        Self::NamesTheirTools,
        Self::CollectsPebbles,
        Self::Superstitious,
        Self::AlwaysHungry,
        Self::TerribleJokes,
        Self::CountsThings,
        Self::TalksToMachines,
        Self::MadeUpProverbs,
        Self::Whittles,
        Self::KnowsEveryonesBusiness,
        Self::WeatherWatcher,
        Self::ForgetsNames,
        Self::TallTales,
        Self::SawTheDeepLights,
        Self::MiningMetaphors,
        Self::BakesForEveryone,
        Self::MistrustsBirds,
        Self::WritesLetters,
        Self::OverlyFormal,
    ];

    /// A third-person description for the report and character sheets.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::HumsOffKey => "hums constantly, never in the right key",
            Self::NamesTheirTools => "has a name for every tool they own",
            Self::CollectsPebbles => "collects interesting pebbles",
            Self::Superstitious => "is deeply superstitious about mining",
            Self::AlwaysHungry => "is always, always hungry",
            Self::TerribleJokes => "tells terrible jokes with total confidence",
            Self::CountsThings => "counts things under their breath",
            Self::TalksToMachines => "talks to machines like old friends",
            Self::MadeUpProverbs => "quotes proverbs nobody else has heard of",
            Self::Whittles => "whittles little figures while they talk",
            Self::KnowsEveryonesBusiness => "knows everyone's business",
            Self::WeatherWatcher => "predicts the weather, mostly wrongly",
            Self::ForgetsNames => "never quite remembers names",
            Self::TallTales => "tells tall tales about their youth",
            Self::SawTheDeepLights => "swears they once saw lights deep in the rock",
            Self::MiningMetaphors => "explains everything in mining metaphors",
            Self::BakesForEveryone => "bakes for the whole village",
            Self::MistrustsBirds => "does not trust birds",
            Self::WritesLetters => "writes long letters they never send",
            Self::OverlyFormal => "is formal to the point of ceremony",
        }
    }

    /// Asides this quirk slips into conversation (templates; `{player}` etc. allowed).
    #[must_use]
    pub const fn asides(self) -> &'static [&'static str] {
        match self {
            Self::HumsOffKey => &[
                "*hums something that might once have been a song*",
                "Sorry, was I humming? I'm always humming.",
                "*hums, loses the tune, finds a different one*",
            ],
            Self::NamesTheirTools => &[
                "Don't mind Beatrice, she's my pickaxe. She's shy.",
                "This wrench is called Gerald. Gerald has seen things.",
                "Say hello to Old Faithful. She's a shovel, but she's also family.",
            ],
            Self::CollectsPebbles => &[
                "Oh! If you find a pebble with a stripe on it, it's mine. Spiritually.",
                "*shows you a pebble* Look at it. Just look at it.",
                "I've got three hundred and eight pebbles. Three hundred and nine if you count Steve.",
            ],
            Self::Superstitious => &[
                "Never whistle in a tunnel. The rock whistles back.",
                "Tap your drill twice before you go down. Just do it. Humour me.",
                "Left boot first, always. Right boot first and the seam goes dry.",
            ],
            Self::AlwaysHungry => &[
                "You haven't got a sandwich on you? A crumb? No?",
                "Is it lunch? It feels like lunch. It always feels like lunch.",
                "I'd trade a week's pay for a hot pie right now.",
            ],
            Self::TerribleJokes => &[
                "Why did the miner cross the road? To get to the other seam! ...Seam. Side. You get it.",
                "What do you call a lazy drill? A bore! Ha! ...Nobody ever laughs.",
                "I told a joke to a rock once. Tough crowd. Stony silence.",
            ],
            Self::CountsThings => &[
                "...forty-one, forty-two. Sorry. Fence posts.",
                "You've got three scratches on your hull. Four. Four scratches.",
                "Seven clouds today. Yesterday it was nine. Make of that what you will.",
            ],
            Self::TalksToMachines => &[
                "*pats your rig* Who's a good little drill? You are.",
                "Your engine sounds sad. Have you been talking to it?",
                "Machines like to be thanked. Ask anyone. Well, ask a machine.",
            ],
            Self::MadeUpProverbs => &[
                "As my gran used to say: a full cargo hold weighs nothing on the way up.",
                "Like they say, the deeper the hole, the shorter the shadow.",
                "Never trust a rock that's smiling. That's an old saying. I'm old, so.",
            ],
            Self::Whittles => &[
                "*holds up a little wooden rig* Look, it's you. Roughly.",
                "*whittles* Hold still a second, I'm doing your nose.",
                "I'm carving the whole village out of pine. I'm up to the outhouse.",
            ],
            Self::KnowsEveryonesBusiness => &[
                "Between you and me, someone in this village has been buying a lot of cheese.",
                "I hear things. I hear all the things.",
                "Ask me who owes who and I'll tell you. Ask me nicely and I'll tell you twice.",
            ],
            Self::WeatherWatcher => &[
                "Rain by sundown. My knee says so. My knee is usually wrong, but it's very sure.",
                "Smell that? That's a dry wind. Or a goat. Hard to tell.",
                "Clouds look like a drill bit today. That's either very good or very bad.",
            ],
            Self::ForgetsNames => &[
                "Good to see you, uh... Driller. Rig person. You.",
                "Remind me your name? I'll forget it again, but I like hearing it.",
                "Oh, it's you! The one with the... rig. Of course it is.",
            ],
            Self::TallTales => &[
                "Did I ever tell you about the time I drilled through a whole mountain with a spoon?",
                "When I was your age I mined a diamond the size of a cow. Small cow. Still.",
                "I once arm-wrestled a boulder. It was a draw.",
            ],
            Self::SawTheDeepLights => &[
                "There are lights down there. Way down. I've seen them. Blue ones.",
                "Folk laugh, but I know what I saw under the basalt. It blinked at me.",
                "If you see the lights, don't follow them. Or do. Then come tell me.",
            ],
            Self::MiningMetaphors => &[
                "Life's a seam, friend. Some days ore, some days slag.",
                "Love's like a vein of gold: you dig and dig and then it's a rock.",
                "Never drill your bridges. That's not quite the saying, but it's better.",
            ],
            Self::BakesForEveryone => &[
                "Take a biscuit. Take two. I baked forty.",
                "*hands you something warm wrapped in cloth* For the road. For the hole.",
                "I made bread shaped like a drill. Nobody understood it.",
            ],
            Self::MistrustsBirds => &[
                "Keep an eye on that crow. It's been watching you.",
                "Birds. Always up to something. Look at them, all smug.",
                "You know why birds fly? Because they're guilty.",
            ],
            Self::WritesLetters => &[
                "I'm halfway through a letter. Twelve pages. I'll never send it.",
                "Do you know how to spell 'regretfully'? It's for a letter.",
                "I write to the deep, sometimes. Just in case someone down there reads.",
            ],
            Self::OverlyFormal => &[
                "Do forgive the interruption, esteemed driller.",
                "I shall endeavour to be brief. I shall fail, but I shall endeavour.",
                "Pray, do mind the step. It is a step of considerable height.",
            ],
        }
    }
}

/// What a character cares about most.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Value {
    Family,
    Honesty,
    Fortune,
    Freedom,
    Knowledge,
    Tradition,
    Craft,
    Adventure,
    Community,
    Wonder,
}

impl Value {
    pub const ALL: [Self; 10] = [
        Self::Family,
        Self::Honesty,
        Self::Fortune,
        Self::Freedom,
        Self::Knowledge,
        Self::Tradition,
        Self::Craft,
        Self::Adventure,
        Self::Community,
        Self::Wonder,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Family => "family",
            Self::Honesty => "honesty",
            Self::Fortune => "fortune",
            Self::Freedom => "freedom",
            Self::Knowledge => "knowledge",
            Self::Tradition => "tradition",
            Self::Craft => "craft",
            Self::Adventure => "adventure",
            Self::Community => "community",
            Self::Wonder => "wonder",
        }
    }

    /// Noun phrases that finish "what matters is ...".
    #[must_use]
    pub const fn phrases(self) -> &'static [&'static str] {
        match self {
            Self::Family => &["the people you come home to", "family, blood or otherwise"],
            Self::Honesty => &[
                "a fair weight and a straight answer",
                "honest work, honestly paid",
            ],
            Self::Fortune => &[
                "a full purse at the end of the week",
                "striking it rich, just once",
            ],
            Self::Freedom => &[
                "nobody telling you where to dig",
                "open sky and an open road",
            ],
            Self::Knowledge => &[
                "knowing how things work",
                "finding out what's really down there",
            ],
            Self::Tradition => &["doing it the way it's always been done", "the old ways"],
            Self::Craft => &["a job done properly", "good tools and a steady hand"],
            Self::Adventure => &["the next horizon", "the bit of the map nobody's drawn yet"],
            Self::Community => &[
                "looking out for your neighbours",
                "a village that sticks together",
            ],
            Self::Wonder => &[
                "the strange and beautiful things in the dark",
                "a little mystery",
            ],
        }
    }
}

/// What a character wants. Wants drive story seeds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Want {
    /// Find out what happened to someone who went down and never came back.
    FindRelative,
    /// Strike it rich.
    StrikeItRich,
    /// Show everyone they're more than they seem.
    ProveThemselves,
    /// Finish an invention.
    BuildInvention,
    /// Learn what's really down there.
    UncoverDeepSecret,
    /// Outdo a rival.
    BeatRival,
    /// Recover a family heirloom lost below.
    RecoverHeirloom,
    /// Pay off a debt.
    PayOffDebt,
    /// Keep the village safe and thriving.
    ProtectVillage,
    /// See the far edges of the map.
    SeeTheWorld,
    /// Make peace with someone they wronged.
    MakeAmends,
}

impl Want {
    pub const ALL: [Self; 11] = [
        Self::FindRelative,
        Self::StrikeItRich,
        Self::ProveThemselves,
        Self::BuildInvention,
        Self::UncoverDeepSecret,
        Self::BeatRival,
        Self::RecoverHeirloom,
        Self::PayOffDebt,
        Self::ProtectVillage,
        Self::SeeTheWorld,
        Self::MakeAmends,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FindRelative => "find a lost relative",
            Self::StrikeItRich => "strike it rich",
            Self::ProveThemselves => "prove themselves",
            Self::BuildInvention => "finish an invention",
            Self::UncoverDeepSecret => "uncover the deep secret",
            Self::BeatRival => "beat a rival",
            Self::RecoverHeirloom => "recover an heirloom",
            Self::PayOffDebt => "pay off a debt",
            Self::ProtectVillage => "protect the village",
            Self::SeeTheWorld => "see the world",
            Self::MakeAmends => "make amends",
        }
    }

    /// Verb phrases that finish "I want to ..." (may use `{focus}` and `{focus_relation}`:
    /// the person the want is about, see [`super::WantFocus`]).
    #[must_use]
    pub const fn phrases(self) -> &'static [&'static str] {
        match self {
            Self::FindRelative => &[
                "find out what happened to my {focus_relation}, {focus}",
                "bring my {focus_relation} {focus} home, or at least their story",
            ],
            Self::StrikeItRich => &[
                "hit one vein so fat I never have to dig again",
                "walk into the bank and watch the clerk faint",
            ],
            Self::ProveThemselves => &[
                "show this whole valley I'm more than they think",
                "do one thing big enough that they put my name on a sign",
            ],
            Self::BuildInvention => &[
                "finish my machine. It's nearly done. It's been nearly done for six years",
                "get my prototype working before somebody else thinks of it",
            ],
            Self::UncoverDeepSecret => &[
                "know what's humming under the basalt",
                "find out who built the old things down deep, and why they stopped",
            ],
            Self::BeatRival => &[
                "beat {focus} at their own game, just once",
                "see the look on {focus}'s face when I come out on top",
            ],
            Self::RecoverHeirloom => &[
                "get back what my family lost down a shaft three generations ago",
                "recover the family keepsake the old mine swallowed",
            ],
            Self::PayOffDebt => &[
                "pay back every coin I owe {focus}",
                "clear my debt to {focus} and sleep through the night again",
            ],
            Self::ProtectVillage => &[
                "keep this village standing, whatever comes up out of the ground",
                "see this place thrive, so nobody has to leave",
            ],
            Self::SeeTheWorld => &[
                "see what's past the last hill on the map",
                "go further than anyone from here has ever gone",
            ],
            Self::MakeAmends => &[
                "make things right with {focus}",
                "tell {focus} I was wrong, and mean it",
            ],
        }
    }
}

/// What a character fears.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Fear {
    TheDark,
    CaveIns,
    DeepThings,
    Heights,
    OldMachines,
    Fire,
    BeingForgotten,
    GoingBroke,
    Gas,
    LosingFamily,
}

impl Fear {
    pub const ALL: [Self; 10] = [
        Self::TheDark,
        Self::CaveIns,
        Self::DeepThings,
        Self::Heights,
        Self::OldMachines,
        Self::Fire,
        Self::BeingForgotten,
        Self::GoingBroke,
        Self::Gas,
        Self::LosingFamily,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TheDark => "the dark",
            Self::CaveIns => "cave-ins",
            Self::DeepThings => "what lives deep down",
            Self::Heights => "heights",
            Self::OldMachines => "the old machines",
            Self::Fire => "fire",
            Self::BeingForgotten => "being forgotten",
            Self::GoingBroke => "going broke",
            Self::Gas => "gas pockets",
            Self::LosingFamily => "losing family",
        }
    }

    /// Noun phrases for "what scares me is ...".
    #[must_use]
    pub const fn phrases(self) -> &'static [&'static str] {
        match self {
            Self::TheDark => &[
                "the dark past where the lamps reach",
                "that moment the headlamp flickers",
            ],
            Self::CaveIns => &[
                "the sound a ceiling makes right before it isn't one",
                "a tunnel that closes up behind you",
            ],
            Self::DeepThings => &[
                "whatever hums under the basalt",
                "the deep places that seem to listen",
            ],
            Self::Heights => &[
                "being up high with nothing underneath",
                "looking down from the top of a shaft",
            ],
            Self::OldMachines => &[
                "the old machines buried down deep, still ticking",
                "metal that was here before any of us",
            ],
            Self::Fire => &[
                "fire, any kind of fire",
                "the glow of lava through thin rock",
            ],
            Self::BeingForgotten => &[
                "nobody remembering my name after I'm gone",
                "being a name nobody can place",
            ],
            Self::GoingBroke => &[
                "an empty purse in a long winter",
                "losing everything on one bad dig",
            ],
            Self::Gas => &[
                "that sweet smell right before the gas finds a spark",
                "pockets of bad air",
            ],
            Self::LosingFamily => &[
                "someone I love going down and not coming up",
                "an empty chair at supper",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{PeopleRng, Personality, Tone, TraitBias};

    #[test]
    fn every_tone_is_common_enough() {
        let mut rng = PeopleRng::new(3, 0, 0, 0);
        let mut counts: BTreeMap<Tone, u32> = BTreeMap::new();
        let mut secondary: BTreeMap<Tone, u32> = BTreeMap::new();
        for _ in 0..3000 {
            let (primary, second) = Personality::roll(&mut rng, TraitBias::NONE).tones();
            assert_ne!(primary, second);
            *counts.entry(primary).or_default() += 1;
            *secondary.entry(second).or_default() += 1;
        }
        for tone in Tone::ALL {
            let count = counts.get(&tone).copied().unwrap_or(0);
            assert!(
                (240..=900).contains(&count),
                "{tone:?} is primary for {count} of 3000: {counts:?}"
            );
            assert!(
                secondary.get(&tone).copied().unwrap_or(0) >= 200,
                "{secondary:?}"
            );
        }
    }

    #[test]
    fn extreme_traits_pick_the_obvious_tone() {
        let mut p = Personality::AVERAGE;
        p.neuroticism = 95;
        assert_eq!(p.tones().0, Tone::Nervous);
        let mut p = Personality::AVERAGE;
        p.agreeableness = 5;
        assert_eq!(p.tones().0, Tone::Gruff);
        let mut p = Personality::AVERAGE;
        p.extraversion = 95;
        assert_eq!(p.tones().0, Tone::Cheerful);
        let mut p = Personality::AVERAGE;
        p.openness = 95;
        assert_eq!(p.tones().0, Tone::Poetic);
        let mut p = Personality::AVERAGE;
        p.agreeableness = 95;
        assert_eq!(p.tones().0, Tone::Earnest);
        let mut p = Personality::AVERAGE;
        p.extraversion = 10;
        p.conscientiousness = 80;
        p.neuroticism = 20;
        assert_eq!(p.tones().0, Tone::Dry);
    }
}
