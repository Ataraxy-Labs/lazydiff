#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TERMWRIGHT_BIN=${TERMWRIGHT_BIN:-/Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/target/debug/termwright}

if [[ ! -x "$TERMWRIGHT_BIN" ]]; then
  cat >&2 <<EOF
termwright binary not found: $TERMWRIGHT_BIN

Build it with:
  cargo build --manifest-path /Users/palanikannanm/.cache/checkouts/github.com/fcoury/termwright/Cargo.toml

Or set TERMWRIGHT_BIN=/path/to/termwright.
EOF
  exit 2
fi

APP="$ROOT/target/dev-fast/lazydiff"
if [[ ! -x "$APP" ]]; then
  echo "missing $APP; run cargo build --profile dev-fast" >&2
  exit 2
fi

tmp=$(mktemp -d)
daemon_pid=""
cleanup() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

cat > "$tmp/simple.diff" <<'DIFF'
diff --git a/demo.txt b/demo.txt
index 83db48f..f735c2d 100644
--- a/demo.txt
+++ b/demo.txt
@@ -1,5 +1,6 @@
 one
-two
+two changed
 three
 four
+five added
 six
DIFF

sock="$tmp/termwright.sock"
XDG_DATA_HOME="$tmp/xdg" "$TERMWRIGHT_BIN" daemon \
  --socket "$sock" \
  --cols 100 \
  --rows 32 \
  -- "$APP" patch "$tmp/simple.diff" \
  >"$tmp/daemon.log" 2>&1 &
daemon_pid=$!

for _ in {1..100}; do
  [[ -S "$sock" ]] && break
  sleep 0.1
done
if [[ ! -S "$sock" ]]; then
  echo "termwright daemon socket was not created" >&2
  cat "$tmp/daemon.log" >&2 || true
  exit 1
fi

tw() {
  "$TERMWRIGHT_BIN" exec --socket "$sock" --method "$1" --params "${2:-null}"
}

tw wait_for_text '{"text":"demo.txt","timeout_ms":8000}' >/dev/null
tw press '{"key":"i"}' >/dev/null
tw type '{"text":"abcdefghijklmnopqrstuvwxyz"}' >/dev/null
sleep 0.2

screen=$(tw screen '{"format":"text"}' | jq -r '.result')
cursor=$(tw screen '{"format":"json"}' | jq -c '.result.cursor')

first_body_line_number=$(printf '%s\n' "$screen" | awk '/abcdefghijklmnopqrstuvwx/ { print NR; exit }')
second_body_line_number=$(printf '%s\n' "$screen" | awk '/yz/ { print NR; exit }')
if [[ -z "$first_body_line_number" || -z "$second_body_line_number" ]]; then
  echo "FAIL: wrapped note editor body lines were not rendered" >&2
  printf '%s\n' "$screen" >&2
  exit 1
fi

first_body_line=$(printf '%s\n' "$screen" | sed -n "${first_body_line_number}p")
second_body_line=$(printf '%s\n' "$screen" | sed -n "${second_body_line_number}p")
cursor_row=$(jq -r '.row' <<<"$cursor")
cursor_col=$(jq -r '.col' <<<"$cursor")

python3 - <<'PY' "$first_body_line" "$first_body_line_number" "$second_body_line" "$second_body_line_number" "$cursor_row" "$cursor_col"
import sys

first_line = sys.argv[1]
first_line_number = int(sys.argv[2])
second_line = sys.argv[3]
second_line_number = int(sys.argv[4])
cursor_row = int(sys.argv[5])
cursor_col = int(sys.argv[6])

# Termwright cursor rows are zero-based; awk line numbers are one-based.
first_row = first_line_number - 1
second_row = second_line_number - 1
first_text = "abcdefghijklmnopqrstuvwx"
second_text = "yz"
first_col = first_line.find(first_text)
second_col = second_line.find(second_text)
if first_col < 0 or second_col < 0:
    print("FAIL: expected wrapped editor text missing")
    print(repr(first_line))
    print(repr(second_line))
    raise SystemExit(1)

if first_line[first_col:first_col + len(first_text)] + second_line[second_col:second_col + len(second_text)] != "abcdefghijklmnopqrstuvwxyz":
    print("FAIL: wrapped text does not reconstruct the typed buffer")
    print(repr(first_line))
    print(repr(second_line))
    raise SystemExit(1)

expected_col = second_col + len(second_text)
if cursor_row != second_row:
    print(f"FAIL: cursor row {cursor_row} is not the wrapped editor row {second_row}")
    print(repr(first_line))
    print(repr(second_line))
    raise SystemExit(1)
if cursor_col != expected_col:
    print(f"FAIL: cursor col {cursor_col} != expected insertion col {expected_col}")
    print(repr(first_line))
    print(repr(second_line))
    raise SystemExit(1)

print("PASS: inline note editor cursor stays aligned with the typed buffer")
PY

tw close >/dev/null || true