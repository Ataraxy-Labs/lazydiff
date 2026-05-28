#!/usr/bin/env bash
# tui-verify.sh — run the headless TUI regression suite (Mode B in
# docs/TUI_VERIFICATION.md). Discovers every scripts/test-*-termwright.sh,
# runs it, and reports PASS/FAIL with a per-test transcript.
#
# Usage:
#   bash scripts/tui-verify.sh           # run all suites
#   bash scripts/tui-verify.sh <name>    # run only test-<name>-termwright.sh
#
# Exit code is the number of failed suites (0 = all pass).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TERMWRIGHT_BIN=${TERMWRIGHT_BIN:-/Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/target/debug/termwright}
APP="$ROOT/target/dev-fast/lazydiff"

# ---- preflight ------------------------------------------------------------

fail_preflight() {
  echo "tui-verify: $1" >&2
  exit 2
}

if [[ ! -x "$TERMWRIGHT_BIN" ]]; then
  cat >&2 <<EOF
tui-verify: termwright not built at \$TERMWRIGHT_BIN
  $TERMWRIGHT_BIN

Build it once:
  bash ~/.agents/skills/librarian/checkout.sh fcoury/termwright --path-only
  cargo build --manifest-path /Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/Cargo.toml

Or override:
  TERMWRIGHT_BIN=/path/to/termwright bash scripts/tui-verify.sh
EOF
  exit 2
fi

if [[ ! -x "$APP" ]]; then
  echo "tui-verify: missing $APP" >&2
  echo "  run: cargo build --profile dev-fast" >&2
  exit 2
fi

# ---- discover suites ------------------------------------------------------

filter="${1:-}"
if [[ -n "$filter" ]]; then
  pattern="$ROOT/scripts/test-${filter}-termwright.sh"
else
  pattern="$ROOT/scripts/test-*-termwright.sh"
fi

# shellcheck disable=SC2206
suites=( $pattern )

if [[ "${#suites[@]}" -eq 0 || ! -f "${suites[0]}" ]]; then
  echo "tui-verify: no termwright suites matched: $pattern" >&2
  exit 2
fi

# ---- run ------------------------------------------------------------------

pass_count=0
fail_count=0
failed=()
start=$(date +%s)

for suite in "${suites[@]}"; do
  name=$(basename "$suite" .sh | sed 's/^test-//; s/-termwright$//')
  printf '── %-40s ' "$name"
  log=$(mktemp)
  if bash "$suite" >"$log" 2>&1; then
    last=$(tail -1 "$log")
    printf 'PASS   %s\n' "$last"
    pass_count=$((pass_count + 1))
  else
    printf 'FAIL\n'
    sed 's/^/    | /' "$log"
    fail_count=$((fail_count + 1))
    failed+=("$name")
  fi
  rm -f "$log"
done

elapsed=$(( $(date +%s) - start ))

echo
echo "── summary ─────────────────────────────────────────────────"
printf 'pass: %d   fail: %d   elapsed: %ds\n' "$pass_count" "$fail_count" "$elapsed"
if (( fail_count > 0 )); then
  echo "failed suites:"
  for f in "${failed[@]}"; do
    echo "  - $f"
  done
fi

exit "$fail_count"
