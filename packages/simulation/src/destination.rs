//! The destination index (Wave 14 W14-B, test mode): a deterministic "where is X" for every
//! kind of place in the game, answered from the generators without streaming the world in.
//!
//! * **Kinds** ([`DestinationKind`], append only): depth (a Descent, a stratum, an exact depth),
//!   the home world's places (the town, villages, sky islands, tombs, caves, vaults, anomaly
//!   rooms, landmarks, settlements, outposts, story logs, camps, boss arenas, gate set pieces,
//!   the Heart, the Moon), space (systems, planets and moons by theme, stations, belts, comets,
//!   nebulae, black holes, wormholes, the Archive's chart systems, theme-boss lairs, alien
//!   towns, builder outposts) and people and history (other players, bookmarks, the back-stack
//!   of recent places).
//! * **Candidates** of a kind are gathered by [`home`] (pure functions of the world settings,
//!   a bounded window round the player) and [`space`] (the space generator's sectors in rings
//!   round the player), each a [`Destination`]: a stable [`DestinationId`] (the kind, the place
//!   and the target tile there) with the Atlas's info card ([`PreviewInfo`]).
//! * **Selectors** ([`Selector`]): nearest, next / previous (cycling outward from where the
//!   player was when the cycle began, see [`TravelCursor`]), random (from a seed the caller
//!   picks), or an exact id (what a name search, a bookmark or the Atlas resolved). A
//!   [`DestinationFilter`] narrows a kind by Descent, stratum, depth, theme or player, so
//!   "the next planet", "a random ice world" and "Descent V" are each one [`TravelRequest`].
//! * **Travel** is the test-mode action [`crate::TestModeAction::TravelTo`] (also `TravelBack`,
//!   `Bookmark`, `RemoveBookmark`): the world resolves the request and moves the rig with
//!   **safe arrival** (`world/destinations.rs`): underground a freshly carved pocket clear of
//!   lava, open water and enemies (unless [`ArrivalOptions`] opts into the water or the arena),
//!   on a planet its surface (local up, rig level), in space stopped a safe distance off.
//!   Arrival never counts toward records or milestones (test mode counts nothing) and never
//!   touches generation.
//! * **Bookmarks and the back-stack** are each player's [`PlayerTravel`] in the world's
//!   [`TravelLog`] (the shared `SectionKey::Travel` section: saved and replicated).

pub mod home;
pub mod space;

use std::{collections::BTreeMap, fmt::Write as _};

use serde::{Deserialize, Serialize};

use crate::{
    descent::{Descent, Stratum},
    ids::PlayerId,
    place::Place,
    space::Theme,
};

/// Bookmarks a player keeps (the oldest goes when a new one would pass this).
pub const MAX_BOOKMARKS: usize = 32;
/// Places on a player's back-stack (the oldest goes first).
pub const MAX_RECENT: usize = 16;
/// Candidates a kind lists at most (the Atlas's list, the cycle of next / previous).
pub const MAX_CANDIDATES: usize = 64;
/// A candidate closer than this (tiles, same place) counts as "here": next skips it.
pub const HERE_TILES: i64 = 24;

