//! Retained runtime state, hit-testing, input pass-through, and layered DrawList builder.

use crate::geom::{Rect, Size, Vec2};
use crate::layout::{LayoutNode, LayoutNodeKind, TextMeasurer};
use crate::style::{Color, KeyCode};
use crate::widget::{TooltipSpec, WidgetId, WorldCanvas};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// GPU-ready rounded rectangle / circle primitive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadPrimitive {
    pub rect: Rect,
    pub bg_color: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub corner_radius: f32,
}

/// GPU-ready text run primitive.
#[derive(Debug, Clone, PartialEq)]
pub struct TextPrimitive {
    pub pos: Vec2,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
}

/// Three-layer draw list rendered in order: world -> hud -> overlay.
#[derive(Debug, Default, Clone)]
pub struct DrawList {
    pub world_quads: Vec<QuadPrimitive>,
    pub world_texts: Vec<TextPrimitive>,
    pub hud_quads: Vec<QuadPrimitive>,
    pub hud_texts: Vec<TextPrimitive>,
    pub overlay_quads: Vec<QuadPrimitive>,
    pub overlay_texts: Vec<TextPrimitive>,
}

/// Retained UI interaction state across declarative TEA rebuilds.
#[derive(Debug, Default, Clone)]
pub struct UiRuntime {
    pub cursor_pos: Vec2,
    pub hovered_id: Option<WidgetId>,
    pub pressed_id: Option<WidgetId>,
    pub pointer_captured_by_hud: bool,
    pub world_drag_start: Option<Vec2>,
    pub world_drag_current: Option<Vec2>,
    pub shift_held: bool,
}

struct HitResult<'a, M> {
    hud_blocked: bool,
    hovered_id: Option<WidgetId>,
    button_node: Option<&'a LayoutNode<M>>,
    tooltip: Option<(&'a TooltipSpec, Rect)>,
}

