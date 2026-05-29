# Feature 002 — v2 shared-core architecture foundation

This document is the durable version of the rewrite foundation discussed in chat. ADR 0009 makes the clean v2 rewrite active; this document explains the terms, seams, and diagrams that guide the first implementation slices.

The rewrite goal is not to avoid complexity. The goal is to put essential complexity behind deep modules so the app does not re-grow a god object.

The app target is not “a better TUI only.” The target is one local Rust runtime server mounted by multiple renderer clients. The terminal client ships first; a GPUI/gpui-component client consumes the same protocol frames and dispatches the same intents.

## Reference lessons we are carrying forward

### Zed — explicit display transformations

Zed's `DisplayMap` owns display transformations instead of letting renderer/input code compute rows independently. Its editor stack layers buffer text through inlays, folds, tabs, wrapping, blocks, and highlights before rendering. LazyDiff v2 should borrow the **display-map idea**, not Zed's GPUI/editor implementation.

LazyDiff's equivalent:

```diagram
╭────────────────────────────╮
│ DiffDocument               │
│ parsed files/hunks/lines   │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ DiffDisplayMap             │
│ folds, inline rows, wraps   │
│ coordinate conversions      │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ VisualRowStream            │
│ one row list, dirty cached  │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ RenderModel                │
│ viewport paint data only    │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ Renderer Adapter           │
│ TUI cells or GPUI elements  │
╰────────────────────────────╯
```

### GPUI/gpui-component — GUI client, not product fork

GPUI and gpui-component give us windowing, focus, overlay/root layers, scroll handles, and virtual list primitives. gpui-component's `VirtualList` is especially relevant: it renders only the visible item range from a list of item sizes and uses a scroll handle to reveal items. LazyDiff should use that as a host capability, not as the owner of diff semantics.

GUI guidance:

- server-owned core owns `VisualRowIndex`, row identities, cursor, folds, commands, and effects;
- GPUI adapter owns GPUI entities, focus handles, scroll handles, and element construction;
- virtualized GUI rows correspond to core visual rows;
- GPUI actions/key bindings translate into the same protocol commands/intents used by the TUI;
- GPUI overlays/dialogs mount shared command/help data rather than duplicating context rules.

### Pierre — one geometry owner, sparse work

- One owner of row geometry.
- Dirty cache for row rebuilds.
- Sparse row-height overrides for unusual rows.
- Render only the viewport.
- Event-driven redraw; no fake FPS loop for static screens.

### ProseMirror — generic core, contributed behavior

- The core owns generic primitives.
- Product behavior becomes contributed data: commands, decorations, inline rows, fold strategies.
- Renderer consumes typed data, not product-specific branches.

### pi-mono — capability seams, not raw mutation

- Contributions get read-only/capability contexts, never `&mut App` or `&mut Surface`.
- Contribution kinds are fixed by the core.
- UI customization includes commands, keymaps, palette entries, inline rendering, chrome/status slots, and overlays — not just tools.
- Runtime plugin loading is deferred; internal/compile-time Rust contribution shape exists now.

### Rust ownership — bad architecture should be hard to express

- One mutable owner per surface.
- Private state.
- `update(intent) -> effects` is the write door.
- Frames are read-only borrows.
- IO leaves as effects.

## Core terms

### `DiffDocument`

The parsed diff as immutable source data. It owns files, hunks, line numbers, line kinds, paths, and stable row identities. It does **not** own cursor, scroll, mouse, comments, PR state, or terminal size.

### `DiffFile`

One file inside a diff. It owns old/new paths, file status, and hunks.

### `DiffHunk`

One hunk inside a file. It owns old/new ranges and hunk lines.

### `DiffLine`

One raw line inside a hunk. It has a kind (`Context`, `Added`, `Removed`), text, old/new line numbers when present, and stable identity.

### `DocumentRow`

A row in the parsed diff document coordinate space. This is not a screen row and not necessarily a row the user can navigate after folds/inline rows/wraps are applied.

### `VisualRow`

A row the reviewer can perceive and navigate. It may represent a file header, hunk header, diff line, expanded context line, inline row, fold summary, or wrapped continuation.

Example shape:

```rust
enum VisualRow {
    FileHeader { file_id: FileId },
    HunkHeader { hunk_id: HunkId },
    DiffLine { document_row: DocumentRow },
    ContextLine { file_id: FileId, side: DiffSide, line_number: LineNumber },
    InlineRow { inline_id: InlineRowId, visual_line: usize },
    FoldSummary { fold_id: FoldId, hidden_count: usize, label: String },
}
```

### `VisualRowIndex`

An index into the `VisualRowStream`. Keyboard navigation, scrolling, mouse mapping, search, Vim motions, and renderer iteration must agree on this coordinate.

### `VisualRowStream`

