# SEEK

![SEEK](assets/logo.png)

A thin, local-first coding-agent harness for the terminal. SEEK wraps a model with a loop, a
small toolset and a TUI, and stays thin by design: a short system prompt, four default tools,
and nothing hidden.

- **Local is first-class.** SEEK owns the lifecycle of the llama.cpp server: it discovers models in
  `~/seek/models/`, starts `llama serve`, waits for health, reads the real context size and stops it
  again on exit.
- **Model-agnostic.** Switch between local and cloud models between turns without losing the
  conversation.
- **Filesystem is configuration.** Skills, tools, models and widgets live in `~/seek/` as plain
  folders and files.

## Status

The project is being built milestone by milestone from the [specification](docs/SPEC.md) (v0.1).

| Milestone | Scope | Status |
| --- | --- | --- |
| v0.1.0 | Workspace, CI, theme, logo, Home/Chat layout, prompt bar, spinner, footer, sidebar | in progress |
| v0.2.0 | Anthropic and OpenAI-compatible providers, reasoning variants, sessions | planned |
| v0.3.0 | Agent loop, the four tools, permissions, tool rows | planned |
| v0.4.0 | llama.cpp supervisor, local models, launch options | planned |
| v0.5.0 | Context accounting, metrics, model switching | planned |
| v0.6.0 | Skills, toggles, `/init` | planned |
| v0.7.0 | Widgets, `install.sh`, rate limits, `fetch` | planned |
| v1.0.0 | Contracts frozen | planned |

## Build

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -p seek
```

Rust 1.88 or newer (edition 2024) is required.

## Layout

```
crates/seek-core/   agent loop, canonical messages, tool trait, permissions, skills, sessions
crates/seek-llm/    providers, SSE normalization, model catalog, rate limiting
crates/seek-local/  llama.cpp supervisor: model scan, ports, spawn/stop, health, logs, tokenize
crates/seek-tui/    ratatui app: model/update/view, components, theme
crates/seek/        binary: wiring, CLI, doctor
builtin/            skills and widgets copied to ~/seek by install.sh
tools/              repository tooling (logo extraction)
docs/SPEC.md        the specification — the source of truth
```

## Contributing

Read [docs/SPEC.md](docs/SPEC.md) first. One task, one branch, one PR, branched from `develop`.
Run `cargo fmt && cargo clippy -- -D warnings && cargo test` before pushing.

## License

Undecided — ask the owner before adding one.
