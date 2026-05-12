# LazyDiff

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.1-orange.svg)](https://github.com/Ataraxy-Labs/lazydiff/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

LazyDiff is a fast terminal UI for reviewing Git diffs. It focuses on the code-review loop: move through files, inspect hunks, search, keep lightweight notes, and stay in the terminal.

> **Alpha:** LazyDiff is ready for early adopters and internal dogfooding, not a production-stable public launch yet. APIs, storage, release packaging, and branding are still being cleaned up.

## Install from source

```bash
cargo install --git https://github.com/ataraxy-labs/lazydiff
```

## Run locally

```bash
git clone https://github.com/ataraxy-labs/lazydiff.git
cd lazydiff
cargo run
```

By default LazyDiff opens the main review queue/home experience. You can also jump directly into Hunk-style review commands:

```bash
cargo run -- diff                    # review worktree changes vs HEAD
cargo run -- diff --staged           # review staged changes
cargo run -- diff origin/main        # review current branch vs a base ref
cargo run -- show HEAD~1             # review a commit
cargo run -- patch path/to/change.diff
git diff --no-color | cargo run -- pager
```

For the fast local-review shortcut, use:

```bash
cargo run -- --branch
```

You can still pass a patch file directly:

```bash
cargo run -- path/to/change.diff
```

## Useful keys

| Key | Action |
| --- | --- |
| `j` / `k` | Move line |
| `ctrl-d` / `ctrl-u` | Half-page scroll |
| `[` / `]` | Previous / next file |
| `/` | Search diff text |
| `f` | Open file picker |
| `m` | Toggle split/unified diff mode |
| `q` / `esc` | Back / quit |

## Support Status

- Local Git diff review and source installs are the supported alpha path.
- GitHub PR/queue integrations are available for dogfooding, but still settling.
- The `ratatui-diffs` crate is vendored in this repo today; crates.io packaging is not supported yet.
- Release binaries are published from alpha tags, but checksums, signing, and package-manager installs are still future work.

## Current Caveats

- Expect UI and persistence details to change across alpha releases.
- Some code paths are intentionally still extraction-era cleanup work.
- The repo currently builds and tests cleanly, but clippy still reports warnings.

## Stored State

LazyDiff stores review sessions, review items, user preferences, cache entries, and last-viewed diff viewport state in one global SQLite database:

```text
$XDG_DATA_HOME/lazydiff/lazydiff.db
```

If `XDG_DATA_HOME` is unset, this defaults to:

```text
~/.local/share/lazydiff/lazydiff.db
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo build --profile dev-fast
```

## Release

Pushing an alpha tag like `v0.1.0-alpha.1` runs the release workflow and uploads platform binaries to a GitHub Release.

```bash
git tag v0.1.0-alpha.1
git push origin v0.1.0-alpha.1
```

## License

MIT
