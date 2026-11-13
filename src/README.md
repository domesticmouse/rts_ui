# `src`

## Overview
Core source package for `rts_ui`, implementing a Flutter-inspired single-pass box-constraint layout engine, a declarative widget system built around The Elm Architecture (`Application` trait), a retained HUD and battlefield input runtime (`UiRuntime`), a batched WGPU SDF-quad and `ab_glyph` shelf-packed glyph-atlas renderer (`UiRenderer`, `FontSystem`, `shader.wgsl`), an interactive RTS HUD showcase (`RtsDemoApp`), and the Winit/WGPU host binary (`main.rs`).

## Interface
- **Crate Root & Modules** (`[lib.rs]`):
  - Public modules: `[lib.rs:demo]`, `[lib.rs:geom]`, `[lib.rs:layout]`, `[lib.rs:render]`, `[lib.rs:runtime]`, `[lib.rs:style]`, `[lib.rs:widget]`
  - Re-exports all public symbols from each module at the crate root (`rts_ui::*`).
- **Binary Entrypoint** (`[main.rs]`):
  - `[main.rs:main]` initializes `env_logger`, creates a `winit::event_loop::EventLoop`, and runs the private `AppHost` / `GpuState` WGPU window loop.
- **2D Geometry & Constraints** (`[geom.rs]`):
  - `[geom.rs:Vec2]` (`ZERO`, `new`, `zero`, `distance`, `length`, `normalized`)
  - `[geom.rs:Size]` (`ZERO`, `new`, `is_empty`)
  - `[geom.rs:Offset]` (`ZERO`, `new`)
  - `[geom.rs:EdgeInsets]` (`ZERO`, `all`, `symmetric`, `only`, `horizontal`, `vertical`)
  - `[geom.rs:Rect]` (`ZERO`, `new`, `from_pos_size`, `from_points`, `contains`, `intersects`, `center`, `deflate`)
  - `[geom.rs:Alignment]` (`TOP_LEFT`, `TOP_CENTER`, `TOP_RIGHT`, `CENTER_LEFT`, `CENTER`, `CENTER_RIGHT`, `BOTTOM_LEFT`, `BOTTOM_CENTER`, `BOTTOM_RIGHT`, `new`, `align_offset`)
  - `[geom.rs:BoxConstraints]` (`new`, `tight`, `loose`, `constrain`, `deflate`, `loosen`)
- **Styling & Hotkeys** (`[style.rs]`):
  - `[style.rs:Color]` (constants `TRANSPARENT`, `WHITE`, `BLACK`, `HUD_BG`, `HUD_PANEL`, `HUD_BORDER`, `HUD_BORDER_HIGHLIGHT`, `ACCENT_CYAN`, `ACCENT_GOLD`, `MINERAL_BLUE`, `GAS_GREEN`, `HEALTH_GREEN`, `HEALTH_YELLOW`, `HEALTH_RED`, `SHIELD_BLUE`, `DANGER_RED`; methods `rgba`, `rgb`, `from_hex`, `with_alpha`, `to_array`)
  - `[style.rs:BoxDecoration]` (`hud_panel`), `[style.rs:TextStyle]` (`new`), `[style.rs:ButtonStyle]`
  - `[style.rs:KeyCode]` (`Q`, `W`, `E`, `R`, `T`, `A`, `S`, `D`, `F`, `Z`, `X`, `C`, `V`, `Digit1`..`Digit4`, `Escape`, `Space`; methods `label`, `from_winit`)
- **Declarative Widgets & TEA** (`[widget.rs]`):
  - `[widget.rs:Application]` (`Message`, `update`, `view`)
  - `[widget.rs:WidgetId]`, `[widget.rs:widget_id]`
  - `[widget.rs:Axis]`, `[widget.rs:MainAxisAlignment]`, `[widget.rs:CrossAxisAlignment]`
  - `[widget.rs:TooltipSpec]` (`new`, `with_hotkey`, `with_minerals`, `with_gas`, `with_cooldown`)
  - `[widget.rs:WorldUnitOverlay]`, `[widget.rs:WorldOrderMarker]`, `[widget.rs:SelectUnitCallback]`, `[widget.rs:BoxSelectCallback]`, `[widget.rs:RightClickCallback]`, `[widget.rs:WorldCanvas]`
  - `[widget.rs:FlexChild]` (`fixed`, `expanded`), `[widget.rs:StackChild]` (`NonPositioned`, `Positioned`)
  - `[widget.rs:ButtonWidget]` (`new`, `with_sub_label`, `with_hotkey`, `with_enabled`, `with_selected`, `with_cooldown`, `with_on_press`, `with_tooltip`, `with_style`)
  - `[widget.rs:ContainerBuilder]` (`id`, `padding`, `width`, `height`, `size`, `decoration`, `alignment`, `block_pointer`, `tooltip`, `child`, `build`)
  - `[widget.rs:Widget]` (`Empty`, `Container`, `Padding`, `SizedBox`, `Align`, `Flex`, `Stack`, `Grid`, `Text`, `ProgressBar`, `Button`, `WorldCanvas`; constructors `container`, `padding`, `sized_box`, `spacer`, `align`, `row`, `column`, `stack`, `grid`, `text`, `progress_bar`, `button`, `world_canvas`)
