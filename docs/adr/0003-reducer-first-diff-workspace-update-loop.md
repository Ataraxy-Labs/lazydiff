# Use a reducer-first Diff Workspace update loop

The **Diff Workspace** will use a reducer-style update loop as its default interaction architecture:

```text
Input event -> DiffWorkspaceIntent -> update(&mut state, intent, context) -> Vec<DiffWorkspaceEffect>
```

The app shell translates raw keyboard and mouse input into **Diff Workspace Intents**. The **Diff Workspace** owns product interaction state updates for cursor movement, scrolling, side-filtered selection, inline focus, draft editor movement, thread focus, and mouse selection. The app shell executes returned **Diff Workspace Effects** such as persistence, clipboard writes, review submission, external navigation, and other IO.

## Reference

This decision is informed by XState's split between simple event-based stores and full statecharts:

- `@xstate/store` models simple state management as events, transitions, snapshots, and queued effects.
- XState `fromTransition` models reducer-like actor logic where an event transforms current context into next context.
- Full XState machines/statecharts add explicit states, guards, entry/exit actions, actors, and orchestration for more complex workflows.

LazyDiff will import the architectural lesson, not the full runtime: use simple event/reducer flow for the common path, and introduce small explicit Rust state enums only for genuinely modal subflows.

## Consequences

- Most diff behavior gets one obvious agent-friendly edit point: add an intent, update state, emit effects, test it.
- The app shell no longer patch-fixes diff behavior by directly mutating workspace-owned state.
- Effects are explicit and testable because state updates return effect requests instead of performing IO inline.
- Modal subflows such as comment editing, search prompt, pending text object input, mouse drag, and submit lifecycle may use explicit state-machine-shaped enums inside the reducer.
- LazyDiff does not build or expose a full XState-like plugin/statechart runtime at this stage.
