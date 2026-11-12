# SEEK Harness: Technical Specification for Claude Code

Spec v0.1 · 11 Nov 2026 · Repository: https://github.com/piskevalee-cpu/Seek.git

## 0. How to use this document

- This is the source of truth for building SEEK. Commit it as `docs/SPEC.md` and keep a short `CLAUDE.md` at the repo root that points here (template in 2.7).
- Build milestone by milestone (section 15). One branch per task (section 2). Do not pull later-milestone features forward.
- **\[DECISION\]** marks a proposal the owner has not explicitly confirmed. Follow it unless told otherwise, and record any change in `docs/DECISIONS.md`.
- **\[VERIFY\]** marks behaviour of external software (llama.cpp, provider APIs) that changes between releases. Check current docs or test against the real binary before relying on it.
- Crate versions below were current when this was written. Use `cargo add` for the latest compatible versions instead of copying numbers.
- If two sections seem to conflict, the UI ownership table (10.2) and the contracts list (2.4) win.

## 1. Product and principles

SEEK is a terminal coding-agent harness for local (llama.cpp) and cloud models. It wraps a model with a loop, tools and a UI, and stays thin by design.

1. **Thin core.** Short system prompt, small default toolset, simple loop. Budget: system prompt plus default tool schemas at most 1,500 tokens, enforced by a test. The pi harness ships about 1,000 tokens with four tools; that is the reference point.
2. **Filesystem is configuration.** Everything user-extensible lives in `~/seek/`.
3. **Model-agnostic.** Switch model after any finished message without losing the conversation.
4. **Local is first-class.** SEEK owns the lifecycle of llama.cpp servers.
5. **Nothing hidden.** Logs, PIDs, context use and speed are always reachable.
6. **One home per piece of information** (10.2).

Non-goals for v1: sub-agents, MCP, plan mode, built-in todo list, IDE integration. **\[DECISION\]** Linux and macOS first; Windows is out of scope for v1.

## 2. Repository, branches and versioning

### 2.1 Repository

- Remote: `https://github.com/piskevalee-cpu/Seek.git`. The crate and binary are named `seek`.
- Before scaffolding, inspect what is already in the repo. Never force-push or rewrite history.
- Bootstrap: create `develop` from `main`; add `docs/SPEC.md`, `CLAUDE.md`, `.gitignore`, `rust-toolchain.toml`, `README.md` (with the logo) and `assets/logo.png`; protect `main` and `develop` on GitHub (PR required, CI green). The license is undecided: ask the owner before adding one.

### 2.2 Branch structure

| Branch | From | Merges into | Purpose |
| --- | --- | --- | --- |
| `main` | n/a | n/a | Always releasable. Only release and hotfix merges. Every release is tagged here. |
| `develop` | `main` | `release/*` | Integration branch. CI must be green. |
| `feature/<scope>-<desc>` | `develop` | `develop` (squash) | One task or milestone item, e.g. `feature/local-supervisor`. |
| `fix/<desc>` | `develop` | `develop` (squash) | Bug fix not yet released. |
| `release/<x.y.z>` | `develop` | `main`, then back to `develop` | Version bump, changelog and last fixes only. |
| `hotfix/<desc>` | `main` | `main` and `develop` | Urgent fix to a released version; bumps PATCH. |
| `docs/…`, `chore/…`, `refactor/…` | `develop` | `develop` | Non-feature work. |
| `experiment/<desc>` | `develop` | never merged | Spikes. Port good ideas into a `feature/` branch. |

### 2.3 Rules for Claude Code

- Never commit directly to `main` or `develop`. One task, one branch, one PR.
- Small commits that build and pass tests. Run `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` before every push.
- A PR description states what changed, which spec section it implements and how it was tested.
- Do not change a contract (2.4) without following the version rules below and noting it in the changelog.

### 2.4 Versioning

- SemVer with tags `vMAJOR.MINOR.PATCH`. One version for the whole workspace (`[workspace.package] version`). `seek --version` prints the version and short git SHA.
- Before 1.0: MINOR = a shipped milestone (section 15) or any breaking contract change; PATCH = fixes. Pre-releases: `v0.4.0-alpha.1`, `v0.4.0-rc.1`.
- From 1.0: breaking a contract means MAJOR.
- **Contracts** (the public API): the `~/seek/` layout, `tool.toml`, `widget.toml`, `providers.toml`, `config.toml`, the per-model sidecar `.toml`, the session JSONL format, CLI flags and slash commands. Every file format carries `schema = 1`, with a migration whenever it changes.
- All contracts freeze at 1.0.0.

### 2.5 Commits and changelog

