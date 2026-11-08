//! Screen layout for menus and modals: a content model (sections of rows, meters and text) laid
//! out by the widget tree and drawn with the pixel UI kit. All coordinates are UI canvas art
//! pixels (see [`super::ui_kit`]).

use std::{cell::RefCell, collections::BTreeMap};

use raylib::prelude::*;

#[allow(
    clippy::wildcard_imports,
    reason = "art code names colours from the whole palette"
)]
use super::pixel_art::palette::*;
use super::{
    pixel_art::sprite::SpriteDef,
    prompts::{self, Hint},
    ui_kit::{self, ButtonState, PanelStyle, TextStyle},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Size {
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Insets {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Insets {
    #[cfg(test)]
    const fn all(value: f32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum TextKind {
    Heading,
    Small,
}

impl TextKind {
    const fn style(self) -> TextStyle {
        match self {
            Self::Heading => TextStyle::Bold,
            Self::Small => TextStyle::Body,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PanelKind {
    Overlay,
}

/// Where dialogs sit: centred, or beside a building's counter so the room stays visible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ModalPlacement {
    #[default]
    Centred,
    /// Left-hand side of the screen, leaving the interior's counter, keeper and rig in view.
    Counter,
}

pub(super) struct UiLayout<'draw, 'handle> {
    draw: &'draw mut RaylibDrawHandle<'handle>,
    viewport: Rectangle,
}

impl<'draw, 'handle> UiLayout<'draw, 'handle> {
    /// Lays out over the whole UI canvas. Call inside a [`ui_kit::UiCanvasScope`].
    pub(super) fn screen(draw: &'draw mut RaylibDrawHandle<'handle>) -> Self {
        Self {
            draw,
            viewport: ui_kit::canvas().rect(),
        }
    }

    /// A dialog frame with a title, subtitle and button prompts; returns the body rect for
    /// custom drawing.
    pub(super) fn canvas_modal(
        &mut self,
        title: &str,
        subtitle: &str,
        hints: &[Hint],
        summary: &str,
    ) -> Rectangle {
        let rect = self.dialog_rect(false);
        let (_, body) = self.modal_frame(title, subtitle, hints, rect, None, false);
        ui_kit::text(
            self.draw,
            summary,
            body.x as i32,
            body.y as i32,
            TextStyle::Body,
            MIST,
        );
        Rectangle {
            x: body.x,
            y: body.y + 14.0,
            width: body.width,
            height: (body.height - 14.0).max(0.0),
        }
    }

    /// A dialog with `content` laid out as scrollable sections.
    pub(super) fn modal_with_render_plan(
        &mut self,
        title: &str,
        subtitle: &str,
        content: &ModalContent,
    ) {
        CURRENT_UI_STATE.with(|current| {
            if let Some(state) = current.borrow_mut().as_mut() {
                state.enter_modal(title, &widgets::WidgetId::new("modal-content"));
            }
        });
        // Measure the content first so short dialogs shrink to fit it.
        let full = self.dialog_rect(content.compact);
        let content_width = (full.width - 24.0 - 8.0).max(0.0);
        let mut node = modal_content_node(content, content_width);
        node.layout(Rectangle::new(0.0, 0.0, content_width, 100_000.0));
        let content_height = match &node {
            widgets::UiNode::Scroll(scroll) => scroll.content_height,
            other => other.rect().height,
        };
        let footer_lines = content.footer.as_ref().map_or_else(Vec::new, |footer| {
            let mut lines = ui_kit::wrap(footer, content_width as i32, TextStyle::Body);
            lines.truncate(3);
            lines
        });
        let footer_height = if footer_lines.is_empty() {
            0.0
        } else {
            (i32::try_from(footer_lines.len()).unwrap_or(3) * TextStyle::Body.line_height() + 7)
                as f32
        };
        let (panel, body) = self.modal_frame(
            title,
            subtitle,
            &content.hints,
            full,
            Some(content_height + footer_height),
            content.warning,
        );
        // Room on the left for the selection cursor.
        let body = Rectangle {
            x: body.x + 8.0,
            width: (body.width - 8.0).max(0.0),
            height: (body.height - footer_height).max(0.0),
            ..body
        };
        let mut node = modal_content_node(content, body.width);
        node.layout(body);
        if let widgets::UiNode::Scroll(scroll) = &node {
            let limit = (scroll.content_height - body.height).max(0.0);
            set_current_scroll_limit(widgets::WidgetId::new("modal-content"), limit);
            set_current_page_step(
                widgets::WidgetId::new("modal-content"),
                (body.height - MENU_ROW_HEIGHT).max(MENU_ROW_HEIGHT),
            );
        }
        if follow_focused_row(&node, body) {
            node = modal_content_node(content, body.width);
            node.layout(body);
        }
        let mut overflow = None;
        if let widgets::UiNode::Scroll(scroll) = &node {
            let limit = (scroll.content_height - body.height).max(0.0);
            if limit > 0.0 {
                overflow = Some((scroll.offset_y.clamp(0.0, limit), scroll.content_height));
            }
        }
        let plan = node.render_plan();
        self.render_plan(&plan);
        if let Some((offset, content_height)) = overflow {
            self.scroll_bar(body, offset, content_height);
            self.scroll_prompt(panel, title);
        }
        if !footer_lines.is_empty() {
            let x = body.x as i32 - 8;
            let mut y = (body.y + body.height) as i32 + 3;
            ui_kit::divider(self.draw, x, y, body.width as i32 + 8);
            y += 4;
            for line in &footer_lines {
                ui_kit::text(self.draw, line, x, y, TextStyle::Body, MIST);
                y += TextStyle::Body.line_height();
            }
        }
    }

    /// A thin scroll bar in the dialog's right margin, so lists taller than the panel show
    /// that more rows exist below (or above) the visible ones, with a small arrow at each end
    /// that has more content past it.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "canvas coordinates are small"
    )]
    fn scroll_bar(&mut self, body: Rectangle, offset: f32, content_height: f32) {
        // Room for an arrow (3 rows) and a gap at each end of the track.
        const ARROW_ROOM: i32 = 5;
        let x = (body.x + body.width) as i32 + 4;
        let top = body.y as i32 + ARROW_ROOM;
        let height = body.height as i32 - ARROW_ROOM * 2;
        if height < 8 || content_height <= 0.0 {
            return;
        }
        let max_offset = (content_height - body.height).max(1.0);
        self.draw.draw_rectangle(x, top, 3, height, INK);
        let thumb = ((body.height / content_height) * height as f32).max(6.0) as i32;
        let travel = (height - thumb).max(0);
        let thumb_top = top + ((offset / max_offset) * travel as f32).round() as i32;
        self.draw.draw_rectangle(x, thumb_top, 3, thumb, AMBER);
        self.draw.draw_rectangle(x, thumb_top, 1, thumb, GOLD);
        let more_above = offset > 0.5;
        let more_below = offset < max_offset - 0.5;
        // 5-pixel-wide arrows centred on the 3-pixel track.
        for (row, width) in [(0, 1), (1, 3), (2, 5)] {
            let left = x + 1 - width / 2;
            if more_above {
                self.draw
                    .draw_rectangle(left, top - ARROW_ROOM + row, width, 1, GOLD);
            }
            if more_below {
                let y = top + height + ARROW_ROOM - 1 - row;
                self.draw.draw_rectangle(left, y, width, 1, GOLD);
            }
        }
    }

    /// The scroll prompt (wheel + Page Up/Down, or the right stick) right-aligned on the title
    /// line of an overflowing dialog, above its scroll bar. Drops its label, then itself, when
    /// the title leaves no room.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "canvas coordinates are small"
    )]
    fn scroll_prompt(&mut self, panel: Rectangle, title: &str) {
        let device = prompts::menu_device();
        let right = (panel.x + panel.width) as i32 - 12;
        let title_end = panel.x as i32 + 12 + ui_kit::text_width(title, TextStyle::Title) + 8;
        let y = panel.y as i32 + 10 + (TextStyle::Title.height() - prompts::PROMPT_HEIGHT) / 2;
        for label in ["scroll", ""] {
            let hint = prompts::hint(crate::input::prompts::Prompt::Scroll, label);
            let Some(width) = prompts::prompt_width(hint, device) else {
                return;
            };
            if right - width >= title_end {
                prompts::draw_prompt(self.draw, right - width, y, hint, device, MIST);
                return;
            }
        }
    }

    /// The dialog rect for this frame's placement (`compact` narrows centred dialogs).
    fn dialog_rect(&self, compact: bool) -> Rectangle {
        let placement = current_modal_placement();
        let rect = modal_rect(self.viewport, placement);
        if compact && placement == ModalPlacement::Centred && rect.width > COMPACT_MODAL_WIDTH {
            Rectangle {
                x: (rect.x + (rect.width - COMPACT_MODAL_WIDTH) * 0.5).floor(),
                width: COMPACT_MODAL_WIDTH,
                ..rect
            }
        } else {
            rect
        }
    }

    /// Dims the screen, draws the dialog panel, title, subtitle, button prompts (for the menu
    /// device) and divider; returns the panel and its body. With `content_height` the panel shrinks to fit
    /// that much body.
    fn modal_frame(
        &mut self,
        title: &str,
        subtitle: &str,
        hints: &[Hint],
        mut rect: Rectangle,
        content_height: Option<f32>,
        warning: bool,
    ) -> (Rectangle, Rectangle) {
        let placement = current_modal_placement();
        if placement == ModalPlacement::Centred {
            ui_kit::dim(self.draw, self.viewport);
        }
        let width = rect.width as i32 - 24;
        let lines = if subtitle.is_empty() {
            Vec::new()
        } else {
            ui_kit::wrap(subtitle, width, TextStyle::Body)
        };
        let subtitle_lines = i32::try_from(lines.len().min(2)).unwrap_or(1);
        let device = prompts::menu_device();
        let prompt_rows = prompts::wrap_bar(hints, device, width);
        let prompt_height = if prompt_rows.is_empty() {
            0
        } else {
            prompts::wrapped_height(prompt_rows.len()) + 3
        };
        let header = 10
            + TextStyle::Title.height()
            + 3
            + subtitle_lines * TextStyle::Body.line_height()
            + prompt_height
            + 8;
        if let Some(content) = content_height {
            let wanted = (header as f32 + content + 14.0).ceil().max(72.0);
            if wanted < rect.height {
                // Centred dialogs stay centred; counter panels keep their top edge.
                if placement == ModalPlacement::Centred {
                    rect.y = (rect.y + (rect.height - wanted) * 0.5).floor();
                }
                rect.height = wanted;
            }
        }
        let (style, title_color) = if warning {
            (PanelStyle::Warning, ORANGE)
        } else {
            (PanelStyle::Accent, GOLD)
        };
        ui_kit::panel(self.draw, rect, style);
        let x = rect.x as i32 + 12;
        let mut y = rect.y as i32 + 10;
        ui_kit::text(self.draw, title, x, y, TextStyle::Title, title_color);
        y += TextStyle::Title.height() + 3;
        for line in lines.iter().take(2) {
            ui_kit::text(self.draw, line, x, y, TextStyle::Body, FOG);
            y += TextStyle::Body.line_height();
        }
        if !prompt_rows.is_empty() {
            y += 1;
            for row in &prompt_rows {
                prompts::draw_bar(self.draw, x, y, row, device, MIST);
                y += prompts::PROMPT_HEIGHT + 2;
            }
        }
        y += 2;
        ui_kit::divider(self.draw, x, y, width);
        y += 6;
        let body = Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: (rect.y + rect.height - 10.0 - y as f32).max(0.0),
        };
        (rect, body)
    }

    fn render_plan(&mut self, plan: &widgets::RenderPlan) {
        for command in plan.commands() {
            match command {
                widgets::RenderCommand::Panel {
                    rect,
                    kind: PanelKind::Overlay,
                } => ui_kit::panel(self.draw, *rect, PanelStyle::Standard),
                widgets::RenderCommand::Text {
                    rect,
                    text,
                    kind,
                    color,
                } => {
                    Self::draw_text_wrapped(self.draw, text, *rect, *kind, *color);
                }
                widgets::RenderCommand::Meter {
                    rect,
                    ratio,
                    fill,
                    danger: _,
                } => {
                    ui_kit::bar(self.draw, *rect, *ratio, bar_colors(*fill), 0.25);
                }
                widgets::RenderCommand::Button {
                    rect,
                    focused,
                    id: _,
                    text,
                    color,
                    row,
                } => self.draw_row(*rect, *focused, text, *color, row),
                widgets::RenderCommand::Canvas { rect } => ui_kit::well(self.draw, *rect),
                // The clip extends left over the selection cursor's gutter.
                widgets::RenderCommand::Clip { rect } => ui_kit::begin_clip(Rectangle {
                    x: rect.x - 8.0,
                    width: rect.width + 8.0,
                    ..*rect
                }),
                widgets::RenderCommand::EndClip => ui_kit::end_clip(),
            }
        }
    }

    fn draw_row(
        &mut self,
        rect: Rectangle,
        focused: bool,
        label: &str,
        color: Color,
        row: &widgets::RowContent,
    ) {
        if row.face == widgets::RowFace::Field {
            Self::draw_field(self.draw, rect, label, &row.detail);
            return;
        }
        if row.face == widgets::RowFace::Plain {
            let y = rect.y as i32 + 1;
            let mut x = rect.x as i32;
            if let Some(icon) = row.icon {
                ui_kit::icon(self.draw, icon, x, y);
                x += icon.width() + 4;
            }
            ui_kit::text(self.draw, label, x, y, TextStyle::Body, MIST);
            if !row.value.is_empty() {
                let right = (rect.x + rect.width) as i32 - 2;
                ui_kit::text_right(
                    self.draw,
                    &row.value,
                    right,
                    y,
                    TextStyle::Bold,
                    row.value_color.unwrap_or(color),
                );
            }
            return;
        }
        let state = if focused {
            ButtonState::Selected
        } else if row.face == widgets::RowFace::Disabled {
            ButtonState::Disabled
        } else {
            ButtonState::Idle
        };
        let idle_value = row.value_color.unwrap_or(BONE);
        if row.icon.is_some() || !row.detail.is_empty() {
            ui_kit::item_row(
                self.draw,
                rect,
                &ui_kit::ItemRow {
                    icon: row.icon,
                    label,
                    detail: &row.detail,
                    price: &row.value,
                    price_color: idle_value,
                    coin: row.coin,
                    state,
                },
            );
        } else {
            ui_kit::button(self.draw, rect, label, state);
            if !row.value.is_empty() {
                let value_color = if state == ButtonState::Selected {
                    INK
                } else {
                    idle_value
                };
                ui_kit::text_right(
                    self.draw,
                    &row.value,
                    (rect.x + rect.width) as i32 - 7,
                    rect.y as i32 + (rect.height as i32 - 9) / 2,
                    TextStyle::Body,
                    value_color,
                );
            }
        }
    }

    /// A text entry well: the typed text's tail (so the caret stays visible) or the
    /// placeholder, and a blinking caret.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "canvas coordinates are small"
    )]
    fn draw_field(draw: &mut RaylibDrawHandle<'_>, rect: Rectangle, text: &str, placeholder: &str) {
        ui_kit::well(draw, rect);
        let (x, y) = (
            rect.x as i32 + 5,
            rect.y as i32 + (rect.height as i32 - 9) / 2,
        );
        let room = rect.width as i32 - 14;
        if text.is_empty() {
            let hint = ui_kit::truncate(placeholder, room, TextStyle::Body);
            ui_kit::text(draw, &hint, x + 3, y, TextStyle::Body, FOG);
        }
        let mut shown = text.to_owned();
        while ui_kit::text_width(&shown, TextStyle::Body) > room && !shown.is_empty() {
            shown.remove(0);
        }
        ui_kit::text(draw, &shown, x, y, TextStyle::Body, CREAM);
        if (ui_kit::seconds() * 2.0) as i32 % 2 == 0 {
            let caret_x = x + ui_kit::text_width(&shown, TextStyle::Body) + 1;
            draw.draw_rectangle(caret_x, y - 1, 2, 10, GOLD);
        }
    }

    fn draw_text_wrapped(
        draw: &mut RaylibDrawHandle<'_>,
        text: &str,
        rect: Rectangle,
        kind: TextKind,
        color: Color,
    ) {
        let layout = layout_text(text, rect.width.max(0.0), kind);
        for line in layout.lines {
            ui_kit::text(
                draw,
                &line.text,
                (rect.x + line.x).round() as i32,
                (rect.y + line.y).round() as i32,
                kind.style(),
                color,
            );
        }
    }
}

