# rts_ui MCP Layer: Dual-Profile Co-Pilot, Coaching & UI Test Server

* **Author:** Brett Morgan
* **Date:** 2026-11-13

## Overview

This proposal adds an unconditional `rmcp`-based Model Context Protocol (MCP) layer to `rts_ui`, activated via runtime CLI flags (`--mcp-stdio` or `--mcp-http [ADDR]`). Because the existing `rts_ui` codebase is an initial draft, the design refactors core framework and demo types directly rather than adding backward-compatibility shims. Both modes share one CPU-capable `UiSession` engine and one `rmcp` `ServerHandler`, exposed through two mode-specific tool profiles:

1. **`--mcp-http` (Live Co-Pilot & Coach Profile):** Attaches to a running `winit`/`wgpu` window over Streamable HTTP (`/mcp` on `127.0.0.1`) via `EventLoopProxy` so Antigravity can assist a human player during an ongoing game. It omits `step_simulation` and raw pointer tools (`move_cursor`, `click_at`, `drag_box`) so the AI never hijacks `UiRuntime::cursor_pos` or cancels a human drag box. Instead, it provides compact game-state observation, selection-independent unit and production commands (targeting explicit `unit_ids` and `building_id` without modifying `selected_ids`, `active_command`, or `status_banner`), a click-through in-HUD AI Activity Console, and TTL-backed coaching highlights for HUD buttons, moving units, and map coordinates.
2. **`--mcp-stdio` (Headless UI Test Profile):** Runs without `winit` or `wgpu` using CPU `FontMetrics` (`Hack-Regular.ttf`) for automated testing over `stdin`/`stdout`. It exposes `step_simulation(dt)`, `UiRuntime` pointer/hotkey/widget actions, coaching and console triggers (so overlay behavior is testable headlessly), co-pilot parity tools, and structured `LayoutNode` / `DrawList` snapshots with explicit precondition errors. Offscreen GPU PNG screenshot capture from `UiRenderer` is deferred to future work.

## Analysis

Removing backward-compatibility constraints simplifies the architecture across five areas while addressing the operational, layering, and game-rule risks surfaced during review.

### 1. Direct Semantic `WidgetId` & First-Class Outcome Signatures (`src/widget.rs`, `src/layout.rs`, `src/runtime.rs`)
* **Semantic `WidgetId`:** Replace the lossy FNV-1a `u64` `WidgetId` (`src/widget.rs:17-27`) directly with a string-backed `WidgetId(Cow<'static, str>)` across `Widget`, `LayoutNode`, and `UiRuntime` (`hovered_id`, `pressed_id`). Static keys such as `"cmd_stim"` and `"top_resource_bar"` remain zero-allocation `&'static str` values, dynamic keys (`"q_slot_0"`, `"sel_card_5"`) use owned strings, and duplicate-key collisions are flagged directly during layout indexing without maintaining parallel `u64` + `String` fields.
* **Structured Outcomes Everywhere:** Update `UiRuntime` input handlers and `Application::update` (`src/widget.rs:12`) directly to return typed outcome/error values (`InputOutcome<M>`, `Result<CommandOutcome, ActionError>`) instead of returning `Vec<M>` and `()` with silent drops. The `winit` host ignores or logs non-fatal human misclicks, while the MCP server returns exact UI or domain failure reasons (such as `BlockedByHud`, `Disabled`, `OnCooldown`, `UnboundHotkey`, `OutOfViewport`, `NoMobileUnitsSelected`, `ProductionQueueFull`, `InsufficientResources`, `InsufficientHp`).

