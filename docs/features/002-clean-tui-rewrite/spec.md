# Feature 002 — Clean Shared-Core Rewrite

Feature 002 replaces the in-place migration track with a clean, isolated v2 runtime server, shared packages, and v2 diff foundation. The TUI is the first renderer; a GPUI/gpui-component GUI must consume the same server protocol and core semantics instead of reimplementing product behavior.

## What this feature is

Build LazyDiff again from scratch with the architecture we now know we need:

- one local Rust server that owns app state, commands, effects, workspaces, contributions, and diff view models;
- renderer-agnostic Rust packages for protocol, diff, and core runtime semantics;
- thin renderer clients for terminal and GPUI/gpui-component frontends;
- a thin App Shell that owns routing, global shortcuts, global overlays, and effect execution;
- per-surface owners with private state and `update(intent) -> effects` APIs;
- a clean Diff Workspace first, backed by a v2 diff crate and one Visual-Row Stream;
- context-aware command/keymap/`?` help contributions so PR context, local diff context, and future contexts do not leak UX into each other;
- consistent navigation semantics, including `Esc`, across surfaces and overlays;
- termwright coverage for observable behavior from the start.

## Why it exists

The old app works in places but is globally inconsistent. Shortcuts are not truly global, context-specific actions appear in the wrong contexts, `Esc` is inconsistent, and many behaviors depend on scattered direct mutation. Feature 001 showed that field-by-field migration can add adapter code before deleting enough legacy code.

The rewrite keeps the hard-won product knowledge, tests, and visual standards, but stops treating the old `App` as the target architecture.

## Scope

In:

- New isolated v2 packages under `packages/lazydiff-v2-{protocol,diff,core}`.
- New isolated v2 apps under `apps/lazydiff-v2-{server,tui,gui}`.
- First tracer bullet: start a local server for a patch and render the current diff frame in the new TUI and GPUI clients.
- Protocol/client boundary documentation and code shape that allows TUI and GPUI/gpui-component hosts to consume the same frames and dispatch the same intents.
- Then build the Diff Workspace solidly before app layers.
- Later app layers: global shortcuts, command palette, help, navigation stack, contexts, PR/local workflows, persistence/effects.

Out for the first tracer bullet:

- GitHub/forge integration.
- Persistence writes.
- Full parity with legacy app.
- Public plugin runtime.
- Production GUI implementation; the first milestone only keeps the seam honest.

## Success criteria

- `lazydiff-v2-server` can load a patch and serve a semantic frame over localhost.
- `lazydiff-v2` can render a diff frame from that local server.
- `lazydiff-gui-v2` can consume the same server frame in GPUI.
- The v2 diff package owns parsed diff data and exposes a small rendering-oriented interface.
- Product behavior is represented in server-owned core/protocol frames, not TUI-only or GUI-only branches.
- The v2 TUI has no dependency on legacy `src/app.rs` or `src/app/*` modules.
- Every v2 behavior slice has a focused test; TUI-observable behavior uses termwright.
- Legacy code is deleted only after v2 parity gates prove replacement behavior.

## Where to read more

- `ARCHITECTURE.md` — v2 vocabulary, diagrams, display-map foundation, optional full-file context, and pi-mono-shaped contribution seams.
- `docs/adr/0009-clean-tui-rewrite.md` — rewrite decision and pointer back to this feature.
