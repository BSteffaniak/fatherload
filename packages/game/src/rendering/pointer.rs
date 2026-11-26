//! What a tap or click on a menu lands on.
//!
//! While a frame is drawn every dialog records where its selectable rows ended up
//! ([`record_row`]) and its own outline ([`record_dialog`]). The next frame's pointer input is
//! resolved against that record ([`hit`]): a row is picked (the shell selects it and confirms),
//! a touch elsewhere on the dialog does nothing and a touch outside it backs out.
//!
//! Only the menus the pointer can drive are recorded: the global screens (title, pause,
//! options, online ...) and the first seat's own dialogs. A global screen covers the first
//! seat's dialog, so it wins while it is up.

use std::cell::RefCell;

use raylib::prelude::Rectangle;

use super::ui_kit;

/// What is under a pointer position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuHit {
    /// A selectable row, by its index among the screen's rows.
    Row(usize),
    /// The dialog, but none of its rows.
    Dialog,
    /// Outside the dialog that is open.
    Outside,
    /// No menu is on screen.
    Nothing,
}

/// One owner's recorded menu: rows and dialog outlines in logical screen points.
/// Turns a point of a recorded area (`u`, `v` in 0..1 across and down it) into its row index.
pub(super) type AreaRow = fn(f32, f32) -> usize;

#[derive(Clone, Debug, Default)]
struct Recorded {
    rows: Vec<(Rectangle, usize)>,
    /// Areas whose every point is a row of its own (the Atlas's map, Wave 14).
    areas: Vec<(Rectangle, AreaRow)>,
    dialogs: Vec<Rectangle>,
}

impl Recorded {
    const fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.dialogs.is_empty() && self.areas.is_empty()
    }

    fn hit(&self, x: f32, y: f32) -> MenuHit {
        let inside = |rect: &Rectangle| {
            x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height
        };
        if let Some((_, index)) = self.rows.iter().find(|(rect, _)| inside(rect)) {
            MenuHit::Row(*index)
        } else if let Some((rect, row)) = self.areas.iter().find(|(rect, _)| inside(rect)) {
            MenuHit::Row(row(
                (x - rect.x) / rect.width.max(1.0),
                (y - rect.y) / rect.height.max(1.0),
            ))
        } else if self.dialogs.iter().any(inside) {
            MenuHit::Dialog
        } else if self.dialogs.is_empty() {
            // Rows without a dialog around them (the title menu): missing them is not "out".
            MenuHit::Nothing
        } else {
            MenuHit::Outside
        }
    }
}

/// Whose menu is being drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Target {
    Global,
    FirstSeat,
    /// Another seat's dialog: the pointer does not drive it.
    Ignored,
}

#[derive(Debug)]
struct Record {
    target: Target,
    global: Recorded,
    first_seat: Recorded,
}

thread_local! {
    static RECORD: RefCell<Record> = const {
        RefCell::new(Record {
            target: Target::Ignored,
            global: Recorded { rows: Vec::new(), areas: Vec::new(), dialogs: Vec::new() },
            first_seat: Recorded { rows: Vec::new(), areas: Vec::new(), dialogs: Vec::new() },
        })
    };
}

/// Forgets the last frame's menus (call before drawing a frame).
pub(super) fn begin_frame() {
    RECORD.with(|record| {
        let mut record = record.borrow_mut();
        record.target = Target::Ignored;
        record.global = Recorded::default();
        record.first_seat = Recorded::default();
    });
}

/// Says whose menu the following draws belong to.
pub(super) fn set_target(target: Target) {
    RECORD.with(|record| record.borrow_mut().target = target);
}

fn with_target(action: impl FnOnce(&mut Recorded)) {
    RECORD.with(|record| {
        let mut record = record.borrow_mut();
        match record.target {
            Target::Global => action(&mut record.global),
            Target::FirstSeat => action(&mut record.first_seat),
            Target::Ignored => {}
        }
    });
}

