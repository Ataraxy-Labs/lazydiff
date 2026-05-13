# LazyDiff

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.4-orange.svg)](https://github.com/Ataraxy-Labs/lazydiff/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

LazyDiff is a fast terminal UI for reviewing Git diffs and GitHub pull requests.
It focuses on the code-review loop: move through files, inspect hunks, browse
semantic changes, search, keep lightweight notes, and stay in the terminal.

> **Alpha:** LazyDiff is ready for early adopters and internal dogfooding, not a
> production-stable public launch yet. UI details, persistence, integrations, and
> release packaging may still change between alpha releases.

## Install

The recommended alpha install path is the installer script. It downloads the
matching GitHub Release archive and installs `lazydiff` into
`~/.lazydiff/bin/lazydiff`.

```bash
curl -fsSL https://raw.githubusercontent.com/Ataraxy-Labs/lazydiff/main/install | sh
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/Ataraxy-Labs/lazydiff/main/install | sh -s -- --version v0.1.0-alpha.4
```

Install from an already-built local binary:

```bash
./install --binary target/release/lazydiff
```

The installer also supports:

```bash
./install --no-modify-path
./install --version v0.1.0-alpha.4
```

If `~/.local/bin` is on your `PATH`, you can symlink the installed release
binary there:

```bash
ln -sfn ~/.lazydiff/bin/lazydiff ~/.local/bin/lazydiff
```

## Update

LazyDiff can update itself from GitHub Releases:

```bash
lazydiff update
```

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

## GitHub and cloud features

GitHub-backed PR review uses device login:

```bash
lazydiff login
lazydiff logout
```

After login, LazyDiff can load the PR review queue/home experience, open PRs,
show descriptions, browse semantic changes, and keep review state in the local
cache. Cloud-backed metadata is configured at build time by the release workflow.

For custom builds, the Convex endpoints can be overridden with:

```bash
LAZYDIFF_CONVEX_URL=https://your-deployment.convex.cloud \
LAZYDIFF_CONVEX_HTTP_URL=https://your-deployment.convex.site \
cargo build --release
```

## Semantic review

LazyDiff uses `sem-core` to turn noisy file diffs into a semantic tree of changed
entities. In PR review, the detail pane can show both the extracted changes and
the pull request description without leaving the terminal.

Release builds currently use a minimal sem grammar set — TypeScript/TSX,
JavaScript/JSX, Python, Go, Rust, and Java — to keep binaries small. Unsupported
languages still fall back to normal textual diff review.

## Useful keys

| Key | Action |
| --- | --- |
| `j` / `k` | Move line |
| `ctrl-d` / `ctrl-u` | Half-page scroll |
| `/` | Search diff text |
| `f` | Open file picker |
| `m` | Toggle split/unified diff mode |
| `enter` | Open/toggle focused item |
| `q` / `esc` | Back / quit |

## Support Status

- Local Git diff review, GitHub PR dogfooding, release archives, and the install
  script are supported alpha paths.
- Release artifacts are published as platform archives with `.sha256` files.
- The `lazydiff-diffs` crate is vendored in this repo today; crates.io packaging
  is not supported yet.
- Package-manager distribution, binary signing, and broader language grammar
  selection are still future work.

## Current Caveats

- Expect UI and persistence details to change across alpha releases.
- Some code paths are intentionally still extraction-era cleanup work.
- The repo currently builds and tests cleanly, but clippy still reports warnings.
- GitHub and cloud-backed flows are still dogfood-grade.

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

Fast local build:

```bash
cargo build --profile dev-fast
target/dev-fast/lazydiff
```

Recommended local symlink split:

```bash
ln -sfn "$PWD/target/dev-fast/lazydiff" ~/.local/bin/lazydiff-dev
ln -sfn ~/.lazydiff/bin/lazydiff ~/.local/bin/lazydiff
```

Quality checks:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo build --profile dev-fast
```

For TUI dogfooding in a second terminal/tmux window, install `watchexec`
and run:

```bash
scripts/dev-watch-tui.sh
```

It rebuilds with `dev-fast` and restarts `target/dev-fast/lazydiff` on Rust or
Cargo manifest changes.

## Release

Pushing an alpha tag like `v0.1.0-alpha.4` runs the release workflow and uploads
platform archives to a GitHub Release:

- `lazydiff-linux-x86_64.tar.gz`
- `lazydiff-macos-arm64.tar.gz`
- `lazydiff-windows-x86_64.zip`
- one `.sha256` checksum per archive

```bash
git tag v0.1.0-alpha.4
git push origin v0.1.0-alpha.4
```

The release workflow builds with production Convex URLs and smoke-tests
`--version` and `--help` before packaging.

## License

MIT