The full ordered list of visual rows for current workspace state. It is cached behind a dirty flag and is the only truthful row geometry for the Diff Workspace.

### `DiffDisplayMap`

The owner of transformations from document rows to visual rows. It handles folds, inline rows, optional context, wrapping, row heights, and coordinate conversions.

### `Viewport`

The visible slice of the `VisualRowStream`.

```rust
struct Viewport {
    first_visible: VisualRowIndex,
    height: usize,
    width: usize,
}
```

### `RenderModel`

The paint-ready, viewport-only model consumed by renderer adapters. The renderer should not compute which rows exist or which commands are enabled.

The model should contain semantic row data and stable IDs, not terminal-specific cells. TUI can lower it to styled spans/cells; GPUI can lower it to elements, virtual-list items, and theme colors.

### `RendererClient`

A thin host-specific edge that turns host events into protocol messages and server frames into host paint primitives.

Examples:

- terminal adapter: crossterm/ratatui events and cells;
- GPUI adapter: gpui actions, focus, windows, virtual lists, and elements.

The client may own host resources, but not product state such as review context, diff cursor, command availability, PR/local behavior, or navigation semantics.

### `RuntimeServer`

The local Rust process that owns AppCore and serves protocol frames/events to renderers. It is the single product runtime for TUI and GUI, similar to a local sidecar. It can start as a simple localhost server and grow into streaming events, subscriptions, and richer client requests without moving product behavior into renderers.

### `DiffWorkspace`

The Surface Owner for the diff surface. It owns the document, display map, visual-row stream, cursor, scroll, selection, search, folds, inline rows, and editor state.

It exposes:

```rust
fn update(intent: DiffIntent) -> Vec<Effect>;
fn frame() -> DiffFrame<'_>;
```

### `DiffFrame`

A read-only snapshot/borrow for one render/input cycle. It exposes the data needed to render and resolve commands without allowing mutation.

### `DiffIntent`

A user/system action sent into `DiffWorkspace`.

Examples: move visual rows, scroll, jump to top/bottom, toggle fold, start search, handle mouse at screen point.

### `Effect`

A request for IO or app-level work. Examples: copy to clipboard, load file context, save draft, submit review, open URL. Reducers do not execute effects.

### `AppShell`

The top-level router/effect runner/global-overlay host. It owns the active surface, navigation stack, global overlays, contribution registry, and effect runner. It does not own diff cursor/scroll/selection.

`AppShell` is server-owned shared core. A terminal process and a GPUI window should both drive this same state machine by sending protocol events and reading `AppFrame`s.

### `Surface`

A coherent interactive screen with one owner: Diff Workspace, Command Palette, Finder, PR Overview, Queue, Review Sidebar, etc.

## Whole-app architecture

```diagram
                    local Rust runtime server
╭────────────────────────────────────────────────────────────╮
│                    RuntimeServer + AppCore                  │
│                                                            │
│  AppShell + surfaces + commands + effects + contributions │
╰────────────────────────────────────────────────────────────╯
                              ▲
                              │ protocol events/frames
              ╭───────────────┴────────────────╮
              │                                │
              ▼                                ▼
╭────────────────────────────╮     ╭────────────────────────────╮
│ Terminal Renderer Client    │     │ GPUI Renderer Client        │
│ crossterm/ratatui/termwright│     │ gpui/gpui-component         │
│ cells, terminal input       │     │ elements, focus, windows    │
╰────────────────────────────╯     ╰────────────────────────────╯
```

Inside the shared core:

```diagram
╭────────────────────────────────────────────────────────────╮
│                         AppShell                           │
│                                                            │
│  owns: routing, navigation stack, global overlays, effects │
│  does NOT own: diff cursor, scroll, selection, search       │
╰───────────────┬───────────────────────┬────────────────────╯
                │                       │
                │ routes intents        │ executes effects
                ▼                       ▼
╭────────────────────────────╮   ╭────────────────────────────╮
│     DiffWorkspace           │   │       EffectRunner          │
│                            │   │                            │
│ owns diff surface state     │   │ clipboard, disk, forge, IO │
│ update(intent) -> effects   │   │ async results -> intents   │
╰──────────────┬─────────────╯   ╰────────────────────────────╯
               │
               │ owns
               ▼
╭────────────────────────────╮
│       DiffDocument          │
│ parsed files/hunks/lines    │
│ immutable source data       │
╰──────────────┬─────────────╯
               │
               │ transformed by
               ▼
╭────────────────────────────╮
│       DiffDisplayMap        │
│ document rows -> visual rows│
│ folds, inline rows, wraps   │
│ coordinate conversions      │
╰──────────────┬─────────────╯
               │
               │ produces cached
               ▼
╭────────────────────────────╮
│      VisualRowStream        │
│ Vec<VisualRow>              │
│ dirty cached                │
│ one truth for row geometry  │
╰───────┬─────────┬──────────╯
        │         │
        │         │ read by same frame
        ▼         ▼
╭────────────╮  ╭────────────────────────╮
│ navigation │  │ mouse/scroll/search     │
│ Vim motion │  │ coordinate mapping      │
╰────────────╯  ╰───────────┬────────────╯
                             │
                             ▼
                  ╭─────────────────────╮
                  │     RenderModel      │
                  │ viewport paint data  │
                  ╰──────────┬──────────╯
                             │
                             ▼
                  ╭─────────────────────╮
                  │ Renderer Adapter     │
                  │ TUI or GPUI lowering │
                  ╰─────────────────────╯
```

