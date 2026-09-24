# engram

> Give your agents a brain.

Semantic memory for AI agents. Index your knowledge base, search by meaning — not keywords. Single binary, local models, no server required.

AI agents are only as good as what they can recall. **engram** gives agents persistent, searchable memory over any collection of plain text and markdown files — without a database server, cloud service, or complex infrastructure.

```
$ engram search "something warm and filling"
 1. recipes/tuscan-white-bean-soup.md     (dist: 0.969)
    # tuscan-white-bean-soup.md  Slow-simmer cannellini beans...
```

Give an agent access to `engram search` and it can retrieve the right context from thousands of documents in milliseconds — without stuffing everything into the prompt.

## Keyword search vs. semantic search

Say you keep a folder of recipe notes:

```markdown
# tuscan-white-bean-soup.md

Slow-simmer cannellini beans with pancetta, kale, and a parmesan rind
in chicken stock. Needs at least 90 minutes for the beans to turn creamy.
Season aggressively at the end — beans soak up salt.

Serve with thick crusty bread. Leftovers get better overnight as the
beans release more starch and the broth thickens.
```

Searching for a specific word works fine:

```
$ grep -rl "cannellini" recipes/
recipes/tuscan-white-bean-soup.md
```

But searching by feeling draws a blank:

```
$ grep -rl "something warm and filling" recipes/
(no matches)
```

engram finds it — the meaning matches even though none of the words do:

```
$ engram search "something warm and filling" --limit 5
 1. recipes/tuscan-white-bean-soup.md      (dist: 0.969)
    # tuscan-white-bean-soup.md  Slow-simmer cannellini beans...
```

Lower distance = stronger match. Results are ranked by semantic similarity across your entire collection — without a server, without a cloud API, and without knowing ahead of time which words your notes use.

## Why engram for agents?

| Problem | engram's answer |
|---|---|
| Context windows are finite | Retrieve only what's relevant, not everything |
| Agents forget between sessions | Persistent index survives restarts |
| RAG needs a database server | sqlite-vec runs as a single file, zero infra |
| Cloud embedding APIs leak data | Local Ollama embedding, nothing leaves the machine |
| Binary format lock-in | Plain files stay plain — any tool can still read them |

## Design principles

- **Your files are never modified.** engram reads them; it never writes to them.
- **The index is a sidecar.** `~/.engram/index.db` is a derived artifact — delete it and rebuild anytime. Your markdown is always the source of truth.
- **Local-first, private by default.** Works with a local [Ollama](https://ollama.com) instance (`nomic-embed-text`) — no API key, no data leaving your machine. Falls back to OpenAI-compatible APIs when Ollama isn't available.
- **One binary, no runtime deps.** SQLite is bundled. Ships as a single static binary for all major platforms.
- **Plain files stay plain.** Works alongside `grep`, `ripgrep`, `fzf`, git, Obsidian, or anything else that reads markdown.

## Installation

```bash
# From source (requires Rust 1.75+)
cargo install --git https://github.com/equitek/engram

# Pre-built binaries
# https://github.com/equitek/engram/releases
```

## Quick start

```bash
# Index your notes — index is created automatically on first run
engram add ~/notes

# Search by meaning
engram search "something warm and comforting for a cold night"

# Check what's indexed
engram status
```

## Using engram with AI agents

engram is designed to be called as a tool from agent systems (OpenClaw, LangChain, custom loops, etc.):

```bash
# Agent retrieves relevant context before answering
engram search "$USER_QUESTION" --limit 5

# Agent indexes a new document after capturing knowledge
engram add ~/notes/new-runbook.md

# Machine-readable JSON output for programmatic integration
engram search "$USER_QUESTION" --json --limit 5

# Use a separate index for a specific project
engram --index /path/to/project-index.db add ~/project/docs
engram --index /path/to/project-index.db search "deployment"
```

The output is plain text by default — easy to parse, pipe, or inject directly into a prompt. Use `--json` for machine-readable output. No SDK required.

## Commands

| Command | Description |
|---|---|
| `engram add <paths...>` | Index files or directories (creates index on first run) |
| `engram search <query>` | Search by meaning |
| `engram remove <paths...>` | Remove files from index |
| `engram rebuild` | Rebuild index from scratch |
| `engram status` | Show index stats |

**Global options:**

| Flag | Description |
|---|---|
| `--index <path>` | Use a custom index database path (overrides `~/.engram/index.db` and `ENGRAM_DB_PATH`) |

**Options for `add`:**

| Flag | Description |
|---|---|
| `--no-progress` | Plain text output instead of progress bar (useful in scripts/CI) |
| `--recursive` / `-r` | Recursively index directories (default: on) |

**Options for `search`:**

| Flag | Description |
|---|---|
| `-l`, `--limit <n>` | Number of results to return (default: 10) |
| `-p`, `--show-path` | Only show file paths (no snippets) |
| `--json` | Output results as a JSON array (machine-readable) |

### JSON output format

When `--json` is passed to `search`, output is a JSON array of objects:

```json
[
  {
    "path": "recipes/tuscan-white-bean-soup.md",
    "snippet": "# tuscan-white-bean-soup.md  Slow-simmer cannellini beans…",
    "distance": 0.969
  }
]
```

## Embedding providers

engram auto-detects the best available provider at init time and records the choice in the index. To switch providers, run `engram rebuild`.

| Provider | How to enable | Privacy |
|---|---|---|
| `ollama/nomic-embed-text` | `ollama pull nomic-embed-text` | ✅ Fully local |
| `openai/text-embedding-3-small` | Set `OPENAI_API_KEY` | Cloud API |
| OpenRouter | Set `OPENROUTER_API_KEY` | Cloud API |

**Default:** Ollama (local) when available, otherwise OpenAI-compatible.

## Supported file types

`.md` `.txt` `.rst` `.org` `.adoc`

## Platforms

| Platform | Status |
|---|---|
| Linux x86_64 | ✅ |
| Linux aarch64 | ✅ |
| macOS aarch64 (Apple Silicon) | ✅ |
| macOS x86_64 | ✅ |
| Windows x86_64 | planned |

## Contributing

PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT