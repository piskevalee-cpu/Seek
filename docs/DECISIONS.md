# Decisions

Records changes to, and clarifications of, the specification. The specification (`docs/SPEC.md`)
marks unconfirmed proposals with `[DECISION]`; this file is where they are confirmed, changed or
rejected, as required by SPEC 0.

Format: one entry per decision, newest last.

## D-001 — The config points at the binary and at the subcommand

**Date:** 2026-11-11 · **Affects:** SPEC 5, SPEC 7.2

SPEC 7.2 says to run `llama-server`, and SPEC 5 puts a `llama-server path` in `config.toml`.
The shipped llama.cpp since 0.9.0 is one `llama` binary whose server is the `serve` subcommand
(`llama serve`, `llama cli`, `llama download`, `llama update`). The legacy `llama-server` binary
still exists in some distributions and accepts the same flags.

**Status:** accepted (the owner decided).

The config has two keys:

```toml
llama_path = "/usr/local/bin/llama"
llama_subcommand = "serve"
```

`llama_subcommand` is empty for the legacy binary, which is the server itself. The old
`llama_server_path` still works: it is migrated to the new keys on the first write
(`Config::migrate`), so an old file keeps working.

## D-002 — Theme colors sampled from the logo

**Date:** 2026-11-11 · **Affects:** SPEC 12, SPEC 10.6, SPEC 16.8

SPEC 12 estimates a green background (`#22B14C`) with light gray (`#B5B4B4`) foreground and asks
for the exact values to be sampled from the PNG with a script.

The assets are three files:

| File | What it is |
| --- | --- |
| `assets/logo.png` | the mark on a transparent background, cropped to its ink (400x144) |
| `assets/logo-green.png` | the same mark on the brand green (500x199) |
| `assets/logo.pixil` | the editable design the two were exported from |

`tools/extract_logo.py` samples them:

| Token | Value | Source |
| --- | --- | --- |
| `cube_light` | `#B4B4B4` | sampled from the strokes of the logo |
| `accent` | `#22B14C` | sampled from the background of the green logo |
| `bg` | `#141617` | the background of the terminal, **not** of the logo |
| `cube_dark` | `#2C2E2F` | an unlit cube |
| `dim` | `#7C7D7E` | the dimmed text |

**Status:** accepted. The `bg` token is deliberately dark: a green terminal is unreadable, and the
logo keeps its green in the asset.

**The transparent variant exists now**, which closes SPEC 16 question 8.

**The extraction.** The script finds the cell size from the pitch of the letters (the letters of
the pixel font are three cells wide and one cell apart, so the pitch of two letters is four
cells) and takes the bounding box of the ink as the grid. That works for the pixel-perfect source
(35px cells), for a scaled and anti-aliased rendering (18.75px cells) and for the transparent one
(16px cells): all three give the same 25x9 grid.

The levels are measured against the ink when the background is transparent, and against the
background when it is a color, so the anti-aliased edges of the green logo stay out of the grid.

**Open:** nothing. The terminal never scales the PNG: the grid is extracted once and committed as a
constant.

## D-003 — Reasoning support is detected via `chat_template_caps`

**Date:** 2026-11-11 · **Affects:** SPEC 9.7

SPEC 9.7 says to detect local reasoning support by reading the chat template from `GET /props` and
looking for `enable_thinking` and `reasoning_effort`. `llama serve` also returns a structured
`chat_template_caps` object with `supports_reasoning_effort`, `reasoning_effort_levels`,
`supports_tool_calls` and `supports_preserve_reasoning`.

Verified against a real model: the template text contains the literal string `enable_thinking`
while `chat_template_caps.supports_reasoning_effort` is `false` and `reasoning_effort_levels` is
empty. Grepping the template therefore yields a wrong answer.

**Status:** accepted. SEEK reads `chat_template_caps` first and only falls back to inspecting the
template text when the caps are absent (older builds).

