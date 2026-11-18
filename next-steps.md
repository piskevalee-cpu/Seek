# SEEK — next steps

Date: 2026-11-18. Repo state at the time of the investigation: `develop` at `6a693d4`, `main` at
`ee71269`, release `v0.7.1`.

This file collects the findings of the investigation and the work that follows from them. It is a
planning document, not a contract; the specification remains the source of truth.

## 1. Universal skill/tool acceptance; real-time listing and calling

### What SEEK does now

- **Skills** are discovered once in `Runtime::new` from `~/seek/skills/` plus `builtin/skills/`
  (`crates/seek/src/lib.rs:84-96`). `seek-core::skill` parses `SKILL.md` with `serde_norway`,
  requires `name` and `description`, and stores `allowed_tools`, `license`, `compatibility`,
  `metadata` (`crates/seek-core/src/skill.rs`). The catalog is only name + description + path.
- **External tools** are scanned once from `~/seek/tools/` (`tool.toml`, `schema.json`, `run`);
  `ExternalTool::run` feeds JSON on stdin and caps output.
- Live updates: the *toggle* state already reaches the running agent (shared `RwLock<ToolRegistry>`
  and context string, D-021). What does **not** update is the *inventory*: a tool or skill folder
  added or edited while SEEK runs is invisible until restart.
- `Skill.allowed_tools` is parsed but **never enforced** — no code reads it.
- Acceptance is strict: a skill with an unparseable frontmatter, a name that differs from its
  folder, or a missing description is skipped. The Agent Skills guide recommends lenient loading
  for cross-client compatibility.

### OMP / Agent Skills reference

- `reference/agentskills/docs/client-implementation/adding-skills-support.mdx` recommends
  scanning both the client-native directory and the cross-client ` .agents/skills/` convention at
  project and user scope; project-level overrides user-level on name collisions; skip `.git` and
  `node_modules`, cap depth/count; warn but load on cosmetic frontmatter errors; skip only when
  `description` is missing or YAML is unparseable.
- MCP clients additionally consume `notifications/tools/list_changed`,
  `notifications/resources/list_changed` and `notifications/prompts/list_changed` so newly
  published tools appear without restart (`reference/omp/docs/extensions.md:686`).

### Proposal

1. Add a `seek_core::catalog` refresh that scans, in order: project `.agents/skills`, project
   `.seek/skills`, user `~/.agents/skills`, `~/seek/skills`, then built-ins. Same-name merges keep
   the higher-precedence entry and record a diagnostic in the log strip.
2. Make parsing lenient: quote unquoted `description` values with colons before retrying; warn on
   name/folder mismatch and long names (do not skip); keep the strict skip for missing description
   and unparseable YAML.
3. Add `/refresh` (or reuse `/tools`, `/skills` opening) to rescan both folders, replacing the
   shared registry and skill catalog live. A bounded `notify`-style watcher can be a later step;
   an explicit command is enough for v1 and needs no new dependency.
4. Enforce `allowed-tools` when a skill is invoked: intersect the skill's list with the enabled
   tools for that turn, and tell the model which tools the skill may use via the injected body.
5. Version/contract note: only additive new scan paths; no change to `tool.toml`/`SKILL.md`.
   Document in SPEC §9.3/9.5 and a new decision.

Tests: precedence and collision, lenient frontmatter, `/refresh` adding a tool/skill mid-turn,
allowed-tools intersection, no panics on broken folders.

## 2. Installer verification (user has not tested it)

Already verified during this investigation:

- `sh -n install.sh` equivalent by execution: `SEEK_HOME=/tmp/... sh install.sh --no-binary`
  printed the 25×9 logo (plain frame when piped), created the five folders and copied the three
  built-ins without error.
- `SEEK_HOME=/tmp/... sh install.sh` downloaded `seek-v0.7.1-x86_64-unknown-linux-gnu.tar.gz`,
  unpacked `seek-v0.7.1-<triple>/seek`, installed it, and `seek --version` printed
  `seek 0.7.1 (ee71269)`.
- `install.sh --help` still prints the usage header.

Still to check by hand (needs a real terminal): the five-frame TTY animation, cursor restore after
it, `NO_COLOR=1`, `CI=1`, an idempotent second run ("kept" lines, no overwrites), a slow/failed
network path falling back to `cargo install`, and macOS target mapping. Add a short installer
section to `docs/TESTING.md`.

## 3. Context compaction — rules and implementation

### SEEK now

- `/compact` takes every message from `App::history()`, sends it to the current model with
  "Summarize the conversation above …", and on success replaces the transcript with
  `Row::User("[compacted]")` plus the summary (`crates/seek/src/lib.rs:553`, `app.rs:2027`).
