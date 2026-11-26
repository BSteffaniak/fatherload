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
//! This module is the Atlas's pure half (no raylib): the destination model and its builders
//! ([`index`]), the search ([`search`]) and the map's geometry ([`map`]). The seat's state and
//! input live in `shell::atlas`, the drawing and the preview textures in `rendering::atlas`.
//!
//! **TODO-SEAM (w14-index):** the destination model here is the Atlas's own thin stand-in for
//! the simulation's destination index (W14-B: destination kinds, selectors, `resolve` to an
//! arrival place and position with preview metadata, `TestModeAction::TravelTo`, bookmarks and
//! the back-stack, saved and replicated). [`index::travel_command`] is the one place a choice
//! becomes a command; it maps onto the test-mode warps that exist today (bands, places, the
//! story). The integrator swaps it for `TravelTo` and the index's selectors, and moves the
//! seat-local bookmarks and recent list onto the index's saved ones.

pub mod hit;
pub mod index;
pub mod map;
pub mod search;

pub use index::{AtlasCategory, AtlasTarget, Destination, DestinationKind, Origin};
