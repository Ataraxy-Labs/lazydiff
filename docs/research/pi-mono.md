# pi-mono — bounded extension capabilities, not raw app mutation

`pi-mono` is the monorepo for the `pi` agentic CLI. It is the closest reference point we found for an extensible terminal-first product. Its lesson for LazyDiff is about *how* extension surfaces should be shaped so the core stays safe.

## What pi does well

The pi core is intentionally minimal. Functionality that does not belong in the core is built as **extensions** that register through controlled contribution points:

| Contribution point | Example |
|---|---|
| Tools | new MCP-style or local tools |
| Commands | slash commands |
| Keyboard shortcuts | named bindings |
| CLI flags | new launch options |
| Message renderers | custom rendering for assistant/tool/user turns |
| Lifecycle hooks | on startup, on message, on shutdown |
| Resource loading | skills, prompts, themes |
| UI/context APIs | extensions get a *context object*, not the app |

The crucial pattern: **extensions never get `&mut App`.** They get a context/capability object that exposes exactly what they are allowed to do. The core decides what's exposed; the extension cannot reach past it.

## What `pi-config` showed us about real extensibility

The user's own `pi-config` extension (`extensions/custom-renderer.ts`) demonstrated that extensions in practice want to customize *much more* than tools:

- compact tool rendering
- command palette
- fixed editor
- footer / status chrome
- working indicator
- message rendering
- tool rendering
- clickable markers
- overlays / selectors
- renderer toggle
- keyboard shortcuts
- layout / compositor behavior

**Conclusion for LazyDiff:** "Review Workflow Contribution" must include more than just commands and effects. It needs view-data slots, chrome/status slots, inline-row producers, decoration producers, and review action surfaces. This is exactly what ADR 0002 now scopes.

## The pattern we are importing

```
┌──────────────────────────────────────────────────┐
│ LazyDiff core (Rust-owned)                       │
│   diff math, visual rows, selection, rendering   │
└────────────────────────┬─────────────────────────┘
                         │ exposes capability surfaces
┌────────────────────────┴─────────────────────────┐
│ Review Workflow Contribution                     │
│  Marker · Command · Keymap · InlineRow · Action  │
│  ChromeSlot · ViewModelSlice · Effect            │
│                                                  │
│ Sees: ReviewContext { selection, file, frame_id }│
│ Never sees: &mut DiffWorkspace, &mut App         │
└──────────────────────────────────────────────────┘
```

A contribution declares what it contributes; the workspace decides when and how to invoke it. The contribution is a *consumer of a context*, not an *owner of state*.

## Concrete TUI techniques worth borrowing

### Clickable markers via ANSI escape sequences

pi embeds clickable identifiers in terminal output using ANSI APC-style escapes such as `\x1b_pi:activity:<id>\x1b\\`. The terminal still renders the visible text normally; the host can detect clicks on the wrapped region and route them to an action. For LazyDiff this is the right primitive for **clickable file headers, hunk markers, comment-resolve buttons, and inline review actions** without inventing a new in-band protocol or rebuilding terminal-mouse mapping per feature.

### Fixed-editor / inline-editor patterns

pi's `FixedEditorCluster` shows the small set of operations a real inline editor needs:

- `normalizeLines(input)` — turn raw text into a list of editor-visible lines (handles wrapping, tabs, control chars).
- `takeTail(lines, max)` — for "scroll to bottom while typing" behavior.
- `capEditorLines(lines, cursor, max)` — keep the cursor visible inside a bounded editor row count.
- `extractCursor(rendered)` — find the cursor position in already-rendered output (so the host can place the terminal cursor).

These are the same four problems LazyDiff's inline comment editor will solve. Port the *shape* of these helpers into the Diff Workspace's editor subflow; do not depend on pi's specific JS implementation.

### Named TUI UI primitives

pi-config uses small composable primitives — `Container`, `Spacer`, `TruncatedText`, `hyperlink` — instead of ad-hoc string formatting. LazyDiff already has some of this in `src/ui/`. The lesson is to keep these primitives **product-meaning-free**: a `TruncatedText` is not a "file path"; a `Container` is not a "comment box." Naming primitives by visual role keeps Review Workflow Contributions composable.

## Important nuances

### Generation tokens for async / stale results

pi handles async tool calls with awareness that the conversation might have moved on. The equivalent for LazyDiff: a Review Workflow Contribution that submits a comment, fetches CI status, or runs an AI suggestion may complete after the user navigated away. Effects should carry a `generation: u64` (or a route token) and the workspace ignores results from generations that no longer match. This prevents "stale draft saved over a new draft" classes of bugs.

### Capabilities are read-mostly + intent-emitting

When a contribution needs to change workspace state, it does so by *emitting an intent* (not by mutating). Same shape as the reducer in ADR 0003. The capability surface is read for current state and write-via-intent for changes.

### No third-party runtime plugin API yet

pi loads extensions as JS modules at runtime. LazyDiff phase one keeps contributions as compile-time internal Rust modules. The shape (contribution types, contexts, lifecycle) is borrowed; the runtime loader is deferred. ADR 0002 records this.

## What we are *not* importing

- A JavaScript / TypeScript plugin loader.
- Arbitrary FFI plugin runtime in Rust (`libloading`, WASM hosts) — defer until we know the public seam is needed.
- pi's specific message-rendering APIs — they are about agent chat, not diffs.

## Where to look

Under `~/.cache/checkouts/github.com/badlogic/pi-mono/`:

- `apps/pi/src/extensions/` — concrete extension contribution shapes.
- `apps/pi/src/core/` — the bounded capability surfaces.
- Public README of pi describes "tools, commands, message renderers, lifecycle hooks, UI APIs, resources" as the surface.

In the user's own `pi-config`:

- `extensions/custom-renderer.ts` — proves real-world contributions want chrome/status/overlay/keymap surfaces, not just tools.

## The carried-over rules

1. Contributions take a context object, not `&mut App`.
2. The set of contribution kinds is fixed by the core; new kinds are core-level decisions.
3. Async work carries a generation token; stale results are dropped.
4. Contribution shape should anticipate UI composition (slots, chrome) from day one.
5. The runtime-loading question is separate from the contribution-shape question. Shape now, runtime later.