## D-004 — The repository was renamed to `Seek`

**Date:** 2026-11-11 · **Affects:** SPEC 2.1

SPEC 2.1 states that the repository name ends with a hyphen (`Seek-`) and must keep it. The owner
renamed the repository to `Seek`; GitHub keeps `Seek-` as a redirect to the new name.

**Status:** accepted. The remote is `https://github.com/piskevalee-cpu/Seek.git` and SPEC 2.1 has
been updated. The crate and binary remain `seek`.

## D-005 — The license stays undecided

**Date:** 2026-11-11 · **Affects:** SPEC 2.1, SPEC 16.2

The repository has no license yet. The owner keeps it that way for now.

**Status:** open. The release archives have no LICENSE file yet, and the Cargo metadata has no
`license` field. Both have to be settled before the crates are published.

## D-006 — The first run asks which model to use

**Date:** 2026-11-11 · **Affects:** SPEC 16.7

Nothing is configured on the first run. SEEK opens the model picker and asks, instead of guessing
a default provider, model or reasoning level.

**Status:** accepted. The picker opens when the application starts and a model has not been chosen
yet, and a local model in `~/seek/models` is enough to make the question worth asking.

## D-007 — The fetch tool is the only network tool

**Date:** 2026-11-11 · **Affects:** SPEC 9.3, SPEC 16.3

There is no browser tool. The `fetch` tool reads a URL and returns it as text.

**Status:** accepted. A browser needs a browser on the machine and is a much bigger thing to
maintain; `fetch` covers the common case.

## D-008 — What was taken from the grok-build sources

**Date:** 2026-11-11 · **Affects:** SPEC 9.2, SPEC 9.3, SPEC 9.4

The grok-build sources (Apache-2.0) were read for the parts that overlap with the specification:
the tool implementations, the truncation helpers, the token estimation, the permission rules and the
compaction.

**Taken and rewritten for SEEK** (`crates/seek-core/src/truncate.rs`):

- The cut of a string always lands on a character boundary. The first version of the capping in
  SEEK used `String::truncate`, which **panics** when the index is in the middle of a character.
- A long line is wrapped instead of being cut, so the model sees all of it. This matters for the
  output of `bash`.
- The marker says how many lines and bytes were left out, and how to fetch them.

**Not taken:**

- The browser and the computer tools: they are tied to the xAI cloud and to a full desktop
  environment. The `fetch` tool stays the only network tool (D-007).
- The web search tool: it calls the xAI Responses API.
- The permission rules: the risk of a command is computed per segment, but SEEK asks for every
  `exec` in `ask` mode anyway, so the extra work buys nothing here.

**The token estimation** is the same heuristic SEEK already used (bytes per token, four).

**The license.** grok-build is Apache-2.0. Nothing was copied verbatim; the ideas were written
again in SEEK's own style. The license of SEEK itself is still undecided (D-005).

## D-009 — SEEK asks for a context of 32768 tokens

**Date:** 2026-11-11 · **Affects:** SPEC 7.2

The context of a model can be huge: the GGUF of a small model says 131072 tokens. llama.cpp
allocates the key and value cache of the whole context up front, and that is 6.5 GB for that
model — enough to take the machine down while a build runs.

The launch options already have an implicit set (SPEC 7.2), so `--ctx-size 32768` joins them. The
user can override it in the launch options of the model, and the editor shows the implicit part.

**Status:** accepted. A coding agent does not need 131k tokens of context, and 32k fits on a
normal machine (about 1.6 GB for that model).

## D-010 — The look of the logo and of the animation

**Date:** 2026-11-11 · **Affects:** SPEC 10.3, SPEC 12

The first version of the interface drew the logo as a grid of cubes, which put a block of colour
around the mark, and the thinking animation was a 3x3 grid of cubes. Neither looked like the
harness it was meant to be.

The look now follows the grok-build pager (Apache-2.0):

