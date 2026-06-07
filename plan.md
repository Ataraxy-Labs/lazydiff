# LazyDiff giant-diff rendering plan

Goal: make `lazydiff --branch` and giant patches feel instant and stay smooth even with thousands of changed files, without breaking vim-style navigation, mouse selection, search, review comments, collapsed unchanged-line expansion, or syntax highlighting.

Reality check: a cold full `git diff origin/develop` in `/Users/palanikannanm/Documents/work/plane-ee-wt/dashboard-widgets-pages` is already around one second and produces about 20 MB / 7.5k files. The product target is therefore **instant first paint + immediate interaction**, not synchronous full-content completion in `<10ms`.

## References to keep checking

- Pierre DOM virtualization: `/Users/palanikannanm/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/components/Virtualizer.ts`
- Pierre file/window mapping: `/Users/palanikannanm/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/utils/virtualDiffLayout.ts`
- Pierre viewport windowing: `/Users/palanikannanm/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/utils/createWindowFromScrollPosition.ts`
- Pierre worker highlighting: `/Users/palanikannanm/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/worker/WorkerPoolManager.ts`
- Pierre highlighter render path: `/Users/palanikannanm/.cache/checkouts/github.com/pierrecomputer/pierre/packages/diffs/src/utils/renderDiffWithHighlighter.ts`
- Zed display-map layering: `/Users/palanikannanm/.cache/checkouts/github.com/zed-industries/zed/crates/editor/src/display_map.rs`
- Zed fold/wrap maps: `/Users/palanikannanm/.cache/checkouts/github.com/zed-industries/zed/crates/editor/src/display_map/fold_map.rs`, `/Users/palanikannanm/.cache/checkouts/github.com/zed-industries/zed/crates/editor/src/display_map/wrap_map.rs`
- Zed summary tree: `/Users/palanikannanm/.cache/checkouts/github.com/zed-industries/zed/crates/sum_tree/src/sum_tree.rs`
- Zed syntax snapshots: `/Users/palanikannanm/.cache/checkouts/github.com/zed-industries/zed/crates/language/src/syntax_map.rs`

## Guardrails

- Agent instruction: every time an agent checks off an item in this file, it must continue working through the remaining unchecked checklist items. Do not stop after a partial checklist unless blocked by a real external dependency; if blocked, document the blocker inline and keep going on any independent unchecked item.
- Vim/cursor/search/review state must stay keyed by logical diff rows or explicit line targets, not by transient rendered rows.
- Collapsed unchanged-context rows are real focus targets. Clicking them should focus/expand them; scrolling should not bury the cursor under them.
- Rendering must be viewport-driven: no full visual-row vector for common scroll, mouse, cursor, or `G` paths.
- Syntax highlighting must never block first paint on huge diffs. Plain diff text first; highlighted spans later.
- Do not replace the current TUI with web/DOM machinery. Borrow Pierre’s layout concepts and Zed’s map/snapshot discipline in Rust.

## Checklist

- [x] Measure the regression on the huge Plane worktree and identify the bottleneck.
  - `git diff origin/develop` itself is roughly 0.5–1.2s.
  - Previous synchronous startup path took roughly 122–126s because it parsed/highlighted the full 20 MB diff before first paint.
- [x] Add temporary safety valves so users are not blocked while the deeper architecture lands.
  - Deferred `--branch` initial patch loading into the TUI refresh path.
  - Huge non-deferred diffs use limited Pierre/source-aware highlighting instead of highlighting every file before paint.
- [x] Introduce a lightweight `DiffLayout`/seek layer over the existing `DiffDocument` row cache.
  - Must answer: total visual rows, visual index for logical row, logical row for visual index, and visible rows for `(scroll_y, height)`.
  - Inline review/editor blocks are extra visual height attached after logical rows.
  - This is the Rust terminal equivalent of Pierre’s `virtualDiffLayout` plus Zed-style display-map separation.
- [x] Replace full visual-row materialization in cursor, mouse, `G`, and scroll paths.
  - Remove hot calls that allocate/scan `visual_rows_with_inline_blocks(...)` for the whole document.
  - Preserve existing vim behavior by translating through layout seeks, not changing cursor identity.
- [x] Make collapsed unchanged-context boundaries first-class in the layout.
  - Cursor centering/ensure-visible treats them like normal rows.
  - Mouse click on a collapsed row focuses it and expands the correct gap.
  - Expanded context removes the helper boundary row and preserves syntax colors for unfolded source lines.
