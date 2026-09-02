# token-perf

[한국어](README.ko.md)

A tool that finds **where a coding agent wastes tokens**, using nothing but local logs.
The goal is attribution, not aggregation — not "how much did I spend this month" but "how much did that one tool charge me across the whole session".

## Why this is needed

The cost of a long session is not output, it is **cache re-reads**. One session from my own logs:

```
793 calls · 786.7K output · 243.3M cache re-read
```

A token that enters the context once is billed again on every remaining call.
So a single 13K tool result on the 6th call charges 8M by the time the session ends.
token-perf attributes that **residual cost** to each tool and ranks them.

Measurement works by differencing usage:

```
context(n) = input + cache_write + cache_read
grew(n)    = context(n) - context(n-1) - output(n-1)   # tokens that entered the context in between
residual(n) = grew(n) × (calls left before the next compaction)  # once the context folds, it ends there
```

Unlike character-count estimates, this stays accurate even when images and binaries are mixed in, because these are the numbers the server actually counted.

## Storage

Sessions that have been read accumulate in SQLite at `$XDG_DATA_HOME/token-perf/token-perf.db`
(`~/.local/share/…` by default). Claude Code deletes transcripts after 30 days by default
(`cleanupPeriodDays`), but sessions kept here stay analyzable after the originals are gone. Every run
re-reads only the grown part of the files that changed (in my environment, ~2s for a first scan of 649
files, ~0.03s after that). The raw JSON of each usage block is stored alongside, so a field you need
later can be pulled out with `json_extract` without a re-scan.

## Install

Prebuilt binaries for the latest release:

```sh
# macOS (Apple silicon)
curl -fsSL https://github.com/M1nu0x0/token-perf/releases/latest/download/token-perf-aarch64-apple-darwin.tar.gz | tar -xz
# macOS (Intel)
curl -fsSL https://github.com/M1nu0x0/token-perf/releases/latest/download/token-perf-x86_64-apple-darwin.tar.gz | tar -xz
# Linux (x86_64)
curl -fsSL https://github.com/M1nu0x0/token-perf/releases/latest/download/token-perf-x86_64-unknown-linux-gnu.tar.gz | tar -xz
```

That leaves a `token-perf` binary in the current directory; move it somewhere on your `PATH`.
Each archive ships a `.sha256` next to it.

## Usage

```sh
token-perf sessions          # session list, by cache re-read
token-perf report [SESSION]  # residual cost per tool (latest session if omitted)
token-perf serve             # web UI (OS picks a free port; the address is printed on start)
token-perf serve --port 5177 # fixed port (--no-open skips launching a browser)
```

When working on the UI, leave `token-perf serve --port 5177` running and use `cd web && npm run dev` —
the vite dev server proxies `/api` to 5177.

### Language

en · ko · ja are supported. The CLI output and the web UI use the same language.

```sh
token-perf --lang ja sessions   # Japanese from this run on, saved to config so it persists
```

The decision order is `--lang` → saved config → `LC_ALL`/`LANG` → `en`. If the code is not supported,
the flag is ignored (and not saved) and only a one-line warning is printed — one typo should not
cost you the language you saved. `--help` stays in English.

## Build

To build from source instead:

```sh
(cd web && npm ci && npm run build)   # the UI must be built first to be embedded in the binary
cargo build --release
```

Building the UI requires Node. Skip it and the release build fails — a binary without the UI serves
nothing but 404s, so the build is not allowed to pass quietly.

## Layout

```
crates/core/     source-agnostic model and analysis (common/) + per-agent parsers (sources/) + axum router
crates/cli/      clap-based CLI
web/             Svelte UI. Its build output is embedded into core
```

To add a new agent, implement `Source` in `sources/` and add one line to `sources::all()`.
The analysis runs on the normalized model, so it needs no changes.

Currently supported: Claude Code (`~/.claude/projects/**/*.jsonl`).

## Pitfalls when reading transcripts

The official docs state that this file format is **an internal implementation detail that may change
between versions** (`code.claude.com/docs/en/claude-directory`). The pitfalls I confirmed in my own
logs — all numbers are from that sample:

- **One response is split across several lines.** Cumulative snapshots and incremental chunks arrive
  mixed together under the same `message.id`. Read only the first line and you miss 72% of the tools
  and half the output tokens; add them all up and the tokens double. **The last line holds the final
  usage**, while tools must be collected from every line and deduplicated by `tool_use.id`.
- **An empty usage stub** arrives first. Count it as a call and the context series collapses to 0.
- **Cache writes have a different unit price per TTL.** `cache_creation.ephemeral_1h_input_tokens`
  is 2× the input price, the 5-minute one 1.25×. Read only the flat field and you cannot tell them
  apart.
- **Tool output is truncated three times.** Full → `toolUseResult.stdout` (30K) → the `tool_result`
  body. Only the last one entered the context, so using `persistedOutputSize` gives the wrong answer.
- **Subagents are separate files** (`<session>/subagents/agent-*.jsonl`). The `.meta.json` next to
  them says which of the parent's tool calls they came from.
- **Deleted after 30 days by default** (`cleanupPeriodDays`).

## License

MIT