/// What kind of place a destination is (append only: the order is the wire and save order).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DestinationKind {
    /// The top of one of the eight Descents.
    #[default]
    Descent,
    /// The top of one of the 41 strata.
    Stratum,
    /// An exact depth under the ground (metres are rows).
    Depth,
    /// The home town's spawn point.
    HomeTown,
    /// A surface village or a sky town.
    Village,
    /// A floating sky island or sky continent.
    SkyIsland,
    /// A tomb or crypt's door.
    Tomb,
    /// A cave ecosystem or a large cavern.
    Cave,
    /// A vault (a point of interest).
    Vault,
    /// An anomaly room (the expedition's rooms and the Strange Layers' crystal rooms).
    AnomalyRoom,
    /// A point of interest or a named set piece of the Descents.
    Landmark,
    /// An underground settlement or a deep settlement's hall.
    Settlement,
    /// A Descent's deep outpost.
    Outpost,
    /// One of the Fathers' expedition log sites.
    StoryLog,
    /// A camp: the expedition's or a wanderers' camp on the surface.
    Camp,
    /// A Descent's gate boss arena (and its mini-boss lairs).
    BossArena,
    /// A Descent's gate chamber.
    GateSetPiece,
    /// The Heart's chamber.
    Heart,
    /// The Moon.
    Moon,
    /// A star system (its star).
    System,
    /// A planet (by theme).
    Planet,
    /// A moon or crater rock of a planet (by theme).
    SmallBody,
    /// A space station.
    Station,
    /// An asteroid belt.
    Belt,
    /// A comet.
    Comet,
    /// A nebula.
    Nebula,
    /// A black hole.
    BlackHole,
    /// A wormhole mouth.
    Wormhole,
    /// A system the Archive's star charts lead to.
    ArchiveSystem,
    /// A theme boss's lair (the nearest world of its theme).
    ThemeBossLair,
    /// An alien town on a settled world.
    AlienTown,
    /// A builders' outpost on an outland or exotic world.
    BuilderOutpost,
    /// Another player.
    Player,
    /// One of the player's bookmarks.
    Bookmark,
    /// A recent place on the player's back-stack.
    Recent,
}

/// The Atlas's categories (W14-C's side bar).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DestinationCategory {
    Depth,
    Places,
    Bosses,
    Settlements,
    Space,
    Players,
    Bookmarks,
    Recent,
}