- Conventional Commits: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`. Scopes: `tui`, `core`, `llm`, `local`, `tools`, `skills`, `widgets`, `install`, `docs`. Example: `feat(local): stop server before model switch`.
- `CHANGELOG.md` in Keep a Changelog format, updated on the release branch.

### 2.6 CI and releases

- GitHub Actions on every PR: fmt, clippy (deny warnings), tests, build on Linux and macOS, and the prompt-budget test (9.2).
- On tag `v*`: build release binaries (Linux x86\_64 and aarch64, macOS arm64 and x86\_64) and attach them to a GitHub Release with the changelog notes.
- Release profile: `lto = true`, `codegen-units = 1`, `strip = true`. Keep the default panic strategy (unwind): server cleanup relies on `Drop` running.

### 2.7 CLAUDE.md template

```
# SEEK
Spec: docs/SPEC.md (source of truth). Rust workspace, ratatui TUI.
- Branch per task from develop; never commit to main or develop.
- Before push: cargo fmt && cargo clippy -- -D warnings && cargo test
- Conventional Commits. Scopes: tui core llm local tools skills widgets install docs
- Keep system prompt + default tool schemas under 1,500 tokens (a test enforces it).
- UI: every piece of info has exactly one home (SPEC 10.2). Never add a second display.
- Do not implement features from later milestones.
```

## 3. Technology

Rust, edition 2024. The minimum Rust version follows the newest dependency (ratatui 0.30 needs 1.88 **\[VERIFY\]**).

- **UI:** `ratatui` 0.30.x, `crossterm` 0.29 with feature `event-stream`.
- **Async:** `tokio` (full), `futures`, `tokio-util` (CancellationToken).
- **HTTP and SSE:** `reqwest` (rustls, stream, json); parse SSE by hand or with `eventsource-stream`.
- **Data:** `serde`, `serde_json`, `toml`, a maintained YAML crate for SKILL.md frontmatter **\[VERIFY\]**.
- **CLI, errors, logs:** `clap`, `color-eyre`, `tracing`, `tracing-appender`.
- **System:** `sysinfo` (CPU, RAM), `nvml-wrapper` (NVIDIA GPU, behind a cargo feature), `rustix` or `nix` (process groups, signals).
- **Text:** `shell-words` (launch options), `ignore` (gitignore-aware walking), `similar` (diffs), `unicode-width`.
- **Git:** shell out to the `git` CLI; no libgit2 (keeps it thin).
- **Input widget:** write a small multi-line input; check `tui-textarea` compatibility with the chosen ratatui version first.

## 4. Architecture

### 4.1 Workspace

```
Seek-/
├── Cargo.toml                workspace
├── crates/
│   ├── seek-core/    agent loop, canonical messages, tool trait, permissions,
│   │                 skills, sessions, context accounting. No UI, no HTTP.
│   ├── seek-llm/     providers (Anthropic Messages, OpenAI-compatible), SSE
│   │                 normalisation, model catalog, rate limiting
│   ├── seek-local/   llama.cpp supervisor: model scan, ports, spawn/stop,
│   │                 health, logs, tokenize
│   ├── seek-tui/     ratatui app: model/update/view, components, theme
│   └── seek/         binary: wiring, CLI, doctor
├── assets/logo.png
├── builtin/          skills and widgets copied to ~/seek by install.sh
└── docs/  install.sh  .github/workflows/
```

### 4.2 Data flow

The UI follows The Elm Architecture: Model, Message, Update, View. One `tokio::select!` loop reads the keys, a 100 ms tick and mpsc receivers. The keys are read by a thread of their own and reach the loop through a channel: the reader of the terminal blocks, and a blocked reader inside the loop freezes the interface. Messages: `Key`, `Resize`, `Tick`, `Agent(AgentEvent)`, `Local(LocalEvent)`, `Vitals(Sample)`, `Widget(WidgetOutput)`. Update is pure (no I/O); side effects are commands sent to core and local tasks. Redraw only when state changed or an animation is active. Ignore key events that are not `KeyEventKind::Press`. Restore the terminal on panic (`ratatui::init()` installs a hook).

### 4.3 Canonical message model

`Message { role, blocks }` where blocks are `Text`, `Thinking { provider, signature, text }`, `ToolUse { id, name, input }` and `ToolResult { id, content, is_error }`. Each provider converts to and from its wire format. On a switch to a different provider, drop `Thinking` blocks (signatures are provider-specific) and keep everything else. History is append-only so prompt caches stay valid.

### 4.4 AgentEvent

`TurnStarted`, `ThinkingDelta`, `TextDelta`, `ToolCallStarted`, `ToolCallReady`, `PermissionRequest { reply: oneshot }`, `ToolOutput`, `Usage { input, output }`, `TurnFinished { reason }`, `Error`.

## 5. The \~/seek directory

Created by `install.sh`; never overwritten once it exists.

```
~/seek/
├── skills/         one folder per skill, each with SKILL.md
├── tools/          one folder per external tool (tool.toml + executable)
├── models/         .gguf files (+ optional <name>.toml sidecar with launch options)
├── widgets/        one folder per home-screen widget (widget.toml + executable)
├── config.toml     theme, default mode, default model, llama-server path
├── providers.toml  cloud providers and their models
└── state/          created at runtime: sessions/, logs/, auth.json (0600),
                    toggles.toml, pids
