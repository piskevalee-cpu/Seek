# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). One version covers the whole
workspace; each release ships a milestone (SPEC 15).

## [Unreleased]

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

[Unreleased]: https://github.com/piskevalee-cpu/Seek/compare/v0.0.0...HEAD
