//! The Atlas's destinations: every place it can travel to, as the simulation's destination
//! index answers them (`fatherload_simulation::destination`, W14-B), never by streaming the
//! world in.
//!
//! Each [`Destination`] is the index's candidate (`WorldState::destinations`, per index kind
//! of a category; `WorldState::search_destinations` for the search box) dressed for the
//! screen: where the preview looks ([`AtlasTarget`], from the candidate's id), the info card
//! ([`Fact`]s from its `PreviewInfo`, the boss and the creatures in their own sprites), and the
//! one [`TravelRequest`] Go sends: `TestModeAction::TravelTo(TravelRequest::exact(id))`, so the
//! preview shows exactly where the index lands (safe arrival, from anywhere). Map spots are
//! exact ids too; only the galaxy story's goal keeps its own warp (`WarpToStory`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "distances and preview reaches are shown, never simulated"
)]

use std::fmt::Write as _;

use fatherload_simulation::{
    PlayerCommand, PlayerId, TestModeAction, WorldState,
    descent::{DescentBoss, Stratum},
    destination::{
        self as index_model, DestinationCategory, DestinationFilter, DestinationId,
        DestinationKind as IndexKind, TravelRequest,
    },
    enemy::EnemyKind,
    place::{BodyId, Place, SpaceBodyKey},
    position::WorldPos,
    space::{self, Space, SpaceTile},
    terrain::TilePosition,
    worldgen::{HazardKind, descent as gen_descent},
};

/// The Atlas's categories, down its side.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AtlasCategory {
    /// Descents I-VIII, their strata and exact depths.
    #[default]
    Depth,
    /// The home town, villages, sky towns, the Moon, outposts, tombs and story sites.
    Places,
    /// Gate bosses, theme bosses and faction bosses.
    Bosses,
    /// Deep settlements, villages and alien towns.
    Settlements,
    /// Star systems, planets by theme, stations, belts, comets, nebulae, black holes and
    /// wormholes.
    Space,
    /// The other players.
    Players,
    /// The seat's bookmarks.
    Bookmarks,
    /// Where the seat travelled from, newest first (the back-stack).
    Recent,
}

impl AtlasCategory {
    pub const ALL: [Self; 8] = [
        Self::Depth,
        Self::Places,
        Self::Bosses,
        Self::Settlements,
        Self::Space,
        Self::Players,
        Self::Bookmarks,
        Self::Recent,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Depth => "Depth",
            Self::Places => "Places",
            Self::Bosses => "Bosses",
            Self::Settlements => "Settlements",
            Self::Space => "Space",
            Self::Players => "Players",
            Self::Bookmarks => "Bookmarks",
            Self::Recent => "Recent",
        }
    }

    /// A shorter title for narrow tabs.
    #[must_use]
    pub const fn short_title(self) -> &'static str {
        match self {
            Self::Depth => "Depth",
            Self::Places => "Places",
            Self::Bosses => "Bosses",
            Self::Settlements => "Towns",
            Self::Space => "Space",
            Self::Players => "Crew",
            Self::Bookmarks => "Marks",
            Self::Recent => "Recent",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or_default()
    }

    /// The category `step` places along the side (wrapping).
    #[must_use]
    pub fn step(self, step: isize) -> Self {
        let count = Self::ALL.len().cast_signed();
        let index = (self.index().cast_signed() + step).rem_euclid(count);
        Self::ALL[index.cast_unsigned()]
    }

    /// What the list says when the category is empty.
    #[must_use]
    pub const fn empty_text(self) -> &'static str {
        match self {
            Self::Bookmarks => "No bookmarks yet: pick a place and press Bookmark.",
            Self::Recent => "Nowhere yet: places you travel from appear here.",
            Self::Players => "No one else is in this world.",
            Self::Bosses | Self::Settlements | Self::Places => "Nothing found in this world.",
            Self::Depth => "This world has no depths.",
            Self::Space => "This world has no space.",
        }
    }
}

impl AtlasCategory {
    /// The Atlas category of the index's category (one to one).
    #[must_use]
    pub const fn of(category: DestinationCategory) -> Self {
        match category {
            DestinationCategory::Depth => Self::Depth,
            DestinationCategory::Places => Self::Places,
            DestinationCategory::Bosses => Self::Bosses,
            DestinationCategory::Settlements => Self::Settlements,
            DestinationCategory::Space => Self::Space,
            DestinationCategory::Players => Self::Players,
            DestinationCategory::Bookmarks => Self::Bookmarks,
            DestinationCategory::Recent => Self::Recent,
        }
    }