/// Bar colours for a meter tinted `fill` (legacy colour hints map onto palette ramps).
fn bar_colors(fill: Color) -> ui_kit::BarColors {
    if fill == FUEL_HINT || fill == GOLD {
        ui_kit::BarColors::FUEL
    } else if fill == LEAF || fill == LIME {
        ui_kit::BarColors::HULL
    } else if fill == CLAY {
        ui_kit::BarColors::CARGO
    } else {
        ui_kit::BarColors::COOL
    }
}

/// Fuel meters pass the fuel colour; kept as a named hint for [`bar_colors`].
const FUEL_HINT: Color = AMBER;

pub(super) struct StatItem {
    label: String,
    value: String,
    color: Color,
    icon: Option<&'static SpriteDef>,
}

impl StatItem {
    pub(super) fn new(label: impl Into<String>, value: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            color,
            icon: None,
        }
    }
}

pub(super) struct ModalContent {
    sections: Vec<Section>,
    compact: bool,
    warning: bool,
    /// Text pinned under the scrolling content (e.g. the selected item's description).
    footer: Option<String>,
    /// Button prompts under the title, drawn for the menu device.
    hints: Vec<Hint>,
}

impl ModalContent {
    pub(super) const fn new(sections: Vec<Section>) -> Self {
        Self {
            sections,
            compact: false,
            warning: false,
            footer: None,
            hints: Vec::new(),
        }
    }

