//! The Atlas's destinations: every kind of place it can travel to, built from the world's
//! generators (never by streaming the world in).
//!
//! **TODO-SEAM (w14-index):** a thin local stand-in for the simulation's destination index
//! (W14-B). The names follow the plan: [`DestinationKind`]s gathered per [`AtlasCategory`],
//! each [`Destination`] carrying where it is ([`AtlasTarget`]) and the preview metadata the
//! info card shows ([`Fact`]s, the boss and the creatures). [`travel_command`] maps a choice
//! onto today's test-mode warps until the index's `TestModeAction::TravelTo` lands.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "distances and preview reaches are shown, never simulated"
)]

use std::fmt::Write as _;

use fatherload_simulation::{
    PlayerCommand, PlayerId, TestModeAction, WorldState,
    descent::{Descent, DescentBoss, Stratum},
    endgame::hunt,
    enemy::EnemyKind,
    hostiles::Faction,
    people::deep_folk::{self, DeepSettlement},
    place::{BodyId, Place, SpaceBodyKey},
    position::WorldPos,
    space::{self, BodyKind, Space, SpaceTile, Theme},
    terrain::TilePosition,
    worldgen::{self, Band, HazardKind, WorldSettings, descent as gen_descent},
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

/// What a destination is (the "kind" Previous/Next cycle through).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DestinationKind {
    Surface,
    Descent,
    Stratum,
    /// A classic world's band.
    Band,
    /// A spot picked on the depth map.
    DepthSpot,
    HomeTown,
    Village,
    SkyTown,
    Moon,
    Outpost,
    Tomb,
    StorySite,
    GateBoss,
    ThemeBoss,
    FactionBoss,
    DeepSettlement,
    AlienTown,
    StarSystem,
    Planet,
    SmallBody,
    Station,
    Belt,
    Comet,
    Nebula,
    BlackHole,
    Wormhole,
    /// A spot picked on the galaxy map.
    SpaceSpot,
    Player,
}

impl DestinationKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Descent => "Descent",
            Self::Stratum => "stratum",
            Self::Band => "band",
            Self::DepthSpot => "depth",
            Self::HomeTown => "town",
            Self::Village => "village",
            Self::SkyTown => "sky town",
            Self::Moon => "moon",
            Self::Outpost => "outpost",
            Self::Tomb => "tomb",
            Self::StorySite => "story",
            Self::GateBoss => "gate boss",
            Self::ThemeBoss => "theme boss",
            Self::FactionBoss => "faction boss",
            Self::DeepSettlement => "deep settlement",
            Self::AlienTown => "alien town",
            Self::StarSystem => "star system",
            Self::Planet => "planet",
            Self::SmallBody => "small body",
            Self::Station => "station",
            Self::Belt => "asteroid belt",
            Self::Comet => "comet",
            Self::Nebula => "nebula",
            Self::BlackHole => "black hole",
            Self::Wormhole => "wormhole",
            Self::SpaceSpot => "space",
            Self::Player => "player",
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
    /// Wherever that player is now.
    Player(PlayerId),
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
            Self::Player(id) => format!("player:{id:?}"),
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
}

