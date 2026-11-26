//! The flight HUD's layout (Wave 13 art-audio): plain rectangles, worked out before anything is
//! drawn, so the meters, the deflector panel, the docking guide, the location plate and its
//! banners, the nav panel, the bounties, the prompt bar and every edge marker's arrow and label
//! keep clear of each other at any size: a desktop window, each split-screen seat, a phone.
//!
//! [`SpaceHudLayout`] places the fixed panels; [`SpaceHudLayout::banner`] fits a centred banner
//! between the panels at its height (or under them); [`Markers`] places the arrows and labels
//! that point at waypoints, wormholes, crewmates, ships and salvage, sliding a marker along the
//! view's edge until it clears every panel and every marker placed before it. The tests lay the
//! HUD out at many resolutions and check that nothing overlaps.

#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    reason = "screen layout in float points"
)]

use raylib::prelude::{Rectangle, Vector2};

/// Height of one line of HUD text (the body face's glyphs, with their shadow).
pub const LABEL_HEIGHT: f32 = 10.0;
/// Room kept between panels.
pub const GAP: f32 = 4.0;
/// Views narrower than this lay the HUD out for a small screen (short texts, no nav panel).
pub const NARROW_WIDTH: f32 = 420.0;
/// The meters panel (top left).
pub const METERS_SIZE: (f32, f32) = (150.0, 60.0);
/// The nav panel (top right): its chart and three lines under it.
pub const NAV_SIZE: (f32, f32) = (136.0, 132.0);
/// The location plate's height (two lines).
pub const PLATE_HEIGHT: f32 = 28.0;
/// One row of button prompts.
pub const PROMPT_ROW: f32 = 14.0;
/// The docking guide (taller once every check is met: the dock prompt).
pub const DOCKING_HEIGHT: (f32, f32) = (50.0, 62.0);
/// Space a marker's arrow keeps from the view's edge.
const EDGE_INSET: f32 = 8.0;
/// Space a marker keeps from panels and other markers.
const MARKER_MARGIN: f32 = 2.0;

/// What the HUD shows this frame, as far as its layout cares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudSpec<'a> {
    /// The phone's touch controls are up (the plate drops under their top row, no prompt bar).
    pub touch: bool,
    /// A nav suite is fitted (the nav panel shows when the view is wide enough).
    pub nav: bool,
    /// The location plate's width.
    pub plate_width: f32,
    /// The deflector panel carries a missile rack (a second row).
    pub rack: bool,
    /// The docking guide shows (`Some(ready)`).
    pub docking: Option<bool>,
    /// Missions under way (the bounties panel, bottom left).
    pub missions: usize,
    /// Rows of button prompts, and the widest row's width.
    pub prompt_rows: usize,
    pub prompt_width: f32,
    /// The phone's touch controls on this view (UI points): banners and markers keep clear
    /// of them.
    pub controls: &'a [Rectangle],
}

/// Every fixed panel of the flight HUD (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceHudLayout {
    pub rect: Rectangle,
    pub narrow: bool,
    pub meters: Rectangle,
    /// The deflector and missile rack, under the meters.
    pub combat: Rectangle,
    /// The docking guide, under the deflector.
    pub docking: Option<Rectangle>,
    /// "No nav suite" under the left column (the line's box).
    pub note: Option<Rectangle>,
    pub nav: Option<Rectangle>,
    pub plate: Rectangle,
    pub bounties: Option<Rectangle>,
    /// The prompt bar's rows (bottom right).
    pub prompts: Option<Rectangle>,
    /// Banners placed so far (the plate's stack and the fight's), each one an obstacle for the
    /// next and for the markers.
    pub banners: Vec<Rectangle>,
    /// The touch controls (obstacles, not panels of the HUD's own).
    pub controls: Vec<Rectangle>,
}

/// Whether two rectangles overlap (touching edges do not).
#[must_use]
pub fn overlaps(a: Rectangle, b: Rectangle) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// `a` grown by `by` on every side.
#[must_use]
pub const fn grown(a: Rectangle, by: f32) -> Rectangle {
    Rectangle {
        x: a.x - by,
        y: a.y - by,
        width: a.width + by * 2.0,
        height: a.height + by * 2.0,
    }
}

const fn bottom(a: Rectangle) -> f32 {
    a.y + a.height
}

const fn right(a: Rectangle) -> f32 {
    a.x + a.width
}

