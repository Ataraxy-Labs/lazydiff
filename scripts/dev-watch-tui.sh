#!/usr/bin/env bash
set -euo pipefail

if ! command -v watchexec >/dev/null 2>&1; then
  cat >&2 <<'EOF'
dev-watch-tui requires watchexec.

Install it with one of:
  brew install watchexec
  cargo install watchexec-cli

Then run this script from your second terminal/tmux window:
  scripts/dev-watch-tui.sh
EOF
  exit 1
fi

quoted_args=()
for arg in "$@"; do
  printf -v quoted_arg '%q' "$arg"
  quoted_args+=("$quoted_arg")
done
run_command="cargo build --profile dev-fast && exec target/dev-fast/lazydiff ${quoted_args[*]}"

watchexec \
  --restart \
  --clear \
  --watch src \
  --watch crates \
  --watch Cargo.toml \
  --watch Cargo.lock \
  --exts rs,toml \
  --shell bash \
  -- "$run_command"