impl Destination {
    fn new(
        kind: DestinationKind,
        category: AtlasCategory,
        name: impl Into<String>,
        subtitle: impl Into<String>,
        target: AtlasTarget,
    ) -> Self {
        let mut destination = Self {
            kind,
            category,
            name: name.into(),
            subtitle: subtitle.into(),
            target,
            facts: Vec::new(),
            boss: None,
            creatures: Vec::new(),
            keywords: String::new(),
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
        AtlasTarget::Player(id) => resolve_player(world, id).map_or_else(
            || "gone".to_owned(),
            |target| distance_text(world, &target, origin),
        ),
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
        #[allow(
            clippy::cast_possible_truncation,
            reason = "under a tenth of a sector of tiles"
        )]
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

/// Where `player` is now, as a target the preview can draw.
#[must_use]
pub fn resolve_player(world: &WorldState, player: PlayerId) -> Option<AtlasTarget> {
    let rig = world.player(player)?;
    Some(match world.place_of(player) {
        Place::Space => AtlasTarget::Space {
            pos: rig.flight.pos,
            reach: 400.0,
        },
        // TODO-SEAM (w14-index): the index resolves a player on a body to their column there.
        Place::OnBody(BodyId::Space(body)) => AtlasTarget::Body { body, column: 0 },
        Place::OnBody(_) => AtlasTarget::Home(rig.tile_in(world.frame())),
    })
}

/// The command that takes `player`'s rig to `destination`.
///
/// **TODO-SEAM (w14-index):** this becomes
/// `PlayerCommand::TestMode(TestModeAction::TravelTo(..))` with the index's safe arrival. Until
/// then it maps onto today's warps: the top of the band a home tile lies in (the surface goes
/// home), the Moon, a body's arrival point, free flight and the galaxy story's goal.
#[must_use]
pub fn travel_command(
    world: &WorldState,
    destination: &Destination,
    player: PlayerId,
) -> Option<PlayerCommand> {
    let action = match destination.target {
        AtlasTarget::Home(tile) => home_warp(world, tile),
        AtlasTarget::Body { body, .. } => {
            TestModeAction::WarpToPlace(Place::OnBody(BodyId::Space(body)))
        }
        AtlasTarget::Space { .. } => TestModeAction::WarpToPlace(Place::Space),
        AtlasTarget::Story => TestModeAction::WarpToStory,
        AtlasTarget::Player(id) => {
            if id == player {
                return None;
            }
            let target = resolve_player(world, id)?;
            let stand_in = Destination {
                target,
                ..destination.clone()
            };
            return travel_command(world, &stand_in, player);
        }
    };
    Some(PlayerCommand::TestMode(action))
}

/// Today's nearest warp to home tile `tile` (see [`travel_command`]).
fn home_warp(world: &WorldState, tile: TilePosition) -> TestModeAction {
    let settings = *world.world_settings();
    if let Some(moon) = worldgen::sky::moon(&settings)
        && (tile.x - moon.center_x).abs() <= moon.half_width
        && tile.y < 0
    {
        return TestModeAction::WarpToPlace(Place::OnBody(BodyId::Moon));
    }
    let surface = world.terrain().surface_row(tile.x);
    if tile.y <= surface + 2 {
        return TestModeAction::WarpToPlace(Place::HOME);
    }
    TestModeAction::WarpToBand(world.terrain().band_at(tile))
}

/// Every destination of `category` in `world`, seen from `player` (bookmarks and recent
/// places are the seat's own: see `shell::atlas`).
#[must_use]
pub fn destinations(
    world: &WorldState,
    player: Option<PlayerId>,
    category: AtlasCategory,
) -> Vec<Destination> {
    let settings = *world.world_settings();
    match category {
        AtlasCategory::Depth => depth_destinations(world, &settings),
        AtlasCategory::Places => place_destinations(world, &settings, player),
        AtlasCategory::Bosses => boss_destinations(world, &settings),
        AtlasCategory::Settlements => settlement_destinations(world, &settings),
        AtlasCategory::Space => space_destinations(world),
        AtlasCategory::Players => player_destinations(world, player),
        AtlasCategory::Bookmarks | AtlasCategory::Recent => Vec::new(),
    }
}

/// A depth on the home world picked on the depth map.
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
        AtlasCategory::Depth,
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
    destination
}

/// A free-flight spot picked on the galaxy map, with the nearest thing named.
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
    Destination::new(
        DestinationKind::SpaceSpot,
        AtlasCategory::Space,
        "Open space",
        subtitle,
        AtlasTarget::Space {
            pos,
            reach: 6_000.0,
        },
    )
    .fact("Coordinates", format!("{}, {}", thousands(x), thousands(y)))
}

