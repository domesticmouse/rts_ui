# rts_ui MCP Layer: Dual-Mode UI & Simulation Control

* **Author:** Brett Morgan
* **Date:** 2026-11-13

## Overview

This proposal adds a Model Context Protocol (MCP) server layer to `rts_ui` using the official Rust `rmcp` SDK. The integration serves two workflows through one shared runtime core:

1. **Interactive play-testing (Streamable HTTP):** When `rts_ui` runs with a `winit`/`wgpu` window, an optional background `tokio` + `axum` Streamable HTTP endpoint (such as `http://127.0.0.1:<port>/mcp`) allows Antigravity to connect to the live window via `mcp_config.json` `"url"`.
2. **Automated UI testing (Headless stdio):** When launched in headless stdio mode, `rts_ui` skips `winit` and `wgpu` initialization, measures text on the CPU using `FontSystem::new()` (`src/render.rs`), steps simulation time on demand, and communicates over `stdin`/`stdout`.

To ensure AI usage actually exercises the UI surface rather than bypassing it, the server retains human-readable string widget keys alongside `u64` `WidgetId` values, exposes structured snapshots of the laid-out widget tree, `DrawList` primitives, `UiRuntime` pointer state, and `RtsDemoApp` game state, and routes actions through `UiRuntime` with structured precondition error reporting. Offscreen GPU PNG screenshot capture from `UiRenderer` is deferred to future work.

## Analysis

Across the seven review lenses (design, codebase observability, YAGNI/architecture, UX/comprehension, backward compatibility, structural maintainability, and contradiction resolution), the codebase reveals one major structural advantage alongside six high-priority design tensions that must be resolved before drafting the implementation spec.

### Verified Baseline Strength
* **CPU Text Measurement Parity (`src/render.rs`):** `FontSystem::new()` loads the embedded `Hack-Regular.ttf` font and implements `TextMeasurer` using `ab_glyph` entirely on the CPU. Only `FontSystem::get_or_rasterize_glyph` touches `&wgpu::Queue` and `&wgpu::Texture`. Decoupling CPU font metrics from `GpuState` (`src/main.rs`) allows headless stdio mode to produce identical `layout_tree` rectangles and `DrawList` primitives (including Layer 2 tooltip wrapping) as the live GPU window for any matching logical viewport `Size`.

### Core Contradictions & Inventive Resolutions

1. **Shared Runtime Core vs. Dual Execution Loop Drift (`src/main.rs`, `src/runtime.rs`)**
   * *Conflict:* `AppHost` (`src/main.rs`) currently owns `RtsDemoApp`, `UiRuntime`, `cached_layout`, and `hud_wants_pointer` synchronization inline inside `winit` callbacks. Building a second orchestration loop for headless stdio would cause behavioral drift (such as `hud_wants_pointer` lagging by one frame or layout not rebuilding after updates), while polling channels inside `AppHost::render_frame()` stalls if macOS throttles `RedrawRequested` when the window is occluded or minimized, or before `ApplicationHandler::resumed()` creates `GpuState`.
   * *Resolution (Extraction + Inversion):* Extract a pure-CPU `UiSession` struct in the library crate that owns `RtsDemoApp`, `UiRuntime`, `FontSystem`, logical `viewport`, `cached_layout`, `cached_draw_list`, and monotonic `frame_seq` / `tick_seq` / `state_epoch` counters. Both headless stdio and `AppHost` delegate every state transition and snapshot to `UiSession`. In windowed HTTP mode, incoming requests wake the main thread immediately via `winit::event_loop::EventLoopProxy` (`ApplicationHandler::user_event`) with bounded channels, request timeouts, and explicit shutdown cleanup on `WindowEvent::CloseRequested`—never waiting on GPU swapchain presentation.
   * *Secondary Trade-off:* `FontSystem` must move out of `GpuState`/`UiRenderer` (or split CPU font metrics from the GPU glyph atlas) so layout runs before `resumed()` and without a GPU device.

