## README.md

No drift detected

## src/README.md

- **Crate Root Re-export Scope (`[src/lib.rs]` vs. `src/README.md` line 9)**: `src/README.md` states that `lib.rs` "Re-exports all public symbols from each module at the crate root (`rts_ui::*`)." In `src/lib.rs` (lines 11–20), `demo::UnitEntity` (`src/demo.rs:124`) and `widget::{SelectUnitCallback, BoxSelectCallback, RightClickCallback, ContainerBuilder}` (`src/widget.rs:123-127`, `src/widget.rs:333`) are public in their respective submodules (`rts_ui::demo` and `rts_ui::widget`) but are not re-exported at the crate root.