impl UiRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when the HUD owns the pointer (hovering a blocking HUD panel/button or pressing one).
    pub fn ui_wants_pointer(&self) -> bool {
        self.pointer_captured_by_hud || self.pressed_id.is_some()
    }

    /// Returns the active battlefield marquee selection rectangle if dragging beyond the 4px threshold.
    pub fn active_selection_box(&self) -> Option<Rect> {
        let start = self.world_drag_start?;
        let current = self.world_drag_current?;
        if start.distance(current) > 4.0 {
            Some(Rect::from_points(start, current))
        } else {
            None
        }
    }

    pub fn set_shift_held(&mut self, shift: bool) {
        self.shift_held = shift;
    }

    pub fn handle_cursor_moved<M>(&mut self, pos: Vec2, layout_root: &LayoutNode<M>) {
        self.cursor_pos = pos;
        let hit = hit_test_hud(layout_root, pos);
        self.pointer_captured_by_hud = hit.hud_blocked;
        self.hovered_id = hit.hovered_id;
        if self.world_drag_start.is_some() {
            self.world_drag_current = Some(pos);
        }
    }

    pub fn handle_mouse_button<M: Clone>(
        &mut self,
        button: MouseButton,
        pressed: bool,
        layout_root: &LayoutNode<M>,
    ) -> Vec<M> {
        let mut messages = Vec::new();
        let hit = hit_test_hud(layout_root, self.cursor_pos);
        self.pointer_captured_by_hud = hit.hud_blocked;
        self.hovered_id = hit.hovered_id;

        match (button, pressed) {
            (MouseButton::Left, true) => {
                if hit.hud_blocked {
                    if let Some(btn_node) = hit.button_node {
                        if let LayoutNodeKind::Button {
                            id,
                            enabled,
                            cooldown_progress,
                            ..
                        } = &btn_node.kind
                        {
                            if *enabled && cooldown_progress.is_none() {
                                self.pressed_id = Some(*id);
                            }
                        }
                    }
                } else {
                    self.world_drag_start = Some(self.cursor_pos);
                    self.world_drag_current = Some(self.cursor_pos);
                }
            }
            (MouseButton::Left, false) => {
                if let Some(pid) = self.pressed_id.take() {
                    if let Some(btn_node) = hit.button_node {
                        if let LayoutNodeKind::Button {
                            id,
                            enabled,
                            cooldown_progress,
                            on_press,
                            ..
                        } = &btn_node.kind
                        {
                            if *id == pid && *enabled && cooldown_progress.is_none() {
                                if let Some(msg) = on_press {
                                    messages.push(msg.clone());
                                }
                            }
                        }
                    }
                } else if let Some(start) = self.world_drag_start.take() {
                    self.world_drag_current = None;
                    if let Some(canvas) = find_world_canvas(layout_root) {
                        if start.distance(self.cursor_pos) > 4.0 {
                            let sel_rect = Rect::from_points(start, self.cursor_pos);
                            if let Some(cb) = &canvas.on_box_select {
                                messages.push(cb(sel_rect, self.shift_held));
                            }
                        } else if let Some(cb) = &canvas.on_select_unit {
                            let hit_unit = canvas
                                .units
                                .iter()
                                .rev()
                                .find(|u| u.pos.distance(self.cursor_pos) <= u.radius + 6.0)
                                .map(|u| u.id);
                            messages.push(cb(hit_unit, self.shift_held));
                        }
                    }
                }
            }
            (MouseButton::Right, true) if !hit.hud_blocked => {
                if let Some(canvas) = find_world_canvas(layout_root) {
                    if let Some(cb) = &canvas.on_right_click {
                        messages.push(cb(self.cursor_pos));
                    }
                }
            }
            _ => {}
        }

        messages
    }

    pub fn handle_key_press<M: Clone>(
        &self,
        key: KeyCode,
        layout_root: &LayoutNode<M>,
    ) -> Vec<M> {
        let mut messages = Vec::new();
        collect_hotkey_messages(layout_root, key, &mut messages);
        messages
    }

    pub fn build_draw_list<M>(
        &self,
        layout_root: &LayoutNode<M>,
        viewport: Size,
        measurer: &mut dyn TextMeasurer,
    ) -> DrawList {
        let mut list = DrawList::default();
        emit_layout_node(self, layout_root, measurer, &mut list);

        // Render active marquee selection box in the world layer (above units, below HUD).
        if let Some(sel_rect) = self.active_selection_box() {
            list.world_quads.push(QuadPrimitive {
                rect: sel_rect,
                bg_color: Color::rgba(0.22, 0.85, 0.55, 0.16),
                border_color: Color::rgba(0.32, 0.96, 0.62, 0.92),
                border_width: 1.5,
                corner_radius: 2.0,
            });
        }

        // Render floating tooltip in the unclipped overlay layer (Layer 2).
        let hit = hit_test_hud(layout_root, self.cursor_pos);
        if let Some((tooltip, anchor_rect)) = hit.tooltip {
            emit_tooltip_overlay(tooltip, anchor_rect, self.cursor_pos, viewport, measurer, &mut list);
        }

        list
    }
}