### 2. Unified Target-Parameterized RTS Rules (`src/demo.rs`)
* **Single Rule Engine (`TargetScope` + `ActorOrigin`):** Refactor `RtsMessage` and `RtsDemoApp` so unit and production commands natively accept a `TargetScope` (`Selected` vs. explicit `unit_ids` / `building_id`) and an `ActorOrigin` (`Human` vs. `AiCopilot`). Human UI inputs route `CommandOutcome` updates to `status_banner` and green `WorldOrderMarker`s; AI co-pilot actions preserve `selected_ids`, `active_command`, and `status_banner`, logging their `CommandOutcome` to the AI Activity Console and emitting distinct AI order markers.
* **Fixing Draft RTS Simulation Gaps:**
  * Add an explicit cooldown/energy rule to `UnitAction::NanoRepair` (`src/demo.rs:478-487`) so neither human nor co-pilot has an infinite free-heal exploit.
  * Include queued units in `RtsDemoApp::supply_used()` (or `supply_reserved()`) so concurrent human and co-pilot production cannot overcommit `supply_cap`.
  * Assign stable `queue_item_id` values to queued production items so `CancelProduction` cannot accidentally cancel the wrong unit if slot `0` finishes during an in-flight HTTP call.
  * Model real per-unit `AttackMove` and `HoldPosition` state on `UnitEntity` and wire `active_command` into `WorldCanvas` click handling so armed command-card buttons work as documented.
  * Deduplicate `unit_ids` batches and explicitly specify per-unit partial vs. atomic failure reporting.

### 3. Coaching Highlight Lifecycle, Layering & Paused Clocks (`src/runtime.rs`, `src/render.rs`, `src/demo.rs`)
* **Two Clocks (`sim_dt` vs. `ui_dt`):** `RtsDemoApp::update(Tick(dt))` exits early when `paused == true` (`src/demo.rs:270-273`). Store coaching highlights (`Widget(WidgetId)`, `Unit(u32)`, `WorldPoint(Vec2)`, `WorldRect(Rect)`) and AI console entries in a bounded `CoachState` inside `UiSession`, and advance UI overlay timers (`ui_dt`) independently of `paused`—driven by frame delta in `--mcp-http` (so hints still expire while paused) and deterministically inside `step_simulation(dt)` in `--mcp-stdio` (sub-stepping `dt > 0.1` in `<= 0.1s` steps up to a bounded max).
* **Layer Ordering:** Emit unit and map coaching highlights in Layer 0 (`world_quads`/`world_texts`) so Layer 1 HUD panels cover units passing beneath them (rejecting or warning when a map target falls entirely behind a `block_pointer(true)` HUD rect), and emit HUD button highlights in Layer 2 (`overlay_quads`/`overlay_texts`) **before** `emit_tooltip_overlay` so hover tooltips stay readable on top.
* **Creation Validation & Scoped Auto-Dismissal:** Validate target existence and usability at highlight creation (`!enabled || cooldown_progress.is_some()` or missing widget/unit fails fast). Auto-dismiss only on target-matching intentional human input (clicking the highlighted button or pressing its bound `KeyCode` hotkey, selecting/ordering the highlighted `unit_id`, or clicking/ordering within the map target) or when the target becomes disabled, cooling down, or missing—never on raw `CursorMoved` mouse jitter—and record a typed `DismissalReason` in diagnostics.

### 4. First-Class AI Activity Console & Text Wrapping (`src/widget.rs`, `src/layout.rs`, `src/runtime.rs`, `src/lib.rs`)
* **Generalize `wrap_text` to `Widget::Text`:** Move `wrap_text` out of `emit_tooltip_overlay` (`src/runtime.rs:817`) into `src/layout.rs` so `Widget::Text` supports optional wrapping and max-line truncation within `BoxConstraints::max_width`, preventing unclipped text overflow in `UiRenderer`.
* **Click-Through Console & Updated Tests:** Mount the bounded AI Activity Console (anchored away from initial unit spawns, such as top-right below `top_resource_bar`) as a first-class non-interactive HUD element (`block_pointer(false)`, zero child `Button`s or `TooltipSpec`s), harden `hit_test_hud` (`src/runtime.rs:222-277`) so `block_pointer(false)` nodes without tooltips never stop traversal, and update layout assertions in `src/lib.rs` (such as `stack_and_positioned_hud_anchoring`) to reflect the updated HUD tree.

