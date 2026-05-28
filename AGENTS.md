# AGENTS.md — start here

LazyDiff is a terminal-first code-review workspace. This file is the entry point for any agent or tool that needs to make changes here. It is intentionally short; the detail lives in the docs it points to. Read it in order on your first turn of a new session.

## What this repo is and what state it's in (read first)

- LazyDiff is being migrated from a god-`App` struct with scattered mutation into a **whole-TUI Surface Owner** architecture, proving the pattern first with the **Diff Workspace** (the most state-heavy surface).
- The migration has **not started implementation yet**. The current sessions have only set up durable docs, vocabulary, ADRs, a per-feature work tracker, a TUI verification harness, and reference-repo caches.
- The next code work is gated on **feature 001 — `docs/features/001-diff-workspace/`** — `bash scripts/work.sh next` returns the first unblocked issue (currently `#1 Create DiffWorkspace module skeleton`).
- The currently active feature is named in `scripts/work.sh` via `$LAZYDIFF_FEATURE` (default `001-diff-workspace`).

## How to work in this repo

- **Humans own product and architecture decisions. Agents execute slices.** If something would surprise the docs below, ask one focused question and wait.
- **One reviewable slice at a time.** Slice rules and the end-of-slice done-check live in the active feature's `RULES.md` (`docs/features/001-diff-workspace/RULES.md`).
- **Before starting work:** `bash scripts/work.sh next` returns the next unblocked issue from the active feature's `issues.json`.
- **Before claiming done:** re-read `docs/NORTH_STAR.md`, run the slice's verification (incl. `bash scripts/tui-verify.sh` for any TUI-observable change), and commit per the active feature's `RULES.md` → "Detailed commits per task."
- **Do not stop on partial progress.** Stop conditions are in the active feature's `RULES.md` → "When the agent stops."

## New-session reading order

A fresh agent session must read these before touching code, in this order, so it inherits the *why* and the constraints — not just the *what*:

1. `docs/NORTH_STAR.md` — mission, preserved strengths, bug classes we exist to kill, architecture invariants, end-of-slice done-check.
2. `CONTEXT.md` — canonical vocabulary (Diff Workspace, Visual-Row Stream, Fold, FoldStrategy, Surface Owner, Contribution, Chrome Slot, Generation Token, …). Use these names everywhere.
3. `docs/adr/0001`…`0008` — accepted architecture decisions. Do not silently violate them.
4. `docs/research/synthesis.md` then `docs/research/{prosemirror,xstate,pi-mono,pierre,rust-modules-and-visibility}.md` — *why* each external pattern shaped a decision. Ground every external-pattern claim in the cached source at `~/.cache/checkouts/`.
5. `docs/learning/ownership-walkthrough.md` — guided product-flow walkthrough of why Rust ownership is the architectural boundary for LazyDiff's bug classes.
6. `docs/features/001-diff-workspace/spec.md` then `plan.md`, `RULES.md`, `issues.json` — the active feature's framing, checklist, playbook, and tickets.
7. `docs/TUI_VERIFICATION.md` — three verification modes; Mode B (headless termwright) is the default for any TUI-observable slice.

## Canonical docs (always-on; survive any feature)

| Doc | What it holds |
|---|---|
| `docs/NORTH_STAR.md` | Mission, preserved strengths, bug classes, architecture invariants, proof-of-architecture features, end-of-slice done-check. Re-read at the end of every slice. |
| `CONTEXT.md` | Canonical vocabulary. |
| `docs/adr/0001`…`0008` | Accepted architecture decisions. |
| `docs/research/` | Why each external pattern (ProseMirror, XState, pi-mono, pierre); cached-checkout paths and refresh commands. |
| `docs/learning/` | Long-form learning material for newer-to-Rust contributors. |
| `docs/TUI_VERIFICATION.md` | How to verify TUI behavior. |
| `scripts/work.sh`, `scripts/tui-verify.sh`, `scripts/dev-watch-tui.sh` | Tooling. |

## Active feature (the only code work in flight)

| File | What it holds |
|---|---|
| `docs/features/001-diff-workspace/spec.md` | Why this feature exists, scope, non-goals, success criteria. |
| `docs/features/001-diff-workspace/plan.md` | Compulsory checklist + operating rule; the source of truth for "is this feature done." |
| `docs/features/001-diff-workspace/RULES.md` | Migration playbook: slice rule, grep checks, engineering quality bar, end-of-slice done-check, detailed commit format, stop conditions. |
| `docs/features/001-diff-workspace/issues.json` | Tickets with acceptance criteria, verification, north-star check. Driven via `bash scripts/work.sh`. |
| `docs/features/001-diff-workspace/README.md` | How the tracker fits the operating rule and the per-slice TDD loop. |

Future features will live in sibling folders (`docs/features/002-…/`, etc.) following the same shape. Index: `docs/features/README.md`.

## Hard rules

- Do not silently change architecture, persistence, rendering, event-loop, or UX policy. Ask the human owner.
- Do not add `App`-side mutation of any surface's private state. State changes go through `update(intent) -> Vec<Effect>` only.
- Do not claim an external-pattern behavior without reading the cached source at `~/.cache/checkouts/`. Refresh missing paths via `bash ~/.agents/skills/librarian/checkout.sh <owner>/<repo> --path-only`.
- Do not run `git commit --amend`, `git push`, or `git push --force` unless explicitly told.
- Do not call compile-only verification "done" for a TUI-observable slice. See `docs/TUI_VERIFICATION.md`.
- Do not introduce a public plugin runtime (dynamic libs / WASM / scripting). Extensibility is internal contributions and compile-time Rust contributions only (ADR 0002).
