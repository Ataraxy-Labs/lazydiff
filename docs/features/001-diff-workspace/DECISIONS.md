# Feature 001 — in-flight deviation log

This file captures **agent deviations from `plan.md` / `RULES.md` / ADRs** that are too small to deserve an ADR amendment but too cross-cutting to live in a single issue's `notes`. It is append-only and dated. Future sessions read this to know what was changed and why without re-deriving it from `git log`.

If a deviation here grows up — recurs across slices, conflicts with an ADR, changes the contribution model itself, or creates a stability promise outside this feature — promote it to an ADR amendment and remove it from this file with a one-line pointer. (Inside this feature, contribution trait *signatures* can still change; that's expected per `spec.md` "Out of scope" #2.)

## Format

```
- YYYY-MM-DD — <one-line what changed> — <one-line why> — <issue id or "no issue">
```

Keep entries terse. Anything longer than two lines belongs in an ADR or a child issue.

## Entries

<!-- newest first; agents append above this comment -->

- 2026-05-28 — Feature 001 superseded by feature 002 clean rewrite — in-place migration was adding adapter glue faster than deleting legacy app architecture; ADR 0009 is now active — no issue
