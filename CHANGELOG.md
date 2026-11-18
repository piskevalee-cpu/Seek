# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). One version covers the whole
workspace; each release ships a milestone (SPEC 15).

## [Unreleased]

## [0.7.6] - 2026-11-18

Markdown answers in the transcript.

### Added
- Markdown in the model's answers: bold, italic, inline code, headings, bullets and block quotes
  are styled in the transcript instead of shown as raw markers.

## [0.7.5] - 2026-11-18

A reliable Enter key and a model selector in the same family as the `@` completion.

### Fixed
- The `/model` selector is now a bottom-anchored, searchable dropdown above the prompt, like the
  `@` completion and OMP's compact picker; the current model is marked `CURRENT`.
- Shift+Enter no longer claims to insert a newline: terminals did not deliver it reliably, so
  Enter always sends and the hint/help say so.

## [0.7.4] - 2026-11-18

Prompt-bar and overlay polish, plus an editable system prompt.

### Added
- `~/seek/SYSTEM.md` is the editable system prompt, installed once from `builtin/SYSTEM.md`; the
  compiled default is the fallback and still governs the prompt-budget test (D-023).

### Fixed
- The prompt bar draws a single cursor (the terminal's native one is hidden), wraps long lines so
  the end of a prompt stays visible, and grows up to the space available before scrolling inside
  the box.

### Changed
- The prompt bar shows the full working directory, and `@` pins now carry the absolute path of the
  pinned file or directory; the completion list also shows that location.
- Windowed menus now draw on an opaque stage with a rounded, responsive frame. The list scrolls to
  the selection with clipped-row markers, and the sidebar is omitted on short terminals as well as
  narrow ones (SPEC 10.3).

## [0.7.3] - 2026-11-18

A correction to 0.7.2: the logo's background stays transparent; the darker cubes are inside the
mark.

### Fixed
- The startup logo no longer fills its bounding box with dark cubes. Only the lit and dim pixels
  are drawn, and those pixels span several dark tones so the sweep still reads clearly (D-022).

## [0.7.2] - 2026-11-18

A visual pass: the prompt bar names the working directory instead of repeating the model, and the
startup logo has a dark body with a stronger sweep.

### Changed
- The prompt bar's top border shows the shortened working directory instead of repeating the
  model name; the footer keeps the model and the branch. The working directory has one home
  (SPEC 10.2).

### Added
- The startup logo now uses a range of dark cube tones inside the mark and a darker resting
  colour, so the bright diagonal streak is easier to see; the background stays transparent
  (D-022).

## [0.7.1] - 2026-11-18

The first patch after 0.7.0: a live tool and skill matrix, web search by query, OMP-style
completion, `@` pins, the side scrollbar, the brand shimmer and the guard fix.

### Fixed
- `/tools` and `/skills` now reach the running agent: the tool registry and skill catalog are
  shared cells replaced on every toggle change, so the next request of the same turn sees them.
  External tools follow the same rule (D-021).
- The startup logo's streak now runs from the top right to the low left, takes 4.2 seconds, and
  reaches full brightness; the old sweep was too faint and went the other way (D-021).
- `install.sh` prints the 25 x 9 logo with the same sweep, and rests on a plain frame when piped
  or when `NO_COLOR`/`CI` is set (D-021).
- `/tools` and `/skills` now change the running session as well as the file: the next turn's tool
  list and skill catalog come from the new toggles, and disabled tools are no longer announced to
  the model. The system prompt no longer names individual tools (D-020).
- The thinking guard no longer panics on non-ASCII reasoning: cycle checks now index characters,
  and a paragraph cut at the cap lands on a character boundary. Tool output capping was already
  safe; this was the guard inspecting the model's own text (SPEC 9.1).

### Changed
- The README header centres the shields, drops the `SEEK` wordmark in favour of a larger logo,
  and has a Next Steps section (provider support, universal skills and tools, widgets). It points
  contributors to the new `CONTRIBUTING.md`.
- SEEK opens on the chat with `[no model selected]` in the footer and in the prompt border; the
  model is chosen with `/model`. A message typed before that stays in the bar and gets a one-line
  note instead of being sent (SPEC 10.1).
- The README follows the Best-README-Template layout and shows only `assets/logo.png`, not the
  green variant.

### Added
- `@` pins in messages: an existing file or directory is attached to the turn as a compact name
  and size, not its content. The agent inspects it with `read` under the usual permissions (SPEC
  9.5, D-019).
- A `pinned` chip row above the prompt while composing a message with `@` paths.
- OMP-style per-pixel twinkle on the startup logo and the thinking line, on top of the diagonal
  sheen, with `reduce_motion` still resting both (SPEC 12).
- OMP-style transcript scrolling: a one-column side bar with a proportional thumb, wheel and
  click/drag scrolling, Shift+Up/Down for five lines, and follow-tail behaviour for new answers.
  The bar sits opposite the sidebar (SPEC 10.3).
- `/settings` now offers the sidebar side (left or right) alongside the user name; the choice is
  saved in `state/user.toml` as `sidebar_side` (SPEC 9.5).
- OMP-style message completion in the prompt bar: `/` commands and skills, `@` file references
  found anywhere below the project, and path completion for `src/`, `./`, `../`, `~/` and absolute
  paths. Tab accepts, arrows move, Esc closes, and a path replaces only its token (SPEC 10.3).
- The web search is on by default: `web_search` joins `read`, `write`, `edit` and `bash`, so an
  agent can answer a question about the world without being handed a URL. `fetch` stays off
  (SPEC 9.3).
- The web search is a chain of providers ported from oh-my-pi: DuckDuckGo, Startpage, Mojeek,
  Ecosia, Hacker News, SearXNG, the Public Web fan-out, and the keyed Brave, Tavily, Exa, Jina,
  Kagi and Firecrawl APIs. The chain is `web_search.providers`; the first provider that answers
  wins and the failures are named in a `Note:` line (SPEC 9.3).
- The structured query parser: `site:`, `inurl:`, `intitle:`, `intext:`, `filetype:`,
  `before:`/`after:`, `lang:`, quoted phrases, `OR` groups and `-exclusions`. Each engine gets
  only the syntax it understands; the rest is enforced on the results, and a constraint that
  would empty the answer is relaxed and reported.
- `web_search.searxng_url` points at a SearXNG instance, and `web_search.key_env` names the
  environment variable of a keyed provider. The keys stay in the environment or
  `state/auth.json` (SPEC 5).

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
- Shift+Tab switches the permission mode, whatever the sidebar does.
- The permission mode is read at every tool call: changing it while the agent works applies at
  once, to the turn that runs now.
- The file tools follow grok-build: the read numbers the lines, the edit replaces every
  occurrence when asked, and says the line the change landed on.
- The web search is the one of oh-my-pi (DuckDuckGo, no key). It is off by default.
- The system prompt is slimmer and teaches the tools: it is 701 of the 1,500 tokens.
- The first run asks for the name of the user, `/settings` asks again, and the model addresses
  the user by it.
- There is no green in the interface any more, and the `LOCAL` tag is only in the model picker.
- The thinking line sits behind the message, in the column of the text.
- The message bar holds the text and nothing else: the status is in the bottom border, so the
  box keeps its shape while the agent works and the cursor is the only thing that moves.
- The commands complete as they are typed: a dropdown of the slash commands and the skills, the
  arrows move the selection, Tab and Enter take it.
- The chat scrolls: the wheel and the page keys, the window anchored to the content so the
  message being read stays put, and the place in the transcript shown in the corner.
- The guards of the model, from oh-my-pi: a model that thinks and never acts is interrupted and
  told to act, and a model that repeats the same call is told to stop.
- Every tool call carries an intent: two to six words that name what the call does.
- The read numbers every line, and the edit takes the line the read showed: the model does not
  copy the text it read.
- The mode of the message is the mode of the session again: the shared mode had overridden it,
  and every tool asked for a permission, even in YOLO.
- The form that adds a provider, with the kind, the base URL, the key environment variable, the
  models, the context and the rate limits (SPEC 10.3).
- The providers are detected from the environment when there is no `providers.toml`, so a bare
  install works (SPEC 6).
- The keys can live in `state/auth.json`, readable by their owner only (SPEC 5).
- The models an endpoint knows are discovered and shown in the picker; the entries of
  `providers.toml` override them (SPEC 6).

[Unreleased]: https://github.com/piskevalee-cpu/Seek/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/piskevalee-cpu/Seek/compare/v0.0.0...v0.7.0
