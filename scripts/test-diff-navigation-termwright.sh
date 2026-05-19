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

{
  echo 'diff --git a/long.txt b/long.txt'
  echo 'index 1111111..2222222 100644'
  echo '--- a/long.txt'
  echo '+++ b/long.txt'
  echo '@@ -1,80 +1,80 @@'
  for i in $(seq 1 80); do
    printf ' line %02d\n' "$i"
  done
} > "$tmp/long.diff"

sock="$tmp/termwright.sock"
XDG_DATA_HOME="$tmp/xdg" "$TERMWRIGHT_BIN" daemon \
  --socket "$sock" \
  --cols 100 \
  --rows 32 \
  -- "$APP" patch "$tmp/long.diff" \
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

cursor_row() {
  tw screen '{"format":"json"}' | jq -r '.result.cursor.row'
}

first_visible_line() {
  tw screen '{"format":"text"}' | jq -r '.result' | python3 -c 'import re, sys
for line in sys.stdin:
    match = re.search(r"\bline (\d{2})\b", line)
    if match:
        print(int(match.group(1)))
        break
else:
    print(0)
'
}

assert_centered_cursor() {
  local label=$1
  local row
  row=$(cursor_row)
  # Rows 10..22 are the stable middle band of the 32-row test terminal after
  # accounting for the top chrome and bottom help/status rows.
  if (( row < 10 || row > 22 )); then
    echo "FAIL: $label cursor row $row is outside the centered diff band" >&2
    tw screen '{"format":"text"}' | jq -r '.result' >&2
    exit 1
  fi
}

assert_visible_cursor() {
  local label=$1
  local row
  row=$(cursor_row)
  # The 32-row test terminal has app chrome above and a footer below. Normal
  # j/k movement should not recenter, but the focused row must remain visible
  # in the diff area.
  if (( row < 4 || row > 30 )); then
    echo "FAIL: $label cursor row $row is outside the visible diff area" >&2
    tw screen '{"format":"text"}' | jq -r '.result' >&2
    exit 1
  fi
}

tw wait_for_text '{"text":"long.txt","timeout_ms":8000}' >/dev/null

# Create a tall inline note near the top of the diff. This exposes the visual
# row bug: document-row centering ignores inline rows above the cursor.
tw press '{"key":"n"}' >/dev/null
for i in $(seq 1 12); do
  tw type "{\"text\":\"note$i\"}" >/dev/null
  tw press '{"key":"Enter"}' >/dev/null
done
tw press '{"key":"Esc"}' >/dev/null
tw press '{"key":"Enter"}' >/dev/null
sleep 0.2

# Regression: crossing an inline note must not fall back to document-row
# focus_row(), which recenters and causes a visible jump around the note.
previous_first=$(first_visible_line)
for _ in $(seq 1 16); do
  tw press '{"key":"j"}' >/dev/null
  sleep 0.05
  current_first=$(first_visible_line)
  delta=$(( current_first - previous_first ))
  if (( delta < 0 )); then
    delta=$(( -delta ))
  fi
  if (( delta > 4 )); then
    echo "FAIL: inline note boundary jump changed first visible line from $previous_first to $current_first" >&2
    tw screen '{"format":"text"}' | jq -r '.result' >&2
    exit 1
  fi
  previous_first=$current_first
done

for _ in $(seq 1 30); do
  tw press '{"key":"j"}' >/dev/null
done
sleep 0.2
assert_visible_cursor "after repeated j"

tw hotkey '{"ctrl":true,"ch":"d"}' >/dev/null
sleep 0.2
assert_centered_cursor "after Ctrl-d"

tw hotkey '{"ctrl":true,"ch":"u"}' >/dev/null
sleep 0.2
assert_centered_cursor "after Ctrl-u"

echo "PASS: diff j/k stays visible and Ctrl-u/d keep the focused visual row centered"
tw close >/dev/null || true