### 5. CPU `FontMetrics` Split, Snapshot DTOs & Transport Lifecycle (`Cargo.toml`, `src/main.rs`, `src/render.rs`)
* **Separate CPU Metrics from GPU Atlas:** Move CPU `FontMetrics` (`ab_glyph` over `Hack-Regular.ttf`) out of `src/render.rs` into `src/layout.rs` (or a dedicated `font.rs` module) owned by `UiSession`, leaving `src/render.rs` strictly for WGPU rendering and `GlyphAtlas` texture caching. Layout and `DrawList` generation then run identically before `ApplicationHandler::resumed()` and in headless `--mcp-stdio`.
* **Unconditional `rmcp` & Isolation:** Add `rmcp`, `tokio`, `axum`, `serde`, `serde_json`, and `schemars` unconditionally to `Cargo.toml`, while keeping core UI modules (`geom`, `style`, `widget`, `layout`, `runtime`) decoupled from `rmcp`/`tokio`/`axum`. Default `cargo run` (no flags) starts the standalone desktop window with no async runtime or listening socket.
* **Transport & Payload Rules:**
  * `--mcp-stdio` implies headless mode, skips `winit`/`wgpu`, reserves `stdout` strictly for JSON-RPC, and routes all `tracing`/`log` output to `stderr` (or `--log-file`).
  * `--mcp-http [ADDR]` (mutually exclusive with `--mcp-stdio`) binds loopback `127.0.0.1` on a background `tokio` thread, wakes `winit` immediately via `EventLoopProxy` (`user_event`) with bounded channel depth and request timeouts, and cancels on `WindowEvent::CloseRequested`.
  * Map internal engine types (`LayoutNode`, `DrawList`, `UiRuntime`, `RtsDemoApp`) into versioned Snapshot DTOs (`schema_version`) rather than serializing closures in `WorldCanvas`. Default `--mcp-http` observations return compact semantic state (<1.5k tokens: economy, units, HUD bounds, active highlights, recent console lines), while `--mcp-stdio` acts as a superset profile exposing full/filtered `LayoutNode` and `DrawList` inspection plus co-pilot/coaching tools for deterministic coverage.

## Goals vs. Non-Goals

* **In-Scope Goals:**
  * Add `rmcp`, `tokio`, `axum`, `serde`, `serde_json`, and `schemars` unconditionally to `Cargo.toml`, activated only by `--mcp-stdio` or `--mcp-http`.
  * Refactor `WidgetId` to string-backed `Cow<'static, str>`, refactor `Application::update` and `UiRuntime` handlers to return structured outcomes/errors, and add wrapping support to `Widget::Text`.
  * Split CPU `FontMetrics` from GPU `GlyphAtlas` and extract a shared `UiSession` core used by both `AppHost` (`src/main.rs`) and headless `--mcp-stdio`.
  * Implement `--mcp-http` Streamable HTTP (`/mcp` on `127.0.0.1`) bridged via `EventLoopProxy` with bounded queues, request timeouts, and shutdown cancellation.
  * Implement `--mcp-stdio` headless mode with `stderr`-only logging, configurable viewport dimensions, deterministic `step_simulation(dt)` sub-stepping, and full UI + co-pilot test tools.
  * Refactor `RtsDemoApp` around `TargetScope` (`Selected` vs. explicit IDs) and fix draft rule gaps (`NanoRepair` gating, queued supply accounting, stable `queue_item_id`s, and per-unit `AttackMove`/`HoldPosition`).
  * Add a wrapped, click-through in-HUD AI Activity Console and TTL-backed coaching highlights for HUD buttons, moving units, and map areas with target-scoped auto-dismissal.

* **Out-of-Scope Non-Goals:**
  * Cargo feature flags gating `rmcp` or MCP sub-transports.
  * Backward-compatibility shims preserving draft `u64 WidgetId`s, silent `()` update signatures, or frozen 5-child test counts in `src/lib.rs`.
  * Offscreen `wgpu` texture readback and PNG screenshot capture from `UiRenderer` (deferred to future work).
  * Legacy `2024-11-05` dual-endpoint HTTP+SSE MCP transport (`/sse` + `/messages`).
  * Exposing `step_simulation` or raw pointer tools (`move_cursor`, `click_at`, `drag_box`) in live `--mcp-http` co-pilot mode.
  * Remote non-loopback hosting, multi-tenant authentication, or GPU scissor-clipping shader rewrites.

