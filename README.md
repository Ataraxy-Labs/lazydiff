# LazyDiff

LazyDiff is a fast terminal UI for reviewing Git diffs. It focuses on the code-review loop: move through files, inspect hunks, search, keep lightweight notes, and stay in the terminal.

> Early extraction from Quiver's Ratatui diff viewer. APIs, storage, and branding are still being cleaned up for a public launch.

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

By default LazyDiff opens a local diff for the current Git repository. You can also pass a patch file:

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

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --profile dev-fast
```

## Release

Pushing a tag like `v0.1.0` runs the release workflow and uploads platform binaries to a GitHub Release.

```bash
git tag v0.1.0
git push origin v0.1.0
```

## License

MIT
