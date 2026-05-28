# pierre — one geometry owner, sparse heights, coalesced redraw

Pierre is a browser-based code-review product known for rendering very large diffs smoothly. Its `@pierre/diffs` package shows the engineering pattern behind that. The lesson for LazyDiff is *not* about the browser; it is about how one owner of geometry plus dirty-flagged work plus coalesced redraw eliminates the "everyone rebuilds their own view of the world" bug class we have today.

## Pierre's architecture in one picture

```
┌────────────────────────────────────────────────────────┐
│ Virtualizer  (single owner)                            │
│   scrollTop · height · scrollHeight                    │
│   observers: Map<el, Instance>                         │
│   visibleInstances: Map<el, Instance>                  │
│   scrollDirty · heightDirty · scrollHeightDirty        │
│   IntersectionObserver decides visibility              │
│   ResizeObserver flips heightDirty                     │
└─────────────────────────┬──────────────────────────────┘
                          │ subscribes
┌─────────────────────────┴──────────────────────────────┐
│ VirtualizedFileDiff (per file)                         │
│   heights: sparse Map<lineIdx, height>                 │
│     only stores NON-standard heights                   │
│   estimateGeometry() for unmeasured lines              │
│   reconcileHeights() runs AFTER render                 │
│   onRender(dirty)  → returns immediately if not dirty  │
└─────────────────────────┬──────────────────────────────┘
                          │ queueRender(cb)
┌─────────────────────────┴──────────────────────────────┐
│ UniversalRenderingManager                              │
│   one requestAnimationFrame tick                       │
│   Set<callback>  deduplicates per-frame                │
│   drains callbacks; re-queues rAF only if new work    │
└────────────────────────────────────────────────────────┘
```

## The five lessons

| # | Pierre pattern | What it solves |
|---|---|---|
| 1 | One owner of geometry (`Virtualizer`) | No two consumers disagree on scroll/height/visibility |
| 2 | Sparse height cache — store only overrides from the standard height | Memory stays O(non-standard lines) |
| 3 | Estimate first, measure later (`reconcileHeights` after render) | Scrolling never blocks on measuring 100k lines |
| 4 | Coalesced render queue (`queueRender` + `Set`) | Burst of state changes → one render callback, not N |
| 5 | Dirty flags + `onRender(dirty)` per subscribed instance | Tree walks are cheap; clean nodes return instantly |

## What transfers to LazyDiff TUI

### One owner of geometry → the Diff Workspace

Today, `src/app.rs` has **5 calls** to a row counter that *lies* (`row_count_for_mode` — counts only diff text, not inline review rows) plus **6 independent rebuilds** of the truthful row list (`visual_rows_with_inline_blocks`). Each rebuild allocates a fresh `Vec` and re-walks the document.

Pierre's `Virtualizer` is the equivalent of what ADR 0004 already mandates: one owner. We are not inventing this — Pierre is just the strongest external proof that one owner per geometry actually works at scale.

### Sparse override cache for row heights

99% of LazyDiff visual rows are 1 terminal cell tall. Only inline review rows (comment boxes, expanded threads, draft editors) are taller. Store only the overrides:

```rust
struct RowHeightCache {
    overrides: HashMap<RowId, u16>,   // only non-1 heights
    // implicit default: 1
}
```

Same shape as Pierre's `heights: Map<number, number>` that only stores non-standard sizes.

### Coalesced redraw signal

`src/app.rs:679..324` already coalesces scroll-wheel and mouse-motion events. Extend the same pattern to *all* state-mutating intents: drain into one `redraw_dirty: bool` and check once per event-loop iteration. This is Pierre's `Set<callback>` deduplication, mapped to a TUI loop.

### Dirty-flagged rebuild of the row list

```rust
impl DiffWorkspace {
    fn rows(&mut self) -> &[WorkspaceVisualRow] {
        if self.rows_dirty {
            self.rebuild_rows();
            self.rows_dirty = false;
        }
        &self.rows_cache
    }
}
```

Pierre's `onRender(dirty)` is the equivalent: a consumer asks the producer for rows; the producer rebuilds only when something actually changed. Every state-mutating workspace method flips `rows_dirty = true`. Rust ownership enforces this: nothing outside the workspace can mutate state, so nothing outside can forget to invalidate.

## What does NOT transfer

| Pierre concept | Why not |
|---|---|
| DOM virtualization (don't render off-screen DOM) | Ratatui already paints only viewport cells. We have no "100k DOM nodes" cost. |
| `IntersectionObserver` / `ResizeObserver` | "Visible" in TUI is `scroll_y .. scroll_y + viewport_height`. Integer math. |
| Async height measurement after paint | "Height" in TUI is a number we choose at insert time. Measurement is free. |
| `requestAnimationFrame` framing | Our loop is event-driven, not frame-driven. We coalesce, we do not poll a frame clock. |

## Honest performance language (so we do not slip into slop)

Pierre's marketing-style "fast" claim is grounded in concrete browser-specific work: avoiding DOM cost on hidden content. For LazyDiff TUI we should describe performance in:

- **Input-to-render latency** — keypress → visible cell change.
- **Draw time** — ms per redraw, measured.
- **Event coalescing** — wheel events collapsed per loop iteration.
- **Idle redraw behavior** — what we do not redraw when nothing changed.

We should not claim "60 FPS TUI" because we are deliberately event-driven, not frame-driven.

## Where to look

Under `~/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/` (at tag `pierre-v1.2.1`):

- `components/Virtualizer.ts` — the single owner of geometry. 663 lines, worth reading in full.
- `components/VirtualizedFileDiff.ts` — per-file subscriber: sparse heights, `onRender(dirty)`, `reconcileHeights`.
- `managers/UniversalRenderingManager.ts` — 37 lines. The whole `queueRender` + `Set` + rAF coalescer fits on one screen.

## The carried-over rules

1. One owner per derived geometry; consumers borrow, never rebuild.
2. Cache stores overrides only; defaults are implicit.
3. Dirty flag is flipped by mutation, cleared by rebuild; consumers ask through the owner.
4. Coalesce input/state-change signals once per event-loop iteration.
5. Use honest performance language anchored in latency/draw time/coalescing, not framerate.

## Refresh

If `~/.cache/checkouts/github.com/pierrecomputer/pierre/` is missing, refresh with:

```sh
bash ~/.agents/skills/librarian/checkout.sh pierrecomputer/pierre --path-only
```

Do not write a new pierre claim in this file or in the ADRs without grounding it in the cached source.