/// Whether `inner` lies inside `outer`.
#[must_use]
pub fn inside(inner: Rectangle, outer: Rectangle) -> bool {
    inner.x >= outer.x - 0.01
        && inner.y >= outer.y - 0.01
        && right(inner) <= right(outer) + 0.01
        && bottom(inner) <= bottom(outer) + 0.01
}

/// Where a message toast rises in a seat's view (`screen::draw_toast`: centred, at most 300
/// points wide, resting 28 points above the bottom, up to three lines).
#[must_use]
pub fn toast_band(rect: Rectangle) -> Rectangle {
    let width = (rect.width - 24.0).min(300.0);
    let height = 40.0;
    Rectangle::new(
        rect.x + (rect.width - width) * 0.5,
        bottom(rect) - 28.0 - height,
        width,
        height,
    )
}

impl SpaceHudLayout {
    /// Lays the fixed panels out in `rect` (UI points).
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "each panel in reading order")]
    pub fn new(rect: Rectangle, spec: &HudSpec) -> Self {
        let narrow = rect.width < NARROW_WIDTH;
        let meters = Rectangle::new(rect.x + 4.0, rect.y + 4.0, METERS_SIZE.0, METERS_SIZE.1);
        let nav = (spec.nav && !narrow).then(|| {
            Rectangle::new(
                right(rect) - NAV_SIZE.0 - 4.0,
                rect.y + 4.0,
                NAV_SIZE.0,
                NAV_SIZE.1,
            )
        });
        // The prompt bar's rows, bottom right (none on a touch screen: the buttons say it).
        let prompts = (!spec.touch && spec.prompt_rows > 0).then(|| {
            let height = spec.prompt_rows as f32 * PROMPT_ROW;
            let width = spec.prompt_width.min(rect.width - 12.0);
            Rectangle::new(
                right(rect) - width - 6.0,
                bottom(rect) - 18.0 - (spec.prompt_rows as f32 - 1.0) * PROMPT_ROW,
                width,
                height,
            )
        });
        let floor = prompts.map_or_else(|| bottom(rect) - 4.0, |bar| bar.y - GAP);
        let mut layout = Self {
            rect,
            narrow,
            meters,
            combat: Rectangle::new(0.0, 0.0, 0.0, 0.0),
            docking: None,
            note: None,
            nav,
            plate: Rectangle::new(0.0, 0.0, 0.0, 0.0),
            bounties: None,
            prompts,
            banners: Vec::new(),
            controls: spec.controls.to_vec(),
        };
        // The plate at the top between the meters and the nav panel (under the touch top row
        // on a touch screen), or, in a view too narrow for that, first in the left column.
        let top = if spec.touch {
            rect.y + 50.0
        } else {
            rect.y + 4.0
        };
        let width = spec.plate_width.min(rect.width - 8.0);
        let at_top = layout.fit(top, width, PLATE_HEIGHT);
        let mut cursor = bottom(meters) + 2.0;
        layout.plate = if (at_top.y - top).abs() < 0.5 {
            at_top
        } else {
            let mut plate = Rectangle::new(meters.x, cursor, width, PLATE_HEIGHT);
            // Clear of a phone's buttons beside the meters.
            if spec
                .controls
                .iter()
                .any(|control| overlaps(*control, plate))
            {
                plate = layout.fit(cursor, width, PLATE_HEIGHT);
            }
            if (plate.y - cursor).abs() < 0.5 && (plate.x - meters.x).abs() < 0.5 {
                cursor = bottom(plate) + GAP;
            }
            plate
        };
        // The left column: the deflector, the docking guide, the nav note.
        layout.combat = Rectangle::new(
            meters.x,
            cursor,
            METERS_SIZE.0,
            if spec.rack { 26.0 } else { 16.0 },
        );
        cursor = bottom(layout.combat) + GAP;
        layout.docking = spec.docking.and_then(|ready| {
            let height = if ready {
                DOCKING_HEIGHT.1
            } else {
                DOCKING_HEIGHT.0
            };
            let panel = Rectangle::new(meters.x, cursor, METERS_SIZE.0, height);
            (bottom(panel) <= floor).then(|| {
                cursor = bottom(panel) + GAP;
                panel
            })
        });
        layout.note = (!spec.nav)
            .then(|| {
                let note = Rectangle::new(
                    meters.x,
                    cursor,
                    (rect.width - 12.0).min(250.0),
                    LABEL_HEIGHT,
                );
                (bottom(note) <= floor).then_some(note)
            })
            .flatten();
        cursor = layout.note.map_or(cursor, |note| bottom(note) + GAP);
        // The bounties, bottom left above the prompts and clear of where a message toast rises
        // (only where there is room for them).
        let toast = toast_band(rect);
        layout.bounties = (spec.missions > 0 && !spec.touch && rect.height >= 200.0)
            .then(|| {
                let width = (rect.width * 0.36).clamp(150.0, 230.0);
                let height = 14.0 + spec.missions as f32 * 10.0;
                let mut panel = Rectangle::new(rect.x + 4.0, floor - height, width, height);
                if overlaps(panel, toast) {
                    panel.y = toast.y - GAP - height;
                }
                let clear = panel.y >= cursor
                    && prompts.is_none_or(|bar| !overlaps(grown(panel, 1.0), bar));
                clear.then_some(panel)
            })
            .flatten();
        layout
    }

    /// Every panel placed (the plate and banners included).
    #[must_use]
    pub fn panels(&self) -> Vec<Rectangle> {
        let mut out = vec![self.meters, self.combat, self.plate];
        out.extend(self.docking);
        out.extend(self.note);
        out.extend(self.nav);
        out.extend(self.bounties);
        out.extend(self.prompts);
        out.extend(self.banners.iter().copied());
        out
    }

    /// A box `width` x `height` as near the middle of the view at `top` as the panels there
    /// allow, or under them when they leave no room.
    #[must_use]
    pub fn fit(&self, top: f32, width: f32, height: f32) -> Rectangle {
        // Inside the view if it can be, else under the panels as far as it takes.
        self.fit_within(top, width, height, true)
            .or_else(|| self.fit_within(top, width, height, false))
            .unwrap_or_else(|| {
                let width = width.min(self.rect.width - 4.0);
                Rectangle::new(
                    self.rect.x + (self.rect.width - width) * 0.5,
                    top,
                    width,
                    height,
                )
            })
    }

    /// [`Self::fit`]'s search, staying above the view's bottom when `bounded`.
    fn fit_within(&self, top: f32, width: f32, height: f32, bounded: bool) -> Option<Rectangle> {
        let width = width.min(self.rect.width - 4.0);
        let centre = self.rect.x + self.rect.width * 0.5;
        let mut obstacles = self.panels();
        obstacles.extend(self.controls.iter().copied());
        let mut y = top;
        for _ in 0..12 {
            let band = Rectangle::new(self.rect.x, y, self.rect.width, height);
            let (mut low, mut high) = (self.rect.x + 2.0, right(self.rect) - 2.0);
            let mut next: Option<f32> = None;
            for obstacle in obstacles
                .iter()
                .filter(|obstacle| obstacle.width > 0.0 && overlaps(grown(**obstacle, 1.0), band))
            {
                if obstacle.x + obstacle.width * 0.5 < centre {
                    low = low.max(right(*obstacle) + GAP);
                } else {
                    high = high.min(obstacle.x - GAP);
                }
                let below = bottom(*obstacle) + GAP;
                next = Some(next.map_or(below, |at: f32| at.min(below)));
            }
            if high - low >= width {
                let x = (centre - width * 0.5).clamp(low, high - width);
                return Some(Rectangle::new(x, y, width, height));
            }
            match next {
                Some(below) if below > y && (!bounded || below + height <= bottom(self.rect)) => {
                    y = below;
                }
                _ => break,
            }
        }
        (!bounded).then(|| Rectangle::new(centre - width * 0.5, y, width, height))
    }

    /// Places a centred banner at `top` (see [`Self::fit`]) and keeps it as an obstacle.
    pub fn banner(&mut self, top: f32, width: f32, height: f32) -> Rectangle {
        let placed = self.fit(top, width, height);
        self.banners.push(placed);
        placed
    }

    /// Where edge markers may go: below the plate's stack (`top`) and above the prompts.
    #[must_use]
    pub fn lanes(&self, top: f32) -> Rectangle {
        let floor = self
            .prompts
            .map_or_else(|| bottom(self.rect) - 4.0, |bar| bar.y - GAP);
        let top = top.max(self.rect.y) + GAP;
        Rectangle::new(
            self.rect.x + 6.0,
            top,
            self.rect.width - 12.0,
            (floor - top).max(LABEL_HEIGHT * 3.0),
        )
    }

    /// The edge markers' layout for this frame, below `top`.
    #[must_use]
    pub fn markers(&self, top: f32) -> Markers {
        let mut reserved = self.panels();
        reserved.extend(self.controls.iter().copied());
        Markers {
            lanes: self.lanes(top),
            reserved,
            placed: Vec::new(),
        }
    }
}