```

**\[DECISION\]** The owner asked for skills, tools, models and widgets. Config and state files sit beside them; `state/` holds everything the app writes by itself so the four user folders stay clean. API keys never go in `config.toml` or `providers.toml`: use an env var name or `state/auth.json`, and redact keys everywhere they could be logged.

## 6. Providers and models

Modelled on opencode: a provider is a base URL, an auth method and a model map.

```toml
# ~/seek/providers.toml
schema = 1
[provider.anthropic]
name = 'Anthropic'
kind = 'anthropic-messages'
base_url = 'https://api.anthropic.com'
env = ['ANTHROPIC_API_KEY']
[provider.anthropic.models.'<model-id>']
name = 'Display name'
context = 200000
rpm = 50        # optional client-side limits
tpm = 100000
[provider.openrouter]
kind = 'openai-chat'
base_url = 'https://openrouter.ai/api/v1'
env = ['OPENROUTER_API_KEY']
```

- Two wire protocols cover nearly everything: `anthropic-messages` and `openai-chat` (any OpenAI-compatible server, including llama.cpp).
- **Anthropic:** POST `/v1/messages` with headers `x-api-key` and `anthropic-version: 2023-06-01`, `stream: true`. Events: `message_start` (input tokens), `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta` (output tokens and stop reason). Tool input arrives as `input_json_delta` partial strings: accumulate per block index and parse at `content_block_stop`. Use `cache_control` on the stable prefix.
- **OpenAI-compatible:** POST `/chat/completions` with `stream: true` and `stream_options: { include_usage: true }` \[VERIFY per server\]; accumulate `tool_calls` deltas by index.
- **Discovery:** for `openai-chat` providers, GET `/models` to list models; explicit entries in `providers.toml` override discovered ones (the rule opencode uses).
- **Auto-detect** providers from common env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`) so a bare install works. A bundled catalog (id, name, context) ships with the binary and `/provider` can edit entries. Refreshing it from a public catalog such as models.dev (which opencode uses) is a later feature.
- **Rate limits:** honor 429 and `retry-after`, read rate-limit response headers when present, apply optional `rpm` and `tpm` from config. Show state in the Metrics block (10.3), only when a provider reports it.
- **opencode free models:** treat as an ordinary `openai-chat` provider entry once the owner supplies endpoint details. **\[DECISION\]** Study opencode's provider code for ideas; copy code only after checking its license.

## 7. Local models (llama.cpp)

### 7.1 Discovery

On startup and whenever `/model` opens, scan `~/seek/models/` for `*.gguf`. Each file becomes model id `local/<file-stem>`, flagged `LOCAL`. For split GGUFs (`-00001-of-0000N`) list only the first shard. An optional sidecar `<file-stem>.toml` stores launch options and the assigned port.

### 7.2 Launch

SEEK runs `llama-server -m <path> --host 127.0.0.1 --port <port>` followed by the user's launch options. The user never types `-m`, `--host` or `--port`; if they do, strip them and warn. Also implicit unless overridden: --reasoning-format deepseek (reasoning arrives separately as reasoning\_content), `--parallel 1` (SEEK is single-conversation, and the KV cache is split across slots, so the default auto slot count can shrink usable context **\[VERIFY\]**) and `--jinja` for native tool-call chat templates **\[VERIFY\]**. Binary: `llama_server_path` in `config.toml`, else `llama-server` on PATH, else a clear error in the log strip. Port: derive from the model id (hash into 8100-8899), confirm with a bind test, persist in the sidecar. One port per model.

### 7.3 Lifecycle (one active server)

Every model switch runs this sequence and logs each step to the log strip:

1. Wait until the current turn has finished (switching is only allowed between turns).
2. Stop the running server: SIGTERM to its process group, wait up to 5 s, then SIGKILL.
3. Start the new server in its own process group; capture stdout and stderr into a ring buffer and `state/logs/`.
4. Poll `GET /health` until ready (it returns 503 while loading). Time out with a clear error.
5. Read `GET /props` for the real context size (`n_ctx`; field location varies by build **\[VERIFY\]**). That value, not the `-c` flag, is the source of truth.
6. Update the Metrics block with the new total and re-count the history (section 8). Cleanup: stop all servers on exit, SIGINT, SIGTERM and panic. Write child PIDs to `state/pids` and kill matching orphans on next start. Cloud-to-cloud switches skip steps 2 to 5.

### 7.4 Launch options editor

In `/model`, pressing right-arrow on a local model opens its options as free text, saved to the sidecar; left-arrow or Esc returns. Show the implicit part as a read-only prefix: `llama-server -m <file> --host 127.0.0.1 --port <port>`. Changes apply at next start; if the model is running, offer a restart after the current turn.

## 8. Context accounting and metrics

