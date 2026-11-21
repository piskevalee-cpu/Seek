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

## D-020 — Toggles reach the running session, and the logo uses a visible streak

**Date:** 2026-11-18 · **Affects:** SPEC 9.2, 9.6, 12

A user reported that turning a tool off in `/tools` did not remove it: the model still saw its
schema and called it. `Runtime::save_toggles` wrote `state/toggles.toml` but never updated the
in-memory `Runtime.toggles` that every turn reads. The change only took effect after a restart.

`Runtime.toggles` is now an `RwLock<Toggles>`. `save_toggles` writes the file and replaces the
in-memory copy, and `run_turn` takes a snapshot before it builds the tool registry and the skill
catalog. A tool turned off is therefore absent from the next request's `tools` and rejected if
the model calls it by name; a skill turned off leaves the next turn's catalog. The snapshot rule
keeps a turn consistent while it runs; the permission mode remains the only setting that applies
mid-turn (SPEC 9.4). The system prompt no longer names individual tools — their schemas and
descriptions are the home of that information — so the model is not told about a tool that was
disabled.

The startup logo's shine was too subtle to see. Replaced the wide raised-cosine band with the
same wrapping triangular streak the thinking line uses, stretched over the logo's 34 diagonal
positions and slowed to six seconds per traverse, peaking at 0.9 instead of 0.33. The per-pixel
twinkle was raised from 0.16 to 0.20. `reduce_motion` still holds both animations still.

**Status:** accepted. The specification and the changelog were updated.

## D-021 — Toggles are live mid-turn, the logo sweeps down, and the installer wears the mark

**Date:** 2026-11-18 · **Affects:** SPEC 9.6, 12, 13

The owner asked for tooling and skills to be toggleable in real time — the agent must see the
change while it is working — and for the startup logo to be faster and more evident, sweeping
from the top right to the low left. They also wanted that aesthetic in `install.sh`.

**Live toggles.** `Runtime` now owns a shared `Arc<RwLock<ToolRegistry>>` and a shared context
string. `save_toggles` writes the file and replaces both in place; `Agent` snapshots them at each
request and each tool call. An external tool turned off is removed from the registry too. Skills
changed with `/skills` rewrite the shared context, so the next request of the same turn sees the
new catalog. Pins stay fixed for the turn as a separate context block. A turn still reads one
consistent registry per call; only the permission mode applies at the instant of the call.

**Logo.** The streak runs backwards along the diagonal (top right to low left), takes 4.2 seconds
instead of six, is four diagonal cells wide, peaks at full highlight, and the small per-pixel
twinkle was raised to 0.20. `reduce_motion` still rests it.

**Installer.** `install.sh` prints the same 25 x 9 bitmap before it does any work. On a terminal
it animates five frames with the same sweep; when stdout is not a TTY, or `NO_COLOR`/`CI` is set,
it prints the resting frame in plain blocks. It uses `awk`, which POSIX sh already relies on, and
no build artefacts.

**Status:** accepted. The specification and the changelog were updated.

## D-022 — The logo has dark cubes and a darker resting mark

**Date:** 2026-11-18 · **Affects:** SPEC 12

The diagonal streak introduced in D-021 was still hard to see. The owner asked for a wider range
of dark cubes and a darker logo colour.

`logo::pixel_color` now returns a colour for every level of the 25 x 9 bitmap. Dark cells are a
shade between the terminal background and the dark cube colour, so the mark has a quiet body
instead of holes. Lit and dim resting colours were pulled towards the background (0.42 and 0.55
blend), while the streak still reaches full highlight; the streak is five diagonal cells wide and
the underlying pulse is 0.10. Contrast, not speed, carries the effect, so the 4.2-second traverse
of D-021 stays. `reduce_motion` still holds it still. The spec's old "only lit pixels are drawn"
rule is replaced; this is a deliberate visual change, not a new file-format contract.

**Status:** accepted. The specification and the changelog were updated.

### Amendment to D-022 — transparent background kept

The first version of D-022 drew the bitmap's empty cells as dark cubes, which produced a filled
rectangle around the mark. The owner clarified that the background must stay transparent and the
"darker cubes" belong **inside** the logo. `pixel_color` again returns `None` for dark cells; lit
and dim pixels get a stable per-pixel shade (`shade_of`) so the mark itself spans several dark
tones, while the sweep still reaches full highlight. The 4.2-second traverse and the twinkle are
unchanged, and `reduce_motion` still rests both. SPEC §12 reflects the corrected rule.