- It is manual only, local models are refused ("a provider is needed to compact"), the summary is
  **not** a session event (so resume loses it), there is no threshold, no auto/mid-turn trigger,
  no protected recent window, and no pruning of old tool output.
- `/truncate` drops oldest messages to fit a model window; switching to a smaller model checks
  `estimate_history` and blocks with a suggestion to compact or truncate.

### OMP rules worth porting (`reference/omp/docs/compaction.md`)

- Triggers: manual `/compact [instructions]`; overflow recovery; incomplete-output recovery;
  post-turn threshold maintenance; mid-turn threshold check before the next provider request;
  idle maintenance.
- Defaults: reserve `max(15% of window, 16384)`; `keepRecentTokens = 20000`; threshold from
  `thresholdTokens`/`thresholdPercent` else reserve-based; success must land at ≤80% of threshold
  or maintenance reports "no progress" and blocks continuation.
- A `CompactionEntry` is a first-class session entry with `summary`, `firstKeptEntryId`,
  `tokensBefore`, optional `method`, `warning`. On rebuild, summary + kept entries are replayed;
  the **display transcript** keeps every entry and shows a slim divider at the compaction point.
- Before summarizing, prune old tool results: never below 50 tokens, never active skill/plan
  reads, superseded reads are blanked with a short notice, and cache-lookback windows are
  respected.

### Proposal for SEEK (small, spec-shaped version)

1. Add `SessionEvent::Compacted { summary, first_kept: usize, tokens_before: u64 }` to the JSONL
   contract (`schema = 1` additive field/event, migration = old files simply have none).
2. `History` gains a compaction boundary: keep all rows for display, but build LLM messages from
   the last summary + messages after `first_kept`. `Session::replay()` returns both.
3. `Runtime::compact(reason)` computes used tokens from the last response usage (ground truth),
   picks a cut point that keeps a recent window (start with ~20% of the model window, cap 20k),
   summarizes the older part, appends the compaction event, and returns headroom.
4. Auto trigger after a turn and before the next request when used ≥ `window − reserve`
   (reserve = max(15%, 16k)); require ≤80% headroom, else a one-line warning, no loop.
5. Tool-result pruning before the cut: drop results older than the kept window, with a short
   `[output elided]` marker; never prune the most recent turn.
6. `/compact <instructions>` passes custom instructions; local models get a deterministic
   extractive fallback (no summarizer needed) or a clear "needs a cloud model" message.
7. Tests: cut point math, resume after compaction, auto trigger/no-progress guard, pruning floor.

## 4. Chat history

- Storage: append-only JSONL at `state/sessions/<id>.jsonl`, header + `Message` events with an
  optional `ModelTag` + `ModelSwitch` events. `Session::title()` derives the title from the first
  user message, `model_history()` walks switches.
- Replay: `Runtime::resume_session(id)` replaces `runtime.history` and opens the file for append;
  `/resume` sends `UiMessage::Session(messages)`; the UI rebuilds `Row`s.
- Gaps: no compaction entries (see §3), no explicit "last session" pointer, `SessionInfo` has no
  cwd so sessions from other projects are mixed in the sidebar, and the transcript cannot show a
  model switch as a visual divider (it is only in the file).

Proposal: add `cwd` to `SessionInfo`, filter the sidebar by current cwd (with an "all projects"
toggle if desired), record compaction events, and render model switches/compactions as inline
dividers from the file rather than reconstructing them from messages.

## 5. `--continue` with real context and last model

There is no `--continue` CLI flag today (`crates/seek/src/main.rs` has only `--reduce-motion`,
`--doctor`). Design:

1. Add `#[arg(long)] continue_: bool` (clap rename to `--continue`).
2. Resolve the target session: `state/last_session.toml` written on every `append_message` and
   `append_model_switch`, or fall back to the newest session whose header `cwd` matches the
   current directory. Delete/stale file falls back to a fresh session.
3. On startup, call `runtime.resume_session(id)` **before** the agent loop and before the UI gets
   control; then set the runtime model from `Session::model_history()?.last()` (last switch wins,
   else the model tag of the last message), and push an `App` message that rebuilds rows, selects
   the model chip and recalculates context.
4. Restore toggles, reasoning level and sidebar side as today (they are already file-backed).
5. Pins and the turn-local context are **not** persisted; a continued conversation starts with a
   fresh turn. Document this.
6. If no model can be recovered, keep `[no model selected]` and let `/model` choose.
7. Tests: CLI parse, resume-with-model, missing/stale pointer, no-new-session-until-message,
   context count after resume.

## 6. Sidebar PIDs and processes (not hand-tested)

- `BashTool` spawns with `.process_group(0)`, registers `pid` as `TAG_AGENT`, and removes it when
  the foreground command ends (`crates/seek-core/src/tools.rs:161-175`).
