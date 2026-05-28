# Keep product behavior in app-level workspaces and generic primitives in lazydiff-diffs

Lazydiff keeps `crates/lazydiff-diffs` as the fast, reusable diff core for documents, coordinates, Vim-like motion algorithms, visual rows, generic diff decorations, and rendering. Product-specific behavior — review notes, comments, drafts, thread expansion, editor lifecycle, semantic annotations, forge/CI annotations, and side effects — belongs in app-level **Diff Workspace** code (or other app-level surfaces), which converts product state into generic diff decorations, visual rows, and effects for the app shell.

This generic-core split applies to **all** product concerns, not just review comments. Anything that means "this is what our product does with the diff" belongs above `lazydiff-diffs`. The crate stays product-agnostic so it can be consumed by future surfaces, tools, or external integrations without forking.

## Reference

This follows the ProseMirror-style split: core modules own generic primitives (model, state, view, decorations), while behavior is added by external product/plugin layers that contribute commands, decorations, and view data without mutating the generic core.

## Consequences

- `lazydiff-diffs` does not learn product concepts (review thread, comment, draft, semantic node, forge annotation, CI status). It exposes the generic primitives those concepts can be expressed as.
- Renderer/coordinate math should not branch on product meaning. Product state becomes generic visual rows or decorations at the app/workspace boundary.
- New first-party features (blame, coverage, CI summaries, AI suggestions) follow the same split: app-level data, generic primitives, no core changes required.
- This ADR is the *generic-vs-product* boundary. The whole-TUI surface ownership pattern is recorded separately in ADR 0006.