- **The logo** draws only its lit pixels; the background of the mark is the background of the
  terminal. A sheen sweeps across it from the bottom left to the top right: a raised-cosine band
  over the diagonal position of every glyph, blended from the resting grey toward the lit colour.
  The logo is centred both ways.
- **The animation** is one line of twelve cells that are dark, with a wave of brightness going
  through them: a `sin²` per cell. The thinking and the loading spinner are the same line.
- **The messages**: the prompt of the user is a band across the width with the arrow `❯` in front
  of it; the answer of the model is plain text, without a band and without a border.
- **The log strip** is one line instead of three.

**Status:** accepted. The specification was updated with it (§10.3, §12).

## D-011 — The colours and the shape of the interface

**Date:** 2026-11-12 · **Affects:** SPEC 10.2, 10.3, 12

The interface follows the pager of the grok-build harness (Apache-2.0), because the owner asked
for that look:

- **The palette** is tokyonight: the background is the storm blue (`#24283b`), the text the pale
  foreground (`#c0caf5`), the dim text `#a9b1d6`, and the gray ramp `#3b4261`, `#565f89`,
  `#737aa2`. The user is blue (`#7aa2f7`), the model magenta (`#bb9af7`), the tools gray. The
  prompt border is `#323e64`, and `#4b5c8c` when it has the focus. The green of SEEK (`#22b14c`)
  stays for the brand: the logo and the `LOCAL` tag.
- **The rails.** Every block of the transcript has a one-column rail on the left, coloured by the
  kind of the block. The rail is what tells the blocks apart, so the boxes are not needed.
- **The spacing.** A blank line sits between the blocks, and a gap of two columns sits between
  the sidebar and the messages.
- **The prompt is a box** with rounded corners: the name of the model in the top border, the keys
  in the bottom one. Before, the prompt was a `>` and a line of text, and it did not look like an
  input at all.
- **The footer is one line**: the mode, the model, the place and the metrics. The metrics moved
  from their own line under the prompt bar into the footer, so the bottom of the screen is one
  line and not three.
- **The thinking line** is eight cells and the streak wraps around: a new one starts at the left
  when the old one reaches the right. A `sin²` ping-ponged instead, and the same cells lit up
  again and again.

**Status:** accepted. The specification was updated with it (§10.2, §10.3).

## D-012 — The turn does not block the commands

**Date:** 2026-11-12 · **Affects:** SPEC 4.1, 9.1, 9.4

The agent loop waited for the turn to end before it took the next command of the user. The
answer to a permission request and the cancel of Esc both travel through that loop, so neither
ever arrived: the tool waited for an answer that could not come, and the turn hung.

This was not a detail: the owner reported a tool that stayed `running` for minutes, and an Esc
that did nothing at all.

The turn now runs in a task of its own, so the loop keeps taking the commands:

- the answer to a permission request arrives, and the tool runs;
- Esc stops the turn at once, even while the model is still loading;
- a cancel that comes before the turn has started is remembered, not lost;
- the permission request and the tool run give up when the turn is cancelled;
- the conversation goes on from turn to turn, so the model sees what was said before.

Two things came with it:

- **The bash command runs in a blocking task.** It waits for the process, and that froze the
  whole runtime: the interface did not answer while a command ran.
- **The answer of a local model never takes more than half of the window.** The provider asked
  for 8,192 tokens whatever the model could hold, and the server refuses a request that does not
  fit: `request (4108 tokens) exceeds the available context size (4096 tokens)`.

**Status:** accepted.

## D-013 — The keys come from a thread of their own

**Date:** 2026-11-12 · **Affects:** SPEC 4.2

The event loop read the crossterm `EventStream` directly. The reader of crossterm blocks until
something happens, and the `EventStream` hands the event over from a thread of its own: the loop
waited for the reader, and the reader waited for the next key. The keys arrived late, or not at
all.

That is what the owner found: **Esc did nothing, and the message that came after it was never
taken.** Nothing in the transcript said why — the keys were simply gone.