    /// The index's kinds listed under this category, in the index's order.
    #[must_use]
    pub fn kinds(self) -> Vec<IndexKind> {
        IndexKind::ALL
            .into_iter()
            .filter(|kind| Self::of(kind.category()) == self)
            .collect()
    }
}

/// What a destination is (the "kind" Previous/Next cycle through): a kind of the destination
/// index, or one of the Atlas's own picks.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DestinationKind {
    /// A candidate of the destination index.
    Index(IndexKind),
    /// A spot picked on the depth map.
    DepthSpot,
    /// A spot picked on the galaxy map.
    SpaceSpot,
    /// Where the galaxy story's current goal is done.
    StorySite,
}

impl DestinationKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Index(kind) => kind.name(),
            Self::DepthSpot => "depth",
            Self::SpaceSpot => "space",
            Self::StorySite => "story",
        }
    }

    /// The Atlas category a kind is filed under.
    #[must_use]
    pub const fn category(self) -> AtlasCategory {
        match self {
            Self::Index(kind) => AtlasCategory::of(kind.category()),
            Self::DepthSpot => AtlasCategory::Depth,
            Self::SpaceSpot => AtlasCategory::Space,
            Self::StorySite => AtlasCategory::Places,
        }
    }
}

/// Where a destination is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AtlasTarget {
    /// A tile of the home world (its sky and the Moon's land included).
    Home(TilePosition),
    /// The surface of a generated body, over `column` of its strip.
    Body { body: SpaceBodyKey, column: i64 },
    /// Free flight at `pos`; `reach` is how many tiles round it the preview shows.
    Space { pos: WorldPos, reach: f32 },
    /// Where the galaxy story's current goal is done.
    Story,
}

impl AtlasTarget {
    /// A short key for caches (two targets with the same key show the same preview).
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Home(tile) => format!("home:{}:{}", tile.x, tile.y),
            Self::Body { body, column } => format!(
                "body:{}:{}:{}:{}:{column}",
                body.sector.x, body.sector.y, body.system, body.body
            ),
            Self::Space { pos, reach } => {
                let (x, y) = pos.tile();
                format!("space:{x}:{y}:{reach:.0}")
            }
            Self::Story => "story".to_owned(),
        }
    }
}

/// One line of the info card.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fact {
    pub label: &'static str,
    pub value: String,
}

impl Fact {
    fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
        }
    }
}

/// A place the Atlas can travel to.
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub kind: DestinationKind,
    pub category: AtlasCategory,
    pub name: String,
    /// One line under the name (the Descent and stratum, the theme, the system).
    pub subtitle: String,
    pub target: AtlasTarget,
    /// The info card: depth or coordinates, gravity, hazards, residents.
    pub facts: Vec<Fact>,
    /// The boss that rules the place (drawn on the preview and the card).
    pub boss: Option<EnemyKind>,
    /// Creatures that roam it (small sprites on the card).
    pub creatures: Vec<EnemyKind>,
    /// Lower-case text the search matches against.
    pub keywords: String,
    /// The destination index's request Go sends (`None`: the story's goal, whose warp is its
    /// own).
    pub request: Option<TravelRequest>,
}

impl Destination {
    fn new(
        kind: DestinationKind,
        name: impl Into<String>,
        subtitle: impl Into<String>,
        target: AtlasTarget,
    ) -> Self {
        let mut destination = Self {
            kind,
            category: kind.category(),
            name: name.into(),
            subtitle: subtitle.into(),
            target,
            facts: Vec::new(),
            boss: None,
            creatures: Vec::new(),
            keywords: String::new(),
            request: None,
        };
        destination.refresh_keywords("");
        destination
    }

    /// Rebuilds the search text from the name, subtitle, kind and category plus `extra`.
    fn refresh_keywords(&mut self, extra: &str) {
        self.keywords = format!(
            "{} {} {} {} {extra}",
            self.name,
            self.subtitle,
            self.kind.name(),
            self.category.title()
        )
        .to_lowercase();
    }

