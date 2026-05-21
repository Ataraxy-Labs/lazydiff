# Diff Viewer Architecture Plan

## Agent Operating Rule

Before finalizing any response on this project:

- Re-read this file.
- If any item under "Compulsory completion order" is unchecked, continue with the first unchecked item.
- Final response must include:
  - completed checklist items this turn
  - next unchecked compulsory item
  - verification run
- Do not claim the diff viewer architecture work is done until every compulsory item is checked.

Goal: make lazydiff's diff viewer use a single, viewer-owned interaction/state architecture for reliable Vim-like UX, while preserving lazydiff's existing fast parsing/rendering core, Pierre styling, syntax highlighting, inline diff spans, themes, and colors.

Product direction: LazyDiff should become a "build your own diff / build your own code review" workspace. The core must stay simple, safe, and Rust-owned, while review workflows become customizable through bounded contribution seams: commands, keybindings, inline rows, decorations, review actions, chrome/status, and explicit effects. This does not mean arbitrary renderer mutation or a public plugin runtime in the first migration slice.

Engineering quality bar for this migration:

- Agents may implement the migration, but product and architecture decisions are human-owned. Use agents for execution speed, not unreviewed direction changes.
- Keep decisions thoughtful and explicit. Ask the human owner one focused architecture question at a time before major direction changes.
- Use Rust ownership to make scattered mutation hard to express, not merely discouraged by comments.
- Build the clean Diff Workspace core first; temporary adapter paths are allowed only as migration bridges.
- Avoid "AI slop" patterns: giant unreviewable rewrites, vague performance claims, hidden policy changes, and behavior without regression tests.
- Prefer honest TUI performance language: event-driven redraw, input-to-render latency, draw time, event coalescing, and idle redraw behavior. Do not market static TUI screens as continuously needing FPS.
- Each migration slice should state what ownership improved, what old mutation disappeared, what test or focused check protects it, and what documentation changed.
- Keep extensibility safe: custom review workflows should contribute view data, commands, decorations, and effects through the Diff Workspace, not bypass the visual-row model or renderer contract.

Intentional architectural divergence:

- [ ] Keep visual selections side-filtered in split view: selecting on the right must not also select/highlight the left, and vice versa.

## 0. Explicit Diff Viewer Architecture Closure

These are the remaining gaps that must be closed before calling the diff viewer revamp done.

Architecture source of truth: central viewer state produces visual rows and paint specs; renderer paints them. lazydiff intentionally adapts this for its left/right split diff buffers: each side has independent horizontal scroll and side-filtered selections, so the architecture must preserve `DiffSide` and pane-local `DiffPaneTextLayout` conversions instead of assuming one merged text buffer.

Compulsory completion order:

- [x] 1. Move cursor and yank flash into viewer-produced renderer overlays.
- [x] 2. Introduce a real `DiffRenderModel` seam so rendering consumes viewer/layout output.
- [x] 3. Make comments, notes, and drafts inline-only in the diff stream; remove bottom preview/modal presentation for normal review flow.
- [x] 4. Make inline comment/editor layout first-class enough for richer threaded comment UI.
- [x] 5. Document the diff coordinate contract near `DiffPaneTextLayout`.
- [x] 6. Replace remaining ad-hoc visual row/count paths with the shared model/count APIs.
- [x] 7. Finish explicit diff focus state cleanup beyond the compatibility `review_sidebar_focus` bridge.
- [x] 8. Replace lazydiff's modal-ish comment draft editing with an inline comment buffer.
- [x] 9. Make inline comment/editor rows navigable by `j/k` as real visual rows, including entering/leaving the box.
- [x] 10. Preserve multiline comment entry while using editor-local row/column state.
- [x] 11. Add comment editor normal/insert behavior for basic Vim motions (`h/j/k/l`, `0`, `$`, `i/a/A/o/O`, `dd`, `x`, `Esc`, submit).
- [x] 12. Make yank flash visibly reuse the same selection paint path and verify it in rendering tests or focused checks.
- [x] 13. Support inline thread display as collapsed first-message summaries with in-place expand/collapse for the full thread.
- [x] 14. Keep inline comment boxes side-local to the target left/right buffer instead of full-width decorations.
- [x] 15. Keep editor focus styling identical to unfocused comment styling; only text body/cursor is editable, not the title/header.

