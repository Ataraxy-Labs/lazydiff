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
7. `docs/TUI_VERIFICATION.md` — how the agent proves a TUI slice works. Three modes (compile-only, headless termwright, live tmux). Mode B (termwright) is the agent's default and produces committed regression tests.

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

## Operating rule — finishing the work

This section governs how the agent finishes work. It is not optional. Re-read it before claiming any slice is done.

### End-of-slice done-check

After finishing any reviewable slice (per the five-rule definition above):

1. Re-read `docs/NORTH_STAR.md` and answer all five done-check questions there. If any answer is "no" or "yes, oversight," fix it before continuing.
2. Run the grep checks listed under "Suggested checks during diff-workspace work" and any grep gate the slice's issue specifies. Confirm the legacy counts went down, not up.
3. Run the slice's `verification` command (typically `cargo test -p <crate> <focus>` and/or `cargo build --profile dev-fast`). Quote the result in the chat update.
4. **If the slice touched TUI behavior, run Mode B from `docs/TUI_VERIFICATION.md`.** Compile-only is not sufficient. The slice must ship a `scripts/test-<slice>-termwright.sh` that fails on the current code, passes after the slice, and is included in `scripts/tui-verify.sh`. Run `bash scripts/tui-verify.sh` and confirm every suite passes.
5. Update `docs/work/issues.json` via `bash scripts/work.sh tick <issue.criterion>` for each acceptance criterion completed, and `bash scripts/work.sh done <issue>` only when all of that issue's criteria are ticked AND verification ran.
6. If the slice surfaced work that doesn't belong in this issue, file a child issue with `bash scripts/work.sh add-child <parent_id> "<title>"`. Do not silently expand the current slice.
7. Commit immediately after the slice passes the done-check. One slice = one commit (see "Detailed commits" below).

### Detailed commits per task

- Use one commit per finished issue (or per logically-coherent sub-step inside an issue, when the sub-step is independently revertible).
- Commit subject line: `<area>: <what changed in one line>` (e.g. `workspace: move inline draft editor into DiffWorkspace modal enum`).
- Commit body must include:
  - **Issue id**: which issue in `docs/work/issues.json` this closes or advances.
  - **What ownership improved**: which field or path is now private to the right owner.
  - **What old mutation disappeared**: the deleted `App` field, the deleted scatter, the deleted parallel computation.
  - **Test/check that protects it**: the test name(s) and any grep gate result.
  - **North-Star check**: one-line note on which invariant or proof-of-architecture feature this slice moved forward, and confirmation that the four done-check answers all pass.
- Never amend a published commit and never `git push --force`. If a slice was wrong, file a child issue and ship a corrective slice.
- Run `git commit` and `git push` only when explicitly approved by the user, per the global instruction. The default is local commits.

### When the agent stops

Stop only when one of these is true:

1. All compulsory items in `plan.md` are checked, AND every open issue in `docs/work/issues.json` is `done` or `blocked` with a recorded reason.
2. A HITL decision blocks progress: file or update the issue with the precise question for the human and stop.
3. A verification failure cannot be resolved within the slice's scope: file a child issue documenting the failure, leave the worktree in a state the human can read, and stop.

Otherwise, the agent **does not stop**. The next action is:

```sh
bash scripts/work.sh next
```

Take the returned issue, repeat the slice loop. Do not declare victory because the previous slice felt large, because tests in *this* slice passed, or because the conversation has been long. Those are not stop conditions.

This rule exists because of the failure mode named in the Anthropic long-running-harness paper (Nov 2025): agents tend to declare partial progress as completion. The remedy is an externalized work list (`docs/work/issues.json`), explicit done-checks, and a single trivial "what's next" command. Use them.

### Before final response of any turn

The final response of any turn must include:

- Completed work this turn (issues ticked or closed, with ids).
- Verification quoted or summarized faithfully (test pass/fail, grep counts before/after).
- Next unchecked compulsory `plan.md` item AND next `bash scripts/work.sh next` result.
- Any HITL question that blocks further progress, stated precisely.

Per `plan.md` operating rule, re-read `plan.md` itself before composing this section; if any compulsory item is still unchecked and unblocked, the agent has not finished its turn.
