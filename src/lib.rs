//! `rts_ui`: Flutter-inspired constraint layout + Elm Architecture UI framework on WGPU for RTS games.

pub mod demo;
pub mod geom;
pub mod layout;
pub mod render;
pub mod runtime;
pub mod style;
pub mod widget;

pub use demo::{RtsDemoApp, RtsMessage, UnitAction, UnitKind};
pub use geom::{Alignment, BoxConstraints, EdgeInsets, Offset, Rect, Size, Vec2};
pub use layout::{layout_tree, LayoutNode, LayoutNodeKind, MonospaceTestMeasurer, TextMeasurer};
pub use render::{FontSystem, UiRenderer};
pub use runtime::{DrawList, MouseButton, QuadPrimitive, TextPrimitive, UiRuntime};
pub use style::{BoxDecoration, ButtonStyle, Color, KeyCode, TextStyle};
pub use widget::{
    widget_id, Application, Axis, ButtonWidget, CrossAxisAlignment, FlexChild, MainAxisAlignment,
    StackChild, TooltipSpec, Widget, WidgetId, WorldCanvas, WorldOrderMarker, WorldUnitOverlay,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_constraints_invariants() {
        let tight = BoxConstraints::tight(Size::new(200.0, 100.0));
        assert_eq!(tight.constrain(Size::new(50.0, 300.0)), Size::new(200.0, 100.0));

        let loose = BoxConstraints::loose(Size::new(400.0, 300.0));
        assert_eq!(loose.constrain(Size::new(150.0, 450.0)), Size::new(150.0, 300.0));

        let deflated = tight.deflate(EdgeInsets::symmetric(10.0, 25.0));
        assert_eq!(deflated.min_width, 150.0);
        assert_eq!(deflated.max_width, 150.0);
        assert_eq!(deflated.min_height, 80.0);
        assert_eq!(deflated.max_height, 80.0);
    }

    #[test]
    fn row_and_column_flex_distribution() {
        let mut measurer = MonospaceTestMeasurer;
        let tree: Widget<()> = Widget::row(
            MainAxisAlignment::Start,
            CrossAxisAlignment::Stretch,
            10.0,
            vec![
                FlexChild::fixed(Widget::spacer(100.0, 40.0)),
                FlexChild::expanded(1.0, Widget::spacer(0.0, 40.0)),
                FlexChild::expanded(2.0, Widget::spacer(0.0, 40.0)),
            ],
        );

        let laid_out = layout_tree(&tree, Size::new(720.0, 100.0), &mut measurer);
        assert_eq!(laid_out.children.len(), 3);
        // Available for expanded = 720 - 100 - (2 * 10) = 600 -> 1:2 split => 200 and 400
        assert_eq!(laid_out.children[0].rect, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(laid_out.children[1].rect, Rect::new(110.0, 0.0, 200.0, 100.0));
        assert_eq!(laid_out.children[2].rect, Rect::new(320.0, 0.0, 400.0, 100.0));
    }

    #[test]
    fn stack_and_positioned_hud_anchoring() {
        let mut measurer = MonospaceTestMeasurer;
        let app = RtsDemoApp::new();
        let tree = app.view();
        let viewport = Size::new(1280.0, 800.0);
        let layout = layout_tree(&tree, viewport, &mut measurer);

        // Children: [0]=WorldCanvas, [1]=TopBar, [2]=Minimap, [3]=SelectionPanel, [4]=CommandCard
        assert_eq!(layout.children.len(), 5);
        assert_eq!(layout.children[0].rect, Rect::new(0.0, 0.0, 1280.0, 800.0));
        assert_eq!(layout.children[1].rect, Rect::new(16.0, 12.0, 1248.0, 46.0));
        assert_eq!(layout.children[2].rect, Rect::new(16.0, 584.0, 236.0, 200.0));
        assert_eq!(layout.children[3].rect, Rect::new(268.0, 584.0, 664.0, 200.0));
        assert_eq!(layout.children[4].rect, Rect::new(948.0, 584.0, 316.0, 200.0));
    }

    #[test]
    fn grid_4x3_command_card_layout() {
        let mut measurer = MonospaceTestMeasurer;
        let cells: Vec<Widget<()>> = (0..12)
            .map(|i| Widget::button(ButtonWidget::new(&format!("b_{i}"), format!("B{i}"))))
            .collect();
        let grid = Widget::align(
            Alignment::TOP_LEFT,
            Widget::grid(4, 70.0, 50.0, 5.0, cells),
        );
        let layout = layout_tree(&grid, Size::new(400.0, 300.0), &mut measurer);
        let grid_node = &layout.children[0];
        assert_eq!(grid_node.children.len(), 12);
        // Cell (0,0)
        assert_eq!(grid_node.children[0].rect, Rect::new(0.0, 0.0, 70.0, 50.0));
        // Cell (3,0)
        assert_eq!(grid_node.children[3].rect, Rect::new(225.0, 0.0, 70.0, 50.0));
        // Cell (0,2)
        assert_eq!(grid_node.children[8].rect, Rect::new(0.0, 110.0, 70.0, 50.0));
    }

    #[test]
    fn ui_wants_pointer_and_box_selection_passthrough() {
        let mut measurer = MonospaceTestMeasurer;
        let mut app = RtsDemoApp::new();
        let viewport = Size::new(1280.0, 800.0);
        let layout = layout_tree(&app.view(), viewport, &mut measurer);
        let mut rt = UiRuntime::new();

        // Hover over minimap HUD panel -> ui_wants_pointer() == true
        rt.handle_cursor_moved(Vec2::new(100.0, 650.0), &layout);
        assert!(rt.ui_wants_pointer());

        // Drag starting inside HUD panel must NOT start a battlefield box selection
        let msgs = rt.handle_mouse_button(MouseButton::Left, true, &layout);
        assert!(msgs.is_empty());
        assert!(rt.active_selection_box().is_none());
        rt.handle_cursor_moved(Vec2::new(400.0, 400.0), &layout);
        assert!(rt.active_selection_box().is_none());
        let msgs = rt.handle_mouse_button(MouseButton::Left, false, &layout);
        assert!(msgs.is_empty());

        // Hover over battlefield center -> ui_wants_pointer() == false
        rt.handle_cursor_moved(Vec2::new(500.0, 250.0), &layout);
        assert!(!rt.ui_wants_pointer());

        // Drag box around Marines (530..580, 270..340)
        rt.handle_mouse_button(MouseButton::Left, true, &layout);
        rt.handle_cursor_moved(Vec2::new(610.0, 360.0), &layout);
        let sel_box = rt.active_selection_box();
        assert!(sel_box.is_some());
        let msgs = rt.handle_mouse_button(MouseButton::Left, false, &layout);
        assert_eq!(msgs.len(), 1);
        for m in msgs {
            app.update(m);
        }
        assert!(app.selected_ids.contains(&5));
        assert!(app.selected_ids.contains(&6));
        assert!(app.selected_ids.contains(&7));
    }

    #[test]
    fn hotkeys_respect_enabled_and_cooldown_state() {
        let mut measurer = MonospaceTestMeasurer;
        let mut app = RtsDemoApp::new();
        let viewport = Size::new(1280.0, 800.0);

        // With squad selected (including Marines), Stimpack [R] works once and puts Marines on cooldown.
        let layout = layout_tree(&app.view(), viewport, &mut measurer);
        let rt = UiRuntime::new();
        let msgs = rt.handle_key_press(KeyCode::R, &layout);
        assert_eq!(msgs.len(), 1);
        for m in msgs {
            app.update(m);
        }

        // Now Marines are on Stimpack cooldown -> pressing [R] again should emit nothing.
        let layout_cd = layout_tree(&app.view(), viewport, &mut measurer);
        let msgs_cd = rt.handle_key_press(KeyCode::R, &layout_cd);
        assert!(msgs_cd.is_empty());

        // Clear selection -> Move [Q] becomes disabled and emits nothing.
        app.update(RtsMessage::ClearSelection);
        let layout_empty = layout_tree(&app.view(), viewport, &mut measurer);
        let msgs_disabled = rt.handle_key_press(KeyCode::Q, &layout_empty);
        assert!(msgs_disabled.is_empty());
    }

    #[test]
    fn floating_tooltip_overlay_and_viewport_clamping() {
        let mut measurer = MonospaceTestMeasurer;
        let app = RtsDemoApp::new();
        let viewport = Size::new(1280.0, 800.0);
        let layout = layout_tree(&app.view(), viewport, &mut measurer);
        let mut rt = UiRuntime::new();

        // Command card starts at x=948, y=584; Move [Q] button is in the first slot near (990, 640).
        rt.handle_cursor_moved(Vec2::new(985.0, 640.0), &layout);
        let draw_list = rt.build_draw_list(&layout, viewport, &mut measurer);

        assert!(!draw_list.overlay_quads.is_empty(), "expected tooltip card quad");
        assert!(!draw_list.overlay_texts.is_empty(), "expected tooltip text");

        for q in &draw_list.overlay_quads {
            assert!(q.rect.x >= 8.0);
            assert!(q.rect.y >= 8.0);
            assert!(q.rect.x + q.rect.width <= viewport.width - 8.0 + 0.01);
            assert!(q.rect.y + q.rect.height <= viewport.height - 8.0 + 0.01);
        }
    }
}