The keys now come from a thread of their own and the event loop reads them from a channel. The
pattern of the branch matters as well: `Some(Ok(event)) = events.next()` disables the branch for
ever when the stream hands over an error, and the user is left with a keyboard that does nothing.

**Status:** accepted. The specification was updated with it (§4.2).

## D-014 — The guards of the model

**Date:** 2026-11-14 · **Affects:** SPEC 9.1

The same 1.7B model is smart in the oh-my-pi harness and dull in SEEK. The difference is not the
prompt: it is that oh-my-pi **watches the model and interrupts it**.

Two habits are watched for, both of which look like work:

- **The reasoning that goes nowhere.** When the reasoning repeats itself or grows past what a
  turn needs, the stream is aborted and a `<system-interrupt>` is put in the history: *"Act now,
  not further planning: emit a real call to an available tool."* The turn goes on with the
  message in the context.
- **The call that comes back.** The same tool with the same arguments five times in a row gets
  a `<system-interrupt>` that tells the model to change its arguments or yield.

The numbers are the ones of oh-my-pi (MIT): a short block that comes back four times covering
180 characters, a long one three times covering 1,024, four paragraphs in a row that are alike
(a trigram overlap of 0.8), eight paragraphs in a row that recycle the wording without naming
anything new, and five identical calls. The guard fires at most twice per turn, and a visible
answer latches it off.

The first version of the guard interrupted the model after a thousand characters of reasoning.
That was too blunt: the model took the instruction literally and made a pointless call to
satisfy it. The paragraph heuristics of oh-my-pi are the ones that work.

The other two things came from there as well: every tool call carries an intent (`i`), and the
read numbers every line so the edit can be made by line.

**Status:** accepted. The specification was updated with it (§9.1).

## D-015 — The web search is a chain of providers

**Date:** 2026-11-17 · **Affects:** SPEC 9.3, 9.4, 5

The first `web_search` was the DuckDuckGo HTML page. That is one engine, and a blocked page
leaves the tool with nothing. The tool is now the search stack of oh-my-pi (MIT): a structured
query parser, one provider per engine, and a chain that walks them in order.

The **query parser** takes the operators apart before anything goes to the network. `site:`,
`inurl:`/`url:`, `intitle:`, `intext:`/`inbody:`/`inanchor:`, `filetype:`, `before:`/`until:`,
`after:`/`since:`, `lang:`, the quoted phrases, the `OR` groups and the `-exclusions` become
fields of the query. Each engine gets the formatter for its own syntax: the engines that parse
the classic operator set get the whole query, the scrapers get the operators that match nothing
across a rendered page demoted to plain keywords (a path-carrying `site:` and every `inurl:`,
except the negated forms), and the APIs that have a parameter for a constraint get it there
(`freshness`, `tbs`, `include_domains`, `start_date`). The remaining constraints are enforced on
the results that come back. A constraint that would empty the answer is relaxed, and the model is
told: `Note: no result matched ...; the constraint was relaxed`. An empty answer is worse than an
approximate one.

The **chain** is `web_search.providers`, and the first provider that answers a non-empty search
wins. The default is `duckduckgo`, `mojeek`, `startpage`, `ecosia`: DuckDuckGo answers from a
shared address, the other three are there for the day it does not. The failures are named in a
`Note:` line, so the model knows whose results it is reading. The thirteen ids are `duckduckgo`,
`startpage`, `mojeek`, `ecosia`, `hackernews`, `searxng`, `public`, `brave`, `tavily`, `exa`,
`jina`, `kagi` and `firecrawl`.

