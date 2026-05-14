# Semantic Review Product Flow

## North Star

lazydiff should turn code review into clearing a semantic map, not scrolling files.

The reviewer should always know:

- what they are reviewing,
- where they are in the PR,
- what is risky,
- what has been cleared,
- what should happen next,
- how to jump into full context and back out naturally.

The product loop should feel game-like in structure, without becoming gimmicky: the PR is a map, semantic groups are zones, changed entities are rooms, and high-risk changes are boss rooms.

```text
Open PR
  ↓
Briefing screen
  ↓
World Map
  ↓
Enter Zone
  ↓
Clear Rooms
  ↓
Boss/Risk Rooms
  ↓
Tests / Evidence
  ↓
Completion
```

## Core Concepts

### World Map

The high-level semantic graph of the PR. It shows zones, progress, hazards, and the next recommended place to go.

### Zone

A coherent review area. Initially this can be deterministic: directory, file group, semantic parent, tests, or risk cluster. Later, LLMs may name zones more naturally.

### Room

A reviewable semantic unit: function, class, component, route handler, test case, config block, or fallback file section.

### Boss / Risk Room

A room with elevated review risk. Risk should start deterministic and can later incorporate external analysis.

Possible deterministic hazards:

- large change,
- public/exported API changed,
- deleted code,
- auth/session/token/storage code,
- migration/cleanup path,
- error handling changed,
- no nearby test change,
- failed CI check.

### Inspect Integration

Difficulty and risk scoring should use `inspect` where possible. There is a local checkout at `../work/inspect`, and an official crate exists as well. The Semantic Review flow should treat inspect as a structured analysis source for:

- difficulty level,
- risk/hazard signals,
- dependency/call graph impact,
- possible review ordering,
- evidence/test relationships,
- complexity ramps.

The UI should still work without LLM calls. `inspect` and deterministic heuristics should provide the first useful version of difficulty and guidance.

## Navigation Principles

```text
enter    go deeper / open selected
esc      go back one level
n        next recommended
space    clear selected room/scope
f        flag concern
m        world map
d        raw diff / full context
t        tests/evidence
?        help
```

Rule of thumb:

- `enter` means go deeper.
- `esc` means come back.
- `n` means guide me.
- `space` means I understand / clear this.
- `f` means I am not comfortable / flag this.

```diagram
╭──────────╮ enter zone ╭──────────╮ enter room ╭──────────╮ enter ctx ╭──────────────╮
│ Map      │───────────▶│ Zone     │───────────▶│ Room     │──────────▶│ Full Context │
╰──────────╯◀───────────╰──────────╯◀───────────╰──────────╯◀──────────╰──────────────╯
       esc/back              esc/back                esc/back
```

## Persistent Review HUD

Every screen should include a compact status line so the reviewer never loses the thread.

Example:

```text
QUIVER · #6596 · HARD · 12/42 cleared · 2/6 hazards · next: transferWebhookOwner
```

The HUD should answer:

- Which PR am I reviewing?
- What stage/zone/room am I in?
- How much is cleared?
- How many hazards remain?
- What is recommended next?

---

# Product Screens

## 1. Loading / Preparation

Purpose: establish that lazydiff is preparing a structured review mission, not just loading a diff.

```text
╭────────────────────────────────────────────────────────────────────────────╮
│ QUIVER                                                                     │
│                                                                            │
│ Loading review map                                                         │
│                                                                            │
│   PR #6596                                                                 │
│   feat(bitbucket-dc): repo-first connections with decoupled webhook ownership│
│                                                                            │
│   ✓ fetched diff                                                           │
│   ✓ parsed files                                                           │
│   ◌ building semantic map                                                  │
│   ◌ running inspect difficulty analysis                                    │
│   ◌ preparing evidence trail                                               │
│                                                                            │
│                         ░░░░░░░░░░░░░░░░░░░░ 64%                            │
╰────────────────────────────────────────────────────────────────────────────╯
```

## 2. Review Briefing

Purpose: summarize complexity and recommended approach before exposing code.

```text
╭─ Review Briefing ──────────────────────────────────────────────────────────╮
│ #6596  feat(bitbucket-dc): repo-first connections                          │
│ Ataraxy-Labs/quiver · quiver-product-flow                                  │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ Difficulty       HARD                                                      │
│ Size             +1059 -274 · 20 files                                     │
│ Checks           5/6 passing · Build packages failed                        │
│ Review Map       5 zones · 42 rooms · 6 hazards                             │
│                                                                            │
│ Suggested Route                                                            │
│   1. Data model changes                                                     │
│   2. Repo-first connection flow                                             │
│   3. Webhook ownership                                                      │
│   4. Event handlers                                                         │
│   5. Tests / evidence                                                       │
│                                                                            │
│ Why this order?                                                            │
│   Start with shape/data changes, then flow, then risky ownership code,      │
│   then confirm behavior with tests.                                         │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ enter begin review   m map   d raw diff   ? controls                       │
╰────────────────────────────────────────────────────────────────────────────╯
```