2. **TEA Purity vs. Precondition Error Reporting (`src/widget.rs`, `src/runtime.rs`, `src/demo.rs`)**
   * *Conflict:* `Application::update` returns `()` and `UiRuntime::handle_mouse_button` / `handle_key_press` return `Vec<M>`, silently ignoring invalid inputs. Silent no-ops exist at both layers: `UiRuntime` ignores clicks on disabled/cooldown buttons, HUD-blocked right-clicks, and unbound `KeyCode`s (`collect_hotkey_messages` also returns on the first match); `RtsDemoApp::update` silently ignores `IssueMoveOrder` with zero mobile units selected, `TriggerStimpack` when `hp <= 10.0` or on cooldown, `TrainUnit` when all producer queues hold 5 items, `CancelProduction` with out-of-range slots, and `FocusUnitFromGroup` for missing unit IDs. Changing `Application::update` or existing `UiRuntime` signatures directly would break the `winit` loop and unit tests (`src/lib.rs`).
   * *Resolution (Extraction + Intermediary):* Keep existing `Application::update` and `UiRuntime::handle_*` signatures intact for human input, and add checked diagnostic dispatch methods (e.g., `UiRuntime::try_handle_*` returning UI outcomes like `BlockedByHud`, `Disabled`, `OnCooldown`, `UnboundHotkey`, `OutOfViewport`, plus an `RtsDemoApp::apply_checked` helper returning typed domain outcomes like `NoMobileUnitsSelected`, `ProductionQueueFull`, `InsufficientResources`, `InsufficientHp`). Both paths share one underlying evaluation function so rules cannot diverge.
   * *Secondary Trade-off:* Domain precondition checks live behind a domain-specific adapter (`McpDomainProvider` or `RtsDemoApp` helper) rather than inside generic `UiRuntime`, keeping the UI framework decoupled from RTS rules.

3. **String Widget Keys vs. Per-Frame Allocations & Compatibility (`src/widget.rs`, `src/layout.rs`, `src/demo.rs`)**
   * *Conflict:* `widget_id(name: &str)` (`src/widget.rs`) hashes keys with FNV-1a into `u64` `WidgetId` and discards the string. Adding a mandatory `String` field to `ButtonWidget`, `Widget::Container`, and `LayoutNodeKind` breaks struct compatibility and allocates heap strings on every 60 FPS `view()` rebuild. Meanwhile, several containers (`src/demo.rs:765, 803, 915`) and all `Text`/`ProgressBar` widgets have no IDs, and duplicate keys (`q_slot_{qi}`, `sel_card_{id}`) are unchecked.
   * *Resolution (Preliminary Action + Segmentation):* Introduce a `WidgetKey` value type pairing `id: WidgetId` with `label: Option<Cow<'static, str>>` (populated automatically by `ButtonWidget::new` and `ContainerBuilder::id` so static literals like `"cmd_move"` stay zero-allocation, while manual `WidgetId` usage remains backward-compatible). Validate key uniqueness during layout indexing and return an ambiguity error rather than picking the first node on collision.
   * *Secondary Trade-off:* Adds `Cow<'static, str>` size to button/container nodes and requires assigning keys to readouts or using structural selectors when inspecting non-button HUD text.

4. **Complete Observability vs. Token Budget & Non-Serializable Closures (`src/layout.rs`, `src/runtime.rs`, `src/widget.rs`)**
   * *Conflict:* `LayoutNode<M>` contains `WorldCanvas<M>` with `Arc<dyn Fn(...) -> M>` callbacks and `Button::on_press: Option<M>`, which cannot derive `serde::Serialize`. Serializing full `LayoutNode` trees (with `LayoutNodeKind::Wrapper` nodes) and full `DrawList` vectors (where `emit_world_canvas` emits ~24 background grid-line `QuadPrimitive`s every frame plus 3–5 quads per unit) produces 5k–20k+ JSON tokens per turn and couples clients to cosmetic wrapper depth.
   * *Resolution (Segmentation + Dynamics):* Map engine types into versioned, pure-data Snapshot DTOs (`schema_version` included). Segment observation into focused tools or scoped flags (`observe_state`, `observe_layout`, `observe_draw_list`) where defaults return compact semantic summaries (game state, keyed/interactive widgets with computed `Rect`s, runtime pointer state, and Layer 2 tooltip overlays) while wrapper nodes and background grid quads are opt-in parameters (`include_wrappers`, `include_world_grid`, `layers`).
   * *Secondary Trade-off:* An agent verifying low-level rendering geometry must explicitly request full `DrawList` / `LayoutNode` expansion.