    fn with_keywords(mut self, extra: &str) -> Self {
        let more = format!("{} {extra}", self.keywords);
        self.keywords = more.to_lowercase();
        self
    }

    fn fact(mut self, label: &'static str, value: impl Into<String>) -> Self {
        self.facts.push(Fact::new(label, value));
        self
    }

    /// The same place filed under `category` (bookmarks and the recent list).
    #[must_use]
    pub fn filed_under(&self, category: AtlasCategory) -> Self {
        Self {
            category,
            ..self.clone()
        }
    }

    /// The index id Go travels to (`None`: the story's goal).
    #[must_use]
    pub fn id(&self) -> Option<DestinationId> {
        match self.request?.selector {
            index_model::Selector::Exact(id) => Some(id),
            _ => None,
        }
    }
}

/// Where the seat is now, for distances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Origin {
    pub place: Place,
    /// Its tile on the home world (`None` away from it).
    pub tile: Option<TilePosition>,
    /// Its position in space (the home world's own place in space while at home).
    pub space: WorldPos,
}

impl Origin {
    /// `player`'s place in `world`.
    #[must_use]
    pub fn of(world: &WorldState, player: PlayerId) -> Option<Self> {
        let rig = world.player(player)?;
        let place = world.place_of(player);
        let galaxy = Space::shared(world.world_seed());
        let home = galaxy
            .body(space::HOME_WORLD_BODY)
            .map_or_else(WorldPos::default, |body| body.centre.pos());
        let space = match place {
            Place::Space => rig.flight.pos,
            Place::OnBody(BodyId::Space(key)) => galaxy
                .body(key)
                .map_or(rig.flight.pos, |body| body.centre.pos()),
            Place::OnBody(_) => home,
        };
        Some(Self {
            place,
            tile: place.is_home_store().then(|| rig.tile_in(world.frame())),
            space,
        })
    }
}

/// How far `target` is from `origin`, for the card ("1,240 m below", "3.1 sectors").
#[must_use]
pub fn distance_text(world: &WorldState, target: &AtlasTarget, origin: &Origin) -> String {
    match *target {
        AtlasTarget::Home(tile) => origin.tile.map_or_else(
            || "on the home world".to_owned(),
            |from| {
                let (dx, dy) = (tile.x - from.x, tile.y - from.y);
                let vertical = match dy.cmp(&0) {
                    std::cmp::Ordering::Greater => format!("{} m below", thousands(dy.into())),
                    std::cmp::Ordering::Less => format!("{} m above", thousands((-dy).into())),
                    std::cmp::Ordering::Equal => "level".to_owned(),
                };
                if dx.abs() < 2 {
                    vertical
                } else {
                    format!(
                        "{vertical}, {} m {}",
                        thousands(dx.abs().into()),
                        if dx > 0 { "east" } else { "west" }
                    )
                }
            },
        ),
        AtlasTarget::Body { body, .. } => {
            let centre = Space::shared(world.world_seed())
                .body(body)
                .map_or(origin.space, |body| body.centre.pos());
            space_distance(origin.space, centre)
        }
        AtlasTarget::Space { pos, .. } => space_distance(origin.space, pos),
        AtlasTarget::Story => "where the road leads".to_owned(),
    }
}

fn space_distance(from: WorldPos, to: WorldPos) -> String {
    let (dx, dy) = from.delta_f64(to);
    let tiles = dx.hypot(dy) / f64::from(fatherload_simulation::TILE_SIZE);
    let sectors = tiles / space::SPACE_SECTOR_TILES as f64;
    if tiles < 1.0 {
        "here".to_owned()
    } else if sectors >= 0.1 {
        format!("{sectors:.1} sectors")
    } else {
        let whole = tiles.round() as i64;
        format!("{} tiles", thousands(whole))
    }
}

/// `value` with thousands separators.
#[must_use]
pub fn thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0 { format!("-{out}") } else { out }
}

/// How many tiles round a space spot of `kind` its preview shows.
const fn space_reach(kind: IndexKind) -> f32 {
    match kind {
        IndexKind::System | IndexKind::ArchiveSystem | IndexKind::Nebula => 14_000.0,
        IndexKind::BlackHole => 12_000.0,
        IndexKind::Belt => 9_000.0,
        IndexKind::FactionBossLair => 4_500.0,
        IndexKind::Planet | IndexKind::SmallBody => 6_000.0,
        IndexKind::Comet | IndexKind::Wormhole => 3_000.0,
        IndexKind::Station => 1_500.0,
        _ => 900.0,
    }
}