fn depth_line(row: i32) -> String {
    format!(
        "{} m (row {})",
        thousands(row.into()),
        thousands(row.into())
    )
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

/// A theme boss's creature, by its name on the hunt list.
fn theme_boss_creature(name: &str) -> Option<EnemyKind> {
    [
        EnemyKind::Sporemother,
        EnemyKind::ReactorWarden,
        EnemyKind::AshenSovereign,
        EnemyKind::Thunderhead,
        EnemyKind::Gloamwarden,
        EnemyKind::RingWarden,
    ]
    .into_iter()
    .find(|kind| {
        let own = kind.name().trim_start_matches("The ").to_lowercase();
        name.to_lowercase().contains(&own)
    })
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
    if !destination.creatures.is_empty() {
        let names: Vec<&str> = destination
            .creatures
            .iter()
            .take(4)
            .map(|kind| kind.name())
            .collect();
        destination = destination.fact("Residents", names.join(", "));
    }
    destination
}

/// A middle column of the home world (the spawn's, where the Descents are drawn).
const HOME_COLUMN: i32 = worldgen::SPAWN_TILE_X;

#[allow(
    clippy::too_many_lines,
    reason = "one block per kind: the surface, Descents, strata or bands"
)]
fn depth_destinations(world: &WorldState, settings: &WorldSettings) -> Vec<Destination> {
    let mut out = Vec::new();
    let surface = world.terrain().surface_row(HOME_COLUMN);
    out.push(
        Destination::new(
            DestinationKind::Surface,
            AtlasCategory::Depth,
            "The surface",
            "Home, at the spawn point",
            AtlasTarget::Home(TilePosition {
                x: HOME_COLUMN,
                y: surface - 2,
            }),
        )
        .fact("Depth", "0 m")
        .with_keywords("top home spawn grass descent 0"),
    );
    if !settings.generator.has_descents() {
        for band in Band::ALL {
            let top = worldgen::strata::band_solid_top_row(settings, band, HOME_COLUMN);
            out.push(
                Destination::new(
                    DestinationKind::Band,
                    AtlasCategory::Depth,
                    band.name(),
                    "A classic band",
                    AtlasTarget::Home(TilePosition {
                        x: HOME_COLUMN,
                        y: top + 1,
                    }),
                )
                .fact("Depth", depth_line(top)),
            );
        }
        return out;
    }
    let layout = gen_descent::layout(settings);
    for descent in Descent::ALL {
        let (top, bottom) = layout.descent_rows(descent);
        let first = descent.first_stratum();
        let start = gen_descent::stratum_solid_top_row(settings, &layout, first, HOME_COLUMN);
        let number = descent.index() + 1;
        let mut destination = Destination::new(
            DestinationKind::Descent,
            AtlasCategory::Depth,
            format!("Descent {} - {}", descent.numeral(), descent.name()),
            format!("{} strata", descent.strata().count()),
            AtlasTarget::Home(TilePosition {
                x: HOME_COLUMN,
                y: start + 2,
            }),
        )
        .fact(
            "Depth",
            format!("{} - {} m", thousands(top.into()), thousands(bottom.into())),
        )
        .fact("Story", descent.story_beat())
        .with_keywords(&format!(
            "descent {number} d{number} {} {}",
            descent.numeral().to_lowercase(),
            descent.name()
        ));
        if let Some(gate) = descent.gate() {
            destination = destination.fact("Gate", gate.boss.name());
            destination.boss = gate_boss_creature(gate.boss);
        }
        destination = with_stratum_facts(destination, first);
        out.push(destination);
    }
    for stratum in Stratum::ALL {
        let (top, bottom) = layout.rows(stratum);
        if bottom <= top {
            continue;
        }
        let descent = stratum.descent();
        let row = gen_descent::stratum_solid_top_row(settings, &layout, stratum, HOME_COLUMN);
        let number = descent.index() + 1;
        let destination = Destination::new(
            DestinationKind::Stratum,
            AtlasCategory::Depth,
            stratum.name(),
            format!("Descent {} - {}", descent.numeral(), descent.name()),
            AtlasTarget::Home(TilePosition {
                x: HOME_COLUMN,
                y: row + 2,
            }),
        )
        .fact(
            "Depth",
            format!("{} - {} m", thousands(top.into()), thousands(bottom.into())),
        )
        .with_keywords(&format!("descent {number} d{number}"));
        out.push(with_stratum_facts(destination, stratum));
    }
    out
}