- `App::sync_processes()` copies `ProcessRegistry::list()` into the sidebar when it is visible;
  `x` calls `registry.kill(pid)` which signals the whole group (`crates/seek-core/src/process.rs`).
- Known gap from AGENTS.md: a background command leaves the registry when the shell exits even
  though the child keeps running.
- Manual test: start `sleep 300 &` via an agent `bash` call, open Ctrl+B, verify PID/tag/command/
  uptime, press `x` on both agent and server entries, verify `kill -0 <pid>` fails afterwards.
- Fix idea: keep the registration for detached/background children and remove entries only after a
  real reaping check (`kill(pid, 0)` / waitpid) rather than at shell exit; risk = stale entries,
  so prune on a tick.

## 7. OMP notification protocol (for terminal completion/error toasts)

Findings (`reference/omp/packages/tui/src/terminal-capabilities.ts`, `desktop-notify.ts`,
`modes/controllers/event-controller.ts`, `modes/settings.ts`):

- Three protocols: `BEL` = `\x07` (fallback), `OSC 9` = `\x1b]9;` (iTerm2/WezTerm),
  `OSC 99` = `\x1b]99;;` (Kitty). Sequences end with `\x1b\\`.
- Structured `TerminalNotification { title, body, id, type, urgency, iconName, sound, actions,
  expiresMs }`. With confirmed OSC-99 support, fields are serialized as `key:value` metadata and
  base64 for control characters, max 2048 bytes per chunk, with IDs namespaced per process.
  Otherwise the structure collapses to `"title: body"` on the chosen single-line protocol.
- Suppression: `PI_NOTIFICATIONS=off|0|false`; headless sessions skip; multiplexer routing is
  attempted first; on Linux BEL-only terminals an optional `notify-send`/`gdbus` toast is fanned
  out. Settings: `completion.notify` default on, `error.notify` default off.
- OMP fires completion on `agent_end` only when the last assistant stop reason is not
  `aborted`/`error`; error notification when it is `error`.

SEEK proposal (minimal): a `seek_tui::notify` module that picks the protocol from `TERM`/
`TERM_PROGRAM`/`WT_SESSION` (default BEL), respects `SEEK_NOTIFICATIONS=off`, writes
`\x1b]9;<title>: <body>\x1b\\` or `\x1b]99;;...\x1b\\` directly to stdout after a completed or
failed turn, never injects bytes into the TUI buffer (write between `terminal.draw` frames), and
exposes `notify.completion` / `notify.error` in `state/user.toml`. Tests: pure formatting,
suppression, stop-reason gating. No new dependency; D-Bus fallback is out of scope for v1.

## 8. Suggested order and acceptance criteria

1. Chat history + compaction events + `--continue` (one coherent contract change, highest value).
2. Universal skills/tools + live refresh (inventory watcher, lenient frontmatter, allowed-tools).
3. Notification protocol (small, isolated).
4. Installer manual checklist and process-registry fix (verification/hardening).

Every item: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace -j 2`, a walk through `docs/TESTING.md`, SPEC/DECISIONS/CHANGELOG updates,
one branch from `develop`, squash PR, release tag after CI.

## 9. Open questions for the owner

- Auto-compaction on by default, or manual only with a threshold warning?
- Should `--continue` always resume the last session of the current directory, or the globally
  last one?
- Which cross-client skill paths must be supported in v1 (`.agents/skills` only, or also
  `.claude/skills`)?
- Desktop notifications: in-band escapes only, or also `notify-send` on Linux?
- Should project-level skills be gated behind a trust prompt?

## 10. Prompt bar caption: show the working directory, not the model twice

**Reported:** the model name appears twice — in the prompt box's top border and in the footer —
while the working directory is not visible near the prompt.

**Fix:** the top border of the message box shows the shortened working directory (`footer::short_path`),
the footer keeps the model chip. One fact, one home (SPEC 10.2). The caption must still fit a long
path: shorten with `~` and keep the last two components, the same rule as the footer.

Tests: the rendered prompt box contains the cwd and does not repeat the model name; the footer still
shows the model; the plain-text and half-block logo tests are unaffected.

## 11. A more evident startup logo

**Reported:** the diagonal streak is still hard to see. The user suggested a wider range of dark
cubes and a darker resting logo colour.

**Fix:** draw the previously blank cells of the 25 x 9 bitmap in a very dark cube shade, so the
mark has body without becoming a filled rectangle. Darken the resting colour of lit and dim pixels
so the streak has more contrast, widen the streak, and keep the six-second-family sweep and the
per-pixel twinkle. `reduce_motion` still holds it still. SPEC §12 and DECISIONS must reflect that
dark cells are now drawn.