## Renderer boundary

The renderer boundary is the rule that keeps the GUI honest. The terminal and GPUI clients are allowed to differ in paint and host mechanics only.

```diagram
╭────────────────────────────────────────────╮
│ Host event                                  │
│ key, mouse, scroll, resize, GPUI action     │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ RendererAdapter                             │
│ normalize into protocol event/request       │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ RuntimeServer + shared AppCore              │
│ command context, reducers, effects          │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ AppFrame / SurfaceFrame / RenderModel       │
│ semantic rows, chrome, overlays, commands   │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ RendererClient                              │
│ lower to terminal cells or GPUI elements    │
╰────────────────────────────────────────────╯
```

Renderer-specific ownership is allowed for:

- terminal handles, alternate screen, raw mode, ratatui layout values;
- GPUI entities, windows, focus handles, scroll handles, themes, and `VirtualList` item sizes;
- host-specific measurement caches that do not decide product behavior.

Renderer-specific ownership is not allowed for:

- command availability and help contents;
- PR/local/raw-patch context rules;
- diff cursor, visual row identity, folds, or search state;
- `Esc` semantics and navigation stack policy;
- effect execution policy beyond host IO plumbing.

For GPUI, the intended shape is:

```diagram
╭────────────────────────────╮
│ GPUI Window / Root          │
│ focus, sheets, dialogs      │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ LazyDiffGpuiClient          │
│ owns Entity handles only    │
╰─────────────┬──────────────╯
              │ reads frame / sends protocol events
              ▼
╭────────────────────────────╮
│ RuntimeServer + AppCore     │
│ AppShell + DiffWorkspace    │
╰─────────────┬──────────────╯
              │ frame rows
              ▼
╭────────────────────────────╮
│ gpui_component::VirtualList │
│ visible range -> row elems  │
╰────────────────────────────╯
```

## Diff row pipeline

```diagram
╭────────────────────────────────────────────╮
│ Unified diff text                           │
│ from file, stdin, git, GitHub, etc.         │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ Parser                                      │
│ text -> DiffDocument                        │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ DiffDocument                                │
│ File 0                                      │
│   Hunk 0                                    │
│     Context / Removed / Added lines         │
│ File 1                                      │
│   Hunk 0                                    │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ DiffDisplayMap                              │
│ inserts:                                    │
│   file header rows                          │
│   hunk header rows                          │
│   inline rows                               │
│   fold summaries                            │
│   wrapped continuation rows later           │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ VisualRowStream                             │
│ 0 FileHeader                                │
│ 1 HunkHeader                                │
│ 2 DiffLine removed                          │
│ 3 DiffLine added                            │
│ 4 InlineRow comment line 0                  │
│ 5 InlineRow comment line 1                  │
│ 6 DiffLine context                          │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ Viewport                                    │
│ first_visible = 2                           │
│ height = 4                                  │
│ visible = rows[2..6]                        │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ RenderModel                                 │
│ screen row 0 -> VisualRow 2                 │
│ screen row 1 -> VisualRow 3                 │
│ screen row 2 -> VisualRow 4                 │
│ screen row 3 -> VisualRow 5                 │
╰────────────────────────────────────────────╯
```

## Optional full-file context

Users should be able to expand beyond diff hunks, but that must fit the same display map rather than becoming a second renderer mode.

Two cases:

1. Context already in the patch.
2. Context not in the patch and must be loaded from local git, working tree, GitHub blobs, or cache.

The diff core must not perform IO. It requests context through effects/capabilities.

```diagram
╭────────────────────────────╮
│ DiffDocument                │
│ parsed patch hunks only     │
╰─────────────┬──────────────╯
              │ optional context loaded
              ▼
╭────────────────────────────╮
│ FileContextStore            │
│ base text / head text       │
│ per file, per side          │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ RegionPlanner               │
│ patch hunks + expanded code │
│ hidden unchanged ranges     │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ FoldState / FoldStrategy    │
│ collapsed/expanded context  │
╰─────────────┬──────────────╯
              ▼
╭────────────────────────────╮
│ DiffDisplayMap              │
│ creates VisualRowStream     │
╰────────────────────────────╯
```