/// Where a marker points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    /// At a thing in view (UI points).
    At(Vector2),
    /// Off the view, along this direction from the view's middle (screen axes).
    Toward(f32, f32),
}

/// A marker as drawn: a ring round a thing in view or an arrow at the view's edge, and its
/// label's box (`None` when no clear spot was left for it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkerSpot {
    pub ring: Option<Vector2>,
    /// The arrow's tip, then its two back corners.
    pub arrow: Option<[Vector2; 3]>,
    pub label: Option<Rectangle>,
    /// Which of the label's texts fit (the first is the full one; later ones are shorter).
    pub variant: usize,
}

/// The ring's outer radius round a marked thing in view.
pub const RING_RADIUS: f32 = 6.5;

impl MarkerSpot {
    /// The box round its ring or arrow.
    #[must_use]
    pub fn mark_box(&self) -> Option<Rectangle> {
        if let Some(at) = self.ring {
            return Some(Rectangle::new(
                at.x - RING_RADIUS,
                at.y - RING_RADIUS,
                RING_RADIUS * 2.0,
                RING_RADIUS * 2.0,
            ));
        }
        self.arrow.map(|points| {
            let (mut low_x, mut low_y) = (f32::MAX, f32::MAX);
            let (mut high_x, mut high_y) = (f32::MIN, f32::MIN);
            for point in points {
                low_x = low_x.min(point.x);
                low_y = low_y.min(point.y);
                high_x = high_x.max(point.x);
                high_y = high_y.max(point.y);
            }
            Rectangle::new(low_x, low_y, high_x - low_x, high_y - low_y)
        })
    }
}

