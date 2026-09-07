# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-09-07

### Added

- `sessions --sort` (date/calls/cache_read/sub_read/output/cost, `-` prefix for ascending) and
  `sessions --grep`, which matches a session's title, project or id.
- `token-perf upgrade`, which updates an installer-script install in place (`--check` only reports
  whether a newer release exists). Any other install has no receipt and is told so rather than
  failing.
- A spike table in `report`: the calls where the context grew the most, so the one tool result that
  set the bill is visible without reading the whole list.

### Changed

- The web UI is rebuilt on Tailwind CSS v4 with three hash-routed views (sessions, report, summary),
  charts for context growth, and deep links that survive a reload.
- Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) instead of a hand-written
  workflow. `token-perf-installer.sh` and `token-perf-installer.ps1` are now published alongside the
  archives, and every archive is `.tar.xz` (`.zip` on Windows) rather than `.tar.gz`.

### Fixed

- The server renders `index.html` in the configured language, so the first paint no longer flashes
  English before the UI switches.
- The web amplification card reads as a multiplier in Korean instead of a literal translation.

## [0.1.0] - 2026-09-05

- First release: a Rust rewrite of the TypeScript prototype, shipping a `token-perf` CLI with an
  embedded Svelte web UI (`token-perf serve`).
- Attributes residual cache-re-read cost per tool by differencing usage across calls, and rolls
  subagent transcripts into their parent session.
- Stores parsed sessions in SQLite and re-reads only what grew since the last run.
- `--json` on `sessions`/`report`/`summary`, `--cost` at Anthropic list prices, en/ko localisation,
  and prebuilt binaries for macOS, Linux and Windows.

[Unreleased]: https://github.com/M1nu0x0/token-perf/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/M1nu0x0/token-perf/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/M1nu0x0/token-perf/releases/tag/v0.1.0
