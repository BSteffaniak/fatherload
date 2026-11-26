//! The Atlas (Wave 14 W14-C): each seat's own travel browser in a test-mode world (see
//! [`crate::atlas`]). It opens from the pause menu, F2 or the touch ATLAS button and covers the
//! seat's view; travel always asks first: the selection shows its preview and card, and only Go
//! travels.
//!
//! Controls (every device; the screen shows each device's own prompts):
//!
//! | Action | Keyboard | Controller | Pointer / touch |
//! | --- | --- | --- | --- |
//! | Choose a destination (the preview follows) | Up / Down, Page Up / Down, wheel | stick / d-pad up-down | tap or click a row |
//! | Category | Left / Right | LB / RB, d-pad left-right | tap a category |
//! | Search | `/` then type, Enter or Esc to finish | - | tap the search box (the browser's text field) |
//! | Go | Enter | A | Go |
//! | Previous / next of the same kind | X / Q | weapon / explosive buttons | Prev / Next |
//! | Random | C | scan | Random |
//! | Bookmark | B | bomb | Bookmark |
//! | Back (travel to where you were) | F | detonate | Back |
//! | Map mode, its cursor | M, then the movement keys | map button, then the stick | Map, then hover or tap the map |
//! | Close | Esc / F2 | B (back) | Close |

use std::collections::BTreeMap;

use fatherload_simulation::{
    PlayerCommand, PlayerId, TestModeAction, WorldState, destination::DestinationId, place::Place,
    space::SpaceTile,
};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::{
    atlas::{
        AtlasCategory, AtlasTarget, Destination, Origin,
        hit::{AtlasButton, AtlasHit},
        index::{self, depth_spot, space_spot},
        map::{DepthMap, GalaxyMap, MapKind},
    },
    input::PlayerInput,
};

/// Longest search text.
const MAX_SEARCH: usize = 32;
/// How far the map cursor moves per second with a stick held all the way (map widths).
const MAP_CURSOR_SPEED: f32 = 0.35;
/// One map step from a menu key press (map widths).
const MAP_STEP: f32 = 1.0 / 48.0;
/// The depth map's width over its height (the drawn map's own shape is close to this).
pub const DEPTH_MAP_ASPECT: f32 = 0.75;

/// Map mode's state.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasMap {
    pub kind: MapKind,
    /// The cursor, `(u, v)` in 0..1.
    pub cursor: (f32, f32),
    pub depth: DepthMap,
    pub galaxy: GalaxyMap,
    /// The spot under the cursor, as a destination (rebuilt when the cursor moves).
    pub spot: Option<Destination>,
}

/// A seat's Atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct SeatAtlas {
    pub category: AtlasCategory,
    /// Each category's destinations, built the first time it is shown after opening.
    lists: BTreeMap<AtlasCategory, Vec<Destination>>,
    /// What the list shows: the category's destinations, or every category's search matches.
    pub shown: Vec<Destination>,
    /// The selected row of [`Self::shown`].
    pub selected: usize,
    /// The search text (empty: browse the category).
    pub search: String,
    /// The search box has the keyboard.
    pub typing: bool,
    /// Map mode (`None`: the list).
    pub map: Option<AtlasMap>,
    /// The seat's player (the index answers for it).
    pub player: Option<PlayerId>,
    /// The seat's bookmarks as the world keeps them (the index's travel log: saved and
    /// replicated), oldest first.
    pub marks: Vec<DestinationId>,
    /// Places on the seat's back-stack in the world's travel log (Back goes to the last).
    pub back_count: usize,
    /// Where the seat is (distances on the card).
    pub origin: Origin,
    /// A pointer pick waiting for this frame's update.
    picked: Option<usize>,
    /// The map point under the mouse this frame.
    pub hovered: Option<(f32, f32)>,
    /// The Random button's draw (presentation only).
    random: u64,
}