/// The edge markers of one frame (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct Markers {
    pub lanes: Rectangle,
    /// Panels to keep clear of.
    pub reserved: Vec<Rectangle>,
    /// Marks and labels placed so far.
    pub placed: Vec<Rectangle>,
}

impl Markers {
    fn clear(&self, area: Rectangle) -> bool {
        // An arrow on the lanes' edge may poke a little past them (they stay inside the view).
        inside(area, grown(self.lanes, 4.0))
            && self
                .reserved
                .iter()
                .chain(&self.placed)
                .all(|taken| !overlaps(grown(area, MARKER_MARGIN), *taken))
    }

    /// Places a marker aimed at `aim` with a label, clear of the panels and the markers placed
    /// before it: the label's texts are `label_widths` wide, the full one first, then shorter
    /// ones to fall back on where the full one finds no room ([`MarkerSpot::variant`]).
    pub fn place(&mut self, aim: Aim, label_widths: &[f32]) -> MarkerSpot {
        let lanes_width = self.lanes.width;
        let widths: Vec<f32> = label_widths
            .iter()
            .map(|width| width.min(lanes_width))
            .collect();
        let label_width = widths.as_slice();
        match aim {
            Aim::At(at) => {
                let ring = Rectangle::new(
                    at.x - RING_RADIUS,
                    at.y - RING_RADIUS,
                    RING_RADIUS * 2.0,
                    RING_RADIUS * 2.0,
                );
                let hidden = self
                    .reserved
                    .iter()
                    .chain(&self.placed)
                    .any(|panel| overlaps(grown(ring, MARKER_MARGIN), *panel));
                if !hidden && inside(ring, self.lanes) {
                    return self.place_ring(at, label_width);
                }
                // Under a panel or another marker: an arrow at the edge toward it instead.
                let centre = self.centre();
                self.place_arrow((at.x - centre.x, at.y - centre.y), label_width)
            }
            Aim::Toward(dx, dy) => self.place_arrow((dx, dy), label_width),
        }
    }

    fn centre(&self) -> Vector2 {
        Vector2::new(
            self.lanes.x + self.lanes.width * 0.5,
            self.lanes.y + self.lanes.height * 0.5,
        )
    }

