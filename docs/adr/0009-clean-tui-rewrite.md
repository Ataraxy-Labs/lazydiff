# Build a clean v2 shared core instead of continuing in-place migration

## Status

Accepted.

## Context

Feature 001 proved the desired direction — Rust-owned surfaces, reducer/effect update loops, one Visual-Row Stream, contribution-shaped internals — but also exposed that the current app is too entangled for a clean field-by-field migration. Global shortcuts are not truly global, `Esc` navigation is inconsistent across surfaces, PR-context and local-diff-context actions bleed into each other, and many interaction paths still mutate shared state directly from `App`.

Continuing to wrap the old `App` risks adding adapter code faster than deleting legacy code. The product has now been built once; the team has enough knowledge to rebuild it with the right architecture from the start.

## Decision

Stop feature 001 as the active execution track. Build a clean, isolated v2 app in parallel while referencing legacy code only as behavior/reference material.

The v2 target is a **local Rust runtime server with renderer clients**, not a TUI-only rewrite. The server owns product truth: workspaces, parsed diff data, semantic row identities, command/keymap registry, effect model, plugin/contribution registry, command validation, IO policy, and semantic frames. A shared client-core package owns renderer-independent interaction logic: cursor movement, scroll anchoring, selection state, keymap application, command target construction, frame caching, and viewport requests. The TUI and GPUI/gpui-component apps are thin clients/renderers: they own host resources and lower shared client view models into terminal cells or GPUI elements. They must not fork product behavior.

ADRs 0001–0008 remain the architecture constraints for v2 unless explicitly amended:

- `lazydiff-diffs`-style core primitives stay generic; product behavior lives above them.
- Every serious surface has one Rust owner with private state.
- State changes go through intents and return effects.
- Async effects use generation tokens.
- Commands, keymaps, palette entries, inline rows, decorations, chrome slots, and fold strategies are contribution-shaped registered values.
- No public plugin runtime ships in this rewrite phase; the server-side contribution registry shape exists first so customization is shared by every renderer later.

The rewrite is broader than the old Diff Workspace migration. It covers the whole app architecture, including both terminal and GUI renderers, and may replace the diff core package as well. The legacy app remains runnable while v2 reaches parity gates, but new architecture work should happen under the v2 modules/crates rather than inside legacy `src/app.rs`.

## Rewrite shape

- New isolated v2 code lives in `packages/` and `apps/` so ownership is visible from day one:
  - `packages/lazydiff-v2-protocol`: renderer-agnostic semantic frames and client/server messages.
  - `packages/lazydiff-v2-diff`: parsed diff data and visual row frame construction.
  - `packages/lazydiff-v2-core`: server-owned AppCore, workspace state, commands, keymaps, effects, and contribution registry.
  - `packages/lazydiff-v2-client`: shared TUI/GUI client interaction state and reducers: cursor, scroll, selection, key handling, command targeting, frame cache, and viewport requests.
  - `apps/lazydiff-v2-server`: local runtime server for a workspace.
  - `apps/lazydiff-v2-tui`: terminal client/renderer.
  - `apps/lazydiff-v2-gui`: GPUI/gpui-component client/renderer.
- Renderer apps are thin host edges. They translate host events into shared client-core intents, execute client-core effects against the server protocol, and translate shared client view models into host-specific paint elements.
- Client presentation/interaction state is independent per renderer by default. TUI and GUI do not mirror scroll position, cursor position, active route, open overlays, or selection unless an explicit follow/sync mode is added later.
- Commands sent to the server should include explicit semantic targets and selection ranges from the shared client core. The server should not need a global cursor to infer what a client meant.
- The first tracer bullet is intentionally small: start a local server for a patch and render the server's current diff frame in the TUI and GPUI clients. Build the diff foundation solidly before adding higher app layers.
- The v2 architecture terms, diagrams, display-map shape, optional full-file context model, shared-core renderer boundary, and pi-mono-style contribution seams are documented in `docs/features/002-clean-tui-rewrite/ARCHITECTURE.md` and are part of this decision's implementation guidance.
- Feature 002 (`docs/features/002-clean-tui-rewrite`) is the active work tracker.
- Feature 001 remains historical evidence and a source of bug-class/test ideas, not the default work loop.

## Consequences

- Future agents should not continue issue 001 migration slices unless explicitly asked.
- It is acceptable for v2 to duplicate a small amount of behavior temporarily while isolated; the deletion gate is removing legacy paths once v2 reaches parity.
- v2 slices should be vertical product-flow slices with focused tests, not broad unverified rewrites.
- Renderer-specific code should be easy to delete or replace because product semantics live in the server-owned core and renderer-independent interaction behavior lives in the shared client core.
- Existing termwright tests remain useful as parity tests and should be extended for v2 behavior.
- The old app is reference code, not the target architecture.
