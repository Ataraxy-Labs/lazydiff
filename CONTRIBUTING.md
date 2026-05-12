# Contributing

Thanks for helping improve LazyDiff.

## Local checks

Please run these before opening a PR:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
```

## Development build

```bash
cargo build --profile dev-fast
./target/dev-fast/lazydiff
```

## Bug reports

Include:

- OS and terminal emulator
- LazyDiff version or commit SHA
- what diff/repo shape triggered the issue
- steps to reproduce
