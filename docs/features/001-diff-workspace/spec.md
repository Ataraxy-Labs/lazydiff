# Feature 001 — Diff Workspace

The Diff Workspace migration is LazyDiff's **proving ground for the whole-TUI Surface Owner architecture**. It is the first feature; everything else builds on what this slice establishes.

## What this feature is

Migrate the diff surface — today scattered across `App`, `DiffBufferState`, `DiffViewerState`, and ad-hoc helpers — into a single Rust-owned **Diff Workspace Owner** that:

- holds the only mutable copy of cursor, scroll, side-filtered selection, inline review focus, draft editor state, thread expansion, mouse drag, search state, folds, and modal subflows;
- exposes a `update(intent: DiffWorkspaceIntent) -> Vec<DiffWorkspaceEffect>` reducer-style API as the only writer;
- lends a borrow-checked, dirty-cached **Visual-Row Stream** that drives navigation, scrolling, mouse mapping, and renderer iteration **from the same list, for one frame**;
- accepts bounded **Review Workflow Contributions** (commands, keymaps, inline rows, decorations, chrome slots, fold strategies, effects) — internal and compile-time-Rust-friendly — without ever handing them `&mut App` or `&mut DiffWorkspace`.

## Why it exists

Today's diff surface is the densest concentration of bugs in LazyDiff because four consumers (navigation, scrolling, mouse, renderer) each compute their own idea of "what rows exist." Off-by-one errors, ghost selections, fold/inline-row drift, and mouse misalignment are *not* unrelated incidents — they are one bug class wearing different costumes. The fix is structural: one cached row list, one mutable owner, compile-error-enforced privacy. Patch fixes have proven incapable of closing this class.

The Diff Workspace also exists to **prove the Surface Owner pattern works** before applying it to Semantic, Finder, Command Palette, Review Sidebar, Commit List, and Queue/Home (see `plan.md` "Whole-TUI follow-on slices").

## Scope

In:

- One Rust-owned `DiffWorkspace` module with private state and a reducer-style API.
- Migration of cursor / scroll / selection / inline focus / draft editor / thread expansion / mouse drag / search / folds / modal subflows into the workspace.
- A unified **Visual-Row Stream** with dirty-flag cache (ADR 0005).
- Internal contribution kinds for commands, keymaps, palette entries, inline rows, decorations, chrome slots, and fold strategies (ADR 0008).
- Compile-time-Rust contributions: friendly forks/embedders can register their own `FoldStrategy`, command, keymap, inline-row producer, decoration producer, or chrome slot in Rust against the stable contribution traits, with no runtime loader (ADR 0002 stage 1 / 1.5).
- Termwright regression tests per TUI-observable slice (Mode B in `docs/TUI_VERIFICATION.md`).

Out:

- Any other surface (Semantic, Finder, Command Palette, Review Sidebar, Commit List, Queue/Home). Those follow under their own future feature folders.
- A public third-party plugin runtime (dynamic libs / WASM / scripting). Explicitly deferred per ADR 0002 stage 4.
- Renderer rewrite. The renderer keeps its current visual quality; only its inputs change.

## Success criteria

- Every compulsory item in `plan.md` is checked.
- Every issue in `issues.json` is `done` or has explicit human follow-up.
- Greps for `viewer_mut()|inline_focus|comment_modal` in `src/app.rs` / `src/app` drop to zero outside the workspace module.
- Greps for `row_count_for_mode|visual_rows_with_inline_blocks` in `src/app.rs|src/render|src/diff_render` drop to zero outside the workspace module.
- `bash scripts/tui-verify.sh` passes every suite.
- A second person (or a fresh agent session) can add a new contribution kind (e.g. an AI-risk-driven `FoldStrategy`) by writing pure Rust against the contribution traits, without modifying `DiffWorkspace` internals or `App`.

## Out of scope for this feature (file as future work if surfaced)

- Performance optimization beyond what falls out of having one cached row list.
- Stable contribution trait signatures for outside crates — trait shapes can still change inside this feature; stability comes later once a few real consumers exercise them.
- Migration of any other surface. If a related cleanup tempts you, file a child issue or a future-feature note rather than expanding scope.

## Where to read more

- `docs/NORTH_STAR.md` — mission, bug classes, invariants, proof-of-architecture features.
- `CONTEXT.md` — canonical vocabulary.
- `docs/adr/0001`…`0008` — the decisions this feature implements.
- `./RULES.md` — migration playbook (slice rule, done-check, commit format, stop conditions).
- `./plan.md` — compulsory checklist and operating rule.
- `./issues.json` — tickets, acceptance criteria, north-star checks.
