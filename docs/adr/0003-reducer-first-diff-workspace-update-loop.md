# Use a reducer-first surface update loop

The **Diff Workspace** — and, by extension, every other app-level surface owner (see ADR 0006) — will use a reducer-style update loop as its default interaction architecture:

```text
Input event -> SurfaceIntent -> update(&mut state, intent, context) -> Vec<SurfaceEffect>
```

The app shell translates raw keyboard and mouse input into **Surface Intents**. The surface owner mutates its own private state (cursor, scroll, selection, focus, modal subflow state) inside `update`. The app shell executes returned **Surface Effects** such as persistence writes, clipboard, forge calls, review submission, navigation, and other IO.

## Reference

This decision is informed by XState's split between simple event-based stores and full statecharts:

- `@xstate/store` models simple state management as events, transitions, snapshots, and queued effects.
- XState `fromTransition` models reducer-like actor logic where an event transforms current context into next context.
- Full XState machines/statecharts add explicit states, guards, entry/exit actions, actors, and orchestration for more complex workflows.

LazyDiff imports the architectural lesson, not the full runtime: use simple event/reducer flow for the common path, and introduce small explicit Rust state enums only for genuinely modal subflows.

## Decision

- Reducers are pure with respect to IO: they mutate state and return effects. They do not perform clipboard, disk, network, or terminal IO inline.
- Effects are explicit values (one variant per effect kind) executed by the app shell's effect runner.
- Modal subflows (search prompt, pending text object, inline editor, mouse drag, submit lifecycle, file picker) use explicit Rust enums with allowed transitions inside the reducer.
- A single input event may produce multiple internal intents in sequence (one "macrostep" of multiple "microsteps"); the reducer applies them until state is stable, then flushes a deduplicated effect list.
- Redraw is signaled via a dirty flag flipped by the reducer; the event loop coalesces signals once per loop iteration. The loop is event-driven; the app does not poll a frame clock.
- Async effects must carry a generation token so stale results can be rejected by the reducer (see ADR 0007).

## Consequences

- Most surface behavior gets one obvious agent-friendly edit point: add an intent, update state, emit effects, test it.
- The app shell no longer patch-fixes surface behavior by directly mutating surface-owned state.
- Effects are explicit and testable because state updates return effect requests instead of performing IO inline.
- Reducer + effect testing requires no mocks; tests assert on (new state, returned effects).
- Modal subflows such as comment editing, search prompt, pending text object input, mouse drag, and submit lifecycle use explicit state-machine-shaped enums inside the reducer.
- The same shape applies to other surfaces (semantic, finder, command palette, commit list, queue/home) when they migrate per ADR 0006.
- LazyDiff does not build or expose a full XState-like plugin/statechart runtime at this stage.
