# Rust modules and visibility — the definitive reference

A single source of truth for crate/module/visibility questions, grounded in the [Rust Reference: Visibility and privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html). Read this when in doubt.

---

## The two access rules (from the Rust Reference, verbatim)

> 1. If an item is **public**, then it can be accessed externally from some module `m` if you can access **all of the item's ancestor modules** from `m`.
> 2. If an item is **private**, it may be accessed by the **current module and its descendants**.

Everything else in this document follows from these two rules. Memorize them.

---

## The vocabulary

| Term | Plain meaning |
|---|---|
| **Crate** | One unit of compilation. Produces either a library (`lib`) or an executable (`bin`). |
| **Workspace** | Cargo concept. A folder containing multiple related crates that share build outputs. |
| **Module** | A namespace inside a crate. Equivalent to a JS file or a folder with an entry file. |
| **Module tree** | The hierarchy built by `mod foo;` declarations, starting from `main.rs` or `lib.rs`. |
| **Item** | Anything you can name: `fn`, `struct`, `enum`, `const`, `static`, `mod`, `trait`, `impl`. |
| **Ancestor module** | A module higher up in the tree from where you are. |
| **Descendant module** | A module lower down in the tree from where you are. |
| **Sibling module** | A module that shares an immediate parent with you. |
| **Path** | The route to an item: `crate::foo::bar::Thing`. |
| **`pub`** | Visibility modifier. Marks an item as "exported" by its module. |
| **`use`** | Brings a path into the current scope as a short name. |
| **`mod foo;`** | Adds `foo.rs` (or `foo/mod.rs`) to the compile tree as a submodule. |

---

## `mod` vs `use` — never confuse these again

These do **different things**:

| | What it does | Without it… |
|---|---|---|
| `mod foo;` | **Adds the file to the build.** Creates a node in the module tree. | The file `foo.rs` is invisible to the compiler. |
| `use crate::foo::bar;` | **Brings `bar` into scope** so you can call it by short name. | You can still call it via the full path `crate::foo::bar(...)`. |

You write `mod foo;` **exactly once**, in the parent module. You write `use ...;` in every file that wants the short name.

> *Analogy*: `mod` is **"add this file to my project."** `use` is the JS-style import you already know.

---

## The default: private

> *"By default, everything in Rust is private."*

— Rust Reference

This means: an item with no visibility modifier is reachable only from its own module and all of its descendant modules.

```rust
mod a {
    fn helper() {}      // private to `a` and `a`'s descendants

    mod b {
        fn other() {
            super::helper();  // ✅ b is a descendant of a, so it sees `helper`
        }
    }
}

mod c {
    fn nope() {
        super::a::helper();  // ❌ c is NOT a descendant of a — error: private
    }
}
```

**Key corollary** (this answers one of your earlier questions): siblings of a private module CAN see it, because they share a parent — they are descendants of that parent.

---

## The visibility modifiers in order of openness

