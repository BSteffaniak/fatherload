//! Authored dialogue line sets: one set per [`LineTopic`], each with variants for every
//! [`Tone`] bucket.
//!
//! Lines use the template grammar ([`super::grammar`]) and may only use [`COMMON_VARS`] plus
//! their topic's [`LineTopic::vars`]; the content tests check every line. Tone guide: warm
//! frontier, a little wry, light humour, mystery deeper down, never grim.
//!
//! - Gruff: short and blunt, soft underneath.
//! - Cheerful: bright, loud, teasing.
//! - Nervous: hedges, trails off, imagines the worst (and is usually wrong).
//! - Poetic: stone, sky and lamplight; a little grand.
//! - Dry: deadpan.
//! - Earnest: sincere and kind.

use crate::people::traits::Tone;

/// Variables every line may use: the speaker's `name` (what they go by), `role`, `work` (a
/// gerund phrase about their job), `village`, and the `player`'s name.
pub const COMMON_VARS: &[&str] = &["name", "role", "work", "village", "player"];

/// What a line set is for, and which extra variables it gets.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LineTopic {
    GreetFirst,
    GreetAgain,
    GreetFriend,
    GreetCold,
    Farewell,
    Work,
    Want,
    Fear,
    Value,
    AboutFamily,
    AboutFriend,
    AboutRival,
    AboutOwes,
    AboutLender,
    AboutSweetheart,
    AboutEstranged,
    AboutWentDeep,
    AboutMovedAway,
    Rumour,
    NoRumour,
    MoodLow,
    MoodHigh,
    RememberFoundPerson,
    RememberArc,
    RememberAbandoned,
    RememberGift,
    RememberSlight,
    RememberDepth,
    RememberArtifact,
    RememberRescue,
    RememberVillage,
    GiftLiked,
    GiftDisliked,
    Thanks,
}

impl LineTopic {
    pub const ALL: [Self; 34] = [
        Self::GreetFirst,
        Self::GreetAgain,
        Self::GreetFriend,
        Self::GreetCold,
        Self::Farewell,
        Self::Work,
        Self::Want,
        Self::Fear,
        Self::Value,
        Self::AboutFamily,
        Self::AboutFriend,
        Self::AboutRival,
        Self::AboutOwes,
        Self::AboutLender,
        Self::AboutSweetheart,
        Self::AboutEstranged,
        Self::AboutWentDeep,
        Self::AboutMovedAway,
        Self::Rumour,
        Self::NoRumour,
        Self::MoodLow,
        Self::MoodHigh,
        Self::RememberFoundPerson,
        Self::RememberArc,
        Self::RememberAbandoned,
        Self::RememberGift,
        Self::RememberSlight,
        Self::RememberDepth,
        Self::RememberArtifact,
        Self::RememberRescue,
        Self::RememberVillage,
        Self::GiftLiked,
        Self::GiftDisliked,
        Self::Thanks,
    ];

    /// Extra variables this topic provides (beyond [`COMMON_VARS`]).
    #[must_use]
    pub const fn vars(self) -> &'static [&'static str] {
        match self {
            Self::Want => &["want"],
            Self::Fear => &["fear"],
            Self::Value => &["value"],
            Self::AboutFamily
            | Self::AboutFriend
            | Self::AboutRival
            | Self::AboutOwes
            | Self::AboutLender
            | Self::AboutSweetheart
            | Self::AboutEstranged
            | Self::AboutWentDeep
            | Self::AboutMovedAway => &["rel", "rel_word"],
            Self::Rumour => &["place", "band", "depth"],
            Self::RememberFoundPerson => &["fact_name", "fact_relation", "days"],
            Self::RememberArc | Self::RememberAbandoned => &["arc", "days"],
            Self::RememberDepth => &["depth"],
            Self::RememberArtifact | Self::RememberRescue => &["fact_name", "days"],
            Self::RememberGift | Self::RememberVillage => &["days"],
            _ => &[],
        }
    }
}

/// Variants of one line for every tone bucket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToneLines {
    pub gruff: &'static [&'static str],
    pub cheerful: &'static [&'static str],
    pub nervous: &'static [&'static str],
    pub poetic: &'static [&'static str],
    pub dry: &'static [&'static str],
    pub earnest: &'static [&'static str],
}

impl ToneLines {
    #[must_use]
    pub const fn get(&self, tone: Tone) -> &'static [&'static str] {
        match tone {
            Tone::Gruff => self.gruff,
            Tone::Cheerful => self.cheerful,
            Tone::Nervous => self.nervous,
            Tone::Poetic => self.poetic,
            Tone::Dry => self.dry,
            Tone::Earnest => self.earnest,
        }
    }
}

/// One authored line set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineSet {
    /// Stable id (no-repeat keys hash it).
    pub id: &'static str,
    pub topic: LineTopic,
    pub lines: ToneLines,
}

/// The set for `topic`.
///
/// # Panics
///
/// Never for authored content: the content tests check every topic has exactly one set.
#[must_use]
pub fn line_set(topic: LineTopic) -> &'static LineSet {
    LINE_SETS
        .iter()
        .find(|set| set.topic == topic)
        .expect("every topic has a line set")
}