impl DestinationKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 35] = [
        Self::Descent,
        Self::Stratum,
        Self::Depth,
        Self::HomeTown,
        Self::Village,
        Self::SkyIsland,
        Self::Tomb,
        Self::Cave,
        Self::Vault,
        Self::AnomalyRoom,
        Self::Landmark,
        Self::Settlement,
        Self::Outpost,
        Self::StoryLog,
        Self::Camp,
        Self::BossArena,
        Self::GateSetPiece,
        Self::Heart,
        Self::Moon,
        Self::System,
        Self::Planet,
        Self::SmallBody,
        Self::Station,
        Self::Belt,
        Self::Comet,
        Self::Nebula,
        Self::BlackHole,
        Self::Wormhole,
        Self::ArchiveSystem,
        Self::ThemeBossLair,
        Self::AlienTown,
        Self::BuilderOutpost,
        Self::Player,
        Self::Bookmark,
        Self::Recent,
    ];

    /// The kind's number (its index in [`Self::ALL`]; events carry it).
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// The kind of number `code`.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        if (code as usize) < Self::ALL.len() {
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// The kind as players read it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Descent => "Descent",
            Self::Stratum => "stratum",
            Self::Depth => "depth",
            Self::HomeTown => "home town",
            Self::Village => "village",
            Self::SkyIsland => "sky island",
            Self::Tomb => "tomb",
            Self::Cave => "cave",
            Self::Vault => "vault",
            Self::AnomalyRoom => "anomaly room",
            Self::Landmark => "landmark",
            Self::Settlement => "settlement",
            Self::Outpost => "outpost",
            Self::StoryLog => "story log",
            Self::Camp => "camp",
            Self::BossArena => "boss",
            Self::GateSetPiece => "gate",
            Self::Heart => "the Heart",
            Self::Moon => "the Moon",
            Self::System => "system",
            Self::Planet => "planet",
            Self::SmallBody => "moon",
            Self::Station => "station",
            Self::Belt => "belt",
            Self::Comet => "comet",
            Self::Nebula => "nebula",
            Self::BlackHole => "black hole",
            Self::Wormhole => "wormhole",
            Self::ArchiveSystem => "chart",
            Self::ThemeBossLair => "lair",
            Self::AlienTown => "alien town",
            Self::BuilderOutpost => "builder outpost",
            Self::Player => "player",
            Self::Bookmark => "bookmark",
            Self::Recent => "recent",
        }
    }

    /// The Atlas category the kind is listed under.
    #[must_use]
    pub const fn category(self) -> DestinationCategory {
        match self {
            Self::Descent | Self::Stratum | Self::Depth | Self::Heart => DestinationCategory::Depth,
            Self::HomeTown
            | Self::SkyIsland
            | Self::Tomb
            | Self::Cave
            | Self::Vault
            | Self::AnomalyRoom
            | Self::Landmark
            | Self::StoryLog
            | Self::GateSetPiece
            | Self::Moon => DestinationCategory::Places,
            Self::BossArena | Self::ThemeBossLair => DestinationCategory::Bosses,
            Self::Village
            | Self::Settlement
            | Self::Outpost
            | Self::Camp
            | Self::AlienTown
            | Self::BuilderOutpost => DestinationCategory::Settlements,
            Self::System
            | Self::Planet
            | Self::SmallBody
            | Self::Station
            | Self::Belt
            | Self::Comet
            | Self::Nebula
            | Self::BlackHole
            | Self::Wormhole
            | Self::ArchiveSystem => DestinationCategory::Space,
            Self::Player => DestinationCategory::Players,
            Self::Bookmark => DestinationCategory::Bookmarks,
            Self::Recent => DestinationCategory::Recent,
        }
    }

    /// Whether the kind lies in space (its candidates come from the space generator).
    #[must_use]
    pub const fn in_space(self) -> bool {
        matches!(self.category(), DestinationCategory::Space)
            || matches!(
                self,
                Self::ThemeBossLair | Self::AlienTown | Self::BuilderOutpost
            )
    }

    /// The kind a word names ("planet", "planets", "descent", "boss", ...), if any.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase();
        let word = word.as_str();
        Some(match word {
            "descent" | "descents" | "chapter" => Self::Descent,
            "stratum" | "strata" | "layer" => Self::Stratum,
            "depth" | "deep" => Self::Depth,
            "home" | "town" | "hometown" | "spawn" => Self::HomeTown,
            "village" | "villages" | "skytown" => Self::Village,
            "island" | "isle" | "sky" | "skyisland" => Self::SkyIsland,
            "tomb" | "tombs" | "crypt" | "crypts" => Self::Tomb,
            "cave" | "caves" | "cavern" => Self::Cave,
            "vault" | "vaults" => Self::Vault,
            "anomaly" | "anomalies" | "room" => Self::AnomalyRoom,
            "landmark" | "landmarks" | "poi" => Self::Landmark,
            "settlement" | "settlements" | "hall" => Self::Settlement,
            "outpost" | "outposts" => Self::Outpost,
            "log" | "logs" | "story" => Self::StoryLog,
            "camp" | "camps" => Self::Camp,
            "boss" | "bosses" | "arena" => Self::BossArena,
            "gate" | "gates" => Self::GateSetPiece,
            "heart" => Self::Heart,
            "moon" | "luna" => Self::Moon,
            "system" | "systems" | "star" => Self::System,
            "planet" | "planets" | "world" => Self::Planet,
            "moonlet" | "rock" | "smallbody" => Self::SmallBody,
            "station" | "stations" | "dock" => Self::Station,
            "belt" | "belts" | "asteroids" => Self::Belt,
            "comet" | "comets" => Self::Comet,
            "nebula" | "nebulae" => Self::Nebula,
            "blackhole" | "hole" => Self::BlackHole,
            "wormhole" | "wormholes" => Self::Wormhole,
            "chart" | "charts" | "archive" => Self::ArchiveSystem,
            "lair" | "lairs" => Self::ThemeBossLair,
            "alien" | "aliens" | "alientown" => Self::AlienTown,
            "builder" | "builders" => Self::BuilderOutpost,
            "player" | "players" => Self::Player,
            "bookmark" | "bookmarks" | "mark" => Self::Bookmark,
            "recent" | "back" | "history" => Self::Recent,
            _ => return None,
        })
    }
}

/// What narrows a kind's candidates.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DestinationFilter {
    /// Every candidate of the kind.
    #[default]
    Any,
    /// Only those in (or the top of) this Descent.
    Descent(Descent),
    /// Only those in (or the top of) this stratum.
    Stratum(Stratum),
    /// This many metres (rows) under the ground at the player's column ([`DestinationKind::Depth`]).
    Metres(i32),
    /// This absolute tile row ([`DestinationKind::Depth`]).
    Row(i32),
    /// Only bodies (and their towns, lairs and outposts) of this space theme.
    Theme(Theme),
    /// Only this player ([`DestinationKind::Player`]).
    Player(PlayerId),
}

