# LazyDiff agent guidance

## Product ambition and decision quality

- Treat LazyDiff as a serious long-term product, not a demo or generated prototype. Optimize for maintainability, correctness, and reviewer trust.
- Agents may write code, docs, tests, and migration mechanics, but product and architecture decisions are human-owned. Use agent speed for execution; use human judgment for direction.
- For architectural decisions, ask the human owner one focused question at a time and explain the trade-off before changing direction.
- Do not silently introduce major architecture, persistence, rendering, event-loop, or UX policy changes. Make the decision explicit in chat and document accepted decisions in `CONTEXT.md`, `plan.md`, or `docs/adr/` as appropriate.
- Prefer thoughtful, small, reviewable changes over broad rewrites. Every migration slice should make ownership clearer or reduce a known class of bugs.

## Where to learn the architecture before you touch code

Before making architecture-shaped changes, read in this order:

0. `docs/NORTH_STAR.md` — the always-on mission, preserved strengths, bug classes we exist to kill, architecture invariants, proof-of-architecture features, and the agent's end-of-slice done-check. Re-read at the end of every reviewable slice.
1. `CONTEXT.md` — canonical vocabulary (Diff Workspace, Surface Owner, Visual-Row Stream, Fold, FoldStrategy, Generation Token, Contribution, Chrome Slot, …).
2. `docs/adr/0001` … `docs/adr/0008` — accepted decisions:
   - `0001` Diff Workspace + generic decorations.
   - `0002` Extension-shaped internals before public plugin API.
   - `0003` Reducer-first surface update loop.
   - `0004` Rust-owned Diff Workspace state.
   - `0005` Unified Visual-Row Stream with dirty cache.
   - `0006` App shell / router + Surface Owners (whole-TUI shape).
   - `0007` Async effects + generation tokens.
   - `0008` Commands, keymaps, and chrome contribution points.
3. `docs/research/synthesis.md` — one-page map of how external research (ProseMirror, XState, pi-mono, pierre) translates into LazyDiff's architecture.
4. `docs/research/{prosemirror,xstate,pi-mono,pierre}.md` — depth on each source's specific lessons. `docs/research/rust-modules-and-visibility.md` covers Rust crate/module/visibility basics for newer-to-Rust contributors.
5. `docs/learning/ownership-walkthrough.md` — guided product-flow walkthrough of why Rust ownership matters for LazyDiff's bug classes.
6. `plan.md` — current migration checklist, operating rule, and whole-TUI follow-on slices.

If a request would surprise any of the above, stop and ask the human owner before changing direction.

### Going deeper: read the cached reference source

When you need more depth on an external pattern that LazyDiff borrows from — ProseMirror decorations/commands/keymaps, XState's `@xstate/store` vs `fromTransition` vs statecharts split, pi-mono's capability contexts and extension surface, Pierre's Virtualizer / sparse height cache / coalesced rendering — read the actual source. The canonical copies are cached at:

```
~/.cache/checkouts/github.com/ProseMirror/{prosemirror,prosemirror-state,prosemirror-view,prosemirror-commands,prosemirror-keymap,prosemirror-history}
~/.cache/checkouts/github.com/statelyai/xstate
~/.cache/checkouts/github.com/badlogic/pi-mono
~/.cache/checkouts/github.com/pierrecomputer/pierre
```

Refresh missing/stale paths with the `librarian` skill:

```sh
bash ~/.agents/skills/librarian/checkout.sh <owner>/<repo> --path-only
```

**Ground every external-pattern claim in the cached source.** Do not infer behavior from memory or paraphrased prior conversation. If a claim ends up in `docs/research/*.md`, `docs/adr/*.md`, `CONTEXT.md`, or `docs/NORTH_STAR.md`, the agent making the claim must have read the relevant cached source.

## Diff Workspace architecture

- Use `CONTEXT.md` for canonical language. Prefer **Diff Workspace**, **Visual Row**, **Inline Review Row**, **Diff Decoration**, and **Diff Workspace Owner** as defined there.
- Treat Rust ownership as an architectural boundary: diff-screen interaction state should have one mutable owner.
- Do not add new `App`-side mutation of cursor, scroll, selection, inline review focus, draft editor focus, thread expansion/focus, or mouse selection.
- Do not expose public mutable fields from the Diff Workspace owner. Prefer semantic operations such as user-meaningful intents/methods that update related state together.
- Renderer code should consume workspace-produced visual rows, decorations, overlays, and view models; it should not mutate interaction state.
- Product IO such as persistence, clipboard, external navigation, and review submission should be requested as explicit effects and executed by the app shell.

## Migration rule

- Build the clean `DiffWorkspace` core first, with private state and correct APIs.
- Temporary adapter code is allowed only to bridge legacy `App` paths into the clean workspace API.
- Do not add new behavior to adapter code or copy existing scattered patterns into the new core.
- If you feel tempted to call `viewer_mut()` from `App`, stop and move the operation into the Diff Workspace owner instead.

## Suggested checks during diff-workspace work

Run focused searches to avoid reintroducing scattered ownership:

```sh
rg "viewer_mut\(\)|inline_focus|comment_modal" src/app.rs src/app
```

Existing hits may remain during migration, but new work should reduce or isolate them rather than increase them.

## What counts as a reviewable migration slice

A diff-workspace migration slice is only reviewable if all five hold:

1. It moves **one concept** (e.g., inline focus, mouse drag, search state) — not three at once.
2. It **deletes the old field/path** the concept used to live in; no leaving a dead duplicate behind.
3. It **adds a workspace operation** that the new code calls; no direct field pokes.
4. It adds **focused tests** for the moved concept (unit on the workspace operation, or a renderer/integration test as appropriate).
5. It includes a **grep check showing the old pattern decreased**, e.g. `rg "viewer_mut\(\)|inline_focus|comment_modal" src/app.rs src/app` before/after counts.

A change that does any one of these without the others is a patch fix, not a slice. Patch fixes are how the codebase got into its current state — do not add more.

## Engineering quality bar

- Be honest and precise in public-facing claims. For TUI performance, prefer input-to-render latency, draw time, event coalescing, and idle redraw behavior over vague "FPS" claims unless a frame-driven loop is truly intended.
- Keep the TUI event loop event-driven by default. Redraw because state changed, input arrived, async work progressed, or a bounded animation/timer requires it; do not add continuous redraw loops for static screens.
- Avoid giant unreviewable changes. Split migrations into narrow commits/PRs that state: what ownership improved, what old mutation disappeared, what tests protect it, and what grep/check shows progress.
- Add regression tests for bug-prone behavior instead of relying on manual confidence. Visual-row navigation, side-filtered selection, search landing, inline editor movement, mouse selection, and redraw behavior should become testable contracts.
- Treat docs as part of engineering quality. If a decision would surprise a future maintainer or agent, document the why, not just the what.