5. **Real-Time Simulation & OS Mouse vs. Deterministic Multi-Step Control (`src/main.rs`, `src/demo.rs`)**
   * *Conflict:* In windowed mode, `AppHost::render_frame` dispatches `RtsMessage::Tick(dt)` from `Instant::now()` every frame, and physical mouse jitter overwrites `UiRuntime::cursor_pos` / `hovered_id`. In `RtsDemoApp::update`, `Tick(dt)` silently returns early when `paused == true` and silently clamps `dt` to `0.0..=0.1`. Passing `dt = 1.0` once silently simulates `0.1s` instead of `1.0s`, while unpaused windowed execution shifts unit positions between `observe` and `click_at` (TOCTOU misses).
   * *Resolution (Beforehand Cushioning + Segmentation):* Provide compound atomic gestures (`click_at`, `drag_box`, `hover_widget`) alongside granular `pointer_down` / `pointer_move` / `pointer_up` tools (so mid-drag `active_selection_box` can be inspected when desired, while failed/aborted gestures clean up `pressed_id`, `world_drag_start`, and per-call `shift_held` overrides). Separate real-time frame ticking from an explicit `step_simulation(dt)` tool that subdivides `dt > 0.1` into deterministic `<= 0.1` substeps (up to a bounded cap) and rejects stepping while paused unless unpaused first. In windowed mode, support pausing/auto-pausing and expose `state_epoch` on every response to detect concurrent human or tick drift.

### Additional Operational & Dependency Guardrails
* **Cargo Feature & CLI Isolation:** Gate `rmcp`, `tokio`, `axum`, `serde`, `serde_json`, and `schemars` behind an optional Cargo feature (e.g., `mcp`) and explicit CLI flags (`--mcp-stdio --headless` vs. `--mcp-http [ADDR]`, default `127.0.0.1:<port>/mcp`) so default `rts_ui` builds stay synchronous and lightweight with no listening socket by default.
* **Stdout Protection & Tracing:** Reserve `stdout` strictly for JSON-RPC in stdio mode; route all `tracing` / `wgpu` / `winit` logs to `stderr` (or an optional `--log-file`) with per-request `request_id`/`session_id` spans and a bounded diagnostic ring buffer.
* **Loopback HTTP Safety & Sessions:** Bind Streamable HTTP to `127.0.0.1` by default, fail fast on port-in-use errors, bound concurrent sessions/queue depth, and use stateless or restart-tolerant session handling so recompiling and restarting `rts_ui` does not break Antigravity's active connection.
* **Action-Model Alignment (`active_command`):** Note a pre-existing gap in `src/demo.rs`: clicking `"cmd_move"` (`Q`) or `"cmd_attack"` (`A`) sets `active_command`, while `WorldCanvas::on_right_click` always emits `IssueMoveOrder` and left-click never consumes `active_command`. Tool docs and semantic commands need to handle this directly.

**Ideal Final Result:** A single `UiSession` core serves both deterministic headless stdio tests and live `winit`/`wgpu` Streamable HTTP play-testing, giving LLMs compact semantic observations and explicit UI/domain error feedback while routing interactions through `UiRuntime` and `layout_tree` with zero duplicated validation logic.

## Goals vs. Non-Goals

* **In-Scope Goals:**
  * Expose an `rmcp`-based MCP server gated behind a Cargo feature flag and explicit CLI flags.
  * Support interactive windowed mode over Streamable HTTP (`/mcp` on `127.0.0.1`) bridged via `EventLoopProxy` to `AppHost` (`src/main.rs`).
  * Support deterministic headless stdio mode using CPU `FontSystem` text measurement (`src/render.rs`) without requiring a window or `wgpu::Device`, including configurable viewport dimensions and deterministic `step_simulation` sub-stepping.
  * Unify windowed and headless orchestration into a shared `UiSession` engine and decouple CPU text metrics from GPU glyph-atlas state.
  * Retain optional zero-allocation `Cow<'static, str>` widget keys alongside `u64` `WidgetId`s with duplicate-key detection.
  * Expose versioned, filterable snapshot DTOs covering `RtsDemoApp` state, keyed/laid-out `LayoutNode` rects, `UiRuntime` pointer/selection state, and `DrawList` primitives (including Layer 2 tooltips).
  * Route both semantic tools (click by widget key, press hotkey, select/order units) and low-level pointer tools (down, move, up, drag with button and shift modifier) through `UiRuntime`, reporting structured UI and domain precondition errors instead of silent no-ops.

* **Out-of-Scope Non-Goals:**
  * Offscreen `wgpu` texture readback and PNG screenshot capture from `UiRenderer` (explicitly deferred to future work).
  * Legacy `2024-11-05` dual-endpoint HTTP+SSE MCP transport (`/sse` + `/messages`), which Antigravity does not support.
  * Remote non-loopback deployment, multi-tenant authentication, or multi-player networking.
  * Rewriting `rts_ui`'s single-pass layout algorithm (`src/layout.rs`) or changing existing `MonospaceTestMeasurer` unit tests (`src/lib.rs`).

