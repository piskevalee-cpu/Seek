# The manual test checklist

How to go through SEEK by hand and check that everything works. Every item says what to do and
what to expect.

An agent that changes the code should walk this list. It is the fastest way to find out that
something broke which only shows up when a person uses it: the layout, the keyboard, the colours
and the flow of a conversation are not covered by the unit tests.

## Before you start

- [ ] **Build it.** `cargo build --release` — no errors, no warnings.
- [ ] **Check it.** `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` — both clean.
- [ ] **Run the tests.** `cargo test --workspace` — every test passes.
- [ ] **Look at the screens without a terminal.** `cargo run -p seek-tui --example screenshot` — the home screen, the chat screen, the sidebar and the picker are printed.
- [ ] **Put a model where SEEK can find it.** `ln -s ~/LLMs/<your-model>.gguf ~/seek/models/` — `seek --doctor` lists it under "local models".
- [ ] **Check the doctor.** `seek --doctor` — it names the home folder, the providers, the local models, `git` and the `llama` binary.
- [ ] **Have a cloud key ready, if you want to test the cloud.** `export ANTHROPIC_API_KEY=...` — without a key SEEK falls back to the scripted provider, which is useful to know but must not be mistaken for the real thing.

Run the TUI in a terminal of **at least 80x24**. Everything below is done in that terminal.

## The first run

- [ ] **Start it.** `seek` — the logo appears in the middle of the screen, the widgets below it, the prompt bar at the bottom, the footer at the very bottom.
- [ ] **The logo.** It is the mark: the plus symbol on the left, `SEEK` on the right. It is drawn with blocks, two columns per pixel.
- [ ] **The logo has no frame.** Only the lit pixels are drawn: the background of the mark is the background of the terminal, not a block of colour.
- [ ] **The logo is centred.** Horizontally and vertically in the home screen.
- [ ] **The sheen.** A shine sweeps across the logo from the bottom left to the top right. With `SEEK_COLOR`/reduced motion it rests.
- [ ] **The widgets.** The `git` widget shows the branch and the recent commits; the `usage` widget shows the tokens used so far. A widget that fails shows its title and a dash.
- [ ] **The model picker.** With a local model installed, the picker opens on its own. The local model is listed with a `LOCAL` tag.
- [ ] **Choose a model.** Press Enter — the picker closes, the footer shows the model.
- [ ] **The footer.** It shows the mode chip (`ASK`), the model, the working directory and the branch, right-aligned.

## The prompt bar

- [ ] **Type.** Every character appears, and the cursor is a block behind the last one.
- [ ] **Send.** Press Enter — the message appears in the transcript as a `USER` box.
- [ ] **A newline.** Shift+Enter (or Alt+Enter) inserts a line instead of sending.
- [ ] **The history.** Press Up — the last message you sent comes back. Press Down — it goes away again.
- [ ] **The suggestions.** Type `/` — a list of the slash commands appears above the bar. Type `/mod` — the list narrows to `/model` and `/mode`.
- [ ] **A backspace.** It deletes one character, not one byte: a character with an accent is gone in one press.

## The conversation

- [ ] **Ask something.** "Say hello in three words." — the answer streams in, word by word, in a `SEEK` box.
- [ ] **The reasoning.** A model that thinks shows its reasoning above the answer, dimmed, behind a vertical bar. Press Alt+T — the reasoning collapses to one row that says "reasoning". Press Alt+T again — it comes back.
- [ ] **Your message is a band.** The prompt of the user is a band across the width with the arrow in front of it. The answer of the model is plain text, without a band.
- [ ] **The metrics.** While the model answers, the line under the prompt bar shows the context (`1.6k / 32.8k`), the percentage, the speed in tokens per second, the think time and the vitals (cpu, ram, gpu, vram).
- [ ] **The animation.** While the model works, a line of dark cells shows at the tail of the transcript, with a wave of brightness going through them. It is one line, twelve cells, and it moves.
- [ ] **A long answer.** Ask for a long text — the transcript scrolls, and the view stays at the bottom.
- [ ] **Scroll up.** Press PageUp — the transcript scrolls up, and the newest lines are no longer glued to the bottom. PageDown scrolls back.
- [ ] **The log strip.** With a local model, one line of the server appears above the prompt bar: "loading model", "server is listening on ...". The strip hides itself after ten seconds of silence and comes back with the next event.