impl Default for SeatAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl SeatAtlas {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            category: AtlasCategory::Depth,
            lists: BTreeMap::new(),
            shown: Vec::new(),
            selected: 0,
            search: String::new(),
            typing: false,
            map: None,
            player: None,
            marks: Vec::new(),
            back_count: 0,
            origin: Origin {
                place: Place::HOME,
                tile: None,
                space: fatherload_simulation::position::WorldPos::from_tile(0, 0),
            },
            picked: None,
            hovered: None,
            random: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The destination the preview and card show: the map's spot in map mode, else the
    /// selected row.
    #[must_use]
    pub fn current(&self) -> Option<&Destination> {
        self.map
            .as_ref()
            .map_or_else(|| self.shown.get(self.selected), |map| map.spot.as_ref())
    }

    /// The destinations Previous/Next would show (the neighbours of the same kind), for the
    /// preview cache to prepare.
    #[must_use]
    pub fn neighbours(&self) -> Vec<&Destination> {
        if self.map.is_some() {
            return Vec::new();
        }
        let mut indices: Vec<usize> = [-1, 1]
            .into_iter()
            .filter_map(|step| self.same_kind(step))
            .filter(|index| *index != self.selected)
            .collect();
        indices.dedup();
        indices
            .into_iter()
            .filter_map(|index| self.shown.get(index))
            .collect()
    }

    /// The index of `destination`'s spot among the seat's bookmarks (the world's), if any.
    #[must_use]
    pub fn bookmark_index(&self, destination: &Destination) -> Option<usize> {
        let id = destination.id()?;
        self.marks
            .iter()
            .position(|mark| (mark.place, mark.x, mark.y) == (id.place, id.x, id.y))
    }

    /// Whether `destination` is bookmarked.
    #[must_use]
    pub fn bookmarked(&self, destination: &Destination) -> bool {
        self.bookmark_index(destination).is_some()
    }

    /// Reads the seat's bookmarks and back-stack from the world's travel log; true when they
    /// changed since the last read.
    fn sync_log(&mut self, world: &WorldState, player: Option<PlayerId>) -> bool {
        self.player = player;
        let Some(player) = player else {
            return false;
        };
        let log = world.player_travel(player);
        let changed = self.marks != log.bookmarks || self.back_count != log.recent.len();
        self.marks = log.bookmarks;
        self.back_count = log.recent.len();
        changed
    }

    /// A pointer pick for the next update (see `shell::pointer`).
    pub const fn pick(&mut self, row: usize) {
        self.picked = Some(row);
    }

    /// The row index of the next destination of the selected one's kind `step` away (wrapping).
    fn same_kind(&self, step: isize) -> Option<usize> {
        let kind = self.shown.get(self.selected)?.kind;
        let count = self.shown.len().cast_signed();
        (1..count)
            .map(|offset| (self.selected.cast_signed() + step * offset).rem_euclid(count))
            .map(isize::cast_unsigned)
            .find(|index| self.shown[*index].kind == kind)
            .or(Some(self.selected))
    }

    /// Forgets the built lists (a new world, or the Atlas opening again).
    fn forget_lists(&mut self) {
        self.lists.clear();
        self.shown.clear();
    }

    /// Readies the Atlas to open again (from the pause menu): fresh lists, the list in front.
    pub fn reopen(&mut self) {
        self.forget_lists();
        self.typing = false;
        self.map = None;
    }

    /// The destinations of `category` (built on first use; the bookmarks and the back-stack
    /// are read from the world's log each time).
    fn list(
        &mut self,
        world: &WorldState,
        player: Option<PlayerId>,
        category: AtlasCategory,
    ) -> Vec<Destination> {
        if matches!(category, AtlasCategory::Bookmarks | AtlasCategory::Recent) {
            return index::destinations(world, player, category);
        }
        self.lists
            .entry(category)
            .or_insert_with(|| index::destinations(world, player, category))
            .clone()
    }

