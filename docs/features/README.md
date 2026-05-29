# docs/features/ — per-feature folders

This directory holds **in-flight and historical feature folders**, one folder per feature. Each folder is self-contained: it carries its own spec, plan, rules, issues, and feature-local README. The contents of a feature folder live and die with that feature; global, always-on docs (NORTH_STAR, CONTEXT, ADRs, research, learning, TUI_VERIFICATION) stay at `docs/` root.

This shape mirrors current (2026) AI-assisted-engineering practice: always-on rules stay minimal and global; per-change specs, plans, and tasks live in a feature folder so they can be created, executed, and archived without polluting the always-on context.

## Folder layout (per feature)

```
docs/features/<NNN>-<slug>/
  ├─ spec.md         # what this feature is, why, scope, non-goals, success criteria
  ├─ plan.md         # compulsory checklist + operating rule for this feature
  ├─ RULES.md        # migration playbook: slice rule, done-check, commit format, stop conditions
  ├─ issues.json     # work-tracker tickets (driven via `bash scripts/work.sh`)
  ├─ DECISIONS.md    # append-only deviation log (cross-slice, not-ADR-worthy decisions)
  └─ README.md       # how the tracker fits the operating rule; per-slice TDD loop
```

## Active feature

| # | Slug | Status | Purpose |
|---|---|---|---|
| 001 | [`001-diff-workspace`](001-diff-workspace/) | superseded | Historical in-place migration attempt. Keep as reference/parity evidence for bug classes and tests. |
| 002 | [`002-clean-tui-rewrite`](002-clean-tui-rewrite/) | active | Build a clean isolated v2 TUI and v2 diff foundation, then layer app surfaces. |

To switch which feature `scripts/work.sh` operates on:

```sh
LAZYDIFF_FEATURE=002-some-feature bash scripts/work.sh next
```

Default is `002-clean-tui-rewrite`.

## When to start a new feature folder

- The current feature's `plan.md` compulsory checklist is fully checked, and its `issues.json` is fully closed or deferred.
- OR the new feature is genuinely independent (different surface, different invariants) and can run in parallel without touching the same code paths.

Do not split a feature mid-flight. Patch fixes belong in the current feature's `issues.json` as child issues.
