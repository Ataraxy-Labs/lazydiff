#!/usr/bin/env bash
# work.sh — tiny issue tracker for the active feature's issues.json
#
# Active feature folder is resolved from $LAZYDIFF_FEATURE (default: 002-clean-tui-rewrite).
# Per-feature layout: docs/features/<slug>/{spec.md,plan.md,RULES.md,issues.json,README.md}
#
# Commands:
#   work list                  open issues, grouped by phase
#   work next                  first unblocked open issue
#   work show <id>             full issue body
#   work start <id>            mark status=in_progress
#   work tick <id>.<n>         tick acceptance criterion n (1-indexed)
#   work untick <id>.<n>       untick acceptance criterion n
#   work done <id>             mark status=done (requires all criteria ticked)
#   work block <id> <blocker>  add blocker to issue
#   work unblock <id> <blocker> remove blocker
#   work add-child <pid> "<t>" create child issue
#   work add "<title>"         create top-level issue
#   work note <id> "<text>"    append a note
#   work stats                 quick counts
#
# Storage: docs/features/<feature>/issues.json (atomic write via temp file).
# Requires: jq.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FEATURE="${LAZYDIFF_FEATURE:-002-clean-tui-rewrite}"
FILE="$ROOT/docs/features/$FEATURE/issues.json"
TMP="$FILE.tmp"

if ! command -v jq >/dev/null 2>&1; then
  echo "work.sh: jq is required" >&2
  exit 2
fi

if [[ ! -f "$FILE" ]]; then
  echo "work.sh: $FILE not found" >&2
  exit 2
fi

now() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }

# write_json EXPR  — runs jq EXPR against FILE and atomically writes back
write_json() {
  jq "$1" "$FILE" > "$TMP" && mv "$TMP" "$FILE"
}

cmd_list() {
  jq -r '
    .issues
    | map(select(.status != "done"))
    | sort_by(.phase, .id)
    | group_by(.phase)
    | .[] | "── phase \(.[0].phase) ──",
      (.[] | "  [\(.status[0:4] | ascii_upcase)] #\(.id) (\(.type)) \(.title)\(if (.blocked_by|length)>0 then "  ← blocked by \(.blocked_by|join(","))" else "" end)")
  ' "$FILE"
}

