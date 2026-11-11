# AGENTS.md

SEEK — a thin, terminal-first coding-agent harness (Rust workspace, ratatui TUI) that drives
local llama.cpp servers and cloud models.

## What this repo is

A Rust workspace built from the specification in `docs/SPEC.md` (spec v0.1). The spec is the
source of truth; this file is the OpenCode twin of `CLAUDE.md`.

```
crates/seek-core/   agent loop, canonical messages, tool trait, permissions, skills, sessions
crates/seek-llm/    providers (anthropic-messages, openai-chat), SSE, model catalog, rate limits
crates/seek-local/  llama.cpp supervisor: model scan, ports, spawn/stop, health, logs, tokenize
crates/seek-tui/    ratatui app: model/update/view, components, theme
crates/seek/        binary: wiring, CLI, doctor
builtin/            skills and widgets copied to ~/seek by install.sh
tools/              repository tooling (logo extraction)
assets/logo.png     the logo
reference/          read-only third-party checkouts (gitignored)
```

Upstream remote: `https://github.com/piskevalee-cpu/Seek.git` (the repo was renamed from
`Seek-`; see `docs/DECISIONS.md` D-004). The crate and binary are named `seek`.

**Milestones v0.1.0 to v0.6.0 are implemented**: workspace, CI, theme, logo, Home/Chat layout,
prompt bar, spinners, footer, sidebar shell, reasoning show/hide, the Anthropic and
OpenAI-compatible providers, the model catalog, the configuration contracts, sessions, the
pickers, the agent loop, the four tools, the permission matrix, the llama.cpp supervisor, the
context accounting with the METRICS block, the skills, the external tool format and the toggles.
Later milestones are listed in `docs/SPEC.md` §15 — **do not implement features from later
milestones.**

## Commands

```sh
cargo fmt --all                                    # format
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                             # all tests
cargo test -p seek-core --test prompt_budget       # the 1,500-token budget
cargo run -p seek                                  # the TUI
cargo run -p seek-tui --example screenshot         # render the UI as text, no terminal needed
cargo build --workspace --release                  # release binaries
```

Before every push: `cargo fmt && cargo clippy -- -D warnings && cargo test`.

This machine has 15 GB of RAM and **no swap**. A full parallel build of this dependency tree plus
a running local server can exhaust it and take the shell down with it. Build with `cargo build -j 2`
or `cargo test -j 2` when a server is running, and keep the context of local servers small.

## Spec precedence (spec §0)

- When sections conflict, **the UI ownership table (10.2) and the contracts list (2.4) win**.
- `[DECISION]` = a proposal the owner has not confirmed. Follow it unless told otherwise, and record
  any change in `docs/DECISIONS.md`.
- `[VERIFY]` = behavior of external software that changes between releases. Check current source or
  test the real binary before relying on it. `reference/llama.cpp` holds the current upstream server
  source.
- The repository's own decisions are in `docs/DECISIONS.md`. Read it before changing anything the
  spec left open.

## Hard constraints (breaking these breaks the spec)

- **Prompt budget ≤ 1,500 tokens** for the system prompt plus the default tool schemas, enforced by
  `crates/seek-core/tests/prompt_budget.rs` (§9.2). It currently sits at ~258 tokens, so there is
  room — but the test is the limit, not the current number. The reference point is pi: ~1,000
  tokens with exactly four tools (`reference/pi/.../core/system-prompt.ts`).
- **One home per fact (§10.2).** Never add a second display of the same information. The `usage`
  widget (lifetime tokens) and the METRICS block (current session) are different scopes, not a
  duplicate. A test in `crates/seek-tui/tests/ui.rs` checks this for the regions that exist.
- **Milestone order (§15).** Build v0.1 → v0.1 in order; do not pull later features forward.
- **Contracts (§2.4)**: the `~/seek/` layout, `tool.toml`, `widget.toml`, `providers.toml`,
  `config.toml`, the per-model sidecar `.toml`, the session JSONL format, CLI flags and slash
  commands. Every format carries `schema = 1`; changing one needs a migration and a changelog note.
  All contracts freeze at 1.0.0.