- **Total window:** local = `n_ctx` from `/props`; cloud = catalog or provider value, overridable per model.
- **Used tokens (ground truth):** usage reported by the last response (Anthropic `message_start` and `message_delta`; OpenAI `usage`; llama.cpp usage and timings **\[VERIFY\]**). Used = last prompt tokens + last output tokens.
- **Estimates and re-count after a model switch:** local = POST `/tokenize` (newer builds accept `messages` and apply the chat template, older ones accept `content` only; `/apply-template` then `/tokenize` is the fallback **\[VERIFY\]**). Cloud = Anthropic's count-tokens endpoint is optional; otherwise estimate with a bytes/3.5 heuristic. Prefix estimates with `~` until a real usage arrives.
- **Switch overflow:** if the history exceeds the new window, block the switch and offer: compact, truncate the oldest turns, or cancel. `/compact` summarizes older turns into one message.
- **tps:** output tokens (reasoning tokens included) divided by generation time for the turn (prefer server-reported timings for local). **Think time:** wall time from the first reasoning delta to the first text or tool delta, summed per turn; hidden if the model emitted no reasoning.
- **Vitals:** CPU %, RAM and, where available, GPU % and VRAM, sampled at 1 Hz. Missing sensors show a dash.

## 9. Agent loop, tools, permissions, skills

### 9.1 Loop

Stream a completion, collect tool calls, and for each: permission check, execute, append a `ToolResult`; repeat until a response has no tool calls. Guards: max iterations per turn (default 100), a cancellation token on Esc, per-tool timeouts, and errors returned to the model as `is_error` results instead of crashes. **Queued messages:** the user can type while the agent works. Messages go to a FIFO queue, show in the transcript as pending, and are delivered as one user message when the turn finishes. **\[DECISION\]** End-of-turn delivery only in v1. **Agent processes:** the `bash` tool runs each command in its own process group and registers it (PID, command, start time). Foreground commands leave the registry on exit; detached or background ones stay until they end or the user kills them.

### 9.2 Token-efficient tool calling