## The keys

Every key of the specification, one by one.

- [ ] **Enter** — sends the message.
- [ ] **Shift+Enter** — inserts a newline.
- [ ] **Esc** — closes an overlay. With a turn running, it stops the turn.
- [ ] **Shift+Tab** — cycles the mode: `READ`, `ASK`, `YOLO`. The chip in the footer changes colour with it.
- [ ] **Ctrl+B** — opens and closes the sidebar.
- [ ] **Ctrl+O** — expands the selected tool row, and collapses it again.
- [ ] **Ctrl+C** — clears the input. Press it twice on an empty input — SEEK quits.
- [ ] **Ctrl+T** — cycles the reasoning level of the model: `off`, `low`, `medium`, `high`, `max`. The chip next to the model name shows it. A model without levels says that reasoning is not configurable for it.
- [ ] **Alt+T** — shows and hides the reasoning.
- [ ] **PageUp / PageDown** — scroll the transcript.
- [ ] **Tab** — with the sidebar open, moves the focus into it. Press Tab again to get back.

## The permissions

- [ ] **Ask for a write.** "Create a file called hello.txt with the word hi in it." — the prompt bar turns into a question with three choices: **Yes**, **No**, **Tell SEEK what to do**. The chosen answer is highlighted, and the line under it says which keys do what.
- [ ] **Yes.** The file is written and the tool row shows `ok` with the time it took.
- [ ] **No.** The tool row shows the refusal, and the model is told that the user said no.
- [ ] **Tell SEEK what to do.** Choose the third choice, type "use hello.md instead", press Enter — the model gets that text as the result of the tool call.
- [ ] **Read mode.** Press Shift+Tab until the chip says `READ` — a write is denied without asking.
- [ ] **Yolo mode.** Press Shift+Tab until the chip says `YOLO` — the write goes through without asking.
- [ ] **A path outside the project.** Ask it to write to `/tmp/whatever.txt` — in `ASK` mode it asks, whatever the tool is.

## The tools

- [ ] **The four tools.** `/tools` lists `read`, `write`, `edit` and `bash` as checked, and `ls`, `grep`, `find` and `fetch` as unchecked.
- [ ] **Turn one off.** Open `/tools`, move to `bash`, press Space — the check disappears. Close the picker: the next turn has no `bash` tool.
- [ ] **Turn one on.** Do the same with `grep` — now it is on.
- [ ] **Read a file.** "Read the first ten lines of Cargo.toml." — the tool row shows the path, the result is the content.
- [ ] **Edit a file.** "Change the description in Cargo.toml to something else." — the file changes and the row says `ok`.
- [ ] **A long output.** "Read the whole of Cargo.lock." — the output is cut with a marker that says how much was left out and how to fetch it.
- [ ] **Bash.** "Run `cargo --version`." — the row shows the command and the output.
- [ ] **The processes.** With a command running, Ctrl+B shows it in the sidebar under "Processes", with its pid and how long it has been running. Press `x` on it to kill it.

## The models

- [ ] **The picker.** `/model` lists the models of the configured providers, grouped by provider, and the local ones with a `LOCAL` tag.
- [ ] **Filter.** Type `sonnet` — the list narrows.
- [ ] **Switch.** Choose a cloud model — the footer follows, and the context bar is recalculated for the new window.
- [ ] **The launch options.** With a local model selected in the picker, press the right arrow — the launch options open. The first line is read-only and shows the command SEEK runs; the second line is yours.
- [ ] **Save the options.** Type `--ctx-size 8192`, press Enter — the options are saved next to the model and used the next time the server starts.
- [ ] **The reasoning level.** `/think` turns the reasoning off and on again; `/think high` sets it to high. The level belongs to the model: switch to another model and back, and it is still there.
- [ ] **A model without levels.** A model whose catalog entry declares no variants shows no reasoning chip, and Ctrl+T says that reasoning is not configurable for it.

## The sessions

