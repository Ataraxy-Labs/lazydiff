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

assert_note_draft_visible() {
  local label=$1
  local screen
  screen=$(tw screen '{"format":"text"}' | jq -r '.result')
  if ! grep -q 'note draft' <<<"$screen"; then
    echo "FAIL: $label did not open an inline note draft" >&2
    printf '%s\n' "$screen" >&2
    exit 1
  fi
}

tw wait_for_text '{"text":"demo.txt","timeout_ms":8000}' >/dev/null

tw press '{"key":"i"}' >/dev/null
sleep 0.2
assert_note_draft_visible "plain i"

tw press '{"key":"Esc"}' >/dev/null
tw press '{"key":"Esc"}' >/dev/null

tw press '{"key":"v"}' >/dev/null
tw press '{"key":"j"}' >/dev/null
tw press '{"key":"i"}' >/dev/null
sleep 0.2
assert_note_draft_visible "visual i"

tw type '{"text":"existing note"}' >/dev/null
tw press '{"key":"Esc"}' >/dev/null
tw press '{"key":"Enter"}' >/dev/null
sleep 0.2

tw press '{"key":"i"}' >/dev/null
sleep 0.2
assert_note_draft_visible "plain i on a line that already has a note"

echo "PASS: i opens inline note drafts for cursor and visual selections"
tw close >/dev/null || true