#[allow(
    clippy::too_many_lines,
    reason = "one block per kind of place on the home world"
)]
fn place_destinations(
    world: &WorldState,
    settings: &WorldSettings,
    player: Option<PlayerId>,
) -> Vec<Destination> {
    let mut out = Vec::new();
    let surface = world.terrain().surface_row(HOME_COLUMN);
    out.push(
        Destination::new(
            DestinationKind::HomeTown,
            AtlasCategory::Places,
            "Home town",
            "The shops, the depot and the spawn point",
            AtlasTarget::Home(TilePosition {
                x: HOME_COLUMN,
                y: surface - 2,
            }),
        )
        .fact("Depth", "0 m")
        .with_keywords("spawn shop depot garage"),
    );
    if let Some(moon) = worldgen::sky::moon(settings) {
        out.push(
            Destination::new(
                DestinationKind::Moon,
                AtlasCategory::Places,
                "The Moon",
                "Its colony, craters and crawlers",
                AtlasTarget::Home(TilePosition {
                    x: moon.center_x,
                    y: moon.top_row - 3,
                }),
            )
            .fact(
                "Altitude",
                format!("{} m up", thousands((-moon.top_row).into())),
            )
            .fact("Gravity", "0.17 g")
            .with_keywords("lunar colony sky"),
        );
    }
    let origin = player
        .and_then(|player| world.player(player))
        .map_or(HOME_COLUMN, |rig| rig.tile_in(world.frame()).x);
    let span = 2_400;
    let villages =
        fatherload_simulation::village::villages_between(settings, origin - span, origin + span);
    for village in villages.iter().take(12) {
        let (kind, setting) = match village.setting {
            fatherload_simulation::village::layout::VillageSetting::Surface => {
                (DestinationKind::Village, "Surface village")
            }
            fatherload_simulation::village::layout::VillageSetting::Underground => {
                (DestinationKind::Village, "Underground hall")
            }
            fatherload_simulation::village::layout::VillageSetting::Sky(_) => {
                (DestinationKind::SkyTown, "Sky town")
            }
        };
        let centre = i32::midpoint(village.first_column, village.last_column);
        out.push(
            Destination::new(
                kind,
                AtlasCategory::Places,
                village.name.clone(),
                setting,
                AtlasTarget::Home(TilePosition {
                    x: centre,
                    y: village.ground_row - 2,
                }),
            )
            .fact("Depth", depth_line(village.ground_row))
            .fact("Prosperity", format!("{} of 5", village.prosperity))
            .with_keywords("village settlement people"),
        );
    }
    let sky_rect = worldgen::TileRect {
        min: TilePosition {
            x: origin - span,
            y: -2_000,
        },
        max: TilePosition {
            x: origin + span,
            y: 0,
        },
    };
    for town in fatherload_simulation::village::sky_towns_in(settings, &sky_rect)
        .iter()
        .take(6)
    {
        let centre = i32::midpoint(town.first_column, town.last_column);
        out.push(
            Destination::new(
                DestinationKind::SkyTown,
                AtlasCategory::Places,
                town.name.clone(),
                "Sky town",
                AtlasTarget::Home(TilePosition {
                    x: centre,
                    y: town.ground_row - 2,
                }),
            )
            .fact(
                "Altitude",
                format!("{} m up", thousands((-town.ground_row).into())),
            )
            .with_keywords("sky island cloud"),
        );
    }
    if settings.generator.has_descents() {
        let layout = fatherload_simulation::descent::fields::cached_layout(settings);
        for descent in Descent::ALL {
            let Some(hall) = gen_descent::landmarks::outpost(settings, &layout, descent) else {
                continue;
            };
            let number = descent.index() + 1;
            out.push(
                with_stratum_facts(
                    Destination::new(
                        DestinationKind::Outpost,
                        AtlasCategory::Places,
                        format!("Outpost {}", descent.numeral()),
                        format!("{} - {}", descent.name(), hall.stratum.name()),
                        AtlasTarget::Home(TilePosition {
                            x: i32::midpoint(hall.bounds.min.x, hall.bounds.max.x),
                            y: hall.bounds.max.y - 2,
                        }),
                    )
                    .fact("Depth", depth_line(hall.bounds.max.y)),
                    hall.stratum,
                )
                .with_keywords(&format!("descent {number} lift cable station")),
            );
        }
    }
    let tomb_rect = worldgen::TileRect {
        min: TilePosition {
            x: origin - 600,
            y: -400,
        },
        max: TilePosition {
            x: origin + 600,
            y: 1_400,
        },
    };
    if settings.generator.has_tombs() {
        let mut tombs = worldgen::streamed::tomb_sites::sites_touching(settings, &tomb_rect);
        tombs.sort_by_key(|site| ((site.door.x - origin).abs(), site.door.y));
        for site in tombs.iter().take(8) {
            out.push(
                Destination::new(
                    DestinationKind::Tomb,
                    AtlasCategory::Places,
                    site.title(),
                    format!("{} floors", site.floors()),
                    AtlasTarget::Home(TilePosition {
                        x: site.door.x,
                        y: site.door.y,
                    }),
                )
                .fact("Depth", depth_line(site.door.y))
                .fact("Tier", site.tier.to_string())
                .with_keywords("tomb crypt dungeon vault"),
            );
        }
    }
    if world.space_enabled() {
        out.push(
            Destination::new(
                DestinationKind::StorySite,
                AtlasCategory::Places,
                "The road's current goal",
                "Where the galaxy story leads next",
                AtlasTarget::Story,
            )
            .with_keywords("galaxy story quest road relic"),
        );
    }
    out
}