## D-023 — The system prompt lives in `~/seek/SYSTEM.md`

**Date:** 2026-11-18 · **Affects:** SPEC 2.4, 5, 9.2

The owner asked the model to "actually read the system prompt in the seek directory" and to be able
to edit the default. Until now the prompt was a compiled constant; only the project's `AGENTS.md`
was read from disk, and it was appended to the context rather than being the system prompt.

`install.sh` now copies `builtin/SYSTEM.md` to `~/seek/SYSTEM.md` with `copy_if_absent`, so an
existing file is never overwritten. `Runtime` reads it at startup through
`seek_core::prompt::system_prompt_from`; a missing or empty file falls back to the compiled
`SYSTEM_PROMPT`. `Agent::with_system_prompt` replaces the compiled base for that session. The user
name, the project `AGENTS.md` context and the per-turn pins are still appended after the system
prompt, as before. The default file begins "You are SEEK, a coding agent in the user's terminal.",
so the model knows its name.

This is additive: `SYSTEM.md` joins the file-format contracts with no schema bump, and the
token-budget test still measures the compiled default, not a user edit.

**Status:** accepted. The specification, the installer, the manual checklist and the changelog
were updated.

## D-024 — Past conversations return with `--continue`

**Date:** 2026-11-18 · **Affects:** SPEC 4.1, 11

The owner asked for "past chat history": a session resumed from the command line with its real
context and the model that was used. Sessions were already append-only JSONL with message and
model-switch events, and `/resume` could open one from the sidebar, but startup always began a new
conversation.

`seek --continue` now chooses the newest session whose header `cwd` matches the directory SEEK was
started in, calls `Runtime::resume_session` before the agent loop and the first frame, and loads
the model tag of the last message into both the runtime and the model chip. `SessionInfo` carries
`cwd` so the choice can be scoped to this project; `Runtime::new_session` writes `self.root` into
the header (the directory the tools run in) rather than the process cwd. A missing or unreadable
session is ignored: SEEK opens a fresh chat, as before. No file format changed; `cwd` was already
in the header.

**Status:** accepted. The specification, the manual checklist and the changelog were updated.

## D-025 — Absolute pins, a quieter composer and a one-command installer

**Date:** 2026-11-18 · **Affects:** SPEC 4.1, 9.5, 10.3, 11, 13

Three reports from a live session:

1. The model saw `@documents` in the user's text and guessed its location even though the pin
   context carried the absolute path. `pins::expand` now replaces a valid `@` mention with the
   absolute path in the text sent to the model. The interface keeps the short form.
2. The thinking animation interrupted the transcript and the bottom border showed a clipped
   "working..." status. The animation now has its own line above the prompt, and the border only
   carries the key hint.
3. "Past chat history" was still hard to reach. `/history` lists the sessions already loaded into
   the app and resumes the selected one, while `state/sessions/last_session.toml` makes
   `seek --continue` resume the last session even from a different directory; a scan by cwd and
   then by age is the fallback.
4. `install.sh` run standalone (for example through `curl | sh`) now clones the repository into a
   temporary directory and re-runs itself there, then removes the clone. `SEEK_INSTALL_REPO`
   overrides the URL for tests. This makes a one-command install possible.

**Status:** accepted. The specification, README, checklist and changelog were updated.

## D-026 — A read is never confirmed, and the web search is a read

**Date:** 2026-11-18 · **Affects:** SPEC 9.3, 9.4

The owner asked that `web_search` and `read` stop asking for confirmation: neither changes
anything, unlike a write or a command.

- `read` was already allowed inside the project, but the path rule of SPEC 9.4 made it ask for a
  path outside the project root in `ask` mode. It is now allowed wherever it goes; the same holds
  for the read-only helpers `ls`, `grep` and `find`. The path rule still asks for a write or a
  command outside the project root, whatever the tool is.
- `web_search` carried `Risk::Network` and was asked in `ask` mode (D-016). It now carries
  `Risk::Read`, like `read`: the tool only reads the web, so it runs in every mode and is no longer
  denied in `read` mode. `fetch` stays `Risk::Network` and still asks in `ask` mode and is denied
  in `read` mode.

