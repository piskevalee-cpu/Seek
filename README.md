<p align="center">
  <img src="assets/logo.png" alt="SEEK" width="420" />
</p>

<p align="center">
  <b>A thin, local-first coding-agent harness for the terminal.</b>
</p>

<p align="center">
  <img src="assets/logo-green.png" alt="SEEK on the brand green" width="300" />
</p>

SEEK wraps a model with a loop, a small toolset and a TUI, and stays thin by design: a short system
prompt, four default tools, and nothing hidden.

- **Local is first-class.** SEEK owns the lifecycle of the llama.cpp server: it discovers models in
  `~/seek/models/`, starts `llama serve`, waits for the health check, reads the real context size
  and stops it again on exit.
- **Model-agnostic.** Switch between local and cloud models between turns without losing the
  conversation.
- **Filesystem is configuration.** Skills, tools, models and widgets live in `~/seek/` as plain
  folders and files.
- **Nothing hidden.** Logs, process ids, context use and speed are always reachable.

## Try it

```sh
git clone https://github.com/piskevalee-cpu/Seek.git
cd Seek
cargo build --release

# a model in ~/seek/models, then:
./target/release/seek
```

The first run opens the model picker. Pick a local model and SEEK starts the server for it.

```sh
# what SEEK found
./target/release/seek --doctor
```

Rust 1.88 or newer (edition 2024) is needed to build. A `llama` binary is needed for local models;
`git` is needed for the widgets.

## Status

The project is being built milestone by milestone from the [specification](docs/SPEC.md) (v0.1).

| Milestone | Scope | Status |
| --- | --- | --- |
| v0.1.0 | Workspace, CI, theme, logo, Home/Chat layout, prompt bar, spinners, footer, sidebar | done |
| v0.2.0 | Anthropic and OpenAI-compatible providers, reasoning variants, `/provider`, sessions | done |
| v0.3.0 | Agent loop, the four tools, permissions, tool rows, processes list | done |
| v0.4.0 | llama.cpp supervisor, models folder, launch options, log strip | done |
| v0.5.0 | Context accounting, METRICS block, `/compact` | done |
| v0.6.0 | Skills, external tool format, toggles, `/init` | done |
| v0.7.0 | Home widgets, `install.sh`, rate limits, `fetch` tool | done |
| v0.8.0 – v1.0.0 | Hardening, contract review, the contracts frozen | in progress |

## Build

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p seek                          # the TUI
cargo run -p seek-tui --example screenshot # the screens as text, without a terminal
```

## Layout

```
crates/seek-core/   agent loop, canonical messages, tool trait, permissions, skills, sessions
crates/seek-llm/    providers, SSE normalization, model catalog, rate limits
crates/seek-local/  llama.cpp supervisor: model scan, ports, spawn/stop, health, logs, tokenize
crates/seek-tui/    ratatui app: model/update/view, components, theme
crates/seek/        binary: wiring, CLI, doctor
builtin/            skills and widgets copied to ~/seek by install.sh
tools/              the logo extraction
docs/SPEC.md        the specification — the source of truth
docs/TESTING.md     the manual test checklist: go through the TUI and check everything
```

## Testing

[`docs/TESTING.md`](docs/TESTING.md) is a checklist for going through the TUI by hand: every
screen, every key, every command, with what to expect. It is the fastest way to find out whether
a change broke something that only shows up when a person uses it.

## Contributing

Read [docs/SPEC.md](docs/SPEC.md) first. One task, one branch, one PR, branched from `develop`.
Run `cargo fmt && cargo clippy -- -D warnings && cargo test` before pushing.

## License

Undecided — ask the owner before adding one.
