//! Flutter-inspired single-pass constraint layout engine.
//!
//! "Constraints go down, sizes go up, parent sets position."

use crate::geom::{Alignment, BoxConstraints, Offset, Rect, Size};
use crate::style::{BoxDecoration, ButtonStyle, Color, KeyCode, TextStyle};
use crate::widget::{
    Axis, CrossAxisAlignment, FlexChild, MainAxisAlignment, StackChild, TooltipSpec, Widget,
    WidgetId, WorldCanvas,
};

/// Text measurement abstraction so layout works with both `ab_glyph` and headless unit tests.
pub trait TextMeasurer {
    fn measure_text(&mut self, text: &str, font_size: f32) -> Size;
}

/// Deterministic monospace text measurer for headless layout tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct MonospaceTestMeasurer;

impl TextMeasurer for MonospaceTestMeasurer {
    fn measure_text(&mut self, text: &str, font_size: f32) -> Size {
        if text.is_empty() {
            return Size::new(0.0, font_size * 1.2);
        }
        let max_chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
        let lines = text.lines().count().max(1) as f32;
        Size::new(max_chars * font_size * 0.6, lines * font_size * 1.25)
    }
}

/// Resolved layout node with absolute screen-space `rect`.
#[derive(Debug, Clone)]
pub struct LayoutNode<M> {
    pub rect: Rect,
    pub kind: LayoutNodeKind<M>,
    pub children: Vec<LayoutNode<M>>,
}

/// Payload retained on a laid-out node for hit-testing, input, and draw-list emission.
#[derive(Debug, Clone)]
pub enum LayoutNodeKind<M> {
    Empty,
    Container {
        id: Option<WidgetId>,
        decoration: BoxDecoration,
        block_pointer: bool,
        tooltip: Option<TooltipSpec>,
    },
    Wrapper,
    Text {
        text: String,
        style: TextStyle,
    },
    ProgressBar {
        value: f32,
        fill_color: Color,
        bg_color: Color,
        border_color: Color,
        corner_radius: f32,
        label: Option<String>,
    },
    Button {
        id: WidgetId,
        label: String,
        sub_label: Option<String>,
        hotkey: Option<KeyCode>,
        enabled: bool,
        selected: bool,
        cooldown_progress: Option<f32>,
        on_press: Option<M>,
        tooltip: Option<TooltipSpec>,
        style: ButtonStyle,
    },
    WorldCanvas(WorldCanvas<M>),
}

struct TempLayoutNode<M> {
    size: Size,
    kind: LayoutNodeKind<M>,
    children: Vec<(Offset, TempLayoutNode<M>)>,
}

/// Runs Flutter-style layout on `widget` within `viewport` and resolves absolute screen rects.
pub fn layout_tree<M: Clone>(
    widget: &Widget<M>,
    viewport: Size,
    measurer: &mut dyn TextMeasurer,
) -> LayoutNode<M> {
    let root_constraints = BoxConstraints::tight(viewport);
    let temp = layout_widget(widget, root_constraints, measurer);
    resolve_absolute_positions(temp, Offset::ZERO)
}

fn resolve_absolute_positions<M>(node: TempLayoutNode<M>, origin: Offset) -> LayoutNode<M> {
    let rect = Rect::from_pos_size(origin, node.size);
    let children = node
        .children
        .into_iter()
        .map(|(rel_offset, child)| {
            let child_origin = Offset::new(origin.dx + rel_offset.dx, origin.dy + rel_offset.dy);
            resolve_absolute_positions(child, child_origin)
        })
        .collect();
    LayoutNode {
        rect,
        kind: node.kind,
        children,
    }
}

