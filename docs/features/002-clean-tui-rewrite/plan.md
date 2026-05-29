# Clean Shared-Core Rewrite Plan

## Agent Operating Rule

Before finalizing any response on this project:

- Re-read this file.
- If any item under "Compulsory completion order" is unchecked, continue with the first unchecked item unless a stop condition in `RULES.md` is true.
- Final response must include completed checklist items, verification, and the next `bash scripts/work.sh next` result.

## Compulsory completion order

- [ ] 1. Create the isolated v2 packages, runtime server seam, and TUI/GUI client bases.
- [ ] 2. Render a current diff in the v2 TUI from the local runtime server.
- [ ] 3. Add a termwright smoke test for v2 diff rendering.
- [ ] 4. Add the v2 Diff Workspace owner with private state, Visual-Row Stream, and reducer API.
- [ ] 5. Implement v2 keyboard navigation (`j/k/gg/G/Ctrl-d/u`) against the Visual-Row Stream.
- [ ] 6. Implement v2 scrolling against the same Visual-Row Stream.
- [ ] 7. Implement v2 mouse row mapping against the same Visual-Row Stream.
- [ ] 8. Add context-aware global command/keymap/help architecture.
- [ ] 9. Add consistent `Esc` navigation semantics through the App Shell.
- [ ] 10. Add PR/local diff context separation so actions do not bleed between contexts.
- [ ] 11. Add a GPUI/gpui-component host spike that consumes the same core frame without duplicating product state.
- [ ] 12. Define parity gates for deleting legacy app paths.

## Operating principle

Build vertical slices in v2, not adapters over legacy `App`. Reference old code for behavior and styling; do not import old app modules into v2. Renderer-specific code may exist at the edge, but product behavior must stay in the server-owned core, protocol frames, contributions, intents, and effects.