pub const LINE_SETS: &[LineSet] = &[
    LineSet {
        id: "greet_first",
        topic: LineTopic::GreetFirst,
        lines: ToneLines {
            gruff: &[
                "{name}. {Role}. You're new. Don't touch anything.",
                "Hm. Another driller. I'm {name}. I do the {role} work round here, and I do it fine without help.",
                "Name's {name}. If you're lost, {village}'s that way. If you're not, what do you want?",
            ],
            cheerful: &[
                "Well hello! A new face in {village}! I'm {name}, the [best|only|loudest] {role} in town!",
                "Oh, you must be {player}! Everyone's talking about you. Well, I am. I'm {name}!",
                "Welcome, welcome! {name}, {role}, professional good-news-haver. What's your name? {player}! Lovely!",
            ],
            nervous: &[
                "Oh! Sorry, you startled me. I'm {name}. The {role}. Mostly. Are you... staying long?",
                "H-hello. {name}. I'd shake your hand but mine are all {role}-y. Sorry.",
                "You're {player}, aren't you? I heard you go down in that rig. On purpose. I'm {name}.",
            ],
            poetic: &[
                "A new lamp on the road. I'm {name}, {role} of {village}, keeper of small wonders.",
                "Every stranger is a seam not yet opened. I'm {name}. And you smell of deep stone.",
                "Welcome to {village}, traveller. I'm {name}. The hills told me someone was coming.",
            ],
            dry: &[
                "You must be the famous {player}. I'm {name}. I'm famous for being here.",
                "{name}. {Role}. Yes, the whole job. No, it's not as exciting as it sounds. It doesn't sound exciting.",
                "Oh good, a newcomer. We were running low. I'm {name}.",
            ],
            earnest: &[
                "Hello there, and welcome to {village}. I'm {name}. If you need anything at all, come find me.",
                "You're {player}? I'm {name}, the {role}. It's really good to have someone new about.",
                "Welcome! I'm {name}. {Village} isn't much, but it's kind. You'll see.",
            ],
        },
    },
    LineSet {
        id: "greet_again",
        topic: LineTopic::GreetAgain,
        lines: ToneLines {
            gruff: &[
                "You again.",
                "{player}. What is it.",
                "Hm. Still in one piece, then.",
                "Back already. Wipe your treads.",
            ],
            cheerful: &[
                "{player}! There you are!",
                "Look who's back! Hi, {player}!",
                "Ooh, it's {player}! Come in, come in!",
                "My favourite driller! Well, top five! Top three!",
            ],
            nervous: &[
                "Oh, {player}! You're back. Good. That's good. Everything's fine?",
                "Hi, {player}. Sorry, I was just... never mind. Hi.",
                "{player}! You made it back up. I always count the rigs coming home.",
                "Oh! It's only you. I mean, it's you! Hello!",
            ],
            poetic: &[
                "{player}, back from the dark again, and still carrying a little of it.",
                "Ah, {player}. The day just got a little brighter.",
                "There's dust of three strata on your hull, {player}. Welcome back.",
                "The wind said you'd come by today, {player}.",
            ],
            dry: &[
                "{player}. You're back. I'll alert the newspapers.",
                "Oh, it's you. I was hoping it would be you. I was hoping a little.",
                "{player}. Still alive. Good for business.",
                "Ah, {player}. Here to brighten my day, or just passing through it?",
            ],
            earnest: &[
                "Good to see you, {player}.",
                "{player}! I was hoping you'd stop by.",
                "Welcome back, {player}. How was it down there?",
                "There you are, {player}. I'm glad you're safe.",
            ],
        },
    },
    LineSet {
        id: "greet_friend",
        topic: LineTopic::GreetFriend,
        lines: ToneLines {
            gruff: &[
                "{player}. Sit down. Kettle's on. Don't make a thing of it.",
                "There's my favourite driller. Don't tell the others.",
            ],
            cheerful: &[
                "{player}! My favourite person! Don't tell anyone, they'll be jealous!",
                "Everybody look, it's {player}! Hug? Hug. Too late, hugging.",
            ],
            nervous: &[
                "{player}! Oh, thank goodness, a friendly face. It's been a day.",
                "I'm so glad it's you, {player}. Honestly. You're the only one who doesn't make me jumpy.",
            ],
            poetic: &[
                "My friend. The lamps burn warmer when you're in {village}.",
                "{player}! Come, sit. Tell me what the deep said to you today.",
            ],
            dry: &[
                "{player}. My least annoying acquaintance. That's high praise; ask anyone.",
                "Oh, it's you. My day was going suspiciously well.",
            ],
            earnest: &[
                "{player}! You always make my day. Truly.",
                "There you are, friend. Come in out of the wind.",
            ],
        },
    },
    LineSet {
        id: "greet_cold",
        topic: LineTopic::GreetCold,
        lines: ToneLines {
            gruff: &[
                "You. Say your piece and go.",
                "I've not forgotten. Neither should you.",
            ],
            cheerful: &[
                "Oh. {player}. Hi. I'm usually a lot friendlier than this.",
                "Hello, {player}. I'm trying to be cheerful about it. Trying.",
            ],
            nervous: &[
                "Oh. It's you. I'm, um, very busy. Very busy.",
                "Please don't... look, just tell me what you need, {player}.",
            ],
            poetic: &[
                "There's a crack between us, {player}. Cracks can be mended, but not by wishing.",
                "You've come back. The rock remembers every blow, you know. So do I.",
            ],
            dry: &[
                "{player}. What a surprise. Not a pleasant one, but technically a surprise.",
                "Oh, wonderful. You.",
            ],
            earnest: &[
                "{player}. I'll be honest, I'm still hurt. But I'll hear you out.",
                "I want to trust you again, {player}. Help me out a little.",
            ],
        },
    },
    LineSet {
        id: "farewell",
        topic: LineTopic::Farewell,
        lines: ToneLines {
            gruff: &[
                "Go on, then.",
                "Mind your hull.",
                "Don't die. Paperwork's awful.",
            ],
            cheerful: &[
                "Bye, {player}! Bring back something shiny!",
                "Off you go! Dig deep, fly fast!",
                "See you soon! Wave if you pass overhead!",
            ],
            nervous: &[
                "Be careful down there. Really careful. Okay. Bye.",
                "You've got enough fuel, right? Right. Sorry. Bye.",
            ],
            poetic: &[
                "Go well, and may the dark be gentle with you.",
                "Until the next lamp, {player}.",
            ],
            dry: &[
                "Well, this was a conversation. Off you pop.",
                "Bye. Try not to fall in anything.",
            ],
            earnest: &[
                "Take care of yourself, {player}. I mean it.",
                "Come back safe. We'll be here.",
            ],
        },
    },
    LineSet {
        id: "work",
        topic: LineTopic::Work,
        lines: ToneLines {
            gruff: &[
                "Spent the morning {work}. Same as yesterday. Same as tomorrow.",
                "Work's work. Today it's {work}.",
                "Can't stop. I'm {work}. Well, I was.",
            ],
            cheerful: &[
                "Busy busy! I've been {work} all day and I love it!",
                "You'll never guess what I've been doing. {Work}! Okay, you might've guessed.",
                "Being {a role} is the best job in the world. Today I was {work}!",
            ],
            nervous: &[
                "I've been {work}. I think it's going okay? It's hard to tell.",
                "Oh, work's... it's fine. I'm {work}. Nobody's complained yet. Yet.",
            ],
            poetic: &[
                "Today I was {work}. There's a kind of music in it, if you listen.",
                "{A role}'s life is small and good. This morning: {work}, and watching the light move.",
            ],
            dry: &[
                "I've been {work}. Living the dream. Someone's dream.",
                "Today's thrilling instalment: {work}.",
            ],
            earnest: &[
                "I've been {work}. It's honest work, and somebody has to do it.",
                "Being {a role} keeps {village} going, in its small way. Today I was {work}.",
            ],
        },
    },
    LineSet {
        id: "want",
        topic: LineTopic::Want,
        lines: ToneLines {
            gruff: &[
                "What I want? To {want}. Don't make it a thing.",
                "One thing. I want to {want}. Then I'll sit down for good.",
            ],
            cheerful: &[
                "One day I'm going to {want}! Just you watch!",
                "Can I tell you a secret? I want to {want}! There, I said it!",
            ],
            nervous: &[
                "Sometimes I think I'd like to {want}. But what if it goes wrong?",
                "Don't laugh. I want to {want}. You're not laughing. Thank you.",
            ],
            poetic: &[
                "Every heart has a seam it's digging toward. Mine? To {want}.",
                "I dream of it at night: to {want}. The dream is very patient with me.",
            ],
            dry: &[
                "My life's ambition is to {want}. Then maybe a nap.",
                "I want to {want}. Some people collect stamps.",
            ],
            earnest: &[
                "If I'm honest, more than anything I want to {want}.",
                "I've never told many people this, but I want to {want}.",
            ],
        },
    },
    LineSet {
        id: "fear",
        topic: LineTopic::Fear,
        lines: ToneLines {
            gruff: &[
                "Scared? Me? ...Fine. {Fear}. Tell anyone and you're walking home.",
                "Everyone's scared of something. Mine's {fear}. There. Done.",
            ],
            cheerful: &[
                "I'm not scared of anything! Except {fear}. And geese. Mostly the first one!",
                "Ha! I laugh at danger! I don't laugh at {fear}, though. Nobody does!",
            ],
            nervous: &[
                "Don't say it too loud, but... {fear}. Gets me every time.",
                "I lie awake thinking about {fear}. Do you? No? Just me then.",
            ],
            poetic: &[
                "We all carry a shadow. Mine is {fear}.",
                "There's a cold place in me, and it's shaped like {fear}.",
            ],
            dry: &[
                "My greatest fear is {fear}. My second greatest is this conversation.",
                "{Fear}. Not keen. Next question.",
            ],
            earnest: &[
                "I'll tell you the truth: {fear} frightens me. Always has.",
                "Can I be honest? {Fear}. It's silly, maybe, but it's real to me.",
            ],
        },
    },
    LineSet {
        id: "value",
        topic: LineTopic::Value,
        lines: ToneLines {
            gruff: &[
                "What matters? {Value}. Everything else is noise.",
                "You want advice? Hold on to {value}. That's it. That's the advice.",
            ],
            cheerful: &[
                "Know what makes life worth it? {Value}! And pie. But mostly the first thing!",
                "My motto: {value}, every day, all day!",
            ],
            nervous: &[
                "I think what matters is... {value}? Is that a silly answer?",
                "When it all goes wrong, and it does, I hang on to {value}.",
            ],
            poetic: &[
                "Strip away the dust and what's left is {value}. That's the real ore.",
                "Some dig for gold. I've always dug for {value}.",
            ],
            dry: &[
                "I believe in {value}. Also in locking the shed.",
                "What matters is {value}. I'd embroider it on a cushion if I could sew.",
            ],
            earnest: &[
                "For me, it comes down to {value}. Always has.",
                "Whatever else happens, {value}. That's what I try to live by.",
            ],
        },
    },
    LineSet {
        id: "about_family",
        topic: LineTopic::AboutFamily,
        lines: ToneLines {
            gruff: &[
                "My {rel_word}, {rel}. Stubborn. Gets it from me.",
                "{Rel} is my {rel_word}. If anyone gives {rel} trouble, they answer to me.",
            ],
            cheerful: &[
                "Have you met my {rel_word}? {Rel} is wonderful! A bit much! Like me!",
                "My {rel_word} {rel} is the best, don't tell {rel} I said so!",
            ],
            nervous: &[
                "My {rel_word}, {rel}... I worry about {rel} constantly. Is that normal?",
                "{Rel} is my {rel_word}. If you see {rel} near the shaft, tell me, okay?",
            ],
            poetic: &[
                "My {rel_word}, {rel}: same blood, different weather.",
                "{Rel} and I grew from the same root, my {rel_word} and I. Bent different ways in the wind.",
            ],
            dry: &[
                "My {rel_word}, {rel}. We get along beautifully as long as we're in different rooms.",
                "{Rel}? {Rel} is my {rel_word}. I didn't choose it, but I'd choose it.",
            ],
            earnest: &[
                "My {rel_word}, {rel}, means the world to me.",
                "I'd do anything for my {rel_word}. {Rel} is the reason I stay in {village}.",
            ],
        },
    },
    LineSet {
        id: "about_friend",
        topic: LineTopic::AboutFriend,
        lines: ToneLines {
            gruff: &[
                "{Rel} is all right. Don't tell {rel} I said that.",
                "{Rel} and I go back. Owe each other more than coin.",
            ],
            cheerful: &[
                "{Rel} is my best friend in the whole valley! We do everything together!",
                "Oh, you have to meet {rel}! Funniest person alive! After me!",
            ],
            nervous: &[
                "{Rel} is a good friend. The only one who doesn't mind when I fret.",
                "I hope {rel} still likes me. Do you think {rel} still likes me?",
            ],
            poetic: &[
                "{Rel} is the kind of friend who's a lamp in a long tunnel.",
                "Friendship's a seam you work together. {Rel} and I have worked ours deep.",
            ],
            dry: &[
                "{Rel} is my friend. We tolerate each other professionally.",
                "{Rel} laughs at my jokes. That's a very rare quality.",
            ],
            earnest: &[
                "{Rel} has been a true friend to me. I'm lucky.",
                "If you ever need help and I'm not around, go to {rel}.",
            ],
        },
    },
    LineSet {
        id: "about_rival",
        topic: LineTopic::AboutRival,
        lines: ToneLines {
            gruff: &[
                "{Rel}. Don't say that name in here.",
                "{Rel} thinks they're better than me. {Rel} is wrong.",
            ],
            cheerful: &[
                "Ha! {Rel}? We have a friendly rivalry! I'm winning! Very friendly!",
                "{Rel} and I are in a little competition. I'm ahead. Don't check.",
            ],
            nervous: &[
                "{Rel} is always one step ahead of me. Always. It's exhausting.",
                "Did {rel} say anything about me? What did {rel} say?",
            ],
            poetic: &[
                "{Rel} and I are two drills in the same rock: one of us will reach the gold first.",
                "Every flame needs a draught. Mine is {rel}, curse their name, gently.",
            ],
            dry: &[
                "{Rel}? Lovely person. Terrible at everything I'm good at.",
                "Ah, {rel}. My nemesis. We wave at each other very politely.",
            ],
            earnest: &[
                "{Rel} and I don't see eye to eye. I wish we did, honestly.",
                "I respect {rel}. I just can't stand losing to {rel}.",
            ],
        },
    },
    LineSet {
        id: "about_owes",
        topic: LineTopic::AboutOwes,
        lines: ToneLines {
            gruff: &[
                "I owe {rel}. I'll pay. Every coin. Don't ask.",
                "{Rel} lent me money when nobody else would. I don't forget that.",
            ],
            cheerful: &[
                "I owe {rel} a teeny bit of money! Well, a lot! I'll pay it back any day now!",
                "{Rel} is my creditor! Fancy word, isn't it? Means I'm broke!",
            ],
            nervous: &[
                "I still owe {rel}. Every time I see {rel} my stomach does a flip.",
                "Please don't mention money near {rel}. I'll pay, I will, I just need one good haul.",
            ],
            poetic: &[
                "There's a weight I carry with {rel}'s name on it. A debt is a stone in the pocket.",
                "I owe {rel} more than coin, and the coin is heavy enough.",
            ],
            dry: &[
                "{Rel} and I have a financial relationship. I'm the finance; {rel} is the relationship.",
                "I owe {rel} money. {Rel} reminds me daily. It's a lovely tradition.",
            ],
            earnest: &[
                "{Rel} helped me through a bad winter. I'm going to repay that debt, properly.",
                "I owe {rel}, and I hate owing anyone. I'll make it right.",
            ],
        },
    },
    LineSet {
        id: "about_lender",
        topic: LineTopic::AboutLender,
        lines: ToneLines {
            gruff: &[
                "{Rel} owes me. {Rel} knows it.",
                "Lent {rel} money. Haven't seen it since. Seen plenty of {rel}, mind.",
            ],
            cheerful: &[
                "{Rel} owes me money, but who's counting! Me. I'm counting. Cheerfully!",
                "Oh, {rel}'ll pay me back. Probably! Eventually! I'm an optimist!",
            ],
            nervous: &[
                "I lent {rel} some money and now I don't know how to ask for it back.",
                "Do you think {rel} remembers the loan? I don't want to be rude.",
            ],
            poetic: &[
                "I gave {rel} a little of my winter. One day spring will give it back.",
                "Money lent is a seed planted. Mine, in {rel}, has not yet sprouted.",
            ],
            dry: &[
                "{Rel} owes me money. I've started thinking of it as a donation with feelings.",
                "I lent {rel} some coin. Best investment I ever made, if you count disappointment.",
            ],
            earnest: &[
                "I helped {rel} out when times were hard. That's what neighbours do.",
                "{Rel} owes me a little, but I'm not worried. {Rel} is good for it.",
            ],
        },
    },
    LineSet {
        id: "about_sweetheart",
        topic: LineTopic::AboutSweetheart,
        lines: ToneLines {
            gruff: &[
                "{Rel}? What about {rel}? Nothing. No reason. Stop looking at me like that.",
                "{Rel} fixed my fence once. That's all. Shut up.",
            ],
            cheerful: &[
                "Don't tell anyone, but I think {rel} is wonderful! Oh no, I said it out loud!",
                "Have you noticed {rel}'s laugh? Isn't it the best laugh? Asking for me!",
            ],
            nervous: &[
                "Does {rel}... ever mention me? No, don't tell me. Yes, tell me.",
                "I tried to talk to {rel} yesterday and I said 'good weather for rocks'. Rocks!",
            ],
            poetic: &[
                "{Rel} walks by and the whole valley seems to hum.",
                "If I wrote a poem about {rel}, and I haven't, and I have, it'd be about lamplight.",
            ],
            dry: &[
                "I don't have feelings for {rel}. I have a very specific medical condition around {rel}.",
                "{Rel}? Yes. Fine. Perhaps. Change the subject.",
            ],
            earnest: &[
                "I'm sweet on {rel}. There. I don't know if it's returned, but I've said it.",
                "{Rel} is the kindest person I know. I'd like {rel} to know that I think so.",
            ],
        },
    },
    LineSet {
        id: "about_estranged",
        topic: LineTopic::AboutEstranged,
        lines: ToneLines {
            gruff: &[
                "{Rel} and I had words. Bad ones. Mine were worse.",
                "Don't ask about {rel}. ...Fine. I was wrong. There.",
            ],
            cheerful: &[
                "{Rel} and I fell out. I keep meaning to fix it! Tomorrow, maybe! Or the day after!",
                "I used to be friends with {rel}. I'm going to be again! I've decided!",
            ],
            nervous: &[
                "I said something awful to {rel} once and I think about it every night.",
                "What if {rel} never forgives me? What if {rel} already has and I missed it?",
            ],
            poetic: &[
                "{Rel} and I broke like a seam under too much weight. Some cracks can still be filled.",
                "There's a road between me and {rel}, overgrown now. I walk it in my sleep.",
            ],
            dry: &[
                "{Rel} and I aren't speaking. Well, I'm speaking. {Rel} is doing the other thing.",
                "We had a disagreement, {rel} and I. It's lasted longer than most marriages.",
            ],
            earnest: &[
                "I hurt {rel}, and I've regretted it ever since. I want to make it right.",
                "If I could take back one thing, it'd be what I said to {rel}.",
            ],
        },
    },
    LineSet {
        id: "about_went_deep",
        topic: LineTopic::AboutWentDeep,
        lines: ToneLines {
            gruff: &[
                "My {rel_word}, {rel}, went down and didn't come up. Rig's still somewhere. So's the answer.",
                "{Rel} was the best driller I knew. Went too deep. That's all I'll say.",
            ],
            cheerful: &[
                "My {rel_word} {rel} went off exploring way down deep! Somewhere! I like to think {rel} is having an adventure!",
                "{Rel} is out there! My {rel_word}! Just hasn't written. For a while. A long while.",
            ],
            nervous: &[
                "My {rel_word} went down past the slate and... I keep the lamp lit. Just in case {rel} comes back.",
                "If you ever find a rig with {rel}'s name scratched on it, please tell me. Please.",
            ],
            poetic: &[
                "My {rel_word} {rel} followed something down into the dark. I like to think it was beautiful.",
                "The deep took {rel} like a tide. Tides return things, sometimes.",
            ],
            dry: &[
                "My {rel_word} {rel} went deep and stopped answering letters. Not the first time, to be fair.",
                "{Rel} said 'back by supper'. That was a lot of suppers ago.",
            ],
            earnest: &[
                "My {rel_word}, {rel}, went down and never came back. I just want to know what happened.",
                "I don't need {rel} back. I'd just like to know {rel} found what they were looking for.",
            ],
        },
    },
    LineSet {
        id: "about_moved_away",
        topic: LineTopic::AboutMovedAway,
        lines: ToneLines {
            gruff: &[
                "My {rel_word}'s off in some other town. Writes when the mood takes. Mood rarely takes.",
                "{Rel} moved away. Fine by me. Kitchen's quieter.",
            ],
            cheerful: &[
                "My {rel_word} {rel} lives way off past the hills now! Sends the best letters!",
                "{Rel} is off seeing the world! I'm so proud! And a bit jealous!",
            ],
            nervous: &[
                "My {rel_word} moved away. I hope {rel} is eating properly.",
                "{Rel} lives so far away now. What if something happens and I don't hear?",
            ],
            poetic: &[
                "My {rel_word} went where the road bends out of sight. I still look down it sometimes.",
                "{Rel} is a letter in a drawer and a voice I hear in the wind.",
            ],
            dry: &[
                "My {rel_word} {rel} moved away. We're very close now. Geographically, not.",
                "{Rel} left for the big town. Big town's welcome to {rel}.",
            ],
            earnest: &[
                "I miss my {rel_word}, {rel}. Every day, a little.",
                "{Rel} moved away to find work. I'm proud, but I miss them.",
            ],
        },
    },
    LineSet {
        id: "rumour",
        topic: LineTopic::Rumour,
        lines: ToneLines {
            gruff: &[
                "There's {a place} down in {band}. {Depth}m or so. Go look, if you must.",
                "Heard about {a place}. {Band}, around {depth}m. Don't say I sent you.",
            ],
            cheerful: &[
                "Ooh, rumour time! Someone saw {a place} down in {band}! About {depth}m! Go go go!",
                "Want a tip? {A place}, {band}, {depth}m-ish! Bring me back a story!",
            ],
            nervous: &[
                "People say there's {a place} in {band}, near {depth}m. I wouldn't go. You might, though.",
                "I heard about {a place} down around {depth}m. In {band}. Be careful, okay?",
            ],
            poetic: &[
                "The rock whispers of {a place}, waiting in {band}, some {depth}m below the grass.",
                "Down in {band}, near {depth}m, {a place} sleeps. Perhaps it's waiting for you.",
            ],
            dry: &[
                "Rumour says there's {a place} in {band}, about {depth}m down. Rumour says a lot of things.",
                "{A place} in {band}, {depth}m. Or a large rock. Usually it's a large rock.",
            ],
            earnest: &[
                "A driller passing through mentioned {a place} in {band}, around {depth}m down. Thought you should know.",
                "If you're heading deep, there's {a place} near {depth}m in {band}. Worth a look.",
            ],
        },
    },
    LineSet {
        id: "no_rumour",
        topic: LineTopic::NoRumour,
        lines: ToneLines {
            gruff: &[
                "No news. Good news, usually.",
                "Haven't heard anything. Ask someone nosier.",
            ],
            cheerful: &[
                "No gossip today! Can you believe it? I'm devastated!",
                "Nothing new! Go make some news and come tell me!",
            ],
            nervous: &[
                "I haven't heard anything. Is that bad? Quiet is sometimes bad.",
                "No news. Which is either good or the calm before something.",
            ],
            poetic: &[
                "The valley's quiet today. Even the wind is keeping its secrets.",
                "No whispers from the deep today. It's sleeping, I think.",
            ],
            dry: &[
                "Breaking news: nothing. Film at eleven.",
                "Nothing to report. The goat got out again, if that counts. It doesn't.",
            ],
            earnest: &[
                "I haven't heard anything new, sorry. I'll keep my ears open for you.",
                "Nothing yet. If I hear anything, you'll be the first to know.",
            ],
        },
    },
    LineSet {
        id: "mood_low",
        topic: LineTopic::MoodLow,
        lines: ToneLines {
            gruff: &[
                "Bad day. Don't ask.",
                "Not in the mood, {player}. Nothing personal.",
            ],
            cheerful: &[
                "I'm a bit down today, honestly. Tell me something funny?",
                "Not my sunniest day! But it's just a cloud. Clouds move!",
            ],
            nervous: &[
                "Everything feels wrong today. Do you ever get that? The whole sky feels heavy.",
                "Sorry, I'm all knotted up today. It'll pass. It usually passes.",
            ],
            poetic: &[
                "Some days the lamps burn low inside you. Today's one of those.",
                "The sky's grey and so am I, a little.",
            ],
            dry: &[
                "I'm having a day. It's one of the bad ones. We have a range.",
                "On a scale of one to ten, I'm a damp sock.",
            ],
            earnest: &[
                "I'm feeling a bit low today, if I'm honest. Thanks for stopping by, it helps.",
                "Rough day. Talking to you is the nicest bit of it.",
            ],
        },
    },
    LineSet {
        id: "mood_high",
        topic: LineTopic::MoodHigh,
        lines: ToneLines {
            gruff: &[
                "Good day today. Don't spoil it.",
                "Feeling all right, actually. Weird.",
            ],
            cheerful: &[
                "What a day! I could hug a boulder!",
                "I'm in such a good mood I might sing! You've been warned!",
            ],
            nervous: &[
                "I feel good today! Which makes me nervous. But good!",
                "Everything's going right. Suspiciously right. I'm enjoying it anyway.",
            ],
            poetic: &[
                "The whole valley's golden today. Even the dust is dancing.",
                "Days like this, you can hear the hills hum.",
            ],
            dry: &[
                "I'm in a good mood. Enjoy it, it's seasonal.",
                "I'm actually happy today. Mark the calendar.",
            ],
            earnest: &[
                "I'm having a really lovely day. I hope yours is too.",
                "Everything feels right today. I'm grateful for it.",
            ],
        },
    },
    LineSet {
        id: "remember_found_person",
        topic: LineTopic::RememberFoundPerson,
        lines: ToneLines {
            gruff: &[
                "You're the one who found my {fact_relation}'s rig. ...I never said thanks. Thanks.",
                "{Days} you brought me word of {fact_name}. Don't think I've forgotten.",
            ],
            cheerful: &[
                "You found {fact_name}! My {fact_relation}! I still can't believe it! You're my hero!",
                "Everyone, this is the one who found my {fact_relation}'s rig! Applause!",
            ],
            nervous: &[
                "I still think about what you found, about {fact_name}. Knowing helps. It really does.",
                "You're the one who found my {fact_relation}. I sleep better now. I thought you should know.",
            ],
            poetic: &[
                "You went into the dark and brought my {fact_relation}'s story back to the light.",
                "{Fact_name}'s rig, found. The deep gave something back, because of you.",
            ],
            dry: &[
                "You found {fact_name}'s rig. Twelve years I looked. You just went and found it. Show-off.",
                "My {fact_relation}'s rig turned up, thanks to you. Typical {fact_name}: parked badly.",
            ],
            earnest: &[
                "I'll never forget what you did, finding my {fact_relation}. Never.",
                "Because of you I know what happened to {fact_name}. Thank you, {player}.",
            ],
        },
    },
    LineSet {
        id: "remember_arc",
        topic: LineTopic::RememberArc,
        lines: ToneLines {
            gruff: &[
                "That business, '{arc}'. You did right by me.",
                "Still owe you for '{arc}'. Don't think I don't know it.",
            ],
            cheerful: &[
                "Remember '{arc}'? What a time! We should do it again! Not exactly that! Something!",
                "I tell everyone about '{arc}'! I've improved it a bit. You fight a bear now.",
            ],
            nervous: &[
                "I'm still so grateful about '{arc}'. I don't know what I'd have done.",
                "Every time I think about '{arc}' I get the shakes. Good shakes. Grateful shakes.",
            ],
            poetic: &[
                "'{Arc}': I've woven it into a story. You're the bright thread.",
                "What you did, '{arc}', still echoes round {village}.",
            ],
            dry: &[
                "'{Arc}'. Yes, I remember. I've been meaning to be grateful at you.",
                "After '{arc}' I had to admit you're competent. It was hard for me.",
            ],
            earnest: &[
                "I think about '{arc}' often. You made a real difference, {player}.",
                "'{Arc}'... thank you, again. It meant more than you know.",
            ],
        },
    },
    LineSet {
        id: "remember_abandoned",
        topic: LineTopic::RememberAbandoned,
        lines: ToneLines {
            gruff: &[
                "You walked away from '{arc}'. I remember.",
                "Said you'd see '{arc}' through. You didn't.",
            ],
            cheerful: &[
                "So, '{arc}'... no hard feelings! Some feelings. Soft ones!",
                "We'll just pretend '{arc}' never happened! Mostly!",
            ],
            nervous: &[
                "I was counting on you with '{arc}'. It's okay. It's... it's okay.",
                "After '{arc}' I wasn't sure you'd come back. You came back.",
            ],
            poetic: &[
                "'{Arc}' is still an unfinished tunnel between us, {player}.",
                "You left '{arc}' half-dug. The rock waits, and so do I.",
            ],
            dry: &[
                "'{Arc}'. You gave up on it. Bold strategy.",
                "Ah, the '{arc}' business. You were very decisive about not doing it.",
            ],
            earnest: &[
                "I was hurt when you dropped '{arc}'. But I'd still like your help, someday.",
                "Let's not let '{arc}' come between us. I'd rather start again.",
            ],
        },
    },
    LineSet {
        id: "remember_gift",
        topic: LineTopic::RememberGift,
        lines: ToneLines {
            gruff: &[
                "That gift {days}. Kept it. Don't make it weird.",
                "Still got what you gave me. On the shelf. Where I can see it.",
            ],
            cheerful: &[
                "I still love that present you gave me {days}! I show it to everyone!",
                "Your gift is on my mantelpiece! Pride of place! Next to my other pride!",
            ],
            nervous: &[
                "I never thanked you properly for the gift {days}. Thank you. Was that weird?",
                "I still have your gift. I'm scared to use it in case it breaks.",
            ],
            poetic: &[
                "Your gift sits by my window. It catches the morning light.",
                "A gift is a little lamp left in someone's house. Yours still burns.",
            ],
            dry: &[
                "You gave me a present {days}. I've decided to forgive you for being thoughtful.",
                "I still have your gift. That puts it ahead of most of my possessions.",
            ],
            earnest: &[
                "I still treasure the gift you gave me, {player}.",
                "That gift {days} really touched me. Thank you.",
            ],
        },
    },
    LineSet {
        id: "remember_slight",
        topic: LineTopic::RememberSlight,
        lines: ToneLines {
            gruff: &[
                "I haven't forgotten what you did.",
                "You owe me an apology. A good one.",
            ],
            cheerful: &[
                "I'm still a teensy bit cross with you! A teensy bit! Teeny!",
                "Let's not talk about the thing! I'm over it! Mostly over it!",
            ],
            nervous: &[
                "I'm still a bit upset about... you know. It's fine. It's not fine.",
                "Please don't do that again. What you did. Please.",
            ],
            poetic: &[
                "There's a scratch on the lamp now, {player}. It still shines, but I see it.",
                "Some words go down deep and stay there, like iron in clay.",
            ],
            dry: &[
                "I'm not still angry. I'm keeping a list. That's different.",
                "Oh, I've let it go. I just keep picking it back up.",
            ],
            earnest: &[
                "What you did hurt me. I want to move past it, but I need you to know.",
                "I'm trying to forgive you, {player}. Honestly, I am.",
            ],
        },
    },
    LineSet {
        id: "remember_depth",
        topic: LineTopic::RememberDepth,
        lines: ToneLines {
            gruff: &[
                "{Depth}m, they say you went. Madness. Respect.",
                "Heard you hit {depth}m. Deeper than I ever went. Don't let it go to your head.",
            ],
            cheerful: &[
                "{Depth}m! You're a legend! I'm telling everyone I know you!",
                "Is it true you went down {depth}m? That's so deep! What's it like? Hot? Spooky?",
            ],
            nervous: &[
                "You really went down to {depth}m? I got dizzy just hearing about it.",
                "{Depth}m... please tell me you're being careful down there.",
            ],
            poetic: &[
                "{Depth}m down. You've been where the old stone dreams.",
                "They say at {depth}m the rock is warm, like something sleeping. Is it true?",
            ],
            dry: &[
                "{Depth}m. Showing off, I see.",
                "You went to {depth}m. Some of us went to the shop. Everyone has their thing.",
            ],
            earnest: &[
                "I heard you reached {depth}m. That's incredible, {player}.",
                "{Depth}m down! I'm proud of you. Is that strange to say? I am.",
            ],
        },
    },
    LineSet {
        id: "remember_artifact",
        topic: LineTopic::RememberArtifact,
        lines: ToneLines {
            gruff: &[
                "That {fact_name} you dug up. Interesting. Keep it away from me.",
                "You found the {fact_name}. Good. Don't sell it to a fool.",
            ],
            cheerful: &[
                "The {fact_name}! You actually found it! Can I touch it? Can I hold it? I'll be careful!",
                "Everyone's talking about your {fact_name}! I started half the talk!",
            ],
            nervous: &[
                "That {fact_name}... does it hum at night? Mine would hum at night.",
                "You found a {fact_name}? Was it... warm? It wasn't warm, was it?",
            ],
            poetic: &[
                "The {fact_name}: a word from the deep, written in a language we've forgotten.",
                "You lifted the {fact_name} into daylight. I wonder if it missed the dark.",
            ],
            dry: &[
                "A {fact_name}. Very nice. I found a button once.",
                "So you found a {fact_name}. And I thought I was having a good week.",
            ],
            earnest: &[
                "Finding that {fact_name} was amazing work, {player}.",
                "The {fact_name} you found {days}... it's the most wonderful thing I've seen.",
            ],
        },
    },
    LineSet {
        id: "remember_rescue",
        topic: LineTopic::RememberRescue,
        lines: ToneLines {
            gruff: &[
                "You hauled {fact_name} out of that hole. I owe you for that.",
                "{Fact_name} would still be down there if not for you. Hm. Good work.",
            ],
            cheerful: &[
                "You saved {fact_name}! You're a hero! An actual hero! I'm baking you something!",
                "Three cheers for the one who rescued {fact_name}! I'll do all three!",
            ],
            nervous: &[
                "When {fact_name} was stuck I couldn't breathe. And then you came. Thank you.",
                "If you hadn't gone after {fact_name}... I don't want to think about it.",
            ],
            poetic: &[
                "You went into the earth's fist and pried {fact_name} free.",
                "{Fact_name} came back up into the sunlight because of you.",
            ],
            dry: &[
                "You rescued {fact_name}. I suppose that makes you useful.",
                "{Fact_name} owes you their life. I've already told {fact_name} to pay up.",
            ],
            earnest: &[
                "Thank you for bringing {fact_name} home. Truly.",
                "We nearly lost {fact_name}. You didn't let that happen.",
            ],
        },
    },
    LineSet {
        id: "remember_village",
        topic: LineTopic::RememberVillage,
        lines: ToneLines {
            gruff: &[
                "{Village}'s better for what you did. Don't let it go to your head.",
                "You helped this place. People noticed. I noticed.",
            ],
            cheerful: &[
                "{Village} owes you big! We should throw you a party! I'm throwing you a party!",
                "Have you seen {village} lately? Thanks to you! Look at it! Gleaming!",
            ],
            nervous: &[
                "Things are so much calmer in {village} since you helped. I can breathe again.",
                "We were all so worried, and then you sorted it. Thank you.",
            ],
            poetic: &[
                "You put a little light back into {village}. It shows in every window.",
                "{Village} will tell stories of what you did, long after we're dust.",
            ],
            dry: &[
                "You saved {village}. We'll name a ditch after you. A nice ditch.",
                "{Village} is grateful. Some of us are even grateful out loud.",
            ],
            earnest: &[
                "What you did for {village} means everything to us.",
                "You've done so much for {village}, {player}. Thank you.",
            ],
        },
    },
    LineSet {
        id: "gift_liked",
        topic: LineTopic::GiftLiked,
        lines: ToneLines {
            gruff: &[
                "For me? ...Hm. Thanks.",
                "Didn't need to do that. Glad you did.",
            ],
            cheerful: &[
                "For me?! I love it! I love you! I mean, I love it!",
                "Oh, it's perfect! I'm going to show everyone!",
            ],
            nervous: &[
                "For me? Really? Oh, I don't know what to say. Thank you!",
                "You shouldn't have! No, I mean, I'm glad you did! Thank you!",
            ],
            poetic: &[
                "A gift is a lamp left in someone's window. Thank you, {player}.",
                "How lovely. I'll keep it where the light can find it.",
            ],
            dry: &[
                "A gift. How thoughtful. I'm genuinely touched, and I hate it.",
                "For me? You'll make me soft. Stop it.",
            ],
            earnest: &[
                "Thank you, {player}. This really means a lot.",
                "That's so kind. I'll treasure it.",
            ],
        },
    },
    LineSet {
        id: "gift_disliked",
        topic: LineTopic::GiftDisliked,
        lines: ToneLines {
            gruff: &[
                "What am I meant to do with this?",
                "Hm. Well. It's a thing.",
            ],
            cheerful: &[
                "Oh! It's... a thing! How... thingy! Thank you!",
                "Ha! I have no idea what this is! I'll find a use! Probably!",
            ],
            nervous: &[
                "Oh. Thank you. I'm... sure it's very useful. To someone.",
                "That's, um, very kind. Is it supposed to smell like that?",
            ],
            poetic: &[
                "Every gift says something. This one says... something.",
                "An unusual offering. The deep has stranger tastes than mine.",
            ],
            dry: &[
                "Thank you. I'll treasure this until you leave.",
                "Wow. You remembered I exist. That's the main thing.",
            ],
            earnest: &[
                "It's not really my sort of thing, but thank you for thinking of me.",
                "That's kind of you. It's the thought that counts, and it counts.",
            ],
        },
    },
    LineSet {
        id: "thanks",
        topic: LineTopic::Thanks,
        lines: ToneLines {
            gruff: &[
                "Good work. That's all.",
                "You did it. Didn't think you would. Thanks.",
            ],
            cheerful: &[
                "You did it! I knew you would! Well, I hoped! Same thing!",
                "Thank you thank you thank you!",
            ],
            nervous: &[
                "You did it! Oh, I'm so relieved. I was so worried. Thank you.",
                "Thank you. Really. I didn't sleep a wink while you were down there.",
            ],
            poetic: &[
                "You've done a fine thing, {player}. The kind of thing songs are made from.",
                "Thank you. The deep gave up a little of itself for you today.",
            ],
            dry: &[
                "You did it. I'll try to contain my excitement. Done.",
                "Well. You're more useful than you look. Thanks.",
            ],
            earnest: &[
                "Thank you, {player}. I won't forget this.",
                "I can't thank you enough. Really.",
            ],
        },
    },
];