    /// Button prompts under the title (actions the menu device has no binding for are hidden).
    pub(super) fn prompts(mut self, hints: &[Hint]) -> Self {
        self.hints = hints.to_vec();
        self
    }

    /// Pins `text` under the content, outside the scroll area.
    pub(super) fn footer(mut self, text: Option<String>) -> Self {
        self.footer = text.filter(|text| !text.is_empty());
        self
    }

    /// A warning dialog (red frame and title): failures and emergencies.
    pub(super) const fn warning(mut self) -> Self {
        self.warning = true;
        self
    }

    /// A narrow dialog for short menus (title, pause, confirmations).
    pub(super) const fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

/// Width of a compact centred dialog in art pixels.
const COMPACT_MODAL_WIDTH: f32 = 300.0;

pub(super) struct Section {
    title: String,
    color: Color,
    items: Vec<SectionItem>,
}

impl Section {
    pub(super) fn new(title: impl Into<String>, color: Color, items: Vec<SectionItem>) -> Self {
        Self {
            title: title.into(),
            color,
            items,
        }
    }
}

/// A shop / list row: icon, label, detail line and a right-aligned value.
pub(super) struct ItemEntry {
    pub icon: Option<&'static SpriteDef>,
    pub label: String,
    pub detail: String,
    pub value: String,
    pub value_color: Color,
    /// Show a coin before the value.
    pub coin: bool,
    pub selected: bool,
    /// Greyed out (e.g. unaffordable), still selectable.
    pub disabled: bool,
}

pub(super) enum SectionItem {
    Meter {
        label: String,
        value: f32,
        max: f32,
        fill: Color,
        danger: Color,
    },
    Stat(StatItem),
    Item(ItemEntry),
    Text(String),
    /// A text entry field showing `text` (or `placeholder` while empty) and a caret.
    Field {
        text: String,
        placeholder: String,
    },
}

impl SectionItem {
    pub(super) fn meter(
        label: impl Into<String>,
        value: f32,
        max: f32,
        fill: Color,
        danger: Color,
    ) -> Self {
        Self::Meter {
            label: label.into(),
            value,
            max,
            fill,
            danger,
        }
    }

    pub(super) fn stat(label: impl Into<String>, value: impl Into<String>, color: Color) -> Self {
        Self::Stat(StatItem::new(label, value, color))
    }

    /// A label/value row with an icon.
    pub(super) fn icon_stat(
        icon: &'static SpriteDef,
        label: impl Into<String>,
        value: impl Into<String>,
        color: Color,
    ) -> Self {
        Self::Stat(StatItem {
            icon: Some(icon),
            ..StatItem::new(label, value, color)
        })
    }

    pub(super) const fn item(entry: ItemEntry) -> Self {
        Self::Item(entry)
    }

