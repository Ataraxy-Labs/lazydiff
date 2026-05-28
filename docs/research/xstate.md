# XState — reducer-first, statecharts only when needed

XState is a JavaScript state-management library that spans a spectrum from "simple event stores" to "full statecharts with actors and orchestration." Its lesson for LazyDiff is about *picking the right point on that spectrum* — and not over-engineering.

## The spectrum XState offers

```
simpler ◄──────────────────────────────────────────────────► more powerful
                                                              (more concepts)

  @xstate/store     fromTransition         createMachine       full XState
  ─────────────     ──────────────         ───────────────     ─────────────
  events +          reducer actor          states + events     statecharts:
  transitions       (context, event)       + guards            states, guards,
  + snapshots       → next context         + actions           entry/exit,
                                                                actors,
                                                                orchestration
```

Most app state needs the left side. Reach right only when modal subflows actually have orthogonal states (typing in a search box vs. dragging the mouse vs. confirming a quit).

## What we are importing

### Reducer-first as the default

```
event in ──► reducer(state, event) ──► (new_state, effects)
```

This is the shape we adopted in ADR 0003 as the `DiffWorkspace` update loop:

```
DiffWorkspaceIntent ──► update(&mut state, intent, ctx) ──► Vec<DiffWorkspaceEffect>
```

Pure update + explicit effect list. Effects (persistence, clipboard, network) are returned, not performed inline. The app shell executes them.

Why this matters:

- One place to add behavior (add an intent, handle it, emit effects, test it).
- Testing the workspace = give it intents, assert on state and emitted effects. No mocks of IO.
- Async work returns through effects with generation tokens so stale results can be dropped safely (lesson we also borrow from pi-mono).

### Small explicit state enums for genuinely modal subflows

XState calls these "states." In Rust they are just enums. The diff workspace has a handful:

- Normal vs. Visual vs. VisualLine selection mode (`DiffBufferMode` today).
- Search-prompt mode (typing a query).
- Pending text-object mode (`i` or `a` waiting for a target).
- Inline editor mode (typing a comment).
- Mouse-drag mode (button down vs. up).

These deserve to be enum variants because they have **different allowed transitions and different key handling**. They do not deserve a full statechart runtime.

### Macrosteps and microsteps — a useful framing, not a runtime

XState internally distinguishes:

- **Microstep** — apply one transition: read event, update state, fire actions.
- **Macrostep** — keep applying microsteps until the machine settles (no more eventless / "always" transitions fire).

The framing transfers even without the runtime. In LazyDiff, an input event may produce multiple internal intents in sequence (e.g., a mouse drag both updates selection *and* triggers a scroll-to-keep-cursor-visible). Treat that as: one external event → one macrostep that runs N microsteps and emits a deduplicated effect list. Practically this is just "apply intents until state is stable, then flush effects." No machine library required.

## What we are *not* importing

- **The XState runtime.** No `interpret`, no actor system, no JSON-defined machines.
- **Hierarchical/parallel state composition.** Overkill for our subflows. A flat enum is enough.
- **Service invocation as actors.** We model async via effects + generation tokens, not actor refs.

## How this protects against bug patterns

- "Direct mutation from many call sites" → forbidden, because the only way to change state is to send an intent.
- "IO performed in the middle of state updates" → forbidden, because IO is an effect returned to the shell.
- "Stale async results overwriting fresh state" → guarded by generation tokens carried on the intent → effect → result path.

## Where to look in the source

Under `~/.cache/checkouts/github.com/statelyai/xstate/`:

- `packages/xstate-store/src/store.ts` — minimal event-store shape (the simplest model).
- `packages/core/src/actors/transition.ts` — `fromTransition` reducer actor.
- `packages/core/src/StateMachine.ts` — full statechart machinery (the part we are deliberately not importing).
- The `@xstate/store` README — clearest article on "events + transitions + snapshots" with no statechart concepts.

## The carried-over rules

1. Default to reducer + effects; only escalate to explicit state enums when a subflow truly is modal.
2. Effects are explicit data, not inline calls.
3. Async results carry a generation token so the workspace can ignore stale ones.
4. State is private; the only way in is an intent.

## Refresh

If `~/.cache/checkouts/github.com/statelyai/xstate/` is missing, refresh with:

```sh
bash ~/.agents/skills/librarian/checkout.sh statelyai/xstate --path-only
```

Do not write a new XState claim in this file or in the ADRs without grounding it in the cached source.
