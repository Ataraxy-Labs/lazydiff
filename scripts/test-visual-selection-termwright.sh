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

cat >"$tmp/simple.diff" <<'EOF'
diff --git a/a.txt b/a.txt
index 1111111..2222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,6 +1,6 @@
 alpha one
 beta two
 gamma three
 delta four
 epsilon five
 zeta six
EOF

sock="$tmp/termwright.sock"
XDG_DATA_HOME="$tmp/xdg" "$TERMWRIGHT_BIN" daemon \
  --socket "$sock" \
  --cols 90 \
  --rows 24 \
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

selection_cell_count() {
  tw screen '{"format":"json"}' | jq '[.result.cells[][] | select(.bg.type == "Rgb" and .bg.value == [31,75,153])] | length'
}

tw wait_for_text '{"text":"alpha","timeout_ms":8000}' >/dev/null

for _ in 1 2 3; do
  tw press '{"key":"j"}' >/dev/null
done
tw press '{"key":"v"}' >/dev/null
tw press '{"key":"j"}' >/dev/null
tw press '{"key":"j"}' >/dev/null
sleep 0.2

selected_cells=$(selection_cell_count)
if (( selected_cells < 10 )); then
  echo "FAIL: visual selection lost its painted range after v j j (selected cells: $selected_cells)" >&2
  tw screen '{"format":"text"}' | jq -r '.result' >&2
  exit 1
fi

echo "PASS: visual v/j selections remain painted across visual-row movement"
tw close >/dev/null || true