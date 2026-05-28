# docs/work/ — the work list

This folder is the externalized issue tracker for LazyDiff. It is intentionally just a JSON file and a small bash script, not a service.

## Files

- `issues.json` — the single source of truth for all work items. Schema is documented at the top of the file.
- `../../scripts/work.sh` — the CLI for adding, ticking, and closing issues. Run `bash scripts/work.sh help` from the repo root.

## Why JSON instead of GitHub Issues or SQLite

- **Git-diffable**: issue edits show up in `git log` and PRs alongside the code that closes them.
- **No tool dependency**: humans read it, agents read it, `jq` and `rg` query it. SQLite needs `sqlite3` and a schema; GitHub Issues needs the network.
- **No migrations**: fields evolve by adding keys; old issues stay valid.
- **Agent-friendly**: any sub-agent can `Read` and `edit_file` the JSON without a query layer.

When the work list outgrows ~200 active items or we need concurrent multi-machine writes, revisit. Until then, this is enough.

## How it fits the operating rule

`AGENTS.md` "Operating rule — finishing the work" mandates that the agent:

1. Runs `bash scripts/work.sh next` to find the next unblocked open issue.
2. Runs the slice (per the five-rule reviewable-slice definition).
3. Ticks acceptance criteria with `bash scripts/work.sh tick <id>.<n>`.
4. Closes the issue with `bash scripts/work.sh done <id>` only when all criteria are ticked AND verification ran.
5. Files child issues for surfaced work with `bash scripts/work.sh add-child <parent_id> "<title>"`.
6. Commits per the "Detailed commits" rules in AGENTS.md.
7. Does not stop unless one of the three stop conditions in AGENTS.md is true.

## Anatomy of one issue

Every issue carries a `north_star_check` field — a behavioral question the agent answers at the end of the slice (see `docs/NORTH_STAR.md`). This makes the goal travel with the work item; a sub-agent picking up the issue mid-stream re-anchors on the same invariants the parent did.

## TUI slices require a termwright test (Mode B)

If an issue's behavior is observable in the TUI (cursor, scroll, selection, search, inline rows, folds, mouse, side filtering, modal subflows, chrome, palette, fold-summary rendering), the slice **must** ship a `scripts/test-<slice>-termwright.sh` regression test per `docs/TUI_VERIFICATION.md`.

The TDD loop per slice:

1. Read the issue's `acceptance_criteria` and `north_star_check`.
2. Write `scripts/test-<slice>-termwright.sh` asserting the new behavior. Run it. It must fail or error on current code.
3. Implement the slice per the ADRs.
4. Re-run the new test until it passes.
5. Run `bash scripts/tui-verify.sh` and confirm every suite still passes.
6. `chmod +x` the new test so it joins the regression suite forever.

The issue's `verification` field should call `bash scripts/tui-verify.sh` (or the slice-specific test) for TUI slices; `cargo test ...` alone is not sufficient for anything TUI-observable. See `AGENTS.md` end-of-slice done-check rule #4 and `docs/NORTH_STAR.md` done-check question #5.
