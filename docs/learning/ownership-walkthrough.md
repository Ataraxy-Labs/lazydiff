# LazyDiff — Active Learning Notebook: Rust Ownership & the Architecture Migration

This is **your** learning notebook. We work through it section by section.

- Each section is written in depth so you can re-read.
- Each section starts with a status box you can tick.
- "❓ Your doubts" and "💡 Your aha" subsections are filled as we go in chat.
- ASCII diagrams use a side-by-side diff style with line numbers (left = today, right = after).

## Progress map

```
  ☑  0. Product flows you've felt break                  (reference — read anytime)
  ☑  1. Ownership — one owner per value
  ☑  2. Borrowing — & vs &mut, the read-many-or-write-one rule
  ☐  3. Module privacy as an architecture lever                        ◄── now teaching
  ☐  4. Today's God Struct in product terms
  ☐  5. Intents — the new front door
  ☐  6. The single visual row list (Pierre's lesson, in Rust)
  ☐  7. Effects — IO leaves through one door
  ☐  8. Generation tokens — making async safe
```

---

## Section 0 — Product flows you've felt break

**Status: ☑ reference — read this whenever a Rust concept feels abstract**

Every Rust concept in Sections 1–8 was put there to fix something real. This section is the *real* — concrete user actions that misbehave today, traced to the code, with the fix sketched.

When a later section feels too abstract, come back here and re-read one of these flows. The Rust concept then has a "this is why" attached to it.

The four flows below are representative, not exhaustive. They share one root cause: **the diff workspace's state is scattered, and the code that reacts to user input rebuilds its own view of the world instead of reading a single shared one.**

---

### Flow A — "Mouse wheel stops short at the bottom"

```
   what the user does                          │  what the user expects
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   Opens a long file that has a comment        │  Scroll wheel down reaches the last
   box rendered between two diff lines.        │  diff line of the file.
   Scrolls down with the mouse wheel.          │
```

```
   what actually happens                       │  why
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   The scroll stops about 4 rows short         │  The scroll-wheel handler asks
   of the real bottom. The user can see        │  `row_count_for_mode(...)` for the
   "the end is here" but pressing `j`          │  total row count. That function counts
   reveals more content below.                 │  ONLY diff-text rows; it ignores the
                                               │  inline comment box (~4 rows tall).
                                               │  So `max_scroll` is computed against
                                               │  the wrong total. Wheel stops short.
```

```
   fields involved                             │  files & lines
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   diff_buffer.viewer.scroll_y                 │  src/app.rs:679   wheel handler
   inline_blocks set (review notes,            │      row_count_for_mode(...)  ← LIES
       expanded threads, draft editor)         │  src/app.rs:2069  keyboard scroll
                                               │      visual_row_count_with_  ← truthful
                                               │      inline_blocks(...)
```

```
   after the migration
   ─────────────────────────────────────────────────────────────────────────────────────
   One workspace owns the row cache. `frame().rows.len()` is the only total anyone
   can ask. The scroll-wheel intent and the keyboard intent both read the same number.
   Wheel reaches the actual last row.
```

**Rust concepts this flow exercises:**

- **Section 1 (ownership):** today, no single thing owns "the total row count"; it's derived independently by two code paths. Migration moves it into the workspace.
- **Section 2 (borrowing):** `frame()` hands out a `&[Row]` borrow; `len()` on that borrow is the only legal total. The type system enforces it.

---

### Flow B — "Click near a comment hits the wrong line"

```
   what the user does                          │  what the user expects
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   Scrolls to a region that has an inline      │  Click lands on the visible line
   comment box. Clicks on a diff line just     │  the user clicked.
   below the comment box.                      │
```

```
   what actually happens                       │  why
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   The cursor lands on the wrong line —        │  Mouse mapping does:
   often one row off. Comment expansion        │      row = scroll_y + click_y
   state seems to "shift" what gets selected.  │  scroll_y was computed by one path
                                               │  using one row total; click_y is in
                                               │  screen coords. The mapper does NOT
                                               │  account for inline rows that sit
                                               │  between scroll_y and the click
                                               │  position. The row index it produces
                                               │  is in the wrong coordinate space.
```

```
   fields involved                             │  files & lines
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   diff_buffer.viewer.scroll_y                 │  mouse event handler in src/app.rs
   inline_blocks                               │  visual_rows_with_inline_blocks rebuilt
   click position (column, row)                │  independently for paint vs. for click
                                               │  vs. for scroll
```

```
   after the migration
   ─────────────────────────────────────────────────────────────────────────────────────
   Mouse mapping consults `frame().rows[scroll.first_visible + click_y]`. That row is
   the same row the renderer painted at that screen position, because both indexed
   into the same list. Click hits exactly what the user clicked.
```

**Rust concepts this flow exercises:**

- **Section 6 (single visual row list):** four consumers (keyboard, scroll, mouse, renderer) read the same `&[Row]` slice. They cannot disagree on what's at index N — by construction.