- Default tools: `read`, `write`, `edit`, `bash`. Read-only helpers `ls`, `grep`, `find` ship but are off by default (pi's approach).
- The file tools follow the grok-build pager (Apache-2.0): `read` numbers the lines (the first line and every tenth carry an anchor `1→`, `11→`), `write` says whether it created or overwrote, `edit` replaces an exact text (unique, or every occurrence with `replace_all`), says the line the change landed on, and returns the difference. An empty text to replace creates the file.
- Terse descriptions, no examples inside schemas, and only enabled tools are sent.
- `edit` is an exact-match replace with a unique-match check, never a whole-file rewrite; `read` takes offset and limit.
- Cap every tool output (default 200 lines or 16 KB, head and tail) with a marker telling the model how to fetch more.
- Skills enter the prompt as a catalog only (9.5).
- Keep the prompt prefix stable and append-only so provider caches and llama.cpp prompt-cache reuse (`cache_prompt` **\[VERIFY\]**) keep working.
- A CI test counts tokens of system prompt plus default schemas and fails above 1,500.

### 9.3 External tool format **\[DECISION\]**

```
~/seek/tools/<name>/
├── tool.toml     name, description, risk, timeout_s, schema = 'schema.json'
├── schema.json   JSON Schema of the arguments
└── run           any executable: args as JSON on stdin, result on stdout,
                  non-zero exit = error
```

`risk` is one of `read`, `write`, `exec`, `network`. Built-in tools implement the same Rust trait and appear in `/tools` next to external ones. A `fetch` tool (HTTP GET to text) is the first network tool. A `web_search` tool follows it: DuckDuckGo's HTML search, which needs no key, the way the oh-my-pi harness does it (MIT). A browser tool is a later milestone; the owner chooses between the grok-code and pi approaches.

### 9.4 Permissions

| Mode | read | write | exec | network |
| --- | --- | --- | --- | --- |
| `read` | allow | deny | deny | deny |
| `ask` | allow | ask | ask | ask |
| `yolo` | allow | allow | allow | allow |

- Cycle with Shift+Tab or set with `/mode`. Shift+Tab works whatever the sidebar does. The mode is displayed in the footer chip only.
- **The mode is read at every tool call.** Changing it while the agent works applies to the turn that runs now: the next call of that turn follows the new mode.
- **Interactive ask:** the prompt bar turns into a selector with three choices: **Yes**, **No**, **Tell SEEK what to do**. The third opens a text input in the same bar; the text goes back to the model as the tool result (`denied by user: <text>`) and the agent continues from it.
- Paths outside the project root are always asked in `ask` mode.

### 9.5 Skills

The name of the user is asked for on the first run, in the prompt bar, and `/settings` asks it
again. It is kept in `state/user.toml` (`schema = 1`) and goes into the prompt: the model
addresses the user by it.

Follow the Agent Skills format. A skill is a folder with `SKILL.md`: YAML frontmatter (`name` up to 64 chars, lowercase letters, numbers and hyphens, matching the folder; `description` up to 1,024 chars saying what it does and when to use it; optional `license`, `compatibility`, `metadata`, `allowed-tools`) followed by Markdown instructions; optional `scripts/`, `references/`, `assets/`. Progressive disclosure: only `name`, `description` and path of enabled skills go in the prompt (about 100 tokens each); the model loads the body with `read`. The user runs a skill with `/<skill-name> <prompt>`, which inserts the body into that turn. `/skills` toggles each skill. `/init` ships as a built-in skill (like opencode's) that inspects the repo and writes `AGENTS.md`; SEEK loads `AGENTS.md` from the project root into the prompt when present.

### 9.6 Toggles

`state/toggles.toml` stores enabled or disabled for tools and skills. Defaults: the four built-in tools on, helpers off, all installed skills on.

### 9.7 Reasoning: thinking toggle and effort

Two independent states, so nothing overlaps: the **reasoning level** (off, or an effort level) changes the request and therefore cost, speed and quality; **reasoning visibility** only changes what the transcript shows.

**Variants (modelled on opencode).** A variant is a named fragment of request-body options merged into the request. The level the user picks is the variant name, and `off` is itself a variant. Each model lists its variants in cycle order. Defaults ship with the catalog per provider kind; the user can override, add or disable them in `providers.toml` (`disabled = true` removes one, as in opencode).

```toml
[provider.anthropic.models.'<model-id>']
variant_default = 'medium'
[provider.anthropic.models.'<model-id>'.variants.off]
# empty: send no thinking options
[provider.anthropic.models.'<model-id>'.variants.high]
thinking = { type = 'enabled', budget_tokens = 16000 }
[provider.openai.models.'<model-id>'.variants.medium]
reasoning_effort = 'medium'
```

**Never invent variants.** Offer only the levels a model's catalog entry declares. opencode once generated generic budget-token variants for a model with its own native effort setting and had to fix it. A model with no variants shows no reasoning chip, and `/think` and Ctrl+T print one line in the log strip saying reasoning is not configurable for it.

**What a variant contains, per provider kind**

| Kind | Off | On, with effort |
| --- | --- | --- |
| `anthropic-messages` | no `thinking` field | `thinking: { type: 'enabled', budget_tokens: N }`. Starting budgets: low 4,000, medium 10,000, high 16,000, max 32,000 **\[VERIFY\]**. N is at least 1,024 and below `max_tokens`. Newer models may declare a native effort or adaptive style instead of a budget; the catalog entry decides. Thinking blocks go back unmodified inside the tool loop |
| `openai-chat` (OpenAI) | `reasoning_effort: 'none'` where supported | `reasoning_effort`: minimal, low, medium, high, xhigh. Not every model accepts every level |
| `openai-chat` (other hosts) | defined per model in `providers.toml` | the host's own fragment, copied as a raw object |
| local llama.cpp | `chat_template_kwargs: { enable_thinking: false }` | `chat_template_kwargs: { enable_thinking: true, reasoning_effort: '<level>' }` |

**Local models (llama.cpp)**

- **Per request, no restart.** SEEK puts `chat_template_kwargs` in each request body, so changing the level never restarts the server.
- **Detect what the model supports.** Once the server is ready, read the chat template from `GET /props` and look for `enable_thinking` and `reasoning_effort` **\[VERIFY\]**. Neither found: no variants. Only `enable_thinking`: variants off and on. Both: off plus the levels the template accepts. llama.cpp's own vocabulary is minimal, low, medium, high, xhigh, max; send only levels the template defines, because an undefined level can raise a template error.
- **Server-wide flags are a power-user path.** `--reasoning on|off`, `--reasoning-effort <level>`, `--reasoning-budget <N>` (token cap on the reasoning segment; 0 ends it at once, -1 means unlimited) and `--chat-template-kwargs` apply to the whole server and take effect only at restart **\[VERIFY\]**. If the launch options contain any of them, SEEK treats that model's reasoning as server-controlled: the chip reads `think: fixed` and `/think` and Ctrl+T do nothing for it.
- **Parsing.** With `--reasoning-format deepseek` (set implicitly, see 7.2) reasoning arrives in `reasoning_content`; parse those deltas as `Thinking`. Fallback: if reasoning shows up inline as `<think>...</think>` inside content, split it in the stream parser.

**Using it**

- `/think` with no argument toggles between `off` and the last non-off level (the first level if none yet). `/think <level>` sets a level. Ctrl+T cycles the model's levels and wraps around (the same key opencode uses for its variant cycle).
- A change applies from the next request; during a running turn it waits for the next turn. The level is stored per model in `state/toggles.toml`; switching to a model restores its stored level, else its `variant_default`.
- `/thoughts` and Alt+T show or hide reasoning text, stored globally as `show_reasoning` in `config.toml` (default on). Hidden reasoning still streams, still counts toward think time and tokens and is still saved in the session; only the transcript changes, to one collapsed row. **\[DECISION\]** These command and key names are proposals.

**Guards**

- `max_tokens` must exceed the reasoning budget plus room for the answer (default 4,096). Raise it automatically instead of letting the request fail.
- If a turn ends with reasoning but no text and no tool call, the budget went to thinking. Show an error row that says so and suggests a lower level or a larger limit.
- Sampling settings that thinking models need (temperature, top-p) belong in the model's launch options or variant, not in this feature.
- Tests: variant merge and override, the think-tag splitter, request-body fixtures per provider kind, the empty-answer guard, and level restore after a model switch.

## 10. UI specification

### 10.1 Screens and regions

Two states, one layout. Home (no messages yet) and Chat share the same regions; Home swaps the transcript for the logo and widgets.

```
SIDEBAR OFF                              SIDEBAR ON (Ctrl+B)
+------------------------------------+   +-----------+------------------------+
| TRANSCRIPT                         |   | SESSIONS  | TRANSCRIPT             |
| (Home: logo + widgets)             |   |           | (Home: logo + widgets) |
|                                    |   | PROCESSES |                        |
|                                    |   |           |                        |
+------------------------------------+   | METRICS   +------------------------+
| LOG STRIP (0-3 lines)              |   | ctx bar   | LOG STRIP              |
| PROMPT BAR                         |   | tps, time | PROMPT BAR             |
| METRICS (one line)                 |   | vitals    | FOOTER                 |
| FOOTER: mode | model | cwd, branch |   +-----------+------------------------+
+------------------------------------+
```

Minimum size 80x24; below that show one 'terminal too small' message. Sidebar width 30 columns; it hides automatically below 100 columns.

### 10.2 Ownership table (each fact has exactly one home)

| Information | Only home | Rule |
| --- | --- | --- |
| Logo | Home transcript area | Never shown in Chat |
| Widgets | Home, under the logo | Never in Chat |
| Past and resumable sessions | Sidebar, Sessions | `/resume` opens the sidebar focused here |
| PIDs (agent-started and local servers) | Sidebar, Processes | Tagged `agent` or `server` |
| Context bar, tokens used/total, tps, think time, rate limits, vitals | METRICS block | Sidebar bottom when open, else the line under the prompt bar. Never both |
| Permission mode | Footer, left chip |  |
| Active model, provider, LOCAL flag | Footer, middle |  |
| Working directory and git branch | Footer, right |  |
| Server and model lifecycle events | Log strip | Process-level events only |
| Reasoning text | Transcript, thinking style |  |
| Tool calls and results | Transcript, tool rows | Collapsed by default |
| Queued messages | Transcript tail, pending style |  |
| Turn-ending errors | Transcript, error row |  |
| Permission prompt | Prompt bar (it changes mode) | No popup |
| Thinking and loading animation | Transcript tail | One spinner at a time |
| Provider setup | `/provider` picker |  |
| Launch options | `/model` picker, right-arrow |  |
| Tool and skill toggles | `/tools` and `/skills` pickers | The only place their state is shown |
| Key reference | `/help` overlay | No always-on hint bar |
| Reasoning level (off, low, medium, ...) | Footer, chip next to the model name | Changed with /think and Ctrl+T; hidden when the model has no levels |
| Reasoning text visibility | Transcript (full text or one collapsed row) | Changed with /thoughts and Alt+T; no chip |
| Session metrics | Footer line, or the bottom of the sidebar when it is open | Never both |

Lifetime usage across sessions is the `usage` widget on Home; METRICS is the current session. They are different scopes, not a duplicate.

### 10.3 Components

- **Transcript.** Every block has a one-column rail on the left, coloured by the kind of the block (blue for the user, magenta for the model, grey for the reasoning and the tools), and a blank line sits between the blocks. The prompt of the user is a band across the width with the arrow `❯` in front of it. The answer of the model is plain text, without a band and without a border. Reasoning is plain dim text behind its rail; when hidden (9.7) it collapses to one row with the same rail and the word reasoning. Tool rows are one line (`bash  cargo test  ok 1.2s`); Ctrl+O expands the selected row. Pending (queued) messages render dimmed with a `queued` tag. Auto-scroll sticks to the bottom unless the user scrolled up.
- **Spinner, thinking.** A line of eight cells that are dark, with a streak of brightness going through them, one step per tick (about 110 ms). The streak travels along the line and wraps around: a new one starts at the left when the old one reaches the right. The cells are full blocks and the dark ones are the background of the terminal. The line sits in the column of the message text, behind the rail, with a blank line between it and the message.
- **Spinner, loading** (model start, health wait): the same line. A `reduce_motion` option replaces both with static text.
- **Context bar.** Ten cubes, each 10% of the window, filling left to right. The cube currently filling uses partial blocks so it grows gradually. Right after it: used / total tokens (for example `12.4k / 32k`) and the percentage. Gray and green normally, amber from 80%, red from 95%. Prefix `~` while the figure is an estimate.
- **Metrics block.** Rows in this order: context bar with counts; tps and think time; rate limit (only if reported); CPU, RAM, GPU, VRAM. In the sidebar form each row has its own line; in the footer it drops elements from the right when the line is narrow.
- **Log strip.** The newest line of the server and model events, dim. It hides after 10 s of silence and reappears on new events.
- **Prompt bar.** A box with rounded corners: the name of the model in the top border, the keys and the status in the bottom one.
- **Scrolling.** The wheel scrolls the transcript, three lines at a time, and PageUp/PageDown scroll a page. The window is anchored to the content and not to the bottom, so the message that is being read stays where it is while the model writes. The place in the transcript is shown in the corner while the user is not at the bottom. The area of the input holds the text and nothing else, so the cursor is the only thing that moves in it, and the box keeps its shape while the agent works.
- **Completion.** Typing `/` opens a dropdown of the slash commands and the skills: a column of the commands, a column of the descriptions, and the picked one highlighted. The arrows move the selection, Tab takes it, Enter takes it and runs it, Esc closes the list. Multi-line input. Enter sends; Shift+Enter or Alt+Enter inserts a newline; Up and Down walk history on a single-line draft. Typing `/` opens one inline suggestion list above the bar, holding slash commands and skills together. Sending while the agent works queues the message. The bar has three modes: normal, permission-ask selector, tell-SEEK input.
- **Footer.** One line: a mode chip (`READ`, `ASK` or `YOLO`, colored per mode), the model name, then the reasoning chip (for example think: high; hidden when the model has no levels), then the working directory (shortened: the home is a `~`) and the git branch, and the metrics of the session. The metrics are in the footer when the sidebar is closed and at the bottom of the sidebar when it is open — never both (10.2). The `LOCAL` tag is only in the model picker: the chat names the model, and that is enough.
- **Sidebar.** Sections in order: Sessions (title and relative time, current project first, Enter resumes), Processes (PID, tag, command, uptime; `x` kills the selected one), then Metrics pinned to the bottom. Tab moves focus into the sidebar when it is open.
- **Pickers.** One reusable centered overlay with a filterable list. `/model`: grouped by provider, local models flagged `LOCAL`, right-arrow opens the launch-options editor, left-arrow returns. `/tools` and `/skills`: checklists where Space toggles and changes apply and persist immediately. `/provider`: a list plus an add form (kind, base URL, key env var or key, models, context, rate limits).

### 10.4 Commands and keys

| Command | Action |
| --- | --- |
| `/model` | Pick a model (between turns only) |
| `/provider` | Add or edit providers |
| `/tools`, `/skills` | Toggle tools, skills |
| `/mode` | Set permission mode |
| `/new` | Start a new session |
| `/resume` | Open the sidebar on Sessions |
| `/compact` | Summarize older context |
| `/init` | Built-in skill: write AGENTS.md |
| `/help`, `/quit` | Help overlay, exit |
| /think \[level\] | Toggle reasoning off and on, or set a level |
| /thoughts | Show or hide reasoning text |

**\[DECISION\]** `/mode`, `/new`, `/resume`, `/compact`, `/help` and `/quit` go beyond the owner's list; they are the minimum needed for the rest of the spec to work.

| Key | Action |
| --- | --- |
| Enter / Shift+Enter | Send / newline |
| Esc | Interrupt the turn; close an overlay |
| Shift+Tab | Cycle mode: read, ask, yolo |
| Ctrl+B | Toggle sidebar |
| Ctrl+O | Expand or collapse a tool row |
| Ctrl+C | Clear input; twice quits |
| PgUp / PgDn | Scroll transcript |
| Ctrl+T | Cycle the reasoning level |
| Alt+T | Show or hide reasoning text |

### 10.5 Home widgets

Home shows the logo centered, widgets in a responsive grid (1 to 3 columns) beneath it, then the prompt bar. A widget is a folder `~/seek/widgets/<name>/` with `widget.toml` (name, title, timeout) and an executable `run` that prints up to 8 lines of 40 columns of plain text. SEEK runs widgets when Home is shown, with a 500 ms timeout; a failing widget shows its title and a dash. Built-in widgets: `git` (recent commits of the working directory, via the `git` CLI) and `usage` (lifetime tokens used). Widgets disappear once the first message is sent.

### 10.6 Theme

Tokens: `bg`, `cube_dark`, `cube_light`, `accent`, `dim`, `warn`, `error`. `cube_light` and `accent` come from the logo (section 12). Use truecolor with a 256-color fallback, and never rely on color alone (the mode chip and the LOCAL tag are text).

## 11. Sessions

Append-only JSONL per session in `state/sessions/<id>.jsonl`: a header (id, cwd, created, title), then one line per message and per model switch. The title is the first user message, truncated. Resuming replays the file into the canonical model and re-counts context for the active model. Model switches are recorded as events, so a resumed session shows which model said what.

## 12. Logo and branding

Asset: `assets/logo.png`. The supplied version has a green background for contrast; also keep a transparent-background version for terminal use. Analysis of the supplied image (about 925 x 339 px, pixel unit about 37 px, so a 25 x 9 grid):

- Two colors: light gray (about #B5B5B5) on green (about #22B14C). **Sample the exact values from the PNG with a script and commit them as theme tokens**; the numbers here are estimates.
- The letters S, E, E, K use a 3 x 5 pixel font with one-pixel gaps and are 5 pixels tall.
- The symbol is a 3 x 3 square with a one-pixel arm in each of the four directions (a plus-shaped star in a 5 x 5 box), plus four detached single-pixel sparkles around it. It is exactly as tall as the letters, so they share a baseline.
- The 3 x 3 core echoes the 3 x 3 thinking spinner, which gives the brand a consistent visual family.
- Terminal rendering: extract the grid from the PNG once with a script, commit it as a constant bitmap in `seek-tui`, and draw each pixel as two columns of full blocks (50 x 9 cells), or with half-blocks (25 x 5) on short terminals. Below 56 columns show the word SEEK as plain text. Never scale the PNG at runtime.
- Only the lit pixels are drawn: the background of the logo is the background of the terminal, not a block of colour. A sheen sweeps across the mark, from the bottom left to the top right, and the logo rests with `reduce_motion`.

## 13. Install script

`install.sh` must be idempotent and never overwrite user files. It: (1) creates `~/seek/{skills,tools,models,widgets}`; (2) copies the built-in `init` skill and the `git` and `usage` widgets only if absent; (3) installs the `seek` binary to `~/.local/bin`, downloading the GitHub Release for the platform or running `cargo install --path crates/seek` with `--from-source`; (4) checks for `llama-server` and `git` and prints how to fix what is missing; (5) prints the next step. An optional `seek doctor` command repeats these checks.

## 14. Testing

- **Unit:** SSE parsers from recorded fixtures (Anthropic and OpenAI style), the permission matrix, config parsing, port selection, context math.
- **Agent loop:** a mock provider with scripted streams (tool call, error, cancel, queued messages).
- **Supervisor:** a fake `llama-server` script (slow health, crash, orphan cleanup).
- **UI:** ratatui `TestBackend` snapshot tests for Home, Chat, sidebar on and off, each picker and the permission ask. One test asserts that every row of the ownership table (10.2) renders in exactly one region.
- **Prompt budget:** the token-count test from 9.2.

## 15. Milestones (each ships as a MINOR version)

| Version | Branch prefix | Scope |
| --- | --- | --- |
| v0.1.0 | `feature/tui-*` | Workspace, CI, theme, logo, Home, Chat layout driven by a fake agent, prompt bar, spinner, footer, sidebar shell, reasoning show/hide |
| v0.2.0 | `feature/llm-*` | Anthropic Messages and OpenAI-compatible providers, streaming, reasoning variants and effort cycling, `/provider`, cloud `/model`, session persistence |
| v0.3.0 | `feature/core-*` | Agent loop, the four tools, permissions with interactive ask, tool rows, queued messages, Processes list |
| v0.4.0 | `feature/local-*` | llama.cpp supervisor, models folder, LOCAL models, launch-options editor, log strip, local reasoning detection |
| v0.5.0 | `feature/context-*` | Context accounting, Metrics block with vitals, model switch between turns with re-count, `/compact` |
| v0.6.0 | `feature/skills-*` | Skills, `/skills`, `/tools`, external tool format, toggles, `/init` |
| v0.7.0 | `feature/widgets-*` | Widgets, `install.sh`, rate limits, `fetch` tool, browser-tool spike |
| v0.8.0 to v0.9.0 | `fix/*` | Hardening, docs, contract review, release binaries |
| v1.0.0 | `release/1.0.0` | Contracts frozen |

The order changes only by owner decision.

## 16. Open questions for the owner

1. Platforms: Linux and macOS only for v1? Windows needs a different process-control layer.
2. License for the repository.
3. Browser tool: the grok-code approach, the pi approach, or fetch-only for v1?
4. opencode free models: endpoint details and usage terms.
5. Is `state/` next to the four user folders acceptable?
6. The six extra slash commands in 10.4.
7. Default provider, model and reasoning level on first run (proposal: medium for cloud models, off for small local models).
8. A transparent-background version of the logo.

## 17. Sources consulted

- opencode providers: https://opencode.ai/en/docs/providers and https://v2.opencode.ai/docs/providers
- llama.cpp server: https://github.com/ggml-org/llama.cpp/tree/master/tools/server and the endpoint tour at https://mvysny.github.io/llama-server-endpoints/
- Token counting through `/tokenize`: https://strandsagents.com/docs/api/python/strands.models.llamacpp/index.md
- Ratatui async template: https://github.com/ratatui-org/templates/blob/main/simple-async/README.md and the 2026 practices summary at https://skillselion.com/skills/blacktop/dotfiles/ratatui-tui
- Agent Skills specification: https://agentskills.io/specification.md
- Anthropic streaming: https://platform.claude.com/docs/build-with-claude/streaming
- pi harness: https://www.ai.engineer/talks/RjfbvDXpFls-pi-coding-agent and https://flaviocopes.com/pi/