The risk is declared by the tool schema (`websearch::web_search_schema`) and applied by
`Agent::risk_of`; the permission matrix itself is unchanged. The manual checklist no longer
expects a prompt for a search.

**Status:** accepted. The specification, README, checklist and changelog were updated.

## D-027 — The system prompt reaches local models; the filesystem overrides it

**Date:** 2026-11-19 · **Affects:** SPEC 4.1, 5, 9.2, 10.3

A live session showed two bugs. First, the model repeated a `git push` because it could not tell
that the earlier one had run: `seek-llm::openai` flattened tool results into loose user text and
dropped the call id. Second, `~/seek/SYSTEM.md` was read by the runtime but never sent to
OpenAI-compatible providers, because `OpenAiProvider::build_body` ignored `request.system` while
the Anthropic provider set `body.system`.

The OpenAI encoder now emits a leading `system` message, assistant `tool_calls` with ids and
`arguments`, and one `role: "tool"` message per result. A command with no output returns the marker
`(no output)` so small models do not re-run it. The transcript is shifted down one row while the
sidebar is open, so the first message lines up with "Sessions" inside the sidebar.

The system prompt has one source of truth: a project's `.seek/SYSTEM.md`, then `~/seek/SYSTEM.md`,
then the compiled default. `.seek/config.toml` overlays only the keys it sets on the user's
`config.toml`. No file format changed; the project directory is optional. `seek doctor` reports
which system prompt file is in use.

**Status:** accepted. The specification, changelog and manual checklist were updated.

## D-028 — Port the grok-build modal chrome into the menus

**Date:** 2026-11-19 · **Affects:** SPEC 10.3

The first "grok-style" menu was only inspired by the pager, and the owner asked for the real thing.
The Apache-2.0 source is public at `xai-org/grok-build`; a shallow checkout now lives in
`reference/grok-build` (gitignored). The model selector and settings menu were rewritten from the
pager's `views/picker.rs` and `views/modal_window.rs`: 90% width with generous vertical margins,
square border with a bold title and `[✗]`, ` search: ` search row, full-width dim divider, `◆`
picker rows with right-aligned labels and badges, a detail line below the selection, and a centered
shortcut footer. Nothing was copied verbatim; the code is written for SEEK's `Theme` and ratatui
0.30 types. The SEEK addition is the directional blue gradient on the selected row.

**Status:** accepted. The specification and changelog were updated.

## D-029 — The model picker is inline, the settings stay a modal

**Date:** 2026-11-19 · **Affects:** SPEC 10.3

The grok-code screenshot shows the model selector as a compact list directly above the prompt,
not as a full-screen modal. D-028 ported the modal chrome, which made SEEK's picker look unlike the
reference. The model selector is now inline: it clears only its own rows, draws a thin divider and
a `/model <model> [window] [effort]` line, then compact rows with the cursor glyph, the model name,
the `CURRENT`/`LOCAL` badge and a right-aligned id, with the directional gradient on the picked row.
The transcript stays visible around it. The settings menu keeps the grok-build modal chrome, which
is how the reference renders settings.

**Status:** accepted. The specification and changelog were updated.

## D-030 — One design language for the menus and the sidebar

**Date:** 2026-11-19 · **Affects:** SPEC 10.3

The inline model picker of D-029 was closer to the grok-code reference, but its `/model <model>
[window] [effort]` line and the grok-build chrome left in `/settings` looked foreign next to the
sidebar. The owner asked for one coherent language across the model picker, the settings panel and
the launch-options editor. All three now use the sidebar's vocabulary: a dark-cube border, a dim
title with spaces, a bold `accent` section header, `cube_light` primary text, `dim` secondary text
and the directional gradient on the selected row. The `/model` hint line is gone; while filtering,
the typed query appears dimmed after the `Models`/`Settings` header. The shared `panel_block` and
`section_header` helpers in `seek-tui::menu` keep the panels in step.

**Status:** accepted. The specification and changelog were updated.

## D-031 — Menus are sidebar panels in the main column

**Date:** 2026-11-20 · **Affects:** SPEC 10.3