## Alternatives Weighed

1. **Write-only UI command tools without state observation or precondition errors**
   * *Rejected by author:* Leaves the LLM blind to unit positions, resources, cooldowns, and selection state, and causes retry/hallucination loops when `UiRuntime` or `RtsDemoApp::update` silently ignores clicks on disabled or cooling-down widgets.
2. **Two disconnected integrations (direct `RtsMessage` LLM bypass + separate raw-coordinate UI test harness)**
   * *Rejected by author:* Bypassing `view()`, `layout_tree()`, `hit_test_hud()`, and `ui_wants_pointer()` during LLM play-testing leaves the UI surface untested and doubles the API maintenance burden.
3. **Offscreen WGPU texture readback and PNG screenshots in v1**
   * *Rejected by author (deferred to future work):* CPU `FontSystem` + structured `LayoutNode` and `DrawList` inspection already verifies layout geometry, hit-testing, and primitive emission without adding GPU staging-buffer overhead to headless tests or vision-token cost to every observation.
4. **Picking only one transport (stdio-only or HTTP-only) in v1**
   * *Weighed from Skeptical Architect (YAGNI) lens:* Considered shipping stdio-only first to avoid `tokio`/`axum` HTTP complexity, or HTTP-only to avoid dual CLI modes. Retained dual transport in the concept because automated CI/regression tests need port-free deterministic stdio while interactive development play-testing in Antigravity needs Streamable HTTP attached to the live window—mitigated by sharing one `rmcp` `ServerHandler` and one `UiSession` core behind Cargo feature flags.
5. **Mutating `Application::update` and `UiRuntime::handle_*` return types in place vs. additive checked methods**
   * *Weighed from Compatibility & Inventive Catalyst lenses:* Rejected breaking trait/method signature changes in place because they would break `winit` event handlers and existing `src/lib.rs` tests; adopted additive checked/outcome wrappers sharing one evaluation path.
6. **Direct `serde::Serialize` on `Widget<M>`, `LayoutNode<M>`, and `RtsDemoApp` vs. Snapshot DTOs**
   * *Weighed from Structural & Compatibility lenses:* Rejected direct serialization because `WorldCanvas<M>` holds `Arc<dyn Fn(...)>` closures and `Button` holds `on_press: Option<M>`, making `LayoutNode<M>` non-serializable and tightly coupling wire schemas to internal ASTs. Adopted versioned DTO projections.

## Key Decisions & Staging for Next Steps

### Decisions Resolved
* **Single `UiSession` core + dual transport:** Both `--mcp-stdio --headless` and `--mcp-http` run against one `UiSession` state machine using CPU `FontSystem` metrics and one `rmcp` `ServerHandler`.
* **Runtime-routed semantic + pointer tools:** Semantic widget clicks resolve `Rect` centers from `LayoutNode` and execute through `UiRuntime`, paired with granular/composite pointer tools for sub-widget geometry tests (such as the `4.0` px drag threshold and HUD pointer blocking).
* **Compact default snapshots with opt-in raw layers:** Default observations return compact semantic state + keyed nodes + Layer 2 tooltips; full wrapper nodes and Layer 0 background grid quads are opt-in.
* **Additive precondition outcomes:** Existing `Application::update` and `UiRuntime::handle_*` signatures remain intact while MCP calls receive structured UI and domain failure codes.

### Open Questions to Resolve Before `/egm-draft`
1. **Transport rollout & dependency gating:** Should `mcp` be an optional Cargo feature (or separate workspace binary) from day one, and should v1 implement both stdio and Streamable HTTP immediately or stage stdio first and Streamable HTTP second on top of `UiSession`?
2. **Windowed simulation & cursor concurrency:** When connected over Streamable HTTP to a live window, should `RtsDemoApp` auto-pause (or require explicit pause before `step_simulation`), and how should `state_epoch` handle concurrent physical mouse movement vs. MCP pointer tools?
3. **Widget key coverage:** Which currently unkeyed widgets in `src/demo.rs` (such as resource readouts, `status_banner`, unit selection portrait, or minimap telemetry) need explicit `WidgetKey` labels vs. remaining readable via game-state / `Text` snapshots?
4. **Domain vs. framework boundary:** Should the MCP server expose a generic `Application`-agnostic UI toolset with an optional `McpDomainProvider` extension trait for `RtsDemoApp`, or ship a unified `rts_ui` tool surface in v1?