Notes:

- Difficulty should use inspect when available.
- v1 route can be deterministic: models/types → public APIs → implementation → hazards → tests.
- Later, AI can improve names/reasons, but this screen should not depend on LLM calls.

## 3. World Map

Purpose: show the whole PR as a graph of zones, progress, and hazards.

```text
╭─ World Map ────────────────────────────────────────────────────────────────╮
│ #6596 repo-first Bitbucket DC connections                     0/42 cleared │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│                              ◐ Data Models                                 │
│                                  │                                         │
│                     ─────────────┴─────────────                            │
│                     │                          │                           │
│            ○ Connection Flow             ⚠ Webhook Ownership                │
│                     │                          │                           │
│             ────────┴────────             ─────┴─────                      │
│             │               │             │          │                     │
│      ○ Repo Creation   ○ Validation   ⚠ Owner Move   ○ Cleanup             │
│                                                                            │
│                              ○ Event Handlers                              │
│                                  │                                         │
│                                ✓ Tests                                     │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ Next recommended: Data Models                                              │
│ Reason: data shape changes affect every downstream room                    │
│                                                                            │
│ enter zone   n next recommended   space clear if leaf   d raw diff   ? help │
╰────────────────────────────────────────────────────────────────────────────╯
```

Node statuses:

```text
○ unvisited
◐ in progress
✓ cleared
⚠ hazard
✕ blocked
▣ selected
```

The World Map should be the primary entry point into review. It should answer:

- What zones exist?
- Which zones are cleared?
- Where are the hazards?
- What should I do next?

## 4. Enter Zone Transition

Purpose: make entering a zone feel spatial and natural.

Frame 1:

```text
╭─ World Map ─────────────────────────────╮
│                                         │
│             ⚠ Webhook Ownership         │
│                  │                      │
│              ────┴────                  │
│              │       │                  │
│        ⚠ Owner Move  ○ Cleanup          │
│                                         │
╰─────────────────────────────────────────╯
```

Frame 2:

```text
╭─ Entering Webhook Ownership ────────────╮
│                                         │
│              ⚠ Webhook Ownership        │
│             ╱                 ╲         │
│       ⚠ Owner Move          ○ Cleanup    │
│                                         │
╰─────────────────────────────────────────╯
```

Frame 3:

```text
╭─ Zone: Webhook Ownership ───────────────╮
│ 0/7 rooms cleared · 2 hazards           │
╰─────────────────────────────────────────╯
```

TUI animation can be simple:

- selected node brightens for a few frames,
- non-selected map dims,
- breadcrumb changes,
- zone outline appears.

## 5. Zone Screen

Purpose: focus review on one coherent area. This is the main Bike-like screen.

```text
╭─ Zone: Webhook Ownership ─────────────────────────────────────────────────╮
│ PR #6596 › Webhook Ownership                         0/7 cleared · 2 hazards│
├───────────────────────────────┬────────────────────────────────────────────┤
│ Rooms                         │ Scope Diff                                 │
│                               │                                            │
│ ▣ ⚠ transferWebhookOwner      │ transferWebhookOwner                       │
│ ○ detachConnectionWebhook     │ function · +28 -11 · hazard               │
│ ○ attachRepositoryWebhook     │                                            │
│ ○ cleanupConnectionWebhook    │ @@ transferWebhookOwner                    │
│ ✓ normalizeWebhookPayload     │ - owner = connection.id                    │
│                               │ + owner = repository.id                    │
│                               │ + await linkWebhookToRepository(...)       │
│                               │                                            │
│                               │                                            │
│                               │                                            │
├───────────────────────────────┴────────────────────────────────────────────┤
│ j/k room   h/l collapse/expand   enter context   space clear   esc map     │
╰────────────────────────────────────────────────────────────────────────────╯
```

Core mechanic:

```text
left row selected → right pane shows scoped diff
```

The tree is the table of contents for the zone.

## 6. Room Screen

Purpose: let the reviewer focus on a single semantic entity.

