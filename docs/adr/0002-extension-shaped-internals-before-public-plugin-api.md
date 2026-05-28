# Use extension-shaped internals before a public plugin API

LazyDiff will shape its app-level surfaces around extension-compatible concepts from the start — but will not expose or depend on a public plugin runtime initially. The first phase ships a fixed set of internal **Review Workflow Contributions** consumed by the Diff Workspace and other surfaces. A public plugin API is explicitly deferred.

## Decision

The internal contribution model has a bounded, named set of contribution kinds:

- **Command** — a named intent that mutates surface state.
- **Keymap entry** — data binding a key sequence to a command.
- **Command palette entry** — data exposing a command in the palette UI.
- **Inline row producer** — emits an inline visual row inside a Diff Workspace (review thread, draft, AI suggestion, CI summary, future kinds).
- **Diff decoration producer** — emits a generic diff decoration (highlight, gutter mark, accent rail).
- **Review action** — a named intent invocable from an inline row (resolve, reply, dismiss, accept suggestion).
- **Chrome / status slot** — a typed region of the app-shell layout (header, footer, sidebar status, auth status, async/error indicator) populated by a contribution.
- **View-model slice** — a read-only data slice the renderer/UI reads from a surface.
- **Effect** — an async/IO request returned by a reducer for the app shell to perform.
- **Forge / integration capability** — a bounded interface (already `Arc<dyn Forge>`-shaped) for external systems.

Each kind has a fixed Rust shape. Contributions take a read-mostly context, not `&mut App`. New contribution kinds are core-level decisions (require an ADR amendment).

## Staged roadmap

The product direction is "build your own diff / build your own code review." It unfolds in five stages; each opens only after the previous proves out via real internal use:

1. **Internal seams.** Express LazyDiff's own features (comments, drafts, search, selection, semantic map, finder, command palette, folds) as the contribution kinds above. No public API. Goal: prove the shape against our own code.
2. **Compile-time Rust contributions.** The contribution traits (`Command`, `KeymapEntry`, `InlineRowProducer`, `DiffDecorationProducer`, `ReviewAction`, `ChromeSlot`, `ViewModelSlice`, `FoldStrategy`, `Effect`, forge capabilities) are public Rust APIs that downstream consumers — friendly forks, vendored embeds, in-house builds — can implement in their own Rust crate and register at startup against the same contribution registry LazyDiff uses internally. This is how "let users write their own Rust to add a fold strategy / command / chrome panel / review action" is supported today. It requires recompilation, no dynamic loader, and the registered code is held to the same `&mut`-free contract as internal contributions.
3. **Declarative resources.** Custom keymaps, themes, and review templates loaded from config files. Resources are data, not executable user code.
4. **First-party integrations.** Blame, CI annotations, coverage hints, AI suggestions, additional forges — added as internal Review Workflow Contributions. Proves the surface across different data sources.
5. **Public plugin API.** Only after stage 4 has multiple independent contributions hardening the surface, expose it for third-party plugins (dynamic libs / WASM / scripting). Until then, no runtime plugin loader exists.

Stages 1 and 2 are the current focus and are not sequential — stage 2 emerges as a consequence of designing stage 1 traits as public Rust APIs from day one. Stages 4–5 are explicitly out of scope until stages 1–3 are real.

## Constraints

- No arbitrary renderer mutation, ever.
- No contribution receives `&mut App` or `&mut DiffWorkspace`. Contributions emit intents/effects; the owner mutates.
- The contribution shape must anticipate UI composition (chrome/status slots), not only behavior. pi-config-style real-world customization extends beyond commands.
- The runtime-loading question (dynamic libs, WASM, scripting) is separate from the contribution-shape question and stays out of scope for the first migration.

## Reference

ProseMirror demonstrates generic core + plugin-contributed behavior. pi-mono demonstrates that real extensions need chrome/status/palette/overlay surfaces, not just commands. Both inform this ADR; LazyDiff borrows the contribution shape, not the runtime.

## Consequences

- Adding a new contribution kind is a core-level decision (ADR amendment), not an in-place hack.
- Stage 1 work expresses LazyDiff's existing features through these contribution kinds, even though only LazyDiff itself consumes them today.
- Stage 2 means contribution traits are designed as public Rust APIs from day one: stable enough that a downstream consumer can write `impl FoldStrategy for MyLockfileFold { … }` (or any other contribution kind) in their own crate, register it at startup, and ship a custom LazyDiff build. No dynamic loader is required; recompilation is the integration surface. The same `&mut`-free contract applies.
- The vocabulary in CONTEXT.md (`Review Workflow Contribution`, `Chrome Slot`, etc.) is the source of truth; code should use those names.
- Public-runtime-plugin-API design discussions (dynamic libs / WASM / scripting) are out of scope until the internal + compile-time contribution shape has survived multiple stage-1, stage-2, and stage-4 changes without forcing `&mut App`.