cmd_next() {
  local id
  id=$(jq -r '
    .issues
    | map(select(.status == "open"))
    | map(select(
        (.blocked_by | length) == 0
        or all(.blocked_by[]; . as $b | (input_filename | not) or (([$b] | inside([])) | not))
      ))
    | sort_by(.phase, .id)
    | first
    | .id // "none"
  ' "$FILE")
  # The above is awkward in jq; do the blocker check the simple way:
  id=$(jq -r '
    . as $root
    | .issues
    | map(select(.status == "open"))
    | map(. as $i | select(
        all($i.blocked_by[]?; . as $b
            | $root.issues[] | select(.id == $b) | .status == "done")
      ))
    | sort_by(.phase, .id)
    | (first | .id) // "none"
  ' "$FILE")
  if [[ "$id" == "none" ]]; then
    echo "no unblocked open issues. either all done or all blocked."
    return 0
  fi
  cmd_show "$id"
}

cmd_show() {
  local id="$1"
  jq -r --argjson id "$id" '
    .issues[] | select(.id == $id) |
    "#\(.id) — \(.title)
status:           \(.status)
type:             \(.type)
phase:            \(.phase)
parent:           \(.parent_id // "—")
discovered during: \(.discovered_during // "—")
blocked by:       \(if (.blocked_by|length)>0 then (.blocked_by|join(", ")) else "—" end)

What to build:
\(.what_to_build)

Acceptance criteria:" +
    (reduce (.acceptance_criteria | to_entries[]) as $c ("";
       . + "\n  \(if $c.value.done then "[x]" else "[ ]" end) \($c.key + 1). \($c.value.text)"
    )) +
    "

Verification:
  \(.verification)

North-star check:
  \(.north_star_check)" +
    (if (.notes // "") != "" then "\n\nNotes:\n  " + .notes else "" end)
  ' "$FILE"
}

cmd_start() {
  local id="$1"
  write_json "(.issues[] | select(.id == $id) | .status) |= \"in_progress\" | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id → in_progress"
}

cmd_tick() {
  local ref="$1"
  local id="${ref%.*}"
  local n="${ref#*.}"
  local idx=$((n - 1))
  write_json "(.issues[] | select(.id == $id) | .acceptance_criteria[$idx].done) |= true | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id criterion $n → ticked"
}

cmd_untick() {
  local ref="$1"
  local id="${ref%.*}"
  local n="${ref#*.}"
  local idx=$((n - 1))
  write_json "(.issues[] | select(.id == $id) | .acceptance_criteria[$idx].done) |= false | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id criterion $n → unticked"
}

cmd_done() {
  local id="$1"
  local all_ticked
  all_ticked=$(jq -r --argjson id "$id" '
    .issues[] | select(.id == $id)
    | (.acceptance_criteria | all(.done)) // false
  ' "$FILE")
  if [[ "$all_ticked" != "true" ]]; then
    echo "work.sh: refusing to close #$id — not all acceptance criteria are ticked" >&2
    cmd_show "$id"
    exit 1
  fi
  write_json "(.issues[] | select(.id == $id) | .status) |= \"done\" | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id → done"
}

cmd_block() {
  local id="$1"; local blocker="$2"
  write_json "(.issues[] | select(.id == $id) | .blocked_by) |= (. + [$blocker] | unique) | (.issues[] | select(.id == $id) | .status) |= (if . == \"open\" then \"blocked\" else . end) | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id ← blocked by #$blocker"
}

cmd_unblock() {
  local id="$1"; local blocker="$2"
  write_json "(.issues[] | select(.id == $id) | .blocked_by) |= map(select(. != $blocker)) | (.issues[] | select(.id == $id) | .status) |= (if . == \"blocked\" and (.blocked_by|length) == 0 then \"open\" else . end) | (.issues[] | select(.id == $id) | .updated_at) |= \"$(now)\""
  echo "#$id → unblocked from #$blocker"
}

cmd_add() {
  local title="$1"
  local next_id
  next_id=$(jq -r '(.issues | map(.id) | max // 0) + 1' "$FILE")
  write_json "
    .issues += [{
      id: $next_id,
      title: \"$title\",
      status: \"open\",
      type: \"AFK\",
      phase: \"X\",
      parent_id: null,
      discovered_during: null,
      blocked_by: [],
      what_to_build: \"\",
      acceptance_criteria: [],
      verification: \"\",
      north_star_check: \"\",
      notes: \"\",
      created_at: \"$(now)\",
      updated_at: \"$(now)\"
    }]
  "
  echo "created #$next_id: $title"
}

cmd_add_child() {
  local pid="$1"; local title="$2"
  local next_id
  next_id=$(jq -r '(.issues | map(.id) | max // 0) + 1' "$FILE")
  local parent_phase
  parent_phase=$(jq -r --argjson p "$pid" '.issues[] | select(.id == $p) | .phase' "$FILE")
  write_json "
    .issues += [{
      id: $next_id,
      title: \"$title\",
      status: \"open\",
      type: \"AFK\",
      phase: \"$parent_phase\",
      parent_id: $pid,
      discovered_during: $pid,
      blocked_by: [],
      what_to_build: \"\",
      acceptance_criteria: [],
      verification: \"\",
      north_star_check: \"\",
      notes: \"\",
      created_at: \"$(now)\",
      updated_at: \"$(now)\"
    }]
  "
  echo "created #$next_id (child of #$pid): $title"
}

cmd_note() {
  local id="$1"; local text="$2"
  local stamp; stamp=$(now)
  write_json "(.issues[] | select(.id == $id) | .notes) |= (. + (if . == \"\" then \"\" else \"\n\" end) + \"[$stamp] $text\") | (.issues[] | select(.id == $id) | .updated_at) |= \"$stamp\""
  echo "#$id ← note"
}

cmd_stats() {
  jq -r '
    .issues
    | group_by(.status)
    | map({status: .[0].status, count: length})
    | sort_by(.status)
    | .[] | "\(.status): \(.count)"
  ' "$FILE"
  echo
  jq -r '
    "total: \(.issues | length)"
  ' "$FILE"
}

usage() {
  sed -n '2,21p' "$0"
}

case "${1:-}" in
  list)        cmd_list ;;
  next)        cmd_next ;;
  show)        cmd_show "${2:?id required}" ;;
  start)       cmd_start "${2:?id required}" ;;
  tick)        cmd_tick "${2:?id.criterion required}" ;;
  untick)      cmd_untick "${2:?id.criterion required}" ;;
  done)        cmd_done "${2:?id required}" ;;
  block)       cmd_block "${2:?id required}" "${3:?blocker required}" ;;
  unblock)     cmd_unblock "${2:?id required}" "${3:?blocker required}" ;;
  add)         cmd_add "${2:?title required}" ;;
  add-child)   cmd_add_child "${2:?parent required}" "${3:?title required}" ;;
  note)        cmd_note "${2:?id required}" "${3:?text required}" ;;
  stats)       cmd_stats ;;
  ""|-h|--help|help) usage ;;
  *) echo "work.sh: unknown command '$1'" >&2; usage; exit 2 ;;
esac
