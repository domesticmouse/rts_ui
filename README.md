# rts_ui Blueprint

## System Overview
`rts_ui` is a Rust UI framework and interactive RTS HUD showcase combining a Flutter-inspired single-pass box-constraint layout engine, declarative widget composition under The Elm Architecture (`Application`), retained HUD/battlefield pointer and hotkey input routing (`UiRuntime`), and a batched WGPU SDF-quad + `ab_glyph` glyph-atlas renderer (`UiRenderer`) hosted on `winit`.

## Subdirectories & System Boundaries
- src: [./src/README.md](./src/README.md) - Core library and executable binary implementing 2D constraint geometry, styling, declarative widgets, single-pass layout, retained input/hit-testing runtime, WGPU batched rendering, and the `RtsDemoApp` showcase.

## Global Invariants
- Single-pass Flutter-style constraint flow: `layout_tree` enforces `BoxConstraints::tight(viewport)` at the root, clamps dimensions within `min <= max`, and resolves relative child offsets into absolute screen-space `Rect` coordinates (`[src/geom.rs:BoxConstraints]`, `[src/layout.rs:layout_tree]`).
- Strict HUD vs. battlefield pointer arbitration: HUD panels with `block_pointer(true)` and `ButtonWidget` nodes capture pointer input (`ui_wants_pointer()`), preventing click, marquee drag-box (`> 4.0` px threshold), and right-click pass-through to `WorldCanvas` (`[src/runtime.rs:UiRuntime::ui_wants_pointer]`, `[src/runtime.rs:UiRuntime::handle_mouse_button]`, `[src/runtime.rs:hit_test_hud]`).
- Deterministic render layering and viewport clamping: `UiRenderer::render` draws Layer 0 (world), Layer 1 (HUD), and Layer 2 (unclipped floating tooltips) in order, while `emit_tooltip_overlay` clamps tooltip cards within `8.0` px viewport margins (`[src/render.rs:UiRenderer::render]`, `[src/runtime.rs:emit_tooltip_overlay]`).
- Simulation and UI state integrity: button clicks and hotkeys fire only when `enabled && cooldown_progress.is_none()`, and `RtsDemoApp::update` clamps `Tick(dt)` to `0.0..=0.1` seconds while enforcing resource, supply, queue-length (`< 5`), and cooldown preconditions (`[src/runtime.rs:collect_hotkey_messages]`, `[src/demo.rs:RtsDemoApp::update]`).

## Global Side Effects
- OS windowing and WGPU swapchain presentation via `winit::event_loop::EventLoop` (`ControlFlow::Poll`), `env_logger` initialization, and `queue.submit` / `frame.present()` (`[src/main.rs:main]`, `[src/main.rs:AppHost]`).
- GPU buffer, pipeline, and `1024 x 1024` `R8Unorm` glyph-atlas texture allocation and updates (`queue.write_buffer`, `queue.write_texture`) using the embedded `assets/fonts/Hack-Regular.ttf` font (`[src/render.rs:UiRenderer]`, `[src/render.rs:FontSystem]`).
- In-memory mutations across `&mut dyn TextMeasurer`, `UiRuntime` interaction state, and `RtsDemoApp` simulation/selection state (`[src/layout.rs:layout_tree]`, `[src/runtime.rs:UiRuntime]`, `[src/demo.rs:RtsDemoApp]`).

## System Verification
- Run: `cargo test`