---

### Flow C — "Pressing `j` past a comment box feels janky"

```
   what the user does                          │  what the user expects
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   Cursor is on diff line 42. An inline        │  `j` moves the cursor down one visual
   comment thread sits between line 42         │  row at a time — into the comment
   and line 43. User presses `j`.              │  rows, then onto line 43.
```

```
   what actually happens                       │  why
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   Sometimes the cursor "skips" the            │  Navigation rebuilds the row list
   comment box; sometimes it stutters on       │  to find the cursor's current index,
   the comment but then the scroll math        │  computes index+1, but the scroll-
   doesn't update so part of the comment       │  to-keep-visible logic uses a
   is hidden. Either way it feels wrong.       │  separately-computed max_scroll.
                                               │  The two don't agree about how
                                               │  tall the page is, so cursor and
                                               │  scroll desynchronize.
```

```
   fields involved                             │  files & lines
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   diff_buffer.viewer.cursor_row               │  src/app.rs:931   resolves visual index
   diff_buffer.viewer.scroll_y                 │  src/app.rs:3562  next-change navigation
   inline_focus                                │  src/app.rs:3711  current cursor pos
                                               │  each rebuilds visual_rows again
```

```
   after the migration
   ─────────────────────────────────────────────────────────────────────────────────────
   `update(Intent::CursorDown)` is one atomic operation. Inside it: cursor index += 1,
   scroll adjusted to keep cursor in viewport, rows marked dirty if needed. All in
   one method, against one row list. `j` always lands on the visually next row.
```

**Rust concepts this flow exercises:**

- **Section 5 (intents):** "move down" is one named intent, handled in one place. Cursor and scroll move together by construction.
- **Section 2 (borrowing):** during `update`, `&mut self` excludes anyone else; the cursor + scroll change as a unit.

---

### Flow D — "Drag-to-select bleeds across sides"

```
   what the user does                          │  what the user expects
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   In side-by-side view, holds mouse down      │  Highlight only on the right side.
   on the right side at line 10 and drags      │  Left side untouched.
   down to line 15 to copy that text.          │
```

```
   what actually happens                       │  why
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   The highlight sometimes appears on the      │  Nine independent fields track the
   left side too, especially during fast       │  drag (`dragging_scrollbar`,
   drags. Releasing the mouse can leave        │  `selecting_text`, `text_selection_
   stale highlights behind. The clipboard      │  dragged`, `pending_screen_selection`,
   copy includes characters from both          │  `screen_selection`, `screen_selection
   sides.                                      │  _bounds`, plus mode flags). They are
                                               │  set/cleared from multiple input
                                               │  handlers. Nothing enforces "side"
                                               │  consistency — the selection's side
                                               │  is implicit and easy to get wrong.
```

```
   fields involved                             │  files & lines
   ─────────────────────────────────────────── │  ─────────────────────────────────────────
   dragging_scrollbar, selecting_text,         │  src/app.rs:168..175
   text_selection_dragged,                     │  src/app.rs handle_mouse paths
   pending_screen_selection, screen_selection, │
   screen_selection_bounds, scrollbar_drag_*   │
   plus diff_focus (which pane)                │
```

```
   after the migration
   ─────────────────────────────────────────────────────────────────────────────────────
   Nine fields collapse into ONE enum:

       enum MouseState {
           Idle,
           DraggingScrollbar { drag: ScrollbarDrag },
           SelectingText    { start: ScreenPoint, current: ScreenPoint, side: Side },
       }

   The selection variant carries `side`. "Dragging scrollbar AND selecting text" is
   no longer a representable state — the compiler refuses it.
```

**Rust concepts this flow exercises:**

- **Section 4 (god struct):** collapsing nonsense states into an enum is the architectural win.
- **Section 1 (ownership):** the workspace owns the mouse state; no other code can put it in a contradictory combination.

---

### The common thread in all four flows

```
   ┌────────────────────────────────────────────────────────────────────────┐
   │                                                                        │
   │  every flow above goes wrong because:                                  │
   │                                                                        │
   │    1. multiple code paths each compute their own view of the world,    │
   │    2. the views drift apart between paths or within one frame,         │
   │    3. the developer who added each path could not see the others,      │
   │       so each "fix" patched its own path without fixing the system.    │
   │                                                                        │
   │  the migration fixes them all the same way:                            │
   │                                                                        │
   │    one owner (Section 1) holds the state                               │
   │    private fields (Section 3) lock outsiders out                       │
   │    one borrowed view (Section 2) is shared by all readers              │
   │    intents (Section 5) are the only way to mutate                      │
   │    one row list (Section 6) is the only coordinate space               │
   │                                                                        │
   └────────────────────────────────────────────────────────────────────────┘
```

When a Rust concept in later sections feels abstract, reread the flow it would fix. The concept exists *because of* the flow.