Full-file context commands should be enabled only when the active `WorkspaceKind` has a provider:

- local diff: git blob / working tree provider;
- pull request: forge blob provider;
- raw patch: often unavailable.

## pi-mono-shaped contribution architecture from day one

The first v2 slice should include the contribution **shape**, even if most contribution lists are initially empty. This prevents global shortcuts, help, chrome, and context-specific actions from becoming scattered match statements.

### Contribution kinds

The registry starts with the ADR 0002/0008 kinds:

- `CommandContribution`
- `KeymapContribution`
- `CommandPaletteEntry`
- `InlineRowProducer`
- `DecorationProducer`
- `ReviewAction`
- `ChromeSlotContribution`
- `ViewModelSlice`
- `FoldStrategy`
- `Effect` descriptions
- Forge/integration capabilities

### `ContributionRegistry`

```rust
struct ContributionRegistry {
    commands: Vec<CommandContribution>,
    keymaps: Vec<KeymapContribution>,
    chrome_slots: Vec<ChromeContribution>,
    inline_rows: Vec<InlineRowContribution>,
    decorations: Vec<DecorationContribution>,
    fold_strategies: Vec<FoldStrategyContribution>,
}
```

### `CommandContext`

Used to prevent PR/local/raw-patch UX from bleeding together.

```rust
struct CommandContext {
    active_surface: SurfaceId,
    workspace_kind: WorkspaceKind,
    mode: SurfaceMode,
    capabilities: CapabilitySet,
}

enum WorkspaceKind {
    PatchFile,
    LocalDiff,
    PullRequest,
    CommitDiff,
}
```

### Shortcut resolution

```diagram
╭────────────────────────────────────────────╮
│ Raw key event                               │
│ e.g. ?, Esc, j, Cmd+P                       │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ AppShell key resolver                       │
│ checks global overlays first                │
│ then global commands                        │
│ then active surface commands                │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ CommandRegistry                             │
│ command id + context + enabled predicate    │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ CommandContext                              │
│ LocalDiff? PR? Queue? Palette?              │
╰──────────────────┬─────────────────────────╯
                   ▼
╭────────────────────────────────────────────╮
│ Dispatch                                    │
│ app command -> AppShell intent              │
│ diff command -> DiffWorkspace intent        │
│ palette command -> CommandPalette intent    │
╰────────────────────────────────────────────╯
```

Day-one built-ins can be tiny:

- `app.quit`
- `app.open_context_help`
- `diff.scroll_down`
- `diff.scroll_up`
- header/footer chrome hints

But they must be registered, not hard-coded directly in the renderer or workspace.

## Vim behavior shape

Vim behavior is a command/intents layer over the same visual row stream.

```diagram
╭────────────╮
│ key: j     │
╰─────┬──────╯
      ▼
╭──────────────────────────╮
│ Keymap                   │
│ j -> diff.move_visual +1 │
╰─────┬────────────────────╯
      ▼
╭──────────────────────────╮
│ DiffIntent               │
│ MoveVisualRows { +1 }    │
╰─────┬────────────────────╯
      ▼
╭──────────────────────────╮
│ DiffWorkspace             │
│ cursor VisualRowIndex +=1 │
│ scroll keeps visible      │
╰─────┬────────────────────╯
      ▼
╭──────────────────────────╮
│ VisualRowStream           │
│ same rows renderer uses   │
╰──────────────────────────╯
```

Later Vim motions (`w`, `b`, `e`), visual mode, yanking, search, and text objects should add commands/intents against the workspace, not new key handling paths.

## Initial v2 workspace map

```text
packages/lazydiff-v2-protocol/ # semantic frames and client/server messages
packages/lazydiff-v2-diff/     # parser, document, visual rows, display map, diff frames
packages/lazydiff-v2-core/     # AppCore, AppShell shape, commands, contributions, effects

apps/lazydiff-v2-server/       # local runtime server
apps/lazydiff-v2-tui/          # terminal client/renderer only
apps/lazydiff-v2-gui/          # GPUI client/renderer only
```

The first implementation may keep some modules small or skeletal, but the seams should exist from day one. Separate packages are justified here because they enforce the server/client dependency direction and keep GPUI dependencies out of protocol, diff, core, server, and TUI builds.

## First milestone

Milestone 1 is **static but architecturally real diff view**:

- parse a real patch in the server-owned core;
- create `DiffWorkspace`/`AppCore`;
- serve `AppFrame`/`DiffFrame` over the local protocol;
- renderer clients paint only protocol frame data;
- no renderer coordinate math;
- terminal and GPUI renderers do not own product state;
- protocol boundary is explicit enough that both clients lower the same frame;
- minimal built-in contribution registry exists;
- unit tests cover parser smoke, visual rows, and viewport slicing;
- termwright smoke test follows immediately.