- [ ] **The sidebar.** Ctrl+B — the sessions are at the top, with their title and how old they are.
- [ ] **A new session.** `/new` — the transcript is empty again.
- [ ] **Resume.** `/resume` — the sidebar opens on the sessions. Move with Up and Down, press Enter — the conversation comes back.
- [ ] **The focus.** With the sidebar open, Tab moves the focus into it; the selected session is highlighted. Tab again moves it back.
- [ ] **The file.** `~/seek/state/sessions/` holds one JSONL file per session: a header, then a line per message and per model switch.
- [ ] **The model switch is in the file.** Switch models in the middle of a conversation: the file says which model said what.

## The context

- [ ] **The bar.** Ten cubes that fill up as the context grows, with the used and the total beside it. It turns amber at 80% and red at 95%.
- [ ] **The estimate.** Before the first answer the number has a `~` in front of it: it is an estimate. After an answer it is exact.
- [ ] **Compact.** Fill the context a bit and press `/compact` — the older turns become one summary message.
- [ ] **Truncate.** `/truncate` drops the oldest turns until the history fits.
- [ ] **An overflow.** Switch to a model with a much smaller window while the context is large — the switch is refused and the app says to compact or truncate first.

## The skills

- [ ] **The list.** `/skills` lists the installed skills with a check next to each.
- [ ] **The built-in one.** `init` is there even when it is not installed in `~/seek/skills`.
- [ ] **Run a skill.** Type `/init` and press Enter — the instructions of the skill fill the prompt, and the model follows them.
- [ ] **Turn one off.** `/skills`, Space on a skill, close the picker — it is gone from the prompt.
- [ ] **The catalog.** The prompt that reaches the model carries the name, the description and the path of every enabled skill, and nothing else.

## The providers

- [ ] **The list.** `/provider` lists the configured providers, with their kind and their base URL.
- [ ] **Add one.** Choose "Add a provider": the form opens with the fields for the name, the kind, the base URL, the key environment variable, the models, the context and the rate limits.
- [ ] **Fill it in.** Tab moves from field to field; type the values. The kind has to be `anthropic-messages` or `openai-chat`.
- [ ] **Save it.** Enter — the provider is written to `~/seek/providers.toml` and shows up in the list.
- [ ] **An incomplete form.** Leave the name empty and press Enter — nothing is written and the app says what is missing.
- [ ] **The detected providers.** With no `providers.toml` and `ANTHROPIC_API_KEY` in the environment, the provider is there without any setup.
- [ ] **The keys.** A key can also live in `~/seek/state/auth.json`, under the name of the environment variable it stands in for. The file is readable by its owner only: `ls -l ~/seek/state/auth.json`.
- [ ] **The discovered models.** With an openai-chat provider that answers, the models it knows are in the picker, marked "found on the endpoint". The entries of `providers.toml` win over them.

## The widgets and the overlays

- [ ] **The widgets disappear.** Send a message — the widgets are gone from the home screen.
- [ ] **The help.** `/help` lists every key. Esc closes it.
- [ ] **The small terminal.** Make the terminal smaller than 80x24 — the app says "terminal too small" instead of drawing something broken.
- [ ] **The narrow terminal.** Make it 90 columns — the sidebar hides itself.
- [ ] **The colours.** `SEEK_COLOR=256 seek` — the app still looks right with 256 colours.

## Leaving

- [ ] **Quit.** `/quit` — the app exits and the terminal is the way it was.
- [ ] **The server is stopped.** After quitting, `pgrep -af "llama serve"` finds nothing: SEEK stopped the server it started.
- [ ] **Ctrl+C twice** — the same.

## Looking at the interface as a picture

- [ ] **Dump a screen.** `cargo run -p seek-tui --example dump_screen -- home > /tmp/home.json` and `python3 tools/screen_to_png.py /tmp/home.json /tmp/home.png` — the home screen as a picture. Do the same with `chat` and `permission`.
- [ ] **The picture matches the terminal.** The picture is a layout picture: every character fills its cell. Use it to check the alignment, the bands and the colors, not the font.

## When something is wrong

- [ ] **The log.** `~/seek/state/logs/` holds the log of the local server.
- [ ] **The doctor.** `seek --doctor` says what SEEK found and what is missing.
- [ ] **The session.** `~/seek/state/sessions/*.jsonl` shows exactly what was said.
