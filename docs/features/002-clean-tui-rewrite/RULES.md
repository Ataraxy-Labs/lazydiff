# Feature 002 rewrite rules

## Active decision

ADR 0009 supersedes feature 001 as the active execution track. ADRs 0001–0008 remain constraints for v2 unless amended.

## Slice rule

- One vertical product-flow slice at a time.
- Keep v2 isolated from legacy `src/app.rs` and `src/app/*`.
- Prefer deleting or obsoleting legacy code after parity gates, not wrapping it.
- For new TUI behavior, write the termwright test first and make it pass.
- For non-visual internal code, focused unit tests are enough.

## Stop conditions

Stop only when one of these is true:

1. All compulsory items in `plan.md` are checked and all issues are done or blocked with a human follow-up.
2. A human architecture/product decision blocks progress.
3. Verification fails in a way that cannot be resolved within the current slice; file the failure as an issue and surface it.

Otherwise run:

```sh
bash scripts/work.sh next
```

## Commit format

One completed issue per commit. Commit body includes:

- Issue id.
- What v2 seam or owner was made deeper.
- What legacy path was avoided, replaced, or queued for deletion.
- Test/check that protects it.
- North-Star check.
