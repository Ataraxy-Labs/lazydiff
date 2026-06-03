# LazyDiff

[![Version](https://img.shields.io/badge/version-0.1.0--alpha.7-orange.svg)](https://github.com/Ataraxy-Labs/lazydiff/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

<img width="1312" height="812" alt="image" src="https://github.com/user-attachments/assets/b703b205-0554-4aa1-a2f7-9b4a29eee6da" />

A fast terminal UI for reviewing Git diffs and GitHub pull requests.

LazyDiff is for the code-review loop: jump through changed files, inspect hunks,
search the diff, browse semantic changes, and keep your focus in the terminal.

> **Alpha:** LazyDiff is currently intended for early adopters and dogfooding.
> Expect UI and workflow changes between alpha releases.

## Features

- Review worktree changes, staged changes, commits, refs, patch files, and stdin diffs.
- Browse pull requests from a terminal-first review queue.
- Open PR descriptions and changed files side by side.
- See semantic code changes powered by [`sem-core`](https://github.com/Ataraxy-Labs/sem).
- Search within diffs and jump directly to matching files or hunks.
- Switch between unified and split diff views.
- Persist lightweight review state locally.
- Login, logout, and update from the CLI.

## Installation

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/Ataraxy-Labs/lazydiff/main/install | sh
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/Ataraxy-Labs/lazydiff/main/install | sh -s -- --version v0.1.0-alpha.5
```

Verify the install:

```bash
lazydiff --version
```

### From source

```bash
cargo install --git https://github.com/Ataraxy-Labs/lazydiff
```

## Usage

Open LazyDiff in a Git repository:

```bash
lazydiff
```

Review local changes directly:

```bash
lazydiff diff                    # worktree changes vs HEAD
lazydiff diff --staged           # staged changes
lazydiff diff origin/main        # current branch vs a base ref
lazydiff show HEAD~1             # a commit
lazydiff patch path/to/file.diff # a patch file
git diff --no-color | lazydiff pager
```

GitHub PR review uses device login:

```bash
lazydiff login
lazydiff logout
```

The GitHub device login needs an OAuth app client ID. Configure it locally:

```toml
# ~/.config/lazydiff/config.toml
github_client_id = "your-github-oauth-client-id"
```

Update LazyDiff from GitHub Releases:

```bash
lazydiff update
```

## Keybindings

| Key | Action |
| --- | --- |
| `j` / `k` | Move line |
| `ctrl-d` / `ctrl-u` | Half-page scroll |
| `/` | Search |
| `f` | Open file picker |
| `m` | Toggle split/unified diff mode |
| `enter` | Open focused item |
| `q` / `esc` | Back / quit |

## Development

```bash
git clone https://github.com/Ataraxy-Labs/lazydiff.git
cd lazydiff
cargo run
```

Fast local TUI loop:

```bash
cargo build --profile dev-fast
scripts/dev-watch-tui.sh
```

Quality checks:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
```

## Release builds

Release artifacts are published as GitHub Release archives for Linux, macOS, and
Windows, with `.sha256` checksums.

Release builds currently include a focused semantic grammar set — TypeScript/TSX,
JavaScript/JSX, Python, Go, Rust, and Java — to keep binaries small. Other
languages still fall back to normal textual diff review.

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for
local development notes.

## License

MIT