- **Branching (§2.2–2.3)**: one task, one branch, one PR, branched from `develop`; never commit
  directly to `main` or `develop`; squash-merge back into `develop`. Conventional Commits with
  scopes `tui core llm local tools skills widgets install docs`.
- **Keep `panic = "unwind"`** in the release profile — stopping the local llama.cpp server on exit
  relies on `Drop` running (§2.6).
- **Never invent reasoning variants (§9.7).** Offer only the levels the model's catalog entry
  declares; a model with no declared variants shows no reasoning chip.
- **Never overwrite user files** in `~/seek/`. `install.sh` is idempotent.
- **API keys never go in `config.toml` or `providers.toml`** — use an env var name or
  `state/auth.json` (mode 0600), and redact them everywhere they could be logged.
- The prompt prefix must stay **stable and append-only** so provider caches and llama.cpp
  `cache_prompt` reuse keep working (§9.2).
- **The logo bitmap is generated.** `crates/seek-tui/src/logo_bitmap.rs` comes from
  `tools/extract_logo.py`; regenerate it with
  `python3 tools/extract_logo.py --out crates/seek-tui/src/logo_bitmap.rs` after changing
  `assets/logo.png`. Never scale the PNG at runtime.

## Architecture notes that are not obvious from the filenames

- The TUI follows The Elm Architecture: Model / Message / Update / View, one `tokio::select!` loop
  over the crossterm `EventStream`, a 100 ms tick and mpsc receivers (`crates/seek-tui/src/run.rs`).
  `update` is pure — no I/O — and side effects are `Command`s sent to the agent task.
- `seek-core` stays UI-free and HTTP-free; `seek-tui` does no provider I/O. The provider boundary
  lives in `seek-llm`.
- Ignore key events that are not `KeyEventKind::Press`. Redraw only when the state changed or an
  animation is running.
- The logo is drawn from a 25x9 cell bitmap: full blocks (50x9 cells) when there is room, half
  blocks (25x5) on short terminals, and the word SEEK below 56 columns.
- The thinking spinner is a 3x3 cube grid lit in the order 1 2 3 6 9 8 7 4 5, three cubes at a
  time. The loading spinner is a row of five cubes. `reduce_motion` (also `--reduce-motion`)
  replaces both with static text.
- The provider interface (`Provider`, `ChatRequest`, `ProviderEvent`) lives in **seek-core**, not
  seek-llm, because the agent loop in seek-core uses it. seek-llm re-exports it and implements it
  for the two wire protocols.
- The agent loop is `seek_core::r#loop::Agent` (`loop` is a Rust keyword, hence the raw
  identifier). It owns the iteration limit, the tool timeout and the permission checks.
- Permissions travel through `PermissionRequests`: the agent sends a request with an id, the UI
  answers it with `Command::Permission { id, decision }`, and the binary hands the answer back.
  The App never talks to the agent directly.
- The permission matrix lives in `seek-core::permission`: reading is **always** allowed, writing
  and running are denied in `read` mode and asked in `ask` mode. A path outside the project root
  is always asked in `ask` mode.
- The context accounting is in `seek-core::context`: the window, the used tokens (the usage of
  the last response is the ground truth), the estimate and the turn statistics.
- The METRICS block has exactly one home: the bottom of the sidebar when it is open, the line
  under the prompt bar otherwise. A UI test checks that the counts appear exactly once.
- The vitals are sampled at 1 Hz by `seek-tui::vitals`. The GPU needs the `nvml` feature; without
  it the GPU and the video memory show a dash.
- The skills are in `seek-core::skill` (the `SKILL.md` format of the Agent Skills
  specification) and the toggles in `seek-core::toggles`. Only the name, the description and the
  path of a skill go into the prompt; the body is loaded when the skill runs.
- A skill runs with `/<name> <prompt>`; the body of the skill is inserted into that turn. The
  built-in `init` skill lives in `builtin/init/SKILL.md` and is loaded from there when it is not
  installed in `~/seek/skills`.
