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

(cd "$ROOT" && cargo build -p lazydiff-v2-server -p lazydiff-v2-tui >/dev/null)

SERVER="$ROOT/target/debug/lazydiff-v2-server"
APP="$ROOT/target/debug/lazydiff-v2"

tmp=$(mktemp -d)
server_pid=""
daemon_pid=""
cleanup() {
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

cat >"$tmp/v2.diff" <<'DIFF'
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
 fn main() {
-    old();
+    new();
 }
DIFF

port=4197
"$SERVER" patch "$tmp/v2.diff" --port "$port" >"$tmp/server.log" 2>&1 &
server_pid=$!

for _ in {1..100}; do
  if grep -q "listening" "$tmp/server.log"; then
    break
  fi
  sleep 0.1
done
if ! grep -q "listening" "$tmp/server.log"; then
  echo "v2 server did not start" >&2
  cat "$tmp/server.log" >&2 || true
  exit 1
fi

sock="$tmp/termwright.sock"
XDG_DATA_HOME="$tmp/xdg" "$TERMWRIGHT_BIN" daemon \
  --socket "$sock" \
  --cols 100 \
  --rows 24 \
  -- bash -lc "'$APP' --server '127.0.0.1:$port'; sleep 5" \
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

assert_visible() {
  local text=$1
  if ! tw wait_for_text "{\"text\":\"$text\",\"timeout_ms\":8000}" >/dev/null; then
    echo "FAIL: expected v2 render to include: $text" >&2
    tw screen '{"format":"text"}' | jq -r '.result' >&2 || true
    exit 1
  fi
}

assert_visible "LazyDiff v2 terminal"
assert_visible "src/main.rs"
assert_visible "fn main()"
assert_visible "-    old();"
assert_visible "+    new();"

echo "PASS: v2 TUI renders server-backed diff frame"
tw close >/dev/null || true