### ❓ Your doubts about product flows

_(filled in chat)_

### 💡 Your aha about product flows

_(filled in chat)_

---

## Section 1 — Ownership: one owner per value

**Status: ☑ understood (signed off)**

### The whole rule

> **Every value in Rust has exactly one owner. When the owner goes away, the value is destroyed. The compiler enforces this — at compile time, not at runtime.**

That's the whole rule. Everything else is decoration.

### Picture: who has the diff document?

```
   ┌────────────────────────────────────┐
   │   the value: a DiffDocument        │
   │   (the parsed diff you're viewing) │
   └────────────────┬───────────────────┘
                    │ owned by
                    ▼
   ┌────────────────────────────────────┐
   │   the owner:  App                  │
   │   (the top-level program object)   │
   └────────────────────────────────────┘
```

`App` owns the `DiffDocument`. While `App` is alive, the `DiffDocument` exists. When `App` is destroyed (program exits), the `DiffDocument` is automatically destroyed too. No garbage collector, no `free()`, no memory leak. The compiler inserts the cleanup for you because it always knows when `App` is going away.

### Picture: ownership transfer

In Rust, when you pass a value to a function the "normal" way, ownership **moves**.

```
   BEFORE the call                    │  AFTER the call
   ─────────────────────────────────  │  ─────────────────────────────────
 1 │ let doc = DiffDocument::new();   │1 │ let doc = DiffDocument::new();
 2 │ store(doc);                      │2 │ store(doc);
 3 │ println!("{:?}", doc); // error  │3 │ // `doc` no longer accessible
                                      │     // it was MOVED into store()
                                      │     // store() now owns it
```

After line 2, `doc` is no longer a thing you can use. The owner became `store`. If you try to read `doc` on line 3, the compiler stops you:

> *error: borrow of moved value: `doc`*

