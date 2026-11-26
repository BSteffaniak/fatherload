//! Where the Atlas's parts go on a seat's canvas (art pixels): a pure function of the canvas
//! size, so the tests check every resolution, split-screen view and phone without a window.
//!
//! Three shapes:
//!
//! * **Wide** (a whole window, a side-by-side seat on a big screen): categories down the
//!   left, the result list beside them, the preview and its info card on the right, the
//!   buttons along the bottom.
//! * **Compact** (a quarter seat, a small window): the categories as two rows of tabs over the
//!   list on the left, the preview over the card on the right, the buttons along the bottom.
//! * **Narrow** (a phone held upright, the smallest seat): tabs on top, the preview, then the
//!   list beside a short card, two rows of buttons.
//!
//! In map mode the map takes the categories' and list's place.

use raylib::prelude::Rectangle;

use crate::atlas::hit::AtlasButton;

/// Space between parts.
const GAP: f32 = 3.0;
/// Outer margin.
const MARGIN: f32 = 4.0;
/// The header (title and search box).
const HEADER: f32 = 17.0;
/// A button's height.
pub const BUTTON_HEIGHT: f32 = 16.0;

/// Which shape the Atlas took.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtlasShape {
    Wide,
    Compact,
    Narrow,
}

/// The Atlas's parts on a canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasLayout {
    pub shape: AtlasShape,
    /// The whole Atlas (its backdrop).
    pub frame: Rectangle,
    /// The title.
    pub title: Rectangle,
    pub search: Rectangle,
    /// Each category's tab, in `AtlasCategory::ALL` order.
    pub categories: Vec<Rectangle>,
    /// The result list (empty in map mode).
    pub list: Rectangle,
    /// The map (map mode only).
    pub map: Rectangle,
    pub preview: Rectangle,
    pub card: Rectangle,
    /// Each button, in `AtlasButton::ALL` order.
    pub buttons: Vec<(AtlasButton, Rectangle)>,
    /// One result row's height.
    pub row_height: f32,
}

const fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

const EMPTY: Rectangle = rect(0.0, 0.0, 0.0, 0.0);

