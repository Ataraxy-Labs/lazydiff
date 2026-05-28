# LazyDiff migration playbook

This is the agent's operating playbook for the in-flight Diff Workspace migration and the follow-on whole-TUI work. It is referenced from `AGENTS.md` (which routes new sessions to the active feature folder) and should be re-read at the end of every slice.

For canonical vocabulary, see `CONTEXT.md`. For the always-on mission, see `docs/NORTH_STAR.md`. For accepted decisions, see `docs/adr/0001`…`0008`. For TUI verification, see `docs/TUI_VERIFICATION.md`. For the active work list, see `./issues.json` (or `bash scripts/work.sh next`). For this feature's plan checklist, see `./plan.md`. For this feature's framing, see `./spec.md`.

## Where to learn the architecture before you touch code

Before making architecture-shaped changes, read in this order:

0. `docs/NORTH_STAR.md` — mission, preserved strengths, bug classes we exist to kill, architecture invariants, proof-of-architecture features, and the end-of-slice done-check.
1. `CONTEXT.md` — canonical vocabulary.
2. `docs/adr/0001`…`docs/adr/0008` — accepted decisions.
3. `docs/research/synthesis.md` — one-page map of how external research translates into LazyDiff's architecture.
4. `docs/research/{prosemirror,xstate,pi-mono,pierre}.md` — depth on each source's specific lessons. `docs/research/rust-modules-and-visibility.md` covers Rust crate/module/visibility basics.
5. `docs/learning/ownership-walkthrough.md` — guided product-flow walkthrough of why Rust ownership matters for LazyDiff's bug classes.
6. `./plan.md` — this feature's migration checklist and compulsory order.
7. `docs/TUI_VERIFICATION.md` — how to verify TUI changes (Modes A/B/C; Mode B / termwright is the default).

If your intended change contradicts an ADR/invariant or changes product, ownership, persistence, rendering, event-loop, or UX policy, ask the human owner one focused question before changing direction.

## Diff Workspace architecture (the load-bearing bits)

- One mutable owner for diff-screen interaction state. State changes go through `update(intent) -> Vec<Effect>`; no public mutable fields on the workspace.
- App doesn't mutate cursor/scroll/selection/inline-focus/draft-editor/thread-expansion/mouse — those live in the workspace.
- Renderer consumes workspace-produced visual rows, decorations, overlays, view models. It doesn't mutate interaction state.
- IO (persistence, clipboard, navigation, review submission) is an explicit `Effect` the app shell executes.
- Clean core first; temporary adapter only as a bridge — don't add new behavior to adapters or copy scattered patterns into the new core.

## Suggested checks during diff-workspace work

Run focused searches to avoid reintroducing scattered ownership:

```sh
rg "viewer_mut\(\)|inline_focus|comment_modal" src/app.rs src/app
rg "row_count_for_mode|visual_rows_with_inline_blocks" src/app.rs src/render src/diff_render
```

Existing hits may remain during migration, but new work should reduce or isolate them rather than increase them.

## What counts as a reviewable behavior-migration slice

A *behavior-migration* slice is reviewable if all five hold:

1. It moves **one concept** (e.g., inline focus, mouse drag, search state) — not three at once.
2. It **deletes the old field/path** the concept used to live in; no leaving a dead duplicate behind.
3. It **adds a workspace operation** that the new code calls; no direct field pokes.
4. It adds **focused tests** for the moved concept. For new behavior or bug fixes, write a failing termwright test first (`docs/TUI_VERIFICATION.md`). For behavior-preserving refactors, ensure a characterization test passes before and after, and add it to `scripts/tui-verify.sh` if it's not already there.
5. It includes a **grep check showing the old pattern decreased**.

For behavior migrations, doing only one of these without the others is a patch fix, not a complete slice. Patch fixes are how the codebase got into its current state — do not add more.

**Scaffold, test-harness, docs, or pure setup slices** follow their issue's acceptance criteria and verification command. They should not migrate behavior opportunistically, and they don't need to delete an old path or reduce grep counts until a behavior actually moves.

## Quality nudges

