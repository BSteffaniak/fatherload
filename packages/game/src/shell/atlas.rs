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

use fatherload_simulation::{PlayerCommand, PlayerId, WorldState, place::Place, space::SpaceTile};

use super::{ModalScreen, SeatContext, SeatUi, ShellRequest, SoundCue};
use crate::{
    atlas::{
        AtlasCategory, AtlasTarget, Destination, DestinationKind, Origin,
        hit::{AtlasButton, AtlasHit},
        index::{self, depth_spot, space_spot},
        map::{DepthMap, GalaxyMap, MapKind},
        search,
    },
    input::PlayerInput,
};

/// Bookmarks and recent places a seat keeps.
pub const ATLAS_LIST_CAP: usize = 24;
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
    /// The seat's bookmarks, oldest first.
    pub bookmarks: Vec<Destination>,
    /// Places the seat travelled to, newest first.
    pub recent: Vec<Destination>,
    /// Places the seat travelled from, newest last (Back pops it).
    pub back_stack: Vec<Destination>,
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
            bookmarks: Vec::new(),
            recent: Vec::new(),
            back_stack: Vec::new(),
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
        [-1, 1]
            .into_iter()
            .filter_map(|step| self.same_kind(step))
            .filter(|index| *index != self.selected)
            .filter_map(|index| self.shown.get(index))
            .collect()
    }

    /// Whether `destination` is bookmarked.
    #[must_use]
    pub fn bookmarked(&self, destination: &Destination) -> bool {
        let key = destination.target.key();
        self.bookmarks
            .iter()
            .any(|bookmark| bookmark.target.key() == key)
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

    /// The destinations of `category` (built on first use).
    fn list(
        &mut self,
        world: &WorldState,
        player: Option<PlayerId>,
        category: AtlasCategory,
    ) -> Vec<Destination> {
        match category {
            AtlasCategory::Bookmarks => return self.bookmarks.clone(),
            AtlasCategory::Recent => return self.recent.clone(),
            _ => {}
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
            let mut every = Vec::new();
            for category in AtlasCategory::ALL {
                every.extend(self.list(world, player, category));
            }
            let mut seen = std::collections::BTreeSet::new();
            every.retain(|destination| seen.insert(destination.target.key()));
            search::filter(&every, &self.search)
                .into_iter()
                .map(|index| every[index].clone())
                .collect()
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

    fn toggle_bookmark(&mut self) -> Option<bool> {
        let destination = self.current()?.clone();
        let key = destination.target.key();
        if let Some(index) = self
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.target.key() == key)
        {
            self.bookmarks.remove(index);
            return Some(false);
        }
        self.bookmarks
            .push(destination.filed_under(AtlasCategory::Bookmarks));
        if self.bookmarks.len() > ATLAS_LIST_CAP {
            self.bookmarks.remove(0);
        }
        Some(true)
    }

    /// Records a trip from `from` to `to` (the recent list and the back-stack).
    fn record_trip(&mut self, from: Option<Destination>, to: &Destination) {
        let key = to.target.key();
        self.recent.retain(|recent| recent.target.key() != key);
        self.recent.insert(0, to.filed_under(AtlasCategory::Recent));
        self.recent.truncate(ATLAS_LIST_CAP);
        if let Some(from) = from {
            self.back_stack
                .push(from.filed_under(AtlasCategory::Recent));
            if self.back_stack.len() > ATLAS_LIST_CAP {
                self.back_stack.remove(0);
            }
        }
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
            MapKind::Galaxy => snap_to_body(world, map.galaxy, map.galaxy.pos_at(cursor)),
        });
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
    map: GalaxyMap,
    pos: fatherload_simulation::position::WorldPos,
) -> Destination {
    let galaxy = fatherload_simulation::space::Space::shared(world.world_seed());
    let reach = map.radius / 40;
    let tile = SpaceTile::of_pos(pos);
    if let Some((reference, _)) = galaxy.systems_near(tile, reach).first()
        && let Some(system) = galaxy.system(*reference)
        && system.centre.distance(tile) <= reach as f64
    {
        let mut spot = space_spot(world, system.centre.pos());
        spot.name.clone_from(&system.name);
        spot.kind = DestinationKind::StarSystem;
        spot.subtitle = format!(
            "{} \u{b7} {} planets",
            system.class.name(),
            system.planet_count()
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
        if self.atlas.shown.is_empty() && self.atlas.search.is_empty() {
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
            AtlasButton::Bookmark => match self.atlas.toggle_bookmark() {
                Some(true) => {
                    "Bookmarked.".clone_into(&mut self.message);
                    cues.push(SoundCue::Ui);
                }
                Some(false) => {
                    "Bookmark removed.".clone_into(&mut self.message);
                    cues.push(SoundCue::UiBack);
                    if self.atlas.category == AtlasCategory::Bookmarks
                        && self.atlas.search.is_empty()
                    {
                        self.atlas.refresh(ctx.world, ctx.player);
                    }
                }
                None => {}
            },
            AtlasButton::Back => {
                let Some(back) = self.atlas.back_stack.pop() else {
                    "Nowhere to go back to yet.".clone_into(&mut self.message);
                    return;
                };
                let Some(player) = ctx.player else {
                    return;
                };
                let Some(command) = index::travel_command(ctx.world, &back, player) else {
                    return;
                };
                self.send_travel(command, ctx, requests);
                self.message = format!("Back to {}.", back.name);
                cues.push(SoundCue::Ui);
                self.close_atlas();
            }
            AtlasButton::Map => {
                if self.atlas.map.is_some() {
                    self.atlas.map = None;
                } else {
                    // The galaxy once the seat is out there (or browsing space), else the
                    // depths of home.
                    let kind = if ctx.world.space_enabled()
                        && (!self.atlas.origin.place.is_home_store()
                            || self.atlas.category == AtlasCategory::Space)
                    {
                        MapKind::Galaxy
                    } else {
                        MapKind::Depth
                    };
                    self.atlas.open_map(ctx.world, kind);
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
        let from = here(ctx.world, player);
        self.atlas.record_trip(from, destination);
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
        requests.push(ShellRequest::Action {
            seat: ctx.seat,
            command,
        });
    }
}

/// Where `player` stands now, as a destination to come back to.
fn here(world: &WorldState, player: PlayerId) -> Option<Destination> {
    let target = index::resolve_player(world, player)?;
    let mut back = match target {
        AtlasTarget::Home(tile) => depth_spot(world, tile),
        AtlasTarget::Space { pos, .. } => space_spot(world, pos),
        AtlasTarget::Body { .. } | AtlasTarget::Player(_) | AtlasTarget::Story => {
            let mut spot = space_spot(world, Origin::of(world, player)?.space);
            spot.target = target;
            "Where you were".clone_into(&mut spot.name);
            spot
        }
    };
    back.subtitle = format!("Left from: {}", back.subtitle);
    Some(back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::index::Fact;
    use fatherload_simulation::terrain::TilePosition;

    fn destination(name: &str, kind: DestinationKind, x: i32) -> Destination {
        Destination {
            kind,
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
        }
    }

    #[test]
    fn previous_and_next_cycle_through_the_same_kind() {
        let mut atlas = SeatAtlas::new();
        atlas.shown = vec![
            destination("a", DestinationKind::Descent, 0),
            destination("b", DestinationKind::Stratum, 1),
            destination("c", DestinationKind::Descent, 2),
            destination("d", DestinationKind::Stratum, 3),
            destination("e", DestinationKind::Descent, 4),
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
    fn bookmarks_toggle_and_trips_fill_recent_and_the_back_stack() {
        let mut atlas = SeatAtlas::new();
        atlas.shown = vec![destination("a", DestinationKind::Descent, 0)];
        assert_eq!(atlas.toggle_bookmark(), Some(true));
        assert!(atlas.bookmarked(&atlas.shown[0].clone()));
        assert_eq!(atlas.bookmarks[0].category, AtlasCategory::Bookmarks);
        assert_eq!(atlas.toggle_bookmark(), Some(false));
        assert!(atlas.bookmarks.is_empty());

        let here = destination("here", DestinationKind::DepthSpot, 9);
        atlas.record_trip(Some(here), &destination("a", DestinationKind::Descent, 0));
        atlas.record_trip(None, &destination("b", DestinationKind::Descent, 1));
        atlas.record_trip(None, &destination("a", DestinationKind::Descent, 0));
        let recent: Vec<&str> = atlas.recent.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(recent, ["a", "b"], "newest first, each once");
        assert_eq!(atlas.back_stack.len(), 1);
        assert_eq!(atlas.back_stack[0].name, "here");
    }
}