The **keys** follow SPEC 5: the environment variable is the name of the entry in
`state/auth.json`, the configuration may point at another variable (`web_search.key_env`), and a
key never goes into `config.toml` or `providers.toml`. The names are `BRAVE_API_KEY`,
`TAVILY_API_KEY`, `EXA_API_KEY`, `JINA_API_KEY`, `KAGI_API_KEY` and `FIRECRAWL_API_KEY`; a
SearXNG instance may ask for `SEARXNG_TOKEN` or `SEARXNG_BASIC_USERNAME`/`SEARXNG_BASIC_PASSWORD`.
The environment wins over the auth file, so a key can be overridden for one run.

The **Public Web** aggregate fans out over the credential-free engines at once and merges what
comes back: the same address found by two engines ranks above an address found by one, the best
rank breaks the tie, and engine order breaks the rest. It is explicit-only in the sense that the
default chain does not name it; a chain that does chose the fan-out. Its soft deadline is five
seconds and its hard deadline thirty, so one slow engine cannot hold the turn.

Two observations from the live check (2026-11-17): DuckDuckGo answers from this machine;
Mojeek and Ecosia answer their bot wall to a shared address; Startpage now serves an Anubis
proof-of-work challenge. Each provider recognises its wall and names it instead of returning an
empty page. A keyed provider is the reliable path when the scrapers are throttled.

**Status:** accepted. The specification was updated with it (§9.3).

## D-016 — The web search is on by default

**Date:** 2026-11-17 · **Affects:** SPEC 9.2, 9.3, 9.4

The oh-my-pi agents search the web by query without being handed a URL, and the owner asked for
the same: a question about the world should not require the user to find the page first. The tool
was ported in D-015 but left off by default, so in practice the agent could not search until the
user opened `/tools`. That made the port useless out of the box.

`web_search` is now in `prompt::DEFAULT_TOOLS` and in `toggles::DEFAULT_ON`, next to `read`,
`write`, `edit` and `bash`. The prompt names it, the agent sends its schema, and `/tools` shows it
checked. The other network tool, `fetch`, is still off by default: reading one page is a narrower
need than finding it, and the search result already carries the snippet.

The permission matrix is unchanged. A search carries `Risk::Network`, so it is asked in `ask`
mode (the default) and in `read` mode, and allowed in `yolo` mode (SPEC 9.4). Asking once per
search is the cost of letting a tool reach the network; the user can switch to `yolo` for a
session that searches a lot.

**Status:** accepted. The specification and the prompt budget test were updated with it (§9.2,
§9.3).

## D-017 — The composer completes the message, and the chat starts with no model

**Date:** 2026-11-17 · **Affects:** SPEC 10.1, 10.3, 10.4

The oh-my-pi composer completes what is typed: slash commands, `@` file references and paths. It
also opens straight into the chat. The owner asked for both: no model picker on launch, a
`[no model selected]` chip instead, `/model` to choose, and OMP's message completion.

**Completion.** A new `seek-tui::completion` module looks at the token under the cursor. At the
start of the line it offers the slash commands and the skills; an `@` at a token boundary searches
the project below the working directory with a subsequence match on the relative path, including
hidden files but never `.git`; a token that looks like a path lists one directory. A completion
replaces only its token, so prose around a path is kept, and a path with a space is quoted. A file
is inserted with a trailing space, a directory with a trailing `/` so the list moves inside it.
Tab accepts, arrows move, Esc closes until the next edit, and Enter accepts a path without
sending but takes and runs a command. The walk is capped (depth 8, 4,000 entries, 50 results) so
it cannot stall the event loop (SPEC 4.2). The list is computed from the state, not stored: the
same function feeds the keys and the view.

**Startup.** `App::new` now names no model, and `Message::Start` no longer opens a picker. The
footer and the top border of the prompt both say `[no model selected]`. A message typed before a
model is chosen is not sent: the transcript asks for `/model` and the draft stays in the bar.
Slash commands still work, including `/model` itself. The first run still asks for the user's name
(SPEC 9.5). The fake demo provider is no longer reached by a normal message without a model.

**Status:** accepted. The specification and the manual checklist were updated with it (§10).

## D-018 — The transcript has a side bar, and the sidebar can move