    /// Rebuilds [`Self::shown`] for the category or the search, keeping the selection on the
    /// same place when it is still listed.
    fn refresh(&mut self, world: &WorldState, player: Option<PlayerId>) {
        let keep = self.shown.get(self.selected).map(|d| d.target.key());
        self.shown = if self.search.trim().is_empty() {
            self.list(world, player, self.category)
        } else {
            // The destination index's own search (every kind; a leading kind word narrows),
            // ranked for the screen (names that start with the query first).
            let mut found = index::search(world, player, &self.search);
            crate::atlas::search::rank(&mut found, &self.search);
            found
        };
        self.selected = keep
            .and_then(|key| {
                self.shown
                    .iter()
                    .position(|destination| destination.target.key() == key)
            })
            .unwrap_or(0)
            .min(self.shown.len().saturating_sub(1));
    }

    fn set_category(
        &mut self,
        category: AtlasCategory,
        world: &WorldState,
        player: Option<PlayerId>,
    ) {
        self.category = category;
        self.search.clear();
        self.typing = false;
        self.map = None;
        self.shown.clear();
        self.selected = 0;
        self.refresh(world, player);
    }

    /// Moves the map cursor to `cursor` and names the spot under it.
    fn move_cursor(&mut self, world: &WorldState, cursor: (f32, f32)) {
        let Some(map) = self.map.as_mut() else {
            return;
        };
        let cursor = (cursor.0.clamp(0.0, 1.0), cursor.1.clamp(0.0, 1.0));
        if map.spot.is_some() && map.cursor == cursor {
            return;
        }
        map.cursor = cursor;
        map.spot = Some(match map.kind {
            MapKind::Depth => {
                // Snap to the surface when the cursor is in the sky over the ground.
                let tile = map.depth.tile_at(cursor);
                let surface = world.terrain().surface_row(tile.x);
                let tile = if tile.y < surface && tile.y > surface - 24 {
                    fatherload_simulation::terrain::TilePosition {
                        x: tile.x,
                        y: surface - 2,
                    }
                } else {
                    tile
                };
                depth_spot(world, tile)
            }
            MapKind::Galaxy => {
                snap_to_body(world, self.player, map.galaxy, map.galaxy.pos_at(cursor))
            }
        });
    }

    /// What the Map button opens next: the first map from the list (the galaxy when the seat
    /// is out there or browsing space, else the depths of home), then the other map once space
    /// is open, then back to the list (`None`).
    #[must_use]
    pub fn next_map(&self, space_open: bool) -> Option<MapKind> {
        let first = if space_open
            && (!self.origin.place.is_home_store() || self.category == AtlasCategory::Space)
        {
            MapKind::Galaxy
        } else {
            MapKind::Depth
        };
        match &self.map {
            None => Some(first),
            Some(map) if map.kind == first && space_open => Some(match first {
                MapKind::Depth => MapKind::Galaxy,
                MapKind::Galaxy => MapKind::Depth,
            }),
            Some(_) => None,
        }
    }

    /// Opens map mode on `kind`, its cursor over the seat (or the selection).
    fn open_map(&mut self, world: &WorldState, kind: MapKind) {
        let centre = self
            .origin
            .tile
            .map_or(fatherload_simulation::worldgen::SPAWN_TILE_X, |tile| tile.x);
        let depth = DepthMap::fit(world.world_settings(), centre, DEPTH_MAP_ASPECT);
        let galaxy = GalaxyMap::round(SpaceTile::of_pos(self.origin.space));
        let cursor = match (kind, self.current().map(|d| d.target)) {
            (MapKind::Depth, Some(AtlasTarget::Home(tile))) => depth.uv_of(tile),
            (MapKind::Depth, _) => self
                .origin
                .tile
                .map_or((0.5, 0.0), |tile| depth.uv_of(tile)),
            (MapKind::Galaxy, Some(AtlasTarget::Space { pos, .. })) => {
                galaxy.uv_of(SpaceTile::of_pos(pos))
            }
            (MapKind::Galaxy, _) => (0.5, 0.5),
        };
        self.map = Some(AtlasMap {
            kind,
            cursor,
            depth,
            galaxy,
            spot: None,
        });
        self.typing = false;
        self.move_cursor(world, cursor);
    }
}