- Event-driven redraw by default; no continuous redraw loops for static screens.
- Regression tests for bug-prone behavior (visual-row nav, side-filtered selection, search landing, inline editor movement, mouse, fold toggle) — write the failing termwright first.
- If a decision would surprise a future maintainer, document the *why* in `DECISIONS.md` or an ADR amendment, not just the *what* in code.

## Operating rule — finishing the work

This section governs how the agent finishes work. It is not optional.

### End-of-slice done-check

After finishing any reviewable slice (per the five-rule definition above):

1. Re-read `docs/NORTH_STAR.md` and answer all five done-check questions there. If any answer is "no" or "yes, oversight," fix it before continuing.
2. Run the grep checks listed above and any grep gate the slice's issue specifies. For behavior-migration slices, confirm legacy counts went down (or the remaining hits are more isolated). For scaffold/setup slices, record the baseline counts in the commit body instead.
3. Run the slice's `verification` command (typically `cargo test -p <crate> <focus>` and/or `cargo build --profile dev-fast`). Quote the result in the chat update.
4. **If the slice touched TUI behavior, run Mode B from `docs/TUI_VERIFICATION.md`.** Compile-only is not sufficient. For new behavior / bug fixes, the slice ships a `scripts/test-<slice>-termwright.sh` that fails on the current code and passes after. For behavior-preserving refactors, a characterization test that passes before and after is fine. Either way, include it in `scripts/tui-verify.sh` and confirm `bash scripts/tui-verify.sh` passes every suite.
5. Update `./issues.json` via `bash scripts/work.sh tick <issue.criterion>` for each acceptance criterion completed, and `bash scripts/work.sh done <issue>` only when all of that issue's criteria are ticked AND verification ran.
6. If the slice surfaced work that doesn't belong in this issue, file a child issue with `bash scripts/work.sh add-child <parent_id> "<title>"`. Do not silently expand the current slice.
7. Commit immediately after the slice passes the done-check. One slice = one commit.

### Detailed commits per task

- Use one commit per finished issue (or per logically-coherent sub-step inside an issue, when the sub-step is independently revertible).
- Commit subject line: `<area>: <what changed in one line>` (e.g. `workspace: move inline draft editor into DiffWorkspace modal enum`).
- Commit body must include:
  - **Issue id**: which issue in `./issues.json` this closes or advances.
  - **What ownership improved**: which field or path is now private to the right owner.
  - **What old mutation disappeared**: the deleted `App` field, the deleted scatter, the deleted parallel computation.
  - **Test/check that protects it**: the test name(s) and any grep gate result.
  - **North-Star check**: one-line note on which invariant or proof-of-architecture feature this slice moved forward, and confirmation that the done-check answers all pass.
- Local commits are expected after each completed slice (one slice = one commit). Run `git push` only when explicitly approved by the user.
- Never amend a published commit and never `git push --force`. If a slice was wrong, file a child issue and ship a corrective slice.

### When the agent stops

Stop only when one of these is true:

1. All compulsory items in `./plan.md` are checked, AND every open issue in `./issues.json` is `done` or `blocked` with a recorded reason.
2. A HITL decision blocks progress: file or update the issue with the precise question for the human and stop.
3. A verification failure cannot be resolved within the slice's scope: file a child issue documenting the failure, leave the worktree in a state the human can read, and stop.

Otherwise, the agent **does not stop**. The next action is:

```sh
bash scripts/work.sh next
```

Take the returned issue, repeat the slice loop. Do not declare victory because the previous slice felt large, because tests in *this* slice passed, or because the conversation has been long. Those are not stop conditions.

This rule exists because of the failure mode named in the Anthropic long-running-harness paper (Nov 2025): agents tend to declare partial progress as completion. The remedy is an externalized work list (`./issues.json`), explicit done-checks, and a single trivial "what's next" command. Use them.

### Before final response of any turn

The final response of any turn must include:

- Completed work this turn (issues ticked or closed, with ids).
- Verification quoted or summarized faithfully (test pass/fail, grep counts before/after).
- Next unchecked compulsory `./plan.md` item AND next `bash scripts/work.sh next` result.
- Any HITL question that blocks further progress, stated precisely.

Per the `./plan.md` operating rule, re-read `./plan.md` itself before composing this section; if any compulsory item is still unchecked and unblocked, the agent has not finished its turn.
