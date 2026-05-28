# Lazydiff Context

Lazydiff is a terminal-first code-review workspace for reading diffs, tracking review attention, and leaving private or pull-request review notes without leaving the terminal. Long term, LazyDiff should let teams build their own diff and code-review workflows without forking the safe core.

## Language

**Diff Workspace**:
The interactive review surface where a diff, cursor, selection, search, inline review rows, and mouse/keyboard navigation behave as one coherent screen.
_Avoid_: Diff viewer, diff pane, renderer, screen when referring to the full interactive surface.

**Visual Row**:
A row the reviewer can perceive and navigate in the diff workspace, whether it comes from diff text or an inline review row.
_Avoid_: Line, document row when the row may be a rendered inline review row.

**Side-Filtered Selection**:
A split-view selection that belongs only to the left or right side of the diff and must not highlight the opposite side.
_Avoid_: Cross-pane selection, mirrored selection.

**Inline Review Row**:
A visual row embedded in the diff workspace for a review note, thread summary, full thread, or editable draft.
_Avoid_: Modal, bottom preview, side panel for normal review flow.

**Diff Decoration**:
A generic visual annotation attached to a diff side, line, range, or visual-row position without deciding the product meaning of that annotation.
_Avoid_: Note, comment, draft when discussing the reusable rendering primitive.

**Diff Workspace Owner**:
The Rust module that has exclusive mutable ownership of the diff screen's interactive state: cursor, scroll, selection, search focus, inline review focus, draft editor focus, thread expansion, and mouse drag state.
_Avoid_: Any pattern where the app shell or renderer directly mutates those fields.

**Review Workflow Contribution**:
A bounded way to add or customize code-review behavior — commands, keybindings, inline rows, decorations, review actions, chrome/status, or external effects — without taking ownership of diff coordinate math or workspace interaction state.
_Avoid_: Plugin when discussing the current internal seam; arbitrary renderer mutation; direct `App` mutation.

**App Shell**:
The thin top-level module that routes input to the active surface, runs effects, and hosts global overlays. It does not own or mutate any surface's interactive state.
_Avoid_: God struct, main controller, app state for the routing/effect role.

**Surface**:
A coherent interactive screen of the TUI — Diff Workspace, Semantic, Finder, Command Palette, Review Sidebar, Commit List, Queue/Home. Each surface has one Rust-owned reducer.
_Avoid_: Screen, page, view when discussing the owner of interactive state.

**Surface Owner**:
The Rust module that has exclusive mutable ownership of a surface's interactive state and exposes `update(intent) -> Vec<effect>` and a read-only frame. The **Diff Workspace Owner** is the first instance.
_Avoid_: Direct field access from the **App Shell** or renderer.

**Surface Intent**:
A user-meaningful or system-meaningful message handed to a **Surface Owner** by the **App Shell**. Inputs are translated into intents; async results return as intents.
_Avoid_: Calling mutator methods directly; passing raw key events into surface internals.

**Surface Effect**:
A description of work the **App Shell**'s effect runner should perform — persistence write, forge call, clipboard, navigation, async kick. Returned by a reducer; never executed inside one.
_Avoid_: Side effects inside reducers; tasks spawned from surface code.

**Chrome Slot**:
A named region of the screen (status segment, header chip, footer hint, side-panel tab) that contributions can fill with a typed value. The renderer composes registered slot values in defined order.
_Avoid_: Renderer-side conditional product UI; hard-coded status strings.

**Contribution**:
A registered value (Command, Keymap entry, Command palette entry, Inline row producer, Decoration producer, Chrome slot value) that a surface or the shell consumes. Contributions receive read-only context and produce data or intents; they never receive `&mut App` or `&mut Surface`.
_Avoid_: Subclassing, ad-hoc hooks, contributions that mutate state directly.

**Generation Token**:
A `(surface, kind, value)` token attached to every async effect. When the result returns, the surface drops it if the current generation no longer matches, preventing stale async data from overwriting the live surface state.
_Avoid_: Applying async results without a token check; relying solely on cooperative cancellation.

**Visual-Row Stream**:
The single cached list of **Visual Rows** the **Diff Workspace Owner** rebuilds on demand and lends to all consumers (navigation, scrolling, mouse mapping, renderer iteration) for one frame. Backed by a dirty flag; sparse row-height overrides keep it cheap. Fold-aware: when a **Fold** is collapsed, its hidden rows are removed from the stream and replaced by a single **Fold Summary Row**.
_Avoid_: Recomputing rows per-consumer; storing parallel row counts; renderer asking the document directly.