/// The spot under the galaxy map's cursor: the nearest star system or body when one is close
/// (a few map pixels), else open space.
#[allow(
    clippy::cast_precision_loss,
    reason = "map distances and preview reaches are shown, never simulated"
)]
fn snap_to_body(
    world: &WorldState,
    player: Option<PlayerId>,
    map: GalaxyMap,
    pos: fatherload_simulation::position::WorldPos,
) -> Destination {
    let galaxy = fatherload_simulation::space::Space::shared(world.world_seed());
    let reach = map.radius / 16;
    let tile = SpaceTile::of_pos(pos);
    if let Some((reference, _)) = galaxy.systems_near(tile, reach).first()
        && let Some(system) = galaxy.system(*reference)
        && system.centre.distance(tile) <= reach as f64
    {
        // The index's own candidate for the system: Go stops off its star as the list's does.
        if let Some(listed) = index::system_named(world, player, &system.name) {
            return listed;
        }
        let mut spot = space_spot(world, system.centre.pos());
        spot.name.clone_from(&system.name);
        spot.subtitle = format!(
            "{} - {}",
            system.class.name(),
            index::planets_text(system.planet_count())
        );
        spot.target = AtlasTarget::Space {
            pos: system.centre.pos(),
            reach: system.extent as f32 * 0.6,
        };
        return spot;
    }
    space_spot(world, pos)
}

impl SeatUi {
    /// Opens the seat's Atlas (test-mode worlds only).
    pub(super) fn open_atlas(&mut self, ctx: &SeatContext<'_>, cues: &mut Vec<SoundCue>) {
        if !ctx.world.is_test_mode() {
            "The Atlas is a test-mode tool.".clone_into(&mut self.message);
            return;
        }
        // A building or home's own screens close: the Atlas covers the view.
        self.modal = Some(ModalScreen::Atlas);
        self.atlas.reopen();
        let _ = self.atlas.sync_log(ctx.world, ctx.player);
        if let Some(player) = ctx.player
            && let Some(origin) = Origin::of(ctx.world, player)
        {
            self.atlas.origin = origin;
        }
        self.atlas.refresh(ctx.world, ctx.player);
        cues.push(SoundCue::Ui);
    }

