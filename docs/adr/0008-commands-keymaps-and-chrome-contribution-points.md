# Commands, keymaps, and chrome are first-class internal contribution points

LazyDiff's extensibility ambition ("build your own diff / build your own code review") starts as **internal** contribution points, not a public plugin runtime (ADR 0002). This ADR fixes the smallest viable set of contribution kinds that every surface owner must support, so that adding a new command, keybinding, status chip, or inline row does not mean editing five files and a god struct.

## Decision

### Six contribution kinds for the first migration

Every surface owner participates in a shared contribution model with these kinds:

1. **Command** — a named, parameterized, surface-scoped (or global) operation. Always describable as a `SurfaceIntent` (or `AppIntent`). Commands have an id, a human label, an optional icon, a scope (which surface(s) they apply to), and an enablement predicate over a read-only `SurfaceContext`.

2. **Keymap entry** — binds a key chord (in a given mode/surface) to a command id with optional arguments. Keymaps are layered: built-in defaults, then user overrides, then surface-local entries. Conflicts resolve by most-specific-wins (surface beats global; user beats default).

3. **Command palette entry** — derived from commands by default; surfaces may contribute custom entries (e.g. dynamic file/PR results). The palette is a global surface that calls into the active surface's command set via the read-only context.

4. **Inline row producer / decoration producer** — per ADR 0001, surfaces accept generic decorations and inline rows from contributions. A producer is a pure function from `(workspace frame, contribution state) -> Vec<Decoration | InlineRow>`. Producers do not mutate; they are sampled when the visual-row stream is rebuilt (ADR 0005).

5. **Chrome slot** — named regions of the screen a contribution can fill: status line segments, header chips, side panels' tabs, footer hints. Each slot has a fixed type signature (e.g. status segment = text + style + width hint). The renderer composes registered slot contributions in a defined order.

6. **Fold strategy** — produces candidate folds for a Diff Workspace (per ADR 0005, folds are first-class on the Visual-Row Stream because they change *what rows exist*). A `FoldStrategy` is a pure function `(workspace frame, contribution state) -> Vec<FoldCandidate>`. Each candidate declares a row range, label, default state (collapsed/expanded), and reason tag. Examples shipped as built-in strategies: unchanged-context, generated-file (lockfiles, snapshots), imports, whitespace-only-hunk, reformatting-only. The workspace owner is the only thing that mutates fold state; strategies only propose.

### Contributions are registered, not subclassed

Contributions are values registered with a `ContributionRegistry` at app startup (and per-surface for surface-local ones). They are not implemented by inheriting from a base class or trait object soup. Where a trait is needed (e.g. `InlineRowProducer`), it has one method and a clear lifetime story.

### Contributions never get `&mut App` or `&mut Surface`

A contribution receives a **read-only context** (`SurfaceContext` / `AppContext`) and returns either a value (decoration, inline row, status segment) or a description of an intent to dispatch. The reducer applies the intent. This is the pi-mono lesson: capabilities, not ownership.

### Scope and IDs

- IDs are namespaced strings: `diff.next_hunk`, `review.submit`, `palette.open`, `chrome.status.branch`, etc.
- Scopes: `global`, or one or more `SurfaceId`s.
- Enablement: a predicate over `SurfaceContext` evaluated each frame (cheap; no IO).

### First built-in registry

The first migration ships the registry with built-in defaults only — every existing keybinding, command, status chip, and inline row becomes an internal contribution. No third-party loading. This proves the seam.

## Why now, why this small

- The registry is the thing that makes "build your own diff" credible without a plugin runtime: internal features are written the same way external ones eventually would be.
- Five kinds cover ~all of what `pi-config/extensions/custom-renderer.ts` actually customizes (commands, keybindings, palette, inline rendering, chrome). More kinds (themes, language servers, agent loops, integrations) are out of scope for this slice.
- It forces surfaces to declare their command surface and chrome surface explicitly, instead of hiding them in renderer code or `App` methods.

## Consequences

- Adding a new keybinding does not mean editing `App::handle_key`; it means adding a keymap entry + command in the surface module.
- Adding a status chip does not mean editing the renderer; it means registering a chrome slot contribution.
- Command palette is no longer a special case; it's a consumer of the registry.
- Tests: registry behavior (override resolution, scoping, enablement) is unit-testable; per-surface command sets become testable contracts.
- External plugin API remains explicitly deferred (ADR 0002). If/when it ships, it will be a thin marshaling layer over this same registry.

## Out of scope

- Theming, color tokens, layout DSL — separate decision when needed.
- Hot-reload of contributions at runtime.
- Sandboxed execution, capability tokens for third-party code.
- Agent-authored review workflows beyond what an internal contribution can already express.
