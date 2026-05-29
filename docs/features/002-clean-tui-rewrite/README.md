# Feature 002 — Clean TUI Rewrite

This is the active work tracker for the clean v2 rewrite. It supersedes feature 001's in-place Diff Workspace migration.

Use:

```sh
bash scripts/work.sh next
```

Feature 002 is the default feature in `scripts/work.sh` and `AGENTS.md`.

The first goal is deliberately small: render a current diff in a new isolated TUI. Build the v2 diff foundation first, then add app layers.

Read `ARCHITECTURE.md` before implementing v2 modules. It contains the rewrite vocabulary, diagrams, display-map foundation, optional full-file context model, and pi-mono-shaped contribution seams attached to ADR 0009.