- The external tools are in `seek-core::external`: `tool.toml`, `schema.json` and a `run`
  executable that gets the arguments as JSON on stdin.
- The four tools are in `seek-core::tools`. `edit` refuses a text that is not unique; `read`
  caps its output and says how to fetch the rest; `bash` runs in its own process group and
  registers the process in the sidebar.
- The local server lives in `seek-local`: `model` scans `~/seek/models`, `port` derives the port
  from the model id, `options` strips the options SEEK owns and adds the implicit ones, and
  `supervisor` starts `llama serve`, waits for the health check, reads `/props` and stops the
  server as a whole process group.
- The supervisor is tested against a **real** llama.cpp server in
  `crates/seek-local/tests/supervisor.rs`. The test skips itself when there is no `llama` binary
  or no model in `~/LLMs`.
- **Memory:** a local server allocates the KV cache for its whole context. The default context of
  the test model is 131072 tokens, which is a **6.5 GB** allocation — enough to OOM the machine
  when a build runs at the same time. The tests start the server with `--ctx-size 4096`
  (about 200 MB). If you start a server by hand, pass a small context too, or add swap.

## Dependencies

Versions verified against crates.io on 2026-11-11. **Use `cargo add` for the latest compatible
version** rather than copying these numbers.

| Crate | Version | Used for | Note |
| --- | --- | --- | --- |
| ratatui | 0.30.2 | TUI | MSRV 1.88, edition 2024; split into ratatui-core/-widgets/-crossterm |
| crossterm | 0.29.0 | terminal + events | needs feature `event-stream` |
| tokio | 1.54.1 | async runtime | features `full` |
| futures | 0.3.34 | stream adapters | |
| tokio-util | 0.7.20 | `CancellationToken` | for Esc cancelling a turn (v0.3) |
| reqwest | 0.13.5 | HTTP + SSE | features `rustls`, `stream`, `json` (v0.2) |
| serde / serde_json | 1.0.229 / 1.0.154 | config, sessions, tool schemas | |
| toml | 1.2.1 | `config.toml`, `providers.toml`, `tool.toml`, `widget.toml`, sidecars | |
| serde_norway | 0.9.42 | YAML frontmatter of SKILL.md | `serde_yaml` is deprecated — do not use it |
| clap | 4.6.7 | CLI | |
| color-eyre | 0.6.5 | error reporting | |
| tracing / tracing-appender | 0.1.44 / 0.2.5 | logs | |
| sysinfo | 0.40.1 | CPU, RAM vitals | |
| nvml-wrapper | 0.13.0 | NVIDIA GPU / VRAM | behind a cargo feature |
| rustix or nix | 1.1.6 / 0.31.3 | process groups, signals | pick one, don't ship both |
| shell-words | 1.1.1 | parse `llama serve` launch options | |
| ignore | 0.4.34 | gitignore-aware walking | |
| similar | 3.2.1 | edit diffs | |
| unicode-width | 0.2.3 | layout width | |
| eventsource-stream | 0.2.3 | optional SSE parsing (or parse by hand) | |
| tiktoken-rs | 0.12 | prompt budget test only | dev-dependency of seek-core |
| anyhow / thiserror | 2.0 / 2.0 | errors | |

System requirements: `git` CLI (shell out — no libgit2), the `llama` binary on `PATH` (see below),
and cmake + a C/C++ compiler if building llama.cpp yourself.

## Toolchain traps (all verified, not from the spec)

- **Rust edition 2024, MSRV 1.88** — ratatui 0.30.2 declares `rust-version = "1.88.0"`.
- **ratatui 0.30 backend version is a feature.** `ratatui = "0.30"` alone does not pin a crossterm
  version for you; add `features = ["crossterm_0_29"]` (or `_0_28`) to match the crossterm you
  depend on. Default is crossterm 0.29. See `reference/ratatui/ratatui-crossterm/Cargo.toml`.
