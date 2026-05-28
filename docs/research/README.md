# Research notes — what informs LazyDiff's architecture

These notes document the external projects we studied while shaping the **Diff Workspace** architecture, and the specific lessons we are importing. They exist so you (and any future agent) can read the decisions in `docs/adr/` and immediately see *why* — without re-doing the research.

## How to read this folder

- Start with `synthesis.md` for the one-page map of how all four sources combine into the LazyDiff architecture.
- Read each source-specific file (`prosemirror.md`, `xstate.md`, `pi-mono.md`, `pierre.md`) when you want depth on that source's lesson.
- Read `rust-modules-and-visibility.md` when in doubt about crates, modules, `mod` vs `use`, `pub(crate)` vs `pub(super)`, or why something isn't visible. It is the authoritative reference for this codebase's Rust mechanics.
- These notes are **not** the decision record. The decisions live in `docs/adr/`. These notes are the evidence behind the decisions.

## Sources studied

| Source | Domain | One-sentence lesson |
|---|---|---|
| [ProseMirror](prosemirror.md) | Rich text editor | Generic core primitives + product behavior via plugins/decorations/commands/keymaps. |
| [XState](xstate.md) | State machines / actors | Reducer-first is usually enough; reach for full statecharts only for genuinely modal subflows. |
| [pi-mono](pi-mono.md) | Agentic CLI / chat | Extensions get bounded capabilities/contexts, not raw app mutation. |
| [pierre](pierre.md) | Browser code review UI | One geometry owner + sparse height cache + coalesced redraw = fast giant diffs. |

## How these map to LazyDiff

- ProseMirror → ADR 0001 (Diff Workspace + generic decorations) and ADR 0002 (extension-shaped internals before a public plugin API).
- XState → ADR 0003 (reducer-first update loop; small explicit state enums only for modal subflows).
- pi-mono → ADR 0002 and ADR 0004 (bounded contribution surfaces; Rust-owned workspace state with no raw mutation).
- pierre → reinforces ADR 0004 (one owner) and informs the **visual row cache** design (sparse override heights, dirty-flag rebuild, coalesced redraw).

## Prior thread

Most of the design dialogue happened in thread `T-019e4443-e447-71e1-b9f4-0f035069d83c`. Use `read_thread` against that ID if you want the original conversation. Key user-owned decisions captured there are also in the `CONTEXT.md` and the ADRs.

## Local checkouts — ground every external claim in real source

The reference repos are cached locally under `~/.cache/checkouts/github.com/`. **Read them directly before claiming how any of these projects work.** Do not infer; grep the source.

| Repo | Local path |
|---|---|
| ProseMirror core | `~/.cache/checkouts/github.com/ProseMirror/prosemirror` |
| ProseMirror state | `~/.cache/checkouts/github.com/ProseMirror/prosemirror-state` |
| ProseMirror view | `~/.cache/checkouts/github.com/ProseMirror/prosemirror-view` |
| ProseMirror commands | `~/.cache/checkouts/github.com/ProseMirror/prosemirror-commands` |
| ProseMirror keymap | `~/.cache/checkouts/github.com/ProseMirror/prosemirror-keymap` |
| ProseMirror history | `~/.cache/checkouts/github.com/ProseMirror/prosemirror-history` |
| XState | `~/.cache/checkouts/github.com/statelyai/xstate` |
| pi-mono | `~/.cache/checkouts/github.com/badlogic/pi-mono` |
| pierre | `~/.cache/checkouts/github.com/pierrecomputer/pierre` |

If a path is missing or stale, refresh it via the `librarian` skill:

```sh
bash ~/.agents/skills/librarian/checkout.sh <owner>/<repo> --path-only
# e.g. bash ~/.agents/skills/librarian/checkout.sh pierrecomputer/pierre --path-only
```

Add `--force-update` to force a fetch. **Do not write a new research claim without grounding it in the checkout.**

## What this folder is NOT

- Not a tutorial for any of these projects.
- Not a public-facing comparison or marketing piece.
- Not a place to add new architecture decisions (those go in `docs/adr/`).