| Modifier | Means |
|---|---|
| *(none)* | Private. Visible to **the declaring module and its descendants**. |
| `pub(self)` | Same as private. Rarely written. |
| `pub(super)` | Visible to the **immediate parent module** (and therefore all of the parent's descendants — siblings, cousins of the declaring module). Same as `pub(in super)`. |
| `pub(in path)` | Visible within the given ancestor module's subtree. `path` must be an ancestor. |
| `pub(crate)` | Visible **anywhere in the current crate**. Not exported outside. |
| `pub` | Visible to anyone who can access this module's ancestors — including external crates if every ancestor on the path is also `pub`. |

---

## The chain-of-doors rule (the big one)

This is the rule that catches everyone. From the Rust Reference:

> *"To access an item, all of its parent items up to the current scope must still be visible as well."*

A `pub fn` inside a private module is **not** reachable from outside that module's subtree. The function is public, but the module containing it is private — the door to the room is locked even though the cupboard inside is unlocked.

```rust
mod outer {            // private (no pub)
    pub fn shiny() {}  // pub, but reachable only inside outer's subtree
}

fn elsewhere() {
    outer::shiny();    // ❌ error — module `outer` is private
}
```

**Mental model**:

```
crate::a::b::c::Thing
       ▲  ▲  ▲    ▲
       │  │  │    └── must be `pub` (or visible to the caller)
       │  │  └─────── must be visible to the caller
       │  └────────── must be visible to the caller
       └───────────── must be visible to the caller
```

**Every segment must be accessible.** Failing the chain at any link is fatal.

---

## Cross-crate vs in-crate

A crate is the strongest privacy boundary in Rust. From outside a crate, the only items you can see are those that are `pub` AND whose ancestor modules are all `pub` all the way up to the crate root.

| What you write | Where it's visible |
|---|---|
| `pub fn x() {}` at the crate root | Anywhere in this crate; external crates too if you depend on this crate. |
| `pub fn x() {}` inside a non-`pub` module | Only inside that module's subtree. Hidden from outside the crate. |
| `pub(crate) fn x() {}` | Anywhere in this crate. Never exported. |
| `pub(super) fn x() {}` | Parent module and its descendants. |
| `fn x() {}` | The declaring module and its descendants only. |

---

## `pub use` — re-exports

Re-exports let you "short-circuit" the path. The Rust Reference calls this "publicly re-exporting items."

```rust
pub use self::implementation::api;   // expose `api` at the current module level

mod implementation {                 // private
    pub mod api {
        pub fn f() {}
    }
}

// Now callers can write `the_outer_module::api::f()` even though
// `implementation` is private. The `pub use` creates an alternative
// public path that bypasses the private module.
```

Use this to keep your file structure private but expose a curated public API.

---

## Worked examples grounded in your LazyDiff repo

### `src/app.rs:144` — `pub(crate) struct App`

```rust
pub(crate) struct App {
    forge: Arc<dyn Forge>,
    // … 90 private fields …
}
```

- The struct is visible anywhere in the `lazydiff` binary crate (so any file under `src/`).
- The fields have no `pub`, so they are private to the `app` module's subtree (i.e., `app.rs` itself and every file under `src/app/`).
- Other crates (like `lazydiff-diffs`) cannot see `App` at all.

### `src/app/diff_buffer.rs:9` — `pub(super) struct DiffBufferState`

```rust
pub(super) struct DiffBufferState { … }
```

- The struct is visible to the parent module (`app`) and therefore to all of `app`'s descendants — which means every sibling under `src/app/` can see it.
- Files outside `src/app/` (e.g., `src/forge/*`) cannot see it.
- Fields are private to `diff_buffer.rs` itself unless individually marked `pub(...)`.

### `src/app.rs:120` — `mod input;`

```rust
mod input;
```

- This *registers* the file `src/app/input.rs` as a submodule of `app`. Without this line, the file would not exist to the compiler.
- The submodule `input` is **private to `app`'s subtree**, because there's no `pub` in front of `mod`.
- `app.rs` can call `input::foo()`. So can any sibling under `src/app/` (e.g., `modals.rs` can write `super::input::foo()`).
- Code outside `app`'s subtree (e.g., `src/bounded_map.rs`) cannot reach `app::input::*`, because the **module `input` itself is private**, even if its functions are marked `pub`.

---

## Your questions, answered explicitly

### Q1: What does `pub(crate)` mean?

Visible anywhere within the current crate. Not exported to external crates that depend on this one.

### Q2: What's a crate vs a module?

A **crate** is a unit of compilation — produces one library or one binary. A **module** is a namespace inside a crate. One crate contains a tree of modules. Your repo has two crates (`lazydiff` and `lazydiff-diffs`) and a tree of modules inside each.

### Q3: Is a JS module the same as a Rust module?

Conceptually yes — both are file-scoped namespaces with explicit exports. Differences:

- Rust splits "add the file to the build" (`mod foo;`) from "import a name" (`use ...;`). JS combines both in `import`.
- Rust has finer-grained visibility (`pub`, `pub(crate)`, `pub(super)`, `pub(in path)`); JS only has exported/not-exported.
- Folder modules need an entry file: Rust uses `foo.rs` next to `foo/`, JS typically uses `foo/index.js`.

### Q4: Why do I need `mod foo;`? Can't I just import?

Because Rust has no runtime module loader. The compiler builds the module tree at compile time by walking `mod` declarations starting from `main.rs` or `lib.rs`. A `.rs` file with no `mod` declaration pointing to it is *invisible to the compiler*. The file isn't in the build at all — it's not a permission issue, it's a "does this code exist" issue.

`use` is a separate step: it brings a name into scope so you can write it short. You can always use the full path `crate::foo::bar()` without writing `use`.

### Q5: What does "parent module" and "sibling submodule" mean?

- **Parent module**: the module that contains the `mod foo;` declaration that introduced you to the tree. For `app/diff_buffer.rs`, the parent is `app` (since `app.rs` contains `mod diff_buffer;`).
- **Sibling submodule**: another module that shares your parent. `app/diff_buffer.rs` and `app/input.rs` are siblings because they're both registered in `app.rs` with `mod` lines.

### Q6: Can a sibling of `app` (say `app1::child::grandchild`) reach `app::input::do_thing()` if `do_thing` is `pub` in `input`?

**Only if every module on the path is visible to the caller.** In your real code:

```rust
// src/app.rs
mod input;        // ← NOT pub. So `input` is private to app's subtree.
```

This means `do_thing` is invisible from `app1::child::grandchild`, even though `do_thing` itself is `pub`, because the chain `crate → app → input` is broken at `input` (it's a private module).

To open the chain, `app.rs` would need to write `pub mod input;` (or `pub(crate) mod input;` if you want crate-wide visibility but no external export).

### Q7: Can siblings of `input` see `input` if `mod input;` is private?

**Yes.** A private module is visible to its parent's entire subtree, which includes all siblings of the module. So `app/modals.rs` can write `super::input::...` and reach into `input`. What it cannot do is reach into `input` from *outside* `app`'s subtree (e.g., from `src/forge/`).

---

## The visibility cheat sheet

```
              from item's POV          from caller's POV
              ─────────────────        ──────────────────
              who sees ME?             who can I call?

              private                   ← only my module and descendants can see me
              pub(self)                 ← same as private
              pub(super)                ← my parent (and parent's descendants) can see me
              pub(in some::ancestor)    ← items in that ancestor's subtree can see me
              pub(crate)                ← everyone in my crate can see me
              pub                       ← everyone who can see all my ancestors can see me
```

```
              when calling x::y::z::thing(), it works iff:
              ─────────────────────────────────────────────
              x, y, z are each visible to me  (chain-of-doors rule)
              AND `thing` is visible to me from inside z
```

---

## Two style approaches (from Kobzol's blog, both common)

You'll see both in real Rust codebases:

| Style | How items are marked | Public API determined by |
|---|---|---|
| **Global visibility** | Each item directly declares its final visibility: `pub`, `pub(crate)`, etc. | The visibility modifier on each item. |
| **Local visibility** | Items are `pub` (meaning "exported to my parent") or private. Modules are kept private and decide via `pub use` what to re-export. | The root module's curated list of `pub use` re-exports. |

LazyDiff currently mixes both. ADR 0004 leans toward local visibility for the diff workspace: keep the `workspace` module private under `app`, mark its API surface `pub` so `app` can use it, and never re-export it outside.

---

## Quick troubleshooting table

| Error | What it usually means |
|---|---|
| `could not find ‘foo’ in the crate root` | You wrote `use crate::foo::...` but no `mod foo;` exists at the crate root. |
| `module ‘foo’ is private` | The module exists, but isn't `pub` in its parent. You can see it from inside the parent's subtree, not from outside. |
| `function ‘bar’ is private` | The function exists, but lacks a `pub` modifier. |
| `can't leak restricted type` | A `pub fn` is trying to return a type that is less visible than the function itself. Either make the type more visible or the function less visible. |
| `unused import: ...` | You wrote `use` for something you never reference. Harmless warning. |

---

## References

- [Rust Reference: Visibility and privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html) — authoritative.
- [Rust Reference: Items and Modules](https://doc.rust-lang.org/reference/items/modules.html) — module tree mechanics.
- [Rust Book, Ch. 7](https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html) — the friendly introduction.
- [Kobzol, *Two ways of interpreting visibility in Rust*](https://kobzol.github.io/rust/2025/04/23/two-ways-of-interpreting-visibility-in-rust.html) — context on the two style approaches.
- [iximiuz, *Understanding Rust Privacy and Visibility Model*](https://iximiuz.com/en/posts/rust-privacy-and-visibility/) — worked examples.