    fn place_ring(&mut self, at: Vector2, label_widths: &[f32]) -> MarkerSpot {
        let ring = Rectangle::new(
            at.x - RING_RADIUS,
            at.y - RING_RADIUS,
            RING_RADIUS * 2.0,
            RING_RADIUS * 2.0,
        );
        // The label beside the ring: right, left, under, over.
        let found = label_widths
            .iter()
            .enumerate()
            .find_map(|(variant, &width)| {
                [
                    (at.x + 9.0, at.y - 4.0),
                    (at.x - 9.0 - width, at.y - 4.0),
                    (at.x - width * 0.5, at.y + 9.0),
                    (at.x - width * 0.5, at.y - 9.0 - LABEL_HEIGHT),
                ]
                .into_iter()
                .map(|(x, y)| Rectangle::new(x, y, width, LABEL_HEIGHT))
                .find(|label| self.clear(*label) && !overlaps(*label, ring))
                .map(|label| (variant, label))
            });
        self.placed.push(ring);
        self.placed.extend(found.map(|(_, label)| label));
        MarkerSpot {
            ring: Some(at),
            arrow: None,
            label: found.map(|(_, label)| label),
            variant: found.map_or(0, |(variant, _)| variant),
        }
    }

    /// The arrow and label for an arrow whose tip is at `tip`, pointing along `(ux, uy)`, its
    /// label at spot `side` (0: inside the arrow, behind it; 1-4: beside the tip, left,
    /// right, under, over).
    fn arrow_at(
        &self,
        tip: Vector2,
        (ux, uy): (f32, f32),
        label_width: f32,
        side: usize,
    ) -> MarkerSpot {
        let left = Vector2::new(tip.x - ux * 9.0 - uy * 5.0, tip.y - uy * 9.0 + ux * 5.0);
        let right = Vector2::new(tip.x - ux * 9.0 + uy * 5.0, tip.y - uy * 9.0 - ux * 5.0);
        let centre = match side {
            1 => Vector2::new(tip.x - 14.0 - label_width * 0.5, tip.y),
            2 => Vector2::new(tip.x + 14.0 + label_width * 0.5, tip.y),
            3 => Vector2::new(tip.x, tip.y + 12.0 + LABEL_HEIGHT * 0.5),
            4 => Vector2::new(tip.x, tip.y - 12.0 - LABEL_HEIGHT * 0.5),
            _ => {
                // Inside the arrow, clear of it.
                let back =
                    14.0 + label_width * 0.5 * ux.abs() + (LABEL_HEIGHT * 0.5 + 4.0) * uy.abs();
                Vector2::new(tip.x - ux * back, tip.y - uy * back)
            }
        };
        let lanes = self.lanes;
        let label = Rectangle::new(
            (centre.x - label_width * 0.5).clamp(lanes.x, right_of(lanes) - label_width),
            (centre.y - LABEL_HEIGHT * 0.5).clamp(lanes.y, bottom(lanes) - LABEL_HEIGHT),
            label_width,
            LABEL_HEIGHT,
        );
        // A label beside the tip stays beside it (pushed off by the lanes' edge, it would
        // float away from its arrow).
        let moved = (label.x + label_width * 0.5 - centre.x).abs()
            + (label.y + LABEL_HEIGHT * 0.5 - centre.y).abs();
        MarkerSpot {
            ring: None,
            arrow: Some([tip, right, left]),
            label: (side == 0 || moved < 1.0).then_some(label),
            variant: 0,
        }
    }

    fn place_arrow(&mut self, (dx, dy): (f32, f32), label_widths: &[f32]) -> MarkerSpot {
        let length = dx.hypot(dy).max(1e-3);
        let (ux, uy) = (dx / length, dy / length);
        let lanes = self.lanes;
        let (half_w, half_h) = (
            lanes.width * 0.5 - EDGE_INSET,
            lanes.height * 0.5 - EDGE_INSET,
        );
        let centre = self.centre();
        let reach = (half_w / ux.abs().max(1e-3)).min(half_h / uy.abs().max(1e-3));
        let tip = Vector2::new(centre.x + ux * reach, centre.y + uy * reach);
        // Slide along the edge (the inset rectangle's perimeter), nearest first both ways.
        let perimeter = Perimeter {
            x: centre.x - half_w,
            y: centre.y - half_h,
            width: half_w * 2.0,
            height: half_h * 2.0,
        };
        let start = perimeter.position_of(tip);
        let total = perimeter.length();
        let step = 5.0;
        let tries = (total / step) as usize;
        let fits = |spot: &MarkerSpot| {
            spot.mark_box()
                .is_some_and(|mark| self.clear(mark) && spot.label.is_some_and(|l| self.clear(l)))
        };
        let mut chosen = None;
        'widths: for (variant, &label_width) in label_widths.iter().enumerate() {
            for index in 0..=tries {
                let offset =
                    step * index.div_ceil(2) as f32 * if index % 2 == 0 { 1.0 } else { -1.0 };
                let tip = perimeter.point_at(start + offset);
                for side in 0..5 {
                    let spot = self.arrow_at(tip, (ux, uy), label_width, side);
                    let label_clear = spot
                        .label
                        .zip(spot.mark_box())
                        .is_some_and(|(l, m)| !overlaps(grown(l, MARKER_MARGIN), m));
                    if label_clear && fits(&spot) {
                        chosen = Some(MarkerSpot { variant, ..spot });
                        break 'widths;
                    }
                }
            }
        }
        // Nowhere clear for the label: the arrow alone, where it clears.
        let spot = chosen.unwrap_or_else(|| {
            (0..=tries)
                .map(|index| {
                    let offset =
                        step * index.div_ceil(2) as f32 * if index % 2 == 0 { 1.0 } else { -1.0 };
                    MarkerSpot {
                        label: None,
                        ..self.arrow_at(perimeter.point_at(start + offset), (ux, uy), 0.0, 0)
                    }
                })
                .find(|spot| spot.mark_box().is_some_and(|mark| self.clear(mark)))
                .unwrap_or(MarkerSpot {
                    ring: None,
                    arrow: None,
                    label: None,
                    variant: 0,
                })
        });
        self.placed.extend(spot.mark_box());
        self.placed.extend(spot.label);
        spot
    }
}

