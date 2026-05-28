# LazyDiff North Star

This is the always-on goal doc. Re-read it at the end of every reviewable slice and answer the done-check below before claiming complete. Behavioral anchors only; abstract principles do not belong here.

## Mission (two-line)

LazyDiff is a terminal-first **"build your own diff / build your own code review"** workspace.
The deeper engineering goal is an **agent-friendly codebase**: most features should be addable as a single Contribution with a single intent, one tested file, and one obvious edit point — no app-wide surgery, no architecture archaeology, no repeating prior instructions to the agent.

## What we preserve (do not regress)

These are the parts of today's LazyDiff the user explicitly wants kept. Any slice that degrades one of these is not done.

1. Fast parsing and rendering core.
2. Pierre styling.
3. Syntax highlighting and inline diff spans.
4. Themes and colors, including the multi-layered **Quiver** theme system.
5. Side-filtered selection in split view (right-side selection never highlights left, and vice versa).
6. Side-local inline comment boxes (not full-width decorations).
7. Editor focus styling identical to unfocused; only text body + cursor edit, not title/header.
8. The renderer's visual quality. The renderer is not replaced; the architecture matches its contract.

## Bug classes we exist to kill

A slice that makes any of these easier to reintroduce is not done.

1. Patch-fix proliferation — small fixes scattered across `App` that each handle one symptom of the same underlying ownership bug.
2. Coordinate mismatches — selection or search highlights drifting because two code paths compute coordinates differently.
3. Off-by-one row indexing around inline rows — cursor sticks, jumps, or skips when inline blocks are present.
4. Duplicated layout logic — multiple consumers each rebuilding their own idea of "rows" or "row counts."
5. Stale async results — forge/persistence results landing on a record the user has already navigated away from.
6. Hidden modal mutation — `App` reaches into surface-private fields and changes related state piecemeal instead of as one intent.

## Architecture invariants (the non-negotiables)

Drawn from ADRs 0001–0008. If a slice breaks one of these, fix the slice; do not weaken the invariant.

1. **One Rust owner per surface.** State is private; the only door in is `update(intent) -> Vec<Effect>`. The app shell routes and runs effects; it never mutates surface state.
2. **`lazydiff-diffs` stays generic.** No product semantics leak in. Notes, comments, drafts, threads, review actions live above the core.
3. **One Visual-Row Stream per Diff Workspace.** All four consumers (navigation, scrolling, mouse mapping, renderer iteration) read the same cached, dirty-flag-rebuilt slice. Folds are first-class on the stream.
4. **Reducer + Effects.** No IO inside reducers. Async effects carry a `GenerationToken`; stale results are dropped.
5. **Contribution, not mutation.** Commands, keymaps, palette entries, inline rows, decorations, chrome slots, fold strategies are registered values. They receive read-only context and emit data or intents. They never receive `&mut App` or `&mut Surface`.
6. **No public plugin runtime in the first migration.** Internal contributions only; the registry exists, third-party loading does not.

## Proof-of-architecture features (the "did it work?" test)

If the architecture succeeded, a future contributor should be able to add **each of these** as one or two Contributions, without editing the workspace owner, renderer math, or coordinate code. If any of them still requires app-wide surgery, the migration is incomplete.

- Blame badges in the gutter.
- AI risk markers on changed lines.
- Test coverage hints per line.
- Resolved-thread ghosts (collapsed/dimmed historical threads).
- Conflict-region markers.
- User-defined fold strategies: auto-fold lockfiles and generated files, fold reformatting-only hunks, fold imports, fold whitespace-only changes, fold by AI-suggested risk.
- A new review-marker kind ("nit", "blocker", "question") with its own keybinding and chrome chip.
- A new command palette entry that runs a custom review action.
- A new status-line segment.

## Engineering honesty (how we talk about it)

- **TUI performance**: prefer concrete signals (input-to-render latency, draw time, event coalescing, idle redraw) over vague FPS claims when they aren't the real signal.
- **Public claims**: be precise about implemented vs. planned. ADRs are decisions; the active feature's `plan.md` is in-flight; NORTH_STAR is direction.
- **Reviewable slices**: one concept moved, old path deleted, new workspace operation added, focused tests, grep proof. Anything else is a patch fix.
- **Humans own product/architecture decisions**; agents execute. For architecture/persistence/rendering/event-loop/UX policy changes, ask one focused question and wait.

## The agent's done-check (run at end of every slice)

After finishing any slice, re-read this file and answer all five:

1. **Did this slice move toward a *simpler* codebase**, or did it add scatter — more fields on `App`, more parallel row computations, more direct mutation paths, more "patch fix" shaped code? If scatter increased, it is not done.
2. **Did the slice delete the old path it replaced?** A duplicate left behind for "safety" is a patch fix in disguise. Delete or document why it stayed (must be temporary adapter only).
3. **Could a future contributor add the equivalent feature as a Contribution** (Command, Keymap, Palette, InlineRow, Decoration, Chrome, FoldStrategy) instead of editing the surface owner? If no, is that a deliberate scope decision or an oversight?
4. **Did anything in the slice silently change persistence, rendering, event-loop, or UX policy?** If yes, stop — that is a human-owned decision and was not approved.
5. **If the slice touched TUI behavior, did I verify it via Mode B (termwright)** per `docs/TUI_VERIFICATION.md` — write the failing test first, make it pass, run `bash scripts/tui-verify.sh` clean? Compile-only is not enough. Watching tmux without committing a test is not enough. The slice must leave a regression test behind.

If any answer is "no" or "yes, oversight," the slice is not done. Fix it before ticking the issue, before committing, and before moving on. If you cannot fix it within scope, file a child issue (`work add-child <id> "..."`) and explicitly note the regression so it is not lost.

## When to stop

Stop only when one of:

- All compulsory items in the active feature's `plan.md` (under `docs/features/<feature>/`) are checked, AND all in-flight issues are done, blocked, or have explicit human follow-up filed.
- A human decision is blocking and you have recorded it as a HITL issue with the question stated.
- A verification failure that you have documented and surfaced for human resolution.

Otherwise: `bash scripts/work.sh next` and continue. Do not stop because the previous slice felt big. Do not declare victory because some things work. The Anthropic long-running-harness paper (Nov 2025) is explicit: agents that stop early on partial progress are the failure mode the harness exists to prevent.
