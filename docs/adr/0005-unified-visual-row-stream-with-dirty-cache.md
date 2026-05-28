# Use one unified visual-row stream with a dirty cache as the diff workspace's coordinate space

The Diff Workspace owns a single cached visual-row stream that all four consumers — keyboard navigation, scrolling, mouse mapping, and renderer iteration — read from. The cache is rebuilt only when state changes; consumers borrow the same slice within a frame.

## Decision

The Diff Workspace owner holds:

- `rows: Vec<WorkspaceVisualRow>` — the unified row stream. Private.
- `rows_dirty: bool` — flipped to `true` by any workspace mutation that affects layout (cursor that triggers reflow, scroll, inline focus, thread expansion, document change, viewport size). Reset to `false` after rebuild.
- `row_height_overrides: HashMap<RowId, u16>` — sparse map of *non-default* row heights only (most rows are height 1).

`WorkspaceVisualRow` is a Rust enum tagged by kind:

```text
enum WorkspaceVisualRow {
    DiffText      { side, doc_row, … },
    InlineReview  { block_id, line, side, … },
    Spacer        { side },
    FoldSummary   { fold_id, hidden_count, label, side, … },
    // future kinds added here; compiler ratchet forces every consumer
    // to handle them
}
```

### Folds are first-class on the stream

A **Fold** is a collapsed range of underlying rows. When a fold is collapsed, its hidden rows are **removed from the stream** during rebuild and replaced by a single `FoldSummary` row carrying the label, hidden count, and originating `FoldStrategy` id. When a fold is expanded, the hidden rows reappear in the next rebuild.

This means folds change *what rows exist*, not just what rows look like. They are therefore part of the stream — not a `DiffDecoration`. The workspace owner holds:

- `folds: BTreeMap<FoldId, FoldState>` — declared folds and their collapsed/expanded state. Private.
- `fold_candidates: Vec<FoldCandidate>` — last-sampled output from registered `FoldStrategy` contributions; refreshed when the document or fold-relevant inputs change.

Toggling a fold dirties `rows_dirty`; the next `frame()` rebuild produces a stream with the affected range collapsed or expanded. Navigation, scrolling, mouse mapping, and renderer iteration are unaware of folds beyond seeing `FoldSummary` as one more row kind, which the compiler ratchet forces every consumer to handle.

Coordinate mapping from a `FoldSummary` row to underlying document coordinates is the workspace owner's job (e.g., for clicking a fold summary to expand it, the renderer dispatches a `ToggleFold(fold_id)` intent; it does not compute doc rows).

Public API:

- `pub fn frame(&mut self, …) -> WorkspaceFrame<'_>` returns a borrow into the cache, lazily rebuilding only if `rows_dirty`.
- `WorkspaceFrame<'_>` carries `rows: &[WorkspaceVisualRow]`, cursor and scroll snapshots, and the workspace's borrowed read view.

All four consumers iterate `frame.rows`:

- keyboard navigation (`j`, `k`, `]c`, motion intents) reads positions inside `frame.rows`.
- scrolling math uses `frame.rows.len()` as the *only* truthful row total (the legacy `row_count_for_mode(...)` paths that ignore inline rows are removed from `src/app.rs`).
- mouse mapping computes `row = frame.scroll.first_visible + click_y` and indexes into `frame.rows`.
- renderer iterates the visible range of `frame.rows` and paints each kind.

Side-filtered selection becomes a *predicate over the same list* (`row.side == cursor.side`), not a second list.

## Reference

This decision is informed by Pierre's `@pierre/diffs` package (`docs/research/pierre.md`):

- One owner of geometry (`Virtualizer`) prevents multiple consumers from disagreeing about scroll/height/visibility.
- Sparse height cache stores only non-standard heights; default is implicit.
- Dirty flag plus lazy rebuild keeps the redraw cost proportional to *state change*, not to consumer count.
- Coalesced render queue eliminates duplicate work per frame.

The Pierre patterns transfer to a TUI even though DOM virtualization does not — see `docs/research/pierre.md` for the per-pattern transferability.

ProseMirror's typed-node document tree informs the unified-row-as-enum choice: future row kinds add a variant, and the compiler points at every consumer that must acknowledge them.

## Decision deferred to implementation slice

Where the cached row list physically lives: inside `crates/lazydiff-diffs::DiffViewerState`, or inside the new app-level `DiffWorkspace` module. Recommendation is the **app-level workspace**, matching ADR 0001 (generic core vs product surface) and ADR 0004 (Rust-owned workspace state): the workspace composes generic document rows from `lazydiff-diffs` with app-level inline review rows. The first implementation slice ratifies this choice or proposes an amendment.

## Consequences

- The bug class "four consumers each rebuild their own row list and disagree mid-frame" becomes structurally impossible. `frame.rows` is the only list; consumers cannot ask for it twice without going through the workspace's cache.
- The borrow checker enforces "no mutation while a paint pass is reading" — `frame()` returns a `&`-borrow tied to the workspace's lifetime; `update(intent)` requires `&mut`; the two cannot coexist.
- Adding a new row kind (AI suggestion, CI status summary, collapsed thread) is a new enum variant. Every `match` on the enum becomes a compile error until it acknowledges the new variant — architectural ratchet, not convention.
- The lying `row_count_for_mode(...)` helper is removed from `src/app.rs` callers; `frame.rows.len()` is the only legal row total.
- Sparse height overrides keep cache memory O(non-default rows), not O(all rows).
- Rebuild is `&mut self`; reads are `&self`. The compiler refuses any attempt to mutate while a `WorkspaceFrame<'_>` borrow is alive.
- A future "second-instance" surface that needs its own visual-row stream (e.g., the Semantic Workspace) follows the same shape (own cache, dirty flag, kind-tagged rows). This ADR's pattern is therefore reusable but not generalized into a trait yet (ADR 0006).