fn hit_test_hud<'a, M>(node: &'a LayoutNode<M>, pos: Vec2) -> HitResult<'a, M> {
    if !node.rect.contains(pos) {
        return HitResult {
            hud_blocked: false,
            hovered_id: None,
            button_node: None,
            tooltip: None,
        };
    }

    // Traverse children in reverse paint order (topmost first).
    for child in node.children.iter().rev() {
        let child_hit = hit_test_hud(child, pos);
        if child_hit.hud_blocked || child_hit.button_node.is_some() || child_hit.tooltip.is_some()
        {
            return child_hit;
        }
    }

    match &node.kind {
        LayoutNodeKind::Button { id, tooltip, .. } => HitResult {
            hud_blocked: true,
            hovered_id: Some(*id),
            button_node: Some(node),
            tooltip: tooltip.as_ref().map(|t| (t, node.rect)),
        },
        LayoutNodeKind::Container {
            id,
            block_pointer,
            tooltip,
            ..
        } => {
            if *block_pointer || tooltip.is_some() {
                HitResult {
                    hud_blocked: *block_pointer,
                    hovered_id: *id,
                    button_node: None,
                    tooltip: tooltip.as_ref().map(|t| (t, node.rect)),
                }
            } else {
                HitResult {
                    hud_blocked: false,
                    hovered_id: None,
                    button_node: None,
                    tooltip: None,
                }
            }
        }
        _ => HitResult {
            hud_blocked: false,
            hovered_id: None,
            button_node: None,
            tooltip: None,
        },
    }
}

fn find_world_canvas<M>(node: &LayoutNode<M>) -> Option<&WorldCanvas<M>> {
    if let LayoutNodeKind::WorldCanvas(canvas) = &node.kind {
        return Some(canvas);
    }
    for child in &node.children {
        if let Some(found) = find_world_canvas(child) {
            return Some(found);
        }
    }
    None
}

fn collect_hotkey_messages<M: Clone>(node: &LayoutNode<M>, key: KeyCode, out: &mut Vec<M>) {
    if let LayoutNodeKind::Button {
        hotkey,
        enabled,
        cooldown_progress,
        on_press,
        ..
    } = &node.kind
    {
        if *hotkey == Some(key) && *enabled && cooldown_progress.is_none() {
            if let Some(msg) = on_press {
                out.push(msg.clone());
                return;
            }
        }
    }
    for child in &node.children {
        collect_hotkey_messages(child, key, out);
    }
}

