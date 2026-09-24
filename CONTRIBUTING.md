# Contributing to engram

Thanks for your interest. Contributions of all kinds are welcome — bug reports, documentation improvements, new features, and tests.

## Getting started

```bash
git clone https://github.com/equitek/engram
cd engram
cargo build
cargo test
```

Requirements: Rust 1.75+. SQLite is bundled (no system dependency needed).

## Running the tests

```bash
cargo test                  # all tests
cargo test chunk            # filter by name
cargo test -- --nocapture   # show println output
```

For integration tests that index real files, set `ENGRAM_TEST_EMBED=1` to use
a deterministic mock embedding provider instead of Ollama:

```bash
ENGRAM_TEST_EMBED=1 cargo test
```

## Code style

- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass
- No `unwrap()` in library code — use `?` and `anyhow::Context`

## Project layout

```
src/
  main.rs      — CLI entry point, arg dispatch
  cli.rs       — clap command definitions
  db.rs        — SQLite/sqlite-vec operations
  embed.rs     — embedding providers (Ollama, OpenAI, mock)
  index.rs     — add/search/remove/rebuild logic, chunking
tests/
  integration.rs — end-to-end tests using temp directories
```

## Submitting a PR

1. Fork the repo and create a branch from `main`
2. Make your changes with tests where applicable
3. Ensure `cargo test`, `cargo fmt --check`, and `cargo clippy` all pass
4. Open a PR with a clear description of what changed and why

## Reporting issues

Open an issue on GitHub with:
- engram version (`engram --version`)
- OS and architecture
- Steps to reproduce
- Expected vs. actual behavior