```text
╭─ Room ────────────────────────────────────────────────────────────────────╮
│ PR #6596 › Webhook Ownership › transferWebhookOwner              ⚠ HAZARD │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ function · src/bitbucket/event-handlers.ts:122                              │
│ +28 -11                                                                     │
│                                                                            │
│ Hazard                                                                      │
│   Ownership semantics changed from connection-owned to repository-owned.    │
│                                                                            │
│ Diff                                                                        │
│                                                                            │
│ @@ transferWebhookOwner                                                     │
│ - const ownerId = connection.id                                             │
│ - await updateWebhookOwner(webhook.id, ownerId)                             │
│ + const ownerId = repository.id                                             │
│ + await linkWebhookToRepository(repository.id, webhook.id)                  │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ space clear   f flag   enter full context   n next   esc back to zone      │
╰────────────────────────────────────────────────────────────────────────────╯
```

The Room can initially be represented inside the Zone layout. A dedicated room screen is useful for high-focus or boss rooms.

## 7. Full Context Screen

Purpose: scoped review must have an escape hatch to raw/full context, then return naturally.

```text
╭─ Full Context ─────────────────────────────────────────────────────────────╮
│ PR #6596 › Webhook Ownership › transferWebhookOwner › full diff             │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ src/bitbucket/event-handlers.ts                                             │
│                                                                            │
│ 117    function transferWebhookOwner(...) {                                 │
│ 118      const webhook = await getWebhook(...)                              │
│ 119                                                                      │
│ 120  -   const ownerId = connection.id                                      │
│ 121  -   await updateWebhookOwner(webhook.id, ownerId)                      │
│ 122  +   const ownerId = repository.id                                      │
│ 123  +   await linkWebhookToRepository(repository.id, webhook.id)           │
│ 124                                                                      │
│ 125      return webhook                                                     │
│ 126    }                                                                   │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ esc return to room   space clear room   / search   c comment               │
╰────────────────────────────────────────────────────────────────────────────╯
```

Transition back should highlight the selected room in the zone tree so the reviewer knows where they returned.

## 8. Boss / Risk Room

Purpose: high-risk code should visibly require more care.

```text
╭─ Boss Room ────────────────────────────────────────────────────────────────╮
│ PR #6596 › Webhook Ownership › transferWebhookOwner              ⚠ BOSS   │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ Why this is risky                                                          │
│   ⚠ ownership semantics changed                                            │
│   ⚠ deletion/cleanup path changed                                          │
│   ⚠ build currently failing                                                │
│   ○ direct test not yet cleared                                             │
│                                                                            │
│ Required checks to clear                                                    │
│   [ ] review implementation diff                                            │
│   [ ] inspect full context                                                  │
│   [ ] visit related test room                                               │
│                                                                            │
│ Diff                                                                        │
│ - owner = connection.id                                                     │
│ + owner = repository.id                                                     │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ enter context   t related tests   f flag   space clear when checks done    │
╰────────────────────────────────────────────────────────────────────────────╯
```

Boss rooms should guide, not block. If the user clears early, show a confirmation or suggestion to visit evidence.

## 9. Evidence Trail

Purpose: connect implementation to tests, docs, and checks.

```text
╭─ Evidence Trail ──────────────────────────────────────────────────────────╮
│ PR #6596 › Webhook Ownership › transferWebhookOwner                        │
├───────────────────────────────┬────────────────────────────────────────────┤
│ Related rooms                 │ Evidence Diff                              │
│                               │                                            │
│ ▣ test transfers owner        │ test/webhook-owner.test.ts                 │
│ ○ test cleanup path           │                                            │
│ ○ Build packages failure      │ + expect(webhook.ownerId).toEqual(repo.id) │
│                               │                                            │
│                               │                                            │
├───────────────────────────────┴────────────────────────────────────────────┤
│ space clear evidence   enter context   esc boss room                       │
╰────────────────────────────────────────────────────────────────────────────╯
```

v1 related evidence can be deterministic:

- test files under same directory,
- files with same symbol names,
- files containing entity name,
- changed tests in same PR,
- failed checks as global evidence.

`inspect` can improve impact/evidence relationships.

## 10. Zone Complete

Purpose: create closure and guide the next step.

```text
╭─ Zone Cleared ─────────────────────────────────────────────────────────────╮
│ Webhook Ownership                                                          │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ ✓ 7 / 7 rooms cleared                                                       │
│ ✓ 2 / 2 hazards reviewed                                                    │
│ ⚠ 1 concern flagged                                                         │
│                                                                            │
│ Notes                                                                      │
│   - transferWebhookOwner reviewed with full context                         │
│   - cleanupConnectionWebhook flagged for author clarification               │
│                                                                            │
│ Next recommended zone                                                       │
│   Event Handlers                                                            │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ enter next zone   m map   esc stay here                                    │
╰────────────────────────────────────────────────────────────────────────────╯
```

## 11. Tests / Evidence Stage

Purpose: tests become the final proof stage, not just another file.