/// Records selectable row `index` at `rect` (current-canvas art pixels), as far as it shows
/// inside `clip` (the scroll area).
pub(super) fn record_row(rect: Rectangle, clip: Rectangle, index: usize) {
    let left = rect.x.max(clip.x);
    let top = rect.y.max(clip.y);
    let right = (rect.x + rect.width).min(clip.x + clip.width);
    let bottom = (rect.y + rect.height).min(clip.y + clip.height);
    if right <= left || bottom <= top {
        return;
    }
    let visible = Rectangle::new(left, top, right - left, bottom - top);
    let points = ui_kit::canvas().to_points(visible);
    with_target(|recorded| recorded.rows.push((points, index)));
}

/// Records an area (current-canvas art pixels) whose every point is a row: `row` names the
/// row at `(u, v)`, 0..1 across and down it (the Atlas's map, Wave 14).
pub(super) fn record_area(rect: Rectangle, row: AreaRow) {
    let points = ui_kit::canvas().to_points(rect);
    with_target(|recorded| recorded.areas.push((points, row)));
}

/// Records a dialog's outline (current-canvas art pixels).
pub(super) fn record_dialog(rect: Rectangle) {
    let points = ui_kit::canvas().to_points(rect);
    with_target(|recorded| recorded.dialogs.push(points));
}

/// What the menus drawn last frame have under `(x, y)` (logical screen points).
#[must_use]
pub fn hit(x: f32, y: f32) -> MenuHit {
    RECORD.with(|record| {
        let record = record.borrow();
        if record.global.is_empty() {
            record.first_seat.hit(x, y)
        } else {
            record.global.hit(x, y)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
        Rectangle::new(x, y, width, height)
    }

    /// The default canvas is 2 points per art pixel at the origin.
    #[test]
    fn rows_are_hit_where_they_show_and_the_dialog_swallows_the_rest() {
        begin_frame();
        set_target(Target::FirstSeat);
        let body = rect(10.0, 20.0, 100.0, 40.0);
        record_dialog(rect(5.0, 5.0, 110.0, 70.0));
        record_row(rect(10.0, 20.0, 100.0, 15.0), body, 0);
        // Half scrolled out of the bottom of the body: only its visible part is hit.
        record_row(rect(10.0, 50.0, 100.0, 15.0), body, 2);
        // Entirely scrolled out.
        record_row(rect(10.0, 70.0, 100.0, 15.0), body, 3);

        assert_eq!(hit(40.0, 50.0), MenuHit::Row(0));
        assert_eq!(hit(40.0, 110.0), MenuHit::Row(2));
        assert_eq!(hit(40.0, 125.0), MenuHit::Dialog, "clipped part of row 2");
        assert_eq!(hit(40.0, 145.0), MenuHit::Dialog, "row 3 is not on screen");
        assert_eq!(hit(40.0, 85.0), MenuHit::Dialog, "between rows");
        assert_eq!(hit(400.0, 300.0), MenuHit::Outside);
    }

    #[test]
    fn a_global_screen_covers_the_first_seats_dialog_and_other_seats_are_ignored() {
        begin_frame();
        assert_eq!(hit(10.0, 10.0), MenuHit::Nothing);
        let everywhere = rect(0.0, 0.0, 500.0, 500.0);
        set_target(Target::Ignored);
        record_row(rect(0.0, 0.0, 50.0, 10.0), everywhere, 7);
        assert_eq!(hit(10.0, 10.0), MenuHit::Nothing);

        set_target(Target::FirstSeat);
        record_row(rect(0.0, 0.0, 50.0, 10.0), everywhere, 1);
        assert_eq!(hit(10.0, 10.0), MenuHit::Row(1));

        set_target(Target::Global);
        record_row(rect(0.0, 30.0, 50.0, 10.0), everywhere, 4);
        assert_eq!(hit(10.0, 70.0), MenuHit::Row(4));
        assert_eq!(
            hit(10.0, 10.0),
            MenuHit::Nothing,
            "rows with no dialog (the title menu) have no outside"
        );

        begin_frame();
        assert_eq!(hit(10.0, 70.0), MenuHit::Nothing);
    }
}
