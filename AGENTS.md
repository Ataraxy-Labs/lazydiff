# AGENTS.md — start here

LazyDiff is a terminal-first code-review workspace. This file is the entry point for any agent or tool that needs to make changes here. It is intentionally short; the detail lives in the docs it points to.

## How to work in this repo

- **Humans own product and architecture decisions. Agents execute slices.** If something would surprise the docs below, ask one focused question and wait.
- **One reviewable slice at a time.** Slice rules and the end-of-slice done-check live in `docs/MIGRATION.md`.
- **Before starting work:** `bash scripts/work.sh next` returns the next unblocked issue from `docs/work/issues.json`.
- **Before claiming done:** re-read `docs/NORTH_STAR.md`, run the slice's verification (incl. `bash scripts/tui-verify.sh` for any TUI-observable change), and commit per `docs/MIGRATION.md` → "Detailed commits per task."
- **Do not stop on partial progress.** Stop conditions are in `docs/MIGRATION.md` → "When the agent stops."

## Canonical docs

| Doc | What it holds |
|---|---|
| `docs/NORTH_STAR.md` | Mission, preserved strengths, bug classes we exist to kill, architecture invariants, proof-of-architecture features, end-of-slice done-check. Re-read at the end of every slice. |
| `CONTEXT.md` | Canonical vocabulary (Diff Workspace, Surface Owner, Visual-Row Stream, Fold, FoldStrategy, Generation Token, Contribution, Chrome Slot, …). |
| `docs/adr/0001`…`0008` | Accepted architecture decisions. |
| `docs/MIGRATION.md` | Migration playbook: slice rule, grep checks, engineering quality bar, end-of-slice done-check, detailed commit format, stop conditions. |
| `docs/TUI_VERIFICATION.md` | How to verify TUI behavior. Three modes (compile-only, headless termwright, live tmux). Mode B is the default. |
| `docs/research/` | Why each external pattern (ProseMirror, XState, pi-mono, pierre); cached-checkout paths and refresh commands. Ground every external-pattern claim in the cached source. |
| `docs/learning/` | Long-form learning material for newer-to-Rust contributors. |
| `plan.md` | Current migration checklist and compulsory item order. |
| `docs/work/issues.json` + `scripts/work.sh` | Active work list and CLI. |

## Hard rules

- Do not silently change architecture, persistence, rendering, event-loop, or UX policy. Ask the human owner.
- Do not add `App`-side mutation of any surface's private state. State changes go through `update(intent) -> Vec<Effect>` only.
- Do not claim an external-pattern behavior without reading the cached source at `~/.cache/checkouts/`. Refresh missing paths via `bash ~/.agents/skills/librarian/checkout.sh <owner>/<repo> --path-only`.
- Do not run `git commit --amend`, `git push`, or `git push --force` unless explicitly told.
- Do not call compile-only verification "done" for a TUI-observable slice. See `docs/TUI_VERIFICATION.md`.