/// Which candidate of a kind to pick.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Selector {
    /// The nearest (the player's own spot excepted).
    #[default]
    Nearest,
    /// The next one outward (see [`TravelCursor`]).
    Next,
    /// The previous one (inward; wraps to the farthest).
    Previous,
    /// A pseudo-random one, from this seed (the same seed picks the same candidate).
    Random(u32),
    /// Exactly this one (a name search's, a bookmark's or the Atlas's pick).
    Exact(DestinationId),
}

/// Arrival opt-outs.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ArrivalOptions {
    /// Arrive in the water: the pocket keeps open water and fluids round it.
    pub in_water: bool,
    /// Arrive inside a boss arena or lair (the boss and its guards stay), not at its door.
    pub in_arena: bool,
}

/// One travel request: "the next planet", "Descent V", "a random ice world".
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TravelRequest {
    pub kind: DestinationKind,
    pub filter: DestinationFilter,
    pub selector: Selector,
    pub arrival: ArrivalOptions,
}

impl TravelRequest {
    /// `selector` over every candidate of `kind`.
    #[must_use]
    pub const fn new(kind: DestinationKind, selector: Selector) -> Self {
        Self {
            kind,
            filter: DestinationFilter::Any,
            selector,
            arrival: ArrivalOptions {
                in_water: false,
                in_arena: false,
            },
        }
    }

    /// The nearest candidate of `kind`.
    #[must_use]
    pub const fn nearest(kind: DestinationKind) -> Self {
        Self::new(kind, Selector::Nearest)
    }

    /// The same request narrowed by `filter`.
    #[must_use]
    pub const fn filtered(mut self, filter: DestinationFilter) -> Self {
        self.filter = filter;
        self
    }

    /// The same request with arrival options.
    #[must_use]
    pub const fn arriving(mut self, arrival: ArrivalOptions) -> Self {
        self.arrival = arrival;
        self
    }

    /// Exactly `id`.
    #[must_use]
    pub const fn exact(id: DestinationId) -> Self {
        Self::new(id.kind, Selector::Exact(id))
    }

    /// The top of Descent `descent`.
    #[must_use]
    pub const fn descent(descent: Descent) -> Self {
        Self::nearest(DestinationKind::Descent).filtered(DestinationFilter::Descent(descent))
    }

    /// `metres` under the ground at the player's column.
    #[must_use]
    pub const fn depth_metres(metres: i32) -> Self {
        Self::nearest(DestinationKind::Depth).filtered(DestinationFilter::Metres(metres))
    }
}

/// A destination's stable identity: its kind and its target tile in its place (home tiles on
/// the home world and the Moon, strip tiles on a body, space tiles in free flight).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DestinationId {
    pub kind: DestinationKind,
    pub place: Place,
    pub x: i64,
    pub y: i64,
}

impl DestinationId {
    #[must_use]
    pub const fn new(kind: DestinationKind, place: Place, x: i64, y: i64) -> Self {
        Self { kind, place, x, y }
    }

    /// The same spot as another kind (bookmarks and the back-stack keep where, not what).
    #[must_use]
    pub const fn as_kind(self, kind: DestinationKind) -> Self {
        Self { kind, ..self }
    }
}

/// The info card of a destination (the Atlas's preview card, W14-C).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreviewInfo {
    /// Where it is, as a line ("Home world", "Kessa II b", "free space").
    pub place: String,
    /// Metres under the ground (home world and bodies), if under it.
    pub depth_m: Option<i32>,
    /// Its target tile (see [`DestinationId`]).
    pub coords: (i64, i64),
    pub descent: Option<Descent>,
    pub stratum: Option<Stratum>,
    /// A body's space theme (planets, their towns, lairs and outposts).
    pub theme: Option<Theme>,
    /// The system it is in (space kinds).
    pub system: Option<String>,
    /// Surface gravity there, milli-g.
    pub gravity_milli_g: Option<u32>,
    /// What can hurt there (the stratum's hazards, a body's air and heat, a star's corona).
    pub hazards: Vec<String>,
    /// Who lives there (a village's folk, an alien species, a stratum's creatures).
    pub residents: Vec<String>,
    /// The boss that waits there, if any.
    pub boss: Option<String>,
    /// How far from the player (tiles in the same place; space tiles between places), when
    /// known.
    pub distance: Option<u64>,
}