const fn right_of(a: Rectangle) -> f32 {
    right(a)
}

/// A rectangle's outline walked clockwise from its top-left corner.
struct Perimeter {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Perimeter {
    fn length(&self) -> f32 {
        2.0 * (self.width + self.height)
    }

    /// How far along the outline `point` (on it) is.
    fn position_of(&self, point: Vector2) -> f32 {
        let (w, h) = (self.width, self.height);
        let (px, py) = (point.x - self.x, point.y - self.y);
        let near = |a: f32, b: f32| (a - b).abs() < 0.5;
        if near(py, 0.0) {
            px.clamp(0.0, w)
        } else if near(px, w) {
            w + py.clamp(0.0, h)
        } else if near(py, h) {
            w + h + (w - px).clamp(0.0, w)
        } else {
            2.0 * w + h + (h - py).clamp(0.0, h)
        }
    }

    /// The point `along` the outline (wrapping).
    fn point_at(&self, along: f32) -> Vector2 {
        let (w, h) = (self.width, self.height);
        let s = along.rem_euclid(self.length().max(1e-3));
        let (px, py) = if s < w {
            (s, 0.0)
        } else if s < w + h {
            (w, s - w)
        } else if s < 2.0 * w + h {
            (w - (s - w - h), h)
        } else {
            (0.0, h - (s - 2.0 * w - h))
        };
        Vector2::new(self.x + px, self.y + py)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_snapshot::split_screen_viewports;
    use crate::rendering::ui_kit::{self, TextStyle, UiCanvas};

    /// Physical screens (width, height, density): desktop windows and monitors, and phones
    /// held both ways.
    const SCREENS: [(i32, i32, f32); 9] = [
        (960, 540, 1.0),
        (1280, 720, 1.0),
        (1366, 768, 1.0),
        (1920, 1080, 1.0),
        (2560, 1440, 1.0),
        (1100, 900, 1.0),
        (2532, 1170, 3.0),
        (1170, 2532, 3.0),
        (2400, 1080, 2.6),
    ];

    /// Every seat's UI rectangle on every screen, one to four seats.
    fn seat_rects() -> Vec<(String, Rectangle)> {
        let mut out = Vec::new();
        for (width, height, density) in SCREENS {
            let screen = UiCanvas::for_framebuffer(width, height, density);
            let points = (
                (width as f32 / density) as i32,
                (height as f32 / density) as i32,
            );
            for seats in 1..=4 {
                for (index, viewport) in split_screen_viewports(seats, points.0, points.1)
                    .into_iter()
                    .enumerate()
                {
                    let canvas = UiCanvas::for_viewport(screen, viewport);
                    out.push((
                        format!("{width}x{height}@{density} seat {index} of {seats}"),
                        canvas.viewport_rect(viewport),
                    ));
                }
            }
        }
        out
    }

    fn specs() -> Vec<HudSpec<'static>> {
        let plate = ui_kit::text_width("OPEN SPACE  HOME SYSTEM", TextStyle::Bold) as f32 + 16.0;
        let long_plate =
            ui_kit::text_width("NEAR THE MOON  ARCHIPELAGO OF VESS", TextStyle::Bold) as f32 + 16.0;
        let mut out = Vec::new();
        for touch in [false, true] {
            for nav in [true, false] {
                for (docking, missions) in [(None, 0), (Some(true), 3), (Some(false), 1)] {
                    for plate_width in [plate, long_plate] {
                        out.push(HudSpec {
                            touch,
                            nav,
                            plate_width,
                            rack: true,
                            docking,
                            missions,
                            prompt_rows: 2,
                            prompt_width: 300.0,
                            controls: &[],
                        });
                    }
                }
            }
        }
        out
    }