    /// One frame of the open Atlas.
    #[allow(
        clippy::too_many_lines,
        reason = "every way into the Atlas's controls, in order"
    )]
    pub(super) fn handle_atlas(
        &mut self,
        input: PlayerInput,
        delta_seconds: f32,
        ctx: &SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        if !ctx.world.is_test_mode() {
            self.close_atlas();
            return;
        }
        if let Some(player) = ctx.player
            && let Some(origin) = Origin::of(ctx.world, player)
        {
            self.atlas.origin = origin;
        }
        // The bookmarks and back-stack are the world's: a list of them follows its changes.
        let log_changed = self.atlas.sync_log(ctx.world, ctx.player);
        if (self.atlas.shown.is_empty() && self.atlas.search.is_empty())
            || (log_changed
                && self.atlas.search.is_empty()
                && matches!(
                    self.atlas.category,
                    AtlasCategory::Bookmarks | AtlasCategory::Recent
                ))
        {
            self.atlas.refresh(ctx.world, ctx.player);
        }
        // The mouse over the map moves its cursor (the preview follows).
        if let Some(cursor) = self.atlas.hovered.take() {
            self.atlas.move_cursor(ctx.world, cursor);
        }
        if let Some(row) = self.atlas.picked.take()
            && let Some(hit) = AtlasHit::decode(row)
        {
            match hit {
                AtlasHit::Category(index) => {
                    let category = AtlasCategory::ALL[index.min(AtlasCategory::ALL.len() - 1)];
                    self.atlas.set_category(category, ctx.world, ctx.player);
                    cues.push(SoundCue::UiMove);
                }
                AtlasHit::Button(button) => {
                    self.press_atlas_button(button, ctx, cues, requests);
                }
                AtlasHit::Search => {
                    self.atlas.typing = true;
                    self.atlas.map = None;
                }
                AtlasHit::Row(row) => {
                    self.atlas.map = None;
                    self.atlas.typing = false;
                    if row < self.atlas.shown.len() {
                        self.atlas.selected = row;
                        cues.push(SoundCue::UiMove);
                    }
                }
                AtlasHit::Map(u, v) => {
                    self.atlas.move_cursor(ctx.world, (u, v));
                    cues.push(SoundCue::UiMove);
                }
            }
            return;
        }
        if self.atlas.typing {
            self.atlas_typing(input, ctx);
            return;
        }
        if input.atlas || input.backs_out() {
            if input.atlas {
                self.close_atlas();
            } else if self.atlas.map.is_some() {
                self.atlas.map = None;
            } else if !self.atlas.search.is_empty() {
                self.atlas.search.clear();
                self.atlas.refresh(ctx.world, ctx.player);
            } else {
                self.close_atlas();
            }
            return;
        }
        if input.text_input == Some('/') {
            self.atlas.typing = true;
            self.atlas.map = None;
            return;
        }
        if input.map {
            self.press_atlas_button(AtlasButton::Map, ctx, cues, requests);
            return;
        }
        for (pressed, button) in [
            (input.confirm, AtlasButton::Go),
            (input.cycle_weapon, AtlasButton::Previous),
            (input.cycle_explosive, AtlasButton::Next),
            (input.scan, AtlasButton::Random),
            (input.bomb, AtlasButton::Bookmark),
            (input.detonate, AtlasButton::Back),
        ] {
            if pressed {
                self.press_atlas_button(button, ctx, cues, requests);
                return;
            }
        }
        if self.atlas.map.is_some() {
            self.steer_map_cursor(input, delta_seconds, ctx);
            return;
        }
        let step = if input.tab_previous || input.menu_left {
            -1
        } else {
            isize::from(input.tab_next || input.menu_right)
        };
        if step != 0 {
            let category = self.atlas.category.step(step);
            self.atlas.set_category(category, ctx.world, ctx.player);
            return;
        }
        let count = self.atlas.shown.len();
        if count == 0 {
            return;
        }
        let mut row = self.atlas.selected.cast_signed();
        if input.menu_up {
            row -= 1;
        }
        if input.menu_down {
            row += 1;
        }
        if input.page_up {
            row -= 8;
        }
        if input.page_down {
            row += 8;
        }
        if input.scroll_home {
            row = 0;
        }
        if input.scroll_end {
            row = count.cast_signed() - 1;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a few wheel notches a frame"
        )]
        let notches = input.ui_scroll.round() as isize;
        row -= notches;
        self.atlas.selected = row.clamp(0, count.cast_signed() - 1).cast_unsigned();
    }

    /// Typing into the search box: letters type, Backspace deletes, Enter or Esc finish.
    fn atlas_typing(&mut self, input: PlayerInput, ctx: &SeatContext<'_>) {
        // `E` also counts as "confirm" in menus; while typing it is just a letter.
        let typed_confirm_key = matches!(input.text_input, Some('e' | 'E'));
        let before = self.atlas.search.clone();
        if input.text_backspace {
            self.atlas.search.pop();
        } else if input.backs_out() {
            self.atlas.typing = false;
        }
        if let Some(character) = input.text_input
            && !character.is_control()
            && self.atlas.search.chars().count() < MAX_SEARCH
        {
            self.atlas.search.push(character);
        }
        if (input.confirm && !typed_confirm_key) || input.atlas {
            self.atlas.typing = false;
        }
        if self.atlas.search != before {
            self.atlas.selected = 0;
            self.atlas.shown.clear();
            self.atlas.refresh(ctx.world, ctx.player);
        }
    }

    /// Moves the map cursor with the movement keys, stick or d-pad.
    fn steer_map_cursor(&mut self, input: PlayerInput, delta_seconds: f32, ctx: &SeatContext<'_>) {
        let Some(map) = self.atlas.map.as_ref() else {
            return;
        };
        let (mut u, mut v) = map.cursor;
        let held = MAP_CURSOR_SPEED * delta_seconds.clamp(0.0, 0.1);
        let vertical = f32::from(i8::from(input.drill_down) - i8::from(input.thrust));
        u = input.horizontal.mul_add(held, u);
        v = vertical.mul_add(held, v);
        let steps = |minus: bool, plus: bool| f32::from(i8::from(plus) - i8::from(minus));
        u = steps(input.menu_left, input.menu_right).mul_add(MAP_STEP, u);
        v = steps(input.menu_up, input.menu_down).mul_add(MAP_STEP, v);
        if (u, v) != map.cursor {
            self.atlas.move_cursor(ctx.world, (u, v));
        }
    }

    fn close_atlas(&mut self) {
        if self.modal == Some(ModalScreen::Atlas) {
            self.modal = None;
        }
        self.atlas.typing = false;
        self.atlas.map = None;
        self.atlas.hovered = None;
        self.atlas.picked = None;
    }

    #[allow(clippy::too_many_lines, reason = "one arm per button")]
    fn press_atlas_button(
        &mut self,
        button: AtlasButton,
        ctx: &SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        match button {
            AtlasButton::Go => {
                let Some(destination) = self.atlas.current().cloned() else {
                    "Pick a destination first.".clone_into(&mut self.message);
                    return;
                };
                self.travel(&destination, ctx, cues, requests);
            }
            AtlasButton::Previous | AtlasButton::Next => {
                let step = if button == AtlasButton::Next { 1 } else { -1 };
                if self.atlas.map.is_some() {
                    self.atlas.map = None;
                }
                if let Some(index) = self.atlas.same_kind(step) {
                    self.atlas.selected = index;
                    cues.push(SoundCue::UiMove);
                }
            }
            AtlasButton::Random => {
                self.atlas.map = None;
                let count = self.atlas.shown.len();
                if count > 0 {
                    // A presentation-only shuffle (xorshift): the travel itself is decided by
                    // the host from the chosen destination.
                    let mut draw = self.atlas.random;
                    draw ^= draw << 13;
                    draw ^= draw >> 7;
                    draw ^= draw << 17;
                    self.atlas.random = draw;
                    let pick = usize::try_from(draw % count as u64).unwrap_or_default();
                    self.atlas.selected = if pick == self.atlas.selected && count > 1 {
                        (pick + 1) % count
                    } else {
                        pick
                    };
                    cues.push(SoundCue::UiMove);
                }
            }
            AtlasButton::Bookmark => {
                // The bookmarks are the world's (the index's travel log): the button asks it.
                let Some(destination) = self.atlas.current().cloned() else {
                    return;
                };
                if let Some(index) = self.atlas.bookmark_index(&destination) {
                    self.send_travel(
                        PlayerCommand::TestMode(TestModeAction::RemoveBookmark(
                            u8::try_from(index).unwrap_or(u8::MAX),
                        )),
                        ctx,
                        requests,
                    );
                    self.atlas.marks.remove(index);
                    "Bookmark removed.".clone_into(&mut self.message);
                    cues.push(SoundCue::UiBack);
                } else if let Some(id) = destination.id() {
                    self.send_travel(
                        PlayerCommand::TestMode(TestModeAction::Bookmark(id)),
                        ctx,
                        requests,
                    );
                    self.atlas.marks.push(id);
                    "Bookmarked.".clone_into(&mut self.message);
                    cues.push(SoundCue::Ui);
                } else {
                    "The story's goal moves on: it cannot be bookmarked."
                        .clone_into(&mut self.message);
                }
            }
            AtlasButton::Back => {
                if self.atlas.back_count == 0 {
                    "Nowhere to go back to yet.".clone_into(&mut self.message);
                    return;
                }
                self.send_travel(
                    PlayerCommand::TestMode(TestModeAction::TravelBack),
                    ctx,
                    requests,
                );
                self.atlas.back_count -= 1;
                "Back to where you were.".clone_into(&mut self.message);
                cues.push(SoundCue::Ui);
                self.close_atlas();
            }
            AtlasButton::Map => {
                match self.atlas.next_map(ctx.world.space_enabled()) {
                    Some(kind) => self.atlas.open_map(ctx.world, kind),
                    None => self.atlas.map = None,
                }
                cues.push(SoundCue::Ui);
            }
            AtlasButton::Close => self.close_atlas(),
        }
    }

    /// Sends the trip to `destination` (it was previewed: the player pressed Go).
    fn travel(
        &mut self,
        destination: &Destination,
        ctx: &SeatContext<'_>,
        cues: &mut Vec<SoundCue>,
        requests: &mut Vec<ShellRequest>,
    ) {
        let Some(player) = ctx.player else {
            return;
        };
        let Some(command) = index::travel_command(ctx.world, destination, player) else {
            "You are already there.".clone_into(&mut self.message);
            return;
        };
        // The world keeps where the rig was on its back-stack (Back, the Recent list).
        self.send_travel(command, ctx, requests);
        self.message = format!("Travelling to {}.", destination.name);
        cues.push(SoundCue::Ui);
        self.close_atlas();
    }

    fn send_travel(
        &mut self,
        command: PlayerCommand,
        ctx: &SeatContext<'_>,
        requests: &mut Vec<ShellRequest>,
    ) {
        // Whatever the seat had open stays behind.
        self.interior_zone = None;
        self.village.interior = None;
        self.village.talk = None;
        self.home = super::homes::SeatHome::new();
        // The index's travel crosses places itself (home, a body, free flight): one action.
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::index::{DestinationKind, Fact};
    use fatherload_simulation::{
        destination::{DestinationKind as IndexKind, TravelRequest},
        terrain::TilePosition,
    };

    fn destination(name: &str, kind: IndexKind, x: i32) -> Destination {
        Destination {
            kind: DestinationKind::Index(kind),
            category: AtlasCategory::Depth,
            name: name.to_owned(),
            subtitle: String::new(),
            target: AtlasTarget::Home(TilePosition { x, y: 10 }),
            facts: vec![Fact {
                label: "Depth",
                value: "10 m".to_owned(),
            }],
            boss: None,
            creatures: Vec::new(),
            keywords: name.to_lowercase(),
            request: Some(TravelRequest::exact(DestinationId::new(
                kind,
                Place::HOME,
                i64::from(x),
                10,
            ))),
        }
    }

    #[test]
    fn previous_and_next_cycle_through_the_same_kind() {
        let mut atlas = SeatAtlas::new();
        atlas.shown = vec![
            destination("a", IndexKind::Descent, 0),
            destination("b", IndexKind::Stratum, 1),
            destination("c", IndexKind::Descent, 2),
            destination("d", IndexKind::Stratum, 3),
            destination("e", IndexKind::Descent, 4),
        ];
        atlas.selected = 0;
        assert_eq!(atlas.same_kind(1), Some(2));
        assert_eq!(atlas.same_kind(-1), Some(4), "wraps");
        atlas.selected = 3;
        assert_eq!(atlas.same_kind(1), Some(1));
        let names: Vec<&str> = atlas.neighbours().iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["b"], "the only other stratum, once");
    }

    #[test]
    fn a_bookmark_is_the_worlds_by_spot_whatever_its_kind() {
        let mut atlas = SeatAtlas::new();
        let stratum = destination("a", IndexKind::Stratum, 7);
        assert!(!atlas.bookmarked(&stratum));
        // The world keeps the spot; a bookmark listed back is of the Bookmark kind.
        atlas.marks = vec![DestinationId::new(IndexKind::Bookmark, Place::HOME, 7, 10)];
        assert_eq!(atlas.bookmark_index(&stratum), Some(0));
        assert!(!atlas.bookmarked(&destination("b", IndexKind::Stratum, 8)));
    }
}