- [ ] True side-by-side visual-row model drives navigation, scrolling, mouse mapping, and renderer iteration.
- [ ] Inline comment and draft editor rows live in the diff visual-row stream, not separate modals/side panels only.
  - [x] Viewer has inline block visual rows.
  - [x] App review comments/drafts are fed into inline block rows.
  - [x] Renderer paints inline block rows.
- [x] Renderer consumes viewer-produced visual rows / overlay paint specs while preserving lazydiff styling.
  - [x] Renderer iterates viewer-produced document visual rows.
  - [x] Renderer paints inline/comment visual rows.
  - [x] Renderer consumes viewer overlay paint specs.
- [x] `DiffViewerState` owns diff state without the old `DiffViewState` adapter.
  - [x] App no longer syncs viewer from old `DiffViewState` before diff actions/render.
  - [x] Cursor screen position is computed by `lazydiff-diffs` viewer.
  - [x] Remaining helper/sidebar/finder/semantic paths read/write viewer state directly.
- [ ] Text objects are either complete for nested/multi-line delimiters or unsupported cases are explicit in UX/tests.
- [ ] Mouse drag selection continues correctly while scrolling.
- [ ] Interactive `target/dev-fast/lazydiff --branch` validation passes for cursor, selection, search, comments, and mouse.
- [x] Cursor rendering, mouse mapping, and selection anchoring all use the same visual-row coordinates when inline rows are present.
- [x] Add explicit same-row side switch key (`Tab`) in split view.

## 1. Cursor, Selection, and Viewport Ownership — target: 100%

Foundation: one diff viewer owner should drive cursor, scroll, selection, search, and render coordinates.

- [x] Add `DiffViewerState` in `crates/lazydiff-diffs`.
- [x] Move cursor row/column/goal/side into `DiffViewerState`.
- [x] Move visual selection into `DiffTextSelection`.
- [x] Move search state into `DiffViewerState`.
- [x] Use dynamic row code offsets instead of hardcoded split/unified offsets.
- [x] Fix split selection paint offset with renderer-level regression test.
- [x] Remove `DiffViewState` as the renderer/input adapter.
- [x] Make `DiffWidget` consume `DiffViewerState` directly.
- [x] Remove old `selected_row` / `selected_side` semantics from diff behavior.
  - [x] Diff cursor/render/mouse/review target paths use `DiffViewerState`.
  - [x] Sidebar/finder/semantic helper paths use viewer state directly.
- [ ] Ensure every cursor mutation calls the same visual-selection update path.
- [x] Ensure yank/comment/search range extraction all consume `DiffTextSelection` directly.

## 2. Side-by-Side Visual Row Mapping — target: 100%, with side-filtered selection

Build a side-by-side visual row architecture while keeping lazydiff's side-specific selection behavior.

- [x] Preserve side-specific selection behavior in split view.
- [x] Add side-preserving split vertical movement as an interim step.
- [x] Add a side-by-side row model.
- [x] Add side-by-side visual row construction.
- [x] Add visual-index lookup for document rows.
- [x] Add side-specific document-row lookup from visual rows.
- [x] Port side-by-side vertical cursor movement using visual rows.
- [x] Port side-by-side horizontal side crossing using visual rows.
- [x] Make scroll calculations use visual rows.
- [x] Make cursor screen-position calculation use visual rows.
- [x] Add tests for uneven add/delete blocks.
- [x] Add tests for split `j/k` side preservation.
- [x] Add tests for split `h/l` side crossing.

## 3. Keyboard Visual Mode — target: 100%

Visual mode should feel deterministic and Vim-like.

