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

## Relationships

- A **Diff Workspace** is made of **Visual Rows**.
- A **Visual Row** may represent diff text or an **Inline Review Row**.
- A **Diff Decoration** may render as highlighted text, gutter marks, rails, or an **Inline Review Row**.
- A **Side-Filtered Selection** applies within one side of a split **Diff Workspace**.
- The **Diff Workspace Owner** updates the workspace's interactive state as one unit; the app shell asks it to perform user-meaningful actions and executes returned side effects.
- A **Review Workflow Contribution** customizes the review experience by contributing data and effects to the **Diff Workspace Owner**; it does not bypass visual rows, coordinate mapping, or renderer contracts.

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