fn emit_layout_node<M>(
    runtime: &UiRuntime,
    node: &LayoutNode<M>,
    measurer: &mut dyn TextMeasurer,
    list: &mut DrawList,
) {
    match &node.kind {
        LayoutNodeKind::Empty | LayoutNodeKind::Wrapper => {}
        LayoutNodeKind::WorldCanvas(canvas) => {
            emit_world_canvas(canvas, node.rect, measurer, list);
        }
        LayoutNodeKind::Container { decoration, .. } => {
            if decoration.bg_color.a > 0.0 || decoration.border_width > 0.0 {
                list.hud_quads.push(QuadPrimitive {
                    rect: node.rect,
                    bg_color: decoration.bg_color,
                    border_color: decoration.border_color,
                    border_width: decoration.border_width,
                    corner_radius: decoration.corner_radius,
                });
            }
        }
        LayoutNodeKind::Text { text, style } => {
            if !text.is_empty() {
                list.hud_texts.push(TextPrimitive {
                    pos: Vec2::new(node.rect.x, node.rect.y),
                    text: text.clone(),
                    font_size: style.font_size,
                    color: style.color,
                });
            }
        }
        LayoutNodeKind::ProgressBar {
            value,
            fill_color,
            bg_color,
            border_color,
            corner_radius,
            label,
        } => {
            list.hud_quads.push(QuadPrimitive {
                rect: node.rect,
                bg_color: *bg_color,
                border_color: *border_color,
                border_width: 1.0,
                corner_radius: *corner_radius,
            });
            let clamped = value.clamp(0.0, 1.0);
            if clamped > 0.005 && node.rect.width > 2.0 {
                let inner_w = ((node.rect.width - 2.0) * clamped).max(2.0);
                let inner_h = (node.rect.height - 2.0).max(1.0);
                list.hud_quads.push(QuadPrimitive {
                    rect: Rect::new(node.rect.x + 1.0, node.rect.y + 1.0, inner_w, inner_h),
                    bg_color: *fill_color,
                    border_color: Color::TRANSPARENT,
                    border_width: 0.0,
                    corner_radius: (*corner_radius - 1.0).max(0.0),
                });
            }
            if let Some(txt) = label {
                let sz = measurer.measure_text(txt, 11.0);
                list.hud_texts.push(TextPrimitive {
                    pos: Vec2::new(
                        node.rect.x + (node.rect.width - sz.width) * 0.5,
                        node.rect.y + (node.rect.height - sz.height) * 0.5,
                    ),
                    text: txt.clone(),
                    font_size: 11.0,
                    color: Color::WHITE,
                });
            }
        }
        LayoutNodeKind::Button {
            id,
            label,
            sub_label,
            hotkey,
            enabled,
            selected,
            cooldown_progress,
            style,
            ..
        } => {
            let hovered = runtime.hovered_id == Some(*id);
            let pressed = runtime.pressed_id == Some(*id);

            let bg_color = if !*enabled {
                style.disabled_bg
            } else if pressed && hovered {
                style.pressed_bg
            } else if *selected {
                style.selected_bg
            } else if hovered {
                style.hover_bg
            } else {
                style.normal_bg
            };

            let border_color = if *selected {
                style.selected_border_color
            } else if hovered && *enabled {
                style.hover_border_color
            } else {
                style.border_color
            };

            list.hud_quads.push(QuadPrimitive {
                rect: node.rect,
                bg_color,
                border_color,
                border_width: style.border_width,
                corner_radius: style.corner_radius,
            });

            // Cooldown sweep overlay (bottom-up translucent curtain).
            if let Some(cd) = cooldown_progress {
                let frac = cd.clamp(0.0, 1.0);
                if frac > 0.0 {
                    let overlay_h = (node.rect.height - 2.0) * frac;
                    let overlay_y = node.rect.y + node.rect.height - 1.0 - overlay_h;
                    list.hud_quads.push(QuadPrimitive {
                        rect: Rect::new(
                            node.rect.x + 1.0,
                            overlay_y,
                            (node.rect.width - 2.0).max(0.0),
                            overlay_h,
                        ),
                        bg_color: Color::rgba(0.02, 0.04, 0.08, 0.68),
                        border_color: Color::ACCENT_CYAN.with_alpha(0.7),
                        border_width: 0.75,
                        corner_radius: 2.0,
                    });
                }
            }

            let text_color = if *enabled {
                style.text_color
            } else {
                style.disabled_text_color
            };

            if let Some(hk) = hotkey {
                let hk_text = hk.label();
                let badge_w = 18.0_f32.max(hk_text.len() as f32 * 8.0 + 6.0);
                list.hud_quads.push(QuadPrimitive {
                    rect: Rect::new(node.rect.x + 4.0, node.rect.y + 4.0, badge_w, 15.0),
                    bg_color: Color::rgba(0.02, 0.03, 0.06, 0.85),
                    border_color: border_color.with_alpha(0.6),
                    border_width: 1.0,
                    corner_radius: 3.0,
                });
                let hk_sz = measurer.measure_text(hk_text, 10.0);
                list.hud_texts.push(TextPrimitive {
                    pos: Vec2::new(
                        node.rect.x + 4.0 + (badge_w - hk_sz.width) * 0.5,
                        node.rect.y + 4.0 + (15.0 - hk_sz.height) * 0.5,
                    ),
                    text: hk_text.to_string(),
                    font_size: 10.0,
                    color: if *enabled {
                        Color::ACCENT_GOLD
                    } else {
                        style.disabled_text_color
                    },
                });
            }

            let label_sz = measurer.measure_text(label, 12.5);
            let has_sub = sub_label.is_some();
            let top_offset = if hotkey.is_some() { 10.0 } else { 0.0 };
            let label_y = if has_sub {
                node.rect.y + top_offset + (node.rect.height - top_offset - label_sz.height) * 0.38
            } else {
                node.rect.y + top_offset + (node.rect.height - top_offset - label_sz.height) * 0.5
            };
            list.hud_texts.push(TextPrimitive {
                pos: Vec2::new(
                    node.rect.x + (node.rect.width - label_sz.width) * 0.5,
                    label_y,
                ),
                text: label.clone(),
                font_size: 12.5,
                color: text_color,
            });

            if let Some(sub) = sub_label {
                let sub_sz = measurer.measure_text(sub, 10.0);
                list.hud_texts.push(TextPrimitive {
                    pos: Vec2::new(
                        node.rect.x + (node.rect.width - sub_sz.width) * 0.5,
                        node.rect.y + node.rect.height - sub_sz.height - 5.0,
                    ),
                    text: sub.clone(),
                    font_size: 10.0,
                    color: if *enabled {
                        Color::ACCENT_CYAN
                    } else {
                        style.disabled_text_color
                    },
                });
            }
        }
    }

    for child in &node.children {
        emit_layout_node(runtime, child, measurer, list);
    }
}