This is unfamiliar coming from JS/Python/Java where passing an object is "shared by reference." In Rust, the default is **move**. (You can opt in to sharing via borrowing — that's Section 2.)

### Why one-owner matters for architecture (not just memory)

In every other language, "this class owns that data" is a polite suggestion. Six months later, somebody mutates the data from elsewhere, and the code lies. In Rust, the rule is enforced *by the compiler*.

Translated to LazyDiff:

```
   today                              │  after migration
   ─────────────────────────────────  │  ─────────────────────────────────
 1 │ App owns: cursor, scroll,        │1 │ App owns: routing, async, caches
 2 │   inline_focus, mouse_drag,      │2 │ DiffWorkspace owns: cursor,
 3 │   expanded_threads, editor,      │3 │   scroll, inline_focus,
 4 │   ~90 fields total               │4 │   mouse_drag, expanded_threads,
 5 │                                  │5 │   editor, row cache, dirty flag
 6 │ Mutations: from anywhere in      │6 │
 7 │   src/app.rs and src/app/*       │7 │ Mutations: ONLY through
 8 │   (120 direct field pokes)       │8 │   workspace.update(intent)
```

The "one owner" rule is the **architectural lever** ADR 0004 leans on. Today, no single thing owns the diff interaction state — `App` has fields, `DiffBufferState` has fields, `CommentModal` has fields, all mutated independently. After migration, one struct (`DiffWorkspace`) owns the lot, and the compiler forbids anyone else from reaching in.

### The four physical consequences

1. **No data races.** If only one thing can write, two threads can't fight over the same memory. (Useful later for async.)
2. **No use-after-free.** The owner controls lifetime; the value can't be touched after its owner is destroyed.
3. **No silent action-at-a-distance.** If you didn't pass ownership or a borrow, the callee can't touch your value. Period.
4. **The compiler tells you exactly where ownership goes.** Every value's life can be traced through the code.

### The five things that can happen to a value

```
   ┌──────────────────────────────────────────────────────────┐
   │  1. Create it          let x = Thing::new();             │
   │  2. Move it            let y = x;            // x gone   │
   │  3. Borrow read-only   read(&x);             // x intact │
   │  4. Borrow exclusive   modify(&mut x);       // x intact │
   │  5. Drop it            // happens automatically at end   │
   └──────────────────────────────────────────────────────────┘
```

Cases 3 and 4 (borrowing) are Section 2 — the second half of the architectural superpower. For now, hold onto: **moving is the default**, and the value can't be in two places at once.

### LazyDiff mapping for ownership (today vs after)

```
   today                              │  after migration
   ─────────────────────────────────  │  ─────────────────────────────────
 1 │ struct App {                     │1 │ struct App {
 2 │     diff_buffer: DiffBufferState,│2 │     workspace: DiffWorkspace,
 3 │     inline_focus: Option<…>,     │3 │     // routing, async, caches
 4 │     comment_modal: Option<…>,    │4 │ }
 5 │     expanded_threads: HashSet,   │5 │
 6 │     // 86 more fields …          │6 │ struct DiffWorkspace {
 7 │ }                                │7 │     // 30-ish fields, ALL private
 8 │                                  │8 │ }
```

Today `App` owns ~90 things flatly. Tomorrow `App` owns one thing (`workspace`) and the workspace owns ~30 fields privately. Same total state, very different ownership shape.

### ❓ Your doubts (filled as we go)

_(to be filled in chat)_

### 💡 Your aha (filled as we go)

_(to be filled in chat)_

---

## Section 2 — Borrowing: `&T` and `&mut T`

**Status: ☐ in progress**

### The whole rule

> **You can have many readers (`&T`) OR one writer (`&mut T`). Never both at the same time.**

That's it. The compiler enforces this just like single-ownership.

### Picture: read-borrow vs write-borrow

```
   read borrow (&T)                   │  write borrow (&mut T)
   ─────────────────────────────────  │  ─────────────────────────────────
 1 │ ┌────────┐                       │1 │ ┌────────┐
 2 │ │ owner  │── &T ─▶ reader 1      │2 │ │ owner  │── &mut T ─▶ writer
 3 │ │        │── &T ─▶ reader 2      │3 │ │        │   nobody else can
 4 │ │        │── &T ─▶ reader 3      │4 │ │        │   read or write
 5 │ └────────┘                       │5 │ └────────┘   while this is live
 6 │                                  │6 │
 7 │ Many readers OK.                 │7 │ Exactly ONE writer.
 8 │ Nobody (even owner) can write    │8 │ Nobody else (not even owner)
 9 │ while borrows are alive.         │9 │ can touch it.
```

Two analogies:

- **Library book.** Either many people can read a copy at the reading-room table at once (`&T`), or one person checks it out exclusively to scribble in it (`&mut T`). Both at once is impossible.
- **Whiteboard.** Either many people are reading it silently (`&T`), or one person is at the marker erasing/rewriting (`&mut T`). You can't read a sentence while it's mid-erasure.

### Why this is a superpower, not a tax

In JS/Java, you can pass a mutable object to two functions and they can race. In Rust, the compiler refuses. So:

- "I'll just cache this list and pass it everywhere" — fine, as long as nobody writes while the reads are happening.
- "I need to mutate while iterating" — compiler stops you, because iteration holds a `&`, and mutation needs `&mut`.

The bugs this kills are the ones nobody catches in JS code review.

### Function signatures translated

```
   Rust syntax                                  │  English
   ──────────────────────────────────────────── │ ────────────────────────────────
 1 │ fn save(doc: DiffDocument)                 │1 │ Take ownership. Caller loses it.
 2 │ fn render(doc: &DiffDocument)              │2 │ Borrow read-only. Caller keeps it.
 3 │ fn parse(doc: &mut DiffDocument)           │3 │ Borrow exclusively. Caller keeps it.
 4 │                                            │4 │
 5 │ impl Foo {                                 │5 │ Methods on Foo:
 6 │   fn name(&self)                           │6 │   read-borrow self
 7 │   fn rename(&mut self, new: String)        │7 │   write-borrow self
 8 │   fn consume(self)                         │8 │   take ownership of self
 9 │ }                                          │9 │ }
```

When you see `&mut self` on a method, that method is currently the *one writer* of the whole struct. Every other thread, every other reader, every other writer is excluded until the method returns.

### LazyDiff mapping for borrowing

```
   today                                          │  after migration
   ──────────────────────────────────────────     │  ──────────────────────────────────
 1 │ self.diff_buffer.viewer_mut().cursor_row +=1;│1 │ self.workspace.update(Intent::CursorDown);
 2 │ // App reaches in, writes directly           │2 │ // App sends an intent
 3 │ // No one else is "told" about the change    │3 │ // workspace's &mut self handles
 4 │                                              │4 │ //   cursor + scroll + cache atomically
 5 │ // Meanwhile, in the SAME frame:             │5 │
 6 │ self.maybe_scroll_to_cursor();               │6 │ let frame = self.workspace.frame();
 7 │ // separate function, recomputes everything  │7 │ render(&frame);
 8 │ // might disagree                            │8 │ // read-borrow only; cannot mutate
```

In the today column, App writes to `viewer.cursor_row` directly, then *separately* calls a function that has to remember to also update scroll. The two operations are not bonded.

In the after column, App calls one method on the workspace (`update`), and the workspace's `&mut self` lets it atomically: move the cursor, adjust the scroll, mark the cache stale. Then a separate read-only `frame()` hands out a shareable view. The borrow-checker enforces that nothing tries to write while the frame is being painted.

### ❓ Your doubts

**Q: If Rust prevents mid-rebuild reads at compile time, how is the current LazyDiff code (also Rust) able to have this bug class at all? Are we bypassing the checker?**

**A:** The borrow checker enforces its rules on every individual `&` / `&mut`. It does **not** enforce architectural rules like "there should be one canonical row list."

Today's code bypasses the checker by **never sharing**. The row-builder function returns an owned `Vec<DiffVisualRow>` — a brand new value. Each consumer (keyboard, scroll, mouse, renderer) calls it independently and gets its own fresh `Vec`. Four `Vec`s exist. None of them borrow from each other. There is no `&` for the checker to complain about. The checker is doing its job correctly: it has checked nothing because nothing is being shared.

The bug is at a higher level: four `Vec`s that should be one. Rust doesn't protect against that automatically.

**Why `&mut self` doesn't save you today:** every method on `App` takes `&mut self`, which gives exclusive access to the *whole* `App`. Inside that method, you can poke any of the 90 fields in any order — the borrow checker is satisfied because nothing else is touching `App` during the call. `&mut self` is too coarse to protect "this method shouldn't be writing those particular fields."

**How the migration changes the rules:** we redesign the types.

- `update(&mut self)` — needs exclusive access.
- `frame(&self) -> &[Row]` — hands back a borrow into the workspace's row cache.

While the renderer holds the `&[Row]`, the workspace is effectively `&`-borrowed. The borrow checker **now** refuses `update(...)` calls during that window (because `update` needs `&mut`, and `&mut` + `&` simultaneously is forbidden).

**The general principle:**

> Rust doesn't enforce your architecture for you. It enforces the rules you've encoded in types. So: encode the architecture in the types, and the compiler becomes your reviewer.

Today's `Vec<T>` returns make the checker silent. Tomorrow's `&[T]` returns from a single owner make the checker your friend. That single change — owned → borrowed-from-one-owner — is what activates Rust's protection.

### 💡 Your aha

- "Rust uses Rust syntax correctly today; it just doesn't *exploit* Rust's checker to protect the architecture." → the migration's value is reshaping types so violating the architecture becomes a compile error.

---

## Section 3 — Module privacy as an architecture lever

**Status: ☐ queued**

### The whole rule

> **A `pub` item is reachable only if every module on the path from the caller to the item is also reachable.** Privacy is a chain of doors; each door must be unlocked for the corridor to be open.

(Full deep-dive in `docs/research/rust-modules-and-visibility.md`. You already read it.)

### Why this matters for LazyDiff

Privacy in Rust is **module-level**, not class-level. Anything inside the same module can touch each other's private fields. Move a thing to its own module → suddenly it has a real privacy boundary.

```
   today                                          │  after migration
   ─────────────────────────────────────────────  │  ─────────────────────────────────
 1 │ src/app.rs (5000+ lines)                     │1 │ src/app.rs
 2 │   struct App { cursor, scroll, … }           │2 │   struct App { workspace, … }
 3 │   fn handle_key()  — pokes any field         │3 │
 4 │   fn handle_mouse()— pokes any field         │4 │ src/app/workspace.rs   ◄── new file
 5 │   fn render()      — pokes any field         │5 │   pub struct DiffWorkspace {
 6 │   …                                          │6 │     cursor, scroll, …     // private
 7 │                                              │7 │   }
 8 │ All those functions live in the              │8 │   impl DiffWorkspace {
 9 │ SAME module as the fields, so privacy        │9 │     pub fn update(&mut self, …) { … }
10 │ doesn't kick in — they can all see           │10 │     pub fn frame(&self) -> Frame<'_> { … }
11 │ everything.                                  │11 │   }
12 │                                              │12 │
13 │                                              │13 │ Now `handle_key` etc. live OUTSIDE
14 │                                              │14 │ `workspace.rs`, so they CANNOT
15 │                                              │15 │ touch cursor/scroll/etc directly.
16 │                                              │16 │ Compiler says: "field is private."
```

Creating a new file `src/app/workspace.rs` and moving the diff state into it is what activates Rust's privacy enforcement. Without that move, privacy doesn't help — everything's in one big module.

### LazyDiff mapping

You already saw this in your reference doc. The migration's *physical act* is:

1. Create `src/app/workspace.rs`.
2. Move the diff-interaction fields into a struct there, **without** `pub` on the fields.
3. Add `mod workspace;` in `src/app.rs` (without `pub` — keep it internal).
4. Add `pub fn update` and `pub fn frame` on `DiffWorkspace`.
5. Delete the fields from `App`. Replace with one field `workspace: DiffWorkspace`.
6. Watch the compiler list the 120 places that used to poke fields directly. Each one becomes either an intent send or a frame read.

The 120 errors are not a problem — they are the **architectural ratchet**. Each one is a place where the old kitchen had a sneaky pen; fixing it requires a real intent name.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Section 4 — Today's God Struct in product terms

**Status: ☐ queued**

### The struct, grouped by what it actually means

```
   ┌─────────────────────────────────────────────────────────┐
   │  App (5000-line struct, 90 fields)                      │
   ├─────────────────────────────────────────────────────────┤
   │  identity:    forge, path, project_label, document      │
   │               (stay on App — fine)                      │
   │                                                         │
   │  routing:     surface, history, should_quit             │
   │               (stay on App — fine)                      │
   │                                                         │
   │  geometry:    viewport_height, surface_scroll_y         │
   │               (mostly stay — global terminal facts)     │
   │                                                         │
   │  ★★ DIFF INTERACTION (the bug zone) ★★                │
   │     diff_buffer       — cursor, scroll, mode, search   │
   │     diff_focus        — which pane has keyboard         │
   │     inline_focus      — which review row is focused     │
   │     expanded_threads  — which threads are unfurled      │
   │     comment_modal     — the inline editor in flight     │
   │     thread_modal      — focused thread reader           │
   │     transient_focus   — the "you jumped here" flash     │
   │     mouse drag fields (9 of them!)                      │
   │     text selection fields (4 of them!)                  │
   │     → ALL of these move into DiffWorkspace              │
   │                                                         │
   │  sidebars:    review_sidebar_*, semantic_*, file_picker │
   │               (own workspaces eventually; not phase 1)  │
   │                                                         │
   │  async:       query_tx, query_rx, query_client          │
   │               (stay on App — they shuttle to workspace) │
   │                                                         │
   │  caches:      pr_diff_cache, pr_patch_cache, …          │
   │               (stay on App — content, not interaction)  │
   │                                                         │
   │  metrics:     draw_count, draw_total, draw_max          │
   │               (stay on App — observability)             │
   └─────────────────────────────────────────────────────────┘
```

### The bug zone, expanded

The "★★" group is **the only group that causes the bugs you've been patching**. Everything else is fine where it is. Phase 1 of the migration only moves that group.

```
   today: split across App and inner structs   │  after: one owner
   ─────────────────────────────────────────── │  ──────────────────────────────────
 1 │ App.diff_buffer.viewer.cursor_row          │1 │ DiffWorkspace.cursor
 2 │ App.diff_buffer.viewer.scroll_y            │2 │ DiffWorkspace.scroll
 3 │ App.diff_buffer.mode                       │3 │ DiffWorkspace.mode
 4 │ App.inline_focus                           │4 │ DiffWorkspace.inline_focus
 5 │ App.expanded_review_threads                │5 │ DiffWorkspace.expanded_threads
 6 │ App.comment_modal                          │6 │ DiffWorkspace.editor
 7 │ App.dragging_scrollbar (bool)              │7 │ DiffWorkspace.mouse: MouseState
 8 │ App.active_scrollbar_drag (option)         │7 │     (enum — exclusive variants)
 9 │ App.selecting_text (bool)                  │7 │
10 │ App.text_selection_dragged (bool)          │7 │
11 │ App.pending_screen_selection (option)      │7 │
12 │ App.screen_selection (option)              │7 │
13 │ App.screen_selection_bounds (option)       │7 │
14 │ App.scrollbar_drag_offset_virtual          │7 │
                                                │   one enum, three states, can't
                                                │   be in "selecting text AND
                                                │   dragging scrollbar" at once
```

The mouse-drag section is the most extreme: today it's nine independent booleans/options that can be in nonsense combinations. After migration it's one enum with three variants, and the compiler refuses the nonsense.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Section 5 — Intents: the new front door

**Status: ☐ queued**

### What an intent is

An **intent** is a tiny English-named value that says "the user wants X to happen." It's a noun-shaped enum.

```rust
enum Intent {
    CursorDown,
    CursorUp,
    PageDown,
    NextChange,
    OpenInlineEditor { line: usize },
    SubmitDraft,
    ExpandThread { id: ThreadId },
    ToggleSideBySide,
    // … one per user-meaningful action
}
```

Instead of `App` reaching in to mutate fields, `App` constructs an intent and hands it to the workspace.

### Picture

```
   today: input → mutation                        │  after: input → intent → update
   ──────────────────────────────────────────     │  ─────────────────────────────────
 1 │ KeyEvent('j')                                │1 │ KeyEvent('j')
 2 │     │                                        │2 │     │
 3 │     ▼                                        │3 │     ▼
 4 │ App.handle_key()                             │4 │ keymap → Intent::CursorDown
 5 │     │                                        │5 │     │
 6 │     ├─▶ self.diff_buffer.viewer_mut()        │6 │     ▼
 7 │     │     .cursor_row += 1;                  │7 │ workspace.update(Intent::CursorDown)
 8 │     ├─▶ self.maybe_scroll_to_cursor();       │8 │     │
 9 │     ├─▶ self.invalidate_inline_layout();     │9 │     │  inside update():
10 │     └─▶ self.set_needs_redraw();             │10 │    │    cursor += 1
11 │                                              │11 │    │    adjust scroll
12 │ (4 independent steps; forget one → bug)      │12 │    │    mark rows dirty
13 │                                              │13 │    │    schedule redraw
14 │                                              │14 │    ▼
15 │                                              │15 │ returns Vec<Effect>
```

The intent isn't magic — it's just a small enum value. The win is that *the workspace's `update` method is the only place that decides what happens for a given intent*, and it can do all the bookkeeping in one atomic step.

### What intents look like for real LazyDiff actions

```
   keypress / mouse event           │  intent
   ─────────────────────────────────│  ─────────────────────────────────
 1 │ j                              │1 │ CursorDown
 2 │ k                              │2 │ CursorUp
 3 │ gg                             │3 │ JumpToTop
 4 │ G                              │4 │ JumpToBottom
 5 │ Ctrl+d                         │5 │ HalfPageDown
 6 │ ]c                             │6 │ NextChange
 7 │ /                              │7 │ EnterSearchMode
 8 │ enter (on a comment row)       │8 │ OpenInlineEditor { line }
 9 │ esc (in editor)                │9 │ CancelEditor
10 │ enter (in editor, submit)      │10 │ SubmitDraft
11 │ click on file header           │11 │ JumpToFile { path }
12 │ mouse drag start               │12 │ BeginSelection { at }
13 │ mouse move during drag         │13 │ ExtendSelection { to }
14 │ mouse release                  │14 │ EndSelection
```

Each line on the left used to be its own ad-hoc handler scattered through `src/app.rs`. Each line on the right becomes one variant in `Intent`, handled in one place inside the workspace.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Section 6 — The single visual row list (Pierre's lesson, in Rust)

**Status: ☐ queued**

### The product situation

The diff screen contains different kinds of rows:

```
   ┌────────────────────────────────────────────────────────┐
   │ 12   fn add(a: i32, b: i32) -> i32 {       ← diff text │
   │ 13 - return a + b;                          ← diff text │
   │ 13 + return a.wrapping_add(b);              ← diff text │
   │      ╭──────────────────────────╮           ← inline    │
   │      │ palani: why wrapping?    │           ← inline    │
   │      │ alice: overflow in fuzz  │           ← inline    │
   │      ╰──────────────────────────╯           ← inline    │
   │ 14   }                                      ← diff text │
   └────────────────────────────────────────────────────────┘
```

Four different consumers need to agree on this list:

```
   ┌─────────────────────────────────────────────────────┐
   │ navigation (j/k)   "what row is next?"              │
   │ scrolling          "how many rows are there?"       │
   │ mouse click        "which row is at this y?"        │
   │ renderer           "for these rows, paint each one" │
   └─────────────────────────────────────────────────────┘
```

### Today: each consumer rebuilds its own list

```
   keyboard         scroll wheel        mouse click       renderer
       │                 │                   │                │
       ▼                 ▼                   ▼                ▼
   rebuild list      rebuild list        rebuild list     rebuild list
   (visual_rows_     (visual_rows_       (visual_rows_    (visual_rows_
    with_inline_      with_inline_        with_inline_    with_inline_
    blocks)           blocks)             blocks)         blocks)

   ❌ FOUR independent rebuilds per frame.
   ❌ If inline state changes mid-frame, they disagree.
   ❌ Plus another function `row_count_for_mode()` that LIES
      (ignores inline rows) gets used in 5 scroll-wheel paths.
```

This is the concrete origin of the "scroll stops short" / "click hits wrong line" bugs.

### After: one borrowed list per frame

```
                ┌──────────────────────────────────┐
                │ DiffWorkspace                    │
                │   rows: Vec<WorkspaceVisualRow>  │
                │   rows_dirty: bool               │
                │                                  │
                │   fn rows(&mut self) -> &[Row] { │
                │     if self.rows_dirty {         │
                │       self.rebuild();            │
                │       self.rows_dirty = false;   │
                │     }                            │
                │     &self.rows                   │
                │   }                              │
                └──────────┬───────────────────────┘
                           │ &[Row]  (borrow, no copy)
       ┌──────────┬────────┴────────┬────────────┐
   keyboard    scroll wheel      mouse click     renderer
   reads same  reads same        reads same      reads same
   slice       slice             slice           slice

   ✅ ONE rebuild per state change (Pierre's pattern).
   ✅ All four consumers iterate the exact same memory.
   ✅ Borrow-checker forbids mutating while a reader holds the slice.
```

### Where Rust's rules slot in

- `rebuild()` is `&mut self` → only one rebuilder at a time.
- After `rebuild()`, the method returns `&[Row]` (a read-only slice) → many readers can share it.
- While any reader holds the slice, the compiler refuses to call `rebuild()` again (would need `&mut self`).
- → impossible to mid-frame mutate the list out from under a paint pass.

### The row enum

```rust
pub enum WorkspaceVisualRow {
    DiffText      { side: Side, doc_row: usize, … },
    InlineReview  { block_id: BlockId, line: usize, … },
    Spacer        { side: Side },
    // future kinds added here; compiler forces every consumer to handle them
}
```

A new row kind (AI suggestion, CI status, collapsed thread summary) is a new enum variant. Every place that matches on the enum gets a compile error until it handles the new variant. **That's the architectural ratchet**: future extension cannot be added in one place without acknowledging all four consumers.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Section 7 — Effects: IO leaves through one door

**Status: ☐ queued**

### The shape

The workspace's `update(intent)` does **only state updates**. It does not save to disk, post to GitHub, write to the clipboard, or read a file. Instead, it returns a list of **effects** — small descriptions of "please do this for me."

```rust
enum Effect {
    SaveDraft   { id: DraftId, body: String },
    PostComment { thread: ThreadId, body: String, gen: GenId },
    CopyToClipboard { text: String },
    FetchPRDetails  { pr: PrId, gen: GenId },
    // …
}

impl DiffWorkspace {
    pub fn update(&mut self, intent: Intent) -> Vec<Effect> { … }
}
```

### Picture

```
   intent in                                     effects out
       │                                             │
       ▼                                             ▼
   ┌────────────────────────────────────────────────────┐
   │ DiffWorkspace::update(intent)                      │
   │   - PURE state changes only                        │
   │   - NO clipboard, NO disk, NO network              │
   │   - returns Vec<Effect> for app shell to perform   │
   └────────────────────────────────────────────────────┘
                          │
                          ▼
   ┌────────────────────────────────────────────────────┐
   │ App::run_effects(effects)                          │
   │   - performs the IO                                │
   │   - on async completion, sends ResultIntent back   │
   │     to workspace via .update(...)                  │
   └────────────────────────────────────────────────────┘
```

### Why this matters

1. **Testing.** You can hand the workspace a sequence of intents and assert on `(new_state, returned effects)`. No mocks. No IO. The reducer is pure.
2. **Replay.** You can record intents and replay them.
3. **Reasoning.** When you read `update()`, you see "what the state did." When you read `run_effects()`, you see "what the world did." They don't tangle.
4. **Async safety.** Combined with generation tokens (Section 8), stale async results can be safely ignored.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Section 8 — Generation tokens: making async safe

**Status: ☐ queued**

### The problem

You start a fetch for PR #42's comments. While that's running, you navigate to PR #99. The fetch for #42 completes 3 seconds later. Without protection, the result quietly overwrites the workspace state — now PR #99 shows PR #42's comments.

### The fix: every async request carries a generation token

```rust
struct GenId(u64);

impl DiffWorkspace {
    current_gen: GenId,
}
```

When the workspace starts an async effect, it stamps the current `gen` on the effect. When the result comes back as a new intent:

```rust
Intent::PRCommentsLoaded { gen: GenId, comments: Vec<…> }
```

…the workspace compares `gen` to `self.current_gen`. If they differ, the result is for a stale request → drop it on the floor. Every state-changing navigation bumps the generation.

### Picture

```
   T=0:  workspace gen=1
         user opens PR #42
         emits Effect::FetchPRDetails { pr: 42, gen: 1 }

   T=1:  user navigates to PR #99
         workspace bumps gen to 2
         emits Effect::FetchPRDetails { pr: 99, gen: 2 }

   T=3:  PR #42 fetch finishes
         App sends Intent::PRLoaded { pr: 42, gen: 1, … }
         workspace: gen=1 != current 2 → DROP (no state change)

   T=5:  PR #99 fetch finishes
         App sends Intent::PRLoaded { pr: 99, gen: 2, … }
         workspace: gen=2 == current → apply
```

### Why it matters in LazyDiff

Drafts, comment posts, thread loads, semantic diff fetches, CI status calls — every one of these can outrun the user's navigation. Generation tokens are the architectural pattern that keeps "fast user + slow network" honest.

### ❓ Your doubts

_(to be filled in chat)_

### 💡 Your aha

_(to be filled in chat)_

---

## Glossary (running)

- **Owner** — the one variable/struct/field that holds a value. When it dies, the value dies.
- **Borrow** — a temporary, restricted reference (`&T` read or `&mut T` write) the owner hands out without giving up ownership.
- **Module** — a Rust namespace, usually one file. Privacy boundary.
- **Crate** — a Rust compilation unit. Strongest privacy boundary.
- **Intent** — a user-meaningful action expressed as a value.
- **Effect** — a side-effect-to-be-performed, returned from the workspace for App to execute.
- **Visual row** — one screen-row slot in the diff workspace.
- **Generation token** — a monotonic id stamped on async requests so stale results can be dropped.
- **Dirty flag** — a bool that says "the cache needs rebuilding before the next read."

---

## How we use this notebook

1. I teach Section N in chat with extra examples and ASCII.
2. You say "got it" or "wait, this part…".
3. I append your doubt to the section's ❓ block in this file.
4. When you confirm understanding, I check off the section in the Progress Map and add any aha-notes to the 💡 block.
5. We move to Section N+1.

When all 8 sections are ticked, you'll have a complete mental model — both for reading the current code AND for executing the migration.