/// The strip column the body's own arrival point lands on: its first alien town on a settled
/// world, else column 0 (`WorldState::warp_to_place`, which the index's arrival-point ids use).
fn arrival_point_column(world: &WorldState, key: SpaceBodyKey) -> i64 {
    fatherload_simulation::place::planet_params_for(world.world_seed(), key)
        .and_then(|params| {
            fatherload_simulation::alien::town::towns_of(&params)
                .first()
                .map(|town| i64::from(town.site.centre))
        })
        .unwrap_or(0)
}

/// What the preview draws for the index's id: its home tile, its body column, its space spot.
#[must_use]
pub fn target_of(world: &WorldState, id: &DestinationId) -> AtlasTarget {
    let tile =
        |value: i64| i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX });
    match id.place {
        Place::OnBody(BodyId::Home | BodyId::Moon) => AtlasTarget::Home(TilePosition {
            x: tile(id.x),
            y: tile(id.y),
        }),
        Place::OnBody(BodyId::Space(body)) => AtlasTarget::Body {
            body,
            column: if id.x == index_model::space::ARRIVAL_POINT {
                arrival_point_column(world, body)
            } else {
                id.x
            },
        },
        Place::Space => {
            let spot = SpaceTile::new(id.x, id.y);
            // A star system's spot is a safe distance off its star: the preview shows the
            // system round its star (Go still stops at the spot).
            if matches!(id.kind, IndexKind::System | IndexKind::ArchiveSystem) {
                let galaxy = Space::shared(world.world_seed());
                if let Some(system) = galaxy
                    .systems_near(spot, space::SPACE_SECTOR_TILES)
                    .first()
                    .and_then(|(reference, _)| galaxy.system(*reference))
                {
                    return AtlasTarget::Space {
                        pos: system.centre.pos(),
                        reach: system.extent as f32 * 0.6,
                    };
                }
            }
            AtlasTarget::Space {
                pos: spot.pos(),
                reach: space_reach(id.kind),
            }
        }
    }
}

/// The command that takes `player`'s rig to `destination`: the destination index's
/// `TravelTo` (the story's goal: its own warp). `None` for the player's own spot.
#[must_use]
pub fn travel_command(
    world: &WorldState,
    destination: &Destination,
    player: PlayerId,
) -> Option<PlayerCommand> {
    let action = match destination.request {
        Some(request) => {
            if let (Some(id), Some(here)) = (destination.id(), world.travel_here(player))
                && id.kind == IndexKind::Player
                && id.as_kind(IndexKind::Recent) == here
            {
                return None;
            }
            TestModeAction::TravelTo(request)
        }
        None if destination.target == AtlasTarget::Story => TestModeAction::WarpToStory,
        None => return None,
    };
    Some(PlayerCommand::TestMode(action))
}

/// The creature a gate's boss fight is drawn as (the set pieces have none).
#[must_use]
pub const fn gate_boss_creature(boss: DescentBoss) -> Option<EnemyKind> {
    match boss {
        DescentBoss::TheSinkhole | DescentBoss::StarCore | DescentBoss::TheHeart => None,
        DescentBoss::MycelialMind => Some(EnemyKind::MycelialMind),
        DescentBoss::Leviathan => Some(EnemyKind::Leviathan),
        DescentBoss::SunSerpent => Some(EnemyKind::SunSerpent),
        DescentBoss::TheGlitch => Some(EnemyKind::TheGlitch),
        DescentBoss::GatekeeperColossus => Some(EnemyKind::GatekeeperColossus),
    }
}

/// `name` folded for comparing names ("The Leviathan" and "leviathan" agree).
fn folded(name: &str) -> String {
    name.trim_start_matches("The ")
        .trim_start_matches("the ")
        .to_lowercase()
}

/// The creature drawn for a boss or resident named `name` on an index card (a gate boss, a
/// theme boss, a stratum's creature), if one has a sprite.
#[must_use]
pub fn creature_named(name: &str) -> Option<EnemyKind> {
    if let Some(boss) = DescentBoss::ALL
        .into_iter()
        .find(|boss| boss.name() == name)
    {
        return gate_boss_creature(boss);
    }
    let wanted = folded(name);
    EnemyKind::ALL
        .into_iter()
        .find(|kind| folded(kind.name()) == wanted)
}

