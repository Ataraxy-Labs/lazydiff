#!/usr/bin/env bash
# tui-verify.sh — run the headless TUI regression suite (Mode B in
# docs/TUI_VERIFICATION.md). Rebuilds the dev-fast binary by default
# so the suite never runs against stale code. Discovers every
# scripts/test-*-termwright.sh, runs it, and reports PASS/FAIL with
# per-test transcript on failure.
#
# Usage:
#   bash scripts/tui-verify.sh                 # rebuild dev-fast + run all suites
#   bash scripts/tui-verify.sh --no-build      # skip rebuild (only when you know it's fresh)
#   bash scripts/tui-verify.sh <name>          # rebuild + run only test-<name>-termwright.sh
#   bash scripts/tui-verify.sh --no-build <n>  # skip rebuild + run only one suite
#
# Exit code is the number of failed suites (0 = all pass) or 2 on preflight error.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TERMWRIGHT_BIN=${TERMWRIGHT_BIN:-/Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/target/debug/termwright}
APP="$ROOT/target/dev-fast/lazydiff"

# ---- arg parsing ----------------------------------------------------------

rebuild=1
filter=""
for arg in "$@"; do
  case "$arg" in
    --no-build) rebuild=0 ;;
    --help|-h)
      sed -n '2,15p' "$0"
      exit 0
      ;;
    -*) echo "tui-verify: unknown flag '$arg'" >&2; exit 2 ;;
    *) filter="$arg" ;;
  esac
done

# ---- preflight: termwright ------------------------------------------------

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

# ---- rebuild dev-fast (default) ------------------------------------------

if (( rebuild == 1 )); then
  echo "── cargo build --profile dev-fast (use --no-build to skip) ─────────"
  build_start=$(date +%s)
  if ! ( cd "$ROOT" && cargo build --profile dev-fast 2>&1 | tail -3 ); then
    echo "tui-verify: cargo build failed; cannot run TUI suite against stale binary" >&2
    exit 2
  fi
  echo "── build done in $(( $(date +%s) - build_start ))s"
  echo
fi

if [[ ! -x "$APP" ]]; then
  echo "tui-verify: missing $APP after build" >&2
  echo "  re-run without --no-build, or: cargo build --profile dev-fast" >&2
  exit 2
fi

# ---- discover suites ------------------------------------------------------

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