fn layout_widget<M: Clone>(
    widget: &Widget<M>,
    constraints: BoxConstraints,
    measurer: &mut dyn TextMeasurer,
) -> TempLayoutNode<M> {
    match widget {
        Widget::Empty => TempLayoutNode {
            size: constraints.constrain(Size::ZERO),
            kind: LayoutNodeKind::Empty,
            children: Vec::new(),
        },

        Widget::Container {
            id,
            padding,
            width,
            height,
            decoration,
            alignment,
            block_pointer,
            tooltip,
            child,
        } => {
            let mut inner_constraints = constraints;
            if let Some(w) = *width {
                let cw = w.clamp(constraints.min_width, constraints.max_width);
                inner_constraints.min_width = cw;
                inner_constraints.max_width = cw;
            }
            if let Some(h) = *height {
                let ch = h.clamp(constraints.min_height, constraints.max_height);
                inner_constraints.min_height = ch;
                inner_constraints.max_height = ch;
            }

            let deflated = inner_constraints.deflate(*padding);
            let child_constraints = if alignment.is_some() {
                deflated.loosen()
            } else {
                deflated
            };

            let mut children = Vec::new();
            let self_size = if let Some(child_widget) = child {
                let child_node = layout_widget(child_widget, child_constraints, measurer);
                let content_w = child_node.size.width + padding.horizontal();
                let content_h = child_node.size.height + padding.vertical();
                let desired = Size::new(
                    width.unwrap_or(if alignment.is_some() && inner_constraints.max_width.is_finite() {
                        inner_constraints.max_width
                    } else {
                        content_w
                    }),
                    height.unwrap_or(if alignment.is_some() && inner_constraints.max_height.is_finite() {
                        inner_constraints.max_height
                    } else {
                        content_h
                    }),
                );
                let final_size = inner_constraints.constrain(desired);
                let inner_avail = Size::new(
                    (final_size.width - padding.horizontal()).max(0.0),
                    (final_size.height - padding.vertical()).max(0.0),
                );
                let align_off = alignment
                    .unwrap_or(Alignment::TOP_LEFT)
                    .align_offset(inner_avail, child_node.size);
                let child_offset =
                    Offset::new(padding.left + align_off.dx, padding.top + align_off.dy);
                children.push((child_offset, child_node));
                final_size
            } else {
                let desired = Size::new(
                    width.unwrap_or(padding.horizontal()),
                    height.unwrap_or(padding.vertical()),
                );
                inner_constraints.constrain(desired)
            };

            TempLayoutNode {
                size: constraints.constrain(self_size),
                kind: LayoutNodeKind::Container {
                    id: *id,
                    decoration: *decoration,
                    block_pointer: *block_pointer,
                    tooltip: tooltip.clone(),
                },
                children,
            }
        }

        Widget::Padding { padding, child } => {
            let child_constraints = constraints.deflate(*padding);
            let child_node = layout_widget(child, child_constraints, measurer);
            let size = constraints.constrain(Size::new(
                child_node.size.width + padding.horizontal(),
                child_node.size.height + padding.vertical(),
            ));
            TempLayoutNode {
                size,
                kind: LayoutNodeKind::Wrapper,
                children: vec![(Offset::new(padding.left, padding.top), child_node)],
            }
        }

        Widget::SizedBox {
            width,
            height,
            child,
        } => {
            let mut tight = constraints;
            if let Some(w) = *width {
                let cw = w.clamp(constraints.min_width, constraints.max_width);
                tight.min_width = cw;
                tight.max_width = cw;
            }
            if let Some(h) = *height {
                let ch = h.clamp(constraints.min_height, constraints.max_height);
                tight.min_height = ch;
                tight.max_height = ch;
            }

            if let Some(child_widget) = child {
                let child_node = layout_widget(child_widget, tight, measurer);
                let size = tight.constrain(child_node.size);
                TempLayoutNode {
                    size,
                    kind: LayoutNodeKind::Wrapper,
                    children: vec![(Offset::ZERO, child_node)],
                }
            } else {
                let size = tight.constrain(Size::new(width.unwrap_or(0.0), height.unwrap_or(0.0)));
                TempLayoutNode {
                    size,
                    kind: LayoutNodeKind::Empty,
                    children: Vec::new(),
                }
            }
        }

        Widget::Align { alignment, child } => {
            let child_node = layout_widget(child, constraints.loosen(), measurer);
            let self_size = constraints.constrain(Size::new(
                if constraints.max_width.is_finite() {
                    constraints.max_width
                } else {
                    child_node.size.width
                },
                if constraints.max_height.is_finite() {
                    constraints.max_height
                } else {
                    child_node.size.height
                },
            ));
            let offset = alignment.align_offset(self_size, child_node.size);
            TempLayoutNode {
                size: self_size,
                kind: LayoutNodeKind::Wrapper,
                children: vec![(offset, child_node)],
            }
        }

        Widget::Flex {
            direction,
            main_axis,
            cross_axis,
            spacing,
            children,
        } => layout_flex(
            *direction,
            *main_axis,
            *cross_axis,
            *spacing,
            children,
            constraints,
            measurer,
        ),

        Widget::Stack { children } => layout_stack(children, constraints, measurer),

        Widget::Grid {
            columns,
            cell_width,
            cell_height,
            spacing,
            children,
        } => layout_grid(
            *columns,
            *cell_width,
            *cell_height,
            *spacing,
            children,
            constraints,
            measurer,
        ),

        Widget::Text { text, style } => {
            let measured = measurer.measure_text(text, style.font_size);
            TempLayoutNode {
                size: constraints.constrain(measured),
                kind: LayoutNodeKind::Text {
                    text: text.clone(),
                    style: *style,
                },
                children: Vec::new(),
            }
        }

        Widget::ProgressBar {
            value,
            fill_color,
            bg_color,
            border_color,
            width,
            height,
            corner_radius,
            label,
        } => {
            let desired_w = width.unwrap_or_else(|| {
                if constraints.max_width.is_finite() {
                    constraints.max_width
                } else {
                    120.0
                }
            });
            let size = constraints.constrain(Size::new(desired_w, *height));
            TempLayoutNode {
                size,
                kind: LayoutNodeKind::ProgressBar {
                    value: *value,
                    fill_color: *fill_color,
                    bg_color: *bg_color,
                    border_color: *border_color,
                    corner_radius: *corner_radius,
                    label: label.clone(),
                },
                children: Vec::new(),
            }
        }

        Widget::Button(btn) => {
            let label_sz = measurer.measure_text(&btn.label, 13.0);
            let sub_sz = btn
                .sub_label
                .as_ref()
                .map(|s| measurer.measure_text(s, 10.5))
                .unwrap_or(Size::ZERO);
            let desired = Size::new(
                label_sz.width.max(sub_sz.width) + 20.0,
                label_sz.height + if btn.sub_label.is_some() { sub_sz.height + 4.0 } else { 0.0 } + 16.0,
            );
            let size = constraints.constrain(desired);
            TempLayoutNode {
                size,
                kind: LayoutNodeKind::Button {
                    id: btn.id,
                    label: btn.label.clone(),
                    sub_label: btn.sub_label.clone(),
                    hotkey: btn.hotkey,
                    enabled: btn.enabled,
                    selected: btn.selected,
                    cooldown_progress: btn.cooldown_progress,
                    on_press: btn.on_press.clone(),
                    tooltip: btn.tooltip.clone(),
                    style: btn.style,
                },
                children: Vec::new(),
            }
        }

        Widget::WorldCanvas(canvas) => {
            let size = constraints.constrain(Size::new(
                if constraints.max_width.is_finite() {
                    constraints.max_width
                } else {
                    constraints.min_width
                },
                if constraints.max_height.is_finite() {
                    constraints.max_height
                } else {
                    constraints.min_height
                },
            ));
            TempLayoutNode {
                size,
                kind: LayoutNodeKind::WorldCanvas(canvas.clone()),
                children: Vec::new(),
            }
        }
    }
}

