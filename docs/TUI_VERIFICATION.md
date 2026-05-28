# TUI verification — how the agent proves a slice works

LazyDiff is a TUI. Compile-only verification (`cargo build`) is **not sufficient** for any slice that touches cursor, scroll, selection, search, inline rows, folds, mouse, side filtering, modal subflows, or rendering output. This doc names the three verification modes, when to use which, and how to drive them.

This is referenced from `docs/MIGRATION.md` "Operating rule — finishing the work" and from each issue's `verification` field in `docs/work/issues.json`. Do not skip it.

## Three modes

| Mode | When | How | Persistence |
|---|---|---|---|
| **A — Compile-only** | Non-visual slices only: types, refactor, doc, internal trait changes that don't reach the renderer | `cargo build --profile dev-fast` | none |
| **B — Headless functional (termwright)** | **Default for any TUI slice.** Drives the dev binary in a real PTY, sends synthetic keys/mouse, asserts on cursor row / screen text / structured output | `bash scripts/tui-verify.sh` rebuilds `target/dev-fast/lazydiff` and runs every suite. Pass `--no-build` only when you are sure the binary is fresh. `bash scripts/tui-verify.sh <name>` runs one suite. | committed `.sh` files become regression tests forever |
| **C — Live tmux pane** | (i) Debugging something termwright can't easily inspect; (ii) HITL validation (issue #11); (iii) human visual review | `live_terminal_run({ command: "bash scripts/dev-watch-tui.sh --branch" })` then `live_terminal_read` / send keys / `live_terminal_close({ kill: true })` | ephemeral |

**Rule**: every TUI slice ships a Mode B test. Mode C is for debugging or HITL, not for "I think it works."

## Why Mode B is the agent's default

Termwright spawns the dev binary inside a real PTY (it's pseudo-terminal-driven, not screen-scraped) and exposes a JSON-RPC daemon. The binary cannot tell it is being driven by a script — keystrokes arrive exactly as if a human pressed them. The output is structured:

- `tw press '{"key":"j"}'` — single key
- `tw press '{"key":"Enter"}'` — special key
- `tw type '{"text":"hello world"}'` — multi-char input
- `tw hotkey '{"ctrl":true,"ch":"d"}'` — modified chord
- `tw screen '{"format":"json"}'` — full screen as JSON, with `.result.cursor.{row,col}`
- `tw screen '{"format":"text"}'` — full screen as plain text
- `tw wait_for_text '{"text":"...","timeout_ms":N}'` — wait for a string to render
- `tw close` — shutdown

This means the agent gets **parseable answers** ("cursor row is 14") instead of having to regex over an ANSI-laden text dump. That's the difference between B and C for an agent.

## Writing a new Mode-B test for your slice

For every TUI slice, before writing implementation code:

1. Create `scripts/test-<slice-name>-termwright.sh`. Use the existing `scripts/test-diff-navigation-termwright.sh` as the canonical template.
2. Stage a deterministic input (synthetic diff, fixed branch, controlled cursor start).
3. Send the keystrokes/mouse events that exercise the new behavior.
4. Assert on cursor row, screen text, or screen JSON. **Make the assertion fail on the current code** — that's the failing test.
5. `chmod +x scripts/test-<slice-name>-termwright.sh`.
6. Implement the slice. The test now passes.
7. Add the new script to `scripts/tui-verify.sh` so it runs in CI/local sweeps forever.

Template structure (mirrors the existing tests):

```sh
#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TERMWRIGHT_BIN=${TERMWRIGHT_BIN:-/Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/target/debug/termwright}
APP="$ROOT/target/dev-fast/lazydiff"

# 1. cargo build --profile dev-fast must have run; we don't rebuild here.
# 2. Stage a deterministic input (e.g. a synthetic .diff).
# 3. Start the daemon with --cols / --rows / -- "$APP" args.
# 4. Use tw press/type/hotkey to drive the binary.
# 5. Assert with tw screen + jq + your shell guard.
# 6. Echo PASS or fail loudly.
```

Build the termwright binary once:

```sh
bash ~/.agents/skills/librarian/checkout.sh fcoury/termwright --path-only
cargo build --manifest-path /Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/Cargo.toml
```

## When to drop into Mode C

Only if Mode B genuinely cannot answer the question. Examples:

- "Does the cursor *visually* look centered in the viewport?" → Mode B can read cursor row + first-visible row; Mode C lets a human eyeball it.
- "Is this color/style change correct?" → Mode B exposes ANSI styling info if asked; for subjective judgment, Mode C with HITL.
- "I'm chasing a flaky bug, I want to scrub through frames interactively." → Mode C.

Recipe for Mode C from the agent:

```text
1. live_terminal_run({ command: "bash scripts/dev-watch-tui.sh --branch", title: "lazydiff-dev" })
   → returns sessionId
2. live_terminal_read({ query: "...", lines: 60 })   to inspect state
3. live_terminal_run with key input or interactive_shell input to drive keys
   (or attach a human via live_terminal_attach)
4. live_terminal_close({ kill: true }) when done
```

**Mode C must not be used as the only verification.** It's ephemeral; the next context window won't remember it. Anything verified only in Mode C is verified by nobody.

## Anti-patterns

- **Compile-only on a TUI slice.** "It compiled" tells you nothing about whether the cursor still lands on row 14. If a slice changes `frame.rows`, navigation, scroll, mouse, selection, search, inline rows, folds, or rendering output, Mode A is not sufficient — even if all unit tests pass.
- **Mode C only.** Watching the dev binary in tmux and concluding "it looks fine" produces zero regression coverage. The next change reintroducing the bug will not be caught.
- **A termwright test that doesn't fail before the slice.** A test that passes against the *current* code is not exercising the new behavior. Write it failing first; make it pass by implementing the slice.
- **Tests that depend on real-world repo state.** Stage synthetic diffs or use fixed fixtures. Reproducibility matters more than realism.

## The TDD loop for a TUI slice

```text
1. Read the issue's `acceptance_criteria` and `north_star_check`.
2. Write scripts/test-<slice>-termwright.sh that asserts the behavior the
   slice should produce. Run it. It MUST fail or error on current code.
3. Implement the slice per the ADRs and the operating rule.
4. Re-run the test. It passes.
5. Run `bash scripts/tui-verify.sh` to confirm no other suite regressed.
6. `bash scripts/work.sh tick <id>.<n>` for each acceptance criterion.
7. `bash scripts/work.sh done <id>` when all ticked + verification ran.
8. Commit per `docs/MIGRATION.md` "Detailed commits per task."
```

If step 2 cannot be made to fail on current code, the slice is either already done or the acceptance criterion is wrong. Stop and ask.