impl AtlasLayout {
    /// The layout on a `width` x `height` canvas, with the map open or not.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one block per shape")]
    pub fn compute(width: f32, height: f32, map_open: bool) -> Self {
        let frame = rect(0.0, 0.0, width.max(1.0), height.max(1.0));
        let shape = if width >= 420.0 && height >= 236.0 {
            AtlasShape::Wide
        } else if width >= 300.0 && height >= 170.0 {
            AtlasShape::Compact
        } else {
            AtlasShape::Narrow
        };
        let inner = rect(
            MARGIN,
            MARGIN,
            (width - MARGIN * 2.0).max(1.0),
            (height - MARGIN * 2.0).max(1.0),
        );
        let title_width = if shape == AtlasShape::Narrow {
            44.0
        } else {
            64.0
        };
        let title = rect(inner.x, inner.y, title_width, HEADER);
        let search = rect(
            inner.x + title_width + GAP,
            inner.y,
            inner.width - title_width - GAP,
            HEADER,
        );
        let body_top = inner.y + HEADER + GAP;
        // Buttons: one row when they fit, else two.
        let rows = if shape == AtlasShape::Wide { 1 } else { 2 };
        let buttons_height = BUTTON_HEIGHT * rows as f32 + GAP * (rows - 1) as f32;
        let buttons_top = inner.y + inner.height - buttons_height;
        let buttons = lay_buttons(
            rect(inner.x, buttons_top, inner.width, buttons_height),
            rows,
        );
        let body = rect(
            inner.x,
            body_top,
            inner.width,
            (buttons_top - GAP - body_top).max(1.0),
        );
        let (categories, list, preview, card, row_height) = match shape {
            AtlasShape::Wide => {
                let side = 70.0;
                let categories = column_tabs(rect(body.x, body.y, side, body.height));
                let list_width = (body.width * 0.32).clamp(120.0, 220.0);
                let list = rect(body.x + side + GAP, body.y, list_width, body.height);
                let right_x = list.x + list.width + GAP;
                let right = rect(right_x, body.y, body.x + body.width - right_x, body.height);
                let preview_height = (right.height * 0.58).floor();
                let preview = rect(right.x, right.y, right.width, preview_height);
                let card = rect(
                    right.x,
                    right.y + preview_height + GAP,
                    right.width,
                    right.height - preview_height - GAP,
                );
                (categories, list, preview, card, 20.0)
            }
            AtlasShape::Compact => {
                let left_width = (body.width * 0.42).floor();
                let tabs_height = 2.0 * 12.0 + GAP;
                let categories = grid_tabs(rect(body.x, body.y, left_width, tabs_height), 4);
                let list = rect(
                    body.x,
                    body.y + tabs_height + GAP,
                    left_width,
                    body.height - tabs_height - GAP,
                );
                let right_x = body.x + left_width + GAP;
                let right = rect(right_x, body.y, body.x + body.width - right_x, body.height);
                let preview_height = (right.height * 0.55).floor();
                let preview = rect(right.x, right.y, right.width, preview_height);
                let card = rect(
                    right.x,
                    right.y + preview_height + GAP,
                    right.width,
                    right.height - preview_height - GAP,
                );
                (categories, list, preview, card, 13.0)
            }
            AtlasShape::Narrow => {
                let tabs_height = 2.0 * 11.0 + GAP;
                let categories = grid_tabs(rect(body.x, body.y, body.width, tabs_height), 4);
                let rest_top = body.y + tabs_height + GAP;
                let rest = rect(
                    body.x,
                    rest_top,
                    body.width,
                    body.height - tabs_height - GAP,
                );
                let preview_height = (rest.height * 0.45).floor();
                let preview = rect(rest.x, rest.y, rest.width, preview_height);
                let lower_top = rest.y + preview_height + GAP;
                let lower_height = rest.height - preview_height - GAP;
                let half = ((rest.width - GAP) / 2.0).floor();
                let list = rect(rest.x, lower_top, half, lower_height);
                let card = rect(
                    rest.x + half + GAP,
                    lower_top,
                    rest.width - half - GAP,
                    lower_height,
                );
                (categories, list, preview, card, 12.0)
            }
        };
        let (map, list) = if map_open {
            // The map takes the categories and the list (the tabs stay in the narrow shapes,
            // where they sit above everything).
            let map = match shape {
                AtlasShape::Wide => {
                    let left = categories.first().map_or(list.x, |tab| tab.x);
                    rect(left, list.y, list.x + list.width - left, list.height)
                }
                AtlasShape::Compact | AtlasShape::Narrow => list,
            };
            (map, EMPTY)
        } else {
            (EMPTY, list)
        };
        let categories = if map_open && shape == AtlasShape::Wide {
            Vec::new()
        } else {
            categories
        };
        Self {
            shape,
            frame,
            title,
            search,
            categories,
            list,
            map,
            preview,
            card,
            buttons,
            row_height,
        }
    }

    /// Every part that is shown, with a name (for the overlap tests).
    #[cfg(test)]
    #[must_use]
    pub fn parts(&self) -> Vec<(String, Rectangle)> {
        let mut parts = vec![
            ("title".to_owned(), self.title),
            ("search".to_owned(), self.search),
            ("preview".to_owned(), self.preview),
            ("card".to_owned(), self.card),
        ];
        if self.list.width > 0.0 {
            parts.push(("list".to_owned(), self.list));
        }
        if self.map.width > 0.0 {
            parts.push(("map".to_owned(), self.map));
        }
        for (index, tab) in self.categories.iter().enumerate() {
            parts.push((format!("category {index}"), *tab));
        }
        for (button, area) in &self.buttons {
            parts.push((format!("{button:?}"), *area));
        }
        parts
    }

    /// How many result rows fit the list.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a few dozen rows"
    )]
    pub fn visible_rows(&self) -> usize {
        ((self.list.height - 4.0) / self.row_height)
            .floor()
            .max(0.0) as usize
    }
}

/// The categories as a column of tabs.
fn column_tabs(area: Rectangle) -> Vec<Rectangle> {
    let count = crate::atlas::AtlasCategory::ALL.len() as f32;
    let height = ((area.height - GAP * (count - 1.0)) / count).clamp(10.0, 18.0);
    (0..crate::atlas::AtlasCategory::ALL.len())
        .map(|index| {
            rect(
                area.x,
                area.y + index as f32 * (height + GAP),
                area.width,
                height,
            )
        })
        .collect()
}