/// One candidate destination: its id, its name and its info card.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Destination {
    pub id: DestinationId,
    pub name: String,
    pub info: PreviewInfo,
    /// Where "in the arena" arrives (a boss arena's floor, a lair's middle), when the kind
    /// has an inside apart from its door.
    pub inside: Option<(i64, i64)>,
}

impl Destination {
    /// A destination with only a name and an id (gatherers fill the card).
    #[must_use]
    pub fn new(id: DestinationId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            info: PreviewInfo {
                coords: (id.x, id.y),
                ..PreviewInfo::default()
            },
            inside: None,
        }
    }

    /// Whether every word of `query` (lower case) appears in its name, kind, theme, Descent
    /// or stratum.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let haystack = self.haystack();
        query
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .all(|word| haystack.contains(&word))
    }

    fn haystack(&self) -> String {
        let mut text = format!("{} {}", self.name, self.id.kind.name());
        if let Some(theme) = self.info.theme {
            text.push(' ');
            text.push_str(theme.name());
        }
        if let Some(descent) = self.info.descent {
            let _ = write!(
                text,
                " descent {} descent {} {}",
                descent.numeral(),
                descent.index() + 1,
                descent.name()
            );
        }
        if let Some(stratum) = self.info.stratum {
            text.push(' ');
            text.push_str(stratum.name());
        }
        if let Some(boss) = &self.info.boss {
            text.push(' ');
            text.push_str(boss);
        }
        if let Some(system) = &self.info.system {
            text.push(' ');
            text.push_str(system);
        }
        for resident in &self.info.residents {
            text.push(' ');
            text.push_str(resident);
        }
        text.to_ascii_lowercase()
    }
}

/// Where a next / previous cycle began and how far it has gone (one per player).
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelCursor {
    pub kind: DestinationKind,
    pub filter: DestinationFilter,
    /// Where the player stood when the cycle began (the candidates are ordered from here).
    pub origin: DestinationId,
    /// The candidate last picked (its index in that order).
    pub index: u16,
}

/// One player's bookmarks, back-stack and cycle.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlayerTravel {
    /// Saved places, oldest first.
    pub bookmarks: Vec<DestinationId>,
    /// Where the player was before each travel, oldest first (the last is "back").
    pub recent: Vec<DestinationId>,
    /// The next / previous cycle under way.
    #[serde(default)]
    pub cursor: Option<TravelCursor>,
    /// Travels made (deterministic random picks mix it in when asked).
    #[serde(default)]
    pub travels: u32,
}

impl PlayerTravel {
    /// Whether nothing is recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
            && self.recent.is_empty()
            && self.cursor.is_none()
            && self.travels == 0
    }

    /// Adds a bookmark (a repeat is moved to the end); the oldest goes past the limit.
    pub fn bookmark(&mut self, id: DestinationId) {
        let id = id.as_kind(DestinationKind::Bookmark);
        self.bookmarks.retain(|kept| *kept != id);
        self.bookmarks.push(id);
        if self.bookmarks.len() > MAX_BOOKMARKS {
            self.bookmarks.remove(0);
        }
    }

    /// Pushes `id` on the back-stack (a repeat of the top is dropped).
    pub fn push_recent(&mut self, id: DestinationId) {
        let id = id.as_kind(DestinationKind::Recent);
        if self.recent.last() == Some(&id) {
            return;
        }
        self.recent.push(id);
        if self.recent.len() > MAX_RECENT {
            self.recent.remove(0);
        }
    }
}

