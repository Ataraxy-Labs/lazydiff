# Use a thin app shell with per-surface Rust-owned interaction state

The app shell (`App`) is a thin router + effect runner + global-overlay host. Each interactive surface of the TUI has its own private Rust owner that controls that surface's interaction state through a reducer + effects loop. The Diff Workspace (ADR 0004) is the first instance; other surfaces follow the same shape surface-by-surface.

## Decision

### Two roles only for the app shell

1. **Router**: decide which surface is active and route raw input to it. Manage cross-surface navigation history and global overlays (command palette, file picker, terminal flows).
2. **Effect runner**: execute effects returned by surface reducers (persistence writes, forge calls, clipboard, navigation transitions, async kicks). Forward async results back to the originating surface as intents.

The app shell does **not** own surface interaction state. It does **not** mutate surface-private fields. It does **not** know how a surface's cursor, scroll, selection, or focus works.

### One Rust owner per surface

Each serious surface becomes its own module under `src/app/` with private state and a small public API. Initial surface inventory and current home in code:

| Surface | Current home | Target owner |
|---|---|---|
| Diff Workspace | scattered across `App` + `DiffBufferState` | `src/app/workspace.rs` (ADR 0004) |
| Semantic | `src/app/semantic.rs` + `App` fields | `SemanticSurface` (or `SemanticWorkspace`) |
| Finder / file picker | `src/app/finder.rs` + `App` fields | `FinderSurface` |
| Command palette | `src/components/command_palette.rs` + `App` fields | `CommandPaletteSurface` |
| Review sidebar | `App` fields + render code | `ReviewSidebarSurface` |
| Commit list | `App` fields + render code | `CommitListSurface` |
| Queue / home | `App` fields + render code | `QueueSurface` |
| Comments reader | `App` fields | `CommentsSurface` (may collapse into Diff Workspace) |

Each surface owner exposes:

- `pub fn update(&mut self, intent: SurfaceIntent, ctx: SurfaceContext) -> Vec<AppEffect>` — reducer per ADR 0003.
- `pub fn frame(&self, …) -> SurfaceFrame<'_>` — read-only view for the renderer.
- Internal modal subflows as Rust enums where genuinely modal (search prompt, drag, editor, submit lifecycle).

### Concrete dispatch shape (start simple)

```text
struct AppShell {
    active_surface: AppSurface,
    surfaces: { diff: DiffWorkspace, semantic: SemanticSurface, … },
    globals: { command_palette, file_picker, terminal_flow, … },
    effect_runner: EffectRunner,
}

enum AppSurface { Queue, CommitList, DetailFull, Diff, Comments, Semantic, … }
```

Input dispatch is an enum match. No `trait Surface` is introduced until at least 2–3 surfaces share the same shape and duplication actually hurts. **Concrete enum dispatch first; abstract traits only when proven necessary.**

## Migration shape

This ADR does not authorize a one-shot rewrite. The migration is surface-by-surface, one reviewable slice at a time, in this order (subject to the user's confirmation per surface):

1. Diff Workspace (ADR 0004, in flight; gates everything else by proving the pattern).
2. App shell / router seam — extract the routing + effect runner without yet moving non-diff surface state.
3. Small overlays first — command palette, finder, review sidebar (each is small and pushes the contribution model from ADR 0002).
4. Semantic Workspace — already has its own scroll/selection/expansion state; natural second instance of the pattern.
5. Commit list + Queue/Home — more entangled with forge/auth/caches; later.
6. Persistence + forge become effect-driven (see ADR 0007 for async/generation tokens).

Each slice obeys the AGENTS.md reviewable-slice rule: one concept, deleted old paths, new workspace operation, focused tests, grep proof.

## Constraints

- The app shell never mutates surface-private fields.
- Surfaces never reach into each other's state; cross-surface coordination is an effect that becomes an intent on the receiving surface.
- `lazydiff-diffs` knows nothing about surfaces; it remains a generic crate (ADR 0001).
- No `trait Surface` until 2–3 owners exist and benefit from it.
- No public plugin API; surfaces are internal Rust modules (ADR 0002).

## Consequences

- `App` shrinks. Today `src/app.rs` is ~5,355 lines holding 90+ fields. As surfaces move into their own modules, `App` becomes a routing + effect-running shell of measurable size (rough target: under 1,500 lines, but the slice count matters more than the line count).
- Future agents cannot quietly add a new feature by appending fields to `App`. The natural home for new behavior is an existing surface or a new surface module.
- Tests improve: each surface's reducer is testable in isolation with synthetic intents and asserted effects.
- The shape is reusable but not over-engineered: concrete modules until duplication forces a trait.
