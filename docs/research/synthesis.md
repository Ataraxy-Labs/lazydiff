# Synthesis — how the research becomes LazyDiff's architecture

This page combines the four research sources into one map for LazyDiff. If you read only one file in `docs/research/`, read this one. The per-source files exist for depth.

## The architecture in one picture

```
┌────────────────────────────────────────────────────────────────┐
│ App Shell                                                      │
│   • routes between screens (queue, commit list, diff, …)       │
│   • runs effects returned by the Diff Workspace                │
│   • owns NO diff interaction state                             │
└──────────────────────────────┬─────────────────────────────────┘
                               │  intents in   /   effects out
┌──────────────────────────────┴─────────────────────────────────┐
│ Diff Workspace  (app-level, Rust-owned)                        │
│                                                                │
│   PRIVATE state                                                │
│     cursor · scroll · selection · search                       │
│     inline focus · draft editor · thread expansion             │
│     mouse drag · viewport                                      │
│                                                                │
│   ONE row list  ◄────────────────────  Pierre's Virtualizer    │
│     rows: Vec<WorkspaceVisualRow>                              │
│     rows_dirty: bool                                           │
│     row_height_overrides: HashMap<RowId, u16>  (sparse)        │
│                                                                │
│   update(intent) → Vec<Effect>   ◄────  XState reducer-first   │
│   small explicit enums for modal subflows                      │
│     (Search, PendingTextObject, Editor, Drag, Submit)          │
│                                                                │
│   accepts Review Workflow Contributions:                       │
│     Markers · Commands · Keymap · InlineRows                   │
│     ChromeSlots · Actions · ViewModelSlices                    │
│                                       ◄────  pi-mono + ProseMirror
└──────────────────────────────┬─────────────────────────────────┘
                               │  borrows view models / rows
┌──────────────────────────────┴─────────────────────────────────┐
│ lazydiff-diffs  (reusable core)                                │
│   • DiffDocument, DiffPaneTextLayout, coordinate math          │
│   • visual-row construction primitives                         │
│   • search · motion · selection algorithms                     │
│   • generic DiffDecoration primitives  ◄──  ProseMirror split  │
│   • render model + render primitives                           │
└────────────────────────────────────────────────────────────────┘
```

## Where each external lesson lands

| Lesson source | Lands at |
|---|---|
| ProseMirror — generic core, behavior via plugins | `lazydiff-diffs` stays generic; product behavior lives in Diff Workspace; visual annotations are generic `DiffDecoration`s, not "comments" |
| ProseMirror — commands + keymaps as data | `DiffWorkspaceIntent` enum + keymap mapping; contributions add commands and bindings |
| XState — reducer-first | `update(state, intent) → effects` is the only entry point to workspace state |
| XState — small explicit state enums for modal subflows | `DiffBufferMode` and friends become explicit Rust enums with allowed transitions |
| pi-mono — bounded capability contexts, never `&mut App` | Review Workflow Contributions get a read-mostly `ReviewContext` and emit intents; they never mutate workspace state directly |
| pi-mono — generation tokens for async | Effects carry a generation; stale results dropped |
| pierre — one owner of geometry | Diff Workspace owns the single `rows` list and the dirty flag |
| pierre — sparse override cache | `row_height_overrides` map stores only non-default heights |
| pierre — coalesced redraw | One `redraw_dirty: bool` drained per event-loop iteration |

## The vocabulary you'll see in the code

These are defined in `CONTEXT.md`. Skim once; use consistently.

- **Diff Workspace** — the interactive review surface as one coherent screen.
- **Diff Workspace Owner** — the Rust module with exclusive mutable ownership of diff-screen interaction state.
- **Visual Row** — one screen-row slot in the diff workspace, tagged with kind (diff text, inline review, future) and side (Left/Right).
- **Side-Filtered Selection** — a split-view selection that belongs to one side only.
- **Inline Review Row** — a visual row embedded in the diff workspace for a note/thread/draft.
- **Diff Decoration** — the generic visual annotation primitive (not a product comment).
- **Review Workflow Contribution** — a bounded extension that customizes review behavior without mutating diff coordinate math or app state.

## The bug class this architecture kills

Today the diff screen has *no single owner of "the row list."* Multiple consumers — keyboard navigation, scroll math, mouse mapping, the renderer — each rebuild their own copy by calling `visual_rows_with_inline_blocks` independently. Some code paths also call a different, *lying* row counter (`row_count_for_mode`) that ignores inline review rows.