    fn assert_apart(name: &str, panels: &[Rectangle]) {
        for (index, a) in panels.iter().enumerate() {
            for b in &panels[index + 1..] {
                assert!(!overlaps(*a, *b), "{name}: {a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn the_flight_hud_panels_never_overlap_at_any_size() {
        for (name, rect) in seat_rects() {
            for spec in specs() {
                let mut layout = SpaceHudLayout::new(rect, &spec);
                // The plate's stack of banners (warnings, the systems' readings) and the
                // fight's banners low in the view.
                let mut top = bottom(layout.plate) + GAP;
                for text in ["HULL HEATING", "SENSORS FOGGED 58%", "PARKING HOLD"] {
                    let width = ui_kit::text_width(text, TextStyle::Bold) as f32 + 12.0;
                    top = bottom(layout.banner(top, width, 15.0)) + 3.0;
                }
                let mut low = rect.y + rect.height * 0.7;
                for text in ["RUSTWAKE CORSAIRS: 4 SHIPS", "ESCAPE JUMP"] {
                    let width = ui_kit::text_width(text, TextStyle::Bold) as f32 + 12.0;
                    low = bottom(layout.banner(low, width, 15.0)) + 3.0;
                }
                let panels = layout.panels();
                assert_apart(&format!("{name} {spec:?}"), &panels);
                // The bounties keep clear of a message toast.
                if let Some(bounties) = layout.bounties {
                    assert!(
                        !overlaps(bounties, toast_band(rect)),
                        "{name} {spec:?}: the bounties sit where a toast rises"
                    );
                }
                for panel in &panels {
                    assert!(
                        panel.x >= rect.x && right(*panel) <= right(rect) + 0.01,
                        "{name} {spec:?}: {panel:?} leaves the view across"
                    );
                }
                // The fixed panels stay on screen top to bottom too.
                for panel in [layout.meters, layout.combat, layout.plate]
                    .into_iter()
                    .chain(layout.nav)
                    .chain(layout.docking)
                    .chain(layout.bounties)
                    .chain(layout.prompts)
                {
                    assert!(
                        inside(panel, rect),
                        "{name} {spec:?}: {panel:?} off the view"
                    );
                }
            }
        }
    }

    #[test]
    fn edge_markers_keep_clear_of_the_panels_and_each_other_at_any_size() {
        // The labels of the reported overlap, the crew and a fight.
        let labels = [
            "WORMHOLE ? 1.8 sectors",
            "Merorath 9.7 sectors",
            "P2 1.2 sectors",
            "CUTTER 1,450 t",
        ];
        for (name, rect) in seat_rects() {
            for spec in specs() {
                let layout = SpaceHudLayout::new(rect, &spec);
                let top = bottom(layout.plate) + GAP;
                for turn in 0..16 {
                    let angle = turn as f32 / 16.0 * std::f32::consts::TAU;
                    let mut markers = layout.markers(top);
                    let mut spots = Vec::new();
                    for (index, label) in labels.iter().enumerate() {
                        // All four the same way (the worst case), the last one in view.
                        let aim = if index == 3 {
                            Aim::At(Vector2::new(
                                rect.x + rect.width * (0.5 + 0.4 * angle.sin()),
                                rect.y + rect.height * (0.5 - 0.4 * angle.cos()),
                            ))
                        } else {
                            Aim::Toward(angle.sin(), -angle.cos())
                        };
                        let width = ui_kit::text_width(label, TextStyle::Body) as f32;
                        let short = ui_kit::text_width("1.8 sectors", TextStyle::Body) as f32;
                        spots.push(markers.place(aim, &[width, short]));
                    }
                    let mut drawn: Vec<Rectangle> = Vec::new();
                    for spot in &spots {
                        let pieces: Vec<Rectangle> =
                            spot.mark_box().into_iter().chain(spot.label).collect();
                        for piece in pieces {
                            assert!(
                                inside(piece, rect),
                                "{name} {spec:?} at {turn}/16: {piece:?} off the view"
                            );
                            for panel in layout.panels() {
                                assert!(
                                    !overlaps(piece, panel),
                                    "{name} {spec:?} at {turn}/16: marker {piece:?} over panel \
                                     {panel:?}"
                                );
                            }
                            for other in &drawn {
                                assert!(
                                    !overlaps(piece, *other),
                                    "{name} {spec:?} at {turn}/16: markers {piece:?} and \
                                     {other:?} overlap"
                                );
                            }
                        }
                        drawn.extend(spot.mark_box());
                        drawn.extend(spot.label);
                    }
                    // Every off-view marker shows, and its label too (in full, or its distance
                    // alone) wherever a seat's view leaves the lanes room.
                    let roomy = markers.lanes.width >= 300.0 && markers.lanes.height >= 120.0;
                    for spot in &spots[..3] {
                        assert!(
                            !roomy || spot.arrow.is_some(),
                            "{name} {spec:?}: an arrow went missing"
                        );
                        assert!(
                            !roomy || spot.label.is_some(),
                            "{name} {spec:?} at {turn}/16: a label found no room"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_phones_touch_controls_stay_clear_of_banners_and_markers() {
        // A landscape phone's view (art pixels) with its stick, its thumb cluster, the
        // rotation pair and the top row.
        let rect = Rectangle::new(0.0, 0.0, 427.0, 240.0);
        let controls = [
            Rectangle::new(8.0, 140.0, 92.0, 92.0),
            Rectangle::new(300.0, 120.0, 52.0, 52.0),
            Rectangle::new(360.0, 150.0, 60.0, 60.0),
            Rectangle::new(84.0, 80.0, 30.0, 22.0),
            Rectangle::new(118.0, 80.0, 30.0, 22.0),
            Rectangle::new(150.0, 4.0, 26.0, 18.0),
            Rectangle::new(180.0, 4.0, 26.0, 18.0),
        ];
        let mut layout = SpaceHudLayout::new(
            rect,
            &HudSpec {
                touch: true,
                nav: true,
                plate_width: 140.0,
                rack: true,
                docking: None,
                missions: 0,
                prompt_rows: 0,
                prompt_width: 0.0,
                controls: &controls,
            },
        );
        let mut top = bottom(layout.plate) + GAP;
        for width in [110.0, 90.0] {
            top = bottom(layout.banner(top, width, 15.0)) + 3.0;
        }
        let low = layout.banner(rect.height * 0.7, 160.0, 15.0);
        for placed in layout.banners.iter().chain([&layout.plate, &low]) {
            for control in &controls {
                assert!(!overlaps(*placed, *control), "{placed:?} over {control:?}");
            }
        }
        let mut markers = layout.markers(top);
        for turn in 0..16 {
            let angle = turn as f32 / 16.0 * std::f32::consts::TAU;
            let spot = markers.place(Aim::Toward(angle.sin(), -angle.cos()), &[60.0, 30.0]);
            for piece in spot.mark_box().into_iter().chain(spot.label) {
                for control in &controls {
                    assert!(!overlaps(piece, *control), "{piece:?} over {control:?}");
                }
            }
        }
    }

    #[test]
    fn the_reported_overlap_is_gone() {
        // 1280 x 720: the wormhole's marker under the nav panel's "No waypoint".
        let rect = Rectangle::new(0.0, 0.0, 640.0, 360.0);
        let layout = SpaceHudLayout::new(
            rect,
            &HudSpec {
                touch: false,
                nav: true,
                plate_width: 140.0,
                rack: true,
                docking: None,
                missions: 0,
                prompt_rows: 1,
                prompt_width: 360.0,
                controls: &[],
            },
        );
        let mut markers = layout.markers(bottom(layout.plate) + GAP);
        let spot = markers.place(Aim::Toward(1.0, -0.62), &[120.0]);
        let nav = layout.nav.expect("the nav panel");
        assert!(!overlaps(spot.label.expect("a label"), nav));
        assert!(!overlaps(spot.mark_box().expect("an arrow"), nav));
    }

    #[test]
    fn the_perimeter_walks_round_and_back() {
        let perimeter = Perimeter {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        };
        for along in [0.0, 30.0, 100.0, 120.0, 180.0, 260.0, 299.0] {
            let point = perimeter.point_at(along);
            assert!(
                (perimeter.position_of(point) - along).abs() < 0.01,
                "{along}"
            );
        }
    }
}
