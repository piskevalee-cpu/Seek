# Decisions

Records changes to, and clarifications of, the specification. The specification (`docs/SPEC.md`)
marks unconfirmed proposals with `[DECISION]`; this file is where they are confirmed, changed or
rejected, as required by SPEC 0.

Format: one entry per decision, newest last.

## D-001 — llama.cpp server binary is `llama serve`

**Date:** 2026-11-11 · **Affects:** SPEC 5, SPEC 7.2, SPEC 13

SPEC 7.2 says to run `llama-server`, and SPEC 5 puts a `llama-server path` in `config.toml`.
The shipped llama.cpp since 0.9.0 is a single `llama` binary whose server is the `serve` subcommand
(`llama serve`, `llama cli`, `llama download`, `llama update`). The legacy `llama-server` binary
still exists in some distributions and accepts the same flags.

**Status:** open — waiting on the owner.

**Options:**
1. Keep the config key `llama_server_path` and treat it as a path to a server binary, with a
   fallback that detects the unified `llama` CLI and appends the `serve` subcommand.
2. Rename the key to `llama_path` and add a `llama_subcommand` key (default `serve`).

Option 2 is cleaner but changes a contract (SPEC 2.4), so it needs a migration and a changelog
entry. Until the owner decides, the code resolves the server command by probing, in order:
`llama_server_path` as given, then `llama serve` on `PATH`, then `llama-server` on `PATH`.

## D-002 — Theme colors sampled from the logo

**Date:** 2026-11-11 · **Affects:** SPEC 12, SPEC 10.6

SPEC 12 estimates a green background (`#22B14C`) with light gray (`#B5B5B5`) foreground and asks
for the exact values to be sampled from the PNG with a script.

`assets/logo.png` is the green logo, 500x199. `tools/extract_logo.py` samples it:

| Token | Value | Source |
| --- | --- | --- |
| `cube_light` | `#B4B4B4` | sampled from the strokes of the logo |
| `accent` | `#22B14C` | sampled from the background of the logo |
| `bg` | `#141617` | the background of the terminal, **not** of the logo |
| `cube_dark` | `#2C2E2F` | an unlit cube |
| `dim` | `#7C7D7E` | the dimmed text |

**Status:** accepted. The `bg` token is deliberately dark: a green terminal is unreadable, and the
logo keeps its green in the asset.

**The extraction.** The script finds the cell size from the pitch of the letters (the letters of
the pixel font are three cells wide and one cell apart, so the pitch of two letters is four
cells) and takes the bounding box of the ink as the grid. That works for the pixel-perfect source
and for a scaled, anti-aliased rendering of the same logo, which is what the asset turned out to
be: 18.75px cells against 35px cells for the same 25x9 grid.

**Open:** the transparent-background variant of the logo is still missing (SPEC 16, question 8).
The terminal never scales the PNG: the grid is extracted once and committed as a constant.

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