A screenshot with the sidebar open showed the model list spanning the whole window and drawing its
rows beneath the sidebar, while the sidebar itself is a bordered `┌ SEEK ┐` panel. The owner asked
for the model selector and settings to look like the sidebar and to adapt when it is open. Both are
now bordered panels with a dim title on the border, grouped `accent` section headers and the same
row styling as the sidebar's sessions. Crucially, overlays are rendered into `main_area` rather than
the full screen, so they occupy the column beside the sidebar; the directional gradient keeps
following the sidebar's side. Settings were reordered so each group header appears once.

**Status:** accepted. The specification and changelog were updated.

## D-032 — A default permission level, and menus that match the sidebar

**Date:** 2026-11-20 · **Affects:** SPEC 10.2, 10.3

A screenshot showed the model panel filled with `theme.bg` while the sidebar is transparent, so
the two panels looked different despite sharing the border and row styles. Menu panels now paint no
background of their own: `Clear` erases the transcript behind them and the terminal shows through,
exactly like the sidebar. Titles and group headers are capitalized to match `SEEK` and `Sessions`.

`Config.default_mode` already existed but nothing read it: `App` and `Runtime` both started in
`ASK`. The `/settings` row called "Permission mode" only changed the live session. It is now called
"Default permission level" and writes `default_mode` to `config.toml`; both the app and the agent
read that value at startup. `/mode` (Shift+Tab) remains session-only.

Finally, the bright vertical borders of the prompt box sat beside the block cursor and read as a
second cursor. They now use the dim `border` colour, leaving one bright vertical mark.

**Status:** accepted. The specification and changelog were updated.

## D-033 — A blank row above the prompt box

**Date:** 2026-11-20 · **Affects:** SPEC 10.3

The last line of the transcript sat directly against the top border of the prompt box, which made
the answer look crowded. The vertical layout now reserves one unprinted row between the
transcript (and the log/pin/completion rows) and the box. The input's row budget and the overlay
area were adjusted by one row so the gap survives long drafts and menus. A UI test checks that the
row above the box is blank.

**Status:** accepted. The specification and changelog were updated.

## D-034 — An animated, dry-runnable installer for Linux and macOS

**Date:** 2026-11-20 · **Affects:** SPEC 13

The installer already sheened the logo on an interactive terminal, but the long steps were silent
and there was no way to preview the changes. The owner asked for animations on Linux and for the
script to work just as well on macOS, with a `--dry-run` to test it.

`install.sh` now detects `uname -s` and names the platform, animates the logo and runs a braille
spinner for the download and the source build when stdout is a TTY without `NO_COLOR`, `CI` or
`TERM=dumb`. Everything is POSIX `sh`: the help uses `awk` instead of a BSD-sensitive `sed`
interval, and `TMPDIR` is stripped of a trailing slash so macOS paths are clean. The release asset
uses `aarch64-apple-darwin` / `x86_64-apple-darwin` on Darwin. `--dry-run` reports the would-be
steps and the asset without creating folders, copying files, downloading or building. The download
and build helpers chain their commands with `&&`, because a command in a `||` list disables
`set -e` inside a shell function and a failed download would otherwise look successful.

CI runs `sh -n`, the help check and a dry run in the `build` job on both ubuntu and macOS.

**Status:** accepted. The specification and changelog were updated.

## D-035 — The installer is a frame-based TUI

**Date:** 2026-11-20 · **Affects:** SPEC 13

A dry run showed the logo sweeping for five frames and then a static log of steps. The owner asked
for the animation to continue, to finish on a complete logo, and for a slower, more interactive
feel. The animated path now redraws a whole frame at 0.12 s intervals: the logo sheen loops for the
entire run, five step rows move from pending to active to done, and the last frame paints every lit
pixel at full highlight. A background animator redraws while the step command runs in the foreground
with its output captured — a first attempt ran the command itself in the background and hung under
a PTY because the status file was never written.

POSIX `sh` has no local variables: the renderer's row counter collided with the step runner's index
and kept every row marked active, so the two functions now use distinct names. The refactor also
dropped the `--no-binary` guard; it is back. The plain path (pipe, `NO_COLOR`, `CI`, `TERM=dumb`)
still prints the same quiet log and the platform line.

**Status:** accepted. The specification and changelog were updated.

