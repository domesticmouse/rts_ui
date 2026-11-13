//! Declarative Flutter-inspired widgets and The Elm Architecture `Application` trait.

use std::sync::Arc;

use crate::geom::{Alignment, EdgeInsets, Rect, Vec2};
use crate::style::{BoxDecoration, ButtonStyle, Color, KeyCode, TextStyle};

/// The Elm Architecture (TEA) trait implemented by the game or application.
pub trait Application {
    type Message: Clone + Send + 'static;

    fn update(&mut self, message: Self::Message);
    fn view(&self) -> Widget<Self::Message>;
}

/// Stable identifier used to track hover/press state across declarative rebuilds.
pub type WidgetId = u64;

/// Deterministic FNV-1a hash helper for creating stable `WidgetId`s from string keys.
pub fn widget_id(name: &str) -> WidgetId {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisAlignment {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    Center,
    End,
    Stretch,
}

/// Rich floating tooltip metadata rendered in the unclipped overlay layer.
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipSpec {
    pub title: String,
    pub hotkey: Option<String>,
    /// `(amount, can_afford)`
    pub mineral_cost: Option<(u32, bool)>,
    /// `(amount, can_afford)`
    pub gas_cost: Option<(u32, bool)>,
    pub cooldown_secs: Option<f32>,
    pub description: String,
}

impl TooltipSpec {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            hotkey: None,
            mineral_cost: None,
            gas_cost: None,
            cooldown_secs: None,
            description: description.into(),
        }
    }

    pub fn with_hotkey(mut self, hotkey: impl Into<String>) -> Self {
        self.hotkey = Some(hotkey.into());
        self
    }

    pub fn with_minerals(mut self, amount: u32, can_afford: bool) -> Self {
        self.mineral_cost = Some((amount, can_afford));
        self
    }

    pub fn with_gas(mut self, amount: u32, can_afford: bool) -> Self {
        self.gas_cost = Some((amount, can_afford));
        self
    }

    pub fn with_cooldown(mut self, secs: f32) -> Self {
        self.cooldown_secs = Some(secs);
        self
    }
}

/// World-space unit or building descriptor projected onto the battlefield canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldUnitOverlay {
    pub id: u32,
    pub pos: Vec2,
    pub radius: f32,
    pub label: String,
    pub hp: f32,
    pub max_hp: f32,
    pub shield: Option<(f32, f32)>,
    pub production_progress: Option<f32>,
    pub selected: bool,
    pub color: Color,
    pub is_building: bool,
}

/// Transient move/attack order ring marker displayed on the battlefield.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldOrderMarker {
    pub pos: Vec2,
    /// Normalized remaining lifetime `0.0..=1.0`.
    pub ttl: f32,
}

/// Callback emitted when left-clicking a unit or empty ground on `WorldCanvas`.
pub type SelectUnitCallback<M> = Arc<dyn Fn(Option<u32>, bool) -> M + Send + Sync>;
/// Callback emitted when finishing a marquee drag-box selection on `WorldCanvas`.
pub type BoxSelectCallback<M> = Arc<dyn Fn(Rect, bool) -> M + Send + Sync>;
/// Callback emitted when right-clicking the battlefield on `WorldCanvas`.
pub type RightClickCallback<M> = Arc<dyn Fn(Vec2) -> M + Send + Sync>;

/// Interactive battlefield layer positioned beneath HUD chrome.
#[derive(Clone)]
pub struct WorldCanvas<M> {
    pub units: Vec<WorldUnitOverlay>,
    pub order_markers: Vec<WorldOrderMarker>,
    pub on_select_unit: Option<SelectUnitCallback<M>>,
    pub on_box_select: Option<BoxSelectCallback<M>>,
    pub on_right_click: Option<RightClickCallback<M>>,
}

impl<M> Default for WorldCanvas<M> {
    fn default() -> Self {
        Self {
            units: Vec::new(),
            order_markers: Vec::new(),
            on_select_unit: None,
            on_box_select: None,
            on_right_click: None,
        }
    }
}

impl<M> std::fmt::Debug for WorldCanvas<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldCanvas")
            .field("units", &self.units.len())
            .field("order_markers", &self.order_markers.len())
            .finish()
    }
}