- **Single-Pass Layout Engine** (`[layout.rs]`):
  - `[layout.rs:TextMeasurer]` (`measure_text`), `[layout.rs:MonospaceTestMeasurer]`
  - `[layout.rs:LayoutNode]`, `[layout.rs:LayoutNodeKind]`, `[layout.rs:layout_tree]`
- **Retained Runtime & Draw List** (`[runtime.rs]`):
  - `[runtime.rs:MouseButton]`, `[runtime.rs:QuadPrimitive]`, `[runtime.rs:TextPrimitive]`, `[runtime.rs:DrawList]`
  - `[runtime.rs:UiRuntime]` (`new`, `ui_wants_pointer`, `active_selection_box`, `set_shift_held`, `handle_cursor_moved`, `handle_mouse_button`, `handle_key_press`, `build_draw_list`)
- **WGPU Renderer & WGSL Shader** (`[render.rs]`, `[shader.wgsl]`):
  - `[render.rs:FontSystem]` (`new`, `TextMeasurer` impl)
  - `[render.rs:UiRenderer]` (`new`, `render`, `font_system`)
  - `[shader.wgsl:Globals]`, `[shader.wgsl:atlas_tex]`, `[shader.wgsl:atlas_sampler]`, `[shader.wgsl:VertexInput]`, `[shader.wgsl:VertexOutput]`, `[shader.wgsl:vs_main]`, `[shader.wgsl:fs_main]`
- **RTS Demo Application** (`[demo.rs]`):
  - `[demo.rs:UnitAction]`, `[demo.rs:UnitKind]` (`name`, `cost`, `train_duration`, `max_hp`, `max_energy`, `radius`, `color`, `is_building`, `weapon_info`)
  - `[demo.rs:UnitEntity]` (`active_production_fraction`)
  - `[demo.rs:RtsMessage]`, `[demo.rs:RtsDemoApp]` (`new`, `supply_used`, `Application` impl)