/// The categories as a grid of tabs, `columns` across.
fn grid_tabs(area: Rectangle, columns: usize) -> Vec<Rectangle> {
    let count = crate::atlas::AtlasCategory::ALL.len();
    let rows = count.div_ceil(columns);
    let width = ((area.width - GAP * (columns - 1) as f32) / columns as f32).floor();
    let height = ((area.height - GAP * (rows - 1) as f32) / rows as f32).floor();
    (0..count)
        .map(|index| {
            let (column, row) = (index % columns, index / columns);
            rect(
                area.x + column as f32 * (width + GAP),
                area.y + row as f32 * (height + GAP),
                width,
                height,
            )
        })
        .collect()
}

/// The buttons, spread over `rows` rows of `area`.
fn lay_buttons(area: Rectangle, rows: usize) -> Vec<(AtlasButton, Rectangle)> {
    let count = AtlasButton::ALL.len();
    let per_row = count.div_ceil(rows);
    let width = ((area.width - GAP * (per_row - 1) as f32) / per_row as f32).floor();
    AtlasButton::ALL
        .iter()
        .enumerate()
        .map(|(index, button)| {
            let (column, row) = (index % per_row, index / per_row);
            (
                *button,
                rect(
                    area.x + column as f32 * (width + GAP),
                    area.y + row as f32 * (BUTTON_HEIGHT + GAP),
                    width,
                    BUTTON_HEIGHT,
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: Rectangle, b: Rectangle) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    /// Canvas sizes (art pixels) the Atlas meets: whole windows at the UI scales, split-screen
    /// halves and quarters, the smallest seat canvas and phones both ways.
    const SIZES: [(f32, f32); 10] = [
        (640.0, 360.0),
        (480.0, 270.0),
        (853.0, 480.0),
        (320.0, 360.0),
        (640.0, 180.0),
        (320.0, 180.0),
        (232.0, 160.0),
        (400.0, 185.0),
        (195.0, 422.0),
        (300.0, 640.0),
    ];

    #[test]
    fn no_part_overlaps_another_or_leaves_the_canvas() {
        for (width, height) in SIZES {
            for map_open in [false, true] {
                let layout = AtlasLayout::compute(width, height, map_open);
                let parts = layout.parts();
                for (name, area) in &parts {
                    assert!(
                        area.width >= 8.0 && area.height >= 8.0,
                        "{name} too small at {width}x{height}: {area:?}"
                    );
                    assert!(
                        area.x >= 0.0
                            && area.y >= 0.0
                            && area.x + area.width <= width + 0.01
                            && area.y + area.height <= height + 0.01,
                        "{name} leaves {width}x{height}: {area:?}"
                    );
                }
                for (index, (a_name, a)) in parts.iter().enumerate() {
                    for (b_name, b) in parts.iter().skip(index + 1) {
                        assert!(
                            !overlap(*a, *b),
                            "{a_name} overlaps {b_name} at {width}x{height} (map {map_open})"
                        );
                    }
                }
                assert!(map_open || layout.visible_rows() >= 3, "{width}x{height}");
                assert_eq!(layout.map.width > 0.0, map_open);
            }
        }
    }

    #[test]
    fn the_shape_follows_the_room() {
        assert_eq!(
            AtlasLayout::compute(640.0, 360.0, false).shape,
            AtlasShape::Wide
        );
        assert_eq!(
            AtlasLayout::compute(320.0, 180.0, false).shape,
            AtlasShape::Compact
        );
        assert_eq!(
            AtlasLayout::compute(232.0, 160.0, false).shape,
            AtlasShape::Narrow
        );
        assert_eq!(
            AtlasLayout::compute(195.0, 422.0, false).shape,
            AtlasShape::Narrow
        );
        // The preview is the biggest part of a whole window.
        let wide = AtlasLayout::compute(640.0, 360.0, false);
        assert!(wide.preview.width >= 280.0 && wide.preview.height >= 150.0);
    }
}
