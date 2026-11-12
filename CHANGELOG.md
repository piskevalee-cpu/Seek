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
- SEEK asks the local server for a context of 32768 tokens; the default of the model can be
  several gigabytes of key and value cache.
- The model that is chosen in a picker now reaches the agent; the command that carried the
  toggles swallowed it.
- The real context size of a local server reaches the metrics.
- A local server that cannot be started is an error, not a scripted answer.
- The conversation is written to a session and can be resumed: `/new`, `/resume`, and the
  sessions in the sidebar (SPEC 11).
- Esc stops the turn that is running (SPEC 9.1).
- The reasoning level is remembered per model (SPEC 9.7).
- The `fetch` tool can be turned on in `/tools`.
- The client side limits of the configuration (`rpm` and `tpm`) are honored (SPEC 6).
- The manual test checklist, `docs/TESTING.md`.
- The logo draws only its lit pixels, is centred both ways and has a sheen sweeping across it.
- The thinking and the loading animation are one line of dark cells with a wave of brightness.
- The prompt of the user is a band with the arrow in front of it; the answer of the model is plain
  text.
- The log strip is one line.
- The permission prompt is a selector: the chosen answer is highlighted and the keys are shown.
- The transcript window is right, and long messages are wrapped, so the streaming no longer
  disappears.
- The helpers and the network tools are off by default.
- The short forms of the launch options count, so `-c 8000` is the context size the user set.
- The servers are stopped when SEEK is closed by a signal.
- The turn no longer blocks the commands: the answer to a permission request and Esc arrive at
  once, and a turn that is cancelled gives up.
- The conversation goes on from turn to turn: the model sees what was said before.
- The bash command runs in a blocking task, so the interface keeps answering while it runs.
- The helpers have a timeout, a depth cap and a file cap: a search over a big tree comes back
  with an answer instead of hanging the turn.
- The row of a tool call is closed by its own id, so a late output does not leave it running.
- The system prompt says what to do: do the work now, claim nothing that a tool did not show,
  answer first, no preamble.
- The colours are the tokyonight of the grok-build pager, and the green of SEEK stays for the
  brand.
- The transcript has a rail per block and a blank line between the blocks.
- The prompt is a box with rounded corners, with the model name in the top border.
- The footer is one line: the mode, the model, the place and the metrics.
- The thinking line is eight cells and the streak wraps around.
- The keys arrive at once: the reader of the terminal was blocking the event loop, so Esc did
  nothing and the message after it was never taken.
- The cancel stops the turn that is running, and not the next one.
- The form that adds a provider, with the kind, the base URL, the key environment variable, the
  models, the context and the rate limits (SPEC 10.3).
- The providers are detected from the environment when there is no `providers.toml`, so a bare
  install works (SPEC 6).
- The keys can live in `state/auth.json`, readable by their owner only (SPEC 5).
- The models an endpoint knows are discovered and shown in the picker; the entries of
  `providers.toml` override them (SPEC 6).

[Unreleased]: https://github.com/piskevalee-cpu/Seek/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/piskevalee-cpu/Seek/compare/v0.0.0...v0.7.0