    pub(super) fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    pub(super) fn field(text: impl Into<String>, placeholder: impl Into<String>) -> Self {
        Self::Field {
            text: text.into(),
            placeholder: placeholder.into(),
        }
    }
}

/// Height of a selectable menu row.
const MENU_ROW_HEIGHT: f32 = ui_kit::BUTTON_HEIGHT as f32;
/// Height of a plain label/value row.
const PLAIN_ROW_HEIGHT: f32 = 11.0;

#[allow(
    clippy::too_many_lines,
    reason = "one match maps every content item kind onto widgets"
)]
fn modal_content_node(content: &ModalContent, width: f32) -> widgets::UiNode {
    let mut sections = Vec::new();
    // The shell's selection ("> " rows, selected items) wins; the remembered focus is only a
    // fallback for lists that mark nothing, so a stale focus never highlights a second row.
    let marks_selection = content.sections.iter().any(|section| {
        section.items.iter().any(|item| match item {
            SectionItem::Stat(stat) => stat.label.starts_with("> "),
            SectionItem::Item(entry) => entry.selected,
            _ => false,
        })
    });
    let focused_id = if marks_selection {
        None
    } else {
        current_focused_id()
    };
    let mut selectable_index = 0_usize;
    let mut focus_order = Vec::new();
    for (section_index, section) in content.sections.iter().enumerate() {
        if section_index > 0 {
            sections.push(widgets::UiNode::Spacer(widgets::SpacerNode::sized(
                width, 2.0,
            )));
        }
        sections.push(widgets::UiNode::Text(widgets::TextNode::colored(
            section.title.to_uppercase(),
            TextKind::Heading,
            width,
            section.color,
        )));
        for item in &section.items {
            match item {
                SectionItem::Meter {
                    label,
                    value,
                    max,
                    fill,
                    danger,
                } => {
                    sections.push(widgets::UiNode::Button(
                        widgets::ButtonNode::label(label, width, PLAIN_ROW_HEIGHT, false, MIST)
                            .with_row(widgets::RowContent {
                                value: format!("{value:.0}/{max:.0}"),
                                value_color: Some(BONE),
                                face: widgets::RowFace::Plain,
                                ..widgets::RowContent::default()
                            }),
                    ));
                    sections.push(widgets::UiNode::Meter(widgets::MeterNode::colored(
                        ratio(*value, *max),
                        width,
                        8.0,
                        *fill,
                        *danger,
                    )));
                }
                SectionItem::Stat(stat) if is_selectable_stat(stat) => {
                    let id = widgets::WidgetId::new(format!("modal-option-{selectable_index}"));
                    selectable_index += 1;
                    focus_order.push(id.clone());
                    let selected = stat.label.trim_start().starts_with('>');
                    if selected {
                        set_current_focused_id(id.clone());
                    }
                    let focused =
                        selected || focused_id.as_ref().is_some_and(|focused| focused == &id);
                    let label = stat.label.get(2..).unwrap_or(&stat.label).to_owned();
                    sections.push(widgets::UiNode::Button(
                        widgets::ButtonNode::identified(
                            Some(id),
                            label,
                            width,
                            MENU_ROW_HEIGHT,
                            focused,
                            stat.color,
                        )
                        .with_row(widgets::RowContent {
                            value: stat.value.clone(),
                            icon: stat.icon,
                            ..widgets::RowContent::default()
                        }),
                    ));
                }
                SectionItem::Stat(stat) => {
                    sections.push(widgets::UiNode::Button(
                        widgets::ButtonNode::label(
                            &stat.label,
                            width,
                            PLAIN_ROW_HEIGHT,
                            false,
                            stat.color,
                        )
                        .with_row(widgets::RowContent {
                            value: stat.value.clone(),
                            value_color: Some(stat.color),
                            icon: stat.icon,
                            face: widgets::RowFace::Plain,
                            ..widgets::RowContent::default()
                        }),
                    ));
                }
                SectionItem::Item(entry) => {
                    let id = widgets::WidgetId::new(format!("modal-option-{selectable_index}"));
                    selectable_index += 1;
                    focus_order.push(id.clone());
                    if entry.selected {
                        set_current_focused_id(id.clone());
                    }
                    let height = if entry.detail.is_empty() {
                        MENU_ROW_HEIGHT + 2.0
                    } else {
                        ui_kit::ITEM_ROW_HEIGHT as f32
                    };
                    sections.push(widgets::UiNode::Button(
                        widgets::ButtonNode::identified(
                            Some(id),
                            &entry.label,
                            width,
                            height,
                            entry.selected,
                            BONE,
                        )
                        .with_row(widgets::RowContent {
                            value: entry.value.clone(),
                            value_color: Some(entry.value_color),
                            detail: entry.detail.clone(),
                            icon: entry.icon,
                            coin: entry.coin,
                            face: if entry.disabled {
                                widgets::RowFace::Disabled
                            } else {
                                widgets::RowFace::Raised
                            },
                        }),
                    ));
                }
                SectionItem::Text(text) => sections.push(widgets::UiNode::Text(
                    widgets::TextNode::colored(text, TextKind::Small, width, MIST),
                )),
                SectionItem::Field { text, placeholder } => {
                    sections.push(widgets::UiNode::Button(
                        widgets::ButtonNode::label(text, width, MENU_ROW_HEIGHT + 2.0, false, BONE)
                            .with_row(widgets::RowContent {
                                detail: placeholder.clone(),
                                face: widgets::RowFace::Field,
                                ..widgets::RowContent::default()
                            }),
                    ));
                }
            }
        }
    }
    set_current_focus_order(focus_order);
    widgets::UiNode::Scroll(widgets::ScrollNode::vertical(
        current_scroll_offset(&widgets::WidgetId::new("modal-content")),
        widgets::UiNode::Stack(widgets::StackNode::vertical(3.0, sections)),
    ))
}

/// Scrolls the modal when the focused row changes (or the dialog opens); returns whether the
/// offset changed (the content must then be laid out again).
///
/// The first selectable row reveals everything above it (as far up as keeps the row in view)
/// and the last reveals everything below it, so headings, stats and notes before the first
/// or after the last button are reachable with the selection alone. Any other row scrolls just
/// enough to stay fully visible, and not at all while it already is. Nothing happens until
/// the focus moves again, so wheel / Page / stick scrolling in between is never undone.
fn follow_focused_row(node: &widgets::UiNode, body: Rectangle) -> bool {
    let plan = node.render_plan();
    let rows: Vec<(&widgets::WidgetId, Rectangle, bool)> = plan
        .commands()
        .iter()
        .filter_map(|command| match command {
            widgets::RenderCommand::Button {
                rect,
                focused,
                id: Some(id),
                ..
            } => Some((id, *rect, *focused)),
            _ => None,
        })
        .collect();
    let Some(index) = rows.iter().position(|(_, _, focused)| *focused) else {
        return false;
    };
    let (id, rect, _) = rows[index];
    let scroll_id = widgets::WidgetId::new("modal-content");
    CURRENT_UI_STATE.with(|current| {
        let mut current = current.borrow_mut();
        let Some(state) = current.as_mut() else {
            return false;
        };
        if !state.follow(id) {
            return false;
        }
        let offset = state.scroll_offset(&scroll_id);
        let limit = state.scroll_limit(&scroll_id);
        let edge = if index == 0 {
            Some(false)
        } else if index + 1 == rows.len() {
            Some(true)
        } else {
            None
        };
        let target = follow_target(rect, body, offset, limit, edge);
        if (target - offset).abs() < 0.5 {
            return false;
        }
        state.set_scroll_offset(scroll_id, target);
        true
    })
}

/// The scroll offset that shows focused row `rect` (laid out at `offset`) inside `body`.
/// `edge` is `Some(false)` for the first selectable row (scroll toward the top), `Some(true)`
/// for the last (toward the bottom, up to `limit`) and `None` for a middle row (the smallest
/// change that keeps it fully visible).
fn follow_target(
    rect: Rectangle,
    body: Rectangle,
    offset: f32,
    limit: f32,
    edge: Option<bool>,
) -> f32 {
    // Keep a little context above and below the row.
    const MARGIN: f32 = 4.0;
    // The row's top and bottom in content coordinates (independent of the current offset).
    let top = rect.y - body.y + offset;
    let bottom = top + rect.height;
    // Offsets that keep the row fully visible with its margin: [lowest, highest].
    let lowest = (bottom + MARGIN - body.height).max(0.0);
    let highest = (top - MARGIN).max(lowest);
    let target = match edge {
        Some(false) => lowest,
        Some(true) => highest.min(limit.max(lowest)),
        None => offset.clamp(lowest, highest),
    };
    target.clamp(0.0, limit.max(0.0))
}