```
Today                                           After

  input ─▶ rebuild list                         input ─▶ workspace.update(intent)
  scroll ─▶ rebuild list                                         │
  mouse  ─▶ rebuild list                              builds rows ONCE (or hits cache)
  render ─▶ rebuild list                                         │
                                                ┌────────────────┴─────────────┐
  4 lists/frame, drift apart                    │ &frame  (immutable borrow)   │
  whenever inline blocks change                 └─┬──────┬──────┬──────┬───────┘
                                              keyboard scroll mouse render
                                                  (all read same slice)
```

Pierre's `Virtualizer` solves this with a single owner and a dirty flag. ADR 0004 mandates the same shape for LazyDiff via Rust ownership.

## The split-brain failure mode (the "kitchen" analogy)

The bug class above has a useful informal name: **split-brain ownership**, or the kitchen analogy from the prior thread —

> *If the waiter, the fridge, and the oven all change the recipe independently, the meal fails.*

In LazyDiff's case the "kitchen" today is `App` + `DiffViewerState` + `CommentModal` all mutating related interaction state (cursor, scroll, focus). Whenever you find yourself wanting to add a "patch fix" that nudges one of them to match the others, you are working *around* the split brain rather than removing it. The right fix is always to move the concept into the Diff Workspace owner.

## Honest engineering — what we say in public

A previous Reddit critique flagged two slop patterns we explicitly avoid:

- **Vague "60fps TUI" claims** for screens that are mostly static. We are event-driven, not frame-driven. Public language uses input-to-render latency, draw time, event coalescing, and idle redraw behavior.
- **Giant unprovenanced commits** (e.g., "+21k lines" with no decision trail). Every architecture-shaped change carries: what ownership improved, what old mutation disappeared, what test/grep protects it, what docs changed.

Concrete latency target the prior thread settled on: **sub-16ms input-to-render latency during active interaction.** "Active" matters — idle screens should not be redrawing at all. This number is a quality bar, not a marketing claim.

## Product roadmap framing (from prior thread)

LazyDiff's extensibility unfolds in four stages. Each stage only opens once the previous one is proven by real internal use. This is the discipline behind ADR 0002 ("extension-shaped internals before a public plugin API").

1. **Internal seams.** Establish contribution points for the things we already do — comments, drafts, search, selection — as bounded surfaces inside the Diff Workspace. No public API yet. Goal: prove the shape against our own code.
2. **Declarative resources.** Custom keymaps, themes, and review templates loaded from config files. Still no executable user code; resources are data.
3. **First-party integrations.** Blame, CI annotations, coverage hints, AI suggestions — added as internal Review Workflow Contributions. Proves the contribution surface across genuinely different data sources.
4. **Public plugin API.** Only after stage 3 has multiple independent contributions hardening the surface, expose it for third-party plugins.

This roadmap is a framing, not a commitment to dates. Stage 1 is the current focus.

## What the migration looks like

The next compulsory plan item is *"True side-by-side visual-row model drives navigation, scrolling, mouse mapping, and renderer iteration."* Concretely that means:

1. Create the app-level Diff Workspace module with private state.
2. Move the row-list ownership into it. One `Vec<WorkspaceVisualRow>` + `rows_dirty: bool`.
3. Replace `row_count_for_mode(...)` calls with `frame.rows.len()`.
4. Replace independent `visual_rows_with_inline_blocks(...)` rebuilds with `&frame.rows`.
5. Add a regression test: building the row list twice in one frame returns the same vec (cache hit).
6. Drop the `App`-side fields that the workspace now owns; let Rust's privacy enforce the boundary.

Each slice should record:

- What ownership improved
- What old mutation disappeared
- What test/grep protects it
- What docs changed

## What this architecture does *not* commit to

- A public third-party plugin API (deferred per ADR 0002).
- A specific UI framework for chrome/status slots beyond what Ratatui provides.
- A specific persistence backend for review notes (effects abstract this away).
- An async runtime change.
- Any change to the existing visual quality (themes, syntax highlighting, Pierre-style spans, inline diff spans — all preserved per plan section 8).

## Where to go from here

- `docs/adr/0001` through `0004` — the formal decisions.
- `plan.md` — the migration checklist and operating rule.
- `AGENTS.md` — the rules future agents (and you) follow when touching this code.
- `CONTEXT.md` — vocabulary used everywhere.
- Per-source notes in this folder — depth on the lessons summarized above.
