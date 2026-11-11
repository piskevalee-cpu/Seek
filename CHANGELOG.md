# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). One version covers the whole
workspace; each release ships a milestone (SPEC 15).

## [Unreleased]

## [0.7.0] - 2026-11-11

### Added
- Repository bootstrap: specification moved to `docs/SPEC.md`, `CLAUDE.md`, `README.md`,
  `rust-toolchain.toml`, `CHANGELOG.md`, logo asset and the logo extraction script.
- Rust workspace with the `seek-core`, `seek-llm`, `seek-local`, `seek-tui` crates and the `seek`
  binary (SPEC 4.1).
- Theme tokens and the 25x9 logo bitmap extracted from `assets/logo.png` (SPEC 12).
- TUI: Home and Chat layout, prompt bar, thinking and loading spinners, footer chips, sidebar
  shell and reasoning show/hide (SPEC 10).
- Prompt-budget test enforcing the 1,500-token limit on the system prompt plus the default tool
  schemas (SPEC 9.2).
- Anthropic Messages and OpenAI-compatible providers, with the SSE parsers tested against
  recorded fixtures (SPEC 6).
- The bundled model catalog with the reasoning variants a model declares (SPEC 9.7).
- `providers.toml` and `config.toml`, the configuration contracts (SPEC 5).
- Sessions as append-only JSONL, with model switches recorded (SPEC 11).
- The reusable picker overlay, with `/model` and `/provider` (SPEC 10.3).
- The agent loop with the four built-in tools, the permission matrix and the interactive ask
  (SPEC 9.1, 9.2, 9.4).
- The process registry: `bash` commands run in their own process group and show up in the
  sidebar.
- The llama.cpp supervisor: model discovery, port selection, the launch options, the health
  check, the properties and the shutdown of `llama serve` (SPEC 7).
- The launch options editor, with the implicit prefix shown read-only (SPEC 7.4).
- The log strip, which shows the last three lines of the local server (SPEC 10.3).
- The context accounting: the window of a model, the used tokens, the estimate and the turn
  statistics (SPEC 8).
- The METRICS block with the context bar, the speed, the think time and the vitals (SPEC 10.3).
- `/compact` and `/truncate`, and the model switch that would overflow the window is blocked
  (SPEC 8).
- The skills: the `SKILL.md` format, the catalog in the prompt, `/<skill> <prompt>` and the
  built-in `init` skill (SPEC 9.5).
- The external tool format and the toggles, with `/tools` and `/skills` as checklists (SPEC 9.3,
  9.6).
- The home widgets, with the built-in `git` and `usage` ones (SPEC 10.5).
- `install.sh`: the folders, the built-in files and the checks (SPEC 13).
- The rate limits: the headers of the providers and the `rpm` and `tpm` of the configuration
  (SPEC 6).
- The `fetch` tool, the first network tool (SPEC 9.3).
- The logo assets: the mark on transparency (cropped to its ink), the green variant and the
  editable design. The extraction handles transparent, scaled and anti-aliased assets.
- The config points at the llama.cpp binary and at its subcommand; the old key is migrated.
- The first run asks which model to use (SPEC 16.7).
- The capping of the tool output lands on a character boundary and wraps the long lines instead
  of cutting them; the first version could panic on a multi-byte character.

[Unreleased]: https://github.com/piskevalee-cpu/Seek/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/piskevalee-cpu/Seek/compare/v0.0.0...v0.7.0
