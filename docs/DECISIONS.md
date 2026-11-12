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
