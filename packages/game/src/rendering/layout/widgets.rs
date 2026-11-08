//! A small retained widget tree: measure, layout and a render plan of draw commands.

use super::{Insets, PanelKind, Size, TextKind, inset};
use crate::rendering::pixel_art::sprite::SpriteDef;
use raylib::prelude::{Color, Rectangle};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::rendering) struct WidgetId(String);

impl WidgetId {
    pub(in crate::rendering) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::rendering) struct UiState {
    focused: Option<WidgetId>,
    scroll_offsets: BTreeMap<WidgetId, f32>,
    scroll_limits: BTreeMap<WidgetId, f32>,
    focus_order: Vec<WidgetId>,
    /// The focused row the modal scroll last brought into view.
    followed: Option<WidgetId>,
    /// Title of the dialog the scroll and follow state belong to.
    modal: Option<String>,
}

impl UiState {
    /// Starts tracking the dialog titled `title`: a different dialog than last frame starts
    /// scrolled to the top with nothing followed yet.
    pub(super) fn enter_modal(&mut self, title: &str, scroll_id: &WidgetId) {
        if self.modal.as_deref() == Some(title) {
            return;
        }
        self.modal = Some(title.to_owned());
        self.followed = None;
        self.scroll_offsets.insert(scroll_id.clone(), 0.0);
    }

    /// Records `id` as followed; returns whether it differs from the last followed row.
    pub(super) fn follow(&mut self, id: &WidgetId) -> bool {
        if self.followed.as_ref() == Some(id) {
            return false;
        }
        self.followed = Some(id.clone());
        true
    }

    pub(super) fn focused(&self) -> Option<WidgetId> {
        self.focused.clone()
    }

    pub(in crate::rendering) fn set_focused(&mut self, id: WidgetId) {
        self.focused = Some(id);
    }

    pub(in crate::rendering) fn set_focus_order(&mut self, order: Vec<WidgetId>) {
        self.focus_order = order;
        if let Some(focused) = &self.focused
            && !self.focus_order.iter().any(|id| id == focused)
        {
            self.focused = self.focus_order.first().cloned();
        }
    }

    pub(in crate::rendering) fn focus_next(&mut self) {
        self.move_focus(1);
    }

    pub(in crate::rendering) fn focus_previous(&mut self) {
        self.move_focus(-1);
    }

    fn move_focus(&mut self, direction: i32) {
        if self.focus_order.is_empty() {
            return;
        }
        let current = self
            .focused
            .as_ref()
            .and_then(|focused| self.focus_order.iter().position(|id| id == focused))
            .unwrap_or(0);
        let len = self.focus_order.len();
        let next = if direction.is_negative() {
            current.checked_sub(1).unwrap_or(len - 1)
        } else {
            (current + 1) % len
        };
        self.focused = Some(self.focus_order[next].clone());
    }

    pub(super) fn scroll_offset(&self, id: &WidgetId) -> f32 {
        self.scroll_offsets.get(id).copied().unwrap_or(0.0)
    }

    pub(super) fn set_scroll_offset(&mut self, id: WidgetId, offset: f32) {
        let limit = self.scroll_limit(&id);
        self.scroll_offsets.insert(id, offset.clamp(0.0, limit));
    }

    pub(in crate::rendering) fn set_scroll_limit(&mut self, id: WidgetId, max_offset: f32) {
        let max_offset = max_offset.max(0.0);
        self.scroll_limits.insert(id.clone(), max_offset);
        let offset = self.scroll_offset(&id).min(max_offset);
        self.scroll_offsets.insert(id, offset);
    }

    pub(super) fn scroll_limit(&self, id: &WidgetId) -> f32 {
        self.scroll_limits
            .get(id)
            .copied()
            .unwrap_or(f32::MAX / 4.0)
    }

