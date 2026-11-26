//! The Atlas (Wave 14 W14-C): a test-mode travel browser.
//!
//! A test-mode world's seat opens a full-screen Atlas (the pause menu's "Atlas" entry, F2 or
//! the touch ATLAS button): destinations sorted into categories down the side (Depth, Places,
//! Bosses, Settlements, Space, Players, Bookmarks, Recent), a type-ahead search box, a live
//! preview of the selected destination drawn with the real tile, sprite and space renderers
//! from the deterministic world, an info card and the buttons Go, Previous/Next (the same
//! kind), Random, Bookmark, Back (to where you were) and Map. Map mode zooms out to a depth map
//! of all eight Descents (or, once you are in space, the galaxy round you); the cursor moves
//! the preview and Go travels to the spot.
//!
//! This module is the Atlas's pure half (no raylib): the destinations as the simulation's
//! destination index answers them ([`index`]: its candidates per category, its search, the map
//! spots as exact ids), the ranking of search results ([`search`]) and the map's geometry
//! ([`map`]). The seat's state and input live in `shell::atlas`, the drawing and the preview
//! textures in `rendering::atlas`.
//!
//! One path (Wave 14 integration): [`index::travel_command`] is the one place a choice becomes
//! a command, the index's `TestModeAction::TravelTo(TravelRequest::exact(id))` (the palette and
//! `--start-at` send the same action); Back is `TravelBack` and Bookmark is `Bookmark(id)` /
//! `RemoveBookmark`, so the bookmarks and the back-stack are the world's (saved and
//! replicated). Only the galaxy story's goal keeps its own warp.

pub mod hit;
pub mod index;
pub mod map;
pub mod search;

pub use index::{AtlasCategory, AtlasTarget, Destination, Origin};
