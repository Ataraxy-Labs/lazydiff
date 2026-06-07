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

cat >"$tmp/two-files.diff" <<'EOF'
diff --git a/a.txt b/a.txt
index 1111111..2222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-alpha old
+alpha new
diff --git a/b.txt b/b.txt
index 3333333..4444444 100644
--- a/b.txt
+++ b/b.txt
@@ -1 +1 @@
-beta old
+beta new
EOF

sock="$tmp/termwright.sock"
XDG_DATA_HOME="$tmp/xdg" "$TERMWRIGHT_BIN" daemon \
  --socket "$sock" \
  --cols 120 \
  --rows 28 \
  -- "$APP" patch "$tmp/two-files.diff" \
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

screen_text() {
  tw screen '{"format":"text"}' | jq -r '.result'
}

assert_screen_contains() {
  local label=$1
  local needle=$2
  if ! screen_text | grep -F "$needle" >/dev/null; then
    echo "FAIL: $label did not find '$needle'" >&2
    screen_text >&2
    exit 1
  fi
}

assert_b_file_viewed() {
  if ! screen_text | python3 -c 'import sys
for line in sys.stdin:
    if "b.txt" in line and "✓" in line:
        raise SystemExit(0)
raise SystemExit(1)
'; then
    echo "FAIL: b.txt was not marked viewed after focusing Changes, moving down, and pressing r" >&2
    screen_text >&2
    exit 1
  fi
}

tw wait_for_text '{"text":"Changes 0/2","timeout_ms":8000}' >/dev/null
assert_screen_contains "initial sidebar" "a.txt"
assert_screen_contains "initial sidebar" "b.txt"

tw press '{"key":"2"}' >/dev/null
tw press '{"key":"j"}' >/dev/null
tw press '{"key":"r"}' >/dev/null
sleep 0.2

tw wait_for_text '{"text":"Changes 1/2","timeout_ms":4000}' >/dev/null
assert_b_file_viewed

tw press '{"key":"Enter"}' >/dev/null
sleep 0.2
assert_screen_contains "opened second file diff" "beta new"

tw press '{"key":"3"}' >/dev/null
tw wait_for_text '{"text":"Review items","timeout_ms":4000}' >/dev/null
tw press '{"key":"Esc"}' >/dev/null
sleep 0.2

tw press '{"key":"4"}' >/dev/null
tw wait_for_text '{"text":"Commits","timeout_ms":4000}' >/dev/null

echo "PASS: Changes panel jump, movement, viewed toggle, and open action work in the real TUI"
tw close >/dev/null || true