    pub(in crate::rendering) fn scroll_by(&mut self, id: WidgetId, delta: f32, max_offset: f32) {
        let limit = self.scroll_limit(&id).min(max_offset.max(0.0));
        let next = (self.scroll_offset(&id) + delta).clamp(0.0, limit);
        self.scroll_offsets.insert(id, next);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LayoutConstraints {
    pub(super) min_width: f32,
    pub(super) max_width: f32,
    pub(super) min_height: f32,
    pub(super) max_height: f32,
}

impl LayoutConstraints {
    pub(super) const fn tight(width: f32, height: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }

    pub(super) const fn loose(max_width: f32, max_height: f32) -> Self {
        Self {
            min_width: 0.0,
            max_width,
            min_height: 0.0,
            max_height,
        }
    }

    pub(super) const fn constrain(self, size: Size) -> Size {
        Size {
            width: size.width.clamp(self.min_width, self.max_width),
            height: size.height.clamp(self.min_height, self.max_height),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum RenderCommand {
    Panel {
        rect: Rectangle,
        kind: PanelKind,
    },
    Text {
        rect: Rectangle,
        text: String,
        kind: TextKind,
        color: Color,
    },
    Meter {
        rect: Rectangle,
        ratio: f32,
        fill: Color,
        danger: Color,
    },
    Button {
        rect: Rectangle,
        focused: bool,
        id: Option<WidgetId>,
        text: String,
        color: Color,
        row: RowContent,
    },
    Canvas {
        rect: Rectangle,
    },
    Clip {
        rect: Rectangle,
    },
    EndClip,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RenderPlan {
    commands: Vec<RenderCommand>,
}

impl RenderPlan {
    pub(super) fn commands(&self) -> &[RenderCommand] {
        &self.commands
    }

    fn push(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug)]
pub(super) enum UiNode {
    Text(TextNode),
    Meter(MeterNode),
    Button(ButtonNode),
    Canvas(CanvasNode),
    Spacer(SpacerNode),
    Stack(StackNode),
    Grid(GridNode),
    Panel(PanelNode),
    Scroll(ScrollNode),
}

impl UiNode {
    pub(super) fn measure(&self, constraints: LayoutConstraints) -> Size {
        match self {
            Self::Text(node) => node.measure(constraints),
            Self::Meter(node) => node.measure(constraints),
            Self::Button(node) => node.measure(constraints),
            Self::Canvas(node) => node.measure(constraints),
            Self::Spacer(node) => node.measure(constraints),
            Self::Stack(node) => node.measure(constraints),
            Self::Grid(node) => node.measure(constraints),
            Self::Panel(node) => node.measure(constraints),
            Self::Scroll(node) => Self::measure_scroll(node, constraints),
        }
    }

    pub(super) fn layout(&mut self, rect: Rectangle) {
        match self {
            Self::Text(node) => node.rect = rect,
            Self::Meter(node) => node.rect = rect,
            Self::Button(node) => node.rect = rect,
            Self::Canvas(node) => node.rect = rect,
            Self::Spacer(node) => node.rect = rect,
            Self::Stack(node) => node.layout(rect),
            Self::Grid(node) => node.layout(rect),
            Self::Panel(node) => node.layout(rect),
            Self::Scroll(node) => node.layout(rect),
        }
    }

    pub(super) const fn rect(&self) -> Rectangle {
        match self {
            Self::Text(node) => node.rect,
            Self::Meter(node) => node.rect,
            Self::Button(node) => node.rect,
            Self::Canvas(node) => node.rect,
            Self::Spacer(node) => node.rect,
            Self::Stack(node) => node.rect,
            Self::Grid(node) => node.rect,
            Self::Panel(node) => node.rect,
            Self::Scroll(node) => node.rect,
        }
    }

    pub(super) fn render_plan(&self) -> RenderPlan {
        let mut plan = RenderPlan::default();
        self.collect_render_commands(&mut plan);
        plan
    }

    fn collect_render_commands(&self, plan: &mut RenderPlan) {
        match self {
            Self::Text(node) => plan.push(RenderCommand::Text {
                rect: node.rect,
                text: node.text.clone(),
                kind: node.kind,
                color: node.color,
            }),
            Self::Meter(node) => plan.push(RenderCommand::Meter {
                rect: node.rect,
                ratio: node.ratio,
                fill: node.fill,
                danger: node.danger,
            }),
            Self::Button(node) => plan.push(RenderCommand::Button {
                rect: node.rect,
                focused: node.focused,
                id: node.id.clone(),
                text: node.text.clone(),
                color: node.color,
                row: node.row.clone(),
            }),
            Self::Canvas(node) => plan.push(RenderCommand::Canvas { rect: node.rect }),
            Self::Spacer(_) => {}
            Self::Stack(node) => {
                for child in &node.children {
                    child.collect_render_commands(plan);
                }
            }
            Self::Grid(node) => {
                for child in &node.children {
                    child.collect_render_commands(plan);
                }
            }
            Self::Panel(node) => {
                plan.push(RenderCommand::Panel {
                    rect: node.rect,
                    kind: node.kind,
                });
                node.child.collect_render_commands(plan);
            }
            Self::Scroll(node) => {
                plan.push(RenderCommand::Clip { rect: node.rect });
                node.child.collect_render_commands(plan);
                plan.push(RenderCommand::EndClip);
            }
        }
    }

    const fn measure_scroll(_node: &ScrollNode, constraints: LayoutConstraints) -> Size {
        constraints.constrain(Size {
            width: constraints.max_width,
            height: constraints.max_height,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct TextNode {
    pub(super) text: String,
    pub(super) kind: TextKind,
    pub(super) color: Color,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) rect: Rectangle,
}

impl TextNode {
    pub(super) fn label(text: impl Into<String>, kind: TextKind, width: f32) -> Self {
        Self::colored(text, kind, width, Color::RAYWHITE)
    }

    pub(super) fn colored(
        text: impl Into<String>,
        kind: TextKind,
        width: f32,
        color: Color,
    ) -> Self {
        let text = text.into();
        let height = super::wrapped_height(&text, width, kind);
        Self {
            text,
            kind,
            color,
            width,
            height,
            rect: zero_rect(),
        }
    }

    pub(super) const fn sized(width: f32, height: f32) -> Self {
        Self {
            text: String::new(),
            kind: TextKind::Small,
            color: Color::RAYWHITE,
            width,
            height,
            rect: zero_rect(),
        }
    }

    const fn measure(&self, constraints: LayoutConstraints) -> Size {
        constraints.constrain(Size {
            width: self.width,
            height: self.height,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct MeterNode {
    pub(super) ratio: f32,
    pub(super) fill: Color,
    pub(super) danger: Color,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) rect: Rectangle,
}

impl MeterNode {
    pub(super) const fn new(ratio: f32, width: f32, height: f32) -> Self {
        Self::colored(ratio, width, height, Color::SKYBLUE, Color::RED)
    }

    pub(super) const fn colored(
        ratio: f32,
        width: f32,
        height: f32,
        fill: Color,
        danger: Color,
    ) -> Self {
        Self {
            ratio,
            fill,
            danger,
            width,
            height,
            rect: zero_rect(),
        }
    }

    const fn measure(&self, constraints: LayoutConstraints) -> Size {
        constraints.constrain(Size {
            width: self.width,
            height: self.height,
        })
    }
}

/// How a button row looks beyond its label: a right-aligned value, a detail line, an icon, and
/// whether it is a raised (selectable) face, a disabled face or a plain label/value row.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::rendering) struct RowContent {
    pub(in crate::rendering) value: String,
    pub(in crate::rendering) value_color: Option<Color>,
    pub(in crate::rendering) detail: String,
    pub(in crate::rendering) icon: Option<&'static SpriteDef>,
    pub(in crate::rendering) coin: bool,
    pub(in crate::rendering) face: RowFace,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::rendering) enum RowFace {
    /// A selectable button face.
    #[default]
    Raised,
    /// A button face greyed out (unaffordable / unavailable).
    Disabled,
    /// No face: a label on the left, the value on the right.
    Plain,
    /// A text entry well: the label is the typed text (or the placeholder in `detail` while
    /// empty), with a blinking caret.
    Field,
}

#[derive(Clone, Debug)]
pub(super) struct ButtonNode {
    pub(super) focused: bool,
    pub(super) id: Option<WidgetId>,
    pub(super) text: String,
    pub(super) color: Color,
    pub(super) row: RowContent,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) rect: Rectangle,
}

impl ButtonNode {
    pub(super) fn label(
        text: impl Into<String>,
        width: f32,
        height: f32,
        focused: bool,
        color: Color,
    ) -> Self {
        Self::identified(None, text, width, height, focused, color)
    }

    pub(super) fn identified(
        id: Option<WidgetId>,
        text: impl Into<String>,
        width: f32,
        height: f32,
        focused: bool,
        color: Color,
    ) -> Self {
        Self {
            focused,
            id,
            text: text.into(),
            color,
            row: RowContent::default(),
            width,
            height,
            rect: zero_rect(),
        }
    }

    pub(super) fn with_row(mut self, row: RowContent) -> Self {
        self.row = row;
        self
    }

    pub(super) fn sized(width: f32, height: f32, focused: bool) -> Self {
        Self::label(String::new(), width, height, focused, Color::RAYWHITE)
    }

    const fn measure(&self, constraints: LayoutConstraints) -> Size {
        constraints.constrain(Size {
            width: self.width,
            height: self.height,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct CanvasNode {
    pub(super) min_width: f32,
    pub(super) min_height: f32,
    pub(super) aspect_ratio: Option<f32>,
    pub(super) rect: Rectangle,
}

impl CanvasNode {
    pub(super) const fn new(min_width: f32, min_height: f32) -> Self {
        Self {
            min_width,
            min_height,
            aspect_ratio: None,
            rect: zero_rect(),
        }
    }

    pub(super) const fn with_aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = Some(aspect_ratio);
        self
    }

    fn measure(&self, constraints: LayoutConstraints) -> Size {
        let mut width = constraints.max_width.max(self.min_width);
        let mut height = constraints.max_height.max(self.min_height);
        if let Some(aspect_ratio) = self.aspect_ratio
            && aspect_ratio > f32::EPSILON
        {
            height = (width / aspect_ratio).min(height);
            width = height * aspect_ratio;
        }
        constraints.constrain(Size { width, height })
    }
}

#[derive(Clone, Debug)]
pub(super) struct SpacerNode {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) rect: Rectangle,
}

impl SpacerNode {
    pub(super) const fn sized(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            rect: zero_rect(),
        }
    }

    const fn measure(&self, constraints: LayoutConstraints) -> Size {
        constraints.constrain(Size {
            width: self.width,
            height: self.height,
        })
    }
}

#[derive(Clone, Debug)]
pub(super) struct StackNode {
    pub(super) axis: Axis,
    pub(super) gap: f32,
    pub(super) children: Vec<UiNode>,
    pub(super) rect: Rectangle,
}

impl StackNode {
    pub(super) const fn vertical(gap: f32, children: Vec<UiNode>) -> Self {
        Self {
            axis: Axis::Vertical,
            gap,
            children,
            rect: zero_rect(),
        }
    }

    pub(super) const fn horizontal(gap: f32, children: Vec<UiNode>) -> Self {
        Self {
            axis: Axis::Horizontal,
            gap,
            children,
            rect: zero_rect(),
        }
    }

    fn measure(&self, constraints: LayoutConstraints) -> Size {
        let mut main: f32 = 0.0;
        let mut cross: f32 = 0.0;
        for (index, child) in self.children.iter().enumerate() {
            let size = child.measure(LayoutConstraints::loose(
                constraints.max_width,
                constraints.max_height,
            ));
            if index > 0 {
                main += self.gap;
            }
            match self.axis {
                Axis::Horizontal => {
                    main += size.width;
                    cross = cross.max(size.height);
                }
                Axis::Vertical => {
                    main += size.height;
                    cross = cross.max(size.width);
                }
            }
        }
        let size = match self.axis {
            Axis::Horizontal => Size {
                width: main,
                height: cross,
            },
            Axis::Vertical => Size {
                width: cross,
                height: main,
            },
        };
        constraints.constrain(size)
    }

    fn layout(&mut self, rect: Rectangle) {
        self.rect = rect;
        let mut cursor = match self.axis {
            Axis::Horizontal => rect.x,
            Axis::Vertical => rect.y,
        };
        for child in &mut self.children {
            let size = child.measure(LayoutConstraints::loose(rect.width, rect.height));
            let child_rect = match self.axis {
                Axis::Horizontal => Rectangle {
                    x: cursor,
                    y: rect.y,
                    width: size.width,
                    height: rect.height,
                },
                Axis::Vertical => Rectangle {
                    x: rect.x,
                    y: cursor,
                    width: rect.width,
                    height: size.height,
                },
            };
            child.layout(child_rect);
            cursor += match self.axis {
                Axis::Horizontal => size.width + self.gap,
                Axis::Vertical => size.height + self.gap,
            };
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct GridNode {
    pub(super) min_column_width: f32,
    pub(super) gap: f32,
    pub(super) children: Vec<UiNode>,
    pub(super) rect: Rectangle,
    pub(super) columns: usize,
}

impl GridNode {
    pub(super) const fn responsive(min_column_width: f32, gap: f32, children: Vec<UiNode>) -> Self {
        Self {
            min_column_width,
            gap,
            children,
            rect: zero_rect(),
            columns: 1,
        }
    }

    fn column_count(&self, width: f32) -> usize {
        let span = (self.min_column_width + self.gap).max(1.0);
        ((width + self.gap) / span).floor().max(1.0) as usize
    }

    fn measure(&self, constraints: LayoutConstraints) -> Size {
        let columns = self.column_count(constraints.max_width);
        let rows = self.children.len().div_ceil(columns).max(1);
        let column_width = ((constraints.max_width
            - self.gap * (columns.saturating_sub(1) as f32))
            / columns as f32)
            .max(0.0);
        let mut row_heights = vec![0.0_f32; rows];
        for (index, child) in self.children.iter().enumerate() {
            let size = child.measure(LayoutConstraints::loose(
                column_width,
                constraints.max_height,
            ));
            row_heights[index / columns] = row_heights[index / columns].max(size.height);
        }
        let height =
            row_heights.into_iter().sum::<f32>() + self.gap * rows.saturating_sub(1) as f32;
        constraints.constrain(Size {
            width: constraints.max_width,
            height,
        })
    }

    fn layout(&mut self, rect: Rectangle) {
        self.rect = rect;
        self.columns = self.column_count(rect.width);
        let column_width = ((rect.width - self.gap * (self.columns.saturating_sub(1) as f32))
            / self.columns as f32)
            .max(0.0);
        let rows = self.children.len().div_ceil(self.columns).max(1);
        let mut row_heights = vec![0.0_f32; rows];
        for (index, child) in self.children.iter().enumerate() {
            let size = child.measure(LayoutConstraints::loose(column_width, rect.height));
            row_heights[index / self.columns] = row_heights[index / self.columns].max(size.height);
        }
        let mut row_y = rect.y;
        for (row, row_height) in row_heights.iter().copied().enumerate() {
            for column in 0..self.columns {
                let index = row * self.columns + column;
                let Some(child) = self.children.get_mut(index) else {
                    continue;
                };
                child.layout(Rectangle {
                    x: rect.x + column as f32 * (column_width + self.gap),
                    y: row_y,
                    width: column_width,
                    height: row_height,
                });
            }
            row_y += row_height + self.gap;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PanelNode {
    pub(super) padding: Insets,
    pub(super) kind: PanelKind,
    pub(super) child: Box<UiNode>,
    pub(super) rect: Rectangle,
}

impl PanelNode {
    pub(super) fn new(padding: Insets, child: UiNode) -> Self {
        Self::with_kind(padding, PanelKind::Overlay, child)
    }

    pub(super) fn with_kind(padding: Insets, kind: PanelKind, child: UiNode) -> Self {
        Self {
            padding,
            kind,
            child: Box::new(child),
            rect: zero_rect(),
        }
    }

    fn measure(&self, constraints: LayoutConstraints) -> Size {
        let horizontal = self.padding.left + self.padding.right;
        let vertical = self.padding.top + self.padding.bottom;
        let child = self.child.measure(LayoutConstraints::loose(
            (constraints.max_width - horizontal).max(0.0),
            (constraints.max_height - vertical).max(0.0),
        ));
        constraints.constrain(Size {
            width: child.width + horizontal,
            height: child.height + vertical,
        })
    }

    fn layout(&mut self, rect: Rectangle) {
        self.rect = rect;
        self.child.layout(inset(rect, self.padding));
    }
}

#[derive(Clone, Debug)]
pub(super) struct ScrollNode {
    pub(super) offset_y: f32,
    pub(super) child: Box<UiNode>,
    pub(super) content_height: f32,
    pub(super) rect: Rectangle,
}

impl ScrollNode {
    pub(super) fn vertical(offset_y: f32, child: UiNode) -> Self {
        Self {
            offset_y,
            child: Box::new(child),
            content_height: 0.0,
            rect: zero_rect(),
        }
    }

    fn layout(&mut self, rect: Rectangle) {
        self.rect = rect;
        let content = self
            .child
            .measure(LayoutConstraints::loose(rect.width, f32::MAX / 4.0));
        self.content_height = content.height;
        let max_offset = (self.content_height - rect.height).max(0.0);
        let offset_y = self.offset_y.clamp(0.0, max_offset);
        self.child.layout(Rectangle {
            x: rect.x,
            y: rect.y - offset_y,
            width: rect.width,
            height: content.height,
        });
    }
}

const fn zero_rect() -> Rectangle {
    Rectangle {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    }
}