fn emit_world_canvas<M>(
    canvas: &WorldCanvas<M>,
    bounds: Rect,
    measurer: &mut dyn TextMeasurer,
    list: &mut DrawList,
) {
    // Subtle tactical battlefield grid lines.
    let grid_step = 80.0_f32;
    let mut x = bounds.x + grid_step;
    while x < bounds.x + bounds.width {
        list.world_quads.push(QuadPrimitive {
            rect: Rect::new(x, bounds.y, 1.0, bounds.height),
            bg_color: Color::rgba(0.16, 0.24, 0.34, 0.16),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radius: 0.0,
        });
        x += grid_step;
    }
    let mut y = bounds.y + grid_step;
    while y < bounds.y + bounds.height {
        list.world_quads.push(QuadPrimitive {
            rect: Rect::new(bounds.x, y, bounds.width, 1.0),
            bg_color: Color::rgba(0.16, 0.24, 0.34, 0.16),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radius: 0.0,
        });
        y += grid_step;
    }

    // Order markers (right-click move waypoints).
    for marker in &canvas.order_markers {
        let r = 8.0 + (1.0 - marker.ttl) * 16.0;
        list.world_quads.push(QuadPrimitive {
            rect: Rect::new(marker.pos.x - r, marker.pos.y - r, r * 2.0, r * 2.0),
            bg_color: Color::TRANSPARENT,
            border_color: Color::HEALTH_GREEN.with_alpha(marker.ttl),
            border_width: 2.0,
            corner_radius: r,
        });
    }

    // Units & Buildings.
    for unit in &canvas.units {
        if unit.selected {
            let ring_r = unit.radius + 7.0;
            list.world_quads.push(QuadPrimitive {
                rect: Rect::new(
                    unit.pos.x - ring_r,
                    unit.pos.y - ring_r,
                    ring_r * 2.0,
                    ring_r * 2.0,
                ),
                bg_color: Color::HEALTH_GREEN.with_alpha(0.12),
                border_color: Color::HEALTH_GREEN,
                border_width: 2.0,
                corner_radius: if unit.is_building { 10.0 } else { ring_r },
            });
        }

        let body_corner = if unit.is_building { 6.0 } else { unit.radius };
        list.world_quads.push(QuadPrimitive {
            rect: Rect::new(
                unit.pos.x - unit.radius,
                unit.pos.y - unit.radius,
                unit.radius * 2.0,
                unit.radius * 2.0,
            ),
            bg_color: unit.color,
            border_color: Color::rgba(0.9, 0.95, 1.0, 0.65),
            border_width: 1.5,
            corner_radius: body_corner,
        });

        // Health bar floating above unit.
        let bar_w = (unit.radius * 2.2).max(34.0);
        let bar_h = 5.0;
        let bar_x = unit.pos.x - bar_w * 0.5;
        let mut bar_y = unit.pos.y - unit.radius - 14.0;

        let hp_frac = if unit.max_hp > 0.0 {
            (unit.hp / unit.max_hp).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let hp_color = if hp_frac > 0.55 {
            Color::HEALTH_GREEN
        } else if hp_frac > 0.25 {
            Color::HEALTH_YELLOW
        } else {
            Color::HEALTH_RED
        };

        list.world_quads.push(QuadPrimitive {
            rect: Rect::new(bar_x, bar_y, bar_w, bar_h),
            bg_color: Color::rgba(0.03, 0.04, 0.06, 0.88),
            border_color: Color::rgba(0.15, 0.20, 0.25, 0.9),
            border_width: 1.0,
            corner_radius: 1.5,
        });
        if hp_frac > 0.0 {
            list.world_quads.push(QuadPrimitive {
                rect: Rect::new(
                    bar_x + 1.0,
                    bar_y + 1.0,
                    ((bar_w - 2.0) * hp_frac).max(1.0),
                    bar_h - 2.0,
                ),
                bg_color: hp_color,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                corner_radius: 1.0,
            });
        }

        if let Some((sh, max_sh)) = unit.shield {
            bar_y -= 5.0;
            let sh_frac = if max_sh > 0.0 {
                (sh / max_sh).clamp(0.0, 1.0)
            } else {
                0.0
            };
            list.world_quads.push(QuadPrimitive {
                rect: Rect::new(bar_x, bar_y, bar_w, 4.0),
                bg_color: Color::rgba(0.03, 0.04, 0.06, 0.85),
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                corner_radius: 1.0,
            });
            if sh_frac > 0.0 {
                list.world_quads.push(QuadPrimitive {
                    rect: Rect::new(bar_x, bar_y, bar_w * sh_frac, 4.0),
                    bg_color: Color::SHIELD_BLUE,
                    border_color: Color::TRANSPARENT,
                    border_width: 0.0,
                    corner_radius: 1.0,
                });
            }
        }

        if let Some(prod) = unit.production_progress {
            let prod_y = unit.pos.y + unit.radius + 6.0;
            list.world_quads.push(QuadPrimitive {
                rect: Rect::new(bar_x, prod_y, bar_w, 4.0),
                bg_color: Color::rgba(0.04, 0.05, 0.08, 0.85),
                border_color: Color::ACCENT_GOLD.with_alpha(0.6),
                border_width: 1.0,
                corner_radius: 1.5,
            });
            list.world_quads.push(QuadPrimitive {
                rect: Rect::new(bar_x + 1.0, prod_y + 1.0, (bar_w - 2.0) * prod.clamp(0.0, 1.0), 2.0),
                bg_color: Color::ACCENT_GOLD,
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
                corner_radius: 1.0,
            });
        }

        let lbl_sz = measurer.measure_text(&unit.label, 11.0);
        list.world_texts.push(TextPrimitive {
            pos: Vec2::new(
                unit.pos.x - lbl_sz.width * 0.5,
                unit.pos.y + unit.radius + if unit.production_progress.is_some() { 13.0 } else { 5.0 },
            ),
            text: unit.label.clone(),
            font_size: 11.0,
            color: Color::rgba(0.86, 0.92, 0.98, 0.88),
        });
    }
}

fn emit_tooltip_overlay(
    tooltip: &TooltipSpec,
    anchor_rect: Rect,
    cursor: Vec2,
    viewport: Size,
    measurer: &mut dyn TextMeasurer,
    list: &mut DrawList,
) {
    let pad = 10.0;
    let card_w = 270.0_f32.min((viewport.width - 16.0).max(120.0));
    let inner_w = card_w - pad * 2.0;

    let title_sz = measurer.measure_text(&tooltip.title, 14.0);
    let wrapped_desc = wrap_text(&tooltip.description, inner_w, 12.0, measurer);
    let has_costs = tooltip.mineral_cost.is_some()
        || tooltip.gas_cost.is_some()
        || tooltip.cooldown_secs.is_some();

    let desc_line_h = 16.0;
    let card_h = pad * 2.0
        + title_sz.height
        + if has_costs { 22.0 } else { 4.0 }
        + wrapped_desc.len() as f32 * desc_line_h
        + 4.0;

    // Default to top-left above the hovered widget/cursor when near bottom-right HUD,
    // then clamp strictly inside the viewport bounds.
    let mut x = cursor.x + 16.0;
    if x + card_w > viewport.width - 8.0 {
        x = anchor_rect.x - card_w - 8.0;
    }
    let mut y = anchor_rect.y - card_h - 8.0;
    if y < 8.0 {
        y = cursor.y + 20.0;
    }
    x = x.clamp(8.0, (viewport.width - card_w - 8.0).max(8.0));
    y = y.clamp(8.0, (viewport.height - card_h - 8.0).max(8.0));

    let card_rect = Rect::new(x, y, card_w, card_h);
    list.overlay_quads.push(QuadPrimitive {
        rect: card_rect,
        bg_color: Color::rgba(0.035, 0.050, 0.080, 0.97),
        border_color: Color::ACCENT_CYAN,
        border_width: 1.5,
        corner_radius: 6.0,
    });

    // Title
    list.overlay_texts.push(TextPrimitive {
        pos: Vec2::new(x + pad, y + pad),
        text: tooltip.title.clone(),
        font_size: 14.0,
        color: Color::ACCENT_GOLD,
    });

    // Hotkey badge on top-right of tooltip
    if let Some(hk) = &tooltip.hotkey {
        let hk_str = format!("[{hk}]");
        let hk_sz = measurer.measure_text(&hk_str, 12.0);
        list.overlay_texts.push(TextPrimitive {
            pos: Vec2::new(x + card_w - pad - hk_sz.width, y + pad + 1.0),
            text: hk_str,
            font_size: 12.0,
            color: Color::ACCENT_CYAN,
        });
    }

    let mut cur_y = y + pad + title_sz.height + 5.0;

    if has_costs {
        let mut cost_x = x + pad;
        if let Some((minerals, ok)) = tooltip.mineral_cost {
            let s = format!("MIN: {minerals}");
            let sz = measurer.measure_text(&s, 11.5);
            list.overlay_texts.push(TextPrimitive {
                pos: Vec2::new(cost_x, cur_y),
                text: s,
                font_size: 11.5,
                color: if ok {
                    Color::MINERAL_BLUE
                } else {
                    Color::DANGER_RED
                },
            });
            cost_x += sz.width + 12.0;
        }
        if let Some((gas, ok)) = tooltip.gas_cost {
            let s = format!("GAS: {gas}");
            let sz = measurer.measure_text(&s, 11.5);
            list.overlay_texts.push(TextPrimitive {
                pos: Vec2::new(cost_x, cur_y),
                text: s,
                font_size: 11.5,
                color: if ok {
                    Color::GAS_GREEN
                } else {
                    Color::DANGER_RED
                },
            });
            cost_x += sz.width + 12.0;
        }
        if let Some(cd) = tooltip.cooldown_secs {
            let s = format!("CD: {cd:.1}s");
            list.overlay_texts.push(TextPrimitive {
                pos: Vec2::new(cost_x, cur_y),
                text: s,
                font_size: 11.5,
                color: Color::rgba(0.80, 0.84, 0.90, 0.95),
            });
        }
        cur_y += 18.0;
    }

    for line in wrapped_desc {
        list.overlay_texts.push(TextPrimitive {
            pos: Vec2::new(x + pad, cur_y),
            text: line,
            font_size: 12.0,
            color: Color::rgba(0.86, 0.90, 0.96, 0.95),
        });
        cur_y += desc_line_h;
    }
}

fn wrap_text(
    text: &str,
    max_width: f32,
    font_size: f32,
    measurer: &mut dyn TextMeasurer,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if measurer.measure_text(&candidate, font_size).width <= max_width || current.is_empty()
            {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