/// Every player's travel records (the world field `travel`, the shared `SectionKey::Travel`
/// section).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TravelLog {
    pub players: BTreeMap<PlayerId, PlayerTravel>,
}

impl TravelLog {
    /// Whether no player has recorded anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.players.values().all(PlayerTravel::is_empty)
    }

    /// `player`'s records (empty when none).
    #[must_use]
    pub fn of(&self, player: PlayerId) -> PlayerTravel {
        self.players.get(&player).cloned().unwrap_or_default()
    }

    /// `player`'s records to change.
    pub fn of_mut(&mut self, player: PlayerId) -> &mut PlayerTravel {
        self.players.entry(player).or_default()
    }

    /// Forgets a player who left the world.
    pub fn remove(&mut self, player: PlayerId) {
        self.players.remove(&player);
    }
}

/// What travel reports (netcode `1140 + code`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum TravelEvent {
    /// The rig arrived at a destination (`detail`: the kind's code).
    Arrived,
    /// The rig went back to its last place (`detail`: places left on the stack).
    WentBack,
    /// A bookmark was saved (`detail`: bookmarks kept).
    Bookmarked,
    /// A bookmark was removed (`detail`: bookmarks kept).
    BookmarkRemoved,
    /// Nothing of that kind could be found (`detail`: the kind's code).
    NotFound,
}

impl TravelEvent {
    /// Its code (netcode adds 1140; at most 19).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Arrived => 0,
            Self::WentBack => 1,
            Self::Bookmarked => 2,
            Self::BookmarkRemoved => 3,
            Self::NotFound => 4,
        }
    }
}

/// The squared distance between two tiles (exact, any range).
#[must_use]
pub const fn distance2(a: (i64, i64), b: (i64, i64)) -> i128 {
    let dx = (a.0 - b.0) as i128;
    let dy = (a.1 - b.1) as i128;
    dx * dx + dy * dy
}

/// The integer square root of `value` (floor; exact, no floating point).
#[must_use]
pub const fn isqrt(value: i128) -> u64 {
    if value <= 0 {
        return 0;
    }
    let root = value.cast_unsigned().isqrt();
    if root > u64::MAX as u128 {
        u64::MAX
    } else {
        #[allow(clippy::cast_possible_truncation, reason = "checked above")]
        let root = root as u64;
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_their_codes_and_parse_their_names() {
        for (index, kind) in DestinationKind::ALL.iter().enumerate() {
            assert_eq!(kind.code() as usize, index);
            assert_eq!(DestinationKind::from_code(kind.code()), Some(*kind));
            let word = kind.name().replace("the ", "").replace(' ', "");
            assert!(
                DestinationKind::parse(&word).is_some(),
                "{word} parses to some kind"
            );
        }
        assert_eq!(
            DestinationKind::parse("planets"),
            Some(DestinationKind::Planet)
        );
        assert_eq!(DestinationKind::parse("nope"), None);
        assert!(TravelEvent::NotFound.code() < 20);
    }

    #[test]
    fn bookmarks_and_the_back_stack_are_bounded_and_ordered() {
        let mut travel = PlayerTravel::default();
        for x in 0..40 {
            travel.bookmark(DestinationId::new(
                DestinationKind::Planet,
                Place::Space,
                x,
                0,
            ));
            travel.push_recent(DestinationId::new(DestinationKind::Cave, Place::HOME, x, 0));
            travel.push_recent(DestinationId::new(DestinationKind::Cave, Place::HOME, x, 0));
        }
        assert_eq!(travel.bookmarks.len(), MAX_BOOKMARKS);
        assert_eq!(travel.recent.len(), MAX_RECENT);
        assert_eq!(travel.recent.last().map(|id| id.x), Some(39));
        assert!(
            travel
                .bookmarks
                .iter()
                .all(|id| id.kind == DestinationKind::Bookmark)
        );
    }

    #[test]
    fn integer_distances_are_exact() {
        assert_eq!(isqrt(distance2((0, 0), (3, 4))), 5);
        assert_eq!(isqrt(distance2((0, 0), (1_000_000_000, 0))), 1_000_000_000);
        assert_eq!(isqrt(0), 0);
    }
}