## Invariants
- `BoxConstraints` enforces `0.0 <= min_width <= max_width` and `0.0 <= min_height <= max_height`, preserving `f32::INFINITY` on unbounded max dimensions during deflation (`[geom.rs:BoxConstraints::new]`, `[geom.rs:BoxConstraints::tight]`, `[geom.rs:BoxConstraints::loose]`, `[geom.rs:BoxConstraints::constrain]`, `[geom.rs:BoxConstraints::deflate]`).
- `Vec2::normalized` returns `Vec2::ZERO` when `length() <= f32::EPSILON`; `Rect::from_points` and `Rect::deflate` ensure non-negative `width` and `height` (`[geom.rs:Vec2::normalized]`, `[geom.rs:Rect::from_points]`, `[geom.rs:Rect::deflate]`).
- `widget_id` computes a deterministic 64-bit FNV-1a hash (`0xcbf29ce484222325` / `0x100000001b3`) from `name.as_bytes()`; `FlexChild::expanded` clamps flex to `>= 0.0`, `Widget::grid` enforces `columns >= 1`, and `Widget::progress_bar` and `ButtonWidget::with_cooldown` clamp values to `0.0..=1.0` (`[widget.rs:widget_id]`, `[widget.rs:FlexChild::expanded]`, `[widget.rs:Widget::grid]`, `[widget.rs:Widget::progress_bar]`, `[widget.rs:ButtonWidget::with_cooldown]`).
- `layout_tree` lays out the root widget with `BoxConstraints::tight(viewport)` and resolves child relative offsets from `Offset::ZERO` into absolute screen-space `Rect`s (`[layout.rs:layout_tree]`, `[layout.rs:resolve_absolute_positions]`).
- In `layout_flex`, non-flex children are measured first with unbounded main axis, then remaining finite space `(max_main - fixed_main_sum - total_spacing).max(0.0)` is split proportionally by `flex / total_flex`; `layout_stack` resolves non-positioned children under `constraints.loosen()` before constraining positioned children against `stack_size` (`[layout.rs:layout_flex]`, `[layout.rs:layout_stack]`).
- `UiRuntime::ui_wants_pointer` returns `true` iff `pointer_captured_by_hud || pressed_id.is_some()`, blocking battlefield click, drag-box, and right-click pass-through whenever the pointer is over a blocking HUD node or pressing a HUD button (`[runtime.rs:UiRuntime::ui_wants_pointer]`, `[runtime.rs:UiRuntime::handle_mouse_button]`, `[runtime.rs:hit_test_hud]`).
- `hit_test_hud` traverses `LayoutNode` children in reverse paint order (topmost first) and returns the first hit with `hud_blocked || button_node.is_some() || tooltip.is_some()` (`[runtime.rs:hit_test_hud]`).
- Battlefield marquee box selection (`active_selection_box` and `on_box_select`) activates only when `start.distance(current) > 4.0`; shorter left-click releases invoke `on_select_unit` against the topmost unit within `u.radius + 6.0` (`[runtime.rs:UiRuntime::active_selection_box]`, `[runtime.rs:UiRuntime::handle_mouse_button]`).
- Button clicks and `handle_key_press` hotkeys emit `on_press` only when `enabled == true` and `cooldown_progress.is_none()`, with `collect_hotkey_messages` returning after the first matching button (`[runtime.rs:UiRuntime::handle_mouse_button]`, `[runtime.rs:collect_hotkey_messages]`).
- Floating tooltip overlays in Layer 2 clamp card width to `270.0_f32.min((viewport.width - 16.0).max(120.0))` and clamp `(x, y)` inside `[8.0, (viewport - card_size - 8.0).max(8.0)]` (`[runtime.rs:emit_tooltip_overlay]`).
- `FontSystem` embeds `../assets/fonts/Hack-Regular.ttf` into a `1024 x 1024` `R8Unorm` shelf-packed atlas with 1-texel padding, clearing `cache` and resetting cursors to `(1, 1)` on vertical overflow (`[render.rs:ATLAS_SIZE]`, `[render.rs:FONT_BYTES]`, `[render.rs:FontSystem::get_or_rasterize_glyph]`).
- `UiRenderer::render` emits primitives in strict layer order: Layer 0 (`world_quads`, `world_texts`), Layer 1 (`hud_quads`, `hud_texts`), Layer 2 (`overlay_quads`, `overlay_texts`); `shader.wgsl:fs_main` switches between SDF rounded-rect rendering (`params.z < 0.5`) and glyph-atlas sampling (`params.z >= 0.5`) (`[render.rs:UiRenderer::render]`, `[shader.wgsl:fs_main]`).
- `AppHost` clamps logical viewport dimensions to `>= 1.0`, skips surface reconfiguration on zero-sized resize/lost events, and only dispatches non-repeat key presses (`[main.rs:AppHost::logical_viewport]`, `[main.rs:AppHost::window_event]`, `[main.rs:AppHost::render_frame]`).
- `RtsDemoApp::update` returns early on `Tick` when `paused` is `true`, clamps `dt` to `0.0..=0.1`, caps building `production_queue.len() < 5`, requires `stim_cooldown <= 0.0` and `hp > 10.0` for `TriggerStimpack`, and prioritizes mobile units over buildings in `BoxSelect` (`[demo.rs:RtsDemoApp::update]`).

## Side Effects
- **GPU & Windowing I/O**: `[main.rs:main]` initializes `env_logger` and runs a polling `winit` `EventLoop`; `[main.rs:AppHost]` creates an OS window, initializes `wgpu::Instance`/`Surface`/`Adapter`/`Device`/`Queue`, reconfigures the swapchain surface on resize/loss, and presents frames via `queue.submit` and `frame.present()` (logging warnings via `log::warn!` on surface errors).
- **GPU Resource Allocation & Uploads**: `[render.rs:UiRenderer::new]` allocates shader module, uniform buffer, `1024 x 1024` `R8Unorm` atlas texture, sampler, bind group, pipeline, and vertex/index buffers; `[render.rs:FontSystem::get_or_rasterize_glyph]` uploads rasterized glyph bitmaps via `queue.write_texture`; `[render.rs:UiRenderer::render]` grows vertex/index buffers on overflow, writes uniforms/geometry via `queue.write_buffer`, and encodes a clearing render pass (`[render.rs:UiRenderer::render]`).
- **In-Memory State Mutations**: `[layout.rs:layout_tree]` and `[runtime.rs:UiRuntime::build_draw_list]` mutate `&mut dyn TextMeasurer`; `[runtime.rs:UiRuntime]` mutates pointer/hover/press/drag/modifier state and invokes `WorldCanvas` callbacks (`on_select_unit`, `on_box_select`, `on_right_click`); `[demo.rs:RtsDemoApp::update]` mutates simulation time, resources, unit positions/HP/energy/cooldowns, production queues, order markers, and selection state.

## Verification
- Run: `cargo test`