**Date:** 2026-11-17 · **Affects:** SPEC 9.5, 10.1, 10.2, 10.3, 10.4

The owner asked for OMP's scrolling in SEEK and for a small bar on the side of the transcript. The
sidebar itself should be able to sit on either side, with the bar always on the opposite side.

**Scrolling.** The two scroll fields (`scroll` from the bottom and `scroll_top` as a frozen anchor)
become one first-visible-row offset and a `follow_tail` flag, the way the oh-my-pi scroll view
works (MIT). A fixed offset keeps the line the user is reading in place while the model appends;
`follow_tail` keeps the view at the bottom only while it was already there. The thumb geometry is
ported from `scroll-viewport.ts`: size `clamp(height² / total, 1, height)`, travel `height - size`,
top `round(offset / maxOffset * travel)`.

The bar is one column with a one-column gap, drawn only when the content overflows and never on
Home. The text is re-wrapped with the two columns removed, so it never runs under the bar. With no
sidebar the bar is on the right, as in OMP. The old numeric `↑ N of total` badge is the previous
home of the same fact; it is now drawn only when the bar cannot be drawn, which keeps SPEC 10.2.

**Mouse.** Mouse capture is now really enabled (`execute!(stdout(), EnableMouseCapture)`); before
this the wheel handlers existed but no mouse events reached the event loop. A left click above or
below the thumb pages in that direction, a click on the thumb or a drag maps the thumb row back to
an offset with the same geometry. Shift+Up/Down scroll five lines; Up/Down stay with history and
row selection, so they are not taken away.

**Settings.** `state/user.toml` gains an optional `sidebar_side = "left" | "right"`, default
`left`. This is a contract change but it is additive and `#[serde(default)]`: an old file parses
unchanged, which is the migration. `/settings` opens a picker with "User name" and "Sidebar side";
the second opens Left / Right and writes the choice back through a `SetSidebarSide` command. The
sidebar rectangle is computed from the side, and the bar follows the opposite side at once.

**Status:** accepted. The specification, the manual checklist and the changelog were updated
(§10).

## D-019 — `@` pins paths, and the logo twinkles like the landing page

**Date:** 2026-11-18 · **Affects:** SPEC 9.2, 9.5, 10.3, 12

The owner asked for the OMP behaviour where `@` in a message points the agent at a file or a
directory, but with far fewer tokens than OMP spends. The landing page's centred logo also has a
per-pixel shine; the same effect should be visible in the terminal at startup and in the thinking
animation.

**Pins.** OMP's `utils/file-mentions.ts` resolves `@path` mentions and injects the whole file
content into a `FileMentionMessage`, capped at 5 MB of text. SEEK resolves the same mentions
exactly (relative to the project root, `~` and absolute paths allowed), rejects ones that do not
exist, deduplicates them and caps them at eight. The turn context gets only a one-line description
per pin — path, size, and up to twelve names for a directory — with no content. The model reads a
pin with `read` when it needs it, and the permission matrix is untouched: a read is always
allowed; a write or a command still follows the mode. This keeps the context in the tens of tokens
instead of thousands and leaves the default prompt prefix unchanged (the budget is now 889 / 1,500
tokens). The draft shows a compact `pinned` chip row above the completions; the message itself is
the home of the pins (SPEC 10.2).

**Logo and thinking.** The landing page draws each logo pixel with a diagonal `px-shine` delay and
a short `px-twinkle` on its own index. The TUI already had the diagonal sweep; it now adds a
cosine twinkle with a per-cell phase (0.75 s period, 0.16 amplitude) in `shimmer::twinkle`. The
startup logo uses it for every lit pixel, and the eight-cell thinking line uses the same twinkle
over its travelling streak. `reduce_motion` still holds both still. This is colour only: the logo
keeps the terminal background behind its unlit pixels, and there is no new green in the theme.

**Status:** accepted. The specification, the manual checklist and the changelog were updated.
