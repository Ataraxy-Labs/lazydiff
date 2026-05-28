# Use Rust ownership for Diff Workspace state

The **Diff Workspace** has a single Rust owner for interactive diff-screen state. Cursor, scroll, side-filtered selection, search focus, inline review focus, draft editor focus, thread expansion, and mouse drag state are mutated through the Diff Workspace owner rather than directly by the app shell or renderer.

This is the **first instance of the broader surface-owner pattern** documented in ADR 0006. The diff-screen ownership rule and migration guardrails below are the proving ground for that pattern; subsequent surfaces (semantic, finder, commit list, queue/home, review sidebar, command palette) follow the same shape surface-by-surface as those slices land.

## Decision

LazyDiff treats Rust's ownership and borrowing model as an architectural tool, not just a memory-safety feature:

- The app shell owns the Diff Workspace value and passes `&mut DiffWorkspace` to update methods.
- The app shell does not directly mutate workspace-owned fields such as cursor, scroll, selection, inline focus, draft editor state, thread focus, or mouse selection.
- Rendering borrows workspace-produced view models, visual rows, decorations, and overlays; it does not mutate interaction state.
- Persisted UI state (saved cursor/scroll/expansion state) is restored *through* workspace operations or intents, never by poking private workspace fields from a persistence layer.
- Product IO remains outside the workspace and is requested through explicit effects.
- Migration uses a clean core plus temporary adapter: build the correct `DiffWorkspace` API first, keep its state private, and allow legacy adapter paths only as a bridge while old `App` code is moved over.
- New diff-screen behavior must be added to the clean workspace API, not to adapter code or scattered `App` helpers.

## Context

The current code has partial ownership: `DiffViewerState` owns reusable cursor, scroll, selection, search, visual-row, and render-model mechanics, while `App` still owns and directly mutates related diff-screen state such as inline focus, comment editor state, expanded threads, mouse drag flags, and some cursor/scroll fields. This makes it easy for one path to update focus while another path updates scroll or selection differently — the "split-brain" failure mode (`docs/research/synthesis.md`).

## Consequences

- Future changes should make invalid ownership hard to express in Rust, not merely discouraged by comments.
- Workspace methods become the narrow mutation surface for diff-screen behavior.
- The app shell becomes an effect runner and screen router, not the coordinator of diff internals.
- This requires introducing a new app-level workspace module before moving fields, so the reusable `lazydiff-diffs` crate stays focused on generic diff mechanics.
- The same ownership rule applies, surface-by-surface, to other interactive surfaces as they migrate (see ADR 0006).

## Guardrails against architectural slop

Use multiple enforcement layers so the decision survives future agent work:

1. **Rust privacy**: keep workspace-owned state private. Expose semantic methods or intents, not public mutable fields.
2. **Clean core plus temporary adapter**: adapter code may translate old app calls into the new workspace API, but adapter code is not the architecture and must not receive new behavior.
3. **Local agent guidance**: keep AGENTS-style rules near the repo/code so future agents are told not to add `App`-side cursor, scroll, selection, inline-focus, draft-editor, thread-focus, or mouse-selection mutation.
4. **Search-based checks**: use focused greps during migration to catch old patterns, for example `rg "viewer_mut\(\)|inline_focus|comment_modal" src/app.rs src/app`.
5. **Compiler ratchet**: after each field moves into `DiffWorkspace`, make it private and fix every compile error by routing through workspace operations.
6. **Review rule**: if a change needs direct `viewer_mut()` from `App`, stop and move that operation into the Diff Workspace owner instead.