fn is_selectable_stat(stat: &StatItem) -> bool {
    let label = stat.label.as_str();
    label.starts_with("> ") || label.starts_with("  ")
}

fn inset(rect: Rectangle, insets: Insets) -> Rectangle {
    Rectangle {
        x: rect.x + insets.left,
        y: rect.y + insets.top,
        width: (rect.width - insets.left - insets.right).max(0.0),
        height: (rect.height - insets.top - insets.bottom).max(0.0),
    }
}

fn ratio(value: f32, max: f32) -> f32 {
    if max <= f32::EPSILON {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

const TEXT_SHADOW_OFFSET: f32 = 1.0;

fn wrapped_height(text: &str, width: f32, kind: TextKind) -> f32 {
    layout_text(text, width, kind).size.height
}

#[derive(Clone, Debug, PartialEq)]
struct TextLineLayout {
    text: String,
    x: f32,
    y: f32,
    width: f32,
    baseline_y: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct TextLayout {
    lines: Vec<TextLineLayout>,
    size: Size,
}

fn layout_text(text: &str, width: f32, kind: TextKind) -> TextLayout {
    let metrics = font_metrics(kind);
    let lines = wrap_text(text, width, kind)
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let y = index as f32 * metrics.line_height;
            let measured_width = measure_text(&text, kind) + TEXT_SHADOW_OFFSET;
            TextLineLayout {
                text,
                x: 0.0,
                y,
                width: measured_width,
                baseline_y: y + metrics.baseline,
            }
        })
        .collect::<Vec<_>>();
    let height = if lines.is_empty() {
        0.0
    } else {
        (lines.len().saturating_sub(1) as f32 * metrics.line_height)
            + measure_text_height(kind)
            + TEXT_SHADOW_OFFSET
    };
    let measured_width = lines.iter().map(|line| line.width).fold(0.0_f32, f32::max);
    TextLayout {
        lines,
        size: Size {
            width: measured_width,
            height,
        },
    }
}

fn wrap_text(text: &str, width: f32, kind: TextKind) -> Vec<String> {
    wrap_text_with_measure(text, (width - TEXT_SHADOW_OFFSET).max(0.0), |candidate| {
        measure_text(candidate, kind)
    })
}

fn measure_text(text: &str, kind: TextKind) -> f32 {
    TEXT_MEASURE_CACHE.with(|cache| {
        let key = TextMeasureKey {
            kind,
            text: text.to_owned(),
        };
        if let Some(width) = cache.borrow().get(&key).copied() {
            return width;
        }
        let width = measure_text_uncached(text, kind);
        cache.borrow_mut().insert(key, width);
        width
    })
}

fn measure_text_uncached(text: &str, kind: TextKind) -> f32 {
    measure_text_size(text, kind).width
}

fn measure_text_height(kind: TextKind) -> f32 {
    measure_text_size("Mg", kind).height
}

fn measure_text_size(text: &str, kind: TextKind) -> Size {
    Size {
        width: ui_kit::text_width(text, kind.style()) as f32,
        height: font_metrics(kind).font_size,
    }
}

#[allow(
    dead_code,
    reason = "formal widget tree foundation is being introduced before screen call sites are migrated to it"
)]
pub(super) mod widgets;

/// Pixel-font metrics in canvas art pixels (glyph cell, line pitch, baseline).
#[derive(Clone, Copy, Debug)]
struct FontMetrics {
    font_size: f32,
    line_height: f32,
    baseline: f32,
}

const fn font_metrics(kind: TextKind) -> FontMetrics {
    let style = kind.style();
    FontMetrics {
        font_size: style.height() as f32,
        line_height: style.line_height() as f32,
        baseline: (super::pixel_art::font::CAP_HEIGHT * style.scale()) as f32,
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TextMeasureKey {
    kind: TextKind,
    text: String,
}

thread_local! {
    static TEXT_MEASURE_CACHE: RefCell<BTreeMap<TextMeasureKey, f32>> = const { RefCell::new(BTreeMap::new()) };
    static CURRENT_UI_STATE: RefCell<Option<widgets::UiState>> = const { RefCell::new(None) };
    static CURRENT_MODAL_PLACEMENT: std::cell::Cell<ModalPlacement> = const { std::cell::Cell::new(ModalPlacement::Centred) };
}

/// Where this frame's dialogs sit.
pub(super) fn set_modal_placement(placement: ModalPlacement) {
    CURRENT_MODAL_PLACEMENT.with(|current| current.set(placement));
}

fn current_modal_placement() -> ModalPlacement {
    CURRENT_MODAL_PLACEMENT.with(std::cell::Cell::get)
}

pub(super) fn set_current_ui_state(state: widgets::UiState) {
    CURRENT_UI_STATE.with(|current| *current.borrow_mut() = Some(state));
}

pub(super) fn take_current_ui_state() -> Option<widgets::UiState> {
    CURRENT_UI_STATE.with(|current| current.borrow_mut().take())
}

fn set_current_scroll_limit(id: widgets::WidgetId, max_offset: f32) {
    CURRENT_UI_STATE.with(|current| {
        if let Some(state) = current.borrow_mut().as_mut() {
            state.set_scroll_limit(id, max_offset);
        }
    });
}

fn set_current_page_step(id: widgets::WidgetId, step: f32) {
    CURRENT_UI_STATE.with(|current| {
        if let Some(state) = current.borrow_mut().as_mut() {
            state.set_page_step(id, step);
        }
    });
}

fn set_current_focus_order(order: Vec<widgets::WidgetId>) {
    CURRENT_UI_STATE.with(|current| {
        if let Some(state) = current.borrow_mut().as_mut() {
            state.set_focus_order(order);
        }
    });
}

fn set_current_focused_id(id: widgets::WidgetId) {
    CURRENT_UI_STATE.with(|current| {
        if let Some(state) = current.borrow_mut().as_mut() {
            state.set_focused(id);
        }
    });
}

fn current_focused_id() -> Option<widgets::WidgetId> {
    CURRENT_UI_STATE.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(widgets::UiState::focused)
    })
}

fn current_scroll_offset(id: &widgets::WidgetId) -> f32 {
    CURRENT_UI_STATE.with(|current| {
        current
            .borrow()
            .as_ref()
            .map_or(0.0, |state| state.scroll_offset(id))
    })
}

/// The dialog rect on a canvas `viewport`, in whole art pixels.
fn modal_rect(viewport: Rectangle, placement: ModalPlacement) -> Rectangle {
    match placement {
        ModalPlacement::Centred => modal_rect_for_viewport(viewport),
        ModalPlacement::Counter => {
            let width = (viewport.width * 0.52)
                .clamp(250.0, 330.0)
                .min(viewport.width - 16.0)
                .max(0.0);
            let height = (viewport.height - 40.0).clamp(0.0, 320.0);
            Rectangle {
                x: (viewport.x + 8.0).floor(),
                y: (viewport.y + 8.0).floor(),
                width: width.floor(),
                height: height.floor(),
            }
        }
    }
}