- [x] Implement `v` with anchor/cursor selection.
- [x] Implement `V` as linewise selection.
- [x] Keep split visual selections side-filtered.
- [x] Fix one-character-left visual selection paint bug.
- [x] Add renderer test for split selection paint position.
- [x] Add tests for side-filtered visual selection.
- [x] Add test for `v + gg`.
- [x] Add test for visual-line motion on one side.
- [x] Add tests for `v + h` reverse selection.
- [x] Add tests for `v + l` forward selection.
- [x] Add tests for `v + G` with side filtering.
- [x] Add tests for `v + 0` and `v + $`.
- [x] Add tests for visual selection after `/`, `n`, and `N`.
- [x] Add tests for yanking visual selections.
- [ ] Validate visual behavior interactively in `target/dev-fast/lazydiff`.

## 4. Search — target: 100%

Search should be viewer-owned and stable across layout/mode changes.

- [x] Move search query/matches/index into `DiffViewerState`.
- [x] Move search recomputation into `DiffViewerState`.
- [x] Move search navigation into `DiffViewerState`.
- [x] Make search landing set cursor to exact match column.
- [ ] Make search column/range math fully display-cell/grapheme aware.
- [x] Recompute or invalidate search safely when layout mode changes.
- [x] Keep visual selection updated when search moves cursor in visual mode.
- [x] Add tests for `/`, `n`, `N` exact cursor landing.
- [x] Add tests for search in split left/right sides.
- [x] Add tests for search while visual selection is active.

## 5. Text Objects — target: 90–100%

Implement text-object algorithms for Vim-like selection power.

- [x] Add first-pass `iw`.
- [x] Add first-pass `aw`.
- [x] Add same-line delimiter objects for brackets/quotes.
- [x] Wire `i` / `a` pending text-object actions to viewer selection.
- [ ] Add `textObjectSearchBounds`-style search bounds.
- [ ] Port `findDelimitedTextObject`.
- [ ] Port `findBracketTextObject`.
- [ ] Port `findQuoteTextObject`.
- [ ] Port `advanceTextObjectPosition`.
- [ ] Port `previousTextObjectPosition`.
- [ ] Support nested delimiters.
- [ ] Support multi-line delimiter objects.
- [ ] Support newline inclusion flags for inner/around multi-line objects.
- [ ] Preserve side-filtered behavior for text objects in split view.
- [x] Add tests for `iw`, `aw`.
- [x] Add tests for `i(`, `a(`, `i{`, `a{`, `i[`, `a[`.
- [x] Add tests for quotes.
- [ ] Add tests for nested and multi-line delimiter objects.

## 6. Mouse Selection — target: 80–100%

Mouse selection should use the same viewer selection model as keyboard visual mode.

- [x] Add viewer-owned `selectionPoint(mouse)`.
- [x] Port mouse document-column mapping.
- [x] Port side-by-side mouse cell mapping.
- [x] Port drag anchor behavior.
- [x] Port drag extension behavior.
- [ ] Port extend-selection-after-scroll behavior.
- [x] Port finish-selection behavior.
- [x] Route mouse selection through `DiffViewerState.selection`.
- [x] Remove old mouse-selection helper semantics from diff keyboard paths.
- [x] Add tests or focused integration checks for mouse-to-document-column mapping.

## 7. Inline Comments and Editor Rows — target: 80–100%

Implement inline review UX while preserving lazydiff persistence/review data.

- [x] Represent comments/drafts as visual rows below target diff rows.
- [x] Render comment boxes inline in the diff viewport.
- [x] Render inline comment editor as a visual row block.
- [x] Include comment/editor rows in scroll height calculations.
- [x] Include comment/editor rows in cursor visibility calculations.
- [x] Preserve existing review persistence and GitHub/local review data model.
- [x] Remove bottom comment preview and normal-flow comment/thread modals in favor of inline rows.
- [x] Inline blocks carry review accent metadata for note/question/instruction/agent/draft highlight styling.
- [ ] Port comment editor normal/insert modes.
- [ ] Port comment editor cursor movement.
- [ ] Port comment editor text selection.
- [ ] Port submit/cancel behavior.
- [ ] Add tests or interactive checklists for inline comment layout.

