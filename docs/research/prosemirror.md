# ProseMirror — generic core, behavior via plugins

ProseMirror is a rich-text editor framework. Its enduring lesson for LazyDiff is *not* about text editing — it is about how to keep a powerful core small and let product behavior live outside it without forking.

## What ProseMirror separates

| Module | Owns |
|---|---|
| `prosemirror-model` | Document tree, node types, schema. No behavior. |
| `prosemirror-state` | `EditorState`, transactions, selection. Reducer-style updates. |
| `prosemirror-view` | DOM rendering of the document + decorations. No state mutation. |
| `prosemirror-transform` | Generic transformations (replace, mark, structural edits). |
| `prosemirror-commands` | Named editor commands. Composable. |
| `prosemirror-keymap` | Maps keys → commands. Just data. |
| `prosemirror-history` | Undo/redo as a plugin, not built into core. |
| `prosemirror-example-setup` | Glue showing how to assemble the above. Not reusable. |

The split that matters: **model/state/view never know what your product is.** Lists, tables, collaboration, comments, mentions — every one of those is a plugin that contributes nodes, decorations, commands, and keymaps. The core never grew a "comment system" or a "table system."

## Decorations — the primitive we steal

A `Decoration` in ProseMirror is a *visual annotation* attached to a range or position in the document. It is **not** a comment, not a highlight, not a draft marker — those are product-specific. A decoration is the generic primitive; products turn their state into decorations and hand them to the view.

```
product state         decoration                  view
─────────────────     ─────────────────────       ─────────────────
"this PR comment   →  Decoration.widget(pos)  →   renders inline UI
 should show         (returns a DOM node)
 inline at line 42"
```

The view does not know "PR comment." It knows "render a widget here." That's why ProseMirror can host comments, mentions, suggestions, code suggestions, AI assistants — they all become decorations.

This is the model behind LazyDiff's `Diff Decoration` term in `CONTEXT.md`. A draft, a note, a thread summary, a CI marker, an AI suggestion — all become *generic* decorations or inline review rows. The renderer does not learn product meaning.

## Commands and keymaps — separation of intent from binding

A command in ProseMirror is `(state, dispatch?) => boolean`. It either applies a transaction or returns false. Keys are bound to commands via a keymap plugin.

The win: **the command is the intent.** It can be invoked by a key, a menu, a slash-command, a tool, an agent, a test. The binding is just data. New review workflows in LazyDiff can contribute commands and keymaps without touching the diff core.

This is the model behind `DiffWorkspaceIntent` in ADR 0003 and the "Review Workflow Contribution" concept in ADR 0002.

## What we are *not* copying

- **The JS plugin runtime.** ProseMirror's plugin system uses live runtime registration. LazyDiff phase one uses Rust compile-time internal contributions; a runtime plugin API is a separate later question.
- **The DOM view layer.** Ratatui already paints cells; we keep our own renderer.
- **The full document tree model.** Diff text is line-structured, not a recursive node tree. Visual rows are a flat list, not a tree.

## The carried-over rules

1. Core knows nothing about product meaning.
2. Product state turns into generic visual annotations.
3. User-meaningful actions are commands (intents), not direct mutations.
4. Key bindings are data that maps keys to commands.
5. Adding a feature (history, comments, AI) should usually be additive — a new plugin/contribution, not a core change.

## Where to look in the source

Under `~/.cache/checkouts/github.com/ProseMirror/`:

- `prosemirror-state/src/state.ts` — `EditorState`, `apply(tr)`, plugins array.
- `prosemirror-view/src/decoration.ts` — `Decoration.widget`, `Decoration.inline`, `Decoration.node`.
- `prosemirror-commands/src/commands.ts` — command signatures.
- `prosemirror-keymap/src/keymap.ts` — keymap plugin.
- `prosemirror-example-setup/src/index.ts` — assembly, not architecture; useful as a "what gets composed at the edges" reference.