## D-036 — A static logo and an arrow-key menu

**Date:** 2026-11-20 · **Affects:** SPEC 13

The frame-based TUI of D-035 was the wrong direction: the owner asked to drop the animation and
make the installer interactive in a simpler way. `install.sh` now prints the complete 25 x 9 logo
once, at full highlight, and presents five choices — full install, folders and files, binary only,
dry run, cancel. ↑/↓ move the `❯` and Enter selects; `q` cancels. Key reading uses `stty` and `dd`
with escape sequences, which works in the POSIX `sh` of both Linux and macOS. A flag preselects its
row, and a pipe or CI skips the menu so automation keeps working. The `--no-binary`, `--dry-run`,
`--from-source`, idempotency and platform logic are unchanged.

**Status:** accepted. The specification and changelog were updated.

## D-037 — One logo bitmap

**Date:** 2026-11-20 · **Affects:** SPEC 12, 13

A screenshot of the installer showed the K of SEEK missing its last column: the copy of the 25 x 9
bitmap in `install.sh` was shifted one column right compared with the generated
`crates/seek-tui/src/logo_bitmap.rs`, so the rightmost lit cell fell outside the 25-cell row. The
installer's heredoc now uses the exact nine rows from the generated file, and CI diffs the two
before running the installer checks. The generated file stays the single source of truth.

**Status:** accepted. The specification and changelog were updated.

## D-038 — The piped install asks, and one URL everywhere

**Date:** 2026-11-20 · **Affects:** SPEC 13

The one-command install pipes `install.sh` into `sh`, so stdin is the script, not a keyboard. With
the menu reading stdin it was silently skipped and the user could not choose a dry run or a full
install. The menu now opens `/dev/tty` on fd 3 for key input, which exists whenever there is a
controlling terminal even when the script itself came from a pipe. CI and headless runs have no
`/dev/tty` and go straight to the steps.

The landing page pointed at the `develop` raw URL while `README.md` pointed at `main`. Both now use
`https://raw.githubusercontent.com/piskevalee-cpu/Seek/main/install.sh`, the release branch.

**Status:** accepted. The specification and changelog were updated.

## D-039 — Skills are discovered in every harness's folder

**Date:** 2026-11-20 · **Affects:** SPEC 9.5

Skills were only discovered in `~/seek/skills` and in the built-in folder. The owner asked for the
folders used by the other harnesses (`.claude`, `.codex`, `.hermes`, and the rest) so a `SKILL.md`
written for one agent works in SEEK without copying it.

`seek_core::skill::discover` now scans, in order: project `.seek/skills`, project `.agents/skills`
and the other harness folders, `~/seek/skills`, the same harness folders under the user's home,
then the built-ins. Project beats user, SEEK's own folder beats another harness, and installed
skills beat built-ins. First name wins: a duplicate keeps the higher-precedence body, so a skill
has one toggle and one `/<name>` target. The scan walks a few levels down to find category folders
(as Hermes and OpenCode use), skips `.git`, `node_modules`, `.hub`, `__pycache__`, `.venv` and
`target`, stops at a folder with a `SKILL.md`, and caps the catalog at 100 skills to bound the
prompt. Any other hidden folder with a `skills/` child is scanned too, so a harness added after
this list still works. Ancestors of the project root are not scanned. No contract file changes:
every location still uses the same `SKILL.md` format, and the resolved absolute path goes into the
catalog so the model can read the body.

**Status:** accepted. The specification, README, checklist and changelog were updated.

## D-040 — Drag to copy with OSC 52 and OMP notifications

**Date:** 2026-11-20 · **Affects:** SPEC 9.4, 10.3

The terminal has mouse capture on, so dragging never selected text for native copy. Following
grok-build, a left-button gesture over the transcript now selects visible lines, highlights them,
and on release reconstructs the text and copies it with OSC 52 (`ESC ] 52 ; c ; <base64> BEL`),
which works over SSH without xclip. A toast appears in the TUI and a terminal notification follows
the protocol of oh-my-pi: OSC 777, OSC 99 on Kitty, a Windows toast in Windows Terminal. The
existing clipboard keys are unchanged; this is additive.

## D-041 — One panel design, a global collapse key, and the log gap