## 8. Renderer Architecture Contract — target: ~80%, not exact code parity

Do not replace lazydiff's beautiful renderer. Match the viewer-owned layout contract while preserving rendering output.

- [x] Keep `DiffDocument` row cache.
- [x] Keep syntax highlighting.
- [x] Keep inline diff spans.
- [x] Keep Pierre render spans/styles.
- [x] Keep current themes/colors.
- [x] Add document-column paint range support.
- [x] Make viewer produce visual rows / paint specs.
  - [x] Viewer produces document-backed visual rows.
  - [x] Viewer produces renderer-ready overlay paint specs.
- [x] Make renderer consume viewer visual rows / paint specs.
  - [x] Renderer consumes viewer visual rows for document rows.
  - [x] Renderer consumes inline visual rows.
  - [x] Renderer consumes overlay paint specs.
- [x] Decouple renderer from old selected-row assumptions.
- [x] Add renderer tests for selection/search paint positions.
- [x] Add renderer tests for split left/right side filtering.
- [ ] Add renderer tests for comments/editor visual rows once added.

## 9. Help Overlay and UX Polish — target: 60–80%

Useful polish after correctness foundations.

- [x] Add diff-specific `?` help overlay.
- [x] Close help with `?`, `q`, or `Esc`.
- [x] Add keybinding list matching supported motions/actions.
- [x] Add yank flash state (`yankSelection`, `yankUntil`).
- [x] Paint yank flash via selection paint path.
- [x] Add status messages for missing search/text objects/notes.

## 10. Review Draft Targeting from Selection — target: 80–100%

Selection-aware comments should use the viewer's range model.

- [x] Compute review draft target from `DiffTextSelection`.
- [x] Preserve side-filtered left/right selection semantics.
- [ ] Detect overlap with existing review drafts.
- [x] Support single-line and multi-line selections.
- [ ] Add tests for right-only and left-only selected ranges.

## 11. Verification and Regression Suite

Use tests to stop interaction regressions.

- [x] `cargo test -p lazydiff-diffs selection_text` passes.
- [x] `cargo test -p lazydiff-diffs viewer` passes.
- [x] Renderer-level selection paint position test added.
- [x] `cargo build --profile dev-fast` passes.
- [x] Add test for `v+h`.
- [x] Add test for `v+l`.
- [x] Add test for `v+gg` from multiple positions.
- [x] Add test for `v+G` from multiple positions.
- [x] Add test for `V+j/k`.
- [x] Add test for `0/$` in visual mode.
- [x] Add test for `Ctrl-d/u` cursor centering.
- [x] Add test for split side crossing with `h/l`.
- [x] Add test for split vertical side preservation in uneven hunks.
- [x] Add test for `n/N` exact search landing.
- [x] Add tests for all supported text objects.
- [ ] Run interactive binary after major slices:

```sh
cargo build --profile dev-fast
LAZYDIFF_KEY_DEBUG=1 LAZYDIFF_THEME=default-dark ./target/dev-fast/lazydiff --branch
```

## Definition of Done

- [x] Diff interaction state is owned by `DiffViewerState` without old adapter semantics.
- [ ] Keyboard visual mode is reliable for all supported motions.
- [ ] Split view navigation is stable and side-filtered by design.
- [ ] Search is viewer-owned and exact-column correct.
- [ ] Text objects are complete or explicitly documented as unsupported.
- [ ] Mouse selection uses the same selection model as keyboard visual mode.
- [ ] Inline comments/editor participate in visual row layout.
- [ ] Renderer keeps lazydiff's current visual quality while consuming viewer-owned layout state.
- [ ] Regression tests cover the interaction paths users rely on.