```text
╭─ Tests / Evidence ─────────────────────────────────────────────────────────╮
│ PR #6596                                                     31/42 cleared │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ Test rooms                                                                 │
│   ✓ connection model tests                                                  │
│   ✓ repo-first creation tests                                               │
│   ○ webhook ownership tests                                                 │
│   ○ event handler tests                                                     │
│                                                                            │
│ Checks                                                                     │
│   ✓ check:format                                                            │
│   ✓ check:lint                                                              │
│   ✓ test                                                                    │
│   × Build packages                                                          │
│                                                                            │
│ Evidence gaps                                                              │
│   ⚠ Webhook cleanup path has no cleared test room yet                       │
│   × Build failure remains unresolved                                        │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ enter selected evidence   n next gap   f flag   m map                      │
╰────────────────────────────────────────────────────────────────────────────╯
```

## 12. Completion

Purpose: give closure and help the reviewer decide what to say or do.

```text
╭─ Review Complete ──────────────────────────────────────────────────────────╮
│ #6596 repo-first Bitbucket DC connections                                  │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│ Progress                                                                   │
│   ✓ 42 / 42 rooms visited                                                   │
│   ✓ 5 / 5 zones reviewed                                                    │
│   ✓ 6 / 6 hazards handled                                                   │
│                                                                            │
│ Remaining issues                                                           │
│   ⚠ 1 concern flagged                                                       │
│   × Build packages failing                                                  │
│                                                                            │
│ Review confidence                                                           │
│   High on implementation flow                                                │
│   Medium overall because CI is failing                                       │
│                                                                            │
│ Suggested final state                                                       │
│   Request changes / wait for build fix                                      │
│                                                                            │
│ Summary                                                                    │
│   Data model and connection flow reviewed. Webhook ownership path reviewed  │
│   with one concern around cleanup behavior. Tests mostly cover the new flow, │
│   but build failure must be resolved.                                       │
│                                                                            │
├────────────────────────────────────────────────────────────────────────────┤
│ c comment summary   m map   d raw diff   esc back                          │
╰────────────────────────────────────────────────────────────────────────────╯
```

---

# Transition/Animation Guidelines

Animations should be fast, useful, and spatial.

## Enter Zone

- selected map node brightens,
- rest of graph dims,
- subtree recenters,
- breadcrumb changes from PR → zone.

## Enter Room

- selected tree row flashes,
- room details expand into the right pane,
- scoped diff appears.

## Enter Full Context

- scoped diff expands into full diff,
- selected changed lines remain highlighted,
- surrounding context fades in.

## Back Out

- full context compresses back to room,
- room row flashes in left tree,
- prior scroll/selection state is preserved.

## Clear Room

- node status changes `○ → ✓`,
- parent rolls up `○ → ◐ → ✓`,
- HUD progress increments.

## Clear Boss

- hazard marker resolves `⚠ → ✓`,
- brief status message appears:

```text
Hazard cleared: ownership semantics reviewed
```

---

# Build Slices

## Slice 1: Review Shell

- add stages/routes: Briefing, Map, Zone, Room, FullContext, Completion,
- implement `enter`, `esc`, `n`, `m`, `d`, `space` semantics,
- persistent HUD,
- no fancy animation yet.

## Slice 2: Zone Tree + Scoped Diff

- semantic tree as zone/room table of contents,
- right pane scoped diff rendered by `lazydiff-diffs`,
- viewed/cleared state rolls up.

## Slice 3: World Map Integration

- graph nodes reflect progress,
- entering selected zone moves into Zone screen,
- returning preserves selection.

## Slice 4: Difficulty + Hazards via Inspect

- use `inspect` for difficulty and risk signals,
- mark boss/risk rooms,
- compute recommended route.

## Slice 5: Evidence

- related tests/checks screen,
- evidence gaps,
- boss-room required checks.

## Slice 6: Transitions

- node flash,
- dim/recenter,
- breadcrumb transition,
- clear animations.

## Slice 7: Completion

- review summary,
- remaining concerns,
- failed checks,
- suggested final state.

---

# Final Mental Model

```diagram
╭──────────────╮
│ Briefing     │  What am I reviewing?
╰──────┬───────╯
       ▼
╭──────────────╮
│ World Map    │  Where should I go?
╰──────┬───────╯
       ▼
╭──────────────╮
│ Zone         │  What rooms are in this area?
╰──────┬───────╯
       ▼
╭──────────────╮
│ Room         │  What exact code changed?
╰──────┬───────╯
       ▼
╭──────────────╮
│ Evidence     │  Can I trust it?
╰──────┬───────╯
       ▼
╭──────────────╮
│ Completion   │  What remains?
╰──────────────╯
```

The reviewer loop is:

```text
enter → inspect → clear → next → back when needed
```
