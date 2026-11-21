---
name: create-widget
description: Write a SEEK home widget — a small folder that prints a short status line into the Widgets sidebar block. Use when the user asks for a widget, a status line, or to show project data in SEEK's sidebar.
---

# Create a widget

A widget is a folder `~/seek/widgets/<name>/` with two files:

- `widget.toml` — `name`, `title` and an optional `timeout_ms` (default 500).
- `run` — an executable that prints plain text. No arguments, no JSON, no colors.

SEEK runs widgets when the Home screen is shown, in the user's current working directory, and
shows their output in the **widget** block of the sidebar. A widget that fails or times out shows
its title and a dash.

## Contract

```
widget.toml:
  name = "todos"          # same as the folder name
  title = "open todos"    # short, shown above the lines
  timeout_ms = 500        # optional; keep the default unless there is a reason

run:
  #!/bin/sh
  ...
```

The output is capped: at most **8 lines**, each at most **40 columns**, truncated on a character
boundary. Print less, not more. The widget is read-only: never write files and never start a server.

## How to write one

1. Pick a name that says what it reports. Create `~/seek/widgets/<name>/`.
2. Write `widget.toml` with the name, a title and no timeout unless the command is genuinely slow.
3. Write `run` with a shebang (`#!/bin/sh` is fine) and make it executable with `chmod +x`.
4. Use only tools that are present: check with `command -v` and print one short line if one is
   missing, rather than failing noisily.
5. Read from the working directory, not from a hard-coded path. `git` and `rg` are common sources.
6. Test it by hand: `cd <project> && ~/seek/widgets/<name>/run`. Count the lines and the columns.

## Example

```
# ~/seek/widgets/todos/widget.toml
name = "todos"
title = "open todos"

# ~/seek/widgets/todos/run
#!/bin/sh
if ! command -v rg >/dev/null 2>&1; then
    echo "rg is not installed"
    exit 0
fi
rg -n "TODO|FIXME" --glob '!target' --glob '!node_modules' . 2>/dev/null |
    head -8 | cut -c1-40
```

## What not to do

- No ANSI colors, no box drawing, no blank first line: the sidebar draws the chrome.
- No network calls, no long builds, no prompts for input: the 500 ms budget is a hard wall.
- No absolute paths to a particular checkout: the widget runs in whatever directory SEEK is open.
- Never overwrite an existing widget of the user. Create a new folder or ask first.

The built-in `git` and `usage` widgets are readable examples in the repository's `builtin/widgets/`.