fn layout_flex<M: Clone>(
    direction: Axis,
    main_axis: MainAxisAlignment,
    cross_axis: CrossAxisAlignment,
    spacing: f32,
    children: &[FlexChild<M>],
    constraints: BoxConstraints,
    measurer: &mut dyn TextMeasurer,
) -> TempLayoutNode<M> {
    let (min_main, max_main, min_cross, max_cross) = match direction {
        Axis::Horizontal => (
            constraints.min_width,
            constraints.max_width,
            constraints.min_height,
            constraints.max_height,
        ),
        Axis::Vertical => (
            constraints.min_height,
            constraints.max_height,
            constraints.min_width,
            constraints.max_width,
        ),
    };

    let count = children.len();
    let total_spacing = if count > 1 {
        spacing * (count - 1) as f32
    } else {
        0.0
    };

    let mut laid_out: Vec<Option<TempLayoutNode<M>>> = (0..count).map(|_| None).collect();
    let mut fixed_main_sum = 0.0_f32;
    let mut total_flex = 0.0_f32;
    let mut max_child_cross = 0.0_f32;

    let (cross_min_for_child, cross_max_for_child) =
        if cross_axis == CrossAxisAlignment::Stretch && max_cross.is_finite() {
            (max_cross, max_cross)
        } else {
            (0.0, max_cross)
        };

    // Pass 1: lay out non-flex children.
    for (idx, item) in children.iter().enumerate() {
        if let Some(flex) = item.flex {
            total_flex += flex;
            continue;
        }
        let child_constraints = match direction {
            Axis::Horizontal => {
                BoxConstraints::new(0.0, f32::INFINITY, cross_min_for_child, cross_max_for_child)
            }
            Axis::Vertical => {
                BoxConstraints::new(cross_min_for_child, cross_max_for_child, 0.0, f32::INFINITY)
            }
        };
        let node = layout_widget(&item.widget, child_constraints, measurer);
        let (c_main, c_cross) = match direction {
            Axis::Horizontal => (node.size.width, node.size.height),
            Axis::Vertical => (node.size.height, node.size.width),
        };
        fixed_main_sum += c_main;
        max_child_cross = max_child_cross.max(c_cross);
        laid_out[idx] = Some(node);
    }

    // Pass 2: distribute remaining space across `Expanded` children.
    let remaining_main = if max_main.is_finite() {
        (max_main - fixed_main_sum - total_spacing).max(0.0)
    } else {
        0.0
    };

    let mut expanded_main_sum = 0.0_f32;
    for (idx, item) in children.iter().enumerate() {
        let Some(flex) = item.flex else {
            continue;
        };
        let share = if total_flex > 0.0 && max_main.is_finite() {
            remaining_main * (flex / total_flex)
        } else {
            0.0
        };
        let child_constraints = match direction {
            Axis::Horizontal => {
                if max_main.is_finite() {
                    BoxConstraints::new(share, share, cross_min_for_child, cross_max_for_child)
                } else {
                    BoxConstraints::new(0.0, f32::INFINITY, cross_min_for_child, cross_max_for_child)
                }
            }
            Axis::Vertical => {
                if max_main.is_finite() {
                    BoxConstraints::new(cross_min_for_child, cross_max_for_child, share, share)
                } else {
                    BoxConstraints::new(cross_min_for_child, cross_max_for_child, 0.0, f32::INFINITY)
                }
            }
        };
        let node = layout_widget(&item.widget, child_constraints, measurer);
        let (c_main, c_cross) = match direction {
            Axis::Horizontal => (node.size.width, node.size.height),
            Axis::Vertical => (node.size.height, node.size.width),
        };
        expanded_main_sum += c_main;
        max_child_cross = max_child_cross.max(c_cross);
        laid_out[idx] = Some(node);
    }

    let children_main_total = fixed_main_sum + expanded_main_sum;
    let content_main_total = children_main_total + total_spacing;

    let desired_main = if max_main.is_finite()
        && (total_flex > 0.0 || !matches!(main_axis, MainAxisAlignment::Start))
    {
        max_main
    } else {
        content_main_total
    };
    let final_main = desired_main.clamp(min_main, max_main);

    let desired_cross = if cross_axis == CrossAxisAlignment::Stretch && max_cross.is_finite() {
        max_cross
    } else {
        max_child_cross
    };
    let final_cross = desired_cross.clamp(min_cross, max_cross);

    let free_space = (final_main - content_main_total).max(0.0);
    let (mut cursor_main, extra_gap) = match main_axis {
        MainAxisAlignment::Start => (0.0, 0.0),
        MainAxisAlignment::Center => (free_space * 0.5, 0.0),
        MainAxisAlignment::End => (free_space, 0.0),
        MainAxisAlignment::SpaceBetween => {
            if count > 1 {
                (0.0, free_space / (count - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        MainAxisAlignment::SpaceAround => {
            if count > 0 {
                let slot = free_space / count as f32;
                (slot * 0.5, slot)
            } else {
                (0.0, 0.0)
            }
        }
    };

    let mut positioned_children = Vec::with_capacity(count);
    for node_opt in laid_out {
        let node = node_opt.expect("every flex child is laid out");
        let (c_main, c_cross) = match direction {
            Axis::Horizontal => (node.size.width, node.size.height),
            Axis::Vertical => (node.size.height, node.size.width),
        };
        let pos_cross = match cross_axis {
            CrossAxisAlignment::Start | CrossAxisAlignment::Stretch => 0.0,
            CrossAxisAlignment::Center => (final_cross - c_cross) * 0.5,
            CrossAxisAlignment::End => final_cross - c_cross,
        };
        let offset = match direction {
            Axis::Horizontal => Offset::new(cursor_main, pos_cross),
            Axis::Vertical => Offset::new(pos_cross, cursor_main),
        };
        positioned_children.push((offset, node));
        cursor_main += c_main + spacing + extra_gap;
    }

    let final_size = match direction {
        Axis::Horizontal => Size::new(final_main, final_cross),
        Axis::Vertical => Size::new(final_cross, final_main),
    };

    TempLayoutNode {
        size: constraints.constrain(final_size),
        kind: LayoutNodeKind::Wrapper,
        children: positioned_children,
    }
}

fn layout_stack<M: Clone>(
    children: &[StackChild<M>],
    constraints: BoxConstraints,
    measurer: &mut dyn TextMeasurer,
) -> TempLayoutNode<M> {
    let mut non_pos_nodes: Vec<(usize, TempLayoutNode<M>)> = Vec::new();
    let mut has_non_pos = false;
    let mut max_w = 0.0_f32;
    let mut max_h = 0.0_f32;

    let loose = constraints.loosen();
    for (idx, child) in children.iter().enumerate() {
        if let StackChild::NonPositioned(widget) = child {
            has_non_pos = true;
            let node = layout_widget(widget, loose, measurer);
            max_w = max_w.max(node.size.width);
            max_h = max_h.max(node.size.height);
            non_pos_nodes.push((idx, node));
        }
    }

    let stack_size = if has_non_pos {
        constraints.constrain(Size::new(max_w, max_h))
    } else {
        constraints.constrain(Size::new(
            if constraints.max_width.is_finite() {
                constraints.max_width
            } else {
                0.0
            },
            if constraints.max_height.is_finite() {
                constraints.max_height
            } else {
                0.0
            },
        ))
    };

    let mut ordered: Vec<Option<(Offset, TempLayoutNode<M>)>> =
        (0..children.len()).map(|_| None).collect();

    for (idx, node) in non_pos_nodes {
        ordered[idx] = Some((Offset::ZERO, node));
    }

    for (idx, child) in children.iter().enumerate() {
        if let StackChild::Positioned {
            left,
            top,
            right,
            bottom,
            width,
            height,
            child: inner,
        } = child
        {
            let (min_w, max_w) = match (*left, *right, *width) {
                (Some(l), Some(r), _) => {
                    let w = (stack_size.width - l - r).max(0.0);
                    (w, w)
                }
                (_, _, Some(w)) => (w.max(0.0), w.max(0.0)),
                _ => (0.0, stack_size.width),
            };
            let (min_h, max_h) = match (*top, *bottom, *height) {
                (Some(t), Some(b), _) => {
                    let h = (stack_size.height - t - b).max(0.0);
                    (h, h)
                }
                (_, _, Some(h)) => (h.max(0.0), h.max(0.0)),
                _ => (0.0, stack_size.height),
            };

            let child_constraints = BoxConstraints::new(min_w, max_w, min_h, max_h);
            let node = layout_widget(inner, child_constraints, measurer);

            let x = match (*left, *right) {
                (Some(l), _) => l,
                (None, Some(r)) => stack_size.width - r - node.size.width,
                (None, None) => 0.0,
            };
            let y = match (*top, *bottom) {
                (Some(t), _) => t,
                (None, Some(b)) => stack_size.height - b - node.size.height,
                (None, None) => 0.0,
            };

            ordered[idx] = Some((Offset::new(x, y), node));
        }
    }

    TempLayoutNode {
        size: stack_size,
        kind: LayoutNodeKind::Wrapper,
        children: ordered.into_iter().flatten().collect(),
    }
}

fn layout_grid<M: Clone>(
    columns: usize,
    cell_width: f32,
    cell_height: f32,
    spacing: f32,
    children: &[Widget<M>],
    constraints: BoxConstraints,
    measurer: &mut dyn TextMeasurer,
) -> TempLayoutNode<M> {
    let cols = columns.max(1);
    let rows = if children.is_empty() {
        0
    } else {
        children.len().div_ceil(cols)
    };

    let cell_constraints = BoxConstraints::tight(Size::new(cell_width, cell_height));
    let mut positioned = Vec::with_capacity(children.len());

    for (i, child) in children.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = col as f32 * (cell_width + spacing);
        let y = row as f32 * (cell_height + spacing);
        let node = layout_widget(child, cell_constraints, measurer);
        positioned.push((Offset::new(x, y), node));
    }

    let total_w = if children.is_empty() {
        0.0
    } else {
        let used_cols = children.len().min(cols);
        used_cols as f32 * cell_width + (used_cols.saturating_sub(1)) as f32 * spacing
    };
    let total_h = if rows == 0 {
        0.0
    } else {
        rows as f32 * cell_height + (rows.saturating_sub(1)) as f32 * spacing
    };

    TempLayoutNode {
        size: constraints.constrain(Size::new(total_w, total_h)),
        kind: LayoutNodeKind::Wrapper,
        children: positioned,
    }
}
