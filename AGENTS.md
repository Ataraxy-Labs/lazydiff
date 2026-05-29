# AGENTS.md

LazyDiff is a terminal-first **"build your own diff / build your own code review"** workspace.

Architecturally it is a **pi-mono-style extensible Rust core**: one stable diff/review workspace with bounded contribution seams for custom commands, keymaps, inline rows, decorations, chrome slots, fold strategies, and review actions — without forking the core. ADR 0009 moves active work to a clean v2 rewrite; legacy app code is reference material, not the target architecture.

This file is the entry point. It is short on purpose. Read in order; consult specifics on demand.

## How to work here

- One reviewable slice at a time. `bash scripts/work.sh next` returns the next unblocked issue from the active feature's `issues.json`.
- Humans own product and architecture decisions. Agents execute slices. For architecture-shaped changes, ask one focused question and wait.
- Active rewrite work belongs in v2 modules/crates (`src/rewrite/`, `src/bin/lazydiff-v2.rs`, `crates/lazydiff-v2-diff`). Do not continue feature 001 or mutate legacy `src/app.rs` architecture unless explicitly asked.
- Refactors and rewrite slices should replace or delete old paths, not wrap them indefinitely. A scaffold slice may add a seam, but follow-up behavior slices should trend toward less code: remove duplicated state, parallel row math, adapter glue, and direct mutation as soon as the new owner covers the behavior.
- Use TDD for behavior. For new behavior or bug fixes, write the failing focused test first; for TUI-observable behavior, that means a termwright Mode-B test. Behavior-preserving refactors need a characterization test or the issue's verification command before and after.
- Don't stop on partial progress. Stop conditions live in the active feature's `RULES.md`.

## Reading order for a new session

1. `docs/NORTH_STAR.md` — mission, bug classes, architecture invariants, end-of-slice done-check.
2. `CONTEXT.md` — canonical vocabulary. Use these names.
3. `docs/adr/0001`…`0009` — accepted architecture decisions; 0009 makes the clean rewrite active.
4. `docs/research/synthesis.md` then per-source notes — *why* each external pattern shaped a decision.
5. `docs/learning/ownership-walkthrough.md` — Rust ownership as architecture boundary.
6. `docs/features/002-clean-tui-rewrite/` — active feature (`spec.md`, `plan.md`, `RULES.md`, `issues.json`, `DECISIONS.md`, `README.md`). Feature 001 is historical/reference.
7. `docs/TUI_VERIFICATION.md` — three verification modes; Mode B (termwright) is default for TUI-observable slices.

## Always-on docs

| Doc | What it holds |
|---|---|
| `docs/NORTH_STAR.md` | Mission, invariants, proof-of-architecture features, done-check. |
| `CONTEXT.md` | Canonical vocabulary. |
| `docs/adr/` | Accepted architecture decisions. |
| `docs/research/` | Why each external pattern; cached-checkout paths. |
| `docs/learning/` | Long-form learning for newer-to-Rust contributors. |
| `docs/TUI_VERIFICATION.md` | How to verify TUI behavior. |
| `scripts/work.sh`, `scripts/tui-verify.sh`, `scripts/dev-watch-tui.sh` | Tooling. |

## Active feature folder

| File | What it holds |
|---|---|
| `spec.md` | Why this feature exists, scope, success criteria. |
| `plan.md` | Compulsory checklist + operating rule. |
| `RULES.md` | Slice rule, done-check, commit format, stop conditions. |
| `issues.json` | Tickets driven via `bash scripts/work.sh`. |
| `DECISIONS.md` | Append-only deviation log. |
| `README.md` | Tracker + per-slice TDD loop. |

`scripts/work.sh` resolves the active feature from `$LAZYDIFF_FEATURE` (default `002-clean-tui-rewrite`). Index at `docs/features/README.md`.

## When you deviate from the plan

Record the call sized to its weight:

| Size | Where | How |
|---|---|---|
| In-slice (option B over A, renamed a helper, deferred a sub-goal) | issue `notes` | `bash scripts/work.sh note <id> "<one-liner>"` |
| Cross-slice, not ADR-worthy (renamed a trait, chose lib X, reordered slices) | active feature's `DECISIONS.md` | dated bullet at top: `- YYYY-MM-DD — <what> — <why> — <issue id>` |
| Architecture-shaped (new contribution kind, ownership change, persistence/event-loop policy) | `docs/adr/` amendment | **ask first**, then file |

A deviation is ADR-shaped if it changes an invariant, ownership boundary, the contribution model, or persistence/rendering/event-loop/UX policy — or if it will shape future features. Otherwise keep it terse in issue notes or `DECISIONS.md`.

## Guardrails (only the load-bearing ones)

- Ask before architecture-shaped changes: persistence policy, rendering pipeline, event loop, UX policy, public-plugin/runtime behavior, or **unplanned** contribution seams. Planned internal contribution work (per the active feature's `spec.md` and the ADRs) does not need to ask.
- TUI-observable slices need Mode B (`docs/TUI_VERIFICATION.md`). Compile-only is not proof.
- Git is local-only by default. No `git push`, no `--force`, no `--amend` on published commits unless told.

Everything else is your call — naming, ordering, micro-design. The ADRs and active feature's `RULES.md` are there to consult, not to memorize.