**Fold**:
A collapsed range of **Visual Rows** in the **Diff Workspace** represented by a single **Fold Summary Row** in the **Visual-Row Stream**. Folds change *what rows exist*, not just how they are decorated, which is why they are first-class on the stream rather than a kind of **Diff Decoration**.
_Avoid_: Modeling collapse as a render-time hack; storing fold state outside the workspace owner.

**Fold Summary Row**:
The single **Visual Row** that stands in for a collapsed **Fold**. Carries the fold's label, the count of hidden rows, the fold's `FoldStrategy` id, and any contributed summary text (e.g., "12 lines of imports", "auto-folded: package-lock.json").
_Avoid_: Multiple summary rows per fold; rendering hidden rows behind the summary; placing the summary outside the stream.

**FoldStrategy**:
A **Contribution** that produces candidate **Folds** for a Diff Workspace. Examples: unchanged-context fold, generated-file fold (lockfiles, snapshots), imports fold, whitespace-only-hunk fold, reformatting fold, AI-risk-driven fold. A FoldStrategy is a pure function from `(workspace frame, contribution state) -> Vec<FoldCandidate>`; the workspace owner decides which candidates apply and what default state (collapsed/expanded) they have.
_Avoid_: FoldStrategies that mutate workspace state; folds that bypass the **Visual-Row Stream**; per-strategy renderer code.

## Relationships

- A **Diff Workspace** is made of **Visual Rows**.
- A **Visual Row** may represent diff text or an **Inline Review Row**.
- A **Diff Decoration** may render as highlighted text, gutter marks, rails, or an **Inline Review Row**.
- A **Side-Filtered Selection** applies within one side of a split **Diff Workspace**.
- The **Diff Workspace Owner** updates the workspace's interactive state as one unit; the app shell asks it to perform user-meaningful actions and executes returned side effects.
- A **Review Workflow Contribution** customizes the review experience by contributing data and effects to the **Diff Workspace Owner**; it does not bypass visual rows, coordinate mapping, or renderer contracts.
- The **App Shell** routes input to the active **Surface** as a **Surface Intent** and runs returned **Surface Effects**; it never mutates surface-private fields.
- A **Surface Owner** updates its private state from one **Surface Intent** at a time and returns zero or more **Surface Effects**; the **Diff Workspace Owner** is the first concrete instance.
- A **Contribution** is consumed by a **Surface Owner** or the **App Shell** to fill a **Chrome Slot**, register a command/keymap entry, or produce inline rows/decorations for the **Visual-Row Stream**.
- Every async **Surface Effect** carries a **Generation Token**; results return as intents and are dropped when the surface's generation has moved on.
- All four diff consumers (navigation, scrolling, mouse mapping, renderer iteration) read the same **Visual-Row Stream** from the **Diff Workspace Owner**.
- A **Fold** is a first-class operation on the **Visual-Row Stream**, not a **Diff Decoration**; toggling it dirties the cache so the stream rebuilds with hidden rows replaced by a **Fold Summary Row**.
- A **FoldStrategy** is a **Contribution** that proposes **Folds**; the **Diff Workspace Owner** accepts, rejects, or merges proposals and is the only thing that mutates fold state.

## Example dialogue

> **Dev:** "Should a draft comment be handled by a modal that floats above the diff?"
> **Domain expert:** "No — it is an **Inline Review Row** inside the **Diff Workspace**, so navigation, scrolling, and rendering all see it as a **Visual Row**."

> **Dev:** "Is this multi-line note a comment feature or a renderer feature?"
> **Domain expert:** "The product note belongs to the **Diff Workspace**; the reusable visual primitive is a **Diff Decoration**."

> **Dev:** "Can the app set cursor row, clear inline focus, and adjust scroll separately?"
> **Domain expert:** "No — those belong to the **Diff Workspace Owner**, so one update keeps navigation, focus, selection, and visual rows consistent."

> **Dev:** "Can a team build its own review workflow with custom markers, actions, and chrome?"
> **Domain expert:** "Yes — that should be a **Review Workflow Contribution** that feeds the **Diff Workspace**, not a fork that mutates renderer math or app state directly."

## Flagged ambiguities

- "diff viewer" has been used for both the renderer and the full interactive surface — resolved: use **Diff Workspace** for the interactive surface and renderer only for painting cells.
- "decoration" should not imply a review note — resolved: use **Diff Decoration** for the reusable visual primitive and **Inline Review Row** for embedded review UI.
- "ownership" should mean a Rust-enforced mutable boundary, not just a team convention — resolved: use **Diff Workspace Owner** for the module allowed to mutate diff-screen interaction state.
- "plugin" can imply arbitrary third-party runtime code — resolved for the current migration: use **Review Workflow Contribution** for bounded internal/customization seams while keeping the public plugin question separate.