fn boss_destinations(world: &WorldState, settings: &WorldSettings) -> Vec<Destination> {
    let mut out = Vec::new();
    if settings.generator.has_descents() {
        let layout = fatherload_simulation::descent::fields::cached_layout(settings);
        for descent in Descent::ALL {
            let Some(gate) = descent.gate() else {
                continue;
            };
            let chamber = gen_descent::landmarks::gate_chamber(settings, &layout, descent);
            let number = descent.index() + 1;
            let mut destination = Destination::new(
                DestinationKind::GateBoss,
                AtlasCategory::Bosses,
                gate.boss.name(),
                format!("Gate of Descent {} - {}", descent.numeral(), descent.name()),
                AtlasTarget::Home(TilePosition {
                    x: i32::midpoint(chamber.bounds.min.x, chamber.bounds.max.x),
                    y: chamber.bounds.max.y - 2,
                }),
            )
            .fact("Depth", depth_line(chamber.bounds.max.y))
            .fact(
                "Rewards",
                gate.rewards
                    .iter()
                    .map(|key| key.name())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
            .with_keywords(&format!("descent {number} gate arena"));
            destination.boss = gate_boss_creature(gate.boss);
            out.push(with_stratum_facts(destination, chamber.stratum));
        }
    }
    if world.space_enabled() {
        let seed = world.world_seed();
        let galaxy = Space::shared(seed);
        for boss in hunt::THEME_BOSSES {
            let Some(body) = hunt::nearest_theme_world(seed, boss.world) else {
                continue;
            };
            let Some(found) = galaxy.body(body) else {
                continue;
            };
            let system = galaxy.system(body.system_ref());
            let world_name = system
                .as_ref()
                .map_or_else(String::new, |system| system.body_name(body.body));
            let mut destination = Destination::new(
                DestinationKind::ThemeBoss,
                AtlasCategory::Bosses,
                boss.name,
                format!("Lair on {world_name} ({})", boss.world.name()),
                AtlasTarget::Body { body, column: 0 },
            )
            .fact("Theme", boss.world.name())
            .fact("Gravity", gravity_text(found.gravity_milli_g))
            .with_keywords("planet lair theme");
            destination.boss = theme_boss_creature(boss.name);
            out.push(destination);
        }
        for faction in Faction::ALL {
            let Some(lair) = world.boss_lair(faction) else {
                continue;
            };
            let (sector_x, sector_y) = lair.system.sector;
            out.push(
                Destination::new(
                    DestinationKind::FactionBoss,
                    AtlasCategory::Bosses,
                    format!("The {} flagship", faction.name()),
                    format!("A lair in sector {sector_x}, {sector_y}"),
                    AtlasTarget::Space {
                        pos: lair.pos,
                        reach: 900.0,
                    },
                )
                .fact("Faction", faction.name())
                .with_keywords("ship fleet lair hostile"),
            );
        }
    }
    out
}

fn settlement_destinations(world: &WorldState, settings: &WorldSettings) -> Vec<Destination> {
    let mut out = Vec::new();
    for place in DeepSettlement::ALL {
        let Some((ground, first, last)) = deep_folk::hall(settings, place) else {
            continue;
        };
        let tile = TilePosition {
            x: i32::midpoint(first, last),
            y: ground - 2,
        };
        let stratum = gen_descent::stratum_at(settings, tile.x, ground);
        out.push(
            Destination::new(
                DestinationKind::DeepSettlement,
                AtlasCategory::Settlements,
                place.name(),
                format!(
                    "Descent {} - {}",
                    stratum.descent().numeral(),
                    stratum.name()
                ),
                AtlasTarget::Home(tile),
            )
            .fact("Depth", depth_line(ground))
            .fact("Residents", format!("{} folk", place.cast().len()))
            .with_keywords("deep folk people hall"),
        );
    }
    let villages = fatherload_simulation::village::villages_between(
        settings,
        HOME_COLUMN - 3_000,
        HOME_COLUMN + 3_000,
    );
    for village in villages.iter().take(8) {
        let centre = i32::midpoint(village.first_column, village.last_column);
        out.push(
            Destination::new(
                DestinationKind::Village,
                AtlasCategory::Settlements,
                village.name.clone(),
                format!("{} village", capitalised(village.size.name())),
                AtlasTarget::Home(TilePosition {
                    x: centre,
                    y: village.ground_row - 2,
                }),
            )
            .fact("Depth", depth_line(village.ground_row))
            .fact("Buildings", village.buildings.len().to_string())
            .with_keywords("village people"),
        );
    }
    if world.space_enabled() {
        let seed = world.world_seed();
        let galaxy = Space::shared(seed);
        for theme in Theme::PLANET {
            let Some(body) = hunt::nearest_theme_world(seed, theme) else {
                continue;
            };
            let Some(params) = fatherload_simulation::place::planet_params_for(seed, body) else {
                continue;
            };
            let towns = fatherload_simulation::alien::town::towns_of(&params);
            let Some(town) = towns.first() else {
                continue;
            };
            let system = galaxy
                .system(body.system_ref())
                .map_or_else(String::new, |system| system.body_name(body.body));
            out.push(
                Destination::new(
                    DestinationKind::AlienTown,
                    AtlasCategory::Settlements,
                    town.name.clone(),
                    format!("{:?} town on {system}", town.species),
                    AtlasTarget::Body {
                        body,
                        column: i64::from(town.site.centre),
                    },
                )
                .fact("World", format!("{system} ({})", theme.name()))
                .fact("People", format!("{:?}", town.species))
                .with_keywords(&format!("alien town {}", theme.name())),
            );
        }
    }
    out
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

fn gravity_text(milli_g: u16) -> String {
    format!("{:.2} g", f32::from(milli_g) / 1_000.0)
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// How far round home the Space category looks (tiles).
pub const SPACE_REACH_TILES: i64 = 3 * space::SPACE_SECTOR_TILES;

#[allow(
    clippy::too_many_lines,
    reason = "one block per kind of thing in space"
)]
fn space_destinations(world: &WorldState) -> Vec<Destination> {
    let mut out = Vec::new();
    if !world.space_enabled() {
        return out;
    }
    let seed = world.world_seed();
    let galaxy = Space::shared(seed);
    // Worlds by theme first ("planet ice"), the nearest of each kind to home.
    for theme in Theme::PLANET {
        let Some(body) = hunt::nearest_theme_world(seed, theme) else {
            continue;
        };
        let (Some(found), Some(system)) = (galaxy.body(body), galaxy.system(body.system_ref()))
        else {
            continue;
        };
        out.push(planet_destination(&system, &found, body));
    }
    let home = galaxy.home_system();
    for (index, body) in home.bodies.iter().enumerate() {
        let Ok(index) = u8::try_from(index) else {
            continue;
        };
        let key = SpaceBodyKey {
            sector: space::HOME_SECTOR,
            system: home.index,
            body: index,
        };
        if matches!(body.kind, BodyKind::HomeWorld | BodyKind::HomeMoon) {
            continue;
        }
        if body.radius < space::MIN_LANDABLE_RADIUS {
            continue;
        }
        out.push(planet_destination(&home, body, key));
    }
    let centre = home.centre;
    for (reference, distance) in galaxy
        .systems_near(centre, SPACE_REACH_TILES)
        .iter()
        .take(16)
    {
        let Some(system) = galaxy.system(*reference) else {
            continue;
        };
        out.push(
            Destination::new(
                DestinationKind::StarSystem,
                AtlasCategory::Space,
                system.name.clone(),
                format!(
                    "{} - {}",
                    capitalised(system.class.name()),
                    planets_text(system.planet_count())
                ),
                AtlasTarget::Space {
                    pos: system.centre.pos(),
                    reach: system.extent as f32 * 0.6,
                },
            )
            .fact("Star", system.class.name())
            .fact(
                "From home",
                format!("{:.1} sectors", distance / space::SPACE_SECTOR_TILES as f64),
            )
            .with_keywords("system star sun"),
        );
        for station in system.stations.iter().take(2) {
            out.push(
                Destination::new(
                    DestinationKind::Station,
                    AtlasCategory::Space,
                    format!("{} {}", system.name, capitalised(station.kind.name())),
                    format!("Station of {}", system.name),
                    AtlasTarget::Space {
                        pos: station.centre.pos(),
                        reach: f32::from(station.size) * 6.0,
                    },
                )
                .fact("Kind", station.kind.name())
                .with_keywords("station dock trade shipyard"),
            );
        }
        for belt in system.belts.iter().take(1) {
            let radius = i64::from(belt.inner.midpoint(belt.outer));
            out.push(
                Destination::new(
                    DestinationKind::Belt,
                    AtlasCategory::Space,
                    format!("{} belt", system.name),
                    format!("{} asteroids", capitalised(belt.ore.name())),
                    AtlasTarget::Space {
                        pos: system.centre.offset(radius, 0).pos(),
                        reach: 600.0,
                    },
                )
                .fact("Ore", belt.ore.name())
                .with_keywords("asteroid belt rocks mining"),
            );
        }
        for comet in system.comets.iter().take(1) {
            out.push(
                Destination::new(
                    DestinationKind::Comet,
                    AtlasCategory::Space,
                    format!("Comet of {}", system.name),
                    "A nucleus and its tail",
                    AtlasTarget::Space {
                        pos: comet.centre.pos(),
                        reach: f32::from(comet.nucleus_radius) * 30.0,
                    },
                )
                .with_keywords("comet tail ice"),
            );
        }
    }
    for sector in galaxy.sectors_near(centre, SPACE_REACH_TILES) {
        for nebula in sector.nebulae.iter().take(1) {
            if out.iter().any(|known| {
                known.kind == DestinationKind::Nebula
                    && known.target.key().contains(&format!("{}", nebula.id))
            }) {
                continue;
            }
            out.push(
                Destination::new(
                    DestinationKind::Nebula,
                    AtlasCategory::Space,
                    format!("Nebula {:04X}", nebula.id & 0xFFFF),
                    "Fog that hides and slows",
                    AtlasTarget::Space {
                        pos: nebula.centre.pos(),
                        reach: (nebula.radius as f32 * 0.4).min(200_000.0),
                    },
                )
                .with_keywords("nebula fog cloud"),
            );
        }
        for hole in &sector.black_holes {
            out.push(
                Destination::new(
                    DestinationKind::BlackHole,
                    AtlasCategory::Space,
                    format!("Black hole {:04X}", hole.id & 0xFFFF),
                    "Lensing, time dilation, no way back",
                    AtlasTarget::Space {
                        pos: hole.centre.pos(),
                        reach: hole.lensing_radius as f32 * 1.4,
                    },
                )
                .fact(
                    "Pull",
                    gravity_text(hole.gravity_milli_g.min(60_000) as u16),
                )
                .with_keywords("black hole singularity lens"),
            );
        }
        for mouth in &sector.wormholes {
            out.push(
                Destination::new(
                    DestinationKind::Wormhole,
                    AtlasCategory::Space,
                    format!("Wormhole {},{}", mouth.id.a.0, mouth.id.a.1),
                    "A mouth to another region",
                    AtlasTarget::Space {
                        pos: mouth.centre.pos(),
                        reach: mouth.radius as f32 * 6.0,
                    },
                )
                .with_keywords("wormhole tunnel portal"),
            );
        }
    }
    // Nebulae overlap their neighbours' lists, and a theme's nearest world may circle home:
    // keep each place once.
    let mut listed = std::collections::BTreeSet::new();
    out.retain(|destination| listed.insert(destination.target.key()));
    out
}

fn planet_destination(
    system: &space::StarSystem,
    body: &space::SpaceBody,
    key: SpaceBodyKey,
) -> Destination {
    let kind = if matches!(body.kind, BodyKind::Planet) {
        DestinationKind::Planet
    } else {
        DestinationKind::SmallBody
    };
    let name = system.body_name(key.body);
    let mut destination = Destination::new(
        kind,
        AtlasCategory::Space,
        name,
        format!(
            "{} {} - {}",
            capitalised(body.theme.name()),
            body.kind.name(),
            system.name
        ),
        AtlasTarget::Body {
            body: key,
            column: 0,
        },
    )
    .fact("Theme", body.theme.name())
    .fact("Gravity", gravity_text(body.gravity_milli_g))
    .fact("Air", body.atmosphere.name())
    .fact("Temperature", format!("{} C", body.temperature_c))
    .fact("Radius", format!("{} tiles", thousands(body.radius.into())));
    if let Some(boss) = hunt::THEME_BOSSES
        .iter()
        .find(|boss| boss.world == body.theme)
    {
        destination = destination.fact("Boss", boss.name);
        destination.boss = theme_boss_creature(boss.name);
    }
    let mut keywords = String::new();
    let _ = write!(keywords, "planet world {} moon", body.theme.name());
    destination.with_keywords(&keywords)
}

fn player_destinations(world: &WorldState, me: Option<PlayerId>) -> Vec<Destination> {
    world
        .player_ids()
        .enumerate()
        .filter(|(_, id)| Some(*id) != me)
        .filter_map(|(index, id)| {
            let target = resolve_player(world, id)?;
            let place = match world.place_of(id) {
                Place::Space => "In space".to_owned(),
                Place::OnBody(BodyId::Home) => "On the home world".to_owned(),
                Place::OnBody(BodyId::Moon) => "On the Moon".to_owned(),
                Place::OnBody(BodyId::Space(_)) => "On another world".to_owned(),
            };
            let mut destination = Destination::new(
                DestinationKind::Player,
                AtlasCategory::Players,
                format!("Player {}", index + 1),
                place,
                AtlasTarget::Player(id),
            )
            .with_keywords(&format!("player {} crew friend", index + 1));
            if let AtlasTarget::Home(tile) = target {
                destination = destination.fact("Depth", depth_line(tile.y));
            }
            Some(destination)
        })
        .collect()
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
    fn every_gate_boss_names_its_creature_or_set_piece() {
        for boss in DescentBoss::ALL {
            let creature = gate_boss_creature(boss);
            assert_eq!(
                creature.is_none(),
                matches!(
                    boss,
                    DescentBoss::TheSinkhole | DescentBoss::StarCore | DescentBoss::TheHeart
                ),
                "{boss:?}"
            );
        }
        for boss in hunt::THEME_BOSSES {
            assert!(
                theme_boss_creature(boss.name).is_some(),
                "{} has a creature",
                boss.name
            );
        }
    }
}
