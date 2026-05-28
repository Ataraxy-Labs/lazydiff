# Async effects use generation tokens; stale results are dropped, not applied

LazyDiff has many async flows: forge fetches (PR data, comments, reviews), persistence writes (notes, drafts, settings), git/forge submissions (review submit, comment post), file IO (semantic loads, blob loads), and background indexing. Today these results often land back on `App` via callbacks/channels and mutate state regardless of whether the user has since navigated away, switched PRs, reloaded the diff, or moved the cursor. That produces ghost updates, cursor jumps, stale comment trees attached to the wrong commit, and "why is this loading spinner stuck" bugs.

## Decision

### Effects are the only async-kick mechanism

Per ADR 0003, surface reducers return `Vec<AppEffect>`. The app shell's effect runner is the **only** code path that performs async work. Reducers themselves are pure with respect to async: they describe what should happen, they do not call `tokio::spawn`, do not hit the network, do not write to disk.

### Every async effect carries a generation token

Each async effect carries a `GenerationToken` (or set of tokens) that identifies the surface state it was launched from. When the result returns as an intent, the surface reducer first checks that its current generation still matches; if not, the result is **dropped** (or, when meaningful, downgraded to a passive cache write).

Concretely:

```rust
pub struct GenerationToken {
    pub surface: SurfaceId,        // which surface launched it
    pub kind: GenerationKind,      // e.g. PrLoad, DiffLoad, ReviewSubmit
    pub value: u64,                // monotonically increasing per (surface, kind)
}

enum DiffIntent {
    …,
    PrCommentsLoaded { token: GenerationToken, comments: Vec<PrComment> },
    PrCommentsFailed { token: GenerationToken, error: ForgeError },
}
```

The surface bumps the relevant generation whenever its identity changes (different PR, different commit, different file set, hard reload, navigation away and back). Stale tokens are silently rejected.

### Categories of async effect

| Category | Examples | Token strategy |
|---|---|---|
| Identity-bound fetch | PR metadata, PR comments, review threads, blob load | Drop if token mismatches; do not mutate surface |
| Cache write-through | "loaded blob X" → put in shared cache | Always allowed; cache key is intrinsic to the data |
| User-initiated submit | submit review, post comment, save note | Show pending state with token; on return, only finalize if token still matches the optimistic record; otherwise reconcile via cache |
| Persistence write | save notes file, save settings | Fire-and-forget with structured error effect; not surface-coupled |
| Long-running watcher | filesystem watcher, terminal stream | Owned by the effect runner; surfaces subscribe via intents |

### No callbacks into `App`

Async machinery (tokio tasks, channels, watchers) lives in the effect runner. It sends results back as intents on a single typed channel that the event loop drains and dispatches to the addressed surface. Async code does **not** hold `Arc<Mutex<App>>` or call methods on `App` directly. This keeps the borrow story clean and the test story sane.

### Errors are effects too

Async failures return as typed error intents (`*Failed { token, error }`). Surfaces decide how to render them (toast, inline error row, panel). The effect runner does not pop dialogs on its own.

## Why generation tokens, not cancellation

True cancellation in Rust async is cooperative and leaky. Generation tokens are simpler and sufficient: we may still pay the cost of an in-flight request, but we never apply its result incorrectly. Where actual cancellation matters for cost (large blob fetches, long forge calls), the effect runner may also drop the future when the generation is bumped — but **correctness comes from the token check, not from cancellation**.

## Consequences

- Bug class eliminated: stale forge data overwriting the active surface after navigation.
- Bug class eliminated: optimistic submit landing on the wrong record after the user moved on.
- Surfaces stay testable: reducer tests can feed `PrCommentsLoaded { token: stale, … }` and assert the state did not change.
- The effect runner becomes a real subsystem with its own module and tests, not a scatter of `tokio::spawn` calls.
- Slightly more boilerplate per async flow; this is intentional and pays back in correctness.

## Out of scope

- Persistent task queues, retry policy frameworks, distributed work — not justified at this scale.
- A full actor system — surfaces are reducers, not actors; only the effect runner is actor-shaped.
- Public/extension-authored async tasks — extensibility for async effects is deferred (ADR 0002).
