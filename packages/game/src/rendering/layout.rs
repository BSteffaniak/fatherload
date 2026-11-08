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

    /// A dialog frame with a title and subtitle; returns the body rect for custom drawing.
    pub(super) fn canvas_modal(&mut self, title: &str, subtitle: &str, summary: &str) -> Rectangle {
        let rect = self.dialog_rect(false);
        let body = self.modal_frame(title, subtitle, rect, None);
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
        // Measure the content first so short dialogs shrink to fit it.
        let full = self.dialog_rect(content.compact);
        let content_width = (full.width - 24.0 - 8.0).max(0.0);
        let mut node = modal_content_node(content, content_width);
        node.layout(Rectangle::new(0.0, 0.0, content_width, 100_000.0));
        let content_height = match &node {
            widgets::UiNode::Scroll(scroll) => scroll.content_height,
            other => other.rect().height,
        };
        let body = self.modal_frame(title, subtitle, full, Some(content_height));
        // Room on the left for the selection cursor.
        let body = Rectangle {
            x: body.x + 8.0,
            width: (body.width - 8.0).max(0.0),
            ..body
        };
        let mut node = modal_content_node(content, body.width);
        node.layout(body);
        if let widgets::UiNode::Scroll(scroll) = &node {
            set_current_scroll_limit(
                widgets::WidgetId::new("modal-content"),
                (scroll.content_height - body.height).max(0.0),
            );
        }
        let plan = node.render_plan();
        self.render_plan(&plan);
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

    /// Dims the screen, draws the dialog panel, title, subtitle and divider; returns the body.
    /// With `content_height` the panel shrinks to fit that much body (centred dialogs only).
    fn modal_frame(
        &mut self,
        title: &str,
        subtitle: &str,
        mut rect: Rectangle,
        content_height: Option<f32>,
    ) -> Rectangle {
        let placement = current_modal_placement();
        if placement == ModalPlacement::Centred {
            ui_kit::dim(self.draw, self.viewport);
        }
        let width = rect.width as i32 - 24;
        let lines = ui_kit::wrap(subtitle, width, TextStyle::Body);
        let subtitle_lines = i32::try_from(lines.len().min(2)).unwrap_or(1);
        let header =
            10 + TextStyle::Title.height() + 3 + subtitle_lines * TextStyle::Body.line_height() + 8;
        if let (Some(content), ModalPlacement::Centred) = (content_height, placement) {
            let wanted = (header as f32 + content + 14.0).ceil().max(120.0);
            if wanted < rect.height {
                rect.y = (rect.y + (rect.height - wanted) * 0.5).floor();
                rect.height = wanted;
            }
        }
        ui_kit::panel(self.draw, rect, PanelStyle::Accent);
        let x = rect.x as i32 + 12;
        let mut y = rect.y as i32 + 10;
        ui_kit::text(self.draw, title, x, y, TextStyle::Title, GOLD);
        y += TextStyle::Title.height() + 3;
        for line in lines.iter().take(2) {
            ui_kit::text(self.draw, line, x, y, TextStyle::Body, FOG);
            y += TextStyle::Body.line_height();
        }
        y += 2;
        ui_kit::divider(self.draw, x, y, width);
        y += 6;
        Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: (rect.y + rect.height - 10.0 - y as f32).max(0.0),
        }
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
}

impl ModalContent {
    pub(super) const fn new(sections: Vec<Section>) -> Self {
        Self {
            sections,
            compact: false,
        }
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
    let focused_id = current_focused_id();
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
            }
        }
    }
    set_current_focus_order(focus_order);
    widgets::UiNode::Scroll(widgets::ScrollNode::vertical(
        current_scroll_offset(&widgets::WidgetId::new("modal-content")),
        widgets::UiNode::Stack(widgets::StackNode::vertical(3.0, sections)),
    ))
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