const fn hazard_name(hazard: HazardKind) -> &'static str {
    match hazard {
        HazardKind::Gas => "gas",
        HazardKind::Lava => "lava",
        HazardKind::MagmaVent => "magma vents",
        HazardKind::Pressure => "pressure",
        HazardKind::Explosive => "unstable rock",
    }
}

fn depth_line(row: i32) -> String {
    format!(
        "{} m (row {})",
        thousands(row.into()),
        thousands(row.into())
    )
}

fn with_stratum_facts(mut destination: Destination, stratum: Stratum) -> Destination {
    let def = stratum.def();
    destination = destination.fact(
        "Stratum",
        format!(
            "{} (Descent {})",
            stratum.name(),
            stratum.descent().numeral()
        ),
    );
    let hazards: Vec<&str> = def
        .hazards
        .iter()
        .filter(|(_, weight)| *weight > 0)
        .map(|(hazard, _)| hazard_name(*hazard))
        .collect();
    if !hazards.is_empty() {
        destination = destination.fact("Hazards", hazards.join(", "));
    }
    destination = destination.fact("Rule", def.signature);
    destination.creatures = def.creatures.iter().map(|(kind, _)| *kind).collect();
    destination
}

/// "1 planet", "4 planets".
#[must_use]
pub fn planets_text(count: usize) -> String {
    if count == 1 {
        "1 planet".to_owned()
    } else {
        format!("{count} planets")
    }
}