- [x] Add progressive syntax scheduling with no permanent cutoff.
  - Plain rows render immediately.
  - The foreground path keeps first paint fast.
  - Large diffs schedule a background full-document syntax pass, so files after the initial budget still get colored.
  - Background results merge syntax/source spans into the current document without collapsing expanded unchanged rows or jumping the cursor.
  - Collapsed unchanged rows hydrate local/commit file sources on demand before expansion; PR sources are attached from the existing batch source fetch.
- [x] Cache/hydrate highlight sources without keychain/auth checks.
  - Local branch source reads are normal disk/git data, not GitHub auth-gated.
  - Expanded local/commit source is hydrated into the document once, then reused.
  - PR diff/source data is cached through the existing PR diff cache and persisted query cache; no startup keychain prompt is required for local diffs.
- [x] Add focused perf instrumentation and tests.
  - Unit tests for layout seek invariants with inline blocks and collapsed/expanded gaps.
  - Bench path for huge patch render/seek without source highlighting.
  - Regression checks for mouse row mapping, `/` search editing/backspace behavior, and sidebar shortcut visibility.
- [x] Decide on streaming/indexing branch diffs by file.
  - First paint can show file headers/progress before the entire patch is parsed.
  - Not needed for the current done criteria: `git diff` is much faster than highlighting, and `--branch` now defers loading into the TUI instead of blocking startup.

## Pierre-level smooth highlighting checklist

Agent instruction for this section: when you check off an item, do not stop. Keep implementing and verifying the next unchecked item until this entire section is complete. If you cannot complete an item, write the blocker and continue with any independent remaining item.

- [x] Add a renderer-level fallback so visible rows are never fully plain while exact highlights are late.
  - Empty `syntax_spans` now get lightweight lexical coloring for keywords, strings, comments, numbers, booleans, and common types.
  - Explicit Pierre/giallo spans still override fallback spans.
- [x] Split visible highlight work from prefetch highlight work.
  - Visible files are cached/highlighted first.
  - Prefetch files run after visible work and cannot delay visible exact colors.
- [x] Expand highlight prefetch beyond the exact viewport.
  - Include visible files first, then nearby document-row files.
  - Keep a cap so prefetch does not explode on huge diffs.
- [x] Keep expanded unchanged-context rows highlightable after source hydration.
  - Source lines without syntax spans no longer count as “highlight complete.”
  - Expanded rows can receive exact cached/daemon spans after source lines are already present.
- [x] Add viewport-first exact highlighting in the daemon.
  - Request visible line windows before full-file highlighting.
  - Return exact spans for visible line ranges first, then continue full-file/background work.
  - Avoid making direct-open near the end wait for full-file source highlighting.
- [x] Add compact viewport/chunk highlight cache.
  - Key by protocol/theme version, path/language, source hash, side, and line chunk.
  - Read only the visible or nearby chunk instead of multi-MB full-file JSON.
  - Keep full-file cache only as a secondary/background optimization.
- [x] Ensure prefetch/background work cannot block visible exact highlighting.
  - High priority: currently visible misses.
  - Medium priority: nearby directional prefetch.
  - Low priority: background full-file/cache warming.
  - Never let prefetch/background jobs block visible jobs.
  - Current implementation keeps the daemon simple and makes prefetch cache-only on misses; visible misses are the only requests allowed to start daemon work, so a serial daemon cannot be occupied by speculative prefetch.
- [x] Add cancellation or stale deprioritization for old prefetch work.
  - If the viewport jumps, old prefetch should stop consuming scarce daemon capacity.
  - Visible work for the current request should supersede stale work.
- [x] Add cache sizing and eviction.
  - Bound total highlight cache size.
  - Evict old chunks/files by age or LRU.
  - Preserve correctness with protocol/theme/source-hash invalidation.
- [x] Add direct-open/end-of-diff regression coverage.
  - Test that visible jobs are emitted/applied before prefetch jobs.
  - Test that opening directly near late files does not wait on unrelated earlier files.
  - Add trace/assertions around visible-first ordering.
- [x] Add fast-scroll regression coverage.
  - Simulate large jumps through many files.
  - Assert renderer fallback colors visible rows immediately.
  - Assert exact highlight requests prioritize the current visible window.

## Done criteria

- Huge `--branch` opens to a real UI immediately, with a calm loader only while the diff body is refreshing.
- Scrolling/PageDown/`G` on the 7.5k-file Plane diff stays responsive.
- Diff colors appear in production and dev builds, including unfolded unchanged context.
- No startup GitHub/keychain prompt is required for local diffs or branch diffs.
- `cargo check --workspace`, targeted diff-buffer tests, and release build pass.
- The Pierre-level smooth highlighting checklist is fully checked off: visible rows are never plain, exact visible highlights arrive before prefetch/background work, and direct-open/fast-scroll regressions are covered.