fn modal_rect_for_viewport(viewport: Rectangle) -> Rectangle {
    // Never larger than the viewport minus a margin, even below the preferred minimum.
    let width = (viewport.width * 0.78)
        .clamp(300.0, 500.0)
        .min(viewport.width - 12.0)
        .max(0.0)
        .floor();
    let height = (viewport.height * 0.88)
        .clamp(200.0, 330.0)
        .min(viewport.height - 12.0)
        .max(0.0)
        .floor();
    Rectangle {
        x: (viewport.x + (viewport.width - width) * 0.5).floor(),
        y: (viewport.y + (viewport.height - height) * 0.5).floor(),
        width,
        height,
    }
}

fn wrap_text_with_measure(
    text: &str,
    width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let width = width.max(0.0);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        wrap_paragraph(paragraph, width, &mut measure, &mut lines);
    }
    lines
}

fn wrap_paragraph(
    paragraph: &str,
    width: f32,
    measure: &mut impl FnMut(&str) -> f32,
    lines: &mut Vec<String>,
) {
    let mut current = String::new();
    for word in paragraph.split_whitespace() {
        if word_fits_or_width_is_unbounded(word, width, measure) {
            append_word_or_flush_line(word, width, measure, lines, &mut current);
        } else {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.extend(split_oversized_word(word, width, measure));
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
}

fn append_word_or_flush_line(
    word: &str,
    width: f32,
    measure: &mut impl FnMut(&str) -> f32,
    lines: &mut Vec<String>,
    current: &mut String,
) {
    let candidate = if current.is_empty() {
        word.to_owned()
    } else {
        format!("{current} {word}")
    };
    if measure(&candidate) <= width || current.is_empty() {
        current.clone_from(&candidate);
    } else {
        lines.push(std::mem::take(current));
        current.push_str(word);
    }
}

fn word_fits_or_width_is_unbounded(
    word: &str,
    width: f32,
    measure: &mut impl FnMut(&str) -> f32,
) -> bool {
    measure(word) <= width
}

fn split_oversized_word(
    word: &str,
    width: f32,
    measure: &mut impl FnMut(&str) -> f32,
) -> Vec<String> {
    if width <= f32::EPSILON {
        return word
            .chars()
            .map(|character| character.to_string())
            .collect();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for character in word.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        if current.is_empty() || measure(&candidate) <= width {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push(character);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn modal_rect_is_centered_and_bounded() {
        let rect = modal_rect_for_viewport(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 720.0,
        });
        assert!(rect.width <= 500.0);
        assert!(rect.height <= 330.0);
        assert!((rect.x - ((1280.0 - rect.width) * 0.5)).abs() <= 1.0);
    }

    #[test]
    fn modal_rect_fits_every_supported_screen() {
        for (width, height) in [
            (960.0, 540.0),
            (1280.0, 720.0),
            (1920.0, 1080.0),
            (2560.0, 1080.0),
            (480.0, 270.0),
        ] {
            let viewport = Rectangle {
                x: 0.0,
                y: 0.0,
                width,
                height,
            };
            let rect = modal_rect_for_viewport(viewport);
            assert!(
                rect.x >= 0.0 && rect.y >= 0.0,
                "{rect:?} at {width}x{height}"
            );
            assert!(rect.x + rect.width <= width && rect.y + rect.height <= height);
            assert!((rect.x + rect.width * 0.5 - width * 0.5).abs() <= 1.0);
            assert!((rect.y + rect.height * 0.5 - height * 0.5).abs() <= 1.0);
            assert_near(rect.x.fract(), 0.0);
            assert_near(rect.width.fract(), 0.0);
        }
    }

    #[test]
    fn text_wrap_respects_max_width_for_word_boundaries() {
        let lines = wrap_text_with_measure("alpha beta gamma", 10.0, |text| text.len() as f32);
        assert_eq!(lines, ["alpha beta", "gamma"]);
    }

    #[test]
    fn text_wrap_preserves_explicit_newlines_and_splits_oversized_words() {
        let lines = wrap_text_with_measure("superlong\nnext", 5.0, |text| text.len() as f32);
        assert_eq!(lines, ["super", "long", "next"]);
    }

    #[test]
    fn text_wrap_handles_blank_lines_and_zero_width() {
        let lines = wrap_text_with_measure("a\n\nb", 0.0, |text| text.len() as f32);
        assert_eq!(lines, ["a", "", "b"]);
    }

    #[test]
    fn text_layout_tracks_line_boxes_and_baselines() {
        let layout = layout_text("alpha", 200.0, TextKind::Small);
        assert_eq!(layout.lines.len(), 1);
        assert_near(layout.lines[0].y, 0.0);
        assert_near(
            layout.lines[0].baseline_y,
            font_metrics(TextKind::Small).baseline,
        );
        assert!(layout.size.height >= measure_text_height(TextKind::Small) + TEXT_SHADOW_OFFSET);
    }

    #[test]
    fn configured_font_metrics_have_ordered_baselines() {
        for kind in [TextKind::Heading, TextKind::Small] {
            let metrics = font_metrics(kind);
            assert!(metrics.font_size > 0.0);
            assert!(metrics.line_height >= metrics.font_size);
            assert!(metrics.baseline > 0.0 && metrics.baseline <= metrics.line_height);
        }
    }

    #[test]
    fn widget_stack_lays_out_children_sequentially() {
        use widgets::{LayoutConstraints, StackNode, TextNode, UiNode};
        let mut node = UiNode::Stack(StackNode::vertical(
            4.0,
            vec![
                UiNode::Text(TextNode::sized(20.0, 10.0)),
                UiNode::Text(TextNode::sized(30.0, 12.0)),
            ],
        ));
        let measured = node.measure(LayoutConstraints::loose(100.0, 100.0));
        assert_near(measured.width, 30.0);
        assert_near(measured.height, 26.0);
        node.layout(Rectangle {
            x: 5.0,
            y: 7.0,
            width: 100.0,
            height: measured.height,
        });
        let UiNode::Stack(stack) = node else {
            panic!("expected stack")
        };
        assert_near(stack.children[0].rect().y, 7.0);
        assert_near(stack.children[1].rect().y, 21.0);
    }

    #[test]
    fn widget_panel_applies_padding_to_child() {
        use widgets::{LayoutConstraints, PanelNode, TextNode, UiNode};
        let mut node = UiNode::Panel(PanelNode::new(
            Insets::all(5.0),
            UiNode::Text(TextNode::sized(20.0, 10.0)),
        ));
        let measured = node.measure(LayoutConstraints::loose(100.0, 100.0));
        assert_near(measured.width, 30.0);
        assert_near(measured.height, 20.0);
        node.layout(Rectangle {
            x: 10.0,
            y: 20.0,
            width: measured.width,
            height: measured.height,
        });
        let UiNode::Panel(panel) = node else {
            panic!("expected panel")
        };
        assert_near(panel.child.rect().x, 15.0);
        assert_near(panel.child.rect().y, 25.0);
    }

    /// Selects each row of a long list in turn (as Down presses would) and returns the dialog's
    /// scroll offset after each frame.
    fn scroll_offsets_while_stepping_down(rows: usize, body: Rectangle) -> Vec<f32> {
        let scroll_id = widgets::WidgetId::new("modal-content");
        set_current_ui_state(widgets::UiState::default());
        let mut offsets = Vec::new();
        for selected in 0..rows {
            let items = (0..rows)
                .map(|row| {
                    let marker = if row == selected { "> " } else { "  " };
                    SectionItem::stat(format!("{marker}Row {row}"), "", BONE)
                })
                .collect();
            let content = ModalContent::new(vec![Section::new("List", BONE, items)]);
            CURRENT_UI_STATE.with(|current| {
                if let Some(state) = current.borrow_mut().as_mut() {
                    state.enter_modal("List", &scroll_id);
                }
            });
            let mut node = modal_content_node(&content, body.width);
            node.layout(body);
            if let widgets::UiNode::Scroll(scroll) = &node {
                set_current_scroll_limit(
                    scroll_id.clone(),
                    (scroll.content_height - body.height).max(0.0),
                );
            }
            let _ = follow_focused_row(&node, body);
            offsets.push(current_scroll_offset(&scroll_id));
        }
        let _ = take_current_ui_state();
        offsets
    }

    #[test]
    fn keyboard_selection_scrolls_only_when_the_row_leaves_the_view() {
        let body = Rectangle::new(0.0, 0.0, 200.0, 120.0);
        let offsets = scroll_offsets_while_stepping_down(20, body);
        let first_scroll = offsets
            .iter()
            .position(|offset| *offset > 0.0)
            .expect("a 20-row list taller than the body scrolls eventually");
        assert!(
            first_scroll >= 3,
            "rows already visible must not scroll the list: {offsets:?}"
        );
        assert!(
            offsets.windows(2).all(|pair| pair[1] >= pair[0]),
            "stepping down never scrolls back up: {offsets:?}"
        );
        // Once following, each step scrolls by about one row, never a fixed jump past it.
        let step = offsets[first_scroll + 1] - offsets[first_scroll];
        assert!(step > 0.0 && step <= MENU_ROW_HEIGHT + 4.0, "{offsets:?}");
    }

    /// A dialog with a status block above `rows` buttons (row `selected` marked) and notes
    /// below them: the trailing-information layout the scroll fixes are about.
    fn framed_list(rows: usize, selected: usize) -> ModalContent {
        let status = (0..4)
            .map(|line| SectionItem::stat(format!("Status {line}"), "1", BONE))
            .collect();
        let items = (0..rows)
            .map(|row| {
                let marker = if row == selected { "> " } else { "  " };
                SectionItem::stat(format!("{marker}Row {row}"), "", BONE)
            })
            .collect();
        let notes = (0..4)
            .map(|line| SectionItem::stat(format!("Note {line}"), "2", BONE))
            .collect();
        ModalContent::new(vec![
            Section::new("Status", BONE, status),
            Section::new("Rows", BONE, items),
            Section::new("Notes", BONE, notes),
        ])
    }

    const LIST_BODY: Rectangle = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 120.0,
    };

    /// Lays out one frame of the "List" dialog the way `modal_with_render_plan` does (state
    /// bookkeeping only, no drawing) and returns the state afterwards.
    fn lay_out_frame(state: widgets::UiState, content: &ModalContent) -> widgets::UiState {
        let scroll_id = widgets::WidgetId::new("modal-content");
        let mut state = state;
        state.begin_frame();
        set_current_ui_state(state);
        CURRENT_UI_STATE.with(|current| {
            if let Some(state) = current.borrow_mut().as_mut() {
                state.enter_modal("List", &scroll_id);
            }
        });
        let mut node = modal_content_node(content, LIST_BODY.width);
        node.layout(LIST_BODY);
        if let widgets::UiNode::Scroll(scroll) = &node {
            set_current_scroll_limit(
                scroll_id.clone(),
                (scroll.content_height - LIST_BODY.height).max(0.0),
            );
            set_current_page_step(
                scroll_id,
                (LIST_BODY.height - MENU_ROW_HEIGHT).max(MENU_ROW_HEIGHT),
            );
        }
        let _ = follow_focused_row(&node, LIST_BODY);
        let mut state = take_current_ui_state().expect("state was set");
        state.end_frame();
        state
    }

    fn offset_of(state: &widgets::UiState) -> f32 {
        state.scroll_offset(&widgets::WidgetId::new("modal-content"))
    }

    fn limit_of(state: &widgets::UiState) -> f32 {
        state.scroll_limit(&widgets::WidgetId::new("modal-content"))
    }

    #[test]
    fn first_and_last_rows_reveal_the_content_before_and_after_them() {
        let rows = 8;
        let mut state = lay_out_frame(widgets::UiState::default(), &framed_list(rows, 0));
        assert!(limit_of(&state) > 0.0, "the list overflows the body");
        assert_near(offset_of(&state), 0.0);
        for selected in 1..rows {
            state = lay_out_frame(state, &framed_list(rows, selected));
        }
        assert_near(offset_of(&state), limit_of(&state));
        // Wrapping round to the first row goes back to the very top (the status block).
        state = lay_out_frame(state, &framed_list(rows, 0));
        assert_near(offset_of(&state), 0.0);
        // A dialog that opens on its last row starts scrolled to the bottom.
        let reopened = lay_out_frame(widgets::UiState::default(), &framed_list(rows, rows - 1));
        assert_near(offset_of(&reopened), limit_of(&reopened));
    }

    #[test]
    fn edge_reveal_keeps_a_row_taller_than_the_gap_in_view() {
        let body = Rectangle::new(0.0, 10.0, 100.0, 50.0);
        // First row sits 60px down the content: scrolling to 0 would hide it, so stop where
        // its bottom (plus margin) is just visible.
        let first = Rectangle::new(0.0, 10.0 + 60.0, 100.0, 16.0);
        assert_near(follow_target(first, body, 0.0, 200.0, Some(false)), 30.0);
        // Last row near the top of 300px of content: the bottom would hide it, so stop where
        // its top (less margin) is at the top of the body.
        let last = Rectangle::new(0.0, 10.0 + 40.0, 100.0, 16.0);
        assert_near(follow_target(last, body, 0.0, 250.0, Some(true)), 36.0);
        // Middle rows that are visible do not move.
        let middle = Rectangle::new(0.0, 10.0 + 20.0, 100.0, 16.0);
        assert_near(follow_target(middle, body, 0.0, 250.0, None), 0.0);
    }

    #[test]
    fn manual_scrolling_persists_until_the_focus_moves() {
        use super::super::apply_modal_scroll;
        let rows = 8;
        let mut state = lay_out_frame(widgets::UiState::default(), &framed_list(rows, 2));
        let opened = offset_of(&state);
        // Wheel notches down reach the scroll offset and survive the next frames.
        let wheel_down = crate::input::PlayerInput {
            ui_scroll: -3.0,
            ..crate::input::PlayerInput::default()
        };
        apply_modal_scroll(&mut state, wheel_down, false);
        let scrolled = offset_of(&state);
        assert_near(scrolled, (opened + 144.0).min(limit_of(&state)));
        assert!(scrolled > opened);
        for _ in 0..3 {
            state = lay_out_frame(state, &framed_list(rows, 2));
        }
        assert_near(offset_of(&state), scrolled);
        // Moving the selection follows it again (row 3 is scrolled out of view above).
        state = lay_out_frame(state, &framed_list(rows, 3));
        assert!(offset_of(&state) < scrolled, "{}", offset_of(&state));
    }

    #[test]
    fn page_keys_home_and_end_scroll_unless_the_overlay_owns_them() {
        use super::super::apply_modal_scroll;
        use crate::input::PlayerInput;
        let rows = 20;
        let mut state = lay_out_frame(widgets::UiState::default(), &framed_list(rows, 0));
        let page = LIST_BODY.height - MENU_ROW_HEIGHT;
        let page_down = PlayerInput {
            page_down: true,
            ..PlayerInput::default()
        };
        apply_modal_scroll(&mut state, page_down, true);
        assert_near(offset_of(&state), 0.0);
        apply_modal_scroll(&mut state, page_down, false);
        assert_near(offset_of(&state), page);
        state = lay_out_frame(state, &framed_list(rows, 0));
        assert_near(offset_of(&state), page);
        let page_up = PlayerInput {
            page_up: true,
            ..PlayerInput::default()
        };
        apply_modal_scroll(&mut state, page_up, false);
        assert_near(offset_of(&state), 0.0);
        let end = PlayerInput {
            scroll_end: true,
            ..PlayerInput::default()
        };
        apply_modal_scroll(&mut state, end, false);
        assert_near(offset_of(&state), limit_of(&state));
        let home = PlayerInput {
            scroll_home: true,
            ..PlayerInput::default()
        };
        apply_modal_scroll(&mut state, home, false);
        assert_near(offset_of(&state), 0.0);
    }

    #[test]
    fn scrolling_without_an_open_dialog_does_nothing_and_reopening_starts_fresh() {
        use super::super::apply_modal_scroll;
        let rows = 8;
        let mut state = lay_out_frame(widgets::UiState::default(), &framed_list(rows, 0));
        // A frame without the dialog closes it.
        state.begin_frame();
        state.end_frame();
        assert!(!state.has_modal());
        let wheel_down = crate::input::PlayerInput {
            ui_scroll: -3.0,
            ..crate::input::PlayerInput::default()
        };
        apply_modal_scroll(&mut state, wheel_down, false);
        assert_near(offset_of(&state), 0.0);
        let reopened = lay_out_frame(state, &framed_list(rows, 0));
        assert!(reopened.has_modal());
        assert_near(offset_of(&reopened), 0.0);
    }

    #[test]
    fn scroll_node_clamps_content_offset() {
        use widgets::{LayoutConstraints, ScrollNode, SpacerNode, UiNode};
        let mut node = UiNode::Scroll(ScrollNode::vertical(
            500.0,
            UiNode::Spacer(SpacerNode::sized(20.0, 200.0)),
        ));
        node.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 80.0,
        });
        let UiNode::Scroll(ref scroll) = node else {
            panic!("expected scroll")
        };
        assert_near(scroll.content_height, 200.0);
        assert_near(scroll.child.rect().y, -120.0);
        assert_near(
            node.measure(LayoutConstraints::loose(50.0, 80.0)).height,
            80.0,
        );
    }

    #[test]
    fn ui_state_tracks_focus_and_scroll_offsets() {
        use widgets::{UiState, WidgetId};
        let inventory = WidgetId::new("inventory");
        let depot = WidgetId::new("depot");
        let mut state = UiState::default();
        assert_eq!(state.focused(), None);
        state.set_focused(inventory.clone());
        assert_eq!(state.focused(), Some(inventory.clone()));
        state.set_focus_order(vec![inventory.clone(), depot.clone()]);
        state.focus_next();
        assert_eq!(state.focused(), Some(depot.clone()));
        state.focus_previous();
        assert_eq!(state.focused(), Some(inventory.clone()));
        state.set_scroll_offset(inventory.clone(), 12.0);
        state.set_scroll_limit(inventory.clone(), 15.0);
        state.scroll_by(inventory.clone(), 10.0, 18.0);
        state.scroll_by(depot.clone(), -10.0, 100.0);
        assert_near(state.scroll_offset(&inventory), 15.0);
        assert_near(state.scroll_offset(&depot), 0.0);
    }

    #[test]
    fn widget_render_plan_preserves_panel_clip_and_text_order() {
        use widgets::{PanelNode, RenderCommand, ScrollNode, TextNode, UiNode};
        let mut node = UiNode::Panel(PanelNode::new(
            Insets::all(4.0),
            UiNode::Scroll(ScrollNode::vertical(
                0.0,
                UiNode::Text(TextNode::sized(20.0, 10.0)),
            )),
        ));
        node.layout(Rectangle {
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 20.0,
        });
        let plan = node.render_plan();
        assert!(matches!(plan.commands()[0], RenderCommand::Panel { .. }));
        assert!(matches!(plan.commands()[1], RenderCommand::Clip { .. }));
        assert!(matches!(plan.commands()[2], RenderCommand::Text { .. }));
        assert!(matches!(plan.commands()[3], RenderCommand::EndClip));
    }

    #[test]
    fn modal_selectable_rows_emit_stable_button_ids_and_focus_selected_row() {
        let content = ModalContent::new(vec![Section::new(
            "Options",
            SKY_LIGHT,
            vec![
                SectionItem::stat("  First", "", BONE),
                SectionItem::stat("> Second", "", GOLD),
            ],
        )]);
        set_current_ui_state(widgets::UiState::default());
        let mut node = modal_content_node(&content, 240.0);
        node.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 100.0,
        });
        let plan = node.render_plan();
        assert!(plan.commands().iter().any(|command| matches!(
            command,
            widgets::RenderCommand::Button {
                id: Some(id),
                focused: false,
                ..
            } if id == &widgets::WidgetId::new("modal-option-0")
        )));
        assert!(plan.commands().iter().any(|command| matches!(
            command,
            widgets::RenderCommand::Button {
                id: Some(id),
                focused: true,
                ..
            } if id == &widgets::WidgetId::new("modal-option-1")
        )));
        assert_eq!(
            take_current_ui_state().and_then(|state| state.focused()),
            Some(widgets::WidgetId::new("modal-option-1")),
        );
    }

    #[test]
    fn rich_nodes_emit_render_commands() {
        use widgets::{ButtonNode, CanvasNode, MeterNode, RenderCommand, UiNode};
        let mut meter = UiNode::Meter(MeterNode::new(0.5, 100.0, 10.0));
        meter.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 10.0,
        });
        let meter_plan = meter.render_plan();
        assert!(matches!(
            meter_plan.commands()[0],
            RenderCommand::Meter { ratio: 0.5, .. }
        ));

        let mut button = UiNode::Button(ButtonNode::sized(80.0, 24.0, true));
        button.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        });
        let button_plan = button.render_plan();
        assert!(matches!(
            button_plan.commands()[0],
            RenderCommand::Button { focused: true, .. }
        ));

        let mut canvas = UiNode::Canvas(CanvasNode::new(120.0, 80.0).with_aspect_ratio(2.0));
        canvas.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 80.0,
        });
        let canvas_plan = canvas.render_plan();
        assert!(matches!(
            canvas_plan.commands()[0],
            RenderCommand::Canvas { .. }
        ));
    }

    #[test]
    fn responsive_grid_assigns_columns_and_cells() {
        use widgets::{GridNode, TextNode, UiNode};
        let mut node = UiNode::Grid(GridNode::responsive(
            50.0,
            10.0,
            vec![
                UiNode::Text(TextNode::sized(20.0, 10.0)),
                UiNode::Text(TextNode::sized(20.0, 20.0)),
                UiNode::Text(TextNode::sized(20.0, 12.0)),
            ],
        ));
        node.layout(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 100.0,
        });
        let UiNode::Grid(grid) = node else {
            panic!("expected grid")
        };
        assert_eq!(grid.columns, 2);
        assert_near(grid.children[0].rect().width, 55.0);
        assert_near(grid.children[2].rect().y, 30.0);
    }
}