fn gravity_text(milli_g: u32) -> String {
    format!("{:.2} g", milli_g as f32 / 1_000.0)
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The card line under the name: the Descent and stratum, or the theme and system, or where
/// (never the name again: a Descent names where it opens, a stratum its Descent).
fn subtitle_of(found: &index_model::Destination) -> String {
    let info = &found.info;
    match (found.id.kind, info.stratum) {
        (IndexKind::Descent, Some(stratum)) => {
            return format!(
                "{} strata, opening in {}",
                stratum.descent().strata().count(),
                stratum.name()
            );
        }
        (IndexKind::Stratum, Some(stratum)) => {
            return format!(
                "Descent {} - {}",
                stratum.descent().numeral(),
                stratum.descent().name()
            );
        }
        _ => {}
    }
    if let Some(stratum) = info.stratum {
        return format!(
            "Descent {} - {}",
            stratum.descent().numeral(),
            stratum.name()
        );
    }
    if let Some(descent) = info.descent {
        return format!("Descent {} - {}", descent.numeral(), descent.name());
    }
    match (info.theme, &info.system) {
        (Some(theme), Some(system)) => format!("{} - {system}", capitalised(theme.name())),
        (Some(theme), None) => capitalised(theme.name()),
        (None, Some(system)) if info.place.is_empty() => format!("{system} system"),
        _ => capitalised(&info.place),
    }
}

/// The index's candidate as the Atlas shows it.
#[must_use]
pub fn from_index(world: &WorldState, found: &index_model::Destination) -> Destination {
    let info = &found.info;
    let kind = DestinationKind::Index(found.id.kind);
    let name = match found.id.kind {
        // The index numbers the log's entries ("recent 1", "bookmark 2").
        IndexKind::Recent | IndexKind::Bookmark => capitalised(&found.name),
        _ => found.name.clone(),
    };
    let mut destination =
        Destination::new(kind, name, subtitle_of(found), target_of(world, &found.id));
    destination.request = Some(TravelRequest::exact(found.id));
    if let Some(metres) = info.depth_m {
        destination = destination.fact("Depth", format!("{} m", thousands(metres.into())));
    }
    if let Place::OnBody(BodyId::Space(_)) = found.id.place {
        if !info.place.is_empty() {
            destination = destination.fact("World", info.place.clone());
        }
    } else {
        destination = destination.fact(
            "Coordinates",
            format!("{}, {}", thousands(found.id.x), thousands(found.id.y)),
        );
    }
    if let Some(stratum) = info.stratum {
        destination = with_stratum_facts(destination, stratum);
    }
    if let Some(theme) = info.theme {
        destination = destination.fact("Theme", theme.name());
    }
    if let Some(gravity) = info.gravity_milli_g {
        destination = destination.fact("Gravity", gravity_text(gravity));
    }
    if !info.hazards.is_empty() && info.stratum.is_none() {
        destination = destination.fact("Hazards", info.hazards.join(", "));
    }
    if !info.residents.is_empty() {
        let shown: Vec<&str> = info.residents.iter().take(4).map(String::as_str).collect();
        destination = destination.fact("Residents", shown.join(", "));
        if destination.creatures.is_empty() {
            destination.creatures = info
                .residents
                .iter()
                .filter_map(|name| creature_named(name))
                .collect();
        }
    }
    if let Some(boss) = &info.boss {
        destination = destination.fact("Boss", boss.clone());
        destination.boss = creature_named(boss);
    }
    let mut extra = String::new();
    if let Some(descent) = info.descent.or_else(|| info.stratum.map(Stratum::descent)) {
        let number = descent.index() + 1;
        let _ = write!(
            extra,
            " descent {number} d{number} {} {}",
            descent.numeral(),
            descent.name()
        );
    }
    for word in [
        info.theme.map(|theme| theme.name().to_owned()),
        info.system.clone(),
        info.boss.clone(),
        Some(info.place.clone()),
    ]
    .into_iter()
    .flatten()
    {
        extra.push(' ');
        extra.push_str(&word);
    }
    for resident in &info.residents {
        extra.push(' ');
        extra.push_str(resident);
    }
    destination.with_keywords(&extra)
}

/// Candidates of one kind a category lists (nearest first).
pub const PER_KIND: usize = 12;

/// The galaxy story's current goal (its own warp, not an index place).
fn story_destination() -> Destination {
    Destination::new(
        DestinationKind::StorySite,
        "The road's current goal",
        "Where the galaxy story leads next",
        AtlasTarget::Story,
    )
    .with_keywords("galaxy story quest road relic")
}

/// Every destination of `category` in `world`, seen from `player`: the destination index's
/// candidates of the category's kinds (nearest first within each kind; Depth's Descents and
/// strata top to bottom), the bookmarks and back-stack the world keeps for the player, and
/// the story's goal under Places once space is open.
#[must_use]
pub fn destinations(
    world: &WorldState,
    player: Option<PlayerId>,
    category: AtlasCategory,
) -> Vec<Destination> {
    let Some(player) = player else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kind in category.kinds() {
        let mut found = world.destinations(player, kind, DestinationFilter::Any);
        if matches!(
            kind,
            IndexKind::Descent | IndexKind::Stratum | IndexKind::Depth
        ) {
            found.sort_by_key(|candidate| (candidate.id.y, candidate.id.x));
        }
        // The nearest few of each kind keep a category varied (64 sky islands would bury the
        // tombs); the Descents and strata are listed whole, and the search reaches the rest.
        let shown = if matches!(kind, IndexKind::Descent | IndexKind::Stratum) {
            found.len()
        } else {
            PER_KIND
        };
        out.extend(
            found
                .iter()
                .take(shown)
                .map(|candidate| from_index(world, candidate).filed_under(category)),
        );
    }
    if category == AtlasCategory::Places && world.space_enabled() {
        out.push(story_destination());
    }
    out
}

/// Every destination whose card answers `query` ("planet ice", "leviathan", "descent 5"):
/// the index's own search, nearest first within each kind.
#[must_use]
pub fn search(world: &WorldState, player: Option<PlayerId>, query: &str) -> Vec<Destination> {
    let Some(player) = player else {
        return Vec::new();
    };
    world
        .search_destinations(player, query, index_model::MAX_CANDIDATES)
        .iter()
        .map(|candidate| from_index(world, candidate))
        .collect()
}

/// A depth on the home world picked on the depth map: Go travels to exactly that tile (the
/// index's safe arrival settles it onto the nearest floor and carves a pocket).
#[must_use]
pub fn depth_spot(world: &WorldState, tile: TilePosition) -> Destination {
    let settings = *world.world_settings();
    let surface = world.terrain().surface_row(tile.x);
    let (name, subtitle) = if settings.generator.has_descents() && tile.y > surface {
        let stratum = gen_descent::stratum_at(&settings, tile.x, tile.y);
        (
            format!("{} m in {}", thousands(tile.y.into()), stratum.name()),
            format!(
                "Descent {} - {}",
                stratum.descent().numeral(),
                stratum.descent().name()
            ),
        )
    } else if tile.y <= surface {
        ("The surface".to_owned(), "Open sky".to_owned())
    } else {
        (
            format!("{} m down", thousands(tile.y.into())),
            world.terrain().band_at(tile).name().to_owned(),
        )
    };
    let mut destination = Destination::new(
        DestinationKind::DepthSpot,
        name,
        subtitle,
        AtlasTarget::Home(tile),
    )
    .fact("Depth", depth_line(tile.y))
    .fact("Column", thousands(tile.x.into()));
    if settings.generator.has_descents() && tile.y > surface {
        let stratum = gen_descent::stratum_at(&settings, tile.x, tile.y);
        destination = with_stratum_facts(destination, stratum);
    }
    destination.request = Some(TravelRequest::exact(DestinationId::new(
        IndexKind::Depth,
        Place::HOME,
        i64::from(tile.x),
        i64::from(tile.y),
    )));
    destination
}

/// A free-flight spot picked on the galaxy map, with the nearest thing named: Go stops there.
#[must_use]
pub fn space_spot(world: &WorldState, pos: WorldPos) -> Destination {
    let galaxy = Space::shared(world.world_seed());
    let near = galaxy.systems_near(SpaceTile::of_pos(pos), space::SPACE_SECTOR_TILES);
    let nearest = near
        .first()
        .and_then(|(reference, distance)| Some((galaxy.system(*reference)?, *distance)));
    let subtitle = nearest.as_ref().map_or_else(
        || "Deep space".to_owned(),
        |(system, distance)| {
            format!(
                "{} tiles from {}",
                thousands(distance.round() as i64),
                system.name
            )
        },
    );
    let (x, y) = pos.tile();
    let mut destination = Destination::new(
        DestinationKind::SpaceSpot,
        "Open space",
        subtitle,
        AtlasTarget::Space {
            pos,
            reach: 6_000.0,
        },
    )
    .fact("Coordinates", format!("{}, {}", thousands(x), thousands(y)));
    // An open-space spot is no listed candidate: an exact id of the back-stack's kind (a bare
    // spot) travels to the tile itself.
    destination.request = Some(TravelRequest::exact(DestinationId::new(
        IndexKind::Recent,
        Place::Space,
        x,
        y,
    )));
    destination
}

/// The index's candidate for the star system `name` near `player` (the galaxy map snaps to
/// it: Go stops a safe distance off its star, as the list's does).
#[must_use]
pub fn system_named(
    world: &WorldState,
    player: Option<PlayerId>,
    name: &str,
) -> Option<Destination> {
    let player = player?;
    world
        .destinations(player, IndexKind::System, DestinationFilter::Any)
        .iter()
        .find(|candidate| candidate.info.system.as_deref() == Some(name) || candidate.name == name)
        .map(|candidate| from_index(world, candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(-12_345_678), "-12,345,678");
    }

    #[test]
    fn categories_step_round_the_side() {
        assert_eq!(AtlasCategory::Depth.step(-1), AtlasCategory::Recent);
        assert_eq!(AtlasCategory::Recent.step(1), AtlasCategory::Depth);
        assert_eq!(AtlasCategory::Places.step(2), AtlasCategory::Settlements);
    }

    #[test]
    fn every_index_kind_has_an_atlas_category() {
        let mut filed: Vec<IndexKind> = AtlasCategory::ALL
            .into_iter()
            .flat_map(AtlasCategory::kinds)
            .collect();
        filed.sort();
        let mut every = IndexKind::ALL.to_vec();
        every.sort();
        assert_eq!(filed, every, "each index kind is listed under one category");
    }

    #[test]
    fn every_boss_names_its_creature_or_set_piece() {
        for boss in DescentBoss::ALL {
            let creature = creature_named(boss.name());
            assert_eq!(
                creature.is_none(),
                matches!(
                    boss,
                    DescentBoss::TheSinkhole | DescentBoss::StarCore | DescentBoss::TheHeart
                ),
                "{boss:?}"
            );
        }
        for boss in fatherload_simulation::endgame::hunt::THEME_BOSSES {
            assert!(
                creature_named(boss.name).is_some(),
                "{} has a creature",
                boss.name
            );
        }
    }
}