/// Child wrapper for `Flex` (`Row` / `Column`) supporting fixed or `Expanded` sizing.
#[derive(Debug, Clone)]
pub struct FlexChild<M> {
    pub flex: Option<f32>,
    pub widget: Widget<M>,
}

impl<M> FlexChild<M> {
    pub fn fixed(widget: Widget<M>) -> Self {
        Self { flex: None, widget }
    }

    pub fn expanded(flex: f32, widget: Widget<M>) -> Self {
        Self {
            flex: Some(flex.max(0.0)),
            widget,
        }
    }
}

/// Child wrapper for `Stack`, matching Flutter's `Positioned` vs non-positioned children.
#[derive(Debug, Clone)]
pub enum StackChild<M> {
    NonPositioned(Widget<M>),
    Positioned {
        left: Option<f32>,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
        width: Option<f32>,
        height: Option<f32>,
        child: Widget<M>,
    },
}

/// Interactive button widget with hotkey, cooldown progress, and optional tooltip.
#[derive(Debug, Clone)]
pub struct ButtonWidget<M> {
    pub id: WidgetId,
    pub label: String,
    pub sub_label: Option<String>,
    pub hotkey: Option<KeyCode>,
    pub enabled: bool,
    pub selected: bool,
    /// `Some(remaining_fraction)` in `0.0..=1.0` where `1.0` is full cooldown remaining.
    pub cooldown_progress: Option<f32>,
    pub on_press: Option<M>,
    pub tooltip: Option<TooltipSpec>,
    pub style: ButtonStyle,
}

impl<M> ButtonWidget<M> {
    pub fn new(id_key: &str, label: impl Into<String>) -> Self {
        Self {
            id: widget_id(id_key),
            label: label.into(),
            sub_label: None,
            hotkey: None,
            enabled: true,
            selected: false,
            cooldown_progress: None,
            on_press: None,
            tooltip: None,
            style: ButtonStyle::default(),
        }
    }

    pub fn with_sub_label(mut self, sub: impl Into<String>) -> Self {
        self.sub_label = Some(sub.into());
        self
    }

    pub fn with_hotkey(mut self, key: KeyCode) -> Self {
        self.hotkey = Some(key);
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_cooldown(mut self, progress: Option<f32>) -> Self {
        self.cooldown_progress = progress.map(|v| v.clamp(0.0, 1.0));
        self
    }

    pub fn with_on_press(mut self, msg: M) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn with_tooltip(mut self, tooltip: TooltipSpec) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn with_style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }
}

/// Declarative widget tree node parameterized by the application's `Message` type.
#[derive(Debug, Clone)]
pub enum Widget<M> {
    Empty,
    Container {
        id: Option<WidgetId>,
        padding: EdgeInsets,
        width: Option<f32>,
        height: Option<f32>,
        decoration: BoxDecoration,
        alignment: Option<Alignment>,
        block_pointer: bool,
        tooltip: Option<TooltipSpec>,
        child: Option<Box<Widget<M>>>,
    },
    Padding {
        padding: EdgeInsets,
        child: Box<Widget<M>>,
    },
    SizedBox {
        width: Option<f32>,
        height: Option<f32>,
        child: Option<Box<Widget<M>>>,
    },
    Align {
        alignment: Alignment,
        child: Box<Widget<M>>,
    },
    Flex {
        direction: Axis,
        main_axis: MainAxisAlignment,
        cross_axis: CrossAxisAlignment,
        spacing: f32,
        children: Vec<FlexChild<M>>,
    },
    Stack {
        children: Vec<StackChild<M>>,
    },
    Grid {
        columns: usize,
        cell_width: f32,
        cell_height: f32,
        spacing: f32,
        children: Vec<Widget<M>>,
    },
    Text {
        text: String,
        style: TextStyle,
    },
    ProgressBar {
        value: f32,
        fill_color: Color,
        bg_color: Color,
        border_color: Color,
        width: Option<f32>,
        height: f32,
        corner_radius: f32,
        label: Option<String>,
    },
    Button(ButtonWidget<M>),
    WorldCanvas(WorldCanvas<M>),
}