**Date:** 2026-11-20 · **Affects:** SPEC 10.3

`Ctrl+O` used to expand the selected tool row. It now collapses or shows **all** reasoning and
command output at once, with per-row expand moved to `Ctrl+E`. All overlays share the sidebar-style
panel of the model and settings menus through a new `seek_tui::panel` module. Finally, the one blank
row moved from below the yellow server log to above it: the log keeps its place just over the
prompt, and the output above it gets the spacing.

## D-042 — `seek update`

**Date:** 2026-11-20 · **Affects:** SPEC 13

A new subcommand reads the version from the manifest of `main`, compares it with the running
version and, unless `--check` is given, downloads the matching GitHub Release, unpacks it with
`tar` and atomically renames it over the running binary. It sends an OMP notification when done.
The release target is derived the same way as in `install.sh`; version comparison and target naming
are pure functions with tests.

**Status:** accepted. The specification, README, checklist and changelog were updated.

## D-043 — Parallel first in the search chain

**Date:** 2026-11-20 · **Affects:** SPEC 9.3

The HTML scrapers that used to start the chain now answer with bot challenges. The reference harness
starts with Parallel's keyless public MCP (`https://search.parallel.ai/mcp`), which returns
LLM-friendly results without a browser or a key. A new `websearch::providers::parallel` speaks
JSON-RPC to that endpoint and leads the default chain; the scrapers remain as fallbacks. Verified
with a live query.

## D-044 — Act, do not ruminate

**Date:** 2026-11-20 · **Affects:** SPEC 9.2

A build task could end as a long essay instead of a first file. An appended `<act>` section in the
system prompt — completion is the only reason to stop, issue the tool call instead of describing it,
keep plans executable, create the first file this turn, stop once verified, never narrate effort —
closes the gap the reference harness solves with `<critical>` blocks and a thinking watchdog. It adds
about 150 tokens; the budget test reports 1050/1500. The prefix stays append-only. The installed
`~/seek/SYSTEM.md` was updated too, preserving the user's own lines.

## D-045 — Configurable sidebar, hardware block and running metrics

**Date:** 2026-11-20 · **Affects:** SPEC 10.3

The sidebar is three stacked blocks. `/settings` assigns sessions, hardware, processes or widgets to
each slot and the choice persists in `state/user.toml`. Hardware is an htop-like view: every core gets
a bar with a gradual gradient fill, plus memory, temperature and GPU. The context bar, thinking
animation and speed moved out of the blocks to a line under the prompt while the sidebar is open.
Ctrl+↑/Ctrl+↓ drive the sidebar selection. The open/closed state is also persisted at toggle time, on
quit and on a signal, so it is restored even after Ctrl+C.

## D-046 — Session management in the shape of grok

**Date:** 2026-11-20 · **Affects:** SPEC 11

The session picker now groups by repository with the current project first, shows the message count,
model and age, and supports `d` → `y` to delete a session after confirmation. The append-only JSONL
contract is unchanged: only the list view and the file removal were added.

## D-047 — Footer metrics, "thought", and the widget skill

**Date:** 2026-11-20 · **Affects:** SPEC 9.5, 10.2, 10.3

The run metrics were on a line above the footer while the sidebar was open. They now sit in the
footer itself, immediately to the right of the permission chip and the model name; the vitals remain
in the sidebar hardware block when it is open so no fact has two homes. The time label is "thought"
rather than "think", in the speed line and the reasoning chip. The sidebar block is named `widget`
in the singular; settings files written with the plural are normalized when read. A built-in skill
named `widget` teaches the widget contract and is copied to `~/seek/skills/widget/` on install.

## D-048 — Skills are staged, then the message continues

**Date:** 2026-11-21 · **Affects:** SPEC 9.5, 10.3

A skill used to run as soon as its completion was taken with Enter, so a prompt could not be written
afterwards. The completion now marks skills with a distinct kind: Enter leaves `/skill ` in the bar
and shows a ` skill ` chip above it (a view of the draft token, like the pins), the user continues
writing, and the next Enter sends the body with the prompt. A bare skill name stages it too; a
disabled skill is not staged and still submits as plain text. The built-in skill that teaches widget
authoring was renamed `create-widget` to match its action.

**Status:** accepted. The specification, README, checklist and changelog were updated.