## Alternatives Weighed

1. **Preserving draft `WidgetId = u64`, `Application::update -> ()` signatures, and frozen layout test assertions via backward-compatibility shims**
   * *Rejected by author:* The existing codebase is an initial draft; maintaining dual `u64` + `String` IDs and parallel `try_handle_*` / `apply_checked` wrappers adds unnecessary complexity compared to refactoring those types directly.
2. **Gating MCP dependencies behind Cargo feature flags**
   * *Rejected by author:* Adds `#[cfg]` conditional compilation complexity and risks uncompiled/untested MCP code during standard `cargo test` runs; runtime CLI flags (`--mcp-stdio`, `--mcp-http`) control activation instead.
3. **Sharing `selected_ids` and `UiRuntime::cursor_pos` between human player and HTTP co-pilot**
   * *Rejected by author:* AI cursor calls or selection changes during live play interrupt human box-select drags, teleport hover tooltips, and cause the human's next right-click to move the wrong units.
4. **Write-only UI button commands without state observation or precondition errors**
   * *Rejected by author:* Leaves the LLM blind to unit/economy state and causes retry loops when `UiRuntime` or `RtsDemoApp` silently drops clicks on disabled or cooling-down widgets.
5. **Two disconnected integrations (direct `RtsMessage` LLM bypass with no UI test harness)**
   * *Rejected by author:* Leaves `view()`, `layout_tree()`, `hit_test_hud()`, and `ui_wants_pointer()` unexercised by automated AI testing.
6. **Offscreen WGPU texture readback and PNG screenshots in v1**
   * *Rejected by author (deferred to future work):* CPU `FontMetrics` + `LayoutNode`/`DrawList` snapshots verify layout geometry, hit-testing, and primitive emission without GPU staging-buffer overhead or vision token cost.
7. **Direct `serde::Serialize` on `Widget<M>` and `LayoutNode<M>`**
   * *Weighed from Structural & Compatibility lenses:* Rejected because `WorldCanvas<M>` holds `Arc<dyn Fn(...)>` closures and `Button` holds generic `M` payloads; adopted dedicated versioned Snapshot DTOs.

## Key Decisions & Staging for Next Steps

### Decisions Resolved
* **Clean-slate refactor over shims:** Replace `u64` `WidgetId` with `Cow<'static, str>`, return structured outcomes directly from `UiRuntime` and `Application::update`, split `FontMetrics` from `GlyphAtlas`, generalize `wrap_text` to `Widget::Text`, and fix `RtsDemoApp` rule gaps directly.
* **No Cargo feature flags:** `rmcp`, `tokio`, `axum`, `serde`, `serde_json`, and `schemars` live unconditionally in `Cargo.toml`; default `cargo run` launches the standalone window with no async runtime or open ports.
* **Dual mode-specific profiles on one `UiSession`:** `--mcp-http` exposes live co-pilot commands, compact observation, AI Activity Console, and coaching highlights without touching human pointer/selection state; `--mcp-stdio` exposes the full headless UI test surface (`step_simulation`, `UiRuntime` pointer/hotkey/widget tools, `LayoutNode`/`DrawList` snapshots) plus co-pilot/coaching tools.
* **Coaching targets & AI Console:** Coaching supports HUD buttons (`WidgetId`), dynamic units (`unit_id`), and battlefield areas (`Vec2`/`Rect`) with separate `ui_dt` timer decay, layered behind tooltips (`world` layer for battlefield targets; `overlay` pre-tooltip for HUD buttons), paired with a wrapped, non-interactive `block_pointer(false)` console.

### Open Questions to Confirm Before `/egm-draft`
1. **Batch partial vs. atomic failure for co-pilot `unit_ids`:** If the co-pilot calls `stimpack` or `move` with `[5, 6, 8]` where one unit is ineligible (for example, low HP, on cooldown, or non-Marine), should the call fail atomically or apply to eligible units and return per-unit statuses?
2. **CLI address defaults for `--mcp-http`:** Should `--mcp-http` require an address/port or default to a fixed loopback endpoint (such as `127.0.0.1:3000/mcp`) for predictable `mcp_config.json` configuration?