/// Builder struct for `Widget::Container` ergonomics.
#[derive(Debug, Clone)]
pub struct ContainerBuilder<M> {
    id: Option<WidgetId>,
    padding: EdgeInsets,
    width: Option<f32>,
    height: Option<f32>,
    decoration: BoxDecoration,
    alignment: Option<Alignment>,
    block_pointer: bool,
    tooltip: Option<TooltipSpec>,
    child: Option<Box<Widget<M>>>,
}

impl<M> Default for ContainerBuilder<M> {
    fn default() -> Self {
        Self {
            id: None,
            padding: EdgeInsets::ZERO,
            width: None,
            height: None,
            decoration: BoxDecoration::default(),
            alignment: None,
            block_pointer: false,
            tooltip: None,
            child: None,
        }
    }
}

impl<M> ContainerBuilder<M> {
    pub fn id(mut self, key: &str) -> Self {
        self.id = Some(widget_id(key));
        self
    }

    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }

    pub fn block_pointer(mut self, block: bool) -> Self {
        self.block_pointer = block;
        self
    }

    pub fn tooltip(mut self, tooltip: TooltipSpec) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn child(mut self, child: Widget<M>) -> Self {
        self.child = Some(Box::new(child));
        self
    }

    pub fn build(self) -> Widget<M> {
        Widget::Container {
            id: self.id,
            padding: self.padding,
            width: self.width,
            height: self.height,
            decoration: self.decoration,
            alignment: self.alignment,
            block_pointer: self.block_pointer,
            tooltip: self.tooltip,
            child: self.child,
        }
    }
}

impl<M> Widget<M> {
    pub fn container() -> ContainerBuilder<M> {
        ContainerBuilder::default()
    }

    pub fn padding(padding: EdgeInsets, child: Widget<M>) -> Self {
        Self::Padding {
            padding,
            child: Box::new(child),
        }
    }

    pub fn sized_box(width: Option<f32>, height: Option<f32>, child: Option<Widget<M>>) -> Self {
        Self::SizedBox {
            width,
            height,
            child: child.map(Box::new),
        }
    }

    pub fn spacer(width: f32, height: f32) -> Self {
        Self::SizedBox {
            width: Some(width),
            height: Some(height),
            child: None,
        }
    }

    pub fn align(alignment: Alignment, child: Widget<M>) -> Self {
        Self::Align {
            alignment,
            child: Box::new(child),
        }
    }

    pub fn row(
        main_axis: MainAxisAlignment,
        cross_axis: CrossAxisAlignment,
        spacing: f32,
        children: Vec<FlexChild<M>>,
    ) -> Self {
        Self::Flex {
            direction: Axis::Horizontal,
            main_axis,
            cross_axis,
            spacing,
            children,
        }
    }

    pub fn column(
        main_axis: MainAxisAlignment,
        cross_axis: CrossAxisAlignment,
        spacing: f32,
        children: Vec<FlexChild<M>>,
    ) -> Self {
        Self::Flex {
            direction: Axis::Vertical,
            main_axis,
            cross_axis,
            spacing,
            children,
        }
    }

    pub fn stack(children: Vec<StackChild<M>>) -> Self {
        Self::Stack { children }
    }

    pub fn grid(
        columns: usize,
        cell_width: f32,
        cell_height: f32,
        spacing: f32,
        children: Vec<Widget<M>>,
    ) -> Self {
        Self::Grid {
            columns: columns.max(1),
            cell_width,
            cell_height,
            spacing,
            children,
        }
    }

    pub fn text(text: impl Into<String>, style: TextStyle) -> Self {
        Self::Text {
            text: text.into(),
            style,
        }
    }

    pub fn progress_bar(
        value: f32,
        fill_color: Color,
        bg_color: Color,
        border_color: Color,
        width: Option<f32>,
        height: f32,
        label: Option<String>,
    ) -> Self {
        Self::ProgressBar {
            value: value.clamp(0.0, 1.0),
            fill_color,
            bg_color,
            border_color,
            width,
            height,
            corner_radius: 3.0,
            label,
        }
    }

    pub fn button(btn: ButtonWidget<M>) -> Self {
        Self::Button(btn)
    }

    pub fn world_canvas(canvas: WorldCanvas<M>) -> Self {
        Self::WorldCanvas(canvas)
    }
}