- **`tui-textarea` 0.7.0 requires `ratatui ^0.29` → incompatible with ratatui 0.30.** The prompt
  bar is hand-written in `crates/seek-tui/src/input.rs`; do not reintroduce that dependency.
- **ratatui's own `rustfmt.toml` uses nightly-only options** (`wrap_comments`, `group_imports`,
  `imports_granularity`). Don't copy it verbatim — keep SEEK's rustfmt stable-compatible.
- **Anthropic tool input arrives as `input_json_delta` partial strings**: accumulate per block
  index and parse at `content_block_stop`. OpenAI-compatible `tool_calls` deltas accumulate by
  index.
- `Layout::areas::<N>(area)` is the const-generic way to split an area in ratatui 0.30;
  `Layout::split` returns the new `Rects` type.

## Local llama.cpp server: verified behavior

**The binary is the unified `llama` CLI, and the server is a subcommand:**

```
llama serve    # HTTP API server   (spec §7.2 still says "llama-server" — see D-001)
llama cli      # interactive
llama download # fetch a GGUF (useful for populating ~/seek/models/)
llama update   # self-update
```

`llama serve -m <path> --host 127.0.0.1 --port <port>` accepts every flag the spec relies on:
`-m/--model`, `--host`, `--port`, `--parallel`, `--jinja`, `--reasoning-format`,
`--reasoning-effort`, `--reasoning-budget`, `--chat-template-kwargs`, `-c/--ctx-size`, `--api-key`.

A legacy `llama-server` binary also exists on this machine (`/usr/bin/llama-server`, 0.9.0) and
takes the same flags, but new code should target `llama serve`. How the config expresses "binary
path + subcommand" is an open owner decision (`docs/DECISIONS.md` D-001).

**Endpoints, verified at runtime** (llama 0.9.0-dev, build 12110, against a real GGUF):

- `GET /health` → **503 while the model loads**, 200 when ready (§7.3 step 4).
- `GET /props` → `default_generation_settings.n_ctx` is the real per-slot context, plus
  `total_slots`, `model_path`, `chat_template` (a ~9 KB string) and **`chat_template_caps`**.
- **`chat_template_caps` is the structured way to detect reasoning support** — do not string-grep
  the template. The MiniCPM5-2B test model reports `supports_reasoning_effort: false` and
  `reasoning_effort_levels: []`, yet its template text literally contains "enable_thinking". A grep
  would wrongly conclude effort levels exist (D-003).
- `POST /tokenize` accepts **only `content`**. Sending `{"messages": [...]}` returns
  `{"tokens": []}` — an empty array, not an error, so a wrong implementation silently reports
  0 tokens. Use `POST /apply-template` with `messages` first, then `POST /tokenize` with the
  resulting prompt as `content`.
- `GET /v1/models` returns **both** `models` and `data` keys; parse the standard `data` array.
- `POST /v1/chat/completions` → `usage` carries `prompt_tokens`, `completion_tokens` and
  `prompt_tokens_details.cached_tokens`. `stream_options: {include_usage: true}` is required to get
  the final usage chunk.
- `chat_template_kwargs.enable_thinking` must be a JSON **boolean**; a string returns HTTP 400
  `invalid type for "enable_thinking" (expected boolean, got string)`.
- Reasoning arrives as `delta.reasoning_content` **only** with `--reasoning-format deepseek`.

## Reference repos (in `reference/`)

Read these instead of guessing at external behavior. **Read for ideas — check the license before
copying code** (spec §6 says this explicitly for opencode).

| Path | Upstream | License | Read this for |
| --- | --- | --- | --- |
| `reference/opencode` | `anomalyco/opencode` (MIT) | MIT | `packages/core/src/variant.ts` (reasoning variants), `skill.ts`, `permission.ts`, `model.ts`, `models-dev.ts`, `usage-limits.ts`, `instructions/`; `packages/ai/src/providers/` (anthropic.ts, openai-compatible) |
| `reference/pi` | `earendil-works/pi` (MIT) | MIT | the thin-harness reference: `packages/coding-agent/src/core/system-prompt.ts` (tool list, prompt sections), `packages/agent/src/agent-loop.ts`, `packages/tui/` |
| `reference/llama.cpp` | `ggml-org/llama.cpp` (MIT) | MIT | `tools/server/README.md` (endpoints) and `tools/server/server-context.cpp` + `server-common.cpp` (exact request/response behavior of `/health`, `/props`, `/tokenize`, `/apply-template`, `chat_template_kwargs`, reasoning flags) |
| `reference/ratatui` | `ratatui-org/ratatui` (MIT) | MIT | 0.30 API and examples, `rustfmt.toml`, `ARCHITECTURE.md` |
| `reference/ratatui-templates` | `ratatui-org/templates` (MIT) | MIT | `simple-async/` — the ratatui 0.30 + crossterm 0.29 + tokio skeleton SEEK follows (`ratatui::init()` / `ratatui::restore()`, `DefaultTerminal`, `EventStream`) |
| `reference/agentskills` | `agentskills/agentskills` (Apache-2.0) | Apache-2.0 | the SKILL.md format spec (`docs/specification.mdx`), how to implement skill loading in a client (`docs/client-implementation/`), reference SDK (`skills-ref/`, Python) |
| `reference/models.dev` | `sst/models.dev` (MIT) | MIT | model catalog: `models/<provider>/<model>.toml` (schema 2: `name`, `[limit].context`, `[capabilities].reasoning`), `providers/` |

The clones are shallow and sparse. Refresh one with `git -C reference/<repo> pull`.

## Testing (spec §14)

- Unit: SSE parsers from recorded fixtures (v0.2), the permission matrix (v0.3), config parsing,
  port selection, context math.
- Agent loop: a mock provider with scripted streams (v0.3) — `seek-llm` already has
  `FakeProvider` for this.
- Supervisor: a fake `llama-server` script (v0.4).
- UI: ratatui `TestBackend` tests in `crates/seek-tui/tests/ui.rs` for Home, Chat, sidebar on/off,
  the overlays, the pickers, the permission selector and the ownership table.
- End to end: `crates/seek-tui/tests/agent_flow.rs` drives the real agent loop with a scripted
  provider and checks what lands in the transcript.
- `cargo run -p seek-tui --example screenshot` prints the rendered screens as text.
- Prompt budget: `crates/seek-core/tests/prompt_budget.rs`.

Run one package with `cargo test -p seek-core`, one test with `cargo test -p seek-core <name>`.

## Known gaps (deliberate, per the milestone order)

- Widgets and `install.sh` (SPEC 10.5, 13) are v0.7.
- The rate limits and the `fetch` tool are v0.7 as well.
- The reasoning level is kept in memory; it is stored per model in `state/toggles.toml` from v0.6.
- The reasoning level is kept in memory; it is stored per model in `state/toggles.toml` from v0.6.
- A background `bash` command leaves the process registry when the shell exits, even though the
  child keeps running.

## Open questions for the owner (spec §16) — don't invent answers

Repo license, browser tool (grok-code vs pi vs fetch-only), opencode free-model endpoint details,
whether `state/` may live next to the four user folders, the six extra slash commands, default
provider/model/reasoning level on first run, and a transparent-background logo. Ask; don't assume.

## This machine

- **Rust**: rustup with the `stable` toolchain (rustc/cargo **1.99.1**) — above the 1.88 MSRV.
- **cmake 4.4.4** is installed, so `reference/llama.cpp` can be built
  (`cmake -B build && cmake --build build --config Release -t llama-server`).
- **llama.cpp 0.9.0-dev** is installed at `~/.local/bin/llama` (unified CLI). A distro build of the
  legacy `llama-server` (0.9.0) is at `/usr/bin/llama-server`.
- **Local GGUF models** live in `~/LLMs/` (e.g. `MiniCPM5-2B-heretic-abliterated-Q4_K_M.gguf`,
  `LFM3-1.7B-Q5_K_M.gguf`). `~/seek/models/` is a separate, user-owned folder — do not move these.
- `reference/` is gitignored and local-only; a fresh clone will not have it.
